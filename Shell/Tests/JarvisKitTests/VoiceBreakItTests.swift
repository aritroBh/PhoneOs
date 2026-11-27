import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

// Voice break-it tests against 1073262 + b3dbc41 (the voice slice), over fakes of every
// port, the real core and the demo backend. `finding…` tests FAIL today and pass once the
// finding is fixed; `sound…` tests are things tried that held. Every wait is on a condition
// with a bounded timeout (`eventually`), never a fixed sleep.

/// A recogniser whose `stop` can be held at a gate (the service awaits each port call, so
/// the main actor is free while it waits: what the real adapter's async hops allow).
final class GatedRecognizer: SpeechRecognizing {
    private let stream: AsyncStream<SpeechRecognitionEvent>
    private let sink: AsyncStream<SpeechRecognitionEvent>.Continuation
    let started = Mutex<[UInt64]>([])
    let stopped = Mutex<[UInt64]>([])
    let gateClosed = Mutex(false)
    let atGate = Mutex(false)

    init() {
        (stream, sink) = AsyncStream.makeStream(of: SpeechRecognitionEvent.self)
    }

    func isAvailable() async -> Bool { true }
    func events() -> AsyncStream<SpeechRecognitionEvent> { stream }
    func start(id: UInt64) async { started.withLock { $0.append(id) } }
    func finish(id: UInt64) async {}

    func stop(id: UInt64) async {
        stopped.withLock { $0.append(id) }
        // Held while the gate is closed (bounded: a test that forgets to open it fails on
        // its own wait, it does not hang).
        let deadline = ContinuousClock.now + .seconds(10)
        while gateClosed.withLock({ $0 }), ContinuousClock.now < deadline {
            atGate.withLock { $0 = true }
            try? await Task.sleep(for: .milliseconds(2))
        }
        atGate.withLock { $0 = false }
    }

    var running: UInt64? {
        guard let last = started.withLock({ $0.last }) else { return nil }
        return stopped.withLock { $0.contains(last) } ? nil : last
    }

    func partial(_ text: String) {
        sink.yield(.partial(id: started.withLock { $0.last ?? 0 }, text: text))
    }
}

@MainActor
@Suite("Voice break-it")
struct VoiceBreakItTests {
    static let approving = ScriptedOwner.Rules(approve: [.firstUse, .systemSetting], decline: [], yes: true)
    static let leaving = ScriptedOwner.Rules(approve: [], decline: [], yes: nil)

    // MARK: - Findings

    /// **Finding, low/medium: another task's start, made while voice's own start is being
    /// carried out, is taken for the voice task.**
    ///
    /// Interleaving: wake word on; "Hey Jarvis" (Listening on the recogniser); partial "turn
    /// off Wi-Fi"; the endpoint passes and the core asks for `StopRecognizer`,
    /// `ShowTranscript`, `StartTask`. While the service awaits the recogniser's stop (any
    /// port call is a suspension point on the main actor), the owner starts "Turn off
    /// Bluetooth" on Ask: `TaskCenter` emits `sessionStarted` and the service queues
    /// `Started`. Then voice's `StartTask` is refused as busy and `StartRefused` is queued
    /// after it.
    ///
    /// What happens: the core reads `Started` in Starting as its own task: it says "On it.",
    /// keeps the barge-in recogniser listening through the owner's typed task (a bare "stop"
    /// from the room then stops it) and would read its answer aloud; the refusal is dropped.
    ///
    /// Should: the start is matched to voice's own request (the shell knows which start it
    /// made), so the core ends the exchange with "I am busy." and treats the typed task as
    /// another task.
    @Test func findingAnotherStartDuringStartingIsTakenForTheVoiceTask() async throws {
        let app = AppHarness("voice-breakit-start-race", rules: Self.leaving)
        let recognizer = GatedRecognizer()
        let wake = FakeWakeWord()
        let synth = FakeSynthesizer()
        let clock = ManualClock()
        let service = VoiceService(
            ports: VoicePorts(recognizer: recognizer, synthesizer: synth, wakeWord: wake, audio: FakeAudioSession()),
            center: app.center, settings: wakeOn(), clock: clock, tickInterval: nil
        )
        defer {
            recognizer.gateClosed.withLock { $0 = false }
            service.stop()
            app.cleanUp()
        }
        await service.start()
        try await eventually("armed") { wake.armed != nil && service.isSettled }
        wake.hear("Hey Jarvis")
        try await eventually("listening on the recogniser") { recognizer.running != nil && service.isSettled }
        recognizer.partial("turn off Wi-Fi")
        try await eventually("the partial is shown") { service.transcript?.text == "turn off Wi-Fi" && service.isSettled }
        recognizer.gateClosed.withLock { $0 = true }
        clock.advance(by: .milliseconds(Int(voiceTimings().endpointSilenceMs)))
        service.tick()
        try await eventually("the service waits on the recogniser's stop") { recognizer.atGate.withLock { $0 } }
        // The owner types a task on Ask meanwhile.
        guard case .success = app.center.start("Turn off Bluetooth") else {
            Issue.record("the typed task did not start")
            return
        }
        recognizer.gateClosed.withLock { $0 = false }
        try await eventually("settled") { service.isSettled && !recognizer.atGate.withLock { $0 } }
        #expect(app.center.session?.text == "Turn off Bluetooth")
        #expect(!synth.texts.contains("On it."), "the owner's typed task was announced as the voice task: \(synth.texts)")
        #expect(service.status.phase == .idle, "\(service.status)")
        #expect(service.status.otherTaskRunning, "\(service.status)")
    }

