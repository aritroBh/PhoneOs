//! Intent keys: a deterministic normal form of a task text, plus slot extraction for text the
//! user dictated that is later typed (`"text mom running late"` -> `"text mom {1}"`).

use crate::flow::Bindings;

/// Maximum number of slots in one flow's intent key.
pub const MAX_SLOTS: usize = 8;

/// Filler phrases dropped from intent keys, as lowercase word sequences; at each position
/// the longest matching phrase wins. Public (read-only) so other crates that tokenise task
/// text, such as the skill matcher, drop exactly the same words as [`intent_key`].
pub const FILLER_PHRASES: &[&[&str]] = &[
    &["i", "want", "you", "to"],
    &["i", "need", "you", "to"],
    &["go", "ahead", "and"],
    &["hey", "jarvis"],
    &["okay", "jarvis"],
    &["ok", "jarvis"],
    &["hi", "jarvis"],
    &["can", "you"],
    &["could", "you"],
    &["would", "you"],
    &["will", "you"],
    &["for", "me"],
    &["please"],
    &["kindly"],
    &["the"],
    &["a"],
    &["an"],
    &["my"],
];

/// Spelling variants of the same word or phrase, as lowercase word sequences (hyphens and
/// spaces are both separators, so `Wi-Fi` arrives as `wi fi`), and the one word they are
/// keyed as. Longest match wins. Deliberately small and explicit.
pub const EQUIVALENT_PHRASES: &[(&[&str], &str)] = &[
    (&["wi", "fi"], "wifi"),
    (&["e", "mail"], "email"),
    (&["e", "mails"], "emails"),
    (&["log", "in"], "login"),
    (&["log", "out"], "logout"),
    (&["sign", "in"], "signin"),
    (&["sign", "out"], "signout"),
    (&["sign", "up"], "signup"),
    (&["turn", "off"], "disable"),
    (&["switch", "off"], "disable"),
    (&["turn", "on"], "enable"),
    (&["switch", "on"], "enable"),
];

/// The length and canonical word of the longest [`EQUIVALENT_PHRASES`] entry starting at
/// `norms[i]`.
fn equivalent_at<S: AsRef<str>>(norms: &[S], i: usize) -> Option<(usize, &'static str)> {
    EQUIVALENT_PHRASES
        .iter()
        .filter(|(phrase, _)| {
            norms
                .get(i..i + phrase.len())
                .is_some_and(|w| w.iter().zip(phrase.iter()).all(|(a, b)| a.as_ref() == *b))
        })
        .map(|(phrase, canon)| (phrase.len(), *canon))
        .max_by_key(|(len, _)| *len)
}

/// Replaces variant spellings in already-normalised words (see [`EQUIVALENT_PHRASES`]).
fn merge_equivalents(ws: Vec<Word>) -> Vec<Word> {
    let norms: Vec<&str> = ws.iter().map(|w| w.norm.as_str()).collect();
    let mut out: Vec<Word> = Vec::with_capacity(ws.len());
    let mut i = 0usize;
    while let Some(w) = ws.get(i) {
        match equivalent_at(&norms, i) {
            Some((len, canon)) => {
                let end = ws.get(i + len - 1).map_or(w.end, |last| last.end);
                out.push(Word {
                    norm: canon.to_owned(),
                    start: w.start,
                    end,
                });
                i += len;
            }
            None => {
                out.push(Word {
                    norm: w.norm.clone(),
                    start: w.start,
                    end: w.end,
                });
                i += 1;
            }
        }
    }
    out
}

/// Re-applies [`EQUIVALENT_PHRASES`] to a stored intent key (space-separated words and
/// `{n}` slots), so keys stored before an entry existed still match new task texts.
pub fn renormalize_key(key: &str) -> String {
    let toks: Vec<&str> = key.split(' ').filter(|t| !t.is_empty()).collect();
    let mut out: Vec<&str> = Vec::with_capacity(toks.len());
    let mut i = 0usize;
    while let Some(t) = toks.get(i) {
        match equivalent_at(&toks, i) {
            Some((len, canon)) => {
                out.push(canon);
                i += len;
            }
            None => {
                out.push(t);
                i += 1;
            }
        }
    }
    out.join(" ")
}

/// One word of a text with its byte span.
struct Word {
    norm: String,
    start: usize,
    end: usize,
}

