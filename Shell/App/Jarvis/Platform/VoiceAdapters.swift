import AudioToolbox
import AVFoundation
import Foundation
import Speech
import JarvisKit

// The app's voice adapters: the IO behind `VoicePorts`. They decide nothing (the core's
// `JarvisVoice` reads every transcript and composes every line); they listen, speak and
// report. Mac-only: not compiled on Linux.
//
// - Recognition is on-device only (`requiresOnDeviceRecognition = true`). Where the device
//   or the language cannot recognise on-device, voice is unavailable: there is no fallback
//   to server recognition, ever.
// - The wake word (first implementation) is the same on-device recogniser listening
//   continuously while Jarvis is in front and the setting is on. No background listening:
//   the app declares no `UIBackgroundModes audio`.
// - The audio session is `.playAndRecord` with mode `.default` (see `SystemAudioSession`).

enum VoiceAdapters {
    /// The ports over the real microphone, synthesiser and audio session.
    static func ports() -> VoicePorts {
        let microphone = Microphone()
        return VoicePorts(
            recognizer: OnDeviceSpeechRecognizer(microphone: microphone),
            synthesizer: SystemSpeechSynthesizer(),
            wakeWord: ForegroundWakeWord(microphone: microphone),
            audio: SystemAudioSession()
        )
    }

    /// The recogniser for the wake phrases and the stop words (English).
    static let locale = Locale(identifier: "en-US")

    /// Asks for speech recognition and the microphone (once each; the system remembers).
    static func authorize() async -> Bool {
        let speech: Bool = await withCheckedContinuation { continuation in
            SFSpeechRecognizer.requestAuthorization { continuation.resume(returning: $0 == .authorized) }
        }
        guard speech else { return false }
        return await AVAudioApplication.requestRecordPermission()
    }
}

enum MicrophoneError: LocalizedError {
    case noInput

    var errorDescription: String? { "no audio input" }
}

/// The one microphone tap. The recogniser and the wake word never run at once (the core
/// never asks for both), so one engine serves both; whoever starts last owns it.
final class Microphone: @unchecked Sendable {
    private let engine = AVAudioEngine()
    private let lock = NSLock()
    private var request: SFSpeechAudioBufferRecognitionRequest?

    func start(feeding request: SFSpeechAudioBufferRecognitionRequest) throws {
        lock.lock()
        defer { lock.unlock() }
        self.request = request
        let input = engine.inputNode
        // With no usable input (a simulator host without a microphone, a route that just
        // went away) the format has no rate or channels, and installing a tap with it
        // raises an Objective-C exception that ends the app. Fail the recognition instead.
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else {
            self.request = nil
            throw MicrophoneError.noInput
        }
        input.removeTap(onBus: 0)
        input.installTap(onBus: 0, bufferSize: 1_024, format: format) { [weak self] buffer, _ in
            self?.append(buffer)
        }
        if !engine.isRunning {
            engine.prepare()
            try engine.start()
        }
    }

    /// Stops feeding `request` (and the engine) if it is the one being fed.
    func stop(_ request: SFSpeechAudioBufferRecognitionRequest?) {
        lock.lock()
        defer { lock.unlock() }
        guard request == nil || request === self.request else { return }
        self.request = nil
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
    }

    private func append(_ buffer: AVAudioPCMBuffer) {
        lock.lock()
        let request = self.request
        lock.unlock()
        request?.append(buffer)
    }
}

/// One on-device recognition of one utterance, reporting through `report`.
final class Recognition: @unchecked Sendable {
    enum Result {
        case partial(String)
        case final(String)
        case failed(String)
    }

    private let microphone: Microphone
    private let request = SFSpeechAudioBufferRecognitionRequest()
    private let lock = NSLock()
    /// Guarded by `lock`. The recogniser is kept for the life of its task (a task whose
    /// recogniser is released can end early or never report).
    private var recognizer: SFSpeechRecognizer?
    private var task: SFSpeechRecognitionTask?
    private var ended = false

