#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 35: the look-alike readings of risk words (`DeIete`, `Rem0ve`, `Send M0ney`)
//! on every real tree in the repository. Every captured tree under `*/tests/fixtures` is
//! compacted and every element tapped and long-pressed: a reading that gives a category the
//! element's own words do not (other than none) would change what real labels are asked
//! about. None may.

mod common;

use common::ios::parse_screen;
use jarvis_observe::compact;
use jarvis_proto::{Action, RiskCategory};
use jarvis_safety::label_risk;
use jarvis_safety::reference::{lookalike_readings, tap_reading_risks};

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

#[test]
fn no_real_label_changes_category_by_a_lookalike_reading() {
    let mut trees = 0usize;
    let mut elements = 0usize;
    let mut with_readings: Vec<String> = Vec::new();
    let mut changed: Vec<String> = Vec::new();
    for (path, text) in real_trees() {
        // Probe round 3 captures add the raw snapshot after the typed tree.
        let typed = text
            .split("\n== raw XCUITest snapshot")
            .next()
            .unwrap_or_default();
        let screen = match parse_screen(typed) {
            Ok(s) => s,
            Err(e) => {
                println!(
                    "not a typed tree: {path}: {}",
                    e.chars().take(100).collect::<String>()
                );
                continue;
            }
        };
        trees += 1;
        let c = compact(&screen);
        for el in &c.elements {
            elements += 1;
            for action in [
                Action::Tap { element: el.index },
                Action::LongPress {
                    element: el.index,
                    duration_ms: 800,
                },
            ] {
                let risks = tap_reading_risks(&action, &c);
                let Some((plain, rest)) = risks.split_first() else {
                    continue;
                };
                if matches!(action, Action::Tap { .. }) {
                    with_readings.push(format!("{:?} {:?}", el.role, el.label));
                }
                if rest.iter().any(|r| *r != RiskCategory::None && r != plain) {
                    changed.push(format!(
                        "{path}: {:?} {:?} value={:?}: {plain:?} -> {rest:?}",
                        el.role, el.label, el.value
                    ));
                }
            }
            // The control's own words through the action phrases (Rule C's reading).
            for t in std::iter::once(&el.label).chain(el.value.as_ref()) {
                let plain = label_risk(t);
                for r in lookalike_readings(t).into_iter().flatten() {
                    let got = label_risk(&r);
                    if got != RiskCategory::None && got != plain {
                        changed.push(format!(
                            "{path}: text {t:?} reads {r:?}: {plain:?} -> {got:?}"
                        ));
                    }
                }
            }
        }
    }
    println!(
        "{trees} real trees, {elements} elements, {} with a look-alike reading:",
        with_readings.len()
    );
    for l in &with_readings {
        println!("  {l}");
    }
    assert!(trees >= 100, "only {trees} real trees found");
    assert!(changed.is_empty(), "{}", changed.join("\n"));
}
