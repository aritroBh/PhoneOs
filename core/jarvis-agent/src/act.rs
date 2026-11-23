//! The one place a runner command leaves the session, and the "Jarvis typed here" state.
//!
//! **Why construction elsewhere is impossible** (thirteenth review, L3). Every handler of
//! [`crate::session::Session`] returns an [`Out`], not an `Effect`. `Out`'s only field is
//! private to this module, and the constructors this module offers build every effect
//! *but* `Effect::Act`: the one function that builds an `Out` carrying a runner command is
//! [`TypedState::emit`], which records the "typed here" state before it lets a `TypeText`
//! go. A handler cannot name an `Out` holding a runner command any other way: not a struct
//! literal (the field is private), not a conversion (there is no `From<Effect>`), not a
//! deserialisation (`Out` has none). Only `Session::step`, the session's public entry,
//! turns an `Out` into an `Effect` ([`Out::into_effect`]). The source scan
//! (`tests/emit_scan.rs`) remains for what types cannot say: that `Effect::Act` is not
//! named outside this module (so `step` cannot build one itself) and that `step` returns
//! only what a handler gave it.
//!
//! **The "typed here" state** ([`TypedState`]). Per app (lower-cased bundle id; an
//! observation without one is keyed [`UNKNOWN_APP`], which every app reads), what Jarvis
//! typed during the task; non-empty is the state the gate reads (`presumed_submit`). It is
//! set by two triggers and never cleared during the task:
//! - **sent** (twelfth review): a `TypeText` passing through [`TypedState::emit`], before
//!   anything verifies it;
//! - **observed** (thirteenth review): an observation, once Jarvis has sent the runner any
//!   command but a wait in this task, in which some text input of the app holds a value
//!   (compared as a multiset of value marks, a secure field by its length only:
//!   `jarvis_observe::CompactView::input_marks`) that the previous observation of that app
//!   in this task did not hold — or the first observation of an app Jarvis brought up.
//!   Paste, AutoFill, dictation, keyboard suggestions, "Replace…", drag and drop and a
//!   link that fills a form are covered without naming them; the label of the field may
//!   change meanwhile (tenth review), only values are compared.

use std::borrow::Cow;
use std::collections::BTreeMap;

use jarvis_proto::{
    Action, ConfirmRequest, Effect, Expect, Key, ModelRequest, Outcome, RunnerCommand, ToolCall,
};
use jarvis_safety::TypedText;

use crate::state::Pending;

/// What a session handler leaves for the shell (see the module docs): any effect, but one
/// carrying a runner command only from [`TypedState::emit`].
#[derive(Debug)]
pub(crate) struct Out(Effect);

impl Out {
    /// `Effect::Observe`.
    pub(crate) fn observe(screenshot: bool) -> Out {
        Out(Effect::Observe { screenshot })
    }

    /// `Effect::Infer`.
    pub(crate) fn infer(request: ModelRequest) -> Out {
        Out(Effect::Infer(request))
    }

    /// `Effect::CallTool`.
    pub(crate) fn call_tool(call: ToolCall) -> Out {
        Out(Effect::CallTool(call))
    }

    /// `Effect::Confirm`.
    pub(crate) fn confirm(request: ConfirmRequest) -> Out {
        Out(Effect::Confirm(request))
    }

    /// `Effect::AskUser`.
    pub(crate) fn ask_user(question: String) -> Out {
        Out(Effect::AskUser { question })
    }

    /// `Effect::Finished`.
    pub(crate) fn finished(outcome: Outcome) -> Out {
        Out(Effect::Finished(outcome))
    }

    /// `Effect::AwaitUnlock`.
    pub(crate) fn await_unlock(reason: String, waited_ms: u64) -> Out {
        Out(Effect::AwaitUnlock { reason, waited_ms })
    }

    /// The effect, for `Session::step` alone.
    pub(crate) fn into_effect(self) -> Effect {
        self.0
    }
}

/// The key of typing sent while the app in front had no bundle id: it applies to every app.
pub(crate) const UNKNOWN_APP: &str = "\u{0}unknown-app";

