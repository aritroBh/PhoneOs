//! Flattening the accessibility tree into the indexed element list.

use std::collections::{BTreeMap, BTreeSet};
use std::ptr;
use std::rc::Rc;

use jarvis_proto::{CompactScreen, Element, Rect, Role, Screen, UiNode};

use crate::fingerprint::{identity_key, FingerprintAssigner};

/// Trees deeper than this are cut (real iOS trees stay far below).
pub(crate) const MAX_DEPTH: usize = 160;
/// Vertical jitter (points) tolerated between elements of the same visual row.
const ROW_TOLERANCE: f64 = 6.0;
/// An element must keep this share of its area visible...
const MIN_VISIBLE_FRACTION: f64 = 0.3;
/// ...or at least this many points in both directions.
const MIN_VISIBLE_SIDE: f64 = 24.0;
/// At most this many child texts are merged into one label.
const MAX_MERGED_PARTS: usize = 12;

/// An element before filtering by geometry.
struct Cand {
    role: Role,
    label: String,
    value: Option<String>,
    placeholder: Option<String>,
    frame: Rect,
    enabled: bool,
    focused: bool,
    selected: bool,
    secure: bool,
    fingerprint: u64,
    /// Nodes sharing a group are siblings or cell-mates (used to drop duplicate texts).
    group: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Skip,
    Container,
    Input,
    Control,
    Text,
    Image,
    Mergeable,
    Leaf,
}

fn kind(node: &UiNode) -> Kind {
    match &node.role {
        Role::Keyboard | Role::Key => Kind::Skip,
        r if r.is_text_input() => Kind::Input,
        Role::Button | Role::Link | Role::Switch | Role::Slider | Role::Picker | Role::Tab => {
            Kind::Control
        }
        Role::StaticText => Kind::Text,
        Role::Image => Kind::Image,
        Role::Cell => Kind::Mergeable,
        Role::Other(_) if node.children.is_empty() => Kind::Leaf,
        _ => Kind::Container,
    }
}

/// Collapses runs of whitespace (including newlines) into single spaces.
fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clean_opt(s: &Option<String>) -> String {
    s.as_deref().map(clean).unwrap_or_default()
}

fn non_empty(s: &Option<String>) -> Option<String> {
    let c = clean_opt(s);
    (!c.is_empty()).then_some(c)
}

/// Indices of `frames` in reading order: rows top to bottom (tolerating jitter), left to
/// right within a row. An element joins the current row when its top edge or its vertical
/// centre is within [`ROW_TOLERANCE`] of the row's first element. Ties keep original order.
fn reading_order(frames: &[Rect]) -> Vec<usize> {
    let top = |i: usize| frames.get(i).map_or(0.0, |f| f.y);
    let cy = |i: usize| frames.get(i).map_or(0.0, |f| f.center().y);
    let x = |i: usize| frames.get(i).map_or(0.0, |f| f.x);
    let mut idx: Vec<usize> = (0..frames.len()).collect();
    idx.sort_by(|&a, &b| {
        top(a)
            .total_cmp(&top(b))
            .then(x(a).total_cmp(&x(b)))
            .then(a.cmp(&b))
    });
    let mut out = Vec::with_capacity(idx.len());
    let mut row: Vec<usize> = Vec::new();
    for i in idx {
        let same_row = row.first().is_some_and(|&a| {
            (top(i) - top(a)).abs() <= ROW_TOLERANCE || (cy(i) - cy(a)).abs() <= ROW_TOLERANCE
        });
        if !row.is_empty() && !same_row {
            flush_row(&mut row, &mut out, x);
        }
        row.push(i);
    }
    flush_row(&mut row, &mut out, x);
    out
}

fn flush_row(row: &mut Vec<usize>, out: &mut Vec<usize>, x: impl Fn(usize) -> f64) {
    row.sort_by(|&a, &b| x(a).total_cmp(&x(b)).then(a.cmp(&b)));
    out.append(row);
}

fn is_live(node: &UiNode) -> bool {
    node.visible && kind(node) != Kind::Skip
}

fn is_interactive_kind(node: &UiNode) -> bool {
    matches!(kind(node), Kind::Control | Kind::Input)
}

/// Where a node's subtree lies in the flat pre-order lists of [`Summaries`].
#[derive(Clone, Copy, Default)]
struct Span {
    t0: usize,
    t1: usize,
    g0: usize,
    g1: usize,
    i0: usize,
    i1: usize,
}

