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

// --- Scan windows cannot be padded (second review) --------------------------------------

fn at(mut e: jarvis_proto::Element, r: jarvis_proto::Rect) -> jarvis_proto::Element {
    e.frame = r;
    e
}

/// 200 texts on the swipe line listed before a "Slide to pay" slider: the crossed controls
/// are filtered to drag-reactive ones before any cap, so the slider still decides.
#[test]
fn padding_on_the_swipe_line_cannot_hide_a_pay_slider() {
    use jarvis_proto::Rect;
    let mut els = Vec::new();
    for i in 1..=200u32 {
        els.push(at(
            el(i, Role::StaticText, "."),
            Rect::new(16.0, 400.0, 361.0, 40.0),
        ));
    }
    els.push(at(
        el(201, Role::StaticText, "Total $42.10"),
        Rect::new(16.0, 200.0, 361.0, 30.0),
    ));
    els.push(at(
        el(202, Role::Slider, "Slide to pay"),
        Rect::new(16.0, 395.0, 361.0, 44.0),
    ));
    let s = screen("com.example.shop", "Shop", els);
    let a = Action::Swipe {
        direction: Direction::Right,
        element: None,
    };
    let mut small = s.clone();
    small.elements.retain(|e| e.index > 200);
    assert_eq!(classify(&a, &small, None), RiskCategory::Payment);
    assert_eq!(
        classify(&a, &s, None),
        RiskCategory::Payment,
        "padding hid the slider"
    );
}

/// More drag-reactive controls on the path than are read one by one: asked
/// (`Unverified` at least), never skipped.
#[test]
fn too_many_crossed_controls_ask() {
    use jarvis_proto::Rect;
    let els: Vec<_> = (1..=60u32)
        .map(|i| {
            at(
                el(i, Role::Slider, "Volume"),
                Rect::new(16.0, 400.0, 361.0, 40.0),
            )
        })
        .collect();
    let s = screen("com.example.app", "Example", els);
    let a = Action::Swipe {
        direction: Direction::Right,
        element: None,
    };
    assert_ne!(classify(&a, &s, None), RiskCategory::None);
}

/// Money context is read from the whole screen: neither padding before the price (screen
/// order, no target) nor padding right next to the target (nearest window) hides it.
#[test]
fn padding_cannot_push_money_out_of_any_context_scan() {
    use jarvis_proto::Rect;
    // No target: submit in a field that is not focused / not given.
    let mut els: Vec<_> = (1..=300u32).map(|i| label(i, "Lorem ipsum")).collect();
    els.push(label(301, "Total $42.10"));
    let mut confirm = button(302, "Continue");
    confirm.frame = Rect::new(16.0, 40.0, 361.0, 36.0);
    els.push(confirm);
    let s = app_screen(els.clone());
    assert_eq!(
        classify(&tap(999), &s, None),
        RiskCategory::Payment,
        "unknown target reads the whole screen"
    );
    // Target with 300 fillers stacked right on it and the price far away.
    let mut els: Vec<_> = (1..=300u32)
        .map(|i| at(label(i, "Lorem"), Rect::new(16.0, 40.0, 361.0, 36.0)))
        .collect();
    els.push(at(
        label(301, "Total $42.10"),
        Rect::new(16.0, 800.0, 361.0, 30.0),
    ));
    els.push(at(button(302, "OK"), Rect::new(16.0, 40.0, 361.0, 36.0)));
    assert_eq!(
        classify(&tap(302), &app_screen(els), None),
        RiskCategory::Payment
    );
}

/// The Return key is found among any number of keys.
#[test]
fn many_keys_cannot_hide_a_send_return_key() {
    let mut els = vec![field(1, Role::TextField, "To")];
    for i in 2..=260u32 {
        els.push(el(i, Role::Key, "a"));
    }
    els.push(el(261, Role::Key, "send"));
    let s = app_screen(els);
    assert_eq!(
        classify(&typed(Some(1), "hi", true), &s, None),
        RiskCategory::SendMessage
    );
}

/// `tap_point`: texts under the point never use up the window of pressable elements, and
/// more pressable elements than are read asks.
#[test]
fn tap_point_reads_the_pressable_elements_under_the_point() {
    use jarvis_proto::Rect;
    let mut els: Vec<_> = (1..=300u32)
        .map(|i| at(label(i, "Lorem"), Rect::new(0.0, 0.0, 393.0, 852.0)))
        .collect();
    els.push(at(
        button(301, "Pay $5.00"),
        Rect::new(100.0, 400.0, 200.0, 50.0),
    ));
    let s = app_screen(els);
    let p = Action::TapPoint {
        x: 200.0 / 393.0,
        y: 425.0 / 852.0,
    };
    assert_eq!(classify(&p, &s, None), RiskCategory::Payment);
    let els: Vec<_> = (1..=40u32)
        .map(|i| at(button(i, "Next"), Rect::new(100.0, 400.0, 200.0, 50.0)))
        .collect();
    assert_ne!(classify(&p, &app_screen(els), None), RiskCategory::None);
}

