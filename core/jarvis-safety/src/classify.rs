//! Risk classification: what could go wrong if an action ran unattended.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use jarvis_proto::{
    Action, CompactScreen, Direction, Element, Point, RiskCategory, Role, Tier, ToolSpec,
};

use crate::gestures::{drag_gesture, finger_direction, GestureKind};
use crate::phrases::{
    action_tables, covers_all, in_app, match_label, match_label_read, match_tokens, match_whole,
    max_risk, severity, sf_symbol_risk, LabelRisk, Phrase, RuleTarget, ACTION_PHRASES, APP_RULES,
    COMPOSE_FIELD_PHRASES, CONTEXT_PHRASES, CREDENTIAL_FIELD_PHRASES, NAVIGATION_PHRASES,
    PRICE_VERB_PHRASES, SETTINGS_ACTION_PHRASES, SETTINGS_BUNDLE_ID, SETTINGS_CONTEXT_PHRASES,
    SHARE_SAFE_PHRASES, SIGN_IN_PHRASES, WHOLE_LABEL_PHRASES,
};
use crate::redact::contains_secret;
use crate::sets::{any_units, hard_words, match_sets_whole, match_units, whole_units, with_units};
use crate::text::{
    bounded, has_foreign_word, has_money, has_non_english_letters, label_is_unreadable, tokens,
    CURRENCY_WORDS, MAX_LABEL_CHARS,
};
use crate::validate::{looks_like_url, url_scheme};
use jarvis_proto::SPRINGBOARD_BUNDLE_ID;

/// "Many controls" under one `tap_point` or on one drag's path. The elements are filtered to
/// the ones that matter (controls that react to the drag, pressable elements under the
/// point). Review pass 31 (M2): it no longer limits what is classified — every such control
/// is (it was the 32 smallest, so a denied "Delete account" under 33 decoys became a
/// question naming a decoy); each costs O(1) beyond its own label, the screen's context
/// being read once per reader. Above this many, the result is at least [`UNREADABLE_RISK`],
/// the question says how many controls are there and names the riskiest found, and it is
/// asked phone only (`many_controls`).
pub(crate) const MAX_SCANNED_CONTROLS: usize = 32;

/// Category for a tap the phrase net cannot read (unknown script or language, oversized
/// label, unknown symbol name, no label at all) and for a tool call without a spec. It
/// ranks below every definite signal, so a known trash symbol or a checkout screen still
/// wins.
pub(crate) const UNREADABLE_RISK: RiskCategory = RiskCategory::Unverified;

/// Review pass 36 (F8): the longest label or value of a control that is classified by its
/// words. A pressable control whose label or value is longer is `Unverified` without its
/// words being read: asked, never truncated and trusted (review pass 37: tabs and keys too;
/// a 600-character tab ending in `De1ete` was read truncated and allowed). Basis:
/// the longest label of any real tree in the repository (106 trees) is 383 characters (a
/// Spotlight result `Cell`); the longest `Button` label is 102 (a lock-screen
/// notification).
pub(crate) const MAX_CONTROL_LABEL_CHARS: usize = 512;

/// Most string arguments of one tool call that are read.
pub(crate) const MAX_TOOL_STRINGS: usize = 64;

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
/// * `OpenUrl` to `tel:`/`facetime:` is `Call` (thirteenth review: the gate raises a link
///   the owner did not give to at least `SendMessage`, see `evaluate_owned`).
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
/// by `validate_action`). URLs in tool arguments and typed text, and a link the owner gave.
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