/// A word of a template: normalised text, or a slot placeholder.
struct Tok {
    norm: String,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '\u{2019}'
}

/// Splits text into words (alphanumeric runs; apostrophes stay inside but vanish from the
/// normalised form). Everything else is a separator.
fn words(text: &str) -> Vec<Word> {
    let mut out = Vec::new();
    let mut current: Option<(usize, usize)> = None;
    for (i, c) in text.char_indices() {
        if is_word_char(c) {
            let end = i + c.len_utf8();
            current = Some(match current {
                Some((s, _)) => (s, end),
                None => (i, end),
            });
        } else if let Some((s, e)) = current.take() {
            push_word(&mut out, text, s, e);
        }
    }
    if let Some((s, e)) = current {
        push_word(&mut out, text, s, e);
    }
    merge_equivalents(out)
}

fn push_word(out: &mut Vec<Word>, text: &str, start: usize, end: usize) {
    let Some(raw) = text.get(start..end) else {
        return;
    };
    let norm: String = raw
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    if !norm.is_empty() {
        out.push(Word { norm, start, end });
    }
}

/// For each token, whether it is part of a filler phrase. Longest phrase wins.
fn mark_fillers(norms: &[&str]) -> Vec<bool> {
    let mut flags = vec![false; norms.len()];
    let mut i = 0;
    while i < norms.len() {
        let mut matched = 0;
        for phrase in FILLER_PHRASES {
            let len = phrase.len();
            let hit = norms
                .get(i..i + len)
                .is_some_and(|w| w.iter().zip(phrase.iter()).all(|(a, b)| a == b));
            if hit && len > matched {
                matched = len;
            }
        }
        if matched == 0 {
            i += 1;
        } else {
            for f in flags.iter_mut().skip(i).take(matched) {
                *f = true;
            }
            i += matched;
        }
    }
    flags
}

fn filler_flags(words: &[Word]) -> Vec<bool> {
    let norms: Vec<&str> = words.iter().map(|w| w.norm.as_str()).collect();
    mark_fillers(&norms)
}

