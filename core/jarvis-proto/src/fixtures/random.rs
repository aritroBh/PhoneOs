//! Seeded random compact screens for differential tests: the same seed always gives the same
//! screen. They are not realistic; they are varied: every role, labels that hit the phrase
//! tables, prices, symbol names, non-Latin and hostile text (quotes, control characters,
//! very long runs), values of every switch form, gaps and duplicates in indices and
//! fingerprints, empty, `NaN` and overlapping frames, rows of equal tiles beside each other,
//! and every screen cue.

use crate::compact::{CompactScreen, CoverKind, CoveredControl, Element, ScreenCues};
use crate::geometry::{Rect, Size};
use crate::ui::Role;

/// A tiny seeded generator (xorshift64).
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    /// A generator for `seed` (any value, zero included).
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    /// The next raw value.
    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A value in `0..n` (`0` when `n` is zero).
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n.max(1)
    }

    /// True `pct` times in a hundred.
    pub fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }

    /// A value in `lo..hi`, on a grid of ten thousand steps.
    pub fn f(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * (self.below(10_000) as f64 / 10_000.0)
    }

    /// One of `xs` (which must not be empty).
    #[allow(clippy::indexing_slicing)]
    pub fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        let i = usize::try_from(self.below(xs.len() as u64)).unwrap_or(0);
        &xs[i]
    }
}

const APPS: &[&str] = &[
    "com.example.shop",
    "com.apple.Preferences",
    "com.apple.MobileSMS",
    "com.apple.springboard",
    "com.apple.mobilesafari",
    "com.apple.mobilephone",
    "com.apple.Home",
    "com.apple.mobilemail",
    "COM.EXAMPLE.SHOP",
];

const WORDS: &[&str] = &[
    "Delete",
    "Buy",
    "Pay",
    "Pay now",
    "$4.99",
    "€12,50",
    "USD 5",
    "Total",
    "Card ending 1234",
    "Send",
    "OK",
    "Continue",
    "Done",
    "AirDrop",
    "Air Drop",
    "Save",
    "Account",
    "Password",
    "Settings",
    "Get",
    "Book",
    "Order",
    "Slide to pay",
    "Swipe up",
    "trash",
    "chevron.right",
    "square.and.arrow.up",
    "xmark",
    "Back",
    "Forward",
    "Wi-Fi",
    "Bluetooth",
    "Cart (0)",
    "Cart (1)",
    "Message",
    "Search",
    "Call",
    "+1 (555) 010-2030",
    "José",
    "Café Central",
    "Привет",
    "日本語のテキスト",
    "🙂 emoji",
    "say \"hi\"",
    "back\\slash",
    "line\nbreak",
    "tab\there",
    "bell\u{7}",
    "Erase All Content and Settings",
    "Desk Lamp, $35.50",
    "In-App Purchases",
    "Subscription",
    "Type DELETE to confirm",
    "9:41",
    "Item",
    "",
    " ",
];

const ROLES: &[Role] = &[
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
    Role::Key,
    Role::ScrollView,
    Role::Table,
];

const OTHER_ROLES: &[&str] = &[
    "Toggle",
    "Stepper",
    "menu",
    "Slider",
    "Other",
    "pickerWheel",
];

const VALUES: &[&str] = &[
    "1", "0", "on", "Off", "true", "FALSE", " 1 ", "", "50%", "hello", "$9.99",
];

/// Random text: one to five pieces of [`WORDS`], sometimes a long run.
pub fn random_text(r: &mut Rng) -> String {
    if r.chance(3) {
        let c = *r.pick(&['x', 'é', '字', '"', ' ']);
        let n = usize::try_from(r.below(400)).unwrap_or(0);
        return std::iter::repeat_n(c, n).collect();
    }
    let most = if r.chance(70) { 2 } else { 5 };
    let n = 1 + r.below(most);
    let mut out = String::new();
    for k in 0..n {
        if k > 0 {
            out.push_str(if r.chance(20) { ", " } else { " " });
        }
        out.push_str(r.pick(WORDS));
    }
    out
}

fn random_role(r: &mut Rng) -> Role {
    if r.chance(8) {
        Role::Other((*r.pick(OTHER_ROLES)).to_owned())
    } else {
        r.pick(ROLES).clone()
    }
}

/// A random element with index `index` on a screen of `size`.
pub fn random_element(r: &mut Rng, index: u32, size: Size) -> Element {
    let role = random_role(r);
    let label = if r.chance(15) {
        String::new()
    } else {
        random_text(r)
    };
    let value = match r.below(4) {
        0 => Some((*r.pick(VALUES)).to_owned()),
        1 if r.chance(50) => Some(random_text(r)),
        _ => None,
    };
    let placeholder = r.chance(15).then(|| random_text(r));
    let frame = match r.below(20) {
        0 => Rect::new(r.f(0.0, size.width), r.f(0.0, size.height), 0.0, 10.0),
        1 if r.chance(10) => Rect::new(f64::NAN, 10.0, 20.0, 20.0),
        _ => Rect::new(
            r.f(-20.0, size.width),
            r.f(-20.0, size.height),
            r.f(1.0, size.width),
            r.f(1.0, 300.0),
        ),
    };
    Element {
        index,
        role,
        label,
        value,
        placeholder,
        frame,
        enabled: !r.chance(10),
        focused: r.chance(5),
        selected: r.chance(10),
        secure: r.chance(5),
        fingerprint: if r.chance(20) {
            r.below(8)
        } else {
            r.next_u64()
        },
    }
}

