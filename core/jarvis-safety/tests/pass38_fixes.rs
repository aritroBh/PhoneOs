#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 38: inside a word that reads as Latin, a character the tables do not know
//! asks (the intruder rule), the compatibility fold, `@` read both ways, soft line breaks,
//! soft punctuation inside a word, digits beside letters. The reviewer's own tests are
//! `pass38_breakit.rs`.

#[cfg(feature = "reference")]
#[path = "generated/assigned.rs"]
mod assigned;
mod common;
#[cfg(feature = "reference")]
#[path = "generated/emoji.rs"]
mod emoji;

use common::{app_screen, button, el, field, label};
use jarvis_proto::{Action, Policy, RiskCategory, Role};
use jarvis_safety::{classify, evaluate, Decision};

fn tap_as(role: Role, l: &str) -> RiskCategory {
    let s = app_screen(vec![el(1, role, l)]);
    classify(&Action::Tap { element: 1 }, &s, None)
}

fn decide_as(policy: &Policy, role: Role, l: &str) -> Decision {
    let s = app_screen(vec![el(1, role, l)]);
    evaluate(policy, &Action::Tap { element: 1 }, &s, None)
}

/// Each `(label, drawn as)`: the drawn word's category (checked to be a risk), the
/// look-alike not allowed by the default policy nor by a policy denying that category.
fn misses(role: &Role, pairs: &[(&str, &str)]) -> Vec<String> {
    let mut wrong = Vec::new();
    for &(l, real) in pairs {
        let want = tap_as(role.clone(), real);
        assert!(
            !matches!(want, RiskCategory::None | RiskCategory::Unverified),
            "{real:?} is a risk word as a {role:?}"
        );
        if matches!(
            decide_as(&Policy::default(), role.clone(), l),
            Decision::Allow
        ) {
            wrong.push(format!("{role:?} {l:?} (drawn {real:?}): default Allow"));
        }
        let deny = Policy {
            deny_categories: vec![want],
            ..Policy::default()
        };
        if matches!(decide_as(&deny, role.clone(), l), Decision::Allow) {
            wrong.push(format!(
                "{role:?} {l:?} under a policy denying {want:?}: Allow"
            ));
        }
    }
    wrong
}

