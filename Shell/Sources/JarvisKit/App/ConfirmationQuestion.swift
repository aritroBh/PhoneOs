import Foundation
import JarvisCore

/// The confirmation sheet's question, as lines: the core's own parts of it
/// (`ConfirmRequest.rows`, written by the code that writes the question, so they are exactly
/// the summary), each with the style its family calls for. Nothing here reads the summary's
/// text to find its parts; a request without rows (built from text alone) is one plain line
/// holding the summary.
///
/// Provenance: `Screen says:` lines are text the page shows (the gate did not verify them),
/// so they are styled apart, labelled "From the page"; `To/Account:` holds the form's
/// recipient fields the action sends; warnings and `Recipient not verified` are warnings.
public struct ConfirmationQuestion: Sendable, Equatable {
    /// How a line is drawn.
    public enum Style: Sendable, Equatable {
        /// What Jarvis will do: prominent.
        case action
        /// A fact the gate read for this question (what is sent, the recipient fields, the
        /// amounts, what Jarvis typed): a labelled row.
        case fact
        /// What the page itself says: secondary, labelled as from the page.
        case page
        /// A warning (the unverified screen, a recipient that is not verified).
        case warning
        /// Anything else, and the summary of a request without rows.
        case plain
    }

    /// One line of the question.
    public struct Line: Sendable, Equatable, Identifiable {
        /// Its position in the question.
        public var id: Int
        /// The core's row family (`nil` for the summary of a request without rows).
        public var kind: QuestionRowKind?
        /// A short label above the text (`nil`: none).
        public var label: String?
        /// The text (the row without its joining separator).
        public var text: String
        /// How it is drawn.
        public var style: Style
    }

    /// The lines in reading order.
    public var lines: [Line]
    /// The whole question as the core wrote it (the sheet's accessibility value).
    public var summary: String

    public init(_ request: ConfirmRequest) {
        summary = request.summary
        let rows = request.rows.filter { !$0.display.isEmpty }
        if rows.isEmpty {
            lines = [Line(id: 0, kind: nil, label: nil, text: request.summary, style: .plain)]
            return
        }
        lines = rows.enumerated().map { index, row in
            let (label, style) = Self.presentation(of: row.kind)
            return Line(id: index, kind: row.kind, label: label, text: row.display, style: style)
        }
    }

    /// The label and style of a row family.
    static func presentation(of kind: QuestionRowKind) -> (String?, Style) {
        switch kind {
        case .action: return (nil, .action)
        case .warning: return (nil, .warning)
        case .sends: return ("What is sent", .fact)
        case .typed: return ("Typed by Jarvis", .fact)
        case .amount: return ("Amounts", .fact)
        case .recipient: return ("Recipient (from the form)", .fact)
        case .notVerified: return ("Check the recipient", .warning)
        case .screenSays: return ("From the page (not verified)", .page)
        case .selected: return ("Selected", .plain)
        case .fields: return ("Other fields", .plain)
        case .note: return (nil, .plain)
        // Rule T: what Jarvis read earlier in this task could be passed on by this input. A
        // question for the flow alone is worded by `ConfirmationWording`; in a question that
        // asks about something else this is the labelled row that says what else is at stake.
        case .flow: return ("Read earlier", .warning)
        // Rule P: a paste or AutoFill control enters content the question cannot show.
        case .insert: return ("Entered without being shown", .warning)
        case .other: return (nil, .plain)
        }
    }

    /// What VoiceOver reads for the whole question: each line with its label, in order.
    public var spoken: String {
        lines.map { line in
            if let label = line.label { return "\(label): \(line.text)" }
            return line.text
        }
        .joined(separator: "\n")
    }
}

extension ConfirmRequest {
    /// A request built from its text alone (no rows, no stated part or target), as tests and
    /// older call sites build one.
    public init(category: RiskCategory, summary: String, app: String, phoneOnly: Bool = false) {
        self.init(category: category, summary: summary, app: app, phoneOnly: phoneOnly,
                  rows: [], stage: .whole, verb: .other, target: nil)
    }
}
