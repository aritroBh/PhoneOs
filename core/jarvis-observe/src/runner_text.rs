//! Parser for the indented text tree the LoopCtrl-style runners return in `page_source`.
//!
//! # Trust: the format is ambiguous and forgeable
//!
//! A node line is `Type, label: '…', value: '…', id: '…', {{x, y}, {w, h}}` with quotes and
//! newlines **not escaped**. On-screen text (a text field's value, a web page's text) can
//! therefore contain characters that read as format: a newline followed by a well-formed
//! line, a fake role, a fake frame. No parser can make an ambiguous format unambiguous, so
//! the session treats every screen read this way as unverified (it asks before any
//! state-changing action, with a warning; see `jarvis-agent`). What the parser does stop:
//!
//! * **Newline injection inside an open quote.** A line that starts a node but leaves a
//!   quoted field open (`TextView, label: 'Note', value: 'thanks` with no closing `', {{`)
//!   makes every following line continuation text of that value until a line closes the
//!   node (`…', {{x, y}, {w, h}}`), however many lines that takes and whatever those lines
//!   look like: a continuation line is never promoted to a node. A node that never closes
//!   swallows the rest of the text (counted as skipped lines): fail closed.
//!
//! What it cannot stop (the residual):
//!
//! * **Closing-quote injection.** Text that itself contains `', {{x, y}, {w, h}}` at the end
//!   of a line closes the node early, and the lines after it parse as nodes the page chose
//!   (any role, any frame, any depth). That is indistinguishable from a real tree.
//! * Text that contains `', value: '` or `', id: '` can move the boundary between fields.
//!
//! The typed tree from Jarvis's own runner has none of these problems and is the supported
//! path for acting; this text path is for experiments (see `docs/RUNNER.md`, "Trust").

use jarvis_proto::{Rect, Role, UiNode};

use crate::compact::MAX_DEPTH;
use thiserror::Error;

/// How many times one open node tries to close on a line that looks like its end before
/// it is given up (bounds the work on adversarial text: each try parses the joined text).
const MAX_CLOSE_ATTEMPTS: usize = 16;

/// The parsed tree plus how many lines had to be skipped.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedTree {
    /// Root of the tree (several top-level lines are wrapped in a synthetic `Application`).
    pub root: UiNode,
    /// Number of non-blank lines that could not be parsed and were skipped.
    pub warnings: usize,
    /// Review pass 31 (L1): nodes deeper than `jarvis_proto::MAX_TREE_DEPTH` levels were
    /// dropped (cut there); the screen built from this tree carries `Screen::tree_cut`.
    pub cut: bool,
}

/// Why a runner text tree could not be parsed at all.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    /// No line looked like a node.
    #[error("no parseable nodes in runner text ({skipped} non-blank lines skipped)")]
    NoNodes {
        /// Number of non-blank lines skipped.
        skipped: usize,
    },
}

struct Fields {
    type_name: String,
    label: Option<String>,
    value: Option<String>,
    id: Option<String>,
    frame: Rect,
}

fn parse_frame(s: &str) -> Option<Rect> {
    let inner = s.trim().strip_prefix("{{")?.strip_suffix("}}")?;
    let (origin, size) = inner.split_once('}')?;
    let size = size.trim_start_matches([',', ' ', '{']);
    let pair = |s: &str| -> Option<(f64, f64)> {
        let (a, b) = s.split_once(',')?;
        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
    };
    let (x, y) = pair(origin)?;
    let (w, h) = pair(size)?;
    Some(Rect::new(x, y, w, h))
}

