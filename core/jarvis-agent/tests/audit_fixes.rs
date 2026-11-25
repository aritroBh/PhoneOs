#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! The tree-assumption audit (126 assumptions of the gate read against the real iOS 26.5
//! trees; 8 new contradictions): the fixes of C1 to C4, C7 and C8, each tested on the real
//! capture that contradicted the code (`tests/fixtures/ios26_probes/`, the observe crate's
//! `ios26` fixtures; see each file's header).
//!
//! * C1: the keyboard's Return key is found by its key-type identifier (`Button "go"
//!   id=Go`), and that type reaches every rule that reasons about what Return does.
//! * C2: SF Symbol names are identifiers (`Image "Share" id=square.and.arrow.up`): a risky
//!   symbol in the pressed control's identifier, or its image's, raises its risk.
//! * C3: a tab bar's items are `Button`s: the selected tab is not "the selection".
//! * C4: the keyboard is bigger than the `Keyboard` node (Emoji and Dictate sit below it).
//! * C7: a notification row that wraps a banner is a tap on that notification.
//! * C8: the `, App` and badge suffixes of home-screen labels, which no real tree shows, are
//!   gone.

mod common;

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_observe::{compact_view, render, CompactView};
use jarvis_proto::fixtures::{node, screen};
use jarvis_proto::{
    Action, CompactScreen, Point, Policy, Rect, RiskCategory, Role, RunnerCommand, Screen, UiNode,
};
use jarvis_safety::{
    control_at, evaluate_sheeted, home_view, in_alert, observed_places, summary_with, tap_spot,
    Decision, PointError, ScreenPlaces, ScreenSource, ScreenTree,
};

const LOGIN_KEYBOARD: &str =
    include_str!("../../jarvis-observe/tests/fixtures/ios26/login_keyboard.txt");
const SAFARI_FORM_KEYBOARD: &str =
    include_str!("../../jarvis-observe/tests/fixtures/ios26_extra/safari_form_keyboard.txt");
const TOGGLES_OFF: &str = include_str!("../../jarvis-observe/tests/fixtures/ios26/toggles_off.txt");
const PLAYGROUND_HOME: &str =
    include_str!("../../jarvis-observe/tests/fixtures/ios26/playground_home.txt");
const SPRINGBOARD_HOME: &str =
    include_str!("../../jarvis-observe/tests/fixtures/ios26/springboard_home.txt");
const SPOTLIGHT_SETTINGS: &str = include_str!("fixtures/ios26_probes/spotlight_settings.txt");
const SPOTLIGHT_SETTINGS_134: &str =
    include_str!("fixtures/ios26_probes/spotlight_settings_134.txt");
const SPOTLIGHT_EXAMPLE: &str = include_str!("fixtures/ios26_probes/spotlight_example_com.txt");
const SPOTLIGHT_EXAMPLE_134: &str =
    include_str!("fixtures/ios26_probes/spotlight_example_com_134.txt");
const SPOTLIGHT_SEARCH_KEY: &str = include_str!("fixtures/ios26_probes/spotlight_search_key.txt");
const EDIT_MENU: &str = include_str!("fixtures/ios26_probes/edit_menu.txt");
const EDIT_MENU_LIST: &str = include_str!("fixtures/ios26_probes/edit_menu_list.txt");
const HOME_ICON_MENU: &str = include_str!("fixtures/ios26_probes/home_icon_menu.txt");

const DANGER_ZONE: &str = include_str!("fixtures/ios26_probes/danger_zone_tab_bar.txt");
const LOCK_SCREEN: &str = include_str!("fixtures/ios26_probes/lock_screen_notification.txt");
const BANNER_HOME: &str = include_str!("fixtures/ios26_probes/banner_springboard.txt");

/// Every real capture with a keyboard up (name, tree).
const KEYBOARDS: &[(&str, &str)] = &[
    ("login_keyboard", LOGIN_KEYBOARD),
    ("safari_form_keyboard", SAFARI_FORM_KEYBOARD),
    ("spotlight_settings", SPOTLIGHT_SETTINGS),
    ("spotlight_settings_134", SPOTLIGHT_SETTINGS_134),
    ("spotlight_example_com", SPOTLIGHT_EXAMPLE),
    ("spotlight_example_com_134", SPOTLIGHT_EXAMPLE_134),
    ("spotlight_search_key", SPOTLIGHT_SEARCH_KEY),
    ("edit_menu", EDIT_MENU),
    ("edit_menu_list", EDIT_MENU_LIST),
];

