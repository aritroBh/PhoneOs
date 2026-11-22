#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Ninth review, property test 3: a tap after Jarvis typed is presumed to submit.
//!
//! Random sequences of type / clear / owner edit / scroll / tap over random forms (one to
//! three form containers in a page scroller, fields of every kind — message, login, search,
//! secure — and buttons, neutral, icon-named or from the table of controls that clearly do
//! not submit). The oracle is written here (its own copy of the table and of the login
//! words), not with the rule's helpers.
//!
//! **Tenth review: the "Jarvis typed here" state.** The rule no longer re-identifies the
//! field Jarvis typed in (its fingerprint hashes the row's merged label and context, so it
//! changed when a self-drawn placeholder hid or a counter moved). The generator now makes
//! exactly those fields: fields inside cells with a self-drawn placeholder text that
//! disappears once text is in, a counter text that follows the typed length ("12/500"),
//! unlabelled text views, a label lent from the row (a static text beside an unlabelled
//! field), the app reformatting what was typed (trimmed, capitalised, autocorrected),
//! scrolling between typing and tapping, buttons inside cells, and up to three forms.
//! Property, checked at every tap on a listed button: once Jarvis typed anything in this
//! app (the state the session keeps; here: any `type` so far), when the button's form holds
//! a filled, non-search, non-secure text input (whoever filled it, listed or scrolled out
//! of the list), and the button is not in the not-a-submit table and not excluded by
//! narrowing (a) (a login form: a secure field, and only email or username fields filled),
//! the tap asks and its question says `Sends:`. The tally counts how many of those asks the
//! ninth review's rule (fingerprint or exact typed text of a listed or unlisted input)
//! could not have found.
//!
//! **Eleventh review: Return after typing.** Sequences also hold `type … submit` (and a bare
//! Return after earlier typing) into fields of every kind with the keyboard up and a
//! neutral Return key ("go", "done", "return", "next"). Property, checked at every Return:
//! once Jarvis typed in this app (the text being typed counts, and fills the field), when
//! the field is not a search field and not a multi-line text view whose Return key only
//! inserts a line break, and its form holds a filled, non-search, non-secure text input
//! (the field included) outside narrowing (a), Return asks and its question says what is
//! sent. The state is never cleared within a sequence (the session keeps it for the task;
//! `jarvis-agent/tests/typed_state_property.rs` checks the session's side with owner
//! approvals and declines in between).
//!
//! Seeded (xorshift); a failure names its seed (`SUBMIT_SEED=<seed>` runs it alone); the
//! default is 20,000 sequences, `SUBMIT_SEQUENCES=<n>` changes it.

use jarvis_observe::compact_view;
use jarvis_proto::fixtures::{keyboard, node, screen, text};
use jarvis_proto::{Action, Policy, Rect, Role, Screen, UiNode};
use jarvis_safety::{evaluate_with, return_summary, Decision, ScreenSource, ScreenTree, TypedText};

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
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
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

/// How a field is drawn (tenth review).
#[derive(Clone, Copy, PartialEq)]
enum Look {
    /// A labelled field on its own.
    Plain,
    /// A labelled field in a cell.
    Cell,
    /// An unlabelled field in a cell with a self-drawn placeholder text (hidden once
    /// filled) and, maybe, a counter text that follows the length.
    Placeholder { counter: bool },
    /// An unlabelled field in a cell whose name is a static text beside it.
    LentLabel,
}

#[derive(Clone)]
struct Field {
    /// The field's name (its label, or for unlabelled fields the text that shows it).
    name: String,
    role: Role,
    look: Look,
    value: String,
}

impl Field {
    fn labelled(&self) -> bool {
        matches!(self.look, Look::Plain | Look::Cell)
    }
}

