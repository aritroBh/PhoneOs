//! Regression tests for the adversarial review of skills and workflows. Each test names
//! the finding (m = matcher, s/f = store, w = workflow runner) it pins down.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use common::*;
use jarvis_proto::OutcomeStatus;
use jarvis_skills::{
    resolve_utterance, Ambiguity, Condition, OnFailure, Resolution, RunLimits, Skill, SkillError,
    SkillKind, SkillParam, SkillStore, Step, Trigger, Workflow, WorkflowEffect, WorkflowEvent,
    WorkflowRun, WorkflowStatus, WorkflowStore,
};

fn skill(id: &str, triggers: &[&str], params: &[&str]) -> Skill {
    Skill {
        id: id.into(),
        name: id.into(),
        triggers: triggers.iter().map(|s| (*s).to_owned()).collect(),
        params: params.iter().map(|p| param(p, true)).collect(),
        ..Skill::default()
    }
}

// ---------------------------------------------------------------------------------------
// Matcher.
// ---------------------------------------------------------------------------------------

#[test]
fn m1_m2_slots_never_capture_only_filler_words() {
    let s = store_with(vec![skill(
        "text",
        &["text {contact} {message}"],
        &["contact", "message"],
    )]);
    let m = s.match_task("text the mom hello there");
    assert_eq!(m[0].args["contact"], "mom");
    assert_eq!(m[0].args["message"], "hello there");
    let s = store_with(vec![skill("call", &["call {who}"], &["who"])]);
    assert!(s.match_task("please call the").is_empty());
    assert!(s.match_task("call the a").is_empty());
    assert_eq!(s.match_task("call the doctor")[0].args["who"], "the doctor");
    // Adjacent slots: the first may not be a lone filler either.
    let s = store_with(vec![skill(
        "text",
        &["text {contact} {message}"],
        &["contact", "message"],
    )]);
    assert!(s.match_task("text the").is_empty());
    assert!(
        s.match_task("text mom the").is_empty(),
        "message is only filler"
    );
}

#[test]
fn m9_opposite_and_negated_requests_do_not_match_loosely() {
    let s = store_with(vec![
        skill(
            "lights-on",
            &["turn on the living room lights tonight"],
            &[],
        ),
        skill("del", &["delete all my old notes"], &[]),
        skill("wifi-on", &["turn on wifi now"], &[]),
        skill("music", &["play some music in the kitchen"], &[]),
    ]);
    for q in [
        "turn off the living room lights tonight",
        "do not delete all my old notes",
        "don't delete all my old notes",
        "never delete all my old notes",
        "delete all my old notes no",
        "turn off wifi now",
        "do not turn on wifi now",
        "pause some music in the kitchen",
        "stop the music in the kitchen",
        "cancel: play some music in the kitchen",
    ] {
        assert!(s.match_task(q).is_empty(), "{q:?} -> {:?}", s.match_task(q));
    }
    // The genuine requests still match, loosely or exactly.
    let m = s.match_task("turn on the living room lights tonight please");
    assert_eq!(m[0].skill_id, "lights-on");
    assert!(m[0].exact);
    let m = s.match_task("turn on living room lights tonight now");
    assert_eq!(m[0].skill_id, "lights-on");
    assert!(!m[0].exact);
    // A negator that the trigger itself contains is fine.
    let s = store_with(vec![skill("dnd", &["do not disturb on"], &[])]);
    assert!(s
        .match_task("do not disturb on")
        .first()
        .is_some_and(|m| m.exact));
    assert_eq!(s.match_task("do not disturb on now").len(), 1);
}

