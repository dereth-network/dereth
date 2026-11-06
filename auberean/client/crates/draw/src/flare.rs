//! Lens flares: the rings and glints a sky object (the sun) throws across the screen while it is
//! in view.
//!
//! A sky object's flare is a list of points, each a picture of its own size standing on the line
//! from the object's place on the screen through the screen's middle. The flare fades in while
//! the object was seen the frame before (nothing drawn over its place on the screen) and out while
//! it was not, at its own speed; it shows more strongly the nearer the object stands to the
//! screen's middle. The object's place is reckoned 1000 metres out along its direction from the
//! origin of the frame's coordinates, not from the viewer.

use auberean_assets::region::{DayDesc, LensFlare};
use auberean_primitives::Vec3;
use auberean_world::math::{length, scale};
use auberean_world::sky::sky_pieces;

/// How far out along its direction a flare's object is placed, metres.
pub const FLARE_DISTANCE: f32 = 1000.0;

/// A point is kept only when it has a picture and a size above this.
const SMALLEST_POINT: f32 = 0.0002;

/// One point of a flare: where it stands on the line through the screen's middle (its place is
/// the object's place on the screen times minus this), its picture, and its width in the screen's
/// coordinates (which run from -1 to 1 across and up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlarePoint {
    pub axis: f32,
    pub texture: u32,
    pub size: f32,
}

/// A sky object's flare in a frame: which object (its place in the day's list), where it is
/// reckoned to stand (in the frame's coordinates), how fast the flare fades in and out (its share
/// a second), and its points.
#[derive(Clone, Debug, PartialEq)]
pub struct SkyFlare {
    pub key: usize,
    pub at: Vec3,
    pub fade_speed: f32,
    pub points: Vec<FlarePoint>,
}

/// A stored flare's points: those with a picture and a size.
// D2-10372, D2-10359
#[must_use]
pub fn flare_points(flare: &LensFlare) -> Vec<FlarePoint> {
    flare
        .points
        .iter()
        .filter(|p| p.size > SMALLEST_POINT && !p.texture.is_none())
        .map(|p| FlarePoint {
            axis: p.axis_position,
            texture: p.texture.0,
            size: p.size,
        })
        .collect()
}

/// The flares of the sky objects showing in `day` at time of day `t`: each object that shows and
/// has points, placed 1000 metres out along the way it faces away from (its own -y).
// D2-10940, D2-05962
#[must_use]
pub fn sky_flares(day: &DayDesc, t: f32) -> Vec<SkyFlare> {
    let mut out = Vec::new();
    for piece in sky_pieces(day, t) {
        let Some(object) = day.objects.get(piece.index) else {
            continue;
        };
        let points = flare_points(&object.flare);
        if points.is_empty() {
            continue;
        }
        let r = piece.rotation;
        let axis = auberean_world::math::Mat4::from_parts(Vec3::ZERO, r, Vec3::new(1.0, 1.0, 1.0))
            .direction(Vec3::new(0.0, -1.0, 0.0));
        let len = length(axis);
        if len <= 0.0 {
            continue;
        }
        out.push(SkyFlare {
            key: piece.index,
            at: scale(axis, FLARE_DISTANCE / len),
            fade_speed: object.flare.fade_speed,
            points,
        });
    }
    out
}

/// The triangle that asks whether a sky object is seen, on a picture `width` by `height` from the
/// object's place `at` on the screen (-1 to 1 each way): it covers the middle of that one pixel
/// and no other's, so anything drawn over that pixel hides the object.
// D2-10370
#[must_use]
pub fn seen_marker(at: (f32, f32), width: u32, height: u32) -> [(f32, f32); 3] {
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (width.max(1) as f32, height.max(1) as f32);
    let middle = |v: f32, n: f32| ((v + 1.0) * n / 2.0).floor().mul_add(1.0, 0.5) * 2.0 / n - 1.0;
    let (x, y) = (middle(at.0, w), middle(at.1, h));
    // Reaching past the pixel's middle by less than a pixel each way.
    let (dx, dy) = (2.0 / w * 0.9, 2.0 / h * 0.9);
    [(x, y + dy), (x - dx, y - dy / 2.0), (x + dx, y - dy / 2.0)]
}

/// How strongly a flare shows, kept from frame to frame: 0 to 1, and when it was last drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlareFade {
    pub level: f32,
    pub last: Option<f64>,
}

impl FlareFade {
    /// The level at time `now` (seconds): raised by the time since it was last drawn times
    /// `speed` when its object was seen the frame before, else lowered so, and held to 0..1.
    // D2-10370
    pub fn advance(&mut self, now: f64, speed: f32, seen: bool) -> f32 {
        #[allow(clippy::cast_possible_truncation)] // a frame's span
        let dt = self.last.map_or(0.0, |t| (now - t) as f32);
        self.last = Some(now);
        let step = dt * speed;
        self.level = if seen {
            self.level + step
        } else {
            self.level - step
        }
        .clamp(0.0, 1.0);
        self.level
    }
}

/// One drawn point of a flare: its rectangle in the screen's coordinates (left, bottom, right,
/// top), its picture and how opaque it is (it adds itself onto what is drawn).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlareQuad {
    pub rect: [f32; 4],
    pub texture: u32,
    pub alpha: f32,
}

/// The points of a flare whose object stands at `at` on the screen (-1 to 1 across and up), at
/// fade `level`, on a display `aspect` times as wide as it is tall: each centered at minus its
/// axis times `at`, its height in the screen's coordinates its width times the aspect (square on
/// such a display); every one as opaque as the square of how near the middle `at` stands (one less
/// the distance over 1.42), at least 0.2, times the level, held to 0..1. None at level 0.
// D2-10370
#[must_use]
pub fn flare_quads(
    points: &[FlarePoint],
    at: (f32, f32),
    aspect: f32,
    level: f32,
) -> Vec<FlareQuad> {
    if level <= 0.0 {
        return Vec::new();
    }
    let near = 1.0 - (at.0 * at.0 + at.1 * at.1).sqrt() * 0.704_225_36;
    let alpha = ((near * near).max(0.2) * level).clamp(0.0, 1.0);
    points
        .iter()
        .map(|p| {
            let (cx, cy) = (at.0 * -p.axis, at.1 * -p.axis);
            let (w, h) = (p.size * 0.5, p.size * 0.5 * aspect);
            FlareQuad {
                rect: [cx - w, cy - h, cx + w, cy + h],
                texture: p.texture,
                alpha,
            }
        })
        .collect()
}
