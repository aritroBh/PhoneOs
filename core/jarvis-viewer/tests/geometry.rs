//! Ported from `Shell/Tests/JarvisKitTests/LiveViewGeometryTests.swift`: point → view
//! mapping, the cursor path and the scrubber (times in milliseconds: the Swift tests' `10.5`
//! seconds are `10_500` here).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

mod common;

use common::close;
use jarvis_viewer::cursor::{self, Phase};
use jarvis_viewer::{
    Gesture, ReplayState, ScreenSource, Speed, StepState, ViewerMapping, ViewerPoint, ViewerRect,
    ViewerScreen, ViewerSize, ViewerStep, ViewerTimeline,
};

fn pt(x: f64, y: f64) -> ViewerPoint {
    ViewerPoint::new(x, y)
}

fn size(w: f64, h: f64) -> ViewerSize {
    ViewerSize::new(w, h)
}

fn close_pt(a: ViewerPoint, b: ViewerPoint, tolerance: f64) -> bool {
    close(a.x, b.x, tolerance) && close(a.y, b.y, tolerance)
}

// MARK: - Mapping

#[test]
fn mapping_portrait_letterbox() {
    let m = ViewerMapping::new(size(400.0, 800.0), size(600.0, 400.0));
    assert!(close(m.scale, 0.5, 1e-9));
    assert!(close_pt(m.origin, pt(200.0, 0.0), 1e-9));
    assert_eq!(m.content_rect(), ViewerRect::new(200.0, 0.0, 200.0, 400.0));
    assert!(close_pt(m.to_view(pt(0.0, 0.0)), pt(200.0, 0.0), 1e-9));
    assert!(close_pt(
        m.to_view(pt(400.0, 800.0)),
        pt(400.0, 400.0),
        1e-9
    ));
    assert!(close_pt(
        m.to_view(pt(200.0, 400.0)),
        pt(300.0, 200.0),
        1e-9
    ));
}

#[test]
fn mapping_landscape_letterbox() {
    let screen = size(852.0, 393.0);
    assert!(screen.is_landscape());
    let m = ViewerMapping::new(screen, size(426.0, 600.0));
    assert!(close(m.scale, 0.5, 1e-9));
    assert!(close(m.origin.x, 0.0, 1e-9));
    assert!(close(m.origin.y, (600.0 - 393.0 * 0.5) / 2.0, 1e-9));
    assert!(close_pt(
        m.to_view(pt(852.0, 393.0)),
        pt(426.0, m.origin.y + 196.5),
        1e-9
    ));
    assert!(screen.display_aspect() > 1.0);
}

#[test]
fn mapping_clamping() {
    let m = ViewerMapping::new(size(400.0, 800.0), size(600.0, 400.0));
    assert!(close_pt(m.to_view(pt(-100.0, -5.0)), pt(200.0, 0.0), 1e-9));
    assert!(close_pt(
        m.to_view(pt(9_999.0, 1e300)),
        pt(400.0, 400.0),
        1e-9
    ));
    assert!(close_pt(
        m.to_view(pt(f64::NAN, f64::INFINITY)),
        pt(300.0, 200.0),
        1e-9
    ));
    let inside = m.content_rect();
    for p in [
        pt(-1e9, 3.0),
        pt(f64::NAN, f64::NAN),
        pt(5.0, -f64::INFINITY),
    ] {
        let v = m.to_view(p);
        assert!(
            v.x >= inside.x && v.x <= inside.max_x() && v.y >= inside.y && v.y <= inside.max_y()
        );
    }
}

#[test]
fn mapping_degenerate_sizes() {
    for bad in [
        size(0.0, 0.0),
        size(-10.0, 800.0),
        size(f64::NAN, 1.0),
        size(f64::INFINITY, 800.0),
    ] {
        assert!(!bad.is_usable());
        let m = ViewerMapping::new(bad, size(393.0, 852.0));
        assert_eq!(m.screen, ViewerSize::FALLBACK_PHONE);
        assert!(close(m.scale, 1.0, 1e-9));
        assert!(m.to_view(pt(10.0, 10.0)).is_finite());
    }
    for view in [size(0.0, 0.0), size(-5.0, -5.0), size(f64::NAN, 100.0)] {
        let m = ViewerMapping::new(size(400.0, 800.0), view);
        assert_eq!(m.scale, 0.0);
        assert!(m.to_view(pt(100.0, 100.0)).is_finite());
        assert_eq!(m.to_screen(pt(0.0, 0.0)), None);
        assert!(m
            .rect_to_view(&ViewerRect::new(0.0, 0.0, 10.0, 10.0))
            .is_none_or(|r| r.width == 0.0 && r.height == 0.0));
    }
    assert_eq!(size(1.0, 10_000.0).display_aspect(), 0.3);
    assert_eq!(size(10_000.0, 1.0).display_aspect(), 3.0);
}

