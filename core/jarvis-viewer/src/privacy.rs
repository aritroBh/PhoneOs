//! What the Live view may show of typed text, URLs and labels. Every rule hides when in
//! doubt, and the core decides: screen text goes through `jarvis_safety::redact` (the
//! redaction applied before prompts and summaries) before it is kept, and whether a
//! field's value may be shown is `jarvis_safety::credential_like_field` plus the secure
//! flag ([`field_value_hidden`]). Both are called in-process, once per label, with no FFI
//! crossing.
//!
//! Lengths are counted in Unicode scalar values (`char`s). The Swift code counted grapheme
//! clusters, so a label of emoji or combining marks may be cut a few characters earlier
//! here; the cut never splits a scalar.

use jarvis_proto as p;

/// What a secure field's text, and any profile value, is shown as.
pub const MASK: &str = "••••";
/// Longest typed text shown (longer text is cut with "…").
pub const MAX_TYPED_CHARACTERS: usize = 32;
/// Longest label shown in a caption or on the wireframe.
pub const MAX_LABEL_CHARACTERS: usize = 40;

/// Swift's `Character.isNewline`, per scalar (`\r\n` is two separators, which gives the
/// same result once empty pieces are dropped).
pub(crate) fn is_newline(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

/// Foundation's `CharacterSet.whitespaces`: tab and the space separators (Zs).
fn is_space(c: char) -> bool {
    matches!(
        c,
        '\t' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
    )
}

/// The pieces between newlines (empty pieces dropped) joined with a space, trimmed of
/// spaces and tabs.
fn one_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for piece in text.split(is_newline).filter(|p| !p.is_empty()) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(piece);
    }
    let trimmed = out.trim_matches(is_space);
    if trimmed.len() == out.len() {
        out
    } else {
        trimmed.to_owned()
    }
}

/// `text` cut to `limit` characters with a trailing "…".
pub fn capped(text: &str, limit: usize) -> String {
    let limit = limit.max(1);
    match text.char_indices().nth(limit) {
        None => text.to_owned(),
        Some(_) => {
            let end = text
                .char_indices()
                .nth(limit - 1)
                .map_or(text.len(), |(i, _)| i);
            let mut out = String::with_capacity(end + 3);
            out.push_str(text.get(..end).unwrap_or_default());
            out.push('…');
            out
        }
    }
}

/// `text` as the core shows it to the owner (`jarvis_safety::redact`).
pub fn redacted(text: &str) -> String {
    jarvis_safety::redact(text)
}

/// The core's redaction leaves `text` exactly as it is (it holds nothing the core would
/// mask).
pub fn is_unchanged_by_redaction(text: &str) -> bool {
    redacted(text) == text
}

/// A label for display: the core's redaction first (so a cut can never split a secret out
/// of its detector), then one line, capped; empty → `None`.
pub fn label(text: Option<&str>) -> Option<String> {
    let text = text?;
    if text.is_empty() {
        return None;
    }
    let flat = one_line(&redacted(text));
    if flat.is_empty() {
        return None;
    }
    if flat.chars().count() > MAX_LABEL_CHARACTERS {
        Some(capped(&flat, MAX_LABEL_CHARACTERS))
    } else {
        Some(flat)
    }
}

/// Remembers [`label`] results, so a screen observed again (the common case: the same
/// screen after each action) does not run the core's redaction over every label again.
///
/// Keyed by a 128-bit hash of the raw text (two SipHash runs with distinct prefixes), so
/// the cache holds no raw screen text, only the display labels it already produced. A hash
/// collision could only show another (already redacted) label, never unredacted text.
/// Bounded: cleared when [`LabelCache::CAPACITY`] entries are reached.
#[derive(Debug, Clone, Default)]
pub struct LabelCache {
    map: std::collections::HashMap<u128, Option<String>>,
}

impl LabelCache {
    /// Entries kept at most (each a hash and a label of at most 40 characters).
    pub const CAPACITY: usize = 4_096;

    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Entries held.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Nothing held.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    fn key(text: &str) -> u128 {
        use std::hash::{Hash, Hasher};
        let mut a = std::collections::hash_map::DefaultHasher::new();
        0u8.hash(&mut a);
        text.hash(&mut a);
        let mut b = std::collections::hash_map::DefaultHasher::new();
        1u8.hash(&mut b);
        text.hash(&mut b);
        (u128::from(a.finish()) << 64) | u128::from(b.finish())
    }

    /// [`label`], remembered.
    pub fn label(&mut self, text: Option<&str>) -> Option<String> {
        let text = text?;
        if text.is_empty() {
            return None;
        }
        let key = Self::key(text);
        if let Some(hit) = self.map.get(&key) {
            return hit.clone();
        }
        let value = label(Some(text));
        if self.map.len() >= Self::CAPACITY {
            self.map.clear();
        }
        self.map.insert(key, value.clone());
        value
    }
}

/// Whether `text` holds a `{{…}}` placeholder (or an unclosed `{{`).
pub fn has_placeholder(text: &str) -> bool {
    text.contains("{{")
}

