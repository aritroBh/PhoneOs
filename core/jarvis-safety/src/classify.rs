//! Risk classification: what could go wrong if an action ran unattended.

use std::cell::OnceCell;
use std::collections::BTreeSet;

use jarvis_proto::{
    Action, CompactScreen, Direction, Element, Point, RiskCategory, Role, Tier, ToolSpec,
};

use crate::gestures::{finger_direction, gesture, GestureKind};
use crate::phrases::{
    action_tables, in_app, match_label, match_tokens, match_whole, max_risk, severity,
    sf_symbol_risk, LabelRisk, Phrase, RuleTarget, ACTION_PHRASES, APP_RULES,
    COMPOSE_FIELD_PHRASES, CONTEXT_PHRASES, CREDENTIAL_FIELD_PHRASES, NAVIGATION_PHRASES,
    PRICE_VERB_PHRASES, SETTINGS_ACTION_PHRASES, SETTINGS_BUNDLE_ID, SETTINGS_CONTEXT_PHRASES,
    SHARE_SAFE_PHRASES,
};
use crate::redact::contains_secret;
use crate::text::{
    bounded, has_foreign_word, has_money, has_non_english_letters, label_is_unreadable, tokens,
    CURRENCY_WORDS,
};
use crate::validate::{looks_like_url, url_scheme};
use jarvis_proto::SPRINGBOARD_BUNDLE_ID;

/// Most controls whose own risk is worked out for one gesture or `tap_point` (each such
/// evaluation reads the whole screen as context). The elements are filtered to the ones
/// that matter (controls that react to the drag, pressable elements under the point)
/// *before* this cap applies, so padding with text cannot push a control out of view;
/// when more relevant controls than this remain, the result is at least
/// [`UNREADABLE_RISK`], so the owner is asked.
pub(crate) const MAX_SCANNED_CONTROLS: usize = 32;

/// Category for a tap the phrase net cannot read (unknown script or language, oversized
/// label, unknown symbol name, no label at all) and for a tool call without a spec. It
/// ranks below every definite signal, so a known trash symbol or a checkout screen still
/// wins.
pub(crate) const UNREADABLE_RISK: RiskCategory = RiskCategory::Unverified;

/// Most string arguments of one tool call that are read.
const MAX_TOOL_STRINGS: usize = 64;

/// Classifies `action` on `screen`.
///
/// * `CallTool` uses the tool's declared risk, raised if the tool's *name* reads as risky
///   (`delete_event`). Shortcut-tier tools (or tools whose name or arguments mention a
///   shortcut) also have their string arguments read (`run_shortcut {name: "Delete All
///   Photos"}` is `Delete`); URL arguments are classified like `OpenUrl`. Without a spec
///   the call is `Unverified` at least (and `evaluate` denies it).
/// * Taps and long presses read the target's label (and value, for non-inputs) through the
///   phrase tables, with app-specific rows (`APP_PHRASES`) and rules (`APP_RULES`: Phone
///   and FaceTime rows and phone numbers are `Call`, Home tiles operate accessories,
///   Settings/Control Center switches). Generic
///   confirmations ("OK", "Continue"), unlabeled buttons and unknown SF Symbol names take
///   on the riskiest signal elsewhere on the screen (prices, "Delete 3 photos?", a Send
///   button). A price in a button label is at least a `Purchase` (a list or product row
///   that only names an item and its price is not: it opens the item); "Book"/"Order"/"Get"
///   with a price on screen is a `Purchase`. On a checkout screen (a price plus payment
///   wording such as "Total" or "Card ending"), every pressable element that is not
///   plainly navigation is a `Payment`. Rows of a share sheet (a screen with AirDrop) send.
///   "Save" on an account or settings screen is `AccountChange`. A pressable element (not a
///   tab or key) that is unlabeled, named by an unknown SF Symbol, or whose label the
///   tables cannot read (see `label_is_unreadable` and `label_is_foreign`) is at least
///   `Unverified`.
/// * `Type` is `Credential` for secure fields, credential-labelled fields or secret-looking
///   text; with `submit` it also takes the compose field's, the keyboard Return key's and
///   the screen's risk, so Return in a message field next to a Send button is
///   `SendMessage`, and a typed `tel:` URL is classified like `OpenUrl`.
/// * `TapPoint` takes the riskiest pressable element under the point, or the screen
///   context when only text or nothing is there.
/// * Swipes and scrolls are classified by the drag the runner will really make (the same
///   path `gestures::gesture` computes for the command). A swipe is read like a tap of its
///   element (with context); a sideways swipe on a cell or button row is at least `Delete`
///   (full swipes trigger the destructive action). A scroll is the same finger movement in
///   the opposite direction, so a sideways scroll, or any scroll whose element is itself an
///   adjustable control (slider, switch, picker, stepper), is classified exactly like the
///   swipe it becomes. Every gesture, with or without an element, also takes the risk of
///   the enabled controls its path crosses that react to such a drag (see
///   [`reacts_to_drag`]): a sideways drag across a "Slide to pay" slider on a checkout is
///   `Payment`. A plain vertical scroll of a list, or a swipe across empty space or plain
///   text, stays `None`.
/// * `OpenUrl` to `tel:`/`facetime:` is `Call`.
/// * Everything else is `None`. An element index that does not exist falls back to the
///   screen context (and is rejected by `validate_action` anyway).
pub fn classify(action: &Action, screen: &CompactScreen, tool: Option<&ToolSpec>) -> RiskCategory {
    let r = Reader::new(screen);
    match action {
        Action::Tap { element } | Action::LongPress { element, .. } => match screen.get(*element) {
            Some(el) => element_risk(el, &r),
            None => r.context(None).risk,
        },
        Action::Type {
            element,
            text,
            submit,
        } => type_risk(*element, text, *submit, &r),
        Action::Swipe { direction, element } => {
            gesture_risk(GestureKind::Swipe, *direction, *element, &r)
        }
        Action::Scroll { direction, element } => {
            gesture_risk(GestureKind::Scroll, *direction, *element, &r)
        }
        Action::TapPoint { x, y } => tap_point_in(*x, *y, &r).0,
        Action::OpenUrl { url } => url_risk(url),
        Action::CallTool { name, args } => tool_risk(name, args, tool),
        Action::Clear { .. }
        | Action::PressHome
        | Action::OpenApp { .. }
        | Action::Wait { .. }
        | Action::AskUser { .. }
        | Action::Look
        | Action::Done { .. }
        | Action::Fail { .. } => RiskCategory::None,
    }
}