#[test]
fn mapping_rects() {
    let m = ViewerMapping::new(size(400.0, 800.0), size(400.0, 800.0));
    assert_eq!(
        m.rect_to_view(&ViewerRect::new(-50.0, 790.0, 100.0, 100.0)),
        Some(ViewerRect::new(0.0, 790.0, 50.0, 10.0))
    );
    assert_eq!(
        m.rect_to_view(&ViewerRect::new(500.0, 0.0, 10.0, 10.0)),
        None
    );
    assert_eq!(m.rect_to_view(&ViewerRect::new(0.0, 0.0, 0.0, 10.0)), None);
    assert_eq!(
        m.rect_to_view(&ViewerRect::new(f64::NAN, 0.0, 10.0, 10.0)),
        None
    );
}

#[test]
fn mapping_inverse() {
    let m = ViewerMapping::new(size(393.0, 852.0), size(300.0, 500.0));
    for p in [
        pt(0.0, 0.0),
        pt(196.5, 426.0),
        pt(393.0, 852.0),
        pt(12.25, 700.0),
    ] {
        let back = m.to_screen(m.to_view(p)).unwrap();
        assert!(close_pt(back, p, 1e-6));
    }
    assert_eq!(
        m.to_screen(pt(0.0, 0.0)),
        None,
        "the letterbox is not on the screen"
    );
}

#[test]
fn rect_containment() {
    let r = ViewerRect::new(10.0, 10.0, 10.0, 10.0);
    assert!(r.contains(pt(10.0, 10.0)));
    assert!(!r.contains(pt(20.0, 15.0)));
    assert!(!r.contains(pt(f64::NAN, 15.0)));
    assert!(!ViewerRect::new(0.0, 0.0, -5.0, 5.0).contains(pt(-1.0, 1.0)));
}

// MARK: - Cursor path

const SCREEN: ViewerSize = ViewerSize::FALLBACK_PHONE;

#[test]
fn cursor_ease() {
    assert_eq!(cursor::ease(0.0), 0.0);
    assert_eq!(cursor::ease(1.0), 1.0);
    assert_eq!(cursor::ease(-3.0), 0.0);
    assert_eq!(cursor::ease(7.0), 1.0);
    assert_eq!(cursor::ease(f64::NAN), 0.0);
    let mut last = -1.0;
    for i in 0..=1_000 {
        let v = cursor::ease(f64::from(i) / 1_000.0);
        assert!(v >= last);
        last = v;
    }
}

#[test]
fn cursor_tap() {
    let rest = pt(50.0, 700.0);
    let target = pt(300.0, 120.0);
    let gesture = Gesture::Tap(target);
    let timing = cursor::timing(&gesture, rest, SCREEN, false);
    assert!(close(
        timing.glide_ms,
        cursor::glide_ms(rest.distance(target)),
        1e-9
    ));
    assert_eq!(timing.action_ms, cursor::TAP_MS);
    assert_eq!(
        cursor::pose(&gesture, rest, SCREEN, 0.0, false).position,
        rest
    );
    // Monotone progress: the distance to the target never grows.
    let mut last_distance = f64::INFINITY;
    for i in 0..=200 {
        let t = timing.glide_ms * f64::from(i) / 200.0;
        let d = cursor::pose(&gesture, rest, SCREEN, t, false)
            .position
            .distance(target);
        assert!(d <= last_distance + 1e-9);
        last_distance = d;
    }
    let pressing = cursor::pose(&gesture, rest, SCREEN, timing.glide_ms + 1.0, false);
    assert!(
        pressing.phase == Phase::Pressing && pressing.is_pressed() && pressing.position == target
    );
    let end = cursor::pose(&gesture, rest, SCREEN, timing.total_ms(), false);
    assert!(end.phase == Phase::Resting && end.position == target && end.progress == 1.0);
    assert_eq!(
        cursor::pose(&gesture, rest, SCREEN, 1e12, false).position,
        target
    );
    assert_eq!(cursor::resting_point_after(&gesture, rest, SCREEN), target);
}

