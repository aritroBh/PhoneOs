//! Shared setup for a run: phone, session configuration, one-call task execution.

use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_proto::{ModelProfile, Policy, SessionConfig, TaskSpec};

use crate::driver::{run_with, RunOptions, RunReport};
use crate::error::SimError;
use crate::human::Human;
use crate::model::Model;
use crate::phone::{ClockCosts, ObserveMode, SimPhone};

/// Knobs shared by `run` and `eval`.
#[derive(Debug, Clone)]
pub struct RunSetup {
    /// How the screen is reported.
    pub mode: ObserveMode,
    /// The model's context window in tokens.
    pub context_tokens: u32,
    /// Whether the model supports schema-constrained output.
    pub grammar: bool,
    /// Overrides the policy's step budget.
    pub max_steps: Option<u32>,
    /// Overrides the policy's time budget (simulated milliseconds).
    pub max_duration_ms: Option<u64>,
    /// Simulated cost of each IO.
    pub costs: ClockCosts,
    /// Driver limits.
    pub options: RunOptions,
}

impl Default for RunSetup {
    fn default() -> Self {
        Self {
            mode: ObserveMode::Structured,
            context_tokens: ModelProfile::default().context_tokens,
            grammar: false,
            max_steps: None,
            max_duration_ms: None,
            costs: ClockCosts::default(),
            options: RunOptions::default(),
        }
    }
}

impl RunSetup {
    /// The session configuration for `phone`: default policy, no tools, no vision.
    pub fn session_config(&self, phone: &SimPhone) -> SessionConfig {
        let mut policy = Policy::default();
        if let Some(n) = self.max_steps {
            policy.max_steps = n;
        }
        if let Some(ms) = self.max_duration_ms {
            policy.max_duration_ms = ms;
        }
        SessionConfig {
            policy,
            tools: Vec::new(),
            model: ModelProfile {
                context_tokens: self.context_tokens,
                supports_vision: false,
                supports_grammar: self.grammar,
            },
            known_apps: phone.known_apps(),
        }
    }

    /// A fresh built-in phone with `start_app` in front (`home` for the home screen).
    pub fn new_phone(&self, start_app: &str) -> Result<SimPhone, SimError> {
        let mut phone = SimPhone::new()?;
        phone.set_mode(self.mode);
        phone.set_costs(self.costs);
        phone.launch(start_app)?;
        Ok(phone)
    }

    /// A session for `task` with `memory`.
    pub fn session(&self, phone: &SimPhone, id: &str, task: &str, memory: MemoryStore) -> Session {
        Session::new(self.session_config(phone), TaskSpec::new(id, task), memory)
    }
}

/// Runs one task on a fresh phone and returns the report.
pub fn run_task(
    setup: &RunSetup,
    start_app: &str,
    id: &str,
    task: &str,
    model: &mut dyn Model,
    human: &mut Human,
    memory: MemoryStore,
) -> Result<RunReport, SimError> {
    let mut phone = setup.new_phone(start_app)?;
    let session = setup.session(&phone, id, task, memory);
    Ok(run_with(session, &mut phone, model, human, &setup.options))
}
