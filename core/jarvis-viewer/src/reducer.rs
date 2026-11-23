//! Reduces what the shell's driver reports during a run into the Live view's
//! [`ViewerTimeline`]: one step per runner command, the screen it acted on, the element
//! under the pointer and a short caption.
//!
//! # Input
//!
//! [`Update`] mirrors the driver's update stream (`DriverUpdate` in the Swift shell) with
//! core types, so the shell forwards each update as it comes, one call each. The order the
//! driver produces: `Started` first; then for every effect `Effect`, `Progress`, and unless
//! the effect is `Finished`, the effect's own updates (`ConfirmationPending`,
//! `QuestionPending`, `AwaitingUnlock`, `Ignored` for lock notes, IO failures and notes)
//! and `EffectCompleted`; `Finished` last. `Prompt` (the user part of a model request)
//! arrives while an `infer` effect runs. The reducer never panics and stays bounded on any
//! other order, duplicates included.
//!
//! The target element is a **display aid only**: the deepest shown element whose frame
//! contains the point, so the viewer can outline it and name it in the caption. It is never
//! used to decide anything; the core alone decides what to do and what needs the owner's
//! OK.
//!
//! # Privacy (each rule is tested)
//!
//! - Every label, placeholder and app name goes through the core's redaction before it is
//!   kept, so the timeline (and its encoding) never holds the raw text. The wireframe keeps
//!   labels and placeholders, never values.
//! - Typed text is never taken from the runner command (which carries the owner's stored
//!   values filled in). The caption shows the text the model or the learned flow asked for
//!   (`Progress::last_action`), with `{{profile.…}}` placeholders masked, only when ALL
//!   hold: the field was resolved from a typed tree (never runner text, whose prompt line
//!   carries no secure marker), it is not secure, the core says it is not credential-like,
//!   the core's redaction leaves the text unchanged, and it is exactly what was typed.
//!   Otherwise the caption is `Type into “<field>”` (the field's redacted label) or
//!   `Type text`. Only the length and a hash of the runner's text are held, until the
//!   next progress.
//! - On runner-text screens typing captions and the approval picture's label come only from
//!   the prompt the core rendered (redacted again). The wireframe of a runner-text screen is
//!   built from the session's parse of the text like a typed tree's (same cap, same
//!   redaction, never a value) and flagged by its `runner_text` source.
//! - `open_url` keeps the host of an `http`/`https` URL, only `<scheme>:` otherwise.
//! - A locked observation keeps nothing of the screen.
//! - Runner error texts are not kept (they may quote what was typed).
//!
//! # Memory
//!
//! At most [`MAX_STEPS`] steps; screens no step refers to (other than the current one) are
//! dropped as soon as that happens; one tree (the current screen's: typed, or parsed
//! runner text, which is used for its wireframe and the approval picture, not for step
//! targets) is held for target lookups.
//!
//! # Approval picture
//!
//! On `ConfirmationPending` the reducer computes [`Reducer::approval`] (see
//! [`crate::approval`]): the evaluated observation's key, its screen and the target the
//! core resolved and stated in the request (`ConfirmRequest::target`). Cleared by the next
//! update of any other kind; never part of the timeline.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use jarvis_proto as p;

use crate::approval::{self, ApprovalTarget, Evaluated};
use crate::geometry::{ViewerPoint, ViewerRect, ViewerSize};
use crate::privacy::{self, LabelCache};
use crate::prompt::{prompt_elements, PromptElement};
use crate::wire::{
    Activity, ElementKind, FrameKey, Gesture, ScreenSource, StepState, ViewerElement, ViewerScreen,
    ViewerStep, ViewerTimeline, MAX_ELEMENTS, MAX_STEPS,
};

/// Tree nodes looked at per screen (a pathological tree cannot stall the view).
pub const MAX_VISITED_NODES: usize = 20_000;
/// Longest bundle id kept (longer ones are cut with "…").
pub const MAX_BUNDLE_ID_CHARACTERS: usize = 128;

/// One thing the driver reported, with core types.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum Update {
    /// A run began (`TaskSpec::id`).
    Started {
        /// The task id.
        task_id: String,
    },
    /// The user part of a model request (what the model is shown).
    Prompt(String),
    /// The core asked for `effect`; `number` counts effects from 1.
    Effect {
        /// The effect.
        effect: p::Effect,
        /// Its number.
        number: u64,
    },
    /// The session's progress after that step.
    Progress(p::Progress),
    /// A confirmation waits for the owner.
    ConfirmationPending(p::ConfirmRequest),
    /// A question waits for the owner.
    QuestionPending,
    /// The session waits for the owner to unlock the phone.
    AwaitingUnlock,
    /// The effect was carried out and `event` goes back to the core.
    EffectCompleted {
        /// The effect.
        effect: p::Effect,
        /// Its result.
        event: p::Event,
    },
    /// The run ended with this status.
    Finished(p::OutcomeStatus),
    /// Anything the viewer does not show (model replies, lock notes, IO failures, notes).
    Ignored,
}

/// How the reducer is set up.
#[derive(Debug, Clone, PartialEq)]
pub struct ReducerConfig {
    /// The step cap, clamped to `1..=MAX_STEPS`.
    pub max_steps: usize,
    /// The shell keeps real-screen frames for this run: unlocked screens get a frame key.
    pub frames_enabled: bool,
    /// Apps named by their display name in "Open …" captions.
    pub known_apps: Vec<p::AppInfo>,
}

