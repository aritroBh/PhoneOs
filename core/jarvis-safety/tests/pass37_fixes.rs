#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 37: one normalisation in front of both readings (`norm`), and the findings'
//! variants the reviewer did not write. The reviewer's own tests are `pass37_breakit.rs`.

mod common;

use common::{app_screen, button, el, field, label};
use jarvis_proto::{Action, Policy, RiskCategory, Role};
use jarvis_safety::{classify, evaluate, Decision, NEVER_REMOTE_APPROVE};

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
        Decision::Deny(_) => "Deny".to_owned(),
        Decision::Confirm(c) => format!("Confirm({:?}, phone only {})", c.category, c.phone_only),
    }
}

/// Each `(label, drawn as)`: the drawn word's category (checked to be a risk), the
/// look-alike not allowed by the default policy and refused by a policy denying it.
fn misses(role: &Role, pairs: &[(&str, &str)]) -> Vec<String> {
    let mut wrong = Vec::new();
    for &(l, real) in pairs {
        let want = tap_as(role.clone(), real);
        assert!(
            !matches!(want, RiskCategory::None | RiskCategory::Unverified),
            "{real:?} is a risk word as a {role:?}"
        );
        let d = decide_as(&Policy::default(), role.clone(), l);
        if matches!(d, Decision::Allow) {
            wrong.push(format!("{role:?} {l:?} (drawn {real:?}): default Allow"));
        }
        let deny = Policy {
            deny_categories: vec![want],
            ..Policy::default()
        };
        let d = decide_as(&deny, role.clone(), l);
        if !matches!(d, Decision::Deny(_)) {
            wrong.push(format!(
                "{role:?} {l:?} under a policy denying {want:?}: {}",
                short(&d)
            ));
        }
    }
    wrong
}

