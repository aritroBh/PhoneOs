#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Probe run 134 (iOS 26.5 simulator, `TreeFactProbeTests`): the safety rules on **real**
//! accessibility trees. Twenty-six adversarial reviews tested the gate on simulated trees;
//! the probes captured real ones, and several contradicted what the code assumed. Every
//! test here is driven by a captured tree (`tests/fixtures/ios26_probes/`, the runner's
//! typed tree of that run; see each file's header).
//!
//! 1. The edit menu's items are raw `MenuItem`s (kept as `Other(MenuItem)`): Rule P and
//!    the hand-off items fire; its paging "Forward" is local by its place in the menu.
//!    Context menus and the Home Screen's icon menu are plain buttons in cells.
//! 2. The share sheet is recognised by the OS's identifiers; its rows' frames are local to
//!    a remote container, so they are listed with no known position and never tapped.
//! 3. A tree too thin to describe the screen (`SFSafariViewController`): every input goes to
//!    a screen Jarvis cannot read.
//! 4. SpringBoard's notification banner is recognised by its identifier.
//! 5. Picker wheels, the slider and the stepper on a screen scroll's path.
//!
//! Probe run 135 added the edit menu's expanded list (plain buttons in cells), a second share
//! sheet, the web sign-in sheet's tree and two system alerts (in SpringBoard's tree only).

mod common;

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_observe::{compact_view, render, CompactView, UNPLACED_MARKER};
use jarvis_proto::{
    Action, AppInfo, CompactScreen, Direction, ParamKind, Policy, RiskCategory, Role,
    RunnerCommand, Screen, Tier, ToolParam, ToolSpec, UiNode,
};
use jarvis_safety::{
    control_at, copy_control, evaluate_sheeted, handoff_control, home_menu_row, home_view,
    in_alert, insert_control, observed_places, thin_tree, unplaced_target, Decision, FlowContext,
    InputKind, ScreenPlaces, ScreenSource, ScreenTree, HOME_SCREEN_READ, LOCAL_MENU_ITEMS,
    UNREADABLE_SCREEN_PREFIX, UNREADABLE_SCREEN_SUFFIX,
};

/// What `ios` (the observe tests' parser) uses to print a compact screen.
fn compact_text(c: &CompactScreen) -> String {
    format!("{c:?}")
}

const FLOW: &str = "what it read there could be passed on here";

const EDIT_MENU: &str = include_str!("fixtures/ios26_probes/edit_menu.txt");
const CONTEXT_MENU: &str = include_str!("fixtures/ios26_probes/context_menu.txt");
const HOME_ICON_MENU: &str = include_str!("fixtures/ios26_probes/home_icon_menu.txt");
const SHARE_SHEET: &str = include_str!("fixtures/ios26_probes/share_sheet.txt");
const SAFARI_VC: &str = include_str!("fixtures/ios26_probes/safari_view_controller.txt");
const BANNER_FRONT: &str = include_str!("fixtures/ios26_probes/banner_front_settings.txt");
const BANNER_HOME: &str = include_str!("fixtures/ios26_probes/banner_springboard.txt");
const CONTROLS: &str = include_str!("fixtures/ios26_probes/controls.txt");
const EDIT_MENU_LIST: &str = include_str!("fixtures/ios26_probes/edit_menu_list.txt");
const WEB_SIGN_IN: &str = include_str!("fixtures/ios26_probes/web_sign_in.txt");
const ALERT_PERMISSION: &str =
    include_str!("fixtures/ios26_probes/alert_notification_permission_springboard.txt");
const ALERT_SIGN_IN: &str =
    include_str!("fixtures/ios26_probes/alert_sign_in_consent_springboard.txt");

fn parse(text: &str) -> Screen {
    ios::parse_screen(text).unwrap()
}

/// The screen as the session works on it (SpringBoard through `home_view`).
fn seen(s: &Screen) -> Screen {
    home_view(s).unwrap_or_else(|| s.clone())
}

fn tree(v: &CompactView) -> ScreenTree<'_> {
    ScreenTree {
        sources: &v.sources,
        parents: &v.parents,
        ends: &v.ends,
        bars: &v.bars,
        nodes: &v.nodes,
        inputs: &v.inputs,
    }
}

fn places_of(s: &Screen, v: &CompactView) -> ScreenPlaces {
    observed_places(
        s,
        &v.screen,
        ScreenSource::Typed,
        Some((&v.webviews, &v.ends, &v.sources)),
    )
}

/// The gate's decision for `action` on `s` (as the session sees it), nothing read.
fn decide(policy: &Policy, action: &Action, s: &Screen) -> Decision {
    let s = seen(s);
    let v = compact_view(&s);
    let share = places_of(&s, &v).share_sheet();
    evaluate_sheeted(
        policy,
        action,
        &v.screen,
        None,
        ScreenSource::Typed,
        Some(&tree(&v)),
        &[],
        &["help me with this screen"],
        false,
        false,
        Some(share),
    )
}

/// The owner who confirms nothing (only forced questions are asked).
fn relaxed() -> Policy {
    Policy {
        confirm: Vec::new(),
        first_use_approval: false,
        ..Policy::default()
    }
}

fn default_policy() -> Policy {
    Policy {
        first_use_approval: false,
        ..Policy::default()
    }
}

fn element<'a>(c: &'a CompactScreen, label: &str) -> &'a jarvis_proto::Element {
    c.elements
        .iter()
        .find(|e| e.label == label)
        .unwrap_or_else(|| panic!("no element {label:?}: {:#?}", c.elements))
}

