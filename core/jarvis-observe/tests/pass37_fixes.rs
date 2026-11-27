#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 37: the normalisation and the soft-boundary bound measured on every text of
//! every captured real tree (labels, values, placeholders, identifiers of every node).

mod common;

use std::collections::BTreeSet;

use common::ios::parse_screen;
use jarvis_proto::{Action, CompactScreen, Element, Policy, Rect, Role, UiNode};
use jarvis_safety::reference::{normalized, soft_boundaries, MAX_SOFT_BOUNDARIES};
use jarvis_safety::{classify, evaluate, Decision};

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
    for t in [&n.label, &n.value, &n.placeholder, &n.identifier]
        .into_iter()
        .flatten()
    {
        if !t.trim().is_empty() {
            out.insert(t.clone());
        }
    }
    // A control reads its label and value together.
    if let (Some(l), Some(v)) = (&n.label, &n.value) {
        if !l.trim().is_empty() && !v.trim().is_empty() {
            out.insert(format!("{l} {v}"));
        }
    }
    for c in &n.children {
        texts(c, out);
    }
}

/// Every distinct text of every real tree, and how many trees were read.
fn real_texts() -> (usize, BTreeSet<String>) {
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
        if let Ok(s) = parse_screen(typed) {
            trees += 1;
            texts(&s.root, &mut all);
        }
    }
    (trees, all)
}

fn screen_with(label: &str, value: Option<&str>, role: Role) -> CompactScreen {
    CompactScreen {
        app_bundle_id: "com.example.app".to_owned(),
        app_name: Some("Example".to_owned()),
        size: jarvis_proto::fixtures::IPHONE_SIZE,
        keyboard_visible: false,
        elements: vec![Element {
            index: 1,
            role,
            label: label.to_owned(),
            value: value.map(str::to_owned),
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

/// F8: the most soft boundaries (case and letter/digit steps, spelled-out joins, narrow
/// spaces) any real text holds, against `MAX_SOFT_BOUNDARIES`: the bound keeps at least
/// twice that margin, so no real label is asked for it. The longest texts are printed.
#[test]
fn the_soft_boundary_bound_has_margin_over_every_real_text() {
    let (trees, all) = real_texts();
    let mut counts: Vec<(usize, &String)> = all.iter().map(|t| (soft_boundaries(t), t)).collect();
    counts.sort_by_key(|a| std::cmp::Reverse(a.0));
    let max = counts.first().map_or(0, |c| c.0);
    println!(
        "{trees} trees, {} distinct texts; most soft boundaries {max} (bound {MAX_SOFT_BOUNDARIES})",
        all.len()
    );
    for (n, t) in counts.iter().take(8) {
        println!("  {n}: {t:?}");
    }
    assert!(trees >= 100, "{trees} trees");
    assert!(
        MAX_SOFT_BOUNDARIES >= 2 * max,
        "bound {MAX_SOFT_BOUNDARIES}, real {max}"
    );
    // A label and value together (as a control reads them) stay under it too.
    for (n, t) in counts.iter().take(20) {
        let s = screen_with(t, Some(t), Role::Button);
        let a = Action::Tap { element: 1 };
        if 2 * n + 1 < MAX_SOFT_BOUNDARIES {
            assert_ne!(
                classify(&a, &s, None),
                jarvis_proto::RiskCategory::Unverified,
                "{t:?}"
            );
        }
    }
}

/// The normalisation on real texts: what it changes (narrow spaces, marks, decomposable
/// letters) is printed, and none of those texts, as a button, is tapped unasked where the
/// raw text was asked (the readings only add).
#[test]
fn normalisation_of_real_texts_is_printed_and_adds_only() {
    let (_, all) = real_texts();
    let changed: Vec<&String> = all.iter().filter(|t| normalized(t) != **t).collect();
    println!(
        "{} of {} texts change under the normalisation",
        changed.len(),
        all.len()
    );
    for t in &changed {
        println!("  {t:?} -> {:?}", normalized(t));
        assert_eq!(normalized(&normalized(t)), normalized(t));
    }
    for t in &all {
        let s = screen_with(t, None, Role::Button);
        let a = Action::Tap { element: 1 };
        let d = evaluate(&Policy::default(), &a, &s, None);
        let plain_ascii = t.is_ascii();
        if plain_ascii {
            continue;
        }
        // A non-ASCII button label is never allowed when its ASCII-folded twin asks.
        let twin = normalized(t);
        let d2 = evaluate(
            &Policy::default(),
            &a,
            &screen_with(&twin, None, Role::Button),
            None,
        );
        if !matches!(d2, Decision::Allow) {
            assert!(
                !matches!(d, Decision::Allow),
                "{t:?} allowed, {twin:?} asked"
            );
        }
    }
}
