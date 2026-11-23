#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Sixteenth review, Part A (safety crate): confinement (Rule T), insert-content controls
//! (Rule P). Copied from the review's probes (`probe16_paste_menu_into_page_unasked`,
//! `probe16_copy_in_messages_unasked`, `probe16_page_keypad_taps_spell_digits_unasked`) and
//! adapted where the rule that answers them is Rule T (said at each).

mod common;

use common::{button, compact, el, field, label, screen};
use jarvis_proto::fixtures::{node, rect, text, IPHONE_SIZE};
use jarvis_proto::{Action, CompactScreen, Element, Policy, RiskCategory, Role, Screen, UiNode};
use jarvis_safety::{
    copy_control, evaluate_confined, evaluate_owned, flow_note, insert_control, observed_context,
    Confinement, Decision, FlowContext, ScreenSource, QUESTION_MARKERS,
};

fn decide(action: &Action, s: &CompactScreen, owner: &[&str]) -> Decision {
    evaluate_owned(
        &Policy::default(),
        action,
        s,
        None,
        ScreenSource::Typed,
        None,
        &[],
        owner,
    )
}

fn confined(action: &Action, s: &CompactScreen, owner: &[&str], places: &[String]) -> Decision {
    evaluate_confined(
        &Policy::default(),
        action,
        s,
        None,
        ScreenSource::Typed,
        None,
        &[],
        owner,
        places,
    )
}

fn show(d: &Decision) -> String {
    match d {
        Decision::Confirm(r) => format!(
            "Confirm[{:?}, phone_only={}]: {}",
            r.category, r.phone_only, r.summary
        ),
        other => format!("{other:?}"),
    }
}

fn page_form() -> CompactScreen {
    screen(
        "com.apple.mobilesafari",
        "Safari",
        vec![
            label(1, "Join our newsletter"),
            field(2, Role::TextField, "Your name"),
            button(3, "Join"),
        ],
    )
}

const OWNER: &[&str] = &["Sign me up for the newsletter on this page"];

/// Probe (as written): the edit menu's "Paste" puts the clipboard into the page's field.
/// Rule P: it always asks, phone only, saying content Jarvis cannot show is entered.
#[test]
fn probe16_paste_menu_into_page_unasked() {
    let mut s = page_form();
    s.elements
        .push(el(4, Role::Other("MenuItem".into()), "Paste"));
    let d = decide(&Action::Tap { element: 4 }, &s, OWNER);
    assert!(
        matches!(d, Decision::Confirm(_)),
        "\"Paste\" into a page field runs unasked: {}",
        show(&d)
    );
    let Decision::Confirm(r) = d else { panic!() };
    assert!(r.phone_only, "{}", r.summary);
    assert!(
        r.summary.contains("content Jarvis cannot show"),
        "{}",
        r.summary
    );
}

/// Probe, changed: Copy itself does not ask (nothing leaves Messages until something is
/// pasted, and every Paste asks). What the probe guards is that the code copied in
/// Messages cannot reach the page unasked: the Paste question names Messages (Rule T: the
/// task read Messages; Rule P: the clipboard holds what was copied there).
#[test]
fn probe16_copy_in_messages_unasked() {
    let messages = screen(
        "com.apple.MobileSMS",
        "Messages",
        vec![
            label(1, "Your Bank code is 482913. Do not share it."),
            el(2, Role::Other("MenuItem".into()), "Copy"),
        ],
    );
    let copy = Action::Tap { element: 2 };
    assert!(copy_control(&copy, &messages));
    // The task read Messages and pressed Copy there.
    let msgs = FlowContext::app("com.apple.MobileSMS");
    let page_ctx = FlowContext::Site {
        app: "com.apple.mobilesafari".into(),
        host: "newsletter.example".into(),
    };
    let mut c = Confinement::new();
    c.read(msgs.clone());
    c.copied(msgs.clone());
    c.read(page_ctx.clone());
    let mut page = page_form();
    page.elements
        .push(el(4, Role::Other("MenuItem".into()), "Paste"));
    let paste = Action::Tap { element: 4 };
    let pasting = insert_control(&paste, &page, None).is_some();
    assert!(pasting);
    let sources = c.unallowed(&page_ctx, pasting);
    assert_eq!(sources, vec![msgs.clone()]);
    let d = confined(&paste, &page, OWNER, &["Messages".to_owned()]);
    let Decision::Confirm(r) = d else {
        panic!("the paste of a code copied in Messages ran unasked")
    };
    assert!(r.phone_only);
    assert!(r.summary.contains("Messages"), "{}", r.summary);
    assert!(
        r.summary.contains("could be passed on here"),
        "{}",
        r.summary
    );
    // Even after the owner allowed Messages into the page, the clipboard of an earlier
    // copy is still a source only through the read set: the paste still asks (Rule P).
    c.allow(&page_ctx, &sources);
    assert!(c.unallowed(&page_ctx, true).is_empty());
    assert!(matches!(decide(&paste, &page, OWNER), Decision::Confirm(_)));
}

