//! The single entry point the agent calls before executing an action.

use jarvis_proto::{
    render_rows, Action, BarNode, CompactScreen, ConfirmRequest, ConfirmVerb, Element, Policy,
    QuestionRow, QuestionRowKind, RawInput, RawNode, RiskCategory, Role, ToolSpec, TrustLevel,
};

use crate::actuation::{actuation, Actuation};
use crate::app::{app_level, check_app, is_springboard, mode_level, Verdict};
use crate::classify::{classify, type_target};
use crate::gate::{changes_state, gate_owned, Gate};
use crate::redact::{contains_secret, credential_like_field, redact, redact_screen};
use crate::sanitize::sanitize_screen_text_with_limit;
use crate::shape::{
    has_number_word, link_shaped, money_shaped, names_someone, number_shaped, owner_words,
};
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
    /// Every text input whose text the digest hashes, listed or not
    /// (`jarvis_observe::CompactView::inputs`, values profile-masked by the session like
    /// the screen): a submit's summary names the fields of its form that are not listed
    /// (eighth review). `&[]` names only listed ones.
    pub inputs: &'a [RawInput],
}

impl ScreenTree<'_> {
    pub(crate) fn source(&self, el: &Element) -> Option<u32> {
        let pos = usize::try_from(el.index).ok()?.checked_sub(1)?;
        self.sources.get(pos).copied()
    }

    /// The raw ancestors of `el`'s node, innermost first, the root excepted (it holds
    /// everything, so it is no one's container).
    pub(crate) fn ancestors(&self, el: &Element) -> Vec<u32> {
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
    evaluate_with(policy, action, screen, tool, source, None, &[])
}

/// [`evaluate_from`], knowing the raw tree's shape (`tree`) and what the session typed
/// during the task (`typed`). Eighth review: a tap that [`presumed_submit`] (a control in
/// the same form or composer as a text input Jarvis typed into, not clearly something
/// else) is classified at least [`RiskCategory::SendMessage`], so it asks and its question
/// shows what is sent. The summary is built with `tree` and `typed` ([`summary_with`]).
pub fn evaluate_with(
    policy: &Policy,
    action: &Action,
    screen: &CompactScreen,
    tool: Option<&ToolSpec>,
    source: ScreenSource,
    tree: Option<&ScreenTree<'_>>,
    typed: &[TypedText],
) -> Decision {
    evaluate_owned(policy, action, screen, tool, source, tree, typed, &[])
}

/// [`evaluate_with`], knowing what the owner wrote for this task (`owner`: the task's text,
/// the running skill's own definition). Thirteenth review (M3): `open_url` is raised to at
/// least `SendMessage` (so it asks; a call link stays `Call`) unless the link was given by
/// the owner ([`owner_gave_url`]), and such a question is `phone_only`; a link the owner
/// gave is classified as before (calls ask, other links do not). The session's gate.
///
/// Fourteenth review: the same link gate covers Return in any field submitting a link and
/// links in a tool's arguments (Rule A); a search field is exempt from the presumed submit
/// only with the owner's words in it (Rule B); and a request whose category the gate
/// guessed — a presumed submit not read from its control's own words, a send-by-label
/// whose form may hold an amount or names no verified recipient, a link the owner did not
/// give — is unclear: asked whatever the owner relaxed ([`crate::gate_owned`]) and
/// `phone_only` (Rule C).
#[allow(clippy::too_many_arguments)]
pub fn evaluate_owned(
    policy: &Policy,
    action: &Action,
    screen: &CompactScreen,
    tool: Option<&ToolSpec>,
    source: ScreenSource,
    tree: Option<&ScreenTree<'_>>,
    typed: &[TypedText],
    owner: &[&str],
) -> Decision {
    evaluate_egress(
        policy, action, screen, tool, source, tree, typed, owner, false,
    )
}

/// Fifteenth review (R1, the typing gate): whether every piece of model-written text
/// `action` sends out — the text a `type` types (with or without Return), every string
/// argument of a tool call — is the owner's own words ([`owner_words`], links included).
/// `open_url` passes the link gate of Rule A instead; other actions send no text.
pub fn egress_owned(action: &Action, owner: &[&str]) -> bool {
    match action {
        Action::Type { text, .. } => owner_words(text, owner),
        Action::CallTool { args, .. } => {
            // Sixteenth review (F7): numbers are model text too; top-level keys are the
            // declared parameter names (`check_call` rejects any other), nested keys ask.
            let mut texts = Vec::new();
            match args {
                serde_json::Value::Object(m) => m
                    .values()
                    .for_each(|v| crate::classify::tool_texts(v, &mut texts)),
                other => crate::classify::tool_texts(other, &mut texts),
            }
            texts.len() < crate::classify::MAX_TOOL_STRINGS
                && texts.iter().all(|s| owner_words(s, owner))
        }
        _ => true,
    }
}

/// The note a question about model text that is not the owner's carries (R1):
/// `. This text is not from your request.`
pub fn not_owner_note() -> QuestionRow {
    row(QuestionRowKind::Note, &format!(". {MARK_NOT_OWNER}"))
}

/// Sixteenth review (Rule T): the note of a question about an input into a place while
/// `places` (display names of the contexts read in this task and not yet allowed into it,
/// in order) could flow there: `. Jarvis has read Messages, example.com and 2 more in this
/// task; what it read there could be passed on here`. At most 4 places are named.
pub fn flow_note(places: &[String]) -> QuestionRow {
    let names: Vec<String> = places.iter().take(4).map(|p| clean(p, 40)).collect();
    let more = places.len().saturating_sub(names.len());
    let list = match (names.split_last(), more) {
        (None, _) => "other places".to_owned(),
        (Some((last, [])), 0) => last.clone(),
        (Some((last, rest)), 0) => format!("{} and {last}", rest.join(", ")),
        (Some(_), n) => format!("{} and {n} more", names.join(", ")),
    };
    row(
        QuestionRowKind::Flow,
        &format!(". Jarvis has read {list} in this task; {MARK_FLOW}"),
    )
}

/// Sixteenth review (Rule P): the note of a question about pressing a control that inserts
/// content Jarvis cannot show (the clipboard, AutoFill) into `place` (the app or site in
/// front, display name); `copied_in` names where Jarvis pressed Copy or Cut in this task.
pub fn paste_note(place: &str, copied_in: &[String]) -> QuestionRow {
    let mut text = format!(
        ". This enters {MARK_PASTE} (the clipboard or AutoFill) into {}",
        clean(place, 40)
    );
    if !copied_in.is_empty() {
        let names: Vec<String> = copied_in.iter().take(3).map(|p| clean(p, 40)).collect();
        text.push_str(&format!(
            "; Jarvis copied text in {} in this task",
            names.join(", ")
        ));
    }
    row(QuestionRowKind::Insert, &text)
}

/// The note of a typing that holds profile values the owner marked sensitive (R1, F2):
/// names the keys, never the values. `keys` are the owner's own key names.
pub fn sensitive_note(keys: &[&str]) -> QuestionRow {
    let names: Vec<String> = keys
        .iter()
        .take(4)
        .map(|k| format!("\"{}\"", clean(k, 30)))
        .collect();
    let more = keys.len().saturating_sub(names.len());
    let more = if more > 0 {
        format!(" and {more} more")
    } else {
        String::new()
    };
    row(
        QuestionRowKind::Note,
        &format!(
            ". {MARK_NOT_OWNER}: it types your saved {}{more}, which you marked sensitive",
            names.join(", ")
        ),
    )
}

/// [`evaluate_owned`], with `sensitive`: the text a `type` types holds a profile value the
/// owner marked sensitive (the session knows; the gate does not), which always asks, phone
/// only (fifteenth review, F2).
///
/// Fifteenth review: R1 — model text that is not the owner's ([`egress_owned`]) is asked
/// before it reaches the phone, at least `SendMessage`, unclear (asked whatever the owner
/// relaxed, phone only), with [`not_owner_note`]; a `type … submit` so asked is one
/// question that also says what its Return does. R3 — a control that sends by its own
/// words on a screen showing money is unclear (`Payment` with a payment context on
/// screen). R4 — a tap on a control inside an alert that is not a plain dismiss is unclear
/// (`SystemSetting` when the alert reads as a permission prompt), its title quoted.
#[allow(clippy::too_many_arguments)]
pub fn evaluate_egress(
    policy: &Policy,
    action: &Action,
    screen: &CompactScreen,
    tool: Option<&ToolSpec>,
    source: ScreenSource,
    tree: Option<&ScreenTree<'_>>,
    typed: &[TypedText],
    owner: &[&str],
    sensitive: bool,
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
    let mut category = match browser_forward(action, screen, tree) {
        // Narrowing (b), ninth review: a "Forward" in a system bar that also holds "Back"
        // is a browser's navigation, read like "Back" (not the send phrase "forward").
        Some(index) => {
            let mut nav = screen.clone();
            if let Some(e) = nav.elements.iter_mut().find(|e| e.index == index) {
                e.label = "Back".to_owned();
            }
            classify_from(action, &nav, tool, source)
        }
        None => classify_from(action, screen, tool, source),
    };
    // Fourteenth review (Rule C): how the category was established. A category the
    // control's own words or role gave (`read`) is READ; one the gate guessed — a presumed
    // submit, a send-by-label whose form may hold an amount, a link the owner did not give —
    // is UNCLEAR: asked whatever categories the owner relaxed, and approved on the phone
    // only. Page-controlled words never lift either.
    let mut unclear = false;
    let presumed = presumed_submit_owned(action, screen, tree, typed, owner);
    // Only a presumed submit or a send needs the control's own words (the cheap test first).
    let read = if presumed || SENDING_CATEGORIES.contains(&category) {
        own_category(action, screen)
    } else {
        RiskCategory::None
    };
    if presumed {
        category = crate::phrases::max_risk(category, RiskCategory::SendMessage);
        if presumed_money(action, screen, tree) {
            category = crate::phrases::max_risk(category, RiskCategory::Payment);
        }
        unclear |= category != read;
    }
    if !unclear && SENDING_CATEGORIES.contains(&read) {
        unclear = sent_amount_unclear(action, screen, tree, read, category);
    }
    // Fourteenth review (Rule A): every way the model's own text becomes a navigation —
    // `open_url`, Return (or the keyboard's Go key) in any field holding a link, a link in a
    // tool's arguments — passes the owner-gave-the-link gate (thirteenth review, M3).
    for url in navigated_links(action, screen) {
        if !owner_gave_url(&url, owner) {
            // A call link stays `Call` (it asks as one); any other link sends.
            if crate::classify::url_risk(&url) == RiskCategory::None {
                category = crate::phrases::max_risk(category, RiskCategory::SendMessage);
            }
            unclear = true;
        }
    }
    // Fifteenth review (R3): a send by its own words on a screen that shows money (any
    // element's text or value money-shaped or an amount) is a guess; with a payment
    // context on the screen it is a payment.
    if !unclear && matches!(read, RiskCategory::SendMessage | RiskCategory::PublicPost) {
        if let Some(payment) = screen_shows_money(screen) {
            unclear = true;
            if payment {
                category = crate::phrases::max_risk(category, RiskCategory::Payment);
            }
        }
    }
    // Fifteenth review (R4): a tap on a control inside an alert that is not a plain dismiss
    // asks, phone only; a permission prompt's is `SystemSetting`.
    if let Some(alert) = alert_of(action, screen, tree).filter(|a| !a.dismiss) {
        let floor = if alert.permission {
            RiskCategory::SystemSetting
        } else {
            RiskCategory::Unverified
        };
        category = crate::phrases::max_risk(category, floor);
        unclear = true;
    }
    // Sixteenth review (Rule P): a control that inserts content the question cannot show
    // (Paste, AutoFill, password items) always asks, phone only.
    let inserts = crate::flow::insert_control(action, screen, tree);
    if let Some(floor) = inserts {
        category = crate::phrases::max_risk(category, floor);
        unclear = true;
    }
    // Fifteenth review (R1): model text that is not the owner's asks before it reaches the
    // phone (Rule A's links are part of it: `owner_words` checks every link in the text).
    let not_owner =
        (sensitive && matches!(action, Action::Type { .. })) || !egress_owned(action, owner);
    if not_owner {
        category = crate::phrases::max_risk(category, RiskCategory::SendMessage);
        unclear = true;
    }
    let runner_text =
        source == ScreenSource::RunnerText && !matches!(action, Action::CallTool { .. });
    let gate = match gate_owned(
        policy,
        category,
        level,
        action,
        screen,
        runner_text,
        unclear,
    ) {
        // Rule P asks whatever the owner relaxed (only a denial refuses).
        Gate::Allow if inserts.is_some() => Gate::Confirm,
        other => other,
    };
    match gate {
        Gate::Allow => Decision::Allow,
        Gate::Deny => Decision::Deny(format!("{category:?} actions are not allowed")),
        Gate::Confirm => {
            let mut notes = if not_owner {
                vec![not_owner_note()]
            } else {
                Vec::new()
            };
            if inserts.is_some() {
                notes.push(paste_note(&place_name(screen), &[]));
            }
            let mut request = ConfirmRequest::from_rows(
                category,
                question_noted(action, screen, category, source, tree, typed, &notes),
                affected_app,
            );
            request.verb = verb_of(action);
            request.phone_only = unclear;
            Decision::Confirm(request)
        }
    }
}

/// The display name of the app in front (its name, else its bundle id).
fn place_name(screen: &CompactScreen) -> String {
    screen
        .app_name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| screen.app_bundle_id.clone())
}

/// Sixteenth review (Rule T): a question asked for the flow alone — the action would run
/// unasked, but `places` (contexts read in this task, not yet allowed into the one in
/// front) could reach it. Category `SendMessage` (what was read could be sent on), phone
/// only; its action line is composed as an action of no risk, followed by [`flow_note`].
#[allow(clippy::too_many_arguments)]
pub fn flow_request(
    action: &Action,
    screen: &CompactScreen,
    source: ScreenSource,
    tree: Option<&ScreenTree<'_>>,
    typed: &[TypedText],
    places: &[String],
    app: String,
) -> ConfirmRequest {
    let rows = question_noted(
        action,
        screen,
        RiskCategory::None,
        source,
        tree,
        typed,
        &[flow_note(places)],
    );
    let mut request = ConfirmRequest::from_rows(RiskCategory::SendMessage, rows, app);
    request.verb = verb_of(action);
    request.phone_only = true;
    request.flow_only = true;
    request
}

/// Sixteenth review: [`evaluate_egress`] under Rule T. `places` are the display names of
/// the contexts read in this task that the owner has not allowed into the one this input
/// goes to (the session's [`crate::Confinement::unallowed`]); empty, or an action that puts
/// no input anywhere ([`crate::flow_checked`]), gives `evaluate_egress`'s decision. Else a
/// denial stays a denial, a question gains [`flow_note`] and becomes phone only (one
/// question, never two), and an action that would run unasked asks for the flow alone
/// ([`flow_request`]).
#[allow(clippy::too_many_arguments)]
pub fn evaluate_confined(
    policy: &Policy,
    action: &Action,
    screen: &CompactScreen,
    tool: Option<&ToolSpec>,
    source: ScreenSource,
    tree: Option<&ScreenTree<'_>>,
    typed: &[TypedText],
    owner: &[&str],
    places: &[String],
) -> Decision {
    let decision = evaluate_egress(
        policy, action, screen, tool, source, tree, typed, owner, false,
    );
    if places.is_empty() || !crate::flow::flow_checked(action) {
        return decision;
    }
    match decision {
        Decision::Deny(r) => Decision::Deny(r),
        Decision::Allow => {
            let app = match (action, tool.and_then(|t| t.app.clone())) {
                (Action::CallTool { .. }, Some(app)) => app,
                _ => screen.app_bundle_id.clone(),
            };
            Decision::Confirm(flow_request(
                action, screen, source, tree, typed, places, app,
            ))
        }
        Decision::Confirm(mut r) => {
            let mut notes = Vec::new();
            if !egress_owned(action, owner) {
                notes.push(not_owner_note());
            }
            if crate::flow::insert_control(action, screen, tree).is_some() {
                notes.push(paste_note(&place_name(screen), &[]));
            }
            notes.push(flow_note(places));
            r.rows = question_noted(action, screen, r.category, source, tree, typed, &notes);
            r.summary = render_rows(&r.rows);
            r.phone_only = true;
            Decision::Confirm(r)
        }
    }
}

/// The kind of `action`, as a confirmation states it ([`ConfirmRequest::verb`]).
pub fn verb_of(action: &Action) -> ConfirmVerb {
    match action {
        Action::Tap { .. } => ConfirmVerb::Tap,
        Action::LongPress { .. } => ConfirmVerb::LongPress,
        Action::Type { .. } => ConfirmVerb::Type,
        Action::Clear { .. } => ConfirmVerb::Clear,
        Action::Scroll { .. } => ConfirmVerb::Scroll,
        Action::Swipe { .. } => ConfirmVerb::Swipe,
        Action::TapPoint { .. } => ConfirmVerb::TapPoint,
        Action::PressHome => ConfirmVerb::PressHome,
        Action::OpenApp { .. } => ConfirmVerb::OpenApp,
        Action::OpenUrl { .. } => ConfirmVerb::OpenUrl,
        Action::CallTool { .. } => ConfirmVerb::CallTool,
        Action::Wait { .. }
        | Action::AskUser { .. }
        | Action::Look
        | Action::Done { .. }
        | Action::Fail { .. } => ConfirmVerb::Other,
    }
}

/// Thirteenth review (M3): whether `url` (trimmed) was given by the owner: it appears in
/// one of `owner` (the task's text, the running skill's own definition) as a whole link —
/// not preceded by a character a link is made of, and followed by the end, a space, a
/// closing bracket or quote, or `,` `;` `!`, or by `.` `?` `:` that end the sentence. So
/// `https://a.co` is not given by a task that says `https://a.com`, nor `…/c` by `…/c?v=1`.
pub fn owner_gave_url(url: &str, owner: &[&str]) -> bool {
    let url = url.trim();
    if url.is_empty() {
        return false;
    }
    let link_char = |c: char| c.is_alphanumeric() || "-._~:/?#[]@!$&*+,;=%".contains(c);
    owner.iter().any(|text| {
        text.match_indices(url).any(|(at, _)| {
            let before = text.get(..at).and_then(|b| b.chars().next_back());
            let mut after = text.get(at + url.len()..).unwrap_or_default().chars();
            let next = after.next();
            let then = after.next();
            let starts = before.is_none_or(|c| !link_char(c));
            let ends = match next {
                None => true,
                Some(c) if c.is_whitespace() => true,
                Some(')' | ']' | '"' | '\'' | '>' | '\u{201D}' | '\u{2019}' | ',' | ';' | '!') => {
                    true
                }
                Some('.' | '?' | ':') => then.is_none_or(char::is_whitespace),
                Some(_) => false,
            };
            starts && ends
        })
    })
}

/// The form an action actuates, and the field Return is pressed in: a pressed or dragged
/// control's form, Return's field's form, or (fourteenth review, Rule E) for a press on
/// something unlisted the whole screen. `None`: the action actuates nothing.
fn actuated_form<'a, 't>(
    action: &Action,
    screen: &'a CompactScreen,
    tree: Option<&ScreenTree<'t>>,
) -> Option<(Form<'a, 't>, Option<&'a Element>)> {
    match actuation(action, screen) {
        Actuation::Press(t) | Actuation::Drag(t) => Some((form_of(t, screen, tree), None)),
        Actuation::PressUnknown => Some((screen_form(screen, tree), None)),
        Actuation::Return(element) => {
            let f = type_target(element, screen)?;
            Some((form_of(f, screen, tree), Some(f)))
        }
        Actuation::Navigation | Actuation::NoEffect => None,
    }
}

/// Whether `e` is a value control whose value a form sends besides its text inputs
/// (fourteenth review, Rule D): a picker, slider, stepper or segmented control, or a button
/// that carries a value.
fn value_control(e: &Element) -> bool {
    !e.role.is_text_input()
        && !is_secure_input(e)
        && (is_value_role(&e.role) || (e.role == Role::Button && field_value(e).is_some()))
}

/// (name, value) of every filled non-secure field of `form` (listed, unlisted, and
/// `field`), and of the value controls inside it ([`value_control`]; without a raw form
/// root, every value control on the screen).
fn form_values<'s>(
    form: &Form<'s, '_>,
    field: Option<&'s Element>,
    screen: &'s CompactScreen,
    tree: Option<&ScreenTree<'_>>,
) -> Vec<(String, String)> {
    let name = |e: &Element| {
        format!(
            "{} {}",
            field_name(e),
            e.placeholder.as_deref().unwrap_or_default()
        )
    };
    let mut fields: Vec<(String, String)> = Vec::new();
    for e in form.listed.iter().copied().chain(field) {
        if let Some(v) = field_value(e).filter(|_| !is_secure_input(e)) {
            fields.push((name(e), v.to_owned()));
        }
    }
    for r in &form.unlisted {
        if let Some(v) = r.value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            let name = [&r.label, &r.placeholder, &r.identifier]
                .iter()
                .filter_map(|x| x.as_deref())
                .collect::<Vec<_>>()
                .join(" ");
            fields.push((name, v.to_owned()));
        }
    }
    for e in screen.elements.iter().filter(|e| value_control(e)) {
        let inside = match (tree, form.root) {
            (Some(t), Some(root)) => t.inside(e, root),
            _ => true,
        };
        if let Some(v) = field_value(e).filter(|_| inside) {
            fields.push((name(e), v.to_owned()));
        }
    }
    fields
}

/// Thirteenth review (M1): whether the form a presumed submit sends is money — decided by
/// what its fields hold, not only by their names. Fourteenth review (Rule D), by shape and
/// tolerant: a field or value control holds a money-shaped value ([`money_shaped`]: a
/// number with any currency-like piece next to it, `US$900`, `900 zł`, `CHF 900.-`,
/// `900,-`, `0.5 BTC`, `nine hundred dollars`; or the older currency sign or code with a
/// number); or one named by an amount word or a currency sign alone ("Amount", "€") holds
/// anything with a digit or an English number word; or a field under a strong recipient
/// head that names someone (Rule G) is filled and another holds a plain number (who plus
/// how much); or a text inside the form (on a typed tree) shows an amount. Such a presumed
/// submit is `Payment`.
fn presumed_money(action: &Action, screen: &CompactScreen, tree: Option<&ScreenTree<'_>>) -> bool {
    let Some((form, field)) = actuated_form(action, screen, tree) else {
        return false;
    };
    let fields = form_values(&form, field, screen, tree);
    if fields.iter().any(|(name, v)| {
        money_value(v)
            || money_shaped(v)
            || (amount_like_name(name, None)
                && (v.chars().any(crate::text::is_digit) || has_number_word(v)))
    }) {
        return true;
    }
    let number = |v: &str| {
        let v = crate::text::fold_digits(v);
        money_or_number(&v)
            && !(v.chars().all(|c| c.is_ascii_digit()) && v.len() >= MIN_PHONE_LIKE_DIGITS)
    };
    let payee = fields
        .iter()
        .position(|(name, v)| strong_recipient_field_name(name.trim()) && names_someone(v));
    if let Some(k) = payee {
        if fields
            .iter()
            .enumerate()
            .any(|(i, (name, v))| i != k && !recipient_field_name(name.trim()) && number(v))
        {
            return true;
        }
    }
    // Amounts shown inside the form (a typed tree only: without one the form is the whole
    // screen, and a price elsewhere on it says nothing about this form).
    match (tree, form.root) {
        (Some(t), Some(root)) => screen.elements.iter().any(|e| {
            !e.role.is_text_input()
                && e.role != Role::Key
                && t.inside(e, root)
                && has_money(&crate::classify::element_text(e))
        }),
        _ => false,
    }
}

/// Fourteenth review (Rule C): whether a control that sends by its own words (`read`, one
/// of [`SENDING_CATEGORIES`]) is still unclear: its form holds a value the gate cannot rule
/// out as an amount ([`number_shaped`], in any field or value control) while no amount
/// classification was made (the category stayed a message or a post), or while no field
/// under a strong recipient head names someone (`Recipient not verified`).
fn sent_amount_unclear(
    action: &Action,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
    read: RiskCategory,
    category: RiskCategory,
) -> bool {
    let Some((form, field)) = actuated_form(action, screen, tree) else {
        return false;
    };
    let fields = form_values(&form, field, screen, tree);
    if !fields.iter().any(|(_, v)| number_shaped(v)) {
        return false;
    }
    let not_money = !matches!(category, RiskCategory::Payment | RiskCategory::Purchase);
    let verified = fields
        .iter()
        .any(|(name, v)| strong_recipient_field_name(name.trim()) && names_someone(v));
    (not_money && matches!(read, RiskCategory::SendMessage | RiskCategory::PublicPost)) || !verified
}