/// The key of an app in [`TypedState`]. An observation without a bundle id is keyed
/// [`UNKNOWN_APP`], whose entries every app reads (twelfth review: fail closed).
fn app_key(bundle_id: &str) -> String {
    let key = bundle_id.trim().to_lowercase();
    if key.is_empty() {
        UNKNOWN_APP.to_owned()
    } else {
        key
    }
}

/// The "Jarvis typed here" state and the one function that sends runner commands (see the
/// module docs).
#[derive(Debug, Clone, Default)]
pub(crate) struct TypedState {
    /// Per app: what the session typed into which field (placeholders kept): the only field
    /// values a confirmation summary may quote besides amounts. An observed fill adds an
    /// entry with no field and no text (it marks the state, it quotes nothing).
    typed: BTreeMap<String, Vec<TypedText>>,
    /// Per app: the input marks of its latest observation in this task, sorted.
    seen: BTreeMap<String, Vec<u64>>,
    /// The session has sent the runner a command other than a wait in this task.
    acted: bool,
}

impl TypedState {
    /// Sends `cmd`: **the one place a runner command leaves the session** (twelfth review;
    /// thirteenth review: enforced by [`Out`]). A `TypeText` sets the "typed here" state for
    /// the app in front (`front`) **when it is sent**, before anything verifies it: whatever
    /// happens next — the text checks out, differs and survives the one retry, Return is
    /// held, the runner fails or reports a lock, the step is cancelled — the text may be in
    /// the app. Every `TypeText` reaches the runner through here: the model's `type`, the
    /// corrective retry, skill and learned-flow steps. Any command but a wait also arms the
    /// observation trigger ([`Self::observed`]).
    pub(crate) fn emit(
        &mut self,
        cmd: RunnerCommand,
        pending: Option<&Pending>,
        front: &str,
    ) -> Out {
        if !matches!(cmd, RunnerCommand::Wait { .. }) {
            self.acted = true;
        }
        if matches!(cmd, RunnerCommand::TypeText { .. }) {
            self.typed_sent(pending, front);
        }
        let expect = expectation(&cmd, front);
        let cmd = cmd.with_expect(expect);
        Out(Effect::Act(cmd))
    }

    /// Sets the state for a `TypeText` being sent (see [`Self::emit`]): under the app in
    /// front (the observation the command was planned on) and, if different, the app of the
    /// screen the step was chosen on. The entry names the target field (the check's
    /// fingerprint, else the target's or the focused input's on the screen the step was
    /// chosen on; `0` when none is known) and the text as decided (placeholders kept; empty
    /// for a secure field). A later verified typing replaces it with the field found after
    /// typing ([`Self::remember`]).
    fn typed_sent(&mut self, pending: Option<&Pending>, front: &str) {
        let (field, text, before_app) = match pending {
            Some(p) => {
                let target = match &p.action {
                    Action::Type {
                        element: Some(i), ..
                    } => p.before.get(*i),
                    _ => None,
                }
                .or_else(|| {
                    p.before
                        .elements
                        .iter()
                        .find(|e| e.focused && e.role.is_text_input())
                });
                let field = p
                    .check
                    .as_ref()
                    .and_then(|c| c.field)
                    .or_else(|| target.map(|e| e.fingerprint))
                    .unwrap_or(0);
                let secure = target.is_some_and(|e| e.secure);
                let text = match &p.action {
                    Action::Type { text, .. } if !secure => text.clone(),
                    _ => String::new(),
                };
                (field, text, Some(p.before.app_bundle_id.clone()))
            }
            None => (0, String::new(), None),
        };
        let mut keys = vec![app_key(front)];
        if let Some(app) = before_app {
            keys.push(app_key(&app));
        }
        keys.sort();
        keys.dedup();
        for key in keys {
            self.mark(key, field, &text);
        }
    }

    /// Adds `(field, text)` to `key`'s entry unless it is there.
    fn mark(&mut self, key: String, field: u64, text: &str) {
        let here = self.typed.entry(key).or_default();
        if !here.iter().any(|t| t.field == field && t.text == text) {
            here.push(TypedText {
                field,
                text: text.to_owned(),
            });
        }
    }