// --- Carousels and list rows (second review) ---------------------------------------------

/// A horizontal carousel of narrow cells (music, App Store shelves): scrolling or swiping it
/// sideways is browsing, not a swipe action, and asks nothing.
#[test]
fn a_sideways_scroll_of_a_carousel_asks_nothing() {
    use jarvis_proto::{Policy, Rect};
    use jarvis_safety::{evaluate, Decision};
    let els = vec![
        at(
            el(1, Role::Cell, "Summer Hits playlist"),
            Rect::new(16.0, 300.0, 160.0, 160.0),
        ),
        at(
            el(2, Role::Cell, "Chill Mix playlist"),
            Rect::new(190.0, 300.0, 160.0, 160.0),
        ),
        at(
            el(3, Role::Button, "Get"),
            Rect::new(360.0, 300.0, 120.0, 160.0),
        ),
    ];
    let s = screen("com.example.music", "Music", els);
    for a in [
        Action::Scroll {
            direction: Direction::Right,
            element: Some(1),
        },
        Action::Swipe {
            direction: Direction::Left,
            element: Some(2),
        },
        Action::Swipe {
            direction: Direction::Left,
            element: None,
        },
    ] {
        assert_eq!(classify(&a, &s, None), RiskCategory::None, "{a:?}");
        assert_eq!(
            evaluate(&Policy::default(), &a, &s, None),
            Decision::Allow,
            "{a:?}"
        );
    }
}

/// A full-width list row (Mail's message list) still has swipe actions: sideways asks
/// `Delete`, on it or across it.
#[test]
fn a_sideways_swipe_on_a_full_width_list_row_still_asks_delete() {
    use jarvis_proto::Rect;
    let els = vec![at(
        el(1, Role::Cell, "Alex, Lunch tomorrow?, 9:41"),
        Rect::new(0.0, 380.0, 393.0, 80.0),
    )];
    let s = screen("com.apple.mobilemail", "Mail", els);
    for a in [
        Action::Swipe {
            direction: Direction::Left,
            element: Some(1),
        },
        Action::Scroll {
            direction: Direction::Right,
            element: Some(1),
        },
        Action::Swipe {
            direction: Direction::Left,
            element: None,
        },
    ] {
        assert_eq!(classify(&a, &s, None), RiskCategory::Delete, "{a:?}");
    }
}

/// A plain vertical scroll of a news list whose headline contains "slide" stays free.
#[test]
fn a_vertical_scroll_over_a_headline_with_slide_asks_nothing() {
    use jarvis_proto::{Policy, Rect};
    use jarvis_safety::{evaluate, Decision};
    let els = vec![
        at(
            el(1, Role::Cell, "Stocks slide as rates rise"),
            Rect::new(0.0, 380.0, 393.0, 80.0),
        ),
        at(
            el(2, Role::StaticText, "AAPL $129.00"),
            Rect::new(16.0, 120.0, 361.0, 30.0),
        ),
        at(
            el(3, Role::Button, "Buy"),
            Rect::new(300.0, 160.0, 60.0, 40.0),
        ),
    ];
    let s = screen("com.example.news", "News", els);
    let a = Action::Scroll {
        direction: Direction::Down,
        element: None,
    };
    assert_eq!(evaluate(&Policy::default(), &a, &s, None), Decision::Allow);
}

// --- Third review: list rows vs carousels, and the cost of a hostile screen ---------------

fn row_swipe(els: Vec<jarvis_proto::Element>, width: f64, target: u32) -> RiskCategory {
    let mut s = screen("com.example.mail", "Mail", els);
    s.size = jarvis_proto::Size {
        width,
        height: 430.0_f64.max(s.size.height),
    };
    classify(
        &Action::Swipe {
            direction: Direction::Left,
            element: Some(target),
        },
        &s,
        None,
    )
}

