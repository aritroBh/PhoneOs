//! Review pass 35: crashes and unbounded work reachable from a model reply; provenance of
//! what a remote-started task leaves in memory.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Instant;

use common::*;
use jarvis_agent::parse_model_output;
use jarvis_memory::MemoryStore;
use jarvis_proto::{RiskCategory, RunnerCommand, Screen, TaskOrigin, TaskSpec};

const SHOP: &str = "com.example.shop";

/// F4 (medium): a task a paired device started asks the first-use question for an app
/// as an ordinary, remotely approvable question (FirstUse is not in
/// `NEVER_REMOTE_APPROVE` and the request is not phone only), and the approval is written
/// into memory for good: one remote "yes" (a device holding `approve`) removes the
/// first-use question for every later task, the owner's own included. Either the question
/// should be phone only for a remote-started task, or its approval should cover that task
/// only and not be kept.
#[test]
fn f4_remote_task_first_use_approval_is_kept_for_good() {
    let phone = Phone::new(product_screen(), |cmd: &RunnerCommand, s: &Screen| {
        if taps(cmd, s, "Add to wishlist") {
            Transition::Go(home_screen())
        } else {
            Transition::Stay
        }
    });
    let wish = format!(
        r#"{{"action":"tap","element":{}}}"#,
        index_of(&product_screen(), "Add to wishlist")
    );
    let r = run_spec(
        first_use_config(),
        TaskSpec::new("t1", "Save the headphones").from_origin(TaskOrigin::Remote),
        MemoryStore::new(),
        phone,
        Script::model(&[&wish, r#"{"action":"done","summary":"ok"}"#]).decisions(&[true]),
    );
    let first_use: Vec<_> = r
        .confirms
        .iter()
        .filter(|c| c.category == RiskCategory::FirstUse)
        .collect();
    assert_eq!(first_use.len(), 1, "{:?}", r.confirms);
    let phone_only = first_use[0].phone_only;
    let kept = r.memory.is_app_approved(SHOP);
    assert!(
        phone_only || !kept,
        "remote-started task: first-use question phone_only={phone_only}, approval kept in \
         memory={kept} (outcome {:?})",
        r.outcome.status
    );
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n as u64)).unwrap()
    }
}

const PIECES: &[&str] = &[
    "{",
    "}",
    "[",
    "]",
    "\"",
    "'",
    ":",
    ",",
    "\\",
    "\\u",
    "\\ud800",
    "\\udfff",
    "action",
    "tap",
    "type",
    "element",
    "text",
    "submit",
    "true",
    "None",
    "1e999",
    "-0",
    "NaN",
    "Infinity",
    "18446744073709551616",
    "4294967296",
    "-1",
    "0.5",
    "args",
    "arguments",
    "call_tool",
    "name",
    "(",
    ")",
    "=",
    " ",
    "\n",
    "é",
    "😀",
    "\u{0}",
    "{{profile.",
    "}}",
    "actions",
    "done",
    "summary",
    "open_url",
    "url",
    "ms",
    "wait",
    "swipe",
    "direction",
    "x",
    "y",
    "tap_point",
    "1.0e308",
];

fn random_reply(rng: &mut Rng, len: usize) -> String {
    let mut s = String::new();
    for _ in 0..len {
        s.push_str(PIECES[rng.below(PIECES.len())]);
    }
    s
}

/// Sound: ten thousand random replies built from JSON and call-syntax pieces never panic.
#[test]
fn sound_parser_never_panics_on_random_replies() {
    let mut rng = Rng(0x35_35_35_35);
    for i in 0..10_000 {
        let reply = random_reply(&mut rng, 1 + i % 60);
        let r = catch_unwind(AssertUnwindSafe(|| {
            let _ = parse_model_output(&reply);
        }));
        assert!(r.is_ok(), "parser panicked on {reply:?}");
    }
}

/// Sound: hostile shapes (deep nesting, huge numbers, long unterminated strings, many
/// calls) are parsed in bounded time.
#[test]
fn sound_parser_time_is_bounded_on_hostile_shapes() {
    let shapes: Vec<String> = vec![
        "[".repeat(200_000),
        "{".repeat(200_000),
        format!("{{\"action\":\"tap\",\"element\":{}}}", "9".repeat(100_000)),
        "tap([".repeat(5_000),
        "tap(\"".repeat(5_000),
        "{\"a\":".repeat(5_000),
        "{}".repeat(50_000),
        "{} tap(1)".repeat(5_000),
        format!("{}tap(1)", "{\"action\":\"wait\"} ".repeat(3_000)),
        "tap(1,".repeat(5_000),
        "tap(x=[1,".repeat(5_000),
    ];
    for s in &shapes {
        let t = Instant::now();
        let r = catch_unwind(AssertUnwindSafe(|| {
            let _ = parse_model_output(s);
        }));
        let ms = t.elapsed().as_millis();
        assert!(
            r.is_ok(),
            "parser panicked on a shape starting {:?}",
            &s[..20]
        );
        println!("{:?}...: {ms} ms", &s[..12]);
        assert!(ms < 5_000, "{ms} ms on a shape starting {:?}", &s[..20]);
    }
}
