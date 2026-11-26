#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Mac run 161 (iPhone 17 Pro Max simulator, iOS 26.5, SwiftUI `List` with `.swipeActions`,
//! probes P13e to P13g): a row whose swipe actions are already showing is not swiped
//! further. After a first drag of 0.30 of the row revealed the actions, a second drag the
//! same way deleted the row at 0.45 of it (one trailing action), 0.30 (two) and 0.40 (one
//! leading) — inside the bound measured from rest (`planned_drag`). While revealed, the
//! row's `Cell` keeps its size and is shifted out of its list (107 pt, 205 pt, +116 pt), and
//! the action buttons sit in a sibling `Other` at the row's rest frame.
//!
//! Every test here is driven by the real trees (`tests/fixtures/ios26_probes3/p13f-*`,
//! `p13g-*`, run 139's `p4d-controls-03`), the XCUITest snapshot and the typed tree
//! Jarvis's own runner produced; the synthetic screens of the property test follow their
//! shapes.

mod common;

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use common::*;
use jarvis_observe::{compact_view, CompactView};
use jarvis_proto::{Action, Direction, Point, Rect, RiskCategory, Role, Screen, Size, UiNode};
use jarvis_safety::gestures::{
    drag_gesture, finger_direction, Gesture, GestureError, GestureKind, ROW_SWIPE_SHARE,
};
use jarvis_safety::{
    evaluate_owned, home_view, planned_drag, planned_path, tap_spot, Decision, ScreenSource,
    ScreenTree, ROW_ACTIONS_NOTE, ROW_SWIPE_NOTE,
};

// ---------------------------------------------------------------------------------------
// The real trees.
// ---------------------------------------------------------------------------------------

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("fixtures/ios26_probes3/", $name))
    };
}

const INSET_ONE_REST: &str = fixture!("p13f-revealed-tree-01-rows-inset-one-row-tree.txt");
const INSET_ONE: &str = fixture!("p13f-revealed-tree-02-revealed-inset-one-row-tree.txt");
const INSET_ONE_RUNNER: &str =
    fixture!("p13f-revealed-tree-03-revealed-inset-one-row-runner-tree.txt");
const EDGE_ONE_REST: &str = fixture!("p13f-revealed-tree-04-rows-edge-one-row-tree.txt");
const EDGE_ONE: &str = fixture!("p13f-revealed-tree-05-revealed-edge-one-row-tree.txt");
const EDGE_ONE_RUNNER: &str =
    fixture!("p13f-revealed-tree-06-revealed-edge-one-row-runner-tree.txt");
const INSET_TWO_REST: &str = fixture!("p13f-revealed-tree-07-rows-inset-two-row-tree.txt");
const INSET_TWO: &str = fixture!("p13f-revealed-tree-08-revealed-inset-two-row-tree.txt");
const INSET_TWO_RUNNER: &str =
    fixture!("p13f-revealed-tree-09-revealed-inset-two-row-runner-tree.txt");
const LEADING_REST: &str = fixture!("p13f-revealed-tree-10-rows-leading-one-row-tree.txt");
const LEADING: &str = fixture!("p13f-revealed-tree-11-revealed-leading-one-row-tree.txt");
const LEADING_RUNNER: &str =
    fixture!("p13f-revealed-tree-12-revealed-leading-one-row-runner-tree.txt");
const TAP_REST: &str = fixture!("p13g-revealed-tap-01-rows-tap-row-tree.txt");
const TAP_REVEALED: &str = fixture!("p13g-revealed-tap-02-revealed-tap-row-1-tree.txt");
const TAP_CLOSED: &str = fixture!("p13g-revealed-tap-03-after-tap-1-tree.txt");
const P4D_REST: &str = fixture!("p4d-controls-02-controls-before-swipes-runner-tree.txt");
const P4D_REVEALED: &str = fixture!("p4d-controls-03-row1-after-runner-swipe-tree.txt");
// The first reveals of the row-length probes kept with run 161 (P13b to P13d), and the
// same lists at rest (runs 149 and 161).
const NARROW_REST: &str = fixture!("p13b-swipe-lengths-02-rows-narrow-row-tree.txt");
const NARROW: &str = fixture!("p13b-swipe-narrow-02-revealed-1-tree.txt");
const COLUMNS_REST: &str = fixture!("p13b-swipe-lengths-05-rows-left-row-tree.txt");
const COLUMNS: &str = fixture!("p13c-swipe-columns-02-revealed-1-tree.txt");
const LEADING_TWO_REST: &str = fixture!("p13b-swipe-lengths-08-rows-leading-row-tree.txt");
const LEADING_TWO: &str = fixture!("p13d-swipe-leading-02-revealed-1-tree.txt");
const PLAIN_REST: &str = fixture!("p13d-swipe-leading-04-rows-plain-row-tree.txt");
const PLAIN: &str = fixture!("p13d-swipe-leading-05-revealed-1-tree.txt");

