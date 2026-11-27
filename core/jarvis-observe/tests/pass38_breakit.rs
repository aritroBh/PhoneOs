#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 38: what the pass-37 normalisation (commit `341f0de`) does to the ask counts
//! of ordinary real-tree texts.
//!
//! Every label and value of every node of every typed-tree fixture of the workspace is put
//! on a one-control screen in six roles and two apps and tapped. The counts of taps that
//! are `Unverified` and of taps that ask at all are printed and held to what was measured
//! on this commit and on `93982bb` (the commit before the normalisation; measured by
//! running this same file against that commit's `jarvis-safety/src`): an `Unverified`
//! question on an ordinary label is one an attacker can use to train the owner to approve.

mod common;

use std::collections::BTreeSet;

use common::ios::parse_screen;
use jarvis_proto::{Action, CompactScreen, Element, Rect, RiskCategory, Role, UiNode};
use jarvis_safety::classify;

fn walk(dir: std::path::PathBuf, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if name == "target" || name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            walk(p, out);
        } else {
            let s = p.to_string_lossy().to_string();
            if s.contains("/tests/fixtures/") && s.ends_with(".txt") {
                if let Ok(t) = std::fs::read_to_string(&p) {
                    out.push(t);
                }
            }
        }
    }
}

fn texts(n: &UiNode, out: &mut BTreeSet<String>) {
    for t in [&n.label, &n.value].into_iter().flatten() {
        if !t.trim().is_empty() {
            out.insert(t.clone());
        }
    }
    for c in &n.children {
        texts(c, out);
    }
}

fn screen_with(label: &str, role: Role, bundle: &str) -> CompactScreen {
    CompactScreen {
        app_bundle_id: bundle.to_owned(),
        app_name: Some("Example".to_owned()),
        size: jarvis_proto::fixtures::IPHONE_SIZE,
        keyboard_visible: false,
        elements: vec![Element {
            index: 1,
            role,
            label: label.to_owned(),
            value: None,
            placeholder: None,
            frame: Rect::new(16.0, 120.0, 361.0, 44.0),
            enabled: true,
            focused: false,
            selected: false,
            secure: false,
            fingerprint: 1,
        }],
        cues: Default::default(),
    }
}

/// Sound (counts): the real-tree texts tapped in six roles and two apps. Measured on this
/// commit: 970 distinct labels and values from 142 trees (11,640 taps), 130 taps
/// `Unverified` (unknown SF Symbol names such as `1.circle.fill` and other-script texts, as
/// Button, Cell, Link, Switch), 1,621 asked; on `93982bb` the same file gives the same
/// 130 and 1,621, with the identical list: the normalisation made no ordinary real text
/// `Unverified` and asked nothing new.
#[test]
fn sound_unverified_and_ask_counts_on_real_trees() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let mut files = Vec::new();
    walk(root, &mut files);
    let mut all = BTreeSet::new();
    let mut trees = 0usize;
    for text in &files {
        let typed = text
            .split("\n== raw XCUITest snapshot")
            .next()
            .unwrap_or_default();
        let Ok(screen) = parse_screen(typed) else {
            continue;
        };
        trees += 1;
        texts(&screen.root, &mut all);
    }
    let (mut taps, mut unverified, mut asked) = (0usize, 0usize, 0usize);
    let mut listed = Vec::new();
    for t in &all {
        for role in [
            Role::Button,
            Role::Cell,
            Role::Link,
            Role::StaticText,
            Role::Switch,
            Role::Tab,
        ] {
            for bundle in ["com.example.app", "com.apple.Preferences"] {
                let s = screen_with(t, role.clone(), bundle);
                let c = classify(&Action::Tap { element: 1 }, &s, None);
                taps += 1;
                if c == RiskCategory::Unverified {
                    unverified += 1;
                    listed.push(format!("{t:?} {role:?} {bundle}"));
                }
                if c != RiskCategory::None {
                    asked += 1;
                }
            }
        }
    }
    println!(
        "{trees} trees, {} texts, {taps} taps, {unverified} Unverified, {asked} asked",
        all.len()
    );
    for l in &listed {
        println!("  {l}");
    }
    assert!(trees >= 142, "{trees}");
    assert!(
        unverified <= 130,
        "{unverified} Unverified taps (130 measured)"
    );
    assert!(asked <= 1621, "{asked} asked taps (1,621 measured)");
}
