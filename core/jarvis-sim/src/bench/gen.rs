//! Deterministic synthetic accessibility trees of a chosen size and shape.
//!
//! Every tree has the chrome of a real app screen (application, window, two pass-through
//! wrappers, a navigation bar with a title, a back button and a search field, a tab bar with
//! four tabs) around one content region:
//!
//! * [`Shape::List`]: a table of cells. Each cell has an image, a title and a subtitle; one
//!   in eight also has a "Details" button, one in sixteen a switch.
//! * [`Shape::Deep`]: the items sit at the end of a spine of [`DEEP_LEVELS`] nested
//!   full-screen wrappers (the spine is deeper than the core's depth cap of 160, which is
//!   part of what it measures). Each item is a button holding its text.
//! * [`Shape::Web`]: a web view of groups: headings, paragraphs of 80 to 220 characters,
//!   links (half of them with the text as a child, as WebKit reports them) and images.
//!
//! Two layouts ([`View`]): `Page` lays the content out on a tall page the way a scroll view
//! or web page is reported: the runner flags nodes off the screen as hidden, so only about
//! one screenful is listed while the whole tree still crosses the bridge and is digested.
//! `Dense` puts every node on the screen in a non-overlapping grid, so all of them are
//! listed: a stress case, not a real screen.
//!
//! The same [`GenSpec`] always gives the same tree (fixed seed, no clock, no hashing of
//! addresses).

use jarvis_proto::{Orientation, Rect, Role, Screen, Size, UiNode};

/// Screen width of the iPhone 17 Pro Max simulator the fixtures come from.
pub const SCREEN_W: f64 = 440.0;
/// Screen height of the same.
pub const SCREEN_H: f64 = 956.0;
/// Top of the content region (below the navigation bar).
const TOP: f64 = 106.0;
/// Bottom of the content region (top of the tab bar).
const BOTTOM: f64 = 850.0;
/// Depth of the wrapper spine of [`Shape::Deep`].
pub const DEEP_LEVELS: usize = 200;
/// Seed of every synthetic tree.
pub const DEFAULT_SEED: u64 = 0x4a41_5256_4953_0001;

/// What the content region holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shape {
    /// A table of cells.
    List,
    /// Items at the end of a 200 level spine.
    Deep,
    /// A web view of small texts and links.
    Web,
}

impl Shape {
    /// All shapes.
    pub const ALL: [Shape; 3] = [Shape::List, Shape::Deep, Shape::Web];

    /// Short name used in case ids and options.
    pub fn name(self) -> &'static str {
        match self {
            Shape::List => "list",
            Shape::Deep => "deep",
            Shape::Web => "web",
        }
    }

    /// Parses a name from [`Shape::name`].
    pub fn parse(s: &str) -> Option<Shape> {
        Shape::ALL.into_iter().find(|v| v.name() == s)
    }
}

/// How the content is laid out on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    /// A tall page: what is off the screen is hidden.
    Page,
    /// Everything on the screen, in a grid.
    Dense,
}

impl View {
    /// Both views.
    pub const ALL: [View; 2] = [View::Page, View::Dense];

    /// Short name used in case ids and options.
    pub fn name(self) -> &'static str {
        match self {
            View::Page => "page",
            View::Dense => "dense",
        }
    }

    /// Parses a name from [`View::name`].
    pub fn parse(s: &str) -> Option<View> {
        View::ALL.into_iter().find(|v| v.name() == s)
    }
}

/// What to generate.
#[derive(Debug, Clone, Copy)]
pub struct GenSpec {
    /// Content shape.
    pub shape: Shape,
    /// Layout.
    pub view: View,
    /// Wanted number of nodes; the tree has at least this many, and fewer than this plus
    /// one content unit more.
    pub nodes: usize,
    /// Seed for the texts.
    pub seed: u64,
}

/// SplitMix64: tiny, fixed, and the same on every platform.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n.max(1) as u64)).unwrap_or(0)
    }
}

const WORDS: [&str; 64] = [
    "morning",
    "summary",
    "album",
    "recent",
    "favorite",
    "calendar",
    "reminder",
    "travel",
    "recipe",
    "garden",
    "weather",
    "podcast",
    "episode",
    "notes",
    "archive",
    "library",
    "project",
    "update",
    "design",
    "review",
    "season",
    "market",
    "coffee",
    "concert",
    "journal",
    "fitness",
    "routine",
    "playlist",
    "article",
    "gallery",
    "profile",
    "schedule",
    "family",
    "weekend",
    "holiday",
    "camera",
    "sunrise",
    "mountain",
    "harbor",
    "village",
    "kitchen",
    "orchard",
    "network",
    "battery",
    "storage",
    "keyboard",
    "shortcut",
    "history",
    "discover",
    "popular",
    "nearby",
    "shared",
    "pinned",
    "folder",
    "reading",
    "listening",
    "watching",
    "learning",
    "walking",
    "cycling",
    "sailing",
    "painting",
    "baking",
    "studio",
];