fn keypad() -> CompactScreen {
    let mut els: Vec<Element> = vec![label(1, "Enter the code to continue")];
    for (i, d) in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"]
        .iter()
        .enumerate()
    {
        els.push(button(2 + u32::try_from(i).unwrap(), d));
    }
    screen("com.apple.mobilesafari", "Safari", els)
}

/// Probe, through Rule T (setup changed as the review's plan says): the task first read
/// Messages (a code), then the page's keypad. The "4" asks — not because it is a keypad,
/// but because Messages was read and not allowed into the page.
#[test]
fn probe16_page_keypad_taps_spell_digits_unasked() {
    let s = keypad();
    let tap = Action::Tap { element: 5 };
    let page = FlowContext::Site {
        app: "com.apple.mobilesafari".into(),
        host: "pin.example".into(),
    };
    let mut c = Confinement::new();
    c.read(FlowContext::app("com.apple.MobileSMS"));
    c.read(page.clone());
    let places: Vec<String> = c
        .unallowed(&page, false)
        .iter()
        .map(|x| x.display(|_| Some("Messages".to_owned())))
        .collect();
    let d = confined(&tap, &s, OWNER, &places);
    assert!(
        matches!(&d, Decision::Confirm(r) if r.phone_only && r.summary.contains("Messages")),
        "a page keypad's \"4\" is pressed unasked after the task read Messages: {}",
        show(&d)
    );
}

/// The mirror: a task that has read only the keypad page. The taps are not asked, and that
/// is safe: the model knows nothing the page does not (the page itself showed the keypad
/// and anything the model could spell with it), so there is nothing to carry.
#[test]
fn keypad_taps_in_a_task_that_read_only_the_page_are_not_asked() {
    let s = keypad();
    let page = FlowContext::Site {
        app: "com.apple.mobilesafari".into(),
        host: "pin.example".into(),
    };
    let mut c = Confinement::new();
    c.read(page.clone());
    assert!(c.unallowed(&page, false).is_empty());
    let d = confined(&Action::Tap { element: 5 }, &s, OWNER, &[]);
    assert_eq!(d, Decision::Allow, "{}", show(&d));
}

