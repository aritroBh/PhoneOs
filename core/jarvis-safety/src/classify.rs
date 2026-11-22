//! Risk classification: what could go wrong if an action ran unattended.

use jarvis_proto::{
    Action, CompactScreen, Direction, Element, Point, RiskCategory, Role, Tier, ToolSpec,
};

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
};
use crate::validate::{looks_like_url, url_scheme};

/// Most elements read as context for one decision. On bigger screens the elements
/// nearest (vertically) to the target are read, so padding a page cannot push a price or
/// warning out of view of the button next to it.
pub(crate) const MAX_CONTEXT_ELEMENTS: usize = 200;

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
///   button). A price in a button label is at least a `Purchase`; "Book"/"Order"/"Get"
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
/// * Swipes are read like taps of their element (with context); a sideways swipe on a cell
///   or button row is at least `Delete` (full swipes trigger the destructive action).
/// * `OpenUrl` to `tel:`/`facetime:` is `Call`.
/// * Everything else is `None`. An element index that does not exist falls back to the
///   screen context (and is rejected by `validate_action` anyway).
pub fn classify(action: &Action, screen: &CompactScreen, tool: Option<&ToolSpec>) -> RiskCategory {
    match action {
        Action::Tap { element } | Action::LongPress { element, .. } => match screen.get(*element) {
            Some(el) => element_risk(el, screen),
            None => context_of(screen, None).risk,
        },
        Action::Type {
            element,
            text,
            submit,
        } => type_risk(*element, text, *submit, screen),
        Action::Swipe {
            direction,
            element: Some(i),
        } => match screen.get(*i) {
            Some(el) => swipe_risk(el, *direction, screen),
            None => context_of(screen, None).risk,
        },
        Action::TapPoint { x, y } => tap_point(*x, *y, screen).0,
        Action::OpenUrl { url } => url_risk(url),
        Action::CallTool { name, args } => tool_risk(name, args, tool),
        Action::Clear { .. }
        | Action::Scroll { .. }
        | Action::Swipe { element: None, .. }
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
fn element_text(el: &Element) -> String {
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

/// The elements read as context for `exclude` (see [`MAX_CONTEXT_ELEMENTS`]).
fn context_elements(screen: &CompactScreen, exclude: Option<u32>) -> Vec<&Element> {
    let mut els: Vec<&Element> = screen
        .elements
        .iter()
        .filter(|e| Some(e.index) != exclude)
        .collect();
    if els.len() > MAX_CONTEXT_ELEMENTS {
        if let Some(y) = exclude
            .and_then(|i| screen.get(i))
            .map(|e| e.frame.center().y)
        {
            els.sort_by(|a, b| {
                (a.frame.center().y - y)
                    .abs()
                    .total_cmp(&(b.frame.center().y - y).abs())
            });
        }
        els.truncate(MAX_CONTEXT_ELEMENTS);
    }
    els
}

/// Reads the screen apart from element `exclude`. Keyboard keys are ignored.
pub(crate) fn context_of(screen: &CompactScreen, exclude: Option<u32>) -> Context {
    let mut tables: Vec<&[Phrase]> = action_tables(&screen.app_bundle_id);
    tables.push(CONTEXT_PHRASES);
    let mut ctx = Context::default();
    for el in context_elements(screen, exclude) {
        if el.role == Role::Key {
            continue;
        }
        let text = element_text(el);
        let toks = tokens(&text);
        let cat = match_tokens(&toks, &tables).category;
        ctx.risk = max_risk(ctx.risk, cat);
        ctx.payment_words |= cat == RiskCategory::Payment;
        ctx.purchase_words |= matches!(cat, RiskCategory::Purchase | RiskCategory::Payment);
        if has_money(&text) {
            ctx.money = true;
            ctx.risk = max_risk(ctx.risk, RiskCategory::Payment);
        }
        ctx.share_sheet |=
            toks.iter().any(|t| t == "airdrop") || toks.windows(2).any(|w| w == ["air", "drop"]);
        ctx.settings_words |=
            match_tokens(&toks, &[SETTINGS_CONTEXT_PHRASES]).category != RiskCategory::None;
    }
    ctx
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

/// Risk of tapping (or long-pressing) `el`.
pub(crate) fn element_risk(el: &Element, screen: &CompactScreen) -> RiskCategory {
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
    if has_money(&text) && !content_row {
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
        let ctx = context_of(screen, Some(el.index));
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
fn type_risk(
    element: Option<u32>,
    text: &str,
    submit: bool,
    screen: &CompactScreen,
) -> RiskCategory {
    let target = type_target(element, screen);
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
        risk = max_risk(risk, submit_risk(target, screen));
        if looks_like_url(text) {
            risk = max_risk(risk, url_risk(text));
        }
    }
    risk
}

/// Risk announced by the keyboard's Return key ("send", "post"); the backspace key and
/// harmless return labels ("go", "search", "done") add nothing.
fn return_key_risk(screen: &CompactScreen) -> RiskCategory {
    screen
        .elements
        .iter()
        .filter(|e| e.role == Role::Key)
        .take(MAX_CONTEXT_ELEMENTS)
        .map(
            |e| match match_label(&e.label, &[ACTION_PHRASES]).category {
                RiskCategory::Delete => RiskCategory::None,
                other => other,
            },
        )
        .fold(RiskCategory::None, max_risk)
}

/// Risk of pressing Return in `target`: the compose field's own meaning, the Return key's
/// label and the screen's context (a Send button, a price). Search fields only search
/// (unless the Return key itself says otherwise).
fn submit_risk(target: Option<&Element>, screen: &CompactScreen) -> RiskCategory {
    let key = return_key_risk(screen);
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
            max_risk(compose, context_of(screen, Some(t.index)).risk)
        }
        None => context_of(screen, None).risk,
    };
    max_risk(key, rest)
}

/// Risk of swiping on `el`: read like a tap of it (with context); sideways on a cell or
/// button row it is at least `Delete`. Vertical swipes on other elements only read their
/// label, since they mostly scroll.
fn swipe_risk(el: &Element, direction: Direction, screen: &CompactScreen) -> RiskCategory {
    let sideways = matches!(direction, Direction::Left | Direction::Right);
    let row = matches!(el.role, Role::Cell | Role::Button);
    if sideways && row {
        max_risk(RiskCategory::Delete, element_risk(el, screen))
    } else if sideways || matches!(el.role, Role::Slider | Role::Switch) {
        element_risk(el, screen)
    } else {
        match_label(&element_text(el), &action_tables(&screen.app_bundle_id)).category
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
    if !(x.is_finite() && y.is_finite()) {
        return (context_of(screen, None).risk, None);
    }
    let p = Point::new(x * screen.size.width, y * screen.size.height);
    let hits: Vec<&Element> = screen
        .elements
        .iter()
        .filter(|e| e.frame.contains(p))
        .collect();
    let best_pressable = hits
        .iter()
        .filter(|e| is_pressable(&e.role))
        .map(|e| (element_risk(e, screen), *e))
        .max_by(|(ra, a), (rb, b)| {
            severity(*ra)
                .cmp(&severity(*rb))
                .then(b.frame.area().total_cmp(&a.frame.area()))
        });
    if let Some((risk, el)) = best_pressable {
        return (risk, Some(el));
    }
    let mut risk = context_of(screen, None).risk;
    for el in &hits {
        if !el.role.is_text_input() {
            risk = max_risk(risk, element_risk(el, screen));
        }
    }
    let largest = hits
        .iter()
        .max_by(|a, b| a.frame.area().total_cmp(&b.frame.area()))
        .copied();
    (risk, largest)
}
