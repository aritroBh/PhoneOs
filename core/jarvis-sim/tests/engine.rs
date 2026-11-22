//! The app engine and the phone: tap resolution, state, scrolling, text input, observation
//! modes and the runner-text round trip.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use jarvis_observe::parse_runner_text;
use jarvis_proto::{Key, Point, Rect, Role, RunnerCommand, UiNode};
use jarvis_sim::app::AppDef;
use jarvis_sim::builtin::{builtin_apps, parse_app, APP_SOURCES};
use jarvis_sim::{ObserveMode, SimError, SimPhone};
use serde_json::json;

fn phone_with(app_json: &str) -> SimPhone {
    let mut apps: Vec<AppDef> = builtin_apps()
        .unwrap()
        .into_iter()
        .filter(|a| a.id == "home")
        .collect();
    apps.push(parse_app("test", app_json).unwrap());
    let mut p = SimPhone::from_apps(apps).unwrap();
    p.launch("t").unwrap();
    p
}

fn tap(p: &mut SimPhone, x: f64, y: f64) -> jarvis_sim::phone::ActOutcome {
    p.act(&RunnerCommand::Tap { x, y })
}

fn find<'a>(n: &'a UiNode, label: &str) -> Option<&'a UiNode> {
    n.find(|x| x.label.as_deref() == Some(label))
}

fn labels(n: &UiNode) -> Vec<String> {
    n.iter()
        .filter(|x| x.role == Role::StaticText || x.role == Role::Cell)
        .filter_map(|x| x.label.clone())
        .collect()
}

const OVERLAP: &str = r#"{
  "id": "t", "bundle_id": "com.test", "name": "T", "start": "s",
  "state": { "a": 0, "b": 0, "c": 0, "d": 0, "e": 0, "hidden": false, "parent": 0, "child": 0 },
  "screens": { "s": { "elements": [
    { "id": "A", "role": "button", "label": "A", "frame": [20, 200, 200, 100], "on_tap": [{ "do": "add", "var": "a", "by": 1 }] },
    { "id": "B", "role": "button", "label": "B", "frame": [120, 250, 200, 100], "on_tap": [{ "do": "add", "var": "b", "by": 1 }] },
    { "id": "C", "role": "button", "label": "C", "when": "hidden", "frame": [20, 200, 300, 150], "on_tap": [{ "do": "add", "var": "c", "by": 1 }] },
    { "id": "D", "role": "button", "label": "D", "frame": [20, 620, 100, 40], "on_tap": [{ "do": "add", "var": "d", "by": 1 }] },
    { "id": "shield", "role": "other", "opaque": true, "frame": [0, 600, 393, 100] },
    { "id": "P", "role": "cell", "frame": [20, 400, 300, 100], "on_tap": [{ "do": "add", "var": "parent", "by": 1 }],
      "children": [ { "id": "K", "role": "button", "label": "K", "frame": [10, 10, 80, 40], "on_tap": [{ "do": "add", "var": "child", "by": 1 }] } ] },
    { "id": "E", "role": "button", "label": "E", "enabled": "false", "frame": [20, 500, 100, 40], "on_tap": [{ "do": "add", "var": "e", "by": 1 }] }
  ] } }
}"#;

#[test]
fn taps_resolve_to_the_topmost_node_containing_the_point() {
    let mut p = phone_with(OVERLAP);
    // Only in A.
    tap(&mut p, 30.0, 210.0);
    // In the overlap of A and B: B is later in paint order, so B wins.
    tap(&mut p, 150.0, 280.0);
    // Only in B.
    tap(&mut p, 300.0, 340.0);
    let s = p.snapshot();
    assert_eq!(s.states["t"]["a"], 1);
    assert_eq!(s.states["t"]["b"], 2);
    // C is conditionally hidden and must not catch taps over A or B.
    assert_eq!(s.states["t"]["c"], 0);
}

