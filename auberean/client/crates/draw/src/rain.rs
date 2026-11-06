//! Rain: up to 2,500 streaks falling about the viewer, as many as the day's rain amount asks.
//!
//! A streak picks a spot within 130 m of the viewer (on the ground, or on the water where the
//! viewer's cell has water above that ground) and then falls to it again and again: its life runs
//! from 1 down to 0 at 150 m a second over 80 m, and each time it wraps the streak takes a new
//! slant from the wind and gravity and a new landing a little off its spot. It is drawn as a quad
//! 8 m long, as wide as a tenth of a meter across the camera, fading out toward its top. Touching
//! down it throws up vapor, and on water within 65 m of the viewer a ripple. A streak that ends up
//! more than five cells from a viewer who has moved picks a new spot.
//!
//! The rain's particle systems stand at the viewer's block and take what the streaks and the
//! player's own body throw off ([`body_effects`]).

use std::sync::Arc;

use auberean_primitives::random::Random;
use auberean_primitives::Vec3;
use auberean_world::math::{add, cross, dot, length, normalize, scale, sub, Mat4};

use crate::mesh::MeshVertex;

/// The most streaks there are.
pub const SEGMENTS: usize = 2500;
/// How far from the viewer, each way, a streak may land.
const SPREAD: f32 = 130.0;
/// How high a streak starts over its landing, and how fast it falls, meters and meters a second.
const FALL: f32 = 80.0;
const SPEED: f32 = 150.0;
/// How long a streak is, and how near the ground it starts to shorten and throw up vapor.
const LENGTH: f32 = 8.0;
/// A streak's quad is this much of the camera's right each way from its line.
const HALF_WIDTH: f32 = 0.05;
/// How far off its spot a streak may land on each fall.
const JITTER: f32 = 0.8;
/// How far a streak may be left by a moving viewer, cells.
const REACH_CELLS: i32 = 5;
/// The square of how far off a ripple shows, meters.
const RIPPLE_REACH: f32 = 4225.0;
/// A streak's color at its foot, and at its top (the same, wholly transparent).
const FOOT: u32 = 0x72E5_E5FF;
const TOP: u32 = 0x00E5_E5FF;
/// The rain amount over which the streaks are drawn.
pub const SHOWS_OVER: f32 = 0.4;

/// The enumeration values of what rain is made of, in the records' mapping of its materials and
/// sounds: its material; its systems (the vapor, what rises from a body under water, the ripples);
/// its steady sound; its four thunderclaps.
pub const MATERIAL: u32 = 0x4000_000F;
pub const VAPOR: u32 = 0x4000_0010;
pub const BUBBLES: u32 = 0x4000_0011;
pub const RIPPLES: u32 = 0x4000_0012;
pub const STEADY_SOUND: u32 = 0x4000_0025;
pub const THUNDER: [u32; 4] = [0x4000_0026, 0x4000_0027, 0x4000_0028, 0x4000_0029];
/// The mapping those values are looked up in.
pub const MAPPING: u32 = 0x4000_0002;

#[derive(Clone, Copy, Debug)]
struct Segment {
    /// The land cell its spot is in: whole cells across the world.
    cell: (i32, i32),
    /// Its spot in that cell's block.
    spot: Vec3,
    /// Where it lands this fall, in that block.
    landing: Vec3,
    /// Its slant, a unit vector.
    slant: Vec3,
    /// Its spot is on water.
    on_water: bool,
    /// It has to pick a new spot.
    dead: bool,
    /// How far through its fall: 1 at the top, 0 at the ground.
    life: f32,
}

/// What the rain reads of the world this frame.
#[derive(Clone, Copy, Debug)]
pub struct RainWorld {
    /// The viewer's land cell, whole cells across the world.
    pub cell: (i32, i32),
    /// Where the viewer stands in its block.
    pub viewer: Vec3,
    /// The camera's right.
    pub right: Vec3,
    /// What pushes the streaks: the wind and gravity added.
    pub push: Vec3,
    /// How high the water of the viewer's cell lies, when it has any.
    pub water: Option<f32>,
    pub now: f64,
    pub cells_per_block: i32,
    pub cell_length: f32,
}

