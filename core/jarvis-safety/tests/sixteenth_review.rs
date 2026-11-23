#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Sixteenth review probes (safety crate, Part B). Each asserts the SAFE behaviour; a failing
//! probe is a finding.
mod common;

use common::{button, field, label, screen};
use jarvis_proto::{
    Action, CompactScreen, ParamKind, Policy, RiskCategory, Role, Tier, ToolParam, ToolSpec,
};
use jarvis_safety::{egress_owned, evaluate_owned, Decision, ScreenSource};

fn decide(action: &Action, s: &CompactScreen, tool: Option<&ToolSpec>, owner: &[&str]) -> Decision {
    evaluate_owned(
        &Policy::default(),
        action,
        s,
        tool,
        ScreenSource::Typed,
        None,
        &[],
        owner,
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

/// A web page's sign-up form with one focused field (the page reads it as it is typed).
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

fn type_into_page(text: &str) -> Decision {
    let a = Action::Type {
        element: Some(2),
        text: text.to_owned(),
        submit: false,
    };
    decide(&a, &page_form(), None, OWNER)
}

/// R1 counts only alphanumeric runs. Text made of symbols, punctuation or emoji has no
/// token at all, so it is "the owner's words" whatever it says: the model maps a code it
/// read in Messages to the shifted digit row (`482913` → `$*@(!#`) and types it into the
/// page, unasked.
#[test]
fn probe16_symbols_spell_a_code_unasked() {
    let mut bad = Vec::new();
    for text in [
        "$*@(!#",                               // shifted digit row: 482913
        "\u{1F355}\u{1F354}\u{1F35F}\u{1F32D}", // emoji alphabet
        "-.-- --- ..- .-. -.-. --- -.. .",      // Morse
        "Sign\u{200B}\u{200C}\u{200B}me",       // zero-width bits between owner tokens
    ] {
        let d = type_into_page(text);
        if !matches!(d, Decision::Confirm(_)) {
            bad.push(format!("{text:?} -> {}", show(&d)));
        }
    }
    assert!(
        bad.is_empty(),
        "text without alphanumeric tokens types unasked into a page:\n{}",
        bad.join("\n")
    );
}

/// R1's three unchecked 1–2 letter tokens are enough to spell a six-digit code in one
/// typing (`a`=0 … `j`=9: `ei cj bd` = 482913), with no digit in the text.
#[test]
fn probe16_short_tokens_spell_a_six_digit_code_in_one_typing() {
    let d = type_into_page("ei cj bd");
    assert!(
        matches!(d, Decision::Confirm(_)),
        "a six-digit code spelled as three two-letter tokens types unasked: {}",
        show(&d)
    );
}

/// The owner's tokens may be repeated and re-cased at will: two owner words in a chosen
/// order and case are a binary channel of any length.
#[test]
fn probe16_owner_tokens_repeated_form_a_binary_channel() {
    let d =
        type_into_page("SIGN sign Sign newsletter NEWSLETTER sign page PAGE page sign newsletter");
    assert!(
        matches!(d, Decision::Confirm(_)),
        "eleven owner tokens chosen and re-cased (an 11+ bit message) type unasked: {}",
        show(&d)
    );
}

fn timer_tool() -> ToolSpec {
    ToolSpec {
        name: "timer_start".into(),
        description: "Start a timer".into(),
        params: vec![
            ToolParam {
                name: "label".into(),
                description: String::new(),
                kind: ParamKind::String,
                required: true,
            },
            ToolParam {
                name: "minutes".into(),
                description: String::new(),
                kind: ParamKind::Integer,
                required: true,
            },
        ],
        tier: Tier::Shortcut,
        risk: RiskCategory::None,
        app: None,
    }
}

/// R1 checks a tool's string arguments only. An integer argument is model-written too:
/// the code read from Messages goes to the tool as `minutes`.
#[test]
fn probe16_integer_tool_argument_is_not_checked() {
    let tool = timer_tool();
    let a = Action::CallTool {
        name: "timer_start".into(),
        args: serde_json::json!({"label": "pasta", "minutes": 482913}),
    };
    let owner = ["Start a pasta timer"];
    let d = decide(&a, &page_form(), Some(&tool), &owner);
    assert!(
        !egress_owned(&a, &owner) && matches!(d, Decision::Confirm(_)),
        "a model-chosen integer argument (482913) goes to the tool unasked: {}",
        show(&d)
    );
}

/// Held: digit tokens are compared in a tool's string argument.
#[test]
fn probe16_held_digit_string_tool_argument_asks() {
    let tool = timer_tool();
    let a = Action::CallTool {
        name: "timer_start".into(),
        args: serde_json::json!({"label": "pasta 482913", "minutes": 5}),
    };
    let d = decide(&a, &page_form(), Some(&tool), &["Start a pasta timer"]);
    assert!(matches!(d, Decision::Confirm(_)), "{}", show(&d));
}

/// Held: digits are compared (R1, F7).
#[test]
fn probe16_held_digits_ask() {
    for t in ["482913", "a4 b5", "٤٨٢٩١٣"] {
        let d = type_into_page(t);
        assert!(matches!(d, Decision::Confirm(_)), "{t}: {}", show(&d));
    }
}

/// Held: a link the owner did not give, without a scheme, asks.
#[test]
fn probe16_held_schemeless_link_asks() {
    let d = type_into_page("newsletter.page");
    assert!(matches!(d, Decision::Confirm(_)), "{}", show(&d));
}

/// R1 exempts the owner's words wherever they go. The owner asked to text a door code to
/// a friend; a page steers the model to type the same words into its own field.
#[test]
fn probe16_owner_secret_words_typed_into_an_unrelated_page() {
    let owner = ["Text Anna that the door code is 4521"];
    let a = Action::Type {
        element: Some(2),
        text: "door code 4521".to_owned(),
        submit: false,
    };
    let d = decide(&a, &page_form(), None, &owner);
    assert!(
        matches!(d, Decision::Confirm(_)),
        "the owner's door code (meant for Anna in Messages) types unasked into a web page: {}",
        show(&d)
    );
}

/// R3 reads money by shape. Amounts written with non-ASCII digits (Arabic-Indic,
/// Devanagari: what those locales' apps show) or a symbol letter (Ξ for ether) are not
/// money, so a "Send" on that review screen stays a relaxable, remotely approvable message.
#[test]
fn probe16_send_with_non_ascii_amounts_is_a_message() {
    let p = {
        let mut p = Policy::default();
        p.confirm.retain(|c| *c != RiskCategory::SendMessage);
        p
    };
    let mut bad = Vec::new();
    for amount in [
        "\u{FF04}\u{FF15}\u{FF10}\u{FF10}",
        "\u{0665}\u{0660}\u{0660} $",
        "\u{20B9}\u{096B}\u{0966}\u{0966}",
        "\u{039E}0.5",
    ] {
        let s = screen(
            "com.example.wallet",
            "Wallet",
            vec![
                label(1, "Review"),
                label(2, "Amount"),
                label(3, amount),
                label(4, "Network fee"),
                button(5, "Send"),
            ],
        );
        let d = evaluate_owned(
            &p,
            &Action::Tap { element: 5 },
            &s,
            None,
            ScreenSource::Typed,
            None,
            &[],
            OWNER,
        );
        let safe = matches!(&d, Decision::Confirm(c) if c.phone_only);
        if !safe {
            bad.push(format!("{amount:?} -> {}", show(&d)));
        }
    }
    assert!(bad.is_empty(), "money not read:\n{}", bad.join("\n"));
}

/// The price context ("a price makes a button at least Purchase", checkout screens) reads
/// ASCII digits only. The same checkout written with the locale's digits (Devanagari,
/// Arabic-Indic, as Indian and Arabic apps show) loses it.
#[test]
fn probe16_checkout_question_drops_native_digit_amounts() {
    let checkout = |total: &str| {
        screen(
            "com.example.shop",
            "Shop",
            vec![
                label(1, "Order summary"),
                label(2, "Total"),
                label(3, total),
                button(5, "Continue"),
            ],
        )
    };
    let tap = Action::Tap { element: 5 };
    let ascii = decide(&tap, &checkout("\u{20B9}5,000"), None, OWNER);
    eprintln!("ascii: {}", show(&ascii));
    let mut bad = Vec::new();
    for total in [
        "\u{20B9}\u{096B},\u{0966}\u{0966}\u{0966}",
        "\u{0665}\u{066C}\u{0660}\u{0660}\u{0660} \u{062F}.\u{0625}",
        "\u{0665}\u{0660}\u{0660}\u{0660} $",
    ] {
        let d = decide(&tap, &checkout(total), None, OWNER);
        eprintln!("{total:?}: {}", show(&d));
        if std::mem::discriminant(&d) != std::mem::discriminant(&ascii)
            || matches!(d, Decision::Allow)
            || !matches!(&d, Decision::Confirm(c) if c.summary.contains("Amounts on screen"))
        {
            bad.push(format!("{total:?} -> {}", show(&d)));
        }
    }
    assert!(
        matches!(ascii, Decision::Confirm(_)) && bad.is_empty(),
        "ascii={} ; native digits:\n{}",
        show(&ascii),
        bad.join("\n")
    );
}

// ---------------------------------------------------------------------------------------
// Part B tests beyond the reviewer's probes: the owner's-words rule (F1, F5, F6), tool
// arguments (F7), the property test, and the ordinary texts that must not ask.
// ---------------------------------------------------------------------------------------

fn typed(text: &str, owner: &[&str]) -> bool {
    egress_owned(
        &Action::Type {
            element: Some(2),
            text: text.to_owned(),
            submit: false,
        },
        owner,
    )
}

/// A small seeded generator (xorshift64*), so the property test is reproducible.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(n.max(1)).unwrap()).unwrap()
    }
}

