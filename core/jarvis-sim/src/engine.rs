//! Turning an app's definition plus its state into a `UiNode` tree and a hit-test table.
//!
//! The tree is what the runner would report. The hit table (in paint order, later entries are
//! on top) is what a tap resolves against: the topmost entry containing the point wins.

use std::collections::BTreeMap;

use jarvis_proto::{Point, Rect, Role, Size, UiNode};
use serde_json::Value;

use crate::app::{AlertDef, AppDef, Effect, ElementDef};
use crate::expr::{display, eval_bool, render, render_string, Env};

/// Screen width in points (iPhone 15, portrait).
pub const SCREEN_W: f64 = 393.0;
/// Screen height in points.
pub const SCREEN_H: f64 = 852.0;
/// The screen as a size.
pub const SCREEN_SIZE: Size = Size::new(SCREEN_W, SCREEN_H);

const NAV_Y: f64 = 59.0;
const NAV_H: f64 = 85.0;
const KEYBOARD_Y: f64 = 516.0;
const ALERT_W: f64 = 300.0;

/// A running app: definition, state and navigation.
#[derive(Debug, Clone)]
pub struct AppRuntime {
    /// The definition.
    pub def: AppDef,
    /// Current state variables.
    pub state: Value,
    /// Screen names, last is current.
    pub nav: Vec<String>,
    /// The alert on screen, if any.
    pub alert: Option<ActiveAlert>,
    /// Scroll offsets by region key.
    pub scroll: BTreeMap<String, f64>,
}

/// An alert that is showing.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveAlert {
    /// Alert name in the app definition.
    pub id: String,
    /// The `for_each` scope of the element that opened it.
    pub scope: Value,
}

impl AppRuntime {
    /// A freshly launched app.
    pub fn new(def: AppDef) -> Self {
        let state = if def.state.is_object() {
            def.state.clone()
        } else {
            Value::Object(serde_json::Map::new())
        };
        let nav = vec![def.start.clone()];
        Self {
            def,
            state,
            nav,
            alert: None,
            scroll: BTreeMap::new(),
        }
    }

    /// Name of the screen on top of the navigation stack.
    pub fn screen_name(&self) -> &str {
        self.nav
            .last()
            .map_or(self.def.start.as_str(), String::as_str)
    }
}

/// What a tap on a hit region does.
#[derive(Debug, Clone, PartialEq)]
pub enum HitAction {
    /// Run the hit's `on_tap` effects (and focus it when it is a text input).
    Effects,
    /// A keyboard key.
    Key(String),
    /// Nothing; the region only absorbs the tap.
    Swallow,
}

/// One tappable region.
#[derive(Debug, Clone)]
pub struct Hit {
    /// Where it is on screen (clipped to its scroll viewport).
    pub frame: Rect,
    /// Role, for transcripts.
    pub role: Role,
    /// Label, for transcripts.
    pub label: String,
    /// Focus key for text inputs.
    pub key: Option<String>,
    /// What happens.
    pub action: HitAction,
    /// Effects of a tap.
    pub on_tap: Vec<Effect>,
    /// Effects of Return while focused.
    pub on_submit: Vec<Effect>,
    /// State variable a text input edits.
    pub bind: Option<String>,
    /// Whether taps do anything.
    pub enabled: bool,
    /// The `for_each` scope its effects run in.
    pub scope: Value,
}

impl Hit {
    /// `Role "label"` for transcripts.
    pub fn describe(&self) -> String {
        if self.label.is_empty() {
            self.role.to_string()
        } else {
            format!("{} \"{}\"", self.role, self.label)
        }
    }
}

/// A scrollable viewport.
#[derive(Debug, Clone)]
pub struct ScrollRegion {
    /// Offset key.
    pub key: String,
    /// Viewport on screen.
    pub frame: Rect,
    /// Total content height.
    pub content_h: f64,
}

/// A rendered screen.
#[derive(Debug, Clone)]
pub struct Rendered {
    /// The `Application` root.
    pub root: UiNode,
    /// Hit regions, bottom to top.
    pub hits: Vec<Hit>,
    /// Scroll viewports, bottom to top.
    pub scrolls: Vec<ScrollRegion>,
    /// Whether the keyboard is up.
    pub keyboard: bool,
    /// Whether the focused input exists on this screen.
    pub focus_found: bool,
}

struct Builder<'a> {
    rt: &'a AppRuntime,
    focus: Option<&'a str>,
    hits: Vec<Hit>,
    scrolls: Vec<ScrollRegion>,
    focus_found: bool,
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(x, y, w, h)
}

