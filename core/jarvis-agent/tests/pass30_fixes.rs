#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 30 (break-it on commit 5ed51c9): regression tests for its findings. M1 a
//! silent depth cut let a deep "Delete account" escape the deny list; M2 a clear's repeated
//! focus tap went to the centre; M3 text decided on one page was typed into another after
//! the focusing tap; L1 an app named like a SpringBoard marker turned the Home Screen into
//! the unidentified layer; L2 the harness never focused a secure field, and the runner-text
//! focus rule had no test. The reviewer's tests are kept as written (`zz30_*`), with more
//! beside them.

mod common;

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{node, rect, screen, IPHONE_SIZE};
use jarvis_proto::{Effect, RiskCategory, Role, RunnerCommand, Screen, UiNode};

#[allow(dead_code)]
fn compact_text(c: &jarvis_proto::CompactScreen) -> String {
    format!("{c:?}")
}

fn nest(depth: usize, leaf: UiNode) -> UiNode {
    let mut n = leaf;
    for _ in 0..depth {
        n = UiNode::new(Role::Other("Other".into()), IPHONE_SIZE.to_rect()).with_children(vec![n]);
    }
    n
}

fn deep_screen(depth: usize) -> Screen {
    let button = UiNode::new(Role::Button, rect(16.0, 400.0, 361.0, 44.0))
        .with_label("Delete account")
        .hittable(true);
    screen(
        "com.example.shop",
        "Shop",
        UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_children(vec![UiNode::new(
            Role::Window,
            IPHONE_SIZE.to_rect(),
        )
        .with_children(vec![
            node(Role::StaticText, "Account", rect(16.0, 100.0, 200.0, 30.0)),
            nest(depth, button),
        ])]),
        false,
    )
}

fn tap_point_run_with(depth: usize, deny: bool) -> Run {
    let s = deep_screen(depth);
    let mut c = vision_config();
    if deny {
        c.policy.deny_categories = vec![RiskCategory::Delete, RiskCategory::AccountChange];
    }
    let x = 196.5 / IPHONE_SIZE.width;
    let y = 422.0 / IPHONE_SIZE.height;
    // A deep tree is built on a bigger stack (the test's own recursion: building and
    // dropping it), as the reviewer's probe did not need at depth 170.
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            run(
                c,
                "Open my account page",
                MemoryStore::new(),
                Phone::still(s),
                Script::model(&[
                    r#"{"action":"look"}"#,
                    &format!(r#"{{"action":"tap_point","x":{x:.4},"y":{y:.4}}}"#),
                    r#"{"action":"fail","reason":"stop"}"#,
                ])
                .decisions(&[true; 4]),
            )
        })
        .unwrap()
        .join()
        .unwrap()
}

fn tap_point_run(depth: usize) -> Run {
    tap_point_run_with(depth, true)
}

fn taps(r: &Run) -> usize {
    r.acts
        .iter()
        .filter(|a| matches!(a, RunnerCommand::Tap { .. }))
        .count()
}

/// Pass 30 (M1), the reviewer's test: a "Delete account" button nested past the
/// compaction's old depth cut (160; the wire allows 256) vanished from every rule.
#[test]
fn zz30_deep_button_escapes_the_deny_list() {
    let shallow = tap_point_run(5);
    assert_eq!(
        taps(&shallow),
        0,
        "shallow: {:?} {:?}",
        shallow.acts,
        shallow.confirms
    );
    let deep = tap_point_run(170);
    assert_eq!(
        taps(&deep),
        0,
        "deep tap sent: {:?} {:#?}",
        deep.acts,
        deep.confirms
    );
}

/// Pass 30 (M1): every level up to the one bound is read (the button at the wire's limit is
/// refused by the deny as at depth 5); past it the screen is incomplete: with a denied
/// category nothing that changes state runs, without one every action asks, phone only,
/// with the warning, and the model is told.
#[test]
fn a_tree_past_the_one_depth_bound_is_an_incomplete_screen() {
    let at_bound = tap_point_run(jarvis_proto::MAX_TREE_DEPTH - 4);
    assert_eq!(taps(&at_bound), 0, "{:?}", at_bound.acts);
    assert!(
        at_bound.last_user().contains("not allowed"),
        "{}",
        at_bound.last_user()
    );
    let past = tap_point_run(jarvis_proto::MAX_TREE_DEPTH + 20);
    assert_eq!(taps(&past), 0, "{:?} {:#?}", past.acts, past.confirms);
    assert!(
        past.last_user().contains("could not be read completely"),
        "{}",
        past.last_user()
    );
    let open = tap_point_run_with(jarvis_proto::MAX_TREE_DEPTH + 20, false);
    let asked = open
        .confirms
        .iter()
        .find(|c| {
            c.summary
                .starts_with(jarvis_safety::INCOMPLETE_SCREEN_WARNING)
        })
        .unwrap_or_else(|| panic!("{:#?}", open.confirms));
    assert!(asked.phone_only);
    assert!(open
        .all_prompt_text()
        .contains("deeper than Jarvis can read"));
}

