//! `SimPhone`: a deterministic stand-in for the iPhone plus its XCUITest runner.

use std::collections::{BTreeMap, BTreeSet};

use jarvis_observe::parse_runner_text;
use jarvis_proto::{
    AppInfo, Key, Observation, Orientation, Point, RunnerCommand, RunnerResult, Screen,
};
use serde_json::{json, Value};

use crate::app::{AppDef, Effect, ElementDef};
use crate::builtin::builtin_apps;
use crate::engine::{
    get_path, path_mut, render_app, set_path, ActiveAlert, AppRuntime, HitAction, Rendered,
    SCREEN_SIZE,
};
use crate::error::SimError;
use crate::expr::{display, eval, eval_bool, number, render_json, render_string, truthy, Env};
use crate::faults::Faults;
use crate::runner_text::to_runner_text;

/// Id of the home screen "app".
pub const HOME_APP: &str = "home";
/// Bundle id of the home screen.
pub const SPRINGBOARD_BUNDLE: &str = "com.apple.springboard";
/// Deepest `if` nesting an effect list may use.
const MAX_EFFECT_DEPTH: usize = 8;

/// How the screen is reported to the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ObserveMode {
    /// The `UiNode` tree as built, flags intact.
    #[default]
    Structured,
    /// The tree serialised to the runner's text format and parsed back: flags are lost.
    RunnerText,
}

impl ObserveMode {
    /// Parses `structured` or `runner-text`.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "structured" => Some(Self::Structured),
            "runner-text" | "runner_text" | "text" => Some(Self::RunnerText),
            _ => None,
        }
    }

    /// The CLI spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::Structured => "structured",
            Self::RunnerText => "runner-text",
        }
    }
}

/// Simulated milliseconds each kind of IO costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockCosts {
    /// One observation.
    pub observe_ms: u64,
    /// One model call.
    pub infer_ms: u64,
    /// One runner command (a `Wait` adds its own duration).
    pub act_ms: u64,
    /// One tool call.
    pub tool_ms: u64,
    /// One question to the owner.
    pub user_ms: u64,
}

impl Default for ClockCosts {
    /// 150 ms observe, 2 s inference, 400 ms act, 300 ms tool, 5 s owner.
    fn default() -> Self {
        Self {
            observe_ms: 150,
            infer_ms: 2_000,
            act_ms: 400,
            tool_ms: 300,
            user_ms: 5_000,
        }
    }
}

/// What the runner said, plus what the simulator did about it.
#[derive(Debug, Clone, PartialEq)]
pub struct ActOutcome {
    /// The reply for `Event::ActResult`.
    pub result: RunnerResult,
    /// One line for the transcript.
    pub note: String,
}

/// The phone's state at one moment, for assertions and success predicates.
#[derive(Debug, Clone, PartialEq)]
pub struct SimSnapshot {
    /// Id of the foreground app.
    pub app: String,
    /// Bundle id of the foreground app.
    pub bundle_id: String,
    /// Current screen name of the foreground app.
    pub screen: String,
    /// Alert showing in the foreground app.
    pub alert: Option<String>,
    /// Simulated time.
    pub now_ms: u64,
    /// State variables of every app by app id.
    pub states: BTreeMap<String, Value>,
    /// Number of runner commands the phone received.
    pub acts: usize,
    /// Whether the phone is locked.
    pub locked: bool,
}

impl SimSnapshot {
    /// The value predicates see: every app's state under its id, plus `phone`
    /// (`app`, `bundle_id`, `screen`, `alert`).
    pub fn root(&self) -> Value {
        let mut m = serde_json::Map::new();
        for (k, v) in &self.states {
            m.insert(k.clone(), v.clone());
        }
        m.insert(
            "phone".to_owned(),
            json!({
                "app": self.app,
                "bundle_id": self.bundle_id,
                "screen": self.screen,
                "alert": self.alert,
                "locked": self.locked,
            }),
        );
        Value::Object(m)
    }

    /// Evaluates an expression such as `messages.thread["Mom"].last == "running late"`.
    pub fn eval(&self, expr: &str) -> Result<Value, SimError> {
        let root = self.root();
        Ok(eval(expr, &Env::new().with(&root))?)
    }