    init(microphone: Microphone) {
        self.microphone = microphone
    }

    /// Starts; `report` is called from the recogniser's queue, at most one `.final` or
    /// `.failed`, and nothing after `cancel()`. A recognition cancelled before (or while) it
    /// starts never touches the microphone: `ended` is checked under the lock before the tap
    /// is installed, the tap is installed under the same lock, and checked again after.
    func start(report: @escaping @Sendable (Result) -> Void) {
        guard !isEnded else { return }
        guard let recognizer = SFSpeechRecognizer(locale: VoiceAdapters.locale),
              recognizer.isAvailable, recognizer.supportsOnDeviceRecognition else {
            if end() { report(.failed("On-device speech recognition is not available.")) }
            return
        }
        // On-device only: never server recognition.
        request.requiresOnDeviceRecognition = true
        request.shouldReportPartialResults = true
        request.addsPunctuation = true
        let task = recognizer.recognitionTask(with: request) { [weak self] result, error in
            guard let self else { return }
            if let result, result.isFinal {
                if self.end() { report(.final(result.bestTranscription.formattedString)) }
            } else if let result {
                if !self.isEnded { report(.partial(result.bestTranscription.formattedString)) }
            } else if let error {
                if self.end() { report(.failed(error.localizedDescription)) }
            }
        }
        lock.lock()
        guard !ended else {
            lock.unlock()
            task.cancel()
            return
        }
        self.recognizer = recognizer
        self.task = task
        var failure: (any Error)?
        do {
            try microphone.start(feeding: request)
        } catch {
            failure = error
        }
        lock.unlock()
        if let failure {
            if end() { report(.failed("The microphone could not start (\(failure.localizedDescription)).")) }
            cancelTask()
        } else if isEnded {
            // Cancelled right after the tap went in: take it down again.
            microphone.stop(request)
            cancelTask()
        }
    }

    /// Ends the audio; the final result follows.
    func finish() {
        request.endAudio()
        microphone.stop(request)
    }

    /// Cancels: nothing more is reported.
    func cancel() {
        _ = end()
        cancelTask()
        microphone.stop(request)
    }

    private func cancelTask() {
        lock.lock()
        let task = self.task
        lock.unlock()
        task?.cancel()
    }

    private var isEnded: Bool {
        lock.lock()
        defer { lock.unlock() }
        return ended
    }

    /// Marks the end; `true` the first time.
    private func end() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        if ended { return false }
        ended = true
        microphone.stop(request)
        return true
    }
}

final class OnDeviceSpeechRecognizer: SpeechRecognizing, @unchecked Sendable {
    private let microphone: Microphone
    private let stream: AsyncStream<SpeechRecognitionEvent>
    private let sink: AsyncStream<SpeechRecognitionEvent>.Continuation
    private let lock = NSLock()
    private var current: (id: UInt64, recognition: Recognition)?

    init(microphone: Microphone) {
        self.microphone = microphone
        (stream, sink) = AsyncStream.makeStream(of: SpeechRecognitionEvent.self)
    }

    func isAvailable() async -> Bool {
        guard let recognizer = SFSpeechRecognizer(locale: VoiceAdapters.locale),
              recognizer.supportsOnDeviceRecognition else { return false }
        let status = SFSpeechRecognizer.authorizationStatus()
        return status != .denied && status != .restricted
    }

    func events() -> AsyncStream<SpeechRecognitionEvent> { stream }

    func start(id: UInt64) async {
        // Registered before the permission await, so a `stop(id)` in that window wins (it
        // clears `current` and cancels the recognition, which then never starts).
        let recognition = Recognition(microphone: microphone)
        let previous = swap((id, recognition))
        previous?.recognition.cancel()
        let allowed = await VoiceAdapters.authorize()
        guard isCurrent(id, recognition) else { return }
        guard allowed else {
            _ = clear(id)
            recognition.cancel()
            sink.yield(.failed(id: id, reason: "Speech recognition or the microphone is not allowed."))
            return
        }
        let sink = sink
        recognition.start { result in
            switch result {
            case .partial(let text): sink.yield(.partial(id: id, text: text))
            case .final(let text): sink.yield(.final(id: id, text: text))
            case .failed(let reason): sink.yield(.failed(id: id, reason: reason))
            }
        }
    }

