#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Regression tests on real iOS 26.5 trees (iPhone simulator, XCUITest snapshots), stored as
//! readable text under `tests/fixtures/ios26/`. Each fixture is what the shell's runner hands
//! the core; the tests assert on what the model gets to choose from.

mod common;

use common::ios::{compact_fixture, dump_fixture, fixture, parse_node, parse_screen};
use common::*;
use jarvis_observe::{compact, estimate_tokens, render, resolve};
use jarvis_proto::{CompactScreen, Role};

const FIXTURES: [&str; 10] = [
    "springboard_home",
    "playground_home",
    "toggles_off",
    "safari_start_page",
    "settings_root",
    "settings_general",
    "settings_about",
    "login_keyboard",
    "long_list_scrolled",
    "alert_dialog",
];

fn tokens(c: &CompactScreen) -> u32 {
    estimate_tokens(&render(c, 100_000))
}

fn assert_frame(e: &jarvis_proto::Element, x: f64, y: f64, w: f64, h: f64) {
    let f = e.frame;
    let close = |a: f64, b: f64| (a - b).abs() < 0.06;
    assert!(
        close(f.x, x) && close(f.y, y) && close(f.width, w) && close(f.height, h),
        "{:?} {:?} has frame {f:?}, wanted ({x}, {y}, {w}, {h})",
        e.role,
        e.label
    );
}

// ---- parser -------------------------------------------------------------------------------

#[test]
fn parser_reads_every_part_of_a_node_line() {
    let n = parse_node(
        r#"TextField "Say \"hi\"" value="a\nb" placeholder="Name" id=form.name {{16, 40.5}, {361, 44}} [disabled, focused, selected, hidden]"#,
    )
    .unwrap();
    assert_eq!(n.role, Role::TextField);
    assert_eq!(n.label.as_deref(), Some("Say \"hi\""));
    assert_eq!(n.value.as_deref(), Some("a\nb"));
    assert_eq!(n.placeholder.as_deref(), Some("Name"));
    assert_eq!(n.identifier.as_deref(), Some("form.name"));
    assert_eq!(
        (n.frame.x, n.frame.y, n.frame.width, n.frame.height),
        (16.0, 40.5, 361.0, 44.0)
    );
    assert!(!n.enabled && n.focused && n.selected && !n.visible && !n.hittable);

    let o = parse_node("Other(StatusBar) id=Home screen icons {{0, 0}, {440, 54}}").unwrap();
    assert_eq!(o.role, Role::Other("StatusBar".into()));
    assert_eq!(o.identifier.as_deref(), Some("Home screen icons"));
    assert!(o.visible && o.hittable && o.enabled);
}

#[test]
fn parser_builds_the_tree_from_indentation_and_reads_the_header() {
    let s = parse_screen(
        "# comment\nobservation 3: com.example.app\nscreen 390x844 pt, keyboard visible\n\n\
         Application \"Example\" {{0, 0}, {390, 844}}\n  Window {{0, 0}, {390, 844}}\n    \
         Button \"A\" {{0, 0}, {10, 10}}\n    Button \"B\" {{0, 10}, {10, 10}}\n  \
         Window {{0, 0}, {390, 844}}\n",
    )
    .unwrap();
    assert_eq!(s.app_bundle_id, "com.example.app");
    assert_eq!(s.app_name.as_deref(), Some("Example"));
    assert_eq!((s.size.width, s.size.height), (390.0, 844.0));
    assert!(s.keyboard_visible);
    assert_eq!(s.root.children.len(), 2);
    assert_eq!(s.root.children[0].children.len(), 2);
    assert!(parse_screen("Application {{0, 0}, {1, 1}}\n    Button {{0, 0}, {1, 1}}\n").is_err());
    assert!(parse_node("Button \"x\" {{0, 0}, {1}}").is_err());
}

#[test]
fn fixtures_parse_with_the_real_screen_size() {
    for name in FIXTURES {
        let s = fixture(name);
        assert_eq!((s.size.width, s.size.height), (440.0, 956.0), "{name}");
        assert_eq!(s.root.role, Role::Application, "{name}");
        assert!(!s.root.children.is_empty(), "{name}");
    }
}

// ---- every real tree ----------------------------------------------------------------------

