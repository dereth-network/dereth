//! The landscape's shadows: the sun's map of the near objects' shadows, projected onto the
//! detailed terrain, and the clouds' shadows scrolling over all of it.
//!
//! The shadow map is a picture seen from the sun. It starts white; every object of the detailed
//! blocks is drawn into it black, through its first texture's alpha; then the whole picture is
//! lifted by one less the day's shadow strength, so a shadow darkens the ground by that strength
//! and no more. The terrain multiplies its color by the map where its points fall.

use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::math::{cross, dot, scale, sub, Mat4};

use crate::scene::ObjectBatch;

/// The shadow map's size, texels on a side.
pub const SHADOW_MAP_SIZE: u32 = 512;

/// How far the map's depth reaches past the terrain, before and behind, in meters.
const DEPTH_MARGIN: f32 = 1000.0;

/// The sun's frame: its right, the direction its light travels, and its up, each unit length.
/// Up is the world's up with its share along the light taken out; when the light falls straight
/// down, the world's north serves instead. `None` for no direction.
// D2-13286
#[must_use]
pub fn light_frame(direction: Vec3) -> Option<[Vec3; 3]> {
    let len = dot(direction, direction).sqrt();
    if len < 0.0002 {
        return None;
    }
    let along = scale(direction, 1.0 / len);
    let unit = |v: Vec3| {
        let l = dot(v, v).sqrt();
        (l >= 0.0002).then(|| scale(v, 1.0 / l))
    };
    let up = unit(sub(Vec3::new(0.0, 0.0, 1.0), scale(along, along.z)))
        .or_else(|| unit(sub(Vec3::new(0.0, 1.0, 0.0), scale(along, along.y))))?;
    let right = cross(along, up);
    Some([right, along, up])
}

/// The sun's shadow map of a frame: where it looks from, what it covers, how dark a shadow is,
/// and the objects that cast one.
#[derive(Clone, Debug)]
pub struct ShadowMap {
    /// World to the sun's frame: a point's distance along its right, along the light, and along
    /// its up (row-vector convention).
    pub light: Mat4,
    /// The detailed terrain's extent in the sun's frame: least and greatest right, along and up.
    pub min: Vec3,
    pub max: Vec3,
    /// How much a shadow takes away, per channel: the day's shadow color times 0.95, held to
    /// 0..1. The map is lifted by one less this after the casters are drawn.
    pub strength: Color,
    /// Every object of the detailed blocks, each fragment with its material's first layer, at
    /// full detail.
    pub casters: Vec<ObjectBatch>,
}

impl ShadowMap {
    /// The map of the terrain points `ground` (the detailed blocks' vertices) with the sun's
    /// light travelling along `direction`; `None` when the light has no direction.
    // D2-10929, D2-10609
    #[must_use]
    pub fn new(
        direction: Vec3,
        ground: impl IntoIterator<Item = Vec3>,
        shadow_color: Color,
        casters: Vec<ObjectBatch>,
    ) -> Option<Self> {
        let [right, along, up] = light_frame(direction)?;
        let light = Mat4([
            [right.x, along.x, up.x, 0.0],
            [right.y, along.y, up.y, 0.0],
            [right.z, along.z, up.z, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]);
        let mut min = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
        let mut max = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut any = false;
        for p in ground {
            let q = light.point(p);
            min = Vec3::new(min.x.min(q.x), min.y.min(q.y), min.z.min(q.z));
            max = Vec3::new(max.x.max(q.x), max.y.max(q.y), max.z.max(q.z));
            any = true;
        }
        if !any || max.x <= min.x || max.z <= min.z {
            return None;
        }
        let s = |c: f32| (c * 0.95).clamp(0.0, 1.0);
        Some(Self {
            light,
            min,
            max,
            strength: Color::new(s(shadow_color.r), s(shadow_color.g), s(shadow_color.b), 1.0),
            casters,
        })
    }

    /// World to the map's clip space, as the map is drawn: right across, up up, depth along the
    /// light from the terrain's nearest point less the margin to its farthest plus the margin.
    // D2-10575
    #[must_use]
    pub fn view_projection(&self) -> Mat4 {
        let (dx, dz) = (self.max.x - self.min.x, self.max.z - self.min.z);
        let (near, far) = (self.min.y - DEPTH_MARGIN, self.max.y + DEPTH_MARGIN);
        let dd = far - near;
        let proj = Mat4([
            [2.0 / dx, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0 / dd, 0.0],
            [0.0, 2.0 / dz, 0.0, 0.0],
            [
                -(self.max.x + self.min.x) / dx,
                -(self.max.z + self.min.z) / dz,
                -near / dd,
                1.0,
            ],
        ]);
        self.light.then(&proj)
    }

    /// World to the map's picture coordinates: `u` across from the least right, `v` down from
    /// the greatest up (the map's first row is its top).
    #[must_use]
    pub fn texture_coordinates(&self) -> Mat4 {
        let (dx, dz) = (self.max.x - self.min.x, self.max.z - self.min.z);
        let to_uv = Mat4([
            [1.0 / dx, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.0, -1.0 / dz, 0.0, 0.0],
            [-self.min.x / dx, 1.0 + self.min.z / dz, 0.0, 1.0],
        ]);
        self.light.then(&to_uv)
    }

    /// Where world point `p` falls in the map, as picture coordinates.
    #[must_use]
    pub fn uv(&self, p: Vec3) -> [f32; 2] {
        let q = self.texture_coordinates().point(p);
        [q.x, q.y]
    }
}

/// The clouds' shadows over the landscape: a picture the ground is multiplied by, laid flat at
/// one repeat every `1 / scale` meters and drifting with the client's running time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudShadows {
    /// The picture (a picture list).
    pub picture: u32,
    /// Picture repeats per meter.
    pub scale: f32,
    /// How far the picture slides per second of running time, in repeats, along x and y.
    pub scroll: [f32; 2],
}

impl CloudShadows {
    /// The picture coordinates of ground point `(x, y)` (the viewer's block frame) after the
    /// client has run for `uptime` seconds: the point times the scale, slid by the scroll times
    /// the time.
    // D2-10056, D2-13337
    #[must_use]
    pub fn uv(&self, x: f32, y: f32, uptime: f64) -> [f32; 2] {
        let off = self.offset(uptime);
        [x * self.scale + off[0], y * self.scale + off[1]]
    }

    /// The slide after `uptime` seconds, wrapped into one repeat (the picture repeats, so only
    /// the fraction matters, and keeping it small keeps a long-running clock precise).
    #[must_use]
    pub fn offset(&self, uptime: f64) -> [f32; 2] {
        let slide = |speed: f32| {
            let s = f64::from(speed) * uptime;
            #[allow(clippy::cast_possible_truncation)]
            let f = (s - s.floor()) as f32;
            f
        };
        [slide(self.scroll[0]), slide(self.scroll[1])]
    }
}
