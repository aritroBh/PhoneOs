import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// A LoopCtrl-shaped JSON-RPC server: answers per method from a table.
private final class LoopCtrlStub: Sendable {
    typealias Reply = @Sendable (_ params: JSONValue) -> StubHTTPServer.Response

    let server: StubHTTPServer

    init(_ replies: [String: Reply]) throws {
        server = try StubHTTPServer { request in
            guard let body = try? request.json(), let method = body["method"]?.stringValue else {
                return .text("bad request", status: 400)
            }
            guard let reply = replies[method] else {
                return .json(["jsonrpc": "2.0", "id": body["id"] ?? .null,
                              "error": ["code": -32601, "message": .string("unknown method \(method)")]])
            }
            return reply(body["params"] ?? .null)
        }
    }

    /// The JSON-RPC bodies received, in order.
    func bodies() throws -> [JSONValue] { try server.requests.map { try $0.json() } }

    func methods() throws -> [String] { try bodies().compactMap { $0["method"]?.stringValue } }

    func runner(_ configure: (inout LoopCtrlRunner.Configuration) -> Void = { _ in }, sealer: any RequestSealer = PlaintextSealer()) -> LoopCtrlRunner {
        var configuration = LoopCtrlRunner.Configuration(baseURL: server.baseURL, requestTimeout: 5)
        configure(&configuration)
        return LoopCtrlRunner(configuration: configuration, sealer: sealer)
    }

    deinit { server.stop() }
}

private func ok(_ result: JSONValue) -> StubHTTPServer.Response {
    .json(["jsonrpc": "2.0", "id": 1, "result": result])
}

private func rpcError(_ code: Int, _ message: String) -> StubHTTPServer.Response {
    .json(["jsonrpc": "2.0", "id": 1, "error": ["code": .number(Double(code)), "message": .string(message)]])
}

private let jpegBytes = Data([0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10])
private let treeText = FakePhone.text(for: .settings)

private let standardReplies: [String: LoopCtrlStub.Reply] = [
    "page_source": { _ in ok(["format": "text", "source": .string(treeText), "keyboard": ["visible": false, "covers_from_y": 852]]) },
    "device_info": { _ in ok(["system": ["foreground_app": "com.apple.Preferences"], "orientation": "portrait"]) },
    "screenshot": { _ in ok([
        "format": "jpeg", "encoding": "base64", "data": .string(jpegBytes.base64EncodedString()),
        "pixel_width": 786, "pixel_height": 1704, "point_width": 393, "point_height": 852, "scale": 2,
    ]) },
]

private func replies(_ overrides: [String: LoopCtrlStub.Reply]) -> [String: LoopCtrlStub.Reply] {
    standardReplies.merging(overrides) { _, new in new }
}

/// One command and the request it must become.
struct CommandCase: Sendable, CustomTestStringConvertible {
    var command: RunnerCommand
    var method: String
    var params: JSONValue
    var testDescription: String { "\(command) -> \(method)" }
}

@Suite("LoopCtrlRunner against a stub JSON-RPC server")
struct LoopCtrlRunnerTests {
    @Test("snapshot: page_source + device_info, screen size learned once from a screenshot")
    func snapshotShape() async throws {
        let stub = try LoopCtrlStub(standardReplies)
        let runner = stub.runner { $0.appNames = [settingsBundle: "Settings"] }

        let first = try await runner.snapshot(includeScreenshot: false)
        #expect(first.source == treeText)
        #expect(first.appBundleId == settingsBundle)
        #expect(first.appName == "Settings")
        #expect(first.screenSize == Size(width: 393, height: 852))
        #expect(first.keyboardVisible == false)
        #expect(first.orientation == .portrait)
        #expect(first.screenshot == nil)
        #expect(first.notes.contains { $0.contains("learned from a screenshot") })
        #expect(try stub.methods() == ["page_source", "device_info", "screenshot"])

        let second = try await runner.snapshot(includeScreenshot: false)
        #expect(second.notes.isEmpty)
        #expect(try stub.methods() == ["page_source", "device_info", "screenshot", "page_source", "device_info"])

        let request = try #require(stub.server.requests.first)
        #expect(request.method == "POST")
        #expect(request.path == "/rpc")
        #expect(request.headers["content-type"] == "application/json")
        let body = try request.json()
        #expect(body["jsonrpc"] == "2.0")
        #expect(body["id"]?.numberValue != nil)
        #expect(body["params"] == ["visible_only": true, "max_children": 50])
        let ids = try stub.bodies().compactMap { $0["id"]?.numberValue }
        #expect(Set(ids).count == ids.count, "request ids are unique")
    }

