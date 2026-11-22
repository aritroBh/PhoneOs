#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Ninth review, property test 2: how the owner's question is composed (reserved slots).
//!
//! Random forms — 0 to 60 fields named from benign, recipient, amount and credential
//! vocabularies and adversarial names, values of 0 to 5,000 characters (emails, phone
//! numbers, amounts, injection text, card numbers, codes, noise with quotes, separators and
//! bidi characters), listed or not (scrolled away, through the raw tree's inputs), plus the
//! screen's facts (0 to 6 amounts, at most one the total, 0 to 4 recipients, at most one of
//! them the payee, a merchant's name after "Pay to:") and every action kind, category and
//! source. Invariants, for every summary:
//!
//! 1. it is at most 480 characters;
//! 2. the screen's total amount is in it (for an action in the app);
//! 3. the payee (the first recipient fact) is in it, at least its first 40 characters
//!    (for a paying or sending category);
//! 4. every omission is announced: `Sends:`/`The form holds:` count the form's fields
//!    exactly and list as many as they say are shown; `Amounts on screen:` lists and counts
//!    exactly the screen's amounts;
//! 5. no credential-like field's value, no secure field's value and no card number appears;
//! 6. no field name or value (or any screen text) makes a second `Amounts on screen:`,
//!    `To/Account:`, `Sends:`/`The form holds:`, `Typed:`, `Selected:` or `Fields:`;
//! 7. composing never panics and takes bounded time.
//!
//! The generator is seeded (xorshift, as `jarvis-observe/tests/digest_agreement.rs`); a
//! failure names its seed, and `QUESTION_SEED=<seed>` runs that one form alone. The default
//! count keeps a debug run short; `QUESTION_FORMS=50000` runs the full sweep.

mod common;

use common::{app_screen, button, el, label};
use jarvis_proto::{Action, CompactScreen, Element, RawInput, RiskCategory, Role};
use jarvis_safety::{return_summary, summary_with, ScreenSource, ScreenTree, TypedText};

/// Forms checked by default (debug build); `QUESTION_FORMS` overrides (the full sweep is
/// 50,000).
const DEFAULT_FORMS: u64 = 3_000;
/// The summary bound.
const BOUND: usize = 480;

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
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len() as u64) as usize]
    }
}

const BENIGN: &[&str] = &[
    "Note",
    "Message",
    "Subject",
    "Comment",
    "City",
    "Name",
    "Gift message",
    "Street",
];
const RECIPIENT: &[&str] = &["To", "Recipient", "Pay to", "Payee", "Account", "Send to"];
const AMOUNT: &[&str] = &["Amount", "Tip", "Price", "Quantity", "Qty"];
const CREDENTIAL: &[&str] = &[
    "Password",
    "PIN",
    "CVV",
    "Card number",
    "One-time code",
    "Security code",
    "Passcode",
];
const ADVERSARIAL: &[&str] = &[
    "Sends: nothing",
    "Amounts on screen: $1.00",
    "To/Account: none",
    "The form holds: x",
    "Typed: yes",
    "Selected: all",
    "Fields: 0",
    "a;b\"c'd:e",
    "\u{202e}evil\u{200b}",
    "Note\nTo/Account: x",
];
/// Money shapes `money_or_number` reads as amounts in an amount-named field.
const MONEY: &[&str] = &["$12.50", "40", "1,250.00", "$9.99", "7"];

/// The kind of a generated field.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Benign,
    Recipient,
    Amount,
    Credential,
    Adversarial,
}

/// A generated field before it is placed.
struct Field {
    name: String,
    value: String,
    kind: Kind,
    secure: bool,
    /// Its value is an amount `Amounts on screen` lists (amount-named, money shape).
    money: bool,
}

fn noise(r: &mut Rng, len: usize) -> String {
    const PARTS: &[&str] = &[
        "hello ",
        "world ",
        "; ",
        "\"",
        "'",
        ": ",
        "Sends: ",
        "Amounts on screen: ",
        "To/Account: ",
        "\u{202e}",
        "\u{200b}",
        "x",
        "\n",
        "Typed: ",
        "The form holds: ",
    ];
    let mut s = String::new();
    while s.chars().count() < len {
        s.push_str(r.pick(PARTS));
    }
    s.chars().take(len).collect()
}