impl Default for ReducerConfig {
    fn default() -> Self {
        ReducerConfig {
            max_steps: MAX_STEPS,
            frames_enabled: false,
            known_apps: Vec::new(),
        }
    }
}

/// What a typed tree says about a field typing may go into (display only).
#[derive(Debug, Clone, PartialEq, Eq)]
struct FieldEvidence {
    /// The core hides its value (secure, or credential-like).
    hidden: bool,
    /// An entry field.
    input: bool,
    /// Its label (else placeholder), redacted and capped.
    label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PointerField {
    number: u32,
    screen_id: Option<u32>,
    field: FieldEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingType {
    number: u32,
    /// The core would hide the field's value (or the field is secure).
    hidden: bool,
    /// A typed tree vouches for a plain entry field.
    plain: bool,
    /// The field's redacted label, for "Type into …".
    label: Option<String>,
    /// Length and in-process hash of the runner's text: the model's text is shown only if
    /// it is exactly what was typed (or the core filled placeholders into it).
    raw_count: usize,
    raw_hash: u64,
}

/// What an update changed, so the shell republishes only what moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Change {
    /// Steps were added, changed or dropped (or the run restarted).
    pub steps: bool,
    /// A screen was added or dropped.
    pub screens: bool,
    /// The activity changed.
    pub activity: bool,
    /// The approval picture was set, replaced or cleared ([`Reducer::approval`]).
    pub approval: bool,
}

impl Change {
    /// Anything changed.
    pub fn any(&self) -> bool {
        self.steps || self.screens || self.activity || self.approval
    }
}

/// The Live view reducer (the Swift `PointerTimeline`).
#[derive(Debug, Clone)]
pub struct Reducer {
    timeline: ViewerTimeline,
    config: ReducerConfig,
    /// The number of the effect being handled.
    effect_number: u64,
    /// The current screen's typed tree (target lookup); not part of the timeline.
    tree: Option<p::UiNode>,
    /// The element list of the latest prompt; cleared when the command it led to completes.
    sight: Vec<PromptElement>,
    /// The core's step counter from the latest progress.
    core_step: Option<u32>,
    /// The step the current `act` effect created.
    act_step: Option<u32>,
    /// A typing step waiting for the model's text (the progress right after its effect).
    pending_type: Option<PendingType>,
    next_screen_id: u32,
    /// What the current screen's typed tree says about its focused field.
    focused_field: Option<FieldEvidence>,
    /// The field under the latest pointer step's point, from the typed tree.
    pointer_field: Option<PointerField>,
    /// Display labels already produced (no raw text; see `LabelCache`).
    labels: LabelCache,
    /// The current (unlocked) screen with a placeholder root, the number of the effect that
    /// observed it, and whether it is a typed tree: what a pending confirmation was
    /// evaluated on. Its tree is `tree` (typed) or `text_root` (parsed runner text).
    evaluated: Option<(p::Screen, u64, bool)>,
    /// The current runner-text screen's parsed tree: approval lookups only (its wireframe was
    /// made when it was observed).
    text_root: Option<p::UiNode>,
    /// The approval sheet's picture of the confirmation pending now (not in the timeline).
    approval: Option<ApprovalTarget>,
}

impl Default for Reducer {
    fn default() -> Self {
        Reducer::new(ReducerConfig::default())
    }
}

impl Reducer {
    /// A reducer with an empty timeline.
    pub fn new(mut config: ReducerConfig) -> Self {
        config.max_steps = config.max_steps.clamp(1, MAX_STEPS);
        Reducer {
            timeline: ViewerTimeline::default(),
            config,
            effect_number: 0,
            tree: None,
            sight: Vec::new(),
            core_step: None,
            act_step: None,
            pending_type: None,
            next_screen_id: 1,
            focused_field: None,
            pointer_field: None,
            labels: LabelCache::new(),
            evaluated: None,
            text_root: None,
            approval: None,
        }
    }

    /// The approval sheet's picture of the confirmation pending now: set by
    /// `ConfirmationPending` (when an unlocked screen was observed), cleared by the next
    /// update of any other kind. Never part of the timeline, so never encoded.
    pub fn approval(&self) -> Option<&ApprovalTarget> {
        self.approval.as_ref()
    }

    /// Computes [`Self::approval`] for `request` on the current screen.
    fn make_approval(&mut self, request: p::ConfirmRequest) -> Option<ApprovalTarget> {
        let (mut screen, observation, typed) = self.evaluated.take()?;
        let screen_id = self.timeline.current_screen_id;
        let slot = if typed {
            &mut self.tree
        } else {
            &mut self.text_root
        };
        let Some(root) = slot.take() else {
            self.evaluated = Some((screen, observation, typed));
            return None;
        };
        screen.root = root;
        // What is asked and where: as the core stated it in the request (its stage, kind and
        // resolved target), never from the question's text or the step history.
        let (mark, caption, accessibility_label) = approval::mark_for(
            &Evaluated {
                screen: &screen,
                typed,
                sight: &self.sight,
            },
            &request,
            &mut self.labels,
        );
        let size = ViewerSize::from(screen.size);
        let placeholder = p::UiNode::new(p::Role::Application, screen.size.to_rect());
        let root = std::mem::replace(&mut screen.root, placeholder);
        if typed {
            self.tree = Some(root);
        } else {
            self.text_root = Some(root);
        }
        self.evaluated = Some((screen, observation, typed));
        Some(ApprovalTarget {
            frame_key: FrameKey {
                task_id: self.timeline.task_id.clone(),
                observation,
            },
            screen_id: screen_id?,
            size,
            request,
            mark,
            caption,
            accessibility_label,
        })
    }