    @Test("snapshot with a screenshot decodes the image and its sizes")
    func snapshotWithScreenshot() async throws {
        let stub = try LoopCtrlStub(standardReplies)
        let observation = try await stub.runner().snapshot(includeScreenshot: true)
        let shot = try #require(observation.screenshot)
        #expect(shot.data == jpegBytes)
        #expect(shot.mimeType == "image/jpeg")
        #expect(shot.pixelWidth == 786 && shot.pixelHeight == 1704)
        #expect(shot.scale == 2)
        #expect(observation.screenSize == Size(width: 393, height: 852))
        #expect(try stub.methods() == ["page_source", "device_info", "screenshot"])
    }

    @Test("a configured screen size needs no screenshot")
    func configuredSize() async throws {
        let stub = try LoopCtrlStub(standardReplies)
        let observation = try await stub.runner { $0.screenSize = Size(width: 440, height: 956) }.snapshot(includeScreenshot: false)
        #expect(observation.screenSize == Size(width: 440, height: 956))
        #expect(try stub.methods() == ["page_source", "device_info"])
    }

    @Test("lenient decoding: object foreground_app, missing keyboard and orientation become notes")
    func lenientDecoding() async throws {
        let tree = treeText + "\n  Keyboard, {{0, 500}, {393, 352}}"
        let stub = try LoopCtrlStub(replies([
            "page_source": { _ in ok(["source": .string(tree)]) },
            "device_info": { _ in ok(["system": ["foreground_app": ["bundle_id": "com.apple.mobilesafari", "name": "Safari"]]]) },
        ]))
        let observation = try await stub.runner { $0.screenSize = screenSize }.snapshot(includeScreenshot: false)
        #expect(observation.appBundleId == "com.apple.mobilesafari")
        #expect(observation.appName == "Safari")
        #expect(observation.keyboardVisible, "sniffed from the Keyboard line")
        #expect(observation.orientation == .portrait)
        #expect(observation.notes.count == 3)
        #expect(observation.notes.contains { $0.contains("no 'format'") })
        #expect(observation.notes.contains { $0.contains("judged from the tree") })
        #expect(observation.notes.contains { $0.contains("assuming portrait") })
    }

    @Test("landscape orientation strings are mapped")
    func orientation() async throws {
        let stub = try LoopCtrlStub(replies([
            "device_info": { _ in ok(["system": ["foreground_app": "com.example"], "orientation": "landscapeLeft"]) },
        ]))
        let observation = try await stub.runner { $0.screenSize = screenSize }.snapshot(includeScreenshot: false)
        #expect(observation.orientation == .landscapeLeft)
        #expect(observation.notes.isEmpty)
    }

