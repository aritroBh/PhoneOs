//! The measured stages and what each one runs.
//!
//! Every stage calls the same public functions the session and the bridge call, on inputs
//! prepared outside the timer. Stages that consume their input (a session, an event) get a
//! fresh clone from an untimed `setup`.

use std::hint::black_box;
use std::time::Instant;

use jarvis_agent::{system_prompt, Session, ACTION_MAX_TOKENS};
use jarvis_memory::MemoryStore;
use jarvis_observe::{
    compact_view, estimate_tokens, parse_runner_text, render, screen_digest, CompactView,
};
use jarvis_proto::{
    Action, CompactScreen, Direction, Effect, Element, Event, ModelOutput, ModelProfile,
    Observation, Policy, PolicyMode, RiskCategory, Role, RunnerResult, Screen, SessionConfig,
    TaskSpec, TimedEvent, TrustLevel,
};
use jarvis_safety::{
    approval_context, classify, evaluate_from, evaluate_owned, gate_on, redact_screen,
    sanitize_screen_text, summary_with, takes_taps, tap_spot, ScreenSource, ScreenTree,
};

use super::gen::perturb;
use super::stats::{measure, Plan, Stats};
use crate::runner_text::to_runner_text;

/// Settings shared by every stage.
#[derive(Debug, Clone)]
pub struct Cfg {
    /// Iteration plan.
    pub plan: Plan,
    /// The model's context window in tokens (decides how much of the screen the prompt keeps).
    pub context_tokens: u32,
    /// A sweep whose estimated time is above this is sampled and extrapolated.
    pub sweep_cap_s: f64,
    /// Run only stages whose name starts with one of these (all when empty).
    pub only: Vec<String>,
}

impl Cfg {
    fn wants(&self, stage: &str) -> bool {
        self.only.is_empty() || self.only.iter().any(|p| stage.starts_with(p.as_str()))
    }
}

/// One measured stage of one case.
#[derive(Debug, Clone)]
pub struct Row {
    /// Case id.
    pub case: String,
    /// Stage name.
    pub stage: String,
    /// The timing; `None` when the stage could not run.
    pub stats: Option<Stats>,
    /// Anything to know about the number (sampled, unexpected effect, skipped).
    pub note: Option<String>,
}

/// What a case's screen looks like to the core.
#[derive(Debug, Clone, Default)]
pub struct Counts {
    /// Raw nodes in the tree.
    pub nodes: usize,
    /// Elements in the compact screen.
    pub elements: usize,
    /// Drawn raw nodes (what a tap point is checked against).
    pub raw_nodes: usize,
    /// Listed elements that take taps (what the sweep visits).
    pub controls: usize,
    /// Bytes of the runner-text form of the tree.
    pub text_bytes: usize,
}

/// Size and make-up of the prompt for one case.
#[derive(Debug, Clone, Default)]
pub struct PromptInfo {
    /// Bytes of the whole screen rendered without any budget.
    pub full_bytes: usize,
    /// Element lines in it.
    pub full_elements: usize,
    /// Bytes of element labels and values (the text the screen itself supplies).
    pub text_bytes: usize,
    /// Bytes of the per-element syntax: index, role name, quotes, flags, newlines.
    pub syntax_bytes: usize,
    /// Per role: element lines and bytes, most bytes first.
    pub by_role: Vec<(String, usize, usize)>,
    /// Token budget of the screen in a prompt for the configured context.
    pub budget_tokens: u32,
    /// Bytes of the screen rendered within that budget.
    pub budget_bytes: usize,
    /// Element lines kept within that budget.
    pub budget_elements: usize,
    /// What a real session sends at the first step: the system prompt's bytes.
    pub system_bytes: usize,
    /// The user prompt's bytes (task, screen and the rest).
    pub user_bytes: usize,
    /// Element lines in the user prompt.
    pub user_elements: usize,
}

/// Everything measured for one case.
#[derive(Debug, Default)]
pub struct CaseResult {
    /// Timed stages.
    pub rows: Vec<Row>,
    /// Sizes.
    pub counts: Counts,
    /// Prompt make-up.
    pub prompt: PromptInfo,
}

const TASK: &str = "Open the second item and read its details";

// ---- sessions ------------------------------------------------------------------------------

