//! Screen geometry in points (`f64`), origin at the top-left, y growing downwards.

use serde::{Deserialize, Serialize};

/// A point on screen, in screen points.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: f64,
    /// Vertical coordinate.
    pub y: f64,
}

impl Point {
    /// Creates a point.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// A width/height pair, in screen points.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Size {
    /// Horizontal extent.
    pub width: f64,
    /// Vertical extent.
    pub height: f64,
}

impl Size {
    /// Creates a size.
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    /// The rectangle of this size anchored at the origin.
    pub const fn to_rect(self) -> Rect {
        Rect::new(0.0, 0.0, self.width, self.height)
    }
}

/// An axis-aligned rectangle in screen points.
///
/// Containment is half-open: the left/top edges are inside, the right/bottom edges are not,
/// so adjacent rectangles never both contain a point on their shared edge. A rectangle with
/// non-positive width or height is empty: it contains nothing and intersects nothing.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

impl Rect {
    /// Creates a rectangle from origin and size.
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The top-left corner.
    pub const fn origin(&self) -> Point {
        Point::new(self.x, self.y)
    }

    /// The width and height.
    pub const fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    /// The right edge (`x + width`).
    pub fn max_x(&self) -> f64 {
        self.x + self.width
    }

    /// The bottom edge (`y + height`).
    pub fn max_y(&self) -> f64 {
        self.y + self.height
    }

    /// The centre point, which is where taps on an element land.
    pub fn center(&self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Area in square points; zero for empty rectangles.
    pub fn area(&self) -> f64 {
        if self.is_empty() {
            0.0
        } else {
            self.width * self.height
        }
    }

    /// True when width or height is not positive (or not a number).
    pub fn is_empty(&self) -> bool {
        !(self.width > 0.0 && self.height > 0.0)
    }

    /// True when `p` lies inside (half-open on the right and bottom edges).
    pub fn contains(&self, p: Point) -> bool {
        !self.is_empty()
            && p.x >= self.x
            && p.x < self.max_x()
            && p.y >= self.y
            && p.y < self.max_y()
    }

    /// True when the two rectangles overlap with positive area.
    pub fn intersects(&self, other: &Rect) -> bool {
        self.intersection(other).is_some()
    }

    /// The overlapping region, if it has positive area.
    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        if self.is_empty() || other.is_empty() {
            return None;
        }
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let r = Rect::new(
            x,
            y,
            self.max_x().min(other.max_x()) - x,
            self.max_y().min(other.max_y()) - y,
        );
        if r.is_empty() {
            None
        } else {
            Some(r)
        }
    }
}
