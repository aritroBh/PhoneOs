import Foundation

/// The screen as a model reads it: the element lines of the data block in a prompt's user
/// part (`[17] Button "Sign in"`, `[4] TextField "Email" value="a@b.c" (focused)`), plus the
/// header and the step history. Used by `ScriptedModel` to answer the way a model would (by
/// reading the rendered screen, never by peeking at the tree), and by QA artifacts.
public struct PromptScreen: Sendable, Equatable {
    /// One `[N] Role "label" …` line.
    public struct Element: Sendable, Equatable {
        public var index: UInt32
        public var role: String
        public var label: String
        public var value: String?
        public var placeholder: String?
        public var flags: [String]

        public init(index: UInt32, role: String, label: String, value: String? = nil, placeholder: String? = nil, flags: [String] = []) {
            self.index = index
            self.role = role
            self.label = label
            self.value = value
            self.placeholder = placeholder
            self.flags = flags
        }

        /// The line as the core renders it (for diagnostics).
        public var line: String {
            var s = "[\(index)] \(role)"
            if !label.isEmpty { s += " \"\(label)\"" }
            if let value { s += " value=\"\(value)\"" } else if let placeholder { s += " placeholder=\"\(placeholder)\"" }
            if !flags.isEmpty { s += " (\(flags.joined(separator: ", ")))" }
            return s
        }
    }

    /// Opening marker of the screen data block (jarvis-safety `SCREEN_BLOCK_START`).
    public static let blockStart = "<<<SCREEN_DATA>>>"
    /// Closing marker (jarvis-safety `SCREEN_BLOCK_END`).
    public static let blockEnd = "<<<END_SCREEN_DATA>>>"

    /// The text between the markers, `nil` when the prompt has no screen block.
    public var block: String?
    /// `App: …` header line, without the prefix.
    public var app: String?
    /// `Keyboard: visible` was present.
    public var keyboardVisible: Bool
    /// The element lines, in order.
    public var elements: [Element]
    /// The `Steps so far:` lines (`1. tap [3] "Wi-Fi" -> ok`), in order.
    public var history: [String]
    /// `Note: …` lines.
    public var notes: [String]

    /// Parses the user part of a `ModelRequest`.
    public init(prompt: String) {
        var block: String?
        if let start = prompt.range(of: Self.blockStart),
           let end = prompt.range(of: Self.blockEnd, range: start.upperBound..<prompt.endIndex) {
            block = String(prompt[start.upperBound..<end.lowerBound]).trimmingCharacters(in: .newlines)
        }
        self.block = block
        var app: String?
        var keyboard = false
        var elements: [Element] = []
        for line in (block ?? "").split(separator: "\n", omittingEmptySubsequences: true) {
            let text = String(line)
            if text.hasPrefix("App: ") {
                app = String(text.dropFirst(5))
            } else if text == "Keyboard: visible" {
                keyboard = true
            } else if let element = Self.parseElement(text) {
                elements.append(element)
            }
        }
        self.app = app
        self.keyboardVisible = keyboard
        self.elements = elements

        // History and notes live before the block.
        let head = prompt.range(of: Self.blockStart).map { String(prompt[..<$0.lowerBound]) } ?? prompt
        var history: [String] = []
        var notes: [String] = []
        var inSteps = false
        for line in head.split(separator: "\n", omittingEmptySubsequences: true) {
            let text = String(line)
            if text == "Steps so far:" {
                inSteps = true
                continue
            }
            if text.hasPrefix("Note: ") {
                notes.append(String(text.dropFirst(6)))
                inSteps = false
                continue
            }
            if inSteps {
                if let first = text.first, first.isNumber, text.contains(". ") {
                    history.append(text)
                } else {
                    inSteps = false
                }
            }
        }
        self.history = history
        self.notes = notes
    }

    /// The result part (after ` -> `) of the last history line.
    public var lastResult: String? {
        guard let last = history.last, let arrow = last.range(of: " -> ", options: .backwards) else { return nil }
        return String(last[arrow.upperBound...])
    }

    /// Parses `[N] Role "label" value="v" (flags)`; `nil` for any other line.
    static func parseElement(_ line: String) -> Element? {
        var s = Substring(line)
        guard s.first == "[", let close = s.firstIndex(of: "]"),
              let index = UInt32(s[s.index(after: s.startIndex)..<close]) else { return nil }
        s = s[s.index(after: close)...]
        guard s.first == " " else { return nil }
        s = s.dropFirst()
        let roleEnd = s.firstIndex(of: " ") ?? s.endIndex
        let role = String(s[..<roleEnd])
        guard !role.isEmpty else { return nil }
        s = s[roleEnd...]
        var element = Element(index: index, role: role, label: "")
        if s.hasPrefix(" \"") {
            s = s.dropFirst(2)
            guard let (label, rest) = readQuoted(s) else { return element }
            element.label = label
            s = rest
        }
        for key in ["value", "placeholder"] where s.hasPrefix(" \(key)=\"") {
            s = s.dropFirst(key.count + 3)
            guard let (text, rest) = readQuoted(s) else { return element }
            if key == "value" { element.value = text } else { element.placeholder = text }
            s = rest
        }
        if s.hasPrefix(" ("), s.hasSuffix(")") {
            element.flags = s.dropFirst(2).dropLast().split(separator: ",").map {
                $0.trimmingCharacters(in: .whitespaces)
            }
        }
        return element
    }

    /// Reads up to the closing unescaped quote; returns the unescaped text and the rest.
    private static func readQuoted(_ s: Substring) -> (String, Substring)? {
        var out = ""
        var i = s.startIndex
        while i < s.endIndex {
            let c = s[i]
            if c == "\\" {
                let next = s.index(after: i)
                guard next < s.endIndex else { return nil }
                switch s[next] {
                case "n": out.append("\n")
                case "r": out.append("\r")
                case "t": out.append("\t")
                default: out.append(s[next])
                }
                i = s.index(after: next)
                continue
            }
            if c == "\"" {
                return (out, s[s.index(after: i)...])
            }
            out.append(c)
            i = s.index(after: i)
        }
        return nil
    }
}