/// Per-subtree summaries computed in one pass: texts, labelled images and top-level
/// interactive nodes, in document order, plus each cell/control's range into them.
struct Summaries<'a> {
    texts: Vec<(Rect, String)>,
    images: Vec<(Rect, String)>,
    interactive: Vec<&'a UiNode>,
    spans: BTreeMap<usize, Span>,
}

fn addr(node: &UiNode) -> usize {
    node as *const UiNode as usize
}

impl<'a> Summaries<'a> {
    fn build(root: &'a UiNode) -> Self {
        let mut s = Summaries {
            texts: Vec::new(),
            images: Vec::new(),
            interactive: Vec::new(),
            spans: BTreeMap::new(),
        };
        // (node, depth, inside an interactive node, span start, is exit marker)
        let mut stack: Vec<(&'a UiNode, usize, bool, Span, bool)> =
            vec![(root, 0, false, Span::default(), false)];
        while let Some((node, depth, in_inter, start, exit)) = stack.pop() {
            if exit {
                let k = kind(node);
                if matches!(k, Kind::Control | Kind::Mergeable) {
                    s.spans.insert(
                        addr(node),
                        Span {
                            t1: s.texts.len(),
                            g1: s.images.len(),
                            i1: s.interactive.len(),
                            ..start
                        },
                    );
                }
                continue;
            }
            if !is_live(node) || depth > MAX_DEPTH {
                continue;
            }
            let label = non_empty(&node.label).or_else(|| non_empty(&node.value));
            match (&node.role, label) {
                (Role::StaticText, Some(l)) => s.texts.push((node.frame, l)),
                (Role::Image, Some(l)) => s.images.push((node.frame, l)),
                _ => {}
            }
            let inter = is_interactive_kind(node);
            if inter && !in_inter {
                s.interactive.push(node);
            }
            let start = Span {
                t0: s.texts.len(),
                g0: s.images.len(),
                i0: s.interactive.len(),
                ..Span::default()
            };
            stack.push((node, depth, in_inter, start, true));
            for c in node.children.iter().rev() {
                stack.push((c, depth + 1, in_inter || inter, Span::default(), false));
            }
        }
        s
    }

    fn span(&self, node: &UiNode) -> Span {
        self.spans.get(&addr(node)).copied().unwrap_or_default()
    }

    /// Count of top-level interactive descendants and the first one.
    fn interactive_in(&self, sp: Span) -> (usize, Option<&'a UiNode>) {
        let count = sp.i1.saturating_sub(sp.i0);
        (
            count,
            self.interactive.get(sp.i0).copied().filter(|_| count > 0),
        )
    }

    /// Texts in the span in reading order, joined; falls back to labelled images. At most
    /// [`MAX_MERGED_PARTS`] parts, then a trailing `…` marks that more text exists.
    fn merged_text(&self, sp: Span) -> String {
        let texts = self.texts.get(sp.t0..sp.t1).unwrap_or_default();
        let parts = if texts.is_empty() {
            self.images.get(sp.g0..sp.g1).unwrap_or_default()
        } else {
            texts
        };
        let frames: Vec<Rect> = parts.iter().map(|(f, _)| *f).collect();
        let mut ordered: Vec<&str> = Vec::new();
        for i in reading_order(&frames) {
            if let Some((_, t)) = parts.get(i) {
                if ordered.last() != Some(&t.as_str()) {
                    ordered.push(t);
                }
            }
        }
        let more = ordered.len() > MAX_MERGED_PARTS;
        ordered.truncate(MAX_MERGED_PARTS);
        let mut out = ordered.join(", ");
        if more {
            out.push_str(", …");
        }
        out
    }
}

#[derive(Clone)]
struct State {
    depth: usize,
    emit: bool,
    group: usize,
    /// Keep `group` for descendants (inside a cell that holds controls).
    sticky: bool,
    /// Static texts below were folded into an unlabeled control's label.
    skip_text: bool,
    inherit: Option<String>,
    /// Nearest enclosing row/container label, folded into fingerprints.
    context: Rc<str>,
}

struct Walker<'a> {
    sums: Summaries<'a>,
    scope: Option<&'a UiNode>,
    cands: Vec<Cand>,
    ordinals: FingerprintAssigner,
    path: Vec<&'a Role>,
    next_id: usize,
}