/// The session configuration the step stages use: first-use approval off (it would ask on
/// the first step of every case), budgets out of the way.
pub fn session_config(context_tokens: u32, step_by_step: bool) -> SessionConfig {
    let mut policy = Policy {
        first_use_approval: false,
        max_steps: 1_000_000,
        max_duration_ms: u64::MAX / 4,
        max_consecutive_failures: 1_000_000,
        ..Policy::default()
    };
    if step_by_step {
        policy.mode = PolicyMode::StepByStep;
    }
    SessionConfig {
        policy,
        model: ModelProfile {
            context_tokens,
            supports_vision: false,
            supports_grammar: false,
        },
        ..SessionConfig::default()
    }
}

/// A session that has not been started.
pub fn new_session(context_tokens: u32, step_by_step: bool) -> Session {
    Session::new(
        session_config(context_tokens, step_by_step),
        TaskSpec::new("bench", TASK),
        MemoryStore::new(),
    )
}

/// An `Observed` event with a copy of `screen`.
pub fn observed(screen: &Screen) -> Event {
    Event::Observed(Observation::new(screen.clone()))
}

/// A tap on `element` as the model's reply.
pub fn tap_reply(element: u32) -> Event {
    let text = serde_json::to_string(&Action::Tap { element }).unwrap_or_default();
    Event::ModelOutput(ModelOutput { text })
}

fn effect_name(e: &Effect) -> &'static str {
    e.name()
}

// ---- target choice -------------------------------------------------------------------------

/// The control the tap stages aim at: of the listed controls that take taps, those of the
/// first kind present (cells, then buttons, then links; any control when there are none),
/// the one in the middle of the reading order. A bigger screen of the same make-up gets the
/// same kind of target, which matters: reading a cell takes in the whole screen, a button
/// only its own label.
pub fn pick_target(screen: &CompactScreen) -> Option<&Element> {
    let usable = |e: &&Element| takes_taps(&e.role) && e.enabled;
    let of_role = |role: Role| -> Vec<&Element> {
        screen
            .elements
            .iter()
            .filter(usable)
            .filter(|e| e.role == role)
            .collect()
    };
    let pool = [Role::Cell, Role::Button, Role::Link]
        .into_iter()
        .map(of_role)
        .find(|p| !p.is_empty())
        .unwrap_or_else(|| screen.elements.iter().filter(usable).collect());
    pool.get(pool.len() / 2).copied()
}

/// Indices of the safe-looking controls a step driver may tap, in screen order.
pub fn tap_targets(screen: &CompactScreen) -> Vec<u32> {
    screen
        .elements
        .iter()
        .filter(|e| takes_taps(&e.role) && e.enabled && !e.label.is_empty())
        .map(|e| e.index)
        .collect()
}

// ---- prompt --------------------------------------------------------------------------------

/// The prompt-safe copy the session shows the model: the same two public calls its private
/// `prompt_safe` makes (redaction, then sanitising of every text), without the profile masks
/// (the benchmark's memory has no profile).
pub fn safe_copy(screen: &CompactScreen) -> CompactScreen {
    let mut safe = redact_screen(screen);
    for e in &mut safe.elements {
        e.label = sanitize_screen_text(&e.label);
        e.value = e.value.as_deref().map(sanitize_screen_text);
        e.placeholder = e.placeholder.as_deref().map(sanitize_screen_text);
    }
    safe
}

/// Tokens a prompt for a model of `context_tokens` leaves for the screen: the window less the
/// reply, the system prompt and 300 tokens for the task, history and notes.
pub fn screen_budget_tokens(context_tokens: u32) -> u32 {
    context_tokens
        .saturating_sub(ACTION_MAX_TOKENS)
        .saturating_sub(estimate_tokens(&system_prompt()))
        .saturating_sub(300)
}

fn count_lines(text: &str) -> usize {
    text.lines().filter(|l| l.starts_with('[')).count()
}