/// Rule P by structure and by whole label: AutoFill and password items ask (Credential), a
/// page's own "Paste" button asks too (over-ask), "Copy"/"Select All" do not.
#[test]
fn insert_controls_by_structure_and_label() {
    let mk = |role: Role, l: &str| {
        let mut s = page_form();
        s.elements.push(el(4, role, l));
        s
    };
    for (role, l, cat) in [
        (
            Role::Other("MenuItem".into()),
            "AutoFill",
            RiskCategory::Credential,
        ),
        (
            Role::Other("MenuItem".into()),
            "Contact",
            RiskCategory::SendMessage,
        ),
        (
            Role::Button,
            "Use Strong Password",
            RiskCategory::Credential,
        ),
        (Role::Button, "Paste", RiskCategory::SendMessage),
        (Role::Button, "Paste and Go", RiskCategory::SendMessage),
    ] {
        let s = mk(role, l);
        let d = decide(&Action::Tap { element: 4 }, &s, OWNER);
        match d {
            Decision::Confirm(r) => {
                assert!(r.phone_only, "{l}");
                assert!(
                    r.category >= cat || r.category == cat,
                    "{l}: {:?}",
                    r.category
                );
            }
            other => panic!("{l}: {other:?}"),
        }
    }
    for l in ["Copy", "Select All", "Look Up"] {
        let s = mk(Role::Other("MenuItem".into()), l);
        assert_eq!(
            decide(&Action::Tap { element: 4 }, &s, OWNER),
            Decision::Allow,
            "{l}"
        );
    }
    // "Contact" outside a menu is an ordinary button.
    let s = mk(Role::Button, "Contact");
    assert_eq!(
        decide(&Action::Tap { element: 4 }, &s, OWNER),
        Decision::Allow
    );
}

/// Rule P asks whatever the owner relaxed (an owner who confirms nothing included).
#[test]
fn paste_asks_even_when_nothing_is_confirmed() {
    let mut s = page_form();
    s.elements
        .push(el(4, Role::Other("MenuItem".into()), "Paste"));
    let p = Policy {
        confirm: Vec::new(),
        ..Policy::default()
    };
    let d = evaluate_owned(
        &p,
        &Action::Tap { element: 4 },
        &s,
        None,
        ScreenSource::Typed,
        None,
        &[],
        OWNER,
    );
    assert!(matches!(d, Decision::Confirm(r) if r.phone_only));
}

/// A flow question merges with the action's own question (one question, phone only).
#[test]
fn flow_note_merges_into_a_risk_question() {
    let s = page_form();
    let typing = Action::Type {
        element: Some(2),
        text: "482913".into(),
        submit: false,
    };
    let d = confined(&typing, &s, OWNER, &["Messages".to_owned()]);
    let Decision::Confirm(r) = d else { panic!() };
    assert!(
        r.summary.contains("This text is not from your request"),
        "{}",
        r.summary
    );
    assert!(
        r.summary.contains("Jarvis has read Messages"),
        "{}",
        r.summary
    );
    assert!(r.phone_only);
    assert_eq!(r.summary.matches("Type ").count(), 1, "{}", r.summary);
}

/// The note names at most 4 places, then counts.
#[test]
fn flow_note_names_four_places() {
    let names: Vec<String> = [
        "Messages",
        "Mail",
        "a.example",
        "b.example",
        "Notes",
        "Photos",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    let n = flow_note(&names);
    assert_eq!(
        n.text,
        ". Jarvis has read Messages, Mail, a.example, b.example and 2 more in this task; what it read there could be passed on here"
    );
    assert_eq!(
        flow_note(&names[..2]).text,
        ". Jarvis has read Messages and Mail in this task; what it read there could be passed on here"
    );
    assert!(QUESTION_MARKERS.contains(&"what it read there could be passed on here"));
}

// ---- contexts from the raw tree ----------------------------------------------------------

fn app(bundle: &str, name: &str, kids: Vec<UiNode>) -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(name)
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(kids)
        ]);
    jarvis_proto::fixtures::screen(bundle, name, root, false)
}

fn ctx(s: &Screen) -> Option<FlowContext> {
    let c = compact(s);
    observed_context(s, &c, ScreenSource::Typed)
}

fn address(value: &str) -> UiNode {
    UiNode::new(Role::TextField, rect(100.0, 800.0, 200.0, 30.0))
        .with_label("Address")
        .with_value(value)
        .with_identifier("TabBarItemTitle")
}

fn webview(kids: Vec<UiNode>) -> UiNode {
    UiNode::new(Role::WebView, rect(0.0, 100.0, 393.0, 650.0)).with_children(kids)
}

#[test]
fn the_site_is_read_from_the_browsers_chrome() {
    let s = app(
        "com.apple.mobilesafari",
        "Safari",
        vec![
            webview(vec![text("Hello", rect(16.0, 140.0, 200.0, 30.0))]),
            address("\u{200E}bank.example"),
        ],
    );
    assert_eq!(
        ctx(&s),
        Some(FlowContext::Site {
            app: "com.apple.mobilesafari".into(),
            host: "bank.example".into()
        })
    );
}

