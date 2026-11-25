#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Twenty-sixth review (P1): the question's statement that Return sends content it does
//! not show is never cut.
//!
//! The Return-alone form of the sends slot, when no character of the field's value fits,
//! says `, which sends the field's whole content (N characters not shown)`. Its length was
//! not bounded by the slot's allotment: with a flow note (or several notes) the slots'
//! minimums are scaled down, a debug build asserted (`fit_slot`), and a release build cut
//! the slot, chopping `(N characters not shown)`. The statement's length is now the sends
//! slot's floor, which no scaling goes below.
//!
//! Property, over random budgets (notes of 0 to 3 rows and 0 to 450 characters each before
//! the question, either screen source), field values of 0 to 600 characters, screens with
//! and without other fields and facts, and both compositions (the typing of a
//! `type … submit` and its Return alone):
//!
//! (a) composing never panics (the debug assertions included: this runs in a debug build);
//! (b) whenever Return sends content the question does not show in full, the question says
//!     so (`not shown`, or a redaction mark where the value was redacted), in its `Sends`
//!     part, which is never cut.
//!
//! Before the fix this panicked in the sends slot (the review's probe) and, under heavy
//! notes, in the amounts slot; both are covered by the same floors. `TWENTYSIXTH_P1_CASES`
//! sets the number of cases (default 1,000).

mod common;

use common::{button, el, label, screen};
use jarvis_proto::{Action, QuestionRow, QuestionRowKind, RiskCategory, Role};
use jarvis_safety::{question_noted, return_question_noted, ScreenSource, TypedText};

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

/// Lowercase words separated by single spaces, `len` characters (no digits, quotes or
/// markers: what the question shows of it is the text itself).
fn words(r: &mut Rng, len: usize) -> String {
    let mut s = String::new();
    while s.chars().count() < len {
        if !s.is_empty() {
            s.push(' ');
        }
        let n = 1 + r.below(8);
        for _ in 0..n {
            s.push(char::from(b'a' + u8::try_from(r.below(26)).unwrap()));
        }
    }
    s.chars().take(len).collect::<String>().trim().to_owned()
}

fn note(r: &mut Rng) -> QuestionRow {
    let len = usize::try_from(r.below(451)).unwrap();
    QuestionRow {
        kind: QuestionRowKind::Note,
        text: format!(". {}", words(r, len)),
    }
}

/// One case: `(rows, the field's whole content, whether Return is alone in its field)`.
fn case(seed: u64) -> (Vec<QuestionRow>, String, bool) {
    let mut r = Rng::new(seed);
    let value = {
        let len = usize::try_from(r.below(601)).unwrap();
        words(&mut r, len)
    };
    let mut f = el(
        1,
        Role::TextField,
        ["Message", "Search site", "Note", "Comment"][usize::try_from(r.below(4)).unwrap()],
    );
    f.focused = true;
    if !value.is_empty() {
        f.value = Some(value.clone());
    }
    let mut elements = vec![f];
    let mut next = 2;
    let others = r.chance(30);
    if others {
        for name in ["Field a", "Field b", "Field c"]
            .iter()
            .take(1 + usize::try_from(r.below(3)).unwrap())
        {
            let mut g = el(next, Role::TextField, name);
            g.value = Some(words(&mut r, 40));
            elements.push(g);
            next += 1;
        }
    }
    for _ in 0..r.below(6) {
        let e = match r.below(4) {
            0 => button(next, "Send"),
            1 => label(next, "Total $1,234.50"),
            2 => label(next, &format!("Pay to: {}", words(&mut r, 30))),
            _ => label(next, &words(&mut r, 120)),
        };
        elements.push(e);
        next += 1;
    }
    let s = screen("com.example.chat", "Chat", elements);
    let notes: Vec<QuestionRow> = (0..r.below(4)).map(|_| note(&mut r)).collect();
    let source = if r.chance(25) {
        ScreenSource::RunnerText
    } else {
        ScreenSource::Typed
    };
    let category = [
        RiskCategory::SendMessage,
        RiskCategory::None,
        RiskCategory::Payment,
        RiskCategory::PublicPost,
    ][usize::try_from(r.below(4)).unwrap()];
    if r.chance(50) {
        // Return alone, after the typing: the field holds the whole content.
        let typed = vec![TypedText {
            field: 1,
            text: value.clone(),
        }];
        let rows = return_question_noted(1, &value, &s, category, source, None, &typed, &notes);
        (rows, value, !others)
    } else {
        // The typing of a `type … submit`: the field holds `value`, the typing adds `text`.
        let text = {
            let len = usize::try_from(r.below(120)).unwrap();
            words(&mut r, len)
        };
        let whole = match (value.is_empty(), text.is_empty()) {
            (true, _) => text.clone(),
            (false, true) => value.clone(),
            (false, false) => format!("{value}{text}"),
        };
        let action = Action::Type {
            element: Some(1),
            text,
            submit: true,
        };
        let rows = question_noted(&action, &s, category, source, None, &[], &notes);
        (rows, whole, !others)
    }
}

#[test]
fn the_return_alone_statement_is_never_cut() {
    let cases: u64 = std::env::var("TWENTYSIXTH_P1_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    let (mut alone, mut hidden) = (0, 0);
    for seed in 0..cases {
        let (rows, whole, _single) = match std::panic::catch_unwind(|| case(seed)) {
            Ok(c) => c,
            Err(e) => {
                let msg = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                    .unwrap_or_default();
                panic!("seed {seed}: composing panicked: {msg}");
            }
        };
        let summary = jarvis_proto::render_rows(&rows);
        let Some(sends) = rows.iter().find(|r| {
            r.kind == QuestionRowKind::Sends
                && r.text
                    .starts_with(", which sends the field's whole content")
        }) else {
            continue;
        };
        alone += 1;
        if whole.is_empty() {
            continue;
        }
        if !sends.text.contains(&format!("\"{whole}\"")) {
            hidden += 1;
            assert!(
                (sends.text.contains("not shown") || sends.text.contains("(redacted "))
                    && !sends.text.ends_with('…'),
                "seed {seed}: the Return sends content the question does not show in full and must say so, uncut: {:?}\n{summary}",
                sends.text
            );
        }
    }
    // The generator reaches the case at all, and the hidden-content case often.
    assert!(alone > cases / 4, "{alone} Return-alone questions");
    assert!(
        hidden > cases / 20,
        "{hidden} with content not shown in full"
    );
}
