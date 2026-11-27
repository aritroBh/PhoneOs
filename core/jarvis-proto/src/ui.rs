//! The accessibility tree as the XCUITest runner reports it.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::geometry::{Rect, Size};

/// The XCUIElement type of a node, restricted to the types the agent cares about.
///
/// Serialized as a snake_case string (`"text_field"`). Deserialization is lenient about
/// case and separators, so `"TextField"`, `"textField"` and `"text_field"` all parse to
/// [`Role::TextField`]. Any other string becomes [`Role::Other`] holding the original text,
/// which serializes back unchanged. Build `Other` through [`Role::from_name`] so that it
/// never holds the name of a known role (which would not round-trip).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Role {
    /// A push button.
    Button,
    /// A hyperlink.
    Link,
    /// A single-line text field.
    TextField,
    /// A password field; its value is never exposed.
    SecureTextField,
    /// A multi-line text view.
    TextView,
    /// A search field.
    SearchField,
    /// Non-interactive text.
    StaticText,
    /// An image.
    Image,
    /// A table or collection cell.
    Cell,
    /// An on/off switch.
    Switch,
    /// A slider.
    Slider,
    /// A picker or picker wheel.
    Picker,
    /// A tab inside a tab bar or segmented control.
    Tab,
    /// A navigation bar.
    NavigationBar,
    /// A tab bar.
    TabBar,
    /// A toolbar.
    Toolbar,
    /// A system or app alert.
    Alert,
    /// An action sheet.
    Sheet,
    /// The software keyboard.
    Keyboard,
    /// One key of the software keyboard.
    Key,
    /// A scroll view.
    ScrollView,
    /// A table view.
    Table,
    /// A collection view.
    CollectionView,
    /// A web view.
    WebView,
    /// A window.
    Window,
    /// The application root.
    Application,
    /// Any other element type, holding the runner's original type name.
    Other(String),
}

/// Known roles and their canonical snake_case names.
const KNOWN_ROLES: &[(Role, &str)] = &[
    (Role::Button, "button"),
    (Role::Link, "link"),
    (Role::TextField, "text_field"),
    (Role::SecureTextField, "secure_text_field"),
    (Role::TextView, "text_view"),
    (Role::SearchField, "search_field"),
    (Role::StaticText, "static_text"),
    (Role::Image, "image"),
    (Role::Cell, "cell"),
    (Role::Switch, "switch"),
    (Role::Slider, "slider"),
    (Role::Picker, "picker"),
    (Role::Tab, "tab"),
    (Role::NavigationBar, "navigation_bar"),
    (Role::TabBar, "tab_bar"),
    (Role::Toolbar, "toolbar"),
    (Role::Alert, "alert"),
    (Role::Sheet, "sheet"),
    (Role::Keyboard, "keyboard"),
    (Role::Key, "key"),
    (Role::ScrollView, "scroll_view"),
    (Role::Table, "table"),
    (Role::CollectionView, "collection_view"),
    (Role::WebView, "web_view"),
    (Role::Window, "window"),
    (Role::Application, "application"),
];

/// Lowercases and drops `_`, `-` and spaces so naming styles compare equal.
fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, '_' | '-' | ' '))
        .flat_map(char::to_lowercase)
        .collect()
}

impl Role {
    /// Parses a runner type name leniently; unknown names become [`Role::Other`].
    pub fn from_name(name: &str) -> Role {
        let key = normalize(name);
        KNOWN_ROLES
            .iter()
            .find(|(_, n)| normalize(n) == key)
            .map(|(r, _)| r.clone())
            .unwrap_or_else(|| Role::Other(name.to_owned()))
    }

    /// The serialized name: snake_case for known roles, the original text for `Other`.
    pub fn as_str(&self) -> &str {
        match self {
            Role::Other(s) => s,
            known => KNOWN_ROLES
                .iter()
                .find(|(r, _)| r == known)
                .map(|(_, n)| *n)
                .unwrap_or("other"),
        }
    }

    /// True for roles that accept typed text.
    pub fn is_text_input(&self) -> bool {
        matches!(
            self,
            Role::TextField | Role::SecureTextField | Role::TextView | Role::SearchField
        )
    }

