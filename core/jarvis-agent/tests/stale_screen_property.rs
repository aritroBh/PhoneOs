#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Stale-screen guard (layer 1, with layer 2 as the phone's runner applies it): random
//! sessions with a random model latency, on a phone that may change the screen while the
//! model thinks: another app comes to the front, the target moves, is relabelled, is
//! replaced by another control in the same frame, an alert appears over it, or nothing
//! changes.
//!
//! Properties, checked against the phone's own record:
//! 1. No tap is emitted for a plan older than `Policy::max_plan_age_ms` when the screen
//!    changed between the observation it was planned on and the tap (the fresh look must
//!    have shown the same app and the same target).
//! 2. When nothing changed, the chosen tap is emitted exactly once for the reply (no
//!    livelock), with at most one extra observation before it.
//! 3. Every gesture carries the app it was planned on (`expect`), and the phone, which
//!    refuses a command whose app is not in front (as the shell's front-app check does),
//!    never carries out a tap in another app.
//!
//! Seeded (xorshift); a failure names its seed (`STALE_SEED=<seed>` runs it alone); the
//! default is 400 sessions, `STALE_SESSIONS=<n>` changes it.
mod common;

use common::*;
use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{node, rect, screen, IPHONE_SIZE};
use jarvis_proto::{
    Effect, Event, ModelOutput, Observation, Role, RunnerCommand, RunnerResult, Screen, TaskSpec,
    TimedEvent, UiNode, DEFAULT_MAX_PLAN_AGE_MS,
};

const DEFAULT_SESSIONS: u64 = 400;
const APP_A: &str = "com.example.shop";
const APP_B: &str = "com.example.acme";

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

/// The phone's page: two neutral buttons; what may change about "Alpha".
#[derive(Clone, Debug, PartialEq)]
struct Page {
    app: &'static str,
    alpha_y: f64,
    alpha_label: String,
    alpha_role: Role,
    alert: bool,
}

impl Page {
    fn start() -> Page {
        Page {
            app: APP_A,
            alpha_y: 300.0,
            alpha_label: "Alpha".to_owned(),
            alpha_role: Role::Button,
            alert: false,
        }
    }

