#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Ninth review, property test 1: where a tap lands.
//!
//! Random screens — a page scroller with rows (labelled, unlabelled and icon-only
//! controls, cards with nested controls, SwiftUI-style toggle rows, text, fields, disabled
//! and hidden ones, rows off screen and partly clipped), navigation bars at the top and tab
//! bars or toolbars at the bottom (before or after the page in the tree, full width or not),
//! overlays in the same window and in later windows (menus, popovers, banners, action
//! sheets, with labelled content and controls), and the keyboard with its prediction bar
//! (sometimes with a control drawn outside the bar). For every listed control T, when the
//! rule ([`jarvis_safety::tap_spot`]) returns a point p, an oracle written here, from the
//! raw tree and without the rule's helpers, checks that:
//!
//! * p lies inside T's listed frame and on the screen;
//! * no other node that takes taps and is not T or T's ancestor contains p (T's own refined
//!   inner control — the same role, holding T's frame — excepted), and no labelled content
//!   drawn after T does, and nothing of the keyboard does;
//! * no system bar contains p unless T is inside it;
//! * the only exception: when T is inside a system bar B containing p, page content
//!   scrolled under B (a page-sized scrollable ancestor; no modal ancestor; not in a later
//!   window than B; for a bottom bar, the scroller before B in the tree).
//!
//! And the converse sanity check: on screens where nothing overlaps, every control gets its
//! centre. Seeded (xorshift); a failure names its seed (`TAP_SEED=<seed>` runs it alone);
//! the default is 50,000 screens (about 15 s in a debug build), `TAP_SCREENS=<n>` changes
//! it. `TAP_RULE=V0|Controls|Labelled` runs an older rule measured side by side, to see the
//! oracle catch its defects.

use jarvis_observe::compact_view;
use jarvis_proto::fixtures::{node, screen, text};
use jarvis_proto::{Rect, Role, Screen, UiNode};
use jarvis_safety::{tap_spot, ScreenTree};

const W: f64 = 393.0;
const H: f64 = 852.0;
/// Screens checked by default (about 15 s in a debug build); `TAP_SCREENS` overrides.
const DEFAULT_SCREENS: u64 = 50_000;

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
    fn f(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * (self.below(10_000) as f64 / 10_000.0)
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len() as u64) as usize]
    }
}

const LABELS: &[&str] = &[
    "Compose", "Delete", "Pay now", "Reply", "Share", "Item", "Open", "Buy", "Next",
];
const ICONS: &[&str] = &["trash", "chevron.right", "square.and.arrow.up", "xmark"];

/// A control: labelled, unlabelled with an icon image, or named by an icon.
fn control(r: &mut Rng, role: Role, frame: Rect, k: usize) -> UiNode {
    let mut n = UiNode::new(role, frame).enabled(!r.chance(10));
    match r.below(3) {
        0 => n = n.with_label(format!("{} {k}", r.pick(LABELS))),
        1 => {
            let s = frame.width.min(frame.height).min(24.0);
            n = n.with_children(vec![node(
                Role::Image,
                r.pick(ICONS),
                Rect::new(frame.x + 4.0, frame.y + 4.0, s, s),
            )]);
        }
        _ => n = n.with_label(*r.pick(ICONS)),
    }
    n
}

