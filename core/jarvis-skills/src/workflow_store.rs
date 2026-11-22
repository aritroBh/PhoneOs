//! The workflow store: validated workflows in a stable order, a versioned JSON snapshot,
//! trigger lookups and run bookkeeping.

use serde::Serialize;
use serde_json::Value;

use crate::error::SkillError;
use crate::schedule::{is_due, minutes_until_next};
use crate::skill::Skill;
use crate::store::{check_version, SkillStore, Upsert};
use crate::text::longer_than;
use crate::workflow::{Step, Trigger, Weekday, Workflow, WorkflowOutcome, MAX_PHRASE_CHARS};

/// Schema version written by [`WorkflowStore::to_json`].
pub const WORKFLOWS_SCHEMA_VERSION: u32 = 1;
/// Most workflows in one store.
pub const MAX_WORKFLOWS: usize = 200;

/// Workflows ordered by id. Every stored workflow passed [`Workflow::validate`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkflowStore {
    workflows: Vec<Workflow>,
}

#[derive(Serialize)]
struct SnapshotRef<'a> {
    schema_version: u32,
    workflows: &'a [Workflow],
}

impl WorkflowStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of workflows.
    pub fn len(&self) -> usize {
        self.workflows.len()
    }

    /// True when empty.
    pub fn is_empty(&self) -> bool {
        self.workflows.is_empty()
    }

    /// All workflows, ordered by id.
    pub fn list(&self) -> &[Workflow] {
        &self.workflows
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

    /// Adds a new workflow. Fails when it is invalid, the id is taken or the store is full.
    pub fn add(&mut self, workflow: Workflow) -> Result<(), SkillError> {
        workflow.validate()?;
        match self.workflows.binary_search_by(|w| w.id.cmp(&workflow.id)) {
            Ok(_) => Err(SkillError::DuplicateId(workflow.id)),
            Err(_) if self.workflows.len() >= MAX_WORKFLOWS => {
                Err(SkillError::StoreFull { max: MAX_WORKFLOWS })
            }
            Err(pos) => {
                self.workflows.insert(pos, workflow);
                Ok(())
            }
        }
    }

    /// Replaces an existing workflow's definition, keeping the stored `created_ms`, `runs`,
    /// `last_run_ms` and `last_outcome`.
    pub fn update(&mut self, mut workflow: Workflow) -> Result<(), SkillError> {
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
        *slot = workflow;
        Ok(())
    }

    /// [`WorkflowStore::update`] when the id exists, else [`WorkflowStore::add`].
    pub fn upsert(&mut self, workflow: Workflow) -> Result<Upsert, SkillError> {
        if self.position(&workflow.id).is_some() {
            self.update(workflow).map(|()| Upsert::Updated)
        } else {
            self.add(workflow).map(|()| Upsert::Inserted)
        }
    }

    /// Removes and returns the workflow with this id.
    pub fn remove(&mut self, id: &str) -> Option<Workflow> {
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
    /// the skill does not declare, required parameters left empty. One line each.
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
            if !skill.enabled {
                out.push(format!(
                    "step {n}: the skill \"{}\" is disabled",
                    skill.name
                ));
            }
            for k in args.keys() {
                if !skill.params.iter().any(|p| p.name == *k) {
                    out.push(format!(
                        "step {n}: the skill \"{}\" has no parameter \"{k}\"",
                        skill.name
                    ));
                }
            }
            let missing = Skill::missing_params(skill, args);
            if !missing.is_empty() {
                out.push(format!(
                    "step {n}: the skill \"{}\" needs {}",
                    skill.name,
                    missing.join(", ")
                ));
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
    pub fn due(&self, weekday: Weekday, minute_of_day: u32, window_minutes: u32) -> Vec<&Workflow> {
        self.workflows
            .iter()
            .filter(|w| w.enabled)
            .filter(|w| match &w.trigger {
                Trigger::Schedule(s) => is_due(s, weekday, minute_of_day, window_minutes),
                _ => false,
            })
            .collect()
    }

    /// The enabled scheduled workflow that fires next and the minutes until it does (ties
    /// go to the lower id), for the shell to schedule a wake-up.
    pub fn next_scheduled(&self, weekday: Weekday, minute_of_day: u32) -> Option<(&Workflow, u32)> {
        self.workflows
            .iter()
            .filter(|w| w.enabled)
            .filter_map(|w| match &w.trigger {
                Trigger::Schedule(s) => Some((w, minutes_until_next(s, weekday, minute_of_day))),
                _ => None,
            })
            .filter(|(_, m)| *m != u32::MAX)
            .min_by(|(a, ma), (b, mb)| ma.cmp(mb).then_with(|| a.id.cmp(&b.id)))
    }

    /// Serializes to `{"schema_version":1,"workflows":[...]}`.
    pub fn to_json(&self) -> Result<String, SkillError> {
        Ok(serde_json::to_string(&SnapshotRef {
            schema_version: WORKFLOWS_SCHEMA_VERSION,
            workflows: &self.workflows,
        })?)
    }

    /// Parses a snapshot; see [`WorkflowStore::from_json_with_issues`].
    pub fn from_json(json: &str) -> Result<WorkflowStore, SkillError> {
        Self::from_json_with_issues(json).map(|(s, _)| s)
    }

    /// Parses a snapshot, also returning one line per skipped workflow. Same rules as
    /// `SkillStore::from_json_with_issues`: structural damage and newer schemas are errors,
    /// a damaged or duplicate entry is skipped and reported.
    pub fn from_json_with_issues(json: &str) -> Result<(WorkflowStore, Vec<String>), SkillError> {
        let value: Value = serde_json::from_str(json)?;
        let obj = value
            .as_object()
            .ok_or_else(|| SkillError::Invalid("top level is not an object".to_owned()))?;
        check_version(obj, WORKFLOWS_SCHEMA_VERSION)?;
        let mut store = WorkflowStore::new();
        let mut issues = Vec::new();
        let items = match obj.get("workflows") {
            None | Some(Value::Null) => return Ok((store, issues)),
            Some(Value::Array(items)) => items,
            Some(_) => return Err(SkillError::Invalid("workflows is not an array".to_owned())),
        };
        for (i, item) in items.iter().enumerate() {
            if store.len() >= MAX_WORKFLOWS {
                issues.push(format!(
                    "workflows[{i}..]: skipped, the store holds at most {MAX_WORKFLOWS} workflows"
                ));
                break;
            }
            match serde_json::from_value::<Workflow>(item.clone()) {
                Ok(w) => {
                    if let Err(e) = store.add(w) {
                        issues.push(format!("workflows[{i}]: skipped: {e}"));
                    }
                }
                Err(e) => issues.push(format!("workflows[{i}]: skipped: {e}")),
            }
        }
        Ok((store, issues))
    }
}