    func finish(id: UInt64) async {
        let recognition = lock.withLock { current?.id == id ? current?.recognition : nil }
        recognition?.finish()
    }

    func stop(id: UInt64) async {
        let recognition: Recognition? = lock.withLock {
            guard current?.id == id else { return nil }
            let recognition = current?.recognition
            current = nil
            return recognition
        }
        recognition?.cancel()
    }

    private func swap(_ next: (id: UInt64, recognition: Recognition)?) -> (id: UInt64, recognition: Recognition)? {
        lock.lock()
        defer { lock.unlock() }
        let previous = current
        current = next
        return previous
    }

    private func isCurrent(_ id: UInt64, _ recognition: Recognition) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return current?.id == id && current?.recognition === recognition
    }

    /// Forgets recognition `id` if it is the current one.
    private func clear(_ id: UInt64) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard current?.id == id else { return false }
        current = nil
        return true
    }
}

/// The first wake-word implementation: the on-device recogniser listening continuously
/// while armed (Jarvis in front, the setting on). Each utterance is reported as transcripts
/// of arming `id`; the core reads the wake phrase from them. A dedicated keyword engine is a
/// later device spike.
final class ForegroundWakeWord: WakeWordDetecting, @unchecked Sendable {
    private let microphone: Microphone
    private let stream: AsyncStream<WakeWordEvent>
    private let sink: AsyncStream<WakeWordEvent>.Continuation
    private let lock = NSLock()
    private var armedId: UInt64?
    private var recognition: Recognition?

    init(microphone: Microphone) {
        self.microphone = microphone
        (stream, sink) = AsyncStream.makeStream(of: WakeWordEvent.self)
    }

    func events() -> AsyncStream<WakeWordEvent> { stream }

    func arm(id: UInt64) async {
        // Armed before the permission await, so a `disarm(id)` in that window wins.
        lock.withLock { armedId = id }
        let allowed = await VoiceAdapters.authorize()
        let still: Bool = lock.withLock {
            let still = armedId == id
            if still && !allowed { armedId = nil }
            return still
        }
        guard still else { return }
        guard allowed else {
            sink.yield(.failed(id: id, reason: "Speech recognition or the microphone is not allowed."))
            return
        }
        listen(id)
    }

    func disarm(id: UInt64) async {
        let recognition: Recognition? = lock.withLock {
            guard armedId == id else { return nil }
            armedId = nil
            let recognition = self.recognition
            self.recognition = nil
            return recognition
        }
        recognition?.cancel()
    }

    /// One utterance; the next starts when it ends, while still armed.
    private func listen(_ id: UInt64) {
        let recognition = Recognition(microphone: microphone)
        lock.lock()
        guard armedId == id else {
            lock.unlock()
            return
        }
        let previous = self.recognition
        self.recognition = recognition
        lock.unlock()
        previous?.cancel()
        // Disarmed meanwhile: this recognition never starts (a disarm after this check
        // cancels it, and a cancelled recognition never touches the microphone).
        lock.lock()
        let still = armedId == id && self.recognition === recognition
        lock.unlock()
        guard still else { return }
        let sink = sink
        recognition.start { [weak self] result in
            switch result {
            case .partial(let text):
                sink.yield(.partial(id: id, text: text))
            case .final(let text):
                sink.yield(.final(id: id, text: text))
                self?.listen(id)
            case .failed(let reason):
                // The core arms it again after its retry delay.
                sink.yield(.failed(id: id, reason: reason))
            }
        }
    }
}