#[derive(Clone)]
struct Form {
    fields: Vec<Field>,
    buttons: Vec<String>,
    /// Buttons sit in cells.
    button_cells: bool,
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
            let (name, role, free) = match r.below(8) {
                0 => ("Email", Role::TextField, false),
                1 => ("Username", Role::TextField, false),
                2 => ("Search", Role::SearchField, false),
                3 => ("Password", Role::SecureTextField, false),
                4 => ("Your review", Role::TextView, true),
                5 => ("Phone", Role::TextField, false),
                _ => (
                    *r.pick(&["Message", "Comment", "Note", "Subject"]),
                    Role::TextField,
                    true,
                ),
            };
            // Message-like fields may be drawn the ways that changed their fingerprint
            // (login, search and secure fields stay labelled: the narrowings read names).
            let look = if !free {
                if r.chance(30) {
                    Look::Cell
                } else {
                    Look::Plain
                }
            } else {
                match r.below(5) {
                    0 => Look::Plain,
                    1 => Look::Cell,
                    2 | 3 => Look::Placeholder {
                        counter: r.chance(50),
                    },
                    _ => Look::LentLabel,
                }
            };
            let role = if free && r.chance(40) {
                Role::TextView
            } else {
                role
            };
            Field {
                name: format!("{name} {f}{i}"),
                role,
                look,
                value: String::new(),
            }
        })
        .collect();
    let buttons = (0..1 + r.below(4))
        .map(|j| format!("{} {f}{j}", r.pick(BUTTONS)))
        .collect();
    Form {
        fields,
        buttons,
        button_cells: r.chance(30),
    }
}

/// For every field (form, index), the frame of its input node.
type FieldFrames = Vec<((usize, usize), Rect)>;

/// The screen, and its fields' frames.
fn build(forms: &[Form], dy: f64) -> (Screen, FieldFrames) {
    build_with(forms, dy, None)
}

/// The screen with the keyboard up (its Return key labelled `key`) and field `focus`
/// focused, when `kb` is given (eleventh review).
fn build_with(
    forms: &[Form],
    dy: f64,
    kb: Option<(&str, (usize, usize))>,
) -> (Screen, FieldFrames) {
    let mut y = 100.0 + dy;
    let mut containers = Vec::new();
    let mut frames = Vec::new();
    for (fi, form) in forms.iter().enumerate() {
        let top = y;
        let mut kids = Vec::new();
        for (i, f) in form.fields.iter().enumerate() {
            let h = if f.role == Role::TextView {
                160.0
            } else {
                44.0
            };
            let frame = Rect::new(16.0, y, W - 32.0, h);
            frames.push(((fi, i), frame));
            let mut n = UiNode::new(f.role.clone(), frame);
            if kb.is_some_and(|(_, at)| at == (fi, i)) {
                n = n.focused(true);
            }
            if f.labelled() {
                n = n.with_label(&f.name);
            }
            if !f.value.is_empty() {
                n = n.with_value(&f.value);
            }
            if f.role == Role::SecureTextField {
                n = n.secure(true);
            }
            let cell = Rect::new(0.0, y - 6.0, W, h + 12.0);
            match f.look {
                Look::Plain => kids.push(n),
                Look::Cell => kids.push(UiNode::new(Role::Cell, cell).with_children(vec![n])),
                Look::Placeholder { counter } => {
                    let mut row = vec![n];
                    if f.value.is_empty() {
                        row.push(text(&f.name, Rect::new(24.0, y + 8.0, 300.0, 20.0)));
                    }
                    if counter {
                        row.push(text(
                            &format!("{}/500", f.value.chars().count()),
                            Rect::new(W - 80.0, y + h - 22.0, 60.0, 18.0),
                        ));
                    }
                    kids.push(UiNode::new(Role::Cell, cell).with_children(row));
                }
                Look::LentLabel => {
                    let label = text(&f.name, Rect::new(16.0, y, 100.0, 20.0));
                    let mut field = UiNode::new(f.role.clone(), Rect::new(120.0, y, W - 136.0, h));
                    if kb.is_some_and(|(_, at)| at == (fi, i)) {
                        field = field.focused(true);
                    }
                    let field = if f.value.is_empty() {
                        field
                    } else {
                        field.with_value(&f.value)
                    };
                    frames.pop();
                    frames.push(((fi, i), Rect::new(120.0, y, W - 136.0, h)));
                    kids.push(UiNode::new(Role::Cell, cell).with_children(vec![label, field]));
                }
            }
            y += h + 16.0;
        }
        for b in &form.buttons {
            let button = node(Role::Button, b, Rect::new(16.0, y, W - 32.0, 50.0));
            if form.button_cells {
                kids.push(
                    UiNode::new(Role::Cell, Rect::new(0.0, y - 4.0, W, 58.0))
                        .with_children(vec![button]),
                );
            } else {
                kids.push(button);
            }
            y += 66.0;
        }
        containers.push(
            UiNode::new(Role::Other("Other".into()), Rect::new(0.0, top, W, y - top))
                .with_children(kids),
        );
        y += 40.0;
    }
    let full = Rect::new(0.0, 0.0, W, H);
    let mut window = vec![UiNode::new(Role::ScrollView, full).with_children(containers)];
    if let Some((key, _)) = kb {
        window.push(keyboard(key));
    }
    let root = UiNode::new(Role::Application, full)
        .with_label("Shop")
        .with_children(vec![UiNode::new(Role::Window, full).with_children(window)]);
    (
        screen("com.example.shop", "Shop", root, kb.is_some()),
        frames,
    )
}

