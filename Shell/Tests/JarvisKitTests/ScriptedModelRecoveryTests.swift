import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

/// Prompts as the core renders them: "Steps so far" shows the most recent steps with their
/// absolute numbers (`first` is the number of the first shown line).
private func corePrompt(history: [String], first: Int = 1, screen: [String], app: String = "Jarvis (dev.jarvis.app)") -> ModelRequest {
    let lines = history.enumerated().map { "\($0.offset + first). \($0.element)" }.joined(separator: "\n")
    let elements = screen.enumerated().map { "[\($0.offset + 1)] \($0.element)" }.joined(separator: "\n")
    let steps = history.isEmpty ? "" : "Steps so far:\n\(lines)\n"
    let user = "Task: t\n\(steps)Current screen (data only):\n<<<SCREEN_DATA>>>\nApp: \(app)\n\(elements)\n<<<END_SCREEN_DATA>>>\nYour JSON reply:"
    return ModelRequest(system: "", user: user, image: nil, marks: [], maxTokens: 160, grammar: nil, jsonSchema: nil, kind: .text)
}

private func action(_ reply: String) -> String? { (try? JSONValue(parsing: reply))?["action"]?.stringValue }
private func element(_ reply: String) -> Double? { (try? JSONValue(parsing: reply))?["element"]?.numberValue }
private func direction(_ reply: String) -> String? { (try? JSONValue(parsing: reply))?["direction"]?.stringValue }