#[test]
fn every_listed_element_is_on_screen_and_indices_are_dense() {
    for name in FIXTURES {
        let c = compact_fixture(name);
        assert!(!c.elements.is_empty(), "{name}");
        let bounds = fixture(name).bounds();
        for (i, e) in c.elements.iter().enumerate() {
            assert_eq!(e.index as usize, i + 1, "{name}");
            assert!(!e.frame.is_empty(), "{name}: {e:?}");
            assert!(
                e.frame.x >= bounds.x
                    && e.frame.y >= bounds.y
                    && e.frame.max_x() <= bounds.max_x()
                    && e.frame.max_y() <= bounds.max_y(),
                "{name}: {} is off screen: {:?}",
                e.label,
                e.frame
            );
            let tap = e.frame.center();
            assert!(
                bounds.contains(tap),
                "{name}: tap point of {} off screen",
                e.label
            );
            assert!(
                !e.label.to_lowercase().contains("scroll bar"),
                "{name}: scroll bar listed"
            );
        }
    }
}

#[test]
fn compaction_of_real_trees_is_deterministic() {
    for name in FIXTURES {
        assert_eq!(dump_fixture(name), dump_fixture(name), "{name}");
    }
}

// ---- bug 1: controls under a zero-size hidden parent ---------------------------------------

#[test]
fn springboard_icons_under_a_hidden_zero_size_page_are_listed() {
    let c = compact_fixture("springboard_home");
    let labels = labels(&c);
    for icon in [
        "Photos",
        "Reminders",
        "News",
        "Health",
        "Wallet",
        "Settings",
    ] {
        assert_eq!(count_label(&c, icon), 1, "{icon} in {labels:?}");
    }
    // Widgets and icons with the same name are different controls.
    assert_eq!(count_label(&c, "Maps"), 2, "{labels:?}");
    assert_eq!(count_label(&c, "Calendar"), 2, "{labels:?}");
    // The dock and the status bar are still there.
    assert_eq!(count_label(&c, "Safari"), 1);
    assert_eq!(count_label(&c, "Messages"), 1);

    let settings = by_label(&c, "Settings");
    assert_eq!(settings.role, Role::Button);
    assert_frame(settings, 309.7, 418.0, 72.0, 94.7);
    assert!(settings.enabled);
    // The tap point is the middle of the icon.
    let (x, y) = tap_point(settings);
    assert!(
        (x - 345.7).abs() < 0.1 && (y - 465.35).abs() < 0.1,
        "{x},{y}"
    );
    assert_eq!(resolve(&c, settings.index).unwrap().label, "Settings");
}

#[test]
fn springboard_does_not_resurrect_off_screen_pages_or_zero_size_icons() {
    let c = compact_fixture("springboard_home");
    let labels = labels(&c);
    // Next page: hidden with a real frame at x = 447.7 on a 440-wide screen.
    for gone in ["Fitness", "Preview"] {
        assert_eq!(count_label(&c, gone), 0, "{gone} listed: {labels:?}");
    }
    // Icons with {{0, 0}, {0, 0}} (folders, apps on page 2) and the folder's children.
    for gone in [
        "Watch",
        "Contacts",
        "Files",
        "Utilities folder",
        "Passwords",
        "Shortcuts",
        "Jarvis",
        "JarvisUITests-Runner",
    ] {
        assert_eq!(count_label(&c, gone), 0, "{gone} listed: {labels:?}");
    }
    assert!(
        c.elements.iter().all(|e| e.frame.x >= 0.0),
        "left-of-home page leaked"
    );
    // 2 widgets, 8 icons, spotlight image, 2 dock icons. The status bar (3 items) is
    // excluded on SpringBoard (fifth review).
    assert_eq!(c.elements.len(), 13, "{labels:?}");
}

#[test]
fn settings_search_field_under_a_hidden_zero_size_wrapper_is_listed() {
    let c = compact_fixture("settings_root");
    let search = c
        .elements
        .iter()
        .find(|e| e.role == Role::SearchField)
        .expect("the search field of the toolbar");
    assert_eq!(search.label, "Search");
    assert_frame(search, 33.0, 885.0, 374.0, 38.0);
}

// ---- bug 2: nested same-role controls ------------------------------------------------------

