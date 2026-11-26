#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Twenty-first review fixes: R21-A (a scroll is of the scroll kind only when it names no
//! element or a scroll container and its drag works no control; everything else is a full
//! input), F2 (a question is partial by its destinations only when an unnamed destination
//! still receives an unallowed source; every action sent because of an approval goes through
//! the look after an approval) and F3 (a swipe's approval keeps nothing: the cost, pinned).

mod common;

use common::*;
use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_observe::compact;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{
    Action, AppInfo, ConfirmRequest, ConfirmVerb, Direction, Effect, Event, ImageRef, Key,
    ModelOutput, Observation, Outcome, OutcomeStatus, Role, RunnerCommand, RunnerResult, Screen,
    SessionConfig, TaskSpec, TimedEvent, UiNode,
};
use jarvis_safety::InputKind;

const FLOW: &str = "what it read there could be passed on here";
const DONE: &str = r#"{"action":"done","summary":"ok"}"#;
const SCROLL: &str = r#"{"action":"scroll","direction":"down"}"#;
const ACME: &str = "com.example.acme";
const SHOP: &str = "com.example.shop";
const SAFARI: &str = "com.apple.mobilesafari";
const LONG_APP: &str = "com.example.longname";
/// 40 characters, no ", " / " and " (shown as is).
const LONG_NAME: &str = "Shop $1000.00 $2000.00 $3000.00 $4000.00";

fn cfg() -> SessionConfig {
    let mut c = config();
    c.known_apps.push(AppInfo::new(SAFARI, "Safari"));
    c.known_apps.push(AppInfo::new(LONG_APP, LONG_NAME));
    c
}

fn scr(app: &str, name: &str, children: Vec<UiNode>) -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(name)
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(children)
        ]);
    screen(app, name, root, false)
}