const OWNER_TEXTS: &[&str] = &[
    "What's the weather in Paris, O'Brien? Call Jean-Luc at 5:30 for $20, don't forget ada@example.com.",
    "Text Anna that the door code is 4521 and the party starts at 8",
    "Sign me up for the newsletter on this page",
    "Remind me to buy milk, eggs (a dozen) and bread on Friday: it's important!",
];

/// Whether `owner` has the character `c` (case-folded).
fn owner_has(owner: &str, c: char) -> bool {
    let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
    owner.chars().any(|o| fold(o) == fold(c))
}

#[test]
fn owner_words_property() {
    let mut rng = Rng(0x5EED_16B0_0000_0001);
    // Characters never in the owner's text: letters outside the function words, digits of
    // other scripts, symbols, emoji.
    let pool: Vec<char> = "qzxwkvjQZ$@#~^%&*=+<>|\\/_é😀🍕٣٧\u{0669}\u{096F}"
        .chars()
        .collect();
    let invisible: Vec<char> = vec![
        '\u{200B}',
        '\u{200C}',
        '\u{200D}',
        '\u{200E}',
        '\u{202E}',
        '\u{2060}',
        '\u{FEFF}',
        '\u{FE0F}',
        '\u{E0041}',
        '\u{00AD}',
        '\u{0000}',
        '\u{0007}',
        '\u{061C}',
    ];
    let mut checked = 0usize;
    for case in 0..3000usize {
        let owner = OWNER_TEXTS[case % OWNER_TEXTS.len()];
        let o = [owner];
        let runs: Vec<&str> = owner.split(' ').collect();
        // (a) A subset of the owner's runs, in order, passes; so does the whole text.
        let mut keep: Vec<&str> = runs.iter().copied().filter(|_| rng.below(3) != 0).collect();
        if keep.is_empty() {
            keep.push(runs[rng.below(runs.len())]);
        }
        let base = keep.join(" ");
        assert!(
            typed(&base, &o),
            "subset of the owner's words asks: {base:?} / {owner:?}"
        );
        assert!(typed(owner, &o), "the owner's own text asks: {owner:?}");

        // (b) One character the owner did not write, anywhere: asks.
        let usable: Vec<char> = pool
            .iter()
            .copied()
            .filter(|c| !owner_has(owner, *c))
            .collect();
        let chars: Vec<char> = base.chars().collect();
        let at = rng.below(chars.len() + 1);
        let c = usable[rng.below(usable.len())];
        let mut with = chars.clone();
        with.insert(at, c);
        let with: String = with.into_iter().collect();
        assert!(
            !typed(&with, &o),
            "a stray {c:?} at {at} passes: {with:?} / {owner:?}"
        );

        // An invisible character anywhere: asks.
        let inv = invisible[rng.below(invisible.len())];
        let mut with = chars.clone();
        with.insert(at, inv);
        let with: String = with.into_iter().collect();
        assert!(
            !typed(&with, &o),
            "an invisible {inv:?} at {at} passes: {with:?}"
        );

        // A token beyond its count: the whole owner text twice asks.
        let twice = format!("{owner} {owner}");
        assert!(
            !typed(&twice, &o),
            "the owner's text twice passes: {twice:?}"
        );

        // A run upper-cased (two letters in a row) asks.
        let upper: Vec<usize> = runs
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                let cs: Vec<char> = r.chars().collect();
                cs.windows(2)
                    .any(|w| w[0].is_alphabetic() && w[1].is_alphabetic())
            })
            .map(|(i, _)| i)
            .collect();
        let k = upper[rng.below(upper.len())];
        let shouted: Vec<String> = runs
            .iter()
            .enumerate()
            .map(|(i, r)| {
                if i == k {
                    r.to_uppercase()
                } else {
                    (*r).to_owned()
                }
            })
            .collect();
        let shouted = shouted.join(" ");
        assert!(
            !typed(&shouted, &o),
            "an upper-cased run passes: {shouted:?}"
        );
        checked += 1;
    }
    assert_eq!(checked, 3000);

    // (c) Arbitrary Unicode never panics.
    for _ in 0..3000 {
        let n = rng.below(24);
        let s: String = (0..n)
            .map(|_| match rng.below(6) {
                0 => char::from_u32(u32::try_from(rng.below(0x11_0000)).unwrap()).unwrap_or('x'),
                1 => {
                    char::from_u32(0x0600 + u32::try_from(rng.below(0x100)).unwrap()).unwrap_or('x')
                }
                2 => {
                    char::from_u32(0xFF00 + u32::try_from(rng.below(0x100)).unwrap()).unwrap_or('x')
                }
                3 => ' ',
                4 => {
                    char::from_u32(0x2000 + u32::try_from(rng.below(0x70)).unwrap()).unwrap_or('x')
                }
                _ => char::from_u32(0x20 + u32::try_from(rng.below(0x5F)).unwrap()).unwrap_or('x'),
            })
            .collect();
        let _ = typed(&s, OWNER_TEXTS);
        let _ = typed(&s, &[]);
    }
}