    fn screen(&self) -> Screen {
        let mut kids = vec![
            node(
                self.alpha_role.clone(),
                &self.alpha_label,
                rect(16.0, self.alpha_y, 361.0, 50.0),
            ),
            node(Role::Button, "Beta", rect(16.0, 600.0, 361.0, 50.0)),
        ];
        if self.alert {
            kids.push(
                UiNode::new(Role::Alert, rect(40.0, 250.0, 313.0, 200.0)).with_children(vec![
                    node(Role::Button, "Allow", rect(40.0, 290.0, 313.0, 60.0)),
                ]),
            );
        }
        let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
            .with_label("App")
            .with_children(vec![
                UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(kids)
            ]);
        screen(self.app, "App", root, false)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Change {
    Nothing,
    AppSwitch,
    Moved,
    Relabelled,
    Replaced,
    Alert,
}

const CHANGES: [Change; 6] = [
    Change::Nothing,
    Change::AppSwitch,
    Change::Moved,
    Change::Relabelled,
    Change::Replaced,
    Change::Alert,
];

fn apply(page: &mut Page, change: Change) {
    match change {
        Change::Nothing => {}
        Change::AppSwitch => page.app = if page.app == APP_A { APP_B } else { APP_A },
        Change::Moved => page.alpha_y = if page.alpha_y == 300.0 { 380.0 } else { 300.0 },
        Change::Relabelled => page.alpha_label.push('+'),
        Change::Replaced => {
            page.alpha_role = Role::Link;
            page.alpha_label = "Delete account".to_owned();
        }
        Change::Alert => page.alert = !page.alert,
    }
}

/// One session; panics with the seed on a violation.
fn session(seed: u64) {
    let mut rng = Rng::new(seed);
    let mut page = Page::start();
    let mut s = Session::new(
        config(),
        TaskSpec::new("t", "Press the buttons"),
        MemoryStore::new(),
    );
    let mut now = 1_000u64;
    let mut event = Event::Start;
    // The page as the session last observed it, and when; the observation the outstanding
    // plan was made on, and the change made while the model thought.
    let mut observed = page.clone();
    let mut last_obs = 0u64;
    let mut plan: Option<(u64, Change)> = None;
    let mut reply_at = 0u64;
    let mut taps_for_reply = 0u32;
    let mut observes_for_reply = 0u32;
    let mut replies = 0u32;
    let mut affects = false;
    let mut confirms_for_reply = 0u32;
    // The tap must go out exactly once for this reply (nothing changed, nothing covers it,
    // and it is not a repetition the stuck guard may replace).
    let mut once = false;
    let mut chosen: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    let limit = DEFAULT_MAX_PLAN_AGE_MS;
    let trace = std::env::var_os("STALE_TRACE").is_some();
    for _ in 0..400 {
        let effect = s.step(TimedEvent::new(now, event));
        if trace {
            let shown = match &effect {
                Effect::Infer(r) => {
                    format!("Infer: ...{}", &r.user[r.user.len().saturating_sub(400)..])
                }
                other => format!("{other:?}"),
            };
            eprintln!("[{now}] {shown} page {page:?}");
        }
        event = match effect {
            Effect::Observe { .. } => {
                now += 100;
                last_obs = now;
                observed = page.clone();
                observes_for_reply += 1;
                Event::Observed(Observation::new(page.screen()))
            }
            Effect::Infer(_) => {
                // Close the previous reply's books: nothing changed and the plan was a tap,
                // so it went out exactly once.
                if once {
                    assert_eq!(
                        taps_for_reply, 1,
                        "seed {seed}: an unchanged screen's tap was emitted {taps_for_reply} times"
                    );
                }
                replies += 1;
                let latency = rng.below(3_000);
                let mut change = CHANGES[rng.below(CHANGES.len() as u64) as usize];
                if change == Change::Replaced && page.alpha_role != Role::Button {
                    change = Change::Nothing;
                }
                // The plan is made on the screen as observed (`seen`); the phone changes
                // while the model thinks.
                plan = Some((last_obs, change));
                let seen = observed.clone();
                apply(&mut page, change);
                now = last_obs + latency;
                reply_at = now;
                taps_for_reply = 0;
                observes_for_reply = 0;
                confirms_for_reply = 0;
                once = false;
                affects = false;
                let text = if replies > 6 {
                    r#"{"action":"done","summary":"ok"}"#.to_owned()
                } else {
                    // The model names a control of the screen it saw (before the change).
                    let beta = rng.below(2) == 0;
                    // What changed since the screen the plan was made on (every change since
                    // the session's last look) that touches the target: the app; an alert
                    // appearing or going (it hides the page under it, or was the target);
                    // or "Alpha" (its frame, label or role) when the target is Alpha. "Beta"
                    // and an alert's button are untouched by changes to Alpha.
                    let alpha = |p: &Page| (p.alpha_y, p.alpha_label.clone(), p.alpha_role.clone());
                    affects = seen.app != page.app
                        || seen.alert != page.alert
                        || (!seen.alert && !beta && alpha(&seen) != alpha(&page));
                    let shown = seen.screen();
                    // An alert hides the page under it: the model then answers the alert.
                    let index = if seen.alert {
                        find(&shown, "Allow")
                    } else if beta {
                        find(&shown, "Beta")
                    } else {
                        find(&shown, &seen.alpha_label)
                    }
                    .unwrap_or_else(|| panic!("seed {seed}: nothing to tap on {seen:?}"));
                    let text = format!(r#"{{"action":"tap","element":{index}}}"#);
                    let repeats = chosen.entry(text.clone()).or_insert(0);
                    *repeats += 1;
                    once = !affects && !seen.alert && *repeats < 3;
                    text
                };
                if trace {
                    eprintln!("  reply at {now}: {text} after {change:?}, once {once}");
                }
                Event::ModelOutput(ModelOutput { text })
            }
            Effect::Act(cmd) => {
                now += 100;
                let (planned_at, change) = plan.expect("an act without a plan");
                let stale = reply_at.saturating_sub(planned_at) > limit;
                if matches!(cmd, RunnerCommand::Tap { .. }) {
                    taps_for_reply += 1;
                    if stale && affects {
                        panic!(
                            "seed {seed}: a tap planned {} ms before the reply was sent although \
                             the screen changed ({change:?}) before it",
                            reply_at - planned_at
                        );
                    }
                    if stale {
                        assert!(
                            observes_for_reply >= 1,
                            "seed {seed}: an old plan's tap was sent without a fresh look"
                        );
                    }
                    // At most the fresh look, plus the look after an approval.
                    assert!(
                        observes_for_reply <= 1 + confirms_for_reply,
                        "seed {seed}: {observes_for_reply} looks before one tap"
                    );
                }
                // Layer 2: the phone refuses a gesture whose app is not in front.
                let expect = cmd.expect().cloned();
                assert!(
                    !matches!(cmd, RunnerCommand::Tap { .. }) || expect.is_some(),
                    "seed {seed}: a tap without its expectation: {cmd:?}"
                );
                if expect.is_some_and(|e| !e.matches(page.app)) {
                    Event::ActResult(RunnerResult::stale(page.app))
                } else {
                    Event::ActResult(RunnerResult::ok())
                }
            }
            Effect::Confirm(req) => {
                now += 100;
                confirms_for_reply += 1;
                // Sixteenth review (Rule T): after an app switch the first tap in the other
                // app asks whether what was read in the first may reach it. Its approval is
                // bound to the exact screen (A1), so a change elsewhere on the page voids it
                // (fail closed): the "sent exactly once" books apply to unasked taps only.
                if req.summary.contains("could be passed on here") {
                    once = false;
                }
                Event::UserDecision { approved: true }
            }
            Effect::AskUser { .. } => Event::UserAnswer {
                text: String::new(),
            },
            Effect::CallTool(_) => panic!("seed {seed}: no tools here"),
            Effect::AwaitUnlock { .. } => Event::DeviceUnlocked,
            Effect::Finished(_) => {
                let p = s.progress();
                assert!(
                    p.fresh_looks <= p.model_calls,
                    "seed {seed}: more fresh looks ({}) than model replies ({})",
                    p.fresh_looks,
                    p.model_calls
                );
                return;
            }
        };
    }
    panic!("seed {seed}: the session did not finish");
}

/// The number of the listed element labelled `label`, if listed.
fn find(screen: &Screen, label: &str) -> Option<u32> {
    jarvis_observe::compact(screen)
        .elements
        .iter()
        .find(|e| e.label == label)
        .map(|e| e.index)
}

#[test]
fn an_old_plan_is_acted_on_only_when_the_fresh_look_shows_the_same_target() {
    if let Some(seed) = std::env::var("STALE_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        session(seed);
        return;
    }
    let n = std::env::var("STALE_SESSIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SESSIONS);
    for seed in 1..=n {
        session(seed);
    }
}
