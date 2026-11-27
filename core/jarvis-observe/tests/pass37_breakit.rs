#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 37: false positives of the look-alike character sets (commit `93982bb`) at
//! the scale of every captured real tree.
//!
//! Every label and value of every node of every typed-tree fixture of the workspace (not
//! only the elements the compactor keeps) is put on a one-control screen as a `Button`, a
//! `Cell`, a `Link` and a `StaticText`, in an ordinary app and in Settings, and tapped:
//! `tap_reading_risks` gives the category by the control's own words (the classification
//! before the sets) and by their sets. A label the sets give a category its own words do
//! not is a question the owner did not get before.

mod common;

use std::collections::BTreeSet;

use common::ios::parse_screen;
use jarvis_proto::{Action, CompactScreen, Element, Rect, RiskCategory, Role, UiNode};
use jarvis_safety::reference::tap_reading_risks;

/// Every typed-tree fixture of the workspace (a file the dump parser reads).
fn real_trees() -> Vec<(String, String)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let mut out = Vec::new();
    let mut dirs = vec![root];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            let s = p.to_string_lossy().to_string();
            let name = e.file_name().to_string_lossy().to_string();
            if name == "target" || name.starts_with('.') {
                continue;
            }
            if p.is_dir() {
                dirs.push(p);
            } else if s.contains("/tests/fixtures/") && s.ends_with(".txt") {
                if let Ok(text) = std::fs::read_to_string(&p) {
                    out.push((s, text));
                }
            }
        }
    }
    out.sort();
    out
}

fn texts(n: &UiNode, out: &mut BTreeSet<String>) {
    for t in [&n.label, &n.value, &n.placeholder, &n.identifier]
        .into_iter()
        .flatten()
    {
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

/// Sound: of the distinct texts of every node of every real tree (labels, values,
/// placeholders and identifiers), none gains a category from the look-alike sets that its
/// own words do not give, in any of four roles and two apps. The count and the texts read
/// are printed.
#[test]
fn sound_no_real_tree_text_gains_a_category_from_the_sets() {
    let mut all = BTreeSet::new();
    let mut trees = 0usize;
    for (_, text) in real_trees() {
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
    let mut gained = Vec::new();
    for t in &all {
        for role in [Role::Button, Role::Cell, Role::Link, Role::StaticText] {
            for bundle in ["com.example.app", "com.apple.Preferences"] {
                let risks = tap_reading_risks(
                    &Action::Tap { element: 1 },
                    &screen_with(t, role.clone(), bundle),
                );
                let Some((&old, rest)) = risks.split_first() else {
                    continue;
                };
                for &c in rest {
                    if c != old && c != RiskCategory::None {
                        gained.push(format!("{t:?} {role:?} in {bundle}: {old:?} -> {c:?}"));
                    }
                }
            }
        }
    }
    println!(
        "{trees} trees, {} distinct texts, {} gained a category",
        all.len(),
        gained.len()
    );
    for g in &gained {
        println!("  {g}");
    }
    assert!(trees >= 100, "only {trees} trees");
    assert!(gained.is_empty(), "{}", gained.join("\n"));
}
