//! Tokenising owner text the way `jarvis_memory::intent_key` does, keeping byte spans so
//! slot values can be cut from the original text.
//!
//! The pipeline mirrors `intent_key` step by step: words are runs of alphanumerics (an
//! apostrophe stays inside a word but vanishes from its normal form), normal forms are
//! lowercased alphanumerics, spelling variants merge through
//! [`jarvis_memory::EQUIVALENT_PHRASES`] (longest first), and filler phrases from
//! [`jarvis_memory::FILLER_PHRASES`] are flagged (longest first). Joining the normal forms of
//! the non-filler words gives exactly `intent_key(text)` (tested).
//!
//! Every step is linear in the text length (the tables are small and fixed).

use jarvis_memory::{EQUIVALENT_PHRASES, FILLER_PHRASES};

/// One word of a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Word {
    /// Normal form (lowercase alphanumerics, spelling variants unified).
    pub norm: String,
    /// Byte offset of the first character in the original text.
    pub start: usize,
    /// Byte offset just past the last character.
    pub end: usize,
    /// Part of a filler phrase ("please", "can you", "the").
    pub filler: bool,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '\u{2019}'
}

fn raw_words(text: &str) -> Vec<Word> {
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
    out
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
        out.push(Word {
            norm,
            start,
            end,
            filler: false,
        });
    }
}

/// Whether the phrase matches the words starting at `i`.
fn phrase_at(words: &[Word], i: usize, phrase: &[&str]) -> bool {
    words
        .get(i..i + phrase.len())
        .is_some_and(|w| w.iter().zip(phrase.iter()).all(|(a, b)| a.norm == *b))
}

fn merge_equivalents(words: Vec<Word>) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::with_capacity(words.len());
    let mut i = 0usize;
    while let Some(w) = words.get(i) {
        let best = EQUIVALENT_PHRASES
            .iter()
            .filter(|(phrase, _)| !phrase.is_empty() && phrase_at(&words, i, phrase))
            .max_by_key(|(phrase, _)| phrase.len());
        match best {
            Some((phrase, canon)) => {
                let end = words
                    .get(i + phrase.len() - 1)
                    .map_or(w.end, |last| last.end);
                out.push(Word {
                    norm: (*canon).to_owned(),
                    start: w.start,
                    end,
                    filler: false,
                });
                i += phrase.len();
            }
            None => {
                out.push(w.clone());
                i += 1;
            }
        }
    }
    out
}

fn mark_fillers(words: &mut [Word]) {
    let mut i = 0usize;
    while i < words.len() {
        let matched = FILLER_PHRASES
            .iter()
            .filter(|phrase| !phrase.is_empty() && phrase_at(words, i, phrase))
            .map(|phrase| phrase.len())
            .max()
            .unwrap_or(0);
        if matched == 0 {
            i += 1;
        } else {
            for w in words.iter_mut().skip(i).take(matched) {
                w.filler = true;
            }
            i += matched;
        }
    }
}

/// The words of `text` with spans, normal forms and filler flags.
pub(crate) fn words(text: &str) -> Vec<Word> {
    let mut ws = merge_equivalents(raw_words(text));
    mark_fillers(&mut ws);
    ws
}

/// The normal forms of the non-filler words of `text` (what `intent_key` joins).
#[cfg(test)]
pub(crate) fn key_tokens(text: &str) -> Vec<String> {
    words(text)
        .into_iter()
        .filter(|w| !w.filler)
        .map(|w| w.norm)
        .collect()
}

/// `text` cut to at most `max_chars` characters (on a character boundary).
pub(crate) fn truncate_chars(text: &str, max_chars: usize) -> &str {
    match text.char_indices().nth(max_chars) {
        Some((i, _)) => text.get(..i).unwrap_or(text),
        None => text,
    }
}

/// Whether `text` is longer than `max_chars` characters, without counting past the limit.
pub(crate) fn longer_than(text: &str, max_chars: usize) -> bool {
    // A char is at most 4 bytes, so a short byte length settles it without iterating.
    if text.len() <= max_chars {
        return false;
    }
    text.chars().nth(max_chars).is_some()
}

/// Whether `text` contains a control character (newlines included).
pub(crate) fn has_control(text: &str) -> bool {
    text.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jarvis_memory::intent_key;

    #[test]
    fn key_tokens_join_to_the_intent_key() {
        for t in [
            "Turn off Wi-Fi",
            "please text Mom I'm on my way",
            "Can you switch on the e-mail sign in for me",
            "  hey jarvis   open the  door!! ",
            "Log in to Acme",
            "",
            "the a an",
            "Ünïcödé wörds and 123 numbers",
        ] {
            assert_eq!(key_tokens(t).join(" "), intent_key(t), "{t:?}");
        }
    }

    #[test]
    fn spans_cut_the_original_words() {
        let t = "Text Mom: I'm late";
        let ws = words(t);
        let cut: Vec<&str> = ws.iter().map(|w| &t[w.start..w.end]).collect();
        assert_eq!(cut, vec!["Text", "Mom", "I'm", "late"]);
        let ws = words("turn off the Wi-Fi");
        assert_eq!(ws[0].norm, "disable");
        assert!(ws[1].filler);
        assert_eq!(ws[2].norm, "wifi");
        assert_eq!(&"turn off the Wi-Fi"[ws[2].start..ws[2].end], "Wi-Fi");
    }

    #[test]
    fn length_helpers_count_characters() {
        assert!(!longer_than("héllo", 5));
        assert!(longer_than("héllo!", 5));
        assert_eq!(truncate_chars("héllo", 2), "hé");
        assert_eq!(truncate_chars("hi", 5), "hi");
    }
}
