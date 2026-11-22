//! The flow store and the top-level, versioned `MemoryStore`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::flow::{flow_id, Bindings, Flow};
use crate::intent::{intent_key, is_slotted, match_slotted};
use crate::log::{LogEntry, TaskLog};
use crate::profile::{Profile, ProfileField};

/// Current schema version written by [`MemoryStore::to_json`].
pub const SCHEMA_VERSION: u32 = 1;
/// Default cap on stored flows.
pub const MAX_FLOWS: usize = 200;

/// Errors from loading or saving memory.
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    /// The bytes are not valid JSON (or serialization failed).
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// The data was written by a newer, incompatible version.
    #[error("memory schema version {found} is newer than supported version {supported}")]
    NewerSchema {
        /// Version in the data.
        found: u64,
        /// Highest version this build reads.
        supported: u32,
    },
    /// The JSON is valid but not shaped like a memory store.
    #[error("invalid memory data: {0}")]
    Invalid(String),
}

/// What [`FlowStore::upsert`] did with a flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    /// No flow with this key and app existed; the flow was added.
    Inserted,
    /// An existing flow was replaced.
    Replaced,
    /// The existing flow was better (shorter or equal, still live); the new one was dropped.
    Kept,
}

/// Learned flows, sorted by id, bounded in number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowStore {
    #[serde(default)]
    flows: Vec<Flow>,
    #[serde(skip, default = "default_cap")]
    max_flows: usize,
}

fn default_cap() -> usize {
    MAX_FLOWS
}

impl Default for FlowStore {
    fn default() -> Self {
        Self {
            flows: Vec::new(),
            max_flows: MAX_FLOWS,
        }
    }
}

impl FlowStore {
    /// An empty store with the default cap.
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty store keeping at most `max_flows` (at least 1) flows.
    pub fn with_capacity(max_flows: usize) -> Self {
        Self {
            flows: Vec::new(),
            max_flows: max_flows.max(1),
        }
    }

    /// Number of stored flows, retired ones included.
    pub fn len(&self) -> usize {
        self.flows.len()
    }

    /// True when no flows are stored.
    pub fn is_empty(&self) -> bool {
        self.flows.is_empty()
    }

    /// Stored flows ordered by id.
    pub fn flows(&self) -> &[Flow] {
        &self.flows
    }

    /// The flow with this id.
    pub fn get(&self, id: &str) -> Option<&Flow> {
        self.flows.iter().find(|f| f.id == id)
    }

    /// Finds the live flow for a task: an exact intent-key match, else a slotted match with
    /// bindings. Retired flows are ignored. Several candidates (for example the same task
    /// learned in two apps) are ambiguous and yield `None`; the model decides then.
    pub fn find(&self, task_text: &str) -> Option<(Flow, Bindings)> {
        let key = intent_key(task_text);
        if key.is_empty() {
            return None;
        }
        let exact: Vec<&Flow> = self
            .flows
            .iter()
            .filter(|f| !f.retired && f.intent_key == key)
            .collect();
        match exact.as_slice() {
            [one] => return Some(((*one).clone(), Bindings::new())),
            [] => {}
            _ => return None,
        }
        let mut found: Vec<(&Flow, Bindings)> = Vec::new();
        for f in self
            .flows
            .iter()
            .filter(|f| !f.retired && is_slotted(&f.intent_key))
        {
            if let Some(b) = match_slotted(&f.intent_key, task_text) {
                found.push((f, b));
            }
        }
        match found.pop() {
            Some((f, b)) if found.is_empty() => Some((f.clone(), b)),
            _ => None,
        }
    }

    /// Adds or replaces a flow. With the same intent key and app, the new flow replaces the old
    /// one only if it is not longer or the old one is retired. The id is recomputed. Over the
    /// cap, the least recently used flow (never the one just stored) is evicted.
    pub fn upsert(&mut self, mut flow: Flow) -> UpsertOutcome {
        flow.id = flow_id(&flow.intent_key, &flow.app_bundle_id);
        let outcome = match self.flows.iter().position(|f| f.id == flow.id) {
            Some(pos) => {
                let better = self
                    .flows
                    .get(pos)
                    .is_none_or(|old| old.retired || flow.steps.len() <= old.steps.len());
                if !better {
                    return UpsertOutcome::Kept;
                }
                if let Some(slot) = self.flows.get_mut(pos) {
                    *slot = flow.clone();
                }
                UpsertOutcome::Replaced
            }
            None => {
                self.flows.push(flow.clone());
                UpsertOutcome::Inserted
            }
        };
        self.flows.sort_by(|a, b| a.id.cmp(&b.id));
        self.evict(Some(&flow.id));
        outcome
    }

    fn evict(&mut self, protect: Option<&str>) {
        while self.flows.len() > self.max_flows {
            let victim = self
                .flows
                .iter()
                .enumerate()
                .filter(|(_, f)| Some(f.id.as_str()) != protect)
                .min_by(|(_, a), (_, b)| {
                    a.last_used_ms
                        .cmp(&b.last_used_ms)
                        .then_with(|| a.id.cmp(&b.id))
                })
                .map(|(i, _)| i);
            match victim {
                Some(i) => {
                    self.flows.remove(i);
                }
                None => break,
            }
        }
    }

