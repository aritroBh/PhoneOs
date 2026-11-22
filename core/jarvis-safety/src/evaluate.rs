//! The single entry point the agent calls before executing an action.

use jarvis_proto::{
    Action, CompactScreen, ConfirmRequest, Element, Policy, RiskCategory, ToolSpec, TrustLevel,
};

use crate::app::{app_level, check_app, is_springboard, mode_level, Verdict};
use crate::classify::{classify, type_target};
use crate::gate::{changes_state, gate_in, Gate};
use crate::redact::{contains_secret, redact};
use crate::sanitize::sanitize_screen_text_with_limit;
use crate::text::{has_money, CURRENCY_WORDS};
use crate::validate::{looks_like_url, validate_action, validate_url};

/// What the agent should do with a proposed action.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// Execute it.
    Allow,
    /// Ask the user with this request; execute only if approved.
    Confirm(ConfirmRequest),
    /// Never execute it; the reason is safe to show the model and the user.
    Deny(String),
}

/// Longest quoted fragment (label, typed text, argument) in a summary, in characters.
const MAX_QUOTE_CHARS: usize = 60;
/// Longest summary, in characters.
const MAX_SUMMARY_CHARS: usize = 200;

/// Whether the action touches or reads the foreground app (and so is blocked inside a
/// denied app). Leaving the app (`PressHome`, `OpenApp`...) stays possible.
fn uses_foreground_app(action: &Action) -> bool {
    matches!(
        action,
        Action::Tap { .. }
            | Action::LongPress { .. }
            | Action::Type { .. }
            | Action::Clear { .. }
            | Action::Scroll { .. }
            | Action::Swipe { .. }
            | Action::TapPoint { .. }
            | Action::Look
    )
}

/// Runs `validate_action`, `check_app`, `classify` and `gate_in` in that order.
///
/// Denies: invalid actions; any action that touches or reads a denied foreground app
/// (`denied_apps` or a `Denied` app rule); `OpenApp` into a denied app; `CallTool` without
/// a matching `tool` spec, with a URL argument whose scheme is not allowed, or whose tool
/// acts on a denied app; any action whose category is in `policy.deny_categories`.
///
/// Otherwise the gate depends on the trust level ([`crate::app_level`]) of the app the
/// action affects (for `CallTool` the tool's app, or `policy.mode` when it has none; else
/// the foreground app): `StepByStep`
/// confirms every action that [`changes_state`] (with category `None` when nothing riskier
/// was found), `Trusted` lets [`crate::TRUSTED_ALLOWED`] categories through, and in every
/// case a category in `policy.confirm` asks, with a one-line sanitised summary such as
/// `Tap "Pay $42.10" in Amazon`.
///
/// First-use approval is not part of this decision; see [`first_use_app`].
pub fn evaluate(
    policy: &Policy,
    action: &Action,
    screen: &CompactScreen,
    tool: Option<&ToolSpec>,
) -> Decision {
    if let Err(e) = validate_action(action, screen) {
        return Decision::Deny(clean(&e.to_string(), MAX_SUMMARY_CHARS));
    }
    if uses_foreground_app(action) {
        if let Verdict::Deny(_) = check_app(policy, &screen.app_bundle_id) {
            return Decision::Deny(format!(
                "{} is on the deny list",
                clean(&screen.app_bundle_id, MAX_QUOTE_CHARS)
            ));
        }
    }
    let mut affected_app = screen.app_bundle_id.clone();
    let mut level = app_level(policy, &screen.app_bundle_id);
    match action {
        Action::OpenApp { app } => {
            if check_app(policy, app).is_denied() {
                return Decision::Deny(format!(
                    "{} is on the deny list",
                    clean(app, MAX_QUOTE_CHARS)
                ));
            }
            affected_app = app.clone();
        }
        Action::CallTool { name, .. } => {
            let mut strings = Vec::new();
            if let Action::CallTool { args, .. } = action {
                crate::classify::tool_strings(args, &mut strings);
            }
            for s in strings.iter().filter(|s| looks_like_url(s)) {
                if let Err(e) = validate_url(s.trim()) {
                    return Decision::Deny(format!(
                        "tool argument: {}",
                        clean(&e.to_string(), MAX_SUMMARY_CHARS)
                    ));
                }
            }
            let Some(spec) = tool.filter(|t| t.name == *name) else {
                return Decision::Deny(format!(
                    "unknown tool \"{}\"",
                    clean(name, MAX_QUOTE_CHARS)
                ));
            };
            match &spec.app {
                Some(app) => {
                    if check_app(policy, app).is_denied() {
                        return Decision::Deny(format!(
                            "tool acts on {}, which is on the deny list",
                            clean(app, MAX_QUOTE_CHARS)
                        ));
                    }
                    affected_app = app.clone();
                    level = app_level(policy, app);
                }
                // A tool that acts in no app is not covered by the foreground app's rule.
                None => level = mode_level(policy),
            }
        }
        _ => {}
    }
    if level == TrustLevel::Denied && !uses_foreground_app(action) {
        // Leaving a denied app stays possible; how it is gated does not depend on it.
        level = app_level(&without_app_denials(policy), &affected_app);
    }
    let category = classify(action, screen, tool);
    match gate_in(policy, category, level, action, screen) {
        Gate::Allow => Decision::Allow,
        Gate::Deny => Decision::Deny(format!("{category:?} actions are not allowed")),
        Gate::Confirm => Decision::Confirm(ConfirmRequest {
            category,
            summary: summarize_with(action, screen, category),
            app: affected_app,
        }),
    }
}

