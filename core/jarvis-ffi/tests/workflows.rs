//! `JarvisWorkflows` and `JarvisWorkflowRun` from Rust: store round trips with damaged
//! entries, warnings against the live skill store, a run driven end to end (protocol
//! errors leave it unchanged, a blocked session stops it), schedules and utterance
//! resolution.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::HashMap;
use std::sync::Arc;

use jarvis_core_ffi::*;

fn wifi_skill() -> Skill {
    Skill {
        id: "wifi-off".into(),
        name: "Wi-Fi off".into(),
        summary: String::new(),
        kind: SkillKind::Written,
        triggers: vec!["turn off wi-fi".into()],
        instructions: "Use the switch on the Wi-Fi page.".into(),
        apps: vec!["com.apple.Preferences".into()],
        confine_to_apps: true,
        params: vec![],
        flow_keys: vec![],
        enabled: true,
        created_ms: 1,
        updated_ms: 1,
        runs: 0,
        last_run_ms: None,
        tools: vec![],
    }
}

fn skills() -> Arc<JarvisSkills> {
    let s = JarvisSkills::new(String::new()).unwrap();
    assert!(s.upsert(wifi_skill()).unwrap());
    s
}

fn morning() -> Workflow {
    Workflow {
        id: "morning".into(),
        name: "Morning".into(),
        enabled: true,
        trigger: Trigger::Phrase {
            text: "good morning".into(),
        },
        steps: vec![
            Step::RunSkill {
                skill_id: "wifi-off".into(),
                args: HashMap::new(),
            },
            Step::AskOwner {
                question: "Read the news?".into(),
            },
            Step::StopUnless {
                condition: Condition::OwnerSaidYes,
            },
            Step::Notify {
                text: "Reading the news".into(),
            },
            Step::Wait { ms: 1_000 },
            Step::RunTask {
                text: "Open the news app".into(),
            },
        ],
        on_failure: OnFailure::Stop,
        created_ms: 0,
        updated_ms: 0,
        runs: 0,
        last_run_ms: None,
        last_outcome: None,
    }
}

fn scheduled(id: &str, days: Vec<Weekday>, hour: u8, minute: u8) -> Workflow {
    Workflow {
        id: id.into(),
        name: id.into(),
        trigger: Trigger::Schedule {
            schedule: Schedule { days, hour, minute },
        },
        steps: vec![Step::Notify { text: "hi".into() }],
        ..morning()
    }
}

#[test]
fn store_crud_issues_and_lookups() {
    let ws = JarvisWorkflows::new(String::new()).unwrap();
    assert!(ws.list().unwrap().is_empty() && !ws.has_load_issues().unwrap());
    assert!(ws.upsert(morning(), 10).unwrap(), "new");
    let stored = ws.get("morning".into()).unwrap().unwrap();
    assert_eq!((stored.created_ms, stored.updated_ms), (10, 10));
    assert!(!ws.upsert(morning(), 20).unwrap(), "update");
    assert_eq!(ws.get("morning".into()).unwrap().unwrap().created_ms, 10);

    let mut bad = morning();
    bad.steps.clear();
    let err = ws.upsert(bad, 30).unwrap_err();
    assert!(
        matches!(&err, JarvisError::InvalidWorkflow { message } if message.contains("steps")),
        "{err:?}"
    );

    // Phrase and external lookups; disabling hides it.
    assert_eq!(ws.match_phrase("Good morning!".into()).unwrap().len(), 1);
    let mut ext = morning();
    ext.id = "home".into();
    ext.trigger = Trigger::External {
        key: "arrive.home".into(),
    };
    ws.upsert(ext, 40).unwrap();
    assert_eq!(
        ws.for_external(" ARRIVE.HOME ".into()).unwrap()[0].id,
        "home"
    );
    ws.set_enabled("morning".into(), false, 50).unwrap();
    assert!(ws.match_phrase("good morning".into()).unwrap().is_empty());
    let w = ws.get("morning".into()).unwrap().unwrap();
    assert!(!w.enabled && w.updated_ms == 50 && w.created_ms == 10);
    assert!(matches!(
        ws.set_enabled("nope".into(), true, 1).unwrap_err(),
        JarvisError::InvalidWorkflow { .. }
    ));

    // Record a run; it survives a JSON round trip.
    let outcome = WorkflowOutcome {
        status: WorkflowStatus::Succeeded,
        steps_run: 6,
        summary: "Done.".into(),
        failed_step: None,
    };
    ws.record_run("home".into(), 99, outcome.clone()).unwrap();
    let back = JarvisWorkflows::new(ws.to_json().unwrap()).unwrap();
    let home = back.get("home".into()).unwrap().unwrap();
    assert_eq!(
        (home.runs, home.last_run_ms, home.last_outcome),
        (1, Some(99), Some(outcome))
    );
    assert_eq!(back.list().unwrap(), ws.list().unwrap());
    assert!(back.remove("home".into()).unwrap());
    assert!(!back.remove("home".into()).unwrap());
}

