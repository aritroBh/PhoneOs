import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

// Voice, second break-it review, in the shell (against 384eb5c). `finding…` tests FAIL
// today and pass once fixed; `sound…` tests held. Every wait is on a condition with a
// bounded timeout (`waitUntil` returns false instead of throwing, so a finding fails by its
// own expectation).

@MainActor
@Suite("Voice break-it 2")
struct VoiceBreakIt2Tests {
    static let approving = ScriptedOwner.Rules(approve: [.firstUse, .systemSetting], decline: [], yes: true)

    /// **Finding S5, medium (library; the app never calls `stop()` today): after `stop()`
    /// and `start()` voice is deaf and mute to its ports.**
    ///
    /// Sequence: wake word on; `start()`; armed; `stop()`; `start()`; armed again (what
    /// `stopThenStartArmsAgainEveryTime` checks); the room says "Hey Jarvis, turn off
    /// Wi-Fi" on the wake word's stream.
    ///
    /// What happens: `stop()` cancels the tasks consuming the ports' streams, and
    /// cancelling a task that iterates an `AsyncStream` finishes that stream. Every port
    /// returns the same stream from `events()` for its lifetime (the fakes and the app's
    /// adapters alike), so the consumers `start()` makes end at once: no transcript, no
    /// wake word, no "speech finished", no interruption ever reaches the new core. The wake
    /// word is armed and the audio session active (the microphone is on), yet nothing it
    /// hears acts — "Jarvis, stop" included; every line said stays "speaking" until
    /// `MAX_SPEAKING_MS`.
    ///
    /// Should: either ports hand out a fresh stream per `events()` call (and the service
    /// takes a new one on `start()`), or the service keeps its consumers across `stop()` and
    /// drops what arrives while stopped.
    @Test func findingVoiceIsDeafAfterStopAndStart() async throws {
        let h = VoiceHarness("voice-breakit2-restart-deaf", rules: Self.approving, settings: wakeOn())
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("armed") { h.wake.armed != nil && h.service.isSettled }
        h.service.stop()
        await h.service.start()
        try await eventually("armed again") {
            h.wake.armed != nil && h.wake.arms.withLock { $0.count } == 2 && h.service.isSettled
        }
        h.wake.hear("Hey Jarvis, turn off Wi-Fi")
        let heard = await waitUntil(seconds: 3) { !h.specs.isEmpty || h.service.transcript != nil }
        #expect(heard, "the request heard after the restart never reached the core")
        #expect(h.service.status.phase == .idle || !h.specs.isEmpty)
    }

    /// A push-to-talk transcript confirmed by touch starts as the owner's words: the spec is
    /// neither voice nor remote and the record is not marked "Voice"; a hands-free request
    /// is marked.
    @Test func soundATouchConfirmedRequestIsTheOwnersAndAHandsFreeOneIsMarked() async throws {
        let h = VoiceHarness("voice-breakit2-confirmed", rules: Self.approving, settings: wakeOn())
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("armed") { h.wake.armed != nil && h.service.isSettled }
        h.service.pushToTalk(pressed: true)
        try await eventually("the recogniser runs") { h.recognizer.running != nil && h.service.isSettled }
        h.recognizer.partial("turn off Wi-Fi")
        h.service.pushToTalk(pressed: false)
        h.recognizer.final("turn off Wi-Fi")
        try await eventually("the row waits") { h.service.transcript?.needsConfirm == true && h.service.isSettled }
        h.service.confirmTranscript("turn off Wi-Fi")
        try await eventually("the confirmed task ended") { h.specs.count == 1 && h.app.center.session == nil }
        try await eventually("settled") { h.service.isSettled && h.service.status.phase == .idle }
        #expect(!h.specs[0].voice && !h.specs[0].remote)
        h.pass(10_000)
        try await eventually("armed") { h.wake.armed != nil && h.service.isSettled }
        h.wake.hear("Hey Jarvis, turn off Wi-Fi")
        try await eventually("the hands-free task ended") { h.specs.count == 2 && h.app.center.session == nil }
        #expect(h.specs[1].voice)
        try await eventually("two records") { h.app.history.count == 2 }
        let records = h.app.history
        #expect(records.first { $0.id == h.specs[0].id }?.heardByVoice == nil)
        #expect(records.first { $0.id == h.specs[1].id }?.heardByVoice == true)
        #expect(records.first { $0.id == h.specs[1].id }?.requesterText == "Voice")
    }
}
