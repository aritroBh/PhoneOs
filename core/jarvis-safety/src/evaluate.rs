//! The single entry point the agent calls before executing an action.

use jarvis_proto::{
    Action, BarNode, CompactScreen, ConfirmRequest, Element, Policy, RawNode, RiskCategory, Role,
    ToolSpec, TrustLevel,
};

use crate::app::{app_level, check_app, is_springboard, mode_level, Verdict};
use crate::classify::{classify, type_target};
use crate::gate::{changes_state, gate_on, Gate};
use crate::redact::{contains_secret, credential_like_field, redact, redact_screen};
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

/// Where the screen an action is judged on came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScreenSource {
    /// A typed accessibility tree (Jarvis's own runner): the supported path for acting.
    #[default]
    Typed,
    /// The third-party runner's text tree: ambiguous and forgeable by on-screen text (see
    /// `jarvis_observe::parse_runner_text`). Every state-changing runner action on it is
    /// at least [`RiskCategory::Unverified`] and its summary starts with
    /// [`UNVERIFIED_SCREEN_WARNING`].
    RunnerText,
}

/// The fixed start of every confirmation summary of an action judged on a screen read from
/// runner text ([`ScreenSource::RunnerText`]).
pub const UNVERIFIED_SCREEN_WARNING: &str =
    "Warning: this screen was read through an unverified text format and may not be what is on the phone. ";

/// Where each compact element comes from in the raw tree (from
/// `jarvis_observe::CompactView`), for summaries that prefer the target's own container.
#[derive(Debug, Clone, Copy)]
pub struct ScreenTree<'a> {
    /// Per compact element (by position), its raw node's index.
    pub sources: &'a [u32],
    /// Per raw node, its parent's index (`u32::MAX` for the root).
    pub parents: &'a [u32],
    /// Per raw node, one past its last descendant's index.
    pub ends: &'a [u32],
    /// The raw navigation and tab bars with their shown frames
    /// (`jarvis_observe::CompactView::bars`), for the tap-point rule's bar precedence
    /// ([`crate::tap_spot`]). Only from a typed tree: the session passes none for a screen
    /// read from runner text, whose roles anyone can write (`&[]` disables precedence).
    pub bars: &'a [BarNode],
    /// Every drawn raw node with its on-screen frame (`jarvis_observe::CompactView::nodes`):
    /// what could take a touch at a point ([`crate::tap_spot`]). `&[]` (a hand-built
    /// screen) falls back to the listed controls.
    pub nodes: &'a [RawNode],
}

impl ScreenTree<'_> {
    pub(crate) fn source(&self, el: &Element) -> Option<u32> {
        let pos = usize::try_from(el.index).ok()?.checked_sub(1)?;
        self.sources.get(pos).copied()
    }

    /// The raw ancestors of `el`'s node, innermost first, the root excepted (it holds
    /// everything, so it is no one's container).
    fn ancestors(&self, el: &Element) -> Vec<u32> {
        let mut out = Vec::new();
        let mut at = self.source(el).and_then(|s| self.parent_of(s));
        while let Some(p) = at {
            let up = self.parent_of(p);
            if up.is_none() || out.len() > self.parents.len() {
                break;
            }
            out.push(p);
            at = up;
        }
        out
    }

    fn parent_of(&self, i: u32) -> Option<u32> {
        usize::try_from(i)
            .ok()
            .and_then(|i| self.parents.get(i))
            .copied()
            .filter(|p| *p != u32::MAX)
    }

    /// Whether raw node `n` is inside raw node `container`'s subtree (not itself).
    pub(crate) fn node_inside(&self, n: u32, container: u32) -> bool {
        let end = usize::try_from(container)
            .ok()
            .and_then(|i| self.ends.get(i))
            .copied()
            .unwrap_or(0);
        n > container && n < end
    }

    /// Whether raw nodes `a` and `b` are the same or one holds the other.
    pub(crate) fn related_nodes(&self, a: u32, b: u32) -> bool {
        a == b || self.node_inside(a, b) || self.node_inside(b, a)
    }

    /// The compact index of the element listed from raw node `n`, if one was.
    pub(crate) fn element_of(&self, n: u32) -> Option<u32> {
        let pos = self.sources.iter().position(|s| *s == n)?;
        u32::try_from(pos.saturating_add(1)).ok()
    }

    /// Whether `el`'s node is inside raw node `container`'s subtree.
    pub(crate) fn inside(&self, el: &Element, container: u32) -> bool {
        let end = usize::try_from(container)
            .ok()
            .and_then(|i| self.ends.get(i))
            .copied()
            .unwrap_or(0);
        self.source(el).is_some_and(|s| s > container && s < end)
    }
}

/// Text the session itself typed into a field during the task: the field's fingerprint and
/// the text as decided (profile placeholders kept, the way the profile-masked screen the
/// summary is built from shows it). A field's value appears in a summary only when it is
/// exactly what the session typed there, or an amount in an amount-like field (see
/// [`summary_with`]).
#[derive(Clone, PartialEq, Eq)]
pub struct TypedText {
    /// Fingerprint of the field the text went into.
    pub field: u64,
    /// The text (placeholders kept).
    pub text: String,
}

impl std::fmt::Debug for TypedText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypedText")
            .field("field", &self.field)
            .field("text_chars", &self.text.chars().count())
            .finish()
    }
}

/// Longest quoted fragment (label, typed text, argument) in a summary, in characters.
const MAX_QUOTE_CHARS: usize = 60;
/// Longest summary, in characters (seventh review: 480, was 320, so the content a send
/// sends fits; stays under the remote hub's 500).
const MAX_SUMMARY_CHARS: usize = 480;

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
    evaluate_from(policy, action, screen, tool, ScreenSource::Typed)
}

/// The category of `action` on a screen from `source`: [`classify`]'s, raised to
/// [`RiskCategory::Unverified`] (from `None`) when the screen came from runner text and the
/// action is one the runner carries out that [`changes_state`] (everything but waits,
/// reads, asking, finishing and plain vertical scrolls; tool calls do not go to the
/// runner).
pub fn classify_from(
    action: &Action,
    screen: &CompactScreen,
    tool: Option<&ToolSpec>,
    source: ScreenSource,
) -> RiskCategory {
    let category = classify(action, screen, tool);
    let runner_action = !matches!(action, Action::CallTool { .. });
    if source == ScreenSource::RunnerText
        && category == RiskCategory::None
        && runner_action
        && changes_state(action, screen)
    {
        RiskCategory::Unverified
    } else {
        category
    }
}

