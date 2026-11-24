//! Every question the built-in eval scenarios ask, in both observation modes: its text is
//! byte-identical to the snapshot taken before the question became structured rows
//! (`tests/fixtures/eval_questions.txt`), and it is exactly its rows rendered.
//!
//! The snapshot was written at 026b2bc. The thirteenth review's commit (c4c1bda) asks the
//! same 162 questions byte for byte (compared before the rows were applied on top of it), so
//! it gained no lines; the questions that commit added are outside the eval scenarios and
//! are pinned by `jarvis-safety/tests/thirteenth_questions.rs`, whose snapshot was written
//! at c4c1bda.
//!
//! Fifteenth review: every text the scenarios type is in their task's words (R1 adds no
//! question); R4 (a tap inside an alert that is not a plain dismiss asks, phone only, its
//! title quoted) changes four lines: `login_wrong_password`'s "OK" on the "Incorrect
//! password" alert is a new question in the structured mode and gains `This answers an
//! alert. Screen says: "Incorrect password"` in the runner-text mode; `notes_delete`'s
//! "Delete" on the "Delete Note?" alert gains `This answers an alert. Screen says: "Delete
//! Note?"` in both modes. 162 questions, now 163.
//!
//! Nineteenth review: 164 questions, still 164; two changed (re-blessed after review, each a
//! rule change): `news_dense_page` structured — the story tap's flow note now names `News and
//! all web content in News (Jarvis cannot tell its parts apart)` (an app's web content covers
//! all of itself from its first look, so it is a source for itself); runner-text — the same
//! tap's `Unverified` question now carries a flow note naming `all pages in News with no
//! readable address (Jarvis cannot tell its parts apart)` (a runner-text screen holding a web
//! view is one covering class, a source for itself) and is asked before the first-use
//! question, as the structured mode's flow question already was.
//!
//! `JARVIS_BLESS_QUESTIONS=1` rewrites the snapshot; never do that to make this pass: a
//! changed question is a behaviour change to review.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use jarvis_proto::{render_rows, ConfirmStage, ConfirmVerb, QuestionRowKind, RiskCategory};
use jarvis_safety::UNVERIFIED_SCREEN_WARNING;
use jarvis_sim::builtin::builtin_tasks;
use jarvis_sim::eval::run_one;
use jarvis_sim::{ObserveMode, RunSetup, ScriptedModel};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/eval_questions.txt"
);

/// One line per question: task, mode, category, the summary as a JSON string.
fn asked() -> (String, Vec<jarvis_proto::ConfirmRequest>) {
    let mut out = String::new();
    let mut all = Vec::new();
    for (mode, name) in [
        (ObserveMode::Structured, "structured"),
        (ObserveMode::RunnerText, "runner-text"),
    ] {
        let setup = RunSetup {
            mode,
            first_use_approval: true,
            ..RunSetup::default()
        };
        for task in builtin_tasks().unwrap() {
            let mut model = ScriptedModel::new(task.script.clone());
            let r = run_one(&task, &setup, &mut model, None).unwrap();
            for q in r.requests {
                out.push_str(&format!(
                    "{}\t{name}\t{:?}\t{}\n",
                    r.id,
                    q.category,
                    serde_json::to_string(&q.summary).unwrap()
                ));
                all.push(q);
            }
        }
    }
    (out, all)
}

#[test]
fn eval_questions_are_unchanged() {
    let (text, _) = asked();
    if std::env::var_os("JARVIS_BLESS_QUESTIONS").is_some() {
        std::fs::write(FIXTURE, &text).unwrap();
    }
    let want = std::fs::read_to_string(FIXTURE).unwrap();
    assert!(!want.trim().is_empty(), "the snapshot is empty");
    for (a, b) in want.lines().zip(text.lines()) {
        assert_eq!(a, b, "a question changed");
    }
    assert_eq!(
        want.lines().count(),
        text.lines().count(),
        "questions added or lost"
    );
}

#[test]
fn every_eval_question_is_its_rows_and_states_what_it_asks() {
    let (_, all) = asked();
    assert!(all.len() > 100, "{}", all.len());
    let mut structured = 0usize;
    for q in &all {
        // One source: the summary is exactly its rows joined.
        assert!(!q.rows.is_empty(), "{q:?}");
        assert_eq!(render_rows(&q.rows), q.summary, "{q:?}");
        if q.rows.len() > 1 {
            structured += 1;
        }
        // Each line family the summary holds has its own row.
        let kinds: Vec<QuestionRowKind> = q.rows.iter().map(|r| r.kind).collect();
        for (marker, kind) in [
            (". Amounts on screen", QuestionRowKind::Amount),
            (". To/Account", QuestionRowKind::Recipient),
            (". Screen says", QuestionRowKind::ScreenSays),
            (". Recipient not verified", QuestionRowKind::NotVerified),
            (". Sends:", QuestionRowKind::Sends),
            (". The form holds:", QuestionRowKind::Sends),
            (
                ", which sends the field's whole content",
                QuestionRowKind::Sends,
            ),
            (". Typed:", QuestionRowKind::Typed),
            (". Fields:", QuestionRowKind::Fields),
            (". Selected:", QuestionRowKind::Selected),
        ] {
            if q.summary.contains(marker) {
                let row = q.rows.iter().find(|r| r.kind == kind);
                assert!(
                    row.is_some_and(|r| r.text.contains(marker)),
                    "{marker} not in a {kind:?} row: {q:?}"
                );
            }
        }
        let head = match q.summary.strip_prefix(UNVERIFIED_SCREEN_WARNING) {
            Some(rest) => {
                assert_eq!(kinds.first(), Some(&QuestionRowKind::Warning), "{q:?}");
                rest
            }
            None => q.summary.as_str(),
        };
        // The stated part agrees with what the question says it asks.
        if q.category != RiskCategory::FirstUse {
            match q.stage {
                ConfirmStage::Return => assert!(head.starts_with("Press Return"), "{q:?}"),
                ConfirmStage::Typing => assert!(head.starts_with("Type "), "{q:?}"),
                ConfirmStage::Whole => assert!(!head.starts_with("Press Return"), "{q:?}"),
            }
        }
        // An action on a screen element states the element the core resolved.
        if matches!(
            q.verb,
            ConfirmVerb::Tap | ConfirmVerb::LongPress | ConfirmVerb::Clear
        ) {
            let t = q.target.as_ref().expect("a target");
            assert!(t.element.is_some() && t.frame.is_some(), "{q:?}");
            assert_eq!(t.digest.len(), 64, "{q:?}");
        }
        if matches!(
            q.verb,
            ConfirmVerb::PressHome | ConfirmVerb::OpenApp | ConfirmVerb::OpenUrl
        ) {
            assert!(q.target.is_none(), "{q:?}");
        }
    }
    // The composer's rows, not a one-row fallback, for the questions with facts.
    assert!(structured > all.len() / 2, "{structured} of {}", all.len());
}