/// What rain throws off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Thrown {
    /// Vapor where a streak touched down or rain struck a body.
    Vapor,
    /// Bubbles about a body under water.
    Bubbles,
    /// A ripple on water.
    Ripple,
}

/// What the ground is at a spot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ground {
    /// Its block isn't loaded: the spot keeps the viewer's height, whatever the water.
    Unknown,
    /// Loaded, at this height, or with no ground there.
    At(Option<f32>),
}

/// The streaks.
#[derive(Debug)]
pub struct Rain {
    segments: Vec<Segment>,
    /// The cell the viewer last stood in.
    cell: Option<(i32, i32)>,
    last: f64,
    /// The quads of the last update.
    vertices: Arc<Vec<MeshVertex>>,
    /// The two triangles of each quad, for all of them.
    indices: Arc<Vec<u16>>,
}

impl Default for Rain {
    // D2-10930, D2-10913
    fn default() -> Self {
        let mut indices = Vec::with_capacity(SEGMENTS * 6);
        for k in 0..SEGMENTS {
            #[allow(clippy::cast_possible_truncation)] // the index wraps, as the client's does
            let n = (k * 4) as u16;
            indices.extend_from_slice(&[
                n,
                n.wrapping_add(1),
                n.wrapping_add(2),
                n,
                n.wrapping_add(3),
                n.wrapping_add(2),
            ]);
        }
        Self {
            segments: vec![
                Segment {
                    cell: (0, 0),
                    spot: Vec3::ZERO,
                    landing: Vec3::ZERO,
                    slant: Vec3::ZERO,
                    on_water: false,
                    dead: true,
                    life: 0.0,
                };
                SEGMENTS
            ],
            cell: None,
            last: 0.0,
            vertices: Arc::new(Vec::new()),
            indices: Arc::new(indices),
        }
    }
}

/// How many streaks fall at `amount` of rain, itself held between 0 and 1.
// D2-10935
#[must_use]
pub fn count(amount: f32) -> usize {
    #[allow(clippy::cast_precision_loss)]
    let total = SEGMENTS as f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (total * amount.clamp(0.0, 1.0)).floor() as usize;
    n
}

impl Rain {
    /// The quads of the last update: four corners a streak, in the viewer's block's frame, and the
    /// triangles of as many.
    #[must_use]
    pub fn drawn(&self) -> (Arc<Vec<MeshVertex>>, Arc<Vec<u16>>) {
        let quads = self.vertices.len() / 4;
        let indices = if quads == SEGMENTS {
            self.indices.clone()
        } else {
            Arc::new(self.indices[..quads * 6].to_vec())
        };
        (self.vertices.clone(), indices)
    }