/// What `ios` (the observe tests' parser) uses to print a compact screen.
fn compact_text(c: &CompactScreen) -> String {
    format!("{c:?}")
}

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

fn default_policy() -> Policy {
    Policy {
        first_use_approval: false,
        ..Policy::default()
    }
}

/// The category a decision asks under (`None` when it does not ask).
fn asked(d: &Decision) -> Option<RiskCategory> {
    match d {
        Decision::Confirm(r) => Some(r.category),
        _ => None,
    }
}

fn element<'a>(c: &'a CompactScreen, label: &str) -> &'a jarvis_proto::Element {
    c.elements
        .iter()
        .find(|e| e.label == label)
        .unwrap_or_else(|| panic!("no element {label:?}: {:#?}", c.elements))
}

fn edit(n: &mut UiNode, f: &mut impl FnMut(&mut UiNode)) {
    f(n);
    for c in &mut n.children {
        edit(c, f);
    }
}

/// The first node (pre-order) `f` accepts.
fn find<'a>(n: &'a UiNode, f: &impl Fn(&UiNode) -> bool) -> Option<&'a UiNode> {
    if f(n) {
        return Some(n);
    }
    n.children.iter().find_map(|c| find(c, f))
}

fn has_id(n: &UiNode, id: &str) -> bool {
    n.identifier.as_deref() == Some(id)
}

// ---- C1: the Return key by its key-type identifier -------------------------------------------

/// On every real keyboard the Return key's type comes from its identifier, and the label
/// stays the cue the question quotes: `Button "return" id=Return`, `Button "go" id=Go`,
/// `Button "search" id=Search` (fails on the old core: no type existed; an English label
/// was the only way once the identifier was not `Return`).
#[test]
fn the_return_keys_type_is_its_identifier_on_every_real_keyboard() {
    let expect = [
        ("login_keyboard", "return", "return"),
        ("safari_form_keyboard", "go", "go"),
        ("spotlight_settings", "go", "go"),
        ("spotlight_settings_134", "go", "go"),
        ("spotlight_example_com", "go", "go"),
        ("spotlight_example_com_134", "go", "go"),
        ("spotlight_search_key", "search", "search"),
        ("edit_menu", "return", "return"),
        ("edit_menu_list", "return", "return"),
    ];
    for (name, label, kind) in expect {
        let text = KEYBOARDS.iter().find(|(n, _)| *n == name).unwrap().1;
        let c = jarvis_observe::compact(&parse(text));
        assert_eq!(c.cues.return_key.as_deref(), Some(label), "{name}");
        assert_eq!(c.cues.return_key_type.as_deref(), Some(kind), "{name}");
    }
}

/// `screen` with its keyboard's Return key relabelled `label` and given the identifier `id`.
fn with_return_key(s: &Screen, label: &str, id: Option<&str>) -> Screen {
    let mut s = s.clone();
    edit(&mut s.root, &mut |n| {
        if n.role == Role::Button
            && n.identifier
                .as_deref()
                .is_some_and(|i| ["Return", "Go", "Search"].contains(&i))
        {
            n.label = Some(label.to_owned());
            n.identifier = id.map(str::to_owned);
        }
    });
    s
}

/// The real login keyboard over a notes screen: a focused multi-line text view.
fn note_with_keyboard(label: &str, id: Option<&str>) -> Screen {
    let mut s = with_return_key(&parse(LOGIN_KEYBOARD), label, id);
    s.app_bundle_id = "com.example.notes".into();
    s.app_name = Some("Notes".into());
    let size = Rect::new(0.0, 0.0, 440.0, 956.0);
    let mut view = node(Role::TextView, "Note", Rect::new(20.0, 120.0, 400.0, 300.0));
    view.focused = true;
    s.root.children[0] = UiNode::new(Role::Window, size).with_children(vec![view]);
    s
}

