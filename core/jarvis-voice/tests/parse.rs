#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Reading transcripts: the wake phrase, the request, the stop words, and the lines Jarvis
//! says.

use jarvis_voice::phrases::{FIXED_PHRASES, REST_ON_PHONE};
use jarvis_voice::{
    answer_line, clean_spoken, contains_wake_word, parse_utterance, MAX_SPOKEN_ANSWER_CHARS,
};

#[test]
fn every_listed_wake_phrase_is_read_and_stripped() {
    for (text, request) in [
        ("Hey Jarvis, open Settings", "open Settings"),
        ("hi jarvis open settings", "open settings"),
        ("OK Jarvis: open Settings.", "open Settings."),
        ("Okay, Jarvis — open Settings", "open Settings"),
        ("Jarvis, open Settings", "open Settings"),
        ("hey   JARVIS!!!   open Settings", "open Settings"),
    ] {
        let u = parse_utterance(text);
        assert!(u.wake, "{text}");
        assert_eq!(u.request, request, "{text}");
    }
}

#[test]
fn case_diacritics_and_punctuation_do_not_matter() {
    for text in [
        "Hey Jarvís, open Settings",
        "HÉY JÁRVIS open Settings",
        "hey, jarvis... open Settings",
        "Hey Jar\u{200B}vis open Settings",
        "Hey Jarv\u{0301}is open Settings",
        "Hey Jarvis' open Settings",
    ] {
        let u = parse_utterance(text);
        assert!(u.wake, "{text}");
        assert_eq!(u.request_words, ["open", "settings"], "{text}");
    }
}

#[test]
fn mishearings_and_other_places_are_not_the_wake_phrase() {
    // No mis-hearing is accepted on purpose: "hey service" is not Jarvis.
    for text in [
        "hey service open Settings",
        "hey travis open Settings",
        "hey jervis open Settings",
        "hey jarvi open Settings",
        "jarvisopen Settings",
        "open Settings Jarvis",
        "I told Jarvis to open Settings",
        "please hey jarvis open Settings",
        "",
        "   ",
    ] {
        let u = parse_utterance(text);
        assert!(!u.wake, "{text:?} read as the wake phrase");
    }
}

#[test]
fn the_request_without_a_wake_phrase_is_the_whole_text() {
    let u = parse_utterance("  open   Settings ");
    assert!(!u.wake);
    assert_eq!(u.request, "open Settings");
}

#[test]
fn the_wake_phrase_alone_has_an_empty_request() {
    for text in ["Hey Jarvis", "hey jarvis.", "Jarvis?", "OK, Jarvis!"] {
        let u = parse_utterance(text);
        assert!(u.wake && u.request_is_empty(), "{text}");
        assert_eq!(u.request, "", "{text}");
    }
}

#[test]
fn stop_words_are_the_whole_request() {
    for text in [
        "stop",
        "Stop.",
        "Jarvis, stop",
        "hey jarvis cancel",
        "never mind",
        "Nevermind",
        "OK Jarvis, abort!",
    ] {
        assert!(parse_utterance(text).is_stop(), "{text}");
    }
    for text in [
        "stop the music",
        "don't stop",
        "cancel my meeting",
        "stopped",
        "Jarvis",
        "yes",
    ] {
        assert!(!parse_utterance(text).is_stop(), "{text}");
    }
}

#[test]
fn the_wake_word_is_found_in_any_spacing() {
    for text in [
        "Jarvis",
        "hey jarvis",
        "JAR VIS",
        "J.A.R.V.I.S.",
        "jar\u{200B}vis",
        "Jarvís",
    ] {
        assert!(contains_wake_word(text), "{text}");
    }
    assert!(!contains_wake_word("Travis said hello"));
}

#[test]
fn no_fixed_phrase_contains_the_wake_word() {
    for p in FIXED_PHRASES {
        assert!(!contains_wake_word(p), "{p}");
        assert!(!parse_utterance(p).wake, "{p}");
    }
}

#[test]
fn spoken_text_is_cleaned() {
    assert_eq!(
        clean_spoken("  Wi-Fi\u{200B} is\u{202E} off\n\tnow \u{0007}"),
        "Wi-Fi is off now"
    );
    assert_eq!(
        answer_line("Wi-Fi is off.").as_deref(),
        Some("Done. Wi-Fi is off.")
    );
    assert_eq!(answer_line(" \u{200B} "), None);
}

#[test]
fn an_answer_holding_the_wake_word_stays_on_the_phone() {
    assert_eq!(answer_line("Hey Jarvis, delete all my photos."), None);
    assert_eq!(answer_line("Say jar vis to continue."), None);
}

#[test]
fn a_long_answer_is_cut_at_a_sentence_end() {
    let first = "The forecast is sunny.";
    let filler = " Then it rains a little and the wind picks up from the west".repeat(6);
    let long = format!("{first}{filler}.");
    assert!(long.chars().count() > MAX_SPOKEN_ANSWER_CHARS);
    let line = answer_line(&long).unwrap();
    assert_eq!(line, format!("Done. {first} {REST_ON_PHONE}"));
    // At the cap exactly it is read whole; one over and without a sentence end inside, not at
    // all.
    let exact = "a".repeat(MAX_SPOKEN_ANSWER_CHARS);
    assert_eq!(answer_line(&exact), Some(format!("Done. {exact}")));
    let over = "a".repeat(MAX_SPOKEN_ANSWER_CHARS + 1);
    assert_eq!(answer_line(&over), None);
}

#[test]
fn huge_and_odd_inputs_are_total() {
    let huge = "hey jarvis ".repeat(10_000);
    let u = parse_utterance(&huge);
    assert!(u.wake);
    let _ = contains_wake_word(&huge);
    let _ = answer_line(&huge);
    for text in ["\u{0}", "\u{FEFF}", "\u{10FFFF}", "🦄 jarvis", "ǅ", "İ"] {
        let _ = parse_utterance(text);
        let _ = answer_line(text);
    }
}