#[test]
fn damaged_entries_are_reported_preserved_and_removable() {
    let json = r#"{"schema_version":1,"workflows":[{"id":"kept","name":"Kept","steps":[]},
        {"id":"ok","name":"Ok","steps":[{"type":"notify","text":"hi"}],"colour":"red"}]}"#;
    let ws = JarvisWorkflows::new(json.into()).unwrap();
    assert!(ws.has_load_issues().unwrap());
    let issues = ws.load_issues().unwrap();
    assert!(issues.iter().any(|i| i.contains("colour")), "{issues:?}");
    assert!(issues.iter().any(|i| i.contains("kept")), "{issues:?}");
    assert_eq!(ws.damaged_ids().unwrap(), vec![Some("kept".to_owned())]);
    assert_eq!(ws.list().unwrap().len(), 1);
    // Saved unchanged, so a later build can read it.
    let saved = ws.to_json().unwrap();
    assert!(saved.contains("\"kept\""));
    let again = JarvisWorkflows::new(saved).unwrap();
    assert_eq!(again.damaged_ids().unwrap().len(), 1);
    // Removing it discards the damaged entry.
    assert!(again.remove("kept".into()).unwrap());
    assert!(again.damaged_ids().unwrap().is_empty());
    assert!(!again.to_json().unwrap().contains("\"kept\""));
    // Structural damage throws.
    assert!(matches!(
        JarvisWorkflows::new("{broken".into()).err().unwrap(),
        JarvisError::InvalidWorkflow { .. }
    ));
    assert!(matches!(
        JarvisWorkflows::new(r#"{"schema_version":99,"workflows":[]}"#.into())
            .err()
            .unwrap(),
        JarvisError::InvalidWorkflow { .. }
    ));
}

#[test]
fn warnings_read_the_live_skill_store() {
    let ws = JarvisWorkflows::new(String::new()).unwrap();
    let sk = skills();
    assert!(ws.warnings(morning(), Arc::clone(&sk)).unwrap().is_empty());
    let mut w = morning();
    w.steps.push(Step::RunSkill {
        skill_id: "missing".into(),
        args: HashMap::new(),
    });
    let warnings = ws.warnings(w, Arc::clone(&sk)).unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("missing"), "{warnings:?}");
    // Disabling the skill shows up at once.
    let mut off = wifi_skill();
    off.enabled = false;
    sk.upsert(off).unwrap();
    assert_eq!(ws.warnings(morning(), sk).unwrap().len(), 1);
}

fn session_finished(status: OutcomeStatus, summary: &str) -> WorkflowEvent {
    WorkflowEvent::SessionFinished {
        status,
        summary: summary.into(),
    }
}

