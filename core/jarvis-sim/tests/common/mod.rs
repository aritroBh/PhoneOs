//! Shared helpers for the end-to-end tests.

#![allow(dead_code)]

use jarvis_memory::MemoryStore;
use jarvis_proto::RiskCategory;
use jarvis_sim::{run_with, Human, ObserveMode, RunReport, RunSetup, ScriptedModel, SimPhone};

/// Both observation modes, for tests that must hold in each.
pub const MODES: [ObserveMode; 2] = [ObserveMode::Structured, ObserveMode::RunnerText];

/// A setup in `mode` where every built-in app is already approved, as on a phone after the
/// first session: these tests are about the apps' own confirmations. First-use approval is
/// exercised from a fresh store by [`fresh_setup`] (tests/policy.rs) and by the built-in
/// eval.
pub fn setup(mode: ObserveMode) -> RunSetup {
    RunSetup {
        mode,
        approved_apps: all_bundle_ids(),
        ..RunSetup::default()
    }
}

/// The default setup: nothing approved yet.
pub fn fresh_setup(mode: ObserveMode) -> RunSetup {
    RunSetup {
        mode,
        ..RunSetup::default()
    }
}

/// Bundle ids of every built-in app.
pub fn all_bundle_ids() -> Vec<String> {
    jarvis_sim::builtin::builtin_apps()
        .unwrap()
        .into_iter()
        .map(|a| a.bundle_id)
        .collect()
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

/// `typed` on a typed tree, `text` in runner-text mode. A screen read from runner text is
/// unverified (fifth review): every state-changing runner action on it asks (category
/// `Unverified`, the summary starting with the warning), so the owner sees more questions
/// there than on the typed path.
pub fn per_mode<T>(mode: ObserveMode, typed: T, text: T) -> T {
    match mode {
        ObserveMode::Structured => typed,
        ObserveMode::RunnerText => text,
    }
}

/// In runner-text mode, every `Unverified` question starts with the warning.
pub fn assert_text_warnings(r: &RunReport, mode: ObserveMode) {
    for c in &r.confirmations {
        let warned = c
            .request
            .summary
            .starts_with(jarvis_safety::UNVERIFIED_SCREEN_WARNING);
        match mode {
            ObserveMode::RunnerText if c.request.category != RiskCategory::FirstUse => {
                assert!(warned, "{}", c.request.summary)
            }
            _ => assert!(!warned, "{}", c.request.summary),
        }
    }
}
