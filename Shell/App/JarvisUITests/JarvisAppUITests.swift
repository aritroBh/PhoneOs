import XCTest
import JarvisKit

/// Plain UI tests of the Jarvis app itself (no agent driving it).
final class JarvisAppUITests: QATestCase {
    @MainActor
    private func launch(_ arguments: [String]) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = arguments
        app.launch()
        return app
    }

    /// The tab bar button `title` (TabView exposes it in the tab bar; fall back to any
    /// button with that label in case a future tab bar is not reported as a TabBar).
    @MainActor
    private func tab(_ title: String, in app: XCUIApplication) -> XCUIElement {
        let inBar = app.tabBars.buttons[title]
        return inBar.waitForExistence(timeout: 5) ? inBar : app.buttons[title].firstMatch
    }

    /// The back button of the pushed screen (the root's bar is hidden, so it is the bar's
    /// first button).
    @MainActor
    private func back(in app: XCUIApplication) {
        let bar = app.navigationBars.firstMatch
        XCTAssertTrue(bar.waitForExistence(timeout: 5), "no navigation bar on the pushed screen")
        bar.buttons.element(boundBy: 0).tap()
    }

    /// A toolbar button by identifier, or by its title when the identifier did not reach
    /// the bar button.
    @MainActor
    private func toolbarButton(_ identifier: String, label: String, in app: XCUIApplication) -> XCUIElement {
        let byId = app.buttons[identifier]
        if byId.waitForExistence(timeout: 3) { return byId }
        let byLabel = app.navigationBars.buttons[label]
        XCTAssertTrue(byLabel.waitForExistence(timeout: 3), "neither \(identifier) nor a bar button \"\(label)\" exists")
        return byLabel
    }

    @MainActor
    func testLaunchShowsAsk() {
        let app = launch(["-jarvis-reset"])
        require(app.staticTexts["ask.prompt"], "the Ask prompt", in: app)
        require(app.textFields["ask.composer"], "the composer", in: app)
        XCTAssertTrue(app.buttons["ask.send"].exists)
        XCTAssertFalse(app.buttons["ask.send"].isEnabled, "send is enabled with an empty task")
        require(app.descendants(matching: .any)["status.pill"].firstMatch, "the status pill", in: app)
        XCTAssertTrue(app.descendants(matching: .any)["status.pill"].firstMatch.label.contains("Standing by"))
    }

    @MainActor
    func testEachTabRenders() {
        let app = launch(["-jarvis-reset"])
        require(app.staticTexts["ask.prompt"], "the Ask screen", in: app)

        tab("Skills", in: app).tap()
        require(app.staticTexts["skills.title"], "the Skills title", in: app)
        require(app.descendants(matching: .any)["skills.empty"].firstMatch, "the empty Skills message", in: app)
        XCTAssertTrue(app.buttons["skills.new"].exists, "no New skill button")
        XCTAssertTrue(app.buttons["skills.filter.learned"].exists, "no kind filter")

        tab("Workflows", in: app).tap()
        require(app.staticTexts["workflows.title"], "the Workflows title", in: app)
        require(app.descendants(matching: .any)["workflows.empty"].firstMatch, "the empty Workflows message", in: app)
        XCTAssertTrue(app.buttons["workflows.new"].exists, "no New workflow button")

        tab("Apps", in: app).tap()
        require(app.staticTexts["apps.title"], "the Apps title", in: app)
        require(app.buttons["apps.row.com.apple.Preferences"], "the Settings trust row", in: app)
        let ways = app.descendants(matching: .any)["apps.ways.com.apple.Preferences"].firstMatch
        XCTAssertTrue(ways.exists, "the Settings row has no ways-in icons")
        XCTAssertEqual(ways.label, "Ways in: screen", "Settings should be screen-only in this build")
        app.buttons["apps.row.com.apple.Preferences"].tap()
        require(app.descendants(matching: .any)["apps.trust.com.apple.Preferences"].firstMatch, "the trust level control", in: app)

        tab("Control", in: app).tap()
        require(app.staticTexts["control.title"], "the Control title", in: app)
        require(app.buttons["rules.mode.guarded"], "the Guarded mode control", in: app)
        require(app.switches["rules.firstUse"], "the first-use switch", in: app)
        XCTAssertTrue(app.buttons["control.remote"].exists, "no Remote control row")
        XCTAssertTrue(app.buttons["control.activity"].exists, "no Activity row")
        XCTAssertTrue(app.buttons["control.away"].exists, "no Away mode row")

        app.buttons["control.remote"].tap()
        let remoteSwitch = app.switches["remote.enabled"]
        require(remoteSwitch, "the Remote control switch", in: app)
        XCTAssertEqual(remoteSwitch.value as? String, "0", "remote control must be off by default")

        tab("Ask", in: app).tap()
        require(app.staticTexts["ask.prompt"], "the Ask screen again", in: app)
    }

    /// Activity is no longer a tab: it opens from the history button on Ask and from the
    /// Activity row in Control.
    @MainActor
    func testActivityOpensFromAskAndControl() {
        let app = launch(["-jarvis-reset"])
        require(app.buttons["ask.history"], "the history button on Ask", in: app)
        app.buttons["ask.history"].tap()
        require(app.staticTexts["activity.title"], "Activity opened from Ask", in: app)
        require(app.staticTexts["activity.empty"], "the empty Activity message", in: app)
        back(in: app)
        require(app.staticTexts["ask.prompt"], "Ask after going back", in: app)

        tab("Control", in: app).tap()
        require(app.buttons["control.activity"], "the Activity row in Control", in: app)
        app.buttons["control.activity"].tap()
        require(app.staticTexts["activity.title"], "Activity opened from Control", in: app)
    }

    /// A Written skill made in the form shows in the list, and its phrase on Ask runs the
    /// task with it ("Using skill: …" in the run view).
    @MainActor
    func testWrittenSkillRunsFromAsk() {
        let app = launch(["-jarvis-playground", "-jarvis-reset"])
        tab("Skills", in: app).tap()
        require(app.buttons["skills.new"], "the New skill button", in: app)
        app.buttons["skills.new"].tap()

        let name = app.textFields["skill.edit.name"]
        require(name, "the skill name field", in: app)
        name.tap()
        name.typeText("Lights out")
        let phrase = app.textFields["skill.edit.trigger.0"]
        require(phrase, "the first phrase field", in: app)
        phrase.tap()
        phrase.typeText("lights out now")
        toolbarButton("skill.edit.save", label: "Save", in: app).tap()

        let row = app.buttons["skills.row.lights-out"]
        require(row, "the new skill in the list", in: app)
        XCTAssertTrue(row.label.contains("Lights out"), "row label: \(row.label)")

        tab("Ask", in: app).tap()
        let composer = app.textFields["ask.composer"]
        require(composer, "the composer", in: app)
        composer.tap()
        composer.typeText("lights out now")
        app.buttons["ask.send"].tap()

        let using = app.descendants(matching: .any)["run.skill"].firstMatch
        require(using, "the Using skill line in the run view", timeout: 15, in: app)
        XCTAssertEqual(using.label, "Using skill: Lights out")
        let approve = app.buttons["confirm.approve"]
        require(approve, "the first-use confirmation", timeout: 20, in: app)
        approve.tap()
        let status = app.staticTexts["run.status"]
        require(status, "the run result", timeout: 30, in: app)
        XCTAssertEqual(status.label, "Done")
    }

    /// Saving without a name shows the core's own message next to the name field.
    @MainActor
    func testSkillFormShowsCoreValidationInline() {
        let app = launch(["-jarvis-reset"])
        tab("Skills", in: app).tap()
        require(app.buttons["skills.new"], "the New skill button", in: app)
        app.buttons["skills.new"].tap()
        require(app.textFields["skill.edit.name"], "the skill form", in: app)
        toolbarButton("skill.edit.save", label: "Save", in: app).tap()
        let error = app.descendants(matching: .any)["skill.edit.error.name"].firstMatch
        require(error, "the inline error next to the name", in: app)
        XCTAssertTrue(error.label.contains("name: must not be empty"), "inline error: \(error.label)")
        XCTAssertTrue(app.textFields["skill.edit.name"].exists, "the form closed on an invalid skill")
    }

    /// Two skills share "network off": Ask offers both and "Just do what I said", which runs
    /// the words without a skill.
    @MainActor
    func testAmbiguousPhraseShowsChooser() {
        let app = launch(["-jarvis-playground", "-jarvis-reset", "-jarvis-seed-ambiguous-skills"])
        tab("Ask", in: app).tap()
        let composer = app.textFields["ask.composer"]
        require(composer, "the composer", in: app)
        composer.tap()
        composer.typeText("network off")
        app.buttons["ask.send"].tap()

        require(app.staticTexts["skill.choice.title"], "the skill chooser", timeout: 10, in: app)
        XCTAssertTrue(app.buttons["skill.choice.network-off-home"].exists, "the first candidate is missing")
        XCTAssertTrue(app.buttons["skill.choice.network-off-work"].exists, "the second candidate is missing")
        let plain = app.buttons["skill.choice.plain"]
        XCTAssertTrue(plain.exists, "no Just do what I said")
        plain.tap()

        let approve = app.buttons["confirm.approve"]
        require(approve, "the run after Just do what I said (first-use confirmation)", timeout: 20, in: app)
        XCTAssertFalse(app.descendants(matching: .any)["run.skill"].firstMatch.exists,
                       "Just do what I said ran with a skill")
        approve.tap()
        require(app.staticTexts["run.status"], "the run result", timeout: 30, in: app)
    }

    /// A skills file with an entry the core cannot load: the banner lists the problems.
    @MainActor
    func testSkillLoadIssueBanner() {
        let app = launch(["-jarvis-reset", "-jarvis-seed-damaged-skills"])
        tab("Skills", in: app).tap()
        require(app.descendants(matching: .any)["skills.issues"].firstMatch, "the load-issue banner", in: app)
        let title = app.staticTexts["skills.issues.title"]
        XCTAssertEqual(title.label, "2 problems loading skills")
        let issues = [app.staticTexts["skills.issue.0"].label, app.staticTexts["skills.issue.1"].label]
        XCTAssertTrue(issues.contains { $0.contains("broken-entry") }, "issues shown: \(issues)")
        XCTAssertTrue(issues.contains { $0.contains("from_the_future") }, "issues shown: \(issues)")
    }

    /// Away mode: the first time explains the costs; accepting shows the black screen; a tap
    /// does not leave; a 2.5 s touch-and-hold does. The second time goes straight in (from
    /// Ask this time).
    @MainActor
    func testAwayModeEnterAndLongPressExit() {
        let app = launch(["-jarvis-reset"])
        tab("Control", in: app).tap()
        require(app.buttons["control.away"], "the Away mode row", in: app)
        app.buttons["control.away"].tap()

        require(app.staticTexts["away.explain.title"], "the Away mode explanation", in: app)
        XCTAssertTrue(app.descendants(matching: .any)["away.explain.costs"].firstMatch.exists, "the costs are not shown")
        XCTAssertTrue(app.switches["away.proximity"].exists, "the proximity option is missing")
        XCTAssertEqual(app.switches["away.proximity"].value as? String, "0", "proximity must be off by default")
        app.buttons["away.acknowledge"].tap()

        let screen = app.descendants(matching: .any)["away.screen"].firstMatch
        require(screen, "the black Away screen", in: app)
        screen.tap()
        XCTAssertFalse(screen.waitForNonExistence(timeout: 2), "a plain tap left Away mode")
        screen.press(forDuration: 2.5)
        XCTAssertTrue(screen.waitForNonExistence(timeout: 5), "touch and hold for 2.5 s did not leave Away mode")
        require(app.staticTexts["control.title"], "Control after leaving Away mode", in: app)

        tab("Ask", in: app).tap()
        require(app.buttons["ask.away"], "the Away button on Ask", in: app)
        app.buttons["ask.away"].tap()
        require(screen, "the black Away screen without the explanation the second time", in: app)
        XCTAssertFalse(app.staticTexts["away.explain.title"].exists, "the explanation came back after it was accepted")
        screen.press(forDuration: 2.5)
        XCTAssertTrue(screen.waitForNonExistence(timeout: 5), "touch and hold did not leave Away mode the second time")
    }

    /// The demo backend (DemoPhone + scripted model) in playground mode: the run view shows
    /// the step log, the confirmation sheet appears and approving it lets the task finish.
    @MainActor
    func testDemoTaskShowsStepLogAndConfirmationSheet() {
        let app = launch(["-jarvis-playground", "-jarvis-reset"])
        tab("Ask", in: app).tap()
        let composer = app.textFields["ask.composer"]
        require(composer, "the composer", in: app)
        composer.tap()
        composer.typeText("Turn off Wi-Fi")
        app.buttons["ask.send"].tap()

        // First use of the demo app asks once.
        let approve = app.buttons["confirm.approve"]
        require(approve, "the confirmation sheet", timeout: 20, in: app)
        let category = app.descendants(matching: .any)["confirm.category"].firstMatch
        XCTAssertTrue(category.exists, "the sheet has no category chip")
        XCTAssertEqual(category.label, "First time in this app")
        XCTAssertTrue(app.buttons["confirm.decline"].exists)
        XCTAssertTrue(app.staticTexts["confirm.app"].label.contains(DemoPhone.bundleId), "the sheet does not name the app")
        approve.tap()

        let status = app.staticTexts["run.status"]
        require(status, "the run result", timeout: 30, in: app)
        XCTAssertEqual(status.label, "Done")
        XCTAssertEqual(app.staticTexts["run.result"].label, "Wi-Fi is off")
        XCTAssertTrue(app.descendants(matching: .any)["run.step.1"].exists, "the step log has no first step")
        XCTAssertTrue(app.descendants(matching: .any)["run.step.2"].exists, "the step log has no second step")
        XCTAssertTrue(app.buttons["run.newTask"].exists)
    }

    /// The Live view on the run screen: a demo task plays in it (the viewer's value is its
    /// caption), the caption changes as the run goes, and the replay controls step back to
    /// an earlier step, forward again, and back to live.
    @MainActor
    func testLiveViewFollowsAndReplaysDemoTask() {
        let app = launch(["-jarvis-playground", "-jarvis-reset"])
        tab("Ask", in: app).tap()
        let composer = app.textFields["ask.composer"]
        require(composer, "the composer", in: app)
        composer.tap()
        composer.typeText("Turn off Wi-Fi")
        app.buttons["ask.send"].tap()

        // The viewer is on the run screen from the start (it may be hidden from the
        // accessibility tree while the confirmation sheet is up, so it is only sampled).
        let viewer = app.descendants(matching: .any)["run.live"].firstMatch
        var captions: [String] = []
        func noteCaption() {
            guard viewer.exists, let value = viewer.value as? String, !value.isEmpty, captions.last != value else { return }
            captions.append(value)
        }
        noteCaption()

        // First use of the demo app asks once; the viewer keeps going behind the sheet.
        let approve = app.buttons["confirm.approve"]
        let askDeadline = Date().addingTimeInterval(20)
        while !approve.exists && Date() < askDeadline {
            noteCaption()
            RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        }
        require(approve, "the first-use confirmation", timeout: 5, in: app)
        approve.tap()
        require(viewer, "the Live view", timeout: 10, in: app)

        let status = app.staticTexts["run.status"]
        let doneDeadline = Date().addingTimeInterval(30)
        while !status.exists && Date() < doneDeadline {
            noteCaption()
            RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        }
        require(status, "the run result", timeout: 5, in: app)
        XCTAssertEqual(status.label, "Done")
        // Let the last step finish playing, then read the final caption.
        RunLoop.current.run(until: Date().addingTimeInterval(1.5))
        noteCaption()
        XCTAssertGreaterThan(Set(captions).count, 1, "the Live view's caption never changed: \(captions)")
        let finalCaption = viewer.value as? String
        XCTAssertEqual(finalCaption, "Done")

        let back = app.buttons["run.live.back"]
        require(back, "the step-back button", in: app)
        if !back.isHittable { app.swipeUp() }
        back.tap()
        let atFirst = NSPredicate(format: "value CONTAINS %@", "Step 1 of 2")
        expectation(for: atFirst, evaluatedWith: viewer)
        waitForExpectations(timeout: 5)
        XCTAssertNotEqual(viewer.value as? String, finalCaption, "stepping back did not change the caption")
        XCTAssertTrue((viewer.value as? String)?.hasPrefix("Tap") == true, "the first step is not a tap: \(String(describing: viewer.value))")

        app.buttons["run.live.forward"].tap()
        let atSecond = NSPredicate(format: "value CONTAINS %@", "Step 2 of 2")
        expectation(for: atSecond, evaluatedWith: viewer)
        waitForExpectations(timeout: 5)

        XCTAssertTrue(app.buttons["run.live.play"].exists, "no play button")
        XCTAssertTrue(app.descendants(matching: .any)["run.live.scrubber"].firstMatch.exists, "no scrubber")
        app.buttons["run.live.followLive"].tap()
        let live = NSPredicate(format: "value == %@", "Done")
        expectation(for: live, evaluatedWith: viewer)
        waitForExpectations(timeout: 5)

        // The element list is one tap away.
        let elements = app.buttons["run.view.elements"]
        require(elements, "the Screen / Elements switch", in: app)
        if !elements.isHittable { app.swipeDown() }
        elements.tap()
        require(app.descendants(matching: .any)["run.sight"].firstMatch, "the element list", in: app)
    }

    /// A demo task that asks for approval: the sheet shows the screen Jarvis evaluated with
    /// the target outlined (the demo phone has no screenshots, so its layout), above the
    /// question; the owner approves each question and the run finishes.
    @MainActor
    func testApprovalSheetShowsTheScreenWithTheTarget() {
        let app = launch(["-jarvis-playground", "-jarvis-reset"])
        tab("Ask", in: app).tap()
        let composer = app.textFields["ask.composer"]
        require(composer, "the composer", in: app)
        composer.tap()
        composer.typeText("Delete my account")
        app.buttons["ask.send"].tap()

        // First use of the demo app, then the delete itself: both are asked with a picture.
        let approve = app.buttons["confirm.approve"]
        let status = app.staticTexts["run.status"]
        var asked = 0
        let deadline = Date().addingTimeInterval(60)
        while !status.exists && Date() < deadline {
            if approve.waitForExistence(timeout: 1) {
                let preview = app.descendants(matching: .any)["confirm.preview"].firstMatch
                require(preview, "the approval preview", timeout: 10, in: app)
                XCTAssertTrue(preview.label.hasPrefix("Outlined:"), "the preview does not name the target: \(preview.label)")
                XCTAssertTrue(app.descendants(matching: .any)["confirm.preview.caption"].firstMatch.exists, "no caption")
                XCTAssertTrue(approve.isHittable, "Approve is not reachable without scrolling")
                approve.tap()
                asked += 1
                // Let this sheet go before looking for the next question.
                let gone = Date().addingTimeInterval(5)
                while approve.exists && Date() < gone {
                    RunLoop.current.run(until: Date().addingTimeInterval(0.1))
                }
            }
        }
        XCTAssertGreaterThan(asked, 0, "no approval was asked")
        require(status, "the run result", timeout: 30, in: app)
        XCTAssertEqual(status.label, "Done")
    }

    /// The demo phone starts locked: the run view shows "Unlock to continue" with the time
    /// waited, then the phone "unlocks", the run asks for first use and finishes.
    @MainActor
    func testRunViewShowsUnlockToContinue() {
        let app = launch(["-jarvis-playground", "-jarvis-reset", "-jarvis-demo-locked"])
        tab("Ask", in: app).tap()
        let composer = app.textFields["ask.composer"]
        require(composer, "the composer", in: app)
        composer.tap()
        composer.typeText("Turn off Wi-Fi")
        app.buttons["ask.send"].tap()

        let lockTitle = app.staticTexts["run.lock.title"]
        require(lockTitle, "the Unlock to continue card", timeout: 15, in: app)
        XCTAssertEqual(lockTitle.label, "Unlock to continue")
        XCTAssertTrue(app.staticTexts["run.lock.waited"].label.hasPrefix("Waiting "))
        XCTAssertTrue(app.descendants(matching: .any)["status.pill"].firstMatch.label.contains("Unlock to continue"))

        let approve = app.buttons["confirm.approve"]
        require(approve, "the first-use confirmation after the unlock", timeout: 30, in: app)
        XCTAssertFalse(lockTitle.exists, "the lock card stayed after the unlock")
        approve.tap()
        let status = app.staticTexts["run.status"]
        require(status, "the run result", timeout: 30, in: app)
        XCTAssertEqual(status.label, "Done")
    }
}
