#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Probe round 3 (Mac run 139, iPhone 17 Pro Max simulator, iOS 26.5): the gate on the
//! real trees of `TreeFactProbe3Tests` / `TreeFactProbe3SystemTests`. Every test is driven
//! by a capture under `tests/fixtures/ios26_probes3/` (copied verbatim below a one-line
//! header; the typed tree the core receives is the part before the raw XCUITest snapshot).
//!
//! * F1: a long sideways swipe on a list row deleted it (p4d): every sideways drag Jarvis
//!   sends is capped at the measured reveal path, and any sideways drag across a row asks.
//! * F2: out-of-process pickers (p2) are a place of their own over the app.
//! * F3: the Return key's real identifiers (p4b, all twelve types; p10 German); a key of
//!   unknown type asks.
//! * F4: the labels the OS gives icon-only controls (p4a, p10) are at least their symbol's
//!   risk by label alone.
//! * F5: Contacts' "Delete Contact" is a `Cell` (p5c).
//! * F6: Safari's JavaScript dialogs and HTTP sign-in dialog (p6a, p6b).
//! * F7: modals, links, page inputs, selection facts, an in-app web view, a badge and
//!   landscape, as the gate reads them.

mod common;

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_observe::{compact_view, CompactView};
use jarvis_proto::{
    Action, AppInfo, CompactScreen, Direction, Element, Policy, RiskCategory, Role, RunnerCommand,
    Screen, UiNode,
};
use jarvis_safety::gestures::{drag_gesture, GestureKind, MAX_SIDEWAYS_SHARE};
use jarvis_safety::{
    evaluate_sheeted, home_view, identifier_symbol_risk, max_risk, observed_places, summary_with,
    Decision, FlowContext, InputKind, PickerKind, ScreenPlaces, ScreenSource, ScreenTree,
    EMERGENCY_CALL_NOTE, ROW_SWIPE_NOTE,
};

/// What `ios` (the observe tests' parser) uses to print a compact screen.
fn compact_text(c: &CompactScreen) -> String {
    format!("{c:?}")
}

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("fixtures/ios26_probes3/", $name))
    };
}

const FLOW: &str = "what it read there could be passed on here";

// p4d: the swipe rows, the segmented control, tabs, values and selection.
const P4D_BEFORE: &str = fixture!("p4d-controls-02-controls-before-swipes-runner-tree.txt");
const P4D_REVEALED: &str = fixture!("p4d-controls-03-row1-after-runner-swipe-tree.txt");
const P4D_BEFORE_FULL: &str =
    fixture!("p4d-controls-04-controls-before-full-swipe-runner-tree.txt");
const P4D_AFTER_FULL: &str = fixture!("p4d-controls-05-row2-after-full-swipe-tree.txt");
const P4D_TABS: &str = fixture!("p4d-controls-06-tabs-tree.txt");
const P4D_VALUES: &str = fixture!("p4d-controls-09-controls-values-tree.txt");
const P4D_DATE_OPEN: &str = fixture!("p4d-controls-10-compact-date-open-tree.txt");
const P4D_SELECTION: &str = fixture!("p4d-controls-12-controls-selection-tree.txt");
// p2: the pickers.
const PHOTO_PICKER: &str = fixture!("p2-pickers-01-photo-picker-tree.txt");
const PHOTO_PICKER_RUNNER: &str = fixture!("p2-pickers-04-photo-picker-runner-tree.txt");
const CONTACT_PICKER: &str = fixture!("p2-pickers-05-contact-picker-tree.txt");
const CONTACT_PICKER_RUNNER: &str = fixture!("p2-pickers-08-contact-picker-runner-tree.txt");
const FILE_PICKER: &str = fixture!("p2-pickers-09-document-picker-tree.txt");
const FILE_PICKER_RUNNER: &str = fixture!("p2-pickers-12-document-picker-runner-tree.txt");
// p4c: modals.
const MODAL_BASE: &str = fixture!("p4c-modals-01-base-tree.txt");
const ACTION_SHEET: &str = fixture!("p4c-modals-02-action-sheet-tree.txt");
const ALERT_FIELD: &str = fixture!("p4c-modals-05-alert-field-tree.txt");
const FULL_SHEET: &str = fixture!("p4c-modals-08-sheet-tree.txt");
const HALF_SHEET: &str = fixture!("p4c-modals-10-half-sheet-tree.txt");
const COVER: &str = fixture!("p4c-modals-12-cover-tree.txt");
const POPOVER: &str = fixture!("p4c-modals-14-popover-tree.txt");
const CONTEXT_MENU: &str = fixture!("p4c-modals-18-context-menu-tree.txt");
// p4a, p10: symbols and the German run.
const SYMBOLS: &str = fixture!("p4a-symbols-01-symbols-tree.txt");
const SYMBOLS_EN_GB: &str = fixture!("p10-german-01-symbols-tree.txt");
const GERMAN_RETURN: &str = fixture!("p10-german-04-return-default-tree.txt");
const GERMAN_GO: &str = fixture!("p10-german-05-return-go-tree.txt");
// p5c, p6a, p6b, p6c, p12a, p7.
const CONTACT_CARD: &str = fixture!("p5c-contacts-01-no-delete-tree.txt");
const SAFARI_PAGE: &str = fixture!("p6a-safari-page-01-page-tree.txt");
const JS_CONFIRM: &str = fixture!("p6a-safari-page-07-js-confirm-tree.txt");
const JS_PROMPT: &str = fixture!("p6a-safari-page-10-js-prompt-tree.txt");
const SELECT_OPEN: &str = fixture!("p6b-safari-inputs-01-select-tree.txt");
const DATE_INPUT_OPEN: &str = fixture!("p6b-safari-inputs-04-date-input-tree.txt");
const FILE_INPUT_OPEN: &str = fixture!("p6b-safari-inputs-07-file-input-tree.txt");
const BASIC_AUTH: &str = fixture!("p6b-safari-inputs-13-basic-auth-tree.txt");
const WEBVIEW: &str = fixture!("p6c-webview-01-webview-tree.txt");
const HOME_BADGE: &str = fixture!("p12a-surfaces-01-home-badge-tree.txt");
const LANDSCAPE: &str = fixture!("p7-landscape-02-landscape-tree.txt");
const LANDSCAPE_KEYBOARD: &str = fixture!("p7-landscape-04-landscape-after-tree.txt");

/// The twelve `UIReturnKeyType` captures (p4b) and the German ones (p10): the fixture, the
/// key's label as compacted, the type its identifier names, whether its Return is of
/// unknown effect.
const RETURN_KEYS: &[(&str, &str, Option<&str>, bool)] = &[
    (
        fixture!("p4b-returnkeys-01-return-default-tree.txt"),
        "return",
        Some("return"),
        false,
    ),
    (
        fixture!("p4b-returnkeys-02-return-go-tree.txt"),
        "go",
        Some("go"),
        false,
    ),
    (
        fixture!("p4b-returnkeys-03-return-google-tree.txt"),
        "search",
        Some("google"),
        false,
    ),
    (
        fixture!("p4b-returnkeys-04-return-join-tree.txt"),
        "join",
        Some("join"),
        false,
    ),
    (
        fixture!("p4b-returnkeys-05-return-next-tree.txt"),
        "next",
        Some("next"),
        false,
    ),
    (
        fixture!("p4b-returnkeys-06-return-route-tree.txt"),
        "route",
        Some("route"),
        false,
    ),
    (
        fixture!("p4b-returnkeys-07-return-search-tree.txt"),
        "search",
        Some("search"),
        false,
    ),
    (
        fixture!("p4b-returnkeys-08-return-send-tree.txt"),
        "send",
        Some("send"),
        false,
    ),
    (
        fixture!("p4b-returnkeys-09-return-send-runner-tree.txt"),
        "send",
        Some("send"),
        false,
    ),
    (
        fixture!("p4b-returnkeys-10-return-yahoo-tree.txt"),
        "search",
        Some("yahoo"),
        false,
    ),
    (
        fixture!("p4b-returnkeys-11-return-done-tree.txt"),
        "done",
        Some("done"),
        false,
    ),
    (
        fixture!("p4b-returnkeys-12-return-emergencyCall-tree.txt"),
        "Emergency Call",
        Some("emergency call"),
        false,
    ),
    (
        fixture!("p4b-returnkeys-13-return-continue-tree.txt"),
        "continue",
        None,
        true,
    ),
    (GERMAN_RETURN, "Return", Some("return"), false),
    (GERMAN_GO, "Öffnen", Some("go"), false),
];