#[test]
fn opaque_overlays_swallow_taps_and_children_beat_their_parents() {
    let mut p = phone_with(OVERLAP);
    let out = tap(&mut p, 50.0, 640.0);
    assert!(out.result.ok);
    assert!(out.note.contains("absorbed"), "{}", out.note);
    assert_eq!(p.snapshot().states["t"]["d"], 0, "D sits under the shield");
    // Inside the parent but outside the child: the parent. On the child: only the child.
    tap(&mut p, 250.0, 470.0);
    tap(&mut p, 50.0, 430.0);
    let s = p.snapshot();
    assert_eq!(s.states["t"]["parent"], 1);
    assert_eq!(s.states["t"]["child"], 1);
}

#[test]
fn disabled_elements_and_empty_space_ignore_taps_without_failing() {
    let mut p = phone_with(OVERLAP);
    let out = tap(&mut p, 50.0, 520.0);
    assert!(
        out.result.ok && out.note.contains("disabled"),
        "{}",
        out.note
    );
    let out = tap(&mut p, 380.0, 800.0);
    assert!(
        out.result.ok && out.note.contains("nothing there"),
        "{}",
        out.note
    );
    assert_eq!(p.snapshot().states["t"]["e"], 0);
}

const STATEFUL: &str = r#"{
  "id": "t", "bundle_id": "com.test", "name": "T", "start": "s",
  "state": { "n": 0, "on": false, "name": "", "items": ["x"], "log": [], "total": 0 },
  "screens": {
    "s": { "title": "Home", "elements": [
      { "id": "count", "role": "static_text", "label": "Count {n}", "frame": [16, 160, 200, 30] },
      { "id": "plus", "role": "button", "label": "Plus", "frame": [16, 200, 100, 40], "on_tap": [{ "do": "add", "var": "n", "by": 1 }] },
      { "id": "flip", "role": "switch", "label": "Flip", "value": "{on ? '1' : '0'}", "frame": [16, 250, 60, 40], "on_tap": [{ "do": "toggle", "var": "on" }] },
      { "id": "extra", "role": "button", "label": "Extra", "when": "on", "frame": [16, 300, 100, 40] },
      { "id": "go", "role": "button", "label": "Go", "enabled": "n >= 2", "frame": [16, 350, 100, 40],
        "on_tap": [{ "do": "if", "cond": "n == 2", "then": [{ "do": "goto", "screen": "two" }], "else": [{ "do": "goto", "screen": "other" }] }] },
      { "id": "adder", "role": "button", "label": "Append", "frame": [16, 400, 100, 40],
        "on_tap": [{ "do": "append", "to": "log", "value": "n={n}" }, { "do": "remove", "from": "items", "index": 0 }, { "do": "set", "var": "name", "value": "{items.len}" }] }
    ] },
    "two": { "title": "Two", "elements": [ { "id": "r", "role": "button", "label": "Replace", "frame": [16, 200, 100, 40], "on_tap": [{ "do": "goto", "screen": "three", "replace": true }] } ] },
    "three": { "title": "Three", "elements": [ { "id": "r", "role": "button", "label": "Reset", "frame": [16, 200, 100, 40], "on_tap": [{ "do": "goto", "screen": "s", "reset": true }] } ] },
    "other": { "title": "Other", "elements": [] }
  }
}"#;

#[test]
fn labels_values_visibility_and_enabled_follow_the_state() {
    let mut p = phone_with(STATEFUL);
    let root = p.structured_root().unwrap();
    assert!(find(&root, "Count 0").is_some());
    assert!(find(&root, "Extra").is_none(), "hidden while `on` is false");
    assert!(!find(&root, "Go").unwrap().enabled);
    assert_eq!(find(&root, "Flip").unwrap().value.as_deref(), Some("0"));

    tap(&mut p, 60.0, 220.0);
    tap(&mut p, 60.0, 220.0);
    tap(&mut p, 40.0, 270.0);
    let root = p.structured_root().unwrap();
    assert!(find(&root, "Count 2").is_some(), "{:?}", labels(&root));
    assert!(find(&root, "Extra").is_some());
    assert!(find(&root, "Go").unwrap().enabled);
    assert_eq!(find(&root, "Flip").unwrap().value.as_deref(), Some("1"));
}

