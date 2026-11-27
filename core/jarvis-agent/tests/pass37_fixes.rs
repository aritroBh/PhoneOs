#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 37 (F9) and Mac run 170 (probe P13h): revealed rows.
//!
//! * F9: a row shifted sideways but still inside its list, with a buttons-only container
//!   covering only the revealed strip, reads as revealed when the cell is *displaced*: off
//!   the column lattice the other rows of its list establish (`revealed_rows`).
//! * P13h (iPhone 17 Pro Max simulator, iOS 26.5, 440 pt): a UIKit collection list
//!   (`UICollectionLayoutListConfiguration`, inset grouped) behaves like SwiftUI's (the
//!   `Cell` shifted −157 / +92 pt, its actions in a sibling `Other` at the row's rest frame);
//!   a plain `UITableView` does **not** move the `Cell` (only its content inside it), and
//!   its action `Button`s are loose direct children of the `Table`, after all the cells.
//!   Both are read as revealed: a same-way drag through the row's band is refused at any
//!   length; the closing way and the other rows keep the usual bound; the captures at rest
//!   and after the tap that closed the row read as nothing revealed.

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use jarvis_observe::{compact_view, CompactView};
use jarvis_proto::{Direction, Point, Rect, Role, Screen, Size, UiNode};
use jarvis_safety::gestures::{
    drag_gesture, finger_direction, Gesture, GestureError, GestureKind, ROW_SWIPE_SHARE,
};
use jarvis_safety::{planned_drag, planned_path, ScreenTree};

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("fixtures/ios26_probes3/", $name))
    };
}

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

fn length(g: &Gesture) -> f64 {
    (g.to.x - g.from.x).hypot(g.to.y - g.from.y)
}

fn opposite(d: Direction) -> Direction {
    match d {
        Direction::Left => Direction::Right,
        _ => Direction::Left,
    }
}

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

const SIDEWAYS: [(GestureKind, Direction); 4] = [
    (GestureKind::Swipe, Direction::Left),
    (GestureKind::Swipe, Direction::Right),
    (GestureKind::Scroll, Direction::Left),
    (GestureKind::Scroll, Direction::Right),
];

// ---------------------------------------------------------------------------------------
// P13h: the UIKit captures.
// ---------------------------------------------------------------------------------------

/// One revealed UIKit capture: the cell's identifier, the list's role, the way the row was
/// pulled, another row of the same list.
struct Uikit {
    name: &'static str,
    trees: &'static [&'static str],
    cell: &'static str,
    other: &'static str,
    list: Role,
    pulled: Direction,
}

const UIKIT: [Uikit; 4] = [
    Uikit {
        name: "UITableView, two trailing actions (the Cell does not move)",
        trees: &[
            fixture!("p13h-uikit-02-table-trailing-revealed-tree.txt"),
            fixture!("p13h-uikit-04-table-trailing-revealed-runner-tree.txt"),
        ],
        cell: "uikit.table.row.1",
        other: "uikit.table.row.3",
        list: Role::Table,
        pulled: Direction::Left,
    },
    Uikit {
        name: "UITableView, one leading action (the Cell does not move)",
        trees: &[
            fixture!("p13h-uikit-07-table-leading-revealed-tree.txt"),
            fixture!("p13h-uikit-09-table-leading-revealed-runner-tree.txt"),
        ],
        cell: "uikit.table.row.4",
        other: "uikit.table.row.2",
        list: Role::Table,
        pulled: Direction::Right,
    },
    Uikit {
        name: "UICollectionView list, two trailing actions (Cell shifted -157 pt)",
        trees: &[
            fixture!("p13h-uikit-12-collection-trailing-revealed-tree.txt"),
            fixture!("p13h-uikit-14-collection-trailing-revealed-runner-tree.txt"),
        ],
        cell: "uikit.collection.row.1",
        other: "uikit.collection.row.3",
        list: Role::CollectionView,
        pulled: Direction::Left,
    },
    Uikit {
        name: "UICollectionView list, one leading action (Cell shifted +92 pt)",
        trees: &[
            fixture!("p13h-uikit-17-collection-leading-revealed-tree.txt"),
            fixture!("p13h-uikit-19-collection-leading-revealed-runner-tree.txt"),
        ],
        cell: "uikit.collection.row.4",
        other: "uikit.collection.row.2",
        list: Role::CollectionView,
        pulled: Direction::Right,
    },
];

