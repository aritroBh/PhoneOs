//! `LiveView`: the reducer, scrubber and scene behind one value (what the FFI object
//! wraps). Not a port: the Swift view did this wiring itself.

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
use jarvis_viewer::{
    cursor, privacy::LabelCache, LiveView, Phase, ReplayAction, Speed, Tone, Update, ViewerPoint,
    ViewerSize,
};

fn feed(live: &mut LiveView, run: &Run) {
    for u in &run.updates {
        live.apply(u.clone());
    }
}

#[test]
fn follows_live_then_replays_with_one_call_per_frame() {
    let mut run = Run::new("t");
    run.observe(observed(login_tree(None)));
    run.act(tap(100.0, 420.0));
    let mut live = LiveView::new(config(200, false));
    feed(&mut live, &run);
    assert_eq!(live.numbers(), &[1]);
    assert_eq!(live.position_text(), "Live · 1 step");
    // The first query syncs the scrubber at its own time: the step plays from there.
    let f = live.frame(1_000.0, false);
    assert_eq!(f.scene.step.map(|s| s.number), Some(1));
    assert!(f.needs_frames && f.tone == Tone::Acting);
    assert_eq!(f.scene.pose.phase, Phase::Gliding);
    // A second step arrives mid-glide: it starts where the cursor is.
    let mid = live.frame(1_100.0, false).scene.pose.position;
    let mut more = Run::new("t");
    more.updates.clear();
    more.number = run.number;
    more.core_step = run.core_step;
    more.act(tap(300.0, 700.0));
    feed(&mut live, &more);
    let f = live.frame(1_100.0, false);
    assert_eq!(f.scene.step.map(|s| s.number), Some(2));
    assert!(f.scene.pose.position.distance(mid) < 1e-9);
    assert_eq!(live.replay().origin(), Some(mid));
    // Replay controls.
    live.control(ReplayAction::StepBack, 2_000.0, false);
    assert_eq!(live.position_text(), "Step 1 of 2");
    let f = live.frame(2_000.0, false);
    assert_eq!(f.scene.position, Some((1, 2)));
    assert_eq!(
        f.scene.accessibility_value(),
        "Tap \u{201C}Send\u{201D}, Step 1 of 2"
    );
    live.control(ReplayAction::NextSpeed, 2_000.0, false);
    assert_eq!(live.replay().speed(), Speed::Double);
    live.control(ReplayAction::TogglePlay, 2_000.0, false);
    assert!(live.replay().is_playing());
    // Playback advances inside the frame query.
    let play = cursor::play_ms_of_step(1, live.timeline(), None, false);
    let f = live.frame(2_000.0 + play / 2.0 + 1.0, false);
    assert_eq!(f.scene.step.map(|s| s.number), Some(2));
    live.control(ReplayAction::GoLive, 9_000.0, false);
    assert!(live.replay().is_following());
    // A new run: a fresh scrubber following live.
    live.control(ReplayAction::StepBack, 9_500.0, false);
    live.apply(Update::Started {
        task_id: "u".into(),
    });
    let f = live.frame(10_000.0, false);
    assert!(f.scene.step.is_none() && live.replay().is_following());
    assert_eq!(live.position_text(), "No steps yet");
}

#[test]
fn view_geometry() {
    let mut run = Run::new("t");
    run.observe(observed(login_tree(None)));
    run.act(tap(100.0, 420.0));
    run.act(p::RunnerCommand::Swipe {
        from: p::Point::new(200.0, 700.0),
        to: p::Point::new(200.0, 200.0),
        duration_ms: 300,
    });
    let mut live = LiveView::new(config(200, false));
    feed(&mut live, &run);
    live.control(ReplayAction::StepBack, 0.0, false);
    // Long after: the tap rests on its target, outlined.
    let f = live.frame(100_000.0, false);
    let g = f.in_view(ViewerSize::new(393.0 / 2.0, 852.0 / 2.0));
    assert_eq!(g.mapping.scale, 0.5);
    assert_eq!(g.cursor, ViewerPoint::new(50.0, 210.0));
    let target = g.target.unwrap();
    assert_eq!(
        (target.x, target.y, target.width, target.height),
        (8.0, 200.0, 180.5, 22.0)
    );
    assert!(g.trail.is_none());
    // The swipe at rest: its whole trail.
    live.control(ReplayAction::StepForward, 200_000.0, false);
    let f = live.frame(300_000.0, false);
    let g = f.in_view(ViewerSize::new(393.0, 852.0));
    assert_eq!(
        g.trail,
        Some((
            ViewerPoint::new(200.0, 700.0),
            ViewerPoint::new(200.0, 200.0)
        ))
    );
}

#[test]
fn label_cache_is_bounded_and_keeps_no_raw_text() {
    let mut cache = LabelCache::new();
    let secret = "Card 4111 1111 1111 1111";
    let shown = cache.label(Some(secret)).unwrap();
    assert!(!shown.contains("4111"));
    assert_eq!(
        cache.label(Some(secret)).unwrap(),
        shown,
        "a hit is the same label"
    );
    assert!(
        !format!("{cache:?}").contains("4111"),
        "only hashes and display labels"
    );
    for i in 0..(LabelCache::CAPACITY + 10) {
        cache.label(Some(&format!("label {i}")));
    }
    assert!(cache.len() <= LabelCache::CAPACITY);
}