/// Pass 30 (M1): the render budget hides elements from the model only — the gate still sees
/// them. A "Delete account" button the prompt does not list (a 1,024-token budget over 600
/// rows): a `tap_point` on it is refused while the owner denies `Delete` (nothing is sent,
/// nothing asked), and asked as a delete when the owner does not.
#[test]
fn the_render_budget_hides_nothing_from_the_gate() {
    let go = |deny: bool| {
        // 600 rows in the left half; the button alone in the right half.
        let mut s = long_list_screen(600);
        nav::edit(&mut s.root, &mut |n| {
            if n.role == Role::Button {
                n.frame.width = 150.0;
            }
        });
        let delete = node(
            Role::Button,
            "Delete account",
            rect(220.0, 790.0, 150.0, 40.0),
        );
        s.root.children[0].children.push(delete);
        let mut c = vision_config();
        c.model.context_tokens = 1_024;
        if deny {
            c.policy.deny_categories = vec![RiskCategory::Delete, RiskCategory::AccountChange];
        }
        run(
            c,
            "Open my account page",
            MemoryStore::new(),
            Phone::still(s),
            Script::model(&[
                r#"{"action":"look"}"#,
                &format!(
                    r#"{{"action":"tap_point","x":{:.4},"y":{:.4}}}"#,
                    295.0 / IPHONE_SIZE.width,
                    810.0 / IPHONE_SIZE.height
                ),
                r#"{"action":"fail","reason":"stop"}"#,
            ])
            .decisions(&[true; 4]),
        )
    };
    let denied = go(true);
    assert!(
        !denied.requests[0].user.contains("Delete account"),
        "the budget lists it after all"
    );
    assert_eq!(taps(&denied), 0, "{:?} {:#?}", denied.acts, denied.confirms);
    assert!(denied.confirms.is_empty(), "{:#?}", denied.confirms);
    let open = go(false);
    assert!(
        open.confirms.iter().any(|c| matches!(
            c.category,
            RiskCategory::Delete | RiskCategory::AccountChange
        )),
        "{:#?}",
        open.confirms
    );
}

/// Pass 30 (M1): the money rule (R3) reads what the budget hides: a "Total due $900.00"
/// line past the render budget makes a "Send" on the screen a payment question.
#[test]
fn the_render_budget_hides_no_money_from_the_gate() {
    let mut s = long_list_screen(600);
    nav::edit(&mut s.root, &mut |n| {
        if n.role == Role::Button {
            n.frame.width = 150.0;
            n.frame.y += 60.0;
        }
    });
    let win = &mut s.root.children[0].children;
    win.insert(
        0,
        node(Role::Button, "Send", rect(220.0, 60.0, 150.0, 40.0)),
    );
    win.push(node(
        Role::StaticText,
        "Total due $900.00",
        rect(220.0, 790.0, 150.0, 20.0),
    ));
    let send = jarvis_observe::compact(&s)
        .elements
        .iter()
        .find(|e| e.label == "Send")
        .unwrap()
        .index;
    let mut c = config();
    c.model.context_tokens = 1_024;
    let r = run(
        c,
        "Send it",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            &format!(r#"{{"action":"tap","element":{send}}}"#),
            r#"{"action":"fail","reason":"stop"}"#,
        ]),
    );
    assert!(
        !r.requests[0].user.contains("900"),
        "the budget lists it after all"
    );
    assert_eq!(
        r.confirms.first().map(|c| c.category),
        Some(RiskCategory::Payment),
        "{:#?}",
        r.confirms
    );
}

