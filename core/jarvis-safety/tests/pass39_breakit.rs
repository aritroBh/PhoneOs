#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 39: break-it tests on commit `384eb5c` (pass 38's rule: inside a word that
//! reads as Latin, a character the tables do not know makes a pressable control
//! `Unverified`; the compatibility fold; soft punctuation; the email exemption removed).
//!
//! Each `f…` test fails today: a label a person reads as an English risk word is pressed
//! unasked under the default policy, or passes a policy that denies its category. Each
//! `sound_…` test is something tried and found to hold. None uses `rn`/`m`, `vv`/`w`,
//! `cl`/`d`, other elements' texts, non-English words or the six hard whitespace
//! characters.

mod common;

use common::{app_screen, el, field};
use jarvis_proto::{
    Action, AppRule, CompactScreen, Direction, Element, Policy, Rect, RiskCategory, Role,
    TrustLevel,
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

fn deny(c: RiskCategory) -> Policy {
    Policy {
        deny_categories: vec![c],
        ..Policy::default()
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
        if matches!(
            decide_as(&Policy::default(), role.clone(), l),
            Decision::Allow
        ) {
            wrong.push(format!(
                "{role:?} {l:?} (drawn {real:?}, {want:?}): category {got:?}, default policy Allow"
            ));
        }
        if matches!(decide_as(&deny(want), role.clone(), l), Decision::Allow) {
            wrong.push(format!(
                "{role:?} {l:?} under a policy denying {want:?}: Allow (the real word is refused)"
            ));
        }
    }
    wrong
}

/// The labels of `labels` allowed (tapped unasked) as a control of `role`.
fn allowed(role: &Role, labels: &[&str]) -> Vec<String> {
    labels
        .iter()
        .filter(|l| {
            matches!(
                decide_as(&Policy::default(), role.clone(), l),
                Decision::Allow
            )
        })
        .map(|l| format!("{role:?} {l:?}: Allow"))
        .collect()
}

const ROLES: [Role; 3] = [Role::Button, Role::Link, Role::Cell];

// ---------------------------------------------------------------------------------------
// Findings.
// ---------------------------------------------------------------------------------------

/// F1 (medium): CJK strokes and Kangxi / CJK radicals are "pictographs" to the rule
/// (General_Category So, East_Asian_Width W: `PICTOGRAPH_RANGES`), so at a word's edge
/// they are known, and the sets do not read them. U+31D1 `㇑` CJK STROKE S and U+2F01
/// `⼁` KANGXI RADICAL LINE are drawn as one plain vertical stroke (U+2F01 is the
/// compatibility form of U+4E28 `丨`, a letter, which the rule does treat as an intruder:
/// "Ca丨丨" asks); U+31D7 `㇗` CJK STROKE SZ and U+31C4 `㇄` are drawn as an `L`.
/// Inputs: "Ca\u{31D1}\u{31D1}" (Call), "Ca\u{2F01}\u{2F01}", "Se\u{31D1}\u{31D1}" (Sell),
/// "Uninsta\u{31D1}\u{31D1}" (Uninstall), "\u{31D1}og Out", "\u{31D7}og Out",
/// "\u{31C4}og Out" (Log Out), "Ca\u{31D1}\u{31D1} 911" (Call 911) as a Button, Link or
/// Cell: category `None`, tapped unasked, and allowed under a policy denying Call / Payment
/// / Delete / AccountChange. Should: inside a word that reads as Latin, a CJK stroke or
/// radical (any wide So character whose compatibility form is a letter, or which is no
/// emoji) is an intruder at the edge too: `Unverified`. On a phone the strokes are drawn
/// full-width ("Ca | |"); a web page or message can put them on a button the owner reads
/// as "Call".
#[test]
fn f1_cjk_strokes_and_radicals_complete_a_risk_word_at_its_edge() {
    assert_ne!(
        tap_as(Role::Button, "Ca\u{4E28}\u{4E28}"),
        RiskCategory::None
    );
    let pairs = [
        ("Ca\u{31D1}\u{31D1}", "Call"),
        ("Ca\u{2F01}\u{2F01}", "Call"),
        ("Se\u{31D1}\u{31D1}", "Sell"),
        ("Uninsta\u{31D1}\u{31D1}", "Uninstall"),
        ("\u{31D1}og Out", "Log Out"),
        ("\u{31D7}og Out", "Log Out"),
        ("\u{31C4}og Out", "Log Out"),
        ("Ca\u{31D1}\u{31D1} 911", "Call 911"),
    ];
    let mut wrong = Vec::new();
    for role in &ROLES {
        wrong.extend(misses(role, &pairs));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F1, enumerated: every code point of CJK Radicals Supplement (U+2E80..U+2EFF), Kangxi
/// Radicals (U+2F00..U+2FDF) and CJK Strokes (U+31C0..U+31EF), 400 in all (the unassigned
/// ones ask already), as the last two letters of "Call" on a Button: none may be tapped
/// unasked (366 are today).
#[test]
fn f1_every_cjk_stroke_and_radical_at_a_word_edge_asks() {
    let mut wrong = Vec::new();
    let mut tried = 0;
    for u in (0x2E80..=0x2FDF).chain(0x31C0..=0x31EF) {
        let Some(c) = char::from_u32(u) else { continue };
        tried += 1;
        let l = format!("Ca{c}{c}");
        if matches!(
            decide_as(&Policy::default(), Role::Button, &l),
            Decision::Allow
        ) {
            wrong.push(format!("U+{u:04X}"));
        }
    }
    println!("{tried} code points tried, {} allowed", wrong.len());
    assert!(
        wrong.is_empty(),
        "{} allowed: {}",
        wrong.len(),
        wrong.iter().take(40).cloned().collect::<Vec<_>>().join(" ")
    );
}

/// F2 (low): the compatibility squares whose decomposition is several ASCII letters
/// (U+33D2 `㏒` SQUARE LOG = "log", U+3388 `㎈` = "cal", U+33CC `㏌` = "in"; 114 of them
/// are in `PICTOGRAPH_RANGES`) are not folded (`COMPAT_FOLD` takes exactly one letter),
/// not intruders (at a word's edge they are pictographs; as a word of their own the word
/// has no letter and is not read), and not read by the sets. Inputs: "\u{33D2} Out",
/// "\u{33D2}out", "\u{33D2} Off" (Log Out / Logout / Log Off: AccountChange),
/// "\u{3388}l Mom" (Call Mom), "\u{33CC}vite" (Invite: SendMessage) on a Button, Link or
/// Cell: `None`, tapped unasked, allowed under a deny of the real category. Should: fold
/// such a character to its letters (both readings), or make it an intruder. On a phone
/// `㏒` is drawn as the three letters "log" condensed into one square: a person reads
/// "log Out" (noticeably narrow, hence low).
#[test]
fn f2_multi_letter_compatibility_squares_are_read_as_nothing() {
    let pairs = [
        ("\u{33D2} Out", "Log Out"),
        ("\u{33D2}out", "Logout"),
        ("\u{33D2} Off", "Log Off"),
        ("\u{3388}l Mom", "Call Mom"),
        ("\u{33CC}vite", "Invite"),
    ];
    let mut wrong = Vec::new();
    for role in &ROLES {
        wrong.extend(misses(role, &pairs));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F3 (high): the intruder rule is read for pressable roles only, and `StaticText` and
/// `Image` are not pressable — but a tap on one is a tap like any other, and the gate does
/// read their words (a StaticText "Delete Account" asks `Delete`, "Pay now" `Payment`,
/// "Call" `Call`). Inputs: "De\u{1C0}ete Account", "Ca\u{2223}\u{2223}", "P\u{FE6B}y now"
/// (pass 38's F2/F3 intruders) tapped as a StaticText or an Image: `None`, tapped unasked
/// and allowed under a policy denying Delete / Call / Payment. Should: what the words of a
/// tapped element are read for, the intruder rule is read for (`Unverified` at least). On
/// a phone a web page's clickable text (`<div onclick>`, a `<span>` in a button-like box)
/// and an image with alt text are exposed as StaticText/Image; pass 38's F2 text "Deǀete
/// Account" is exactly such a row.
#[test]
fn f3_static_text_and_images_skip_the_intruder_rule() {
    let pairs = [
        ("De\u{1C0}ete Account", "Delete Account"),
        ("Ca\u{2223}\u{2223}", "Call"),
        ("P\u{FE6B}y now", "Pay now"),
        ("Se\u{1C1}", "Sell"),
    ];
    let mut wrong = Vec::new();
    for role in [Role::StaticText, Role::Image] {
        wrong.extend(misses(&role, &pairs));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F4 (high where the owner marked the app Trusted): `Trusted` lets `Unverified` through
/// unasked (`TRUSTED_ALLOWED`), so every label pass 38 made `Unverified` to catch a
/// disguised risk word — "De\u{1C0}ete Account", "P\u{FE6B}y", "Ca\u{2223}\u{2223}" — is
/// pressed unasked in a Trusted app (tap, long press and tap_point), and **allowed under a
/// policy that denies Delete / Payment / Call**, while "Delete Account", "Pay", "Call"
/// there ask or are refused. The sixth review closed the same hole for runner text
/// ("riding on `Unverified`, which Trusted allows, sent a forgeable screen's tap unasked").
/// Should: the `Unverified` of an intruder (a label whose drawing the tables do not know)
/// is not one Trusted relaxes — asked, phone only — and while the owner denies any
/// category it is refused or at least asked. On a phone: the owner trusts a browser or a
/// messaging app to cut prompts; a page or message in it shows "Deǀete Account".
#[test]
fn f4_a_trusted_app_presses_an_intruder_unasked_even_under_a_deny() {
    let trusted = |deny: Vec<RiskCategory>| Policy {
        app_rules: vec![AppRule::new("com.example.app", TrustLevel::Trusted)],
        deny_categories: deny,
        ..Policy::default()
    };
    let mut wrong = Vec::new();
    for (l, real, cat) in [
        (
            "De\u{1C0}ete Account",
            "Delete Account",
            RiskCategory::Delete,
        ),
        ("P\u{FE6B}y", "Pay", RiskCategory::Payment),
        ("Ca\u{2223}\u{2223}", "Call", RiskCategory::Call),
    ] {
        for role in [Role::Button, Role::Cell] {
            // The real word under the same policies: asked, and refused under the deny.
            assert!(!matches!(
                decide_as(&trusted(vec![]), role.clone(), real),
                Decision::Allow
            ));
            assert!(matches!(
                decide_as(&trusted(vec![cat]), role.clone(), real),
                Decision::Deny(_)
            ));
            let s = app_screen(vec![el(1, role.clone(), l)]);
            let f = s.elements[0].frame;
            let point = Action::TapPoint {
                x: (f.x + f.width / 2.0) / s.size.width,
                y: (f.y + f.height / 2.0) / s.size.height,
            };
            for (an, a) in [
                ("tap", Action::Tap { element: 1 }),
                (
                    "long press",
                    Action::LongPress {
                        element: 1,
                        duration_ms: 800,
                    },
                ),
                ("tap_point", point),
            ] {
                for (pn, p) in [
                    ("trusted", trusted(vec![])),
                    ("trusted, denying", trusted(vec![cat])),
                ] {
                    if matches!(evaluate(&p, &a, &s, None), Decision::Allow) {
                        wrong.push(format!("{role:?} {l:?} {an} ({pn} {cat:?}): Allow"));
                    }
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F5 (medium): a list row's words are counted after punctuation splits them, so a price
/// with a decimal point or thousands separator, or a hyphenated phone number, is two or
/// three words: a three-word command row becomes a "long free-text row" (more than
/// `MAX_ROW_LABEL_WORDS`), its first comma segment is not a whole action phrase, and it is
/// content. Inputs (Cell): "Pay $12.50 now", "Buy now $4.99", "Transfer \u{A3}1,000 now",
/// "Pay rent \u{A3}950.00", "Tip $2.50 now", "Call 1-800-555-0199": `None`, tapped unasked,
/// allowed under a deny of Payment / Purchase / Call; the same rows written "Pay $1250
/// now", "Pay \u{A3}5.00", "Call 555-0100" ask. Should: an amount or a phone number is one
/// word where a row's words are counted (and the content-row rule does not drop the money
/// reading of a three-word row). On a phone: a bills list ("Pay $12.50 now"), a support
/// page row ("Call 1-800-555-0199"), an in-app store row ("Buy now $4.99").
#[test]
fn f5_a_price_or_phone_number_turns_a_short_command_row_into_content() {
    for asks in ["Pay $1250 now", "Pay \u{A3}5.00", "Call 555-0100"] {
        assert_ne!(tap_as(Role::Cell, asks), RiskCategory::None, "{asks:?}");
    }
    let rows = [
        ("Pay $12.50 now", RiskCategory::Payment),
        ("Buy now $4.99", RiskCategory::Purchase),
        ("Transfer \u{A3}1,000 now", RiskCategory::Payment),
        ("Pay rent \u{A3}950.00", RiskCategory::Payment),
        ("Tip $2.50 now", RiskCategory::Payment),
        ("Call 1-800-555-0199", RiskCategory::Call),
    ];
    let mut wrong = Vec::new();
    for (l, cat) in rows {
        // As a Button the same words ask under that category.
        assert_eq!(tap_as(Role::Button, l), cat, "{l:?}");
        if matches!(
            decide_as(&Policy::default(), Role::Cell, l),
            Decision::Allow
        ) {
            wrong.push(format!("Cell {l:?}: default policy Allow"));
        }
        if matches!(decide_as(&deny(cat), Role::Cell, l), Decision::Allow) {
            wrong.push(format!("Cell {l:?}: Allow under a deny of {cat:?}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F6 (low): the readings carried beside a tap's category (`reading_risks`, and pass 37
/// F5's `Unverified` beside a category read only after the normalisation) are read for
/// `Tap` and `LongPress` only. A `tap_point` at the same control's centre carries only the
/// riskiest category: "S\u{117}nd" (a Button) is asked `SendMessage` **phone only** when
/// tapped, but `SendMessage` approvable remotely by `tap_point` at its centre. Should: a
/// point whose target is one control carries what a tap of that control carries. On a
/// phone a vision-fallback point over the button turns a phone-only question into one a
/// paired device may approve.
#[test]
fn f6_tap_point_drops_what_a_tap_of_the_same_control_carries() {
    let s = app_screen(vec![el(1, Role::Button, "S\u{117}nd")]);
    let f = s.elements[0].frame;
    let point = Action::TapPoint {
        x: (f.x + f.width / 2.0) / s.size.width,
        y: (f.y + f.height / 2.0) / s.size.height,
    };
    let ask = |a: &Action| match evaluate(&Policy::default(), a, &s, None) {
        Decision::Confirm(c) => (c.category, c.phone_only),
        other => panic!("{other:?}"),
    };
    let tap = ask(&Action::Tap { element: 1 });
    assert_eq!(tap, (RiskCategory::SendMessage, true));
    assert_eq!(
        ask(&point),
        tap,
        "tap_point at the centre of the same button"
    );
}

/// F7 (medium): a word with no ASCII letter whose letters are Cyrillic look-alikes plus a
/// letter of a script the tables do not read is neither "Latin" (so no intruder) nor, on a
/// long row, unreadable (a content row skips that signal). "\u{405}\u{435}\u{3163}\u{3163}"
/// (Cyrillic `Ѕе` + U+3163 `ㅣ` HANGUL LETTER I, drawn as a vertical stroke): the Cell
/// "\u{405}\u{435}\u{3163}\u{3163} now, best price today guaranteed" is `None` and allowed
/// under a deny of Payment, while "Sell now, best price today guaranteed" asks `Payment`
/// (its first segment is a command). As a short Button or Cell the same word asks
/// (`Unverified`). Should: a word whose letters fold to Latin except for letters drawn as
/// strokes is read as Latin (the foreign letter an intruder), on every row length. On a
/// phone a marketplace row reads "Ѕеㅣㅣ now, …".
#[test]
fn f7_a_mixed_script_word_hides_a_command_on_a_long_row() {
    let real = "Sell now, best price today guaranteed";
    assert_eq!(tap_as(Role::Cell, real), RiskCategory::Payment);
    let l = "\u{405}\u{435}\u{3163}\u{3163} now, best price today guaranteed";
    let mut wrong = Vec::new();
    if matches!(
        decide_as(&Policy::default(), Role::Cell, l),
        Decision::Allow
    ) {
        wrong.push("default policy Allow".to_owned());
    }
    if matches!(
        decide_as(&deny(RiskCategory::Payment), Role::Cell, l),
        Decision::Allow
    ) {
        wrong.push("Allow under a deny of Payment".to_owned());
    }
    assert!(wrong.is_empty(), "Cell {l:?}: {}", wrong.join(", "));
}

/// F8 (low; debug builds only): the "Amounts on screen" slot of a typed submit's question
/// is not bounded when an amount sits inside one long run of text: a Button labelled
/// `"x" * 300 + "\u{A3}1"` (or `$1`, 302 characters, under `MAX_CONTROL_LABEL_CHARS`)
/// beside a focused field, and `type "hello"` with submit, trips `fit_slot`'s
/// `debug_assert` ("a summary slot exceeds its allotment (323 > 200)"): the gate panics
/// (found by the arbitrary-Unicode fuzz below; from about 175 characters on). In a release
/// build `hard_cut` cuts the slot instead. Should: every slot renderer fits its allotment
/// by construction, as `fit_slot` documents (the amount shown as its amount alone). On a
/// phone a debug build (the simulator runs, the test suite) is stalled by any page that
/// shows a long unbroken text holding a price.
#[test]
fn f8_a_long_amount_text_overflows_the_question_slot() {
    let mut wrong = Vec::new();
    for cur in ["\u{A3}", "$", "\u{20BE}"] {
        let label = format!("{}{cur}1", "x".repeat(300));
        let l = label.clone();
        let panicked = std::panic::catch_unwind(move || {
            let s = app_screen(vec![
                el(1, Role::Button, &l),
                field(2, Role::TextField, "Note"),
            ]);
            let _ = evaluate(
                &Policy::default(),
                &Action::Type {
                    element: Some(2),
                    text: "hello".to_owned(),
                    submit: true,
                },
                &s,
                None,
            );
        })
        .is_err();
        if panicked {
            wrong.push(format!("{cur}: panicked"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("; "));
}

// ---------------------------------------------------------------------------------------
// Sound.
// ---------------------------------------------------------------------------------------

/// Sound: the fold table's letters drawn as `l` or `I` read as `l` (Greek capital iota
/// and its accented forms, Cyrillic `І Ї Ӏ`, `ʟ`, `Ł ł`, `Ɪ`): of the 132 letters folded to
/// another ASCII letter, none drawn as `l`/`I` reads as harmless in "De?ete", "Ca??".
#[test]
fn sound_letters_drawn_as_l_read_as_l() {
    for c in [
        '\u{399}', '\u{38A}', '\u{3AA}', '\u{406}', '\u{407}', '\u{4C0}', '\u{29F}', '\u{141}',
        '\u{142}', '\u{A7AE}', '\u{1FD8}', '\u{1F38}',
    ] {
        for (l, real) in [
            (format!("De{c}ete Account"), RiskCategory::Delete),
            (format!("Ca{c}{c}"), RiskCategory::Call),
        ] {
            assert_eq!(tap_as(Role::Button, &l), real, "{l:?}");
        }
    }
}

/// Sound: leet and punctuation spellings a person reads at a glance ask: `|` `!` `1` for
/// `l`, `€` for `e`, `5`/`$` for `s`, punctuation between the letters.
#[test]
fn sound_leet_and_punctuated_spellings_ask() {
    let labels = [
        "Ca||",
        "De|ete",
        "De!ete",
        "De1ete",
        "5\u{20AC}||",
        "$\u{20AC}||",
        "5ell",
        "$ell",
        "Se||",
        "\u{405}\u{435}||",
        "Ca.ll",
        "Ca_ll",
        "Ca-ll",
        "Ca/ll",
        "Ca:ll",
        "Ca*ll",
        "Ca#ll",
        "C.a.l.l",
        "P.a.y",
        "S-e-n-d",
        "Sen.d",
        "#call",
        "C@ll",
    ];
    let mut wrong = Vec::new();
    for role in &ROLES {
        wrong.extend(allowed(role, &labels));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Sound: an intruder asks on every pointing action at the control (tap, long press,
/// `tap_point` at its centre) under the default policy and under a policy denying the
/// word's category; a label that holds an intruder AND a definite risk word is refused
/// under that word's deny (deny wins).
#[test]
fn sound_an_intruder_asks_on_every_pointing_action_and_a_read_word_is_still_denied() {
    for (l, cat) in [
        ("De\u{1C0}ete Account", RiskCategory::Delete),
        ("P\u{FE6B}y", RiskCategory::Payment),
        ("Ca\u{2223}\u{2223}", RiskCategory::Call),
    ] {
        for role in [Role::Button, Role::Cell, Role::Link, Role::Switch] {
            let s = app_screen(vec![el(1, role.clone(), l)]);
            let f = s.elements[0].frame;
            for a in [
                Action::Tap { element: 1 },
                Action::LongPress {
                    element: 1,
                    duration_ms: 800,
                },
                Action::TapPoint {
                    x: (f.x + f.width / 2.0) / s.size.width,
                    y: (f.y + f.height / 2.0) / s.size.height,
                },
            ] {
                for p in [Policy::default(), deny(cat)] {
                    assert!(
                        !matches!(evaluate(&p, &a, &s, None), Decision::Allow),
                        "{role:?} {l:?} {a:?}"
                    );
                }
            }
        }
    }
    for (l, cat) in [
        ("Delete Ac\u{2223}count", RiskCategory::Delete),
        ("Pay n\u{1C0}w", RiskCategory::Payment),
    ] {
        assert!(
            matches!(decide_as(&deny(cat), Role::Button, l), Decision::Deny(_)),
            "{l:?}"
        );
    }
}

/// Sound (scoping): one hostile node does not make the rest of the screen ask. Beside a
/// Button "De\u{1C0}ete Account" and a focused field labelled "Pass\u{2223}word", a Button
/// "Next", a Cell "Wi-Fi" and a Link "Help" are tapped unasked.
#[test]
fn sound_one_intruder_does_not_make_the_screen_unverified() {
    let mut f = field(2, Role::TextField, "Pass\u{2223}word");
    f.placeholder = Some("P\u{FE6B}y".to_owned());
    let s = app_screen(vec![
        el(1, Role::Button, "De\u{1C0}ete Account"),
        f,
        el(3, Role::Button, "Next"),
        el(4, Role::Cell, "Wi-Fi"),
        el(5, Role::Link, "Help"),
    ]);
    for i in 3..=5 {
        let d = evaluate(&Policy::default(), &Action::Tap { element: i }, &s, None);
        assert!(matches!(d, Decision::Allow), "element {i}: {d:?}");
    }
}

/// Sound (usability): ordinary hyphenated, dotted, slashed and underscored labels gain no
/// category from the look-alike sets beyond what their own words give (the soft
/// punctuation adds nothing): for each, the sets' reading is the plain reading or
/// nothing. Measured beside it and printed: how many ask at all (by their own words:
/// "Sell-by date", "Post-it", "Check-out time", "call-to-action" ask as before).
#[cfg(feature = "reference")]
#[test]
fn sound_soft_punctuation_adds_nothing_to_ordinary_labels() {
    let corpus = [
        "e-mail",
        "E-mail",
        "check-in",
        "Check-in",
        "log-in",
        "sign-up",
        "Sign-up",
        "opt-out",
        "Opt-in",
        "add-on",
        "Add-ons",
        "follow-up",
        "drop-down",
        "pop-up",
        "Pop-up blocker",
        "sign-in",
        "Sign-in options",
        "Clean-up",
        "Set-up",
        "Set-up guide",
        "Back-up",
        "Look-up",
        "Stand-by",
        "Mark-up",
        "Roll-out",
        "Cut-off",
        "Hand-off",
        "Lay-out",
        "Play-off",
        "Kick-off",
        "Walk-in",
        "Drive-thru",
        "Wi-Fi",
        "README.md",
        "IMG_0042.HEIC",
        "Invoice_2024.pdf",
        "Version 17.4.1",
        "10:45 a.m.",
        "9:30",
        "https://www.example.com",
        "apple.com/shop",
        "To-do",
        "To-do list",
        "Self-service",
        "Re-try",
        "Pre-install",
        "Ex-wife",
        "X-ray",
        "T-Mobile",
        "Jean-Luc",
        "Mary-Kate",
        "Hewlett-Packard",
        "Rolls-Royce",
        "Mercedes-Benz",
        "Coca-Cola",
        "Spider-Man",
        "co-op",
        "co-worker",
        "e-commerce",
        "e-banking",
        "e-wallet",
        "e-signature",
        "e-sign",
        "Sell-by date",
        "Post-it",
        "Post-op",
        "Check-out time",
        "call-to-action",
        "Send-off",
        "e-book",
        "Co-pay",
        "Sell-through",
        "pay-per-view",
        "Re-call",
        "Yes/No",
        "Play/Pause",
        "N/A",
        "AT&T",
        "Q&A",
        "U.S.",
        "p.m.",
    ];
    let mut wrong = Vec::new();
    let mut asking = Vec::new();
    for l in corpus {
        for role in ROLES {
            let s = app_screen(vec![el(1, role.clone(), l)]);
            let r = jarvis_safety::reference::tap_reading_risks(&Action::Tap { element: 1 }, &s);
            if let [plain, sets] = r[..] {
                if plain != sets {
                    wrong.push(format!("{role:?} {l:?}: plain {plain:?}, sets {sets:?}"));
                }
            }
            if role == Role::Button
                && !matches!(
                    evaluate(&Policy::default(), &Action::Tap { element: 1 }, &s, None),
                    Decision::Allow
                )
            {
                asking.push(l);
            }
        }
    }
    println!(
        "{} labels; {} ask as a Button by their own words: {asking:?}",
        corpus.len(),
        asking.len()
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Sound (usability): addresses, handles and "@ price" texts gain no category from the
/// `@` beyond their own words ("pay@stripe.com", "@paypal", "@delete_me" ask by their
/// words, as without the `@`).
#[cfg(feature = "reference")]
#[test]
fn sound_addresses_and_handles_gain_nothing_from_the_at() {
    let corpus = [
        "@anna",
        "@sales",
        "@billing",
        "@callme",
        "anna@gmail.com",
        "billing@acme.io",
        "user@host",
        "root@192.168.0.1",
        "@alice@mastodon.social",
        "@bob@mstdn.jp",
        "Meet @ 5pm",
        "Lunch @ Noon",
        "liam.o@icloud.com",
        "support@apple.com",
        "noreply@github.com",
        "team@slack.com",
    ];
    let mut wrong = Vec::new();
    for l in corpus {
        for role in ROLES {
            if tap_as(role.clone(), l) != RiskCategory::None {
                wrong.push(format!("{role:?} {l:?}: {:?}", tap_as(role.clone(), l)));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
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
    /// Any scalar value: one in four ASCII, the rest anywhere up to U+10FFFF (surrogates
    /// replaced).
    fn any_char(&mut self) -> char {
        if self.below(4) == 0 {
            char::from(u8::try_from(0x20 + self.below(95)).unwrap())
        } else {
            let u = u32::try_from(self.below(0x11_0000)).unwrap();
            char::from_u32(u).unwrap_or('\u{2223}')
        }
    }
}

/// Sound: arbitrary Unicode (every plane, surrogates replaced) in labels, values and
/// placeholders, mixed with risk words and intruders, through every action kind: no panic,
/// and no label holding "Delete" (with an intruder) is ever allowed on a Button. (A typed
/// submit beside a label longer than 150 characters is left to F8.)
#[test]
fn sound_arbitrary_unicode_never_panics() {
    let mut r = Rng(0x5EED_0039_0000_0001);
    for k in 0..4000u32 {
        let len = if k % 50 == 0 { 600 } else { r.below(24) };
        let mut t: String = (0..len).map(|_| r.any_char()).collect();
        if k % 3 == 0 {
            t.insert_str(0, "Delete \u{1C0} ");
        }
        let role = [
            Role::Button,
            Role::Cell,
            Role::Tab,
            Role::StaticText,
            Role::Key,
        ][k as usize % 5]
            .clone();
        let mut e = el(1, role.clone(), &t);
        if k % 2 == 0 {
            e.value = Some(t.clone());
        }
        let mut f = field(2, Role::TextField, &t);
        f.placeholder = Some(t.clone());
        let s = app_screen(vec![e, f, el(3, Role::Key, &t)]);
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
            Action::Scroll {
                direction: Direction::Down,
                element: None,
            },
            Action::Type {
                element: Some(2),
                text: t.clone(),
                submit: true,
            },
            Action::Type {
                element: None,
                text: t.clone(),
                submit: false,
            },
        ] {
            if matches!(a, Action::Type { submit: true, .. }) && t.chars().count() > 150 {
                continue;
            }
            let _ = classify(&a, &s, None);
            let d = evaluate(&Policy::default(), &a, &s, None);
            if k % 3 == 0 && role == Role::Button && matches!(a, Action::Tap { .. }) {
                assert!(!matches!(d, Decision::Allow), "{t:?}");
            }
        }
    }
}

/// Sound (cost): 3,000 stacked buttons whose labels are one long word that the intruder
/// rule must read to its end (look-alike and compatibility letters, marks, known symbols,
/// the unknown character last) decide a tap on "OK", a point over the stack and a typed
/// submit within 6x a same-size page of ASCII words of the same length (best of two;
/// times printed).
#[test]
fn sound_a_page_of_long_intruder_words_costs_what_an_ordinary_one_does() {
    let unit = "C\u{430}\u{1D5C5}\u{301}\u{2C7C}\u{FF4C}\u{20AC}\u{24DB}\u{1E37}";
    let hostile: String = unit.repeat(40) + "\u{2223}";
    let ordinary: String = "abcdefghijkl".repeat(30);
    let page = |text: &str| -> CompactScreen {
        let els: Vec<Element> = (0..3000u32)
            .map(|i| {
                let mut e = el(i + 1, Role::Button, text);
                e.frame = Rect::new(16.0, 300.0, 361.0, 44.0);
                e
            })
            .chain(std::iter::once({
                let mut f = field(3001, Role::TextField, text);
                f.placeholder = Some(text.to_owned());
                f
            }))
            .chain(std::iter::once(el(3002, Role::Button, "OK")))
            .collect();
        app_screen(els)
    };
    let time = |s: &CompactScreen| -> f64 {
        (0..2)
            .map(|_| {
                let t = std::time::Instant::now();
                for a in [
                    Action::Tap { element: 3002 },
                    Action::TapPoint {
                        x: 196.0 / 393.0,
                        y: 322.0 / 852.0,
                    },
                    Action::Type {
                        element: Some(3001),
                        text: "hello".to_owned(),
                        submit: true,
                    },
                ] {
                    let _ = evaluate(&Policy::default(), &a, s, None);
                }
                t.elapsed().as_secs_f64()
            })
            .fold(f64::INFINITY, f64::min)
    };
    assert!(jarvis_safety_has_intruder(&hostile));
    let (s0, s1) = (page(&ordinary), page(&hostile));
    let (t0, t1) = (time(&s0), time(&s1));
    println!(
        "3,000 x {} chars: ordinary {t0:.3} s, intruder words {t1:.3} s, ratio {:.2}",
        hostile.chars().count(),
        t1 / t0
    );
    assert!(t1 <= 6.0 * t0.max(1e-3), "{t1} vs {t0}");
}

/// Whether the label holds an intruder, by its effect (a Button with it is `Unverified`).
fn jarvis_safety_has_intruder(l: &str) -> bool {
    tap_as(Role::Button, l) == RiskCategory::Unverified
}

// ---------------------------------------------------------------------------------------
// Undecided (printed, not asserted).
// ---------------------------------------------------------------------------------------

/// Undecided: spellings whose reading depends on how a person reads them or how iOS draws
/// a character, printed with their category as a Button and a Cell: leet digraphs for D
/// (`|)`, `[)`); the small capital I and Greek iota (fold to `i`, drawn as a short stroke);
/// characters the normalisation drops although some fonts draw them (Arabic pound /
/// piastre marks above U+0890/U+0891, end of ayah U+06DD, Kaithi number sign U+110BD);
/// circle emoji for `o`; the CJK mouth radical for `O`; a pure `Unverified` under a policy
/// denying a category (asked, not refused: unlike an incomplete screen).
#[test]
fn undecided_spellings_are_printed() {
    for l in [
        "|)elete",
        "[)elete",
        "|)3|_3+3",
        "De\u{26A}ete Account",
        "De\u{3B9}ete Account",
        "Ca\u{26A}\u{26A}",
        "De\u{890}ete",
        "Ca\u{891}\u{891}",
        "L\u{6DD}g Out",
        "Ca\u{110BD}\u{110BD}",
        "\u{2B55}rder",
        "\u{2F1D}rder",
        "Pos\u{2F17}",
        "\u{2F15}all",
    ] {
        println!(
            "{l:?}: Button {:?}, Cell {:?}",
            tap_as(Role::Button, l),
            tap_as(Role::Cell, l)
        );
    }
    let d = decide_as(
        &deny(RiskCategory::Delete),
        Role::Button,
        "De\u{1C0}ete Account",
    );
    println!("\"De\\u{{1C0}}ete Account\" under a deny of Delete: {d:?}");
}
