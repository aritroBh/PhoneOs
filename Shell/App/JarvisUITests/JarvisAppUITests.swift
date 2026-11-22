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

        tab("Activity", in: app).tap()
        require(app.staticTexts["activity.title"], "the Activity title", in: app)
        require(app.staticTexts["activity.empty"], "the empty Activity message", in: app)

        tab("Apps", in: app).tap()
        require(app.staticTexts["apps.title"], "the Apps title", in: app)
        require(app.buttons["apps.row.com.apple.Preferences"], "the Settings trust row", in: app)
        app.buttons["apps.row.com.apple.Preferences"].tap()
        require(app.descendants(matching: .any)["apps.trust.com.apple.Preferences"].firstMatch, "the trust level control", in: app)

        tab("Rules", in: app).tap()
        require(app.staticTexts["rules.title"], "the Rules title", in: app)
        require(app.buttons["rules.mode.guarded"], "the Guarded mode card", in: app)
        require(app.switches["rules.firstUse"], "the first-use switch", in: app)

        tab("Ask", in: app).tap()
        require(app.staticTexts["ask.prompt"], "the Ask screen again", in: app)
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