impl<'a> Walker<'a> {
    fn walk(&mut self, node: &'a UiNode, st: &State) {
        if !node.visible || st.depth > MAX_DEPTH {
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        let emit = st.emit || self.scope.is_some_and(|s| ptr::eq(s, node));
        let st = State { emit, ..st.clone() };
        match kind(node) {
            Kind::Skip => {}
            Kind::Container => {
                let mut inner = st.clone();
                if !matches!(node.role, Role::Application | Role::Window) {
                    if let Some(l) = non_empty(&node.label) {
                        inner.context = Rc::from(l.as_str());
                    }
                }
                self.descend(node, id, &inner);
            }
            Kind::Input => {
                let label = non_empty(&node.label)
                    .or_else(|| non_empty(&node.placeholder))
                    .or_else(|| st.inherit.clone())
                    .or_else(|| non_empty(&node.identifier))
                    .unwrap_or_default();
                self.push(node, label, &st);
            }
            Kind::Control => {
                let label = non_empty(&node.label)
                    .or_else(|| {
                        Some(self.sums.merged_text(self.sums.span(node))).filter(|t| !t.is_empty())
                    })
                    .or_else(|| st.inherit.clone())
                    .or_else(|| non_empty(&node.identifier))
                    .unwrap_or_default();
                self.push(node, label, &st);
            }
            Kind::Text => {
                if let Some(label) = non_empty(&node.label).or_else(|| non_empty(&node.value)) {
                    if !st.skip_text {
                        self.push(node, label, &st);
                    }
                }
            }
            Kind::Image | Kind::Leaf => {
                if let Some(label) = non_empty(&node.label) {
                    self.push(node, label, &st);
                }
            }
            Kind::Mergeable => self.mergeable(node, id, &st),
        }
    }

    fn mergeable(&mut self, node: &'a UiNode, id: usize, st: &State) {
        let sp = self.sums.span(node);
        let (count, first) = self.sums.interactive_in(sp);
        let is_cell = node.role == Role::Cell;
        if count == 0 {
            let label = non_empty(&node.label).unwrap_or_else(|| self.sums.merged_text(sp));
            let label = if label.is_empty() {
                non_empty(&node.identifier).unwrap_or_default()
            } else {
                label
            };
            if label.is_empty() && !is_cell {
                self.descend(node, id, st);
            } else if !label.is_empty() {
                self.push(node, label, st);
            }
            return;
        }
        // A row holding controls is not itself the target: list the controls, and lend the
        // row's text to a single unlabeled control.
        let merged = self.sums.merged_text(sp);
        let unlabeled = first.is_some_and(|f| {
            non_empty(&f.label).is_none()
                && (!f.role.is_text_input() || non_empty(&f.placeholder).is_none())
        });
        let lend = count == 1 && unlabeled && !merged.is_empty();
        let inner = State {
            sticky: true,
            group: id,
            context: if let Some(l) = non_empty(&node.label) {
                Rc::from(l.as_str())
            } else if merged.is_empty() {
                st.context.clone()
            } else {
                Rc::from(merged.as_str())
            },
            skip_text: st.skip_text || lend,
            inherit: if lend {
                Some(merged)
            } else {
                st.inherit.clone()
            },
            ..st.clone()
        };
        self.descend(node, id, &inner);
    }

    fn descend(&mut self, node: &'a UiNode, id: usize, st: &State) {
        self.path.push(&node.role);
        let child = State {
            depth: st.depth + 1,
            group: if st.sticky { st.group } else { id },
            ..st.clone()
        };
        for c in &node.children {
            self.walk(c, &child);
        }
        self.path.pop();
    }

    fn push(&mut self, node: &UiNode, label: String, st: &State) {
        if !st.emit {
            return;
        }
        let mut key = identity_key(node.identifier.as_deref().filter(|s| !s.is_empty()), &label);
        if !st.context.is_empty() {
            key.push_str("\u{1}c:");
            key.push_str(&st.context);
        }
        let fingerprint = self.ordinals.assign(&node.role, &key, &self.path);
        let secure = node.secure || node.role == Role::SecureTextField;
        let placeholder = non_empty(&node.placeholder);
        let value = if secure || node.role == Role::StaticText {
            None
        } else {
            non_empty(&node.value).filter(|v| placeholder.as_ref() != Some(v))
        };
        self.cands.push(Cand {
            role: node.role.clone(),
            label,
            value,
            placeholder: if node.role.is_text_input() {
                placeholder
            } else {
                None
            },
            frame: node.frame,
            enabled: node.enabled,
            focused: node.focused,
            selected: node.selected,
            secure,
            fingerprint,
            group: st.group,
        });
    }
}

/// The topmost visible Alert/Sheet (last in document order) and the first visible keyboard.
fn find_overlays(root: &UiNode) -> (Option<&UiNode>, Option<Rect>) {
    let mut alert = None;
    let mut keyboard = None;
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if !n.visible {
            continue;
        }
        match n.role {
            Role::Alert | Role::Sheet if !n.frame.is_empty() => alert = Some(n),
            Role::Keyboard if keyboard.is_none() && !n.frame.is_empty() => {
                keyboard = Some(n.frame);
            }
            _ => {}
        }
        stack.extend(n.children.iter().rev());
    }
    (alert, keyboard)
}

