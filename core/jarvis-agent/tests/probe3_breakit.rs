#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code,
    unused_imports
)]
//! Review pass 28 of probe round 3 (break-it on mutated real trees, differential forgery
//! checks): regression tests for its four findings (H1 a sticky picker let the app's own
//! content ride a picker grant; H2 a denied `Call` hidden behind a send; M1 a fake
//! `Keyboard` node hid the emergency key; M2 a forged `Cell` frame hid the row) and the
//! reviewer's differential probes, kept as printed records.

mod common;

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_observe::{compact_view, CompactView};
use jarvis_proto::{
    Action, AppInfo, CompactScreen, Direction, Element, Policy, Rect, RiskCategory, Role,
    RunnerCommand, Screen, UiNode,
};
use jarvis_safety::{
    evaluate_sheeted, home_view, max_risk, observed_places, Decision, FlowContext, InputKind,
    ScreenPlaces, ScreenSource, ScreenTree, EMERGENCY_CALL_NOTE, ROW_SWIPE_NOTE,
};

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("fixtures/ios26_probes3/", $name))
    };
}

const P4D_BEFORE: &str = fixture!("p4d-controls-02-controls-before-swipes-runner-tree.txt");
const SEND: &str = fixture!("p4b-returnkeys-08-return-send-tree.txt");
const PLAIN: &str = fixture!("p4b-returnkeys-01-return-default-tree.txt");
const EMERGENCY: &str = fixture!("p4b-returnkeys-12-return-emergencyCall-tree.txt");
const CONTINUE: &str = fixture!("p4b-returnkeys-13-return-continue-tree.txt");
const CONTACT_PICKER: &str = fixture!("p2-pickers-05-contact-picker-tree.txt");
const MODAL_BASE: &str = fixture!("p4c-modals-01-base-tree.txt");
const BASIC_AUTH: &str = fixture!("p6b-safari-inputs-13-basic-auth-tree.txt");
const LANDSCAPE: &str = fixture!("p7-landscape-02-landscape-tree.txt");

fn compact_text(c: &CompactScreen) -> String {
    format!("{c:?}")
}
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
fn rank(d: &Decision) -> u8 {
    match d {
        Decision::Deny(_) => 3,
        Decision::Confirm(r) if r.phone_only => 2,
        Decision::Confirm(_) => 1,
        _ => 0,
    }
}
fn btn(label: &str, id: Option<&str>, f: Rect) -> UiNode {
    let mut b = UiNode::new(Role::Button, f);
    b.label = Some(label.into());
    b.identifier = id.map(str::to_owned);
    b
}
/// A fake `Keyboard` an app puts in its own window (first in tree order), holding one key.
fn with_fake_keyboard(s: &Screen, key: UiNode, kb_frame: Rect) -> Screen {
    let mut s = s.clone();
    let kb = UiNode::new(Role::Keyboard, kb_frame).with_children(vec![key]);
    s.root.children[0].children.insert(0, kb);
    s
}
fn as_textview(s: &Screen) -> Screen {
    let mut s = s.clone();
    edit(&mut s.root, &mut |n| {
        if n.focused && n.role.is_text_input() {
            n.role = Role::TextView;
        }
    });
    s
}

// ---- Return keys ---------------------------------------------------------------------------

#[test]
fn rk_print_fake_keyboard_cues() {
    for (name, text) in [
        ("send", SEND),
        ("emergency", EMERGENCY),
        ("continue", CONTINUE),
    ] {
        let s = parse(text);
        let tiny = Rect::new(439.0, 955.0, 1.0, 1.0);
        let f = with_fake_keyboard(&s, btn("return", Some("Return"), tiny), tiny);
        let a = compact_view(&s).screen.cues;
        let b = compact_view(&f).screen.cues;
        println!(
            "{name}: real {:?}/{:?}/{} fake {:?}/{:?}/{}",
            a.return_key,
            a.return_key_type,
            a.return_key_unknown,
            b.return_key,
            b.return_key_type,
            b.return_key_unknown
        );
    }
}

