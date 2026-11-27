//! Reading a transcript: the wake phrase, the request after it, the stop words.
//!
//! Matching works on words: runs of letters and digits after invisible characters,
//! combining marks and apostrophes are dropped and each letter is folded the way
//! `jarvis-safety` folds it (lowercase, accents and look-alikes to ASCII). So case,
//! diacritics and punctuation do not matter: "Hey, JARVIS!" and "hey jarvís" both start
//! with the wake phrase. Only the listed phrases count; a mis-hearing ("hey service",
//! "hey travis") is not Jarvis. No mis-hearing is accepted on purpose.

use jarvis_safety::{fold_char, is_combining, is_invisible};

/// The wake phrases, as words. Bare "jarvis" counts only at the start of an utterance
/// (every phrase is matched only there).
pub const WAKE_PHRASES: &[&[&str]] = &[
    &["hey", "jarvis"],
    &["hi", "jarvis"],
    &["ok", "jarvis"],
    &["okay", "jarvis"],
    &["jarvis"],
];

/// The stop words, as words. Outside a stop context the whole request (after the wake
/// phrase is stripped) must be one of them ([`Utterance::is_stop`]); in a stop context the
/// session reads them with [`wake_then_stop`] and [`only_stop_words`]. "nevermind" is how
/// recognisers often write "never mind".
pub const STOP_PHRASES: &[&[&str]] = &[
    &["stop"],
    &["cancel"],
    &["never", "mind"],
    &["nevermind"],
    &["abort"],
];

/// The word every wake phrase contains: no line Jarvis says may contain it, in any
/// spelling that reads the same once spaces and punctuation are gone.
const WAKE_WORD: &str = "jarvis";

/// A transcript is read up to this many characters as it arrives (before anything else);
/// a longer one is marked [`Utterance::too_long`] and refused by the session ("That was too
/// long."). Keeps every step linear in this bound, whatever the recogniser (or a script)
/// delivers.
pub const MAX_TRANSCRIPT_CHARS: usize = 20_000;

/// After its whitespace runs are collapsed, a transcript is read up to this many
/// characters; a longer one is marked [`Utterance::too_long`] (refused, never cut). Far
/// above the session's request cap, so a request at that cap always fits.
pub const MAX_PARSE_CHARS: usize = 2_000;

/// A transcript, read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Utterance {
    /// It starts with a wake phrase.
    pub wake: bool,
    /// The request: the text after the wake phrase, cleaned (invisible characters removed,
    /// whitespace collapsed, leading punctuation trimmed). The whole text without one.
    pub request: String,
    /// The request's words (folded).
    pub request_words: Vec<String>,
    /// Every word of the transcript, the wake phrase included (folded).
    pub words: Vec<String>,
    /// The transcript was longer than [`MAX_TRANSCRIPT_CHARS`], or than [`MAX_PARSE_CHARS`]
    /// once its whitespace runs were collapsed: only its start was read, and the session
    /// refuses it as too long (it never acts on a cut request; the stop words still stop).
    pub too_long: bool,
}

impl Utterance {
    /// The request is one of the stop words (and nothing else).
    pub fn is_stop(&self) -> bool {
        STOP_PHRASES
            .iter()
            .any(|p| same_words(&self.request_words, p))
    }

    /// Nothing after the wake phrase.
    pub fn request_is_empty(&self) -> bool {
        self.request_words.is_empty()
    }
}

/// The number of words of the longest phrase of `phrases` that `words` starts with (0:
/// none).
fn phrase_len(words: &[String], phrases: &[&[&str]]) -> usize {
    phrases
        .iter()
        .filter(|p| p.len() <= words.len() && words.iter().zip(p.iter()).all(|(w, p)| w == p))
        .map(|p| p.len())
        .max()
        .unwrap_or(0)
}

/// The stop grammar's first form, read on words: a wake phrase immediately followed by a
/// stop phrase anywhere in `words` ("…the weather tonight. Jarvis, stop", "Jarvis, stop
/// the music"). Returns the index just past the first such stop phrase. Only where the
/// session is in a stop context (a task starts or runs); in Idle the wake phrase counts at
/// the start only.
pub fn wake_then_stop(words: &[String]) -> Option<usize> {
    (0..words.len()).find_map(|at| {
        let rest = words.get(at..)?;
        WAKE_PHRASES
            .iter()
            .filter(|p| p.len() <= rest.len() && rest.iter().zip(p.iter()).all(|(w, p)| w == p))
            .find_map(|p| {
                let after = at + p.len();
                match phrase_len(words.get(after..)?, STOP_PHRASES) {
                    0 => None,
                    n => Some(after + n),
                }
            })
    })
}

/// The stop grammar's second form, read on words: `words` is made only of stop phrases and
/// wake phrases, in any order and number, with at least one stop phrase ("Stop, stop!",
/// "Stop, Jarvis!", "Cancel, cancel"). Acts only where a bare stop does.
pub fn only_stop_words(words: &[String]) -> bool {
    let mut at = 0;
    let mut stops = 0usize;
    while at < words.len() {
        let rest = words.get(at..).unwrap_or(&[]);
        let stop = phrase_len(rest, STOP_PHRASES);
        let wake = phrase_len(rest, WAKE_PHRASES);
        match (stop, wake) {
            (0, 0) => return false,
            (s, w) if s >= w => {
                stops += 1;
                at += s;
            }
            (_, w) => at += w,
        }
    }
    stops > 0
}