/// Risk of opening `url`: calls are `Call`, everything else `None` (schemes are policed
/// by `validate_action`).
pub(crate) fn url_risk(url: &str) -> RiskCategory {
    match url_scheme(url.trim()).as_deref() {
        Some("tel" | "telprompt" | "facetime" | "facetime-audio") => RiskCategory::Call,
        _ => RiskCategory::None,
    }
}

/// Collects up to [`MAX_TOOL_STRINGS`] string values from a JSON value, depth-first.
pub(crate) fn tool_strings(v: &serde_json::Value, out: &mut Vec<String>) {
    if out.len() >= MAX_TOOL_STRINGS {
        return;
    }
    match v {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Array(items) => items.iter().for_each(|i| tool_strings(i, out)),
        serde_json::Value::Object(map) => map.values().for_each(|i| tool_strings(i, out)),
        _ => {}
    }
}

/// Whether `text` mentions a shortcut (`run_shortcut`, "Shortcuts").
fn mentions_shortcut(text: &str) -> bool {
    tokens(&bounded(text))
        .iter()
        .any(|t| t == "shortcut" || t == "shortcuts")
}

/// Risk of calling a tool.
fn tool_risk(name: &str, args: &serde_json::Value, tool: Option<&ToolSpec>) -> RiskCategory {
    let mut strings = Vec::new();
    tool_strings(args, &mut strings);
    let mut risk = match_label(name, &[ACTION_PHRASES]).category;
    let shortcut = tool.is_some_and(|t| t.tier == Tier::Shortcut)
        || mentions_shortcut(name)
        || strings.iter().any(|s| mentions_shortcut(s));
    for s in &strings {
        if shortcut {
            risk = max_risk(risk, match_label(s, &[ACTION_PHRASES]).category);
        }
        if looks_like_url(s) {
            risk = max_risk(risk, url_risk(s));
        }
    }
    match tool {
        Some(spec) => max_risk(spec.risk, risk),
        None => max_risk(risk, UNREADABLE_RISK),
    }
}

/// The text an element shows, bounded: its label, plus its value unless it is a text input
/// (whose value is user content, not a description of what the element does).
pub(crate) fn element_text(el: &Element) -> String {
    match (&el.value, el.role.is_text_input()) {
        (Some(v), false) if !v.is_empty() => format!("{} {}", bounded(&el.label), bounded(v)),
        _ => bounded(&el.label).into_owned(),
    }
}

/// Whether the element is something that is pressed rather than read or typed into.
pub(crate) fn is_pressable(role: &Role) -> bool {
    !role.is_text_input() && !matches!(role, Role::StaticText | Role::Image)
}

/// Whether any phrase of `table` matches `toks` (even a `None` row).
fn matches_any(toks: &[String], table: &[Phrase]) -> bool {
    table.iter().any(|p| {
        let words = tokens(p.text);
        !words.is_empty() && toks.windows(words.len()).any(|w| w == words.as_slice())
    })
}

/// True when the text contains a phone-number-like run: 7–15 digits joined by spaces,
/// dashes, dots, parentheses or a leading `+`.
fn has_phone_number(text: &str) -> bool {
    let mut digits = 0usize;
    for c in text.chars().chain(std::iter::once('x')) {
        if c.is_ascii_digit() {
            digits += 1;
        } else if matches!(c, ' ' | '-' | '.' | '(' | ')' | '+' | '\u{a0}') {
            continue;
        } else {
            if (7..=15).contains(&digits) {
                return true;
            }
            digits = 0;
        }
    }
    false
}

