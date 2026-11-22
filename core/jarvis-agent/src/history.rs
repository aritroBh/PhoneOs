//! The step history shown to the model: one short line per step with its result.
//!
//! Lines are built from prompt-safe data only: element labels come from the sanitised
//! screen, typed text is the model's own text (placeholders, not profile values) redacted and
//! sanitised, tool output and user answers are sanitised.

use jarvis_proto::{Action, CompactScreen};
use jarvis_safety::{redact, sanitize_screen_text_with_limit};

/// Characters of a label or text quoted in a history line.
const QUOTE_CHARS: usize = 32;
/// Steps shown in a prompt at most.
pub(crate) const MAX_SHOWN: usize = 8;

/// One finished step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    /// What was done, e.g. `tap [3] "Wi-Fi"`.
    pub action: String,
    /// How it went, e.g. `ok` or `failed: no visible change`.
    pub result: String,
    /// The step came from a learned flow, not the model.
    pub replayed: bool,
}

/// Steps so far, oldest first.
#[derive(Debug, Clone, Default)]
pub(crate) struct History {
    entries: Vec<Entry>,
}

impl History {
    /// Appends a step.
    pub(crate) fn push(&mut self, action: String, result: impl Into<String>, replayed: bool) {
        self.entries.push(Entry {
            action,
            result: result.into(),
            replayed,
        });
    }

    /// The most recent `n` steps with their 1-based step numbers.
    pub(crate) fn recent(&self, n: usize) -> Vec<(usize, &Entry)> {
        let skip = self.entries.len().saturating_sub(n);
        self.entries
            .iter()
            .enumerate()
            .skip(skip)
            .map(|(i, e)| (i + 1, e))
            .collect()
    }
}

/// Prompt-safe short text: redacted, sanitised, quotes turned into `'`, capped.
pub(crate) fn clean(text: &str, max_chars: usize) -> String {
    sanitize_screen_text_with_limit(&redact(text), max_chars).replace('"', "'")
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", clean(text, QUOTE_CHARS))
}

/// `[3] "Wi-Fi"` using the (sanitised) screen the action was chosen on.
fn element_ref(index: u32, screen: &CompactScreen) -> String {
    match screen.get(index) {
        Some(e) if !e.label.trim().is_empty() => format!("[{index}] {}", quoted(&e.label)),
        Some(e) => format!("[{index}] {}", e.role.as_str()),
        None => format!("[{index}]"),
    }
}

/// A one-line description of an action for the history, e.g. `type [4] "{{profile.email}}"`.
pub(crate) fn describe(action: &Action, screen: &CompactScreen) -> String {
    let dir = |d: &jarvis_proto::Direction| format!("{d:?}").to_lowercase();
    match action {
        Action::Tap { element } => format!("tap {}", element_ref(*element, screen)),
        Action::LongPress { element, .. } => {
            format!("long_press {}", element_ref(*element, screen))
        }
        Action::Type {
            element,
            text,
            submit,
        } => {
            let target = element.map(|e| format!(" {}", element_ref(e, screen)));
            format!(
                "type{} {}{}",
                target.unwrap_or_default(),
                quoted(text),
                if *submit { " +return" } else { "" }
            )
        }
        Action::Clear { element } => format!("clear {}", element_ref(*element, screen)),
        Action::Scroll { direction, element } => match element {
            Some(e) => format!("scroll {} in {}", dir(direction), element_ref(*e, screen)),
            None => format!("scroll {}", dir(direction)),
        },
        Action::Swipe { direction, element } => match element {
            Some(e) => format!("swipe {} on {}", dir(direction), element_ref(*e, screen)),
            None => format!("swipe {}", dir(direction)),
        },
        Action::TapPoint { x, y } => format!("tap_point ({x:.2}, {y:.2})"),
        Action::PressHome => "press_home".to_owned(),
        Action::OpenApp { app } => format!("open_app {}", quoted(app)),
        Action::OpenUrl { url } => format!("open_url {}", quoted(url)),
        Action::Wait { ms } => format!("wait {ms}ms"),
        Action::CallTool { name, args } => {
            format!(
                "call_tool {} {}",
                clean(name, QUOTE_CHARS),
                clean(&args.to_string(), 60)
            )
        }
        Action::AskUser { question } => format!("ask_user {}", quoted(question)),
        Action::Look => "look".to_owned(),
        Action::Done { summary } => format!("done {}", quoted(summary)),
        Action::Fail { reason } => format!("fail {}", quoted(reason)),
    }
}