/// `AVSpeechSynthesizer`; every utterance's end (finished or cancelled) is reported by id.
final class SystemSpeechSynthesizer: NSObject, SpeechSynthesizing, AVSpeechSynthesizerDelegate, @unchecked Sendable {
    private let synthesizer = AVSpeechSynthesizer()
    private let stream: AsyncStream<UInt64>
    private let sink: AsyncStream<UInt64>.Continuation
    private let lock = NSLock()
    private var ids: [ObjectIdentifier: UInt64] = [:]

    override init() {
        (stream, sink) = AsyncStream.makeStream(of: UInt64.self)
        super.init()
        synthesizer.delegate = self
        // The app's audio session is used as configured (`SystemAudioSession`).
        synthesizer.usesApplicationAudioSession = true
    }

    func events() -> AsyncStream<UInt64> { stream }

    func speak(_ text: String, id: UInt64) async {
        // The utterance is made on the main actor, where the synthesiser takes it: it is
        // not Sendable and never crosses.
        await MainActor.run {
            let utterance = AVSpeechUtterance(string: text)
            utterance.voice = AVSpeechSynthesisVoice(language: "en-US")
            lock.withLock { ids[ObjectIdentifier(utterance)] = id }
            synthesizer.speak(utterance)
        }
    }

    func stopSpeaking() async {
        _ = await MainActor.run { synthesizer.stopSpeaking(at: .immediate) }
    }

    func playCue(_ cue: VoiceCueSound) async {
        // The system's begin and end recording sounds.
        AudioServicesPlaySystemSound(cue == .listening ? 1113 : 1114)
    }

    private func ended(_ utterance: AVSpeechUtterance) {
        lock.lock()
        let id = ids.removeValue(forKey: ObjectIdentifier(utterance))
        lock.unlock()
        if let id { sink.yield(id) }
    }

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
        ended(utterance)
    }

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
        ended(utterance)
    }
}

/// `AVAudioSession`: `.playAndRecord` (the microphone and speech at once), mode `.default`,
/// to the speaker. Not `.voiceChat`: that mode is for calls (it applies the system's
/// voice processing and call-style gain, and on iOS changes how other audio is mixed and
/// routed); Jarvis's echo rule does not depend on it — the core ignores what it hears while
/// it speaks and for a tail after, and anything contained in what it just said.
final class SystemAudioSession: AudioSessionControlling, @unchecked Sendable {
    private let stream: AsyncStream<AudioSessionEvent>
    private let sink: AsyncStream<AudioSessionEvent>.Continuation
    private var observers: [NSObjectProtocol] = []

    init() {
        (stream, sink) = AsyncStream.makeStream(of: AudioSessionEvent.self)
        let center = NotificationCenter.default
        let sink = sink
        observers.append(center.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: nil) { note in
            guard let info = note.userInfo,
                  let raw = info[AVAudioSessionInterruptionTypeKey] as? UInt,
                  let type = AVAudioSession.InterruptionType(rawValue: raw) else { return }
            switch type {
            case .began:
                sink.yield(.interruptionBegan)
            case .ended:
                let options = AVAudioSession.InterruptionOptions(rawValue: info[AVAudioSessionInterruptionOptionKey] as? UInt ?? 0)
                sink.yield(.interruptionEnded(shouldResume: options.contains(.shouldResume)))
            @unknown default:
                break
            }
        })
        observers.append(center.addObserver(forName: AVAudioSession.routeChangeNotification, object: nil, queue: nil) { note in
            guard let raw = note.userInfo?[AVAudioSessionRouteChangeReasonKey] as? UInt,
                  let reason = AVAudioSession.RouteChangeReason(rawValue: raw) else { return }
            // Headphones unplugged: speech must not carry on out of the speaker.
            if reason == .oldDeviceUnavailable { sink.yield(.routeChanged) }
        })
    }

    deinit {
        for observer in observers { NotificationCenter.default.removeObserver(observer) }
    }

    func events() -> AsyncStream<AudioSessionEvent> { stream }

    func activate() async throws {
        let session = AVAudioSession.sharedInstance()
        try session.setCategory(.playAndRecord, mode: .default, options: [.defaultToSpeaker])
        try session.setActive(true)
    }

    func deactivate() async {
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }
}