#[test]
fn a_run_end_to_end_with_a_protocol_error_that_changes_nothing() {
    let sk = skills();
    let run = JarvisWorkflowRun::new(morning(), default_run_limits().unwrap()).unwrap();
    assert_eq!(run.awaiting().unwrap(), Awaiting::Start);
    assert!(run.pending().unwrap().is_none());

    let WorkflowEffect::StartSession { task, skill } = run
        .step(1_000, WorkflowEvent::Start, Arc::clone(&sk))
        .unwrap()
    else {
        panic!("skill step first")
    };
    let skill = skill.expect("a skill step carries its SkillContext");
    assert_eq!(
        (skill.id.as_str(), skill.confine_to_apps),
        ("wifi-off", true)
    );
    assert_eq!(task, "turn off wi-fi");

    // Protocol misuse: typed, and the run is exactly as before.
    let before = (
        run.awaiting().unwrap(),
        run.pending().unwrap(),
        run.current_step().unwrap(),
    );
    let err = run
        .step(1_001, WorkflowEvent::Notified, Arc::clone(&sk))
        .unwrap_err();
    assert_eq!(
        err,
        JarvisError::WorkflowProtocol {
            got: "notified".into(),
            expected: "session_finished".into()
        }
    );
    // A rejected event does not advance the clock either: a far-future protocol error
    // must not use up the time budget.
    assert!(run
        .step(99_999_999, WorkflowEvent::Start, Arc::clone(&sk))
        .is_err());
    assert_eq!(
        (
            run.awaiting().unwrap(),
            run.pending().unwrap(),
            run.current_step().unwrap()
        ),
        before
    );

    let eff = run
        .step(
            2_000,
            session_finished(OutcomeStatus::Success, "Wi-Fi is off"),
            Arc::clone(&sk),
        )
        .unwrap();
    assert_eq!(
        eff,
        WorkflowEffect::AskOwner {
            question: "Read the news?".into()
        }
    );
    let eff = run
        .step(
            3_000,
            WorkflowEvent::OwnerAnswered {
                yes: true,
                text: String::new(),
            },
            Arc::clone(&sk),
        )
        .unwrap();
    assert_eq!(
        eff,
        WorkflowEffect::Notify {
            text: "Reading the news".into()
        }
    );
    let eff = run
        .step(3_100, WorkflowEvent::Notified, Arc::clone(&sk))
        .unwrap();
    assert_eq!(eff, WorkflowEffect::Wait { until_ms: 4_100 });
    // Too early: the same wait again.
    let eff = run
        .step(3_500, WorkflowEvent::WaitElapsed, Arc::clone(&sk))
        .unwrap();
    assert_eq!(eff, WorkflowEffect::Wait { until_ms: 4_100 });
    let eff = run
        .step(4_200, WorkflowEvent::WaitElapsed, Arc::clone(&sk))
        .unwrap();
    assert_eq!(
        eff,
        WorkflowEffect::StartSession {
            task: "Open the news app".into(),
            skill: None
        }
    );
    assert_eq!(run.current_step().unwrap(), Some(5));
    let WorkflowEffect::Finished { outcome } = run
        .step(
            5_000,
            session_finished(OutcomeStatus::Success, "News open"),
            Arc::clone(&sk),
        )
        .unwrap()
    else {
        panic!("last step done")
    };
    assert_eq!(outcome.status, WorkflowStatus::Succeeded);
    assert_eq!(outcome.steps_run, 6);
    assert!(run.is_finished().unwrap());
    assert_eq!(run.awaiting().unwrap(), Awaiting::Nothing);
    assert_eq!(run.outcome().unwrap(), Some(outcome.clone()));
    // Finished is sticky, and finish() returns the same outcome.
    assert_eq!(
        run.step(6_000, WorkflowEvent::Cancel, Arc::clone(&sk))
            .unwrap(),
        WorkflowEffect::Finished {
            outcome: outcome.clone()
        }
    );
    assert_eq!(run.finish(7_000).unwrap(), outcome);

    let ws = JarvisWorkflows::new(String::new()).unwrap();
    ws.upsert(morning(), 1).unwrap();
    ws.record_run("morning".into(), 5_000, outcome).unwrap();
    assert_eq!(ws.get("morning".into()).unwrap().unwrap().runs, 1);
}

