#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use jarvis_memory::log::*;
use jarvis_memory::*;
use jarvis_proto::OutcomeStatus;

fn entry(task: &str) -> LogEntry {
    LogEntry {
        task: task.into(),
        steps: vec![LogStep {
            action: "tap".into(),
            app: "com.a".into(),
            ok: true,
        }],
        outcome: OutcomeStatus::Success,
        summary: "done".into(),
        started_ms: 1,
        finished_ms: 2,
        lock_waits: 0,
        locked_ms: 0,
    }
}

#[test]
fn ring_drops_oldest() {
    let mut log = TaskLog::with_capacity(3);
    for i in 0..5 {
        log.push(entry(&format!("task {i}")));
    }
    let tasks: Vec<_> = log.entries().iter().map(|e| e.task.as_str()).collect();
    assert_eq!(tasks, ["task 2", "task 3", "task 4"]);
}

#[test]
fn strings_and_steps_are_capped() {
    let mut log = TaskLog::new();
    let mut e = entry(&"é".repeat(MAX_TEXT_CHARS * 3));
    e.summary = "s".repeat(10_000);
    e.steps = (0..MAX_STEPS_PER_ENTRY + 50)
        .map(|i| LogStep {
            action: format!("{i}{}", "a".repeat(500)),
            app: "p".repeat(500),
            ok: i % 2 == 0,
        })
        .collect();
    log.push(e);
    let e = &log.entries()[0];
    assert_eq!(e.task.chars().count(), MAX_TEXT_CHARS);
    assert_eq!(e.summary.chars().count(), MAX_TEXT_CHARS);
    assert_eq!(e.steps.len(), MAX_STEPS_PER_ENTRY);
    assert!(
        e.steps[0].action.starts_with("50"),
        "most recent steps kept"
    );
    assert!(e
        .steps
        .iter()
        .all(|s| s.action.chars().count() <= MAX_NAME_CHARS
            && s.app.chars().count() <= MAX_NAME_CHARS));
}

#[test]
fn default_capacity_and_zero_capacity() {
    let mut log = TaskLog::new();
    for i in 0..MAX_ENTRIES + 10 {
        log.push(entry(&i.to_string()));
    }
    assert_eq!(log.len(), MAX_ENTRIES);
    let mut one = TaskLog::with_capacity(0);
    one.push(entry("a"));
    one.push(entry("b"));
    assert_eq!(one.len(), 1);
}

#[test]
fn loading_applies_caps_to_oversized_files() {
    let entries: Vec<String> = (0..MAX_ENTRIES + 20)
        .map(|i| format!(r#"{{"task":"t{i}","outcome":"failed"}}"#))
        .collect();
    let json = format!(r#"{{"log":{{"entries":[{}]}}}}"#, entries.join(","));
    let store = MemoryStore::from_json(&json).unwrap();
    assert_eq!(store.log.len(), MAX_ENTRIES);
    assert_eq!(store.log.entries()[0].task, "t20");
}
