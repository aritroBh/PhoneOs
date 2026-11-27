//! The `jarvis-sim` command line, driven through `cli::main_with`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::PathBuf;

use jarvis_sim::builtin::{builtin_tasks, TASK_SOURCES};
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

#[test]
fn apps_lists_every_app_with_its_screens_and_alerts() {
    let (code, out, _) = cli(&["apps"]);
    assert_eq!(code, 0);
    for needle in [
        "settings",
        "com.apple.Preferences",
        "screen wifi",
        "messages",
        "screen thread",
        "acme",
        "alert  wrong_password",
        "shop",
        "screen cart",
        "notes",
        "alert  delete_note",
        "passwords",
        "home",
    ] {
        assert!(out.contains(needle), "missing {needle:?} in:\n{out}");
    }
}

#[test]
fn dump_prints_what_the_model_would_see() {
    let (code, out, _) = cli(&[
        "dump",
        "--app",
        "shop",
        "--screen",
        "cart",
        "--set",
        r#"shop.cart=[{"name":"Desk Lamp","price":35.5}]"#,
        "--set",
        "shop.total=35.5",
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("App: Shop (com.example.shop)"), "{out}");
    assert!(out.contains("Cell \"Desk Lamp, $35.50\""), "{out}");
    assert!(out.contains("Button \"Place Order\""), "{out}");
    let (code, out, _) = cli(&[
        "dump",
        "--app",
        "acme",
        "--screen",
        "login",
        "--mode",
        "runner-text",
    ]);
    assert_eq!(code, 0);
    assert!(out.contains("runner-text mode"));
    assert!(out.contains("SecureTextField \"Password\""), "{out}");
    // No `disabled` flag in runner-text mode.
    assert!(!out.contains("disabled"), "{out}");
    let (_, structured, _) = cli(&["dump", "--app", "acme", "--screen", "login"]);
    assert!(
        structured.contains("Button \"Sign in\" (disabled)"),
        "{structured}"
    );
}

#[test]
fn dump_rejects_unknown_apps_and_screens() {
    let (code, _, err) = cli(&["dump", "--app", "nope", "--screen", "x"]);
    assert_eq!(code, 2);
    assert!(err.contains("unknown app"), "{err}");
    let (code, _, err) = cli(&["dump", "--app", "shop", "--screen", "nope"]);
    assert_eq!(code, 2);
    assert!(err.contains("no screen"), "{err}");
}

#[test]
fn eval_builtin_passes_every_task_in_both_modes() {
    for mode in ["structured", "runner-text"] {
        let (code, out, err) = cli(&["eval", "builtin", "--mode", mode, "--strict"]);
        assert_eq!(code, 0, "{mode}\n{out}\n{err}");
        let n = builtin_tasks().unwrap().len();
        assert!(n >= 15, "{n} tasks");
        assert!(out.contains(&format!("total: {n}/{n} passed")), "{out}");
        assert!(out.contains("PASS"));
        assert!(!out.contains("FAIL"));
    }
    assert_eq!(TASK_SOURCES.len(), builtin_tasks().unwrap().len());
}

#[test]
fn eval_json_is_machine_readable_and_strict_controls_the_exit_code() {
    let (code, out, _) = cli(&["eval", "builtin", "--json", "--filter", "wifi"]);
    assert_eq!(code, 0);
    // One JSON object per finished task, then the totals: a killed run keeps what finished.
    let lines: Vec<serde_json::Value> = out
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let v = &lines[0];
    assert_eq!(v["id"], "wifi_off");
    assert_eq!(v["passed"], true);
    assert_eq!(v["model_calls"], 3);
    // The transcript: every prompt, reply and effect of the judged run.
    let t = &v["transcript"];
    assert!(t["system"].as_str().unwrap().starts_with("You are Jarvis."));
    assert_eq!(t["prompts"].as_array().unwrap().len(), 3);
    assert_eq!(t["replies"].as_array().unwrap().len(), 3);
    assert!(t["prompts"][1]
        .as_str()
        .unwrap()
        .contains("-> ok; now in Settings > Wi-Fi"));
    assert!(t["effects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e.as_str().unwrap().contains("confirm [FirstUse]")));
    let totals = lines.last().unwrap();
    assert_eq!(totals["total"], lines.len() - 1);
    assert_eq!(totals["passed"], lines.len() - 1);
    // Forcing the owner to decline makes the send-message task fail its predicate.
    let (code, out, _) = cli(&["eval", "builtin", "--filter", "text_mom", "--decline"]);
    assert_eq!(
        code, 0,
        "failures do not change the exit code without --strict"
    );
    assert!(out.contains("FAIL"), "{out}");
    let (code, _, _) = cli(&[
        "eval",
        "builtin",
        "--filter",
        "text_mom",
        "--decline",
        "--strict",
    ]);
    assert_eq!(code, 1);
}

#[test]
fn eval_reads_a_task_directory_and_reports_missing_scripts() {
    let dir = tmp("eval-tasks");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("a_open.json"),
        r#"{"id":"open","task":"Open Settings","start_app":"home","success":"phone.app == 'settings'",
            "script":["{\"action\":\"open_app\",\"app\":\"Settings\"}","{\"action\":\"done\",\"summary\":\"ok\"}"]}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("b_noscript.json"),
        r#"{"id":"noscript","task":"Do something","success":"true"}"#,
    )
    .unwrap();
    let (code, out, err) = cli(&["eval", dir.to_str().unwrap(), "--strict"]);
    assert_eq!(code, 1, "{out}\n{err}");
    assert!(out.contains("open") && out.contains("PASS"), "{out}");
    assert!(
        out.contains("noscript") && out.contains("no model"),
        "{out}"
    );
    assert!(out.contains("total: 1/2 passed"), "{out}");
}

#[test]
fn run_uses_the_bundled_script_and_saves_memory_for_a_replay() {
    let mem = tmp("cli-memory.json");
    let _ = std::fs::remove_file(&mem);
    let task = "Text Mom that I'm running late";
    let args = [
        "run",
        "--task",
        task,
        "--start-app",
        "messages",
        "--memory",
        mem.to_str().unwrap(),
    ];
    let (code, first, err) = cli(&args);
    assert_eq!(code, 0, "{first}\n{err}");
    assert!(first.contains("outcome: Success"), "{first}");
    assert!(first.contains("4 model calls"), "{first}");
    assert!(first.contains("memory: 1 flows saved"), "{first}");
    assert!(mem.exists());
    // The second run replays the learned flow: zero model calls.
    let (code, second, _) = cli(&args);
    assert_eq!(code, 0);
    assert!(second.contains("memory flows: 1"), "{second}");
    assert!(second.contains("0 model calls"), "{second}");
    assert!(second.contains("outcome: Success"), "{second}");
    assert!(second.contains("saved routine"), "{second}");
}

#[test]
fn run_accepts_a_script_file_modes_and_owner_flags() {
    let script = tmp("cli-script.jsonl");
    std::fs::write(
        &script,
        "# turn Wi-Fi off\n{\"action\":\"tap\",\"element\":\"@Cell:Wi-Fi\"}\n{\"action\":\"tap\",\"element\":\"@Switch:Wi-Fi\"}\n{\"action\":\"done\",\"summary\":\"off\"}\n",
    )
    .unwrap();
    for mode in ["structured", "runner-text"] {
        let (code, out, err) = cli(&[
            "run",
            "--task",
            "Turn off Wi-Fi",
            "--start-app",
            "settings",
            "--script",
            script.to_str().unwrap(),
            "--mode",
            mode,
            "--approve",
            "--context-tokens",
            "8192",
        ]);
        assert_eq!(code, 0, "{err}");
        assert!(out.contains("outcome: Success"), "{out}");
        assert!(out.contains(&format!("mode: {mode}")), "{out}");
        assert!(out.contains("confirm [SystemSetting]"), "{out}");
        assert!(out.contains("owner approves"), "{out}");
    }
    let (_, out, _) = cli(&[
        "run",
        "--task",
        "Turn off Wi-Fi",
        "--start-app",
        "settings",
        "--script",
        script.to_str().unwrap(),
        "--decline",
        "--approve-app",
        "settings",
    ]);
    assert!(out.contains("owner declines"), "{out}");
    assert!(out.contains("pre-approved: com.apple.Preferences"), "{out}");
    assert!(!out.contains("confirm [FirstUse]"), "{out}");
    assert!(out.contains("final: settings/wifi"), "{out}");
}

#[test]
fn policy_flags_reach_the_session() {
    let script = tmp("cli-policy-script.jsonl");
    std::fs::write(
        &script,
        "{\"action\":\"tap\",\"element\":\"@Cell:Wi-Fi\"}\n{\"action\":\"tap\",\"element\":\"@Switch:Wi-Fi\"}\n{\"action\":\"done\",\"summary\":\"off\"}\n",
    )
    .unwrap();
    let base = [
        "run",
        "--task",
        "Turn off Wi-Fi",
        "--start-app",
        "settings",
        "--script",
        script.to_str().unwrap(),
        "--approve",
    ];
    // Default: first-use approval on, guarded.
    let (code, out, err) = cli(&base);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("policy: guarded, first-use approval on"),
        "{out}"
    );
    assert!(
        out.contains("confirm [FirstUse] Let Jarvis act in Settings (com.apple.Preferences)?"),
        "{out}"
    );
    assert!(!out.contains("confirm [None]"), "{out}");
    // --step-by-step confirms the plain row tap too.
    let mut args = base.to_vec();
    args.push("--step-by-step");
    let (code, out, err) = cli(&args);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("policy: step-by-step"), "{out}");
    assert!(out.contains("confirm [None] Tap"), "{out}");
    assert!(out.contains("outcome: Success"), "{out}");
    // --no-first-use drops the app prompt.
    let mut args = base.to_vec();
    args.push("--no-first-use");
    let (code, out, err) = cli(&args);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("first-use approval off"), "{out}");
    assert!(!out.contains("FirstUse"), "{out}");
    assert!(out.contains("confirm [SystemSetting]"), "{out}");
    // Eval accepts the same flags.
    let (code, out, err) = cli(&[
        "eval",
        "builtin",
        "--filter",
        "wifi_off",
        "--no-first-use",
        "--strict",
    ]);
    assert_eq!(code, 1, "{out}{err}");
    assert!(
        out.contains("no first_use confirmation was requested"),
        "{out}"
    );
}

