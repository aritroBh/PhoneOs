import Foundation
import JarvisCore
import Synchronization
@testable import JarvisKit

// Fakes of the four voice ports. Each records what the service asked of it and lets a test
// play what the hardware would report (a transcript, a finished utterance, an interruption).

final class FakeRecognizer: SpeechRecognizing {
    private let available: Bool
    private let stream: AsyncStream<SpeechRecognitionEvent>
    private let sink: AsyncStream<SpeechRecognitionEvent>.Continuation
    /// Ids started, finished and stopped, in order.
    let started = Mutex<[UInt64]>([])
    let finished = Mutex<[UInt64]>([])
    let stopped = Mutex<[UInt64]>([])

    init(available: Bool = true) {
        self.available = available
        (stream, sink) = AsyncStream.makeStream(of: SpeechRecognitionEvent.self)
    }

    func isAvailable() async -> Bool { available }
    func events() -> AsyncStream<SpeechRecognitionEvent> { stream }
    func start(id: UInt64) async { started.withLock { $0.append(id) } }
    func finish(id: UInt64) async { finished.withLock { $0.append(id) } }
    func stop(id: UInt64) async { stopped.withLock { $0.append(id) } }

    /// The latest recognition, while it runs (started and neither stopped nor ended).
    var running: UInt64? {
        guard let last = started.withLock({ $0.last }) else { return nil }
        return stopped.withLock { $0.contains(last) } ? nil : last
    }

    func partial(_ text: String, id: UInt64? = nil) {
        sink.yield(.partial(id: id ?? started.withLock { $0.last ?? 0 }, text: text))
    }

    func final(_ text: String, id: UInt64? = nil) {
        let id = id ?? started.withLock { $0.last ?? 0 }
        // The recognition ended by itself.
        stopped.withLock { $0.append(id) }
        sink.yield(.final(id: id, text: text))
    }

    func fail(_ reason: String) {
        let id = started.withLock { $0.last ?? 0 }
        stopped.withLock { $0.append(id) }
        sink.yield(.failed(id: id, reason: reason))
    }
}

final class FakeWakeWord: WakeWordDetecting {
    private let stream: AsyncStream<WakeWordEvent>
    private let sink: AsyncStream<WakeWordEvent>.Continuation
    let arms = Mutex<[UInt64]>([])
    let disarms = Mutex<[UInt64]>([])

    init() {
        (stream, sink) = AsyncStream.makeStream(of: WakeWordEvent.self)
    }

    func events() -> AsyncStream<WakeWordEvent> { stream }
    func arm(id: UInt64) async { arms.withLock { $0.append(id) } }
    func disarm(id: UInt64) async { disarms.withLock { $0.append(id) } }

    /// The current arming, while armed.
    var armed: UInt64? {
        guard let last = arms.withLock({ $0.last }) else { return nil }
        return disarms.withLock { $0.contains(last) } ? nil : last
    }

    /// The room says `text` (one utterance) while armed.
    func hear(_ text: String) {
        sink.yield(.final(id: arms.withLock { $0.last ?? 0 }, text: text))
    }
}

final class FakeSynthesizer: SpeechSynthesizing {
    private let stream: AsyncStream<UInt64>
    private let sink: AsyncStream<UInt64>.Continuation
    /// Utterances end as soon as they are queued.
    let autoFinish: Bool
    let said = Mutex<[(id: UInt64, text: String)]>([])
    let open = Mutex<[UInt64]>([])
    let cues = Mutex<[VoiceCueSound]>([])
    let stops = Mutex(0)

    init(autoFinish: Bool = true) {
        self.autoFinish = autoFinish
        (stream, sink) = AsyncStream.makeStream(of: UInt64.self)
    }

    func events() -> AsyncStream<UInt64> { stream }

    func speak(_ text: String, id: UInt64) async {
        said.withLock { $0.append((id, text)) }
        if autoFinish {
            sink.yield(id)
        } else {
            open.withLock { $0.append(id) }
        }
    }

    func stopSpeaking() async {
        stops.withLock { $0 += 1 }
        finishAll()
    }

    func playCue(_ cue: VoiceCueSound) async { cues.withLock { $0.append(cue) } }

    /// Every open utterance ends.
    func finishAll() {
        for id in open.withLock({ let ids = $0; $0 = []; return ids }) { sink.yield(id) }
    }

    var texts: [String] { said.withLock { $0.map(\.text) } }
}

final class FakeAudioSession: AudioSessionControlling {
    private let stream: AsyncStream<AudioSessionEvent>
    private let sink: AsyncStream<AudioSessionEvent>.Continuation
    let activations = Mutex(0)
    let deactivations = Mutex(0)
    let failActivation = Mutex(false)

    init() {
        (stream, sink) = AsyncStream.makeStream(of: AudioSessionEvent.self)
    }

    func events() -> AsyncStream<AudioSessionEvent> { stream }

    func activate() async throws {
        if failActivation.withLock({ $0 }) { throw InjectedError(message: "the audio session is busy") }
        activations.withLock { $0 += 1 }
    }

    func deactivate() async { deactivations.withLock { $0 += 1 } }

    func interrupt() { sink.yield(.interruptionBegan) }
    func endInterruption(shouldResume: Bool) { sink.yield(.interruptionEnded(shouldResume: shouldResume)) }
    func routeChange() { sink.yield(.routeChanged) }
}

/// A voice service over fakes, on an `AppHarness` (the demo backend and the real core).
@MainActor
final class VoiceHarness {
    let app: AppHarness
    let recognizer: FakeRecognizer
    let wake = FakeWakeWord()
    let synth: FakeSynthesizer
    let audio = FakeAudioSession()
    let clock = ManualClock()
    let service: VoiceService
    /// Every task spec the driver started, in order.
    var specs: [TaskSpec] = []

    init(_ name: String, rules: ScriptedOwner.Rules = ScriptedOwner.Rules(), settings: VoiceSettings,
         autoFinishSpeech: Bool = true, available: Bool = true) {
        app = AppHarness(name, rules: rules)
        recognizer = FakeRecognizer(available: available)
        synth = FakeSynthesizer(autoFinish: autoFinishSpeech)
        service = VoiceService(
            ports: VoicePorts(recognizer: recognizer, synthesizer: synth, wakeWord: wake, audio: audio),
            center: app.center, settings: settings, clock: clock, tickInterval: nil
        )
        app.center.observe { [weak self] event in
            if case .session(_, .update(.started(let spec))) = event { self?.specs.append(spec) }
        }
        service.onChange = { [weak self] in
            guard let self, let t = self.service.transcript, self.transcripts.last != t else { return }
            self.transcripts.append(t)
        }
    }

    /// Every transcript row shown, in order.
    var transcripts: [VoiceService.Transcript] = []

    /// Moves the clock past the speaking tail (and any other `ms`) and ticks.
    func pass(_ ms: UInt64 = 1_000) {
        clock.advance(by: .milliseconds(Int(ms)))
        service.tick()
    }

    func cleanUp() {
        service.stop()
        app.cleanUp()
    }
}

func wakeOn(_ speakAnswers: VoiceSpeakAnswers = .never) -> VoiceSettings {
    VoiceSettings(wakeWord: true, speakAnswers: speakAnswers, confirmPushToTalk: true)
}
