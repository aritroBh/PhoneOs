//! Classification that depends on the rest of the screen, typed text, gestures and tools.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::{app_screen, button, compact, el, field, index_of, label, screen};
use jarvis_proto::fixtures::{compose_screen, login_screen, settings_screen};
use jarvis_proto::{Action, Direction, RiskCategory, Role, Tier, ToolSpec};
use jarvis_safety::classify;

fn tap(i: u32) -> Action {
    Action::Tap { element: i }
}

fn typed(element: Option<u32>, text: &str, submit: bool) -> Action {
    Action::Type {
        element,
        text: text.to_owned(),
        submit,
    }
}

#[test]
fn generic_confirm_on_checkout_screen_is_payment() {
    for generic in [
        "Confirm", "Continue", "OK", "Done", "Next", "Submit", "Yes", "",
    ] {
        let s = screen(
            "com.amazon.Amazon",
            "Amazon",
            vec![
                label(1, "Order Summary"),
                label(2, "Total"),
                label(3, "$42.10"),
                label(4, "Visa •••• 4242"),
                button(5, generic),
            ],
        );
        assert_eq!(
            classify(&tap(5), &s, None),
            RiskCategory::Payment,
            "{generic:?}"
        );
    }
}

#[test]
fn generic_confirm_with_only_a_price_is_payment() {
    let s = app_screen(vec![label(1, "Monthly plan: €9.99"), button(2, "Continue")]);
    assert_eq!(classify(&tap(2), &s, None), RiskCategory::Payment);
}

#[test]
fn generic_confirm_on_plain_screen_is_none() {
    let s = app_screen(vec![
        label(1, "Welcome to Example"),
        label(2, "We'd like to show you around."),
        button(3, "Continue"),
    ]);
    assert_eq!(classify(&tap(3), &s, None), RiskCategory::None);
}

#[test]
fn destructive_alert_makes_ok_a_delete() {
    let s = screen(
        "com.apple.mobileslideshow",
        "Photos",
        vec![
            label(1, "Delete 3 photos?"),
            label(2, "This action cannot be undone."),
            button(3, "Cancel"),
            button(4, "OK"),
        ],
    );
    assert_eq!(classify(&tap(4), &s, None), RiskCategory::Delete);
    assert_eq!(classify(&tap(3), &s, None), RiskCategory::None);
}

#[test]
fn explanatory_text_without_the_verb_still_counts() {
    let s = app_screen(vec![
        label(1, "These items will be permanently removed."),
        button(2, "Confirm"),
    ]);
    assert_eq!(classify(&tap(2), &s, None), RiskCategory::Delete);
    // ...but "Recently Deleted" as a tapped label is navigation.
    let s = app_screen(vec![button(1, "Recently Deleted")]);
    assert_eq!(classify(&tap(1), &s, None), RiskCategory::None);
}

#[test]
fn delete_menu_item_that_opens_confirmation_is_delete() {
    let s = app_screen(vec![
        el(1, Role::Cell, "Edit"),
        el(2, Role::Cell, "Delete Conversation…"),
    ]);
    assert_eq!(classify(&tap(2), &s, None), RiskCategory::Delete);
}

#[test]
fn unlabeled_icon_button_inherits_context() {
    let s = app_screen(vec![label(1, "Delete this note?"), button(2, "")]);
    assert_eq!(classify(&tap(2), &s, None), RiskCategory::Delete);
}

#[test]
fn navigation_tabs_on_risky_screen_stay_none() {
    // Only generic or unlabeled targets inherit context; a named tab does not.
    let s = app_screen(vec![
        label(1, "Total $42.10"),
        el(2, Role::Tab, "Messages"),
        el(3, Role::Tab, "Orders"),
    ]);
    assert_eq!(classify(&tap(2), &s, None), RiskCategory::None);
    assert_eq!(classify(&tap(3), &s, None), RiskCategory::None);
}