#[test]
fn effects_set_append_remove_goto_replace_reset_and_back() {
    let mut p = phone_with(STATEFUL);
    tap(&mut p, 60.0, 220.0);
    tap(&mut p, 60.0, 220.0);
    tap(&mut p, 60.0, 420.0); // append "n=2", remove items[0], name = 0
    let st = p.snapshot().states["t"].clone();
    assert_eq!(st["log"], json!(["n=2"]));
    assert_eq!(st["items"], json!([]));
    assert_eq!(st["name"], json!(0));
    // `if` takes the then-branch at n == 2 and pushes `two`, which has a back button.
    tap(&mut p, 60.0, 370.0);
    assert_eq!(p.snapshot().screen, "two");
    let root = p.structured_root().unwrap();
    assert!(
        find(&root, "Home").is_some(),
        "back button is titled with the previous screen"
    );
    // Replace swaps `two` for `three` without growing the stack; back returns to `s`.
    tap(&mut p, 60.0, 220.0);
    assert_eq!(p.snapshot().screen, "three");
    let back = find(&p.structured_root().unwrap(), "Home")
        .unwrap()
        .frame
        .center();
    tap(&mut p, back.x, back.y);
    assert_eq!(p.snapshot().screen, "s");
    // Reset leaves no back button.
    tap(&mut p, 60.0, 370.0);
    tap(&mut p, 60.0, 220.0);
    tap(&mut p, 60.0, 220.0);
    assert_eq!(p.snapshot().screen, "s");
    let root = p.structured_root().unwrap();
    assert!(root
        .find(|n| n.identifier.as_deref() == Some("BackButton"))
        .is_none());
}

const SCROLLING: &str = r#"{
  "id": "t", "bundle_id": "com.test", "name": "T", "start": "s",
  "state": { "rows": [], "picked": "" },
  "screens": { "s": { "title": "List", "elements": [
    { "id": "list", "role": "table", "scroll": true, "frame": [0, 150, 393, 500],
      "children": [ { "kind": "row", "for_each": "rows", "item": "r", "id": "row-{index}", "h": 50, "label": "{r}",
                      "on_tap": [{ "do": "set", "var": "picked", "value": "{r}" }] } ] }
  ] } }
}"#;

fn scrolling_phone() -> SimPhone {
    let mut p = phone_with(SCROLLING);
    let rows: Vec<String> = (0..30).map(|i| format!("Row {i}")).collect();
    p.set_state("t", "rows", json!(rows)).unwrap();
    p
}

fn swipe(p: &mut SimPhone, from_y: f64, to_y: f64) -> String {
    p.act(&RunnerCommand::Swipe {
        from: Point::new(200.0, from_y),
        to: Point::new(200.0, to_y),
        duration_ms: 300,
    })
    .note
}

