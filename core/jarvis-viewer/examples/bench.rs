//! Timings of the Live view's hot paths on a 400-element screen and a 200-step timeline.
//!
//! ```text
//! cargo run --release -p jarvis-viewer --example bench
//! ```
//!
//! Prints the mean and worst time per call. Targets: one reducer update for an
//! observation < 1 ms; a per-frame scene query < 20 µs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::hint::black_box;
use std::time::{Duration, Instant};

use jarvis_proto as p;
use jarvis_viewer::{LiveView, ReducerConfig, Update, ViewerSize};

/// A screen with `n` labelled elements in a few containers (labels like a real app's,
/// some with digits and some long, so redaction does real work).
fn screen(n: usize, seed: usize) -> p::UiNode {
    let words = [
        "Inbox",
        "Settings",
        "Wi-Fi",
        "Bluetooth",
        "Order #4821 shipped",
        "Total 42.50",
        "Meeting at 10:30 with Alex",
        "Reply",
        "Archive",
        "Your code is ready",
        "Notifications and sounds for messages",
        "Search",
    ];
    let roles = [
        p::Role::Cell,
        p::Role::Button,
        p::Role::StaticText,
        p::Role::TextField,
        p::Role::Image,
        p::Role::Switch,
    ];
    let mut groups = Vec::new();
    let per_group = 20;
    for g in 0..n.div_ceil(per_group) {
        let mut children = Vec::new();
        for i in 0..per_group {
            let k = g * per_group + i;
            if k >= n {
                break;
            }
            let mut node = p::UiNode::new(
                roles[(k + seed) % roles.len()].clone(),
                p::Rect::new((k % 3) as f64 * 130.0, (k / 3) as f64 * 6.0, 120.0, 5.5),
            )
            .with_label(format!(
                "{} {} {}",
                words[(k + seed) % words.len()],
                k,
                seed
            ))
            .with_value("value");
            if k % 7 == 0 {
                node = node.with_placeholder("Type here");
            }
            node.focused = k == 3;
            children.push(node);
        }
        groups.push(
            p::UiNode::new(p::Role::ScrollView, p::Rect::new(0.0, 0.0, 393.0, 852.0))
                .with_children(children),
        );
    }
    p::UiNode::new(p::Role::Application, p::Rect::new(0.0, 0.0, 393.0, 852.0))
        .with_label("Mail")
        .push_child(
            p::UiNode::new(p::Role::Window, p::Rect::new(0.0, 0.0, 393.0, 852.0))
                .with_children(groups),
        )
}

fn observed(root: p::UiNode) -> p::Event {
    p::Event::Observed(p::Observation::new(p::Screen {
        app_bundle_id: "com.apple.mobilemail".into(),
        app_name: Some("Mail".into()),
        size: p::Size::new(393.0, 852.0),
        root,
        keyboard_visible: false,
        orientation: p::Orientation::Portrait,
    }))
}

struct Stat {
    total: Duration,
    worst: Duration,
    n: u32,
}

impl Stat {
    fn new() -> Self {
        Stat {
            total: Duration::ZERO,
            worst: Duration::ZERO,
            n: 0,
        }
    }
    fn add(&mut self, d: Duration) {
        self.total += d;
        self.worst = self.worst.max(d);
        self.n += 1;
    }
    fn print(&self, what: &str) {
        println!(
            "{what:<52} mean {:>9.2} µs   worst {:>9.2} µs   (n = {})",
            self.total.as_secs_f64() * 1e6 / f64::from(self.n),
            self.worst.as_secs_f64() * 1e6,
            self.n
        );
    }
}

fn redact_only() {
    let root = screen(400, 0);
    let labels: Vec<String> = root.iter().filter_map(|n| n.label.clone()).collect();
    let t = Instant::now();
    for _ in 0..100 {
        for l in &labels {
            black_box(jarvis_safety::redact(l));
        }
    }
    println!(
        "redact {} labels: {:.1} µs",
        labels.len(),
        t.elapsed().as_secs_f64() * 1e6 / 100.0
    );
    let t = Instant::now();
    for _ in 0..100 {
        black_box(jarvis_viewer::reducer::wireframe(
            &root,
            ViewerSize::new(393.0, 852.0),
            &mut jarvis_viewer::privacy::LabelCache::new(),
        ));
    }
    println!(
        "wireframe, cold label cache: {:.1} µs",
        t.elapsed().as_secs_f64() * 1e6 / 100.0
    );
    let mut cache = jarvis_viewer::privacy::LabelCache::new();
    let t = Instant::now();
    for _ in 0..100 {
        black_box(jarvis_viewer::reducer::wireframe(
            &root,
            ViewerSize::new(393.0, 852.0),
            &mut cache,
        ));
    }
    println!(
        "wireframe, warm label cache: {:.1} µs",
        t.elapsed().as_secs_f64() * 1e6 / 100.0
    );
    let t = Instant::now();
    for _ in 0..100 {
        black_box(root.clone());
    }
    println!(
        "clone tree: {:.1} µs",
        t.elapsed().as_secs_f64() * 1e6 / 100.0
    );
}

