#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 33: break-it tests against b229c8d (review pass 32).
//!
//! FINDINGS (these tests FAIL on b229c8d, for the reason named):
//! - `f1_a_narrow_inset_row_gets_a_drag_past_the_measured_delete_share`: an inset row
//!   (a `Cell` under 0.8 of its list, no grid neighbour) narrower than 178 pt gets the 80-pt
//!   floor: 0.61 to 0.80 of the row, at or past the shares that deleted real rows (0.60
//!   leading action, 0.75 on 200- and 300-pt rows), where a list row of the same width is
//!   refused (`RowTooNarrow`).
//! - `f2_a_two_column_list_row_keeps_the_screen_cap`: the measured two-column layout
//!   (200-pt rows, run 149) with both columns in ONE list (a grid of swipeable cards):
//!   every row has an alike neighbour, so the grid signal lifts the bound; the element-less
//!   swipe travels 240 pt = 1.26 of the 190-pt row (0.75 deleted the 200-pt column row).
//!
//! SOUND (these pass; properties checked):
//! - `s1_deny_delete_and_the_question_hold_on_the_finding_layouts`
//! - `s2_grid_signal_cost_is_linear_in_band_cells`
//! - `s3_every_encoded_snapshot_fits_the_frame_and_flags_what_it_drops`
//! - `s4_an_overlay_painted_last_cut_by_the_wire_is_an_incomplete_screen`
//! - `s5_regression_survey_of_sideways_swipes_on_real_trees` (prints what each gets)

