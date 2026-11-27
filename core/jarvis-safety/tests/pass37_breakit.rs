#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 37: break-it tests on commit `93982bb` (look-alikes matched by character
//! sets, `jarvis_safety::sets`; the per-decision cache `classify::look_scope`; the
//! 512-character control bound).
//!
//! Each `f…` test fails today: a label a system font draws as an English risk word (or a
//! control a reasonable owner calls risky) is tapped unasked under the default policy, or
//! passes a policy that denies its category; or a decision costs far more than an ordinary
//! page of the same size. Each `sound_…` test is something tried and found to hold. None
//! uses a pair the commit states it does not fold (`rn`/`m`, `vv`/`w`, `cl`/`d`), other
//! elements' texts, or non-English words.

mod common;

use common::{app_screen, button, el, field, label};
use jarvis_proto::{Action, CompactScreen, Direction, Element, Policy, Rect, RiskCategory, Role};
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
        Decision::Deny(_) => "Deny".to_owned(),
        Decision::Confirm(c) => format!("Confirm({:?})", c.category),
    }
}

/// For each `(label, drawn as)` pair, tapped as a control of `role`: the real word must be
/// a risk word in that role (checked, so a test cannot pass vacuously); the look-alike
/// must not be tapped unasked under the default policy, and must not be allowed by a policy
/// that denies the real word's category.
fn misses(role: &Role, pairs: &[(&str, &str)]) -> Vec<String> {
    let mut wrong = Vec::new();
    for &(l, real) in pairs {
        let want = tap_as(role.clone(), real);
        assert!(
            !matches!(want, RiskCategory::None | RiskCategory::Unverified),
            "{real:?} is a risk word as a {role:?}"
        );
        let got = tap_as(role.clone(), l);
        let d = decide_as(&Policy::default(), role.clone(), l);
        if matches!(d, Decision::Allow) {
            wrong.push(format!(
                "{role:?} {l:?} (drawn {real:?}, {want:?}): category {got:?}, default policy Allow"
            ));
        }
        let deny = Policy {
            deny_categories: vec![want],
            ..Policy::default()
        };
        let d = decide_as(&deny, role.clone(), l);
        if matches!(d, Decision::Allow) {
            wrong.push(format!(
                "{role:?} {l:?} under a policy denying {want:?}: Allow (the real word is refused)"
            ));
        }
    }
    wrong
}

