//! Confirmation gates.

use jarvis_proto::{Action, CompactScreen, Direction, Policy, RiskCategory, Role, TrustLevel};

/// What to do with an action of a given risk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// Run it.
    Allow,
    /// Ask the user first.
    Confirm,
    /// Never run it.
    Deny,
}

/// Categories a `Trusted` app may run without asking even when they are in
/// `policy.confirm`: changing a setting inside the app and tapping a control the phrase
/// net cannot read. Everything else — `Payment`, `Purchase`, `Delete`, `Credential`,
/// `AccountChange`, `SendMessage`, `PublicPost` and `Call` — still follows `confirm`.
pub const TRUSTED_ALLOWED: &[RiskCategory] =
    &[RiskCategory::SystemSetting, RiskCategory::Unverified];

/// Categories only the phone itself may approve (never a remote device): money, purchases,
/// credentials, account changes, deletions, and controls the phrase net could not read
/// (`jarvis_remote::NEVER_REMOTE_APPROVE` is this list). Review pass 29 (M1): a request that
/// carries any of them beside the category it shows is asked phone only (remote
/// approvability is the AND over every carried category).
pub const NEVER_REMOTE_APPROVE: [RiskCategory; 6] = [
    RiskCategory::Payment,
    RiskCategory::Purchase,
    RiskCategory::Credential,
    RiskCategory::AccountChange,
    RiskCategory::Delete,
    RiskCategory::Unverified,
];

/// The gate for `category` under `policy`: `None` is allowed, categories in
/// `policy.deny_categories` are denied, categories in `policy.confirm` need confirmation,
/// others are allowed.
pub fn gate(policy: &Policy, category: RiskCategory) -> Gate {
    if category == RiskCategory::None {
        Gate::Allow
    } else if policy.denies_category(category) {
        Gate::Deny
    } else if policy.requires_confirmation(category) {
        Gate::Confirm
    } else {
        Gate::Allow
    }
}

/// [`gate`] for an action of `category` in an app of trust `level`:
///
/// * `Denied`: [`Gate::Deny`].
/// * `StepByStep`: denied categories are denied; every action that [`changes_state`]
///   needs confirmation; the rest follows [`gate`].
/// * `Trusted`: denied categories are denied; [`TRUSTED_ALLOWED`] categories are
///   allowed; the rest follows [`gate`].
/// * `Guarded`: [`gate`].
pub fn gate_in(
    policy: &Policy,
    category: RiskCategory,
    level: TrustLevel,
    action: &Action,
    screen: &CompactScreen,
) -> Gate {
    match level {
        TrustLevel::Denied => Gate::Deny,
        TrustLevel::StepByStep => match gate(policy, category) {
            Gate::Deny => Gate::Deny,
            _ if changes_state(action, screen) => Gate::Confirm,
            other => other,
        },
        TrustLevel::Trusted => match gate(policy, category) {
            Gate::Confirm if TRUSTED_ALLOWED.contains(&category) => Gate::Allow,
            other => other,
        },
        TrustLevel::Guarded => gate(policy, category),
    }
}

/// [`gate_in`] plus the **screen-source rule** (sixth review): when the screen the action is
/// judged on came from the third-party runner's text format (`runner_text`, which on-screen
/// text can forge), every action that [`changes_state`] is confirmed, whatever the policy
/// mode, the app's trust level (a `Trusted` app's [`TRUSTED_ALLOWED`] categories included)
/// or the categories the owner relaxed. Only a denial (a deny-listed app, a denied
/// category) takes precedence, and refuses. This is a rule on the source, not on a
/// category: no trust level or relaxed category can let such an action through unasked.
pub fn gate_on(
    policy: &Policy,
    category: RiskCategory,
    level: TrustLevel,
    action: &Action,
    screen: &CompactScreen,
    runner_text: bool,
) -> Gate {
    match gate_in(policy, category, level, action, screen) {
        Gate::Deny => Gate::Deny,
        _ if runner_text && changes_state(action, screen) => Gate::Confirm,
        other => other,
    }
}