/// A constructed keyboard whose Return key is labelled in another language with the
/// identifier `Send` is a send Return: in a multi-line text view its Return submits, so
/// the typed words count ("buy milk" is a purchase) — as with the English "send" label —
/// where a plain Return only inserts a line break (fails on the old core: the label
/// "Senden" was no Return-key label, the cue was lost and the line-break leniency applied).
#[test]
fn a_localized_send_key_is_a_send_return() {
    let typed = Action::Type {
        element: None,
        text: "buy milk".into(),
        submit: true,
    };
    let class = |s: &Screen| {
        let c = jarvis_observe::compact(s);
        jarvis_safety::classify(&typed, &c, None)
    };
    let localized = note_with_keyboard("Senden", Some("Send"));
    let c = jarvis_observe::compact(&localized);
    assert_eq!(c.cues.return_key.as_deref(), Some("Senden"));
    assert_eq!(c.cues.return_key_type.as_deref(), Some("send"));
    let english = class(&note_with_keyboard("send", None));
    let plain = class(&note_with_keyboard("return", Some("Return")));
    assert_ne!(english, plain, "the fixture must tell them apart");
    assert_eq!(class(&localized), english);
    // Without the identifier a localized label says nothing (the second way is English).
    assert_eq!(class(&note_with_keyboard("Senden", None)), plain);
    // The question quotes the key as the owner sees it.
    let v = compact_view(&localized);
    let q = summary_with(
        &typed,
        &v.screen,
        class(&localized),
        ScreenSource::Typed,
        Some(&tree(&v)),
        &[],
    );
    assert!(q.contains("(\"Senden\")"), "{q}");
}

/// The other rules that read what Return does take the type too: a send Return in a text
/// view sends the field (`Sends:`), and the question's own category is the key's
/// (`own_category`): an owner-relaxed policy still asks a localized send key's Return.
#[test]
fn a_localized_send_keys_return_asks_like_an_english_one() {
    let typed = Action::Type {
        element: None,
        text: "see you at noon".into(),
        submit: true,
    };
    let d = |s: &Screen| asked(&decide(&default_policy(), &typed, s));
    let english = d(&note_with_keyboard("send", None));
    assert_eq!(english, Some(RiskCategory::SendMessage));
    assert_eq!(d(&note_with_keyboard("Senden", Some("Send"))), english);
}

/// A button whose identifier is a Return-key type **outside** the keyboard never becomes the
/// Return key: not on a screen without a keyboard, and not beside the real keyboard (the
/// login form's "Sign In" given the identifier `Send`).
#[test]
fn a_send_identifier_outside_the_keyboard_is_no_return_key() {
    let mut home = parse(PLAYGROUND_HOME);
    edit(&mut home.root, &mut |n| {
        if n.role == Role::Button {
            n.identifier = Some("Send".into());
        }
    });
    let c = jarvis_observe::compact(&home);
    assert_eq!(c.cues.return_key, None);
    assert_eq!(c.cues.return_key_type, None);
    let mut login = parse(LOGIN_KEYBOARD);
    edit(&mut login.root, &mut |n| {
        if n.label.as_deref() == Some("Sign In") {
            n.identifier = Some("Send".into());
        }
    });
    let c = jarvis_observe::compact(&login);
    assert_eq!(c.cues.return_key_type.as_deref(), Some("return"));
    assert_eq!(c.cues.return_key.as_deref(), Some("return"));
}

// ---- C2: SF Symbol names are identifiers ------------------------------------------------------

/// The real Home Screen icon menu's "Remove App" row holds `Image "remove" id=minus.circle`.
/// Labelled in another language, the row is still a `Delete` by its image's identifier
/// (fails on the old core: the symbol table read labels only, and "App entfernen" / "Entfernen"
/// are no symbol names).
#[test]
fn a_risky_symbol_identifier_raises_a_localized_row_on_the_real_icon_menu() {
    let mut s = parse(HOME_ICON_MENU);
    edit(&mut s.root, &mut |n| match n.label.as_deref() {
        Some("Remove App") => n.label = Some("App entfernen".into()),
        Some("remove") => n.label = Some("entfernen".into()),
        _ => {}
    });
    let v = compact_view(&seen(&s));
    let row = element(&v.screen, "App entfernen");
    let d = decide(&default_policy(), &Action::Tap { element: row.index }, &s);
    assert_eq!(asked(&d), Some(RiskCategory::Delete), "{d:?}");
}

/// The real expanded edit menu's "Share…" holds `Image "Share" id=square.and.arrow.up`:
/// relabelled "Teilen…" it is still at least what the share symbol says (fails on the old
/// core: the hand-off rule's `SendMessage` only).
#[test]
fn a_risky_symbol_identifier_raises_a_localized_row_on_the_real_edit_menu() {
    let mut s = parse(EDIT_MENU_LIST);
    edit(&mut s.root, &mut |n| match n.label.as_deref() {
        Some("Share…") => n.label = Some("Teilen…".into()),
        Some("Share") => n.label = Some("Teilen".into()),
        _ => {}
    });
    let v = compact_view(&s);
    let row = element(&v.screen, "Teilen…");
    let d = decide(&default_policy(), &Action::Tap { element: row.index }, &s);
    assert_eq!(asked(&d), Some(RiskCategory::PublicPost), "{d:?}");
}