fn tap(c: &CompactScreen, label: &str) -> Action {
    Action::Tap {
        element: element(c, label).index,
    }
}

fn asks_phone_only(d: &Decision) -> bool {
    matches!(d, Decision::Confirm(r) if r.phone_only)
}

fn edit(n: &mut UiNode, f: &mut impl FnMut(&mut UiNode)) {
    f(n);
    for c in &mut n.children {
        edit(c, f);
    }
}

fn remove(n: &mut UiNode, drop: &impl Fn(&UiNode) -> bool) {
    n.children.retain(|c| !drop(c));
    for c in &mut n.children {
        remove(c, drop);
    }
}

fn flows(r: &Run) -> Vec<String> {
    r.confirms
        .iter()
        .filter(|c| c.summary.contains(FLOW))
        .map(|c| c.summary.clone())
        .collect()
}

fn lookup() -> ToolSpec {
    ToolSpec {
        name: "lookup".into(),
        description: "Looks something up".into(),
        params: vec![ToolParam {
            name: "q".into(),
            kind: ParamKind::String,
            required: true,
            description: "query".into(),
        }],
        tier: Tier::Native,
        risk: RiskCategory::None,
        app: None,
        self_contained: false,
    }
}

// ---- fix 1: the edit menu ------------------------------------------------------------------

fn edit_menu() -> Screen {
    parse(EDIT_MENU)
}

/// The same tree as the old shell typed it: every `MenuItem` a `Button`.
fn edit_menu_as_captured() -> Screen {
    let mut s = edit_menu();
    edit(&mut s.root, &mut |n| {
        if n.role.is_menu_item() {
            n.role = Role::Button;
        }
    });
    s
}

/// The menu's items are listed as the controls they are, under their own role, and the
/// model reads `MenuItem` (fails on the old core: a `MenuItem` with children was a
/// container, and only its static text was listed).
#[test]
fn edit_menu_items_are_listed_as_menu_items() {
    let c = jarvis_observe::compact(&edit_menu());
    for l in ["Cut", "Copy", "Replace…", "AutoFill"] {
        let e = element(&c, l);
        assert!(e.role.is_menu_item(), "{l}: {e:?}");
        assert!(e.enabled && e.role.is_interactive(), "{l}");
        assert!(jarvis_safety::takes_taps(&e.role), "{l}");
        assert!(
            !c.elements
                .iter()
                .any(|o| o.label == l && o.role == Role::StaticText),
            "{l} listed twice"
        );
    }
    let text = render(&c, 4_000);
    assert!(text.contains("MenuItem \"Replace…\""), "{text}");
    // Each item can be tapped where it is (nothing covers it).
    for l in ["Cut", "Copy", "Replace…", "AutoFill"] {
        assert!(c.cues.cover_of(element(&c, l).index).is_none(), "{l}");
    }
}

/// "Replace…" hands the selection to another service: it asks, phone only, whatever the
/// policy; "AutoFill" inserts content the question cannot show (Rule P).
#[test]
fn replace_and_autofill_ask_on_the_real_edit_menu() {
    let s = edit_menu();
    let c = jarvis_observe::compact(&s);
    let v = compact_view(&s);
    let t = tree(&v);
    let replace = tap(&c, "Replace…");
    let autofill = tap(&c, "AutoFill");
    assert!(handoff_control(&replace, &c, Some(&t)));
    assert_eq!(
        insert_control(&autofill, &c, Some(&t)),
        Some(RiskCategory::Credential)
    );
    for policy in [relaxed(), default_policy()] {
        for a in [&replace, &autofill] {
            let d = decide(&policy, a, &s);
            assert!(asks_phone_only(&d), "{a:?}: {d:?}");
        }
    }
    // The label iOS 26 uses carries an ellipsis character: not a local item.
    assert!(!LOCAL_MENU_ITEMS.contains(&"replace…"));
    assert!(!LOCAL_MENU_ITEMS.contains(&"replace"));
}

/// The old shell's typing (`Button`) hid every menu item from the core: "Replace…" was an
/// ordinary button and was sent unasked (the gap this fix closes; "AutoFill" still asked by
/// its whole label).
#[test]
fn as_captured_the_menu_items_were_ordinary_buttons() {
    let s = edit_menu_as_captured();
    let c = jarvis_observe::compact(&s);
    let v = compact_view(&s);
    let replace = tap(&c, "Replace…");
    assert!(!handoff_control(&replace, &c, Some(&tree(&v))));
    assert_eq!(decide(&relaxed(), &replace, &s), Decision::Allow);
}

/// Cut and Copy follow the copy rule: local (no hand-off, no insert) and Copy marks the
/// clipboard's source.
#[test]
fn cut_and_copy_are_local_on_the_real_edit_menu() {
    let s = edit_menu();
    let c = jarvis_observe::compact(&s);
    let v = compact_view(&s);
    let t = tree(&v);
    for l in ["Cut", "Copy"] {
        let a = tap(&c, l);
        assert!(!handoff_control(&a, &c, Some(&t)), "{l}");
        assert!(insert_control(&a, &c, Some(&t)).is_none(), "{l}");
        assert!(copy_control(&a, &c), "{l}");
        assert!(!asks_phone_only(&decide(&relaxed(), &a, &s)), "{l}");
    }
    assert_eq!(
        decide(&default_policy(), &tap(&c, "Copy"), &s),
        Decision::Allow
    );
}