fn tap(i: u32) -> String {
    format!(r#"{{"action":"tap","element":{i}}}"#)
}

fn scroll_on(i: u32, dir: &str) -> String {
    format!(r#"{{"action":"scroll","direction":"{dir}","element":{i}}}"#)
}

fn flows(r: &Run) -> Vec<&ConfirmRequest> {
    r.confirms
        .iter()
        .filter(|c| c.summary.contains(FLOW))
        .collect()
}

fn drags(acts: &[RunnerCommand]) -> usize {
    acts.iter()
        .filter(|a| matches!(a, RunnerCommand::Swipe { .. }))
        .count()
}

// ---- R21-A: the kind by the screen -----------------------------------------------------

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

/// A shop with a control of each kind, all away from the screen scroll's path (x = 196.5)
/// and its start point, a scroll container (a labelled, childless `ScrollView`: listed as
/// an element with the tree's role) and a plain button. `level`: a vertical slider across
/// the screen scroll's path.
fn shop(n: usize, level: bool) -> Screen {
    let mut kids = vec![
        text("Book a pickup", rect(16.0, 100.0, 170.0, 30.0)),
        UiNode::new(
            Role::Other("ScrollView".into()),
            rect(220.0, 150.0, 157.0, 120.0),
        )
        .with_label("Results"),
        node(Role::Picker, "Hour", rect(16.0, 300.0, 90.0, 160.0)).with_value("1"),
        node(Role::Slider, "Volume", rect(16.0, 480.0, 150.0, 30.0)).with_value("50%"),
        node(Role::Switch, "Gift wrap", rect(16.0, 530.0, 150.0, 30.0)).with_value("0"),
        node(Role::Button, "Details", rect(220.0, 300.0, 150.0, 44.0)),
    ];
    for k in 0..3 {
        kids.push(
            UiNode::new(
                Role::Cell,
                rect(220.0, 360.0 + 60.0 * k as f64, 157.0, 50.0),
            )
            .with_label(format!("Order {k}, pickup at 9"))
            .hittable(true),
        );
    }
    if level {
        kids.push(node(Role::Slider, "Level", rect(180.0, 380.0, 33.0, 200.0)).with_value("5"));
    }
    kids.push(text(&format!("Viewed {n}"), rect(16.0, 800.0, 170.0, 20.0)));
    scr(SHOP, "Shop", kids)
}

/// Acme, then Shop; every input in Shop changes it; after the first drag in Shop a vertical
/// slider appears on the screen scroll's path when `level_after_drag`.
fn acme_then_shop(level_after_drag: bool) -> Phone {
    let mut n = 0usize;
    let mut dragged = false;
    Phone::new(acme(), move |cmd, s| {
        // A wait (the settle before a second look after an approval) changes nothing.
        if matches!(cmd, RunnerCommand::Wait { .. }) {
            return Transition::Stay;
        }
        n += 1;
        if taps(cmd, s, "Open Shop") {
            return Transition::Go(shop(n, false));
        }
        if s.app_bundle_id == SHOP {
            dragged |= matches!(cmd, RunnerCommand::Swipe { .. });
            return Transition::Go(shop(n, level_after_drag && dragged));
        }
        Transition::Stay
    })
}

fn verbs(r: &Run) -> Vec<ConfirmVerb> {
    flows(r).iter().map(|c| c.verb).collect()
}

/// Runs Acme → Shop with `grant` (the first input into Shop, approved) then `then`.
fn shop_run(grant: &str, then: &[&str], level_after_drag: bool) -> Run {
    let open = index_of(&acme(), "Open Shop");
    let mut model = vec![tap(open), grant.to_owned()];
    model.extend(then.iter().map(|s| (*s).to_owned()));
    model.push(DONE.to_owned());
    let model: Vec<&str> = model.iter().map(String::as_str).collect();
    run(
        cfg(),
        "Open Shop and book a pickup",
        MemoryStore::new(),
        acme_then_shop(level_after_drag),
        Script::model(&model).decisions(&[true; 20]),
    )
}

/// Run 149: the order rows are 157 pt wide, under the 80 / 0.45 ≈ 178 pt a row drag needs:
/// a sideways drag across one is never sent (`RowTooNarrow`), under any grant.
fn narrow_row(c: &str) -> bool {
    c == scroll_on(index_of(&shop(0, false), "Order 1, pickup at 9"), "left")
}

/// Run 149: under a scroll grant or a full grant, the sideways scroll on a 157-pt order row
/// is refused before any question and nothing is dragged (it was asked, then sent).
#[test]
fn a_sideways_scroll_on_a_narrow_row_is_never_sent() {
    let row = scroll_on(index_of(&shop(0, false), "Order 1, pickup at 9"), "left");
    let details = tap(index_of(&shop(0, false), "Details"));
    for grant in [SCROLL.to_owned(), details] {
        let r = shop_run(&grant, &[&row], false);
        let own = usize::from(grant == SCROLL);
        assert_eq!(
            drags(&r.acts),
            own,
            "{grant}: only the grant's own drag: {:?}",
            r.acts
        );
        assert!(
            r.all_prompt_text().contains("too narrow to swipe safely"),
            "{grant}"
        );
    }
}

fn controls() -> Vec<String> {
    let s = shop(0, false);
    vec![
        scroll_on(index_of(&s, "Hour"), "down"),
        scroll_on(index_of(&s, "Volume"), "left"),
        scroll_on(index_of(&s, "Gift wrap"), "right"),
        scroll_on(index_of(&s, "Order 1, pickup at 9"), "left"),
    ]
}

/// The kinds, decided by the one function on the screen: a screen scroll and a scroll on the
/// scroll container are scrolls; a scroll on a picker, slider, switch, sideways row, button
/// or text is full; a screen scroll whose path crosses a vertical slider is full.
#[test]
fn the_kind_of_a_scroll_is_decided_on_the_screen() {
    let s = compact(&shop(0, false));
    let at = |label: &str| s.elements.iter().find(|e| e.label == label).unwrap().index;
    let scroll = |element: Option<u32>, direction| Action::Scroll { direction, element };
    assert_eq!(
        InputKind::of(&scroll(None, Direction::Down), &s),
        InputKind::Scroll
    );
    assert_eq!(
        s.get(at("Results")).unwrap().role,
        Role::Other("ScrollView".into())
    );
    assert_eq!(
        InputKind::of(&scroll(Some(at("Results")), Direction::Down), &s),
        InputKind::Scroll
    );
    for (label, d) in [
        ("Hour", Direction::Down),
        ("Volume", Direction::Left),
        ("Gift wrap", Direction::Right),
        ("Order 1, pickup at 9", Direction::Left),
        ("Details", Direction::Down),
        ("Book a pickup", Direction::Down),
    ] {
        assert_eq!(
            InputKind::of(&scroll(Some(at(label)), d), &s),
            InputKind::Full,
            "{label}"
        );
    }
    let lv = compact(&shop(0, true));
    assert_eq!(
        InputKind::of(&scroll(None, Direction::Down), &lv),
        InputKind::Full
    );
    // Every other action is full.
    assert_eq!(
        InputKind::of(
            &Action::Swipe {
                direction: Direction::Down,
                element: None
            },
            &s
        ),
        InputKind::Full
    );
    assert_eq!(
        InputKind::of(
            &Action::Tap {
                element: at("Details")
            },
            &s
        ),
        InputKind::Full
    );
}

/// Under a scroll grant, a scroll on a picker, a slider, a switch or a row dragged sideways
/// asks (a full input); its approval keeps a full grant, so a second one is silent.
#[test]
fn a_scroll_on_a_control_asks_under_a_scroll_grant() {
    for c in controls() {
        if narrow_row(&c) {
            continue;
        }
        let r = shop_run(SCROLL, &[&c, &c], false);
        assert_eq!(r.outcome.status, OutcomeStatus::Success, "{c}");
        assert_eq!(
            verbs(&r),
            vec![ConfirmVerb::Scroll, ConfirmVerb::Scroll],
            "{c}: {:#?}",
            flows(&r)
        );
        assert_eq!(drags(&r.acts), 3, "{c}");
    }
}

/// Under a full grant (a tap approved), the same scrolls ask nothing more.
#[test]
fn a_scroll_on_a_control_is_covered_by_a_full_grant() {
    let details = tap(index_of(&shop(0, false), "Details"));
    for c in controls() {
        if narrow_row(&c) {
            continue;
        }
        let r = shop_run(&details, &[&c, &c], false);
        assert_eq!(r.outcome.status, OutcomeStatus::Success, "{c}");
        assert_eq!(verbs(&r), vec![ConfirmVerb::Tap], "{c}: {:#?}", flows(&r));
        assert_eq!(drags(&r.acts), 2, "{c}");
    }
}

/// A scroll whose element is no scroll container (a text) asks under a scroll grant.
#[test]
fn a_scroll_on_a_non_container_element_asks_under_a_scroll_grant() {
    let t = scroll_on(index_of(&shop(0, false), "Book a pickup"), "down");
    let r = shop_run(SCROLL, &[&t], false);
    assert_eq!(
        verbs(&r),
        vec![ConfirmVerb::Scroll, ConfirmVerb::Scroll],
        "{:#?}",
        flows(&r)
    );
}

/// A screen scroll and a scroll on a scroll container stay silent under a scroll grant.
#[test]
fn screen_and_container_scrolls_stay_silent_under_a_scroll_grant() {
    let results = scroll_on(index_of(&shop(0, false), "Results"), "down");
    let r = shop_run(SCROLL, &[SCROLL, &results, &results], false);
    assert_eq!(r.outcome.status, OutcomeStatus::Success);
    assert_eq!(verbs(&r), vec![ConfirmVerb::Scroll], "{:#?}", flows(&r));
    assert_eq!(drags(&r.acts), 4);
}

/// A screen scroll whose path crosses a vertical slider (it appeared after the first scroll)
/// asks under the scroll grant: it works the slider.
#[test]
fn a_screen_scroll_across_a_slider_asks_under_a_scroll_grant() {
    let r = shop_run(SCROLL, &[SCROLL], true);
    assert_eq!(r.outcome.status, OutcomeStatus::Success);
    assert_eq!(
        verbs(&r),
        vec![ConfirmVerb::Scroll, ConfirmVerb::Scroll],
        "{:#?}",
        flows(&r)
    );
}

// ---- F2: partial questions and the look after an approval -------------------------------

/// `common::run` with a hook that may change the screen while the k-th question is up.
fn drive(
    config: SessionConfig,
    task: &str,
    mut phone: Phone,
    mut script: Script,
    mut change: impl FnMut(usize, &ConfirmRequest, &mut Screen),
) -> (Vec<Effect>, Vec<ConfirmRequest>, Outcome) {
    let mut session = Session::new(config, TaskSpec::new("t", task), MemoryStore::new());
    let mut now = 1_000u64;
    let mut event = Event::Start;
    let mut effects = Vec::new();
    let mut confirms = Vec::new();
    for _ in 0..MAX_ITERATIONS {
        let effect = session.step(TimedEvent::new(now, event));
        now += TICK_MS;
        effects.push(effect.clone());
        event = match effect {
            Effect::Observe { screenshot } => Event::Observed(Observation {
                screen: phone.screen.clone(),
                screenshot: screenshot.then(|| ImageRef {
                    id: "s".into(),
                    width: 1179,
                    height: 2556,
                }),
                device_locked: false,
            }),
            Effect::Infer(_) => match script.model.pop_front() {
                Some(text) => Event::ModelOutput(ModelOutput { text }),
                None => Event::ModelFailed {
                    error: "script exhausted".into(),
                },
            },
            Effect::Act(cmd) => {
                let result = match (phone.transition)(&cmd, &phone.screen) {
                    Transition::Stay => Event::ActResult(RunnerResult::ok()),
                    Transition::Go(s) => {
                        phone.screen = s;
                        Event::ActResult(RunnerResult::ok())
                    }
                    Transition::Fail(e) => Event::ActResult(RunnerResult::err(e)),
                };
                // Run 147: a tap on a text input focuses it, as on the device.
                if let RunnerCommand::Tap { x, y, .. } = &cmd {
                    focus_tapped(&mut phone.screen, jarvis_proto::Point::new(*x, *y));
                }
                result
            }
            Effect::Confirm(req) => {
                change(confirms.len(), &req, &mut phone.screen);
                confirms.push(req);
                Event::phone_decision(script.decisions.pop_front().unwrap_or(false))
            }
            Effect::AwaitUnlock { .. } => Event::DeviceUnlocked,
            Effect::Finished(o) => return (effects, confirms, o),
            other => panic!("unexpected {other:?}"),
        };
    }
    panic!("did not finish");
}

/// Three places with long names (`twentieth_fixes::three_places`, with a link in the inner
/// frame), and `extra` native controls.
fn three_places(extra: Vec<UiNode>) -> Screen {
    let mut kids = vec![node(Role::Button, "Menu", rect(8.0, 50.0, 80.0, 40.0))];
    kids.push(
        UiNode::new(Role::WebView, rect(0.0, 140.0, 393.0, 600.0)).with_children(vec![
            text("Outer page", rect(16.0, 160.0, 361.0, 30.0)),
            UiNode::new(Role::WebView, rect(0.0, 400.0, 393.0, 300.0)).with_children(vec![
                text("Inner frame", rect(16.0, 420.0, 361.0, 30.0)),
                node(Role::Link, "Learn more", rect(16.0, 470.0, 200.0, 40.0)),
            ]),
        ]),
    );
    kids.extend(extra);
    scr(LONG_APP, LONG_NAME, kids)
}

fn changed() -> Screen {
    three_places(vec![node(
        Role::Button,
        "Slide to pay $500",
        rect(150.0, 250.0, 100.0, 300.0),
    )])
}

/// Whether every runner command that follows an approved question comes after an
/// observation (the look after an approval) — on every path.
fn looked_before_every_act(effects: &[Effect], decisions: &[bool]) -> bool {
    let mut k = 0usize;
    let mut pending = false;
    for e in effects {
        match e {
            Effect::Confirm(_) => {
                pending |= decisions.get(k).copied().unwrap_or(false);
                k += 1;
            }
            Effect::Observe { .. } | Effect::Infer(_) => pending = false,
            Effect::Act(_) if pending => return false,
            _ => {}
        }
    }
    true
}

/// The swipe and the effects from the last question before it.
fn last_question_to_drag(effects: &[Effect]) -> Option<&[Effect]> {
    let at = effects
        .iter()
        .position(|e| matches!(e, Effect::Act(RunnerCommand::Swipe { .. })))?;
    let q = effects[..at]
        .iter()
        .rposition(|e| matches!(e, Effect::Confirm(_)))?;
    Some(&effects[q..=at])
}

/// F2a: after an approved tap into the inner frame, the scroll asks in two rounds: the first
/// names only the sources that fit (partial by its sources); the second names the rest and
/// cannot name all three destinations, but the one it leaves out (the inner frame) has
/// nothing unallowed: it is not partial — no third round, and the drag follows the look
/// after its approval (before the fix it was partial, its approval decided the scroll again
/// and sent it with no look: probe p2).
#[test]
fn a_destination_left_out_with_nothing_unallowed_is_not_partial() {
    let s0 = three_places(vec![]);
    let learn = index_of(&s0, "Learn more");
    let (effects, confirms, _) = drive(
        cfg(),
        "Look through the offers",
        Phone::still(s0),
        Script::model(&[&tap(learn), SCROLL, DONE]).decisions(&[true; 20]),
        |_, _, _| {},
    );
    let scrolls: Vec<&ConfirmRequest> = confirms
        .iter()
        .filter(|c| c.verb == ConfirmVerb::Scroll)
        .collect();
    assert_eq!(scrolls.len(), 2, "{scrolls:#?}");
    assert!(
        scrolls[0].summary.contains("Jarvis asks about them next"),
        "{}",
        scrolls[0].summary
    );
    assert!(
        scrolls[1].summary.contains("(it reaches 1 more place")
            && !scrolls[1].summary.contains("Jarvis asks about them next"),
        "{}",
        scrolls[1].summary
    );
    let span = last_question_to_drag(&effects).expect("the scroll was sent");
    assert!(span.iter().any(|e| matches!(e, Effect::Observe { .. })));
    assert!(looked_before_every_act(&effects, &[true; 20]));
}

/// F2a, the changed screen: the page changes while the (not partial) scroll question is up;
/// the look after the approval finds it changed and the drag is not sent (the model is told
/// and decides again).
#[test]
fn a_changed_screen_after_the_last_approval_is_not_acted_on() {
    let s0 = three_places(vec![]);
    let learn = index_of(&s0, "Learn more");
    let (effects, _, outcome) = drive(
        cfg(),
        "Look through the offers",
        Phone::still(s0),
        Script::model(&[&tap(learn), SCROLL, DONE]).decisions(&[true; 20]),
        |_, q, s| {
            if q.verb == ConfirmVerb::Scroll {
                *s = changed();
            }
        },
    );
    assert!(
        outcome
            .summary
            .contains("the screen changed after the owner approved it"),
        "{outcome:?}"
    );
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::Act(RunnerCommand::Swipe { .. }))),
        "dragged on a changed screen"
    );
}