/// [`gate_in`] plus the screen-source rule (as [`gate_on`]; review pass 31: `source_rule` is
/// decided by the caller, `crate::evaluate::source_rule` — on runner text every action that
/// [`changes_state`], on a screen not read completely every action that
/// [`changes_incomplete_screen`]) plus the **unclear rule** (fourteenth review, Rule C): a request whose
/// category the gate guessed (`unclear`: a presumed submit, a send-by-label whose form may
/// hold an amount or names no verified recipient, a link the owner did not give) is
/// confirmed whatever the trust level or the categories the owner relaxed — a guess is not
/// relaxable. Only a denial (a deny-listed app, a denied category) takes precedence, and
/// an owner who confirms nothing at all (`policy.confirm` empty) keeps that meaning for
/// what is merely unclear (no such request asks except by the screen-source rule).
///
/// Seventeenth review: `forced` — the request carries content to another place that Rule T
/// leaves to this rule (a link the owner did not give after something was read; an insert
/// or hand-on control of an edit menu): it is confirmed whatever the policy relaxes, an
/// empty `confirm` list included. Only a denial refuses first. The one decision point the
/// session's gate (`evaluate_owned`) uses.
#[allow(clippy::too_many_arguments)]
pub fn gate_owned(
    policy: &Policy,
    category: RiskCategory,
    level: TrustLevel,
    action: &Action,
    screen: &CompactScreen,
    source_rule: bool,
    unclear: bool,
    forced: bool,
) -> Gate {
    let gated = match gate_in(policy, category, level, action, screen) {
        Gate::Deny => Gate::Deny,
        _ if source_rule => Gate::Confirm,
        other => other,
    };
    match gated {
        Gate::Allow if forced => Gate::Confirm,
        Gate::Allow if unclear && category != RiskCategory::None && !policy.confirm.is_empty() => {
            Gate::Confirm
        }
        other => other,
    }
}

/// Whether `action` can change state: everything except `Wait`, `Look`, `Done`, `Fail`,
/// `AskUser` and a plain vertical `Scroll` (one whose target, if any, is not a picker,
/// slider or switch, where dragging changes the value). `PressHome`, `OpenApp`, `OpenUrl`,
/// `CallTool`, focusing taps and sideways scrolls all count.
pub fn changes_state(action: &Action, screen: &CompactScreen) -> bool {
    match action {
        Action::Wait { .. }
        | Action::Look
        | Action::Done { .. }
        | Action::Fail { .. }
        | Action::AskUser { .. } => false,
        Action::Scroll { direction, element } => {
            let vertical = matches!(direction, Direction::Up | Direction::Down);
            let value_control = element
                .and_then(|i| screen.get(i))
                .is_some_and(|e| matches!(e.role, Role::Picker | Role::Slider | Role::Switch));
            !vertical || value_control
        }
        _ => true,
    }
}

/// Review pass 31 (M1): whether `action`, judged on a screen Jarvis could not read completely
/// (`ScreenSource::Incomplete`), counts as changing state there: every gesture that touches
/// the screen — a scroll in any direction (an element-less vertical one too: its drag may
/// cross a picker or a slider that was not read), a swipe, a tap, `tap_point`, a long
/// press, typing, clearing, Return — and `open_app`, `open_url` and tool calls. Not
/// `wait`, `look`, `done`, `fail`, `ask_user`, and not `press_home`: Home is the way off such
/// a screen and touches nothing on it (it is gated as on any screen, Rule T included).
pub fn changes_incomplete_screen(action: &Action) -> bool {
    !matches!(
        action,
        Action::Wait { .. }
            | Action::Look
            | Action::Done { .. }
            | Action::Fail { .. }
            | Action::AskUser { .. }
            | Action::PressHome
    )
}