/// The typed tree of a capture (the core's input); the raw snapshot after it is cut off.
fn parse(text: &str) -> Screen {
    let typed = text
        .split("\n== raw XCUITest snapshot")
        .next()
        .unwrap_or(text);
    ios::parse_screen(typed).unwrap()
}

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

/// The gate's decision for `action` on `s` (as the session sees it), nothing read, the
/// owner's words `owner`.
fn decide_owned(policy: &Policy, action: &Action, s: &Screen, owner: &str) -> Decision {
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
        &[owner],
        false,
        false,
        Some(share),
    )
}

fn decide(policy: &Policy, action: &Action, s: &Screen) -> Decision {
    decide_owned(policy, action, s, "help me with this screen")
}

fn default_policy() -> Policy {
    Policy {
        first_use_approval: false,
        ..Policy::default()
    }
}

/// An owner who relaxed every category.
fn relaxed() -> Policy {
    Policy {
        confirm: Vec::new(),
        first_use_approval: false,
        ..Policy::default()
    }
}

fn asked(d: &Decision) -> Option<RiskCategory> {
    match d {
        Decision::Confirm(r) => Some(r.category),
        _ => None,
    }
}

fn summary(d: &Decision) -> String {
    match d {
        Decision::Confirm(r) => r.summary.clone(),
        other => format!("{other:?}"),
    }
}

fn element<'a>(c: &'a CompactScreen, label: &str) -> &'a Element {
    c.elements
        .iter()
        .find(|e| e.label == label)
        .or_else(|| {
            c.elements
                .iter()
                .find(|e| e.placeholder.as_deref() == Some(label))
        })
        .unwrap_or_else(|| panic!("no element {label:?}: {:#?}", c.elements))
}

fn index(s: &Screen, label: &str) -> u32 {
    element(&compact_view(&seen(s)).screen, label).index
}

fn tap(s: &Screen, label: &str) -> Action {
    Action::Tap {
        element: index(s, label),
    }
}

fn edit(n: &mut UiNode, f: &mut impl FnMut(&mut UiNode)) {
    f(n);
    for c in &mut n.children {
        edit(c, f);
    }
}

fn walk<'a>(n: &'a UiNode, out: &mut Vec<&'a UiNode>) {
    out.push(n);
    for c in &n.children {
        walk(c, out);
    }
}

fn mentions(s: &Screen, text: &str) -> bool {
    let mut all = Vec::new();
    walk(&s.root, &mut all);
    all.iter().any(|n| n.label.as_deref() == Some(text))
}

fn flows(r: &Run) -> Vec<String> {
    r.confirms
        .iter()
        .filter(|c| c.summary.contains(FLOW))
        .map(|c| c.summary.clone())
        .collect()
}

fn jarvis_config() -> jarvis_proto::SessionConfig {
    let mut c = config();
    c.known_apps.push(AppInfo::new("dev.jarvis.app", "Jarvis"));
    c
}

// ---- F1: a long sideways swipe on a list row deletes it -------------------------------------

/// The real full-width swipe (safe-area edge to edge) deleted "Swipe row 2" with no tap:
/// the tree after it holds rows 1 and 3 only; before it, all three.
#[test]
fn f1_the_full_width_swipe_deleted_the_row() {
    let before = parse(P4D_BEFORE_FULL);
    let after = parse(P4D_AFTER_FULL);
    for row in ["Swipe row 1", "Swipe row 2", "Swipe row 3"] {
        assert!(mentions(&before, row), "{row}");
    }
    assert!(mentions(&after, "Swipe row 1") && mentions(&after, "Swipe row 3"));
    assert!(!mentions(&after, "Swipe row 2"));
}

/// The core's own row swipe on the real row is the measured reveal path — (340, 234.3) →
/// (100, 234.3) in 200 ms — and no sideways scroll or swipe Jarvis computes on the real
/// screens, with or without an element, portrait or landscape, travels further than that
/// share of the width (fails on the old core: the element-less sideways drag travelled
/// 242.9 pt of 440, 60 % of the safe area, beyond the measured 240).
#[test]
fn f1_no_sideways_drag_is_longer_than_the_measured_reveal() {
    let s = parse(P4D_BEFORE);
    let c = compact_view(&s).screen;
    let row = element(&c, "Swipe row 1");
    let g = drag_gesture(GestureKind::Swipe, Direction::Left, Some(row.frame), &c).unwrap();
    assert!(
        (g.from.x - 340.0).abs() < 1e-6 && (g.to.x - 100.0).abs() < 1e-6,
        "{g:?}"
    );
    assert!(
        (g.from.y - 234.3).abs() < 1e-6 && g.from.y == g.to.y,
        "{g:?}"
    );
    assert_eq!(g.duration_ms, 200);
    for text in [P4D_BEFORE, P4D_REVEALED, LANDSCAPE, CONTACT_PICKER] {
        let c = compact_view(&parse(text)).screen;
        let cap = c.size.width * MAX_SIDEWAYS_SHARE + 1e-9;
        let frames = std::iter::once(None).chain(c.elements.iter().map(|e| Some(e.frame)));
        for frame in frames {
            for kind in [GestureKind::Swipe, GestureKind::Scroll] {
                for d in [Direction::Left, Direction::Right] {
                    if let Ok(g) = drag_gesture(kind, d, frame, &c) {
                        assert!(
                            (g.to.x - g.from.x).abs() <= cap,
                            "{kind:?} {d:?} {frame:?}: {g:?}"
                        );
                        assert!(g.duration_ms >= 200);
                    }
                }
            }
        }
    }
    // The element-less one (the one the old core made longer).
    let g = drag_gesture(GestureKind::Swipe, Direction::Left, None, &c).unwrap();
    assert!((g.from.x - g.to.x - 240.0).abs() < 1e-6, "{g:?}");
}