/// Parses `label: '...'`, `value: '...'`, `id: '...'` (in that order, each optional) from
/// the text after the type name. Quotes and commas inside values are tolerated: a value
/// ends at the first following field marker, or at the final quote.
fn parse_fields(mut rest: &str) -> Option<[Option<String>; 3]> {
    const KEYS: [&str; 3] = ["label", "value", "id"];
    let marker = |k: &str| format!(", {k}: '");
    let mut out: [Option<String>; 3] = [None, None, None];
    let mut next_key = 0;
    while !rest.trim().is_empty() {
        let (slot, key) = KEYS
            .iter()
            .enumerate()
            .skip(next_key)
            .find(|(_, k)| rest.starts_with(&marker(k)))?;
        let body_and_more = rest.get(marker(key).len()..)?;
        let end = KEYS
            .iter()
            .skip(slot + 1)
            .filter_map(|k| body_and_more.find(&format!("'{}", marker(k))))
            .min();
        let (body, more) = match end {
            Some(at) => (body_and_more.get(..at)?, body_and_more.get(at + 1..)?),
            None => (
                body_and_more
                    .trim_end()
                    .strip_suffix('\'')
                    .unwrap_or(body_and_more),
                "",
            ),
        };
        if let Some(o) = out.get_mut(slot) {
            *o = Some(body.to_owned());
        }
        next_key = slot + 1;
        rest = more;
    }
    Some(out)
}

fn parse_node(body: &str) -> Option<Fields> {
    let body = body.trim_end();
    let frame_at = body.rfind("{{")?;
    let frame = parse_frame(body.get(frame_at..)?)?;
    let head = body.get(..frame_at)?.trim_end().trim_end_matches(',');
    let type_end = head.find(',').unwrap_or(head.len());
    let type_name = head.get(..type_end)?.trim();
    let valid_type = !type_name.is_empty()
        && type_name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '<' | '>'));
    if !valid_type {
        return None;
    }
    let [label, value, id] = parse_fields(head.get(type_end..)?)?;
    Some(Fields {
        type_name: type_name.to_owned(),
        label,
        value,
        id,
        frame,
    })
}

fn looks_like_node_start(line: &str) -> bool {
    line.split_once(',').is_some_and(|(t, rest)| {
        !t.is_empty()
            && t.chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '<' | '>'))
            && rest.trim_start().starts_with(['l', 'v', 'i', '{'])
    })
}

/// Whether `line` (a node start that did not parse) leaves a quoted field open: after its
/// last field marker (`, label: '`, `, value: '`, `, id: '`) there is no closing `', {{`.
/// A line whose quote closed but whose frame is broken (`…', {{10, 10}, {80`) is not open:
/// it is a truncated line, skipped on its own.
fn quote_open(line: &str) -> bool {
    let last = ["label", "value", "id"]
        .iter()
        .filter_map(|k| {
            let m = format!(", {k}: '");
            line.rfind(&m).map(|at| at + m.len())
        })
        .max();
    last.and_then(|at| line.get(at..))
        .is_some_and(|rest| !rest.contains("', {{"))
}

/// Whether `line` can end an open node: a valid frame at its end right after a closing
/// quote (`third', {{10, 10}, {300, 120}}`).
fn closes_node(line: &str) -> bool {
    let line = line.trim_end();
    let Some(at) = line.rfind("{{") else {
        return false;
    };
    line.get(at..).and_then(parse_frame).is_some()
        && line
            .get(..at)
            .is_some_and(|head| head.trim_end().trim_end_matches(',').ends_with('\''))
}

fn to_node(f: Fields) -> UiNode {
    let role = Role::from_name(&f.type_name);
    let mut node = UiNode::new(role, f.frame).hittable(true);
    node.label = f.label.filter(|s| !s.is_empty());
    node.value = f.value.filter(|s| !s.is_empty());
    node.identifier = f.id.filter(|s| !s.is_empty());
    node
}

