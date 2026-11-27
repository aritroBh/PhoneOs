import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

// The voice break-it fixes in the shell, each with a variant the review did not try: the
// request id passed through `TaskCenter` (f1, f9), the row's lifetime (S1), a stopped
// service (S2), stop then start (S3), the voice mark in the history (S4), "Run again" on a
// voice task's result, and the lock state reported before `start()`. Every wait is on a
// condition with a bounded timeout.

@MainActor
@Suite("Voice review fixes")
struct VoiceReviewFixTests {
    static let approving = ScriptedOwner.Rules(approve: [.firstUse, .systemSetting], decline: [], yes: true)
    static let leaving = ScriptedOwner.Rules(approve: [], decline: [], yes: nil)

    /// The voice task's session carries the request id and its start is taken for the voice
    /// task ("On it."); a typed task's session carries none.
    @Test func theVoiceTasksSessionCarriesItsRequest() async throws {
        let h = VoiceHarness("voice-fix-request", rules: Self.approving, settings: wakeOn())
        defer { h.cleanUp() }
        var requests: [UInt64?] = []
        h.app.center.observe { event in
            if case .sessionStarted(let session) = event { requests.append(session.voiceRequest) }
        }
        await h.service.start()
        try await eventually("armed") { h.wake.armed != nil }
        h.wake.hear("Hey Jarvis, turn off Wi-Fi")
        try await eventually("the voice task ended") { h.specs.count == 1 && h.app.center.session == nil }
        try await eventually("settled") { h.service.isSettled && h.service.status.phase == .idle }
        #expect(h.synth.texts.first == "On it.", "\(h.synth.texts)")
        guard case .success(let typed) = h.app.center.start("turn off Wi-Fi") else {
            Issue.record("the typed task did not start")
            return
        }
        _ = await typed.value
        #expect(requests.count == 2)
        #expect(requests.first.flatMap { $0 } != nil, "\(requests)")
        #expect(requests.last.flatMap { $0 } == nil, "\(requests)")
    }

    /// Another start landing just before voice's own (inside the start call itself): the
    /// core hears "started" without its id, then its own refusal, and says "I am busy.",
    /// never "On it.", and the typed task is another task to it.
    @Test func aStartLandingInsideVoicesOwnStartIsNotTheVoiceTask() async throws {
        let app = AppHarness("voice-fix-inside-start", rules: Self.leaving)
        let recognizer = FakeRecognizer()
        let wake = FakeWakeWord()
        let synth = FakeSynthesizer()
        let center = app.center
        let service = VoiceService(
            ports: VoicePorts(recognizer: recognizer, synthesizer: synth, wakeWord: wake, audio: FakeAudioSession()),
            center: center, settings: wakeOn(), clock: ManualClock(), tickInterval: nil,
            startTask: { text, origin, request in
                // The owner's typed task wins the slot first.
                _ = center.start("Turn off Bluetooth")
                switch center.start(text, origin: origin, voiceRequest: request) {
                case .success: return nil
                case .failure(let refusal): return refusal
                }
            }
        )
        defer {
            service.stop()
            app.cleanUp()
        }
        await service.start()
        try await eventually("armed") { wake.armed != nil && service.isSettled }
        wake.hear("Hey Jarvis, turn off Wi-Fi")
        try await eventually("busy") { synth.texts.contains("I am busy.") && service.isSettled }
        #expect(!synth.texts.contains("On it."), "\(synth.texts)")
        #expect(center.session?.text == "Turn off Bluetooth")
        #expect(center.session?.voiceRequest == nil)
        #expect(service.status.phase == .idle)
        #expect(service.status.otherTaskRunning)
        // Nothing listens for it but the wake word (a bare "stop" reaches nothing).
        #expect(!service.status.recognizing)
    }