/// One row of the page at `y`.
fn row(r: &mut Rng, k: usize, y: f64) -> UiNode {
    let x = if r.chance(10) { r.f(-60.0, 0.0) } else { 16.0 };
    let w = if r.chance(10) { W + 80.0 } else { W - 32.0 };
    let h = r.f(36.0, 120.0);
    let f = Rect::new(x, y, w, h);
    let mut n = match r.below(9) {
        0 | 1 => control(r, Role::Button, f, k),
        2 => UiNode::new(Role::Cell, f).with_children(vec![
            text(
                &format!("Row {k}"),
                Rect::new(x + 16.0, y + 8.0, 200.0, 20.0),
            ),
            text(
                &format!("${k}.00"),
                Rect::new(x + 16.0, y + 30.0, 80.0, 20.0),
            ),
        ]),
        3 => {
            // A card holding its texts and a nested control (a trash button, a "Buy"
            // button, or an inner control covering nearly all of it).
            let inner = match r.below(3) {
                0 => control(
                    r,
                    Role::Button,
                    Rect::new(f.max_x() - 54.0, y + 4.0, 44.0, 36.0),
                    k + 1000,
                ),
                1 => node(
                    Role::Button,
                    "Buy now",
                    Rect::new(x + 20.0, y + h / 2.0 - 10.0, 120.0, 20.0),
                ),
                _ => UiNode::new(Role::Button, Rect::new(x + 1.0, y + 1.0, w - 2.0, h - 2.0))
                    .with_children(vec![node(
                        Role::Image,
                        "trash",
                        Rect::new(x + 8.0, y + 8.0, 20.0, 20.0),
                    )]),
            };
            let mut kids = vec![inner];
            if r.chance(60) {
                kids.insert(
                    0,
                    text(
                        &format!("Card {k}"),
                        Rect::new(x + 16.0, y + 4.0, 160.0, 20.0),
                    ),
                );
            }
            let card = UiNode::new(Role::Button, f).with_children(kids);
            if r.chance(50) {
                card.with_label(format!("Card {k}"))
            } else {
                card
            }
        }
        4 => text(&format!("Text {k}"), f),
        5 => UiNode::new(Role::Switch, f)
            .with_label(format!("Toggle {k}"))
            .with_value("0")
            .with_children(vec![UiNode::new(
                Role::Switch,
                Rect::new(f.max_x() - 70.0, y + 8.0, 63.0, 28.0),
            )
            .with_value("0")]),
        6 => UiNode::new(Role::TextField, Rect::new(x, y, w, 44.0))
            .with_label(format!("Field {k}"))
            .with_value("hello"),
        7 => UiNode::new(Role::Cell, f),
        _ => control(r, Role::Link, f, k),
    };
    if r.chance(4) {
        n = n.visible(false);
    }
    n
}

/// A list of `n` controls in a vertical stack inside `f`.
fn controls_in(r: &mut Rng, f: Rect, n: usize, role: Role, base: usize) -> Vec<UiNode> {
    let h = f.height / n.max(1) as f64;
    (0..n)
        .map(|i| {
            control(
                r,
                role.clone(),
                Rect::new(f.x, f.y + h * i as f64, f.width, h),
                base + i,
            )
        })
        .collect()
}

/// A random overlay (menu, popover, banner, action sheet) at a random place.
fn overlay(r: &mut Rng, base: usize) -> UiNode {
    let (w, h) = (r.f(120.0, W), r.f(60.0, 320.0));
    let f = Rect::new(r.f(0.0, W - w), r.f(0.0, H - h), w, h);
    let role = r
        .pick(&[
            Role::Other("Menu".into()),
            Role::Other("Popover".into()),
            Role::Other("Other".into()),
            Role::CollectionView,
            Role::Other("ActionSheet".into()),
        ])
        .clone();
    let n = 1 + r.below(4) as usize;
    let mut kids = controls_in(r, f, n, Role::Button, base);
    if r.chance(40) {
        kids.push(text("Banner text", Rect::new(f.x, f.y, f.width, 20.0)));
    }
    UiNode::new(role, f).with_children(kids)
}

fn bar(r: &mut Rng, role: Role, f: Rect, base: usize) -> UiNode {
    let n = 1 + r.below(4) as usize;
    let w = f.width / n as f64;
    let kids: Vec<UiNode> = (0..n)
        .map(|i| {
            let tab = role == Role::TabBar && r.chance(50);
            control(
                r,
                if tab { Role::Tab } else { Role::Button },
                Rect::new(
                    f.x + w * i as f64 + 4.0,
                    f.y + 4.0,
                    44.0_f64.min(w - 8.0),
                    40.0,
                ),
                base + i,
            )
        })
        .collect();
    UiNode::new(role, f).with_children(kids)
}

