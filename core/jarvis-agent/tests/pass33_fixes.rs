#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 33: the fixes' own tests (the reviewer's are `pass33_breakit.rs`).
//!
//! - F1: an inset row's drag is `max(0.45 w, 80)` but never more than
//!   `INSET_ROW_MAX_SHARE` (0.55) of it; a row for which 80 pt is more is refused, as a narrow
//!   list row is. A squarish cell with alike cells beside it that make no grid signal (a
//!   calendar under a popover's dismiss region, its lone last day) keeps pass 32's bound.
//! - F2: a row-shaped cell (at least `ROW_SHAPE_MIN_ASPECT` = 3 times as wide as its drawn
//!   height) is never a grid cell and makes no grid signal, whatever is beside it; a frame
//!   taller than its row's pitch is read at the pitch.
//! - A look that comes back without a screenshot is decided on the element list, and the
//!   model is told.

mod common;

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_observe::{compact_view, CompactView};
use jarvis_proto::{Action, Direction, Rect, RequestKind, Role, Screen, UiNode};
use jarvis_safety::gestures::{
    drag_gesture, GestureError, GestureKind, INSET_ROW_MAX_SHARE, MIN_TRAVEL, ROW_SWIPE_SHARE,
};
use jarvis_safety::{
    evaluate_owned, planned_drag, Decision, ScreenSource, ScreenTree, ROW_SWIPE_NOTE,
};

// ---------------------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------------------

macro_rules! fixture {
    ($dir:literal, $name:literal) => {
        include_str!(concat!("fixtures/", $dir, "/", $name))
    };
}

const P13_ROWS: &str = fixture!("ios26_probes3", "p13-swipe-lengths-01-rows-tree.txt");

fn compact_text(c: &jarvis_proto::CompactScreen) -> String {
    format!("{c:?}")
}

fn parse(text: &str) -> Screen {
    let typed = text
        .split("\n== raw XCUITest snapshot")
        .next()
        .unwrap_or(text);
    ios::parse_screen(typed).unwrap()
}

fn tree(v: &CompactView) -> ScreenTree<'_> {
    ScreenTree {
        sources: &v.sources,
        parents: &v.parents,
        ends: &v.ends,
        bars: &v.bars,
        nodes: &v.nodes,
        inputs: &v.inputs,
    }
}

fn edit(n: &mut UiNode, f: &mut impl FnMut(&mut UiNode)) {
    f(n);
    for c in &mut n.children {
        edit(c, f);
    }
}

fn length(g: &jarvis_safety::gestures::Gesture) -> f64 {
    (g.to.x - g.from.x).hypot(g.to.y - g.from.y)
}

/// (planned travel, the reach's travel) of a swipe left on `frame` (or element-less).
fn swipe(s: &Screen, frame: Option<Rect>) -> (Result<f64, GestureError>, f64) {
    let v = compact_view(s);
    let planned = planned_drag(
        GestureKind::Swipe,
        Direction::Left,
        frame,
        &v.screen,
        Some(&tree(&v)),
    )
    .map(|g| length(&g));
    let reach = drag_gesture(GestureKind::Swipe, Direction::Left, frame, &v.screen).unwrap();
    (planned, length(&reach))
}

fn frame_of(s: &Screen, label: &str) -> Rect {
    compact_view(s)
        .screen
        .elements
        .iter()
        .find(|e| e.label == label)
        .unwrap_or_else(|| panic!("{label}"))
        .frame
}

/// The real P13 list (iOS 26.5) with every row's `Cell` (and what spans it) at `x`,
/// `width` wide; the list keeps its 440 pt.
fn rows_at(x: f64, width: f64) -> Screen {
    let mut s = parse(P13_ROWS);
    edit(&mut s.root, &mut |n| {
        if n.role == Role::Cell {
            edit(n, &mut |k| {
                if k.frame.width >= 399.0 && k.frame.width <= 401.0 {
                    k.frame.x = x;
                    k.frame.width = width;
                }
            });
        }
    });
    s
}

/// `rows_at(20, width)` with a copy of every row at `20 + width + gap`, in the same list
/// (the reviewer's two-column list, for any width).
fn two_columns(width: f64, gap: f64) -> Screen {
    let mut s = rows_at(20.0, width);
    let right = 20.0 + width + gap;
    edit(&mut s.root, &mut |n| {
        if n.role == Role::CollectionView {
            let mut out = Vec::new();
            for c in n.children.drain(..) {
                let copy = (c.role == Role::Cell && (c.frame.x - 20.0).abs() < 0.5).then(|| {
                    let mut r = c.clone();
                    edit(&mut r, &mut |k| {
                        if (k.frame.x - 20.0).abs() < 0.5 && (k.frame.width - width).abs() < 0.5 {
                            k.frame.x = right;
                        }
                        if let Some(l) = &k.label {
                            k.label = Some(format!("Right {l}"));
                        }
                    });
                    r
                });
                out.push(c);
                out.extend(copy);
            }
            n.children = out;
        }
    });
    s
}

