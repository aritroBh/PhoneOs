#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 37: break-it tests on the revealed-row overlap condition of commit `93982bb`
//! (`revealed_rows`' second reading: an action container must overlap a cell of its list in
//! its rows by 0.25 of the narrower width, or be that cell's size within 1 pt).
//!
//! The case the commit names as left to the second reading: a shifted row that stays
//! inside its list (a two-column list in one `CollectionView`), so the first reading (a
//! cell reaching past its list's side) does not see it.

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use jarvis_observe::{compact_view, CompactView};
use jarvis_proto::{Direction, Point, Rect, Role, Screen, Size, UiNode};
use jarvis_safety::gestures::{Gesture, GestureError};
use jarvis_safety::{planned_path, ScreenTree};

const INSET_ONE: &str =
    include_str!("fixtures/ios26_probes3/p13f-revealed-tree-02-revealed-inset-one-row-tree.txt");

/// What `ios` (the observe tests' parser) uses to print a compact screen.
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

fn node(role: Role, label: Option<&str>, frame: Rect, children: Vec<UiNode>) -> UiNode {
    UiNode {
        role,
        label: label.map(str::to_owned),
        value: None,
        identifier: None,
        placeholder: None,
        frame,
        enabled: true,
        visible: true,
        hittable: true,
        focused: false,
        selected: false,
        secure: false,
        children,
    }
}

fn cell(x: f64, y: f64, name: &str) -> UiNode {
    node(
        Role::Cell,
        None,
        Rect::new(x, y, 200.0, 52.0),
        vec![node(
            Role::StaticText,
            Some(name),
            Rect::new(x, y, 200.0, 52.0),
            Vec::new(),
        )],
    )
}

/// A two-column list in one `CollectionView` (0..440): left cells at x = 16, right cells
/// at x = 224, 200 x 52 each. The right row "Right 1" (rest frame 224..424) is pulled left
/// 180 pt (to 44..244, still inside its list) with its trailing actions in a sibling
/// container (a direct child of the list held by no cell) of frame `container`, the
/// buttons "Flag" and "Delete" hugging its trailing edge.
fn two_columns(container: Rect) -> Screen {
    let y = 300.0;
    let b = |x: f64, w: f64, l: &str| {
        node(
            Role::Button,
            Some(l),
            Rect::new(x, y + 4.0, w, 44.0),
            vec![node(
                Role::StaticText,
                Some(l),
                Rect::new(x + 10.0, y + 18.0, w - 20.0, 16.0),
                Vec::new(),
            )],
        )
    };
    let actions = node(
        Role::Other("Other".to_owned()),
        None,
        container,
        vec![
            b(container.max_x() - 170.0, 80.0, "Flag"),
            b(container.max_x() - 85.0, 80.0, "Delete"),
        ],
    );
    let list = node(
        Role::CollectionView,
        None,
        Rect::new(0.0, 250.0, 440.0, 600.0),
        vec![
            cell(16.0, y, "Left 1"),
            cell(44.0, y, "Right 1"),
            actions,
            cell(16.0, y + 60.0, "Left 2"),
            cell(224.0, y + 60.0, "Right 2"),
            cell(16.0, y + 120.0, "Left 3"),
            cell(224.0, y + 120.0, "Right 3"),
        ],
    );
    let window = node(
        Role::Window,
        None,
        Rect::new(0.0, 0.0, 440.0, 956.0),
        vec![list],
    );
    let root = node(
        Role::Application,
        Some("Lists"),
        Rect::new(0.0, 0.0, 440.0, 956.0),
        vec![window],
    );
    Screen {
        app_bundle_id: "com.example.lists".to_owned(),
        app_name: Some("Lists".to_owned()),
        size: Size::new(440.0, 956.0),
        keyboard_visible: false,
        root,
        ..parse(INSET_ONE)
    }
}

/// A 100-pt drag with the finger moving left through the right column's revealed row, from
/// over its actions (x = 400, y = 326): the same way as the reveal, which fires the
/// trailing action (a full swipe) or drags the row further.
fn same_way(s: &Screen) -> Result<Gesture, GestureError> {
    let v = compact_view(s);
    let t = tree(&v);
    planned_path(
        Gesture {
            from: Point::new(400.0, 326.0),
            to: Point::new(300.0, 326.0),
            duration_ms: 200,
        },
        Direction::Left,
        &v.screen,
        Some(&t),
    )
}

/// Sound (control): the action container at the row's rest frame (224..424, the row's
/// size, as SwiftUI places it, P13f) is read as a revealed row's: the same-way drag is
/// refused.
#[test]
fn sound_a_container_at_the_rest_frame_of_a_row_inside_its_list() {
    let s = two_columns(Rect::new(224.0, 300.0, 200.0, 52.0));
    assert_eq!(same_way(&s), Err(GestureError::RowActionsShowing));
}

/// F9 (low): the action container sized to the revealed strip only (244..424, 180 x 52:
/// the area the row vacated, where UIKit's swipe-action pull view lies) overlaps no cell of
/// its rows (the shifted row ends at 244, the left column at 216) and is not a cell's size,
/// so since `93982bb` it no longer reads as a revealed row's; the row stays inside its list,
/// so the first reading does not see it either. A drag left through the revealed "Delete"
/// is planned (the full swipe fires the trailing action). Before `93982bb` any such
/// container in a cell's rows was enough (the diff only adds the overlap condition).
/// Synthetic: no capture of a two-column list in one `CollectionView` or of a UIKit table
/// exists in the fixtures (P13h's captures are not in the repository); the second reading
/// is described as "not required" for the measured SwiftUI shapes.
#[test]
fn f9_a_revealed_strip_container_inside_its_list_is_not_read_as_revealed() {
    let s = two_columns(Rect::new(244.0, 300.0, 180.0, 52.0));
    assert_eq!(same_way(&s), Err(GestureError::RowActionsShowing));
}
