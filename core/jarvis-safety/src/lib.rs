//! Safety layer for the Jarvis core: app deny list and trust levels, first-use targets, risk
//! classification, confirmation gates (Guarded / StepByStep / Trusted), budgets,
//! screen-text sanitising, redaction and action validation.
//!
//! Screen content is untrusted data. Everything here is pure, deterministic and panic-free.
//! The agent calls [`evaluate`] for every proposed action, [`sanitize_screen_text`] and
//! [`redact`] on every piece of screen text that enters a prompt or log, and keeps a
//! [`Budget`] per task.
//!
//! # What this layer does and does not protect against
//!
//! Two kinds of control live here. **Structural controls** do not depend on reading labels:
//! the per-app deny list and trust levels ([`app_level`]), first-use approval of every new
//! app ([`first_use_app`], enforced by the agent), `StepByStep` mode, which confirms every
//! action that [`changes_state`], categories that are never executed
//! (`Policy::deny_categories`), URL scheme checks (tool arguments included) and budgets.
//! A **keyword net** reads English labels and nearby text through phrase tables plus a few
//! app-specific rules to catch common blunders — tapping Pay, Send, Delete or Call, typing
//! a password — and treats what it cannot read (other scripts, common non-English words
//! and accented letters on controls, unlabeled or symbol-named buttons, tools without a
//! spec) as `Unverified`, which asks by default. Untrusted screen text is kept from posing
//! as prompt structure or leaking obvious secrets into prompts and logs.
//!
//! It does **not** understand what a control really does. In `Guarded` mode, inside an
//! app the owner has approved, an English-labelled control with no risky word or context
//! (a "Next" that actually pays) runs without asking; only `StepByStep` closes that gap,
//! at the cost of a prompt per step. The foreign-word table is small, so many non-English
//! labels still read as harmless. `OpenUrl` and tools that declare no app are not
//! first-use gated, because their target app is unknown. Sanitising breaks known delimiter
//! and chat-template forms but cannot stop a model from *following* injected prose;
//! redaction is pattern-based and will miss secrets in unusual formats.
//!
//! The real boundary is the operating system: Face ID / passcode prompts, the Apple Pay
//! sheet, iOS's own call and profile-install confirmations, and the deny list keeping the
//! agent out of Passwords, Wallet and banking apps. Nothing here replaces them.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod actuation;
mod app;
mod approval;
mod budget;
mod classify;
mod confine;
mod evaluate;
mod gate;
pub mod gestures;
mod hit;
mod phrases;
mod redact;
mod sanitize;
mod text;
mod validate;

pub use app::{
    app_level, check_app, check_app_name, denied_app_names, is_springboard, label_names_denied_app,
    matching_rule, mode_level, search_names_denied_app, widget_names_denied_app, Verdict,
};
pub use approval::{approval_context, ApprovalContext};
pub use budget::{Budget, BudgetVerdict};
pub use classify::classify;
pub use confine::check_confinement;
pub use evaluate::{
    classify_from, evaluate, evaluate_from, evaluate_owned, evaluate_with, first_use_app,
    first_use_request, heading_like_name, owner_gave_url, presumed_submit, recipient_field_name,
    return_summary, strong_recipient_field_name, summarize, summary_for, summary_with, Decision,
    ScreenSource, ScreenTree, TypedText, QUESTION_HEADINGS, QUESTION_KEYWORDS, QUESTION_MARKERS,
    RECIPIENT_FIELD_HEADS, RECIPIENT_FIELD_NEGATIVES, RECIPIENT_FIELD_QUALIFIERS,
    RECIPIENT_FIELD_STRONG_HEADS, RECIPIENT_FIELD_WEAK_HEADS, UNVERIFIED_SCREEN_WARNING,
};
pub use gate::{changes_state, gate, gate_in, gate_on, Gate, TRUSTED_ALLOWED};
pub use hit::{
    control_at, point_is_clear, system_bar, takes_taps, tap_spot, tap_spot_under, Bar, Covered,
    CoveredBy, Edge, OccluderRule, PointError, GRID as TAP_GRID, OCCLUDER_RULE,
    PAGE_MIN_BAR_HEIGHTS, PAGE_MIN_WIDTH, SYSTEM_BAR_BOTTOM_REGION, SYSTEM_BAR_MAX_BOTTOM_HEIGHT,
    SYSTEM_BAR_MAX_TOP_HEIGHT, SYSTEM_BAR_MIN_CAP, SYSTEM_BAR_MIN_WIDTH, SYSTEM_BAR_TOP_REGION,
};
/// The implementations faster code replaced, kept so differential tests in other crates can
/// hold the new code to them. Tests only (feature `reference`).
#[cfg(feature = "reference")]
#[doc(hidden)]
pub mod reference {
    pub use crate::hit::reference::tap_spot_under;
}
pub use phrases::{
    label_risk, AppRule, Phrase, RuleTarget, ACTION_PHRASES, APP_PHRASES, APP_RULES,
    APP_STORE_BUNDLE_ID, COMPOSE_FIELD_PHRASES, CONTEXT_PHRASES, CREDENTIAL_FIELD_PHRASES,
    HIDDEN_FIELD_PHRASES, NAVIGATION_PHRASES, PRICE_VERB_PHRASES, SETTINGS_ACTION_PHRASES,
    SETTINGS_BUNDLE_ID, SETTINGS_CONTEXT_PHRASES, SF_SYMBOLS, SHARE_SAFE_PHRASES,
};
pub use redact::{
    contains_secret, credential_like_field, redact, redact_element, redact_screen, ACCOUNT_MASK,
    CARD_MASK, CODE_MASK, KEY_MASK, SECRET_MASK,
};
pub use sanitize::{
    sanitize_screen_text, sanitize_screen_text_with_limit, MAX_SCREEN_TEXT_CHARS, SCREEN_BLOCK_END,
    SCREEN_BLOCK_START,
};
pub use text::{label_is_foreign, label_is_unreadable, FOREIGN_UI_WORDS};
pub use validate::{
    url_scheme, validate_action, ActionError, ALLOWED_URL_SCHEMES, MAX_LONG_PRESS_MS,
    MAX_TYPE_CHARS, MAX_URL_LEN, MAX_WAIT_MS,
};