/// Every label asks (is not allowed by the default policy) as a control of `role`.
fn all_ask(role: &Role, labels: &[&str]) -> Vec<String> {
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

const PRESSABLE: [Role; 5] = [
    Role::Button,
    Role::Cell,
    Role::Link,
    Role::Switch,
    Role::Tab,
];

/// F1: `@` is read both ways everywhere (an address's too): plain as a separator, by the
/// sets as `a` or `e`. A risk word with `@` asks however the word ends; an address alone
/// still asks nothing unless its letters spell a risk through the `@`.
#[test]
fn f1_at_is_read_both_ways_in_every_word() {
    let pairs = [
        ("P@y.", "Pay."),
        ("P@y.com", "Pay"),
        ("P@y@mail.com", "Pay"),
        ("Del@te.", "Delete."),
        ("Sw@p!", "Swap"),
        ("C@ll\u{2026}", "Call"),
        ("Tr@nsfer.now", "Transfer"),
        ("Don@te:", "Donate"),
        ("@llow.", "Allow"),
    ];
    let mut wrong = Vec::new();
    for role in [Role::Button, Role::Link, Role::Cell] {
        wrong.extend(misses(&role, &pairs));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    for l in [
        "anna@mail.com",
        "bob@x.io",
        "help@charity.org",
        "Email support@help.com",
        "john.appleseed@icloud.com",
    ] {
        for role in [Role::Button, Role::Cell, Role::Link] {
            assert_eq!(
                tap_as(role.clone(), l),
                RiskCategory::None,
                "{l:?} {role:?}"
            );
        }
    }
}

/// F2: letters that fold to nothing (click letters, the name letters the fold table does
/// not hold, other Latin extensions) inside a Latin word make every pressable control
/// `Unverified`: a row, a long row, a tab, a switch, a link.
#[test]
fn f2_unknown_letters_in_a_latin_word_ask_on_every_pressable_role() {
    let pairs = [
        ("De\u{1C0}ete Account", "Delete Account"),
        ("Ca\u{1C1}", "Call"),
        ("\u{1C0}og Out", "Log Out"),
        ("Bl\u{1C3}ck", "Block"),
        ("\u{DF}lock", "Block"),
        ("Sig\u{14B} Out", "Sign Out"),
        ("Ou\u{167} of Office", "Log Out"),
        ("De\u{29F}ete", "Delete"),
    ];
    let mut wrong = Vec::new();
    for role in PRESSABLE {
        for &(l, real) in &pairs {
            if real == "Log Out" && l.starts_with("Ou") {
                // Not a drawn risk word; only asked.
                if matches!(
                    decide_as(&Policy::default(), role.clone(), l),
                    Decision::Allow
                ) {
                    wrong.push(format!("{role:?} {l:?}: Allow"));
                }
                continue;
            }
            if matches!(
                decide_as(&Policy::default(), role.clone(), l),
                Decision::Allow
            ) {
                wrong.push(format!("{role:?} {l:?}: Allow"));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    // A long row (content by its words) and a key too.
    for (role, l) in [
        (Role::Cell, "De\u{1C0}ete Account and all of your data"),
        (Role::Cell, "Ca\u{1C1} Anna Smith mobile now"),
        (Role::Key, "De\u{1C0}ete"),
        (Role::Tab, "Se\u{1C1}"),
    ] {
        assert!(
            !matches!(
                decide_as(&Policy::default(), role.clone(), l),
                Decision::Allow
            ),
            "{role:?} {l:?}"
        );
    }
    // The category the readings give stays carried beside Unverified.
    let s = app_screen(vec![button(1, "Delete \u{1C0}Account")]);
    let deny = Policy {
        deny_categories: vec![RiskCategory::Delete],
        ..Policy::default()
    };
    assert!(matches!(
        evaluate(&deny, &Action::Tap { element: 1 }, &s, None),
        Decision::Deny(_)
    ));
    match evaluate(&Policy::default(), &Action::Tap { element: 1 }, &s, None) {
        Decision::Confirm(c) => assert!(c.phone_only, "{c:?}"),
        other => panic!("{other:?}"),
    }
}

/// F3: symbols drawn as letters that the sets do not read: every one of them is an
/// intruder inside a Latin word (math, box drawing, dingbats, halfwidth and small forms,
/// Hebrew punctuation, number forms), on a button and a row, under a deny policy too.
#[test]
fn f3_unknown_symbols_in_a_latin_word_ask() {
    let mut wrong = Vec::new();
    for c in [
        '\u{2223}',
        '\u{2502}',
        '\u{23D0}',
        '\u{FFE8}',
        '\u{2758}',
        '\u{5C0}',
        '\u{A6}',
        '\u{2016}',
        '\u{2225}',
        '\u{2551}',
        '\u{FF5C}',
        '\u{2759}',
        '\u{275A}',
        '\u{23B8}',
        '\u{2C92}',
        '\u{A4F2}',
        '\u{16C1}',
        '\u{1D7F7}',
    ] {
        let l = format!("Ca{c}{c}");
        for role in [Role::Button, Role::Cell] {
            if matches!(
                decide_as(&Policy::default(), role.clone(), &l),
                Decision::Allow
            ) {
                wrong.push(format!("{role:?} {l:?} U+{:04X}", u32::from(c)));
            }
            let deny = Policy {
                deny_categories: vec![RiskCategory::Call],
                ..Policy::default()
            };
            if matches!(decide_as(&deny, role.clone(), &l), Decision::Allow) {
                wrong.push(format!("{role:?} {l:?} deny Call: Allow"));
            }
        }
    }
    for l in ["P\u{FE6B}y", "\u{FE69}end", "P\u{FF20}y", "\u{FE69}ell Now"] {
        for role in [Role::Button, Role::Cell] {
            if matches!(
                decide_as(&Policy::default(), role.clone(), l),
                Decision::Allow
            ) {
                wrong.push(format!("{role:?} {l:?}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F4: the compatibility fold reads parenthesized, circled, squared, superscript and
/// modifier letters and digits as the letters they enclose: beside an ordinary word they
/// spell the word (its category, not only Unverified).
#[test]
fn f4_compatibility_forms_read_as_their_letters() {
    for (l, want) in [
        (
            "\u{249F}\u{24A0}\u{24A7}\u{24A0}\u{24AF}\u{24A0} Account",
            RiskCategory::Delete,
        ),
        (
            "\u{24AE}\u{24A0}\u{24A9}\u{249F} Money",
            RiskCategory::Payment,
        ),
        ("\u{24C5}\u{24B6}\u{24CE} now", RiskCategory::Payment),
        ("\u{1F13F}\u{1F130}\u{1F148} now", RiskCategory::Payment),
        ("Ca\u{2160}\u{2160}", RiskCategory::Call),
        ("Ca\u{2474}\u{2474}", RiskCategory::Call),
        ("\u{1D3E}ay", RiskCategory::Payment),
        ("\u{17F}ell", RiskCategory::Payment),
    ] {
        assert_eq!(tap_as(Role::Button, l), want, "{l:?}");
    }
}

/// Whitespace: line breaks other than U+2028/U+2029 (newline, carriage return, form feed,
/// NEL) are soft: a risk word broken over two lines reads joined, and a phrase whose space
/// is a line break still reads. Tab, NBSP, U+3000, U+1680, U+2028 and U+2029 stay hard
/// (pinned by the reviewer's `sound_narrow_spaces_join_and_wide_spaces_split`).
#[test]
fn line_breaks_are_soft_boundaries() {
    let wrong = all_ask(
        &Role::Button,
        &[
            "De\nlete",
            "Sign\nOut",
            "De\r\nlete",
            "Remo\u{85}ve",
            "Tr\u{C}ansfer",
            "Delete\nAccount",
            "P\na\ny",
        ],
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    let wrong = all_ask(&Role::Cell, &["De\nlete", "Remo\u{85}ve"]);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    for sep in [
        '\t', '\u{A0}', '\u{3000}', '\u{1680}', '\u{2028}', '\u{2029}',
    ] {
        assert_eq!(
            tap_as(Role::Button, &format!("De{sep}lete")),
            RiskCategory::None,
            "U+{:04X}",
            u32::from(sep)
        );
    }
    // A two-line ordinary label is read as before.
    for l in ["Recently\nPlayed", "Wi-Fi\nConnected", "10:41\nMonday"] {
        assert_eq!(
            tap_as(Role::Button, l),
            tap_as(Role::Button, &l.replace('\n', " ")),
            "{l:?}"
        );
    }
}

/// Punctuation inside a word is a soft boundary to the sets: a split risk word reads
/// joined; the plain reading still splits there.
#[test]
fn punctuation_inside_a_word_does_not_stop_a_phrase() {
    let pairs = [
        ("De-lete", "Delete"),
        ("De.lete", "Delete"),
        ("De/lete", "Delete"),
        ("D-e-l-e-t-e", "Delete"),
        ("Sign_Out", "Sign Out"),
        ("Re\u{2014}move", "Remove"),
        ("Tr*ansfer", "Transfer"),
        ("Pa:y", "Pay"),
        ("Un\u{B7}install", "Uninstall"),
        ("Log\u{2026}Out", "Log Out"),
    ];
    let mut wrong = Vec::new();
    for role in [Role::Button, Role::Cell, Role::Link] {
        wrong.extend(misses(&role, &pairs));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    // A split at a space stays a split ("De lete"), an exception stays literal.
    assert_eq!(tap_as(Role::Button, "De lete"), RiskCategory::None);
    assert_eq!(tap_as(Role::Button, "Don't Allow"), RiskCategory::None);
    assert_eq!(tap_as(Role::Button, "Wi-Fi"), RiskCategory::None);
}

/// The look-alike digits `2 6 9` read as letters where their run touches a letter of the
/// word (as at pass 37; a weak-digit rule needing letters on both sides was tried in this
/// pass and undone: it let `6lock` and `Lo9 Out` through). Count and size labels are only
/// measured here: what the sets add to them is printed, not asserted away.
#[test]
fn lookalike_digits_read_where_they_touch_a_letter() {
    let wrong = misses(
        &Role::Button,
        &[
            ("6lock", "Block"),
            ("Lo9 Out", "Log Out"),
            ("Si9n Out", "Sign Out"),
            ("Su6scribe", "Subscribe"),
            ("Up9rade", "Upgrade"),
        ],
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    // A digit alone or after a space stays a number (no letter touches it).
    for l in ["Page 2 of 9", "Size 6", "Season 2", "6 items"] {
        assert_eq!(tap_as(Role::Button, l), RiskCategory::None, "{l:?}");
    }
    #[cfg(feature = "reference")]
    for l in [
        "6in",
        "8in",
        "S60 \u{A3}",
        "iPhone 60ft",
        "Galaxy S9",
        "HERO9",
        "XC90",
        "B2B",
    ] {
        for role in [Role::Button, Role::Cell] {
            let s = app_screen(vec![el(1, role.clone(), l)]);
            let r = jarvis_safety::reference::tap_reading_risks(&Action::Tap { element: 1 }, &s);
            println!("{l:?} {role:?}: {r:?}");
        }
    }
}

/// The texts read for Return, the credential-field test and the sign-in press: a label or
/// placeholder with an intruder is not trusted.
#[test]
fn intruders_in_field_labels_and_return_keys_ask() {
    let typed = |label: &str, placeholder: Option<&str>, submit: bool| {
        let mut f = field(1, Role::TextField, label);
        f.placeholder = placeholder.map(str::to_owned);
        f.focused = true;
        let s = app_screen(vec![f]);
        classify(
            &Action::Type {
                element: Some(1),
                text: "hello".to_owned(),
                submit,
            },
            &s,
            None,
        )
    };
    assert_ne!(
        typed("Pa\u{2223}\u{2223}word", None, false),
        RiskCategory::None
    );
    assert_ne!(
        typed("Notes", Some("Pass\u{1C0}word"), false),
        RiskCategory::None
    );
    assert_ne!(
        typed("Type DE\u{1C0}ETE to confirm", None, true),
        RiskCategory::None
    );
    // Ordinary fields are read as before.
    assert_eq!(
        typed("Notes", Some("Add a note"), false),
        RiskCategory::None
    );
    // A Return key labelled with an intruder.
    let mut f = field(1, Role::TextField, "Search");
    f.focused = true;
    let s = app_screen(vec![f, el(2, Role::Key, "Se\u{2223}d"), label(3, "Notes")]);
    let c = classify(
        &Action::Type {
            element: Some(1),
            text: "hello".to_owned(),
            submit: true,
        },
        &s,
        None,
    );
    assert_ne!(c, RiskCategory::None);
}

/// The punctuation a word may hold anywhere besides ASCII (`norm::is_word_punctuation`).
#[cfg(feature = "reference")]
const WORD_PUNCTUATION: &[char] = &[
    '\u{2010}', '\u{2011}', '\u{2012}', '\u{2013}', '\u{2014}', '\u{2015}', '\u{2212}', '\u{201C}',
    '\u{201D}', '\u{B7}', '\u{2022}', '\u{2026}', '\u{2122}', '\u{AE}', '\u{A9}', '\u{B0}',
    '\u{2116}', '\u{2032}', '\u{2033}',
];

/// Why a code point may leave a label allowed, as the rule states it (`None`: it may not).
#[cfg(feature = "reference")]
fn allowed_class(c: char, at_edge: bool) -> Option<&'static str> {
    let n = jarvis_safety::reference::normalized(&c.to_string());
    let mut it = n.chars();
    let (first, second) = (it.next(), it.next());
    if c.is_whitespace() || n == "\u{2009}" {
        return Some("whitespace: splits the word as a space does");
    }
    let Some(f) = first else {
        return Some("dropped (invisible, format, mark, apostrophe): the word reads joined");
    };
    if second.is_some() {
        return None;
    }
    if f.is_ascii() {
        return Some("ASCII after the normalisation (another letter, a digit, punctuation)");
    }
    if f.is_alphabetic() && jarvis_safety::fold_char(f).is_ascii_alphabetic() {
        return Some("a letter the fold table reads as another ASCII letter");
    }
    if matches!(f, '\u{A1}' | '\u{20AC}' | '\u{A2}' | '\u{A5}' | '\u{A3}') {
        return Some("a look-alike symbol of the sets read as another letter");
    }
    if WORD_PUNCTUATION.contains(&f) {
        return Some("word punctuation: a visible break inside the word");
    }
    // Review pass 39 (F1): at the edge only an emoji (Extended_Pictographic or
    // Emoji_Presentation, emoji-data.txt 15.1.0) or a listed arrow, chevron, check mark or
    // cross; it was any non-letter (CJK strokes and radicals were allowed there).
    let emoji = emoji::EMOJI
        .iter()
        .any(|&(a, b)| (a..=b).contains(&u32::from(f)));
    if at_edge && !f.is_alphabetic() && (emoji || EDGE_SYMBOLS.contains(&f)) {
        return Some("at the word's edge (arrow, check mark, pictograph): visibly not a letter");
    }
    None
}

/// Review pass 39: the arrows, chevrons, check marks and crosses a word may hold at its edge.
#[cfg(feature = "reference")]
const EDGE_SYMBOLS: &[char] = &[
    '\u{2039}', '\u{203A}', '\u{AB}', '\u{BB}', '\u{2190}', '\u{2191}', '\u{2192}', '\u{2193}',
    '\u{2713}', '\u{2714}', '\u{2715}', '\u{2716}', '\u{D7}',
];

/// The class, for EVERY assigned code point of Unicode 15.1 (287,346, private use
/// included): `De{c}ete Account`, `{c}og Out`, `Ca{c}{c}` and `P{c}y`, tapped as a Button
/// and as a Cell, each either asks (a category or `Unverified`) or `c` is in the expected
/// set ([`allowed_class`], counted and printed per reason): whitespace, a dropped
/// character, ASCII or a letter folded to ASCII (another letter than the word needs),
/// a look-alike symbol, word punctuation, or a non-letter at the word's edge. No other
/// code point leaves any of these labels allowed.
#[cfg(feature = "reference")]
#[test]
fn every_code_point_in_a_risk_word_asks_or_is_known() {
    assert_eq!(
        assigned::ASSIGNED
            .iter()
            .map(|&(a, b)| b - a + 1)
            .sum::<u32>(),
        assigned::ASSIGNED_COUNT
    );
    assert_eq!(
        emoji::EMOJI.iter().map(|&(a, b)| b - a + 1).sum::<u32>(),
        emoji::EMOJI_COUNT
    );
    let cps: Vec<char> = assigned::ASSIGNED
        .iter()
        .flat_map(|&(a, b)| a..=b)
        .filter_map(char::from_u32)
        .collect();
    type Make = fn(char) -> String;
    let labels: [(Make, bool); 4] = [
        (|c| format!("De{c}ete Account"), false),
        (|c| format!("{c}og Out"), true),
        (|c| format!("Ca{c}{c}"), true),
        (|c| format!("P{c}y"), false),
    ];
    let threads = 3usize;
    let chunk = cps.len().div_ceil(threads);
    type Classes = std::collections::BTreeMap<&'static str, (usize, usize, Option<char>)>;
    let results: Vec<(Vec<String>, Classes)> = std::thread::scope(|sc| {
        let hs: Vec<_> = cps
            .chunks(chunk)
            .map(|part| {
                sc.spawn(move || {
                    let mut wrong = Vec::new();
                    let mut classes = std::collections::BTreeMap::new();
                    for &c in part {
                        for (make, edge) in &labels {
                            let l = make(c);
                            for role in [Role::Button, Role::Cell] {
                                if tap_as(role.clone(), &l) != RiskCategory::None {
                                    continue;
                                }
                                match allowed_class(c, *edge) {
                                    Some(why) => {
                                        let e =
                                            classes.entry(why).or_insert((0usize, 0usize, None));
                                        e.0 += 1;
                                        if e.2 != Some(c) {
                                            e.1 += 1;
                                            e.2 = Some(c);
                                        }
                                    }
                                    None => wrong.push(format!(
                                        "U+{:04X} {l:?} {role:?}: allowed",
                                        u32::from(c)
                                    )),
                                }
                            }
                        }
                    }
                    (wrong, classes)
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut wrong = Vec::new();
    let mut classes = std::collections::BTreeMap::new();
    for (w, c) in results {
        wrong.extend(w);
        for (k, (taps, points, _)) in c {
            let e = classes.entry(k).or_insert((0usize, 0usize));
            e.0 += taps;
            e.1 += points;
        }
    }
    println!(
        "{} code points, 8 taps each; allowed taps (code points) by reason:",
        cps.len()
    );
    for (k, (taps, points)) in &classes {
        println!("  {taps:>7} ({points:>6})  {k}");
    }
    assert!(
        wrong.is_empty(),
        "{} allowed outside the expected set:\n{}",
        wrong.len(),
        wrong
            .iter()
            .take(60)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The generated address corpus (feature `reference`): 21,760 realistic addresses (80
/// first names in 8 local-part shapes, 40 role names, × 32 common and odd domains), each
/// tapped as a Cell, a Link, a StaticText (a text in a row) and a Button. Counted: taps that
/// gain a category from reading `@` as a letter (the same address with `@` replaced by a
/// space asks nothing more), printed with their shapes.
#[test]
fn an_address_corpus_rarely_gains_a_category_from_the_at() {
    let first = [
        "anna", "bob", "carol", "dave", "emma", "frank", "grace", "henry", "ivy", "jack", "kate",
        "liam", "mia", "noah", "olivia", "paul", "quinn", "ruth", "sam", "tina", "uma", "victor",
        "wendy", "xavier", "yara", "zoe", "adam", "bella", "chloe", "daniel", "ella", "felix",
        "george", "hannah", "isaac", "julia", "kevin", "laura", "mark", "nina", "oscar", "petra",
        "rachel", "steve", "tom", "ursula", "vera", "will", "yusuf", "zach", "ali", "amy", "ben",
        "eve", "ian", "joe", "kim", "lee", "max", "ned", "pat", "ray", "sue", "ted", "val", "al",
        "ed", "jo", "li", "ty", "aaron", "erin", "owen", "ana", "eli", "ada", "ari", "ira", "otto",
        "una",
    ];
    let last = [
        "smith", "lee", "garcia", "nguyen", "oconnor", "mueller", "rossi", "kim",
    ];
    let roles = [
        "pay",
        "billing",
        "sales",
        "support",
        "delete-me",
        "admin",
        "info",
        "noreply",
        "accounts",
        "payments",
        "orders",
        "hello",
        "help",
        "contact",
        "team",
        "jobs",
        "press",
        "security",
        "abuse",
        "postmaster",
        "webmaster",
        "invoice",
        "refunds",
        "donate",
        "give",
        "tips",
        "book",
        "booking",
        "reservations",
        "call",
        "sell",
        "trade",
        "swap",
        "order",
        "send",
        "share",
        "post",
        "mail",
        "me",
        "x",
    ];
    let domains = [
        "gmail.com",
        "icloud.com",
        "me.com",
        "mac.com",
        "outlook.com",
        "hotmail.com",
        "yahoo.com",
        "proton.me",
        "fastmail.fm",
        "aol.com",
        "gmx.de",
        "web.de",
        "mail.ru",
        "example.com",
        "example.org",
        "acme.co.uk",
        "bank.com",
        "paypal.com",
        "apple.com",
        "shop.io",
        "x.ai",
        "a.co",
        "e.com",
        "lly.com",
        "llow.io",
        "ll.me",
        "nd.org",
        "pp.le",
        "ccept.net",
        "ble.dev",
        "dmin.app",
        "pple.com",
    ];
    let mut locals: Vec<String> = Vec::new();
    for (k, f) in first.iter().enumerate() {
        let l = last[k % last.len()];
        let i = &f[..1];
        locals.extend([
            (*f).to_owned(),
            format!("{f}.{l}"),
            format!("{i}.{l}"),
            format!("{f}+news"),
            format!("{f}{}", 70 + k),
            format!("{l}.{f}"),
            format!("{f}_{l}"),
            format!("{f}{l}"),
        ]);
    }
    locals.extend(roles.iter().map(|r| (*r).to_owned()));
    let mut taps = 0usize;
    let mut gained = Vec::new();
    for l in &locals {
        for d in domains {
            let addr = format!("{l}@{d}");
            let spaced = format!("{l} {d}");
            for role in [Role::Cell, Role::Link, Role::StaticText, Role::Button] {
                taps += 1;
                let a = tap_as(role.clone(), &addr);
                let b = tap_as(role.clone(), &spaced);
                if a != b && a != RiskCategory::None {
                    gained.push(format!("{addr:?} {role:?}: {b:?} -> {a:?}"));
                }
            }
        }
    }
    println!(
        "{} addresses, {taps} taps, {} gain a category from the @",
        locals.len() * domains.len(),
        gained.len()
    );
    let mut by_domain = std::collections::BTreeMap::new();
    for g in &gained {
        let d = g
            .split('@')
            .nth(1)
            .unwrap_or("")
            .split('"')
            .next()
            .unwrap_or("");
        *by_domain.entry(d.to_owned()).or_insert(0usize) += 1;
    }
    for (d, n) in &by_domain {
        println!("  @{d}: {n}");
    }
    for g in gained.iter().filter(|g| g.contains("Button")) {
        println!("  {g}");
    }
    assert!(locals.len() * domains.len() >= 20_000);
    // Measured: every gain is on `llow.io`, a domain made to finish `allow` after the `@`
    // (680 addresses x 4 roles = 2,720 taps, of which 2,350 gain `SystemSetting`; the rest
    // already asked); none on the 31 other domains, the odd ones (`pple.com`, `ccept.net`,
    // `dmin.app`, `e.com`, `nd.org`) included.
    assert!(by_domain.keys().all(|d| d == "llow.io"), "{by_domain:?}");
}

/// Ordinary labels with punctuation inside a word (hyphenated words, abbreviations,
/// slashes, ampersands, file names and handles) and the reviewer's count/price corpus
/// joined by punctuation: counted, taps that gain a category from the soft punctuation (the
/// same label with that punctuation replaced by a space asks less), printed and judged in
/// the commit message.
#[test]
fn ordinary_punctuated_labels_rarely_gain_a_category() {
    let words = [
        "Wi-Fi",
        "T-Shirt",
        "e-mail",
        "Add-ons",
        "Check-in",
        "Sign-up",
        "U.S.",
        "e.g.",
        "N/A",
        "24/7",
        "AT&T",
        "C++",
        "Re:",
        "Fwd:",
        "Mr.",
        "Dr.",
        "St.",
        "No.",
        "Vol.",
        "I/O",
        "On/Off",
        "A/B",
        "Q&A",
        "R&D",
        "B&B",
        "P&L",
        "Coca-Cola",
        "Rolls-Royce",
        "Jean-Luc",
        "co-op",
        "re-enter",
        "self-service",
        "follow-up",
        "built-in",
        "drop-down",
        "pop-up",
        "set-up",
        "pick-up",
        "drive-thru",
        "opt-in",
        "plug-in",
        "e-book",
        "eSIM",
        "x.com",
        "README.md",
        "photo_2024.jpg",
        "IMG_0042.HEIC",
        "user_name",
        "first.last",
        "v2.1.0",
        "iOS-17",
        "Mon-Fri",
        "9-5",
        "A-Z",
        "Dolby-Atmos",
        "Hi-Res",
        "Lo-Fi",
        "To-Do",
        "To-dos",
        "Up-to-date",
        "Ready-to-go",
        "Do-it-yourself",
        "Re-order",
        "Pre-installed",
        "Non-stop",
        "All-in-one",
        "Year-to-date",
        "Q1/Q2",
        "Kids/Teens",
        "Men's/Women's",
        "Rock&Roll",
        "Salt&Pepper",
        "Mac/PC",
        "Music/Video",
        "News+",
        "Apple.com",
        "Settings/General",
        "Photos/Recents",
        "Back/Forward",
        "Yes/No",
        "OK/Cancel",
        "Save/Load",
        "Open/Close",
        "Show/Hide",
        "Play/Pause",
        "Copy/Paste",
        "Cut/Copy",
        "Undo/Redo",
        "Zoom-in",
        "Zoom-out",
        "Log-in",
        "Logged-in",
        "Read-only",
        "Full-screen",
        "Dark-mode",
        "Low-power",
        "Auto-lock",
        "Night-shift",
        "Do-not-disturb",
        "Set-top",
        "Long-press",
        "Swipe-up",
        "Tap-to-pay",
        "One-time",
        "Two-factor",
        "Re-try",
        "Re-load",
        "Un-do",
        "co-pay",
        "pay-per-view",
        "Pay-as-you-go",
        "Buy-now",
        "Wish-list",
        "Gift-card",
        "Top-up",
        "Sell-by",
        "Best-before",
        "Add-to-cart",
        "Post-it",
        "e-Ticket",
        "Dept.",
        "Inc.",
        "Ltd.",
        "Co.",
        "approx.",
        "min.",
        "max.",
        "Ave.",
        "Blvd.",
        "Rd.",
        "Apt.",
        "Fig.",
        "p.m.",
        "a.m.",
        "Ph.D.",
        "i.e.",
        "etc.",
        "vs.",
        "w/o",
        "c/o",
        "s/he",
    ];
    let mut taps = 0usize;
    let mut gained = Vec::new();
    for w in words {
        for role in [Role::Button, Role::Cell, Role::Link] {
            taps += 1;
            let a = tap_as(role.clone(), w);
            let spaced: String = w
                .chars()
                .map(|c| if c.is_ascii_punctuation() { ' ' } else { c })
                .collect();
            let b = tap_as(role.clone(), &spaced);
            // A definite category gained (a dotted word alone is an unknown SF Symbol
            // name, `Unverified` either way).
            if a != b && !matches!(a, RiskCategory::None | RiskCategory::Unverified) {
                gained.push(format!("{w:?} {role:?}: {b:?} -> {a:?}"));
            }
        }
    }
    println!(
        "{} labels, {taps} taps, {} gain a category",
        words.len(),
        gained.len()
    );
    for g in &gained {
        println!("  {g}");
    }
    // Judged in the commit message; every gain is a joined risk word (`Re-try` is not one).
    assert!(gained.len() * 20 <= taps, "{} of {taps}", gained.len());
}

/// Sound: ordinary accented, symbol-bearing and multi-script labels are not made
/// `Unverified` by the intruder rule (they are read as before).
#[cfg(feature = "reference")]
#[test]
fn ordinary_labels_hold_no_intruder() {
    for l in [
        "Caf\u{E9} M\u{FC}ller",
        "S\u{F8}ren \u{141}ukasz",
        "Wi\u{2011}Fi 6",
        "Title\u{2026}",
        "\u{2039} Back",
        "Done \u{2713}",
        "AirPods\u{2019} Case",
        "\u{A9} 2024 Apple Inc.",
        "iPhone\u{AE}",
        "72\u{B0}F",
        "Trending \u{1F525}",
        "\u{1F44B}Hello",
        "\u{20B9}499 only",
        "m\u{B2}",
        "10\u{2009}000",
        "\u{423}\u{434}\u{430}\u{43B}\u{438}\u{442}\u{44C}",
        "\u{524A}\u{9664}",
    ] {
        assert!(!jarvis_safety::reference::has_intruder(l), "{l:?}");
    }
}