fn cell(frame: Rect, label: &str) -> UiNode {
    UiNode::new(Role::Cell, frame)
        .with_children(vec![UiNode::new(Role::StaticText, frame).with_label(label)])
}

/// `s` with `extra` inserted into the list right after the `Cell` of "Full swipe row 1"
/// (`y = 278`).
fn beside_row1(mut s: Screen, extra: Vec<UiNode>) -> Screen {
    let mut extra = Some(extra);
    edit(&mut s.root, &mut |n| {
        if n.role == Role::CollectionView {
            if let Some(at) = n
                .children
                .iter()
                .position(|c| c.role == Role::Cell && (c.frame.y - 278.0).abs() < 0.5)
            {
                if let Some(extra) = extra.take() {
                    for (k, e) in extra.into_iter().enumerate() {
                        n.children.insert(at + 1 + k, e);
                    }
                }
            }
        }
    });
    assert!(extra.is_none(), "inserted");
    s
}

/// Every `Cell` of `s` whose frame starts at `x` made `height` tall (what it holds too).
fn taller(mut s: Screen, x: f64, height: f64) -> Screen {
    edit(&mut s.root, &mut |n| {
        if n.role == Role::Cell && (n.frame.x - x).abs() < 0.5 {
            edit(n, &mut |k| {
                if (k.frame.x - x).abs() < 0.5 && k.frame.height >= 51.0 {
                    k.frame.height = height;
                }
            });
        }
    });
    s
}

fn decide(a: &Action, s: &Screen) -> Decision {
    let v = compact_view(s);
    evaluate_owned(
        &config().policy,
        a,
        &v.screen,
        None,
        ScreenSource::Typed,
        Some(&tree(&v)),
        &[],
        &[],
    )
}

fn asks_delete_with_the_note(s: &Screen, label: &str) {
    asks_delete(s, label, true);
}

/// The element swipe (and, with `element_less`, the element-less one) on `label` asks as
/// `Delete`, phone only, with the row-swipe note.
fn asks_delete(s: &Screen, label: &str, element_less: bool) {
    let v = compact_view(s);
    let el = v
        .screen
        .elements
        .iter()
        .find(|e| e.label == label)
        .unwrap()
        .index;
    for element in [Some(el), None]
        .into_iter()
        .take(if element_less { 2 } else { 1 })
    {
        let a = Action::Swipe {
            direction: Direction::Left,
            element,
        };
        let Decision::Confirm(q) = decide(&a, s) else {
            panic!("{label} {element:?}: not asked")
        };
        assert_eq!(q.category, jarvis_proto::RiskCategory::Delete);
        assert!(q.phone_only && q.summary.contains(ROW_SWIPE_NOTE));
    }
}

// ---------------------------------------------------------------------------------------
// F1: inset rows.
// ---------------------------------------------------------------------------------------

/// Every inset width of the real P13 rows from 60 to 350 pt (all under 0.8 of the 440-pt
/// list): the element and the element-less swipe are refused exactly when 80 pt is more
/// than 0.55 of the row, else travel at most `max(0.45 w, 80)` and at most 0.55 w, never
/// past the reach.
#[test]
fn f1_inset_rows_of_every_width_are_bounded_below_the_delete_share_or_refused() {
    let mut refused = 0;
    for w in (60..=350).step_by(2) {
        let width = f64::from(w);
        let s = rows_at((440.0 - width) / 2.0, width);
        for frame in [Some(frame_of(&s, "Full swipe row 1")), None] {
            let (planned, reach) = swipe(&s, frame);
            if INSET_ROW_MAX_SHARE * width < MIN_TRAVEL {
                assert_eq!(planned, Err(GestureError::RowTooNarrow), "{w} {frame:?}");
                refused += 1;
                continue;
            }
            let t = planned.unwrap_or_else(|e| panic!("{w} {frame:?}: {e}"));
            assert!(
                t <= (ROW_SWIPE_SHARE * width).max(MIN_TRAVEL) + 1e-6,
                "{w}: {t}"
            );
            assert!(t <= INSET_ROW_MAX_SHARE * width + 1e-6, "{w}: {t}");
            assert!(t < 0.60 * width, "{w}: {t}");
            assert!(t <= reach + 1e-6);
        }
    }
    assert!(refused > 0);
}

