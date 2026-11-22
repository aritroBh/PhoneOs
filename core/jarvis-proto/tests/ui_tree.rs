//! UI tree tests: runner-style JSON parses into the shared fixtures, roles are lenient and
//! round-trip, the tree iterator and geometry helpers behave.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use jarvis_proto::fixtures;
use jarvis_proto::*;
use serde_json::json;

/// A login screen as a runner might send it: mixed-case role names, default-valued fields
/// omitted. Must equal `fixtures::login_screen()`.
const LOGIN_JSON: &str = r#"
{
  "app_bundle_id": "com.example.acme",
  "app_name": "Acme",
  "size": {"width": 393, "height": 852},
  "keyboard_visible": true,
  "orientation": "portrait",
  "root": {
    "role": "Application", "label": "Acme",
    "frame": {"x": 0, "y": 0, "width": 393, "height": 852},
    "children": [{
      "role": "window",
      "frame": {"x": 0, "y": 0, "width": 393, "height": 852},
      "children": [
        {"role": "staticText", "label": "Welcome back",
         "frame": {"x": 24, "y": 120, "width": 345, "height": 40}},
        {"role": "other", "frame": {"x": 0, "y": 200, "width": 393, "height": 300},
         "children": [
           {"role": "textField", "label": "Email", "value": "alex@example.com",
            "identifier": "login.email", "placeholder": "you@example.com",
            "frame": {"x": 24, "y": 220, "width": 345, "height": 44}, "hittable": true},
           {"role": "secureTextField", "label": "Password", "identifier": "login.password",
            "placeholder": "Password", "secure": true, "focused": true, "hittable": true,
            "frame": {"x": 24, "y": 280, "width": 345, "height": 44}},
           {"role": "button", "label": "Sign in", "identifier": "login.submit",
            "enabled": false, "hittable": true,
            "frame": {"x": 24, "y": 344, "width": 345, "height": 50}},
           {"role": "link", "label": "Forgot password?", "hittable": true,
            "frame": {"x": 130, "y": 410, "width": 133, "height": 20}}
         ]},
        {"role": "keyboard", "frame": {"x": 0, "y": 516, "width": 393, "height": 336},
         "children": [
           {"role": "key", "label": "q", "hittable": true, "frame": {"x": 3, "y": 527, "width": 37, "height": 46}},
           {"role": "key", "label": "w", "hittable": true, "frame": {"x": 42, "y": 527, "width": 37, "height": 46}},
           {"role": "key", "label": "e", "hittable": true, "frame": {"x": 81, "y": 527, "width": 37, "height": 46}},
           {"role": "button", "label": "shift", "hittable": true, "frame": {"x": 3, "y": 635, "width": 46, "height": 46}},
           {"role": "key", "label": "delete", "hittable": true, "frame": {"x": 344, "y": 635, "width": 46, "height": 46}},
           {"role": "key", "label": "space", "hittable": true, "frame": {"x": 100, "y": 689, "width": 193, "height": 46}},
           {"role": "button", "label": "go", "hittable": true, "frame": {"x": 297, "y": 689, "width": 93, "height": 46}}
         ]}
      ]
    }]
  }
}
"#;

#[test]
fn runner_json_parses_into_login_fixture() {
    let parsed: Screen = serde_json::from_str(LOGIN_JSON).unwrap();
    assert_eq!(parsed, fixtures::login_screen());

    let password = parsed
        .root
        .find(|n| n.role == Role::SecureTextField)
        .unwrap();
    assert!(password.secure && password.focused && password.value.is_none());
    let sign_in = parsed
        .root
        .find(|n| n.label.as_deref() == Some("Sign in"))
        .unwrap();
    assert!(!sign_in.enabled);
    assert!(parsed.keyboard_visible);
    assert_eq!(parsed.bounds(), Rect::new(0.0, 0.0, 393.0, 852.0));

    // Re-serialized output uses canonical snake_case roles and parses back identically.
    let out = serde_json::to_value(&parsed).unwrap();
    assert_eq!(
        out["root"]["children"][0]["children"][1]["children"][1]["role"],
        "secure_text_field"
    );
    let again: Screen = serde_json::from_value(out).unwrap();
    assert_eq!(again, parsed);
}

#[test]
fn all_fixtures_round_trip_and_are_realistic() {
    for s in [
        fixtures::settings_screen(),
        fixtures::login_screen(),
        fixtures::compose_screen(),
    ] {
        let json = serde_json::to_string(&s).unwrap();
        let back: Screen = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
        assert_eq!(s.root.role, Role::Application);
        assert!(s.root.count() > 10, "{} too small", s.app_bundle_id);
    }

    let settings = fixtures::settings_screen();
    let switch = settings.root.find(|n| n.role == Role::Switch).unwrap();
    assert_eq!(switch.value.as_deref(), Some("0"));
    assert!(settings
        .root
        .iter()
        .any(|n| n.frame.area() == 0.0 && n.role == Role::Other("other".into())));
    assert!(settings
        .root
        .iter()
        .any(|n| !settings.bounds().intersects(&n.frame) && n.role == Role::Cell));

    let compose = fixtures::compose_screen();
    let send = compose
        .root
        .find(|n| n.role == Role::Button && n.label.as_deref() == Some("Send"))
        .unwrap();
    assert!(send.enabled && send.hittable);
    let body = compose.root.find(|n| n.role.is_text_input()).unwrap();
    assert_eq!(body.value.as_deref(), Some("See you at 7"));
    assert!(compose.keyboard_visible);
    assert!(compose.root.iter().any(|n| n.role == Role::Keyboard));
}