/// One revealed kind: its revealed captures, its capture at rest, its row's text and the
/// way the row was pulled (the finger direction that would pull it further).
struct Kind {
    name: &'static str,
    revealed: &'static [&'static str],
    rest: &'static str,
    row: &'static str,
    pulled: Direction,
}

const KINDS: [Kind; 10] = [
    Kind {
        name: "a 300-pt row of a narrow list, two trailing actions",
        revealed: &[NARROW],
        rest: NARROW_REST,
        row: "Narrow row 1",
        pulled: Direction::Left,
    },
    Kind {
        name: "the left of two 200-pt columns (two lists), two trailing actions",
        revealed: &[COLUMNS],
        rest: COLUMNS_REST,
        row: "Left row 1",
        pulled: Direction::Left,
    },
    Kind {
        name: "two leading actions (440-pt rows)",
        revealed: &[LEADING_TWO],
        rest: LEADING_TWO_REST,
        row: "Leading row 1",
        pulled: Direction::Right,
    },
    Kind {
        name: "edge to edge, two trailing actions (440-pt rows)",
        revealed: &[PLAIN],
        rest: PLAIN_REST,
        row: "Plain row 1",
        pulled: Direction::Left,
    },
    Kind {
        name: "inset, one trailing action (400-pt rows)",
        revealed: &[INSET_ONE, INSET_ONE_RUNNER],
        rest: INSET_ONE_REST,
        row: "Inset one row 1",
        pulled: Direction::Left,
    },
    Kind {
        name: "edge to edge, one trailing action (440-pt rows)",
        revealed: &[EDGE_ONE, EDGE_ONE_RUNNER],
        rest: EDGE_ONE_REST,
        row: "Edge one row 1",
        pulled: Direction::Left,
    },
    Kind {
        name: "inset, two trailing actions",
        revealed: &[INSET_TWO, INSET_TWO_RUNNER],
        rest: INSET_TWO_REST,
        row: "Inset two row 1",
        pulled: Direction::Left,
    },
    Kind {
        name: "one leading action (440-pt rows)",
        revealed: &[LEADING, LEADING_RUNNER],
        rest: LEADING_REST,
        row: "Leading one row 1",
        pulled: Direction::Right,
    },
    Kind {
        name: "button rows, two trailing actions",
        revealed: &[TAP_REVEALED],
        rest: TAP_REST,
        row: "Tap row 1",
        pulled: Direction::Left,
    },
    Kind {
        name: "run 139's controls list, two trailing actions",
        revealed: &[P4D_REVEALED],
        rest: P4D_REST,
        row: "Swipe row 1",
        pulled: Direction::Left,
    },
];

/// The four sideways drags.
const SIDEWAYS: [(GestureKind, Direction); 4] = [
    (GestureKind::Swipe, Direction::Left),
    (GestureKind::Swipe, Direction::Right),
    (GestureKind::Scroll, Direction::Left),
    (GestureKind::Scroll, Direction::Right),
];

// ---------------------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------------------

/// What `ios` (the observe tests' parser) uses to print a compact screen.
fn compact_text(c: &jarvis_proto::CompactScreen) -> String {
    format!("{c:?}")
}

/// The typed tree of a capture (the core's input); the raw snapshot after it is cut off.
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