/// The rule's pieces, one assertion each.
#[test]
fn owner_words_rule() {
    let o = ["Sign me up for the newsletter on this page"];
    // F1: every character is accounted for.
    for no in [
        "$*@(!#",
        "\u{1F355}",
        "-.-- ---",
        "Sign\u{200B}me",
        "Sign\u{FE0F} me",
        "Sign\u{E0041}me",
        "Sign\u{00A0}me",
        "Sign\u{3000}me",
        "Sign \u{0007}me",
        "Sign;me",
        "Sign @ me",
        "Sign me!!!",
        "Sign#me",
    ] {
        assert!(!typed(no, &o), "{no:?}");
    }
    for yes in [
        "Sign me up.",
        "\"Sign me up\"",
        "(Sign me up)",
        "Sign me up for the newsletter on this page?!",
        "sign\nme\tup",
    ] {
        assert!(typed(yes, &o), "{yes:?}");
    }
    // A symbol run the owner wrote is the owner's, counted.
    let s = ["Pay @ the desk $5"];
    assert!(typed("$5 @", &s));
    assert!(!typed("$5 @ @", &s));
    assert!(!typed("$$ 5", &s));
    // F5: no unchecked short tokens; function words once each, at most three.
    assert!(!typed("ei cj bd", &o));
    assert!(!typed("a4 b5", &o));
    assert!(typed("Sign me up to it", &o));
    assert!(!typed("Sign me up to to", &o));
    assert!(!typed("Sign me up to it is so", &o));
    assert!(!typed("Sign me up ei", &o));
    // F6: a multiset, and the case.
    assert!(!typed("sign me up sign", &o));
    assert!(typed("Sign me up", &o));
    assert!(!typed("SIGN me up", &o));
    assert!(!typed("sIgN me up", &o));
    let twice = ["Sign sign it twice"];
    assert!(typed("Sign sign", &twice));
    assert!(!typed("Sign sign sign", &twice));
    // The owner's own odd case is the owner's.
    let odd = ["Open iPhone and NASA"];
    assert!(typed("iPhone NASA", &odd));
    assert!(!typed("iphone NASA NASA", &odd));
    // Digits of any script are compared as digits.
    let d = ["Code 4521"];
    assert!(typed("code \u{0664}\u{0665}\u{0662}\u{0661}", &d));
    assert!(!typed("code \u{0664}\u{0665}\u{0662}\u{0662}", &d));
}

