import XCTest

/// Voice through the whole pipeline in the app, with the scripted recogniser
/// (`-jarvis-voice-scripted`: the wake word hears "Hey Jarvis, open Settings" once, then the
/// recogniser hears "yes" every time it starts). The core reads the transcripts; the demo
/// backend runs the task.
final class VoiceUITests: QATestCase {
    /// "Hey Jarvis, open Settings" starts the task hands-free with the request shown, and the
    /// scripted "yes" heard while the first-use question is open approves nothing: the
    /// question stays until the owner taps.
    @MainActor
    func testHandsFreeRequestStartsAndASpokenYesApprovesNothing() {
        let app = launchPlayground(extraArguments: ["-jarvis-voice-scripted"])

        let title = app.staticTexts["run.title"]
        require(title, "the run view of the voice-started task", timeout: 30, in: app)
        XCTAssertEqual(title.label, "open Settings", "the request is not shown as heard")

        let approve = app.buttons["confirm.approve"]
        require(approve, "the first-use question", timeout: 30, in: app)
        // Wait until at least two scripted "yes" reached the core while the question was open.
        let heard = app.staticTexts["voice.qa.heard"]
        require(heard, "the voice QA counter", in: app)
        let twice = expectation(for: NSPredicate(format: "label MATCHES %@", "Heard while asking: ([2-9]|[1-9][0-9]+)"),
                                evaluatedWith: heard)
        wait(for: [twice], timeout: 30)

        // Still asking: a spoken word never approves.
        XCTAssertTrue(approve.exists, "the question went away after a spoken yes")
        let status = app.staticTexts["run.status"]
        XCTAssertFalse(status.exists && status.label == "Done", "the task finished on a spoken yes")

        // Only a touch answers it.
        app.buttons["confirm.decline"].tap()
        require(status, "the run result after declining", timeout: 30, in: app)
        XCTAssertNotEqual(status.label, "Done")
    }
}
