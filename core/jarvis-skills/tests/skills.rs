//! Skill validation, the store's CRUD and JSON snapshot, contexts and flow promotion.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::collections::BTreeMap;

use common::*;
use jarvis_memory::{Flow, FlowAction, FlowStep};
use jarvis_proto::Role;
use jarvis_skills::{Skill, SkillError, SkillKind, SkillStore, Upsert, MAX_SKILLS};

fn args(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[test]
fn realistic_skills_validate() {
    text_skill().validate().unwrap();
    wifi_skill().validate().unwrap();
}

#[test]
fn every_limit_is_enforced_with_a_typed_error() {
    let base = wifi_skill();
    let cases: Vec<(&str, Skill)> = vec![
        (
            "empty id",
            Skill {
                id: String::new(),
                ..base.clone()
            },
        ),
        (
            "upper id",
            Skill {
                id: "Wifi".into(),
                ..base.clone()
            },
        ),
        (
            "long id",
            Skill {
                id: "a".repeat(49),
                ..base.clone()
            },
        ),
        (
            "blank name",
            Skill {
                name: "  ".into(),
                ..base.clone()
            },
        ),
        (
            "long name",
            Skill {
                name: "n".repeat(61),
                ..base.clone()
            },
        ),
        (
            "newline name",
            Skill {
                name: "a\nb".into(),
                ..base.clone()
            },
        ),
        (
            "13 triggers",
            Skill {
                triggers: (0..13).map(|i| format!("do thing {i}")).collect(),
                ..base.clone()
            },
        ),
        (
            "long trigger",
            Skill {
                triggers: vec!["w ".repeat(61)],
                ..base.clone()
            },
        ),
        (
            "filler-only trigger",
            Skill {
                triggers: vec!["please".into()],
                ..base.clone()
            },
        ),
        (
            "slot-only trigger",
            Skill {
                triggers: vec!["{x}".into()],
                params: vec![param("x", true)],
                ..base.clone()
            },
        ),
        (
            "undeclared slot",
            Skill {
                triggers: vec!["call {who}".into()],
                ..base.clone()
            },
        ),
        (
            "unbalanced",
            Skill {
                triggers: vec!["call {who".into()],
                ..base.clone()
            },
        ),
        (
            "stray close",
            Skill {
                triggers: vec!["call who}".into()],
                ..base.clone()
            },
        ),
        (
            "bad slot name",
            Skill {
                triggers: vec!["call {Who}".into()],
                params: vec![param("who", true)],
                ..base.clone()
            },
        ),
        (
            "slot twice",
            Skill {
                triggers: vec!["call {who} and {who}".into()],
                params: vec![param("who", true)],
                ..base.clone()
            },
        ),
        (
            "long instructions",
            Skill {
                instructions: "i".repeat(2001),
                ..base.clone()
            },
        ),
        (
            "17 apps",
            Skill {
                apps: (0..17).map(|i| format!("com.example.a{i}")).collect(),
                ..base.clone()
            },
        ),
        (
            "app with space",
            Skill {
                apps: vec!["com.example app".into()],
                ..base.clone()
            },
        ),
        (
            "9 params",
            Skill {
                params: (0..9).map(|i| param(&format!("p{i}"), false)).collect(),
                ..base.clone()
            },
        ),
        (
            "digit param",
            Skill {
                params: vec![param("1x", false)],
                ..base.clone()
            },
        ),
        (
            "long param",
            Skill {
                params: vec![param(&"p".repeat(25), false)],
                ..base.clone()
            },
        ),
        (
            "dup param",
            Skill {
                params: vec![param("a", false), param("a", true)],
                ..base.clone()
            },
        ),
        (
            "confined, no apps",
            Skill {
                apps: vec![],
                confine_to_apps: true,
                ..base.clone()
            },
        ),
    ];
    for (what, skill) in cases {
        assert!(skill.validate().is_err(), "{what} should be refused");
    }
    // Exactly at the limits is fine.
    Skill {
        id: "a".repeat(48),
        name: "n".repeat(60),
        triggers: (0..12).map(|i| format!("do thing number {i}")).collect(),
        instructions: "i".repeat(2000),
        apps: (0..16).map(|i| format!("com.example.a{i}")).collect(),
        params: (0..8).map(|i| param(&format!("p_{i}"), false)).collect(),
        ..base
    }
    .validate()
    .unwrap();
    assert!(matches!(
        Skill { triggers: vec!["call {who}".into()], ..wifi_skill() }.validate(),
        Err(SkillError::UndeclaredSlot { slot, .. }) if slot == "who"
    ));
}

#[test]
fn store_crud_keeps_a_stable_order_and_unique_ids() {
    let mut s = SkillStore::new();
    s.add(wifi_skill()).unwrap();
    s.add(text_skill()).unwrap();
    let ids: Vec<&str> = s.list().iter().map(|k| k.id.as_str()).collect();
    assert_eq!(ids, vec!["text-someone", "wifi-off"]);
    assert_eq!(
        s.add(wifi_skill()),
        Err(SkillError::DuplicateId("wifi-off".into()))
    );
    assert!(matches!(
        s.add(Skill {
            id: "Bad Id".into(),
            ..wifi_skill()
        }),
        Err(SkillError::InvalidId(_))
    ));

    s.record_run("wifi-off", 500).unwrap();
    s.record_run("wifi-off", 700).unwrap();
    // Updating keeps the bookkeeping, replaces the definition.
    let mut edited = wifi_skill();
    edited.name = "Wi-Fi off now".into();
    edited.updated_ms = 900;
    edited.runs = 0;
    s.update(edited).unwrap();
    let w = s.get("wifi-off").unwrap();
    assert_eq!(
        (w.name.as_str(), w.runs, w.last_run_ms, w.updated_ms),
        ("Wi-Fi off now", 2, Some(700), 900)
    );
    assert_eq!(
        s.update(Skill {
            id: "nope".into(),
            ..wifi_skill()
        }),
        Err(SkillError::NotFound("nope".into()))
    );
    assert_eq!(
        s.record_run("nope", 1),
        Err(SkillError::NotFound("nope".into()))
    );

    assert_eq!(
        s.upsert(Skill {
            id: "aaa".into(),
            ..wifi_skill()
        })
        .unwrap(),
        Upsert::Inserted
    );
    assert_eq!(
        s.upsert(Skill {
            id: "aaa".into(),
            name: "A".into(),
            ..wifi_skill()
        })
        .unwrap(),
        Upsert::Updated
    );
    assert_eq!(s.list()[0].id, "aaa");
    assert_eq!(s.remove("aaa").unwrap().name, "A");
    assert!(s.remove("aaa").is_none());
    assert_eq!(s.len(), 2);
}

#[test]
fn store_is_capped() {
    let mut s = SkillStore::new();
    for i in 0..MAX_SKILLS {
        s.add(Skill {
            id: format!("s{i}"),
            ..wifi_skill()
        })
        .unwrap();
    }
    assert_eq!(
        s.add(Skill {
            id: "one-more".into(),
            ..wifi_skill()
        }),
        Err(SkillError::StoreFull { max: MAX_SKILLS })
    );
}

#[test]
fn json_snapshot_round_trips_and_is_versioned() {
    let mut s = store_with(vec![text_skill(), wifi_skill()]);
    s.record_run("text-someone", 42).unwrap();
    let json = s.to_json().unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["skills"][0]["kind"], "written");
    assert_eq!(v["skills"][0]["last_run_ms"], 42);
    assert_eq!(SkillStore::from_json(&json).unwrap(), s);
    // Empty and section-less snapshots load as empty stores.
    assert!(SkillStore::from_json("{}").unwrap().is_empty());
    assert!(SkillStore::from_json(r#"{"skills":null}"#)
        .unwrap()
        .is_empty());
}

#[test]
fn unknown_fields_and_missing_fields_are_tolerated() {
    let json = r#"{
        "schema_version": 1,
        "future_section": {"x": 1},
        "skills": [
            {"id": "wifi-off", "name": "Wi-Fi off", "triggers": ["turn off wifi"],
             "colour": "blue", "kind": "built_in"}
        ]
    }"#;
    let s = SkillStore::from_json(json).unwrap();
    let k = s.get("wifi-off").unwrap();
    assert_eq!(k.kind, SkillKind::BuiltIn);
    assert!(k.enabled, "enabled defaults to true");
    assert!(k.params.is_empty() && k.last_run_ms.is_none());
}

