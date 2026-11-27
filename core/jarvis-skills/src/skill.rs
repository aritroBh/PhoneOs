//! The skill data model, its validation and trigger parsing.
//!
//! A skill is a named, reusable capability: what the owner calls it, the phrases that
//! trigger it (`"turn off wi-fi"`, `"text {contact} {message}"`), the owner's own guidance
//! in plain language, the apps it works in and, for skills learned from a task, the intent
//! keys of the learned flows that replay it without the model.

use std::collections::{BTreeMap, BTreeSet};

use jarvis_proto::SkillContext;
use serde::{Deserialize, Serialize};

use crate::error::{field, SkillError};
use crate::text::{clean_arg, has_hidden, longer_than, words};

/// Longest skill name, in characters.
pub const MAX_NAME_CHARS: usize = 60;
/// Longest summary, in characters.
pub const MAX_SUMMARY_CHARS: usize = 200;
/// Most triggers per skill.
pub const MAX_TRIGGERS: usize = 12;
/// Longest trigger, in characters.
pub const MAX_TRIGGER_CHARS: usize = 120;
/// Most `{slot}`s in one trigger.
pub const MAX_SLOTS_PER_TRIGGER: usize = 8;
/// Longest instructions, in characters.
pub const MAX_INSTRUCTIONS_CHARS: usize = 2_000;
/// Most apps per skill.
pub const MAX_APPS: usize = 16;
/// Longest bundle id, in characters.
pub const MAX_BUNDLE_ID_CHARS: usize = 256;
/// Most parameters per skill.
pub const MAX_PARAMS: usize = 8;
/// Longest parameter name, in characters.
pub const MAX_PARAM_NAME_CHARS: usize = 24;
/// Longest parameter description, in characters.
pub const MAX_PARAM_DESCRIPTION_CHARS: usize = 200;
/// Most flow keys per skill.
pub const MAX_FLOW_KEYS: usize = 16;
/// Longest flow key, in characters.
pub const MAX_FLOW_KEY_CHARS: usize = 300;
/// Longest id (skills and workflows), in characters.
pub const MAX_ID_CHARS: usize = 48;
/// Longest argument value, in characters: the same cap as a matched request
/// ([`crate::matcher::MAX_TASK_CHARS`]), so a value captured from a request always fits.
/// Longer values are refused ([`SkillError::ArgTooLong`]), never cut.
pub const MAX_ARG_CHARS: usize = crate::matcher::MAX_TASK_CHARS;
/// Most allow-listed app-less tools per skill.
pub const MAX_TOOLS: usize = 16;
/// Longest tool name, in characters.
pub const MAX_TOOL_NAME_CHARS: usize = 64;

/// Every JSON field of [`Skill`]; other keys in a snapshot are reported as ignored.
pub(crate) const SKILL_FIELDS: &[&str] = &[
    "id",
    "name",
    "summary",
    "kind",
    "triggers",
    "instructions",
    "apps",
    "confine_to_apps",
    "tools",
    "params",
    "flow_keys",
    "enabled",
    "created_ms",
    "updated_ms",
    "runs",
    "last_run_ms",
];
/// Every JSON field of [`SkillParam`].
pub(crate) const PARAM_FIELDS: &[&str] = &["name", "description", "required"];

/// Where a skill came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillKind {
    /// Promoted from a learned flow (see `SkillStore::promote_flow`).
    Learned,
    /// Written by the owner.
    #[default]
    Written,
    /// Shipped with the app.
    BuiltIn,
}

/// A value the skill takes from the request, filled by a `{name}` slot in a trigger.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SkillParam {
    /// Name used in `{slots}` and as the argument key: `[a-z_][a-z0-9_]{0,23}`.
    pub name: String,
    /// What the value is, for the owner.
    pub description: String,
    /// Whether the skill needs a value to run.
    pub required: bool,
}