    /// Remembers `screen` (its tree goes to `tree` or `text_root`) as the current one.
    fn remember_screen(&mut self, mut screen: p::Screen, typed: bool) {
        let placeholder = p::UiNode::new(p::Role::Application, screen.size.to_rect());
        let root = std::mem::replace(&mut screen.root, placeholder);
        if typed {
            self.tree = Some(root);
            self.text_root = None;
        } else {
            self.tree = None;
            self.text_root = Some(root);
        }
        self.evaluated = Some((screen, self.effect_number, typed));
    }

    /// No current screen to evaluate (locked).
    fn forget_screen(&mut self) {
        self.tree = None;
        self.text_root = None;
        self.evaluated = None;
    }

    /// A reducer continuing from a decoded timeline (e.g. on a remote controller). Typing
    /// evidence and the tree are not carried over; new screens get fresh ids.
    pub fn resume(timeline: ViewerTimeline, config: ReducerConfig) -> Self {
        let mut r = Reducer::new(config);
        r.next_screen_id = timeline
            .screens
            .iter()
            .map(|s| s.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        r.timeline = timeline;
        r
    }

    /// The timeline so far.
    pub fn timeline(&self) -> &ViewerTimeline {
        &self.timeline
    }

    /// The timeline, for a remote controller's decoder ([`crate::delta`]).
    pub(crate) fn timeline_mut(&mut self) -> &mut ViewerTimeline {
        &mut self.timeline
    }

    /// The configuration (with the clamped cap).
    pub fn config(&self) -> &ReducerConfig {
        &self.config
    }

    /// Steps so far.
    pub fn steps(&self) -> &[ViewerStep] {
        &self.timeline.steps
    }

    /// Applies one update.
    pub fn apply(&mut self, update: Update) -> Change {
        let before_steps = self.steps_signature();
        let before_screens = self.screens_signature();
        let before_activity = self.timeline.activity.clone();
        let mut steps_touched = false;
        let had_approval = self.approval.is_some();
        if !matches!(update, Update::ConfirmationPending(_)) {
            self.approval = None;
        }
        match update {
            Update::Started { task_id } => {
                let config = self.config.clone();
                *self = Reducer::new(config);
                self.timeline.task_id = task_id;
                steps_touched = true;
            }
            Update::Prompt(text) => self.sight = prompt_elements(&text),
            Update::Effect { effect, number } => {
                self.effect_number = number;
                self.act_step = None;
                self.pending_type = None;
                match effect {
                    p::Effect::Observe { .. } => self.timeline.activity = Activity::Looking,
                    p::Effect::Infer(_) => self.timeline.activity = Activity::Thinking,
                    p::Effect::Act(command) => {
                        self.add_step(command);
                        self.timeline.activity = Activity::Acting;
                        steps_touched = true;
                    }
                    p::Effect::CallTool(call) => {
                        self.timeline.activity = Activity::UsingTool(
                            self.labels
                                .label(Some(&call.name))
                                .unwrap_or_else(|| "a tool".to_owned()),
                        );
                    }
                    p::Effect::Confirm(request) => {
                        self.timeline.activity = Activity::WaitingForApproval(
                            risk_category_title(request.category).to_owned(),
                        );
                    }
                    p::Effect::AskUser { .. } => {
                        self.timeline.activity = Activity::WaitingForAnswer
                    }
                    p::Effect::AwaitUnlock { .. } => self.timeline.activity = Activity::Locked,
                    p::Effect::Finished(_) => {}
                }
            }
            Update::Progress(progress) => {
                self.core_step = Some(progress.step);
                // The progress right after an `act` effect counts the step that sent it.
                if let Some(i) = self.act_step.and_then(|n| self.timeline.step_index(n)) {
                    if let Some(step) = self.timeline.steps.get_mut(i) {
                        step.core_step = self.core_step;
                        steps_touched = true;
                    }
                }
                if let Some(action) = progress.last_action {
                    if self.resolve_pending(action) {
                        steps_touched = true;
                    }
                }
            }
            Update::ConfirmationPending(request) => {
                self.timeline.activity =
                    Activity::WaitingForApproval(risk_category_title(request.category).to_owned());
                self.approval = self.make_approval(request);
            }
            Update::QuestionPending => self.timeline.activity = Activity::WaitingForAnswer,
            Update::AwaitingUnlock => self.timeline.activity = Activity::Locked,
            Update::Ignored => {}
            Update::EffectCompleted { effect, event } => {
                let is_act = matches!(effect, p::Effect::Act(_));
                match event {
                    p::Event::Observed(observation) => self.ingest_tree(observation),
                    p::Event::ObservedRunnerText(observation) => self.ingest_text(observation),
                    p::Event::ActResult(result) if is_act => {
                        self.complete(&result);
                        steps_touched = true;
                    }
                    // Stopped while the command was in flight: nothing is claimed about it.
                    p::Event::Cancel if is_act => {
                        self.mark_stopped();
                        steps_touched = true;
                    }
                    _ => {}
                }
                // The prompt's element list belongs to the decision that led to this command.
                if is_act {
                    self.sight.clear();
                }
            }
            Update::Finished(status) => {
                self.timeline.activity = Activity::Finished(status.into());
                let stopped = status == p::OutcomeStatus::Cancelled;
                for step in &mut self.timeline.steps {
                    if step.state == StepState::Running {
                        step.state = if stopped {
                            StepState::Stopped
                        } else {
                            StepState::Failed("Not finished".to_owned())
                        };
                        steps_touched = true;
                    }
                }
                self.pending_type = None;
            }
        }
        Change {
            steps: steps_touched || before_steps != self.steps_signature(),
            screens: before_screens != self.screens_signature(),
            activity: before_activity != self.timeline.activity,
            approval: had_approval || self.approval.is_some(),
        }
    }

    fn steps_signature(&self) -> (usize, Option<u32>, u32) {
        (
            self.timeline.steps.len(),
            self.timeline.steps.last().map(|s| s.number),
            self.timeline.dropped_steps,
        )
    }

    fn screens_signature(&self) -> (usize, Option<u32>, Option<u32>) {
        (
            self.timeline.screens.len(),
            self.timeline.screens.first().map(|s| s.id),
            self.timeline.current_screen_id,
        )
    }

    // MARK: Screens

    fn take_screen_id(&mut self) -> u32 {
        let id = self.next_screen_id;
        self.next_screen_id = self.next_screen_id.saturating_add(1);
        id
    }

    fn locked_screen(id: u32, size: ViewerSize) -> ViewerScreen {
        ViewerScreen {
            id,
            size,
            app_name: None,
            bundle_id: String::new(),
            source: ScreenSource::Locked,
            elements: Vec::new(),
            elements_truncated: false,
            frame_key: None,
            focused_input_secure: None,
        }
    }

    fn ingest_tree(&mut self, observation: p::Observation) {
        let id = self.take_screen_id();
        let p::Observation {
            screen,
            device_locked,
            ..
        } = observation;
        let size = ViewerSize::from(screen.size);
        if device_locked {
            self.forget_screen();
            self.focused_field = None;
            self.add_screen(Self::locked_screen(id, size));
            return;
        }
        let (elements, truncated) = wireframe(&screen.root, size, &mut self.labels);
        self.focused_field = focused_field(&screen.root, &mut self.labels);
        let focused_input_secure = focused_input_secure(&screen.root);
        let app_name = self.labels.label(screen.app_name.as_deref());
        let bundle_id = privacy::capped(&screen.app_bundle_id, MAX_BUNDLE_ID_CHARACTERS);
        self.remember_screen(screen, true);
        let frame_key = self.frame_key();
        self.add_screen(ViewerScreen {
            id,
            size,
            app_name,
            bundle_id,
            source: ScreenSource::Tree,
            elements,
            elements_truncated: truncated,
            frame_key,
            focused_input_secure,
        });
    }

    fn ingest_text(&mut self, observation: p::Observation) {
        let id = self.take_screen_id();
        self.forget_screen();
        self.focused_field = None;
        let screen = observation.screen;
        let size = ViewerSize::from(screen.size);
        if observation.device_locked {
            self.add_screen(Self::locked_screen(id, size));
            return;
        }
        let frame_key = self.frame_key();
        // The tree the session's own parser yields (the FFI parses with
        // `snapshot_to_observation`), drawn exactly like a typed tree's: the same element
        // cap, the same redaction of every label and placeholder, never a value, a lock
        // screen keeps nothing (above). Lines without a frame are not nodes, and a frame with
        // no area on the screen is not drawn. `source: runner_text` says the layout came
        // from the runner's text (preview review). What the text cannot vouch for stays off:
        // no focused-field evidence for typing captions, no `focused_input_secure`.
        let (elements, truncated) = wireframe(&screen.root, size, &mut self.labels);
        let app_name = self.labels.label(screen.app_name.as_deref());
        let bundle_id = privacy::capped(&screen.app_bundle_id, MAX_BUNDLE_ID_CHARACTERS);
        self.remember_screen(screen, false);
        self.add_screen(ViewerScreen {
            id,
            size,
            app_name,
            bundle_id,
            source: ScreenSource::RunnerText,
            elements,
            elements_truncated: truncated,
            frame_key,
            focused_input_secure: None,
        });
    }

    fn frame_key(&self) -> Option<FrameKey> {
        self.config.frames_enabled.then(|| FrameKey {
            task_id: self.timeline.task_id.clone(),
            observation: self.effect_number,
        })
    }

    fn add_screen(&mut self, screen: ViewerScreen) {
        self.timeline.current_screen_id = Some(screen.id);
        self.timeline.screens.push(screen);
        self.prune();
    }

    /// Keeps the screens the kept steps refer to and the current one.
    fn prune(&mut self) {
        let mut used: Vec<u32> = self
            .timeline
            .steps
            .iter()
            .filter_map(|s| s.screen_id)
            .collect();
        if let Some(current) = self.timeline.current_screen_id {
            used.push(current);
        }
        used.sort_unstable();
        used.dedup();
        self.timeline
            .screens
            .retain(|s| used.binary_search(&s.id).is_ok());
    }

    // MARK: Steps

    fn add_step(&mut self, command: p::RunnerCommand) {
        let number = self
            .timeline
            .dropped_steps
            .saturating_add(u32::try_from(self.timeline.steps.len()).unwrap_or(u32::MAX))
            .saturating_add(1);
        let (screen_id, size, source) = match self.timeline.current_screen() {
            Some(s) => (Some(s.id), s.size, Some(s.source.clone())),
            None => (None, ViewerSize::FALLBACK_PHONE, None),
        };
        let mut target = None;
        let mut target_field = None;
        let gesture;
        let caption;
        match command {
            p::RunnerCommand::Tap { x, y } => {
                let pt = ViewerPoint::new(x, y);
                gesture = Gesture::Tap(pt);
                if let Some((element, node)) = self
                    .tree
                    .as_ref()
                    .and_then(|t| target_node(pt, t, size, &mut self.labels))
                {
                    target_field = Some(field(node, &mut self.labels));
                    target = Some(element);
                }
                caption = pointer_caption("Tap", target.as_ref());
            }
            p::RunnerCommand::LongPress { x, y, duration_ms } => {
                let pt = ViewerPoint::new(x, y);
                gesture = Gesture::LongPress {
                    point: pt,
                    duration_ms,
                };
                if let Some((element, node)) = self
                    .tree
                    .as_ref()
                    .and_then(|t| target_node(pt, t, size, &mut self.labels))
                {
                    target_field = Some(field(node, &mut self.labels));
                    target = Some(element);
                }
                caption = pointer_caption("Press and hold", target.as_ref());
            }
            p::RunnerCommand::Swipe {
                from,
                to,
                duration_ms,
            } => {
                let a = ViewerPoint::from(from);
                let b = ViewerPoint::from(to);
                gesture = Gesture::Swipe {
                    from: a,
                    to: b,
                    duration_ms,
                };
                target = self
                    .tree
                    .as_ref()
                    .and_then(|t| target_node(a, t, size, &mut self.labels))
                    .map(|(e, _)| e);
                caption = swipe_caption(a, b);
            }
            p::RunnerCommand::TypeText { text: raw } => {
                let (hidden, plain, label) = self.typing_evidence(screen_id, source.as_ref());
                let mut hasher = DefaultHasher::new();
                raw.hash(&mut hasher);
                caption = type_caption(None, label.as_deref());
                self.pending_type = Some(PendingType {
                    number,
                    hidden,
                    plain,
                    label,
                    raw_count: raw.chars().count(),
                    raw_hash: hasher.finish(),
                });
                gesture = Gesture::Type(None);
            }
            p::RunnerCommand::PressKey { key } => {
                let name = key_name(key);
                gesture = Gesture::Key(name.to_owned());
                caption = if key == p::Key::Home {
                    "Go home".to_owned()
                } else {
                    format!("Press {name}")
                };
            }
            p::RunnerCommand::LaunchApp { bundle_id } => {
                let name = self.app_name(&bundle_id);
                caption = format!("Open {name}");
                gesture = Gesture::Launch(name);
            }
            p::RunnerCommand::OpenUrl { url } => {
                let link = privacy::link_of(&url);
                caption = link_caption(link.as_deref());
                gesture = Gesture::OpenUrl(link.unwrap_or_default());
            }
            p::RunnerCommand::Wait { ms } => {
                gesture = Gesture::Wait(ms);
                caption = wait_caption(ms);
            }
        }
        self.timeline.steps.push(ViewerStep {
            number,
            core_step: self.core_step,
            screen_id,
            gesture,
            target,
            caption,
            state: StepState::Running,
        });
        self.pointer_field = target_field.map(|field| PointerField {
            number,
            screen_id,
            field,
        });
        self.act_step = Some(number);
        let max = self.config.max_steps;
        if self.timeline.steps.len() > max {
            let drop = self.timeline.steps.len() - max;
            self.timeline.steps.drain(..drop);
            self.timeline.dropped_steps = self
                .timeline
                .dropped_steps
                .saturating_add(u32::try_from(drop).unwrap_or(u32::MAX));
            self.prune();
        }
    }

    /// What is known about the field text goes into. Hidden evidence (secure, or the core
    /// calls the field credential-like) wins; "plain" needs a typed tree naming an entry
    /// field (the field tapped just before on the same screen, or the focused one). The
    /// prompt's element list never makes a field plain: its line has no secure marker.
    fn typing_evidence(
        &mut self,
        screen_id: Option<u32>,
        source: Option<&ScreenSource>,
    ) -> (bool, bool, Option<String>) {
        let mut fields: Vec<&FieldEvidence> = Vec::with_capacity(2);
        if source == Some(&ScreenSource::Tree) {
            // A tap on a field just before, on the same screen, puts the focus there.
            if let (Some(previous), Some(tapped)) =
                (self.timeline.steps.last(), &self.pointer_field)
            {
                if tapped.number == previous.number
                    && tapped.screen_id == screen_id
                    && (tapped.field.input || tapped.field.hidden)
                {
                    fields.push(&tapped.field);
                }
            }
            if let Some(focused) = &self.focused_field {
                fields.push(focused);
            }
        }
        let mut hidden = fields.iter().any(|f| f.hidden);
        let mut prompt_label: Option<String> = None;
        for e in self
            .sight
            .iter()
            .filter(|e| e.is_focused() && (is_input_role(&e.role) || is_secure_role(&e.role)))
        {
            if prompt_field_hidden(e) {
                hidden = true;
            }
            if prompt_label.is_none() {
                prompt_label = self.labels.label(Some(&e.label));
            }
        }
        let plain = !hidden && fields.iter().any(|f| f.input);
        let label = fields
            .first()
            .and_then(|f| f.label.clone())
            .or(prompt_label);
        (hidden, plain, label)
    }

    /// Sets a pending typing step's text from the model's (or flow's) `type` action.
    /// Returns whether a step changed.
    fn resolve_pending(&mut self, action: p::Action) -> bool {
        let Some(pending) = self.pending_type.take() else {
            return false;
        };
        let p::Action::Type { element, text, .. } = action else {
            return false;
        };
        let Some(i) = self.timeline.step_index(pending.number) else {
            return false;
        };
        let mut hidden = pending.hidden;
        let mut plain = pending.plain;
        let mut label = pending.label;
        if let Some(e) = element.and_then(|index| self.sight.iter().find(|e| e.index == index)) {
            if prompt_field_hidden(e) {
                hidden = true;
            }
            if !is_input_role(&e.role) {
                plain = false;
            }
            // The field the core named for this action.
            if let Some(named) = self.labels.label(Some(&e.label)) {
                label = Some(named);
            }
        }
        // Shown only when it is exactly what was typed, or the core filled stored values
        // into placeholders (which are then masked), and the core's redaction would not
        // change a character of it.
        let consistent = privacy::has_placeholder(&text) || {
            let mut hasher = DefaultHasher::new();
            text.hash(&mut hasher);
            text.chars().count() == pending.raw_count && hasher.finish() == pending.raw_hash
        };
        let shown = if plain && !hidden && consistent && privacy::is_unchanged_by_redaction(&text) {
            privacy::typed_text(&text, false)
        } else {
            None
        };
        let Some(step) = self.timeline.steps.get_mut(i) else {
            return false;
        };
        step.caption = type_caption(shown.as_deref(), label.as_deref());
        step.gesture = Gesture::Type(shown);
        true
    }

    /// The command in flight when the run was stopped.
    fn mark_stopped(&mut self) {
        let Some(i) = self.act_step.and_then(|n| self.timeline.step_index(n)) else {
            return;
        };
        if let Some(step) = self.timeline.steps.get_mut(i) {
            if step.state == StepState::Running {
                step.state = StepState::Stopped;
            }
        }
    }

    fn complete(&mut self, result: &p::RunnerResult) {
        let Some(i) = self.act_step.and_then(|n| self.timeline.step_index(n)) else {
            return;
        };
        let Some(step) = self.timeline.steps.get_mut(i) else {
            return;
        };
        step.state = if result.ok {
            StepState::Done
        } else if result.failure == Some(p::RunnerFailure::DeviceLocked) {
            StepState::Failed("The phone was locked".to_owned())
        } else {
            // The runner's own words are not kept: they may quote typed text.
            StepState::Failed("The phone did not do it".to_owned())
        };
    }

    /// The app's display name when Jarvis knows it, else the bundle id.
    fn app_name(&self, bundle_id: &str) -> String {
        match self
            .config
            .known_apps
            .iter()
            .find(|a| a.bundle_id == bundle_id)
        {
            Some(app) if !app.name.is_empty() => app.name.clone(),
            _ => privacy::capped(bundle_id, 64),
        }
    }
}

// MARK: - Tree helpers

fn is_secure_node(node: &p::UiNode) -> bool {
    node.secure || node.role == p::Role::SecureTextField
}

/// Whether `node` is shown on the screen (`bounds`), and where (clipped).
fn shown_frame(node: &p::UiNode, bounds: &ViewerRect) -> Option<ViewerRect> {
    if matches!(node.role, p::Role::Application | p::Role::Window) || !node.visible {
        return None;
    }
    ViewerRect::from(node.frame).intersection(bounds)
}

/// `node` as a wireframe element (its frame already clipped).
fn make_element(
    node: &p::UiNode,
    frame: ViewerRect,
    depth: u32,
    labels: &mut LabelCache,
) -> ViewerElement {
    // Label, else placeholder (both through the core's redaction); never the value.
    let label = labels
        .label(node.label.as_deref())
        .or_else(|| labels.label(node.placeholder.as_deref()));
    ViewerElement {
        frame,
        role: role_name(&node.role),
        kind: kind(&node.role),
        label,
        secure: is_secure_node(node),
        depth,
    }
}

fn screen_bounds(size: ViewerSize) -> ViewerRect {
    let u = size.usable();
    ViewerRect::new(0.0, 0.0, u.width, u.height)
}

/// The shown elements of `root` in drawing order (parents first), clipped to the screen:
/// visible, with a finite positive area on the screen; the application and window nodes
/// (the whole screen) are left out. At most [`MAX_ELEMENTS`]; labels are computed only for
/// the elements kept.
pub fn wireframe(
    root: &p::UiNode,
    size: ViewerSize,
    labels: &mut LabelCache,
) -> (Vec<ViewerElement>, bool) {
    let bounds = screen_bounds(size);
    let mut out = Vec::new();
    let mut truncated = false;
    let mut visited = 0usize;
    // Iterative pre-order: a deep tree cannot overflow the stack.
    let mut stack: Vec<(&p::UiNode, u32)> = vec![(root, 0)];
    while let Some((node, depth)) = stack.pop() {
        visited += 1;
        if visited > MAX_VISITED_NODES {
            truncated = true;
            break;
        }
        let mut child_depth = depth;
        if let Some(frame) = shown_frame(node, &bounds) {
            if out.len() < MAX_ELEMENTS {
                out.push(make_element(node, frame, depth, labels));
            } else {
                truncated = true;
            }
            child_depth = depth.saturating_add(1);
        }
        stack.extend(node.children.iter().rev().map(|c| (c, child_depth)));
    }
    (out, truncated)
}

/// Whether the focused input is secure; `None` when no input is focused.
pub fn focused_input_secure(root: &p::UiNode) -> Option<bool> {
    let mut found = None;
    let mut visited = 0usize;
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if visited >= MAX_VISITED_NODES {
            break;
        }
        visited += 1;
        if node.focused {
            if is_secure_node(node) {
                return Some(true);
            }
            if kind(&node.role) == ElementKind::Input {
                found = Some(false);
            }
        }
        stack.extend(node.children.iter());
    }
    found
}

/// What the tree says about `node` as a field typing may go into; the core decides
/// whether its value is hidden.
fn field(node: &p::UiNode, labels: &mut LabelCache) -> FieldEvidence {
    let hidden = privacy::field_value_hidden(
        &node.role,
        node.label.as_deref(),
        node.placeholder.as_deref(),
        node.identifier.as_deref(),
        is_secure_node(node),
    );
    FieldEvidence {
        hidden,
        input: kind(&node.role) == ElementKind::Input,
        label: labels
            .label(node.label.as_deref())
            .or_else(|| labels.label(node.placeholder.as_deref())),
    }
}

/// The focused field of the tree: a hidden one wins; `None` when no entry field (or secure
/// node) is focused.
fn focused_field(root: &p::UiNode, labels: &mut LabelCache) -> Option<FieldEvidence> {
    let mut found: Option<FieldEvidence> = None;
    let mut visited = 0usize;
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if visited >= MAX_VISITED_NODES {
            break;
        }
        visited += 1;
        if node.focused && (node.secure || kind(&node.role) == ElementKind::Input) {
            let evidence = field(node, labels);
            if evidence.hidden {
                return Some(evidence);
            }
            if found.is_none() {
                found = Some(evidence);
            }
        }
        stack.extend(node.children.iter());
    }
    found
}

