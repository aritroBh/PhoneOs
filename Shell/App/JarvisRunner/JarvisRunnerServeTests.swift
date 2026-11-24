import Foundation
import XCTest
import JarvisCore
import JarvisKit

/// Jarvis's own runner: one long-running UI test that serves `XCUITestPhoneRunner` (the
/// typed tree the simulator QA uses) to the Jarvis app over the authenticated loopback
/// protocol until the test process is stopped (docs/JARVIS-RUNNER.md).
///
/// It runs only when `JARVIS_RUNNER_SERVE=1` is in its environment (the JarvisRunner
/// scheme sets it), so the QA build, which compiles this bundle, never blocks on it.
///
/// Key provisioning (candidate R2, unverified on a device): a fresh key per start, handed
/// to the Jarvis app in its launch environment; the app keeps it in its Keychain. The key
/// is never logged.
final class JarvisRunnerServeTests: XCTestCase {
    static let appBundleId = "dev.jarvis.app"

    @MainActor
    func testServeUntilStopped() async throws {
        try XCTSkipUnless(
            ProcessInfo.processInfo.environment["JARVIS_RUNNER_SERVE"] == "1",
            "set JARVIS_RUNNER_SERVE=1 (the JarvisRunner scheme does) to serve"
        )
        continueAfterFailure = true
        // Serve for days; the process is stopped from outside.
        executionTimeAllowance = 7 * 24 * 60 * 60

        let random = SystemRunnerRandom()
        let key = try RunnerKey.generate(random)
        let app = XCUIApplication()
        app.launchEnvironment = ProvisionedRunnerKeySource.environment(for: key)
        app.launch()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 30), "Jarvis did not reach the foreground")

        let runner = XCUITestPhoneRunner()
        runner.noteActivated(Self.appBundleId)
        let server = RunnerServer(runner: runner, key: key, random: random, onEvent: { event in
            // Kinds and reasons only: never the key, a tree, typed text or a URL.
            NSLog("JarvisRunner: %@", String(describing: event))
        })
        let listener = NWRunnerListener(server: server)
        let port = try await listener.start(port: JarvisRunnerDefaults.port)
        NSLog("JarvisRunner: serving on 127.0.0.1:%d", Int(port))

        // The main actor stays free while this sleeps, so the runner's XCUITest calls run.
        while !Task.isCancelled {
            try await Task.sleep(for: .seconds(60))
        }
        listener.stop()
        await server.stop()
    }
}