/// Fourteenth review (Rule A): the links `action` navigates to — `open_url`'s; the text
/// Return (or the keyboard's Go/Search/Return key) submits in a field when it is
/// link-shaped ([`link_shaped`]: the typed text, else the field's current value), in any
/// field; and every link in a tool's arguments.
fn navigated_links(action: &Action, screen: &CompactScreen) -> Vec<String> {
    match action {
        Action::OpenUrl { url } => vec![url.clone()],
        Action::CallTool { args, .. } => {
            let mut strings = Vec::new();
            crate::classify::tool_strings(args, &mut strings);
            let mut links: Vec<String> = Vec::new();
            for s in strings {
                if looks_like_url(&s) || link_shaped(&s) {
                    links.push(s.trim().to_owned());
                } else {
                    // Fifteenth review (F6): a link inside a sentence, in brackets or after
                    // a label is a link too.
                    links.extend(crate::shape::links_in(&s));
                }
            }
            links
        }
        // Return (`actuation`'s `Return`; matched on the action so other actions pay nothing).
        Action::Type {
            element,
            text,
            submit: true,
        } => submitted_link(type_target(*element, screen), text)
            .into_iter()
            .collect(),
        // The keyboard's own submit key, on a screen that lists keys.
        Action::Tap { element } | Action::LongPress { element, .. } => match screen.get(*element) {
            Some(k) if k.role == Role::Key && submit_key_label(&k.label) => {
                let field = screen
                    .elements
                    .iter()
                    .find(|e| e.focused && e.role.is_text_input());
                submitted_link(field, "").into_iter().collect()
            }
            _ => Vec::new(),
        },
        Action::Type { submit: false, .. }
        | Action::Clear { .. }
        | Action::Scroll { .. }
        | Action::Swipe { .. }
        | Action::TapPoint { .. }
        | Action::PressHome
        | Action::OpenApp { .. }
        | Action::Wait { .. }
        | Action::AskUser { .. }
        | Action::Look
        | Action::Done { .. }
        | Action::Fail { .. } => Vec::new(),
    }
}

/// Whether a keyboard key's label submits its field (Return, Go, Search, …).
fn submit_key_label(label: &str) -> bool {
    let l = label.trim().to_lowercase();
    l == "return" || l == "enter" || SUBMIT_RETURN_KEYS.contains(&l.as_str())
}

/// The link Return submits in `field` after typing `text`: the typed text when it is
/// link-shaped, else the field's current value when that is (never a secure field's).
fn submitted_link(field: Option<&Element>, text: &str) -> Option<String> {
    if link_shaped(text) {
        return Some(text.trim().to_owned());
    }
    let f = field.filter(|f| !is_secure_input(f))?;
    // Fifteenth review: what the field holds once `text` is typed after what it holds (a
    // link typed in two steps is one link).
    let all = after_text(Some(f), text);
    if link_shaped(&all) {
        return Some(all.trim().to_owned());
    }
    field_value(f).filter(|v| link_shaped(v)).map(str::to_owned)
}

/// What field `field` holds after `text` is typed into it (fifteenth review): its value
/// and then `text`, or its value alone when it already ends with `text` (read on the screen
/// after the typing). A secure field's or no field's: `text`.
fn after_text(field: Option<&Element>, text: &str) -> String {
    let prior = field
        .filter(|f| !is_secure_input(f) && !f.secure)
        .and_then(field_value)
        .unwrap_or_default();
    let t = text.trim();
    if !t.is_empty() && prior.ends_with(t) {
        prior.to_owned()
    } else {
        format!("{prior}{text}")
    }
}

/// Fewest digits of a plain digit run read as a phone or account number rather than an
/// amount, in the "who plus how much" rule of [`presumed_money`].
const MIN_PHONE_LIKE_DIGITS: usize = 7;

/// A value that is a money amount with its currency ("$900", "900.00 EUR") and nothing
/// else (thirteenth review: makes a presumed submit `Payment` whatever the field's name).
fn money_value(v: &str) -> bool {
    money_or_number(v) && has_money(v)
}

/// Whether a value is shown as an amount (seventh review: a money amount or a plain or
/// grouped number; fourteenth review, Rule D: or money by its shape, `R$ 900,00`, `900,-`).
/// Number words count as amounts for the category (Rule D) but are not quoted as one.
fn amount_shown(v: &str) -> bool {
    money_or_number(v) || money_shaped(v)
}

/// Fourteenth review (Rule C): the category the actuated control's **own** words give —
/// its label (and value, for a non-input) read through the action phrases without the
/// screen's context; for Return, the Return key's label. `None` for anything else. A
/// category the gate reaches otherwise (a presumption, the screen's context) on a presumed
/// submit is a guess.
fn own_category(action: &Action, screen: &CompactScreen) -> RiskCategory {
    let tables = crate::phrases::action_tables(&screen.app_bundle_id);
    let own = |text: &str| crate::phrases::match_label(text, &tables).category;
    match actuation(action, screen) {
        Actuation::Press(t) | Actuation::Drag(t) => own(&crate::classify::element_text(t)),
        Actuation::Return(_) => screen
            .cues
            .return_key
            .as_deref()
            .map_or(RiskCategory::None, own),
        Actuation::PressUnknown | Actuation::Navigation | Actuation::NoEffect => RiskCategory::None,
    }
}

/// Words of an element's text that make a screen showing money a payment context (R3).
const PAYMENT_CONTEXT_WORDS: &[&str] = &[
    "amount", "total", "fee", "fees", "price", "pay", "payment", "transfer", "balance", "due",
    "charge", "wallet", "gas",
];

/// Most characters of an element's text R3 reads.
const MAX_MONEY_TEXT_CHARS: usize = 200;

/// Fifteenth review (R3): whether anything on `screen` — every element's label and value,
/// static texts included — is money by shape ([`money_shaped`], [`has_money`]); `Some(true)`
/// when an element also names a payment context ("Amount", "Network fee", "Total").
fn screen_shows_money(screen: &CompactScreen) -> Option<bool> {
    screen_money(screen, true)
}

/// [`screen_shows_money`] with the payment context read only when `want_context`; without it
/// the scan stops at the first money found (the answer is then `Some(false)` at least).
fn screen_money(screen: &CompactScreen, want_context: bool) -> Option<bool> {
    let mut money = false;
    let mut context = false;
    for e in &screen.elements {
        if is_secure_input(e) {
            continue;
        }
        for t in [Some(e.label.as_str()), e.value.as_deref()]
            .into_iter()
            .flatten()
            .map(|t| t.get(..t.len().min(MAX_MONEY_TEXT_CHARS)).unwrap_or(t))
        {
            if t.is_empty() {
                continue;
            }
            money |= money_shaped(t) || has_money(t) || crate::shape::symbol_amount(t);
            if !context && (want_context || !money) {
                context = crate::text::tokens(t)
                    .iter()
                    .any(|w| PAYMENT_CONTEXT_WORDS.contains(&w.as_str()));
            }
            if money && (context || !want_context) {
                return Some(context);
            }
        }
    }
    money.then_some(context)
}

/// A control inside an alert, a system sheet or a notification banner (R4).
struct AlertTap {
    /// The alert's title (its label, else its first text), as the screen shows it.
    title: String,
    /// It reads as a permission or trust prompt.
    permission: bool,
    /// The control's whole label is a plain dismiss ("Cancel", "Don't Allow", …).
    dismiss: bool,
}

/// Whole labels of an alert's buttons that only dismiss it (R4: compared whole, folded).
const ALERT_DISMISS_LABELS: &[&str] = &[
    "cancel",
    "don't allow",
    "dont allow",
    "do not allow",
    "not now",
    "close",
    "dismiss",
    "later",
];

/// Phrases of an alert's texts that make it a permission or trust prompt (R4).
const PERMISSION_SHAPES: &[&str] = &[
    "would like to",
    "wants to",
    "trust this computer",
    "track your activity",
    "paste from",
    "local network",
    "bluetooth",
    "notifications",
    "your location",
    "use location",
    "contacts",
    "photos",
    "camera",
    "microphone",
    "face id",
    "allow ",
];

/// Whether role `role` is an alert container (R4): an alert anywhere; a sheet or a
/// notification banner on SpringBoard, which owns them.
fn alert_role(role: &Role, springboard: bool) -> bool {
    match role {
        Role::Alert => true,
        Role::Sheet => springboard,
        Role::Other(name) => {
            let n = name.to_lowercase();
            springboard && (n.contains("banner") || n.contains("notification"))
        }
        _ => false,
    }
}

/// Fifteenth review (R4): the alert (or SpringBoard sheet or banner) the control `action`
/// presses sits in — by the raw tree's ancestors' roles (facts from the OS), else a listed
/// alert whose frame holds the control.
fn alert_of(
    action: &Action,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
) -> Option<AlertTap> {
    let target = match action {
        Action::Tap { element } | Action::LongPress { element, .. } => screen.get(*element)?,
        Action::TapPoint { x, y } => crate::classify::tap_point(*x, *y, screen).1?,
        _ => return None,
    };
    if target.role.is_text_input() || target.role == Role::Key || is_alert_like(target, screen) {
        return None;
    }
    let springboard = crate::app::is_springboard(&screen.app_bundle_id);
    let mut texts: Vec<String> = Vec::new();
    let mut found = false;
    if let Some(t) = tree.filter(|t| !t.nodes.is_empty()) {
        if let Some(container) = t
            .ancestors(target)
            .into_iter()
            .find(|a| crate::hit::raw_node(t, *a).is_some_and(|n| alert_role(&n.role, springboard)))
        {
            found = true;
            let end = usize::try_from(container)
                .ok()
                .and_then(|i| t.ends.get(i))
                .copied()
                .unwrap_or(container);
            if let Some(l) = crate::hit::raw_node(t, container).and_then(|n| n.label.clone()) {
                texts.push(l);
            }
            for n in t
                .nodes
                .iter()
                .filter(|n| n.node > container && n.node < end && n.role == Role::StaticText)
                .take(8)
            {
                if let Some(l) = &n.label {
                    texts.push(l.clone());
                }
            }
            // Listed texts inside it (a raw node's label may be withheld).
            for e in screen
                .elements
                .iter()
                .filter(|e| e.role == Role::StaticText && t.inside(e, container))
                .take(8)
            {
                texts.push(e.label.clone());
            }
        }
    }
    if !found {
        // Without the raw tree (or when the tree places no alert above it): a listed alert
        // whose frame holds the control, else any listed alert (an alert is modal: what can
        // be tapped while one is up is the alert's).
        let c = target.frame.center();
        let alerts = || {
            screen
                .elements
                .iter()
                .filter(|e| alert_role(&e.role, springboard))
        };
        if tree.is_some_and(|t| !t.nodes.is_empty()) && alerts().all(|a| !a.frame.contains(c)) {
            return None;
        }
        let alert = alerts()
            .filter(|e| e.frame.contains(c))
            .min_by(|a, b| a.frame.area().total_cmp(&b.frame.area()))
            .or_else(|| alerts().next())?;
        texts.push(alert.label.clone());
        let inside: Vec<&Element> = screen
            .elements
            .iter()
            .filter(|e| e.role == Role::StaticText && alert.frame.contains(e.frame.center()))
            .take(8)
            .collect();
        let shown: Vec<&Element> = if inside.is_empty() {
            screen
                .elements
                .iter()
                .filter(|e| e.role == Role::StaticText)
                .take(3)
                .collect()
        } else {
            inside
        };
        texts.extend(shown.into_iter().map(|e| e.label.clone()));
    }
    texts.retain(|t| !t.trim().is_empty());
    texts.dedup();
    let all = texts.join(" ").to_lowercase().replace('\u{2019}', "'");
    let permission = PERMISSION_SHAPES.iter().any(|p| all.contains(p));
    let label: String = crate::text::tokens(&target.label.replace('\u{2019}', "'")).join(" ");
    let whole = target
        .label
        .trim()
        .trim_matches(|c: char| c.is_ascii_punctuation() && c != '\'')
        .to_lowercase()
        .replace('\u{2019}', "'");
    let dismiss = ALERT_DISMISS_LABELS.contains(&whole.as_str())
        || ALERT_DISMISS_LABELS.contains(&label.as_str());
    Some(AlertTap {
        title: texts.into_iter().next().unwrap_or_default(),
        permission,
        dismiss,
    })
}

/// Fifteenth review (R4b): whether the control `action` presses is inside an alert, a
/// SpringBoard sheet or a notification banner (never a home-screen launch).
pub fn in_alert(action: &Action, screen: &CompactScreen, tree: Option<&ScreenTree<'_>>) -> bool {
    alert_of(action, screen, tree).is_some()
}

/// Whether `e` is itself an alert container (a tap on the alert, not on a control in it).
fn is_alert_like(e: &Element, screen: &CompactScreen) -> bool {
    alert_role(&e.role, crate::app::is_springboard(&screen.app_bundle_id))
}

/// The rows a question about a tap inside an alert carries (R4): what it answers, and the
/// alert's title under `Screen says:` (page provenance: the title is on-screen text).
fn alert_rows(
    action: &Action,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
) -> Vec<QuestionRow> {
    let Some(a) = alert_of(action, screen, tree).filter(|a| !a.dismiss) else {
        return Vec::new();
    };
    let what = if a.permission {
        MARK_PERMISSION
    } else {
        MARK_ALERT
    };
    let mut rows = vec![row(QuestionRowKind::Note, &format!(". {what}"))];
    if !a.title.trim().is_empty() {
        rows.push(row(
            QuestionRowKind::ScreenSays,
            &format!(". {MARK_SCREEN_SAYS}: \"{}\"", clean(&a.title, 80)),
        ));
    }
    rows
}

/// The control a tap or long press names (a `tap_point`'s is resolved by the classifier and
/// never narrowed) when it is a browser's "Forward" (ninth review, narrowing (b)): its
/// label is only "Forward" ("Go Forward", "Forward Page") and it sits in a system bar that
/// also holds a "Back" control. A "Forward" anywhere else (Mail's toolbar, a message's
/// menu) stays a send phrase.
fn browser_forward(
    action: &Action,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
) -> Option<u32> {
    let (Action::Tap { element } | Action::LongPress { element, .. }) = action else {
        return None;
    };
    let t = screen.get(*element)?;
    let toks = crate::text::tokens(&t.label);
    let only_forward = toks.iter().any(|w| w == "forward")
        && toks
            .iter()
            .all(|w| matches!(w.as_str(), "forward" | "go" | "page"));
    (only_forward && crate::hit::in_bar_with_back(t, screen, tree)).then_some(t.index)
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
    ConfirmRequest::one_line(
        RiskCategory::FirstUse,
        QuestionRowKind::Action,
        sanitize_screen_text_with_limit(&summary, MAX_SUMMARY_CHARS),
        bundle_id.to_owned(),
    )
}

/// Redacts, sanitises and caps `s`, turns `"` (and the quotation marks that look like it)
/// into `'` so it can sit inside quotes, and neutralises the question's own markers
/// ([`defang`], [`neutralise_markers`]).
pub(crate) fn clean(s: &str, max: usize) -> String {
    let quoted: String = sanitize_screen_text_with_limit(&redact(s), max)
        .chars()
        .map(|c| match c {
            '"' | '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' | '\u{02DD}'
            | '\u{3003}' | '\u{FF02}' | '\u{301D}' | '\u{301E}' => '\'',
            c => c,
        })
        .collect();
    neutralise_markers(&defang(&quoted))
}

/// The headings of the question's facts (eleventh review: one list, the composer writes
/// these and [`defang`] reads their last words, so page text can never write one).
const MARK_AMOUNTS: &str = "Amounts on screen";
const MARK_RECIPIENTS: &str = "To/Account";
const MARK_SCREEN_SAYS: &str = "Screen says";
const MARK_SENDS: &str = "Sends";
const MARK_HOLDS: &str = "The form holds";
const MARK_TYPED: &str = "Typed";
const MARK_SELECTED: &str = "Selected";
const MARK_FIELDS: &str = "Fields";
/// The question's other markers (eleventh review): phrases Jarvis writes that page text
/// must not reproduce ([`neutralise_markers`]; the title and site markers end in `"`, which
/// page text never keeps).
const MARK_NOT_VERIFIED: &str = "Recipient not verified";
const MARK_SHOWN_ABOVE: &str = "(shown above)";
const MARK_LOOKALIKE: &str = "(look-alike letters)";
/// Fourteenth review (Rule I): a link whose host follows a `user@` part.
const MARK_USER_INFO: &str = "(a user name before the host, not shown)";
const MARK_NEAREST: &str = "(nearest)";
const MARK_TITLE: &str = "screen \"";
const MARK_SITE: &str = "site \"";
/// Twelfth review: `. Screen says: N more lines naming a recipient` (was `N not shown`).
const MARK_MORE_SAYS: &str = "more lines naming a recipient";
/// Fifteenth review (R1): model text that is not the owner's.
const MARK_NOT_OWNER: &str = "This text is not from your request";
/// Fifteenth review (R4): a tap inside an alert.
const MARK_ALERT: &str = "This answers an alert";
const MARK_PERMISSION: &str = "This answers a system alert and may grant a permission";
/// Sixteenth review (Rule T): what was read elsewhere could reach this place.
const MARK_FLOW: &str = "what it read there could be passed on here";
/// Sixteenth review (Rule P): a control that inserts content the question cannot show.
const MARK_PASTE: &str = "content Jarvis cannot show";

/// Every heading the question writes, each followed by a colon (or ` (N …):`) in a
/// question (eleventh review: the single source the composer, [`defang`] and the property
/// tests use).
pub const QUESTION_HEADINGS: &[&str] = &[
    MARK_AMOUNTS,
    MARK_RECIPIENTS,
    MARK_SCREEN_SAYS,
    MARK_SENDS,
    MARK_HOLDS,
    MARK_TYPED,
    MARK_SELECTED,
    MARK_FIELDS,
];

/// Every marker the question writes besides [`QUESTION_HEADINGS`] (eleventh review).
pub const QUESTION_MARKERS: &[&str] = &[
    MARK_NOT_VERIFIED,
    MARK_SHOWN_ABOVE,
    MARK_LOOKALIKE,
    MARK_USER_INFO,
    MARK_NEAREST,
    MARK_TITLE,
    MARK_SITE,
    MARK_MORE_SAYS,
    MARK_NOT_OWNER,
    MARK_ALERT,
    MARK_PERMISSION,
    MARK_FLOW,
    MARK_PASTE,
];

/// Twelfth review: the other words the composer writes at the start of the question or of
/// one of its parts (the action line's verbs, the runner-text warning, the cap note). With
/// [`QUESTION_HEADINGS`] and [`QUESTION_MARKERS`] they are what a page-derived name must
/// not read as ([`heading_like_name`]).
pub const QUESTION_KEYWORDS: &[&str] = &[
    "Tap",
    "Long-press",
    "Turn on",
    "Turn off",
    "Type",
    "Press Return",
    "Clear",
    "Scroll",
    "Swipe",
    "Run",
    "Open",
    "Warning",
    "Facts from",
];