/// The captures at rest and after the tap that closed the row.
const UIKIT_QUIET: [&str; 8] = [
    fixture!("p13h-uikit-01-table-trailing-rest-tree.txt"),
    fixture!("p13h-uikit-05-table-trailing-after-tap-tree.txt"),
    fixture!("p13h-uikit-06-table-leading-rest-tree.txt"),
    fixture!("p13h-uikit-10-table-leading-after-tap-tree.txt"),
    fixture!("p13h-uikit-11-collection-trailing-rest-tree.txt"),
    fixture!("p13h-uikit-15-collection-trailing-after-tap-tree.txt"),
    fixture!("p13h-uikit-16-collection-leading-rest-tree.txt"),
    fixture!("p13h-uikit-20-collection-leading-after-tap-tree.txt"),
];

fn node_by_id<'a>(s: &'a Screen, id: &str) -> &'a UiNode {
    s.root
        .find(|n| n.identifier.as_deref() == Some(id))
        .unwrap_or_else(|| panic!("no node {id}"))
}

fn list_of(s: &Screen, role: &Role, at: Point) -> Rect {
    s.root
        .find(|n| &n.role == role && n.frame.contains(at))
        .expect("no list")
        .frame
}

/// The heights (every 2 pt) at which a full-width path reads as crossing a revealed row.
fn revealed_at(s: &Screen) -> Vec<f64> {
    let v = compact_view(s);
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

/// Every horizontal path through the revealed row's rows (from 10 to 400 pt, starting
/// anywhere) with the finger moving the way the row was pulled is refused; the other way
/// it is planned and bounded at 0.45 of the row; paths through another row of the same list
/// are planned and bounded either way.
#[test]
fn uikit_revealed_rows_refuse_the_same_way_drag_at_every_length() {
    let failed: Vec<String> = UIKIT
        .iter()
        .filter_map(|k| {
            std::panic::catch_unwind(|| check_uikit(k))
                .err()
                .map(|e| format!("{}: {:?}", k.name, e.downcast_ref::<String>()))
        })
        .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

fn check_uikit(k: &Uikit) {
    {
        for text in k.trees {
            let s = parse(text);
            let v = compact_view(&s);
            let t = tree(&v);
            let cell = node_by_id(&s, k.cell).frame;
            let other = node_by_id(&s, k.other).frame;
            let list = list_of(&s, &k.list, other.center());
            let width = v.screen.size.width;
            for y in [cell.y + 0.5, cell.center().y, cell.max_y() - 0.5] {
                let mut x = 0.0;
                while x <= width {
                    let mut len = 10.0;
                    while len <= 400.0 {
                        let g = path(x, y, len, k.pulled);
                        let (px0, px1) = (g.from.x.min(g.to.x), g.from.x.max(g.to.x));
                        if px0 <= list.max_x().max(cell.max_x()) && px1 >= list.x.min(cell.x) {
                            assert_eq!(
                                planned_path(g, k.pulled, &v.screen, Some(&t)),
                                Err(GestureError::RowActionsShowing),
                                "{}: from {x} at {y}, {len} pt",
                                k.name
                            );
                        }
                        if (17.6..=422.4).contains(&x) && x >= list.x && x <= list.max_x() {
                            let back = opposite(k.pulled);
                            let g = path(x, y, len, back);
                            let p = match planned_path(g, back, &v.screen, Some(&t)) {
                                Ok(p) => p,
                                // The table's loose action buttons are what the list holds
                                // directly: pass 34's narrow-row rule refuses a closing drag
                                // that crosses one (stricter, never longer).
                                Err(GestureError::RowTooNarrow)
                                    if crosses_loose_button(&v, k, g) =>
                                {
                                    len += 10.0;
                                    continue;
                                }
                                Err(e) => {
                                    panic!("{}: closing from {x}, {len} pt: {e:?}", k.name)
                                }
                            };
                            assert!(length(&p) <= len + 1e-6, "{}", k.name);
                            assert!(
                                length(&p) <= ROW_SWIPE_SHARE * cell.width.min(440.0) + 1e-6,
                                "{}: closing from {x}, {len} pt: {}",
                                k.name,
                                length(&p)
                            );
                        }
                        len += 10.0;
                    }
                    x += 20.0;
                }
            }
            // Another row of the same list: the usual bound, both ways.
            let y = other.center().y;
            for finger in [Direction::Left, Direction::Right] {
                for len in [40.0, 120.0, 300.0] {
                    let x = match finger {
                        Direction::Left => 400.0,
                        _ => 40.0,
                    };
                    let p = planned_path(path(x, y, len, finger), finger, &v.screen, Some(&t))
                        .unwrap_or_else(|e| panic!("{}: other row {finger:?}: {e:?}", k.name));
                    assert!(
                        length(&p) <= ROW_SWIPE_SHARE * other.width + 1e-6,
                        "{}: other row {finger:?} {len}: {}",
                        k.name,
                        length(&p)
                    );
                }
            }
            // The revealed actions, dragged as elements: refused the way the row was pulled.
            for b in v
                .screen
                .elements
                .iter()
                .filter(|e| ["Delete", "Flag", "Remove"].contains(&e.label.as_str()))
            {
                for (kind, d) in SIDEWAYS {
                    let got = planned_drag(kind, d, Some(b.frame), &v.screen, Some(&t));
                    if finger_direction(kind, d) == k.pulled {
                        assert_eq!(
                            got,
                            Err(GestureError::RowActionsShowing),
                            "{}: {} {kind:?} {d:?}",
                            k.name,
                            b.label
                        );
                    } else {
                        match got {
                            Ok(g) => assert!(length(&g) <= ROW_SWIPE_SHARE * 440.0 + 1e-6),
                            // A table's loose action button is a narrow row of its own
                            // (pass 34): a drag on it the closing way is refused.
                            Err(GestureError::RowTooNarrow) if k.list == Role::Table => {}
                            Err(e) => panic!("{}: {e:?}", k.name),
                        }
                    }
                }
            }
            // Only this row's band reads as revealed.
            let at = revealed_at(&s);
            assert!(!at.is_empty(), "{}", k.name);
            assert!(
                at.iter()
                    .all(|&y| y >= cell.y - 2.0 && y <= cell.max_y() + 2.0),
                "{}: {at:?} (row {cell:?})",
                k.name
            );
        }
    }
}

/// Whether `g` crosses a revealed action button of a table (a loose button: the list's
/// direct child).
fn crosses_loose_button(v: &CompactView, k: &Uikit, g: Gesture) -> bool {
    let (x0, x1) = (g.from.x.min(g.to.x), g.from.x.max(g.to.x));
    k.list == Role::Table
        && v.screen.elements.iter().any(|e| {
            ["Delete", "Flag", "Remove"].contains(&e.label.as_str())
                && e.frame.x <= x1
                && e.frame.max_x() >= x0
                && e.frame.y <= g.from.y
                && e.frame.max_y() >= g.from.y
        })
}

/// The captures at rest and after the closing tap read as nothing revealed, and every
/// drag the session plans on them is never refused as a revealed row.
#[test]
fn uikit_rows_at_rest_and_after_the_tap_are_not_revealed() {
    for (k, text) in UIKIT_QUIET.iter().enumerate() {
        let s = parse(text);
        assert!(
            revealed_at(&s).is_empty(),
            "capture {k}: {:?}",
            revealed_at(&s)
        );
        let v = compact_view(&s);
        let t = tree(&v);
        let frames = std::iter::once(None).chain(v.screen.elements.iter().map(|e| Some(e.frame)));
        for frame in frames {
            for (kind, d) in SIDEWAYS {
                assert_ne!(
                    planned_drag(kind, d, frame, &v.screen, Some(&t)),
                    Err(GestureError::RowActionsShowing),
                    "capture {k}"
                );
            }
        }
        let reach = drag_gesture(GestureKind::Swipe, Direction::Left, None, &v.screen);
        assert!(reach.is_ok(), "capture {k}");
    }
}

// ---------------------------------------------------------------------------------------
// F9: displaced cells inside their list (synthetic, the shapes of `pass37_breakit.rs`).
// ---------------------------------------------------------------------------------------

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

fn cell(x: f64, y: f64, w: f64, name: &str) -> UiNode {
    node(
        Role::Cell,
        None,
        Rect::new(x, y, w, 52.0),
        vec![node(
            Role::StaticText,
            Some(name),
            Rect::new(x + 10.0, y + 16.0, 100.0, 20.0),
            Vec::new(),
        )],
    )
}

fn buttons(container: Rect, labels: &[&str], trailing: bool) -> UiNode {
    let n = labels.len();
    // Buttons inside the container, hugging its trailing (or leading) edge.
    let w = ((container.width - 5.0 * n as f64) / n as f64).min(80.0);
    let kids = labels
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let x = if trailing {
                container.max_x() - (w + 5.0) * (n - i) as f64 + 5.0
            } else {
                container.x + 5.0 + (w + 5.0) * i as f64
            };
            node(
                Role::Button,
                Some(l),
                Rect::new(x, container.y + 4.0, w, 44.0),
                Vec::new(),
            )
        })
        .collect();
    node(Role::Other("Other".to_owned()), None, container, kids)
}