#[test]
fn corrupt_snapshots_are_typed_errors_and_bad_entries_are_skipped() {
    for bad in [
        "",
        "not json",
        "[1,2]",
        r#"{"skills": 3}"#,
        r#"{"schema_version": "one"}"#,
        "{\"skills\": [",
        "\u{0}",
    ] {
        let e = SkillStore::from_json(bad).unwrap_err();
        assert!(
            matches!(e, SkillError::Json(_) | SkillError::Invalid(_)),
            "{bad:?}: {e:?}"
        );
    }
    assert_eq!(
        SkillStore::from_json(r#"{"schema_version": 2, "skills": []}"#).unwrap_err(),
        SkillError::NewerSchema {
            found: 2,
            supported: 1
        }
    );
    let json = r#"{"skills": [
        {"id": "ok", "name": "Fine", "triggers": ["do it"]},
        {"id": "BAD", "name": "Bad id"},
        {"id": "ok", "name": "Duplicate"},
        {"id": 7},
        "string",
        {"id": "also-ok", "name": "Also fine", "triggers": ["call {who}"]}
    ]}"#;
    let (s, issues) = SkillStore::from_json_with_issues(json).unwrap();
    let ids: Vec<&str> = s.list().iter().map(|k| k.id.as_str()).collect();
    assert_eq!(ids, vec!["ok"]);
    assert_eq!(s.get("ok").unwrap().name, "Fine");
    assert_eq!(issues.len(), 5, "{issues:#?}");
    assert!(issues
        .iter()
        .any(|i| i.starts_with("skills[5]") && i.contains("{who}")));
}

