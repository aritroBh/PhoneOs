import Foundation
import JarvisCore

/// What the Live view may show of typed text, URLs and labels. Every rule here hides when in
/// doubt.
///
/// One privacy rule for everything the viewer shows or encodes, decided by the core: screen
/// text (labels, placeholders, app names) goes through the core's own redaction
/// (`redactScreenText`, the one applied before prompts and summaries) before it is kept,
/// and whether a field's value may be shown is the core's answer (`fieldValueIsHidden`:
/// secure, or named as a credential or a bank or card detail). A failing core call hides.
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

    /// The host of an `http`/`https` `url` for an "Open …" caption: never the path, query,
    /// fragment, port or user info. `nil` for any other scheme (a custom scheme's "host" can
    /// be a token), when there is no host, or when the core's redaction would change it.
    public static func host(of url: String) -> String? {
        let trimmed = url.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let components = URLComponents(string: trimmed),
              let scheme = components.scheme?.lowercased(), scheme == "http" || scheme == "https",
              let host = components.host, !host.isEmpty
        else {
            return nil
        }
        let lowered = host.lowercased()
        guard isUnchangedByRedaction(lowered) else { return nil }
        return capped(lowered, 64)
    }

    /// What an "Open …" caption names for `url`: the host for `http`/`https`, only
    /// `<scheme>:` for any other scheme (`tel:`, `myapp:`), `nil` when there is neither.
    public static func link(of url: String) -> String? {
        if let host = host(of: url) { return host }
        let trimmed = url.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let colon = trimmed.firstIndex(of: ":") else { return nil }
        let scheme = trimmed[..<colon].lowercased()
        // RFC 3986: a letter, then letters, digits, "+", "-" or ".".
        guard let first = scheme.unicodeScalars.first, CharacterSet.letters.contains(first), first.isASCII,
              scheme.unicodeScalars.allSatisfy({ $0.isASCII && (CharacterSet.alphanumerics.contains($0) || "+-.".unicodeScalars.contains($0)) }),
              scheme.count <= 32, scheme != "http", scheme != "https"
        else {
            return nil
        }
        return scheme + ":"
    }

    /// A label for display: the core's redaction first (so a cut can never split a secret
    /// out of its detector), then one line, capped; empty, or a failing core call → `nil`.
    public static func label(_ text: String?) -> String? {
        guard let text, !text.isEmpty, let safe = redacted(text) else { return nil }
        let flat = safe.split(whereSeparator: \.isNewline).joined(separator: " ").trimmingCharacters(in: .whitespaces)
        guard !flat.isEmpty else { return nil }
        return capped(flat, maxLabelCharacters)
    }

    /// `text` as the core shows it to the owner (`redactScreenText`); `nil` when the core
    /// call fails (nothing is shown then).
    public static func redacted(_ text: String) -> String? {
        try? redactScreenText(text: text)
    }

    /// The core's redaction leaves `text` exactly as it is (it holds nothing the core would
    /// mask). `false` when the core call fails.
    public static func isUnchangedByRedaction(_ text: String) -> Bool {
        redacted(text) == text
    }

    /// Whether the core hides the value of this field (secure, or named as a credential or
    /// a bank or card detail). `true` when the core call fails.
    public static func fieldValueHidden(role: Role, label: String?, placeholder: String?, identifier: String?,
                                        secure: Bool) -> Bool {
        (try? fieldValueIsHidden(role: role, label: label, placeholder: placeholder, identifier: identifier,
                                 secure: secure)) ?? true
    }

    /// `text` cut to `limit` characters with a trailing "…".
    public static func capped(_ text: String, _ limit: Int) -> String {
        let limit = max(1, limit)
        guard text.count > limit else { return text }
        return String(text.prefix(limit - 1)) + "…"
    }
}