fn screen_of(list: UiNode) -> Screen {
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
        ..parse(fixture!(
            "p13f-revealed-tree-02-revealed-inset-one-row-tree.txt"
        ))
    }
}

fn refuses(s: &Screen, from: Point, finger: Direction) -> bool {
    drag(s, from, finger) == Err(GestureError::RowActionsShowing)
}

fn drag(s: &Screen, from: Point, finger: Direction) -> Result<Gesture, GestureError> {
    let v = compact_view(s);
    let t = tree(&v);
    let to = match finger {
        Direction::Left => Point::new(from.x - 100.0, from.y),
        _ => Point::new(from.x + 100.0, from.y),
    };
    planned_path(
        Gesture {
            from,
            to,
            duration_ms: 200,
        },
        finger,
        &v.screen,
        Some(&t),
    )
}

/// F9 variants the reviewer did not write: the left column's row pulled right (leading
/// actions in the strip it vacated), a three-column grid's middle row pulled left, and a
/// shift of only 60 pt: each refused the way it was pulled, the other way planned.
#[test]
fn f9_displaced_rows_inside_their_list_read_as_revealed() {
    // Left column pulled right by 100 pt, leading "Pin" in the strip 16..116.
    let y = 300.0;
    let list = node(
        Role::CollectionView,
        None,
        Rect::new(0.0, 250.0, 440.0, 600.0),
        vec![
            cell(116.0, y, 200.0, "Left 1"),
            buttons(Rect::new(16.0, y, 100.0, 52.0), &["Pin"], false),
            cell(224.0, y, 200.0, "Right 1"),
            cell(16.0, y + 60.0, 200.0, "Left 2"),
            cell(224.0, y + 60.0, 200.0, "Right 2"),
        ],
    );
    let s = screen_of(list);
    assert!(refuses(&s, Point::new(60.0, y + 26.0), Direction::Right));
    assert!(!refuses(&s, Point::new(300.0, y + 26.0), Direction::Left));
    // Three columns of 130 pt (16, 155, 294); the middle one pulled left 60 pt.
    let list = node(
        Role::CollectionView,
        None,
        Rect::new(0.0, 250.0, 440.0, 600.0),
        vec![
            cell(16.0, y, 130.0, "A1"),
            cell(95.0, y, 130.0, "B1"),
            buttons(Rect::new(225.0, y, 60.0, 52.0), &["X"], true),
            cell(294.0, y, 130.0, "C1"),
            cell(16.0, y + 60.0, 130.0, "A2"),
            cell(155.0, y + 60.0, 130.0, "B2"),
            cell(294.0, y + 60.0, 130.0, "C2"),
        ],
    );
    let s = screen_of(list);
    let got = drag(&s, Point::new(280.0, y + 26.0), Direction::Left);
    assert_eq!(got, Err(GestureError::RowActionsShowing));
}

