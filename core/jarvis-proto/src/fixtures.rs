//! Realistic UI trees for tests in this and other crates (feature `test-util`).
//!
//! Screens are modelled on an iPhone 15 in portrait (393 x 852 points) and mimic what the
//! XCUITest runner reports, including the noise compaction must handle: wrapper `Other`
//! nodes, zero-area nodes, hidden nodes and rows scrolled below the screen.

use crate::geometry::{Rect, Size};
use crate::ui::{Observation, Orientation, Role, Screen, UiNode};

/// iPhone 15 portrait screen size in points.
pub const IPHONE_SIZE: Size = Size::new(393.0, 852.0);

/// Shorthand for a rectangle.
pub const fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(x, y, width, height)
}

/// Shorthand for a visible, hittable node with a label.
pub fn node(role: Role, label: &str, frame: Rect) -> UiNode {
    UiNode::new(role, frame).with_label(label).hittable(true)
}

/// Shorthand for a non-hittable static text.
pub fn text(label: &str, frame: Rect) -> UiNode {
    UiNode::new(Role::StaticText, frame).with_label(label)
}

/// Wraps a root in a portrait iPhone `Screen`.
pub fn screen(bundle_id: &str, app_name: &str, root: UiNode, keyboard_visible: bool) -> Screen {
    Screen {
        app_bundle_id: bundle_id.to_owned(),
        app_name: Some(app_name.to_owned()),
        size: IPHONE_SIZE,
        root,
        keyboard_visible,
        orientation: Orientation::Portrait,
    }
}

/// Wraps a screen in an observation without a screenshot.
pub fn observation(screen: Screen) -> Observation {
    Observation::new(screen)
}

/// A Settings cell: hittable cell holding a title text and optional detail text.
fn settings_cell(title: &str, detail: Option<&str>, y: f64) -> UiNode {
    let mut children = vec![text(title, rect(60.0, y + 12.0, 200.0, 20.0))];
    if let Some(d) = detail {
        children.push(text(d, rect(260.0, y + 12.0, 100.0, 20.0)));
    }
    UiNode::new(Role::Cell, rect(16.0, y, 361.0, 44.0))
        .hittable(true)
        .with_children(children)
}

/// The Settings root list: Airplane Mode switch (off), Wi-Fi, Bluetooth, Notifications,
/// General, plus one row below the fold and a zero-area spacer.
pub fn settings_screen() -> Screen {
    let airplane = UiNode::new(Role::Cell, rect(16.0, 200.0, 361.0, 44.0))
        .hittable(true)
        .with_children(vec![
            text("Airplane Mode", rect(60.0, 212.0, 200.0, 20.0)),
            UiNode::new(Role::Switch, rect(312.0, 206.0, 51.0, 31.0))
                .with_label("Airplane Mode")
                .with_value("0")
                .with_identifier("airplane-mode-switch")
                .hittable(true),
        ]);
    let table = UiNode::new(Role::Table, rect(0.0, 140.0, 393.0, 712.0)).with_children(vec![
        UiNode::new(Role::SearchField, rect(16.0, 148.0, 361.0, 36.0))
            .with_label("Search")
            .with_placeholder("Search")
            .hittable(true),
        airplane,
        settings_cell("Wi-Fi", Some("HomeNet"), 244.0),
        settings_cell("Bluetooth", Some("On"), 288.0),
        settings_cell("Notifications", None, 352.0),
        settings_cell("General", None, 416.0),
        UiNode::new(
            Role::Other("other".to_owned()),
            rect(16.0, 460.0, 361.0, 0.0),
        ),
        settings_cell("Privacy & Security", None, 900.0).hittable(false),
    ]);
    let nav = UiNode::new(Role::NavigationBar, rect(0.0, 59.0, 393.0, 81.0))
        .with_identifier("Settings")
        .with_children(vec![text("Settings", rect(16.0, 96.0, 140.0, 41.0))]);
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Settings")
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(vec![nav, table])
        ]);
    screen("com.apple.Preferences", "Settings", root, false)
}

