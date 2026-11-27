import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

/// The confirmation sheet's question as lines: from the core's rows (carried untouched from
/// the session through `SessionDriver` and the human gate), never from the summary's text.
@Suite("ConfirmationQuestion", .timeLimit(.minutes(1)))
struct ConfirmationQuestionTests {
    private func row(_ kind: QuestionRowKind, _ text: String, _ display: String) -> QuestionRow {
        QuestionRow(kind: kind, text: text, display: display)
    }

    @Test("a request built from text alone is one plain line holding the summary")
    func textOnly() {
        let request = ConfirmRequest(category: .delete, summary: "Delete 3 photos", app: "x")
        #expect(request.rows.isEmpty && request.target == nil && request.stage == .whole && request.verb == .other)
        let question = ConfirmationQuestion(request)
        #expect(question.lines == [ConfirmationQuestion.Line(id: 0, kind: nil, label: nil, text: "Delete 3 photos", style: .plain)])
        #expect(question.summary == "Delete 3 photos")
        #expect(question.spoken == "Delete 3 photos")
    }

    @Test("each row family gets its style and label; the page's own text is set apart")
    func rowsMapToStyledLines() {
        let rows = [
            row(.warning, "Warning: this screen may not be what is on the phone. ", "Warning: this screen may not be what is on the phone."),
            row(.action, "Tap 'Pay' in Shop", "Tap 'Pay' in Shop"),
            row(.sends, ". Sends: Note: 'hi'", "Sends: Note: 'hi'"),
            row(.typed, ". Typed: 'hi'", "Typed: 'hi'"),
            row(.amount, ". Amounts on screen: Total $5.00", "Amounts on screen: Total $5.00"),
            row(.recipient, ". To/Account: To: '@bob'", "To/Account: To: '@bob'"),
            row(.notVerified, ". Recipient not verified", "Recipient not verified"),
            row(.screenSays, ". Screen says: Pay to: Mallory", "Screen says: Pay to: Mallory"),
            row(.selected, ". Selected: Express", "Selected: Express"),
            row(.fields, ". Fields: Name (filled)", "Fields: Name (filled)"),
            row(.note, ". Facts from the 250 nearest elements", "Facts from the 250 nearest elements"),
            row(.other, ". x", "x"),
        ]
        let summary = rows.map(\.text).joined()
        let request = ConfirmRequest(category: .payment, summary: summary, app: "com.example.shop", phoneOnly: false,
                                     rows: rows, stage: .whole, verb: .tap, target: nil)
        let question = ConfirmationQuestion(request)
        #expect(question.lines.map(\.text) == rows.map(\.display))
        #expect(question.lines.map(\.kind) == rows.map { Optional($0.kind) })
        #expect(question.lines.map(\.style) == [.warning, .action, .fact, .fact, .fact, .fact, .warning, .page, .plain, .plain, .plain, .plain])
        let page = question.lines[7]
        #expect(page.label == "From the page (not verified)")
        #expect(question.lines[5].label == "Recipient (from the form)")
        #expect(question.lines[1].label == nil)
        #expect(question.summary == summary)
        #expect(question.spoken.contains("From the page (not verified): Screen says: Pay to: Mallory"))
        #expect(Set(question.lines.map(\.id)).count == rows.count)
    }

    @Test("rows with no display text are left out; none left is the summary")
    func emptyRows() {
        let request = ConfirmRequest(category: .payment, summary: "Pay", app: "a", phoneOnly: false,
                                     rows: [row(.note, "", "")], stage: .whole, verb: .tap, target: nil)
        #expect(ConfirmationQuestion(request).lines.map(\.text) == ["Pay"])
    }

    @Test("the real core's questions reach the gate with their rows, stage, kind and target")
    func realCoreRows() async throws {
        let h = Harness()
        let (result, _) = try await h.run()
        #expect(result.outcome.status == .success)
        let confirms = h.gate.confirms
        #expect(confirms.count == 2)
        for request in confirms {
            #expect(!request.rows.isEmpty)
            #expect(request.rows.map(\.text).joined() == request.summary, "the summary is its rows joined")
            // Runner text: the warning is its own row, first.
            #expect(request.rows.first?.kind == .warning)
            #expect(request.rows.contains { $0.kind == .action })
            #expect(request.stage == .whole)
            #expect(request.verb == .tap)
            let target = try #require(request.target)
            #expect(target.element != nil && target.frame != nil && target.digest.count == 64)
            let question = ConfirmationQuestion(request)
            #expect(question.lines.first?.style == .warning)
            #expect(question.lines.contains { $0.style == .action })
            #expect(question.lines.allSatisfy { !$0.text.hasPrefix(". ") })
        }
    }
}

