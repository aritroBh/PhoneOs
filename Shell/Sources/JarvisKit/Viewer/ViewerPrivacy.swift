import Foundation

/// What the Live view may show of typed text, URLs and labels. Every rule here hides when in
/// doubt.
public enum ViewerPrivacy {
    /// What a secure field's text, and any profile value, is shown as.
    public static let mask = "••••"
    /// Longest typed text shown (longer text is cut with "…").
    public static let maxTypedCharacters = 32
    /// Longest label shown in a caption or on the wireframe.
    public static let maxLabelCharacters = 40

    /// The text to show for typing `modelText` (the text the model or the learned flow
    /// asked for, from `SessionProgress.lastAction`, never the raw runner command).
    ///
    /// - `secure`: the target is, or may be, a secure field → `mask`, whatever the text.
    /// - `{{profile.…}}` placeholders (and anything between `{{` and `}}`) → `mask`: the
    ///   core types the owner's stored value there.
    /// - Newlines are flattened, the length is capped; empty text → `nil` (nothing shown).
    public static func typedText(_ modelText: String, secure: Bool) -> String? {
        if secure { return mask }
        var text = maskPlaceholders(modelText)
        text = text.split(whereSeparator: \.isNewline).joined(separator: " ")
        text = text.trimmingCharacters(in: .whitespaces)
        guard !text.isEmpty else { return nil }
        return capped(text, maxTypedCharacters)
    }

    /// Whether `text` holds a `{{…}}` placeholder (or an unclosed `{{`).
    public static func hasPlaceholder(_ text: String) -> Bool {
        text.contains("{{")
    }

    /// `text` with every `{{…}}` (and an unclosed `{{…` to the end) replaced by `mask`.
    static func maskPlaceholders(_ text: String) -> String {
        var out = ""
        var rest = Substring(text)
        while let open = rest.range(of: "{{") {
            out += rest[..<open.lowerBound]
            out += mask
            let after = rest[open.upperBound...]
            guard let close = after.range(of: "}}") else { return out }
            rest = after[close.upperBound...]
        }
        out += rest
        return out
    }

    /// The host of `url` for an "Open …" caption: never the path, query, fragment or user
    /// info. `nil` when there is no host (`tel:`, `sms:`, a bare word).
    public static func host(of url: String) -> String? {
        let trimmed = url.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let components = URLComponents(string: trimmed), let host = components.host, !host.isEmpty else {
            return nil
        }
        let lowered = host.lowercased()
        return capped(lowered, 64)
    }

    /// A label for display: one line, capped; empty → `nil`.
    public static func label(_ text: String?) -> String? {
        guard let text else { return nil }
        let flat = text.split(whereSeparator: \.isNewline).joined(separator: " ").trimmingCharacters(in: .whitespaces)
        guard !flat.isEmpty else { return nil }
        return capped(flat, maxLabelCharacters)
    }

    /// `text` cut to `limit` characters with a trailing "…".
    public static func capped(_ text: String, _ limit: Int) -> String {
        let limit = max(1, limit)
        guard text.count > limit else { return text }
        return String(text.prefix(limit - 1)) + "…"
    }
}
