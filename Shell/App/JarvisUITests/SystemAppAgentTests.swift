import XCTest
import JarvisCore
import JarvisKit

/// Tier B: the agent drives real system apps in the simulator. Their trees differ between
/// iOS versions; the artifacts of these runs are how real trees are learned. First-launch
/// prompts are handled by the agent (dismiss rules) and logged by the interruption monitor.
/// A failure names what was on screen (see `QAOutcome.explanation` and the artifacts).
final class SystemAppAgentTests: QATestCase {
    /// First-launch surfaces the agent may dismiss. Each rule needs the surface's own text
    /// to be listed and fires at most once, so a toolbar "Close" (run 13: Safari's
    /// StopButton, which aborted the page load) is never mistaken for one. Only "What's New"
    /// style sheets are covered; anything else stays on screen and shows up in the artifacts.
    private static let firstLaunchDismissals = [
        DismissRule(tap: ElementQuery("Continue", match: .exact, roles: ["Button"]),
                    when: [ElementQuery("What's New", match: .contains, roles: ["StaticText"])]),
        DismissRule(tap: ElementQuery("Not Now", match: .exact, roles: ["Button"]),
                    when: [ElementQuery("What's New", match: .contains, roles: ["StaticText"])]),
    ]

    private let settingsId = "com.apple.Preferences"
    private let safariId = "com.apple.mobilesafari"

    @MainActor
    private func startOnHomeScreen(terminating ids: [String]) {
        for id in ids { XCUIApplication(bundleIdentifier: id).terminate() }
        XCUIDevice.shared.press(.home)
    }

    @MainActor
    func testSettingsGeneralAbout() async throws {
        startOnHomeScreen(terminating: [settingsId])
        let run = makeRun("system-settings-about")
        let model = ScriptedModel(
            steps: [
                .openApp("Settings"),
                .scroll(to: "General", maxScrolls: 6),
                .tap("General", match: .exact),
                .scroll(to: "About", maxScrolls: 4),
                .tap("About", match: .exact),
                .done("About is open", when: ["iOS Version", "Software Version", "Model Name", "Serial Number"]),
            ],
            options: .init(dismiss: Self.firstLaunchDismissals)
        )
        let owner = QAOwner(transcript: run.transcript)
        let outcome = try await run.run(task: "Open About in Settings", model: model, owner: owner, firstUseApproval: true)

        XCTAssertEqual(outcome.result.outcome.status, .success, outcome.explanation)
        XCTAssertEqual(XCUIApplication(bundleIdentifier: settingsId).state, .runningForeground, outcome.explanation)
        XCTAssertTrue(owner.confirms.contains { $0.category == .firstUse && $0.app == settingsId },
                      "first use of Settings was not approved through the gate; confirmations: \(owner.confirms)")
    }

    @MainActor
    func testSpringboardOpensAnApp() async throws {
        startOnHomeScreen(terminating: [settingsId])
        let run = makeRun("system-springboard-open")
        let model = ScriptedModel(
            steps: [
                .tap("Settings", role: "Button", match: .exact),
                // SpringBoard has no "General"; it appears only once Settings is in front.
                .done("Settings is open", when: ["General"]),
            ],
            options: .init(dismiss: Self.firstLaunchDismissals)
        )
        let outcome = try await run.run(task: "Open the Settings app from the home screen", model: model, firstUseApproval: true)

        XCTAssertEqual(outcome.result.outcome.status, .success, outcome.explanation)
        XCTAssertEqual(XCUIApplication(bundleIdentifier: settingsId).state, .runningForeground, outcome.explanation)
        let firstPrompt = try XCTUnwrap(outcome.requests.first?.user, "no prompt was built")
        XCTAssertTrue(firstPrompt.contains("com.apple.springboard"), "the first observation was not SpringBoard:\n\(firstPrompt)")
    }

    @MainActor
    func testSafariFillsAndSubmitsAForm() async throws {
        let server = try FormCaptureServer()
        defer { server.stop() }
        startOnHomeScreen(terminating: [safariId])
        let run = makeRun("system-safari-form")
        let url = server.baseURL.absoluteString + "/"
        let model = ScriptedModel(
            steps: [
                .openURL(url),
                .type("Ada Lovelace", into: "Full name", role: "TextField"),
                .type("ada@example.com", into: "Email address", role: "TextField"),
                .tap("Send form", role: "Button"),
                .done("Form sent", when: ["Thanks, Ada Lovelace"]),
            ],
            options: .init(dismiss: Self.firstLaunchDismissals, patience: 8, waitMs: 1_000)
        )
        let outcome = try await run.run(task: "Fill in the contact form in Safari", model: model, firstUseApproval: true)
        let submitted = await server.waitForSubmission(timeout: .seconds(5))
        let serverLog = server.log.joined(separator: "\n")
        let logAttachment = XCTAttachment(string: "server at \(url)\n" + (serverLog.isEmpty ? "(no request reached the server)" : serverLog))
        logAttachment.name = "system-safari-form.server-log.txt"
        logAttachment.lifetime = .keepAlways
        add(logAttachment)

        XCTAssertEqual(outcome.result.outcome.status, .success, outcome.explanation + "\nserver log:\n\(serverLog)")
        XCTAssertEqual(submitted?["name"], "Ada Lovelace", "server received \(String(describing: submitted)); log:\n\(serverLog)")
        XCTAssertEqual(submitted?["email"], "ada@example.com", "server received \(String(describing: submitted)); log:\n\(serverLog)")
    }
}
