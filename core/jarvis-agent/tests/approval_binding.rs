#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! What an approval is bound to (fifth review): the raw digest with every node's shown bit
//! (A1), the compact screen's content (A2), the target's raw node (A3) and the summary with
//! its category (A4); and the exclusions, which only cover what content cannot imitate.
//! The first five tests are the fifth review's probes, kept as regressions.

mod common;

use common::*;
use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_observe::{compact, screen_digest};
use jarvis_proto::fixtures::{keyboard, node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{
    Effect, Event, ModelOutput, Observation, Point, Role, RunnerCommand, Screen, TaskSpec,
    TimedEvent, UiNode,
};

const BANK: &str = "com.example.bank";

fn app_root_of(children: Vec<UiNode>) -> UiNode {
    UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Bank")
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(children)
        ])
}

fn scr(children: Vec<UiNode>) -> Screen {
    screen(BANK, "Bank", app_root_of(children), false)
}

struct D {
    s: Session,
    now: u64,
}

impl D {
    fn new() -> Self {
        Self::with(MemoryStore::new())
    }
    fn with(memory: MemoryStore) -> Self {
        D {
            s: Session::new(config(), TaskSpec::new("t", "Pay my bill"), memory),
            now: 1_000,
        }
    }
    fn step(&mut self, e: Event) -> Effect {
        self.now += 50;
        self.s.step(TimedEvent::new(self.now, e))
    }
    fn observed(&mut self, s: &Screen) -> Effect {
        self.step(Event::Observed(Observation::new(s.clone())))
    }
    /// Start on `asked`, the model proposes `action`; returns the confirmation summary.
    fn ask(&mut self, asked: &Screen, action: &str) -> String {
        assert!(matches!(self.step(Event::Start), Effect::Observe { .. }));
        assert!(matches!(self.observed(asked), Effect::Infer(_)));
        let e = self.step(Event::ModelOutput(ModelOutput {
            text: action.to_owned(),
        }));
        let Effect::Confirm(req) = e else {
            panic!("expected a confirmation, got {e:?}")
        };
        req.summary
    }
    /// [`D::ask`] for a tap on compact element `idx`, then the owner approves.
    fn ask_approve(&mut self, asked: &Screen, idx: u32) -> String {
        let summary = self.ask(asked, &format!(r#"{{"action":"tap","element":{idx}}}"#));
        self.approve();
        summary
    }
    fn approve(&mut self) {
        assert_eq!(
            self.step(Event::UserDecision { approved: true }),
            Effect::Observe { screenshot: false }
        );
    }
    /// The look after the approval shows `fresh` (also at the settle retry).
    fn look(&mut self, fresh: &Screen) -> Effect {
        let e = self.observed(fresh);
        if let Effect::Act(RunnerCommand::Wait { .. }) = e {
            let r = self.step(Event::ActResult(jarvis_proto::RunnerResult::ok()));
            assert!(matches!(r, Effect::Observe { .. }), "{r:?}");
            return self.observed(fresh);
        }
        e
    }
}

fn node_with_id<'a>(n: &'a UiNode, id: &str) -> Option<&'a UiNode> {
    if n.identifier.as_deref() == Some(id) {
        return Some(n);
    }
    n.children.iter().find_map(|c| node_with_id(c, id))
}

fn pay_index(s: &Screen) -> u32 {
    compact(s)
        .elements
        .iter()
        .find(|e| e.label == "Pay")
        .unwrap()
        .index
}

// ---------------------------------------------------------------------------------------
// A. The binding.
// ---------------------------------------------------------------------------------------

fn bills(swapped: bool) -> Screen {
    let (y1, y2) = if swapped {
        (400.0, 200.0)
    } else {
        (200.0, 400.0)
    };
    scr(vec![
        text("Electricity bill $40.00", rect(16.0, y1, 250.0, 30.0)),
        node(Role::Button, "Pay", rect(280.0, y1, 90.0, 30.0)).with_identifier("pay-electricity"),
        text("Mallory Ltd invoice $4,000.00", rect(16.0, y2, 250.0, 30.0)),
        node(Role::Button, "Pay", rect(280.0, y2, 90.0, 30.0)).with_identifier("pay-mallory"),
    ])
}

