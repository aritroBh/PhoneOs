//! Shared setup for a run: phone, session configuration, one-call task execution.

use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_proto::{AppRule, ModelProfile, Policy, PolicyMode, SessionConfig, TaskSpec};

use crate::driver::{run_with, RunOptions, RunReport};
use crate::error::SimError;
use crate::faults::Faults;
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
    /// Confirm every state-changing action (`PolicyMode::StepByStep`).
    pub step_by_step: bool,
    /// Ask before the first action inside each app (on by default, like `Policy`).
    pub first_use_approval: bool,
    /// Per-app trust levels added to the policy.
    pub app_rules: Vec<AppRule>,
    /// Bundle ids approved in memory before the run (no first-use prompt for them).
    pub approved_apps: Vec<String>,
    /// Overrides the policy's limit on waiting for an unlock (simulated milliseconds).
    pub max_locked_wait_ms: Option<u64>,
    /// Faults injected into the phone (locks, failing observations, dropped characters).
    pub faults: Faults,
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
            step_by_step: false,
            first_use_approval: Policy::default().first_use_approval,
            app_rules: Vec::new(),
            approved_apps: Vec::new(),
            max_locked_wait_ms: None,
            faults: Faults::default(),
        }
    }
}

impl RunSetup {
    /// The session configuration for `phone`: the default policy with this setup's mode,
    /// first-use flag, app rules and budgets; no tools, no vision.
    pub fn session_config(&self, phone: &SimPhone) -> SessionConfig {
        let mut policy = Policy::default();
        if self.step_by_step {
            policy.mode = PolicyMode::StepByStep;
        }
        policy.first_use_approval = self.first_use_approval;
        policy.app_rules.extend(self.app_rules.iter().cloned());
        if let Some(n) = self.max_steps {
            policy.max_steps = n;
        }
        if let Some(ms) = self.max_duration_ms {
            policy.max_duration_ms = ms;
        }
        if let Some(ms) = self.max_locked_wait_ms {
            policy.max_locked_wait_ms = ms;
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
            agent_bundle_id: None,
            drive_own_app: false,
            skill: None,
        }
    }

    /// A fresh built-in phone with `start_app` in front (`home` for the home screen).
    pub fn new_phone(&self, start_app: &str) -> Result<SimPhone, SimError> {
        let mut phone = SimPhone::new()?;
        phone.set_mode(self.mode);
        phone.set_costs(self.costs);
        phone.set_faults(self.faults.clone());
        phone.launch(start_app)?;
        Ok(phone)
    }

    /// A session for `task` with `memory`, plus this setup's pre-approved apps.
    pub fn session(
        &self,
        phone: &SimPhone,
        id: &str,
        task: &str,
        mut memory: MemoryStore,
    ) -> Session {
        for app in &self.approved_apps {
            memory.approve_app(app);
        }
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
