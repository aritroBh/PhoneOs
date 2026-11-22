#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod common;

use common::*;
use jarvis_observe::{compact, estimate_tokens, render};
use jarvis_proto::fixtures::{compose_screen, login_screen, settings_screen, text};
use jarvis_proto::{CompactScreen, Role, UiNode};

#[test]
fn token_estimate_is_ceil_bytes_over_four() {
    assert_eq!(estimate_tokens(""), 0);
    assert_eq!(estimate_tokens("a"), 1);
    assert_eq!(estimate_tokens("abcd"), 1);
    assert_eq!(estimate_tokens("abcde"), 2);
    // Bytes, not chars: the ellipsis is three bytes.
    assert_eq!(estimate_tokens("…"), 1);
    assert_eq!(estimate_tokens("……"), 2);
}

#[test]
fn golden_settings() {
    let got = render(&compact(&settings_screen()), 1000);
    let want = "\
App: Settings (com.apple.Preferences)
[1] StaticText \"Settings\"
[2] SearchField \"Search\"
[3] Switch \"Airplane Mode\" value=\"0\"
[4] Cell \"Wi-Fi, HomeNet\"
[5] Cell \"Bluetooth, On\"
[6] Cell \"Notifications\"
[7] Cell \"General\"";
    assert_eq!(got, want);
}

#[test]
fn golden_login_with_keyboard_focus_and_disabled() {
    let got = render(&compact(&login_screen()), 1000);
    let want = "\
App: Acme (com.example.acme)
Keyboard: visible
[1] StaticText \"Welcome back\"
[2] TextField \"Email\" value=\"alex@example.com\"
[3] SecureTextField \"Password\" (focused)
[4] Button \"Sign in\" (disabled)
[5] Link \"Forgot password?\"";
    assert_eq!(got, want);
}

#[test]
fn golden_compose_text_view_focused_with_value() {
    let got = render(&compact(&compose_screen()), 1000);
    assert!(
        got.contains("[7] TextView \"Message\" value=\"See you at 7\" (focused)"),
        "{got}"
    );
    assert!(got.starts_with("App: Messages (com.apple.MobileSMS)\nKeyboard: visible\n"));
}

#[test]
fn empty_placeholder_and_selected_flags_render() {
    let c = compacted(vec![
        jarvis_proto::UiNode::new(Role::TextField, r(20.0, 100.0, 300.0, 44.0))
            .with_label("To")
            .with_placeholder("Name or number"),
        jarvis_proto::UiNode::new(Role::Tab, r(0.0, 770.0, 98.0, 49.0))
            .with_label("Mail")
            .selected(true)
            .enabled(false),
        jarvis_proto::UiNode::new(Role::Button, r(150.0, 770.0, 44.0, 44.0)),
    ]);
    let got = render(&c, 1000);
    assert!(
        got.contains("[1] TextField \"To\" placeholder=\"Name or number\"\n"),
        "{got}"
    );
    assert!(
        got.contains("[2] Tab \"Mail\" (selected, disabled)"),
        "{got}"
    );
    assert!(got.ends_with("[3] Button"), "{got}");
}

#[test]
fn missing_app_name_falls_back_to_bundle_id() {
    let mut c = compact(&login_screen());
    c.app_name = None;
    assert!(render(&c, 1000).starts_with("App: com.example.acme\n"));
}