#[test]
fn toggle_switch_carries_the_row_label_and_the_inner_frame() {
    let c = compact_fixture("toggles_off");
    let switches: Vec<_> = c
        .elements
        .iter()
        .filter(|e| e.role == Role::Switch)
        .collect();
    assert_eq!(switches.len(), 1, "{}", dump_fixture("toggles_off"));
    let sw = switches[0];
    assert_eq!(sw.label, "Show tips");
    assert_eq!(sw.value.as_deref(), Some("0"));
    assert!(sw.enabled);
    assert_frame(sw, 339.0, 183.0, 63.0, 28.0);
    // Not the row's centre (220, 197), which toggles nothing.
    assert_eq!(tap_point(sw), (370.5, 197.0));
    // The label text of the row is not listed twice and the status cell is still there.
    assert_eq!(count_label(&c, "Show tips"), 1);
    assert_eq!(by_label(&c, "Tips are off").role, Role::Cell);
}

#[test]
fn toggle_fingerprint_does_not_depend_on_the_frame_chosen() {
    use jarvis_observe::find_by_fingerprint;
    let c = compact_fixture("toggles_off");
    let sw = by_label(&c, "Show tips");
    assert_eq!(
        find_by_fingerprint(&c, sw.fingerprint).map(|e| e.index),
        Some(sw.index)
    );
    // Same control seen from a tree without the inner switch: same fingerprint.
    let mut s = fixture("toggles_off");
    fn strip(n: &mut jarvis_proto::UiNode) {
        if n.role == Role::Switch {
            n.children.clear();
        }
        n.children.iter_mut().for_each(strip);
    }
    strip(&mut s.root);
    let plain = compact(&s);
    assert_eq!(by_label(&plain, "Show tips").fingerprint, sw.fingerprint);
    assert_ne!(by_label(&plain, "Show tips").frame, sw.frame);
}

// ---- other real-tree expectations ----------------------------------------------------------

#[test]
fn safari_start_page_lists_the_page_and_the_bottom_bar_without_scroll_bars() {
    let c = compact_fixture("safari_start_page");
    let l = labels(&c);
    for want in [
        "Start Page",
        "Customize Start Page",
        "Favorites",
        "Apple",
        "Bing",
        "Google",
        "Yahoo",
        "Edit",
        "More",
    ] {
        assert_eq!(count_label(&c, want), 1, "{want} in {l:?}");
    }
    assert_eq!(by_label(&c, "Apple").role, Role::Cell);
    assert_eq!(by_label(&c, "Customize Start Page").role, Role::Button);
    let address = by_label(&c, "Address");
    assert_eq!(address.role, Role::TextField);
    assert_eq!(address.value.as_deref(), Some("\u{200e}127.0.0.1"));
}

#[test]
fn settings_rows_keep_their_labels_and_values() {
    let g = compact_fixture("settings_general");
    assert_eq!(by_label(&g, "About").role, Role::Button);
    assert_eq!(by_label(&g, "VPN & Device Management").role, Role::Button);
    assert!(g
        .elements
        .iter()
        .any(|e| e.role == Role::Cell && e.label.starts_with("General, Manage")));

    let a = compact_fixture("settings_about");
    assert_eq!(by_label(&a, "iOS Version, 26.5").role, Role::Button);
    assert_eq!(by_label(&a, "Capacity, 343.07 GB").role, Role::Button);
    assert_eq!(
        by_label(&a, "Certificate Trust Settings").role,
        Role::StaticText
    );
}

#[test]
fn login_form_with_keyboard_hides_what_the_keyboard_covers() {
    let c = compact_fixture("login_keyboard");
    assert!(c.keyboard_visible);
    let user = by_label(&c, "Username");
    assert_eq!(user.value.as_deref(), Some("Ada"));
    assert!(user.focused);
    assert!(by_label(&c, "Password").secure);
    assert_eq!(count_label(&c, "Ask"), 0, "tab bar is under the keyboard");
    // The real tree's return key (`Button "return" id=Return`) becomes a cue, not an
    // element; a plain "return" is not mentioned to the model.
    assert_eq!(c.cues.return_key.as_deref(), Some("return"));
    assert_eq!(count_label(&c, "return"), 0);
    assert!(!render(&c, 100_000).contains("Return key"));
    // Screens without a keyboard have no return-key cue.
    for name in ["settings_root", "springboard_home", "alert_dialog"] {
        assert_eq!(compact_fixture(name).cues.return_key, None, "{name}");
    }
}

#[test]
fn alert_scopes_to_its_own_content() {
    let c = compact_fixture("alert_dialog");
    assert_eq!(
        labels(&c),
        vec![
            "Storage almost full",
            "Free up space to keep syncing.",
            "Dismiss"
        ]
    );
}

