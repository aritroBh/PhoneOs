//! The question's rows (`QuestionRow`): a slot's cut keeps them joining to the cut text, and
//! the final sanitising pass keeps them joining to the summary.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use super::*;

fn rows(parts: &[(QuestionRowKind, &str)]) -> Vec<QuestionRow> {
    parts.iter().map(|(k, t)| row(*k, t)).collect()
}

#[test]
fn a_cut_keeps_the_rows_and_their_join() {
    let r = rows(&[
        (QuestionRowKind::Recipient, ". To/Account: To: \"bob\""),
        (
            QuestionRowKind::ScreenSays,
            ". Screen says: Pay to: Mallory",
        ),
    ]);
    let joined = render_rows(&r);
    for n in [0, 1, 5, 25, 26, 40, n_chars(&joined)] {
        let cut = hard_cut(&joined, n);
        let split = split_cut(&r, &cut);
        assert_eq!(render_rows(&split), cut, "{n}");
        assert!(split.iter().all(|r| !r.text.is_empty()));
        if let Some(first) = split.first() {
            assert_eq!(first.kind, QuestionRowKind::Recipient);
        }
    }
    // Uncut: the rows as they were.
    assert_eq!(fit_rows(r.clone(), 1000), r);
}

#[test]
fn the_final_pass_keeps_rows_that_join_to_the_summary() {
    // Nothing to sanitise: the rows stand.
    let r = rows(&[
        (QuestionRowKind::Action, "Tap \"Pay\" in Shop"),
        (QuestionRowKind::Amount, ". Amounts on screen: $5.00"),
    ]);
    assert_eq!(finish_question(r.clone(), MAX_SUMMARY_CHARS), r);
    // A row the pass changes on its own: each row sanitised.
    let r = rows(&[
        (QuestionRowKind::Action, "Tap  \"Pay\""),
        (QuestionRowKind::Sends, ". Sends: x"),
    ]);
    let done = finish_question(r, MAX_SUMMARY_CHARS);
    assert_eq!(render_rows(&done), "Tap \"Pay\". Sends: x");
    assert_eq!(done.len(), 2);
    // A change across rows: the summary as one row.
    let r = rows(&[
        (QuestionRowKind::Action, "Tap "),
        (QuestionRowKind::Sends, " x"),
    ]);
    let done = finish_question(r, MAX_SUMMARY_CHARS);
    assert_eq!(
        render_rows(&done),
        finish_summary("Tap  x", MAX_SUMMARY_CHARS)
    );
    assert_eq!(done.len(), 1);
}