fn prompt_info(
    safe: &CompactScreen,
    context_tokens: u32,
    request: Option<(usize, usize, usize)>,
) -> PromptInfo {
    let full = render(safe, u32::MAX);
    let mut by_role: Vec<(String, usize, usize)> = Vec::new();
    for line in full.lines().filter(|l| l.starts_with('[')) {
        let role = line
            .split_once("] ")
            .map(|(_, r)| r.split([' ', '\t']).next().unwrap_or(""))
            .unwrap_or("")
            .to_owned();
        let bytes = line.len() + 1;
        match by_role.iter_mut().find(|(r, _, _)| *r == role) {
            Some(slot) => {
                slot.1 += 1;
                slot.2 += bytes;
            }
            None => by_role.push((role, 1, bytes)),
        }
    }
    by_role.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    let text_bytes: usize = safe
        .elements
        .iter()
        .map(|e| e.label.len() + e.value.as_deref().map_or(0, str::len))
        .sum();
    let element_bytes: usize = by_role.iter().map(|(_, _, b)| b).sum();
    let budget_tokens = screen_budget_tokens(context_tokens);
    let budgeted = render(safe, budget_tokens);
    let (system_bytes, user_bytes, user_elements) = request.unwrap_or_default();
    PromptInfo {
        full_bytes: full.len(),
        full_elements: count_lines(&full),
        text_bytes,
        syntax_bytes: element_bytes.saturating_sub(text_bytes),
        by_role,
        budget_tokens,
        budget_bytes: budgeted.len(),
        budget_elements: count_lines(&budgeted),
        system_bytes,
        user_bytes,
        user_elements,
    }
}

// ---- the case runner ------------------------------------------------------------------------

