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
        tools: vec![],
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
        tools: vec![],
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
                device_locked: false,
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

fn args(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

/// Review finding f1: an unloadable skill survives a save through the bridge.
#[test]
fn f1_unloadable_skills_survive_to_json() {
    let json = r#"{"schema_version":1,"skills":[
      {"id":"good","name":"Good","apps":["com.x"],"confine_to_apps":true},
      {"id":"damaged","name":"Owner relies on this","apps":["com.y"],"confine_to_apps":true,
       "triggers":["go"],"runs":-1}]}"#;
    let s = JarvisSkills::new(json.into()).unwrap();
    assert!(s.has_load_issues().unwrap());
    assert!(s.load_issues().unwrap()[0].contains("\"damaged\" (Owner relies on this)"));
    let s2 = JarvisSkills::new(s.to_json().unwrap()).unwrap();
    assert!(s2.has_load_issues().unwrap(), "still reported after a save");
    assert!(s2.to_json().unwrap().contains("Owner relies on this"));
    assert_eq!(s2.list().unwrap().len(), 1);
    assert!(
        s2.remove("damaged".into()).unwrap(),
        "the owner can discard it"
    );
    assert!(!s2.to_json().unwrap().contains("Owner relies on this"));
}

/// Review finding f2: a stale or forged match is rechecked against the current skill.
#[test]
fn f2_context_for_match_rechecks_the_skill() {
    let s = JarvisSkills::new(String::new()).unwrap();
    s.upsert(text_skill()).unwrap();
    let m = s
        .match_task("text mom hello".into())
        .unwrap()
        .matches
        .remove(0);
    assert!(m.exact);
    // A new required parameter: the old match no longer fills it.
    let mut k = text_skill();
    k.params.push(SkillParam {
        name: "when".into(),
        description: String::new(),
        required: true,
    });
    k.triggers = vec!["text {contact} {message} at {when}".into()];
    s.upsert(k).unwrap();
    let e = s.context_for_match(m.clone()).unwrap_err();
    assert!(
        matches!(&e, JarvisError::InvalidSkill { message } if message.contains("needs when")),
        "{e:?}"
    );
    // Disabled, unknown, undeclared, over-long: all refused.
    let mut off = text_skill();
    off.enabled = false;
    s.upsert(off).unwrap();
    assert!(s.context_for_match(m.clone()).is_err());
    let mut forged = m.clone();
    forged.skill_id = "nope".into();
    assert!(s.context_for_match(forged).is_err());
    s.upsert(text_skill()).unwrap();
    assert!(s.context_for_match(m.clone()).is_ok());
    let mut forged = m.clone();
    forged.args.insert("evil".into(), "x".into());
    assert!(s.context_for_match(forged).is_err());
    let mut forged = m.clone();
    forged.args.insert("contact".into(), "x".repeat(501));
    assert!(s.context_for_match(forged).is_err());
    // Values are cleaned, not trusted verbatim.
    let mut padded = m;
    padded
        .args
        .insert("contact".into(), " Mo\u{200B}m\n".into());
    assert_eq!(s.context_for_match(padded).unwrap().args["contact"], "Mom");
}

/// Review finding f5: a long captured value reaches the session whole.
#[test]
fn f5_long_values_are_not_cut() {
    let s = JarvisSkills::new(String::new()).unwrap();
    s.upsert(text_skill()).unwrap();
    let msg = "word ".repeat(90);
    let m = s
        .match_task(format!("text mom {msg}"))
        .unwrap()
        .matches
        .remove(0);
    assert_eq!(m.args["message"], msg.trim());
    let c = s.context_for_match(m).unwrap();
    assert_eq!(c.args["message"], msg.trim());
    let t = s
        .task_text(
            "text-someone".into(),
            args(&[("contact", "mom"), ("message", msg.trim())]),
        )
        .unwrap();
    assert!(t.ends_with(msg.trim()));
}

#[test]
fn match_result_says_when_the_owner_must_choose() {
    let s = JarvisSkills::new(String::new()).unwrap();
    let mut lights = text_skill();
    lights.id = "lights".into();
    lights.triggers = vec!["turn on the living room lights tonight".into()];
    lights.params = vec![];
    s.upsert(lights).unwrap();
    let exact = s
        .match_task("turn on the living room lights tonight".into())
        .unwrap();
    assert!(exact.matches[0].exact && !exact.needs_owner_choice);
    let loose = s
        .match_task("turn on living room lights tonight now".into())
        .unwrap();
    assert!(!loose.matches[0].exact && loose.needs_owner_choice && !loose.ambiguous);
    assert!(s
        .match_task("turn off the living room lights tonight".into())
        .unwrap()
        .matches
        .is_empty());
    // kind is kept on update and changed explicitly.
    let mut k = s.get("lights".into()).unwrap().unwrap();
    k.kind = SkillKind::BuiltIn;
    s.upsert(k).unwrap();
    assert_eq!(
        s.get("lights".into()).unwrap().unwrap().kind,
        SkillKind::Written
    );
    s.set_kind("lights".into(), SkillKind::BuiltIn, 9).unwrap();
    assert_eq!(
        s.get("lights".into()).unwrap().unwrap().kind,
        SkillKind::BuiltIn
    );
}

#[test]
fn a_confined_skill_with_an_allow_listed_tool_carries_it_over_the_bridge() {
    let s = JarvisSkills::new(String::new()).unwrap();
    let mut k = text_skill();
    k.tools = vec!["open_link".into()];
    s.upsert(k).unwrap();
    let c = s
        .context(
            "text-someone".into(),
            args(&[("contact", "mom"), ("message", "hi")]),
        )
        .unwrap();
    assert_eq!(c.tools, vec!["open_link".to_owned()]);
}