fn random_screen(seed: u64) -> Screen {
    let r = &mut Rng::new(seed);
    let full = Rect::new(0.0, 0.0, W, H);
    // The page.
    let page_frame = match r.below(10) {
        0 => Rect::new(0.0, 0.0, W * 0.6, H),
        1 => Rect::new(0.0, 300.0, W, 120.0),
        _ => full,
    };
    let page_role = r
        .pick(&[
            Role::ScrollView,
            Role::CollectionView,
            Role::Table,
            Role::Other("Other".into()),
        ])
        .clone();
    let mut y = r.f(-150.0, 40.0);
    let mut rows = Vec::new();
    for k in 0..r.below(14) as usize {
        rows.push(row(r, k, y));
        y += r.f(20.0, 130.0);
    }
    let page = UiNode::new(page_role, page_frame).with_children(rows);
    let mut win = Vec::new();
    let nav = r.chance(70).then(|| {
        let h = *r.pick(&[44.0, 54.0, 96.0, 106.0, 400.0]);
        let w = if r.chance(85) { W } else { W * 0.7 };
        let role = if r.chance(85) {
            Role::NavigationBar
        } else {
            Role::Toolbar
        };
        bar(r, role, Rect::new(0.0, 59.0, w, h), 100)
    });
    let bottom = r.chance(70).then(|| {
        let (role, h) = if r.chance(60) {
            (Role::TabBar, 83.0)
        } else {
            (Role::Toolbar, *r.pick(&[49.0, 86.0, H]))
        };
        let w = if r.chance(90) { W } else { W * 0.5 };
        let f = if h >= H {
            full
        } else {
            Rect::new(0.0, H - h, w, h)
        };
        bar(r, role, f, 200)
    });
    let nav_first = r.chance(50);
    let bottom_first = r.chance(15);
    if let (Some(n), true) = (&nav, nav_first) {
        win.push(n.clone());
    }
    if let (Some(b), true) = (&bottom, bottom_first) {
        win.push(b.clone());
    }
    win.push(page);
    if let (Some(n), false) = (nav, nav_first) {
        win.push(n);
    }
    if let (Some(b), false) = (bottom, bottom_first) {
        win.push(b);
    }
    if r.chance(25) {
        win.push(overlay(r, 300));
    }
    if r.chance(3) {
        // An action sheet as an alert-like overlay (scopes the screen to itself).
        let f = Rect::new(8.0, H - 300.0, W - 16.0, 290.0);
        win.push(UiNode::new(Role::Sheet, f).with_children(controls_in(
            r,
            f,
            3,
            Role::Button,
            400,
        )));
    }
    let mut windows = vec![UiNode::new(Role::Window, full).with_children(win)];
    if r.chance(20) {
        windows.push(UiNode::new(Role::Window, full).with_children(vec![overlay(r, 500)]));
    }
    let keyboard = r.chance(20);
    if keyboard {
        let kb = Rect::new(0.0, H - 336.0, W, 336.0);
        let bar_f = Rect::new(0.0, kb.y - 44.0, W, 44.0);
        let mut assistant_kids = vec![UiNode::new(Role::Other("Other".into()), bar_f)
            .with_children(vec![node(
                Role::Button,
                "Passwords",
                Rect::new(30.0, bar_f.y, 300.0, 44.0),
            )])];
        if r.chance(30) {
            // A control drawn outside the bar's own frame: ordinary content.
            assistant_kids.push(node(
                Role::Button,
                "Buy coins $99.99",
                Rect::new(16.0, r.f(100.0, 400.0), W - 32.0, 64.0),
            ));
        }
        let assistant = UiNode::new(Role::Other("Other".into()), bar_f)
            .with_identifier("SystemInputAssistantView")
            .with_children(assistant_kids);
        let keys: Vec<UiNode> = (0..10)
            .map(|i| {
                node(
                    Role::Key,
                    "q",
                    Rect::new(4.0 + 39.0 * f64::from(i), kb.y + 10.0, 37.0, 46.0),
                )
            })
            .collect();
        let kbd = UiNode::new(Role::Keyboard, kb).with_children(keys);
        windows.push(UiNode::new(Role::Window, full).with_children(vec![assistant, kbd]));
    }
    let root = UiNode::new(Role::Application, full)
        .with_label("App")
        .with_children(windows);
    screen("com.example.app", "App", root, keyboard)
}