/// Words joined to between `min` and `max` characters (ASCII, so cutting is safe), first
/// letter capitalised.
fn words(rng: &mut Rng, min: usize, max: usize) -> String {
    let mut s = String::new();
    while s.len() < min {
        if !s.is_empty() {
            s.push(' ');
        }
        s.push_str(WORDS.get(rng.below(WORDS.len())).copied().unwrap_or("item"));
    }
    s.truncate(max);
    while s.ends_with(' ') {
        s.pop();
    }
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => s,
    }
}

/// Where the content units go: one row of a page, or one cell of a grid on the screen.
struct Grid {
    view: View,
    row_h: f64,
    cols: usize,
    rows: usize,
}

impl Grid {
    fn new(view: View, n: usize, row_h: f64) -> Grid {
        match view {
            View::Page => Grid {
                view,
                row_h,
                cols: 1,
                rows: n.max(1),
            },
            View::Dense => {
                let area = (BOTTOM - TOP) / SCREEN_W;
                let cols = ((n.max(1) as f64 / area).sqrt().ceil() as usize).max(1);
                Grid {
                    view,
                    row_h,
                    cols,
                    rows: n.max(1).div_ceil(cols),
                }
            }
        }
    }

    fn slot(&self, i: usize) -> Rect {
        match self.view {
            View::Page => Rect::new(0.0, TOP + i as f64 * self.row_h, SCREEN_W, self.row_h),
            View::Dense => {
                let (w, h) = (
                    SCREEN_W / self.cols as f64,
                    (BOTTOM - TOP) / self.rows as f64,
                );
                Rect::new(
                    (i % self.cols) as f64 * w,
                    TOP + (i / self.cols) as f64 * h,
                    w,
                    h,
                )
            }
        }
    }
}

/// `slot`'s sub-rectangle by fractions of its width and height.
fn sub(slot: Rect, fx: f64, fy: f64, fw: f64, fh: f64) -> Rect {
    Rect::new(
        slot.x + slot.width * fx,
        slot.y + slot.height * fy,
        slot.width * fw,
        slot.height * fh,
    )
}

/// A node with the runner's visibility rule: shown when its frame meets the screen.
fn node(role: Role, frame: Rect, label: Option<String>) -> UiNode {
    let screen = Rect::new(0.0, 0.0, SCREEN_W, SCREEN_H);
    let visible = frame.intersects(&screen);
    let mut n = UiNode::new(role, frame).visible(visible).hittable(visible);
    n.label = label;
    n
}

fn other() -> Role {
    Role::Other("Other".to_owned())
}

fn text(frame: Rect, label: String) -> UiNode {
    node(Role::StaticText, frame, Some(label))
}

// ---- content units ------------------------------------------------------------------------

/// Nodes in list cell `i`.
fn list_cell_nodes(i: usize) -> usize {
    4 + usize::from(i.is_multiple_of(8)) + usize::from(i.is_multiple_of(16))
}

fn list_cell(i: usize, slot: Rect, rng: &mut Rng) -> UiNode {
    let mut cell = node(Role::Cell, slot, None);
    let icon = words(rng, 5, 12);
    let title = words(rng, 8, 28);
    let mut subtitle = words(rng, 30, 70);
    if i.is_multiple_of(25) {
        subtitle.push_str(&format!(" \u{b7} ${}.{:02}", 5 + i % 90, i % 100));
    }
    cell.children.push(node(
        Role::Image,
        sub(slot, 0.02, 0.15, 0.12, 0.7),
        Some(icon),
    ));
    cell.children
        .push(text(sub(slot, 0.16, 0.08, 0.45, 0.4), title));
    cell.children
        .push(text(sub(slot, 0.16, 0.5, 0.45, 0.4), subtitle));
    if i.is_multiple_of(8) {
        cell.children.push(node(
            Role::Button,
            sub(slot, 0.64, 0.2, 0.16, 0.6),
            Some("Details".to_owned()),
        ));
    }
    if i.is_multiple_of(16) {
        let sw = node(
            Role::Switch,
            sub(slot, 0.82, 0.2, 0.16, 0.6),
            Some("Notify".to_owned()),
        );
        cell.children
            .push(sw.with_value(if i.is_multiple_of(32) { "1" } else { "0" }));
    }
    cell
}

/// Nodes in web group `i`.
fn web_group_nodes(i: usize) -> usize {
    1 + usize::from(i.is_multiple_of(6))
        + 1
        + if i % 3 == 2 {
            0
        } else {
            1 + usize::from(i.is_multiple_of(2))
        }
        + usize::from(i.is_multiple_of(5))
}

