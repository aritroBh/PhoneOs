#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Differential test of the faster tap-point search: on random screens (the generator of
//! `tap_landing_property.rs`, copied unchanged, plus a crowded variant that adds many
//! overlapping rows to its page), [`jarvis_safety::tap_spot_under`] gives exactly what the
//! implementation it replaced gives ([`jarvis_safety::reference::tap_spot_under`]): the
//! same point, or the same refusal naming the same cover, for every listed element, under
//! every occluder rule, with and without a preferred point, with and without drawn nodes.
//!
//! Seeded; `TAP_DIFF_SCREENS=<n>` changes the default of 600 screens (about 25 s in a debug
//! build; 50,000 were run once, in a release build), `TAP_DIFF_SEED=<n>` runs one alone.

use jarvis_observe::compact_view;
use jarvis_proto::fixtures::{node, screen, text};
use jarvis_proto::{Point, Rect, Role, Screen, UiNode};
use jarvis_safety::{reference, tap_spot, tap_spot_under, OccluderRule, ScreenTree, OCCLUDER_RULE};

/// Screens compared by default.
const DEFAULT_SCREENS: u64 = 600;

// ---- The generator of tap_landing_property.rs (copied unchanged) -----------------------

/// Screen sizes (tenth review): iPhone portrait and landscape, iPad portrait and
/// landscape.
const SIZES: &[(f64, f64)] = &[
    (393.0, 852.0),
    (852.0, 393.0),
    (1024.0, 1366.0),
    (1366.0, 1024.0),
];

thread_local! {
    /// The size of the screen being generated and checked (iPhone portrait by default).
    static SIZE: std::cell::Cell<(f64, f64)> = const { std::cell::Cell::new((393.0, 852.0)) };
}

/// The current screen's width.
fn sw() -> f64 {
    SIZE.with(|s| s.get().0)
}

/// The current screen's height.
fn sh() -> f64 {
    SIZE.with(|s| s.get().1)
}

/// Marks the generator's own page scroller, for the oracle (an identifier, which the rule
/// does not read).
const PAGE_ID: &str = "oracle.page";

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
    let w = if r.chance(10) {
        sw() + 80.0
    } else {
        sw() - 32.0
    };
    let h = r.f(36.0, 120.0);
    let f = Rect::new(x, y, w, h);
    let mut n = match r.below(10) {
        9 => {
            // Tenth review: a nested scroller — a carousel of cards across the row, or an
            // inner list — that may reach outside the row.
            let inner_role = r
                .pick(&[Role::ScrollView, Role::CollectionView, Role::Table])
                .clone();
            let tall = r.chance(30);
            let sf = if tall {
                Rect::new(0.0, y, sw(), r.f(200.0, sh()))
            } else {
                Rect::new(x, y, w, h)
            };
            let n_items = 2 + r.below(3) as usize;
            let items = if tall {
                controls_in(r, sf, n_items, Role::Button, k * 10 + 2000)
            } else {
                let iw = r.f(100.0, 220.0);
                (0..n_items)
                    .map(|i| {
                        control(
                            r,
                            Role::Button,
                            Rect::new(x + 8.0 + (iw + 8.0) * i as f64, y + 4.0, iw, h - 8.0),
                            k * 10 + 2000 + i,
                        )
                    })
                    .collect()
            };
            UiNode::new(inner_role, sf).with_children(items)
        }
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
    // Tenth review: overlays taller than 320 pt too (up to the whole screen).
    let tall = if r.chance(30) { sh() } else { 320.0 };
    let (w, h) = (r.f(120.0, sw()), r.f(60.0, tall));
    let f = Rect::new(r.f(0.0, sw() - w), r.f(0.0, sh() - h), w, h);
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
            // Tenth review: a bar child reaching outside the bar (above, below or both).
            let (dy, dh) = if r.chance(15) {
                *r.pick(&[
                    (-30.0, 30.0),
                    (0.0, f.height + 30.0),
                    (-20.0, f.height + 40.0),
                ])
            } else {
                (0.0, 0.0)
            };
            control(
                r,
                if tab { Role::Tab } else { Role::Button },
                Rect::new(
                    f.x + w * i as f64 + 4.0,
                    f.y + 4.0 + dy,
                    44.0_f64.min(w - 8.0),
                    40.0 + dh,
                ),
                base + i,
            )
        })
        .collect();
    UiNode::new(role, f).with_children(kids)
}