    /// Records the result of a replay. Updates counters and `last_used_ms`, and retires the
    /// flow when it has at least 3 uses and more failures than successes. Returns whether the
    /// flow is now retired, or `None` for an unknown id.
    pub fn record_result(&mut self, flow_id: &str, success: bool, now_ms: u64) -> Option<bool> {
        let flow = self.flows.iter_mut().find(|f| f.id == flow_id)?;
        if success {
            flow.successes = flow.successes.saturating_add(1);
        } else {
            flow.failures = flow.failures.saturating_add(1);
        }
        flow.last_used_ms = now_ms;
        if flow.should_retire() {
            flow.retired = true;
        }
        Some(flow.retired)
    }

    fn from_flows(flows: Vec<Flow>) -> Self {
        let mut store = Self::default();
        let mut flows = flows;
        flows.sort_by(|a, b| a.id.cmp(&b.id).then(b.last_used_ms.cmp(&a.last_used_ms)));
        flows.dedup_by(|b, a| a.id == b.id);
        store.flows = flows;
        store.evict(None);
        store
    }
}

/// Everything the agent remembers, persisted by the shell as JSON bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemoryStore {
    /// Schema version of the data.
    pub schema_version: u32,
    /// Learned flows.
    pub flows: FlowStore,
    /// User profile.
    pub profile: Profile,
    /// Task log.
    pub log: TaskLog,
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            flows: FlowStore::default(),
            profile: Profile::default(),
            log: TaskLog::default(),
        }
    }
}

impl MemoryStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Serializes to JSON (always at [`SCHEMA_VERSION`]).
    pub fn to_json(&self) -> Result<String, MemoryError> {
        let mut view = self.clone();
        view.schema_version = SCHEMA_VERSION;
        Ok(serde_json::to_string(&view)?)
    }

    /// Parses JSON written by this or an older/minor-newer version.
    ///
    /// Unknown fields are ignored; missing sections become empty. Individual flows, profile
    /// fields or log entries that cannot be understood (for example a step type from a newer
    /// minor version) are skipped rather than failing the load. A `schema_version` above
    /// [`SCHEMA_VERSION`] is rejected with [`MemoryError::NewerSchema`].
    pub fn from_json(json: &str) -> Result<MemoryStore, MemoryError> {
        let value: Value = serde_json::from_str(json)?;
        let obj = value
            .as_object()
            .ok_or_else(|| MemoryError::Invalid("top level is not an object".to_owned()))?;
        match obj.get("schema_version") {
            None | Some(Value::Null) => {}
            Some(v) => {
                let found = v.as_u64().ok_or_else(|| {
                    MemoryError::Invalid("schema_version is not an unsigned integer".to_owned())
                })?;
                if found > u64::from(SCHEMA_VERSION) {
                    return Err(MemoryError::NewerSchema {
                        found,
                        supported: SCHEMA_VERSION,
                    });
                }
            }
        }
        let mut store = MemoryStore::default();
        if let Some(section) = section(obj, "flows")? {
            let flows = lenient_list::<Flow>(section.get("flows"), "flows.flows")?;
            store.flows = FlowStore::from_flows(flows);
        }
        if let Some(section) = section(obj, "profile")? {
            if let Some(fields) = section.get("fields") {
                let map = fields.as_object().ok_or_else(|| {
                    MemoryError::Invalid("profile.fields is not an object".to_owned())
                })?;
                for (k, v) in map {
                    if let Ok(field) = serde_json::from_value::<ProfileField>(v.clone()) {
                        store.profile.fields.insert(k.clone(), field);
                    }
                }
            }
        }
        if let Some(section) = section(obj, "log")? {
            let entries = lenient_list::<LogEntry>(section.get("entries"), "log.entries")?;
            store.log = TaskLog::from_entries(entries);
        }
        Ok(store)
    }
}

/// The object under `name`, `None` when absent or null.
fn section<'a>(
    obj: &'a serde_json::Map<String, Value>,
    name: &str,
) -> Result<Option<&'a serde_json::Map<String, Value>>, MemoryError> {
    match obj.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(m)) => Ok(Some(m)),
        Some(_) => Err(MemoryError::Invalid(format!("{name} is not an object"))),
    }
}

/// Deserializes each element of an optional array, skipping elements that do not fit.
fn lenient_list<T: serde::de::DeserializeOwned>(
    value: Option<&Value>,
    what: &str,
) -> Result<Vec<T>, MemoryError> {
    match value {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => Ok(items
            .iter()
            .filter_map(|v| serde_json::from_value::<T>(v.clone()).ok())
            .collect()),
        Some(_) => Err(MemoryError::Invalid(format!("{what} is not an array"))),
    }
}