    /// S1: the row of a confirmed request goes when its start is refused, and the row of a
    /// hands-free request when its task ends.
    @Test func theRowGoesWhenItsExchangeEnds() async throws {
        let app = AppHarness("voice-fix-row", rules: Self.leaving)
        let recognizer = FakeRecognizer()
        let synth = FakeSynthesizer()
        // The shell refuses the start (the slot was taken in between).
        let service = VoiceService(
            ports: VoicePorts(recognizer: recognizer, synthesizer: synth, wakeWord: FakeWakeWord(),
                              audio: FakeAudioSession()),
            center: app.center, settings: defaultVoiceSettings(), clock: ManualClock(), tickInterval: nil,
            startTask: { _, _, _ in .busy }
        )
        defer {
            service.stop()
            app.cleanUp()
        }
        await service.start()
        service.pushToTalk(pressed: true)
        try await eventually("the recogniser runs") { recognizer.running != nil && service.isSettled }
        recognizer.partial("turn off Wi-Fi")
        service.pushToTalk(pressed: false)
        recognizer.final("turn off Wi-Fi")
        try await eventually("the row waits for a touch") { service.transcript?.needsConfirm == true && service.isSettled }
        service.confirmTranscript("turn off Wi-Fi")
        try await eventually("refused") { service.isSettled && synth.texts.contains("I am busy.") }
        #expect(service.status.phase == .idle)
        #expect(service.transcript == nil, "\(String(describing: service.transcript))")

        let w = VoiceHarness("voice-fix-row-hands-free", rules: Self.approving, settings: wakeOn())
        defer { w.cleanUp() }
        await w.service.start()
        try await eventually("armed") { w.wake.armed != nil }
        w.wake.hear("Hey Jarvis, turn off Wi-Fi")
        try await eventually("the row is shown") { w.service.transcript?.text == "turn off Wi-Fi" }
        try await eventually("the task ended") {
            w.app.center.session == nil && w.service.isSettled && w.service.status.phase == .idle
        }
        #expect(w.service.transcript == nil)
    }

    /// S2: a stopped service steps nothing: settings, ticks, a confirm and task events drive
    /// no port.
    @Test func aStoppedServiceDrivesNoPort() async throws {
        let h = VoiceHarness("voice-fix-stopped", rules: Self.approving, settings: defaultVoiceSettings())
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("settled") { h.service.isSettled }
        h.service.stop()
        h.service.setSettings(wakeOn(.whenUnlocked))
        h.pass(10_000)
        h.service.confirmTranscript("turn off Wi-Fi")
        _ = h.app.center.start("turn off Wi-Fi")
        try await eventually("the typed task ended") { h.app.center.session == nil }
        try await eventually("settled") { h.service.isSettled }
        #expect(h.wake.arms.withLock { $0 }.isEmpty)
        #expect(h.recognizer.started.withLock { $0 }.isEmpty)
        #expect(h.synth.texts.isEmpty, "\(h.synth.texts)")
    }

    /// S3: stop and start, twice: the wake word is armed again each time.
    @Test func stopThenStartArmsAgainEveryTime() async throws {
        let h = VoiceHarness("voice-fix-restart", settings: wakeOn())
        defer { h.cleanUp() }
        for round in 1...3 {
            await h.service.start()
            try await eventually("armed in round \(round)") {
                h.wake.armed != nil && h.wake.arms.withLock { $0.count } == round && h.service.isSettled
            }
            h.service.stop()
            try await eventually("disarmed in round \(round)") { h.wake.armed == nil }
        }
    }

    /// S3: a voice task that outlives a stop is another task to the restarted service: its
    /// end is not taken for a request of the new core, nothing is said for it, and voice
    /// works again after it.
    @Test func aVoiceTaskThatOutlivesAStopIsAnotherTaskAfterTheRestart() async throws {
        let h = VoiceHarness("voice-fix-outlive", rules: Self.leaving, settings: wakeOn())
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("armed") { h.wake.armed != nil }
        h.wake.hear("Hey Jarvis, turn off Wi-Fi")
        try await eventually("a question is pending") { h.app.owner.broker.pending != nil }
        h.service.stop()
        await h.service.start()
        try await eventually("the restarted core knows a task runs") {
            h.service.isSettled && h.service.status.otherTaskRunning && h.service.status.phase == .idle
        }
        let said = h.synth.texts.count
        let prompt = try #require(h.app.owner.broker.pending)
        _ = h.app.owner.broker.decline(prompt.id)
        try await eventually("the task ended") {
            h.app.center.session == nil && h.service.isSettled && !h.service.status.otherTaskRunning
        }
        #expect(h.synth.texts.count == said, "\(h.synth.texts)")
        try await eventually("armed again") { h.wake.armed != nil }
    }

