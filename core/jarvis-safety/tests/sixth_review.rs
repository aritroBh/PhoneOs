#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Sixth review: the source-level rule for runner-text screens, the summary field rule and
//! where taps land (`jarvis_safety::tap_spot`). The review's probes are kept here as named
//! regression tests.

mod common;

use common::{app_screen, button, el, label};
use jarvis_proto::{
    Action, AppRule, BarNode, CompactScreen, Element, Point, Policy, RawNode, Rect, RiskCategory,
    Role, TrustLevel,
};
use jarvis_safety::{
    control_at, evaluate_from, summarize, summary_for, summary_with, tap_spot, Bar, Covered,
    CoveredBy, Decision, ScreenSource, ScreenTree, TypedText,
};

fn tf(index: u32, name: &str, value: &str) -> Element {
    let mut e = el(index, Role::TextField, name);
    e.value = Some(value.to_owned());
    e
}

fn tap(i: u32) -> Action {
    Action::Tap { element: i }
}

fn at(mut e: Element, x: f64, y: f64, w: f64, h: f64) -> Element {
    e.frame = Rect::new(x, y, w, h);
    e
}

fn trusted() -> Policy {
    let mut policy = Policy::default();
    policy
        .app_rules
        .push(AppRule::new("com.example.app", TrustLevel::Trusted));
    policy
}

// --- 4. Runner-text screens always confirm -----------------------------------------------

/// Probe: a runner-text screen in an app the owner marked Trusted. `Unverified` is in
/// `TRUSTED_ALLOWED`, so riding on the category sent the tap unasked.
#[test]
fn runner_text_in_a_trusted_app_asks() {
    let s = app_screen(vec![label(1, "Profile"), button(2, "Next")]);
    let d = evaluate_from(&trusted(), &tap(2), &s, None, ScreenSource::RunnerText);
    assert!(matches!(d, Decision::Confirm(_)), "{d:?}");
    // A category Trusted allows (a setting) asks too, under its own category.
    let s = app_screen(vec![label(1, "Settings"), button(2, "Dark mode")]);
    let d = evaluate_from(&trusted(), &tap(2), &s, None, ScreenSource::RunnerText);
    assert!(matches!(d, Decision::Confirm(_)), "{d:?}");
}

/// The same screen read as a typed tree in the same Trusted app does not ask.
#[test]
fn a_typed_screen_in_the_trusted_app_does_not_ask() {
    let s = app_screen(vec![label(1, "Profile"), button(2, "Next")]);
    let d = evaluate_from(&trusted(), &tap(2), &s, None, ScreenSource::Typed);
    assert_eq!(d, Decision::Allow);
}

/// The owner relaxed every category (nothing in `confirm`): a runner-text screen still asks;
/// a denied category still refuses first; waits and plain scrolls change nothing and pass.
#[test]
fn runner_text_asks_with_relaxed_categories_and_denials_come_first() {
    let mut relaxed = Policy::default();
    relaxed.confirm.clear();
    let s = app_screen(vec![
        label(1, "Profile"),
        button(2, "Next"),
        button(3, "Pay"),
    ]);
    let d = evaluate_from(&relaxed, &tap(2), &s, None, ScreenSource::RunnerText);
    assert!(matches!(d, Decision::Confirm(_)), "{d:?}");
    assert_eq!(
        evaluate_from(&relaxed, &tap(2), &s, None, ScreenSource::Typed),
        Decision::Allow
    );
    let mut deny = relaxed.clone();
    deny.deny_categories.push(RiskCategory::Payment);
    let d = evaluate_from(&deny, &tap(3), &s, None, ScreenSource::RunnerText);
    assert!(matches!(d, Decision::Deny(_)), "{d:?}");
    let wait = Action::Wait { ms: 500 };
    assert_eq!(
        evaluate_from(&relaxed, &wait, &s, None, ScreenSource::RunnerText),
        Decision::Allow
    );
}

// --- 5. Summaries name fields; values only when safe -------------------------------------

