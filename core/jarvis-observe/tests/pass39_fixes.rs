#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 39: the cannot-be-read flag (an intruder, a mixed-script look-alike word, a
//! drawn format character, past the length or soft-boundary bound; StaticText and Image
//! included), the emoji edge table, the multi-letter fold and numbers as one word,
//! measured on every text of every real tree (the reviewer's survey: six roles, two apps).
//!
//! `JARVIS_SURVEY_DUMP=<path>` writes every tap's category, decision under the default
//! policy and flag (one line per text, role and app) so two commits can be compared line
//! by line; the counts are asserted here as measured on this commit.

mod common;

use std::collections::BTreeSet;

use common::ios::parse_screen;
use jarvis_proto::{Action, CompactScreen, Element, Policy, Rect, RiskCategory, Role, UiNode};
use jarvis_safety::classify;
use jarvis_safety::reference::cannot_be_read;
use jarvis_safety::{evaluate, Decision};

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

/// The real-tree survey: 970 texts in six roles and two apps (11,640 taps). Measured on
/// the reviewer's pass-39 tree (through the same dump): 130 `Unverified`, 1,621 taps of a
/// category, 1,649 asked under the default policy. On this tree: the same 130, 1,621 and
/// 1,649, with the identical dump line by line (no label changed its category or its
/// decision), and no tap is flagged.
#[test]
fn real_tree_texts_flagged_and_asked() {
    let (trees, all) = real_texts();
    let (mut taps, mut unverified, mut category, mut asked, mut flagged) = (0, 0, 0, 0, 0);
    let mut lines = Vec::new();
    let mut flagged_texts = BTreeSet::new();
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
                let a = Action::Tap { element: 1 };
                let c = classify(&a, &s, None);
                let d = match evaluate(&Policy::default(), &a, &s, None) {
                    Decision::Allow => "Allow".to_owned(),
                    Decision::Deny(_) => "Deny".to_owned(),
                    Decision::Confirm(r) => format!("Ask {:?} {}", r.category, r.phone_only),
                };
                let f = cannot_be_read(&a, &s);
                taps += 1;
                unverified += usize::from(c == RiskCategory::Unverified);
                category += usize::from(c != RiskCategory::None);
                asked += usize::from(d != "Allow");
                flagged += usize::from(f);
                if f {
                    flagged_texts.insert(t.clone());
                }
                lines.push(format!("{t:?}\t{role:?}\t{bundle}\t{c:?}\t{d}\t{f}"));
            }
        }
    }
    if let Ok(path) = std::env::var("JARVIS_SURVEY_DUMP") {
        std::fs::write(path, lines.join("\n")).unwrap();
    }
    println!(
        "{trees} trees, {} texts, {taps} taps, {unverified} Unverified, {category} of a \
         category, {asked} asked, {flagged} flagged",
        all.len()
    );
    for t in &flagged_texts {
        println!("  flagged: {t:?}");
    }
    assert!(trees >= 142, "{trees}");
    assert_eq!(taps, 11_640);
    assert_eq!(unverified, UNVERIFIED, "Unverified taps");
    assert_eq!(category, CATEGORY, "taps of a category");
    assert_eq!(asked, ASKED, "asked taps");
    assert_eq!(flagged, FLAGGED, "flagged taps");
}

const UNVERIFIED: usize = 130;
const CATEGORY: usize = 1_621;
const ASKED: usize = 1_649;
const FLAGGED: usize = 0;

/// No real text is flagged in any role the gate reads the words of (an image, a key and a
/// menu item besides the survey's), nor as a field's label and placeholder typed into with
/// Return.
#[test]
fn no_real_text_is_flagged_in_any_role() {
    let (_, all) = real_texts();
    let mut flagged = Vec::new();
    for t in &all {
        for role in [Role::Image, Role::Key, Role::Other("MenuItem".to_owned())] {
            let s = screen_with(t, role.clone(), "com.example.app");
            if cannot_be_read(&Action::Tap { element: 1 }, &s) {
                flagged.push(format!("{role:?} {t:?}"));
            }
        }
        let mut s = screen_with("", Role::TextField, "com.example.app");
        if let Some(f) = s.elements.first_mut() {
            f.label = t.clone();
            f.placeholder = Some(t.clone());
            f.focused = true;
        }
        let a = Action::Type {
            element: Some(1),
            text: "hello".to_owned(),
            submit: true,
        };
        if cannot_be_read(&a, &s) {
            flagged.push(format!("field {t:?}"));
        }
    }
    println!("{} flagged: {flagged:?}", flagged.len());
    assert!(flagged.is_empty());
}
