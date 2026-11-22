//! Parser for the indented text tree the LoopCtrl-style runners return in `page_source`.

use jarvis_proto::{Rect, Role, UiNode};
use thiserror::Error;

/// How many physical lines one node (a label with embedded newlines) may span.
const MAX_CONTINUATION_LINES: usize = 8;

/// The parsed tree plus how many lines had to be skipped.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedTree {
    /// Root of the tree (several top-level lines are wrapped in a synthetic `Application`).
    pub root: UiNode,
    /// Number of non-blank lines that could not be parsed and were skipped.
    pub warnings: usize,
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
/// whitespace are ignored; a label with embedded newlines is rejoined; unparseable lines are
/// skipped and counted in [`ParsedTree::warnings`]; a jump of more than one level attaches to
/// the nearest open parent; several top-level lines are wrapped in a synthetic `Application`.
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
        if parsed.is_none() && looks_like_node_start(body) {
            let mut joined = body.to_owned();
            for (n, extra) in lines
                .iter()
                .skip(i)
                .take(MAX_CONTINUATION_LINES)
                .enumerate()
            {
                if looks_like_node_start(extra.trim()) {
                    // The next real node starts here: this line was truncated, not wrapped.
                    break;
                }
                joined.push('\n');
                joined.push_str(extra.trim_end());
                if extra.trim_end().ends_with("}}") {
                    if let Some(f) = parse_node(&joined) {
                        parsed = Some(f);
                        i += n + 1;
                    }
                    break;
                }
            }
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
    for (indent, fields) in entries {
        let depth = ((indent - base) / unit).min(stack.len());
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
    Ok(ParsedTree { root, warnings })
}
