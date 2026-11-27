import Foundation
import Synchronization

/// Voice ports that play a script instead of using the microphone (the app's UI tests, with
/// `-jarvis-voice-scripted`; and the JarvisKit tests that check that hook). Like the
/// scripted model, they decide nothing: the core still reads every transcript.
///
/// - The wake word, each time it is armed, hears the next of `wakeLines` (once each) as one
///   final utterance after `delay`.
/// - The recogniser, each time it starts, hears the next of `recognizerLines` as a final
///   utterance after `delay`; when they run out it repeats the last one (so a scripted
///   "yes" keeps arriving while a question is on screen).
/// - Speech ends at once; no cue is played; the audio session does nothing.
public enum ScriptedVoice {
    /// The UI test's script: "Hey Jarvis, open Settings", then "yes" during the task.
    public static let uiTestWakeLines = ["Hey Jarvis, open Settings"]
    public static let uiTestRecognizerLines = ["yes"]

    public static func ports(wakeLines: [String], recognizerLines: [String],
                             delay: Duration = .milliseconds(500)) -> VoicePorts {
        VoicePorts(
            recognizer: ScriptedRecognizer(lines: recognizerLines, delay: delay),
            synthesizer: SilentSynthesizer(),
            wakeWord: ScriptedWakeWord(lines: wakeLines, delay: delay),
            audio: SilentAudioSession()
        )
    }
}

/// Plays `lines` in order to one listener at a time.
private final class LinePlayer: Sendable {
    private let lines: [String]
    private let repeatLast: Bool
    private let next = Mutex(0)

    init(lines: [String], repeatLast: Bool) {
        self.lines = lines
        self.repeatLast = repeatLast
    }

    /// The next line, `nil` when none is left.
    func take() -> String? {
        next.withLock { index in
            if index < lines.count {
                defer { index += 1 }
                return lines[index]
            }
            return repeatLast ? lines.last : nil
        }
    }
}

final class ScriptedRecognizer: SpeechRecognizing {
    private let player: LinePlayer
    private let delay: Duration
    private let stream: AsyncStream<SpeechRecognitionEvent>
    private let sink: AsyncStream<SpeechRecognitionEvent>.Continuation
    /// The recognition that is running (`nil` once stopped or ended).
    private let current = Mutex<UInt64?>(nil)

    init(lines: [String], delay: Duration) {
        player = LinePlayer(lines: lines, repeatLast: true)
        self.delay = delay
        (stream, sink) = AsyncStream.makeStream(of: SpeechRecognitionEvent.self)
    }

    func isAvailable() async -> Bool { true }
    func events() -> AsyncStream<SpeechRecognitionEvent> { stream }

    func start(id: UInt64) async {
        current.withLock { $0 = id }
        guard let line = player.take() else { return }

        Task { [self] in
            try? await Task.sleep(for: delay)
            let live = current.withLock { value -> Bool in
                guard value == id else { return false }
                value = nil
                return true
            }
            if live { sink.yield(.final(id: id, text: line)) }
        }
    }

    func finish(id: UInt64) async {}

    func stop(id: UInt64) async {
        current.withLock { if $0 == id { $0 = nil } }
    }
}

final class ScriptedWakeWord: WakeWordDetecting {
    private let player: LinePlayer
    private let delay: Duration
    private let stream: AsyncStream<WakeWordEvent>
    private let sink: AsyncStream<WakeWordEvent>.Continuation
    private let current = Mutex<UInt64?>(nil)

    init(lines: [String], delay: Duration) {
        player = LinePlayer(lines: lines, repeatLast: false)
        self.delay = delay
        (stream, sink) = AsyncStream.makeStream(of: WakeWordEvent.self)
    }

    func events() -> AsyncStream<WakeWordEvent> { stream }

    func arm(id: UInt64) async {
        current.withLock { $0 = id }
        guard let line = player.take() else { return }

        Task { [self] in
            try? await Task.sleep(for: delay)
            if current.withLock({ $0 == id }) { sink.yield(.final(id: id, text: line)) }
        }
    }

    func disarm(id: UInt64) async {
        current.withLock { if $0 == id { $0 = nil } }
    }
}

final class SilentSynthesizer: SpeechSynthesizing {
    private let stream: AsyncStream<UInt64>
    private let sink: AsyncStream<UInt64>.Continuation

    init() {
        (stream, sink) = AsyncStream.makeStream(of: UInt64.self)
    }

    func events() -> AsyncStream<UInt64> { stream }
    func speak(_ text: String, id: UInt64) async { sink.yield(id) }
    func stopSpeaking() async {}
    func playCue(_ cue: VoiceCueSound) async {}
}

final class SilentAudioSession: AudioSessionControlling {
    private let stream: AsyncStream<AudioSessionEvent>

    init() {
        stream = AsyncStream { _ in }
    }

    func events() -> AsyncStream<AudioSessionEvent> { stream }
    func activate() async throws {}
    func deactivate() async {}
}
