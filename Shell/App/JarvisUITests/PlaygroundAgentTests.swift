import XCTest
import JarvisCore
import JarvisKit

/// Tier A: the agent (real core, SessionDriver, scripted model that reads the prompt) drives
/// the Jarvis app's Playground fixtures through the in-process XCUITest runner.
/// Deterministic: these must pass.
final class PlaygroundAgentTests: QATestCase {
    /// The password the owner types. It must never reach a prompt, a reply, the memory or
    /// the transcript.
    private let password = "Qa-Secret-7391"

    @MainActor
    func testLoginFormWithOwnerEnteredPassword() async throws {
        let app = launchPlayground()
        let run = makeRun("playground-login")
        run.runner.noteActivated(Self.appBundleId)
        let password = password
        let owner = QAOwner(transcript: run.transcript, onQuestion: { question in
            // The core never types into a password field: it asks the owner, who types it.
            guard question.lowercased().contains("password") else { return nil }
            let field = XCUIApplication(bundleIdentifier: QATestCase.appBundleId).secureTextFields["login.password"]
            guard field.waitForExistence(timeout: 5) else { return nil }
            field.tap()
            field.typeText(password)
            return "I entered it. Continue."
        })
        let model = ScriptedModel(steps: [
            .tap("Login form"),
            .type("Ada", into: "Username", role: "TextField"),
            .type("(the owner types it)", into: "Password", role: "SecureTextField"),
            .tap("Sign In", role: "Button"),
            .done("Signed in as Ada", when: ["Welcome, Ada"]),
        ])
        let outcome = try await run.run(task: "Sign in to the playground as Ada", model: model, owner: owner)

        XCTAssertEqual(outcome.result.outcome.status, .success, outcome.explanation)
        XCTAssertEqual(owner.questions.count, 1, "the password field is handed to the owner exactly once; questions: \(owner.questions)")
        XCTAssertEqual(app.staticTexts["login.message"].label, "Welcome, Ada", outcome.explanation)
        XCTAssertEqual(app.textFields["login.username"].value as? String, "Ada")

        // The secure value never leaves the owner's hands.
        XCTAssertFalse(outcome.allModelText.contains(password), "the password reached a prompt or reply")
        XCTAssertFalse(outcome.transcriptJSON.contains(password), "the password reached the transcript")
        let memory = try XCTUnwrap(outcome.memoryJSON, "no memory was saved")
        XCTAssertFalse(memory.contains(password), "the password reached the saved memory")
    }

    @MainActor
    func testToggle() async throws {
        let app = launchPlayground()
        let run = makeRun("playground-toggle")
        run.runner.noteActivated(Self.appBundleId)
        let outcome = try await run.run(task: "Turn on tips in the playground", model: ScriptedModel(steps: Self.toggleScript))

        XCTAssertEqual(outcome.result.outcome.status, .success, outcome.explanation)
        XCTAssertEqual(app.staticTexts["toggles.status"].label, "Tips are on", outcome.explanation)
        XCTAssertEqual(app.switches["toggles.tips"].value as? String, "1", outcome.explanation)
    }

    @MainActor
    func testScrollThenTap() async throws {
        let app = launchPlayground()
        let run = makeRun("playground-scroll")
        run.runner.noteActivated(Self.appBundleId)
        let model = ScriptedModel(steps: [
            .tap("Long list"),
            .scroll(to: "Item 60", maxScrolls: 25),
            .tap("Item 60", match: .exact),
            .done("Opened Item 60", when: ["Selected Item 60"]),
        ])
        let outcome = try await run.run(task: "Open item 60 in the long list", model: model)

        XCTAssertEqual(outcome.result.outcome.status, .success, outcome.explanation)
        XCTAssertTrue(model.decisions.contains { $0.reply.contains(#""action":"scroll""#) }, "the row was reachable without scrolling; the fixture is too short")
        XCTAssertEqual(app.staticTexts["list.selected"].label, "Selected Item 60", outcome.explanation)
    }

    @MainActor
    func testDestructiveTapIsGatedAndDeclineLeavesUIUnchanged() async throws {
        let app = launchPlayground()
        let run = makeRun("playground-delete-declined")
        run.runner.noteActivated(Self.appBundleId)
        let owner = QAOwner(transcript: run.transcript, decide: { $0.category != .delete })
        let model = ScriptedModel(steps: [
            .tap("Danger zone"),
            .tap("Delete account", role: "Button"),
            .done("Deleted", when: ["Account deleted"]),
        ])
        let outcome = try await run.run(task: "Delete my playground account", model: model, owner: owner)

        XCTAssertTrue(owner.confirms.contains { $0.category == .delete }, "no delete confirmation was asked; confirmations: \(owner.confirms)")
        XCTAssertNotEqual(outcome.result.outcome.status, .success, outcome.explanation)
        XCTAssertEqual(app.staticTexts["danger.status"].label, "Account active", "the declined tap was executed")
        XCTAssertTrue(app.buttons["danger.delete"].exists)
    }

    @MainActor
    func testSystemStyleAlert() async throws {
        let app = launchPlayground()
        let run = makeRun("playground-alert")
        run.runner.noteActivated(Self.appBundleId)
        let model = ScriptedModel(steps: [
            .tap("Alerts"),
            .tap("Show alert", role: "Button"),
            .tap("Dismiss", role: "Button", match: .exact),
            .done("Alert dismissed", when: ["Alert dismissed"]),
        ])
        let outcome = try await run.run(task: "Show the playground alert and dismiss it", model: model)

        XCTAssertEqual(outcome.result.outcome.status, .success, outcome.explanation)
        XCTAssertEqual(app.staticTexts["alerts.status"].label, "Alert dismissed", outcome.explanation)
    }

    @MainActor
    func testSecondRunReplaysLearnedFlowWithZeroModelCalls() async throws {
        _ = launchPlayground()
        let first = makeRun("playground-replay-run1")
        first.runner.noteActivated(Self.appBundleId)
        let learned = try await first.run(task: "Turn on tips in the playground", model: ScriptedModel(steps: Self.toggleScript))
        XCTAssertEqual(learned.result.outcome.status, .success, learned.explanation)
        let memory = try XCTUnwrap(learned.memoryJSON, "the first run saved no memory")
        XCTAssertEqual(MemorySnapshot(json: memory).flows.count, 1, "the first run learned no flow")

        // Fresh app state, same task, saved memory: the flow replays without the model.
        let app = launchPlayground()
        let second = makeRun("playground-replay-run2")
        second.runner.noteActivated(Self.appBundleId)
        let untouchable = ScriptedModel(steps: [.fail(reason: "the model was called during a replay")])
        let replay = try await second.run(task: "Turn on tips in the playground", model: untouchable, memoryJSON: memory)

        XCTAssertEqual(replay.result.outcome.status, .success, replay.explanation)
        XCTAssertEqual(replay.result.modelCalls, 0, replay.explanation)
        XCTAssertEqual(untouchable.callCount, 0)
        XCTAssertEqual(app.staticTexts["toggles.status"].label, "Tips are on", replay.explanation)
    }

    static let toggleScript: [ScriptStep] = [
        .tap("Toggles"),
        // A SwiftUI Toggle is a Switch whose tree value is "0"/"1"; the core shows it to the
        // model as (off)/(on), so the script asks for the state, not for a value.
        .tap(ElementQuery("Show tips", match: .exact, roles: ["Switch"], isOn: false)),
        .done("Tips are on", when: ["Tips are on"]),
    ]
}
