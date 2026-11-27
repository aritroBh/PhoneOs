import Foundation
import JarvisCore

// The IO voice needs, one port per piece of hardware or OS service. Every decision (what a
// transcript means, what is said, when a task starts) is the core's (`JarvisVoice`); these
// only listen, speak and report. The app's adapters live in `App/Jarvis/Platform/
// VoiceAdapters.swift`; tests use the fakes in `Tests/JarvisKitTests/Support/VoiceFakes.swift`.
//
// Every recognition and every arming carries an id chosen by `VoiceService`; events report
// it, so a late result of a recognition that was stopped is never taken for the next one.

/// What one recognition reports.
public enum SpeechRecognitionEvent: Sendable, Equatable {
    /// The words so far.
    case partial(id: UInt64, text: String)
    /// The whole utterance; the recognition has ended.
    case final(id: UInt64, text: String)
    /// The recognition failed (or speech recognition is unavailable); it has ended.
    case failed(id: UInt64, reason: String)
}

/// The speech recogniser. The app's adapter is `SFSpeechRecognizer` with on-device
/// recognition required (`requiresOnDeviceRecognition = true`): where that is not available,
/// voice is unavailable; it never falls back to server recognition.
public protocol SpeechRecognizing: Sendable {
    /// Whether on-device recognition can run here at all (language supported, the device
    /// can, not refused by the owner). Asking for permission happens on the first `start`.
    func isAvailable() async -> Bool
    /// Every recognition's events, in order (one stream for the port's lifetime).
    func events() -> AsyncStream<SpeechRecognitionEvent>
    /// Starts recognition `id` (one utterance: it ends with `.final` or `.failed`).
    func start(id: UInt64) async
    /// Ends the audio of recognition `id` and delivers its final result (push to talk
    /// released).
    func finish(id: UInt64) async
    /// Cancels recognition `id`: no more events from it.
    func stop(id: UInt64) async
}

/// What the wake word reports.
public enum WakeWordEvent: Sendable, Equatable {
    /// A dedicated keyword engine heard the phrase (no text).
    case detected(id: UInt64)
    /// The foreground implementation: the recogniser's transcript stream while armed. The
    /// core reads the wake phrase from it.
    case partial(id: UInt64, text: String)
    case final(id: UInt64, text: String)
    /// Listening failed; it is no longer armed.
    case failed(id: UInt64, reason: String)
}

/// The wake word. First implementation (the app): the foreground recogniser's own
/// transcripts while Jarvis is in front and the setting is on. A dedicated keyword-spotting
/// engine and background listening are a later device spike.
public protocol WakeWordDetecting: Sendable {
    func events() -> AsyncStream<WakeWordEvent>
    /// Starts listening as arming `id` (continuous: it keeps listening across utterances).
    func arm(id: UInt64) async
    /// Stops listening.
    func disarm(id: UInt64) async
}

/// The cue sounds.
public enum VoiceCueSound: Sendable, Equatable {
    case listening
    case ended
}

/// The speech synthesiser. The app's adapter is `AVSpeechSynthesizer`.
public protocol SpeechSynthesizing: Sendable {
    /// Ids of utterances that ended (finished or stopped), in order.
    func events() -> AsyncStream<UInt64>
    /// Queues `text` as utterance `id` (returns at once; the end arrives on `events`).
    func speak(_ text: String, id: UInt64) async
    /// Stops everything queued or being said (each reports its end).
    func stopSpeaking() async
    func playCue(_ cue: VoiceCueSound) async
}

/// What the audio session reports.
public enum AudioSessionEvent: Sendable, Equatable {
    /// A phone call, Siri or an alarm took the audio.
    case interruptionBegan
    /// It ended; `shouldResume` is the system's hint.
    case interruptionEnded(shouldResume: Bool)
    /// The route changed in a way that should stop speech (headphones unplugged).
    case routeChanged
}

/// The audio session. The app's adapter is `AVAudioSession` (`.playAndRecord`, mode
/// `.default`; see the adapter for why).
public protocol AudioSessionControlling: Sendable {
    func events() -> AsyncStream<AudioSessionEvent>
    func activate() async throws
    func deactivate() async
}

/// The four ports together.
public struct VoicePorts: Sendable {
    public var recognizer: any SpeechRecognizing
    public var synthesizer: any SpeechSynthesizing
    public var wakeWord: any WakeWordDetecting
    public var audio: any AudioSessionControlling

    public init(recognizer: any SpeechRecognizing, synthesizer: any SpeechSynthesizing,
                wakeWord: any WakeWordDetecting, audio: any AudioSessionControlling) {
        self.recognizer = recognizer
        self.synthesizer = synthesizer
        self.wakeWord = wakeWord
        self.audio = audio
    }
}
