//! `JarvisSkills`: the skill store and matcher for Swift.
//!
//! One `jarvis_skills::SkillStore` behind a `Mutex` (calls from any thread are serialized).
//! The shell persists the bytes from `toJson()` and passes them back to the constructor.
//! Every method runs inside [`guarded`], so a panic is a thrown `JarvisError.internal`; a
//! poisoned mutex (a panic that escaped while the lock was held) is recovered, because the
//! store only changes through whole-value operations that validate first.
//!
//! Errors from the store (validation, unknown id, corrupt snapshot) are
//! `JarvisError.invalidSkill`.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};

use jarvis_memory::MemoryStore;
use jarvis_skills::{Ambiguity, SkillError, SkillStore, Upsert};

use crate::error::JarvisError;
use crate::guard::guarded;
use crate::types::{Skill, SkillContext, SkillMatch, SkillMatchResult};

struct Inner {
    store: SkillStore,
    issues: Vec<String>,
}

/// The owner's skills. Swift: `JarvisSkills`.
#[derive(uniffi::Object)]
pub struct JarvisSkills {
    inner: Mutex<Inner>,
}

fn skill_error(e: SkillError) -> JarvisError {
    JarvisError::InvalidSkill {
        message: e.to_string(),
    }
}

fn to_btree(args: HashMap<String, String>) -> BTreeMap<String, String> {
    args.into_iter().collect()
}

impl JarvisSkills {
    /// Loads a store from snapshot JSON (shared by the FFI constructor and Rust tests).
    pub fn from_json(json: &str) -> Result<Self, JarvisError> {
        let (store, issues) = if json.trim().is_empty() {
            (SkillStore::new(), Vec::new())
        } else {
            SkillStore::from_json_with_issues(json).map_err(skill_error)?
        };
        Ok(JarvisSkills {
            inner: Mutex::new(Inner { store, issues }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => {
                self.inner.clear_poison();
                poison.into_inner()
            }
        }
    }

    fn with<T>(
        &self,
        what: &str,
        f: impl FnOnce(&mut Inner) -> Result<T, JarvisError>,
    ) -> Result<T, JarvisError> {
        guarded(what, || {
            let mut inner = self.lock();
            f(&mut inner)
        })
    }
}

#[uniffi::export]
impl JarvisSkills {
    /// Loads the store from the bytes `toJson()` returned last time (empty for a new
    /// store). A corrupt or newer snapshot throws `JarvisError.invalidSkill`; individual
    /// damaged skills are skipped and listed by `loadIssues()`.
    #[uniffi::constructor]
    pub fn new(json: String) -> Result<Arc<Self>, JarvisError> {
        guarded("JarvisSkills.init", || Self::from_json(&json).map(Arc::new))
    }

    /// One line per skill skipped when the snapshot was loaded.
    pub fn load_issues(&self) -> Result<Vec<String>, JarvisError> {
        self.with("load_issues", |i| Ok(i.issues.clone()))
    }

    /// The versioned snapshot to persist.
    pub fn to_json(&self) -> Result<String, JarvisError> {
        self.with("to_json", |i| i.store.to_json().map_err(skill_error))
    }

    /// All skills, ordered by id.
    pub fn list(&self) -> Result<Vec<Skill>, JarvisError> {
        self.with("list", |i| {
            Ok(i.store.list().iter().cloned().map(Into::into).collect())
        })
    }

    /// The skill with this id.
    pub fn get(&self, id: String) -> Result<Option<Skill>, JarvisError> {
        self.with("get", |i| Ok(i.store.get(&id).cloned().map(Into::into)))
    }

    /// Adds or replaces a skill (an update keeps the stored `createdMs`, `runs` and
    /// `lastRunMs`). Returns true when it was new. Invalid skills throw
    /// `JarvisError.invalidSkill` with the reason.
    pub fn upsert(&self, skill: Skill) -> Result<bool, JarvisError> {
        self.with("upsert", |i| {
            i.store
                .upsert(skill.into())
                .map(|u| u == Upsert::Inserted)
                .map_err(skill_error)
        })
    }

    /// Removes a skill; false when there was none.
    pub fn remove(&self, id: String) -> Result<bool, JarvisError> {
        self.with("remove", |i| Ok(i.store.remove(&id).is_some()))
    }

    /// Records a run of the skill at `nowMs`.
    pub fn record_run(&self, id: String, now_ms: u64) -> Result<(), JarvisError> {
        self.with("record_run", |i| {
            i.store.record_run(&id, now_ms).map_err(skill_error)
        })
    }

    /// The enabled skills matching the owner's request, best first. When `ambiguous`, ask
    /// the owner which one they meant rather than picking.
    pub fn match_task(&self, text: String) -> Result<SkillMatchResult, JarvisError> {
        self.with("match_task", |i| {
            let matches = i.store.match_task(&text);
            Ok(SkillMatchResult {
                ambiguous: matches.is_ambiguous(),
                matches: matches.into_iter().map(Into::into).collect(),
            })
        })
    }

    /// The `SessionConfig.skill` value for running skill `skillId` with `args` (undeclared
    /// arguments are dropped).
    pub fn context(
        &self,
        skill_id: String,
        args: HashMap<String, String>,
    ) -> Result<SkillContext, JarvisError> {
        self.with("context", |i| {
            let skill = i
                .store
                .get(&skill_id)
                .ok_or_else(|| skill_error(SkillError::NotFound(skill_id.clone())))?;
            Ok(skill.context(to_btree(args)).into())
        })
    }

    /// The `SessionConfig.skill` value for a match from `matchTask`.
    pub fn context_for_match(&self, m: SkillMatch) -> Result<SkillContext, JarvisError> {
        self.context(m.skill_id, m.args)
    }

    /// A task text for running the skill with `args` without the owner's words (e.g. from
    /// a workflow): its first trigger the arguments fill, else its name.
    pub fn task_text(
        &self,
        skill_id: String,
        args: HashMap<String, String>,
    ) -> Result<String, JarvisError> {
        self.with("task_text", |i| {
            let skill = i
                .store
                .get(&skill_id)
                .ok_or_else(|| skill_error(SkillError::NotFound(skill_id.clone())))?;
            Ok(skill.task_text(&to_btree(args)))
        })
    }

    /// Turns the learned flow `flowId` from the memory bytes into a `learned` skill named
    /// `name`; returns the new skill's id.
    pub fn promote_flow(
        &self,
        memory_json: String,
        flow_id: String,
        name: String,
        now_ms: u64,
    ) -> Result<String, JarvisError> {
        self.with("promote_flow", |i| {
            let memory =
                MemoryStore::from_json(&memory_json).map_err(|e| JarvisError::InvalidMemory {
                    message: e.to_string(),
                })?;
            let flow = memory.flows.get(&flow_id).ok_or_else(|| {
                skill_error(SkillError::Flow(format!("no learned flow {flow_id:?}")))
            })?;
            i.store
                .promote_flow(flow, &name, now_ms)
                .map_err(skill_error)
        })
    }
}