/// Sixteenth review: the flow question (Rule T) and the paste note (Rule P) have their own
/// row kinds; a question asked for the flow alone reads as that, not as sending a message.
@Suite("ConfirmationWording for flow and paste questions", .timeLimit(.minutes(1)))
struct FlowWordingTests {
    private static let flowText = ". Jarvis has read Messages in this task; what it read there could be passed on here"
    private static let pasteText = ". This enters content Jarvis cannot show (the clipboard or AutoFill) into Safari"

    private func row(_ kind: QuestionRowKind, _ text: String) -> QuestionRow {
        let display = text.hasPrefix(". ") ? String(text.dropFirst(2)) : text
        return QuestionRow(kind: kind, text: text, display: display)
    }

    private func request(_ category: RiskCategory, _ rows: [QuestionRow], flowOnly: Bool) -> ConfirmRequest {
        ConfirmRequest(category: category, summary: rows.map(\.text).joined(), app: "com.apple.mobilesafari", phoneOnly: true,
                       rows: rows, stage: .whole, verb: .tap, target: nil, flowOnly: flowOnly)
    }

    @Test("a question asked for the flow alone has its own title and names the app")
    func flowOnly() {
        let q = request(.sendMessage, [row(.action, "Tap 'Next' in Safari"), row(.flow, Self.flowText)], flowOnly: true)
        let wording = ConfirmationWording(q, appName: "Safari")
        #expect(wording.title == "Let Jarvis use what it read here?")
        #expect(wording.subtitle == "In Safari")
        #expect(wording.chip != riskCategoryTitle(.sendMessage))
        #expect(!wording.title.contains("Go ahead") && !wording.title.lowercased().contains("send"))
        // The sheet still shows the action and the flow note, and reads them as one element.
        let question = ConfirmationQuestion(q)
        #expect(question.lines.map(\.kind) == [.action, .flow])
        #expect(question.lines[1].label == "Read earlier" && question.lines[1].style == .warning)
        #expect(question.summary == q.summary)
    }

    @Test("a question with a category of its own keeps it and shows the flow as a labelled row")
    func merged() {
        let q = request(.payment, [row(.action, "Tap 'Pay' in Shop"), row(.amount, ". Amounts on screen: $5.00"), row(.flow, Self.flowText)], flowOnly: false)
        let wording = ConfirmationWording(q, appName: "Shop")
        #expect(wording.title == "Go ahead in Shop?")
        #expect(wording.chip == riskCategoryTitle(.payment))
        #expect(wording.subtitle == nil)
        let flow = ConfirmationQuestion(q).lines.last
        #expect(flow?.kind == .flow && flow?.label == "Read earlier" && flow?.style == .warning)
        // A real send question with a flow row is not worded as a flow question either.
        let send = request(.sendMessage, [row(.action, "Tap 'Send' in Messages"), row(.sends, ". Sends: 'hi'"), row(.flow, Self.flowText)], flowOnly: false)
        #expect(ConfirmationWording(send, appName: "Messages").title == "Go ahead in Messages?")
    }

    @Test("a paste question keeps its category title and labels the insert row")
    func paste() {
        let q = request(.sendMessage, [row(.action, "Tap 'Paste' in Safari"), row(.insert, Self.pasteText), row(.flow, Self.flowText)], flowOnly: false)
        let wording = ConfirmationWording(q, appName: "Safari")
        #expect(wording.title == "Go ahead in Safari?")
        #expect(wording.subtitle == nil)
        let lines = ConfirmationQuestion(q).lines
        #expect(lines.map(\.kind) == [.action, .insert, .flow])
        #expect(lines[1].label == "Entered without being shown" && lines[1].style == .warning)
        #expect(ConfirmationQuestion(q).spoken.contains("Entered without being shown: This enters content Jarvis cannot show"))
    }

    @Test("without the flag, a flow row alone does not change the wording (text-only requests)")
    func noFlag() {
        let q = request(.sendMessage, [row(.action, "Tap 'Next' in Safari"), row(.flow, Self.flowText)], flowOnly: false)
        #expect(ConfirmationWording(q, appName: "Safari").title == "Go ahead in Safari?")
        let plain = ConfirmRequest(category: .sendMessage, summary: "Tap Send", app: "x")
        #expect(plain.flowOnly == false)
    }
}
