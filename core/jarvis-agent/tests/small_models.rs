//! Behaviour that a small local model needs from the session, from the local-model
//! evaluation (docs/MODEL-EVAL.md): home-screen icon taps get the checks `open_app` gets,
//! step results say what changed, loops on changing screens are seen and stopped, and
//! `done` right after a decline or a failed step is not a success.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::cell::Cell;
use std::rc::Rc;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{
    Action, OutcomeStatus, RiskCategory, Role, RunnerCommand, Screen, SessionConfig, SkillContext,
    UiNode,
};

fn root(label: &str, children: Vec<UiNode>) -> UiNode {
    UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(label)
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(children)
        ])
}

fn tap(label: &str, screen: &Screen) -> String {
    format!(
        "{{\"thought\":\"go\",\"action\":\"tap\",\"element\":{}}}",
        index_of(screen, label)
    )
}

const DONE: &str = "{\"thought\":\"finished\",\"action\":\"done\",\"summary\":\"All done\"}";

// ---- 1. home-screen icons get the checks open_app gets ------------------------------------

/// The home screen of the evaluation: Passwords sits next to ordinary apps, plus a folder,
/// the search pill and an icon Jarvis does not know.
fn home() -> Screen {
    screen(
        "com.apple.springboard",
        "Home",
        root(
            "Home",
            vec![
                node(Role::Button, "Settings", rect(20.0, 100.0, 64.0, 64.0)),
                node(Role::Button, "Passwords", rect(110.0, 100.0, 64.0, 64.0)),
                node(Role::Button, "Shop", rect(200.0, 100.0, 64.0, 64.0)),
                node(Role::Button, "Weather", rect(290.0, 100.0, 64.0, 64.0)),
                node(
                    Role::Button,
                    "Utilities folder",
                    rect(20.0, 200.0, 64.0, 64.0),
                ),
                node(Role::Button, "Search", rect(150.0, 700.0, 90.0, 30.0)),
            ],
        ),
        false,
    )
}

/// A home screen whose icons launch their app (Weather launches an unknown app).
fn home_phone() -> Phone {
    Phone::new(home(), |cmd, s: &Screen| {
        for (label, bundle) in [
            ("Passwords", "com.apple.Passwords"),
            ("Settings", "com.apple.Preferences"),
            ("Shop", "com.example.shop"),
            ("Weather", "com.example.weather"),
        ] {
            if taps(cmd, s, "Inside") {
                return Transition::Go(screen(
                    &s.app_bundle_id,
                    "App",
                    root("App", vec![text("Opened", rect(16.0, 300.0, 300.0, 44.0))]),
                    false,
                ));
            }
            if taps(cmd, s, label) {
                return Transition::Go(screen(
                    bundle,
                    label,
                    root(
                        label,
                        vec![node(Role::Button, "Inside", rect(16.0, 300.0, 300.0, 44.0))],
                    ),
                    false,
                ));
            }
        }
        Transition::Stay
    })
}

fn touches_icon(acts: &[RunnerCommand], label: &str) -> bool {
    acts.iter().any(|c| taps(c, &home(), label))
}

/// Model evaluation finding 1: `tap @Passwords` on the home screen launched the denied app;
/// the deny check only ran on the next observation, with the app already in front.
#[test]
fn tapping_a_denied_icon_on_the_home_screen_never_sends_the_tap() {
    let t = tap("Passwords", &home());
    let run = run(
        config(),
        "Open Passwords",
        MemoryStore::new(),
        home_phone(),
        Script::model(&[&t, &t]),
    );
    assert!(run.acts.is_empty(), "{:?}", run.acts);
    assert!(!touches_icon(&run.acts, "Passwords"));
    assert_eq!(
        run.outcome.status,
        OutcomeStatus::Blocked,
        "{}",
        run.outcome.summary
    );
    assert!(
        run.outcome.summary.contains("deny list"),
        "{}",
        run.outcome.summary
    );
    assert!(run.requests[1]
        .user
        .contains("not allowed: Passwords is on the deny list"));
    assert_eq!(run.screen.app_bundle_id, "com.apple.springboard");
}