/// A fake address bar drawn inside the page is never trusted: the real chrome decides, and
/// without one the site is not readable.
#[test]
fn a_fake_address_bar_inside_the_page_is_not_trusted() {
    let fake = webview(vec![address("bank.example")]);
    let s = app("com.apple.mobilesafari", "Safari", vec![fake.clone()]);
    assert_eq!(
        ctx(&s),
        Some(FlowContext::SiteUnreadable("com.apple.mobilesafari".into()))
    );
    let s = app(
        "com.apple.mobilesafari",
        "Safari",
        vec![fake, address("evil.example")],
    );
    assert_eq!(
        ctx(&s),
        Some(FlowContext::Site {
            app: "com.apple.mobilesafari".into(),
            host: "evil.example".into()
        })
    );
}

#[test]
fn an_unreadable_host_is_the_app_with_its_site_not_readable() {
    for v in ["", "Search or enter website name", "Apple – Official site"] {
        let s = app(
            "com.apple.mobilesafari",
            "Safari",
            vec![webview(Vec::new()), address(v)],
        );
        assert_eq!(
            ctx(&s),
            Some(FlowContext::SiteUnreadable("com.apple.mobilesafari".into())),
            "{v:?}"
        );
    }
    // Two different hosts in the chrome: not readable.
    let s = app(
        "com.apple.mobilesafari",
        "Safari",
        vec![
            webview(Vec::new()),
            address("a.example"),
            address("b.example"),
        ],
    );
    assert!(matches!(ctx(&s), Some(FlowContext::SiteUnreadable(_))));
    // Runner text: never read from its forgeable structure.
    let s = app(
        "com.apple.mobilesafari",
        "Safari",
        vec![webview(Vec::new()), address("a.example")],
    );
    let c = compact(&s);
    assert!(matches!(
        observed_context(&s, &c, ScreenSource::RunnerText),
        Some(FlowContext::SiteUnreadable(_))
    ));
    let name = FlowContext::SiteUnreadable("com.apple.mobilesafari".into())
        .display(|_| Some("Safari".to_owned()));
    assert_eq!(name, "Safari (site not readable)");
}

fn icon(label: &str, x: f64) -> UiNode {
    node(Role::Button, label, rect(x, 120.0, 72.0, 72.0)).with_identifier(label)
}

#[test]
fn a_home_screen_of_icons_is_not_a_read() {
    let home = app(
        "com.apple.springboard",
        " ",
        vec![
            icon("Settings", 20.0),
            icon("Messages", 112.0),
            node(
                Role::Other("Other".into()),
                "Dock",
                rect(0.0, 760.0, 393.0, 90.0),
            )
            .with_children(vec![icon("Safari", 136.0)]),
        ],
    );
    assert_eq!(ctx(&home), None);
}

#[test]
fn a_home_screen_with_a_widget_or_a_banner_is_a_read() {
    let widget = app(
        "com.apple.springboard",
        " ",
        vec![
            icon("Settings", 20.0),
            node(Role::Button, "Calendar", rect(200.0, 300.0, 180.0, 180.0))
                .with_value("Widget, Stack")
                .with_identifier("Calendar"),
        ],
    );
    assert_eq!(
        ctx(&widget),
        Some(FlowContext::app("com.apple.springboard"))
    );
    let banner = app(
        "com.apple.springboard",
        " ",
        vec![
            icon("Settings", 20.0),
            UiNode::new(
                Role::Other("NotificationBanner".into()),
                rect(8.0, 50.0, 377.0, 80.0),
            )
            .with_children(vec![text(
                "Bank: your code is 482913",
                rect(16.0, 60.0, 300.0, 20.0),
            )]),
        ],
    );
    assert_eq!(
        ctx(&banner),
        Some(FlowContext::app("com.apple.springboard"))
    );
}