#[test]
fn a_megabyte_snapshot_is_refused_or_loaded_quickly() {
    let huge = format!(
        r#"{{"skills":[{{"id":"x","name":"{}","triggers":["{}"]}}]}}"#,
        "n".repeat(1_000_000),
        "t ".repeat(500_000)
    );
    let started = std::time::Instant::now();
    let (s, issues) = SkillStore::from_json_with_issues(&huge).unwrap();
    assert!(s.is_empty());
    assert_eq!(issues.len(), 1);
    assert!(started.elapsed().as_secs() < 2, "{:?}", started.elapsed());
}

#[test]
fn context_carries_declared_args_only() {
    let skill = text_skill();
    let ctx = skill.context(args(&[
        ("contact", " Mom "),
        ("message", "on my way"),
        ("evil", "ignored"),
        ("blank", ""),
    ]));
    assert_eq!(ctx.id, "text-someone");
    assert_eq!(ctx.name, "Text someone");
    assert!(ctx.confine_to_apps);
    assert_eq!(ctx.apps, vec![MESSAGES.to_owned()]);
    assert_eq!(
        ctx.args,
        args(&[("contact", "Mom"), ("message", "on my way")])
    );
    assert_eq!(ctx.instructions, skill.instructions);
    let long = skill.context(args(&[("contact", &"x".repeat(1000))]));
    assert_eq!(long.args["contact"].chars().count(), 200);
}