/// A long press opens the icon's quick actions, so it is refused the same way; so is a
/// `tap_point` landing on the icon.
#[test]
fn long_press_and_tap_point_on_a_denied_icon_are_refused_too() {
    let i = index_of(&home(), "Passwords");
    let lp = format!("{{\"action\":\"long_press\",\"element\":{i}}}");
    let run1 = run(
        config(),
        "Open Passwords",
        MemoryStore::new(),
        home_phone(),
        Script::model(&[&lp, &lp]),
    );
    assert!(run1.acts.is_empty(), "{:?}", run1.acts);
    assert_eq!(run1.outcome.status, OutcomeStatus::Blocked);

    let mut c = vision_config();
    c.policy.first_use_approval = false;
    // Passwords' centre: (142, 132) on a 393 x 852 screen.
    let point = "{\"action\":\"tap_point\",\"x\":0.361,\"y\":0.155}";
    let run2 = run(
        c,
        "Open Passwords",
        MemoryStore::new(),
        Phone::still(home()),
        Script::model(&[
            "{\"action\":\"look\"}",
            point,
            "{\"action\":\"look\"}",
            point,
        ]),
    );
    assert!(
        !run2
            .acts
            .iter()
            .any(|c| matches!(c, RunnerCommand::Tap { .. })),
        "{:?}",
        run2.acts
    );
    assert_eq!(
        run2.outcome.status,
        OutcomeStatus::Blocked,
        "{}",
        run2.outcome.summary
    );
}

/// A known, allowed app's icon is just a launch, as with `open_app`: no prompt at the tap;
/// first-use approval is asked at the first action inside the app.
#[test]
fn a_known_icon_launches_like_open_app_and_first_use_is_asked_inside() {
    let inside = "{\"action\":\"tap\",\"element\":1}";
    let run = run(
        first_use_config(),
        "Open Shop",
        MemoryStore::new(),
        home_phone(),
        Script::model(&[&tap("Shop", &home()), inside, DONE]).decisions(&[true]),
    );
    assert_eq!(
        run.outcome.status,
        OutcomeStatus::Success,
        "{}",
        run.outcome.summary
    );
    assert_eq!(run.confirms.len(), 1);
    assert_eq!(run.confirms[0].category, RiskCategory::FirstUse);
    assert_eq!(run.confirms[0].app, "com.example.shop");
    assert!(touches_icon(&run.acts, "Shop"));
}

/// An icon Jarvis cannot name may launch anything: the tap asks first (naming the label),
/// never silently, and a decline means it is never sent.
#[test]
fn an_unknown_icon_asks_first_with_its_label() {
    let t = tap("Weather", &home());
    let declined = run(
        first_use_config(),
        "Open Weather",
        MemoryStore::new(),
        home_phone(),
        Script::model(&[&t, "{\"action\":\"fail\",\"reason\":\"declined\"}"]).decisions(&[false]),
    );
    assert_eq!(declined.confirms.len(), 1);
    let c = &declined.confirms[0];
    assert_eq!(c.category, RiskCategory::FirstUse);
    assert!(c.summary.contains("'Weather'"), "{}", c.summary);
    assert!(c.summary.contains("home screen"), "{}", c.summary);
    assert!(declined.acts.is_empty());

    let approved = run(
        first_use_config(),
        "Open Weather",
        MemoryStore::new(),
        home_phone(),
        Script::model(&[&t, "{\"action\":\"tap\",\"element\":1}", DONE]).decisions(&[true, true]),
    );
    assert!(touches_icon(&approved.acts, "Weather"));
    // The launched app still gets its own first-use approval.
    assert_eq!(approved.confirms.len(), 2, "{:?}", approved.confirms);
    assert_eq!(approved.confirms[1].app, "com.example.weather");
    assert_eq!(approved.outcome.status, OutcomeStatus::Success);

    // Folders and the search pill open SpringBoard's own UI: no prompt.
    let folder = run(
        first_use_config(),
        "Open the folder",
        MemoryStore::new(),
        Phone::still(home()),
        Script::model(&[
            &tap("Utilities folder", &home()),
            &tap("Search", &home()),
            DONE,
        ]),
    );
    assert!(folder.confirms.is_empty(), "{:?}", folder.confirms);
    // Return in the home screen's search opens the top hit: asked like an unknown icon.
    let search = Screen {
        root: root(
            "Home",
            vec![
                UiNode::new(Role::SearchField, rect(16.0, 100.0, 361.0, 40.0))
                    .with_label("Search")
                    .hittable(true)
                    .focused(true),
            ],
        ),
        ..home()
    };
    let submit = "{\"action\":\"type\",\"element\":1,\"text\":\"pass\",\"submit\":true}";
    let searched = run(
        first_use_config(),
        "Search for pass",
        MemoryStore::new(),
        Phone::still(search),
        Script::model(&[submit, "{\"action\":\"fail\"}"]).decisions(&[false]),
    );
    assert_eq!(searched.confirms.len(), 1);
    assert!(searched.confirms[0].summary.contains("top search result"));
    assert!(searched.acts.is_empty(), "{:?}", searched.acts);
}

