//! Workflows: a trigger plus an ordered list of steps, run unattended by
//! [`crate::run::WorkflowRun`].
//!
//! # A workflow never widens policy
//!
//! Every step that touches the phone ([`Step::RunSkill`], [`Step::RunTask`]) becomes an
//! ordinary agent session under the owner's `Policy`, exactly as if the owner had typed the
//! task: deny list, trust levels, first-use approval, every confirmation and every budget
//! apply, and confirmations still go to the owner (an unattended run waits for them or
//! fails). A skill step can only narrow further (`confine_to_apps`). Nothing in a workflow
//! can approve an action, mark an app trusted or skip a prompt.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::error::{field, SkillError};
use crate::skill::{is_valid_id, is_valid_param_name, MAX_ARG_CHARS, MAX_NAME_CHARS};
use crate::text::{has_control, longer_than};

/// Most steps in one workflow.
pub const MAX_WORKFLOW_STEPS: usize = 24;
/// Longest text of a step (task, question, notification), in characters.
pub const MAX_STEP_TEXT_CHARS: usize = 500;
/// Longest phrase trigger or condition text, in characters.
pub const MAX_PHRASE_CHARS: usize = 120;
/// Longest external trigger key, in characters.
pub const MAX_EXTERNAL_KEY_CHARS: usize = 64;
/// Longest single wait a workflow may contain, in milliseconds (one hour).
pub const MAX_WAIT_STEP_MS: u64 = 3_600_000;
/// Most arguments of a `RunSkill` step.
pub const MAX_STEP_ARGS: usize = 8;

/// A day of the week, Monday first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Weekday {
    /// Monday.
    Mon,
    /// Tuesday.
    Tue,
    /// Wednesday.
    Wed,
    /// Thursday.
    Thu,
    /// Friday.
    Fri,
    /// Saturday.
    Sat,
    /// Sunday.
    Sun,
}

impl Weekday {
    /// Every day, Monday first.
    pub const ALL: [Weekday; 7] = [
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
        Weekday::Sat,
        Weekday::Sun,
    ];

    /// 0 for Monday to 6 for Sunday.
    pub fn index(self) -> u32 {
        match self {
            Weekday::Mon => 0,
            Weekday::Tue => 1,
            Weekday::Wed => 2,
            Weekday::Thu => 3,
            Weekday::Fri => 4,
            Weekday::Sat => 5,
            Weekday::Sun => 6,
        }
    }

    /// The day `days` after this one (wrapping).
    pub fn plus(self, days: u32) -> Weekday {
        let i = (self.index() + days % 7) % 7;
        Weekday::ALL
            .get(usize::try_from(i).unwrap_or(0))
            .copied()
            .unwrap_or(Weekday::Mon)
    }
}

/// A weekly time: on these days at `hour:minute`, in the phone's local time (the shell
/// supplies local weekday and time; the core has no timezone database).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Schedule {
    /// Days it runs on; must not be empty.
    pub days: BTreeSet<Weekday>,
    /// Hour, 0 to 23.
    pub hour: u8,
    /// Minute, 0 to 59.
    pub minute: u8,
}

/// What starts a workflow.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    /// Only when the owner starts it.
    #[default]
    Manual,
    /// When the owner says this phrase (normalised like `intent_key`).
    Phrase {
        /// The phrase.
        text: String,
    },
    /// On a weekly schedule.
    Schedule(Schedule),
    /// When the shell reports this key, e.g. an iOS Shortcuts automation (arrive home,
    /// charger connected, focus mode) that the shell maps to a key.
    External {
        /// `[a-z0-9._-]`, 1 to 64 characters.
        key: String,
    },
}

/// A test on what happened so far, for [`Step::StopUnless`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
    /// The most recent session step succeeded.
    LastSucceeded,
    /// The most recent session's summary contains `text` (case-insensitive).
    LastSummaryContains {
        /// Text to look for.
        text: String,
    },
    /// The most recent `AskOwner` was answered yes.
    OwnerSaidYes,
}

/// One step of a workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Step {
    /// Run a skill as a session.
    RunSkill {
        /// The skill's id.
        skill_id: String,
        /// Parameter values.
        #[serde(default)]
        args: BTreeMap<String, String>,
    },
    /// Run a free-text task as a session.
    RunTask {
        /// The task, as the owner would say it.
        text: String,
    },
    /// Ask the owner a yes/no question.
    AskOwner {
        /// The question.
        question: String,
    },
    /// Post a notification.
    Notify {
        /// The text.
        text: String,
    },
    /// Wait.
    Wait {
        /// How long, in milliseconds.
        ms: u64,
    },
    /// End the run (status `Stopped`) unless the condition holds.
    StopUnless {
        /// The condition.
        condition: Condition,
    },
}

/// What to do when a step fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnFailure {
    /// End the run.
    #[default]
    Stop,
    /// Go on with the next step.
    Continue,
}

/// How a workflow run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    /// Every step ran and none failed.
    #[default]
    Succeeded,
    /// Every step ran but at least one failed (with `OnFailure::Continue`).
    CompletedWithFailures,
    /// A step failed and the run stopped, or the run itself could not go on (budget,
    /// invalid workflow).
    Failed,
    /// A `StopUnless` condition did not hold.
    Stopped,
    /// The owner cancelled.
    Cancelled,
}