/// Fifth review probe: twins swapped by layout (only frames moved) change the compact
/// screen's order: void, never Mallory's "Pay". Seventh review: frames are hashed, so the
/// digest differs too (was: equal; deliberate change).
#[test]
fn twin_pay_buttons_swapped_by_layout_do_not_pay_the_other_bill() {
    let asked = bills(false);
    let fresh = bills(true);
    assert_ne!(
        screen_digest(&asked),
        screen_digest(&fresh),
        "frames are hashed"
    );
    let mut d = D::new();
    let summary = d.ask_approve(&asked, pay_index(&asked));
    let e = d.look(&fresh);
    if let Effect::Act(RunnerCommand::Tap { x, y }) = e {
        let mallory = node_with_id(&fresh.root, "pay-mallory").unwrap();
        assert!(
            !mallory.frame.contains(Point::new(x, y)),
            "approved '{summary}'; tapped Mallory's Pay"
        );
    }
    assert!(!matches!(e, Effect::Act(_)), "{e:?}");
}

/// Two identical twins (same role, label, everything but their identifiers' order in the
/// tree is the same) that only swap frames: the digest and the compact content are equal,
/// but compact element 1 now comes from the other raw node (A3), so the approval is void.
/// Voiding (rather than tapping the approved raw node where it is now) is the safe choice:
/// the owner approved "the first Pay on the screen", and what the screen shows first is
/// now another node; nothing ever taps a node the approval was not given on.
#[test]
fn identical_twins_that_swap_frames_void_the_approval() {
    let twins = |swapped: bool| {
        let (y1, y2) = if swapped {
            (400.0, 200.0)
        } else {
            (200.0, 400.0)
        };
        scr(vec![
            node(Role::Button, "Pay", rect(16.0, y1, 361.0, 50.0)),
            node(Role::Button, "Pay", rect(16.0, y2, 361.0, 50.0)),
        ])
    };
    let asked = twins(false);
    let fresh = twins(true);
    // Seventh review: frames are hashed, so the digest voids it first (was: equal).
    assert_ne!(screen_digest(&asked), screen_digest(&fresh));
    let strip = |s: &Screen| {
        let mut c = compact(s);
        for e in &mut c.elements {
            e.frame = jarvis_proto::Rect::default();
            e.fingerprint = 0;
        }
        c
    };
    assert_eq!(
        strip(&asked),
        strip(&fresh),
        "everything but frames is equal"
    );
    let mut d = D::new();
    d.ask_approve(&asked, 1);
    let e = d.look(&fresh);
    assert!(!matches!(e, Effect::Act(_)), "{e:?}");
    // Unswapped, the same approval runs (control).
    let mut d = D::new();
    d.ask_approve(&asked, 1);
    assert!(matches!(
        d.look(&twins(false)),
        Effect::Act(RunnerCommand::Tap { .. })
    ));
}

fn bill_list(dy: f64) -> Screen {
    let rows = [
        ("Electricity bill $40.00", "pay-1"),
        ("Mallory Ltd invoice $4,000.00", "pay-2"),
        ("Water bill $25.00", "pay-3"),
    ];
    let mut kids = Vec::new();
    for (k, (t, id)) in rows.iter().enumerate() {
        let y = 100.0 + 100.0 * k as f64 - dy;
        kids.push(text(t, rect(16.0, y, 250.0, 30.0)));
        kids.push(node(Role::Button, "Pay", rect(280.0, y, 90.0, 30.0)).with_identifier(*id));
    }
    scr(kids)
}