/// An app the owner declined in this session cannot be reached through its icon, and a
/// confined skill still may not act on the home screen at all.
#[test]
fn declined_apps_and_skill_confinement_cover_icon_taps() {
    // Decline Shop's first use inside it, go home, tap its icon.
    let shop_inside = screen(
        "com.example.shop",
        "Shop",
        root(
            "Shop",
            vec![node(Role::Button, "Inside", rect(16.0, 300.0, 300.0, 44.0))],
        ),
        false,
    );
    let phone = Phone::new(shop_inside, |cmd, _s: &Screen| match cmd {
        RunnerCommand::PressKey { .. } => Transition::Go(home()),
        _ => Transition::Stay,
    });
    let run1 = run(
        first_use_config(),
        "Buy something",
        MemoryStore::new(),
        phone,
        Script::model(&[
            "{\"action\":\"tap\",\"element\":1}",
            "{\"action\":\"press_home\"}",
            &tap("Shop", &home()),
        ])
        .decisions(&[false]),
    );
    assert_eq!(
        run1.outcome.status,
        OutcomeStatus::Blocked,
        "{}",
        run1.outcome.summary
    );
    assert!(run1
        .outcome
        .summary
        .contains("did not let Jarvis act in Shop"));
    assert!(!touches_icon(&run1.acts, "Shop"));

    let mut c: SessionConfig = config();
    c.skill = Some(SkillContext {
        id: "s".into(),
        name: "Settings helper".into(),
        instructions: String::new(),
        apps: vec!["com.apple.Preferences".into()],
        confine_to_apps: true,
        ..SkillContext::default()
    });
    let t = tap("Settings", &home());
    let run2 = run(
        c,
        "Open Settings",
        MemoryStore::new(),
        home_phone(),
        Script::model(&[&t, &t]),
    );
    assert!(run2.acts.is_empty());
    assert_eq!(run2.outcome.status, OutcomeStatus::Blocked);
}

// ---- 3 and 4. what changed, and loops on changing screens --------------------------------

/// A product page whose "Add to Cart" adds one item each time (shown in "Cart (n)").
fn product(cart: u32) -> Screen {
    screen(
        "com.example.shop",
        "Shop",
        root(
            "Shop",
            vec![
                node(
                    Role::Button,
                    &format!("Cart ({cart})"),
                    rect(260.0, 60.0, 120.0, 40.0),
                ),
                text("Wireless Headphones", rect(16.0, 120.0, 361.0, 30.0)),
                text("$129.00", rect(16.0, 160.0, 200.0, 24.0)),
                node(Role::Button, "Add to Cart", rect(16.0, 600.0, 361.0, 50.0)),
            ],
        ),
        false,
    )
}

fn cart_phone(count: Rc<Cell<u32>>) -> Phone {
    Phone::new(product(0), move |cmd, s: &Screen| {
        if taps(cmd, s, "Add to Cart") {
            count.set(count.get() + 1);
            return Transition::Go(product(count.get()));
        }
        Transition::Stay
    })
}