// ---- The oracle: written from the raw tree, without the rule's helpers ------------------

/// One raw node, flattened in pre-order (the digest's node order).
struct Flat<'a> {
    node: &'a UiNode,
    parent: Option<usize>,
    end: usize,
}

fn flatten(root: &UiNode) -> Vec<Flat<'_>> {
    fn go<'a>(n: &'a UiNode, parent: Option<usize>, out: &mut Vec<Flat<'a>>) {
        let me = out.len();
        out.push(Flat {
            node: n,
            parent,
            end: 0,
        });
        for c in &n.children {
            go(c, Some(me), out);
        }
        out[me].end = out.len();
    }
    let mut out = Vec::new();
    go(root, None, &mut out);
    out
}

fn ancestors(flat: &[Flat<'_>], i: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut at = flat[i].parent;
    while let Some(p) = at {
        out.push(p);
        at = flat[p].parent;
    }
    out
}

fn inside(flat: &[Flat<'_>], n: usize, container: usize) -> bool {
    n > container && n < flat[container].end
}

fn name(role: &Role) -> String {
    match role {
        Role::Other(s) => s
            .chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect(),
        r => r.as_str().replace('_', ""),
    }
}

fn takes_taps(n: &UiNode) -> bool {
    let has_text = n.label.as_deref().is_some_and(|l| !l.trim().is_empty())
        || (!n.secure && n.value.as_deref().is_some_and(|v| !v.trim().is_empty()));
    match &n.role {
        Role::Cell => has_text,
        Role::Button
        | Role::Link
        | Role::Switch
        | Role::Slider
        | Role::Picker
        | Role::Tab
        | Role::TextField
        | Role::SecureTextField
        | Role::TextView
        | Role::SearchField => true,
        r => matches!(
            name(r).as_str(),
            "menuitem" | "checkbox" | "toggle" | "stepper"
        ),
    }
}

fn container(role: &Role) -> bool {
    matches!(
        name(role).as_str(),
        "application"
            | "window"
            | "scrollview"
            | "table"
            | "collectionview"
            | "webview"
            | "navigationbar"
            | "tabbar"
            | "toolbar"
            | "alert"
            | "sheet"
            | "keyboard"
    )
}

fn scroller(role: &Role) -> bool {
    matches!(
        name(role).as_str(),
        "scrollview" | "table" | "collectionview" | "webview"
    )
}

fn modal(role: &Role) -> bool {
    let n = name(role);
    ["alert", "sheet", "popover", "menu", "dialog", "modal"]
        .iter()
        .any(|k| n.contains(k))
        && !n.contains("menuitem")
}

fn clip(f: Rect) -> Option<Rect> {
    if f.is_empty() {
        return None;
    }
    f.intersection(&Rect::new(0.0, 0.0, W, H))
}

fn holds(outer: &Rect, inner: &Rect) -> bool {
    inner.x >= outer.x - 1.0
        && inner.y >= outer.y - 1.0
        && inner.max_x() <= outer.max_x() + 1.0
        && inner.max_y() <= outer.max_y() + 1.0
}

/// What the oracle knows of a screen.
struct World<'a> {
    flat: Vec<Flat<'a>>,
    /// Drawn: no hidden ancestor (or self) with a real frame, inside the alert if one is up.
    drawn: Vec<bool>,
    /// The keyboard part a node belongs to, with that part's root frame (nodes reaching
    /// outside it are ordinary content).
    keyboard: Vec<Option<Rect>>,
    /// System bars: (raw index, shown frame, bottom edge).
    bars: Vec<(usize, Rect, bool)>,
}

fn world(s: &Screen) -> World<'_> {
    let flat = flatten(&s.root);
    let n = flat.len();
    let hidden = |i: usize| !flat[i].node.visible && !flat[i].node.frame.is_empty();
    let mut drawn: Vec<bool> = (0..n)
        .map(|i| !hidden(i) && ancestors(&flat, i).iter().all(|a| !hidden(*a)))
        .collect();
    // The topmost alert or sheet scopes the screen.
    let alert = (0..n).rev().find(|&i| {
        drawn[i]
            && matches!(flat[i].node.role, Role::Alert | Role::Sheet)
            && !flat[i].node.frame.is_empty()
    });
    if let Some(a) = alert {
        for (i, d) in drawn.iter_mut().enumerate() {
            *d = *d && (i == a || inside(&flat, i, a));
        }
    }
    // The keyboard: `Keyboard` subtrees, and input-assistant subtrees in a window with one.
    let mut keyboard = vec![None; n];
    let windows: Vec<usize> = (0..n).filter(|&i| flat[i].parent == Some(0)).collect();
    for &w in &windows {
        let has_kb = (w..flat[w].end).any(|i| flat[i].node.role == Role::Keyboard);
        for i in w..flat[w].end {
            let root = flat[i].node.role == Role::Keyboard
                || (has_kb
                    && flat[i].node.identifier.as_deref() == Some("SystemInputAssistantView"));
            if root && keyboard[i].is_none() {
                let rf = flat[i].node.frame;
                for (j, slot) in keyboard.iter_mut().enumerate().take(flat[i].end).skip(i) {
                    let own = clip(flat[j].node.frame);
                    if j == i || own.is_none_or(|f| holds(&rf, &f)) {
                        *slot = Some(rf);
                    }
                }
            }
        }
    }
    // The keyboard's top, for "shown".
    let kb_top = if alert.is_some() {
        None
    } else {
        (0..n)
            .find(|&i| {
                drawn[i] && flat[i].node.role == Role::Keyboard && !flat[i].node.frame.is_empty()
            })
            .map(|i| flat[i].node.frame.y)
    };
    let shown = |f: Rect| -> Option<Rect> {
        let mut v = clip(f)?;
        if let Some(top) = kb_top {
            if v.max_y() > top {
                if v.y >= top {
                    return None;
                }
                v.height = top - v.y;
            }
        }
        let frac = v.area() / f.area();
        (!v.is_empty() && (frac >= 0.3 || (v.width >= 24.0 && v.height >= 24.0))).then_some(v)
    };
    let mut bars = Vec::new();
    if alert.is_none() {
        for i in 0..n {
            let role = &flat[i].node.role;
            if !matches!(role, Role::NavigationBar | Role::TabBar | Role::Toolbar) || !drawn[i] {
                continue;
            }
            if ancestors(&flat, i)
                .iter()
                .any(|a| scroller(&flat[*a].node.role))
            {
                continue;
            }
            let Some(f) = shown(flat[i].node.frame) else {
                continue;
            };
            let top = f.y <= 0.15 * H && f.height <= (0.2 * H).max(100.0);
            let bottom = f.max_y() >= 0.9 * H && f.height <= (0.15 * H).max(100.0);
            if f.width >= 0.9 * W && (top || bottom) {
                let is_bottom = !top || (bottom && f.center().y >= H / 2.0);
                bars.push((i, f, is_bottom));
            }
        }
    }
    World {
        flat,
        drawn,
        keyboard,
        bars,
    }
}