    /// **Finding, low: the transcript row with Start outlives its exchange.**
    ///
    /// Input: push to talk, "turn off Wi-Fi", the owner touches Start; the task runs and
    /// ends.
    ///
    /// What happens: the core never clears the row when the confirmed task starts, and the
    /// service clears a row only when it does not wait for a touch: after the task ends the
    /// row still shows "Heard. Check it, then Start." with an enabled Start that does nothing
    /// (the core is no longer Confirming). The owner sees a request "waiting" that is not.
    ///
    /// Should: the row goes once its Start was used (or the exchange ended).
    @Test func findingTheConfirmRowOutlivesItsExchange() async throws {
        let h = VoiceHarness("voice-breakit-row", rules: Self.approving, settings: defaultVoiceSettings())
        defer { h.cleanUp() }
        await h.service.start()
        h.service.pushToTalk(pressed: true)
        try await eventually("the recogniser runs") { h.recognizer.running != nil && h.service.isSettled }
        h.recognizer.partial("turn off Wi-Fi")
        h.service.pushToTalk(pressed: false)
        h.recognizer.final("turn off Wi-Fi")
        try await eventually("the row waits for a touch") { h.service.transcript?.needsConfirm == true && h.service.isSettled }
        h.service.confirmTranscript("turn off Wi-Fi")
        try await eventually("the task started") { h.specs.count == 1 }
        try await eventually("the task ended") {
            h.app.center.session == nil && h.service.isSettled && h.service.status.phase == .idle
        }
        #expect(h.service.transcript == nil, "a stale row with Start: \(String(describing: h.service.transcript))")
    }

    /// **Finding, low: after `stop()` the service still steps the core and drives the
    /// microphone.**
    ///
    /// Input: `start()`, `stop()` (documented "nothing listens or speaks afterwards"), then
    /// the owner's mic button (`pushToTalk(pressed: true)`).
    ///
    /// What happens: `send` is not gated on `running`: the core opens Listening and the
    /// service starts a recognition, whose results no consumer reads any more (the stream
    /// consumers were cancelled), so nothing ever stops it. The app does not call `stop()`
    /// today; any future caller (a "voice off" switch, a poison path, the scene going away)
    /// would leave the microphone on.
    ///
    /// Should: after `stop()` no event is stepped and no port is driven until `start()`.
    @Test func findingAStoppedServiceStillStartsTheMicrophone() async throws {
        let h = VoiceHarness("voice-breakit-stopped", settings: defaultVoiceSettings())
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("settled") { h.service.isSettled }
        h.service.stop()
        h.service.pushToTalk(pressed: true)
        try await eventually("settled") { h.service.isSettled }
        #expect(h.recognizer.started.withLock { $0 }.isEmpty, "the microphone started after stop()")
    }