/// `text` with every `{{…}}` (and an unclosed `{{…` to the end) replaced by [`MASK`].
pub fn mask_placeholders(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("{{") {
        out.push_str(rest.get(..open).unwrap_or_default());
        out.push_str(MASK);
        let after = rest.get(open + 2..).unwrap_or_default();
        match after.find("}}") {
            Some(close) => rest = after.get(close + 2..).unwrap_or_default(),
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// The text to show for typing `model_text` (the text the model or the learned flow asked
/// for, never the raw runner command).
///
/// - `secure`: the target is, or may be, a secure field → [`MASK`], whatever the text.
/// - `{{profile.…}}` placeholders (and anything between `{{` and `}}`) → [`MASK`]: the
///   core types the owner's stored value there.
/// - Newlines are flattened, the length is capped; empty text → `None`.
pub fn typed_text(model_text: &str, secure: bool) -> Option<String> {
    if secure {
        return Some(MASK.to_owned());
    }
    let text = one_line(&mask_placeholders(model_text));
    if text.is_empty() {
        return None;
    }
    Some(capped(&text, MAX_TYPED_CHARACTERS))
}

/// RFC 3986 scheme: a letter, then letters, digits, `+`, `-` or `.`.
fn is_scheme(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// A character RFC 3986 allows anywhere in a URI reference.
fn is_uri_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '-' | '.'
                | '_'
                | '~'
                | ':'
                | '/'
                | '?'
                | '#'
                | '['
                | ']'
                | '@'
                | '!'
                | '$'
                | '&'
                | '\''
                | '('
                | ')'
                | '*'
                | '+'
                | ','
                | ';'
                | '='
                | '%'
        )
}

/// The host of an `http`/`https` `url` for an "Open …" caption: never the path, query,
/// fragment, port or user info. `None` for any other scheme (a custom scheme's "host" can
/// be a token), when there is no host, when the URL is not a valid URI (any character
/// outside RFC 3986, a percent-encoded host, a port that is not a number), or when the
/// core's redaction would change it.
pub fn host_of(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if !trimmed.chars().all(is_uri_char) {
        return None;
    }
    let (scheme, rest) = trimmed.split_once(':')?;
    if !is_scheme(scheme) {
        return None;
    }
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let rest = rest.strip_prefix("//")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let (host, port) = if host_port.starts_with('[') {
        let close = host_port.find(']')?;
        let (host, after) = host_port.split_at(close + 1);
        (host, after.strip_prefix(':'))
    } else {
        match host_port.split_once(':') {
            Some((h, port)) => (h, Some(port)),
            None => (host_port, None),
        }
    };
    if port.is_some_and(|p| !p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    if host.is_empty() || host.contains('%') || host.contains('@') {
        return None;
    }
    let lowered = host.to_ascii_lowercase();
    if !is_unchanged_by_redaction(&lowered) {
        return None;
    }
    Some(capped(&lowered, 64))
}

/// What an "Open …" caption names for `url`: the host for `http`/`https`, only `<scheme>:`
/// for any other scheme (`tel:`, `myapp:`), `None` when there is neither.
pub fn link_of(url: &str) -> Option<String> {
    if let Some(host) = host_of(url) {
        return Some(host);
    }
    let trimmed = url.trim();
    let colon = trimmed.find(':')?;
    let scheme = trimmed.get(..colon)?.to_lowercase();
    if !is_scheme(&scheme) || scheme.chars().count() > 32 || scheme == "http" || scheme == "https" {
        return None;
    }
    Some(scheme + ":")
}

/// Whether the core hides the value of a field with this role, label, placeholder,
/// identifier and secure flag: a secure field (flag or `SecureTextField` role), or an entry
/// field named as a credential or a bank or card detail. An identifier counts as the
/// label (compaction uses it when the field has no label); it is checked whenever one is
/// given, so the answer is at least as strict as the core's own. This is the function the
/// FFI's `fieldValueIsHidden` calls.
pub fn field_value_hidden(
    role: &p::Role,
    label: Option<&str>,
    placeholder: Option<&str>,
    identifier: Option<&str>,
    secure: bool,
) -> bool {
    if secure || *role == p::Role::SecureTextField {
        return true;
    }
    let placeholder = placeholder.filter(|t| !t.trim().is_empty());
    let named = |label: &str| {
        jarvis_safety::credential_like_field(&p::Element {
            index: 1,
            role: role.clone(),
            label: label.to_owned(),
            value: None,
            placeholder: placeholder.map(str::to_owned),
            frame: p::Rect::new(0.0, 0.0, 0.0, 0.0),
            enabled: true,
            focused: false,
            selected: false,
            secure: false,
            fingerprint: 0,
        })
    };
    let mut any = false;
    let mut hidden = false;
    for candidate in [label, identifier]
        .into_iter()
        .flatten()
        .filter(|t| !t.trim().is_empty())
    {
        any = true;
        if named(candidate) {
            hidden = true;
            break;
        }
    }
    if !any {
        return named("");
    }
    hidden
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capped_counts_characters() {
        assert_eq!(capped("abc", 3), "abc");
        assert_eq!(capped("abcd", 3), "ab…");
        assert_eq!(capped("ééééé", 2), "é…");
        assert_eq!(capped("x", 0), "x");
        assert_eq!(capped("xy", 0), "…");
    }

    #[test]
    fn one_line_flattens() {
        assert_eq!(one_line("  a\n\nb \r\nc\t"), "a b  c");
        assert_eq!(one_line("\n \n"), "");
    }
}