/// The result of a run.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkflowOutcome {
    /// How it ended.
    pub status: WorkflowStatus,
    /// Steps that ran (started), conditions included.
    pub steps_run: u32,
    /// One line for the owner.
    pub summary: String,
    /// 0-based index of the first step that failed, if any.
    pub failed_step: Option<u32>,
}

/// An automation. See the module docs: it never widens policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Workflow {
    /// Stable id, `[a-z0-9-]{1,48}`, unique in the store.
    pub id: String,
    /// What the owner calls it.
    pub name: String,
    /// Disabled workflows never trigger and refuse to run.
    pub enabled: bool,
    /// What starts it.
    pub trigger: Trigger,
    /// The steps, in order.
    pub steps: Vec<Step>,
    /// What a failed step does.
    pub on_failure: OnFailure,
    /// When it was created, in the shell's milliseconds.
    pub created_ms: u64,
    /// When its definition last changed.
    pub updated_ms: u64,
    /// How many times it ran.
    pub runs: u32,
    /// When it last ran.
    pub last_run_ms: Option<u64>,
    /// How the last run ended.
    pub last_outcome: Option<WorkflowOutcome>,
}

impl Default for Workflow {
    /// An empty, enabled, manual workflow (not valid until it has an id, a name and steps).
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            enabled: true,
            trigger: Trigger::Manual,
            steps: Vec::new(),
            on_failure: OnFailure::Stop,
            created_ms: 0,
            updated_ms: 0,
            runs: 0,
            last_run_ms: None,
            last_outcome: None,
        }
    }
}

fn check_text(name: &str, value: &str, max: usize) -> Result<(), SkillError> {
    if value.trim().is_empty() {
        return Err(field(name, "must not be empty"));
    }
    if longer_than(value, max) {
        return Err(field(name, format!("at most {max} characters")));
    }
    if has_control(value) {
        return Err(field(name, "must be one line without control characters"));
    }
    Ok(())
}

/// Whether `key` is a valid external trigger key: `[a-z0-9._-]{1,64}`.
pub fn is_valid_external_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= MAX_EXTERNAL_KEY_CHARS
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
}

impl Workflow {
    /// Checks every limit and rule: id, name, trigger (phrase text, schedule days and time,
    /// external key), 1 to [`MAX_WORKFLOW_STEPS`] steps, step texts, waits of 1 ms to
    /// [`MAX_WAIT_STEP_MS`], skill ids and argument names. Whether referenced skills exist
    /// is not checked here (see `WorkflowStore::warnings`).
    pub fn validate(&self) -> Result<(), SkillError> {
        if !is_valid_id(&self.id) {
            return Err(SkillError::InvalidId(self.id.clone()));
        }
        check_text("name", &self.name, MAX_NAME_CHARS)?;
        match &self.trigger {
            Trigger::Manual => {}
            Trigger::Phrase { text } => {
                check_text("trigger.text", text, MAX_PHRASE_CHARS)?;
                if jarvis_memory::intent_key(text).is_empty() {
                    return Err(field("trigger.text", "needs a word besides filler words"));
                }
            }
            Trigger::Schedule(s) => {
                if s.days.is_empty() {
                    return Err(field("trigger.days", "needs at least one day"));
                }
                if s.hour > 23 || s.minute > 59 {
                    return Err(field("trigger", "hour 0 to 23 and minute 0 to 59"));
                }
            }
            Trigger::External { key } => {
                if !is_valid_external_key(key) {
                    return Err(field(
                        "trigger.key",
                        "1 to 64 characters a-z, 0-9, '.', '_' and '-'",
                    ));
                }
            }
        }
        if self.steps.is_empty() {
            return Err(field("steps", "needs at least one step"));
        }
        if self.steps.len() > MAX_WORKFLOW_STEPS {
            return Err(SkillError::TooMany {
                field: "steps".to_owned(),
                max: MAX_WORKFLOW_STEPS,
            });
        }
        for (i, step) in self.steps.iter().enumerate() {
            let at = |f: &str| format!("steps[{i}].{f}");
            match step {
                Step::RunSkill { skill_id, args } => {
                    if !is_valid_id(skill_id) {
                        return Err(SkillError::InvalidId(skill_id.clone()));
                    }
                    if args.len() > MAX_STEP_ARGS {
                        return Err(SkillError::TooMany {
                            field: at("args"),
                            max: MAX_STEP_ARGS,
                        });
                    }
                    for (k, v) in args {
                        if !is_valid_param_name(k) {
                            return Err(SkillError::BadParam(k.clone()));
                        }
                        check_text(&at(&format!("args.{k}")), v, MAX_ARG_CHARS)?;
                    }
                }
                Step::RunTask { text } => check_text(&at("text"), text, MAX_STEP_TEXT_CHARS)?,
                Step::AskOwner { question } => {
                    check_text(&at("question"), question, MAX_STEP_TEXT_CHARS)?;
                }
                Step::Notify { text } => check_text(&at("text"), text, MAX_STEP_TEXT_CHARS)?,
                Step::Wait { ms } => {
                    if *ms == 0 || *ms > MAX_WAIT_STEP_MS {
                        return Err(field(
                            at("ms"),
                            format!("1 to {MAX_WAIT_STEP_MS} milliseconds"),
                        ));
                    }
                }
                Step::StopUnless { condition } => {
                    if let Condition::LastSummaryContains { text } = condition {
                        check_text(&at("condition.text"), text, MAX_PHRASE_CHARS)?;
                    }
                }
            }
        }
        Ok(())
    }
}
