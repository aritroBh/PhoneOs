#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Sixth review: the digest and compaction agree. On random trees, a non-frame mutation of
//! one node that leaves the digest equal leaves the compact content an approval compares
//! (role, label, value, placeholder, selected, enabled, secure) equal: *anything not hashed
//! is not in the compact screen*. The review ran 200,000 trees; this keeps 5,000 with the
//! same deterministic seed (`DIGEST_TREES=200000` runs the full sweep).
//!
//! Digest v6 (CI run of df4a416): the order-free rule. On random trees with runs of
//! hidden, area-less subtrees and near-misses, the digest is equal **exactly** when an
//! independent oracle's canonical form (raw order, but each maximal run of order-free
//! siblings sorted) is equal; and when it is, the whole `CompactView` an approval and the
//! gate read is equal (the audit in the digest's module docs).

use jarvis_observe::{compact_view, screen_digest};
use jarvis_proto::fixtures::IPHONE_SIZE;
use jarvis_proto::{Rect, Role, Screen, UiNode};

/// Random trees checked by default (the review used 200,000; `DIGEST_TREES` overrides).
const TREES: usize = 5_000;

/// Trees to check: `DIGEST_TREES` when set, else `default`.
fn trees(default: usize) -> usize {
    std::env::var("DIGEST_TREES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

struct Rng(u64);
impl Rng {
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
}

const WORDS: &[&str] = &[
    "Pay", "Send", "Alice", "$40.00", "Delete", "OK", "Note", "send", "return",
];

fn role(r: &mut Rng, depth: usize) -> Role {
    let roles = [
        Role::Other("Other".into()),
        Role::Button,
        Role::StaticText,
        Role::Cell,
        Role::TextField,
        Role::Image,
        Role::Keyboard,
        Role::Key,
        Role::Alert,
        Role::Switch,
        Role::Window,
        Role::Other("StatusBar".into()),
        Role::Other("Icon".into()),
        Role::SecureTextField,
    ];
    if depth <= 1 && r.chance(30) {
        return Role::Keyboard;
    }
    roles[r.below(roles.len() as u64) as usize].clone()
}

fn opt(r: &mut Rng, pct: u64) -> Option<String> {
    r.chance(pct)
        .then(|| WORDS[r.below(WORDS.len() as u64) as usize].to_owned())
}

fn gen(r: &mut Rng, depth: usize, budget: &mut usize) -> UiNode {
    let w = IPHONE_SIZE.width;
    let h = IPHONE_SIZE.height;
    let frame = if r.chance(8) {
        Rect::new(0.0, 0.0, 0.0, 0.0)
    } else {
        let y = r.f(-50.0, h + 50.0);
        Rect::new(r.f(-20.0, w), y, r.f(1.0, w), r.f(1.0, 300.0))
    };
    let mut n = UiNode::new(role(r, depth), frame);
    n.label = opt(r, 50);
    n.value = opt(r, 20);
    n.identifier = opt(r, 15);
    n.placeholder = opt(r, 10);
    n.visible = !r.chance(10);
    n.enabled = !r.chance(10);
    n.selected = r.chance(10);
    n.focused = r.chance(10);
    n.secure = r.chance(5);
    n.hittable = r.chance(50);
    if depth < 6 {
        let kids = r.below(4) as usize;
        for _ in 0..kids {
            if *budget == 0 {
                break;
            }
            *budget -= 1;
            n.children.push(gen(r, depth + 1, budget));
        }
    }
    n
}

fn tree(r: &mut Rng) -> Screen {
    let mut budget = 40usize;
    let windows = 1 + r.below(3) as usize;
    let mut root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_label("App");
    for _ in 0..windows {
        let mut win = UiNode::new(Role::Window, IPHONE_SIZE.to_rect());
        for _ in 0..(1 + r.below(4)) {
            win.children.push(gen(r, 1, &mut budget));
        }
        root.children.push(win);
    }
    let bundle = if r.chance(20) {
        "com.apple.springboard"
    } else {
        "com.example.app"
    };
    Screen {
        app_bundle_id: bundle.into(),
        app_name: None,
        size: IPHONE_SIZE,
        root,
        keyboard_visible: false,
        orientation: Default::default(),
    }
}

/// The compact content an approval compares (A2): no frames, focus or fingerprints.
fn facts(s: &Screen) -> Vec<String> {
    let v = compact_view(s);
    v.screen
        .elements
        .iter()
        .map(|e| {
            format!(
                "{}|{}|{:?}|{:?}|{}{}{}",
                e.role.as_str(),
                e.label,
                e.value,
                e.placeholder,
                e.selected,
                e.enabled,
                e.secure
            )
        })
        .collect()
}

fn count(n: &UiNode) -> usize {
    1 + n.children.iter().map(count).sum::<usize>()
}

fn nth<'a>(n: &'a mut UiNode, k: &mut usize) -> Option<&'a mut UiNode> {
    if *k == 0 {
        return Some(n);
    }
    *k -= 1;
    for c in n.children.iter_mut() {
        if let Some(p) = nth(c, k) {
            return Some(p);
        }
    }
    None
}

