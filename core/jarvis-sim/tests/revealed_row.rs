//! Mac run 161 (iOS 26.5, probes P13e and P13f) against the simulated phone: a row with
//! swipe actions reveals them on a first bounded swipe (its cell shifted 107 pt left, the
//! actions beside it) and is deleted by a second swipe the same way of 0.45 of the row —
//! Jarvis's own bounded swipe made twice deleted the row in every kind measured. The
//! session takes a fresh look before every action, so the second swipe is judged on the
//! screen that shows the revealed row, and it is refused: a model that asks for the same
//! swipe twice gets the first asked and performed and the second refused.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::{Point, RiskCategory, RunnerCommand};
use jarvis_sim::app::AppDef;
use jarvis_sim::builtin::{builtin_apps, parse_app};
use jarvis_sim::{Human, ObserveMode, RunSetup, ScriptedModel, SimPhone};

const ROWS: &str = r#"{
  "id": "rows", "bundle_id": "com.example.rows", "name": "Rows", "start": "list",
  "state": { "items": ["Row A", "Row B", "Row C"], "shown": {}, "last_deleted": "" },
  "screens": { "list": { "title": "Rows", "elements": [
    { "id": "rows-list", "role": "collection_view", "scroll": true, "frame": [0, 150, 393, 702],
      "children": [
        { "kind": "row", "for_each": "items", "item": "t", "id": "row-{index}", "h": 52,
          "label": "{t}", "reveal": "shown.{index}",
          "actions": [
            { "id": "trash", "role": "button", "label": "Delete", "on_tap": [
              { "do": "set", "var": "shown.{index}", "value": false },
              { "do": "remove", "from": "items", "index": "{index}" },
              { "do": "set", "var": "last_deleted", "value": "{t}" } ] }
          ],
          "on_full_swipe": [
            { "do": "remove", "from": "items", "index": "{index}" },
            { "do": "set", "var": "last_deleted", "value": "{t}" } ] }
      ] }
  ] } }
}"#;

fn phone(mode: ObserveMode) -> SimPhone {
    let mut apps: Vec<AppDef> = builtin_apps()
        .unwrap()
        .into_iter()
        .filter(|a| a.id == "home")
        .collect();
    apps.push(parse_app("rows", ROWS).unwrap());
    let mut p = SimPhone::from_apps(apps).unwrap();
    p.set_mode(mode);
    p.launch("rows").unwrap();
    p
}

fn rows_setup(mode: ObserveMode) -> RunSetup {
    let mut s = setup(mode);
    s.approved_apps.push("com.example.rows".to_owned());
    s
}

fn swipe_left(spec: &str) -> String {
    format!(r#"{{"thought":"t","action":"swipe","direction":"left","element":"@{spec}"}}"#)
}

fn swipes(cmds: &[RunnerCommand]) -> Vec<(Point, Point)> {
    cmds.iter()
        .filter_map(|c| match c {
            RunnerCommand::Swipe { from, to, .. } => Some((*from, *to)),
            _ => None,
        })
        .collect()
}

/// The simulated row models the measured hazard: the session's own bounded swipe on the
/// row (0.45 of 393 pt) only reveals it from rest, and deletes it when made again.
#[test]
fn the_simulated_row_deletes_on_a_second_bounded_swipe() {
    // The path the session plans for a swipe left on the first row (run below), sent raw.
    let mut model = ScriptedModel::new([swipe_left("Cell:Row A"), done("ok")]);
    let mut p = phone(ObserveMode::Structured);
    let r = run_phone(
        &rows_setup(ObserveMode::Structured),
        &mut p,
        "Swipe the first row left",
        &mut model,
        &mut Human::approve_all(),
        MemoryStore::new(),
    );
    let sent = swipes(&r.commands);
    assert_eq!(sent.len(), 1, "{}", r.transcript());
    let (from, to) = sent[0];
    assert!(
        (from.x - to.x - 0.45 * 393.0).abs() < 1e-6,
        "{from:?} -> {to:?}"
    );
    assert!(r.final_state.check("rows.shown.0 == true").unwrap());
    assert_eq!(
        r.final_state
            .eval("rows.items")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        3
    );
    // The same path again, straight to the phone: the row is deleted.
    let out = p.act(&RunnerCommand::Swipe {
        from,
        to,
        duration_ms: 200,
        expect: None,
    });
    assert!(out.note.contains("full swipe"), "{}", out.note);
    let snap = p.snapshot();
    assert!(snap.check("rows.last_deleted == \"Row A\"").unwrap());
    assert_eq!(
        snap.eval("rows.items").unwrap().as_array().unwrap().len(),
        2
    );
}

/// A model that asks for the same swipe twice: the first is asked (`Delete`, the row-swipe
/// rule), approved and performed (the row's actions show); the second is judged on the
/// fresh look that shows the revealed row and refused before any question — nothing is
/// sent, the model is told the row's actions are showing, and no row is deleted. Both
/// observation modes.
#[test]
fn the_same_swipe_asked_twice_is_performed_once_and_then_refused() {
    for mode in MODES {
        let mut model = ScriptedModel::new([
            swipe_left("Cell:Row A"),
            swipe_left("Cell:Row A"),
            done("Swiped"),
        ]);
        let mut p = phone(mode);
        let r = run_phone(
            &rows_setup(mode),
            &mut p,
            "Swipe the first row left",
            &mut model,
            &mut Human::approve_all(),
            MemoryStore::new(),
        );
        let t = r.transcript();
        assert_eq!(swipes(&r.commands).len(), 1, "{mode:?}\n{t}");
        assert_eq!(r.confirmations.len(), 1, "{mode:?}\n{t}");
        let q = &r.confirmations[0].request;
        assert!(
            matches!(q.category, RiskCategory::Delete | RiskCategory::Unverified),
            "{mode:?}: {:?}",
            q.category
        );
        assert!(
            t.contains("swipe actions are showing"),
            "{mode:?}: the model is told\n{t}"
        );
        assert!(r.final_state.check("rows.shown.0 == true").unwrap(), "{t}");
        assert!(
            r.final_state.check("rows.last_deleted == \"\"").unwrap(),
            "{t}"
        );
        assert_eq!(
            r.final_state
                .eval("rows.items")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            3,
            "{t}"
        );
    }
}

/// The revealed row as the core reads it from the simulated phone: its cell shifted out of
/// the list, the Delete button in a sibling beside it; a tap on Delete is asked as
/// `Delete` and, approved, deletes the row (an ordinary tap judged by its label).
#[test]
fn the_revealed_delete_is_an_ordinary_tap() {
    let mut model = ScriptedModel::new([
        swipe_left("Cell:Row B"),
        tap("Button:Delete"),
        done("Deleted"),
    ]);
    let mut p = phone(ObserveMode::Structured);
    let r = run_phone(
        &rows_setup(ObserveMode::Structured),
        &mut p,
        "Delete row B",
        &mut model,
        &mut Human::approve_all(),
        MemoryStore::new(),
    );
    let t = r.transcript();
    assert_eq!(r.confirmations.len(), 2, "{t}");
    assert_eq!(
        r.confirmations[1].request.category,
        RiskCategory::Delete,
        "{t}"
    );
    assert!(
        r.final_state
            .check("rows.last_deleted == \"Row B\"")
            .unwrap(),
        "{t}"
    );
}
