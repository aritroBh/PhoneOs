#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 39: what cannot be read is its own fact (asked on the phone only at every
//! trust level, refused while the owner denies any category), for every role whose words
//! are read and on every path; the emoji edge table; multi-letter compatibility forms;
//! mixed-script look-alike words; drawn format characters; a number as one word of a row;
//! the amounts slot within its allotment. The reviewer's own tests are
//! `pass39_breakit.rs`.

mod common;

use common::{app_screen, el, field};
use jarvis_proto::{
    Action, AppRule, CompactScreen, Direction, Policy, PolicyMode, RiskCategory, Role, TrustLevel,
};
use jarvis_safety::{classify, evaluate, Decision};

fn tap_as(role: Role, l: &str) -> RiskCategory {
    let s = app_screen(vec![el(1, role, l)]);
    classify(&Action::Tap { element: 1 }, &s, None)
}

fn decide_as(policy: &Policy, role: Role, l: &str) -> Decision {
    let s = app_screen(vec![el(1, role, l)]);
    evaluate(policy, &Action::Tap { element: 1 }, &s, None)
}

fn short(d: &Decision) -> String {
    match d {
        Decision::Allow => "Allow".to_owned(),
        Decision::Deny(why) => format!("Deny({why})"),
        Decision::Confirm(c) => format!("Confirm({:?}, phone_only {})", c.category, c.phone_only),
    }
}

/// The policies of every trust tier, without and with a denial of `deny` (a category the
/// label is not of, or the one it is).
fn tiers(deny: Option<RiskCategory>) -> Vec<(&'static str, Policy)> {
    let with = |level: Option<TrustLevel>, mode: PolicyMode| Policy {
        app_rules: level
            .map(|l| vec![AppRule::new("com.example.app", l)])
            .unwrap_or_default(),
        deny_categories: deny.into_iter().collect(),
        mode,
        ..Policy::default()
    };
    vec![
        ("guarded", with(None, PolicyMode::Guarded)),
        (
            "trusted",
            with(Some(TrustLevel::Trusted), PolicyMode::Guarded),
        ),
        (
            "step by step",
            with(Some(TrustLevel::StepByStep), PolicyMode::Guarded),
        ),
        (
            "guarded app",
            with(Some(TrustLevel::Guarded), PolicyMode::Guarded),
        ),
    ]
}

/// Labels that cannot be read, one per reason, with the category a reading still gives.
fn unreadable_labels() -> Vec<(String, &'static str)> {
    vec![
        ("De\u{1C0}ete Account".to_owned(), "intruder"),
        ("Ca\u{2223}\u{2223}".to_owned(), "intruder"),
        ("P\u{FE6B}y now".to_owned(), "intruder"),
        ("Ca\u{31D1}\u{31D1}".to_owned(), "CJK stroke at the edge"),
        ("\u{33D2}out".to_owned(), "multi-letter form beside letters"),
        ("De\u{890}ete".to_owned(), "drawn format character"),
        (
            "\u{405}\u{435}\u{3163}\u{3163} now, best price today guaranteed".to_owned(),
            "mixed-script look-alike word",
        ),
        (
            format!("Settings {} Next", "x".repeat(600)),
            "past 512 characters",
        ),
        ("aA".repeat(100), "past the soft-boundary bound"),
    ]
}

const ROLES: [Role; 8] = [
    Role::Button,
    Role::Link,
    Role::Cell,
    Role::Switch,
    Role::Tab,
    Role::Key,
    Role::StaticText,
    Role::Image,
];

/// The pointing actions at element 1 of `s`: tap, long press, `tap_point` at its centre.
fn pointing(s: &CompactScreen) -> Vec<(&'static str, Action)> {
    let f = s.elements[0].frame;
    vec![
        ("tap", Action::Tap { element: 1 }),
        (
            "long press",
            Action::LongPress {
                element: 1,
                duration_ms: 800,
            },
        ),
        (
            "tap_point",
            Action::TapPoint {
                x: (f.x + f.width / 2.0) / s.size.width,
                y: (f.y + f.height / 2.0) / s.size.height,
            },
        ),
    ]
}