/// A named, reusable capability. See the module docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Skill {
    /// Stable id, 1 to 48 characters of `a-z`, `0-9` and `-`, unique in the store.
    pub id: String,
    /// What the owner calls it.
    pub name: String,
    /// One line on what it does.
    pub summary: String,
    /// Where it came from.
    pub kind: SkillKind,
    /// Phrases that trigger it; `{param}` slots capture values.
    pub triggers: Vec<String>,
    /// Owner-written guidance in plain language, shown to the model.
    pub instructions: String,
    /// Bundle ids the skill works in.
    pub apps: Vec<String>,
    /// Act only inside `apps` while the skill runs (see `jarvis_safety::check_confinement`).
    pub confine_to_apps: bool,
    /// App-less tools a confined skill may still call, by exact name.
    pub tools: Vec<String>,
    /// Declared parameters.
    pub params: Vec<SkillParam>,
    /// Intent keys (`jarvis_memory::intent_key` form) of learned flows that replay it.
    pub flow_keys: Vec<String>,
    /// Disabled skills never match.
    pub enabled: bool,
    /// When it was created, in the shell's milliseconds.
    pub created_ms: u64,
    /// When its definition last changed.
    pub updated_ms: u64,
    /// How many times it ran.
    pub runs: u32,
    /// When it last ran.
    pub last_run_ms: Option<u64>,
}

impl Default for Skill {
    /// An empty, enabled, written skill (not valid until it has an id and a name).
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            summary: String::new(),
            kind: SkillKind::Written,
            triggers: Vec::new(),
            instructions: String::new(),
            apps: Vec::new(),
            confine_to_apps: false,
            tools: Vec::new(),
            params: Vec::new(),
            flow_keys: Vec::new(),
            enabled: true,
            created_ms: 0,
            updated_ms: 0,
            runs: 0,
            last_run_ms: None,
        }
    }
}

/// One piece of a parsed trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Part {
    /// A literal word in normal form (see `crate::text`).
    Lit(String),
    /// A `{name}` slot.
    Slot(String),
}

/// Whether `name` is a valid parameter name: `[a-z_][a-z0-9_]{0,23}`.
pub fn is_valid_param_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first == '_')
        && name.len() <= MAX_PARAM_NAME_CHARS
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Whether `id` is a valid skill or workflow id: `[a-z0-9-]{1,48}`.
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_CHARS
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Parses a trigger into literal words (normalised like `intent_key`, filler dropped) and
/// slots. Linear in the trigger length.
///
/// Errors: unbalanced or nested braces, an invalid slot name, the same slot twice, more
/// than [`MAX_SLOTS_PER_TRIGGER`] slots, or no literal word at all (a trigger of only slots
/// would match anything).
pub(crate) fn parse_trigger(trigger: &str) -> Result<Vec<Part>, String> {
    let mut parts = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut rest = trigger;
    loop {
        let open = rest.find('{');
        let close = rest.find('}');
        match (open, close) {
            (None, None) => {
                push_literals(&mut parts, rest);
                break;
            }
            (None, Some(_)) => return Err("a } without a matching {".to_owned()),
            (Some(o), Some(c)) if c > o => {
                let before = rest.get(..o).unwrap_or_default();
                let name = rest.get(o + 1..c).unwrap_or_default();
                if name.contains('{') {
                    return Err("nested {".to_owned());
                }
                if !is_valid_param_name(name) {
                    return Err(format!("{{{name}}} is not a valid parameter name"));
                }
                if !seen.insert(name) {
                    return Err(format!("{{{name}}} appears twice"));
                }
                push_literals(&mut parts, before);
                parts.push(Part::Slot(name.to_owned()));
                rest = rest.get(c + 1..).unwrap_or_default();
            }
            (Some(_), Some(_)) => return Err("a } without a matching {".to_owned()),
            (Some(_), None) => return Err("a { without a matching }".to_owned()),
        }
    }
    if seen.len() > MAX_SLOTS_PER_TRIGGER {
        return Err(format!("at most {MAX_SLOTS_PER_TRIGGER} slots"));
    }
    if !parts.iter().any(|p| matches!(p, Part::Lit(_))) {
        return Err("needs at least one word besides slots and filler words".to_owned());
    }
    Ok(parts)
}

fn push_literals(parts: &mut Vec<Part>, chunk: &str) {
    parts.extend(
        words(chunk)
            .into_iter()
            .filter(|w| !w.filler)
            .map(|w| Part::Lit(w.norm)),
    );
}

/// Checks a single-line text field.
fn check_text(name: &str, value: &str, max: usize, required: bool) -> Result<(), SkillError> {
    if required && value.trim().is_empty() {
        return Err(field(name, "must not be empty"));
    }
    if longer_than(value, max) {
        return Err(field(name, format!("at most {max} characters")));
    }
    if has_hidden(value) {
        return Err(field(
            name,
            "must be one line without control or invisible characters",
        ));
    }
    Ok(())
}