#[test]
fn scrolling_virtualises_rows_clamps_and_keeps_taps_aligned() {
    let mut p = scrolling_phone();
    let rows = |p: &mut SimPhone| -> Vec<String> {
        p.structured_root()
            .unwrap()
            .iter()
            .filter(|n| {
                n.role == Role::StaticText
                    && n.label.as_deref().is_some_and(|l| l.starts_with("Row"))
            })
            .filter_map(|n| n.label.clone())
            .collect()
    };
    let first = rows(&mut p);
    assert_eq!(first.first().map(String::as_str), Some("Row 0"));
    assert!(
        first.len() <= 11,
        "only rows in the viewport exist: {}",
        first.len()
    );
    assert!(!first.contains(&"Row 20".to_owned()));

    assert!(swipe(&mut p, 500.0, 200.0).contains("0 -> 300"));
    let second = rows(&mut p);
    assert_eq!(second.first().map(String::as_str), Some("Row 6"));

    // A tap lands on the row under the finger after scrolling.
    // Row 6 is at y = 150 + 6*50 - 300 = 150..200; tap its centre.
    tap(&mut p, 100.0, 175.0);
    assert_eq!(p.snapshot().states["t"]["picked"], "Row 6");

    // Scrolling stops at the end of the content (30 rows * 50 = 1500; viewport 500).
    assert!(
        swipe(&mut p, 600.0, 160.0).contains("-> 740"),
        "partial scroll clamps"
    );
    assert!(swipe(&mut p, 600.0, 160.0).contains("-> 1000"));
    assert!(swipe(&mut p, 600.0, 160.0).contains("already at the end"));
    assert!(rows(&mut p).contains(&"Row 29".to_owned()));

    // Scrolling back up works, and the viewport edges clip taps.
    assert!(swipe(&mut p, 200.0, 650.0).contains("1000 -> 550"));
    // Outside the viewport nothing scrolls; horizontal swipes are ignored.
    assert!(swipe(&mut p, 120.0, 60.0).contains("nothing scrollable"));
    let h = p
        .act(&RunnerCommand::Swipe {
            from: Point::new(300.0, 300.0),
            to: Point::new(50.0, 290.0),
            duration_ms: 200,
        })
        .note;
    assert!(h.contains("horizontal"), "{h}");
}

const INPUT: &str = r#"{
  "id": "t", "bundle_id": "com.test", "name": "T", "start": "s",
  "state": { "email": "", "pw": "", "sent": [] },
  "screens": { "s": { "elements": [
    { "id": "email", "role": "text_field", "label": "Email", "placeholder": "you@x.com", "bind": "email", "frame": [24, 200, 345, 44] },
    { "id": "pw", "role": "secure_text_field", "label": "Password", "bind": "pw", "secure": true, "frame": [24, 260, 345, 44],
      "on_submit": [{ "do": "append", "to": "sent", "value": "submitted {email}" }] }
  ] } }
}"#;

#[test]
fn text_input_focus_keyboard_typing_secure_values_and_keys() {
    let mut p = phone_with(INPUT);
    // Typing with nothing focused is a runner error, as on a real device.
    let out = p.act(&RunnerCommand::TypeText { text: "x".into() });
    assert!(!out.result.ok);

    tap(&mut p, 100.0, 220.0);
    let obs = p.observe().unwrap();
    assert!(obs.screen.keyboard_visible);
    assert!(find(&obs.screen.root, "Email").unwrap().focused);
    assert!(obs.screen.root.find(|n| n.role == Role::Keyboard).is_some());

    p.act(&RunnerCommand::TypeText { text: "ab".into() });
    // Tapping the on-screen 'q' key types it; Delete removes one character.
    let q = find(&p.structured_root().unwrap(), "q")
        .unwrap()
        .frame
        .center();
    tap(&mut p, q.x, q.y);
    p.act(&RunnerCommand::PressKey { key: Key::Delete });
    p.act(&RunnerCommand::TypeText { text: "c".into() });
    assert_eq!(p.snapshot().states["t"]["email"], "abc");
    assert_eq!(
        find(&p.structured_root().unwrap(), "Email")
            .unwrap()
            .value
            .as_deref(),
        Some("abc")
    );

    // Secure fields report bullets, never the text, and Return runs on_submit.
    tap(&mut p, 100.0, 280.0);
    p.act(&RunnerCommand::TypeText {
        text: "hunter2".into(),
    });
    let root = p.structured_root().unwrap();
    let pw = find(&root, "Password").unwrap();
    assert!(pw.secure);
    assert_eq!(pw.value.as_deref(), Some("\u{2022}".repeat(7).as_str()));
    p.act(&RunnerCommand::PressKey { key: Key::Return });
    assert_eq!(p.snapshot().states["t"]["sent"], json!(["submitted abc"]));
}