/// The deepest shown element (topmost among equals) whose frame contains `point`, with its
/// tree node; `None` when there is none. Display only.
pub fn target_node<'t>(
    point: ViewerPoint,
    root: &'t p::UiNode,
    size: ViewerSize,
    labels: &mut LabelCache,
) -> Option<(ViewerElement, &'t p::UiNode)> {
    if !point.is_finite() {
        return None;
    }
    let bounds = screen_bounds(size);
    let mut best: Option<(&p::UiNode, ViewerRect, u32)> = None;
    let mut visited = 0usize;
    let mut stack: Vec<(&p::UiNode, u32)> = vec![(root, 0)];
    while let Some((node, depth)) = stack.pop() {
        if visited >= MAX_VISITED_NODES {
            break;
        }
        visited += 1;
        let mut child_depth = depth;
        if let Some(clipped) = shown_frame(node, &bounds) {
            child_depth = depth.saturating_add(1);
            // Pre-order visits later siblings later, so `>=` keeps the one drawn on top.
            let deeper = best.as_ref().is_none_or(|(_, _, d)| depth >= *d);
            if ViewerRect::from(node.frame).contains(point) && deeper {
                best = Some((node, clipped, depth));
            }
        }
        stack.extend(node.children.iter().rev().map(|c| (c, child_depth)));
    }
    best.map(|(node, frame, depth)| (make_element(node, frame, depth, labels), node))
}

