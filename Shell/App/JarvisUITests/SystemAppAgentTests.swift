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
        // Safari's first-launch tip about its menu (simulator run 9): it covers the page.
        DismissRule(tap: ElementQuery("Close", match: .exact, roles: ["Button"]),
                    when: [ElementQuery("You can now view these items in the menu", match: .contains, roles: ["StaticText"])]),
    ]

    /// Sixteenth review (Rule T): a question that asks whether what Jarvis read elsewhere in
    /// the task may reach the app in front (the core's flow note).
    static func isFlowQuestion(_ request: ConfirmRequest) -> Bool {
        request.summary.contains("what it read there could be passed on here")
    }

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
        // This owner approves everything, so a flow question would pass unseen: assert none was
        // asked (home screen rule, sixteenth review: the widgets are not shown to the model).
        XCTAssertFalse(owner.confirms.contains(where: SystemAppAgentTests.isFlowQuestion), "a flow question was asked after the home screen: \(owner.confirms)")
    }

    /// The Settings icon is a known app (`KnownApps.common`), so since the core checks a
    /// home-screen tap like `open_app` it needs no question at the tap, and no first-use
    /// question either (that comes with the first action inside Settings, and this script
    /// takes none). The owner here declines everything, so any question fails the test
    /// instead of being approved silently.
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
        let owner = QAOwner(transcript: run.transcript, decide: { _ in false })
        let outcome = try await run.run(task: "Open the Settings app from the home screen", model: model, owner: owner, firstUseApproval: true)

        XCTAssertEqual(outcome.result.outcome.status, .success, outcome.explanation)
        XCTAssertEqual(XCUIApplication(bundleIdentifier: settingsId).state, .runningForeground, outcome.explanation)
        XCTAssertTrue(owner.confirms.isEmpty, "a known app's icon asked the owner something: \(owner.confirms)")
        let firstPrompt = try XCTUnwrap(outcome.requests.first?.user, "no prompt was built")
        XCTAssertTrue(firstPrompt.contains("com.apple.springboard"), "the first observation was not SpringBoard:\n\(firstPrompt)")
    }

    /// An icon whose label is not a known app (here Settings, taken out of the known list)
    /// asks a one-off first-use confirmation with app `com.apple.springboard` and the icon's
    /// label. The QA owner approves exactly that request, by rule, and the test asserts it
    /// was asked and how the sheet words it.
    @MainActor
    func testSpringboardUnknownIconAsksOnce() async throws {
        startOnHomeScreen(terminating: [settingsId])
        let run = makeRun("system-springboard-unknown-icon")
        let model = ScriptedModel(
            steps: [
                .tap("Settings", role: "Button", match: .exact),
                .done("Settings is open", when: ["General"]),
            ],
            options: .init(dismiss: Self.firstLaunchDismissals)
        )
        let settings = settingsId
        let owner = QAOwner(transcript: run.transcript, decide: { request in
            // The icon question, and (a first-launch sheet's button is an action in Settings)
            // Settings' own first use. Nothing else: the home screen's widgets are not shown
            // to the model, so the home screen is not something Jarvis read, and no flow
            // question (Rule T) is asked when the first input goes into Settings.
            request.category == .firstUse && ["com.apple.springboard", settings].contains(request.app)
        })
        let unknownToCore = KnownApps.common.filter { $0.name != "Settings" }
        let outcome = try await run.run(
            task: "Open the Settings app from the home screen", model: model, owner: owner,
            firstUseApproval: true, knownApps: unknownToCore
        )

        // Home screen rule (sixteenth review): the widgets' contents never reach the model,
        // so reading the home screen is not a read and no flow question was asked.
        XCTAssertFalse(owner.confirms.contains(where: SystemAppAgentTests.isFlowQuestion), "a flow question was asked after the home screen: \(owner.confirms)")
        let asked = try XCTUnwrap(owner.confirms.first, "tapping an icon the core cannot name asked nothing\n\(outcome.explanation)")
        XCTAssertEqual(owner.confirms.filter { $0.app == "com.apple.springboard" }.count, 1, "\(owner.confirms)")
        XCTAssertEqual(asked.category, .firstUse)
        XCTAssertEqual(asked.app, "com.apple.springboard")
        XCTAssertEqual(ConfirmationWording(asked, appName: "SpringBoard").title, "Open \"Settings\" from the Home Screen?", asked.summary)
        XCTAssertEqual(outcome.result.outcome.status, .success, outcome.explanation)
        XCTAssertEqual(XCUIApplication(bundleIdentifier: settingsId).state, .runningForeground, outcome.explanation)
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
            // Run 109: Safari's first launch sat on its Start Page for about 8 s and the page's
            // content appeared after 20 s; with 8 waits the script gave up before the form
            // existed (nothing was typed, nothing asked).
            options: .init(dismiss: Self.firstLaunchDismissals, patience: 30, waitMs: 1_000)
        )
        // The owner's rule: first-use confirmations (Safari's, and a first-launch sheet's
        // button) and the one question for the submit, which must name the button and what is
        // sent. Anything else is declined, and fails the run with that reason.
        let safari = safariId
        let owner = QAOwner(transcript: run.transcript, decide: { request in
            if request.category == .firstUse { return true }
            // Home screen rule (sixteenth review): the run starts on the home screen, but its
            // widgets are not shown to the model, so it is not a read and no flow question
            // (Rule T) is asked at the first input into Safari; one would be declined here.
            return request.app == safari
                && request.summary.contains("Send form")
                && request.summary.contains("Ada Lovelace")
                && request.summary.contains("ada@example.com")
        })
        // The owner names the link in the request: a link the owner did not give is asked
        // before it is opened (thirteenth review), and this owner would decline it. The owner
        // also says what to fill in: text not from the request is asked before it is typed
        // (fifteenth review, R1), and this owner would decline that too.
        let task = "Open \(url) in Safari and fill in the contact form with the name Ada Lovelace and the email ada@example.com"
        let outcome = try await run.run(task: task, model: model, owner: owner, firstUseApproval: true)
        XCTAssertFalse(owner.confirms.contains(where: SystemAppAgentTests.isFlowQuestion), "a flow question was asked after the home screen: \(owner.confirms)")
        XCTAssertFalse(owner.confirms.contains { $0.summary.contains("This text is not from your request") }, "the owner's own words were asked: \(owner.confirms)")
        XCTAssertFalse(owner.confirms.contains { $0.summary.contains("Open a link") }, "a link the owner gave was asked: \(owner.confirms)")
        let submitted = await server.waitForSubmission(timeout: .seconds(5))
        let serverLog = server.log.joined(separator: "\n")
        let logAttachment = XCTAttachment(string: "server at \(url)\n" + (serverLog.isEmpty ? "(no request reached the server)" : serverLog))
        logAttachment.name = "system-safari-form.server-log.txt"
        logAttachment.lifetime = .keepAlways
        add(logAttachment)

        let submitQuestions: [ConfirmRequest] = owner.confirms.filter { $0.app == safari && $0.summary.contains("Send form") }
        XCTAssertEqual(submitQuestions.count, 1, "the submit asks exactly once: \(owner.confirms)")
        if let question = submitQuestions.first {
            XCTAssertTrue(question.summary.contains("Sends:"), "the question does not say what is sent: \(question.summary)")
            XCTAssertTrue(question.summary.contains("Ada Lovelace"), question.summary)
            XCTAssertTrue(question.summary.contains("ada@example.com"), question.summary)
        }
        XCTAssertEqual(outcome.result.outcome.status, .success, outcome.explanation + "\nserver log:\n\(serverLog)")
        XCTAssertEqual(submitted?["name"], "Ada Lovelace", "server received \(String(describing: submitted)); log:\n\(serverLog)")
        XCTAssertEqual(submitted?["email"], "ada@example.com", "server received \(String(describing: submitted)); log:\n\(serverLog)")
    }
}