/// The boundary: 146 pt gets 80 pt (0.548), 145 pt is refused (80 would be 0.552); pass
/// 31's 150-pt cell gets 80 pt (0.53).
#[test]
fn f1_the_boundary_of_the_refusal() {
    for (width, want) in [
        (146.0, Some(MIN_TRAVEL)),
        (145.0, None),
        (150.0, Some(MIN_TRAVEL)),
    ] {
        let s = rows_at((440.0 - width) / 2.0, width);
        let (t, _) = swipe(&s, Some(frame_of(&s, "Full swipe row 1")));
        match want {
            Some(v) => assert!((t.unwrap() - v).abs() < 1e-6, "{width}"),
            None => assert_eq!(t, Err(GestureError::RowTooNarrow), "{width}"),
        }
    }
}

/// The question is unchanged on the refused and the bounded inset rows (asked as
/// `Delete`, phone only, with the row-swipe note, which now says 0.60 to 0.69).
#[test]
fn f1_the_question_and_the_note_hold() {
    assert!(ROW_SWIPE_NOTE.contains("0.60 to 0.69"));
    for width in [100.0, 132.0, 150.0, 300.0] {
        let s = rows_at((440.0 - width) / 2.0, width);
        asks_delete_with_the_note(&s, "Full swipe row 1");
    }
}

/// The compact date picker's calendar (a popover: the full-screen "dismiss popup" button
/// is drawn after it, so its days make no grid signal): a day is a squarish cell with alike
/// days beside it — grid-like: its drag keeps pass 32's bound (80 pt, never refused) — and
/// so is the last row's lone day (Sunday, January 31: alike days side by side in the rows
/// above). Before this pass's split it would have been refused as a narrow inset row.
#[test]
fn f1_a_calendar_under_a_popover_keeps_its_drag() {
    let s = parse(fixture!(
        "ios26_probes3",
        "p4d-controls-10-compact-date-open-tree.txt"
    ));
    for day in [
        "Friday, January 1",
        "Thursday, January 7",
        "Sunday, January 31",
    ] {
        let f = frame_of(&s, day);
        let (t, reach) = swipe(&s, Some(f));
        let t = t.unwrap_or_else(|e| panic!("{day}: {e}"));
        assert!(
            (t - reach).abs() < 1e-6 && t <= MIN_TRAVEL + 1e-6,
            "{day}: {t} {reach}"
        );
    }
}

/// A select's native list (Safari, p6b): its rows span the list (list rows, 0.45 of
/// 250 pt), and the 30-pt "Vertical scroll bar" the list holds directly (no `Cell` role)
/// keeps pass 32's floor: the swipe on a row is bounded, not refused.
#[test]
fn f1_a_scroll_indicator_beside_list_rows_refuses_nothing() {
    let s = parse(fixture!(
        "ios26_probes3",
        "p6b-safari-inputs-01-select-tree.txt"
    ));
    for row in ["Alpha", "Beta", "Gamma"] {
        let (t, _) = swipe(&s, Some(frame_of(&s, row)));
        let t = t.unwrap_or_else(|e| panic!("{row}: {e}"));
        assert!(t <= ROW_SWIPE_SHARE * 250.0 + 1e-6, "{row}: {t}");
    }
}

/// Forgeries against the refusal: a narrow inset row given an alike neighbour that is
/// covered (grid-like) gets pass 32's 80-pt floor, not more; an alike neighbour that is
/// row-shaped, much narrower or in another list makes nothing (refused). An alike,
/// uncovered, squarish neighbour is the documented forger residual (the screen cap; the
/// question still asks).
#[test]
fn f1_forged_neighbours_of_a_narrow_inset_row() {
    let (x, w) = (170.0, 100.0);
    let right = Rect::new(x + w, 278.0, w, 52.0);
    // Covered by a later branch's text.
    let mut covered = beside_row1(rows_at(x, w), vec![cell(right, "Covered")]);
    let cover = UiNode::new(
        Role::Other("Other".into()),
        Rect::new(0.0, 0.0, 440.0, 956.0),
    )
    .with_children(vec![UiNode::new(
        Role::StaticText,
        Rect::new(x + w + 10.0, 284.0, 80.0, 40.0),
    )
    .with_label("Banner")]);
    edit(&mut covered.root, &mut |n| {
        if n.role == Role::Window {
            n.children.push(cover.clone());
        }
    });
    let f = frame_of(&covered, "Full swipe row 1");
    let (t, _) = swipe(&covered, Some(f));
    assert!(t.unwrap() <= MIN_TRAVEL + 1e-6, "covered");
    let inner_list = UiNode::new(Role::CollectionView, Rect::new(x + w, 0.0, 170.0, 956.0))
        .with_children(vec![cell(right, "Other list")]);
    for (name, extra) in [
        (
            "row-shaped",
            vec![cell(Rect::new(x + w, 278.0, w, 30.0), "Flat")],
        ),
        (
            "much narrower",
            vec![cell(Rect::new(x + w, 278.0, 40.0, 52.0), "Thin")],
        ),
        ("another list", vec![inner_list]),
    ] {
        let s = beside_row1(rows_at(x, w), extra);
        let (t, _) = swipe(&s, Some(frame_of(&s, "Full swipe row 1")));
        assert_eq!(t, Err(GestureError::RowTooNarrow), "{name}");
    }
    // The residual: an alike, uncovered, squarish neighbour (the question still asks).
    let s = beside_row1(rows_at(x, w), vec![cell(right, "Alike")]);
    let (t, reach) = swipe(&s, Some(frame_of(&s, "Full swipe row 1")));
    println!("alike uncovered neighbour of a 100-pt row: {t:?} (reach {reach})");
    asks_delete_with_the_note(&s, "Full swipe row 1");
}