    /// S4: who asked, in the record: voice is marked and shown as "Voice", a paired device by
    /// its name, the owner by nothing; an old record without the field reads as unmarked.
    @Test func theRecordSaysWhoAsked() throws {
        let spec = TaskSpec(id: "t", text: "turn off Wi-Fi", remote: false, voice: true)
        let result = RunResult(outcome: Outcome(status: .success, summary: "ok", steps: 1, durationMs: 1),
                               endReason: .sessionFinished, memory: .saved, effectCount: 1, modelCalls: 1)
        let voice = TaskRecord(task: spec, result: result, origin: .voice)
        #expect(voice.heardByVoice == true && voice.requesterText == "Voice")
        let remote = TaskRecord(task: spec, result: result,
                                origin: .remote(taskId: 1, deviceId: "d", deviceName: "Voice"))
        #expect(remote.heardByVoice == nil && remote.requesterText == "from Voice")
        let owner = TaskRecord(task: spec, result: result, origin: .owner)
        #expect(owner.heardByVoice == nil && owner.requesterText == nil)
        var json = String(decoding: try JSONEncoder().encode(owner), as: UTF8.self)
        #expect(!json.contains("heardByVoice"))
        json = String(decoding: try JSONEncoder().encode(voice), as: UTF8.self)
        let back = try JSONDecoder().decode(TaskRecord.self, from: Data(json.utf8))
        #expect(back == voice)
    }

    /// "Run again" on a voice task's result starts the same words as the owner's (the owner
    /// read them and touched the button): what `AppModel.runAgain` does, through the same
    /// entry, `TaskCenter.start(_:skill:origin: .owner)`.
    @Test func runAgainOnAVoiceTasksResultIsTheOwners() async throws {
        let h = VoiceHarness("voice-fix-run-again", rules: Self.approving, settings: wakeOn())
        defer { h.cleanUp() }
        var sessions: [TaskCenter.Session] = []
        h.app.center.observe { event in
            if case .sessionStarted(let session) = event { sessions.append(session) }
        }
        await h.service.start()
        try await eventually("armed") { h.wake.armed != nil }
        h.wake.hear("Hey Jarvis, turn off Wi-Fi")
        try await eventually("the voice task ended") { h.specs.count == 1 && h.app.center.session == nil }
        let heard = try #require(sessions.first)
        #expect(heard.origin == .voice)
        guard case .success(let again) = h.app.center.start(heard.text, skill: heard.skill, origin: .owner) else {
            Issue.record("run again did not start")
            return
        }
        _ = await again.value
        try await eventually("two records") { h.app.history.count == 2 }
        #expect(h.specs.count == 2 && !h.specs[1].voice && !h.specs[1].remote)
        let records = h.app.history
        #expect(records.first { $0.id == h.specs[0].id }?.heardByVoice == true)
        #expect(records.first { $0.id == h.specs[1].id }?.heardByVoice == nil)
    }

    /// R3: the lock state reported before `start()` holds from the first step: nothing is
    /// armed while locked, and unlocking arms.
    @Test func theLockReportedBeforeStartHolds() async throws {
        let h = VoiceHarness("voice-fix-locked-at-launch", settings: wakeOn())
        defer { h.cleanUp() }
        h.service.setLocked(true)
        await h.service.start()
        try await eventually("settled") { h.service.isSettled }
        #expect(h.wake.arms.withLock { $0 }.isEmpty)
        h.service.setLocked(false)
        try await eventually("armed") { h.wake.armed != nil }
    }
}
