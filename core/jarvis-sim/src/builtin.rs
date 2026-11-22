//! The apps and tasks embedded in the binary.

use crate::app::AppDef;
use crate::error::SimError;
use crate::eval::TaskDef;

/// Built-in app files as `(file name, JSON text)`.
pub const APP_SOURCES: &[(&str, &str)] = &[
    ("home.json", include_str!("../apps/home.json")),
    ("settings.json", include_str!("../apps/settings.json")),
    ("messages.json", include_str!("../apps/messages.json")),
    ("acme.json", include_str!("../apps/acme.json")),
    ("shop.json", include_str!("../apps/shop.json")),
    ("notes.json", include_str!("../apps/notes.json")),
    ("passwords.json", include_str!("../apps/passwords.json")),
    ("phone.json", include_str!("../apps/phone.json")),
    ("laden.json", include_str!("../apps/laden.json")),
    ("news.json", include_str!("../apps/news.json")),
];

/// Built-in task files as `(file name, JSON text)`.
pub const TASK_SOURCES: &[(&str, &str)] = &[
    ("wifi_off.json", include_str!("../tasks/wifi_off.json")),
    (
        "bluetooth_off.json",
        include_str!("../tasks/bluetooth_off.json"),
    ),
    (
        "dark_mode_on.json",
        include_str!("../tasks/dark_mode_on.json"),
    ),
    (
        "open_settings.json",
        include_str!("../tasks/open_settings.json"),
    ),
    ("text_mom.json", include_str!("../tasks/text_mom.json")),
    (
        "text_mom_declined.json",
        include_str!("../tasks/text_mom_declined.json"),
    ),
    (
        "text_alex_from_home.json",
        include_str!("../tasks/text_alex_from_home.json"),
    ),
    (
        "login_hands_over.json",
        include_str!("../tasks/login_hands_over.json"),
    ),
    (
        "login_wrong_password.json",
        include_str!("../tasks/login_wrong_password.json"),
    ),
    ("cart_add.json", include_str!("../tasks/cart_add.json")),
    (
        "order_headphones.json",
        include_str!("../tasks/order_headphones.json"),
    ),
    (
        "order_declined.json",
        include_str!("../tasks/order_declined.json"),
    ),
    (
        "injection_desk_lamp.json",
        include_str!("../tasks/injection_desk_lamp.json"),
    ),
    ("notes_find.json", include_str!("../tasks/notes_find.json")),
    (
        "notes_delete.json",
        include_str!("../tasks/notes_delete.json"),
    ),
    (
        "denied_passwords.json",
        include_str!("../tasks/denied_passwords.json"),
    ),
    (
        "step_by_step_wifi.json",
        include_str!("../tasks/step_by_step_wifi.json"),
    ),
    (
        "first_use_declined.json",
        include_str!("../tasks/first_use_declined.json"),
    ),
    (
        "app_rule_denied.json",
        include_str!("../tasks/app_rule_denied.json"),
    ),
    (
        "app_rule_trusted.json",
        include_str!("../tasks/app_rule_trusted.json"),
    ),
    (
        "foreign_pay.json",
        include_str!("../tasks/foreign_pay.json"),
    ),
    ("phone_call.json", include_str!("../tasks/phone_call.json")),
    (
        "lock_mid_task.json",
        include_str!("../tasks/lock_mid_task.json"),
    ),
    (
        "lock_during_confirm.json",
        include_str!("../tasks/lock_during_confirm.json"),
    ),
    (
        "lock_during_replay.json",
        include_str!("../tasks/lock_during_replay.json"),
    ),
    (
        "locked_too_long.json",
        include_str!("../tasks/locked_too_long.json"),
    ),
    (
        "observe_fails_then_recovers.json",
        include_str!("../tasks/observe_fails_then_recovers.json"),
    ),
    (
        "observe_fails_until_budget.json",
        include_str!("../tasks/observe_fails_until_budget.json"),
    ),
    (
        "dropped_chars_corrected.json",
        include_str!("../tasks/dropped_chars_corrected.json"),
    ),
    (
        "dropped_chars_twice.json",
        include_str!("../tasks/dropped_chars_twice.json"),
    ),
    (
        "icon_tap_denied.json",
        include_str!("../tasks/icon_tap_denied.json"),
    ),
    (
        "cart_add_once.json",
        include_str!("../tasks/cart_add_once.json"),
    ),
    (
        "switch_already_on.json",
        include_str!("../tasks/switch_already_on.json"),
    ),
    (
        "done_after_decline.json",
        include_str!("../tasks/done_after_decline.json"),
    ),
    (
        "notes_two_scrolls.json",
        include_str!("../tasks/notes_two_scrolls.json"),
    ),
    (
        "news_dense_page.json",
        include_str!("../tasks/news_dense_page.json"),
    ),
];

/// Parses one app definition.
pub fn parse_app(name: &str, json: &str) -> Result<AppDef, SimError> {
    serde_json::from_str(json).map_err(|source| SimError::AppJson {
        name: name.to_owned(),
        source,
    })
}

/// All built-in apps.
pub fn builtin_apps() -> Result<Vec<AppDef>, SimError> {
    APP_SOURCES
        .iter()
        .map(|(name, json)| parse_app(name, json))
        .collect()
}

/// All built-in eval tasks, in file-name order.
pub fn builtin_tasks() -> Result<Vec<TaskDef>, SimError> {
    TASK_SOURCES
        .iter()
        .map(|(name, json)| {
            serde_json::from_str(json)
                .map_err(|e| SimError::Input(format!("built-in task {name}: {e}")))
        })
        .collect()
}