/// One run of 260 observe + act effects over `screens` (cycled), timing each update.
fn drive(
    screens: &[p::UiNode],
    observe: &mut Stat,
    act: &mut Stat,
    progress: &mut Stat,
) -> LiveView {
    let mut live = LiveView::new(ReducerConfig::default());
    live.apply(Update::Started {
        task_id: "bench".into(),
    });
    let mut number = 0u64;
    // 260 commands: the cap is reached, old steps and screens are dropped on the way.
    for i in 0..260u32 {
        number += 1;
        let effect = p::Effect::Observe { screenshot: false };
        live.apply(Update::Effect {
            effect: effect.clone(),
            number,
        });
        let event = observed(screens[i as usize % screens.len()].clone());
        let t = Instant::now();
        live.apply(Update::EffectCompleted { effect, event });
        observe.add(t.elapsed());

        number += 1;
        let command = if i % 5 == 4 {
            p::RunnerCommand::TypeText {
                text: "hello there".into(),
            }
        } else {
            p::RunnerCommand::Tap {
                x: f64::from(i % 390),
                y: f64::from((i * 7) % 840),
            }
        };
        let effect = p::Effect::Act(command);
        let t = Instant::now();
        live.apply(Update::Effect {
            effect: effect.clone(),
            number,
        });
        act.add(t.elapsed());
        let t = Instant::now();
        live.apply(Update::Progress(p::Progress {
            step: i + 1,
            last_action: Some(p::Action::Type {
                element: None,
                text: "hello there".into(),
                submit: false,
            }),
            ..p::Progress::default()
        }));
        progress.add(t.elapsed());
        live.apply(Update::EffectCompleted {
            effect,
            event: p::Event::ActResult(p::RunnerResult::ok()),
        });
    }
    live
}

fn main() {
    redact_only();
    // Cold: every observation a screen never seen, every label new to the label cache.
    // Warm: the same 8 screens over and over (a screen observed again after each action).
    let cold_screens: Vec<p::UiNode> = (0..260).map(|s| screen(400, 1_000 + s)).collect();
    let warm_screens: Vec<p::UiNode> = (0..8).map(|s| screen(400, s)).collect();
    let mut observe_cold = Stat::new();
    let mut observe_warm = Stat::new();
    let mut act = Stat::new();
    let mut progress = Stat::new();
    drive(&cold_screens, &mut observe_cold, &mut act, &mut progress);
    let mut live = drive(&warm_screens, &mut observe_warm, &mut act, &mut progress);
    let t = live.timeline();
    println!(
        "timeline: {} steps, {} screens, {} elements on the current screen",
        t.steps.len(),
        t.screens.len(),
        t.current_screen().map_or(0, |s| s.elements.len())
    );
    observe_cold.print("reducer: observation, 400 elements, all labels new");
    observe_warm.print("reducer: observation, 400 elements, screen seen before");
    act.print("reducer: act effect (target lookup on 400 elements)");
    progress.print("reducer: progress (typed-text check)");

    // Per-frame scene query: following live, then replaying.
    let mut frame = Stat::new();
    let mut now = 1_000_000.0;
    let view = ViewerSize::new(320.0, 690.0);
    for i in 0..20_000u32 {
        now += 16.7;
        if i == 10_000 {
            live.control(jarvis_viewer::ReplayAction::TogglePlay, now, false);
        }
        let t = Instant::now();
        let f = live.frame(now, false);
        let g = f.in_view(view);
        black_box((&f.scene.caption, f.needs_frames, g.cursor));
        frame.add(t.elapsed());
    }
    frame.print("frame: scene + needs_frames + view geometry");

    let mut encode = Stat::new();
    let mut bytes = Vec::new();
    for _ in 0..50 {
        let t = Instant::now();
        bytes = live.timeline().encode();
        encode.add(t.elapsed());
    }
    encode.print(&format!("encode the timeline ({} KiB)", bytes.len() / 1024));
    let mut decode = Stat::new();
    for _ in 0..50 {
        let t = Instant::now();
        black_box(jarvis_viewer::ViewerTimeline::decode(&bytes).unwrap());
        decode.add(t.elapsed());
    }
    decode.print("decode the timeline");
}