// ---- token budget --------------------------------------------------------------------------

/// Token estimates (bytes / 4) of the full render of each fixture, before this change and now.
/// Fixing the hidden-parent bug adds the home screen's icons (51 -> 120 tokens) and the Settings
/// search field. The scroll and page cues add one short line where the tree says there is more
/// (home screen page, Settings lists, the long list, the alert's text); switches shown as
/// `(off)` instead of `value="0"` save a little. Nothing else may grow.
#[test]
fn token_estimates_stay_close_to_the_known_numbers() {
    // (fixture, tokens before the fix, ceiling now)
    let table = [
        ("springboard_home", 51, 131),
        ("playground_home", 71, 71),
        ("toggles_off", 62, 62),
        ("safari_start_page", 149, 149),
        ("settings_root", 119, 133),
        ("settings_general", 178, 185),
        ("settings_about", 134, 134),
        ("login_keyboard", 61, 61),
        ("long_list_scrolled", 151, 164),
        ("alert_dialog", 34, 41),
    ];
    for (name, before, ceiling) in table {
        let t = tokens(&compact_fixture(name));
        println!("{name}: {before} -> {t} tokens");
        assert!(
            t <= ceiling,
            "{name}: {t} tokens (was {before}, ceiling {ceiling})"
        );
        assert!(
            t >= before.min(ceiling) * 9 / 10,
            "{name}: {t} tokens, suspiciously few"
        );
    }
}

// ---- cues for small models ------------------------------------------------------------------

/// The scroll cue comes only from iOS's own scroll indicator (`Vertical scroll bar, N pages`,
/// value = position): two or more pages and a position short of the end. Lists that fit, and
/// screens without an indicator, get no cue.
#[test]
fn scroll_cues_follow_the_scroll_indicator_of_real_trees() {
    use jarvis_observe::{MORE_ABOVE, MORE_BELOW};
    // (fixture, more above, more below)
    let table = [
        ("long_list_scrolled", true, true), // 6 pages at 12%
        ("settings_root", false, true),     // 2 pages at 0%
        ("settings_general", false, true),
        ("alert_dialog", false, true), // the alert's message view: 2 pages at 0%
        ("playground_home", false, false), // 1 page
        ("login_keyboard", false, false),
        ("safari_start_page", false, false),
        ("springboard_home", false, false),
        ("toggles_off", false, false),
    ];
    for (name, above, below) in table {
        let c = compact_fixture(name);
        assert_eq!(
            (c.cues.more_above, c.cues.more_below),
            (above, below),
            "{name}"
        );
        let text = render(&c, 100_000);
        assert_eq!(text.contains(MORE_ABOVE), above, "{name}\n{text}");
        assert_eq!(text.contains(MORE_BELOW), below, "{name}\n{text}");
        if below {
            assert!(text.ends_with(MORE_BELOW), "{name}: cue is the last line");
        }
    }
}

/// SpringBoard's page control ("Page 1 of 2") survives as one line under the header.
#[test]
fn home_screen_keeps_its_page_indicator_as_one_line() {
    let c = compact_fixture("springboard_home");
    assert_eq!(c.cues.page, Some((1, 2)));
    let text = render(&c, 100_000);
    let mut lines = text.lines();
    assert!(lines.next().unwrap().starts_with("App: "));
    assert_eq!(
        lines.next().unwrap(),
        "Page 1 of 2 (swipe left for the next page)"
    );
    assert_eq!(text.matches("Page 1 of 2").count(), 1, "{text}");
    for name in FIXTURES.iter().filter(|n| **n != "springboard_home") {
        assert_eq!(compact_fixture(name).cues.page, None, "{name}");
    }
}

/// Switches read `(on)`/`(off)`; the raw value stays in the element for verification.
#[test]
fn switches_render_as_on_or_off_and_keep_the_raw_value() {
    let c = compact_fixture("toggles_off");
    let switches: Vec<_> = c
        .elements
        .iter()
        .filter(|e| e.role == Role::Switch)
        .collect();
    assert!(!switches.is_empty());
    let text = render(&c, 100_000);
    for s in switches {
        assert_eq!(s.value.as_deref(), Some("0"), "{:?}", s.label);
        assert!(
            text.contains(&format!("[{}] Switch \"{}\" (off)", s.index, s.label)),
            "{text}"
        );
    }
    assert!(!text.contains("value=\"0\""), "{text}");
}