#[test]
fn only_exact_unambiguous_matches_may_run_without_asking() {
    let s = store_with(vec![
        skill(
            "lights-on",
            &["turn on the living room lights tonight"],
            &[],
        ),
        text_skill(),
    ]);
    let exact = s.match_task("turn on the living room lights tonight");
    assert!(exact[0].exact && !exact.needs_owner_choice());
    let slotted = s.match_task("text mom hi");
    assert!(slotted[0].exact && !slotted.needs_owner_choice());
    let loose = s.match_task("turn on living room lights tonight now");
    assert!(!loose[0].exact);
    assert!(
        loose.needs_owner_choice(),
        "a loose match is only a suggestion"
    );
    assert!(!s.match_task("nothing like it").needs_owner_choice());
    let mut twin = wifi_skill();
    twin.id = "wifi-twin".into();
    let tied = store_with(vec![wifi_skill(), twin]).match_task("turn off wifi");
    assert!(tied[0].exact && tied.is_ambiguous() && tied.needs_owner_choice());
}

/// Review finding s9: 500 skills × 12 slotted triggers × 8 slots on a 500-character
/// request that passes every literal pre-check.
#[test]
fn s9_matching_at_the_limits_is_fast() {
    let mut trig = String::new();
    for i in 0..8 {
        trig.push_str("b b b b b ");
        trig.push_str(&format!("{{p{i}}} "));
    }
    let trig = trig.trim().to_owned();
    assert!(trig.chars().count() <= 120);
    let skills: Vec<Skill> = (0..500)
        .map(|i| Skill {
            id: format!("s{i:03}"),
            name: format!("s{i}"),
            triggers: vec![trig.clone(); 12],
            params: (0..8).map(|p| param(&format!("p{p}"), true)).collect(),
            ..Skill::default()
        })
        .collect();
    let st = store_with(skills);
    let matching = "b ".repeat(250);
    // Fails at the first literal, after the whole table was filled for every trigger.
    let failing = "c ".to_owned() + &"b ".repeat(249);
    assert_eq!(matching.chars().count(), 500);
    let t = Instant::now();
    let m = st.match_task(&matching);
    assert_eq!(m.len(), 500);
    assert!(st.match_task(&failing).is_empty());
    let elapsed = t.elapsed();
    // Release: tens of milliseconds. Debug builds are ~20x slower; the bound only has to
    // catch a quadratic or exponential regression, without flaking on a slow CI machine.
    let bound = if cfg!(debug_assertions) {
        Duration::from_secs(4)
    } else {
        Duration::from_millis(600)
    };
    eprintln!("s9 match time: {elapsed:?}");
    assert!(elapsed < bound, "{elapsed:?}");
}

#[test]
fn compiled_triggers_follow_every_change() {
    let mut s = store_with(vec![wifi_skill()]);
    assert_eq!(s.match_task("turn off wifi").len(), 1);
    let mut edited = wifi_skill();
    edited.triggers = vec!["kill the wifi".into()];
    s.update(edited, 2).unwrap();
    assert!(s.match_task("turn off wifi").is_empty());
    assert_eq!(s.match_task("kill the wifi").len(), 1);
    s.add(text_skill()).unwrap();
    assert_eq!(s.match_task("text mom hi")[0].skill_id, "text-someone");
    assert_eq!(s.match_task("kill the wifi")[0].skill_id, "wifi-off");
    s.remove("text-someone");
    assert!(s.match_task("text mom hi").is_empty());
    let loaded = SkillStore::from_json(&s.to_json().unwrap()).unwrap();
    assert_eq!(loaded.match_task("kill the wifi").len(), 1);
}

// ---------------------------------------------------------------------------------------
// Store.
// ---------------------------------------------------------------------------------------