/// The lower-cased words of `s` with look-alikes folded (letters and digits only).
fn folded_words(s: &str) -> Vec<String> {
    let folded: String = s
        .chars()
        .take(4 * MAX_FIELD_NAME_CHARS)
        .map(crate::text::fold_char)
        .collect::<String>()
        .to_lowercase();
    folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Twelfth review (F3): whether a page-derived name (a field's, an option's, a tool
/// argument's key, a role's) reads like something the composer writes — after look-alike
/// folding its words equal, start with or end with the words of an entry of
/// [`QUESTION_HEADINGS`], [`QUESTION_MARKERS`] or [`QUESTION_KEYWORDS`], or its letters end
/// with that entry's letters (glued: "MySends"), or it holds any letter outside ASCII (no
/// look-alike table decides). Such a name is always shown in quotes, and
/// a real heading never is, so a quoted name cannot pass for one.
pub fn heading_like_name(name: &str) -> bool {
    // A letter outside ASCII (or a combining mark) is quoted whatever it reads as: no
    // table of look-alikes decides (as `defang` since the eleventh review).
    if name
        .chars()
        .any(|c| crate::text::non_ascii_letter(c) || crate::text::is_combining(c))
    {
        return true;
    }
    let words = folded_words(name);
    if words.is_empty() {
        return false;
    }
    let glued: String = words.concat();
    static ENTRIES: std::sync::OnceLock<Vec<(Vec<String>, String)>> = std::sync::OnceLock::new();
    let entries = ENTRIES.get_or_init(|| {
        QUESTION_HEADINGS
            .iter()
            .chain(QUESTION_MARKERS)
            .chain(QUESTION_KEYWORDS)
            .map(|entry| {
                let e = folded_words(entry);
                let g = e.concat();
                (e, g)
            })
            .filter(|(e, _)| !e.is_empty())
            .collect()
    });
    entries
        .iter()
        .any(|(e, g)| words.starts_with(e) || words.ends_with(e) || glued.ends_with(g.as_str()))
}

/// `name` in double quotes when it is [`heading_like_name`] (its own `"` already turned
/// into another mark by cleaning), else as it is.
fn quote_heading_like(name: String) -> String {
    if heading_like_name(&name) {
        format!("\"{name}\"")
    } else {
        name
    }
}

/// The phrase markers page text could otherwise spell out ([`neutralise_markers`]).
const PHRASE_MARKERS: &[&str] = &[
    MARK_NOT_VERIFIED,
    MARK_SHOWN_ABOVE,
    MARK_LOOKALIKE,
    MARK_USER_INFO,
    MARK_NEAREST,
    MARK_MORE_SAYS,
    MARK_NOT_OWNER,
    MARK_ALERT,
    MARK_PERMISSION,
    MARK_FLOW,
    MARK_PASTE,
];

/// Whether `c` reads as a colon ([`crate::text::colon_shaped`]; eleventh review: every
/// colon-shaped character, not a list of look-alikes seen so far).
fn colon_like(c: char) -> bool {
    crate::text::colon_shaped(c)
}

/// Whether the 1–3 words right before the end of `out` (at most 64 characters back) hold a
/// letter outside ASCII (eleventh review: such a run is never one of Jarvis's headings, so a
/// colon after it is page text posing as one — Cherokee `Ꭺ`, mathematical `𝐒`, Lisu, …).
fn non_ascii_run(out: &[char]) -> bool {
    let mut words = 0usize;
    let mut in_word = false;
    for &c in out.iter().rev().take(64) {
        if c.is_whitespace() {
            if in_word {
                words += 1;
                in_word = false;
                if words >= 3 {
                    break;
                }
            }
            continue;
        }
        in_word = true;
        if crate::text::non_ascii_letter(c) {
            return true;
        }
    }
    false
}

/// `s` with the question's phrase markers ([`PHRASE_MARKERS`]) made visibly different
/// wherever page text spells one out (case and look-alikes folded): its parentheses are
/// dropped and its spaces become hyphens (`shown-above`, `Recipient-not-verified`; brackets
/// would be turned back into parentheses by sanitising), so only Jarvis writes them
/// (eleventh review).
fn neutralise_markers(s: &str) -> String {
    let mut chars: Vec<char> = s.chars().collect();
    let folded: Vec<char> = chars
        .iter()
        .map(|&c| match c {
            '\u{207D}' | '\u{208D}' | '\u{FE59}' | '\u{2768}' | '\u{276A}' | '\u{2E28}' => '(',
            '\u{207E}' | '\u{208E}' | '\u{FE5A}' | '\u{2769}' | '\u{276B}' | '\u{2E29}' => ')',
            c if c.is_whitespace() => ' ',
            c => crate::text::fold_char(c),
        })
        .collect();
    for m in PHRASE_MARKERS {
        let m: Vec<char> = m.to_lowercase().chars().collect();
        let Some(first) = m.first().copied() else {
            continue;
        };
        let mut i = 0usize;
        while i + m.len() <= folded.len() {
            // Sixteenth review: a letter outside ASCII stands for any letter of the marker
            // (no look-alike table decides: Georgian `ყ` for `y` was missed by the fold).
            let same = |k: usize, want: char| {
                folded.get(i + k) == Some(&want)
                    || (want.is_ascii_alphabetic()
                        && chars
                            .get(i + k)
                            .is_some_and(|c| crate::text::non_ascii_letter(*c)))
            };
            if i + m.len() <= folded.len()
                && same(0, first)
                && m.iter().enumerate().all(|(k, w)| same(k, *w))
            {
                for c in chars.iter_mut().skip(i).take(m.len()) {
                    *c = match *c {
                        c if c.is_whitespace() => '-',
                        c if folded_paren(c).is_some() => '\0',
                        c => c,
                    };
                }
                i += m.len();
            } else {
                i += 1;
            }
        }
    }
    chars.into_iter().filter(|c| *c != '\0').collect()
}

/// `(` or `)` for a parenthesis or one of its look-alikes.
fn folded_paren(c: char) -> Option<char> {
    match c {
        '(' | '\u{207D}' | '\u{208D}' | '\u{FE59}' | '\u{2768}' | '\u{276A}' | '\u{2E28}' => {
            Some('(')
        }
        ')' | '\u{207E}' | '\u{208E}' | '\u{FE5A}' | '\u{2769}' | '\u{276B}' | '\u{2E29}' => {
            Some(')')
        }
        _ => None,
    }
}

/// The letters at the end of `folded` (look-alike folded, lower case), skipping trailing
/// whitespace.
fn last_word(folded: &[char]) -> String {
    let end = folded
        .iter()
        .rposition(|c| !c.is_whitespace())
        .map_or(0, |p| p + 1);
    let head = folded.get(..end).unwrap_or_default();
    let start = head
        .iter()
        .rposition(|c| !c.is_alphabetic())
        .map_or(0, |p| p + 1);
    head.get(start..).unwrap_or_default().iter().collect()
}

/// Screen text cannot pose as a summary's fact headings: a colon after the word "screen",
/// "selected", "account", "fields", "sends", "holds" or "typed" ("Amounts on screen:",
/// "Selected:", "To/Account:", "Fields:", "Sends:", "The form holds:", "Typed:"; the last
/// three since the ninth review) is dropped. Tenth review: the letters are compared after
/// look-alike folding ([`crate::text::fold_char`]: Cyrillic, Greek, fullwidth and styled
/// letters), colon look-alikes count as colons, and a heading word followed by a
/// parenthesis ("To/Account (2 different):", "Amounts on screen (3 totals):", "Sends: 4
/// fields (2 shown):") loses its colon too.
///
/// Eleventh review (stop enumerating look-alikes): every colon-shaped character
/// ([`crate::text::colon_shaped`]) counts as a colon; a colon after a run of 1–3 words
/// holding any letter outside ASCII (Cherokee, Lisu, Armenian, Georgian, mathematical
/// letters, an accented letter — no table needed) is replaced by a space; for all-ASCII
/// runs the heading-word test above stays, its words read from [`QUESTION_HEADINGS`].
fn defang(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut kept: Vec<char> = Vec::with_capacity(chars.len());
    let mut folded: Vec<char> = Vec::with_capacity(chars.len());
    for (i, &c) in chars.iter().enumerate() {
        if colon_like(c) && non_ascii_run(&kept) {
            let spaced = kept.last().is_some_and(|p| p.is_whitespace())
                || chars.get(i + 1).is_some_and(|n| n.is_whitespace());
            if !spaced {
                out.push(' ');
                kept.push(' ');
                folded.push(' ');
            }
            continue;
        }
        if colon_like(c) {
            let mut word = last_word(&folded);
            if word.is_empty() {
                // "… (2 different):": the word before the parenthesis.
                let end = folded
                    .iter()
                    .rposition(|c| !c.is_whitespace())
                    .map_or(0, |p| p + 1);
                if end > 0 && folded.get(end - 1) == Some(&')') {
                    if let Some(open) = folded
                        .get(..end)
                        .and_then(|h| h.iter().rposition(|c| *c == '('))
                    {
                        word = last_word(folded.get(..open).unwrap_or_default());
                    }
                }
            }
            // The word may be glued to what is before it ("xSends:"): a heading's word at
            // the end of the letters before the colon is enough (ninth review).
            if heading_word_end(&word) {
                continue;
            }
        }
        out.push(c);
        kept.push(c);
        folded.push(crate::text::fold_char(c));
    }
    out
}

/// Whether `word` (folded, lower case) ends with the last word of one of
/// [`QUESTION_HEADINGS`] ("screen", "account", "says", "sends", "holds", "typed",
/// "selected", "fields").
fn heading_word_end(word: &str) -> bool {
    QUESTION_HEADINGS.iter().any(|h| {
        let last = h
            .rsplit(|c: char| !c.is_alphabetic())
            .find(|w| !w.is_empty())
            .unwrap_or(h);
        word.ends_with(&last.to_lowercase())
    })
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
        let word = crate::text::fold_digits(&word);
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
    let w = crate::text::fold_digits(w);
    w.chars().any(|c| c.is_ascii_digit())
        && w.chars()
            .all(|c| c.is_ascii_digit() || c == ',' || c == '.')
}

/// A currency word ("USD", "euros"), trailing punctuation ignored.
fn is_currency_word(w: &str) -> bool {
    // A lone currency sign beside a number ("5000 $") and a dotted abbreviation in another
    // script ("5,000 د.إ") are currency words too (sixteenth review, F4).
    let mut it = w.chars();
    if let (Some(c), None) = (it.next(), it.next()) {
        if crate::text::is_currency_symbol(c) {
            return true;
        }
    }
    if crate::shape::dotted_abbreviation(w) {
        return true;
    }
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
    let text = crate::text::fold_digits(text);
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

/// `"label"` (at most `max` characters), or the role and index when it has no text.
fn describe_with(el: &Element, max: usize) -> String {
    let text = [Some(el.label.as_str()), el.placeholder.as_deref()]
        .into_iter()
        .flatten()
        .find(|t| !t.trim().is_empty());
    match text {
        Some(t) => format!("\"{}\"", clean_label(t, max)),
        // An `Other` role's name is the tree's text (tenth review: cleaned like a label).
        None => {
            let role = clean(el.role.as_str(), 24);
            // Twelfth review (F3): a role named like a heading is quoted.
            if heading_like_name(&role) {
                format!("'{role}' #{}", el.index)
            } else {
                format!("{role} #{}", el.index)
            }
        }
    }
}

/// The element description (at most `max` characters), or `element N` when it is not on
/// the screen.
fn describe_index_with(screen: &CompactScreen, index: u32, max: usize) -> String {
    screen
        .get(index)
        .map_or_else(|| format!("element {index}"), |e| describe_with(e, max))
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
pub(crate) const PER_PERIOD_WORDS: &[&str] =
    &["year", "yr", "annum", "month", "mo", "week", "wk", "day"];
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
        || (amount_like_name(label, placeholder) && amount_shown(value))
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
    /// The screen's own total: total wording in a text the app shows (not a field's value).
    total: bool,
    /// Tenth review: the text is an exact total phrase ([`TOTAL_PHRASES`]) and its amount,
    /// nothing else ("Order total $512.40"; not "Bundle total value $9,000.00").
    exact: bool,
    /// How close it is to the target ([`Closeness`]).
    near: Closeness,
}

/// How close an element is to the target, smaller first: not in the target's row band,
/// the depth (in the target's raw ancestors, innermost 0) of the nearest container holding
/// both (0 for every element without the raw tree), and the distance between their
/// centres.
#[derive(Clone, Copy)]
struct Closeness {
    off_row: bool,
    container: usize,
    distance: f64,
}

impl Closeness {
    fn cmp(&self, other: &Closeness) -> std::cmp::Ordering {
        self.off_row
            .cmp(&other.off_row)
            .then(self.container.cmp(&other.container))
            .then(self.distance.total_cmp(&other.distance))
    }
}

/// The target and its raw ancestors (innermost first), for [`closeness`].
struct Anchor<'a, 't> {
    target: Option<&'a Element>,
    ancestors: Vec<u32>,
    tree: Option<&'a ScreenTree<'t>>,
}

impl<'a, 't> Anchor<'a, 't> {
    fn new(target: Option<&'a Element>, tree: Option<&'a ScreenTree<'t>>) -> Self {
        let ancestors = match (target, tree) {
            (Some(t), Some(tr)) => tr.ancestors(t),
            _ => Vec::new(),
        };
        Anchor {
            target,
            ancestors,
            tree,
        }
    }

    /// How close `e` is to the target (everything is equally close without a target).
    fn closeness(&self, e: &Element) -> Closeness {
        let Some(t) = self.target else {
            return Closeness {
                off_row: false,
                container: 0,
                distance: 0.0,
            };
        };
        let off_row = t.frame.is_empty() || e.frame.is_empty() || !same_row(&t.frame, &e.frame);
        let container = match self.tree {
            Some(tr) => self
                .ancestors
                .iter()
                .position(|a| tr.inside(e, *a))
                .unwrap_or(self.ancestors.len()),
            None => 0,
        };
        let distance = if t.frame.is_empty() || e.frame.is_empty() {
            f64::MAX
        } else {
            let (a, b) = (t.frame.center(), e.frame.center());
            let d = (a.x - b.x).hypot(a.y - b.y);
            if d.is_finite() {
                d
            } else {
                f64::MAX
            }
        };
        Closeness {
            off_row,
            container,
            distance,
        }
    }
}

/// Exact total phrases (tenth review): a text that is one of these and its amount, nothing
/// else, is the screen's total before any sentence that merely contains "total".
const TOTAL_PHRASES: &[&str] = &[
    "total",
    "order total",
    "amount due",
    "grand total",
    "total due",
    "you pay",
    "pay",
    "total amount",
    "amount to pay",
    "total to pay",
    "due today",
    "total due today",
    "total today",
    "balance due",
    "estimated total",
];

/// Whether `text` is an exact total phrase with its amounts ([`TOTAL_PHRASES`]): its words
/// outside the amounts, look-alike folded, are one of the phrases.
fn exact_total(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let ranges = amount_ranges(&chars);
    if ranges.is_empty() {
        return false;
    }
    let mut rest = String::new();
    let mut at = 0usize;
    for &(a, b) in &ranges {
        rest.extend(chars.get(at..a).unwrap_or_default());
        rest.push(' ');
        at = b;
    }
    rest.extend(chars.get(at..).unwrap_or_default());
    let folded: String = rest.chars().map(crate::text::fold_char).collect();
    let phrase = folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    TOTAL_PHRASES.contains(&phrase.as_str())
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
    // Thirteenth review: a field named by a currency sign or code alone ("€", "USD").
    lone_currency(label)
        || placeholder.is_some_and(lone_currency)
        || name
            .split(|c: char| !c.is_alphanumeric())
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
        && !t.chars().any(crate::text::is_digit)
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
/// Order (tenth review): exact total phrases ([`TOTAL_PHRASES`]), then other texts with
/// total wording, each nearest the target first ([`Closeness`]); then the other charges
/// (field amounts, and texts with charge wording, see `has_charge_words`); then the rest;
/// largest first after that (ties: screen order). The slot shows the first
/// [`MAX_LISTED_AMOUNTS`], always with the first, the nearest total and the largest amount
/// on the screen (see `amounts_slot`).
fn amount_facts(screen: &CompactScreen, anchor: &Anchor<'_, '_>) -> Vec<AmountFact> {
    let target = anchor.target;
    let mut out: Vec<AmountFact> = Vec::new();
    // Each text once (tenth review: a set, not a scan of what is already listed).
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut push = |out: &mut Vec<AmountFact>,
                    e: &Element,
                    text: String,
                    value: f64,
                    charge: bool,
                    field: bool| {
        if seen.insert(text.clone()) {
            let exact = !field && exact_total(&text);
            out.push(AmountFact {
                total: !field && (exact || has_total_words(&text)),
                exact,
                near: anchor.closeness(e),
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
            if let Some(v) = field_value(e).filter(|v| amount_like_field(e) && amount_shown(v)) {
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
                push(&mut out, e, text, value, true, true);
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
        push(&mut out, e, text, value, charge, false);
    }
    // Stable: equal keys keep screen order. The screen's own total first (ninth review: a
    // text with total wording, never a field's value), then the other charges, then the
    // rest. Tenth review: among the totals, exact total phrases ("Order total $512.40")
    // before sentences that merely contain "total", and within each the nearest to the
    // target first (same row, same container, distance), not the largest; the other
    // charges and the rest largest first.
    let tier = |f: &AmountFact| -> u8 {
        if f.exact {
            0
        } else if f.total {
            1
        } else if f.charge {
            2
        } else {
            3
        }
    };
    out.sort_by(|a, b| {
        let (ta, tb) = (tier(a), tier(b));
        ta.cmp(&tb)
            .then_with(|| {
                if ta <= 1 {
                    a.near.cmp(&b.near)
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .then(b.value.total_cmp(&a.value))
    });
    out
}

/// Whether `text` names the total: a word starting with "total" (not "subtotal"), "due" or
/// "grand" ("Order total", "Amount due", "Grand total").
fn has_total_words(text: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .any(|w| w.starts_with("total") || w == "due" || w == "grand")
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
        // Twelfth review (F3): an option named like a heading is quoted.
        let t = quote_heading_like(t);
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
pub(crate) fn money_or_number(v: &str) -> bool {
    let v = crate::text::fold_digits(v);
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
) -> (Vec<String>, usize) {
    // Eleventh review (F9): sets, not scans (was quadratic in listed inputs), and at most
    // `MAX_LISTED_INPUTS` fields read; the rest are counted.
    let sent: std::collections::HashSet<u32> = sent.iter().map(|s| s.index).collect();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();
    let mut more = 0usize;
    for e in screen.elements.iter().filter(|e| {
        is_value_field(e) && !target.is_some_and(|t| t.index == e.index) && !sent.contains(&e.index)
    }) {
        if out.len() >= MAX_LISTED_INPUTS {
            more += 1;
            continue;
        }
        let name = field_name(e);
        // Twelfth review (F3): a heading-like name is quoted (cleaning makes it `'…'`).
        let name = if name.is_empty() {
            "A field".to_owned()
        } else {
            quote_heading_like(name)
        };
        let value = field_value(e);
        let t = match value {
            Some(_) if credential_like_field(e) => format!("{name} (hidden)"),
            Some(v) if amount_like_field(e) && amount_shown(v) => continue,
            Some(v) if raw.get(e.index).is_some_and(|r| typed_here(r, typed)) => {
                format!("{name}: {v}")
            }
            Some(_) => format!("{name} (filled)"),
            None => format!("{name} (empty)"),
        };
        if seen.insert(t.clone()) {
            out.push(t);
        }
    }
    (out, more)
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

/// Twelfth review: a field is **recipient-named** only by a closed rule (the single source
/// the composer and the property tests use, see [`recipient_field_name`]). Its name, look-
/// alikes folded and lower-cased, a trailing `:`/`*`/`(required)`/`(optional)` dropped,
/// starts with one of these heads (whole words)…
///
/// Thirteenth review: the heads are [`RECIPIENT_FIELD_STRONG_HEADS`] (a field that can
/// only name who is paid or messaged) and [`RECIPIENT_FIELD_WEAK_HEADS`] (a field that can
/// be either side of a transfer — the payer's own "Account" as much as the payee's).
pub const RECIPIENT_FIELD_HEADS: &[&str] = &[
    "to",
    "send to",
    "sending to",
    "pay to",
    "paying",
    "transfer to",
    "email to",
    "recipient",
    "recipients",
    "payee",
    "beneficiary",
    "iban",
    "to account",
    "account",
    "account number",
    "account no",
];

/// Thirteenth review: the heads of [`RECIPIENT_FIELD_HEADS`] that name only the other side
/// (who is paid or messaged). Only a field under one of these makes `Recipient not
/// verified` unnecessary.
pub const RECIPIENT_FIELD_STRONG_HEADS: &[&str] = &[
    "to",
    "send to",
    "sending to",
    "pay to",
    "paying",
    "transfer to",
    "email to",
    "recipient",
    "recipients",
    "payee",
    "beneficiary",
    "to account",
];

/// Thirteenth review: the heads of [`RECIPIENT_FIELD_HEADS`] that can be either side of a
/// transfer (the payer's own account as well as the payee's): such a field's value is
/// still shown under `To/Account:` with its own name, but does not stand for a verified
/// recipient. The two lists partition [`RECIPIENT_FIELD_HEADS`] (tested).
pub const RECIPIENT_FIELD_WEAK_HEADS: &[&str] =
    &["account", "account number", "account no", "iban"];

/// …and every word after the head is one of these qualifiers ("Recipient's email",
/// "Payee account number", "To phone") or a number — thirteenth review: a number only
/// after a head of two or more words ("Send to 2") or after another qualifier ("Recipient
/// email 2"), never right after a one-word head ("To 2", the end of a range; "Recipient
/// 2")…
pub const RECIPIENT_FIELD_QUALIFIERS: &[&str] = &[
    "name", "email", "e-mail", "address", "number", "no", "phone", "mobile", "account", "iban",
    "handle", "username", "id", "'s",
];

/// …and no word of the name is one of these: a memo, a reference, the payer's own account,
/// a quoted header or a search is never the recipient ("Note to recipient", "Reference for
/// account statement", "From account", "Amount to send").
pub const RECIPIENT_FIELD_NEGATIVES: &[&str] = &[
    "note",
    "memo",
    "message",
    "reference",
    "ref",
    "subject",
    "description",
    "comment",
    "reason",
    "from",
    "reply",
    "cc",
    "bcc",
    "search",
    "filter",
    "amount",
];

/// The words of a field name for [`recipient_field_name`]: look-alikes folded, lower case,
/// a trailing `:`, `*`, `(required)` or `(optional)` dropped, split at anything but
/// letters, digits, `-` and `'`; a possessive `'s` (`’s`) is its own word.
fn recipient_name_words(name: &str) -> Vec<String> {
    let mut folded: String = name
        .chars()
        .take(MAX_FIELD_NAME_CHARS * 2)
        .map(|c| match c {
            '\u{2019}' | '\u{2018}' | '`' => '\'',
            c => crate::text::fold_char(c),
        })
        .collect::<String>()
        .to_lowercase();
    loop {
        let t = folded.trim_end();
        let t = t
            .strip_suffix(':')
            .or_else(|| t.strip_suffix('*'))
            .or_else(|| t.strip_suffix("(required)"))
            .or_else(|| t.strip_suffix("(optional)"));
        match t {
            Some(rest) => folded = rest.to_owned(),
            None => break,
        }
    }
    let mut words = Vec::new();
    for w in folded.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '\'')) {
        let w = w.trim_matches(|c| c == '-' || c == '\'');
        if w.is_empty() {
            continue;
        }
        match w.strip_suffix("'s") {
            Some(stem) if !stem.trim_matches('\'').is_empty() => {
                words.push(stem.trim_matches('\'').to_owned());
                words.push("'s".to_owned());
            }
            _ => words.extend(w.split('\'').filter(|p| !p.is_empty()).map(str::to_owned)),
        }
    }
    words
}

/// Twelfth review: whether a field named `name` names who is paid or messaged — its name is
/// head-anchored on [`RECIPIENT_FIELD_HEADS`], the rest only [`RECIPIENT_FIELD_QUALIFIERS`],
/// and no word is one of [`RECIPIENT_FIELD_NEGATIVES`] ("To", "Pay to", "Recipient's
/// email", "IBAN"; not "Note to recipient", "From account", "Message to Mallory"). Only
/// such a field's value is asserted under `To/Account:`.
pub fn recipient_field_name(name: &str) -> bool {
    recipient_field_head(name, RECIPIENT_FIELD_HEADS)
}

/// Thirteenth review: whether a field named `name` is recipient-named
/// ([`recipient_field_name`]) under a **strong** head ([`RECIPIENT_FIELD_STRONG_HEADS`]):
/// it can only name who is paid or messaged ("To", "Payee", "Recipient's email"; not
/// "Account", "IBAN").
pub fn strong_recipient_field_name(name: &str) -> bool {
    recipient_field_head(name, RECIPIENT_FIELD_STRONG_HEADS)
}

/// The closed rule of [`recipient_field_name`] over the heads `heads`.
fn recipient_field_head(name: &str, heads: &[&str]) -> bool {
    let words = recipient_name_words(name);
    if words.is_empty()
        || words
            .iter()
            .any(|w| RECIPIENT_FIELD_NEGATIVES.contains(&w.as_str()))
    {
        return false;
    }
    let number = |w: &str| w.chars().all(|c| c.is_ascii_digit());
    heads.iter().any(|h| {
        let head: Vec<&str> = h.split(' ').collect();
        words.len() >= head.len()
            && words.iter().zip(&head).all(|(w, h)| w == h)
            && words.iter().enumerate().skip(head.len()).all(|(k, w)| {
                RECIPIENT_FIELD_QUALIFIERS.contains(&w.as_str())
                    // Thirteenth review: a number qualifies a head of two or more words,
                    // or follows a qualifier, never a one-word head ("To 2").
                    || (number(w)
                        && (head.len() >= 2
                            || (k > head.len()
                                && words.get(k - 1).is_some_and(|p| {
                                    RECIPIENT_FIELD_QUALIFIERS.contains(&p.as_str())
                                }))))
            })
    })
}

/// Headings that introduce who is paid or messaged, lower case ("Pay to: Mallory's Merch
/// LLC", "To: Alice", "Account: Checking ••••4521").
const RECIPIENT_HEADINGS: &[&str] = &[
    "pay to",
    "paying",
    "payee",
    "recipient",
    "recipients",
    "send to",
    "sending to",
    "transfer to",
    "to",
    "account",
    "beneficiary",
    "iban",
];

/// The recipient heading `text` starts with (case-insensitive, whole words), and the rest.
fn split_recipient_heading(text: &str) -> Option<&str> {
    let lower = text.trim().to_lowercase();
    RECIPIENT_HEADINGS.iter().find_map(|h| {
        let rest = lower.strip_prefix(h)?;
        if rest.chars().next().is_some_and(char::is_alphanumeric) {
            return None;
        }
        let skip = text.trim().len().checked_sub(rest.len())?;
        text.trim().get(skip..)
    })
}

/// Whether `text` is a recipient heading followed by `:` and a name: "Pay to:
/// Mallory's Merch LLC", "To: @alex" (ninth review: a merchant's name is a recipient too).
fn recipient_heading(text: &str) -> bool {
    split_recipient_heading(text).is_some_and(|rest| {
        let rest = rest.trim_start();
        rest.strip_prefix(':')
            .is_some_and(|a| a.chars().any(char::is_alphanumeric))
    })
}

/// Whether `text` is a recipient heading alone ("Pay to", "Recipient:"), whose name is the
/// text next to or under it.
fn recipient_heading_alone(text: &str) -> bool {
    split_recipient_heading(text).is_some_and(|rest| rest.trim().trim_end_matches(':').is_empty())
}

/// Whether `text` has a word of `table`.
fn has_word(text: &str, table: &[&str]) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .any(|w| table.contains(&w.as_str()))
}

/// Starts of a quoted message's header lines (tenth review): a text starting with one of
/// these (whole words, look-alikes folded), or holding the word "wrote", is never a
/// recipient ("From: dave@…", "Sent from alice@…", "Cc: carol@…", "On Mon, bob@… wrote:").
const QUOTE_STARTS: &[&str] = &["from", "sent from", "cc", "bcc", "reply to", "reply-to"];

/// Whether `text` reads as a quoted message's header line ([`QUOTE_STARTS`]).
fn quoted_header(text: &str) -> bool {
    let folded: String = text.trim().chars().map(crate::text::fold_char).collect();
    let starts = QUOTE_STARTS.iter().any(|q| {
        folded
            .strip_prefix(q)
            .is_some_and(|rest| !rest.chars().next().is_some_and(char::is_alphanumeric))
    });
    starts || has_word(&folded, &["wrote"])
}

/// The proper recipient headings on `screen` (not the target, never a secure or protected
/// field's text), nearest to the target first (screen order without a target): a recipient
/// heading followed by a colon and a name ("Pay to: Mallory's Merch LLC", "To: Alice
/// Smith"), or a heading alone ("Pay to") joined with the static text beside it or right
/// under it. Tenth review: nothing else is a recipient — free text that merely mentions
/// "to" or "account" next to an address, a phone number or an `@handle` (a chat bubble,
/// a quoted mail line) is not, and a quoted header ([`quoted_header`]) never is; the
/// fields of the target's form named like a recipient are listed before these (see
/// `summarize_with`). Money texts are left to the amounts. All are returned: the list
/// shows [`MAX_LISTED_RECIPIENTS`] and counts the rest.
fn recipient_facts(screen: &CompactScreen, anchor: &Anchor<'_, '_>) -> Vec<String> {
    let target = anchor.target;
    let scrollers = Scrollers::new(anchor);
    let mut ranked: Vec<(Closeness, usize, String)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let els = &screen.elements;
    let mut joined: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for (k, e) in els.iter().enumerate().filter(|(_, e)| {
        !e.secure
            && e.role != Role::Key
            && !credential_like_field(e)
            && !target.is_some_and(|t| t.index == e.index)
    }) {
        if joined.contains(&e.index) {
            continue;
        }
        let mut t = fact_text(e).trim().to_owned();
        // A heading alone ("Pay to") with the payee's name next to or right under it
        // (ninth review): one fact, "Pay to: Mallory's Merch LLC".
        if e.role == Role::StaticText && recipient_heading_alone(&t) {
            if let Some(n) = els.get(k.saturating_add(1)).filter(|n| {
                n.role == Role::StaticText
                    && !target.is_some_and(|tg| tg.index == n.index)
                    && !n.label.trim().is_empty()
                    && (same_row(&n.frame, &e.frame)
                        || (n.frame.y >= e.frame.max_y() - 1.0
                            && n.frame.y - e.frame.max_y() <= STACKED_LABEL_GAP))
            }) {
                t = format!("{}: {}", t.trim_end_matches(':').trim(), n.label.trim());
                joined.insert(n.index);
            }
        }
        if t.is_empty() || has_money(&t) || !recipient_heading(&t) || quoted_header(&t) {
            continue;
        }
        // Eleventh review (F3b): a pair in a scrolling container that does not hold the
        // target (a transcript above a compose bar, a feed above a fixed pay bar) is not
        // taken.
        if scrollers.apart(e) {
            continue;
        }
        if !seen.insert(t.clone()) {
            continue;
        }
        ranked.push((anchor.closeness(e), k, t));
    }
    ranked.sort_by(|a, b| a.0.distance.total_cmp(&b.0.distance).then(a.1.cmp(&b.1)));
    ranked.into_iter().map(|(_, _, t)| t).collect()
}

/// The scrolling containers of the raw tree (eleventh review, F3b): scroll, table,
/// collection and web views, from the drawn nodes the tree knows (`ScreenTree::nodes`).
struct Scrollers<'a, 't> {
    anchor: &'a Anchor<'a, 't>,
    nodes: std::collections::HashSet<u32>,
    target: Option<u32>,
}

/// Most raw ancestors walked from one element to the one it shares with the target.
const MAX_SCROLLER_WALK: usize = 4_096;

impl<'a, 't> Scrollers<'a, 't> {
    fn new(anchor: &'a Anchor<'a, 't>) -> Self {
        let nodes = anchor
            .tree
            .map_or_else(std::collections::HashSet::new, |t| {
                t.nodes
                    .iter()
                    .filter(|n| {
                        matches!(
                            n.role,
                            Role::ScrollView | Role::Table | Role::CollectionView | Role::WebView
                        )
                    })
                    .map(|n| n.node)
                    .collect()
            });
        let target = match (anchor.target, anchor.tree) {
            (Some(t), Some(tr)) => tr.source(t),
            _ => None,
        };
        Scrollers {
            anchor,
            nodes,
            target,
        }
    }

    /// Whether `e` sits in a scrolling container that does not also hold the target (its
    /// raw ancestors up to the first one holding the target). Unknown without the tree or
    /// a target: never.
    fn apart(&self, e: &Element) -> bool {
        let (Some(tree), Some(target)) = (self.anchor.tree, self.target) else {
            return false;
        };
        if self.nodes.is_empty() {
            return false;
        }
        let Some(mut at) = tree.source(e) else {
            return false;
        };
        for _ in 0..MAX_SCROLLER_WALK {
            let Some(up) = tree.parent_of(at) else {
                return false;
            };
            if up == target || tree.node_inside(target, up) {
                return false;
            }
            if self.nodes.contains(&up) {
                return true;
            }
            at = up;
        }
        // Deeper than the walk: not taken (only costs an entry).
        true
    }
}

/// The name a recipient entry gives, for counting distinct recipients (tenth review): a
/// heading pair's name after its heading, a field's value; letters, digits and `@`,
/// look-alike folded. Empty when it names no one.
fn recipient_key(name: &str) -> String {
    name.chars()
        .map(crate::text::fold_char)
        .filter(|c| c.is_alphanumeric() || *c == '@')
        .collect()
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
    /// Fields beyond the ones read (eleventh review: at most [`MAX_LISTED_INPUTS`]).
    fields_more: usize,
    /// Tenth review: the facts were read from part of a large screen only
    /// ([`MAX_FACT_ELEMENTS`]): says so, first among the other facts.
    note: String,
}

/// `Heading: a; b; c and N more` with at most `max` entries, each `entry`-formatted; with
/// `entry` 0 (no room) `Heading: N not shown`. Never silent about what it leaves out.
fn fact_list(
    heading: &str,
    items: &[String],
    extra: usize,
    max: usize,
    entry: usize,
) -> Option<String> {
    if items.is_empty() && extra == 0 {
        return None;
    }
    if entry == 0 || items.is_empty() {
        return Some(format!("{heading}: {} not shown", items.len() + extra));
    }
    let shown: Vec<String> = items
        .iter()
        .take(max)
        .map(|t| fact_entry(t, entry))
        .collect();
    let more = items.len().saturating_sub(max) + extra;
    let tail = if more > 0 {
        format!(" and {more} more")
    } else {
        String::new()
    };
    Some(format!("{heading}: {}{tail}", shown.join("; ")))
}

/// Characters in `s`.
fn n_chars(s: &str) -> usize {
    s.chars().count()
}

/// `s` cut to at most `n` characters, ending in `…` when cut. A last resort for the
/// release build: every slot renderer fits its allotment by construction (debug builds
/// assert it, see `fit_slot`).
fn hard_cut(s: &str, n: usize) -> String {
    if n_chars(s) <= n {
        return s.to_owned();
    }
    if n == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(n - 1).collect();
    out.push('…');
    out
}

/// A slot's text within its allotment (see `summarize_with`).
fn fit_slot(text: String, allot: usize) -> String {
    debug_assert!(
        n_chars(&text) <= allot,
        "a summary slot exceeds its allotment ({} > {allot}): {text}",
        n_chars(&text)
    );
    hard_cut(&text, allot)
}

/// An amount entry shown as its amount alone: the largest amount in it (`$5,000.00`),
/// cleaned and bounded.
fn amount_alone(f: &AmountFact) -> String {
    let chars: Vec<char> = f.text.chars().collect();
    let best = amount_ranges(&chars)
        .into_iter()
        .map(|(a, b)| {
            chars
                .get(a..b)
                .unwrap_or_default()
                .iter()
                .collect::<String>()
        })
        .max_by(|x, y| {
            amount_value(x)
                .unwrap_or(-1.0)
                .total_cmp(&amount_value(y).unwrap_or(-1.0))
        });
    fact_entry(best.as_deref().unwrap_or(&f.text), MAX_AMOUNT_CHARS)
}

/// Slot 2 (ninth review): `. Amounts on screen: Total $5,000.00; …; and N more`, within
/// `budget` characters. The entries are the first [`MAX_LISTED_AMOUNTS`] in rank order
/// (exact total phrases first, then other total wording, then other charges, then the rest;
/// see `amount_facts`), with three always among them: the first, the total-worded amount
/// nearest the target (tenth review) and the largest amount on the screen. When the screen
/// shows more than one distinct total-worded amount the heading says how many
/// (`. Amounts on screen (3 totals): …`, tenth review), so the owner is told the page names
/// several totals rather than one being picked silently. Shrinking: each entry to 40, 30,
/// 24 characters, then to its amount alone, then fewer entries — never one of the three.
/// What is left out is counted (`and N more`). Its minimum (three amounts alone, the
/// heading and the count) fits [`SLOT_AMOUNTS`]'s minimum for amounts of up to 13
/// characters.
fn amounts_slot(amounts: &[AmountFact], budget: usize) -> String {
    if amounts.is_empty() {
        return String::new();
    }
    let largest = amounts
        .iter()
        .enumerate()
        .fold(None::<(usize, f64)>, |best, (k, f)| match best {
            Some((_, v)) if v >= f.value => best,
            _ => Some((k, f.value)),
        })
        .map_or(0, |(k, _)| k);
    let nearest_total = amounts
        .iter()
        .enumerate()
        .filter(|(_, f)| f.total)
        .min_by(|(_, a), (_, b)| a.near.cmp(&b.near))
        .map(|(k, _)| k);
    // Distinct total-worded amounts (by value).
    let mut totals: Vec<f64> = amounts
        .iter()
        .filter(|f| f.total)
        .map(|f| f.value)
        .collect();
    totals.sort_by(f64::total_cmp);
    totals.dedup_by(|a, b| a.total_cmp(b).is_eq());
    if totals.len() > MAX_LABELLED_TOTALS {
        return compact_amounts(amounts, nearest_total, largest, totals.len(), budget);
    }
    let mut keep: Vec<usize> = vec![0];
    // Eleventh review: every distinct exact-phrase total is shown (at most three, since
    // there are at most three distinct totals here), before any other entry.
    let mut exact_values: Vec<f64> = Vec::new();
    for (k, f) in amounts.iter().enumerate().filter(|(_, f)| f.exact) {
        if !exact_values.iter().any(|v| v.total_cmp(&f.value).is_eq()) {
            exact_values.push(f.value);
            keep.push(k);
        }
    }
    for k in nearest_total.into_iter().chain(std::iter::once(largest)) {
        let same_value = amounts.get(k).is_some_and(|f| {
            keep.iter()
                .filter_map(|&i| amounts.get(i))
                .any(|g| g.value.total_cmp(&f.value).is_eq())
        });
        if !keep.contains(&k) && !same_value {
            keep.push(k);
        }
    }
    keep.sort_unstable();
    keep.dedup();
    let mut picked: Vec<usize> = (0..amounts.len().min(MAX_LISTED_AMOUNTS)).collect();
    for &k in &keep {
        if !picked.contains(&k) {
            if let Some(pos) = picked.iter().rposition(|i| !keep.contains(i)) {
                if let Some(slot) = picked.get_mut(pos) {
                    *slot = k;
                }
            }
        }
    }
    picked.sort_unstable();
    let plain = ". Amounts on screen: ";
    let heading = if totals.len() > 1 {
        format!(". Amounts on screen ({} totals): ", totals.len())
    } else {
        plain.to_owned()
    };
    let render_with = |idx: &[usize], entry: usize, heading: &str| -> String {
        let shown: Vec<String> = idx
            .iter()
            .filter_map(|&k| amounts.get(k))
            .map(|f| {
                if entry == 0 {
                    amount_alone(f)
                } else {
                    fact_entry(&f.text, entry)
                }
            })
            .collect();
        let more = amounts.len().saturating_sub(idx.len());
        let tail = if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        };
        format!("{heading}{}{tail}", shown.join("; "))
    };
    let render = |idx: &[usize], entry: usize| render_with(idx, entry, &heading);
    for entry in [40, 30, 24, 0] {
        let t = render(&picked, entry);
        if n_chars(&t) <= budget {
            return t;
        }
    }
    // Fewer entries, amounts alone: never the first, the nearest total nor the largest.
    for k in (keep.len()..picked.len()).rev() {
        let mut idx: Vec<usize> = picked
            .iter()
            .copied()
            .filter(|i| !keep.contains(i))
            .take(k.saturating_sub(keep.len()))
            .collect();
        idx.extend(&keep);
        idx.sort_unstable();
        let t = render(&idx, 0);
        if n_chars(&t) <= budget {
            return t;
        }
    }
    let t = render(&keep, 0);
    if n_chars(&t) <= budget {
        return t;
    }
    // Very long amounts: the plain heading (the count of totals goes first), then the
    // kept entries from the end (the first, an exact total, stays).
    let mut idx = keep.clone();
    loop {
        let t = render_with(&idx, 0, plain);
        if n_chars(&t) <= budget || idx.len() <= 1 {
            return t;
        }
        idx.pop();
    }
}

/// At most this many distinct total-worded amounts are shown with their labels; more
/// switch the amounts to [`compact_amounts`] (eleventh review).
const MAX_LABELLED_TOTALS: usize = 3;

/// The amounts slot when the screen shows more than [`MAX_LABELLED_TOTALS`] distinct
/// total-worded amounts (eleventh review: a page can always place more exact-phrase decoys
/// nearer the target than the real total): `. Amounts on screen (7 totals): $9,999.00;
/// $512.40; $5.04 (nearest); … and 4 more` — the distinct total amounts only, without
/// labels, largest first, as many as fit, the one nearest the target always among them
/// and marked; `and K more` counts every amount on the screen not shown. The largest
/// amount on the screen stays listed whatever its wording (fifth review): when it is not a
/// total it comes first, with its label (`Membership $899.00; $1.06 (nearest); …`).
fn compact_amounts(
    amounts: &[AmountFact],
    nearest: Option<usize>,
    largest: usize,
    n_totals: usize,
    budget: usize,
) -> String {
    let lead: Option<String> = amounts
        .get(largest)
        .filter(|f| !f.total)
        .map(|f| fact_entry(&f.text, MAX_AMOUNT_CHARS));
    // One fact per distinct total value, largest first (ties: rank order).
    let mut distinct: Vec<usize> = Vec::new();
    let mut order: Vec<usize> = (0..amounts.len())
        .filter(|&k| amounts.get(k).is_some_and(|f| f.total))
        .collect();
    order.sort_by(|&a, &b| {
        let (fa, fb) = (amounts.get(a), amounts.get(b));
        let va = fa.map_or(-1.0, |f| f.value);
        let vb = fb.map_or(-1.0, |f| f.value);
        vb.total_cmp(&va).then(a.cmp(&b))
    });
    // Sorted by value: a duplicate value follows its first.
    let mut last: Option<f64> = None;
    for k in order {
        let v = amounts.get(k).map_or(-1.0, |f| f.value);
        if last.is_some_and(|l| l.total_cmp(&v).is_eq()) {
            continue;
        }
        last = Some(v);
        distinct.push(k);
    }
    // The nearest total stands for its value.
    let nearest_value = nearest.and_then(|k| amounts.get(k)).map(|f| f.value);
    let near_pos = nearest_value.and_then(|v| {
        distinct.iter().position(|&i| {
            amounts
                .get(i)
                .is_some_and(|f| f.value.total_cmp(&v).is_eq())
        })
    });
    let heading = format!(". {MARK_AMOUNTS} ({n_totals} totals): ");
    let render = |k: usize| -> String {
        // The first k, with the nearest one in place of the k-th when it is later.
        let mut idx: Vec<usize> = (0..k.min(distinct.len())).collect();
        if let Some(p) = near_pos {
            if p >= k && k > 0 {
                if let Some(last) = idx.last_mut() {
                    *last = p;
                }
            }
        }
        let shown: Vec<String> = lead
            .iter()
            .cloned()
            .chain(idx.iter().filter_map(|&p| {
                let f = amounts.get(*distinct.get(p)?)?;
                let a = amount_alone(f);
                Some(if Some(p) == near_pos {
                    format!("{a} {MARK_NEAREST}")
                } else {
                    a
                })
            }))
            .collect();
        let more = amounts.len().saturating_sub(shown.len());
        let tail = if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        };
        format!("{heading}{}{tail}", shown.join("; "))
    };
    // Each entry takes at least 4 characters ("$1; "): no more can fit than that allows.
    let most = distinct.len().min(budget / 4 + 1);
    for k in (1..=most).rev() {
        let t = render(k);
        if n_chars(&t) <= budget {
            return t;
        }
    }
    hard_cut(&render(1), budget)
}

/// Characters kept from the end of a recipient entry that must be cut (tenth review): an
/// account's or card's distinguishing suffix ("… ending 4471").
const RECIPIENT_TAIL_CHARS: usize = 12;

/// `s` within `n` characters keeping its start and its last [`RECIPIENT_TAIL_CHARS`]
/// characters (`Acme Payments Ltd … ending 4471`; tenth review: a payee is no longer cut
/// from its end only, so the suffix that tells two accounts apart survives).
fn ends_cut(s: &str, n: usize) -> String {
    let len = n_chars(s);
    if len <= n {
        return s.to_owned();
    }
    let tail = RECIPIENT_TAIL_CHARS.min(n / 2);
    let head = n.saturating_sub(tail + 3);
    let h: String = s.chars().take(head).collect();
    let t: String = s.chars().skip(len - tail).collect();
    format!("{} … {}", h.trim_end(), t.trim_start())
}

/// One recipient entry (eleventh review): what comes before its value (`To: "` for a
/// field, `Pay to: ` for a heading pair the screen shows), the value, what closes it (`"`
/// for a field), whether it mixes look-alike letters, and for a field its position among
/// the sent fields. All already cleaned.
struct Recipient {
    head: String,
    value: String,
    close: String,
    lookalike: bool,
    field: Option<usize>,
}

/// The longest email host shown whole; a longer one shows its first
/// [`HOST_HEAD_CHARS`] and last [`HOST_TAIL_CHARS`] (eleventh review).
const MAX_HOST_CHARS: usize = 40;
const HOST_HEAD_CHARS: usize = 12;
const HOST_TAIL_CHARS: usize = 24;
/// Characters of an email's local part kept at least when it must be cut.
const MIN_LOCAL_CHARS: usize = 6;
/// Longest field name in a recipient entry at the slot's minimum.
const MIN_RECIPIENT_NAME_CHARS: usize = 12;

/// `value`'s local part and host when it reads as an email: exactly one `@`, and a host
/// with a dot that has characters on both sides.
fn email_parts(value: &str) -> Option<(&str, &str)> {
    if value.matches('@').count() != 1 {
        return None;
    }
    let (local, host) = value.split_once('@')?;
    let dotted = host
        .char_indices()
        .any(|(i, c)| c == '.' && i > 0 && i + 1 < host.len());
    (dotted && !host.chars().any(char::is_whitespace)).then_some((local, host))
}

/// `value` within about `cap` characters for a recipient entry, and how many of its
/// characters were left out (eleventh review). An email keeps its `@` and its host (up to
/// [`MAX_HOST_CHARS`] whole, else its first 12 and last 24 around `…`) and gives up
/// characters of its local part first, down to [`MIN_LOCAL_CHARS`]: `alice.…@pay-bigbank.com`
/// (it may exceed `cap` to keep that). Any other value keeps its start and its last
/// [`RECIPIENT_TAIL_CHARS`] (`Acme Payments Ltd … ending 4471`).
fn recipient_cut(value: &str, cap: usize) -> (String, usize) {
    let len = n_chars(value);
    if len <= cap {
        return (value.to_owned(), 0);
    }
    if let Some((local, host)) = email_parts(value) {
        let host_len = n_chars(host);
        let host_shown = if host_len <= MAX_HOST_CHARS {
            host.to_owned()
        } else {
            let h: String = host.chars().take(HOST_HEAD_CHARS).collect();
            let t: String = host.chars().skip(host_len - HOST_TAIL_CHARS).collect();
            format!("{h}…{t}")
        };
        let local_len = n_chars(local);
        let room = cap.saturating_sub(n_chars(&host_shown) + 2);
        let keep = room.max(MIN_LOCAL_CHARS).min(local_len);
        let local_shown: String = local.chars().take(keep).collect();
        let cut_local = keep < local_len;
        let shown = if cut_local {
            format!("{local_shown}…@{host_shown}")
        } else {
            format!("{local_shown}@{host_shown}")
        };
        let host_kept = if host_len <= MAX_HOST_CHARS {
            host_len
        } else {
            HOST_HEAD_CHARS + HOST_TAIL_CHARS
        };
        return (shown, local_len - keep + host_len - host_kept);
    }
    let shown = ends_cut(value, cap);
    let tail = RECIPIENT_TAIL_CHARS.min(cap / 2);
    let head = cap.saturating_sub(tail + 3).min(len);
    (shown, len.saturating_sub(head + tail))
}

/// The shortest an entry's value is shown: an email at its minimum cut, else 24
/// characters.
fn min_value_cap(value: &str) -> usize {
    match email_parts(value) {
        Some((_, host)) => {
            MIN_LOCAL_CHARS + 2 + n_chars(host).min(HOST_HEAD_CHARS + 1 + HOST_TAIL_CHARS)
        }
        None => 24,
    }
}

/// One recipient entry at value cap `cap` (and, at the slot's minimum, its field name cut
/// to [`MIN_RECIPIENT_NAME_CHARS`]): `To: "alice.…@pay-bigbank.com" (40 not shown) (look-alike
/// letters)`.
fn recipient_text(r: &Recipient, cap: usize, short_name: bool) -> String {
    let cap = if cap == 0 {
        min_value_cap(&r.value)
    } else {
        cap
    };
    let (value, hidden) = recipient_cut(&r.value, cap);
    let head = if short_name && r.field.is_some() {
        short_field_head(&r.head)
    } else {
        r.head.clone()
    };
    let mut out = format!("{head}{value}{}", r.close);
    if hidden > 0 {
        out.push_str(&format!(" ({hidden} not shown)"));
    }
    if r.lookalike {
        out.push(' ');
        out.push_str(MARK_LOOKALIKE);
    }
    out
}

/// A field entry's head (`Name: "`) with the name cut to [`MIN_RECIPIENT_NAME_CHARS`]
/// (a quoted name stays quoted).
fn short_field_head(head: &str) -> String {
    let name = head.strip_suffix(": \"").unwrap_or(head);
    if n_chars(name) <= MIN_RECIPIENT_NAME_CHARS {
        return head.to_owned();
    }
    let short = match name.strip_prefix('\'').and_then(|n| n.strip_suffix('\'')) {
        Some(inner) => format!("'{}'", hard_cut(inner, MIN_RECIPIENT_NAME_CHARS - 2)),
        None => match name.strip_prefix('"').and_then(|n| n.strip_suffix('"')) {
            Some(inner) => format!("\"{}\"", hard_cut(inner, MIN_RECIPIENT_NAME_CHARS - 2)),
            None => hard_cut(name, MIN_RECIPIENT_NAME_CHARS),
        },
    };
    format!("{short}: \"")
}

/// Slot 3 (ninth review; eleventh review: evidence with its provenance). Two markers in
/// one slot, within `budget` characters:
///
/// - `. To/Account: To: "@mallory"` — only the target's form fields named like a recipient,
///   with their values (what the action sends: the one recipient the gate can assert);
/// - `. Screen says: Pay to: Mallory's Merch LLC` — recipient heading pairs read from the
///   screen's text, labelled as what the screen says;
/// - `. Recipient not verified` in place of `To/Account:` when `unverified` (a sending,
///   posting, paying or purchasing action whose form names no recipient).
///
/// Each list shows at most [`MAX_LISTED_RECIPIENTS`] entries, each value cut to 60, then
/// 40 characters ([`recipient_cut`]: an email keeps `@` and its host), then fewer entries,
/// then the first at its minimum (24, or an email's minimal cut, field name cut to 12),
/// then `Screen says:` as a count; the rest counted (`and N more`); a cut value says how
/// many characters are not shown. When a list names more than one distinct recipient
/// (`distinct`) its heading says so (`. To/Account (2 different): …`). Returns the text and,
/// per field entry, whether it was shown (its value is then not repeated under `Sends:`).
fn recipients_slot(
    fields: &[Recipient],
    says: &[Recipient],
    distinct: (usize, usize),
    unverified: bool,
    budget: usize,
) -> (Vec<QuestionRow>, Vec<usize>, usize) {
    if fields.is_empty() && says.is_empty() && !unverified {
        return (Vec::new(), Vec::new(), 0);
    }
    let head = |mark: &str, n: usize| {
        if n > 1 {
            format!(". {mark} ({n} different): ")
        } else {
            format!(". {mark}: ")
        }
    };
    let list = |items: &[Recipient], mark: &str, n: usize, k: usize, cap: usize, short: bool| {
        if items.is_empty() {
            return String::new();
        }
        if k == 0 {
            // Twelfth review: what the uncounted lines are.
            return format!(". {mark}: {} {MARK_MORE_SAYS}", items.len());
        }
        let shown: Vec<String> = items
            .iter()
            .take(k)
            .map(|r| recipient_text(r, cap, short))
            .collect();
        let more = items.len().saturating_sub(k);
        let tail = if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        };
        format!("{}{}{tail}", head(mark, n), shown.join("; "))
    };
    // (fields shown, field cap, short names, says shown, says cap): the form's recipient
    // fields, `Recipient not verified`, then what the screen says.
    let parts = |l: (usize, usize, bool, usize, usize)| -> (String, String, String) {
        // Thirteenth review: `unverified` with entries listed (weak heads only) follows them.
        let not_verified = if unverified {
            format!(". {MARK_NOT_VERIFIED}")
        } else {
            String::new()
        };
        let listed = if fields.is_empty() {
            String::new()
        } else {
            list(fields, MARK_RECIPIENTS, distinct.0, l.0, l.1, l.2)
        };
        (
            listed,
            not_verified,
            list(says, MARK_SCREEN_SAYS, distinct.1, l.3, l.4, false),
        )
    };
    let render = |l: (usize, usize, bool, usize, usize)| -> String {
        let (listed, not_verified, says) = parts(l);
        format!("{listed}{not_verified}{says}")
    };
    let nf = fields.len().min(MAX_LISTED_RECIPIENTS);
    let ns = says.len().min(MAX_LISTED_RECIPIENTS);
    let mut levels: Vec<(usize, usize, bool, usize, usize)> =
        vec![(nf, 60, false, ns, 60), (nf, 40, false, ns, 40)];
    for k in (1..ns).rev() {
        levels.push((nf, 40, false, k, 40));
    }
    for k in (1..nf).rev() {
        levels.push((k, 40, false, ns.min(1), 40));
    }
    levels.extend([
        (nf.min(1), 40, false, ns.min(1), 24),
        (nf.min(1), 24, true, ns.min(1), 24),
        (nf.min(1), 0, true, ns.min(1), 24),
        (nf.min(1), 0, true, 0, 0),
    ]);
    let mut chosen = None;
    for l in &levels {
        if n_chars(&render(*l)) <= budget {
            chosen = Some(*l);
            break;
        }
    }
    let last = (nf.min(1), 0, true, 0, 0);
    let minimal = n_chars(&render(last));
    let l = chosen.unwrap_or(last);
    let shown: Vec<usize> = fields.iter().take(l.0).filter_map(|r| r.field).collect();
    // The slot's minimum is the last level's length (`minimal`, see `summarize_with`), so
    // it fits but where the minimums had to be scaled down (behind the runner-text
    // warning, every slot at its minimum): there it is cut, marked `…`.
    let (listed, not_verified, says_part) = parts(l);
    let rows = vec![
        row(QuestionRowKind::Recipient, &listed),
        row(QuestionRowKind::NotVerified, &not_verified),
        row(QuestionRowKind::ScreenSays, &says_part),
    ];
    let joined = render_rows(&rows);
    let cut = hard_cut(&joined, budget);
    let rows = if cut == joined {
        non_empty(rows)
    } else {
        split_cut(&rows, &cut)
    };
    (rows, shown, minimal)
}

/// Slot 5 (ninth review): the other facts — the row text after "next to", the item text
/// after "under" (both drawn inside the action line), `Selected:` and `Fields:` — within
/// `budget` characters: each part to 40, then 24, then 16 characters, then the lists as
/// counts (`Selected: 3 options`, `Fields: 2 not shown`). Returns (inline, rest): the rest
/// as its rows (the cap note, `Selected:`, `Fields:`).
fn other_slot(
    next_to: &str,
    under: &str,
    facts: &Facts,
    budget: usize,
) -> (String, Vec<QuestionRow>) {
    const LEVELS: [(usize, usize, usize, usize); 5] = [
        (MAX_NEIGHBOUR_CHARS, MAX_NEIGHBOUR_CHARS, 40, 40),
        (24, 30, 24, 24),
        (16, 24, 16, 0),
        (12, 12, 0, 0),
        (0, 0, 0, 0),
    ];
    let render = |(n, u, sel, fld): (usize, usize, usize, usize)| -> (String, Vec<QuestionRow>) {
        let mut inline = String::new();
        if !next_to.is_empty() && n > 0 {
            inline.push_str(&format!(" next to \"{}\"", clean_label(next_to, n)));
        }
        if !under.is_empty() && u > 0 {
            inline.push_str(&format!(" under \"{}\"", clean_label(under, u)));
        }
        // The cap note first: it is the last to be cut.
        let mut rest = vec![row(QuestionRowKind::Note, &facts.note)];
        if !facts.selected.is_empty() {
            let part = if sel == 0 {
                Some(format!("Selected: {} options", facts.selected.len()))
            } else {
                fact_list(MARK_SELECTED, &facts.selected, 0, MAX_LISTED_SELECTED, sel)
            };
            if let Some(p) = part {
                rest.push(row(QuestionRowKind::Selected, &format!(". {p}")));
            }
        }
        if let Some(p) = fact_list(
            MARK_FIELDS,
            &facts.fields,
            facts.fields_more,
            MAX_LISTED_FIELDS,
            fld,
        ) {
            rest.push(row(QuestionRowKind::Fields, &format!(". {p}")));
        }
        (inline, non_empty(rest))
    };
    let mut last = (String::new(), Vec::new());
    for l in LEVELS {
        let (i, r) = render(l);
        if n_chars(&i) + n_chars(&render_rows(&r)) <= budget {
            return (i, r);
        }
        last = (i, r);
    }
    // Only the counts are left (the inline texts are gone at the last level): cut them.
    let (i, r) = last;
    let joined = render_rows(&r);
    let cut = hard_cut(&joined, budget);
    if cut == joined {
        (i, r)
    } else {
        (i, split_cut(&r, &cut))
    }
}

/// The row text after "next to" and the item text after "under" of a press on `el`.
fn press_context(
    el: &Element,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
) -> (String, String) {
    if switch_is_on(el).is_some() {
        return (String::new(), String::new());
    }
    (
        row_texts(el, screen).join(" "),
        item_texts(el, screen, tree).join(", "),
    )
}

/// The summary of pressing `el` (`verb` "Tap" or "Long-press"), as (head, tail): a tap on
/// an on/off switch says what it will do (`Turn on "Wi-Fi"`, ` in Settings`); otherwise
/// `Tap "Buy"` and ` in Shop`. The row and item texts (`next to "Socks $5.00"`, `under
/// "Order #1001, Placed Nov 20"`) go between them (slot 5); the screen's facts follow (see
/// `summarize_with`), never one amount picked by distance.
fn press_summary(verb: &str, el: &Element, in_app: &str, label: usize) -> (String, String) {
    let what = describe_with(el, label);
    if verb == "Tap" {
        if let Some(on) = switch_is_on(el) {
            let turn = if on { "Turn off" } else { "Turn on" };
            return (format!("{turn} {what}"), in_app.to_owned());
        }
    }
    (format!("{verb} {what}"), in_app.to_owned())
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
/// pattern redaction (cards, labelled codes); a credential-like field is `(hidden)`. Fields
/// not being sent keep the rule above.
///
/// **Eighth review.** A long press and a `tap_point` send like a tap; any asked tap whose
/// form holds text shows `Sends:`; the form includes its unlisted text inputs (`(not on
/// screen)`, from [`ScreenTree::inputs`]); Return that submits a form with other fields
/// says `which submits the form. Sends: …`.
///
/// **Ninth review: reserved slots.** The summary is composed in one pass from five slots
/// in priority order, each with a guaranteed minimum and a maximum (`summarize_with`): (1)
/// the action line; (2) the amounts (the screen's total first, the largest always); (3) the
/// recipients (read from the whole screen, a merchant after "Pay to:" included, then the
/// sent fields *named* like a recipient, with their values); (4) what is sent (`Sends:` for
/// a sending, paying, posting, purchasing, presumed-submit or unverified action, `The form
/// holds:` for any other; the fields Jarvis typed first, then the recipient- and
/// amount-named ones, then the rest; names sanitised); (5) the other facts. A slot never
/// eats into the minimums of the slots after it, says what it leaves out, and the whole is
/// within the bound by construction.
pub fn summary_with(
    action: &Action,
    screen: &CompactScreen,
    category: RiskCategory,
    source: ScreenSource,
    tree: Option<&ScreenTree<'_>>,
    typed: &[TypedText],
) -> String {
    render_rows(&question_with(
        action, screen, category, source, tree, typed,
    ))
}

/// The question [`summary_with`] renders, as its parts ([`QuestionRow`]): the summary is
/// exactly these rows joined ([`render_rows`]). The rows are what the composer writes, slot
/// by slot (the runner-text warning, the action line, what is sent, `Typed:`, the amounts,
/// `To/Account:` or `Recipient not verified`, `Screen says:`, the cap note, `Selected:`,
/// `Fields:`), so a display can lay them out without reading the text.
pub fn question_with(
    action: &Action,
    screen: &CompactScreen,
    category: RiskCategory,
    source: ScreenSource,
    tree: Option<&ScreenTree<'_>>,
    typed: &[TypedText],
) -> Vec<QuestionRow> {
    question_noted(action, screen, category, source, tree, typed, &[])
}

/// [`question_with`] with `notes` (fifteenth review: [`not_owner_note`],
/// [`sensitive_note`]) after the action line, room kept for them within the bound. A tap
/// inside an alert (R4) states it after the action line too, with the alert's title under
/// `Screen says:`.
pub fn question_noted(
    action: &Action,
    screen: &CompactScreen,
    category: RiskCategory,
    source: ScreenSource,
    tree: Option<&ScreenTree<'_>>,
    typed: &[TypedText],
    notes: &[QuestionRow],
) -> Vec<QuestionRow> {
    let mut notes = notes.to_vec();
    notes.extend(alert_rows(action, screen, tree));
    amount_noted(screen, category, notes, |notes| {
        with_source(source, |max| {
            noted(max, notes, |room| {
                summarize_with(
                    action, screen, category, source, tree, typed, false, false, room,
                )
            })
        })
    })
}

/// Sixteenth review (F8): the question `build(notes)` composes, with [`MARK_AMOUNT_UNREAD`]
/// after the action line when it is asked in a money context — a payment, or a send on a
/// screen that shows money ([`screen_shows_money`]), or a payment on a screen with digits in
/// a label — and found no amount to show: the owner
/// is told the gate could not read the amount rather than shown a question that looks
/// complete without one.
fn amount_noted(
    screen: &CompactScreen,
    category: RiskCategory,
    mut notes: Vec<QuestionRow>,
    build: impl Fn(&[QuestionRow]) -> Vec<QuestionRow>,
) -> Vec<QuestionRow> {
    let mut rows = build(&notes);
    // An amount the question shows anywhere (its rows, or the action line: "Pay $5") is read.
    if rows
        .iter()
        .any(|r| r.kind == QuestionRowKind::Amount || has_money(&r.text))
    {
        return rows;
    }
    let money_context = match category {
        // The cheap digit test first: the whole-screen money scan only when no label has one.
        RiskCategory::Payment => {
            screen
                .elements
                .iter()
                .any(|e| !is_secure_input(e) && e.label.chars().any(crate::text::is_digit))
                || screen_money(screen, false).is_some()
        }
        RiskCategory::SendMessage | RiskCategory::PublicPost => {
            screen_money(screen, false).is_some()
        }
        _ => false,
    };
    if !money_context {
        return rows;
    }
    let note = row(QuestionRowKind::Warning, &format!(". {MARK_AMOUNT_UNREAD}"));
    // Within the bound the note goes in without composing the question again (a large page
    // costs one composition); a question at the bound is composed again with room for it.
    if render_rows(&rows).chars().count() + n_chars(&note.text) <= MAX_SUMMARY_CHARS {
        insert_notes(&mut rows, std::slice::from_ref(&note));
        return rows;
    }
    notes.push(note);
    build(&notes)
}

/// The warning of a question in a money context that found no amount to show (F8).
pub const MARK_AMOUNT_UNREAD: &str = "Amount not read";

/// The rows `build(room)` composes within `max` less the notes, with the notes after the
/// action line.
fn noted(
    max: usize,
    notes: &[QuestionRow],
    build: impl Fn(usize) -> Vec<QuestionRow>,
) -> Vec<QuestionRow> {
    if notes.is_empty() {
        return build(max);
    }
    let need: usize = notes.iter().map(|r| n_chars(&r.text)).sum();
    let mut rows = build(max.saturating_sub(need).max(120));
    insert_notes(&mut rows, notes);
    rows
}

/// `notes` into `rows` after the action line and what continues its sentence (`, which
/// sends …`).
fn insert_notes(rows: &mut Vec<QuestionRow>, notes: &[QuestionRow]) {
    let mut at = rows
        .iter()
        .position(|r| r.kind == QuestionRowKind::Action)
        .map_or(0, |k| k + 1);
    while rows.get(at).is_some_and(|r| r.text.starts_with(", ")) {
        at += 1;
    }
    for (k, n) in notes.iter().enumerate() {
        rows.insert(at + k, n.clone());
    }
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
    render_rows(&return_question(
        field, text, screen, category, source, tree, typed,
    ))
}

/// The question [`return_summary`] renders, as its parts (see [`question_with`]).
pub fn return_question(
    field: u32,
    text: &str,
    screen: &CompactScreen,
    category: RiskCategory,
    source: ScreenSource,
    tree: Option<&ScreenTree<'_>>,
    typed: &[TypedText],
) -> Vec<QuestionRow> {
    return_question_noted(field, text, screen, category, source, tree, typed, &[])
}

/// [`return_question`] with `notes` after the action line (see [`question_noted`]).
#[allow(clippy::too_many_arguments)]
pub fn return_question_noted(
    field: u32,
    text: &str,
    screen: &CompactScreen,
    category: RiskCategory,
    source: ScreenSource,
    tree: Option<&ScreenTree<'_>>,
    typed: &[TypedText],
    notes: &[QuestionRow],
) -> Vec<QuestionRow> {
    let action = Action::Type {
        element: Some(field),
        text: text.to_owned(),
        submit: true,
    };
    amount_noted(screen, category, notes.to_vec(), |notes| {
        with_source(source, |max| {
            noted(max, notes, |room| {
                summarize_with(
                    &action, screen, category, source, tree, typed, true, false, room,
                )
            })
        })
    })
}

/// A question of at most [`MAX_SUMMARY_CHARS`] from `build(max)`, behind the warning (its
/// own row) on a runner-text screen.
fn with_source(
    source: ScreenSource,
    build: impl Fn(usize) -> Vec<QuestionRow>,
) -> Vec<QuestionRow> {
    match source {
        ScreenSource::Typed => build(MAX_SUMMARY_CHARS),
        ScreenSource::RunnerText => {
            let room = MAX_SUMMARY_CHARS.saturating_sub(UNVERIFIED_SCREEN_WARNING.chars().count());
            let mut rows = vec![row(QuestionRowKind::Warning, UNVERIFIED_SCREEN_WARNING)];
            rows.extend(build(room.max(120)));
            rows
        }
    }
}

/// A question row.
fn row(kind: QuestionRowKind, text: &str) -> QuestionRow {
    QuestionRow {
        kind,
        text: text.to_owned(),
    }
}

/// `rows` without the empty ones.
fn non_empty(rows: impl IntoIterator<Item = QuestionRow>) -> Vec<QuestionRow> {
    rows.into_iter().filter(|r| !r.text.is_empty()).collect()
}

/// `cut` (the texts of `rows` joined, or a cut of that: a prefix of it ending in `…`) as the
/// same rows: each row takes as many characters as its own text has, the last the rest.
/// Rows left empty are dropped, so the rows still render to exactly `cut`.
fn split_cut(rows: &[QuestionRow], cut: &str) -> Vec<QuestionRow> {
    let mut chars = cut.chars();
    let mut out = Vec::with_capacity(rows.len());
    for (i, r) in rows.iter().enumerate() {
        let n = if i + 1 == rows.len() {
            usize::MAX
        } else {
            n_chars(&r.text)
        };
        let piece: String = chars.by_ref().take(n).collect();
        out.push(QuestionRow {
            kind: r.kind,
            text: piece,
        });
    }
    non_empty(out)
}

/// A slot's rows within its allotment, cut like [`fit_slot`] cuts its text.
fn fit_rows(rows: Vec<QuestionRow>, allot: usize) -> Vec<QuestionRow> {
    let joined = render_rows(&rows);
    let cut = fit_slot(joined.clone(), allot);
    if cut == joined {
        non_empty(rows)
    } else {
        split_cut(&rows, &cut)
    }
}

/// A field whose whole content an action sends (see [`summary_with`]): its name as shown
/// ([`display_name`]), where it is (listed, or not on screen), its cleaned, redacted value
/// (`None`: hidden; empty: nothing in it), and whether it is named like a recipient or an
/// amount, or was typed by Jarvis (its place in `Sends:` and `To/Account:`, ninth review).
struct SentField {
    name: String,
    note: &'static str,
    value: Option<String>,
    /// Characters of the value beyond what is kept of it at all (the first
    /// [`crate::sanitize::MAX_SCREEN_TEXT_CHARS`]): counted as not shown.
    beyond: usize,
    recipient: bool,
    /// Thirteenth review: named under a strong head ([`strong_recipient_field_name`]).
    strong: bool,
    amount: bool,
    typed: bool,
}

/// Longest value of a sent field shown before anything else is shortened (eighth review):
/// longer values are cut to it first, saying how many characters are not shown.
const MAX_SENT_VALUE_CHARS: usize = 120;

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

/// Return-key labels that submit (eighth review: a multi-line text view's Return submits
/// only with one of these; a single-line field's Return always may).
const SUBMIT_RETURN_KEYS: &[&str] = &[
    "send", "go", "done", "join", "search", "continue", "route", "submit", "post", "reply",
];

/// Whether a tap on `el` in `category` sends the fields of its form by its own words.
fn tap_sends(el: &Element, category: RiskCategory) -> bool {
    SENDING_CATEGORIES.contains(&category)
        || crate::text::tokens(&el.label)
            .iter()
            .any(|t| SUBMIT_WORDS.contains(&t.as_str()))
}

/// The form of `target`: the listed text inputs (other than the target) and the unlisted
/// raw text inputs ([`ScreenTree::inputs`]) under the nearest raw ancestor of the target
/// that holds any text input; without the raw tree, or when no ancestor below the root
/// holds one, every listed text input on the screen (and no unlisted one). Secure fields
/// are left out (they are never sent through Jarvis's question; a secure field is typed by
/// the owner).
struct Form<'a, 't> {
    listed: Vec<&'a Element>,
    unlisted: Vec<&'t RawInput>,
    /// The form holds a secure field (left out of `listed` and `unlisted`).
    has_secure: bool,
    /// Unlisted inputs beyond the [`MAX_UNLISTED_INPUTS`] examined (counted, not read).
    unlisted_more: usize,
    /// The raw node holding the form (its nearest ancestor holding a text input), when the
    /// raw tree found one (thirteenth review: amounts shown inside the form).
    root: Option<u32>,
}

fn form_of<'a, 't>(
    target: &Element,
    screen: &'a CompactScreen,
    tree: Option<&ScreenTree<'t>>,
) -> Form<'a, 't> {
    let inputs: Vec<&Element> = screen
        .elements
        .iter()
        .filter(|e| e.role.is_text_input() && e.index != target.index)
        .collect();
    if let Some(t) = tree {
        let own = t.source(target);
        let listed: std::collections::BTreeSet<u32> = t.sources.iter().copied().collect();
        let unlisted: Vec<&RawInput> = t
            .inputs
            .iter()
            .filter(|r| Some(r.node) != own && !listed.contains(&r.node))
            .collect();
        for a in t.ancestors(target) {
            let inside: Vec<&Element> = inputs.iter().copied().filter(|e| t.inside(e, a)).collect();
            let raw: Vec<&RawInput> = unlisted
                .iter()
                .copied()
                .filter(|r| t.node_inside(r.node, a))
                .collect();
            if !inside.is_empty() || !raw.is_empty() {
                let has_secure = inside.iter().any(|e| is_secure_input(e))
                    || raw
                        .iter()
                        .any(|r| r.secure || r.role == Role::SecureTextField);
                let mut open: Vec<&RawInput> = raw
                    .into_iter()
                    .filter(|r| !r.secure && r.role != Role::SecureTextField)
                    .collect();
                let unlisted_more = open.len().saturating_sub(MAX_UNLISTED_INPUTS);
                open.truncate(MAX_UNLISTED_INPUTS);
                return Form {
                    listed: inside.into_iter().filter(|e| !is_secure_input(e)).collect(),
                    unlisted: open,
                    has_secure,
                    unlisted_more,
                    root: Some(a),
                };
            }
        }
    }
    let has_secure = inputs.iter().any(|e| is_secure_input(e));
    Form {
        listed: inputs.into_iter().filter(|e| !is_secure_input(e)).collect(),
        unlisted: Vec::new(),
        has_secure,
        unlisted_more: 0,
        root: None,
    }
}

/// Whether `e` is a secure field (flag or role).
fn is_secure_input(e: &Element) -> bool {
    e.secure || e.role == Role::SecureTextField
}

/// Most unlisted text inputs of a form that are examined (ninth review: a form with 50,000
/// hidden inputs costs bounded work; the summary says how many more it holds).
const MAX_UNLISTED_INPUTS: usize = 200;
/// Most listed text inputs read for a question (eleventh review, F9): the form's, nearest
/// the target first, then the other value fields; the rest are counted.
const MAX_LISTED_INPUTS: usize = 200;

impl Form<'_, '_> {
    /// The names of the form's filled (non-secure) fields, listed or not.
    fn filled_names(&self) -> impl Iterator<Item = String> + '_ {
        self.listed
            .iter()
            .filter(|e| field_value(e).is_some())
            .map(|e| {
                format!(
                    "{} {}",
                    field_name(e),
                    e.placeholder.as_deref().unwrap_or_default()
                )
            })
            .chain(
                self.unlisted
                    .iter()
                    .filter(|r| r.value.as_deref().is_some_and(|v| !v.trim().is_empty()))
                    .map(|r| {
                        [&r.label, &r.placeholder, &r.identifier]
                            .iter()
                            .filter_map(|x| x.as_deref())
                            .collect::<Vec<_>>()
                            .join(" ")
                    }),
            )
    }

    /// Whether any field of the form holds text.
    fn has_filled(&self) -> bool {
        self.listed.iter().any(|e| field_value(e).is_some())
            || self
                .unlisted
                .iter()
                .any(|r| r.value.as_deref().is_some_and(|v| !v.trim().is_empty()))
    }

    fn is_empty(&self) -> bool {
        self.listed.is_empty() && self.unlisted.is_empty()
    }
}

/// An unlisted raw text input as an element (for the field rules: names, credential-like
/// names, redaction), with its value cut to [`MAX_READ_VALUE_CHARS`] before the label-aware
/// redaction; also its value's whole length.
fn raw_input_element(r: &RawInput) -> (Element, usize) {
    let label = r
        .label
        .clone()
        .or_else(|| r.placeholder.clone())
        .or_else(|| r.identifier.clone())
        .unwrap_or_default();
    let len = r.value.as_deref().map_or(0, n_chars);
    let el = Element {
        index: 0,
        role: r.role.clone(),
        label,
        value: if r.secure {
            None
        } else {
            r.value.as_deref().map(read_cap)
        },
        placeholder: r.placeholder.clone(),
        frame: jarvis_proto::Rect::new(0.0, 0.0, 0.0, 0.0),
        enabled: true,
        focused: false,
        selected: false,
        secure: r.secure,
        fingerprint: r.fingerprint.unwrap_or(0),
    };
    (crate::redact::redact_element(&el), len)
}

/// Longest part of a value or label read at all (ninth review): a longer one is cut to it
/// *before* redaction and classification (then redacted, then cut to the display cap), so
/// a 2 MB value costs bounded work; what is cut is counted as not shown.
pub(crate) const MAX_READ_VALUE_CHARS: usize = 2_000;

/// `s` cut to [`MAX_READ_VALUE_CHARS`] characters.
fn read_cap(s: &str) -> String {
    if s.len() <= MAX_READ_VALUE_CHARS {
        return s.to_owned();
    }
    s.chars().take(MAX_READ_VALUE_CHARS).collect()
}

/// Whether a field is named like who is paid or messaged (ninth review: by its name only,
/// never by the shape of its value — an email in a "Contact email" field is no payee;
/// twelfth review: by the closed rule of [`recipient_field_name`]).
fn recipient_named(el: &Element) -> bool {
    recipient_field_name(&field_name(el))
}

/// Longest field name shown in `Sends:` (ninth review).
const MAX_FIELD_NAME_CHARS: usize = 40;

/// A field's name as `Sends:` and `To/Account:` show it (ninth review): cleaned like screen
/// text, `:` and `;` turned into spaces, quotes into `’`, at most
/// [`MAX_FIELD_NAME_CHARS`] characters, and put in single quotes when it holds anything
/// but letters, digits and spaces — so a name cannot imitate an entry or Jarvis's wording;
/// twelfth review: in double quotes when it reads like a heading ([`heading_like_name`]).
fn display_name(raw: &str) -> String {
    let c = clean(raw, MAX_FIELD_NAME_CHARS);
    let neutral: String = c
        .chars()
        .map(|ch| match ch {
            ':' | ';' => ' ',
            '"' | '\'' | '`' => '\u{2019}',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let name = neutral.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return "A field".to_owned();
    }
    // Twelfth review (F3): a name that reads like a heading, a marker or the composer's
    // own wording is always in double quotes ("Screen says": "…"), never followed by a
    // bare `:` of ours.
    if heading_like_name(&name) {
        return format!("\"{name}\"");
    }
    if name.chars().all(|c| c.is_alphanumeric() || c == ' ') {
        name
    } else {
        format!("'{name}'")
    }
}

/// `el` (values already cut to [`MAX_READ_VALUE_CHARS`]) as a field being sent; `len` is
/// its value's whole length, `note` says where it is: hidden when secure or named as a
/// credential.
fn sent_field(el: &Element, note: &'static str, len: usize, typed: bool) -> SentField {
    let hidden = el.secure || credential_like_field(el);
    let cap = crate::sanitize::MAX_SCREEN_TEXT_CHARS;
    let value = if hidden {
        None
    } else {
        Some(
            field_value(el)
                .map(|v| clean(v, cap).replace(';', ","))
                .unwrap_or_default(),
        )
    };
    let shown = value.as_deref().map_or(0, n_chars);
    let beyond = if hidden || shown == 0 {
        0
    } else {
        len.saturating_sub(cap.min(len))
    };
    SentField {
        name: display_name(&field_name(el)),
        note,
        value,
        beyond,
        recipient: !hidden && recipient_named(el),
        strong: !hidden && strong_recipient_field_name(&field_name(el)),
        amount: !hidden && amount_like_field(el),
        typed,
    }
}

/// How a summary says what is sent.
#[derive(Clone, Copy, PartialEq)]
enum SendMode {
    /// A tap (or long press, or tap point) that submits its form: `. Sends: …`.
    Tap,
    /// Return in a field alone: `, which sends the field's whole content: "…"`.
    ReturnAlone,
    /// Return that submits a form (eighth review): `, which submits the form. Sends: …`.
    ReturnForm,
}

/// What an action sends (see [`what_is_sent`]).
struct Sent<'a> {
    /// In the order `Sends:` lists them: Return's own field, the fields Jarvis typed, the
    /// recipient- and amount-named ones, the rest.
    fields: Vec<SentField>,
    /// How many fields the form holds, the unlisted ones beyond
    /// [`MAX_UNLISTED_INPUTS`] included.
    total: usize,
    /// The listed elements among them (not repeated under `Fields:`).
    listed: Vec<&'a Element>,
    mode: SendMode,
    /// `Sends:` (a sending action) or `The form holds:` (any other asked action).
    sends: bool,
}

/// Whether Return in `field` submits (eighth review): a single-line field's Return may
/// always submit; a multi-line text view's only with a submit-labelled key.
fn submit_return(field: &Element, screen: &CompactScreen) -> bool {
    if field.role != Role::TextView {
        return true;
    }
    screen.cues.return_key.as_deref().is_some_and(|k| {
        let k = k.trim().to_lowercase();
        SUBMIT_RETURN_KEYS.contains(&k.as_str())
    })
}

/// Whether the raw input `r` holds what Jarvis typed during the task: its fingerprint is a
/// typed field's (and it holds text), or its value is exactly a text Jarvis typed.
fn raw_typed(r: &RawInput, typed: &[TypedText]) -> bool {
    let Some(v) = r.value.as_deref().map(str::trim).filter(|v| !v.is_empty()) else {
        return false;
    };
    typed.iter().any(|t| {
        r.fingerprint.is_some_and(|f| f == t.field)
            || (!t.text.trim().is_empty() && t.text.trim() == v)
    })
}

/// What `action` sends, and how the summary says it (see [`summary_with`]): Return sends
/// its field, or with a submit Return the field and the rest of its form; a tap, long press
/// or tap point on a control that submits its form by its category or words, or any asked
/// tap whose form holds text (eighth review), sends the form's text inputs, the listed ones
/// and the ones not listed (`(not on screen)`). Order: Return's own field, the fields Jarvis
/// typed during the task (listed or not, ninth review), the recipient- and amount-named
/// ones, the rest.
#[allow(clippy::too_many_arguments)]
fn what_is_sent<'a>(
    action: &Action,
    target: Option<&'a Element>,
    screen: &'a CompactScreen,
    raw: &CompactScreen,
    category: RiskCategory,
    tree: Option<&ScreenTree<'_>>,
    typed: &[TypedText],
    return_only: bool,
) -> Sent<'a> {
    let none = || Sent {
        fields: Vec::new(),
        total: 0,
        listed: Vec::new(),
        mode: SendMode::Tap,
        sends: false,
    };
    let raw_len = |e: &Element| {
        raw.get(e.index)
            .and_then(|r| r.value.as_deref())
            .map_or(0, n_chars)
    };
    // Evidence that Jarvis typed in a field (its place in `Sends:` only, tenth review): the
    // fingerprint taken when typing, or its value being a text Jarvis typed.
    let typed_in = |e: &Element| {
        raw.get(e.index).is_some_and(|r| {
            let v = r.value.as_deref().map(str::trim).unwrap_or_default();
            typed
                .iter()
                .any(|x| x.field == r.fingerprint || (!v.is_empty() && x.text.trim() == v))
        })
    };
    // Thirteenth review: from the one classification of what the action actuates.
    let kind = match target {
        Some(t) if return_only && matches!(action, Action::Type { .. }) => {
            Actuation::Return(Some(t.index))
        }
        _ => actuation(action, raw),
    };
    // Fourteenth review (Rule E): a press on something unlisted sends what the whole screen
    // holds, as far as anyone can tell.
    if matches!(kind, Actuation::PressUnknown) {
        let f = screen_form(screen, tree);
        if !f.has_filled() {
            return none();
        }
        let c = jarvis_proto::Point::new(screen.size.width / 2.0, screen.size.height / 2.0);
        return sent_of(f, SendMode::Tap, None, c, true, raw, typed);
    }
    let Some(t) = target else {
        return none();
    };
    let form = || form_of(t, screen, tree);
    let (form, mode, first) = match kind {
        Actuation::Return(_) if return_only => {
            let f = form();
            if submit_return(t, screen) && !f.is_empty() {
                (f, SendMode::ReturnForm, Some(t))
            } else {
                return Sent {
                    fields: vec![sent_field(t, "", raw_len(t), typed_in(t))],
                    total: 1,
                    listed: vec![t],
                    mode: SendMode::ReturnAlone,
                    sends: true,
                };
            }
        }
        Actuation::Press(_) | Actuation::Drag(_) => {
            // Pressing a field, a switch or a picker sends nothing; nor does dragging one,
            // but for a slider (a "slide to …" control may submit, thirteenth review).
            let dragged_slider = matches!(kind, Actuation::Drag(_)) && slider_like(&t.role);
            if (is_value_role(&t.role) && !dragged_slider)
                || is_switch_like(&t.role)
                || t.role.is_text_input()
            {
                return none();
            }
            let f = form();
            if !(tap_sends(t, category) || f.has_filled()) {
                return none();
            }
            (f, SendMode::Tap, None)
        }
        Actuation::Return(_)
        | Actuation::PressUnknown
        | Actuation::Navigation
        | Actuation::NoEffect => return none(),
    };
    let sends = mode != SendMode::Tap
        || tap_sends(t, category)
        || matches!(category, RiskCategory::Unverified);
    sent_of(form, mode, first, t.frame.center(), sends, raw, typed)
}

/// What a form sends (see [`what_is_sent`]): `first` (Return's own field) first, then the
/// form's fields, the ones nearest `center` read first.
fn sent_of<'a>(
    form: Form<'a, '_>,
    mode: SendMode,
    first: Option<&'a Element>,
    center: jarvis_proto::Point,
    sends: bool,
    raw: &CompactScreen,
    typed: &[TypedText],
) -> Sent<'a> {
    let raw_len = |e: &Element| {
        raw.get(e.index)
            .and_then(|r| r.value.as_deref())
            .map_or(0, n_chars)
    };
    let typed_in = |e: &Element| {
        raw.get(e.index).is_some_and(|r| {
            let v = r.value.as_deref().map(str::trim).unwrap_or_default();
            typed
                .iter()
                .any(|x| x.field == r.fingerprint || (!v.is_empty() && x.text.trim() == v))
        })
    };
    // (rank, position, field): typed 0, recipient- or amount-named 1, the rest 2.
    let rank = |f: &SentField| {
        if f.typed {
            0u8
        } else if f.recipient || f.amount {
            1
        } else {
            2
        }
    };
    let mut ranked: Vec<(u8, usize, SentField)> = Vec::new();
    let mut listed: Vec<&Element> = form.listed.clone();
    // Eleventh review (F9): at most `MAX_LISTED_INPUTS` listed fields are read, the ones
    // nearest the target (in screen order); the rest are counted in `N fields`.
    let mut read: Vec<usize> = (0..form.listed.len()).collect();
    let listed_more = read.len().saturating_sub(MAX_LISTED_INPUTS);
    if listed_more > 0 {
        let c = center;
        let dist = |k: usize| {
            form.listed.get(k).map_or(f64::MAX, |e| {
                let d = e.frame.center();
                let v = (d.x - c.x).hypot(d.y - c.y);
                if v.is_finite() {
                    v
                } else {
                    f64::MAX
                }
            })
        };
        read.select_nth_unstable_by(MAX_LISTED_INPUTS, |&a, &b| {
            dist(a).total_cmp(&dist(b)).then(a.cmp(&b))
        });
        read.truncate(MAX_LISTED_INPUTS);
        read.sort_unstable();
    }
    for k in read {
        if let Some(e) = form.listed.get(k) {
            let f = sent_field(e, "", raw_len(e), typed_in(e));
            ranked.push((rank(&f), k, f));
        }
    }
    let base = form.listed.len();
    for (k, r) in form.unlisted.iter().enumerate() {
        let note = if r.on_screen {
            " (not listed)"
        } else {
            " (not on screen)"
        };
        let (el, len) = raw_input_element(r);
        let f = sent_field(&el, note, len, raw_typed(r, typed));
        ranked.push((rank(&f), base + k, f));
    }
    ranked.sort_by_key(|(rank, k, _)| (*rank, *k));
    let mut fields: Vec<SentField> = Vec::new();
    if let Some(field) = first {
        fields.push(sent_field(field, "", raw_len(field), typed_in(field)));
        listed.push(field);
    }
    fields.extend(ranked.into_iter().map(|(_, _, f)| f));
    let total = fields
        .len()
        .saturating_add(form.unlisted_more)
        .saturating_add(listed_more);
    Sent {
        fields,
        total,
        listed,
        mode,
        sends,
    }
}

/// One field of `Sends:` with its value cut to `cap` characters (0: the value is not
/// quoted, only how long it is), or `Name (shown above)` when its value is under
/// `To/Account:` (`above`).
fn sent_item(f: &SentField, cap: usize, above: bool) -> String {
    let name = &f.name;
    if above {
        return format!("{name} (shown above){}", f.note);
    }
    let value = match f.value.as_deref() {
        None => " (hidden)".to_owned(),
        Some("") => " (empty)".to_owned(),
        Some(v) => {
            let n = n_chars(v);
            if n <= cap && f.beyond == 0 {
                format!(": \"{v}\"")
            } else if cap == 0 {
                let left = n.saturating_add(f.beyond);
                format!(" (filled, {left} character{} not shown)", plural(left))
            } else {
                let head: String = v.chars().take(cap).collect();
                let left = n.saturating_sub(cap.min(n)).saturating_add(f.beyond);
                format!(
                    ": \"{}…\" ({left} more character{} not shown)",
                    head.trim_end(),
                    plural(left)
                )
            }
        }
    };
    format!("{name}{value}{}", f.note)
}

/// `"s"` unless `n` is 1.
fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// The largest `v` in `lo..=hi` for which `fits(v)` holds (`fits` is monotone: true up to
/// some value), or `None` when it fails at `lo`.
fn largest_fitting(lo: usize, hi: usize, mut fits: impl FnMut(usize) -> bool) -> Option<usize> {
    if hi < lo || !fits(lo) {
        return None;
    }
    let (mut ok, mut bad) = (lo, hi.saturating_add(1));
    while bad - ok > 1 {
        let mid = ok + (bad - ok) / 2;
        if fits(mid) {
            ok = mid;
        } else {
            bad = mid;
        }
    }
    Some(ok)
}

/// Slot 4 (ninth review): what is sent, within `budget` characters. `Sends:` for a
/// sending action, `The form holds:` for any other; Return alone says `, which sends the
/// field's whole content: "…"`. Each value is shown up to [`MAX_SENT_VALUE_CHARS`]; to fit,
/// the values are cut to a common length (down to 24), then the list is cut from its end
/// (`Sends: N fields (k shown): …`), then the values are left out (`(filled, N characters
/// not shown)`), then only the count stays. `above[i]`: field `i` is shown under
/// `To/Account:`. Every cut is stated.
fn sends_slot(sent: &Sent<'_>, above: &[bool], budget: usize) -> String {
    let fields = &sent.fields;
    if fields.is_empty() && sent.total == 0 {
        return String::new();
    }
    let is_above = |i: usize| above.get(i).copied().unwrap_or(false);
    if sent.mode == SendMode::ReturnAlone {
        let render = |cap: usize| -> String {
            match fields.first() {
                Some(f) => {
                    let item = sent_item(f, cap, false);
                    let rest = item.get(f.name.len()..).unwrap_or_default();
                    format!(", which sends the field's whole content{rest}")
                }
                None => ", which sends the field's whole content".to_owned(),
            }
        };
        if let Some(cap) =
            largest_fitting(1, MAX_SENT_VALUE_CHARS, |c| n_chars(&render(c)) <= budget)
        {
            return render(cap);
        }
        // No room for any of the value: how long it is.
        let left = fields.first().map_or(0, |f| {
            f.value
                .as_deref()
                .map_or(0, n_chars)
                .saturating_add(f.beyond)
        });
        return format!(
            ", which sends the field's whole content ({left} character{} not shown)",
            plural(left)
        );
    }
    let prefix = if sent.mode == SendMode::ReturnForm {
        ", which submits the form. "
    } else {
        ". "
    };
    let word = if sent.sends || sent.mode == SendMode::ReturnForm {
        "Sends"
    } else {
        "The form holds"
    };
    let render = |k: usize, cap: usize| -> String {
        let items: Vec<String> = fields
            .iter()
            .take(k)
            .enumerate()
            .map(|(i, f)| sent_item(f, cap, is_above(i)))
            .collect();
        let head = if k >= sent.total {
            format!("{prefix}{word}: ")
        } else if k == 0 {
            format!("{prefix}{word}: {} fields (0 shown)", sent.total)
        } else {
            format!("{prefix}{word}: {} fields ({k} shown): ", sent.total)
        };
        format!("{head}{}", items.join("; "))
    };
    let all = fields.len();
    let fits = |k: usize, cap: usize| n_chars(&render(k, cap)) <= budget;
    if let Some(cap) = largest_fitting(24, MAX_SENT_VALUE_CHARS, |c| fits(all, c)) {
        return render(all, cap);
    }
    if let Some(k) = largest_fitting(1, all, |k| fits(k, 24)) {
        return render(k, 24);
    }
    if let Some(k) = largest_fitting(1, all, |k| fits(k, 0)) {
        return render(k, 0);
    }
    render(0, 0)
}

/// Whether the session's typing in this task makes a tap `action` a presumed submit
/// (eighth review, mirroring Return in a text view): the target is a control in the same
/// form or composer ([`form_of`]) as a text input Jarvis typed into during this task that
/// still holds text (not a search field, not secure), and it is not clearly something
/// else: not a text input, switch, slider, picker, stepper or tab, not in a tab bar, and
/// not labelled like cancel, close, back, clear, emoji, dictation, attach, add, photo,
/// camera, keyboard switch, formatting, expand/collapse or show/hide password
/// ([`NOT_SUBMIT_WORDS`]; "forgot" since the ninth review). Taps, long presses and tap
/// points (their riskiest control); not on the home screen.
///
/// **Ninth review.** The typed field is found among the form's unlisted inputs too (a field
/// scrolled out of the list: by the fingerprint compaction gave it, or by the text Jarvis
/// typed), at most [`MAX_UNLISTED_INPUTS`] of them. Narrowing (a): in a login form (it
/// holds a secure field, and every filled non-secure field is named like an account —
/// email, username, user, login, ID, account, phone, mobile) a tap is not presumed to send
/// what was typed; its own label is still classified.
///
/// **Tenth review: the "Jarvis typed here" state.** The typed field is no longer
/// re-identified: its fingerprint hashes the row's merged label and context, so a
/// self-drawn placeholder that hides once text is in, a counter "0/500" → "12/500", or the
/// app reformatting the text lost the presumption. Now `typed` being non-empty *is* the
/// state — the session passes the texts it typed in the app in front, and (eleventh
/// review) keeps them for the whole task (see the agent's `Session::typed_here`) — and
/// while it is set a tap on a control (exclusions as above)
/// whose form holds **any** filled, non-search, non-secure text input, listed or not, is
/// presumed to submit it. The fingerprint and typed-text matches only order `Sends:`
/// (typed fields first).
///
/// **Eleventh review: Return.** `type … submit` (the Return part, gated on the screen after
/// the typing) is presumed to submit too, see [`presumed_return`].
///
/// **Fourteenth review.** A press on something the screen does not list (a `tap_point`
/// resolving to no element, Rule E) is presumed to submit when the state is set and any
/// input on the screen is filled (the form is the screen). A search field is exempt only
/// while what it submits is the owner's own words (Rule B, [`presumed_submit_owned`]); this
/// function knows no owner text, so no search field is exempt here. Every label exemption
/// is a whole-label match (Rule F, [`not_submit_label`]).
pub fn presumed_submit(
    action: &Action,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
    typed: &[TypedText],
) -> bool {
    presumed_submit_owned(action, screen, tree, typed, &[])
}

/// [`presumed_submit`], knowing what the owner wrote for this task (`owner`, as for
/// `evaluate_owned`): a search field is exempt from the presumption only while the text it
/// submits (typed text, else its current value) is the owner's own words
/// ([`owner_words`]: every character of it is accounted for by the owner's text). Fourteenth review, Rule B: the field's name, which any page
/// controls, no longer exempts it alone.
pub fn presumed_submit_owned(
    action: &Action,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
    typed: &[TypedText],
    owner: &[&str],
) -> bool {
    if is_springboard(&screen.app_bundle_id) {
        return false;
    }
    // Return after `type` is presumed on its own terms (the typing sets the state); every
    // other actuation needs the state (the cheap test first: no classification without it).
    if typed.is_empty() && !matches!(action, Action::Type { submit: true, .. }) {
        return false;
    }
    // Thirteenth review: one exhaustive classification decides what is actuated.
    let t = match actuation(action, screen) {
        Actuation::Return(element) => {
            let text = match action {
                Action::Type { text, .. } => text.as_str(),
                _ => "",
            };
            return presumed_return(element, text, screen, tree, typed, owner);
        }
        Actuation::Press(t) => {
            // Pressing a value control (slider, picker, stepper) sets its value.
            if is_value_role(&t.role) {
                return false;
            }
            t
        }
        Actuation::Drag(t) => {
            // Dragging a picker or stepper sets its value; a slider dragged to its end (a
            // "slide to …" control) may submit.
            if is_value_role(&t.role) && !slider_like(&t.role) {
                return false;
            }
            t
        }
        // Fourteenth review (Rule E): what an unlisted spot is cannot be read; after
        // typing, any filled input on the screen may be what it submits.
        Actuation::PressUnknown => {
            return form_presumes(&screen_form(screen, tree), None, owner);
        }
        Actuation::Navigation | Actuation::NoEffect => return false,
    };
    if t.role.is_text_input()
        || is_switch_like(&t.role)
        || t.role == Role::Tab
        || not_submit_label(&t.label)
    {
        return false;
    }
    if let Some(tr) = tree {
        if tr
            .bars
            .iter()
            .any(|b| b.role == Role::TabBar && tr.inside(t, b.node))
        {
            return false;
        }
    }
    let form = form_of(t, screen, tree);
    form_presumes(&form, None, owner)
}

/// The whole screen as one form (fourteenth review, Rule E): every listed text input and,
/// on a raw tree, every unlisted one (at most [`MAX_UNLISTED_INPUTS`]).
fn screen_form<'a, 't>(screen: &'a CompactScreen, tree: Option<&ScreenTree<'t>>) -> Form<'a, 't> {
    let inputs: Vec<&Element> = screen
        .elements
        .iter()
        .filter(|e| e.role.is_text_input())
        .collect();
    let mut unlisted: Vec<&RawInput> = Vec::new();
    let mut has_secure = inputs.iter().any(|e| is_secure_input(e));
    if let Some(t) = tree {
        let listed: std::collections::BTreeSet<u32> = t.sources.iter().copied().collect();
        for r in t.inputs.iter().filter(|r| !listed.contains(&r.node)) {
            if r.secure || r.role == Role::SecureTextField {
                has_secure = true;
            } else {
                unlisted.push(r);
            }
        }
    }
    let unlisted_more = unlisted.len().saturating_sub(MAX_UNLISTED_INPUTS);
    unlisted.truncate(MAX_UNLISTED_INPUTS);
    Form {
        listed: inputs.into_iter().filter(|e| !is_secure_input(e)).collect(),
        unlisted,
        has_secure,
        unlisted_more,
        root: None,
    }
}

/// A slider (thirteenth review: dragged to its end, a "slide to …" control submits).
fn slider_like(role: &Role) -> bool {
    match role {
        Role::Slider => true,
        Role::Other(name) => name.to_lowercase().contains("slider"),
        _ => false,
    }
}

/// Whether a field is a search field: role `SearchField`, or "search" in its name or
/// placeholder. Fourteenth review: this only names the field; a search field is exempt
/// from the presumption only with the owner's words in it ([`search_exempt`]).
fn search_named(role: &Role, name: &str, placeholder: Option<&str>) -> bool {
    *role == Role::SearchField
        || has_word(name, &["search"])
        || has_word(placeholder.unwrap_or_default(), &["search"])
}

/// Rule B: whether a search-named field submitting `text` is exempt from the presumed
/// submit: every word of it is the owner's ([`owner_words`]).
fn search_exempt(
    role: &Role,
    name: &str,
    placeholder: Option<&str>,
    text: &str,
    owner: &[&str],
) -> bool {
    search_named(role, name, placeholder) && owner_words(text, owner)
}

/// Whether `form` makes a submit presumed (tenth review): it holds a filled, non-secure
/// text input, listed or not, that is not a search field holding the owner's words (Rule
/// B), and is not a login form (narrowing (a)). With `also` (eleventh review: Return after
/// `type`), that field counts too — as filled by the text being typed when that is not
/// empty.
fn form_presumes(form: &Form<'_, '_>, also: Option<(&Element, &str)>, owner: &[&str]) -> bool {
    // Narrowing (a), ninth review: a login form (a secure field, and only email or
    // username fields filled besides) is not presumed submitted by what Jarvis typed in the
    // email field; the target's own label is still classified, and typing the password is
    // handed to the owner as before.
    let filled = |e: &Element, text: &str| !text.trim().is_empty() || field_value(e).is_some();
    let also_name = also.filter(|(e, text)| filled(e, text)).map(|(e, _)| {
        format!(
            "{} {}",
            field_name(e),
            e.placeholder.as_deref().unwrap_or_default()
        )
    });
    if form.has_secure && form.filled_names().chain(also_name).all(|n| login_name(&n)) {
        return false;
    }
    // Tenth review: any filled, non-secure text input of the form, listed or not — the
    // typed field is no longer re-identified (its fingerprint changes when a placeholder
    // hides or a counter moves, the app reformats the text, …). Fourteenth review: a search
    // field counts unless it holds the owner's words.
    let open = |e: &Element, text: &str| {
        !is_secure_input(e)
            && !(search_exempt(
                &e.role,
                &field_name(e),
                e.placeholder.as_deref(),
                text,
                owner,
            ) && field_value(e).is_none_or(|v| owner_words(v, owner)))
    };
    if also.is_some_and(|(e, text)| filled(e, text) && open(e, text)) {
        return true;
    }
    let listed = form
        .listed
        .iter()
        .any(|e| field_value(e).is_some() && open(e, ""));
    listed
        || form.unlisted.iter().any(|r| {
            let name = r
                .label
                .as_deref()
                .or(r.placeholder.as_deref())
                .or(r.identifier.as_deref())
                .unwrap_or_default();
            let value = r.value.as_deref().unwrap_or_default();
            !r.secure
                && r.role != Role::SecureTextField
                && !value.trim().is_empty()
                && !search_exempt(&r.role, name, r.placeholder.as_deref(), value, owner)
        })
}

/// Eleventh review (F1): whether Return after `type` into `element` (the `submit` of a
/// `type`, gated as its own part on the screen after the typing) is presumed to submit what
/// the form holds. The typing itself sets the app's "typed here" state, so this holds
/// whenever the field is not secure, not a search field holding the owner's words
/// (fourteenth review, Rule B: the typed text and the field's value are both the owner's)
/// and not a multi-line text view whose Return inserts a line break (Return key not a
/// submit label: that Return is no submit), and the form — the field included, counted as
/// filled by `text` when it is not empty — holds a filled input that counts (narrowing
/// (a), the login form, unchanged). Empty `text` (Return alone) needs the state already
/// set (`typed`) or, thirteenth review, the field itself to hold text (what Return sends
/// is observed).
fn presumed_return(
    element: Option<u32>,
    text: &str,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
    typed: &[TypedText],
    owner: &[&str],
) -> bool {
    let types = !text.trim().is_empty();
    let Some(field) = type_target(element, screen) else {
        return false;
    };
    // Thirteenth review: Return alone sends what the field holds, whoever put it there
    // (pasted, AutoFilled, dictated, typed earlier): a field that holds text is presumed
    // submitted by it even without the "typed here" state.
    if !types && typed.is_empty() && field_value(field).is_none() {
        return false;
    }
    if !field.role.is_text_input()
        || is_secure_input(field)
        || (search_exempt(
            &field.role,
            &field_name(field),
            field.placeholder.as_deref(),
            text,
            owner,
        ) && field_value(field).is_none_or(|v| owner_words(v, owner)))
    {
        return false;
    }
    // A multi-line text view whose Return key is not a submit label inserts a line break
    // (the classifier still reads the text view as a composer).
    if field.role == Role::TextView && !submit_return(field, screen) {
        return false;
    }
    let form = form_of(field, screen, tree);
    form_presumes(&form, Some((field, text)), owner)
}

/// Field names of a login form's account field (narrowing (a), ninth review): email,
/// username, user, login, Apple ID, account, phone or mobile number.
const LOGIN_NAME_WORDS: &[&str] = &[
    "email", "mail", "username", "user", "login", "id", "account", "phone", "mobile",
];

/// Whether a field name reads like a login's account field ([`LOGIN_NAME_WORDS`]).
fn login_name(name: &str) -> bool {
    let lower = name.to_lowercase().replace(['-', '_'], "");
    has_word(&lower, LOGIN_NAME_WORDS) || lower.contains("email") || lower.contains("username")
}

/// Whole labels of a control next to a composer that clearly does not submit it (see
/// [`presumed_submit`]; fourteenth review, Rule F: a closed list of whole phrases, compared
/// with the whole label — a label that merely contains one of these words is not exempt).
const NOT_SUBMIT_PHRASES: &[&str] = &[
    "forgot password",
    "forgot your password",
    "forgot username",
    "forgot your username",
    "forgot email",
    "forgot id",
    "forgot apple id",
    "forgot password or apple id",
    "cancel",
    "close",
    "back",
    "go back",
    "dismiss",
    "clear",
    "clear text",
    "clear search",
    "delete",
    "emoji",
    "emojis",
    "emoji keyboard",
    "sticker",
    "stickers",
    "dictate",
    "dictation",
    "microphone",
    "mic",
    "voice input",
    "attach",
    "attach file",
    "attachment",
    "attachments",
    "add attachment",
    "paperclip",
    "add",
    "plus",
    "add photo",
    "add photos",
    "photo",
    "photos",
    "photo library",
    "camera",
    "take photo",
    "keyboard",
    "next keyboard",
    "hide keyboard",
    "dismiss keyboard",
    "globe",
    "format",
    "formatting",
    "text format",
    "text formatting",
    "bold",
    "italic",
    "underline",
    "strikethrough",
    "expand",
    "collapse",
    "show",
    "hide",
    "show password",
    "hide password",
];

/// SF Symbol names (as an unlabelled image button's label) that clearly do not submit
/// (Rule F): one of these, optionally followed by [`SF_SYMBOL_VARIANTS`] components
/// (`xmark.circle.fill`, `eye.slash`).
const NOT_SUBMIT_SYMBOLS: &[&str] = &[
    "xmark",
    "face.smiling",
    "paperclip",
    "mic",
    "waveform",
    "plus",
    "photo",
    "photo.on.rectangle",
    "photo.on.rectangle.angled",
    "camera",
    "globe",
    "keyboard",
    "keyboard.chevron.compact.down",
    "textformat",
    "textformat.alt",
    "bold",
    "italic",
    "underline",
    "strikethrough",
    "chevron.down",
    "chevron.up",
    "chevron.left",
    "chevron.backward",
    "eye",
    "delete.left",
];

/// Variant components an SF Symbol name may carry after its base.
const SF_SYMBOL_VARIANTS: &[&str] = &["fill", "circle", "square", "slash", "badge", "rectangle"];

/// Whether `label` names a control that clearly does not submit (fourteenth review, Rule
/// F): its **whole** label — trimmed, case-folded, inner spaces collapsed, punctuation at
/// the ends dropped — equals one of [`NOT_SUBMIT_PHRASES`], or it is an SF Symbol name of
/// [`NOT_SUBMIT_SYMBOLS`]. "Back to shop", "Continue & close" or "Add comment" are not.
fn not_submit_label(label: &str) -> bool {
    let lower = label.trim().to_lowercase();
    let symbol = lower.trim();
    if !symbol.is_empty()
        && symbol
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.')
        && NOT_SUBMIT_SYMBOLS.iter().any(|base| {
            symbol == *base
                || symbol.strip_prefix(base).is_some_and(|rest| {
                    rest.starts_with('.')
                        && rest
                            .split('.')
                            .skip(1)
                            .all(|v| SF_SYMBOL_VARIANTS.contains(&v))
                })
        })
    {
        return true;
    }
    let phrase = lower
        .trim_matches(|c: char| !c.is_alphanumeric())
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    NOT_SUBMIT_PHRASES.contains(&phrase.as_str())
}

/// The summary's reserved slots (ninth review), in priority order, each as (minimum,
/// maximum) characters of a [`MAX_SUMMARY_CHARS`] summary: (1) the action line — verb,
/// target, app, and Return's `Typed:`; (2) the amounts; (3) the recipients; (4) what is
/// sent; (5) the other facts (`next to`, `under`, `Selected:`, `Fields:`). See
/// `summarize_with`.
const SLOT_ACTION: (usize, usize) = (92, 220);
/// Slot 2, the amounts: the total and the largest amount alone always fit the minimum.
const SLOT_AMOUNTS: (usize, usize) = (90, 200);
/// Slot 3, the recipients. Eleventh review: the minimum is what the slot's last level
/// needs for the screen at hand (`recipients_slot`), at least 72 and at most
/// [`MAX_RECIPIENTS_MIN`]: one entry at its minimal cut — for an email `. To/Account (NN
/// different): ` (29), its field name cut to 12, `: "` (3), 6 characters of its local
/// part, `…@` (2), a host of 40, `"` (1), ` (2000 not shown)` (17), ` (look-alike
/// letters)` (21) and ` and NN more` (12), 143 at most — and `. Screen says: NN more lines
/// naming a recipient` (47, twelfth review): 190.
const SLOT_RECIPIENTS: (usize, usize) = (72, 240);
/// The largest minimum the recipients slot reserves (see [`SLOT_RECIPIENTS`]).
const MAX_RECIPIENTS_MIN: usize = 190;
/// Slot 4, what is sent: the count always fits the minimum; no maximum of its own.
const SLOT_SENDS: (usize, usize) = (76, usize::MAX);
/// Slot 5, the other facts: their counts fit the minimum.
const SLOT_OTHER: (usize, usize) = (48, 160);

/// `raw` with every label, value and placeholder cut to [`MAX_READ_VALUE_CHARS`] (before
/// redaction; ninth review).
fn read_capped(raw: &CompactScreen) -> CompactScreen {
    let long = |s: &str| s.len() > MAX_READ_VALUE_CHARS;
    if !raw.elements.iter().any(|e| {
        long(&e.label)
            || e.value.as_deref().is_some_and(long)
            || e.placeholder.as_deref().is_some_and(long)
    }) {
        return raw.clone();
    }
    let mut out = raw.clone();
    for e in &mut out.elements {
        e.label = read_cap(&e.label);
        e.value = e.value.as_deref().map(read_cap);
        e.placeholder = e.placeholder.as_deref().map(read_cap);
    }
    out
}

/// Most elements a question reads its facts from besides the text inputs and the target
/// (tenth review): on a larger screen the ones nearest the target (the first ones without a
/// target), and the question says so (`. Facts from the 250 nearest elements`).
const MAX_FACT_ELEMENTS: usize = 250;
/// Most characters of one element's label, value or placeholder read for the facts (tenth
/// review; the target's and the text inputs' excepted): a longer text keeps its first three
/// quarters and its last quarter of this, so a trailing amount is still read.
const MAX_FACT_TEXT_CHARS: usize = 500;

/// Most elements beyond the nearest [`MAX_FACT_ELEMENTS`] read because they hold a
/// recipient heading or an exact total (eleventh review; the nearest ones when more).
const MAX_FACT_CANDIDATES: usize = 250;
/// Longest text the candidate test reads of an element (a heading pair or an exact total
/// is short; a longer text is never one of the exact totals).
const MAX_CANDIDATE_CHARS: usize = 200;

/// What the cheap pre-filter found in an element (see `fact_screen`).
enum Candidate {
    No,
    /// A recipient heading pair or an exact total phrase.
    Yes,
    /// A recipient heading alone ("Pay to"), whose name is the next text.
    HeadingAlone,
}

/// The pre-filter of `fact_screen` (eleventh review, F7): whether `e` may hold a recipient
/// heading pair, a heading alone or an exact total, reading at most
/// [`MAX_CANDIDATE_CHARS`] of its text.
fn fact_candidate(e: &Element) -> Candidate {
    if e.secure || e.role == Role::Key || e.role.is_text_input() {
        return Candidate::No;
    }
    let text = &e.label;
    if text.len() > MAX_CANDIDATE_CHARS * 4 || n_chars(text) > MAX_CANDIDATE_CHARS {
        return Candidate::No;
    }
    let t = text.trim();
    if recipient_heading_alone(t) {
        return Candidate::HeadingAlone;
    }
    if recipient_heading(t) || (t.chars().any(crate::text::is_digit) && exact_total(t)) {
        return Candidate::Yes;
    }
    Candidate::No
}

/// `s` within [`MAX_FACT_TEXT_CHARS`] characters: its head and its tail.
fn fact_cap(s: &str) -> String {
    if s.len() <= MAX_FACT_TEXT_CHARS {
        return s.to_owned();
    }
    let n = n_chars(s);
    if n <= MAX_FACT_TEXT_CHARS {
        return s.to_owned();
    }
    let head = MAX_FACT_TEXT_CHARS * 3 / 4;
    let tail = MAX_FACT_TEXT_CHARS - head - 1;
    let mut out: String = s.chars().take(head).collect();
    out.push(' ');
    out.extend(s.chars().skip(n - tail));
    out
}

/// The part of a screen a question reads (see [`fact_screen`]).
struct FactScreen {
    screen: CompactScreen,
    /// Elements other than text inputs were left out ([`MAX_FACT_ELEMENTS`]).
    capped: bool,
    /// Listed text inputs of the target's form left out ([`MAX_LISTED_INPUTS`]).
    form_dropped: usize,
    /// Other listed, non-secure text inputs left out.
    other_dropped: usize,
}

/// The distance between the centres of `a` (when known) and `e` (`f64::MAX` when either
/// frame is empty or the distance is not finite; 0 without an anchor).
fn distance_to(a: Option<jarvis_proto::Point>, e: &Element) -> f64 {
    match a {
        Some(a) if !e.frame.is_empty() => {
            let c = e.frame.center();
            let d = (a.x - c.x).hypot(a.y - c.y);
            if d.is_finite() {
                d
            } else {
                f64::MAX
            }
        }
        Some(_) => f64::MAX,
        None => 0.0,
    }
}

/// The part of `raw` a question reads (tenth review, bounded work): the text inputs (their
/// values bounded by [`MAX_READ_VALUE_CHARS`]), the target, and the [`MAX_FACT_ELEMENTS`]
/// other elements nearest the target by distance between centres (ties: screen order; the
/// first ones without a target), in screen order, with the texts of the others cut to
/// [`MAX_FACT_TEXT_CHARS`]. Eleventh review (F7): plus the recipient headings and exact
/// totals found among all elements ([`fact_candidate`]); (F9): at most
/// [`MAX_LISTED_INPUTS`] text inputs, those of the target's form nearest it first, then the
/// others nearest it; the ones left out are counted (the question's `N fields` and
/// `and N more` include them).
fn fact_screen(
    raw: &CompactScreen,
    target: Option<u32>,
    tree: Option<&ScreenTree<'_>>,
) -> FactScreen {
    let anchor = target
        .and_then(|i| raw.get(i))
        .filter(|t| !t.frame.is_empty())
        .map(|t| t.frame.center());
    // (F9) The text inputs read.
    let inputs: Vec<usize> = raw
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.role.is_text_input() && Some(e.index) != target)
        .map(|(k, _)| k)
        .collect();
    let mut read_inputs: Option<std::collections::HashSet<usize>> = None;
    let (mut form_dropped, mut other_dropped) = (0usize, 0usize);
    if inputs.len() > MAX_LISTED_INPUTS {
        let in_form: std::collections::HashSet<u32> = match target.and_then(|i| raw.get(i)) {
            Some(t) => form_of(t, raw, tree)
                .listed
                .iter()
                .map(|e| e.index)
                .collect(),
            None => std::collections::HashSet::new(),
        };
        let mut form: Vec<(f64, usize)> = Vec::new();
        let mut rest: Vec<(f64, usize)> = Vec::new();
        for &k in &inputs {
            let Some(e) = raw.elements.get(k) else {
                continue;
            };
            let d = distance_to(anchor, e);
            if in_form.contains(&e.index) {
                form.push((d, k));
            } else {
                rest.push((d, k));
            }
        }
        let by = |a: &(f64, usize), b: &(f64, usize)| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1));
        form.sort_by(by);
        rest.sort_by(by);
        let take_form = form.len().min(MAX_LISTED_INPUTS);
        let take_rest = rest.len().min(MAX_LISTED_INPUTS - take_form);
        let counted = |e: &Element| !is_secure_input(e);
        form_dropped = form
            .iter()
            .skip(take_form)
            .filter(|(_, k)| raw.elements.get(*k).is_some_and(counted))
            .count();
        other_dropped = rest
            .iter()
            .skip(take_rest)
            .filter(|(_, k)| raw.elements.get(*k).is_some_and(counted))
            .count();
        read_inputs = Some(
            form.iter()
                .take(take_form)
                .chain(rest.iter().take(take_rest))
                .map(|(_, k)| *k)
                .collect(),
        );
    }
    let mut others: Vec<(f64, usize)> = raw
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.role.is_text_input() && Some(e.index) != target)
        .map(|(k, e)| (distance_to(anchor, e), k))
        .collect();
    let capped = others.len() > MAX_FACT_ELEMENTS;
    let long = |s: &str| s.len() > MAX_FACT_TEXT_CHARS;
    let any_long = raw.elements.iter().any(|e| {
        long(&e.label)
            || e.value.as_deref().is_some_and(long)
            || e.placeholder.as_deref().is_some_and(long)
    });
    if !capped && !any_long && read_inputs.is_none() {
        return FactScreen {
            screen: raw.clone(),
            capped: false,
            form_dropped: 0,
            other_dropped: 0,
        };
    }
    // Eleventh review (F7): recipient headings and exact totals are looked for among *all*
    // elements (a cheap test on a bounded part of each text), before the nearest-elements
    // cap is applied to the rest, so a payee or a total beyond the cap is still read.
    let mut candidates: Vec<(f64, usize)> = Vec::new();
    if capped {
        for &(d, k) in &others {
            let Some(e) = raw.elements.get(k) else {
                continue;
            };
            match fact_candidate(e) {
                Candidate::No => {}
                Candidate::Yes => candidates.push((d, k)),
                Candidate::HeadingAlone => {
                    candidates.push((d, k));
                    // A heading alone ("Pay to") is read with the text after it.
                    if raw
                        .elements
                        .get(k + 1)
                        .is_some_and(|n| n.role == Role::StaticText && Some(n.index) != target)
                    {
                        candidates.push((d, k + 1));
                    }
                }
            }
        }
        // Bounded: the nearest ones when a page holds very many.
        if candidates.len() > MAX_FACT_CANDIDATES {
            candidates.select_nth_unstable_by(MAX_FACT_CANDIDATES, |a, b| {
                a.0.total_cmp(&b.0).then(a.1.cmp(&b.1))
            });
            candidates.truncate(MAX_FACT_CANDIDATES);
        }
        others.select_nth_unstable_by(MAX_FACT_ELEMENTS, |a, b| {
            a.0.total_cmp(&b.0).then(a.1.cmp(&b.1))
        });
        others.truncate(MAX_FACT_ELEMENTS);
    }
    let kept: std::collections::HashSet<usize> = others
        .into_iter()
        .chain(candidates)
        .map(|(_, k)| k)
        .collect();
    let elements = raw
        .elements
        .iter()
        .enumerate()
        .filter_map(|(k, e)| {
            if Some(e.index) == target {
                return Some(e.clone());
            }
            if e.role.is_text_input() {
                let read = read_inputs.as_ref().is_none_or(|r| r.contains(&k));
                return read.then(|| e.clone());
            }
            if !kept.contains(&k) {
                return None;
            }
            let mut e = e.clone();
            e.label = fact_cap(&e.label);
            e.value = e.value.as_deref().map(fact_cap);
            e.placeholder = e.placeholder.as_deref().map(fact_cap);
            Some(e)
        })
        .collect();
    let out = CompactScreen {
        app_bundle_id: raw.app_bundle_id.clone(),
        app_name: raw.app_name.clone(),
        size: raw.size,
        keyboard_visible: raw.keyboard_visible,
        elements,
        cues: raw.cues.clone(),
    };
    FactScreen {
        screen: out,
        capped,
        form_dropped,
        other_dropped,
    }
}

