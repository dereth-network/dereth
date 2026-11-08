//! The lamps: the scene's lights worked out per pixel, from lists of the lights that reach each
//! piece of the view.
//!
//! - **Indoors** the lamps take the place of retail's own light on rooms and on the objects in
//!   them: the static lights retail burns into a room's walls at their vertices, the moving
//!   lights it lights them with as they are drawn, and the eight lights it lights each object
//!   with at its vertices.
//!   - On a room's walls a static light's lamp is retail's burn worked out at every pixel, where
//!     retail works it out at the walls' vertices and draws between them. It holds nothing of a
//!     room from one frame to the next.
//!   - On objects a static light's lamp falls off with the square of the distance and ends
//!     smoothly at its reach. Its strength is calibrated, light by light, to retail's burn: the
//!     brightness the light gives the surfaces round it, saturation included, is the burn's
//!     ([`calibrate`]).
//!   - A moving light's lamp is retail's own light worked out at every pixel: its intensity
//!     over the distance, ended smoothly at its reach.
//!   - Objects in a room are lit by the same lamps as its walls, so a room shows retail's
//!     brightness with the light drawn where it falls.
//! - **Outdoors at night** the outdoor placements' lamps (lamp posts, lanterns, torches: their
//!   lights, their flames and their warm glowing glass) and the lights of what moves outdoors light
//!   the ground and what stands on it. Retail lights the outdoors with the sun alone; these come on
//!   as the sun goes down ([`night`]). A placement's lamp glows too: what is right at it is
//!   brightened, its flame or its glass, and a soft halo stands round it (`halo.slang`), which what
//!   stands nearer the eye cuts. A flame flickers: it never repeats, a small flame faster than a
//!   large one, it goes redder as it dims, and its light sways a little.
//!
//! The view is cut into [`GRID`] pieces, its tiles of the screen by its slices of depth, and each
//! piece lists the lamps whose reach meets it (a compute pass, `lamps.slang`'s `cs_clusters`);
//! each pixel then adds the lamps of its piece (`fs_lamps`) into [`LampsPass`]'s target, the
//! lamps' light, which the resolve adds. Its alpha says where the lamps replace retail's light
//! indoors. [`clusters`] is the same listing on the processor.

use std::collections::{BTreeMap, HashMap};

use dereth_gfx::{
    Blend, Buffer, BufferDesc, BufferUsage, ColorAttachment, ColorTarget, ComputeDesc, Cull,
    Device, Factor, Format, GraphicsDesc, Load, Location, Pipeline, RasterPass, Recorder, Rect,
    Texture, TextureDesc, TextureUsage, Touch, Use,
};
use glam::{Mat4, Vec3, Vec4, Vec4Swizzles};

use super::{push_words, shader, Targets};
use crate::scene::{BlockId, FrameDelta, Lamp, LampSource, LightKind, SceneDelta};
use crate::shaders::Entry;

/// What the lamps light. Off, the pass draws nothing and the frame is as it is without it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LampSettings {
    /// Rooms and the objects in them lit by the lamps per pixel, in place of retail's light.
    pub indoor: bool,
    /// The outdoors lit at night by the lights the outdoor placements carry and the lights of
    /// what moves outdoors.
    pub outdoor: bool,
}

impl LampSettings {
    /// Nothing lit.
    pub const OFF: Self = Self {
        indoor: false,
        outdoor: false,
    };

    /// Whether anything is.
    #[must_use]
    pub const fn any(self) -> bool {
        self.indoor || self.outdoor
    }
}

/// The view's pieces: tiles across, tiles down, slices of depth.
pub const GRID: [u32; 3] = [16, 9, 24];

/// The lamps one piece lists at most.
pub const PER_PIECE: u32 = 63;

/// The words one piece takes in the list: its count, then its lamps.
const PIECE_WORDS: u32 = PER_PIECE + 1;

/// The depth the slices start at and end at, metres from the eye. Nearer than the first is the
/// first slice; nothing farther than the last is lit.
pub const SLICE_NEAR: f32 = 0.5;
pub const SLICE_FAR: f32 = 240.0;

/// The lamps listed at most, of each kind.
pub const MAX_LAMPS: usize = 2048;

/// How far from the eye an outdoor lamp is listed, metres.
pub const OUTDOOR_REACH: f32 = 160.0;

/// The distance a lamp's light stops growing nearer than, metres: a surface touching a lamp is
/// lit as one this far from it.
pub const SOFTEN: f32 = 0.5;

/// How a lamp's light ends at its reach: 1 near it, 0 at `d == reach`, smoothly between.
fn window(d: f32, reach: f32) -> f32 {
    if d >= reach || reach <= 0.0 {
        return 0.0;
    }
    let x = d / reach;
    let w = (1.0 - x * x * x * x).clamp(0.0, 1.0);
    w * w
}

/// A static light's lamp's light at distance `d` within reach `reach`, before its strength and
/// the surface's turn: the inverse square, ended smoothly at the reach.
#[must_use]
pub fn falloff(d: f32, reach: f32) -> f32 {
    window(d, reach) / (d * d + SOFTEN * SOFTEN)
}

/// A moving light's lamp's light at distance `d` within reach `reach`, before its intensity and
/// the surface's turn: retail's, the inverse of the distance, faded out over the last tenth of
/// the reach where retail's ends at once.
#[must_use]
pub fn falloff_moving(d: f32, reach: f32) -> f32 {
    if d >= reach || reach <= 0.0 {
        return 0.0;
    }
    ((reach - d) / (0.1 * reach)).clamp(0.0, 1.0) / d.max(1e-3)
}

/// How a surface turned to a lamp takes its light, at cosine `mu`: wrapped as retail's burn
/// turns it, lit to 120 degrees and at two thirds facing it squarely, or as a plain cosine.
#[must_use]
pub fn turn(mu: f32, wrapped: bool) -> f32 {
    if wrapped {
        ((mu + 0.5) * (2.0 / 3.0)).max(0.0)
    } else {
        mu.max(0.0)
    }
}

/// Retail's burn of one static light into a room's wall, a channel's share of the light's
/// color: at distance `d` from a light of `intensity` and `falloff`, on a wall whose normal
/// makes cosine `mu` with the direction to it. Flat within a metre, the inverse square beyond,
/// zero past 1.3 times the falloff and past 120 degrees, at most the light's own color.
#[must_use]
pub fn retail_burn(d: f32, mu: f32, intensity: f32, falloff: f32) -> f32 {
    let reach = falloff * 1.3;
    if d >= reach || mu <= -0.5 {
        return 0.0;
    }
    let near = if d * d > 1.0 { 1.0 / (d * d) } else { 1.0 };
    ((1.0 - d / reach) * intensity * (mu + 0.5) * (2.0 / 3.0) * near).min(1.0)
}

/// Retail's fixed-function light on an object's vertex (and on a room's, for the lights that
/// move): the light's intensity times the cosine over the distance, within 1.5 times its
/// falloff, at most the light's own color.
#[must_use]
pub fn retail_point(d: f32, mu: f32, intensity: f32, falloff: f32) -> f32 {
    if d >= falloff * 1.5 || mu <= 0.0 {
        return 0.0;
    }
    (intensity * mu / d.max(1e-3)).min(1.0)
}