#[test]
fn task_text_fills_the_first_fillable_trigger() {
    let skill = text_skill();
    assert_eq!(
        skill.task_text(&args(&[("contact", "Mom"), ("message", "on my way {x}")])),
        "text Mom on my way {x}"
    );
    assert_eq!(
        skill.task_text(&args(&[("contact", "Mom")])),
        "Text someone"
    );
    assert_eq!(wifi_skill().task_text(&BTreeMap::new()), "turn off wi-fi");
    assert_eq!(
        skill.missing_params(&args(&[("contact", "Mom")])),
        vec!["message"]
    );
}

fn flow(key: &str, app: &str) -> Flow {
    Flow {
        id: String::new(),
        app_bundle_id: app.into(),
        intent_key: key.into(),
        steps: vec![FlowStep {
            fingerprint: Some(1),
            role: Some(Role::Switch),
            label: "Wi-Fi".into(),
            app_bundle_id: app.into(),
            action: FlowAction::Tap,
            screen_hash_before: 0,
        }],
        successes: 2,
        failures: 0,
        last_used_ms: 5,
        retired: false,
    }
}

#[test]
fn promote_flow_builds_a_learned_skill_that_matches_its_task() {
    let mut s = store_with(vec![wifi_skill()]);
    let id = s
        .promote_flow(&flow("text mom {1}", MESSAGES), "Text Mom", 1_000)
        .unwrap();
    assert_eq!(id, "text-mom");
    let k = s.get(&id).unwrap().clone();
    assert_eq!(k.kind, SkillKind::Learned);
    assert_eq!(k.triggers, vec!["text mom {slot1}".to_owned()]);
    assert_eq!(k.params.len(), 1);
    assert!(k.params[0].required);
    assert_eq!(k.flow_keys, vec!["text mom {1}".to_owned()]);
    assert_eq!(k.apps, vec![MESSAGES.to_owned()]);
    assert!(!k.confine_to_apps);
    assert_eq!((k.created_ms, k.updated_ms), (1_000, 1_000));
    // It matches the kind of request the flow was learned from, and its task text has
    // the same intent key as the flow.
    let m = s.match_task("Text Mom running late");
    assert_eq!(m[0].skill_id, "text-mom");
    assert_eq!(m[0].args["slot1"], "running late");
    let task = k.task_text(&m[0].args);
    assert_eq!(task, "text mom running late");
    assert!(jarvis_memory::match_slotted("text mom {1}", &task).is_some());

    // Same flow again: refused, naming the existing skill.
    assert_eq!(
        s.promote_flow(&flow("text mom {1}", MESSAGES), "Again", 2),
        Err(SkillError::AlreadyPromoted("text-mom".into()))
    );
    // A name that collides gets a suffix; a blank name uses the key.
    let id2 = s
        .promote_flow(&flow("disable wifi", SETTINGS), "Wi-Fi off", 3)
        .unwrap();
    assert_eq!(id2, "wi-fi-off");
    let id3 = s
        .promote_flow(&flow("disable bluetooth", SETTINGS), "  ", 4)
        .unwrap();
    assert_eq!(id3, "disable-bluetooth");
    assert_eq!(s.get(&id3).unwrap().name, "disable bluetooth");
    let id4 = s
        .promote_flow(&flow("enable bluetooth", SETTINGS), "Wi-Fi off", 4)
        .unwrap();
    assert_eq!(id4, "wi-fi-off-2");

    let mut retired = flow("open notes", "com.apple.mobilenotes");
    retired.retired = true;
    assert!(matches!(
        s.promote_flow(&retired, "Notes", 1),
        Err(SkillError::Flow(_))
    ));
    assert!(matches!(
        s.promote_flow(&flow("", SETTINGS), "x", 1),
        Err(SkillError::Flow(_))
    ));
    assert!(matches!(
        s.promote_flow(&flow("open notes", ""), "x", 1),
        Err(SkillError::Flow(_))
    ));
}