/// Model evaluation finding 3: the history said only "ok", so a model could not see that
/// "Add to Cart" had worked.
#[test]
fn step_results_say_what_changed() {
    let count = Rc::new(Cell::new(0));
    let add = tap("Add to Cart", &product(0));
    let run = run(
        config(),
        "Add the headphones to my cart",
        MemoryStore::new(),
        cart_phone(count.clone()),
        Script::model(&[&add, DONE]),
    );
    assert_eq!(run.outcome.status, OutcomeStatus::Success);
    assert!(
        run.last_user().contains(
            "1. tap [4] \"Add to Cart\" -> ok; changed: Button \"Cart (0)\" -> \"Cart (1)\"\n"
        ),
        "{}",
        run.last_user()
    );
    assert_eq!(count.get(), 1);
}

/// Model evaluation finding 1 ("Why tasks failed"): "Add to Cart" tapped 8 to 10 times. Now
/// the third time draws a warning with what changed since the first, the fifth a last
/// warning, and choosing it a sixth time ends the session as stuck.
#[test]
fn a_loop_on_a_changing_screen_is_warned_twice_then_stopped() {
    let count = Rc::new(Cell::new(0));
    let add = tap("Add to Cart", &product(0));
    let script: Vec<&str> = std::iter::repeat_n(add.as_str(), 12).collect();
    let run = run(
        config(),
        "Add the headphones to my cart",
        MemoryStore::new(),
        cart_phone(count.clone()),
        Script::model(&script),
    );
    assert_eq!(
        run.outcome.status,
        OutcomeStatus::Failed,
        "{}",
        run.outcome.summary
    );
    assert!(
        run.outcome.summary.starts_with("Stuck:"),
        "{}",
        run.outcome.summary
    );
    assert_eq!(count.get(), 5, "five taps carried out, the sixth refused");
    assert_eq!(run.infers(), 6);
    let warn1 = &run.requests[3].user;
    assert!(
        warn1.contains(
            "3 times in a row; since the first time: changed: Button \"Cart (0)\" -> \"Cart (3)\""
        ),
        "{warn1}"
    );
    assert!(warn1.contains("(done 3 times in a row)"), "{warn1}");
    assert!(
        !run.requests[4].user.contains("times in a row; since"),
        "a note is shown once"
    );
    let warn2 = &run.requests[5].user;
    assert!(warn2.contains("5 times in a row"), "{warn2}");
    assert!(
        warn2.contains("once more, the task stops as stuck"),
        "{warn2}"
    );
}

/// The eval's other loop: the same message typed again and again into the same field.
/// Typed-text verification accepts each (the text is appended after the prior text), so
/// it is the repeat rule that sees it.
#[test]
fn typing_the_same_text_into_the_same_field_again_is_a_loop() {
    let compose = |value: &str| {
        let mut f = UiNode::new(Role::TextView, rect(16.0, 700.0, 300.0, 40.0))
            .with_label("Message")
            .hittable(true)
            .focused(true);
        if !value.is_empty() {
            f = f.with_value(value);
        }
        screen(
            "com.apple.MobileSMS",
            "Messages",
            root(
                "Messages",
                vec![
                    f,
                    node(Role::Button, "Send", rect(320.0, 700.0, 60.0, 40.0)),
                ],
            ),
            true,
        )
    };
    let phone = Phone::new(compose(""), move |cmd, s: &Screen| match cmd {
        RunnerCommand::TypeText { text } => {
            let prior = jarvis_observe::compact(s).elements[0]
                .value
                .clone()
                .unwrap_or_default();
            Transition::Go(compose(&format!("{prior}{text}")))
        }
        _ => Transition::Stay,
    });
    let ty = "{\"action\":\"type\",\"element\":1,\"text\":\"running late\"}";
    let run = run(
        config(),
        "Text Mom that I'm running late",
        MemoryStore::new(),
        phone,
        Script::model(&[ty, ty, ty, ty]),
    );
    let third = &run.requests[3].user;
    assert!(
        third.contains("field now \"running laterunning laterunning"),
        "{third}"
    );
    assert!(third.contains("3 times in a row"), "{third}");
}