/// A cleaned heading pair split into its heading (`Pay to: `, with its colon when it kept
/// one) and its name.
fn split_pair(entry: &str) -> (String, String) {
    let entry = entry.trim();
    let Some(rest) = split_recipient_heading(entry) else {
        return (String::new(), entry.to_owned());
    };
    let at = entry.len().saturating_sub(rest.len());
    let heading = entry.get(..at).unwrap_or_default().trim_end();
    let rest = rest.trim_start();
    match rest.strip_prefix(':') {
        Some(name) => (format!("{heading}: "), name.trim().to_owned()),
        None => (format!("{heading} "), rest.trim().to_owned()),
    }
}

/// The title of the screen (eleventh review, F3c): on a typed tree, the static text inside
/// the topmost navigation bar (the bars `jarvis_observe` reports have no web-view, scroll,
/// table or collection ancestor), else the bar's own label; cleaned and cut to
/// [`MAX_TITLE_CHARS`] (the cut marked `…`). `None` without the raw tree or a bar.
fn screen_title(screen: &CompactScreen, tree: Option<&ScreenTree<'_>>) -> Option<String> {
    let tree = tree?;
    let bar = tree
        .bars
        .iter()
        .filter(|b| b.role == Role::NavigationBar)
        .min_by(|a, b| a.frame.y.total_cmp(&b.frame.y))?;
    let text = screen
        .elements
        .iter()
        .find(|e| {
            e.role == Role::StaticText && !e.label.trim().is_empty() && tree.inside(e, bar.node)
        })
        .map(|e| e.label.clone())
        .or_else(|| {
            tree.nodes
                .iter()
                .find(|n| n.node == bar.node)
                .and_then(|n| n.label.clone())
        })?;
    let t = clean(&text, crate::sanitize::MAX_SCREEN_TEXT_CHARS);
    let t = t.trim();
    if t.is_empty() {
        return None;
    }
    Some(hard_cut(t, MAX_TITLE_CHARS))
}