#[test]
fn cursor_glide_duration() {
    assert_eq!(cursor::glide_ms(0.0), cursor::GLIDE_RANGE_MS.0);
    assert_eq!(cursor::glide_ms(1e9), cursor::GLIDE_RANGE_MS.1);
    assert_eq!(cursor::glide_ms(f64::NAN), cursor::GLIDE_RANGE_MS.0);
    assert_eq!(cursor::glide_ms(-40.0), cursor::GLIDE_RANGE_MS.0);
    assert!(cursor::glide_ms(300.0) > cursor::glide_ms(200.0));
}

#[test]
fn cursor_swipe() {
    let rest = pt(196.0, 426.0);
    let from = pt(196.0, 700.0);
    let to = pt(196.0, 200.0);
    let gesture = Gesture::Swipe {
        from,
        to,
        duration_ms: 400,
    };
    let timing = cursor::timing(&gesture, rest, SCREEN, false);
    assert_eq!(timing.action_ms, 400.0);
    assert_eq!(
        cursor::pose(&gesture, rest, SCREEN, timing.glide_ms, false).position,
        from
    );
    let mut last_y = f64::INFINITY;
    for i in 0..=100 {
        let pose = cursor::pose(
            &gesture,
            rest,
            SCREEN,
            timing.glide_ms + 400.0 * f64::from(i) / 100.0,
            false,
        );
        assert!(pose.position.y <= last_y + 1e-9, "moves up only");
        last_y = pose.position.y;
    }
    assert_eq!(
        cursor::pose(&gesture, rest, SCREEN, timing.total_ms(), false).position,
        to
    );
    let short = Gesture::Swipe {
        from,
        to,
        duration_ms: 1,
    };
    assert_eq!(
        cursor::timing(&short, rest, SCREEN, false).action_ms,
        cursor::SWIPE_RANGE_MS.0
    );
    let long = Gesture::Swipe {
        from,
        to,
        duration_ms: 90_000,
    };
    assert_eq!(
        cursor::timing(&long, rest, SCREEN, false).action_ms,
        cursor::SWIPE_RANGE_MS.1
    );
}

#[test]
fn cursor_long_press() {
    let rest = pt(10.0, 10.0);
    let p = pt(100.0, 100.0);
    let gesture = Gesture::LongPress {
        point: p,
        duration_ms: 1_000,
    };
    let timing = cursor::timing(&gesture, rest, SCREEN, false);
    assert_eq!(timing.action_ms, 1_000.0);
    let half = cursor::pose(&gesture, rest, SCREEN, timing.glide_ms + 500.0, false);
    assert!(half.phase == Phase::Pressing && close(half.progress, 0.5, 1e-9) && half.position == p);
    let zero = Gesture::LongPress {
        point: p,
        duration_ms: 0,
    };
    assert_eq!(
        cursor::timing(&zero, rest, SCREEN, false).action_ms,
        cursor::LONG_PRESS_RANGE_MS.0
    );
    let max = Gesture::LongPress {
        point: p,
        duration_ms: u64::MAX,
    };
    assert_eq!(
        cursor::timing(&max, rest, SCREEN, false).action_ms,
        cursor::LONG_PRESS_RANGE_MS.1
    );
}

#[test]
fn cursor_parked() {
    let rest = pt(120.0, 330.0);
    for gesture in [
        Gesture::Type(Some("hi".into())),
        Gesture::Type(None),
        Gesture::Key("Return".into()),
        Gesture::Launch("Settings".into()),
        Gesture::OpenUrl("example.com".into()),
        Gesture::Wait(800),
    ] {
        for t in [0.0, 100.0, 5_000.0] {
            let pose = cursor::pose(&gesture, rest, SCREEN, t, false);
            assert!(pose.position == rest && pose.phase == Phase::Parked && !pose.is_pressed());
        }
        assert_eq!(cursor::resting_point_after(&gesture, rest, SCREEN), rest);
        assert_eq!(cursor::timing(&gesture, rest, SCREEN, false).glide_ms, 0.0);
    }
}