/// The menu's "Forward" pages the menu: by its place (a sibling of the container that
/// holds the menu items) it is local, not the send phrase "forward" (fails on the old code:
/// asked as a send). The same button away from the menu items is a send again.
#[test]
fn the_menus_forward_pages_the_menu() {
    let s = edit_menu();
    let c = jarvis_observe::compact(&s);
    let forward = tap(&c, "Forward");
    assert_eq!(decide(&default_policy(), &forward, &s), Decision::Allow);
    // Without the menu items beside it (only by its label): a send phrase, asked.
    let mut away = edit_menu();
    remove(&mut away.root, &|n| {
        n.role == Role::CollectionView && n.frame.height < 100.0
    });
    let c2 = jarvis_observe::compact(&away);
    assert!(c2.elements.iter().all(|e| !e.role.is_menu_item()));
    let d = decide(&default_policy(), &tap(&c2, "Forward"), &away);
    assert!(matches!(d, Decision::Confirm(_)), "{d:?}");
}

/// Items the probe did not capture are decided by the closed list of local items: anything
/// not on it hands the selection off (asks), placed where the real items are.
#[test]
fn uncaptured_items_follow_the_closed_local_list() {
    for (label, hands) in [
        ("Paste", false),
        ("Select All", false),
        ("Select", false),
        ("Look Up", true),
        ("Translate", true),
        ("Share…", true),
        ("Search Web", true),
        ("Frobnicate", true),
    ] {
        let mut s = edit_menu();
        edit(&mut s.root, &mut |n| {
            if n.role.is_menu_item() && n.label.as_deref() == Some("Replace…") {
                n.label = Some(label.to_owned());
            }
            if n.role == Role::StaticText && n.label.as_deref() == Some("Replace…") {
                n.label = Some(label.to_owned());
            }
        });
        let c = jarvis_observe::compact(&s);
        let v = compact_view(&s);
        let a = tap(&c, label);
        assert_eq!(handoff_control(&a, &c, Some(&tree(&v))), hands, "{label}");
        let d = decide(&relaxed(), &a, &s);
        let paste = label == "Paste";
        assert_eq!(asks_phone_only(&d), hands || paste, "{label}: {d:?}");
    }
}

/// A session on the real edit menu: tapping "Replace…" asks on the phone even for the
/// owner who confirms nothing.
#[test]
fn a_session_asks_before_replace() {
    let s = edit_menu();
    let replace = index_of(&s, "Replace…");
    let mut c = config();
    c.policy.confirm.clear();
    let r = run(
        c,
        "Fix the text in the field",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            &format!(r#"{{"action":"tap","element":{replace}}}"#),
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[false]),
    );
    assert_eq!(r.confirms.len(), 1, "{:#?}", r.confirms);
    assert!(r.confirms[0].phone_only);
    assert!(
        r.confirms[0]
            .summary
            .contains("may send the selected content to another app or service"),
        "{}",
        r.confirms[0].summary
    );
    assert!(r.acts.iter().all(|a| point_of(a).is_none()), "{:?}", r.acts);
}

// ---- fix 1: context and icon menus ---------------------------------------------------------

/// Stated limit: a context menu's items are plain buttons in the cells of a collection view
/// next to an `Other "Preview"` — the same structure as an ordinary list of buttons — so they
/// are pressed as ordinary buttons (no hand-off by structure, no label rule invented).
#[test]
fn context_menu_items_are_ordinary_buttons_a_stated_limit() {
    let s = parse(CONTEXT_MENU);
    let c = jarvis_observe::compact(&s);
    let v = compact_view(&s);
    for l in ["Probe first action", "Probe second action"] {
        let e = element(&c, l);
        assert_eq!(e.role, Role::Button, "{l}");
        assert!(!handoff_control(&tap(&c, l), &c, Some(&tree(&v))), "{l}");
        assert!(!home_menu_row(&tap(&c, l), &c, Some(&tree(&v))), "{l}");
    }
}

fn icon_menu() -> Screen {
    parse(HOME_ICON_MENU)
}

/// The Home Screen's icon menu: its rows are recognised by SpringBoard's structure (a control
/// in a cell of a collection view), whatever their labels; the real Home Screen of icons has
/// none (fails on the old code: only `MenuItem`s or menu ancestors counted).
#[test]
fn the_real_icon_menu_rows_are_menu_rows() {
    let s = seen(&icon_menu());
    let v = compact_view(&s);
    let c = &v.screen;
    for l in [
        "Bluetooth",
        "Wi-Fi",
        "Cellular",
        "Battery",
        "Edit Home Screen",
        "Remove App",
    ] {
        assert!(home_menu_row(&tap(c, l), c, Some(&tree(&v))), "{l}");
    }
    // The real Home Screen's icons are not.
    let home = seen(&parse(include_str!(
        "../../jarvis-observe/tests/fixtures/ios26/springboard_home.txt"
    )));
    let hv = compact_view(&home);
    for e in hv.screen.elements.iter().filter(|e| e.role == Role::Button) {
        let a = Action::Tap { element: e.index };
        assert!(
            !home_menu_row(&a, &hv.screen, Some(&tree(&hv))),
            "{}",
            e.label
        );
    }
    // A screen with the menu is something Jarvis read (fails on the old code: its rows are
    // plain buttons, read as icons).
    assert_eq!(
        places_of(&s, &v).read_places(),
        vec![FlowContext::app("com.apple.springboard")]
    );
    assert!(places_of(&home, &hv).read_places().is_empty());
}

