//! The workflow store: validated workflows in a stable order, a versioned JSON snapshot,
//! trigger lookups and run bookkeeping.
//!
//! Loading follows the skill store's rules: unknown fields are reported, unreadable entries
//! are kept verbatim and written back by [`WorkflowStore::to_json`].

use serde_json::Value;

use crate::error::SkillError;
use crate::schedule::{is_due, minutes_until_next};
use crate::store::{
    check_version, entry_label, keep_damaged, snapshot_json, unknown_keys, Damaged, SkillStore,
    Upsert,
};
use crate::text::longer_than;
use crate::workflow::{
    Step, Trigger, Weekday, Workflow, WorkflowOutcome, MAX_PHRASE_CHARS, WORKFLOW_FIELDS,
};

/// Schema version written by [`WorkflowStore::to_json`].
pub const WORKFLOWS_SCHEMA_VERSION: u32 = 1;
/// Most workflows in one store.
pub const MAX_WORKFLOWS: usize = 200;

/// Workflows ordered by id. Every stored workflow passed [`Workflow::validate`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkflowStore {
    workflows: Vec<Workflow>,
    damaged: Vec<Damaged>,
    issues: Vec<String>,
}

impl WorkflowStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of (valid) workflows.
    pub fn len(&self) -> usize {
        self.workflows.len()
    }

    /// True when there are no valid workflows.
    pub fn is_empty(&self) -> bool {
        self.workflows.is_empty()
    }

    /// All workflows, ordered by id.
    pub fn list(&self) -> &[Workflow] {
        &self.workflows
    }

    /// What loading reported (ignored fields, entries kept verbatim).
    pub fn load_issues(&self) -> &[String] {
        &self.issues
    }

    /// Whether loading reported anything.
    pub fn has_load_issues(&self) -> bool {
        !self.issues.is_empty()
    }

    fn position(&self, id: &str) -> Option<usize> {
        self.workflows
            .binary_search_by(|w| w.id.as_str().cmp(id))
            .ok()
    }

    /// The workflow with this id.
    pub fn get(&self, id: &str) -> Option<&Workflow> {
        self.position(id).and_then(|i| self.workflows.get(i))
    }

    /// Adds a new workflow, timestamps as given. Fails when it is invalid, the id is taken
    /// or the store is full. A damaged entry with the same id is replaced.
    pub fn add(&mut self, workflow: Workflow) -> Result<(), SkillError> {
        workflow.validate()?;
        match self.workflows.binary_search_by(|w| w.id.cmp(&workflow.id)) {
            Ok(_) => Err(SkillError::DuplicateId(workflow.id)),
            Err(_) if self.workflows.len() >= MAX_WORKFLOWS => {
                Err(SkillError::StoreFull { max: MAX_WORKFLOWS })
            }
            Err(pos) => {
                let id = workflow.id.clone();
                self.damaged
                    .retain(|d| d.id.as_deref() != Some(id.as_str()));
                self.workflows.insert(pos, workflow);
                Ok(())
            }
        }
    }

    /// Replaces an existing workflow's definition, keeping the stored `created_ms`, `runs`,
    /// `last_run_ms` and `last_outcome`; `updated_ms` becomes `now_ms`.
    pub fn update(&mut self, mut workflow: Workflow, now_ms: u64) -> Result<(), SkillError> {
        workflow.validate()?;
        let pos = self
            .position(&workflow.id)
            .ok_or_else(|| SkillError::NotFound(workflow.id.clone()))?;
        let slot = self
            .workflows
            .get_mut(pos)
            .ok_or_else(|| SkillError::NotFound(workflow.id.clone()))?;
        workflow.created_ms = slot.created_ms;
        workflow.runs = slot.runs;
        workflow.last_run_ms = slot.last_run_ms;
        workflow.last_outcome = slot.last_outcome.take();
        workflow.updated_ms = now_ms;
        *slot = workflow;
        Ok(())
    }

    /// [`WorkflowStore::update`] when the id exists, else [`WorkflowStore::add`] with
    /// `created_ms` and `updated_ms` set to `now_ms` and no runs.
    pub fn upsert(&mut self, mut workflow: Workflow, now_ms: u64) -> Result<Upsert, SkillError> {
        if self.position(&workflow.id).is_some() {
            self.update(workflow, now_ms).map(|()| Upsert::Updated)
        } else {
            workflow.created_ms = now_ms;
            workflow.updated_ms = now_ms;
            workflow.runs = 0;
            workflow.last_run_ms = None;
            workflow.last_outcome = None;
            self.add(workflow).map(|()| Upsert::Inserted)
        }
    }

    /// Removes and returns the workflow with this id (a damaged entry with the id goes too).
    pub fn remove(&mut self, id: &str) -> Option<Workflow> {
        self.damaged.retain(|d| d.id.as_deref() != Some(id));
        self.position(id).map(|i| self.workflows.remove(i))
    }

    /// Records a finished run: `runs` (saturating), `last_run_ms`, `last_outcome`.
    pub fn record_run(
        &mut self,
        id: &str,
        now_ms: u64,
        outcome: &WorkflowOutcome,
    ) -> Result<(), SkillError> {
        let pos = self
            .position(id)
            .ok_or_else(|| SkillError::NotFound(id.to_owned()))?;
        if let Some(w) = self.workflows.get_mut(pos) {
            w.runs = w.runs.saturating_add(1);
            w.last_run_ms = Some(now_ms);
            w.last_outcome = Some(outcome.clone());
        }
        Ok(())
    }

    /// Problems that do not stop a workflow from being saved but will fail a step when it
    /// runs: `RunSkill` steps naming a skill that does not exist or is disabled, arguments
    /// the skill does not declare, required parameters left empty, values too long. One
    /// line each.
    pub fn warnings(workflow: &Workflow, skills: &SkillStore) -> Vec<String> {
        let mut out = Vec::new();
        for (i, step) in workflow.steps.iter().enumerate() {
            let Step::RunSkill { skill_id, args } = step else {
                continue;
            };
            let n = i + 1;
            let Some(skill) = skills.get(skill_id) else {
                out.push(format!("step {n}: there is no skill \"{skill_id}\""));
                continue;
            };
            if let Err(e) = skill.context(args.clone()) {
                out.push(format!("step {n}: {e}"));
            }
        }
        out
    }

    /// Enabled workflows whose `Phrase` trigger equals `text` after `intent_key`
    /// normalisation, ordered by id. Text longer than a few phrase lengths matches nothing.
    pub fn match_phrase(&self, text: &str) -> Vec<&Workflow> {
        if longer_than(text, MAX_PHRASE_CHARS * 4) {
            return Vec::new();
        }
        let key = jarvis_memory::intent_key(text);
        if key.is_empty() {
            return Vec::new();
        }
        self.workflows
            .iter()
            .filter(|w| w.enabled)
            .filter(|w| match &w.trigger {
                Trigger::Phrase { text } => jarvis_memory::intent_key(text) == key,
                _ => false,
            })
            .collect()
    }

    /// Enabled workflows whose `External` trigger has this key (ignoring ASCII case and
    /// surrounding whitespace), ordered by id.
    pub fn for_external(&self, key: &str) -> Vec<&Workflow> {
        let key = key.trim();
        if key.is_empty() {
            return Vec::new();
        }
        self.workflows
            .iter()
            .filter(|w| w.enabled)
            .filter(|w| match &w.trigger {
                Trigger::External { key: k } => k.trim().eq_ignore_ascii_case(key),
                _ => false,
            })
            .collect()
    }

    /// Enabled scheduled workflows that fired within the last `window_minutes` minutes of
    /// local time (`weekday`, `minute_of_day`), ordered by id; see [`crate::schedule::is_due`].
    /// A `minute_of_day` above 1439 is an error.
    pub fn due(
        &self,
        weekday: Weekday,
        minute_of_day: u32,
        window_minutes: u32,
    ) -> Result<Vec<&Workflow>, SkillError> {
        let mut out = Vec::new();
        for w in self.workflows.iter().filter(|w| w.enabled) {
            if let Trigger::Schedule(s) = &w.trigger {
                if is_due(s, weekday, minute_of_day, window_minutes)? {
                    out.push(w);
                }
            }
        }
        Ok(out)
    }

    /// The enabled scheduled workflow that fires next and the minutes until it does (ties
    /// go to the lower id), for the shell to schedule a wake-up.
    pub fn next_scheduled(
        &self,
        weekday: Weekday,
        minute_of_day: u32,
    ) -> Result<Option<(&Workflow, u32)>, SkillError> {
        let mut best: Option<(&Workflow, u32)> = None;
        for w in self.workflows.iter().filter(|w| w.enabled) {
            if let Trigger::Schedule(s) = &w.trigger {
                let m = minutes_until_next(s, weekday, minute_of_day)?;
                if best.is_none_or(|(_, b)| m < b) {
                    best = Some((w, m));
                }
            }
        }
        Ok(best)
    }

    /// Serializes to `{"schema_version":1,"workflows":[...]}`, entries that could not be
    /// loaded last and unchanged.
    pub fn to_json(&self) -> Result<String, SkillError> {
        snapshot_json(
            WORKFLOWS_SCHEMA_VERSION,
            "workflows",
            &self.workflows,
            &self.damaged,
        )
    }

    /// Parses a snapshot; see [`WorkflowStore::from_json_with_issues`].
    pub fn from_json(json: &str) -> Result<WorkflowStore, SkillError> {
        Self::from_json_with_issues(json).map(|(s, _)| s)
    }

    /// Parses a snapshot, also returning its [`WorkflowStore::load_issues`]. Same rules as
    /// `SkillStore::from_json_with_issues`: structural damage and newer schemas are errors;
    /// unknown fields are reported; damaged or duplicate entries are kept and reported.
    pub fn from_json_with_issues(json: &str) -> Result<(WorkflowStore, Vec<String>), SkillError> {
        let value: Value = serde_json::from_str(json)?;
        let obj = value
            .as_object()
            .ok_or_else(|| SkillError::Invalid("top level is not an object".to_owned()))?;
        check_version(obj, WORKFLOWS_SCHEMA_VERSION)?;
        let mut store = WorkflowStore::new();
        let mut issues = unknown_keys("snapshot", obj, &["schema_version", "workflows"]);
        let items = match obj.get("workflows") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(items)) => items.clone(),
            Some(_) => return Err(SkillError::Invalid("workflows is not an array".to_owned())),
        };
        let mut damaged = Vec::new();
        for (i, item) in items.into_iter().enumerate() {
            let label = entry_label("workflows", i, &item);
            if let Some(o) = item.as_object() {
                issues.extend(unknown_keys(&label, o, WORKFLOW_FIELDS));
            }
            let result = serde_json::from_value::<Workflow>(item.clone())
                .map_err(SkillError::from)
                .and_then(|w| store.add(w));
            if let Err(e) = result {
                keep_damaged(&mut damaged, &mut issues, &label, item, &e);
            }
        }
        store.damaged = damaged;
        store.issues = issues.clone();
        Ok((store, issues))
    }
}