    /// Move the rain on at `amount`: as many streaks as it asks fall, picking their spots with
    /// `ground`, and `throw` is told what they throw off and where in the viewer's block's frame.
    // D2-10934, D2-10935
    pub fn update(
        &mut self,
        amount: f32,
        w: &RainWorld,
        rng: &Random,
        ground: &mut dyn FnMut((i32, i32), Vec3) -> Ground,
        throw: &mut dyn FnMut(Thrown, Vec3),
    ) {
        let n = count(amount);
        if n == 0 {
            self.vertices = Arc::new(Vec::new());
            return;
        }
        if self.cell != Some(w.cell) {
            self.cell = Some(w.cell);
            for s in &mut self.segments[..n] {
                if !s.dead
                    && ((s.cell.0 - w.cell.0).abs() > REACH_CELLS
                        || (s.cell.1 - w.cell.1).abs() > REACH_CELLS)
                {
                    s.dead = true;
                }
            }
        }
        let side = scale(w.right, HALF_WIDTH);
        #[allow(clippy::cast_possible_truncation)]
        let dt = (w.now - self.last) as f32;
        self.last = w.now;
        #[allow(clippy::cast_precision_loss)]
        let block_length = w.cell_length * w.cells_per_block as f32;
        let block_of = |cell: (i32, i32)| {
            (
                cell.0.div_euclid(w.cells_per_block),
                cell.1.div_euclid(w.cells_per_block),
            )
        };
        let home = block_of(w.cell);
        let mut out = Vec::with_capacity(n * 4);
        for s in &mut self.segments[..n] {
            if s.dead {
                let mut p = w.viewer;
                p.x += rng.roll_float(-SPREAD, SPREAD);
                p.y += rng.roll_float(-SPREAD, SPREAD);
                let x = (home.0 * w.cells_per_block)
                    + dereth_primitives::num::floor_to_i32(p.x / w.cell_length);
                let y = (home.1 * w.cells_per_block)
                    + dereth_primitives::num::floor_to_i32(p.y / w.cell_length);
                if (0..256 * w.cells_per_block).contains(&x)
                    && (0..256 * w.cells_per_block).contains(&y)
                {
                    let block = block_of((x, y));
                    #[allow(clippy::cast_precision_loss)]
                    {
                        p.x -= (block.0 - home.0) as f32 * block_length;
                        p.y -= (block.1 - home.1) as f32 * block_length;
                    }
                    s.cell = (x, y);
                    s.spot = p;
                    s.on_water = false;
                    match ground(block, p) {
                        Ground::Unknown => {}
                        Ground::At(height) => {
                            if let Some(h) = height {
                                s.spot.z = h;
                            }
                            if let Some(h) = w.water {
                                if s.spot.z < h {
                                    s.spot.z = h;
                                    s.on_water = true;
                                }
                            }
                        }
                    }
                    s.landing = s.spot;
                    s.life = rng.roll_float(0.0, 1.0);
                    s.dead = false;
                }
            }
            let block = block_of(s.cell);
            #[allow(clippy::cast_precision_loss)]
            let offset = Vec3::new(
                (block.0 - home.0) as f32 * block_length,
                (block.1 - home.1) as f32 * block_length,
                0.0,
            );
            if s.life <= 0.0 {
                let whole = dereth_primitives::num::to_i32(s.life);
                #[allow(clippy::cast_precision_loss)]
                {
                    s.life = (s.life + 1.0) - whole as f32;
                }
                let y = w.push.y + rng.roll_float(-0.5, 0.5);
                let x = w.push.x + rng.roll_float(-0.5, 0.5);
                s.slant = normalize(Vec3::new(x, y, w.push.z));
                s.landing = s.spot;
                s.landing.x += rng.roll_float(-JITTER, JITTER);
                s.landing.y += rng.roll_float(-JITTER, JITTER);
            }
            let high = s.life * FALL;
            let mut long = LENGTH;
            if high <= LENGTH {
                let at = add(offset, s.landing);
                throw(Thrown::Vapor, at);
                long = high;
                if s.on_water {
                    let dx = s.landing.x - w.viewer.x + offset.x;
                    let dy = s.landing.y - w.viewer.y + offset.y;
                    if dx * dx + dy * dy < RIPPLE_REACH {
                        throw(Thrown::Ripple, at);
                    }
                }
            }
            let top = sub(s.landing, scale(s.slant, high));
            let foot = add(top, scale(s.slant, long));
            let corner = |p: Vec3, diffuse: u32, u: f32, v: f32| MeshVertex {
                position: [p.x, p.y, p.z],
                diffuse,
                uv: [[u, v]; 4],
                ..MeshVertex::default()
            };
            let (left, right) = (sub(offset, side), add(offset, side));
            out.push(corner(add(left, top), TOP, 0.0, 0.0));
            out.push(corner(add(left, foot), FOOT, 0.0, 1.0));
            out.push(corner(add(right, foot), FOOT, 1.0, 1.0));
            out.push(corner(add(right, top), TOP, 1.0, 0.0));
            s.life -= dt * SPEED / FALL;
        }
        self.vertices = Arc::new(out);
    }
}