/// A screen of the Playground holding one icon button: `Button <label> > Image <image>
/// id=<symbol>`, with nothing else risky on it.
fn icon_button(label: &str, image: &str, symbol: &str) -> Screen {
    let full = Rect::new(0.0, 0.0, 440.0, 956.0);
    let mut img = node(Role::Image, image, Rect::new(200.0, 400.0, 24.0, 24.0));
    img.identifier = Some(symbol.into());
    let mut button = node(Role::Button, label, Rect::new(190.0, 390.0, 44.0, 44.0));
    button.children = vec![img];
    let title = node(
        Role::StaticText,
        "Notizen",
        Rect::new(20.0, 80.0, 200.0, 30.0),
    );
    let root = UiNode::new(Role::Application, full)
        .with_label("Example")
        .with_children(vec![
            UiNode::new(Role::Window, full).with_children(vec![title, button])
        ]);
    screen("com.example.app", "Example", root, false)
}

/// Constructed, with non-English labels: the symbol in the image's identifier decides at
/// least `trash` → Delete, `paperplane.fill` → SendMessage, `creditcard` → Purchase; the
/// button's own identifier counts too (fails on the old core for every case: each label is
/// German, no symbol name).
#[test]
fn a_risky_symbol_identifier_raises_a_control_whatever_its_label() {
    for (label, image, symbol, want) in [
        ("Löschen", "Papierkorb", "trash", RiskCategory::Delete),
        (
            "Senden",
            "Senden",
            "paperplane.fill",
            RiskCategory::SendMessage,
        ),
        ("Karte", "Karte", "creditcard", RiskCategory::Purchase),
    ] {
        let s = icon_button(label, image, symbol);
        let v = compact_view(&s);
        let b = element(&v.screen, label);
        let d = decide(&default_policy(), &Action::Tap { element: b.index }, &s);
        let got = asked(&d).unwrap_or(RiskCategory::None);
        assert_eq!(
            jarvis_safety::max_risk(got, want),
            got,
            "{symbol}: {got:?} below {want:?}"
        );
        // On the button itself.
        let mut own = s.clone();
        edit(&mut own.root, &mut |n| {
            if n.role == Role::Button {
                n.identifier = Some(symbol.into());
                n.children.clear();
            }
        });
        let v = compact_view(&own);
        let b = element(&v.screen, label);
        let got = asked(&decide(
            &default_policy(),
            &Action::Tap { element: b.index },
            &own,
        ))
        .unwrap_or(RiskCategory::None);
        assert_eq!(jarvis_safety::max_risk(got, want), got, "{symbol} (own)");
    }
}

/// Identifiers never exempt: a "Senden" button whose image's identifier is a not-submit
/// symbol (`xmark.circle.fill`) is still pressed as a send after Jarvis typed into the
/// form (the not-submit symbols stay label-based; an app can set any identifier).
#[test]
fn an_identifier_never_exempts_a_submit() {
    let mut s = icon_button("Senden", "Senden", "xmark.circle.fill");
    let mut field = node(
        Role::TextField,
        "Nachricht",
        Rect::new(20.0, 300.0, 400.0, 44.0),
    );
    field.value = Some("hallo".into());
    s.root.children[0].children.push(field);
    let v = compact_view(&s);
    let b = element(&v.screen, "Senden");
    let f = element(&v.screen, "Nachricht");
    let typed = [jarvis_safety::TypedText {
        field: f.fingerprint,
        text: "hallo".into(),
    }];
    let d = evaluate_sheeted(
        &default_policy(),
        &Action::Tap { element: b.index },
        &v.screen,
        None,
        ScreenSource::Typed,
        Some(&tree(&v)),
        &typed,
        &["help me with this screen"],
        false,
        false,
        None,
    );
    let got = asked(&d).unwrap_or(RiskCategory::None);
    assert_eq!(
        jarvis_safety::max_risk(got, RiskCategory::SendMessage),
        got,
        "{d:?}"
    );
}

