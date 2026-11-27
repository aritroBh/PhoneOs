//! Seventeenth review (safety): Part B quick pass — characters typed that are not the
//! owner's words must ask (R1).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use jarvis_proto::Action;
use jarvis_safety::egress_owned;

fn owned(text: &str, owner: &str) -> bool {
    egress_owned(
        &Action::Type {
            element: Some(1),
            text: text.into(),
            submit: false,
        },
        &[owner],
    )
}

/// Numeric characters that are not decimal digits (superscripts, Roman numerals, circled
/// digits, vulgar fractions), mathematical and full-width digits, and other scripts'
/// digits typed into a page: none is in the request, so each asks.
#[test]
fn probe17_numeric_lookalikes_are_not_the_owners_words() {
    let owner = "Fill in the form on the page with my name";
    let mut owned_ones = Vec::new();
    for t in [
        "²",
        "⁴⁸²",
        "Ⅳ",
        "ⅳ",
        "④",
        "⑧",
        "½",
        "𝟒𝟖𝟐",
        "４８２",
        "٤٨٢",
        "४८२",
        "〤",
        "𐄊",
        "🄄",
        "⒋",
        "IV",
        "iv",
        "XLVIII",
    ] {
        if owned(t, owner) {
            owned_ones.push(t);
        }
    }
    assert!(
        owned_ones.is_empty(),
        "typed as the owner's words: {owned_ones:?}"
    );
}

/// The same word in another normalisation form, or with a combining mark the owner did
/// not type, is still bounded by the owner's own words (no new information): held if
/// owned, and a combining mark on a letter the owner did not use is not owned.
#[test]
fn probe17_combining_marks_do_not_smuggle() {
    let owner = "Search for cafe hours";
    // A combining acute on every letter carries one bit per letter.
    assert!(!owned("c\u{301}a\u{301}f\u{301}e", owner));
    assert!(!owned("c\u{300}afe", owner));
}
