//! The real iOS 26 trees under `jarvis-observe/tests/fixtures/ios26/`.
//!
//! The fixtures are the runner's typed-tree dump (`UiTreeBuilder.dump`), one node per line.
//! The parser here reads the same format as the one the observe crate's tests use
//! (`tests/common/ios.rs`, which is test code and cannot be called from here).

use std::path::{Path, PathBuf};

use jarvis_proto::{Orientation, Rect, Role, Screen, Size, UiNode};

/// Where the fixtures are in a checkout of the repository.
pub fn default_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../jarvis-observe/tests/fixtures/ios26")
}

/// A parsed fixture.
pub struct Fixture {
    /// File name without the extension.
    pub name: String,
    /// The tree.
    pub screen: Screen,
}

/// Every `*.txt` file in `dir`, parsed, in name order.
pub fn load_all(dir: &Path) -> Result<Vec<Fixture>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let screen = parse_screen(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        out.push(Fixture { name, screen });
    }
    Ok(out)
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

fn parse_tree(lines: &[&str]) -> Result<UiNode, String> {
    let mut stack: Vec<(usize, UiNode)> = Vec::new();
    let mut root: Option<UiNode> = None;
    for line in lines {
        let body = line.trim_start_matches(' ');
        let indent = line.len() - body.len();
        if indent % 2 != 0 {
            return Err(format!("odd indentation: {line:?}"));
        }
        let depth = indent / 2;
        let node = parse_node(body)?;
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

fn parse_node(line: &str) -> Result<UiNode, String> {
    let (role_text, mut rest) = match line.find(' ') {
        Some(i) => line.split_at(i),
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
            let (id, after) = r.split_at(end);
            node.identifier = Some(unescape(id));
            rest = after;
        } else if let Some(r) = rest.strip_prefix(" {{") {
            let end = r.find("}}").ok_or("unterminated frame")?;
            let (inside, after) = r.split_at(end);
            let nums: Vec<f64> = inside
                .split([',', '{', '}', ' '])
                .filter(|s| !s.is_empty())
                .map(num)
                .collect::<Result<_, _>>()?;
            let &[x, y, w, h] = nums.as_slice() else {
                return Err(format!("frame needs four numbers: {line:?}"));
            };
            node.frame = Rect::new(x, y, w, h);
            let after = after.strip_prefix("}}").unwrap_or(after);
            if let Some(flags) = after.trim().strip_prefix('[') {
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
            } else if !after.trim().is_empty() {
                return Err(format!("trailing text {after:?} in {line:?}"));
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
            '"' => return Ok((out, s.get(i + 1..).unwrap_or(""))),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_checked_in_fixture_parses() {
        let all = load_all(&default_dir()).unwrap();
        assert_eq!(all.len(), 10);
        for f in &all {
            assert_eq!(f.screen.root.role, Role::Application, "{}", f.name);
            assert!(f.screen.root.count() > 10, "{}", f.name);
        }
    }

    #[test]
    fn a_node_line_reads_every_part() {
        let n = parse_node(
            r#"TextField "Say \"hi\"" value="a\nb" placeholder="Name" id=form.name {{16, 40.5}, {361, 44}} [disabled, focused]"#,
        )
        .unwrap();
        assert_eq!(n.label.as_deref(), Some("Say \"hi\""));
        assert_eq!(n.value.as_deref(), Some("a\nb"));
        assert_eq!(n.identifier.as_deref(), Some("form.name"));
        assert!(!n.enabled && n.focused);
        assert_eq!(n.frame, Rect::new(16.0, 40.5, 361.0, 44.0));
    }
}
