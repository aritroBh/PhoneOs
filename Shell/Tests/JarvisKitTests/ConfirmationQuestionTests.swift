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