/// The strength a lamp of reach `reach` needs so that the brightness it gives the surfaces round
/// it (each turned to it as `wrapped` says, see [`turn`]), each at most the lamp's color, adds up
/// to what `retail` gives them (a channel's share, at a distance and a cosine): over every
/// distance to `reach`, each weighted by the area of the sphere there, and every turn of the
/// surface.
#[must_use]
pub fn calibrate(retail: impl Fn(f32, f32) -> f32, reach: f32, wrapped: bool) -> f32 {
    const DISTANCES: usize = 96;
    const TURNS: usize = 48;
    if reach <= 0.0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)] // small grid counts
    let sample =
        |k: usize, n: usize, span: f32, from: f32| from + (k as f32 + 0.5) * span / n as f32;
    let mut points = Vec::with_capacity(DISTANCES * TURNS);
    for i in 0..DISTANCES {
        let d = sample(i, DISTANCES, reach, 0.0);
        for j in 0..TURNS {
            let mu = sample(j, TURNS, 2.0, -1.0);
            points.push((d, mu, d * d));
        }
    }
    let want: f32 = points.iter().map(|&(d, mu, a)| retail(d, mu) * a).sum();
    if want <= 0.0 {
        return 0.0;
    }
    let given = |power: f32| -> f32 {
        points
            .iter()
            .map(|&(d, mu, a)| (power * turn(mu, wrapped) * falloff(d, reach)).min(1.0) * a)
            .sum()
    };
    // The brightness grows with the strength; halve the interval to the strength that gives it.
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    while given(hi) < want && hi < 1.0e7 {
        hi *= 4.0;
    }
    for _ in 0..48 {
        let mid = 0.5 * (lo + hi);
        if given(mid) < want {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// The kinds of lamp, as the shader tells them apart: a static light indoors (its turn wrapped
/// on walls), one outdoors (wrapped), a moving one indoors (plain).
const KIND_STATIC: f32 = 1.0;
const KIND_OUTDOOR: f32 = 2.0;
const KIND_MOVING: f32 = 3.0;

/// One lamp as the pass lists it, in the frame's space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuLamp {
    pub position: Vec3,
    /// How far its light reaches on walls and outdoors, the farthest it reaches anything.
    pub reach: f32,
    /// How far it reaches objects indoors.
    pub object_reach: f32,
    pub color: [f32; 3],
    /// Whether it lights the indoors (the light pools) or the outdoors.
    pub indoor: bool,
    /// Whether it is a static light: on walls its lamp is retail's burn, and outdoors its turn is
    /// wrapped as the burn's.
    pub burned: bool,
    /// Its strength on rooms' walls (its intensity: its lamp there is retail's own light), on
    /// objects indoors, and outdoors (night folded in).
    pub walls: f32,
    pub objects: f32,
    pub outdoors: f32,
    /// How near a surface brightens with the lamp's own glow, metres, and how much (night and
    /// flicker folded in); nought for a lamp that doesn't glow.
    pub glow_reach: f32,
    pub glow: f32,
}

/// One light's lamp, as [`Calibration::lamp`] works it out: its reach on walls and on objects,
/// and its strength.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calibrated {
    pub reach: f32,
    pub object_reach: f32,
    pub strength: f32,
}

/// Each light's lamp, worked out once per light.
#[derive(Debug, Default)]
pub struct Calibration {
    // ORDER-OK: a memo, only looked up.
    memo: HashMap<(bool, u32, u32), Calibrated>,
}

impl Calibration {
    /// The lamp of a light of `intensity` and `falloff`, burned into walls when `burned` (a
    /// static light) or lighting them as it is drawn (a moving one).
    ///
    /// A static light reaches as far as retail burns it, 1.3 times its falloff, and its strength
    /// is matched to the burn, its turn wrapped as the burn's is. A moving light reaches as far
    /// as retail lights with it, 1.5 times its falloff, its strength its intensity: its lamp is
    /// retail's light ([`falloff_moving`]).
    ///
    /// Objects take the lamp with its turn plain, out to the falloff: retail lights an object only
    /// by the lights whose falloff meets it, and lights it at full brightness wherever it faces a
    /// strong one, which no lamp falling with distance can match.
    pub fn lamp(&mut self, burned: bool, intensity: f32, falloff: f32) -> Calibrated {
        *self
            .memo
            .entry((burned, intensity.to_bits(), falloff.to_bits()))
            .or_insert_with(|| {
                let (reach, strength) = if burned {
                    let reach = falloff * 1.3;
                    (
                        reach,
                        calibrate(|d, mu| retail_burn(d, mu, intensity, falloff), reach, true),
                    )
                } else {
                    (falloff * 1.5, intensity)
                };
                Calibrated {
                    reach,
                    object_reach: falloff.min(reach),
                    strength,
                }
            })
    }

    /// How many lights it has worked out.
    #[must_use]
    pub fn len(&self) -> usize {
        self.memo.len()
    }

    /// Whether it has worked none out.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.memo.is_empty()
    }
}

/// How far the outdoor lamps are lit, 0 by day to 1 at night, from the sun: they come on as it
/// sinks below about ten degrees and are full once it is down to one, and are full when it gives
/// no light at all.
#[must_use]
pub fn night(light: &crate::scene::RetailLight) -> f32 {
    let b = light.sun.length();
    let elevation = if b > 1e-4 { light.sun.z / b } else { -1.0 };
    let t = ((0.18 - elevation) / 0.16).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How far an outdoor light's pool reaches against its falloff, the least and the most it reaches,
/// metres, and how strong it is at full brightness.
const OUTDOOR_RANGE: f32 = 2.0;
const OUTDOOR_POOL: (f32, f32) = (5.0, 16.0);
const OUTDOOR_STRENGTH: f32 = 8.0;
/// The color a flame's warmth takes a moving light toward, and how far.
const OUTDOOR_WARM: [f32; 3] = [1.0, 0.62, 0.3];
const OUTDOOR_WARMTH: f32 = 0.5;
/// The color a flame's sprite is pulled toward, so a deep red sprite still lights the ground as
/// firelight does.
const FIRE: [f32; 3] = [1.0, 0.52, 0.18];
/// The warm light most lamps throw on the ground: their authored colors are saturated, and are
/// pulled halfway toward it.
const LAMPLIGHT: [f32; 3] = [1.0, 0.56, 0.26];
/// Two lights nearer than this, metres, are one lamp: a torch and its post, say.
const ONE_LAMP: f32 = 0.6;
/// How far from the eye a lamp stands a halo, metres, and how many do in a frame.
const HALO_REACH: f32 = 120.0;
pub const MAX_HALOS: usize = 128;

/// `c` taken a share `k` toward `to`, its brightest channel 1.
fn toward(c: [f32; 3], to: [f32; 3], k: f32) -> [f32; 3] {
    let m = [0, 1, 2].map(|i| c[i] + (to[i] - c[i]) * k);
    let top = m[0].max(m[1]).max(m[2]).max(1e-4);
    m.map(|v| v / top)
}

/// How a flame's light stands at one moment against burning steadily.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Flame {
    /// Its brightness, 1 its usual.
    brightness: f32,
    /// What its color is multiplied by besides its brightness: a dimmer flame is a redder one.
    tint: [f32; 3],
    /// How far its light stands from its place, meters.
    sway: Vec3,
}

impl Flame {
    const STEADY: Self = Self {
        brightness: 1.0,
        tint: [1.0; 3],
        sway: Vec3::ZERO,
    };
}

/// How far a flame's brightness wanders about its usual at most, a share of it, and how far a
/// draught takes it down besides.
const FLAME_WANDER: f32 = 0.3;
const FLAME_DRAUGHT: f32 = 0.15;
/// How fast a flame pulses, the slowest and the fastest drawn, times a second. A flame `d` meters
/// across pulses about `1.5 / sqrt(d)` times a second, so a candle dances and a bonfire rolls.
const FLAME_PULSE: (f32, f32) = (0.9, 3.5);
/// How much faster than red the green and the blue fall as a flame dims, as powers of its
/// brightness.
const FLAME_COOLING: [f32; 2] = [0.4, 1.0];

/// About how wide a flame is whose light reaches `falloff` meters: a larger fire lights farther.
fn flame_size(falloff: f32) -> f32 {
    (falloff * 0.04).clamp(0.05, 1.5)
}

/// Smooth noise at `t`, -1 to 1, for the flame `seed`'s strand `strand`: values at the whole
/// numbers eased between, so it never steps from frame to frame and never repeats.
fn strand(seed: u32, strand: u32, t: f32) -> f32 {
    let whole = t.floor();
    let f = t - whole;
    // LINT-OK: a whole number of seconds' lattice, far inside the type for a session's clock.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let i = whole as i32 as u32;
    let at = |k: u32| {
        let mut h = k ^ seed.rotate_left(7) ^ strand.wrapping_mul(0x2C1B_3C6D);
        h = (h ^ (h >> 16)).wrapping_mul(0x7FEB_352D);
        h = (h ^ (h >> 15)).wrapping_mul(0x846C_A68B);
        h ^= h >> 16;
        #[allow(clippy::cast_precision_loss)] // 24 bits, exact in f32
        let v = (h >> 8) as f32 / 16_777_215.0;
        v * 2.0 - 1.0
    };
    let e = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    at(i) + (at(i.wrapping_add(1)) - at(i)) * e
}

/// A lamp's flame at `t` seconds: steady for a lamp that isn't one. Its own number keeps it from
/// moving with any other, and the same clock gives the same flame however fast it is drawn.
fn flicker(lamp: &Lamp, t: f32) -> Flame {
    use dereth_primitives::num::math;
    if !lamp.flicker {
        return Flame::STEADY;
    }
    let size = flame_size(lamp.falloff);
    let pulse = (1.5 / size.sqrt()).clamp(FLAME_PULSE.0, FLAME_PULSE.1);
    let s = lamp.seed;
    // Three octaves at unrelated rates, the slowest carrying most.
    let wander = 0.6 * strand(s, 0, t * pulse)
        + 0.28 * strand(s, 1, t * pulse * 2.31)
        + 0.12 * strand(s, 2, t * pulse * 3.93);
    // Now and then a draught catches it: it ducks and comes back.
    let draught = ((strand(s, 3, t * 0.27) - 0.3) / 0.7).max(0.0);
    let k = 1.0 + FLAME_WANDER * wander - FLAME_DRAUGHT * draught * draught;
    let lean = (0.02 + 0.04 * size).min(0.07);
    let sway = Vec3::new(
        strand(s, 4, t * pulse * 0.6),
        strand(s, 5, t * pulse * 0.6),
        0.5 * (k - 1.0) / FLAME_WANDER,
    ) * lean;
    Flame {
        brightness: k,
        tint: [
            1.0,
            math::powf(k, FLAME_COOLING[0]),
            math::powf(k, FLAME_COOLING[1]),
        ],
        sway,
    }
}

