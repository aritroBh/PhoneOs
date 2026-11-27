//! Plain geometry for the Live view, with stable JSON keys. Every function is total: NaN,
//! infinity, zero and negative sizes give a usable answer.
//!
//! `min`/`max` follow the Swift standard library's definitions (`y < x ? y : x` and
//! `y >= x ? y : x`), not `f64::min`/`f64::max`, so a NaN behaves exactly as it did in the
//! Swift implementation this was ported from.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Swift's `min(x, y)`.
#[inline]
pub(crate) fn smin(x: f64, y: f64) -> f64 {
    if y < x {
        y
    } else {
        x
    }
}

/// Swift's `max(x, y)`.
#[inline]
pub(crate) fn smax(x: f64, y: f64) -> f64 {
    if y >= x {
        y
    } else {
        x
    }
}

/// Largest coordinate magnitude written on the wire.
pub const WIRE_COORDINATE_LIMIT: f64 = 1_000_000.0;
/// Wire coordinates are multiples of 1/`WIRE_COORDINATE_STEPS` of a point.
pub const WIRE_COORDINATE_STEPS: f64 = 256.0;

/// `v` as written on the wire: clamped to ±[`WIRE_COORDINATE_LIMIT`] and rounded to
/// 1/[`WIRE_COORDINATE_STEPS`] of a point. Such a value prints with at most 15 significant
/// digits, so every JSON reader (also serde_json without its `float_roundtrip` feature)
/// reads back exactly the same double, and encode → decode → encode is stable.
pub fn wire_coordinate(v: f64) -> f64 {
    let c = v.clamp(-WIRE_COORDINATE_LIMIT, WIRE_COORDINATE_LIMIT);
    (c * WIRE_COORDINATE_STEPS).round() / WIRE_COORDINATE_STEPS
}

/// A coordinate on the wire: finite numbers as JSON numbers ([`wire_coordinate`]), a
/// non-finite one as `null` (which reads back as NaN), so a command's point is kept as sent
/// and still round-trips.
mod lenient_f64 {
    use super::*;

    pub fn serialize<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
        if v.is_finite() {
            s.serialize_f64(wire_coordinate(*v))
        } else {
            s.serialize_none()
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        Ok(Option::<f64>::deserialize(d)?.unwrap_or(f64::NAN))
    }
}

/// A point in screen points (or view points, after [`ViewerMapping`]).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ViewerPoint {
    /// Horizontal coordinate.
    #[serde(with = "lenient_f64")]
    pub x: f64,
    /// Vertical coordinate.
    #[serde(with = "lenient_f64")]
    pub y: f64,
}

impl ViewerPoint {
    /// The origin.
    pub const ZERO: ViewerPoint = ViewerPoint { x: 0.0, y: 0.0 };

    /// Creates a point.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Both coordinates are finite.
    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    /// Distance to `other` (0 when either point is not finite).
    pub fn distance(&self, other: ViewerPoint) -> f64 {
        if !self.is_finite() || !other.is_finite() {
            return 0.0;
        }
        let d = (other.x - self.x) * (other.x - self.x) + (other.y - self.y) * (other.y - self.y);
        if d.is_finite() {
            d.sqrt()
        } else {
            0.0
        }
    }

    /// The point `fraction` of the way to `other` (`fraction` clamped to 0…1).
    pub fn interpolated(&self, other: ViewerPoint, fraction: f64) -> ViewerPoint {
        let f = if fraction.is_finite() {
            smin(smax(fraction, 0.0), 1.0)
        } else {
            0.0
        };
        if f >= 1.0 {
            return other;
        }
        if f <= 0.0 {
            return *self;
        }
        ViewerPoint::new(
            self.x + (other.x - self.x) * f,
            self.y + (other.y - self.y) * f,
        )
    }
}

/// A width and height.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ViewerSize {
    /// Width.
    #[serde(with = "lenient_f64")]
    pub width: f64,
    /// Height.
    #[serde(with = "lenient_f64")]
    pub height: f64,
}