/// Fifth review probe: the list scrolled 125 pt and the first row left the shown set, so
/// every index shifted by a row. The shown bit is hashed (A1): void.
#[test]
fn a_scroll_that_moves_a_row_out_of_the_shown_set_voids() {
    let asked = bill_list(0.0);
    let fresh = bill_list(125.0);
    assert_ne!(screen_digest(&asked), screen_digest(&fresh));
    let mut d = D::new();
    let summary = d.ask_approve(&asked, pay_index(&asked));
    let e = d.look(&fresh);
    if let Effect::Act(RunnerCommand::Tap { x, y }) = e {
        let second = node_with_id(&fresh.root, "pay-2").unwrap();
        assert!(
            !second.frame.contains(Point::new(x, y)),
            "approved '{summary}'; tapped the Mallory row's Pay"
        );
    }
    assert!(!matches!(e, Effect::Act(_)), "{e:?}");
}

/// A scroll that keeps the shown set and order. Seventh review: the layout is part of the
/// approval (frames are hashed), so it voids; was: ran on the same raw node at its new frame
/// (deliberate change). The same screen still runs.
#[test]
fn a_scroll_that_keeps_the_shown_set_taps_the_same_node_at_its_new_frame() {
    let asked = bill_list(0.0);
    let fresh = bill_list(20.0);
    assert_ne!(screen_digest(&asked), screen_digest(&fresh));
    let mut d = D::new();
    d.ask_approve(&asked, pay_index(&asked));
    let e = d.look(&fresh);
    assert!(!matches!(e, Effect::Act(_)), "{e:?}");
    let mut d = D::new();
    d.ask_approve(&asked, pay_index(&asked));
    let e = d.look(&asked);
    let Effect::Act(RunnerCommand::Tap { x, y }) = e else {
        panic!("expected the tap, got {e:?}")
    };
    let first = node_with_id(&asked.root, "pay-1").unwrap();
    assert!(first.frame.contains(Point::new(x, y)), "({x}, {y})");
}

