import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

// The second voice break-it fixes in the shell, each with a variant the review did not
// try: every port reaches the core after any number of restarts and an old arming's
// results are still ignored (S5), `stop()`'s cleanup lands before the next `start()` (M4),
// the covered (inactive) scene, and the voice mark on the run view. Every wait is on a
// condition with a bounded timeout.

@MainActor
@Suite("Voice review fixes 2")
struct VoiceReviewFix2Tests {
    static let approving = ScriptedOwner.Rules(approve: [.firstUse, .systemSetting], decline: [], yes: true)

    /// S5: after two restarts the wake word's, the synthesiser's, the recogniser's and the
    /// audio session's events all reach the core.
    @Test func everyPortReachesTheCoreAfterRestarts() async throws {
        let h = VoiceHarness("voice-fix2-restarts", rules: Self.approving, settings: wakeOn())
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("armed") { h.wake.armed != nil && h.service.isSettled }
        for round in 2...3 {
            h.service.stop()
            await h.service.start()
            try await eventually("armed again (\(round))") {
                h.wake.armed != nil && h.wake.arms.withLock { $0.count } == round && h.service.isSettled
            }
        }
        // The wake word and the synthesiser.
        h.wake.hear("Hey Jarvis, turn off Wi-Fi")
        try await eventually("the voice task ended") { h.specs.count == 1 && h.app.center.session == nil }
        #expect(h.specs[0].voice)
        try await eventually("its lines were said and finished") {
            h.synth.texts.contains("On it.") && !h.service.status.speaking && h.service.isSettled
                && h.service.status.phase == .idle
        }
        // The recogniser.
        h.pass(10_000)
        h.service.pushToTalk(pressed: true)
        try await eventually("the recogniser runs") { h.recognizer.running != nil && h.service.isSettled }
        h.recognizer.partial("open Mail")
        h.service.pushToTalk(pressed: false)
        h.recognizer.final("open Mail")
        try await eventually("the row waits") { h.service.transcript?.needsConfirm == true }
        h.service.discardTranscript()
        // The audio session.
        h.audio.interrupt()
        try await eventually("interrupted") { h.service.status.interrupted }
    }

    /// S5: what an arming of the old core reports after the restart is ignored.
    @Test func anOldArmingsResultAfterARestartIsIgnored() async throws {
        let h = VoiceHarness("voice-fix2-old-arming", rules: Self.approving, settings: wakeOn())
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("armed") { h.wake.armed != nil && h.service.isSettled }
        let old = try #require(h.wake.armed)
        h.service.stop()
        await h.service.start()
        try await eventually("armed again") { h.wake.armed != nil && h.wake.armed != old && h.service.isSettled }
        h.wake.hear("Hey Jarvis, turn off Wi-Fi", id: old)
        let started = await waitUntil(seconds: 0.5) { !h.specs.isEmpty }
        #expect(!started, "an old arming's transcript started \(h.specs)")
        // The current arming still works.
        h.wake.hear("Hey Jarvis, turn off Wi-Fi")
        try await eventually("the voice task started") { h.specs.count == 1 }
    }