#[test]
fn rk_fake_keyboard_differential() {
    let tiny = Rect::new(439.0, 955.0, 1.0, 1.0);
    let deny_send = Policy {
        deny_categories: vec![RiskCategory::SendMessage],
        ..default_policy()
    };
    let deny_call = Policy {
        deny_categories: vec![RiskCategory::Call],
        ..default_policy()
    };
    for (name, text) in [
        ("send", SEND),
        ("emergency", EMERGENCY),
        ("continue", CONTINUE),
    ] {
        for tv in [false, true] {
            let mut s = parse(text);
            if tv {
                s = as_textview(&s);
            }
            let f = with_fake_keyboard(&s, btn("return", Some("Return"), tiny), tiny);
            for (pn, p) in [
                ("default", default_policy()),
                ("relaxed", relaxed()),
                ("deny_send", deny_send.clone()),
                ("deny_call", deny_call.clone()),
            ] {
                let a = return_in(&s, "hello");
                let d0 = decide_owned(&p, &a, &s, "type hello");
                let d1 = decide_owned(&p, &return_in(&f, "hello"), &f, "type hello");
                let tag = if rank(&d1) < rank(&d0) {
                    "LOOSER"
                } else {
                    "ok"
                };
                println!(
                    "{tag} {name} tv={tv} {pn}: real {:?} r{} | fake {:?} r{}",
                    asked(&d0),
                    rank(&d0),
                    asked(&d1),
                    rank(&d1)
                );
            }
        }
    }
}

#[test]
fn rk_emergency_key_with_call_denied_is_not_refused() {
    let s = parse(EMERGENCY);
    let a = return_in(&s, "hello");
    let deny_call = Policy {
        deny_categories: vec![RiskCategory::Call],
        ..default_policy()
    };
    let d = decide_owned(&deny_call, &a, &s, "type hello");
    println!("{d:?}");
    // The same key in a search field (no presumed send): refused.
    let mut search = s.clone();
    edit(&mut search.root, &mut |n| {
        if n.focused && n.role.is_text_input() {
            n.role = Role::SearchField;
        }
    });
    let d2 = decide_owned(
        &deny_call,
        &return_in(&search, "hello"),
        &search,
        "type hello",
    );
    println!("search: {d2:?}");
    assert!(matches!(d2, Decision::Deny(_)));
    assert!(
        matches!(d, Decision::Deny(_)),
        "Call denied, Return on the emergency key asked: {d:?}"
    );
}

// ---- Rows ----------------------------------------------------------------------------------

fn swipe(dir: Direction, el: Option<u32>) -> Action {
    Action::Swipe {
        direction: dir,
        element: el,
    }
}
fn scroll(dir: Direction, el: Option<u32>) -> Action {
    Action::Scroll {
        direction: dir,
        element: el,
    }
}

/// Mutates the `Cell` holding "Two" (the segmented control's row).
fn mutate_two_cell(f: impl Fn(&mut UiNode)) -> Screen {
    let mut s = parse(P4D_BEFORE);
    edit(&mut s.root, &mut |n| {
        if n.role == Role::Cell && (n.frame.y - 509.3).abs() < 0.01 {
            f(n);
        }
    });
    s
}

