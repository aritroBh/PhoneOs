//! The Live view as a delta stream (`delta.rs`): encoder → bytes → decoder rebuilds the
//! phone's timeline (and its scenes) for arbitrary runs; with lost deltas and resyncs it
//! converges; every delta and snapshot stays within its bound; secrets, secure values and
//! locked screens never reach the bytes; arbitrary bytes and orders never panic the
//! decoder nor break its bounds.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::cast_precision_loss
)]

mod common;

use common::random::*;
use common::*;
use jarvis_proto as p;
use jarvis_viewer::delta::{wire_screen, wire_timeline, DeltaBody};
use jarvis_viewer::{
    scene, Applied, DeltaDecoder, DeltaEncoder, LiveView, ReplayState, ResyncReason, Update,
    ViewerDelta, ViewerTimeline, MAX_DELTA_BYTES, MAX_ELEMENTS, MAX_SCREENS, MAX_SNAPSHOT_BYTES,
    MAX_STEPS, SNAPSHOT_STEPS,
};

/// Feeds the encoder and returns the encoded deltas, each checked against the bound.
fn feed(encoder: &mut DeltaEncoder, live: &LiveView) -> Vec<Vec<u8>> {
    encoder
        .feed(live.runs(), live.timeline())
        .iter()
        .map(|d| {
            let bytes = d.encode();
            assert!(bytes.len() <= MAX_DELTA_BYTES, "{} bytes", bytes.len());
            bytes
        })
        .collect()
}

fn snapshot(encoder: &DeltaEncoder) -> Vec<Vec<u8>> {
    let bytes: Vec<Vec<u8>> = encoder.snapshot().iter().map(ViewerDelta::encode).collect();
    let total: usize = bytes.iter().map(Vec::len).sum();
    assert!(total <= MAX_SNAPSHOT_BYTES, "snapshot {total} bytes");
    assert!(bytes.iter().all(|b| b.len() <= MAX_DELTA_BYTES));
    bytes
}

/// What the phone shows, as the wire carries it.
fn normalized(t: &ViewerTimeline) -> ViewerTimeline {
    ViewerTimeline::decode(&wire_timeline(t).encode()).unwrap()
}

/// Scenes compared by their debug text (a NaN coordinate equals itself there).
fn same_scenes(phone: &ViewerTimeline, remote: &ViewerTimeline, rng: &mut Rng) {
    let numbers = phone.step_numbers();
    assert_eq!(numbers, remote.step_numbers());
    for _ in 0..6 {
        let now = rng.f(0.0, 1e6);
        let mut r = ReplayState::new();
        r.sync(&numbers, now, None);
        match rng.below(4) {
            0 => r.step_back(&numbers, now),
            1 => r.scrub_to(
                numbers.first().copied().unwrap_or(1) + u32::try_from(rng.below(8)).unwrap(),
                &numbers,
                now,
            ),
            2 => r.toggle_play(&numbers, now),
            _ => {}
        }
        let at = now + rng.f(0.0, 3_000.0);
        let rm = rng.coin();
        assert_eq!(
            format!("{:?}", scene::make(phone, &r, at, rm)),
            format!("{:?}", scene::make(remote, &r, at, rm))
        );
        assert_eq!(
            scene::needs_frames(phone, &r, at, rm),
            scene::needs_frames(remote, &r, at, rm)
        );
    }
}

/// The relaxed invariants a decoder keeps whatever it is fed.
fn check_bounds(t: &ViewerTimeline) {
    assert!(t.steps.len() <= MAX_STEPS);
    assert!(t.screens.len() <= MAX_SCREENS);
    for w in t.steps.windows(2) {
        assert_eq!(w[1].number, w[0].number + 1, "numbers are consecutive");
    }
    for w in t.screens.windows(2) {
        assert!(w[0].id < w[1].id, "screens in id order");
    }
    for s in &t.screens {
        assert!(s.elements.len() <= MAX_ELEMENTS);
    }
    if let Some(first) = t.steps.first() {
        assert_eq!(u64::from(first.number), u64::from(t.dropped_steps) + 1);
    }
    t.check_bounds().unwrap();
}

/// A run's updates: a random run from the viewer's generators, or arbitrary update orders
/// (interleaved runs, duplicates).
fn random_updates(rng: &mut Rng, secrets: &[&str]) -> Vec<Update> {
    if rng.below(3) == 0 {
        let mut updates = Vec::new();
        for _ in 0..rng.below(200) {
            updates.push(random_update(rng));
        }
        updates
    } else {
        random_run(rng, secrets).updates
    }
}