    /// Probe run 134 (iOS 26.5): whether this is an item of an edit or context menu —
    /// XCUITest's raw type `MenuItem`, which the shell keeps as `Role::Other("MenuItem")`
    /// (it used to map it to [`Role::Button`], so the core never saw a menu item). A menu
    /// item is pressed like a button: it is interactive, listed as a control and takes taps
    /// everywhere a button does.
    pub fn is_menu_item(&self) -> bool {
        match self {
            Role::Other(name) => normalize(name) == "menuitem",
            _ => false,
        }
    }

    /// True for roles a user can act on directly (tap, type, toggle, drag). A menu item
    /// ([`Role::is_menu_item`]) is one.
    pub fn is_interactive(&self) -> bool {
        self.is_text_input()
            || self.is_menu_item()
            || matches!(
                self,
                Role::Button
                    | Role::Link
                    | Role::Cell
                    | Role::Switch
                    | Role::Slider
                    | Role::Picker
                    | Role::Tab
                    | Role::Key
            )
    }

    /// True for structural roles that mainly group other elements.
    pub fn is_container(&self) -> bool {
        matches!(
            self,
            Role::NavigationBar
                | Role::TabBar
                | Role::Toolbar
                | Role::Alert
                | Role::Sheet
                | Role::Keyboard
                | Role::ScrollView
                | Role::Table
                | Role::CollectionView
                | Role::WebView
                | Role::Window
                | Role::Application
                | Role::Other(_)
        )
    }
}

impl Default for Role {
    /// XCUIElement's own catch-all type, `other`.
    fn default() -> Self {
        Role::Other("other".to_owned())
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Role {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Role {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Role::from_name(&s))
    }
}

/// One node of the accessibility tree.
///
/// Every field has a serde default, so runner JSON may omit fields: missing booleans are
/// `false` except `enabled` and `visible`, which default to `true` (the common case).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiNode {
    /// Element type.
    pub role: Role,
    /// Accessibility label.
    pub label: Option<String>,
    /// Current value (text content, switch state "0"/"1", slider percentage...).
    pub value: Option<String>,
    /// Developer-assigned accessibility identifier.
    pub identifier: Option<String>,
    /// Placeholder text of an empty text input.
    pub placeholder: Option<String>,
    /// Frame in screen points.
    pub frame: Rect,
    /// Whether the element accepts interaction.
    pub enabled: bool,
    /// Whether the element is on screen and not hidden.
    pub visible: bool,
    /// Whether a tap at its centre would reach it.
    pub hittable: bool,
    /// Whether it has keyboard focus.
    pub focused: bool,
    /// Whether it is selected (tabs, cells, segments).
    pub selected: bool,
    /// Whether it is a secure (password) field.
    pub secure: bool,
    /// Child nodes in runner order.
    pub children: Vec<UiNode>,
}

impl Default for UiNode {
    fn default() -> Self {
        Self {
            role: Role::default(),
            label: None,
            value: None,
            identifier: None,
            placeholder: None,
            frame: Rect::default(),
            enabled: true,
            visible: true,
            hittable: false,
            focused: false,
            selected: false,
            secure: false,
            children: Vec::new(),
        }
    }
}

impl UiNode {
    /// Creates a node with the given role and frame; `secure` is set for `SecureTextField`.
    pub fn new(role: Role, frame: Rect) -> Self {
        let secure = role == Role::SecureTextField;
        Self {
            role,
            frame,
            secure,
            ..Self::default()
        }
    }

    /// Sets the label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Sets the value.
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Sets the identifier.
    pub fn with_identifier(mut self, identifier: impl Into<String>) -> Self {
        self.identifier = Some(identifier.into());
        self
    }

    /// Sets the placeholder.
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Sets `hittable`.
    pub fn hittable(mut self, hittable: bool) -> Self {
        self.hittable = hittable;
        self
    }

    /// Sets `enabled`.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Sets `visible`.
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Sets `focused`.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Sets `selected`.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Sets `secure`.
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    /// Replaces the children.
    pub fn with_children(mut self, children: Vec<UiNode>) -> Self {
        self.children = children;
        self
    }

    /// Appends one child.
    pub fn push_child(mut self, child: UiNode) -> Self {
        self.children.push(child);
        self
    }

    /// Depth-first, pre-order iterator over this node and all descendants.
    pub fn iter(&self) -> Iter<'_> {
        Iter { stack: vec![self] }
    }

    /// The first node in depth-first pre-order matching `pred`.
    pub fn find(&self, mut pred: impl FnMut(&UiNode) -> bool) -> Option<&UiNode> {
        self.iter().find(|n| pred(n))
    }

    /// Number of nodes in this subtree, including `self`.
    pub fn count(&self) -> usize {
        self.iter().count()
    }
}

