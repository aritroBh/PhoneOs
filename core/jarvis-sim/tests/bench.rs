//! `jarvis-sim bench`, driven through `cli::main_with` on tiny inputs.
//!
//! These tests run in a debug build, where the numbers mean nothing, so they check that
//! every stage runs and is reported, not how long it takes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::PathBuf;

use jarvis_sim::cli::main_with;

fn cli(args: &[&str]) -> (i32, String, String) {
    let args: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = main_with(&args, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

fn tmp(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

const STAGES: [&str; 24] = [
    "ingest.runner_text",
    "compact",
    "digest",
    "safe_copy",
    "render.full",
    "render.budget",
    "tap.one",
    "tap.sweep",
    "classify.tap",
    "classify.type",
    "classify.swipe",
    "classify.return",
    "evaluate.tap",
    "evaluate.type",
    "evaluate.swipe",
    "evaluate.return",
    "gate",
    "summary.tap",
    "recheck.holds_on",
    "recheck.core",
    "step.observe_infer",
    "step.reply_act",
    "step.verify_infer",
    "step.approved_look",
];

#[test]
fn a_small_run_times_every_stage_on_fixtures_and_synthetic_trees() {
    let json_path = tmp("bench-smoke.json");
    let (code, out, err) = cli(&[
        "bench",
        "--sizes",
        "300",
        "--views",
        "page,dense",
        "--iters",
        "2",
        "--warmup",
        "1",
        "--sweep-cap-s",
        "0.001",
        "--no-memory",
        "--json",
        json_path.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("real iOS 26 trees"), "{out}");
    assert!(out.contains("prompt size"), "{out}");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&json_path).unwrap()).unwrap();
    let rows = json["rows"].as_array().unwrap();
    let cases = json["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 10 + 3 * 2);
    for case in cases {
        let id = case["id"].as_str().unwrap();
        for stage in STAGES {
            let row = rows
                .iter()
                .find(|r| r["case"] == id && r["stage"] == stage)
                .unwrap_or_else(|| panic!("{id}: no row for {stage}"));
            assert!(row["p50_us"].is_number(), "{id} {stage}: {row}");
        }
    }
    // The scaling table needs 10k nodes: none here.
    assert!(json["scaling"].as_array().unwrap().is_empty());
}

#[test]
fn a_dense_tree_lists_its_nodes_and_a_page_tree_hides_most_of_them() {
    let json_path = tmp("bench-counts.json");
    let (code, _, err) = cli(&[
        "bench",
        "--no-fixtures",
        "--no-memory",
        "--sizes",
        "1000",
        "--shapes",
        "list",
        "--stages",
        "digest",
        "--iters",
        "1",
        "--json",
        json_path.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{err}");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&json_path).unwrap()).unwrap();
    let get = |id: &str, key: &str| -> u64 {
        json["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id)
            .unwrap()[key]
            .as_u64()
            .unwrap()
    };
    assert!(get("list-1000-page", "elements") < 60);
    assert!(get("list-1000-dense", "elements") > 200);
    assert_eq!(
        get("list-1000-page", "nodes"),
        get("list-1000-dense", "nodes")
    );
}

#[test]
fn the_command_is_documented_and_rejects_unknown_options() {
    let (code, out, _) = cli(&["bench", "--help"]);
    assert_eq!(code, 0);
    assert!(out.contains("--check") && out.contains("--json"));
    let (code, _, err) = cli(&["bench", "--bogus", "1"]);
    assert_eq!(code, 2);
    assert!(err.contains("unknown option --bogus"), "{err}");
    let (code, _, err) = cli(&["bench", "--shapes", "tree"]);
    assert_eq!(code, 2);
    assert!(err.contains("unknown shape"), "{err}");
}

#[test]
fn check_refuses_a_debug_build() {
    if cfg!(debug_assertions) {
        let (code, _, err) = cli(&["bench", "--check", "--no-fixtures", "--sizes", "100"]);
        assert_eq!(code, 2);
        assert!(err.contains("release build"), "{err}");
    }
}