// ---- tap points (sixth review) -----------------------------------------------------------

/// Every listed control on every real screen gets an uncontested tap point: real trees
/// (full-screen `Other` overlays, tab bars whose buttons overlap a little, nested cells) do
/// not make ordinary taps refused. Text inputs included (they are tapped to focus).
///
/// On `long_list_scrolled` the list has scrolled under the translucent navigation and tab
/// bars, so list rows (`Item 7`/`Item 8`, `Item 23`/`Item 24`, buttons themselves) lie
/// under the back button and under all five tab-bar buttons. Bar precedence (iOS draws the
/// bars on top) makes those six taps usable: rows outside the bars do not contest a bar's
/// controls, and the rows are tapped in their part outside the bars. Until bar precedence
/// those six were refused.
///
/// One refusal remains, deliberately: `Item 24` ({20, 907.7} 400 × 52, shown to the
/// screen's bottom at 956) lies wholly under the tab bar ({0, 873} 440 × 83), so every
/// point of it would go to the tab bar: "covered by the tab bar" is the true answer (the
/// model scrolls first). The list is exact: any other refusal, or this one passing, fails.
#[test]
fn every_listed_control_has_an_uncontested_tap_point() {
    let mut refused: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let mut bar_points = 0usize;
    for name in FIXTURES {
        let s = fixture(name);
        let view = jarvis_observe::compact_view(&s);
        let tree = jarvis_safety::ScreenTree {
            sources: &view.sources,
            parents: &view.parents,
            ends: &view.ends,
            bars: &view.bars,
            nodes: &view.nodes,
        };
        for e in view
            .screen
            .elements
            .iter()
            .filter(|e| e.enabled && jarvis_safety::takes_taps(&e.role))
        {
            checked += 1;
            match jarvis_safety::tap_spot(e, &view.screen, Some(&tree), None) {
                Ok(p) => {
                    assert!(e.frame.contains(p), "{name}: {}", e.label);
                    // A point inside a bar's frame is only ever a control of that bar's.
                    for b in &view.bars {
                        if b.frame.contains(p) {
                            bar_points += 1;
                            let src = view.sources[(e.index - 1) as usize];
                            assert!(
                                b.node < src && src < view.ends[b.node as usize],
                                "{name}: {} tapped inside a bar it is not in",
                                e.label
                            );
                        }
                    }
                }
                Err(c) => refused.push(format!(
                    "{name}: [{}] {:?} {:?} covered by {:?}",
                    e.index, e.role, e.label, c.by
                )),
            }
        }
    }
    assert!(checked > 100, "{checked}");
    assert!(bar_points >= 6, "{bar_points}");
    let known: Vec<String> = KNOWN_REFUSALS
        .iter()
        .map(|(entry, _reason)| (*entry).to_owned())
        .collect();
    assert_eq!(refused, known, "{}", refused.join("\n"));
}

/// The six controls the sweep used to refuse on `long_list_scrolled`, by name: the back
/// button and the five tab-bar buttons are tapped at their own centres, a list row lying
/// partly under the tab bar is tapped outside the bar, and the row wholly under it is
/// refused, naming the tab bar.
#[test]
fn long_list_scrolled_bar_controls_are_tapped_at_their_centres() {
    let s = fixture("long_list_scrolled");
    let view = jarvis_observe::compact_view(&s);
    let tree = jarvis_safety::ScreenTree {
        sources: &view.sources,
        parents: &view.parents,
        ends: &view.ends,
        bars: &view.bars,
        nodes: &view.nodes,
    };
    let roles: Vec<&Role> = view.bars.iter().map(|b| &b.role).collect();
    assert_eq!(roles, [&Role::NavigationBar, &Role::TabBar]);
    for label in ["Playground", "Ask", "Activity", "Apps", "Rules"] {
        for e in view.screen.elements.iter().filter(|e| e.label == label) {
            let p = jarvis_safety::tap_spot(e, &view.screen, Some(&tree), None).unwrap();
            assert_eq!(p, e.frame.center(), "{label}");
        }
    }
    let tab = view.bars[1].frame;
    let under: Vec<_> = view
        .screen
        .elements
        .iter()
        .filter(|e| jarvis_safety::takes_taps(&e.role) && e.frame.intersects(&tab))
        .filter(|e| {
            let src = view.sources[(e.index - 1) as usize];
            !(view.bars[1].node < src && src < view.ends[view.bars[1].node as usize])
        })
        .collect();
    let labels: Vec<&str> = under.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(labels, ["Item 23", "Item 24"]);
    let p = jarvis_safety::tap_spot(under[0], &view.screen, Some(&tree), None).unwrap();
    assert!(!tab.contains(p) && under[0].frame.contains(p), "{p:?}");
    assert_eq!(
        jarvis_safety::tap_spot(under[1], &view.screen, Some(&tree), None),
        Err(jarvis_safety::Covered {
            target: under[1].index,
            by: jarvis_safety::CoveredBy::Bar(jarvis_safety::Bar::Tab),
        })
    );
}