// ---------------------------------------------------------------------------------------
// F2: row-shaped cells.
// ---------------------------------------------------------------------------------------

/// Two-column lists of the real P13 rows, every width from 156 pt (3 times the 52-pt row)
/// to 210 pt and gaps of 0 to 30 pt: no swipe on either column travels more than the
/// row's inset bound, and every one asks.
#[test]
fn f2_two_column_lists_of_rows_are_bounded_at_every_width() {
    for w in (156..=210).step_by(6) {
        let width = f64::from(w);
        for gap in [0.0, 10.0, 30.0] {
            if 20.0 + 2.0 * width + gap > 440.0 {
                continue;
            }
            let s = two_columns(width, gap);
            let bound = (ROW_SWIPE_SHARE * width).max(MIN_TRAVEL);
            for frame in [
                Some(frame_of(&s, "Full swipe row 1")),
                Some(frame_of(&s, "Right Full swipe row 1")),
                None,
            ] {
                let (t, _) = swipe(&s, frame);
                let t = t.unwrap_or_else(|e| panic!("{w} {gap} {frame:?}: {e}"));
                assert!(t <= bound + 1e-6, "{w} {gap} {frame:?}: {t} > {bound}");
                assert!(t <= INSET_ROW_MAX_SHARE * width + 1e-6);
            }
            asks_delete_with_the_note(&s, "Right Full swipe row 1");
        }
    }
}

/// A row-shaped neighbour makes no grid signal for a squarish cell beside it (it is left
/// out of the band): a 150 x 52 row with an alike 160 x 52 row beside it is an inset row.
#[test]
fn f2_a_row_shaped_neighbour_makes_no_signal() {
    let s = beside_row1(
        rows_at(20.0, 150.0),
        vec![cell(Rect::new(180.0, 278.0, 160.0, 52.0), "Wide")],
    );
    let (t, reach) = swipe(&s, Some(frame_of(&s, "Full swipe row 1")));
    assert!((t.unwrap() - MIN_TRAVEL).abs() < 1e-6 && reach > MIN_TRAVEL + 5.0);
}

/// The forged taller frame: the two-column rows' `Cell` frames (and what spans them) made
/// 190 pt tall — squarish by their frames (1.0) — over the rows below them. The rule reads
/// each row at its pitch (cut where the next row of its column starts: 52 pt), so it is
/// row-shaped and bounded. Only the first row of each column forged, or every row: the same.
#[test]
fn f2_a_forged_taller_frame_is_read_at_the_rows_pitch() {
    let base = two_columns(190.0, 20.0);
    let all = taller(taller(base.clone(), 20.0, 190.0), 230.0, 190.0);
    let bound = ROW_SWIPE_SHARE * 190.0;
    for (name, s) in [("every row", all)] {
        for frame in [
            Some(frame_of(&s, "Full swipe row 1")),
            Some(frame_of(&s, "Right Full swipe row 1")),
            None,
        ] {
            let (t, _) = swipe(&s, frame);
            let t = t.unwrap_or_else(|e| panic!("{name} {frame:?}: {e}"));
            assert!(t <= bound + 1e-6, "{name} {frame:?}: {t}");
        }
        asks_delete_with_the_note(&s, "Right Full swipe row 1");
    }
    // Only row 1 of each column forged tall (rows 2.. below it, honest).
    let mut one = base;
    edit(&mut one.root, &mut |n| {
        if n.role == Role::Cell && (n.frame.y - 278.0).abs() < 0.5 {
            edit(n, &mut |k| {
                if k.frame.height >= 51.0 {
                    k.frame.height = 190.0;
                }
            });
        }
    });
    let (t, _) = swipe(&one, Some(frame_of(&one, "Full swipe row 1")));
    assert!(t.unwrap() <= bound + 1e-6, "row 1 only");
}