/// One bone of a body's skeleton, in the body's frame: where it is and where its parent is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bone {
    pub at: Vec3,
    pub parent: Vec3,
}

/// What rain and water do to a body standing outdoors: for each bone, `rain` (the unit direction
/// rain falls in, when it is raining) strikes it three times in four, unless it lies along the
/// rain, throwing up vapor at a point along it above any water; with `water` (how high the water of
/// its cell lies), a point of it deep under water gives off bubbles four times in five and a bone
/// crossing the surface makes a ripple one time in five. `place` puts the body's frame in the
/// frame things are thrown in.
// D2-11036
pub fn body_effects(
    bones: &[Bone],
    place: &Mat4,
    rain: Option<Vec3>,
    water: Option<f32>,
    rng: &Random,
    throw: &mut dyn FnMut(Thrown, Vec3),
) {
    let rain_here = rain.map(|r| {
        let origin = place.point(Vec3::ZERO);
        let x = normalize(sub(place.point(Vec3::new(1.0, 0.0, 0.0)), origin));
        let y = normalize(sub(place.point(Vec3::new(0.0, 1.0, 0.0)), origin));
        let z = normalize(cross(x, y));
        Vec3::new(dot(r, x), dot(r, y), dot(r, z))
    });
    for b in bones {
        let struck = rain_here.is_some() && rng.roll_float(0.0, 1.0) <= 0.75;
        let bubbles = rng.roll_float(0.0, 1.0) <= 0.8;
        let ripple = rng.roll_float(0.0, 1.0) <= 0.2;
        if !(struck || bubbles || ripple) {
            continue;
        }
        let v = sub(b.at, b.parent);
        let long = length(v);
        if long <= 0.05 {
            continue;
        }
        let along = scale(v, 1.0 / long);
        if let (true, Some(r)) = (struck, rain_here) {
            if 1.0 - dot(along, r).abs() > 0.4 {
                let t = rng.roll_float(0.0, long);
                let p = place.point(add(b.parent, scale(along, t)));
                if water.is_none_or(|h| h < p.z) {
                    throw(Thrown::Vapor, p);
                }
            }
        }
        let Some(h) = water else { continue };
        if bubbles {
            let t = rng.roll_float(0.0, long);
            let p = place.point(add(b.parent, scale(along, t)));
            if p.z < h - 0.2 {
                throw(Thrown::Bubbles, p);
            }
        }
        if ripple {
            let (a, c) = (place.point(b.parent), place.point(b.at));
            if (h < a.z && c.z < h) || (a.z < h && h < c.z) {
                throw(
                    Thrown::Ripple,
                    Vec3::new((c.x + a.x) * 0.5, (c.y + a.y) * 0.5, h),
                );
            }
        }
    }
}

/// How much rain falls and how the wind blows at time of day `t` on `day` (the fraction of the
/// day gone): the day's rain and its wind; on a dry moment a wind that wanders with the time of
/// day.
// D2-11025
#[must_use]
pub fn weather(day: &auberean_assets::region::DayDesc, t: f32, rng: &Random) -> (f32, Vec3) {
    let (amount, x, y) = auberean_world::sky::rain(day, t);
    if amount != 0.0 {
        return (amount, Vec3::new(x, y, 0.0));
    }
    let w = auberean_assets::common::Waveform {
        kind: 7,
        amplitude: 2.0,
        frequency: 1.0,
        roughness: 1.0,
        ..auberean_assets::common::Waveform::default()
    };
    let at = f64::from(t * 30.0);
    let z = auberean_world::wave::value_for_phase(&w, 3000.0, at, rng) * 0.5;
    let y = auberean_world::wave::value_for_phase(&w, 2000.0, at, rng) * 4.0;
    let x = auberean_world::wave::value_for_phase(&w, 1000.0, at, rng) * 4.0;
    (0.0, Vec3::new(x, y, z))
}