/// Probe: field values the old name lists did not catch were quoted in summaries.
#[test]
fn field_values_are_not_quoted_in_summaries() {
    let fields = [
        ("CVV2", "737"),
        ("Security number", "4417"),
        ("Card no.", "4111 1111 1111 1111"),
        ("Card #", "5500 0000 0000 0004"),
        ("Answer", "Fluffy"),
        ("Mother's maiden name", "Smithers"),
        ("Date of birth", "04/07/1981"),
        ("Tax ID", "12-3456789"),
        ("Driver license", "D1234567"),
        ("Passport number", "X12345678"),
        ("Field 1", "4111111111111111"),
        ("Field 2", "482913"),
        ("Memorable word", "zebra42"),
        ("Access code", "73519"),
        ("CSC", "123"),
    ];
    let mut leaks = Vec::new();
    // Seventh review: "Pay now" sends the field, and a sent field's whole value is shown
    // after the pattern redaction (cards, labelled codes) and the credential-name rule. The
    // one value those let through is a bare 6-digit number in a neutrally named field
    // (deliberate; see `summary_with`). Eighth review: every asked tap whose form holds text
    // shows what the form holds ("Remove card" too; was: a tap that sends nothing quoted
    // none), so the same one value is let through for both targets; deliberate change.
    let sent_ok = ["482913"];
    for target in ["Pay now", "Remove card"] {
        for (name, value) in fields {
            let s = app_screen(vec![
                tf(1, name, value),
                label(2, "Fee $25.00"),
                button(3, target),
            ]);
            let sum = summarize(&tap(3), &s, None);
            if sum.contains(value) && !sent_ok.contains(&value) {
                leaks.push(format!("{target}: {name}={value}: {sum}"));
            }
            // Ninth review: a sent field's name is shown with its quotes neutralised (`'` →
            // `’`) and, holding more than letters, digits and spaces, in single quotes.
            let shown = name.trim_end_matches('.').replace('\'', "\u{2019}");
            assert!(sum.contains(&shown), "{sum}");
        }
    }
    // A static text (not a field) with a code / full card number.
    let s = app_screen(vec![
        label(1, "Your code is 482913"),
        label(2, "Card 4111 1111 1111 1111 Total $25.00"),
        button(3, "Pay now"),
    ]);
    let sum = summarize(&tap(3), &s, None);
    for v in ["482913", "4111 1111 1111 1111"] {
        if sum.contains(v) {
            leaks.push(format!("static {v}: {sum}"));
        }
    }
    assert!(leaks.is_empty(), "{}", leaks.join("\n"));
}

/// The new names are masked in prompts as well.
#[test]
fn the_new_names_are_credential_like_fields() {
    for name in [
        "CSC",
        "CVV2",
        "Security number",
        "Memorable word",
        "Answer",
        "Mother's maiden name",
        "Date of birth",
        "Tax ID",
        "Driver license",
        "Passport number",
        "Access code",
    ] {
        assert!(
            jarvis_safety::credential_like_field(&tf(1, name, "x")),
            "{name}"
        );
    }
    assert!(!jarvis_safety::credential_like_field(&tf(1, "Note", "x")));
}

