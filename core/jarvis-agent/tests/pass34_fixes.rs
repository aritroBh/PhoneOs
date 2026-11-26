#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 34: the fixes' own tests (the reviewer's are `pass34_breakit.rs`).
//!
//! - F1: every crossed cell of a list is bounded at its own width, whatever it is read as:
//!   a grid cell no longer lifts the bound to the screen cap. A grid cell (or what cannot be
//!   a row) gets `max(0.45 w, 80)`, never refused; any other row that is no list row gets
//!   the same but is refused when that is more than 0.55 of it. From 146 pt up, no planned
//!   sideways drag travels more than 0.55 of the narrowest cell it crosses.
//! - F2: what a `Table`/`CollectionView` holds directly that holds text or a control (and is
//!   at least 20 pt wide) is a row exactly as a `Cell` is, whatever its role; a node with no
//!   text and no control in it (a scroll indicator, a separator) keeps the floor.

mod common;

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use common::*;
use jarvis_observe::{compact_view, CompactView};
use jarvis_proto::{Action, Direction, Rect, RiskCategory, Role, Screen, Size, UiNode};
use jarvis_safety::gestures::{
    drag_gesture, Gesture, GestureError, GestureKind, INSET_ROW_MAX_SHARE, MIN_TRAVEL,
    ROW_SWIPE_SHARE,
};
use jarvis_safety::{evaluate_owned, planned_drag, Decision, ScreenSource, ScreenTree};

// ---------------------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------------------

macro_rules! fixture {
    ($dir:literal, $name:literal) => {
        include_str!(concat!("fixtures/", $dir, "/", $name))
    };
}

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

fn length(g: &Gesture) -> f64 {
    (g.to.x - g.from.x).hypot(g.to.y - g.from.y)
}

/// The four sideways drags: swipes left and right, scrolls left and right.
const SIDEWAYS: [(GestureKind, Direction); 4] = [
    (GestureKind::Swipe, Direction::Left),
    (GestureKind::Swipe, Direction::Right),
    (GestureKind::Scroll, Direction::Left),
    (GestureKind::Scroll, Direction::Right),
];

/// (planned drag, the reach) of `kind` `direction` on `frame` (or element-less).
fn drag(
    v: &CompactView,
    kind: GestureKind,
    direction: Direction,
    frame: Option<Rect>,
) -> (Result<Gesture, GestureError>, Gesture) {
    let planned = planned_drag(kind, direction, frame, &v.screen, Some(&tree(v)));
    let reach = drag_gesture(kind, direction, frame, &v.screen).unwrap();
    (planned, reach)
}

/// (planned travel, the reach's travel) of a swipe left on `frame` (or element-less).
fn swipe(s: &Screen, frame: Option<Rect>) -> (Result<f64, GestureError>, f64) {
    let v = compact_view(s);
    let (p, r) = drag(&v, GestureKind::Swipe, Direction::Left, frame);
    (p.map(|g| length(&g)), length(&r))
}