/// Parses the indented text tree from a runner's `page_source` result.
///
/// Line format: two spaces of indent per depth, then
/// `TypeName, label: '...', value: '...', id: '...', {{x, y}, {w, h}}` with label, value and
/// id present only when non-empty, and unmapped types shown as `Type<n>`. The indent width is
/// inferred. Because that text carries no flags, nodes get `enabled`, `visible` and
/// `hittable` true, `focused` and `selected` false, and `secure` only for
/// `SecureTextField`.
///
/// Robustness: labels may contain commas, quotes and braces; blank lines and trailing
/// whitespace are ignored; a label or value with embedded newlines is rejoined, and every
/// line after a node start with an open quote is continuation text until the node closes
/// (never a node of its own, see the module docs); unparseable lines are skipped and
/// counted in [`ParsedTree::warnings`]; a jump of more than one level attaches to the
/// nearest open parent; several top-level lines are wrapped in a synthetic `Application`.
/// Review pass 31 (L1): a node deeper than `jarvis_proto::MAX_TREE_DEPTH` levels (and all it
/// holds) is dropped and the tree says so ([`ParsedTree::cut`]); it was flattened to
/// siblings at the cap, which no rule could tell from a real screen.
///
/// The format comes from reading the runner's source (see `docs/RUNNER.md` section 2.1) and
/// has not been verified against a real device.
pub fn parse_runner_text(source: &str) -> Result<ParsedTree, ParseError> {
    let lines: Vec<&str> = source.lines().collect();
    let mut entries: Vec<(usize, Fields)> = Vec::new();
    let mut warnings = 0;
    let mut i = 0;
    while let Some(&line) = lines.get(i) {
        i += 1;
        let body = line.trim();
        if body.is_empty() {
            continue;
        }
        let indent = line.chars().take_while(|c| c.is_whitespace()).count();
        let mut parsed = parse_node(body);
        if parsed.is_none() && looks_like_node_start(body) && quote_open(body) {
            // Every following line is text of the open field until one closes the node.
            let mut joined = body.to_owned();
            let mut attempts = 0usize;
            let mut consumed = 0usize;
            while let Some(extra) = lines.get(i + consumed) {
                consumed += 1;
                let extra = extra.trim_end();
                joined.push('\n');
                joined.push_str(extra);
                if attempts < MAX_CLOSE_ATTEMPTS && closes_node(extra) {
                    attempts += 1;
                    if let Some(f) = parse_node(&joined) {
                        parsed = Some(f);
                        break;
                    }
                }
            }
            if parsed.is_none() {
                // Never closed: the rest is text of a broken node, never nodes.
                warnings += lines
                    .iter()
                    .skip(i)
                    .take(consumed)
                    .filter(|l| !l.trim().is_empty())
                    .count();
            }
            i += consumed;
        }
        match parsed {
            Some(f) => entries.push((indent, f)),
            None => warnings += 1,
        }
    }
    if entries.is_empty() {
        return Err(ParseError::NoNodes { skipped: warnings });
    }

    let base = entries.iter().map(|(i, _)| *i).min().unwrap_or(0);
    let unit = entries
        .iter()
        .map(|(i, _)| i - base)
        .filter(|d| *d > 0)
        .min()
        .unwrap_or(1);

    let mut roots: Vec<UiNode> = Vec::new();
    let mut stack: Vec<UiNode> = Vec::new();
    let close = |stack: &mut Vec<UiNode>, roots: &mut Vec<UiNode>| {
        if let Some(done) = stack.pop() {
            match stack.last_mut() {
                Some(parent) => parent.children.push(done),
                None => roots.push(done),
            }
        }
    };
    let mut cut = false;
    for (indent, fields) in entries {
        let depth = ((indent - base) / unit).min(stack.len());
        // This node's level is `depth + 1`: past the bound it is cut, with what it holds
        // (every deeper line that follows attaches no higher than the stack allows).
        if depth >= MAX_DEPTH {
            cut = true;
            continue;
        }
        while stack.len() > depth {
            close(&mut stack, &mut roots);
        }
        stack.push(to_node(fields));
    }
    while !stack.is_empty() {
        close(&mut stack, &mut roots);
    }

    let root = if roots.len() == 1 {
        roots.pop().unwrap_or_default()
    } else {
        let frame = roots.iter().fold(None::<Rect>, |acc, n| {
            Some(match acc {
                None => n.frame,
                Some(a) => {
                    let x = a.x.min(n.frame.x);
                    let y = a.y.min(n.frame.y);
                    Rect::new(
                        x,
                        y,
                        a.max_x().max(n.frame.max_x()) - x,
                        a.max_y().max(n.frame.max_y()) - y,
                    )
                }
            })
        });
        UiNode::new(Role::Application, frame.unwrap_or_default()).with_children(roots)
    };
    let mut root = root;
    // The synthetic `Application` adds a level.
    cut |= jarvis_proto::cut_tree(&mut root);
    Ok(ParsedTree {
        root,
        warnings,
        cut,
    })
}