/// The real trees' symbol identifiers that name no risk stay without one (no over-ask on the
/// Playground's tab bar: `hand.tap`, `message`, `sparkles`, `square.grid.2x2`,
/// `slider.horizontal.3`).
#[test]
fn harmless_real_symbol_identifiers_add_nothing() {
    for text in [PLAYGROUND_HOME, TOGGLES_OFF, DANGER_ZONE, LOGIN_KEYBOARD] {
        let v = compact_view(&parse(text));
        assert!(
            v.nodes.iter().all(|n| n.symbol.is_none()),
            "{:?}",
            v.nodes
                .iter()
                .filter(|n| n.symbol.is_some())
                .collect::<Vec<_>>()
        );
    }
    let v = compact_view(&parse(HOME_ICON_MENU));
    assert!(v
        .nodes
        .iter()
        .any(|n| n.symbol == Some(RiskCategory::Delete)));
}

// ---- C3: a selected tab is not "the selection" -------------------------------------------------

/// The real "Danger zone" with its tab bar (`Button "Playground" id=hand.tap [selected]` in
/// a `TabBar`): the "Delete account" question no longer says `Selected: Playground` (fails on
/// the old core: the run-137 question read `Tap "Delete account" in Jarvis, screen "Danger
/// zone". Selected: Playground`).
#[test]
fn the_selected_tab_is_not_the_selection_on_the_real_danger_zone() {
    let s = parse(DANGER_ZONE);
    let r = run(
        config(),
        "delete my account",
        MemoryStore::new(),
        Phone::still(s.clone()),
        Script::model(&[&format!(
            r#"{{"action":"tap","element":{}}}"#,
            index_of(&s, "Delete account")
        )])
        .decisions(&[false]),
    );
    let q = r
        .confirms
        .iter()
        .find(|c| c.summary.contains("Delete account"))
        .unwrap_or_else(|| panic!("{:#?}", r.confirms));
    assert!(!q.summary.contains("Selected"), "{}", q.summary);
    assert!(q.summary.contains("Danger zone"), "{}", q.summary);
}

/// A selected row that is not a tab is still reported: the real "Danger zone" with its
/// status row marked selected (constructed: the corpus holds no selected list row) says
/// `Selected: Account active` and still not "Playground"; without the raw tree (a hand-built
/// screen) the role decides as before and the tab is listed.
#[test]
fn a_selected_row_beside_the_tab_bar_is_still_reported() {
    let mut s = parse(DANGER_ZONE);
    edit(&mut s.root, &mut |n| {
        if n.frame == Rect::new(20.0, 168.0, 400.0, 52.0) {
            n.selected = true;
        }
    });
    let v = compact_view(&s);
    let target = element(&v.screen, "Delete account");
    let ask = |t: Option<&ScreenTree<'_>>| {
        summary_with(
            &Action::Tap {
                element: target.index,
            },
            &v.screen,
            RiskCategory::Delete,
            ScreenSource::Typed,
            t,
            &[],
        )
    };
    let q = ask(Some(&tree(&v)));
    let selected = q.split("Selected:").nth(1).unwrap_or_else(|| panic!("{q}"));
    assert!(selected.contains("Account active"), "{q}");
    assert!(!selected.contains("Playground"), "{q}");
    let q = ask(None);
    assert!(q.contains("Playground"), "{q}");
}

// ---- C4: the keyboard is bigger than the `Keyboard` node ----------------------------------------

/// The frame of the input host (`inputView`) holding the keyboard, of every real capture.
fn input_host(s: &Screen) -> Rect {
    let kb = find(&s.root, &|n| n.role == Role::Keyboard).unwrap().frame;
    find(&s.root, &|n| {
        has_id(n, "inputView") && n.frame.y <= kb.y && n.frame.max_y() >= kb.max_y()
    })
    .unwrap()
    .frame
}