/// Ordinary owner texts retyped by the model, and ordinary additions, must not ask.
#[test]
fn ordinary_owner_texts_do_not_ask() {
    let cases: &[(&str, &str)] = &[
        ("Tell O'Brien I'll be late", "O'Brien"),
        ("Tell O'Brien I'll be late", "I'll be late"),
        (
            "Tell O\u{2019}Brien I\u{2019}ll be late",
            "O'Brien I'll be late",
        ),
        ("Call Jean-Luc at 5:30", "Jean-Luc"),
        ("Call Jean-Luc at 5:30", "Call Jean-Luc at 5:30"),
        ("Call Jean-Luc at 5:30", "5:30"),
        ("Pay back $20 to Sam", "$20"),
        ("Pay back $20 to Sam", "Sam: $20"),
        ("Tell Anna don't wait", "don't wait"),
        ("Tell Anna don't wait", "Don't wait."),
        ("Email ada@example.com the notes", "ada@example.com"),
        (
            "Email ada@example.com the notes",
            "the notes, ada@example.com",
        ),
        (
            "What's the weather in Paris",
            "What's the weather in Paris?",
        ),
        ("What's the weather in Paris", "weather Paris"),
        ("what is the weather in paris", "Weather in Paris."),
        ("Say hello to Anna", "Hi Anna, hello!"),
        ("Remind me at 6 pm to call mom", "Call mom at 6 pm."),
        ("Look up pizza near me", "pizza near me"),
        (
            "Meet at the cafe (the one on 5th)",
            "the cafe (the one on 5th)",
        ),
        (
            "Thanks! See you at the game.",
            "Thanks! See you at the game.",
        ),
    ];
    for (owner, typed_text) in cases {
        assert!(
            typed(typed_text, &[owner]),
            "{typed_text:?} asks for {owner:?}"
        );
    }
}