    /// **Finding, low: `stop()` then `start()` leaves the wake word off for good.**
    ///
    /// Input: wake word on; `start()` (armed); `stop()` (the ports are disarmed, but the core
    /// is not told); `start()` again.
    ///
    /// What happens: the core still believes the wake word is armed, so the settings sent by
    /// the second `start()` change nothing and `ArmWakeWord` is never sent: hands-free voice
    /// is dead until the owner toggles the setting.
    ///
    /// Should: `stop()` resets the core (a new `JarvisVoice`, or events that say nothing
    /// listens) so a later `start()` arms the wake word again.
    @Test func findingRestartAfterStopNeverArmsTheWakeWord() async throws {
        let h = VoiceHarness("voice-breakit-restart", settings: wakeOn())
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("armed") { h.wake.armed != nil && h.service.isSettled }
        h.service.stop()
        try await eventually("disarmed") { h.wake.armed == nil }
        await h.service.start()
        h.pass()
        try await eventually("settled") { h.service.isSettled }
        let rearmed = await waitUntil(seconds: 2) { h.wake.armed != nil }
        #expect(rearmed, "the wake word is never armed again after stop() and start()")
    }

    /// **Finding, low: Activity cannot tell a task the microphone started from one the owner
    /// typed.**
    ///
    /// Input: a hands-free "Hey Jarvis, turn off Wi-Fi" runs to the end; the owner types
    /// the same "turn off Wi-Fi" and it runs to the end.
    ///
    /// What happens: `TaskRecord(task:result:origin:)` keeps the remote device's name and
    /// the workflow, but nothing of `.voice`: the two records are equal but for ids and
    /// times. The owner reviewing Activity cannot see that a video or a call started a task
    /// (the core's own `memory.log` entry has no origin either).
    ///
    /// Should: the record says the task was started by voice (as "from <device>" does for a
    /// paired device).
    @Test func findingHistoryDoesNotMarkAVoiceStartedTask() async throws {
        let h = VoiceHarness("voice-breakit-history", rules: Self.approving, settings: wakeOn())
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("armed") { h.wake.armed != nil }
        h.wake.hear("Hey Jarvis, turn off Wi-Fi")
        try await eventually("the voice task ended") { h.specs.count == 1 && h.app.center.session == nil }
        try await eventually("recorded") { h.app.history.count == 1 }
        guard case .success(let typed) = h.app.center.start("turn off Wi-Fi") else {
            Issue.record("the typed task did not start")
            return
        }
        _ = await typed.value
        try await eventually("recorded") { h.app.history.count == 2 }
        let records = h.app.history
        func comparable(_ r: TaskRecord) -> TaskRecord {
            var r = r
            r.id = ""
            r.finishedAt = Date(timeIntervalSince1970: 0)
            r.durationMs = 0
            r.modelCalls = 0
            r.steps = 0
            // The second run replays the flow the first one taught ("Done (saved routine).").
            r.summary = ""
            return r
        }
        let voice = try #require(records.first { $0.id == h.specs.first?.id })
        let owner = try #require(records.first { $0.id != voice.id })
        #expect(voice.task == owner.task)
        #expect(comparable(voice) != comparable(owner), "the voice-started record reads as the owner's: \(voice)")
    }

    // MARK: - Sound