/// Sound: what F9's rule must leave alone. A carousel (one row of cards, no other rows) and
/// one scrolled partway (every card moved together, off the screen's lattice but on its
/// own) with a "See All" footer beside the last card; a two-column grid at rest with a
/// footer button below it; every cell of every row moved by the same amount.
#[test]
fn sound_carousels_and_grids_at_rest_are_not_displaced() {
    let y = 300.0;
    for offset in [0.0, -37.0, -120.0] {
        let mut kids: Vec<UiNode> = (0..4)
            .map(|i| cell(16.0 + offset + 170.0 * f64::from(i), y, 160.0, "Card"))
            .collect();
        kids.push(buttons(
            Rect::new(16.0 + offset + 680.0, y, 90.0, 52.0),
            &["See All"],
            false,
        ));
        let list = node(
            Role::CollectionView,
            None,
            Rect::new(0.0, 250.0, 440.0, 120.0),
            kids,
        );
        let s = screen_of(list);
        for finger in [Direction::Left, Direction::Right] {
            for x in [60.0, 220.0, 380.0] {
                assert!(
                    !refuses(&s, Point::new(x, y + 26.0), finger),
                    "offset {offset} {finger:?} {x}"
                );
            }
        }
    }
    // A grid scrolled sideways as a whole (every row moved by 30 pt) with a footer.
    let list = node(
        Role::CollectionView,
        None,
        Rect::new(0.0, 250.0, 440.0, 600.0),
        vec![
            cell(46.0, y, 180.0, "L1"),
            cell(236.0, y, 180.0, "R1"),
            cell(46.0, y + 60.0, 180.0, "L2"),
            buttons(Rect::new(236.0, y + 60.0, 90.0, 52.0), &["More"], false),
        ],
    );
    let s = screen_of(list);
    for finger in [Direction::Left, Direction::Right] {
        for x in [60.0, 220.0, 380.0] {
            for row in [y + 26.0, y + 86.0] {
                assert!(!refuses(&s, Point::new(x, row), finger), "{finger:?} {x}");
            }
        }
    }
}

/// `compact_text` is used by the shared parser.
#[test]
fn the_parser_reads_the_p13h_captures() {
    let s = parse(fixture!("p13h-uikit-02-table-trailing-revealed-tree.txt"));
    let v = compact_view(&s);
    assert!(compact_text(&v.screen).contains("Delete"));
}
