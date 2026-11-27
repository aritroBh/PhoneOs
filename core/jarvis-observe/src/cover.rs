//! Which listed controls a tap would be refused for because something covers them.
//!
//! The decision is `jarvis_safety::tap_spot`'s (the one every tap, long press, `type` and
//! `clear` is planned with): a control is covered exactly when `tap_spot` finds no free spot
//! for it (`Err`), with the default preference and the screen as the session holds it (the
//! typed tree: its bars count). Nothing here approximates it; this module only makes the
//! call cheap enough to make for every control once per observation.
//!
//! **Cost.** `tap_spot` reads every drawn node once for its first candidate point, so a
//! call per control would be quadratic (the "sweep" of the architecture notes). The result
//! does not depend on a drawn node whose frame does not reach the target's frame (every
//! candidate point is inside it, and a node contests only a point it contains), so each
//! call is given the drawn nodes that reach the control's frame, found through a uniform
//! grid over the screen, in node order, plus the nodes too large to index (they reach
//! nearly every frame anyway). Two things read a drawn node by something other than its
//! frame: the ancestors of the target's node, when it sits inside a system bar (bar
//! precedence looks the scroll views above a contesting node up), so a control inside a
//! bar is judged against all drawn nodes; and an empty drawn list, which `tap_spot` takes as
//! "no tree", so a lone far-away node stands in when no node reaches the frame.

use jarvis_proto::{BarNode, CompactScreen, CoverKind, CoveredControl, Element, RawNode, Rect};
use jarvis_safety::{takes_taps, tap_spot, Bar, CoveredBy, ScreenTree};

/// Grid columns and rows over the screen.
const COLS: usize = 16;
const ROWS: usize = 32;
/// A node reaching more cells than this is kept out of the grid and offered to every query.
const MAX_CELLS: usize = 128;
/// Candidate points a refused control is tried at (the default point and the 5 x 5 grid).
const TAP_CANDIDATES: usize = 26;
/// Most node comparisons one screen's cover pass may spend (a few milliseconds in a release
/// build; real screens use a few percent of it, a page of 20,000 clear rows about 10 %).
const WORK_BUDGET: usize = 1_000_000;

/// Inclusive grid columns and rows a frame reaches.
type Span = ((usize, usize), (usize, usize));

/// A uniform grid of drawn-node positions by cell (compressed: `starts[c]..starts[c + 1]`
/// of `items` are cell `c`'s). Only nodes that can contest a point are indexed ([`evidence`]).
struct Grid {
    w: f64,
    h: f64,
    starts: Vec<u32>,
    items: Vec<u32>,
    big: Vec<u32>,
}

/// Whether a drawn node can contest a point at all: it is a control, part of the keyboard
/// or shows text (`jarvis_safety`'s `hit` module docs: nothing else ever contests).
fn evidence(n: &RawNode) -> bool {
    takes_taps(&n.role) || n.keyboard || n.text
}

impl Grid {
    fn build(nodes: &[RawNode], w: f64, h: f64) -> Grid {
        let mut g = Grid {
            w,
            h,
            starts: vec![0; COLS * ROWS + 1],
            items: Vec::new(),
            big: Vec::new(),
        };
        // Pass 1 counts per cell, pass 2 fills.
        let mut spans: Vec<Option<Span>> = Vec::with_capacity(nodes.len());
        for (i, n) in nodes.iter().enumerate() {
            if !evidence(n) {
                spans.push(None);
                continue;
            }
            let at = u32::try_from(i).unwrap_or(u32::MAX);
            let sp @ ((c0, c1), (r0, r1)) = g.span(n.frame);
            if (c1 - c0 + 1) * (r1 - r0 + 1) > MAX_CELLS {
                g.big.push(at);
                spans.push(None);
                continue;
            }
            for r in r0..=r1 {
                for c in c0..=c1 {
                    if let Some(k) = g.starts.get_mut(r * COLS + c + 1) {
                        *k += 1;
                    }
                }
            }
            spans.push(Some(sp));
        }
        for c in 0..COLS * ROWS {
            let before = g.starts.get(c).copied().unwrap_or(0);
            if let Some(k) = g.starts.get_mut(c + 1) {
                *k += before;
            }
        }
        let total = g.starts.last().copied().unwrap_or(0) as usize;
        g.items = vec![0; total];
        let mut fill: Vec<u32> = g.starts.iter().take(COLS * ROWS).copied().collect();
        for (i, sp) in spans.iter().enumerate() {
            let Some(((c0, c1), (r0, r1))) = sp else {
                continue;
            };
            for r in *r0..=*r1 {
                for c in *c0..=*c1 {
                    if let Some(k) = fill.get_mut(r * COLS + c) {
                        if let Some(slot) = g.items.get_mut(*k as usize) {
                            *slot = u32::try_from(i).unwrap_or(u32::MAX);
                        }
                        *k += 1;
                    }
                }
            }
        }
        g
    }

    /// Cell columns and rows (inclusive, clamped) a frame reaches.
    fn span(&self, f: Rect) -> Span {
        let cell = |v: f64, size: f64, n: usize| -> usize {
            let top = (n - 1) as f64;
            // Float to int saturates; NaN is 0.
            ((v / size * n as f64).floor().clamp(0.0, top)) as usize
        };
        (
            (
                cell(f.x, self.w, COLS),
                cell(f.max_x(), self.w, COLS).max(cell(f.x, self.w, COLS)),
            ),
            (
                cell(f.y, self.h, ROWS),
                cell(f.max_y(), self.h, ROWS).max(cell(f.y, self.h, ROWS)),
            ),
        )
    }