#[test]
fn return_in_message_field_with_send_button_is_send() {
    let s = compact(&compose_screen());
    let msg = index_of(&s, "Message");
    assert_eq!(
        classify(&typed(Some(msg), "See you at 7", true), &s, None),
        RiskCategory::SendMessage
    );
    // Focused field implied.
    assert_eq!(
        classify(&typed(None, "See you at 7", true), &s, None),
        RiskCategory::SendMessage
    );
    // Typing without Return only drafts.
    assert_eq!(
        classify(&typed(Some(msg), "See you at 7", false), &s, None),
        RiskCategory::None
    );
    // Tapping the field only focuses it.
    assert_eq!(classify(&tap(msg), &s, None), RiskCategory::None);
    // The Send button itself.
    assert_eq!(
        classify(&tap(index_of(&s, "Send")), &s, None),
        RiskCategory::SendMessage
    );
}

#[test]
fn return_in_compose_field_without_send_button() {
    let s = app_screen(vec![field(1, Role::TextView, "Add a comment…")]);
    assert_eq!(
        classify(&typed(Some(1), "Nice!", true), &s, None),
        RiskCategory::PublicPost
    );
    let s = app_screen(vec![field(1, Role::TextField, "iMessage")]);
    assert_eq!(
        classify(&typed(Some(1), "hi", true), &s, None),
        RiskCategory::SendMessage
    );
}

#[test]
fn return_in_search_field_is_none_even_on_busy_screen() {
    let mut search = field(1, Role::SearchField, "Search");
    search.placeholder = Some("Search messages".to_owned());
    let s = app_screen(vec![search, button(2, "Send"), button(3, "Delete")]);
    assert_eq!(
        classify(&typed(Some(1), "dinner", true), &s, None),
        RiskCategory::None
    );
}

#[test]
fn return_in_plain_form_is_none() {
    let s = app_screen(vec![
        field(1, Role::TextField, "Title"),
        button(2, "Cancel"),
    ]);
    assert_eq!(
        classify(&typed(Some(1), "Groceries", true), &s, None),
        RiskCategory::None
    );
}

#[test]
fn return_on_checkout_form_is_payment() {
    let s = app_screen(vec![
        label(1, "Total $18.00"),
        field(2, Role::TextField, "Promo code"),
        button(3, "Place Order"),
    ]);
    assert_eq!(
        classify(&typed(Some(2), "SPRING", true), &s, None),
        RiskCategory::Payment
    );
}

#[test]
fn typing_credentials() {
    let s = compact(&login_screen());
    let email = index_of(&s, "Email");
    let password = index_of(&s, "Password");
    assert_eq!(
        classify(&typed(Some(email), "alex@example.com", false), &s, None),
        RiskCategory::None
    );
    // The password field is focused and secure.
    assert_eq!(
        classify(&typed(Some(password), "hunter2", false), &s, None),
        RiskCategory::Credential
    );
    assert_eq!(
        classify(&typed(None, "hunter2", true), &s, None),
        RiskCategory::Credential
    );

    for lbl in [
        "Verification code",
        "CVV",
        "Card number",
        "Enter PIN",
        "One-time code",
    ] {
        let s = app_screen(vec![field(1, Role::TextField, lbl)]);
        assert_eq!(
            classify(&typed(Some(1), "123456", false), &s, None),
            RiskCategory::Credential,
            "{lbl}"
        );
    }
    // Credential-looking text in an ordinary field.
    let s = app_screen(vec![field(1, Role::TextView, "Notes")]);
    for secret in [
        "4111 1111 1111 1111",
        "sk-proj-a8F3kLm29QzX7wYp4Rt6Vb1N",
        "Tr0ub4dor&3x",
        "my code is 482913",
    ] {
        assert_eq!(
            classify(&typed(Some(1), secret, false), &s, None),
            RiskCategory::Credential,
            "{secret}"
        );
    }
    for plain in [
        "Buy milk and eggs",
        "alex@example.com",
        "Meeting at 7:30",
        "Hello2You",
    ] {
        assert_eq!(
            classify(&typed(Some(1), plain, false), &s, None),
            RiskCategory::None,
            "{plain}"
        );
    }
}

