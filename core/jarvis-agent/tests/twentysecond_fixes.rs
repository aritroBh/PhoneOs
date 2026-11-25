#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Twenty-second review, R22-A: a wait the model asks for (or a learned flow replays) is
//! sent at one of three fixed levels — 1 000, 3 000 or 10 000 ms, the smallest not below
//! what was asked — never with the asked duration; the core's own waits are unchanged.

mod common;

use common::*;
use jarvis_agent::{APPROVAL_SETTLE_MS, TYPING_LOOK_MS};
use jarvis_memory::{FlowAction, FlowStore, MemoryStore};
use jarvis_proto::fixtures::random::Rng;
use jarvis_proto::fixtures::{node, rect, settings_screen, text, IPHONE_SIZE};
use jarvis_proto::{AppInfo, OutcomeStatus, Role, RunnerCommand, Screen, UiNode};
use jarvis_safety::{wait_level, MAX_WAIT_MS, WAIT_LEVELS_MS};

const DONE: &str = r#"{"action":"done","summary":"ok"}"#;

fn wait(ms: u64) -> String {
    format!(r#"{{"action":"wait","ms":{ms}}}"#)
}

fn waits(r: &Run) -> Vec<u64> {
    r.acts
        .iter()
        .filter_map(|a| match a {
            RunnerCommand::Wait { ms } => Some(*ms),
            _ => None,
        })
        .collect()
}

/// One model wait of `asked` ms on a still screen: the wait commands sent.
fn sent_for(asked: u64) -> (Vec<u64>, Run) {
    let r = run(
        config(),
        "Wait for the page",
        MemoryStore::new(),
        Phone::still(settings_screen()),
        Script::model(&[&wait(asked), DONE]),
    );
    (waits(&r), r)
}

#[test]
fn each_boundary_is_sent_at_its_level() {
    for (asked, sent) in [
        (0, 1_000),
        (1, 1_000),
        (999, 1_000),
        (1_000, 1_000),
        (1_001, 3_000),
        (3_000, 3_000),
        (3_001, 10_000),
        (9_999, 10_000),
        (10_000, 10_000),
    ] {
        assert_eq!(wait_level(asked), sent, "{asked}");
        let (w, r) = sent_for(asked);
        assert_eq!(w, vec![sent], "asked {asked}");
        assert_eq!(r.outcome.status, OutcomeStatus::Success, "{asked}");
        // What the model reads back is the duration sent, not the one it asked for.
        let after = &r.requests[1].user;
        assert!(after.contains(&format!("wait {sent}ms")), "{after}");
        if asked != sent {
            assert!(!after.contains(&format!("wait {asked}ms")), "{after}");
        }
    }
}

/// Above the longest level nothing changes: refused as before (`MAX_WAIT_MS`), no wait is
/// sent.
#[test]
fn a_wait_above_the_longest_level_is_still_refused() {
    assert_eq!(wait_level(MAX_WAIT_MS + 1), MAX_WAIT_MS + 1);
    let (w, r) = sent_for(MAX_WAIT_MS + 1);
    assert!(w.is_empty(), "{w:?}");
    assert!(
        r.all_prompt_text().contains("10000"),
        "{}",
        r.all_prompt_text()
    );
}

/// For any asked duration up to the maximum, the duration sent is a level, not below what
/// was asked, and the smallest such level.
#[test]
fn every_asked_duration_is_sent_as_the_smallest_level_not_below_it() {
    let mut r = Rng::new(22);
    let mut asked: Vec<u64> = (0..=MAX_WAIT_MS + 2).step_by(7).collect();
    asked.extend((0..20_000).map(|_| r.below(MAX_WAIT_MS + 1)));
    asked.extend([u64::MAX, MAX_WAIT_MS + 1, 999, 1_001, 2_999, 3_001]);
    for ms in asked {
        let sent = wait_level(ms);
        if ms <= MAX_WAIT_MS {
            assert!(WAIT_LEVELS_MS.contains(&sent), "{ms} -> {sent}");
            assert!(sent >= ms, "{ms} -> {sent}");
            assert!(
                WAIT_LEVELS_MS.iter().all(|l| *l < ms || *l >= sent),
                "{ms} -> {sent}: a smaller level fits"
            );
        } else {
            assert_eq!(sent, ms, "above the maximum: unchanged, and refused");
        }
    }
    // Every sent value an end-to-end run shows is a level (a few random ones).
    for _ in 0..12 {
        let ms = r.below(MAX_WAIT_MS + 1);
        let (w, _) = sent_for(ms);
        assert_eq!(w.len(), 1);
        assert!(WAIT_LEVELS_MS.contains(&w[0]), "{ms} -> {w:?}");
    }
}

/// The core's own waits are not leveled: the typing settle look is still 400 ms, the
/// settle before the look after an approval 500 ms (pinned end to end by
/// `typing_stable.rs`, `approval_look.rs`, `review_fixes.rs` and `session_lock.rs`).
#[test]
fn core_chosen_waits_are_unchanged() {
    assert_eq!(TYPING_LOOK_MS, 400);
    assert_eq!(APPROVAL_SETTLE_MS, 500);
    assert!(!WAIT_LEVELS_MS.contains(&TYPING_LOOK_MS));
    assert!(!WAIT_LEVELS_MS.contains(&APPROVAL_SETTLE_MS));
    // End to end: a typing whose field shows a lagging value first gets the 400 ms look;
    // the model's own wait after it is leveled.
    let idx = index_of(&search_screen(true, None), "Search");
    let phone = Phone::new(search_screen(true, None), move |cmd, _s| match cmd {
        RunnerCommand::TypeText { .. } => Transition::Go(search_screen(true, Some("A"))),
        RunnerCommand::Wait { ms } if *ms == TYPING_LOOK_MS => {
            Transition::Go(search_screen(true, Some("Ada")))
        }
        _ => Transition::Stay,
    });
    let r = run(
        config(),
        "Search for Ada",
        MemoryStore::new(),
        phone,
        Script::model(&[
            &format!(r#"{{"action":"type","element":{idx},"text":"Ada"}}"#),
            &wait(1_200),
            DONE,
        ]),
    );
    assert_eq!(waits(&r), vec![TYPING_LOOK_MS, 3_000], "{:?}", r.acts);
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.outcome.summary
    );
}

// ---- the review's scenario (probe a2) ------------------------------------------------------

const FLOW: &str = "what it read there could be passed on here";
const SCROLL: &str = r#"{"action":"scroll","direction":"down"}"#;
const SCROLL_UP: &str = r#"{"action":"scroll","direction":"up"}"#;
const ACME: &str = "com.example.acme";
const SHOP: &str = "com.example.shop";

fn scr(app: &str, name: &str, children: Vec<UiNode>) -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(name)
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(children)
        ]);
    jarvis_proto::fixtures::screen(app, name, root, false)
}