/// Return keys of a multi-line text view that submit (the oracle's copy).
const SUBMIT_KEYS: &[&str] = &[
    "send", "go", "done", "join", "search", "continue", "route", "submit", "post", "reply",
];

/// What the oracle expects of Return in field `i` of form `f` (eleventh review). The form
/// of a field is the nearest container holding another text input: its own form when that
/// has one, else (a form of one field) the whole page.
fn expected_return(forms: &[Form], f: usize, i: usize, key: &str) -> bool {
    let own = &forms[f];
    let page;
    let form = if own.fields.len() > 1 {
        own
    } else {
        page = Form {
            fields: forms.iter().flat_map(|g| g.fields.clone()).collect(),
            buttons: Vec::new(),
            button_cells: false,
        };
        &page
    };
    let field = &own.fields[i];
    if field.role == Role::SearchField || words(&field.name).contains(&"search".to_owned()) {
        return false;
    }
    if field.role == Role::TextView && !SUBMIT_KEYS.contains(&key) {
        return false;
    }
    let secure = form.fields.iter().any(|f| f.role == Role::SecureTextField);
    let login = |f: &Field| {
        let n = f.name.to_lowercase();
        f.labelled()
            && (words(&n).iter().any(|w| LOGIN_WORDS.contains(&w.as_str()))
                || n.contains("email")
                || n.contains("username"))
    };
    let filled_login_only = form
        .fields
        .iter()
        .filter(|f| f.role != Role::SecureTextField && !f.value.is_empty())
        .all(login);
    if secure && filled_login_only {
        return false;
    }
    form.fields.iter().any(|f| {
        f.role != Role::SecureTextField
            && f.role != Role::SearchField
            && !words(&f.name).contains(&"search".to_owned())
            && !f.value.is_empty()
    })
}

/// What the app makes of the typed text (tenth review): kept, trimmed, capitalised, or
/// "autocorrected".
fn reformat(r: &mut Rng, typed: &str) -> String {
    match r.below(4) {
        0 => typed.to_owned(),
        1 => typed.trim().to_owned(),
        2 => {
            let mut c = typed.trim_start().chars();
            match c.next() {
                Some(f) => f.to_uppercase().chain(c).collect(),
                None => String::new(),
            }
        }
        _ => typed.replace("helo", "hello").replace("teh", "the"),
    }
}

