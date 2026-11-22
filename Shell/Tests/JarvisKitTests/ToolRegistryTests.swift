import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

@Suite("ToolRegistry")
struct ToolRegistryTests {
    private func registry() -> ToolRegistry {
        let r = ToolRegistry()
        SampleTools.registerEcho(in: r)
        SampleTools.registerClockNow(in: r, now: { Date(timeIntervalSince1970: 86_400) }, timeZone: TimeZone(secondsFromGMT: 3_600) ?? .current)
        return r
    }

    @Test("specs in registration order; replacing by name keeps one")
    func specs() async {
        let r = registry()
        #expect(await r.toolSpecs().map(\.name) == ["echo", "clock_now"])
        r.register(ToolSpecs.spec(name: "echo", description: "v2")) { _ in "v2" }
        #expect(r.specs.map(\.name) == ["clock_now", "echo"])
        #expect(await r.execute(ToolCall(name: "echo", argsJson: "{}")).output == "v2")
    }

    @Test("sample tools")
    func samples() async {
        let r = registry()
        #expect(await r.execute(ToolCall(name: "echo", argsJson: #"{"text":"hi"}"#)) == ToolResult(ok: true, output: "hi", error: nil))
        let clock = await r.execute(ToolCall(name: "clock_now", argsJson: ""))
        #expect(clock.ok)
        #expect(clock.output.hasPrefix("1970-01-02T01:00:00+01:00"))
    }

    @Test("failures are results, never throws")
    func failures() async {
        let r = registry()
        r.register(ToolSpecs.spec(name: "boom", description: "fails")) { _ in throw InjectedError(message: "kaput") }
        r.register(ToolSpecs.spec(name: "typed", description: "fails")) { _ in throw ToolError.failed("no permission") }
        let cases: [(ToolCall, String)] = [
            (ToolCall(name: "nope", argsJson: "{}"), "unknown tool 'nope'"),
            (ToolCall(name: "echo", argsJson: "{bad"), "invalid arguments: arguments are not valid JSON"),
            (ToolCall(name: "echo", argsJson: "[1]"), "invalid arguments: arguments must be a JSON object, got array"),
            (ToolCall(name: "echo", argsJson: "{}"), "invalid arguments: missing required parameter(s): text"),
            (ToolCall(name: "echo", argsJson: #"{"text":[1]}"#), "invalid arguments: text must be a string"),
            (ToolCall(name: "boom", argsJson: "{}"), "kaput"),
            (ToolCall(name: "typed", argsJson: "{}"), "no permission"),
        ]
        for (call, message) in cases {
            #expect(await r.execute(call) == ToolResult(ok: false, output: "", error: message))
        }
    }

    @Test("argument accessors convert leniently")
    func arguments() throws {
        let args = try ToolArguments(json: #"{"n":"2.5","i":3,"b":true,"s":"x"}"#)
        #expect(args.number("n") == 2.5)
        #expect(args.string("i") == "3")
        #expect(args.string("b") == "true")
        #expect(args.number("s") == nil)
        #expect(args.string("missing") == nil)
    }
}