fn shift(node: &mut UiNode, dy: f64) {
    node.frame.y += dy;
    for c in &mut node.children {
        shift(c, dy);
    }
}

fn bullets(s: &str) -> String {
    "\u{2022}".repeat(s.chars().count())
}

/// Reads a dotted path (`thread.Mom`) from a value; missing is `null`.
pub fn get_path(root: &Value, path: &str) -> Value {
    let mut cur = root;
    for seg in path.split('.').filter(|s| !s.is_empty()) {
        let next = match cur {
            Value::Object(o) => o.get(seg),
            Value::Array(a) => seg.parse::<usize>().ok().and_then(|i| a.get(i)),
            _ => None,
        };
        match next {
            Some(n) => cur = n,
            None => return Value::Null,
        }
    }
    cur.clone()
}

/// A mutable reference to an existing path, if every step exists.
pub fn path_mut<'v>(root: &'v mut Value, path: &str) -> Option<&'v mut Value> {
    let mut cur = root;
    for seg in path.split('.').filter(|s| !s.is_empty()) {
        cur = match cur {
            Value::Object(o) => o.get_mut(seg)?,
            Value::Array(a) => a.get_mut(seg.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

/// Assigns `val` at `path`, creating missing object levels. False when the path is blocked.
pub fn set_path(root: &mut Value, path: &str, val: Value) -> bool {
    fn go(v: &mut Value, segs: &[&str], val: Value) -> bool {
        let Some((head, rest)) = segs.split_first() else {
            *v = val;
            return true;
        };
        match v {
            Value::Object(o) => {
                let child = o.entry((*head).to_owned()).or_insert(Value::Null);
                if !rest.is_empty() && child.is_null() {
                    *child = Value::Object(serde_json::Map::new());
                }
                go(child, rest, val)
            }
            Value::Array(a) => match head.parse::<usize>().ok().and_then(|i| a.get_mut(i)) {
                Some(c) => go(c, rest, val),
                None => false,
            },
            _ => false,
        }
    }
    let segs: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    go(root, &segs, val)
}

impl<'a> Builder<'a> {
    fn env<'e>(&'e self, scope: &'e Value) -> Env<'e> {
        Env::new().with(scope).with(&self.rt.state)
    }

    fn text(&self, template: &str, scope: &Value) -> String {
        render_string(template, &self.env(scope))
    }

    fn truth(&self, expr: &str, scope: &Value, default: bool) -> bool {
        eval_bool(expr, &self.env(scope)).unwrap_or(default)
    }

    /// The scopes an element is instantiated in (one, or one per list item).
    fn instances(&self, def: &ElementDef, scope: &Value) -> Vec<Value> {
        let Some(list_expr) = &def.for_each else {
            return vec![scope.clone()];
        };
        let items = match crate::expr::eval(list_expr, &self.env(scope)) {
            Ok(Value::Array(a)) => a,
            _ => Vec::new(),
        };
        let name = def.item.clone().unwrap_or_else(|| "item".to_owned());
        items
            .into_iter()
            .enumerate()
            .map(|(i, item)| {
                let mut s = scope.as_object().cloned().unwrap_or_default();
                s.insert(name.clone(), item);
                s.insert("index".to_owned(), Value::from(i));
                Value::Object(s)
            })
            .collect()
    }

    fn build_list(
        &mut self,
        defs: &[ElementDef],
        parent: Rect,
        column: bool,
        gap: f64,
        scope: &Value,
        cursor: &mut f64,
    ) -> Vec<UiNode> {
        let (origin, size) = (parent.origin(), parent.size());
        let mut out = Vec::new();
        for def in defs {
            for sc in self.instances(def, scope) {
                if let Some(w) = &def.when {
                    if !self.truth(w, &sc, false) {
                        continue;
                    }
                }
                let rel = match def.frame {
                    Some([x, y, w, h]) => rect(x, y, w, h),
                    None if column => {
                        let h = def.h.unwrap_or(44.0);
                        let r = rect(0.0, *cursor, size.width, h);
                        *cursor += h + gap;
                        r
                    }
                    None => rect(0.0, 0.0, size.width, def.h.unwrap_or(size.height)),
                };
                let node = self.build_node(def, rel, origin, &sc);
                out.push(node);
            }
        }
        out
    }

    fn push_hit(&mut self, hit: Hit) {
        self.hits.push(hit);
    }

    fn build_node(&mut self, def: &ElementDef, rel: Rect, origin: Point, scope: &Value) -> UiNode {
        let abs = rect(origin.x + rel.x, origin.y + rel.y, rel.width, rel.height);
        let row = def.kind.as_deref() == Some("row");
        let role = if row {
            Role::Cell
        } else {
            Role::from_name(def.role.as_deref().unwrap_or("other"))
        };
        let id = def.id.as_deref().map(|i| self.text(i, scope));
        let key = id.clone().or_else(|| def.bind.clone());
        let label = def
            .label
            .as_deref()
            .map(|l| self.text(l, scope))
            .unwrap_or_default();
        let enabled = def
            .enabled
            .as_deref()
            .is_none_or(|e| self.truth(e, scope, false));
        let selected = def
            .selected
            .as_deref()
            .is_some_and(|e| self.truth(e, scope, false));
        let focused = def.bind.is_some() && key.is_some() && key.as_deref() == self.focus;
        if focused {
            self.focus_found = true;
        }
        let mut node = UiNode::new(role.clone(), abs)
            .enabled(enabled)
            .selected(selected);
        if def.secure {
            node = node.secure(true);
        }
        if !row && !label.is_empty() {
            node = node.with_label(label.clone());
        }
        if let Some(i) = &id {
            node = node.with_identifier(i.clone());
        }
        if let Some(p) = &def.placeholder {
            node = node.with_placeholder(self.text(p, scope));
        }
        let value = match &def.bind {
            Some(b) => {
                let v = display(&get_path(&self.rt.state, b));
                match (v.is_empty(), def.secure || role == Role::SecureTextField) {
                    (true, _) => None,
                    (false, true) => Some(bullets(&v)),
                    (false, false) => Some(v),
                }
            }
            None => def
                .value
                .as_deref()
                .map(|v| self.text(v, scope))
                .filter(|v| !v.is_empty()),
        };
        if let Some(v) = value {
            node = node.with_value(v);
        }
        node = node.focused(focused);
        let interactive = !def.on_tap.is_empty() || def.bind.is_some() || def.opaque;
        if interactive {
            node = node.hittable(true);
            self.push_hit(Hit {
                frame: abs,
                role: role.clone(),
                label: label.clone(),
                key: key.clone(),
                action: if def.on_tap.is_empty() && def.bind.is_none() {
                    HitAction::Swallow
                } else {
                    HitAction::Effects
                },
                on_tap: def.on_tap.clone(),
                on_submit: def.on_submit.clone(),
                bind: def.bind.clone(),
                enabled,
                scope: scope.clone(),
            });
        }
        if row {
            node.children = self.row_children(def, &label, abs, scope);
            return node;
        }
        if def.scroll {
            node.children = self.scroll_children(def, id.as_deref(), abs, scope);
            return node;
        }
        let column = def.layout.as_deref() == Some("column");
        let mut cursor = 0.0;
        node.children = self.build_list(&def.children, abs, column, def.gap, scope, &mut cursor);
        node
    }

    fn row_children(
        &mut self,
        def: &ElementDef,
        title: &str,
        abs: Rect,
        scope: &Value,
    ) -> Vec<UiNode> {
        let ty = abs.y + (abs.height - 20.0) / 2.0;
        let mut kids = vec![UiNode::new(
            Role::StaticText,
            rect(abs.x + 16.0, ty, abs.width - 100.0, 20.0),
        )
        .with_label(title)];
        if let Some(sw) = &def.switch {
            let var = self.text(sw, scope);
            let on = crate::expr::truthy(&get_path(&self.rt.state, &var));
            let frame = rect(
                abs.x + abs.width - 67.0,
                abs.y + (abs.height - 31.0) / 2.0,
                51.0,
                31.0,
            );
            kids.push(
                UiNode::new(Role::Switch, frame)
                    .with_label(title)
                    .with_value(if on { "1" } else { "0" })
                    .hittable(true),
            );
            self.push_hit(Hit {
                frame,
                role: Role::Switch,
                label: title.to_owned(),
                key: None,
                action: HitAction::Effects,
                on_tap: vec![Effect::Toggle { var }],
                on_submit: Vec::new(),
                bind: None,
                enabled: true,
                scope: scope.clone(),
            });
        } else if let Some(d) = &def.detail {
            let detail = self.text(d, scope);
            if !detail.is_empty() {
                kids.push(
                    UiNode::new(
                        Role::StaticText,
                        rect(abs.x + abs.width - 190.0, ty, 160.0, 20.0),
                    )
                    .with_label(detail),
                );
            }
        }
        kids
    }

    fn scroll_children(
        &mut self,
        def: &ElementDef,
        id: Option<&str>,
        abs: Rect,
        scope: &Value,
    ) -> Vec<UiNode> {
        let start = self.hits.len();
        let mut cursor = 0.0;
        let mut kids = self.build_list(&def.children, abs, true, def.gap, scope, &mut cursor);
        let content_h = (cursor - def.gap).max(0.0);
        let key = format!("{}/{}", self.rt.screen_name(), id.unwrap_or("scroll"));
        let max = (content_h - abs.height).max(0.0);
        let offset = self
            .rt
            .scroll
            .get(&key)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, max);
        for k in &mut kids {
            shift(k, -offset);
        }
        kids.retain(|k| k.frame.intersects(&abs));
        let tail = self.hits.split_off(start);
        for mut h in tail {
            h.frame.y -= offset;
            if let Some(clipped) = h.frame.intersection(&abs) {
                h.frame = clipped;
                self.hits.push(h);
            }
        }
        self.scrolls.push(ScrollRegion {
            key,
            frame: abs,
            content_h,
        });
        kids
    }

    fn nav_bar(&mut self, title: &str, buttons: &[ElementDef]) -> UiNode {
        let bar = rect(0.0, NAV_Y, SCREEN_W, NAV_H);
        let mut kids = Vec::new();
        if self.rt.nav.len() > 1 {
            let prev = self
                .rt
                .nav
                .get(self.rt.nav.len() - 2)
                .and_then(|n| self.rt.def.screens.get(n))
                .and_then(|s| s.title.clone())
                .unwrap_or_else(|| "Back".to_owned());
            let frame = rect(8.0, NAV_Y + 4.0, 100.0, 40.0);
            kids.push(
                UiNode::new(Role::Button, frame)
                    .with_label(prev.clone())
                    .with_identifier("BackButton")
                    .hittable(true),
            );
            self.push_hit(Hit {
                frame,
                role: Role::Button,
                label: prev,
                key: None,
                action: HitAction::Effects,
                on_tap: vec![Effect::Back],
                on_submit: Vec::new(),
                bind: None,
                enabled: true,
                scope: Value::Null,
            });
        }
        kids.push(
            UiNode::new(
                Role::StaticText,
                rect(16.0, NAV_Y + 41.0, SCREEN_W - 32.0, 41.0),
            )
            .with_label(title),
        );
        let mut cursor = 0.0;
        let scope = Value::Object(serde_json::Map::new());
        kids.extend(self.build_list(buttons, bar, false, 0.0, &scope, &mut cursor));
        UiNode::new(Role::NavigationBar, bar)
            .with_identifier(title)
            .with_children(kids)
    }

    fn alert(&mut self, def: &AlertDef, scope: &Value) -> UiNode {
        self.push_hit(Hit {
            frame: rect(0.0, 0.0, SCREEN_W, SCREEN_H),
            role: Role::Other("other".to_owned()),
            label: String::new(),
            key: None,
            action: HitAction::Swallow,
            on_tap: Vec::new(),
            on_submit: Vec::new(),
            bind: None,
            enabled: true,
            scope: Value::Null,
        });
        let h = 100.0 + 44.0 * def.buttons.len() as f64;
        let x = (SCREEN_W - ALERT_W) / 2.0;
        let y = 300.0;
        let title = self.text(&def.title, scope);
        let message = self.text(&def.message, scope);
        let mut kids = vec![UiNode::new(
            Role::StaticText,
            rect(x + 16.0, y + 16.0, ALERT_W - 32.0, 22.0),
        )
        .with_label(title)];
        if !message.is_empty() {
            kids.push(
                UiNode::new(
                    Role::StaticText,
                    rect(x + 16.0, y + 44.0, ALERT_W - 32.0, 44.0),
                )
                .with_label(message),
            );
        }
        for (i, b) in def.buttons.iter().enumerate() {
            let frame = rect(x, y + 100.0 + 44.0 * i as f64, ALERT_W, 44.0);
            let label = self.text(&b.label, scope);
            kids.push(
                UiNode::new(Role::Button, frame)
                    .with_label(label.clone())
                    .hittable(true),
            );
            let mut effects = vec![Effect::Dismiss];
            effects.extend(b.then.iter().cloned());
            self.push_hit(Hit {
                frame,
                role: Role::Button,
                label,
                key: None,
                action: HitAction::Effects,
                on_tap: effects,
                on_submit: Vec::new(),
                bind: None,
                enabled: true,
                scope: scope.clone(),
            });
        }
        UiNode::new(Role::Alert, rect(x, y, ALERT_W, h)).with_children(kids)
    }

    fn keyboard(&mut self) -> UiNode {
        let frame = rect(0.0, KEYBOARD_Y, SCREEN_W, SCREEN_H - KEYBOARD_Y);
        self.push_hit(Hit {
            frame,
            role: Role::Keyboard,
            label: String::new(),
            key: None,
            action: HitAction::Swallow,
            on_tap: Vec::new(),
            on_submit: Vec::new(),
            bind: None,
            enabled: true,
            scope: Value::Null,
        });
        let mut keys = Vec::new();
        let mut add = |this: &mut Self, role: Role, label: &str, f: Rect| {
            keys.push(
                UiNode::new(role.clone(), f)
                    .with_label(label)
                    .hittable(true),
            );
            this.push_hit(Hit {
                frame: f,
                role,
                label: label.to_owned(),
                key: None,
                action: HitAction::Key(label.to_owned()),
                on_tap: Vec::new(),
                on_submit: Vec::new(),
                bind: None,
                enabled: true,
                scope: Value::Null,
            });
        };
        let rows = ["qwertyuiop", "asdfghjkl", "zxcvbnm"];
        for (r, row) in rows.iter().enumerate() {
            let n = row.chars().count() as f64;
            let w = 37.0;
            let x0 = (SCREEN_W - n * 39.0 + 2.0) / 2.0;
            for (i, c) in row.chars().enumerate() {
                let f = rect(
                    x0 + 39.0 * i as f64,
                    KEYBOARD_Y + 11.0 + 54.0 * r as f64,
                    w,
                    46.0,
                );
                add(self, Role::Key, &c.to_string(), f);
            }
        }
        add(
            self,
            Role::Key,
            "delete",
            rect(344.0, KEYBOARD_Y + 119.0, 46.0, 46.0),
        );
        add(
            self,
            Role::Key,
            "space",
            rect(100.0, KEYBOARD_Y + 173.0, 193.0, 46.0),
        );
        add(
            self,
            Role::Button,
            "return",
            rect(297.0, KEYBOARD_Y + 173.0, 93.0, 46.0),
        );
        UiNode::new(Role::Keyboard, frame).with_children(keys)
    }
}

/// Renders the app's current screen. `focus` is the key of the focused text input.
pub fn render_app(rt: &AppRuntime, focus: Option<&str>) -> Rendered {
    let mut b = Builder {
        rt,
        focus,
        hits: Vec::new(),
        scrolls: Vec::new(),
        focus_found: false,
    };
    let empty = Value::Object(serde_json::Map::new());
    let mut kids = Vec::new();
    if let Some(screen) = rt.def.screens.get(rt.screen_name()) {
        let mut cursor = 0.0;
        kids.extend(b.build_list(
            &screen.elements,
            rect(0.0, 0.0, SCREEN_W, SCREEN_H),
            false,
            0.0,
            &empty,
            &mut cursor,
        ));
        if let Some(title) = &screen.title {
            let t = render_string(title, &Env::new().with(&rt.state));
            let bar = b.nav_bar(&t, &screen.nav_buttons);
            kids.push(bar);
        }
    }
    let alert = rt
        .alert
        .as_ref()
        .and_then(|a| rt.def.alerts.get(&a.id).map(|d| (d, a.scope.clone())));
    let mut keyboard = false;
    if let Some((def, scope)) = alert {
        let node = b.alert(def, &scope);
        kids.push(node);
    } else if b.focus_found {
        keyboard = true;
        let node = b.keyboard();
        kids.push(node);
    }
    let full = rect(0.0, 0.0, SCREEN_W, SCREEN_H);
    let root = UiNode::new(Role::Application, full)
        .with_label(rt.def.name.clone())
        .with_children(vec![UiNode::new(Role::Window, full).with_children(kids)]);
    Rendered {
        root,
        hits: b.hits,
        scrolls: b.scrolls,
        keyboard,
        focus_found: b.focus_found,
    }
}

/// Renders a template against a state (used by predicates and tests).
pub fn render_value(template: &str, state: &Value) -> Value {
    render(template, &Env::new().with(state))
}