/// `policy` with no app denied, for gating the way out of a denied app.
fn without_app_denials(policy: &Policy) -> Policy {
    let mut p = policy.clone();
    p.denied_apps.clear();
    p.app_rules.retain(|r| r.level != TrustLevel::Denied);
    p
}

/// The app that `action` would act inside and that therefore needs first-use approval
/// under `policy`, before the agent's own bookkeeping (already-approved apps, its own
/// app) is applied.
///
/// `None` when `policy.first_use_approval` is off, when the action does not act inside an
/// app (`PressHome`, `OpenApp`, `OpenUrl`, `Wait`, `Look`, asking, finishing, plain
/// vertical scrolls — see [`changes_state`]), when the app is SpringBoard, and when its
/// trust level is `Trusted` (or `Denied`, which `evaluate` refuses anyway). UI actions act
/// inside the foreground app; a `CallTool` acts inside its spec's `app`, if any.
pub fn first_use_app(
    policy: &Policy,
    action: &Action,
    screen: &CompactScreen,
    tool: Option<&ToolSpec>,
) -> Option<String> {
    if !policy.first_use_approval || !changes_state(action, screen) {
        return None;
    }
    let app = match action {
        Action::CallTool { name, .. } => tool.filter(|t| t.name == *name)?.app.clone()?,
        Action::Tap { .. }
        | Action::LongPress { .. }
        | Action::Type { .. }
        | Action::Clear { .. }
        | Action::Scroll { .. }
        | Action::Swipe { .. }
        | Action::TapPoint { .. } => screen.app_bundle_id.clone(),
        _ => return None,
    };
    if app.trim().is_empty() || is_springboard(&app) {
        return None;
    }
    match app_level(policy, &app) {
        TrustLevel::Trusted | TrustLevel::Denied => None,
        TrustLevel::Guarded | TrustLevel::StepByStep => Some(app),
    }
}

/// The `FirstUse` confirmation for acting in `bundle_id`, e.g.
/// `Let Jarvis act in Shop (com.example.shop)?` (sanitised; the name is left out when
/// unknown or blank).
pub fn first_use_request(bundle_id: &str, app_name: Option<&str>) -> ConfirmRequest {
    let id = clean(bundle_id, MAX_QUOTE_CHARS);
    let summary = match app_name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => format!("Let Jarvis act in {} ({id})?", clean(name, MAX_QUOTE_CHARS)),
        None => format!("Let Jarvis act in {id}?"),
    };
    ConfirmRequest {
        category: RiskCategory::FirstUse,
        summary: sanitize_screen_text_with_limit(&summary, MAX_SUMMARY_CHARS),
        app: bundle_id.to_owned(),
    }
}