fn frame_of(s: &Screen, label: &str) -> Rect {
    compact_view(s)
        .screen
        .elements
        .iter()
        .find(|e| e.label == label || e.label.starts_with(&format!("{label},")))
        .unwrap_or_else(|| panic!("{label}"))
        .frame
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

fn describe(d: &Decision) -> String {
    match d {
        Decision::Allow => "allow".into(),
        Decision::Deny(_) => "deny".into(),
        Decision::Confirm(q) => format!(
            "confirm {:?}{}",
            q.category,
            if q.phone_only { " phone-only" } else { "" }
        ),
    }
}

/// The element-less swipe left on `s` asks as `Delete`, phone only.
fn element_less_asks_delete(s: &Screen, what: &str) {
    let a = Action::Swipe {
        direction: Direction::Left,
        element: None,
    };
    let Decision::Confirm(q) = decide(&a, s) else {
        panic!("{what}: not asked")
    };
    assert_eq!(q.category, RiskCategory::Delete, "{what}");
    assert!(q.phone_only, "{what}");
}

/// A 440 x 956 screen (the P13 phone) holding `list` in its window.
fn phone_with(list: UiNode) -> Screen {
    let full = Rect::new(0.0, 0.0, 440.0, 956.0);
    let root = UiNode::new(Role::Application, full)
        .with_label("Notes")
        .with_children(vec![
            UiNode::new(Role::Window, full).with_children(vec![list])
        ]);
    let mut s = jarvis_proto::fixtures::screen("com.example.notes", "Notes", root, false);
    s.size = Size::new(440.0, 956.0);
    s
}

/// A full-screen `CollectionView` holding `rows` directly.
fn list_of(rows: Vec<UiNode>) -> Screen {
    phone_with(
        UiNode::new(Role::CollectionView, Rect::new(0.0, 0.0, 440.0, 956.0)).with_children(rows),
    )
}

/// A row of `role` at `frame`; with `text`, holding a title line (and a subtitle line when
/// it is taller than 52 pt), as a list cell does.
fn row(role: Role, frame: Rect, label: &str, text: bool) -> UiNode {
    let mut kids = Vec::new();
    if text {
        kids.push(
            UiNode::new(
                Role::StaticText,
                Rect::new(
                    frame.x + 8.0,
                    frame.y + 4.0,
                    (frame.width - 16.0).max(1.0),
                    20.0,
                ),
            )
            .with_label(label),
        );
        if frame.height > 52.0 {
            kids.push(
                UiNode::new(
                    Role::StaticText,
                    Rect::new(
                        frame.x + 8.0,
                        frame.y + 26.0,
                        (frame.width - 16.0).max(1.0),
                        20.0,
                    ),
                )
                .with_label(format!("{label} preview")),
            );
        }
    }
    UiNode::new(role, frame).with_children(kids)
}

fn cell(frame: Rect, label: &str) -> UiNode {
    row(Role::Cell, frame, label, true)
}

/// The y of the element-less sideways path on the 440 x 956 phone (the safe area's centre).
fn path_y() -> f64 {
    let s = list_of(Vec::new());
    let v = compact_view(&s);
    drag_gesture(GestureKind::Swipe, Direction::Left, None, &v.screen)
        .unwrap()
        .from
        .y
}

/// Columns of `Cell` rows: column `i` at `xs[i]`, `widths[i]` wide, its rows `heights[i]`
/// tall, stacked from y = 160 to the screen's bottom; labelled `C{i} row {k}`.
fn columns(xs: &[f64], widths: &[f64], heights: &[f64]) -> Screen {
    let mut rows = Vec::new();
    for (i, ((&x, &w), &h)) in xs.iter().zip(widths).zip(heights).enumerate() {
        let mut y = 160.0;
        let mut k = 0;
        while y + h <= 900.0 {
            rows.push(cell(Rect::new(x, y, w, h), &format!("C{i} row {k}")));
            y += h;
            k += 1;
        }
    }
    list_of(rows)
}

/// The label of the row of column `i` (rows `h` tall from y = 160) the element-less path
/// crosses.
fn crossed_label(i: usize, h: f64) -> String {
    let k = ((path_y() - 160.0) / h).floor();
    format!("C{i} row {k}")
}

/// `n` equal columns `w` wide spread over the 440-pt list.
fn spread(n: usize, w: f64) -> Vec<f64> {
    let count = f64::from(u32::try_from(n).unwrap());
    let m = ((440.0 - count * w) / (count + 1.0)).max(0.0);
    (0..n)
        .map(|i| m + f64::from(u32::try_from(i).unwrap()) * (w + m))
        .collect()
}

// ---------------------------------------------------------------------------------------
// F1: multi-column lists of every row height.
// ---------------------------------------------------------------------------------------

/// Two-column lists of `Cell` rows 150 to 210 pt wide and 44 to 160 pt tall: every
/// sideways drag (element-less or on a row of either column; swipes and scrolls, both
/// ways) travels at most 0.55 of the row, from the start the gate read, never refused;
/// every swipe asks as `Delete`, phone only. Before this pass a row under 3 times as wide
/// as tall was a grid cell beside its alike neighbour: the element-less drag kept the
/// 240-pt screen cap (1.26 of a 190-pt row).
#[test]
fn f1_two_column_rows_of_every_height_and_width_are_bounded() {
    for w in (150..=210).step_by(10) {
        let w = f64::from(w);
        for h in [
            44.0, 52.0, 56.0, 64.0, 70.0, 80.0, 90.0, 100.0, 120.0, 140.0, 160.0,
        ] {
            let s = columns(&spread(2, w), &[w, w], &[h, h]);
            let v = compact_view(&s);
            let frames = [
                None,
                Some(frame_of(&s, &crossed_label(0, h))),
                Some(frame_of(&s, &crossed_label(1, h))),
                Some(frame_of(&s, "C0 row 1")),
            ];
            for frame in frames {
                for (kind, direction) in SIDEWAYS {
                    let (p, r) = drag(&v, kind, direction, frame);
                    let p = p.unwrap_or_else(|e| panic!("{w}x{h} {frame:?} {kind:?}: {e}"));
                    let t = length(&p);
                    assert!(
                        t <= INSET_ROW_MAX_SHARE * w + 1e-6,
                        "{w}x{h} {frame:?} {kind:?} {direction:?}: {t} = {:.2} of the row",
                        t / w
                    );
                    assert!(t <= length(&r) + 1e-6 && p.from == r.from);
                }
            }
            if h == 64.0 || h == 160.0 {
                element_less_asks_delete(&s, &format!("{w}x{h}"));
            }
        }
    }
}

/// Three columns of `Cell` rows: from 146 pt up a crossed row's drag is at most 0.55 of
/// it; narrower, a row-shaped one is refused and a squarish one (a grid cell: alike cells
/// side by side) gets at most `MIN_TRAVEL` — the stated residual — never the screen cap.
#[test]
fn f1_three_column_rows_are_bounded() {
    for w in [80.0, 100.0, 110.0, 120.0, 130.0, 140.0, 146.0] {
        for h in [30.0, 44.0, 52.0, 64.0, 80.0, 100.0, 120.0] {
            let s = columns(&spread(3, w), &[w, w, w], &[h, h, h]);
            let v = compact_view(&s);
            for frame in [
                None,
                Some(frame_of(&s, &crossed_label(0, h))),
                Some(frame_of(&s, &crossed_label(1, h))),
                Some(frame_of(&s, &crossed_label(2, h))),
            ] {
                for (kind, direction) in SIDEWAYS {
                    let (p, _) = drag(&v, kind, direction, frame);
                    let squarish = w < 3.0 * h;
                    match p {
                        Err(GestureError::RowTooNarrow) => {
                            assert!(!squarish, "{w}x{h} {frame:?}: a grid cell refused");
                        }
                        Ok(g) => {
                            let t = length(&g);
                            assert!(
                                t <= (INSET_ROW_MAX_SHARE * w).max(MIN_TRAVEL) + 1e-6,
                                "{w}x{h} {frame:?} {kind:?}: {t}"
                            );
                            if w >= 146.0 {
                                assert!(t <= INSET_ROW_MAX_SHARE * w + 1e-6, "{w}x{h}: {t}");
                            }
                        }
                        Err(e) => panic!("{w}x{h} {frame:?}: {e}"),
                    }
                }
            }
            element_less_asks_delete(&s, &format!("3 x {w}x{h}"));
        }
    }
}

/// A mixed layout: a column of tall rows (190 x 100) beside a column of short ones
/// (190 x 52) — not alike, so no grid: each row is an inset row, at most 0.45 of it.
#[test]
fn f1_a_tall_row_beside_a_short_one_is_bounded() {
    let s = columns(&spread(2, 190.0), &[190.0, 190.0], &[100.0, 52.0]);
    let v = compact_view(&s);
    for frame in [
        None,
        Some(frame_of(&s, &crossed_label(0, 100.0))),
        Some(frame_of(&s, &crossed_label(1, 52.0))),
    ] {
        for (kind, direction) in SIDEWAYS {
            let t = length(&drag(&v, kind, direction, frame).0.unwrap());
            assert!(
                t <= ROW_SWIPE_SHARE * 190.0 + 1e-6,
                "{frame:?} {kind:?}: {t}"
            );
        }
    }
    element_less_asks_delete(&s, "mixed");
}

// ---------------------------------------------------------------------------------------
// F1: forged neighbours on squarish cells.
// ---------------------------------------------------------------------------------------

/// One column of squarish `Cell` rows `w` x `h` at `x`, with `extra` in the list right
/// after the row the element-less path crosses (`Target`).
fn squarish_with(x: f64, w: f64, h: f64, extra: Vec<UiNode>) -> (Screen, Rect) {
    let k = ((path_y() - 160.0) / h).floor();
    let target = Rect::new(x, 160.0 + k * h, w, h);
    let mut rows = Vec::new();
    let mut y = 160.0;
    let mut extra = Some(extra);
    while y + h <= 900.0 {
        if (y - target.y).abs() < 0.5 {
            rows.push(cell(target, "Target"));
            rows.extend(extra.take().unwrap_or_default());
        } else {
            rows.push(cell(Rect::new(x, y, w, h), "Row"));
        }
        y += h;
    }
    (list_of(rows), target)
}

/// Every forged neighbour of pass 32 (hidden, zero-sized, off screen, partly off screen,
/// 1 pt wide, 200 slivers, in another list, covered by text, much shorter, much narrower,
/// overlapping) and an honest alike one, beside a squarish row: a 190 x 100 row is never
/// dragged more than 0.55 of it (0.45: `max(0.45 w, 80)` for a grid cell and an inset row
/// alike); a 100 x 52 row is refused or gets at most `MIN_TRAVEL` (the residual: only an
/// alike, drawn neighbour — covered or not — spares it the refusal). Before this pass, the
/// honest neighbour lifted the element-less drag to the 240-pt screen cap.
#[test]
fn f1_forged_neighbours_of_squarish_rows_never_lift_the_bound() {
    for (x, w, h) in [
        (20.0, 190.0, 100.0),
        (20.0, 100.0, 52.0),
        (20.0, 120.0, 90.0),
    ] {
        let k = ((path_y() - 160.0) / h).floor();
        let y = 160.0 + k * h;
        let right = Rect::new(x + w + 10.0, y, w, h);
        let mut hidden = cell(right, "Hidden");
        hidden.visible = false;
        for c in &mut hidden.children {
            c.visible = false;
        }
        let slivers: Vec<UiNode> = (0..200)
            .map(|i| {
                cell(
                    Rect::new(x + w + f64::from(i), y, 1.0, h),
                    &format!("Sliver {i}"),
                )
            })
            .collect();
        let inner = UiNode::new(
            Role::CollectionView,
            Rect::new(x + w + 5.0, 0.0, 440.0 - x - w - 5.0, 956.0),
        )
        .with_children(vec![cell(right, "Other list")]);
        // Text drawn over its centre by a later branch of the window, outside the list.
        let cover = UiNode::new(
            Role::Other("Other".into()),
            Rect::new(0.0, 0.0, 440.0, 956.0),
        )
        .with_children(vec![UiNode::new(
            Role::StaticText,
            Rect::new(
                right.x + 4.0,
                right.y + 4.0,
                right.width - 8.0,
                right.height - 8.0,
            ),
        )
        .with_label("Banner")]);
        let forgeries: Vec<(&str, Vec<UiNode>)> = vec![
            ("hidden", vec![hidden]),
            (
                "zero-sized",
                vec![cell(Rect::new(right.x, y, 0.0, 0.0), "Zero")],
            ),
            ("off screen", vec![cell(Rect::new(500.0, y, w, h), "Off")]),
            (
                "partly off screen",
                vec![cell(Rect::new(430.0, y, w, h), "Edge")],
            ),
            (
                "1 pt wide",
                vec![cell(Rect::new(right.x, y, 1.0, h), "Thin")],
            ),
            ("200 slivers", slivers),
            ("another list", vec![inner]),
            ("covered", vec![cell(right, "Covered")]),
            (
                "much shorter",
                vec![cell(Rect::new(right.x, y + 10.0, w, 5.0), "Short")],
            ),
            (
                "much narrower",
                vec![cell(Rect::new(right.x, y, w * 0.4, h), "Narrow")],
            ),
            (
                "overlapping",
                vec![cell(Rect::new(x + w * 0.5, y, w, h), "Over")],
            ),
            ("alike (honest)", vec![cell(right, "Alike")]),
        ];
        for (name, extra) in forgeries {
            let (mut s, target) = squarish_with(x, w, h, extra);
            if name == "covered" {
                if let Some(window) = s.root.children.first_mut() {
                    window.children.push(cover.clone());
                }
            }
            let v = compact_view(&s);
            for frame in [None, Some(target)] {
                for (kind, direction) in SIDEWAYS {
                    let (p, r) = drag(&v, kind, direction, frame);
                    match p {
                        Ok(g) => {
                            let t = length(&g);
                            let cap = if w >= 146.0 {
                                INSET_ROW_MAX_SHARE * w
                            } else {
                                MIN_TRAVEL
                            };
                            assert!(
                                t <= cap + 1e-6,
                                "{w}x{h} {name} {frame:?} {kind:?}: {t} (reach {})",
                                length(&r)
                            );
                        }
                        // Refused (stricter): the narrow row with no grid signal, or a
                        // narrow forged cell the path crosses (a 1-pt sliver, the text of
                        // a zero-sized cell read at its own width). An alike neighbour,
                        // covered or not, is a grid signal: never refused.
                        Err(GestureError::RowTooNarrow) => {
                            assert!(
                                !matches!(name, "alike (honest)" | "covered"),
                                "{w}x{h} {name} {frame:?}: refused"
                            );
                        }
                        Err(e) => panic!("{w}x{h} {name}: {e}"),
                    }
                }
            }
            element_less_asks_delete(&s, &format!("{w}x{h} {name}"));
        }
    }
}

// ---------------------------------------------------------------------------------------
// F2: rows a list holds without the `Cell` role.
// ---------------------------------------------------------------------------------------

/// The bound the rule gives one crossed row of a single centred column (no grid): `None`
/// refused.
fn expected_single(row_like: bool, w: f64) -> Option<f64> {
    if w >= 0.8 * 440.0 {
        let b = ROW_SWIPE_SHARE * w;
        return (b >= MIN_TRAVEL).then_some(b);
    }
    let b = (ROW_SWIPE_SHARE * w).max(MIN_TRAVEL);
    if row_like && b > INSET_ROW_MAX_SHARE * w {
        return None;
    }
    Some(b)
}

/// One centred column of rows of every width from 20 to 440 pt, as `Cell`s, as `Other`s
/// holding text, as bare `Button`s (a control) and as bare `Other`s (no text, no control):
/// the first three are bounded alike (refused under 145.5 pt, else `max(0.45 w, 80)`, at
/// most 0.55 of the row; list rows from 0.8 of the list at 0.45); only the bare `Other`
/// (which cannot be a row) keeps the never-refused floor.
#[test]
fn f2_rows_without_the_cell_role_are_rows() {
    for w in (20..=440).step_by(10) {
        let w = f64::from(w);
        for (name, role, text, row_like) in [
            ("Cell", Role::Cell, true, true),
            ("Other with text", Role::Other("Other".into()), true, true),
            ("bare Button", Role::Button, false, true),
            ("bare Other", Role::Other("Other".into()), false, false),
        ] {
            let x = (440.0 - w) / 2.0;
            let rows = (0..14)
                .map(|i| {
                    row(
                        role.clone(),
                        Rect::new(x, 160.0 + 52.0 * f64::from(i), w, 52.0),
                        &format!("Note {i}"),
                        text,
                    )
                })
                .collect();
            let s = list_of(rows);
            let v = compact_view(&s);
            let k = ((path_y() - 160.0) / 52.0).floor();
            let target = Rect::new(x, 160.0 + 52.0 * k, w, 52.0);
            let want = expected_single(row_like, w);
            for frame in [None, Some(target)] {
                let (p, r) = drag(&v, GestureKind::Swipe, Direction::Left, frame);
                match (want, p) {
                    (None, Err(GestureError::RowTooNarrow)) => {}
                    (Some(b), Ok(g)) => {
                        let t = length(&g);
                        let expect = b.min(length(&r));
                        assert!(
                            (t - expect).abs() < 1e-6,
                            "{name} {w} {frame:?}: {t}, expected {expect}"
                        );
                        if row_like && w >= 146.0 {
                            assert!(t <= INSET_ROW_MAX_SHARE * w + 1e-6);
                        }
                    }
                    (want, got) => panic!("{name} {w} {frame:?}: {got:?}, expected {want:?}"),
                }
            }
        }
    }
}

/// A scroll indicator (an `Other` labelled `Vertical scroll bar, 3 pages`, holding only an
/// unlabelled bar) and a horizontal one beside full-width `Cell` rows: neither is a row (no
/// text and no control in it), so a swipe on a row beside them is bounded, never refused.
#[test]
fn f2_a_scroll_indicator_beside_rows_refuses_nothing() {
    let mut rows: Vec<UiNode> = (0..14)
        .map(|i| {
            cell(
                Rect::new(20.0, 160.0 + 52.0 * f64::from(i), 400.0, 52.0),
                &format!("Note {i}"),
            )
        })
        .collect();
    let bar = |label: &str, f: Rect, inner: Rect| {
        UiNode::new(Role::Other("Other".into()), f)
            .with_label(label)
            .with_value("0%")
            .with_children(vec![UiNode::new(Role::Other("Other".into()), inner)])
    };
    rows.push(bar(
        "Vertical scroll bar, 3 pages",
        Rect::new(405.0, 400.0, 30.0, 86.0),
        Rect::new(432.0, 403.0, 3.0, 80.0),
    ));
    rows.push(bar(
        "Horizontal scroll bar, 1 page",
        Rect::new(20.0, 470.0, 188.0, 30.0),
        Rect::new(23.0, 497.0, 182.0, 3.0),
    ));
    let s = list_of(rows);
    for label in ["Note 5", "Note 6", "Note 4"] {
        let f = frame_of(&s, label);
        for frame in [Some(f), None] {
            let (t, _) = swipe(&s, frame);
            let t = t.unwrap_or_else(|e| panic!("{label} {frame:?}: {e}"));
            assert!(t <= ROW_SWIPE_SHARE * 400.0 + 1e-6, "{label}: {t}");
            assert!(t >= MIN_TRAVEL - 1e-6, "{label}: {t}");
        }
    }
    // The real one: Safari's select list (its 30-pt vertical bar beside 250-pt rows).
    let s = parse(fixture!(
        "ios26_probes3",
        "p6b-safari-inputs-01-select-tree.txt"
    ));
    for label in ["Alpha", "Beta", "Gamma"] {
        let (t, _) = swipe(&s, Some(frame_of(&s, label)));
        let t = t.unwrap_or_else(|e| panic!("{label}: {e}"));
        assert!(t <= ROW_SWIPE_SHARE * 250.0 + 1e-6, "{label}: {t}");
    }
}

/// The compact date picker under its popover's dismiss region: its days (`Button`s the
/// calendar's `CollectionView` holds directly: a control, so rows; squarish with alike days
/// beside them: grid cells) still get their 80-pt drag, as before.
#[test]
fn f2_the_compact_date_pickers_days_keep_their_drag() {
    let s = parse(fixture!(
        "ios26_probes3",
        "p4d-controls-10-compact-date-open-tree.txt"
    ));
    for day in [
        "Friday, January 1",
        "Thursday, January 7",
        "Sunday, January 31",
    ] {
        let (t, reach) = swipe(&s, Some(frame_of(&s, day)));
        let t = t.unwrap_or_else(|e| panic!("{day}: {e}"));
        assert!(
            (t - reach).abs() < 1e-6 && (t - MIN_TRAVEL).abs() < 1e-6,
            "{day}: {t} {reach}"
        );
    }
}

// ---------------------------------------------------------------------------------------
// The property: from 146 pt up, at most 0.55 of the narrowest crossed cell.
// ---------------------------------------------------------------------------------------

/// A small seeded generator (xorshift64*), so a failure replays.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in `lo..=hi` (whole points).
    fn range(&mut self, lo: u32, hi: u32) -> f64 {
        let span = u64::from(hi - lo + 1);
        f64::from(lo + u32::try_from(self.next() % span).unwrap())
    }
    fn pick(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(n).unwrap()).unwrap()
    }
}