// MARK: - Roles and captions

/// The roles of entry fields, as the core names them.
pub const INPUT_ROLES: [&str; 4] = ["TextField", "SecureTextField", "TextView", "SearchField"];

fn is_input_role(role: &str) -> bool {
    INPUT_ROLES.contains(&role)
}

fn is_secure_role(role: &str) -> bool {
    role == "SecureTextField"
}

/// Whether the core hides the value of a field the prompt names (its role and label as the
/// core rendered them).
fn prompt_field_hidden(e: &PromptElement) -> bool {
    is_secure_role(&e.role)
        || privacy::field_value_hidden(
            &role_named(&e.role),
            Some(&e.label),
            e.placeholder.as_deref(),
            None,
            false,
        )
}

/// The roles with a fixed name.
const NAMED_ROLES: [(p::Role, &str); 26] = [
    (p::Role::Button, "Button"),
    (p::Role::Link, "Link"),
    (p::Role::TextField, "TextField"),
    (p::Role::SecureTextField, "SecureTextField"),
    (p::Role::TextView, "TextView"),
    (p::Role::SearchField, "SearchField"),
    (p::Role::StaticText, "StaticText"),
    (p::Role::Image, "Image"),
    (p::Role::Cell, "Cell"),
    (p::Role::Switch, "Switch"),
    (p::Role::Slider, "Slider"),
    (p::Role::Picker, "Picker"),
    (p::Role::Tab, "Tab"),
    (p::Role::NavigationBar, "NavigationBar"),
    (p::Role::TabBar, "TabBar"),
    (p::Role::Toolbar, "Toolbar"),
    (p::Role::Alert, "Alert"),
    (p::Role::Sheet, "Sheet"),
    (p::Role::Keyboard, "Keyboard"),
    (p::Role::Key, "Key"),
    (p::Role::ScrollView, "ScrollView"),
    (p::Role::Table, "Table"),
    (p::Role::CollectionView, "CollectionView"),
    (p::Role::WebView, "WebView"),
    (p::Role::Window, "Window"),
    (p::Role::Application, "Application"),
];

