//! Every `ActionError` variant, plus the accepted cases next to each.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::{app_screen, button, compact, el, field, index_of, label};
use jarvis_proto::fixtures::{login_screen, settings_screen};
use jarvis_proto::{Action, Direction, Role};
use jarvis_safety::{
    url_scheme, validate_action, ActionError, MAX_LONG_PRESS_MS, MAX_TYPE_CHARS, MAX_URL_LEN,
    MAX_WAIT_MS,
};

fn typed(element: Option<u32>, text: &str, submit: bool) -> Action {
    Action::Type {
        element,
        text: text.to_owned(),
        submit,
    }
}

fn url(u: &str) -> Result<(), ActionError> {
    validate_action(&Action::OpenUrl { url: u.to_owned() }, &app_screen(vec![]))
}

#[test]
fn unknown_and_disabled_elements() {
    let s = compact(&login_screen());
    let sign_in = index_of(&s, "Sign in");
    assert_eq!(
        validate_action(&Action::Tap { element: 999 }, &s),
        Err(ActionError::UnknownElement(999))
    );
    assert_eq!(
        validate_action(&Action::Tap { element: sign_in }, &s),
        Err(ActionError::DisabledElement(sign_in))
    );
    let forgot = index_of(&s, "Forgot password?");
    assert_eq!(
        validate_action(&Action::Tap { element: forgot }, &s),
        Ok(())
    );
    let scroll = Action::Scroll {
        direction: Direction::Down,
        element: Some(77),
    };
    assert_eq!(
        validate_action(&scroll, &s),
        Err(ActionError::UnknownElement(77))
    );
    let swipe = Action::Swipe {
        direction: Direction::Left,
        element: None,
    };
    assert_eq!(validate_action(&swipe, &s), Ok(()));
}

#[test]
fn typing_targets() {
    let s = compact(&login_screen());
    let email = index_of(&s, "Email");
    let password = index_of(&s, "Password");
    let forgot = index_of(&s, "Forgot password?");
    assert_eq!(
        validate_action(&typed(Some(email), "a@b.c", false), &s),
        Ok(())
    );
    assert_eq!(
        validate_action(&typed(Some(forgot), "x", false), &s),
        Err(ActionError::NotTextInput {
            element: forgot,
            role: "link".to_owned()
        })
    );
    assert_eq!(
        validate_action(&typed(Some(password), "hunter2", false), &s),
        Err(ActionError::SecureFieldTyping(password))
    );
    // The focused field is the password field.
    assert_eq!(
        validate_action(&typed(None, "hunter2", false), &s),
        Err(ActionError::SecureFieldTyping(password))
    );
    // Nothing focused and no keyboard.
    let s = compact(&settings_screen());
    assert_eq!(
        validate_action(&typed(None, "wifi", false), &s),
        Err(ActionError::NoFocusedInput)
    );
    let mut disabled = field(1, Role::TextField, "Name");
    disabled.enabled = false;
    let s = app_screen(vec![disabled]);
    assert_eq!(
        validate_action(&typed(Some(1), "x", false), &s),
        Err(ActionError::DisabledElement(1))
    );
    assert_eq!(
        validate_action(&typed(None, "x", false), &s),
        Err(ActionError::DisabledElement(1))
    );
}

#[test]
fn typed_text_rules() {
    let s = app_screen(vec![field(1, Role::TextView, "Message")]);
    assert_eq!(
        validate_action(&typed(Some(1), "", false), &s),
        Err(ActionError::EmptyText)
    );
    // Empty text with submit just presses Return.
    assert_eq!(validate_action(&typed(Some(1), "", true), &s), Ok(()));
    let long = "a".repeat(MAX_TYPE_CHARS + 1);
    assert_eq!(
        validate_action(&typed(Some(1), &long, false), &s),
        Err(ActionError::TextTooLong {
            len: MAX_TYPE_CHARS + 1,
            max: MAX_TYPE_CHARS
        })
    );
    let max = "é".repeat(MAX_TYPE_CHARS);
    assert_eq!(validate_action(&typed(Some(1), &max, false), &s), Ok(()));
    for bad in ["hi\n", "hi\r", "a\tb", "x\u{2028}y", "nul\u{0}"] {
        assert_eq!(
            validate_action(&typed(Some(1), bad, false), &s),
            Err(ActionError::ControlCharacters),
            "{bad:?}"
        );
    }
    assert_eq!(
        validate_action(
            &Action::AskUser {
                question: "  ".to_owned()
            },
            &s
        ),
        Err(ActionError::EmptyText)
    );
}

#[test]
fn clear_needs_a_text_input() {
    let s = app_screen(vec![field(1, Role::SearchField, "Search"), button(2, "Go")]);
    assert_eq!(validate_action(&Action::Clear { element: 1 }, &s), Ok(()));
    assert_eq!(
        validate_action(&Action::Clear { element: 2 }, &s),
        Err(ActionError::NotTextInput {
            element: 2,
            role: "button".to_owned()
        })
    );
}