/// Review finding f1: a skill that cannot be loaded must not vanish on the next save.
#[test]
fn f1_damaged_entries_survive_save_and_reload() {
    let json = r#"{"schema_version":1,"skills":[
      {"id":"good","name":"Good","apps":["com.x"],"confine_to_apps":true},
      {"id":"damaged","name":"Owner relies on this","apps":["com.y"],"confine_to_apps":true,
       "triggers":["go"],"runs":-1}]}"#;
    let (s, issues) = SkillStore::from_json_with_issues(json).unwrap();
    assert_eq!(s.len(), 1);
    assert!(s.has_load_issues());
    assert_eq!(issues, s.load_issues());
    assert_eq!(issues.len(), 1);
    assert!(
        issues[0].starts_with(
            "skills[1] \"damaged\" (Owner relies on this): not loaded, kept unchanged"
        ),
        "{issues:?}"
    );
    assert_eq!(s.damaged_ids(), vec![Some("damaged".to_owned())]);
    let saved = s.to_json().unwrap();
    let (again, issues) = SkillStore::from_json_with_issues(&saved).unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(issues.len(), 1, "still reported after the round trip");
    let raw: serde_json::Value = serde_json::from_str(&again.to_json().unwrap()).unwrap();
    assert_eq!(
        raw["skills"][1]["runs"], -1,
        "kept byte for byte in meaning"
    );
    assert_eq!(raw["skills"][1]["name"], "Owner relies on this");

    // Re-creating the skill replaces the damaged entry; removing it discards it.
    let mut fixed = again.clone();
    fixed
        .add(Skill {
            id: "damaged".into(),
            name: "Fixed".into(),
            ..Skill::default()
        })
        .unwrap();
    assert!(fixed.damaged_ids().is_empty());
    let mut gone = again.clone();
    assert!(gone.remove("damaged").is_none());
    assert!(gone.damaged_ids().is_empty());
    assert!(!gone.to_json().unwrap().contains("Owner relies on this"));
}

/// Review finding s4: a misspelled field is tolerated but never silent.
#[test]
fn s4_unknown_fields_are_reported() {
    let j = r#"{"schema_version":1,"later":1,"skills":[
        {"id":"a","name":"n","apps":["com.x"],"confineToApps":true,
         "params":[{"name":"p","required":true,"hint":"x"}],
         "triggers":["go {p}"]}]}"#;
    let (s, issues) = SkillStore::from_json_with_issues(j).unwrap();
    assert!(!s.get("a").unwrap().confine_to_apps);
    assert!(
        issues.contains(&"skills[0] \"a\" (n): unknown field confineToApps ignored".to_owned()),
        "{issues:?}"
    );
    assert!(issues.contains(&"snapshot: unknown field later ignored".to_owned()));
    assert!(issues
        .iter()
        .any(|i| i.contains("params[0]: unknown field hint ignored")));
    // Every field the store writes is a known field (no false reports on round trip).
    let full = SkillStore::from_json(&store_with(vec![text_skill()]).to_json().unwrap()).unwrap();
    assert!(!full.has_load_issues(), "{:?}", full.load_issues());
}

/// Review finding s6: `update` cannot rewrite the store's bookkeeping.
#[test]
fn s6_update_keeps_kind_and_history() {
    let mut st = SkillStore::new();
    st.add(Skill {
        kind: SkillKind::Learned,
        created_ms: 5,
        updated_ms: 5,
        ..wifi_skill()
    })
    .unwrap();
    st.record_run("wifi-off", 6).unwrap();
    st.update(
        Skill {
            kind: SkillKind::BuiltIn,
            created_ms: 1,
            updated_ms: 1,
            runs: 0,
            last_run_ms: None,
            ..wifi_skill()
        },
        50,
    )
    .unwrap();
    let g = st.get("wifi-off").unwrap();
    assert_eq!(
        (g.kind, g.created_ms, g.updated_ms, g.runs, g.last_run_ms),
        (SkillKind::Learned, 5, 50, 1, Some(6))
    );
    st.set_kind("wifi-off", SkillKind::Written, 60).unwrap();
    assert_eq!(st.get("wifi-off").unwrap().kind, SkillKind::Written);
    assert_eq!(
        st.set_kind("nope", SkillKind::Written, 1),
        Err(SkillError::NotFound("nope".into()))
    );
    // A fresh upsert stamps creation with now.
    st.upsert(
        Skill {
            id: "new".into(),
            created_ms: 1,
            runs: 9,
            ..wifi_skill()
        },
        70,
    )
    .unwrap();
    let n = st.get("new").unwrap();
    assert_eq!((n.created_ms, n.updated_ms, n.runs), (70, 70, 0));
}