/// What the rest of the screen says, gathered in one pass.
#[derive(Debug, Default)]
pub(crate) struct Context {
    /// The riskiest signal: risky labels, explanatory text, prices (as `Payment`).
    pub risk: RiskCategory,
    /// A price is shown.
    pub money: bool,
    /// Payment wording is shown ("Total", "Card ending", a Pay button).
    pub payment_words: bool,
    /// Purchase wording is shown ("In-App Purchases", "Subscription").
    pub purchase_words: bool,
    /// The screen is a share sheet (it offers AirDrop).
    pub share_sheet: bool,
    /// The screen reads like an account or settings page.
    pub settings_words: bool,
}

/// What one element contributes to the [`Context`] of a decision.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Signal {
    /// Its phrase-table category (an amount reads as `Payment`).
    pub risk: RiskCategory,
    /// It shows a price.
    pub money: bool,
    /// Payment wording.
    pub payment_words: bool,
    /// Purchase wording.
    pub purchase_words: bool,
    /// It offers AirDrop (share sheet).
    pub share_sheet: bool,
    /// Account or settings wording.
    pub settings_words: bool,
}

/// The tables context is read with on `screen`.
fn context_tables(screen: &CompactScreen) -> Vec<&'static [Phrase]> {
    let mut tables: Vec<&[Phrase]> = action_tables(&screen.app_bundle_id);
    tables.push(CONTEXT_PHRASES);
    tables
}

/// What `el` contributes to the context of a decision (nothing for keyboard keys).
fn signal_of(el: &Element, tables: &[&[Phrase]]) -> Signal {
    if el.role == Role::Key {
        return Signal::default();
    }
    let text = element_text(el);
    let toks = tokens(&text);
    let cat = match_tokens(&toks, tables).category;
    let money = has_money(&text);
    Signal {
        risk: if money {
            max_risk(cat, RiskCategory::Payment)
        } else {
            cat
        },
        money,
        payment_words: cat == RiskCategory::Payment,
        purchase_words: matches!(cat, RiskCategory::Purchase | RiskCategory::Payment),
        share_sheet: toks.iter().any(|t| t == "airdrop")
            || toks.windows(2).any(|w| w == ["air", "drop"]),
        settings_words: match_tokens(&toks, &[SETTINGS_CONTEXT_PHRASES]).category
            != RiskCategory::None,
    }
}

/// Most risk levels (see `severity`): one counter each.
const RISK_LEVELS: usize = 16;

/// Every element's [`Signal`] on a screen, counted, so the context of a decision "apart
/// from element N" is the whole count minus that one element's signal: one pass over the
/// screen per classification, however many controls are evaluated.
#[derive(Debug, Default)]
struct Tally {
    risks: [u32; RISK_LEVELS],
    money: u32,
    payment_words: u32,
    purchase_words: u32,
    share_sheet: u32,
    settings_words: u32,
}

fn level(c: RiskCategory) -> usize {
    usize::from(severity(c)).min(RISK_LEVELS - 1)
}

impl Tally {
    fn add(&mut self, s: Signal) {
        if let Some(n) = self.risks.get_mut(level(s.risk)) {
            *n = n.saturating_add(1);
        }
        self.money = self.money.saturating_add(u32::from(s.money));
        self.payment_words = self
            .payment_words
            .saturating_add(u32::from(s.payment_words));
        self.purchase_words = self
            .purchase_words
            .saturating_add(u32::from(s.purchase_words));
        self.share_sheet = self.share_sheet.saturating_add(u32::from(s.share_sheet));
        self.settings_words = self
            .settings_words
            .saturating_add(u32::from(s.settings_words));
    }

    /// The context these counts give once `minus` (one element's signal) is taken out.
    fn without(&self, minus: Signal) -> Context {
        let left = |n: u32, flag: bool| n.saturating_sub(u32::from(flag)) > 0;
        let out_level = level(minus.risk);
        let risk = (0..RISK_LEVELS)
            .rev()
            .find(|&l| {
                let n = self.risks.get(l).copied().unwrap_or(0);
                left(n, l == out_level)
            })
            .and_then(|l| RiskCategory::ALL.iter().copied().find(|c| level(*c) == l))
            .unwrap_or(RiskCategory::None);
        Context {
            risk,
            money: left(self.money, minus.money),
            payment_words: left(self.payment_words, minus.payment_words),
            purchase_words: left(self.purchase_words, minus.purchase_words),
            share_sheet: left(self.share_sheet, minus.share_sheet),
            settings_words: left(self.settings_words, minus.settings_words),
        }
    }
}