#[test]
fn row_cell_frame_forgery_differential() {
    let base = parse(P4D_BEFORE);
    let muts: Vec<(&str, Screen)> = vec![
        (
            "zero",
            mutate_two_cell(|n| n.frame = Rect::new(0.0, 0.0, 0.0, 0.0)),
        ),
        ("thin", mutate_two_cell(|n| n.frame.height = 1.0)),
        (
            "offscreen",
            mutate_two_cell(|n| n.frame = Rect::new(-500.0, 509.3, 400.0, 61.0)),
        ),
        (
            "tall",
            mutate_two_cell(|n| {
                n.frame.y = 100.0;
                n.frame.height = 900.0;
            }),
        ),
        ("hidden", mutate_two_cell(|n| n.visible = false)),
        (
            "cell_zero_inner_narrow",
            mutate_two_cell(|n| {
                n.frame = Rect::new(0.0, 0.0, 0.0, 0.0);
                for c in &mut n.children {
                    edit(c, &mut |k| {
                        if k.frame.width > 340.0 {
                            k.frame.x = 50.0;
                            k.frame.width = 340.0;
                        }
                    });
                }
            }),
        ),
        (
            "cell_thin_inner_narrow",
            mutate_two_cell(|n| {
                n.frame.height = 1.0;
                for c in &mut n.children {
                    edit(c, &mut |k| {
                        if k.frame.width > 340.0 {
                            k.frame.x = 50.0;
                            k.frame.width = 340.0;
                        }
                    });
                }
            }),
        ),
        (
            "cell_kept_inner_narrow",
            mutate_two_cell(|n| {
                for c in &mut n.children {
                    edit(c, &mut |k| {
                        if k.frame.width > 340.0 {
                            k.frame.x = 50.0;
                            k.frame.width = 340.0;
                        }
                    });
                }
            }),
        ),
        (
            "retype_other",
            mutate_two_cell(|n| n.role = Role::Other("Other".into())),
        ),
        ("wrap_in_scroll_wide", {
            let mut s = parse(P4D_BEFORE);
            edit(&mut s.root, &mut |n| {
                if n.role == Role::Cell && (n.frame.y - 509.3).abs() < 0.01 {
                    n.role = Role::Other("Other".into());
                    let kids = std::mem::take(&mut n.children);
                    let mut sv =
                        UiNode::new(Role::ScrollView, Rect::new(-1000.0, 509.3, 3000.0, 61.0));
                    sv.children = kids;
                    n.children = vec![sv];
                }
            });
            s
        }),
    ];
    for (dir_name, mk) in [
        (
            "swipeL",
            (|e| swipe(Direction::Left, e)) as fn(Option<u32>) -> Action,
        ),
        ("swipeR", |e| swipe(Direction::Right, e)),
        ("scrollL", |e| scroll(Direction::Left, e)),
        ("scrollR", |e| scroll(Direction::Right, e)),
    ] {
        let a = mk(Some(index(&base, "Two")));
        let d0 = decide(&default_policy(), &a, &base);
        for (mn, m) in &muts {
            let c = compact_view(&seen(m)).screen;
            let Some(e) = c.elements.iter().find(|e| e.label == "Two") else {
                println!("{mn}: Two not listed");
                continue;
            };
            let a1 = mk(Some(e.index));
            let d1 = decide(&default_policy(), &a1, m);
            let tag = if rank(&d1) < rank(&d0) {
                "LOOSER"
            } else {
                "ok"
            };
            println!(
                "{tag} {dir_name} {mn}: real {:?}/{} mutated {:?}/{} note={}",
                asked(&d0),
                rank(&d0),
                asked(&d1),
                rank(&d1),
                summary(&d1).contains(ROW_SWIPE_NOTE)
            );
        }
    }
}

// ---- Picker stickiness ---------------------------------------------------------------------

const HALF_SHEET: &str = fixture!("p4c-modals-10-half-sheet-tree.txt");

fn acme(mut s: Screen) -> Screen {
    s.app_bundle_id = "com.example.acme".into();
    s.app_name = Some("Acme".into());
    s.root.label = Some("Acme".into());
    s
}

fn lookup_config() -> jarvis_proto::SessionConfig {
    let mut c = config();
    c.known_apps.push(AppInfo::new("com.example.acme", "Acme"));
    c.tools.push(jarvis_proto::ToolSpec {
        name: "lookup".into(),
        description: "Looks up".into(),
        params: vec![jarvis_proto::ToolParam {
            name: "q".into(),
            kind: jarvis_proto::ParamKind::String,
            required: true,
            description: "q".into(),
        }],
        tier: jarvis_proto::Tier::Native,
        risk: RiskCategory::None,
        app: None,
        self_contained: true,
    });
    c
}