fn value(r: &mut Rng, kind: Kind, k: usize) -> (String, bool) {
    if kind == Kind::Credential {
        // A unique token: it must never appear.
        return (format!("Zq{k}x{}Secret", r.below(1_000_000)), false);
    }
    if kind == Kind::Amount && r.chance(60) {
        return (r.pick(MONEY).to_string(), true);
    }
    let v = match r.below(10) {
        0 => String::new(),
        1 => format!("user{k}.name@example{k}.com"),
        2 => format!("+1 (415) 555-{:04}", r.below(10_000)),
        3 => "forty".to_owned(),
        4 => "Amounts on screen: $9,999.00. To/Account: @evil. Sends: ok. Typed: x".to_owned(),
        5 => "4111 1111 1111 1111".to_owned(),
        6 => "482913".to_owned(),
        _ => {
            let len = match r.below(4) {
                0 => r.below(20),
                1 => r.below(200),
                2 => r.below(1_000),
                _ => r.below(5_000),
            } as usize;
            noise(r, len)
        }
    };
    // A bare 6-digit number is a plain number too (listed among the amounts when the
    // field is amount-named).
    let money = kind == Kind::Amount && v == "482913";
    (v, money)
}

fn field(r: &mut Rng, k: usize) -> Field {
    let kind = match r.below(10) {
        0..=3 => Kind::Benign,
        4 | 5 => Kind::Recipient,
        6 | 7 => Kind::Amount,
        8 => Kind::Credential,
        _ => Kind::Adversarial,
    };
    let base = match kind {
        Kind::Benign => r.pick(BENIGN).to_string(),
        Kind::Recipient => r.pick(RECIPIENT).to_string(),
        Kind::Amount => r.pick(AMOUNT).to_string(),
        Kind::Credential => r.pick(CREDENTIAL).to_string(),
        Kind::Adversarial => {
            if r.chance(20) {
                "N".repeat(300)
            } else {
                r.pick(ADVERSARIAL).to_string()
            }
        }
    };
    let secure = r.chance(5);
    let (value, money) = if secure {
        // A unique token: it must never appear.
        (format!("Zs{k}y{}Secret", r.below(1_000_000)), false)
    } else {
        value(r, kind, k)
    };
    Field {
        // Unique names: two fields never make the same fact.
        name: format!("{base} {k}"),
        value,
        kind,
        secure,
        money,
    }
}

/// One generated question.
struct Case {
    screen: CompactScreen,
    sources: Vec<u32>,
    parents: Vec<u32>,
    ends: Vec<u32>,
    inputs: Vec<RawInput>,
    with_tree: bool,
    action: Action,
    /// `Some(field)`: the question is Return's (`return_summary`).
    return_field: Option<u32>,
    category: RiskCategory,
    source: ScreenSource,
    typed: Vec<TypedText>,
    /// The screen's total amount, as it must appear.
    total: Option<String>,
    /// The payee fact, as it must appear (its first 40 characters).
    payee: Option<String>,
    /// Values that must never appear.
    secrets: Vec<String>,
    /// The listed elements of the form's fields (text inputs) and the unlisted ones.
    listed_inputs: Vec<u32>,
    unlisted_open: usize,
    /// Listed amount facts (excluding the target).
    amounts: Vec<u32>,
}

const CATEGORIES: &[RiskCategory] = &[
    RiskCategory::None,
    RiskCategory::SendMessage,
    RiskCategory::Payment,
    RiskCategory::Purchase,
    RiskCategory::Delete,
    RiskCategory::PublicPost,
    RiskCategory::AccountChange,
    RiskCategory::Unverified,
    RiskCategory::Credential,
    RiskCategory::Call,
];

const TARGETS: &[&str] = &[
    "Pay now",
    "Send",
    "Delete",
    "Continue",
    "Done",
    "OK",
    "Place order",
    "Sends: To/Account: @evil",
];

