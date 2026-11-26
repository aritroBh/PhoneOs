#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Run 146 (Mac, iOS 26.5 simulator): a step was verified with a single look and no settle
//! wait. A replayed tap on a SwiftUI switch was looked at 1.42 s after the tap and still read
//! `value="0"` (it read `1` at 2.58 s in the run that learned the step): the step was
//! "nothing changed", the replay ended and the model was called. The core now waits
//! [`VERIFY_SETTLE_MS`] and looks once more before concluding "nothing changed" — for model
//! steps and replayed steps alike, never for a scroll or swipe, through the chosen-look gate
//! (R24), not as a step, and with the final result alone in the history.

mod common;

use std::cell::Cell;
use std::rc::Rc;

use common::*;
use jarvis_agent::{Session, VERIFY_SETTLE_MS};
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::settings_screen;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{
    Effect, Event, ModelOutput, Observation, OutcomeStatus, Role, RunnerCommand, RunnerResult,
    Screen, TaskSpec, TimedEvent, UiNode,
};

fn settle() -> Effect {
    Effect::Act(RunnerCommand::Wait {
        ms: VERIFY_SETTLE_MS,
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

fn script() -> Vec<String> {
    vec![
        r#"{"action":"tap","element":4}"#.to_owned(),
        format!(r#"{{"action":"tap","element":{}}}"#, switch_index()),
        r#"{"action":"done","summary":"Wi-Fi is off"}"#.to_owned(),
    ]
}

/// Settings → Wi-Fi → the switch. `late`: the switch's new value shows only after a wait
/// (what the verify settle wait gives it), as on the Mac run; otherwise at once.
fn wifi_phone(late: bool) -> Phone {
    let flipped = Rc::new(Cell::new(false));
    Phone::new(settings_screen(), move |cmd, s: &Screen| {
        if taps(cmd, s, "Wi-Fi, HomeNet") {
            return Transition::Go(wifi_screen(true));
        }
        if s.app_bundle_id == "com.apple.Preferences" && taps(cmd, s, "Wi-Fi") {
            if late {
                flipped.set(true);
                return Transition::Stay;
            }
            return Transition::Go(wifi_screen(false));
        }
        if matches!(cmd, RunnerCommand::Wait { .. }) && flipped.replace(false) {
            return Transition::Go(wifi_screen(false));
        }
        Transition::Stay
    })
}

fn strs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

/// A model's tap whose change shows only on the second look is a success: one settle wait,
/// no extra step, no extra model call, and the history shows only "OK".
#[test]
fn a_tap_whose_change_shows_on_the_second_look_is_a_success() {
    let s = script();
    let r = run(
        config(),
        "Turn off Wi-Fi",
        MemoryStore::new(),
        wifi_phone(true),
        Script::model(&strs(&s)).decisions(&[true; 4]),
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.outcome.summary
    );
    assert_eq!(r.outcome.steps, 3, "the second look is not a step");
    assert_eq!(r.infers(), 3, "the second look is not a model call");
    assert_eq!(
        r.settle_waits,
        vec![2],
        "one settle wait, after the switch's tap"
    );
    let last = r.last_user();
    assert!(!last.contains("nothing changed"), "{last}");
    assert!(last.contains("\"Wi-Fi\" -> ok"), "{last}");
    assert!(jarvis_observe::compact(&r.screen)
        .elements
        .iter()
        .any(|e| e.role == Role::Switch && e.value.as_deref() == Some("0")));
}

/// The Mac failure: the replayed tap's change shows only after the settle wait, and the
/// replay continues with zero model calls (it used to diverge and call the model).
#[test]
fn a_replayed_tap_whose_change_shows_on_the_second_look_keeps_the_replay() {
    let s = script();
    let first = run(
        config(),
        "Turn off Wi-Fi",
        MemoryStore::new(),
        wifi_phone(false),
        Script::model(&strs(&s)).decisions(&[true; 4]),
    );
    assert_eq!(first.outcome.status, OutcomeStatus::Success);
    assert!(first.settle_waits.is_empty(), "{:?}", first.settle_waits);
    assert_eq!(first.memory.flows.len(), 1);
    let second = run(
        config(),
        "turn off Wi-Fi.",
        first.memory,
        wifi_phone(true),
        Script::model(&[]).decisions(&[true; 4]),
    );
    assert_eq!(
        second.outcome.status,
        OutcomeStatus::Success,
        "{}\n{}",
        second.outcome.summary,
        second.all_prompt_text()
    );
    assert_eq!(second.infers(), 0, "the replay must not call the model");
    assert_eq!(second.acts.len(), 2);
    assert_eq!(second.settle_waits, vec![2]);
    let flow = &second.memory.flows.flows()[0];
    assert_eq!(
        flow.successes, 2,
        "the replay counts as a success of the flow"
    );
}

/// A change that never appears: exactly one settle wait and one more look per step, then
/// the old result. A scroll to the end of a list takes no settle wait.
#[test]
fn a_change_that_never_shows_takes_one_retry_then_the_old_result() {
    let r = run(
        config(),
        "open general",
        MemoryStore::new(),
        Phone::still(settings_screen()),
        Script::model(&[
            r#"{"action":"tap","element":7}"#,
            r#"{"action":"scroll","direction":"down"}"#,
            r#"{"action":"swipe","direction":"left","element":7}"#,
            r#"{"action":"fail","reason":"nothing happens"}"#,
        ])
        .decisions(&[true]),
    );
    assert_eq!(r.settle_waits, vec![1], "the tap only: {:?}", r.effects);
    let observes = r
        .effects
        .iter()
        .filter(|e| matches!(e, Effect::Observe { .. }))
        .count();
    // First look, the tap's two, the scroll's one, the swipe's (approved: the look after
    // the approval, then its verify look) two.
    assert_eq!(observes, 6, "{:?}", r.effects);
    let u = r.last_user();
    assert!(
        u.contains("tap [7] \"General\" -> failed: nothing changed on screen"),
        "{u}"
    );
    assert_eq!(
        u.matches("nothing changed on screen").count(),
        1,
        "the step history shows the final result only: {u}"
    );
    assert!(
        u.contains("scroll down -> failed: nothing changed (end of the list?)"),
        "{u}"
    );
    assert_eq!(r.outcome.steps, 4);
}

struct Driver {
    s: Session,
    now: u64,
}

impl Driver {
    fn new() -> Driver {
        Driver {
            s: Session::new(
                config(),
                TaskSpec::new("t", "Open General"),
                MemoryStore::new(),
            ),
            now: 1_000,
        }
    }
    fn at(&mut self, e: Event) -> Effect {
        self.now += 50;
        self.s.step(TimedEvent::new(self.now, e))
    }
    fn observed(&mut self, screen: &Screen) -> Effect {
        self.at(Event::Observed(Observation::new(screen.clone())))
    }
    fn model(&mut self, text: &str) -> Effect {
        self.at(Event::ModelOutput(ModelOutput {
            text: text.to_owned(),
        }))
    }
    fn ok(&mut self) -> Effect {
        self.at(Event::ActResult(RunnerResult::ok()))
    }
    /// Start, tap "General", the first verify look unchanged: the settle wait is out.
    fn tapped_unchanged() -> Driver {
        let mut d = Driver::new();
        assert_eq!(d.at(Event::Start), Effect::Observe { screenshot: false });
        assert!(matches!(d.observed(&settings_screen()), Effect::Infer(_)));
        assert!(matches!(
            d.model(r#"{"action":"tap","element":7}"#),
            Effect::Act(RunnerCommand::Tap { .. })
        ));
        assert_eq!(d.ok(), Effect::Observe { screenshot: false });
        assert_eq!(d.observed(&settings_screen()), settle());
        assert_eq!(d.s.progress().step, 1);
        d
    }
}

fn infer_user(e: Effect) -> String {
    match e {
        Effect::Infer(r) => r.user,
        other => panic!("expected Infer, got {other:?}"),
    }
}

/// Another app in front on the second look is a change, not "nothing changed".
#[test]
fn another_app_in_front_on_the_second_look_is_a_change() {
    let mut d = Driver::tapped_unchanged();
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    let u = infer_user(d.observed(&home_screen()));
    assert!(!u.contains("nothing changed"), "{u}");
    assert!(u.contains("\"General\" -> ok"), "{u}");
    assert_eq!(d.s.progress().step, 1, "the second look is not a step");
    assert_eq!(d.s.progress().model_calls, 2);
}

/// The phone locks between the two looks — the wait refused as locked, or the second look
/// locked: the action ran, its result is unknown (as for a lock during a verify look).
#[test]
fn a_lock_between_the_two_looks_leaves_the_result_unknown() {
    for second_look_locked in [false, true] {
        let mut d = Driver::tapped_unchanged();
        let e = if second_look_locked {
            assert_eq!(d.ok(), Effect::Observe { screenshot: false });
            let mut locked = Observation::new(settings_screen());
            locked.device_locked = true;
            d.at(Event::Observed(locked))
        } else {
            d.at(Event::ActResult(RunnerResult::device_locked()))
        };
        assert!(matches!(e, Effect::AwaitUnlock { .. }), "{e:?}");
        assert_eq!(
            d.at(Event::DeviceUnlocked),
            Effect::Observe { screenshot: false }
        );
        let u = infer_user(d.observed(&settings_screen()));
        assert!(
            u.contains("done, but the phone locked before the result could be checked"),
            "{second_look_locked}: {u}"
        );
        assert!(!u.contains("nothing changed"), "{u}");
        assert_eq!(d.s.progress().step, 1);
    }
}

// ---------------------------------------------------------------------------------------
// R24: no settle look while a place on the screen may not see one.
// ---------------------------------------------------------------------------------------

const MSGS: &str = "com.apple.MobileSMS";
const SHOP: &str = "com.example.shop";

fn scr(app: &str, name: &str, children: Vec<UiNode>) -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(name)
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(children)
        ]);
    screen(app, name, root, false)
}

fn messages() -> Screen {
    scr(
        MSGS,
        "Messages",
        vec![
            text(
                "Bank: your code is 482913. Balance $12,480.17",
                rect(16.0, 140.0, 361.0, 40.0),
            ),
            node(
                Role::Link,
                "shop.example/daily",
                rect(16.0, 200.0, 361.0, 44.0),
            ),
        ],
    )
}

/// Shop, and with `loading` an empty web view (a page still loading: a place of its own,
/// with nothing listed yet, so the compact screen is the same).
fn shop(loading: bool) -> Screen {
    let mut kids = vec![
        text("Daily reward", rect(16.0, 60.0, 200.0, 30.0)),
        node(Role::Button, "Details", rect(16.0, 300.0, 200.0, 44.0)),
    ];
    if loading {
        kids.push(UiNode::new(Role::WebView, rect(0.0, 400.0, 393.0, 400.0)));
    }
    scr(SHOP, "Shop", kids)
}

/// R24: after Messages was read, the tap on "Details" in Shop is asked (the flow) and
/// approved; it brings up a page still loading (a new place, nothing listed). The screen
/// shows no change and that page may not see a look Jarvis chooses: no settle wait, no
/// second look, no question for it — the first reading concludes. The same tap with nothing
/// read before (Shop from the start) gets its settle wait.
#[test]
fn no_settle_look_while_a_place_on_the_screen_may_not_see_it() {
    let details = index_of(&shop(false), "Details");
    let tap_details = format!(r#"{{"action":"tap","element":{details}}}"#);
    let link = format!(
        r#"{{"action":"tap","element":{}}}"#,
        index_of(&messages(), "shop.example/daily")
    );
    let phone = |loads: bool| {
        Phone::new(messages(), move |cmd, s: &Screen| {
            if s.app_bundle_id == MSGS && taps(cmd, s, "shop.example/daily") {
                return Transition::Go(shop(false));
            }
            if loads && s.app_bundle_id == SHOP && taps(cmd, s, "Details") {
                return Transition::Go(shop(true));
            }
            Transition::Stay
        })
    };
    let fail = r#"{"action":"fail","reason":"no details"}"#;
    let after_read = run(
        config(),
        "Open the shop reward",
        MemoryStore::new(),
        phone(true),
        Script::model(&[&link, &tap_details, fail]).decisions(&[true; 4]),
    );
    assert_eq!(after_read.acts.len(), 2, "{:?}", after_read.acts);
    assert!(
        after_read.settle_waits.is_empty(),
        "{:?}",
        after_read.effects
    );
    let u = after_read.last_user();
    assert!(
        u.contains("\"Details\" -> failed: nothing changed on screen"),
        "{u}"
    );
    assert!(!after_read.confirms.is_empty(), "the tap's flow question");

    // The same approved tap with no new place on screen (the approval allowed Messages into
    // Shop): its settle wait and second look are taken, and no question is added for them.
    let plain = run(
        config(),
        "Open the shop reward",
        MemoryStore::new(),
        phone(false),
        Script::model(&[&link, &tap_details, fail]).decisions(&[true; 4]),
    );
    assert_eq!(plain.settle_waits, vec![2], "{}", plain.last_user());
    assert_eq!(plain.confirms.len(), after_read.confirms.len());
}