/// On every real keyboard capture no point inside the input host resolves to a tappable
/// control, and a `tap_point` there is refused as the keyboard: a grid over the host, the
/// Emoji and Dictate buttons' centres included (fails on the old core: below y 898 the
/// `Keyboard` node ended and those buttons were plain unlisted controls).
#[test]
fn no_point_of_the_input_host_is_tappable_on_any_real_keyboard() {
    for (name, text) in KEYBOARDS {
        let s = seen(&parse(text));
        let v = compact_view(&s);
        let t = tree(&v);
        let host = input_host(&s);
        let mut points: Vec<Point> = Vec::new();
        for i in 0..12 {
            for j in 0..12 {
                points.push(Point::new(
                    host.x + host.width * (f64::from(i) + 0.5) / 12.0,
                    host.y + host.height * (f64::from(j) + 0.5) / 12.0,
                ));
            }
        }
        for b in ["Emoji", "Dictate"] {
            let n = find(&s.root, &|n| n.label.as_deref() == Some(b)).unwrap();
            points.push(n.frame.center());
        }
        for p in points {
            assert!(
                matches!(
                    control_at(p, &v.screen, Some(&t)),
                    Err(PointError::Keyboard)
                ),
                "{name}: {p:?} -> {:?}",
                control_at(p, &v.screen, Some(&t))
            );
        }
        // Listing agrees: nothing listed lies in the host, Emoji and Dictate are not listed.
        for e in &v.screen.elements {
            assert!(!matches!(e.label.as_str(), "Emoji" | "Dictate"), "{name}");
            if e.frame.y >= host.y {
                panic!("{name}: listed inside the keyboard: {e:?}");
            }
        }
    }
}

