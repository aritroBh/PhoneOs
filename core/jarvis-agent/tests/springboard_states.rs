#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Run 146 (Mac, iOS 26.5 simulator), probe P15: the Today view, the App Library, an open
//! folder and the Home Screen's edit mode ("jiggle mode") all read as SpringBoard in front
//! with SpringBoard's tree, which still holds the Home Screen's icon container. The real
//! runner trees (`p15-springboard-*-runner-tree.txt`, what the core received) decide:
//!
//! - Today view: kept as the Home Screen; its widgets are stubs (nothing listed, not even the
//!   news headlines; a widget cannot be tapped, `tap_point` is refused there).
//! - App Library, open folder, edit mode: never the Home Screen. The shell reports them as the
//!   unidentified layer, named; a runner that does not is caught by the core
//!   (`jarvis_safety::home_screen_mode`): the screen is read as one Jarvis cannot read.
//!   Before, the App Library's search field and the folder's name field took typing unasked
//!   (renaming the folder), and the edit mode's icons were tapped as launches.

mod common;

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_observe::compact_view;
use jarvis_proto::{AppInfo, OutcomeStatus, RiskCategory, RunnerCommand, Screen};
use jarvis_safety::{home_screen_mode, home_view, HomeScreenMode};

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("fixtures/ios26_probes3/", $name))
    };
}

fn compact_text(c: &jarvis_proto::CompactScreen) -> String {
    format!("{c:?}")
}

fn parse(text: &str) -> Screen {
    let typed = text
        .split("\n== raw XCUITest snapshot")
        .next()
        .unwrap_or(text);
    ios::parse_screen(typed).unwrap()
}

const HOME: &str = fixture!("p15-springboard-02-home-runner-tree.txt");
const TODAY: &str = fixture!("p15-springboard-04-today-runner-tree.txt");
const LIBRARY: &str = fixture!("p15-springboard-06-app-library-runner-tree.txt");
const FOLDER: &str = fixture!("p15-springboard-08-folder-runner-tree.txt");
const JIGGLE: &str = fixture!("p15-springboard-10-jiggle-runner-tree.txt");
const AFTER: &str = fixture!("p15-springboard-11-home-after-tree.txt");

/// Every P15 capture, raw (the probe's own snapshot) and the runner's, with its mode.
const CAPTURES: [(&str, &str, Option<HomeScreenMode>); 11] = [
    ("home", fixture!("p15-springboard-01-home-tree.txt"), None),
    ("home runner", HOME, None),
    ("today", fixture!("p15-springboard-03-today-tree.txt"), None),
    ("today runner", TODAY, None),
    (
        "library",
        fixture!("p15-springboard-05-app-library-tree.txt"),
        Some(HomeScreenMode::AppLibrary),
    ),
    ("library runner", LIBRARY, Some(HomeScreenMode::AppLibrary)),
    (
        "folder",
        fixture!("p15-springboard-07-folder-tree.txt"),
        Some(HomeScreenMode::Folder),
    ),
    ("folder runner", FOLDER, Some(HomeScreenMode::Folder)),
    (
        "jiggle",
        fixture!("p15-springboard-09-jiggle-tree.txt"),
        Some(HomeScreenMode::EditMode),
    ),
    ("jiggle runner", JIGGLE, Some(HomeScreenMode::EditMode)),
    ("home after", AFTER, None),
];

fn cfg() -> jarvis_proto::SessionConfig {
    let mut c = first_use_config();
    for (id, name) in [
        ("com.apple.calculator", "Calculator"),
        ("com.apple.mobilecal", "Calendar"),
        ("com.apple.news", "News"),
        ("com.apple.Health", "Health"),
        ("com.apple.Maps", "Maps"),
        ("com.apple.Fitness", "Fitness"),
        ("dev.jarvis.app", "Jarvis"),
    ] {
        c.known_apps.push(AppInfo::new(id, name));
    }
    c
}