/// A message Jarvis typed is shown before Send; one it did not type is "(filled)"; an empty
/// field is "(empty)"; an amount field shows its amount; a note with an amount in it does
/// not.
#[test]
fn typed_text_amounts_and_other_fields() {
    let s = app_screen(vec![
        at(tf(1, "Message", "On my way"), 16.0, 200.0, 361.0, 44.0),
        at(el(2, Role::TextField, "Subject"), 16.0, 260.0, 361.0, 44.0),
        at(button(3, "Send"), 16.0, 700.0, 361.0, 50.0),
    ]);
    let send = |typed: &[TypedText]| {
        summary_with(
            &tap(3),
            &s,
            RiskCategory::SendMessage,
            ScreenSource::Typed,
            None,
            typed,
        )
    };
    let typed = [TypedText {
        field: 1,
        text: "On my way".to_owned(),
    }];
    // Seventh review: Send sends the form, so the message shows whole whoever typed it
    // (was "Message (filled)" unless Jarvis typed it; deliberate change).
    assert_eq!(
        send(&typed),
        // Eleventh review (F3d): a paying or sending action whose form names no recipient says `Recipient not verified`.
        "Tap \"Send\" in Example. Sends: Message: \"On my way\"; Subject (empty). Recipient not verified"
    );
    assert_eq!(send(&[]), send(&typed));
    let amounts = app_screen(vec![
        at(tf(1, "Amount", "$4,000.00"), 16.0, 200.0, 361.0, 44.0),
        at(tf(2, "Tip", "15"), 16.0, 260.0, 361.0, 44.0),
        at(tf(3, "Note", "rent $40"), 16.0, 320.0, 361.0, 44.0),
        at(tf(4, "Quantity", "two"), 16.0, 380.0, 361.0, 44.0),
        at(button(5, "Pay"), 16.0, 700.0, 361.0, 50.0),
    ]);
    // An action that sends nothing (seventh review: Pay would send the form and show it
    // all). Eighth review: every asked tap whose form holds text shows the form ("Remove
    // card" now says `Sends: …`; deliberate change), so the field rule is checked on a
    // swipe of the same button, which sends nothing.
    let mut amounts = amounts;
    amounts.elements[4].label = "Remove card".to_owned();
    let sum = summarize(&tap(5), &amounts, None);
    // Ninth review: a "Remove card" tap does not send, so the form is "The form holds:"
    // (was "Sends:"), and the amount-named fields come before the rest (was screen order);
    // deliberate changes.
    assert!(
        sum.contains(
            "The form holds: Amount: \"$4,000.00\"; Tip: \"15\"; Quantity: \"two\"; Note: \"rent $40\""
        ),
        "{sum}"
    );
    let swipe = Action::Swipe {
        direction: jarvis_proto::Direction::Left,
        element: Some(5),
    };
    let sum = summary_for(
        &swipe,
        &amounts,
        RiskCategory::Delete,
        ScreenSource::Typed,
        None,
    );
    assert!(sum.contains("Amount: $4,000.00"), "{sum}");
    assert!(sum.contains("Tip: 15"), "{sum}");
    assert!(sum.contains("Note (filled)"), "{sum}");
    assert!(sum.contains("Quantity (filled)"), "{sum}");
    assert!(!sum.contains("rent"), "{sum}");
}

/// Recipient and selected facts never quote a field's value either.
#[test]
fn recipient_facts_never_quote_a_field_value() {
    let s = app_screen(vec![
        at(tf(1, "To", "mallory@example.com"), 16.0, 200.0, 361.0, 44.0),
        at(label(2, "Total $5.00"), 16.0, 260.0, 361.0, 22.0),
        at(button(3, "Pay"), 16.0, 700.0, 361.0, 50.0),
    ]);
    // Seventh review: Pay sends the field, so its whole value is shown under "Sends:" (was
    // "To (filled)"; deliberate change). Ninth review: a sent field *named* like a
    // recipient is listed under "To/Account:" with the value it sends, ahead of the other
    // sent fields, and "Sends:" points there (was `Sends: To: "mallory@example.com"` and
    // no "To/Account"; deliberate change). Facts read from other elements still never quote
    // a field's value.
    let sum = summarize(&tap(3), &s, None);
    assert!(sum.contains("Sends: To (shown above)"), "{sum}");
    assert!(
        sum.contains("To/Account: To: \"mallory@example.com\""),
        "{sum}"
    );
    // An action that sends nothing still names the field only. Eighth review: every asked
    // tap whose form holds text shows the form (a "Remove card" tap now says `Sends: To:
    // "mallory@example.com"`; deliberate change), so this is checked on a swipe.
    let s = app_screen(vec![
        at(tf(1, "To", "mallory@example.com"), 16.0, 200.0, 361.0, 44.0),
        at(label(2, "Total $5.00"), 16.0, 260.0, 361.0, 22.0),
        at(button(3, "Remove card"), 16.0, 700.0, 361.0, 50.0),
    ]);
    let sum = summary_for(&tap(3), &s, RiskCategory::Delete, ScreenSource::Typed, None);
    // Ninth review: an action that does not send says "The form holds:" (was "Sends:";
    // deliberate change).
    assert!(
        sum.contains("The form holds: To: \"mallory@example.com\""),
        "{sum}"
    );
    assert!(!sum.contains("To/Account"), "{sum}");
    let swipe = Action::Swipe {
        direction: jarvis_proto::Direction::Left,
        element: Some(3),
    };
    let sum = summary_for(&swipe, &s, RiskCategory::Payment, ScreenSource::Typed, None);
    assert!(!sum.contains("mallory"), "{sum}");
    assert!(sum.contains("To (filled)"), "{sum}");
}

