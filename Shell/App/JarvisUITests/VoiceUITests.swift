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

    /// M1: a long push-to-talk transcript (`-jarvis-voice-scripted-long`) is shown whole on
    /// the confirm row: the field holds the full text and grew past one line, so its end is
    /// on screen before Start.
    @MainActor
    func testALongPushToTalkTranscriptIsShownWhole() {
        let app = launchPlayground(extraArguments: ["-jarvis-voice-scripted-long"])

        // Playground mode opens on the Playground tab; the microphone is on Ask. (Mac run of
        // 5f28ad6: the test looked for the button on the Playground list and timed out.)
        let inBar = app.tabBars.buttons["Ask"]
        let ask = inBar.waitForExistence(timeout: 10) ? inBar : app.buttons["Ask"].firstMatch
        require(ask, "the Ask tab", timeout: 10, in: app)
        ask.tap()

        let mic = app.buttons["ask.mic"]
        require(mic, "the push-to-talk button", timeout: 30, in: app)
        mic.tap()

        // A vertical text field is a text view to XCUITest; look it up by identifier only.
        let field = app.descendants(matching: .any)["voice.transcript.text"]
        require(field, "the confirm row's text", timeout: 30, in: app)
        let full = "Open Notes, make a new note called shopping list, "
            + "and write milk, eggs, bread, two kinds of cheese, coffee beans, olive oil, tomatoes, "
            + "basil, rice and the batteries for the kitchen clock, then pin it to the top so I see "
            + "it first thing tomorrow morning"
        XCTAssertEqual(field.value as? String, full, "the row does not hold the whole transcript")
        // One line of this field is under 50 points tall (16 pt text, 10 pt padding each
        // side, 44 pt minimum); the long transcript needs several.
        XCTAssertGreaterThan(field.frame.height, 90, "the field did not grow past one line: \(field.frame)")
        let window = app.windows.firstMatch.frame
        XCTAssertLessThanOrEqual(field.frame.maxY, window.maxY, "the field's end is off screen: \(field.frame)")
        XCTAssertTrue(app.staticTexts["voice.transcript.count"].exists, "the character count is not shown")
        XCTAssertTrue(app.buttons["voice.transcript.start"].exists, "Start is not offered")
    }
}