/// How an outdoor placement's lamp is lit, by what it is.
#[derive(Debug, Clone, Copy, PartialEq)]
struct LampShape {
    color: [f32; 3],
    brightness: f32,
    /// How far its pool reaches, metres.
    reach: f32,
    /// How near a surface takes its glow, and how wide its halo stands, metres.
    glow_reach: f32,
    halo: f32,
}

fn lamp_shape(lamp: &Lamp) -> LampShape {
    let range = lamp.falloff * OUTDOOR_RANGE;
    match lamp.source {
        LampSource::Authored => LampShape {
            color: toward(lamp.color, LAMPLIGHT, 0.5),
            brightness: (lamp.intensity / 100.0).clamp(0.3, 1.0),
            reach: range.clamp(OUTDOOR_POOL.0, OUTDOOR_POOL.1),
            glow_reach: 0.32,
            halo: 1.0,
        },
        LampSource::Flame => LampShape {
            color: toward(lamp.color, FIRE, 0.6),
            brightness: 0.9,
            reach: (range * 0.75).clamp(4.0, 10.0),
            glow_reach: 0.25,
            halo: 0.75,
        },
        LampSource::Glow => LampShape {
            color: toward(lamp.color, LAMPLIGHT, 0.3),
            brightness: (lamp.intensity * 0.55).clamp(0.3, 1.0),
            reach: (range * 0.8).clamp(4.0, 12.0),
            glow_reach: 0.4,
            halo: 0.85,
        },
    }
}

/// One lamp's halo: where it stands, how wide, and its color with the night and its flicker folded
/// in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Halo {
    pub position: Vec3,
    pub size: f32,
    pub color: [f32; 3],
}

/// The outdoor placements' lamps of `outdoor` as `frame` lights them at `t` seconds (its night
/// `n`), with their halos: the nearest first, and lamps standing in one place as one.
fn placed_lamps(
    frame: &FrameDelta,
    outdoor: &BTreeMap<BlockId, Vec<Lamp>>,
    n: f32,
    t: f32,
) -> (Vec<GpuLamp>, Vec<Halo>) {
    let eye = frame.camera.eye;
    let mut lit = Vec::new();
    for (id, lamps) in outdoor {
        let (x, y) = (i32::from(id >> 8), i32::from(id & 0xFF));
        #[allow(clippy::cast_precision_loss)] // a few hundred blocks
        let o = Vec3::new(
            (x - frame.origin[0]) as f32 * 192.0,
            (y - frame.origin[1]) as f32 * 192.0,
            0.0,
        );
        for lamp in lamps {
            let shape = lamp_shape(lamp);
            let mut position = lamp.position + o;
            position.z += super::light::lifted(frame, lamp.lifts, position);
            if position.distance(eye) > OUTDOOR_REACH + shape.reach {
                continue;
            }
            let flame = flicker(lamp, t);
            let k = n * flame.brightness;
            lit.push((
                GpuLamp {
                    position: position + flame.sway,
                    reach: shape.reach,
                    object_reach: shape.reach,
                    color: [0, 1, 2].map(|c| shape.color[c] * flame.tint[c]),
                    indoor: false,
                    burned: true,
                    walls: 0.0,
                    objects: 0.0,
                    outdoors: OUTDOOR_STRENGTH * shape.brightness * k,
                    glow_reach: shape.glow_reach,
                    glow: k,
                },
                shape,
            ));
        }
    }
    lit.sort_by(|a, b| {
        a.0.position
            .distance_squared(eye)
            .total_cmp(&b.0.position.distance_squared(eye))
    });
    let mut kept: Vec<(GpuLamp, LampShape)> = Vec::with_capacity(lit.len());
    for l in lit {
        if !kept
            .iter()
            .any(|k| k.0.position.distance_squared(l.0.position) < ONE_LAMP * ONE_LAMP)
        {
            kept.push(l);
        }
    }
    let halos = kept
        .iter()
        .filter(|(l, _)| l.position.distance(eye) < HALO_REACH)
        .take(MAX_HALOS)
        .map(|(l, s)| Halo {
            position: l.position,
            size: s.halo,
            color: l.color.map(|c| c * s.brightness * l.glow),
        })
        .collect();
    (kept.into_iter().map(|(l, _)| l).collect(), halos)
}

/// The lamps of the frame as `settings` lights them at `t` seconds: the light pools for the indoors,
/// each light calibrated; and, as night comes, the outdoor placements' lamps `outdoor` (block by
/// block, in their blocks' own space) and the moving lights outdoors, within [`OUTDOOR_REACH`] of
/// the eye. With the halos of what stands at the placements' lamps.
pub fn lamps_of(
    frame: &FrameDelta,
    outdoor: &BTreeMap<BlockId, Vec<Lamp>>,
    settings: LampSettings,
    calibration: &mut Calibration,
    t: f32,
) -> (Vec<GpuLamp>, Vec<Halo>) {
    let mut out = Vec::new();
    let mut halos = Vec::new();
    let l = &frame.light;
    if settings.indoor {
        for (i, p) in l.lights.iter().enumerate().take(MAX_LAMPS) {
            if p.kind != LightKind::Point {
                continue;
            }
            let burned = i < l.statics;
            let c = calibration.lamp(burned, p.intensity, p.falloff);
            out.push(GpuLamp {
                position: p.position,
                reach: c.reach,
                object_reach: c.object_reach,
                color: p.color,
                indoor: true,
                burned,
                walls: p.intensity,
                objects: c.strength,
                outdoors: 0.0,
                glow_reach: 0.0,
                glow: 0.0,
            });
        }
    }
    let n = night(l);
    if settings.outdoor && n > 0.0 {
        let eye = frame.camera.eye;
        let (mut outdoors, placed) = placed_lamps(frame, outdoor, n, t);
        if frame.landscape {
            halos = placed;
        }
        for light in frame.lamps.iter().filter(|m| m.kind == LightKind::Point) {
            // A small warm pool about its foot: no retail light does this for it to be matched to.
            let reach = (light.falloff * OUTDOOR_RANGE).clamp(OUTDOOR_POOL.0, OUTDOOR_POOL.1);
            if light.position.distance(eye) > OUTDOOR_REACH + reach {
                continue;
            }
            outdoors.push(GpuLamp {
                position: light.position,
                reach,
                object_reach: reach,
                color: [0, 1, 2]
                    .map(|k| light.color[k] + (OUTDOOR_WARM[k] - light.color[k]) * OUTDOOR_WARMTH),
                indoor: false,
                burned: true,
                walls: 0.0,
                objects: 0.0,
                outdoors: OUTDOOR_STRENGTH * (light.intensity / 100.0).clamp(0.3, 1.0) * n,
                glow_reach: 0.0,
                glow: 0.0,
            });
        }
        // The nearest first, so a capped list keeps the ones nearest the eye.
        outdoors.sort_by(|a, b| {
            a.position
                .distance_squared(eye)
                .total_cmp(&b.position.distance_squared(eye))
        });
        outdoors.truncate(MAX_LAMPS);
        out.extend(outdoors);
    }
    (out, halos)
}

/// The lamps as rows of four 32-bit values, three a lamp: where it is and its reach, its color
/// and kind, and its three strengths with its reach on objects.
#[must_use]
pub fn lamp_rows(lamps: &[GpuLamp]) -> Vec<u8> {
    let mut out = Vec::with_capacity(lamps.len() * 48);
    let mut f = |v: [f32; 4]| {
        for x in v {
            out.extend_from_slice(&x.to_le_bytes());
        }
    };
    for l in lamps {
        f([l.position.x, l.position.y, l.position.z, l.reach]);
        f([
            l.color[0],
            l.color[1],
            l.color[2],
            match (l.indoor, l.burned) {
                (false, _) => KIND_OUTDOOR,
                (true, true) => KIND_STATIC,
                (true, false) => KIND_MOVING,
            },
        ]);
        if l.indoor {
            f([l.walls, l.objects, l.outdoors, l.object_reach]);
        } else {
            f([l.glow_reach, l.glow, l.outdoors, l.object_reach]);
        }
    }
    out
}

