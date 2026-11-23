//! The questions the thirteenth review added (`thirteenth/cases.rs`: `open_url` links the
//! owner did not give, `sms:`/`mailto:` recipients and what they send, weak recipient
//! heads, amounts listed from field values, presumed submits and drags): each decision is
//! byte-identical to the snapshot `tests/fixtures/thirteenth_questions.txt`, which was
//! written at c4c1bda (the thirteenth review's commit), before the question became rows,
//! by running these same cases there. And each question is exactly its rows rendered, with
//! each line in a row of its family.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

#[path = "thirteenth/cases.rs"]
mod cases;
mod common;

use jarvis_proto::{render_rows, ConfirmVerb, QuestionRowKind, RiskCategory};
use jarvis_safety::Decision;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/thirteenth_questions.txt"
);

#[test]
fn thirteenth_review_questions_are_unchanged() {
    let text: String = cases::cases()
        .iter()
        .map(|(name, d)| cases::line(name, d))
        .collect();
    let want = std::fs::read_to_string(FIXTURE).unwrap();
    assert!(!want.trim().is_empty(), "the snapshot is empty");
    for (a, b) in want.lines().zip(text.lines()) {
        assert_eq!(a, b, "a question changed");
    }
    assert_eq!(want.lines().count(), text.lines().count());
}

#[test]
fn thirteenth_review_questions_are_their_rows() {
    let mut asked = 0;
    for (name, d) in cases::cases() {
        let Decision::Confirm(q) = d else { continue };
        asked += 1;
        assert_eq!(render_rows(&q.rows), q.summary, "{name}: {q:?}");
        let kinds: Vec<QuestionRowKind> = q.rows.iter().map(|r| r.kind).collect();
        assert_eq!(
            kinds.first(),
            Some(&QuestionRowKind::Action),
            "{name}: {q:?}"
        );
        for (marker, kind) in [
            (". Amounts on screen", QuestionRowKind::Amount),
            (". To/Account", QuestionRowKind::Recipient),
            (". Screen says", QuestionRowKind::ScreenSays),
            (". Recipient not verified", QuestionRowKind::NotVerified),
            (". Sends:", QuestionRowKind::Sends),
            (". The form holds:", QuestionRowKind::Sends),
            (". Typed:", QuestionRowKind::Typed),
            (". Fields:", QuestionRowKind::Fields),
            (". Selected:", QuestionRowKind::Selected),
        ] {
            for r in q.rows.iter().filter(|r| r.text.contains(marker)) {
                assert_eq!(r.kind, kind, "{name}: {marker} in {r:?}");
            }
        }
        if name.starts_with("open ") {
            assert_eq!(q.verb, ConfirmVerb::OpenUrl, "{name}");
            // The link's scheme, host, path and query are the action line.
            assert!(
                q.rows[0].text.starts_with("Open "),
                "{name}: {:?}",
                q.rows[0]
            );
            if q.category == RiskCategory::SendMessage
                && (name.contains("sms") || name.contains("mailto"))
            {
                assert!(
                    q.rows[0]
                        .text
                        .contains(" link that was not in your request, to "),
                    "{name}: {q:?}"
                );
            }
        }
    }
    assert!(asked >= 25, "{asked}");
    // The lines the thirteenth review added each have their row.
    let all: Vec<_> = cases::cases()
        .into_iter()
        .filter_map(|(n, d)| match d {
            Decision::Confirm(q) => Some((n, q)),
            _ => None,
        })
        .collect();
    let has = |name: &str, kind: QuestionRowKind, text: &str| {
        let (_, q) = all.iter().find(|(n, _)| n == name).expect(name);
        assert!(
            q.rows
                .iter()
                .any(|r| r.kind == kind && r.text.contains(text)),
            "{name}: no {kind:?} row with {text:?}: {q:?}"
        );
    };
    has(
        "open sms:+15551234567&body=Code%20482913",
        QuestionRowKind::Sends,
        ". Sends: body: \"",
    );
    has(
        "open mailto:x@evil.example?subject=hi&body=yo",
        QuestionRowKind::Sends,
        ". Sends: subject: \"hi\"; body: \"yo\"",
    );
    has(
        "open mailto:x@evil.example?subject=hi&body=yo",
        QuestionRowKind::Action,
        "to \"x@evil.example\"",
    );
    has(
        "open https://collector.example/c?v=482913",
        QuestionRowKind::Action,
        "https://collector.example, path and query \"",
    );
    has(
        "weak head alone",
        QuestionRowKind::Recipient,
        ". To/Account: Account: \"Everyday Checking 4521\"",
    );
    has(
        "weak head alone",
        QuestionRowKind::NotVerified,
        ". Recipient not verified",
    );
    has(
        "presumed money Recipient/How much?",
        QuestionRowKind::Amount,
        "900",
    );
}