impl ViewerSize {
    /// The size assumed when a screen reports none it can be drawn with (an iPhone 16/17
    /// in portrait).
    pub const FALLBACK_PHONE: ViewerSize = ViewerSize {
        width: 393.0,
        height: 852.0,
    };

    /// Creates a size.
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    /// Both sides finite and positive.
    pub fn is_usable(&self) -> bool {
        self.width.is_finite() && self.height.is_finite() && self.width > 0.0 && self.height > 0.0
    }

    /// Landscape (wider than tall).
    pub fn is_landscape(&self) -> bool {
        self.is_usable() && self.width > self.height
    }

    /// This size when usable, otherwise [`ViewerSize::FALLBACK_PHONE`].
    pub fn usable(&self) -> ViewerSize {
        if self.is_usable() {
            *self
        } else {
            Self::FALLBACK_PHONE
        }
    }

    /// Width / height of `usable`, clamped to 0.3…3 so an absurd report cannot make the
    /// viewer a sliver.
    pub fn display_aspect(&self) -> f64 {
        let s = self.usable();
        smin(smax(s.width / s.height, 0.3), 3.0)
    }

    /// The centre of `usable`.
    pub fn center(&self) -> ViewerPoint {
        let s = self.usable();
        ViewerPoint::new(s.width / 2.0, s.height / 2.0)
    }

    /// `point` moved inside `0…width × 0…height` of `usable` (a non-finite coordinate
    /// becomes the centre's).
    pub fn clamp(&self, point: ViewerPoint) -> ViewerPoint {
        let s = self.usable();
        let x = if point.x.is_finite() {
            smin(smax(point.x, 0.0), s.width)
        } else {
            s.width / 2.0
        };
        let y = if point.y.is_finite() {
            smin(smax(point.y, 0.0), s.height)
        } else {
            s.height / 2.0
        };
        ViewerPoint::new(x, y)
    }
}

/// An axis-aligned rectangle. Containment is half-open, like the core's `Rect`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ViewerRect {
    /// Left edge.
    #[serde(with = "lenient_f64")]
    pub x: f64,
    /// Top edge.
    #[serde(with = "lenient_f64")]
    pub y: f64,
    /// Width.
    #[serde(with = "lenient_f64")]
    pub width: f64,
    /// Height.
    #[serde(with = "lenient_f64")]
    pub height: f64,
}

impl ViewerRect {
    /// The empty rectangle at the origin.
    pub const ZERO: ViewerRect = ViewerRect {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };

    /// Creates a rectangle.
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Every component is finite.
    pub fn is_finite(&self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
    }

    /// Finite with a positive width and height.
    pub fn has_area(&self) -> bool {
        self.is_finite() && self.width > 0.0 && self.height > 0.0
    }

    /// Right edge.
    pub fn max_x(&self) -> f64 {
        self.x + self.width
    }

    /// Bottom edge.
    pub fn max_y(&self) -> f64 {
        self.y + self.height
    }