/// Twenty-sixth review (L1) on the real tree: a quick action labelled like a known app is
/// not that app's launch; its destination is undecidable, so every place read is a source
/// for it and its approval keeps nothing (a later `open_app` of that app asks again). Fails
/// on the old code: resolved as a launch of the app, which leaves the Home Screen out.
#[test]
fn an_icon_menu_row_named_like_an_app_is_no_launch_of_it() {
    let s = icon_menu();
    let bluetooth = index_of(&seen(&s), "Bluetooth");
    let mut c = config();
    c.known_apps
        .push(AppInfo::new("com.example.bluetooth", "Bluetooth"));
    c.tools.push(lookup());
    let r = run(
        c,
        "look up hello and then open bluetooth",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            r#"{"action":"call_tool","name":"lookup","args":{"q":"hello"}}"#,
            &format!(r#"{{"action":"tap","element":{bluetooth}}}"#),
            r#"{"action":"open_app","app":"Bluetooth"}"#,
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[true; 6]),
    );
    let fq = flows(&r);
    // The tap keeps every place read as a source, the Home Screen with its menu included (a
    // launch of a known app would leave the Home Screen out).
    let tap_q = fq
        .iter()
        .find(|q| q.starts_with("Tap \"Bluetooth\""))
        .unwrap();
    assert!(tap_q.contains(HOME_SCREEN_READ), "{tap_q}");
    assert!(fq.iter().any(|q| q.starts_with("Open")), "{fq:#?}");
}

// ---- fix 2: the share sheet ----------------------------------------------------------------

fn share_sheet() -> Screen {
    parse(SHARE_SHEET)
}

/// The real share sheet is a raw `Popover` (typed `Sheet`) with no "AirDrop" row: it is
/// recognised by the OS's identifiers (fails on the old code: no AirDrop, no share sheet).
#[test]
fn the_real_share_sheet_is_recognised_by_its_identifiers() {
    let s = share_sheet();
    let v = compact_view(&s);
    let p = places_of(&s, &v);
    assert!(p.share_sheet());
    assert!(p.shows_sheet());
    assert!(v.screen.elements.iter().all(|e| e.label != "AirDrop"));
    // Each identifier alone decides.
    for keep in jarvis_safety::SHARE_SHEET_IDS {
        let mut one = share_sheet();
        edit(&mut one.root, &mut |n| {
            if n.identifier
                .as_deref()
                .is_some_and(|i| jarvis_safety::SHARE_SHEET_IDS.contains(&i) && i != *keep)
            {
                n.identifier = None;
            }
        });
        let v1 = compact_view(&one);
        assert!(places_of(&one, &v1).share_sheet(), "{keep}");
    }
    // None of them, no AirDrop: no share sheet.
    let mut none = share_sheet();
    edit(&mut none.root, &mut |n| {
        if n.identifier
            .as_deref()
            .is_some_and(|i| jarvis_safety::SHARE_SHEET_IDS.contains(&i))
        {
            n.identifier = None;
        }
    });
    let vn = compact_view(&none);
    assert!(!places_of(&none, &vn).share_sheet());
}

/// The rows' frames are local to the remote container ("Reminders" at `{24, 107}` while the
/// sheet is at `{9, 565}`): they are listed with the position marker, a tap on one is
/// refused, they neither cover nor are covered, and a point is never resolved to one
/// (fails on the old code: tapped at the local coordinates, on the app behind).
#[test]
fn share_sheet_rows_have_no_known_position() {
    let s = share_sheet();
    let v = compact_view(&s);
    let c = &v.screen;
    let text = render(c, 4_000);
    for l in ["Reminders", "More", "Copy", "Assign to Contact", "Print"] {
        let e = element(c, l);
        assert!(c.cues.is_unplaced(e.index), "{l}");
        assert!(c.cues.cover_of(e.index).is_none(), "{l}");
        let line = text
            .lines()
            .find(|line| line.starts_with(&format!("[{}] ", e.index)))
            .unwrap();
        assert!(line.ends_with(UNPLACED_MARKER), "{line}");
        for a in [
            Action::Tap { element: e.index },
            Action::LongPress {
                element: e.index,
                duration_ms: 800,
            },
            Action::Scroll {
                direction: Direction::Down,
                element: Some(e.index),
            },
        ] {
            assert_eq!(unplaced_target(&a, c), Some(e.index), "{l} {a:?}");
        }
        // A point at the row's (local) frame is not the row.
        let p = e.frame.center();
        assert!(
            control_at(p, c, Some(&tree(&v))).unwrap_or(None).is_none(),
            "{l}"
        );
    }
    // Nothing inside the remote container is a drawn node a point could land on.
    let container = v
        .screen
        .elements
        .iter()
        .find(|e| e.label == "Reminders")
        .map(|e| e.index)
        .unwrap();
    let raw = v.sources[(container - 1) as usize];
    assert!(v.nodes.iter().all(|n| n.node != raw));
}

/// A session on the real share sheet: a tap on a row is refused with a reason the model
/// sees, nothing is sent; a `tap_point` over the sheet asks, phone only (the share sheet's
/// rule), whatever the policy.
#[test]
fn a_session_never_taps_a_share_sheet_row_by_its_frame() {
    let s = share_sheet();
    let copy = index_of(&s, "Copy");
    let mut c = vision_config();
    c.policy.confirm.clear();
    let r = run(
        c,
        "Copy the text from the share sheet",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            &format!(r#"{{"action":"tap","element":{copy}}}"#),
            r#"{"action":"look"}"#,
            r#"{"action":"tap_point","x":0.3,"y":0.75}"#,
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[false; 6]),
    );
    let all = r.all_prompt_text();
    assert!(all.contains("does not know where element"), "{all}");
    assert!(all.contains(UNPLACED_MARKER.trim()), "{all}");
    assert!(
        r.acts
            .iter()
            .all(|a| !matches!(a, RunnerCommand::Tap { .. })),
        "{:?}",
        r.acts
    );
    let asked: Vec<&str> = r.confirms.iter().map(|c| c.summary.as_str()).collect();
    assert!(
        r.confirms
            .iter()
            .any(|c| c.phone_only && c.summary.starts_with("Tap")),
        "{asked:#?}"
    );
}