/// A random compact screen for `seed`: usually a few dozen elements, sometimes none and
/// sometimes a few hundred, partly laid out as rows of equal tiles.
pub fn random_compact_screen(seed: u64) -> CompactScreen {
    let mut r = Rng::new(seed);
    let size = match r.below(10) {
        0 => Size::new(852.0, 393.0),
        1 => Size::new(0.0, 0.0),
        _ => Size::new(393.0, 852.0),
    };
    let n = match r.below(10) {
        0 => r.below(4),
        1 => 60 + r.below(340),
        _ => 4 + r.below(56),
    };
    let mut elements = Vec::new();
    let mut index = 0u32;
    while (elements.len() as u64) < n {
        index += if r.chance(5) { 2 } else { 1 };
        if r.chance(2) {
            index = index.saturating_sub(1).max(1);
        }
        if r.chance(10) {
            // A row of equal tiles side by side (a carousel or grid), tops nearly aligned.
            let cols = 2 + r.below(6);
            let y = r.f(0.0, size.height.max(1.0));
            let (w, h) = (r.f(20.0, 120.0), r.f(20.0, 200.0));
            let role = if r.chance(50) {
                Role::Cell
            } else {
                Role::Button
            };
            for c in 0..cols {
                let mut e = random_element(&mut r, index, size);
                e.role = role.clone();
                e.frame = Rect::new(c as f64 * (w + r.f(0.0, 4.0)), y + r.f(0.0, 3.0), w, h);
                elements.push(e);
                index += 1;
            }
            continue;
        }
        elements.push(random_element(&mut r, index, size));
    }
    // Covered controls: ascending, unique indices (some screens, a few controls each).
    // (Its own generator: the other fields' draws stay what they were.)
    let mut covered: Vec<CoveredControl> = Vec::new();
    let mut c = Rng::new(seed ^ 0xC0E7);
    if c.chance(40) {
        for e in &elements {
            if c.chance(10) && covered.last().is_none_or(|l| l.index < e.index) {
                covered.push(CoveredControl {
                    index: e.index,
                    by: *c.pick(&[
                        CoverKind::NavigationBar,
                        CoverKind::TabBar,
                        CoverKind::Toolbar,
                        CoverKind::Keyboard,
                        CoverKind::Other,
                    ]),
                });
            }
        }
    }
    CompactScreen {
        app_bundle_id: (*r.pick(APPS)).to_owned(),
        app_name: match r.below(4) {
            0 => None,
            1 => Some(String::new()),
            _ => Some(random_text(&mut r)),
        },
        size,
        keyboard_visible: r.chance(30),
        elements,
        cues: ScreenCues {
            more_above: r.chance(20),
            more_below: r.chance(30),
            page: r.chance(15).then(|| {
                let count = 1 + u32::try_from(r.below(4)).unwrap_or(0);
                (
                    1 + u32::try_from(r.below(u64::from(count))).unwrap_or(0),
                    count,
                )
            }),
            return_key: r.chance(30).then(|| {
                (*r.pick(&["return", "send", "go", "search", "done", "Return"])).to_owned()
            }),
            // Drawn last: the rest of the screen is what it was before the type existed.
            return_key_type: r
                .chance(30)
                .then(|| (*r.pick(&["return", "send", "go", "search"])).to_owned()),
            covered,
            unplaced: Vec::new(),
        },
    }
}

/// The screen after a random step on `before`: some values, states and labels change
/// (a relabel gets a new fingerprint), some elements go, some come, sometimes the app or
/// the title changes.
pub fn random_next_screen(before: &CompactScreen, seed: u64) -> CompactScreen {
    let mut r = Rng::new(seed ^ 0x5EED);
    let mut after = before.clone();
    let mut elements = Vec::new();
    for e in &before.elements {
        if r.chance(8) {
            continue;
        }
        let mut e = e.clone();
        match r.below(10) {
            0 => e.value = Some((*r.pick(VALUES)).to_owned()),
            1 => e.selected = !e.selected,
            2 => e.enabled = !e.enabled,
            3 => {
                e.label = random_text(&mut r);
                e.fingerprint = r.next_u64();
            }
            4 if r.chance(30) => e.value = None,
            _ => {}
        }
        elements.push(e);
        if r.chance(5) {
            let index = elements.len() as u32 + 1;
            elements.push(random_element(&mut r, index, before.size));
        }
    }
    if r.chance(10) {
        elements.reverse();
    }
    after.elements = elements;
    if r.chance(5) {
        after.app_bundle_id = (*r.pick(APPS)).to_owned();
    }
    after
}
