//! `JarvisViewer` from Rust: a real session's effects and events forwarded one call per
//! driver update (in the driver's order), the light timeline, screens on demand, the
//! encoded bytes, replay and the per-frame scene record.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

mod common;

use common::*;
use jarvis_core_ffi::*;

fn viewer() -> std::sync::Arc<JarvisViewer> {
    JarvisViewer::new(CoreViewerConfig {
        max_steps: 200,
        frames_enabled: true,
        known_apps: vec![AppInfo {
            bundle_id: SETTINGS.to_owned(),
            name: "Settings".to_owned(),
        }],
    })
    .unwrap()
}

/// Drives the Settings → Wi-Fi session and forwards every update to `viewer` the way the
/// shell's driver reports them: `.started`, then per effect `.effect`, `.progress`, the
/// effect's own update, `.effectCompleted`; `.finished` last. Returns the feed count.
fn drive_viewer(viewer: &JarvisViewer) -> usize {
    drive_viewer_with(viewer, |_| {})
}

/// [`drive_viewer`], calling `on_confirm` with the viewer right after each
/// `.confirmationPending` was fed.
fn drive_viewer_with(viewer: &JarvisViewer, mut on_confirm: impl FnMut(&JarvisViewer)) -> usize {
    drive_updates(
        |u| {
            viewer.feed(u).unwrap();
        },
        || on_confirm(viewer),
    )
}