/// Measures every stage on `screen`.
pub fn run_case(id: &str, screen: &Screen, cfg: &Cfg) -> CaseResult {
    let mut out = CaseResult::default();
    let plan = &cfg.plan;
    let put = |rows: &mut Vec<Row>, stage: &str, stats: Option<Stats>, note: Option<String>| {
        rows.push(Row {
            case: id.to_owned(),
            stage: stage.to_owned(),
            stats,
            note,
        });
    };

    // Inputs, prepared once.
    let text = to_runner_text(&screen.root);
    let view: CompactView = compact_view(screen);
    let compact = &view.screen;
    let tree = ScreenTree {
        sources: &view.sources,
        parents: &view.parents,
        ends: &view.ends,
        bars: &view.bars,
        nodes: &view.nodes,
        inputs: &view.inputs,
    };
    let safe = safe_copy(compact);
    let controls: Vec<&Element> = compact
        .elements
        .iter()
        .filter(|e| takes_taps(&e.role))
        .collect();
    let target = pick_target(compact);
    let field = compact
        .elements
        .iter()
        .find(|e| e.role.is_text_input())
        .map(|e| e.index);
    out.counts = Counts {
        nodes: screen.root.count(),
        elements: compact.elements.len(),
        raw_nodes: view.nodes.len(),
        controls: controls.len(),
        text_bytes: text.len(),
    };

    // 1. Ingest.
    if cfg.wants("ingest.runner_text") {
        let s = measure(plan, || (), |()| parse_runner_text(&text));
        put(&mut out.rows, "ingest.runner_text", s, None);
    }
    #[cfg(feature = "ffi")]
    ffi_stages(&mut out.rows, id, screen, cfg);
    // The session owns the observation it is handed and frees the tree when it is done.
    if cfg.wants("ingest.drop_tree") {
        let s = measure(plan, || screen.clone(), drop);
        put(&mut out.rows, "ingest.drop_tree", s, None);
    }

    // 2-4. Compaction, digest, prompt-safe copy.
    if cfg.wants("compact") {
        let s = measure(plan, || (), |()| compact_view(screen));
        put(&mut out.rows, "compact", s, None);
    }
    if cfg.wants("digest") {
        let s = measure(plan, || (), |()| screen_digest(screen));
        put(&mut out.rows, "digest", s, None);
    }
    if cfg.wants("safe_copy") {
        let s = measure(plan, || (), |()| safe_copy(compact));
        put(&mut out.rows, "safe_copy", s, None);
    }

    // 5. Prompt rendering.
    let budget_tokens = screen_budget_tokens(cfg.context_tokens);
    if cfg.wants("render.full") {
        let s = measure(plan, || (), |()| render(&safe, u32::MAX));
        put(&mut out.rows, "render.full", s, None);
    }
    // `prompt::build` renders at budget 0 once per prompt, to learn the size of the header
    // and footer.
    if cfg.wants("render.zero") {
        let s = measure(plan, || (), |()| render(&safe, 0));
        put(&mut out.rows, "render.zero", s, None);
    }
    if cfg.wants("render.budget") {
        let s = measure(plan, || (), |()| render(&safe, budget_tokens));
        put(&mut out.rows, "render.budget", s, None);
    }

    // 6. Tap points.
    match target {
        Some(t) if cfg.wants("tap.one") => {
            let s = measure(
                plan,
                || (),
                |()| tap_spot(black_box(t), compact, Some(&tree), None),
            );
            let note = tap_spot(t, compact, Some(&tree), None)
                .is_err()
                .then(|| "target is covered".to_owned());
            put(&mut out.rows, "tap.one", s, note);
        }
        None if cfg.wants("tap.one") => {
            put(
                &mut out.rows,
                "tap.one",
                None,
                Some("no control".to_owned()),
            );
        }
        _ => {}
    }
    if cfg.wants("tap.sweep") {
        let (s, note) = sweep(&controls, compact, &tree, cfg);
        put(&mut out.rows, "tap.sweep", s, note);
    }

    // 7. Classification, evaluation, gate.
    let tap_index = target.map_or(1, |t| t.index);
    let actions: [(&str, Action); 4] = [
        ("tap", Action::Tap { element: tap_index }),
        (
            "type",
            Action::Type {
                element: field,
                text: "hello world".to_owned(),
                submit: false,
            },
        ),
        (
            "swipe",
            Action::Swipe {
                direction: Direction::Left,
                element: target.map(|t| t.index),
            },
        ),
        (
            "return",
            Action::Type {
                element: field,
                text: "hello world".to_owned(),
                submit: true,
            },
        ),
    ];
    let policy = Policy::default();
    for (name, action) in &actions {
        let stage = format!("classify.{name}");
        if cfg.wants(&stage) {
            let s = measure(plan, || (), |()| classify(black_box(action), compact, None));
            let note = (field.is_none() && matches!(action, Action::Type { .. }))
                .then(|| "no text field on screen: element is null".to_owned());
            put(&mut out.rows, &stage, s, note);
        }
        let stage = format!("evaluate.{name}");
        if cfg.wants(&stage) {
            let s = measure(
                plan,
                || (),
                |()| {
                    // Fourteenth review (Rule B): the search field's leniency rests on the
                    // owner's words, so the task the stage stands for gives them ("search
                    // for hello world"): `evaluate.return` keeps measuring the unasked
                    // search Return it always measured, not the question's composition.
                    evaluate_owned(
                        &policy,
                        black_box(action),
                        compact,
                        None,
                        ScreenSource::Typed,
                        None,
                        &[],
                        &["Search for hello world"],
                    )
                },
            );
            put(&mut out.rows, &stage, s, None);
        }
    }
    if cfg.wants("gate") {
        let tap = Action::Tap { element: tap_index };
        let s = measure(
            plan,
            || (),
            |()| {
                gate_on(
                    &policy,
                    black_box(RiskCategory::Payment),
                    TrustLevel::Guarded,
                    &tap,
                    compact,
                    false,
                )
            },
        );
        put(&mut out.rows, "gate", s, None);
    }
    let tap = Action::Tap { element: tap_index };
    let category = classify(&tap, compact, None);
    if cfg.wants("summary.tap") {
        let s = measure(
            plan,
            || (),
            |()| {
                summary_with(
                    &tap,
                    compact,
                    category,
                    ScreenSource::Typed,
                    Some(&tree),
                    &[],
                )
            },
        );
        put(&mut out.rows, "summary.tap", s, None);
    }

    // 8. The approval re-check, from the public pieces it is made of.
    let asked = approval_context(&tap, compact);
    let asked_digest = screen_digest(screen);
    if cfg.wants("recheck.holds_on") {
        let s = measure(plan, || (), |()| asked.holds_on(&tap, compact));
        put(&mut out.rows, "recheck.holds_on", s, None);
    }
    if cfg.wants("recheck.core") {
        let s = measure(
            plan,
            || (),
            |()| {
                let same_digest = screen_digest(screen) == asked_digest;
                let same_record = asked.holds_on(&tap, compact);
                let decision = evaluate_from(&policy, &tap, compact, None, ScreenSource::Typed);
                let summary = summary_with(
                    &tap,
                    compact,
                    category,
                    ScreenSource::Typed,
                    Some(&tree),
                    &[],
                );
                let spot = target.map(|t| tap_spot(t, compact, Some(&tree), None));
                (same_digest, same_record, decision, summary, spot)
            },
        );
        put(&mut out.rows, "recheck.core", s, None);
    }

    // 9. The session.
    session_stages(&mut out, id, screen, tap_index, cfg);
    out.prompt = prompt_info(&safe, cfg.context_tokens, out.prompt_request_sizes());
    out
}

