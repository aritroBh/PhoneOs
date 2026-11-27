//! `check_typed` and `correction_deletes`: table-driven cases for what a field may show
//! after the agent typed into it.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use jarvis_agent::{check_typed, correction_deletes, MismatchKind, TypedVerdict};

/// What a case expects.
#[derive(Debug, PartialEq)]
enum Want {
    Match,
    Mismatch(MismatchKind, &'static str),
    Unverifiable,
}

struct Case {
    name: &'static str,
    typed: &'static str,
    before: Option<&'static str>,
    after: Option<&'static str>,
    placeholder: Option<&'static str>,
    want: Want,
}

fn case(
    name: &'static str,
    typed: &'static str,
    before: Option<&'static str>,
    after: Option<&'static str>,
    want: Want,
) -> Case {
    Case {
        name,
        typed,
        before,
        after,
        placeholder: None,
        want,
    }
}

fn with_placeholder(mut c: Case, p: &'static str) -> Case {
    c.placeholder = Some(p);
    c
}

#[test]
fn check_typed_table() {
    use MismatchKind::*;
    use Want::Mismatch as M;
    let cases = vec![
        // Arrived intact.
        case(
            "exact",
            "running late",
            None,
            Some("running late"),
            Want::Match,
        ),
        case("empty prior", "hi", Some(""), Some("hi"), Want::Match),
        // Prior text: typing appends at the caret.
        case(
            "appended to prior",
            " there",
            Some("Hi"),
            Some("Hi there"),
            Want::Match,
        ),
        case(
            "appended without space",
            "late",
            Some("running "),
            Some("running late"),
            Want::Match,
        ),
        case(
            "caret in the middle",
            "big ",
            Some("a dog"),
            Some("a big dog"),
            Want::Match,
        ),
        case(
            "field replaced its content",
            "new",
            Some("old"),
            Some("new"),
            Want::Match,
        ),
        // The keyboard's legitimate changes.
        case(
            "auto-capitalised",
            "running late",
            None,
            Some("Running late"),
            Want::Match,
        ),
        case("all caps field", "abc", None, Some("ABC"), Want::Match),
        case(
            "trailing space added",
            "hello",
            None,
            Some("hello "),
            Want::Match,
        ),
        case(
            "trailing newline added",
            "hello",
            None,
            Some("hello\n"),
            Want::Match,
        ),
        case(
            "smart apostrophe",
            "I'm late",
            None,
            Some("I\u{2019}m late"),
            Want::Match,
        ),
        case(
            "smart quotes",
            "say \"hi\"",
            None,
            Some("say \u{201C}hi\u{201D}"),
            Want::Match,
        ),
        case("smart dash", "a--b", None, Some("a\u{2014}b"), Want::Match),
        case(
            "ellipsis",
            "wait...",
            None,
            Some("wait\u{2026}"),
            Want::Match,
        ),
        case(
            "non-breaking space",
            "a b",
            None,
            Some("a\u{00A0}b"),
            Want::Match,
        ),
        // Multi-byte text.
        case(
            "emoji intact",
            "on my way 🚗",
            None,
            Some("on my way 🚗"),
            Want::Match,
        ),
        case(
            "accented capitalised",
            "élan",
            None,
            Some("Élan"),
            Want::Match,
        ),
        case("cjk intact", "東京駅", None, Some("東京駅"), Want::Match),
        case(
            "emoji dropped",
            "on my way 🚗",
            None,
            Some("on my way "),
            M(Truncated, "on my way"),
        ),
        case(
            "cjk dropped",
            "東京駅",
            None,
            Some("東駅"),
            M(Dropped, "東駅"),
        ),
        // Lost characters.
        case(
            "dropped in the middle",
            "running late",
            None,
            Some("rnning lte"),
            M(Dropped, "rnning lte"),
        ),
        case(
            "dropped after prior",
            " there",
            Some("Hi"),
            Some("Hi thre"),
            M(Dropped, "thre"),
        ),
        case(
            "nothing arrived",
            "hello",
            Some(""),
            Some(""),
            M(Missing, ""),
        ),
        case(
            "nothing arrived after prior",
            "x",
            Some("Hi"),
            Some("Hi"),
            M(Missing, ""),
        ),
        // Doubled keystrokes.
        case(
            "doubled characters",
            "hello",
            None,
            Some("hheello"),
            M(Extra, "hheello"),
        ),
        case(
            "doubled after prior",
            "ok",
            Some("A "),
            Some("A ookk"),
            M(Extra, "ookk"),
        ),
        // Retyping what the field already held must not pass on the prior text alone.
        case(
            "prior already held it",
            "hello",
            Some("hello"),
            Some("hellohelo"),
            M(Dropped, "helo"),
        ),
        // Auto-correction rewriting a word is a difference the model must see.
        case(
            "autocorrected word",
            "teh cat",
            None,
            Some("the cat"),
            M(Different, "the cat"),
        ),
        // Length-limited fields keep the beginning.
        case(
            "max length truncates",
            "123456789",
            None,
            Some("12345"),
            M(Truncated, "12345"),
        ),
        // No value reported: cannot be checked.
        case("no value", "hello", None, None, Want::Unverifiable),
        case(
            "no value after prior",
            "hello",
            Some("x"),
            None,
            Want::Unverifiable,
        ),
        // Placeholders reported as values.
        with_placeholder(
            case(
                "placeholder shown when empty",
                "milk",
                None,
                Some("Search"),
                M(Missing, ""),
            ),
            "Search",
        ),
        with_placeholder(
            case(
                "placeholder as prior",
                "milk",
                Some("Search"),
                Some("milk"),
                Want::Match,
            ),
            "Search",
        ),
        with_placeholder(
            case(
                "typed text equals placeholder",
                "Search",
                Some(""),
                Some("Search"),
                M(Missing, ""),
            ),
            "Search",
        ),
        // Typing only whitespace checks nothing.
        case("whitespace only", "  ", None, Some(""), Want::Match),
    ];
    let mut failures = Vec::new();
    for c in &cases {
        let got = match check_typed(c.typed, c.before, c.after, c.placeholder) {
            TypedVerdict::Match => Want::Match,
            TypedVerdict::Unverifiable => Want::Unverifiable,
            TypedVerdict::Mismatch { kind, found } => {
                let found: &'static str = Box::leak(found.into_boxed_str());
                Want::Mismatch(kind, found)
            }
        };
        if got != c.want {
            failures.push(format!("{}: got {got:?}, want {:?}", c.name, c.want));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `(before, after, placeholder, max, want)`.
type DeleteCase = (
    Option<&'static str>,
    &'static str,
    Option<&'static str>,
    usize,
    Option<usize>,
);

#[test]
fn correction_deletes_only_what_was_appended() {
    let cases: Vec<DeleteCase> = vec![
        (None, "rnning lte", None, 64, Some(10)),
        (Some(""), "abc", None, 64, Some(3)),
        (Some("Hi"), "Hi thre", None, 64, Some(5)),
        (Some("Hi"), "Hi", None, 64, Some(0)),
        // Emoji and accents count as characters, not bytes.
        (None, "on my way 🚗", None, 64, Some(11)),
        (Some("é"), "éàü", None, 64, Some(2)),
        // The prior text changed: what was typed cannot be told apart, delete nothing.
        (Some("Hello"), "hello world", None, 64, None),
        // Too much to delete.
        (None, "abcdef", None, 5, None),
        // A placeholder reads as empty, before and after.
        (Some("Search"), "mlk", Some("Search"), 64, Some(3)),
        (None, "Search", Some("Search"), 64, Some(0)),
    ];
    for (before, after, placeholder, max, want) in cases {
        assert_eq!(
            correction_deletes(before, after, placeholder, max),
            want,
            "{before:?} -> {after:?}"
        );
    }
}
