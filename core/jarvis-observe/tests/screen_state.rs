#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod common;

use common::*;
use jarvis_observe::{
    compact, diff, find_by_fingerprint, marks, resolve, screen_hash, ResolveError,
};
use jarvis_proto::fixtures::{login_screen, settings_screen, text};
use jarvis_proto::{Role, UiNode};

fn clock_screen(time: &str, battery: &str) -> jarvis_proto::Screen {
    app_screen(vec![
        text(time, r(300.0, 14.0, 80.0, 20.0)),
        text(&format!("Battery {battery}"), r(20.0, 14.0, 120.0, 20.0)),
        button("Sync", 20.0, 200.0, 100.0, 44.0),
    ])
}

#[test]
fn hash_is_stable_for_identical_screens_and_ignores_layout_and_focus() {
    let a = compact(&login_screen());
    let b = compact(&login_screen());
    assert_eq!(screen_hash(&a), screen_hash(&b));

    let mut shifted = a.clone();
    for e in &mut shifted.elements {
        e.frame.y += 17.0;
        e.focused = !e.focused;
    }
    assert_eq!(screen_hash(&a), screen_hash(&shifted));
}

#[test]
fn status_bar_chrome_is_ignored_by_hash_and_diff() {
    let a = compact(&clock_screen("9:41", "85%"));
    let b = compact(&clock_screen("9:42", "84%"));
    assert_eq!(screen_hash(&a), screen_hash(&b));
    let d = diff(&a, &b);
    assert!(!d.changed, "{d:?}");
    assert!(d.appeared.is_empty() && d.disappeared.is_empty());
    // A real change next to the ticking clock is still seen.
    let mut c = clock_screen("9:43", "84%");
    c.root.children[0].children[2].label = Some("Retry".into());
    assert!(diff(&b, &compact(&c)).changed);
}

#[test]
fn percentages_and_times_in_real_content_still_count() {
    let slider = |v: &str| {
        compacted(vec![jarvis_proto::UiNode::new(
            Role::Slider,
            r(20.0, 200.0, 300.0, 40.0),
        )
        .with_label("Volume")
        .with_value(v)])
    };
    assert!(diff(&slider("20%"), &slider("80%")).changed);
    assert_ne!(screen_hash(&slider("20%")), screen_hash(&slider("80%")));

    let alarm = |l: &str| compacted(vec![label(l, 20.0, 200.0)]);
    assert!(diff(&alarm("Alarm, 7:00 AM"), &alarm("Alarm, 8:30 AM")).changed);

    let field = |v: &str| {
        compacted(vec![jarvis_proto::UiNode::new(
            Role::TextField,
            r(20.0, 300.0, 300.0, 40.0),
        )
        .with_label("Tip")
        .with_value(v)])
    };
    assert!(diff(&field("15%"), &field("20%")).changed);
}

#[test]
fn hash_changes_on_real_changes() {
    let base = compact(&login_screen());
    let h0 = screen_hash(&base);

    let mut typed = base.clone();
    typed.elements[1].value = Some("sam@example.com".into());
    assert_ne!(h0, screen_hash(&typed), "typed text");

    let mut enabled = base.clone();
    enabled.elements[3].enabled = true;
    assert_ne!(h0, screen_hash(&enabled), "button enabled");

    let mut relabelled = base.clone();
    relabelled.elements[3].label = "Sign up".into();
    assert_ne!(h0, screen_hash(&relabelled), "label");

    let mut no_kb = base.clone();
    no_kb.keyboard_visible = false;
    assert_ne!(h0, screen_hash(&no_kb), "keyboard");

    let mut other_app = base.clone();
    other_app.app_bundle_id = "com.example.other".into();
    assert_ne!(h0, screen_hash(&other_app), "app");

    let mut fewer = base.clone();
    fewer.elements.pop();
    assert_ne!(h0, screen_hash(&fewer), "element removed");
}

#[test]
fn diff_reports_appeared_disappeared_and_app_change() {
    let before = compact(&login_screen());
    let same = diff(&before, &before.clone());
    assert!(!same.changed && !same.app_changed);
    assert!(same.appeared.is_empty() && same.disappeared.is_empty());

    // The login form is replaced by an error alert.
    let alert = UiNode::new(Role::Alert, r(60.0, 330.0, 273.0, 190.0)).with_children(vec![
        label("Wrong password", 80.0, 350.0),
        button("OK", 70.0, 450.0, 120.0, 44.0),
    ]);
    let after = compacted_app(&login_screen(), alert);
    let d = diff(&before, &after);
    assert!(d.changed);
    assert!(!d.app_changed);
    let appeared: Vec<&str> = d.appeared.iter().map(|e| e.label.as_str()).collect();
    let gone: Vec<&str> = d.disappeared.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(appeared, vec!["Wrong password", "OK"]);
    assert_eq!(
        gone,
        vec![
            "Welcome back",
            "Email",
            "Password",
            "Sign in",
            "Forgot password?"
        ]
    );

    let settings = compact(&settings_screen());
    let d = diff(&before, &settings);
    assert!(d.app_changed && d.changed);
}

fn compacted_app(base: &jarvis_proto::Screen, extra: UiNode) -> jarvis_proto::CompactScreen {
    let mut s = base.clone();
    s.root.children[0].children.push(extra);
    compact(&s)
}

#[test]
fn diff_flags_value_only_change_as_changed_without_appearances() {
    let before = compact(&login_screen());
    let mut after = before.clone();
    after.elements[1].value = Some("other@example.com".into());
    let d = diff(&before, &after);
    assert!(d.changed);
    assert!(d.appeared.is_empty() && d.disappeared.is_empty());
}

#[test]
fn diff_ignores_pure_scroll_of_the_same_elements() {
    let before = compact(&settings_screen());
    let mut after = before.clone();
    for e in &mut after.elements {
        e.frame.y += 3.0;
    }
    assert!(!diff(&before, &after).changed);
}

#[test]
fn marks_mirror_elements_in_index_order() {
    let c = compact(&login_screen());
    let m = marks(&c);
    assert_eq!(m.len(), c.elements.len());
    for (mark, e) in m.iter().zip(&c.elements) {
        assert_eq!(mark.index, e.index);
        assert_eq!(mark.frame, e.frame);
    }
    assert_eq!(m.first().map(|m| m.index), Some(1));
}

#[test]
fn resolve_returns_elements_and_precise_errors() {
    let c = compact(&login_screen());
    assert_eq!(resolve(&c, 2).unwrap().label, "Email");
    assert_eq!(resolve(&c, 0), Err(ResolveError::Zero));
    assert_eq!(
        resolve(&c, 6),
        Err(ResolveError::OutOfRange { index: 6, count: 5 })
    );
    assert!(matches!(
        resolve(&c, u32::MAX),
        Err(ResolveError::OutOfRange { .. })
    ));
    let msg = resolve(&c, 99).unwrap_err().to_string();
    assert!(msg.contains("99") && msg.contains('5'), "{msg}");

    let empty = compacted(vec![]);
    assert!(resolve(&empty, 1).is_err());
}

#[test]
fn find_by_fingerprint_survives_reindexing() {
    let before = compact(&login_screen());
    let target = before.elements[3].clone();
    let mut after = before.clone();
    after.elements.remove(0);
    for (i, e) in after.elements.iter_mut().enumerate() {
        e.index = (i + 1) as u32;
    }
    let found = find_by_fingerprint(&after, target.fingerprint).unwrap();
    assert_eq!(found.label, target.label);
    assert_ne!(found.index, target.index);
    assert!(find_by_fingerprint(&after, 42).is_none());
}