/// What the oracle expects of a tap on button `b` of form `f` once Jarvis typed here.
fn expected_ask(form: &Form, b: &str) -> bool {
    if words(b).iter().any(|w| NOT_SUBMIT.contains(&w.as_str())) {
        return false;
    }
    let secure = form.fields.iter().any(|f| f.role == Role::SecureTextField);
    let login = |f: &Field| {
        let n = f.name.to_lowercase();
        f.labelled()
            && (words(&n).iter().any(|w| LOGIN_WORDS.contains(&w.as_str()))
                || n.contains("email")
                || n.contains("username"))
    };
    let filled_login_only = form
        .fields
        .iter()
        .filter(|f| f.role != Role::SecureTextField && !f.value.is_empty())
        .all(login);
    if secure && filled_login_only {
        return false;
    }
    form.fields.iter().any(|f| {
        f.role != Role::SecureTextField
            && f.role != Role::SearchField
            && !words(&f.name).contains(&"search".to_owned())
            && !f.value.is_empty()
    })
}

/// Counts of what the sequences exercised.
#[derive(Default)]
struct Tally {
    taps: usize,
    expected: usize,
    expected_unlisted: usize,
    /// Expected asks the ninth review's rule could not have found (no filled input of the
    /// form carries a kept fingerprint or exactly a kept text).
    beyond_ninth: usize,
    owner_filled_only: usize,
    returns: usize,
    returns_expected: usize,
    returns_bare: usize,
}