/// The halos' constants as rows of four 32-bit values: the view's right and up, then for each its
/// place and width, and its color.
fn halo_rows(view: Mat4, halos: &[Halo]) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 + halos.len() * 32);
    let mut f = |v: [f32; 4]| {
        for x in v {
            out.extend_from_slice(&x.to_le_bytes());
        }
    };
    let to_world = view.inverse();
    let (right, up) = (to_world.x_axis, to_world.y_axis);
    f([right.x, right.y, right.z, 0.0]);
    f([up.x, up.y, up.z, 0.0]);
    for h in halos {
        f([h.position.x, h.position.y, h.position.z, h.size]);
        f([h.color[0], h.color[1], h.color[2], 0.0]);
    }
    out
}

/// How the view is cut into [`GRID`]'s pieces for a frame: its camera's view and projection and
/// the world's size in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub view: Mat4,
    pub projection: Mat4,
    pub extent: (u32, u32),
}

impl View {
    /// Each tile's size in pixels.
    #[must_use]
    pub fn tile(&self) -> (u32, u32) {
        (
            self.extent.0.div_ceil(GRID[0]).max(1),
            self.extent.1.div_ceil(GRID[1]).max(1),
        )
    }

    /// The depth slice `k` starts at, metres from the eye.
    #[must_use]
    pub fn slice_depth(k: u32) -> f32 {
        #[allow(clippy::cast_precision_loss)] // a few dozen slices
        let t = k as f32 / GRID[2] as f32;
        SLICE_NEAR * dereth_primitives::num::math::powf(SLICE_FAR / SLICE_NEAR, t)
    }

    /// The direction from the eye, in the eye's space, through the point `ndc` of the screen,
    /// scaled to depth 1.
    fn ray(&self, ndc: (f32, f32)) -> Vec3 {
        let p = self.projection.inverse() * Vec4::new(ndc.0, ndc.1, 1.0, 1.0);
        let v = p.xyz() / p.w;
        if v.z.abs() < 1e-9 {
            return Vec3::Z;
        }
        v / v.z
    }

    /// Piece `(i, j, k)`'s bounds in the eye's space: its corner nearest the origin of each
    /// axis, and its farthest.
    #[must_use]
    pub fn bounds(&self, i: u32, j: u32, k: u32) -> (Vec3, Vec3) {
        let (tw, th) = self.tile();
        #[allow(clippy::cast_precision_loss)] // pixel counts
        let ndc = |px: u32, py: u32| {
            (
                px.min(self.extent.0) as f32 / self.extent.0.max(1) as f32 * 2.0 - 1.0,
                1.0 - py.min(self.extent.1) as f32 / self.extent.1.max(1) as f32 * 2.0,
            )
        };
        let (z0, z1) = (Self::slice_depth(k), Self::slice_depth(k + 1));
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for (px, py) in [
            (i * tw, j * th),
            ((i + 1) * tw, j * th),
            (i * tw, (j + 1) * th),
            ((i + 1) * tw, (j + 1) * th),
        ] {
            let r = self.ray(ndc(px, py));
            for z in [z0, z1] {
                let p = r * z;
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
        (lo, hi)
    }
}

/// Whether the sphere at `centre` of radius `radius` meets the box from `lo` to `hi`.
#[must_use]
pub fn meets(centre: Vec3, radius: f32, lo: Vec3, hi: Vec3) -> bool {
    let nearest = centre.clamp(lo, hi);
    nearest.distance_squared(centre) <= radius * radius
}

/// Each piece's lamps, as the compute pass lists them: piece `(i, j, k)` at
/// `(k * GRID[1] + j) * GRID[0] + i`, its lamps in order, at most [`PER_PIECE`].
#[must_use]
pub fn clusters(lamps: &[GpuLamp], v: &View) -> Vec<Vec<u32>> {
    let eye_space: Vec<Vec3> = lamps
        .iter()
        .map(|l| v.view.transform_point3(l.position))
        .collect();
    let mut out = Vec::new();
    for k in 0..GRID[2] {
        for j in 0..GRID[1] {
            for i in 0..GRID[0] {
                let (lo, hi) = v.bounds(i, j, k);
                let mut list = Vec::new();
                for (n, (l, c)) in lamps.iter().zip(&eye_space).enumerate() {
                    if list.len() >= PER_PIECE as usize {
                        break;
                    }
                    if meets(*c, l.reach, lo, hi) {
                        list.push(u32::try_from(n).unwrap_or(u32::MAX));
                    }
                }
                out.push(list);
            }
        }
    }
    out
}

/// The format of the lamps' light.
pub const LAMP_LIGHT: Format = Format::Rgba16Float;

/// One frame's lamps, as the pass records them.
#[derive(Debug, Clone, Copy)]
pub struct LampsPlan {
    /// The lamps' light, and its index in the table: rgb the light the lamps add, a 1 where they
    /// take the place of retail's own light indoors.
    pub light: Texture,
    pub light_index: u32,
    lamps: Buffer,
    lamps_index: u32,
    pieces: Buffer,
    pieces_index: u32,
    rows: u32,
    count: u32,
    /// Whether the viewer stands inside with the outdoors drawn: the outdoor surfaces are then
    /// placed by the outdoor depth.
    inside: bool,
    clusters: Pipeline,
    shade: Pipeline,
    /// The halos' constants, how many, and how they are drawn on the frame's kind of target.
    halos: Option<(u32, u32, Pipeline)>,
}

/// What the pass needs of a frame.
#[derive(Debug)]
pub struct LampInputs<'a> {
    pub frame: &'a FrameDelta,
    /// The world's targets.
    pub targets: &'a Targets,
    /// Whether the scene's colors are linear this frame, as the post passes want them.
    pub linear: bool,
}

/// The lamps' pass: its pipelines, made when it is, its buffers, its target, and the outdoor
/// placements' lights of the blocks the scene holds.
#[derive(Debug)]
pub struct LampsPass {
    settings: LampSettings,
    clusters: Pipeline,
    shade: Pipeline,
    /// The halos over each kind of target.
    halo: [(Format, Pipeline); 2],
    halos: Grow,
    lamps: Grow,
    rows: Grow,
    /// The scene's clock when the lamps were first lit.
    lit_from: Option<f64>,
    pieces: Buffer,
    pieces_index: u32,
    light: Option<(Texture, u32, u32, u32)>,
    // ORDER-OK: walked in block order; only the set matters.
    outdoor: BTreeMap<BlockId, Vec<Lamp>>,
    calibration: Calibration,
    last_count: u32,
    /// The last frame's lamps and how its view was cut, for [`Self::reference`].
    #[cfg(any(test, feature = "test-support"))]
    last: Option<(Vec<GpuLamp>, View)>,
    /// The last frame's halos.
    #[cfg(any(test, feature = "test-support"))]
    last_halos: Vec<Halo>,
}

/// A storage buffer rewritten every frame, grown when a frame needs more.
#[derive(Debug)]
struct Grow {
    buffer: Buffer,
    index: u32,
    size: u64,
    label: &'static str,
}

impl Grow {
    fn new(d: &mut Device, label: &'static str, size: u64) -> Result<Self, dereth_gfx::Error> {
        let buffer = d.create_buffer(&BufferDesc {
            label,
            size,
            usage: BufferUsage::STORAGE,
            location: Location::Gpu,
        })?;
        let index = d.bind_buffer(buffer)?;
        Ok(Self {
            buffer,
            index,
            size,
            label,
        })
    }

