#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 38: break-it tests on commit `341f0de` (one normalisation in front of both
//! matchers, `jarvis_safety::norm`; the email rule for `@`; the new glyphs of the sets; the
//! whole-label lowering rule; the soft-boundary bound).
//!
//! Each `f…` test fails today: a label a system font draws as an English risk word is
//! tapped unasked under the default policy, or passes a policy that denies its category.
//! Each `sound_…` test is something tried and found to hold. None uses a pair the commit
//! states it does not fold (`rn`/`m`, `vv`/`w`, `cl`/`d`), other elements' texts, or
//! non-English words.
//!
//! The generated tables of `norm.rs` were checked outside this file against Python's
//! `unicodedata` 15.1.0 over every code point: `MARK_RANGES` is exactly General_Category
//! Mn + Me (1,998 code points) and `BASE_OF` exactly the recipe in `norm.rs` (1,993
//! entries), no disagreement.

mod common;

use common::{app_screen, button, el, field, label, screen};
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
        let deny = Policy {
            deny_categories: vec![want],
            ..Policy::default()
        };
        if matches!(decide_as(&deny, role.clone(), l), Decision::Allow) {
            wrong.push(format!(
                "{role:?} {l:?} under a policy denying {want:?}: Allow (the real word is refused)"
            ));
        }
    }
    wrong
}

/// Whether every label of `labels` asks (is not allowed) as a control of `role`.
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

// ---------------------------------------------------------------------------------------
// Findings.
// ---------------------------------------------------------------------------------------