/// Redacts, sanitises and caps `s`, and turns `"` into `'` so it can sit inside quotes.
fn clean(s: &str, max: usize) -> String {
    defang(&sanitize_screen_text_with_limit(&redact(s), max).replace('"', "'"))
}

/// Screen text cannot pose as a summary's fact headings: a colon after the word "screen"
/// or "selected" ("Amounts on screen:", "Selected:") is dropped.
fn defang(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == ':' {
            let word: String = out
                .trim_end()
                .chars()
                .rev()
                .take_while(|c| c.is_alphabetic())
                .collect::<Vec<char>>()
                .into_iter()
                .rev()
                .flat_map(char::to_lowercase)
                .collect();
            if word == "screen" || word == "selected" {
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Most amounts kept from the middle of a shortened label.
const MAX_KEPT_AMOUNTS: usize = 3;
/// Longest amount kept from the middle of a shortened label, in characters.
const MAX_AMOUNT_CHARS: usize = 24;

/// Char ranges (start, end) of the amounts in `chars`: words that read as money on their
/// own ("$500", "500€") or with their neighbour ("USD 500", "500 EUR").
pub(crate) fn amount_ranges(chars: &[char]) -> Vec<(usize, usize)> {
    let mut words: Vec<(usize, usize)> = Vec::new();
    let mut start = None;
    for (i, c) in chars.iter().enumerate() {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(i),
            (true, Some(st)) => {
                words.push((st, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(st) = start {
        words.push((st, chars.len()));
    }
    let text =
        |a: usize, b: usize| -> String { chars.get(a..b).unwrap_or_default().iter().collect() };
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (k, &(a, b)) in words.iter().enumerate() {
        let word = text(a, b);
        let range = if has_money(&word) {
            Some((a, b))
        } else if is_number(&word) {
            // "USD 500", "500 EUR": a number next to a currency word.
            let prev = k.checked_sub(1).and_then(|j| words.get(j));
            let next = words.get(k.saturating_add(1));
            prev.filter(|&&(pa, pb)| is_currency_word(&text(pa, pb)))
                .map(|&(pa, _)| (pa, b))
                .or_else(|| {
                    next.filter(|&&(na, nb)| is_currency_word(&text(na, nb)))
                        .map(|&(_, nb)| (a, nb))
                })
        } else {
            None
        };
        if let Some(r) = range {
            match out.last_mut() {
                Some(last) if r.0 <= last.1 => last.1 = last.1.max(r.1),
                _ => out.push(r),
            }
        }
    }
    out
}

/// A number as written in a label: digits with `,` or `.` separators.
fn is_number(w: &str) -> bool {
    w.chars().any(|c| c.is_ascii_digit())
        && w.chars()
            .all(|c| c.is_ascii_digit() || c == ',' || c == '.')
}

/// A currency word ("USD", "euros"), trailing punctuation ignored.
fn is_currency_word(w: &str) -> bool {
    let w = w
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    CURRENCY_WORDS.contains(&w.as_str())
}

/// The value of an amount as written ("$1,500.00" → 1500.0, "1.500,50 €" → 1500.5,
/// "USD 42" → 42.0): its first run of digits and `,`/`.` separators, where the last
/// separator is the decimal point when one or two digits follow it and every other
/// separator groups thousands. `None` when it has no digits.
pub(crate) fn amount_value(text: &str) -> Option<f64> {
    let run: String = text
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.')
        .collect();
    let run = run.trim_end_matches([',', '.']);
    if run.is_empty() {
        return None;
    }
    let (int_part, frac_part) = match run.rfind([',', '.']) {
        Some(i) => {
            let frac = run.get(i.saturating_add(1)..).unwrap_or("");
            if (1..=2).contains(&frac.len()) {
                (run.get(..i).unwrap_or(""), frac)
            } else {
                (run, "")
            }
        }
        None => (run, ""),
    };
    let digits: String = int_part.chars().filter(char::is_ascii_digit).collect();
    let number = if frac_part.is_empty() {
        digits
    } else {
        format!("{digits}.{frac_part}")
    };
    number.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// A label for the owner's summary: redacted and sanitised like [`clean`], and when longer
/// than `max` characters shortened from the middle, keeping its start and its end
/// ("Confirm and pay … $500"), so a trailing amount is never cut off. Of the amounts in the
/// part that is cut, the [`MAX_KEPT_AMOUNTS`] largest (by value; the largest always, equal
/// ones once) are kept in their order (`head … $499.99 … tail`), and the rest are counted
/// ("… and 3 more amounts …"), even if that makes the label a little longer than `max`:
/// decoy amounts cannot hide the real charge.
fn clean_label(s: &str, max: usize) -> String {
    let full = clean(s, crate::sanitize::MAX_SCREEN_TEXT_CHARS);
    let chars: Vec<char> = full.chars().collect();
    if chars.len() <= max {
        return full;
    }
    let amounts = amount_ranges(&chars);
    let kept_len: usize = amounts
        .iter()
        .take(MAX_KEPT_AMOUNTS)
        .map(|(a, b)| (b - a).min(MAX_AMOUNT_CHARS) + 3)
        .sum::<usize>()
        + if amounts.len() > MAX_KEPT_AMOUNTS {
            24
        } else {
            0
        };
    let room = max.saturating_sub(kept_len.saturating_add(3)).max(24);
    let mut head_len = room / 2;
    let mut tail_start = chars.len().saturating_sub(room - head_len);
    // Cut at word boundaries when one is near.
    if let Some(sp) = (head_len / 2..head_len)
        .rev()
        .find(|&i| chars.get(i).is_some_and(|c| c.is_whitespace()))
    {
        head_len = sp;
    }
    if let Some(sp) = (tail_start..tail_start.saturating_add(10).min(chars.len()))
        .find(|&i| chars.get(i).is_some_and(|c| c.is_whitespace()))
    {
        tail_start = sp;
    }
    // Never split an amount: one that straddles a cut is kept whole.
    for &(a, b) in &amounts {
        if a < head_len && b > head_len {
            head_len = a;
        }
        if a < tail_start && b > tail_start {
            tail_start = a;
        }
    }
    let piece = |a: usize, b: usize| -> String {
        chars
            .get(a..b)
            .unwrap_or_default()
            .iter()
            .collect::<String>()
            .trim()
            .to_owned()
    };
    // The amounts in the cut part, largest first (ties: the earlier one).
    let mut middle: Vec<(usize, usize, f64)> = amounts
        .iter()
        .filter(|(a, b)| *a >= head_len && *b <= tail_start)
        .map(|&(a, b)| (a, b, amount_value(&piece(a, b)).unwrap_or(-1.0)))
        .collect();
    let total = middle.len();
    middle.sort_by(|x, y| y.2.total_cmp(&x.2).then(x.0.cmp(&y.0)));
    let mut kept: Vec<(usize, String)> = Vec::new();
    for &(a, b, _) in &middle {
        if kept.len() >= MAX_KEPT_AMOUNTS {
            break;
        }
        let text = piece(a, b.min(a.saturating_add(MAX_AMOUNT_CHARS)));
        if !kept.iter().any(|(_, t)| *t == text) {
            kept.push((a, text));
        }
    }
    let shown = kept.len();
    // Equal amounts shown once still count as shown.
    let hidden = total.saturating_sub(
        middle
            .iter()
            .filter(|&&(a, b, _)| {
                let text = piece(a, b.min(a.saturating_add(MAX_AMOUNT_CHARS)));
                kept.iter().any(|(_, t)| *t == text)
            })
            .count()
            .max(shown),
    );
    kept.sort_by_key(|(a, _)| *a);
    let mut parts = vec![piece(0, head_len)];
    parts.extend(kept.into_iter().map(|(_, t)| t));
    if hidden > 0 {
        parts.push(format!(
            "and {hidden} more amount{}",
            if hidden == 1 { "" } else { "s" }
        ));
    }
    parts.push(piece(tail_start, chars.len()));
    parts.retain(|p| !p.is_empty());
    parts.join(" … ")
}

/// How long the screen-text parts of a summary may be (characters). A summary is built
/// with the first set of limits whose result fits [`MAX_SUMMARY_CHARS`].
#[derive(Clone, Copy)]
struct Limits {
    /// The target's quoted label.
    label: usize,
    /// The row text after "next to".
    next_to: usize,
    /// The app name.
    app: usize,
    /// Each entry of "Amounts on screen" (0: the amount alone).
    amount_entry: usize,
    /// Each entry of "Selected" (0: only the count).
    selected_entry: usize,
}

const LIMITS: [Limits; 4] = [
    Limits {
        label: MAX_QUOTE_CHARS,
        next_to: MAX_NEIGHBOUR_CHARS,
        app: MAX_QUOTE_CHARS,
        amount_entry: 40,
        selected_entry: 40,
    },
    Limits {
        label: 40,
        next_to: 24,
        app: 30,
        amount_entry: 24,
        selected_entry: 24,
    },
    Limits {
        label: 30,
        next_to: 16,
        app: 24,
        amount_entry: 0,
        selected_entry: 16,
    },
    Limits {
        label: 24,
        next_to: 0,
        app: 20,
        amount_entry: 0,
        selected_entry: 0,
    },
];

/// `"label"`, or the role and index when the element has no text.
fn describe(el: &Element) -> String {
    describe_with(el, MAX_QUOTE_CHARS)
}

/// `"label"` (at most `max` characters), or the role and index when it has no text.
fn describe_with(el: &Element, max: usize) -> String {
    let text = [Some(el.label.as_str()), el.placeholder.as_deref()]
        .into_iter()
        .flatten()
        .find(|t| !t.trim().is_empty());
    match text {
        Some(t) => format!("\"{}\"", clean_label(t, max)),
        None => format!("{} #{}", el.role.as_str(), el.index),
    }
}

/// The element description, or `element N` when it is not on the screen.
fn describe_index(screen: &CompactScreen, index: u32) -> String {
    screen
        .get(index)
        .map_or_else(|| format!("element {index}"), describe)
}

/// Longest row text quoted after "next to", in characters.
const MAX_NEIGHBOUR_CHARS: usize = 40;
/// Most amounts listed after "Amounts on screen:"; more are counted ("and 2 more").
pub(crate) const MAX_LISTED_AMOUNTS: usize = 3;
/// Most selected options listed after "Selected:"; more are counted.
pub(crate) const MAX_LISTED_SELECTED: usize = 2;
/// Elements taller than this share of the screen (scroll and web views, side bars) overlap
/// every row and do not label one.
const MAX_ROW_ITEM_HEIGHT: f64 = 0.5;
/// Largest gap (points) between a bare amount and a static text right above it for that
/// text to label it in the summary's facts.
const STACKED_LABEL_GAP: f64 = 8.0;

/// Whether `el` is an on/off switch, and if so whether it is on (`value` "1"/"on"/"true"
/// or "0"/"off"/"false"; any other value is unknown).
pub(crate) fn switch_is_on(el: &Element) -> Option<bool> {
    let switch = match &el.role {
        jarvis_proto::Role::Switch => true,
        jarvis_proto::Role::Other(name) => {
            let n = name.to_lowercase();
            n.contains("switch") || n.contains("toggle")
        }
        _ => false,
    };
    if !switch {
        return None;
    }
    match el
        .value
        .as_deref()
        .map(|v| v.trim().to_lowercase())
        .as_deref()
    {
        Some("1" | "on" | "true") => Some(true),
        Some("0" | "off" | "false") => Some(false),
        _ => None,
    }
}

/// Whether two frames share part of their vertical extent (the same row band).
pub(crate) fn same_row(a: &jarvis_proto::Rect, b: &jarvis_proto::Rect) -> bool {
    a.y < b.max_y() && b.y < a.max_y()
}

/// Whether `e` can label a row: not a screen-high container, not empty.
fn row_sized(e: &Element, screen: &CompactScreen) -> bool {
    let tall = screen.size.height * MAX_ROW_ITEM_HEIGHT;
    !e.frame.is_empty() && !(tall.is_finite() && tall > 0.0 && e.frame.height > tall)
}

/// The static texts in `el`'s row band (see [`same_row`]), in screen order, that say more
/// than `el`'s own label: what tells one "Buy" from another ("Socks", "$5.00").
fn row_texts<'a>(el: &Element, screen: &'a CompactScreen) -> Vec<&'a str> {
    let own = el.label.to_lowercase();
    screen
        .elements
        .iter()
        .filter(|e| e.index != el.index && e.role == jarvis_proto::Role::StaticText)
        .filter(|e| row_sized(e, screen) && same_row(&e.frame, &el.frame))
        .map(|e| e.label.trim())
        .filter(|l| !l.is_empty() && !own.contains(&l.to_lowercase()))
        .collect()
}

/// One amount on the screen, for the summary: the text it is shown with (its element's
/// own text, or, for a bare amount, the static texts of its row before it), and its value.
struct AmountFact {
    text: String,
    value: f64,
}

/// Every element of `screen` that shows an amount (keys, secure fields and the target
/// itself, whose label the summary already quotes, excepted), with the label text of its
/// row, largest first (ties: screen order), each text once. A bare amount ("$5.00") is
/// labelled by the static texts of its row band; with none, by a static text stacked right
/// on top of it (a product name above its price).
fn amount_facts(screen: &CompactScreen, target: Option<&Element>) -> Vec<AmountFact> {
    let mut out: Vec<AmountFact> = Vec::new();
    // The last static text seen, in screen order (a candidate stacked label).
    let mut last_text: Option<&Element> = None;
    for e in screen.elements.iter() {
        let above = if e.role == jarvis_proto::Role::StaticText {
            last_text.replace(e)
        } else {
            last_text
        };
        if e.secure
            || e.role == jarvis_proto::Role::Key
            || target.is_some_and(|t| t.index == e.index)
        {
            continue;
        }
        let own = crate::classify::element_text(e);
        let chars: Vec<char> = own.chars().collect();
        let ranges = amount_ranges(&chars);
        if ranges.is_empty() {
            continue;
        }
        let value = ranges
            .iter()
            .filter_map(|&(a, b)| {
                amount_value(
                    &chars
                        .get(a..b)
                        .unwrap_or_default()
                        .iter()
                        .collect::<String>(),
                )
            })
            .fold(f64::MIN, f64::max);
        // Its words besides the amounts.
        let mut rest = String::new();
        let mut at = 0usize;
        for &(a, b) in &ranges {
            rest.extend(chars.get(at..a).unwrap_or_default());
            at = b;
        }
        rest.extend(chars.get(at..).unwrap_or_default());
        let names_something = rest.chars().any(char::is_alphabetic);
        let text = if names_something || !row_sized(e, screen) {
            own.trim().to_owned()
        } else {
            let mut row: Vec<&str> = screen
                .elements
                .iter()
                .filter(|o| o.index != e.index && o.role == jarvis_proto::Role::StaticText)
                .filter(|o| row_sized(o, screen) && same_row(&o.frame, &e.frame))
                .map(|o| o.label.trim())
                .filter(|l| !l.is_empty() && !has_money(l))
                .collect();
            if row.is_empty() {
                if let Some(a) = above.filter(|a| {
                    !has_money(&a.label)
                        && a.frame.x < e.frame.max_x()
                        && e.frame.x < a.frame.max_x()
                        && row_sized(a, screen)
                        && a.frame.max_y() <= e.frame.y + 1.0
                        && e.frame.y - a.frame.max_y() <= STACKED_LABEL_GAP
                }) {
                    row.push(a.label.trim());
                }
            }
            row.push(own.trim());
            row.join(" ")
        };
        if !out.iter().any(|f| f.text == text) {
            out.push(AmountFact {
                text,
                value: if value.is_finite() { value } else { -1.0 },
            });
        }
    }
    // Stable: equal values keep screen order.
    out.sort_by(|a, b| b.value.total_cmp(&a.value));
    out
}

/// The texts of the selected or checked options on `screen` other than `target` (tabs and
/// keys excepted), each once, in screen order.
fn selected_facts(screen: &CompactScreen, target: Option<&Element>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for e in screen.elements.iter().filter(|e| {
        e.selected
            && !e.secure
            && !matches!(e.role, jarvis_proto::Role::Tab | jarvis_proto::Role::Key)
            && !target.is_some_and(|t| t.index == e.index)
    }) {
        let t = crate::classify::element_text(e).trim().to_owned();
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

/// One fact entry for the summary: cleaned like a label (amounts kept when shortened), with
/// the separators of the summary's own structure (`;`) neutralised.
fn fact_entry(text: &str, max: usize) -> String {
    clean_label(text, max).replace(';', ",")
}

/// The facts appended to an in-app summary: `Amounts on screen: Order total $500.00; You
/// save $5.00` (every distinct amount with the label text of its row, largest first, at
/// most [`MAX_LISTED_AMOUNTS`], then `and N more`) and `Selected: Monthly, $4.99` (at most
/// [`MAX_LISTED_SELECTED`], then `and N more`). Empty parts are left out.
fn facts_suffix(amounts: &[AmountFact], selected: &[String], limits: Limits) -> Vec<String> {
    let mut parts = Vec::new();
    if !amounts.is_empty() {
        let shown: Vec<String> = amounts
            .iter()
            .take(MAX_LISTED_AMOUNTS)
            .map(|f| {
                if limits.amount_entry == 0 {
                    // The amounts alone.
                    let chars: Vec<char> = f.text.chars().collect();
                    let tokens: Vec<String> = amount_ranges(&chars)
                        .iter()
                        .map(|&(a, b)| {
                            let t: String = chars.get(a..b).unwrap_or_default().iter().collect();
                            fact_entry(&t, MAX_AMOUNT_CHARS)
                        })
                        .collect();
                    tokens.join(" ")
                } else {
                    fact_entry(&f.text, limits.amount_entry)
                }
            })
            .collect();
        let more = amounts.len().saturating_sub(MAX_LISTED_AMOUNTS);
        let tail = if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        };
        parts.push(format!("Amounts on screen: {}{tail}", shown.join("; ")));
    }
    if !selected.is_empty() {
        if limits.selected_entry == 0 {
            parts.push(format!("Selected: {} options", selected.len()));
        } else {
            let shown: Vec<String> = selected
                .iter()
                .take(MAX_LISTED_SELECTED)
                .map(|t| fact_entry(t, limits.selected_entry))
                .collect();
            let more = selected.len().saturating_sub(MAX_LISTED_SELECTED);
            let tail = if more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            };
            parts.push(format!("Selected: {}{tail}", shown.join("; ")));
        }
    }
    parts
}

/// The summary of pressing `el` (`verb` "Tap" or "Long-press"): a tap on an on/off switch
/// says what it will do (`Turn on "Wi-Fi" in Settings`); a control whose row has other
/// text names it (`Tap "Buy" next to "Socks $5.00" in Shop`). The screen's amounts and
/// selected options follow as facts (see `facts_suffix`), never one amount picked by
/// distance.
fn press_summary(
    verb: &str,
    el: &Element,
    screen: &CompactScreen,
    in_app: &str,
    l: Limits,
) -> String {
    let what = describe_with(el, l.label);
    if verb == "Tap" {
        if let Some(on) = switch_is_on(el) {
            let turn = if on { "Turn off" } else { "Turn on" };
            return format!("{turn} {what}{in_app}");
        }
    }
    let row = row_texts(el, screen).join(" ");
    let next_to = if row.is_empty() || l.next_to == 0 {
        String::new()
    } else {
        format!(" next to \"{}\"", clean_label(&row, l.next_to))
    };
    format!("{verb} {what}{next_to}{in_app}")
}

/// A one-line, human-readable, sanitised description of `action` for a confirmation
/// prompt, e.g. `Tap "Pay $42.10" in Amazon`. Typed secrets are never shown.
pub fn summarize(action: &Action, screen: &CompactScreen, tool: Option<&ToolSpec>) -> String {
    summarize_with(action, screen, classify(action, screen, tool))
}

fn summarize_with(action: &Action, screen: &CompactScreen, category: RiskCategory) -> String {
    // Actions in the foreground app state the screen's facts after the action itself.
    let target = match action {
        Action::Tap { element } | Action::LongPress { element, .. } | Action::Clear { element } => {
            screen.get(*element)
        }
        Action::Type { element, .. } => type_target(*element, screen),
        Action::Scroll { element, .. } | Action::Swipe { element, .. } => {
            element.and_then(|i| screen.get(i))
        }
        Action::TapPoint { x, y } => crate::classify::tap_point(*x, *y, screen).1,
        _ => None,
    };
    let with_facts = uses_foreground_app(action) && !matches!(action, Action::Look);
    let (amounts, selected) = if with_facts {
        (amount_facts(screen, target), selected_facts(screen, target))
    } else {
        (Vec::new(), Vec::new())
    };
    let mut line = String::new();
    for l in LIMITS {
        let mut parts = vec![action_line(action, screen, category, l)];
        parts.extend(facts_suffix(&amounts, &selected, l));
        line = parts.join(". ");
        if line.chars().count() <= MAX_SUMMARY_CHARS {
            break;
        }
    }
    sanitize_screen_text_with_limit(&line, MAX_SUMMARY_CHARS)
}

/// What `action` does, without the screen's facts.
fn action_line(
    action: &Action,
    screen: &CompactScreen,
    category: RiskCategory,
    l: Limits,
) -> String {
    let app_name = screen
        .app_name
        .as_deref()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or(&screen.app_bundle_id);
    let in_app = format!(" in {}", clean(app_name, l.app));
    match action {
        Action::Tap { element } => match screen.get(*element) {
            Some(el) => press_summary("Tap", el, screen, &in_app, l),
            None => format!("Tap element {element}{in_app}"),
        },
        Action::LongPress { element, .. } => match screen.get(*element) {
            Some(el) => press_summary("Long-press", el, screen, &in_app, l),
            None => format!("Long-press element {element}{in_app}"),
        },
        Action::Type {
            element,
            text,
            submit,
        } => {
            let target = type_target(*element, screen);
            let secret = category == RiskCategory::Credential
                || contains_secret(text)
                || target.is_some_and(|t| t.secure);
            let what = if secret {
                format!("a secret ({} characters)", text.chars().count())
            } else {
                format!("\"{}\"", clean(text, MAX_QUOTE_CHARS))
            };
            let into = target.map_or_else(
                || "the focused field".to_owned(),
                |t| describe_with(t, l.label),
            );
            let ret = if *submit { " and press Return" } else { "" };
            format!("Type {what} into {into}{ret}{in_app}")
        }
        Action::Clear { element } => format!("Clear {}{in_app}", describe_index(screen, *element)),
        Action::Scroll { direction, element } => match element {
            Some(i) => format!(
                "Scroll {direction:?} in {}{in_app}",
                describe_index(screen, *i)
            ),
            None => format!("Scroll {direction:?}{in_app}"),
        },
        Action::Swipe { direction, element } => match element {
            Some(i) => format!(
                "Swipe {direction:?} on {}{in_app}",
                describe_index(screen, *i)
            ),
            None => format!("Swipe {direction:?}{in_app}"),
        },
        Action::TapPoint { x, y } => {
            // The element that decided the risk: the riskiest pressable one under the
            // point, never a decorative text inside it.
            let on = crate::classify::tap_point(*x, *y, screen)
                .1
                .map(|e| format!(" on {}", describe_with(e, l.label)))
                .unwrap_or_default();
            format!("Tap at ({:.0}%, {:.0}%){on}{in_app}", x * 100.0, y * 100.0)
        }
        Action::PressHome => "Go to the Home Screen".to_owned(),
        Action::OpenApp { app } => format!("Open {}", clean(app, MAX_QUOTE_CHARS)),
        Action::OpenUrl { url } => format!("Open {}", clean(url, 120)),
        Action::Wait { ms } => format!("Wait {ms} ms"),
        Action::CallTool { name, args } => {
            let mut parts = Vec::new();
            if let Some(map) = args.as_object() {
                for (k, v) in map {
                    let v = match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    parts.push(format!("{}: {}", clean(k, 30), clean(&v, 40)));
                }
            }
            if parts.is_empty() {
                format!("Run {}", clean(name, MAX_QUOTE_CHARS))
            } else {
                format!(
                    "Run {} ({})",
                    clean(name, MAX_QUOTE_CHARS),
                    parts.join(", ")
                )
            }
        }
        Action::AskUser { question } => format!("Ask: {}", clean(question, 120)),
        Action::Look => format!("Take a screenshot{in_app}"),
        Action::Done { summary } => format!("Finish: {}", clean(summary, 120)),
        Action::Fail { reason } => format!("Give up: {}", clean(reason, 120)),
    }
}