/// F2 true partial: nothing granted, so the destination left out of the first scroll
/// question still receives unallowed sources: a second question; after its approval the
/// drag follows the look after an approval.
#[test]
fn a_true_partial_asks_again_and_the_drag_follows_a_look() {
    let (effects, confirms, _) = drive(
        cfg(),
        "Look through the offers",
        Phone::still(three_places(vec![])),
        Script::model(&[SCROLL, DONE]).decisions(&[true; 20]),
        |_, _, _| {},
    );
    let scrolls = confirms
        .iter()
        .filter(|c| c.verb == ConfirmVerb::Scroll)
        .count();
    assert!(scrolls >= 2, "{confirms:#?}");
    let span = last_question_to_drag(&effects).expect("the scroll was sent");
    assert!(span.iter().any(|e| matches!(e, Effect::Observe { .. })));
    assert!(looked_before_every_act(&effects, &[true; 20]));
}

/// F2 true partial, changed screen: the page changes while the second (last) question is
/// up; the look after it refuses the drag.
#[test]
fn a_true_partial_on_a_changed_screen_is_not_acted_on() {
    let (effects, confirms, outcome) = drive(
        cfg(),
        "Look through the offers",
        Phone::still(three_places(vec![])),
        Script::model(&[SCROLL, DONE]).decisions(&[true; 20]),
        |k, _, s| {
            if k == 1 {
                *s = changed();
            }
        },
    );
    assert!(
        outcome
            .summary
            .contains("the screen changed after the owner approved it"),
        "{outcome:?}"
    );
    assert!(confirms.len() >= 2);
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::Act(RunnerCommand::Swipe { .. }))),
        "dragged on a changed screen"
    );
}