/// One classification's view of a screen: the context tables, and what is costly to work
/// out and needed by many controls (the screen's [`Tally`], the cells and buttons that sit
/// beside a sibling), computed at most once.
pub(crate) struct Reader<'a> {
    pub screen: &'a CompactScreen,
    tables: Vec<&'static [Phrase]>,
    tally: OnceCell<Tally>,
    beside: OnceCell<BTreeSet<u32>>,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(screen: &'a CompactScreen) -> Self {
        Reader {
            screen,
            tables: context_tables(screen),
            tally: OnceCell::new(),
            beside: OnceCell::new(),
        }
    }

    /// Reads the screen apart from element `exclude`. Keyboard keys are ignored. Every
    /// element is read: the context only ever grows (the riskiest signal, flags that are
    /// set), so there is no window a page could push a price or a warning out of with
    /// padding. The whole screen is read once per [`Reader`]; excluding an element takes
    /// its own signal out of the counts.
    pub(crate) fn context(&self, exclude: Option<u32>) -> Context {
        let tally = self.tally.get_or_init(|| {
            let mut t = Tally::default();
            for el in &self.screen.elements {
                t.add(signal_of(el, &self.tables));
            }
            t
        });
        // Every element with that index (indices are unique on a compacted screen).
        let mut minus = Signal::default();
        let mut excluded = 0usize;
        for el in self
            .screen
            .elements
            .iter()
            .filter(|e| Some(e.index) == exclude)
        {
            minus = signal_of(el, &self.tables);
            excluded += 1;
        }
        if excluded > 1 {
            // Duplicate indices (a hand-built screen): read it the slow way.
            let mut t = Tally::default();
            for el in self
                .screen
                .elements
                .iter()
                .filter(|e| Some(e.index) != exclude)
            {
                t.add(signal_of(el, &self.tables));
            }
            return t.without(Signal::default());
        }
        tally.without(minus)
    }

    /// Whether `el` (a cell or button) has a sibling beside it (see [`is_list_row`]).
    fn is_beside_sibling(&self, el: &Element) -> bool {
        self.beside
            .get_or_init(|| beside_siblings(self.screen))
            .contains(&el.index)
    }
}

/// The label risk of `el`'s own text (SF Symbol names read through their table).
/// Most words a free-text row (cell, text, image) may have for its whole label to be read
/// as a control name.
pub(crate) const MAX_ROW_LABEL_WORDS: usize = 3;

/// Whether the role usually carries content (message previews, notes, search results)
/// rather than a control name.
fn is_free_text_role(role: &Role) -> bool {
    matches!(role, Role::Cell | Role::StaticText | Role::Image)
}

/// For a long free-text row, the tokens of its first comma-separated segment when that
/// whole segment is an action phrase ("Delete Account", "Erase All Content and
/// Settings"); `None` when the row is ordinary content.
fn row_command_tokens(el: &Element, screen: &CompactScreen) -> Option<Vec<String>> {
    let label = bounded(&el.label);
    let first = label.split(',').next().unwrap_or("");
    let seg = tokens(first);
    let lr = match_whole(&seg, &action_tables(&screen.app_bundle_id));
    (lr.category != RiskCategory::None || lr.generic).then_some(seg)
}

fn own_label_risk(el: &Element, toks: &[String], screen: &CompactScreen) -> (LabelRisk, bool) {
    if let Some(cat) = sf_symbol_risk(&el.label) {
        // A symbol name: its meaning, or "unlabeled" when unknown.
        let generic = cat == RiskCategory::None;
        return (
            LabelRisk {
                category: cat,
                generic,
            },
            generic,
        );
    }
    let lr = match_tokens(toks, &action_tables(&screen.app_bundle_id));
    (lr, toks.is_empty())
}

/// Whether `el` is a list or product row that shows a price next to the item's name
/// ("Desk Lamp, $35.50"): tapping it opens the item, it does not buy it. Only a `Cell`
/// qualifies, and only when its own words are neither an action phrase (Buy, Pay, Order,
/// Checkout... or a generic OK/Continue) nor a price verb (Get, Book, Rent...), and when
/// it names something besides the amount: a row that is only a price ("$0.99", "USD 5")
/// still asks. Buttons and links with a price, checkout screens (see `element_risk`) and
/// rows whose first segment is a command are unaffected.
fn is_priced_item_row(el: &Element, toks: &[String], lr: LabelRisk) -> bool {
    let names_something = toks
        .iter()
        .any(|t| !t.chars().all(|c| c.is_ascii_digit()) && !CURRENCY_WORDS.contains(&t.as_str()));
    el.role == Role::Cell
        && lr.category == RiskCategory::None
        && !lr.generic
        && !matches_any(toks, PRICE_VERB_PHRASES)
        && names_something
}

