#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Twelfth review (F1): the "typed here" state is set where a `TypeText` is **sent**, in
//! `Session::emit`, the one function that builds `Effect::Act`. This is a source scan over
//! `core/jarvis-agent/src` (an accepted structural check: Rust cannot make an enum variant
//! of another crate private): it fails if `Effect::Act` is named anywhere but in `emit`, or
//! if `Effect` variants are imported unqualified, so a runner command (a `TypeText` among
//! them) cannot reach the shell without passing through the function that sets the state.

use std::path::Path;

fn sources() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).unwrap();
            out.push((path.display().to_string(), text));
        }
    }
    assert!(
        out.len() >= 5,
        "jarvis-agent sources not found in {}",
        dir.display()
    );
    out
}

#[test]
fn only_emit_builds_runner_effects() {
    let mut sites = Vec::new();
    for (path, text) in sources() {
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            if code.contains("Effect::Act") {
                sites.push(format!("{path}:{}", n + 1));
            }
            assert!(
                !(code.contains("use ") && code.contains("Effect::")),
                "{path}:{}: Effect variants imported unqualified: {line}",
                n + 1
            );
            assert!(
                !code.contains("Effect as "),
                "{path}:{}: Effect re-imported under another name: {line}",
                n + 1
            );
        }
    }
    assert_eq!(
        sites.len(),
        1,
        "Effect::Act built outside Session::emit: {sites:#?}"
    );
    let (path, text) = sources()
        .into_iter()
        .find(|(_, t)| t.contains("fn emit(&mut self, cmd: RunnerCommand"))
        .expect("Session::emit exists");
    let start = text.find("fn emit(&mut self, cmd: RunnerCommand").unwrap();
    let body = &text[start..];
    let end = body.find("\n    }\n").unwrap();
    assert!(
        body[..end].contains("Effect::Act(cmd)") && body[..end].contains("self.typed_sent("),
        "{path}: emit must set the typed-here state and build the effect"
    );
}