/// A field in the long-named app; `framed`: its web content (two nested web views, whose
/// covering names do not fit one question) is shown under it.
fn form(framed: bool, value: Option<&str>) -> Screen {
    let mut field = UiNode::new(Role::TextField, rect(16.0, 60.0, 361.0, 40.0))
        .with_label("Note")
        .hittable(true);
    if let Some(v) = value {
        field = field.with_value(v);
    }
    let mut kids = vec![field];
    if framed {
        kids.push(
            UiNode::new(Role::WebView, rect(0.0, 140.0, 393.0, 600.0)).with_children(vec![
                text("Outer page", rect(16.0, 160.0, 361.0, 30.0)),
                UiNode::new(Role::WebView, rect(0.0, 400.0, 393.0, 300.0))
                    .with_children(vec![text("Inner frame", rect(16.0, 420.0, 361.0, 30.0))]),
            ]),
        );
    }
    scr(LONG_APP, LONG_NAME, kids)
}

/// F2b, the Return stage: after reading Acme, `open_app` asks the flow question; then a
/// `type … submit` types, and Return — gated on the screen after the typing (`gate_return`)
/// — asks its own question, whose approval sends Return only after the look after an
/// approval. Every command that follows an approval comes after a look.
#[test]
fn a_typing_and_its_return_follow_a_look_after_each_approval() {
    let open = r#"{"action":"open_app","app":"com.example.longname"}"#;
    let field = index_of(&form(false, None), "Note");
    let typing = format!(r#"{{"action":"type","element":{field},"text":"hello","submit":true}}"#);
    // The typing shows the app's web content under the field (read at the look after it).
    let phone = Phone::new(acme(), |cmd, _| match cmd {
        RunnerCommand::LaunchApp { .. } => Transition::Go(form(false, None)),
        RunnerCommand::TypeText { .. }
        | RunnerCommand::PressKey {
            key: Key::Return, ..
        } => Transition::Go(form(true, Some("hello"))),
        _ => Transition::Stay,
    });
    let decisions = [true; 20];
    let (effects, confirms, outcome) = drive(
        cfg(),
        "Write hello in Shop",
        phone,
        Script::model(&[open, &typing, DONE]).decisions(&decisions),
        |_, _, _| {},
    );
    assert_eq!(outcome.status, OutcomeStatus::Success, "{confirms:#?}");
    assert!(
        effects.iter().any(|e| matches!(
            e,
            Effect::Act(RunnerCommand::PressKey {
                key: Key::Return,
                ..
            })
        )),
        "{confirms:#?}"
    );
    assert!(looked_before_every_act(&effects, &decisions));
}

// ---- F3: the accepted cost of swipes ------------------------------------------------------

fn gallery(k: usize) -> Screen {
    let page = vec![
        text("Holiday photos", rect(16.0, 100.0, 361.0, 30.0)),
        UiNode::new(
            Role::Other("Carousel".into()),
            rect(0.0, 200.0, 393.0, 300.0),
        )
        .with_children(vec![node(
            Role::Image,
            &format!("Photo {k} of 8"),
            rect(0.0, 200.0, 393.0, 300.0),
        )]),
    ];
    let kids = vec![
        UiNode::new(Role::WebView, rect(0.0, 0.0, 393.0, 700.0)).with_children(page),
        node(Role::Button, "Back", rect(8.0, 760.0, 40.0, 40.0)),
        UiNode::new(Role::TextField, rect(100.0, 800.0, 200.0, 30.0))
            .with_label("Address")
            .with_value("photos.example")
            .with_identifier("TabBarItemTitle"),
    ];
    scr(SAFARI, "Safari", kids)
}

/// F3 (accepted, not fixed): five swipes through a page's gallery ask five flow questions
/// (a swipe's approval keeps nothing: a kept one would cover an edge swipe that navigates,
/// and no OS fact tells them apart; ARCHITECTURE.md, Known gaps (9)); an owner who approves
/// everything finishes. Pinned so a later change is noticed
/// (`twentyfirst_review::p4_…`, ignored, asserts at most four).
#[test]
fn f3_a_gallery_of_swipes_asks_once_per_swipe() {
    let mut k = 1usize;
    let phone = Phone::new(gallery(1), move |cmd, _| {
        if matches!(cmd, RunnerCommand::Swipe { .. }) {
            k += 1;
            Transition::Go(gallery(k))
        } else {
            Transition::Stay
        }
    });
    let photo = index_of(&gallery(1), "Photo 1 of 8");
    let sw = format!(r#"{{"action":"swipe","direction":"left","element":{photo}}}"#);
    let r = run(
        cfg(),
        "Show me the holiday photos on this page",
        MemoryStore::new(),
        phone,
        Script::model(&[&sw, &sw, &sw, &sw, &sw, DONE]).decisions(&[true; 20]),
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Success, "{:?}", r.outcome);
    assert_eq!(flows(&r).len(), 5, "{:#?}", flows(&r));
    assert_eq!(drags(&r.acts), 5);
}

// ---- Over-asking, measured (an owner who always approves) ----------------------------------

fn settings(hour: usize) -> Screen {
    let mut kids = vec![text("Alarm", rect(16.0, 100.0, 170.0, 30.0))];
    for k in 0..4 {
        kids.push(
            UiNode::new(Role::Cell, rect(0.0, 150.0 + 50.0 * k as f64, 393.0, 44.0))
                .with_label(format!("Option {k}"))
                .hittable(true),
        );
    }
    kids.push(
        node(Role::Picker, "Hour", rect(16.0, 400.0, 120.0, 160.0)).with_value(hour.to_string()),
    );
    scr("com.example.clock", "Clock", kids)
}

/// A settings-like single app: scroll its list, then set a picker by three scrolls on it.
/// Nothing was read elsewhere, so there is no flow question, before and after R21-A (a
/// scroll on the picker is a full input, but there is no other place to ask about).
#[test]
fn a_single_app_picker_task_asks_no_flow_question() {
    let mut hour = 1usize;
    let phone = Phone::new(settings(1), move |cmd, _| match cmd {
        RunnerCommand::Swipe { from, .. } if from.x < 140.0 => {
            hour += 1;
            Transition::Go(settings(hour))
        }
        _ => Transition::Stay,
    });
    let spin = scroll_on(index_of(&settings(1), "Hour"), "down");
    let r = run(
        cfg(),
        "Set the alarm hour to 4",
        MemoryStore::new(),
        phone,
        Script::model(&[SCROLL, &spin, &spin, &spin, DONE]).decisions(&[true; 20]),
    );
    assert_eq!(flows(&r).len(), 0, "{:#?}", flows(&r));
    assert_eq!(drags(&r.acts), 4, "{:?} {:#?}", r.outcome, r.confirms);
}