/// Risk of tapping (or long-pressing) `el`.
pub(crate) fn element_risk(el: &Element, r: &Reader<'_>) -> RiskCategory {
    let screen = r.screen;
    if el.role.is_text_input() {
        // Tapping a field only focuses it.
        return RiskCategory::None;
    }
    let pressable = is_pressable(&el.role);
    let text = element_text(el);
    let all_toks = tokens(&text);
    // A long free-text row is content: its words only count when its first segment is a
    // command in its own right. App rules (keyed by app, not words) still apply.
    let content_row = is_free_text_role(&el.role) && all_toks.len() > MAX_ROW_LABEL_WORDS;
    let toks = if content_row {
        row_command_tokens(el, screen).unwrap_or_default()
    } else {
        all_toks
    };
    let (lr, mut no_label) = own_label_risk(el, &toks, screen);
    if content_row {
        no_label = false;
    }
    let mut risk = lr.category;
    if el.role == Role::Key && risk == RiskCategory::Delete {
        // The keyboard's backspace key is labelled "delete".
        risk = RiskCategory::None;
    }
    if has_money(&text) && !content_row && !is_priced_item_row(el, &toks, lr) {
        risk = match risk {
            RiskCategory::SendMessage | RiskCategory::Payment => RiskCategory::Payment,
            other => max_risk(other, RiskCategory::Purchase),
        };
    }
    let navigation = matches_any(&toks, NAVIGATION_PHRASES);
    let plain_target = pressable && !matches!(el.role, Role::Tab | Role::Key) && !navigation;
    for rule in APP_RULES
        .iter()
        .filter(|r| in_app(&screen.app_bundle_id, r.bundle_prefix))
    {
        let applies = match rule.target {
            RuleTarget::AnyPressable => plain_target,
            RuleTarget::Cell => el.role == Role::Cell,
            RuleTarget::Switch => el.role == Role::Switch,
            RuleTarget::PhoneNumber => pressable && has_phone_number(&text),
        };
        if applies {
            risk = max_risk(risk, rule.category);
        }
    }
    let unlabeled = no_label && pressable;
    let needs_context = lr.generic
        || unlabeled
        || plain_target
        || matches_any(&toks, SETTINGS_ACTION_PHRASES)
        || matches_any(&toks, PRICE_VERB_PHRASES);
    if needs_context {
        let ctx = r.context(Some(el.index));
        if lr.generic || unlabeled {
            risk = max_risk(risk, ctx.risk);
        }
        let settings = ctx.settings_words || in_app(&screen.app_bundle_id, SETTINGS_BUNDLE_ID);
        if settings && matches_any(&toks, SETTINGS_ACTION_PHRASES) {
            risk = max_risk(risk, RiskCategory::AccountChange);
        }
        if (ctx.money || ctx.purchase_words)
            && !content_row
            && matches_any(&toks, PRICE_VERB_PHRASES)
        {
            risk = max_risk(risk, RiskCategory::Purchase);
        }
        if plain_target && ctx.share_sheet && !matches_any(&toks, SHARE_SAFE_PHRASES) {
            risk = max_risk(risk, RiskCategory::SendMessage);
        }
        // On a checkout screen, any button that is not plainly navigation may be the one
        // that pays ("Order", "Let's go", "Swipe up", "Place Order"), whatever its wording.
        if plain_target && !content_row && ctx.money && ctx.payment_words {
            risk = RiskCategory::Payment;
        }
    }
    // Latin-script accents are only a language signal on controls: rows and texts carry
    // names ("José", "Café Central").
    let foreign = has_foreign_word(&el.label)
        || (!is_free_text_role(&el.role) && has_non_english_letters(&el.label));
    let unreadable = label_is_unreadable(&el.label)
        || el.value.as_deref().is_some_and(label_is_unreadable)
        || foreign
        || unlabeled;
    if pressable && !content_row && !matches!(el.role, Role::Tab | Role::Key) && unreadable {
        risk = max_risk(risk, UNREADABLE_RISK);
    }
    risk
}

/// The element `Type` lands in: the given one, or the focused text input.
pub(crate) fn type_target(element: Option<u32>, screen: &CompactScreen) -> Option<&Element> {
    match element {
        Some(i) => screen.get(i),
        None => screen
            .elements
            .iter()
            .find(|e| e.focused && e.role.is_text_input()),
    }
}

/// Risk of typing `text` (and pressing Return when `submit`).
fn type_risk(element: Option<u32>, text: &str, submit: bool, r: &Reader<'_>) -> RiskCategory {
    let target = type_target(element, r.screen);
    let mut risk = RiskCategory::None;
    if let Some(t) = target {
        let field = format!(
            "{} {}",
            bounded(&t.label),
            bounded(t.placeholder.as_deref().unwrap_or(""))
        );
        if t.secure
            || t.role == Role::SecureTextField
            || match_label(&field, &[CREDENTIAL_FIELD_PHRASES]).category != RiskCategory::None
        {
            risk = RiskCategory::Credential;
        }
    }
    if contains_secret(text) {
        risk = RiskCategory::Credential;
    }
    if submit {
        risk = max_risk(risk, submit_risk(target, r));
        if looks_like_url(text) {
            risk = max_risk(risk, url_risk(text));
        }
    }
    risk
}

/// Return-key labels that only move the caret, search or go to the next field: they add
/// nothing to a submit.
const PLAIN_RETURN_LABELS: &[&str] = &["return", "search", "next", "enter", "route"];