// --- 1. Where taps land -------------------------------------------------------------------

fn screen_of(elements: Vec<Element>) -> CompactScreen {
    app_screen(elements)
}

/// Two buttons overlapping completely: every candidate is contested, whichever is first in
/// document order.
#[test]
fn a_fully_covered_button_has_no_point() {
    let pay = at(button(1, "Pay $40.00"), 16.0, 300.0, 361.0, 50.0);
    let mallory = at(button(2, "Pay $4,000.00"), 16.0, 302.0, 361.0, 50.0);
    let s = screen_of(vec![pay.clone(), mallory.clone()]);
    assert_eq!(
        tap_spot(&pay, &s, None, None),
        Err(Covered {
            target: 1,
            by: CoveredBy::Control(2)
        })
    );
    assert_eq!(
        tap_spot(&mallory, &s, None, None),
        Err(Covered {
            target: 2,
            by: CoveredBy::Control(1)
        })
    );
}

/// A partial overlap over the centre: another point inside the target is used, and it is
/// uncontested. Deterministic.
#[test]
fn a_partial_overlap_moves_the_point_inside_the_target() {
    let pay = at(button(1, "Pay"), 16.0, 300.0, 361.0, 50.0);
    let badge = at(button(2, "Info"), 150.0, 290.0, 100.0, 40.0);
    let s = screen_of(vec![pay.clone(), badge.clone()]);
    let p = tap_spot(&pay, &s, None, None).unwrap();
    assert!(pay.frame.contains(p) && !badge.frame.contains(p), "{p:?}");
    assert_eq!(tap_spot(&pay, &s, None, None).unwrap(), p);
    // Static texts, images and containers never cover.
    let mut overlay = at(
        el(3, Role::Other("Other".into()), "Overlay"),
        0.0,
        0.0,
        393.0,
        852.0,
    );
    overlay.enabled = true;
    let text = at(label(4, "Pay now"), 16.0, 300.0, 361.0, 50.0);
    let s = screen_of(vec![overlay, text, pay.clone()]);
    assert_eq!(tap_spot(&pay, &s, None, None), Ok(pay.frame.center()));
    // Seventh review: a disabled control takes the touch too (a web button marked
    // `aria-disabled` still runs its handler). Was: it did not; deliberate change.
    let mut off = at(button(5, "Other"), 16.0, 300.0, 361.0, 50.0);
    off.enabled = false;
    let s = screen_of(vec![pay.clone(), off]);
    assert_eq!(
        tap_spot(&pay, &s, None, None),
        Err(Covered {
            target: 1,
            by: CoveredBy::Control(5)
        })
    );
}

/// A switch inside a full-width cell (related in the raw tree): the switch's own point when
/// the switch is the target, the cell's when the cell is. The same frames without the
/// ancestry are a conflict for the switch (its every point is inside the cell).
#[test]
fn a_switch_inside_a_cell_and_the_cell() {
    let cell = at(el(1, Role::Cell, "Wi-Fi"), 0.0, 200.0, 393.0, 52.0);
    let switch = at(el(2, Role::Switch, "Wi-Fi"), 320.0, 210.0, 51.0, 31.0);
    let s = screen_of(vec![cell.clone(), switch.clone()]);
    // Raw nodes: 0 root, 1 cell, 2 switch (inside the cell).
    let sources = [1, 2];
    let parents = [u32::MAX, 0, 1];
    let ends = [3, 3, 3];
    let tree = ScreenTree {
        sources: &sources,
        parents: &parents,
        ends: &ends,
        bars: &[],
        nodes: &[],
        inputs: &[],
    };
    let p = tap_spot(&switch, &s, Some(&tree), None).unwrap();
    assert!(switch.frame.contains(p));
    let c = tap_spot(&cell, &s, Some(&tree), None).unwrap();
    assert_eq!(c, cell.frame.center());
    assert!(!switch.frame.contains(c));
    assert!(tap_spot(&switch, &s, None, None).is_err());
}