/// Review finding s8: invisible and bidi characters make different values look alike.
#[test]
fn s8_invisible_characters_are_refused() {
    let base = wifi_skill();
    for bad in [
        Skill {
            name: "Wi-Fi\u{202E}ffo".into(),
            ..base.clone()
        },
        Skill {
            name: "Wi\u{200B}Fi".into(),
            ..base.clone()
        },
        Skill {
            triggers: vec!["turn off\u{2060} wifi".into()],
            ..base.clone()
        },
        Skill {
            apps: vec!["com.x\u{200B}".into()],
            ..base.clone()
        },
        Skill {
            summary: "a\u{FEFF}".into(),
            ..base.clone()
        },
        Skill {
            tools: vec!["open\u{200D}link".into()],
            ..base.clone()
        },
        Skill {
            params: vec![SkillParam {
                name: "p".into(),
                description: "d\u{0085}".into(),
                required: false,
            }],
            ..base.clone()
        },
    ] {
        assert!(bad.validate().is_err(), "{bad:?}");
    }
    // Instructions may span lines and are sanitised when they enter a prompt.
    Skill {
        instructions: "line one\nline two".into(),
        ..base
    }
    .validate()
    .unwrap();
    let mut w = workflow_with(vec![Step::Notify {
        text: "hi\u{202E}".into(),
    }]);
    assert!(w.validate().is_err());
    w.steps = vec![Step::Notify { text: "hi".into() }];
    w.name = "Bed\u{200B}time".into();
    assert!(w.validate().is_err());
}

#[test]
fn tool_allow_list_is_validated_and_carried_into_the_context() {
    let mut k = wifi_skill();
    k.tools = vec!["open_link".into()];
    k.validate().unwrap();
    assert_eq!(
        k.context(BTreeMap::new()).unwrap().tools,
        vec!["open_link".to_owned()]
    );
    k.tools = vec!["two words".into()];
    assert!(k.validate().is_err());
    k.tools = (0..17).map(|i| format!("t{i}")).collect();
    assert!(k.validate().is_err());
}

// ---------------------------------------------------------------------------------------
// Workflow runner.
// ---------------------------------------------------------------------------------------

fn workflow_with(steps: Vec<Step>) -> Workflow {
    Workflow {
        id: "w".into(),
        name: "W".into(),
        steps,
        on_failure: OnFailure::Continue,
        ..Workflow::default()
    }
}

fn finished(e: &WorkflowEffect) -> &jarvis_skills::WorkflowOutcome {
    match e {
        WorkflowEffect::Finished { outcome } => outcome,
        other => panic!("not finished: {other:?}"),
    }
}

fn done(status: OutcomeStatus, summary: &str) -> WorkflowEvent {
    WorkflowEvent::SessionFinished {
        status,
        summary: summary.into(),
    }
}

