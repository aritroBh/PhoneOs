//! Workflows: validation, the sans-IO runner, the store and schedules.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::*;
use jarvis_proto::OutcomeStatus;
use jarvis_skills::schedule::{MINUTES_PER_DAY, MINUTES_PER_WEEK};
use jarvis_skills::{
    is_due, minutes_since_last, minutes_until_next, Awaiting, Condition, OnFailure, RunError,
    RunLimits, Schedule, SkillError, Step, Trigger, Weekday, Workflow, WorkflowEffect,
    WorkflowEvent, WorkflowOutcome, WorkflowRun, WorkflowStatus, WorkflowStore,
};

fn args(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

fn leaving_work() -> Workflow {
    Workflow {
        id: "leaving-work".into(),
        name: "Leaving work".into(),
        trigger: Trigger::External {
            key: "left.office".into(),
        },
        steps: vec![
            Step::AskOwner {
                question: "Tell Mom you're on your way?".into(),
            },
            Step::StopUnless {
                condition: Condition::OwnerSaidYes,
            },
            Step::RunSkill {
                skill_id: "text-someone".into(),
                args: args(&[("contact", "Mom"), ("message", "on my way")]),
            },
            Step::StopUnless {
                condition: Condition::LastSucceeded,
            },
            Step::RunTask {
                text: "Turn on Bluetooth".into(),
            },
            Step::Wait { ms: 60_000 },
            Step::Notify {
                text: "Mom knows you're coming.".into(),
            },
        ],
        ..Workflow::default()
    }
}

fn finished(e: &WorkflowEffect) -> &WorkflowOutcome {
    match e {
        WorkflowEffect::Finished { outcome } => outcome,
        other => panic!("not finished: {other:?}"),
    }
}

/// A run plus the skill store it reads at each step.
struct R {
    run: WorkflowRun,
    skills: jarvis_skills::SkillStore,
}

impl R {
    fn new(w: Workflow, skills: &jarvis_skills::SkillStore, limits: RunLimits) -> R {
        R {
            run: WorkflowRun::new(w, limits),
            skills: skills.clone(),
        }
    }

    fn step(&mut self, now: u64, ev: WorkflowEvent) -> Result<WorkflowEffect, RunError> {
        self.run.step(now, ev, &self.skills)
    }
}

impl std::ops::Deref for R {
    type Target = WorkflowRun;
    fn deref(&self) -> &WorkflowRun {
        &self.run
    }
}

fn ok(run: &mut R, now: u64, ev: WorkflowEvent) -> WorkflowEffect {
    run.step(now, ev).unwrap()
}

fn session_done(success: bool, summary: &str) -> WorkflowEvent {
    WorkflowEvent::SessionFinished {
        status: if success {
            OutcomeStatus::Success
        } else {
            OutcomeStatus::Failed
        },
        summary: summary.into(),
    }
}

fn yes() -> WorkflowEvent {
    WorkflowEvent::OwnerAnswered {
        yes: true,
        text: "yes".into(),
    }
}

#[test]
fn a_full_run_emits_each_effect_in_order() {
    let skills = store_with(vec![text_skill()]);
    let mut run = R::new(leaving_work(), &skills, RunLimits::default());
    assert_eq!(run.awaiting(), Awaiting::Start);
    assert_eq!(
        ok(&mut run, 1_000, WorkflowEvent::Start),
        WorkflowEffect::AskOwner {
            question: "Tell Mom you're on your way?".into()
        }
    );
    let e = ok(&mut run, 2_000, yes());
    let WorkflowEffect::StartSession { task, skill } = &e else {
        panic!("{e:?}");
    };
    assert_eq!(task, "text Mom on my way");
    let ctx = skill.as_ref().unwrap();
    assert_eq!(ctx.id, "text-someone");
    assert!(ctx.confine_to_apps);
    assert_eq!(
        ctx.args,
        args(&[("contact", "Mom"), ("message", "on my way")])
    );
    assert_eq!(run.current_step(), Some(2));
    assert_eq!(run.pending(), Some(&e));
    assert_eq!(
        ok(&mut run, 3_000, session_done(true, "Sent")),
        WorkflowEffect::StartSession {
            task: "Turn on Bluetooth".into(),
            skill: None
        }
    );
    assert_eq!(
        ok(&mut run, 4_000, session_done(true, "Bluetooth is on")),
        WorkflowEffect::Wait { until_ms: 64_000 }
    );
    // Too early: the wait is repeated.
    assert_eq!(
        ok(&mut run, 10_000, WorkflowEvent::WaitElapsed),
        WorkflowEffect::Wait { until_ms: 64_000 }
    );
    assert_eq!(
        ok(&mut run, 64_000, WorkflowEvent::WaitElapsed),
        WorkflowEffect::Notify {
            text: "Mom knows you're coming.".into()
        }
    );
    let done = ok(&mut run, 64_100, WorkflowEvent::Notified);
    let o = finished(&done);
    assert_eq!(o.status, WorkflowStatus::Succeeded);
    assert_eq!(o.steps_run, 7);
    assert_eq!(o.failed_step, None);
    assert!(o.summary.contains("Bluetooth is on"), "{}", o.summary);
    // Finished is sticky.
    assert_eq!(ok(&mut run, 70_000, WorkflowEvent::Start), done);
    assert_eq!(run.outcome(), Some(o));
    assert_eq!(run.awaiting(), Awaiting::Nothing);
}

#[test]
fn stop_unless_stops_when_the_condition_fails() {
    let skills = store_with(vec![text_skill()]);
    let mut run = R::new(leaving_work(), &skills, RunLimits::default());
    ok(&mut run, 0, WorkflowEvent::Start);
    let e = ok(
        &mut run,
        1,
        WorkflowEvent::OwnerAnswered {
            yes: false,
            text: "not now".into(),
        },
    );
    let o = finished(&e);
    assert_eq!(o.status, WorkflowStatus::Stopped);
    assert_eq!(o.steps_run, 2);
    assert_eq!(o.failed_step, None);
    assert!(o.summary.contains("step 2"), "{}", o.summary);
}

#[test]
fn summary_condition_is_case_insensitive() {
    let wf = Workflow {
        id: "check".into(),
        name: "Check".into(),
        steps: vec![
            Step::RunTask {
                text: "Check the weather".into(),
            },
            Step::StopUnless {
                condition: Condition::LastSummaryContains {
                    text: "rain".into(),
                },
            },
            Step::Notify {
                text: "Take an umbrella".into(),
            },
        ],
        ..Workflow::default()
    };
    let skills = store_with(vec![]);
    let mut run = R::new(wf.clone(), &skills, RunLimits::default());
    ok(&mut run, 0, WorkflowEvent::Start);
    assert!(matches!(
        ok(&mut run, 1, session_done(true, "RAIN expected at 4pm")),
        WorkflowEffect::Notify { .. }
    ));
    let mut run = R::new(wf, &skills, RunLimits::default());
    ok(&mut run, 0, WorkflowEvent::Start);
    let e = ok(&mut run, 1, session_done(true, "Sunny all day"));
    assert_eq!(finished(&e).status, WorkflowStatus::Stopped);
}

#[test]
fn a_failed_session_stops_or_continues_per_on_failure() {
    let wf = |on_failure| Workflow {
        id: "two".into(),
        name: "Two tasks".into(),
        on_failure,
        steps: vec![
            Step::RunTask {
                text: "Order coffee".into(),
            },
            Step::RunTask {
                text: "Turn on Do Not Disturb".into(),
            },
        ],
        ..Workflow::default()
    };
    let skills = store_with(vec![]);
    let mut run = R::new(wf(OnFailure::Stop), &skills, RunLimits::default());
    ok(&mut run, 0, WorkflowEvent::Start);
    let e = ok(
        &mut run,
        1,
        session_done(false, "Blocked: the owner declined a payment action."),
    );
    let o = finished(&e);
    assert_eq!(o.status, WorkflowStatus::Failed);
    assert_eq!(o.failed_step, Some(0));
    assert_eq!(o.steps_run, 1);
    assert!(o.summary.contains("declined"), "{}", o.summary);

    let mut run = R::new(wf(OnFailure::Continue), &skills, RunLimits::default());
    ok(&mut run, 0, WorkflowEvent::Start);
    assert!(matches!(
        ok(&mut run, 1, session_done(false, "")),
        WorkflowEffect::StartSession { .. }
    ));
    let e = ok(&mut run, 2, session_done(true, "DND on"));
    let o = finished(&e);
    assert_eq!(o.status, WorkflowStatus::CompletedWithFailures);
    assert_eq!(o.failed_step, Some(0));
    assert_eq!(o.steps_run, 2);
}

#[test]
fn missing_disabled_or_underfilled_skills_fail_their_step_with_a_reason() {
    let mut disabled = text_skill();
    disabled.id = "text-disabled".into();
    disabled.enabled = false;
    let skills = store_with(vec![text_skill(), disabled]);
    for (skill_id, a, needle) in [
        ("no-such-skill", args(&[]), "does not exist"),
        (
            "text-disabled",
            args(&[("contact", "Mom"), ("message", "hi")]),
            "disabled",
        ),
        ("text-someone", args(&[("contact", "Mom")]), "needs message"),
    ] {
        let wf = Workflow {
            id: "w".into(),
            name: "W".into(),
            steps: vec![Step::RunSkill {
                skill_id: skill_id.into(),
                args: a,
            }],
            ..Workflow::default()
        };
        let mut run = R::new(wf, &skills, RunLimits::default());
        let e = ok(&mut run, 0, WorkflowEvent::Start);
        let o = finished(&e);
        assert_eq!(o.status, WorkflowStatus::Failed, "{skill_id}");
        assert_eq!(o.failed_step, Some(0));
        assert!(o.summary.contains(needle), "{skill_id}: {}", o.summary);
    }
}

#[test]
fn unexpected_events_are_typed_errors_that_leave_the_run_unchanged() {
    let skills = store_with(vec![text_skill()]);
    let mut run = R::new(leaving_work(), &skills, RunLimits::default());
    assert_eq!(
        run.step(0, WorkflowEvent::WaitElapsed),
        Err(RunError::UnexpectedEvent {
            got: "wait_elapsed",
            expected: "start"
        })
    );
    assert_eq!(run.awaiting(), Awaiting::Start);
    let ask = ok(&mut run, 0, WorkflowEvent::Start);
    for bad in [
        WorkflowEvent::Start,
        session_done(true, "x"),
        WorkflowEvent::Notified,
        WorkflowEvent::WaitElapsed,
    ] {
        let err = run.step(5, bad.clone()).unwrap_err();
        assert!(err.to_string().contains("owner_answered"), "{err}");
        assert_eq!(run.awaiting(), Awaiting::Answer);
        assert_eq!(run.pending(), Some(&ask));
    }
    // The right answer still works afterwards.
    assert!(matches!(
        ok(&mut run, 6, yes()),
        WorkflowEffect::StartSession { .. }
    ));
    // Even past the deadline, a wrong event does not change the state.
    let late = RunLimits::default().max_total_ms + 10;
    assert!(run.step(late, yes()).is_err());
    assert_eq!(run.awaiting(), Awaiting::Session);
}

#[test]
fn cancel_is_honoured_everywhere() {
    let skills = store_with(vec![text_skill()]);
    let mut before = R::new(leaving_work(), &skills, RunLimits::default());
    let e = ok(&mut before, 0, WorkflowEvent::Cancel);
    assert_eq!(finished(&e).status, WorkflowStatus::Cancelled);
    assert_eq!(finished(&e).steps_run, 0);
    let mut during = R::new(leaving_work(), &skills, RunLimits::default());
    ok(&mut during, 0, WorkflowEvent::Start);
    ok(&mut during, 1, yes());
    let e = ok(&mut during, 2, WorkflowEvent::Cancel);
    assert_eq!(finished(&e).status, WorkflowStatus::Cancelled);
    assert_eq!(finished(&e).steps_run, 3);
}

#[test]
fn limits_bound_steps_time_and_waits() {
    let skills = store_with(vec![text_skill()]);
    let tight = RunLimits {
        max_steps: 3,
        ..RunLimits::default()
    };
    let mut run = R::new(leaving_work(), &skills, tight);
    let e = ok(&mut run, 0, WorkflowEvent::Start);
    assert_eq!(finished(&e).status, WorkflowStatus::Failed);
    assert!(finished(&e).summary.contains("more than 3 steps"));

    let short_wait = RunLimits {
        max_wait_ms: 30_000,
        ..RunLimits::default()
    };
    let mut run = R::new(leaving_work(), &skills, short_wait);
    ok(&mut run, 0, WorkflowEvent::Start);
    ok(&mut run, 1, yes());
    ok(&mut run, 2, session_done(true, "Sent"));
    let e = ok(&mut run, 3, session_done(true, "On"));
    assert_eq!(finished(&e).failed_step, Some(5));
    assert!(
        finished(&e).summary.contains("longer than"),
        "{}",
        finished(&e).summary
    );

    let short_total = RunLimits {
        max_total_ms: 10_000,
        ..RunLimits::default()
    };
    let mut run = R::new(leaving_work(), &skills, short_total);
    ok(&mut run, 0, WorkflowEvent::Start);
    let e = ok(&mut run, 10_001, yes());
    assert_eq!(finished(&e).status, WorkflowStatus::Failed);
    assert!(finished(&e).summary.contains("out of time"));
}

#[test]
fn invalid_or_disabled_workflows_fail_at_start() {
    let skills = store_with(vec![]);
    let mut wf = leaving_work();
    wf.enabled = false;
    let e = R::new(wf, &skills, RunLimits::default()).step(0, WorkflowEvent::Start);
    assert!(finished(&e.unwrap()).summary.contains("disabled"));
    let wf = Workflow {
        id: "Bad".into(),
        ..leaving_work()
    };
    let e = R::new(wf, &skills, RunLimits::default())
        .step(0, WorkflowEvent::Start)
        .unwrap();
    assert!(finished(&e).summary.contains("not valid"));
}

#[test]
fn workflow_validation_enforces_limits() {
    leaving_work().validate().unwrap();
    let base = leaving_work();
    let bad = [
        Workflow {
            steps: vec![],
            ..base.clone()
        },
        Workflow {
            steps: vec![Step::Notify { text: "x".into() }; 25],
            ..base.clone()
        },
        Workflow {
            steps: vec![Step::Wait { ms: 0 }],
            ..base.clone()
        },
        Workflow {
            steps: vec![Step::Wait { ms: 3_600_001 }],
            ..base.clone()
        },
        Workflow {
            steps: vec![Step::RunTask { text: " ".into() }],
            ..base.clone()
        },
        Workflow {
            steps: vec![Step::RunTask {
                text: "t".repeat(501),
            }],
            ..base.clone()
        },
        Workflow {
            steps: vec![Step::RunSkill {
                skill_id: "Nope!".into(),
                args: args(&[]),
            }],
            ..base.clone()
        },
        Workflow {
            steps: vec![Step::RunSkill {
                skill_id: "ok".into(),
                args: args(&[("Bad", "x")]),
            }],
            ..base.clone()
        },
        Workflow {
            trigger: Trigger::Phrase {
                text: "please".into(),
            },
            ..base.clone()
        },
        Workflow {
            trigger: Trigger::External {
                key: "Has Space".into(),
            },
            ..base.clone()
        },
        Workflow {
            trigger: Trigger::Schedule(Schedule {
                days: BTreeSet::new(),
                hour: 7,
                minute: 0,
            }),
            ..base.clone()
        },
        Workflow {
            trigger: Trigger::Schedule(Schedule {
                days: [Weekday::Mon].into(),
                hour: 24,
                minute: 0,
            }),
            ..base.clone()
        },
        Workflow {
            name: String::new(),
            ..base.clone()
        },
    ];
    for w in bad {
        assert!(w.validate().is_err(), "{w:?}");
    }
    Workflow {
        steps: vec![Step::Notify { text: "x".into() }; 24],
        ..base
    }
    .validate()
    .unwrap();
}

fn weekday_morning() -> Workflow {
    Workflow {
        id: "morning".into(),
        name: "Morning".into(),
        trigger: Trigger::Schedule(Schedule {
            days: [
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
            ]
            .into(),
            hour: 7,
            minute: 30,
        }),
        steps: vec![Step::RunSkill {
            skill_id: "wifi-off".into(),
            args: args(&[]),
        }],
        ..Workflow::default()
    }
}

#[test]
fn store_crud_json_and_lookups() {
    let skills = store_with(vec![wifi_skill()]);
    let mut s = WorkflowStore::new();
    s.add(leaving_work()).unwrap();
    s.add(weekday_morning()).unwrap();
    s.add(Workflow {
        id: "bedtime".into(),
        name: "Bedtime".into(),
        trigger: Trigger::Phrase {
            text: "Good night!".into(),
        },
        steps: vec![Step::RunTask {
            text: "Turn on Do Not Disturb".into(),
        }],
        ..Workflow::default()
    })
    .unwrap();
    assert_eq!(
        s.add(weekday_morning()),
        Err(SkillError::DuplicateId("morning".into()))
    );
    let ids: Vec<&str> = s.list().iter().map(|w| w.id.as_str()).collect();
    assert_eq!(ids, vec!["bedtime", "leaving-work", "morning"]);

    // Warnings, not errors, for skills that do not exist (text-someone is not in `skills`).
    let warnings = WorkflowStore::warnings(&leaving_work(), &skills);
    assert_eq!(
        warnings,
        vec!["step 3: there is no skill \"text-someone\"".to_owned()]
    );
    assert!(WorkflowStore::warnings(&weekday_morning(), &skills).is_empty());
    let with_text = store_with(vec![text_skill()]);
    let mut wf = leaving_work();
    wf.steps[2] = Step::RunSkill {
        skill_id: "text-someone".into(),
        args: args(&[("contact", "Mom"), ("colour", "red")]),
    };
    let w = WorkflowStore::warnings(&wf, &with_text);
    assert_eq!(
        w,
        vec!["step 3: the skill \"text-someone\" has no parameter \"colour\"".to_owned()]
    );
    wf.steps[2] = Step::RunSkill {
        skill_id: "text-someone".into(),
        args: args(&[("contact", "Mom")]),
    };
    let w = WorkflowStore::warnings(&wf, &with_text);
    assert_eq!(
        w,
        vec!["step 3: the skill \"text-someone\" needs message".to_owned()]
    );

    // Lookups.
    let names = |v: Vec<&Workflow>| v.iter().map(|w| w.id.clone()).collect::<Vec<_>>();
    assert_eq!(names(s.match_phrase("good night")), vec!["bedtime"]);
    assert_eq!(
        names(s.match_phrase("Hey Jarvis, good night please")),
        vec!["bedtime"]
    );
    assert!(s.match_phrase("good morning").is_empty());
    assert!(s.match_phrase(&"good night ".repeat(100_000)).is_empty());
    assert_eq!(names(s.for_external("left.office")), vec!["leaving-work"]);
    assert_eq!(names(s.for_external(" LEFT.OFFICE ")), vec!["leaving-work"]);
    assert!(s.for_external("arrived.home").is_empty());
    assert_eq!(
        names(s.due(Weekday::Mon, 7 * 60 + 30, 1).unwrap()),
        vec!["morning"]
    );
    assert!(s.due(Weekday::Mon, 7 * 60 + 31, 1).unwrap().is_empty());
    assert_eq!(
        names(s.due(Weekday::Mon, 7 * 60 + 45, 20).unwrap()),
        vec!["morning"]
    );
    assert!(s.due(Weekday::Sat, 7 * 60 + 30, 1).unwrap().is_empty());
    let (next, minutes) = s.next_scheduled(Weekday::Fri, 8 * 60).unwrap().unwrap();
    assert_eq!(next.id, "morning");
    assert_eq!(minutes, 3 * MINUTES_PER_DAY - 30);

    // Disabled workflows are never found.
    let mut off = weekday_morning();
    off.enabled = false;
    s.update(off, 10).unwrap();
    assert!(s.due(Weekday::Mon, 7 * 60 + 30, 1).unwrap().is_empty());
    assert!(s.next_scheduled(Weekday::Mon, 0).unwrap().is_none());

    // Bookkeeping and the snapshot.
    let outcome = WorkflowOutcome {
        status: WorkflowStatus::Succeeded,
        steps_run: 1,
        summary: "Done.".into(),
        failed_step: None,
    };
    s.record_run("bedtime", 99, &outcome).unwrap();
    assert_eq!(
        s.record_run("nope", 1, &outcome),
        Err(SkillError::NotFound("nope".into()))
    );
    let bed = s.get("bedtime").unwrap();
    assert_eq!((bed.runs, bed.last_run_ms), (1, Some(99)));
    let mut renamed = bed.clone();
    renamed.name = "Bed".into();
    renamed.runs = 0;
    renamed.last_outcome = None;
    s.update(renamed, 11).unwrap();
    assert_eq!(s.get("bedtime").unwrap().last_outcome, Some(outcome));
    let json = s.to_json().unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["workflows"][2]["trigger"]["type"], "schedule");
    assert_eq!(v["workflows"][2]["trigger"]["days"][0], "mon");
    assert_eq!(
        v["workflows"][1]["steps"][1]["condition"]["type"],
        "owner_said_yes"
    );
    assert_eq!(WorkflowStore::from_json(&json).unwrap(), s);
    assert!(s.remove("bedtime").is_some());
    assert_eq!(s.len(), 2);
}