/// A raw point resolves to the smallest control containing it; two unrelated controls
/// containing it are ambiguous.
#[test]
fn a_raw_point_resolves_to_one_control_or_is_ambiguous() {
    let pay = at(button(1, "Pay"), 16.0, 300.0, 361.0, 50.0);
    let other = at(button(2, "Other"), 16.0, 320.0, 361.0, 50.0);
    let s = screen_of(vec![pay.clone(), other]);
    let only_pay = Point::new(100.0, 305.0);
    assert_eq!(control_at(only_pay, &s, None).unwrap().unwrap().index, 1);
    assert!(control_at(Point::new(100.0, 330.0), &s, None).is_err());
    assert!(control_at(Point::new(100.0, 600.0), &s, None)
        .unwrap()
        .is_none());
}

// --- 1b. Bar precedence and the 5 × 5 grid ------------------------------------------------
//
// One raw tree for the bar tests (393 pt wide screen):
//   0 root; 1 navigation bar { 2 back }; 3 tab bar { 4 Home, 5 Search }; 6, 7, 8 rows.
// Compact elements: [1] back, [2] Home, [3] Search, [4] [5] [6] rows (or other content).

const BAR_SOURCES: [u32; 6] = [2, 4, 5, 6, 7, 8];
const BAR_PARENTS: [u32; 9] = [u32::MAX, 0, 1, 0, 3, 3, 0, 0, 0];
const BAR_ENDS: [u32; 9] = [9, 3, 3, 6, 5, 6, 7, 8, 9];

fn bar_tree(bars: &[BarNode]) -> ScreenTree<'_> {
    ScreenTree {
        sources: &BAR_SOURCES,
        parents: &BAR_PARENTS,
        ends: &BAR_ENDS,
        bars,
        nodes: &[],
        inputs: &[],
    }
}

// Ninth review: a bar's precedence needs evidence that what lies under it is page content
// scrolled under the bar (a page-sized scrollable ancestor; for a bottom bar, before the bar
// in the tree), which only the raw nodes give. This tree has it:
//   0 root; 1 window { 2 navigation bar { 3 back }; 4 scroll view { 5, 6, 7 rows }; 8 tab
//   bar { 9 Home, 10 Search } }.
// Compact elements as above: [1] back, [2] Home, [3] Search, [4] [5] [6] rows.
const SCROLLED_SOURCES: [u32; 6] = [3, 9, 10, 5, 6, 7];
const SCROLLED_PARENTS: [u32; 11] = [u32::MAX, 0, 1, 2, 1, 4, 4, 4, 1, 8, 8];
const SCROLLED_ENDS: [u32; 11] = [11, 11, 4, 4, 8, 6, 7, 8, 11, 10, 11];

/// The raw nodes of the scrolled tree for screen `s` (its elements by index) and `bars`.
fn scrolled_nodes(s: &CompactScreen, bars: &[BarNode]) -> Vec<RawNode> {
    let full = Rect::new(0.0, 0.0, s.size.width, s.size.height);
    let raw = |node: u32, role: Role, frame: Rect, label: Option<String>| RawNode {
        node,
        role,
        frame,
        text: label.is_some(),
        label,
        keyboard: false,
    };
    let mut v = vec![
        raw(0, Role::Application, full, None),
        raw(1, Role::Window, full, None),
        raw(4, Role::ScrollView, full, None),
    ];
    for b in bars {
        v.push(raw(b.node, b.role.clone(), b.frame, None));
    }
    for e in &s.elements {
        let node = SCROLLED_SOURCES[(e.index - 1) as usize];
        v.push(raw(node, e.role.clone(), e.frame, Some(e.label.clone())));
    }
    v.sort_by_key(|n| n.node);
    v.dedup_by_key(|n| n.node);
    v
}