    fn write(&mut self, d: &mut Device, bytes: &[u8]) -> Result<(), dereth_gfx::Error> {
        let need = (bytes.len() as u64).max(16);
        if need > self.size {
            let size = need.next_power_of_two();
            let bigger = d.create_buffer(&BufferDesc {
                label: self.label,
                size,
                usage: BufferUsage::STORAGE,
                location: Location::Gpu,
            })?;
            d.destroy_buffer(std::mem::replace(&mut self.buffer, bigger));
            self.index = d.bind_buffer(self.buffer)?;
            self.size = size;
        }
        if bytes.is_empty() {
            return Ok(());
        }
        d.write_buffer(self.buffer, 0, bytes)
    }
}

/// The pieces' list, every piece's words.
const PIECES_BYTES: u64 = (GRID[0] * GRID[1] * GRID[2] * PIECE_WORDS) as u64 * 4;

impl LampsPass {
    /// The pass on `gfx`, its pipelines made, off.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(gfx: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let clusters = gfx.create_compute_pipeline(&ComputeDesc {
            label: "remaster lamp lists",
            shader: shader(Entry::LampsCsClusters),
        })?;
        let shade = gfx.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster lamps",
            vertex: shader(Entry::FullscreenVsMain),
            fragment: Some(shader(Entry::LampsFsLamps)),
            color: &[ColorTarget {
                format: LAMP_LIGHT,
                blend: None,
                write_alpha: true,
            }],
            depth: None,
            cull: Cull::None,
        })?;
        let linear = super::post::scene_format(gfx);
        let mut halo = |format| {
            gfx.create_graphics_pipeline(&GraphicsDesc {
                label: "remaster lamp halos",
                vertex: shader(Entry::HaloVsHalo),
                fragment: Some(shader(Entry::HaloFsHalo)),
                color: &[ColorTarget {
                    format,
                    blend: Some(Blend {
                        src: Factor::One,
                        dst: Factor::One,
                    }),
                    write_alpha: false,
                }],
                depth: None,
                cull: Cull::None,
            })
            .map(|p| (format, p))
        };
        let halo = [halo(super::COLOR)?, halo(linear)?];
        let pieces = gfx.create_buffer(&BufferDesc {
            label: "remaster lamp pieces",
            size: PIECES_BYTES,
            usage: BufferUsage::STORAGE | BufferUsage::TRANSFER_SRC,
            location: Location::Gpu,
        })?;
        let pieces_index = gfx.bind_buffer(pieces)?;
        Ok(Self {
            settings: LampSettings::OFF,
            clusters,
            shade,
            halo,
            halos: Grow::new(gfx, "remaster lamp halos", 1 << 12)?,
            lit_from: None,
            lamps: Grow::new(gfx, "remaster lamps", 1 << 14)?,
            rows: Grow::new(gfx, "remaster lamp frame", 512)?,
            pieces,
            pieces_index,
            light: None,
            outdoor: BTreeMap::new(),
            calibration: Calibration::default(),
            last_count: 0,
            #[cfg(any(test, feature = "test-support"))]
            last_halos: Vec::new(),
            #[cfg(any(test, feature = "test-support"))]
            last: None,
        })
    }

    /// What the lamps light from the next frame on.
    pub fn set_settings(&mut self, settings: LampSettings) {
        self.settings = settings;
    }

    /// What the lamps light.
    #[must_use]
    pub fn settings(&self) -> LampSettings {
        self.settings
    }

    /// The lamps the last frame listed.
    #[must_use]
    pub fn last_count(&self) -> u32 {
        self.last_count
    }

    /// The last frame's halos and how its view was cut.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn last_halos(&self) -> Option<(&[Halo], &View)> {
        let (_, view) = self.last.as_ref()?;
        Some((&self.last_halos, view))
    }

    /// The last frame's lamps, and each piece's list of them as [`clusters`] makes it on the
    /// processor: what the compute pass's lists are checked against.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn reference(&self) -> Option<(&[GpuLamp], Vec<Vec<u32>>)> {
        let (lamps, view) = self.last.as_ref()?;
        Some((lamps, clusters(lamps, view)))
    }

    /// How far within or beyond lamp `lamp`'s reach piece `piece` of the last frame's view lies,
    /// metres: a list that has the lamp where the reference has not, or the other way round, is
    /// only right on this edge.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn edge(&self, piece: u32, lamp: u32) -> Option<f32> {
        let (lamps, v) = self.last.as_ref()?;
        let l = lamps.get(lamp as usize)?;
        let (i, j, k) = (
            piece % GRID[0],
            (piece / GRID[0]) % GRID[1],
            piece / (GRID[0] * GRID[1]),
        );
        let (lo, hi) = v.bounds(i, j, k);
        let c = v.view.transform_point3(l.position);
        Some(c.clamp(lo, hi).distance(c) - l.reach)
    }

    /// The outdoor placements' lights of the blocks held, by block.
    #[must_use]
    pub fn outdoor(&self) -> &BTreeMap<BlockId, Vec<Lamp>> {
        &self.outdoor
    }

    /// Keep the outdoor placements' lights of the blocks the scene's changes bring, and let go
    /// of the ones of the blocks they take away.
    pub fn take(&mut self, deltas: &[SceneDelta]) {
        for d in deltas {
            match d {
                SceneDelta::BlockIn(b) | SceneDelta::BlockChanged(b) => {
                    if b.lamps.is_empty() {
                        self.outdoor.remove(&b.block);
                    } else {
                        self.outdoor.insert(b.block, b.lamps.clone());
                    }
                }
                SceneDelta::BlockOut(id) => {
                    self.outdoor.remove(id);
                }
                _ => {}
            }
        }
    }

    /// The frame's lamps listed, their light's target made at the world's size and their rows
    /// written; `None` when the lamps light nothing this frame.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn prepare(
        &mut self,
        gfx: &mut Device,
        inputs: &LampInputs<'_>,
    ) -> Result<Option<LampsPlan>, dereth_gfx::Error> {
        self.last_count = 0;
        #[cfg(any(test, feature = "test-support"))]
        {
            self.last = None;
        }
        if !self.settings.any() {
            return Ok(None);
        }
        let frame = inputs.frame;
        // The flicker follows the scene's clock, so the same frames flicker alike however fast
        // they are drawn. A clock that went back (a new scene) starts it again.
        let from = match self.lit_from {
            Some(s) if s <= frame.clock => s,
            _ => frame.clock,
        };
        self.lit_from = Some(from);
        #[allow(clippy::cast_possible_truncation)] // seconds since the lamps were first lit
        let (lamps, halos) = lamps_of(
            frame,
            &self.outdoor,
            self.settings,
            &mut self.calibration,
            (frame.clock - from) as f32,
        );
        let t = inputs.targets;
        let light = match self.light {
            Some((tex, index, w, h)) if (w, h) == (t.width, t.height) => (tex, index),
            old => {
                if let Some((tex, ..)) = old {
                    gfx.destroy_texture(tex);
                }
                let tex = gfx.create_texture(&TextureDesc {
                    label: "remaster lamps' light",
                    width: t.width,
                    height: t.height,
                    mips: 1,
                    format: LAMP_LIGHT,
                    usage: TextureUsage::COLOR_TARGET
                        | TextureUsage::SAMPLED
                        | TextureUsage::TRANSFER_SRC,
                })?;
                let index = gfx.bind_texture(tex)?;
                self.light = Some((tex, index, t.width, t.height));
                (tex, index)
            }
        };
        self.lamps.write(gfx, &lamp_rows(&lamps))?;
        let view = View {
            view: frame.camera.view,
            projection: frame.camera.projection,
            extent: (t.width, t.height),
        };
        self.rows
            .write(gfx, &frame_rows(&view, frame, self.settings))?;
        let count = u32::try_from(lamps.len()).unwrap_or(u32::MAX);
        self.last_count = count;
        #[cfg(any(test, feature = "test-support"))]
        self.last_halos.clone_from(&halos);
        let halos = if halos.is_empty() {
            None
        } else {
            self.halos
                .write(gfx, &halo_rows(frame.camera.view, &halos))?;
            let format = if inputs.linear {
                super::post::scene_format(gfx)
            } else {
                super::COLOR
            };
            self.halo.iter().find(|(f, _)| *f == format).map(|(_, p)| {
                (
                    self.halos.index,
                    u32::try_from(halos.len()).unwrap_or(0),
                    *p,
                )
            })
        };
        #[cfg(any(test, feature = "test-support"))]
        {
            self.last = Some((lamps, view));
        }
        Ok(Some(LampsPlan {
            light: light.0,
            light_index: light.1,
            lamps: self.lamps.buffer,
            lamps_index: self.lamps.index,
            pieces: self.pieces,
            pieces_index: self.pieces_index,
            rows: self.rows.index,
            count,
            inside: frame.landscape && !frame.light.outdoor,
            clusters: self.clusters,
            shade: self.shade,
            halos,
        }))
    }

    /// Record the lamps: their lists, then their light, from the G-buffer's targets `t`. It goes
    /// after the G-buffer and the indoor stamps, before the resolve.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn record(
        plan: &LampsPlan,
        r: &mut Recorder<'_>,
        t: &Targets,
    ) -> Result<(), dereth_gfx::Error> {
        r.timestamp("lamp lists");
        r.compute(
            &[
                Touch::Buffer(plan.lamps, Use::StorageRead),
                Touch::Buffer(plan.pieces, Use::StorageWrite),
            ],
            |c| {
                c.set_pipeline(plan.clusters)?;
                c.push(
                    0,
                    &push_words(&[plan.rows, plan.lamps_index, plan.count, plan.pieces_index]),
                )?;
                c.dispatch((GRID[0] * GRID[1] * GRID[2]).div_ceil(64), 1, 1);
                Ok(())
            },
        )?;
        let outdoor_depth = if plan.inside {
            t.outdoor_depth
        } else {
            t.depth
        };
        let outdoor_depth_index = if plan.inside { t.index[6] } else { t.index[4] };
        let words = push_words(&[
            plan.rows,
            plan.lamps_index,
            plan.pieces_index,
            t.index[1],
            t.index[3],
            t.index[4],
            outdoor_depth_index,
        ]);
        r.timestamp("lamps");
        r.raster(
            &RasterPass {
                name: "lamps",
                color: &[ColorAttachment {
                    texture: plan.light,
                    load: Load::Clear([0.0; 4]),
                }],
                depth: None,
                touches: &[
                    Touch::Buffer(plan.lamps, Use::StorageRead),
                    Touch::Buffer(plan.pieces, Use::StorageRead),
                    Touch::Texture(t.normal, Use::Sampled),
                    Touch::Texture(t.material, Use::Sampled),
                    Touch::Texture(t.depth, Use::Sampled),
                    Touch::Texture(outdoor_depth, Use::Sampled),
                ],
                area: Some(Rect {
                    x: 0,
                    y: 0,
                    width: t.width,
                    height: t.height,
                }),
            },
            |d| {
                d.set_pipeline(plan.shade)?;
                d.push(0, &words)?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )
    }

    /// Record the lamps' halos over the scene's colors, for frame constants `frame`. It goes after
    /// everything the world drew, and the depth tells what stands in front of each.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn record_halos(
        plan: &LampsPlan,
        r: &mut Recorder<'_>,
        t: &Targets,
        frame: u32,
    ) -> Result<(), dereth_gfx::Error> {
        let Some((rows, count, pipeline)) = plan.halos else {
            return Ok(());
        };
        let mut touches = vec![Touch::Texture(t.depth, Use::Sampled)];
        if plan.inside {
            touches.push(Touch::Texture(t.outdoor_depth, Use::Sampled));
        }
        r.timestamp("lamp halos");
        let words = push_words(&[
            frame,
            rows,
            t.index[4],
            if plan.inside { t.index[6] } else { t.index[4] },
        ]);
        r.raster(
            &RasterPass {
                name: "lamp halos",
                color: &[ColorAttachment {
                    texture: t.color,
                    load: Load::Keep,
                }],
                depth: None,
                touches: &touches,
                area: Some(Rect {
                    x: 0,
                    y: 0,
                    width: t.width,
                    height: t.height,
                }),
            },
            |d| {
                d.set_pipeline(pipeline)?;
                d.push(0, &words)?;
                d.draw(6, count, 0, 0);
                Ok(())
            },
        )
    }

    /// The last frame's lamps' light and lamp lists, read back once the device has finished
    /// them: the light as four half floats a pixel, and each piece's lamps.
    #[cfg(any(test, feature = "test-support"))]
    pub fn read_back(&self, gfx: &mut Device) -> Option<(Vec<u8>, Vec<Vec<u32>>)> {
        let (tex, ..) = self.light?;
        gfx.wait_idle().ok()?;
        let light = gfx.read_texture(tex, 0).ok()?;
        let readback = gfx
            .create_buffer(&BufferDesc {
                label: "remaster lamp pieces read back",
                size: PIECES_BYTES,
                usage: BufferUsage::TRANSFER_DST,
                location: Location::Readback,
            })
            .ok()?;
        let bytes = (|| {
            let mut r = gfx.begin()?;
            r.copy_buffer(self.pieces, 0, readback, 0, PIECES_BYTES)?;
            r.touch(&[Touch::Buffer(readback, Use::HostRead)])?;
            let ticket = r.submit()?;
            gfx.wait(ticket)?;
            gfx.read_buffer(readback, 0, PIECES_BYTES)
        })();
        gfx.destroy_buffer(readback);
        let words: Vec<u32> = bytes
            .ok()?
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| u32::from_le_bytes(*w))
            .collect();
        let lists = words
            .chunks(PIECE_WORDS as usize)
            .map(|p| {
                let n = (p[0] as usize).min(PER_PIECE as usize);
                p[1..=n].to_vec()
            })
            .collect();
        Some((light, lists))
    }
}