/// Rows moved without changing the tree or the compact order: the summary recomputed on
/// the fresh screen differs ("next to" became "under"), so the approval is void (A4).
#[test]
fn rows_moved_with_the_same_order_but_another_summary_void() {
    let bill = |pay_y: f64| {
        scr(vec![
            text("Electricity bill", rect(16.0, 100.0, 250.0, 30.0)),
            node(Role::Button, "Pay", rect(280.0, pay_y, 90.0, 30.0)),
        ])
    };
    let asked = bill(100.0);
    let fresh = bill(140.0);
    // Seventh review: frames are hashed, so the digest differs too (was: equal).
    assert_ne!(screen_digest(&asked), screen_digest(&fresh));
    let labels = |s: &Screen| {
        compact(s)
            .elements
            .iter()
            .map(|e| e.label.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(labels(&asked), labels(&fresh), "same order");
    let mut d = D::new();
    let summary = d.ask_approve(&asked, pay_index(&asked));
    assert!(
        summary.contains("next to \"Electricity bill\""),
        "{summary}"
    );
    let e = d.look(&fresh);
    assert!(!matches!(e, Effect::Act(_)), "{e:?}");
}

/// Fifth review probe: two panels in the tree, one shown; only their visibility flags
/// change. The shown bit is hashed (A1): void.
#[test]
fn a_hidden_panel_that_becomes_shown_voids_the_approval() {
    let panel = |amount: &str, shown: bool| {
        let mut p = UiNode::new(Role::Other("Other".into()), rect(0.0, 150.0, 393.0, 600.0))
            .with_children(vec![
                text(
                    &format!("Send {amount} to Mallory"),
                    rect(16.0, 200.0, 361.0, 30.0),
                ),
                node(Role::Button, "Confirm", rect(16.0, 600.0, 361.0, 50.0)),
            ]);
        fn set(n: &mut UiNode, v: bool) {
            n.visible = v;
            n.hittable = v;
            for c in &mut n.children {
                set(c, v);
            }
        }
        set(&mut p, shown);
        p
    };
    let make = |first: bool| scr(vec![panel("$5.00", first), panel("$5,000.00", !first)]);
    let asked = make(true);
    let fresh = make(false);
    assert_ne!(screen_digest(&asked), screen_digest(&fresh));
    let idx = index_of(&asked, "Confirm");
    let mut d = D::new();
    let summary = d.ask_approve(&asked, idx);
    let e = d.look(&fresh);
    assert!(
        !matches!(e, Effect::Act(_)),
        "approved '{summary}', acted with $5,000.00 shown: {e:?}"
    );
}

// ---------------------------------------------------------------------------------------
// B. Exclusions.
// ---------------------------------------------------------------------------------------

fn wrapped_in(wrapper: UiNode, amount: &str, kb: bool) -> Screen {
    let mut kids = vec![wrapper.with_children(vec![
        text(
            &format!("Transfer to Mallory {amount}"),
            rect(16.0, 200.0, 361.0, 30.0),
        ),
        node(Role::Button, "Send money", rect(16.0, 400.0, 361.0, 50.0)),
    ])];
    if kb {
        // A keyboard in the app's own window (a text field focused).
        kids.push(keyboard("Done"));
    }
    screen(BANK, "Bank", app_root_of(kids), kb)
}

fn approve_then_change(wrapper: fn() -> UiNode, kb: bool) -> (String, Effect) {
    let asked = wrapped_in(wrapper(), "$5.00", kb);
    let fresh = wrapped_in(wrapper(), "$5,000.00", kb);
    let idx = index_of(&asked, "Send money");
    let mut d = D::new();
    let summary = d.ask_approve(&asked, idx);
    (summary, d.look(&fresh))
}

/// Fifth review probe: a `StatusBar` node inside an app is ordinary content: hashed,
/// listed, and a change in it voids.
#[test]
fn a_status_bar_role_inside_an_app_is_content() {
    let bar = || UiNode::new(Role::Other("StatusBar".into()), rect(0.0, 0.0, 393.0, 1.0));
    let s = wrapped_in(bar(), "$5.00", false);
    assert!(compact(&s)
        .elements
        .iter()
        .any(|e| e.label == "Transfer to Mallory $5.00"));
    assert_ne!(
        screen_digest(&s),
        screen_digest(&wrapped_in(bar(), "$5,000.00", false))
    );
    let (summary, e) = approve_then_change(bar, false);
    assert!(summary.contains("$5.00"), "{summary}");
    assert!(!matches!(e, Effect::Act(_)), "{e:?}");
}

/// Fifth review probe: the identifier `SystemInputAssistantView` (which any app can set)
/// excludes nothing without a keyboard in its window. Seventh review: in the keyboard's
/// window its text is left out of the digest, and so (the invariant) out of the compact
/// screen: content an app wraps in it there cannot be tapped, read or summarised, and its
/// structure and geometry stay hashed (was: hashed and listed as any content; deliberate
/// change). Eighth review: only the assistant's own node is hashed (role, frame,
/// identifier, flags), not its inner structure, so a node moving inside it keeps the
/// digest (was: changed it; deliberate change); moving the assistant itself still changes
/// it, and nothing inside it is listed or tappable either way.
#[test]
fn an_input_assistant_identifier_hides_nothing() {
    let bar = || {
        UiNode::new(Role::Other("Other".into()), IPHONE_SIZE.to_rect())
            .with_identifier("SystemInputAssistantView")
    };
    let (summary, e) = approve_then_change(bar, false);
    assert!(!matches!(e, Effect::Act(_)), "'{summary}' {e:?}");
    let asked = wrapped_in(bar(), "$5.00", true);
    let c = compact(&asked);
    assert!(
        c.elements
            .iter()
            .all(|e| e.label != "Send money" && !e.label.contains("Mallory")),
        "{:?}",
        c.elements
    );
    let mut moved = wrapped_in(bar(), "$5.00", true);
    moved.root.children[0].children[0].children[1].frame.y += 10.0;
    assert_eq!(screen_digest(&asked), screen_digest(&moved));
    let mut moved = wrapped_in(bar(), "$5.00", true);
    moved.root.children[0].children[0].frame.y += 10.0;
    assert_ne!(screen_digest(&asked), screen_digest(&moved));
}

/// A screen whose keyboard is in a top-level window of its own (as on iOS 26), or in the
/// app's window (`shared`), or absent.
fn pay_screen(keyboard_up: bool, shared: bool, return_label: &str) -> Screen {
    let mut content = vec![
        text("Total $42.10", rect(16.0, 200.0, 361.0, 30.0)),
        node(Role::Button, "Pay", rect(16.0, 300.0, 361.0, 50.0)),
    ];
    let mut windows = Vec::new();
    if keyboard_up && shared {
        content.push(keyboard(return_label));
    }
    windows.push(UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(content));
    if keyboard_up && !shared {
        // Seventh review: every node of the keyboard's window covers what is under it, so a
        // full-screen keyboard window would cover "Pay" (see
        // `a_full_screen_keyboard_window_covers_every_tap`); this window is the keyboard's
        // area.
        windows.push(
            UiNode::new(Role::Window, rect(0.0, 516.0, 393.0, 336.0))
                .with_children(vec![keyboard(return_label)]),
        );
    }
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Bank")
        .with_children(windows);
    screen(BANK, "Bank", root, keyboard_up)
}

/// Sixth review: the keyboard is hashed like any other node, so a keyboard that appears,
/// hides or changes between the question and the look voids even a plain tap (documented
/// fail-closed). Was: it did not void a plain tap (its window was excluded); deliberate
/// change.
#[test]
fn a_keyboard_that_appears_or_hides_voids_a_plain_tap() {
    for shared in [false, true] {
        for (asked_kb, fresh_kb, label) in [
            (false, true, "Send"),
            (true, false, "Send"),
            (true, true, "Send"),
        ] {
            let asked = pay_screen(asked_kb, shared, "Done");
            let fresh = pay_screen(fresh_kb, shared, label);
            assert_ne!(screen_digest(&asked), screen_digest(&fresh));
            let mut d = D::new();
            d.ask_approve(&asked, index_of(&asked, "Pay"));
            let e = d.look(&fresh);
            assert!(
                !matches!(e, Effect::Act(RunnerCommand::Tap { .. })),
                "shared {shared}, {asked_kb} -> {fresh_kb}: {e:?}"
            );
        }
    }
    // The same keyboard on both looks keeps the approval.
    let asked = pay_screen(true, false, "Done");
    let mut d = D::new();
    d.ask_approve(&asked, index_of(&asked, "Pay"));
    assert!(matches!(
        d.look(&asked),
        Effect::Act(RunnerCommand::Tap { .. })
    ));
}

fn chat(return_label: &str) -> Screen {
    chat_with(return_label, None)
}

fn chat_with(return_label: &str, value: Option<&str>) -> Screen {
    let mut f = UiNode::new(Role::TextField, rect(16.0, 300.0, 361.0, 44.0))
        .with_label("Message")
        .hittable(true);
    if let Some(v) = value {
        f = f.with_value(v);
    }
    f.focused = true;
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Chat")
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect())
                .with_children(vec![text("Mom", rect(16.0, 80.0, 200.0, 30.0)), f]),
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect())
                .with_children(vec![keyboard(return_label)]),
        ]);
    screen("com.example.chat", "Chat", root, true)
}

