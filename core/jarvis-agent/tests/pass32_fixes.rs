#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 32: the fixes' own tests (the reviewer's are `pass32_breakit.rs`).
//!
//! - F1: every crossed `Cell` (and every row a `Table`/`CollectionView` holds directly) is
//!   bounded at its own width; only a positive grid signal (two alike, visible, uncovered
//!   cells side by side in the path's band of the same list, each at most half of it, the
//!   row no wider than them) keeps the screen cap. The forged signals are tested here.
//! - F1b: a `Cell` is never a pager's page by its height; the Home Screen's widget pages
//!   (rows by shape of a scroll view) still are.
//! - F2: the wire cuts the tree by bytes too (and each text at `MAX_NODE_STRING_BYTES`),
//!   flags the cut, and every snapshot fits the server frame; the session can leave the
//!   cut screen.

mod common;

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_observe::{compact_view, CompactView};
use jarvis_proto::fixtures::{rect, IPHONE_SIZE};
use jarvis_proto::{Direction, Key, Rect, Role, RunnerCommand, Screen, UiNode};
use jarvis_safety::gestures::{
    drag_gesture, GestureError, GestureKind, INSET_ROW_MAX_SHARE, MIN_TRAVEL, ROW_SWIPE_SHARE,
};
use jarvis_safety::{planned_drag, ScreenTree};

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