#[test]
fn the_stream_rebuilds_the_phone_timeline_and_its_scenes() {
    let mut rng = Rng(0x0DE1_7A00_5EED_0001);
    for round in 0..250 {
        let updates = random_updates(&mut rng, &[]);
        let max = usize::try_from(rng.below(40)).unwrap() + if rng.coin() { 0 } else { 200 };
        let mut phone = LiveView::new(config(max, rng.coin()));
        let mut remote = LiveView::default();
        let mut encoder = DeltaEncoder::new();
        // Some updates are not fed (several updates merge into one diff).
        let skip = rng.below(3) == 0;
        let count = updates.len();
        for (i, u) in updates.into_iter().enumerate() {
            phone.apply(u);
            if skip && rng.coin() && i + 1 < count {
                continue;
            }
            for bytes in feed(&mut encoder, &phone) {
                let applied = remote.apply_delta(&bytes);
                assert!(
                    !matches!(applied, Applied::NeedsResync(_) | Applied::Incompatible(_)),
                    "round {round}: {applied:?} for {}",
                    String::from_utf8_lossy(&bytes)
                );
                check_bounds(remote.timeline());
            }
        }
        let want = normalized(phone.timeline());
        assert_eq!(
            String::from_utf8(remote.timeline().encode()).unwrap(),
            String::from_utf8(want.encode()).unwrap(),
            "round {round}"
        );
        same_scenes(&want, remote.timeline(), &mut rng);
        assert!(!remote.needs_resync());
    }
}

#[test]
fn with_lost_deltas_and_resyncs_it_converges() {
    let mut rng = Rng(0x0DE1_7A00_5EED_0002);
    let mut resyncs = 0;
    for round in 0..200 {
        let updates = random_updates(&mut rng, &[]);
        let mut phone = LiveView::new(config(200, rng.coin()));
        let mut remote = ViewerTimeline::default();
        let mut decoder = DeltaDecoder::new();
        let mut encoder = DeltaEncoder::new();
        let loss = rng.below(5) + 1;
        // A controller joining late reports a gap on its first delta (no reset yet).
        let mut asked = false;
        let late = rng.below(10);
        // A delta lost after the last one applied shows no gap until the next arrives;
        // the hub, which is what drops deltas for real, schedules a snapshot then.
        let mut lost_unnoticed = false;
        for (i, u) in updates.into_iter().enumerate() {
            phone.apply(u);
            let deltas = feed(&mut encoder, &phone);
            if (i as u64) < late {
                // Not connected yet.
                continue;
            }
            if asked && rng.below(3) == 0 {
                // The resync request reached the phone: the snapshot comes before newer deltas.
                asked = false;
                lost_unnoticed = false;
                resyncs += 1;
                for bytes in snapshot(&encoder) {
                    assert!(!matches!(
                        decoder.apply(&mut remote, &bytes),
                        Applied::NeedsResync(_)
                    ));
                }
                // The deltas of this feed are already in the snapshot.
                continue;
            }
            for bytes in deltas {
                if rng.below(20) < loss {
                    lost_unnoticed = true;
                    continue;
                }
                if let Applied::NeedsResync(_) = decoder.apply(&mut remote, &bytes) {
                    assert!(!asked, "round {round}: one request per gap");
                    asked = true;
                    lost_unnoticed = false;
                }
                // A new run's reset in the live stream resyncs too.
                if !decoder.needs_resync() {
                    asked = false;
                }
                check_bounds(&remote);
            }
        }
        if decoder.needs_resync() || lost_unnoticed {
            for bytes in snapshot(&encoder) {
                decoder.apply(&mut remote, &bytes);
            }
        }
        assert!(!decoder.needs_resync());
        check_bounds(&remote);
        // Converged: the same run, activity and current screen; the newest steps exactly;
        // every screen held is the phone's (older ones may be "not available").
        let want = normalized(phone.timeline());
        assert_eq!(remote.task_id, want.task_id, "round {round}");
        assert_eq!(remote.activity, want.activity, "round {round}");
        assert_eq!(remote.current_screen_id, want.current_screen_id);
        let kept = remote.steps.len();
        assert!(kept >= want.steps.len().min(SNAPSHOT_STEPS) || kept == want.steps.len());
        let tail = &want.steps[want.steps.len() - kept..];
        assert_eq!(
            serde_json::to_string(&remote.steps).unwrap(),
            serde_json::to_string(tail).unwrap(),
            "round {round}"
        );
        for s in &remote.screens {
            let theirs = want.screens.iter().find(|w| w.id == s.id).unwrap();
            assert_eq!(
                serde_json::to_string(s).unwrap(),
                serde_json::to_string(theirs).unwrap()
            );
        }
    }
    assert!(resyncs > 50);
}