/// Review pass 35 (low): a stored workflow without `enabled` loads disabled, so a scheduled
/// one never starts running unattended because the field was lost; one that says so stays
/// enabled, and code building on `Workflow::default()` gets an enabled workflow.
#[test]
fn a_stored_workflow_without_enabled_loads_disabled() {
    let json = r#"{"schema_version":1,"workflows":[
        {"id":"m","name":"M","trigger":{"type":"schedule","days":["mon"],"hour":7,"minute":30},
         "steps":[{"type":"notify","text":"hi"}]},
        {"id":"e","name":"E","enabled":true,"steps":[{"type":"notify","text":"hi"}]}
    ]}"#;
    let (s, issues) = WorkflowStore::from_json_with_issues(json).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert!(!s.get("m").unwrap().enabled);
    assert!(s.get("e").unwrap().enabled);
    let w: Workflow = serde_json::from_str(r#"{"id":"w","name":"W"}"#).unwrap();
    assert!(!w.enabled);
    assert!(Workflow::default().enabled);
}

#[test]
fn workflow_snapshots_tolerate_unknown_fields_and_reject_corruption() {
    let json = r#"{"schema_version":1,"extra":true,"workflows":[
        {"id":"w","name":"W","steps":[{"type":"notify","text":"hi","icon":"bell"}],"colour":"red"},
        {"id":"x","name":"X","steps":[{"type":"teleport"}]},
        {"id":"w","name":"Dup","steps":[{"type":"notify","text":"hi"}]}
    ]}"#;
    let (s, issues) = WorkflowStore::from_json_with_issues(json).unwrap();
    assert_eq!(s.len(), 1);
    assert_eq!(s.get("w").unwrap().name, "W");
    assert_eq!(issues.len(), 4, "{issues:?}");
    assert_eq!(issues[0], "snapshot: unknown field extra ignored");
    assert_eq!(
        issues[1],
        "workflows[0] \"w\" (W): unknown field colour ignored"
    );
    assert!(issues[2].starts_with("workflows[1] \"x\" (X): not loaded, kept unchanged"));
    assert!(s.has_load_issues());
    // Nothing is lost on save: both unreadable entries come back, and are reported again.
    let (again, issues2) = WorkflowStore::from_json_with_issues(&s.to_json().unwrap()).unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(issues2.len(), 2, "{issues2:?}");
    assert!(again.to_json().unwrap().contains("teleport"));
    for bad in ["", "[]", r#"{"workflows":{}}"#, "{"] {
        assert!(WorkflowStore::from_json(bad).is_err(), "{bad:?}");
    }
    assert!(matches!(
        WorkflowStore::from_json(r#"{"schema_version":9}"#),
        Err(SkillError::NewerSchema { found: 9, .. })
    ));
}