#[test]
fn usage_errors_exit_with_code_two_and_say_what_is_wrong() {
    let (code, _, err) = cli(&["run", "--task", "Something nobody scripted"]);
    assert_eq!(code, 2);
    assert!(err.contains("no model"), "{err}");
    let (code, _, err) = cli(&["run"]);
    assert_eq!(code, 2);
    assert!(err.contains("--task"), "{err}");
    let (code, _, err) = cli(&["run", "--task", "x", "--bogus", "1"]);
    assert_eq!(code, 2);
    assert!(err.contains("--bogus"), "{err}");
    let (code, _, err) = cli(&["run", "--task", "x", "--mode", "pixels"]);
    assert_eq!(code, 2);
    assert!(err.contains("--mode"), "{err}");
    let (code, _, err) = cli(&[
        "run",
        "--task",
        "x",
        "--approve",
        "--decline",
        "--script",
        "x",
    ]);
    assert_eq!(code, 2);
    assert!(err.contains("mutually exclusive"), "{err}");
    let (code, _, err) = cli(&["frobnicate"]);
    assert_eq!(code, 2);
    assert!(err.contains("frobnicate"), "{err}");
    let (code, out, _) = cli(&["--help"]);
    assert_eq!(code, 0);
    assert!(out.contains("USAGE"));
}

