#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Run 146 (Mac, probe P13; P05b Reminders agrees): a sideways drag across a list row is
//! bounded by the ROW, not the screen. On a 400-pt row (`Cell {{20, 278}, {400, 52}}`, a
//! 440-pt screen) 240 pt (0.60 of the row) only revealed the row's actions, and 264 pt (0.66)
//! and more deleted the row every time. The core's capped swipe (240/440 of the screen, and
//! 0.6 of an element's width with an 80-pt minimum) gave a narrower row — an inset list, a
//! split view, a landscape column — a drag above that share. Now the travel across a row is
//! at most `ROW_SWIPE_SHARE` (0.45; run 149) of the narrowest crossed row, and a row too narrow for
//! the 80-pt minimum gets no drag at all.

mod common;

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_observe::{compact_view, CompactView};
use jarvis_proto::fixtures::random::Rng;
use jarvis_proto::{Action, Direction, Rect, Role, RunnerCommand, Screen, UiNode};
use jarvis_safety::gestures::{
    drag_gesture, Gesture, GestureError, GestureKind, MAX_SIDEWAYS_SHARE, MIN_TRAVEL,
    ROW_SWIPE_SHARE,
};
use jarvis_safety::{planned_drag, ScreenTree, ROW_SWIPE_NOTE};

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("fixtures/ios26_probes3/", $name))
    };
}

const P13_ROWS: &str = fixture!("p13-swipe-lengths-01-rows-tree.txt");
const P4D_BEFORE: &str = fixture!("p4d-controls-02-controls-before-swipes-runner-tree.txt");

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

fn travel(g: &Gesture) -> f64 {
    (g.to.x - g.from.x).hypot(g.to.y - g.from.y)
}

/// The drag planned for a swipe left on the element labelled `label` (or element-less).
fn planned(s: &Screen, label: Option<&str>) -> Result<Gesture, GestureError> {
    let v = compact_view(s);
    let frame = label.map(|l| {
        v.screen
            .elements
            .iter()
            .find(|e| e.label == l)
            .unwrap_or_else(|| panic!("{l}: {:#?}", v.screen.elements))
            .frame
    });
    planned_drag(
        GestureKind::Swipe,
        Direction::Left,
        frame,
        &v.screen,
        Some(&tree(&v)),
    )
}

/// The P13 list with every row's cell (and everything it holds) narrowed to `width` points,
/// centred on the screen: an inset list.
fn narrowed(width: f64) -> Screen {
    let mut s = parse(P13_ROWS);
    let x = (440.0 - width) / 2.0;
    edit(&mut s.root, &mut |n| {
        if n.role == Role::Cell {
            edit(n, &mut |k| {
                if k.frame.width >= 399.0 {
                    k.frame.x = x;
                    k.frame.width = width;
                }
            });
        }
    });
    s
}

/// On the real 400-pt row: the swipe on "Full swipe row 1" travels 0.45 of the row (180 pt),
/// below the 240 pt measured to reveal only and the 264 pt measured to delete, from the same
/// start as before; the element-less swipe across the rows likewise.
#[test]
fn p13_a_row_swipe_travels_a_share_of_the_row() {
    let s = parse(P13_ROWS);
    let c = compact_view(&s).screen;
    for label in [Some("Full swipe row 1"), None] {
        let g = planned(&s, label).unwrap();
        let t = travel(&g);
        assert!((t - ROW_SWIPE_SHARE * 400.0).abs() < 1e-6, "{label:?}: {t}");
        assert!(
            t < 240.0,
            "below the 240 pt that only revealed (and the 264 pt that deleted)"
        );
        let reach = drag_gesture(
            GestureKind::Swipe,
            Direction::Left,
            label.map(|l| c.elements.iter().find(|e| e.label == l).unwrap().frame),
            &c,
        )
        .unwrap();
        assert_eq!(g.from, reach.from, "the start is the one the gate read");
        assert!(travel(&reach) >= t);
    }
}