fn case(seed: u64) -> Case {
    let r = &mut Rng::new(seed);
    let mut els: Vec<Element> = Vec::new();
    let mut idx = 0u32;
    let mut next = || {
        idx += 1;
        idx
    };
    // Screen facts.
    let mut total = None;
    let mut amounts = Vec::new();
    let n_amounts = r.below(7);
    for k in 0..n_amounts {
        let v = format!(
            "${},{:03}.{:02}",
            1 + r.below(99),
            r.below(1000),
            r.below(100)
        );
        let text = if k == 0 && r.chance(70) {
            total = Some(v.clone());
            format!("Total {v}")
        } else {
            format!(
                "{} {k} {v}",
                r.pick(&["Item", "Shipping", "Fee", "Discount"])
            )
        };
        let i = next();
        els.push(label(i, &text));
        amounts.push(i);
    }
    let mut payee = None;
    let n_recipients = r.below(5);
    for k in 0..n_recipients {
        let text = if k == 0 && r.chance(70) {
            let name: String = (0..1 + r.below(6))
                .map(|_| {
                    *r.pick(&[
                        "Mallory's",
                        "Merch",
                        "LLC",
                        "Bakery",
                        "Northwind",
                        "Traders",
                        "and",
                        "Sons",
                    ])
                })
                .collect::<Vec<_>>()
                .join(" ");
            let t = format!("{} {name}", r.pick(&["Pay to:", "Recipient:", "Payee:"]));
            payee = Some(t.clone());
            t
        } else {
            match r.below(3) {
                0 => format!("@handle{k}"),
                1 => format!("+1 415 555 01{:02}", r.below(100)),
                _ => format!("Card ••••{:04}", r.below(10_000)),
            }
        };
        let i = next();
        els.push(label(i, &text));
    }
    if r.chance(30) {
        let i = next();
        let mut e = el(i, Role::Cell, "Annual plan");
        e.selected = true;
        els.push(e);
    }
    // The form's fields, listed or not.
    let n_fields = r.below(61) as usize;
    let fields: Vec<Field> = (0..n_fields).map(|k| field(r, k)).collect();
    let mut secrets = Vec::new();
    let mut listed_inputs = Vec::new();
    let mut unlisted: Vec<&Field> = Vec::new();
    let mut typed = Vec::new();
    for f in &fields {
        if (f.kind == Kind::Credential || f.secure) && !f.value.is_empty() {
            secrets.push(f.value.clone());
        }
        if f.value.contains("4111 1111") {
            secrets.push("4111 1111".to_owned());
        }
        if r.chance(25) {
            unlisted.push(f);
            continue;
        }
        let i = next();
        let role = if f.secure {
            Role::SecureTextField
        } else {
            r.pick(&[Role::TextField, Role::TextField, Role::TextView])
                .clone()
        };
        let mut e = el(i, role, &f.name);
        e.value = (!f.value.is_empty()).then(|| f.value.clone());
        e.secure = f.secure;
        if f.money && !f.secure {
            amounts.push(i);
        }
        if !f.secure && r.chance(20) {
            typed.push(TypedText {
                field: e.fingerprint,
                text: f.value.clone(),
            });
        }
        listed_inputs.push(i);
        els.push(e);
    }
    let target = next();
    let target_label = *r.pick(TARGETS);
    els.push(button(target, target_label));
    let mut screen = app_screen(els);
    if r.chance(10) {
        screen.keyboard_visible = true;
        screen.cues.return_key = Some(r.pick(&["send", "return", "go"]).to_string());
    }
    // The raw tree: 0 root, 1 the form, the listed elements, then the unlisted inputs.
    let n = screen.elements.len() as u32;
    let total_nodes = 2 + n + unlisted.len() as u32;
    let sources: Vec<u32> = (0..n).map(|k| 2 + k).collect();
    let mut parents = vec![u32::MAX, 0];
    let mut ends = vec![total_nodes, total_nodes];
    for k in 2..total_nodes {
        parents.push(1);
        ends.push(k + 1);
    }
    let mut inputs: Vec<RawInput> = Vec::new();
    for e in screen.elements.iter().filter(|e| e.role.is_text_input()) {
        inputs.push(RawInput {
            node: 1 + e.index,
            role: e.role.clone(),
            label: Some(e.label.clone()),
            identifier: None,
            placeholder: None,
            value: if e.secure { None } else { e.value.clone() },
            secure: e.secure,
            on_screen: true,
            fingerprint: Some(e.fingerprint),
        });
    }
    let mut unlisted_open = 0;
    for (k, f) in unlisted.iter().enumerate() {
        if !f.secure {
            unlisted_open += 1;
        }
        let fp = 10_000 + k as u64;
        if !f.secure && r.chance(20) {
            typed.push(TypedText {
                field: fp,
                text: f.value.clone(),
            });
        }
        inputs.push(RawInput {
            node: 2 + n + k as u32,
            role: if f.secure {
                Role::SecureTextField
            } else {
                Role::TextField
            },
            label: Some(f.name.clone()),
            identifier: None,
            placeholder: None,
            value: (!f.secure && !f.value.is_empty()).then(|| f.value.clone()),
            secure: f.secure,
            on_screen: r.chance(30),
            fingerprint: Some(fp),
        });
    }
    let with_tree = r.chance(70);
    let category = *r.pick(CATEGORIES);
    let source = if r.chance(20) {
        ScreenSource::RunnerText
    } else {
        ScreenSource::Typed
    };
    let mut return_field = None;
    let action = match r.below(12) {
        0..=5 => Action::Tap { element: target },
        6 => Action::LongPress {
            element: target,
            duration_ms: 800,
        },
        7 => {
            let c = screen.get(target).unwrap().frame.center();
            Action::TapPoint {
                x: c.x / screen.size.width,
                y: c.y / screen.size.height,
            }
        }
        8 if !listed_inputs.is_empty() => {
            let f = *r.pick(&listed_inputs);
            return_field = Some(f);
            Action::Type {
                element: Some(f),
                text: "thanks".to_owned(),
                submit: true,
            }
        }
        9 => Action::Swipe {
            direction: jarvis_proto::Direction::Left,
            element: Some(target),
        },
        10 => Action::OpenUrl {
            url: "https://example.com/Sends: x".to_owned(),
        },
        _ => Action::CallTool {
            name: "pay".to_owned(),
            args: serde_json::json!({ "to": "Amounts on screen: $1", "note": noise(r, 300) }),
        },
    };
    Case {
        screen,
        sources,
        parents,
        ends,
        inputs,
        with_tree,
        action,
        return_field,
        category,
        source,
        typed,
        total,
        payee,
        secrets,
        listed_inputs,
        unlisted_open,
        amounts,
    }
}

