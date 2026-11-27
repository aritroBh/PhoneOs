import Foundation
import Synchronization
import XCTest
import JarvisCore
import JarvisKit

/// The simulator proof of Jarvis's own runner protocol: the real `XCUITestPhoneRunner`
/// served by `RunnerServer` on a loopback port, driven by `JarvisRunnerClient` over real
/// sockets (both ends in this process; on a phone they are two processes). Checks that a
/// typed tree crosses, a tap acts, and that neither a stranger's client nor a stranger's
/// server gets anywhere.
final class JarvisRunnerLoopbackTests: QATestCase {
    /// A server that is not Jarvis's runner (another app holding a port): serves a forged
    /// tree to anyone who asks, and counts who did.
    private final class ForgedRunner: PhoneRunner {
        let calls = Mutex(0)

        func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
            calls.withLock { $0 += 1 }
            let root = UiNode(role: .button, label: "FORGED Approve", value: nil, identifier: "forged", placeholder: nil,
                              frame: Rect(x: 0, y: 0, width: 400, height: 800), enabled: true, visible: true, hittable: true,
                              focused: false, selected: false, secure: false, children: [])
            return RunnerObservation(root: root, appBundleId: "dev.jarvis.app", screenSize: Size(width: 400, height: 800))
        }

        func perform(_ command: RunnerCommand) async throws -> RunnerResult {
            calls.withLock { $0 += 1 }
            return RunnerResult(ok: true, error: nil)
        }
    }

    private static func find(_ root: UiNode, identifier: String) -> UiNode? {
        var stack = [root]
        while let node = stack.popLast() {
            if node.identifier == identifier { return node }
            stack.append(contentsOf: node.children)
        }
        return nil
    }

    private static func count(_ root: UiNode) -> Int {
        var stack = [root]
        var n = 0
        while let node = stack.popLast() {
            n += 1
            stack.append(contentsOf: node.children)
        }
        return n
    }

    @MainActor
    func testTypedTreeAndTapOverAuthenticatedLoopback() async throws {
        executionTimeAllowance = 150
        let app = launchPlayground()
        let random = SystemRunnerRandom()
        let key = try RunnerKey.generate(random)

        let runner = XCUITestPhoneRunner()
        runner.noteActivated(Self.appBundleId)
        // A slow simulator's whole-tree reads can take tens of seconds (QAHarness gives the
        // in-process runner 30 s): longer deadlines here, in the same order as the app's
        // (server < client).
        let server = RunnerServer(runner: runner, key: key, random: random,
                                  configuration: RunnerServer.Configuration(readDeadline: .seconds(40), commandDeadline: .seconds(40)))
        let listener = NWRunnerListener(server: server)
        let port = try await listener.start(port: 0)
        defer { listener.stop() }

        // 1. A typed tree of the playground.
        let client = JarvisRunnerClient(connector: NWRunnerConnector(port: port), keySource: FixedRunnerKeySource(key.reveal()), random: random,
                                        configuration: JarvisRunnerClient.Configuration(readTimeout: .seconds(45), commandTimeout: .seconds(45)))
        let first = try await client.snapshot(includeScreenshot: false)
        let root = try XCTUnwrap(first.root, "the runner sent text, not a typed tree")
        XCTAssertGreaterThan(Self.count(root), 3, "the typed tree has no elements")
        XCTAssertEqual(first.appBundleId, Self.appBundleId)
        let alerts = try XCTUnwrap(Self.find(root, identifier: "playground.alerts"), "no Alerts row in the typed tree")

        // 2. One tap through the protocol changes the screen.
        let frame = alerts.frame
        let result = try await client.perform(.tap(x: frame.x + frame.width / 2, y: frame.y + frame.height / 2))
        XCTAssertTrue(result.ok, "tap refused: \(result.error ?? "")")
        var opened = false
        for _ in 0..<20 where !opened {
            let now = try await client.snapshot(includeScreenshot: false)
            opened = now.root.flatMap { Self.find($0, identifier: "alerts.show") } != nil
            if !opened { try await Task.sleep(for: .milliseconds(500)) }
        }
        XCTAssertTrue(opened, "the Alerts screen did not appear in the typed tree after the tap")
        XCTAssertTrue(app.buttons["alerts.show"].waitForExistence(timeout: 5), "the tap did not open the Alerts screen")

        // 2b. A command planned on another app is refused by the runner next to the gesture.
        let refused = try await client.perform(.tap(x: 1, y: 1), expect: CommandExpect(bundleId: "com.apple.Preferences"))
        XCTAssertFalse(refused.ok, "a tap planned on Settings ran over the playground")
        XCTAssertEqual(refused.failure, .stale)

        // Free the one connection slot for the next client.
        await client.disconnect()
        try await waitUntilFree(server)

        // 3. A client with another key gets nothing.
        let stranger = JarvisRunnerClient(connector: NWRunnerConnector(port: port),
                                          keySource: FixedRunnerKeySource(try random.bytes(count: 32)), random: random)
        do {
            _ = try await stranger.perform(.tap(x: 1, y: 1))
            XCTFail("a client with the wrong key was served")
        } catch let error as JarvisRunnerError {
            XCTAssertEqual(error, .unauthenticated)
        }
        try await waitUntilFree(server)

        // 4. A server with another key (a squatter) is treated as absent.
        let forged = ForgedRunner()
        let squatter = RunnerServer(runner: forged, key: try RunnerKey.generate(random), random: random)
        let squatterListener = NWRunnerListener(server: squatter)
        let squatterPort = try await squatterListener.start(port: 0)
        defer { squatterListener.stop() }
        let fooled = JarvisRunnerClient(connector: NWRunnerConnector(port: squatterPort), keySource: FixedRunnerKeySource(key.reveal()), random: random)
        do {
            _ = try await fooled.snapshot(includeScreenshot: false)
            XCTFail("the client accepted a server that cannot prove the key")
        } catch let error as JarvisRunnerError {
            XCTAssertEqual(error, .unauthenticated)
        }
        XCTAssertEqual(forged.calls.withLock { $0 }, 0, "the client sent a request to the squatter")

        // The real runner still serves the right client afterwards.
        let again = try await client.snapshot(includeScreenshot: false)
        XCTAssertNotNil(again.root)
        await client.disconnect()
        await server.stop()
    }

    @MainActor
    private func waitUntilFree(_ server: RunnerServer) async throws {
        for _ in 0..<50 {
            if await !server.isServing { return }
            try await Task.sleep(for: .milliseconds(100))
        }
        XCTFail("the runner server did not free its connection")
    }
}
