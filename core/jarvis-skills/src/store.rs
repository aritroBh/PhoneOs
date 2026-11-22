//! The skill store: validated skills in a stable order, a versioned JSON snapshot, run
//! statistics and promotion of learned flows.

use jarvis_memory::Flow;
use serde::Serialize;
use serde_json::Value;

use crate::error::SkillError;
use crate::skill::{
    is_valid_id, Skill, SkillKind, SkillParam, MAX_ID_CHARS, MAX_NAME_CHARS, MAX_TRIGGER_CHARS,
};
use crate::text::truncate_chars;

/// Schema version written by [`SkillStore::to_json`].
pub const SKILLS_SCHEMA_VERSION: u32 = 1;
/// Most skills in one store.
pub const MAX_SKILLS: usize = 500;

/// What [`SkillStore::upsert`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upsert {
    /// A new item was added.
    Inserted,
    /// An existing item was replaced.
    Updated,
}

/// Skills ordered by id. Every stored skill passed [`Skill::validate`]; ids are unique.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SkillStore {
    skills: Vec<Skill>,
}

#[derive(Serialize)]
struct SnapshotRef<'a> {
    schema_version: u32,
    skills: &'a [Skill],
}

impl SkillStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of skills.
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// True when the store holds no skills.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// All skills, ordered by id.
    pub fn list(&self) -> &[Skill] {
        &self.skills
    }

    /// The skill with this id.
    pub fn get(&self, id: &str) -> Option<&Skill> {
        self.position(id).and_then(|i| self.skills.get(i))
    }

    fn position(&self, id: &str) -> Option<usize> {
        self.skills.binary_search_by(|s| s.id.as_str().cmp(id)).ok()
    }

    /// Adds a new skill. Fails when it is invalid, the id is taken or the store is full.
    pub fn add(&mut self, skill: Skill) -> Result<(), SkillError> {
        skill.validate()?;
        match self.skills.binary_search_by(|s| s.id.cmp(&skill.id)) {
            Ok(_) => Err(SkillError::DuplicateId(skill.id)),
            Err(_) if self.skills.len() >= MAX_SKILLS => {
                Err(SkillError::StoreFull { max: MAX_SKILLS })
            }
            Err(pos) => {
                self.skills.insert(pos, skill);
                Ok(())
            }
        }
    }

    /// Replaces the definition of an existing skill. The stored `created_ms`, `runs` and
    /// `last_run_ms` are kept (they are the store's bookkeeping, not part of the
    /// definition); everything else, `updated_ms` included, comes from `skill`.
    pub fn update(&mut self, mut skill: Skill) -> Result<(), SkillError> {
        skill.validate()?;
        let pos = self
            .position(&skill.id)
            .ok_or_else(|| SkillError::NotFound(skill.id.clone()))?;
        let slot = self
            .skills
            .get_mut(pos)
            .ok_or_else(|| SkillError::NotFound(skill.id.clone()))?;
        skill.created_ms = slot.created_ms;
        skill.runs = slot.runs;
        skill.last_run_ms = slot.last_run_ms;
        *slot = skill;
        Ok(())
    }

    /// [`SkillStore::update`] when the id exists, else [`SkillStore::add`].
    pub fn upsert(&mut self, skill: Skill) -> Result<Upsert, SkillError> {
        if self.position(&skill.id).is_some() {
            self.update(skill).map(|()| Upsert::Updated)
        } else {
            self.add(skill).map(|()| Upsert::Inserted)
        }
    }

    /// Removes and returns the skill with this id.
    pub fn remove(&mut self, id: &str) -> Option<Skill> {
        self.position(id).map(|i| self.skills.remove(i))
    }

    /// Records a run: increments `runs` (saturating) and sets `last_run_ms`.
    pub fn record_run(&mut self, id: &str, now_ms: u64) -> Result<(), SkillError> {
        let pos = self
            .position(id)
            .ok_or_else(|| SkillError::NotFound(id.to_owned()))?;
        if let Some(s) = self.skills.get_mut(pos) {
            s.runs = s.runs.saturating_add(1);
            s.last_run_ms = Some(now_ms);
        }
        Ok(())
    }

    /// Serializes to the versioned snapshot `{"schema_version":1,"skills":[...]}`.
    pub fn to_json(&self) -> Result<String, SkillError> {
        Ok(serde_json::to_string(&SnapshotRef {
            schema_version: SKILLS_SCHEMA_VERSION,
            skills: &self.skills,
        })?)
    }

    /// Parses a snapshot; see [`SkillStore::from_json_with_issues`]. The issues are dropped.
    pub fn from_json(json: &str) -> Result<SkillStore, SkillError> {
        Self::from_json_with_issues(json).map(|(store, _)| store)
    }

    /// Parses a snapshot written by this or an older version, also returning one line per
    /// skill that was skipped.
    ///
    /// Errors (never a panic): not JSON, not an object, `skills` not an array, a
    /// `schema_version` that is not an unsigned integer or is newer than
    /// [`SKILLS_SCHEMA_VERSION`]. Unknown fields are ignored. A skill entry that does not
    /// deserialize or validate, a duplicate id (the first one wins) and entries beyond
    /// [`MAX_SKILLS`] are skipped and reported, so one damaged entry cannot lose the
    /// owner's other skills.
    pub fn from_json_with_issues(json: &str) -> Result<(SkillStore, Vec<String>), SkillError> {
        let value: Value = serde_json::from_str(json)?;
        let obj = value
            .as_object()
            .ok_or_else(|| SkillError::Invalid("top level is not an object".to_owned()))?;
        check_version(obj, SKILLS_SCHEMA_VERSION)?;
        let mut store = SkillStore::new();
        let mut issues = Vec::new();
        let items = match obj.get("skills") {
            None | Some(Value::Null) => return Ok((store, issues)),
            Some(Value::Array(items)) => items,
            Some(_) => return Err(SkillError::Invalid("skills is not an array".to_owned())),
        };
        for (i, item) in items.iter().enumerate() {
            if store.len() >= MAX_SKILLS {
                issues.push(format!(
                    "skills[{i}..]: skipped, the store holds at most {MAX_SKILLS} skills"
                ));
                break;
            }
            match serde_json::from_value::<Skill>(item.clone()) {
                Ok(skill) => {
                    if let Err(e) = store.add(skill) {
                        issues.push(format!("skills[{i}]: skipped: {e}"));
                    }
                }
                Err(e) => issues.push(format!("skills[{i}]: skipped: {e}")),
            }
        }
        Ok((store, issues))
    }

    /// Builds a [`SkillKind::Learned`] skill from a learned flow and adds it; returns its
    /// id.
    ///
    /// The trigger is the flow's intent key with its numbered slots renamed (`{1}` ->
    /// `{slot1}`, each a required parameter), so the skill matches the same requests the
    /// flow does and [`Skill::task_text`] reproduces a task the flow replays on. The app is
    /// the flow's app; confinement starts off (the owner can turn it on). The id is derived
    /// from `name` (or the key when `name` is blank) and made unique with a numeric suffix.
    ///
    /// Errors: the flow is retired, has no key or app, already backs a skill
    /// ([`SkillError::AlreadyPromoted`]), or the result does not validate (for example a key
    /// longer than a trigger may be).
    pub fn promote_flow(
        &mut self,
        flow: &Flow,
        name: &str,
        now_ms: u64,
    ) -> Result<String, SkillError> {
        if flow.retired {
            return Err(SkillError::Flow("the flow is retired".to_owned()));
        }
        let key = flow.intent_key.trim();
        if key.is_empty() {
            return Err(SkillError::Flow("the flow has no intent key".to_owned()));
        }
        if flow.app_bundle_id.trim().is_empty() {
            return Err(SkillError::Flow("the flow has no app".to_owned()));
        }
        if let Some(existing) = self
            .skills
            .iter()
            .find(|s| s.flow_keys.iter().any(|k| k == key))
        {
            return Err(SkillError::AlreadyPromoted(existing.id.clone()));
        }
        let mut params = Vec::new();
        let trigger: Vec<String> = key
            .split(' ')
            .filter(|t| !t.is_empty())
            .map(|t| {
                match t
                    .strip_prefix('{')
                    .and_then(|r| r.strip_suffix('}'))
                    .and_then(|n| n.parse::<u32>().ok())
                {
                    Some(n) => {
                        let name = format!("slot{n}");
                        if !params.iter().any(|p: &SkillParam| p.name == name) {
                            params.push(SkillParam {
                                name: name.clone(),
                                description: format!("Value {n} of the learned task"),
                                required: true,
                            });
                        }
                        format!("{{{name}}}")
                    }
                    None => t.to_owned(),
                }
            })
            .collect();
        let trigger = trigger.join(" ");
        if trigger.chars().count() > MAX_TRIGGER_CHARS {
            return Err(SkillError::Flow(format!(
                "the intent key is longer than a trigger may be ({MAX_TRIGGER_CHARS} characters)"
            )));
        }
        let display = if name.trim().is_empty() {
            truncate_chars(key, MAX_NAME_CHARS).to_owned()
        } else {
            name.trim().to_owned()
        };
        let id = self.unique_id(&display);
        let skill = Skill {
            id: id.clone(),
            name: display,
            summary: "Learned from a task Jarvis completed.".to_owned(),
            kind: SkillKind::Learned,
            triggers: vec![trigger],
            instructions: String::new(),
            apps: vec![flow.app_bundle_id.trim().to_owned()],
            confine_to_apps: false,
            params,
            flow_keys: vec![key.to_owned()],
            enabled: true,
            created_ms: now_ms,
            updated_ms: now_ms,
            runs: 0,
            last_run_ms: None,
        };
        self.add(skill)?;
        Ok(id)
    }

    /// A free id derived from `name`: lowercase ASCII letters and digits, other runs
    /// become `-`, capped, `skill` when nothing is left, then `-2`, `-3`... until free.
    pub fn unique_id(&self, name: &str) -> String {
        let mut base = String::new();
        for c in name.chars() {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                base.push(c);
            } else if !base.is_empty() && !base.ends_with('-') {
                base.push('-');
            }
            if base.len() >= MAX_ID_CHARS - 4 {
                break;
            }
        }
        let base = base.trim_matches('-');
        let base = if base.is_empty() { "skill" } else { base };
        if self.get(base).is_none() && is_valid_id(base) {
            return base.to_owned();
        }
        (2u32..)
            .map(|n| format!("{base}-{n}"))
            .find(|id| self.get(id).is_none())
            .unwrap_or_else(|| base.to_owned())
    }
}

/// Rejects a `schema_version` newer than `supported` (or not an unsigned integer).
pub(crate) fn check_version(
    obj: &serde_json::Map<String, Value>,
    supported: u32,
) -> Result<(), SkillError> {
    match obj.get("schema_version") {
        None | Some(Value::Null) => Ok(()),
        Some(v) => {
            let found = v.as_u64().ok_or_else(|| {
                SkillError::Invalid("schema_version is not an unsigned integer".to_owned())
            })?;
            if found > u64::from(supported) {
                Err(SkillError::NewerSchema { found, supported })
            } else {
                Ok(())
            }
        }
    }
}