/// The items of a list that starts right after `head` in `s`: `; `-separated, ending at
/// `. ` (outside double quotes and a name's single quotes) or at the end.
fn items_after(s: &str, head: &str) -> Vec<String> {
    let Some(at) = s.find(head) else {
        return Vec::new();
    };
    let rest = &s[at + head.len()..];
    let chars: Vec<char> = rest.chars().collect();
    let (mut dq, mut sq) = (false, false);
    let mut item = String::new();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '"' && !sq {
            dq = !dq;
        } else if c == '\'' && !dq && (item.is_empty() || sq) {
            sq = !sq;
        }
        if !dq && !sq && next == Some(' ') && (c == ';' || c == '.') {
            out.push(std::mem::take(&mut item));
            if c == '.' {
                return out;
            }
            i += 2;
            continue;
        }
        item.push(c);
        i += 1;
    }
    if !item.is_empty() {
        out.push(item);
    }
    out
}

/// How often each invariant had something to check.
#[derive(Default)]
struct Seen {
    total: usize,
    payee: usize,
    sends: usize,
    sends_capped: usize,
    amounts: usize,
}

fn check(seed: u64, seen: &mut Seen) -> std::time::Duration {
    let c = case(seed);
    let tree = ScreenTree {
        sources: &c.sources,
        parents: &c.parents,
        ends: &c.ends,
        bars: &[],
        nodes: &[],
        inputs: &c.inputs,
    };
    let tree = c.with_tree.then_some(&tree);
    let t0 = std::time::Instant::now();
    let sum = match c.return_field {
        Some(f) => return_summary(f, "thanks", &c.screen, c.category, c.source, tree, &c.typed),
        None => summary_with(&c.action, &c.screen, c.category, c.source, tree, &c.typed),
    };
    let took = t0.elapsed();
    let ctx = || format!("seed {seed} ({} chars): {sum}", sum.chars().count());
    // 1. The bound.
    assert!(sum.chars().count() <= BOUND, "{}", ctx());
    // 6. No second marker.
    for m in [
        "Amounts on screen:",
        "To/Account:",
        "Typed:",
        "Selected:",
        "Fields:",
    ] {
        assert!(sum.matches(m).count() <= 1, "{m}: {}", ctx());
    }
    assert!(
        sum.matches("Sends:").count() + sum.matches("The form holds:").count() <= 1,
        "{}",
        ctx()
    );
    // 5. No secret.
    for s in &c.secrets {
        assert!(!sum.contains(s.as_str()), "secret {s:?}: {}", ctx());
    }
    let in_app = !matches!(c.action, Action::OpenUrl { .. } | Action::CallTool { .. });
    let target_idx = match &c.action {
        Action::Tap { element } | Action::LongPress { element, .. } => Some(*element),
        Action::Swipe { element, .. } => *element,
        Action::Type { element, .. } => *element,
        Action::TapPoint { .. } => c.screen.elements.last().map(|e| e.index),
        _ => None,
    };
    // 2. The total.
    if let (true, Some(t)) = (in_app, &c.total) {
        seen.total += 1;
        assert!(sum.contains(t.as_str()), "total {t}: {}", ctx());
    }
    // 3. The payee.
    let pays = matches!(
        c.category,
        RiskCategory::Payment | RiskCategory::SendMessage | RiskCategory::Purchase
    );
    if let (true, true, Some(p)) = (in_app, pays, &c.payee) {
        seen.payee += 1;
        let head: String = p.chars().take(40).collect();
        assert!(sum.contains(&head), "payee {p:?}: {}", ctx());
        assert!(sum.contains("To/Account: "), "{}", ctx());
    }
    // 4. Counts. What is sent: the header's count is the form's fields.
    let head = ["Sends: ", "The form holds: "]
        .into_iter()
        .find(|h| sum.contains(h));
    if let Some(h) = head {
        seen.sends += 1;
        let others = c
            .listed_inputs
            .iter()
            .filter(|i| Some(**i) != target_idx)
            .filter(|i| !c.screen.get(**i).unwrap().secure)
            .count();
        let expected = match c.return_field {
            Some(_) => 1 + others + if c.with_tree { c.unlisted_open } else { 0 },
            None => others + if c.with_tree { c.unlisted_open } else { 0 },
        };
        let at = sum.find(h).unwrap() + h.len();
        let rest = &sum[at..];
        let counted = rest
            .split_once(" fields (")
            .filter(|(n, _)| n.chars().all(|c| c.is_ascii_digit()))
            .map(|(n, more)| {
                let shown: usize = more.split(' ').next().unwrap().parse().unwrap();
                (n.parse::<usize>().unwrap(), shown)
            });
        match counted {
            Some((n, shown)) => {
                seen.sends_capped += 1;
                assert_eq!(n, expected, "field count: {}", ctx());
                assert!(shown < n, "{}", ctx());
                let items = if shown == 0 {
                    Vec::new()
                } else {
                    items_after(&sum, " shown): ")
                };
                assert_eq!(items.len(), shown, "{items:?}: {}", ctx());
            }
            None => {
                let items = items_after(&sum, h);
                assert_eq!(items.len(), expected, "{items:?}: {}", ctx());
            }
        }
    }
    // The amounts: listed and counted exactly.
    if sum.contains("Amounts on screen: ") {
        seen.amounts += 1;
        let expected = c
            .amounts
            .iter()
            .filter(|i| Some(**i) != target_idx)
            .filter(|i| {
                let e = c.screen.get(**i).unwrap();
                !e.label.to_lowercase().contains("password")
            })
            .count();
        let mut items = items_after(&sum, "Amounts on screen: ");
        let mut more = 0usize;
        if let Some(last) = items.last_mut() {
            if let Some((a, b)) = last.rsplit_once(" and ") {
                if let Some(n) = b.strip_suffix(" more") {
                    more = n.parse().unwrap();
                    *last = a.to_owned();
                }
            }
        }
        assert_eq!(items.len() + more, expected, "amounts {items:?}: {}", ctx());
    } else if in_app {
        let any = c.amounts.iter().any(|i| Some(*i) != target_idx);
        assert!(!any, "amounts left out silently: {}", ctx());
    }
    took
}

#[test]
fn every_question_keeps_the_charge_and_the_payee_and_announces_what_it_leaves_out() {
    if let Some(seed) = std::env::var("QUESTION_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        check(seed, &mut Seen::default());
        return;
    }
    let forms = std::env::var("QUESTION_FORMS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_FORMS);
    let mut slowest = std::time::Duration::ZERO;
    let mut seen = Seen::default();
    for seed in 1..=forms {
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(seed, &mut seen)));
        let took = match run {
            Ok(t) => t,
            Err(e) => {
                let msg = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                    .unwrap_or_default();
                panic!("seed {seed} failed (run alone: QUESTION_SEED={seed}): {msg}");
            }
        };
        slowest = slowest.max(took);
    }
    println!(
        "{forms} forms; slowest summary {slowest:?}; checked: total {}, payee {}, sends {} \
         ({} capped), amounts {}",
        seen.total, seen.payee, seen.sends, seen.sends_capped, seen.amounts
    );
    // The generator exercises every invariant.
    assert!(seen.total > 0 && seen.payee > 0 && seen.sends_capped > 0 && seen.amounts > 0);
    // 7. Bounded time (a debug build; generous).
    assert!(slowest.as_secs() < 2, "{slowest:?}");
}
