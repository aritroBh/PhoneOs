#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 38: the loose-button reading of `revealed_rows` (F5: the button's whole
//! height inside one cell's rows; F6: past `REVEAL_BANDS` crossed cells, fail closed). The
//! reviewer's own tests are `pass38_breakit.rs`.

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use jarvis_observe::{compact_view, CompactView};
use jarvis_proto::{Direction, Point, Screen};
use jarvis_safety::gestures::{Gesture, GestureError};
use jarvis_safety::{planned_path, ScreenTree};

const TABLE_REST: &str =
    include_str!("fixtures/ios26_probes3/p13h-uikit-01-table-trailing-rest-tree.txt");
const TABLE_REVEALED: &str =
    include_str!("fixtures/ios26_probes3/p13h-uikit-02-table-trailing-revealed-tree.txt");
const P13H: [(&str, &str, bool); 16] = [
    (
        "01",
        include_str!("fixtures/ios26_probes3/p13h-uikit-01-table-trailing-rest-tree.txt"),
        false,
    ),
    (
        "02",
        include_str!("fixtures/ios26_probes3/p13h-uikit-02-table-trailing-revealed-tree.txt"),
        true,
    ),
    (
        "04",
        include_str!(
            "fixtures/ios26_probes3/p13h-uikit-04-table-trailing-revealed-runner-tree.txt"
        ),
        true,
    ),
    (
        "05",
        include_str!("fixtures/ios26_probes3/p13h-uikit-05-table-trailing-after-tap-tree.txt"),
        false,
    ),
    (
        "06",
        include_str!("fixtures/ios26_probes3/p13h-uikit-06-table-leading-rest-tree.txt"),
        false,
    ),
    (
        "07",
        include_str!("fixtures/ios26_probes3/p13h-uikit-07-table-leading-revealed-tree.txt"),
        true,
    ),
    (
        "09",
        include_str!("fixtures/ios26_probes3/p13h-uikit-09-table-leading-revealed-runner-tree.txt"),
        true,
    ),
    (
        "10",
        include_str!("fixtures/ios26_probes3/p13h-uikit-10-table-leading-after-tap-tree.txt"),
        false,
    ),
    (
        "11",
        include_str!("fixtures/ios26_probes3/p13h-uikit-11-collection-trailing-rest-tree.txt"),
        false,
    ),
    (
        "12",
        include_str!("fixtures/ios26_probes3/p13h-uikit-12-collection-trailing-revealed-tree.txt"),
        true,
    ),
    (
        "14",
        include_str!(
            "fixtures/ios26_probes3/p13h-uikit-14-collection-trailing-revealed-runner-tree.txt"
        ),
        true,
    ),
    (
        "15",
        include_str!("fixtures/ios26_probes3/p13h-uikit-15-collection-trailing-after-tap-tree.txt"),
        false,
    ),
    (
        "16",
        include_str!("fixtures/ios26_probes3/p13h-uikit-16-collection-leading-rest-tree.txt"),
        false,
    ),
    (
        "17",
        include_str!("fixtures/ios26_probes3/p13h-uikit-17-collection-leading-revealed-tree.txt"),
        true,
    ),
    (
        "19",
        include_str!(
            "fixtures/ios26_probes3/p13h-uikit-19-collection-leading-revealed-runner-tree.txt"
        ),
        true,
    ),
    (
        "20",
        include_str!("fixtures/ios26_probes3/p13h-uikit-20-collection-leading-after-tap-tree.txt"),
        false,
    ),
];

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

/// The capture's text with `lines` inserted as direct children of the table, before the
/// first line holding `before`.
fn with_table_children(text: &str, before: &str, lines: &[String]) -> String {
    let at = text.find(before).expect("anchor");
    let line_start = text[..at].rfind('\n').map_or(0, |k| k + 1);
    let pad = " ".repeat(38);
    let mut out = String::with_capacity(text.len() + lines.len() * 80);
    out.push_str(&text[..line_start]);
    for l in lines {
        out.push_str(&pad);
        out.push_str(l);
        out.push('\n');
    }
    out.push_str(&text[line_start..]);
    out
}

/// The heights (every 2 pt) at which a full-width sideways path is refused as crossing a
/// revealed row.
fn revealed_at(s: &Screen) -> Vec<f64> {
    let v = compact_view(s);
    let t = tree(&v);
    let size = v.screen.size;
    let mut out = Vec::new();
    let mut y = 0.0;
    while y <= size.height {
        for finger in [Direction::Left, Direction::Right] {
            let (a, b) = (size.width * 0.04, size.width * 0.96);
            let (from, to) = match finger {
                Direction::Left => (b, a),
                _ => (a, b),
            };
            let g = Gesture {
                from: Point::new(from, y),
                to: Point::new(to, y),
                duration_ms: 200,
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

/// Every P13h capture reads as before: the revealed ones somewhere, the others nowhere.
#[test]
fn every_p13h_capture_reads_as_before() {
    for (name, text, revealed) in P13H {
        let at = revealed_at(&parse(text));
        assert_eq!(!at.is_empty(), revealed, "{name}: {at:?}");
    }
}

/// F5 variants: floating buttons over the table at rest — a 44-pt one straddling two rows,
/// a 56-pt one, a tall one, one at the leading edge — read as no revealed row; a 44-pt one
/// inside one row's height still does (what the captures' actions look like).
#[test]
fn f5_floating_buttons_over_a_table_are_not_row_actions() {
    for b in [
        "Button \"Top\" {{372, 230}, {56, 44}}",
        "Button \"Compose\" {{372, 404}, {56, 56}}",
        "Button \"New\" {{20, 300}, {60, 60}}",
        "Button \"Scan\" {{360, 160}, {70, 120}}",
    ] {
        let text = with_table_children(
            TABLE_REST,
            "Other(Other) \"Vertical scroll bar",
            &[b.to_owned()],
        );
        let at = revealed_at(&parse(&text));
        assert!(at.is_empty(), "{b}: {at:?}");
    }
    // A button of an action's height inside one row reads as that row's action (fail
    // closed: a floating button of exactly that shape is not told apart).
    let text = with_table_children(
        TABLE_REST,
        "Other(Other) \"Vertical scroll bar",
        &["Button \"Delete\" {{366.5, 206.0}, {63.3, 44}}".to_owned()],
    );
    let at = revealed_at(&parse(&text));
    assert!(
        !at.is_empty() && at.iter().all(|&y| (199.0..=258.0).contains(&y)),
        "{at:?}"
    );
}

/// F6 variants: more decoy cells than `REVEAL_BANDS` in the revealed row's band make every
/// sideways drag through that band refused (fail closed); at rest (no button for the
/// search to read) the same decoys refuse nothing.
#[test]
fn f6_past_the_band_bound_a_sideways_drag_is_refused() {
    for n in [64usize, 65, 200] {
        let decoys: Vec<String> = (0..n)
            .map(|k| format!("Cell id=decoy.{k} {{{{0, 148.3}}, {{2, 53}}}}"))
            .collect();
        let text = with_table_children(TABLE_REVEALED, "Cell id=uikit.table.row.1 ", &decoys);
        let at = revealed_at(&parse(&text));
        assert!(
            at.iter().any(|&y| (150.0..=200.0).contains(&y)),
            "{n} decoys: {at:?}"
        );
        // Rows below the decoys' band read as the capture does.
        assert!(at.iter().all(|&y| y <= 204.0), "{n} decoys: {at:?}");
        let text = with_table_children(TABLE_REST, "Cell id=uikit.table.row.1 ", &decoys);
        assert!(revealed_at(&parse(&text)).is_empty(), "{n} decoys at rest");
    }
}