/// Marker and refusal are one decision, on the real sheet and on random screens with
/// random unplaced sets: a line carries the marker exactly when a tap naming its element
/// is refused for it.
#[test]
fn position_marker_iff_refusal() {
    let check = |c: &CompactScreen| {
        let text = render(c, 1_000_000);
        for e in &c.elements {
            let line = text
                .lines()
                .find(|l| l.starts_with(&format!("[{}] ", e.index)))
                .unwrap();
            let refused = unplaced_target(&Action::Tap { element: e.index }, c).is_some();
            assert_eq!(line.ends_with(UNPLACED_MARKER), refused, "{line}");
        }
    };
    check(&compact_view(&share_sheet()).screen);
    for seed in 0..300u64 {
        let mut c = jarvis_proto::fixtures::random::random_compact_screen(seed);
        c.cues.unplaced = c
            .elements
            .iter()
            .filter(|e| (u64::from(e.index) ^ seed) % 3 == 0)
            .map(|e| e.index)
            .collect();
        check(&c);
    }
}

/// Stickiness keeps applying: once the share sheet was seen, a later look with its
/// identifiers gone (cell reuse) and a `Sheet` still up asks for a press there.
#[test]
fn the_share_sheet_stays_sticky_without_its_identifiers() {
    let s = share_sheet();
    let mut later = share_sheet();
    edit(&mut later.root, &mut |n| {
        if n.identifier
            .as_deref()
            .is_some_and(|i| jarvis_safety::SHARE_SHEET_IDS.contains(&i))
        {
            n.identifier = None;
        }
    });
    let v = compact_view(&later);
    assert!(!places_of(&later, &v).share_sheet());
    let copy = index_of(&later, "Copy");
    assert!(!v.screen.cues.is_unplaced(copy));
    let mut c = config();
    c.policy.confirm.clear();
    let r = run(
        c,
        "Copy the text from the share sheet",
        MemoryStore::new(),
        Phone::new(s, move |cmd, _| match cmd {
            RunnerCommand::Wait { .. } => Transition::Go(later.clone()),
            _ => Transition::Stay,
        }),
        Script::model(&[
            r#"{"action":"wait","ms":1000}"#,
            &format!(r#"{{"action":"tap","element":{copy}}}"#),
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[false; 4]),
    );
    assert!(
        r.confirms
            .iter()
            .any(|c| c.phone_only && c.summary.starts_with("Tap \"Copy\"")),
        "{:#?}",
        r.confirms
    );
}

// ---- fix 3: a tree that does not describe the screen ---------------------------------------

fn safari_vc() -> Screen {
    parse(SAFARI_VC)
}

const JARVIS_UNREADABLE: &str =
    "a screen in Jarvis that Jarvis cannot read (it may show another party's page)";

/// The real tree with `SFSafariViewController` up is thin (the vision fallback's predicate)
/// and every input on it goes to the screen Jarvis cannot read, not to the app's place
/// (fails on the old code: the app's own place).
#[test]
fn the_safari_view_controller_tree_is_unreadable() {
    let s = safari_vc();
    let v = compact_view(&s);
    assert!(v.screen.elements.is_empty(), "{:#?}", v.screen.elements);
    assert!(thin_tree(&v.screen));
    let p = places_of(&s, &v);
    let u = FlowContext::UnreadableScreen("dev.jarvis.app".into());
    assert_eq!(p.unreadable(), Some(&u));
    for a in [
        Action::TapPoint { x: 0.5, y: 0.5 },
        Action::Type {
            element: None,
            text: "hello".into(),
            submit: false,
        },
        Action::Swipe {
            direction: Direction::Up,
            element: None,
        },
        Action::Scroll {
            direction: Direction::Down,
            element: None,
        },
    ] {
        assert_eq!(
            p.destinations(&a, &v.screen, Some(&tree(&v))),
            vec![u.clone()],
            "{a:?}"
        );
    }
    // Waits and looks are seen by it too.
    assert!(p
        .destinations(&Action::Wait { ms: 1000 }, &v.screen, Some(&tree(&v)))
        .contains(&u));
    assert!(p.every().contains(&u));
    assert_eq!(u.display(|_| Some("Jarvis".into())), JARVIS_UNREADABLE);
    assert!(u.undecidable() && u.bundle().is_none());
    // What is read there is unchanged: the app's place.
    assert_eq!(p.read_places(), vec![FlowContext::app("dev.jarvis.app")]);
}

/// A screen whose tree is fine is not thin: its inputs go to its own place as before.
#[test]
fn a_real_tree_that_describes_its_screen_is_not_unreadable() {
    for text in [CONTROLS, EDIT_MENU, BANNER_FRONT] {
        let s = parse(text);
        let v = compact_view(&s);
        assert!(!thin_tree(&v.screen), "{}", s.app_bundle_id);
        let p = places_of(&s, &v);
        assert!(p.unreadable().is_none());
        let first = v
            .screen
            .elements
            .iter()
            .find(|e| e.role == Role::Button)
            .unwrap();
        assert_eq!(
            p.destinations(
                &Action::Tap {
                    element: first.index
                },
                &v.screen,
                Some(&tree(&v))
            ),
            vec![FlowContext::app(&s.app_bundle_id)]
        );
    }
}

/// A vision task on the real thin tree: each `tap_point` asks, naming the unreadable screen
/// and everything read, and no approval is kept (the next one asks again).
#[test]
fn every_input_on_an_unreadable_screen_asks() {
    let s = safari_vc();
    let r = run(
        vision_config(),
        "Fill in the contact form on the page",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            r#"{"action":"tap_point","x":0.5,"y":0.3}"#,
            r#"{"action":"look"}"#,
            r#"{"action":"tap_point","x":0.5,"y":0.4}"#,
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[true; 8]),
    );
    let fq = flows(&r);
    let taps: Vec<&String> = fq.iter().filter(|q| q.starts_with("Tap")).collect();
    assert_eq!(taps.len(), 2, "{fq:#?}");
    for q in &taps {
        assert!(q.contains(JARVIS_UNREADABLE), "{q}");
        assert!(q.contains("screen pictures"), "{q}");
        assert!(q.contains("Jarvis has read"), "{q}");
    }
    // The look between them asks too (the screen it shows may be another party's), and,
    // once approved, is taken.
    assert!(
        fq.iter()
            .any(|q| q.starts_with("Look") && q.contains(JARVIS_UNREADABLE)),
        "{fq:#?}"
    );
    assert!(r.all_prompt_text().contains("look -> screenshot taken"));
}

/// Jarvis's words in the name are never shortened (only the app's name is), and an app
/// name imitating them is shown by its bundle id.
#[test]
fn the_unreadable_screens_name_is_guarded() {
    let long = "An App With A Very Long Display Name That Goes On And On Forever";
    let name = format!("{UNREADABLE_SCREEN_PREFIX}{long}{UNREADABLE_SCREEN_SUFFIX}");
    let (row, _, _) = jarvis_safety::flow_note_into(&["Messages".into()], &[name]);
    assert!(row.text.contains(UNREADABLE_SCREEN_SUFFIX), "{}", row.text);
    assert!(
        row.text
            .contains(&format!(", into {UNREADABLE_SCREEN_PREFIX}An App")),
        "{}",
        row.text
    );
    assert!(!row.text.contains(long), "{}", row.text);
    // An app named like Jarvis's words: by its bundle id.
    let mut s = safari_vc();
    s.app_name = Some("Mail that Jarvis cannot read".into());
    s.root.label = s.app_name.clone();
    let r = run(
        vision_config(),
        "Fill in the contact form on the page",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            r#"{"action":"tap_point","x":0.5,"y":0.3}"#,
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[false; 4]),
    );
    let fq = flows(&r);
    assert!(
        fq.iter().any(|q| q.contains(&format!(
            "{UNREADABLE_SCREEN_PREFIX}dev.jarvis.app{UNREADABLE_SCREEN_SUFFIX}"
        ))),
        "{fq:#?}"
    );
}