fn opposite(d: Direction) -> Direction {
    match d {
        Direction::Left => Direction::Right,
        Direction::Right => Direction::Left,
        Direction::Up => Direction::Down,
        Direction::Down => Direction::Up,
    }
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

fn element_index(v: &CompactView, label: &str) -> u32 {
    v.screen
        .elements
        .iter()
        .find(|e| e.label == label || e.label.starts_with(&format!("{label},")))
        .unwrap_or_else(|| panic!("no element {label:?}"))
        .index
}

/// The rows (`y` extent) of the row whose text is `label`: the listed element's frame.
fn band(v: &CompactView, label: &str) -> (f64, f64) {
    let e = v.screen.get(element_index(v, label)).unwrap();
    (e.frame.y, e.frame.max_y())
}

/// The listed elements whose centre lies in the rows `y0..=y1`.
fn in_band(v: &CompactView, (y0, y1): (f64, f64)) -> Vec<(u32, Rect, String)> {
    v.screen
        .elements
        .iter()
        .filter(|e| {
            let c = e.frame.center().y;
            c >= y0 && c <= y1
        })
        .map(|e| (e.index, e.frame, e.label.clone()))
        .collect()
}

fn edit(n: &mut UiNode, f: &mut impl FnMut(&mut UiNode)) {
    f(n);
    for c in &mut n.children {
        edit(c, f);
    }
}

/// The frame of the list (`CollectionView`) holding the point `at`.
fn list_at(s: &Screen, at: Point) -> Rect {
    s.root
        .find(|n| n.role == Role::CollectionView && n.frame.contains(at))
        .expect("no list")
        .frame
}

/// `s` with everything the list holding `at` holds moved down by `dy` (the same list
/// scrolled), so a row's band lies where an element-less drag runs.
fn scrolled(s: &Screen, at: Point, dy: f64) -> Screen {
    fn go(n: &mut UiNode, at: Point, dy: f64) -> bool {
        if n.role == Role::CollectionView && n.frame.contains(at) {
            for c in &mut n.children {
                edit(c, &mut |m| m.frame.y += dy);
            }
            return true;
        }
        n.children.iter_mut().any(|c| go(c, at, dy))
    }
    let mut s = s.clone();
    assert!(go(&mut s.root, at, dy), "no list");
    s
}

/// A horizontal path at `y` from `x` travelling `len` with the finger moving `finger`.
fn path(x: f64, y: f64, len: f64, finger: Direction) -> Gesture {
    let to = match finger {
        Direction::Left => x - len,
        _ => x + len,
    };
    Gesture {
        from: Point::new(x, y),
        to: Point::new(to, y),
        duration_ms: 200,
    }
}

/// A closing drag that was not planned: only ever the existing narrow-row refusal, and only
/// on the two-column capture, where 36 pt of the shifted 200-pt cell is left on screen (the
/// inset-row rule reads the drawn part: under 145.5 pt, refused — stricter, never longer).
fn closing_refused(k: &Kind, e: GestureError, what: &str) {
    assert!(
        e == GestureError::RowTooNarrow && k.row == "Left row 1",
        "{}: closing {what}: {e:?}",
        k.name
    );
}

fn summary(d: &Decision) -> String {
    match d {
        Decision::Confirm(q) => q.summary.clone(),
        other => format!("{other:?}"),
    }
}

// ---------------------------------------------------------------------------------------
// 1. The same-way drag is refused, at every length, by element, element-less and scroll.
// ---------------------------------------------------------------------------------------

/// On every revealed capture, every horizontal path through the row's band (its rows
/// across the whole list, from 10 pt up to 400 pt, starting anywhere from the left edge
/// to the right edge — over the shifted cell, the vacated part, the buttons, or only
/// passing through) whose finger moves the way the row was pulled is refused with
/// `RowActionsShowing`; the other way it is planned (never refused for that), never longer
/// than asked and never longer than 0.45 of the row.
#[test]
fn every_revealed_kind_refuses_the_same_way_drag_at_every_length() {
    for k in &KINDS {
        for text in k.revealed {
            let s = parse(text);
            let v = compact_view(&s);
            let t = tree(&v);
            let (y0, y1) = band(&v, k.row);
            let row = v.screen.get(element_index(&v, k.row)).unwrap().frame;
            let list = list_at(&s, row.center());
            // The band across: the list and the part of the shifted row on screen.
            let (bx0, bx1) = (list.x.min(row.x), list.max_x().max(row.max_x()));
            let width = v.screen.size.width;
            for y in [y0 + 0.5, (y0 + y1) / 2.0, y1 - 0.5] {
                let mut x = 0.0;
                while x <= width {
                    let mut len = 10.0;
                    while len <= 400.0 {
                        let g = path(x, y, len, k.pulled);
                        let (px0, px1) = (g.from.x.min(g.to.x), g.from.x.max(g.to.x));
                        if px0 <= bx1 && px1 >= bx0 {
                            assert_eq!(
                                planned_path(g, k.pulled, &v.screen, Some(&t)),
                                Err(GestureError::RowActionsShowing),
                                "{}: from {x} at {y}, {len} pt",
                                k.name
                            );
                        }
                        // The other way, from inside the list and the safe area (where
                        // the planned drags start): planned and bounded by the row.
                        let back = opposite(k.pulled);
                        if !(17.6..=422.4).contains(&x) || x < list.x || x > list.max_x() {
                            len += 10.0;
                            continue;
                        }
                        let g = path(x, y, len, back);
                        match planned_path(g, back, &v.screen, Some(&t)) {
                            Ok(p) => {
                                assert!(length(&p) <= len + 1e-6, "{}", k.name);
                                assert!(
                                    length(&p) <= ROW_SWIPE_SHARE * 440.0 + 1e-6,
                                    "{}: from {x}, {len} pt: {}",
                                    k.name,
                                    length(&p)
                                );
                            }
                            Err(e) => closing_refused(k, e, &format!("from {x}, {len} pt")),
                        }
                        len += 10.0;
                    }
                    x += 20.0;
                }
            }
        }
    }
}

/// The drags the session plans: a swipe and a sideways scroll on every listed element in
/// the row's band (the row's text, every revealed action), and the element-less ones with
/// the list scrolled so the band lies on their path: the way the row was pulled, refused;
/// the other way, planned and bounded. Unscrolled, the element-less drag does not cross the
/// band and is planned as before.
#[test]
fn by_element_element_less_and_scroll() {
    for k in &KINDS {
        for text in k.revealed {
            let s = parse(text);
            let v = compact_view(&s);
            let t = tree(&v);
            let rows = band(&v, k.row);
            // In the band of this row's list (the other column's list has its own).
            let list = list_at(
                &s,
                v.screen
                    .get(element_index(&v, k.row))
                    .unwrap()
                    .frame
                    .center(),
            );
            let elements: Vec<_> = in_band(&v, rows)
                .into_iter()
                .filter(|(_, f, _)| f.center().x >= list.x && f.center().x <= list.max_x())
                .collect();
            assert!(elements.len() >= 2, "{}: {elements:?}", k.name);
            for (_, frame, label) in &elements {
                for (kind, d) in SIDEWAYS {
                    let got = planned_drag(kind, d, Some(*frame), &v.screen, Some(&t));
                    if finger_direction(kind, d) == k.pulled {
                        assert_eq!(
                            got,
                            Err(GestureError::RowActionsShowing),
                            "{}: {label} {kind:?} {d:?}",
                            k.name
                        );
                    } else {
                        match got {
                            Ok(g) => assert!(length(&g) <= ROW_SWIPE_SHARE * 440.0 + 1e-6),
                            Err(e) => closing_refused(k, e, label),
                        }
                    }
                }
            }
            // Element-less: unscrolled, the drag runs below the row.
            for (kind, d) in SIDEWAYS {
                assert_ne!(
                    planned_drag(kind, d, None, &v.screen, Some(&t)),
                    Err(GestureError::RowActionsShowing),
                    "{}",
                    k.name
                );
            }
            // Scrolled so the band lies on the element-less path.
            let reach = drag_gesture(GestureKind::Swipe, Direction::Left, None, &v.screen).unwrap();
            let dy = reach.from.y - (rows.0 + rows.1) / 2.0;
            let at = v
                .screen
                .get(element_index(&v, k.row))
                .unwrap()
                .frame
                .center();
            let s2 = scrolled(&s, at, dy);
            let v2 = compact_view(&s2);
            let t2 = tree(&v2);
            for (kind, d) in SIDEWAYS {
                let got = planned_drag(kind, d, None, &v2.screen, Some(&t2));
                if finger_direction(kind, d) == k.pulled {
                    assert_eq!(
                        got,
                        Err(GestureError::RowActionsShowing),
                        "{}: element-less {kind:?} {d:?}",
                        k.name
                    );
                } else {
                    match got {
                        Ok(g) => assert!(length(&g) <= ROW_SWIPE_SHARE * 440.0 + 1e-6),
                        Err(e) => closing_refused(k, e, "element-less"),
                    }
                }
            }
            // A vertical scroll is as before (no refusal, its reach).
            for d in [Direction::Up, Direction::Down] {
                let got = planned_drag(GestureKind::Scroll, d, None, &v2.screen, Some(&t2));
                assert_eq!(
                    got,
                    drag_gesture(GestureKind::Scroll, d, None, &v2.screen),
                    "{}",
                    k.name
                );
            }
        }
    }
}

/// The closing drag is asked as any row swipe is (`Delete`, phone only), and its question
/// says the row's actions are showing — never the at-rest note, whose shares were measured
/// from a row at rest. The same row at rest keeps the at-rest note.
#[test]
fn the_closing_drag_asks_and_says_the_actions_are_showing() {
    for k in &KINDS {
        for text in k.revealed {
            let s = parse(text);
            let v = compact_view(&s);
            let el = element_index(&v, k.row);
            for (kind, d) in SIDEWAYS {
                if finger_direction(kind, d) == k.pulled {
                    continue;
                }
                let a = match kind {
                    GestureKind::Swipe => Action::Swipe {
                        direction: d,
                        element: Some(el),
                    },
                    GestureKind::Scroll => Action::Scroll {
                        direction: d,
                        element: Some(el),
                    },
                };
                let Decision::Confirm(q) = decide(&a, &s) else {
                    panic!("{}: not asked", k.name)
                };
                assert_eq!(q.category, RiskCategory::Delete, "{}", k.name);
                assert!(q.phone_only, "{}", k.name);
                assert!(
                    q.summary.contains(ROW_ACTIONS_NOTE),
                    "{}: {}",
                    k.name,
                    q.summary
                );
                assert!(
                    !q.summary.contains(ROW_SWIPE_NOTE),
                    "{}: {}",
                    k.name,
                    q.summary
                );
                assert!(!q.summary.contains("at rest"), "{}", q.summary);
            }
        }
        let s = parse(k.rest);
        let v = compact_view(&s);
        let a = Action::Swipe {
            direction: opposite(k.pulled),
            element: Some(element_index(&v, k.row)),
        };
        let d = decide(&a, &s);
        assert!(
            summary(&d).contains(ROW_SWIPE_NOTE),
            "{}: {}",
            k.name,
            summary(&d)
        );
        assert!(!summary(&d).contains(ROW_ACTIONS_NOTE), "{}", k.name);
    }
}

/// Worst case of the closing direction in numbers: the drag Jarvis plans the closing way
/// on each revealed row, against the row's measured shift. What it can carry the row past
/// "closed" (`travel - shift`, the row starts displaced against the drag) stays under 0.22
/// of the row — under the 0.30 that only revealed, far under the 0.60 that deleted from
/// rest.
#[test]
fn the_closing_drag_cannot_reach_a_full_swipe_on_the_other_side() {
    // (capture, row, the shift measured by P13f, the row's width)
    let cases = [
        (INSET_ONE, "Inset one row 1", 107.3, 400.0),
        (EDGE_ONE, "Edge one row 1", 107.3, 440.0),
        (INSET_TWO, "Inset two row 1", 204.7, 400.0),
        (LEADING, "Leading one row 1", 116.3, 440.0),
        (TAP_REVEALED, "Tap row 1", 204.7, 400.0),
    ];
    for (text, row, shift, width) in cases {
        let s = parse(text);
        let v = compact_view(&s);
        let frame = v.screen.get(element_index(&v, row)).unwrap().frame;
        let pulled_left = row != "Leading one row 1";
        let close = if pulled_left {
            Direction::Right
        } else {
            Direction::Left
        };
        let mut worst = f64::NEG_INFINITY;
        // Element-less: the list scrolled so the band lies on the path.
        let reach = drag_gesture(GestureKind::Swipe, close, None, &v.screen).unwrap();
        let sv = compact_view(&scrolled(
            &s,
            frame.center(),
            reach.from.y - frame.center().y,
        ));
        for (target, cv) in [(Some(frame), &v), (None, &sv)] {
            let tv = tree(cv);
            for kind in [GestureKind::Swipe, GestureKind::Scroll] {
                let d = if kind == GestureKind::Swipe {
                    close
                } else {
                    opposite(close)
                };
                let g = planned_drag(kind, d, target, &cv.screen, Some(&tv)).unwrap();
                let past = length(&g) - shift;
                worst = worst.max(past);
                println!(
                    "CLOSING|{row}|{kind:?} {d:?} {}|travel {:.1}|past closed {:.1} ({:.2} of the row)",
                    if target.is_some() { "on the row" } else { "element-less" },
                    length(&g),
                    past,
                    past / width
                );
            }
        }
        assert!(worst / width < 0.22, "{row}: {worst}");
        assert!(
            worst <= ROW_SWIPE_SHARE * width - shift + 1e-6,
            "{row}: {worst}"
        );
    }
}

// ---------------------------------------------------------------------------------------
// 2. Taps on the revealed actions are ordinary taps.
// ---------------------------------------------------------------------------------------

/// A tap on the revealed "Delete" (`id trash`, outside the row's `Cell`) asks as `Delete`,
/// on the leading "Remove" (`id trash`) too, on "Flag" it does not; the tap point Jarvis
/// plans lies on the button itself (not in the shifted cell, not refused as covered). A
/// tap on the shifted row's content (it closes the row, P13g) is an ordinary tap on that
/// content, its point on the part of the row still shown, on no action button.
#[test]
fn taps_on_the_revealed_actions_are_judged_by_label_and_land_on_them() {
    let tap = |v: &CompactView, label: &str| Action::Tap {
        element: element_index(v, label),
    };
    for text in [
        INSET_ONE,
        INSET_ONE_RUNNER,
        EDGE_ONE,
        EDGE_ONE_RUNNER,
        INSET_TWO,
        INSET_TWO_RUNNER,
        TAP_REVEALED,
        P4D_REVEALED,
    ] {
        let s = parse(text);
        let v = compact_view(&s);
        let t = tree(&v);
        let d = decide(&tap(&v, "Delete"), &s);
        let Decision::Confirm(q) = &d else {
            panic!("Delete not asked: {d:?}")
        };
        assert_eq!(q.category, RiskCategory::Delete);
        let del = v.screen.get(element_index(&v, "Delete")).unwrap();
        let p = tap_spot(del, &v.screen, Some(&t), None).unwrap();
        assert!(del.frame.contains(p), "{p:?} {:?}", del.frame);
        if text.contains("\"Flag\"") {
            assert!(matches!(decide(&tap(&v, "Flag"), &s), Decision::Allow));
        }
    }
    for text in [LEADING, LEADING_RUNNER] {
        let s = parse(text);
        let v = compact_view(&s);
        let t = tree(&v);
        let d = decide(&tap(&v, "Remove"), &s);
        let Decision::Confirm(q) = &d else {
            panic!("Remove not asked: {d:?}")
        };
        assert_eq!(q.category, RiskCategory::Delete);
        let b = v.screen.get(element_index(&v, "Remove")).unwrap();
        // Its identifier `trash` names the Delete symbol (the label alone would too).
        assert!(v.nodes.iter().any(|n| n.label.as_deref() == Some("Remove")
            && n.role == Role::Button
            && n.symbol == Some(RiskCategory::Delete)));
        let p = tap_spot(b, &v.screen, Some(&t), None).unwrap();
        assert!(b.frame.contains(p));
    }
    // The row's content: an ordinary tap, on the part of the row shown, on no button.
    for (text, row) in [
        (TAP_REVEALED, "Tap row 1"),
        (INSET_ONE, "Inset one row 1"),
        (LEADING, "Leading one row 1"),
    ] {
        let s = parse(text);
        let v = compact_view(&s);
        let t = tree(&v);
        assert_eq!(decide(&tap(&v, row), &s), Decision::Allow, "{row}");
        let e = v.screen.get(element_index(&v, row)).unwrap();
        let p = tap_spot(e, &v.screen, Some(&t), None).unwrap();
        assert!(
            e.frame.contains(p) && p.x >= 0.0 && p.x <= 440.0,
            "{row}: {p:?}"
        );
        for b in &v.screen.elements {
            if b.role == Role::Button && b.label != row {
                assert!(!b.frame.contains(p), "{row}: {p:?} on {}", b.label);
            }
        }
    }
    // Closed by the tap (P13g 03): at rest again, nothing reads as revealed.
    assert_no_revealed("p13g closed", &parse(TAP_CLOSED));
}

// ---------------------------------------------------------------------------------------
// 3. At rest: exactly as before.
// ---------------------------------------------------------------------------------------

/// Every horizontal path across the screen at every 2 pt of height, both ways, is never
/// refused as a revealed row; panics naming the first one that is.
fn assert_no_revealed(name: &str, s: &Screen) {
    if let Some(y) = revealed_at(s).first() {
        panic!("{name}: reads as revealed at y {y}");
    }
}

/// The heights (every 2 pt) at which a full-width path reads as crossing a revealed row.
fn revealed_at(s: &Screen) -> Vec<f64> {
    let s = home_view(s).unwrap_or_else(|| s.clone());
    let v = compact_view(&s);
    let t = tree(&v);
    let size = v.screen.size;
    let mut out = Vec::new();
    let mut y = 0.0;
    while y <= size.height {
        for finger in [Direction::Left, Direction::Right] {
            let (a, b) = (size.width * 0.04, size.width * 0.96);
            let g = match finger {
                Direction::Left => path(b, y, b - a, finger),
                _ => path(a, y, b - a, finger),
            };
            if planned_path(g, finger, &v.screen, Some(&t)) == Err(GestureError::RowActionsShowing)
            {
                out.push(y);
                break;
            }
        }
        y += 2.0;
    }
    out
}

/// The same rows at rest: no path anywhere reads as revealed, and every drag the session
/// plans (by element on every listed element, element-less, swipe and scroll) is planned
/// exactly as `planned_path` of its reach (the bound alone).
#[test]
fn the_same_rows_at_rest_behave_as_before() {
    for k in &KINDS {
        let s = parse(k.rest);
        assert_no_revealed(k.name, &s);
        let v = compact_view(&s);
        let t = tree(&v);
        let frames = std::iter::once(None).chain(v.screen.elements.iter().map(|e| Some(e.frame)));
        for frame in frames {
            for (kind, d) in SIDEWAYS {
                let got = planned_drag(kind, d, frame, &v.screen, Some(&t));
                assert_ne!(got, Err(GestureError::RowActionsShowing), "{}", k.name);
            }
        }
        // A swipe the way the row would be pulled is bounded at 0.45 of the 400/440-pt row.
        let el = v.screen.get(element_index(&v, k.row)).unwrap();
        let g = planned_drag(
            GestureKind::Swipe,
            k.pulled,
            Some(el.frame),
            &v.screen,
            Some(&t),
        )
        .unwrap();
        assert!(
            length(&g) <= ROW_SWIPE_SHARE * el.frame.width + 1e-6,
            "{}",
            k.name
        );
    }
}

// ---------------------------------------------------------------------------------------
// 4. A property test over shifts, widths, insets and directions (fixed seed).
// ---------------------------------------------------------------------------------------

/// xorshift64*, fixed seed: reproducible.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * ((self.next() >> 11) as f64 / (1u64 << 53) as f64)
    }
    fn pick<T: Copy>(&mut self, v: &[T]) -> T {
        v[(self.next() % v.len() as u64) as usize]
    }
}