    /// Evaluates an expression and returns its truthiness.
    pub fn check(&self, expr: &str) -> Result<bool, SimError> {
        let root = self.root();
        Ok(eval_bool(expr, &Env::new().with(&root))?)
    }
}

/// The simulated phone.
#[derive(Debug, Clone)]
pub struct SimPhone {
    apps: BTreeMap<String, AppRuntime>,
    foreground: String,
    focus: Option<String>,
    mode: ObserveMode,
    now_ms: u64,
    costs: ClockCosts,
    act_count: usize,
    fail_acts: BTreeSet<usize>,
    faults: Faults,
    lock: LockState,
}

/// The lock screen side of the phone and its fault counters.
#[derive(Debug, Clone, Default)]
struct LockState {
    /// Indices into `Faults::locks` that have fired.
    fired: BTreeSet<usize>,
    locked: bool,
    /// When the owner unlocks (`None` while locked: never).
    unlock_at: Option<u64>,
    /// Commands carried out since the last unlock without an observation in between.
    stale_acts: usize,
    /// An unlocked observation was taken since the last unlock.
    observed: bool,
    /// Commands refused because the phone was locked.
    refused: usize,
    /// Times the phone locked.
    locks: usize,
    /// Unlocked observations attempted.
    observe_attempts: usize,
    /// Observations that failed on purpose.
    observe_failures: usize,
    /// `TypeText` commands received.
    typed: usize,
}

/// What the runner reports for an observation.
#[derive(Debug, Clone, PartialEq)]
pub enum SimObservation {
    /// The screen.
    Screen(Observation),
    /// The phone is locked: a lock-screen snapshot flagged `device_locked`.
    Locked(Observation),
    /// The snapshot failed (an injected fault).
    Failed(String),
}

fn check_expr(app: &str, expr: &str) -> Result<(), SimError> {
    eval(expr, &Env::new())
        .map(|_| ())
        .map_err(|e| SimError::AppInvalid {
            app: app.to_owned(),
            reason: e.to_string(),
        })
}

