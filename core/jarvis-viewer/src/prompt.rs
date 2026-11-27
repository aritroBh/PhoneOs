//! The element lines of a prompt's screen block, as the core rendered them
//! (`[4] TextField "Email" placeholder="you@example.com" (focused)`). The reducer reads
//! them for typing evidence on runner-text screens. Values are parsed past but never kept.

/// Opening marker of the screen data block (jarvis-safety `SCREEN_BLOCK_START`).
pub const BLOCK_START: &str = "<<<SCREEN_DATA>>>";
/// Closing marker (jarvis-safety `SCREEN_BLOCK_END`).
pub const BLOCK_END: &str = "<<<END_SCREEN_DATA>>>";

/// One `[N] Role "label" …` line, without its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptElement {
    /// The element index the model refers to.
    pub index: u32,
    /// The role as rendered (`TextField`).
    pub role: String,
    /// The label (empty when none).
    pub label: String,
    /// The placeholder, when rendered (only for elements without a value).
    pub placeholder: Option<String>,
    /// Flags (`focused`, `selected`, `disabled`), without a switch's `on`/`off`.
    pub flags: Vec<String>,
}

impl PromptElement {
    /// The line carries the `focused` flag.
    pub fn is_focused(&self) -> bool {
        self.flags.iter().any(|f| f == "focused")
    }
}

/// The element lines of the screen block of `prompt` (the user part of a model request),
/// in order; empty when there is no block.
pub fn prompt_elements(prompt: &str) -> Vec<PromptElement> {
    let Some(start) = prompt.find(BLOCK_START) else {
        return Vec::new();
    };
    let after = prompt.get(start + BLOCK_START.len()..).unwrap_or_default();
    let Some(end) = after.find(BLOCK_END) else {
        return Vec::new();
    };
    let block = after
        .get(..end)
        .unwrap_or_default()
        .trim_matches(crate::privacy::is_newline);
    block
        .split('\n')
        .filter(|l| !l.is_empty())
        .filter(|l| !l.starts_with("App: ") && *l != "Keyboard: visible")
        .filter_map(parse_element)
        .collect()
}

/// Parses `[N] Role "label" value="v" (flags)`; `None` for any other line.
pub fn parse_element(line: &str) -> Option<PromptElement> {
    let s = line.strip_prefix('[')?;
    let close = s.find(']')?;
    let index: u32 = s.get(..close)?.parse().ok()?;
    let s = s.get(close + 1..)?.strip_prefix(' ')?;
    let role_end = s.find(' ').unwrap_or(s.len());
    let role = s.get(..role_end)?;
    if role.is_empty() {
        return None;
    }
    let mut s = s.get(role_end..)?;
    let mut element = PromptElement {
        index,
        role: role.to_owned(),
        label: String::new(),
        placeholder: None,
        flags: Vec::new(),
    };
    if let Some(rest) = s.strip_prefix(" \"") {
        let Some((label, rest)) = read_quoted(rest) else {
            return Some(element);
        };
        element.label = label;
        s = rest;
    }
    for key in ["value", "placeholder"] {
        let prefix_len = key.len() + 3;
        let matches = s.starts_with(' ')
            && s.get(1..).is_some_and(|t| t.starts_with(key))
            && s.get(1 + key.len()..).is_some_and(|t| t.starts_with("=\""));
        if !matches {
            continue;
        }
        let Some((text, rest)) = read_quoted(s.get(prefix_len..)?) else {
            return Some(element);
        };
        if key == "placeholder" {
            element.placeholder = Some(text);
        }
        s = rest;
    }
    if let Some(inner) = s.strip_prefix(" (").and_then(|t| t.strip_suffix(')')) {
        for flag in inner.split(',').filter(|f| !f.is_empty()).map(str::trim) {
            if flag != "on" && flag != "off" {
                element.flags.push(flag.to_owned());
            }
        }
    }
    Some(element)
}

/// Reads up to the closing unescaped quote; returns the unescaped text and the rest.
fn read_quoted(s: &str) -> Option<(String, &str)> {
    let mut out = String::new();
    let mut chars = s.char_indices();
    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            let (_, next) = chars.next()?;
            match next {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                other => out.push(other),
            }
            continue;
        }
        if c == '"' {
            return Some((out, s.get(i + 1..)?));
        }
        out.push(c);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_block() {
        let prompt = "Steps so far:\n1. tap\n<<<SCREEN_DATA>>>\nApp: Mail (com.apple.mobilemail)\n\
                      Keyboard: visible\n[1] TextField \"Email\" value=\"a@b.c\" (focused)\n\
                      [2] SecureTextField \"Pass\\\"word\" placeholder=\"Required\"\n\
                      [3] Switch \"Wi-Fi\" (on, selected)\n… more below; scroll down\n\
                      <<<END_SCREEN_DATA>>>\n[9] Button \"outside\"";
        let els = prompt_elements(prompt);
        assert_eq!(els.len(), 3);
        assert_eq!(els[0].index, 1);
        assert_eq!(els[0].role, "TextField");
        assert_eq!(els[0].label, "Email");
        assert!(els[0].is_focused());
        assert_eq!(els[1].label, "Pass\"word");
        assert_eq!(els[1].placeholder.as_deref(), Some("Required"));
        assert_eq!(els[2].flags, vec!["selected".to_owned()]);
        assert!(prompt_elements("no block").is_empty());
        assert!(parse_element("[x] Button").is_none());
        assert!(parse_element("[1]Button").is_none());
        assert_eq!(
            parse_element("[1] Button \"unterminated").map(|e| e.label),
            Some(String::new())
        );
    }
}