#[test]
fn runner_text_mode_loses_flags_but_keeps_the_keyboard_flag() {
    let mut p = phone_with(INPUT);
    tap(&mut p, 100.0, 220.0);
    let structured = p.observe().unwrap();
    p.set_mode(ObserveMode::RunnerText);
    let text = p.observe().unwrap();
    assert!(find(&structured.screen.root, "Email").unwrap().focused);
    assert!(
        !find(&text.screen.root, "Email").unwrap().focused,
        "focus is not in the text format"
    );
    assert!(
        text.screen.keyboard_visible,
        "the keyboard flag comes from a separate runner field"
    );
    assert!(find(&text.screen.root, "Email")
        .unwrap()
        .placeholder
        .is_none());
}

const ALERTS: &str = r#"{
  "id": "t", "bundle_id": "com.test", "name": "T", "start": "s",
  "state": { "x": 0, "deleted": false },
  "screens": { "s": { "elements": [
    { "id": "ask", "role": "button", "label": "Ask", "frame": [20, 200, 100, 40], "on_tap": [{ "do": "alert", "id": "confirm" }] },
    { "id": "inc", "role": "button", "label": "Inc", "frame": [20, 300, 100, 40], "on_tap": [{ "do": "add", "var": "x", "by": 1 }] }
  ] } },
  "alerts": { "confirm": { "title": "Sure?", "message": "x is {x}", "buttons": [
    { "label": "No" }, { "label": "Yes", "then": [{ "do": "set", "var": "deleted", "value": true }] } ] } }
}"#;

#[test]
fn alerts_are_modal_and_their_buttons_run_effects_after_closing() {
    let mut p = phone_with(ALERTS);
    tap(&mut p, 50.0, 220.0);
    assert_eq!(p.snapshot().alert.as_deref(), Some("confirm"));
    let root = p.structured_root().unwrap();
    let alert = root.find(|n| n.role == Role::Alert).unwrap();
    assert!(find(alert, "x is 0").is_some());
    // The button under the alert is blocked.
    let out = tap(&mut p, 50.0, 320.0);
    assert!(out.note.contains("absorbed"), "{}", out.note);
    assert_eq!(p.snapshot().states["t"]["x"], 0);
    // "No" closes without effects; "Yes" closes and runs its effects.
    let no = find(alert, "No").unwrap().frame.center();
    tap(&mut p, no.x, no.y);
    assert_eq!(p.snapshot().alert, None);
    assert_eq!(p.snapshot().states["t"]["deleted"], false);
    tap(&mut p, 50.0, 220.0);
    let yes = find(&p.structured_root().unwrap(), "Yes")
        .unwrap()
        .frame
        .center();
    tap(&mut p, yes.x, yes.y);
    assert_eq!(p.snapshot().alert, None);
    assert_eq!(p.snapshot().states["t"]["deleted"], true);
}

#[test]
fn launching_home_and_urls_switch_apps_and_apps_keep_their_state() {
    let mut p = SimPhone::new().unwrap();
    assert_eq!(p.snapshot().app, "home");
    // Tap the Settings icon on the home screen.
    let icon = find(&p.structured_root().unwrap(), "Settings")
        .unwrap()
        .frame
        .center();
    tap(&mut p, icon.x, icon.y);
    assert_eq!(p.snapshot().app, "settings");
    p.set_state("settings", "wifi", json!(false)).unwrap();
    assert!(p.act(&RunnerCommand::PressKey { key: Key::Home }).result.ok);
    assert_eq!(p.snapshot().app, "home");
    assert!(
        p.act(&RunnerCommand::LaunchApp {
            bundle_id: "com.apple.Preferences".into()
        })
        .result
        .ok
    );
    assert_eq!(p.snapshot().states["settings"]["wifi"], false);
    assert!(
        p.act(&RunnerCommand::OpenUrl {
            url: "sms:+15551234".into()
        })
        .result
        .ok
    );
    assert_eq!(p.snapshot().app, "messages");
    let bad = p.act(&RunnerCommand::LaunchApp {
        bundle_id: "com.nope".into(),
    });
    assert!(!bad.result.ok);
    assert!(bad.result.error.unwrap().contains("com.nope"));
    assert!(
        !p.act(&RunnerCommand::OpenUrl {
            url: "gopher://x".into()
        })
        .result
        .ok
    );
    let t0 = p.now_ms();
    p.act(&RunnerCommand::Wait { ms: 250 });
    assert_eq!(p.now_ms(), t0 + 250);
}