/// F1 (high): the email rule keeps `@` plain for "the last `@` of a whitespace-separated
/// word when a `.` follows it in that word" — and a risk word with `@` for `a` is such a
/// word as soon as a full stop ends it. "P@y.", "Tr@nsfer.", "C@ll.", "Del@te.",
/// "Don@te.", "P@y..." (no address: nothing after the dot, no host) read `p`, `y` /
/// `tr`, `nsfer`…: tapped unasked, and allowed under a policy denying the category; the
/// same labels without the stop ("P@y", "Sw@p") ask. A sentence-style button ("P@y.",
/// "C@ll.") or a trailing ellipsis is ordinary UI text, so an attacker needs nothing
/// unusual. Should: an `@` is an address's only where the word is shaped like one — text
/// before it and a host of letters on both sides of a dot after it (`anna@mail.com`); a
/// trailing stop or ellipsis is punctuation. (`P@y.com` is a real address shape and is
/// left out here; it reads `Pay.com` to a person and is allowed today too.)
#[test]
fn f1_a_risk_word_ending_in_a_full_stop_keeps_its_at_plain() {
    assert_eq!(tap_as(Role::Button, "P@y"), RiskCategory::Payment);
    assert_eq!(tap_as(Role::Button, "Sw@p"), RiskCategory::Payment);
    let pairs = [
        ("P@y.", "Pay."),
        ("Tr@nsfer.", "Transfer."),
        ("Sw@p.", "Swap."),
        ("C@ll.", "Call."),
        ("Del@te.", "Delete."),
        ("Don@te.", "Donate."),
        ("P@y...", "Pay..."),
        ("Upgr@de.", "Upgrade."),
    ];
    let mut wrong = Vec::new();
    for role in [Role::Button, Role::Link, Role::Cell] {
        wrong.extend(misses(&role, &pairs));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F2 (high on rows): the Latin letters U+01C0 `ǀ` (dental click, drawn as a plain
/// vertical stroke, like `l` or `I`) and U+01C1 `ǁ` (lateral click, two strokes, like
/// `ll`) have no decomposition and are not in the fold table; the "readable letter" test
/// counts all of Latin Extended-B as readable, and on a `Cell` (a free-text role) an
/// extended Latin letter is not a language signal, so nothing asks. "Deǀete Account",
/// "Caǁ", "Seǁ", "Bǀock", "ǀog Out", "Uninstaǁ" as list rows are tapped unasked and pass a
/// policy denying the category (on a Button, Link or Switch they are `Unverified`, asked).
/// Should: a letter counts as readable only when the tables read it (it folds or
/// decomposes to ASCII) or is one of the name letters (ß æ ø đ ł þ ð œ ŋ ħ ŧ); anything
/// else in a pressable row is `Unverified`. A Settings-like list row "Deǀete Account" is
/// exactly what a person reads as Delete Account.
#[test]
fn f2_click_letters_hide_a_risk_word_on_a_row() {
    let wrong = misses(
        &Role::Cell,
        &[
            ("De\u{1C0}ete Account", "Delete Account"),
            ("Ca\u{1C1}", "Call"),
            ("Se\u{1C1}", "Sell"),
            ("B\u{1C0}ock", "Block"),
            ("\u{1C0}og Out", "Log Out"),
            ("Uninsta\u{1C1}", "Uninstall"),
        ],
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F3 (medium, as pass 37's F6): the sets read ASCII `|` as `i`/`l` and `@`, `$` as `a`,
/// `s`, but not the characters drawn the same: the divides sign U+2223 `∣`, the box
/// drawing light vertical U+2502 `│`, the vertical line extension U+23D0 `⏐`, the
/// halfwidth light vertical U+FFE8 `￨`, the light vertical bar U+2758 `❘`, the Hebrew
/// paseq U+05C0 `׀`, the broken bar U+00A6 `¦`, the double vertical line U+2016 `‖` (for
/// `ll`), the small commercial at U+FE6B `﹫` and small dollar U+FE69 `﹩`. None is a
/// letter, so nothing is `Unverified`: "Ca∣∣", "De│ete Account", "Se‖", "P﹫y", "﹩end" are
/// tapped unasked and pass a policy denying the category, on a Button as on a row. Should:
/// the sets' symbol table is the compatibility (NFKC) fold of every symbol plus Unicode's
/// confusables of `| @ $ !`, not a hand list; a symbol inside a word that the table does
/// not know makes a pressable control `Unverified`.
#[test]
fn f3_vertical_bars_and_small_forms_outside_the_sets() {
    let pairs = [
        ("Ca\u{2223}\u{2223}", "Call"),
        ("De\u{2502}ete Account", "Delete Account"),
        ("Ca\u{23D0}\u{23D0}", "Call"),
        ("De\u{FFE8}ete", "Delete"),
        ("Ca\u{2758}\u{2758}", "Call"),
        ("Ca\u{5C0}\u{5C0}", "Call"),
        ("De\u{A6}ete", "Delete"),
        ("Se\u{2016}", "Sell"),
        ("P\u{FE6B}y", "Pay"),
        ("\u{FE69}end", "Send"),
    ];
    let mut wrong = Vec::new();
    for role in [Role::Button, Role::Cell] {
        wrong.extend(misses(&role, &pairs));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F4 (low): the parenthesized small letters U+249C..U+24B5 (`⒟⒠⒧⒠⒯⒠`, compatibility forms
/// of `(d)(e)…`) are symbols to the gate (not letters, not folded as the circled letters
/// U+24B6..U+24E9 are), so beside an ordinary word they are no label at all: "⒟⒠⒧⒠⒯⒠
/// Account" and "⒮⒠⒩⒟ Money" are tapped unasked (alone they are unlabeled, asked). Low:
/// each letter is drawn in visible parentheses. Should: fold them as the circled letters.
#[test]
fn f4_parenthesized_letters_beside_a_word() {
    let wrong = misses(
        &Role::Button,
        &[
            (
                "\u{249F}\u{24A0}\u{24A7}\u{24A0}\u{24AF}\u{24A0} Account",
                "Delete Account",
            ),
            ("\u{24AE}\u{24A0}\u{24A9}\u{249F} Money", "Send Money"),
        ],
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

// ---------------------------------------------------------------------------------------
// Sound.
// ---------------------------------------------------------------------------------------

/// Sound: real addresses still keep `@` plain and a risk word beside one asks; the email
/// rule's split at a narrow space holds (`P@y\u{2009}.` is two words: `P@y` asks); an `@`
/// that is not its word's last reads `a`.
#[test]
fn sound_the_email_rule_on_real_addresses() {
    for l in [
        "anna@mail.com",
        "bob@x.io",
        "help@charity.org",
        "Mail anna@mail.com",
    ] {
        assert_eq!(tap_as(Role::Button, l), RiskCategory::None, "{l:?}");
    }
    let wrong = all_ask(
        &Role::Button,
        &[
            "P@y anna@mail.com",
            "P@y\u{2009}.",
            "P@y@mail.com",
            "Tr@nsfer to bob@x.io",
            "P@y now.",
            "P@y,",
            "P@y!",
            "Make @dmin",
        ],
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Sound: narrow spaces and invisibles inside and between the words of a phrase, with
/// spelled-out joins and symbols; a vertical tab is dropped. Spaces that are drawn wide
/// (tab, NBSP, ideographic space, ogham space mark) and line or paragraph breaks (U+2028,
/// U+2029, NEL) split a word as a visible gap or a new line does: "De\u{3000}lete" is two
/// words to the eye too, and is not read as Delete (not a finding).
#[test]
fn sound_narrow_spaces_join_and_wide_spaces_split() {
    let wrong = all_ask(
        &Role::Button,
        &[
            "Delete\u{2009}Account",
            "Delete\u{200B}Account",
            "Delete\u{2060}Account",
            "Sign\u{200A}Out",
            "P\u{2009}a\u{2009}y",
            "$\u{2009}E\u{2009}N\u{2009}D",
            "P@y\u{2009}anna@mail.com",
            "De\u{B}lete",
            "Sign\u{3000}Out",
            "Sign\u{A0}Out",
            "Tran\u{202F}sfer",
            "Rem\u{205F}ove",
        ],
    );
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
}

/// Sound: the order of the steps. A mark between a letter and a look-alike digit, a
/// capital with dot (İ decomposes to I, then case), the Kelvin and Ångström signs, a
/// ligature (unreadable: asked), a decomposition into a fullwidth or styled form folded
/// again, math and circled letters, squared and negative-squared letters (alphabetic,
/// unreadable: asked), small capitals.
#[test]
fn sound_decompose_drop_fold_case_in_order() {
    let wrong = all_ask(
        &Role::Button,
        &[
            "De1\u{301}ete",
            "D\u{301}e\u{20DD}lete",
            "S\u{130}GN OUT",
            "Bloc\u{212A}",
            "Turn O\u{FB00}",
            "Inst\u{FB06}all",
            "Turn \u{1D40E}\u{1D41F}\u{1D41F}",
            "\u{1D4AB}\u{1D4B6}\u{1D4CE} now",
            "\u{24C5}\u{24B6}\u{24CE} now",
            "\u{1F13F}\u{1F130}\u{1F148} Now",
            "\u{1F17F}\u{1F170}\u{1F188} Now",
            "\u{1F133}\u{1F134}\u{1F13B}\u{1F134}\u{1F143}\u{1F134} Account",
            "Sign Ou\u{1D1B}",
            "De\u{29F}ete",
            "Ca\u{2160}\u{2160}",
            "Si\u{1D4F}n Out",
            "Pay\u{FE0F}",
        ],
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    let wrong = all_ask(
        &Role::Cell,
        &[
            "Turn O\u{FB00}",
            "\u{1F133}\u{1F134}\u{1F13B}\u{1F134}\u{1F143}\u{1F134} Account",
            "De\u{2160}ete",
        ],
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Sound: the whole-label lowering rule on a checkout screen (a price and payment wording
/// beside the control). Labels that are navigation words only stay lowered as before;
/// anything else asks `Payment`, including "Done", "Next", "Continue", "Back Up Now",
/// "Go Back", "Back 2", a spelled-out "B a c k", "Done ✓" and "Cancel Order". Nothing the
/// old rule asked is lowered now (the rule only narrowed). A look-alike spelling of a
/// navigation word lowers only where the plain reading already reads the word (Cyrillic
/// `а` in "Bаck" is drawn as and read as "Back": no more permissive than the ASCII word).
#[test]
fn sound_the_whole_label_lowering_rule_on_a_checkout() {
    let checkout = |l: &str| {
        let s = app_screen(vec![
            button(1, l),
            label(2, "Total $24.99"),
            label(3, "Card ending 4242"),
        ]);
        classify(&Action::Tap { element: 1 }, &s, None)
    };
    for l in [
        "Done",
        "Next",
        "Continue",
        "Back Up Now",
        "Go Back",
        "Back to Cart",
        "Back 2",
        "Back (2)",
        "B a c k",
        "Done \u{2713}",
        "Skip",
        "Previous",
        "Return",
        "Cancel Order",
    ] {
        assert_eq!(checkout(l), RiskCategory::Payment, "{l:?}");
    }
    for l in [
        "Back",
        "Close",
        "Cancel",
        "Not Now",
        "Dismiss",
        "\u{2039} Back",
        "Back \u{203A}",
        "Close \u{2715}",
        "Close Back",
        "B\u{430}ck",
    ] {
        assert_eq!(checkout(l), RiskCategory::None, "{l:?}");
    }
}

/// Sound: ordinary product rows with model numbers and prices, and short count and size
/// labels, gain no category from the new glyphs (`9` g, `6` b/g, `2` z, `£ ¥ € ¢`).
#[test]
fn sound_model_numbers_and_prices_do_not_ask_as_rows() {
    for l in [
        "Volvo S60, \u{A3}32,995",
        "Volvo XC90 \u{A3}58,000",
        "Nike Air Max 90, \u{A3}120",
        "Galaxy S9, \u{A3}199",
        "Canon R6, \u{A3}2,099",
        "Sony A6000, \u{A3}399",
        "LG C2 65in, \u{A3}1,299",
        "Road Bike 29er, \u{A3}899",
        "DJI Mini 2, \u{20AC}459",
        "Audi A6, \u{20AC}52,000",
        "Lenovo Yoga 9i, \u{A3}1,399",
        "GoPro HERO9 \u{A3}249",
        "Tablet 6in",
    ] {
        assert_eq!(tap_as(Role::Cell, l), RiskCategory::None, "{l:?}");
    }
    for l in [
        "6 items",
        "Page 2 of 9",
        "Top 10",
        "Wi-Fi 6",
        "iPhone 16",
        "Buy 2 Get 1",
        "Size 6",
        "Season 2",
        "9am",
        "6pm",
        "2nd",
        "16GB",
        "Gen 2",
        "Ages 9+",
    ] {
        let want = jarvis_safety::label_risk(l);
        assert_eq!(tap_as(Role::Button, l), want, "{l:?}");
    }
}

/// Sound (counted, feature `reference`): a generated corpus of 346,560 ordinary short
/// labels (95 prefixes such as `iPhone `, `S`, `Gen `, `Page `, `£`, `€`, `Total £` ×
/// 43 numbers × 85 suffixes such as `GB`, `in`, `pm`, `x`, ` items`, ` of 9`, `€`), each
/// tapped as a Button and as a Cell: measured 284 taps (0.04 %) gain a category from the
/// sets the plain reading does not give — 10 Buttons (`6in`, `8in`, `#6in`, `#8in` read
/// `bin`; `B1D` bid; `C4rd`; `T1p`; `S3nd`, `Galaxy S3nd`; `$3nd`) and 274 Cells, almost
/// all a two-word priced row whose `60`/`90` reads `go` beside a currency (`S60 £`,
/// `iPhone 60ft`: `ft` is the forint), which is then not an item-and-price row. Asserted:
/// at most 0.1 % of taps; printed.
#[cfg(feature = "reference")]
#[test]
fn sound_a_generated_corpus_of_counts_and_prices_rarely_gains_a_category() {
    use jarvis_safety::reference::tap_reading_risks;
    let nums = [
        "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "12", "16", "20", "22", "24", "26",
        "29", "32", "36", "42", "49", "60", "64", "66", "69", "90", "96", "99", "128", "256",
        "360", "512", "699", "999", "1080", "2024", "2026", "4K", "9.99", "2.99", "6.49", "19.99",
    ];
    let pre = [
        "",
        "iPhone ",
        "iPhone",
        "S",
        "M",
        "Gen ",
        "Gen",
        "No. ",
        "#",
        "v",
        "Q",
        "W",
        "A",
        "B",
        "C",
        "PS",
        "H",
        "F",
        "R",
        "T",
        "Z",
        "X",
        "G",
        "E",
        "L",
        "Page ",
        "Top ",
        "Size ",
        "Season ",
        "Episode ",
        "Track ",
        "Chapter ",
        "Level ",
        "Step ",
        "Room ",
        "Gate ",
        "Seat ",
        "Bus ",
        "Route ",
        "Apt ",
        "Model ",
        "Galaxy S",
        "Pixel ",
        "Note ",
        "Wi-Fi ",
        "AirPods Pro ",
        "Watch Series ",
        "Buy ",
        "Get ",
        "Mix ",
        "Plan ",
        "Area ",
        "Zone ",
        "Kids ",
        "Ages ",
        "Day ",
        "Week ",
        "Vol. ",
        "Part ",
        "Act ",
        "Lot ",
        "Box ",
        "Bay ",
        "Lane ",
        "Ch",
        "Ep",
        "S0",
        "E0",
        "\u{A3}",
        "\u{20AC}",
        "\u{A5}",
        "\u{A2}",
        "$",
        "Total \u{A3}",
        "Price \u{20AC}",
        "Was \u{A3}",
        "Now \u{20AC}",
        "Save \u{A3}",
        "Only \u{A5}",
        "Sale ",
        "Deal ",
        "Item ",
        "Cell ",
        "Line ",
        "Table ",
        "Block ",
        "Bed ",
        "Lift ",
        "Unit ",
        "Deck ",
        "Big ",
        "Bag ",
        "Leg ",
        "Ice ",
        "Eye ",
        "Bee ",
    ];
    let suf = [
        "",
        " items",
        " Items",
        "GB",
        "MB",
        "TB",
        "K",
        "x",
        "X",
        "p",
        "fps",
        "Hz",
        "mAh",
        "st",
        "nd",
        "rd",
        "th",
        "am",
        "pm",
        "AM",
        "PM",
        " min",
        "h",
        "s",
        "kg",
        "lb",
        "lbs",
        "oz",
        "ml",
        "L",
        "mm",
        "cm",
        "in",
        "ft",
        "%",
        "+",
        " Pro",
        "G",
        "E",
        "a",
        "b",
        "c",
        "e",
        "g",
        "i",
        "l",
        "y",
        "z",
        "ize",
        "ig",
        "ag",
        "ee",
        " of 9",
        " of 6",
        " of 2",
        " for 2",
        " Get 1",
        " Get 1 Free",
        " \u{A3}",
        " \u{20AC}",
        "\u{20AC}",
        "\u{A3}",
        "\u{A5}",
        "\u{A2}",
        " left",
        " new",
        " more",
        " days",
        " weeks",
        "-pack",
        "-step",
        "D",
        "B",
        "Gb",
        "Mb",
        "kb",
        "KB",
        "Y",
        "LE",
        "SE",
        "GE",
        "BE",
        "ZE",
        "IE",
    ];
    let mut seen = std::collections::BTreeSet::new();
    let (mut taps, mut gained) = (0usize, Vec::new());
    for p in pre {
        for n in nums {
            for s in suf {
                let l = format!("{p}{n}{s}");
                if !seen.insert(l.clone()) {
                    continue;
                }
                for role in [Role::Button, Role::Cell] {
                    taps += 1;
                    let sc = app_screen(vec![el(1, role.clone(), &l)]);
                    if let [old, new] = tap_reading_risks(&Action::Tap { element: 1 }, &sc)[..] {
                        if old != new && new != RiskCategory::None {
                            gained.push(format!("{l:?} {role:?}: {old:?} -> {new:?}"));
                        }
                    }
                }
            }
        }
    }
    println!(
        "{} labels, {taps} taps, {} gained",
        seen.len(),
        gained.len()
    );
    for g in gained.iter().filter(|g| g.contains("Button")) {
        println!("  {g}");
    }
    assert!(gained.len() * 1000 <= taps, "{} of {taps}", gained.len());
}

/// Sound (counted, feature `reference`): the cost of the sets on hostile labels just under
/// the soft-boundary bound against ordinary text of the same length, per character, and a
/// whole page: 3,000 buttons of 512 characters, one tapped, and a point over a stack. The
/// worst unit found ("3rase a11 c0ntent and ", 92 soft boundaries) walks 3.2 trie steps per
/// character against 1.1 for ordinary words; labels over the bound walk none. Asserted on
/// counted steps (at most 4x per character); the page timings are printed (debug build:
/// hostile and ordinary pages within the same order).
#[cfg(feature = "reference")]
#[test]
fn sound_hostile_labels_under_the_bound_cost_bounded_work() {
    use jarvis_safety::reference::{soft_boundaries, walk_steps, MAX_SOFT_BOUNDARIES};
    let fit = |unit: &str| {
        let mut s = String::new();
        while s.chars().count() + unit.chars().count() <= 512 {
            s.push_str(unit);
        }
        s
    };
    let steps = |l: &str| {
        let s = app_screen(vec![el(1, Role::Button, l)]);
        let b = walk_steps();
        let _ = classify(&Action::Tap { element: 1 }, &s, None);
        (walk_steps() - b) as f64 / l.chars().count() as f64
    };
    let ordinary = fit("Recently played songs ");
    let base = steps(&ordinary).max(1.0);
    for unit in [
        "3rase a11 c0ntent and ",
        "Il1|!\u{A1} ",
        "1111 ",
        "6969 ",
        "\u{A3}\u{20AC}\u{A3}\u{20AC} ",
        "De1ete ",
        "S1gn 0ut 0f a11 ",
    ] {
        let l = fit(unit);
        let per = steps(&l);
        println!(
            "{unit:?}: {} soft, {per:.2} steps/char (ordinary {base:.2})",
            soft_boundaries(&l)
        );
        if soft_boundaries(&l) > MAX_SOFT_BOUNDARIES {
            assert_eq!(per, 0.0, "{unit:?}");
        } else {
            assert!(per <= 4.0 * base, "{unit:?}: {per} vs {base}");
        }
    }
    let page = |text: &str| -> (u64, f64) {
        let els: Vec<Element> = (0..3000u32)
            .map(|i| {
                let mut e = button(i + 1, text);
                e.frame = Rect::new(16.0, 300.0, 361.0, 44.0);
                e
            })
            .collect();
        let s = app_screen(els);
        let b = walk_steps();
        let t = std::time::Instant::now();
        for a in [
            Action::Tap { element: 1 },
            Action::TapPoint {
                x: 196.0 / 393.0,
                y: 322.0 / 852.0,
            },
        ] {
            assert!(
                !matches!(evaluate(&Policy::default(), &a, &s, None), Decision::Allow)
                    || text.starts_with("Recently")
            );
        }
        (walk_steps() - b, t.elapsed().as_secs_f64())
    };
    let (n0, t0) = page(&ordinary);
    let (n1, t1) = page(&fit("3rase a11 c0ntent and "));
    println!("3,000 x 512: ordinary {n0} steps {t0:.3} s, hostile {n1} steps {t1:.3} s");
    assert!(n1 as f64 <= 4.0 * (n0 as f64).max(1.0), "{n1} vs {n0}");
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

/// Characters of other scripts and forms the tables do not cover: Hangul syllables and
/// jamo (with fillers), Arabic presentation forms, Indic two-part vowel signs (whose
/// decomposition holds a spacing mark), CJK compatibility ideographs (singleton
/// decompositions), musical note symbols, long mark runs, variation selectors.
fn other_script_char(r: &mut Rng) -> char {
    let ranges: [(u32, u32); 12] = [
        (0xAC00, 0xD7A3),
        (0x1100, 0x11FF),
        (0x3130, 0x318F),
        (0xFB50, 0xFDFF),
        (0xFE70, 0xFEFF),
        (0x0BCA, 0x0BCC),
        (0x0DDA, 0x0DDE),
        (0xF900, 0xFAFF),
        (0x1D15E, 0x1D164),
        (0x0300, 0x036F),
        (0xE0100, 0xE01EF),
        (0x2000, 0x206F),
    ];
    let (lo, hi) = ranges[r.below(ranges.len())];
    let u = lo + u32::try_from(r.below(usize::try_from(hi - lo + 1).unwrap())).unwrap();
    char::from_u32(u).unwrap_or('x')
}

/// Sound: no panic and idempotent normalisation on other scripts (Hangul, Arabic
/// presentation forms, Indic, CJK compatibility, marks), mixed with risk words, through
/// every action kind that reads a label; a label of 3,000 marks on one letter is asked.
#[test]
fn sound_other_scripts_never_panic_and_normalise_once() {
    let mut r = Rng(0x5EED_0038_0000_0001);
    for k in 0..3000u32 {
        let len = if k % 40 == 0 { 520 } else { r.below(30) };
        let mut t: String = (0..len).map(|_| other_script_char(&mut r)).collect();
        if k % 3 == 0 {
            t.insert_str(0, "Delete ");
        }
        #[cfg(feature = "reference")]
        {
            let once = jarvis_safety::reference::normalized(&t);
            assert!(once.chars().count() <= t.chars().count());
            assert_eq!(jarvis_safety::reference::normalized(&once), once, "{t:?}");
        }
        let mut e = el(
            1,
            [Role::Button, Role::Cell, Role::Tab][k as usize % 3].clone(),
            &t,
        );
        if k % 2 == 0 {
            e.value = Some(t.clone());
        }
        let mut f = field(2, Role::TextField, &t);
        f.placeholder = Some(t.clone());
        let s = screen("com.example.app", "X", vec![e, f]);
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
                direction: Direction::Right,
                element: Some(1),
            },
            Action::Type {
                element: Some(2),
                text: t.clone(),
                submit: true,
            },
        ] {
            let _ = classify(&a, &s, None);
            let _ = evaluate(&Policy::default(), &a, &s, None);
        }
    }
    let marks = format!("D{}elete", "\u{301}".repeat(3000));
    assert!(!matches!(
        decide_as(&Policy::default(), Role::Button, &marks),
        Decision::Allow
    ));
}

/// Sound: a 20,000-node screen of stacked buttons whose labels mix other scripts (Hangul,
/// Arabic presentation forms, marks…) decides a tap on a generic "OK" and a point over
/// the stack, against a same-size screen of ordinary ASCII labels of the same length:
/// best of two, the ratio printed and asserted at most 6x (measured 2.8x in a debug build:
/// 1.7 s against 4.8 s; low, nothing is pressed unasked).
#[test]
fn sound_a_twenty_thousand_element_screen_costs_what_an_ordinary_one_does() {
    let build = |other: bool| -> CompactScreen {
        let mut r = Rng(0x5EED_0038_0000_0002);
        let els: Vec<Element> = (0..20_000u32)
            .map(|i| {
                let t: String = (0..12)
                    .map(|_| {
                        if other {
                            other_script_char(&mut r)
                        } else {
                            char::from(b'a' + u8::try_from(r.below(26)).unwrap())
                        }
                    })
                    .collect();
                let mut e = button(i + 1, &format!("Item {i} {t}"));
                e.frame = Rect::new(16.0, 300.0, 361.0, 44.0);
                e
            })
            .chain(std::iter::once(button(30_001, "OK")))
            .collect();
        app_screen(els)
    };
    let time = |s: &CompactScreen| {
        (0..2)
            .map(|_| {
                let t = std::time::Instant::now();
                let _ = evaluate(
                    &Policy::default(),
                    &Action::Tap { element: 30_001 },
                    s,
                    None,
                );
                let p = evaluate(
                    &Policy::default(),
                    &Action::TapPoint {
                        x: 196.0 / 393.0,
                        y: 322.0 / 852.0,
                    },
                    s,
                    None,
                );
                assert!(!matches!(p, Decision::Allow));
                t.elapsed().as_secs_f64()
            })
            .fold(f64::INFINITY, f64::min)
    };
    let ordinary = time(&build(false));
    let other = time(&build(true));
    println!("20,000 elements: ordinary {ordinary:.3} s, other scripts {other:.3} s");
    assert!(other <= 6.0 * ordinary, "{other} vs {ordinary}");
}