/// The role a prompt line names (`TextField` → `TextField`), matched exactly; anything
/// else is `Other`.
pub fn role_named(name: &str) -> p::Role {
    NAMED_ROLES
        .iter()
        .find(|(_, n)| *n == name)
        .map_or_else(|| p::Role::Other(name.to_owned()), |(r, _)| r.clone())
}

/// The role's display name (`Button`); another role's own name, capped at 32.
pub fn role_name(role: &p::Role) -> String {
    match role {
        p::Role::Other(name) => privacy::capped(if name.is_empty() { "Other" } else { name }, 32),
        known => NAMED_ROLES
            .iter()
            .find(|(r, _)| r == known)
            .map_or_else(|| "Other".to_owned(), |(_, n)| (*n).to_owned()),
    }
}

/// How an element of `role` is tinted.
pub fn kind(role: &p::Role) -> ElementKind {
    use p::Role as R;
    match role {
        R::Button | R::Link | R::Cell | R::Switch | R::Slider | R::Picker | R::Tab | R::Key => {
            ElementKind::Control
        }
        R::TextField | R::SecureTextField | R::TextView | R::SearchField => ElementKind::Input,
        R::StaticText => ElementKind::Text,
        R::Image => ElementKind::Image,
        R::Alert | R::Sheet => ElementKind::Alert,
        R::Keyboard => ElementKind::Keyboard,
        R::NavigationBar
        | R::TabBar
        | R::Toolbar
        | R::ScrollView
        | R::Table
        | R::CollectionView
        | R::WebView
        | R::Window
        | R::Application
        | R::Other(_) => ElementKind::Container,
    }
}

