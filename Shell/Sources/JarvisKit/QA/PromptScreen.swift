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
        /// A switch's state as the core renders it (`(on)` / `(off)`); `nil` for anything
        /// that is not a switch. The raw `0`/`1` of the tree is not in the prompt any more.
        public var isOn: Bool?
        /// Other flags (`focused`, `selected`, `disabled`), without the switch state.
        public var flags: [String]

        public init(index: UInt32, role: String, label: String, value: String? = nil, placeholder: String? = nil, isOn: Bool? = nil, flags: [String] = []) {
            self.index = index
            self.role = role
            self.label = label
            self.value = value
            self.placeholder = placeholder
            self.isOn = isOn
            self.flags = flags
        }

        /// What a run view shows next to the label: `On` / `Off` for a switch, else the value.
        public var displayValue: String? {
            if let isOn { return isOn ? "On" : "Off" }
            return value
        }

        /// The line as the core renders it (for diagnostics).
        public var line: String {
            var s = "[\(index)] \(role)"
            if !label.isEmpty { s += " \"\(label)\"" }
            if let value { s += " value=\"\(value)\"" } else if let placeholder { s += " placeholder=\"\(placeholder)\"" }
            let all = (isOn.map { [$0 ? "on" : "off"] } ?? []) + flags
            if !all.isEmpty { s += " (\(all.joined(separator: ", ")))" }
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
    /// The cue lines the core adds under the header and at the end of the list.
    public var cues: Cues

    /// What the screen says about the part of it that is not shown.
    public struct Cues: Sendable, Equatable {
        /// `… more above; scroll up`.
        public var moreAbove = false
        /// `… more below; scroll down` or `… N more elements below; scroll down`.
        public var moreBelow = false
        /// `Page 1 of 2 …`.
        public var page: (number: Int, count: Int)?

        public init(moreAbove: Bool = false, moreBelow: Bool = false, page: (number: Int, count: Int)? = nil) {
            self.moreAbove = moreAbove
            self.moreBelow = moreBelow
            self.page = page
        }

        public static func == (a: Cues, b: Cues) -> Bool {
            a.moreAbove == b.moreAbove && a.moreBelow == b.moreBelow && a.page?.number == b.page?.number && a.page?.count == b.page?.count
        }

        /// The cue a block line is, `nil` for any other line.
        static func parse(_ line: String, into cues: inout Cues) -> Bool {
            if line == "… more above; scroll up" {
                cues.moreAbove = true
                return true
            }
            if line == "… more below; scroll down" || (line.hasPrefix("… ") && line.hasSuffix(" more elements below; scroll down")) {
                cues.moreBelow = true
                return true
            }
            if line.hasPrefix("Page ") {
                let parts = line.dropFirst(5).split(separator: " ")
                if parts.count >= 3, parts[1] == "of", let n = Int(parts[0]), let c = Int(parts[2]) {
                    cues.page = (n, c)
                    return true
                }
            }
            return false
        }
    }

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
        var cues = Cues()
        for line in (block ?? "").split(separator: "\n", omittingEmptySubsequences: true) {
            let text = String(line)
            if text.hasPrefix("App: ") {
                app = String(text.dropFirst(5))
            } else if text == "Keyboard: visible" {
                keyboard = true
            } else if Cues.parse(text, into: &cues) {
                continue
            } else if let element = Self.parseElement(text) {
                elements.append(element)
            }
        }
        self.cues = cues
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

    /// The result part (after ` -> `) of the last history line, including what changed
    /// (`ok; changed: Button "Cart (0)" -> "Cart (1)"`).
    public var lastResult: String? {
        history.last.flatMap(Self.result(ofHistoryLine:))
    }

    /// The outcome of the last history line.
    public var lastOutcome: StepOutcome? {
        lastResult.map(StepOutcome.init(result:))
    }

    /// The result of a history line: what follows the first ` -> ` that is followed by a
    /// result the core can write. The change suffix itself holds arrows (`"Cart (0)" ->
    /// "Cart (1)"`, `Switch "Wi-Fi" off -> on`), so the last arrow is not the separator.
    static func result(ofHistoryLine line: String) -> String? {
        var from = line.startIndex
        while let arrow = line.range(of: " -> ", range: from..<line.endIndex) {
            let rest = String(line[arrow.upperBound...])
            if StepOutcome(result: rest) != .unknown { return rest }
            from = arrow.upperBound
        }
        // Another shape of result (`screenshot taken`): after the first arrow.
        return line.range(of: " -> ").map { String(line[$0.upperBound...]) }
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
            for flag in s.dropFirst(2).dropLast().split(separator: ",").map({ $0.trimmingCharacters(in: .whitespaces) }) {
                switch flag {
                case "on": element.isOn = true
                case "off": element.isOn = false
                default: element.flags.append(flag)
                }
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


/// How a step in "Steps so far" ended, read from the start of its result. What follows
/// `ok;` (the change summary) is never looked at: it quotes screen text.
public enum StepOutcome: Sendable, Equatable {
    /// `ok` / `ok; changed: …` / `ok; now in …` / `ok; screen unchanged`, or a tool's `ok: …`.
    case ok
    /// `done, but the phone locked before the result could be checked`.
    case ranUnchecked
    /// `failed: …`.
    case failed
    /// `not done: …` (declined, repeated, lock came first).
    case notDone
    /// `interrupted: …`.
    case interrupted
    /// `not answered: …`.
    case notAnswered
    /// `owner answered: …`: the core handed the step to the owner (a question, or text it
    /// never types itself such as a password) and the owner replied. The step is over.
    case ownerAnswered
    /// Anything else (`screenshot taken`, `not available: …`, text of another shape).
    case unknown

    public init(result: String) {
        let r = result.trimmingCharacters(in: .whitespaces).lowercased()
        func starts(_ p: String) -> Bool { r.hasPrefix(p) }
        if r == "ok" || starts("ok;") || starts("ok:") { self = .ok }
        else if starts("done, but") { self = .ranUnchecked }
        else if starts("failed") { self = .failed }
        else if starts("not done") { self = .notDone }
        else if starts("interrupted") { self = .interrupted }
        else if starts("not answered") { self = .notAnswered }
        else if starts("owner answered") { self = .ownerAnswered }
        else { self = .unknown }
    }
}