#[test]
fn injected_runner_failures_hit_exactly_the_chosen_command_once() {
    let mut p = phone_with(OVERLAP);
    p.fail_act_at(1);
    assert!(tap(&mut p, 30.0, 210.0).result.ok);
    let failed = tap(&mut p, 30.0, 210.0);
    assert!(!failed.result.ok);
    assert!(failed.note.starts_with("FAILED"));
    assert!(tap(&mut p, 30.0, 210.0).result.ok);
    assert_eq!(
        p.snapshot().states["t"]["a"],
        2,
        "the failed tap had no effect"
    );
    assert_eq!(p.snapshot().acts, 3);
}

// ---------------------------------------------------------------------------------------
// Runner text.
// ---------------------------------------------------------------------------------------

fn same_shape(a: &UiNode, b: &UiNode, path: &str) {
    assert_eq!(a.role, b.role, "{path}");
    assert_eq!(a.label, b.label, "{path} label");
    assert_eq!(a.value, b.value, "{path} value");
    assert_eq!(a.identifier, b.identifier, "{path} id");
    let t = |r: &Rect| (r.x.trunc(), r.y.trunc(), r.width.trunc(), r.height.trunc());
    assert_eq!(t(&a.frame), t(&b.frame), "{path} frame");
    assert_eq!(a.children.len(), b.children.len(), "{path} children");
    for (i, (x, y)) in a.children.iter().zip(&b.children).enumerate() {
        same_shape(x, y, &format!("{path}/{i}"));
    }
}

#[test]
fn runner_text_round_trips_every_builtin_screen_through_the_real_parser() {
    let mut checked = 0;
    for def in builtin_apps().unwrap() {
        for screen in def.screens.keys() {
            let mut p = SimPhone::new().unwrap();
            // Give screens that depend on state something to show.
            p.set_state("messages", "current", json!("Mom")).unwrap();
            p.set_state("messages", "draft", json!("hi")).unwrap();
            p.set_state("shop", "selected", json!("Desk Lamp")).unwrap();
            p.set_state(
                "shop",
                "cart",
                json!([{"name": "Desk Lamp", "price": 35.5}]),
            )
            .unwrap();
            p.set_state("notes", "open", json!("Garden")).unwrap();
            p.show_screen(&def.id, screen).unwrap();
            let tree = p.structured_root().unwrap();
            let text = p.runner_text().unwrap();
            let parsed = parse_runner_text(&text).unwrap();
            assert_eq!(parsed.warnings, 0, "{}/{screen}:\n{text}", def.id);
            same_shape(&tree, &parsed.root, &format!("{}/{screen}", def.id));
            checked += 1;
        }
    }
    assert!(checked >= 15, "only {checked} screens");
}

#[test]
fn runner_text_follows_the_loopctrl_line_format() {
    let mut p = SimPhone::new().unwrap();
    p.show_screen("settings", "wifi").unwrap();
    let text = p.runner_text().unwrap();
    let mut lines = text.lines();
    assert_eq!(
        lines.next().unwrap(),
        "Application, label: 'Settings', {{0, 0}, {393, 852}}"
    );
    assert!(
        text.contains("\n    Table, {{0, 150}, {393, 702}}"),
        "{text}"
    );
    assert!(
        text.contains("Switch, label: 'Wi-Fi', value: '1', {{326, 156}, {51, 31}}"),
        "{text}"
    );
    // Cells carry no label of their own; their texts are children, as on the device.
    assert!(
        text.contains("Cell, id: 'wifi-switch', {{0, 150}, {393, 44}}"),
        "{text}"
    );
    assert!(
        text.contains("\n      Cell, {{0, 232}, {393, 44}}"),
        "{text}"
    );
    assert!(
        text.contains("NavigationBar, id: 'Wi-Fi', {{0, 59}, {393, 85}}"),
        "{text}"
    );
}