/// Longest screen title in the action line (eleventh review).
const MAX_TITLE_CHARS: usize = 40;

/// How a summary is composed (ninth review: reserved slots, one pass). Walking the slots in
/// priority order — (1) the action, (2) the amounts, (3) the recipients, (4) what is sent,
/// (5) the other facts — each slot gets up to its maximum but never the summed minimums of
/// the present slots after it; what a slot leaves unused flows to the later ones. Each slot
/// renders within its allotment, saying what it leaves out (`and N more`, `N fields (k
/// shown)`, `(N more characters not shown)`), so no slot is ever dropped silently and the
/// whole is within the bound by construction (asserted in debug builds; a release build
/// cuts a slot that would not fit to its allotment, never another slot). The text keeps
/// the reading order: the action line (with `next to`/`under`), what is sent, `Typed:`, the
/// amounts, the recipients, `Selected:`, `Fields:`.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
fn summarize_with(
    action: &Action,
    raw: &CompactScreen,
    category: RiskCategory,
    source: ScreenSource,
    tree: Option<&ScreenTree<'_>>,
    typed: &[TypedText],
    return_only: bool,
    type_return: bool,
    max: usize,
) -> Vec<QuestionRow> {
    // Fifteenth review (R1): a `type … submit` is asked as one question — the typing and
    // the Return it presses — with Return's facts read on the screen as it will be after the
    // typing (the field holding what it holds now and the typed text after it).
    if !return_only && !type_return {
        if let Action::Type {
            element,
            text,
            submit: true,
        } = action
        {
            if let Some(t) = type_target(*element, raw)
                .filter(|t| !text.is_empty() && !is_secure_input(t) && !t.secure)
            {
                let value = after_text(Some(t), text);
                let mut after = raw.clone();
                if let Some(e) = after.elements.iter_mut().find(|e| e.index == t.index) {
                    e.value = Some(value.clone());
                }
                let mut typed_after = typed.to_vec();
                typed_after.push(TypedText {
                    field: t.fingerprint,
                    text: value,
                });
                return summarize_with(
                    action,
                    &after,
                    category,
                    source,
                    tree,
                    &typed_after,
                    false,
                    true,
                    max,
                );
            }
        }
    }
    // Twelfth review: a bare Return (`type` with no text, `submit`) is worded and sent like
    // Return ("Press Return in …, which sends …"), never `Type "" into …`.
    let return_only = return_only
        || type_return
        || matches!(action, Action::Type { text, submit: true, .. } if text.is_empty());
    // Actions in the foreground app state the screen's facts after the action itself.
    let target_index = match action {
        Action::Tap { element } | Action::LongPress { element, .. } | Action::Clear { element } => {
            Some(*element)
        }
        Action::Type { element, .. } => type_target(*element, raw).map(|e| e.index),
        Action::Scroll { element, .. } | Action::Swipe { element, .. } => *element,
        Action::TapPoint { x, y } => crate::classify::tap_point(*x, *y, raw).1.map(|e| e.index),
        _ => None,
    };
    // Facts and quoted labels come from the prompt's label-aware redaction, of values cut
    // to a bounded length first (ninth review), on a bounded part of the screen (tenth
    // review: the text inputs, the target and the elements nearest it, see `fact_screen`).
    let FactScreen {
        screen: reduced,
        capped,
        form_dropped,
        other_dropped,
    } = fact_screen(raw, target_index, tree);
    let redacted = redact_screen(&read_capped(&reduced));
    let screen = &redacted;
    let target = target_index.and_then(|i| screen.get(i));
    let anchor = Anchor::new(target, tree);
    // Thirteenth review: a drag sends the form of the control it actuates (its element, or
    // the control on its path or under the finger); the facts keep the action's target.
    let dragged = match action {
        Action::Scroll { .. } | Action::Swipe { .. } => match actuation(action, raw) {
            Actuation::Drag(e) => Some(screen.get(e.index).unwrap_or(e)),
            _ => None,
        },
        _ => None,
    };
    let mut sent = what_is_sent(
        action,
        dragged.or(target),
        screen,
        raw,
        category,
        tree,
        typed,
        return_only,
    );
    // (F9) The form's listed inputs the question did not read are counted.
    if !sent.fields.is_empty() || sent.total > 0 {
        sent.total = sent.total.saturating_add(form_dropped);
    }
    // A tap presumed to submit what Jarvis typed sends, whatever its category says.
    let presumed = presumed_submit(action, raw, tree, typed);
    sent.sends |= presumed;
    let with_facts = uses_foreground_app(action) && !matches!(action, Action::Look);
    // Eleventh review: posting counts as sending here too.
    let pays = SENDING_CATEGORIES.contains(&category);
    let (fields, fields_more) = if with_facts {
        let (f, more) = field_facts(screen, raw, target, typed, &sent.listed);
        // The form's unread inputs are not "other" fields when the action sends the form.
        let unread = if sent.fields.is_empty() {
            other_dropped + form_dropped
        } else {
            other_dropped
        };
        (f, more + unread)
    } else {
        (Vec::new(), 0)
    };
    let mut facts = if with_facts {
        Facts {
            amounts: amount_facts(screen, &anchor),
            recipients: if pays {
                recipient_facts(screen, &anchor)
            } else {
                Vec::new()
            },
            selected: selected_facts(screen, target),
            fields,
            fields_more,
            note: match (capped, target) {
                (false, _) => String::new(),
                (true, Some(_)) => format!(". Facts from the {MAX_FACT_ELEMENTS} nearest elements"),
                (true, None) => format!(". Facts from the first {MAX_FACT_ELEMENTS} elements"),
            },
        }
    } else {
        Facts {
            amounts: Vec::new(),
            recipients: Vec::new(),
            selected: Vec::new(),
            fields: Vec::new(),
            fields_more: 0,
            note: String::new(),
        }
    };
    // Thirteenth review (M1): a presumed submit the gate made `Payment` by what its form
    // holds (not a payment or purchase by its own label or the screen, which lists its
    // amounts already) and whose screen shows no amount lists the numbers it sends under
    // `Amounts on screen:` with their field names ("'How much?': 900"). The cost (a
    // classification without the presumption) is paid only there.
    if with_facts
        && presumed
        && category == RiskCategory::Payment
        && facts.amounts.is_empty()
        && !matches!(
            classify_from(action, raw, None, source),
            RiskCategory::Payment | RiskCategory::Purchase
        )
    {
        for f in &sent.fields {
            let Some(v) = f
                .value
                .as_deref()
                .filter(|v| !f.recipient && amount_shown(v))
            else {
                continue;
            };
            if facts.amounts.iter().any(|a| a.text.ends_with(v)) {
                continue;
            }
            facts.amounts.push(AmountFact {
                text: format!("{}: {v}", f.name),
                value: amount_value(v).unwrap_or(-1.0),
                charge: true,
                total: false,
                exact: false,
                near: Closeness {
                    off_row: true,
                    container: usize::MAX,
                    distance: f64::MAX,
                },
            });
        }
    }
    // Recipients (eleventh review: evidence with its provenance). `To/Account:` holds only
    // the form's fields named like a recipient that name someone, with their values (what
    // the action sends); `Screen says:` the recipient heading pairs read from the screen's
    // text (tenth review's tiers otherwise unchanged); an empty recipient field is shown
    // under `Sends:` only.
    let cap = crate::sanitize::MAX_SCREEN_TEXT_CHARS;
    let mut field_entries: Vec<Recipient> = Vec::new();
    // Thirteenth review: whether an entry is under a strong head (a field that can only name
    // the other side); weak-head entries ("Account", "IBAN") are shown, but do not stand
    // for a verified recipient.
    let mut strong_entry = false;
    let mut says: Vec<Recipient> = Vec::new();
    let mut field_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut says_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    if pays {
        for (i, f) in sent.fields.iter().enumerate().filter(|(_, f)| f.recipient) {
            // Fourteenth review (Rule G): a value that names no one (a date, a bare number) is
            // no recipient entry; it stays under `Sends:`.
            let Some(v) = f.value.as_deref().filter(|v| names_someone(v)) else {
                continue;
            };
            let key = recipient_key(v);
            if key.is_empty() {
                continue;
            }
            field_names.insert(key);
            strong_entry |= f.strong;
            field_entries.push(Recipient {
                head: format!("{}: \"", f.name),
                value: v.to_owned(),
                close: "\"".to_owned(),
                lookalike: crate::text::mixed_script(v),
                field: Some(i),
            });
        }
    }
    for t in &facts.recipients {
        let entry = clean(t, cap).replace(';', ",");
        let (head, value) = split_pair(&entry);
        if value.is_empty() || says.iter().any(|r| r.head == head && r.value == value) {
            continue;
        }
        let key = recipient_key(&value);
        if !key.is_empty() {
            says_names.insert(key);
        }
        says.push(Recipient {
            lookalike: crate::text::mixed_script(&value),
            head,
            value,
            close: String::new(),
            field: None,
        });
    }
    // A sending, posting, paying or purchasing action whose form names no recipient says
    // so (eleventh review, F3d). Twelfth review: only when the action is such by its own
    // label or role; a merely presumed submit ("Continue", "Done", Return after typing),
    // which the gate raises to `SendMessage` (`evaluate_with`), already says what it sends.
    // So a `SendMessage` question on a presumed submit is classified again without the
    // presumption (the cost is paid only there).
    let unverified = with_facts && pays && !strong_entry && {
        let own = if !presumed || category != RiskCategory::SendMessage {
            category
        } else {
            match browser_forward(action, raw, tree) {
                Some(index) => {
                    let mut nav = raw.clone();
                    if let Some(e) = nav.elements.iter_mut().find(|e| e.index == index) {
                        e.label = "Back".to_owned();
                    }
                    classify_from(action, &nav, None, source)
                }
                None => classify_from(action, raw, None, source),
            }
        };
        SENDING_CATEGORIES.contains(&own)
    };
    let (next_to, under) = match (action, target) {
        (Action::Tap { .. } | Action::LongPress { .. }, Some(t)) => press_context(t, screen, tree),
        _ => (String::new(), String::new()),
    };
    let other = !(next_to.is_empty()
        && under.is_empty()
        && facts.selected.is_empty()
        && facts.fields.is_empty()
        && facts.fields_more == 0
        && facts.note.is_empty());
    let present = [
        true,
        !facts.amounts.is_empty(),
        !field_entries.is_empty() || !says.is_empty() || unverified,
        !sent.fields.is_empty(),
        other,
    ];
    // The screen's title (eleventh review, F3c): on a typed tree only, room reserved for it
    // in the action slot.
    let title = if source == ScreenSource::Typed && with_facts {
        screen_title(screen, tree)
    } else {
        None
    };
    let title_part = title
        .as_deref()
        .map(|t| format!(", {MARK_TITLE}{t}\""))
        .unwrap_or_default();
    // The recipients' minimum (eleventh review): what their last level needs — one entry
    // at its minimal cut (an email keeps `@` and its host), the count of the rest and of
    // `Screen says:` — at least `SLOT_RECIPIENTS.0`, at most `MAX_RECIPIENTS_MIN`.
    let recipients_min = if present[2] {
        let (_, _, minimal) = recipients_slot(
            &field_entries,
            &says,
            (field_names.len(), says_names.len()),
            unverified,
            usize::MAX,
        );
        minimal.clamp(SLOT_RECIPIENTS.0, MAX_RECIPIENTS_MIN)
    } else {
        0
    };
    let bounds = [
        (
            SLOT_ACTION.0 + n_chars(&title_part),
            SLOT_ACTION.1 + n_chars(&title_part),
        ),
        SLOT_AMOUNTS,
        (recipients_min, SLOT_RECIPIENTS.1),
        SLOT_SENDS,
        SLOT_OTHER,
    ];
    let slots = Slots::new(present, bounds, max);
    let mut room = max;
    // (1) The action.
    let a1 = slots.allot(0, room);
    let (head, tail, typed_part) = action_slot(
        action,
        screen,
        category,
        return_only,
        type_return,
        &title_part,
        a1,
    );
    room = room.saturating_sub(n_chars(&head) + n_chars(&tail) + n_chars(&typed_part));
    // (2) The amounts.
    let a2 = slots.allot(1, room);
    let amounts = fit_slot(amounts_slot(&facts.amounts, a2), a2);
    room = room.saturating_sub(n_chars(&amounts));
    // (3) The recipients.
    let a3 = slots.allot(2, room);
    let (recipients, shown, _) = recipients_slot(
        &field_entries,
        &says,
        (field_names.len(), says_names.len()),
        unverified,
        a3,
    );
    let recipients = fit_rows(recipients, a3);
    room = room.saturating_sub(n_chars(&render_rows(&recipients)));
    // A value shown under `To/Account:` (whole or cut) is not repeated under `Sends:`
    // (eleventh review, F4).
    let mut above = vec![false; sent.fields.len()];
    for i in shown {
        if let Some(a) = above.get_mut(i) {
            *a = true;
        }
    }
    // (4) What is sent.
    let a4 = slots.allot(3, room);
    let sends = fit_slot(sends_slot(&sent, &above, a4), a4);
    room = room.saturating_sub(n_chars(&sends));
    // (5) The other facts.
    let a5 = slots.allot(4, room);
    let (inline, rest) = other_slot(&next_to, &under, &facts, a5);
    let (inline, rest) = if n_chars(&inline) + n_chars(&render_rows(&rest)) <= a5 {
        (inline, rest)
    } else {
        (String::new(), fit_rows(rest, a5))
    };
    // The reading order: the action line (with `next to`/`under`), what is sent, `Typed:`,
    // the amounts, the recipients, the other facts.
    let mut rows = action_rows(action, &format!("{head}{inline}{tail}"));
    rows.extend([
        row(QuestionRowKind::Sends, &sends),
        row(QuestionRowKind::Typed, &typed_part),
        row(QuestionRowKind::Amount, &amounts),
    ]);
    rows.extend(recipients);
    rows.extend(rest);
    finish_question(non_empty(rows), max)
}