#[test]
fn arbitrary_bytes_and_orders_never_panic_the_decoder() {
    let mut rng = Rng(0x0DE1_7A00_5EED_0003);
    // Valid deltas from real runs, to mutate and reorder.
    let mut corpus: Vec<Vec<u8>> = Vec::new();
    for _ in 0..30 {
        let mut phone = LiveView::new(config(200, true));
        let mut encoder = DeltaEncoder::new();
        for u in random_updates(&mut rng, &[]) {
            phone.apply(u);
            corpus.extend(feed(&mut encoder, &phone));
        }
        corpus.extend(snapshot(&encoder));
    }
    let kinds = [
        "reset",
        "snapshot_steps",
        "step_added",
        "step_changed",
        "step_replaced",
        "steps_dropped",
        "current_screen",
        "screen",
        "activity",
        "zzz",
    ];
    for round in 0..400 {
        let mut t = ViewerTimeline::default();
        let mut d = DeltaDecoder::new();
        for _ in 0..rng.below(200) {
            let bytes: Vec<u8> = match rng.below(6) {
                // Valid deltas in any order, duplicated.
                0 | 1 => rng.pick(&corpus).clone(),
                // Bit flips and cuts.
                2 => {
                    let mut b = rng.pick(&corpus).clone();
                    for _ in 0..=rng.below(4) {
                        let i = usize::try_from(rng.below(b.len() as u64)).unwrap();
                        b[i] ^= 1 << rng.below(8);
                    }
                    b.truncate(usize::try_from(rng.below(b.len() as u64 + 1)).unwrap());
                    b
                }
                // Raw noise.
                3 => (0..rng.below(300)).map(|_| rng.next() as u8).collect(),
                // Plausible JSON with hostile numbers.
                4 => {
                    let n = |rng: &mut Rng| match rng.below(4) {
                        0 => u64::MAX,
                        1 => u64::from(u32::MAX),
                        _ => rng.below(30),
                    };
                    let (g, s, x) = (n(&mut rng), n(&mut rng), n(&mut rng));
                    format!(
                        r#"{{"gen":{g},"seq":{s},"kind":"{}","task_id":"t","dropped_steps":{x},"number":{x},"screen_id":{x},"step":{{"number":{x},"gesture":{{"kind":"wait","ms":1}},"caption":"c","state":{{"kind":"done"}}}},"steps":[],"activity":{{"kind":"acting"}}}}"#,
                        rng.pick(&kinds)
                    )
                    .into_bytes()
                }
                // Oversized.
                _ => vec![b' '; MAX_DELTA_BYTES + 1 + usize::try_from(rng.below(10)).unwrap()],
            };
            let _ = d.apply(&mut t, &bytes);
            check_bounds(&t);
        }
        // A snapshot always recovers.
        let mut phone = LiveView::new(config(200, false));
        let mut encoder = DeltaEncoder::new();
        for u in random_run(&mut rng, &[]).updates {
            phone.apply(u);
            feed(&mut encoder, &phone);
        }
        for bytes in snapshot(&encoder) {
            let a = d.apply(&mut t, &bytes);
            assert!(
                !matches!(a, Applied::NeedsResync(_)),
                "round {round}: {a:?}"
            );
        }
        assert!(!d.needs_resync());
    }
}

