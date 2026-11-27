#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod common;
use common::*;
use jarvis_memory::*;

fn flow(task: &str, app: &str, steps: usize, last_used: u64) -> Flow {
    let mut f = record_text_flow(task, "zzz").unwrap();
    f.app_bundle_id = app.to_owned();
    while f.steps.len() > steps {
        f.steps.pop();
    }
    while f.steps.len() < steps {
        f.steps.push(f.steps[0].clone());
    }
    f.last_used_ms = last_used;
    f
}

#[test]
fn find_exact_match_ignores_fillers_and_case() {
    let mut s = FlowStore::new();
    s.upsert(flow("Text mom running late", MSG, 4, 1));
    assert!(s
        .find("hey jarvis, please TEXT my mom running late!")
        .is_some());
    assert!(s.find("text dad running late").is_none());
    assert!(s.find("").is_none());
}

#[test]
fn upsert_replaces_only_when_not_longer_or_retired() {
    let mut s = FlowStore::new();
    assert_eq!(
        s.upsert(flow("text mom hi", MSG, 4, 1)),
        UpsertOutcome::Inserted
    );
    assert_eq!(
        s.upsert(flow("text mom hi", MSG, 6, 2)),
        UpsertOutcome::Kept
    );
    assert_eq!(s.flows()[0].steps.len(), 4);
    assert_eq!(
        s.upsert(flow("text mom hi", MSG, 4, 3)),
        UpsertOutcome::Replaced
    );
    assert_eq!(
        s.upsert(flow("text mom hi", MSG, 3, 4)),
        UpsertOutcome::Replaced
    );
    assert_eq!(s.flows()[0].steps.len(), 3);
    // different app = different flow
    assert_eq!(
        s.upsert(flow("text mom hi", "com.whatsapp", 9, 5)),
        UpsertOutcome::Inserted
    );
    assert_eq!(s.len(), 2);

    // retire, then a longer flow may replace it
    let id = s
        .flows()
        .iter()
        .find(|f| f.app_bundle_id == MSG)
        .unwrap()
        .id
        .clone();
    s.record_result(&id, false, 10);
    s.record_result(&id, false, 11);
    assert_eq!(s.record_result(&id, false, 12), Some(true));
    assert_eq!(
        s.upsert(flow("text mom hi", MSG, 8, 13)),
        UpsertOutcome::Replaced
    );
    let f = s.get(&id).unwrap();
    assert!(!f.retired && f.steps.len() == 8 && f.failures == 0);
}

#[test]
fn retirement_needs_three_uses_and_more_failures_than_successes() {
    let mut s = FlowStore::new();
    s.upsert(flow("text mom hi", MSG, 4, 1)); // successes = 1
    let id = s.flows()[0].id.clone();
    assert_eq!(s.record_result(&id, false, 2), Some(false)); // 1/1, 2 uses
    assert_eq!(s.record_result(&id, true, 3), Some(false)); // 2/1
    assert_eq!(s.record_result(&id, false, 4), Some(false)); // 2/2, 4 uses, not more
    assert_eq!(s.record_result(&id, false, 5), Some(true)); // 2/3
    assert_eq!(s.get(&id).unwrap().last_used_ms, 5);
    assert!(
        s.find("text mom hi").is_none(),
        "retired flows are not replayed"
    );
    assert_eq!(s.record_result("nope", true, 1), None);
}

#[test]
fn eviction_removes_least_recently_used_and_keeps_new_flow() {
    let mut s = FlowStore::with_capacity(3);
    s.upsert(flow("task one", MSG, 4, 50));
    s.upsert(flow("task two", MSG, 4, 10));
    s.upsert(flow("task three", MSG, 4, 30));
    s.upsert(flow("task four", MSG, 4, 5)); // oldest timestamp, but it is the new one
    assert_eq!(s.len(), 3);
    assert!(s.find("task four").is_some());
    assert!(s.find("task two").is_none(), "LRU evicted");
    assert!(s.find("task one").is_some() && s.find("task three").is_some());
}

#[test]
fn store_is_sorted_by_id_regardless_of_insert_order() {
    let mut a = FlowStore::new();
    let mut b = FlowStore::new();
    for t in ["task one", "task two", "task three"] {
        a.upsert(flow(t, MSG, 4, 1));
    }
    for t in ["task three", "task one", "task two"] {
        b.upsert(flow(t, MSG, 4, 1));
    }
    assert_eq!(a, b);
}

#[test]
fn ambiguous_matches_return_nothing() {
    let mut s = FlowStore::new();
    s.upsert(flow("text mom hi", MSG, 4, 1));
    s.upsert(flow("text mom hi", "com.whatsapp", 4, 1));
    assert!(s.find("text mom hi").is_none());

    let mut s = FlowStore::new();
    let mut a = flow("x", MSG, 4, 1);
    a.intent_key = "open {1} app".into();
    let mut b = flow("x", MSG, 4, 1);
    b.intent_key = "open {1} app".into();
    b.app_bundle_id = "com.other".into();
    s.upsert(a);
    s.upsert(b);
    assert!(s.find("open maps app").is_none());
}

#[test]
fn slotted_flow_is_found_through_the_store() {
    let mut s = FlowStore::new();
    s.upsert(record_text_flow("text mom running late", "running late").unwrap());
    let (f, b) = s.find("Can you text mom I'm stuck in traffic").unwrap();
    assert_eq!(f.intent_key, "text mom {1}");
    assert_eq!(b[&1], "I'm stuck in traffic");
}