/// The action line as its rows: one `Action` row, except that the line of an `open_url` to
/// an `sms:`/`mailto:` link the owner did not give ([`open_url_line`]) ends in what the link
/// sends (`. Sends: Body: "…"`), its own `Sends` row. The link's parts are quoted through
/// [`clean`], which neutralises the question's markers, so the first `. Sends: ` is the
/// line's own.
fn action_rows(action: &Action, line: &str) -> Vec<QuestionRow> {
    if let Action::OpenUrl { .. } = action {
        if let Some(k) = line.find(&format!(". {MARK_SENDS}: ")) {
            let (what, sends) = line.split_at(k);
            return vec![
                row(QuestionRowKind::Action, what),
                row(QuestionRowKind::Sends, sends),
            ];
        }
    }
    vec![row(QuestionRowKind::Action, line)]
}

/// The reserved slots of one summary (see `summarize_with`).
struct Slots {
    present: [bool; 5],
    mins: [usize; 5],
    maxs: [usize; 5],
}

impl Slots {
    /// Slots for a summary of `max` characters: an empty slot reserves nothing; when the
    /// minimums of the present slots exceed `max` (never for the 480 bound; behind the
    /// runner-text warning, 379, since the eleventh review: 92 + 90 + 122 + 76 + 48 = 428)
    /// the other facts' reserve goes first, then the action line's down to 60, then all
    /// are scaled down in proportion; the action alone may use the whole bound.
    fn new(present: [bool; 5], bounds: [(usize, usize); 5], max: usize) -> Slots {
        let mut mins = [0usize; 5];
        let mut maxs = [0usize; 5];
        for (i, (p, (lo, hi))) in present.iter().zip(bounds).enumerate() {
            if let (Some(m), Some(x)) = (mins.get_mut(i), maxs.get_mut(i)) {
                *m = if *p { lo } else { 0 };
                *x = hi;
            }
        }
        // Eleventh review: when the minimums exceed the bound (behind the runner-text
        // warning, 379), the other facts give up their reserve first, then the action line
        // (whose last resort is a cut head); only then are all scaled down in proportion.
        let mut sum: usize = mins.iter().sum();
        for i in [4usize, 0] {
            if sum > max {
                if let Some(m) = mins.get_mut(i) {
                    let floor = if i == 0 { 60 } else { 0 };
                    let give = (sum - max).min(m.saturating_sub(floor));
                    *m -= give;
                    sum -= give;
                }
            }
        }
        if sum > max && sum > 0 {
            for m in &mut mins {
                *m = m.saturating_mul(max) / sum;
            }
        }
        if present.iter().skip(1).all(|p| !p) {
            if let Some(m) = maxs.first_mut() {
                *m = max;
            }
        }
        Slots {
            present,
            mins,
            maxs,
        }
    }