/// F1: the `@` exemption is the email-shaped word's own, and within it only the last `@`
/// (an address holds one): a full-width `＠`, a word of two `@`s, a dot-separated label.
#[test]
fn f1_only_the_email_word_keeps_its_at() {
    let mut wrong = Vec::new();
    for role in [Role::Button, Role::Link] {
        wrong.extend(misses(
            &role,
            &[
                ("P\u{FF20}y anna@mail.com", "Pay anna@mail.com"),
                ("P@y@mail.com", "Pay mail.com"),
                ("Don@te\u{b7}help@charity.org", "Donate help charity.org"),
                ("bob@x.io \u{2014} Tr@nsfer", "bob x.io Transfer"),
            ],
        ));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    // Sound: an address alone and an address beside harmless words stay harmless.
    for l in [
        "anna@mail.com",
        "Email support@help.com",
        "Contact: bob@x.io",
    ] {
        assert_eq!(tap_as(Role::Button, l), RiskCategory::None, "{l:?}");
    }
}

/// F2: symbols are decided after the spelled-out join, so a spelled word keeps them.
#[test]
fn f2_spelled_out_words_keep_their_symbols() {
    let wrong = misses(
        &Role::Button,
        &[
            ("$ U B M I T", "SUBMIT"),
            ("P @ Y N O W", "PAY NOW"),
            ("$ I G N O U T", "SIGN OUT"),
            ("C A $ H O U T", "CASH OUT"),
        ],
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    // Sound: a price and a lone symbol between words read as before.
    for l in ["x $ y", "Go !", "A | B"] {
        assert_eq!(tap_as(Role::Button, l), RiskCategory::None, "{l:?}");
    }
}

/// F3: every narrow space inside a word and as the space of a phrase; the ideographic
/// space and ordinary spaces stay hard; real narrow spaces (a time, a French question
/// mark, digit grouping) change nothing.
#[test]
fn f3_every_narrow_space_is_a_soft_boundary() {
    let narrow: Vec<char> = (0x2000..=0x200A)
        .chain([0x202F, 0x205F])
        .map(|u| char::from_u32(u).unwrap())
        .collect();
    let mut wrong = Vec::new();
    for &c in &narrow {
        for role in [Role::Button, Role::Cell, Role::Link] {
            let inside = format!("Del{c}ete");
            let between = format!("Sign{c}Out");
            let both = format!("Re{c}move{c}Account");
            wrong.extend(misses(
                &role,
                &[
                    (inside.as_str(), "Delete"),
                    (between.as_str(), "Sign Out"),
                    (both.as_str(), "Remove Account"),
                ],
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    for l in [
        "10:03\u{202F}AM",
        "1\u{202F}000 photos",
        "Photo, March 30, 2018, 7:14\u{202F}PM",
    ] {
        assert_eq!(tap_as(Role::Cell, l), RiskCategory::None, "{l:?}");
    }
    // A French label with a narrow no-break space before `?` keeps its language signal.
    assert_eq!(
        tap_as(Role::Button, "Supprimer\u{202F}?"),
        RiskCategory::Unverified
    );
}

/// F4: every nonspacing and enclosing mark (Unicode 15.1) inside "Delete", by the
/// normalisation's own table, and a few by name: the combining enclosing circle (Me), a
/// musical combining mark, a Tibetan vowel sign, Hebrew points, the Arabic fatha.
#[test]
fn f4_every_mark_is_dropped_without_a_split() {
    let named = [
        '\u{20DD}',
        '\u{1D167}',
        '\u{0F72}',
        '\u{05B4}',
        '\u{064E}',
        '\u{1E8D0}',
        '\u{E0100}',
    ];
    let mut wrong = Vec::new();
    for c in named {
        let l = format!("Del{c}ete");
        wrong.extend(misses(&Role::Button, &[(l.as_str(), "Delete")]));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    // Every code point the normalisation drops (not only marks: invisibles and controls
    // too) leaves "Delete" whole.
    #[cfg(feature = "reference")]
    {
        let mut dropped = 0u32;
        for u in 0x80..=0x10FFFFu32 {
            let Some(c) = char::from_u32(u) else { continue };
            if !jarvis_safety::reference::normalized(&c.to_string()).is_empty() {
                continue;
            }
            dropped += 1;
            let l = format!("Del{c}ete");
            assert_eq!(tap_as(Role::Button, &l), RiskCategory::Delete, "U+{u:04X}");
        }
        assert!(dropped > 1998, "{dropped}");
    }
}

/// F5: more accented letters, on rows and on buttons; on a button the other-language
/// signal keeps `Unverified` carried beside the category (phone only).
#[test]
fn f5_decomposed_letters_read_their_base_and_keep_unverified() {
    let pairs = [
        ("\u{154}emove", "Remove"),
        ("Er\u{227}se All Content", "Erase All Content"),
        ("S\u{1EB9}nd", "Send"),
        ("Bl\u{1ECD}ck", "Block"),
        ("Tr\u{227}nsfer", "Transfer"),
        ("\u{1E0E}elete", "Delete"),
        ("Pa\u{1EF3}", "Pay"),
    ];
    for role in [Role::Cell, Role::Button, Role::Link] {
        let wrong = misses(&role, &pairs);
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }
    for (l, _) in pairs {
        match decide_as(&Policy::default(), Role::Button, l) {
            Decision::Confirm(c) => assert!(c.phone_only, "{l:?}: {c:?}"),
            other => panic!("{l:?}: {}", short(&other)),
        }
    }
    // A send asked by the decomposed word is SendMessage, phone only (Unverified carried).
    let s = app_screen(vec![button(1, "S\u{1EB9}nd")]);
    match evaluate(&Policy::default(), &Action::Tap { element: 1 }, &s, None) {
        Decision::Confirm(c) => {
            assert_eq!(c.category, RiskCategory::SendMessage);
            assert!(c.phone_only);
            assert!(!NEVER_REMOTE_APPROVE.contains(&RiskCategory::SendMessage));
        }
        other => panic!("{}", short(&other)),
    }
}

/// F6: more glyph look-alikes, and the same glyphs where they mean themselves.
#[test]
fn f6_more_glyphs_read_as_letters_inside_words() {
    let wrong = misses(
        &Role::Button,
        &[
            ("\u{20AC}rase All", "Erase All"),
            ("B\u{A3}ock", "Block"),
            ("Pa\u{A5}", "Pay"),
            ("\u{A2}all", "Call"),
            ("6lock", "Block"),
            ("Lo9 Out", "Log Out"),
            ("Subm\u{A1}t", "Submit"),
        ],
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    for l in [
        "\u{20AC}5",
        "5 \u{A3}",
        "\u{A5}500",
        "iPhone 16",
        "Top 20",
        "iOS 26",
        "2FA",
    ] {
        assert_eq!(
            classify(
                &Action::Tap { element: 1 },
                &app_screen(vec![el(1, Role::Cell, l)]),
                None
            ),
            match l {
                "\u{20AC}5" | "5 \u{A3}" | "\u{A5}500" => {
                    classify(
                        &Action::Tap { element: 1 },
                        &app_screen(vec![el(1, Role::Cell, "$5")]),
                        None,
                    )
                }
                _ => RiskCategory::None,
            },
            "{l:?}"
        );
    }
}

fn checkout(l: &str) -> jarvis_proto::CompactScreen {
    app_screen(vec![
        label(1, "Total"),
        label(2, "$5.00"),
        label(3, "Card ending 1234"),
        button(4, l),
    ])
}

/// F7: on a checkout a control is navigation only when its words are navigation words
/// alone; navigation labels themselves stay lowered.
#[test]
fn f7_only_a_label_that_is_navigation_is_lowered() {
    let a = Action::Tap { element: 4 };
    for l in [
        "Back to Cart and Pay",
        "Help me pay",
        "Close and confirm",
        "Details \u{203A} Go",
    ] {
        assert_eq!(
            classify(&a, &checkout(l), None),
            RiskCategory::Payment,
            "{l:?}"
        );
    }
    for l in [
        "Cancel",
        "Back",
        "Not Now",
        "No thanks",
        "Close",
        "Learn more",
        "Back \u{203A}",
    ] {
        assert_ne!(
            classify(&a, &checkout(l), None),
            RiskCategory::Payment,
            "{l:?}"
        );
    }
}

/// F7 (share sheet): a row naming a device or person beside a share-safe word is a send
/// by its own words; a share-safe row alone is lowered by its words — and, sound, any
/// press on a share sheet is still asked by the share-sheet rule, so an AirDrop device an
/// attacker named exactly "Copy" is not pressed unasked.
#[test]
fn sound_a_share_sheet_row_named_copy_is_still_asked() {
    let a = Action::Tap { element: 2 };
    for l in ["Copy Anna's iPhone", "Copy \u{b7} Anna", "Print to Bob"] {
        let s = app_screen(vec![label(1, "AirDrop"), button(2, l)]);
        assert_eq!(classify(&a, &s, None), RiskCategory::SendMessage, "{l:?}");
    }
    for l in ["Copy", "Print", "Markup"] {
        let s = app_screen(vec![label(1, "AirDrop"), button(2, l)]);
        assert_eq!(classify(&a, &s, None), RiskCategory::None, "{l:?}");
        let d = evaluate(&Policy::default(), &a, &s, None);
        assert!(!matches!(d, Decision::Allow), "{l:?}: {}", short(&d));
    }
}

/// F8: counted work. The trie steps of a label of alternating letters and digits (its
/// soft boundaries past the bound) are none: it is asked without the walk; a hostile label
/// just under the bound costs at most four times the steps per character of ordinary text.
#[cfg(feature = "reference")]
#[test]
fn f8_hostile_labels_cost_bounded_counted_work() {
    use jarvis_safety::reference::{soft_boundaries, walk_steps, MAX_SOFT_BOUNDARIES};
    let steps = |l: &str| {
        let s = app_screen(vec![el(1, Role::Button, l)]);
        let before = walk_steps();
        let c = classify(&Action::Tap { element: 1 }, &s, None);
        (walk_steps() - before, c)
    };
    let fit = |unit: &str, n: usize| {
        let mut s = String::new();
        while s.chars().count() + unit.chars().count() <= n {
            s.push_str(unit);
        }
        s
    };
    let ordinary = fit("Recently played songs ", 512);
    let (base, c) = steps(&ordinary);
    assert_eq!(c, RiskCategory::None);
    assert!(base > 0);
    for unit in ["a1", "1I", "De1ete BIock 5eII |og ", "aB"] {
        let hostile = fit(unit, 512);
        assert!(soft_boundaries(&hostile) > MAX_SOFT_BOUNDARIES, "{unit:?}");
        let (n, c) = steps(&hostile);
        assert_eq!(n, 0, "{unit:?}: {n} steps");
        assert_eq!(c, RiskCategory::Unverified, "{unit:?}");
    }
    // Just under the bound: walked, within four times the ordinary text's steps per
    // character (pass 36's bound for a hostile page; measured 3.3 against 1.1).
    let under = fit("De1ete ", 7 * (MAX_SOFT_BOUNDARIES / 2));
    assert!(soft_boundaries(&under) <= MAX_SOFT_BOUNDARIES);
    let (n, c) = steps(&under);
    assert_eq!(c, RiskCategory::Delete);
    let per = |steps: u64, text: &str| steps as f64 / text.chars().count() as f64;
    assert!(
        per(n, &under) <= 4.0 * per(base, &ordinary).max(1.0),
        "{} vs {}",
        per(n, &under),
        per(base, &ordinary)
    );
    // A real risk word beside the hostile run is still asked, never allowed.
    let mixed = format!("Delete {}", fit("a1", 400));
    let deny = Policy {
        deny_categories: vec![RiskCategory::Delete],
        ..Policy::default()
    };
    assert!(!matches!(
        decide_as(&deny, Role::Button, &mixed),
        Decision::Allow
    ));
}

/// Suspected (a): Return in a field whose long label holds a risk word past the head and
/// tail the matcher reads: the field's label is `Unverified` (the category is not lost),
/// and the press is never allowed under a policy denying the word.
#[test]
fn a_return_press_in_a_field_with_an_overlong_label_is_unverified() {
    let pad = "lorem ipsum ".repeat(80);
    let long = format!("{pad}Type DELETE to erase everything {pad}");
    let deny = Policy {
        deny_categories: vec![RiskCategory::Delete],
        ..Policy::default()
    };
    for placeholder in [false, true] {
        let mut f = field(1, Role::TextField, if placeholder { "Name" } else { &long });
        if placeholder {
            f.placeholder = Some(long.clone());
        }
        f.focused = true;
        let s = app_screen(vec![f]);
        let t = Action::Type {
            element: Some(1),
            text: "now".to_owned(),
            submit: true,
        };
        assert_eq!(classify(&t, &s, None), RiskCategory::Unverified);
        assert!(!matches!(evaluate(&deny, &t, &s, None), Decision::Allow));
    }
}

/// Suspected (b): a tab or key whose label is past 512 characters is `Unverified` (it was
/// read truncated, the sets skipped it: "Settings x…x De1ete" was allowed).
#[test]
fn b_an_overlong_tab_or_key_is_unverified() {
    let l = format!("Settings {} De1ete", "x".repeat(600));
    let deny = Policy {
        deny_categories: vec![RiskCategory::Delete],
        ..Policy::default()
    };
    for role in [Role::Tab, Role::Key] {
        assert_eq!(
            tap_as(role.clone(), &l),
            RiskCategory::Unverified,
            "{role:?}"
        );
        let d = decide_as(&deny, role.clone(), &l);
        assert!(matches!(d, Decision::Confirm(_)), "{role:?}: {}", short(&d));
    }
    // Ordinary tabs and keys read as before.
    assert_eq!(tap_as(Role::Tab, "Library"), RiskCategory::None);
    assert_eq!(tap_as(Role::Key, "delete"), RiskCategory::None);
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
}

/// The normalisation over arbitrary Unicode (seeded): no panic, never more characters
/// than its input, idempotent (normalising twice is normalising once); the gate on the raw
/// and the normalised text never panics.
#[cfg(feature = "reference")]
#[test]
fn fuzz_the_normalisation_is_bounded_and_idempotent() {
    use jarvis_safety::reference::normalized;
    let mut r = Rng(0x5EED_0037_F1C5_0001);
    for k in 0..20_000u32 {
        let len = if k % 50 == 0 { 600 } else { r.below(24) };
        let t: String = (0..len)
            .map(|_| {
                let u = match r.below(4) {
                    0 => r.below(0x80),
                    1 => 0x2000 + r.below(0x70),
                    2 => 0x300 + r.below(0x1D00),
                    _ => r.below(0x11_0000),
                };
                char::from_u32(u32::try_from(u).unwrap()).unwrap_or('\u{FFFD}')
            })
            .collect();
        let once = normalized(&t);
        assert!(once.chars().count() <= t.chars().count(), "{t:?}");
        assert_eq!(normalized(&once), once, "{t:?}");
        if k % 10 == 0 {
            for l in [t.as_str(), once.as_str()] {
                let _ = tap_as(Role::Button, l);
                let _ = tap_as(Role::Cell, l);
            }
        }
    }
}