/// The flag on every role and pointing action, at every trust tier: asked on the phone
/// only; and refused while the owner denies any category (here `Call`, which none of the
/// labels reads as, and `Delete`).
#[test]
fn what_cannot_be_read_asks_phone_only_at_every_tier_and_is_refused_under_any_deny() {
    let mut wrong = Vec::new();
    for (l, why) in unreadable_labels() {
        for role in ROLES {
            let s = app_screen(vec![el(1, role.clone(), &l)]);
            for (an, a) in pointing(&s) {
                assert!(
                    jarvis_safety::reference::cannot_be_read(&a, &s),
                    "{role:?} {l:?} {an}: not flagged ({why})"
                );
                for (tn, p) in tiers(None) {
                    match evaluate(&p, &a, &s, None) {
                        Decision::Confirm(c) if c.phone_only => {}
                        d => wrong.push(format!("{role:?} {why} {an} {tn}: {}", short(&d))),
                    }
                }
                for deny in [RiskCategory::Call, RiskCategory::Delete] {
                    for (tn, p) in tiers(Some(deny)) {
                        let d = evaluate(&p, &a, &s, None);
                        if !matches!(d, Decision::Deny(_)) {
                            wrong.push(format!(
                                "{role:?} {why} {an} {tn} denying {deny:?}: {}",
                                short(&d)
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// An owner who confirms nothing at all is still asked (the flag is not a relaxable
/// category), and a denied app stays denied.
#[test]
fn an_empty_confirm_list_still_asks_and_a_denied_app_is_still_denied() {
    let s = app_screen(vec![el(1, Role::Button, "De\u{1C0}ete Account")]);
    let relaxed = Policy {
        confirm: Vec::new(),
        app_rules: vec![AppRule::new("com.example.app", TrustLevel::Trusted)],
        ..Policy::default()
    };
    let d = evaluate(&relaxed, &Action::Tap { element: 1 }, &s, None);
    assert!(
        matches!(&d, Decision::Confirm(c) if c.phone_only),
        "{}",
        short(&d)
    );
    let denied = Policy {
        app_rules: vec![AppRule::new("com.example.app", TrustLevel::Denied)],
        ..Policy::default()
    };
    assert!(matches!(
        evaluate(&denied, &Action::Tap { element: 1 }, &s, None),
        Decision::Deny(_)
    ));
}

/// The flag carries every category the readings give: `Delete ǀAccount` still asks
/// `Delete` (phone only), a denial of `Delete` names it.
#[test]
fn the_flag_keeps_the_categories_the_readings_give() {
    let s = app_screen(vec![el(1, Role::Button, "Delete \u{1C0}Account")]);
    match evaluate(&Policy::default(), &Action::Tap { element: 1 }, &s, None) {
        Decision::Confirm(c) => {
            assert_eq!(c.category, RiskCategory::Delete);
            assert!(c.phone_only);
        }
        d => panic!("{}", short(&d)),
    }
    let deny = Policy {
        deny_categories: vec![RiskCategory::Delete],
        ..Policy::default()
    };
    match evaluate(&deny, &Action::Tap { element: 1 }, &s, None) {
        Decision::Deny(why) => assert!(why.contains("Delete"), "{why}"),
        d => panic!("{}", short(&d)),
    }
}

/// Typing into a field whose label or placeholder cannot be read, and Return on a key
/// whose label cannot be read: asked phone only in a Trusted app, refused under any deny.
#[test]
fn typing_and_return_where_the_field_or_key_cannot_be_read() {
    let trusted = |deny: Vec<RiskCategory>| Policy {
        app_rules: vec![AppRule::new("com.example.app", TrustLevel::Trusted)],
        deny_categories: deny,
        ..Policy::default()
    };
    let mut f = field(1, Role::TextField, "Pass\u{2223}word");
    f.placeholder = Some("Name".to_owned());
    let by_field = app_screen(vec![f]);
    let by_key = app_screen(vec![
        field(1, Role::TextField, "Note"),
        el(2, Role::Key, "Se\u{2223}\u{2223}"),
    ]);
    let mut wrong = Vec::new();
    for (sn, s, submit) in [
        ("field", &by_field, false),
        ("field", &by_field, true),
        ("key", &by_key, true),
    ] {
        let a = Action::Type {
            element: Some(1),
            text: "hello".to_owned(),
            submit,
        };
        assert!(jarvis_safety::reference::cannot_be_read(&a, s), "{sn}");
        match evaluate(&trusted(vec![]), &a, s, None) {
            Decision::Confirm(c) if c.phone_only => {}
            d => wrong.push(format!("{sn} submit {submit}: {}", short(&d))),
        }
        let d = evaluate(&trusted(vec![RiskCategory::Call]), &a, s, None);
        if !matches!(d, Decision::Deny(_)) {
            wrong.push(format!("{sn} submit {submit} denying Call: {}", short(&d)));
        }
    }
    // Typing (no Return) beside a key that cannot be read is not about the key.
    let a = Action::Type {
        element: Some(1),
        text: "hello".to_owned(),
        submit: false,
    };
    assert!(!jarvis_safety::reference::cannot_be_read(&a, &by_key));
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// A swipe on a row or a sideways drag across one that cannot be read is flagged too.
#[test]
fn swipes_read_the_flag_of_what_they_read() {
    let s = app_screen(vec![el(1, Role::Cell, "De\u{1C0}ete Account")]);
    for d in [Direction::Left, Direction::Right] {
        let a = Action::Swipe {
            direction: d,
            element: Some(1),
        };
        assert!(jarvis_safety::reference::cannot_be_read(&a, &s), "{d:?}");
        let deny = Policy {
            deny_categories: vec![RiskCategory::Call],
            ..Policy::default()
        };
        assert!(matches!(evaluate(&deny, &a, &s, None), Decision::Deny(_)));
    }
}

/// What is merely unknown (another script, an accented word on a button) is `Unverified`
/// without the flag; ordinary labels carry no flag.
#[test]
fn the_merely_unknown_is_not_flagged() {
    for l in [
        "\u{524A}\u{9664}",
        "\u{423}\u{434}\u{430}\u{43B}\u{438}\u{442}\u{44C}",
        "L\u{F6}schen",
        "Delete Account",
        "Wi-Fi",
        "Done\u{2705}",
        "\u{2B50} Favorites",
        "Photos \u{1F4F7}",
        "\u{AE}2024 Apple\u{2122}",
        "5\u{339E}",
        "\u{33D2} Out",
    ] {
        let s = app_screen(vec![el(1, Role::Button, l)]);
        assert!(
            !jarvis_safety::reference::cannot_be_read(&Action::Tap { element: 1 }, &s),
            "{l:?}"
        );
    }
}

/// Measured and printed (not changed by this pass, as decided): in a Trusted app a control
/// that is merely `Unverified` (a label in another script) under a denial of a category is
/// allowed unasked — `TRUSTED_ALLOWED` holds `Unverified`, and nothing it reads is denied.
/// In a guarded app it asks.
#[test]
fn measured_a_merely_unverified_control_in_a_trusted_app_under_a_deny() {
    let trusted = Policy {
        app_rules: vec![AppRule::new("com.example.app", TrustLevel::Trusted)],
        deny_categories: vec![RiskCategory::Delete],
        ..Policy::default()
    };
    let guarded = Policy {
        deny_categories: vec![RiskCategory::Delete],
        ..Policy::default()
    };
    for l in ["\u{524A}\u{9664}", "L\u{F6}schen"] {
        assert_eq!(tap_as(Role::Button, l), RiskCategory::Unverified, "{l:?}");
        let t = decide_as(&trusted, Role::Button, l);
        let g = decide_as(&guarded, Role::Button, l);
        println!(
            "{l:?}: trusted, denying Delete: {}; guarded: {}",
            short(&t),
            short(&g)
        );
        assert!(matches!(g, Decision::Confirm(_)), "{l:?}");
    }
}

/// F6: a `tap_point` at a control's centre asks what a tap of it asks (category and phone
/// only), for look-alike, accented and flagged labels.
#[test]
fn tap_point_carries_what_a_tap_carries() {
    let mut wrong = Vec::new();
    for l in [
        "S\u{117}nd",
        "D\u{117}lete",
        "DeIete",
        "P@y now",
        "De\u{1C0}ete Account",
        "Ca\u{2223}\u{2223}",
    ] {
        for role in [Role::Button, Role::Cell, Role::Link] {
            let s = app_screen(vec![el(1, role.clone(), l)]);
            let acts = pointing(&s);
            let ask = |a: &Action| match evaluate(&Policy::default(), a, &s, None) {
                Decision::Confirm(c) => Some((c.category, c.phone_only)),
                _ => None,
            };
            let tap = ask(&acts[0].1);
            for (an, a) in &acts[1..] {
                if ask(a) != tap {
                    wrong.push(format!("{role:?} {l:?}: tap {tap:?}, {an} {:?}", ask(a)));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F1: every CJK stroke and radical at a word's edge asks (as the last letters of "Call",
/// the first of "Log Out"); real emoji at the edge do not.
#[test]
fn cjk_strokes_and_radicals_ask_at_the_edge_and_emoji_do_not() {
    let mut wrong = Vec::new();
    for u in (0x2E80..=0x2FDF).chain(0x31C0..=0x31EF) {
        let Some(c) = char::from_u32(u) else { continue };
        for l in [format!("Ca{c}{c}"), format!("{c}og Out")] {
            if matches!(
                decide_as(&Policy::default(), Role::Cell, &l),
                Decision::Allow
            ) {
                wrong.push(format!("U+{u:04X} {l:?}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join(" "));
    for l in [
        "Done\u{2705}",
        "\u{2B50}Favorites",
        "Photos\u{1F4F7}",
        "\u{2764}\u{FE0F}Likes",
    ] {
        assert!(
            matches!(
                decide_as(&Policy::default(), Role::Button, l),
                Decision::Allow
            ),
            "{l:?}"
        );
    }
    // Not emoji: reserved pictographs (`⚬` drawn like an `o`, `☐`), Yi radicals, alchemy.
    for c in ['\u{26AC}', '\u{2610}', '\u{A490}', '\u{1F700}'] {
        let l = format!("{c}rder now");
        assert_eq!(tap_as(Role::Button, &l), RiskCategory::Unverified, "{l:?}");
    }
}

/// F2: multi-letter compatibility forms read as their letters (`㏒ Out` is `log Out`:
/// `AccountChange`); beside other letters they cannot be read; the bounds are measured
/// after the expansion.
#[test]
fn multi_letter_forms_read_as_their_letters() {
    assert_eq!(
        tap_as(Role::Button, "\u{33D2} Out"),
        RiskCategory::AccountChange
    );
    assert_eq!(
        tap_as(Role::Button, "\u{33D2} Off"),
        RiskCategory::AccountChange
    );
    for l in [
        "\u{33D2}out",
        "\u{3388}l Mom",
        "\u{33CC}vite",
        "Pro\u{FB01}le",
    ] {
        let s = app_screen(vec![el(1, Role::Button, l)]);
        assert!(
            jarvis_safety::reference::cannot_be_read(&Action::Tap { element: 1 }, &s),
            "{l:?}"
        );
    }
    // 200 squares are 200 characters but 600 letters: past the 512-character bound.
    let l = "\u{33D2}".repeat(200);
    assert_eq!(l.chars().count(), 200);
    let s = app_screen(vec![el(1, Role::Button, &l)]);
    assert!(jarvis_safety::reference::cannot_be_read(
        &Action::Tap { element: 1 },
        &s
    ));
    // Japanese units stay what they are.
    assert_eq!(tap_as(Role::Cell, "5\u{339E}"), RiskCategory::None);
}

/// F7: a mixed-script look-alike word is flagged on any row length; a word in one script
/// is not (the other-language rule applies).
#[test]
fn mixed_script_lookalike_words_are_flagged() {
    for l in [
        "\u{405}\u{435}\u{3163}\u{3163}",
        "\u{405}\u{435}\u{3163}\u{3163} now, best price today guaranteed",
        "\u{421}\u{430}\u{3163}\u{3163}",
        "\u{405}\u{435}\u{3B9}\u{3B9}\u{5D5}",
    ] {
        for role in [Role::Cell, Role::StaticText, Role::Button] {
            let s = app_screen(vec![el(1, role.clone(), l)]);
            assert!(
                jarvis_safety::reference::cannot_be_read(&Action::Tap { element: 1 }, &s),
                "{role:?} {l:?}"
            );
        }
    }
    for l in [
        "\u{423}\u{434}\u{430}\u{43B}\u{438}\u{442}\u{44C} \u{430}\u{43A}\u{43A}\u{430}\u{443}\u{43D}\u{442}",
        "\u{394}\u{3B9}\u{3B1}\u{3B3}\u{3C1}\u{3B1}\u{3C6}\u{3AE}",
        "\u{30ED}\u{30B0}\u{30A2}\u{30A6}\u{30C8}\u{3059}\u{308B}",
        "\u{D55C}\u{AD6D}\u{C5B4} \u{6F22}\u{5B57}",
    ] {
        let s = app_screen(vec![el(1, Role::Cell, l)]);
        assert!(
            !jarvis_safety::reference::cannot_be_read(&Action::Tap { element: 1 }, &s),
            "{l:?}"
        );
    }
}

/// Drawn format characters (Prepended_Concatenation_Mark) inside a Latin word: flagged,
/// read across by the sets (the word still asks its category), refused under a deny.
#[test]
fn drawn_format_characters_are_not_dropped() {
    for c in [
        '\u{600}',
        '\u{605}',
        '\u{6DD}',
        '\u{70F}',
        '\u{890}',
        '\u{891}',
        '\u{8E2}',
        '\u{110BD}',
        '\u{110CD}',
    ] {
        let l = format!("De{c}lete");
        assert_eq!(tap_as(Role::Button, &l), RiskCategory::Delete, "{l:?}");
        let s = app_screen(vec![el(1, Role::Button, &l)]);
        assert!(jarvis_safety::reference::cannot_be_read(
            &Action::Tap { element: 1 },
            &s
        ));
        assert_eq!(
            jarvis_safety::reference::normalized(&l).chars().count(),
            7,
            "{l:?}"
        );
    }
}

/// F5: a number with its separators and currency sign is one word of a row: short
/// command rows ask; priced item rows and model numbers stay content (the reviewer's
/// pass-38 sound labels).
#[test]
fn a_number_is_one_word_of_a_row() {
    for (l, want) in [
        ("Pay $12.50 now", RiskCategory::Payment),
        ("Call 1-800-555-0199", RiskCategory::Call),
        ("Pay \u{A3}1,000.00 now", RiskCategory::Payment),
        ("Book 12:30 slot", RiskCategory::Purchase),
    ] {
        let got = tap_as(Role::Cell, l);
        assert!(
            jarvis_safety::max_risk(got, want) == got && got != RiskCategory::None,
            "{l:?}: {got:?}"
        );
    }
    for l in [
        "Volvo S60, \u{A3}32,995",
        "Volvo XC90 \u{A3}58,000",
        "Coffee $4.50 each",
        "Battery 85%",
        "Updated 10:45 AM",
        "Desk Lamp, $35.50",
    ] {
        assert_eq!(tap_as(Role::Cell, l), RiskCategory::None, "{l:?}");
    }
}

/// A small seeded generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n as u64)).unwrap()
    }
    fn any_char(&mut self) -> char {
        match self.below(6) {
            0 => char::from(u8::try_from(0x20 + self.below(95)).unwrap()),
            1 => ['$', '\u{A3}', '\u{20AC}', '\u{20BE}', '1', '0', '.', ','][self.below(8)],
            _ => {
                char::from_u32(u32::try_from(self.below(0x11_0000)).unwrap()).unwrap_or('\u{2223}')
            }
        }
    }
}

/// F8: the reviewer's arbitrary-Unicode fuzz with a typed submit beside labels of every
/// length (its skip of labels past 150 characters removed), amounts and long runs mixed in:
/// no slot trips its allotment (debug builds assert it), nothing panics.
#[test]
fn the_question_fits_its_slots_whatever_the_screen_holds() {
    let mut r = Rng(0x5EED_0039_F8F8_0001);
    for k in 0..3000u32 {
        let len = match k % 10 {
            0 => 300 + r.below(300),
            1 => 150 + r.below(150),
            _ => r.below(40),
        };
        let mut t: String = (0..len).map(|_| r.any_char()).collect();
        if k % 4 == 0 {
            t.push_str(["\u{A3}1", "$1", "\u{20BE}1", "$1,000.00"][r.below(4)]);
        }
        if k % 7 == 0 {
            t = format!("{}{t}", "x".repeat(r.below(400)));
        }
        let role = [Role::Button, Role::Cell, Role::StaticText, Role::Link][k as usize % 4].clone();
        let mut e = el(1, role, &t);
        if k % 2 == 0 {
            e.value = Some(t.clone());
        }
        let mut f = field(2, Role::TextField, "Note");
        if k % 3 == 0 {
            f.placeholder = Some(t.clone());
        }
        let s = app_screen(vec![e, f, el(3, Role::Button, &t)]);
        for a in [
            Action::Type {
                element: Some(2),
                text: "hello".to_owned(),
                submit: true,
            },
            Action::Type {
                element: Some(2),
                text: t.clone(),
                submit: true,
            },
            Action::Tap { element: 1 },
        ] {
            let _ = evaluate(&Policy::default(), &a, &s, None);
        }
    }
    for cur in ["\u{A3}", "$", "\u{20BE}", "USD "] {
        for n in [150, 175, 200, 300, 500] {
            let label = format!("{}{cur}1", "x".repeat(n));
            let s = app_screen(vec![
                el(1, Role::Button, &label),
                field(2, Role::TextField, "Note"),
            ]);
            let d = evaluate(
                &Policy::default(),
                &Action::Type {
                    element: Some(2),
                    text: "hello".to_owned(),
                    submit: true,
                },
                &s,
                None,
            );
            assert!(!matches!(d, Decision::Allow), "{n} {cur}");
        }
    }
}

/// Labels of other scripts, as `pass38_breakit.rs`'s 20,000-element test draws them
/// (Hangul syllables and jamo, Arabic presentation forms, Tamil and Sinhala vowel signs,
/// CJK compatibility ideographs, musical symbols, combining marks, variation selectors,
/// general punctuation), beside `Item N`.
#[cfg(feature = "reference")]
const OTHER: &str = "Item 5 \u{AC00}\u{AC01}\u{1100}\u{3131}\u{FB50}\u{FE70}\u{BCA}\u{F900}\u{300}\u{E0100}\u{2001}\u{AC02}";
#[cfg(feature = "reference")]
const ASCII: &str = "Item 5 abcdefghijkl";

/// The normalisation steps (`reference::norm_steps`: one per character a reading normalises,
/// one per multi-letter table search) of deciding `a` on `n` stacked buttons labelled `l`,
/// per character of one label.
#[cfg(feature = "reference")]
fn steps_per_char(l: &str, n: u32, a: &Action) -> f64 {
    let els = (0..n)
        .map(|i| {
            let mut e = el(i + 1, Role::Button, l);
            e.frame = jarvis_proto::Rect::new(16.0, 300.0, 361.0, 44.0);
            e
        })
        .chain(std::iter::once(el(30_001, Role::Button, "OK")))
        .collect();
    let s = app_screen(els);
    let before = jarvis_safety::reference::norm_steps();
    let _ = evaluate(&Policy::default(), a, &s, None);
    let d = jarvis_safety::reference::norm_steps() - before;
    d as f64 / l.chars().count() as f64 / f64::from(n)
}

/// Counted work (not time): a label of other scripts is normalised a bounded number of
/// times per decision, against an ASCII label of the same length — each control's tokens,
/// look-alike units and intruder check once per look, its context signal shared with its
/// own reading. Measured on this commit (steps per character per label): a `tap_point`
/// over 200 stacked buttons 2.23 against 1.00; one button tapped 6.79 against 2.00 (the
/// question is composed too). Before this change: 17.93 and 28.74 (each reading tokenized
/// again, the multi-letter prescan searched the table per character, the intruder check
/// ran twice per control and normalised every character twice).
#[cfg(feature = "reference")]
#[test]
fn other_scripts_cost_bounded_normalisation_work() {
    let point = Action::TapPoint {
        x: 196.0 / 393.0,
        y: 322.0 / 852.0,
    };
    let tap = Action::Tap { element: 1 };
    let (o_point, a_point) = (
        steps_per_char(OTHER, 200, &point),
        steps_per_char(ASCII, 200, &point),
    );
    let (o_tap, a_tap) = (
        steps_per_char(OTHER, 1, &tap),
        steps_per_char(ASCII, 1, &tap),
    );
    println!(
        "steps per character per label: tap_point x200 other {o_point:.2} ascii {a_point:.2}; \
         tap other {o_tap:.2} ascii {a_tap:.2}"
    );
    assert!(o_point <= 2.6, "{o_point}");
    assert!(o_point <= 2.6 * a_point.max(1.0), "{o_point} vs {a_point}");
    assert!(a_point <= 1.2, "{a_point}");
    assert!(o_tap <= 8.0, "{o_tap}");
    assert!(a_tap <= 2.5, "{a_tap}");
}