    /// The allotment of slot `i` when `room` characters are left: up to its maximum, never
    /// into the minimums of the present slots after it.
    fn allot(&self, i: usize, room: usize) -> usize {
        if !self.present.get(i).copied().unwrap_or(false) {
            return 0;
        }
        let reserve: usize = self.mins.iter().skip(i + 1).sum();
        self.maxs
            .get(i)
            .copied()
            .unwrap_or(0)
            .min(room.saturating_sub(reserve))
    }
}

/// The final sanitising pass over a composed summary. The composition always fits the
/// bound, so this cut never removes a fact (checked in debug builds and by tests).
fn finish_summary(line: &str, max: usize) -> String {
    debug_assert!(
        line.chars().count() <= max,
        "a composed summary exceeds its bound ({} > {max})",
        line.chars().count()
    );
    sanitize_screen_text_with_limit(line, max)
}

/// The final pass ([`finish_summary`]) over the composed rows, keeping them the question's
/// exact parts: the summary is the joined rows sanitised; when that changes nothing (the
/// parts are cleaned when written, so it normally does not) the rows stand, else each row
/// sanitised on its own when they still join to the summary, else the summary as one row.
fn finish_question(rows: Vec<QuestionRow>, max: usize) -> Vec<QuestionRow> {
    let line = render_rows(&rows);
    let summary = finish_summary(&line, max);
    if summary == line {
        debug_rows_hold_their_lines(&rows);
        return rows;
    }
    let each = non_empty(rows.iter().map(|r| QuestionRow {
        kind: r.kind,
        text: sanitize_screen_text_with_limit(&r.text, max),
    }));
    if render_rows(&each) == summary {
        debug_rows_hold_their_lines(&each);
        return each;
    }
    vec![row(QuestionRowKind::Action, &summary)]
}

