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
//! same deterministic seed (raise `TREES` locally to run the full sweep).

use jarvis_observe::{compact_view, screen_digest};
use jarvis_proto::fixtures::IPHONE_SIZE;
use jarvis_proto::{Rect, Role, Screen, UiNode};

/// Random trees checked (the review used 200,000).
const TREES: usize = 5_000;

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
    for t in 0..TREES {
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