    /// M4: a slow cleanup after `stop()` finishes before `start()` activates the audio
    /// session again (the deactivation never lands after the new activation).
    @Test func stopsCleanupLandsBeforeTheNextStart() async throws {
        let h = VoiceHarness("voice-fix2-cleanup-order", settings: wakeOn())
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("armed") { h.wake.armed != nil && h.service.isSettled }
        h.audio.deactivateDelay.withLock { $0 = .milliseconds(200) }
        h.service.stop()
        await h.service.start()
        try await eventually("armed again") {
            h.wake.arms.withLock { $0.count } == 2 && h.wake.armed != nil && h.service.isSettled
        }
        #expect(h.audio.order.withLock { $0 } == ["activate", "deactivate", "activate"],
                "\(h.audio.order.withLock { $0 })")
        // A stop while that start waits for the cleanup: the start does not go on.
        h.service.stop()
        let pending = Task { @MainActor in await h.service.start() }
        try await Task.sleep(for: .milliseconds(20))
        h.service.stop()
        await pending.value
        await h.service.waitForCleanup()
        let armed = await waitUntil(seconds: 0.5) { h.wake.arms.withLock { $0.count } > 2 }
        #expect(!armed, "a start cancelled by stop() armed the wake word")
        await h.service.start()
        try await eventually("armed after the next start") { h.wake.arms.withLock { $0.count } == 3 }
    }

    /// Covered (inactive): the wake word listens but a hands-free request starts nothing and
    /// nothing is said; active again, it starts. The scene is kept across stop and start.
    @Test func whileCoveredAHandsFreeRequestStartsNothing() async throws {
        let h = VoiceHarness("voice-fix2-covered", rules: Self.approving, settings: wakeOn())
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("armed") { h.wake.armed != nil && h.service.isSettled }
        h.service.setScene(.inactive)
        h.wake.hear("Hey Jarvis, turn off Wi-Fi")
        let started = await waitUntil(seconds: 0.5) { !h.specs.isEmpty || !h.synth.texts.isEmpty }
        #expect(!started, "\(h.specs) \(h.synth.texts)")
        #expect(h.wake.armed != nil, "the microphone went off while covered")
        // Kept while stopped.
        h.service.stop()
        await h.service.start()
        try await eventually("armed again") { h.wake.arms.withLock { $0.count } == 2 && h.service.isSettled }
        #expect(h.service.currentScene == .inactive)
        h.wake.hear("Hey Jarvis, turn off Wi-Fi")
        let startedAfterRestart = await waitUntil(seconds: 0.5) { !h.specs.isEmpty }
        #expect(!startedAfterRestart)
        h.service.setScene(.active)
        h.wake.hear("Hey Jarvis, turn off Wi-Fi")
        try await eventually("the voice task started") { h.specs.count == 1 }
        #expect(h.specs[0].voice)
    }

    /// M1's UI test hook (`-jarvis-voice-scripted-long`): push to talk on the scripted ports
    /// puts the whole long transcript on the row, waiting for Start (it is under the core's
    /// request cap, so it is not refused).
    @Test func theLongScriptedTranscriptReachesTheRowWhole() async throws {
        let app = AppHarness("voice-fix2-long-row")
        defer { app.cleanUp() }
        let long = ScriptedVoice.uiTestLongTranscript
        #expect(long.count > 150 && long.count <= Int(voiceTimings().maxRequestChars), "\(long.count)")
        let service = VoiceService(
            ports: ScriptedVoice.ports(wakeLines: [], recognizerLines: [long], delay: .milliseconds(20)),
            center: app.center, settings: defaultVoiceSettings(), tickInterval: .milliseconds(20)
        )
        defer { service.stop() }
        await service.start()
        try await eventually("push to talk can be offered") { service.canPushToTalk && service.isSettled }
        service.pushToTalk(pressed: true)
        try await eventually("the row waits for Start") { service.transcript?.needsConfirm == true }
        #expect(service.transcript?.text == long)
    }

    /// The run view marks a voice-started run (beside "Run again"); a typed run is not.
    @Test func theRunViewMarksAVoiceStartedRun() {
        var heard = LiveRun()
        heard.apply(.update(.started(TaskSpec(id: "v", text: "turn off Wi-Fi", voice: true))))
        #expect(heard.heardByVoice && heard.voiceMark != nil)
        var typed = LiveRun()
        typed.apply(.update(.started(TaskSpec(id: "t", text: "turn off Wi-Fi"))))
        #expect(!typed.heardByVoice && typed.voiceMark == nil)
        // A run view built for a voice session before its first update keeps the mark.
        let early = LiveRun(task: "turn off Wi-Fi", maxSteps: 10, heardByVoice: true)
        #expect(early.heardByVoice)
    }
}