/// "Tap “Send”", or the verb alone.
pub fn pointer_caption(verb: &str, target: Option<&ViewerElement>) -> String {
    match target.and_then(|t| t.label.as_deref()) {
        Some(label) => format!("{verb} \u{201C}{label}\u{201D}"),
        None => verb.to_owned(),
    }
}

/// "Type “hello”" when the text may be shown, else "Type into “Email”" when the field's
/// label is known, else "Type text".
pub fn type_caption(shown: Option<&str>, into: Option<&str>) -> String {
    if let Some(shown) = shown {
        return format!("Type \u{201C}{shown}\u{201D}");
    }
    if let Some(label) = into {
        return format!("Type into \u{201C}{label}\u{201D}");
    }
    "Type text".to_owned()
}

/// "Open example.com", "Open a tel: link", "Open a link".
pub fn link_caption(link: Option<&str>) -> String {
    match link {
        None | Some("") => "Open a link".to_owned(),
        Some(link) if link.ends_with(':') => format!("Open a {link} link"),
        Some(link) => format!("Open {link}"),
    }
}

/// "Swipe up/down/left/right", "Swipe" for a drag of half a point or less.
pub fn swipe_caption(a: ViewerPoint, b: ViewerPoint) -> String {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    if !(dx.is_finite() && dy.is_finite() && (dx.abs() > 0.5 || dy.abs() > 0.5)) {
        return "Swipe".to_owned();
    }
    if dy.abs() >= dx.abs() {
        return if dy < 0.0 { "Swipe up" } else { "Swipe down" }.to_owned();
    }
    if dx < 0.0 {
        "Swipe left"
    } else {
        "Swipe right"
    }
    .to_owned()
}