/// The top-level window (a child of the root) holding raw node `i`.
fn window_of(w: &World<'_>, i: usize) -> usize {
    let mut at = i;
    while let Some(p) = w.flat[at].parent {
        if p == 0 {
            return at;
        }
        at = p;
    }
    at
}

/// The exception: `n` is page content scrolled under bar `b` (see the module docs).
fn scrolled_under(w: &World<'_>, n: usize, b: (usize, Rect, bool)) -> bool {
    let (bi, bf, bottom) = b;
    if inside(&w.flat, n, bi) || window_of(w, n) > window_of(w, bi) {
        return false;
    }
    let anc = ancestors(&w.flat, n);
    if anc.iter().any(|a| modal(&w.flat[*a].node.role)) {
        return false;
    }
    anc.iter().any(|&a| {
        let s = w.flat[a].node;
        scroller(&s.role)
            && w.drawn[a]
            && clip(s.frame).is_some_and(|f| f.width >= 0.9 * W && f.height >= 2.0 * bf.height)
            && (!bottom || (a < bi && !inside(&w.flat, bi, a)))
    })
}

/// What the oracle finds wrong with point `p` for the target listed from raw node `t` with
/// listed frame `tf`, if anything.
fn violation(w: &World<'_>, t: usize, tf: Rect, p: jarvis_proto::Point) -> Option<String> {
    if !tf.contains(p) || p.x < 0.0 || p.y < 0.0 || p.x > W || p.y > H {
        return Some(format!("{p:?} outside the target {tf:?} or the screen"));
    }
    let home = w
        .bars
        .iter()
        .copied()
        .find(|(bi, f, _)| f.contains(p) && inside(&w.flat, t, *bi));
    for (bi, f, _) in &w.bars {
        if f.contains(p) && !inside(&w.flat, t, *bi) {
            return Some(format!("{p:?} is inside bar #{bi} {f:?}"));
        }
    }
    let tnode = w.flat[t].node;
    for (i, fl) in w.flat.iter().enumerate() {
        if i == t || !w.drawn[i] || inside(&w.flat, t, i) {
            continue;
        }
        let n = fl.node;
        if let Some(rf) = w.keyboard[i] {
            if clip(n.frame)
                .and_then(|f| f.intersection(&rf))
                .is_some_and(|f| f.contains(p))
            {
                return Some(format!("{p:?} is on the keyboard (#{i} {:?})", n.role));
            }
            continue;
        }
        if !clip(n.frame).is_some_and(|f| f.contains(p)) {
            continue;
        }
        let excused = home.is_some_and(|b| scrolled_under(w, i, b));
        if takes_taps(n) {
            if inside(&w.flat, i, t) {
                let refined = n.role == tnode.role && holds(&n.frame, &tf);
                if !refined {
                    return Some(format!(
                        "{p:?} is on #{i} {:?} {:?} nested in the target",
                        n.role, n.label
                    ));
                }
                continue;
            }
            if !excused {
                return Some(format!(
                    "{p:?} is on control #{i} {:?} {:?}",
                    n.role, n.label
                ));
            }
            continue;
        }
        let labelled = n.label.as_deref().is_some_and(|l| !l.trim().is_empty())
            || (!n.secure && n.value.as_deref().is_some_and(|v| !v.trim().is_empty()));
        let later = i >= w.flat[t].end;
        if labelled && later && !container(&n.role) && !excused {
            return Some(format!(
                "{p:?} is on labelled content #{i} {:?} {:?} drawn later",
                n.role, n.label
            ));
        }
    }
    None
}