mod common;

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_observe::{compact_view, CompactView};
use jarvis_proto::fixtures::{rect, IPHONE_SIZE};
use jarvis_proto::{Action, Direction, Policy, Rect, RiskCategory, Role, Screen, UiNode};
use jarvis_safety::gestures::{
    drag_gesture, GestureError, GestureKind, MIN_TRAVEL, ROW_SWIPE_SHARE,
};
use jarvis_safety::{
    evaluate_owned, planned_drag, Decision, ScreenSource, ScreenTree, INCOMPLETE_SCREEN_WARNING,
    ROW_SWIPE_NOTE,
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

/// The largest share of a row measured to only reveal its actions on the worst row kind
/// (the leading-action row: 242 of 440 pt, run 149).
const REVEAL_ONLY_SHARE: f64 = 0.55;

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

fn planned(s: &Screen, frame: Option<Rect>) -> Result<f64, GestureError> {
    let v = compact_view(s);
    planned_drag(
        GestureKind::Swipe,
        Direction::Left,
        frame,
        &v.screen,
        Some(&tree(&v)),
    )
    .map(|g| length(&g))
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

fn index_in(v: &CompactView, label: &str) -> u32 {
    v.screen
        .elements
        .iter()
        .find(|e| e.label == label)
        .unwrap_or_else(|| panic!("{label}"))
        .index
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

/// `rows_at(20, 190)` with a copy of every row beside it at x = 230, in the SAME list: a
/// two-column list of swipeable rows (the measured two-column layout of run 149, but with
/// both columns in one `CollectionView`, as a grid of swipeable cards lays them out).
fn two_columns() -> Screen {
    let mut s = rows_at(20.0, 190.0);
    edit(&mut s.root, &mut |n| {
        if n.role == Role::CollectionView {
            let mut out = Vec::new();
            for c in n.children.drain(..) {
                let right = (c.role == Role::Cell && (c.frame.x - 20.0).abs() < 0.5).then(|| {
                    let mut r = c.clone();
                    edit(&mut r, &mut |k| {
                        if (k.frame.x - 20.0).abs() < 0.5 && (k.frame.width - 190.0).abs() < 0.5 {
                            k.frame.x = 230.0;
                        }
                        if let Some(l) = &k.label {
                            k.label = Some(format!("Right {l}"));
                        }
                    });
                    r
                });
                out.push(c);
                out.extend(right);
            }
            n.children = out;
        }
    });
    s
}

fn decide(p: &Policy, a: &Action, s: &Screen) -> Decision {
    let v = compact_view(s);
    evaluate_owned(
        p,
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
            "confirm {:?}{}{}",
            q.category,
            if q.phone_only { " phone-only" } else { "" },
            if q.summary.contains(ROW_SWIPE_NOTE) {
                " row-note"
            } else {
                ""
            }
        ),
    }
}

// ---------------------------------------------------------------------------------------
// F1 (finding, low): the inset row's 80-pt floor.
// ---------------------------------------------------------------------------------------

/// Pass 32 bounds an inset row (a `Cell` under 0.8 of its list, no grid neighbour) at
/// `max(0.45 * width, MIN_TRAVEL)`: "never below 80 pt, never refused". For a row under
/// 178 pt that floor is more than 0.45 of the row: 0.61 of a 132-pt row (0.60 deleted the
/// leading-action row), 0.67 of 120 pt, 0.80 of 100 pt (0.75 deleted the 200- and 300-pt
/// rows). The real P13 rows inset to these widths (a padded list on a narrow column, or a
/// row the app reports narrowed) are swiped that far once the question is approved — while a
/// LIST row of the same width is refused outright (`RowTooNarrow`, the control below).
/// Expected: at most the largest share measured to only reveal (0.55), or no drag.
#[test]
fn f1_a_narrow_inset_row_gets_a_drag_past_the_measured_delete_share() {
    let mut failures = Vec::new();
    for width in [100.0f64, 120.0, 132.0] {
        // Control: the same width as a one-row list (a list row) is refused.
        let x = (440.0 - width) / 2.0;
        let one = {
            let row =
                UiNode::new(Role::Cell, Rect::new(x, 278.0, width, 52.0)).with_children(vec![
                    UiNode::new(Role::StaticText, Rect::new(x, 278.0, width, 52.0))
                        .with_label("Only row"),
                ]);
            let list = UiNode::new(Role::CollectionView, Rect::new(x, 278.0, width, 52.0))
                .with_children(vec![row]);
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
        assert_eq!(
            planned(&one, Some(frame_of(&one, "Only row"))),
            Err(GestureError::RowTooNarrow),
            "control {width}"
        );
        let s = rows_at(x, width);
        let t = match planned(&s, Some(frame_of(&s, "Full swipe row 1"))) {
            Ok(t) => t,
            Err(_) => continue,
        };
        if t > REVEAL_ONLY_SHARE * width + 1e-6 {
            failures.push(format!(
                "inset row {width} pt: travels {t:.1} pt = {:.2} of the row (a list row of \
                 this width is refused; 0.60 and 0.75 deleted real rows)",
                t / width
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

// ---------------------------------------------------------------------------------------
// F2 (finding, low; the documented forger residual, reachable by an honest layout).
// ---------------------------------------------------------------------------------------

/// Run 149 measured a two-column list: the 200-pt left column's row was deleted from 150 pt
/// (0.75). With both columns in ONE list (a two-column grid of swipeable cards), every row
/// has an alike, drawn, uncovered neighbour in its band, so pass 32's grid signal keeps the
/// screen cap for it: the element-less swipe left (from x = 340 in the right column to
/// x = 100) travels 240 pt = 1.26 of the 190-pt row it starts on; the element swipe on the
/// right row 114 pt (0.60). Both are sent once the row-swipe question is approved (and
/// unasked with an empty confirm list). Expected (the commit's claim "every crossed Cell is
/// bounded at 0.45 of its own width" unless it is a grid cell): a grid of rows that are
/// themselves swipeable is not told apart from one that is not, so at least the
/// element-less drag should stay within the row it starts on.
#[test]
fn f2_a_two_column_list_row_keeps_the_screen_cap() {
    let s = two_columns();
    // The layout is what it says: two alike columns in one list.
    let left = frame_of(&s, "Full swipe row 1");
    let right = frame_of(&s, "Right Full swipe row 1");
    assert!((left.width - 190.0).abs() < 0.5 && (right.x - 230.0).abs() < 0.5);
    // The element swipe on the right row: 114 pt = 0.60 (printed; under the 0.66 measured
    // to only reveal on the 200-pt column, so not asserted).
    println!("element swipe: {:?}", planned(&s, Some(right)));
    let mut failures = Vec::new();
    #[allow(clippy::single_element_loop)]
    for (name, frame) in [("element-less", None::<Rect>)] {
        let t = planned(&s, frame).unwrap();
        if t > ROW_SWIPE_SHARE * 190.0 + 1e-6 {
            failures.push(format!(
                "{name}: travels {t:.1} pt = {:.2} of the 190-pt row (bound {ROW_SWIPE_SHARE}; \
                 0.75 deleted the measured 200-pt column row)",
                t / 190.0
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

// ---------------------------------------------------------------------------------------
// SOUND.
// ---------------------------------------------------------------------------------------

/// On both finding layouts the deny policy and the question hold: under a denied `Delete`
/// every sideways swipe/scroll across the row is refused; by default it asks as `Delete`,
/// phone only, with the row-swipe note. With an empty confirm list it is sent unasked
/// (printed: the bound is then the only guard, which is why F1/F2 matter).
#[test]
fn s1_deny_delete_and_the_question_hold_on_the_finding_layouts() {
    for (name, s, label) in [
        ("inset 100", rows_at(170.0, 100.0), "Full swipe row 1"),
        ("two columns", two_columns(), "Right Full swipe row 1"),
    ] {
        let v = compact_view(&s);
        let el = index_in(&v, label);
        for a in [
            Action::Swipe {
                direction: Direction::Left,
                element: Some(el),
            },
            Action::Swipe {
                direction: Direction::Left,
                element: None,
            },
            Action::Scroll {
                direction: Direction::Right,
                element: Some(el),
            },
            Action::Swipe {
                direction: Direction::Right,
                element: Some(el),
            },
        ] {
            let mut deny = config().policy;
            deny.deny_categories = vec![RiskCategory::Delete];
            assert!(
                matches!(decide(&deny, &a, &s), Decision::Deny(_)),
                "{name} {a:?}"
            );
            let Decision::Confirm(q) = decide(&config().policy, &a, &s) else {
                panic!("{name} {a:?}: not asked")
            };
            assert_eq!(q.category, RiskCategory::Delete, "{name} {a:?}");
            assert!(q.phone_only && q.summary.contains(ROW_SWIPE_NOTE));
            let mut empty = config().policy;
            empty.confirm = vec![];
            println!(
                "{name} {a:?}: empty confirm list -> {}",
                describe(&decide(&empty, &a, &s))
            );
        }
    }
}

/// The grid signal's cost with thousands of cells in the drag's band of one list (every
/// pair alike, so every cover check runs): 5x the cells costs at most ~8x.
#[test]
fn s2_grid_signal_cost_is_linear_in_band_cells() {
    let screen_of = |n: usize| {
        let cells: Vec<UiNode> = (0..n)
            .map(|i| {
                let x = 20.0 + (i % 2) as f64 * 100.0;
                UiNode::new(Role::Cell, Rect::new(x, 400.0, 90.0, 52.0)).with_children(vec![
                    UiNode::new(Role::StaticText, Rect::new(x, 400.0, 90.0, 52.0))
                        .with_label(format!("Cell {i}")),
                ])
            })
            .collect();
        let list = UiNode::new(Role::CollectionView, IPHONE_SIZE.to_rect()).with_children(cells);
        jarvis_proto::fixtures::screen(
            "com.example.shop",
            "Shop",
            UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_children(vec![UiNode::new(
                Role::Window,
                IPHONE_SIZE.to_rect(),
            )
            .with_children(vec![list])]),
            false,
        )
    };
    let mut times = Vec::new();
    for n in [2_000usize, 10_000] {
        let s = screen_of(n);
        let v = compact_view(&s);
        let t = std::time::Instant::now();
        for _ in 0..3 {
            let _ = planned_drag(
                GestureKind::Swipe,
                Direction::Left,
                Some(Rect::new(20.0, 400.0, 90.0, 52.0)),
                &v.screen,
                Some(&tree(&v)),
            );
        }
        times.push(t.elapsed().as_secs_f64());
    }
    println!("grid signal: {times:?}");
    assert!(times[1] < times[0] * 8.0 + 0.05, "{times:?}");
}

/// The wire's byte budget over many shapes: labels and identifiers from 0 to 100 KiB (some
/// of multi-byte characters), trees of 1 to 30,000 nodes, a screenshot from none to the
/// largest carried, many notes: every snapshot encodes, fits the server frame, decodes, and
/// is flagged cut exactly when what comes back differs from what was sent.
#[test]
fn s3_every_encoded_snapshot_fits_the_frame_and_flags_what_it_drops() {
    use jarvis_runner::message::{decode_reply, encode_reply};
    use jarvis_runner::{LockState, Reply, Screenshot, Snapshot};
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for case in 0..24u64 {
        let nodes = [1usize, 50, 2_000, 30_000][(case % 4) as usize];
        let label_max = [0usize, 900, 20_000, 100_000][((case / 4) % 4) as usize];
        let shot = [0usize, 1_000_000, jarvis_runner::MAX_SCREENSHOT_BYTES][(case % 3) as usize];
        let mut kids = Vec::with_capacity(nodes);
        for i in 0..nodes {
            let len = if label_max == 0 {
                0
            } else {
                (next() as usize) % label_max
            };
            let text = if i % 3 == 0 {
                "é".repeat(len / 2)
            } else {
                "x".repeat(len)
            };
            let mut n = UiNode::new(Role::StaticText, rect(0.0, i as f64, 100.0, 1.0))
                .with_label(text.clone());
            if i % 7 == 0 {
                n.identifier = Some(text.clone());
                n.role = Role::Other(text);
            }
            kids.push(n);
        }
        let root =
            UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_children(vec![UiNode::new(
                Role::Window,
                IPHONE_SIZE.to_rect(),
            )
            .with_children(kids)]);
        let snap = Snapshot {
            root: root.clone(),
            app_bundle_id: "com.example.shop".into(),
            app_name: Some("Shop".into()),
            screen_size: IPHONE_SIZE,
            keyboard_visible: false,
            orientation: jarvis_proto::Orientation::Portrait,
            device_locked: false,
            lock_state: LockState::Unlocked,
            notes: (0..200)
                .map(|k| format!("note {k} {}", "n".repeat(500)))
                .collect(),
            screenshot: (shot > 0).then(|| Screenshot {
                data: vec![7u8; shot],
                mime_type: "image/png".into(),
                pixel_width: 1320,
                pixel_height: 2868,
                scale: Some(3.0),
            }),
            tree_cut: false,
        };
        let bytes = encode_reply(1, &Reply::Snapshot(Box::new(snap))).expect("encoded");
        assert!(
            bytes.len() + jarvis_runner::channel::DATA_OVERHEAD <= jarvis_runner::MAX_SERVER_FRAME,
            "case {case}: {} bytes",
            bytes.len()
        );
        let (_, Reply::Snapshot(back)) = decode_reply(&bytes).expect("decoded") else {
            panic!("not a snapshot")
        };
        assert_eq!(back.tree_cut, back.root != root, "case {case}");
    }
}

/// An overlay painted LAST (an alert at the end of the window, after a long page) is what a
/// pre-order byte cut drops first. Over the wire the screen comes back flagged; in a
/// session a tap on the page under where the alert was asks with the incomplete-screen
/// warning, phone only, and under a denied category it is refused (nothing tapped).
#[test]
fn s4_an_overlay_painted_last_cut_by_the_wire_is_an_incomplete_screen() {
    use jarvis_runner::message::{decode_reply, encode_reply};
    use jarvis_runner::{LockState, Reply, Snapshot};
    let long = "x".repeat(1_000);
    let mut kids: Vec<UiNode> = vec![UiNode::new(Role::Button, rect(16.0, 400.0, 361.0, 44.0))
        .with_label("Continue")
        .hittable(true)];
    kids.extend((0..20_000).map(|i| {
        UiNode::new(Role::StaticText, rect(0.0, 1_000.0 + i as f64, 393.0, 1.0))
            .with_label(format!("{i} {long}"))
    }));
    kids.push(
        UiNode::new(Role::Alert, IPHONE_SIZE.to_rect()).with_children(vec![UiNode::new(
            Role::Button,
            rect(16.0, 400.0, 361.0, 44.0),
        )
        .with_label("Delete all data")
        .hittable(true)]),
    );
    let root =
        UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_children(vec![UiNode::new(
            Role::Window,
            IPHONE_SIZE.to_rect(),
        )
        .with_children(kids)]);
    let snap = Snapshot {
        root,
        app_bundle_id: "com.example.shop".into(),
        app_name: Some("Shop".into()),
        screen_size: IPHONE_SIZE,
        keyboard_visible: false,
        orientation: jarvis_proto::Orientation::Portrait,
        device_locked: false,
        lock_state: LockState::Unlocked,
        notes: vec![],
        screenshot: None,
        tree_cut: false,
    };
    let bytes = encode_reply(1, &Reply::Snapshot(Box::new(snap))).unwrap();
    let (_, Reply::Snapshot(back)) = decode_reply(&bytes).unwrap() else {
        panic!()
    };
    assert!(back.tree_cut);
    let screen = Screen {
        app_bundle_id: back.app_bundle_id,
        app_name: back.app_name,
        size: back.screen_size,
        root: back.root,
        keyboard_visible: back.keyboard_visible,
        orientation: back.orientation,
        tree_cut: back.tree_cut,
    };
    assert!(!format!("{:?}", screen.root).contains("Delete all data"));
    let idx = index_of(&screen, "Continue");
    let tap = format!(r#"{{"action":"tap","element":{idx}}}"#);
    let fail = r#"{"action":"fail","reason":"stop"}"#;
    let r = run(
        config(),
        "Continue",
        MemoryStore::new(),
        Phone::still(screen.clone()),
        Script::model(&[&tap, fail]),
    );
    let q = r.confirms.first().expect("asked");
    assert!(q.phone_only && q.summary.starts_with(INCOMPLETE_SCREEN_WARNING));
    let mut c = config();
    c.policy.deny_categories = vec![RiskCategory::Delete];
    let r = run(
        c,
        "Continue",
        MemoryStore::new(),
        Phone::still(screen),
        Script::model(&[&tap, fail]).decisions(&[true; 4]),
    );
    assert!(r.confirms.is_empty());
    assert!(!r
        .acts
        .iter()
        .any(|a| matches!(a, jarvis_proto::RunnerCommand::Tap { .. })));
}

/// What a sideways swipe gets on the real iOS 26.5 trees: the element-less swipe left and
/// the element swipe left on the first listed cell (when one is listed), under the default
/// policy — the decision and the planned travel against the reach. Printed (run with
/// `--nocapture`); asserts only that nothing panics and that the planned path never exceeds
/// the reach.
#[test]
fn s5_regression_survey_of_sideways_swipes_on_real_trees() {
    let cases: Vec<(&str, &str)> = vec![
        ("p13 rows", P13_ROWS),
        (
            "p4d controls rows",
            fixture!(
                "ios26_probes3",
                "p4d-controls-02-controls-before-swipes-runner-tree.txt"
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
            "date input",
            fixture!("ios26_probes3", "p6b-safari-inputs-04-date-input-tree.txt"),
        ),
        ("share sheet", fixture!("ios26_probes", "share_sheet.txt")),
        ("edit menu", fixture!("ios26_probes", "edit_menu.txt")),
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
        let first_cell = v
            .screen
            .elements
            .iter()
            .find(|e| e.role == Role::Cell || e.role == Role::Button)
            .map(|e| (e.index, e.frame, e.label.clone()));
        let mut targets: Vec<(Option<u32>, Option<Rect>, String)> =
            vec![(None, None, "element-less".into())];
        if let Some((i, f, l)) = first_cell {
            targets.push((Some(i), Some(f), format!("{l:?}")));
        }
        for (el, frame, what) in targets {
            let a = Action::Swipe {
                direction: Direction::Left,
                element: el,
            };
            let d = decide(&config().policy, &a, &s);
            let reach = drag_gesture(GestureKind::Swipe, Direction::Left, frame, &v.screen)
                .map(|g| length(&g));
            let plan = planned_drag(
                GestureKind::Swipe,
                Direction::Left,
                frame,
                &v.screen,
                Some(&tree(&v)),
            )
            .map(|g| length(&g));
            if let (Ok(p), Ok(r)) = (&plan, &reach) {
                assert!(*p <= *r + 1e-6, "{name} {what}");
            }
            println!(
                "{name:18} {what:40} {:32} planned {:?} reach {:?}",
                describe(&d),
                plan.map(|t| (t * 10.0).round() / 10.0),
                reach.map(|t| (t * 10.0).round() / 10.0)
            );
        }
    }
    let _ = MIN_TRAVEL;
}