/// Review finding w1 and the meaning of "last": a step that failed without a session is
/// the last result; asking, notifying and a completed wait leave it alone.
#[test]
fn w1_last_result_is_the_most_recent_task_or_failure() {
    let skills = SkillStore::new();
    // RunTask ok, then a missing skill fails (Continue), then the gate must not pass.
    let mut r = WorkflowRun::new(
        workflow_with(vec![
            Step::RunTask { text: "a".into() },
            Step::RunSkill {
                skill_id: "missing".into(),
                args: BTreeMap::new(),
            },
            Step::StopUnless {
                condition: Condition::LastSucceeded,
            },
            Step::Notify {
                text: "should not run".into(),
            },
        ]),
        RunLimits::default(),
    );
    r.step(0, WorkflowEvent::Start, &skills).unwrap();
    let e = r
        .step(1, done(OutcomeStatus::Success, "ok"), &skills)
        .unwrap();
    assert_eq!(finished(&e).status, WorkflowStatus::Stopped);
    assert_eq!(finished(&e).failed_step, Some(1));

    // A refused wait is a failure too, and its reason is the last summary.
    let limits = RunLimits {
        max_wait_ms: 10,
        ..RunLimits::default()
    };
    let mut r = WorkflowRun::new(
        workflow_with(vec![
            Step::RunTask { text: "a".into() },
            Step::Wait { ms: 60_000 },
            Step::StopUnless {
                condition: Condition::LastSummaryContains {
                    text: "longer than".into(),
                },
            },
            Step::StopUnless {
                condition: Condition::LastSucceeded,
            },
        ]),
        limits,
    );
    r.step(0, WorkflowEvent::Start, &skills).unwrap();
    let e = r
        .step(1, done(OutcomeStatus::Success, "ok"), &skills)
        .unwrap();
    assert_eq!(finished(&e).status, WorkflowStatus::Stopped);
    assert!(
        finished(&e).summary.contains("step 4"),
        "{}",
        finished(&e).summary
    );

    // AskOwner, Notify and a completed Wait do not change the last result.
    let mut r = WorkflowRun::new(
        workflow_with(vec![
            Step::RunTask { text: "a".into() },
            Step::AskOwner {
                question: "go?".into(),
            },
            Step::Notify { text: "n".into() },
            Step::Wait { ms: 5 },
            Step::StopUnless {
                condition: Condition::LastSucceeded,
            },
            Step::StopUnless {
                condition: Condition::OwnerSaidYes,
            },
        ]),
        RunLimits::default(),
    );
    r.step(0, WorkflowEvent::Start, &skills).unwrap();
    r.step(1, done(OutcomeStatus::Success, "ok"), &skills)
        .unwrap();
    r.step(
        2,
        WorkflowEvent::OwnerAnswered {
            yes: false,
            text: String::new(),
        },
        &skills,
    )
    .unwrap();
    r.step(3, WorkflowEvent::Notified, &skills).unwrap();
    let e = r.step(10, WorkflowEvent::WaitElapsed, &skills).unwrap();
    let o = finished(&e);
    assert_eq!(o.status, WorkflowStatus::Stopped);
    assert!(
        o.summary.contains("step 6"),
        "LastSucceeded passed, OwnerSaidYes did not: {}",
        o.summary
    );

    // Before any task there is no last result.
    let mut r = WorkflowRun::new(
        workflow_with(vec![Step::StopUnless {
            condition: Condition::LastSucceeded,
        }]),
        RunLimits::default(),
    );
    let e = r.step(0, WorkflowEvent::Start, &skills).unwrap();
    assert_eq!(finished(&e).status, WorkflowStatus::Stopped);
}

/// Review finding w2: a safety stop ends the run whatever `on_failure` says.
#[test]
fn w2_blocked_interrupted_and_cancelled_sessions_end_the_run() {
    let skills = SkillStore::new();
    let wf = workflow_with(vec![
        Step::RunTask {
            text: "open passwords".into(),
        },
        Step::RunTask {
            text: "next".into(),
        },
    ]);
    assert_eq!(wf.on_failure, OnFailure::Continue);
    for (status, expected) in [
        (OutcomeStatus::Blocked, WorkflowStatus::Blocked),
        (OutcomeStatus::Interrupted, WorkflowStatus::Interrupted),
        (OutcomeStatus::Cancelled, WorkflowStatus::Cancelled),
    ] {
        let mut r = WorkflowRun::new(wf.clone(), RunLimits::default());
        r.step(0, WorkflowEvent::Start, &skills).unwrap();
        let e = r
            .step(1, done(status, "Blocked: deny list"), &skills)
            .unwrap();
        let o = finished(&e);
        assert_eq!(o.status, expected, "{status:?}");
        assert_eq!(o.failed_step, Some(0));
        assert_eq!(o.steps_run, 1);
    }
    // An ordinary failure continues.
    let mut r = WorkflowRun::new(wf, RunLimits::default());
    r.step(0, WorkflowEvent::Start, &skills).unwrap();
    let e = r
        .step(1, done(OutcomeStatus::Failed, "no"), &skills)
        .unwrap();
    assert!(matches!(e, WorkflowEffect::StartSession { .. }));
    let e = r
        .step(2, done(OutcomeStatus::Success, "ok"), &skills)
        .unwrap();
    assert_eq!(finished(&e).status, WorkflowStatus::CompletedWithFailures);
    // The event's JSON carries the status.
    let json = serde_json::to_value(done(OutcomeStatus::Interrupted, "locked")).unwrap();
    assert_eq!(json["type"], "session_finished");
    assert_eq!(json["status"], "interrupted");
}