#[test]
fn oversized_or_undecodable_input_asks_for_a_resync_once() {
    let mut t = ViewerTimeline::default();
    let mut d = DeltaDecoder::new();
    let reset = br#"{"v":1,"gen":4,"seq":9,"kind":"reset","task_id":"t"}"#;
    assert!(matches!(d.apply(&mut t, reset), Applied::Changed(_)));
    let big = vec![b'{'; MAX_DELTA_BYTES + 1];
    assert_eq!(
        d.apply(&mut t, &big),
        Applied::NeedsResync(ResyncReason::Undecodable)
    );
    assert_eq!(d.apply(&mut t, b"garbage"), Applied::Unchanged);
    // A generation change without a reset.
    let mut d = DeltaDecoder::new();
    d.apply(&mut t, reset);
    let other = br#"{"gen":5,"seq":10,"kind":"activity","activity":{"kind":"looking"}}"#;
    assert_eq!(
        d.apply(&mut t, other),
        Applied::NeedsResync(ResyncReason::Generation)
    );
    // A step whose number does not follow.
    let mut d = DeltaDecoder::new();
    d.apply(&mut t, reset);
    let step = br#"{"gen":4,"seq":10,"kind":"step_added","step":{"number":7,"gesture":{"kind":"wait","ms":1},"caption":"c","state":{"kind":"done"}}}"#;
    assert_eq!(
        d.apply(&mut t, step),
        Applied::NeedsResync(ResyncReason::Invalid)
    );
}

/// A screen of `n` buttons with long labels.
fn big_tree(n: usize, salt: usize) -> p::UiNode {
    let children = (0..n)
        .map(|i| {
            node(
                p::Role::Button,
                Some(&format!("Button number {i} of screen {salt} long label")),
                rect(
                    (i % 10) as f64 * 39.0,
                    60.0 + (i / 10) as f64 * 19.0,
                    38.0,
                    18.0,
                ),
            )
        })
        .collect();
    node(
        p::Role::Application,
        Some("App"),
        rect(0.0, 0.0, 393.0, 852.0),
    )
    .children(children)
    .build()
}

#[test]
fn a_full_screen_fits_one_delta_with_its_most_relevant_elements() {
    let mut run = Run::new("big");
    let mut root = big_tree(398, 0);
    // A text field and an alert among many buttons: kept whatever is cut.
    root.children.push(
        node(
            p::Role::TextField,
            Some("Search"),
            rect(0.0, 10.0, 393.0, 40.0),
        )
        .build(),
    );
    root.children.push(
        node(
            p::Role::Alert,
            Some("Alert"),
            rect(20.0, 300.0, 350.0, 200.0),
        )
        .build(),
    );
    run.observe(observed(root));
    let t = run.timeline();
    let screen = &t.screens[0];
    assert_eq!(screen.elements.len(), MAX_ELEMENTS);
    let wire = wire_screen(screen);
    assert!(wire.elements_truncated);
    assert!(wire.elements.len() < MAX_ELEMENTS && wire.elements.len() > 250);
    assert!(wire
        .elements
        .iter()
        .any(|e| e.label.as_deref() == Some("Search")));
    assert!(wire
        .elements
        .iter()
        .any(|e| e.label.as_deref() == Some("Alert")));
    // Kept in drawing order.
    let positions: Vec<usize> = wire
        .elements
        .iter()
        .map(|e| screen.elements.iter().position(|o| o == e).unwrap())
        .collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]));
    let d = ViewerDelta {
        generation: u64::MAX,
        seq: u64::MAX,
        body: DeltaBody::Screen(wire),
    };
    assert!(d.encode().len() <= MAX_DELTA_BYTES);
}

/// A 200-step run, every step on a new full-size (400-element) screen.
fn heavy_run() -> Run {
    let mut run = Run::new("heavy");
    for i in 0..200 {
        run.observe(observed(big_tree(400, i)));
        run.act(tap(100.0, 100.0 + (i % 40) as f64));
    }
    run.add(Update::Finished(p::OutcomeStatus::Success));
    run
}