/// A sideways drag whose path crosses a row of the real list is a row swipe whatever the
/// verb: the segmented control sits in a row of the list, and a swipe or a sideways scroll
/// on its "Two" asks as `Delete`, with the row-swipe note (fails on the old core: "Two" has
/// siblings beside it, so it was no list row, and its row's `Cell` is not listed — allowed).
#[test]
fn f1_a_sideways_drag_across_a_real_row_asks_whatever_the_verb() {
    let s = parse(P4D_BEFORE);
    let two = index(&s, "Two");
    for a in [
        Action::Swipe {
            direction: Direction::Left,
            element: Some(two),
        },
        Action::Swipe {
            direction: Direction::Right,
            element: Some(two),
        },
        Action::Scroll {
            direction: Direction::Right,
            element: Some(two),
        },
        Action::Scroll {
            direction: Direction::Left,
            element: Some(two),
        },
    ] {
        let d = decide(&default_policy(), &a, &s);
        assert_eq!(asked(&d), Some(RiskCategory::Delete), "{a:?}: {d:?}");
        assert!(summary(&d).contains(ROW_SWIPE_NOTE), "{}", summary(&d));
    }
    // The row itself (a listed cell) asked before and still does, now with the note.
    let row = index(&s, "Swipe row 2");
    let d = decide(
        &default_policy(),
        &Action::Swipe {
            direction: Direction::Left,
            element: Some(row),
        },
        &s,
    );
    assert_eq!(asked(&d), Some(RiskCategory::Delete));
    assert!(summary(&d).contains(ROW_SWIPE_NOTE), "{}", summary(&d));
    // A vertical scroll of the list stays a plain scroll.
    let d = decide(
        &default_policy(),
        &Action::Scroll {
            direction: Direction::Down,
            element: None,
        },
        &s,
    );
    assert_eq!(asked(&d), None, "{d:?}");
}

/// A session swipes the real row only after asking, and then sends the measured path.
#[test]
fn f1_a_session_asks_before_the_row_swipe_and_sends_the_measured_path() {
    let s = parse(P4D_BEFORE);
    let row = index(&s, "Swipe row 1");
    let r = run(
        jarvis_config(),
        "Show the actions of swipe row 1",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            &format!(r#"{{"action":"swipe","direction":"left","element":{row}}}"#),
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[true]),
    );
    let asked: Vec<&str> = r.confirms.iter().map(|c| c.summary.as_str()).collect();
    assert!(
        r.confirms
            .iter()
            .any(|c| c.category == RiskCategory::Delete && c.summary.contains(ROW_SWIPE_NOTE)),
        "{asked:#?}"
    );
    let swipes: Vec<&RunnerCommand> = r
        .acts
        .iter()
        .filter(|a| matches!(a, RunnerCommand::Swipe { .. }))
        .collect();
    match swipes.as_slice() {
        [RunnerCommand::Swipe {
            from,
            to,
            duration_ms,
            ..
        }] => {
            // Runs 146 and 149 (P13): across the 400-pt row the travel is 0.45 of the row
            // (180 pt, was the screen cap's 240), its start kept.
            assert!((from.x - 340.0).abs() < 1e-6 && (to.x - 160.0).abs() < 1e-6);
            assert_eq!(*duration_ms, 200);
        }
        other => panic!("{other:?}"),
    }
}

/// After the reveal the revealed actions are ordinary controls: "Delete" (`id trash`) asks
/// as `Delete`, "Flag" does not (confirmed: unchanged).
#[test]
fn f1_the_revealed_delete_asks_as_delete() {
    let s = parse(P4D_REVEALED);
    let d = decide(&default_policy(), &tap(&s, "Delete"), &s);
    assert_eq!(asked(&d), Some(RiskCategory::Delete), "{d:?}");
    assert_eq!(
        asked(&decide(&default_policy(), &tap(&s, "Flag"), &s)),
        None
    );
}

/// The real list with its rows retyped the way other UI stacks report them: the
/// `CollectionView` a `ScrollView`, each `Cell` an `Other` one wrapper down (React Native's
/// `ScrollView > Other > Other` rows, a SwiftUI `LazyVStack`), and "No action" stretched to
/// lie on the element-less drag's path (y 468.4).
fn retyped_rows() -> Screen {
    let mut s = parse(P4D_BEFORE);
    edit(&mut s.root, &mut |n| {
        if n.role == Role::CollectionView {
            n.role = Role::ScrollView;
            let rows = std::mem::take(&mut n.children);
            let size = n.frame;
            n.children = vec![UiNode::new(Role::Other("Other".into()), size).with_children(rows)];
        }
        if n.role == Role::Cell {
            n.role = Role::Other("Other".into());
            if (n.frame.y - 399.3).abs() < 0.01 {
                n.frame.height = 80.0;
            }
        }
    });
    s
}

/// Review of round 3 (M3): a row is also any node of row height spanning most of its
/// nearest scrollable ancestor at any depth: on the retyped list a swipe on a row's text
/// asks as `Delete`, and an element-less sideways scroll across a row is a *full* input
/// (never covered by a scroll grant) and asks (fails on 9ae7aeb: no `Cell`, no direct child
/// of a table: allowed, and the scroll was of the scroll kind).
#[test]
fn f1_rows_of_any_ui_stack_are_rows() {
    let s = retyped_rows();
    let v = compact_view(&s);
    let row = v
        .screen
        .elements
        .iter()
        .find(|e| e.label == "Swipe row 2")
        .unwrap();
    assert_ne!(row.role, Role::Cell);
    let d = decide(
        &default_policy(),
        &Action::Swipe {
            direction: Direction::Left,
            element: Some(row.index),
        },
        &s,
    );
    assert_eq!(asked(&d), Some(RiskCategory::Delete), "{d:?}");
    let scroll = Action::Scroll {
        direction: Direction::Right,
        element: None,
    };
    assert_eq!(InputKind::of(&scroll, &v.screen), InputKind::Scroll);
    assert_eq!(
        InputKind::of_in(&scroll, &v.screen, Some(&tree(&v))),
        InputKind::Full
    );
    assert_eq!(
        asked(&decide(&default_policy(), &scroll, &s)),
        Some(RiskCategory::Delete)
    );
    // A vertical scroll stays a scroll.
    let down = Action::Scroll {
        direction: Direction::Down,
        element: None,
    };
    assert_eq!(
        InputKind::of_in(&down, &v.screen, Some(&tree(&v))),
        InputKind::of(&down, &v.screen)
    );
}

/// Review of round 3 (L1): the row's full-swipe action is unknown, so a row swipe asks
/// phone only, also for an owner who relaxed `Delete` (fails on 9ae7aeb: allowed for them).
#[test]
fn f1_a_row_swipe_asks_an_owner_who_relaxed_delete() {
    let s = parse(P4D_BEFORE);
    let row = index(&s, "Swipe row 1");
    let policy = Policy {
        confirm: default_policy()
            .confirm
            .into_iter()
            .filter(|c| *c != RiskCategory::Delete)
            .collect(),
        ..default_policy()
    };
    let d = decide(
        &policy,
        &Action::Swipe {
            direction: Direction::Left,
            element: Some(row),
        },
        &s,
    );
    assert!(matches!(&d, Decision::Confirm(r) if r.phone_only), "{d:?}");
}

// ---- F2: out-of-process pickers -------------------------------------------------------------

const PICKERS: &[(&str, &str, PickerKind)] = &[
    (PHOTO_PICKER, "photo", PickerKind::Photos),
    (PHOTO_PICKER_RUNNER, "photo (runner)", PickerKind::Photos),
    (CONTACT_PICKER, "contact", PickerKind::Contacts),
    (
        CONTACT_PICKER_RUNNER,
        "contact (runner)",
        PickerKind::Contacts,
    ),
    (FILE_PICKER, "file", PickerKind::Files),
    (FILE_PICKER_RUNNER, "file (runner)", PickerKind::Files),
];

fn name(b: &str) -> Option<String> {
    (b == "dev.jarvis.app").then(|| "Jarvis".to_owned())
}