/// Sixteenth review (F7): every value of a tool call's arguments as text, for the owner's-words
/// rule: strings as they are, numbers rendered as written (`482913`, `0.5`), and the keys of
/// nested objects. Booleans carry one bit of a declared parameter and no text: they are not
/// collected. Fail closed: no declared default exists to excuse a number, so a number the
/// owner did not write asks. Depth-first, at most [`MAX_TOOL_STRINGS`] values.
pub(crate) fn tool_texts(v: &serde_json::Value, out: &mut Vec<String>) {
    if out.len() >= MAX_TOOL_STRINGS {
        return;
    }
    match v {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Number(n) => out.push(n.to_string()),
        serde_json::Value::Array(items) => items.iter().for_each(|i| tool_texts(i, out)),
        serde_json::Value::Object(map) => {
            for (k, i) in map {
                out.push(k.clone());
                tool_texts(i, out);
            }
        }
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
    crate::phrases::any_tokens(toks, table)
}

/// True when the text contains a phone-number-like run: 7–15 digits joined by spaces,
/// dashes, dots, parentheses or a leading `+`.
fn has_phone_number(text: &str) -> bool {
    let mut digits = 0usize;
    for c in text
        .chars()
        .map(crate::text::fold_digit)
        .chain(std::iter::once('x'))
    {
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
    let money = crate::text::has_money_in(&text, Some(&toks));
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
#[derive(Debug, Default, Clone)]
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
    /// Per element index, its first position in `screen.elements` and how many share it.
    positions: OnceCell<HashMap<u32, (usize, usize)>>,
    beside: OnceCell<BTreeSet<u32>>,
    stacked: OnceCell<HashMap<CardKey, bool>>,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(screen: &'a CompactScreen) -> Self {
        Reader {
            screen,
            tables: context_tables(screen),
            tally: OnceCell::new(),
            positions: OnceCell::new(),
            beside: OnceCell::new(),
            stacked: OnceCell::new(),
        }
    }

    /// Reads the screen apart from element `exclude`. Keyboard keys are ignored. Every
    /// element is read: the context only ever grows (the riskiest signal, flags that are
    /// set), so there is no window a page could push a price or a warning out of with
    /// padding. The whole screen is read once per [`Reader`]; excluding an element takes
    /// its own signal out of the counts.
    pub(crate) fn context(&self, exclude: Option<u32>) -> Context {
        let tally = self.tally.get_or_init(|| {
            // Review pass 36 (F8): read once per look (every reader of the screen the gate
            // is deciding on shares it).
            if let Some(t) = look_get(self.screen, |c| c.tally.clone()).flatten() {
                return t;
            }
            let mut t = Tally::default();
            for el in &self.screen.elements {
                t.add(signal_of(el, &self.tables));
            }
            look_put(self.screen, |c| c.tally = Some(t.clone()));
            t
        });
        // Every element with that index (indices are unique on a compacted screen). Review
        // pass 31 (M2): found through a per-reader map, so excluding one element costs O(1)
        // and classifying every control under a point or on a path stays linear.
        let Some(exclude) = exclude else {
            return tally.without(Signal::default());
        };
        let positions = self.positions.get_or_init(|| {
            let mut m: HashMap<u32, (usize, usize)> = HashMap::new();
            for (k, el) in self.screen.elements.iter().enumerate() {
                m.entry(el.index).or_insert((k, 0)).1 += 1;
            }
            m
        });
        let (minus, excluded) = match positions.get(&exclude) {
            None => (Signal::default(), 0),
            Some(&(k, n)) => (
                self.screen
                    .elements
                    .get(k)
                    .map(|el| signal_of(el, &self.tables))
                    .unwrap_or_default(),
                n,
            ),
        };
        let exclude = Some(exclude);
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

    /// Whether `el` (a page-shaped element, [`page_shaped`]) has an element of its family
    /// stacked above or below it ([`stacks_with`]). Worked out for every page-shaped element
    /// of the screen at once ([`stacked_cards`], twenty-second review F2: was a scan of the
    /// whole screen per element, quadratic on a screen of stacked tall cards).
    fn is_stacked(&self, el: &Element) -> bool {
        let known = self
            .stacked
            .get_or_init(|| stacked_cards(self.screen))
            .get(&card_key(el))
            .copied();
        // An element not of this screen (never, in practice): the direct scan.
        known.unwrap_or_else(|| self.screen.elements.iter().any(|o| stacks_with(el, o)))
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
    let mut lr = match_tokens(toks, &action_tables(&screen.app_bundle_id));
    // Probe round 3 (F4, F6): the OS's own whole labels (`WHOLE_LABEL_PHRASES`), read on
    // the whole label of a pressable control only (a text "Card" is no icon-only control).
    if is_pressable(&el.role) {
        let whole = match_whole(&tokens(&el.label), &[WHOLE_LABEL_PHRASES]);
        lr.category = max_risk(lr.category, whole.category);
    }
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

/// Risk of tapping (or long-pressing) `el`: the riskier of its own words
/// ([`element_risk_read`]) and of their look-alike character sets ([`element_risk_sets`],
/// review pass 36). Computed once per element per look (review pass 36, F8).
pub(crate) fn element_risk(el: &Element, r: &Reader<'_>) -> RiskCategory {
    let (plain, sets, _) = element_risks(el, r);
    max_risk(plain, sets)
}

/// `el`'s category by its own words and by their look-alike sets, and whether it is a
/// control the tables cannot read (`Unverified` raised by its label: another script or
/// language, no label, too long), cached per look.
fn element_risks(el: &Element, r: &Reader<'_>) -> (RiskCategory, RiskCategory, bool) {
    let at = position(r.screen, el);
    if let Some(hit) = at.and_then(|k| look_get(r.screen, |c| c.risks.get(&k).copied()).flatten()) {
        return hit;
    }
    let (plain, content_row, unverified) = element_risk_plain(el, r);
    let sets = element_risk_sets(el, r, content_row);
    if let Some(k) = at {
        look_put(r.screen, |c| {
            c.risks.insert(k, (plain, sets, unverified));
        });
    }
    (plain, sets, unverified)
}

/// The position of `el` in `screen.elements` when it is one of them (by address).
fn position(screen: &CompactScreen, el: &Element) -> Option<usize> {
    let base = screen.elements.as_ptr() as usize;
    let at = el as *const Element as usize;
    let size = std::mem::size_of::<Element>().max(1);
    let off = at.checked_sub(base)?;
    let k = off / size;
    (off % size == 0 && screen.elements.get(k).is_some_and(|e| std::ptr::eq(e, el))).then_some(k)
}

/// Review pass 35/36: the categories of a tap on the target of `action` (a tap or long
/// press): by its own words first, then by their look-alike character sets — kept both
/// (the gate carries them all: a denial of any refuses). Empty when the sets give nothing.
pub(crate) fn reading_risks(action: &Action, screen: &CompactScreen) -> Vec<RiskCategory> {
    let (Action::Tap { element } | Action::LongPress { element, .. }) = action else {
        return Vec::new();
    };
    let Some(el) = screen.get(*element) else {
        return Vec::new();
    };
    let r = Reader::new(screen);
    let (plain, sets, _) = element_risks(el, &r);
    if sets == RiskCategory::None {
        return Vec::new();
    }
    vec![plain, sets]
}

/// Review pass 37 (F5): whether the target of `action` (a tap or long press) is a control
/// whose label the tables cannot read as English (another language, an accented letter on
/// a button, another script) while its words now give a definite category (`Sėnd` reads
/// `SendMessage` after the canonical decomposition): the gate then carries `Unverified`
/// beside that category, so the stripped reading adds a category and never takes the
/// phone-only `Unverified` away.
pub(crate) fn unread_label_beside_a_category(action: &Action, screen: &CompactScreen) -> bool {
    let (Action::Tap { element } | Action::LongPress { element, .. }) = action else {
        return false;
    };
    let Some(el) = screen.get(*element) else {
        return false;
    };
    let (plain, _, unverified) = element_risks(el, &Reader::new(screen));
    unverified && plain != UNREADABLE_RISK
}

/// Review pass 36: the category of a tap on `el` by the look-alike character sets of its
/// label and value ([`crate::sets`]): the phrase tables, a generic word's context, the
/// settings and price verbs, a price beside a risk word, read as [`element_risk_read`]
/// reads them. Only raises: lowering tests (navigation, share-safe words) and everything
/// that does not depend on the words are left to the plain reading, whose category is
/// carried beside this one. `None` for text inputs (a tap only focuses them), SF Symbol
/// names, and a label or value longer than [`MAX_CONTROL_LABEL_CHARS`] (that control is
/// unverified already).
fn element_risk_sets(el: &Element, r: &Reader<'_>, content_row: bool) -> RiskCategory {
    let screen = r.screen;
    if el.role.is_text_input()
        || too_long(&el.label)
        || el.value.as_deref().is_some_and(too_long)
        || sf_symbol_risk(&el.label).is_some()
    {
        return RiskCategory::None;
    }
    let tables = action_tables(&screen.app_bundle_id);
    let text = element_text(el);
    let pressable = is_pressable(&el.role);
    if pressable && too_soft(&text) {
        // Unverified by the plain reading, without either reading (review pass 37, F8).
        return RiskCategory::None;
    }
    // One reading of the units per text (review pass 36, F8). A long free-text row is
    // content (see `element_risk_read`). Its words are counted between separators here:
    // `Device Rem0ve` is two words, not the plain reading's four tokens. When the plain
    // reading took the row as content and these words do not, only what a look-alike adds
    // counts (`MacBook Air M3` stays a row's content).
    let read = |u: &[crate::sets::Unit], beyond: bool, whole_here: bool| {
        let mut lr = match_units(u, &tables, beyond);
        if whole_here {
            lr.category = max_risk(lr.category, whole_units(u, &[WHOLE_LABEL_PHRASES]).category);
        }
        (
            lr,
            any_units(u, SETTINGS_ACTION_PHRASES, false),
            any_units(u, PRICE_VERB_PHRASES, false),
        )
    };
    let label_is_text = el.value.as_deref().is_none_or(str::is_empty);
    let words = with_units(&text, |u| {
        let words_content = is_free_text_role(&el.role) && hard_words(u) > MAX_ROW_LABEL_WORDS;
        let beyond = content_row && !words_content;
        let here = (!words_content).then(|| read(u, beyond, pressable && !beyond && label_is_text));
        (words_content, beyond, here)
    });
    let (content_row, beyond, here) = words;
    let (mut lr, settings_verb, price_verb) = match here {
        Some(r) => r,
        None => {
            let label = bounded(&el.label);
            let first = label.split(',').next().unwrap_or("");
            let command = match_sets_whole(first, &tables);
            if command.category == RiskCategory::None && !command.generic {
                return RiskCategory::None;
            }
            with_units(first, |u| read(u, false, false))
        }
    };
    if pressable && !beyond && !(label_is_text && !content_row) {
        let whole = match_sets_whole(&el.label, &[WHOLE_LABEL_PHRASES]);
        lr.category = max_risk(lr.category, whole.category);
    }
    let mut risk = lr.category;
    if el.role == Role::Key && risk == RiskCategory::Delete {
        // The keyboard's backspace key is labelled "delete".
        risk = RiskCategory::None;
    }
    // A price beside the words, as the plain reading takes it (a cell that only names an
    // item and its price is left to the plain reading's priced-row rule).
    let priced_row =
        el.role == Role::Cell && risk == RiskCategory::None && !lr.generic && !price_verb;
    if !priced_row && !content_row && has_money(&text) {
        risk = match risk {
            RiskCategory::SendMessage | RiskCategory::Payment => RiskCategory::Payment,
            other => max_risk(other, RiskCategory::Purchase),
        };
    }
    if lr.generic || settings_verb || price_verb {
        let ctx = r.context(Some(el.index));
        if lr.generic {
            risk = max_risk(risk, ctx.risk);
        }
        let settings = ctx.settings_words || in_app(&screen.app_bundle_id, SETTINGS_BUNDLE_ID);
        if settings && settings_verb {
            risk = max_risk(risk, RiskCategory::AccountChange);
        }
        if (ctx.money || ctx.purchase_words) && !content_row && price_verb {
            risk = max_risk(risk, RiskCategory::Purchase);
        }
    }
    risk
}

/// Review pass 37 (F8): whether `text`'s look-alike units hold more soft boundaries than
/// [`crate::sets::MAX_SOFT_BOUNDARIES`] (only a text of more characters can).
fn too_soft(text: &str) -> bool {
    text.len() > crate::sets::MAX_SOFT_BOUNDARIES
        && with_units(text, crate::sets::soft_boundaries) > crate::sets::MAX_SOFT_BOUNDARIES
}

/// Whether `text` is longer than [`MAX_CONTROL_LABEL_CHARS`].
fn too_long(text: &str) -> bool {
    text.len() > MAX_CONTROL_LABEL_CHARS && text.chars().count() > MAX_CONTROL_LABEL_CHARS
}

/// Review pass 36: whether `label` is, as a whole, a sign-in word
/// ([`SIGN_IN_PHRASES`]) by its own words or their look-alike sets (`Sign ln`, `Log ln`).
pub(crate) fn sign_in_words(label: &str) -> bool {
    let b = bounded(label);
    match_whole(&tokens(&b), &[SIGN_IN_PHRASES]).category != RiskCategory::None
        || match_sets_whole(&b, &[SIGN_IN_PHRASES]).category != RiskCategory::None
}

/// Review pass 36: `text`'s category through the action phrases by its look-alike sets
/// alone (tests and surveys).
#[cfg(feature = "reference")]
pub(crate) fn lookalike_label_risk(text: &str) -> RiskCategory {
    crate::sets::match_sets(&bounded(text), &[ACTION_PHRASES]).category
}

/// Risk of tapping (or long-pressing) `el` by its own words (the classification before the
/// look-alike sets).
#[cfg(test)]
fn element_risk_read(el: &Element, r: &Reader<'_>) -> RiskCategory {
    element_risk_plain(el, r).0
}

/// [`element_risk_read`], whether `el` is a long free-text row (content), and whether its
/// label made it `Unverified` (see [`reading_risks`]).
fn element_risk_plain(el: &Element, r: &Reader<'_>) -> (RiskCategory, bool, bool) {
    let screen = r.screen;
    if el.role.is_text_input() {
        // Tapping a field only focuses it.
        return (RiskCategory::None, false, false);
    }
    let pressable = is_pressable(&el.role);
    // Review pass 36 (F8): a control named by more than a control's name holds is asked
    // without reading its words (never truncate-and-trust; a page of such labels costs no
    // more than measuring them). Review pass 37: tabs and keys too (their labels were read
    // truncated, and the sets skip such a label).
    if pressable && (too_long(&el.label) || el.value.as_deref().is_some_and(too_long)) {
        return (UNREADABLE_RISK, false, true);
    }
    let text = element_text(el);
    // Review pass 37 (F8): so is one whose words hold more soft boundaries than any real
    // label (`a1a1…`): asked without reading them, by neither reading.
    if pressable && too_soft(&text) {
        return (UNREADABLE_RISK, false, true);
    }
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
    // Review pass 37 (F7): a control is plainly navigation only when its label *is*
    // navigation words (every word inside a literal navigation phrase: `Back`, `Not now`,
    // `Close`), not when one stands anywhere among others (`Swipe up Back`, `Slide Help`).
    let navigation = covers_all(&toks, NAVIGATION_PHRASES);
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
        // Review pass 37 (F7): on a share sheet a row stays on the device only when its
        // label is share-safe words alone (`Copy`, `Save Image`); a row naming a person or
        // a device beside such a word (`Copy Anna's iPhone`) sends.
        if plain_target && ctx.share_sheet && !covers_all(&toks, SHARE_SAFE_PHRASES) {
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
    let unread_words = label_is_unreadable(&el.label)
        || el.value.as_deref().is_some_and(label_is_unreadable)
        || foreign;
    let asked = pressable && !content_row && !matches!(el.role, Role::Tab | Role::Key);
    if asked && (unread_words || unlabeled) {
        risk = max_risk(risk, UNREADABLE_RISK);
    }
    (risk, content_row, asked && unread_words)
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
            || match_label_read(&field, &[CREDENTIAL_FIELD_PHRASES]).category != RiskCategory::None
        {
            risk = RiskCategory::Credential;
        }
    }
    if contains_secret(text) {
        risk = RiskCategory::Credential;
    }
    if submit {
        risk = max_risk(risk, submit_risk(target, text, r));
        if looks_like_url(text) {
            risk = max_risk(risk, url_risk(text));
        }
    }
    risk
}

/// Return-key labels (`UIReturnKeyType`, English) that submit something rather than insert
/// a line break.
const SUBMIT_RETURN_LABELS: &[&str] = &[
    "go",
    "google",
    "join",
    "next",
    "route",
    "search",
    "send",
    "yahoo",
    "done",
    "emergency call",
    "continue",
];

/// Return-key labels that only move the caret, search or go to the next field: they add
/// nothing to a submit.
const PLAIN_RETURN_LABELS: &[&str] = &["return", "search", "next", "enter", "route"];

/// Risk announced by the keyboard's Return key: its type by identifier and its label as
/// compacted into `screen.cues` (audit C1: `ScreenCues::return_key_words`, so a localized
/// label cannot hide a `Send` key), or, on a hand-built screen that still lists keys, the
/// `Key` elements. A label that is an action word asks under its category ("send" is
/// `SendMessage`, "pay" `Payment`, "post" `PublicPost`, "call" `Call`); a generic
/// confirmation ("go", "done", "join", "continue") means what the screen around it means,
/// so it takes the screen's context risk (a "done" key on a checkout pays). Plain labels
/// ("return", "search", "next") and the backspace key add nothing.
fn return_key_risk(target: Option<&Element>, r: &Reader<'_>) -> RiskCategory {
    let screen = r.screen;
    let cue = screen.cues.return_key_words();
    let keys = screen
        .elements
        .iter()
        .filter(|e| e.role == Role::Key)
        .map(|e| e.label.as_str());
    let tables = action_tables(&screen.app_bundle_id);
    let mut risk = RiskCategory::None;
    let mut generic = false;
    for label in cue.iter().map(String::as_str).chain(keys) {
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
    // Probe round 3 (F3): a keyboard is up whose Return key no identifier types (the real
    // `.continue` key, any key whose type the table does not know): what Return does is
    // unknown, so it is at least `Unverified` (fail closed; its label still counts above).
    if screen.cues.return_key_unknown {
        risk = max_risk(risk, UNREADABLE_RISK);
    }
    risk
}

/// Risk of pressing Return in `target` after typing `text`, the highest of (seventh
/// review): the field's label and placeholder read through the action tables ("Type
/// DELETE to confirm" is `Delete`), the typed text read the same way (a confirmation word
/// typed into such a field), the compose field's own meaning, the Return key's label and the
/// screen's context (a Send button, a price). A search field keeps its leniency for the
/// typed text and the screen (Return there searches for the words, whatever they say),
/// not for its own label; so does a multi-line text view whose Return key is not a submit
/// label (Return inserts a line break: the text is content).
fn submit_risk(target: Option<&Element>, text: &str, r: &Reader<'_>) -> RiskCategory {
    let key = return_key_risk(target, r);
    let tables = action_tables(&r.screen.app_bundle_id);
    // Generic words ("OK", "confirm") mean what the screen means; definite ones count.
    let words = |t: &str| match_label(t, &tables).category;
    // Review pass 35: the field's own label and placeholder in every look-alike reading.
    let field_words = |t: &str| match_label_read(t, &tables).category;
    let named = target.map_or(RiskCategory::None, |t| {
        let placeholder = t.placeholder.as_deref().unwrap_or("");
        let named = max_risk(field_words(&t.label), field_words(placeholder));
        // Review pass 37: a label or placeholder longer than the matcher reads is read
        // truncated (its head and tail): a risk word in the cut middle is lost, so such a
        // field is `Unverified` at least (asked, never truncated and trusted).
        let over = |s: &str| s.len() > MAX_LABEL_CHARS && s.chars().count() > MAX_LABEL_CHARS;
        if over(&t.label) || over(placeholder) {
            max_risk(named, UNREADABLE_RISK)
        } else {
            named
        }
    });
    // In a multi-line text view a Return key that is not a submit label inserts a line
    // break: the typed text is content there, not a command (a note "buy milk"). Audit C1:
    // neither its type by identifier nor its label may be a submit.
    // Probe round 3 (F3): a Return key of unknown type may submit (no leniency).
    let line_break = target.is_some_and(|t| t.role == Role::TextView)
        && !r.screen.cues.return_key_unknown
        && r.screen
            .cues
            .return_key_words()
            .iter()
            .all(|k| !SUBMIT_RETURN_LABELS.contains(&tokens(k).join(" ").as_str()));
    let typed = if target.is_some_and(|t| t.role == Role::SearchField) || line_break {
        RiskCategory::None
    } else {
        words(&bounded(text))
    };
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
    max_risk(max_risk(key, rest), max_risk(named, typed))
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
    matches!(role, Role::Cell | Role::Button) || role.is_menu_item()
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
/// beside them ([`side_by_side`]): items of a carousel or grid. Sorted by top edge, each
/// element looks for one such sibling among the ones whose top is within
/// [`SIBLING_MAX_TOP_OFFSET`] of its own, below it in that order and then above, and stops
/// at the first (a row of a grid finds its neighbour at once, where comparing every pair
/// of a row is quadratic in the row's length).
fn beside_siblings(screen: &CompactScreen) -> BTreeSet<u32> {
    let mut items: Vec<&Element> = screen
        .elements
        .iter()
        .filter(|e| row_family(&e.role) && !e.frame.is_empty())
        .collect();
    items.sort_by(|a, b| a.frame.y.total_cmp(&b.frame.y));
    let mut out = BTreeSet::new();
    for (k, a) in items.iter().enumerate() {
        // `side_by_side` needs tops within the offset, so a top that is not a finite
        // number never has a sibling.
        if !a.frame.y.is_finite() {
            continue;
        }
        let below = items
            .iter()
            .skip(k.saturating_add(1))
            .take_while(|b| b.frame.y - a.frame.y <= SIBLING_MAX_TOP_OFFSET);
        let above = items
            .iter()
            .take(k)
            .rev()
            .take_while(|b| a.frame.y - b.frame.y <= SIBLING_MAX_TOP_OFFSET);
        if below.chain(above).any(|b| side_by_side(a, b)) {
            out.insert(a.index);
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
/// A **page** is not a row: a full-width cell or button (at least [`PAGE_MIN_WIDTH_SHARE`]
/// of the screen width) taller than [`MAX_LIST_ROW_SHARE`] of the screen height **with no
/// element of its family stacked above or below it at a similar width** (ratio at least
/// [`STACK_MIN_WIDTH_RATIO`]) in the shown set: an onboarding page, a story, a paging card,
/// where a sideways drag turns the page. A stack of tall rows is a list (Mail rows with a
/// long preview in landscape, rows at an accessibility text size), whose sideways drag
/// reveals Delete. Height alone never makes a page. With an unknown screen size only the
/// sibling rules apply.
fn is_list_row(el: &Element, r: &Reader<'_>) -> bool {
    row_family(&el.role)
        && !is_page(el, r)
        && !in_app(&r.screen.app_bundle_id, SPRINGBOARD_BUNDLE_ID)
        && !r.is_beside_sibling(el)
}

/// Whether `el` is a lone full-width tall card, see [`is_list_row`].
fn is_page(el: &Element, r: &Reader<'_>) -> bool {
    page_shaped(el, r.screen) && !r.is_stacked(el)
}

/// Whether `el` is full-width and taller than a row on a screen of known size: the shape of
/// a page, before the elements stacked with it are looked at ([`is_list_row`]).
fn page_shaped(el: &Element, screen: &CompactScreen) -> bool {
    let (w, h) = (screen.size.width, screen.size.height);
    let known = w.is_finite() && w > 0.0 && h.is_finite() && h > 0.0;
    !(!known
        || el.frame.height <= h * MAX_LIST_ROW_SHARE
        || el.frame.width < w * PAGE_MIN_WIDTH_SHARE)
}

/// Whether `o` is an element of `el`'s family stacked above or below it at a similar width
/// (see [`is_list_row`]).
fn stacks_with(el: &Element, o: &Element) -> bool {
    let ratio = {
        let (lo, hi) = (
            o.frame.width.min(el.frame.width),
            o.frame.width.max(el.frame.width),
        );
        if hi > 0.0 {
            lo / hi
        } else {
            0.0
        }
    };
    o.index != el.index
        && row_family(&o.role)
        && !o.frame.is_empty()
        && ratio >= STACK_MIN_WIDTH_RATIO
        && o.frame.x < el.frame.max_x()
        && el.frame.x < o.frame.max_x()
        && (o.frame.max_y() <= el.frame.y + STACK_OVERLAP_TOLERANCE
            || el.frame.max_y() <= o.frame.y + STACK_OVERLAP_TOLERANCE)
}

/// What [`stacked_cards`] keys an element by: its index and frame, all [`stacks_with`]
/// reads of it.
type CardKey = (u32, [u64; 4]);

fn card_key(el: &Element) -> CardKey {
    let f = el.frame;
    (
        el.index,
        [
            f.x.to_bits(),
            f.y.to_bits(),
            f.width.to_bits(),
            f.height.to_bits(),
        ],
    )
}

/// For every page-shaped element of `screen` ([`page_shaped`]), whether an element stacks
/// with it ([`stacks_with`]), in O(n log n) for any layout where the elements of a similar
/// width overlap horizontally (on-screen ones do). Two sweeps, one per side: the
/// candidates in the order of the edge a partner must clear, the possible partners in the
/// order of their own edge, each admitted once and kept by width; a candidate then checks
/// only the admitted ones of a width that can be within [`STACK_MIN_WIDTH_RATIO`] of its
/// own, with the exact [`stacks_with`]. A partner of a width that is not finite never
/// stacks (`x < x + NaN` is false; a ratio against an infinite width is 0 or `NaN`), nor
/// does a candidate of such a width: both are left out. The differential test holds it to
/// the scan it replaced.
fn stacked_cards(screen: &CompactScreen) -> HashMap<CardKey, bool> {
    let mut out: HashMap<CardKey, bool> = HashMap::new();
    let cards: Vec<&Element> = screen
        .elements
        .iter()
        .filter(|e| page_shaped(e, screen))
        .collect();
    if cards.is_empty() {
        return out;
    }
    for c in &cards {
        out.insert(card_key(c), false);
    }
    let partners: Vec<&Element> = screen
        .elements
        .iter()
        .filter(|o| row_family(&o.role) && !o.frame.is_empty() && o.frame.width.is_finite())
        .collect();
    let finite: Vec<&Element> = cards
        .into_iter()
        .filter(|c| c.frame.width.is_finite())
        .collect();
    // Above: `o.max_y <= c.y + tolerance`.
    sweep(
        &finite,
        &partners,
        |c| c.frame.y + STACK_OVERLAP_TOLERANCE,
        |o| o.frame.max_y(),
        false,
        &mut out,
    );
    // Below: `c.max_y <= o.y + tolerance`.
    sweep(
        &finite,
        &partners,
        |c| c.frame.max_y(),
        |o| o.frame.y + STACK_OVERLAP_TOLERANCE,
        true,
        &mut out,
    );
    out
}

/// One side of [`stacked_cards`]: a partner `o` is on that side of card `c` when
/// `edge(o) <= bound(c)` (`below` false) or `edge(o) >= bound(c)` (`below` true). A `NaN`
/// on either side never compares, so it is left out.
fn sweep(
    cards: &[&Element],
    partners: &[&Element],
    bound: impl Fn(&Element) -> f64,
    edge: impl Fn(&Element) -> f64,
    below: bool,
    out: &mut HashMap<CardKey, bool>,
) {
    let order = |a: &f64, b: &f64| {
        if below {
            b.total_cmp(a)
        } else {
            a.total_cmp(b)
        }
    };
    let mut cs: Vec<(f64, &Element)> = cards
        .iter()
        .map(|c| (bound(c), *c))
        .filter(|(b, _)| !b.is_nan())
        .collect();
    cs.sort_by(|a, b| order(&a.0, &b.0));
    let mut ps: Vec<(f64, &Element)> = partners
        .iter()
        .map(|o| (edge(o), *o))
        .filter(|(e, _)| !e.is_nan())
        .collect();
    ps.sort_by(|a, b| order(&a.0, &b.0));
    let admits = |e: f64, b: f64| if below { e >= b } else { e <= b };
    // Admitted partners by width (finite and positive: the bit order is the numeric order).
    let mut by_width: BTreeMap<u64, Vec<&Element>> = BTreeMap::new();
    let mut next = 0usize;
    for (b, c) in cs {
        while let Some((e, o)) = ps.get(next) {
            if !admits(*e, b) {
                break;
            }
            by_width
                .entry(o.frame.width.to_bits())
                .or_default()
                .push(*o);
            next += 1;
        }
        let key = card_key(c);
        if out.get(&key).copied().unwrap_or(false) {
            continue;
        }
        let w = c.frame.width;
        // A superset of the widths within the ratio (rounding included); a width too close
        // to the ends of the float range checks every admitted partner.
        let (lo, hi) = if (1e-300..=1e300).contains(&w) {
            (w * 0.8, w / 0.8)
        } else {
            (0.0, f64::INFINITY)
        };
        let hit = by_width
            .range(lo.to_bits()..=hi.to_bits())
            .flat_map(|(_, os)| os.iter())
            .any(|o| stacks_with(c, o));
        if hit {
            out.insert(key, true);
        }
    }
}

/// Tallest share of the screen height a lone full-width card may take and still be a row
/// (see [`is_list_row`]): a 200 pt row on an 852 pt screen is a row; a lone 420 pt card is
/// a page, three 240 pt rows stacked are a list.
pub(crate) const MAX_LIST_ROW_SHARE: f64 = 0.25;
/// Share of the screen width a page spans (a full-width card).
pub(crate) const PAGE_MIN_WIDTH_SHARE: f64 = 0.9;
/// Width ratio at which an element stacked above or below a tall card makes it a row.
pub(crate) const STACK_MIN_WIDTH_RATIO: f64 = 0.9;
/// Overlap (points) tolerated between rows stacked one above the other.
const STACK_OVERLAP_TOLERANCE: f64 = 1.0;

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
    if let Ok(g) = drag_gesture(kind, direction, target.map(|e| e.frame), screen) {
        let crossed: Vec<&Element> = crossed_controls(r, g.from, g.to, finger)
            .into_iter()
            .filter(|el| !target.is_some_and(|t| t.index == el.index))
            .collect();
        // Review pass 31 (M2): every crossed control is classified (linear: the context of
        // each is O(1) through the reader); more than the cap also makes it a guess.
        if crossed.len() > MAX_SCANNED_CONTROLS {
            risk = max_risk(risk, UNREADABLE_RISK);
        }
        for el in crossed {
            risk = max_risk(risk, swipe_risk(el, finger, r));
        }
    }
    risk
}

/// Thirteenth review: the control a scroll or swipe actuates, if any (see
/// `crate::actuation`): its element when that element reacts to the drag
/// ([`reacts_to_drag`]) or is button-like (a button, a link, an adjustable or "slide to"
/// control); else the first enabled control on the drag's path that reacts to it (the
/// first review's element-less drag across a pay slider); else the smallest enabled
/// button-like control under the point where the finger lands. `None`: the drag starts on
/// page content with no actionable control under it and crosses none — it only moves the
/// page. The path is the one the runner will make ([`gesture`]); without one (unknown
/// screen size, an element in the system-gesture margin) only the element counts.
pub(crate) fn drag_target(
    kind: GestureKind,
    direction: Direction,
    element: Option<u32>,
    screen: &CompactScreen,
) -> Option<&Element> {
    let r = Reader::new(screen);
    let finger = finger_direction(kind, direction);
    let target = element.and_then(|i| screen.get(i));
    let button_like = |el: &Element| {
        el.enabled
            && el.role != Role::Key
            && (matches!(el.role, Role::Button | Role::Link)
                || el.role.is_menu_item()
                || is_adjustable(el))
    };
    if let Some(el) = target {
        if button_like(el) || reacts_to_drag(el, finger, &r) {
            return Some(el);
        }
    }
    let g = drag_gesture(kind, direction, target.map(|e| e.frame), screen).ok()?;
    if let Some(el) = crossed_controls(&r, g.from, g.to, finger)
        .into_iter()
        .find(|el| !target.is_some_and(|t| t.index == el.index))
    {
        return Some(el);
    }
    screen
        .elements
        .iter()
        .filter(|e| e.frame.contains(g.from) && button_like(e))
        .min_by(|a, b| a.frame.area().total_cmp(&b.frame.area()))
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
        match_label_read(&element_text(el), &action_tables(&r.screen.app_bundle_id)).category
    }
}

/// Review pass 36 (F8): what the gate has worked out about the screen it is deciding on,
/// shared by every reader of that screen during one decision ([`look_scope`]): the context
/// counts, each element's categories and each point's target. Keyed by the screen's
/// address, which cannot change or be reused while the scope borrows it.
#[derive(Default)]
pub(crate) struct LookCache {
    screen: usize,
    tally: Option<Tally>,
    risks: HashMap<usize, (RiskCategory, RiskCategory, bool)>,
    points: HashMap<(u64, u64), (RiskCategory, Option<usize>)>,
}

thread_local! {
    static LOOK: std::cell::RefCell<Vec<LookCache>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// While alive, classifications of `screen` are computed once and shared (see
/// [`LookCache`]). Scopes nest; one for the screen already in scope adds nothing.
pub(crate) struct LookScope<'a> {
    pushed: bool,
    _screen: std::marker::PhantomData<&'a CompactScreen>,
}

/// Opens a [`LookScope`] on `screen`.
pub(crate) fn look_scope(screen: &CompactScreen) -> LookScope<'_> {
    let key = screen as *const CompactScreen as usize;
    let pushed = LOOK
        .with(|l| {
            l.try_borrow_mut().ok().map(|mut v| {
                if v.iter().any(|c| c.screen == key) {
                    false
                } else {
                    v.push(LookCache {
                        screen: key,
                        ..LookCache::default()
                    });
                    true
                }
            })
        })
        .unwrap_or(false);
    LookScope {
        pushed,
        _screen: std::marker::PhantomData,
    }
}

impl Drop for LookScope<'_> {
    fn drop(&mut self) {
        if self.pushed {
            LOOK.with(|l| {
                if let Ok(mut v) = l.try_borrow_mut() {
                    v.pop();
                }
            });
        }
    }
}

/// `f` of the cache of `screen` when a scope is open on it.
fn look_get<R>(screen: &CompactScreen, f: impl FnOnce(&LookCache) -> R) -> Option<R> {
    let key = screen as *const CompactScreen as usize;
    LOOK.with(|l| {
        let v = l.try_borrow().ok()?;
        v.iter().rev().find(|c| c.screen == key).map(f)
    })
}

/// Records that the point `(x, y)` of `screen` (a scope open on it) lands on the element
/// with `index` (its first one) with `risk`: a copy of a screen whose point was resolved on
/// the original.
pub(crate) fn look_seed_point(
    screen: &CompactScreen,
    x: f64,
    y: f64,
    risk: RiskCategory,
    index: Option<u32>,
) {
    let at = index.and_then(|i| screen.elements.iter().position(|e| e.index == i));
    if index.is_some() && at.is_none() {
        return;
    }
    look_put(screen, |c| {
        c.points.insert((x.to_bits(), y.to_bits()), (risk, at));
    });
}

/// Updates the cache of `screen` when a scope is open on it.
fn look_put(screen: &CompactScreen, f: impl FnOnce(&mut LookCache)) {
    let key = screen as *const CompactScreen as usize;
    LOOK.with(|l| {
        if let Ok(mut v) = l.try_borrow_mut() {
            if let Some(c) = v.iter_mut().rev().find(|c| c.screen == key) {
                f(c);
            }
        }
    });
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
    // Review pass 36 (F8): one resolution per point per look.
    let key = (x.to_bits(), y.to_bits());
    if let Some((risk, at)) = look_get(screen, |c| c.points.get(&key).copied()).flatten() {
        return (risk, at.and_then(|k| screen.elements.get(k)));
    }
    let (risk, el) = tap_point_uncached(x, y, r);
    let at = el.and_then(|e| position(screen, e));
    if el.is_none() || at.is_some() {
        look_put(screen, |c| {
            c.points.insert(key, (risk, at));
        });
    }
    (risk, el)
}

fn tap_point_uncached<'a>(x: f64, y: f64, r: &Reader<'a>) -> (RiskCategory, Option<&'a Element>) {
    let screen = r.screen;
    if !(x.is_finite() && y.is_finite()) {
        return (r.context(None).risk, None);
    }
    let p = Point::new(x * screen.size.width, y * screen.size.height);
    // Probe run 134: an element with no known position is never what a point lands on.
    let hits: Vec<&Element> = screen
        .elements
        .iter()
        .filter(|e| e.frame.contains(p) && !screen.cues.is_unplaced(e.index))
        .collect();
    // Pressable elements first, smallest first (what a finger hits), then capped: texts
    // and containers under the point never use up the window.
    let mut pressable: Vec<&Element> = hits
        .iter()
        .copied()
        .filter(|e| is_pressable(&e.role))
        .collect();
    pressable.sort_by(|a, b| a.frame.area().total_cmp(&b.frame.area()));
    // Review pass 31 (M2): every pressable element under the point is classified (it was the
    // 32 smallest: a denied "Delete account" under 33 small decoys became a question naming
    // a decoy). More than the cap also makes the result a guess (asked, phone only).
    let too_many = pressable.len() > MAX_SCANNED_CONTROLS;
    let best_pressable = pressable
        .into_iter()
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
    for el in others {
        risk = max_risk(risk, element_risk(el, r));
    }
    let largest = hits
        .iter()
        .max_by(|a, b| a.frame.area().total_cmp(&b.frame.area()))
        .copied();
    (risk, largest)
}

/// Review pass 31 (M2): what lies under a `tap_point` or on a drag's path when more than
/// [`MAX_SCANNED_CONTROLS`] controls are there (the classification read every one of them;
/// the question says how many, names the riskiest found, and is asked phone only).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ManyControls<'a> {
    /// How many controls (pressable elements under the point, controls the path crosses
    /// that react to it; without a pressable one under a point, the elements there).
    pub count: usize,
    /// The riskiest of them (ties: the first the classification met); `None` when the point
    /// holds no pressable element.
    pub riskiest: Option<&'a Element>,
    /// The action is a drag (a path), not a point.
    pub path: bool,
}

/// Review pass 31 (M2): [`ManyControls`] for `action` on `screen` when the point or path holds
/// more than [`MAX_SCANNED_CONTROLS`] controls (the same sets [`classify`] reads), else `None`.
pub(crate) fn many_controls<'a>(
    action: &Action,
    screen: &'a CompactScreen,
) -> Option<ManyControls<'a>> {
    let r = Reader::new(screen);
    match action {
        Action::TapPoint { x, y } => {
            if !(x.is_finite() && y.is_finite()) {
                return None;
            }
            let p = Point::new(x * screen.size.width, y * screen.size.height);
            let hits: Vec<&Element> = screen
                .elements
                .iter()
                .filter(|e| e.frame.contains(p) && !screen.cues.is_unplaced(e.index))
                .collect();
            let pressable = hits.iter().filter(|e| is_pressable(&e.role)).count();
            if pressable > MAX_SCANNED_CONTROLS {
                let (_, riskiest) = tap_point_in(*x, *y, &r);
                return Some(ManyControls {
                    count: pressable,
                    riskiest,
                    path: false,
                });
            }
            let others = hits.iter().filter(|e| !e.role.is_text_input()).count();
            (pressable == 0 && others > MAX_SCANNED_CONTROLS).then_some(ManyControls {
                count: others,
                riskiest: None,
                path: false,
            })
        }
        Action::Scroll { direction, element } | Action::Swipe { direction, element } => {
            let kind = if matches!(action, Action::Swipe { .. }) {
                GestureKind::Swipe
            } else {
                GestureKind::Scroll
            };
            let finger = finger_direction(kind, *direction);
            let target = element.and_then(|i| screen.get(i));
            let g = drag_gesture(kind, *direction, target.map(|e| e.frame), screen).ok()?;
            let crossed: Vec<&Element> = crossed_controls(&r, g.from, g.to, finger)
                .into_iter()
                .filter(|el| !target.is_some_and(|t| t.index == el.index))
                .collect();
            if crossed.len() <= MAX_SCANNED_CONTROLS {
                return None;
            }
            // The riskiest by its reading; among equals, the one whose own words carry the
            // risk (a money screen lends every control its context).
            let tables = action_tables(&screen.app_bundle_id);
            let key = |el: &Element| {
                (
                    severity(swipe_risk(el, finger, &r)),
                    severity(match_label(&element_text(el), &tables).category),
                )
            };
            let mut riskiest: Option<(_, &Element)> = None;
            for el in &crossed {
                let k = key(el);
                if riskiest.is_none_or(|(best, _)| k > best) {
                    riskiest = Some((k, el));
                }
            }
            Some(ManyControls {
                count: crossed.len(),
                riskiest: riskiest.map(|(_, e)| e),
                path: true,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod differential {
    use super::*;
    use jarvis_proto::fixtures::random::{random_compact_screen, random_element, Rng};
    use jarvis_proto::Rect;

    /// [`beside_siblings`] as it was before each element stopped at its first sibling:
    /// every pair within the offset compared.
    fn reference_beside_siblings(screen: &CompactScreen) -> BTreeSet<u32> {
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

    /// A grid of cells and buttons, some the same size and some not, tops jittered around
    /// the row's (within and beyond the offset), with stray elements, empty and `NaN` or
    /// infinite frames, and duplicate indices.
    fn grid(seed: u64) -> CompactScreen {
        let mut screen = random_compact_screen(seed);
        let mut r = Rng::new(seed ^ 0x6121D);
        let (cols, rows) = (1 + r.below(30), 1 + r.below(30));
        let (w, h) = (r.f(5.0, 40.0), r.f(5.0, 40.0));
        let mut index = 1u32;
        for row in 0..rows {
            for col in 0..cols {
                let mut e = random_element(&mut r, index, screen.size);
                e.role = if r.chance(70) {
                    Role::Cell
                } else {
                    Role::Button
                };
                let jitter = |r: &mut Rng, span: f64| r.f(-span, span);
                let (dw, dh) = if r.chance(20) {
                    (jitter(&mut r, 8.0), jitter(&mut r, 8.0))
                } else {
                    (0.0, 0.0)
                };
                let gap = if r.chance(10) { -3.0 } else { r.f(0.0, 3.0) };
                e.frame = Rect::new(
                    col as f64 * (w + gap),
                    row as f64 * (h + 1.0) + jitter(&mut r, 2.5),
                    w + dw,
                    h + dh,
                );
                if r.chance(1) {
                    e.frame.y = *r.pick(&[f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0]);
                }
                screen.elements.push(e);
                index += if r.chance(3) { 0 } else { 1 };
            }
        }
        screen
    }

    /// The new [`beside_siblings`] finds exactly the reference's set on thousands of random
    /// screens and grids, so every classification that reads it is unchanged.
    #[test]
    fn beside_siblings_match_the_reference() {
        let mut found = 0usize;
        for seed in 0..6_000u64 {
            for screen in [random_compact_screen(seed), grid(seed)] {
                let new = beside_siblings(&screen);
                assert_eq!(new, reference_beside_siblings(&screen), "seed {seed}");
                found += new.len();
            }
        }
        assert!(found > 10_000, "{found}");
    }

    /// `is_page` as it was before the twenty-second review (F2): a scan of the whole screen
    /// for each element. The oracle of [`stacked_cards`].
    fn reference_is_page(el: &Element, screen: &CompactScreen) -> bool {
        let (w, h) = (screen.size.width, screen.size.height);
        let known = w.is_finite() && w > 0.0 && h.is_finite() && h > 0.0;
        if !known
            || el.frame.height <= h * MAX_LIST_ROW_SHARE
            || el.frame.width < w * PAGE_MIN_WIDTH_SHARE
        {
            return false;
        }
        !screen.elements.iter().any(|o| {
            let ratio = {
                let (lo, hi) = (
                    o.frame.width.min(el.frame.width),
                    o.frame.width.max(el.frame.width),
                );
                if hi > 0.0 {
                    lo / hi
                } else {
                    0.0
                }
            };
            o.index != el.index
                && row_family(&o.role)
                && !o.frame.is_empty()
                && ratio >= STACK_MIN_WIDTH_RATIO
                && o.frame.x < el.frame.max_x()
                && el.frame.x < o.frame.max_x()
                && (o.frame.max_y() <= el.frame.y + STACK_OVERLAP_TOLERANCE
                    || el.frame.max_y() <= o.frame.y + STACK_OVERLAP_TOLERANCE)
        })
    }

    /// Tall cards and rows near the page shape: widths around the screen's and the ratio's
    /// edge, stacked, overlapping, side by side, off screen, with odd screen sizes, roles
    /// outside the family, `NaN`, infinite and empty frames, and duplicate indices.
    fn cards(seed: u64) -> CompactScreen {
        let mut screen = random_compact_screen(seed);
        let mut r = Rng::new(seed ^ 0xF2_CA4D);
        if r.chance(15) {
            screen.size = *r.pick(&[
                jarvis_proto::Size::new(f64::NAN, 852.0),
                jarvis_proto::Size::new(393.0, f64::INFINITY),
                jarvis_proto::Size::new(3.0, 2.0),
                jarvis_proto::Size::new(1e-310, 1e-310),
                jarvis_proto::Size::new(1e301, 1e301),
            ]);
        }
        let (sw, sh) = (screen.size.width, screen.size.height);
        let (sw, sh) = if sw.is_finite() && sh.is_finite() {
            (sw, sh)
        } else {
            (393.0, 852.0)
        };
        let n = 1 + r.below(60);
        let mut index = 500u32;
        for _ in 0..n {
            let mut e = random_element(&mut r, index, screen.size);
            e.role = r
                .pick(&[
                    Role::Cell,
                    Role::Cell,
                    Role::Button,
                    Role::Button,
                    Role::StaticText,
                ])
                .clone();
            let width = sw * r.f(0.7, 1.2);
            let height = if r.chance(70) {
                sh * r.f(0.2, 0.6)
            } else {
                sh * r.f(0.0, 0.3)
            };
            let x = match r.below(6) {
                0 => sw * r.f(-1.5, 1.5),
                1 => sw * r.below(3) as f64,
                _ => r.f(-4.0, 4.0),
            };
            let y = if r.chance(40) {
                // On a shared line so tops and bottoms meet within the tolerance.
                (sh * 0.25) * r.below(4) as f64 + r.f(-1.5, 1.5)
            } else {
                r.f(-sh, 2.0 * sh)
            };
            e.frame = Rect::new(x, y, width, height);
            if r.chance(3) {
                let odd = *r.pick(&[f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0, 0.0]);
                match r.below(4) {
                    0 => e.frame.x = odd,
                    1 => e.frame.y = odd,
                    2 => e.frame.width = odd,
                    _ => e.frame.height = odd,
                }
            }
            if r.chance(5) {
                e.frame.width = sw * 0.9;
            }
            screen.elements.push(e);
            index += if r.chance(10) { 0 } else { 1 };
        }
        screen
    }

    /// F2: the cached decision equals the scan it replaced on every element of thousands of
    /// random screens, and both outcomes occur.
    #[test]
    fn stacked_cards_match_the_reference() {
        let (mut pages, mut stacked) = (0usize, 0usize);
        for seed in 0..4_000u64 {
            for screen in [random_compact_screen(seed), cards(seed)] {
                let r = Reader::new(&screen);
                for el in &screen.elements {
                    let old = reference_is_page(el, &screen);
                    assert_eq!(is_page(el, &r), old, "seed {seed}: {el:?}");
                    if old {
                        pages += 1;
                    } else if page_shaped(el, &screen) {
                        stacked += 1;
                    }
                }
            }
        }
        assert!(pages > 1_000 && stacked > 1_000, "{pages} {stacked}");
    }
}

/// Review pass 35: ordinary labels holding digits, symbols, capital `I`s and alternating
/// case classify exactly as before the look-alike readings.
#[cfg(test)]
mod pass35_ordinary_labels {
    use super::*;
    use jarvis_proto::Rect;

    /// Labels real apps use that hold a look-alike candidate (digits beside letters, `@`,
    /// `$`, `|`, `!`, a capital `I` after a lowercase letter, several camel steps) or none.
    const ORDINARY: &[&str] = &[
        "iPhone",
        "WiFi",
        "Wi-Fi",
        "Wi\u{2011}Fi 6",
        "H2O",
        "MP3",
        "MP4",
        "B2B",
        "Top 10",
        "Top10",
        "Top5",
        "Pay42",
        "iOS 26",
        "iOS 26.5",
        "iPadOS",
        "5G",
        "4G LTE",
        "4K",
        "3D Touch",
        "v2",
        "v1.3",
        "24h",
        "1st",
        "2nd",
        "3rd",
        "4th",
        "Win10",
        "x86",
        "A1",
        "A17 Pro",
        "M3 Max",
        "S24 Ultra",
        "Pixel 8",
        "Series 9",
        "Q3 Report",
        "COVID-19",
        "USB-C",
        "H.264",
        "802.11ax",
        "Gate B7",
        "Seat 14A",
        "Flight UA857",
        "Room 101",
        "Route 66",
        "Bus 38",
        "Channel 5",
        "7-Eleven",
        "7Up",
        "3M",
        "R2D2",
        "C3PO",
        "B12",
        "Omega3",
        "Level 3",
        "Gr8",
        "B4",
        "L8r",
        "4ever",
        "1Password",
        "OpenID",
        "PayID",
        "Apple ID",
        "FaceID",
        "TouchID",
        "LinkedIn",
        "McIntosh",
        "YouTube",
        "YouTubeTV",
        "DuckDuckGo",
        "PlayStation 5",
        "AirPodsPro",
        "MacBookAir",
        "iCloud+",
        "eBay",
        "McDonald's",
        "AT&T",
        "Go!",
        "Done!",
        "Hello!",
        "Yahoo!",
        "Hi there!",
        "john@example.com",
        "@handle",
        "$HOME",
        "Home | Settings",
        "Model Number, A3257LL/A",
        "Serial Number, ZJLGNW7555",
        "PageFormatMenuButton",
        "Il y a 5 min",
        "AL",
        "Al Green",
        "10:41",
        "100%",
        "Sum 41",
        "Maroon 5",
        "U2",
        "Blink-182",
        "Send",
        "Delete",
        "Pay $5.00",
        "Buy 1 Get 1",
        "SignIn",
        "LogIn",
    ];

    fn element(role: Role, label: &str) -> Element {
        Element {
            index: 1,
            role,
            label: label.to_owned(),
            value: None,
            placeholder: None,
            frame: Rect::new(16.0, 40.0, 361.0, 36.0),
            enabled: true,
            focused: false,
            selected: false,
            secure: false,
            fingerprint: 1,
        }
    }

    #[test]
    fn ordinary_labels_classify_exactly_as_before() {
        assert!(ORDINARY.len() >= 60, "{}", ORDINARY.len());
        let mut wrong = Vec::new();
        for label in ORDINARY {
            for role in [Role::Button, Role::Cell, Role::Link, Role::StaticText] {
                for bundle in ["com.example.app", "com.apple.Preferences"] {
                    let el = element(role.clone(), label);
                    let screen = CompactScreen {
                        app_bundle_id: bundle.to_owned(),
                        app_name: Some("App".to_owned()),
                        size: jarvis_proto::fixtures::IPHONE_SIZE,
                        keyboard_visible: false,
                        elements: vec![el.clone()],
                        cues: Default::default(),
                    };
                    let r = Reader::new(&screen);
                    let before = element_risk_read(&el, &r);
                    let now = element_risk(&el, &r);
                    let readings = reading_risks(&Action::Tap { element: 1 }, &screen);
                    let extra = readings
                        .iter()
                        .any(|c| *c != RiskCategory::None && *c != before);
                    let tables = action_tables(bundle);
                    let own_before = match_label(label, &tables);
                    let own_now = match_label_read(label, &tables);
                    if now != before || extra || own_before != own_now {
                        wrong.push(format!(
                            "{label:?} {role:?} in {bundle}: {before:?} -> {now:?} \
                             (readings {readings:?}; own {own_before:?} -> {own_now:?})"
                        ));
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }
}