/// Probe: a conversation row in the primary column of a landscape split view (320 of 932
/// pt) is a list row: a sideways swipe on it asks `Delete`.
#[test]
fn a_split_view_row_swipe_asks_delete() {
    use jarvis_proto::Rect;
    let row = at(
        el(1, Role::Cell, "Mom, Dinner on Sunday?"),
        Rect::new(0.0, 100.0, 320.0, 80.0),
    );
    assert_eq!(row_swipe(vec![row], 932.0, 1), RiskCategory::Delete);
}

/// Rows of an inset-grouped list and a single lone narrow cell are rows; a carousel (cells
/// and a button side by side, App Store style) and a two-column grid are not.
#[test]
fn rows_ask_and_carousels_and_grids_do_not() {
    use jarvis_proto::Rect;
    let inset = vec![
        at(
            el(1, Role::Cell, "Alex, Lunch?"),
            Rect::new(16.0, 200.0, 361.0, 60.0),
        ),
        at(
            el(2, Role::Cell, "Sam, Hi"),
            Rect::new(16.0, 260.0, 361.0, 60.0),
        ),
    ];
    assert_eq!(row_swipe(inset, 393.0, 1), RiskCategory::Delete);
    let lone = vec![at(
        el(1, Role::Cell, "Draft"),
        Rect::new(100.0, 300.0, 150.0, 60.0),
    )];
    assert_eq!(row_swipe(lone, 393.0, 1), RiskCategory::Delete);
    let carousel = vec![
        at(
            el(1, Role::Cell, "Summer Hits playlist"),
            Rect::new(16.0, 300.0, 160.0, 160.0),
        ),
        at(
            el(2, Role::Cell, "Chill Mix playlist"),
            Rect::new(190.0, 300.0, 160.0, 160.0),
        ),
        at(
            el(3, Role::Button, "Get"),
            Rect::new(360.0, 300.0, 120.0, 160.0),
        ),
    ];
    assert_eq!(row_swipe(carousel, 393.0, 2), RiskCategory::None);
    let grid = vec![
        at(
            el(1, Role::Cell, "Photo, Beach"),
            Rect::new(0.0, 200.0, 195.0, 195.0),
        ),
        at(
            el(2, Role::Cell, "Photo, Dog"),
            Rect::new(197.0, 200.0, 195.0, 195.0),
        ),
        at(
            el(3, Role::Cell, "Photo, Cake"),
            Rect::new(0.0, 397.0, 195.0, 195.0),
        ),
        at(
            el(4, Role::Cell, "Photo, Tree"),
            Rect::new(197.0, 397.0, 195.0, 195.0),
        ),
    ];
    assert_eq!(row_swipe(grid.clone(), 393.0, 3), RiskCategory::None);
    // A small accessory button beside a row does not make the row a carousel item.
    let with_info = vec![
        at(
            el(1, Role::Cell, "Alex, Lunch?"),
            Rect::new(0.0, 200.0, 330.0, 80.0),
        ),
        at(
            el(2, Role::Button, "More Info"),
            Rect::new(340.0, 220.0, 44.0, 44.0),
        ),
    ];
    assert_eq!(row_swipe(with_info, 393.0, 1), RiskCategory::Delete);
}

/// A 5,000-element hostile screen with 40 controls under the point: the screen context is
/// read once per classification, so a tap_point or an element-less swipe stays cheap in a
/// debug build (it was ~0.5 s per call before).
#[test]
fn classifying_a_huge_screen_reads_it_once() {
    use jarvis_proto::Rect;
    use std::time::{Duration, Instant};
    let mut els = Vec::new();
    for i in 0..5000u32 {
        let role = if i % 3 == 0 {
            Role::Button
        } else {
            Role::StaticText
        };
        let text = if i % 7 == 0 {
            format!("Item {i} $1{i}.00 total pay")
        } else {
            format!("Item {i} description")
        };
        els.push(at(
            el(i, role, &text),
            Rect::new(16.0, f64::from(i % 20) * 40.0, 361.0, 36.0),
        ));
    }
    for i in 0..40u32 {
        els.push(at(
            button(5000 + i, "Next"),
            Rect::new(100.0, 400.0, 200.0, 50.0),
        ));
    }
    let s = app_screen(els);
    let point = Action::TapPoint {
        x: 200.0 / 393.0,
        y: 425.0 / 852.0,
    };
    let swipe = Action::Swipe {
        direction: Direction::Left,
        element: None,
    };
    let t = Instant::now();
    let r1 = classify(&point, &s, None);
    let tap_point = t.elapsed();
    let t = Instant::now();
    let r2 = classify(&swipe, &s, None);
    let swiped = t.elapsed();
    println!("5,000 elements: tap_point {tap_point:?}, swipe {swiped:?}");
    assert_ne!(r1, RiskCategory::None);
    assert_ne!(r2, RiskCategory::None);
    assert!(tap_point < Duration::from_millis(1500), "{tap_point:?}");
    assert!(swiped < Duration::from_millis(1500), "{swiped:?}");
}