#[test]
fn cursor_clamping() {
    let gesture = Gesture::Tap(pt(-500.0, 5_000.0));
    let rest = pt(f64::NAN, f64::NAN);
    let start = cursor::pose(&gesture, rest, SCREEN, -10.0, false);
    assert_eq!(start.position, SCREEN.center());
    assert_eq!(cursor::pose(&gesture, rest, SCREEN, f64::NAN, false), start);
    let end = cursor::pose(&gesture, rest, SCREEN, 1e9, false);
    assert_eq!(end.position, pt(0.0, 852.0));
    // A zero-size screen draws on the fallback phone.
    let zero = cursor::pose(
        &Gesture::Tap(pt(10.0, 10.0)),
        ViewerPoint::ZERO,
        size(0.0, 0.0),
        1e9,
        false,
    );
    assert_eq!(zero.position, pt(10.0, 10.0));
}

#[test]
fn cursor_reduce_motion() {
    let rest = pt(0.0, 0.0);
    let p = pt(300.0, 600.0);
    let pose = cursor::pose(&Gesture::Tap(p), rest, SCREEN, 0.0, true);
    assert!(pose.position == p && pose.phase == Phase::Pressing);
    let swipe = Gesture::Swipe {
        from: p,
        to: pt(300.0, 100.0),
        duration_ms: 300,
    };
    assert_eq!(
        cursor::pose(&swipe, rest, SCREEN, 10.0, true).position,
        pt(300.0, 100.0)
    );
}

#[test]
fn cursor_continuity() {
    let screen = ViewerScreen {
        id: 1,
        size: SCREEN,
        app_name: Some("A".into()),
        bundle_id: "a".into(),
        source: ScreenSource::Tree,
        elements: vec![],
        elements_truncated: false,
        frame_key: None,
        focused_input_secure: None,
    };
    let gestures = [
        Gesture::Tap(pt(30.0, 40.0)),
        Gesture::Type(None),
        Gesture::Swipe {
            from: pt(200.0, 700.0),
            to: pt(200.0, 100.0),
            duration_ms: 300,
        },
        Gesture::Key("Return".into()),
        Gesture::LongPress {
            point: pt(999.0, -4.0),
            duration_ms: 600,
        },
        Gesture::Wait(100),
        Gesture::Tap(pt(196.0, 426.0)),
    ];
    let steps: Vec<ViewerStep> = gestures
        .into_iter()
        .enumerate()
        .map(|(i, g)| ViewerStep {
            number: u32::try_from(i).unwrap() + 1,
            core_step: None,
            screen_id: Some(1),
            gesture: g,
            target: None,
            caption: String::new(),
            state: StepState::Done,
        })
        .collect();
    let timeline = ViewerTimeline {
        task_id: "t".into(),
        steps: steps.clone(),
        screens: vec![screen],
        current_screen_id: Some(1),
        ..ViewerTimeline::default()
    };
    assert_eq!(
        cursor::resting_point_before(1, &timeline),
        SCREEN.center(),
        "the first step glides in from the middle"
    );
    for pair in steps.windows(2) {
        let end_a = cursor::pose_of_step(pair[0].number, &timeline, 1e9, None, false).unwrap();
        let start_b = cursor::pose_of_step(pair[1].number, &timeline, 0.0, None, false).unwrap();
        assert!(
            close_pt(end_a.position, start_b.position, 1e-9),
            "jump between step {} and {}",
            pair[0].number,
            pair[1].number
        );
    }
    assert_eq!(
        cursor::resting_point_after_latest(&timeline),
        pt(196.0, 426.0)
    );
    assert_eq!(cursor::pose_of_step(99, &timeline, 0.0, None, false), None);
}

// MARK: - Replay controls

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn replay_follows() {
    let mut r = ReplayState::new();
    r.sync(&[], 0.0, None);
    assert_eq!(r.current(), None);
    r.sync(&[1], 1_000.0, None);
    assert!(r.current() == Some(1) && r.is_following() && r.started_at_ms() == 1_000.0);
    r.sync(&[1], 2_000.0, None);
    assert_eq!(r.started_at_ms(), 1_000.0, "the same step does not restart");
    r.sync(&[1, 2], 3_000.0, None);
    assert!(r.current() == Some(2) && r.started_at_ms() == 3_000.0);
    assert_eq!(r.fraction(&[1, 2]), 1.0);
}

