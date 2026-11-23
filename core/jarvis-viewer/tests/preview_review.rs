//! Review probes (approval preview, P5): hostile trees and actions never panic, never give
//! a non-finite or off-screen outline, and stay bounded.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

mod common;

use common::*;
use jarvis_proto as p;
use jarvis_viewer::{ApprovalMark, Update};

fn req() -> p::ConfirmRequest {
    p::ConfirmRequest {
        category: p::RiskCategory::Payment,
        summary: "x".to_owned(),
        app: "com.apple.mobilemail".to_owned(),
        phone_only: false,
        ..Default::default()
    }
}

fn picture(event: p::Event, action: p::Action) -> Option<jarvis_viewer::ApprovalTarget> {
    let mut run = Run::new("t");
    run.observe(event);
    run.number += 1;
    run.add(Update::Effect {
        effect: p::Effect::Confirm(req()),
        number: run.number,
    });
    run.add(Update::Progress(progress(1, Some(action))));
    run.add(Update::ConfirmationPending(req()));
    let r = run.reduce();
    r.approval().cloned()
}

fn check(mark: &ApprovalMark) {
    match mark {
        ApprovalMark::Outline { frame, unit } => {
            for v in [
                frame.x,
                frame.y,
                frame.width,
                frame.height,
                unit.x,
                unit.y,
                unit.width,
                unit.height,
            ] {
                assert!(v.is_finite(), "{mark:?}");
            }
            assert!(unit.x >= 0.0 && unit.y >= 0.0, "{mark:?}");
            assert!(
                unit.x + unit.width <= 1.0 + 1e-9 && unit.y + unit.height <= 1.0 + 1e-9,
                "{mark:?}"
            );
            assert!(unit.width > 0.0 && unit.height > 0.0, "{mark:?}");
        }
        ApprovalMark::Point { point, unit } => {
            assert!(point.x.is_finite() && point.y.is_finite(), "{mark:?}");
            assert!((0.0..=1.0).contains(&unit.x) && (0.0..=1.0).contains(&unit.y));
        }
        ApprovalMark::Nothing => {}
    }
}

#[test]
fn probe_hostile_frames_and_sizes_never_panic_or_mark_off_screen() {
    let frames = [
        rect(f64::NAN, 10.0, 100.0, 40.0),
        rect(10.0, 10.0, f64::INFINITY, 40.0),
        rect(-1e308, -1e308, f64::MAX, f64::MAX),
        rect(100.0, 100.0, -50.0, -50.0),
        rect(0.0, 0.0, 0.0, 0.0),
        rect(390.0, 849.0, 1e9, 1e9),
        rect(-500.0, 400.0, 520.0, 40.0),
    ];
    let sizes = [
        PHONE,
        p::Size {
            width: 0.0,
            height: 0.0,
        },
        p::Size {
            width: f64::NAN,
            height: 852.0,
        },
        p::Size {
            width: -393.0,
            height: 852.0,
        },
        p::Size {
            width: 1e300,
            height: 1e300,
        },
    ];
    let actions = [
        p::Action::Tap { element: 1 },
        p::Action::Tap { element: 2 },
        p::Action::Tap { element: u32::MAX },
        p::Action::Tap { element: 0 },
        p::Action::TapPoint {
            x: f64::NAN,
            y: 0.5,
        },
        p::Action::TapPoint { x: 1.0, y: 1.0 },
        p::Action::TapPoint { x: -0.0, y: 0.0 },
        p::Action::Type {
            element: None,
            text: "x".to_owned(),
            submit: false,
        },
    ];
    for size in sizes {
        for f in frames {
            let root = node(
                p::Role::Application,
                Some("A"),
                rect(0.0, 0.0, 393.0, 852.0),
            )
            .children(vec![
                node(p::Role::Button, Some("Pay"), f),
                node(p::Role::TextField, Some("Note"), f).focused(true),
            ])
            .build();
            for a in &actions {
                let event = observed_full(
                    root.clone(),
                    size,
                    false,
                    Some("Mail"),
                    "com.apple.mobilemail",
                );
                if let Some(t) = picture(event, a.clone()) {
                    check(&t.mark);
                }
            }
        }
    }
}

#[test]
fn probe_deep_and_wide_trees_are_bounded() {
    // 500-deep chain (2,000 overflows the 2 MiB test stack in the harness's recursive clone/drop, also without any confirmation: pre-existing) and 20,000 siblings: the picture is computed, without panic.
    let mut deep = node(p::Role::Button, Some("Pay"), rect(16.0, 700.0, 361.0, 50.0)).build();
    for _ in 0..500 {
        let mut parent =
            p::UiNode::new(p::Role::Other("Group".into()), rect(0.0, 0.0, 393.0, 852.0));
        parent.children = vec![deep];
        deep = parent;
    }
    let t = picture(observed(deep), p::Action::Tap { element: 1 });
    if let Some(t) = t {
        check(&t.mark);
    }
    let wide = node(
        p::Role::Application,
        Some("A"),
        rect(0.0, 0.0, 393.0, 852.0),
    )
    .children(
        (0..20_000)
            .map(|i| {
                node(
                    p::Role::Button,
                    Some("B"),
                    rect(0.0, f64::from(i % 800), 10.0, 10.0),
                )
            })
            .collect(),
    )
    .build();
    let start = std::time::Instant::now();
    let t = picture(observed(wide), p::Action::Tap { element: 19_999 });
    assert!(start.elapsed().as_secs() < 20, "{:?}", start.elapsed());
    if let Some(t) = t {
        check(&t.mark);
    }
}

fn deep_tree(depth: usize) -> p::UiNode {
    let mut deep = node(p::Role::Button, Some("Pay"), rect(16.0, 700.0, 361.0, 50.0)).build();
    for _ in 0..depth {
        let mut parent =
            p::UiNode::new(p::Role::Other("Group".into()), rect(0.0, 0.0, 393.0, 852.0));
        parent.children = vec![deep];
        deep = parent;
    }
    deep
}

/// Baseline: the same deep tree observed with no confirmation (pre-existing paths only).
#[test]
fn probe_deep_tree_observed_without_confirmation() {
    let mut run = Run::new("t");
    run.observe(observed(deep_tree(500)));
    let r = run.reduce();
    assert!(r.approval().is_none());
}

/// The core's own compaction of that tree (what the session does on every observation).
#[test]
fn probe_deep_tree_compacted_by_the_core() {
    let screen = p::Screen {
        app_bundle_id: "a".into(),
        app_name: None,
        size: PHONE,
        root: deep_tree(500),
        keyboard_visible: false,
        orientation: p::Orientation::Portrait,
    };
    let c = jarvis_observe::compact(&screen);
    assert!(c.elements.len() <= 1);
}
