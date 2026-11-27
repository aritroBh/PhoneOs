#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 38: break-it tests on the UIKit revealed-row rule of commit `341f0de`
//! (`revealed_rows`: a loose `Button`, a direct child of a `Table`/`CollectionView`, in a
//! cell's rows and width marks that cell revealed).
//!
//! The trees are the real P13h captures (`ios26_probes3/p13h-uikit-*`) with one change
//! each, written into the typed-tree text: a floating button added to the table at rest,
//! or decoy cells added before the revealed row.

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
/// first line holding `before` (a child of the table: 38 spaces of indentation).
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

/// Sound: the unchanged captures read as before (the revealed one in row 1's band only,
/// the rest capture nowhere), so the tests below measure only their one change.
#[test]
fn sound_the_unchanged_table_captures_read_as_before() {
    assert!(revealed_at(&parse(TABLE_REST)).is_empty());
    let at = revealed_at(&parse(TABLE_REVEALED));
    assert!(!at.is_empty());
    assert!(at.iter().all(|&y| (146.0..=204.0).contains(&y)), "{at:?}");
}

/// F5 (low, usability): a floating action button that an app adds to its `UITableView`
/// (`tableView.addSubview(composeButton)`, a common pattern for a "Compose" or "Scroll to
/// top" button) is a loose `Button`, a direct child of the `Table`, lying in a cell's rows
/// and width. Input: the P13h table **at rest** plus `Button "Compose" {{372, 404},
/// {56, 56}}` as the table's last child (bottom-right, over row 6). The row under it reads
/// as revealed (trailing side): every right-to-left drag across that row is refused with
/// `RowActionsShowing`, though nothing is revealed. Should: a loose button marks a row
/// revealed only when it looks like a row action (the height of that row, flush with the
/// row's trailing or leading edge), not any button floating over the list. On a phone the
/// agent cannot swipe that row (or any row the button covers) at all.
#[test]
fn f5_a_floating_button_over_a_table_reads_as_a_revealed_row() {
    let text = with_table_children(
        TABLE_REST,
        "Other(Other) \"Vertical scroll bar",
        &["Button \"Compose\" {{372, 404}, {56, 56}}".to_owned()],
    );
    let s = parse(&text);
    let at = revealed_at(&s);
    assert!(
        at.is_empty(),
        "a table at rest with a floating button reads as revealed at y = {at:?}"
    );
}

/// F6 (low): the loose-button reading looks for the button's cell among the first
/// `REVEAL_BANDS` (64) crossed cells only, in tree order. Input: the P13h **revealed**
/// table (row 1's Delete and Flag showing, the `Cell` unmoved) with 64 decoy cells
/// `Cell id=decoy.N {{0, 148.3}, {2, 53}}` inserted before row 1 (2 pt wide, in row 1's
/// rows). Row 1 is no longer read as revealed: a same-way (right-to-left) drag through it
/// is planned and bounded instead of refused. Should: decoys that cannot hold the button
/// are not counted against the bound (or past the bound the row reads as revealed, fail
/// closed, as the first reading does past `REVEAL_CHECKS`). Low: only the app drawing the
/// list can shape its tree like this, and the table captures did not delete at 0.45.
#[test]
fn f6_decoy_cells_hide_a_revealed_table_row() {
    let decoys: Vec<String> = (0..64)
        .map(|k| format!("Cell id=decoy.{k} {{{{0, 148.3}}, {{2, 53}}}}"))
        .collect();
    let text = with_table_children(TABLE_REVEALED, "Cell id=uikit.table.row.1 ", &decoys);
    let s = parse(&text);
    let at = revealed_at(&s);
    assert!(
        at.iter().any(|&y| (150.0..=200.0).contains(&y)),
        "row 1's actions are showing but no drag through its band is refused: {at:?}"
    );
}

/// Sound: the same decoys below the bound (63 of them) leave row 1 read as revealed.
#[test]
fn sound_fewer_decoy_cells_than_the_bound_keep_the_row_revealed() {
    let decoys: Vec<String> = (0..63)
        .map(|k| format!("Cell id=decoy.{k} {{{{0, 148.3}}, {{2, 53}}}}"))
        .collect();
    let text = with_table_children(TABLE_REVEALED, "Cell id=uikit.table.row.1 ", &decoys);
    let at = revealed_at(&parse(&text));
    assert!(at.iter().any(|&y| (150.0..=200.0).contains(&y)), "{at:?}");
}

/// Sound: a "Load more" footer button set as the table's footer view (a loose button
/// below the last cell, in no cell's rows) does not mark any row revealed.
#[test]
fn sound_a_footer_button_below_the_cells_is_not_a_revealed_row() {
    let text = with_table_children(
        TABLE_REST,
        "Other(Other) \"Vertical scroll bar",
        &["Button \"Load More\" {{0, 572.3}, {440, 44}}".to_owned()],
    );
    assert!(revealed_at(&parse(&text)).is_empty());
}
