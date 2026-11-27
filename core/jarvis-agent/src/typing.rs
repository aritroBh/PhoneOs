//! Typed-text verification: did what the agent typed arrive in the field?
//!
//! Synthesized text entry can drop (or occasionally double) characters, so after a `type`
//! the session reads the target field's value on the next observation and compares it with
//! [`check_typed`]. The comparison is deliberately forgiving about what the keyboard
//! legitimately changes and strict about lost characters:
//!
//! - **Prior text.** Typing appends at the caret, so a field that held `prior` should hold
//!   `prior + typed`. The text after an unchanged prior is compared; failing that, the
//!   typed text counts when the field is exactly the prior text with the typed text
//!   inserted somewhere (the caret was elsewhere); a field that now holds exactly the typed
//!   text counts too (some fields select their content on focus, so typing replaces it).
//! - **Case.** Auto-capitalisation changes case, so comparison ignores case
//!   (`char::to_lowercase`, which also covers non-ASCII letters).
//! - **Whitespace.** A trailing space or newline may be added (auto-correction accepts a
//!   word with a space); leading and trailing whitespace is ignored, and non-breaking
//!   spaces count as spaces.
//! - **Smart punctuation.** Curly quotes count as straight quotes, `…` as `...`, an em dash
//!   as `--` and an en dash as `-`.
//! - **Placeholders.** Some runners report an empty field's placeholder as its value; a
//!   value equal to the placeholder is read as empty.
//! - **Unknown values.** A field that reports no value at all after typing cannot be
//!   checked ([`TypedVerdict::Unverifiable`]): retyping on a guess could double the text.
//!   Secure fields never expose their value, so the session never checks them (and never
//!   types into them: those are handed to the owner).
//!
//! Auto-correction that rewrites a word ("teh" to "the") is reported as a mismatch; the
//! model is told what the field holds and decides.

/// How a field's content differs from what was typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MismatchKind {
    /// Nothing of the typed text arrived.
    Missing,
    /// A beginning of the typed text arrived: the end was lost or the field has a length
    /// limit.
    Truncated,
    /// Some characters are missing from the middle.
    Dropped,
    /// Extra characters appeared (for example doubled keystrokes).
    Extra,
    /// The text differs in some other way (auto-correction, a different field).
    Different,
}

impl MismatchKind {
    /// A few words for the model.
    pub fn describe(self) -> &'static str {
        match self {
            MismatchKind::Missing => "nothing arrived",
            MismatchKind::Truncated => "the end is missing (or the field limits its length)",
            MismatchKind::Dropped => "characters were dropped",
            MismatchKind::Extra => "extra characters appeared",
            MismatchKind::Different => "the text differs",
        }
    }
}

/// The result of [`check_typed`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypedVerdict {
    /// The field holds the typed text.
    Match,
    /// The field does not hold the typed text.
    Mismatch {
        /// How it differs.
        kind: MismatchKind,
        /// The part of the field compared with the typed text (after the prior text, when
        /// that was unchanged), as the field shows it.
        found: String,
    },
    /// The field reports no value, so nothing can be said.
    Unverifiable,
}

/// Compares the typed text with the field's value after typing.
///
/// `before` and `after` are the field's reported values before and after typing,
/// `placeholder` its placeholder (a value equal to it reads as empty). See the module docs
/// for the rules.
pub fn check_typed(
    typed: &str,
    before: Option<&str>,
    after: Option<&str>,
    placeholder: Option<&str>,
) -> TypedVerdict {
    let Some(after) = after else {
        return TypedVerdict::Unverifiable;
    };
    let want = norm(typed);
    if want.is_empty() {
        return TypedVerdict::Match;
    }
    let after_raw = effective(after, placeholder);
    let prior_raw = before.map_or("", |b| effective(b, placeholder));
    let cur = norm(after_raw);
    let prior = norm(prior_raw);
    if cur == want {
        return TypedVerdict::Match;
    }
    let (got, got_raw) = match cur.strip_prefix(prior.as_str()) {
        Some(rest) if !prior.is_empty() => {
            if rest.trim() == want {
                return TypedVerdict::Match;
            }
            (rest.trim().to_owned(), tail_raw(after_raw, prior_raw))
        }
        _ => (cur.clone(), after_raw.trim().to_owned()),
    };
    if inserted(&cur, &want, &prior) {
        return TypedVerdict::Match;
    }
    TypedVerdict::Mismatch {
        kind: classify(&want, &got),
        found: got_raw,
    }
}

/// How many Delete presses remove what typing appended, so the text can be typed again:
/// the characters after an unchanged `before` (exact, not normalised). `None` when the
/// field's text no longer starts with its prior text (the typed part cannot be told apart,
/// so nothing is deleted) or more than `max` presses would be needed.
pub fn correction_deletes(
    before: Option<&str>,
    after: &str,
    placeholder: Option<&str>,
    max: usize,
) -> Option<usize> {
    let after = effective(after, placeholder);
    let prior = before.map_or("", |b| effective(b, placeholder));
    let rest = after.strip_prefix(prior)?;
    let n = rest.chars().count();
    (n <= max).then_some(n)
}

/// `value`, or empty when it is the placeholder.
fn effective<'a>(value: &'a str, placeholder: Option<&str>) -> &'a str {
    match placeholder {
        Some(p) if !p.is_empty() && value == p => "",
        _ => value,
    }
}

/// The raw text after `prior_raw` in `after_raw`, trimmed (falls back to all of it).
fn tail_raw(after_raw: &str, prior_raw: &str) -> String {
    after_raw
        .strip_prefix(prior_raw)
        .unwrap_or(after_raw)
        .trim()
        .to_owned()
}

/// Lowercased, smart punctuation folded, whitespace runs kept but non-breaking spaces made
/// plain, leading and trailing whitespace removed.
fn norm(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{2032}' => out.push('\''),
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{2033}' => out.push('"'),
            '\u{2026}' => out.push_str("..."),
            '\u{2014}' => out.push_str("--"),
            '\u{2013}' => out.push('-'),
            '\u{00A0}' | '\u{202F}' => out.push(' '),
            c => out.extend(c.to_lowercase()),
        }
    }
    out.trim().to_owned()
}

/// Whether `cur` is `prior` with `want` inserted at some position (whitespace runs around
/// the insertion are not significant).
fn inserted(cur: &str, want: &str, prior: &str) -> bool {
    let prior = collapse(prior);
    cur.match_indices(want).any(|(i, m)| {
        let (head, rest) = cur.split_at(i);
        let tail = rest.get(m.len()..).unwrap_or("");
        collapse(&format!("{head}{tail}")) == prior
    })
}

/// Whitespace runs as one space, trimmed.
fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether `small` is a subsequence of `big` (by chars).
fn subsequence(small: &str, big: &str) -> bool {
    let mut it = big.chars();
    small.chars().all(|c| it.any(|b| b == c))
}

fn classify(want: &str, got: &str) -> MismatchKind {
    if got.is_empty() {
        MismatchKind::Missing
    } else if want.starts_with(got) {
        MismatchKind::Truncated
    } else if got.chars().count() < want.chars().count() && subsequence(got, want) {
        MismatchKind::Dropped
    } else if got.chars().count() > want.chars().count() && subsequence(want, got) {
        MismatchKind::Extra
    } else {
        MismatchKind::Different
    }
}
