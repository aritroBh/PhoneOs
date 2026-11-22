//! Safety layer for the Jarvis core: app deny list, risk classification, confirmation gates,
//! budgets, screen-text sanitising, redaction and action validation.
//!
//! Screen content is untrusted data. Everything here is pure, deterministic and panic-free.
//! The agent calls [`evaluate`] for every proposed action, [`sanitize_screen_text`] and
//! [`redact`] on every piece of screen text that enters a prompt or log, and keeps a
//! [`Budget`] per task.

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

pub use app::{check_app, Verdict};
pub use budget::{Budget, BudgetVerdict};
pub use classify::classify;
pub use evaluate::{evaluate, summarize, Decision};
pub use gate::{gate, Gate};
pub use phrases::{
    label_risk, Phrase, ACTION_PHRASES, APP_PHRASES, APP_STORE_BUNDLE_ID, COMPOSE_FIELD_PHRASES,
    CONTEXT_PHRASES, CREDENTIAL_FIELD_PHRASES, SETTINGS_ACTION_PHRASES, SETTINGS_BUNDLE_ID,
    SETTINGS_CONTEXT_PHRASES,
};
pub use redact::{
    contains_secret, redact, redact_element, redact_screen, CARD_MASK, CODE_MASK, KEY_MASK,
    SECRET_MASK,
};
pub use sanitize::{
    sanitize_screen_text, sanitize_screen_text_with_limit, MAX_SCREEN_TEXT_CHARS, SCREEN_BLOCK_END,
    SCREEN_BLOCK_START,
};
pub use validate::{
    url_scheme, validate_action, ActionError, ALLOWED_URL_SCHEMES, MAX_LONG_PRESS_MS,
    MAX_TYPE_CHARS, MAX_URL_LEN, MAX_WAIT_MS,
};