    /// The centre.
    pub fn center(&self) -> ViewerPoint {
        ViewerPoint::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Half-open containment; an empty or non-finite rectangle contains nothing.
    pub fn contains(&self, p: ViewerPoint) -> bool {
        self.has_area()
            && p.is_finite()
            && p.x >= self.x
            && p.x < self.max_x()
            && p.y >= self.y
            && p.y < self.max_y()
    }

    /// Where content of `size` lands when fitted into this rectangle keeping its aspect
    /// ratio, centred (letterboxed, never stretched or cropped). Content without a usable
    /// size, or a rectangle without area, gives this rectangle.
    pub fn aspect_fit(&self, size: ViewerSize) -> ViewerRect {
        if !self.has_area() || !size.is_usable() {
            return *self;
        }
        let scale = smin(self.width / size.width, self.height / size.height);
        if !(scale.is_finite() && scale > 0.0) {
            return *self;
        }
        let w = size.width * scale;
        let h = size.height * scale;
        ViewerRect::new(
            self.x + (self.width - w) / 2.0,
            self.y + (self.height - h) / 2.0,
            w,
            h,
        )
    }

    /// The overlap with `other`, `None` when they do not overlap with a positive area.
    pub fn intersection(&self, other: &ViewerRect) -> Option<ViewerRect> {
        if !self.has_area() || !other.has_area() {
            return None;
        }
        let left = smax(self.x, other.x);
        let top = smax(self.y, other.y);
        let right = smin(self.max_x(), other.max_x());
        let bottom = smin(self.max_y(), other.max_y());
        if right > left && bottom > top {
            Some(ViewerRect::new(left, top, right - left, bottom - top))
        } else {
            None
        }
    }
}

impl From<jarvis_proto::Rect> for ViewerRect {
    fn from(r: jarvis_proto::Rect) -> Self {
        ViewerRect::new(r.x, r.y, r.width, r.height)
    }
}

impl From<jarvis_proto::Size> for ViewerSize {
    fn from(s: jarvis_proto::Size) -> Self {
        ViewerSize::new(s.width, s.height)
    }
}

impl From<jarvis_proto::Point> for ViewerPoint {
    fn from(p: jarvis_proto::Point) -> Self {
        ViewerPoint::new(p.x, p.y)
    }
}

/// Maps screen points into a view of a given size: the screen is aspect-fitted and centred
/// (letterboxed on the long sides).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewerMapping {
    /// The screen size used (`usable` of the one given).
    pub screen: ViewerSize,
    /// The view size given (negative or non-finite sides count as 0).
    pub view: ViewerSize,
    /// View points per screen point (0 when the view has no area).
    pub scale: f64,
    /// Where the screen's top-left corner lands in the view.
    pub origin: ViewerPoint,
}

impl ViewerMapping {
    /// The mapping of `screen` into `view`.
    pub fn new(screen: ViewerSize, view: ViewerSize) -> Self {
        let s = screen.usable();
        let vw = if view.width.is_finite() {
            smax(view.width, 0.0)
        } else {
            0.0
        };
        let vh = if view.height.is_finite() {
            smax(view.height, 0.0)
        } else {
            0.0
        };
        let raw = smin(vw / s.width, vh / s.height);
        let scale = if raw.is_finite() { smax(raw, 0.0) } else { 0.0 };
        ViewerMapping {
            screen: s,
            view: ViewerSize::new(vw, vh),
            scale,
            origin: ViewerPoint::new((vw - s.width * scale) / 2.0, (vh - s.height * scale) / 2.0),
        }
    }

    /// The area of the view the screen occupies.
    pub fn content_rect(&self) -> ViewerRect {
        ViewerRect::new(
            self.origin.x,
            self.origin.y,
            self.screen.width * self.scale,
            self.screen.height * self.scale,
        )
    }

    /// A screen point in view coordinates (clamped to the screen first, so a point outside
    /// it lands on its edge, never in the letterbox).
    pub fn to_view(&self, p: ViewerPoint) -> ViewerPoint {
        let c = self.screen.clamp(p);
        ViewerPoint::new(
            self.origin.x + c.x * self.scale,
            self.origin.y + c.y * self.scale,
        )
    }

    /// A screen rectangle in view coordinates, clipped to the screen; `None` when nothing
    /// of it is on the screen.
    pub fn rect_to_view(&self, r: &ViewerRect) -> Option<ViewerRect> {
        let clipped = r.intersection(&ViewerRect::new(
            0.0,
            0.0,
            self.screen.width,
            self.screen.height,
        ))?;
        Some(ViewerRect::new(
            self.origin.x + clipped.x * self.scale,
            self.origin.y + clipped.y * self.scale,
            clipped.width * self.scale,
            clipped.height * self.scale,
        ))
    }

    /// The screen point under a view point, `None` outside the content or when the view has
    /// no area.
    pub fn to_screen(&self, v: ViewerPoint) -> Option<ViewerPoint> {
        if self.scale.is_nan() || self.scale <= 0.0 || !v.is_finite() {
            return None;
        }
        let p = ViewerPoint::new(
            (v.x - self.origin.x) / self.scale,
            (v.y - self.origin.y) / self.scale,
        );
        if p.x >= 0.0 && p.y >= 0.0 && p.x <= self.screen.width && p.y <= self.screen.height {
            Some(p)
        } else {
            None
        }
    }
}