    /// Positions (ascending, unique) of the nodes that could reach `f`.
    fn near(&self, f: Rect, out: &mut Vec<u32>) {
        out.clear();
        out.extend_from_slice(&self.big);
        let ((c0, c1), (r0, r1)) = self.span(f);
        for r in r0..=r1 {
            for c in c0..=c1 {
                let at = r * COLS + c;
                let (from, to) = (
                    self.starts.get(at).copied().unwrap_or(0) as usize,
                    self.starts.get(at + 1).copied().unwrap_or(0) as usize,
                );
                if let Some(cell) = self.items.get(from..to) {
                    out.extend_from_slice(cell);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
    }
}

fn reaches(n: &RawNode, f: &Rect) -> bool {
    let a = n.frame;
    !(a.x >= f.max_x() || a.max_x() <= f.x || a.y >= f.max_y() || a.max_y() <= f.y)
}

/// The kind a refusal's cover is shown as.
fn kind_of(by: CoveredBy) -> CoverKind {
    match by {
        CoveredBy::Bar(Bar::Navigation) => CoverKind::NavigationBar,
        CoveredBy::Bar(Bar::Tab) => CoverKind::TabBar,
        CoveredBy::Bar(Bar::Tool) => CoverKind::Toolbar,
        CoveredBy::Keyboard => CoverKind::Keyboard,
        CoveredBy::Control(_) | CoveredBy::Node(_) | CoveredBy::Inner(_) => CoverKind::Other,
    }
}

/// The raw tree's shape of a compact screen (the parts of `CompactView` a tap is planned
/// with).
pub(crate) struct Shape<'a> {
    pub sources: &'a [u32],
    pub parents: &'a [u32],
    pub ends: &'a [u32],
    pub bars: &'a [BarNode],
    pub nodes: &'a [RawNode],
}

/// The listed controls ([`takes_taps`]) of `screen` a tap would be refused for, by
/// ascending index, with what covers each.
pub(crate) fn covered_controls(screen: &CompactScreen, shape: &Shape<'_>) -> Vec<CoveredControl> {
    let full = ScreenTree {
        sources: shape.sources,
        parents: shape.parents,
        ends: shape.ends,
        bars: shape.bars,
        nodes: shape.nodes,
        inputs: &[],
    };
    let grid = Grid::build(shape.nodes, screen.size.width, screen.size.height);
    let indexed = screen.size.width > 0.0 && screen.size.height > 0.0;
    let mut out = Vec::new();
    let mut work = 0usize;
    let mut near: Vec<u32> = Vec::new();
    let mut subset: Vec<RawNode> = Vec::new();
    for (pos, e) in screen.elements.iter().enumerate() {
        // Probe run 134: an element with no known position is refused as such, never as
        // covered (one marker, `cues.unplaced`).
        if !takes_taps(&e.role) || screen.cues.is_unplaced(e.index) {
            continue;
        }
        let own = shape.sources.get(pos).copied();
        let in_bar = own.is_some_and(|s| {
            shape
                .bars
                .iter()
                .any(|b| s >= b.node && s < end_of(shape, b.node))
        });
        if work >= WORK_BUDGET {
            // A pathological screen (thousands of controls stacked on each other): the
            // rest are left unmarked, which only means the model may try the tap once.
            break;
        }
        let mut seen = shape.nodes.len();
        let spot = if in_bar || !indexed || shape.nodes.is_empty() {
            tap_spot(e, screen, Some(&full), None)
        } else {
            near_nodes(&grid, shape, e, own, &mut near, &mut subset);
            let tree = ScreenTree {
                sources: shape.sources,
                parents: shape.parents,
                ends: shape.ends,
                bars: shape.bars,
                nodes: &subset,
                inputs: &[],
            };
            seen = subset.len() + 1;
            tap_spot(e, screen, Some(&tree), None)
        };
        // A refusal tried every candidate point against every node seen.
        work += if spot.is_err() {
            seen * TAP_CANDIDATES
        } else {
            seen
        };
        if let Err(c) = spot {
            out.push(CoveredControl {
                index: e.index,
                by: kind_of(c.by),
            });
        }
    }
    out.sort_by_key(|c| c.index);
    out.dedup_by_key(|c| c.index);
    out
}

fn end_of(shape: &Shape<'_>, node: u32) -> u32 {
    usize::try_from(node)
        .ok()
        .and_then(|i| shape.ends.get(i))
        .copied()
        .unwrap_or(0)
}

/// Fills `subset` with the drawn nodes that reach `e`'s frame, in node order, leaving out
/// the target's own node and its ancestors (they never contest a point of the target, so a
/// deeply nested page does not hand every control its whole chain of wrappers); the first
/// drawn node alone when none is left (an empty list would read as "no tree").
fn near_nodes(
    grid: &Grid,
    shape: &Shape<'_>,
    e: &Element,
    own: Option<u32>,
    near: &mut Vec<u32>,
    subset: &mut Vec<RawNode>,
) {
    grid.near(e.frame, near);
    subset.clear();
    for &i in near.iter() {
        let Some(n) = usize::try_from(i).ok().and_then(|i| shape.nodes.get(i)) else {
            continue;
        };
        if !reaches(n, &e.frame) {
            continue;
        }
        // Nodes that cannot contest a point of the target (module docs of
        // `jarvis_safety`'s `hit`: only controls, the keyboard and labelled content do; the
        // target's own node, its ancestors and the non-control content inside it never do),
        // are not copied: most of a crowded page is such nodes.
        let (inside, holds) = own.map_or((false, false), |t| {
            (
                n.node > t && n.node < end_of(shape, t),
                n.node <= t && t < end_of(shape, n.node),
            )
        });
        if holds || (inside && !takes_taps(&n.role)) {
            continue;
        }
        subset.push(n.clone());
    }
    if subset.is_empty() {
        subset.extend(shape.nodes.first().cloned());
    }
}