impl CaseResult {
    fn prompt_request_sizes(&self) -> Option<(usize, usize, usize)> {
        let p = &self.prompt;
        (p.user_bytes > 0).then_some((p.system_bytes, p.user_bytes, p.user_elements))
    }
}

/// The sweep: `tap_spot` for every listed control that takes taps. A sweep that would take
/// longer than `cfg.sweep_cap_s` is run on an evenly spaced sample and scaled up.
fn sweep(
    controls: &[&Element],
    compact: &CompactScreen,
    tree: &ScreenTree<'_>,
    cfg: &Cfg,
) -> (Option<Stats>, Option<String>) {
    if controls.is_empty() {
        return (None, Some("no control".to_owned()));
    }
    let probe = controls.len().min(8);
    let start = Instant::now();
    for e in controls.iter().take(probe) {
        let _ = black_box(tap_spot(e, compact, Some(tree), None));
    }
    let per = start.elapsed().as_secs_f64() / probe as f64;
    let full = per * controls.len() as f64;
    let (subset, scale, note): (Vec<&Element>, f64, Option<String>) = if full <= cfg.sweep_cap_s {
        (controls.to_vec(), 1.0, None)
    } else {
        // `probe` is at most 8 and at most the number of controls: with fewer than 8 controls
        // on a slow run `clamp(8, len)` had min > max and panicked.
        let want = ((1.0 / per.max(1e-9)) as usize).clamp(probe, controls.len());
        let stride = controls.len().div_ceil(want);
        let sample: Vec<&Element> = controls.iter().step_by(stride).copied().collect();
        let scale = controls.len() as f64 / sample.len() as f64;
        let note = format!(
            "sampled {} of {} controls, scaled up (about {:.1} s if run in full)",
            sample.len(),
            controls.len(),
            full
        );
        (sample, scale, Some(note))
    };
    let s = measure(
        &cfg.plan,
        || (),
        |()| {
            for e in &subset {
                let _ = black_box(tap_spot(black_box(e), compact, Some(tree), None));
            }
        },
    );
    (s.map(|s| s.scaled(scale)), note)
}