    /// Remembers that the session typed `pending`'s text into the field `field`
    /// (placeholders kept) in the app `front`: summaries may then quote that field's value
    /// while it is exactly that text, and the app's state is set (tenth review).
    pub(crate) fn remember(&mut self, pending: &Pending, field: Option<u64>, front: &str) {
        let (Some(field), Action::Type { text, .. }) = (field, &pending.action) else {
            return;
        };
        if text.trim().is_empty() {
            return;
        }
        let here = self.typed.entry(app_key(front)).or_default();
        here.retain(|t| t.field != field);
        here.push(TypedText {
            field,
            text: text.clone(),
        });
    }

    /// Thirteenth review (H1): an observation of app `app` whose text inputs hold `marks`
    /// (`jarvis_observe::CompactView::input_marks`). Once the session has acted in this task,
    /// a mark this app's previous observation did not hold (as a multiset: a second field
    /// filled with the same text counts) sets the app's state; the first observation of an
    /// app compares with nothing, so any filled input sets it then. Before Jarvis acted,
    /// observations only record (text present when the task starts is not Jarvis's: the
    /// documented per-task gap).
    pub(crate) fn observed(&mut self, app: &str, mut marks: Vec<u64>) {
        marks.sort_unstable();
        let key = app_key(app);
        let before = self.seen.get(&key).map_or(&[][..], Vec::as_slice);
        let entered = self.acted && !sorted_subset(&marks, before);
        if entered {
            self.mark(key.clone(), 0, "");
        }
        self.seen.insert(key, marks);
    }

    /// What the session typed in app `front` (see [`TypedState`]); non-empty is the "Jarvis
    /// typed here" state the gate reads. Typing recorded under [`UNKNOWN_APP`] counts for
    /// every app (twelfth review).
    pub(crate) fn here(&self, front: &str) -> Cow<'_, [TypedText]> {
        let key = app_key(front);
        let here = self.typed.get(&key).map_or(&[][..], Vec::as_slice);
        match self.typed.get(UNKNOWN_APP).filter(|_| key != UNKNOWN_APP) {
            Some(unknown) if !unknown.is_empty() => {
                Cow::Owned(here.iter().chain(unknown).cloned().collect())
            }
            _ => Cow::Borrowed(here),
        }
    }
}

/// Stale-screen guard, layer 2: what `cmd` expects of the phone, filled here, where every
/// command leaves the session. Gestures and typing (Return and Delete included) expect the
/// app of the observation they were planned (or re-checked) on, `front`, to be in front, so
/// a runner that reads another app in front refuses them (`RunnerFailure::Stale`); the
/// later commands of one plan (the focus tap's typing, a clear's deletes) carry it too.
/// Home, app launches, links and waits change or ignore the front app and expect nothing;
/// so does a command planned on an observation without a bundle id.
fn expectation(cmd: &RunnerCommand, front: &str) -> Option<Expect> {
    let front = front.trim();
    let gesture = match cmd {
        RunnerCommand::PressKey { key, .. } => *key != Key::Home,
        RunnerCommand::Tap { .. }
        | RunnerCommand::LongPress { .. }
        | RunnerCommand::Swipe { .. }
        | RunnerCommand::TypeText { .. } => true,
        RunnerCommand::LaunchApp { .. }
        | RunnerCommand::OpenUrl { .. }
        | RunnerCommand::Wait { .. } => false,
    };
    (gesture && !front.is_empty()).then(|| Expect {
        bundle_id: front.to_owned(),
    })
}

/// Whether the sorted multiset `a` is contained in the sorted multiset `b`.
fn sorted_subset(a: &[u64], b: &[u64]) -> bool {
    let mut j = 0;
    for x in a {
        while b.get(j).is_some_and(|y| y < x) {
            j += 1;
        }
        if b.get(j) != Some(x) {
            return false;
        }
        j += 1;
    }
    true
}