fn join_non_filler(toks: &[Tok]) -> String {
    let norms: Vec<&str> = toks.iter().map(|t| t.norm.as_str()).collect();
    let flags = mark_fillers(&norms);
    toks.iter()
        .zip(flags)
        .filter(|(_, f)| !f)
        .map(|(t, _)| t.norm.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Normalises a task text: lowercase, punctuation stripped, spelling variants unified
/// ([`EQUIVALENT_PHRASES`]: `Wi-Fi`/`wifi`/`wi fi`, `turn off`/`switch off`/`disable`...),
/// filler words dropped, whitespace collapsed. Braces are punctuation, so the result never
/// contains a slot placeholder.
pub fn intent_key(task_text: &str) -> String {
    let toks: Vec<Tok> = words(task_text)
        .into_iter()
        .map(|w| Tok { norm: w.norm })
        .collect();
    join_non_filler(&toks)
}

/// Intent key of `task_text` with the given `(start, end, slot)` byte spans replaced by `{slot}`.
///
/// Returns `None` when a span is out of range, not on a char boundary, or spans overlap.
pub fn slotted_key(task_text: &str, spans: &[(usize, usize, u32)]) -> Option<String> {
    let mut sorted: Vec<(usize, usize, u32)> = spans.to_vec();
    sorted.sort();
    let mut toks: Vec<Tok> = Vec::new();
    let mut cursor = 0usize;
    for (start, end, slot) in sorted {
        if start < cursor || end <= start {
            return None;
        }
        let piece = task_text.get(cursor..start)?;
        toks.extend(words(piece).into_iter().map(|w| Tok { norm: w.norm }));
        toks.push(Tok {
            norm: format!("{{{slot}}}"),
        });
        cursor = end;
        task_text.get(start..end)?;
    }
    let rest = task_text.get(cursor..)?;
    toks.extend(words(rest).into_iter().map(|w| Tok { norm: w.norm }));
    Some(join_non_filler(&toks))
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Finds the single, word-bounded, case-insensitive occurrence of `needle` in `task_text`.
///
/// Returns `None` when `needle` is shorter than two characters, is not already trimmed, occurs
/// zero or several times, or only occurs inside a longer word.
pub fn find_span(task_text: &str, needle: &str) -> Option<(usize, usize)> {
    if needle != needle.trim() || needle.chars().count() < 2 {
        return None;
    }
    let hay: Vec<(usize, char)> = task_text.char_indices().collect();
    let pat: Vec<char> = needle.chars().map(fold).collect();
    let mut found: Option<(usize, usize, usize)> = None;
    let mut count = 0;
    for (i, w) in hay.windows(pat.len()).enumerate() {
        if w.iter().zip(pat.iter()).all(|((_, a), b)| fold(*a) == *b) {
            count += 1;
            let first = w.first()?;
            let last = w.last()?;
            found = Some((first.0, last.0 + last.1.len_utf8(), i));
        }
    }
    if count != 1 {
        return None;
    }
    let (start, end, i) = found?;
    let first_char = needle.chars().next()?;
    let last_char = needle.chars().next_back()?;
    let prev = i.checked_sub(1).and_then(|p| hay.get(p)).map(|(_, c)| *c);
    let next = hay.get(i + pat.len()).map(|(_, c)| *c);
    if first_char.is_alphanumeric() && prev.is_some_and(char::is_alphanumeric) {
        return None;
    }
    if last_char.is_alphanumeric() && next.is_some_and(char::is_alphanumeric) {
        return None;
    }
    Some((start, end))
}

enum Part {
    Lit(String),
    Slot(u32),
}

fn parse_template(key: &str) -> Vec<Part> {
    key.split(' ')
        .filter(|t| !t.is_empty())
        .map(|t| {
            t.strip_prefix('{')
                .and_then(|r| r.strip_suffix('}'))
                .and_then(|n| n.parse::<u32>().ok())
                .map_or_else(|| Part::Lit(t.to_owned()), Part::Slot)
        })
        .collect()
}

/// True when the key contains at least one `{n}` placeholder.
pub fn is_slotted(key: &str) -> bool {
    parse_template(key)
        .iter()
        .any(|p| matches!(p, Part::Slot(_)))
}

/// Number of literal (non-slot) tokens in a slotted key.
pub fn literal_count(key: &str) -> usize {
    parse_template(key)
        .iter()
        .filter(|p| matches!(p, Part::Lit(_)))
        .count()
}

/// Matches `task_text` against a slotted intent key and returns the slot values.
///
/// Literals must appear in order (filler words may be interspersed). A slot swallows words up
/// to the next literal, which must occur exactly once after the slot starts; a trailing slot
/// swallows the rest. Values are the task's own words, without outer punctuation. Returns
/// `None` for non-slotted keys, adjacent or repeated slots, any mismatch, and any ambiguity.
pub fn match_slotted(key: &str, task_text: &str) -> Option<Bindings> {
    let template = parse_template(key);
    if !template.iter().any(|p| matches!(p, Part::Slot(_))) {
        return None;
    }
    let ws = words(task_text);
    let fl = filler_flags(&ws);
    let n = ws.len();
    let mut out = Bindings::new();
    let mut i = 0usize;
    for (t, part) in template.iter().enumerate() {
        match part {
            Part::Lit(l) => {
                while fl.get(i).copied().unwrap_or(false) {
                    i += 1;
                }
                if ws.get(i)?.norm != *l {
                    return None;
                }
                i += 1;
            }
            Part::Slot(id) => {
                let first = ws.get(i)?;
                let last_idx = match template.get(t + 1) {
                    None => n.checked_sub(1)?,
                    Some(Part::Slot(_)) => return None,
                    Some(Part::Lit(next)) => {
                        let hits: Vec<usize> = ws
                            .iter()
                            .enumerate()
                            .skip(i + 1)
                            .filter(|(j, w)| {
                                !fl.get(*j).copied().unwrap_or(false) && w.norm == *next
                            })
                            .map(|(j, _)| j)
                            .collect();
                        match hits.as_slice() {
                            [j] => j.checked_sub(1)?,
                            _ => return None,
                        }
                    }
                };
                let last = ws.get(last_idx)?;
                let value = task_text.get(first.start..last.end)?;
                if out.insert(*id, value.to_owned()).is_some() {
                    return None;
                }
                i = last_idx + 1;
            }
        }
    }
    if fl.iter().skip(i).all(|f| *f) && i <= n {
        Some(out)
    } else {
        None
    }
}