/// Review finding w4: a clock that jumps backwards cannot stretch waits or the budget.
#[test]
fn w4_backwards_clock_is_ignored() {
    let skills = SkillStore::new();
    let wf = workflow_with(vec![
        Step::Wait { ms: 60_000 },
        Step::Notify { text: "x".into() },
    ]);
    let mut r = WorkflowRun::new(wf.clone(), RunLimits::default());
    r.step(10_000_000, WorkflowEvent::Start, &skills).unwrap();
    // Time goes back: the run still measures from the highest clock seen.
    let e = r.step(0, WorkflowEvent::WaitElapsed, &skills).unwrap();
    assert_eq!(
        e,
        WorkflowEffect::Wait {
            until_ms: 10_060_000
        }
    );
    let e = r
        .step(10_060_000, WorkflowEvent::WaitElapsed, &skills)
        .unwrap();
    assert!(matches!(e, WorkflowEffect::Notify { .. }));

    // The total budget too: an early timestamp after a late one does not reset it.
    let limits = RunLimits {
        max_total_ms: 100_000,
        ..RunLimits::default()
    };
    let wf = workflow_with(vec![
        Step::RunTask { text: "a".into() },
        Step::RunTask { text: "b".into() },
    ]);
    let mut r = WorkflowRun::new(wf, limits);
    r.step(1_000, WorkflowEvent::Start, &skills).unwrap();
    r.step(500_000, done(OutcomeStatus::Failed, "late"), &skills)
        .unwrap();
    let e = r
        .step(2_000, done(OutcomeStatus::Success, "ok"), &skills)
        .unwrap();
    assert_eq!(finished(&e).status, WorkflowStatus::Failed);
}

/// Review finding w6: what saves also runs (one wait cap).
#[test]
fn w6_the_longest_valid_wait_runs_with_default_limits() {
    let skills = SkillStore::new();
    let max = jarvis_skills::workflow::MAX_WAIT_STEP_MS;
    assert_eq!(RunLimits::default().max_wait_ms, max);
    let wf = workflow_with(vec![Step::Wait { ms: max }]);
    wf.validate().unwrap();
    let mut r = WorkflowRun::new(wf, RunLimits::default());
    assert_eq!(
        r.step(0, WorkflowEvent::Start, &skills).unwrap(),
        WorkflowEffect::Wait { until_ms: max }
    );
    assert!(workflow_with(vec![Step::Wait { ms: max + 1 }])
        .validate()
        .is_err());
}