fn web_group(i: usize, slot: Rect, rng: &mut Rng) -> UiNode {
    let mut g = node(other(), slot, None);
    if i.is_multiple_of(6) {
        g.children
            .push(text(sub(slot, 0.02, 0.0, 0.96, 0.2), words(rng, 20, 60)));
    }
    g.children
        .push(text(sub(slot, 0.02, 0.2, 0.96, 0.3), words(rng, 80, 220)));
    if !(i % 3 == 2) {
        let label = words(rng, 8, 35);
        let frame = sub(slot, 0.02, 0.5, 0.5, 0.25);
        let mut link = node(Role::Link, frame, Some(label.clone()));
        if i.is_multiple_of(2) {
            link.children.push(text(frame, label));
        }
        g.children.push(link);
    }
    if i.is_multiple_of(5) {
        g.children.push(node(
            Role::Image,
            sub(slot, 0.02, 0.75, 0.96, 0.25),
            Some(words(rng, 10, 40)),
        ));
    }
    g
}

/// Nodes in deep item `i`.
fn deep_item_nodes(i: usize) -> usize {
    2 + usize::from(i.is_multiple_of(4))
}

fn deep_item(i: usize, slot: Rect, rng: &mut Rng) -> UiNode {
    let label = words(rng, 10, 30);
    let mut b = node(Role::Button, slot, Some(label.clone()));
    b.children.push(text(sub(slot, 0.16, 0.1, 0.7, 0.8), label));
    if i.is_multiple_of(4) {
        b.children.push(node(
            Role::Image,
            sub(slot, 0.02, 0.15, 0.12, 0.7),
            Some(words(rng, 5, 12)),
        ));
    }
    b
}

// ---- the tree -----------------------------------------------------------------------------

/// Number of content units needed for `nodes` in total (the chrome and spine excluded).
fn units_for(shape: Shape, wanted: usize) -> usize {
    let chrome = chrome_nodes(shape);
    let per: fn(usize) -> usize = match shape {
        Shape::List => list_cell_nodes,
        Shape::Deep => deep_item_nodes,
        Shape::Web => web_group_nodes,
    };
    let (mut total, mut units) = (chrome, 0);
    while total < wanted {
        total += per(units);
        units += 1;
    }
    units.max(1)
}

/// Nodes outside the content units: application, window, two wrappers, navigation bar and
/// its three children, tab bar and its four tabs, the content container (and the spine).
fn chrome_nodes(shape: Shape) -> usize {
    let base = 1 + 1 + 2 + 1 + 3 + 1 + 4;
    match shape {
        Shape::List | Shape::Web => base + 1,
        Shape::Deep => base + DEEP_LEVELS,
    }
}

/// Generates the tree described by `spec`.
pub fn generate(spec: &GenSpec) -> Screen {
    let mut rng = Rng(spec.seed);
    let units = units_for(spec.shape, spec.nodes);
    let content_frame = Rect::new(0.0, TOP, SCREEN_W, BOTTOM - TOP);
    let content = match spec.shape {
        Shape::List => {
            let grid = Grid::new(spec.view, units, 64.0);
            let cells = (0..units)
                .map(|i| list_cell(i, grid.slot(i), &mut rng))
                .collect();
            node(Role::Table, content_frame, None).with_children(cells)
        }
        Shape::Web => {
            let grid = Grid::new(spec.view, units, 40.0);
            let groups = (0..units)
                .map(|i| web_group(i, grid.slot(i), &mut rng))
                .collect();
            node(Role::WebView, content_frame, Some("Page".to_owned())).with_children(groups)
        }
        Shape::Deep => {
            let grid = Grid::new(spec.view, units, 48.0);
            let mut levels: Vec<Vec<UiNode>> = (0..DEEP_LEVELS).map(|_| Vec::new()).collect();
            for i in 0..units {
                let level = i * DEEP_LEVELS / units;
                if let Some(l) = levels.get_mut(level) {
                    l.push(deep_item(i, grid.slot(i), &mut rng));
                }
            }
            // Bottom-up so that building needs no recursion.
            let mut inner: Option<UiNode> = None;
            for items in levels.into_iter().rev() {
                let mut wrapper = node(other(), content_frame, None);
                wrapper.children = items;
                wrapper.children.extend(inner.take());
                inner = Some(wrapper);
            }
            inner.unwrap_or_else(|| node(other(), content_frame, None))
        }
    };

    let full = Rect::new(0.0, 0.0, SCREEN_W, SCREEN_H);
    let nav_frame = Rect::new(0.0, 0.0, SCREEN_W, 100.0);
    let mut nav = node(Role::NavigationBar, nav_frame, None).with_identifier("Bench");
    nav.children.push(node(
        Role::Button,
        Rect::new(8.0, 56.0, 60.0, 36.0),
        Some("Back".to_owned()),
    ));
    nav.children
        .push(text(Rect::new(80.0, 56.0, 120.0, 36.0), "Bench".to_owned()));
    let mut search = node(
        Role::SearchField,
        Rect::new(210.0, 56.0, 220.0, 36.0),
        Some("Search".to_owned()),
    );
    search.placeholder = Some("Search".to_owned());
    nav.children.push(search);
    let mut tabs = node(
        Role::TabBar,
        Rect::new(0.0, BOTTOM, SCREEN_W, SCREEN_H - BOTTOM),
        None,
    );
    for (k, name) in ["Home", "Explore", "Library", "Profile"].iter().enumerate() {
        tabs.children.push(node(
            Role::Tab,
            Rect::new(k as f64 * 110.0, BOTTOM + 8.0, 110.0, 49.0),
            Some((*name).to_owned()),
        ));
    }
    let wrapper = node(other(), full, None).with_children(vec![nav, content, tabs]);
    let wrapper = node(other(), full, None).with_children(vec![wrapper]);
    let window = node(Role::Window, full, None).with_children(vec![wrapper]);
    let root = node(Role::Application, full, Some("Bench".to_owned())).with_children(vec![window]);
    Screen {
        app_bundle_id: "com.example.bench".to_owned(),
        app_name: Some("Bench".to_owned()),
        size: Size::new(SCREEN_W, SCREEN_H),
        root,
        keyboard_visible: false,
        orientation: Orientation::Portrait,
    }
}