/// The rule under test: the one in force, or (`TAP_RULE=V0|Controls|Labelled`, to see the
/// oracle catch an older rule's defects) another one measured side by side.
fn spot(
    e: &jarvis_proto::Element,
    s: &jarvis_proto::CompactScreen,
    tree: &ScreenTree<'_>,
) -> Result<jarvis_proto::Point, jarvis_safety::Covered> {
    use jarvis_safety::OccluderRule as R;
    let rule = match std::env::var("TAP_RULE").as_deref() {
        Ok("V0") => R::V0,
        Ok("Controls") => R::Controls,
        Ok("Labelled") => R::Labelled,
        _ => return tap_spot(e, s, Some(tree), None),
    };
    jarvis_safety::tap_spot_under(rule, e, s, Some(tree), None)
}

/// Counts of what one screen exercised.
#[derive(Default)]
struct Tally {
    controls: usize,
    points: usize,
    refused: usize,
    in_bars: usize,
}

fn check(seed: u64, tally: &mut Tally) {
    let s = random_screen(seed);
    let view = compact_view(&s);
    let tree = ScreenTree {
        sources: &view.sources,
        parents: &view.parents,
        ends: &view.ends,
        bars: &view.bars,
        nodes: &view.nodes,
        inputs: &view.inputs,
    };
    let w = world(&s);
    assert_eq!(w.flat.len(), view.parents.len(), "seed {seed}: node order");
    for e in &view.screen.elements {
        let t = view.sources[(e.index - 1) as usize] as usize;
        if !takes_taps(w.flat[t].node) {
            continue;
        }
        tally.controls += 1;
        match spot(e, &view.screen, &tree) {
            Ok(p) => {
                tally.points += 1;
                if w.bars
                    .iter()
                    .any(|(bi, f, _)| f.contains(p) && inside(&w.flat, t, *bi))
                {
                    tally.in_bars += 1;
                }
                if let Some(v) = violation(&w, t, e.frame, p) {
                    panic!(
                        "seed {seed}: [{}] {:?} {:?} at {:?}: {v}\n(run alone: TAP_SEED={seed})",
                        e.index, e.role, e.label, e.frame
                    );
                }
            }
            Err(_) => tally.refused += 1,
        }
    }
}

