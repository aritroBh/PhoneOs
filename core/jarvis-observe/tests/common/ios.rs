#![allow(dead_code)]
//! Parser for the "typed tree" text dump the iOS runner writes for QA artifacts
//! (`UiTreeBuilder.dump`), so real XCUITest trees can live in the repo as readable fixtures.
//!
//! ```text
//! observation 1: com.apple.springboard
//! screen 440x956 pt, keyboard hidden
//!
//! Application "Jarvis" {{0, 0}, {440, 956}}
//!   Window {{0, 0}, {440, 956}}
//!     Button "Sign In" id=login.submit {{16, 400}, {361, 44}} [disabled, focused]
//! ```
//!
//! One node per line, two spaces per level. A line is the role (`Button`, or `Other(Name)` for
//! roles the core does not know), then optionally `"label"`, `value="…"`, `placeholder="…"`,
//! `id=…` (up to the frame), the frame `{{x, y}, {w, h}}` and `[flags]` (`disabled`,
//! `focused`, `selected`, `secure`, `hidden`). Strings use `\"`, `\\`, `\n`, `\r`, `\t`.
//! Header lines other than `observation N: <bundle id>` and `screen WxH pt, keyboard
//! hidden|visible` are ignored, as are blank lines and lines starting with `#`.

use jarvis_observe::compact;
use jarvis_proto::{CompactScreen, Orientation, Rect, Role, Screen, Size, UiNode};

use super::compact_text;