/// Random lists: lines of 1 to 4 cells side by side (widths 60 to 440 pt, fitted to the
/// list; heights 30 to 200 pt; roles `Cell`, `Other` or `Button`; with or without text),
/// stacked from y = 120. Every planned sideways drag (element-less, or on a random cell;
/// swipes and scrolls both ways) that crosses a cell wider than 146 pt travels at most 0.55
/// of the narrowest such cell it crosses, and never more than its reach.
#[test]
fn property_no_drag_past_055_of_a_crossed_wide_cell() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut checked = 0usize;
    for case in 0..400 {
        let mut frames: Vec<Rect> = Vec::new();
        let mut rows = Vec::new();
        let mut y = 120.0;
        while y < 860.0 {
            let n = 1 + rng.pick(4);
            let h = rng.range(30, 200);
            let mut x = rng.range(0, 20);
            for c in 0..n {
                let room = 440.0 - x;
                if room < 60.0 {
                    break;
                }
                let w = rng.range(60, 440).min(room);
                let role = match rng.pick(3) {
                    0 => Role::Cell,
                    1 => Role::Other("Other".into()),
                    _ => Role::Button,
                };
                let text = rng.pick(2) == 0;
                let f = Rect::new(x, y, w, h);
                rows.push(row(role, f, &format!("L{y} c{c}"), text));
                frames.push(f);
                x += w + rng.range(0, 30);
            }
            y += h;
        }
        let s = list_of(rows);
        let v = compact_view(&s);
        for _ in 0..6 {
            let frame = if rng.pick(3) == 0 {
                None
            } else {
                Some(frames[rng.pick(frames.len())])
            };
            let (kind, direction) = SIDEWAYS[rng.pick(4)];
            let (p, r) = drag(&v, kind, direction, frame);
            let Ok(g) = p else { continue };
            let t = length(&g);
            assert!(t <= length(&r) + 1e-6, "case {case}");
            let (x0, x1) = (g.from.x.min(g.to.x), g.from.x.max(g.to.x));
            let (y0, y1) = (g.from.y.min(g.to.y), g.from.y.max(g.to.y));
            let narrowest = frames
                .iter()
                .filter(|f| f.x <= x1 && f.max_x() >= x0 && f.y <= y1 && f.max_y() >= y0)
                .map(|f| f.width)
                .filter(|&w| w > 146.0)
                .fold(f64::INFINITY, f64::min);
            if narrowest.is_finite() {
                checked += 1;
                assert!(
                    t <= INSET_ROW_MAX_SHARE * narrowest + 1e-6,
                    "case {case} {kind:?} {direction:?} {frame:?}: {t} > 0.55 x {narrowest}"
                );
            }
        }
    }
    assert!(checked > 500, "{checked}");
}

