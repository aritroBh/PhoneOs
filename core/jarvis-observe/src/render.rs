//! Prompt text for a compact screen, degraded gracefully to fit a token budget.

use jarvis_proto::{CompactScreen, Element, Role};

/// Label caps (in characters) tried in order when the full text is over budget.
const LABEL_CAPS: [usize; 4] = [48, 32, 20, 12];

/// Estimated tokens of `text`: ceil(UTF-8 bytes / 4).
pub fn estimate_tokens(text: &str) -> u32 {
    let n = text.len().div_ceil(4);
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The role as the rendered screen names it (`Button`, `Cell`, ...).
pub fn role_name(role: &Role) -> &str {
    match role {
        Role::Button => "Button",
        Role::Link => "Link",
        Role::TextField => "TextField",
        Role::SecureTextField => "SecureTextField",
        Role::TextView => "TextView",
        Role::SearchField => "SearchField",
        Role::StaticText => "StaticText",
        Role::Image => "Image",
        Role::Cell => "Cell",
        Role::Switch => "Switch",
        Role::Slider => "Slider",
        Role::Picker => "Picker",
        Role::Tab => "Tab",
        Role::NavigationBar => "NavigationBar",
        Role::TabBar => "TabBar",
        Role::Toolbar => "Toolbar",
        Role::Alert => "Alert",
        Role::Sheet => "Sheet",
        Role::Keyboard => "Keyboard",
        Role::Key => "Key",
        Role::ScrollView => "ScrollView",
        Role::Table => "Table",
        Role::CollectionView => "CollectionView",
        Role::WebView => "WebView",
        Role::Window => "Window",
        Role::Application => "Application",
        Role::Other(_) => "Other",
    }
}

/// Escapes `"`, `\` and control characters so a label stays on one line inside quotes.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// Cuts `s` to at most `cap` characters, ending in `…` when cut.
fn truncate(s: &str, cap: Option<usize>) -> String {
    match cap {
        Some(cap) if s.chars().count() > cap => {
            let mut t: String = s.chars().take(cap.saturating_sub(1)).collect();
            t.push('…');
            t
        }
        _ => s.to_owned(),
    }
}

/// `on`/`off` for a switch whose value is one of the forms iOS and SwiftUI report
/// (`1`/`0`, `true`/`false`, `on`/`off`); `None` for anything else, which is shown raw.
/// Only the rendering changes: `Element::value` keeps the raw value for verification.
pub fn switch_state(e: &Element) -> Option<&'static str> {
    if e.role != Role::Switch {
        return None;
    }
    match e
        .value
        .as_deref()
        .map(str::trim)?
        .to_ascii_lowercase()
        .as_str()
    {
        "1" | "true" | "on" => Some("on"),
        "0" | "false" | "off" => Some("off"),
        _ => None,
    }
}

fn element_line(e: &Element, cap: Option<usize>) -> String {
    let mut line = format!("[{}] {}", e.index, role_name(&e.role));
    if !e.label.is_empty() {
        line.push_str(&format!(" \"{}\"", escape(&truncate(&e.label, cap))));
    }
    let switch = switch_state(e);
    match (&e.value, &e.placeholder) {
        _ if switch.is_some() => {}
        (Some(v), _) => line.push_str(&format!(" value=\"{}\"", escape(&truncate(v, cap)))),
        (None, Some(p)) if e.role.is_text_input() && *p != e.label => {
            line.push_str(&format!(" placeholder=\"{}\"", escape(&truncate(p, cap))));
        }
        _ => {}
    }
    let mut flags: Vec<&str> = Vec::new();
    if let Some(state) = switch {
        flags.push(state);
    }
    if e.focused {
        flags.push("focused");
    }
    if e.selected {
        flags.push("selected");
    }
    if !e.enabled {
        flags.push("disabled");
    }
    if !flags.is_empty() {
        line.push_str(&format!(" ({})", flags.join(", ")));
    }
    line
}

fn header(screen: &CompactScreen) -> String {
    let mut out = match &screen.app_name {
        Some(name) if !name.is_empty() => format!("App: {name} ({})", screen.app_bundle_id),
        _ => format!("App: {}", screen.app_bundle_id),
    };
    if screen.keyboard_visible {
        out.push_str("\nKeyboard: visible");
        // The Return key's label only when it says more than "return" (a "send" key sends).
        if let Some(key) = screen
            .cues
            .return_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty() && !k.eq_ignore_ascii_case("return"))
        {
            out.push_str(&format!(" (Return key: \"{}\")", escape(key)));
        }
    }
    if let Some((page, count)) = screen.cues.page {
        out.push_str(&format!("\nPage {page} of {count}"));
        out.push_str(match (page > 1, page < count) {
            (_, true) => " (swipe left for the next page)",
            (true, false) => " (swipe right for the previous page)",
            (false, false) => "",
        });
    }
    if screen.cues.more_above {
        out.push_str(&format!("\n{MORE_ABOVE}"));
    }
    out
}

/// Cue line when a list continues above the visible part.
pub const MORE_ABOVE: &str = "… more above; scroll up";
/// Cue line when a list continues below the visible part.
pub const MORE_BELOW: &str = "… more below; scroll down";