#[test]
fn replay_scrubbed_back_stays() {
    let mut r = ReplayState::new();
    r.sync(&[1, 2, 3], 0.0, None);
    r.step_back(&[1, 2, 3], 1_000.0);
    assert!(r.current() == Some(2) && !r.is_following() && !r.is_playing());
    r.step_back(&[1, 2, 3], 1_000.0);
    r.step_back(&[1, 2, 3], 1_000.0);
    assert_eq!(r.current(), Some(1), "stops at the oldest");
    r.sync(&[1, 2, 3, 4, 5], 2_000.0, None);
    assert_eq!(r.current(), Some(1), "not yanked forward");
    r.step_forward(&[1, 2, 3, 4, 5], 3_000.0);
    assert!(r.current() == Some(2) && !r.is_following());
    r.go_live(&[1, 2, 3, 4, 5], 4_000.0);
    assert!(r.current() == Some(5) && r.is_following() && r.started_at_ms() == 4_000.0);
    r.sync(&[1, 2, 3, 4, 5, 6], 5_000.0, None);
    assert_eq!(r.current(), Some(6));
}

#[test]
fn replay_forward_end() {
    let mut r = ReplayState::new();
    r.sync(&[1, 2], 0.0, None);
    r.step_forward(&[1, 2], 1_000.0);
    assert!(r.current() == Some(2) && !r.is_following());
    r.sync(&[1, 2, 3], 2_000.0, None);
    assert_eq!(r.current(), Some(2));
}

#[test]
fn replay_play() {
    let mut r = ReplayState::new();
    let numbers = [4, 5, 6];
    r.sync(&numbers, 0.0, None);
    r.toggle_play(&numbers, 10_000.0);
    assert!(
        r.is_playing() && !r.is_following() && r.current() == Some(4),
        "at the newest step, play starts from the oldest"
    );
    let duration = |_: u32| 1_000.0;
    assert!(!r.advance(&numbers, 10_500.0, duration));
    assert!(r.advance(&numbers, 11_000.0, duration));
    assert!(r.current() == Some(5) && r.started_at_ms() == 11_000.0);
    r.set_speed(Speed::Double, 11_000.0);
    assert!(near(r.elapsed_ms(11_250.0), 500.0));
    assert!(r.advance(&numbers, 11_500.0, duration));
    assert_eq!(r.current(), Some(6));
    assert!(!r.advance(&numbers, 20_000.0, duration));
    assert!(
        !r.is_playing() && r.current() == Some(6),
        "stops at the newest"
    );
    r.toggle_play(&numbers, 21_000.0);
    r.toggle_play(&numbers, 21_100.0);
    assert!(!r.is_playing() && r.current() == Some(4));
}

#[test]
fn replay_speed_keeps_position() {
    let mut r = ReplayState::new();
    r.sync(&[1, 2], 0.0, None);
    r.scrub_to(1, &[1, 2], 10_000.0);
    assert!(near(r.elapsed_ms(10_400.0), 400.0));
    r.set_speed(Speed::Double, 10_400.0);
    assert!(near(r.elapsed_ms(10_400.0), 400.0));
    assert!(near(r.elapsed_ms(10_500.0), 600.0));
    assert!(Speed::Normal.next() == Speed::Double && Speed::Double.title() == "2×");
}

#[test]
fn replay_evicted() {
    let mut r = ReplayState::new();
    r.sync(&[1, 2, 3], 0.0, None);
    r.scrub_to(1, &[1, 2, 3], 1_000.0);
    r.sync(&[3, 4, 5], 2_000.0, None);
    assert!(r.current() == Some(3) && !r.is_following());
    r.sync(&[], 3_000.0, None);
    assert!(r.current().is_none() && !r.is_playing());
}

#[test]
fn replay_scrubber() {
    let mut r = ReplayState::new();
    let numbers = [10, 11, 12, 13, 14];
    r.sync(&numbers, 0.0, None);
    r.scrub_fraction(0.5, &numbers, 1_000.0);
    assert!(r.current() == Some(12) && r.fraction(&numbers) == 0.5);
    r.scrub_fraction(f64::NAN, &numbers, 1_000.0);
    assert_eq!(r.current(), Some(10));
    r.scrub_fraction(7.0, &numbers, 1_000.0);
    assert_eq!(r.current(), Some(14));
    r.scrub_to(99, &numbers, 2_000.0);
    assert_eq!(r.current(), Some(14));
    let mut empty = ReplayState::new();
    empty.step_back(&[], 0.0);
    empty.toggle_play(&[], 0.0);
    empty.scrub_fraction(0.3, &[], 0.0);
    assert!(empty.current().is_none() && !empty.is_playing() && empty.is_following());
    assert_eq!(empty.elapsed_ms(f64::NAN), 0.0);
}