/// One word and where it is in the text (byte offsets).
struct Word {
    text: String,
    end: usize,
}

/// Reads `text`: whether it starts with a wake phrase, and the request after it. Whitespace
/// runs are collapsed first (padding never pushes the request past the bound); a transcript
/// over either bound is read only in part and marked [`Utterance::too_long`]. A remainder
/// that is itself only wake phrases ("Hey Jarvis, hey Jarvis") is the wake phrase alone.
pub fn parse_utterance(text: &str) -> Utterance {
    let (collapsed, mut too_long) = collapse_whitespace(text);
    let text = bounded(&collapsed);
    too_long |= text.len() < collapsed.len();
    let words = words(text);
    let mut wake_len = wake_phrase_len(&words);
    if wake_len > 0 {
        // Nothing but wake phrases after the first: the wake phrase alone.
        let mut at = wake_len;
        while at < words.len() {
            match wake_phrase_len(words.get(at..).unwrap_or(&[])) {
                0 => break,
                n => at += n,
            }
        }
        if at == words.len() {
            wake_len = at;
        }
    }
    let rest_start = match wake_len.checked_sub(1) {
        Some(last) => words.get(last).map_or(0, |w| w.end),
        None => 0,
    };
    let request = clean_request(text.get(rest_start..).unwrap_or(""));
    Utterance {
        wake: wake_len > 0,
        request,
        request_words: words
            .iter()
            .skip(wake_len)
            .map(|w| w.text.clone())
            .collect(),
        words: words.into_iter().map(|w| w.text).collect(),
        too_long,
    }
}

/// The number of words of the longest wake phrase `words` starts with (0: none).
fn wake_phrase_len(words: &[Word]) -> usize {
    WAKE_PHRASES
        .iter()
        .filter(|p| starts_with(words, p))
        .map(|p| p.len())
        .max()
        .unwrap_or(0)
}

/// The first [`MAX_TRANSCRIPT_CHARS`] characters of `text` with every whitespace run made one
/// space, and whether `text` was longer than that bound.
fn collapse_whitespace(text: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len().min(4 * MAX_TRANSCRIPT_CHARS));
    let mut in_space = false;
    for (read, c) in text.chars().enumerate() {
        if read == MAX_TRANSCRIPT_CHARS {
            return (out, true);
        }
        if c.is_whitespace() {
            if !in_space {
                out.push(' ');
                in_space = true;
            }
        } else {
            out.push(c);
            in_space = false;
        }
    }
    (out, false)
}

/// Whether `text` contains the wake word in any spacing or punctuation ("Jarvis",
/// "jar vis", "J.A.R.V.I.S."): every letter folded, everything else dropped.
pub fn contains_wake_word(text: &str) -> bool {
    let squashed: String = bounded(text)
        .chars()
        .filter(|c| !is_invisible(*c) && !is_combining(*c))
        .map(fold_char)
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    squashed.contains(WAKE_WORD)
}

/// The folded words of `text` (for the echo check).
pub(crate) fn word_list(text: &str) -> Vec<String> {
    words(bounded(text)).into_iter().map(|w| w.text).collect()
}

/// Whether `heard` (non-empty) appears as consecutive words inside `spoken`.
pub(crate) fn contained_in(heard: &[String], spoken: &[String]) -> bool {
    if heard.is_empty() || heard.len() > spoken.len() {
        return false;
    }
    spoken.windows(heard.len()).any(|w| w == heard)
}

/// A request as typed into a task: invisible and control characters removed, whitespace
/// collapsed to single spaces, leading punctuation and spaces trimmed.
pub(crate) fn clean_request(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_whitespace() {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            continue;
        }
        if c.is_control() || is_invisible(c) {
            continue;
        }
        if out.is_empty() && is_leading_punctuation(c) {
            continue;
        }
        out.push(c);
    }
    let trimmed = out.trim_end();
    trimmed.to_owned()
}

fn is_leading_punctuation(c: char) -> bool {
    matches!(
        c,
        ',' | '.' | ':' | ';' | '!' | '?' | '-' | '\u{2013}' | '\u{2014}' | '\u{2026}'
    )
}

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}' | '\u{2018}' | '\u{02BC}' | '`')
}

/// The first [`MAX_PARSE_CHARS`] characters of `text`.
fn bounded(text: &str) -> &str {
    match text.char_indices().nth(MAX_PARSE_CHARS) {
        Some((end, _)) => text.get(..end).unwrap_or(text),
        None => text,
    }
}

fn words(text: &str) -> Vec<Word> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut end = 0usize;
    for (i, raw) in text.char_indices() {
        if is_invisible(raw) || is_combining(raw) || is_apostrophe(raw) {
            continue;
        }
        let folded = fold_char(raw);
        if folded.is_alphanumeric() {
            cur.extend(folded.to_lowercase());
            end = i + raw.len_utf8();
        } else if !cur.is_empty() {
            out.push(Word {
                text: std::mem::take(&mut cur),
                end,
            });
        }
    }
    if !cur.is_empty() {
        out.push(Word { text: cur, end });
    }
    out
}

fn starts_with(words: &[Word], phrase: &[&str]) -> bool {
    phrase.len() <= words.len() && words.iter().zip(phrase).all(|(w, p)| w.text == *p)
}

fn same_words(words: &[String], phrase: &[&str]) -> bool {
    words.len() == phrase.len() && words.iter().zip(phrase).all(|(w, p)| w == p)
}