#[cfg(not(feature = "http"))]
#[test]
fn model_url_needs_the_http_feature() {
    let (code, _, err) = cli(&[
        "run",
        "--task",
        "x",
        "--model-url",
        "http://localhost:1/v1",
        "--model",
        "m",
    ]);
    assert_eq!(code, 2);
    assert!(err.contains("--features http"), "{err}");
}

#[cfg(feature = "http")]
#[test]
fn model_url_talks_to_an_openai_compatible_server() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    // A stub server that answers every request with the same `done` action.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        // Read the whole request (headers, then Content-Length bytes of body). A single
        // `read` can return before the body arrives; replying then closes the socket
        // with unread data and the client sees a reset.
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let n = s.read(&mut chunk).unwrap();
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&buf);
            if let Some(head_end) = text.find("\r\n\r\n") {
                let content_length = text[..head_end]
                    .lines()
                    .find_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        k.eq_ignore_ascii_case("content-length")
                            .then(|| v.trim().parse::<usize>().ok())?
                    })
                    .unwrap_or(0);
                if buf.len() >= head_end + 4 + content_length {
                    break;
                }
            }
        }
        let n = buf.len();
        let body = r#"{"choices":[{"message":{"content":"{\"action\":\"done\",\"summary\":\"stub says done\"}"}}]}"#;
        let reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        s.write_all(reply.as_bytes()).unwrap();
        String::from_utf8_lossy(&buf[..n]).into_owned()
    });
    let (code, out, err) = cli(&[
        "run",
        "--task",
        "Anything",
        "--start-app",
        "settings",
        "--model-url",
        &url,
        "--model",
        "stub-model",
    ]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("outcome: Success"), "{out}");
    assert!(out.contains("stub says done"), "{out}");
    let request = server.join().unwrap();
    assert!(request.contains("\"model\":\"stub-model\""), "{request}");
    assert!(request.contains("Anything"), "the task reached the server");
    let (code, _, err) = cli(&["run", "--task", "x", "--model-url", &url]);
    assert_eq!(code, 2);
    assert!(err.contains("--model NAME"), "{err}");
}