#[test]
fn stream_size_of_a_200_step_run() {
    let run = heavy_run();
    let mut phone = LiveView::new(config(200, true));
    let mut encoder = DeltaEncoder::new();
    let (mut stream, mut deltas, mut largest) = (0usize, 0usize, 0usize);
    for u in &run.updates {
        phone.apply(u.clone());
        for bytes in feed(&mut encoder, &phone) {
            stream += bytes.len();
            deltas += 1;
            largest = largest.max(bytes.len());
        }
    }
    let full = phone.timeline().encode().len();
    let snap: usize = snapshot(&encoder).iter().map(Vec::len).sum();
    println!(
        "200-step run, 400-element screens: full timeline encode {full} bytes; \
         delta stream {stream} bytes in {deltas} deltas (largest {largest}); \
         snapshot {snap} bytes"
    );
    assert!(full > 10_000_000);
    assert!(largest <= MAX_DELTA_BYTES);
    assert!(snap <= MAX_SNAPSHOT_BYTES);
    assert!(stream < full);
    // A typical run: small screens.
    let mut rng = Rng(7);
    let mut phone = LiveView::new(config(200, false));
    let mut encoder = DeltaEncoder::new();
    let mut run = Run::new("typical");
    for i in 0..200 {
        if i % 2 == 0 {
            run.observe(observed(login_tree(None)));
        }
        run.act(tap(rng.f(0.0, 393.0), rng.f(0.0, 852.0)));
    }
    let mut stream = 0usize;
    for u in &run.updates {
        phone.apply(u.clone());
        stream += feed(&mut encoder, &phone)
            .iter()
            .map(Vec::len)
            .sum::<usize>();
    }
    let snap: usize = snapshot(&encoder).iter().map(Vec::len).sum();
    println!(
        "200-step run, sign-in screens: full timeline encode {} bytes; delta stream {stream} \
         bytes; snapshot {snap} bytes",
        phone.timeline().encode().len()
    );
}

#[test]
fn a_snapshot_is_bounded_and_carries_the_newest_steps() {
    let run = heavy_run();
    let mut phone = LiveView::new(config(200, true));
    let mut encoder = DeltaEncoder::new();
    for u in &run.updates {
        phone.apply(u.clone());
        feed(&mut encoder, &phone);
    }
    let snap = encoder.snapshot();
    assert!(matches!(snap[0].body, DeltaBody::Reset { .. }));
    let mut t = ViewerTimeline::default();
    let mut d = DeltaDecoder::new();
    for delta in &snap {
        d.apply(&mut t, &delta.encode());
    }
    assert_eq!(t.steps.len(), SNAPSHOT_STEPS);
    assert_eq!(t.steps.last().unwrap().number, 200);
    assert_eq!(t.dropped_steps, 100);
    // Only the current screen: the steps' own screens are "not available".
    assert_eq!(t.screens.len(), 1);
    assert_eq!(Some(t.screens[0].id), t.current_screen_id);
    assert!(t.screens[0].frame_key.is_none());
    check_bounds(&t);
}

fn stream_text(updates: &[Update], rng: &mut Rng) -> String {
    let mut phone = LiveView::new(config(200, rng.coin()));
    let mut encoder = DeltaEncoder::new();
    let mut out = Vec::new();
    for u in updates {
        phone.apply(u.clone());
        for bytes in feed(&mut encoder, &phone) {
            out.extend(bytes);
        }
        if rng.below(5) == 0 {
            for bytes in snapshot(&encoder) {
                out.extend(bytes);
            }
        }
    }
    String::from_utf8(out).unwrap()
}

#[test]
fn secrets_never_reach_a_delta_or_a_snapshot() {
    let mut rng = Rng(0x0DE1_7A00_5EED_0004);
    for round in 0..300 {
        let run = random_run(&mut rng, &SECRETS);
        let text = stream_text(&run.updates, &mut rng);
        for secret in SECRETS {
            assert!(!text.contains(secret), "round {round}: {secret}");
        }
        assert!(!text.contains("1111 1111"), "round {round}");
    }
    // Secure field values, a value typed into a secure field, frame keys.
    let mut run = Run::new("login");
    run.observe(observed(login_tree(Some("password"))));
    run.act(tap(100.0, 282.0));
    run.act_typed(type_text(SECRET_VALUE), type_action(Some(2), SECRET_VALUE));
    run.act_typed(type_text("4321-PIN"), type_action(None, "4321-PIN"));
    let text = stream_text(&run.updates, &mut rng);
    assert!(!text.contains(SECRET_VALUE) && !text.contains("hunter2"));
    assert!(!text.contains("4321") && !text.contains("me@example.org"));
    assert!(!text.contains("frame_key"));
}