#[test]
fn both_modes_agree_on_roles_labels_and_frames() {
    let mut p = SimPhone::new().unwrap();
    p.show_screen("shop", "products").unwrap();
    let a = p.observe().unwrap();
    p.set_mode(ObserveMode::RunnerText);
    let b = p.observe().unwrap();
    same_shape(&a.screen.root, &b.screen.root, "shop/products");
    assert_eq!(a.screen.app_bundle_id, b.screen.app_bundle_id);
    assert_eq!(a.screen.size, b.screen.size);
    // Realistic iPhone frame, as in the proto fixtures.
    assert_eq!((a.screen.size.width, a.screen.size.height), (393.0, 852.0));
}

// ---------------------------------------------------------------------------------------
// Definitions and predicates.
// ---------------------------------------------------------------------------------------

#[test]
fn every_builtin_app_file_parses_and_validates() {
    assert_eq!(APP_SOURCES.len(), 9);
    let phone = SimPhone::new().unwrap();
    let ids = phone.app_ids();
    for id in [
        "acme",
        "dialer",
        "home",
        "laden",
        "messages",
        "notes",
        "passwords",
        "settings",
        "shop",
    ] {
        assert!(ids.contains(&id.to_owned()), "{id}");
    }
    let known: Vec<String> = phone.known_apps().into_iter().map(|a| a.name).collect();
    assert!(!known.contains(&"Home".to_owned()));
    assert!(known.contains(&"Passwords".to_owned()));
}

#[test]
fn bad_definitions_are_rejected_with_clear_errors() {
    let home = || {
        builtin_apps()
            .unwrap()
            .into_iter()
            .find(|a| a.id == "home")
            .unwrap()
    };
    let build = |json: &str| -> Result<SimPhone, SimError> {
        SimPhone::from_apps(vec![home(), parse_app("bad", json)?])
    };
    let app = |body: &str| {
        format!(
            r#"{{"id":"t","bundle_id":"b","name":"T","start":"s","screens":{{"s":{{"elements":[{body}]}}}}}}"#
        )
    };
    // A typo in a field name is an error, not silently ignored.
    assert!(matches!(
        build(&app(r#"{"role":"button","lable":"x"}"#)),
        Err(SimError::AppJson { .. })
    ));
    // A goto to a screen that does not exist.
    let e = build(&app(
        r#"{"role":"button","on_tap":[{"do":"goto","screen":"nope"}]}"#,
    ))
    .unwrap_err();
    assert!(e.to_string().contains("nope"), "{e}");
    // An unparsable expression.
    let e = build(&app(r#"{"role":"button","when":"a =="}"#)).unwrap_err();
    assert!(e.to_string().contains("bad expression"), "{e}");
    // A missing start screen.
    let e = build(r#"{"id":"t","bundle_id":"b","name":"T","start":"x","screens":{"s":{}}}"#)
        .unwrap_err();
    assert!(e.to_string().contains("start screen"), "{e}");
    // No home screen.
    assert!(matches!(
        SimPhone::from_apps(vec![parse_app("t", &app("")).unwrap()]),
        Err(SimError::UnknownApp(_))
    ));
}

#[test]
fn snapshot_predicates_read_app_state_and_the_phone() {
    let mut p = SimPhone::new().unwrap();
    p.launch("messages").unwrap();
    let s = p.snapshot();
    assert!(s.check(r#"messages.thread["Mom"].len == 2"#).unwrap());
    assert!(s
        .check(r#"phone.app == "messages" && phone.screen == "list""#)
        .unwrap());
    assert!(s.check("settings.wifi && !settings.dark").unwrap());
    assert!(s.check("phone.alert == null").unwrap());
    assert_eq!(
        s.eval(r#"messages.thread["Alex"].last"#).unwrap(),
        json!("Lunch tomorrow?")
    );
    assert!(s.check("a ==").is_err());
}
