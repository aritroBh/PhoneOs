#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 31: the reviewer's break-it probes (`zz_breakit31.rs`) as regression tests,
//! printing probes turned into assertions, plus the fixes' own tests.
//!
//! - M1: on an incomplete screen every gesture that touches it (a vertical scroll too) asks,
//!   phone only, and is refused while any category is denied; `press_home` is the way off
//!   (gated as on any screen); `open_app`/`open_url` keep the incomplete rules; a learned
//!   flow is not replayed there.
//! - M2: more than 32 controls under a point or on a path: every one is classified (a denied
//!   one refuses), the question names the riskiest and says how many, phone only.
//! - L1: a tree deeper than the bound (or with more nodes than the wire carries) is cut at
//!   every intake and flagged; the session reads an incomplete screen and can leave it.
//! - L2: sideways drags across grid, carousel, calendar, menu and pager cells keep the
//!   screen cap (asked); list rows keep their bound and refusal.

mod common;

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_observe::{compact_view, CompactView};
use jarvis_proto::fixtures::{node, rect, screen, IPHONE_SIZE};
use jarvis_proto::{
    Action, Direction, Key, OutcomeStatus, Policy, Rect, RiskCategory, Role, RunnerCommand, Screen,
    SessionConfig, UiNode, MAX_TREE_DEPTH,
};
use jarvis_safety::gestures::{GestureError, GestureKind, MAX_SIDEWAYS_SHARE};
use jarvis_safety::{
    evaluate_owned, planned_drag, Decision, ScreenSource, ScreenTree, INCOMPLETE_SCREEN_WARNING,
    ROW_SWIPE_NOTE,
};

// ---------------------------------------------------------------------------------------
// Helpers (from the reviewer's probes).
// ---------------------------------------------------------------------------------------

fn nest(depth: usize, leaf: UiNode) -> UiNode {
    let mut n = leaf;
    for _ in 0..depth {
        n = UiNode::new(Role::Other("Other".into()), IPHONE_SIZE.to_rect()).with_children(vec![n]);
    }
    n
}

fn shop(kids: Vec<UiNode>) -> Screen {
    screen(
        "com.example.shop",
        "Shop",
        UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_children(vec![UiNode::new(
            Role::Window,
            IPHONE_SIZE.to_rect(),
        )
        .with_children(kids)]),
        false,
    )
}

fn deep_with(depth: usize, leaf: UiNode) -> Screen {
    shop(vec![
        node(Role::StaticText, "Account", rect(16.0, 100.0, 200.0, 30.0)),
        node(Role::Button, "Help", rect(16.0, 150.0, 100.0, 44.0)),
        nest(depth, leaf),
    ])
}

fn picker() -> UiNode {
    UiNode::new(Role::Picker, rect(0.0, 250.0, IPHONE_SIZE.width, 400.0))
        .with_label("Transfer amount")
        .with_value("100")
        .hittable(true)
}

fn buy_now() -> UiNode {
    UiNode::new(Role::Button, rect(16.0, 400.0, 361.0, 44.0))
        .with_label("Buy now")
        .hittable(true)
}

/// Runs on a big stack (the trees here are deeper than the bound on purpose).
fn big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

fn drive_with(s: Screen, c: SessionConfig, steps: &[&str], decisions: bool) -> Run {
    let steps: Vec<String> = steps.iter().map(|s| (*s).to_owned()).collect();
    big_stack(move || {
        let refs: Vec<&str> = steps.iter().map(String::as_str).collect();
        run(
            c,
            "Look at my account page",
            MemoryStore::new(),
            Phone::still(s),
            Script::model(&refs).decisions(&[decisions; 8]),
        )
    })
}

fn drive(s: Screen, c: SessionConfig, steps: &[&str]) -> Run {
    drive_with(s, c, steps, false)
}

fn gestures(r: &Run) -> usize {
    r.acts
        .iter()
        .filter(|a| !matches!(a, RunnerCommand::Wait { .. }))
        .count()
}

fn sent_home(r: &Run) -> bool {
    r.acts
        .iter()
        .any(|a| matches!(a, RunnerCommand::PressKey { key: Key::Home, .. }))
}

fn policies() -> Vec<(&'static str, SessionConfig)> {
    let d = config();
    let mut relaxed = config();
    relaxed.policy.confirm.clear();
    relaxed.policy.first_use_approval = false;
    let mut deny = config();
    deny.policy.deny_categories = vec![RiskCategory::Payment];
    vec![("default", d), ("relaxed", relaxed), ("deny-payment", deny)]
}