/// F7: numbers are model text; booleans carry no text; nested values ask.
#[test]
fn tool_arguments_numbers_are_checked() {
    let call = |args: serde_json::Value, owner: &[&str]| {
        egress_owned(
            &Action::CallTool {
                name: "timer_start".into(),
                args,
            },
            owner,
        )
    };
    let owner = ["Start a pasta timer for 12 minutes"];
    assert!(call(
        serde_json::json!({"label": "pasta", "minutes": 12}),
        &owner
    ));
    assert!(!call(
        serde_json::json!({"label": "pasta", "minutes": 13}),
        &owner
    ));
    assert!(!call(
        serde_json::json!({"label": "pasta", "minutes": 482913}),
        &owner
    ));
    assert!(!call(
        serde_json::json!({"label": "pasta", "minutes": 0.5}),
        &owner
    ));
    assert!(call(
        serde_json::json!({"label": "pasta", "on": true}),
        &owner
    ));
    assert!(!call(
        serde_json::json!({"label": "pasta", "x": [1, 2]}),
        &owner
    ));
    assert!(!call(
        serde_json::json!({"label": {"482913": "pasta"}}),
        &owner
    ));
    // A number in the owner's text in another script is the same number.
    assert!(call(
        serde_json::json!({"label": "pasta", "minutes": 12}),
        &["Start a pasta timer for \u{0661}\u{0662} minutes"]
    ));
}