/// Narrowed rows: the travel is at most 0.45 of the row whatever its width, and a row too
/// narrow for an 80-pt drag gets none (the old code sent 0.6 of the element, at least 80).
#[test]
fn p13_a_narrowed_row_gets_a_shorter_drag_or_none() {
    for width in [
        360.0, 300.0, 240.0, 200.0, 179.0, 178.0, 177.0, 150.0, 120.0, 100.0, 60.0,
    ] {
        let s = narrowed(width);
        for label in [Some("Full swipe row 1"), None] {
            match planned(&s, label) {
                Ok(g) => {
                    let t = travel(&g);
                    assert!(
                        t <= ROW_SWIPE_SHARE * width + 1e-9,
                        "{width} {label:?}: {t}"
                    );
                    assert!(t + 1e-9 >= MIN_TRAVEL.min(ROW_SWIPE_SHARE * width));
                }
                Err(e) => {
                    assert_eq!(e, GestureError::RowTooNarrow, "{width} {label:?}");
                    assert!(ROW_SWIPE_SHARE * width < MIN_TRAVEL, "{width} {label:?}");
                }
            }
        }
        if ROW_SWIPE_SHARE * width < MIN_TRAVEL {
            assert_eq!(
                planned(&s, Some("Full swipe row 1")),
                Err(GestureError::RowTooNarrow),
                "{width}"
            );
        }
    }
}

/// The P4d controls list (400-pt rows): the swipe on the segmented control's "Two" (in its
/// row's `Cell`) and on "Swipe row 1" are bounded by the row (they were 240 pt).
#[test]
fn p4d_row_swipes_are_bounded_by_the_row() {
    let s = parse(P4D_BEFORE);
    for label in ["Two", "Swipe row 1"] {
        let g = planned(&s, Some(label)).unwrap();
        assert!(
            travel(&g) <= ROW_SWIPE_SHARE * 400.0 + 1e-9,
            "{label}: {}",
            travel(&g)
        );
    }
}

/// A drag that crosses no row keeps the screen-share cap alone (the old path).
#[test]
fn a_drag_across_no_row_keeps_the_screen_cap() {
    let s = home_screen();
    let v = compact_view(&s);
    let p = planned_drag(
        GestureKind::Swipe,
        Direction::Left,
        None,
        &v.screen,
        Some(&tree(&v)),
    )
    .unwrap();
    let reach = drag_gesture(GestureKind::Swipe, Direction::Left, None, &v.screen).unwrap();
    assert_eq!(p, reach);
    assert!(travel(&p) <= v.screen.size.width * MAX_SIDEWAYS_SHARE + 1e-9);
}

/// Property: for any row width and position, the planned travel across it is at most
/// `ROW_SWIPE_SHARE` of the row's width (or the drag is refused, never longer).
#[test]
fn property_planned_travel_is_bounded_by_the_row() {
    for seed in 0..400u64 {
        let r = &mut Rng::new(seed);
        let width = r.f(20.0, 440.0);
        let x = r.f(0.0, 440.0 - width);
        let y = r.f(150.0, 700.0);
        let label = format!("Row {seed}");
        let cell =
            UiNode::new(Role::Cell, Rect::new(x, y, width, 52.0)).with_children(vec![UiNode::new(
                Role::StaticText,
                Rect::new(x, y, width, 52.0),
            )
            .with_label(&label)]);
        let list = UiNode::new(Role::CollectionView, Rect::new(0.0, 100.0, 440.0, 800.0))
            .with_children(vec![cell]);
        let root = UiNode::new(Role::Application, Rect::new(0.0, 0.0, 440.0, 956.0))
            .with_label("App")
            .with_children(vec![UiNode::new(
                Role::Window,
                Rect::new(0.0, 0.0, 440.0, 956.0),
            )
            .with_children(vec![list])]);
        let mut s = jarvis_proto::fixtures::screen("com.example.app", "App", root, false);
        s.size = jarvis_proto::Size::new(440.0, 956.0);
        for dir in [Direction::Left, Direction::Right] {
            for kind in [GestureKind::Swipe, GestureKind::Scroll] {
                let v = compact_view(&s);
                let frame = v
                    .screen
                    .elements
                    .iter()
                    .find(|e| e.label == label)
                    .map(|e| e.frame);
                match planned_drag(kind, dir, frame, &v.screen, Some(&tree(&v))) {
                    Ok(g) => assert!(
                        travel(&g) <= ROW_SWIPE_SHARE * width + 1e-9,
                        "seed {seed}: {width} {g:?}"
                    ),
                    Err(GestureError::RowTooNarrow) => {
                        assert!(ROW_SWIPE_SHARE * width < MIN_TRAVEL, "seed {seed}")
                    }
                    Err(_) => {}
                }
            }
        }
    }
}