/// Each real picker's screen is a place of its own over the app, by the OS's identifiers
/// (fails on the old core: it was the app's own place, `App(dev.jarvis.app)`).
#[test]
fn f2_each_real_picker_is_a_place_of_its_own() {
    for (text, what, kind) in PICKERS {
        let s = parse(text);
        let v = compact_view(&s);
        let p = places_of(&s, &v);
        let want = FlowContext::SystemPicker {
            app: "dev.jarvis.app".into(),
            kind: *kind,
        };
        assert_eq!(p.base(), &want, "{what}");
        // Review pass 28 (H1): read from the picker and from the app it is shown over.
        assert_eq!(
            p.read_places(),
            vec![want.clone(), FlowContext::app("dev.jarvis.app")],
            "{what}"
        );
        assert_eq!(p.picker(), Some(*kind), "{what}");
        assert!(p.shows_presentation(), "{what}");
        // Review M2 (c): the app leads the name.
        assert!(
            want.display(name).starts_with("Jarvis (showing the ")
                && want.display(name).ends_with(" picker)"),
            "{}",
            want.display(name)
        );
    }
    // The app's own screen (the modal probe's base) is the app.
    let s = parse(MODAL_BASE);
    let v = compact_view(&s);
    let p = places_of(&s, &v);
    assert_eq!(p.base(), &FlowContext::app("dev.jarvis.app"));
    assert_eq!(p.picker(), None);
}

/// Review of round 3 (M2): the picker is a source distinction only — every input on a
/// picker screen (a row, "Cancel", a plain scroll, typing, a look, a wait) is an input into
/// the picker's place **and** the app it is shown over, and the app stays on the screen's
/// places (fails on 9ae7aeb: "Cancel", the scroll and the look went to the picker alone).
#[test]
fn f2_every_input_on_a_picker_reaches_the_app() {
    let s = parse(CONTACT_PICKER);
    let v = compact_view(&s);
    let p = places_of(&s, &v);
    let picker = p.base().clone();
    let app = FlowContext::app("dev.jarvis.app");
    let both = vec![picker.clone(), app.clone()];
    let dest = |a: &Action| p.destinations(a, &v.screen, Some(&tree(&v)));
    let search = Action::Type {
        element: Some(index(&s, "Search")),
        text: "Kate".into(),
        submit: false,
    };
    for a in [
        tap(&s, "John Appleseed"),
        tap(&s, "Cancel"),
        Action::Scroll {
            direction: Direction::Down,
            element: None,
        },
        search,
        Action::Look,
        Action::Wait { ms: 1000 },
    ] {
        assert_eq!(dest(&a), both, "{a:?}");
    }
    assert_eq!(p.every(), both);
    assert!(p.observers(&[picker]).contains(&app));
    // A picker over a listed browser also reaches its pages (a file input's picker hands
    // the choice to the site, which is not on screen).
    let mut safari = parse(FILE_PICKER);
    safari.app_bundle_id = "com.apple.mobilesafari".into();
    let v = compact_view(&safari);
    let p = places_of(&safari, &v);
    assert_eq!(
        p.destinations(&tap(&safari, "Cancel"), &v.screen, Some(&tree(&v))),
        vec![
            FlowContext::SystemPicker {
                app: "com.apple.mobilesafari".into(),
                kind: PickerKind::Files
            },
            FlowContext::app("com.apple.mobilesafari"),
            FlowContext::SiteUnreadable("com.apple.mobilesafari".into()),
        ]
    );
}

/// The decisions and flow questions a session makes for `script` on `screen` (the app's
/// display name "Acme").
fn session_on(screen: Screen, script: &[&str]) -> Run {
    let mut c = config();
    c.known_apps.push(AppInfo::new("com.example.acme", "Acme"));
    run(
        c,
        "Help me with this screen",
        MemoryStore::new(),
        Phone::still(screen),
        Script::model(script).decisions(&[true; 12]),
    )
}

