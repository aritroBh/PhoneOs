#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Twelfth review (F1): the "typed here" state is set where a `TypeText` is **sent**.
//!
//! Thirteenth review (L3): the rule rests on the type system first. Every handler of the
//! session returns `act::Out`, whose field is private to `src/act.rs`; the only `Out`
//! carrying a runner command is built by `TypedState::emit`, which records the state (see
//! `src/act.rs`'s module docs for why construction elsewhere is impossible). This scan
//! checks what types cannot: over **every** source file under `src` (recursively), with
//! comments removed by a lexer that knows string and character literals (a `//` inside a
//! string is not a comment), that
//! - `Effect::Act` / an `Act(` variant of `Effect` is named only in `src/act.rs`;
//! - no type alias, `use … as`, or glob import renames `Effect` or imports its variants;
//! - no `Effect` is deserialised in the crate (a JSON `"act"` effect would bypass `emit`);
//! - `-> Effect` appears only on `Session::step` and `Out::into_effect`, and `step`'s body
//!   is only the conversion of what the handlers returned.

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// `text` with comments replaced by spaces (line structure kept); string, raw-string and
/// character literals are kept as they are.
fn strip_comments(text: &str) -> String {
    let c: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        let next = c.get(i + 1).copied();
        if ch == '/' && next == Some('/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if ch == '/' && next == Some('*') {
            let mut depth = 0;
            while i < c.len() {
                if c[i] == '/' && c.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if c[i] == '*' && c.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(if c[i] == '\n' { '\n' } else { ' ' });
                    i += 1;
                }
            }
            continue;
        }
        // Raw strings: r"…", r#"…"#.
        if ch == 'r' && (next == Some('"') || next == Some('#')) {
            let mut j = i + 1;
            let mut hashes = 0;
            while c.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if c.get(j) == Some(&'"') {
                let close: String = std::iter::once('"')
                    .chain(std::iter::repeat_n('#', hashes))
                    .collect();
                let rest: String = c[j + 1..].iter().collect();
                let end = rest.find(&close).map_or(c.len(), |k| {
                    j + 1 + rest[..k].chars().count() + close.chars().count()
                });
                out.extend(&c[i..end]);
                i = end;
                continue;
            }
        }
        if ch == '"' {
            out.push(ch);
            i += 1;
            while i < c.len() {
                out.push(c[i]);
                if c[i] == '\\' {
                    if let Some(&n) = c.get(i + 1) {
                        out.push(n);
                    }
                    i += 2;
                    continue;
                }
                i += 1;
                if c[i - 1] == '"' {
                    break;
                }
            }
            continue;
        }
        // A character literal ('"', '/', '\''); a lifetime ('a) is left alone.
        if ch == '\'' {
            let lit = match (c.get(i + 1), c.get(i + 2), c.get(i + 3)) {
                (Some('\\'), _, _) => c[i + 2..].iter().position(|&x| x == '\'').map(|k| k + 3),
                (Some(_), Some('\''), _) => Some(3),
                _ => None,
            };
            if let Some(n) = lit {
                out.extend(&c[i..(i + n).min(c.len())]);
                i += n;
                continue;
            }
        }
        out.push(ch);
        i += 1;
    }
    out
}

fn sources() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&dir, &mut files);
    files.sort();
    let out: Vec<(String, String)> = files
        .into_iter()
        .map(|p| {
            let text = std::fs::read_to_string(&p).unwrap();
            (p.display().to_string(), strip_comments(&text))
        })
        .collect();
    assert!(
        out.len() >= 5,
        "jarvis-agent sources not found in {}",
        dir.display()
    );
    out
}

#[test]
fn the_comment_stripper_knows_strings() {
    let code = "let a = \"// not a comment Effect::Act\"; // Effect::Act\nlet b = '\"'; /* x */ c";
    let s = strip_comments(code);
    assert!(s.contains("\"// not a comment Effect::Act\""), "{s}");
    assert_eq!(s.matches("Effect::Act").count(), 1, "{s}");
    assert!(s.contains("let b = '\"';"), "{s}");
    assert!(!s.contains("x */"), "{s}");
    let raw = "let r = r#\"// Effect::Act \"#; // gone";
    let s = strip_comments(raw);
    assert!(s.contains("r#\"// Effect::Act \"#"), "{s}");
    assert!(!s.contains("gone"), "{s}");
}

#[test]
fn only_emit_builds_runner_effects() {
    let mut act_sites = Vec::new();
    let mut returns = Vec::new();
    for (path, code) in sources() {
        let in_act = path.ends_with("act.rs");
        for (n, line) in code.lines().enumerate() {
            let at = format!("{path}:{}", n + 1);
            let squashed: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            if squashed.contains("Effect::Act") || squashed.contains("Effect::<") {
                act_sites.push(at.clone());
            }
            assert!(
                !(squashed.starts_with("use") && squashed.contains("Effect::")),
                "{at}: Effect variants imported unqualified: {line}"
            );
            assert!(
                !squashed.contains("Effectas"),
                "{at}: Effect re-imported under another name: {line}"
            );
            assert!(
                !(squashed.starts_with("type")
                    && squashed.contains("=")
                    && squashed.contains("Effect")),
                "{at}: a type alias names Effect: {line}"
            );
            assert!(
                !squashed.contains("Effect>(") && !squashed.contains(":Effect=serde"),
                "{at}: an Effect is deserialised: {line}"
            );
            if squashed.contains("->Effect") {
                returns.push(at);
            }
        }
        if !in_act {
            assert!(
                !code.contains("Act("),
                "{path}: a runner effect is built outside src/act.rs"
            );
        }
    }
    assert_eq!(
        act_sites.len(),
        1,
        "Effect::Act built outside TypedState::emit: {act_sites:#?}"
    );
    assert!(act_sites[0].contains("act.rs"), "{act_sites:?}");
    assert_eq!(returns.len(), 2, "functions returning Effect: {returns:#?}");
    let all = sources();
    let (_, act) = all.iter().find(|(p, _)| p.ends_with("act.rs")).unwrap();
    let start = act.find("pub(crate) fn emit(").unwrap();
    let body = &act[start..];
    let end = body.find("\n    }\n").unwrap();
    assert!(
        body[..end].contains("Out(Effect::Act(cmd))") && body[..end].contains("self.typed_sent("),
        "emit must set the typed-here state and build the effect"
    );
    assert!(
        !act.contains("impl From<Effect>") && !act.contains("Deserialize"),
        "Out must not be built from an Effect"
    );
    let (_, session) = all.iter().find(|(p, _)| p.ends_with("session.rs")).unwrap();
    let start = session
        .find("pub fn step(&mut self, ev: TimedEvent) -> Effect {")
        .unwrap();
    let body = &session[start..];
    let end = body.find("\n    }\n").unwrap();
    let statements: Vec<&str> = body[..end]
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(
        statements,
        ["self.step_out(ev).into_effect()"],
        "Session::step must only convert what the handlers returned"
    );
}