/// Clips `frame` to the screen and above the keyboard; `None` when too little remains visible.
fn visible_frame(frame: Rect, bounds: Rect, keyboard: Option<Rect>) -> Option<Rect> {
    if frame.is_empty() {
        return None;
    }
    let mut vis = if bounds.is_empty() {
        frame
    } else {
        frame.intersection(&bounds)?
    };
    if let Some(kb) = keyboard {
        if kb.intersects(&vis) {
            if vis.y >= kb.y {
                return None;
            }
            vis.height = kb.y - vis.y;
        }
    }
    let fraction = vis.area() / frame.area();
    let big_enough = vis.width >= MIN_VISIBLE_SIDE && vis.height >= MIN_VISIBLE_SIDE;
    (!vis.is_empty() && (fraction >= MIN_VISIBLE_FRACTION || big_enough)).then_some(vis)
}

fn round(f: &Rect) -> [i64; 4] {
    // Float to int saturates; frames are small screen coordinates.
    [f.x, f.y, f.width, f.height].map(|v| v.round() as i64)
}

/// Flattens a screen to its indexed, ordered, deduplicated actionable/informative elements.
///
/// * Invisible, zero-area, off-screen (or mostly off-screen) and keyboard-covered nodes are
///   dropped; partly visible ones keep the visible part of their frame.
/// * While an Alert/Sheet is up only its subtree is listed.
/// * Keyboard keys are never listed; `keyboard_visible` is set instead.
/// * Cells (and hittable `Other` rows) take their label from child static texts, and those
///   texts are not listed again. A row holding controls is not listed; its controls are.
/// * Secure fields never carry a value; a value equal to the placeholder is dropped.
pub fn compact(screen: &Screen) -> CompactScreen {
    let (alert, keyboard) = find_overlays(&screen.root);
    let mut walker = Walker {
        sums: Summaries::build(&screen.root),
        scope: alert,
        cands: Vec::new(),
        ordinals: FingerprintAssigner::default(),
        path: Vec::new(),
        next_id: 0,
    };
    let start = State {
        depth: 0,
        emit: alert.is_none(),
        group: 0,
        sticky: false,
        skip_text: false,
        inherit: None,
        context: Rc::from(""),
    };
    walker.walk(&screen.root, &start);

    let bounds = screen.bounds();
    let occluder = if alert.is_some() { None } else { keyboard };
    let mut kept: Vec<(Cand, Rect)> = walker
        .cands
        .into_iter()
        .filter_map(|c| visible_frame(c.frame, bounds, occluder).map(|f| (c, f)))
        .collect();

    // Texts duplicating a control's label inside the same group are redundant.
    let control_labels: BTreeSet<(usize, String)> = kept
        .iter()
        .filter(|(c, _)| c.role != Role::StaticText)
        .map(|(c, _)| (c.group, c.label.to_lowercase()))
        .collect();
    let mut seen: BTreeSet<(String, String, String, [i64; 4])> = BTreeSet::new();
    kept.retain(|(c, f)| {
        if c.role == Role::StaticText && control_labels.contains(&(c.group, c.label.to_lowercase()))
        {
            return false;
        }
        seen.insert((
            c.role.as_str().to_owned(),
            c.label.clone(),
            c.value.clone().unwrap_or_default(),
            round(f),
        ))
    });

    let frames: Vec<Rect> = kept.iter().map(|(_, f)| *f).collect();
    let mut slots: Vec<Option<(Cand, Rect)>> = kept.into_iter().map(Some).collect();
    let mut elements = Vec::with_capacity(slots.len());
    for i in reading_order(&frames) {
        if let Some((c, frame)) = slots.get_mut(i).and_then(Option::take) {
            elements.push(Element {
                index: u32::try_from(elements.len() + 1).unwrap_or(u32::MAX),
                role: c.role,
                label: c.label,
                value: c.value,
                placeholder: c.placeholder,
                frame,
                enabled: c.enabled,
                focused: c.focused,
                selected: c.selected,
                secure: c.secure,
                fingerprint: c.fingerprint,
            });
        }
    }

    CompactScreen {
        app_bundle_id: screen.app_bundle_id.clone(),
        app_name: screen.app_name.clone(),
        size: screen.size,
        keyboard_visible: screen.keyboard_visible || keyboard.is_some(),
        elements,
    }
}
