import XCTest
import JarvisKit

/// Workflows and Remote control in the app (playground backend; no network: remote control
/// uses the in-memory transport there).
final class WorkflowRemoteUITests: QATestCase {
    @MainActor
    private func launch(_ arguments: [String]) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = arguments
        app.launch()
        return app
    }

    @MainActor
    private func tab(_ title: String, in app: XCUIApplication) -> XCUIElement {
        let inBar = app.tabBars.buttons[title]
        return inBar.waitForExistence(timeout: 5) ? inBar : app.buttons[title].firstMatch
    }

    @MainActor
    private func toolbarButton(_ identifier: String, label: String, in app: XCUIApplication) -> XCUIElement {
        let byId = app.buttons[identifier]
        if byId.waitForExistence(timeout: 3) { return byId }
        let byLabel = app.navigationBars.buttons[label]
        XCTAssertTrue(byLabel.waitForExistence(timeout: 3), "neither \(identifier) nor a bar button \"\(label)\" exists")
        return byLabel
    }

    /// In playground builds Workflows is a row in Control.
    @MainActor
    private func openWorkflows(in app: XCUIApplication) {
        tab("Control", in: app).tap()
        let row = app.buttons["control.workflows"]
        require(row, "the Workflows row in Control", in: app)
        row.tap()
        require(app.staticTexts["workflows.title"], "the Workflows screen", in: app)
    }

    /// Types into the field. `endEditing`: then presses Return, as a person does before
    /// moving on, and waits until the keyboard is gone, so the next control is tapped on a
    /// settled screen with nothing over it.
    @MainActor
    private func type(_ text: String, into identifier: String, endEditing: Bool = false, in app: XCUIApplication) {
        let field = app.textFields[identifier]
        require(field, "the field \(identifier)", in: app)
        bringAboveKeyboard(field, in: app)
        XCTAssertTrue(focus(field), "the field \(identifier) never took the keyboard focus\n\(focusReport(field, in: app))")
        field.typeText(text)
        guard endEditing else { return }
        field.typeText("\n")
        // Not an assertion: where Return keeps the keyboard up, the next field is still
        // brought above it (bringAboveKeyboard) before it is tapped.
        _ = app.keyboards.firstMatch.waitForNonExistence(timeout: 5)
    }

    /// What the next run needs to see when a field does not take the focus: the field's
    /// frame and hittability, the keyboard's frame and the editor's text fields.
    @MainActor
    private func focusReport(_ field: XCUIElement, in app: XCUIApplication) -> String {
        let keyboard = app.keyboards.firstMatch
        var lines = [
            "field exists \(field.exists), hittable \(field.exists && field.isHittable), frame \(field.exists ? "\(field.frame)" : "-")",
            "keyboard " + (keyboard.exists ? "up, frame \(keyboard.frame)" : "not shown"),
        ]
        let readsFocus = field.responds(to: NSSelectorFromString("hasKeyboardFocus"))
        lines += app.textFields.allElementsBoundByIndex.map { other in
            let focus = readsFocus ? String(describing: other.value(forKey: "hasKeyboardFocus")) : "unreadable"
            return "text field \(other.identifier) frame \(other.frame) focus \(focus)"
        }
        let shot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        shot.name = "focus-failure"
        shot.lifetime = .keepAlways
        add(shot)
        return lines.joined(separator: "\n")
    }

    /// Taps `field` until it has the keyboard focus. A tap made while the editor is still
    /// redrawing (a step's kind was just chosen, which replaces its text field) can leave
    /// the focus nowhere; `typeText` then fails the test at once (simulator runs 9 and 10).
    @MainActor
    private func focus(_ field: XCUIElement) -> Bool {
        let readsFocus = field.responds(to: NSSelectorFromString("hasKeyboardFocus"))
        for _ in 0..<4 {
            guard field.waitForExistence(timeout: 3) else { return false }
            field.tap()
            guard readsFocus else {
                // No way to ask: give the editor time to settle after the tap.
                RunLoop.current.run(until: Date().addingTimeInterval(0.8))
                return true
            }
            let deadline = Date().addingTimeInterval(1.5)
            while Date() < deadline {
                if (field.value(forKey: "hasKeyboardFocus") as? Bool) == true { return true }
                RunLoop.current.run(until: Date().addingTimeInterval(0.1))
            }
        }
        return false
    }

    /// Scrolls the editor until `field` is clear of the keyboard. A field that appears while
    /// the keyboard is up can lie under it; a tap there lands on the keyboard and the field
    /// never gets the focus (simulator run 9: "Neither element nor any descendant has
    /// keyboard focus").
    @MainActor
    private func bringAboveKeyboard(_ field: XCUIElement, in app: XCUIApplication) {
        let keyboard = app.keyboards.firstMatch
        for _ in 0..<4 {
            guard keyboard.exists, keyboard.frame.height > 0,
                  field.frame.maxY > keyboard.frame.minY - 12 else { return }
            let from = app.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0))
                .withOffset(CGVector(dx: app.frame.midX, dy: max(140, keyboard.frame.minY - 60)))
            let to = from.withOffset(CGVector(dx: 0, dy: -180))
            from.press(forDuration: 0.05, thenDragTo: to)
        }
    }

    /// Makes a workflow in the editor: `steps` are (kind, text) for each step.
    @MainActor
    private func createWorkflow(_ name: String, phrase: String? = nil, steps: [(String, String)], in app: XCUIApplication) {
        app.buttons["workflows.new"].tap()
        type(name, into: "workflow.edit.name", endEditing: true, in: app)
        if let phrase {
            app.buttons["workflow.edit.trigger.phrase"].tap()
            type(phrase, into: "workflow.edit.phrase", endEditing: true, in: app)
        }
        for (index, step) in steps.enumerated() {
            if index > 0 {
                let add = app.buttons["workflow.edit.addStep"]
                if !add.isHittable { app.swipeUp() }
                add.tap()
            }
            let kind = app.buttons["workflow.edit.step.\(index).kind.\(step.0)"]
            require(kind, "the \(step.0) kind of step \(index + 1)", in: app)
            if !kind.isHittable { app.swipeUp() }
            kind.tap()
            // The chip shows the kind as chosen once the step has been redrawn.
            let deadline = Date().addingTimeInterval(5)
            while !kind.isSelected && Date() < deadline {
                RunLoop.current.run(until: Date().addingTimeInterval(0.1))
            }
            XCTAssertTrue(kind.isSelected, "the \(step.0) kind of step \(index + 1) was not shown as chosen")
            type(step.1, into: "workflow.edit.step.\(index).text", endEditing: true, in: app)
        }
        toolbarButton("workflow.edit.save", label: "Save", in: app).tap()
    }

    /// A two-step workflow made in the editor runs from Run now: both steps show, the
    /// confirmation still comes up for the task step, the outcome is recorded.
    @MainActor
    func testTwoStepWorkflowRunsFromRunNow() {
        let app = launch(["-jarvis-playground", "-jarvis-reset"])
        openWorkflows(in: app)
        createWorkflow("Evening", steps: [("runTask", "Turn off Wi-Fi"), ("notify", "Wi-Fi is off now")], in: app)

        let row = app.buttons["workflows.row.evening"]
        require(row, "the new workflow in the list", in: app)
        XCTAssertTrue(row.label.contains("2 steps"), "row label: \(row.label)")
        row.tap()
        let run = app.buttons["workflow.detail.run"]
        require(run, "Run now", in: app)
        run.tap()

        require(app.staticTexts["workflow.run.title"], "the workflow run view", timeout: 10, in: app)
        XCTAssertTrue(app.descendants(matching: .any)["workflow.run.step.0"].exists, "the first step is not shown")
        XCTAssertTrue(app.descendants(matching: .any)["workflow.run.step.1"].exists, "the second step is not shown")
        let approve = app.buttons["confirm.approve"]
        require(approve, "the first-use confirmation inside the workflow", timeout: 20, in: app)
        approve.tap()

        let status = app.staticTexts["workflow.run.status"]
        require(status, "the workflow outcome", timeout: 30, in: app)
        XCTAssertEqual(status.label, "Done")
        XCTAssertTrue(app.descendants(matching: .any)["workflow.run.step.1"].firstMatch.label.contains("done"),
                      "step 2: \(app.descendants(matching: .any)["workflow.run.step.1"].firstMatch.label)")
        XCTAssertTrue(app.staticTexts["workflow.run.note.0"].exists || app.descendants(matching: .any)["workflow.run.note.0"].exists,
                      "the notification text is not shown")
        app.buttons["workflow.run.done"].tap()
        let last = app.descendants(matching: .any)["workflow.detail.lastOutcome"].firstMatch
        require(last, "the recorded last outcome on the workflow's page", in: app)
        XCTAssertTrue(last.label.contains("Done"), "last outcome: \(last.label)")
    }

    /// Saying a workflow's phrase on Ask runs the workflow (resolveUtterance).
    @MainActor
    func testPhraseRunsWorkflowFromAsk() {
        let app = launch(["-jarvis-playground", "-jarvis-reset"])
        openWorkflows(in: app)
        createWorkflow("Goodnight", phrase: "goodnight jarvis", steps: [("notify", "Sleep well")], in: app)
        require(app.buttons["workflows.row.goodnight"], "the new workflow in the list", in: app)

        tab("Ask", in: app).tap()
        type("goodnight jarvis", into: "ask.composer", in: app)
        app.buttons["ask.send"].tap()
        require(app.staticTexts["workflow.run.title"], "the workflow run view after the phrase", timeout: 10, in: app)
        XCTAssertEqual(app.staticTexts["workflow.run.title"].label, "Goodnight")
        let status = app.staticTexts["workflow.run.status"]
        require(status, "the workflow outcome", timeout: 20, in: app)
        XCTAssertEqual(status.label, "Done")
    }

    /// A scheduled workflow shows its next run and the note that schedules need Jarvis open.
    @MainActor
    func testScheduledWorkflowShowsNextRun() {
        let app = launch(["-jarvis-playground", "-jarvis-reset", "-jarvis-seed-scheduled-workflow"])
        openWorkflows(in: app)
        // The row is one button; its label carries the next run.
        let row = app.buttons["workflows.row.morning-note"]
        require(row, "the scheduled workflow", in: app)
        XCTAssertTrue(row.label.contains("Next run "), "row label: \(row.label)")
        XCTAssertTrue(row.label.contains("Every day at"), "row label: \(row.label)")
        XCTAssertTrue(app.descendants(matching: .any)["workflows.scheduleNote"].firstMatch.exists, "no note about schedules")
    }

    /// Remote control: on, Pair a device shows the code, the QR and the countdown; Cancel
    /// takes it away.
    @MainActor
    func testRemoteControlPairingShowsCodeAndQR() {
        let app = launch(["-jarvis-playground", "-jarvis-reset"])
        tab("Control", in: app).tap()
        require(app.buttons["control.remote"], "the Remote control row", in: app)
        app.buttons["control.remote"].tap()
        let toggle = app.switches["remote.enabled"]
        require(toggle, "the Remote control switch", in: app)
        XCTAssertEqual(toggle.value as? String, "0")
        toggle.tap()
        if (toggle.value as? String) == "0" {
            // Some iOS versions only toggle when the switch itself (trailing edge) is hit.
            toggle.coordinate(withNormalizedOffset: CGVector(dx: 0.92, dy: 0.5)).tap()
        }
        XCTAssertEqual(toggle.value as? String, "1", "remote control did not turn on")
        let pair = app.buttons["remote.pair"]
        require(pair, "Pair a device", in: app)
        XCTAssertTrue(app.descendants(matching: .any)["remote.noDevices"].firstMatch.exists, "no empty device list")
        pair.tap()

        let code = app.staticTexts["remote.pair.code"]
        require(code, "the pairing code", in: app)
        XCTAssertEqual(code.label.isEmpty, false)
        XCTAssertTrue(app.descendants(matching: .any)["remote.pair.qr"].firstMatch.exists, "no QR code")
        let countdown = app.staticTexts["remote.pair.countdown"]
        XCTAssertTrue(countdown.exists && countdown.label.hasPrefix("Expires in "), "countdown: \(countdown.label)")
        app.buttons["remote.pair.cancel"].tap()
        XCTAssertTrue(code.waitForNonExistence(timeout: 5), "the code stayed on screen after Cancel")
        require(pair, "Pair a device again", in: app)
    }

    /// A device paired in-process (launch argument) is listed with what it may do and the
    /// fixed list of what can never be approved remotely.
    @MainActor
    func testPairedDeviceRowWithPermissions() {
        let app = launch(["-jarvis-playground", "-jarvis-reset", "-jarvis-seed-remote-device"])
        tab("Control", in: app).tap()
        require(app.buttons["control.remote"], "the Remote control row", in: app)
        app.buttons["control.remote"].tap()
        let device = app.descendants(matching: .any)["remote.device.0"].firstMatch
        require(device, "the paired device", timeout: 20, in: app)
        XCTAssertTrue(device.label.contains("Test Mac"), "device row: \(device.label)")
        let start = app.switches["remote.device.0.startTasks"]
        require(start, "the Start and stop tasks switch", in: app)
        XCTAssertEqual(start.value as? String, "1", "a new device may start tasks")
        XCTAssertEqual(app.switches["remote.device.0.approve"].value as? String, "0", "a new device must not approve")
        let never = app.staticTexts["remote.device.0.neverRemote"]
        if !never.exists { app.swipeUp() }
        require(never, "the never-remote note", in: app)
        XCTAssertTrue(never.label.lowercased().contains("paying"), "never-remote note: \(never.label)")
        XCTAssertTrue(never.label.lowercased().contains("deleting"), "never-remote note: \(never.label)")
    }

    @MainActor
    func testWatchScreenLayoutPermission() {
        let app = launch(["-jarvis-playground", "-jarvis-reset", "-jarvis-seed-remote-device"])
        tab("Control", in: app).tap()
        require(app.buttons["control.remote"], "the Remote control row", in: app)
        app.buttons["control.remote"].tap()
        let device = app.descendants(matching: .any)["remote.device.0"].firstMatch
        require(device, "the paired device", timeout: 20, in: app)
        let watch = app.switches["remote.device.0.watchScreen"]
        if !watch.exists { app.swipeUp() }
        require(watch, "the Watch the screen layout switch", in: app)
        XCTAssertEqual(watch.value as? String, "0", "watching the screen layout must be off for a new device")
        let note = app.staticTexts["remote.device.0.watchScreenNote"]
        if !note.isHittable { app.swipeUp() }
        require(note, "the Watch the screen layout footnote", in: app)
        XCTAssertTrue(note.label.contains("outlines of the screen, where it taps, and step captions"), "footnote: \(note.label)")
        XCTAssertTrue(note.label.contains("Never pictures of the screen, passwords or the lock screen"), "footnote: \(note.label)")
        watch.tap()
        if (watch.value as? String) == "0" {
            // Some iOS versions only toggle when the switch itself (trailing edge) is hit.
            watch.coordinate(withNormalizedOffset: CGVector(dx: 0.92, dy: 0.5)).tap()
        }
        let on = NSPredicate(format: "value == '1'")
        expectation(for: on, evaluatedWith: watch)
        waitForExpectations(timeout: 10)
        XCTAssertEqual(app.switches["remote.device.0.approve"].value as? String, "0", "watching must not turn approving on")
    }
}