fn scrolled_tree<'a>(bars: &'a [BarNode], nodes: &'a [RawNode]) -> ScreenTree<'a> {
    ScreenTree {
        sources: &SCROLLED_SOURCES,
        parents: &SCROLLED_PARENTS,
        ends: &SCROLLED_ENDS,
        bars,
        nodes,
        inputs: &[],
    }
}

/// The tab bar of the scrolled tree (node 8).
fn scrolled_tab_bar() -> BarNode {
    BarNode {
        node: 8,
        ..tab_bar()
    }
}

/// The navigation bar of the scrolled tree (node 2).
fn scrolled_nav_bar(role: Role, width: f64) -> BarNode {
    BarNode {
        node: 2,
        ..nav_bar(role, width)
    }
}

fn nav_bar(role: Role, width: f64) -> BarNode {
    BarNode {
        node: 1,
        role,
        frame: Rect::new(0.0, 59.0, width, 44.0),
    }
}

fn tab_bar() -> BarNode {
    BarNode {
        node: 3,
        role: Role::TabBar,
        frame: Rect::new(0.0, 770.0, 393.0, 82.0),
    }
}

fn back() -> Element {
    at(button(1, "Back"), 16.0, 59.0, 44.0, 44.0)
}

fn home() -> Element {
    at(button(2, "Home"), 20.0, 775.0, 100.0, 54.0)
}

fn search() -> Element {
    at(button(3, "Search"), 130.0, 775.0, 100.0, 54.0)
}

/// Rows scrolled under both bars: the back button and the tab-bar buttons are tapped at
/// their centres (the bars are drawn on top); the rows are tapped in their part outside
/// the bars. Without the bars (the same frames) the bar controls are contested, as before.
/// Ninth review: the rows are shown to be page content by the raw tree (a page-sized
/// scroll view before the tab bar); without that evidence they contest the bar controls
/// (checked at the end; was: precedence over anything outside the bar).
#[test]
fn bar_controls_win_over_rows_scrolled_under_them() {
    let row_top = at(button(4, "Item 1"), 0.0, 40.0, 393.0, 80.0);
    let row_bottom = at(button(5, "Item 9"), 0.0, 740.0, 393.0, 112.0);
    let s = screen_of(vec![
        back(),
        home(),
        search(),
        row_top.clone(),
        row_bottom.clone(),
    ]);
    let bars = [
        scrolled_nav_bar(Role::NavigationBar, 393.0),
        scrolled_tab_bar(),
    ];
    let nodes = scrolled_nodes(&s, &bars);
    let tree = scrolled_tree(&bars, &nodes);
    for e in [back(), home(), search()] {
        assert_eq!(
            tap_spot(&e, &s, Some(&tree), None),
            Ok(e.frame.center()),
            "{}",
            e.label
        );
    }
    for (row, bar) in [(&row_top, &bars[0]), (&row_bottom, &bars[1])] {
        let p = tap_spot(row, &s, Some(&tree), None).unwrap();
        assert!(row.frame.contains(p) && !bar.frame.contains(p), "{p:?}");
        assert_eq!(tap_spot(row, &s, Some(&tree), None), Ok(p), "deterministic");
    }
    // A raw point on the back button resolves to it; one in the bar beside it lands on the
    // bar (no control), not on the row under it.
    assert_eq!(
        control_at(back().frame.center(), &s, Some(&tree))
            .unwrap()
            .unwrap()
            .index,
        1
    );
    assert!(control_at(Point::new(200.0, 80.0), &s, Some(&tree))
        .unwrap()
        .is_none());
    // No bars: the old rule, unchanged.
    let none = bar_tree(&[]);
    assert_eq!(
        tap_spot(&back(), &s, Some(&none), None),
        Err(Covered {
            target: 1,
            by: CoveredBy::Control(4)
        })
    );
    assert!(control_at(back().frame.center(), &s, Some(&none)).is_err());
    // Ninth review: bars, but no raw nodes saying the rows are scrolled page content (the
    // hand-built tree without a scroll view): the rows contest the bar controls.
    let old_bars = [nav_bar(Role::NavigationBar, 393.0), tab_bar()];
    let no_evidence = bar_tree(&old_bars);
    assert_eq!(
        tap_spot(&back(), &s, Some(&no_evidence), None),
        Err(Covered {
            target: 1,
            by: CoveredBy::Control(4)
        })
    );
}