/// Review of round 3 (M2): a forged picker gains nothing. A third-party app adds a node
/// with the contact picker's identifier and a backdrop-sized node to its own screen: shown,
/// the screen is read as a picker over the app, and every step asks at least what it asks
/// on the same tree without the forged nodes (each question a superset, nothing more sent),
/// naming the app first; hidden, the identifier makes no picker at all (fails on 9ae7aeb: a
/// hidden identifier made a picker, and its "Cancel" and scrolls were local).
#[test]
fn f2_a_forged_picker_gains_nothing() {
    let mut plain = parse(MODAL_BASE);
    plain.app_bundle_id = "com.example.acme".into();
    plain.app_name = Some("Acme".into());
    plain.root.label = Some("Acme".into());
    let forge = |hidden: bool| {
        let mut s = plain.clone();
        let size = jarvis_proto::Rect::new(0.0, 0.0, 440.0, 956.0);
        let mut id = UiNode::new(Role::Other("Other".into()), size);
        id.identifier = Some("ContactsListView".into());
        id.visible = !hidden;
        let back = UiNode::new(
            Role::Other("Other".into()),
            jarvis_proto::Rect::new(-440.0, -956.0, 1320.0, 2868.0),
        );
        s.root.children[0].children.insert(0, id);
        s.root.children[0].children.insert(0, back);
        s
    };
    let hidden = forge(true);
    let v = compact_view(&hidden);
    assert_eq!(places_of(&hidden, &v).picker(), None);
    let forged = forge(false);
    let v = compact_view(&forged);
    let p = places_of(&forged, &v);
    assert_eq!(p.picker(), Some(PickerKind::Contacts));
    assert!(p.every().contains(&FlowContext::app("com.example.acme")));
    let c = compact_view(&plain).screen;
    let cancel = c
        .elements
        .iter()
        .find(|e| e.role == Role::Button)
        .map(|e| e.index)
        .unwrap();
    let script = [
        r#"{"action":"look"}"#,
        r#"{"action":"scroll","direction":"down"}"#,
        &format!(r#"{{"action":"tap","element":{cancel}}}"#),
        r#"{"action":"done","summary":"ok"}"#,
    ];
    let a = session_on(plain.clone(), &script);
    let b = session_on(forged, &script);
    assert!(b.confirms.len() >= a.confirms.len(), "{:#?}", b.confirms);
    assert!(b.acts.len() <= a.acts.len(), "{:?} vs {:?}", b.acts, a.acts);
    assert!(
        b.confirms.iter().all(|q| !q.summary.contains("picker")
            || q.summary.contains("Acme (showing the contact picker)")),
        "{:#?}",
        b.confirms
    );
}

/// Review of round 3 (M2): the contact picker inside Jarvis's own app while it is not
/// driven (`agent_bundle_ids`): the model gets the stub, nothing of the picker reaches a
/// prompt, and the session asks no flow question for it.
#[test]
fn f2_a_picker_in_jarvis_own_app_stays_hidden() {
    let mut c = jarvis_config();
    c.agent_bundle_ids = vec!["dev.jarvis.app".into()];
    let r = run(
        c,
        "Look at the screen",
        MemoryStore::new(),
        Phone::still(parse(CONTACT_PICKER)),
        Script::model(&[
            r#"{"action":"look"}"#,
            r#"{"action":"done","summary":"ok"}"#,
        ]),
    );
    let all = r.all_prompt_text();
    assert!(
        !all.contains("John Appleseed") && !all.contains("Kate Bell"),
        "{all}"
    );
    assert!(flows(&r).is_empty(), "{:#?}", r.confirms);
}

/// A session on the real contact picker: tapping a contact asks the flow question naming
/// the picker as what was read (the name is handed to the app); review M2: "Cancel" asks
/// too (nothing on a picker screen is local to it).
#[test]
fn f2_a_session_asks_before_handing_a_contact_to_the_app() {
    let s = parse(CONTACT_PICKER);
    let john = index(&s, "John Appleseed");
    let cancel = index(&s, "Cancel");
    let r = run(
        jarvis_config(),
        "Pick a contact",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            &format!(r#"{{"action":"tap","element":{john}}}"#),
            &format!(r#"{{"action":"tap","element":{cancel}}}"#),
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[true; 6]),
    );
    let fq = flows(&r);
    assert_eq!(fq.len(), 2, "{fq:#?}");
    assert!(fq[0].starts_with("Tap \"John Appleseed\""), "{}", fq[0]);
    assert!(fq[1].starts_with("Tap \"Cancel\""), "{}", fq[1]);
    for q in &fq {
        assert!(q.contains("Jarvis (showing the contact picker)"), "{q}");
    }
}

/// The picker without its own identifiers (another of its screens) while its sheet's
/// backdrop is still there: still the picker (sticky); a contact's name read there and
/// typed into the app's own field afterwards is asked as a flow from the picker.
#[test]
fn f2_the_picker_is_sticky_and_a_read_name_typed_into_the_app_asks() {
    let mut bare = parse(CONTACT_PICKER);
    edit(&mut bare.root, &mut |n| {
        if n.identifier.as_deref() == Some("ContactsListView") {
            n.identifier = None;
        }
    });
    // Unit: without the identifier alone, the app's own place (the session keeps it).
    let v = compact_view(&bare);
    assert_eq!(
        places_of(&bare, &v).base(),
        &FlowContext::app("dev.jarvis.app")
    );
    let picker = parse(CONTACT_PICKER);
    let close = index(&picker, "Close");
    let mut field_screen = parse(ALERT_FIELD);
    field_screen.app_bundle_id = "dev.jarvis.app".into();
    let kate = index(&bare, "Kate Bell");
    let field = index(&field_screen, "Probe alert field");
    let mut step = 0;
    let (b, f) = (bare.clone(), field_screen.clone());
    let phone = Phone::new(picker, move |cmd, _| {
        if !matches!(cmd, RunnerCommand::Tap { .. }) {
            return Transition::Stay;
        }
        step += 1;
        match step {
            1 => Transition::Go(b.clone()),
            2 => Transition::Go(f.clone()),
            _ => Transition::Stay,
        }
    });
    let r = run(
        jarvis_config(),
        "Pick Kate Bell and put her name in the field",
        MemoryStore::new(),
        phone,
        Script::model(&[
            &format!(r#"{{"action":"tap","element":{close}}}"#),
            &format!(r#"{{"action":"tap","element":{kate}}}"#),
            &format!(r#"{{"action":"type","element":{field},"text":"Kate Bell"}}"#),
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[true; 8]),
    );
    let fq = flows(&r);
    assert!(
        fq.iter().any(|q| q.starts_with("Tap \"Kate Bell\"")
            && q.contains("Jarvis (showing the contact picker)")),
        "{fq:#?}"
    );
    assert!(
        fq.iter()
            .any(|q| q.starts_with("Type") && q.contains("Jarvis (showing the contact picker)")),
        "{fq:#?}"
    );
}

// ---- F3: the Return key's real identifiers --------------------------------------------------

/// Every captured Return key: its label, its type by the real identifier (`Join:`, `Next:`,
/// `Emergency call`), and `.continue`'s key (no identifier) of unknown effect (fails on the
/// old core: `Join:`, `Next:` and `Emergency call` named no type, and nothing marked a key
/// of unknown type).
#[test]
fn f3_every_captured_return_key_is_read_by_its_real_identifier() {
    for (text, label, kind, unknown) in RETURN_KEYS {
        let c = compact_view(&parse(text)).screen;
        assert_eq!(c.cues.return_key.as_deref(), Some(*label), "{label}");
        assert_eq!(c.cues.return_key_type.as_deref(), *kind, "{label}");
        assert_eq!(c.cues.return_key_unknown, *unknown, "{label}");
    }
}

/// The focused field of a capture and Return pressed in it after typing `text`.
fn return_in(s: &Screen, text: &str) -> Action {
    let c = compact_view(s).screen;
    let field = c
        .elements
        .iter()
        .find(|e| e.focused && e.role.is_text_input())
        .unwrap_or_else(|| panic!("no focused field: {:#?}", c.elements));
    Action::Type {
        element: Some(field.index),
        text: text.into(),
        submit: true,
    }
}

/// The real `.continue` capture with its focused field turned into a search field (whose
/// Return keeps the search leniency for the typed text and the screen).
fn as_search(s: &Screen) -> Screen {
    let mut s = s.clone();
    edit(&mut s.root, &mut |n| {
        if n.focused && n.role.is_text_input() {
            n.role = Role::SearchField;
        }
    });
    s
}

/// Return on the real `.continue` key (no identifier) is of unknown effect: it asks, phone
/// only, at least `Unverified`; so does a key at the keyboard's corner whose label no table
/// knows ("Weiter", found by its place only to be quoted). In a search field, whose Return
/// is otherwise lenient, it asks as `Unverified` (fails on the old core: "continue" there
/// took the empty screen's context and was allowed; "Weiter" was no Return key at all).
/// Typed keys keep their meaning: `Emergency call` is a call, `Send` a send.
#[test]
fn f3_a_return_key_of_unknown_type_asks() {
    let owner = "type hello";
    let at_least_unverified = |d: &Decision| {
        let c = asked(d).unwrap_or_else(|| panic!("not asked: {d:?}"));
        assert_eq!(max_risk(c, RiskCategory::Unverified), c, "{d:?}");
        assert!(matches!(d, Decision::Confirm(r) if r.phone_only), "{d:?}");
    };
    let cont = parse(RETURN_KEYS[12].0);
    at_least_unverified(&decide_owned(
        &default_policy(),
        &return_in(&cont, "hello"),
        &cont,
        owner,
    ));
    let search = as_search(&cont);
    let d = decide_owned(
        &default_policy(),
        &return_in(&search, "hello"),
        &search,
        owner,
    );
    assert_eq!(asked(&d), Some(RiskCategory::Unverified), "{d:?}");
    // A localized key at the corner with neither cue.
    let mut weiter = cont.clone();
    edit(&mut weiter.root, &mut |n| {
        if n.label.as_deref() == Some("continue") {
            n.label = Some("Weiter".into());
        }
    });
    let c = compact_view(&weiter).screen;
    assert_eq!(c.cues.return_key.as_deref(), Some("Weiter"));
    assert!(c.cues.return_key_unknown);
    let weiter = as_search(&weiter);
    let d = decide_owned(
        &default_policy(),
        &return_in(&weiter, "hello"),
        &weiter,
        owner,
    );
    assert_eq!(asked(&d), Some(RiskCategory::Unverified), "{d:?}");
    // A typed plain key in the same search field asks nothing.
    let plain = as_search(&parse(RETURN_KEYS[0].0));
    let d = decide_owned(
        &default_policy(),
        &return_in(&plain, "hello"),
        &plain,
        owner,
    );
    assert_eq!(asked(&d), None, "{d:?}");
    // Typed keys.
    let emergency = parse(RETURN_KEYS[11].0);
    let d = decide_owned(
        &default_policy(),
        &return_in(&emergency, "hello"),
        &emergency,
        owner,
    );
    // Review M1: the call raises, never replaces: the field's presumed send stays (at least
    // `SendMessage`), the note names the call, phone only.
    let c = asked(&d).unwrap();
    assert_eq!(max_risk(c, RiskCategory::SendMessage), c, "{d:?}");
    assert!(summary(&d).contains(EMERGENCY_CALL_NOTE), "{}", summary(&d));
    assert!(matches!(&d, Decision::Confirm(r) if r.phone_only));
    let send = parse(RETURN_KEYS[7].0);
    let d = decide_owned(&default_policy(), &return_in(&send, "hello"), &send, owner);
    assert_eq!(asked(&d), Some(RiskCategory::SendMessage), "{d:?}");
    // The German go key is typed, not unknown.
    let go = parse(GERMAN_GO);
    assert!(!compact_view(&go).screen.cues.return_key_unknown);
}

/// Review of round 3 (M1): an owner who denies sending is refused a Return on the
/// emergency-call key of a field that sends (fails on 9ae7aeb: the category was replaced by
/// `Call`, so it asked instead of refusing); an owner who relaxed everything is still asked.
#[test]
fn f3_the_emergency_call_key_never_hides_a_send() {
    let emergency = parse(RETURN_KEYS[11].0);
    let a = return_in(&emergency, "hello");
    let deny = Policy {
        deny_categories: vec![RiskCategory::SendMessage],
        ..default_policy()
    };
    let d = decide_owned(&deny, &a, &emergency, "type hello");
    assert!(matches!(d, Decision::Deny(_)), "{d:?}");
    let d = decide_owned(&relaxed(), &a, &emergency, "type hello");
    assert!(asked(&d).is_some(), "{d:?}");
}

// ---- F4: the labels the OS gives icon-only controls -----------------------------------------

/// Every button of the real symbol screens (SwiftUI with identifiers, UIKit without, the
/// eight toolbar system items; en-US and the en-GB labels of the German run): the category
/// a tap on it asks under (`None`: not asked), by the default policy. Printed for the
/// record.
fn symbol_table(text: &str, strip: bool) -> Vec<(String, Option<String>, Option<RiskCategory>)> {
    let mut s = parse(text);
    let v = compact_view(&s);
    let mut all = Vec::new();
    walk(&s.root, &mut all);
    let ids: Vec<Option<String>> = v
        .screen
        .elements
        .iter()
        .map(|e| {
            all.get(v.sources[(e.index - 1) as usize] as usize)
                .and_then(|n| n.identifier.clone())
        })
        .collect();
    if strip {
        edit(&mut s.root, &mut |n| {
            if matches!(n.role, Role::Button | Role::Image) {
                n.identifier = None;
            }
        });
    }
    let c = compact_view(&s).screen;
    c.elements
        .iter()
        .zip(ids)
        .filter(|(e, _)| e.role == Role::Button)
        .map(|(e, id)| {
            let d = decide(&default_policy(), &Action::Tap { element: e.index }, &s);
            (e.label.clone(), id, asked(&d))
        })
        .collect()
}

/// By label alone (every identifier removed: what a UIKit button gives), every captured
/// control whose symbol `SF_SYMBOLS` names a risk is at least that risk, and the UIKit and
/// en-GB descriptions are too (fails on the old core: "shopping cart", "card", "shopping
/// bag", "Arrow Up Circle" were allowed, and "Bin", "Shopping Trolley", "Shopping Cart",
/// "Card", "Shopping Bag" only asked through their identifier).
#[test]
fn f4_every_risky_symbols_label_is_at_least_its_risk_alone() {
    for (text, what) in [(SYMBOLS, "p4a"), (SYMBOLS_EN_GB, "p10 en-GB")] {
        for (label, id, cat) in symbol_table(text, false) {
            println!(
                "{what} | {label:?} | id {} | {cat:?}",
                id.as_deref().unwrap_or("-")
            );
        }
        // Each SwiftUI button's symbol risk by its identifier, then its label alone.
        let s = parse(text);
        let mut all = Vec::new();
        walk(&s.root, &mut all);
        let risky: Vec<(String, RiskCategory)> = all
            .iter()
            .filter(|n| n.role == Role::Button)
            .filter_map(|n| Some((n.label.clone()?, identifier_symbol_risk(n)?)))
            .collect();
        assert!(risky.len() >= 10, "{risky:?}");
        let alone = symbol_table(text, true);
        for (label, want) in &risky {
            let got = alone
                .iter()
                .find(|(l, _, _)| l == label)
                .and_then(|(_, _, c)| *c)
                .unwrap_or(RiskCategory::None);
            assert_eq!(
                max_risk(got, *want),
                got,
                "{what}: {label:?} alone is {got:?}, its symbol {want:?}"
            );
        }
    }
    // The UIKit buttons (no identifier in the capture) by their lowercase descriptions.
    let uikit = symbol_table(SYMBOLS, false);
    for (label, want) in [
        ("trash", RiskCategory::Delete),
        ("send", RiskCategory::SendMessage),
        ("shopping cart", RiskCategory::Purchase),
        ("card", RiskCategory::Purchase),
        ("Call", RiskCategory::Call),
        ("Share", RiskCategory::PublicPost),
        ("Arrow Up Circle", RiskCategory::SendMessage),
        ("remove", RiskCategory::Delete),
        ("shopping bag", RiskCategory::Purchase),
        ("Delete", RiskCategory::Delete),
    ] {
        let got = uikit
            .iter()
            .filter(|(l, id, _)| l == label && id.is_none())
            .map(|(_, _, c)| c.unwrap_or(RiskCategory::None))
            .collect::<Vec<_>>();
        assert!(!got.is_empty(), "{label}");
        for g in got {
            assert_eq!(max_risk(g, want), g, "{label:?}: {g:?} < {want:?}");
        }
    }
    // The en-GB forms by label alone.
    let gb = symbol_table(SYMBOLS_EN_GB, true);
    for (label, want) in [
        ("Bin", RiskCategory::Delete),
        ("Shopping Trolley", RiskCategory::Purchase),
    ] {
        let got = gb
            .iter()
            .find(|(l, _, _)| l == label)
            .and_then(|(_, _, c)| *c)
            .unwrap_or(RiskCategory::None);
        assert_eq!(max_risk(got, want), got, "{label:?}");
    }
}

/// The whole-label entries never match inside a longer label.
#[test]
fn f4_the_whole_labels_do_not_over_match() {
    let mut s = parse(SYMBOLS);
    edit(&mut s.root, &mut |n| match n.label.as_deref() {
        Some("card") => n.label = Some("Card details".into()),
        Some("shopping bag") => n.label = Some("Shopping bag size guide".into()),
        _ => {}
    });
    let c = compact_view(&s).screen;
    for l in ["Card details", "Shopping bag size guide"] {
        let e = element(&c, l);
        let got = jarvis_safety::classify(&Action::Tap { element: e.index }, &c, None);
        assert_ne!(got, RiskCategory::Purchase, "{l}");
    }
}

// ---- F5: a destructive row is a `Cell` ------------------------------------------------------

/// Contacts' "Delete Contact" is a `Cell`: a tap on it asks as `Delete`, and so it would as
/// a static text or an `Other` (the label decides, not the role). Confirmed: unchanged.
#[test]
fn f5_the_delete_contact_cell_asks_as_delete_whatever_its_role() {
    let s = parse(CONTACT_CARD);
    assert_eq!(
        element(&compact_view(&s).screen, "Delete Contact").role,
        Role::Cell
    );
    for role in [
        None,
        Some(Role::StaticText),
        Some(Role::Other("Other".into())),
    ] {
        let mut s = s.clone();
        if let Some(role) = &role {
            edit(&mut s.root, &mut |n| {
                if n.label.as_deref() == Some("Delete Contact") {
                    n.role = role.clone();
                }
            });
        }
        let d = decide(&default_policy(), &tap(&s, "Delete Contact"), &s);
        assert_eq!(asked(&d), Some(RiskCategory::Delete), "{role:?}: {d:?}");
    }
}

// ---- F6: Safari's JavaScript dialogs and HTTP sign-in ---------------------------------------

/// HTTP Basic auth: the dialog is in the page's web view, the address field reads "Sign in
/// to website" (no host), so every input there goes to the page with no readable address —
/// never to the host the dialog's text names. A user name typed beside the password asks as
/// a credential, phone only, even for an owner who relaxed everything; the password is
/// refused; "Sign In" asks as an account action; "Cancel" is a plain dismiss (fails on the
/// old core: the user name asked as a send and was allowed relaxed; "Sign In" was allowed).
#[test]
fn f6_the_http_sign_in_dialog() {
    let s = parse(BASIC_AUTH);
    let v = compact_view(&s);
    let p = places_of(&s, &v);
    let page = FlowContext::SiteUnreadable("com.apple.mobilesafari".into());
    let user = Action::Type {
        element: Some(index(&s, "User Name")),
        text: "ada".into(),
        submit: false,
    };
    let password = Action::Type {
        element: Some(index(&s, "Password")),
        text: "hunter2".into(),
        submit: false,
    };
    for a in [&user, &password, &tap(&s, "Sign In"), &tap(&s, "Cancel")] {
        assert_eq!(
            p.destinations(a, &v.screen, Some(&tree(&v))),
            vec![page.clone()]
        );
    }
    assert_eq!(
        page.display(|_| Some("Safari".into())),
        "pages in Safari with no readable address"
    );
    for policy in [default_policy(), relaxed()] {
        let d = decide_owned(&policy, &user, &s, "sign in as ada");
        assert_eq!(asked(&d), Some(RiskCategory::Credential), "{d:?}");
        assert!(matches!(&d, Decision::Confirm(r) if r.phone_only));
        assert!(!summary(&d).contains("127.0.0.1"), "{}", summary(&d));
        assert!(matches!(decide(&policy, &password, &s), Decision::Deny(_)));
    }
    let d = decide(&default_policy(), &tap(&s, "Sign In"), &s);
    assert_eq!(asked(&d), Some(RiskCategory::AccountChange), "{d:?}");
    assert_eq!(
        asked(&decide(&default_policy(), &tap(&s, "Cancel"), &s)),
        None
    );
}

/// Review of round 3 (L4): the page's layout cannot defeat the sign-in rules — the user name
/// wrapped in a group of its own (a `<div role=group>`) still lies in the password's web view
/// (fails on 9ae7aeb: "same raw parent" was required).
#[test]
fn f6_a_wrapped_user_name_is_still_a_credential() {
    let mut s = parse(BASIC_AUTH);
    let mut done = false;
    edit(&mut s.root, &mut |n| {
        if done {
            return;
        }
        if let Some(i) = n
            .children
            .iter()
            .position(|c| c.placeholder.as_deref() == Some("User Name"))
        {
            done = true;
            let field = n.children.remove(i);
            let frame = field.frame;
            n.children.insert(
                i,
                UiNode::new(Role::Other("Other".into()), frame).with_children(vec![field]),
            );
        }
    });
    let user = Action::Type {
        element: Some(index(&s, "User Name")),
        text: "ada".into(),
        submit: false,
    };
    let d = decide_owned(&relaxed(), &user, &s, "sign in as ada");
    assert_eq!(asked(&d), Some(RiskCategory::Credential), "{d:?}");
}

/// JavaScript `confirm()` and `prompt()`: the dialog (`Other id=SFDialogView` in the page's
/// `WebView`) is the page's: OK and Cancel are inputs into the page's place (its host read
/// from the address field) and ask nothing by themselves; typing into the prompt's field is
/// an input into the page too and asks as model text (confirmed: unchanged).
#[test]
fn f6_javascript_dialogs_are_the_pages() {
    let site = FlowContext::Site {
        app: "com.apple.mobilesafari".into(),
        host: "127.0.0.1".into(),
    };
    for text in [JS_CONFIRM, JS_PROMPT] {
        let s = parse(text);
        let v = compact_view(&s);
        let p = places_of(&s, &v);
        for label in ["OK", "Cancel"] {
            let a = tap(&s, label);
            assert_eq!(
                p.destinations(&a, &v.screen, Some(&tree(&v))),
                vec![site.clone()]
            );
            assert_eq!(asked(&decide(&default_policy(), &a, &s)), None, "{label}");
        }
    }
    let s = parse(JS_PROMPT);
    let v = compact_view(&s);
    let field = v
        .screen
        .elements
        .iter()
        .find(|e| e.role == Role::TextField && e.label.is_empty())
        .unwrap();
    let a = Action::Type {
        element: Some(field.index),
        text: "hunter2".into(),
        submit: false,
    };
    assert_eq!(
        places_of(&s, &v).destinations(&a, &v.screen, Some(&tree(&v))),
        vec![site]
    );
    assert!(asked(&decide(&default_policy(), &a, &s)).is_some());
}

// ---- F7: confirmations on real trees --------------------------------------------------------

/// (a) Modals: the action sheet's destructive button and the alert's ask as `Delete`; the
/// content behind every modal is absent from the tree; a full sheet, a half sheet and a
/// cover hold no `Sheet` or `Alert` node and their content is listed; the popover's
/// dismiss region is not listed (an `Other`) where it is an `Other`, and where it is a
/// `Button` (the compact date picker) a tap on it is a plain dismiss.
#[test]
fn f7a_modals() {
    let sheet = parse(ACTION_SHEET);
    let d = decide(&default_policy(), &tap(&sheet, "Delete item"), &sheet);
    assert_eq!(asked(&d), Some(RiskCategory::Delete));
    assert_eq!(
        asked(&decide(
            &default_policy(),
            &tap(&sheet, "Keep item"),
            &sheet
        )),
        None
    );
    let alert = parse(ALERT_FIELD);
    assert_eq!(
        asked(&decide(&default_policy(), &tap(&alert, "Delete"), &alert)),
        Some(RiskCategory::Delete)
    );
    let ctx = parse(CONTEXT_MENU);
    assert_eq!(
        asked(&decide(
            &default_policy(),
            &tap(&ctx, "Delete from context menu"),
            &ctx
        )),
        Some(RiskCategory::Delete)
    );
    for (text, marker) in [
        (FULL_SHEET, "Probe full sheet body text"),
        (HALF_SHEET, "Probe half sheet body text"),
        (COVER, "Probe cover body text"),
    ] {
        let s = parse(text);
        let mut all = Vec::new();
        walk(&s.root, &mut all);
        assert!(
            all.iter()
                .all(|n| !matches!(n.role, Role::Sheet | Role::Alert)),
            "{marker}"
        );
        let c = compact_view(&s).screen;
        assert!(
            c.elements.iter().any(|e| e.label.contains(marker)),
            "{marker}: {:#?}",
            c.elements
        );
        // The base screen's content is not in the tree behind it.
        assert!(!mentions(&s, "Show action sheet"), "{marker}");
    }
    let pop = compact_view(&parse(POPOVER)).screen;
    assert!(pop.elements.iter().all(|e| e.label != "dismiss popup"));
    let date = parse(P4D_DATE_OPEN);
    let d = decide(&default_policy(), &tap(&date, "dismiss popup"), &date);
    assert_eq!(asked(&d), None, "{d:?}");
}

/// (b) Links carry no URL: the SwiftUI `Link` is a `Button "Probe link"` with no URL in any
/// label, value or identifier; Safari's page links are `Link` nodes without one. A tap on a
/// page link goes to the page's place (its host from the address field), never to a place
/// read from the link.
#[test]
fn f7b_links_carry_no_destination() {
    let s = parse(P4D_BEFORE);
    let c = compact_view(&s).screen;
    let link = element(&c, "Probe link");
    assert_eq!(link.role, Role::Button);
    let mut all = Vec::new();
    walk(&s.root, &mut all);
    assert!(all.iter().all(|n| {
        [&n.label, &n.value, &n.identifier]
            .iter()
            .all(|t| !t.as_deref().unwrap_or("").contains("example.com"))
    }));
    let page = parse(SAFARI_PAGE);
    let v = compact_view(&page);
    let links: Vec<&Element> = v
        .screen
        .elements
        .iter()
        .filter(|e| e.role == Role::Link)
        .collect();
    assert!(!links.is_empty());
    let p = places_of(&page, &v);
    for l in links {
        assert!(
            l.value.as_deref().is_none_or(|v| !v.contains("http")),
            "{l:?}"
        );
        let d = p.destinations(
            &Action::Tap { element: l.index },
            &v.screen,
            Some(&tree(&v)),
        );
        assert_eq!(
            d,
            vec![FlowContext::Site {
                app: "com.apple.mobilesafari".into(),
                host: "127.0.0.1".into()
            }]
        );
    }
}

/// (d) A page's `<select>`, date and file inputs open native lists in Safari's chrome (no web
/// view holds them): what a tap on each asks today, stated.
#[test]
fn f7d_page_inputs_open_native_lists() {
    let select = parse(SELECT_OPEN);
    let date = parse(DATE_INPUT_OPEN);
    let file = parse(FILE_INPUT_OPEN);
    for (s, label, want) in [
        (&select, "Alpha", None),
        (&select, "Gamma", None),
        (&date, "Reset", Some(RiskCategory::Delete)),
        (&date, "Done", Some(RiskCategory::Delete)),
        (&file, "Photo Library", None),
        (&file, "Choose File", None),
    ] {
        let d = decide(&default_policy(), &tap(s, label), s);
        assert_eq!(asked(&d), want, "{label}: {d:?}");
    }
    // The select's options are listed in Safari's own place (a stated limit).
    let v = compact_view(&select);
    assert_eq!(
        places_of(&select, &v).base(),
        &FlowContext::app("com.apple.mobilesafari")
    );
}

/// (e) "Selected:" facts: a selected segment and a selected row are selections and are
/// named; a selected tab of a `TabBar` is not.
#[test]
fn f7e_selected_facts() {
    let q = |s: &Screen, label: &str| {
        let v = compact_view(&seen(s));
        let a = Action::Tap {
            element: element(&v.screen, label).index,
        };
        summary_with(
            &a,
            &v.screen,
            RiskCategory::Delete,
            ScreenSource::Typed,
            Some(&tree(&v)),
            &[],
        )
    };
    let seg = parse(P4D_BEFORE);
    let text = q(&seg, "Probe link");
    assert!(text.contains("Selected: Two"), "{text}");
    assert!(!text.contains("Playground"), "{text}");
    let sel = parse(P4D_SELECTION);
    let text = q(&sel, "Check row 1");
    assert!(text.contains("Check row 2"), "{text}");
    assert!(text.contains("Beta"), "{text}");
    let tabs = parse(P4D_TABS);
    let c = compact_view(&tabs).screen;
    let other = c
        .elements
        .iter()
        .find(|e| e.role == Role::Button && !e.selected && e.label.starts_with("Probe tab"))
        .unwrap();
    let text = q(&tabs, &other.label.clone());
    assert!(!text.contains("Selected: Probe tab 1"), "{text}");
    // The stepper's increment and decrement stay nested in its row button (not listed).
    let values = compact_view(&parse(P4D_VALUES)).screen;
    assert!(values
        .elements
        .iter()
        .all(|e| !e.label.ends_with("Increment")));
    // A secure field never shows its value.
    assert!(values
        .elements
        .iter()
        .filter(|e| e.role == Role::SecureTextField)
        .all(|e| e.value.is_none()));
}

/// (f) The in-app web view (p6c): no address anywhere; its content is the app's web
/// content, a place of its own beside the app's.
#[test]
fn f7f_in_app_web_content_is_its_own_place() {
    let s = parse(WEBVIEW);
    let v = compact_view(&s);
    let p = places_of(&s, &v);
    assert_eq!(p.base(), &FlowContext::app("dev.jarvis.app"));
    assert_eq!(
        p.read_places(),
        vec![
            FlowContext::app("dev.jarvis.app"),
            FlowContext::Embedded {
                app: "dev.jarvis.app".into(),
                within: None
            }
        ]
    );
}

/// (g) The badge: `Icon "Jarvis" value="7 new items"`; a tap on it launches Jarvis by its
/// exact label.
#[test]
fn f7g_a_badged_icon_still_launches_by_its_label() {
    let s = parse(HOME_BADGE);
    let jarvis = index(&s, "Jarvis");
    let r = run(
        jarvis_config(),
        "Open Jarvis",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            &format!(r#"{{"action":"tap","element":{jarvis}}}"#),
            r#"{"action":"done","summary":"ok"}"#,
        ]),
    );
    assert!(
        r.acts.iter().any(
            |a| matches!(a, RunnerCommand::LaunchApp { bundle_id } if bundle_id == "dev.jarvis.app")
                || matches!(a, RunnerCommand::Tap { .. })
        ),
        "{:?}",
        r.acts
    );
    assert!(r.confirms.is_empty(), "{:?}", r.confirms);
}

/// (h) Landscape: frames are landscape points (956 × 440) and the sideways cap is a share
/// of that width. The landscape keyboard's own window reports its frames rotated (`Keyboard
/// {{2, 117}, {162, 722}}` on a 956 × 440 screen): the keyboard's area then reaches from its
/// top to the screen's bottom across the width, which holds the real keyboard (the bottom of
/// the screen) — over-covered, never under-covered.
#[test]
fn f7h_landscape() {
    let c = compact_view(&parse(LANDSCAPE)).screen;
    assert_eq!((c.size.width, c.size.height), (956.0, 440.0));
    let g = drag_gesture(GestureKind::Swipe, Direction::Left, None, &c).unwrap();
    assert!((g.from.x - g.to.x) <= 956.0 * MAX_SIDEWAYS_SHARE + 1e-9);
    let v = compact_view(&parse(LANDSCAPE_KEYBOARD));
    let kb: Vec<_> = v
        .nodes
        .iter()
        .filter(|n| n.role == Role::Keyboard)
        .collect();
    let area = kb.first().unwrap().frame;
    assert!(
        area.x <= 0.0 && area.max_x() >= 956.0 && area.max_y() >= 440.0,
        "{area:?}"
    );
    assert!(area.y <= 440.0 - 162.0, "{area:?}");
}

/// Review pass 36 (residual of F2): the dialog's "Sign In" drawn with a lowercase `l` for
/// the capital `I` ("Sign ln", "Log ln", "SIGN lN") is the button that sends the password
/// form: asked as an account action like "Sign In"; refused under a policy that denies
/// account changes.
#[test]
fn f6_a_lookalike_sign_in_beside_the_password_is_an_account_action() {
    for drawn in ["Sign ln", "Log ln", "SIGN lN", "Sign\u{200B} ln"] {
        let mut s = parse(BASIC_AUTH);
        let mut done = 0;
        edit(&mut s.root, &mut |n| {
            if n.role == Role::Button && n.label.as_deref() == Some("Sign In") {
                n.label = Some(drawn.to_owned());
                done += 1;
            }
        });
        assert_eq!(done, 1, "{drawn}");
        let d = decide(&default_policy(), &tap(&s, drawn), &s);
        assert_eq!(
            asked(&d),
            Some(RiskCategory::AccountChange),
            "{drawn}: {d:?}"
        );
        let deny = Policy {
            deny_categories: vec![RiskCategory::AccountChange],
            ..default_policy()
        };
        assert!(
            matches!(decide(&deny, &tap(&s, drawn), &s), Decision::Deny(_)),
            "{drawn}"
        );
    }
}
