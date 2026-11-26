#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Twenty-first review probes (agent). Each asserts the SAFE behaviour: a failing probe is a
//! hole; a passing one refutes a suspicion. Copied unchanged in setup and assertions
//! (reformatted only); p1, p1b and p2 failed before the fixes (R21-A, F2), p1c and p3 held;
//! p4 (over-asking) is ignored as an accepted cost.

mod common;

use common::*;
use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{
    AppInfo, ConfirmRequest, ConfirmVerb, Effect, Event, ImageRef, ModelOutput, Observation, Role,
    RunnerCommand, RunnerResult, Screen, SessionConfig, TaskSpec, TimedEvent, UiNode,
};

const FLOW: &str = "what it read there could be passed on here";
const DONE: &str = r#"{"action":"done","summary":"ok"}"#;
const SCROLL: &str = r#"{"action":"scroll","direction":"down"}"#;
const ACME: &str = "com.example.acme";
const SHOP: &str = "com.example.shop";
const SAFARI: &str = "com.apple.mobilesafari";

fn cfg() -> SessionConfig {
    let mut c = config();
    c.known_apps.push(AppInfo::new(SAFARI, "Safari"));
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

// ---- P1: a scroll that actuates a control is kept as a scroll ----------------------------

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

/// Shop: a list of items, and (left, away from the screen scroll's path at x = 196) a
/// picker wheel showing `hour` (a native `UIPickerView`/`UIDatePicker` wheel; listed with
/// role Picker), and a slider.
fn shop(hour: usize, n: usize) -> Screen {
    let mut rows = vec![text("Book a pickup", rect(16.0, 100.0, 361.0, 30.0))];
    rows.push(
        node(Role::Picker, "Hour", rect(16.0, 300.0, 90.0, 160.0)).with_value(hour.to_string()),
    );
    for k in 0..4 {
        rows.push(node(
            Role::Button,
            &format!("Item {k}"),
            rect(220.0, 150.0 + 60.0 * k as f64, 150.0, 44.0),
        ));
    }
    // A product tile (a card button taller than the 80 pt minimum travel: a drag on it
    // starts inside it).
    rows.push(node(
        Role::Button,
        "Gift card tile",
        rect(220.0, 420.0, 150.0, 140.0),
    ));
    rows.push(text(&format!("Viewed {n}"), rect(16.0, 800.0, 361.0, 20.0)));
    scr(SHOP, "Shop", rows)
}

fn acme_then_shop() -> Phone {
    let mut n = 0usize;
    let mut hour = 1usize;
    Phone::new(acme(), move |cmd, s| {
        n += 1;
        if taps(cmd, s, "Open Shop") {
            return Transition::Go(shop(hour, n));
        }
        if s.app_bundle_id == SHOP {
            if let RunnerCommand::Swipe { from, .. } = cmd {
                if from.x < 110.0 {
                    hour += 1;
                }
            }
            return Transition::Go(shop(hour, n));
        }
        Transition::Stay
    })
}

/// P1 (kind confusion): after reading Acme's code, the first input into Shop is a screen
/// scroll; its question ("Scroll Down … Jarvis has read Acme …") is approved and kept as a
/// scroll-only grant. Every later `scroll` is then silent — including a scroll whose element
/// is a picker wheel, which the runner turns into a drag that SETS the wheel's value (the
/// classifier itself knows the picker reacts to the drag: `reacts_to_drag`). Spinning the
/// wheel by repeated scrolls enters a chosen value into Shop. Safe: a scroll that actuates a
/// control asks like a tap (it is a full input).
#[test]
fn p1_a_picker_scroll_is_not_covered_by_a_scroll_grant() {
    let a = acme();
    let open = index_of(&a, "Open Shop");
    let wheel = index_of(&shop(1, 0), "Hour");
    let spin = scroll_on(wheel, "down");
    let r = run(
        cfg(),
        "Open Shop and book a pickup",
        MemoryStore::new(),
        acme_then_shop(),
        Script::model(&[&tap(open), SCROLL, &spin, &spin, &spin, DONE]).decisions(&[true; 8]),
    );
    let fq = flows(&r);
    eprintln!(
        "flow questions ({}): {:#?}\ndrags {}\noutcome {:?}",
        fq.len(),
        fq.iter().map(|c| (&c.verb, &c.summary)).collect::<Vec<_>>(),
        drags(&r.acts),
        r.outcome
    );
    assert!(
        fq.len() >= 2,
        "the picker was spun {} times into Shop after only a screen scroll was approved",
        drags(&r.acts).saturating_sub(1)
    );
}

/// P1b: the same grant covers a scroll whose element is any control: the drag starts on the
/// element the model chose (touchDown on a native button; touchstart/pointerdown on a page's
/// element), so the choice of element — not only a direction — reaches the place. Safe: a
/// scroll started on a button-like control asks like a tap.
#[test]
fn p1b_a_scroll_started_on_a_button_is_not_covered_by_a_scroll_grant() {
    let a = acme();
    let open = index_of(&a, "Open Shop");
    let item = index_of(&shop(1, 0), "Gift card tile");
    let r = run(
        cfg(),
        "Open Shop and look around",
        MemoryStore::new(),
        acme_then_shop(),
        Script::model(&[&tap(open), SCROLL, &scroll_on(item, "down"), DONE]).decisions(&[true; 8]),
    );
    let fq = flows(&r);
    eprintln!(
        "flow questions ({}): {:#?}\nacts {:?}",
        fq.len(),
        fq.iter().map(|c| (&c.verb, &c.summary)).collect::<Vec<_>>(),
        r.acts
    );
    assert!(
        fq.len() >= 2,
        "a drag started on the tile was sent under a scroll-only grant"
    );
}

/// P1c (refutation): a plain screen scroll after the scroll grant stays silent (the grant
/// works as designed), and a tap still asks.
#[test]
fn p1c_screen_scrolls_silent_tap_asks() {
    let a = acme();
    let open = index_of(&a, "Open Shop");
    let item = index_of(&shop(1, 0), "Item 1");
    let r = run(
        cfg(),
        "Open Shop and look around",
        MemoryStore::new(),
        acme_then_shop(),
        Script::model(&[&tap(open), SCROLL, SCROLL, &tap(item), DONE]).decisions(&[true; 8]),
    );
    let v: Vec<ConfirmVerb> = flows(&r).iter().map(|c| c.verb).collect();
    assert_eq!(v, vec![ConfirmVerb::Scroll, ConfirmVerb::Tap], "{v:?}");
}

// ---- P2: a partial approval with destinations left over skips the approval look --------

/// A driver like `common::run` that lets the screen change while a question is up
/// (`change(k, screen)` is called when the k-th question, from 0, is shown) and records the
/// effects in order.
fn drive(
    config: SessionConfig,
    task: &str,
    mut phone: Phone,
    mut script: Script,
    mut change: impl FnMut(usize, &ConfirmRequest, &mut Screen),
) -> (Vec<Effect>, Vec<ConfirmRequest>) {
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
            Effect::Act(cmd) => match (phone.transition)(&cmd, &phone.screen) {
                Transition::Stay => Event::ActResult(RunnerResult::ok()),
                Transition::Go(s) => {
                    phone.screen = s;
                    Event::ActResult(RunnerResult::ok())
                }
                Transition::Fail(e) => Event::ActResult(RunnerResult::err(e)),
            },
            Effect::Confirm(req) => {
                change(confirms.len(), &req, &mut phone.screen);
                confirms.push(req);
                Event::phone_decision(script.decisions.pop_front().unwrap_or(false))
            }
            Effect::AwaitUnlock { .. } => Event::DeviceUnlocked,
            Effect::Finished(_) => return (effects, confirms),
            other => panic!("unexpected {other:?}"),
        };
    }
    panic!("did not finish");
}

