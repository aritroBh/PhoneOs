import XCTest
import JarvisKit

/// "Control another phone" in one app process: the phone side (remote control on, its
/// in-memory transport in playground mode) and the controller screen paired with it over
/// the in-process loopback (no network). The task runs on the playground's demo backend
/// (DemoPhone + scripted model), as every playground task does.
///
/// The pairing code is not read from the screen: the pairing sheet labels it "Pairing code"
/// for accessibility on purpose (the code stays out of the accessibility tree). The
/// playground-only "Pair with this phone (QA)" hands this phone's open offer to the
/// controller in-process instead.
final class ControllerUITests: QATestCase {
    @MainActor
    private func tab(_ title: String, in app: XCUIApplication) -> XCUIElement {
        let inBar = app.tabBars.buttons[title]
        return inBar.waitForExistence(timeout: 5) ? inBar : app.buttons[title].firstMatch
    }

    /// Swipes up until `element` is hittable (at most `tries` swipes).
    @MainActor
    private func scrollTo(_ element: XCUIElement, in app: XCUIApplication, tries: Int = 6) {
        var left = tries
        while left > 0, !(element.exists && element.isHittable) {
            app.swipeUp()
            left -= 1
        }
    }

    /// Taps a row that pushes a screen; one more tap if the first was synthesized while the
    /// list was still laying out (as `openRemoteControl` in `WorkflowRemoteUITests`).
    @MainActor
    private func push(_ row: XCUIElement, _ what: String, in app: XCUIApplication) {
        require(row, what, in: app)
        scrollTo(row, in: app)
        row.tap()
        if !row.waitForNonExistence(timeout: 5), row.isHittable { row.tap() }
    }

    @MainActor
    private func back(in app: XCUIApplication) {
        let button = app.navigationBars.buttons.element(boundBy: 0)
        XCTAssertTrue(button.waitForExistence(timeout: 5), "no back button")
        button.tap()
    }

    @MainActor
    private func switchOn(_ toggle: XCUIElement, _ what: String, in app: XCUIApplication) {
        require(toggle, what, in: app)
        scrollTo(toggle, in: app)
        guard (toggle.value as? String) != "1" else { return }
        toggle.tap()
        if (toggle.value as? String) == "0" {
            // Some iOS versions only toggle when the switch itself (trailing edge) is hit.
            toggle.coordinate(withNormalizedOffset: CGVector(dx: 0.92, dy: 0.5)).tap()
        }
        expectation(for: NSPredicate(format: "value == '1'"), evaluatedWith: toggle)
        waitForExpectations(timeout: 10)
    }

    /// Waits until the controller's status line reads `text` (contains).
    @MainActor
    private func waitForStatus(_ text: String, timeout: TimeInterval, in app: XCUIApplication) {
        let status = app.descendants(matching: .any)["controller.status"].firstMatch
        require(status, "the controller's status line", in: app)
        expectation(for: NSPredicate(format: "label CONTAINS %@", text), evaluatedWith: status)
        waitForExpectations(timeout: timeout)
    }