/// Debug builds (every test): a row that starts with a line's marker is of that line's
/// family, and a row of a fact family starts with one of its family's markers. (A marker
/// inside a row is not checked: a quoted screen value may hold marker-like text.)
fn debug_rows_hold_their_lines(rows: &[QuestionRow]) {
    if !cfg!(debug_assertions) {
        return;
    }
    const MARKERS: [(&str, QuestionRowKind); 15] = [
        (
            ". This text is not from your request",
            QuestionRowKind::Note,
        ),
        (". This answers an alert", QuestionRowKind::Note),
        (". This answers a system alert", QuestionRowKind::Note),
        (". Amounts on screen", QuestionRowKind::Amount),
        (". To/Account", QuestionRowKind::Recipient),
        (". Screen says", QuestionRowKind::ScreenSays),
        (". Recipient not verified", QuestionRowKind::NotVerified),
        (". Sends:", QuestionRowKind::Sends),
        (". The form holds:", QuestionRowKind::Sends),
        (
            ", which sends the field's whole content",
            QuestionRowKind::Sends,
        ),
        (", which submits the form", QuestionRowKind::Sends),
        (". Typed:", QuestionRowKind::Typed),
        (". Fields:", QuestionRowKind::Fields),
        (". Selected:", QuestionRowKind::Selected),
        (". Facts from", QuestionRowKind::Note),
    ];
    for r in rows {
        // A row cut at the slot's bound may end inside its own marker (`. Scr…`).
        let cut = r.text.strip_suffix('…').unwrap_or(&r.text);
        let starts: Vec<QuestionRowKind> = MARKERS
            .iter()
            .filter(|(m, _)| r.text.starts_with(m))
            .map(|(_, k)| *k)
            .collect();
        let cut_of_own = MARKERS
            .iter()
            .any(|(m, k)| *k == r.kind && m.starts_with(cut));
        let fact = !matches!(
            r.kind,
            QuestionRowKind::Action | QuestionRowKind::Warning | QuestionRowKind::Other
        );
        debug_assert!(
            starts.iter().all(|k| *k == r.kind) && (!fact || !starts.is_empty() || cut_of_own),
            "a {:?} row reads {:?}: {rows:?}",
            r.kind,
            r.text
        );
    }
}

/// What was typed, as a summary says it: quoted (at most `cap` characters), or "a secret
/// (N characters)".
fn typed_words(text: &str, category: RiskCategory, target: Option<&Element>, cap: usize) -> String {
    let secret = category == RiskCategory::Credential
        || contains_secret(text)
        || target.is_some_and(|t| t.secure);
    if secret {
        format!("a secret ({} characters)", text.chars().count())
    } else {
        format!("\"{}\"", clean(text, cap))
    }
}

/// Slot 1: the action line as (head, tail, typed) within `budget` characters, the target's
/// label, the app's name and Return's typed text quoted at 60, then 40/30/40, 30/24/30,
/// 24/20/24 and 16/12/16 characters. `next to`/`under` (slot 5) go between head and tail.
fn action_slot(
    action: &Action,
    screen: &CompactScreen,
    category: RiskCategory,
    return_only: bool,
    type_return: bool,
    title: &str,
    budget: usize,
) -> (String, String, String) {
    const LEVELS: [(usize, usize, usize); 5] = [
        (MAX_QUOTE_CHARS, MAX_QUOTE_CHARS, MAX_QUOTE_CHARS),
        (40, 30, 40),
        (30, 24, 30),
        (24, 20, 24),
        (16, 12, 16),
    ];
    let typed_part = |cap: usize| match action {
        Action::Type { element, text, .. } if return_only && !type_return && !text.is_empty() => {
            format!(
                ". Typed: {}",
                typed_words(text, category, type_target(*element, screen), cap)
            )
        }
        _ => String::new(),
    };
    let mut last = (String::new(), String::new(), String::new());
    for (label, app, typed) in LEVELS {
        let (h, mut t) = action_line(
            action,
            screen,
            category,
            label,
            app,
            return_only,
            type_return,
        );
        // The screen's title follows the app's name (eleventh review): `in Messages,
        // screen "Mallory"`.
        if !t.is_empty() {
            t.push_str(title);
        }
        let ty = typed_part(typed);
        if n_chars(&h) + n_chars(&t) + n_chars(&ty) <= budget {
            return (h, t, ty);
        }
        last = (h, t, ty);
    }
    let (h, t, ty) = last;
    let room = budget.saturating_sub(n_chars(&t) + n_chars(&ty));
    if room >= 16 {
        (hard_cut(&h, room), t, ty)
    } else {
        (
            hard_cut(&format!("{h}{t}{ty}"), budget),
            String::new(),
            String::new(),
        )
    }
}

/// Thirteenth review (M3): the action line of `open_url`. A link the gate asks about because
/// the owner did not give it (`SendMessage`) says so and shows where it goes: for a web or
/// app link its scheme, its full host (an over-long host as its first 12 and last 24
/// characters, `(look-alike letters)` when it holds a non-ASCII letter or a punycode
/// label) and its path and query quoted, at most `cap` characters, the cut stated; for an
/// `sms:`/`mailto:` link its recipient and what it sends. Any other `open_url` (a call
/// link, a link the owner gave) reads `Open <link>` as before.
fn open_url_line(url: &str, category: RiskCategory, cap: usize) -> String {
    if category != RiskCategory::SendMessage {
        return format!("Open {}", clean(url, 120));
    }
    let url = url.trim();
    let (scheme, rest) = url.split_once(':').unwrap_or(("", url));
    let scheme = scheme.to_ascii_lowercase();
    let quoted = quote_cut;
    if matches!(
        scheme.as_str(),
        "sms" | "smsto" | "mms" | "mailto" | "imessage"
    ) {
        let rest = rest.trim_start_matches('/');
        let (to, query) = match rest.find(['?', '&']) {
            Some(k) => (
                rest.get(..k).unwrap_or_default(),
                rest.get(k + 1..).unwrap_or_default(),
            ),
            None => (rest, ""),
        };
        let mut to_all: Vec<&str> = vec![to];
        let mut sends: Vec<String> = Vec::new();
        for pair in query.split('&') {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            match k.to_ascii_lowercase().as_str() {
                "to" | "cc" | "bcc" => to_all.push(v),
                _ if !v.is_empty() => {
                    sends.push(format!("{}: {}", display_name(k), quoted(v, cap)))
                }
                _ => {}
            }
        }
        let to: Vec<String> = to_all
            .into_iter()
            .filter(|t| !t.is_empty())
            .map(|t| quoted(t, cap.min(MAX_HOST_CHARS)))
            .collect();
        let to = if to.is_empty() {
            "no recipient".to_owned()
        } else {
            to.join(", ")
        };
        let sends = if sends.is_empty() {
            String::new()
        } else {
            format!(". {MARK_SENDS}: {}", sends.join("; "))
        };
        return format!(
            "Open a {} link that was not in your request, to {to}{sends}",
            clean(&scheme, 12)
        );
    }
    let after = rest.strip_prefix("//").unwrap_or(rest);
    format!(
        "Open a link that was not in your request: {}://{}",
        clean(&scheme, 24),
        link_place(after, cap)
    )
}

/// `text` quoted, at most `cap` characters of it, the cut stated.
fn quote_cut(text: &str, cap: usize) -> String {
    let n = n_chars(text);
    if n <= cap {
        format!("\"{}\"", clean(text, cap))
    } else {
        let head: String = text.chars().take(cap).collect();
        format!(
            "\"{}…\" ({} more character{} not shown)",
            clean(&head, cap),
            n - cap,
            plural(n - cap)
        )
    }
}

/// Where a web link goes, as the question shows it (`after`: the link after its scheme and
/// `//`): its authority exactly — the full host (over 40 characters: its first 12 and last
/// 24; `(look-alike letters)` for a non-ASCII letter or a punycode label) and, fourteenth
/// review (Rule I), its port — with a `user@` part left out but stated
/// ([`MARK_USER_INFO`]: `user@host` is a spoofing device), then the path and query quoted
/// to `cap` characters, the cut stated.
fn link_place(after: &str, cap: usize) -> String {
    let split = after.find(['/', '?', '#']).unwrap_or(after.len());
    let (authority, path) = after.split_at(split);
    // The host is what follows any `user@` and precedes any port.
    let user_info = authority.contains('@');
    let host_port = authority.rsplit('@').next().unwrap_or_default();
    let (host, port) = match host_port.rsplit_once(':') {
        Some((h, port))
            if !port.is_empty()
                && port.chars().all(|c| c.is_ascii_digit())
                && !h.ends_with(':') =>
        {
            (h, Some(port))
        }
        _ => (host_port, None),
    };
    let lookalike = host.chars().any(|c| !c.is_ascii() && c.is_alphabetic())
        || host
            .split('.')
            .any(|label| label.to_ascii_lowercase().starts_with("xn--"));
    let host_shown = if n_chars(host) > MAX_HOST_CHARS {
        let head: String = host.chars().take(HOST_HEAD_CHARS).collect();
        let tail: String = {
            let n = n_chars(host);
            host.chars().skip(n - HOST_TAIL_CHARS).collect()
        };
        format!(
            "{}…{}",
            clean(&head, HOST_HEAD_CHARS),
            clean(&tail, HOST_TAIL_CHARS)
        )
    } else if host.is_empty() {
        "no host".to_owned()
    } else {
        clean(host, MAX_HOST_CHARS)
    };
    let port = port
        .map(|p| format!(":{}", clean(p, 8)))
        .unwrap_or_default();
    let mut mark = String::new();
    if lookalike {
        mark.push_str(&format!(" {MARK_LOOKALIKE}"));
    }
    if user_info {
        mark.push_str(&format!(" {MARK_USER_INFO}"));
    }
    let path = if path.is_empty() || path == "/" {
        String::new()
    } else {
        format!(", path and query {}", quote_cut(path, cap))
    };
    format!("{host_shown}{port}{mark}{path}")
}

/// Fourteenth review (Rule A): how the question shows a link that Return submits in a
/// field — exactly as the `open_url` question shows it ([`link_place`]), its scheme when it
/// has one.
fn submitted_link_text(link: &str, cap: usize) -> String {
    let link = link.trim();
    let scheme = crate::validate::url_scheme(link);
    match link.split_once("://") {
        Some((s, after)) if scheme.is_some() => {
            format!("{}://{}", clean(s, 24), link_place(after, cap))
        }
        _ if scheme.is_some_and(|s| crate::shape::BARE_LINK_SCHEMES.contains(&s.as_str())) => {
            quote_cut(link, cap)
        }
        _ => link_place(link, cap),
    }
}
/// What `action` does, without the screen's facts, as (head, tail): the tail is ` in App`
/// for actions in the foreground app (`next to`/`under` go between them), empty otherwise.
/// The target's label is quoted at most `label` characters, the app's name `app`.
fn action_line(
    action: &Action,
    screen: &CompactScreen,
    category: RiskCategory,
    label: usize,
    app: usize,
    return_only: bool,
    type_return: bool,
) -> (String, String) {
    let app_name = screen
        .app_name
        .as_deref()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or(&screen.app_bundle_id);
    let in_app = format!(" in {}", clean(app_name, app));
    let whole = |s: String| (s, String::new());
    match action {
        Action::Tap { element } => match screen.get(*element) {
            Some(el) => press_summary("Tap", el, &in_app, label),
            None => (format!("Tap element {element}"), in_app),
        },
        Action::LongPress { element, .. } => match screen.get(*element) {
            Some(el) => press_summary("Long-press", el, &in_app, label),
            None => (format!("Long-press element {element}"), in_app),
        },
        Action::Type {
            element,
            text,
            submit,
        } => {
            let target = type_target(*element, screen);
            let what = typed_words(text, category, target, label);
            // Fourteenth review (Rule A): Return that submits a link opens it; the question
            // shows the link as the `open_url` question does.
            let link = if *submit && category != RiskCategory::Credential {
                submitted_link(target, text)
                    .map(|l| format!(", which opens the link {}", submitted_link_text(&l, label)))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let into = target.map_or_else(
                || "the focused field".to_owned(),
                |t| describe_with(t, label),
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
                if type_return {
                    // Fifteenth review (R1): the typing and its Return, one question.
                    return (
                        format!("Type {what} into {into} and press Return{key}{link}"),
                        in_app,
                    );
                }
                // What it sends (the field's whole content) and what was typed follow.
                return (format!("Press Return{key} in {into}{link}"), in_app);
            }
            let ret = if *submit { " and press Return" } else { "" };
            (format!("Type {what} into {into}{ret}{link}"), in_app)
        }
        Action::Clear { element } => (
            format!("Clear {}", describe_index_with(screen, *element, label)),
            in_app,
        ),
        Action::Scroll { direction, element } => match element {
            Some(i) => (
                format!(
                    "Scroll {direction:?} in {}",
                    describe_index_with(screen, *i, label)
                ),
                in_app,
            ),
            None => (format!("Scroll {direction:?}"), in_app),
        },
        Action::Swipe { direction, element } => match element {
            Some(i) => (
                format!(
                    "Swipe {direction:?} on {}",
                    describe_index_with(screen, *i, label)
                ),
                in_app,
            ),
            None => (format!("Swipe {direction:?}"), in_app),
        },
        Action::TapPoint { x, y } => {
            // The element that decided the risk: the riskiest pressable one under the
            // point, never a decorative text inside it.
            let on = crate::classify::tap_point(*x, *y, screen)
                .1
                .map(|e| format!(" on {}", describe_with(e, label)))
                .unwrap_or_default();
            (
                format!("Tap at ({:.0}%, {:.0}%){on}", x * 100.0, y * 100.0),
                in_app,
            )
        }
        Action::PressHome => whole("Go to the Home Screen".to_owned()),
        Action::OpenApp { app } => whole(format!("Open {}", clean(app, MAX_QUOTE_CHARS))),
        Action::OpenUrl { url } => whole(open_url_line(url, category, label)),
        Action::Wait { ms } => whole(format!("Wait {ms} ms")),
        Action::CallTool { name, args } => {
            let mut parts = Vec::new();
            if let Some(map) = args.as_object() {
                for (k, v) in map {
                    let v = match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    // The colon is ours: a key named like a heading ("Sends") must not
                    // make one (tenth review).
                    // Twelfth review: a heading-like key is quoted as well.
                    let key = quote_heading_like(clean(k, 30));
                    parts.push(defang(&format!("{key}: {}", clean(&v, 40))));
                }
            }
            if parts.is_empty() {
                whole(format!("Run {}", clean(name, MAX_QUOTE_CHARS)))
            } else {
                whole(format!(
                    "Run {} ({})",
                    clean(name, MAX_QUOTE_CHARS),
                    parts.join(", ")
                ))
            }
        }
        Action::AskUser { question } => whole(format!("Ask: {}", clean(question, 120))),
        Action::Look => ("Take a screenshot".to_owned(), in_app),
        Action::Done { summary } => whole(format!("Finish: {}", clean(summary, 120))),
        Action::Fail { reason } => whole(format!("Give up: {}", clean(reason, 120))),
    }
}

#[cfg(test)]
mod question_rows;