/// Sixth review: a submit's Return is gated on its own after the typing, on the post-typing
/// screen, whose Return key is in the record (and, the keys being hashed, in the digest):
/// "send" turning into "go" between that question and the look voids it. The keys are
/// hashed for every action, so the same change voids a plain tap too. Was: the Return key
/// was recorded only for a submit and a plain tap kept its approval (deliberate change).
#[test]
fn a_return_key_that_changes_voids_the_return_and_a_tap() {
    let asked = chat("send");
    let idx = index_of(&asked, "Message");
    let submit = format!(r#"{{"action":"type","element":{idx},"text":"hi","submit":true}}"#);
    for (fresh, acts) in [
        (chat_with("send", Some("hi")), true),
        (chat_with("go", Some("hi")), false),
    ] {
        let mut d = D::new();
        assert!(matches!(d.step(Event::Start), Effect::Observe { .. }));
        assert!(matches!(d.observed(&asked), Effect::Infer(_)));
        let e = d.step(Event::ModelOutput(ModelOutput {
            text: submit.clone(),
        }));
        assert!(
            matches!(e, Effect::Act(RunnerCommand::TypeText { .. })),
            "{e:?}"
        );
        let e = d.step(Event::ActResult(jarvis_proto::RunnerResult::ok()));
        assert!(matches!(e, Effect::Observe { .. }), "{e:?}");
        let Effect::Confirm(req) = d.observed(&chat_with("send", Some("hi"))) else {
            panic!("the Return is asked after typing")
        };
        assert!(req.summary.contains("Press Return"), "{}", req.summary);
        d.approve();
        let e = d.look(&fresh);
        assert_eq!(
            matches!(e, Effect::Act(RunnerCommand::PressKey { .. })),
            acts,
            "{e:?}"
        );
    }
    let tap = pay_screen(true, false, "send");
    let mut d = D::new();
    d.ask_approve(&tap, index_of(&tap, "Pay"));
    assert!(!matches!(
        d.look(&pay_screen(true, false, "go")),
        Effect::Act(_)
    ));
}

// ---------------------------------------------------------------------------------------
// D. Summaries travel: nothing protected reaches them, or what is built from them.
// ---------------------------------------------------------------------------------------

/// Credential and bank fields are listed by name only, the owner's profile values are
/// masked, and a decline's outcome text (built from the summary) carries none of them.
#[test]
fn summaries_and_decline_texts_never_carry_protected_values() {
    let mut memory = MemoryStore::new();
    memory.profile.set("email", "sam.secret@example.org", false);
    let field = |label: &str, value: &str, y: f64| {
        UiNode::new(Role::TextField, rect(16.0, y, 361.0, 44.0))
            .with_label(label)
            .with_value(value)
            .hittable(true)
    };
    let s = scr(vec![
        text("Verify your identity", rect(16.0, 60.0, 361.0, 30.0)),
        field("SSN", "123-45-6789", 100.0),
        field("Card number", "4111 1111 1111 1111", 150.0),
        field("CVV", "737", 200.0),
        field("IBAN", "DE89 3704 0044 0532 0130 00", 250.0),
        field("One-time code", "482913", 300.0),
        field("Email", "sam.secret@example.org", 350.0),
        text("Fee $25.00", rect(16.0, 450.0, 361.0, 30.0)),
        node(Role::Button, "Pay now", rect(16.0, 700.0, 361.0, 50.0)),
    ]);
    let mut d = D::with(memory);
    let summary = d.ask(
        &s,
        &format!(
            r#"{{"action":"tap","element":{}}}"#,
            index_of(&s, "Pay now")
        ),
    );
    let secrets = [
        "123-45-6789",
        "4111",
        "737",
        "DE89",
        "482913",
        "sam.secret@example.org",
    ];
    for v in secrets {
        assert!(!summary.contains(v), "{v} in the summary: {summary}");
    }
    assert!(summary.contains("(hidden)"), "{summary}");
    // Declined; the model then claims it is done: the outcome quotes the summary.
    assert!(matches!(
        d.step(Event::UserDecision { approved: false }),
        Effect::Infer(_)
    ));
    let e = d.step(Event::ModelOutput(ModelOutput {
        text: r#"{"action":"done","summary":"paid"}"#.to_owned(),
    }));
    let Effect::Finished(outcome) = e else {
        panic!("{e:?}")
    };
    assert!(outcome.summary.contains("declined"), "{}", outcome.summary);
    for v in secrets {
        assert!(!outcome.summary.contains(v), "{v}: {}", outcome.summary);
    }
}
