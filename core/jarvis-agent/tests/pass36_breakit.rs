#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 36: break-it tests on the revealed-row rule of commit `6bfa2eb`
//! (`jarvis_safety::planned_path`, `revealed_rows`). Variants of the real revealed capture
//! (Mac run 161, `p13f-revealed-tree-02`: the 400-pt inset row "Inset one row 1" shifted
//! to x = -87.3 with its "Delete" button in a sibling container), and a synthetic carousel.

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

fn edit(n: &mut UiNode, f: &mut impl FnMut(&mut UiNode)) {
    f(n);
    for c in &mut n.children {
        edit(c, f);
    }
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

/// The verdict on a 120-pt drag with the finger moving `finger` through the middle of the
/// revealed row's rows (y = 252.3), from x = 300.
fn verdict(s: &Screen, finger: Direction) -> Result<Gesture, GestureError> {
    let v = compact_view(s);
    let t = tree(&v);
    planned_path(
        path(300.0, 252.3, 120.0, finger),
        finger,
        &v.screen,
        Some(&t),
    )
}

/// Sound (new coverage): the capture as it is refuses the same-way drag (the control).
#[test]
fn sound_the_capture_refuses_the_same_way_drag() {
    let s = parse(INSET_ONE);
    assert_eq!(
        verdict(&s, Direction::Left),
        Err(GestureError::RowActionsShowing)
    );
}

/// Sound (new coverage): the same revealed row in a `Table` instead of a `CollectionView`
/// (a UIKit-backed list) is still read as revealed.
#[test]
fn sound_a_table_instead_of_a_collection_view() {
    let mut s = parse(INSET_ONE);
    edit(&mut s.root, &mut |n| {
        if n.role == Role::CollectionView {
            n.role = Role::Table;
        }
    });
    assert_eq!(
        verdict(&s, Direction::Left),
        Err(GestureError::RowActionsShowing)
    );
}

/// Sound (new coverage): a partial reveal (mid-animation, the cell shifted 22 pt, inside
/// the 4-pt tolerance of its list's side) is still caught by the action container.
#[test]
fn sound_a_partial_reveal_is_caught_by_its_action_container() {
    let mut s = parse(INSET_ONE);
    let mut moved = 0;
    edit(&mut s.root, &mut |n| {
        if n.frame.x == -87.3 && n.frame.y == 226.3 {
            n.frame.x = -2.0;
            moved += 1;
        }
    });
    assert!(moved >= 8, "the shifted row's nodes: {moved}");
    assert_eq!(
        verdict(&s, Direction::Left),
        Err(GestureError::RowActionsShowing)
    );
}

/// Sound (new coverage): with the action container moved one level down under a
/// full-height wrapper (so the second reading misses it), the shifted cell alone is
/// enough.
#[test]
fn sound_the_shifted_cell_alone_is_enough() {
    let mut s = parse(INSET_ONE);
    let mut wrapped = 0;
    edit(&mut s.root, &mut |n| {
        if n.role == Role::CollectionView {
            let at = n.children.iter().position(|c| {
                matches!(c.role, Role::Other(_)) && c.frame == Rect::new(20.0, 226.3, 400.0, 52.0)
            });
            if let Some(i) = at {
                let inner = n.children.remove(i);
                let mut wrapper = inner.clone();
                wrapper.frame = Rect::new(0.0, 191.3, 440.0, 764.7);
                wrapper.children = vec![inner];
                n.children.insert(i, wrapper);
                wrapped += 1;
            }
        }
    });
    assert_eq!(wrapped, 1, "the action container");
    assert_eq!(
        verdict(&s, Direction::Left),
        Err(GestureError::RowActionsShowing)
    );
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

/// A horizontal carousel (a `CollectionView` of 150 x 150 cards, 10 pt apart) scrolled to
/// its end, where a "See All" button sits in a footer after the last card in the same rows
/// (a supplementary view: a direct child `Other` of the list, held by no `Cell`). Nothing
/// is revealed.
fn carousel() -> Screen {
    let card = |x: f64, i: u32| {
        node(
            Role::Cell,
            None,
            Rect::new(x, 300.0, 150.0, 150.0),
            vec![node(
                Role::StaticText,
                Some(&format!("Album {i}")),
                Rect::new(x + 10.0, 410.0, 130.0, 20.0),
                Vec::new(),
            )],
        )
    };
    let footer = node(
        Role::Other("Other".to_owned()),
        None,
        Rect::new(346.0, 300.0, 80.0, 150.0),
        vec![node(
            Role::Button,
            Some("See All"),
            Rect::new(356.0, 355.0, 60.0, 40.0),
            Vec::new(),
        )],
    );
    let list = node(
        Role::CollectionView,
        None,
        Rect::new(0.0, 300.0, 440.0, 150.0),
        vec![card(-134.0, 4), card(26.0, 5), card(186.0, 6), footer],
    );
    let window = node(
        Role::Window,
        None,
        Rect::new(0.0, 0.0, 440.0, 956.0),
        vec![
            node(
                Role::StaticText,
                Some("Recently Played"),
                Rect::new(16.0, 260.0, 200.0, 24.0),
                Vec::new(),
            ),
            list,
        ],
    );
    let root = node(
        Role::Application,
        Some("Music"),
        Rect::new(0.0, 0.0, 440.0, 956.0),
        vec![window],
    );
    Screen {
        app_bundle_id: "com.example.music".to_owned(),
        app_name: Some("Music".to_owned()),
        size: Size::new(440.0, 956.0),
        keyboard_visible: false,
        root,
        ..parse(INSET_ONE)
    }
}

/// F7 (low): a carousel's trailing "See All" footer reads as a revealed row's action
/// container (a button no cell holds, in a row-high direct child of the list, in a cell's
/// rows; centred in its container, so "both ways"): every sideways drag across the carousel
/// is refused with `RowActionsShowing`, in both directions, where commit `3b30991` planned
/// a bounded drag. The carousel cannot be scrolled sideways by any drag through its rows.
#[test]
fn f7_a_carousel_footer_button_reads_as_revealed_row_actions() {
    let s = carousel();
    let v = compact_view(&s);
    let t = tree(&v);
    let mut wrong = Vec::new();
    for finger in [Direction::Left, Direction::Right] {
        let g = path(220.0, 375.0, 100.0, finger);
        let got = planned_path(g, finger, &v.screen, Some(&t));
        if got == Err(GestureError::RowActionsShowing) {
            wrong.push(format!("finger {finger:?}: {got:?}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
