//! Safety layer for the Jarvis core: app deny list, risk classification, confirmation gates,
//! budgets, screen-text sanitising, redaction and action validation.
//!
//! Screen content is untrusted data. Everything here is pure, deterministic and panic-free.
//! The agent calls [`evaluate`] for every proposed action, [`sanitize_screen_text`] and
//! [`redact`] on every piece of screen text that enters a prompt or log, and keeps a
//! [`Budget`] per task.
//!
//! # What this layer does and does not protect against
//!
//! This is a **net for blunders, not a security boundary**. It catches the common ways a
//! small model goes wrong — tapping Pay, Send or Delete it should not, typing a password,
//! looping forever — and it keeps untrusted screen text from posing as prompt structure
//! or leaking obvious secrets into prompts and logs. It errs towards asking.
//!
//! It does **not** understand what a button really does. Classification reads English
//! labels and nearby text through phrase tables plus a few app-specific rules; a button
//! whose label says nothing (and whose screen gives no hint) can still do something
//! irreversible. Non-English and unreadable labels are only caught bluntly (a tap on one
//! always asks, via [`label_is_unreadable`]); German, French or Spanish UI in Latin script
//! is read as if it were English and mostly passes as harmless. Sanitising breaks known
//! delimiter and chat-template forms but cannot stop a model from *following* injected
//! prose; redaction is pattern-based and will miss secrets in unusual formats.
//!
//! The real boundary is the operating system: Face ID / passcode prompts, the Apple Pay
//! sheet, iOS's own call and profile-install confirmations, and the deny list keeping the
//! agent out of Passwords, Wallet and banking apps. Nothing here replaces them.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod app;
mod budget;
mod classify;
mod evaluate;
mod gate;
mod phrases;
mod redact;
mod sanitize;
mod text;
mod validate;

pub use app::{app_level, check_app, is_springboard, matching_rule, Verdict};
pub use budget::{Budget, BudgetVerdict};
pub use classify::classify;
pub use evaluate::{evaluate, first_use_app, first_use_request, summarize, Decision};
pub use gate::{changes_state, gate, gate_in, Gate, TRUSTED_ALLOWED};
pub use phrases::{
    label_risk, AppRule, Phrase, RuleTarget, ACTION_PHRASES, APP_PHRASES, APP_RULES,
    APP_STORE_BUNDLE_ID, COMPOSE_FIELD_PHRASES, CONTEXT_PHRASES, CREDENTIAL_FIELD_PHRASES,
    NAVIGATION_PHRASES, PRICE_VERB_PHRASES, SETTINGS_ACTION_PHRASES, SETTINGS_BUNDLE_ID,
    SETTINGS_CONTEXT_PHRASES, SF_SYMBOLS, SHARE_SAFE_PHRASES,
};
pub use redact::{
    contains_secret, redact, redact_element, redact_screen, ACCOUNT_MASK, CARD_MASK, CODE_MASK,
    KEY_MASK, SECRET_MASK,
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