#[test]
fn every_tap_point_lands_on_its_control_alone() {
    if let Some(seed) = std::env::var("TAP_SEED").ok().and_then(|v| v.parse().ok()) {
        check(seed, &mut Tally::default());
        return;
    }
    let screens = std::env::var("TAP_SCREENS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SCREENS);
    let mut tally = Tally::default();
    for seed in 1..=screens {
        let run =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(seed, &mut tally)));
        if let Err(e) = run {
            let msg = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_default();
            panic!("seed {seed} failed (run alone: TAP_SEED={seed}): {msg}");
        }
    }
    println!(
        "{screens} screens: {} controls, {} points ({} inside their bar), {} refused",
        tally.controls, tally.points, tally.in_bars, tally.refused
    );
    assert!(tally.points > tally.refused && tally.in_bars > 0 && tally.refused > 0);
}

/// The converse: where nothing overlaps (a grid of controls, no bars, no keyboard, no
/// overlay), every control is tapped at its centre.
#[test]
fn without_overlap_every_control_gets_its_centre() {
    let screens = std::env::var("TAP_SCREENS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_SCREENS)
        / 4;
    for seed in 1..=screens.max(100) {
        let r = &mut Rng::new(seed);
        let cols = 1 + r.below(3) as usize;
        let rows = 1 + r.below(10) as usize;
        let (cw, ch) = (W / cols as f64, (H - 100.0) / rows as f64);
        let mut kids = Vec::new();
        for i in 0..rows {
            for j in 0..cols {
                let f = Rect::new(
                    cw * j as f64 + 4.0,
                    50.0 + ch * i as f64 + 2.0,
                    cw - 8.0,
                    ch - 4.0,
                );
                let role = r
                    .pick(&[Role::Button, Role::Link, Role::Cell, Role::Switch])
                    .clone();
                kids.push(match role {
                    Role::Cell => UiNode::new(Role::Cell, f).with_children(vec![text(
                        &format!("Row {i} {j}"),
                        Rect::new(f.x + 2.0, f.y + 2.0, f.width / 2.0, f.height / 2.0),
                    )]),
                    r => node(r, &format!("Control {i} {j}"), f),
                });
            }
        }
        let full = Rect::new(0.0, 0.0, W, H);
        let root = UiNode::new(Role::Application, full).with_children(vec![UiNode::new(
            Role::Window,
            full,
        )
        .with_children(vec![UiNode::new(Role::ScrollView, full).with_children(kids)])]);
        let s = screen("com.example.app", "App", root, false);
        let view = compact_view(&s);
        let tree = ScreenTree {
            sources: &view.sources,
            parents: &view.parents,
            ends: &view.ends,
            bars: &view.bars,
            nodes: &view.nodes,
            inputs: &view.inputs,
        };
        assert_eq!(view.screen.elements.len(), rows * cols, "seed {seed}");
        for e in &view.screen.elements {
            assert_eq!(
                tap_spot(e, &view.screen, Some(&tree), None),
                Ok(e.frame.center()),
                "seed {seed}: {}",
                e.label
            );
        }
    }
}