fn acme() -> Screen {
    scr(
        ACME,
        "Acme",
        vec![
            text("Your code is 4821", rect(16.0, 140.0, 361.0, 40.0)),
            node(Role::Button, "Open Shop", rect(16.0, 300.0, 361.0, 44.0)),
        ],
    )
}

fn shop(n: usize) -> Screen {
    let mut rows = vec![text("Feed", rect(16.0, 100.0, 361.0, 30.0))];
    for k in 0..6 {
        rows.push(text(
            &format!("Post {}", n + k),
            rect(16.0, 150.0 + 90.0 * k as f64, 361.0, 80.0),
        ));
    }
    scr(SHOP, "Shop", rows)
}

/// After one approved scroll into Shop (a kept scroll-only grant), the model's waits
/// between the unasked scrolls are sent at their levels: 4821 → 10 000, 1337 → 3 000.
/// What the page can still learn per unasked scroll: direction, count, one of three levels
/// and the model's own latency (Known gaps).
#[test]
fn a2_waits_between_unasked_scrolls_are_leveled() {
    let mut c = config();
    c.known_apps
        .push(AppInfo::new("com.apple.mobilesafari", "Safari"));
    let a = acme();
    let open = index_of(&a, "Open Shop");
    let mut n = 0usize;
    let phone = Phone::new(a, move |cmd, s| {
        n += 1;
        if taps(cmd, s, "Open Shop") || s.app_bundle_id == SHOP {
            return Transition::Go(shop(n));
        }
        Transition::Stay
    });
    let r = run(
        c,
        "Open Shop and read the feed",
        MemoryStore::new(),
        phone,
        Script::model(&[
            &format!(r#"{{"action":"tap","element":{open}}}"#),
            SCROLL,
            &wait(4821),
            SCROLL_UP,
            &wait(1337),
            SCROLL,
            DONE,
        ])
        .decisions(&[true; 8]),
    );
    let swipes = r
        .acts
        .iter()
        .filter(|a| matches!(a, RunnerCommand::Swipe { .. }))
        .count();
    let flows = r
        .confirms
        .iter()
        .filter(|c| c.summary.contains(FLOW))
        .count();
    assert_eq!(swipes, 3, "{:?}", r.acts);
    assert_eq!(flows, 1, "one scroll approval, kept for the later scrolls");
    assert_eq!(waits(&r), vec![10_000, 3_000], "{:?}", r.acts);
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.outcome.summary
    );
}