// ---------------------------------------------------------------------------------------
// The survey of real trees (pass 33's, widened): printed, so a change shows.
// ---------------------------------------------------------------------------------------

/// Every real tree with rows, grids or icons: the element-less swipe left and the swipe
/// left on every listed `Cell` and `Button`, with the decision and the planned travel
/// (`SURVEY|` lines). No planned drag is longer than its reach, every element-less swipe
/// is planned (none refused), and the Home Screen, App Library and folder icons (no list
/// cells) keep their reach.
#[test]
fn survey_of_sideways_swipes_on_real_trees() {
    let cases: Vec<(&str, &str)> = vec![
        (
            "p13 rows",
            fixture!("ios26_probes3", "p13-swipe-lengths-01-rows-tree.txt"),
        ),
        (
            "p13a full swipe",
            fixture!(
                "ios26_probes3",
                "p13a-swipe-lengths-01-rows-full-swipe-row-tree.txt"
            ),
        ),
        (
            "p13b narrow",
            fixture!(
                "ios26_probes3",
                "p13b-swipe-lengths-02-rows-narrow-row-tree.txt"
            ),
        ),
        (
            "p13b left",
            fixture!(
                "ios26_probes3",
                "p13b-swipe-lengths-05-rows-left-row-tree.txt"
            ),
        ),
        (
            "p13b leading",
            fixture!(
                "ios26_probes3",
                "p13b-swipe-lengths-08-rows-leading-row-tree.txt"
            ),
        ),
        (
            "p4d controls rows",
            fixture!(
                "ios26_probes3",
                "p4d-controls-02-controls-before-swipes-runner-tree.txt"
            ),
        ),
        (
            "compact date",
            fixture!(
                "ios26_probes3",
                "p4d-controls-10-compact-date-open-tree.txt"
            ),
        ),
        (
            "photo picker",
            fixture!("ios26_probes3", "p2-pickers-01-photo-picker-tree.txt"),
        ),
        (
            "contact picker",
            fixture!("ios26_probes3", "p2-pickers-05-contact-picker-tree.txt"),
        ),
        (
            "contacts",
            fixture!("ios26_probes3", "p5c-contacts-01-no-delete-tree.txt"),
        ),
        (
            "home screen",
            fixture!("ios26_probes3", "p15-springboard-01-home-tree.txt"),
        ),
        (
            "today view",
            fixture!("ios26_probes3", "p15-springboard-03-today-tree.txt"),
        ),
        (
            "app library",
            fixture!("ios26_probes3", "p15-springboard-05-app-library-tree.txt"),
        ),
        (
            "folder",
            fixture!("ios26_probes3", "p15-springboard-07-folder-tree.txt"),
        ),
        (
            "safari page",
            fixture!("ios26_probes3", "p6a-safari-page-01-page-tree.txt"),
        ),
        (
            "safari select",
            fixture!("ios26_probes3", "p6b-safari-inputs-01-select-tree.txt"),
        ),
        (
            "date input",
            fixture!("ios26_probes3", "p6b-safari-inputs-04-date-input-tree.txt"),
        ),
        ("share sheet", fixture!("ios26_probes", "share_sheet.txt")),
        ("edit menu", fixture!("ios26_probes", "edit_menu.txt")),
        (
            "edit menu list",
            fixture!("ios26_probes", "edit_menu_list.txt"),
        ),
        ("safari favourites", fixture!("safari_form", "02.txt")),
        (
            "settings root",
            include_str!("../../jarvis-observe/tests/fixtures/ios26/settings_root.txt"),
        ),
        (
            "long list",
            include_str!("../../jarvis-observe/tests/fixtures/ios26/long_list_scrolled.txt"),
        ),
        (
            "safari start page",
            include_str!("../../jarvis-observe/tests/fixtures/ios26/safari_start_page.txt"),
        ),
    ];
    for (name, text) in cases {
        let s = parse(text);
        let v = compact_view(&s);
        let mut targets: Vec<(Option<u32>, Option<Rect>, String)> =
            vec![(None, None, "element-less".into())];
        for e in &v.screen.elements {
            if e.role == Role::Cell || e.role == Role::Button {
                targets.push((Some(e.index), Some(e.frame), format!("{:?}", e.label)));
            }
        }
        for (el, frame, what) in targets {
            let a = Action::Swipe {
                direction: Direction::Left,
                element: el,
            };
            let d = decide(&a, &s);
            let Ok(reach) = drag_gesture(GestureKind::Swipe, Direction::Left, frame, &v.screen)
            else {
                println!("SURVEY|{name}|{what}|{}|no reach", describe(&d));
                continue;
            };
            let (plan, _) = drag(&v, GestureKind::Swipe, Direction::Left, frame);
            let plan = plan.map(|g| length(&g));
            if let Ok(p) = plan {
                assert!(p <= length(&reach) + 1e-6, "{name} {what}");
            }
            if el.is_none() {
                assert!(plan.is_ok(), "{name}: the element-less swipe is refused");
            }
            if matches!(name, "home screen" | "app library" | "folder") {
                if let Ok(p) = plan {
                    assert!((p - length(&reach)).abs() < 1e-6, "{name} {what}: {p}");
                }
            }
            println!(
                "SURVEY|{name}|{what}|{}|{:?}|{:.1}",
                describe(&d),
                plan.map(|t| (t * 10.0).round() / 10.0),
                length(&reach)
            );
        }
    }
}