/// Legitimate repetition is left alone: a stepper pressed to reach a value, a control whose
/// own value changes, typing into several fields, scrolling a long list, and a "Continue"
/// that leads through different pages.
#[test]
fn legitimate_repetition_is_not_a_loop() {
    // A stepper: "+" raises "Quantity: n".
    let stepper = |n: u32| {
        screen(
            "com.example.shop",
            "Shop",
            root(
                "Shop",
                vec![
                    text(&format!("Quantity: {n}"), rect(16.0, 300.0, 200.0, 30.0)),
                    node(Role::Button, "Increment", rect(300.0, 300.0, 44.0, 44.0)),
                ],
            ),
            false,
        )
    };
    let n = Rc::new(Cell::new(0));
    let n2 = n.clone();
    let phone = Phone::new(stepper(0), move |cmd, s: &Screen| {
        if taps(cmd, s, "Increment") {
            n2.set(n2.get() + 1);
            return Transition::Go(stepper(n2.get()));
        }
        Transition::Stay
    });
    let plus = tap("Increment", &stepper(0));
    let mut script: Vec<&str> = std::iter::repeat_n(plus.as_str(), 8).collect();
    script.push(DONE);
    let run1 = run(
        config(),
        "Set quantity to 8",
        MemoryStore::new(),
        phone,
        Script::model(&script),
    );
    assert_eq!(
        run1.outcome.status,
        OutcomeStatus::Success,
        "{}",
        run1.outcome.summary
    );
    assert_eq!(n.get(), 8);
    assert!(!run1.all_prompt_text().contains("times in a row"));

    // Wizard pages: the same "Continue" fingerprint, a different page each time.
    let page = |i: u32| {
        screen(
            "com.example.setup",
            "Setup",
            root(
                "Setup",
                vec![
                    text(&format!("Step {i}"), rect(16.0, 120.0, 300.0, 30.0)),
                    text(
                        &format!("Explanation for step {i}"),
                        rect(16.0, 200.0, 300.0, 30.0),
                    ),
                    text(&format!("Detail {i}"), rect(16.0, 260.0, 300.0, 30.0)),
                    node(Role::Button, "Continue", rect(16.0, 700.0, 361.0, 50.0)),
                ],
            ),
            false,
        )
    };
    let at = Rc::new(Cell::new(0));
    let at2 = at.clone();
    let wizard = Phone::new(page(0), move |cmd, s: &Screen| {
        if taps(cmd, s, "Continue") {
            at2.set(at2.get() + 1);
            return Transition::Go(page(at2.get()));
        }
        Transition::Stay
    });
    let cont = tap("Continue", &page(0));
    let mut script: Vec<&str> = std::iter::repeat_n(cont.as_str(), 7).collect();
    script.push(DONE);
    let run2 = run(
        config(),
        "Finish setup",
        MemoryStore::new(),
        wizard,
        Script::model(&script),
    );
    assert_eq!(
        run2.outcome.status,
        OutcomeStatus::Success,
        "{}",
        run2.outcome.summary
    );
    assert_eq!(at.get(), 7);
    assert!(!run2.all_prompt_text().contains("times in a row"));
}

/// Scrolling a list many times is what lists are for; typing into several fields is
/// several actions.
#[test]
fn scrolling_and_filling_a_form_are_not_loops() {
    let offset = Rc::new(Cell::new(0usize));
    let list = |from: usize| {
        let rows: Vec<UiNode> = (from..from + 8)
            .map(|i| {
                node(
                    Role::Cell,
                    &format!("Note {i}"),
                    rect(16.0, 150.0 + (i - from) as f64 * 60.0, 361.0, 50.0),
                )
            })
            .collect();
        screen("com.example.notes", "Notes", root("Notes", rows), false)
    };
    let o2 = offset.clone();
    let phone = Phone::new(list(0), move |cmd, _s: &Screen| match cmd {
        RunnerCommand::Swipe { .. } => {
            o2.set(o2.get() + 4);
            Transition::Go(list(o2.get()))
        }
        _ => Transition::Stay,
    });
    let down = "{\"action\":\"scroll\",\"direction\":\"down\"}";
    let mut script: Vec<&str> = std::iter::repeat_n(down, 6).collect();
    script.push(DONE);
    let run1 = run(
        config(),
        "Find note 30",
        MemoryStore::new(),
        phone,
        Script::model(&script),
    );
    assert_eq!(
        run1.outcome.status,
        OutcomeStatus::Success,
        "{}",
        run1.outcome.summary
    );
    assert!(!run1.all_prompt_text().contains("times in a row"));

    let form = screen(
        "com.example.acme",
        "Acme",
        root(
            "Acme",
            vec![
                UiNode::new(Role::TextField, rect(16.0, 200.0, 361.0, 40.0))
                    .with_label("First")
                    .hittable(true),
                UiNode::new(Role::TextField, rect(16.0, 260.0, 361.0, 40.0))
                    .with_label("Middle")
                    .hittable(true),
                UiNode::new(Role::TextField, rect(16.0, 320.0, 361.0, 40.0))
                    .with_label("Last")
                    .hittable(true),
            ],
        ),
        false,
    );
    let script: Vec<String> = (1..=3)
        .map(|i| format!("{{\"action\":\"type\",\"element\":{i},\"text\":\"x\"}}"))
        .chain([DONE.to_owned()])
        .collect();
    let refs: Vec<&str> = script.iter().map(String::as_str).collect();
    let run2 = run(
        config(),
        "Fill the form",
        MemoryStore::new(),
        Phone::still(form),
        Script::model(&refs),
    );
    assert!(!run2.all_prompt_text().contains("times in a row"));
}