#[test]
fn settings_switch_and_save() {
    let s = compact(&settings_screen());
    let airplane = s
        .elements
        .iter()
        .find(|e| e.role == Role::Switch)
        .unwrap()
        .index;
    assert_eq!(
        classify(&tap(airplane), &s, None),
        RiskCategory::SystemSetting
    );
    // Cells in Settings only navigate.
    let wifi = s
        .elements
        .iter()
        .find(|e| e.label.starts_with("Wi-Fi"))
        .unwrap()
        .index;
    assert_eq!(classify(&tap(wifi), &s, None), RiskCategory::None);

    // A switch elsewhere is not a device setting by itself.
    let s = app_screen(vec![el(1, Role::Switch, "Dark mode")]);
    assert_eq!(classify(&tap(1), &s, None), RiskCategory::None);

    // "Save" on an account page is an account change; on a plain form it is not.
    let s = app_screen(vec![
        label(1, "Edit Profile"),
        field(2, Role::TextField, "Display name"),
        button(3, "Save"),
    ]);
    assert_eq!(classify(&tap(3), &s, None), RiskCategory::AccountChange);
    let s = app_screen(vec![
        field(1, Role::TextField, "Note title"),
        button(2, "Save"),
    ]);
    assert_eq!(classify(&tap(2), &s, None), RiskCategory::None);
}

#[test]
fn swipe_on_cell_sideways_is_delete() {
    let s = app_screen(vec![el(1, Role::Cell, "Alex — Dinner tonight?")]);
    for d in [Direction::Left, Direction::Right] {
        let a = Action::Swipe {
            direction: d,
            element: Some(1),
        };
        assert_eq!(classify(&a, &s, None), RiskCategory::Delete);
    }
    let a = Action::Swipe {
        direction: Direction::Up,
        element: Some(1),
    };
    assert_eq!(classify(&a, &s, None), RiskCategory::None);
    let a = Action::Scroll {
        direction: Direction::Down,
        element: None,
    };
    assert_eq!(classify(&a, &s, None), RiskCategory::None);
    // A slider labelled with its action.
    let s = app_screen(vec![el(1, Role::Slider, "slide to power off")]);
    let a = Action::Swipe {
        direction: Direction::Right,
        element: Some(1),
    };
    assert_eq!(classify(&a, &s, None), RiskCategory::SystemSetting);
}

#[test]
fn tap_point_uses_element_under_point_or_context() {
    // Button 5 sits at y = 200..236 (see common::el); x 16..377.
    let s = app_screen(vec![label(1, "Price $9.99"), button(5, "Buy")]);
    let on_buy = Action::TapPoint {
        x: 0.5,
        y: 218.0 / 852.0,
    };
    assert_eq!(classify(&on_buy, &s, None), RiskCategory::Purchase);
    // Nowhere in particular: the screen shows a price, so assume the worst.
    let nowhere = Action::TapPoint { x: 0.5, y: 0.95 };
    assert_eq!(classify(&nowhere, &s, None), RiskCategory::Payment);
    // On a plain screen an empty spot is harmless.
    let plain = app_screen(vec![label(1, "Hello")]);
    assert_eq!(classify(&nowhere, &plain, None), RiskCategory::None);
}

#[test]
fn tools_use_declared_risk_raised_by_name() {
    let s = app_screen(vec![]);
    let spec = |name: &str, risk| ToolSpec {
        name: name.to_owned(),
        description: String::new(),
        params: vec![],
        tier: Tier::Native,
        risk,
        app: None,
    };
    let call = |name: &str| Action::CallTool {
        name: name.to_owned(),
        args: serde_json::json!({}),
    };
    let t = spec("create_reminder", RiskCategory::None);
    assert_eq!(
        classify(&call("create_reminder"), &s, Some(&t)),
        RiskCategory::None
    );
    let t = spec("send_imessage", RiskCategory::SendMessage);
    assert_eq!(
        classify(&call("send_imessage"), &s, Some(&t)),
        RiskCategory::SendMessage
    );
    // Declared None but the name says delete: over-ask.
    let t = spec("delete_event", RiskCategory::None);
    assert_eq!(
        classify(&call("delete_event"), &s, Some(&t)),
        RiskCategory::Delete
    );
    // Declared risk is never lowered by an innocent name.
    let t = spec("run_shortcut", RiskCategory::Payment);
    assert_eq!(
        classify(&call("run_shortcut"), &s, Some(&t)),
        RiskCategory::Payment
    );
}