const LONG_APP: &str = "com.example.longname";
/// 40 characters, no ", " / " and " (shown as is).
const LONG_NAME: &str = "Shop $1000.00 $2000.00 $3000.00 $4000.00";

/// A third-party app (not a browser) whose web content holds a nested web view (an
/// embedded frame: an ad, a payment widget) — three places with long names, as in
/// `twentieth_fixes::three_places`. `extra`: native controls added on top.
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

/// P2 (stale screen): an input with three destinations whose names do not all fit is asked
/// in parts (R20-A). When the destination left unnamed has nothing unallowed (here the
/// inner frame, granted by an earlier approved tap), approving the named part re-decides
/// the action, finds nothing more to ask, and SENDS it at once — without the look after an
/// approval (A1–A4: the screen must still be the one the owner was asked about). Here the
/// page changed while the owner answered: a "Slide to pay" control now lies on the drag's
/// path. Safe: after the last approval a fresh look precedes the drag (it would find the
/// screen changed and not act).
#[test]
fn p2_a_partial_destination_approval_still_looks_before_acting() {
    let mut c = cfg();
    c.known_apps.push(AppInfo::new(LONG_APP, LONG_NAME));
    let s0 = three_places(vec![]);
    let learn = index_of(&s0, "Learn more");
    let changed = three_places(vec![node(
        Role::Button,
        "Slide to pay $500",
        rect(150.0, 250.0, 100.0, 300.0),
    )]);
    let (effects, confirms) = drive(
        c,
        "Look through the offers",
        Phone::still(s0),
        Script::model(&[&tap(learn), SCROLL, DONE]).decisions(&[true; 20]),
        |_, q, s| {
            // While the scroll's question is up the page reflows (the owner takes time).
            if q.verb == ConfirmVerb::Scroll {
                *s = changed.clone();
            }
        },
    );
    for c in &confirms {
        eprintln!("Q {:?} {:?}: {}", c.verb, c.category, c.summary);
    }
    // The drag and what came just before it.
    let at = effects
        .iter()
        .position(|e| matches!(e, Effect::Act(RunnerCommand::Swipe { .. })));
    let Some(at) = at else {
        return; // nothing dragged: safe
    };
    let last_q = effects[..at]
        .iter()
        .rposition(|e| matches!(e, Effect::Confirm(_)))
        .expect("the scroll asked");
    let looked = effects[last_q..at]
        .iter()
        .any(|e| matches!(e, Effect::Observe { .. }));
    eprintln!(
        "effects between last question and drag: {:?}",
        &effects[last_q..=at]
    );
    assert!(
        looked,
        "the scroll was sent right after a partial approval, with no look at the (changed) \
         screen"
    );
}