/// A `tap_point` on the real Dictate button's centre is refused (Rule H): no tap is sent,
/// whatever the owner approves (fails on the old core: the point was below the `Keyboard`
/// node, on no listed control, and went to the session's gate).
#[test]
fn a_tap_point_on_dictate_is_refused() {
    // The login form in an ordinary app (Jarvis's own app is not acted on while not driven).
    let mut s = parse(LOGIN_KEYBOARD);
    s.app_bundle_id = "com.example.login".into();
    let d = find(&s.root, &|n| n.label.as_deref() == Some("Dictate"))
        .unwrap()
        .frame
        .center();
    let (x, y) = (d.x / 440.0, d.y / 956.0);
    let point = format!(r#"{{"action":"tap_point","x":{x:.4},"y":{y:.4}}}"#);
    let r = run(
        vision_config(),
        "dictate a note",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[r#"{"action":"look"}"#, &point, &point]).decisions(&[true; 4]),
    );
    assert!(
        !r.acts.iter().any(|a| matches!(
            a,
            RunnerCommand::Tap { .. } | RunnerCommand::LongPress { .. }
        )),
        "{:?}",
        r.acts
    );
}

/// A scroll whose element-less drag would reach below the `Keyboard` node is refused or
/// planned above the keyboard's whole area: no drag of a planned command starts or ends in
/// the input host.
#[test]
fn no_drag_touches_the_input_host() {
    let s = parse(LOGIN_KEYBOARD);
    let host = input_host(&s);
    for dir in ["up", "down", "left", "right"] {
        let a = format!(r#"{{"action":"scroll","direction":"{dir}"}}"#);
        let r = run(
            config(),
            "scroll",
            MemoryStore::new(),
            Phone::still(s.clone()),
            Script::model(&[&a]).decisions(&[true; 4]),
        );
        for c in &r.acts {
            if let RunnerCommand::Swipe { from, to, .. } = c {
                assert!(!host.contains(*from) && !host.contains(*to), "{dir}: {c:?}");
            }
        }
    }
}

/// A random app screen behind a real-shaped keyboard that ends above the screen's bottom
/// (a row of buttons below it, an input host or none, content everywhere): every listed
/// line carries the cover marker exactly when `tap_spot` refuses it, no listed element's
/// shown part reaches into the keyboard's area, and every point of the area is refused as
/// the keyboard — listing and refusal are one decision.
#[test]
fn listing_and_refusal_agree_around_a_short_keyboard() {
    use jarvis_proto::fixtures::random::Rng;
    use jarvis_proto::CoverKind;
    use jarvis_safety::{takes_taps, CoveredBy};
    let full = Rect::new(0.0, 0.0, 440.0, 956.0);
    for seed in 0..300u64 {
        let r = &mut Rng::new(seed);
        let kb_y = r.f(500.0, 700.0);
        let kb = Rect::new(0.0, kb_y, 440.0, r.f(150.0, 250.0));
        let mut rows = Vec::new();
        for k in 0..r.below(20) {
            let y = r.f(60.0, 940.0);
            let role = r.pick(&[Role::Button, Role::Cell, Role::StaticText, Role::Switch]);
            rows.push(node(
                role.clone(),
                &format!("Row {k}"),
                Rect::new(r.f(0.0, 300.0), y, r.f(40.0, 440.0), r.f(20.0, 120.0)),
            ));
        }
        let app = UiNode::new(Role::Window, full).with_children(rows);
        let keys = UiNode::new(Role::Keyboard, kb).with_children(vec![node(
            Role::Key,
            "q",
            Rect::new(4.0, kb.y + 8.0, 40.0, 46.0),
        )]);
        let below = UiNode::new(Role::Other("Other".into()), full).with_children(vec![
            node(
                Role::Button,
                "Emoji",
                Rect::new(2.0, kb.max_y() - 10.0, 80.0, 68.0),
            ),
            node(
                Role::Button,
                "Dictate",
                Rect::new(357.0, kb.max_y() - 10.0, 80.0, 68.0),
            ),
        ]);
        let mut windows = vec![app];
        if r.chance(60) {
            let host = Rect::new(0.0, kb.y - 44.0, 440.0, 956.0 - kb.y + 44.0);
            windows.push(UiNode::new(Role::Window, full).with_children(vec![
                UiNode::new(Role::Other("Other".into()), host).with_identifier("inputView"),
            ]));
        }
        windows.push(UiNode::new(Role::Window, full).with_children(vec![keys, below]));
        let root = UiNode::new(Role::Application, full)
            .with_label("App")
            .with_children(windows);
        let mut s = screen("com.example.app", "App", root, true);
        s.size = jarvis_proto::Size::new(440.0, 956.0);
        let v = compact_view(&s);
        let t = tree(&v);
        let text = render(&v.screen, u32::MAX);
        for e in &v.screen.elements {
            assert!(
                !matches!(e.label.as_str(), "Emoji" | "Dictate"),
                "seed {seed}"
            );
            assert!(e.frame.y < kb.y, "seed {seed}: {e:?}");
            let line = text
                .lines()
                .find(|l| l.starts_with(&format!("[{}] ", e.index)))
                .unwrap();
            let refused = if takes_taps(&e.role) {
                tap_spot(e, &v.screen, Some(&t), None).err()
            } else {
                None
            };
            let expect = refused.map(|c| match c.by {
                CoveredBy::Keyboard => CoverKind::Keyboard,
                CoveredBy::Bar(_) => CoverKind::TabBar,
                _ => CoverKind::Other,
            });
            assert_eq!(
                v.screen.cues.cover_of(e.index).map(|k| {
                    if matches!(k, CoverKind::Keyboard | CoverKind::Other) {
                        k
                    } else {
                        CoverKind::TabBar
                    }
                }),
                expect,
                "seed {seed}: {line}"
            );
            if let Ok(p) = tap_spot(e, &v.screen, Some(&t), None) {
                assert!(p.y < kb.y, "seed {seed}: {line} at {p:?}");
            }
        }
        for i in 0..8 {
            for j in 0..8 {
                let p = Point::new(
                    440.0 * (f64::from(i) + 0.5) / 8.0,
                    kb.y + (956.0 - kb.y) * (f64::from(j) + 0.5) / 8.0,
                );
                assert!(
                    matches!(
                        control_at(p, &v.screen, Some(&t)),
                        Err(PointError::Keyboard)
                    ),
                    "seed {seed}: {p:?} {:?} kb {kb:?} {:?}",
                    control_at(p, &v.screen, Some(&t)),
                    v.nodes.iter().filter(|n| n.keyboard).collect::<Vec<_>>()
                );
            }
        }
    }
}

// ---- C7: a notification row that wraps a banner is a banner -------------------------------------

/// The real lock screen's notification row is `Button "10 minutes ago, Ready for Apple
/// Intelligence, …" id=ListCell` wrapping the banner (`NotificationShortLookView`): a tap on
/// the listed row is a tap on that notification (R4: asked, phone only), never a Home Screen
/// launch (fails on the old core: only the banner's ancestors were looked at).
#[test]
fn a_notification_row_wrapping_a_banner_is_a_tap_on_it() {
    let s = parse(LOCK_SCREEN);
    let shown = seen(&s);
    let v = compact_view(&shown);
    let t = tree(&v);
    let row = v
        .screen
        .elements
        .iter()
        .find(|e| {
            e.label
                .starts_with("10 minutes ago, Ready for Apple Intelligence")
        })
        .unwrap_or_else(|| panic!("{:#?}", v.screen.elements));
    let tap = Action::Tap { element: row.index };
    // The listed row is the outer `ListCell` button: the banner is inside it, not above it.
    let own = t.sources[usize::try_from(row.index - 1).unwrap()];
    assert!(
        v.nodes
            .iter()
            .any(|n| n.banner && n.node > own && n.node < t.ends[own as usize]),
        "the banner is not in the row's subtree"
    );
    assert!(in_alert(&tap, &v.screen, Some(&t)));
    match decide(&default_policy(), &tap, &s) {
        Decision::Confirm(r) => assert!(r.phone_only, "{r:?}"),
        other => panic!("{other:?}"),
    }
    // Exactly as a tap on the real banner's own button is decided (`banner_springboard`).
    let banner = parse(BANNER_HOME);
    let bv = compact_view(&seen(&banner));
    let inner = bv
        .screen
        .elements
        .iter()
        .find(|e| e.role == Role::Button && e.label.contains("Probe banner"))
        .unwrap();
    let relaxed = Policy {
        confirm: Vec::new(),
        first_use_approval: false,
        ..Policy::default()
    };
    // Stated: under an owner who relaxed every category this is allowed, as the banner's own
    // button is; the old core asked it as an unknown launch.
    for p in [default_policy(), relaxed] {
        let b = decide(
            &p,
            &Action::Tap {
                element: inner.index,
            },
            &banner,
        );
        let r = decide(&p, &tap, &s);
        assert_eq!(asked(&b).is_some(), asked(&r).is_some(), "{b:?} / {r:?}");
    }
}

/// The session sends no tap on the row unasked and never resolves it as a launch.
#[test]
fn the_session_asks_a_tap_on_a_notification_row() {
    let s = parse(LOCK_SCREEN);
    let shown = seen(&s);
    let c = jarvis_observe::compact(&shown);
    let row = c
        .elements
        .iter()
        .find(|e| e.label.starts_with("10 minutes ago"))
        .unwrap();
    let r = run(
        config(),
        "open the notification",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[&format!(r#"{{"action":"tap","element":{}}}"#, row.index)])
            .decisions(&[false; 4]),
    );
    assert!(r.acts.is_empty(), "{:?}", r.acts);
    assert!(
        r.confirms
            .iter()
            .any(|c| c.phone_only && c.summary.starts_with("Tap")),
        "{:#?}",
        r.confirms
    );
}

// ---- C8: no `, App` or badge suffix ---------------------------------------------------------------

/// A real Home Screen icon keeps its label the app's name and puts what it shows in its value
/// (`Button "Calendar" value="Sunday, November 22"`): the label forms `<Name>, App` and
/// `<Name>, <n> unread` that the launch rule accepted occur in no real tree. Such a label is
/// now an unknown launch: it asks, where it was a launch of the named app (fails on the old
/// core: "Settings, App" and "Settings, 2 unread" launched Settings unasked).
#[test]
fn a_suffixed_home_screen_label_is_an_unknown_launch() {
    let real = parse(SPRINGBOARD_HOME);
    let cal = find(&real.root, &|n| {
        n.value.as_deref() == Some("Sunday, November 22")
    })
    .unwrap();
    assert_eq!(cal.label.as_deref(), Some("Calendar"));
    let home = |label: &str| {
        let full = Rect::new(0.0, 0.0, 440.0, 956.0);
        // A banner makes the Home Screen a read, so a launch Jarvis cannot name asks.
        let banner = UiNode::new(
            Role::Other("NotificationBanner".to_owned()),
            Rect::new(8.0, 50.0, 424.0, 80.0),
        )
        .with_label("Messages, now, Your sign-in code is 482913");
        let icon = node(Role::Button, label, Rect::new(20.0, 300.0, 72.0, 94.0));
        let root = UiNode::new(Role::Application, full).with_children(vec![UiNode::new(
            Role::Window,
            full,
        )
        .with_children(vec![banner, icon])]);
        screen("com.apple.springboard", "SpringBoard", root, false)
    };
    let launches = |label: &str| {
        let s = home(label);
        let mut c = config();
        c.policy.confirm = Vec::new();
        run(
            c,
            "Open Settings",
            MemoryStore::new(),
            Phone::still(s.clone()),
            Script::model(&[&format!(
                r#"{{"action":"tap","element":{}}}"#,
                index_of(&s, label)
            )])
            .decisions(&[false; 4]),
        )
    };
    let plain = launches("Settings");
    assert!(plain.confirms.is_empty(), "{:#?}", plain.confirms);
    assert_eq!(plain.acts.len(), 1, "{:?}", plain.acts);
    for label in [
        "Settings, App",
        "SETTINGS, app",
        "Settings, 2 unread",
        "Settings, 1 notification",
    ] {
        let r = launches(label);
        assert!(r.acts.is_empty(), "{label}: {:?}", r.acts);
        assert!(!r.confirms.is_empty(), "{label}");
    }
}