fn run(seed: u64, tally: &mut Tally) {
    let r = &mut Rng::new(seed);
    let mut forms: Vec<Form> = (0..1 + r.below(3) as usize)
        .map(|f| gen_form(r, f))
        .collect();
    let mut dy = 0.0;
    // The `TypedText` the session keeps; non-empty is the "typed here" state.
    let mut kept: Vec<TypedText> = Vec::new();
    // Which fields Jarvis typed into (form, field).
    let mut jarvis: Vec<(usize, usize)> = Vec::new();
    for step in 0..4 + r.below(10) {
        let (s, frames) = build(&forms, dy);
        let view = compact_view(&s);
        let f = r.below(forms.len() as u64) as usize;
        match r.below(11) {
            0..=3 => {
                // Jarvis types into a listed, non-secure field (found by its frame, as the
                // session finds its target by index).
                let i = r.below(forms[f].fields.len() as u64) as usize;
                let field = forms[f].fields[i].clone();
                if field.role == Role::SecureTextField {
                    continue;
                }
                let Some(frame) = frames.iter().find(|(k, _)| *k == (f, i)).map(|(_, r)| *r) else {
                    continue;
                };
                let c = frame.center();
                let Some(e) = view
                    .screen
                    .elements
                    .iter()
                    .find(|e| e.role.is_text_input() && e.frame.contains(c))
                else {
                    continue;
                };
                let text = format!(
                    "{}helo {seed} teh {step}{}",
                    if r.chance(30) { "  " } else { "" },
                    if r.chance(30) { " " } else { "" }
                );
                kept.push(TypedText {
                    field: e.fingerprint,
                    text: text.clone(),
                });
                jarvis.push((f, i));
                forms[f].fields[i].value = reformat(r, &text);
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
            10 => {
                // Eleventh review: `type … submit` (or a bare Return after earlier typing)
                // with a neutral Return key; the Return part is gated on the screen after
                // the typing, with the keyboard up and the field focused.
                let i = r.below(forms[f].fields.len() as u64) as usize;
                if forms[f].fields[i].role == Role::SecureTextField {
                    continue;
                }
                let bare = !kept.is_empty() && r.chance(30);
                let key = *r.pick(&["go", "done", "return", "next"]);
                let text = if bare {
                    String::new()
                } else {
                    format!("helo {seed} ret {step}")
                };
                if !bare {
                    forms[f].fields[i].value = reformat(r, &text);
                }
                let (s, frames) = build_with(&forms, dy, Some((key, (f, i))));
                let view = compact_view(&s);
                let Some(frame) = frames.iter().find(|(k, _)| *k == (f, i)).map(|(_, r)| *r) else {
                    continue;
                };
                let c = frame.center();
                let Some(e) = view
                    .screen
                    .elements
                    .iter()
                    .find(|e| e.role.is_text_input() && e.frame.contains(c))
                else {
                    continue;
                };
                if !bare {
                    kept.push(TypedText {
                        field: e.fingerprint,
                        text: text.clone(),
                    });
                    jarvis.push((f, i));
                }
                tally.returns += 1;
                if !expected_return(&forms, f, i, key) {
                    continue;
                }
                tally.returns_expected += 1;
                if bare {
                    tally.returns_bare += 1;
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
                    &Action::Type {
                        element: Some(e.index),
                        text: text.clone(),
                        submit: true,
                    },
                    &view.screen,
                    None,
                    ScreenSource::Typed,
                    Some(&tree),
                    &kept,
                );
                let Decision::Confirm(req) = &d else {
                    panic!(
                        "seed {seed} step {step}: Return ({key:?}) in {:?} after typing is \
                         not asked: {d:?}\n(run alone: SUBMIT_SEED={seed})",
                        forms[f].fields[i].name
                    );
                };
                // The session asks Return's own question (`return_summary`).
                let q = return_summary(
                    e.index,
                    &text,
                    &view.screen,
                    req.category,
                    ScreenSource::Typed,
                    Some(&tree),
                    &kept,
                );
                assert!(
                    q.contains("Sends:") || q.contains("which sends the field's whole content"),
                    "seed {seed}: {q}"
                );
            }
            _ => {
                let j = r.below(forms[f].buttons.len() as u64) as usize;
                let b = forms[f].buttons[j].clone();
                let Some(e) = view.screen.elements.iter().find(|e| e.label == b) else {
                    continue;
                };
                tally.taps += 1;
                if kept.is_empty() || !expected_ask(&forms[f], &b) {
                    continue;
                }
                tally.expected += 1;
                let unlisted = forms[f].fields.iter().zip(0..).any(|(fl, i)| {
                    !fl.value.is_empty()
                        && frames
                            .iter()
                            .find(|(k, _)| *k == (f, i))
                            .is_some_and(|(_, fr)| {
                                !view.screen.elements.iter().any(|e| {
                                    e.role.is_text_input() && e.frame.contains(fr.center())
                                })
                            })
                });
                if unlisted {
                    tally.expected_unlisted += 1;
                }
                // Could the ninth review's rule have found a typed field here?
                let ninth = view.inputs.iter().any(|ri| {
                    ri.value.as_deref().is_some_and(|v| {
                        !v.trim().is_empty()
                            && kept.iter().any(|t| {
                                ri.fingerprint == Some(t.field) || t.text.trim() == v.trim()
                            })
                    })
                });
                if !ninth {
                    tally.beyond_ninth += 1;
                }
                if !forms[f]
                    .fields
                    .iter()
                    .zip(0..)
                    .any(|(fl, i)| !fl.value.is_empty() && jarvis.contains(&(f, i)))
                {
                    tally.owner_filled_only += 1;
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
fn a_tap_after_typing_asks_while_its_form_holds_text() {
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
        "{n} sequences: {} taps, {} expected to ask ({} with a filled field off the list, {} \
         the ninth review's rule could not find, {} with only text Jarvis did not type); {} \
         Returns, {} expected to ask ({} bare)",
        tally.taps,
        tally.expected,
        tally.expected_unlisted,
        tally.beyond_ninth,
        tally.owner_filled_only,
        tally.returns,
        tally.returns_expected,
        tally.returns_bare
    );
    assert!(tally.expected > 0 && tally.expected_unlisted > 0 && tally.beyond_ninth > 0);
    assert!(tally.returns_expected > 0 && tally.returns_bare > 0);
}