impl super::Renderer {
    /// The lamps' pass.
    #[must_use]
    pub fn lamps(&self) -> &LampsPass {
        &self.lamps
    }

    /// What the lamps light, from the next frame on.
    pub fn set_lamp_settings(&mut self, settings: LampSettings) {
        self.lamps.set_settings(settings);
    }

    /// The last frame's lamps' light and lamp lists, read back: see [`LampsPass::read_back`].
    #[cfg(any(test, feature = "test-support"))]
    pub fn lamp_read_back(&mut self) -> Option<(Vec<u8>, Vec<Vec<u32>>)> {
        self.lamps.read_back(&mut self.gfx)
    }
}

/// The pass's own constants, as float4 rows: the view, the projection's inverse and the view and
/// projection's inverse (four columns each), the eye, the world's size and its tiles' size, the
/// slices' near and far and how many there are, what the lamps light, and (as words) the grid
/// and how many lamps a piece lists.
fn frame_rows(v: &View, frame: &FrameDelta, settings: LampSettings) -> Vec<u8> {
    let mut out = Vec::new();
    let mut f = |x: [f32; 4]| {
        for w in x {
            out.extend_from_slice(&w.to_le_bytes());
        }
    };
    for m in [
        v.view,
        v.projection.inverse(),
        (v.projection * v.view).inverse(),
    ] {
        for c in m.to_cols_array_2d() {
            f(c);
        }
    }
    let e = frame.camera.eye;
    f([e.x, e.y, e.z, 1.0]);
    let (tw, th) = v.tile();
    #[allow(clippy::cast_precision_loss)] // pixel counts
    f([v.extent.0 as f32, v.extent.1 as f32, tw as f32, th as f32]);
    #[allow(clippy::cast_precision_loss)] // a few dozen slices
    f([SLICE_NEAR, SLICE_FAR, GRID[2] as f32, SOFTEN]);
    f([
        if settings.indoor { 1.0 } else { 0.0 },
        if settings.outdoor { 1.0 } else { 0.0 },
        0.0,
        0.0,
    ]);
    for w in [GRID[0], GRID[1], GRID[2], PER_PIECE] {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;
    use crate::scene::Light;

    /// The brightness a lamp of `power` and `reach` gives the surfaces round it, as
    /// [`calibrate`] weighs them.
    fn given(power: f32, reach: f32, wrapped: bool) -> f32 {
        let mut s = 0.0;
        for i in 0..400 {
            let d = (i as f32 + 0.5) * reach / 400.0;
            for j in 0..100 {
                let mu = -1.0 + (j as f32 + 0.5) * 0.02;
                s += (power * turn(mu, wrapped) * falloff(d, reach)).min(1.0) * d * d;
            }
        }
        s
    }

    fn wanted(f: impl Fn(f32, f32) -> f32, reach: f32) -> f32 {
        let mut s = 0.0;
        for i in 0..400 {
            let d = (i as f32 + 0.5) * reach / 400.0;
            for j in 0..100 {
                let mu = -1.0 + (j as f32 + 0.5) * 0.02;
                s += f(d, mu) * d * d;
            }
        }
        s
    }

    #[test]
    fn a_calibrated_lamp_gives_the_surfaces_round_it_retails_brightness() {
        for (intensity, fall) in [(100.0, 3.0), (100.0, 10.0), (2.25, 10.0), (0.5, 4.0)] {
            for (name, f, reach, wrapped) in [
                (
                    "burn",
                    retail_burn as fn(f32, f32, f32, f32) -> f32,
                    fall * 1.3,
                    true,
                ),
                ("point", retail_point, fall * 1.5, false),
            ] {
                let power = calibrate(|d, mu| f(d, mu, intensity, fall), reach, wrapped);
                let want = wanted(|d, mu| f(d, mu, intensity, fall), reach);
                let got = given(power, reach, wrapped);
                assert!(
                    (got - want).abs() <= want * 0.02,
                    "{name} {intensity} {fall}: {got} for {want}"
                );
            }
        }
    }

    #[test]
    fn a_moving_lamp_is_retails_light_faded_out_at_its_reach() {
        assert_eq!(falloff_moving(15.0, 15.0), 0.0);
        assert!(falloff_moving(14.99, 15.0) < 1e-3);
        for d in [0.5f32, 1.0, 2.0, 4.0, 8.0, 13.4] {
            let want = retail_point(d, 1.0, 0.1, 10.0);
            let got = 0.1 * falloff_moving(d, 15.0);
            assert!((got - want).abs() <= want * 1e-5, "{d}: {got} for {want}");
        }
        assert!((falloff_moving(14.25, 15.0) * 14.25 - 0.5).abs() < 1e-4);
    }

    #[test]
    fn a_lamp_ends_smoothly_at_its_reach_and_falls_with_the_square_nearer() {
        assert_eq!(falloff(10.0, 10.0), 0.0);
        assert!(falloff(9.99, 10.0) < 1e-5);
        let ratio = falloff(1.0, 100.0) / falloff(2.0, 100.0);
        assert!((ratio - (4.25 / 1.25)).abs() < 0.01, "{ratio}");
        assert!(falloff(0.0, 10.0) <= 1.0 / (SOFTEN * SOFTEN) + 1e-6);
    }

    #[test]
    fn retails_burn_is_flat_inside_a_metre_and_capped_at_the_lights_color() {
        assert_eq!(retail_burn(0.5, 1.0, 100.0, 10.0), 1.0);
        assert_eq!(retail_burn(13.0, 1.0, 100.0, 10.0), 0.0);
        assert_eq!(retail_burn(2.0, -0.5, 100.0, 10.0), 0.0);
        let a = retail_burn(5.0, 1.0, 1.0, 10.0);
        let want = (1.0 - 5.0 / 13.0) * 1.5 * (2.0 / 3.0) / 25.0;
        assert!((a - want).abs() < 1e-6, "{a} {want}");
        assert_eq!(retail_point(16.0, 1.0, 100.0, 10.0), 0.0);
        assert!((retail_point(4.0, 0.5, 2.0, 10.0) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn a_wrapped_turn_lights_to_120_degrees_and_a_plain_one_to_90() {
        assert!((turn(1.0, true) - 1.0).abs() < 1e-6);
        assert!((turn(0.0, true) - 1.0 / 3.0).abs() < 1e-6);
        assert_eq!(turn(-0.5, true), 0.0);
        assert_eq!(turn(-0.9, true), 0.0);
        assert_eq!(turn(1.0, false), 1.0);
        assert_eq!(turn(0.0, false), 0.0);
        assert_eq!(turn(-0.3, false), 0.0);
    }

    #[test]
    fn the_lamps_come_on_as_the_sun_goes_down() {
        let mut l = crate::scene::RetailLight {
            sun: Vec3::new(0.0, 1.0, 1.0),
            ..Default::default()
        };
        assert_eq!(night(&l), 0.0);
        l.sun = Vec3::new(0.0, 1.0, 0.0);
        assert_eq!(night(&l), 1.0);
        l.sun = Vec3::ZERO;
        assert_eq!(night(&l), 1.0);
        l.sun = Vec3::new(0.0, 0.995, 0.1);
        let n = night(&l);
        assert!(n > 0.0 && n < 1.0, "{n}");
    }

    fn view() -> View {
        View {
            view: Mat4::IDENTITY,
            projection: Mat4::perspective_lh(1.0, 16.0 / 9.0, 0.25, 1000.0),
            extent: (1600, 900),
        }
    }

    fn lamp(at: Vec3, reach: f32) -> GpuLamp {
        GpuLamp {
            position: at,
            reach,
            object_reach: reach,
            color: [1.0; 3],
            indoor: true,
            burned: true,
            walls: 1.0,
            objects: 1.0,
            outdoors: 0.0,
            glow_reach: 0.0,
            glow: 0.0,
        }
    }

    #[test]
    fn a_lamp_is_listed_in_every_piece_its_reach_meets_and_in_no_other() {
        let v = view();
        // A lamp 10 m straight ahead, reaching 2 m: the middle tiles, the slices round 10 m.
        let lists = clusters(&[lamp(Vec3::new(0.0, 0.0, 10.0), 2.0)], &v);
        let listed: Vec<(u32, u32, u32)> = lists
            .iter()
            .enumerate()
            .filter(|(_, l)| !l.is_empty())
            .map(|(n, _)| {
                let n = u32::try_from(n).unwrap();
                (
                    n % GRID[0],
                    (n / GRID[0]) % GRID[1],
                    n / (GRID[0] * GRID[1]),
                )
            })
            .collect();
        for (i, j) in [(7, 4), (8, 4)] {
            assert!(
                listed.iter().any(|&(a, b, _)| (a, b) == (i, j)),
                "tile {i} {j}"
            );
        }
        for &(i, j, k) in &listed {
            let (lo, hi) = v.bounds(i, j, k);
            assert!(meets(Vec3::new(0.0, 0.0, 10.0), 2.0, lo, hi));
            assert!(View::slice_depth(k + 1) >= 8.0 && View::slice_depth(k) <= 12.0);
            assert!((3..=12).contains(&i) && (1..=7).contains(&j), "{i} {j}");
        }
        // Behind the eye: listed nowhere.
        let behind = clusters(&[lamp(Vec3::new(0.0, 0.0, -10.0), 2.0)], &v);
        assert!(behind.iter().all(Vec::is_empty));
    }

    #[test]
    fn a_piece_lists_at_most_its_share_and_keeps_the_first() {
        let v = view();
        let many: Vec<GpuLamp> = (0..100)
            .map(|_| lamp(Vec3::new(0.0, 0.0, 10.0), 50.0))
            .collect();
        let lists = clusters(&many, &v);
        let full = lists.iter().find(|l| !l.is_empty()).expect("listed");
        assert_eq!(full.len(), PER_PIECE as usize);
        assert_eq!(full[0], 0);
        assert_eq!(full[full.len() - 1], PER_PIECE - 1);
    }

    #[test]
    fn the_indoors_takes_the_pools_and_the_outdoors_its_lamps_only_at_night() {
        let mut frame = FrameDelta::default();
        frame.light.lights = vec![
            Light {
                kind: LightKind::Point,
                cell: 0x0100_0100,
                position: Vec3::new(1.0, 2.0, 3.0),
                direction: Vec3::ZERO,
                color: [1.0, 0.5, 0.25],
                intensity: 100.0,
                falloff: 4.0,
                cone_angle: 0.0,
            },
            Light {
                kind: LightKind::Point,
                cell: 0x0100_0100,
                position: Vec3::new(0.0, 0.0, 2.0),
                direction: Vec3::ZERO,
                color: [1.0; 3],
                intensity: 2.25,
                falloff: 10.0,
                cone_angle: 0.0,
            },
        ];
        frame.light.statics = 1;
        frame.light.sun = Vec3::new(0.0, 1.0, 1.0);
        frame.origin = [0xA9, 0xB4];
        let mut outdoor = BTreeMap::new();
        outdoor.insert(
            crate::scene::block_id(0xAA, 0xB4),
            vec![Lamp {
                source: LampSource::Authored,
                flicker: false,
                seed: 0,
                position: Vec3::new(10.0, 10.0, 4.0),
                lifts: [0.0; 2],
                color: [1.0, 0.8, 0.5],
                intensity: 1.0,
                falloff: 6.0,
            }],
        );
        let mut c = Calibration::default();
        let all = LampSettings {
            indoor: true,
            outdoor: true,
        };
        let (day, no_halos) = lamps_of(&frame, &outdoor, all, &mut c, 0.0);
        assert!(no_halos.is_empty(), "no halo by day");
        assert_eq!(day.len(), 2, "by day, the pools alone");
        let burn = calibrate(|d, mu| retail_burn(d, mu, 100.0, 4.0), 5.2, true);
        assert_eq!(
            (day[0].walls, day[0].objects, day[1].walls, day[1].objects),
            (100.0, burn, 2.25, 2.25),
            "on walls each light's intensity; on objects a static light by its burn"
        );
        assert_eq!(
            (
                day[0].reach,
                day[0].object_reach,
                day[1].reach,
                day[1].object_reach
            ),
            (5.2, 4.0, 15.0, 10.0),
            "walls as far as retail lights them, objects as far as it chooses their lights"
        );
        assert!(day[0].burned && !day[1].burned);
        frame.light.sun = Vec3::ZERO;
        frame.camera.eye = Vec3::new(190.0, 0.0, 2.0);
        let (night, _) = lamps_of(&frame, &outdoor, all, &mut c, 0.0);
        assert_eq!(night.len(), 3);
        let o = night[2];
        assert!(!o.indoor && o.outdoors > 0.0);
        assert_eq!(
            o.position,
            Vec3::new(202.0, 10.0, 4.0),
            "the block's offset"
        );
        assert!(lamps_of(&frame, &outdoor, LampSettings::OFF, &mut c, 0.0)
            .0
            .is_empty());
        assert_eq!(c.len(), 2, "the indoor lights alone are calibrated");
    }

    fn street(lamps: Vec<Lamp>) -> (FrameDelta, BTreeMap<BlockId, Vec<Lamp>>) {
        let frame = FrameDelta {
            landscape: true,
            origin: [0xA9, 0xB4],
            ..FrameDelta::default()
        };
        let mut outdoor = BTreeMap::new();
        outdoor.insert(crate::scene::block_id(0xA9, 0xB4), lamps);
        (frame, outdoor)
    }

    fn at(position: Vec3, source: LampSource) -> Lamp {
        Lamp {
            source,
            flicker: false,
            seed: 0,
            position,
            lifts: [0.0; 2],
            color: [1.0, 0.9, 0.7],
            intensity: 100.0,
            falloff: 6.0,
        }
    }

    const OUTDOORS: LampSettings = LampSettings {
        indoor: false,
        outdoor: true,
    };

    /// A placement's lamp at night glows and stands a halo, and a flame's is smaller than a lamp's;
    /// a moving light does neither.
    #[test]
    fn a_placements_lamp_glows_and_stands_a_halo_at_night() {
        let (mut frame, outdoor) = street(vec![
            at(Vec3::new(5.0, 0.0, 2.0), LampSource::Authored),
            at(Vec3::new(50.0, 0.0, 2.0), LampSource::Flame),
        ]);
        frame.lamps = vec![Light {
            kind: LightKind::Point,
            cell: 0,
            position: Vec3::new(20.0, 0.0, 2.0),
            direction: Vec3::ZERO,
            color: [1.0; 3],
            intensity: 1.0,
            falloff: 5.0,
            cone_angle: 0.0,
        }];
        let mut c = Calibration::default();
        let (lamps, halos) = lamps_of(&frame, &outdoor, OUTDOORS, &mut c, 0.0);
        assert_eq!(lamps.len(), 3);
        assert_eq!(halos.len(), 2, "the placements' lamps alone");
        assert_eq!((halos[0].size, halos[1].size), (1.0, 0.75));
        assert_eq!(lamps[0].glow, 1.0);
        assert_eq!(lamps[1].glow, 0.0, "the moving light, second nearest");
        assert_eq!((lamps[0].glow_reach, lamps[2].glow_reach), (0.32, 0.25));
    }

    /// Two lights standing in one place are one lamp, and past its reach a lamp stands no halo.
    #[test]
    fn lamps_in_one_place_are_one_and_a_far_lamp_stands_no_halo() {
        let (frame, outdoor) = street(vec![
            at(Vec3::new(5.0, 0.0, 2.0), LampSource::Authored),
            at(Vec3::new(5.3, 0.0, 2.0), LampSource::Flame),
            at(Vec3::new(150.0, 0.0, 2.0), LampSource::Authored),
        ]);
        let mut c = Calibration::default();
        let (lamps, halos) = lamps_of(&frame, &outdoor, OUTDOORS, &mut c, 0.0);
        assert_eq!(lamps.len(), 2);
        assert_eq!(halos.len(), 1);
    }

    /// Only a flame flickers, the same however fast it is drawn, and its pool and halo with it.
    #[test]
    fn a_flame_flickers_with_the_clock_and_a_lamp_holds_steady() {
        let mut flame = at(Vec3::new(5.0, 0.0, 2.0), LampSource::Flame);
        flame.flicker = true;
        let steady = at(Vec3::new(40.0, 0.0, 2.0), LampSource::Authored);
        let (frame, outdoor) = street(vec![flame, steady]);
        let mut c = Calibration::default();
        let (a, ha) = lamps_of(&frame, &outdoor, OUTDOORS, &mut c, 1.0);
        let (b, hb) = lamps_of(&frame, &outdoor, OUTDOORS, &mut c, 1.3);
        assert_ne!(a[0].outdoors, b[0].outdoors, "the flame");
        assert_ne!(ha[0].color, hb[0].color, "its halo");
        assert_eq!(a[1].outdoors, b[1].outdoors, "the lamp");
        let (again, _) = lamps_of(&frame, &outdoor, OUTDOORS, &mut c, 1.0);
        assert_eq!(again[0].outdoors, a[0].outdoors, "by the clock");
    }

    /// A lamp stands as high as its placement is drawn on the ground.
    #[test]
    fn a_lamp_stands_as_high_as_its_placement_is_lifted() {
        let mut lamp = at(Vec3::new(5.0, 0.0, 2.0), LampSource::Authored);
        lamp.lifts = [0.75; 2];
        let (frame, outdoor) = street(vec![lamp]);
        let mut c = Calibration::default();
        let (lamps, halos) = lamps_of(&frame, &outdoor, OUTDOORS, &mut c, 0.0);
        assert_eq!(lamps[0].position.z, 2.75);
        assert_eq!(halos[0].position.z, 2.75);
    }

    /// A flame of `falloff` and its own number `seed`, its brightness every `step` seconds.
    fn burning(seed: u32, falloff: f32, step: f32, n: usize) -> Vec<f32> {
        let mut lamp = at(Vec3::ZERO, LampSource::Flame);
        lamp.flicker = true;
        lamp.seed = seed;
        lamp.falloff = falloff;
        (0..n)
            .map(|k| flicker(&lamp, k as f32 * step).brightness)
            .collect()
    }

    fn mean(v: &[f32]) -> f32 {
        v.iter().sum::<f32>() / v.len() as f32
    }

    /// A flame never goes out nor flares past what reads as a flame, and from one frame to the
    /// next at 60 a second it never steps.
    #[test]
    fn a_flame_stays_a_flame_and_never_steps_between_frames() {
        for seed in 0..32 {
            for falloff in [1.0, 6.0, 30.0] {
                let v = burning(seed * 7_919, falloff, 1.0 / 60.0, 3_600);
                for &k in &v {
                    assert!((0.55..=1.35).contains(&k), "{seed} {falloff}: {k}");
                }
                let step = v
                    .windows(2)
                    .map(|w| (w[1] - w[0]).abs())
                    .fold(0.0f32, f32::max);
                assert!(step < 0.1, "{seed} {falloff}: {step} in a frame");
            }
        }
    }

    /// How often a flame of `falloff` crosses its middle, times a second, over many flames.
    fn pace(falloff: f32) -> f32 {
        let mut crossings = 0;
        for seed in 0..100 {
            let v = burning(seed * 131 + 3, falloff, 0.01, 2_000);
            let m = mean(&v);
            crossings += v
                .windows(2)
                .filter(|w| (w[0] - m) * (w[1] - m) < 0.0)
                .count();
        }
        crossings as f32 / (100.0 * 20.0)
    }

    /// A small flame dances faster than a large one: a candle's light quicker than a torch's, a
    /// torch's than a bonfire's.
    #[test]
    fn a_small_flame_dances_faster_than_a_large_one() {
        let (candle, torch, bonfire) = (pace(1.5), pace(12.0), pace(37.5));
        assert!(candle > torch * 1.3, "{candle} against {torch}");
        assert!(torch > bonfire * 1.3, "{torch} against {bonfire}");
    }

    /// Two flames don't move together, one flame doesn't repeat itself, and the same clock gives
    /// the same flame.
    #[test]
    fn two_flames_move_apart_and_neither_repeats() {
        let (a, b) = (burning(1, 6.0, 0.02, 2_000), burning(2, 6.0, 0.02, 2_000));
        let (ma, mb) = (mean(&a), mean(&b));
        let var = |v: &[f32], m: f32| v.iter().map(|x| (x - m) * (x - m)).sum::<f32>();
        let r = a
            .iter()
            .zip(&b)
            .map(|(x, y)| (x - ma) * (y - mb))
            .sum::<f32>()
            / (var(&a, ma) * var(&b, mb)).sqrt();
        assert!(r.abs() < 0.3, "the two move together: {r}");
        let v = burning(77, 6.0, 0.02, 12_000);
        let m = mean(&v);
        let whole = var(&v, m);
        for lag in (150..6_000).step_by(29) {
            let c: f32 = (0..v.len() - lag)
                .map(|i| (v[i] - m) * (v[i + lag] - m))
                .sum();
            assert!(c / whole < 0.45, "it comes round again {lag} steps on");
        }
        let mut lamp = at(Vec3::ZERO, LampSource::Flame);
        lamp.flicker = true;
        assert_eq!(flicker(&lamp, 321.5), flicker(&lamp, 321.5));
    }

    /// As a flame dims its light goes redder and as it brightens yellower; its light sways a few
    /// centimeters, where a steady lamp's holds still.
    #[test]
    fn a_dimmer_flame_is_redder_and_its_light_sways_a_little() {
        let mut lamp = at(Vec3::ZERO, LampSource::Flame);
        lamp.flicker = true;
        let mut dim = 0;
        let mut bright = 0;
        for k in 0..4_000 {
            let f = flicker(&lamp, k as f32 * 0.013);
            assert!(f.sway.length() < 0.1, "{:?}", f.sway);
            if f.brightness < 0.95 {
                dim += 1;
                assert!(f.tint[2] < f.tint[1] && f.tint[1] < 1.0, "{f:?}");
            } else if f.brightness > 1.05 {
                bright += 1;
                assert!(f.tint[2] > f.tint[1] && f.tint[1] > 1.0, "{f:?}");
            }
        }
        assert!(dim > 100 && bright > 100, "{dim} dim and {bright} bright");
        let steady = at(Vec3::ZERO, LampSource::Authored);
        assert_eq!(flicker(&steady, 5.0), Flame::STEADY);
    }
}