/// Depth-first, pre-order iterator over a [`UiNode`] subtree.
#[derive(Debug, Clone)]
pub struct Iter<'a> {
    stack: Vec<&'a UiNode>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a UiNode;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.stack.pop()?;
        self.stack.extend(node.children.iter().rev());
        Some(node)
    }
}

impl<'a> IntoIterator for &'a UiNode {
    type Item = &'a UiNode;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Interface orientation of the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    /// Upright portrait.
    #[default]
    Portrait,
    /// Portrait, upside down.
    PortraitUpsideDown,
    /// Landscape, home indicator on the right.
    LandscapeLeft,
    /// Landscape, home indicator on the left.
    LandscapeRight,
}

/// One snapshot of the foreground app's accessibility tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Screen {
    /// Bundle id of the foreground app (`com.apple.springboard` for the home screen).
    pub app_bundle_id: String,
    /// Display name of the foreground app, when known.
    #[serde(default)]
    pub app_name: Option<String>,
    /// Screen size in points for the current orientation.
    pub size: Size,
    /// Root of the tree (normally an `Application` node).
    pub root: UiNode,
    /// Whether the software keyboard is up.
    #[serde(default)]
    pub keyboard_visible: bool,
    /// Current interface orientation.
    #[serde(default)]
    pub orientation: Orientation,
    /// Review pass 31 (L1): an intake cut the tree at [`crate::MAX_TREE_DEPTH`] levels (or
    /// the runner wire at its node count): part of the screen was not copied. The core reads
    /// such a screen as an incomplete one (`ScreenCues::truncated`, every action that touches
    /// it asks; Home works). Every intake that cuts sets it (the shell's first copy, the
    /// typed-tree builder, the runner wire's encoder, the FFI conversion, the runner-text
    /// parser), so a cut never arrives unflagged. Absent in JSON written before: `false`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tree_cut: bool,
}

/// Review pass 31 (L1): `root` cut, without recursion, so that no node lies deeper than
/// [`crate::MAX_TREE_DEPTH`] levels (the root is level 1): the children of every node at the
/// bound are dropped. Returns whether anything was dropped. The dropped subtrees are
/// released iteratively too (a deep tree's own `Drop` recurses).
pub fn cut_tree(root: &mut UiNode) -> bool {
    let mut cut = false;
    let mut dropped: Vec<UiNode> = Vec::new();
    let mut stack: Vec<(&mut UiNode, usize)> = vec![(root, 1)];
    while let Some((node, depth)) = stack.pop() {
        if depth >= crate::MAX_TREE_DEPTH {
            if !node.children.is_empty() {
                cut = true;
                dropped.append(&mut node.children);
            }
            continue;
        }
        for c in node.children.iter_mut() {
            stack.push((c, depth + 1));
        }
    }
    drop_iteratively(dropped);
    cut
}

/// Releases `nodes` and everything under them without recursing on depth (a `UiNode`'s
/// own `Drop` recurses once per level).
pub fn drop_iteratively(mut nodes: Vec<UiNode>) {
    while let Some(mut n) = nodes.pop() {
        nodes.append(&mut n.children);
    }
}

impl Screen {
    /// The screen bounds as a rectangle at the origin.
    pub fn bounds(&self) -> Rect {
        self.size.to_rect()
    }
}

/// A handle to a screenshot the shell keeps; pixels never enter the core.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ImageRef {
    /// Shell-assigned opaque id.
    pub id: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// What the shell hands back for an `Effect::Observe`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    /// The accessibility snapshot.
    pub screen: Screen,
    /// Screenshot handle, present when one was requested and captured.
    #[serde(default)]
    pub screenshot: Option<ImageRef>,
    /// The runner reports the device as locked (`screen_locked`): the snapshot is of the
    /// lock screen (or black) and must not be acted on. The session ignores `screen` and
    /// waits for the owner to unlock (see `Effect::AwaitUnlock`). Absent in JSON written
    /// before lock awareness existed, which reads as `false`.
    #[serde(default)]
    pub device_locked: bool,
}

impl Observation {
    /// An unlocked observation of `screen` without a screenshot.
    pub fn new(screen: Screen) -> Self {
        Self {
            screen,
            screenshot: None,
            device_locked: false,
        }
    }
}