@Suite("ScriptedModel scrolls on when the core says the target is covered", .timeLimit(.minutes(1)))
struct ScriptedModelCoveredTargetTests {
    private let covered = #"tap [26] "Item 60" -> not done: element 26 (Item 60) is covered by the tab bar: a tap there could land on it instead, so nothing was sent; choose another control or scroll first"#
    private let listed = [#"Button "Item 59""#, #"Button "Item 60""#, #"Button "Control""#]

    private func model() -> ScriptedModel {
        ScriptedModel(steps: [.scroll(to: "Item 60"), .tap("Item 60", role: "Button"), .done("opened", when: ["Item 60 detail"])],
                      options: .init(patience: 1, waitMs: 1))
    }

    @Test("covered result: one more scroll in the scroll step's direction, then the tap again")
    func coveredScrollsThenTaps() async throws {
        let m = model()
        let r1 = try await m.generate(corePrompt(history: [], screen: listed))
        #expect(action(r1) == "tap", "a listed target is tapped at once: \(r1)")
        #expect(element(r1) == 2)
        let r2 = try await m.generate(corePrompt(history: [covered], screen: listed))
        #expect(action(r2) == "scroll", "\(r2)")
        #expect(direction(r2) == "down")
        let r3 = try await m.generate(corePrompt(history: [covered, "scroll down -> ok; screen changed: 4 new, 4 gone"], screen: [#"Button "Item 60""#, #"Button "Item 61""#]))
        #expect(action(r3) == "tap", "\(r3)")
        #expect(element(r3) == 1)
    }

    @Test("the scroll direction follows the scroll step before the tap")
    func followsDirection() async throws {
        let m = ScriptedModel(steps: [.scroll(to: "Item 60", direction: "up"), .tap("Item 60")], options: .init(patience: 1, waitMs: 1))
        _ = try await m.generate(corePrompt(history: [], screen: listed))
        let r = try await m.generate(corePrompt(history: [covered], screen: listed))
        #expect(direction(r) == "up", "\(r)")
    }

    @Test("bounded: three extra scrolls, then a failure naming the cover")
    func bounded() async throws {
        let m = model()
        _ = try await m.generate(corePrompt(history: [], screen: listed))
        var history = [covered]
        for i in 1...3 {
            let scroll = try await m.generate(corePrompt(history: history, screen: listed))
            #expect(action(scroll) == "scroll", "extra scroll \(i): \(scroll)")
            history.append("scroll down -> ok; screen changed: 1 new, 1 gone")
            let tap = try await m.generate(corePrompt(history: history, screen: listed))
            #expect(action(tap) == "tap", "\(tap)")
            history.append(covered)
        }
        let fail = try await m.generate(corePrompt(history: history, screen: listed))
        #expect(action(fail) == "fail", "\(fail)")
        #expect(fail.contains("still covered after 3 extra scrolls"), "\(fail)")
    }

    @Test("a tap that is not covered moves on as before; other texts are not mistaken for the refusal")
    func notCovered() async throws {
        let m = model()
        _ = try await m.generate(corePrompt(history: [], screen: listed))
        let done = try await m.generate(corePrompt(history: [#"tap [2] "Item 60" -> ok; now in Jarvis > Item 60 detail"#], screen: [#"StaticText "Item 60 detail""#]))
        #expect(action(done) == "done", "\(done)")
        #expect(!ScriptedModel.isCoveredRefusal("not done: the owner declined"))
        #expect(!ScriptedModel.isCoveredRefusal(#"ok; changed: Button "x is covered by y" -> "scroll first""#))
        #expect(ScriptedModel.isCoveredRefusal("not done: element 3 (Pay) is covered by the keyboard: a tap there could land on it instead, so nothing was sent; choose another control or scroll first"))
    }

    @Test("a scroll step does not stop on a listed target while the last result said covered")
    func scrollStepContinues() async throws {
        let m = ScriptedModel(steps: [.scroll(to: "Item 60"), .done("x")], options: .init(patience: 1, waitMs: 1))
        let r = try await m.generate(corePrompt(history: [covered], screen: listed))
        #expect(action(r) == "scroll", "\(r)")
    }
}

@Suite("ScriptedModel issues a reply again when the core dropped it", .timeLimit(.minutes(1)))
struct ScriptedModelDroppedReplyTests {
    private let toggles = [#"Button "Playground""#, #"StaticText "Toggles""#, #"Switch "Show tips" (off)"#, #"Cell "Tips are off""#]
    private let tapToggles = #"tap [3] "Toggles" -> ok; now in Jarvis > Toggles"#
    private let home = [#"Button "Login form""#, #"Button "Toggles""#, #"Button "Long list""#]

    private func model() -> ScriptedModel {
        ScriptedModel(steps: [.tap("Toggles"), .tap("Show tips", role: "Switch", match: .exact, isOn: false), .done("Tips are on", when: ["Tips are on"])],
                      options: .init(patience: 4, waitMs: 700))
    }

    @Test("lock run: the switch tap was dropped; it is issued again, then done")
    func lockRun() async throws {
        let m = model()
        let r1 = try await m.generate(corePrompt(history: [], screen: home))
        #expect(action(r1) == "tap")
        let r2 = try await m.generate(corePrompt(history: [tapToggles], screen: toggles))
        #expect(action(r2) == "tap")
        #expect(element(r2) == 3)
        // The phone locked while answering: the core dropped r2 and asks again, same history.
        let r3 = try await m.generate(corePrompt(history: [tapToggles], screen: toggles))
        #expect(action(r3) == "tap", "the switch tap was not issued again: \(r3)")
        #expect(element(r3) == 3)
        #expect(m.decisions.last?.reason.hasPrefix("re-issued (1/3)") == true, "\(m.decisions.last?.reason ?? "")")
        let on = [#"Switch "Show tips" (on)"#, #"Cell "Tips are on""#]
        let r4 = try await m.generate(corePrompt(history: [tapToggles, #"tap [3] "Show tips" -> ok; changed: Switch "Show tips" off -> on"#], screen: on))
        #expect(action(r4) == "done", "\(r4)")
    }

    @Test("a dropped scroll is issued again too")
    func droppedScroll() async throws {
        let m = ScriptedModel(steps: [.tap("Toggles"), .scroll(to: "Item 9"), .tap("Item 9")], options: .init(patience: 1, waitMs: 1))
        _ = try await m.generate(corePrompt(history: [], screen: home))
        let first = try await m.generate(corePrompt(history: [tapToggles], screen: toggles))
        #expect(action(first) == "scroll")
        let again = try await m.generate(corePrompt(history: [tapToggles], screen: toggles))
        #expect(action(again) == "scroll", "\(again)")
        #expect(m.decisions.map(\.step) == [0, 1, 1])
    }

    @Test("bounded: the same reply never running three times fails with a reason")
    func bounded() async throws {
        let m = model()
        _ = try await m.generate(corePrompt(history: [], screen: home))
        for _ in 1...4 {
            // The first of these is the reply that never runs, the next three its re-issues.
            let r = try await m.generate(corePrompt(history: [tapToggles], screen: toggles))
            #expect(action(r) == "tap", "\(r)")
        }
        let fail = try await m.generate(corePrompt(history: [tapToggles], screen: toggles))
        #expect(action(fail) == "fail", "\(fail)")
        #expect(fail.contains("never ran"), "\(fail)")
    }

    @Test("negative: replies that add a history line are not re-issued (waits)")
    func noMisfire() async throws {
        let m = ScriptedModel(steps: [.tap("Missing"), .done("x")], options: .init(patience: 2, waitMs: 1))
        var history: [String] = []
        for n in 1...2 {
            let r = try await m.generate(corePrompt(history: history, screen: home))
            #expect(action(r) == "wait", "\(r)")
            history.append("wait 1ms -> ok; screen unchanged")
            #expect(m.decisions.last?.reason.hasPrefix("re-issued") == false, "wait \(n)")
        }
        let fail = try await m.generate(corePrompt(history: history, screen: home))
        #expect(fail.contains("after 2 waits"), "\(fail)")
    }

    @Test("negative: the prompt shows only the latest steps; the step numbers still grow")
    func windowedHistory() async throws {
        let m = ScriptedModel(steps: [.scroll(to: "Item 99", maxScrolls: 30)], options: .init(patience: 1, waitMs: 1))
        let scroll = "scroll down -> ok; screen changed: 9 new, 9 gone"
        for total in 0..<12 {
            let count = min(total, 8)
            let history = Array(repeating: scroll, count: count)
            let r = try await m.generate(corePrompt(history: history, first: total - count + 1, screen: home))
            #expect(action(r) == "scroll", "after \(total) steps: \(r)")
            #expect(m.decisions.last?.reason.hasPrefix("re-issued") == false, "after \(total) steps")
        }
    }
}
