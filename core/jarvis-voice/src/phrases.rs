//! Every line Jarvis says aloud. What is spoken is an output to the room, so the core says
//! only these fixed phrases, plus the task's answer when the owner's setting allows it
//! ([`answer_line`]). No phrase contains the wake word, and no question's details are ever
//! spoken ("I need your approval on the phone." and nothing else).

use jarvis_safety::is_invisible;

use crate::parse::contains_wake_word;

/// A task started.
pub const ON_IT: &str = "On it.";
/// The task waits for an approval or an answer, which is given by touch on the phone.
pub const APPROVAL_ON_PHONE: &str = "I need your approval on the phone.";
/// The task finished well (no answer read aloud).
pub const DONE: &str = "Done.";
/// The task finished well and its answer stays on the phone (locked, or the answer cannot
/// be read aloud).
pub const DONE_ANSWER_ON_PHONE: &str = "Done. The answer is on your phone.";
/// The task failed or could not start.
pub const COULD_NOT: &str = "I could not do that.";
/// The task was stopped.
pub const STOPPED: &str = "Stopped.";
/// No request was heard.
pub const NOT_CAUGHT: &str = "I did not catch that.";
/// The request was over the length cap (it is refused, never cut).
pub const TOO_LONG: &str = "That was too long.";
/// A task started elsewhere runs.
pub const BUSY: &str = "I am busy.";
/// Follows an answer cut at a sentence end.
pub const REST_ON_PHONE: &str = "The rest is on your phone.";

/// Every fixed phrase (the tests check none contains the wake word).
pub const FIXED_PHRASES: &[&str] = &[
    ON_IT,
    APPROVAL_ON_PHONE,
    DONE,
    DONE_ANSWER_ON_PHONE,
    COULD_NOT,
    STOPPED,
    NOT_CAUGHT,
    TOO_LONG,
    BUSY,
    REST_ON_PHONE,
];

/// The most characters of an answer read aloud; a longer one is cut at the last sentence
/// end within it and followed by [`REST_ON_PHONE`].
pub const MAX_SPOKEN_ANSWER_CHARS: usize = 240;

/// `text` made fit to be said: control and invisible characters (the workspace's notion,
/// `jarvis_safety::is_invisible`) removed, all whitespace collapsed to single spaces,
/// trimmed.
pub fn clean_spoken(text: &str) -> String {
    let mut out = String::with_capacity(text.len().min(4 * MAX_SPOKEN_ANSWER_CHARS));
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
        out.push(c);
    }
    out.trim_end().to_owned()
}

/// The line that reads `answer` aloud ("Done. <answer>"), or `None` when it must stay on
/// the phone: empty after cleaning, it contains the wake word in any spelling, or it is
/// longer than [`MAX_SPOKEN_ANSWER_CHARS`] with no sentence end to cut at. A long answer is
/// cut after its last sentence end within the cap and followed by [`REST_ON_PHONE`].
///
/// Whether the answer may be spoken at all (the setting, the lock) is the session's
/// decision; this only composes.
pub fn answer_line(answer: &str) -> Option<String> {
    let clean = clean_spoken(answer);
    if clean.is_empty() || contains_wake_word(&clean) {
        return None;
    }
    if clean.chars().count() <= MAX_SPOKEN_ANSWER_CHARS {
        return Some(format!("{DONE} {clean}"));
    }
    let head: String = clean.chars().take(MAX_SPOKEN_ANSWER_CHARS).collect();
    let cut = sentence_end(&head, clean.get(head.len()..).unwrap_or(""))?;
    let kept = head.get(..cut)?.trim_end();
    Some(format!("{DONE} {kept} {REST_ON_PHONE}"))
}

/// The byte offset just after the last sentence end in `head` (a `.`, `!` or `?` followed by
/// a space, or by the end of `head` when `rest` starts with a space).
fn sentence_end(head: &str, rest: &str) -> Option<usize> {
    let mut best = None;
    let mut chars = head.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if !matches!(c, '.' | '!' | '?') {
            continue;
        }
        let after = i + c.len_utf8();
        let followed_by_space = match chars.peek() {
            Some((_, next)) => *next == ' ',
            None => rest.starts_with(' '),
        };
        if followed_by_space && after > 1 {
            best = Some(after);
        }
    }
    best
}