#[test]
fn urls_and_non_ui_actions() {
    let s = app_screen(vec![button(1, "Delete")]);
    let url = |u: &str| Action::OpenUrl { url: u.to_owned() };
    assert_eq!(
        classify(&url("tel:+14155550123"), &s, None),
        RiskCategory::Call
    );
    assert_eq!(
        classify(&url("FaceTime:alex@example.com"), &s, None),
        RiskCategory::Call
    );
    assert_eq!(
        classify(&url("https://example.com/checkout"), &s, None),
        RiskCategory::None
    );
    for a in [
        Action::PressHome,
        Action::Look,
        Action::Wait { ms: 500 },
        Action::OpenApp {
            app: "Messages".to_owned(),
        },
        Action::Done {
            summary: "Deleted it".to_owned(),
        },
        Action::AskUser {
            question: "Pay now?".to_owned(),
        },
        Action::Clear { element: 1 },
    ] {
        assert_eq!(classify(&a, &s, None), RiskCategory::None, "{a:?}");
    }
}

#[test]
fn unknown_element_falls_back_to_context() {
    let s = app_screen(vec![label(1, "Pay $5 now"), button(2, "OK")]);
    assert_eq!(classify(&tap(99), &s, None), RiskCategory::Payment);
}

/// Model evaluation finding: every product row with a price asked for a Purchase
/// confirmation. A row that names an item and its price opens the item; it is not a buy.
#[test]
fn priced_list_rows_navigate_and_are_not_purchases() {
    let s = app_screen(vec![
        el(1, Role::Cell, "Lamp, $35.50"),
        el(2, Role::Cell, "Desk Lamp, $35.50"),
        el(3, Role::Cell, "Headphones 129 USD"),
        button(4, "Cart (0)"),
    ]);
    for i in 1..=3 {
        assert_eq!(classify(&tap(i), &s, None), RiskCategory::None, "row {i}");
    }
    // The product page's own button does not become a purchase because a price is shown.
    let product = app_screen(vec![label(1, "$129.00"), button(2, "Add to Cart")]);
    assert_eq!(classify(&tap(2), &product, None), RiskCategory::None);
}

/// The narrowing stays narrow: a price on a button, a row that is only a price, a row whose
/// words are a buy/pay/order action or a price verb, and any row on a checkout screen still
/// ask.
#[test]
fn priced_controls_and_command_rows_still_ask() {
    let s = app_screen(vec![
        button(1, "$0.99"),
        button(2, "Headphones $129"),
        el(3, Role::Cell, "$0.99"),
        el(4, Role::Cell, "USD 5"),
        el(5, Role::Cell, "Buy, $0.99"),
        el(6, Role::Cell, "Get $1.99"),
        el(7, Role::Cell, "Order now $5"),
        el(8, Role::Link, "Lamp $35"),
    ]);
    for i in 1..=8 {
        let r = classify(&tap(i), &s, None);
        assert!(
            matches!(r, RiskCategory::Purchase | RiskCategory::Payment),
            "element {i}: {r:?}"
        );
    }
    // A confirmation-style screen (price plus payment wording): every row pays.
    let checkout = app_screen(vec![
        el(1, Role::Cell, "Lamp, $35.50"),
        label(2, "Total: $35.50"),
        button(3, "Place Order"),
    ]);
    assert_eq!(
        classify(&tap(1), &checkout, None),
        RiskCategory::Payment,
        "row on a checkout screen"
    );
    assert_eq!(classify(&tap(3), &checkout, None), RiskCategory::Payment);
}