/// Every refusal of the tap-point rule in force on the ten fixtures, with why it is a genuine
/// cover (read from the fixture). Seventh review follow-up: rule v2 (controls, the keyboard,
/// labelled content drawn later) refuses nothing v0 did not.
const KNOWN_REFUSALS: &[(&str, &str)] = &[(
    "long_list_scrolled: [26] Button \"Item 24\" covered by Bar(Tab)",
    "row scrolled wholly under the tab bar ({20, 907.7} 400 x 48.3 shown, tab bar {0, 873} 440 x 83)",
)];

/// The rules measured side by side on the ten fixtures (seventh review follow-up), per
/// fixture and in total: v0 (the sixth review's rule, listed enabled controls), v2 without
/// labelled content (controls, enabled or not, and the keyboard), v2 (the rule in force),
/// and the two narrowings of v2's labelled-content clause the follow-up named. Each gives
/// exactly the one genuine refusal. Before the keyboard's nodes were clipped to their
/// subtree's root (the prediction bar's scroll content reports `{0, 11} 440 x 1244` but is
/// drawn inside the 44 pt bar, `login_keyboard` fixture line `Other(Other) {{0, 11}, {440,
/// 1244}}`), v2 without labelled content refused all 5 controls of `login_keyboard` too.
#[test]
fn the_tap_rules_measured_side_by_side() {
    use jarvis_safety::OccluderRule as R;
    let rules = [
        R::V0,
        R::Controls,
        R::Labelled,
        R::LabelledNotOwnLabel,
        R::LabelledOtherSubtree,
    ];
    let mut totals = Vec::new();
    for rule in rules {
        let mut per = Vec::new();
        for name in FIXTURES {
            let s = fixture(name);
            let view = jarvis_observe::compact_view(&s);
            let tree = jarvis_safety::ScreenTree {
                sources: &view.sources,
                parents: &view.parents,
                ends: &view.ends,
                bars: &view.bars,
                nodes: &view.nodes,
            };
            let n = view
                .screen
                .elements
                .iter()
                .filter(|e| e.enabled && jarvis_safety::takes_taps(&e.role))
                .filter(|e| {
                    jarvis_safety::tap_spot_under(rule, e, &view.screen, Some(&tree), None).is_err()
                })
                .count();
            per.push((name, n));
        }
        let total: usize = per.iter().map(|(_, n)| n).sum();
        println!("{rule:?}: total {total}; {per:?}");
        totals.push(total);
        for (name, n) in per {
            let want = usize::from(name == "long_list_scrolled");
            assert_eq!(n, want, "{rule:?} {name}");
        }
    }
    assert_eq!(totals, [1, 1, 1, 1, 1]);
    assert_eq!(jarvis_safety::OCCLUDER_RULE, R::Labelled);
}

/// With the keyboard up every listed control that is not under the `Keyboard` node's own
/// frame gets a tap point (the keyboard's window and its full-screen containers cover
/// nothing; the prediction bar covers only its 44 pt).
#[test]
fn with_the_keyboard_up_every_control_above_it_is_tappable() {
    let s = fixture("login_keyboard");
    let view = jarvis_observe::compact_view(&s);
    let tree = jarvis_safety::ScreenTree {
        sources: &view.sources,
        parents: &view.parents,
        ends: &view.ends,
        bars: &view.bars,
        nodes: &view.nodes,
    };
    let controls: Vec<_> = view
        .screen
        .elements
        .iter()
        .filter(|e| jarvis_safety::takes_taps(&e.role))
        .collect();
    assert_eq!(controls.len(), 5);
    for e in controls {
        assert!(
            jarvis_safety::tap_spot(e, &view.screen, Some(&tree), None).is_ok(),
            "{}",
            e.label
        );
    }
}