/// Changes the navigation bar title to `Bench <variant>`, so that two variants of one
/// screen differ in the compact screen and the digest (steps need a visible change).
pub fn set_variant(screen: &mut Screen, variant: u32) {
    let mut at = &mut screen.root;
    // Application -> Window -> wrapper -> wrapper -> navigation bar -> title.
    for index in [0usize, 0, 0, 0] {
        let Some(next) = at.children.get_mut(index) else {
            return;
        };
        at = next;
    }
    if let Some(title) = at.children.get_mut(1) {
        title.label = Some(format!("Bench {variant}"));
    }
}

/// Changes the text of the first shown static text of any screen (a real fixture too), the
/// way a screen changes after a tap. Returns whether there was one.
pub fn perturb(screen: &mut Screen) -> bool {
    fn first_text(n: &mut UiNode) -> Option<&mut UiNode> {
        // Iterative: the tree may be deep.
        let mut stack: Vec<&mut UiNode> = vec![n];
        while let Some(node) = stack.pop() {
            if node.role == Role::StaticText && node.visible && node.label.is_some() {
                return Some(node);
            }
            stack.extend(node.children.iter_mut().rev());
        }
        None
    }
    match first_text(&mut screen.root) {
        Some(n) => {
            if let Some(l) = n.label.as_mut() {
                l.push_str(" (updated)");
            }
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(shape: Shape, view: View, nodes: usize) -> GenSpec {
        GenSpec {
            shape,
            view,
            nodes,
            seed: DEFAULT_SEED,
        }
    }

    #[test]
    fn trees_are_deterministic_and_close_to_the_wanted_size() {
        for shape in Shape::ALL {
            for view in View::ALL {
                let a = generate(&spec(shape, view, 1000));
                let b = generate(&spec(shape, view, 1000));
                assert_eq!(a, b, "{shape:?} {view:?}");
                let n = a.root.count();
                assert!((1000..1000 + 12).contains(&n), "{shape:?} {view:?}: {n}");
            }
        }
    }

    #[test]
    fn the_deep_shape_is_deeper_than_the_depth_cap() {
        fn depth(n: &UiNode) -> usize {
            1 + n.children.iter().map(depth).max().unwrap_or(0)
        }
        let s = generate(&spec(Shape::Deep, View::Page, 2000));
        assert!(depth(&s.root) > 200);
    }

    #[test]
    fn page_view_hides_what_is_off_screen_and_dense_shows_everything() {
        let page = generate(&spec(Shape::List, View::Page, 2000));
        let hidden = page.root.iter().filter(|n| !n.visible).count();
        assert!(hidden > 1500, "{hidden}");
        let dense = generate(&spec(Shape::List, View::Dense, 2000));
        assert!(dense.root.iter().all(|n| n.visible));
    }

    #[test]
    fn variants_differ() {
        let mut a = generate(&spec(Shape::List, View::Page, 300));
        let b = a.clone();
        set_variant(&mut a, 7);
        assert_ne!(a, b);
    }
}