    /// A task started elsewhere (typed on Ask) is never announced: no "I need your approval"
    /// for its question, nothing at its end, and no spoken "yes" answers its question.
    @Test func soundAnotherTasksEventsAreNeverSpokenAndAYesAnswersNothing() async throws {
        let h = VoiceHarness("voice-breakit-other", rules: Self.leaving, settings: wakeOn())
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("armed") { h.wake.armed != nil }
        _ = h.app.center.start("turn off Wi-Fi")
        try await eventually("a question is pending") { h.app.owner.broker.pending != nil }
        let prompt = try #require(h.app.owner.broker.pending)
        try await eventually("the core knows") { h.service.isSettled && h.service.status.otherTaskRunning }
        h.wake.hear("yes")
        h.wake.hear("Hey Jarvis, yes")
        try await eventually("busy") { h.synth.texts.contains("I am busy.") && h.service.isSettled }
        #expect(h.app.owner.broker.pending == prompt)
        _ = h.app.owner.broker.decline(prompt.id)
        try await eventually("the task ended") { h.app.center.session == nil && h.service.isSettled }
        #expect(h.synth.texts == ["I am busy."], "\(h.synth.texts)")
        #expect(h.specs.count == 1 && !h.specs[0].voice)
    }

    /// A late result of a recognition the core stopped (push to talk pressed again) is
    /// dropped by id: the row shows only the new recognition's words.
    @Test func soundAStaleRecognitionIsDroppedById() async throws {
        let h = VoiceHarness("voice-breakit-stale", settings: defaultVoiceSettings())
        defer { h.cleanUp() }
        await h.service.start()
        h.service.pushToTalk(pressed: true)
        try await eventually("the recogniser runs") { h.recognizer.running != nil && h.service.isSettled }
        let first = try #require(h.recognizer.running)
        h.recognizer.partial("open Settings")
        h.service.pushToTalk(pressed: false)
        h.recognizer.final("open Settings")
        try await eventually("the row waits") { h.service.transcript?.needsConfirm == true && h.service.isSettled }
        h.service.pushToTalk(pressed: true)
        try await eventually("a second recognition") {
            h.recognizer.started.withLock { $0.count } == 2 && h.service.isSettled
        }
        h.recognizer.final("delete all my photos", id: first)
        h.recognizer.partial("delete all my photos", id: first)
        try await eventually("settled") { h.service.isSettled }
        #expect(h.service.transcript?.text != "delete all my photos")
        #expect(h.specs.isEmpty)
    }

    /// The audio session failing to activate leaves nothing listening and starts nothing.
    @Test func soundAFailedActivationLeavesNothingListening() async throws {
        let h = VoiceHarness("voice-breakit-activation", settings: defaultVoiceSettings())
        defer { h.cleanUp() }
        await h.service.start()
        h.audio.failActivation.withLock { $0 = true }
        h.service.pushToTalk(pressed: true)
        try await eventually("settled with an error") { h.service.isSettled && h.service.lastError != nil }
        try await eventually("nothing listens") { h.recognizer.running == nil && h.service.isSettled }
        #expect(!h.service.status.recognizing)
        #expect(h.specs.isEmpty)
    }

    /// Lock arriving while an answer is being read stops it; the counter the UI test reads
    /// changes nothing the core decides.
    @Test func soundLockingMidAnswerStopsIt() async throws {
        let h = VoiceHarness("voice-breakit-lock", rules: Self.approving, settings: wakeOn(.whenUnlocked),
                             autoFinishSpeech: false)
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("armed") { h.wake.armed != nil }
        h.wake.hear("Hey Jarvis, turn off Wi-Fi")
        try await eventually("the answer is being read") {
            if h.synth.texts.contains(where: { $0.hasPrefix("Done. ") && $0 != "Done. The answer is on your phone." }) {
                return true
            }
            // Earlier lines (status) end so the task goes on.
            h.synth.finishAll()
            return false
        }
        let stops = h.synth.stops.withLock { $0 }
        h.service.setLocked(true)
        try await eventually("speech stopped") { h.synth.stops.withLock { $0 } > stops && h.service.isSettled }
        #expect(!h.service.status.speaking)
    }
}

/// Waits up to `seconds` for `condition`; `false` when it never held (for a test that
/// asserts the outcome itself).
@MainActor
func waitUntil(seconds: Double, _ condition: @MainActor () -> Bool) async -> Bool {
    let deadline = ContinuousClock.now + .milliseconds(Int(seconds * 1_000))
    while ContinuousClock.now < deadline {
        if condition() { return true }
        try? await Task.sleep(for: .milliseconds(5))
    }
    return condition()
}