    @MainActor
    func testControlAnotherPhoneOverLoopback() {
        // Belt and braces: a notification-permission alert (the app asks when remote
        // control is turned on outside the playground) is answered, not left over the UI.
        _ = addUIInterruptionMonitor(withDescription: "Notification permission") { element in
            nonisolated(unsafe) let alert = element
            return MainActor.assumeIsolated {
                for title in ["Allow", "Don\u{2019}t Allow", "Don't Allow"] where alert.buttons[title].exists {
                    alert.buttons[title].tap()
                    return true
                }
                return false
            }
        }
        let app = launchPlayground()
        tab("Control", in: app).tap()
        push(app.buttons["control.remote"], "the Remote control row", in: app)

        // The phone side: remote control on and listening (its status line; the
        // connection details row depends on the transport's addresses, not needed here).
        switchOn(app.switches["remote.enabled"], "the Remote control switch", in: app)
        let remoteStatus = app.descendants(matching: .any)["remote.status"].firstMatch
        require(remoteStatus, "the Remote control status line", timeout: 10, in: app)
        expectation(for: NSPredicate(format: "label CONTAINS %@", "Listening"), evaluatedWith: remoteStatus)
        waitForExpectations(timeout: 15)

        // The controller: pair with this phone. The code is handed over in-process; the
        // code field is never read (its accessibility value is empty on purpose).
        push(app.buttons["remote.controlAnother"], "Control another phone", in: app)
        require(app.staticTexts["controller.title"], "the controller screen", in: app)
        let pairHere = app.buttons["controller.qa.pairHere"]
        require(pairHere, "Pair with this phone (QA)", in: app)
        scrollTo(pairHere, in: app)
        pairHere.tap()

        // Paired, not yet confirmed: the controller shows its check string and nothing else.
        waitForStatus("Waiting for confirmation", timeout: 30, in: app)
        let prompt = app.staticTexts["controller.confirmCheck"]
        require(prompt, "the controller's confirm line", timeout: 10, in: app)
        let controllerCheck = String(prompt.label.suffix(7))
        XCTAssertTrue(prompt.label.hasPrefix("Confirm on the phone that it shows "), "prompt: \(prompt.label)")
        XCTAssertFalse(app.textFields["controller.composer"].exists && app.textFields["controller.composer"].isEnabled,
                       "a device waiting for confirmation was offered to start a task")

        // The phone side: the pending row shows the same string; the owner taps Confirm.
        back(in: app)
        let pendingCheck = app.staticTexts["remote.pending.0.check"]
        require(pendingCheck, "the pending device on the phone", timeout: 20, in: app)
        XCTAssertEqual(pendingCheck.label, controllerCheck, "the check strings differ")
        let confirm = app.buttons["remote.pending.0.confirm"]
        require(confirm, "Confirm on the phone", in: app)
        scrollTo(confirm, in: app)
        confirm.tap()

        // The phone side: the new device may watch the screen layout.
        let device = app.descendants(matching: .any)["remote.device.0"].firstMatch
        require(device, "the controller in the phone's device list", timeout: 20, in: app)
        XCTAssertFalse(app.descendants(matching: .any)["remote.pending.0"].firstMatch.exists, "still listed as pending")
        switchOn(app.switches["remote.device.0.watchScreen"], "Watch the screen layout", in: app)
        XCTAssertEqual(app.switches["remote.device.0.approve"].value as? String, "0", "watching must not turn approving on")

        // The controller: confirmed (idle), then watch, then start a scripted task.
        push(app.buttons["remote.controlAnother"], "Control another phone", in: app)
        waitForStatus("Idle", timeout: 20, in: app)
        // Let in, but the controller keeps its numbers until the owner says they matched
        // (a welcome is not proof that the phone confirmed THIS device).
        let kept = app.staticTexts["controller.check"]
        require(kept, "the controller's check string after the welcome", timeout: 10, in: app)
        XCTAssertEqual(kept.label, controllerCheck, "the controller shows other numbers after the welcome")
        let reminder = app.staticTexts["controller.checkReminder"]
        require(reminder, "the controller's reminder to compare the numbers", in: app)
        XCTAssertTrue(reminder.label.contains(controllerCheck), "reminder: \(reminder.label)")
        XCTAssertFalse(reminder.label.contains("Confirmed on the phone"), "reminder: \(reminder.label)")
        let matched = app.buttons["controller.checkMatched"]
        require(matched, "The numbers matched", in: app)
        scrollTo(matched, in: app)
        matched.tap()
        XCTAssertTrue(kept.waitForNonExistence(timeout: 10), "the check string stayed after the owner said it matched")
        XCTAssertTrue(app.descendants(matching: .any)["controller.sharingNote"].firstMatch.exists,
                      "a new device does not see screen text: the screen must say what it is not shown")
        let watch = app.buttons["controller.watch"]
        require(watch, "Watch the screen layout on the controller", in: app)
        scrollTo(watch, in: app)
        watch.tap()
        let live = app.descendants(matching: .any)["controller.live"].firstMatch
        require(live, "the controller's Live view", timeout: 20, in: app)

        let composer = app.textFields["controller.composer"]
        require(composer, "the controller's composer", in: app)
        app.swipeDown()
        composer.tap()
        composer.typeText("Turn off Wi-Fi")
        let send = app.buttons["controller.send"]
        require(send, "the controller's send button", in: app)
        send.tap()

        // The task runs on this phone: its Run screen comes up over the tabs, and the first
        // use in Settings is asked there (a new device may not approve).
        let approve = app.buttons["confirm.approve"]
        require(approve, "the first-use confirmation on the phone", timeout: 40, in: app)
        approve.tap()
        let newTask = app.buttons["run.newTask"]
        require(newTask, "the phone's run to end", timeout: 60, in: app)
        newTask.tap()
        XCTAssertTrue(newTask.waitForNonExistence(timeout: 10), "the phone's Run screen stayed up")

        // Back on the controller: it saw the task end, and the Live view drew it.
        waitForStatus("Ended", timeout: 30, in: app)
        let status = app.descendants(matching: .any)["controller.status"].firstMatch
        XCTAssertTrue(status.label.contains("Done"), "status: \(status.label)")
        require(live, "the controller's Live view after the run", timeout: 10, in: app)
        let caption = (live.value as? String) ?? ""
        XCTAssertFalse(caption.isEmpty, "the controller's Live view shows no caption")
        XCTAssertFalse(app.buttons["controller.approve"].exists, "an approval was offered to a device that may not approve")
    }
}