const FAIL: &str = r#"{"action":"fail","reason":"stop"}"#;
const DONE: &str = r#"{"action":"done","summary":"ok"}"#;
const REFUSED: &str = "the screen could not be read completely";

fn compact_text(c: &jarvis_proto::CompactScreen) -> String {
    format!("{c:?}")
}

fn tree(v: &CompactView) -> ScreenTree<'_> {
    ScreenTree {
        sources: &v.sources,
        parents: &v.parents,
        ends: &v.ends,
        bars: &v.bars,
        nodes: &v.nodes,
        inputs: &v.inputs,
    }
}

fn levels(n: &UiNode) -> usize {
    let mut best = 0;
    let mut stack = vec![(n, 1usize)];
    while let Some((e, d)) = stack.pop() {
        best = best.max(d);
        stack.extend(e.children.iter().map(|c| (c, d + 1)));
    }
    best
}

// ---------------------------------------------------------------------------------------
// M1: every gesture on an incomplete screen; Home is the way out.
// ---------------------------------------------------------------------------------------

/// Reviewer A1: an element-less vertical scroll whose drag crosses an unread (deep) picker
/// was sent unasked under every policy at depth 170 (the same tree at depth 5 asks a
/// Payment question by default). Now it asks (phone only, with the warning) whatever the
/// policy relaxes, and is refused while a category is denied.
#[test]
fn m1_a_vertical_scroll_over_an_unread_picker_asks_or_is_refused() {
    for (name, c) in policies() {
        let r = drive(
            deep_with(170, picker()),
            c,
            &[r#"{"action":"scroll","direction":"down"}"#, FAIL],
        );
        assert_eq!(gestures(&r), 0, "{name}: {:?}", r.acts);
        if name == "deny-payment" {
            assert!(r.confirms.is_empty(), "{name}: {:#?}", r.confirms);
            assert!(r.all_prompt_text().contains(REFUSED), "{name}");
        } else {
            let c = r.confirms.first().expect(name);
            assert!(c.phone_only, "{name}");
            assert!(
                c.summary.starts_with(INCOMPLETE_SCREEN_WARNING),
                "{name}: {}",
                c.summary
            );
            assert!(c.category >= RiskCategory::Unverified || c.category == RiskCategory::Payment);
        }
    }
}

/// Reviewer A1b, as assertions: every action kind that touches the screen asks phone only
/// (default, relaxed) or is refused (a denied category); `open_app` (a known app) and
/// `open_url` keep the incomplete rules (asked, refused under a denial); Home is sent under
/// every policy with no incomplete-screen question.
#[test]
fn m1_every_action_kind_on_an_incomplete_screen() {
    let touching = [
        r#"{"action":"tap","element":2}"#,
        r#"{"action":"long_press","element":2,"duration_ms":800}"#,
        r#"{"action":"swipe","direction":"left"}"#,
        r#"{"action":"scroll","direction":"right"}"#,
        r#"{"action":"scroll","direction":"up"}"#,
        r#"{"action":"scroll","direction":"down"}"#,
        r#"{"action":"tap_point","x":0.5,"y":0.47}"#,
        r#"{"action":"open_app","app":"Settings"}"#,
        r#"{"action":"open_url","url":"https://example.com"}"#,
    ];
    for (name, c) in policies() {
        for k in touching {
            let mut c = c.clone();
            c.model.supports_vision = true;
            let r = drive(deep_with(170, buy_now()), c, &[k, FAIL]);
            assert_eq!(gestures(&r), 0, "{name} {k}: {:?}", r.acts);
            if name == "deny-payment" {
                assert!(r.confirms.is_empty(), "{name} {k}: {:#?}", r.confirms);
                assert!(r.all_prompt_text().contains(REFUSED), "{name} {k}");
            } else {
                assert!(
                    r.confirms
                        .iter()
                        .any(|q| q.phone_only && q.summary.starts_with(INCOMPLETE_SCREEN_WARNING)),
                    "{name} {k}: {:#?}",
                    r.confirms
                );
            }
        }
        // Home: sent, no incomplete-screen question (nothing else was read to ask about).
        let r = drive(
            deep_with(170, buy_now()),
            c.clone(),
            &[r#"{"action":"press_home"}"#, FAIL],
        );
        assert!(sent_home(&r), "{name}: {:?} {:#?}", r.acts, r.confirms);
        assert!(r.confirms.is_empty(), "{name}: {:#?}", r.confirms);
    }
}

/// Home on an incomplete screen goes through Rule T as on any screen (the flow accounting
/// is unchanged): with a screenshot read, Home asks the flow question (phone only) and is
/// not refused while a category is denied; approved, it is sent.
#[test]
fn m1_home_keeps_its_flow_question_and_is_never_refused() {
    for (name, mut c) in policies() {
        c.model.supports_vision = true;
        let r = drive_with(
            deep_with(170, buy_now()),
            c,
            &[r#"{"action":"press_home"}"#, FAIL],
            true,
        );
        assert!(sent_home(&r), "{name}: {:?}", r.acts);
        assert!(!r.all_prompt_text().contains(REFUSED), "{name}");
        for q in &r.confirms {
            assert!(q.flow_only && q.phone_only, "{name}: {q:#?}");
        }
    }
}

/// Reviewer A1b's `open_app "Safari"` sent nothing and asked nothing: not a bug of the
/// incomplete rules — the test config knows no app named Safari, so the step is "not done"
/// before any gate (the same on a complete screen).
#[test]
fn m1_open_app_of_an_unknown_name_is_not_done_on_any_screen() {
    for s in [deep_with(170, buy_now()), deep_with(3, buy_now())] {
        let r = drive(
            s,
            config(),
            &[r#"{"action":"open_app","app":"Safari"}"#, FAIL],
        );
        assert!(r.acts.is_empty() && r.confirms.is_empty());
        assert!(
            r.all_prompt_text().contains("open_app"),
            "{}",
            r.last_user()
        );
    }
}

/// The session with a deep tree, end to end: after Home it leaves (Home is the way out of a
/// screen it cannot read), under a denial too.
#[test]
fn m1_the_session_can_leave_an_incomplete_screen() {
    let mut c = config();
    c.policy.deny_categories = vec![RiskCategory::Delete];
    let deep = deep_with(170, buy_now());
    let r = big_stack(move || {
        let phone = Phone::new(deep, |cmd, _s: &Screen| match cmd {
            RunnerCommand::PressKey { key: Key::Home, .. } => Transition::Go(home_screen()),
            _ => Transition::Stay,
        });
        run(
            c,
            "Go home",
            MemoryStore::new(),
            phone,
            Script::model(&[r#"{"action":"press_home"}"#, DONE]),
        )
    });
    assert!(sent_home(&r), "{:?}", r.acts);
    assert_eq!(r.outcome.status, OutcomeStatus::Success, "{:?}", r.outcome);
}

/// A learned flow stops on an incomplete screen: its element-less vertical scroll was
/// replayed unasked there (the flow matched the listed part of the screen); now the replay
/// diverges and the model decides through the incomplete-screen rules.
#[test]
fn m1_a_replay_stops_on_an_incomplete_screen() {
    let shallow = shop(vec![
        node(Role::StaticText, "Account", rect(16.0, 100.0, 200.0, 30.0)),
        node(Role::Button, "Help", rect(16.0, 150.0, 100.0, 44.0)),
    ]);
    let scroll = r#"{"action":"scroll","direction":"down"}"#;
    let s = shallow.clone();
    let learned = run(
        config(),
        "scroll my account page",
        MemoryStore::new(),
        Phone::new(s, |cmd, s: &Screen| match cmd {
            RunnerCommand::Swipe { .. } => {
                let mut n = s.clone();
                n.root.children[0].children.push(node(
                    Role::StaticText,
                    "Orders",
                    rect(16.0, 300.0, 200.0, 30.0),
                ));
                Transition::Go(n)
            }
            _ => Transition::Stay,
        }),
        Script::model(&[scroll, DONE]),
    );
    assert_eq!(learned.memory.flows.len(), 1, "a flow was learned");
    // The same screen, with an unread deep picker under the drag.
    let mut deep = shallow.clone();
    deep.root.children[0].children.push(nest(170, picker()));
    // Control: the flow replays on the complete screen.
    let again = run(
        config(),
        "scroll my account page",
        learned.memory.clone(),
        Phone::still(shallow),
        Script::model(&[FAIL]),
    );
    assert!(
        again
            .acts
            .iter()
            .any(|a| matches!(a, RunnerCommand::Swipe { .. })),
        "control: the flow replays: {:?}",
        again.acts
    );
    let memory = learned.memory.clone();
    let r = big_stack(move || {
        run(
            config(),
            "scroll my account page",
            memory,
            Phone::still(deep),
            Script::model(&[FAIL]),
        )
    });
    assert!(
        !r.acts
            .iter()
            .any(|a| matches!(a, RunnerCommand::Swipe { .. })),
        "the replay sent its scroll on an incomplete screen: {:?}",
        r.acts
    );
    assert!(r.infers() >= 1, "the model decides");
    assert!(
        r.all_prompt_text().contains("could not be read completely"),
        "the divergence is told"
    );
}

/// A skill session's steps on an incomplete screen go through the same rules (a skill only
/// narrows): the tap asks, phone only, with the warning.
#[test]
fn m1_a_skill_session_on_an_incomplete_screen_asks() {
    let mut c = config();
    c.skill = Some(jarvis_proto::SkillContext {
        id: "shop".into(),
        name: "Shop".into(),
        instructions: "Look at the account page.".into(),
        apps: vec!["com.example.shop".into()],
        confine_to_apps: true,
        args: std::collections::BTreeMap::new(),
        tools: Vec::new(),
    });
    let r = drive(
        deep_with(170, buy_now()),
        c,
        &[r#"{"action":"scroll","direction":"down"}"#, FAIL],
    );
    assert_eq!(gestures(&r), 0, "{:?}", r.acts);
    let q = r.confirms.first().expect("asked");
    assert!(q.phone_only && q.summary.starts_with(INCOMPLETE_SCREEN_WARNING));
}

// ---------------------------------------------------------------------------------------
// M2: more controls than the cap.
// ---------------------------------------------------------------------------------------

fn decoy_screen(decoys: usize) -> Screen {
    let mut kids = vec![node(
        Role::Button,
        "Delete account",
        rect(16.0, 380.0, 361.0, 84.0),
    )];
    for k in 0..decoys {
        let d = 2.0 + k as f64 * 0.5 / (decoys.max(40) as f64 / 40.0);
        kids.push(node(
            Role::Button,
            &format!("Info {k}"),
            rect(196.5 - d, 422.0 - d, 2.0 * d, 2.0 * d),
        ));
    }
    shop(kids)
}

fn decide(p: &Policy, a: &Action, s: &Screen) -> Decision {
    let v = compact_view(s);
    evaluate_owned(
        p,
        a,
        &v.screen,
        None,
        ScreenSource::Typed,
        Some(&tree(&v)),
        &[],
        &[],
    )
}

/// Reviewer A1e, as assertions: with 33 or more decoy buttons over a "Delete account"
/// button, a denied Delete became `Confirm(Unverified)` naming "Info 0". Now every hit is
/// classified: denied → refused at every count; default → asked naming Delete, phone only
/// and saying how many controls are there once over the cap.
#[test]
fn m2_scanned_controls_cap_under_tap_point() {
    let point = Action::TapPoint {
        x: 196.5 / IPHONE_SIZE.width,
        y: 422.0 / IPHONE_SIZE.height,
    };
    for decoys in [0usize, 31, 33, 40, 400] {
        let s = decoy_screen(decoys);
        let mut deny = config().policy;
        deny.deny_categories = vec![RiskCategory::Delete];
        let d = decide(&deny, &point, &s);
        assert!(matches!(d, Decision::Deny(_)), "{decoys}: {d:?}");
        let d = decide(&config().policy, &point, &s);
        let Decision::Confirm(q) = d else {
            panic!("{decoys}: {d:?}")
        };
        assert_eq!(q.category, RiskCategory::Delete, "{decoys}: {q:?}");
        assert!(
            q.summary.contains("Delete account"),
            "{decoys}: {}",
            q.summary
        );
        assert!(!q.summary.contains("\"Info "), "{decoys}: {}", q.summary);
        if decoys > 32 {
            assert!(q.phone_only, "{decoys}");
            assert!(
                q.summary
                    .contains(&format!("{} controls are under this point", decoys + 1)),
                "{decoys}: {}",
                q.summary
            );
        }
    }
}

/// The path variant: 32 pickers crossed before a "Slide to pay" slider (the 33rd crossed
/// control, in screen order) by an element-less vertical scroll. The cap skipped the slider
/// (`Unverified`, not denied). Now a denied Payment refuses; the default asks naming it.
#[test]
fn m2_a_pay_slider_as_the_33rd_crossed_control() {
    let mut kids = Vec::new();
    for k in 0..32 {
        kids.push(
            UiNode::new(
                Role::Picker,
                rect(186.0, 200.0 + f64::from(k) * 12.0, 20.0, 10.0),
            )
            .with_label(format!("Volume {k}"))
            .hittable(true),
        );
    }
    kids.push(
        UiNode::new(Role::Slider, rect(186.0, 600.0, 20.0, 120.0))
            .with_label("Slide to pay $500")
            .hittable(true),
    );
    let s = shop(kids);
    let scroll = Action::Scroll {
        direction: Direction::Down,
        element: None,
    };
    let mut deny = config().policy;
    deny.deny_categories = vec![RiskCategory::Payment];
    let d = decide(&deny, &scroll, &s);
    assert!(matches!(d, Decision::Deny(_)), "{d:?}");
    let Decision::Confirm(q) = decide(&config().policy, &scroll, &s) else {
        panic!("not asked")
    };
    assert!(q.phone_only);
    assert!(
        q.summary.contains("This drag crosses 33 controls"),
        "{}",
        q.summary
    );
    assert!(q.summary.contains("Slide to pay $500"), "{}", q.summary);
}

/// The classification of every hit is linear: 10x the controls under the point takes at
/// most ~12x the time (best of three).
#[test]
fn m2_classifying_every_hit_is_linear() {
    let time = |n: usize| {
        let s = decoy_screen(n);
        let v = compact_view(&s);
        let point = Action::TapPoint {
            x: 196.5 / IPHONE_SIZE.width,
            y: 422.0 / IPHONE_SIZE.height,
        };
        (0..3)
            .map(|_| {
                let t = std::time::Instant::now();
                let c = jarvis_safety::classify(&point, &v.screen, None);
                assert_eq!(c, RiskCategory::Delete);
                t.elapsed().as_secs_f64()
            })
            .fold(f64::INFINITY, f64::min)
    };
    let small = time(400);
    let large = time(4000);
    assert!(
        large < small * 12.0 + 0.01,
        "400: {small:.5}s, 4000: {large:.5}s"
    );
}

// ---------------------------------------------------------------------------------------
// L1: cut and flag at every intake; the session reads an incomplete screen and leaves.
// ---------------------------------------------------------------------------------------

fn wire_round_trip(s: &Screen) -> Screen {
    use jarvis_runner::message::{decode_reply, encode_reply};
    use jarvis_runner::{LockState, Reply, Snapshot};
    let snap = Snapshot {
        root: s.root.clone(),
        app_bundle_id: s.app_bundle_id.clone(),
        app_name: s.app_name.clone(),
        screen_size: s.size,
        keyboard_visible: s.keyboard_visible,
        orientation: s.orientation,
        device_locked: false,
        lock_state: LockState::Unlocked,
        notes: vec![],
        screenshot: None,
        tree_cut: false,
    };
    let bytes = encode_reply(7, &Reply::Snapshot(Box::new(snap))).expect("encoded, not refused");
    let (_, reply) = decode_reply(&bytes).expect("decoded");
    let Reply::Snapshot(back) = reply else {
        panic!("not a snapshot")
    };
    Screen {
        app_bundle_id: back.app_bundle_id,
        app_name: back.app_name,
        size: back.screen_size,
        root: back.root,
        keyboard_visible: back.keyboard_visible,
        orientation: back.orientation,
        tree_cut: back.tree_cut,
    }
}

/// A 170-deep and a 1,000-deep tree cross the runner wire (it refused them: "the reply
/// could not be encoded: tree depth"), cut at the bound and flagged; in a session the
/// screen is incomplete: Home works, a tap asks (phone only, with the warning), a denied
/// category refuses the tap.
#[test]
fn l1_a_deep_tree_crosses_the_wire_and_the_session_can_leave() {
    for depth in [170usize, 1_000] {
        let deep = big_stack(move || wire_round_trip(&deep_with(depth, buy_now())));
        assert!(deep.tree_cut, "{depth}");
        assert_eq!(levels(&deep.root), MAX_TREE_DEPTH, "{depth}");
        assert!(compact_view(&deep).screen.cues.truncated);
        // Home.
        let r = drive(
            deep.clone(),
            config(),
            &[r#"{"action":"press_home"}"#, FAIL],
        );
        assert!(sent_home(&r), "{depth}: {:?}", r.acts);
        // A tap asks.
        let r = drive(
            deep.clone(),
            config(),
            &[r#"{"action":"tap","element":2}"#, FAIL],
        );
        assert_eq!(gestures(&r), 0);
        let q = r.confirms.first().expect("asked");
        assert!(q.phone_only && q.summary.starts_with(INCOMPLETE_SCREEN_WARNING));
        // A denial refuses it, and Home still works.
        let mut c = config();
        c.policy.deny_categories = vec![RiskCategory::Payment];
        let r = drive(
            deep.clone(),
            c.clone(),
            &[r#"{"action":"tap","element":2}"#, FAIL],
        );
        assert!(r.confirms.is_empty() && gestures(&r) == 0);
        assert!(r.all_prompt_text().contains(REFUSED));
        let r = drive(deep, c, &[r#"{"action":"press_home"}"#, FAIL]);
        assert!(sent_home(&r));
    }
}

/// A tree that fits is not flagged; one level past the bound is (the encoder's cut is at
/// exactly the bound).
#[test]
fn l1_the_wire_cuts_at_exactly_the_bound() {
    let mk = |levels: usize| {
        // The Application, the Window and `levels - 3` wrappers above the leaf.
        deep_with(levels - 3, buy_now())
    };
    let at = big_stack(move || wire_round_trip(&mk(MAX_TREE_DEPTH)));
    assert!(!at.tree_cut);
    assert_eq!(levels(&at.root), MAX_TREE_DEPTH);
    assert!(!compact_view(&at).screen.cues.truncated);
    let over = big_stack(move || wire_round_trip(&mk(MAX_TREE_DEPTH + 1)));
    assert!(over.tree_cut);
    assert_eq!(levels(&over.root), MAX_TREE_DEPTH);
}

/// Node count: the encoder writes the first 100,000 nodes in pre-order and flags the cut (a
/// deliberate cut, not a refusal: the session can then leave the screen).
#[test]
fn l1_the_wire_cuts_at_the_node_count_and_flags_it() {
    let n = jarvis_runner::MAX_TREE_NODES + 5;
    let kids: Vec<UiNode> = (0..n)
        .map(|i| node(Role::StaticText, &format!("t{i}"), rect(0.0, 0.0, 1.0, 1.0)))
        .collect();
    let s = shop(kids);
    let back = wire_round_trip(&s);
    assert!(back.tree_cut);
    let mut count = 0usize;
    let mut stack = vec![&back.root];
    while let Some(e) = stack.pop() {
        count += 1;
        stack.extend(e.children.iter());
    }
    assert_eq!(count, jarvis_runner::MAX_TREE_NODES);
}

/// The runner-text intake: a line deeper than the bound is cut (it was flattened to
/// siblings at the cap, unflagged) and the screen is flagged.
#[test]
fn l1_runner_text_deeper_than_the_bound_is_cut_and_flagged() {
    let mut text = String::new();
    for d in 0..200usize {
        text.push_str(&"  ".repeat(d));
        text.push_str(&format!(
            "Other, label: 'L{d}', {{{{0, 0}}, {{393, 852}}}}\n"
        ));
    }
    let t = big_stack(move || jarvis_observe::parse_runner_text(&text).unwrap());
    assert!(t.cut);
    assert_eq!(levels(&t.root), MAX_TREE_DEPTH);
    let mut shallow = String::new();
    for d in 0..10usize {
        shallow.push_str(&"  ".repeat(d));
        shallow.push_str(&format!(
            "Other, label: 'L{d}', {{{{0, 0}}, {{393, 852}}}}\n"
        ));
    }
    assert!(!jarvis_observe::parse_runner_text(&shallow).unwrap().cut);
}

/// Runner text cut at the bound keeps its source (`RunnerText`) and gets the incomplete
/// rules from the cue: a denial refuses a vertical scroll and a tap there; Home stays.
#[test]
fn l1_cut_runner_text_gets_the_incomplete_rules() {
    let mut s = deep_with(3, buy_now());
    s.tree_cut = true;
    let v = compact_view(&s);
    assert!(v.screen.cues.truncated);
    let mut deny = config().policy;
    deny.deny_categories = vec![RiskCategory::Payment];
    let judge = |a: &Action| {
        evaluate_owned(
            &deny,
            a,
            &v.screen,
            None,
            ScreenSource::RunnerText,
            None,
            &[],
            &[],
        )
    };
    for a in [
        Action::Scroll {
            direction: Direction::Down,
            element: None,
        },
        Action::Tap { element: 2 },
    ] {
        assert!(matches!(judge(&a), Decision::Deny(_)), "{a:?}");
    }
    assert!(!matches!(judge(&Action::PressHome), Decision::Deny(_)));
}

/// A flagged screen is incomplete even when nothing deeper than the bound remains.
#[test]
fn l1_the_flag_alone_makes_the_screen_incomplete() {
    let mut s = deep_with(3, buy_now());
    assert!(!compact_view(&s).screen.cues.truncated);
    s.tree_cut = true;
    assert!(compact_view(&s).screen.cues.truncated);
    let r = drive(
        s,
        config(),
        &[r#"{"action":"scroll","direction":"down"}"#, FAIL],
    );
    let q = r.confirms.first().expect("asked");
    assert!(q.summary.starts_with(INCOMPLETE_SCREEN_WARNING));
}

/// Run 151's class, on the Rust side: every walk the bound protects, on a tree at exactly
/// the bound, on a 512 KB thread (an iOS secondary thread's) in a debug build: the
/// compaction and its layout and raw-tree walks, the render, the digest, the home-screen
/// mode, the session's step (places, the gate, the prompt), the runner wire both ways, JSON
/// and the tree's own `Drop`.
#[test]
fn l1_every_walk_at_the_bound_runs_on_a_512_kb_stack() {
    let worker = std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(|| {
            let s = deep_with(MAX_TREE_DEPTH - 3, buy_now());
            assert_eq!(levels(&s.root), MAX_TREE_DEPTH);
            let v = compact_view(&s);
            assert!(!v.screen.cues.truncated);
            let _ = jarvis_observe::render(&v.screen, 4_000);
            let _ = jarvis_observe::screen_digest(&s);
            let _ = jarvis_safety::home_screen_mode(&s.root);
            let mut sb = s.clone();
            sb.app_bundle_id = jarvis_proto::SPRINGBOARD_BUNDLE_ID.to_owned();
            let _ = compact_view(&sb);
            let _ = jarvis_safety::home_screen_mode(&sb.root);
            let back = wire_round_trip(&s);
            assert!(!back.tree_cut);
            let json = serde_json::to_string(&s).unwrap();
            let _: Result<Screen, _> = serde_json::from_str(&json);
            let r = run(
                config(),
                "Look",
                MemoryStore::new(),
                Phone::still(s.clone()),
                Script::model(&[
                    r#"{"action":"scroll","direction":"down"}"#,
                    r#"{"action":"tap","element":2}"#,
                    FAIL,
                ])
                .decisions(&[true; 4]),
            );
            drop(back);
            drop(sb);
            drop(s);
            r.acts.len()
        })
        .unwrap();
    assert!(worker.join().unwrap() > 0);
}

// ---------------------------------------------------------------------------------------
// L2: list rows bound the drag; grid cells keep the screen cap (and the question).
// ---------------------------------------------------------------------------------------

macro_rules! fixture {
    ($dir:literal, $name:literal) => {
        include_str!(concat!("fixtures/", $dir, "/", $name))
    };
}

fn parse(text: &str) -> Screen {
    let typed = text
        .split("\n== raw XCUITest snapshot")
        .next()
        .unwrap_or(text);
    ios::parse_screen(typed).unwrap()
}

fn planned(
    s: &Screen,
    kind: GestureKind,
    dir: Direction,
    frame: Option<Rect>,
) -> Result<f64, GestureError> {
    let v = compact_view(s);
    planned_drag(kind, dir, frame, &v.screen, Some(&tree(&v)))
        .map(|g| (g.to.x - g.from.x).hypot(g.to.y - g.from.y))
}

/// The real fixtures the bound refused (reviewer A5): the Safari start page's favourites,
/// a page's date input, the compact date picker, the edit menus and the Home Screen's
/// widgets (whose 176.7-pt widget-stack pager pages read as rows of a 176.7-pt list). Each
/// sideways drag that crosses a cell is now planned (the screen cap) and still asked, phone
/// only, with the row-swipe note.
#[test]
fn l2_real_grid_cells_get_a_planned_drag_and_still_ask() {
    let fixtures: [(&str, &str); 6] = [
        ("safari_form/02.txt", fixture!("safari_form", "02.txt")),
        (
            "p6b-safari-inputs-04",
            fixture!("ios26_probes3", "p6b-safari-inputs-04-date-input-tree.txt"),
        ),
        (
            "p4d-controls-10",
            fixture!(
                "ios26_probes3",
                "p4d-controls-10-compact-date-open-tree.txt"
            ),
        ),
        ("edit_menu", fixture!("ios26_probes", "edit_menu.txt")),
        (
            "edit_menu_list",
            fixture!("ios26_probes", "edit_menu_list.txt"),
        ),
        (
            "p15-springboard-01",
            fixture!("ios26_probes3", "p15-springboard-01-home-tree.txt"),
        ),
    ];
    for (name, text) in fixtures {
        let s = parse(text);
        let v = compact_view(&s);
        let mut frames: Vec<Option<Rect>> = vec![None];
        frames.extend(v.screen.elements.iter().map(|e| Some(e.frame)));
        let mut asked = 0usize;
        for f in &frames {
            for (kind, dir) in [
                (GestureKind::Scroll, Direction::Right),
                (GestureKind::Swipe, Direction::Left),
            ] {
                match planned(&s, kind, dir, *f) {
                    Ok(t) => assert!(t <= v.screen.size.width * MAX_SIDEWAYS_SHARE + 1e-6),
                    Err(e) => assert_ne!(e, GestureError::RowTooNarrow, "{name}: {f:?} {kind:?}"),
                }
            }
            // The question is unchanged: a sideways drag across a cell still asks.
            let Some(f) = f else { continue };
            let Some(e) = v.screen.elements.iter().find(|e| e.frame == *f) else {
                continue;
            };
            let a = Action::Swipe {
                direction: Direction::Left,
                element: Some(e.index),
            };
            if jarvis_safety::round3_question_notes(&a, &v.screen, Some(&tree(&v)))
                .iter()
                .any(|n| n.text.contains(ROW_SWIPE_NOTE))
            {
                let d = decide(&config().policy, &a, &s);
                match d {
                    Decision::Confirm(q) => {
                        assert!(q.phone_only, "{name}: {q:?}");
                        asked += 1;
                    }
                    Decision::Deny(_) => {}
                    Decision::Allow => panic!("{name}: a row swipe went unasked: {e:?}"),
                }
            }
        }
        assert!(
            asked > 0 || name == "p15-springboard-01",
            "{name}: no row-swipe question"
        );
    }
}

/// A narrow cell in a full-width list (the narrowing of a row's `Cell` that pass 28 and 29
/// forged) is no list row for the bound: the drag keeps the screen cap — and the question,
/// unchanged, still asks, phone only, with the row-swipe note. The undrawable-cell forgery
/// (pass 29) keeps the bound, from the node's own frame (fail closed).
#[test]
fn l2_a_narrow_cell_in_a_wide_list_keeps_the_question() {
    let p13 = parse(fixture!(
        "ios26_probes3",
        "p13-swipe-lengths-01-rows-tree.txt"
    ));
    let mut s = p13.clone();
    fn edit(n: &mut UiNode, f: &mut impl FnMut(&mut UiNode)) {
        f(n);
        for c in &mut n.children {
            edit(c, f);
        }
    }
    edit(&mut s.root, &mut |n| {
        if n.role == Role::Cell {
            n.frame.x = 145.0;
            n.frame.width = 150.0;
        }
    });
    let v = compact_view(&s);
    let row = v
        .screen
        .elements
        .iter()
        .find(|e| e.label == "Full swipe row 1")
        .unwrap();
    let a = Action::Swipe {
        direction: Direction::Left,
        element: Some(row.index),
    };
    let Decision::Confirm(q) = decide(&config().policy, &a, &s) else {
        panic!("not asked")
    };
    assert!(
        q.phone_only && q.summary.contains(ROW_SWIPE_NOTE),
        "{}",
        q.summary
    );
    assert!(planned(&s, GestureKind::Swipe, Direction::Left, Some(row.frame)).is_ok());
    // Undrawable cells (pass 29): the bound stays, from the content's own frame.
    let mut u = p13;
    edit(&mut u.root, &mut |n| {
        if n.role == Role::Cell {
            n.frame = Rect::new(0.0, 0.0, 0.0, 0.0);
        }
    });
    let v = compact_view(&u);
    let row = v
        .screen
        .elements
        .iter()
        .find(|e| e.label == "Full swipe row 1")
        .unwrap();
    let t = planned(&u, GestureKind::Swipe, Direction::Left, Some(row.frame));
    match t {
        Ok(t) => assert!(t <= 0.45 * row.frame.width.max(1.0) + 1e-6, "{t}"),
        Err(e) => assert_eq!(e, GestureError::RowTooNarrow),
    }
}

/// A 150-pt-wide LIST's rows are still refused (a list row too narrow for an 80-pt drag).
#[test]
fn l2_a_narrow_lists_rows_are_still_refused() {
    let row =
        UiNode::new(Role::Cell, Rect::new(120.0, 300.0, 150.0, 52.0)).with_children(vec![node(
            Role::StaticText,
            "Row",
            rect(120.0, 300.0, 150.0, 52.0),
        )]);
    let list =
        UiNode::new(Role::Table, Rect::new(120.0, 100.0, 150.0, 700.0)).with_children(vec![row]);
    let s = shop(vec![list]);
    let v = compact_view(&s);
    let f = v
        .screen
        .elements
        .iter()
        .find(|e| e.label == "Row")
        .unwrap()
        .frame;
    assert_eq!(
        planned(&s, GestureKind::Swipe, Direction::Left, Some(f)),
        Err(GestureError::RowTooNarrow)
    );
}