/// Checks a bundle id: non-empty, at most [`MAX_BUNDLE_ID_CHARS`], no whitespace or control
/// characters.
pub(crate) fn check_bundle_id(name: &str, id: &str) -> Result<(), SkillError> {
    if id.is_empty() || longer_than(id, MAX_BUNDLE_ID_CHARS) {
        return Err(field(
            name,
            format!("a bundle id of 1 to {MAX_BUNDLE_ID_CHARS} characters"),
        ));
    }
    if id.chars().any(char::is_whitespace) || has_hidden(id) {
        return Err(field(
            name,
            "a bundle id has no spaces, control or invisible characters",
        ));
    }
    Ok(())
}

impl Skill {
    /// Checks every limit and rule: id, name, summary, triggers (count, length, slots that
    /// are declared parameters), instructions, apps (count, bundle-id shape, and at least
    /// one when `confine_to_apps`), tools, parameters (count, names, unique), flow keys.
    /// Names, summaries, triggers, descriptions, bundle ids, tools and flow keys must not
    /// contain control or invisible characters (zero-width, bidi controls...), which could
    /// make two different values look alike to the owner. Instructions may span lines; they
    /// are sanitised when they enter a prompt.
    pub fn validate(&self) -> Result<(), SkillError> {
        if !is_valid_id(&self.id) {
            return Err(SkillError::InvalidId(self.id.clone()));
        }
        check_text("name", &self.name, MAX_NAME_CHARS, true)?;
        check_text("summary", &self.summary, MAX_SUMMARY_CHARS, false)?;
        if self.params.len() > MAX_PARAMS {
            return Err(SkillError::TooMany {
                field: "params".to_owned(),
                max: MAX_PARAMS,
            });
        }
        let mut names: BTreeSet<&str> = BTreeSet::new();
        for (i, p) in self.params.iter().enumerate() {
            if !is_valid_param_name(&p.name) || !names.insert(p.name.as_str()) {
                return Err(SkillError::BadParam(p.name.clone()));
            }
            check_text(
                &format!("params[{i}].description"),
                &p.description,
                MAX_PARAM_DESCRIPTION_CHARS,
                false,
            )?;
        }
        if self.triggers.len() > MAX_TRIGGERS {
            return Err(SkillError::TooMany {
                field: "triggers".to_owned(),
                max: MAX_TRIGGERS,
            });
        }
        for (i, t) in self.triggers.iter().enumerate() {
            check_text(&format!("triggers[{i}]"), t, MAX_TRIGGER_CHARS, true)?;
            let parts = parse_trigger(t).map_err(|reason| SkillError::BadTrigger {
                trigger: t.clone(),
                reason,
            })?;
            for p in parts {
                if let Part::Slot(s) = p {
                    if !names.contains(s.as_str()) {
                        return Err(SkillError::UndeclaredSlot {
                            trigger: t.clone(),
                            slot: s,
                        });
                    }
                }
            }
        }
        if longer_than(&self.instructions, MAX_INSTRUCTIONS_CHARS) {
            return Err(field(
                "instructions",
                format!("at most {MAX_INSTRUCTIONS_CHARS} characters"),
            ));
        }
        if self.apps.len() > MAX_APPS {
            return Err(SkillError::TooMany {
                field: "apps".to_owned(),
                max: MAX_APPS,
            });
        }
        for (i, a) in self.apps.iter().enumerate() {
            check_bundle_id(&format!("apps[{i}]"), a)?;
        }
        if self.confine_to_apps && self.apps.is_empty() {
            return Err(field(
                "confine_to_apps",
                "a confined skill needs at least one app",
            ));
        }
        if self.tools.len() > MAX_TOOLS {
            return Err(SkillError::TooMany {
                field: "tools".to_owned(),
                max: MAX_TOOLS,
            });
        }
        for (i, t) in self.tools.iter().enumerate() {
            check_text(&format!("tools[{i}]"), t, MAX_TOOL_NAME_CHARS, true)?;
            if t.chars().any(char::is_whitespace) {
                return Err(field(format!("tools[{i}]"), "a tool name has no spaces"));
            }
        }
        if self.flow_keys.len() > MAX_FLOW_KEYS {
            return Err(SkillError::TooMany {
                field: "flow_keys".to_owned(),
                max: MAX_FLOW_KEYS,
            });
        }
        for (i, k) in self.flow_keys.iter().enumerate() {
            check_text(&format!("flow_keys[{i}]"), k, MAX_FLOW_KEY_CHARS, true)?;
        }
        Ok(())
    }

