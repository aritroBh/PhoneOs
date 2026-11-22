//! `JarvisSkills` from Rust: load/save, upsert with validation errors, matching, contexts,
//! flow promotion, and a session that runs a confined skill.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::collections::HashMap;

use common::*;
use jarvis_core_ffi::*;

fn text_skill() -> Skill {
    Skill {
        id: "text-someone".into(),
        name: "Text someone".into(),
        summary: String::new(),
        kind: SkillKind::Written,
        triggers: vec!["text {contact} {message}".into()],
        instructions: "Use the existing thread.".into(),
        apps: vec!["com.apple.MobileSMS".into()],
        confine_to_apps: true,
        params: vec![
            SkillParam {
                name: "contact".into(),
                description: String::new(),
                required: true,
            },
            SkillParam {
                name: "message".into(),
                description: String::new(),
                required: true,
            },
        ],
        flow_keys: vec![],
        enabled: true,
        created_ms: 1,
        updated_ms: 1,
        runs: 0,
        last_run_ms: None,
    }
}

#[test]
fn store_round_trip_match_and_context() {
    let skills = JarvisSkills::new(String::new()).unwrap();
    assert!(skills.list().unwrap().is_empty());
    assert!(skills.upsert(text_skill()).unwrap());
    assert!(
        !skills.upsert(text_skill()).unwrap(),
        "second upsert updates"
    );
    let mut bad = text_skill();
    bad.triggers = vec!["text {who}".into()];
    let err = skills.upsert(bad).unwrap_err();
    assert!(
        matches!(&err, JarvisError::InvalidSkill { message } if message.contains("{who}")),
        "{err:?}"
    );

    let result = skills.match_task("Text Mom I'm on my way".into()).unwrap();
    assert!(!result.ambiguous);
    let m = result.matches[0].clone();
    assert_eq!(m.skill_id, "text-someone");
    assert_eq!(m.args["contact"], "Mom");
    assert_eq!(m.args["message"], "I'm on my way");
    let ctx = skills.context_for_match(m.clone()).unwrap();
    assert!(ctx.confine_to_apps);
    assert_eq!(ctx.args, m.args);
    assert_eq!(
        skills.task_text("text-someone".into(), m.args).unwrap(),
        "text Mom I'm on my way"
    );
    assert!(matches!(
        skills.context("nope".into(), HashMap::new()),
        Err(JarvisError::InvalidSkill { .. })
    ));

    skills.record_run("text-someone".into(), 77).unwrap();
    let json = skills.to_json().unwrap();
    let again = JarvisSkills::new(json).unwrap();
    assert!(again.load_issues().unwrap().is_empty());
    assert_eq!(
        again
            .get("text-someone".into())
            .unwrap()
            .unwrap()
            .last_run_ms,
        Some(77)
    );
    assert!(again.remove("text-someone".into()).unwrap());
    assert!(!again.remove("text-someone".into()).unwrap());

    assert!(matches!(
        JarvisSkills::new("{not json".into()),
        Err(JarvisError::InvalidSkill { .. })
    ));
    let partly =
        JarvisSkills::new(r#"{"skills":[{"id":"X"},{"id":"ok","name":"Ok"}]}"#.into()).unwrap();
    assert_eq!(partly.list().unwrap().len(), 1);
    assert_eq!(partly.load_issues().unwrap().len(), 1);
}

#[test]
fn promote_a_flow_learned_by_a_session() {
    // Learn the Wi-Fi flow with the shared end-to-end scenario, then promote it.
    let first = drive("");
    let memory = first.memory_json.clone();
    let parsed: serde_json::Value = serde_json::from_str(&memory).unwrap();
    let flow_id = parsed["flows"]["flows"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let skills = JarvisSkills::new(String::new()).unwrap();
    let id = skills
        .promote_flow(memory.clone(), flow_id.clone(), "Wi-Fi off".into(), 5)
        .unwrap();
    let skill = skills.get(id.clone()).unwrap().unwrap();
    assert_eq!(skill.kind, SkillKind::Learned);
    assert_eq!(skill.apps, vec![SETTINGS.to_owned()]);
    let m = skills.match_task("Turn off Wi-Fi".into()).unwrap();
    assert_eq!(m.matches[0].skill_id, id);
    assert!(matches!(
        skills.promote_flow(memory, "nope".into(), "x".into(), 5),
        Err(JarvisError::InvalidSkill { .. })
    ));
    assert!(matches!(
        skills.promote_flow("{".into(), flow_id, "x".into(), 5),
        Err(JarvisError::InvalidMemory { .. })
    ));
}

#[test]
fn a_confined_skill_session_refuses_to_open_another_app() {
    let mut config = config();
    config.skill = Some(SkillContext {
        id: "wifi".into(),
        name: "Wi-Fi".into(),
        instructions: "Only use Settings.".into(),
        apps: vec![SETTINGS.into()],
        confine_to_apps: true,
        args: HashMap::new(),
    });
    let s = JarvisSession::new(config, task(), String::new()).unwrap();
    assert!(matches!(s.step(1, Event::Start), Effect::Observe { .. }));
    let mut now = 2;
    let Effect::Infer { request } = s.step(
        now,
        Event::Observed {
            observation: ScreenObservation {
                screen: settings_screen(),
                screenshot: None,
            },
        },
    ) else {
        panic!("expected infer");
    };
    assert!(request
        .user
        .contains("Owner's guidance for this task (skill: Wi-Fi):"));
    assert!(request.user.contains("| Only use Settings."));
    let open = r#"{"action":"open_app","app":"com.example.shop"}"#;
    for _ in 0..2 {
        now += 1;
        let e = s.step(
            now,
            Event::ModelOutput {
                output: ModelOutput { text: open.into() },
            },
        );
        match e {
            Effect::Infer { request } => {
                assert!(
                    request.user.contains("works only in its own apps"),
                    "{}",
                    request.user
                );
            }
            Effect::Finished { outcome } => {
                assert_eq!(outcome.status, OutcomeStatus::Blocked);
                return;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    panic!("the second escape should block");
}

#[test]
fn default_session_config_has_no_skill() {
    assert!(api::default_session_config().unwrap().skill.is_none());
}