#[test]
fn iterator_is_depth_first_preorder() {
    let tree = UiNode::new(Role::Window, Rect::default()).with_children(vec![
        UiNode::new(Role::Table, Rect::default()).with_children(vec![
            UiNode::new(Role::Cell, Rect::default()).with_label("a"),
            UiNode::new(Role::Cell, Rect::default()).with_label("b"),
        ]),
        UiNode::new(Role::Button, Rect::default()).with_label("c"),
    ]);
    let order: Vec<String> = tree
        .iter()
        .map(|n| n.label.clone().unwrap_or_else(|| n.role.to_string()))
        .collect();
    assert_eq!(order, ["window", "table", "a", "b", "c"]);
    assert_eq!((&tree).into_iter().count(), 5);
}

#[test]
fn roles_parse_leniently_and_unknown_roles_round_trip() {
    for s in ["textField", "TextField", "text_field", "TEXT-FIELD"] {
        assert_eq!(Role::from_name(s), Role::TextField, "{s}");
    }
    let r: Role = serde_json::from_value(json!("datePicker")).unwrap();
    assert_eq!(r, Role::Other("datePicker".into()));
    assert_eq!(serde_json::to_value(&r).unwrap(), json!("datePicker"));
    let node: UiNode = serde_json::from_value(json!({"role": "Map"})).unwrap();
    assert_eq!(node.role, Role::Other("Map".into()));
    assert_eq!(serde_json::to_value(&node).unwrap()["role"], "Map");
    assert!(node.enabled && node.visible && !node.hittable);

    // Every known role serializes to snake_case and parses back to itself.
    let known = [
        Role::Button,
        Role::Link,
        Role::TextField,
        Role::SecureTextField,
        Role::TextView,
        Role::SearchField,
        Role::StaticText,
        Role::Image,
        Role::Cell,
        Role::Switch,
        Role::Slider,
        Role::Picker,
        Role::Tab,
        Role::NavigationBar,
        Role::TabBar,
        Role::Toolbar,
        Role::Alert,
        Role::Sheet,
        Role::Keyboard,
        Role::Key,
        Role::ScrollView,
        Role::Table,
        Role::CollectionView,
        Role::WebView,
        Role::Window,
        Role::Application,
    ];
    for r in known {
        let v = serde_json::to_value(&r).unwrap();
        let s = v.as_str().unwrap();
        assert!(s.chars().all(|c| c.is_ascii_lowercase() || c == '_'), "{s}");
        assert_eq!(serde_json::from_value::<Role>(v.clone()).unwrap(), r);
    }
}

#[test]
fn role_classification() {
    for r in [
        Role::TextField,
        Role::SecureTextField,
        Role::TextView,
        Role::SearchField,
    ] {
        assert!(r.is_text_input() && r.is_interactive(), "{r}");
    }
    for r in [
        Role::Button,
        Role::Link,
        Role::Cell,
        Role::Switch,
        Role::Key,
    ] {
        assert!(r.is_interactive() && !r.is_text_input(), "{r}");
    }
    for r in [
        Role::StaticText,
        Role::Image,
        Role::Table,
        Role::Other("x".into()),
    ] {
        assert!(!r.is_interactive(), "{r}");
    }
    assert!(Role::ScrollView.is_container() && !Role::Button.is_container());
}

#[test]
fn rect_geometry() {
    let r = Rect::new(10.0, 20.0, 100.0, 40.0);
    assert_eq!(r.center(), Point::new(60.0, 40.0));
    assert_eq!(r.area(), 4000.0);
    assert_eq!((r.max_x(), r.max_y()), (110.0, 60.0));
    assert!(r.contains(Point::new(10.0, 20.0)));
    assert!(r.contains(r.center()));
    assert!(
        !r.contains(Point::new(110.0, 30.0)),
        "right edge is exclusive"
    );
    assert!(
        !r.contains(Point::new(50.0, 60.0)),
        "bottom edge is exclusive"
    );

    let overlapping = Rect::new(100.0, 50.0, 50.0, 50.0);
    assert!(r.intersects(&overlapping) && overlapping.intersects(&r));
    assert_eq!(
        r.intersection(&overlapping),
        Some(Rect::new(100.0, 50.0, 10.0, 10.0))
    );
    let touching = Rect::new(110.0, 20.0, 10.0, 10.0);
    assert!(!r.intersects(&touching), "shared edge is not overlap");

    let empty = Rect::new(10.0, 20.0, 0.0, 40.0);
    assert!(empty.is_empty());
    assert_eq!(empty.area(), 0.0);
    assert!(!empty.contains(Point::new(10.0, 20.0)));
    assert!(!empty.intersects(&r));
    assert_eq!(Rect::new(0.0, 0.0, -5.0, 5.0).area(), 0.0);

    let screen = fixtures::IPHONE_SIZE.to_rect();
    let below_fold = Rect::new(16.0, 900.0, 361.0, 44.0);
    assert!(!screen.intersects(&below_fold));
}