// ---------------------------------------------------------------------------------------
// Schedules: property-style checks over every start minute of the week.
// ---------------------------------------------------------------------------------------

/// Whether minute `m` of the week (0 = Monday 00:00) is a scheduled minute.
fn fires_at(s: &Schedule, m: u32) -> bool {
    let day = Weekday::ALL[(m / MINUTES_PER_DAY) as usize];
    s.days.contains(&day) && m % MINUTES_PER_DAY == u32::from(s.hour) * 60 + u32::from(s.minute)
}

fn schedules() -> Vec<Schedule> {
    vec![
        Schedule {
            days: Weekday::ALL.into_iter().collect(),
            hour: 0,
            minute: 0,
        },
        Schedule {
            days: Weekday::ALL.into_iter().collect(),
            hour: 23,
            minute: 59,
        },
        Schedule {
            days: [Weekday::Mon, Weekday::Wed, Weekday::Fri].into(),
            hour: 7,
            minute: 30,
        },
        Schedule {
            days: [Weekday::Sun].into(),
            hour: 12,
            minute: 0,
        },
        Schedule {
            days: [Weekday::Sat, Weekday::Sun].into(),
            hour: 9,
            minute: 15,
        },
    ]
}

#[test]
fn minutes_until_next_is_the_first_firing_minute_ahead_for_every_start_point() {
    for s in schedules() {
        for start in 0..MINUTES_PER_WEEK {
            let weekday = Weekday::ALL[(start / MINUTES_PER_DAY) as usize];
            let mod_day = start % MINUTES_PER_DAY;
            let d = minutes_until_next(&s, weekday, mod_day).unwrap();
            assert!(d < MINUTES_PER_WEEK, "{s:?} at {start}: {d}");
            assert!(
                fires_at(&s, (start + d) % MINUTES_PER_WEEK),
                "{s:?} at {start}: +{d}"
            );
            // Nothing fires earlier.
            for e in 0..d.min(MINUTES_PER_DAY + 2) {
                assert!(
                    !fires_at(&s, (start + e) % MINUTES_PER_WEEK),
                    "{s:?} at {start}: +{e}"
                );
            }
            let back = minutes_since_last(&s, weekday, mod_day).unwrap();
            assert!(back < MINUTES_PER_WEEK);
            assert!(fires_at(
                &s,
                (start + MINUTES_PER_WEEK - back) % MINUTES_PER_WEEK
            ));
            assert_eq!(d == 0, back == 0);
            assert_eq!(is_due(&s, weekday, mod_day, 1).unwrap(), d == 0);
        }
    }
}

