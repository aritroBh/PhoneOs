#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Sixteenth review, home screen rule, on the real iOS 26.5 `springboard_home` tree: what
//! its widgets show is not in the tree the model gets, its icons, dock, page dots and search
//! pill are, the digest is still of the raw tree, and the screen is not a read.

use jarvis_observe::{compact, compact_view, estimate_tokens, render, screen_digest};
use jarvis_proto::Role;
use jarvis_safety::{home_view, observed_context, ScreenSource};
use jarvis_sim::bench::fixtures::{default_dir, load_all};

fn home() -> jarvis_proto::Screen {
    load_all(&default_dir())
        .unwrap()
        .into_iter()
        .find(|f| f.name == "springboard_home")
        .unwrap()
        .screen
}

#[test]
fn the_real_home_screen_loses_its_widgets_and_keeps_its_launch_targets() {
    let raw = home();
    let shown = home_view(&raw).unwrap();
    let full = compact(&raw);
    let c = compact(&shown);
    let count = |s: &jarvis_proto::CompactScreen, l: &str| {
        s.elements.iter().filter(|e| e.label == l).count()
    };
    // Widgets and icons with the same name: only the icon is left.
    assert_eq!(count(&full, "Maps"), 2);
    assert_eq!(count(&c, "Maps"), 1);
    assert_eq!(count(&full, "Calendar"), 2);
    assert_eq!(count(&c, "Calendar"), 1);
    assert!(c
        .elements
        .iter()
        .all(|e| !e.value.as_deref().unwrap_or_default().contains("Widget")));
    for icon in [
        "Photos",
        "Reminders",
        "News",
        "Health",
        "Wallet",
        "Settings",
        "Safari",
        "Messages",
    ] {
        assert_eq!(count(&c, icon), 1, "{icon}");
    }
    assert_eq!(c.cues.page, Some((1, 2)));
    // The surviving icons resolve and keep their tap points.
    let settings = c.elements.iter().find(|e| e.label == "Settings").unwrap();
    assert_eq!(settings.role, Role::Button);
    assert!(jarvis_observe::resolve(&c, settings.index).is_ok());
    // The digest is of the raw tree (an approval is pinned to what the phone showed).
    assert_eq!(screen_digest(&raw), screen_digest(&raw.clone()));
    assert_ne!(screen_digest(&raw), screen_digest(&shown));
    // No home screen without an alert or banner is a read, whatever its widgets.
    let view = compact_view(&shown);
    assert!(observed_context(&shown, &view.screen, ScreenSource::Typed).is_none());
}

/// Token ceiling of the real home screen as the model gets it: 93 tokens now, ceiling 100.
/// With its widgets it measures 101 tokens (ceiling 131 in
/// `jarvis-observe/tests/ios26.rs`, measured on the raw tree); the model's view is smaller.
#[test]
fn the_models_view_of_the_real_home_screen_is_smaller() {
    let raw = home();
    let before = estimate_tokens(&render(&compact(&raw), 100_000));
    let after = estimate_tokens(&render(&compact(&home_view(&raw).unwrap()), 100_000));
    println!("springboard_home: {before} -> {after} tokens");
    assert!(after < before, "{after} vs {before}");
    assert!(after <= 100, "{after}");
}