/// How much of the screen to show.
struct Plan<'a> {
    /// Label/value character cap.
    cap: Option<usize>,
    /// Static texts to omit, in omission order (positions in `elements`).
    omitted_texts: &'a [usize],
    /// Number of bottom elements to cut after omitting texts.
    cut: usize,
}

fn build(screen: &CompactScreen, plan: &Plan<'_>) -> String {
    let mut omitted = vec![false; screen.elements.len()];
    for &i in plan.omitted_texts {
        if let Some(slot) = omitted.get_mut(i) {
            *slot = true;
        }
    }
    let shown: Vec<&Element> = screen
        .elements
        .iter()
        .zip(&omitted)
        .filter(|(_, o)| !**o)
        .map(|(e, _)| e)
        .collect();
    let keep = shown.len().saturating_sub(plan.cut);
    let mut out = header(screen);
    for e in shown.iter().take(keep) {
        out.push('\n');
        out.push_str(&element_line(e, plan.cap));
    }
    if !plan.omitted_texts.is_empty() {
        out.push_str(&format!(
            "\n… {} static texts omitted",
            plan.omitted_texts.len()
        ));
    }
    let cut = shown.len() - keep;
    if cut > 0 {
        out.push_str(&format!("\n… {cut} more elements below; scroll down"));
    } else if screen.cues.more_below {
        out.push('\n');
        out.push_str(MORE_BELOW);
    }
    out
}

/// Smallest `n` in `0..=max` with `fits(n)`, assuming `fits` is monotone; `None` if even `max` fails.
fn smallest_fitting(max: usize, fits: impl Fn(usize) -> bool) -> Option<usize> {
    if !fits(max) {
        return None;
    }
    let (mut lo, mut hi) = (0, max);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if fits(mid) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    Some(lo)
}

/// Positions of static texts, farthest from any interactive element first (ties: lowest on screen first).
fn static_text_omission_order(screen: &CompactScreen) -> Vec<usize> {
    let mut anchors: Vec<f64> = screen
        .elements
        .iter()
        .filter(|e| e.role.is_interactive())
        .map(|e| e.frame.center().y)
        .collect();
    anchors.sort_by(f64::total_cmp);
    let distance = |y: f64| -> f64 {
        let at = anchors.partition_point(|a| *a < y);
        let below = anchors.get(at).map(|a| (a - y).abs());
        let above = at
            .checked_sub(1)
            .and_then(|i| anchors.get(i))
            .map(|a| (y - a).abs());
        match (above, below) {
            (Some(a), Some(b)) => a.min(b),
            (Some(d), None) | (None, Some(d)) => d,
            (None, None) => f64::INFINITY,
        }
    };
    let mut texts: Vec<(usize, f64)> = screen
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.role == Role::StaticText)
        .map(|(i, e)| (i, distance(e.frame.center().y)))
        .collect();
    texts.sort_by(|a, b| b.1.total_cmp(&a.1).then(b.0.cmp(&a.0)));
    texts.into_iter().map(|(i, _)| i).collect()
}

/// Renders the screen as prompt text within `budget_tokens` (see [`estimate_tokens`]).
///
/// First line `App: Name (bundle.id)`, then `Keyboard: visible` when up (with
/// `(Return key: "send")` when the Return key reads anything but "return"), then one line per
/// element, e.g. `[4] TextField "Email" value="a@b.c" (focused)`. Over budget, it degrades in
/// order: truncate long labels and values; omit static texts far from interactive elements
/// (`… N static texts omitted`); drop elements from the bottom
/// (`… N more elements below; scroll down`). Element indices are never
/// renumbered. If even the header and footer exceed the budget, those alone are returned.
///
/// Switches show `(on)`/`(off)` instead of their raw `0`/`1` value ([`switch_state`]). The
/// screen's [`jarvis_proto::ScreenCues`] add one line each: `Page 1 of 2 (swipe left for
/// the next page)` and `… more above; scroll up` after the header, `… more below; scroll
/// down` at the end (the budget-cut line says the same when elements were dropped).
pub fn render(screen: &CompactScreen, budget_tokens: u32) -> String {
    let fits = |s: &str| estimate_tokens(s) <= budget_tokens;
    let none: [usize; 0] = [];

    let full = build(
        screen,
        &Plan {
            cap: None,
            omitted_texts: &none,
            cut: 0,
        },
    );
    if fits(&full) {
        return full;
    }
    for cap in LABEL_CAPS {
        let s = build(
            screen,
            &Plan {
                cap: Some(cap),
                omitted_texts: &none,
                cut: 0,
            },
        );
        if fits(&s) {
            return s;
        }
    }

    let cap = LABEL_CAPS.last().copied();
    let order = static_text_omission_order(screen);
    let with_omitted = |k: usize, cut: usize| {
        build(
            screen,
            &Plan {
                cap,
                omitted_texts: order.get(..k).unwrap_or(&order),
                cut,
            },
        )
    };
    if let Some(k) = smallest_fitting(order.len(), |k| fits(&with_omitted(k, 0))) {
        return with_omitted(k, 0);
    }
    let remaining = screen.elements.len() - order.len();
    let cut =
        smallest_fitting(remaining, |c| fits(&with_omitted(order.len(), c))).unwrap_or(remaining);
    with_omitted(order.len(), cut)
}