// ---- 5. done after a decline or a failed step ----------------------------------------------

fn buy_phone() -> Phone {
    Phone::new(product_screen(), |cmd, s: &Screen| {
        if taps(cmd, s, "Add to wishlist") {
            return Transition::Go(bought_screen());
        }
        Transition::Stay
    })
}

/// Model evaluation finding 2: "done" straight after the owner declined was a success.
#[test]
fn done_right_after_a_declined_confirmation_is_blocked() {
    let buy = tap("Buy now", &product_screen());
    let run1 = run(
        config(),
        "Buy the headphones",
        MemoryStore::new(),
        buy_phone(),
        Script::model(&[&buy, DONE]).decisions(&[false]),
    );
    assert_eq!(run1.confirms[0].category, RiskCategory::Purchase);
    assert_eq!(
        run1.outcome.status,
        OutcomeStatus::Blocked,
        "{}",
        run1.outcome.summary
    );
    assert!(
        run1.outcome.summary.contains("the owner declined"),
        "{}",
        run1.outcome.summary
    );
    assert!(
        run1.outcome.summary.contains("All done"),
        "the agent's words are kept"
    );

    // Something that worked after the decline: done is taken at its word again.
    let wish = tap("Add to wishlist", &product_screen());
    let run2 = run(
        config(),
        "Buy the headphones or save them",
        MemoryStore::new(),
        buy_phone(),
        Script::model(&[&buy, &wish, DONE]).decisions(&[false]),
    );
    assert_eq!(
        run2.outcome.status,
        OutcomeStatus::Success,
        "{}",
        run2.outcome.summary
    );
}

/// `done` right after a failed step, with nothing working since, is not a success; the end
/// of a list is not a failure, and a later success clears it.
#[test]
fn done_right_after_a_failed_step_is_failed() {
    let nothing = tap("Wireless Headphones", &product_screen());
    let run1 = run(
        config(),
        "Open the headphones",
        MemoryStore::new(),
        Phone::still(product_screen()),
        Script::model(&[&nothing, DONE]),
    );
    assert_eq!(
        run1.outcome.status,
        OutcomeStatus::Failed,
        "{}",
        run1.outcome.summary
    );
    assert!(
        run1.outcome.summary.contains("the last step failed"),
        "{}",
        run1.outcome.summary
    );

    let down = "{\"action\":\"scroll\",\"direction\":\"down\"}";
    let run2 = run(
        config(),
        "Scroll to the end",
        MemoryStore::new(),
        Phone::still(product_screen()),
        Script::model(&[down, DONE]),
    );
    assert_eq!(
        run2.outcome.status,
        OutcomeStatus::Success,
        "{}",
        run2.outcome.summary
    );

    let wish = tap("Add to wishlist", &product_screen());
    let run3 = run(
        config(),
        "Save the headphones",
        MemoryStore::new(),
        buy_phone(),
        Script::model(&[&nothing, &wish, DONE]),
    );
    assert_eq!(
        run3.outcome.status,
        OutcomeStatus::Success,
        "{}",
        run3.outcome.summary
    );
    // Not the session's business: whether a summary is true when the last step worked.
    let _ = Action::Look;
}