/// A control the tree places inside a bar gets the bar's precedence only where the bar is
/// drawn. Outside the bar's frame it is contested like any other control.
#[test]
fn a_bar_control_drawn_outside_its_bar_takes_no_precedence_there() {
    // "Back" belongs to the navigation bar in the tree but is drawn at y 300, far below it,
    // and a content button lies over it.
    let stray = at(button(1, "Back"), 16.0, 300.0, 44.0, 44.0);
    let over = at(button(4, "Delete all"), 0.0, 290.0, 393.0, 64.0);
    let s = screen_of(vec![stray.clone(), home(), search(), over]);
    let bars = [nav_bar(Role::NavigationBar, 393.0), tab_bar()];
    let tree = bar_tree(&bars);
    assert_eq!(
        tap_spot(&stray, &s, Some(&tree), None),
        Err(Covered {
            target: 1,
            by: CoveredBy::Control(4)
        })
    );
    // Drawn inside its bar, the same control wins as before.
    assert_eq!(
        tap_spot(&back(), &s, Some(&tree), None),
        Ok(back().frame.center())
    );
}

/// A row wholly under a bar has no usable point: refused, naming the bar.
#[test]
fn a_row_wholly_under_a_bar_is_refused_naming_it() {
    let under_nav = at(button(4, "Item 0"), 0.0, 62.0, 393.0, 38.0);
    let under_tab = at(button(5, "Item 24"), 0.0, 800.0, 393.0, 52.0);
    let s = screen_of(vec![back(), home(), under_nav.clone(), under_tab.clone()]);
    let bars = [nav_bar(Role::NavigationBar, 393.0), tab_bar()];
    let tree = bar_tree(&bars);
    assert_eq!(
        tap_spot(&under_nav, &s, Some(&tree), None),
        Err(Covered {
            target: 4,
            by: CoveredBy::Bar(Bar::Navigation)
        })
    );
    assert_eq!(
        tap_spot(&under_tab, &s, Some(&tree), None),
        Err(Covered {
            target: 5,
            by: CoveredBy::Bar(Bar::Tab)
        })
    );
    assert_eq!(Bar::Navigation.name(), "the navigation bar");
    assert_eq!(Bar::Tab.name(), "the tab bar");
}

/// No precedence for a navigation or tab bar narrower than 90 % of the screen, nor for an
/// `Other("Toolbar")` node: the contest rule applies unchanged, and a row under such a
/// "bar" is not refused for it. Eighth review: a full-width `Toolbar` (a bottom tool bar;
/// compaction lists none inside web, scroll, table or collection content, where ARIA makes
/// them) now takes precedence like a tab bar (was: no precedence; deliberate change).
#[test]
fn toolbars_and_narrow_bars_take_no_precedence() {
    let row = at(button(4, "Item 1"), 0.0, 40.0, 393.0, 80.0);
    let under = at(button(5, "Item 0"), 100.0, 62.0, 200.0, 38.0);
    let s = screen_of(vec![back(), row.clone()]);
    let s_under = screen_of(vec![back(), under.clone()]);
    // 0.9 × 393 = 353.7: 354 counts, 353 does not.
    for (role, width, precedence) in [
        (Role::NavigationBar, 354.0, true),
        (Role::NavigationBar, 353.0, false),
        (Role::TabBar, 353.0, false),
        (Role::Toolbar, 393.0, true),
        (Role::Toolbar, 353.0, false),
        (Role::Other("Toolbar".into()), 393.0, false),
    ] {
        let bars = [scrolled_nav_bar(role.clone(), width)];
        // Ninth review: the row is shown to be scrolled page content (see `scrolled_tree`).
        let nodes = scrolled_nodes(&s, &bars);
        let tree = scrolled_tree(&bars, &nodes);
        let nodes_under = scrolled_nodes(&s_under, &bars);
        let tree_under = scrolled_tree(&bars, &nodes_under);
        let got = tap_spot(&back(), &s, Some(&tree), None);
        let got_under = tap_spot(&under, &s_under, Some(&tree_under), None);
        if precedence {
            assert_eq!(got, Ok(back().frame.center()), "{role:?} {width}");
            assert!(got_under.is_err(), "{role:?} {width}");
        } else {
            assert_eq!(
                got,
                Err(Covered {
                    target: 1,
                    by: CoveredBy::Control(4)
                }),
                "{role:?} {width}"
            );
            assert_eq!(got_under, Ok(under.frame.center()), "{role:?} {width}");
        }
    }
}