// ---- P3: action-only pairs do not outlive a stale approval --------------------------------

/// P3 (refutation): a swipe's approval keeps nothing beyond the action; when the screen
/// changed while the owner answered (look finds it stale), the model's next action (a tap)
/// asks: `action_allowed` is cleared at the next proposal.
#[test]
fn p3_action_only_pairs_do_not_survive_a_stale_approval() {
    let a = acme();
    let open = index_of(&a, "Open Shop");
    let item = index_of(&shop(1, 0), "Item 2");
    let swipe = r#"{"action":"swipe","direction":"left"}"#;
    let mut changed_once = false;
    let (_effects, confirms) = drive(
        cfg(),
        "Open Shop",
        acme_then_shop(),
        Script::model(&[&tap(open), swipe, &tap(item), DONE]).decisions(&[true; 8]),
        move |k, _, s| {
            if k == 0 && !changed_once && s.app_bundle_id == SHOP {
                changed_once = true;
                *s = shop(7, 99);
            }
        },
    );
    let v: Vec<ConfirmVerb> = confirms
        .iter()
        .filter(|c| c.summary.contains(FLOW))
        .map(|c| c.verb)
        .collect();
    eprintln!("{v:?}");
    assert!(v.contains(&ConfirmVerb::Tap), "{v:?}");
}

// ---- P4: over-asking — a swipe-heavy single-app task -------------------------------------

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

/// P4 (over-asking, LOW): an ordinary one-app task — swipe through a page's gallery in
/// Safari (5 swipes) — asks a flow question for every swipe (each reaches the page and the
/// chrome; a swipe keeps nothing). An owner who approves everything sees 5 questions.
///
/// Twenty-first review (F3): accepted, not fixed — a kept swipe approval would also cover an
/// edge swipe that navigates, and no OS fact tells them apart. The actual count is pinned in
/// `twentyfirst_fixes.rs::f3_a_gallery_of_swipes_asks_once_per_swipe`.
#[test]
#[ignore = "accepted cost: a swipe's approval keeps nothing (ARCHITECTURE.md, Known gaps (9))"]
fn p4_a_gallery_of_swipes_asks_at_most_four_questions() {
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
    let fq = flows(&r);
    eprintln!(
        "{} flow questions, {} drags, {:?}",
        fq.len(),
        drags(&r.acts),
        r.outcome
    );
    assert!(fq.len() <= 4, "{} flow questions for 5 swipes", fq.len());
}