#[test]
fn equal_digest_implies_equal_compact_content_for_non_frame_mutations() {
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    let mut checked = 0usize;
    let mut failures = Vec::new();
    for t in 0..trees(TREES) {
        let s = tree(&mut r);
        let total = count(&s.root);
        let k0 = r.below(total as u64) as usize;
        let mut m = s.clone();
        let mut k = k0;
        let node = nth(&mut m.root, &mut k).unwrap();
        let kind = r.below(9);
        match kind {
            0 => node.label = Some(format!("X{}", r.below(5))),
            1 => node.value = Some(format!("V{}", r.below(5))),
            2 => node.identifier = Some(format!("I{}", r.below(5))),
            3 => node.placeholder = Some(format!("P{}", r.below(5))),
            4 => node.enabled = !node.enabled,
            5 => node.selected = !node.selected,
            6 => node.visible = !node.visible,
            7 => node.secure = !node.secure,
            _ => node.hittable = !node.hittable,
        }
        let role = node.role.clone();
        if screen_digest(&s) == screen_digest(&m) {
            checked += 1;
            if facts(&s) != facts(&m) {
                failures.push(format!(
                    "tree {t}, node {k0} ({:?}), mutation {kind}:\n  {:?}\n  {:?}",
                    role,
                    facts(&s),
                    facts(&m)
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {checked} equal-digest pairs differ in compact content; first:\n{}",
        failures.len(),
        {
            let mut kinds: Vec<String> = failures
                .iter()
                .map(|f| {
                    f.split(", mutation ")
                        .nth(1)
                        .unwrap()
                        .split(':')
                        .next()
                        .unwrap()
                        .to_owned()
                })
                .collect();
            kinds.sort();
            kinds.dedup();
            format!(
                "kinds {kinds:?}\n{}",
                failures
                    .iter()
                    .take(2)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        }
    );
    eprintln!("checked {checked} equal-digest pairs");
}

// ---------------------------------------------------------------------------------------
// Digest v6: the order-free rule against an independent oracle.
// ---------------------------------------------------------------------------------------

/// Random order-free trees and permutations checked by default (`DIGEST_TREES` overrides).
const ORDER_TREES: usize = 5_000;

/// A hidden, area-less subtree that is order-free (see the digest's module docs, *Order*).
fn ghost(r: &mut Rng, depth: usize) -> UiNode {
    let roles = [
        Role::Button,
        Role::Other("Other".into()),
        Role::Cell,
        Role::Switch,
        Role::Other("Icon".into()),
        Role::StaticText,
        Role::Image,
        Role::Link,
    ];
    let role = roles[r.below(roles.len() as u64) as usize].clone();
    let text = matches!(role, Role::StaticText | Role::Image);
    let mut n = UiNode::new(role, Rect::default());
    n.visible = false;
    if !text {
        n.label = opt(r, 60);
        n.value = opt(r, 20);
    }
    n.identifier = opt(r, 30);
    n.enabled = !r.chance(10);
    n.selected = r.chance(10);
    n.focused = r.chance(10);
    if depth < 3 {
        for _ in 0..r.below(3) {
            n.children.push(ghost(r, depth + 1));
        }
    }
    n
}

/// A run's member that is *almost* order-free: one node of it is shown, framed, a text
/// input, a labelled text or secure.
fn near_miss(r: &mut Rng) -> UiNode {
    let mut g = ghost(r, 1);
    let mut k = r.below(count(&g) as u64) as usize;
    let n = nth(&mut g, &mut k).unwrap();
    match r.below(6) {
        0 => n.visible = true,
        1 => n.frame = Rect::new(r.f(0.0, 300.0), r.f(0.0, 700.0), r.f(1.0, 90.0), 40.0),
        2 => {
            n.frame = Rect::new(500.0, 100.0, 50.0, 40.0);
        }
        3 => n.role = Role::TextField,
        4 => {
            n.role = Role::StaticText;
            n.label = Some(format!("T{}", r.below(3)));
        }
        _ => n.secure = true,
    }
    g
}

/// Like [`gen`] without keyboards, alerts or the status bar (the oracle does not model
/// them; the unit tests cover them), with runs of order-free subtrees and near-misses
/// among the children.
fn gen_runs(r: &mut Rng, depth: usize, budget: &mut usize) -> UiNode {
    let roles = [
        Role::Other("Other".into()),
        Role::Button,
        Role::StaticText,
        Role::Cell,
        Role::TextField,
        Role::Image,
        Role::Switch,
        Role::Other("Icon".into()),
        Role::SecureTextField,
    ];
    let w = IPHONE_SIZE.width;
    let h = IPHONE_SIZE.height;
    let frame = if r.chance(15) {
        Rect::default()
    } else {
        Rect::new(
            r.f(-20.0, w),
            r.f(-50.0, h + 50.0),
            r.f(1.0, w),
            r.f(1.0, 300.0),
        )
    };
    let mut n = UiNode::new(roles[r.below(roles.len() as u64) as usize].clone(), frame);
    n.label = opt(r, 50);
    n.value = opt(r, 20);
    n.identifier = opt(r, 15);
    n.placeholder = opt(r, 10);
    n.visible = !r.chance(15);
    n.enabled = !r.chance(10);
    n.selected = r.chance(10);
    n.secure = r.chance(5);
    if depth < 5 {
        for _ in 0..r.below(4) {
            if *budget == 0 {
                break;
            }
            *budget -= 1;
            match r.below(10) {
                0..=2 => {
                    for _ in 0..(1 + r.below(4)) {
                        n.children.push(ghost(r, 1));
                    }
                }
                3 => n.children.push(near_miss(r)),
                _ => n.children.push(gen_runs(r, depth + 1, budget)),
            }
        }
    }
    n
}

fn tree_runs(r: &mut Rng) -> Screen {
    let mut budget = 30usize;
    let mut root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_label("App");
    let mut win = UiNode::new(Role::Window, IPHONE_SIZE.to_rect());
    for _ in 0..(1 + r.below(3)) {
        win.children.push(gen_runs(r, 1, &mut budget));
    }
    root.children.push(win);
    Screen {
        app_bundle_id: "com.example.app".into(),
        app_name: None,
        size: IPHONE_SIZE,
        root,
        keyboard_visible: false,
        orientation: Default::default(),
    }
}

/// The oracle's order-free predicate, written from the rule's statement (not the code):
/// every node of the subtree flagged invisible with no area, none a text input, secure,
/// a labelled or valued text or image, a keyboard or a key.
fn oracle_free(n: &UiNode) -> bool {
    !n.visible
        && n.frame.is_empty()
        && !n.secure
        && !n.role.is_text_input()
        && !matches!(n.role, Role::Keyboard | Role::Key)
        && !(matches!(n.role, Role::StaticText | Role::Image)
            && (n.label.is_some() || n.value.is_some()))
        && n.children.iter().all(oracle_free)
}

/// The oracle's canonical form of a subtree: every hashed field (the frame as the digest
/// sees it: clipped to the screen, rounded; no secure value), children in raw order except
/// that each maximal run of consecutive order-free children is sorted. Inside an order-free
/// subtree every child is order-free, so all of its children are sorted.
fn canon(n: &UiNode, out: &mut String) {
    let screen = IPHONE_SIZE.to_rect();
    let frame = n
        .frame
        .intersection(&screen)
        .map(|f| [f.x, f.y, f.width, f.height].map(|v| v.round() as i64));
    let secure = n.secure || n.role == Role::SecureTextField;
    out.push_str(&format!(
        "({}|{:?}|{:?}|{:?}|{:?}|{:?}|{}{}{}{}",
        n.role.as_str(),
        frame,
        n.label,
        if secure { None } else { n.value.as_ref() },
        n.identifier,
        n.placeholder,
        n.enabled,
        n.selected,
        secure,
        n.visible
    ));
    let mut i = 0;
    while i < n.children.len() {
        if oracle_free(&n.children[i]) {
            let mut run = Vec::new();
            while i < n.children.len() && oracle_free(&n.children[i]) {
                let mut s = String::new();
                canon(&n.children[i], &mut s);
                run.push(s);
                i += 1;
            }
            run.sort();
            out.push_str(&format!("[run {}]", run.join(",")));
        } else {
            canon(&n.children[i], out);
            i += 1;
        }
    }
    out.push(')');
}

fn oracle(s: &Screen) -> String {
    let mut out = String::new();
    canon(&s.root, &mut out);
    out
}

/// Paths (child indices from the root) of every node with two or more children.
fn parents_of(n: &UiNode, at: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
    if n.children.len() >= 2 {
        out.push(at.clone());
    }
    for (i, c) in n.children.iter().enumerate() {
        at.push(i);
        parents_of(c, at, out);
        at.pop();
    }
}

fn at_path<'a>(n: &'a mut UiNode, path: &[usize]) -> &'a mut UiNode {
    path.iter().fold(n, |n, &i| &mut n.children[i])
}

fn shuffle<T>(r: &mut Rng, v: &mut [T]) {
    for i in (1..v.len()).rev() {
        let j = r.below(i as u64 + 1) as usize;
        v.swap(i, j);
    }
}

/// The maximal runs of order-free children of `n`, as index ranges (oracle's view).
fn free_runs(n: &UiNode) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < n.children.len() {
        if oracle_free(&n.children[i]) {
            let s = i;
            while i < n.children.len() && oracle_free(&n.children[i]) {
                i += 1;
            }
            out.push(s..i);
        } else {
            i += 1;
        }
    }
    out
}

/// The binding property of digest v6: digest equality is exactly oracle equality, over
/// random reorders (anywhere, and inside runs), near-misses, cross-sibling and
/// cross-parent moves; and an equal digest leaves the whole `CompactView` equal (screen,
/// sources, bars, covers, inputs). Mutation-checked: hashing every sibling list as a
/// multiset fails it (reported in the commit).
#[test]
fn order_free_reorders_leave_the_compact_view_equal() {
    let mut r = Rng(0x2545_F491_4F6C_DD1D);
    let n = trees(ORDER_TREES);
    let mut kinds = [[0usize; 2]; 5];
    let mut failures = Vec::new();
    for t in 0..n {
        let s = tree_runs(&mut r);
        let mut parents = Vec::new();
        parents_of(&s.root, &mut Vec::new(), &mut parents);
        if parents.is_empty() {
            continue;
        }
        let path = parents[r.below(parents.len() as u64) as usize].clone();
        let mut m = s.clone();
        let kind = r.below(5) as usize;
        {
            let p = at_path(&mut m.root, &path);
            let runs = free_runs(p);
            match kind {
                // Any reorder of one sibling list.
                0 => shuffle(&mut r, &mut p.children),
                // A reorder inside one order-free run (or, without one, anywhere).
                1 => match runs.iter().find(|x| x.len() >= 2) {
                    Some(x) => shuffle(&mut r, &mut p.children[x.clone()]),
                    None => shuffle(&mut r, &mut p.children),
                },
                // A member swapped with its neighbour (possibly across a non-free sibling).
                2 => {
                    let i = r.below(p.children.len() as u64 - 1) as usize;
                    p.children.swap(i, i + 1);
                }
                // A near-miss: one node of a run's member is no longer order-free.
                3 => {
                    if let Some(x) = runs.first() {
                        let g = &mut p.children[x.start];
                        let mut k = r.below(count(g) as u64) as usize;
                        let node = nth(g, &mut k).unwrap();
                        if r.chance(50) {
                            node.visible = true;
                        } else {
                            node.frame = Rect::new(20.0, 20.0, 40.0, 40.0);
                        }
                    }
                    shuffle(&mut r, &mut p.children);
                }
                // A member moved into its next sibling (another parent and depth).
                _ => {
                    let i = r.below(p.children.len() as u64 - 1) as usize;
                    let moved = p.children.remove(i);
                    p.children[i].children.push(moved);
                }
            }
        }
        let same_digest = screen_digest(&s) == screen_digest(&m);
        let same_oracle = oracle(&s) == oracle(&m);
        kinds[kind][usize::from(same_oracle)] += 1;
        if same_digest != same_oracle {
            failures.push(format!(
                "tree {t}, kind {kind}: digest equal {same_digest}, oracle equal {same_oracle}"
            ));
            continue;
        }
        if same_digest {
            let (a, b) = (compact_view(&s), compact_view(&m));
            if a.screen != b.screen
                || a.sources != b.sources
                || a.bars != b.bars
                || a.nodes != b.nodes
                || a.inputs != b.inputs
            {
                failures.push(format!(
                    "tree {t}, kind {kind}: equal digest, compact differs"
                ));
            }
        }
    }
    eprintln!("{n} trees; per kind [oracle different, oracle equal]: {kinds:?}");
    assert!(
        failures.is_empty(),
        "{} failures; first: {:?}",
        failures.len(),
        failures.iter().take(3).collect::<Vec<_>>()
    );
    // The sweep exercised both sides of the rule.
    assert!(kinds[1][1] > n / 20, "{kinds:?}");
    assert!(kinds[0][0] > n / 20 && kinds[3][0] > 0 && kinds[4][0] > n / 20);
}