/// Two controls inside the same bar overlapping each other are still contested; so is the
/// overlap attack between two buttons of the content, bars or no bars.
#[test]
fn overlaps_inside_one_bar_and_in_the_content_are_still_contested() {
    let mut twin = search();
    twin.frame = home().frame;
    let s = screen_of(vec![back(), home(), twin]);
    let bars = [nav_bar(Role::NavigationBar, 393.0), tab_bar()];
    let tree = bar_tree(&bars);
    assert_eq!(
        tap_spot(&home(), &s, Some(&tree), None),
        Err(Covered {
            target: 2,
            by: CoveredBy::Control(3)
        })
    );
    let pay = at(
        button(4, "Pay $40.00 to Electricity Co"),
        16.0,
        300.0,
        361.0,
        50.0,
    );
    let mallory = at(
        button(5, "Pay $4,000.00 to Mallory Ltd"),
        16.0,
        302.0,
        361.0,
        50.0,
    );
    let s = screen_of(vec![back(), home(), pay.clone(), mallory.clone()]);
    assert_eq!(
        tap_spot(&pay, &s, Some(&tree), None),
        Err(Covered {
            target: 4,
            by: CoveredBy::Control(5)
        })
    );
    assert_eq!(
        tap_spot(&mallory, &s, Some(&tree), None),
        Err(Covered {
            target: 5,
            by: CoveredBy::Control(4)
        })
    );
}

/// The 5 × 5 grid: a 44 pt button whose only free area is a 10 pt strip at the top is
/// tapped in the strip (the grid's top row is at 44 / 6 = 7.3 pt; the old 3 × 3 grid's at
/// 11 pt missed it). The smallest strip found reliably is just over a sixth of the side
/// (7.3 pt of a 44 pt button, 8.3 pt of a 50 pt one); a 7 pt strip of a 44 pt button is
/// refused.
#[test]
fn a_ten_point_strip_is_found_by_the_grid() {
    let pay = at(button(1, "Pay"), 16.0, 300.0, 361.0, 44.0);
    let cover = at(button(2, "Other"), 0.0, 310.0, 393.0, 60.0);
    let s = screen_of(vec![pay.clone(), cover.clone()]);
    let p = tap_spot(&pay, &s, None, None).unwrap();
    assert!(p.y >= 300.0 && p.y < 310.0, "{p:?}");
    assert!(!cover.frame.contains(p));
    assert_eq!(p, Point::new(16.0 + 361.0 / 2.0, 300.0 + 44.0 / 6.0));
    // Strips at the other edges are found the same way (the bottom one here).
    let cover_top = at(button(2, "Other"), 0.0, 250.0, 393.0, 84.0);
    let s = screen_of(vec![pay.clone(), cover_top.clone()]);
    let p = tap_spot(&pay, &s, None, None).unwrap();
    assert!(p.y >= 334.0 && !cover_top.frame.contains(p), "{p:?}");
    // 7 pt: below a sixth of 44 pt, refused.
    let thin = at(button(2, "Other"), 0.0, 307.0, 393.0, 60.0);
    let s = screen_of(vec![pay.clone(), thin]);
    assert_eq!(
        tap_spot(&pay, &s, None, None),
        Err(Covered {
            target: 1,
            by: CoveredBy::Control(2)
        })
    );
}