/// Pass 30 (M2), the reviewer's test: the repeated focus tap of a `clear` is the plan's own
/// tap (at the trailing edge, where the question's point is), not the centre.
#[test]
fn zz30_clear_retap_lands_elsewhere_than_the_approved_point() {
    let field = |focused: bool| {
        screen(
            "com.example.shop",
            "Shop",
            UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_children(vec![
                node(Role::StaticText, "Note", rect(16.0, 100.0, 200.0, 30.0)),
                UiNode::new(Role::TextField, rect(16.0, 200.0, 361.0, 44.0))
                    .with_label("Note")
                    .with_value("hello world")
                    .focused(focused)
                    .hittable(true),
            ]),
            false,
        )
    };
    let s = field(false);
    let i = jarvis_observe::compact(&s)
        .elements
        .iter()
        .find(|e| e.role == Role::TextField)
        .unwrap()
        .index;
    let mut taps = 0usize;
    let mut phone = Phone::new(s, move |cmd, _s: &Screen| match cmd {
        RunnerCommand::Tap { .. } => {
            taps += 1;
            Transition::Go(field(taps >= 2))
        }
        _ => Transition::Stay,
    });
    phone.auto_focus = false;
    let r = run(
        config(),
        "Clear the note",
        MemoryStore::new(),
        phone,
        Script::model(&[
            &format!(r#"{{"action":"clear","element":{i}}}"#),
            r#"{"action":"fail","reason":"stop"}"#,
        ])
        .decisions(&[true; 4]),
    );
    let pts: Vec<_> = r.acts.iter().filter_map(point_of).collect();
    assert_eq!(pts.len(), 2, "{:?}", r.acts);
    assert!(
        (pts[0].x - pts[1].x).abs() < 1.0,
        "first {:?} retap {:?}",
        pts[0],
        pts[1]
    );
}

#[allow(dead_code)]
fn _unused(_: Effect) {}

mod nav {
    use super::*;
    const PAGE: &str = include_str!("fixtures/ios26_probes3/run147-safari-form-05-tree.txt");
    const UNFOCUSED: &str = include_str!("fixtures/ios26_probes3/run147-safari-form-07-tree.txt");

    pub fn parse(text: &str) -> Screen {
        let typed = text
            .split("\n== raw XCUITest snapshot")
            .next()
            .unwrap_or(text);
        super::ios::parse_screen(typed).unwrap()
    }
    pub fn edit(n: &mut UiNode, f: &mut impl FnMut(&mut UiNode)) {
        f(n);
        for c in &mut n.children {
            edit(c, f);
        }
    }
    /// `s` with the address bar showing `host` and "Full name" focused or not.
    pub fn at(mut s: Screen, host: &str, focused: bool) -> Screen {
        edit(&mut s.root, &mut |n| {
            if n.label.as_deref() == Some("Address") {
                n.value = Some(format!("\u{200e}{host}"));
            }
            if n.role == Role::TextField && n.label.as_deref() == Some("Full name") {
                n.focused = focused;
            }
        });
        s
    }

    fn go(after_first: &'static str) -> Run {
        go_at(after_first, 2)
    }

    pub fn typed(r: &Run) -> usize {
        r.acts
            .iter()
            .filter(|a| matches!(a, RunnerCommand::TypeText { .. }))
            .count()
    }

    /// Pass 30 (M3), the reviewer's test: the first focus look already shows evil.example,
    /// the field focused.
    #[test]
    fn zz30_focus_look_types_into_a_page_of_another_host() {
        let r = go_at("evil.example", 1);
        let n = typed(&r);
        let asked = r
            .confirms
            .iter()
            .any(|c| c.summary.contains("evil.example"));
        assert!(n == 0 || asked, "typed unasked (first look)");
    }

    pub fn go_at(after_first: &'static str, from: usize) -> Run {
        go_with(after_first, from, false)
    }

    pub fn go_with(after_first: &'static str, from: usize, submit: bool) -> Run {
        let page = parse(PAGE);
        let field = jarvis_observe::compact(&page)
            .elements
            .iter()
            .find(|e| e.role == Role::TextField && e.label == "Full name")
            .unwrap()
            .index;
        let mut taps = 0usize;
        let mut phone = Phone::new(page, move |cmd, s: &Screen| match cmd {
            RunnerCommand::Tap { .. } => {
                taps += 1;
                let host = if taps >= from {
                    after_first
                } else {
                    "127.0.0.1"
                };
                Transition::Go(at(parse(UNFOCUSED), host, taps >= from))
            }
            RunnerCommand::TypeText { .. } => Transition::Go(s.clone()),
            _ => Transition::Stay,
        });
        phone.auto_focus = false;
        run(
            config(),
            "Fill in the form: Full name Ada Lovelace",
            MemoryStore::new(),
            phone,
            Script::model(&[
                &format!(
                    r#"{{"action":"type","element":{field},"text":"Ada Lovelace","submit":{submit}}}"#
                ),
                r#"{"action":"fail","reason":"stop"}"#,
            ])
            .decisions(&[true; 8]),
        )
    }

    /// Pass 30 (M3), the reviewer's test: the page navigates to another host while the focus
    /// check settles and retaps: the text decided on 127.0.0.1 is not typed into
    /// evil.example unasked.
    #[test]
    fn zz30_focus_retry_types_into_a_page_of_another_host() {
        let same = go("127.0.0.1");
        let moved = go("evil.example");
        assert_eq!(typed(&same), 1);
        let asked = moved
            .confirms
            .iter()
            .any(|c| c.summary.contains("evil.example"));
        assert!(
            typed(&moved) == 0 || asked,
            "typed into evil.example unasked"
        );
    }

    /// Pass 30 (M3): the model reads why nothing was typed; no second tap goes to the other
    /// page; the same step on the same host still types once.
    #[test]
    fn a_page_of_another_host_after_the_focus_tap_gets_nothing() {
        for from in [1, 2] {
            let moved = go_at("evil.example", from);
            assert_eq!(typed(&moved), 0, "{from}: {:?}", moved.acts);
            let tapped = moved
                .acts
                .iter()
                .filter(|a| matches!(a, RunnerCommand::Tap { .. }))
                .count();
            assert_eq!(
                tapped, from,
                "{from}: no tap into the other page: {:?}",
                moved.acts
            );
            assert!(
                moved.last_user().contains("the page changed"),
                "{}",
                moved.last_user()
            );
        }
    }

    /// Pass 30 (M3): a `type … submit` whose page moved after the focusing tap: neither the
    /// text nor Return is sent unasked.
    #[test]
    fn the_return_stage_is_not_sent_into_another_host() {
        let moved = go_with("evil.example", 1, true);
        let returned = moved.acts.iter().any(|a| {
            matches!(
                a,
                RunnerCommand::PressKey {
                    key: jarvis_proto::Key::Return,
                    ..
                }
            )
        });
        let asked = moved
            .confirms
            .iter()
            .any(|c| c.summary.contains("evil.example"));
        assert!(
            (!returned && typed(&moved) == 0) || asked,
            "{:?}",
            moved.acts
        );
    }
}

/// Pass 30 (M3): a `clear` whose page moved after the focusing tap sends no Deletes.
#[test]
fn a_clear_into_another_host_sends_no_deletes() {
    let page = nav::parse(include_str!(
        "fixtures/ios26_probes3/run147-safari-form-05-tree.txt"
    ));
    let mut filled = page.clone();
    nav::edit(&mut filled.root, &mut |n| {
        if n.role == Role::TextField && n.label.as_deref() == Some("Full name") {
            n.value = Some("Ada".into());
        }
    });
    let field = jarvis_observe::compact(&filled)
        .elements
        .iter()
        .find(|e| e.role == Role::TextField && e.label == "Full name")
        .unwrap()
        .index;
    let after = {
        let mut a = nav::at(filled.clone(), "evil.example", true);
        nav::edit(&mut a.root, &mut |n| {
            if n.role == Role::TextField && n.label.as_deref() == Some("Full name") {
                n.value = Some("Ada".into());
            }
        });
        a
    };
    let mut phone = Phone::new(filled, move |cmd, _s: &Screen| match cmd {
        RunnerCommand::Tap { .. } => Transition::Go(after.clone()),
        _ => Transition::Stay,
    });
    phone.auto_focus = false;
    let r = run(
        config(),
        "Clear the full name",
        MemoryStore::new(),
        phone,
        Script::model(&[
            &format!(r#"{{"action":"clear","element":{field}}}"#),
            r#"{"action":"fail","reason":"stop"}"#,
        ])
        .decisions(&[true; 8]),
    );
    let deletes = r
        .acts
        .iter()
        .filter(|a| {
            matches!(
                a,
                RunnerCommand::PressKey {
                    key: jarvis_proto::Key::Delete,
                    ..
                }
            )
        })
        .count();
    assert_eq!(deletes, 0, "{:?}", r.acts);
}

/// A sign-in form: a user-name field and a secure password field, neither focused.
fn sign_in(focus_user: bool, focus_password: bool) -> Screen {
    screen(
        "com.example.shop",
        "Shop",
        UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_children(vec![UiNode::new(
            Role::Window,
            IPHONE_SIZE.to_rect(),
        )
        .with_children(vec![
            UiNode::new(Role::TextField, rect(16.0, 200.0, 361.0, 44.0))
                .with_label("User name")
                .focused(focus_user)
                .hittable(true),
            UiNode::new(Role::SecureTextField, rect(16.0, 260.0, 361.0, 44.0))
                .with_label("Password")
                .focused(focus_password)
                .secure(true)
                .hittable(true),
        ])]),
        false,
    )
}

fn user_field(s: &Screen) -> u32 {
    jarvis_observe::compact(s)
        .elements
        .iter()
        .find(|e| e.label == "User name")
        .unwrap()
        .index
}

fn type_user(after_tap: Screen, auto_focus: bool) -> Run {
    let s = sign_in(false, false);
    let field = user_field(&s);
    let mut phone = Phone::new(s, move |cmd, _s: &Screen| match cmd {
        RunnerCommand::Tap { .. } => Transition::Go(after_tap.clone()),
        _ => Transition::Stay,
    });
    phone.auto_focus = auto_focus;
    run(
        config(),
        "Sign in as ada",
        MemoryStore::new(),
        phone,
        Script::model(&[
            &format!(r#"{{"action":"type","element":{field},"text":"ada","submit":false}}"#),
            r#"{"action":"fail","reason":"stop"}"#,
        ])
        .decisions(&[true; 8]),
    )
}

fn typed(r: &Run) -> usize {
    r.acts
        .iter()
        .filter(|a| matches!(a, RunnerCommand::TypeText { .. }))
        .count()
}

/// Pass 30 (L2): on a typed tree with an unfocused secure field beside the target, the text
/// goes to the user-name field once the tap focuses it; with the password field focused after
/// the tap (focus moved), nothing is typed; with no field focused even after the retap, nothing.
#[test]
fn a_typed_tree_with_a_secure_field_types_only_into_the_focused_target() {
    let ok = type_user(sign_in(true, false), false);
    assert_eq!(typed(&ok), 1, "{:?}", ok.acts);
    let moved = type_user(sign_in(false, true), false);
    assert_eq!(typed(&moved), 0, "{:?}", moved.acts);
    assert!(
        moved
            .last_user()
            .contains("the focus was not in that field"),
        "{}",
        moved.last_user()
    );
    let none = type_user(sign_in(false, false), false);
    assert_eq!(typed(&none), 0, "{:?}", none.acts);
    assert!(
        none.last_user().contains("did not take focus"),
        "{}",
        none.last_user()
    );
    // The harness focuses what it taps, a secure field too.
    let mut s = sign_in(false, false);
    focus_tapped(&mut s, jarvis_proto::Point::new(100.0, 280.0));
    let c = jarvis_observe::compact(&s);
    assert!(
        c.elements
            .iter()
            .any(|e| e.label == "Password" && e.focused),
        "{:#?}",
        c.elements
    );
}

/// Pass 30 (L2): a screen read from runner text carries no focus: its old rule holds — after
/// the focusing tap the text is sent (the runner's own check and the typed-text check stay
/// the guard), with the unverified-screen question asked first.
#[test]
fn runner_text_keeps_the_old_focus_rule() {
    use jarvis_proto::{Event, ModelOutput, Observation, RunnerResult, TaskSpec, TimedEvent};
    let s = sign_in(false, false);
    let field = user_field(&s);
    let mut session = jarvis_agent::Session::new(
        config(),
        TaskSpec::new("t", "Sign in as ada"),
        MemoryStore::new(),
    );
    let mut now = 1_000;
    let mut event = Event::Start;
    let mut model = vec![
        format!(r#"{{"action":"type","element":{field},"text":"ada","submit":false}}"#),
        r#"{"action":"fail","reason":"stop"}"#.to_owned(),
    ]
    .into_iter();
    let mut acts = Vec::new();
    let mut confirms = 0;
    for _ in 0..200 {
        now += 50;
        let e = session.step(TimedEvent::new(now, event));
        event = match e {
            Effect::Observe { .. } => Event::ObservedRunnerText(Observation::new(s.clone())),
            Effect::Infer(_) => match model.next() {
                Some(text) => Event::ModelOutput(ModelOutput { text }),
                None => Event::ModelFailed {
                    error: "done".into(),
                },
            },
            Effect::Confirm(_) => {
                confirms += 1;
                Event::UserDecision { approved: true }
            }
            Effect::Act(cmd) => {
                acts.push(cmd.without_expect());
                Event::ActResult(RunnerResult::ok())
            }
            Effect::Finished(_) => break,
            _ => Event::Cancel,
        };
    }
    assert!(confirms >= 1, "the unverified screen asks");
    assert!(
        acts.iter()
            .any(|a| matches!(a, RunnerCommand::TypeText { .. })),
        "{acts:?}"
    );
}
