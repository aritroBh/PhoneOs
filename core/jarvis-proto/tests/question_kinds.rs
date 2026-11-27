#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Sixteenth review: the row kinds `Flow` and `Insert` on the wire. Older JSON (no rows, or
//! only the older kinds) reads as before; the summary is the rows' texts either way.

use jarvis_proto::{render_rows, ConfirmRequest, QuestionRow, QuestionRowKind, RiskCategory};

fn row(kind: QuestionRowKind, text: &str) -> QuestionRow {
    QuestionRow {
        kind,
        text: text.to_owned(),
    }
}

#[test]
fn flow_and_insert_rows_round_trip_by_name() {
    let rows = vec![
        row(QuestionRowKind::Action, "Tap \"Next\" in Safari"),
        row(
            QuestionRowKind::Flow,
            ". Jarvis has read Messages in this task; what it read there could be passed on here",
        ),
        row(
            QuestionRowKind::Insert,
            ". This enters content Jarvis cannot show (the clipboard or AutoFill) into Safari",
        ),
    ];
    let q = ConfirmRequest::from_rows(RiskCategory::SendMessage, rows.clone(), "x".to_owned());
    assert_eq!(q.summary, render_rows(&rows));
    let json = serde_json::to_string(&q).unwrap();
    assert!(json.contains(r#""kind":"flow""#), "{json}");
    assert!(json.contains(r#""kind":"insert""#), "{json}");
    let back: ConfirmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(back, q);
}

#[test]
fn older_json_reads_as_before() {
    // No rows at all (before structured questions).
    let q: ConfirmRequest =
        serde_json::from_str(r#"{"category":"send_message","summary":"Tap Send","app":"a"}"#)
            .unwrap();
    assert!(q.rows.is_empty());
    // Every older kind still parses to itself, the note kind included.
    for (name, kind) in [
        ("action", QuestionRowKind::Action),
        ("warning", QuestionRowKind::Warning),
        ("sends", QuestionRowKind::Sends),
        ("typed", QuestionRowKind::Typed),
        ("amount", QuestionRowKind::Amount),
        ("recipient", QuestionRowKind::Recipient),
        ("not_verified", QuestionRowKind::NotVerified),
        ("screen_says", QuestionRowKind::ScreenSays),
        ("selected", QuestionRowKind::Selected),
        ("fields", QuestionRowKind::Fields),
        ("note", QuestionRowKind::Note),
        ("other", QuestionRowKind::Other),
    ] {
        let r: QuestionRow =
            serde_json::from_str(&format!(r#"{{"kind":"{name}","text":"t"}}"#)).unwrap();
        assert_eq!(r.kind, kind, "{name}");
    }
    // A kind this core does not know is refused as before (no catch-all).
    assert!(serde_json::from_str::<QuestionRow>(r#"{"kind":"nonsense","text":"t"}"#).is_err());
}
