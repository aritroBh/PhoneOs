//! Bounded task log: text only, redacted by the caller before it is stored here.

use jarvis_proto::OutcomeStatus;
use serde::{Deserialize, Serialize};

/// Default number of entries kept.
pub const MAX_ENTRIES: usize = 100;
/// Steps kept per entry (the most recent ones).
pub const MAX_STEPS_PER_ENTRY: usize = 100;
/// Character cap for task text and summary.
pub const MAX_TEXT_CHARS: usize = 500;
/// Character cap for action and app names.
pub const MAX_NAME_CHARS: usize = 100;

fn clip(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// One executed step as shown in the task log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogStep {
    /// Action name (`tap`, `type`, ...), never its arguments.
    pub action: String,
    /// App the step ran in.
    #[serde(default)]
    pub app: String,
    /// Whether the step succeeded.
    #[serde(default)]
    pub ok: bool,
}

/// One finished task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    /// The task text (already redacted).
    pub task: String,
    /// Steps taken.
    #[serde(default)]
    pub steps: Vec<LogStep>,
    /// How the task ended.
    pub outcome: OutcomeStatus,
    /// Outcome summary (already redacted).
    #[serde(default)]
    pub summary: String,
    /// Start time in the shell's milliseconds.
    #[serde(default)]
    pub started_ms: u64,
    /// End time in the shell's milliseconds.
    #[serde(default)]
    pub finished_ms: u64,
    /// Times the phone locked during the task (each one a wait for the owner to unlock).
    #[serde(default)]
    pub lock_waits: u32,
    /// Total time spent waiting for the owner to unlock, in milliseconds.
    #[serde(default)]
    pub locked_ms: u64,
}

impl LogEntry {
    fn clamp(&mut self) {
        self.task = clip(&self.task, MAX_TEXT_CHARS);
        self.summary = clip(&self.summary, MAX_TEXT_CHARS);
        if self.steps.len() > MAX_STEPS_PER_ENTRY {
            let excess = self.steps.len() - MAX_STEPS_PER_ENTRY;
            self.steps.drain(..excess);
        }
        for s in &mut self.steps {
            s.action = clip(&s.action, MAX_NAME_CHARS);
            s.app = clip(&s.app, MAX_NAME_CHARS);
        }
    }
}

/// A bounded ring of [`LogEntry`], oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskLog {
    #[serde(default)]
    entries: Vec<LogEntry>,
    #[serde(skip, default = "default_cap")]
    max_entries: usize,
}

fn default_cap() -> usize {
    MAX_ENTRIES
}

impl Default for TaskLog {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            max_entries: MAX_ENTRIES,
        }
    }
}

impl TaskLog {
    /// An empty log with the default capacity.
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty log keeping at most `max_entries` (at least 1) entries.
    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries: max_entries.max(1),
        }
    }

    /// Appends an entry, clipping long strings and dropping the oldest entries over the cap.
    pub fn push(&mut self, mut entry: LogEntry) {
        entry.clamp();
        self.entries.push(entry);
        if self.entries.len() > self.max_entries {
            let excess = self.entries.len() - self.max_entries;
            self.entries.drain(..excess);
        }
    }

    /// Entries, oldest first.
    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the log has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Re-applies the caps to everything (used after loading from disk).
    pub(crate) fn from_entries(entries: Vec<LogEntry>) -> Self {
        let mut log = Self::default();
        for e in entries {
            log.push(e);
        }
        log
    }
}