/// The forged taller frame with nothing below it in its list: clipped to the list's drawn
/// frame (a list that ends at its last row reads the row at its own height: bounded). A
/// list with room below its last row cannot be told from a squarish card (the documented
/// residual): printed, and the question still asks.
#[test]
fn f2_a_forged_taller_frame_with_nothing_below() {
    // One row in each column, the list ending at the row (clipped: bounded).
    let one_row = |list_height: f64| {
        let tall = |x: f64, label: &str| cell(Rect::new(x, 278.0, 190.0, 190.0), label);
        let list = UiNode::new(
            Role::CollectionView,
            Rect::new(0.0, 278.0, 440.0, list_height),
        )
        .with_children(vec![tall(20.0, "Left row"), tall(230.0, "Right row")]);
        let root = UiNode::new(Role::Application, Rect::new(0.0, 0.0, 440.0, 956.0))
            .with_label("App")
            .with_children(vec![UiNode::new(
                Role::Window,
                Rect::new(0.0, 0.0, 440.0, 956.0),
            )
            .with_children(vec![list])]);
        let mut s = jarvis_proto::fixtures::screen("com.example.app", "App", root, false);
        s.size = jarvis_proto::Size::new(440.0, 956.0);
        s
    };
    let clipped = one_row(52.0);
    let (t, _) = swipe(&clipped, Some(frame_of(&clipped, "Left row")));
    assert!(t.unwrap() <= ROW_SWIPE_SHARE * 190.0 + 1e-6, "clipped");
    let room = one_row(600.0);
    let (t, reach) = swipe(&room, Some(frame_of(&room, "Left row")));
    println!("forged 190 x 190 cells with room below: {t:?} (reach {reach})");
    // The element-less path (at the screen's middle) does not cross these rows.
    asks_delete(&room, "Left row", false);
}

/// The real grids keep their grid reading with the row-shape rule: the widest real grid
/// cell (the edit menu's 98.7 x 44 item, 2.24) keeps its reach, as do Safari's favourites
/// and the month calendar (`pass32_fixes.rs`).
#[test]
fn f2_the_widest_real_grid_cell_keeps_its_reach() {
    let s = parse(fixture!("ios26_probes", "edit_menu.txt"));
    let (t, reach) = swipe(&s, Some(Rect::new(148.3, 220.0, 98.7, 44.0)));
    assert!((t.unwrap() - reach).abs() < 1e-6, "{reach}");
}

// ---------------------------------------------------------------------------------------
// A look without a screenshot.
// ---------------------------------------------------------------------------------------

/// A `look` that comes back without a screenshot (the runner took none, or one too large to
/// send was left out, `jarvis_runner::message::SCREENSHOT_TOO_LARGE_NOTE`): the next request
/// is a text request with no image, and the model is told no picture came; with a
/// screenshot the request is a vision one and carries no such note.
#[test]
fn a_look_without_a_screenshot_is_decided_on_the_element_list() {
    let screen = jarvis_proto::fixtures::settings_screen();
    let look = r#"{"action":"look"}"#;
    let fail = r#"{"action":"fail","reason":"stop"}"#;
    let mut phone = Phone::still(screen.clone());
    phone.can_screenshot = false;
    let r = run(
        vision_config(),
        "Open Wi-Fi",
        MemoryStore::new(),
        phone,
        Script::model(&[look, fail]),
    );
    assert!(r.observes.iter().any(|s| *s), "a screenshot was asked for");
    let after = r.requests.get(1).expect("asked again");
    assert!(after.image.is_none());
    assert_eq!(after.kind, RequestKind::Text);
    assert!(after.user.contains(jarvis_agent::NO_SCREENSHOT_NOTE));
    let r = run(
        vision_config(),
        "Open Wi-Fi",
        MemoryStore::new(),
        Phone::still(screen),
        Script::model(&[look, fail]),
    );
    let after = r.requests.get(1).expect("asked again");
    assert!(after.image.is_some());
    assert_eq!(after.kind, RequestKind::Vision);
    assert!(!r
        .all_prompt_text()
        .contains(jarvis_agent::NO_SCREENSHOT_NOTE));
}