// ---- fix 4: banners ------------------------------------------------------------------------

/// SpringBoard's banner is `Other(Type83) id=NotificationShortLookView` under an `Other
/// "Notification"`: kept unchanged and in full for the model, the Home Screen showing it is
/// a read, and it is named as the Home Screen with notifications (fails on the old code:
/// stubbed with its container, an icons-only home screen).
#[test]
fn the_real_banner_makes_the_home_screen_a_read() {
    let raw = parse(BANNER_HOME);
    let s = seen(&raw);
    let v = compact_view(&s);
    let c = &v.screen;
    assert!(
        c.elements
            .iter()
            .any(|e| e.label.contains("Probe banner 4417")),
        "{:#?}",
        c.elements
    );
    let p = places_of(&s, &v);
    let home = FlowContext::app("com.apple.springboard");
    assert_eq!(p.read_places(), vec![home.clone()]);
    assert_eq!(home.display_read(|_| None), HOME_SCREEN_READ);
    // A tap on the banner is a tap in an alert-like container (R4), never a launch.
    let banner = c
        .elements
        .iter()
        .find(|e| e.role == Role::Button && e.label.contains("Probe banner"))
        .unwrap();
    assert!(in_alert(
        &Action::Tap {
            element: banner.index
        },
        c,
        Some(&tree(&v))
    ));
    // The app-switcher card behind it stays stubbed (its label never reaches the model).
    assert!(c.elements.iter().all(|e| e.label != "Settings"));
}

/// The front app's own tree (Settings) does not hold the banner: it is not a read of it. A
/// banner over an app reaches Jarvis only through a screen picture.
#[test]
fn the_front_apps_tree_alone_is_not_a_read_of_the_banner() {
    let s = parse(BANNER_FRONT);
    let v = compact_view(&s);
    let all = render(&v.screen, 1_000_000);
    assert!(!all.contains("Probe banner"), "{all}");
    assert!(!all.contains("Jarvis probe"), "{all}");
    assert_eq!(
        places_of(&s, &v).read_places(),
        vec![FlowContext::app("com.apple.Preferences")]
    );
}

/// A session on the real SpringBoard tree with the banner: the question of the next input
/// names the Home Screen with notifications.
#[test]
fn a_question_after_the_banner_names_the_home_screen_read() {
    let s = parse(BANNER_HOME);
    let mut c = config();
    c.tools.push(lookup());
    let r = run(
        c,
        "look up hello",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            r#"{"action":"call_tool","name":"lookup","args":{"q":"hello"}}"#,
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[false; 4]),
    );
    let fq = flows(&r);
    assert!(fq.iter().any(|q| q.contains(HOME_SCREEN_READ)), "{fq:#?}");
}