fn session(screen: &Screen, step: &str) -> Run {
    run(
        cfg(),
        "Open Calculator",
        MemoryStore::new(),
        Phone::still(screen.clone()),
        Script::model(&[step, r#"{"action":"fail","reason":"stop"}"#]).decisions(&[true; 6]),
    )
}

/// Which mode each capture shows; none in the other SpringBoard captures of the probe rounds.
#[test]
fn p15_modes_by_springboards_own_identifiers() {
    for (name, text, mode) in CAPTURES {
        assert_eq!(home_screen_mode(&parse(text).root), mode, "{name}");
    }
    for text in [
        fixture!("p12a-surfaces-01-home-badge-tree.txt"),
        fixture!("p13-swipe-lengths-01-rows-tree.txt"),
    ] {
        assert_eq!(home_screen_mode(&parse(text).root), None);
    }
}

/// The Today view stays the Home Screen with its widgets as stubs: nothing of a widget is
/// listed (no news headline, no calendar text), a widget cannot be tapped, `tap_point` is
/// refused, and nothing is sent for a tap on a stub.
#[test]
fn p15_today_view_shows_no_widget_content() {
    let s = parse(TODAY);
    assert!(
        TODAY.contains("The Wall Street Journal"),
        "the tree holds the headlines"
    );
    let shown = compact_view(&home_view(&s).unwrap()).screen;
    assert!(shown.elements.is_empty(), "{:#?}", shown.elements);
    let r = session(&s, r#"{"action":"tap_point","x":0.3,"y":0.2}"#);
    let all = r.all_prompt_text();
    for word in [
        "Wall Street Journal",
        "Top Stories",
        "No Events Today",
        "Trending",
    ] {
        assert!(!all.contains(word), "{word}");
    }
    assert!(r.acts.is_empty(), "{:?}", r.acts);
    assert!(all.contains("tap_point needs a screenshot"), "{all}");
}

/// The plain Home Screen before and after: icons listed, a known app's icon launches.
#[test]
fn p15_plain_home_screen_is_unchanged() {
    for text in [HOME, AFTER] {
        let s = parse(text);
        // The model numbers the Home Screen as shown (widgets as stubs).
        let maps = compact_view(&home_view(&s).unwrap())
            .screen
            .elements
            .iter()
            .find(|e| e.label == "Maps")
            .unwrap()
            .index;
        let r = session(&s, &format!(r#"{{"action":"tap","element":{maps}}}"#));
        assert!(
            r.acts
                .iter()
                .any(|a| matches!(a, RunnerCommand::Tap { .. })),
            "{:?}",
            r.acts
        );
        assert!(r.confirms.is_empty(), "{:#?}", r.confirms);
        assert!(r.requests[0].user.contains("com.apple.springboard"));
        assert!(!r.requests[0].user.contains("cannot identify"));
    }
}

/// App Library, open folder, edit mode: read as the unidentified layer — nothing of them is
/// listed (no search field, pod, folder name, Edit/Done), nothing is tapped or typed, every
/// input asks, and the only things sent after approval are what the owner approved there.
#[test]
fn p15_library_folder_and_edit_mode_are_not_the_home_screen() {
    for (name, text) in [("library", LIBRARY), ("folder", FOLDER), ("jiggle", JIGGLE)] {
        let s = parse(text);
        // Before: these were listed and tapped or typed into as the Home Screen's.
        let before = compact_view(&home_view(&s).unwrap()).screen;
        assert!(!before.elements.is_empty(), "{name}");
        let first = before.elements[0].clone();
        for step in [
            format!(r#"{{"action":"tap","element":{}}}"#, first.index),
            format!(
                r#"{{"action":"type","element":{},"text":"Calculator","submit":false}}"#,
                first.index
            ),
            r#"{"action":"tap_point","x":0.08,"y":0.1}"#.to_owned(),
        ] {
            let r = run(
                cfg(),
                "Open Calculator",
                MemoryStore::new(),
                Phone::still(s.clone()),
                Script::model(&[&step, r#"{"action":"fail","reason":"stop"}"#])
                    .decisions(&[true; 6]),
            );
            assert!(r.acts.is_empty(), "{name} {step}: {:?}", r.acts);
            let first_prompt = &r.requests[0].user;
            assert!(
                first_prompt.contains("an app Jarvis cannot identify"),
                "{name}: {first_prompt}"
            );
            for word in [
                "App Library",
                "Utilities",
                "Folder name",
                "Edit",
                "Done",
                "Fitness",
                "Other",
            ] {
                assert!(
                    !first_prompt.contains(word),
                    "{name}: {word}: {first_prompt}"
                );
            }
        }
        // A swipe asks (declined: nothing sent).
        let r = run(
            cfg(),
            "Open Calculator",
            MemoryStore::new(),
            Phone::still(s.clone()),
            Script::model(&[
                r#"{"action":"swipe","direction":"left"}"#,
                r#"{"action":"fail","reason":"stop"}"#,
            ]),
        );
        assert!(!r.confirms.is_empty(), "{name}");
        assert!(r.acts.is_empty(), "{name}: {:?}", r.acts);
        assert_eq!(r.outcome.status, OutcomeStatus::Failed);
    }
}

/// Edit mode: whatever a runner reports, no tap there is a launch: a tap on an icon whose
/// label is a known app ("Fitness") is not sent as the Home Screen's launch, and the
/// `DeleteButton` of every icon is never reachable.
#[test]
fn p15_edit_mode_never_launches_or_deletes_unasked() {
    let s = parse(JIGGLE);
    let fitness = index_of(&s, "Fitness");
    let r = run(
        cfg(),
        "Open Fitness",
        MemoryStore::new(),
        Phone::still(s.clone()),
        Script::model(&[
            &format!(r#"{{"action":"tap","element":{fitness}}}"#),
            r#"{"action":"fail","reason":"stop"}"#,
        ]),
    );
    assert!(r.acts.is_empty(), "{:?}", r.acts);
    assert!(
        r.confirms
            .iter()
            .all(|c| c.category != RiskCategory::None || c.phone_only),
        "{:#?}",
        r.confirms
    );
}

fn edit(n: &mut jarvis_proto::UiNode, f: &mut impl FnMut(&mut jarvis_proto::UiNode)) {
    f(n);
    for c in &mut n.children {
        edit(c, f);
    }
}

/// Review pass 30 (L1): an icon's identifier and label are the app's display name, and a
/// widget's content is a third party's: an app named "DeleteButton" or "Notes, Folder name",
/// or a widget holding a node with such an identifier, makes no mode of the Home Screen; the
/// real states are still recognised, the edit mode also next to such an icon.
#[test]
fn p15_markers_are_read_only_from_springboards_own_nodes() {
    let forged_icons = |text: &str| {
        let mut s = parse(text);
        let mut k = 0;
        edit(&mut s.root, &mut |n| {
            if n.label.as_deref() == Some("Maps") && n.value.is_none() {
                let name = if k == 0 {
                    "DeleteButton"
                } else {
                    "Notes, Folder name"
                };
                k += 1;
                n.label = Some(name.into());
                n.identifier = Some(name.into());
            }
            if n.label.as_deref() == Some("Health") && n.value.is_none() {
                n.label = Some("dewey-pod-view".into());
                n.identifier = Some("dewey-pod-view".into());
            }
        });
        s
    };
    let home = forged_icons(HOME);
    assert_eq!(home_screen_mode(&home.root), None);
    // A widget holding an unlabelled `Button id=DeleteButton` and a folder-name field.
    let mut widget = parse(HOME);
    edit(&mut widget.root, &mut |n| {
        if n.value.as_deref().is_some_and(|v| v.starts_with("Widget")) {
            let mut icon = jarvis_proto::UiNode::new(jarvis_proto::Role::Button, n.frame);
            icon.children.push(
                jarvis_proto::UiNode::new(jarvis_proto::Role::Button, n.frame)
                    .with_identifier("DeleteButton"),
            );
            n.children.push(icon);
            n.children.push(
                jarvis_proto::UiNode::new(jarvis_proto::Role::TextField, n.frame)
                    .with_identifier("Utilities, Folder name"),
            );
        }
    });
    assert_eq!(home_screen_mode(&widget.root), None);
    // The session still reads the forged Home Screen as the Home Screen.
    let r = session(&home, r#"{"action":"fail","reason":"stop"}"#);
    assert!(
        !r.requests[0].user.contains("cannot identify"),
        "{}",
        r.requests[0].user
    );
    // The real states, the edit mode next to an icon named "DeleteButton".
    assert_eq!(
        home_screen_mode(&forged_icons(JIGGLE).root),
        Some(HomeScreenMode::EditMode)
    );
    assert_eq!(
        home_screen_mode(&parse(LIBRARY).root),
        Some(HomeScreenMode::AppLibrary)
    );
    assert_eq!(
        home_screen_mode(&parse(FOLDER).root),
        Some(HomeScreenMode::Folder)
    );
}

/// Pass 30 (the reviewer's open question, pinned): on the real Today-view tree the model is
/// shown no element at all — no widget text, no widget identifier, no headline — and nothing
/// can be tapped; a swipe and a scroll are sent unasked as on the plain Home Screen (the OS
/// sees them, no third party), and no question carries widget content.
#[test]
fn p15_today_view_lists_nothing_and_its_swipes_ask_nothing() {
    let s = parse(TODAY);
    let shown = compact_view(&home_view(&s).unwrap()).screen;
    assert!(shown.elements.is_empty(), "{:#?}", shown.elements);
    let widget_words = [
        "Wall Street Journal",
        "Top Stories",
        "Trending",
        "Featured",
        "No Events Today",
        "iconic-date-header-view",
        "battery-widget",
        "NearbyWidgetMapImage",
        "left-of-home",
    ];
    for step in [
        r#"{"action":"swipe","direction":"left"}"#,
        r#"{"action":"scroll","direction":"down"}"#,
        r#"{"action":"tap","element":1}"#,
    ] {
        let r = session(&s, step);
        let all = r.all_prompt_text();
        for w in widget_words {
            assert!(!all.contains(w), "{step}: {w}");
            for c in &r.confirms {
                assert!(!c.summary.contains(w), "{step}: {w} in {}", c.summary);
            }
        }
        if step.contains("tap") {
            assert!(r.acts.is_empty(), "{step}: {:?}", r.acts);
        } else {
            assert!(r.confirms.is_empty(), "{step}: {:#?}", r.confirms);
            assert!(
                r.acts
                    .iter()
                    .any(|a| matches!(a, RunnerCommand::Swipe { .. })),
                "{step}: {:?}",
                r.acts
            );
        }
    }
}
