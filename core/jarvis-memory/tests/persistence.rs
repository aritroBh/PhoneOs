#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod common;
use common::*;
use jarvis_memory::*;
use jarvis_proto::OutcomeStatus;

fn populated() -> MemoryStore {
    let mut m = MemoryStore::new();
    m.flows
        .upsert(record_text_flow("text mom running late", "running late").unwrap());
    m.flows
        .upsert(record_text_flow("text dad hello", "zzz").unwrap());
    m.profile.set("email", "a@b.c", false);
    m.profile.set("dob", "1990-01-01", true);
    m.log.push(LogEntry {
        task: "t".into(),
        steps: vec![LogStep {
            action: "tap".into(),
            app: "a".into(),
            ok: false,
        }],
        outcome: OutcomeStatus::Failed,
        summary: "s".into(),
        started_ms: 1,
        finished_ms: 9,
    });
    m
}

#[test]
fn json_round_trip_is_lossless_and_deterministic() {
    let m = populated();
    let json = m.to_json().unwrap();
    assert_eq!(json, m.to_json().unwrap());
    let back = MemoryStore::from_json(&json).unwrap();
    assert_eq!(back, m);
    assert_eq!(back.to_json().unwrap(), json);
    assert!(json.contains(r#""schema_version":1"#));
}

#[test]
fn unknown_fields_are_ignored_everywhere() {
    let m = populated();
    let mut v: serde_json::Value = serde_json::from_str(&m.to_json().unwrap()).unwrap();
    v["future_section"] = serde_json::json!({"a": 1});
    v["flows"]["flows"][0]["new_field"] = serde_json::json!(true);
    v["flows"]["flows"][0]["steps"][0]["extra"] = serde_json::json!([1, 2]);
    v["profile"]["fields"]["email"]["note"] = serde_json::json!("x");
    v["log"]["entries"][0]["extra"] = serde_json::json!(null);
    let back = MemoryStore::from_json(&v.to_string()).unwrap();
    assert_eq!(back, m);
}

#[test]
fn missing_sections_default() {
    let empty = MemoryStore::from_json("{}").unwrap();
    assert_eq!(empty, MemoryStore::new());
    let only_profile =
        MemoryStore::from_json(r#"{"profile":{"fields":{"city":{"value":"Oslo"}}}}"#).unwrap();
    assert_eq!(only_profile.profile.lookup("city").unwrap().value, "Oslo");
    assert!(only_profile.flows.is_empty() && only_profile.log.is_empty());
    assert!(MemoryStore::from_json(r#"{"flows":null,"log":null,"profile":null}"#).is_ok());
}

#[test]
fn newer_schema_is_rejected_clearly() {
    let err = MemoryStore::from_json(r#"{"schema_version":2,"flows":{}}"#).unwrap_err();
    assert!(matches!(
        err,
        MemoryError::NewerSchema {
            found: 2,
            supported: 1
        }
    ));
    assert!(err.to_string().contains("newer"));
    assert!(MemoryStore::from_json(r#"{"schema_version":0}"#).is_ok());
    assert!(matches!(
        MemoryStore::from_json(r#"{"schema_version":"1"}"#),
        Err(MemoryError::Invalid(_))
    ));
}

#[test]
fn garbage_never_panics() {
    let inputs = [
        "",
        " ",
        "null",
        "42",
        "[]",
        "\"x\"",
        "{",
        "}",
        "{\"flows\":1}",
        "{\"flows\":{\"flows\":3}}",
        "{\"profile\":{\"fields\":[]}}",
        "{\"log\":{\"entries\":{}}}",
        "\u{0}\u{1}\u{2}",
        "{\"schema_version\":-1}",
        "{\"schema_version\":1e99}",
        "{\"flows\":{\"flows\":[null,1,\"x\",{},{\"id\":5}]}}",
    ];
    for i in inputs {
        let _ = MemoryStore::from_json(i);
    }
    let deep = "[".repeat(100_000);
    assert!(MemoryStore::from_json(&deep).is_err());
    let deep_obj = format!("{}1{}", "{\"a\":".repeat(5000), "}".repeat(5000));
    assert!(MemoryStore::from_json(&deep_obj).is_err());
    for bytes in 0u8..=255 {
        let s = String::from_utf8_lossy(&[bytes, b'{', bytes]).into_owned();
        let _ = MemoryStore::from_json(&s);
    }
}

#[test]
fn unreadable_items_are_skipped_not_fatal() {
    let m = populated();
    let mut v: serde_json::Value = serde_json::from_str(&m.to_json().unwrap()).unwrap();
    // A step type from a newer minor version, and a nonsense entry.
    v["flows"]["flows"][0]["steps"][0]["action"] = serde_json::json!({"type": "teleport"});
    v["flows"]["flows"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!("junk"));
    v["profile"]["fields"]["bad"] = serde_json::json!(7);
    let back = MemoryStore::from_json(&v.to_string()).unwrap();
    assert_eq!(back.flows.len(), 1);
    assert!(back.profile.lookup("bad").is_none() && back.profile.lookup("email").is_some());
}

#[test]
fn oversized_flow_list_is_trimmed_on_load() {
    let f = record_text_flow("text mom hi", "zzz").unwrap();
    let flows: Vec<String> = (0..MAX_FLOWS_TEST + 5)
        .map(|i| {
            let mut g = f.clone();
            g.intent_key = format!("task {i:04}");
            g.last_used_ms = i as u64;
            serde_json::to_string(&g).unwrap()
        })
        .collect();
    let json = format!(r#"{{"flows":{{"flows":[{}]}}}}"#, flows.join(","));
    let store = MemoryStore::from_json(&json).unwrap();
    assert_eq!(store.flows.len(), 200);
    assert!(store.flows.find("task 0000").is_none(), "oldest evicted");
}
const MAX_FLOWS_TEST: usize = 200;

#[test]
fn fifty_thousand_flows_load_quickly_and_are_bounded() {
    let f = record_text_flow("text mom hi", "zzz").unwrap();
    let one = serde_json::to_string(&f).unwrap();
    let json = format!(
        r#"{{"flows":{{"flows":[{}]}}}}"#,
        vec![one; 50_000].join(",")
    );
    let t = std::time::Instant::now();
    let store = MemoryStore::from_json(&json).unwrap();
    assert!(t.elapsed().as_secs() < 5);
    assert!(store.flows.len() <= 200);
}

#[test]
fn loaded_flows_are_revalidated() {
    let f = record_text_flow("text mom hi", "zzz").unwrap();
    let mut bad_id = f.clone();
    bad_id.id = "forged".into();
    bad_id.failures = 5; // should be retired
    let mut empty = f.clone();
    empty.intent_key = "other".into();
    empty.steps.clear();
    let mut long = f.clone();
    long.intent_key = "long".into();
    long.steps = vec![f.steps[0].clone(); 65];
    let json = serde_json::json!({"flows":{"flows":[bad_id, empty, long]}}).to_string();
    let store = MemoryStore::from_json(&json).unwrap();
    assert_eq!(store.flows.len(), 1);
    let loaded = &store.flows.flows()[0];
    assert_eq!(
        loaded.id,
        flow_id(&loaded.intent_key, &loaded.app_bundle_id)
    );
    assert!(loaded.retired);
}