#[test]
fn a_locked_phone_sends_nothing_of_the_screen() {
    let mut run = Run::new("locked");
    let tree = node(
        p::Role::Application,
        Some("Bank"),
        rect(0.0, 0.0, 393.0, 852.0),
    )
    .children(vec![node(
        p::Role::StaticText,
        Some("Balance 12,345.67"),
        rect(10.0, 100.0, 300.0, 40.0),
    )])
    .build();
    run.observe(observed_full(tree, PHONE, true, Some("Bank"), "com.bank"));
    run.act(tap(10.0, 120.0));
    run.add(Update::AwaitingUnlock);
    let mut rng = Rng(3);
    let text = stream_text(&run.updates, &mut rng);
    assert!(!text.contains("Balance") && !text.contains("12,345") && !text.contains("Bank"));
    assert!(!text.contains("com.bank"));
    assert!(text.contains(r#""source":"locked""#));
    assert!(text.contains(r#""kind":"locked""#));
}

#[test]
fn a_new_run_starts_a_new_generation_with_a_reset() {
    let mut phone = LiveView::default();
    let mut encoder = DeltaEncoder::new();
    let mut run = Run::new("a");
    run.observe(observed(login_tree(None)));
    run.act(tap(100.0, 420.0));
    for u in &run.updates {
        phone.apply(u.clone());
        encoder.feed(phone.runs(), phone.timeline());
    }
    let first = encoder.generation();
    // The same task id again: still a new run.
    phone.apply(Update::Started {
        task_id: "a".into(),
    });
    let deltas = encoder.feed(phone.runs(), phone.timeline());
    assert_eq!(encoder.generation(), first + 1);
    assert!(matches!(deltas[0].body, DeltaBody::Reset { .. }));
    // Screen ids restart: screen 1 of the new run is sent again.
    phone.apply(Update::Effect {
        effect: p::Effect::Observe { screenshot: false },
        number: 1,
    });
    phone.apply(Update::EffectCompleted {
        effect: p::Effect::Observe { screenshot: false },
        event: observed(login_tree(None)),
    });
    let deltas = encoder.feed(phone.runs(), phone.timeline());
    assert!(deltas
        .iter()
        .any(|d| matches!(&d.body, DeltaBody::Screen(s) if s.id == 1)));
}

#[test]
fn the_approval_picture_never_enters_a_delta_or_a_snapshot() {
    let full = rect(0.0, 0.0, PHONE.width, PHONE.height);
    let tree = node(p::Role::Application, Some("Shop"), full)
        .children(vec![node(p::Role::Window, None, full).children(vec![
            node(
                p::Role::Button,
                Some("Pay $512.40"),
                rect(16.0, 700.0, 361.0, 50.0),
            ),
        ])])
        .build();
    let screen = p::Screen {
        app_bundle_id: "com.example.shop".to_owned(),
        app_name: Some("Shop".to_owned()),
        size: PHONE,
        root: tree.clone(),
        keyboard_visible: false,
        orientation: p::Orientation::Portrait,
    };
    let index = jarvis_observe::compact(&screen)
        .elements
        .iter()
        .find(|e| e.label == "Pay $512.40")
        .map(|e| e.index)
        .unwrap();
    let request = p::ConfirmRequest {
        category: p::RiskCategory::Payment,
        summary: "SUMMARY-ONLY-ON-THE-SHEET".to_owned(),
        app: "com.example.shop".to_owned(),
        phone_only: false,
        ..Default::default()
    };
    let mut run = Run::new("pay");
    run.observe(observed_full(
        tree,
        PHONE,
        false,
        Some("Shop"),
        "com.example.shop",
    ));
    run.number += 1;
    run.updates.push(Update::Effect {
        effect: p::Effect::Confirm(request.clone()),
        number: run.number,
    });
    run.updates.push(Update::Progress(progress(
        1,
        Some(p::Action::Tap { element: index }),
    )));
    run.updates.push(Update::ConfirmationPending(request));
    let mut phone = LiveView::new(config(200, true));
    let mut encoder = DeltaEncoder::new();
    let mut text = String::new();
    for u in &run.updates {
        phone.apply(u.clone());
        for bytes in feed(&mut encoder, &phone) {
            text.push_str(std::str::from_utf8(&bytes).unwrap());
        }
    }
    let approval = phone
        .reducer()
        .approval()
        .cloned()
        .expect("an approval picture");
    for bytes in snapshot(&encoder) {
        text.push_str(std::str::from_utf8(&bytes).unwrap());
    }
    assert!(!text.contains("SUMMARY-ONLY-ON-THE-SHEET"));
    assert!(!text.contains(&approval.caption), "{}", approval.caption);
    assert!(!text.contains(&approval.accessibility_label));
    assert!(!text.contains("frame_key") && !text.contains("\"unit\""));
    assert!(!text.contains("outline") && !text.contains("Outline"));
    // The stream is the timeline's, which holds no approval state at all.
    assert!(text.contains("waiting_for_approval"));
}
