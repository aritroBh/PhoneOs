//! The skill store: validated skills in a stable order, a versioned JSON snapshot, run
//! statistics and promotion of learned flows.
//!
//! Loading is forward compatible and loses nothing: unknown fields are ignored but
//! reported, and an entry that cannot be read (damaged, from a newer version, over a limit,
//! a duplicate id) is kept verbatim and written back by [`SkillStore::to_json`], so saving
//! never destroys a skill the owner relies on. [`SkillStore::load_issues`] lists both.

use jarvis_memory::Flow;
use serde_json::{Map, Value};

use crate::error::SkillError;
use crate::matcher::{compile, CompiledSkill};
use crate::skill::{
    is_valid_id, Skill, SkillKind, SkillParam, MAX_ID_CHARS, MAX_NAME_CHARS, MAX_TRIGGER_CHARS,
    PARAM_FIELDS, SKILL_FIELDS,
};
use crate::text::truncate_chars;

/// Schema version written by [`SkillStore::to_json`].
pub const SKILLS_SCHEMA_VERSION: u32 = 1;
/// Most skills in one store.
pub const MAX_SKILLS: usize = 500;
/// Most unreadable entries kept verbatim (in either store); further ones are dropped and
/// reported.
pub const MAX_DAMAGED: usize = 500;

/// What [`SkillStore::upsert`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upsert {
    /// A new item was added.
    Inserted,
    /// An existing item was replaced.
    Updated,
}

/// An entry of a snapshot that could not be loaded, kept verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Damaged {
    /// Its `id`, when it has a string one.
    pub id: Option<String>,
    /// The entry as it was read.
    pub raw: Value,
}

/// Skills ordered by id. Every stored skill passed [`Skill::validate`]; ids are unique.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SkillStore {
    skills: Vec<Skill>,
    /// `compile(skills[i])`, kept in step with `skills`.
    compiled: Vec<CompiledSkill>,
    damaged: Vec<Damaged>,
    issues: Vec<String>,
}