fn phone(width: f64, list: UiNode) -> Screen {
    let full = Rect::new(0.0, 0.0, width, 956.0);
    let root = UiNode::new(Role::Application, full)
        .with_label("Notes")
        .with_children(vec![
            UiNode::new(Role::Window, full).with_children(vec![list])
        ]);
    let mut s = jarvis_proto::fixtures::screen("com.example.notes", "Notes", root, false);
    s.size = Size::new(width, 956.0);
    s
}

fn row_cell(frame: Rect, label: &str) -> UiNode {
    UiNode::new(Role::Cell, frame).with_children(vec![UiNode::new(
        Role::Other("Other".into()),
        frame,
    )
    .with_children(vec![UiNode::new(Role::StaticText, frame).with_label(label)])])
}

/// The sibling action container P13f measured: `Other > Other > Other > Button` at the
/// row's rest frame, the buttons at its trailing (or leading) side.
fn actions(rest: Rect, n: usize, leading: bool) -> UiNode {
    let mut buttons = Vec::new();
    for i in 0..n {
        let x = if leading {
            rest.x + 10.0 + 98.0 * i as f64
        } else {
            rest.max_x() - 10.0 - 87.0 - 98.0 * i as f64
        };
        let f = Rect::new(x, rest.y + 4.0, 87.0, rest.height - 8.0);
        let (label, id) = if i == 0 {
            ("Delete", "trash")
        } else {
            ("Flag", "flag")
        };
        buttons.push(
            UiNode::new(Role::Other("Other".into()), f).with_children(vec![UiNode::new(
                Role::Button,
                f,
            )
            .with_label(label)
            .with_identifier(id)]),
        );
    }
    UiNode::new(Role::Other("Other".into()), rest).with_children(vec![UiNode::new(
        Role::Other("Other".into()),
        rest,
    )
    .with_children(buttons)])
}