#[test]
fn tap_point_range() {
    let s = app_screen(vec![label(1, "x")]);
    for (x, y) in [(0.0, 0.0), (1.0, 1.0), (0.5, 0.25)] {
        assert_eq!(validate_action(&Action::TapPoint { x, y }, &s), Ok(()));
    }
    for (x, y) in [
        (-0.01, 0.5),
        (0.5, 1.01),
        (f64::NAN, 0.5),
        (0.5, f64::INFINITY),
        (393.0, 400.0),
    ] {
        assert!(matches!(
            validate_action(&Action::TapPoint { x, y }, &s),
            Err(ActionError::PointOutOfRange { .. })
        ));
    }
}

#[test]
fn waits_presses_and_names() {
    let s = app_screen(vec![button(1, "Photo")]);
    assert_eq!(
        validate_action(&Action::Wait { ms: MAX_WAIT_MS }, &s),
        Ok(())
    );
    assert_eq!(
        validate_action(
            &Action::Wait {
                ms: MAX_WAIT_MS + 1
            },
            &s
        ),
        Err(ActionError::WaitTooLong {
            ms: MAX_WAIT_MS + 1,
            max: MAX_WAIT_MS
        })
    );
    let press = |ms| Action::LongPress {
        element: 1,
        duration_ms: ms,
    };
    assert_eq!(validate_action(&press(MAX_LONG_PRESS_MS), &s), Ok(()));
    assert_eq!(
        validate_action(&press(60_000), &s),
        Err(ActionError::PressTooLong {
            ms: 60_000,
            max: MAX_LONG_PRESS_MS
        })
    );
    assert_eq!(
        validate_action(
            &Action::OpenApp {
                app: " ".to_owned()
            },
            &s
        ),
        Err(ActionError::EmptyAppName)
    );
    let tool = |name: &str, args| Action::CallTool {
        name: name.to_owned(),
        args,
    };
    assert_eq!(
        validate_action(&tool("", serde_json::json!({})), &s),
        Err(ActionError::EmptyToolName)
    );
    assert_eq!(
        validate_action(&tool("x", serde_json::json!([1, 2])), &s),
        Err(ActionError::BadToolArgs)
    );
    assert_eq!(
        validate_action(&tool("x", serde_json::Value::Null), &s),
        Ok(())
    );
    for a in [
        Action::PressHome,
        Action::Look,
        Action::Done {
            summary: String::new(),
        },
        Action::Fail {
            reason: String::new(),
        },
    ] {
        assert_eq!(validate_action(&a, &s), Ok(()));
    }
}

#[test]
fn url_schemes() {
    for ok in [
        "https://example.com",
        "HTTPS://Example.com/path?q=1#frag",
        "http://localhost:8080/",
        "mailto:alex@example.com",
        "sms:+14155550123",
        "tel:+14155550123",
        "facetime:alex@example.com",
        "maps:?q=coffee",
    ] {
        assert_eq!(url(ok), Ok(()), "{ok}");
    }
    for (bad, scheme) in [
        ("javascript:alert(1)", "javascript"),
        ("JavaScript:alert(1)", "javascript"),
        ("data:text/html,<script>", "data"),
        ("file:///etc/passwd", "file"),
        ("shortcuts://run-shortcut?name=Wipe", "shortcuts"),
        ("prefs:root=General", "prefs"),
        ("App-Prefs:root=WIFI", "app-prefs"),
        ("venmo://paycharge?txn=pay&amount=500", "venmo"),
        ("intent://x", "intent"),
    ] {
        assert_eq!(
            url(bad),
            Err(ActionError::UrlSchemeNotAllowed(scheme.to_owned())),
            "{bad}"
        );
    }
    for bad in [
        "example.com",
        "",
        "https:example.com",
        "https://",
        "https://user:pass@example.com",
        "https://apple.com@evil.example",
        "https://exa mple.com",
        " https://example.com",
        "https://example.com/\n",
        "https://exam\u{200B}ple.com",
        "https:\\\\evil.example",
        "mailto:",
        "1http://x",
    ] {
        assert!(
            matches!(url(bad), Err(ActionError::MalformedUrl(_))),
            "{bad:?} -> {:?}",
            url(bad)
        );
    }
    let long = format!("https://example.com/{}", "a".repeat(MAX_URL_LEN));
    assert_eq!(url(&long), Err(ActionError::MalformedUrl("too long")));
    assert_eq!(url_scheme("HTTP://x").as_deref(), Some("http"));
    assert_eq!(url_scheme("no scheme"), None);
    // Element roles in errors come from the screen, not the model.
    let s = app_screen(vec![el(1, Role::from_name("weird"), "x")]);
    assert!(matches!(
        validate_action(&typed(Some(1), "x", false), &s),
        Err(ActionError::NotTextInput { .. })
    ));
}
