#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 38: the intruder rule, the compatibility fold, the removed email exemption,
//! the soft line breaks and the soft punctuation inside a word, measured on
//! every text of every real tree (the reviewer's survey: six roles, two apps).
//!
//! `JARVIS_SURVEY_DUMP=<path>` writes every tap's category (one line per text, role and
//! app) so two commits can be compared line by line; the counts are asserted here as
//! measured on this commit.

mod common;

use std::collections::BTreeSet;

use common::ios::parse_screen;
use jarvis_proto::{Action, CompactScreen, Element, Rect, RiskCategory, Role, UiNode};
use jarvis_safety::classify;
use jarvis_safety::reference::{has_intruder, soft_boundaries};

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

/// The real-tree survey: 970 texts in six roles and two apps. Measured on `24cd1a0` (the
/// reviewer's commit): 130 `Unverified`, 1,621 asked. On this commit: the same 130 and
/// 1,621, with the identical list of taps (compared line by line through
/// `JARVIS_SURVEY_DUMP`): the intruder rule, the compatibility fold, the soft line breaks,
/// the soft punctuation and the email change made no real text `Unverified` and asked
/// nothing new.
#[test]
fn real_tree_texts_ask_exactly_as_before() {
    let (trees, all) = real_texts();
    let (mut taps, mut unverified, mut asked) = (0usize, 0usize, 0usize);
    let mut lines = Vec::new();
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
                unverified += usize::from(c == RiskCategory::Unverified);
                asked += usize::from(c != RiskCategory::None);
                lines.push(format!("{t:?}\t{role:?}\t{bundle}\t{c:?}"));
            }
        }
    }
    if let Ok(path) = std::env::var("JARVIS_SURVEY_DUMP") {
        std::fs::write(path, lines.join("\n")).unwrap();
    }
    println!(
        "{trees} trees, {} texts, {taps} taps, {unverified} Unverified, {asked} asked",
        all.len()
    );
    assert!(trees >= 142, "{trees}");
    assert_eq!(unverified, 130, "Unverified taps");
    assert_eq!(asked, 1621, "asked taps");
}

/// No real text holds an intruder in a word with an ASCII letter (the words the intruder
/// rule reads); the soft-boundary bound keeps its margin with line breaks soft.
#[test]
fn no_real_text_holds_an_intruder() {
    let (_, all) = real_texts();
    let with: Vec<&String> = all.iter().filter(|t| has_intruder(t)).collect();
    println!("{} texts with an intruder", with.len());
    for t in &with {
        println!("  {t:?}");
    }
    assert!(with.is_empty());
    let max = all.iter().map(|t| soft_boundaries(t)).max().unwrap_or(0);
    println!("most soft boundaries {max}");
    assert!(2 * max <= jarvis_safety::reference::MAX_SOFT_BOUNDARIES);
}