/// Random lists of rows (widths 320 to 440 pt, insets 0 to 24 pt), one of them shifted
/// by 10 to 260 pt either way, with or without its action container, and random paths
/// through its band: the way it was pulled is refused exactly when the cell reaches past
/// its list by more than the tolerance or the action container is there; the other way is
/// never refused for that and never longer than 0.45 of the row (nor the asked length).
/// The same list with no row shifted (at rest) reads as revealed nowhere.
#[test]
fn property_shifted_rows_refuse_the_way_they_were_pulled() {
    let mut rng = Rng(0x5eed_1612_6105_0001);
    let mut refused = 0usize;
    for case in 0..600 {
        let width = rng.pick(&[320.0, 375.0, 393.0, 402.0, 430.0, 440.0]);
        let inset = rng.pick(&[0.0, 0.0, 16.0, 20.0, 24.0]);
        let row_w = width - 2.0 * inset;
        let h = rng.pick(&[44.0, 52.0, 61.0]);
        let top = 200.0;
        let n_rows = 8;
        let which = (rng.next() % n_rows as u64) as usize;
        let shift_mag = rng.range(10.0, 260.0);
        let pulled = rng.pick(&[Direction::Left, Direction::Right]);
        let container = !rng.next().is_multiple_of(3);
        let n_actions = 1 + (rng.next() % 2) as usize;
        let shift = if pulled == Direction::Left {
            -shift_mag
        } else {
            shift_mag
        };
        let build = |shifted: bool| {
            let mut kids = Vec::new();
            for i in 0..n_rows {
                let rest = Rect::new(inset, top + h * i as f64, row_w, h);
                if shifted && i == which {
                    let f = Rect::new(inset + shift, rest.y, row_w, h);
                    kids.push(row_cell(f, &format!("Row {i}")));
                    if container {
                        kids.push(actions(rest, n_actions, pulled == Direction::Right));
                    }
                } else {
                    kids.push(row_cell(rest, &format!("Row {i}")));
                }
            }
            phone(
                width,
                UiNode::new(Role::CollectionView, Rect::new(0.0, 150.0, width, 806.0))
                    .with_children(kids),
            )
        };
        let rest = build(false);
        if case % 20 == 0 {
            assert_no_revealed("synthetic at rest", &rest);
        }
        let s = build(true);
        let v = compact_view(&s);
        let t = tree(&v);
        let sticks_out = if pulled == Direction::Left {
            inset + shift < -4.0
        } else {
            inset + shift + row_w > width + 4.0
        };
        let expect = sticks_out || container;
        let y = top + h * which as f64 + rng.range(0.5, h - 0.5);
        for _ in 0..8 {
            let len = rng.range(10.0, 400.0);
            let x = rng.range(0.0, width);
            let g = path(x, y, len, pulled);
            let got = planned_path(g, pulled, &v.screen, Some(&t));
            if expect {
                refused += 1;
                assert_eq!(
                    got,
                    Err(GestureError::RowActionsShowing),
                    "case {case}: w {width} inset {inset} shift {shift} container {container}"
                );
            } else {
                assert_ne!(
                    got,
                    Err(GestureError::RowActionsShowing),
                    "case {case}: w {width} inset {inset} shift {shift} container {container} row {which} y {y}"
                );
            }
            let back = opposite(pulled);
            let g = path(x, y, len, back);
            // What of the row the path can touch: the shifted cell's part on screen, and
            // the container at the rest frame.
            let (px0, px1) = (g.from.x.min(g.to.x), g.from.x.max(g.to.x));
            let (cx0, cx1) = ((inset + shift).max(0.0), (inset + shift + row_w).min(width));
            let touches =
                (px0 <= cx1 && px1 >= cx0) || (container && px0 <= inset + row_w && px1 >= inset);
            match planned_path(g, back, &v.screen, Some(&t)) {
                Ok(p) => {
                    assert!(length(&p) <= len + 1e-6);
                    if touches {
                        // Bounded at 0.45 of what it crosses (the cell's part on screen
                        // may be narrower: tighter), raised to 80 pt at most.
                        assert!(
                            length(&p) <= (ROW_SWIPE_SHARE * row_w).max(80.0) + 1e-6,
                            "case {case}: {} on a {row_w}-pt row",
                            length(&p)
                        );
                    }
                }
                Err(e) => {
                    assert_ne!(e, GestureError::RowActionsShowing, "case {case}");
                }
            }
        }
    }
    assert!(refused > 1000, "{refused}");
}