    @Test("unknown shapes are typed errors, not guesses")
    func typedShapeErrors() async throws {
        let cases: [([String: LoopCtrlStub.Reply], LoopCtrlError)] = [
            (["page_source": { _ in ok(["format": "text"]) }],
             .missingField(method: "page_source", field: "source")),
            (["page_source": { _ in ok(["format": "json", "source": "{}"]) }],
             .unexpectedValue(method: "page_source", field: "format", found: "\"json\"")),
            (["page_source": { _ in ok(["format": "text", "source": 3]) }],
             .unexpectedValue(method: "page_source", field: "source", found: "number")),
            (["device_info": { _ in ok(["system": [:]]) }],
             .missingField(method: "device_info", field: "system.foreground_app")),
            (["device_info": { _ in ok(["system": ["foreground_app": 7]]) }],
             .unexpectedValue(method: "device_info", field: "system.foreground_app", found: "7")),
            (["screenshot": { _ in ok(["data": "!!!not base64!!!", "pixel_width": 1, "pixel_height": 1]) }],
             .unexpectedValue(method: "screenshot", field: "data", found: "not base64 (string)")),
            (["screenshot": { _ in ok(["data": .string(jpegBytes.base64EncodedString())]) }],
             .missingField(method: "screenshot", field: "pixel_width")),
        ]
        for (overrides, expected) in cases {
            let stub = try LoopCtrlStub(replies(overrides))
            await #expect(throws: expected) {
                _ = try await stub.runner().snapshot(includeScreenshot: true)
            }
        }
    }

    static let commandCases: [CommandCase] = [
        CommandCase(command: .tap(x: 196.5, y: 222), method: "tap", params: ["x": 196.5, "y": 222]),
        CommandCase(command: .longPress(x: 10, y: 20, durationMs: 800), method: "long_press", params: ["x": 10, "y": 20, "duration": 0.8]),
        CommandCase(command: .swipe(from: Point(x: 200, y: 700), to: Point(x: 200, y: 200), durationMs: 300), method: "drag",
                    params: ["from_x": 200, "from_y": 700, "to_x": 200, "to_y": 200, "duration": 0.3]),
        CommandCase(command: .typeText(text: "hello, 'world'"), method: "type", params: ["text": "hello, 'world'"]),
        CommandCase(command: .pressKey(key: .return), method: "type", params: ["text": "", "submit": true]),
        CommandCase(command: .pressKey(key: .delete), method: "type", params: ["text": "\u{8}"]),
        CommandCase(command: .pressKey(key: .tab), method: "type", params: ["text": "\t"]),
        CommandCase(command: .pressKey(key: .space), method: "type", params: ["text": " "]),
        CommandCase(command: .pressKey(key: .escape), method: "type", params: ["text": "\u{1B}"]),
        CommandCase(command: .pressKey(key: .home), method: "home", params: [:]),
        CommandCase(command: .launchApp(bundleId: "com.apple.mobilesafari"), method: "launch_app", params: ["bundle_id": "com.apple.mobilesafari"]),
        CommandCase(command: .openUrl(url: "https://example.com/a?b=c"), method: "open_url", params: ["url": "https://example.com/a?b=c"]),
        CommandCase(command: .wait(ms: 1_500), method: "wait", params: ["duration": 1.5]),
    ]

    @Test("each RunnerCommand maps to its JSON-RPC request", arguments: commandCases)
    func commandMapping(_ c: CommandCase) async throws {
        let stub = try LoopCtrlStub([c.method: { _ in ok(["message": "done"]) }])
        let result = try await stub.runner().perform(c.command)
        #expect(result == RunnerResult(ok: true, error: nil))
        let body = try #require(try stub.bodies().first)
        #expect(body["method"] == .string(c.method))
        #expect(body["params"] == c.params)
    }

    @Test("activate_app launch strategy")
    func activateStrategy() async throws {
        let stub = try LoopCtrlStub(["activate_app": { _ in ok([:]) }])
        let result = try await stub.runner { $0.launchStrategy = .activateApp }.perform(.launchApp(bundleId: "com.example"))
        #expect(result.ok)
        #expect(try stub.methods() == ["activate_app"])
    }

    @Test("a business error is a failed RunnerResult")
    func businessError() async throws {
        let stub = try LoopCtrlStub(["tap": { _ in rpcError(-32000, "element not hittable") }])
        let result = try await stub.runner().perform(.tap(x: 1, y: 2))
        #expect(result == RunnerResult(ok: false, error: "element not hittable (tap error -32000)"))
    }

    @Test("an unknown method is a failed RunnerResult")
    func unknownMethod() async throws {
        let stub = try LoopCtrlStub([:])
        let result = try await stub.runner().perform(.pressKey(key: .home))
        #expect(!result.ok)
        #expect(result.error?.contains("-32601") == true)
    }

    @Test("an auth error asks for pairing")
    func authError() async throws {
        let stub = try LoopCtrlStub(["tap": { _ in rpcError(-32099, "bad envelope") }])
        await #expect(throws: LoopCtrlError.authenticationRequired("bad envelope")) {
            _ = try await stub.runner().perform(.tap(x: 1, y: 2))
        }
    }

    @Test("snapshot calls turn runner errors into rpcError")
    func snapshotRPCError() async throws {
        let stub = try LoopCtrlStub(replies(["page_source": { _ in rpcError(-32603, "snapshot failed") }]))
        await #expect(throws: LoopCtrlError.rpcError(method: "page_source", code: -32603, message: "snapshot failed")) {
            _ = try await stub.runner().snapshot(includeScreenshot: false)
        }
    }

    @Test("HTTP and body errors are typed")
    func httpErrors() async throws {
        let s500 = try LoopCtrlStub(["tap": { _ in .text("boom", status: 500) }])
        await #expect(throws: LoopCtrlError.httpStatus(500, body: "boom")) {
            _ = try await s500.runner().perform(.tap(x: 1, y: 2))
        }
        let garbage = try LoopCtrlStub(["tap": { _ in .text("<html>", status: 200) }])
        await #expect(throws: LoopCtrlError.malformedResponse(method: "tap", reason: "not a JSON object: <html>")) {
            _ = try await garbage.runner().perform(.tap(x: 1, y: 2))
        }
    }

    @Test("an unreachable runner is a transport error")
    func unreachable() async throws {
        let stub = try LoopCtrlStub([:])
        let runner = stub.runner()
        stub.server.stop()
        await #expect {
            _ = try await runner.perform(.tap(x: 1, y: 2))
        } throws: { error in
            if case LoopCtrlError.transport = error { return true }
            return false
        }
    }

    @Test("a runner that does not answer times out")
    func timeout() async throws {
        let stub = try LoopCtrlStub(["tap": { _ in .json(["jsonrpc": "2.0", "id": 1, "result": [:]], delay: 3) }])
        let runner = stub.runner { $0.requestTimeout = 0.3 }
        let started = ContinuousClock.now
        await #expect {
            _ = try await runner.perform(.tap(x: 1, y: 2))
        } throws: { error in
            if case LoopCtrlError.transport = error { return true }
            return false
        }
        #expect(ContinuousClock.now - started < .seconds(2.5))
    }

    @Test("the sealer wraps requests and unwraps responses")
    func sealer() async throws {
        struct Base64Sealer: RequestSealer {
            func seal(body: Data, into request: URLRequest) async throws -> URLRequest {
                var request = request
                request.setValue("test-1", forHTTPHeaderField: "X-Seal")
                request.httpBody = try JSONValue.object(["ct": .string(body.base64EncodedString())]).encoded()
                return request
            }

            func unseal(body: Data, response: HTTPURLResponse) async throws -> Data {
                guard let ct = try JSONValue(parsing: body)["ct"]?.stringValue, let plain = Data(base64Encoded: ct) else {
                    throw InjectedError(message: "not sealed")
                }
                return plain
            }
        }
        let server = try StubHTTPServer { request in
            guard request.headers["x-seal"] == "test-1",
                  let ct = try? request.json()["ct"]?.stringValue,
                  let inner = Data(base64Encoded: ct),
                  let method = try? JSONValue(parsing: inner)["method"]?.stringValue, method == "home"
            else { return .text("unsealed request", status: 400) }
            let reply = (try? JSONValue.object(["jsonrpc": "2.0", "id": 1, "result": ["message": "ok"]]).encoded()) ?? Data()
            return .json(["ct": .string(reply.base64EncodedString())])
        }
        defer { server.stop() }
        let runner = LoopCtrlRunner(configuration: .init(baseURL: server.baseURL, requestTimeout: 5), sealer: Base64Sealer())
        #expect(try await runner.perform(.pressKey(key: .home)).ok)
    }

    @Test("LoopCtrlRunner drives the real core end to end")
    func endToEndWithDriver() async throws {
        // A stateful stub: the Settings app of FakePhone, served over JSON-RPC.
        let phone = FakePhone()
        let stub = try LoopCtrlStub(replies([
            "page_source": { _ in ok(["format": "text", "source": .string(FakePhone.text(for: phone.page)), "keyboard": ["visible": false]]) },
            "tap": { params in
                let x = params["x"]?.numberValue ?? -1
                let y = params["y"]?.numberValue ?? -1
                phone.apply(.tap(x: x, y: y))
                return ok(["message": "tapped"])
            },
        ]))
        let h = Harness()
        let config = try SessionConfigBuilder.make(capabilities: h.model.capabilities, knownApps: [AppInfo(bundleId: settingsBundle, name: "Settings")])
        let deps = DriverDependencies(runner: stub.runner(), model: h.model, tools: h.tools, human: h.gate, memory: h.memory, screenshots: h.screenshots)
        let driver = try SessionDriver(config: config, task: h.task, memoryJSON: "", dependencies: deps)
        let result = await driver.run()
        #expect(result.outcome.status == .success)
        #expect(phone.page == .wifiOff)
        #expect(try stub.methods().filter { $0 == "tap" }.count == 2)
    }
}