// ---- learned flows ------------------------------------------------------------------------

fn wifi_phone() -> Phone {
    Phone::new(settings_screen(), |cmd, s: &Screen| {
        if taps(cmd, s, "Wi-Fi, HomeNet") {
            return Transition::Go(wifi_screen(true));
        }
        if s.app_bundle_id == "com.apple.Preferences" && taps(cmd, s, "Wi-Fi") {
            let on = jarvis_observe::compact(s)
                .elements
                .iter()
                .any(|e| e.label == "Wi-Fi" && e.value.as_deref() == Some("1"));
            return Transition::Go(wifi_screen(!on));
        }
        Transition::Stay
    })
}

fn switch_index() -> u32 {
    jarvis_observe::compact(&wifi_screen(true))
        .elements
        .iter()
        .find(|e| e.role == Role::Switch)
        .unwrap()
        .index
}

fn recorded_waits(store: &FlowStore) -> Vec<u64> {
    store
        .flows()
        .iter()
        .flat_map(|f| f.steps.iter())
        .filter_map(|s| match s.action {
            FlowAction::Wait { ms } => Some(ms),
            _ => None,
        })
        .collect()
}

/// A learned flow records a model's wait at the level sent (1 500 → 3 000), and a flow
/// stored before this rule with a raw duration replays at its level (1 337 → 3 000).
#[test]
fn learned_flows_store_and_replay_waits_at_their_level() {
    let script = [
        r#"{"action":"tap","element":4}"#.to_owned(),
        wait(1_500),
        format!(r#"{{"action":"tap","element":{}}}"#, switch_index()),
        DONE.to_owned(),
    ];
    let s: Vec<&str> = script.iter().map(String::as_str).collect();
    let first = run(
        config(),
        "Turn off Wi-Fi",
        MemoryStore::new(),
        wifi_phone(),
        Script::model(&s).decisions(&[true; 4]),
    );
    assert_eq!(first.outcome.status, OutcomeStatus::Success);
    assert_eq!(waits(&first), vec![3_000]);
    assert_eq!(first.memory.flows.len(), 1);
    assert_eq!(recorded_waits(&first.memory.flows), vec![3_000]);

    // Replayed as recorded.
    let second = run(
        config(),
        "turn off Wi-Fi.",
        first.memory.clone(),
        wifi_phone(),
        Script::model(&[]).decisions(&[true; 4]),
    );
    assert_eq!(second.infers(), 0, "{}", second.outcome.summary);
    assert_eq!(waits(&second), vec![3_000]);

    // A flow stored with the model's raw duration (before R22-A) replays at its level.
    let mut memory = first.memory;
    let json = serde_json::to_string(&memory.flows).unwrap();
    assert!(json.contains(r#""ms":3000"#), "{json}");
    memory.flows = serde_json::from_str(&json.replace(r#""ms":3000"#, r#""ms":1337"#)).unwrap();
    assert_eq!(recorded_waits(&memory.flows), vec![1_337]);
    let third = run(
        config(),
        "turn off Wi-Fi.",
        memory,
        wifi_phone(),
        Script::model(&[]).decisions(&[true; 4]),
    );
    assert_eq!(third.infers(), 0, "{}", third.outcome.summary);
    assert_eq!(waits(&third), vec![3_000], "{:?}", third.acts);
}