#[test]
fn a_blocked_session_stops_the_run_whatever_on_failure_says() {
    let sk = skills();
    let mut w = morning();
    w.on_failure = OnFailure::Continue;
    let run = JarvisWorkflowRun::new(w, default_run_limits().unwrap()).unwrap();
    run.step(1, WorkflowEvent::Start, Arc::clone(&sk)).unwrap();
    let WorkflowEffect::Finished { outcome } = run
        .step(
            2,
            session_finished(OutcomeStatus::Blocked, "The owner declined"),
            sk,
        )
        .unwrap()
    else {
        panic!("blocked ends the run")
    };
    assert_eq!(outcome.status, WorkflowStatus::Blocked);
    assert_eq!(outcome.failed_step, Some(0));
    assert_eq!(outcome.steps_run, 1);
    assert!(outcome.summary.contains("blocked"), "{}", outcome.summary);
}

#[test]
fn a_missing_skill_fails_its_step_and_finish_cancels() {
    let empty = JarvisSkills::new(String::new()).unwrap();
    let run = JarvisWorkflowRun::new(morning(), default_run_limits().unwrap()).unwrap();
    let WorkflowEffect::Finished { outcome } = run
        .step(1, WorkflowEvent::Start, Arc::clone(&empty))
        .unwrap()
    else {
        panic!("a missing skill fails the step, on_failure stop ends the run")
    };
    assert_eq!(outcome.status, WorkflowStatus::Failed);
    assert!(
        outcome.summary.contains("does not exist"),
        "{}",
        outcome.summary
    );

    // finish() on a running run cancels it; on a fresh one too.
    let run = JarvisWorkflowRun::new(morning(), default_run_limits().unwrap()).unwrap();
    run.step(1, WorkflowEvent::Start, skills()).unwrap();
    let out = run.finish(2).unwrap();
    assert_eq!(out.status, WorkflowStatus::Cancelled);
    assert_eq!(run.finish(3).unwrap(), out, "idempotent");
    assert_eq!(run.workflow().unwrap(), morning());

    // A disabled workflow refuses to run.
    let mut off = morning();
    off.enabled = false;
    let run = JarvisWorkflowRun::new(off, default_run_limits().unwrap()).unwrap();
    let WorkflowEffect::Finished { outcome } = run.step(1, WorkflowEvent::Start, empty).unwrap()
    else {
        panic!()
    };
    assert_eq!(outcome.status, WorkflowStatus::Failed);
}

