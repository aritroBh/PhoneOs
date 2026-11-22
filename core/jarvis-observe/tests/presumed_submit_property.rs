#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Ninth review, property test 3: a tap after Jarvis typed is presumed to submit.
//!
//! Random sequences of type / clear / owner edit / scroll / tap over random forms (one or
//! two form containers in a page scroller, fields of every kind — message, login, search,
//! secure — and buttons, neutral, icon-named or from the table of controls that clearly do
//! not submit). Property, checked at every tap on a listed button: if Jarvis typed into a
//! non-search, non-secure field of the button's form and that field still holds the text
//! (listed or scrolled out of the list), and the button is not in the not-a-submit table and
//! not excluded by narrowing (a) (a login form: a secure field, and only email or username
//! fields filled), the tap asks. The oracle is written here (its own copy of the table and
//! of the login words), not with the rule's helpers.
//!
//! Seeded (xorshift); a failure names its seed (`SUBMIT_SEED=<seed>` runs it alone); the
//! default is 20,000 sequences, `SUBMIT_SEQUENCES=<n>` changes it.

use jarvis_observe::compact_view;
use jarvis_proto::fixtures::{node, screen};
use jarvis_proto::{Action, Policy, Rect, Role, Screen, UiNode};
use jarvis_safety::{evaluate_with, Decision, ScreenSource, ScreenTree, TypedText};

const W: f64 = 393.0;
const H: f64 = 852.0;
/// Sequences checked by default; `SUBMIT_SEQUENCES` overrides.
const DEFAULT_SEQUENCES: u64 = 20_000;

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

/// The oracle's copy of the controls that clearly do not submit (narrowing table).
const NOT_SUBMIT: &[&str] = &[
    "forgot",
    "cancel",
    "close",
    "back",
    "dismiss",
    "xmark",
    "clear",
    "delete",
    "emoji",
    "emojis",
    "smiling",
    "sticker",
    "stickers",
    "dictate",
    "dictation",
    "microphone",
    "mic",
    "waveform",
    "attach",
    "attachment",
    "attachments",
    "paperclip",
    "add",
    "plus",
    "photo",
    "photos",
    "camera",
    "keyboard",
    "globe",
    "format",
    "formatting",
    "textformat",
    "bold",
    "italic",
    "underline",
    "strikethrough",
    "expand",
    "collapse",
    "chevron",
    "show",
    "hide",
    "eye",
];
/// The oracle's copy of the login account-field words (narrowing (a)).
const LOGIN_WORDS: &[&str] = &[
    "email", "mail", "username", "user", "login", "id", "account", "phone", "mobile",
];

const BUTTONS: &[&str] = &[
    "Done",
    "OK",
    "Finish",
    "Submit",
    "Next",
    "Sign in",
    "Continue",
    "Post",
    "arrow.right.circle",
    "paperplane.fill",
    "Cancel",
    "Back",
    "Forgot password?",
    "Add photo",
    "Show",
    "Clear",
    "chevron.right",
];

#[derive(Clone)]
struct Field {
    name: String,
    role: Role,
    value: String,
}

#[derive(Clone)]
struct Form {
    fields: Vec<Field>,
    buttons: Vec<String>,
}

