//! Shared helpers for the end-to-end tests.

#![allow(dead_code)]

use jarvis_memory::MemoryStore;
use jarvis_proto::RiskCategory;
use jarvis_sim::{run_with, Human, ObserveMode, RunReport, RunSetup, ScriptedModel, SimPhone};

/// Both observation modes, for tests that must hold in each.
pub const MODES: [ObserveMode; 2] = [ObserveMode::Structured, ObserveMode::RunnerText];

pub fn setup(mode: ObserveMode) -> RunSetup {
    RunSetup {
        mode,
        ..RunSetup::default()
    }
}

/// `{"action":"tap","element":"@<spec>"}`
pub fn tap(spec: &str) -> String {
    format!(r#"{{"thought":"t","action":"tap","element":"@{spec}"}}"#)
}

/// `{"action":"type",...}` into the element `@<spec>`.
pub fn type_into(spec: &str, text: &str) -> String {
    format!(r#"{{"thought":"t","action":"type","element":"@{spec}","text":"{text}"}}"#)
}

pub fn done(summary: &str) -> String {
    format!(r#"{{"thought":"t","action":"done","summary":"{summary}"}}"#)
}

pub fn fail(reason: &str) -> String {
    format!(r#"{{"thought":"t","action":"fail","reason":"{reason}"}}"#)
}

pub fn scroll_down() -> String {
    r#"{"thought":"t","action":"scroll","direction":"down"}"#.to_owned()
}

pub fn open_app(app: &str) -> String {
    format!(r#"{{"thought":"t","action":"open_app","app":"{app}"}}"#)
}

/// Runs `task` on a fresh phone with `app` in front.
pub fn go(
    mode: ObserveMode,
    app: &str,
    task: &str,
    outputs: &[String],
    human: &mut Human,
) -> (RunReport, ScriptedModel) {
    let mut model = ScriptedModel::new(outputs.iter().cloned());
    let report = go_with(
        &setup(mode),
        app,
        task,
        &mut model,
        human,
        MemoryStore::new(),
    );
    (report, model)
}

/// Like [`go`] with explicit setup, model and memory.
pub fn go_with(
    setup: &RunSetup,
    app: &str,
    task: &str,
    model: &mut ScriptedModel,
    human: &mut Human,
    memory: MemoryStore,
) -> RunReport {
    let mut phone = setup.new_phone(app).unwrap();
    run_phone(setup, &mut phone, task, model, human, memory)
}

/// Runs `task` on an existing phone.
pub fn run_phone(
    setup: &RunSetup,
    phone: &mut SimPhone,
    task: &str,
    model: &mut ScriptedModel,
    human: &mut Human,
    memory: MemoryStore,
) -> RunReport {
    let session = setup.session(phone, "t", task, memory);
    run_with(session, phone, model, human, &setup.options)
}

pub fn state<'a>(r: &'a RunReport, app: &str) -> &'a serde_json::Value {
    r.final_state.states.get(app).unwrap()
}

pub fn categories(r: &RunReport) -> Vec<RiskCategory> {
    r.confirm_categories()
}