/// Review finding w7: each skill step reads the store when it starts.
#[test]
fn w7_skill_changes_mid_run_take_effect() {
    let mut st = store_with(vec![text_skill()]);
    let args: BTreeMap<String, String> = [("contact", "Mom"), ("message", "hi")]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    let wf = Workflow {
        on_failure: OnFailure::Stop,
        ..workflow_with(vec![
            Step::Wait { ms: 100 },
            Step::RunSkill {
                skill_id: "text-someone".into(),
                args: args.clone(),
            },
            Step::RunSkill {
                skill_id: "text-someone".into(),
                args,
            },
        ])
    };
    let mut r = WorkflowRun::new(wf, RunLimits::default());
    r.step(0, WorkflowEvent::Start, &st).unwrap();
    // The owner adds an app to the skill while the run waits: the session gets it.
    let mut wider = text_skill();
    wider.apps.push(SETTINGS.into());
    st.update(wider, 50).unwrap();
    let WorkflowEffect::StartSession { skill, .. } =
        r.step(100, WorkflowEvent::WaitElapsed, &st).unwrap()
    else {
        panic!()
    };
    assert_eq!(skill.unwrap().apps.len(), 2);
    // Then disables it: the next step fails instead of running the old definition.
    let mut off = st.get("text-someone").unwrap().clone();
    off.enabled = false;
    st.update(off, 60).unwrap();
    let e = r
        .step(200, done(OutcomeStatus::Success, "Sent"), &st)
        .unwrap();
    let o = finished(&e);
    assert_eq!(o.status, WorkflowStatus::Failed);
    assert_eq!(o.failed_step, Some(2));
    assert!(o.summary.contains("disabled"), "{}", o.summary);
}

// ---------------------------------------------------------------------------------------
// Workflow phrase vs skill trigger.
// ---------------------------------------------------------------------------------------

fn phrase_workflow(id: &str, phrase: &str) -> Workflow {
    Workflow {
        id: id.into(),
        name: id.into(),
        trigger: Trigger::Phrase {
            text: phrase.into(),
        },
        steps: vec![Step::Notify { text: "x".into() }],
        ..Workflow::default()
    }
}

#[test]
fn utterance_precedence_is_defined() {
    let skills = store_with(vec![
        wifi_skill(),
        skill("lights", &["turn on the living room lights tonight"], &[]),
    ]);
    let mut wfs = WorkflowStore::new();
    wfs.add(phrase_workflow("bedtime", "good night")).unwrap();
    wfs.add(phrase_workflow("wifi-flow", "turn off wifi"))
        .unwrap();
    wfs.add(phrase_workflow(
        "lights-flow",
        "turn on living room lights tonight now",
    ))
    .unwrap();

    // Workflow only.
    let u = resolve_utterance(&skills, &wfs, "Good night!");
    assert_eq!(
        u.resolution,
        Resolution::RunWorkflow {
            workflow_id: "bedtime".into()
        }
    );
    // Exact workflow and exact skill: the owner chooses.
    let u = resolve_utterance(&skills, &wfs, "turn off wi-fi");
    assert_eq!(u.workflows, vec!["wifi-flow".to_owned()]);
    assert!(u.skills[0].exact);
    assert_eq!(u.resolution, Resolution::OwnerChoice);
    // Exact workflow beats a loose skill match.
    let u = resolve_utterance(&skills, &wfs, "turn on living room lights tonight now");
    assert!(!u.skills.is_empty() && !u.skills[0].exact);
    assert_eq!(
        u.resolution,
        Resolution::RunWorkflow {
            workflow_id: "lights-flow".into()
        }
    );
    // Skill only: exact runs, loose asks, nothing is a plain task.
    let u = resolve_utterance(&skills, &WorkflowStore::new(), "turn off wifi");
    assert!(matches!(u.resolution, Resolution::RunSkill(m) if m.skill_id == "wifi-off"));
    let u = resolve_utterance(
        &skills,
        &WorkflowStore::new(),
        "turn on living room lights tonight now",
    );
    assert_eq!(u.resolution, Resolution::OwnerChoice);
    let u = resolve_utterance(&skills, &wfs, "what's the weather");
    assert_eq!(u.resolution, Resolution::NoMatch);
    // Two workflows on one phrase: the owner chooses.
    wfs.add(phrase_workflow("bedtime-2", "good night")).unwrap();
    assert_eq!(
        resolve_utterance(&skills, &wfs, "good night").resolution,
        Resolution::OwnerChoice
    );
}