// --- Split views: two list columns side by side are rows, not a grid ---------------------

/// Mail or Settings on a wide screen: a 320 pt row of the primary column next to a 600 pt
/// row of the detail column (same height, tops aligned). The columns are lists, so a
/// sideways swipe on either row asks `Delete`.
#[test]
fn a_split_view_row_beside_a_wider_row_is_still_a_list_row() {
    use jarvis_proto::Rect;
    let split = vec![
        at(
            el(1, Role::Cell, "Mom, Dinner on Sunday?"),
            Rect::new(0.0, 100.0, 320.0, 80.0),
        ),
        at(
            el(2, Role::Cell, "Attachment, invoice.pdf"),
            Rect::new(332.0, 100.0, 600.0, 80.0),
        ),
    ];
    assert_eq!(row_swipe(split.clone(), 932.0, 1), RiskCategory::Delete);
    assert_eq!(row_swipe(split, 932.0, 2), RiskCategory::Delete);
}

/// Two equally wide list columns whose rows do not line up (different row heights, or
/// rows offset by a header): rows, not a grid.
#[test]
fn equal_columns_of_rows_with_unaligned_tops_are_list_rows() {
    use jarvis_proto::Rect;
    let mut els = Vec::new();
    for k in 0..4u32 {
        let y = 100.0 + 60.0 * f64::from(k);
        els.push(at(
            el(k + 1, Role::Cell, &format!("Left {k}")),
            Rect::new(0.0, y, 460.0, 60.0),
        ));
        // The right column starts 30 pt lower (a section header above it).
        els.push(at(
            el(k + 11, Role::Cell, &format!("Right {k}")),
            Rect::new(472.0, y + 30.0, 460.0, 60.0),
        ));
    }
    assert_eq!(row_swipe(els.clone(), 932.0, 2), RiskCategory::Delete);
    assert_eq!(row_swipe(els, 932.0, 12), RiskCategory::Delete);
    // Tops 1 pt apart (rounding) still read as aligned: a grid of equal tiles.
    let tiles = vec![
        at(
            el(1, Role::Cell, "Photo, Beach"),
            Rect::new(0.0, 200.0, 195.0, 195.0),
        ),
        at(
            el(2, Role::Cell, "Photo, Dog"),
            Rect::new(197.0, 201.0, 195.0, 195.0),
        ),
    ];
    assert_eq!(row_swipe(tiles, 393.0, 1), RiskCategory::None);
}

/// A cell next to a button of nearly but not quite the same size (85 %) is no longer a
/// carousel item: only nearly equal siblings (90 %) make one.
#[test]
fn a_sibling_must_be_nearly_the_same_size() {
    use jarvis_proto::Rect;
    let els = vec![
        at(
            el(1, Role::Cell, "Alex, Lunch?"),
            Rect::new(0.0, 200.0, 200.0, 80.0),
        ),
        at(
            el(2, Role::Button, "Archive"),
            Rect::new(210.0, 200.0, 170.0, 80.0),
        ),
    ];
    assert_eq!(row_swipe(els, 393.0, 1), RiskCategory::Delete);
    let els = vec![
        at(
            el(1, Role::Cell, "Summer Hits playlist"),
            Rect::new(16.0, 300.0, 160.0, 160.0),
        ),
        at(
            el(2, Role::Button, "Chill Mix playlist"),
            Rect::new(190.0, 300.0, 150.0, 160.0),
        ),
    ];
    assert_eq!(row_swipe(els, 393.0, 1), RiskCategory::None);
}

/// The documented residual case: two equally wide list columns with equal, aligned rows
/// cannot be told from a two-column grid, and read as one (a sideways drag is not taken
/// for `Delete`). Pinned so a change to it is deliberate.
#[test]
fn residual_equal_aligned_list_columns_read_as_a_grid() {
    use jarvis_proto::Rect;
    let els = vec![
        at(
            el(1, Role::Cell, "Left row"),
            Rect::new(0.0, 100.0, 460.0, 60.0),
        ),
        at(
            el(2, Role::Cell, "Right row"),
            Rect::new(472.0, 100.0, 460.0, 60.0),
        ),
    ];
    assert_eq!(row_swipe(els, 932.0, 1), RiskCategory::None);
}