/// Carousels and pagers: cells of one list side by side, scrolled by any offset (so edge
/// cells reach past the list on either side), never read as revealed — the neighbour
/// fills the band an edge cell would have vacated.
#[test]
fn property_carousels_and_pagers_never_read_as_revealed() {
    let mut rng = Rng(0x5eed_1612_6105_0002);
    for case in 0..400 {
        let width = rng.pick(&[375.0, 393.0, 440.0]);
        let pager = rng.next().is_multiple_of(2);
        // Carousels up to near-full-width cards (a peeking pager), spacing up to 24 pt.
        let (cell_w, gap) = if pager {
            (width, 0.0)
        } else {
            (rng.range(80.0, width - 24.0), rng.range(0.0, 24.0))
        };
        let h = if pager { 300.0 } else { rng.range(60.0, 220.0) };
        let offset = rng.range(0.0, 3.0 * (cell_w + gap));
        let lead = if pager { 0.0 } else { 16.0 };
        let mut kids = Vec::new();
        for i in 0..12 {
            let x = lead + (cell_w + gap) * i as f64 - offset;
            kids.push(row_cell(
                Rect::new(x, 300.0, cell_w, h),
                &format!("Card {i}"),
            ));
        }
        let s = phone(
            width,
            UiNode::new(Role::CollectionView, Rect::new(0.0, 300.0, width, h)).with_children(kids),
        );
        let at = revealed_at(&s);
        assert!(
            at.is_empty(),
            "case {case}: pager {pager} {cell_w} {gap} {offset}: {at:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------
// 5. The survey over every real tree in the fixtures.
// ---------------------------------------------------------------------------------------

/// The captures known to show a row's actions (each with the row it shows).
const KNOWN_REVEALED: [&str; 14] = [
    "p13b-swipe-narrow-02-revealed-1-tree.txt",
    "p13c-swipe-columns-02-revealed-1-tree.txt",
    "p13d-swipe-leading-02-revealed-1-tree.txt",
    "p13d-swipe-leading-05-revealed-1-tree.txt",
    "p13f-revealed-tree-02-revealed-inset-one-row-tree.txt",
    "p13f-revealed-tree-03-revealed-inset-one-row-runner-tree.txt",
    "p13f-revealed-tree-05-revealed-edge-one-row-tree.txt",
    "p13f-revealed-tree-06-revealed-edge-one-row-runner-tree.txt",
    "p13f-revealed-tree-08-revealed-inset-two-row-tree.txt",
    "p13f-revealed-tree-09-revealed-inset-two-row-runner-tree.txt",
    "p13f-revealed-tree-11-revealed-leading-one-row-tree.txt",
    "p13f-revealed-tree-12-revealed-leading-one-row-runner-tree.txt",
    "p13g-revealed-tap-02-revealed-tap-row-1-tree.txt",
    "p4d-controls-03-row1-after-runner-swipe-tree.txt",
];

/// Every real tree in the fixtures of `jarvis-agent` and `jarvis-observe` (Home Screen,
/// App Library, Today view, folders, share sheet, edit menus, pickers, Safari, Settings,
/// Contacts, every list): where a full-width sideways path reads as crossing a revealed row
/// (`SURVEY|` lines). Only the captures that show a row's actions do, and only in that
/// row's band; no capture at rest (no carousel, pager or icon grid) does.
#[test]
fn survey_of_every_real_tree() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dirs = [
        root.join("tests/fixtures/ios26_probes"),
        root.join("tests/fixtures/ios26_probes3"),
        root.join("tests/fixtures/safari_form"),
        root.join("../jarvis-observe/tests/fixtures/ios26"),
        root.join("../jarvis-observe/tests/fixtures/ios26_extra"),
    ];
    let mut read = 0usize;
    let mut found = Vec::new();
    for dir in dirs {
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "txt"))
            .collect();
        files.sort();
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap();
            let typed = text
                .split("\n== raw XCUITest snapshot")
                .next()
                .unwrap_or(&text);
            let Ok(s) = ios::parse_screen(typed) else {
                println!("SURVEY|{}|not a tree", f.display());
                continue;
            };
            read += 1;
            let name = f.file_name().unwrap().to_string_lossy().into_owned();
            let at = revealed_at(&s);
            println!(
                "SURVEY|{name}|{}",
                match (at.first(), at.last()) {
                    (Some(a), Some(b)) => format!("revealed at y {a} to {b}"),
                    _ => "none".into(),
                }
            );
            if !at.is_empty() {
                found.push((name, at));
            }
        }
    }
    assert!(read >= 100, "{read} trees read");
    let names: Vec<&str> = found.iter().map(|(n, _)| n.as_str()).collect();
    let mut known: Vec<&str> = KNOWN_REVEALED.to_vec();
    known.sort_unstable();
    let mut got = names.clone();
    got.sort_unstable();
    assert_eq!(got, known, "trees that read as revealed");
    // Each only in the band of its revealed row (52-pt rows: at most 26 heights).
    for (name, at) in &found {
        let span = at.last().unwrap() - at.first().unwrap();
        assert!(span <= 54.0 && at.len() <= 28, "{name}: {at:?}");
    }
}