/// The software keyboard with a few letter keys, shift, delete, space and a return key
/// labelled `return_label` (e.g. "go", "send").
pub fn keyboard(return_label: &str) -> UiNode {
    UiNode::new(Role::Keyboard, rect(0.0, 516.0, 393.0, 336.0)).with_children(vec![
        node(Role::Key, "q", rect(3.0, 527.0, 37.0, 46.0)),
        node(Role::Key, "w", rect(42.0, 527.0, 37.0, 46.0)),
        node(Role::Key, "e", rect(81.0, 527.0, 37.0, 46.0)),
        node(Role::Button, "shift", rect(3.0, 635.0, 46.0, 46.0)),
        node(Role::Key, "delete", rect(344.0, 635.0, 46.0, 46.0)),
        node(Role::Key, "space", rect(100.0, 689.0, 193.0, 46.0)),
        node(Role::Button, return_label, rect(297.0, 689.0, 93.0, 46.0)),
    ])
}

/// A third-party login form: email filled in, focused empty password field, disabled
/// "Sign in" button, "Forgot password?" link and the keyboard up.
pub fn login_screen() -> Screen {
    let form = UiNode::new(
        Role::Other("other".to_owned()),
        rect(0.0, 200.0, 393.0, 300.0),
    )
    .with_children(vec![
        UiNode::new(Role::TextField, rect(24.0, 220.0, 345.0, 44.0))
            .with_label("Email")
            .with_value("alex@example.com")
            .with_identifier("login.email")
            .with_placeholder("you@example.com")
            .hittable(true),
        UiNode::new(Role::SecureTextField, rect(24.0, 280.0, 345.0, 44.0))
            .with_label("Password")
            .with_identifier("login.password")
            .with_placeholder("Password")
            .hittable(true)
            .focused(true),
        node(Role::Button, "Sign in", rect(24.0, 344.0, 345.0, 50.0))
            .with_identifier("login.submit")
            .enabled(false),
        node(
            Role::Link,
            "Forgot password?",
            rect(130.0, 410.0, 133.0, 20.0),
        ),
    ]);
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Acme")
        .with_children(vec![UiNode::new(Role::Window, IPHONE_SIZE.to_rect())
            .with_children(vec![
                text("Welcome back", rect(24.0, 120.0, 345.0, 40.0)),
                form,
                keyboard("go"),
            ])]);
    screen("com.example.acme", "Acme", root, true)
}

/// A Messages conversation with Alex: two bubbles, a draft in the focused message field,
/// an enabled Send button and the keyboard up.
pub fn compose_screen() -> Screen {
    let nav = UiNode::new(Role::NavigationBar, rect(0.0, 59.0, 393.0, 96.0)).with_children(vec![
        node(Role::Button, "Messages", rect(8.0, 70.0, 44.0, 44.0)).with_identifier("BackButton"),
        node(Role::Button, "Alex", rect(150.0, 66.0, 93.0, 80.0)),
        node(Role::Button, "FaceTime", rect(341.0, 70.0, 44.0, 44.0)),
    ]);
    let transcript = UiNode::new(Role::CollectionView, rect(0.0, 155.0, 393.0, 300.0))
        .with_identifier("TranscriptCollectionView")
        .with_children(vec![
            UiNode::new(Role::Cell, rect(0.0, 300.0, 393.0, 60.0)).with_children(vec![text(
                "Dinner tonight?",
                rect(16.0, 310.0, 180.0, 40.0),
            )]),
            UiNode::new(Role::Cell, rect(0.0, 370.0, 393.0, 60.0)).with_children(vec![text(
                "Sure, what time?",
                rect(200.0, 380.0, 177.0, 40.0),
            )]),
        ]);
    let entry = UiNode::new(
        Role::Other("other".to_owned()),
        rect(0.0, 460.0, 393.0, 56.0),
    )
    .with_children(vec![
        node(Role::Button, "Apps", rect(8.0, 468.0, 40.0, 40.0)),
        UiNode::new(Role::TextView, rect(56.0, 468.0, 285.0, 40.0))
            .with_label("Message")
            .with_value("See you at 7")
            .with_identifier("messageBodyField")
            .hittable(true)
            .focused(true),
        node(Role::Button, "Send", rect(345.0, 470.0, 36.0, 36.0)).with_identifier("sendButton"),
    ]);
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Messages")
        .with_children(vec![UiNode::new(Role::Window, IPHONE_SIZE.to_rect())
            .with_children(vec![nav, transcript, entry, keyboard("send")])]);
    screen("com.apple.MobileSMS", "Messages", root, true)
}