// ---- fix 5: controls on a drag's path ------------------------------------------------------

/// Spike (h) on the real tree: a screen scroll whose path crosses a picker wheel or the
/// slider is a full input; with them gone the same scroll only scrolls. A scroll starting on
/// the stepper's button is full (it is a button).
#[test]
fn real_wheels_slider_and_stepper_make_a_scroll_full() {
    let s = parse(CONTROLS);
    let c = jarvis_observe::compact(&s);
    assert!(c.elements.iter().filter(|e| e.role == Role::Picker).count() >= 4);
    assert!(c.elements.iter().any(|e| e.role == Role::Slider));
    let down = Action::Scroll {
        direction: Direction::Down,
        element: None,
    };
    assert_eq!(InputKind::of(&down, &c), InputKind::Full);
    let up = Action::Scroll {
        direction: Direction::Up,
        element: None,
    };
    assert_eq!(InputKind::of(&up, &c), InputKind::Full);
    // Only the slider left on the path: still full.
    let mut slider = parse(CONTROLS);
    remove(&mut slider.root, &|n| {
        matches!(&n.role, Role::Other(name) if name == "Picker" || name == "DatePicker")
            || n.role == Role::Switch
            || n.identifier.as_deref() == Some("probe.controls.stepper")
    });
    let cs = jarvis_observe::compact(&slider);
    assert!(cs.elements.iter().all(|e| e.role != Role::Picker));
    assert_eq!(InputKind::of(&down, &cs), InputKind::Full);
    // Nothing that reacts left: the same scroll only scrolls.
    let mut bare = slider.clone();
    remove(&mut bare.root, &|n| n.role == Role::Slider);
    let cb = jarvis_observe::compact(&bare);
    assert_eq!(InputKind::of(&down, &cb), InputKind::Scroll);
    // The stepper: its row-wide button (its Decrement and Increment buttons are inside it).
    let stepper = c
        .elements
        .iter()
        .find(|e| e.label.starts_with("Probe count 3"))
        .unwrap();
    assert_eq!(stepper.role, Role::Button);
    let on = Action::Scroll {
        direction: Direction::Down,
        element: Some(stepper.index),
    };
    assert_eq!(InputKind::of(&on, &c), InputKind::Full);
}

// ---- run 135: the edit menu's expanded list --------------------------------------------------

fn edit_menu_list() -> Screen {
    parse(EDIT_MENU_LIST)
}

/// The expanded list has no `MenuItem` and no identifier: plain buttons in cells, the shape a
/// context menu and the icon menu share. Its rows carry the OS's own words.
#[test]
fn the_expanded_edit_menu_is_a_list_of_plain_buttons() {
    let s = edit_menu_list();
    let shape = jarvis_safety::edit_menu_shape(&s.root);
    assert!(
        !shape.menu_items && shape.list && shape.os_words,
        "{shape:?}"
    );
    let c = jarvis_observe::compact(&s);
    assert_eq!(element(&c, "Look Up").role, Role::Button);
    // As the core reads it without the session's knowledge: ordinary buttons (the gap).
    let v = compact_view(&s);
    assert!(!handoff_control(&tap(&c, "Look Up"), &c, Some(&tree(&v))));
    // The three real trees of that shape: the context menu and the icon menu carry none of
    // the OS's edit-menu words (so the second rule never takes them).
    for text in [CONTEXT_MENU, HOME_ICON_MENU] {
        let shape = jarvis_safety::edit_menu_shape(&parse(text).root);
        assert!(
            shape.list && !shape.os_words && !shape.menu_items,
            "{shape:?}"
        );
    }
    let first = jarvis_safety::edit_menu_shape(&edit_menu().root);
    assert!(first.menu_items, "{first:?}");
}

/// Read as the edit menu (what the session does, `edit_menu_view`): the hand-off items ask,
/// phone only, whatever the policy; Cut and Copy do not; AutoFill keeps Rule P.
#[test]
fn the_expanded_edit_menu_asks_for_its_hand_off_items() {
    let s = jarvis_safety::edit_menu_view(&edit_menu_list());
    let c = jarvis_observe::compact(&s);
    let v = compact_view(&s);
    let t = tree(&v);
    for l in ["Look Up", "Translate", "Search Web", "Share…", "Replace…"] {
        let a = tap(&c, l);
        assert!(element(&c, l).role.is_menu_item(), "{l}");
        assert!(handoff_control(&a, &c, Some(&t)), "{l}");
        assert!(asks_phone_only(&decide(&relaxed(), &a, &s)), "{l}");
    }
    let autofill = tap(&c, "AutoFill");
    assert_eq!(
        insert_control(&autofill, &c, Some(&t)),
        Some(RiskCategory::Credential)
    );
    assert!(asks_phone_only(&decide(&relaxed(), &autofill, &s)));
    for l in ["Cut", "Copy"] {
        let a = tap(&c, l);
        assert!(!handoff_control(&a, &c, Some(&t)), "{l}");
        assert!(copy_control(&a, &c), "{l}");
        assert!(!asks_phone_only(&decide(&relaxed(), &a, &s)), "{l}");
    }
}

