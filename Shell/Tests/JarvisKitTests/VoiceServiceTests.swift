import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

/// `VoiceService` over fakes of every port, the real core (`JarvisVoice`, the agent session)
/// and the demo backend: what the core decides is carried out, and the safety rules hold
/// end to end. Every wait is on a condition.
@MainActor
@Suite("Voice")
struct VoiceServiceTests {
    /// The owner approves the demo task's questions by touch.
    static let approving = ScriptedOwner.Rules(approve: [.firstUse, .systemSetting], decline: [], yes: true)
    /// The owner leaves questions on screen.
    static let leaving = ScriptedOwner.Rules(approve: [], decline: [], yes: nil)

    @Test func theWakeWordIsOffByDefaultAndPushToTalkStillWorks() async throws {
        let h = VoiceHarness("voice-default", settings: defaultVoiceSettings())
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("settled") { h.service.isSettled }
        #expect(h.wake.arms.withLock { $0 }.isEmpty)
        #expect(h.service.canPushToTalk)
        h.service.pushToTalk(pressed: true)
        try await eventually("the recogniser runs") { h.recognizer.running != nil && h.service.isSettled }
        #expect(h.synth.cues.withLock { $0 } == [.listening])
    }

    @Test func handsFreeStartsAVoiceTaskAndSaysShortLines() async throws {
        let h = VoiceHarness("voice-hands-free", rules: Self.approving, settings: wakeOn())
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("the wake word is armed") { h.wake.armed != nil }
        #expect(h.audio.activations.withLock { $0 } == 1)

        h.wake.hear("Hey Jarvis, turn off Wi-Fi")
        try await eventually("the task started") { h.specs.count == 1 }
        let spec = try #require(h.specs.first)
        #expect(spec.text == "turn off Wi-Fi")
        #expect(spec.voice && !spec.remote, "a hands-free task must reach the core as Voice")
        // The request was shown on the phone, as final, with no touch asked for.
        #expect(h.transcripts.contains(VoiceService.Transcript(text: "turn off Wi-Fi", isFinal: true,
                                                               needsConfirm: false, endpointed: false)),
                "\(h.transcripts)")
        try await eventually("the task ends") { h.app.center.session == nil && h.synth.texts.contains("Done.") }
        #expect(h.synth.texts.first == "On it.")
        // The answer stays on the phone (the setting is Never).
        #expect(!h.synth.texts.contains { $0.contains("Wi-Fi is off") }, "\(h.synth.texts)")
        #expect(!h.synth.texts.contains { $0.localizedCaseInsensitiveContains("jarvis") })
    }

    @Test func pushToTalkShowsTheTranscriptAndStartsOnlyOnTouchAsTheOwnersWords() async throws {
        let h = VoiceHarness("voice-ptt", rules: Self.approving, settings: defaultVoiceSettings())
        defer { h.cleanUp() }
        await h.service.start()
        h.service.pushToTalk(pressed: true)
        try await eventually("the recogniser runs") { h.recognizer.running != nil }
        h.recognizer.partial("turn off Wi-Fi")
        h.service.pushToTalk(pressed: false)
        try await eventually("the audio is finished") { !h.recognizer.finished.withLock { $0 }.isEmpty }
        h.recognizer.final("turn off Wi-Fi")
        try await eventually("the transcript waits for a touch") { h.service.transcript?.needsConfirm == true }
        #expect(h.service.transcript?.text == "turn off Wi-Fi")
        #expect(h.app.center.session == nil, "a push-to-talk transcript started without a touch")

        h.service.confirmTranscript("Turn off Wi-Fi")
        try await eventually("the task started") { h.specs.count == 1 }
        let spec = try #require(h.specs.first)
        #expect(spec.text == "Turn off Wi-Fi")
        #expect(!spec.voice && !spec.remote, "a transcript confirmed by touch is the owner's words")
        try await eventually("the task ends") { h.app.center.session == nil }
    }

    @Test func stopByVoiceDuringTheTask() async throws {
        let h = VoiceHarness("voice-stop", rules: Self.leaving, settings: wakeOn())
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("armed") { h.wake.armed != nil }
        h.wake.hear("Hey Jarvis, turn off Wi-Fi")
        try await eventually("a question is pending") { h.app.owner.broker.pending != nil }
        try await eventually("the core knows") {
            h.service.isSettled && h.service.status.phase == .taskRunning(questionPending: true)
        }
        try await eventually("the barge-in recogniser runs") { h.recognizer.running != nil }
        h.pass()
        h.recognizer.final("stop")
        try await eventually("the task stopped") { h.app.center.session == nil }
        try await eventually("Stopped. is said") { h.synth.texts.contains("Stopped.") }
    }

    @Test func aSpokenYesWhileAQuestionIsPendingApprovesNothing() async throws {
        let h = VoiceHarness("voice-yes", rules: Self.leaving, settings: wakeOn())
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("armed") { h.wake.armed != nil }
        h.wake.hear("Hey Jarvis, turn off Wi-Fi")
        try await eventually("a question is pending") { h.app.owner.broker.pending != nil }
        let prompt = try #require(h.app.owner.broker.pending)
        try await eventually("the approval line is said") {
            h.synth.texts.contains("I need your approval on the phone.")
                && h.service.status.phase == .taskRunning(questionPending: true)
        }
        // The line holds nothing of the question.
        if case .confirm(_, let request) = prompt {
            #expect(!h.synth.texts.contains { $0.contains(request.app) || $0.contains(request.summary) })
        }
        h.pass()
        for (n, text) in ["yes", "Yes, approve it", "Hey Jarvis, yes"].enumerated() {
            try await eventually("the barge-in recogniser runs") { h.recognizer.running != nil }
            h.recognizer.final(text)
            try await eventually("heard \(text)") { h.service.heardWhileQuestionPending == n + 1 && h.service.isSettled }
            h.pass()
        }
        #expect(h.app.owner.broker.pending == prompt, "the question was answered by voice")
        #expect(h.app.center.session != nil)
        // The owner declines by touch: that is the only way it ends.
        _ = h.app.owner.broker.decline(prompt.id)
        try await eventually("the task ends") { h.app.center.session == nil }
    }

    @Test func jarvisIgnoresWhatItHearsWhileItSpeaks() async throws {
        let h = VoiceHarness("voice-echo", rules: Self.leaving, settings: wakeOn(), autoFinishSpeech: false)
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("armed") { h.wake.armed != nil }
        h.wake.hear("Hey Jarvis, turn off Wi-Fi")
        try await eventually("both lines are being said") {
            h.synth.texts == ["On it.", "I need your approval on the phone."] && h.service.isSettled
        }
        try await eventually("the barge-in recogniser runs") { h.recognizer.running != nil && h.service.isSettled }
        // The room hears a request while Jarvis speaks: ignored (not even "I am busy.").
        h.recognizer.final("Hey Jarvis, delete my photos")
        try await eventually("settled") { h.service.isSettled }
        #expect(!h.synth.texts.contains("I am busy."))
        // After the speech and its tail the same words are answered "I am busy." (a task runs).
        h.synth.finishAll()
        try await eventually("speech ended") { h.service.isSettled && !h.service.status.speaking }
        h.pass()
        try await eventually("the barge-in recogniser runs") { h.recognizer.running != nil && h.service.isSettled }
        h.recognizer.final("Hey Jarvis, delete my photos")
        try await eventually("busy") { h.synth.texts.contains("I am busy.") }
        #expect(h.specs.count == 1)
    }

    @Test func anInterruptionDiscardsListeningAndResumesWhenTold() async throws {
        let h = VoiceHarness("voice-interrupt", settings: wakeOn())
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("armed") { h.wake.armed != nil }
        h.wake.hear("Hey Jarvis")
        try await eventually("listening for the request") { h.recognizer.running != nil && h.service.isSettled }
        let cut = try #require(h.recognizer.running)
        h.recognizer.partial("turn off")
        h.audio.interrupt()
        try await eventually("the recogniser stopped") { h.service.isSettled && h.service.status.interrupted }
        #expect(h.recognizer.stopped.withLock { $0 }.contains(cut))
        #expect(h.wake.armed == nil)
        // A late result of the cut exchange starts nothing.
        h.recognizer.final("turn off Wi-Fi", id: cut)
        h.pass(10_000)
        try await eventually("settled") { h.service.isSettled }
        #expect(h.specs.isEmpty)
        let before = h.wake.arms.withLock { $0.count }
        h.audio.endInterruption(shouldResume: true)
        try await eventually("armed again") { h.wake.arms.withLock { $0.count } == before + 1 && h.wake.armed != nil }
    }

    @Test func theAnswerIsNotSpokenWhenLocked() async throws {
        for locked in [true, false] {
            let h = VoiceHarness("voice-locked-\(locked)", rules: Self.leaving, settings: wakeOn(.whenUnlocked))
            await h.service.start()
            try await eventually("armed") { h.wake.armed != nil }
            h.wake.hear("Hey Jarvis, turn off Wi-Fi")
            try await eventually("a question is pending") { h.app.owner.broker.pending != nil }
            h.service.setLocked(locked)
            try await eventually("settled") { h.service.isSettled }
            // The owner answers every question by touch until the task ends.
            try await eventually("the task ends") {
                if let pending = h.app.owner.broker.pending { _ = h.app.owner.broker.approve(pending.id) }
                return h.app.center.session == nil
            }
            try await eventually("the end is said") { h.synth.texts.contains { $0.hasPrefix("Done.") } }
            let end = try #require(h.synth.texts.last { $0.hasPrefix("Done.") })
            if locked {
                #expect(end == "Done. The answer is on your phone.")
            } else {
                #expect(end == "Done. Wi-Fi is off")
            }
            h.cleanUp()
        }
    }

    @Test func aRequestWhileAnotherTaskRunsIsBusyAndStartsNothing() async throws {
        let h = VoiceHarness("voice-busy", rules: Self.leaving, settings: wakeOn())
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("armed") { h.wake.armed != nil }
        // The owner types a task on Ask.
        _ = h.app.center.start("Turn off Wi-Fi")
        try await eventually("the core knows another task runs") { h.service.isSettled && h.service.status.otherTaskRunning }
        h.wake.hear("Hey Jarvis, delete my account")
        try await eventually("busy") { h.synth.texts.contains("I am busy.") }
        #expect(h.specs.count == 1)
        // "Jarvis, stop" stops it (the safe direction).
        h.pass()
        h.wake.hear("Jarvis, stop")
        try await eventually("stopped") { h.app.center.session == nil }
    }

    @Test func voiceIsOffWhereOnDeviceRecognitionIsUnavailable() async throws {
        let h = VoiceHarness("voice-unavailable", settings: wakeOn(), available: false)
        defer { h.cleanUp() }
        await h.service.start()
        try await eventually("settled") { h.service.isSettled }
        #expect(!h.service.isAvailable)
        #expect(!h.service.canPushToTalk)
        #expect(h.wake.arms.withLock { $0 }.isEmpty)
        h.service.pushToTalk(pressed: true)
        try await eventually("settled") { h.service.isSettled }
        #expect(h.recognizer.started.withLock { $0 }.isEmpty)
    }

    @Test func settingsDecodeToTheQuietDefaults() throws {
        func decode(_ json: String) throws -> JarvisSettings {
            try JSONDecoder().decode(JarvisSettings.self, from: Data(json.utf8))
        }
        let old = try decode("{}")
        #expect(!old.voiceWakeWord && old.voiceSpeakAnswers == .never)
        #expect(old.voice == defaultVoiceSettings())
        let bad = try decode(#"{"voice_wake_word": "yes", "voice_speak_answers": "always"}"#)
        #expect(!bad.voiceWakeWord && bad.voiceSpeakAnswers == .never)
        let on = try decode(#"{"voice_wake_word": true, "voice_speak_answers": "when_unlocked"}"#)
        #expect(on.voice == VoiceSettings(wakeWord: true, speakAnswers: .whenUnlocked, confirmPushToTalk: true))
        let back = try decode(String(decoding: try JSONEncoder().encode(on), as: UTF8.self))
        #expect(back == on)
    }

    /// The UI test's hook (`-jarvis-voice-scripted`): the scripted ports start "open Settings"
    /// hands-free as a voice task, and the scripted "yes" that keeps arriving while the
    /// question is on screen approves nothing.
    @Test func theScriptedPortsOfTheUITestDriveTheRealPipeline() async throws {
        let app = AppHarness("voice-scripted", rules: Self.leaving)
        defer { app.cleanUp() }
        var specs: [TaskSpec] = []
        app.center.observe { event in
            if case .session(_, .update(.started(let spec))) = event { specs.append(spec) }
        }
        let service = VoiceService(
            ports: ScriptedVoice.ports(wakeLines: ScriptedVoice.uiTestWakeLines,
                                       recognizerLines: ScriptedVoice.uiTestRecognizerLines,
                                       delay: .milliseconds(20)),
            center: app.center, settings: wakeOn(), tickInterval: .milliseconds(20)
        )
        defer { service.stop() }
        await service.start()
        // The driver's events reach the main actor through a stream: the start may be seen
        // after the question is already pending.
        try await eventually("the task started and asks") { !specs.isEmpty && app.owner.broker.pending != nil }
        #expect(specs.map(\.text) == ["open Settings"])
        #expect(specs.first?.voice == true)
        let prompt = try #require(app.owner.broker.pending)
        try await eventually("scripted yeses arrived during the question", seconds: 20) {
            service.heardWhileQuestionPending >= 2
        }
        #expect(app.owner.broker.pending == prompt)
        #expect(app.center.session != nil)
        _ = app.owner.broker.decline(prompt.id)
        try await eventually("the task ends") { app.center.session == nil }
    }

    @Test func outcomesMapToTaskEvents() {
        #expect(VoiceService.taskEnd(Outcome(status: .success, summary: "Wi-Fi is off", steps: 2, durationMs: 1))
            == .finishedOk(answer: "Wi-Fi is off"))
        #expect(VoiceService.taskEnd(Outcome(status: .cancelled, summary: "", steps: 0, durationMs: 0)) == .stopped)
        #expect(VoiceService.taskEnd(Outcome(status: .blocked, summary: "x", steps: 0, durationMs: 0)) == .failed)
    }
}
