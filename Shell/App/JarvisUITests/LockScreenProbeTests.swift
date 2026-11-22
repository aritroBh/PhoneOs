import Foundation
import XCTest
import JarvisCore
import JarvisKit

/// Evidence gathering, not a feature test: what an in-process XCUITest runner sees and can
/// do while the simulator is locked (no passcode: "display off / lock screen" only), and
/// whether a session recovers after waking.
///
/// Every step runs even when an earlier one failed, records what happened (never what was
/// expected), and adds observation artifacts (`lock-probe-<step>.NN-…`: typed tree, the core's
/// compact screen, screenshot) plus one line in `lock-probe.json`. XCTest issues raised by
/// probing calls are captured as data with `XCTExpectFailure` (non-strict).
///
/// Assertions: only that the probe reached the end and wrote its report, and, when waking
/// worked, that a short agent task succeeds right after.
final class LockScreenProbeTests: QATestCase {
    private struct Entry {
        var step: String
        var action: String
        var outcome: String
        var details: [String: String]
        var elapsedMs: Int
    }

    private var entries: [Entry] = []
    private let settingsId = "com.apple.Preferences"

    @MainActor
    func testLockScreenProbe() async throws {
        continueAfterFailure = true

        // 1. Baseline.
        let app = launchPlayground()
        await observe("1-baseline", "app launched, observe")

        // 2. Lock.
        var locked = false
        record("2-lock", "lock the device") { _ in
            let what = SimulatorLock.lock()
            locked = what == "pressLockButton performed"
            return what
        }
        try await Task.sleep(for: .seconds(2))

        // 3. While locked.
        await observe("3a-locked-snapshot", "observe while locked (which app is in front?)")
        record("3b-locked-screenshot", "XCUIScreen.main.screenshot()") { details in
            let shot = XCUIScreen.main.screenshot()
            details["pixels"] = "\(Double(shot.image.size.width * shot.image.scale).compactText)x\(Double(shot.image.size.height * shot.image.scale).compactText)"
            details["png_bytes"] = "\(shot.pngRepresentation.count)"
            attach(XCTAttachment(screenshot: shot), "lock-probe.3b-locked-screenshot.png")
            return "screenshot taken"
        }
        record("3c-locked-tap", "tap the screen centre via SpringBoard coordinates") { _ in
            XCUIApplication(bundleIdentifier: "com.apple.springboard")
                .coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
            return "tap sent"
        }
        await observe("3d-after-locked-tap", "observe after the tap")
        record("3e-locked-activate", "XCUIApplication(Settings).activate()") { details in
            let settings = XCUIApplication(bundleIdentifier: settingsId)
            settings.activate()
            let foreground = settings.wait(for: .runningForeground, timeout: 10)
            details["state_after"] = "\(settings.state.rawValue)"
            return foreground ? "Settings reached the foreground" : "Settings did not reach the foreground in 10 s"
        }
        await observe("3f-after-locked-activate", "observe after activate")
        record("3g-locked-open-url", "XCUIDevice.shared.system.open(https://www.apple.com)") { _ in
            guard let url = URL(string: "https://www.apple.com") else { return "bad URL" }
            XCUIDevice.shared.system.open(url)
            return "open(_:) returned"
        }
        await observe("3h-after-locked-open-url", "observe after open URL")

        // 4. Wake / unlock attempts.
        var unlocked = false
        let attempts: [(String, String, @MainActor () -> Void)] = [
            ("4a-home-once", "press home once", { XCUIDevice.shared.press(.home) }),
            ("4b-home-twice", "press home twice", {
                XCUIDevice.shared.press(.home)
                XCUIDevice.shared.press(.home)
            }),
            ("4c-swipe-up", "swipe up from the bottom edge on SpringBoard", {
                let board = XCUIApplication(bundleIdentifier: "com.apple.springboard")
                let bottom = board.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.995))
                let middle = board.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.35))
                bottom.press(forDuration: 0.05, thenDragTo: middle)
            }),
        ]
        for (step, action, perform) in attempts {
            record(step, action) { _ in
                perform()
                return "done"
            }
            try await Task.sleep(for: .seconds(1))
            await observe("\(step)-observe", "observe after \(action)")
            var worked = false
            record("\(step)-activate", "activate Settings after \(action)") { details in
                let settings = XCUIApplication(bundleIdentifier: settingsId)
                settings.activate()
                worked = settings.wait(for: .runningForeground, timeout: 8)
                details["state_after"] = "\(settings.state.rawValue)"
                return worked ? "Settings reached the foreground: home screen reachable" : "Settings did not reach the foreground"
            }
            if worked {
                unlocked = true
                XCUIDevice.shared.press(.home)
                break
            }
        }

        // 5. Recovery: one short agent task right after waking.
        var recovered: Bool?
        if unlocked {
            _ = launchPlayground()
            let run = makeRun("lock-probe-5-recovery")
            run.runner.noteActivated(Self.appBundleId)
            let start = Date()
            do {
                let outcome = try await run.run(task: "Turn on tips in the playground",
                                                model: ScriptedModel(steps: PlaygroundAgentTests.toggleScript))
                recovered = outcome.result.outcome.status == .success
                entries.append(Entry(step: "5-recovery", action: "scripted toggle task after waking",
                                     outcome: "\(outcome.result.outcome.status): \(outcome.result.outcome.summary)",
                                     details: ["model_calls": "\(outcome.result.modelCalls)", "steps": "\(outcome.result.outcome.steps)"],
                                     elapsedMs: (Date().timeIntervalSince(start) * 1_000).clampedInt))
            } catch {
                recovered = false
                entries.append(Entry(step: "5-recovery", action: "scripted toggle task after waking",
                                     outcome: "threw: \(error)", details: [:], elapsedMs: (Date().timeIntervalSince(start) * 1_000).clampedInt))
            }
        } else {
            entries.append(Entry(step: "5-recovery", action: "scripted toggle task after waking",
                                 outcome: "not run: no wake attempt made Settings reachable", details: [:], elapsedMs: 0))
        }
        _ = app

        // Report.
        let report = reportJSON(locked: locked, unlocked: unlocked, recovered: recovered)
        let summary = summaryText(locked: locked, unlocked: unlocked, recovered: recovered)
        XCTContext.runActivity(named: "Lock-screen probe summary") { activity in
            let text = XCTAttachment(string: summary)
            text.name = "lock-probe.summary.txt"
            text.lifetime = .keepAlways
            activity.add(text)
        }
        print(summary)
        attach(XCTAttachment(data: report, uniformTypeIdentifier: "public.json"), "lock-probe.json")

        XCTAssertFalse(report.isEmpty, "the probe wrote no report")
        XCTAssertTrue(entries.contains { $0.step == "5-recovery" }, "the probe did not reach the end")
        if unlocked {
            XCTAssertEqual(recovered, true, "after waking, the agent task did not succeed:\n\(summary)")
        }
    }

    // MARK: - Steps

    /// Runs one probing action; XCTest issues it raises are captured as data.
    @MainActor
    private func record(_ step: String, _ action: String, _ body: (inout [String: String]) throws -> String) {
        var details: [String: String] = [:]
        var issues: [String] = []
        let options = XCTExpectedFailure.Options()
        options.isStrict = false
        options.issueMatcher = { issue in
            issues.append(issue.compactDescription)
            return true
        }
        let start = Date()
        var outcome: String
        do {
            outcome = try XCTExpectFailure("lock probe: \(action) may fail while locked", options: options) {
                try body(&details)
            }
        } catch {
            outcome = "threw: \(error)"
        }
        if !issues.isEmpty {
            details["xctest_issues"] = issues.joined(separator: " | ")
            outcome += " (XCTest issues recorded)"
        }
        entries.append(Entry(step: step, action: action, outcome: outcome, details: details,
                             elapsedMs: (Date().timeIntervalSince(start) * 1_000).clampedInt))
    }

    /// One observation through the real core (a one-step session whose model only says
    /// done), so the artifacts include the core's compact screen. The runner starts with no
    /// app in mind, so its foreground choice is itself evidence.
    @MainActor
    private func observe(_ step: String, _ action: String) async {
        let run = makeRun("lock-probe-\(step)")
        let start = Date()
        var details: [String: String] = [:]
        let outcome: String
        do {
            let result = try await run.run(task: "Report the screen", model: ScriptedModel(steps: [.done("observed")]),
                                           maxSteps: 3, maxDurationMs: 60_000)
            if let user = result.requests.first?.user {
                let screen = PromptScreen(prompt: user)
                details["foreground"] = screen.app ?? "?"
                details["elements"] = "\(screen.elements.count)"
                details["first_elements"] = screen.elements.prefix(8).map(\.line).joined(separator: "; ")
                details["keyboard"] = screen.keyboardVisible ? "visible" : "hidden"
                outcome = "observed"
            } else {
                outcome = "no prompt was built: \(result.result.outcome.status) \(result.result.endReason) \(result.result.outcome.summary)"
            }
        } catch {
            outcome = "threw: \(error)"
        }
        entries.append(Entry(step: step, action: action, outcome: outcome, details: details,
                             elapsedMs: (Date().timeIntervalSince(start) * 1_000).clampedInt))
    }

    // MARK: - Report

    private func reportJSON(locked: Bool, unlocked: Bool, recovered: Bool?) -> Data {
        let object: [String: Any] = [
            "locked_attempted": locked,
            "unlocked": unlocked,
            "recovered": recovered.map { $0 ? "yes" : "no" } ?? "not run",
            "steps": entries.map { e in
                ["step": e.step, "action": e.action, "outcome": e.outcome, "details": e.details, "elapsed_ms": e.elapsedMs] as [String: Any]
            },
        ]
        return (try? JSONSerialization.data(withJSONObject: object, options: [.prettyPrinted, .sortedKeys])) ?? Data()
    }

    private func summaryText(locked: Bool, unlocked: Bool, recovered: Bool?) -> String {
        var lines = ["LOCK-SCREEN PROBE: lock attempted \(locked), unlocked \(unlocked), recovered \(recovered.map(String.init) ?? "not run")"]
        for e in entries {
            var line = "- \(e.step): \(e.outcome) (\(e.elapsedMs) ms)"
            if let fg = e.details["foreground"] { line += " foreground=\(fg) elements=\(e.details["elements"] ?? "?")" }
            if let issues = e.details["xctest_issues"] { line += " issues=\(issues)" }
            lines.append(line)
        }
        return lines.joined(separator: "\n")
    }

    @MainActor
    private func attach(_ attachment: XCTAttachment, _ name: String) {
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