/// Reads `tests/fixtures/ios26/<name>.txt`.
pub fn fixture(name: &str) -> Screen {
    let path = format!(
        "{}/tests/fixtures/ios26/{name}.txt",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    parse_screen(&text).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

/// Compacts a fixture.
pub fn compact_fixture(name: &str) -> CompactScreen {
    compact(&fixture(name))
}

/// The compact screen as `[n] Role "label" value=… @ (x, y, w, h)` lines.
pub fn dump_fixture(name: &str) -> String {
    compact_text(&compact_fixture(name))
}

/// Parses a whole dump (header and tree) into a [`Screen`].
pub fn parse_screen(text: &str) -> Result<Screen, String> {
    let mut bundle = String::new();
    let mut size = Size::new(440.0, 956.0);
    let mut keyboard = false;
    let mut tree_lines: Vec<&str> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if tree_lines.is_empty() && !line.starts_with(' ') {
            if let Some(rest) = line.strip_prefix("observation ") {
                bundle = rest
                    .split_once(": ")
                    .map_or("", |(_, b)| b)
                    .trim()
                    .to_owned();
                continue;
            }
            if let Some(rest) = line.strip_prefix("screen ") {
                let dims = rest.split(' ').next().unwrap_or("");
                if let Some((w, h)) = dims.split_once('x') {
                    size = Size::new(num(w)?, num(h)?);
                }
                keyboard = rest.contains("keyboard visible");
                continue;
            }
            if !line.starts_with("Application") {
                continue;
            }
        }
        tree_lines.push(line);
    }
    let root = parse_tree(&tree_lines)?;
    let app_name = root
        .label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    Ok(Screen {
        app_bundle_id: bundle,
        app_name,
        size,
        root,
        keyboard_visible: keyboard,
        orientation: Orientation::Portrait,
    })
}

/// Parses indented node lines into a tree (exactly one root).
pub fn parse_tree(lines: &[&str]) -> Result<UiNode, String> {
    let mut stack: Vec<(usize, UiNode)> = Vec::new();
    let mut root: Option<UiNode> = None;
    for line in lines {
        let indent = line.len() - line.trim_start_matches(' ').len();
        if indent % 2 != 0 {
            return Err(format!("odd indentation: {line:?}"));
        }
        let depth = indent / 2;
        let node = parse_node(line.trim_start_matches(' '))?;
        while stack.last().is_some_and(|(d, _)| *d >= depth) {
            if let Some((_, done)) = stack.pop() {
                attach(&mut stack, &mut root, done)?;
            }
        }
        if depth != stack.len() {
            return Err(format!("indentation jumps a level: {line:?}"));
        }
        stack.push((depth, node));
    }
    while let Some((_, done)) = stack.pop() {
        attach(&mut stack, &mut root, done)?;
    }
    root.ok_or_else(|| "no tree".to_owned())
}

fn attach(
    stack: &mut [(usize, UiNode)],
    root: &mut Option<UiNode>,
    done: UiNode,
) -> Result<(), String> {
    match stack.last_mut() {
        Some((_, parent)) => parent.children.push(done),
        None if root.is_none() => *root = Some(done),
        None => return Err("more than one root".to_owned()),
    }
    Ok(())
}

fn num(s: &str) -> Result<f64, String> {
    s.trim().parse().map_err(|_| format!("bad number {s:?}"))
}

/// Parses one node line (without indentation) into a childless node.
pub fn parse_node(line: &str) -> Result<UiNode, String> {
    let (role_text, mut rest) = match line.find(' ') {
        Some(i) => (&line[..i], &line[i..]),
        None => (line, ""),
    };
    let role = match role_text
        .strip_prefix("Other(")
        .and_then(|r| r.strip_suffix(')'))
    {
        Some(name) => Role::Other(name.to_owned()),
        None => Role::from_name(role_text),
    };
    let mut node = UiNode::new(role, Rect::default());
    loop {
        if let Some(r) = rest.strip_prefix(" \"") {
            let (s, after) = quoted(r)?;
            node.label = Some(s);
            rest = after;
        } else if let Some(r) = rest.strip_prefix(" value=\"") {
            let (s, after) = quoted(r)?;
            node.value = Some(s);
            rest = after;
        } else if let Some(r) = rest.strip_prefix(" placeholder=\"") {
            let (s, after) = quoted(r)?;
            node.placeholder = Some(s);
            rest = after;
        } else if let Some(r) = rest.strip_prefix(" id=") {
            let end = r.find(" {{").ok_or("identifier without frame")?;
            node.identifier = Some(unescape(&r[..end]));
            rest = &r[end..];
        } else if let Some(r) = rest.strip_prefix(" {{") {
            let end = r.find("}}").ok_or("unterminated frame")?;
            let nums: Vec<f64> = r[..end]
                .split([',', '{', '}', ' '])
                .filter(|s| !s.is_empty())
                .map(num)
                .collect::<Result<_, _>>()?;
            let [x, y, w, h] = nums[..] else {
                return Err(format!("frame needs four numbers: {line:?}"));
            };
            node.frame = Rect::new(x, y, w, h);
            rest = &r[end + 2..];
            if let Some(flags) = rest.trim().strip_prefix('[') {
                let flags = flags.strip_suffix(']').ok_or("unterminated flags")?;
                for flag in flags.split(',').map(str::trim) {
                    match flag {
                        "disabled" => node.enabled = false,
                        "focused" => node.focused = true,
                        "selected" => node.selected = true,
                        "secure" => node.secure = true,
                        "hidden" => {
                            node.visible = false;
                            node.hittable = false;
                        }
                        other => return Err(format!("unknown flag {other:?}")),
                    }
                }
            } else if !rest.trim().is_empty() {
                return Err(format!("trailing text {rest:?} in {line:?}"));
            }
            if node.visible {
                node.hittable = true;
            }
            return Ok(node);
        } else {
            return Err(format!("cannot parse {rest:?} in {line:?}"));
        }
    }
}

/// Reads a string up to its closing unescaped quote; returns it and the rest of the line.
fn quoted(s: &str) -> Result<(String, &str), String> {
    let mut out = String::new();
    let mut chars = s.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((out, &s[i + 1..])),
            '\\' => match chars.next() {
                Some((_, 'n')) => out.push('\n'),
                Some((_, 'r')) => out.push('\r'),
                Some((_, 't')) => out.push('\t'),
                Some((_, other)) => out.push(other),
                None => break,
            },
            other => out.push(other),
        }
    }
    Err(format!("unterminated string in {s:?}"))
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}