/// "Wait 300 ms", "Wait 2 s", "Wait 1.5 s".
pub fn wait_caption(ms: u64) -> String {
    if ms < 1_000 {
        return format!("Wait {ms} ms");
    }
    if ms.is_multiple_of(1_000) {
        return format!("Wait {} s", ms / 1_000);
    }
    #[allow(clippy::cast_precision_loss)]
    let seconds = ms as f64 / 1_000.0;
    format!("Wait {seconds:.1} s")
}

/// The key's name.
pub fn key_name(key: p::Key) -> &'static str {
    match key {
        p::Key::Return => "Return",
        p::Key::Delete => "Delete",
        p::Key::Tab => "Tab",
        p::Key::Space => "Space",
        p::Key::Escape => "Escape",
        p::Key::Home => "Home",
    }
}

/// Short owner-facing name of a risk category (the chip on confirmation sheets).
pub fn risk_category_title(category: p::RiskCategory) -> &'static str {
    use p::RiskCategory as C;
    match category {
        C::None => "Action",
        C::SendMessage => "Send message",
        C::Payment => "Payment",
        C::Purchase => "Purchase",
        C::Delete => "Delete",
        C::PublicPost => "Public post",
        C::AccountChange => "Account change",
        C::SystemSetting => "System setting",
        C::Credential => "Credentials",
        C::Unverified => "Unreadable control",
        C::Call => "Phone call",
        C::FirstUse => "First time in this app",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captions() {
        assert_eq!(wait_caption(300), "Wait 300 ms");
        assert_eq!(wait_caption(1_500), "Wait 1.5 s");
        assert_eq!(wait_caption(2_000), "Wait 2 s");
        assert_eq!(wait_caption(1_050), "Wait 1.1 s");
        assert_eq!(link_caption(Some("tel:")), "Open a tel: link");
        assert_eq!(role_named("TextField"), p::Role::TextField);
        assert_eq!(role_named("textfield"), p::Role::Other("textfield".into()));
        assert_eq!(role_name(&p::Role::Other(String::new())), "Other");
        assert_eq!(role_name(&p::Role::SecureTextField), "SecureTextField");
    }
}