/// Risk announced by the keyboard's Return key: its label as compacted into
/// `screen.cues.return_key` (or, on a hand-built screen that still lists keys, the `Key`
/// elements). A label that is an action word asks under its category ("send" is
/// `SendMessage`, "pay" `Payment`, "post" `PublicPost`, "call" `Call`); a generic
/// confirmation ("go", "done", "join", "continue") means what the screen around it means,
/// so it takes the screen's context risk (a "done" key on a checkout pays). Plain labels
/// ("return", "search", "next") and the backspace key add nothing.
fn return_key_risk(target: Option<&Element>, r: &Reader<'_>) -> RiskCategory {
    let screen = r.screen;
    let cue = screen.cues.return_key.as_deref();
    let keys = screen
        .elements
        .iter()
        .filter(|e| e.role == Role::Key)
        .map(|e| e.label.as_str());
    let tables = action_tables(&screen.app_bundle_id);
    let mut risk = RiskCategory::None;
    let mut generic = false;
    for label in cue.into_iter().chain(keys) {
        let plain = PLAIN_RETURN_LABELS.contains(&tokens(label).join(" ").as_str());
        let lr = match_label(label, &tables);
        match lr.category {
            // The keyboard's backspace key is labelled "delete".
            RiskCategory::Delete => {}
            other => risk = max_risk(risk, other),
        }
        generic |= lr.generic && !plain;
    }
    if generic {
        risk = max_risk(risk, r.context(target.map(|t| t.index)).risk);
    }
    risk
}

/// Risk of pressing Return in `target`: the compose field's own meaning, the Return key's
/// label and the screen's context (a Send button, a price). Search fields only search
/// (unless the Return key itself says otherwise).
fn submit_risk(target: Option<&Element>, r: &Reader<'_>) -> RiskCategory {
    let key = return_key_risk(target, r);
    let rest = match target {
        Some(t) if t.role == Role::SearchField => RiskCategory::None,
        Some(t) => {
            let field = format!(
                "{} {}",
                bounded(&t.label),
                bounded(t.placeholder.as_deref().unwrap_or(""))
            );
            let mut compose = match_label(&field, &[COMPOSE_FIELD_PHRASES]).category;
            if t.role == Role::TextView {
                // Return in a multi-line view only inserts a newline unless the view is a
                // chat composer, so a submit there is presumed to send.
                compose = max_risk(compose, RiskCategory::SendMessage);
            }
            max_risk(compose, r.context(Some(t.index)).risk)
        }
        None => r.context(None).risk,
    };
    max_risk(key, rest)
}

/// Whether `role`/`el` is a control whose value or state a drag changes: a slider, switch,
/// picker or stepper (by role, or an `Other` role of that name), or anything labelled as a
/// slide/swipe action ("Slide to pay", "Swipe up to unlock").
fn is_adjustable(el: &Element) -> bool {
    let named = match &el.role {
        Role::Slider | Role::Switch | Role::Picker => true,
        Role::Other(name) => {
            let n: String = name
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase();
            matches!(
                n.as_str(),
                "slider" | "switch" | "toggle" | "picker" | "pickerwheel" | "stepper"
            )
        }
        _ => false,
    };
    named
        || tokens(&bounded(&el.label))
            .iter()
            .any(|t| matches!(t.as_str(), "slide" | "swipe"))
}

/// Least ratio of the smaller to the larger width, and of the smaller to the larger height,
/// for two cells or buttons side by side to count as items of one carousel or grid: nearly
/// the same size. Two list columns of a split view (a 320 pt row next to a 600 pt row) are
/// not.
pub(crate) const SIBLING_MIN_SIZE_RATIO: f64 = 0.9;
/// Most distance, in points, between the top edges of two carousel or grid items: their
/// tops are aligned. Rows of two lists side by side whose tops do not line up are rows.
pub(crate) const SIBLING_MAX_TOP_OFFSET: f64 = 2.0;

/// Cells and buttons are one family here: App Store shelves mix cells and "Get" buttons.
fn row_family(role: &Role) -> bool {
    matches!(role, Role::Cell | Role::Button)
}

/// Whether `a` and `b` sit side by side as items of one carousel or grid: no horizontal
/// overlap, top edges aligned (within [`SIBLING_MAX_TOP_OFFSET`]) and nearly the same size
/// (width and height ratios at least [`SIBLING_MIN_SIZE_RATIO`]).
///
/// Residual case: two equally wide list columns whose rows have the same height and line up
/// (a split view with both columns the same width, each a list of equal rows) read as a
/// grid, so a sideways drag on such a row is not taken for Delete. Nothing in the
/// accessibility tree tells those apart from a two-column grid of tiles.
fn side_by_side(a: &Element, b: &Element) -> bool {
    let (fa, fb) = (a.frame, b.frame);
    let ratio = |x: f64, y: f64| {
        let (lo, hi) = (x.min(y), x.max(y));
        if hi > 0.0 {
            lo / hi
        } else {
            0.0
        }
    };
    (fa.max_x() <= fb.x || fb.max_x() <= fa.x)
        && (fa.y - fb.y).abs() <= SIBLING_MAX_TOP_OFFSET
        && ratio(fa.height, fb.height) >= SIBLING_MIN_SIZE_RATIO
        && ratio(fa.width, fb.width) >= SIBLING_MIN_SIZE_RATIO
}