#[test]
fn quotes_backslashes_and_newlines_are_escaped_on_one_line() {
    let c = compacted(vec![
        button("Say \"hi\"\nthen \\ go", 20.0, 100.0, 100.0, 44.0),
        jarvis_proto::UiNode::new(Role::TextView, r(20.0, 200.0, 300.0, 100.0))
            .with_label("Notes")
            .with_value("line1\nline2\t\"q\"\u{7}"),
    ]);
    // Labels are whitespace-normalised by compact; values keep their newlines.
    let got = render(&c, 1000);
    assert!(
        got.contains(r#"[1] Button "Say \"hi\" then \\ go""#),
        "{got}"
    );
    assert!(got.contains(r#"value="line1 line2 \"q\" ""#), "{got}");
    assert_eq!(
        got.lines().count(),
        3,
        "escaping must keep one element per line: {got}"
    );
}

#[test]
fn newlines_in_values_never_break_the_line_structure() {
    let mut c = compacted(vec![button("Ok", 20.0, 100.0, 100.0, 44.0)]);
    c.elements[0].value = Some("a\nb\r\nc\td".into());
    let got = render(&c, 1000);
    assert!(got.contains(r#"value="a\nb\r\nc\td""#), "{got}");
    assert_eq!(got.lines().count(), 2);
}

/// A screen with long labels, interactive rows near the top, and chatty static text far below.
fn busy_screen() -> CompactScreen {
    let mut kids = Vec::new();
    for i in 0..6 {
        kids.push(button(
            &format!("Action number {i} with a rather long descriptive title that keeps going"),
            20.0,
            100.0 + f64::from(i) * 50.0,
            350.0,
            44.0,
        ));
    }
    for i in 0..10 {
        kids.push(text(
            &format!("Footnote {i}: legal boilerplate nobody needs to read"),
            r(20.0, 420.0 + f64::from(i) * 30.0, 350.0, 20.0),
        ));
    }
    compacted(kids)
}

fn tokens(s: &str) -> u32 {
    estimate_tokens(s)
}

#[test]
fn within_budget_output_is_untouched() {
    let c = busy_screen();
    let full = render(&c, 10_000);
    assert!(!full.contains('…'));
    assert_eq!(render(&c, tokens(&full)), full, "exactly at budget is fine");
}

#[test]
fn stage_one_truncates_labels_only() {
    let c = busy_screen();
    let full = render(&c, 10_000);
    let got = render(&c, tokens(&full) - 5);
    assert!(tokens(&got) <= tokens(&full) - 5);
    assert!(
        got.contains("…\""),
        "labels are cut with an ellipsis: {got}"
    );
    assert_eq!(
        got.lines().count(),
        full.lines().count(),
        "no element dropped"
    );
    assert!(!got.contains("omitted") && !got.contains("more elements"));
    assert!(got.contains("[16]"), "last element still present");
}

#[test]
fn stage_two_drops_far_statics_before_any_interactive_element() {
    let c = busy_screen();
    let got = render(&c, 100);
    let want = "\
App: Example (com.example.app)
[1] Button \"Action numb…\"
[2] Button \"Action numb…\"
[3] Button \"Action numb…\"
[4] Button \"Action numb…\"
[5] Button \"Action numb…\"
[6] Button \"Action numb…\"
[7] StaticText \"Footnote 0:…\"
[8] StaticText \"Footnote 1:…\"
[9] StaticText \"Footnote 2:…\"
[10] StaticText \"Footnote 3:…\"
[11] StaticText \"Footnote 4:…\"
… 5 static texts omitted";
    assert_eq!(got, want);
    assert!(tokens(&got) <= 100);
}

#[test]
fn stage_three_drops_from_the_bottom_and_says_how_many() {
    let c = busy_screen();
    let got = render(&c, 50);
    let want = "\
App: Example (com.example.app)
[1] Button \"Action numb…\"
[2] Button \"Action numb…\"
[3] Button \"Action numb…\"
… 10 static texts omitted
… 3 more elements (scroll to see)";
    assert_eq!(got, want);
    assert!(tokens(&got) <= 50);
}

#[test]
fn stages_only_ever_advance_as_the_budget_shrinks() {
    let c = busy_screen();
    let full = tokens(&render(&c, 10_000));
    let stage = |s: &str| {
        if s.contains("more elements") {
            3
        } else if s.contains("omitted") {
            2
        } else if s.contains('…') {
            1
        } else {
            0
        }
    };
    let mut prev = 0;
    let mut seen = [false; 4];
    for budget in (1..=full).rev() {
        let st = stage(&render(&c, budget));
        assert!(
            st >= prev,
            "budget {budget} regressed from stage {prev} to {st}"
        );
        prev = st;
        seen[st] = true;
    }
    assert_eq!(seen, [true; 4], "all four stages are reachable");
}

#[test]
fn tiny_budget_still_returns_header_and_footer() {
    let c = busy_screen();
    let got = render(&c, 1);
    assert!(got.starts_with("App: Example (com.example.app)"));
    assert!(got.contains("more elements"));
    assert!(!got.contains("[1]"));
}

#[test]
fn every_feasible_budget_is_respected_and_shrinking_never_adds_elements() {
    let c = compact(&compose_screen());
    let full = render(&c, 10_000);
    let mut previous = usize::MAX;
    for budget in (1..=tokens(&full)).rev() {
        let got = render(&c, budget);
        let listed = got.lines().filter(|l| l.starts_with('[')).count();
        if listed > 0 {
            assert!(
                tokens(&got) <= budget,
                "budget {budget}: {} tokens\n{got}",
                tokens(&got)
            );
        }
        assert!(
            listed <= previous,
            "budget {budget} listed more elements than a larger one"
        );
        previous = listed;
    }
}

#[test]
fn rendering_is_deterministic() {
    let c = busy_screen();
    assert_eq!(render(&c, 100), render(&c, 100));
    let json = serde_json::to_string(&c).unwrap();
    let back: CompactScreen = serde_json::from_str(&json).unwrap();
    assert_eq!(render(&back, 100), render(&c, 100));
}

#[test]
fn no_interactive_elements_still_degrades() {
    let kids: Vec<UiNode> = (0..30)
        .map(|i| {
            text(
                &format!("Paragraph {i} of an article"),
                r(16.0, 80.0 + f64::from(i) * 24.0, 360.0, 20.0),
            )
        })
        .collect();
    let c = compacted(kids);
    let got = render(&c, 80);
    assert!(tokens(&got) <= 80, "{got}");
    assert!(got.contains("[1] StaticText"));
}