/// Picker in Acme -> tool call approved (picker -> lookup) -> tap a contact -> Acme's own
/// half sheet (its own content, with the presentation backdrop) -> tool call with that
/// content. `backdrop`: whether Acme's sheet keeps the backdrop node.
fn sticky_run(backdrop: bool) -> Run {
    let picker = acme(parse(CONTACT_PICKER));
    let mut sheet = acme(parse(HALF_SHEET));
    if !backdrop {
        edit(&mut sheet.root, &mut |n| {
            if n.frame.width > 1300.0 {
                n.frame = Rect::new(0.0, 0.0, 440.0, 956.0);
            }
        });
    }
    let john = index(&picker, "John Appleseed");
    let s2 = sheet.clone();
    let phone = Phone::new(picker, move |cmd, _| match cmd {
        RunnerCommand::Tap { .. } => Transition::Go(s2.clone()),
        _ => Transition::Stay,
    });
    run(
        lookup_config(),
        "Look up John Appleseed, then look up Probe half sheet body text",
        MemoryStore::new(),
        phone,
        Script::model(&[
            r#"{"action":"call_tool","name":"lookup","args":{"q":"John Appleseed"}}"#,
            &format!(r#"{{"action":"tap","element":{john}}}"#),
            r#"{"action":"wait","ms":1000}"#,
            r#"{"action":"call_tool","name":"lookup","args":{"q":"Probe half sheet body text"}}"#,
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[true; 12]),
    )
}

#[test]
fn picker_sticky_app_sheet_rides_picker_grant() {
    for b in [false, true] {
        let r = sticky_run(b);
        println!("backdrop={b}: {} confirms", r.confirms.len());
        for c in &r.confirms {
            println!("   {:?} {}", c.category, c.summary);
        }
        println!("   tool calls: {:?}", r.tool_calls);
    }
    let with = sticky_run(true);
    let without = sticky_run(false);
    let asks_last = |r: &Run| {
        r.confirms
            .iter()
            .any(|c| c.summary.contains("Probe half sheet body text"))
    };
    assert_eq!(
        asks_last(&with),
        asks_last(&without),
        "sticky picker changed whether the second tool call asks"
    );
}

#[test]
fn rk_fake_keyboard_hides_the_emergency_key() {
    let s = parse(EMERGENCY);
    let at = Rect::new(439.0, 955.0, 1.0, 1.0);
    let fake = with_fake_keyboard(&s, btn("return", Some("Return"), at), at);
    // The fake keyboard no longer wins (it did on a2fff97: the cue read `return`): every
    // keyboard is read, the emergency key kept, and two keyboards make Return unknown.
    let cues = compact_view(&fake).screen.cues;
    assert_eq!(cues.return_key_type.as_deref(), Some("emergency call"));
    assert!(cues.return_key_unknown);
    let d0 = decide_owned(&relaxed(), &return_in(&s, "hello"), &s, "type hello");
    let d1 = decide_owned(&relaxed(), &return_in(&fake, "hello"), &fake, "type hello");
    assert!(asked(&d0).is_some(), "{d0:?}");
    assert!(
        asked(&d1).is_some(),
        "the fake keyboard let Return on the emergency key through: {d1:?}"
    );
}

#[test]
fn row_forged_cell_frame_hides_the_row() {
    let base = parse(P4D_BEFORE);
    // The segmented control's row: its `Cell` 1 pt high, its content inset to 340 of 440.
    let forged = mutate_two_cell(|n| {
        n.frame.height = 1.0;
        for c in &mut n.children {
            edit(c, &mut |k| {
                if k.frame.width > 340.0 {
                    k.frame.x = 50.0;
                    k.frame.width = 340.0;
                }
            });
        }
    });
    let a0 = swipe(Direction::Left, Some(index(&base, "Two")));
    let a1 = swipe(Direction::Left, Some(index(&forged, "Two")));
    assert_eq!(
        asked(&decide(&default_policy(), &a0, &base)),
        Some(RiskCategory::Delete)
    );
    let d = decide(&default_policy(), &a1, &forged);
    assert_eq!(
        asked(&d),
        Some(RiskCategory::Delete),
        "row swipe no longer asked: {d:?}"
    );
}

#[test]
fn row_inset_content_without_cell_role() {
    // Natural layout (no frame forgery): rows are `Other` (React Native), content 340 of 440.
    let mut s = parse(P4D_BEFORE);
    edit(&mut s.root, &mut |n| {
        if n.role == Role::Cell {
            n.role = Role::Other("Other".into());
        }
        if n.frame.width > 340.0 && n.frame.width < 440.0 && n.frame.height < 100.0 {
            n.frame.x = 50.0;
            n.frame.width = 340.0;
        }
    });
    let c = compact_view(&s).screen;
    let row = c
        .elements
        .iter()
        .find(|e| e.label == "Swipe row 2")
        .unwrap();
    let d = decide(
        &default_policy(),
        &swipe(Direction::Left, Some(row.index)),
        &s,
    );
    println!("inset RN row: {:?} {}", asked(&d), summary(&d));
}

/// Review pass 28 (H1), the forged variant: the app's own screen carrying the contact
/// picker's identifier and a backdrop-sized node. Its content is the app's too, so a tool
/// call carrying it after an approval for something else is asked at least as on the same
/// screen without the forgery (differential), nothing more is sent, and the question names
/// the app.
fn forged_run(forge: bool) -> Run {
    let mut s = acme(parse(MODAL_BASE));
    if forge {
        let size = Rect::new(0.0, 0.0, 440.0, 956.0);
        let mut id = UiNode::new(Role::Other("Other".into()), size);
        id.identifier = Some("ContactsListView".into());
        let back = UiNode::new(
            Role::Other("Other".into()),
            Rect::new(-440.0, -956.0, 1320.0, 2868.0),
        );
        s.root.children[0].children.insert(0, id);
        s.root.children[0].children.insert(0, back);
    }
    run(
        lookup_config(),
        "Look up what this screen says",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            r#"{"action":"call_tool","name":"lookup","args":{"q":"Modals"}}"#,
            r#"{"action":"wait","ms":1000}"#,
            r#"{"action":"call_tool","name":"lookup","args":{"q":"Show action sheet"}}"#,
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[true; 12]),
    )
}

/// Review pass 28 (H2), generalised: every category a rule attaches is carried, so an owner
/// who denies `Delete` is refused a sideways swipe on a row that reads as a purchase (the
/// riskier `Purchase` hid the row-swipe `Delete` on a2fff97: asked instead of refused).
#[test]
fn a_denied_row_swipe_is_refused_behind_a_riskier_label() {
    let mut s = parse(P4D_BEFORE);
    edit(&mut s.root, &mut |n| {
        if n.label.as_deref() == Some("Swipe row 1") {
            n.label = Some("Buy now".into());
        }
    });
    let row = index(&s, "Buy now");
    let deny_delete = Policy {
        deny_categories: vec![RiskCategory::Delete],
        ..default_policy()
    };
    let d = decide(&deny_delete, &swipe(Direction::Left, Some(row)), &s);
    assert!(matches!(d, Decision::Deny(_)), "{d:?}");
    // Without the denial it asks as the riskier category.
    let d = decide(&default_policy(), &swipe(Direction::Left, Some(row)), &s);
    assert_eq!(asked(&d), Some(RiskCategory::Purchase), "{d:?}");
}

#[test]
fn forged_picker_content_is_asked_like_the_apps() {
    let plain = forged_run(false);
    let forged = forged_run(true);
    let tool_questions = |r: &Run| {
        r.confirms
            .iter()
            .filter(|c| c.summary.contains("lookup"))
            .count()
    };
    assert!(tool_questions(&plain) > 0, "{:#?}", plain.confirms);
    assert!(
        tool_questions(&forged) >= tool_questions(&plain),
        "plain {:#?}\nforged {:#?}",
        plain.confirms,
        forged.confirms
    );
    assert!(forged.tool_calls.len() <= plain.tool_calls.len());
    // Every question of the plain run is asked on the forged one too.
    for c in &plain.confirms {
        let head = c.summary.split(". ").next().unwrap_or_default();
        assert!(
            forged.confirms.iter().any(|f| f.summary.starts_with(head)),
            "{head}: {:#?}",
            forged.confirms
        );
    }
    for c in forged
        .confirms
        .iter()
        .filter(|c| c.summary.contains("what it read there"))
    {
        assert!(c.summary.contains("Acme"), "{}", c.summary);
    }
}

// ---------------------------------------------------------------------------------------
// Review pass 29: four findings, each with its regression test.
// ---------------------------------------------------------------------------------------

fn policy_without(cats: &[RiskCategory]) -> Policy {
    let mut p = default_policy();
    p.confirm.retain(|c| !cats.contains(c));
    p
}

/// Pass 29 (M1): the confirm side of the carried set. An owner relaxed `Delete` (not
/// `SendMessage`). A button whose OS symbol is a paper plane (a send) asks; forged to read
/// "Delete" it ran unasked (the riskiest category, relaxed, was the only one consulted).
#[test]
fn relaxed_max_category_hides_a_carried_send() {
    let s = parse(fixture!("p4a-symbols-01-symbols-tree.txt"));
    let p = policy_without(&[RiskCategory::Delete]);
    let mk = |label: &str| {
        let mut m = s.clone();
        edit(&mut m.root, &mut |n| {
            if n.identifier.as_deref() == Some("paperplane") {
                n.label = Some(label.into());
            }
        });
        m
    };
    let plain = mk("Plane");
    let d0 = decide(&p, &tap(&plain, "Plane"), &plain);
    assert!(asked(&d0).is_some(), "{d0:?}");
    for label in ["Delete", "Remove", "Send"] {
        let forged = mk(label);
        let d1 = decide(&p, &tap(&forged, label), &forged);
        assert!(rank(&d1) >= rank(&d0), "{label}: {d0:?} -> {d1:?}");
        assert_eq!(
            asked(&d1),
            Some(RiskCategory::SendMessage),
            "{label}: {d1:?}"
        );
    }
    // The riskier relaxed category is named, and the question is phone only (a carried
    // `Delete` is never approved remotely).
    let forged = mk("Delete");
    match decide(&p, &tap(&forged, "Delete"), &forged) {
        Decision::Confirm(r) => {
            assert!(
                r.summary.contains("It also reads as: Delete"),
                "{}",
                r.summary
            );
            assert!(r.phone_only);
        }
        other => panic!("{other:?}"),
    }
}

/// "Total due $900.00" on the symbols screen, and with `fake_kb` a 1 x 1 `Keyboard` node an
/// app draws at y 590.
fn symbols_with_money(fake_kb: bool) -> Screen {
    let mut s = parse(fixture!("p4a-symbols-01-symbols-tree.txt"));
    edit(&mut s.root, &mut |n| {
        if n.role == Role::StaticText && n.label.as_deref() == Some("UIKit toolbar items") {
            n.label = Some("Total due $900.00".into());
        }
    });
    if fake_kb {
        let at = Rect::new(439.0, 590.0, 1.0, 1.0);
        let kb = UiNode::new(Role::Keyboard, at);
        let last = s.root.children[0].children.len();
        s.root.children[0].children.insert(last, kb);
    }
    s
}

/// Pass 29 (M2): a 1 x 1 `Keyboard` node made the strip from its top to the screen's bottom
/// the keyboard's area, which hid "Total due $900.00" from R3: Confirm(Payment, phone only)
/// became Confirm(SendMessage), or Allow with sends relaxed. Now only a plausible system
/// keyboard gets the strip (the odd one makes Return of unknown effect), and R3 reads drawn
/// raw text too, so covering it cannot lower the decision; a `Keyboard` node above the
/// screen hides nothing either.
#[test]
fn fake_keyboard_hides_money_from_r3() {
    for p in [
        default_policy(),
        policy_without(&[RiskCategory::SendMessage]),
    ] {
        let s0 = symbols_with_money(false);
        let s1 = symbols_with_money(true);
        let d0 = decide(&p, &tap(&s0, "Send"), &s0);
        let d1 = decide(&p, &tap(&s1, "Send"), &s1);
        assert_eq!(asked(&d0), Some(RiskCategory::Payment), "{d0:?}");
        assert!(rank(&d1) >= rank(&d0), "{d0:?}\n->\n{d1:?}");
        assert_eq!(asked(&d1), asked(&d0), "{d1:?}");
    }
    let s1 = symbols_with_money(true);
    let c = compact_view(&s1).screen;
    assert!(
        c.cues.return_key_unknown,
        "an odd keyboard: Return of unknown effect"
    );
    assert!(
        c.elements.iter().any(|e| e.label.contains("900")),
        "the 1 x 1 node no longer covers the strip below it"
    );
    // A `Keyboard` node above the screen.
    let mut s2 = symbols_with_money(false);
    s2.root.children[0].children.push(UiNode::new(
        Role::Keyboard,
        Rect::new(0.0, -10.0, 440.0, 20.0),
    ));
    let c2 = compact_view(&s2).screen;
    assert!(c2.elements.iter().any(|e| e.label.contains("900")));
    let d2 = decide(&default_policy(), &tap(&s2, "Send"), &s2);
    assert_eq!(asked(&d2), Some(RiskCategory::Payment), "{d2:?}");
    // The real keyboards keep their area (portrait and the rotated landscape one).
    for text in [PLAIN, SEND, EMERGENCY, CONTINUE] {
        let v = compact_view(&parse(text));
        let kb = v.nodes.iter().find(|n| n.role == Role::Keyboard).unwrap();
        assert!(
            kb.frame.max_y() >= 955.0 && kb.frame.y <= 655.0,
            "{:?}",
            kb.frame
        );
    }
    let landscape = compact_view(&parse(fixture!("p7-landscape-04-landscape-after-tree.txt")));
    let kb = landscape
        .nodes
        .iter()
        .find(|n| n.role == Role::Keyboard)
        .unwrap();
    assert!(
        kb.frame.max_y() >= 440.0 && kb.frame.x <= 0.0,
        "{:?}",
        kb.frame
    );
}

/// Pass 29 (M3): Cell ancestry from every raw node's role. A `Cell` whose frame is not
/// drawable (zero, off screen, below the screen, negative-sized, not a number) lost its
/// role, and with its content inset below 0.8 of the scroller the swipe on "Two" went from
/// Confirm(Delete) to Allow.
#[test]
fn cell_undrawable_frames_with_inset_content() {
    let base = parse(P4D_BEFORE);
    let d0 = decide(
        &default_policy(),
        &swipe(Direction::Left, Some(index(&base, "Two"))),
        &base,
    );
    assert_eq!(asked(&d0), Some(RiskCategory::Delete), "{d0:?}");
    let frames = [
        ("zero", Rect::new(0.0, 0.0, 0.0, 0.0)),
        ("offscreen-left", Rect::new(-1000.0, 509.3, 400.0, 61.0)),
        ("below-screen", Rect::new(0.0, 5000.0, 440.0, 61.0)),
        ("negative-size", Rect::new(440.0, 570.3, -440.0, -61.0)),
        ("nan", Rect::new(f64::NAN, f64::NAN, f64::NAN, f64::NAN)),
    ];
    for (name, fr) in frames {
        let m = mutate_two_cell(|n| {
            n.frame = fr;
            for c in &mut n.children {
                edit(c, &mut |k| {
                    if k.frame.width > 340.0 {
                        k.frame.x = 50.0;
                        k.frame.width = 340.0;
                    }
                });
            }
        });
        let d = decide(
            &default_policy(),
            &swipe(Direction::Left, Some(index(&m, "Two"))),
            &m,
        );
        assert!(rank(&d) >= rank(&d0), "{name}: {d0:?} -> {d:?}");
        assert_eq!(asked(&d), Some(RiskCategory::Delete), "{name}: {d:?}");
    }
}

/// `k` shown `Keyboard` nodes on the plain Return-key screen.
fn many_keyboards(k: usize) -> Screen {
    let mut s = parse(PLAIN);
    let kids: Vec<UiNode> = (0..k)
        .map(|i| {
            UiNode::new(
                Role::Keyboard,
                Rect::new((i % 400) as f64, 700.0, 10.0, 10.0),
            )
            .with_children(vec![btn(
                "return",
                Some("Return"),
                Rect::new(0.0, 700.0, 5.0, 5.0),
            )])
        })
        .collect();
    s.root.children[0].children.extend(kids);
    s
}

/// Pass 29 (L1): `keyboard_area` walked the whole tree once per keyboard (4000 forged
/// keyboards: 3.3 s, 16x the input took 123x the time). The input hosts are collected once:
/// 16x the keyboards takes well under 48x the time (best of three, so a busy machine does
/// not decide), and Return stays of unknown effect.
#[test]
fn many_keyboards_compact_in_linear_time() {
    let time = |k: usize| {
        let s = many_keyboards(k);
        (0..3)
            .map(|_| {
                let t = std::time::Instant::now();
                let v = compact_view(&s);
                assert!(v.screen.cues.return_key_unknown);
                t.elapsed().as_secs_f64()
            })
            .fold(f64::INFINITY, f64::min)
    };
    let small = time(250);
    let large = time(4000);
    assert!(
        large < small * 48.0 + 0.05,
        "250: {small:.4}s, 4000: {large:.4}s"
    );
}