impl SkillStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of (valid) skills.
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// True when the store holds no valid skills.
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

    pub(crate) fn compiled(&self) -> (&[Skill], &[CompiledSkill]) {
        (&self.skills, &self.compiled)
    }

    /// What loading reported: one line per ignored unknown field and per entry that could
    /// not be loaded (kept verbatim, see the module docs). Empty for a store built in code.
    pub fn load_issues(&self) -> &[String] {
        &self.issues
    }

    /// Whether loading reported anything; the app should show [`SkillStore::load_issues`].
    pub fn has_load_issues(&self) -> bool {
        !self.issues.is_empty()
    }

    /// Ids of entries kept verbatim because they could not be loaded (`None` for an entry
    /// without a readable id).
    pub fn damaged_ids(&self) -> Vec<Option<String>> {
        self.damaged.iter().map(|d| d.id.clone()).collect()
    }

    fn position(&self, id: &str) -> Option<usize> {
        self.skills.binary_search_by(|s| s.id.as_str().cmp(id)).ok()
    }

    /// Adds a new skill, keeping its timestamps and kind as given. Fails when it is
    /// invalid, the id is taken or the store is full. A damaged entry with the same id is
    /// replaced (the owner re-created the skill).
    pub fn add(&mut self, skill: Skill) -> Result<(), SkillError> {
        skill.validate()?;
        match self.skills.binary_search_by(|s| s.id.cmp(&skill.id)) {
            Ok(_) => Err(SkillError::DuplicateId(skill.id)),
            Err(_) if self.skills.len() >= MAX_SKILLS => {
                Err(SkillError::StoreFull { max: MAX_SKILLS })
            }
            Err(pos) => {
                self.discard_damaged(&skill.id);
                self.compiled.insert(pos, compile(&skill));
                self.skills.insert(pos, skill);
                Ok(())
            }
        }
    }

    /// Replaces the definition of an existing skill. The store's bookkeeping is kept:
    /// `kind` (see [`SkillStore::set_kind`]), `created_ms`, `runs` and `last_run_ms`;
    /// `updated_ms` becomes `now_ms`.
    pub fn update(&mut self, mut skill: Skill, now_ms: u64) -> Result<(), SkillError> {
        skill.validate()?;
        let pos = self
            .position(&skill.id)
            .ok_or_else(|| SkillError::NotFound(skill.id.clone()))?;
        let slot = self
            .skills
            .get_mut(pos)
            .ok_or_else(|| SkillError::NotFound(skill.id.clone()))?;
        skill.kind = slot.kind;
        skill.created_ms = slot.created_ms;
        skill.runs = slot.runs;
        skill.last_run_ms = slot.last_run_ms;
        skill.updated_ms = now_ms;
        if let Some(c) = self.compiled.get_mut(pos) {
            *c = compile(&skill);
        }
        *slot = skill;
        Ok(())
    }

    /// [`SkillStore::update`] when the id exists, else [`SkillStore::add`] with
    /// `created_ms` and `updated_ms` set to `now_ms` and no runs.
    pub fn upsert(&mut self, mut skill: Skill, now_ms: u64) -> Result<Upsert, SkillError> {
        if self.position(&skill.id).is_some() {
            self.update(skill, now_ms).map(|()| Upsert::Updated)
        } else {
            skill.created_ms = now_ms;
            skill.updated_ms = now_ms;
            skill.runs = 0;
            skill.last_run_ms = None;
            self.add(skill).map(|()| Upsert::Inserted)
        }
    }

    /// Changes where a skill is recorded as coming from (the one way to change `kind`).
    pub fn set_kind(&mut self, id: &str, kind: SkillKind, now_ms: u64) -> Result<(), SkillError> {
        let pos = self
            .position(id)
            .ok_or_else(|| SkillError::NotFound(id.to_owned()))?;
        if let Some(s) = self.skills.get_mut(pos) {
            s.kind = kind;
            s.updated_ms = now_ms;
        }
        Ok(())
    }

    /// Removes and returns the skill with this id (a damaged entry with the id goes too).
    pub fn remove(&mut self, id: &str) -> Option<Skill> {
        self.discard_damaged(id);
        let pos = self.position(id)?;
        if pos < self.compiled.len() {
            self.compiled.remove(pos);
        }
        Some(self.skills.remove(pos))
    }

    /// Drops the damaged entries with this id; returns whether there were any.
    pub fn discard_damaged(&mut self, id: &str) -> bool {
        let before = self.damaged.len();
        self.damaged.retain(|d| d.id.as_deref() != Some(id));
        before != self.damaged.len()
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

    /// Serializes to the versioned snapshot `{"schema_version":1,"skills":[...]}`. Entries
    /// that could not be loaded follow the valid skills, unchanged.
    pub fn to_json(&self) -> Result<String, SkillError> {
        snapshot_json(SKILLS_SCHEMA_VERSION, "skills", &self.skills, &self.damaged)
    }

    /// Parses a snapshot; see [`SkillStore::from_json_with_issues`].
    pub fn from_json(json: &str) -> Result<SkillStore, SkillError> {
        Self::from_json_with_issues(json).map(|(store, _)| store)
    }

    /// Parses a snapshot written by this or an older version, also returning its
    /// [`SkillStore::load_issues`].
    ///
    /// Errors (never a panic): not JSON, not an object, `skills` not an array, a
    /// `schema_version` that is not an unsigned integer or is newer than
    /// [`SKILLS_SCHEMA_VERSION`]. Unknown fields (top level, skill, parameter) are ignored
    /// and reported: `skill "a": unknown field confineToApps ignored`. A skill entry that
    /// does not deserialize or validate, a duplicate id (the first one wins) and entries
    /// beyond [`MAX_SKILLS`] are kept verbatim (up to [`MAX_DAMAGED`]) and reported with
    /// their id and name, so one damaged entry cannot lose the owner's skills.
    pub fn from_json_with_issues(json: &str) -> Result<(SkillStore, Vec<String>), SkillError> {
        let value: Value = serde_json::from_str(json)?;
        let obj = value
            .as_object()
            .ok_or_else(|| SkillError::Invalid("top level is not an object".to_owned()))?;
        check_version(obj, SKILLS_SCHEMA_VERSION)?;
        let mut store = SkillStore::new();
        let mut issues = unknown_keys("snapshot", obj, &["schema_version", "skills"]);
        let items = match obj.get("skills") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(items)) => items.clone(),
            Some(_) => return Err(SkillError::Invalid("skills is not an array".to_owned())),
        };
        let mut damaged = Vec::new();
        for (i, item) in items.into_iter().enumerate() {
            let label = entry_label("skills", i, &item);
            if let Some(o) = item.as_object() {
                issues.extend(unknown_keys(&label, o, SKILL_FIELDS));
                if let Some(Value::Array(params)) = o.get("params") {
                    for (j, p) in params.iter().enumerate() {
                        if let Some(po) = p.as_object() {
                            issues.extend(unknown_keys(
                                &format!("{label} params[{j}]"),
                                po,
                                PARAM_FIELDS,
                            ));
                        }
                    }
                }
            }
            let result = serde_json::from_value::<Skill>(item.clone())
                .map_err(SkillError::from)
                .and_then(|skill| store.add(skill));
            if let Err(e) = result {
                keep_damaged(&mut damaged, &mut issues, &label, item, &e);
            }
        }
        store.damaged = damaged;
        store.issues = issues.clone();
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
            tools: Vec::new(),
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