/// Through a session: the narrow row's swipe is not sent, the model reads why, and the
/// owner is not asked about a drag that will not happen; a wide row's swipe is asked with
/// the note saying what the measurement says, and the path sent is the bounded one.
#[test]
fn a_session_refuses_a_narrow_row_swipe_and_bounds_a_wide_one() {
    let swipe_on = |s: &Screen| {
        let i = index_of(s, "Full swipe row 1");
        format!(r#"{{"action":"swipe","direction":"left","element":{i}}}"#)
    };
    let narrow = narrowed(150.0);
    let r = run(
        config(),
        "Reveal the row's actions",
        MemoryStore::new(),
        Phone::still(narrow.clone()),
        Script::model(&[&swipe_on(&narrow), r#"{"action":"fail","reason":"stop"}"#])
            .decisions(&[true; 4]),
    );
    assert!(
        !r.acts
            .iter()
            .any(|a| matches!(a, RunnerCommand::Swipe { .. })),
        "{:?}",
        r.acts
    );
    assert!(r.confirms.is_empty(), "{:#?}", r.confirms);
    assert!(
        r.last_user().contains("too narrow to swipe safely"),
        "{}",
        r.last_user()
    );

    let wide = parse(P13_ROWS);
    let r = run(
        config(),
        "Reveal the row's actions",
        MemoryStore::new(),
        Phone::still(wide.clone()),
        Script::model(&[&swipe_on(&wide), r#"{"action":"fail","reason":"stop"}"#])
            .decisions(&[true; 4]),
    );
    assert!(
        r.confirms
            .iter()
            .any(|c| c.summary.contains(ROW_SWIPE_NOTE)),
        "{:#?}",
        r.confirms
    );
    assert!(ROW_SWIPE_NOTE.contains("0.60 to 0.75 of the row deleted it"));
    let sent: Vec<f64> = r
        .acts
        .iter()
        .filter_map(|a| match a {
            RunnerCommand::Swipe { from, to, .. } => Some((to.x - from.x).hypot(to.y - from.y)),
            _ => None,
        })
        .collect();
    assert_eq!(sent.len(), 1, "{:?}", r.acts);
    assert!(sent[0] <= ROW_SWIPE_SHARE * 400.0 + 1e-9, "{sent:?}");
    let _ = Action::Look;
}

/// Run 149 (P13a, P13b): on each measured row kind the planned travel stays at most the
/// largest drag measured to only reveal the row's actions there (the left-to-right swipe of
/// the leading-action row too).
#[test]
fn p13ab_each_row_kind_gets_at_most_its_reveal_only_length() {
    for (text, label, dir, revealed, width) in [
        (
            include_str!(
                "fixtures/ios26_probes3/p13a-swipe-lengths-01-rows-full-swipe-row-tree.txt"
            ),
            "Full swipe row 1",
            Direction::Left,
            250.0,
            400.0,
        ),
        (
            include_str!("fixtures/ios26_probes3/p13b-swipe-lengths-02-rows-narrow-row-tree.txt"),
            "Narrow row 1",
            Direction::Left,
            198.0,
            300.0,
        ),
        (
            include_str!("fixtures/ios26_probes3/p13b-swipe-lengths-05-rows-left-row-tree.txt"),
            "Left row 1",
            Direction::Left,
            132.0,
            200.0,
        ),
        (
            include_str!("fixtures/ios26_probes3/p13b-swipe-lengths-08-rows-leading-row-tree.txt"),
            "Leading row 1",
            Direction::Right,
            242.0,
            440.0,
        ),
    ] {
        let s = parse(text);
        let v = compact_view(&s);
        let frame = v
            .screen
            .elements
            .iter()
            .find(|e| e.label == label)
            .unwrap_or_else(|| panic!("{label}: {:#?}", v.screen.elements))
            .frame;
        for (kind, d) in [
            (GestureKind::Swipe, dir),
            (
                GestureKind::Scroll,
                if dir == Direction::Left {
                    Direction::Right
                } else {
                    Direction::Left
                },
            ),
        ] {
            let g = planned_drag(kind, d, Some(frame), &v.screen, Some(&tree(&v))).unwrap();
            let t = travel(&g);
            assert!(t <= revealed, "{label} {kind:?}: {t} > {revealed}");
            assert!(t <= ROW_SWIPE_SHARE * width + 1e-9, "{label}: {t}");
            assert!(t >= MIN_TRAVEL - 1e-9, "{label}: {t}");
        }
    }
}