#[test]
fn a_session_forwarded_one_call_per_update() {
    let v = viewer();
    let feeds = drive_viewer(&v);
    assert!(feeds > 10);
    let t = v.timeline().unwrap();
    assert_eq!(t.task_id, "t1");
    assert_eq!(t.steps.len(), 2, "{:?}", t.steps);
    assert_eq!(t.steps[0].caption, "Tap \u{201C}Wi-Fi\u{201D}");
    assert!(matches!(t.steps[0].gesture, CoreViewerGesture::Tap { .. }));
    assert_eq!(t.steps[0].target.as_ref().unwrap().role, "StaticText");
    assert_eq!(
        t.steps[1].caption, "Tap",
        "a runner-text screen has no wireframe"
    );
    assert!(t.steps.iter().all(|s| s.state == CoreViewerStepState::Done));
    assert_eq!(
        t.steps.iter().map(|s| s.core_step).collect::<Vec<_>>(),
        vec![Some(1), Some(2)]
    );
    assert_eq!(
        t.activity,
        CoreViewerActivity::Finished {
            outcome: CoreViewerOutcome::Success
        }
    );
    assert_eq!(t.activity_caption, "Done");
    // Screens: summaries in the timeline, wireframes on demand.
    let first = t
        .screens
        .iter()
        .find(|s| Some(s.id) == t.steps[0].screen_id)
        .unwrap();
    assert_eq!(first.source, CoreViewerSource::Tree);
    assert_eq!(first.display_name, "Settings");
    assert_eq!(first.frame_key.as_ref().unwrap().task_id, "t1");
    let wire = v.screen(first.id).unwrap().unwrap();
    assert_eq!(wire.elements.len() as u32, first.element_count);
    assert!(wire
        .elements
        .iter()
        .any(|e| e.label.as_deref() == Some("Wi-Fi")));
    let second = t
        .screens
        .iter()
        .find(|s| Some(s.id) == t.steps[1].screen_id)
        .unwrap();
    assert_eq!(second.source, CoreViewerSource::RunnerText);
    // Preview review: a runner-text screen now has a wireframe from the session's parse
    // (was: none), flagged `from_text`; labels only, never values or ids.
    assert!(second.from_text && !first.from_text);
    let text_wire = v.screen(second.id).unwrap().unwrap();
    assert_eq!(text_wire.elements.len() as u32, second.element_count);
    assert!(text_wire
        .elements
        .iter()
        .any(|e| e.role == "Switch" && e.label.as_deref() == Some("Wi-Fi")));
    assert!(v.screen(9_999).unwrap().is_none());
    // The bytes: versioned, no raw runner text, decodable into another viewer.
    let bytes = v.encode().unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains(r#""version":1"#));
    assert!(!text.contains("BackButton") && !text.contains("{{0, 0}"));
    assert_eq!(viewer_wire_version(), 1);
    let remote = JarvisViewer::decoded(
        bytes.clone(),
        CoreViewerConfig {
            max_steps: 200,
            frames_enabled: false,
            known_apps: vec![],
        },
    )
    .unwrap();
    assert_eq!(remote.encode().unwrap(), bytes);
    assert_eq!(remote.timeline().unwrap().steps, t.steps);
    let bad = JarvisViewer::decoded(
        b"{\"version\":9}".to_vec(),
        CoreViewerConfig {
            max_steps: 1,
            frames_enabled: false,
            known_apps: vec![],
        },
    );
    assert!(matches!(bad, Err(JarvisError::InvalidJson { .. })));
}

#[test]
fn scene_and_replay_records() {
    let v = viewer();
    drive_viewer(&v);
    let q = |now_ms: f64| CoreSceneQuery {
        now_ms,
        reduce_motion: false,
        view_width: 393.0 / 2.0,
        view_height: 852.0 / 2.0,
        image_width: 786.0,
        image_height: 1704.0,
    };
    // The first frame syncs the scrubber: the newest step plays.
    let s = v.scene(q(10_000.0)).unwrap();
    assert_eq!(s.step_number, Some(2));
    assert!(s.needs_frames && s.following && !s.playing);
    assert_eq!(s.scale, 0.5);
    assert_eq!(s.content, rect(0.0, 0.0, 196.5, 426.0));
    assert_eq!(s.image_in_view, Some(rect(0.0, 0.0, 196.5, 426.0)));
    assert_eq!(s.position_count, 2);
    // Played out, the run finished: static, idle cursor on the latest screen.
    let s = v.scene(q(60_000.0)).unwrap();
    assert!(s.step_number.is_none() && !s.needs_frames && s.is_finished);
    assert_eq!(s.caption, "Done");
    assert_eq!(s.tone, CoreSceneTone::Success);
    assert_eq!(
        s.cursor_in_view,
        Point {
            x: s.cursor.x / 2.0,
            y: s.cursor.y / 2.0
        }
    );
    // Step back: replay of the first step, with its position and target outline.
    let status = v
        .replay(CoreReplayAction::StepBack, 70_000.0, false)
        .unwrap();
    assert_eq!(status.position_text, "Step 1 of 2");
    assert!(!status.following && status.current == Some(1));
    let s = v.scene(q(70_000.0 + 10_000.0)).unwrap();
    assert_eq!(s.step_number, Some(1));
    assert_eq!(s.position_index, Some(1));
    assert_eq!(s.phase, CoreCursorPhase::Resting);
    assert_eq!(
        s.accessibility_value,
        "Tap \u{201C}Wi-Fi\u{201D}, Step 1 of 2"
    );
    assert!(s.target_in_view.is_some());
    let status = v
        .replay(CoreReplayAction::NextSpeed, 80_000.0, false)
        .unwrap();
    assert_eq!(status.speed, CoreReplaySpeed::Double);
    assert_eq!(status.speed_title, "2×");
    let status = v.replay(CoreReplayAction::GoLive, 80_000.0, false).unwrap();
    assert_eq!(status.position_text, "Live · 2 steps");
    // NaN and absurd views are handled.
    let s = v
        .scene(CoreSceneQuery {
            now_ms: f64::NAN,
            reduce_motion: true,
            view_width: -1.0,
            view_height: f64::INFINITY,
            image_width: 0.0,
            image_height: 0.0,
        })
        .unwrap();
    assert!(s.cursor_in_view.x.is_finite() && s.scale == 0.0 && s.image_in_view.is_none());
}

#[test]
fn feed_reports_changes_and_tolerates_bad_tool_json() {
    let v = viewer();
    let c = v
        .feed(CoreViewerUpdate::Started {
            task_id: "x".into(),
        })
        .unwrap();
    assert!(c.steps && c.generation == 1);
    let c = v
        .feed(CoreViewerUpdate::Effect {
            effect: Effect::CallTool {
                call: ToolCall {
                    name: "clock_now".into(),
                    args_json: "not json".into(),
                },
            },
            number: 1,
        })
        .unwrap();
    assert!(c.activity && !c.steps && !c.screens);
    assert_eq!(
        v.timeline().unwrap().activity,
        CoreViewerActivity::UsingTool {
            name: "clock_now".into()
        }
    );
    let before = v.generation().unwrap();
    let c = v.feed(CoreViewerUpdate::Ignored).unwrap();
    assert!(!c.steps && !c.screens && !c.activity && c.generation == before);
    // A progress whose action is a tool call with bad JSON still resolves nothing safely.
    v.feed(CoreViewerUpdate::Progress {
        progress: SessionProgress {
            step: 1,
            phase: Phase::Acting,
            last_action: Some(Action::CallTool {
                name: "t".into(),
                args_json: "{".into(),
            }),
            app: None,
            model_calls: 0,
            parse_retries: 0,
            waiting_for_unlock: false,
            locked_wait_ms: 0,
            locked_total_ms: 0,
            fresh_looks: 0,
            stale_plans: 0,
            screen_locked: false,
            screen_hidden: false,
        },
    })
    .unwrap();
}

#[test]
fn secrets_on_screen_never_reach_the_records_or_bytes() {
    let v = viewer();
    v.feed(CoreViewerUpdate::Started {
        task_id: "s".into(),
    })
    .unwrap();
    let mut screen = settings_screen();
    screen.root.children[0].children.push(UiNode {
        placeholder: Some("Card 4111 1111 1111 1111".into()),
        value: Some("hunter2-SECRET".into()),
        ..ui(
            Role::TextField,
            Some("Your code is 482913"),
            rect(16.0, 400.0, 361.0, 44.0),
            true,
            vec![],
        )
    });
    v.feed(CoreViewerUpdate::Effect {
        effect: Effect::Observe { screenshot: false },
        number: 1,
    })
    .unwrap();
    v.feed(CoreViewerUpdate::EffectCompleted {
        effect: Effect::Observe { screenshot: false },
        event: Event::Observed {
            observation: ScreenObservation {
                screen,
                screenshot: None,
                device_locked: false,
            },
        },
    })
    .unwrap();
    // Runner text with a secret in its raw source: parsed for approval lookups only, never
    // kept in the timeline, a screen or the bytes.
    v.feed(CoreViewerUpdate::EffectCompleted {
        effect: Effect::Observe { screenshot: false },
        event: Event::ObservedRunnerText {
            snapshot: snapshot("StaticText, label: 'IBAN GB82 WEST 1234 5698 7654 32'".into()),
        },
    })
    .unwrap();
    let t = v.timeline().unwrap();
    let all = format!(
        "{t:?} {:?} {}",
        t.screens
            .iter()
            .map(|s| v.screen(s.id).unwrap())
            .collect::<Vec<_>>(),
        String::from_utf8(v.encode().unwrap()).unwrap()
    );
    for secret in ["482913", "4111 1111", "hunter2", "GB82 WEST"] {
        assert!(!all.contains(secret), "{secret}");
    }
}

/// The approval sheet's picture: a real session's confirmation on a runner-text screen
/// outlines the switch the core will tap (indexed as the core indexed it), bound to the
/// observation the confirmation was computed from; it is gone once the owner answered, and
/// never in the encoded bytes.
#[test]
fn a_confirmation_on_runner_text_outlines_its_target() {
    let v = viewer();
    let mut seen = Vec::new();
    drive_viewer_with(&v, |viewer| {
        let t = viewer.timeline().unwrap();
        let target = viewer.approval_target().unwrap();
        let bytes = String::from_utf8(viewer.encode().unwrap()).unwrap();
        seen.push((t, target, bytes));
    });
    assert!(!seen.is_empty(), "the Wi-Fi switch on runner text is asked");
    let mut runner_text_confirmations = 0;
    for (t, target, bytes) in &seen {
        let target = target.as_ref().expect("a picture while pending");
        let current = t.current_screen_id.unwrap();
        assert_eq!(target.screen_id, current);
        assert_eq!(target.frame_key.task_id, "t1");
        let info = t.screens.iter().find(|s| s.id == current).unwrap();
        assert_eq!(info.frame_key.as_ref(), Some(&target.frame_key));
        // The Wi-Fi row on the typed Settings screen, the switch on the runner-text one.
        let (frame, label) = if info.source == CoreViewerSource::RunnerText {
            runner_text_confirmations += 1;
            ((312.0, 166.0, 51.0, 31.0), "Outlined: Switch")
        } else {
            ((16.0, 200.0, 361.0, 44.0), "Outlined: ")
        };
        assert_eq!(
            target.mark,
            CoreApprovalMark::Outline {
                frame: rect(frame.0, frame.1, frame.2, frame.3),
                unit: Rect {
                    x: frame.0 / 393.0,
                    y: frame.1 / 852.0,
                    width: frame.2 / 393.0,
                    height: frame.3 / 852.0,
                },
            }
        );
        assert!(
            target.accessibility_label.starts_with(label),
            "{}",
            target.accessibility_label
        );
        assert!(!bytes.contains(&target.caption));
        assert!(!bytes.contains("Outlined"));
    }
    assert!(runner_text_confirmations > 0);
    assert_eq!(v.approval_target().unwrap(), None, "cleared once answered");
}