/// F1 (high): `@` is read as `a` "not in an email", but the email test is the whole
/// label's: one email-shaped word anywhere in the label (`x@y.z`) turns `@` off in every
/// other word. "P@y anna@mail.com" (a P2P pay button naming its payee), "Tr@nsfer to
/// bob@x.io", "Don@te · help@charity.org" draw as "Pay …", "Transfer …", "Donate …": the
/// `@` is a separator, the words read `p`, `y` / `tr`, `nsfer`, and each is tapped unasked
/// and passes a policy denying payments. The same label without the email asks. (As a
/// `Cell` the label is a long free-text row: content by its plain reading too.)
#[test]
fn f1_an_email_elsewhere_in_the_label_turns_off_at_for_a() {
    assert_eq!(tap_as(Role::Button, "P@y anna"), RiskCategory::Payment);
    assert_eq!(tap_as(Role::Button, "Tr@nsfer"), RiskCategory::Payment);
    let mut wrong = Vec::new();
    for role in [Role::Button, Role::Link] {
        wrong.extend(misses(
            &role,
            &[
                ("P@y anna@mail.com", "Pay anna@mail.com"),
                ("Tr@nsfer to bob@x.io", "Transfer to bob@x.io"),
                (
                    "Don@te \u{b7} help@charity.org",
                    "Donate \u{b7} help@charity.org",
                ),
            ],
        ));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F2 (medium): a spelled-out word (`P a y`, `D.E.L.E.T.E`, `L 0 G 0 U T` all ask) loses
/// its symbol look-alikes: `@ $ | !` are decided (letter or separator) before the
/// one-character words are joined, and a lone symbol between spaces touches no letter, so
/// it becomes a separator and the join skips it. "$ E N D" reads `END`, "P @ Y" reads `PY`,
/// "C A | |" reads `CA`, "$ E | |" reads `E`: tapped unasked, allowed under a policy
/// denying the category. The same words with digits ("5 3 L L", "P 4 Y") ask.
#[test]
fn f2_spelled_out_words_drop_their_symbol_lookalikes() {
    let wrong = misses(
        &Role::Button,
        &[
            ("$ E N D", "S E N D"),
            ("P @ Y", "P A Y"),
            ("C A | |", "C A L L"),
            ("$ E | |", "S E L L"),
            ("D E | E T E", "D E L E T E"),
        ],
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F3 (high): the narrow Unicode spaces split a word where the eye sees none. A hair space
/// (U+200A, about 1/24 em: under a point at button size), a six-per-em space (U+2006), a
/// narrow no-break space (U+202F), a thin space (U+2009) or a medium mathematical space
/// (U+205F) inside "Delete", "Pay", "Send", "Remove", "Sign Out" is a hard boundary for
/// both matchers ("De lete", "Pa y"): the control is tapped unasked and passes a policy
/// denying the category. Neither `is_invisible` nor the whitespace handling treats them as
/// joining; a renderer draws the word whole.
#[test]
fn f3_narrow_spaces_split_a_risk_word_invisibly() {
    let mut wrong = Vec::new();
    for role in [Role::Button, Role::Cell] {
        wrong.extend(misses(
            &role,
            &[
                ("De\u{200A}lete", "Delete"),
                ("Pa\u{200A}y", "Pay"),
                ("Se\u{200A}nd", "Send"),
                ("Rem\u{2006}ove", "Remove"),
                ("Del\u{202F}ete Account", "Delete Account"),
                ("Tran\u{2009}sfer", "Transfer"),
                ("Sign O\u{205F}ut", "Sign Out"),
            ],
        ));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F4 (medium): combining marks are dropped only in five blocks (U+0300.., U+1AB0..,
/// U+1DC0.., U+20D0.., U+FE20..). A nonspacing mark of another block that is not
/// alphabetic (so not "unreadable") — the Cyrillic titlo U+0483, the Hebrew accent U+05A2,
/// the Thai tone mark U+0E48, the Devanagari stress sign U+0951, the Vedic tone U+1CD0, the
/// ideographic tone mark U+302A — is a separator: "Del\u{0E48}ete" reads `del`, `ete`. The
/// mark draws as a small accent on the letter before it; the word reads "Delete". Tapped
/// unasked, allowed under a policy denying deletes.
#[test]
fn f4_combining_marks_outside_the_dropped_blocks_split_a_word() {
    let wrong = misses(
        &Role::Button,
        &[
            ("Del\u{0483}ete", "Delete"),
            ("Del\u{05A2}ete", "Delete"),
            ("Del\u{0E48}ete", "Delete"),
            ("Rem\u{0951}ove", "Remove"),
            ("Se\u{1CD0}nd", "Send"),
            ("Pa\u{302A}y", "Pay"),
        ],
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F5 (medium): accented Latin letters outside `fold_confusable`'s list are read as no
/// letter, and on a `Cell` (a free-text role) an accented letter is not a language signal,
/// so nothing asks: "Dėlete Account" (ė U+0117), "Rėmove", "Sįgn Out" (į U+012F), "Ŗeset"
/// (Ŗ U+0156), "Ŀog Out"… tapped as list rows unasked, allowed under a policy denying the
/// category (the same rows unaccented ask). On a `Button` they are `Unverified` (asked).
/// The table folds é è ê ë ē ę ě but not ė ȩ ẹ ẽ; ì í î ï but not ī į ĩ; ř but not ŕ ŗ;
/// ĺ ľ ł but not ļ; a canonical decomposition (NFD, then the dropped marks) would read all.
#[test]
fn f5_accented_letters_outside_the_fold_table_hide_a_risk_row() {
    let wrong = misses(
        &Role::Cell,
        &[
            ("D\u{117}lete Account", "Delete Account"),
            ("R\u{117}move", "Remove"),
            ("S\u{12F}gn Out", "Sign Out"),
            ("\u{156}eset", "Reset"),
            ("Cl\u{1EB9}ar History", "Clear History"),
            ("Bl\u{14F}ck", "Block"),
        ],
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F6 (medium): glyphs drawn as a letter that the sets' table does not hold: the inverted
/// exclamation mark `¡` (an `i`), the euro sign `€` (an `E`), `9` for `g`. "S¡gn Out",
/// "Subm¡t", "Un¡nstall", "D€L€T€", "Si9n Out" are tapped unasked. The table holds `!`
/// for `i` (between letters) and `3` for `e`, so the same idea is in scope; `¡` is the
/// closest of them (an `i` with the dot below the stem).
#[test]
fn f6_glyph_lookalikes_outside_the_table() {
    let wrong = misses(
        &Role::Button,
        &[
            ("S\u{A1}gn Out", "Sign Out"),
            ("Un\u{A1}nstall", "Uninstall"),
            ("D\u{20AC}L\u{20AC}T\u{20AC}", "DELETE"),
            ("Si9n Out", "Sign Out"),
        ],
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Sound (with a caveat): a text field's label and placeholder are read truncated
/// (`bounded`: the first 768 and the last 256 characters of a longer text) by the
/// credential-field test, where a pressable control's label past 512 characters is
/// `Unverified`. A field labelled with 900 characters, "Password", 900 more classifies
/// `None` (not `Credential`), yet typing into it is still asked (as `SendMessage`, by the
/// rules on text typed into an unidentified field), so a policy denying credentials does
/// not let it through unasked; the same field labelled "Password" is refused.
#[test]
fn sound_a_long_field_label_is_still_asked() {
    let pad = "lorem ipsum ".repeat(75);
    let long = format!("{pad}Password {pad}");
    assert!(long.chars().count() > 1800);
    let deny = Policy {
        deny_categories: vec![RiskCategory::Credential],
        ..Policy::default()
    };
    let typing = |l: &str| {
        let s = app_screen(vec![field(1, Role::TextField, l)]);
        let a = Action::Type {
            element: Some(1),
            text: "correct horse".to_owned(),
            submit: false,
        };
        (classify(&a, &s, None), evaluate(&deny, &a, &s, None))
    };
    let (c, d) = typing("Password");
    assert_eq!(c, RiskCategory::Credential);
    assert!(matches!(d, Decision::Deny(_)), "{}", short(&d));
    let (c, d) = typing(&long);
    assert!(
        !matches!(d, Decision::Allow),
        "a {}-character field label with Password in its middle: {c:?}, {}",
        long.chars().count(),
        short(&d)
    );
}

/// F7 (medium): on a checkout screen (a price, "Total", "Card ending"), a control that is
/// not "plainly navigation" is a payment whatever its words; "plainly navigation" is any
/// navigation word anywhere among its words. "Swipe up" asks (Payment), "Swipe up Back" and
/// "Slide Help" are tapped unasked. Pre-existing (the plain reading's `navigation`), not
/// new in `93982bb`; the sets do not lower it further ("Swipe up B4ck" asks).
#[test]
fn f7_a_navigation_word_beside_other_words_lowers_a_checkout_control() {
    let checkout = |l: &str| {
        app_screen(vec![
            label(1, "Total"),
            label(2, "$5.00"),
            label(3, "Card ending 1234"),
            button(4, l),
        ])
    };
    let a = Action::Tap { element: 4 };
    assert_eq!(
        classify(&a, &checkout("Swipe up"), None),
        RiskCategory::Payment
    );
    let mut wrong = Vec::new();
    for l in ["Swipe up Back", "Slide Help", "Swipe up \u{b7} Details"] {
        let d = evaluate(&Policy::default(), &a, &checkout(l), None);
        if matches!(d, Decision::Allow) {
            wrong.push(format!("{l:?} on a checkout: Allow"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Fits `unit` as many whole times as it goes into 512 characters (the bound the sets read).
fn fit(unit: &str) -> String {
    let mut s = String::new();
    while s.chars().count() + unit.chars().count() <= 512 {
        s.push_str(unit);
    }
    s
}

fn stacked(text: &str, with_value: bool) -> CompactScreen {
    let els: Vec<Element> = (0..800u32)
        .map(|i| {
            let mut e = button(i + 1, text);
            e.frame = Rect::new(16.0, 300.0, 361.0, 44.0);
            if with_value {
                e.value = Some(text.to_owned());
            }
            e
        })
        .chain(std::iter::once(label(9_001, "Library")))
        .collect();
    app_screen(els)
}

/// The best of three decisions of a `tap_point` over the stack, in seconds.
fn best_time(s: &CompactScreen) -> f64 {
    let at = Action::TapPoint {
        x: 196.0 / 393.0,
        y: 322.0 / 852.0,
    };
    (0..3)
        .map(|_| {
            let t = std::time::Instant::now();
            let d = evaluate(&Policy::default(), &at, s, None);
            assert!(!matches!(d, Decision::Allow));
            t.elapsed().as_secs_f64()
        })
        .fold(f64::INFINITY, f64::min)
}

/// F8 (low, cost): the sets walk the phrase tries from every boundary, and a label of
/// alternating letters and digits ("a1a1a1…", "1I1I…") makes every character a boundary
/// (a letter/digit step) whose set opens trie edges (`1` is also `i` and `l`). A
/// `tap_point` over 3,000 stacked buttons whose label and value are 512 such characters
/// (the most the sets read) took 2.0-2.1 s in a release build here, against 0.44-0.49 s for
/// the same page of ordinary words of the same length (4.2-4.5x; label only 1.0-1.3 s
/// against 0.24 s), on a loaded machine; pass 36's own hostile text ("De1ete BIock 5eII
/// |og") costs 2x. This test stacks 800 controls (debug build: 3.6-4.0x; 4.0-4.1x on 3,000).
/// A plain tap on a generic "OK" beside 20,000 such buttons (the context is read whole) took
/// 0.9-1.1 s in release against 0.21 s for ordinary words.
/// Pass 36 bounds a hostile page at 4x an ordinary one; this asserts 3x: both pages are
/// timed in the same test, best of three, so the shape of the input, not the machine's
/// load, makes the difference. Low: a decision is slow, nothing is pressed unasked.
#[test]
fn f8_alternating_letters_and_digits_cost_several_times_an_ordinary_page() {
    let ordinary = fit("Recently played songs ");
    let mut report = Vec::new();
    let mut worst = 0.0f64;
    for with_value in [false, true] {
        let base = best_time(&stacked(&ordinary, with_value));
        for unit in ["a1", "1I", "De1ete BIock 5eII |og "] {
            let hostile = fit(unit);
            let t = best_time(&stacked(&hostile, with_value));
            let ratio = t / base;
            report.push(format!(
                "value {with_value}: {unit:?} x{}: {:.1} ms, ordinary {:.1} ms ({ratio:.2}x)",
                hostile.chars().count() / unit.chars().count(),
                t * 1e3,
                base * 1e3
            ));
            worst = worst.max(ratio);
        }
    }
    println!("{}", report.join("\n"));
    assert!(worst <= 3.0, "{}", report.join("\n"));
}

/// Sound: the cancel rule. A literal harmless phrase only covers its own words: "Don't
/// Allow Delete", "OK, delete", "Resend code & pay", "Cancel Pay", "Back Pay" ask under
/// the risk word; a look-alike spelling of a harmless phrase cancels nothing ("D0n't
/// AIIow", "Rest0re Purchases", "Resend C0de", "CaII Log"); a look-alike spelling of a
/// navigation word lowers nothing on a checkout ("Swipe up B4ck").
#[test]
fn sound_the_cancel_rule_covers_only_its_own_literal_words() {
    for (l, want) in [
        ("Don't Allow Delete", RiskCategory::Delete),
        ("OK, delete", RiskCategory::Delete),
        ("Resend code & pay", RiskCategory::Payment),
        ("Cancel Pay", RiskCategory::Payment),
        ("Back Pay", RiskCategory::Payment),
        ("Dont AllowAccess", RiskCategory::SystemSetting),
        ("Don't Allow Once", RiskCategory::SystemSetting),
        ("D0n't AIIow", RiskCategory::SystemSetting),
        ("Rest0re Purchases", RiskCategory::Delete),
        ("Resend C0de", RiskCategory::SendMessage),
        ("CaII Log", RiskCategory::Call),
        ("Order History Pay", RiskCategory::Payment),
    ] {
        assert_eq!(tap_as(Role::Button, l), want, "{l:?}");
    }
    for l in [
        "Don't Allow",
        "Restore Purchases",
        "Resend Code",
        "Call Log",
    ] {
        assert_eq!(tap_as(Role::Button, l), RiskCategory::None, "{l:?}");
    }
    let s = app_screen(vec![
        label(1, "Total"),
        label(2, "$5.00"),
        label(3, "Card ending 1234"),
        button(4, "Swipe up B4ck"),
    ]);
    assert_eq!(
        classify(&Action::Tap { element: 4 }, &s, None),
        RiskCategory::Payment
    );
}

/// Sound: combinations inside the declared model ask — digits in spelled-out words, soft
/// hyphen and Mongolian vowel separator inside a word, all-caps digit words, camel joins.
#[test]
fn sound_declared_combinations_ask() {
    for (l, want) in [
        ("Tr4nsfer", RiskCategory::Payment),
        ("TR4N5F3R", RiskCategory::Payment),
        ("L 0 G 0 U T", RiskCategory::AccountChange),
        ("5 3 L L", RiskCategory::Payment),
        ("P 4 Y", RiskCategory::Payment),
        ("De\u{AD}lete", RiskCategory::Delete),
        ("Dele\u{180E}te", RiskCategory::Delete),
        ("S1gnOut", RiskCategory::AccountChange),
        ("Pay\u{2011}Now", RiskCategory::Payment),
        ("deleteaccount", RiskCategory::Delete),
        ("Subm1t", RiskCategory::SendMessage),
    ] {
        assert_eq!(tap_as(Role::Button, l), want, "{l:?}");
    }
}

/// Sound: letters outside every fold table (Lisu `ꓡ`, the Roman numeral `Ⅾ`, the Cyrillic
/// palochka `ӏ`, an accented letter on a button) make a pressable control `Unverified`:
/// asked, never allowed.
#[test]
fn sound_unfolded_letters_on_a_button_are_asked() {
    for l in [
        "DE\u{A4E1}ETE",
        "\u{216E}elete",
        "De\u{4CF}ete",
        "D\u{117}lete Account",
        "De\u{1C0}ete",
    ] {
        for role in [Role::Button, Role::Link] {
            let d = decide_as(&Policy::default(), role.clone(), l);
            assert!(
                matches!(d, Decision::Confirm(_)),
                "{role:?} {l:?}: {}",
                short(&d)
            );
        }
    }
}

/// Sound: a 513-character harmless decoy stacked with a "Delete" button at one point, in
/// either order: the point asks Delete and a policy denying deletes refuses; a control past
/// the bound alone asks `Unverified` by tap, long press and point — a category only the
/// phone may approve (`NEVER_REMOTE_APPROVE`; the request's `phone_only` flag stays false,
/// as for every category-gated question).
#[test]
fn sound_a_decoy_past_the_bound_does_not_hide_a_risky_control() {
    let deny = Policy {
        deny_categories: vec![RiskCategory::Delete],
        ..Policy::default()
    };
    let at = Action::TapPoint {
        x: 196.0 / 393.0,
        y: 322.0 / 852.0,
    };
    let frame = Rect::new(16.0, 300.0, 361.0, 44.0);
    for first in [true, false] {
        let mut a = button(1, &"x".repeat(513));
        let mut b = button(2, "Delete");
        a.frame = frame;
        b.frame = frame;
        let s = app_screen(if first { vec![a, b] } else { vec![b, a] });
        assert!(matches!(evaluate(&deny, &at, &s, None), Decision::Deny(_)));
        match evaluate(&Policy::default(), &at, &s, None) {
            Decision::Confirm(r) => assert_eq!(r.category, RiskCategory::Delete),
            other => panic!("{}", short(&other)),
        }
    }
    let mut a = button(1, &format!("Settings {}", "x".repeat(600)));
    a.frame = frame;
    let s = app_screen(vec![a]);
    for action in [
        Action::Tap { element: 1 },
        Action::LongPress {
            element: 1,
            duration_ms: 800,
        },
        at,
    ] {
        match evaluate(&Policy::default(), &action, &s, None) {
            Decision::Confirm(r) => {
                assert_eq!(r.category, RiskCategory::Unverified, "{action:?}");
                assert!(
                    jarvis_safety::NEVER_REMOTE_APPROVE.contains(&r.category),
                    "{action:?}"
                );
            }
            other => panic!("{action:?}: {}", short(&other)),
        }
    }
}

/// Sound: the per-decision cache is the decision's alone. Screens built, decided and dropped
/// one after the other (a freed screen's address is reused by the next one at once) with
/// the same element index and frame but different words decide each by its own words, in
/// both orders, many times on one thread; so do two screens alive together, and a screen
/// decided again after another one.
#[test]
fn sound_the_look_cache_never_outlives_its_screen() {
    let make = |l: &str| Box::new(app_screen(vec![button(1, l)]));
    let a = Action::Tap { element: 1 };
    let at = Action::TapPoint {
        x: 196.0 / 393.0,
        y: 60.0 / 852.0,
    };
    let mut addresses = std::collections::BTreeSet::new();
    for k in 0..200 {
        let l = if k % 2 == 0 { "Delete" } else { "Library" };
        let s = make(l);
        addresses.insert(std::ptr::from_ref::<CompactScreen>(&s) as usize);
        for action in [&a, &at] {
            let d = evaluate(&Policy::default(), action, &s, None);
            assert_eq!(
                matches!(d, Decision::Allow),
                k % 2 == 1,
                "{l:?} {action:?}: {}",
                short(&d)
            );
        }
        drop(s);
    }
    println!(
        "{} distinct screen addresses over 200 screens",
        addresses.len()
    );
    let risky = make("Delete");
    let plain = make("Library");
    for _ in 0..3 {
        assert!(!matches!(
            evaluate(&Policy::default(), &a, &risky, None),
            Decision::Allow
        ));
        assert!(matches!(
            evaluate(&Policy::default(), &a, &plain, None),
            Decision::Allow
        ));
    }
}

/// A small seeded generator (xorshift64*), so the fuzz below is reproducible.
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

const POOL: &[char] = &[
    'a',
    'D',
    'e',
    'l',
    'I',
    'i',
    't',
    'P',
    'y',
    's',
    'O',
    'o',
    'n',
    ' ',
    ' ',
    '-',
    '.',
    ',',
    '\'',
    '0',
    '1',
    '3',
    '4',
    '5',
    '7',
    '8',
    '9',
    '@',
    '$',
    '|',
    '!',
    '\u{0301}',
    '\u{0483}',
    '\u{200B}',
    '\u{200D}',
    '\u{00AD}',
    '\u{00A0}',
    '\u{200A}',
    '\u{3000}',
    '\u{FF10}',
    '\u{FF24}',
    '\u{1D7CE}',
    '\u{1D400}',
    '\u{0430}',
    '\u{0406}',
    '\u{04CF}',
    '\u{03BF}',
    '\u{0130}',
    '\u{00DF}',
    '\u{FB03}',
    '\u{1F5D1}',
    '\u{1F4B8}',
    '\u{202E}',
    '\u{066B}',
    '\u{0660}',
    '\u{2160}',
    '\u{00B2}',
    '\u{0E48}',
    '\u{E0041}',
    '\u{FE0F}',
    '\u{24D0}',
    '\u{2113}',
    '\u{A4E1}',
    '\n',
    '\t',
    '\u{2028}',
    '\u{FFFD}',
    '\u{10FFFF}',
    '\u{0000}',
    '\u{001F}',
    '\u{0085}',
];

fn random_text(r: &mut Rng) -> String {
    let len = match r.below(10) {
        0 => 500 + r.below(40),
        1..=3 => r.below(40),
        _ => r.below(8),
    };
    (0..len)
        .map(|_| {
            if r.below(4) == 0 {
                char::from_u32(u32::try_from(r.below(0x3_0000)).unwrap()).unwrap_or('x')
            } else {
                POOL[r.below(POOL.len())]
            }
        })
        .collect()
}

/// Sound: no panic. 4,000 seeded random labels, values and placeholders (the look-alike
/// characters, invisibles, combining marks, multi-character lowercase letters, fullwidth
/// and mathematical forms, controls, any code point below U+30000, lengths 0 to 540)
/// through taps, long presses, points, sideways and vertical swipes, and typing with
/// Return, on controls of several roles in an app and in Settings.
#[test]
fn sound_random_unicode_never_panics_the_gate() {
    let mut r = Rng(0x5EED_0037_0000_0001);
    let roles = [
        Role::Button,
        Role::Cell,
        Role::Link,
        Role::StaticText,
        Role::Switch,
        Role::Key,
        Role::Tab,
    ];
    for k in 0..4000u32 {
        let mut e = el(1, roles[r.below(roles.len())].clone(), &random_text(&mut r));
        if r.below(2) == 0 {
            e.value = Some(random_text(&mut r));
        }
        let mut f = field(2, Role::TextField, &random_text(&mut r));
        f.placeholder = Some(random_text(&mut r));
        let mut s = app_screen(vec![e, f, button(3, &random_text(&mut r))]);
        if k % 3 == 0 {
            s.app_bundle_id = "com.apple.Preferences".to_owned();
        }
        for a in [
            Action::Tap { element: 1 },
            Action::LongPress {
                element: 1,
                duration_ms: 600,
            },
            Action::TapPoint {
                x: 0.5,
                y: 50.0 / 852.0,
            },
            Action::Swipe {
                direction: Direction::Left,
                element: Some(1),
            },
            Action::Swipe {
                direction: Direction::Up,
                element: Some(1),
            },
            Action::Type {
                element: Some(2),
                text: random_text(&mut r),
                submit: true,
            },
        ] {
            let _ = classify(&a, &s, None);
            let _ = evaluate(&Policy::default(), &a, &s, None);
        }
    }
}