#[test]
fn next_and_last_are_consistent_across_the_week() {
    for s in schedules() {
        // Walking forward d minutes then asking "since last" gives 0, and one minute later 1.
        for start in (0..MINUTES_PER_WEEK).step_by(7) {
            let weekday = Weekday::ALL[(start / MINUTES_PER_DAY) as usize];
            let d = minutes_until_next(&s, weekday, start % MINUTES_PER_DAY).unwrap();
            let at = (start + d) % MINUTES_PER_WEEK;
            let w2 = Weekday::ALL[(at / MINUTES_PER_DAY) as usize];
            assert_eq!(minutes_since_last(&s, w2, at % MINUTES_PER_DAY).unwrap(), 0);
            let after = (at + 1) % MINUTES_PER_WEEK;
            let w3 = Weekday::ALL[(after / MINUTES_PER_DAY) as usize];
            let since = minutes_since_last(&s, w3, after % MINUTES_PER_DAY).unwrap();
            assert!(since == 1 || fires_at(&s, after), "{s:?} {after}: {since}");
        }
    }
}

/// Review finding w8: out-of-range input is a typed error, not a clamp.
#[test]
fn out_of_range_schedules_and_times_are_errors() {
    let never = Schedule::default();
    assert!(matches!(
        minutes_until_next(&never, Weekday::Mon, 0),
        Err(SkillError::Schedule(_))
    ));
    assert!(is_due(&never, Weekday::Mon, 0, 10_000).is_err());
    let late = &schedules()[1];
    assert!(minutes_until_next(late, Weekday::Mon, 1_440).is_err());
    assert!(minutes_since_last(late, Weekday::Mon, 5_000).is_err());
    let bad = Schedule {
        days: [Weekday::Mon].into(),
        hour: 24,
        minute: 0,
    };
    assert!(minutes_until_next(&bad, Weekday::Mon, 0).is_err());
    let bad = Schedule {
        days: [Weekday::Mon].into(),
        hour: 7,
        minute: 60,
    };
    assert!(minutes_since_last(&bad, Weekday::Mon, 0).is_err());
    assert_eq!(Weekday::from_index(6).unwrap(), Weekday::Sun);
    assert!(Weekday::from_index(7).is_err());
    let mut store = WorkflowStore::new();
    store.add(weekday_morning()).unwrap();
    assert!(store.due(Weekday::Mon, 1_440, 1).is_err());
    assert!(store.next_scheduled(Weekday::Mon, 99_999).is_err());
    // A window of 0 is never due.
    assert!(!is_due(&schedules()[0], Weekday::Mon, 0, 0).unwrap());
}