fn random_screen(seed: u64) -> Screen {
    let r = &mut Rng::new(seed);
    // Tenth review: landscape and iPad-sized screens too.
    let size = if r.chance(60) {
        SIZES[0]
    } else {
        *r.pick(SIZES)
    };
    SIZE.with(|s| s.set(size));
    let full = Rect::new(0.0, 0.0, sw(), sh());
    let nav_y = if sh() < 500.0 {
        0.0
    } else if sw() > 500.0 {
        24.0
    } else {
        59.0
    };
    // The page.
    let page_frame = match r.below(10) {
        0 => Rect::new(0.0, 0.0, sw() * 0.6, sh()),
        1 => Rect::new(0.0, 300.0, sw(), 120.0),
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
    let page = UiNode::new(page_role, page_frame)
        .with_identifier(PAGE_ID)
        .with_children(rows);
    let mut win = Vec::new();
    let nav = r.chance(70).then(|| {
        let h = *r.pick(&[44.0, 54.0, 96.0, 106.0, 400.0]);
        let w = if r.chance(85) { sw() } else { sw() * 0.7 };
        let role = if r.chance(85) {
            Role::NavigationBar
        } else {
            Role::Toolbar
        };
        bar(r, role, Rect::new(0.0, nav_y, w, h), 100)
    });
    let bottom = r.chance(70).then(|| {
        let (role, h) = if r.chance(60) {
            (Role::TabBar, 83.0)
        } else {
            (Role::Toolbar, *r.pick(&[49.0, 86.0, sh()]))
        };
        let w = if r.chance(90) { sw() } else { sw() * 0.5 };
        let f = if h >= sh() {
            full
        } else {
            Rect::new(0.0, sh() - h, w, h)
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
    if r.chance(10) {
        // Tenth review: a same-window overlay holding a page-sized scroller (a sheet the
        // tree types as a plain scroll view, without a modal role), drawn after the bars,
        // its controls reaching up under the navigation bar. The documented known gap.
        let top = r.f(0.0, nav_y + 40.0);
        let f = Rect::new(0.0, top, sw(), sh() - top);
        let n = 2 + r.below(5) as usize;
        let kh = r.f(120.0, f.height);
        let kids = controls_in(r, Rect::new(0.0, top, sw(), kh), n, Role::Button, 600);
        let scroller = UiNode::new(r.pick(&[Role::ScrollView, Role::CollectionView]).clone(), f)
            .with_children(kids);
        win.push(if r.chance(50) {
            UiNode::new(Role::Other("Other".into()), f).with_children(vec![scroller])
        } else {
            scroller
        });
    }
    if r.chance(3) {
        // An action sheet as an alert-like overlay (scopes the screen to itself).
        let f = Rect::new(8.0, sh() - 300.0, sw() - 16.0, 290.0);
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
        let kb = Rect::new(0.0, sh() - 336.0, sw(), 336.0);
        let bar_f = Rect::new(0.0, kb.y - 44.0, sw(), 44.0);
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
                Rect::new(16.0, r.f(100.0, 400.0), sw() - 32.0, 64.0),
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
    let mut s = screen("com.example.app", "App", root, keyboard);
    s.size = jarvis_proto::Size::new(sw(), sh());
    if sw() > sh() {
        s.orientation = jarvis_proto::Orientation::LandscapeLeft;
    }
    s
}

// ---- The crowded variant ----------------------------------------------------------------

/// The page of `n`, if it is the generator's page scroller or holds it.
fn page_of(n: &mut UiNode) -> Option<&mut UiNode> {
    if n.identifier.as_deref() == Some(PAGE_ID) {
        return Some(n);
    }
    n.children.iter_mut().find_map(page_of)
}

/// [`random_screen`] with 20 to 120 more rows on its page, a few points apart (so most
/// overlap and most candidate points are covered).
fn crowded_screen(seed: u64) -> Screen {
    let mut s = random_screen(seed);
    let r = &mut Rng::new(seed ^ 0xC0DE);
    if let Some(page) = page_of(&mut s.root) {
        let mut y = r.f(-150.0, 40.0);
        for k in 0..(20 + r.below(100)) as usize {
            page.children.push(row(r, 2000 + k, y));
            y += r.f(2.0, 30.0);
        }
    }
    s
}

// ---- The comparison ---------------------------------------------------------------------

const RULES: [OccluderRule; 6] = [
    OccluderRule::V0,
    OccluderRule::Controls,
    OccluderRule::Labelled,
    OccluderRule::LabelledNotOwnLabel,
    OccluderRule::LabelledOtherSubtree,
    OccluderRule::LabelledNested,
];

/// Compares new and old on every listed element of the screen of `seed`; returns how many
/// points were compared and how many refused (so the test can check refusals were
/// exercised).
fn compare(seed: u64, crowded: bool) -> (usize, usize) {
    let s = if crowded {
        crowded_screen(seed)
    } else {
        random_screen(seed)
    };
    let view = compact_view(&s);
    let tree = ScreenTree {
        sources: &view.sources,
        parents: &view.parents,
        ends: &view.ends,
        bars: &view.bars,
        nodes: &view.nodes,
        inputs: &view.inputs,
    };
    let no_nodes = ScreenTree { nodes: &[], ..tree };
    let mut r = Rng::new(seed ^ 0xD1FF);
    let (mut compared, mut refused) = (0, 0);
    for e in &view.screen.elements {
        let preferred = [
            None,
            Some(Point::new(
                e.frame.x + r.f(0.0, e.frame.width),
                e.frame.y + r.f(0.0, e.frame.height),
            )),
            Some(Point::new(r.f(0.0, s.size.width), r.f(0.0, s.size.height))),
        ];
        for pref in preferred {
            let new = tap_spot(e, &view.screen, Some(&tree), pref);
            let old = reference::tap_spot_under(OCCLUDER_RULE, e, &view.screen, Some(&tree), pref);
            assert_eq!(
                new, old,
                "seed {seed} (crowded {crowded}): [{}] {:?} {:?} preferred {pref:?}\n(run alone: TAP_DIFF_SEED={seed})",
                e.index, e.role, e.label
            );
            compared += 1;
            refused += usize::from(new.is_err());
            for rule in RULES {
                for t in [Some(&tree), Some(&no_nodes), None] {
                    assert_eq!(
                        tap_spot_under(rule, e, &view.screen, t, pref),
                        reference::tap_spot_under(rule, e, &view.screen, t, pref),
                        "seed {seed} (crowded {crowded}): [{}] rule {rule:?} preferred {pref:?}",
                        e.index
                    );
                }
            }
        }
    }
    (compared, refused)
}

#[test]
fn tap_points_match_the_reference() {
    if let Some(seed) = std::env::var("TAP_DIFF_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        compare(seed, false);
        compare(seed, true);
        return;
    }
    let screens = std::env::var("TAP_DIFF_SCREENS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SCREENS);
    let (mut compared, mut refused) = (0, 0);
    for seed in 0..screens {
        // One screen in four crowded.
        let (c, f) = compare(seed, seed % 4 == 3);
        compared += c;
        refused += f;
    }
    assert!(refused > 0 && refused < compared, "{refused} of {compared}");
    eprintln!("compared {compared} tap points, {refused} refused");
}
