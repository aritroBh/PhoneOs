//! Typed errors for skills, workflows and their stores.

/// Why a skill, a workflow or a snapshot was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SkillError {
    /// The bytes are not valid JSON, or serialization failed.
    #[error("invalid JSON: {0}")]
    Json(String),
    /// The snapshot was written by a newer, incompatible version.
    #[error("schema version {found} is newer than supported version {supported}")]
    NewerSchema {
        /// Version in the data.
        found: u64,
        /// Highest version this build reads.
        supported: u32,
    },
    /// The JSON is valid but not shaped like a snapshot.
    #[error("invalid snapshot: {0}")]
    Invalid(String),
    /// An id is not 1 to 48 characters of `a-z`, `0-9` and `-`.
    #[error("invalid id {0:?}: use 1 to 48 characters a-z, 0-9 and -")]
    InvalidId(String),
    /// An item with this id already exists.
    #[error("id {0:?} is already in use")]
    DuplicateId(String),
    /// No item with this id.
    #[error("no item with id {0:?}")]
    NotFound(String),
    /// The store holds its maximum number of items.
    #[error("the store is full ({max} items)")]
    StoreFull {
        /// The cap.
        max: usize,
    },
    /// A text field is empty, too long or contains control characters.
    #[error("{field}: {reason}")]
    Field {
        /// Which field (`name`, `triggers[2]`, `steps[0].text`...).
        field: String,
        /// What is wrong.
        reason: String,
    },
    /// A list field has too many entries.
    #[error("{field}: at most {max} entries")]
    TooMany {
        /// Which field.
        field: String,
        /// The cap.
        max: usize,
    },
    /// A trigger phrase cannot be parsed.
    #[error("trigger {trigger:?}: {reason}")]
    BadTrigger {
        /// The trigger as given.
        trigger: String,
        /// What is wrong.
        reason: String,
    },
    /// A trigger uses a `{slot}` that is not a declared parameter.
    #[error("trigger {trigger:?} uses {{{slot}}}, which is not a declared parameter")]
    UndeclaredSlot {
        /// The trigger as given.
        trigger: String,
        /// The slot name.
        slot: String,
    },
    /// A parameter name is not `[a-z_][a-z0-9_]{0,23}`, or is declared twice.
    #[error("parameter {0:?}: names are 1 to 24 characters a-z, 0-9 and _, not starting with a digit, and unique")]
    BadParam(String),
    /// A learned flow cannot become a skill.
    #[error("flow cannot be promoted: {0}")]
    Flow(String),
    /// A skill already exists for this learned flow.
    #[error("skill {0:?} already replays this flow")]
    AlreadyPromoted(String),
}

impl From<serde_json::Error> for SkillError {
    fn from(e: serde_json::Error) -> Self {
        SkillError::Json(e.to_string())
    }
}

/// A [`SkillError::Field`] for `field`.
pub(crate) fn field(field: impl Into<String>, reason: impl Into<String>) -> SkillError {
    SkillError::Field {
        field: field.into(),
        reason: reason.into(),
    }
}