fn check_effects(def: &AppDef, effects: &[Effect]) -> Result<(), SimError> {
    let bad = |reason: String| SimError::AppInvalid {
        app: def.id.clone(),
        reason,
    };
    for e in effects {
        match e {
            Effect::Goto { screen, .. } if !def.screens.contains_key(screen) => {
                return Err(bad(format!("goto unknown screen `{screen}`")));
            }
            Effect::Alert { id } if !def.alerts.contains_key(id) => {
                return Err(bad(format!("unknown alert `{id}`")));
            }
            Effect::If {
                cond,
                then,
                otherwise,
            } => {
                check_expr(&def.id, cond)?;
                check_effects(def, then)?;
                check_effects(def, otherwise)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn check_element(def: &AppDef, el: &ElementDef) -> Result<(), SimError> {
    for e in [&el.when, &el.enabled, &el.selected, &el.for_each]
        .into_iter()
        .flatten()
    {
        check_expr(&def.id, e)?;
    }
    check_effects(def, &el.on_tap)?;
    check_effects(def, &el.on_submit)?;
    el.children.iter().try_for_each(|c| check_element(def, c))
}

/// Checks an app definition for dangling screens, alerts and unparsable expressions.
pub fn validate_app(def: &AppDef) -> Result<(), SimError> {
    if !def.screens.contains_key(&def.start) {
        return Err(SimError::AppInvalid {
            app: def.id.clone(),
            reason: format!("start screen `{}` does not exist", def.start),
        });
    }
    for s in def.screens.values() {
        s.elements.iter().try_for_each(|e| check_element(def, e))?;
        s.nav_buttons
            .iter()
            .try_for_each(|e| check_element(def, e))?;
    }
    for a in def.alerts.values() {
        for b in &a.buttons {
            check_effects(def, &b.then)?;
        }
    }
    Ok(())
}

impl SimPhone {
    /// A phone with the built-in apps, showing the home screen.
    pub fn new() -> Result<Self, SimError> {
        Self::from_apps(builtin_apps()?)
    }

    /// A phone with these apps (one must have id `home`), showing the home screen.
    pub fn from_apps(defs: Vec<AppDef>) -> Result<Self, SimError> {
        let mut apps = BTreeMap::new();
        for d in defs {
            validate_app(&d)?;
            apps.insert(d.id.clone(), AppRuntime::new(d));
        }
        if !apps.contains_key(HOME_APP) {
            return Err(SimError::UnknownApp(HOME_APP.to_owned()));
        }
        Ok(Self {
            apps,
            foreground: HOME_APP.to_owned(),
            focus: None,
            mode: ObserveMode::Structured,
            now_ms: 0,
            costs: ClockCosts::default(),
            act_count: 0,
            fail_acts: BTreeSet::new(),
            faults: Faults::default(),
            lock: LockState {
                observed: true,
                ..LockState::default()
            },
        })
    }

    /// Injects faults (locks, failing observations, dropped characters).
    pub fn set_faults(&mut self, faults: Faults) {
        self.faults = faults;
    }

    /// The injected faults.
    pub fn faults(&self) -> &Faults {
        &self.faults
    }

    /// Whether the phone is locked.
    pub fn is_locked(&self) -> bool {
        self.lock.locked
    }

    /// Locks the phone now; the owner unlocks `unlock_after_ms` later (never for `None`).
    pub fn lock_now(&mut self, unlock_after_ms: Option<u64>) {
        if self.lock.locked {
            return;
        }
        self.lock.locked = true;
        self.lock.locks += 1;
        self.lock.unlock_at = unlock_after_ms.map(|d| self.now_ms.saturating_add(d));
    }

    /// The owner unlocks the phone (Face ID): the same app is in front, as it was left.
    pub fn unlock(&mut self) {
        if self.lock.locked {
            self.lock.locked = false;
            self.lock.unlock_at = None;
            self.lock.observed = false;
        }
    }

    /// Fires locks and unlocks that are due by the clock.
    pub fn apply_schedule(&mut self) {
        if self.lock.locked {
            if self.lock.unlock_at.is_some_and(|t| self.now_ms >= t) {
                self.unlock();
            }
            return;
        }
        let due = self.faults.locks.iter().enumerate().find(|(i, l)| {
            !self.lock.fired.contains(i) && l.at_ms.is_some_and(|t| self.now_ms >= t)
        });
        if let Some((i, spec)) = due {
            let after = spec.unlock_after_ms;
            self.lock.fired.insert(i);
            self.lock_now(after);
        }
    }

    /// Locks the phone if a lock is scheduled instead of the reply to effect `n`.
    pub fn lock_at_effect(&mut self, n: usize) -> bool {
        if self.lock.locked {
            return false;
        }
        let due = self
            .faults
            .locks
            .iter()
            .enumerate()
            .find(|(i, l)| !self.lock.fired.contains(i) && l.at_effect == Some(n));
        match due {
            Some((i, spec)) => {
                let after = spec.unlock_after_ms;
                self.lock.fired.insert(i);
                self.lock_now(after);
                true
            }
            None => false,
        }
    }

    /// The simulated shell waits for an unlock for up to `poll_ms`: true once unlocked.
    pub fn wait_for_unlock(&mut self, poll_ms: u64) -> bool {
        if !self.lock.locked {
            return true;
        }
        let step = match self.lock.unlock_at {
            Some(t) => t.saturating_sub(self.now_ms).clamp(1, poll_ms.max(1)),
            None => poll_ms.max(1),
        };
        self.advance(step);
        self.apply_schedule();
        !self.lock.locked
    }

    /// Runner commands carried out after an unlock before any fresh observation (each one
    /// acted on a screen decided before the lock). Must stay 0.
    pub fn stale_acts(&self) -> usize {
        self.lock.stale_acts
    }

    /// Runner commands refused because the phone was locked.
    pub fn locked_refusals(&self) -> usize {
        self.lock.refused
    }

    /// Times the phone locked.
    pub fn lock_count(&self) -> usize {
        self.lock.locks
    }

    /// Observations that failed on purpose.
    pub fn observe_failures(&self) -> usize {
        self.lock.observe_failures
    }

    /// What the runner reports for an observation now: the screen, a locked snapshot, or
    /// an injected failure.
    pub fn observe_result(&mut self) -> Result<SimObservation, SimError> {
        self.apply_schedule();
        if self.lock.locked {
            let screen = Screen {
                app_bundle_id: SPRINGBOARD_BUNDLE.to_owned(),
                app_name: Some("Lock Screen".to_owned()),
                size: SCREEN_SIZE,
                root: jarvis_proto::UiNode::new(
                    jarvis_proto::Role::Application,
                    SCREEN_SIZE.to_rect(),
                ),
                keyboard_visible: false,
                orientation: Orientation::Portrait,
            };
            return Ok(SimObservation::Locked(Observation {
                screen,
                screenshot: None,
                device_locked: true,
            }));
        }
        let n = self.lock.observe_attempts;
        self.lock.observe_attempts += 1;
        if let Some(f) = self.faults.observe_fail {
            if n >= f.after && n < f.after.saturating_add(f.times) {
                self.lock.observe_failures += 1;
                return Ok(SimObservation::Failed(
                    "snapshot failed: the accessibility tree did not arrive".to_owned(),
                ));
            }
        }
        let obs = self.observe()?;
        self.lock.observed = true;
        Ok(SimObservation::Screen(obs))
    }

    /// Chooses how observations are reported.
    pub fn set_mode(&mut self, mode: ObserveMode) {
        self.mode = mode;
    }

    /// The observation mode.
    pub fn mode(&self) -> ObserveMode {
        self.mode
    }

    /// Sets the simulated cost of each kind of IO.
    pub fn set_costs(&mut self, costs: ClockCosts) {
        self.costs = costs;
    }

    /// The simulated cost of each kind of IO.
    pub fn costs(&self) -> ClockCosts {
        self.costs
    }

    /// Simulated time in milliseconds.
    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }

    /// Advances the simulated clock.
    pub fn advance(&mut self, ms: u64) {
        self.now_ms = self.now_ms.saturating_add(ms);
    }

    /// Makes the `n`-th runner command (0-based, counting every command) fail once.
    pub fn fail_act_at(&mut self, n: usize) {
        self.fail_acts.insert(n);
    }

    /// Ids of all apps, sorted.
    pub fn app_ids(&self) -> Vec<String> {
        self.apps.keys().cloned().collect()
    }

    /// The definition of an app.
    pub fn app_def(&self, id: &str) -> Option<&AppDef> {
        self.apps.get(id).map(|a| &a.def)
    }

    /// Apps the agent may open by name (everything except the home screen).
    pub fn known_apps(&self) -> Vec<AppInfo> {
        self.apps
            .values()
            .filter(|a| a.def.id != HOME_APP)
            .map(|a| AppInfo::new(a.def.bundle_id.clone(), a.def.name.clone()))
            .collect()
    }

    /// Brings an app to the front (like tapping its icon).
    pub fn launch(&mut self, id: &str) -> Result<(), SimError> {
        if !self.apps.contains_key(id) {
            return Err(SimError::UnknownApp(id.to_owned()));
        }
        self.foreground = id.to_owned();
        self.focus = None;
        Ok(())
    }

    /// Shows a specific screen of an app (for `dump` and tests), resetting its navigation.
    pub fn show_screen(&mut self, id: &str, screen: &str) -> Result<(), SimError> {
        let rt = self
            .apps
            .get_mut(id)
            .ok_or_else(|| SimError::UnknownApp(id.to_owned()))?;
        if !rt.def.screens.contains_key(screen) {
            return Err(SimError::UnknownScreen {
                app: id.to_owned(),
                screen: screen.to_owned(),
            });
        }
        rt.nav = vec![screen.to_owned()];
        rt.alert = None;
        self.foreground = id.to_owned();
        self.focus = None;
        Ok(())
    }

    /// Mutable access to an app's state variable (for setting up scenarios in tests).
    pub fn set_state(&mut self, id: &str, path: &str, value: Value) -> Result<(), SimError> {
        let rt = self
            .apps
            .get_mut(id)
            .ok_or_else(|| SimError::UnknownApp(id.to_owned()))?;
        if set_path(&mut rt.state, path, value) {
            Ok(())
        } else {
            Err(SimError::Input(format!("cannot set `{id}.{path}`")))
        }
    }

    fn fg(&self) -> Option<&AppRuntime> {
        self.apps.get(&self.foreground)
    }

    fn render_fg(&self) -> Option<Rendered> {
        self.fg().map(|rt| render_app(rt, self.focus.as_deref()))
    }

    fn refresh_focus(&mut self) {
        if let Some(r) = self.render_fg() {
            if !r.focus_found {
                self.focus = None;
            }
        }
    }

    /// The rendered tree of the foreground app, flags intact.
    pub fn structured_root(&mut self) -> Result<jarvis_proto::UiNode, SimError> {
        self.refresh_focus();
        self.render_fg()
            .map(|r| r.root)
            .ok_or_else(|| SimError::UnknownApp(self.foreground.clone()))
    }

    /// The runner's text rendering of the foreground app.
    pub fn runner_text(&mut self) -> Result<String, SimError> {
        Ok(to_runner_text(&self.structured_root()?))
    }

    /// What the runner reports now, in the configured mode.
    pub fn observe(&mut self) -> Result<Observation, SimError> {
        self.refresh_focus();
        let r = self
            .render_fg()
            .ok_or_else(|| SimError::UnknownApp(self.foreground.clone()))?;
        let (bundle, name) = self
            .fg()
            .map(|a| (a.def.bundle_id.clone(), a.def.name.clone()))
            .unwrap_or_default();
        let root = match self.mode {
            ObserveMode::Structured => r.root,
            ObserveMode::RunnerText => {
                parse_runner_text(&to_runner_text(&r.root))
                    .map_err(|e| SimError::RunnerText(e.to_string()))?
                    .root
            }
        };
        Ok(Observation::new(Screen {
            app_bundle_id: bundle,
            app_name: Some(name),
            size: SCREEN_SIZE,
            root,
            keyboard_visible: r.keyboard,
            orientation: Orientation::Portrait,
        }))
    }

    /// The phone's state for assertions.
    pub fn snapshot(&self) -> SimSnapshot {
        let (bundle_id, screen, alert) = self.fg().map_or_else(Default::default, |a| {
            (
                a.def.bundle_id.clone(),
                a.screen_name().to_owned(),
                a.alert.as_ref().map(|x| x.id.clone()),
            )
        });
        SimSnapshot {
            app: self.foreground.clone(),
            bundle_id,
            screen,
            alert,
            now_ms: self.now_ms,
            states: self
                .apps
                .iter()
                .map(|(k, v)| (k.clone(), v.state.clone()))
                .collect(),
            acts: self.act_count,
            locked: self.lock.locked,
        }
    }

    // ---------------------------------------------------------------------------------
    // Runner commands.
    // ---------------------------------------------------------------------------------

    /// Carries out one runner command. While locked every command is refused with
    /// `RunnerResult::device_locked` and nothing happens.
    pub fn act(&mut self, cmd: &RunnerCommand) -> ActOutcome {
        let n = self.act_count;
        self.act_count += 1;
        self.apply_schedule();
        if !self.lock.locked {
            let due = self
                .faults
                .locks
                .iter()
                .enumerate()
                .find(|(i, l)| !self.lock.fired.contains(i) && l.before_act == Some(n));
            if let Some((i, spec)) = due {
                let after = spec.unlock_after_ms;
                self.lock.fired.insert(i);
                self.lock_now(after);
            }
        }
        if self.lock.locked {
            self.lock.refused += 1;
            return ActOutcome {
                note: format!("REFUSED: {}: the phone is locked", cmd.name()),
                result: RunnerResult::device_locked(),
            };
        }
        if !self.lock.observed {
            self.lock.stale_acts += 1;
        }
        if self.fail_acts.remove(&n) {
            return fail(format!("{}: injected runner failure", cmd.name()));
        }
        match cmd {
            RunnerCommand::Tap { x, y } => self.tap(Point::new(*x, *y)),
            RunnerCommand::LongPress { x, y, .. } => {
                let p = Point::new(*x, *y);
                let what = self.hit_description(p);
                good(format!(
                    "long press at ({x:.0}, {y:.0}) on {what}: no effect"
                ))
            }
            RunnerCommand::Swipe { from, to, .. } => self.swipe(*from, *to),
            RunnerCommand::TypeText { text } => self.type_text(text, true),
            RunnerCommand::PressKey { key } => self.press_key(*key),
            RunnerCommand::LaunchApp { bundle_id } => {
                let found = self
                    .apps
                    .values()
                    .find(|a| a.def.bundle_id == *bundle_id)
                    .map(|a| a.def.id.clone());
                match found {
                    Some(id) => {
                        self.foreground = id.clone();
                        self.focus = None;
                        good(format!("launched {id}"))
                    }
                    None => fail(format!("no app with bundle id {bundle_id} is installed")),
                }
            }
            RunnerCommand::OpenUrl { url } => {
                let found = self
                    .apps
                    .values()
                    .find(|a| a.def.url_prefixes.iter().any(|p| url.starts_with(p)))
                    .map(|a| a.def.id.clone());
                match found {
                    Some(id) => {
                        self.foreground = id.clone();
                        self.focus = None;
                        good(format!("opened {url} in {id}"))
                    }
                    None => fail(format!("no app handles {url}")),
                }
            }
            RunnerCommand::Wait { ms } => {
                self.advance(*ms);
                good(format!("waited {ms} ms"))
            }
        }
    }

    fn hit_description(&self, p: Point) -> String {
        self.render_fg()
            .and_then(|r| {
                r.hits
                    .iter()
                    .rev()
                    .find(|h| h.frame.contains(p))
                    .map(|h| h.describe())
            })
            .unwrap_or_else(|| "empty space".to_owned())
    }

    fn tap(&mut self, p: Point) -> ActOutcome {
        let Some(r) = self.render_fg() else {
            return fail("no foreground app".to_owned());
        };
        let Some(hit) = r.hits.iter().rev().find(|h| h.frame.contains(p)).cloned() else {
            return good(format!("tap at ({:.0}, {:.0}): nothing there", p.x, p.y));
        };
        let at = format!("tap at ({:.0}, {:.0}) on {}", p.x, p.y, hit.describe());
        if !hit.enabled {
            return good(format!("{at}: disabled, no effect"));
        }
        match &hit.action {
            HitAction::Swallow => good(format!("{at}: absorbed, no effect")),
            HitAction::Key(k) => {
                let note = self.key_label(k);
                good(format!("{at}: {note}"))
            }
            HitAction::Effects => {
                if hit.bind.is_some() {
                    self.focus.clone_from(&hit.key);
                }
                let mut notes = Vec::new();
                let app = self.foreground.clone();
                self.run_effects(&app, &hit.on_tap, &hit.scope, &mut notes, 0);
                let tail = if notes.is_empty() {
                    if hit.bind.is_some() {
                        "focused".to_owned()
                    } else {
                        "no effect".to_owned()
                    }
                } else {
                    notes.join("; ")
                };
                good(format!("{at}: {tail}"))
            }
        }
    }

    fn swipe(&mut self, from: Point, to: Point) -> ActOutcome {
        let Some(r) = self.render_fg() else {
            return fail("no foreground app".to_owned());
        };
        let dy = from.y - to.y;
        if dy.abs() < (from.x - to.x).abs() {
            return good("horizontal swipe: no effect".to_owned());
        }
        let alert = self.fg().is_some_and(|a| a.alert.is_some());
        let Some(region) = r.scrolls.iter().rev().find(|s| s.frame.contains(from)) else {
            return good("swipe: nothing scrollable under the finger".to_owned());
        };
        if alert {
            return good("swipe: blocked by an alert".to_owned());
        }
        let max = (region.content_h - region.frame.height).max(0.0);
        let rt = match self.apps.get_mut(&self.foreground) {
            Some(rt) => rt,
            None => return fail("no foreground app".to_owned()),
        };
        let cur = rt
            .scroll
            .get(&region.key)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, max);
        let next = (cur + dy).clamp(0.0, max);
        rt.scroll.insert(region.key.clone(), next);
        if (next - cur).abs() < f64::EPSILON {
            good(format!("scroll: already at the end (offset {cur:.0})"))
        } else {
            good(format!("scroll: offset {cur:.0} -> {next:.0} of {max:.0}"))
        }
    }

    fn focused_hit(&self) -> Option<crate::engine::Hit> {
        let focus = self.focus.as_deref()?;
        self.render_fg()?
            .hits
            .into_iter()
            .rev()
            .find(|h| h.bind.is_some() && h.key.as_deref() == Some(focus))
    }

    fn edit_bound(&mut self, bind: &str, f: impl FnOnce(String) -> String) {
        let Some(rt) = self.apps.get_mut(&self.foreground) else {
            return;
        };
        let cur = display(&get_path(&rt.state, bind));
        set_path(&mut rt.state, bind, Value::String(f(cur)));
    }

    /// Types into the focused field; `agent` typing is subject to dropped characters.
    fn type_text(&mut self, text: &str, agent: bool) -> ActOutcome {
        let Some(hit) = self.focused_hit() else {
            return fail("no text field has keyboard focus".to_owned());
        };
        let Some(bind) = hit.bind.clone() else {
            return fail("focused element is not a text input".to_owned());
        };
        let n = self.lock.typed;
        if agent {
            self.lock.typed += 1;
        }
        let t = match self.faults.drop_chars {
            Some(d) if agent && n < d.times => d.apply(text),
            _ => text.to_owned(),
        };
        self.edit_bound(&bind, |cur| cur + &t);
        let shown = if hit.role == jarvis_proto::Role::SecureTextField {
            format!("{} characters", text.chars().count())
        } else if t != text {
            format!("\"{text}\" but only \"{t}\" arrived (dropped characters)")
        } else {
            format!("\"{text}\"")
        };
        good(format!("typed {shown} into {}", hit.describe()))
    }

    /// The owner types `text` on the phone themselves: into the first secure field on the
    /// screen (where the agent hands over), else into the focused field.
    pub fn owner_types(&mut self, text: &str) -> ActOutcome {
        let Some(r) = self.render_fg() else {
            return fail("no foreground app".to_owned());
        };
        let secure = r.hits.iter().find(|h| {
            h.bind.is_some() && h.enabled && h.role == jarvis_proto::Role::SecureTextField
        });
        if let Some(h) = secure {
            self.focus.clone_from(&h.key);
        }
        self.type_text(text, false)
    }

    fn key_label(&mut self, label: &str) -> String {
        let Some(hit) = self.focused_hit() else {
            return "no focused field".to_owned();
        };
        let Some(bind) = hit.bind.clone() else {
            return "no focused field".to_owned();
        };
        match label {
            "delete" => {
                self.edit_bound(&bind, |mut s| {
                    s.pop();
                    s
                });
                "deleted a character".to_owned()
            }
            "return" => {
                let mut notes = Vec::new();
                let app = self.foreground.clone();
                self.run_effects(&app, &hit.on_submit, &hit.scope, &mut notes, 0);
                format!("return; {}", notes.join("; "))
            }
            other => {
                let c = if other == "space" { " " } else { other };
                let c = c.to_owned();
                self.edit_bound(&bind, |cur| cur + &c);
                format!("typed {c:?}")
            }
        }
    }

    fn press_key(&mut self, key: Key) -> ActOutcome {
        match key {
            Key::Home => {
                self.foreground = HOME_APP.to_owned();
                self.focus = None;
                good("home".to_owned())
            }
            Key::Return => good(self.key_label("return")),
            Key::Delete => good(self.key_label("delete")),
            Key::Space => good(self.key_label("space")),
            Key::Tab | Key::Escape => good(format!("{key:?}: no effect")),
        }
    }

    // ---------------------------------------------------------------------------------
    // Effects.
    // ---------------------------------------------------------------------------------

    fn run_effects(
        &mut self,
        app: &str,
        effects: &[Effect],
        scope: &Value,
        notes: &mut Vec<String>,
        depth: usize,
    ) {
        if depth > MAX_EFFECT_DEPTH {
            notes.push("effects nested too deeply".to_owned());
            return;
        }
        for eff in effects {
            let Some(rt) = self.apps.get_mut(app) else {
                return;
            };
            let env_text = |t: &str, rt: &AppRuntime| {
                render_string(t, &Env::new().with(scope).with(&rt.state))
            };
            match eff {
                Effect::Goto {
                    screen,
                    replace,
                    reset,
                } => {
                    if *reset {
                        rt.nav.clear();
                    } else if *replace {
                        rt.nav.pop();
                    }
                    rt.nav.push(screen.clone());
                    rt.alert = None;
                    self.focus = None;
                    notes.push(format!("goto {screen}"));
                }
                Effect::Back => {
                    if rt.nav.len() > 1 {
                        rt.nav.pop();
                    }
                    self.focus = None;
                    notes.push("back".to_owned());
                }
                Effect::Set { var, value } => {
                    let path = env_text(var, rt);
                    let v = render_json(value, &Env::new().with(scope).with(&rt.state));
                    notes.push(format!("{path} = {}", display(&v)));
                    set_path(&mut rt.state, &path, v);
                }
                Effect::Toggle { var } => {
                    let path = env_text(var, rt);
                    let now = !truthy(&get_path(&rt.state, &path));
                    notes.push(format!("{path} = {now}"));
                    set_path(&mut rt.state, &path, Value::Bool(now));
                }
                Effect::Add { var, by } => {
                    let path = env_text(var, rt);
                    let by = render_json(by, &Env::new().with(scope).with(&rt.state));
                    let sum = get_path(&rt.state, &path).as_f64().unwrap_or(0.0)
                        + by.as_f64().unwrap_or(0.0);
                    set_path(&mut rt.state, &path, number(sum));
                    notes.push(format!("{path} += {}", display(&by)));
                }
                Effect::Append { to, value } => {
                    let path = env_text(to, rt);
                    let v = render_json(value, &Env::new().with(scope).with(&rt.state));
                    notes.push(format!("{path} += {}", display(&v)));
                    if get_path(&rt.state, &path).is_null() {
                        set_path(&mut rt.state, &path, Value::Array(Vec::new()));
                    }
                    if let Some(Value::Array(a)) = path_mut(&mut rt.state, &path) {
                        a.push(v);
                    }
                }
                Effect::Remove { from, index } => {
                    let path = env_text(from, rt);
                    let ix = render_json(index, &Env::new().with(scope).with(&rt.state));
                    let ix = ix.as_u64().and_then(|n| usize::try_from(n).ok());
                    if let (Some(ix), Some(Value::Array(a))) = (ix, path_mut(&mut rt.state, &path))
                    {
                        if ix < a.len() {
                            a.remove(ix);
                            notes.push(format!("removed {path}[{ix}]"));
                        }
                    }
                }
                Effect::Alert { id } => {
                    rt.alert = Some(ActiveAlert {
                        id: id.clone(),
                        scope: scope.clone(),
                    });
                    self.focus = None;
                    notes.push(format!("alert {id}"));
                }
                Effect::Dismiss => {
                    rt.alert = None;
                    notes.push("dismissed alert".to_owned());
                }
                Effect::Launch { app: target } => {
                    if self.apps.contains_key(target) {
                        self.foreground = target.clone();
                        self.focus = None;
                        notes.push(format!("launched {target}"));
                    } else {
                        notes.push(format!("cannot launch unknown app {target}"));
                    }
                }
                Effect::If {
                    cond,
                    then,
                    otherwise,
                } => {
                    let yes =
                        eval_bool(cond, &Env::new().with(scope).with(&rt.state)).unwrap_or(false);
                    let branch = if yes { then } else { otherwise };
                    self.run_effects(app, branch, scope, notes, depth + 1);
                }
            }
        }
    }
}

fn good(note: String) -> ActOutcome {
    ActOutcome {
        result: RunnerResult::ok(),
        note,
    }
}

fn fail(error: String) -> ActOutcome {
    ActOutcome {
        note: format!("FAILED: {error}"),
        result: RunnerResult::err(error),
    }
}