/// [`evaluate`] for a screen from `source`: on a screen read from runner text every
/// state-changing runner action is at least `Unverified` ([`classify_from`]) and **always
/// asks**, whatever the policy mode, the app's trust level or the categories the owner
/// relaxed (a source-level rule in the gate, [`crate::gate_on`]); its summary starts with
/// [`UNVERIFIED_SCREEN_WARNING`]. Deny-list and category denials still refuse first.
pub fn evaluate_from(
    policy: &Policy,
    action: &Action,
    screen: &CompactScreen,
    tool: Option<&ToolSpec>,
    source: ScreenSource,
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
    let category = classify_from(action, screen, tool, source);
    let runner_text =
        source == ScreenSource::RunnerText && !matches!(action, Action::CallTool { .. });
    match gate_on(policy, category, level, action, screen, runner_text) {
        Gate::Allow => Decision::Allow,
        Gate::Deny => Decision::Deny(format!("{category:?} actions are not allowed")),
        Gate::Confirm => Decision::Confirm(ConfirmRequest {
            category,
            summary: summary_for(action, screen, category, source, None),
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

/// Screen text cannot pose as a summary's fact headings: a colon after the word "screen",
/// "selected", "account" or "fields" ("Amounts on screen:", "Selected:", "To/Account:",
/// "Fields:") is dropped.
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
            if matches!(word.as_str(), "screen" | "selected" | "account" | "fields") {
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
/// with the first set of limits whose result fits [`MAX_SUMMARY_CHARS`]. Amounts shrink
/// last: every other text is shortened (and then dropped) first.
#[derive(Clone, Copy)]
struct Limits {
    /// The target's quoted label.
    label: usize,
    /// The row text after "next to".
    next_to: usize,
    /// The item text after "under" (0: left out).
    under: usize,
    /// The app name.
    app: usize,
    /// Each entry of "Amounts on screen" (0: the amount alone).
    amount_entry: usize,
    /// Each entry of "To/Account" (0: left out).
    recipient_entry: usize,
    /// Each entry of "Selected" (0: only the count).
    selected_entry: usize,
    /// Each entry of "Fields" (0: left out).
    field_entry: usize,
}

const LIMITS: [Limits; 5] = [
    Limits {
        label: MAX_QUOTE_CHARS,
        next_to: MAX_NEIGHBOUR_CHARS,
        under: MAX_NEIGHBOUR_CHARS,
        app: MAX_QUOTE_CHARS,
        amount_entry: 40,
        recipient_entry: 40,
        selected_entry: 40,
        field_entry: 40,
    },
    Limits {
        label: 40,
        next_to: 24,
        under: 30,
        app: 30,
        amount_entry: 30,
        recipient_entry: 30,
        selected_entry: 24,
        field_entry: 24,
    },
    Limits {
        label: 30,
        next_to: 16,
        under: 24,
        app: 24,
        amount_entry: 24,
        recipient_entry: 24,
        selected_entry: 16,
        field_entry: 0,
    },
    Limits {
        label: 30,
        next_to: 16,
        under: 20,
        app: 24,
        amount_entry: 0,
        recipient_entry: 20,
        selected_entry: 16,
        field_entry: 0,
    },
    Limits {
        label: 24,
        next_to: 0,
        under: 0,
        app: 20,
        amount_entry: 0,
        recipient_entry: 0,
        selected_entry: 0,
        field_entry: 0,
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

/// A role in plain words for messages ("button", "text", "text field"; another role's
/// name cleaned and bounded).
pub(crate) fn role_words(role: &Role) -> String {
    match role {
        Role::StaticText => "text".to_owned(),
        Role::TextField | Role::TextView => "text field".to_owned(),
        Role::SecureTextField => "password field".to_owned(),
        Role::SearchField => "search field".to_owned(),
        Role::Cell => "row".to_owned(),
        Role::Other(name) => clean(name, 24).to_lowercase(),
        other => other.as_str().replace('_', " "),
    }
}

/// Screen text for a quote in a message: cleaned like a summary label (redacted,
/// sanitised, `"` → `'`, shortened from the middle keeping amounts).
pub(crate) fn clean_quote(s: &str, max: usize) -> String {
    clean_label(s, max)
}

/// Longest row text quoted after "next to", in characters.
const MAX_NEIGHBOUR_CHARS: usize = 40;
/// Most amounts listed after "Amounts on screen:"; more are counted ("and 2 more").
pub(crate) const MAX_LISTED_AMOUNTS: usize = 5;
/// Most selected options listed after "Selected:"; more are counted.
pub(crate) const MAX_LISTED_SELECTED: usize = 3;
/// Most recipient or account texts listed after "To/Account:"; more are counted.
pub(crate) const MAX_LISTED_RECIPIENTS: usize = 3;
/// Most field values listed after "Fields:"; more are counted.
pub(crate) const MAX_LISTED_FIELDS: usize = 2;
/// Most static texts that name the target's item after "under".
const MAX_ITEM_TEXTS: usize = 2;
/// How far above the target (points from its top to a text's bottom) a text may sit to
/// name its item (a card's title stacked above its button).
const ITEM_REACH: f64 = 160.0;
/// Elements taller than this share of the screen (scroll and web views, side bars) overlap
/// every row and do not label one.
const MAX_ROW_ITEM_HEIGHT: f64 = 0.5;
/// Largest gap (points) between a bare amount and a static text right above it for that
/// text to label it in the summary's facts.
const STACKED_LABEL_GAP: f64 = 8.0;
/// Field labels (or placeholders) whose bare number is an amount ("Amount: 500").
const AMOUNT_FIELD_WORDS: &[&str] = &["amount", "total", "price", "tip", "quantity", "qty"];
/// Word starts that mark an amount as the charge (listed first): total, due, pay,
/// charge, billed, renews.
const CHARGE_WORD_STARTS: &[&str] = &["total", "due", "pay", "charg", "bill", "subtotal", "renew"];
/// Words after "per" that make a recurring charge ("per month").
const PER_PERIOD_WORDS: &[&str] = &["year", "yr", "annum", "month", "mo", "week", "wk", "day"];
/// Period suffixes written after a slash ("$9.99/mo").
const SLASH_PERIODS: &[&str] = &[
    "/yr", "/year", "/mo", "/month", "/week", "/wk", "/day", "/ year", "/ mo", "/ month",
];

/// Whether `el` is an on/off switch, and if so whether it is on (`value` "1"/"on"/"true"
/// or "0"/"off"/"false"; any other value is unknown).
pub(crate) fn switch_is_on(el: &Element) -> Option<bool> {
    if !is_switch_like(&el.role) {
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

/// Switches, toggles and checkboxes (an option that is on or off by itself, not one of a
/// group).
fn is_switch_like(role: &Role) -> bool {
    match role {
        Role::Switch => true,
        Role::Other(name) => {
            let n = name.to_lowercase();
            n.contains("switch") || n.contains("toggle") || n.contains("checkbox")
        }
        _ => false,
    }
}

/// Controls whose value the owner sets and that are not on/off switches: text inputs,
/// sliders, steppers, pickers, segmented controls.
fn is_value_field(el: &Element) -> bool {
    if el.secure || el.role == Role::SecureTextField {
        return false;
    }
    is_value_role(&el.role)
}

/// Whether a field of `role` with name `label` / `placeholder` may have `value` quoted in a
/// note for the owner without knowing what the session typed: any non-field element, or an
/// amount-like field holding an amount or a plain number (see [`summary_with`]).
pub(crate) fn value_quotable(
    role: &Role,
    label: &str,
    placeholder: Option<&str>,
    value: &str,
) -> bool {
    !(is_value_role(role) || role.is_text_input())
        || (amount_like_name(label, placeholder) && money_or_number(value))
}

/// Roles of [`is_value_field`].
fn is_value_role(role: &Role) -> bool {
    match role {
        Role::Slider | Role::Picker => true,
        Role::Other(name) => {
            let n = name.to_lowercase();
            ["stepper", "slider", "picker", "segmented"]
                .iter()
                .any(|k| n.contains(k))
        }
        r => r.is_text_input(),
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
        .filter(|e| e.index != el.index && e.role == Role::StaticText)
        .filter(|e| row_sized(e, screen) && same_row(&e.frame, &el.frame))
        .map(|e| e.label.trim())
        .filter(|l| !l.is_empty() && !own.contains(&l.to_lowercase()))
        .collect()
}

/// The static texts that name `el`'s item when they are stacked above it (a card: "Order
/// #1001" / "Placed Nov 20" / [Delete order]): the texts between the previous control and
/// `el` in screen order that sit above it (bottom within [`ITEM_REACH`] of its top),
/// overlap it horizontally, are not in its row band and show no amount (amounts are
/// listed as facts); the first [`MAX_ITEM_TEXTS`].
///
/// When more than one text qualifies and the raw tree is known (`tree`), texts in the
/// target's own container are preferred over mere proximity: the innermost raw ancestor of
/// the target that holds any of them decides, and only the texts inside it are kept.
fn item_texts<'a>(
    el: &Element,
    screen: &'a CompactScreen,
    tree: Option<&ScreenTree<'_>>,
) -> Vec<&'a str> {
    let Some(pos) = screen.elements.iter().position(|e| e.index == el.index) else {
        return Vec::new();
    };
    let mut found: Vec<&'a Element> = Vec::new();
    for e in screen.elements.iter().take(pos).rev() {
        if e.role.is_interactive() {
            break;
        }
        let f = e.frame;
        let above = f.max_y() <= el.frame.y + 1.0 && el.frame.y - f.max_y() <= ITEM_REACH;
        let overlaps = f.x < el.frame.max_x() && el.frame.x < f.max_x();
        if e.role == Role::StaticText
            && row_sized(e, screen)
            && above
            && overlaps
            && !same_row(&f, &el.frame)
            && !e.label.trim().is_empty()
            && !has_money(&e.label)
        {
            found.push(e);
        }
    }
    found.reverse();
    if let Some(tree) = tree.filter(|_| found.len() > 1) {
        let own = tree
            .ancestors(el)
            .into_iter()
            .find(|a| found.iter().any(|e| tree.inside(e, *a)));
        if let Some(a) = own {
            found.retain(|e| tree.inside(e, a));
        }
    }
    found.truncate(MAX_ITEM_TEXTS);
    found.into_iter().map(|e| e.label.trim()).collect()
}

/// One amount on the screen, for the summary: the text it is shown with (its element's
/// own text, a field's name and value, or, for a bare amount, the static texts of its
/// row), its value, and whether it reads as the charge.
struct AmountFact {
    text: String,
    value: f64,
    charge: bool,
}

/// Whether `text` has charge wording: a word starting with total, due, pay, charge, bill
/// or renew; "then"; "per year / month / week" (and `/yr`, `/mo`...); "after (the / free)
/// trial".
fn has_charge_words(text: &str) -> bool {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let starts = words
        .iter()
        .any(|w| CHARGE_WORD_STARTS.iter().any(|s| w.starts_with(s)) || *w == "then");
    let per = words
        .windows(2)
        .any(|p| matches!(p, [a, b] if *a == "per" && PER_PERIOD_WORDS.contains(b)));
    let slash = SLASH_PERIODS.iter().any(|s| lower.contains(s));
    let trial = words.windows(2).any(|p| matches!(p, ["after", "trial"]))
        || words
            .windows(3)
            .any(|p| matches!(p, ["after", "the" | "free" | "your", "trial"]));
    starts || per || slash || trial
}

/// Whether `field`'s name (label or placeholder) is amount-like.
fn amount_like_field(el: &Element) -> bool {
    amount_like_name(&el.label, el.placeholder.as_deref())
}

/// Whether a name (label or placeholder) is amount-like.
fn amount_like_name(label: &str, placeholder: Option<&str>) -> bool {
    let name = format!("{label} {}", placeholder.unwrap_or_default()).to_lowercase();
    name.split(|c: char| !c.is_alphanumeric())
        .any(|w| AMOUNT_FIELD_WORDS.contains(&w))
}

/// A field's name for its fact: its label, else its placeholder.
fn field_name(el: &Element) -> String {
    let label = el.label.trim();
    if label.is_empty() {
        el.placeholder
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_owned()
    } else {
        label.to_owned()
    }
}

/// A field's value worth stating: set, not blank, not its placeholder.
fn field_value(el: &Element) -> Option<&str> {
    el.value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty() && Some(*v) != el.placeholder.as_deref().map(str::trim))
}

/// A lone currency symbol or code shown as its own element ("$", "€", "USD"): no digits,
/// and money once a number is put next to it.
fn lone_currency(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty()
        && t.chars().count() <= 4
        && !t.chars().any(|c| c.is_ascii_digit())
        && (has_money(&format!("{t}5")) || has_money(&format!("5 {t}")))
}

/// The largest amount value among `ranges` of `chars`.
fn max_amount(chars: &[char], ranges: &[(usize, usize)]) -> f64 {
    ranges
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
        .fold(f64::MIN, f64::max)
}

/// Every amount on `screen` (keys, secure fields and the target itself, whose label the
/// summary already quotes, excepted), each text once:
///
/// - an element that shows an amount, with the label text of its row: its own words, else
///   the static texts of its row band, else a static text stacked right on top of it;
/// - an amount split over two elements of one row ("$" and "25") is joined ("$25");
/// - a value field (text input, slider, stepper, picker) whose name is amount-like (amount,
///   total, price, tip, quantity, qty) and whose value is a money amount or a plain number,
///   as `Name: value` (sixth review: no other field's value is listed here, see
///   `summary_with`).
///
/// Order: the charge first (field amounts, and texts with charge wording, see
/// `has_charge_words`), then the rest; largest first within each (ties: screen order). The
/// facts list shows the first [`MAX_LISTED_AMOUNTS`] and always the largest amount on the
/// screen, whatever its wording (see `facts_suffix`).
fn amount_facts(screen: &CompactScreen, target: Option<&Element>) -> Vec<AmountFact> {
    let mut out: Vec<AmountFact> = Vec::new();
    let push = |out: &mut Vec<AmountFact>, text: String, value: f64, charge: bool| {
        if !out.iter().any(|f| f.text == text) {
            out.push(AmountFact {
                text,
                value: if value.is_finite() { value } else { -1.0 },
                charge,
            });
        }
    };
    // Elements joined into a split amount (the number after or before a lone symbol).
    let mut joined: Vec<u32> = Vec::new();
    // The last static text seen, in screen order (a candidate stacked label).
    let mut last_text: Option<&Element> = None;
    let els = &screen.elements;
    for (k, e) in els.iter().enumerate() {
        let above = if e.role == Role::StaticText {
            last_text.replace(e)
        } else {
            last_text
        };
        if e.secure
            || e.role == Role::Key
            || credential_like_field(e)
            || target.is_some_and(|t| t.index == e.index)
            || joined.contains(&e.index)
        {
            continue;
        }
        if is_value_field(e) {
            // Only an amount-like field holding an amount or a plain number shows its
            // value here (sixth review; see `summary_with`).
            if let Some(v) = field_value(e).filter(|v| amount_like_field(e) && money_or_number(v)) {
                let chars: Vec<char> = v.chars().collect();
                let ranges = amount_ranges(&chars);
                let value = if ranges.is_empty() {
                    amount_value(v).unwrap_or(-1.0)
                } else {
                    max_amount(&chars, &ranges)
                };
                let name = field_name(e);
                let text = if name.is_empty() {
                    v.to_owned()
                } else {
                    format!("{name}: {v}")
                };
                push(&mut out, text, value, true);
            }
            continue;
        }
        let mut own = crate::classify::element_text(e);
        // "$" + "25" (or "25" + "€") as two elements of one row: one amount.
        if e.role == Role::StaticText && lone_currency(&e.label) {
            let next = els.get(k.saturating_add(1)).filter(|n| {
                n.role == Role::StaticText
                    && is_number(n.label.trim())
                    && same_row(&n.frame, &e.frame)
                    && !target.is_some_and(|t| t.index == n.index)
            });
            match next {
                Some(n) => {
                    let sym = e.label.trim();
                    let sep = if sym.chars().all(char::is_alphabetic) {
                        " "
                    } else {
                        ""
                    };
                    own = format!("{sym}{sep}{}", n.label.trim());
                    joined.push(n.index);
                }
                None => continue,
            }
        } else if e.role == Role::StaticText && is_number(e.label.trim()) {
            if let Some(n) = els.get(k.saturating_add(1)).filter(|n| {
                n.role == Role::StaticText
                    && lone_currency(&n.label)
                    && same_row(&n.frame, &e.frame)
                    && !target.is_some_and(|t| t.index == n.index)
            }) {
                let sym = n.label.trim();
                let sep = if sym.chars().all(char::is_alphabetic) {
                    " "
                } else {
                    ""
                };
                own = format!("{}{sep}{sym}", e.label.trim());
                joined.push(n.index);
            }
        }
        let chars: Vec<char> = own.chars().collect();
        let ranges = amount_ranges(&chars);
        if ranges.is_empty() {
            continue;
        }
        let value = max_amount(&chars, &ranges);
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
            let mut row: Vec<&str> = els
                .iter()
                .filter(|o| {
                    o.index != e.index && o.role == Role::StaticText && !joined.contains(&o.index)
                })
                .filter(|o| row_sized(o, screen) && same_row(&o.frame, &e.frame))
                .map(|o| o.label.trim())
                .filter(|l| !l.is_empty() && !has_money(l) && !lone_currency(l) && !is_number(l))
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
        let charge = has_charge_words(&text);
        push(&mut out, text, value, charge);
    }
    // Stable: equal keys keep screen order.
    out.sort_by(|a, b| b.charge.cmp(&a.charge).then(b.value.total_cmp(&a.value)));
    out
}

/// The texts of the selected or checked options on `screen` other than `target` (tabs and
/// keys excepted), each once, ranked: options that carry an amount or sit in the row band
/// of an amount or of the target first; then options of a group (radio-like: buttons,
/// cells, segments) before switches and checkboxes; then screen order.
fn selected_facts(screen: &CompactScreen, target: Option<&Element>) -> Vec<String> {
    let amount_rows: Vec<jarvis_proto::Rect> = screen
        .elements
        .iter()
        .filter(|e| !e.secure && e.role != Role::Key && !e.frame.is_empty())
        .filter(|e| {
            let t: Vec<char> = fact_text(e).chars().collect();
            !amount_ranges(&t).is_empty()
        })
        .map(|e| e.frame)
        .collect();
    let mut ranked: Vec<(u8, u8, String)> = Vec::new();
    for e in screen.elements.iter().filter(|e| {
        e.selected
            && !e.secure
            && !matches!(e.role, Role::Tab | Role::Key)
            && !target.is_some_and(|t| t.index == e.index)
    }) {
        let t = fact_text(e).trim().to_owned();
        if t.is_empty() || ranked.iter().any(|(_, _, x)| *x == t) {
            continue;
        }
        let chars: Vec<char> = t.chars().collect();
        let near_amount = !amount_ranges(&chars).is_empty()
            || (!e.frame.is_empty()
                && row_sized(e, screen)
                && (amount_rows.iter().any(|r| same_row(r, &e.frame))
                    || target.is_some_and(|tg| same_row(&tg.frame, &e.frame))));
        let switch = is_switch_like(&e.role);
        ranked.push((u8::from(!near_amount), u8::from(switch), t));
    }
    // Stable: equal ranks keep screen order.
    ranked.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    ranked.into_iter().map(|(_, _, t)| t).collect()
}

/// Least count of digits in a plain digit run (no separators) that is never read as an
/// amount: an account, card or reference number, not a sum (seventh review).
const MIN_PLAIN_DIGIT_RUN: usize = 9;

/// Whether `v`, an amount-like field's value, is a money amount or a plain number and
/// nothing else (seventh review: grouped and locale formats too): an optional sign, a
/// currency symbol or code before or after, a number grouped with `,` `.` `'` or spaces
/// ("4.000,00 €", "50 000", "-50", "1,250.5"), and an optional period ("40.00 USD /
/// month", "$9.99/mo", "per month"). A plain run of [`MIN_PLAIN_DIGIT_RUN`] or more digits
/// is not an amount (it may be an account number), nor are digit groups of a card number
/// ("4111 1111 1111 1111").
fn money_or_number(v: &str) -> bool {
    let t = v.trim();
    let t = t
        .strip_prefix(['-', '+', '\u{2212}'])
        .unwrap_or(t)
        .trim_start();
    if t.is_empty() {
        return false;
    }
    let lower = t.to_lowercase();
    // A trailing period: "/ month", "/mo", "per month", "a month".
    let body = match lower.rsplit_once('/') {
        Some((head, period)) if PER_PERIOD_WORDS.contains(&period.trim()) => head.to_owned(),
        Some(_) => return false,
        None => {
            let words: Vec<&str> = lower.split_whitespace().collect();
            match words.as_slice() {
                [head @ .., "per" | "a", p] if PER_PERIOD_WORDS.contains(p) => head.join(" "),
                _ => lower.clone(),
            }
        }
    };
    // Currency symbols and codes before or after the number.
    let mut groups: Vec<String> = Vec::new();
    for w in body.split_whitespace() {
        let core = w.trim_matches(|c: char| crate::text::is_currency_symbol(c));
        if core.is_empty() || is_currency_word(core) {
            continue;
        }
        groups.push(core.to_owned());
    }
    let Some((first, rest)) = groups.split_first() else {
        return false;
    };
    let grouped = |g: &str| {
        g.chars()
            .all(|c| c.is_ascii_digit() || matches!(c, ',' | '.' | '\''))
    };
    if !grouped(first) || !first.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    // Space-grouped thousands: every later group is three digits (a decimal tail allowed
    // on the last one).
    let n = rest.len();
    for (k, g) in rest.iter().enumerate() {
        let digits: String = g.chars().take_while(char::is_ascii_digit).collect();
        let tail = g.get(digits.len()..).unwrap_or("");
        let tail_ok = tail.is_empty()
            || (k + 1 == n
                && matches!(tail.chars().next(), Some(',' | '.'))
                && tail.chars().skip(1).all(|c| c.is_ascii_digit()));
        if digits.len() != 3 || !tail_ok {
            return false;
        }
    }
    if !rest.is_empty() && first.chars().filter(char::is_ascii_digit).count() > 3 {
        return false;
    }
    let plain = groups.len() == 1 && first.chars().all(|c| c.is_ascii_digit());
    !(plain && first.len() >= MIN_PLAIN_DIGIT_RUN)
}

/// Whether the value of `el` (from the profile-masked screen) is exactly what the session
/// typed into that field during the task (surrounding whitespace ignored).
fn typed_here(el: &Element, typed: &[TypedText]) -> bool {
    let Some(v) = el.value.as_deref().map(str::trim).filter(|v| !v.is_empty()) else {
        return false;
    };
    typed
        .iter()
        .any(|t| t.field == el.fingerprint && t.text.trim() == v)
}

/// The value fields on `screen` (text inputs, sliders, steppers, pickers; never secure
/// ones, nor the target) other than amount-like fields holding an amount (listed with the
/// amounts), by name: `Name: value` only when the value is exactly what the session typed
/// there (`typed`, compared on `raw`, the profile-masked screen before redaction; the value
/// shown is the redacted one), else `Name (filled)` or `Name (empty)`. A field named as a
/// credential or a bank or card detail ([`credential_like_field`]) that holds a value is
/// `Name (hidden)`. A field without a name is "A field".
fn field_facts(
    screen: &CompactScreen,
    raw: &CompactScreen,
    target: Option<&Element>,
    typed: &[TypedText],
    sent: &[&Element],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for e in screen.elements.iter().filter(|e| {
        is_value_field(e)
            && !target.is_some_and(|t| t.index == e.index)
            && !sent.iter().any(|s| s.index == e.index)
    }) {
        let name = field_name(e);
        let name = if name.is_empty() {
            "A field".to_owned()
        } else {
            name
        };
        let value = field_value(e);
        let t = match value {
            Some(_) if credential_like_field(e) => format!("{name} (hidden)"),
            Some(v) if amount_like_field(e) && money_or_number(v) => continue,
            Some(v) if raw.get(e.index).is_some_and(|r| typed_here(r, typed)) => {
                format!("{name}: {v}")
            }
            Some(_) => format!("{name} (filled)"),
            None => format!("{name} (empty)"),
        };
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

/// The text a fact may quote for `el`: its label alone for a value field (whose value is
/// shown only under the field rule, see [`field_facts`]), else [`element_text`].
///
/// [`element_text`]: crate::classify::element_text
fn fact_text(el: &Element) -> String {
    if is_value_field(el) || el.role.is_text_input() {
        el.label.clone()
    } else {
        crate::classify::element_text(el)
    }
}

/// Whether `word` looks like who is paid or messaged: a phone number (7 to 15 digits with
/// `+ ( ) - .` and spaces), a masked account or card suffix (`••••4521`, `****4521`,
/// `x4521`), an email address or an `@handle`.
fn recipient_word(word: &str) -> bool {
    let w = word.trim_matches(|c: char| matches!(c, ',' | ';' | '(' | ')' | '.' | ':'));
    if w.is_empty() {
        return false;
    }
    // Email address or @handle.
    if let Some(at) = w.find('@') {
        let (user, domain) = (w.get(..at).unwrap_or(""), w.get(at + 1..).unwrap_or(""));
        let handle = user.is_empty()
            && domain.chars().count() >= 2
            && domain
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '-'));
        let email = !user.is_empty() && domain.contains('.') && !domain.starts_with('.');
        return handle || email;
    }
    // Masked suffix: mask characters followed by 2 to 6 digits.
    let masks = |c: char| matches!(c, '•' | '*' | '·' | '●' | '∙' | 'x' | 'X' | '…');
    let digits_after_mask = w.trim_start_matches(masks);
    digits_after_mask.len() < w.len()
        && (2..=6).contains(&digits_after_mask.chars().count())
        && digits_after_mask.chars().all(|c| c.is_ascii_digit())
}

/// Whether `text` contains a phone number: a run of 7 to 15 digits written with only
/// `+ ( ) - .` and single spaces between them.
fn has_phone_number(text: &str) -> bool {
    let mut digits = 0usize;
    let mut prev_space = false;
    for c in text.chars().chain(std::iter::once('|')) {
        if c.is_ascii_digit() {
            digits += 1;
            prev_space = false;
        } else if matches!(c, '+' | '(' | ')' | '-' | '.')
            || (c == ' ' && !prev_space && digits > 0)
        {
            prev_space = c == ' ';
        } else {
            if (7..=15).contains(&digits) {
                return true;
            }
            digits = 0;
            prev_space = false;
        }
    }
    false
}

/// Words that say who is paid or messaged ("To", "Pay to", "Recipient", "Send to",
/// "Account").
const RECIPIENT_WORDS: &[&str] = &[
    "to",
    "recipient",
    "recipients",
    "payee",
    "beneficiary",
    "account",
    "acct",
    "iban",
];
/// Words next to which a long number is a reference, not a recipient.
const REFERENCE_WORDS: &[&str] = &[
    "order",
    "reference",
    "ref",
    "invoice",
    "help",
    "support",
    "tracking",
    "confirmation",
    "booking",
    "receipt",
    "ticket",
    "case",
];

/// Whether `text` has a word of `table`.
fn has_word(text: &str, table: &[&str]) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .any(|w| table.contains(&w.as_str()))
}

/// The texts on `screen` (not the target's, never a secure or protected field's) that say
/// who is paid or messaged: phone numbers, masked account or card suffixes, email
/// addresses, `@handles`. Money texts are left to the amounts. Ranked: texts with
/// recipient wording (to, pay to, recipient, send to, account) first, then other
/// recipients, and last a 7–15 digit number next to order / reference / invoice / help
/// wording; within each, a selected option (the chosen card or account) first, then the
/// closest to the target (screen order without a target). All
/// are returned: the list shows [`MAX_LISTED_RECIPIENTS`] and counts the rest.
fn recipient_facts(screen: &CompactScreen, target: Option<&Element>) -> Vec<String> {
    let mut ranked: Vec<(u8, bool, f64, usize, String)> = Vec::new();
    for (k, e) in screen.elements.iter().enumerate().filter(|(_, e)| {
        !e.secure
            && e.role != Role::Key
            && !credential_like_field(e)
            && !target.is_some_and(|t| t.index == e.index)
    }) {
        let t = fact_text(e).trim().to_owned();
        if t.is_empty() || has_money(&t) || ranked.iter().any(|(_, _, _, _, x)| *x == t) {
            continue;
        }
        let word = t.split_whitespace().any(recipient_word);
        let phone = has_phone_number(&t);
        if !word && !phone {
            continue;
        }
        let tier = if has_word(&t, RECIPIENT_WORDS) {
            0
        } else if !word && has_word(&t, REFERENCE_WORDS) {
            2
        } else {
            1
        };
        let distance = target
            .filter(|tg| !tg.frame.is_empty() && !e.frame.is_empty())
            .map_or(0.0, |tg| {
                let (a, b) = (tg.frame.center(), e.frame.center());
                let d = (a.x - b.x).hypot(a.y - b.y);
                if d.is_finite() {
                    d
                } else {
                    f64::MAX
                }
            });
        ranked.push((tier, !e.selected, distance, k, t));
    }
    ranked.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.cmp(&b.1))
            .then(a.2.total_cmp(&b.2))
            .then(a.3.cmp(&b.3))
    });
    ranked.into_iter().map(|(_, _, _, _, t)| t).collect()
}

/// One fact entry for the summary: cleaned like a label (amounts kept when shortened), with
/// the separators of the summary's own structure (`;`) neutralised.
fn fact_entry(text: &str, max: usize) -> String {
    clean_label(text, max).replace(';', ",")
}

/// The facts of an in-app summary, gathered once.
struct Facts {
    amounts: Vec<AmountFact>,
    recipients: Vec<String>,
    selected: Vec<String>,
    fields: Vec<String>,
}

/// `Heading: a; b; c and N more` with at most `max` entries, each `entry`-formatted; with
/// `entry` 0 (no room) `Heading: N not shown`. Never silent about what it leaves out.
fn fact_list(heading: &str, items: &[String], max: usize, entry: usize) -> Option<String> {
    if items.is_empty() {
        return None;
    }
    if entry == 0 {
        return Some(format!("{heading}: {} not shown", items.len()));
    }
    let shown: Vec<String> = items
        .iter()
        .take(max)
        .map(|t| fact_entry(t, entry))
        .collect();
    let more = items.len().saturating_sub(max);
    let tail = if more > 0 {
        format!(" and {more} more")
    } else {
        String::new()
    };
    Some(format!("{heading}: {}{tail}", shown.join("; ")))
}

/// The facts appended to an in-app summary, in this order (amounts first, so they are the
/// last screen text to shrink):
///
/// - `Amounts on screen: Due today $199.00; Compare at $1,200.00 and 2 more` (every
///   distinct amount, the charge first, at most [`MAX_LISTED_AMOUNTS`], then always the
///   count of the rest);
/// - `To/Account: To account ••••4521; @alex and 2 more` (payments and messages only, ranked
///   by recipient wording and closeness to the target, at most [`MAX_LISTED_RECIPIENTS`],
///   then `and N more`);
/// - `Selected: Annual plan` (at most [`MAX_LISTED_SELECTED`], ranked, then `and N more`);
/// - `Fields: Message: On my way; Note (filled); Card number (hidden)` (a value only where
///   the session typed it, see [`field_facts`]; at most [`MAX_LISTED_FIELDS`], then
///   `and N more`; a protected field by name only).
///
/// Empty parts are left out; a part with no room left says how many entries it has
/// (`Fields: 2 not shown`), so nothing is dropped silently.
fn facts_suffix(facts: &Facts, limits: Limits) -> Vec<String> {
    let mut parts = Vec::new();
    let amounts = &facts.amounts;
    if !amounts.is_empty() {
        // The largest amount on the screen is always listed, whatever its wording: it
        // takes the last listed place when the ranking left it out.
        let largest = amounts
            .iter()
            .enumerate()
            .fold(None::<(usize, f64)>, |best, (k, f)| match best {
                Some((_, v)) if v >= f.value => best,
                _ => Some((k, f.value)),
            })
            .map(|(k, _)| k);
        let mut picked: Vec<&AmountFact> = amounts.iter().take(MAX_LISTED_AMOUNTS).collect();
        if let Some(k) = largest.filter(|k| *k >= MAX_LISTED_AMOUNTS) {
            if let (Some(last), Some(big)) = (picked.last_mut(), amounts.get(k)) {
                *last = big;
            }
        }
        let shown: Vec<String> = picked
            .iter()
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
                    if tokens.is_empty() {
                        fact_entry(&f.text, MAX_AMOUNT_CHARS)
                    } else {
                        tokens.join(" ")
                    }
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
    parts.extend(fact_list(
        "To/Account",
        &facts.recipients,
        MAX_LISTED_RECIPIENTS,
        limits.recipient_entry,
    ));
    if !facts.selected.is_empty() {
        if limits.selected_entry == 0 {
            parts.push(format!("Selected: {} options", facts.selected.len()));
        } else {
            parts.extend(fact_list(
                "Selected",
                &facts.selected,
                MAX_LISTED_SELECTED,
                limits.selected_entry,
            ));
        }
    }
    parts.extend(fact_list(
        "Fields",
        &facts.fields,
        MAX_LISTED_FIELDS,
        limits.field_entry,
    ));
    parts
}

/// The summary of pressing `el` (`verb` "Tap" or "Long-press"): a tap on an on/off switch
/// says what it will do (`Turn on "Wi-Fi" in Settings`); a control whose row has other
/// text names it (`Tap "Buy" next to "Socks $5.00" in Shop`); a control under a card's
/// stacked title names that too (`Tap "Delete order" under "Order #1001, Placed Nov 20"
/// in Shop`). The screen's facts follow (see `facts_suffix`), never one amount picked by
/// distance.
fn press_summary(
    verb: &str,
    el: &Element,
    screen: &CompactScreen,
    in_app: &str,
    l: Limits,
    tree: Option<&ScreenTree<'_>>,
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
    let item = item_texts(el, screen, tree).join(", ");
    let under = if item.is_empty() || l.under == 0 {
        String::new()
    } else {
        format!(" under \"{}\"", clean_label(&item, l.under))
    };
    format!("{verb} {what}{next_to}{under}{in_app}")
}

/// A one-line, human-readable, sanitised description of `action` for a confirmation
/// prompt, e.g. `Tap "Pay $42.10" in Amazon`. Typed secrets are never shown.
pub fn summarize(action: &Action, screen: &CompactScreen, tool: Option<&ToolSpec>) -> String {
    summary_for(
        action,
        screen,
        classify(action, screen, tool),
        ScreenSource::Typed,
        None,
    )
}

/// The confirmation summary of `action` on `screen` for a known `category` (as
/// [`evaluate_from`] builds it): on a runner-text screen it starts with
/// [`UNVERIFIED_SCREEN_WARNING`]; with `tree`, the `under "…"` part prefers texts in the
/// target's own container. The screen's facts are built from the label-aware redaction
/// the prompt uses (`redact_screen`); fields named as credentials or bank/card details are
/// listed by name only. Callers mask the owner's profile values in `screen` first.
pub fn summary_for(
    action: &Action,
    screen: &CompactScreen,
    category: RiskCategory,
    source: ScreenSource,
    tree: Option<&ScreenTree<'_>>,
) -> String {
    summary_with(action, screen, category, source, tree, &[])
}

/// [`summary_for`], knowing what the session typed during the task (`typed`).
///
/// **Field values (sixth review).** A field's value (text input, slider, stepper, picker)
/// is shown only when (a) it is exactly what the session typed into that field during this
/// task (the owner sees what Jarvis wrote, with the usual profile masking), or (b) the
/// field's label or placeholder is amount-like (amount, total, price, tip, quantity, qty)
/// and the value is a money amount or a plain number (listed with the amounts). Every
/// other field is named with `(filled)` or `(empty)`; a field named as a credential or a
/// bank or card detail shows `(hidden)`. Static texts keep the usual redaction.
///
/// **What is sent (seventh review).** When the action sends fields, the summary shows the
/// *whole current value* of each, whoever typed it: Return sends its field (`return_summary`:
/// `Press Return ("send") in "Message" in Chat, which sends the field's whole content:
/// "…". Typed: "ok"`); a tap whose category is `SendMessage`, `PublicPost`, `Payment` or
/// `Purchase`, or whose label has a submit word (submit, send, post, publish, reply,
/// confirm, save), sends every non-secure text input under the nearest raw ancestor of the
/// target that holds a text input (its form; without the raw tree, every text input on the
/// screen): `Tap "Send" in Chat. Sends: Message: "…"; Subject (empty)`. The values pass the
/// pattern redaction (cards, labelled codes); a credential-like field is `(hidden)`. Other
/// facts shrink first; a value is cut only when the summary would not fit the bound (480
/// characters) otherwise, and then says how many characters are not shown. Fields not being
/// sent keep the rule above.
pub fn summary_with(
    action: &Action,
    screen: &CompactScreen,
    category: RiskCategory,
    source: ScreenSource,
    tree: Option<&ScreenTree<'_>>,
    typed: &[TypedText],
) -> String {
    with_source(source, |max| {
        summarize_with(action, screen, category, tree, typed, false, max)
    })
}

/// The summary of pressing Return in field `field` after the session typed `text` there (the
/// second part of a `type … submit`, gated on the post-typing `screen`): `Press Return
/// ("send") in "Message" in Chat, which sends the field's whole content: "On my way".
/// Typed: "On my way"`, then the screen's facts (recipients and amounts of that screen; the
/// field itself is not repeated). Built like [`summary_with`]; `text` keeps its profile
/// placeholders, a secret is never quoted.
pub fn return_summary(
    field: u32,
    text: &str,
    screen: &CompactScreen,
    category: RiskCategory,
    source: ScreenSource,
    tree: Option<&ScreenTree<'_>>,
    typed: &[TypedText],
) -> String {
    let action = Action::Type {
        element: Some(field),
        text: text.to_owned(),
        submit: true,
    };
    with_source(source, |max| {
        summarize_with(&action, screen, category, tree, typed, true, max)
    })
}

/// A summary of at most [`MAX_SUMMARY_CHARS`] from `build(max)`, behind the warning on a
/// runner-text screen.
fn with_source(source: ScreenSource, build: impl Fn(usize) -> String) -> String {
    match source {
        ScreenSource::Typed => build(MAX_SUMMARY_CHARS),
        ScreenSource::RunnerText => {
            let room = MAX_SUMMARY_CHARS.saturating_sub(UNVERIFIED_SCREEN_WARNING.chars().count());
            format!("{UNVERIFIED_SCREEN_WARNING}{}", build(room.max(120)))
        }
    }
}

/// A field whose whole content an action sends (see [`summary_with`]): its name and its
/// cleaned, redacted value (`None`: hidden; empty: nothing in it).
struct SentField {
    name: String,
    value: Option<String>,
}

/// Categories of a tap that sends what the form around it holds.
const SENDING_CATEGORIES: &[RiskCategory] = &[
    RiskCategory::SendMessage,
    RiskCategory::PublicPost,
    RiskCategory::Payment,
    RiskCategory::Purchase,
];

/// Words of a tapped control that submit its form whatever its category.
const SUBMIT_WORDS: &[&str] = &[
    "submit", "send", "post", "publish", "reply", "confirm", "save",
];

/// Whether a tap on `el` in `category` sends the fields of its form.
fn tap_sends(el: &Element, category: RiskCategory) -> bool {
    SENDING_CATEGORIES.contains(&category)
        || crate::text::tokens(&el.label)
            .iter()
            .any(|t| SUBMIT_WORDS.contains(&t.as_str()))
}

/// The text inputs a tap on `target` sends: every non-secure text input under the nearest
/// raw ancestor of the target that holds any text input (its form); without the raw tree,
/// or when no ancestor below the root holds one, every non-secure text input on the screen.
fn form_fields<'a>(
    target: &Element,
    screen: &'a CompactScreen,
    tree: Option<&ScreenTree<'_>>,
) -> Vec<&'a Element> {
    let inputs: Vec<&Element> = screen
        .elements
        .iter()
        .filter(|e| e.role.is_text_input() && e.index != target.index)
        .collect();
    if let Some(t) = tree {
        for a in t.ancestors(target) {
            let inside: Vec<&Element> = inputs.iter().copied().filter(|e| t.inside(e, a)).collect();
            if !inside.is_empty() {
                return inside.into_iter().filter(|e| !e.secure).collect();
            }
        }
    }
    inputs.into_iter().filter(|e| !e.secure).collect()
}

/// `el` as a field being sent: hidden when secure or named as a credential.
fn sent_field(el: &Element) -> SentField {
    let name = field_name(el);
    let name = if name.is_empty() {
        "A field".to_owned()
    } else {
        name
    };
    let value = if el.secure || credential_like_field(el) {
        None
    } else {
        Some(
            field_value(el)
                .map(|v| clean(v, crate::sanitize::MAX_SCREEN_TEXT_CHARS))
                .unwrap_or_default(),
        )
    };
    SentField { name, value }
}

/// `: "value"` (its first `keep` characters when it is longer, followed by how many are
/// not shown), or ` (hidden)` / ` (empty)` when there is nothing to quote.
fn sent_value(f: &SentField, keep: usize) -> String {
    match f.value.as_deref() {
        None => " (hidden)".to_owned(),
        Some("") => " (empty)".to_owned(),
        Some(v) => {
            let n = v.chars().count();
            if n <= keep {
                format!(": \"{v}\"")
            } else {
                let head: String = v.chars().take(keep).collect();
                let left = n - keep;
                format!(
                    ": \"{}…\" ({left} more character{} not shown)",
                    head.trim_end(),
                    if left == 1 { "" } else { "s" }
                )
            }
        }
    }
}

/// The part of a summary that says what is sent, each value cut to at most `keep[i]`.
fn sent_part(sent: &[SentField], keep: &[usize], return_only: bool) -> String {
    let value = |i: usize| {
        sent.get(i).map_or_else(String::new, |f| {
            sent_value(f, keep.get(i).copied().unwrap_or(usize::MAX))
        })
    };
    if return_only {
        format!(", which sends the field's whole content{}", value(0))
    } else {
        let items: Vec<String> = sent
            .iter()
            .enumerate()
            .map(|(i, f)| format!("{}{}", clean(&f.name, MAX_QUOTE_CHARS), value(i)))
            .collect();
        format!(". Sends: {}", items.join("; "))
    }
}

fn summarize_with(
    action: &Action,
    raw: &CompactScreen,
    category: RiskCategory,
    tree: Option<&ScreenTree<'_>>,
    typed: &[TypedText],
    return_only: bool,
    max: usize,
) -> String {
    // Facts and quoted labels come from the prompt's label-aware redaction.
    let redacted = redact_screen(raw);
    let screen = &redacted;
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
    // What the action sends (seventh review): Return sends its field; a sending tap sends
    // the text inputs of its form. Each is shown whole.
    let sent_fields: Vec<&Element> = match (action, target) {
        (Action::Type { .. }, Some(t)) if return_only => vec![t],
        (Action::Tap { .. }, Some(t)) if tap_sends(t, category) => form_fields(t, screen, tree),
        _ => Vec::new(),
    };
    let sent: Vec<SentField> = sent_fields.iter().map(|e| sent_field(e)).collect();
    let with_facts = uses_foreground_app(action) && !matches!(action, Action::Look);
    let facts = if with_facts {
        let pays = matches!(
            category,
            RiskCategory::Payment | RiskCategory::SendMessage | RiskCategory::Purchase
        );
        let fields = field_facts(screen, raw, target, typed, &sent_fields);
        Facts {
            amounts: amount_facts(screen, target),
            recipients: if pays {
                recipient_facts(screen, target)
            } else {
                Vec::new()
            },
            selected: selected_facts(screen, target),
            fields,
        }
    } else {
        Facts {
            amounts: Vec::new(),
            recipients: Vec::new(),
            selected: Vec::new(),
            fields: Vec::new(),
        }
    };
    // What was typed, beside the content Return sends.
    let typed_part = match action {
        Action::Type { text, .. } if return_only => {
            Some(format!("Typed: {}", typed_words(text, category, target)))
        }
        _ => None,
    };
    let full: Vec<usize> = vec![usize::MAX; sent.len()];
    let build = |l: Limits, keep: &[usize]| -> String {
        let mut line = action_line(action, screen, category, l, tree, return_only);
        if !sent.is_empty() {
            line.push_str(&sent_part(&sent, keep, return_only));
        }
        let mut parts = vec![line];
        parts.extend(typed_part.clone());
        parts.extend(facts_suffix(&facts, l));
        parts.join(". ")
    };
    let mut line = String::new();
    let mut last = None;
    for l in LIMITS {
        line = build(l, &full);
        last = Some(l);
        if line.chars().count() <= max {
            return sanitize_screen_text_with_limit(&line, max);
        }
    }
    // Too long with every other fact at its shortest: the sent values are cut, each to an
    // equal share of the room left, saying how many characters are not shown.
    if let Some(last) = last.filter(|_| !sent.is_empty()) {
        let lens: Vec<usize> = sent
            .iter()
            .map(|f| f.value.as_deref().map_or(0, |v| v.chars().count()))
            .collect();
        let total: usize = lens.iter().sum();
        let over = line.chars().count().saturating_sub(max);
        let mut budget = total.saturating_sub(over);
        loop {
            let share = budget / sent.len().max(1);
            let keep: Vec<usize> = lens.iter().map(|&n| n.min(share.max(1))).collect();
            line = build(last, &keep);
            let n = line.chars().count();
            if n <= max || budget == 0 {
                break;
            }
            budget = budget.saturating_sub(n - max).saturating_sub(1);
        }
    }
    sanitize_screen_text_with_limit(&line, max)
}

/// What was typed, as a summary says it: quoted, or "a secret (N characters)".
fn typed_words(text: &str, category: RiskCategory, target: Option<&Element>) -> String {
    let secret = category == RiskCategory::Credential
        || contains_secret(text)
        || target.is_some_and(|t| t.secure);
    if secret {
        format!("a secret ({} characters)", text.chars().count())
    } else {
        format!("\"{}\"", clean(text, MAX_QUOTE_CHARS))
    }
}

/// What `action` does, without the screen's facts.
fn action_line(
    action: &Action,
    screen: &CompactScreen,
    category: RiskCategory,
    l: Limits,
    tree: Option<&ScreenTree<'_>>,
    return_only: bool,
) -> String {
    let app_name = screen
        .app_name
        .as_deref()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or(&screen.app_bundle_id);
    let in_app = format!(" in {}", clean(app_name, l.app));
    match action {
        Action::Tap { element } => match screen.get(*element) {
            Some(el) => press_summary("Tap", el, screen, &in_app, l, tree),
            None => format!("Tap element {element}{in_app}"),
        },
        Action::LongPress { element, .. } => match screen.get(*element) {
            Some(el) => press_summary("Long-press", el, screen, &in_app, l, tree),
            None => format!("Long-press element {element}{in_app}"),
        },
        Action::Type {
            element,
            text,
            submit,
        } => {
            let target = type_target(*element, screen);
            let what = typed_words(text, category, target);
            let into = target.map_or_else(
                || "the focused field".to_owned(),
                |t| describe_with(t, l.label),
            );
            if return_only {
                let key = screen
                    .cues
                    .return_key
                    .as_deref()
                    .map(|k| clean(k, 24))
                    .filter(|k| !k.is_empty())
                    .map(|k| format!(" (\"{k}\")"))
                    .unwrap_or_default();
                // What it sends (the field's whole content) and what was typed follow.
                return format!("Press Return{key} in {into}{in_app}");
            }
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