fn length(from: jarvis_proto::Point, to: jarvis_proto::Point) -> f64 {
    (to.x - from.x).hypot(to.y - from.y)
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
    .map(|g| length(g.from, g.to));
    let reach = drag_gesture(GestureKind::Swipe, Direction::Left, frame, &v.screen).unwrap();
    (planned, length(reach.from, reach.to))
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

/// The P13 list with every row's `Cell` (and what spans it) at `x`, `width` wide; the list
/// keeps its 440 pt.
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

/// `s` with `extra` inserted into the list (`CollectionView`) right after the `Cell` of
/// "Full swipe row 1" (`y = 278`).
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

fn cell(frame: Rect, label: &str) -> UiNode {
    UiNode::new(Role::Cell, frame)
        .with_children(vec![UiNode::new(Role::StaticText, frame).with_label(label)])
}

/// The row of the left column of a two-column layout: 190 pt at x = 20 in the 440-pt list.
const LEFT: (f64, f64) = (20.0, 190.0);

fn left_bound() -> f64 {
    ROW_SWIPE_SHARE * LEFT.1
}

// ---------------------------------------------------------------------------------------
// F1: padded rows are bounded at their own width.
// ---------------------------------------------------------------------------------------

/// Every inset of the real P13 rows from 0.79 of the list down to 0.30: the element and the
/// element-less swipe travel at most 0.45 of the row (never less than `MIN_TRAVEL`), from
/// the start the gate read. Review pass 33 (F1): a row for which `MIN_TRAVEL` is more than
/// `INSET_ROW_MAX_SHARE` of it (under about 145 pt) is refused, as a narrow list row is
/// (pass 32 never refused it: that drag was 0.61 of the 132-pt row, past the measured 0.60).
#[test]
fn f1_padded_rows_of_every_inset_are_bounded_at_their_own_width() {
    for pct in 30..=79u32 {
        let share = f64::from(pct) / 100.0;
        let width = 440.0 * share;
        let s = rows_at((440.0 - width) / 2.0, width);
        for frame in [Some(frame_of(&s, "Full swipe row 1")), None] {
            let (planned, reach) = swipe(&s, frame);
            if INSET_ROW_MAX_SHARE * width < MIN_TRAVEL {
                assert_eq!(
                    planned,
                    Err(GestureError::RowTooNarrow),
                    "{share} {frame:?}"
                );
                continue;
            }
            let t = planned.unwrap_or_else(|e| panic!("{share} {frame:?}: {e}"));
            let bound = (ROW_SWIPE_SHARE * width).max(MIN_TRAVEL);
            assert!(
                t <= INSET_ROW_MAX_SHARE * width + 1e-6,
                "{share} {frame:?}: {t}"
            );
            assert!(t <= bound + 1e-6, "{share} {frame:?}: {t} > {bound}");
            assert!(t <= reach + 1e-6);
            if ROW_SWIPE_SHARE * width >= MIN_TRAVEL {
                assert!(t <= ROW_SWIPE_SHARE * width + 1e-6, "{share}: {t}");
            }
        }
    }
}

/// The control of the forgeries below: two alike cells side by side in the band of the
/// swipe, both visible and uncovered, each under half the list — a grid: the drag keeps
/// its reach (the screen-cap path, still asked). Without the neighbour the left row is an
/// inset row, bounded at 0.45 of its 190 pt. Review pass 33 (F2): a 190 x 52 cell is
/// row-shaped (3.65 times as wide as tall, as the measured two-column rows): never a grid
/// cell, so with the neighbour too it is bounded at 0.45 of its width; the grid control is
/// now made of squarish cells (150 x 52, 2.88): alone bounded at `MIN_TRAVEL`, with an
/// alike neighbour the reach (the forgeries against the grid signal on squarish cells are
/// in `pass33_fixes.rs`).
#[test]
fn f1_two_alike_cells_side_by_side_are_a_grid() {
    let lone = rows_at(LEFT.0, LEFT.1);
    let f = frame_of(&lone, "Full swipe row 1");
    let (t, reach) = swipe(&lone, Some(f));
    assert!((t.unwrap() - left_bound()).abs() < 1e-6);
    assert!(reach > left_bound() + 10.0);
    let grid = beside_row1(
        rows_at(LEFT.0, LEFT.1),
        vec![cell(Rect::new(230.0, 278.0, 190.0, 52.0), "Right column")],
    );
    let (t, reach) = swipe(&grid, Some(f));
    assert!((t.unwrap() - left_bound()).abs() < 1e-6, "{t:?} {reach}");
    // Squarish cells: the grid control.
    let lone = rows_at(20.0, 150.0);
    let f = frame_of(&lone, "Full swipe row 1");
    let (t, reach) = swipe(&lone, Some(f));
    assert!((t.unwrap() - MIN_TRAVEL).abs() < 1e-6 && reach > MIN_TRAVEL + 5.0);
    let grid = beside_row1(
        rows_at(20.0, 150.0),
        vec![cell(Rect::new(180.0, 278.0, 150.0, 52.0), "Right column")],
    );
    let (t, reach) = swipe(&grid, Some(f));
    assert!((t.unwrap() - reach).abs() < 1e-6, "{t:?} {reach}");
}

/// Forged grid signals: a neighbour that is hidden, zero-sized, off screen, 1 pt wide (one
/// or two hundred of them), covered by text drawn over it, in another list, much shorter,
/// much narrower or overlapping the row — none makes a grid: the row stays bounded at its
/// own width.
#[test]
fn f1_forged_grid_signals_do_not_lift_the_bound() {
    let right = Rect::new(230.0, 278.0, 190.0, 52.0);
    let mut hidden = cell(right, "Hidden");
    hidden.visible = false;
    for c in &mut hidden.children {
        c.visible = false;
    }
    let one_pt: Vec<UiNode> = (0..200)
        .map(|i| {
            cell(
                Rect::new(230.0 + f64::from(i), 278.0, 1.0, 52.0),
                &format!("Sliver {i}"),
            )
        })
        .collect();
    let inner_list = UiNode::new(Role::CollectionView, Rect::new(220.0, 0.0, 220.0, 956.0))
        .with_children(vec![cell(right, "Other list")]);
    let forgeries: Vec<(&str, Vec<UiNode>)> = vec![
        ("hidden", vec![hidden]),
        (
            "zero-sized",
            vec![cell(Rect::new(230.0, 278.0, 0.0, 0.0), "Zero")],
        ),
        (
            "off screen",
            vec![cell(Rect::new(500.0, 278.0, 190.0, 52.0), "Off")],
        ),
        (
            "partly off screen",
            vec![cell(Rect::new(425.0, 278.0, 190.0, 52.0), "Edge")],
        ),
        (
            "1 pt wide",
            vec![cell(Rect::new(230.0, 278.0, 1.0, 52.0), "Thin")],
        ),
        ("200 slivers", one_pt),
        ("another list", vec![inner_list]),
        (
            "much shorter",
            vec![cell(Rect::new(230.0, 300.0, 190.0, 5.0), "Short")],
        ),
        (
            "much narrower",
            vec![cell(Rect::new(230.0, 278.0, 100.0, 52.0), "Narrow")],
        ),
        (
            "overlapping",
            vec![cell(Rect::new(100.0, 278.0, 190.0, 52.0), "Over")],
        ),
    ];
    for (name, extra) in forgeries {
        let s = beside_row1(rows_at(LEFT.0, LEFT.1), extra);
        let f = frame_of(&s, "Full swipe row 1");
        let (t, _) = swipe(&s, Some(f));
        let t = t.unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(t <= left_bound() + 1e-6, "{name}: {t} > {}", left_bound());
    }
    // Covered: an alike neighbour with text drawn over its centre (a later branch of the
    // window, outside the list).
    let mut s = beside_row1(rows_at(LEFT.0, LEFT.1), vec![cell(right, "Covered")]);
    let cover =
        UiNode::new(Role::Other("Other".into()), IPHONE_SIZE.to_rect()).with_children(vec![
            UiNode::new(Role::StaticText, Rect::new(240.0, 284.0, 170.0, 40.0))
                .with_label("Banner"),
        ]);
    edit(&mut s.root, &mut |n| {
        if n.role == Role::Window {
            n.children.push(cover.clone());
        }
    });
    let f = frame_of(&s, "Full swipe row 1");
    let (t, _) = swipe(&s, Some(f));
    assert!(t.unwrap() <= left_bound() + 1e-6, "covered");
}

/// A padded row wider than half its list is never a grid cell, whatever is beside it: the
/// real P13 row at 0.60 of the list (264 pt) with the widest cell that fits beside it in
/// the margin, and with an alike 264-pt cell beside it (mostly off screen).
#[test]
fn f1_a_row_wider_than_half_its_list_is_never_a_grid_cell() {
    let width = 264.0;
    let x = (440.0 - width) / 2.0;
    for extra in [
        cell(
            Rect::new(x + width, 278.0, 440.0 - x - width, 52.0),
            "Margin",
        ),
        cell(Rect::new(x + width, 278.0, width, 52.0), "Alike"),
    ] {
        let s = beside_row1(rows_at(x, width), vec![extra]);
        let f = frame_of(&s, "Full swipe row 1");
        let (t, _) = swipe(&s, Some(f));
        assert!(t.unwrap() <= ROW_SWIPE_SHARE * width + 1e-6);
    }
}

/// The real grids keep their reach (the screen-cap path; their question is unchanged,
/// `pass31_fixes.rs`): Safari's start-page favourites, the month calendar's days, the edit
/// menu's items (and the 1-pt separators between them), the edit menu list's two top cells.
#[test]
fn f1_real_grids_keep_their_reach() {
    let cases: [(&str, &str, Rect); 5] = [
        (
            "safari favourites",
            fixture!("safari_form", "02.txt"),
            Rect::new(20.0, 447.0, 72.0, 94.3),
        ),
        (
            "calendar day",
            fixture!("ios26_probes3", "p6b-safari-inputs-04-date-input-tree.txt"),
            Rect::new(99.7, 357.3, 48.0, 50.0),
        ),
        (
            "edit menu item",
            fixture!("ios26_probes", "edit_menu.txt"),
            Rect::new(80.3, 220.0, 68.0, 44.0),
        ),
        (
            "edit menu list cell",
            fixture!("ios26_probes", "edit_menu_list.txt"),
            Rect::new(134.7, 219.0, 117.0, 77.0),
        ),
        (
            "compact date picker day",
            fixture!(
                "ios26_probes3",
                "p4d-controls-10-compact-date-open-tree.txt"
            ),
            Rect::new(0.0, 0.0, 0.0, 0.0),
        ),
    ];
    for (name, text, frame) in cases {
        let s = parse(text);
        let frame = if frame.is_empty() {
            // The compact date picker: its first day button.
            let v = compact_view(&s);
            v.screen
                .elements
                .iter()
                .find(|e| e.role == Role::Button && e.label.ends_with("January 1"))
                .unwrap_or_else(|| panic!("{name}: {:#?}", v.screen.elements))
                .frame
        } else {
            frame
        };
        let (t, reach) = swipe(&s, Some(frame));
        assert!((t.unwrap() - reach).abs() < 1e-6, "{name}: {t:?} {reach}");
    }
}

// ---------------------------------------------------------------------------------------
// F1b: one-row lists are lists; widget pages are still pages.
// ---------------------------------------------------------------------------------------

/// A one-row list of every width (the list's frame its row's): the row is a list row,
/// bounded at 0.45 of it, or refused when that is under `MIN_TRAVEL`; never the screen cap.
#[test]
fn f1b_one_row_lists_of_every_width_are_lists() {
    for width in [440.0, 400.0, 300.0, 200.0, 178.0, 150.0] {
        let x = (440.0 - width) / 2.0;
        let row = cell(Rect::new(x, 278.0, width, 52.0), "Only row");
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
        let f = frame_of(&s, "Only row");
        match swipe(&s, Some(f)).0 {
            Ok(t) => assert!(t <= ROW_SWIPE_SHARE * width + 1e-6, "{width}: {t}"),
            Err(e) => {
                assert_eq!(e, GestureError::RowTooNarrow, "{width}");
                assert!(ROW_SWIPE_SHARE * width < MIN_TRAVEL);
            }
        }
    }
}

/// The real pager: the Home Screen's widget pages (176.7-pt pages of a 176.7-pt
/// `ScrollView`, rows by shape, no `Cell`) keep their reach (no refusal).
#[test]
fn f1b_the_home_screen_widget_pages_are_still_pages() {
    let s = parse(fixture!(
        "ios26_probes3",
        "p15-springboard-01-home-tree.txt"
    ));
    for frame in [
        Rect::new(31.0, 94.0, 176.7, 176.7),
        Rect::new(232.3, 94.0, 176.7, 176.7),
    ] {
        let (t, reach) = swipe(&s, Some(frame));
        assert!((t.unwrap() - reach).abs() < 1e-6, "{frame:?}");
    }
}

// ---------------------------------------------------------------------------------------
// F2: the wire's byte budget.
// ---------------------------------------------------------------------------------------

fn snapshot_of(s: &Screen, shot: Option<jarvis_runner::Screenshot>) -> jarvis_runner::Snapshot {
    jarvis_runner::Snapshot {
        root: s.root.clone(),
        app_bundle_id: s.app_bundle_id.clone(),
        app_name: s.app_name.clone(),
        screen_size: s.size,
        keyboard_visible: s.keyboard_visible,
        orientation: s.orientation,
        device_locked: false,
        lock_state: jarvis_runner::LockState::Unlocked,
        notes: vec!["a note".into()],
        screenshot: shot,
        tree_cut: false,
    }
}

fn shop(kids: Vec<UiNode>) -> Screen {
    jarvis_proto::fixtures::screen(
        "com.example.shop",
        "Shop",
        UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_children(vec![UiNode::new(
            Role::Window,
            IPHONE_SIZE.to_rect(),
        )
        .with_children(kids)]),
        false,
    )
}

fn article(nodes: usize, label_bytes: usize) -> Screen {
    let long = "x".repeat(label_bytes);
    shop(
        (0..nodes)
            .map(|i| {
                UiNode::new(Role::StaticText, rect(0.0, i as f64, 393.0, 1.0))
                    .with_label(format!("{i} {long}"))
            })
            .collect(),
    )
}

fn count(n: &UiNode) -> usize {
    let mut c = 0usize;
    let mut stack = vec![n];
    while let Some(e) = stack.pop() {
        c += 1;
        stack.extend(e.children.iter());
    }
    c
}

/// Encodes, checks the frame bound, decodes: the screen the session would get.
fn over_the_wire(snap: jarvis_runner::Snapshot) -> (usize, Screen) {
    use jarvis_runner::message::{decode_reply, encode_reply};
    use jarvis_runner::Reply;
    let bytes = encode_reply(7, &Reply::Snapshot(Box::new(snap))).expect("encoded");
    assert!(
        bytes.len() + jarvis_runner::channel::DATA_OVERHEAD <= jarvis_runner::MAX_SERVER_FRAME,
        "{} bytes",
        bytes.len()
    );
    let (_, reply) = decode_reply(&bytes).expect("decoded");
    let Reply::Snapshot(back) = reply else {
        panic!("not a snapshot")
    };
    let screen = Screen {
        app_bundle_id: back.app_bundle_id,
        app_name: back.app_name,
        size: back.screen_size,
        root: back.root,
        keyboard_visible: back.keyboard_visible,
        orientation: back.orientation,
        tree_cut: back.tree_cut,
    };
    (bytes.len(), screen)
}

/// A long article (20,000 and 60,000 nodes of 1,000-byte labels) and the same with a
/// screenshot of the largest size carried: the tree is cut to fit the frame, flagged, and
/// what is written is a pre-order prefix (the first nodes, in order).
#[test]
fn f2_a_tree_past_the_frame_is_cut_by_bytes_and_flagged() {
    for (nodes, shot) in [
        (20_000usize, 0usize),
        (60_000, 0),
        (20_000, jarvis_runner::MAX_SCREENSHOT_BYTES),
    ] {
        let s = article(nodes, 1_000);
        let screenshot = (shot > 0).then(|| jarvis_runner::Screenshot {
            data: vec![0u8; shot],
            mime_type: "image/png".into(),
            pixel_width: 1320,
            pixel_height: 2868,
            scale: Some(3.0),
        });
        let (_, back) = over_the_wire(snapshot_of(&s, screenshot));
        assert!(back.tree_cut, "{nodes} {shot}");
        let kept = &back.root.children[0].children;
        assert!(
            !kept.is_empty() && kept.len() < nodes,
            "{nodes} {shot}: {}",
            kept.len()
        );
        for (i, k) in kept.iter().enumerate() {
            assert!(k.label.as_deref().unwrap().starts_with(&format!("{i} ")));
        }
        assert!(compact_view(&back).screen.cues.truncated);
    }
}

/// A tree that fits is not cut, and comes back equal (the real P13 tree and a 2,000-node
/// article).
#[test]
fn f2_a_tree_that_fits_is_not_cut() {
    for s in [parse(P13_ROWS), article(2_000, 1_000)] {
        let (_, back) = over_the_wire(snapshot_of(&s, None));
        assert!(!back.tree_cut);
        assert_eq!(back.root, s.root);
        assert_eq!(count(&back.root), count(&s.root));
    }
}

/// A text longer than `MAX_NODE_STRING_BYTES` (two-byte characters, so the cut falls inside
/// one) is cut at a character boundary below the bound and the snapshot is flagged; a text
/// of exactly the bound is not.
#[test]
fn f2_a_long_text_is_cut_at_a_character_boundary_and_flagged() {
    use jarvis_runner::message::MAX_NODE_STRING_BYTES;
    let long = format!("a{}", "é".repeat(MAX_NODE_STRING_BYTES));
    let s = shop(vec![UiNode::new(
        Role::StaticText,
        rect(0.0, 100.0, 393.0, 40.0),
    )
    .with_label(long.clone())]);
    let (_, back) = over_the_wire(snapshot_of(&s, None));
    assert!(back.tree_cut);
    let got = back.root.children[0].children[0].label.clone().unwrap();
    assert!(got.len() <= MAX_NODE_STRING_BYTES && got.len() >= MAX_NODE_STRING_BYTES - 1);
    assert!(long.starts_with(&got));
    let exact = "y".repeat(MAX_NODE_STRING_BYTES);
    let s = shop(vec![UiNode::new(
        Role::StaticText,
        rect(0.0, 100.0, 393.0, 40.0),
    )
    .with_label(exact.clone())]);
    let (_, back) = over_the_wire(snapshot_of(&s, None));
    assert!(!back.tree_cut);
    assert_eq!(
        back.root.children[0].children[0].label.as_deref(),
        Some(exact.as_str())
    );
}

/// Through the channels: the server's reply to a snapshot of the 20,000-node article is the
/// (cut, flagged) snapshot, not "the reply is too large".
#[test]
fn f2_the_server_channel_sends_the_cut_snapshot() {
    use jarvis_runner::{
        ClientEvent, Reply, Request, RunnerClientChannel, RunnerServerChannel, ServerEvent,
    };
    const NOW: u64 = 1_000_000;
    let key = [0x5a_u8; 32];
    let mut client = RunnerClientChannel::new(
        &key,
        &(0..32u8)
            .map(|i| i.wrapping_mul(37).wrapping_add(11))
            .collect::<Vec<u8>>(),
    )
    .unwrap();
    let mut server = RunnerServerChannel::new(
        &key,
        &(0..32u8)
            .map(|i| i.wrapping_mul(53).wrapping_add(7))
            .collect::<Vec<u8>>(),
    )
    .unwrap();
    let sends_s = |e: &[ServerEvent]| -> Vec<u8> {
        e.iter()
            .filter_map(|e| match e {
                ServerEvent::Send(b) => Some(b.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    };
    let sends_c = |e: &[ClientEvent]| -> Vec<u8> {
        e.iter()
            .filter_map(|e| match e {
                ClientEvent::Send(b) => Some(b.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    };
    let hello = client.start().unwrap();
    let server_hello = sends_s(&server.receive(&hello, NOW).unwrap());
    let finish = sends_c(&client.receive(&server_hello).unwrap());
    server.receive(&finish, NOW).unwrap();
    let (id, bytes) = client
        .request(
            &Request::Snapshot {
                include_screenshot: false,
            },
            NOW + 60_000,
        )
        .unwrap();
    server.receive(&bytes, NOW).unwrap();
    let s = article(20_000, 1_000);
    let out = server
        .reply(id, &Reply::Snapshot(Box::new(snapshot_of(&s, None))))
        .unwrap();
    let events = client.receive(&out).unwrap();
    let [ClientEvent::Reply {
        reply: Reply::Snapshot(back),
        ..
    }] = events.as_slice()
    else {
        panic!("not a snapshot: {:?}", events.len())
    };
    assert!(back.tree_cut);
}

/// In a session the cut article is an incomplete screen and Home is sent.
#[test]
fn f2_the_session_can_leave_the_cut_screen() {
    let (_, back) = over_the_wire(snapshot_of(&article(20_000, 1_000), None));
    assert!(back.tree_cut);
    let r = run(
        config(),
        "Read the article",
        MemoryStore::new(),
        Phone::still(back),
        Script::model(&[
            r#"{"action":"press_home"}"#,
            r#"{"action":"fail","reason":"stop"}"#,
        ]),
    );
    assert!(
        r.acts
            .iter()
            .any(|a| matches!(a, RunnerCommand::PressKey { key: Key::Home, .. })),
        "{:?}",
        r.acts
    );
}