fn words(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn gen_form(r: &mut Rng, f: usize) -> Form {
    let n = 1 + r.below(5) as usize;
    let fields = (0..n)
        .map(|i| {
            let (name, role) = match r.below(8) {
                0 => ("Email", Role::TextField),
                1 => ("Username", Role::TextField),
                2 => ("Search", Role::SearchField),
                3 => ("Password", Role::SecureTextField),
                4 => ("Your review", Role::TextView),
                5 => ("Phone", Role::TextField),
                _ => (
                    *r.pick(&["Message", "Comment", "Note", "Subject"]),
                    Role::TextField,
                ),
            };
            Field {
                name: format!("{name} {f}{i}"),
                role,
                value: String::new(),
            }
        })
        .collect();
    let buttons = (0..1 + r.below(4))
        .map(|j| format!("{} {f}{j}", r.pick(BUTTONS)))
        .collect();
    Form { fields, buttons }
}

fn build(forms: &[Form], dy: f64) -> Screen {
    let mut y = 100.0 + dy;
    let mut containers = Vec::new();
    for form in forms {
        let top = y;
        let mut kids = Vec::new();
        for f in &form.fields {
            let h = if f.role == Role::TextView {
                160.0
            } else {
                44.0
            };
            let mut n =
                UiNode::new(f.role.clone(), Rect::new(16.0, y, W - 32.0, h)).with_label(&f.name);
            if !f.value.is_empty() {
                n = n.with_value(&f.value);
            }
            if f.role == Role::SecureTextField {
                n = n.secure(true);
            }
            kids.push(n);
            y += h + 16.0;
        }
        for b in &form.buttons {
            kids.push(node(Role::Button, b, Rect::new(16.0, y, W - 32.0, 50.0)));
            y += 66.0;
        }
        containers.push(
            UiNode::new(Role::Other("Other".into()), Rect::new(0.0, top, W, y - top))
                .with_children(kids),
        );
        y += 40.0;
    }
    let full = Rect::new(0.0, 0.0, W, H);
    let root = UiNode::new(Role::Application, full)
        .with_label("Shop")
        .with_children(vec![UiNode::new(Role::Window, full).with_children(vec![
            UiNode::new(Role::ScrollView, full).with_children(containers),
        ])]);
    screen("com.example.shop", "Shop", root, false)
}

/// What the oracle expects of a tap on button `b` of form `f`.
fn expected_ask(form: &Form, b: &str, typed: &[(String, String)]) -> bool {
    if words(b).iter().any(|w| NOT_SUBMIT.contains(&w.as_str())) {
        return false;
    }
    let secure = form.fields.iter().any(|f| f.role == Role::SecureTextField);
    let login = |name: &str| {
        let n = name.to_lowercase();
        words(&n).iter().any(|w| LOGIN_WORDS.contains(&w.as_str()))
            || n.contains("email")
            || n.contains("username")
    };
    let filled_login_only = form
        .fields
        .iter()
        .filter(|f| f.role != Role::SecureTextField && !f.value.is_empty())
        .all(|f| login(&f.name));
    if secure && filled_login_only {
        return false;
    }
    form.fields.iter().any(|f| {
        f.role != Role::SecureTextField
            && f.role != Role::SearchField
            && !words(&f.name).contains(&"search".to_owned())
            && !f.value.is_empty()
            && typed.iter().any(|(n, t)| *n == f.name && *t == f.value)
    })
}

/// Counts of what the sequences exercised.
#[derive(Default)]
struct Tally {
    taps: usize,
    expected: usize,
    expected_unlisted: usize,
}

fn run(seed: u64, tally: &mut Tally) {
    let r = &mut Rng::new(seed);
    let mut forms: Vec<Form> = (0..1 + r.below(2) as usize)
        .map(|f| gen_form(r, f))
        .collect();
    let mut dy = 0.0;
    // (field name, text) Jarvis typed, and the `TypedText` the session keeps.
    let mut typed: Vec<(String, String)> = Vec::new();
    let mut kept: Vec<TypedText> = Vec::new();
    for step in 0..4 + r.below(10) {
        let s = build(&forms, dy);
        let view = compact_view(&s);
        let f = r.below(forms.len() as u64) as usize;
        match r.below(10) {
            0..=3 => {
                // Jarvis types into a listed, non-secure field.
                let i = r.below(forms[f].fields.len() as u64) as usize;
                let field = forms[f].fields[i].clone();
                if field.role == Role::SecureTextField {
                    continue;
                }
                let Some(e) = view.screen.elements.iter().find(|e| e.label == field.name) else {
                    continue;
                };
                let text = format!("hello {seed} {step}");
                kept.push(TypedText {
                    field: e.fingerprint,
                    text: text.clone(),
                });
                typed.push((field.name.clone(), text.clone()));
                forms[f].fields[i].value = text;
            }
            4 => {
                let i = r.below(forms[f].fields.len() as u64) as usize;
                forms[f].fields[i].value.clear();
            }
            5 => {
                // The owner edits a field (not Jarvis's text any more).
                let i = r.below(forms[f].fields.len() as u64) as usize;
                if forms[f].fields[i].role != Role::SecureTextField {
                    forms[f].fields[i].value = format!("owner {step}");
                }
            }
            6 | 7 => dy = -f64::from(r.below(4) as u32) * 300.0,
            _ => {
                let j = r.below(forms[f].buttons.len() as u64) as usize;
                let b = forms[f].buttons[j].clone();
                let Some(e) = view.screen.elements.iter().find(|e| e.label == b) else {
                    continue;
                };
                tally.taps += 1;
                if !expected_ask(&forms[f], &b, &typed) {
                    continue;
                }
                tally.expected += 1;
                let unlisted = forms[f].fields.iter().any(|fl| {
                    !fl.value.is_empty() && !view.screen.elements.iter().any(|e| e.label == fl.name)
                });
                if unlisted {
                    tally.expected_unlisted += 1;
                }
                let tree = ScreenTree {
                    sources: &view.sources,
                    parents: &view.parents,
                    ends: &view.ends,
                    bars: &view.bars,
                    nodes: &view.nodes,
                    inputs: &view.inputs,
                };
                let d = evaluate_with(
                    &Policy::default(),
                    &Action::Tap { element: e.index },
                    &view.screen,
                    None,
                    ScreenSource::Typed,
                    Some(&tree),
                    &kept,
                );
                let Decision::Confirm(req) = &d else {
                    panic!(
                        "seed {seed} step {step}: a tap on {b:?} after typing is not asked: \
                         {d:?}\n(run alone: SUBMIT_SEED={seed})"
                    );
                };
                assert!(
                    req.summary.contains("Sends:"),
                    "seed {seed}: {}",
                    req.summary
                );
            }
        }
    }
}

#[test]
fn a_tap_after_typing_asks_until_the_text_is_gone() {
    if let Some(seed) = std::env::var("SUBMIT_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        run(seed, &mut Tally::default());
        return;
    }
    let n = std::env::var("SUBMIT_SEQUENCES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SEQUENCES);
    let mut tally = Tally::default();
    for seed in 1..=n {
        run(seed, &mut tally);
    }
    println!(
        "{n} sequences: {} taps, {} expected to ask ({} with a typed field off the list)",
        tally.taps, tally.expected, tally.expected_unlisted
    );
    assert!(tally.expected > 0 && tally.expected_unlisted > 0);
}