/// The indices of the cells and buttons of `screen` that have a sibling of their family
/// beside them ([`side_by_side`]): items of a carousel or grid. One sweep in vertical order;
/// each element is compared only with the ones whose top is within
/// [`SIBLING_MAX_TOP_OFFSET`] of its own.
fn beside_siblings(screen: &CompactScreen) -> BTreeSet<u32> {
    let mut items: Vec<&Element> = screen
        .elements
        .iter()
        .filter(|e| row_family(&e.role) && !e.frame.is_empty())
        .collect();
    items.sort_by(|a, b| a.frame.y.total_cmp(&b.frame.y));
    let mut out = BTreeSet::new();
    for (k, a) in items.iter().enumerate() {
        for b in items.iter().skip(k.saturating_add(1)) {
            if b.frame.y - a.frame.y > SIBLING_MAX_TOP_OFFSET {
                break;
            }
            if side_by_side(a, b) {
                out.insert(a.index);
                out.insert(b.index);
            }
        }
    }
    out
}

/// Whether `el` is a list or button row that reveals swipe actions on a sideways drag: a
/// cell or button, outside the home screen (whose icons have none: a sideways swipe there
/// turns the page), **unless** a sibling of its family sits beside it ([`side_by_side`]:
/// another horizontal place, top edges aligned, nearly the same size): then it is an item of
/// a carousel or grid (music, App Store and news shelves, photo grids), where a sideways
/// drag pages. There is no screen-width threshold: a row in a split view's narrow column
/// (beside the wider detail column's rows) or an inset-grouped list is still a row.
///
/// A row is also short: taller than [`MAX_LIST_ROW_SHARE`] of the screen height, a lone
/// cell or button is a page (an onboarding page, a story, a paging card), where a sideways
/// drag turns the page. With an unknown screen height only the sibling rule applies.
fn is_list_row(el: &Element, r: &Reader<'_>) -> bool {
    let screen_h = r.screen.size.height;
    let page =
        screen_h.is_finite() && screen_h > 0.0 && el.frame.height > screen_h * MAX_LIST_ROW_SHARE;
    row_family(&el.role)
        && !page
        && !in_app(&r.screen.app_bundle_id, SPRINGBOARD_BUNDLE_ID)
        && !r.is_beside_sibling(el)
}

/// Tallest share of the screen height a list row with swipe actions takes (see
/// [`is_list_row`]): a 200 pt row on an 852 pt screen is a row, a 420 pt card is a page.
pub(crate) const MAX_LIST_ROW_SHARE: f64 = 0.25;

/// Whether a drag in finger direction `finger` that crosses `el` acts on it. Sideways:
/// adjustable controls (sliders, switches, steppers, slide-to-act controls) and list rows
/// ([`is_list_row`]), whose swipe actions a sideways drag reveals and a long one triggers;
/// not the items of a carousel or grid (a sibling beside them), nor home-screen icons.
/// Vertically: pickers, vertical sliders and slide-to-act controls; rows only scroll.
/// Disabled elements and keyboard keys never react.
fn reacts_to_drag(el: &Element, finger: Direction, r: &Reader<'_>) -> bool {
    if !el.enabled || el.role == Role::Key {
        return false;
    }
    let sideways = matches!(finger, Direction::Left | Direction::Right);
    if sideways {
        is_adjustable(el) || is_list_row(el, r)
    } else {
        match el.role {
            Role::Picker => true,
            Role::Slider => el.frame.height > el.frame.width,
            Role::Switch => false,
            _ => is_adjustable(el),
        }
    }
}

/// The controls whose frame the straight path `from` → `to` touches and that react to a
/// drag in finger direction `finger` ([`reacts_to_drag`]), in screen order. Filtered first,
/// so any number of texts or inert elements on the path cannot crowd a control out.
fn crossed_controls<'a>(
    r: &Reader<'a>,
    from: Point,
    to: Point,
    finger: Direction,
) -> Vec<&'a Element> {
    let (x0, x1) = (from.x.min(to.x), from.x.max(to.x));
    let (y0, y1) = (from.y.min(to.y), from.y.max(to.y));
    r.screen
        .elements
        .iter()
        .filter(|e| {
            let f = e.frame;
            !f.is_empty() && f.x <= x1 && f.max_x() >= x0 && f.y <= y1 && f.max_y() >= y0
        })
        .filter(|e| reacts_to_drag(e, finger, r))
        .collect()
}