    /// The context a session runs this skill under, checked against the skill as it is now.
    ///
    /// Errors: the skill is disabled ([`SkillError::Disabled`]), an argument names an
    /// undeclared parameter ([`SkillError::UndeclaredArg`]), a value is longer than
    /// [`MAX_ARG_CHARS`] after cleaning ([`SkillError::ArgTooLong`]; values are never cut),
    /// or a required parameter has no value ([`SkillError::MissingParams`]). Values are
    /// cleaned (control and invisible characters removed, whitespace collapsed, trimmed);
    /// values that clean to nothing count as absent.
    pub fn context(&self, args: BTreeMap<String, String>) -> Result<SkillContext, SkillError> {
        if !self.enabled {
            return Err(SkillError::Disabled(self.id.clone()));
        }
        let args = self.checked_args(args)?;
        Ok(SkillContext {
            id: self.id.clone(),
            name: self.name.clone(),
            instructions: self.instructions.clone(),
            apps: self.apps.clone(),
            confine_to_apps: self.confine_to_apps,
            tools: self.tools.clone(),
            args,
        })
    }

    /// `args` checked and cleaned as [`Skill::context`] does (without the enabled check).
    pub fn checked_args(
        &self,
        args: BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, String>, SkillError> {
        let mut out = BTreeMap::new();
        for (k, v) in args {
            if !self.params.iter().any(|p| p.name == k) {
                return Err(SkillError::UndeclaredArg {
                    skill: self.id.clone(),
                    arg: k,
                });
            }
            let v = clean_arg(&v);
            if longer_than(&v, MAX_ARG_CHARS) {
                return Err(SkillError::ArgTooLong {
                    arg: k,
                    max: MAX_ARG_CHARS,
                });
            }
            if !v.is_empty() {
                out.insert(k, v);
            }
        }
        let missing = self.missing_params(&out);
        if !missing.is_empty() {
            return Err(SkillError::MissingParams {
                skill: self.id.clone(),
                params: missing,
            });
        }
        Ok(out)
    }

    /// Names of required parameters that `args` does not fill (blank counts as missing).
    pub fn missing_params(&self, args: &BTreeMap<String, String>) -> Vec<String> {
        self.params
            .iter()
            .filter(|p| p.required && args.get(&p.name).is_none_or(|v| v.trim().is_empty()))
            .map(|p| p.name.clone())
            .collect()
    }

    /// The task text for a session that runs this skill with `args`: the first trigger
    /// whose slots `args` all fill, with the values put in (`text {contact} {message}` ->
    /// `text mom on my way`), so learned flows keyed on that phrase replay; else the name.
    pub fn task_text(&self, args: &BTreeMap<String, String>) -> String {
        for t in &self.triggers {
            let Ok(parts) = parse_trigger(t) else {
                continue;
            };
            let fillable = parts.iter().all(|p| match p {
                Part::Slot(s) => args.get(s).is_some_and(|v| !v.trim().is_empty()),
                Part::Lit(_) => true,
            });
            if fillable {
                return fill_slots(t, args);
            }
        }
        self.name.clone()
    }
}

/// `trigger` with every `{name}` replaced by its (trimmed) value, in one pass so a value
/// that itself contains braces is never substituted again.
fn fill_slots(trigger: &str, args: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(trigger.len());
    let mut rest = trigger;
    while let Some(o) = rest.find('{') {
        let Some(c) = rest.get(o..).and_then(|r| r.find('}')).map(|c| c + o) else {
            break;
        };
        out.push_str(rest.get(..o).unwrap_or_default());
        let name = rest.get(o + 1..c).unwrap_or_default();
        match args.get(name) {
            Some(v) => out.push_str(v.trim()),
            None => out.push_str(rest.get(o..=c).unwrap_or_default()),
        }
        rest = rest.get(c + 1..).unwrap_or_default();
    }
    out.push_str(rest);
    out
}