fn session_stages(out: &mut CaseResult, id: &str, screen: &Screen, tap_index: u32, cfg: &Cfg) {
    let plan = &cfg.plan;
    let wants_any = [
        "step.observe_infer",
        "step.reply_act",
        "step.round",
        "step.verify_infer",
        "step.approved_look",
    ]
    .iter()
    .any(|s| cfg.wants(s));
    // The prompt of the first step is wanted whatever the filter says.
    let ctx = cfg.context_tokens;
    let mut start = new_session(ctx, false);
    let first = start.step(TimedEvent::new(0, Event::Start));
    let mut put = |stage: &str, stats: Option<Stats>, note: Option<String>| {
        out.rows.push(Row {
            case: id.to_owned(),
            stage: stage.to_owned(),
            stats,
            note,
        });
    };
    if !matches!(first, Effect::Observe { .. }) {
        put(
            "step.observe_infer",
            None,
            Some(format!("start gave {}", effect_name(&first))),
        );
        return;
    }
    let mut asked_obs = start.clone();
    let infer = asked_obs.step(TimedEvent::new(1_000, observed(screen)));
    if let Effect::Infer(req) = &infer {
        out.prompt.system_bytes = req.system.len();
        out.prompt.user_bytes = req.user.len();
        out.prompt.user_elements = count_lines(&req.user);
    }
    if !wants_any {
        return;
    }
    let note_for = |e: &Effect, want: &[&str]| -> Option<String> {
        (!want.contains(&effect_name(e))).then(|| format!("effect was {}", effect_name(e)))
    };

    if cfg.wants("step.observe_infer") {
        let s = measure(
            plan,
            || (start.clone(), TimedEvent::new(1_000, observed(screen))),
            |(mut s, ev)| {
                let e = s.step(ev);
                (s, e)
            },
        );
        put("step.observe_infer", s, note_for(&infer, &["infer"]));
    }

    let reply = || TimedEvent::new(2_000, tap_reply(tap_index));
    let mut probe = asked_obs.clone();
    let acted = probe.step(reply());
    if cfg.wants("step.reply_act") {
        let s = measure(
            plan,
            || (asked_obs.clone(), reply()),
            |(mut s, ev)| {
                let e = s.step(ev);
                (s, e)
            },
        );
        put("step.reply_act", s, note_for(&acted, &["act", "confirm"]));
    }
    if cfg.wants("step.round") {
        let s = measure(
            plan,
            || {
                (
                    start.clone(),
                    TimedEvent::new(1_000, observed(screen)),
                    reply(),
                )
            },
            |(mut s, first_ev, second_ev)| {
                let a = s.step(first_ev);
                let b = s.step(second_ev);
                (s, a, b)
            },
        );
        put("step.round", s, note_for(&acted, &["act", "confirm"]));
    }

    // The look after an action: the verification, then the next prompt (the steady state of
    // a task: every further step starts here).
    if cfg.wants("step.verify_infer") {
        let mut waiting = probe.clone();
        let observe = waiting.step(TimedEvent::new(2_500, Event::ActResult(RunnerResult::ok())));
        let mut changed = screen.clone();
        let did_change = perturb(&mut changed);
        let mut check = waiting.clone();
        let result = check.step(TimedEvent::new(3_000, observed(&changed)));
        let s = measure(
            plan,
            || (waiting.clone(), TimedEvent::new(3_000, observed(&changed))),
            |(mut s, ev)| {
                let e = s.step(ev);
                (s, e)
            },
        );
        let note = if matches!(acted, Effect::Confirm(_)) {
            Some(
                "the tap asks for confirmation: the look after an action is not measured"
                    .to_owned(),
            )
        } else if !matches!(observe, Effect::Observe { .. }) {
            Some(format!("the result gave {}", effect_name(&observe)))
        } else if !did_change {
            Some("no static text to change: the screen is unchanged".to_owned())
        } else {
            note_for(&result, &["infer"])
        };
        put("step.verify_infer", s, note);
    }

    // The approval: ask under StepByStep (every tap asks), approve, then the fresh look.
    if cfg.wants("step.approved_look") {
        let mut s = new_session(ctx, true);
        s.step(TimedEvent::new(0, Event::Start));
        s.step(TimedEvent::new(1_000, observed(screen)));
        let asked = s.step(TimedEvent::new(2_000, tap_reply(tap_index)));
        if !matches!(asked, Effect::Confirm(_)) {
            put(
                "step.approved_look",
                None,
                Some(format!("the tap gave {}, not confirm", effect_name(&asked))),
            );
            return;
        }
        let looking = s.step(TimedEvent::new(
            3_000,
            Event::UserDecision { approved: true },
        ));
        if !matches!(looking, Effect::Observe { .. }) {
            put(
                "step.approved_look",
                None,
                Some(format!("approval gave {}", effect_name(&looking))),
            );
            return;
        }
        let mut probe = s.clone();
        let result = probe.step(TimedEvent::new(4_000, observed(screen)));
        let stats = measure(
            plan,
            || (s.clone(), TimedEvent::new(4_000, observed(screen))),
            |(mut s, ev)| {
                let e = s.step(ev);
                (s, e)
            },
        );
        put("step.approved_look", stats, note_for(&result, &["act"]));
    }
}

#[cfg(feature = "ffi")]
fn ffi_stages(rows: &mut Vec<Row>, id: &str, screen: &Screen, cfg: &Cfg) {
    use jarvis_core_ffi as ffi;
    use uniffi::{Lift, Lower};

    let event = ffi::Event::Observed {
        observation: ffi::ScreenObservation {
            screen: ffi::Screen::from(screen.clone()),
            screenshot: None,
            device_locked: false,
        },
    };
    let mut put = |stage: &str, stats: Option<Stats>, note: Option<String>| {
        rows.push(Row {
            case: id.to_owned(),
            stage: stage.to_owned(),
            stats,
            note,
        });
    };
    if cfg.wants("ingest.ffi_lift") {
        let s = measure(
            &cfg.plan,
            || <ffi::Event as Lower<ffi::UniFfiTag>>::lower(event.clone()),
            <ffi::Event as Lift<ffi::UniFfiTag>>::try_lift_from_rust_buffer,
        );
        put("ingest.ffi_lift", s, None);
    }
    if cfg.wants("ingest.ffi_convert") {
        let s = measure(&cfg.plan, || event.clone(), ffi::event_to_proto);
        put("ingest.ffi_convert", s, None);
    }
}