/// The session: page 1 (`MenuItem`s), "Forward" (local, unasked), page 2 (the list) — the
/// state is sticky, so "Look Up" there asks, phone only, for the owner who confirms nothing.
#[test]
fn a_session_follows_the_edit_menu_to_its_list() {
    let page1 = edit_menu();
    let forward = index_of(&page1, "Forward");
    let look_up = index_of(&edit_menu_list(), "Look Up");
    let mut c = config();
    c.policy.confirm.clear();
    let r = run(
        c,
        "Look up the word in the field",
        MemoryStore::new(),
        Phone::new(page1, |cmd, s| {
            if taps(cmd, s, "Forward") {
                Transition::Go(edit_menu_list())
            } else {
                Transition::Stay
            }
        }),
        Script::model(&[
            &format!(r#"{{"action":"tap","element":{forward}}}"#),
            &format!(r#"{{"action":"tap","element":{look_up}}}"#),
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[false; 4]),
    );
    assert_eq!(r.confirms.len(), 1, "{:#?}", r.confirms);
    assert!(r.confirms[0].phone_only);
    assert!(
        r.confirms[0].summary.starts_with("Tap \"Look Up\""),
        "{}",
        r.confirms[0].summary
    );
    assert!(
        r.all_prompt_text().contains("MenuItem \"Look Up\""),
        "the list's rows are read as menu items"
    );
}

/// Reached directly (no page-1 look): the list is the edit menu by the OS's own words.
/// A context menu reached directly is not (its items are ordinary buttons: stated limit).
#[test]
fn a_session_on_the_list_alone_decides_by_the_os_words() {
    for (screen, label, asks) in [
        (edit_menu_list(), "Translate", true),
        (parse(CONTEXT_MENU), "Probe first action", false),
    ] {
        let i = index_of(&screen, label);
        let mut c = config();
        c.policy.confirm.clear();
        let r = run(
            c,
            "Do it",
            MemoryStore::new(),
            Phone::still(screen),
            Script::model(&[
                &format!(r#"{{"action":"tap","element":{i}}}"#),
                r#"{"action":"done","summary":"ok"}"#,
            ])
            .decisions(&[false; 4]),
        );
        assert_eq!(!r.confirms.is_empty(), asks, "{label}: {:#?}", r.confirms);
    }
}

// ---- run 135: the web sign-in sheet; system alerts -------------------------------------------

/// With an `ASWebAuthenticationSession` sheet up the host's snapshot is twelve unlabelled
/// nodes, the host app's own screen gone too: thin, every input to the unreadable screen.
#[test]
fn the_web_sign_in_tree_is_unreadable() {
    let s = parse(WEB_SIGN_IN);
    let v = compact_view(&s);
    assert!(v.screen.elements.is_empty(), "{:#?}", v.screen.elements);
    assert!(thin_tree(&v.screen));
    let p = places_of(&s, &v);
    let u = FlowContext::UnreadableScreen("dev.jarvis.app".into());
    assert_eq!(
        p.destinations(
            &Action::TapPoint { x: 0.5, y: 0.5 },
            &v.screen,
            Some(&tree(&v))
        ),
        vec![u]
    );
}

/// System alerts are in SpringBoard's tree only. The runner now reports the top layer (the
/// alert, SpringBoard's tree) and the core reads it as "a system alert": `top_layer.rs`.
/// These pin the trees.
#[test]
fn system_alerts_are_in_springboards_tree() {
    for (text, title) in [
        (ALERT_PERMISSION, "Would Like to Send You Notifications"),
        (ALERT_SIGN_IN, "to Sign In"),
    ] {
        let s = parse(text);
        assert_eq!(s.app_bundle_id, "com.apple.springboard");
        let c = jarvis_observe::compact(&seen(&s));
        assert!(
            c.elements
                .iter()
                .any(|e| e.role == Role::Alert || e.label.contains(title)),
            "{title}: {:#?}",
            c.elements
        );
    }
}

/// The list without any of the OS's words (renamed in the real tree): after a page-1 look it
/// is still the edit menu (sticky) and "Translate" asks; reached directly it is not.
#[test]
fn the_edit_menu_state_is_sticky_from_its_first_page() {
    let renamed = || {
        let mut s = edit_menu_list();
        edit(&mut s.root, &mut |n| {
            if let Some(l) = n.label.as_deref() {
                if jarvis_safety::EDIT_MENU_OS_LABELS.contains(&l) {
                    n.label = Some(format!("{l} it"));
                }
            }
        });
        s
    };
    assert!(!jarvis_safety::edit_menu_shape(&renamed().root).os_words);
    let translate = index_of(&renamed(), "Translate");
    for (start, asks) in [(edit_menu(), true), (renamed(), false)] {
        let forward = jarvis_observe::compact(&start)
            .elements
            .iter()
            .find(|e| e.label == "Forward")
            .map(|e| e.index);
        let mut steps: Vec<String> = Vec::new();
        if let Some(f) = forward {
            steps.push(format!(r#"{{"action":"tap","element":{f}}}"#));
        }
        steps.push(format!(r#"{{"action":"tap","element":{translate}}}"#));
        steps.push(r#"{"action":"done","summary":"ok"}"#.to_owned());
        let steps: Vec<&str> = steps.iter().map(String::as_str).collect();
        let mut c = config();
        c.policy.confirm.clear();
        let r = run(
            c,
            "Translate the word",
            MemoryStore::new(),
            Phone::new(start, move |cmd, s| {
                if taps(cmd, s, "Forward") {
                    Transition::Go(renamed())
                } else {
                    Transition::Stay
                }
            }),
            Script::model(&steps).decisions(&[false; 4]),
        );
        let asked = r
            .confirms
            .iter()
            .any(|c| c.phone_only && c.summary.starts_with("Tap \"Translate\""));
        assert_eq!(asked, asks, "{:#?}", r.confirms);
    }
}