/// Risk of a scroll or swipe (see [`classify`]): the drag is computed exactly as the
/// command will make it; the target element's own risk counts for swipes, sideways
/// scrolls and scrolls of adjustable controls, and every control the path crosses that
/// reacts to the drag adds its swipe risk. More than [`MAX_SCANNED_CONTROLS`] such
/// controls is at least [`UNREADABLE_RISK`] (asked, never silently skipped).
fn gesture_risk(
    kind: GestureKind,
    direction: Direction,
    element: Option<u32>,
    r: &Reader<'_>,
) -> RiskCategory {
    let screen = r.screen;
    let finger = finger_direction(kind, direction);
    let sideways = matches!(finger, Direction::Left | Direction::Right);
    let target = match element {
        Some(i) => match screen.get(i) {
            Some(el) => Some(el),
            None => return r.context(None).risk,
        },
        None => None,
    };
    let mut risk = RiskCategory::None;
    if let Some(el) = target {
        let own = kind == GestureKind::Swipe || sideways || is_adjustable(el);
        if own {
            risk = swipe_risk(el, finger, r);
        }
    }
    // Without a computable path (unknown screen size, an element in the system-gesture
    // margin) the runner gets no command at all; nothing more to read.
    if let Ok(g) = gesture(kind, direction, target.map(|e| e.frame), screen.size) {
        let crossed: Vec<&Element> = crossed_controls(r, g.from, g.to, finger)
            .into_iter()
            .filter(|el| !target.is_some_and(|t| t.index == el.index))
            .collect();
        if crossed.len() > MAX_SCANNED_CONTROLS {
            risk = max_risk(risk, UNREADABLE_RISK);
        }
        for el in crossed.into_iter().take(MAX_SCANNED_CONTROLS) {
            risk = max_risk(risk, swipe_risk(el, finger, r));
        }
    }
    risk
}

/// Risk of swiping on `el`: read like a tap of it (with context); sideways on a list row
/// ([`is_list_row`]) it is at least `Delete`. Vertical swipes on adjustable controls are
/// read like taps too; on other elements they only read their label, since they mostly
/// scroll.
fn swipe_risk(el: &Element, direction: Direction, r: &Reader<'_>) -> RiskCategory {
    let sideways = matches!(direction, Direction::Left | Direction::Right);
    if sideways && is_list_row(el, r) {
        max_risk(RiskCategory::Delete, element_risk(el, r))
    } else if sideways || is_adjustable(el) {
        element_risk(el, r)
    } else {
        match_label(&element_text(el), &action_tables(&r.screen.app_bundle_id)).category
    }
}

/// Risk of tapping a normalized point, and the element it lands on.
///
/// The target is the riskiest pressable element under the point (ties: the smallest);
/// its risk decides. With no pressable element there, the target is the largest element
/// under the point and the screen context counts too, since what is really there is
/// unknown.
pub(crate) fn tap_point(
    x: f64,
    y: f64,
    screen: &CompactScreen,
) -> (RiskCategory, Option<&Element>) {
    tap_point_in(x, y, &Reader::new(screen))
}

fn tap_point_in<'a>(x: f64, y: f64, r: &Reader<'a>) -> (RiskCategory, Option<&'a Element>) {
    let screen = r.screen;
    if !(x.is_finite() && y.is_finite()) {
        return (r.context(None).risk, None);
    }
    let p = Point::new(x * screen.size.width, y * screen.size.height);
    let hits: Vec<&Element> = screen
        .elements
        .iter()
        .filter(|e| e.frame.contains(p))
        .collect();
    // Pressable elements first, smallest first (what a finger hits), then capped: texts
    // and containers under the point never use up the window.
    let mut pressable: Vec<&Element> = hits
        .iter()
        .copied()
        .filter(|e| is_pressable(&e.role))
        .collect();
    pressable.sort_by(|a, b| a.frame.area().total_cmp(&b.frame.area()));
    let too_many = pressable.len() > MAX_SCANNED_CONTROLS;
    let best_pressable = pressable
        .into_iter()
        .take(MAX_SCANNED_CONTROLS)
        .map(|e| (element_risk(e, r), e))
        .max_by(|(ra, a), (rb, b)| {
            severity(*ra)
                .cmp(&severity(*rb))
                .then(b.frame.area().total_cmp(&a.frame.area()))
        });
    if let Some((risk, el)) = best_pressable {
        let risk = if too_many {
            max_risk(risk, UNREADABLE_RISK)
        } else {
            risk
        };
        return (risk, Some(el));
    }
    let mut risk = r.context(None).risk;
    let others: Vec<&&Element> = hits.iter().filter(|e| !e.role.is_text_input()).collect();
    if others.len() > MAX_SCANNED_CONTROLS {
        risk = max_risk(risk, UNREADABLE_RISK);
    }
    for el in others.into_iter().take(MAX_SCANNED_CONTROLS) {
        risk = max_risk(risk, element_risk(el, r));
    }
    let largest = hits
        .iter()
        .max_by(|a, b| a.frame.area().total_cmp(&b.frame.area()))
        .copied();
    (risk, largest)
}