#[test]
fn schedules_due_next_and_typed_range_errors() {
    let ws = JarvisWorkflows::new(String::new()).unwrap();
    ws.upsert(scheduled("a-early", vec![Weekday::Mon], 7, 30), 1)
        .unwrap();
    ws.upsert(
        scheduled(
            "b-late",
            vec![Weekday::Mon, Weekday::Mon, Weekday::Tue],
            9,
            0,
        ),
        1,
    )
    .unwrap();
    // Days are a set, Monday first.
    assert_eq!(
        ws.get("b-late".into()).unwrap().unwrap().trigger,
        Trigger::Schedule {
            schedule: Schedule {
                days: vec![Weekday::Mon, Weekday::Tue],
                hour: 9,
                minute: 0
            }
        }
    );
    let due = ws.due(Weekday::Mon, 7 * 60 + 30, 1).unwrap();
    assert_eq!(
        due.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(),
        ["a-early"]
    );
    assert!(ws.due(Weekday::Mon, 7 * 60 + 31, 1).unwrap().is_empty());
    assert_eq!(ws.due(Weekday::Mon, 7 * 60 + 35, 10).unwrap().len(), 1);
    let next = ws.next_scheduled(Weekday::Mon, 8 * 60).unwrap().unwrap();
    assert_eq!(
        (next.workflow.id.as_str(), next.minutes_until),
        ("b-late", 60)
    );
    assert!(JarvisWorkflows::new(String::new())
        .unwrap()
        .next_scheduled(Weekday::Sun, 0)
        .unwrap()
        .is_none());

    for err in [
        ws.due(Weekday::Mon, 1_440, 1).unwrap_err(),
        ws.next_scheduled(Weekday::Mon, 5_000).unwrap_err(),
        weekday_from_index(7).unwrap_err(),
        schedule_minutes_until_next(
            Schedule {
                days: vec![],
                hour: 1,
                minute: 0,
            },
            Weekday::Mon,
            0,
        )
        .unwrap_err(),
        schedule_is_due(
            Schedule {
                days: vec![Weekday::Mon],
                hour: 24,
                minute: 0,
            },
            Weekday::Mon,
            0,
            1,
        )
        .unwrap_err(),
    ] {
        assert!(
            matches!(err, JarvisError::InvalidSchedule { .. }),
            "{err:?}"
        );
    }
    assert_eq!(weekday_from_index(0).unwrap(), Weekday::Mon);
    assert_eq!(weekday_from_index(6).unwrap(), Weekday::Sun);
    let s = Schedule {
        days: vec![Weekday::Wed],
        hour: 12,
        minute: 0,
    };
    assert_eq!(
        schedule_minutes_until_next(s.clone(), Weekday::Tue, 12 * 60).unwrap(),
        1_440
    );
    assert_eq!(
        schedule_minutes_since_last(s.clone(), Weekday::Wed, 12 * 60 + 5).unwrap(),
        5
    );
    assert!(schedule_is_due(s, Weekday::Wed, 12 * 60, 1).unwrap());
    let limits = default_run_limits().unwrap();
    assert_eq!(
        (limits.max_steps, limits.max_total_ms, limits.max_wait_ms),
        (24, 3_600_000, 1_800_000)
    );
}

#[test]
fn resolve_utterance_precedence() {
    let sk = skills();
    let ws = JarvisWorkflows::new(String::new()).unwrap();
    ws.upsert(morning(), 1).unwrap();

    let m = resolve_utterance(Arc::clone(&sk), Arc::clone(&ws), "good morning".into()).unwrap();
    assert_eq!(m.workflows, vec!["morning".to_owned()]);
    assert_eq!(
        m.resolution,
        Resolution::RunWorkflow {
            workflow_id: "morning".into()
        }
    );
    assert!(!m.needs_owner_choice);

    let m = resolve_utterance(Arc::clone(&sk), Arc::clone(&ws), "turn off wi-fi".into()).unwrap();
    let Resolution::RunSkill { skill_match } = &m.resolution else {
        panic!("{m:?}")
    };
    assert!(skill_match.exact && skill_match.skill_id == "wifi-off");
    assert!(m.workflows.is_empty() && !m.needs_owner_choice);

    // A workflow phrase that is also an exact skill trigger: the owner chooses.
    let mut clash = morning();
    clash.id = "wifi-routine".into();
    clash.trigger = Trigger::Phrase {
        text: "turn off wi-fi".into(),
    };
    ws.upsert(clash, 2).unwrap();
    let m = resolve_utterance(Arc::clone(&sk), Arc::clone(&ws), "turn off wi-fi".into()).unwrap();
    assert_eq!(m.resolution, Resolution::OwnerChoice);
    assert!(m.needs_owner_choice && m.workflows.len() == 1 && !m.skills.is_empty());

    let m = resolve_utterance(sk, ws, "order a pizza".into()).unwrap();
    assert_eq!(m.resolution, Resolution::NoMatch);
    assert!(!m.needs_owner_choice);
}