/// `skills[3] "id" (name)`: the entry's position, id and name when readable.
pub(crate) fn entry_label(section: &str, index: usize, item: &Value) -> String {
    let mut label = format!("{section}[{index}]");
    if let Some(id) = item.get("id").and_then(Value::as_str) {
        label.push_str(&format!(" {:?}", truncate_chars(id, 60)));
    }
    if let Some(name) = item.get("name").and_then(Value::as_str) {
        label.push_str(&format!(" ({})", truncate_chars(name, 60)));
    }
    label
}

/// One issue per key of `obj` not in `known`.
pub(crate) fn unknown_keys(label: &str, obj: &Map<String, Value>, known: &[&str]) -> Vec<String> {
    obj.keys()
        .filter(|k| !known.contains(&k.as_str()))
        .take(32)
        .map(|k| format!("{label}: unknown field {} ignored", truncate_chars(k, 60)))
        .collect()
}

/// Keeps an unreadable entry verbatim (up to [`MAX_DAMAGED`]) and reports it.
pub(crate) fn keep_damaged(
    damaged: &mut Vec<Damaged>,
    issues: &mut Vec<String>,
    label: &str,
    raw: Value,
    error: &SkillError,
) {
    if damaged.len() >= MAX_DAMAGED {
        issues.push(format!(
            "{label}: dropped: {error} (more than {MAX_DAMAGED} unreadable entries)"
        ));
        return;
    }
    issues.push(format!("{label}: not loaded, kept unchanged: {error}"));
    damaged.push(Damaged {
        id: raw.get("id").and_then(Value::as_str).map(str::to_owned),
        raw,
    });
}

/// `{"schema_version":v,"<key>":[items..., damaged...]}`.
pub(crate) fn snapshot_json<T: serde::Serialize>(
    version: u32,
    key: &str,
    items: &[T],
    damaged: &[Damaged],
) -> Result<String, SkillError> {
    let mut list: Vec<Value> = Vec::with_capacity(items.len() + damaged.len());
    for item in items {
        list.push(serde_json::to_value(item)?);
    }
    list.extend(damaged.iter().map(|d| d.raw.clone()));
    let mut obj = Map::new();
    obj.insert("schema_version".to_owned(), Value::from(version));
    obj.insert(key.to_owned(), Value::Array(list));
    Ok(serde_json::to_string(&Value::Object(obj))?)
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
