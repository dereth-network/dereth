//! Per-pixel sun and sky light, sun shadows, point lights, high dynamic range, exposure, bloom
//! and tone mapping.
//!
//! Two passes share the work:
//!
//! - [`Lighting`], over the opaque world: it draws the sun's shadow cascades from the retained
//!   landscape and static geometry of every resident block (so hills and houses off screen still
//!   cast), then lights every re-shaded pixel again. The vertex light each surface was drawn with
//!   is divided out of its colour, and the sun (shadowed through soft cascades and short
//!   screen-space contact rays, which catch the characters), a sky hemisphere and the scene's
//!   point lights are put back, in linear high dynamic range.
//! - [`LightingPost`], at the high dynamic range slot: bloom, exposure, a filmic tone map (ACES,
//!   or AgX) and a light grade, written into the picture.

pub mod cells;
pub mod derive;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::{Mat4, Vec3, Vec4, Vec4Swizzles};

use crate::build::Build;
use crate::build::Prebuilt;
use crate::graph::WarmCx;
use crate::graph::{EncodeCx, HifiPass, PrepareCx};
use crate::shared::camera::{render_from_clip, HEIGHT_UP};
use crate::shared::fullscreen::COLOUR_WGSL;
use crate::shared::static_feed::{DeviceCopy, LevelRuns};
use crate::{Caps, DebugView, HifiError, HifiFrame, HifiSettings, Level};

const SHADOW_WGSL: &str = include_str!("shadow.wgsl");
const RELIGHT_WGSL: &str = include_str!("relight.wgsl");
const POST_WGSL: &str = include_str!("post.wgsl");
const EXPOSURE_WGSL: &str = include_str!("exposure.wgsl");
const TONE_WGSL: &str = include_str!("tone.wgsl");

const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub(crate) const CASCADES: usize = 4;
/// [`CASCADES`], as the shadow map's layer count.
const CASCADE_LAYERS: u32 = 4;
/// The far distance of each cascade, metres.
pub(crate) const SPLITS: [f32; CASCADES] = [14.0, 45.0, 150.0, 560.0];
/// How far toward the sun casters are looked for beyond a cascade's own sphere, metres.
const CASTER_REACH: f32 = 900.0;
/// How many point lights the relight takes: the length of its list of them.
pub const MAX_LIGHTS: usize = 128;
const CASTER_STRIDE: u64 = 256;
const BLOOM_LEVELS: u32 = 6;
/// The share of the sun a leaf of the world's own trees keeps in its plant's shadow. Those trees
/// are a few crossed cards, whose shadows hold none of the gaps a crown has between its leaves:
/// without it their canopies go black in patches. (An art pack's leaves take theirs from the
/// light the scene tunes for its art, `HifiFrame::pack_light`.)
const LEAF_SHADOW_FLOOR: f32 = 0.4;

pub(crate) fn env_f32(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

pub(crate) fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|f| f.to_le_bytes()).collect()
}

pub(crate) fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        dereth_primitives::num::math::powf((c + 0.055) / 1.055, 2.4)
    }
}

/// The sun and ambient light as the vertex light took them, 0..1 per channel.
/// The sun's colour, linear, at a brightness of one: the tint a white balance against the sun
/// divides the picture by (`DERETH_HIFI_WHITE_BALANCE`). White for a sun with no colour.
#[must_use]
pub fn sun_tint(frame: &HifiFrame) -> [f32; 3] {
    let linear = frame.sky.sun_color.map(|c| {
        let s = f32::from(c) / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            dereth_primitives::num::math::powf((s + 0.055) / 1.055, 2.4)
        }
    });
    let l = 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
    if l <= 1e-4 {
        return [1.0; 3];
    }
    linear.map(|c| c / l)
}

pub(crate) fn legacy_light(frame: &HifiFrame) -> ([f32; 3], [f32; 3]) {
    let sky = &frame.sky;
    let sun = sky
        .sun_color
        .map(|c| f32::from(c) / 255.0 * sky.sun_brightness);
    let amb = sky
        .ambient_color
        .map(|c| f32::from(c) / 255.0 * sky.ambient_level.max(0.2));
    (sun, amb)
}

/// How much of the sun's brightness the ambient light the world's objects are drawn with outdoors
/// adds to the landscape's level.
const OBJECT_AMBIENT_FROM_SUN: f32 = 0.2;

/// The ambient light the world's objects outdoors are drawn with, 0..1 per channel: the
/// landscape's level and a fifth of the sun's brightness more, in the ambient colour. The light
/// per pixel takes the landscape's ambient out of every surface ([`legacy_light`]), so an object
/// keeps that fifth and comes out brighter than its own colour under the new light. An art pack's
/// surface drawn with this ambient comes out as bright as the object it replaces.
pub(crate) fn object_ambient(frame: &HifiFrame) -> [f32; 3] {
    let sky = &frame.sky;
    let level = sky.ambient_level + OBJECT_AMBIENT_FROM_SUN * sky.sun_brightness;
    sky.ambient_color.map(|c| f32::from(c) / 255.0 * level)
}

/// The point lights the relight puts on the frame's surfaces at `gain`: the nearest
/// [`MAX_LIGHTS`] that reach within 250 metres of the eye, leaving out the copies of the lights
/// burned into the rooms that the rooms' own light per pixel takes, those inside a building's or a dungeon's
/// cells first, then the rest, each kind nearest first. None at no gain, so no pixel walks them.
#[must_use]
pub fn point_lights(frame: &HifiFrame, gain: f32) -> Vec<&crate::snapshot::HifiLight> {
    if gain <= 0.0 {
        return Vec::new();
    }
    let eye = frame.camera.eye;
    let mut lights: Vec<&crate::snapshot::HifiLight> = frame
        .lights
        .iter()
        .filter(|l| !l.burned && l.falloff > 0.0 && (l.position - eye).length() - l.falloff < 250.0)
        .collect();
    lights.sort_by(|a, b| {
        (a.position - eye)
            .length()
            .total_cmp(&(b.position - eye).length())
    });
    lights.truncate(MAX_LIGHTS);
    // Stable: each kind stays nearest first.
    lights.sort_by_key(|l| !l.interior);
    lights
}

/// Whether the sun is up far enough to shade and shadow what the frame shows of the outdoors.
pub(crate) fn sun_up(frame: &HifiFrame) -> bool {
    frame.sky.sees_outside && frame.sky.sun_direction.z > 0.02 && frame.sky.sun_brightness > 0.02
}

/// How the day's light is shared between the sun and the sky. The authored sun shines as
/// brightly through a rainy day's cloud as on a clear one, and as near the horizon as overhead;
/// here the cloud the day's weather brings, a fog that closes in by day, and the long path of a
/// low sun through the air take light from the sun's beam, and the sky gives most of it back as
/// light from all round. Under a grey sky the sun's shadows soften and fade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DayLight {
    /// The share of the sun's light that reaches the ground in its beam: 1 under a clear sky
    /// with the sun high.
    pub sun: f32,
    /// How overcast the day is: 0 for a clear sky, 1 for one wholly grey.
    pub cover: f32,
    /// The sun's light the sky gives back from all round, as a share of the sun's own.
    pub scattered: f32,
}

/// How much of the sun's beam a wholly grey sky takes.
const COVER_TAKES: f32 = 0.85;
/// How much of the sun's beam the air takes from a sun at the horizon.
const LOW_SUN_TAKES: f32 = 0.3;
/// The height of the sun, as the vertical share of its direction, over which the air takes less
/// and less of its beam: all it takes below the first, none above the second (about two and
/// twenty degrees).
const LOW_SUN: (f32, f32) = (0.03, 0.35);
/// How much of the light taken from the beam the sky gives back: cloud scatters most of what it
/// takes.
const SCATTERED_BACK: f32 = 0.8;
/// How much of what the sky gives back comes from the bright cloud about the sun.
const ABOUT_THE_SUN: f32 = 0.5;

impl DayLight {
    /// The light of a frame under a clear sky with the sun high: the sun's beam whole.
    pub const CLEAR: Self = Self {
        sun: 1.0,
        cover: 0.0,
        scattered: 0.0,
    };

    /// The share of the day's light in `frame`: the cloud of its weather group, a fog that
    /// closes in by day, and the height of the sun.
    #[must_use]
    pub fn of(frame: &HifiFrame) -> Self {
        use crate::passes::atmosphere::model::Weather;
        let sky = &frame.sky;
        let elevation = sky.sun_direction.z;
        let weather = Weather::of(&sky.day_group).overcast() / Weather::Rainy.overcast();
        // A fog that closes in to a few hundred metres by day is a grey sky too; the evening's
        // own haze is not.
        let fog = sky.fog.map_or(0.0, |f| {
            (1.0 - smoothstep(250.0, 900.0, f.max)) * smoothstep(0.15, 0.35, elevation)
        });
        let cover = weather.max(fog).clamp(0.0, 1.0);
        let low = 1.0 - smoothstep(LOW_SUN.0, LOW_SUN.1, elevation);
        let sun = (1.0 - COVER_TAKES * cover) * (1.0 - LOW_SUN_TAKES * low);
        Self {
            sun,
            cover,
            scattered: (1.0 - sun) * SCATTERED_BACK,
        }
    }

    /// The sun's light the sky gives back, linear and grey, for a sun of linear colour `sun_lin`
    /// at `gain`: the share seen from all round, and the share from the bright cloud about the
    /// sun, which the things under it shade as they shade the sun.
    #[must_use]
    pub fn sky_light(&self, sun_lin: [f32; 3], gain: f32) -> (f32, f32) {
        let lum = 0.2126 * sun_lin[0] + 0.7152 * sun_lin[1] + 0.0722 * sun_lin[2];
        let given = lum * gain * self.scattered;
        (given * (1.0 - ABOUT_THE_SUN), given * ABOUT_THE_SUN)
    }

    /// How many times wider the sun's shadows soften than under a clear sky.
    #[must_use]
    pub fn penumbra(&self) -> f32 {
        1.0 + 5.0 * self.cover
    }
}

/// The exposure for the frame's light: a key of the sun and sky, held to a sane range so night
/// stays night and noon is not crushed.
#[must_use]
pub fn exposure(frame: &HifiFrame, sun_gain: f32, amb_gain: f32) -> f32 {
    let (sun, amb) = legacy_light(frame);
    let lum = |c: [f32; 3]| {
        let l = c.map(|x| dereth_primitives::num::math::powf(x.max(0.0), 2.2));
        0.2126 * l[0] + 0.7152 * l[1] + 0.0722 * l[2]
    };
    let s = if sun_up(frame) {
        lum(sun) * sun_gain * frame.sky.sun_direction.z.clamp(0.2, 1.0)
    } else {
        0.0
    };
    let key = s + lum(amb) * amb_gain;
    // A gentle lift where the light is dim, so the night reads, without turning it into day.
    // While the sun stands a few degrees over the horizon its own light is drawn as it is: the
    // lift fades out as it rises, so a low sun does not light the land far past the ordinary
    // picture.
    let lift = (0.35 / key.max(1e-3)).clamp(1.0, 1.6);
    let risen = smoothstep(
        LIFT_GONE_FROM.0,
        LIFT_GONE_FROM.1,
        if frame.sky.sees_outside {
            frame.sky.sun_direction.z
        } else {
            0.0
        },
    );
    env_f32("DERETH_HIFI_EXPOSURE", 1.8) * (lift + (1.0 - lift) * risen)
}

/// The height of the sun, as the vertical share of its direction, over which the night's lift of
/// the exposure fades out: whole below the first, gone above the second (about one and five
/// degrees).
const LIFT_GONE_FROM: (f32, f32) = (0.02, 0.08);

fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How the eye adapts to the light of a frame indoors: the exposure that draws a room as the
/// ordinary frame does, the brightness the frame's mean light is brought toward and how much of
/// the way, the least and the greatest exposure, the light the brightest share of the frame is
/// brought to at most, and the seconds it takes to adjust into the dark and into the light.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Adaptation {
    /// The exposure that draws a room of ordinary brightness about as bright as the ordinary
    /// frame draws it, the tone map's deeper shadows taken into account.
    pub base: f32,
    /// The brightness, linear, the frame's geometric mean light is brought toward.
    pub key: f32,
    /// How much of the way toward the key the exposure goes: 1 all of it, 0 none.
    pub strength: f32,
    /// The least exposure.
    pub least: f32,
    /// The greatest exposure.
    pub most: f32,
    /// The light, before the tone map, the brightest share of the frame is brought to at most:
    /// bright, with its detail kept, never white.
    pub white: f32,
    /// The seconds the eye takes to adjust to a darker frame (a brighter exposure).
    pub into_dark: f32,
    /// The seconds the eye takes to adjust to a brighter frame (a dimmer exposure).
    pub into_light: f32,
}

impl Adaptation {
    /// The adaptation indoors.
    #[must_use]
    pub fn indoors() -> Self {
        let base = env_f32("DERETH_HIFI_INDOOR_EXPOSURE", 2.6);
        Self {
            base,
            key: env_f32("DERETH_HIFI_INDOOR_KEY", 0.033),
            strength: env_f32("DERETH_HIFI_INDOOR_ADAPT", 0.4),
            least: base * 0.6,
            most: base * env_f32("DERETH_HIFI_INDOOR_MOST", 1.5),
            white: env_f32("DERETH_HIFI_INDOOR_WHITE", 2.5),
            into_dark: 1.6,
            into_light: 0.5,
        }
    }
}

/// How bright the outdoors is drawn against the ordinary picture of the same frame: its mean
/// brightness held between the least and the most share of the ordinary picture's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutdoorHold {
    /// The least share: one by day, so the light never dims the daylit world below the ordinary
    /// picture; none at night, which the light draws darker on purpose.
    pub least: f32,
    /// The most share: a little over one, so what the light adds in shade, contrast and color
    /// shows without the whole being drawn brighter.
    pub most: f32,
}

impl OutdoorHold {
    /// The hold for `frame`: by day once the sun stands a few degrees up, at night none from
    /// below.
    #[must_use]
    pub fn of(frame: &HifiFrame) -> Self {
        let risen = if frame.sky.sees_outside {
            smoothstep(
                LIFT_GONE_FROM.0,
                LIFT_GONE_FROM.1,
                frame.sky.sun_direction.z,
            )
        } else {
            0.0
        };
        Self {
            least: risen,
            most: env_f32("DERETH_HIFI_OUTDOOR_MOST", 1.08),
        }
    }
}

/// The exposure pass's constants that hold the outdoors to the ordinary picture: whether it is
/// held (`held`), the hold, the tone map's look, the authored fog the picture before the light
/// lacks under the physical sky (`sky_on`), the eye, and the way back from the depth to the world.
fn outdoor_constants(frame: &HifiFrame, sky_on: bool, held: bool) -> Vec<f32> {
    let hold = OutdoorHold::of(frame);
    let mut c = vec![
        f32::from(u8::from(held)),
        hold.least,
        hold.most,
        frame.pack_light.exposure,
        env_f32("DERETH_HIFI_TOE", 0.36),
        env_f32("DERETH_HIFI_CONTRAST", 1.04),
        env_f32("DERETH_HIFI_VIGNETTE", 0.28),
        0.0,
    ];
    // Under the physical sky the world is replayed without its authored fog; the ordinary
    // picture has it.
    match frame.sky.fog.filter(|_| sky_on && frame.sky.outdoor) {
        Some(fog) => {
            c.extend([fog.min, fog.max, 1.0, 0.0]);
            c.extend(fog.color.map(|v| f32::from(v) / 255.0));
            c.push(1.0);
        }
        None => c.extend([0.0; 8]),
    }
    let cam = &frame.camera;
    c.extend([cam.eye.x, cam.eye.y, cam.eye.z, 0.0]);
    c.extend(render_from_clip(cam.view, cam.projection).to_cols_array());
    let pack = &frame.pack_light;
    c.extend([pack.leaf_exposure, pack.ground_exposure, 0.0, 0.0]);
    c
}

/// How far past the sun and the sky's exposure a room that shows the outdoors is exposed at most.
const OUTDOORS_SHOWN_EXPOSURE: f32 = 1.25;

/// How long after the last frame indoors the eye still adjusts outdoors, rather than taking the
/// sun and sky's exposure at once, seconds.
const ADJUSTING_OUTDOORS: f32 = 3.0;
/// The longest gap between two frames over which the eye is taken to have kept its adaptation,
/// seconds; after a longer one it takes the frame's exposure at once.
const ADAPTATION_KEPT: f32 = 0.5;

/// The retained shadow geometry of one block: every detail level of its objects, and the lists
/// of the vertices its objects' current levels draw.
#[derive(Debug)]
struct CasterBlock {
    generation: u32,
    /// The landscape and the objects an art pack has no models for.
    opaque: Option<DeviceCopy>,
    /// How many of the opaque copy's drawn vertices, first, are the landscape's: left out while
    /// the drawn ground casts in their place.
    landscape: u32,
    /// The objects an art pack has models for, apart: they cast no shadow while the pack's
    /// models are drawn in their place.
    covered: Option<DeviceCopy>,
    /// Cut-out batches: positions and texture coordinates, the texture slot, and whether the
    /// batch holds objects an art pack has models for.
    cutouts: Vec<(DeviceCopy, u32, bool)>,
}

impl CasterBlock {
    /// List the vertices each copy draws while the block's objects stand at `levels`: how many
    /// copies' lists were made again, and how many were kept as they were.
    fn select(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, levels: &[u32]) -> (u64, u64) {
        let mut out = (0, 0);
        for copy in self
            .opaque
            .iter_mut()
            .chain(self.covered.iter_mut())
            .chain(self.cutouts.iter_mut().map(|(c, _, _)| c))
        {
            if copy.select(device, queue, levels) {
                out.0 += 1;
            } else {
                out.1 += 1;
            }
        }
        out
    }
}

/// Where a vertex format's first texture coordinates sit, bytes.
fn uv_offset(format: dereth_render::VertexFormat) -> usize {
    if crate::derive::reshade::has_normal(format) {
        28
    } else {
        16
    }
}

fn read_f32(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// A cut-out batch's positions and texture coordinates, five floats a vertex, every detail level
/// of its objects, and how they lie by level.
fn cutout_vertices(batch: &crate::snapshot::HifiStaticBatch) -> (Vec<f32>, LevelRuns) {
    let stride = batch.format.stride() as usize;
    let count = batch.bytes.len() / stride;
    let usable = count - count % 3;
    let uv = uv_offset(batch.format);
    let mut runs = LevelRuns::default();
    runs.batch(&batch.runs, u32::try_from(usable).unwrap_or(u32::MAX));
    let mut out = Vec::with_capacity(usable * 5);
    for v in 0..usable {
        let at = v * stride;
        out.extend_from_slice(&[
            read_f32(&batch.bytes, at),
            read_f32(&batch.bytes, at + 4),
            read_f32(&batch.bytes, at + 8),
            read_f32(&batch.bytes, at + uv),
            read_f32(&batch.bytes, at + uv + 4),
        ]);
    }
    (out, runs)
}

/// The vertex positions of a block's retained geometry, block-local, three per triangle: the
/// landscape and the batches that are not of objects an art pack has models for, or (`covered`)
/// those batches alone; every detail level of their objects, and how they lie by level.
fn caster_positions(
    block: &crate::snapshot::HifiStaticBlock,
    covered: bool,
) -> (Vec<f32>, LevelRuns) {
    let mut out = Vec::with_capacity(block.terrain_triangles.len() * 9);
    let mut runs = LevelRuns::default();
    if !covered {
        for t in &block.terrain_triangles {
            for p in t {
                out.extend_from_slice(&[p.x, p.y, p.z]);
            }
        }
        runs.every(u32::try_from(block.terrain_triangles.len() * 3).unwrap_or(u32::MAX));
    }
    for batch in &block.batches {
        if batch.format.is_pre_transformed()
            || (batch.cutout && batch.texture.is_some())
            || batch.art_pack != covered
        {
            continue;
        }
        let stride = batch.format.stride() as usize;
        let count = batch.bytes.len() / stride;
        let usable = count - count % 3;
        runs.batch(&batch.runs, u32::try_from(usable).unwrap_or(u32::MAX));
        for v in 0..usable {
            let at = v * stride;
            let f = |k: usize| {
                let b = &batch.bytes[at + k * 4..at + k * 4 + 4];
                f32::from_le_bytes([b[0], b[1], b[2], b[3]])
            };
            out.extend_from_slice(&[f(0), f(1), f(2)]);
        }
    }
    (out, runs)
}

#[derive(Debug)]
pub(crate) struct Cascades {
    pub(crate) matrices: [Mat4; CASCADES],
    pub(crate) texel: [f32; CASCADES],
    pub(crate) depth_range: [f32; CASCADES],
}

/// A block's side, metres.
const BLOCK_METRES: f32 = 192.0;
/// How far over a block's highest ground its casters may stand, metres: past the tallest tower.
const CASTER_HEADROOM: f32 = 250.0;
/// How far under its lowest ground its casters may reach, metres.
const CASTER_FOOTROOM: f32 = 50.0;

/// The lowest and highest of a block's drawn ground heights; nothing known is everything.
fn height_range(heights: &[f32]) -> (f32, f32) {
    heights
        .iter()
        .fold(None, |r: Option<(f32, f32)>, &h| {
            Some(r.map_or((h, h), |(lo, hi)| (lo.min(h), hi.max(h))))
        })
        .unwrap_or((-1.0e5, 1.0e5))
}

/// The box a block's shadow casters stand in, render space: its square, from under its lowest
/// ground to over its highest.
fn caster_bounds(origin: Vec3, (low, high): (f32, f32)) -> (Vec3, Vec3) {
    (
        Vec3::new(origin.x, origin.y, origin.z + low - CASTER_FOOTROOM),
        Vec3::new(
            origin.x + BLOCK_METRES,
            origin.y + BLOCK_METRES,
            origin.z + high + CASTER_HEADROOM,
        ),
    )
}

/// Whether anything in the box `lo..hi` falls inside cascade `m`'s map: a box wholly past one
/// side of it would be clipped away whole, so it is not drawn into it.
fn reaches(m: &Mat4, lo: Vec3, hi: Vec3) -> bool {
    let mut outside = [true; 6];
    for i in 0..8 {
        let corner = Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        let p = *m * corner.extend(1.0);
        let p = p.xyz() / p.w;
        outside[0] &= p.x < -1.0;
        outside[1] &= p.x > 1.0;
        outside[2] &= p.y < -1.0;
        outside[3] &= p.y > 1.0;
        outside[4] &= p.z < 0.0;
        outside[5] &= p.z > 1.0;
    }
    !outside.iter().any(|o| *o)
}

/// Stabilised cascades around the camera's frustum slices, looking down the sun.
#[allow(clippy::needless_range_loop)] // each cascade fills three tables at its index
pub(crate) fn cascades(frame: &HifiFrame, size: u32) -> Cascades {
    let cam = &frame.camera;
    let inv = render_from_clip(cam.view, cam.projection);
    let un = |x: f32, y: f32| {
        let p = inv * Vec4::new(x, y, 0.5, 1.0);
        p.xyz() / p.w
    };
    let eye = cam.eye;
    let forward = (un(0.0, 0.0) - eye).normalize_or(Vec3::Y);
    let rays: Vec<Vec3> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .iter()
        .map(|(x, y)| un(*x, *y) - eye)
        .collect();
    let l = frame.sky.sun_direction.normalize_or(Vec3::Z);
    let up = if l.z.abs() > 0.99 { Vec3::Y } else { Vec3::Z };
    let light_view = Mat4::look_at_rh(Vec3::ZERO, -l, up);
    let mut out = Cascades {
        matrices: [Mat4::IDENTITY; CASCADES],
        texel: [1.0; CASCADES],
        depth_range: [1.0; CASCADES],
    };
    let mut near = cam.near.max(0.05);
    for c in 0..CASCADES {
        let far = SPLITS[c].min(cam.far.max(near + 1.0));
        let mut corners = Vec::with_capacity(8);
        for d in [near, far] {
            for r in &rays {
                corners.push(eye + *r * (d / r.dot(forward).max(1e-3)));
            }
        }
        let centre = corners.iter().copied().sum::<Vec3>() / 8.0;
        let mut radius = corners
            .iter()
            .map(|p| (*p - centre).length())
            .fold(0.0f32, f32::max);
        radius = (radius * 16.0).ceil() / 16.0;
        let texel = 2.0 * radius / size as f32;
        let mut cs = light_view * centre.extend(1.0);
        cs.x = (cs.x / texel).floor() * texel;
        cs.y = (cs.y / texel).floor() * texel;
        let z_near = -cs.z - radius - CASTER_REACH;
        let z_far = -cs.z + radius;
        let proj = Mat4::orthographic_rh(
            cs.x - radius,
            cs.x + radius,
            cs.y - radius,
            cs.y + radius,
            z_near,
            z_far,
        );
        out.matrices[c] = proj * light_view;
        out.texel[c] = texel;
        out.depth_range[c] = z_far - z_near;
        near = far;
    }
    out
}

#[derive(Debug)]
struct LightingGpu {
    caster_layout: wgpu::BindGroupLayout,
    caster_pipeline: wgpu::RenderPipeline,
    cutout_pipeline: wgpu::RenderPipeline,
    leaf_layout: wgpu::BindGroupLayout,
    leaf_sampler: wgpu::Sampler,
    caster_uniform: wgpu::Buffer,
    caster_capacity: u64,
    caster_bind: wgpu::BindGroup,
    relight_layout: wgpu::BindGroupLayout,
    relight: wgpu::RenderPipeline,
    copy: wgpu::RenderPipeline,
    params: wgpu::Buffer,
    lights: wgpu::Buffer,
    cmp: wgpu::Sampler,
}

fn tex_entry(
    binding: u32,
    sample_type: wgpu::TextureSampleType,
    dim: wgpu::TextureViewDimension,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: dim,
            multisampled: false,
        },
        count: None,
    }
}

fn buf_entry(binding: u32, stage: wgpu::ShaderStages, dynamic: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: stage,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: dynamic,
            min_binding_size: None,
        },
        count: None,
    }
}

fn uniform(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn fullscreen(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    module: &wgpu::ShaderModule,
    entry: &str,
    target: wgpu::ColorTargetState,
) -> wgpu::RenderPipeline {
    fullscreen_targets(device, layout, module, entry, &[Some(target)])
}

fn fullscreen_targets(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    module: &wgpu::ShaderModule,
    entry: &str,
    targets: &[Option<wgpu::ColorTargetState>],
) -> wgpu::RenderPipeline {
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("high-fidelity lighting"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.build_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(entry),
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets,
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn caster_bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("high-fidelity shadow casters"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer,
                offset: 0,
                size: wgpu::BufferSize::new(80),
            }),
        }],
    })
}

impl LightingGpu {
    fn new(device: &wgpu::Device) -> Self {
        let caster_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("high-fidelity shadow casters"),
            entries: &[buf_entry(0, wgpu::ShaderStages::VERTEX, true)],
        });
        let shadow_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("high-fidelity shadow casters"),
            source: wgpu::ShaderSource::Wgsl(SHADOW_WGSL.into()),
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("high-fidelity shadow casters"),
            bind_group_layouts: &[Some(&caster_layout)],
            immediate_size: 0,
        });
        let caster_pipeline = device.build_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("high-fidelity shadow casters"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &shadow_module,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 12,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 0,
                    }],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                unclipped_depth: device
                    .features()
                    .contains(wgpu::Features::DEPTH_CLIP_CONTROL),
                ..wgpu::PrimitiveState::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: SHADOW_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 0,
                    slope_scale: 1.5,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });
        let leaf_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("high-fidelity leaf shadow"),
            entries: &[
                tex_entry(
                    0,
                    wgpu::TextureSampleType::Float { filterable: true },
                    wgpu::TextureViewDimension::D2,
                ),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let cut_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("high-fidelity leaf shadow"),
            bind_group_layouts: &[Some(&caster_layout), Some(&leaf_layout)],
            immediate_size: 0,
        });
        let cutout_pipeline = device.build_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("high-fidelity leaf shadow"),
            layout: Some(&cut_pl),
            vertex: wgpu::VertexState {
                module: &shadow_module,
                entry_point: Some("vs_cut"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 20,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 12,
                            shader_location: 1,
                        },
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                unclipped_depth: device
                    .features()
                    .contains(wgpu::Features::DEPTH_CLIP_CONTROL),
                ..wgpu::PrimitiveState::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: SHADOW_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 0,
                    slope_scale: 1.5,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shadow_module,
                entry_point: Some("fs_cut"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[],
            }),
            multiview_mask: None,
            cache: None,
        });
        let leaf_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("high-fidelity leaf shadow"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });
        let caster_capacity = 512;
        let caster_uniform = uniform(
            device,
            "high-fidelity shadow casters",
            caster_capacity * CASTER_STRIDE,
        );
        let caster_bind = caster_bind(device, &caster_layout, &caster_uniform);

        let float = wgpu::TextureSampleType::Float { filterable: false };
        let d2 = wgpu::TextureViewDimension::D2;
        let relight_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("high-fidelity relight"),
            entries: &[
                tex_entry(0, float, d2),
                tex_entry(1, float, d2),
                tex_entry(2, wgpu::TextureSampleType::Depth, d2),
                tex_entry(
                    3,
                    wgpu::TextureSampleType::Depth,
                    wgpu::TextureViewDimension::D2Array,
                ),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                buf_entry(5, wgpu::ShaderStages::FRAGMENT, false),
                buf_entry(6, wgpu::ShaderStages::FRAGMENT, false),
                tex_entry(7, float, d2),
                tex_entry(8, float, d2),
                tex_entry(9, float, d2),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("high-fidelity relight"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{RELIGHT_WGSL}\n{COLOUR_WGSL}\n{COPY_WGSL}").into(),
            ),
        });
        let target = wgpu::ColorTargetState {
            format: HDR,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        };
        let relight = fullscreen_targets(
            device,
            &relight_layout,
            &module,
            "fs_relight",
            &[Some(target.clone()), Some(target.clone())],
        );
        let copy = fullscreen(device, &relight_layout, &module, "fs_copy", target);
        let cmp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("high-fidelity shadow compare"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..wgpu::SamplerDescriptor::default()
        });
        Self {
            caster_layout,
            caster_pipeline,
            cutout_pipeline,
            leaf_layout,
            leaf_sampler,
            caster_uniform,
            caster_capacity,
            caster_bind,
            relight_layout,
            relight,
            copy,
            params: uniform(device, "high-fidelity relight", 1024),
            lights: uniform(device, "high-fidelity lights", (MAX_LIGHTS * 32) as u64),
            cmp,
        }
    }
}

const COPY_WGSL: &str = "
@fragment
fn fs_copy(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(picture, vec2<i32>(p.xy), 0);
}
";

/// A texture and its view, with the size it was made at.
#[derive(Debug)]
struct Held {
    _texture: wgpu::Texture,
    _view: wgpu::TextureView,
    size: (u32, u32),
}

fn hdr_texture(device: &wgpu::Device, label: &str, size: (u32, u32), mips: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size.0.max(1),
            height: size.1.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

/// The lighting pass over the opaque world.
#[derive(Debug, Default)]
pub struct Lighting {
    /// The art pack's models, which cast the sun's shadows in place of the objects they replace
    /// while they are drawn.
    pack: Option<crate::passes::art_pack::PackCasters>,
    /// The drawn ground, which casts the sun's shadows in place of the landscape's triangles
    /// while it is drawn.
    drawn: Option<crate::passes::drawn::DrawnCasters>,
    gpu: Option<LightingGpu>,
    /// The same, built ahead by the warm-up.
    pre_gpu: Prebuilt<LightingGpu>,
    blocks: HashMap<u16, CasterBlock>,
    /// The detail level each object of a resident block stands at, as last handed over.
    levels: HashMap<u16, Arc<Vec<u32>>>,
    /// The blocks whose levels or geometry changed since their lists were made: listed on the
    /// next frame that casts the sun's shadows.
    stale: HashSet<u16>,
    /// How many blocks' shadow geometry has been copied to the device, how many times a copy's
    /// drawn vertices have been listed, how many times a copy's list was kept as the levels drew
    /// the same of it, and how many changes of a block's levels have been handed over.
    copies: (u64, u64, u64, u64),
    /// The leaves' pictures as the cut-out shadow casters bind them, by texture slot, with the
    /// texture each was made for.
    leaves: HashMap<u32, (wgpu::Texture, wgpu::BindGroup)>,
    /// How many of those have been made.
    leaves_made: u64,
    /// Whether this frame steps indoors: the pass then leaves it as re-shaded.
    split: bool,
    /// The last frame's sun shadows: the blocks drawn into the cascades, a block counted once for
    /// each cascade it is drawn into, and the blocks held.
    cascade_blocks: (u64, u64),
    shadow: Option<(
        wgpu::Texture,
        Vec<wgpu::TextureView>,
        wgpu::TextureView,
        u32,
    )>,
    /// The traced scene the lamps' light is traced through, on a device with ray queries.
    rt: crate::passes::rt::RtScene,
    /// A lamp picture of one unlit texel, bound when the lamps draw nothing.
    no_lamps: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// The outdoor lamps' light after dusk.
    lamps: crate::passes::lamps::LampLight,
    /// The rooms' own light per pixel.
    cells: cells::CellLight,
    /// Whether the last frame handed over blocks' geometry or levels, not yet taken.
    handed: bool,
    /// Whether a frame the pass sat out handed some over.
    missed: bool,
    /// Whether every block's geometry is wanted again, for what was missed.
    refeed: bool,
}

impl Lighting {
    /// The pass, with the art pack's models to cast shadows through.
    #[must_use]
    pub fn with_art_pack(pack: crate::passes::art_pack::PackCasters) -> Self {
        Self {
            pack: Some(pack),
            ..Self::default()
        }
    }

    /// The pass, with the art pack's models and the drawn ground to cast shadows through.
    #[must_use]
    pub fn with_casters(
        pack: crate::passes::art_pack::PackCasters,
        drawn: crate::passes::drawn::DrawnCasters,
    ) -> Self {
        Self {
            pack: Some(pack),
            drawn: Some(drawn),
            ..Self::default()
        }
    }
}

/// The side of each shadow cascade's map for a world viewport `height` pixels high: a quarter as
/// many texels again as the viewport has rows, to the next power of two, between 2048 and 4096,
/// so a cascade's texel is about as fine as one or two pixels at its far end and finer nearer (a
/// finer map costs four times the drawing for each doubling, most of it in the cut-out leaves of
/// a forest).
/// `DERETH_HIFI_SHADOW_SIZE` asks for another. One texel, only so the light has a map to bind,
/// when no shadow is drawn.
#[must_use]
pub fn shadow_size(s: &HifiSettings, height: u32) -> u32 {
    if !shadows_wanted(s) {
        return 1;
    }
    std::env::var("DERETH_HIFI_SHADOW_SIZE")
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or_else(|| {
            (height.saturating_mul(5) / 4)
                .next_power_of_two()
                .clamp(2048, 4096)
        })
        .clamp(1, 8192)
}

fn shadows_wanted(s: &HifiSettings) -> bool {
    s.shadows.is_on() || matches!(s.lighting, Level::Medium | Level::High | Level::Ultra)
}

impl HifiPass for Lighting {
    fn name(&self) -> &'static str {
        "lighting"
    }

    fn notes(&self, notes: &mut Vec<(&'static str, u64)>) {
        notes.push(("room lights walked", self.cells.walked() as u64));
        notes.push(("sun shadow cascade blocks drawn", self.cascade_blocks.0));
        notes.push(("sun shadow blocks held", self.cascade_blocks.1));
        notes.push(("shadow geometry copied", self.copies.0));
        notes.push(("shadow geometry listed", self.copies.1));
        notes.push(("shadow geometry lists kept", self.copies.2));
        notes.push(("detail levels handed over", self.copies.3));
        notes.push(("leaf shadow pictures bound", self.leaves_made));
        let (structures, top_levels, blocks) = self.rt.counts();
        notes.push(("traced structures built", structures));
        notes.push(("traced top levels built", top_levels));
        notes.push(("traced blocks held", blocks as u64));
    }

    fn wants_feed_again(&mut self) -> bool {
        let refeed = std::mem::take(&mut self.refeed);
        self.rt.take_request() || refeed
    }

    fn wanted(&self, s: &HifiSettings, _f: &HifiFrame, _caps: &Caps) -> bool {
        s.lighting.is_on() && (self.gpu.is_some() || !self.pre_gpu.waits())
    }

    fn observe(&mut self, cx: &mut PrepareCx<'_, '_>) {
        // A block's geometry and levels are handed over once. A frame the pass sits out, while
        // its pipelines or the presentation's are being built, leaves them untaken.
        let frame = cx.frame;
        self.missed = self.missed || self.handed;
        self.handed = cx.settings.lighting.is_on()
            && !(frame.static_feed.is_empty() && frame.static_levels.is_empty());
    }

    fn warm(&mut self, cx: &mut WarmCx<'_>) {
        if !cx.settings.lighting.is_on() {
            return;
        }
        if self.gpu.is_none() {
            let device = cx.device.clone();
            self.pre_gpu.ask(cx.warm, move || LightingGpu::new(&device));
        }
        let traced = crate::passes::lamps::LampLight::wanted(cx.settings.lamps, cx.caps.ray_query);
        self.lamps.warm(cx, traced);
        self.rt.warm(cx, traced);
        self.cells.warm(cx, traced);
    }

    fn prepare(&mut self, cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        let frame = cx.frame;
        if std::env::var_os("DERETH_HIFI_LIGHTING_TRACE").is_some() {
            let (sun, amb) = legacy_light(frame);
            eprintln!(
                "lighting: sun {:?} b {} col {:?} amb {} {:?} fog {:?} tod {} legacy sun {sun:?} amb {amb:?} exposure {} lights {} feed {} blocks {}",
                frame.sky.sun_direction,
                frame.sky.sun_brightness,
                frame.sky.sun_color,
                frame.sky.ambient_level,
                frame.sky.ambient_color,
                frame.sky.fog,
                frame.sky.time_of_day,
                exposure(frame, 2.6, 0.85),
                frame.lights.len(),
                frame.static_feed.len(),
                self.blocks.len(),
            );
        }
        // What a frame sat out missed is asked for again once the pass is drawn, so that it comes
        // to a frame that takes it.
        self.handed = false;
        self.refeed = self.refeed || std::mem::take(&mut self.missed);
        self.split =
            crate::reshade::ReshadePlan::new(cx.seam.tables()).is_some_and(|p| p.is_split());
        let device = cx.seam.device().clone();
        let queue = cx.seam.queue().clone();
        // The blocks whose drawn vertices are listed again: those whose objects' levels changed,
        // and those whose geometry is new. They are listed only for a frame that casts the sun's
        // shadows: indoors, by night, or with the shadows off, they wait.
        for l in &frame.static_levels {
            self.levels.insert(l.block, Arc::clone(&l.levels));
            self.stale.insert(l.block);
            self.copies.3 += 1;
        }
        for block in &frame.static_feed {
            if self
                .blocks
                .get(&block.block)
                .is_some_and(|b| b.generation == block.generation)
            {
                continue;
            }
            let copy = |(data, runs): (Vec<f32>, LevelRuns)| {
                DeviceCopy::new(
                    &device,
                    &queue,
                    "high-fidelity shadow casters",
                    &floats(&data),
                    runs,
                    wgpu::BufferUsages::empty(),
                )
            };
            #[allow(clippy::cast_possible_truncation)]
            let landscape = (block.terrain_triangles.len() * 3) as u32;
            let cutouts = block
                .batches
                .iter()
                .filter(|b| b.cutout && !b.format.is_pre_transformed())
                .filter_map(|b| {
                    let slot = b.texture?;
                    Some((copy(cutout_vertices(b))?, slot, b.art_pack))
                })
                .collect();
            self.blocks.insert(
                block.block,
                CasterBlock {
                    generation: block.generation,
                    opaque: copy(caster_positions(block, false)),
                    landscape,
                    covered: copy(caster_positions(block, true)),
                    cutouts,
                },
            );
            self.copies.0 += 1;
            self.stale.insert(block.block);
        }
        if shadows_wanted(cx.settings) && sun_up(frame) {
            for id in self.stale.drain() {
                if let Some(b) = self.blocks.get_mut(&id) {
                    let levels = self.levels.get(&id).map_or(&[][..], |l| l.as_slice());
                    let (listed, kept) = b.select(&device, &queue, levels);
                    self.copies.1 += listed;
                    self.copies.2 += kept;
                }
            }
        }
        let resident = crate::shared::resident::ResidentBlocks::of(&frame.terrain);
        self.blocks.retain(|id, _| resident.holds(*id));
        self.levels.retain(|id, _| resident.holds(*id));
        self.stale.retain(|id| resident.holds(*id));
        if crate::passes::lamps::LampLight::wanted(cx.settings.lamps, cx.caps.ray_query) {
            self.rt
                .prepare(&device, &queue, frame, crate::passes::rt::traces(frame));
        }
        Ok(())
    }

    // The casts are offsets into a constant buffer of a few hundred blocks.
    #[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
    fn encode(&mut self, cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        let Some(rt) = cx.reshade else {
            return Ok(());
        };
        if self.split {
            return Ok(());
        }
        let frame = cx.frame;
        let device = cx.seam.device().clone();
        let queue = cx.seam.queue().clone();
        let pre = &mut self.pre_gpu;
        let gpu = self
            .gpu
            .get_or_insert_with(|| pre.take().unwrap_or_else(|| LightingGpu::new(&device)));
        let size = cx.seam.surface_size();

        // --- the shadow cascades ---------------------------------------------------------
        let shadows = shadows_wanted(cx.settings) && sun_up(frame) && !self.blocks.is_empty();
        let rows = match frame.camera.viewport[3] {
            0 => size.1,
            h => h,
        };
        let map = shadow_size(cx.settings, rows);
        if self.shadow.as_ref().is_none_or(|s| s.3 != map) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("high-fidelity sun shadow"),
                size: wgpu::Extent3d {
                    width: map,
                    height: map,
                    depth_or_array_layers: CASCADE_LAYERS,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: SHADOW_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let layers = (0..CASCADE_LAYERS)
                .map(|layer| {
                    texture.create_view(&wgpu::TextureViewDescriptor {
                        dimension: Some(wgpu::TextureViewDimension::D2),
                        base_array_layer: layer,
                        array_layer_count: Some(1),
                        ..wgpu::TextureViewDescriptor::default()
                    })
                })
                .collect();
            let all = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..wgpu::TextureViewDescriptor::default()
            });
            self.shadow = Some((texture, layers, all, map));
        }
        let cas = cascades(frame, map);
        let Some(shadow) = self.shadow.as_ref() else {
            return Ok(());
        };
        self.cascade_blocks = (0, 0);
        if shadows {
            let origins: HashMap<u16, (Vec3, (f32, f32))> = frame
                .terrain
                .iter()
                .map(|t| (t.block, (t.origin, height_range(&t.heights))))
                .collect();
            // Each block, and which cascades it can cast into: the others would clip it all away.
            let (draws, reach): (Vec<(&CasterBlock, Vec3)>, Vec<[bool; CASCADES]>) = self
                .blocks
                .iter()
                .filter_map(|(id, b)| {
                    let (o, heights) = origins.get(id)?;
                    let (lo, hi) = caster_bounds(*o, *heights);
                    let reach = std::array::from_fn(|c| reaches(&cas.matrices[c], lo, hi));
                    Some(((b, *o), reach))
                })
                .unzip();
            self.cascade_blocks.1 = draws.len() as u64;
            // Each block's constants for each cascade it casts into, one after another.
            let mut slots: Vec<[Option<u32>; CASCADES]> = vec![[None; CASCADES]; draws.len()];
            let mut held = 0u32;
            for c in 0..CASCADES {
                for (k, r) in reach.iter().enumerate() {
                    if r[c] {
                        slots[k][c] = Some(held);
                        held += 1;
                    }
                }
            }
            let needed = u64::from(held.max(1));
            if needed > gpu.caster_capacity {
                gpu.caster_capacity = needed.next_power_of_two();
                gpu.caster_uniform = uniform(
                    &device,
                    "high-fidelity shadow casters",
                    gpu.caster_capacity * CASTER_STRIDE,
                );
                gpu.caster_bind = caster_bind(&device, &gpu.caster_layout, &gpu.caster_uniform);
            }
            let mut bytes = vec![0u8; (needed * CASTER_STRIDE) as usize];
            for ((_, origin), block_slots) in draws.iter().zip(&slots) {
                for (c, slot) in block_slots.iter().enumerate() {
                    let Some(slot) = slot else { continue };
                    let at = (u64::from(*slot) * CASTER_STRIDE) as usize;
                    bytes[at..at + 64].copy_from_slice(&floats(&cas.matrices[c].to_cols_array()));
                    bytes[at + 64..at + 80]
                        .copy_from_slice(&floats(&[origin.x, origin.y, origin.z, 0.0]));
                }
            }
            queue.write_buffer(&gpu.caster_uniform, 0, &bytes);
            // While the art pack's models are drawn, they cast the shadows of the objects they
            // replace.
            let pack = self.pack.as_ref().filter(|p| p.replacing());
            if let Some(p) = pack {
                p.prepare(&device, &queue, &cas.matrices);
            }
            // While the drawn ground is drawn, it casts in place of the landscape's triangles.
            let drawn = self.drawn.as_ref().filter(|d| d.replacing());
            if let Some(d) = drawn {
                d.prepare(&queue, &cas.matrices);
            }
            // The leaves' pictures as the cut-out casters bind them, kept from frame to frame while
            // their slots hold the same textures.
            let mut used: Vec<u32> = Vec::new();
            for ((block, _), block_slots) in draws.iter().zip(&slots) {
                if block_slots.iter().all(Option::is_none) {
                    continue;
                }
                for (_, slot, _) in &block.cutouts {
                    if used.contains(slot) {
                        continue;
                    }
                    used.push(*slot);
                    let Some(t) = cx.seam.texture(dereth_render::device::TextureSlot(*slot)) else {
                        continue;
                    };
                    if self.leaves.get(slot).is_some_and(|(held, _)| held == t) {
                        continue;
                    }
                    let view = t.create_view(&wgpu::TextureViewDescriptor {
                        dimension: Some(wgpu::TextureViewDimension::D2),
                        base_array_layer: 0,
                        array_layer_count: Some(1),
                        ..wgpu::TextureViewDescriptor::default()
                    });
                    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("high-fidelity leaf shadow"),
                        layout: &gpu.leaf_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&gpu.leaf_sampler),
                            },
                        ],
                    });
                    self.leaves.insert(*slot, (t.clone(), bg));
                    self.leaves_made += 1;
                }
            }
            self.leaves.retain(|slot, _| used.contains(slot));
            #[allow(clippy::needless_range_loop)] // a cascade's index picks its blocks and offsets
            for c in 0..CASCADES {
                let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("sun shadow cascade"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &shadow.1[c],
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    // The four cascades are timed as one span.
                    timestamp_writes: cx.timer.span_writes("sun shadows", c, CASCADES),
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                cx.seam.count_pass();
                pass.set_pipeline(&gpu.caster_pipeline);
                for (k, (block, _)) in draws.iter().enumerate() {
                    let Some(slot) = slots[k][c] else {
                        continue;
                    };
                    self.cascade_blocks.0 += 1;
                    #[allow(clippy::cast_possible_truncation)]
                    let offset = (u64::from(slot) * CASTER_STRIDE) as u32;
                    let from = if drawn.is_some() { block.landscape } else { 0 };
                    pass.set_bind_group(0, &gpu.caster_bind, &[offset]);
                    if let Some(copy) = &block.opaque {
                        copy.draw(&mut pass, from);
                    }
                    if let Some(copy) = block.covered.as_ref().filter(|_| pack.is_none()) {
                        copy.draw(&mut pass, 0);
                    }
                }
                pass.set_pipeline(&gpu.cutout_pipeline);
                for (k, (block, _)) in draws.iter().enumerate() {
                    let Some(slot) = slots[k][c] else {
                        continue;
                    };
                    #[allow(clippy::cast_possible_truncation)]
                    let offset = (u64::from(slot) * CASTER_STRIDE) as u32;
                    pass.set_bind_group(0, &gpu.caster_bind, &[offset]);
                    for (copy, slot, covered) in &block.cutouts {
                        if *covered && pack.is_some() {
                            continue;
                        }
                        let Some((_, tb)) = self.leaves.get(slot) else {
                            continue;
                        };
                        pass.set_bind_group(1, tb, &[]);
                        copy.draw(&mut pass, 0);
                    }
                }
                if let Some(p) = pack {
                    p.draw(&mut pass, c);
                }
                if let Some(d) = drawn {
                    d.draw(&mut pass, c);
                }
            }
        }

        // The cascades are shared with the passes that follow (the occlusion's debug view).
        if shadows {
            #[allow(clippy::cast_lossless)]
            let bytes = u64::from(map) * u64::from(map) * 4 * CASCADES as u64;
            cx.resources.insert(
                crate::resources::ResourceName::SunShadow,
                crate::resources::Resource::Texture(shadow.0.clone(), shadow.2.clone()),
                bytes,
            )?;
        } else {
            cx.resources
                .release(crate::resources::ResourceName::SunShadow);
        }

        // --- the relight -------------------------------------------------------------------
        // The picture as it stands before the light per pixel, which the exposure also holds the
        // outdoors to.
        let src = cx.resources.texture(
            &device,
            crate::resources::ResourceName::LitCopy,
            crate::resources::TextureSpec {
                width: size.0,
                height: size.1,
                format: HDR,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
            },
        )?;
        #[allow(clippy::cast_precision_loss)]
        let viewport = cx
            .seam
            .world_viewport()
            .map_or([0.0, 0.0, size.0 as f32, size.1 as f32], |v| {
                [v.x as f32, v.y as f32, v.width as f32, v.height as f32]
            });
        // The lamps, and the traced scene they and the rooms' lights are traced through, once
        // their device objects are built.
        let lamps_on =
            crate::passes::lamps::LampLight::wanted(cx.settings.lamps, cx.caps.ray_query)
                && self.lamps.ready()
                && self.rt.ready();
        let built = lamps_on && self.rt.build(&device, &queue, cx.encoder, cx.seam, frame);
        // The lamps' light, traced through the same structures.
        let lamp_picture = if lamps_on && built {
            match self.rt.bindings() {
                Some(scene) => self
                    .lamps
                    .encode(
                        &device, &queue, cx.encoder, &scene, frame, HDR, cx.depth, &rt.normal,
                        size, viewport, cx.timer,
                    )
                    .map(|p| p.view),
                None => None,
            }
        } else {
            None
        };
        let no_lamps = &self
            .no_lamps
            .get_or_insert_with(|| {
                let t = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("high-fidelity no lamps"),
                    size: wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: HDR,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let v = t.create_view(&wgpu::TextureViewDescriptor::default());
                (t, v)
            })
            .1;
        let lamp_view = lamp_picture.unwrap_or(no_lamps);
        // The rooms' own lights per pixel, traced through the same structures with the lamps,
        // while the viewer is in a room: the rooms seen from outdoors keep the light they were
        // drawn with.
        let scene = if built { self.rt.bindings() } else { None };
        let room_light = if frame.sky.outdoor || !self.cells.ready(scene.is_some()) {
            None
        } else {
            self.cells.encode(
                &device,
                &queue,
                cx.encoder,
                scene.as_ref(),
                frame,
                cx.depth,
                &rt.normal,
                size,
                viewport,
                cx.timer,
            )
        };
        let room_view = room_light.unwrap_or(no_lamps);
        // The Remastered sky's clouds' shadows, while its clouds cast them this frame.
        let cloud_map = frame
            .sky_knobs
            .as_ref()
            .filter(|_| {
                cx.settings.art_pack
                    && frame.art_pack.is_some()
                    && cx.settings.sky.is_on()
                    && frame.sky.outdoor
            })
            .and_then(|k| {
                let plan = crate::passes::atmosphere::heavens::plan(frame, k);
                crate::passes::atmosphere::heavens::shadow_map(frame, k, &plan)
            });
        let cloud_view = match (
            cloud_map,
            cx.resources
                .get(crate::resources::ResourceName::CloudShadow),
        ) {
            (Some(_), Some(crate::resources::Resource::Texture(_, v))) => Some(v.clone()),
            _ => None,
        };
        let cloud_map = cloud_map.filter(|_| cloud_view.is_some());
        let cloud_view = cloud_view.as_ref().unwrap_or(no_lamps);
        let cam = &frame.camera;
        let (sun, amb) = legacy_light(frame);
        let sun_gain = env_f32("DERETH_HIFI_SUN", 2.6);
        let amb_gain = env_f32("DERETH_HIFI_AMBIENT", 0.85);
        let mut p: Vec<f32> = Vec::with_capacity(256);
        p.extend(render_from_clip(cam.view, cam.projection).to_cols_array());
        p.extend((cam.view * HEIGHT_UP).inverse().to_cols_array());
        p.extend((cam.projection * cam.view * HEIGHT_UP).to_cols_array());
        for m in &cas.matrices {
            p.extend(m.to_cols_array());
        }
        p.extend(SPLITS);
        p.extend(cas.texel);
        p.extend(cas.depth_range);
        p.extend(viewport);
        let sun_on = sun_up(frame);
        // The point lights' gain: none while the sun stands high.
        let point_gain = env_f32("DERETH_HIFI_POINT", 0.35)
            * if sun_on {
                1.0 - (frame.sky.sun_direction.z * frame.sky.sun_brightness * 4.0).clamp(0.0, 1.0)
            } else {
                1.0
            };
        let lights = point_lights(frame, point_gain);
        #[allow(clippy::cast_precision_loss)] // at most a few hundred lights
        let inside = lights.iter().filter(|l| l.interior).count() as f32;
        #[allow(clippy::cast_precision_loss)]
        p.extend([cam.eye.x, cam.eye.y, cam.eye.z, lights.len() as f32]);
        let l = frame.sky.sun_direction.normalize_or(Vec3::Z);
        p.extend([l.x, l.y, l.z, f32::from(u8::from(shadows))]);
        // What the cloud and the air leave of the sun's beam, and give back to the sky.
        let day = if sun_on {
            DayLight::of(frame)
        } else {
            DayLight::CLEAR
        };
        // After dark the sky's directional light (the moon's) is kept as the ordinary frame had
        // it, unshadowed: without it the night ground holds only the dim ambient, far below
        // what the vertex light gave it.
        let sun_w = if sun_on {
            sun_gain * day.sun
        } else if frame.sky.sees_outside {
            env_f32("DERETH_HIFI_MOON", 1.0)
        } else {
            0.0
        };
        p.extend([sun[0], sun[1], sun[2], sun_w]);
        p.extend([amb[0], amb[1], amb[2], amb_gain]);
        // Under the physical sky the world is replayed without its authored fog and the sky's
        // air puts the distance back over the whole picture, so the new light is not faded out
        // by a fog that is not drawn.
        let unfogged = cx.settings.sky.is_on() && frame.sky.outdoor;
        match frame.sky.fog.filter(|_| !unfogged) {
            Some(fog) => {
                p.extend([fog.min, fog.max, 1.0, 0.0]);
                p.extend(fog.color.map(|c| srgb_to_linear(f32::from(c) / 255.0)));
                p.push(1.0);
            }
            None => {
                p.extend([0.0, 1.0, 0.0, 0.0]);
                p.extend([0.0; 4]);
            }
        }
        let view_mode = f32::from(u8::from(cx.settings.debug == DebugView::Shadows));
        #[allow(clippy::cast_precision_loss)]
        p.extend([
            point_gain,
            if sun_on {
                // Under a grey sky the beam's contact shadows fade with it.
                env_f32("DERETH_HIFI_CONTACT", 2.5) * (1.0 - day.cover)
            } else {
                0.0
            },
            map as f32,
            view_mode,
        ]);
        let fwd = {
            let inv = render_from_clip(cam.view, cam.projection);
            let c = inv * Vec4::new(0.0, 0.0, 0.5, 1.0);
            (c.xyz() / c.w - cam.eye).normalize_or(Vec3::Y)
        };
        p.extend([fwd.x, fwd.y, fwd.z, cam.far]);
        // The light through leaves is always on with the light. Without the bounced light (which
        // carries the sun's alone), the sky's own fill stands in for it where the sun or the moon
        // does not reach.
        p.extend([
            1.0,
            env_f32("DERETH_HIFI_SSS", 1.0),
            if cx.settings.global_illumination.is_on() && sun_up(frame) {
                0.0
            } else {
                env_f32("DERETH_HIFI_SHADE_FILL", 0.18)
            },
            env_f32("DERETH_HIFI_CONTACT_THICKNESS", 0.2),
        ]);
        p.extend([
            f32::from(u8::from(lamp_picture.is_some())),
            env_f32("DERETH_HIFI_LAMP_GLOW", 2.5),
            if std::env::var_os("DERETH_HIFI_LAMPS_SHOW").is_some() {
                1.0
            } else {
                0.0
            },
            inside,
        ]);
        p.extend([
            f32::from(u8::from(frame.sky.sees_outside)),
            f32::from(u8::from(room_light.is_some())),
            f32::from(u8::from(
                std::env::var_os("DERETH_HIFI_ROOM_SHOW").is_some(),
            )),
            f32::from(u8::from(!frame.sky.outdoor)),
        ]);
        p.extend([env_f32("DERETH_HIFI_ROOM_KEPT", 0.45), 0.0, 0.0, 0.0]);
        let sun_lin = sun.map(|c| dereth_primitives::num::math::powf(c.max(0.0), 2.2));
        let (all_round, about_sun) = day.sky_light(sun_lin, if sun_on { sun_gain } else { 0.0 });
        p.extend([all_round, about_sun, day.penumbra(), 0.0]);
        let pack = &frame.pack_light;
        p.extend([
            pack.inner_sky,
            env_f32("DERETH_HIFI_LEAF_FLOOR", LEAF_SHADOW_FLOOR),
            pack.scatter,
            pack.scatter_deep,
        ]);
        match cloud_map {
            Some(m) => {
                p.extend([m.center.x, m.center.y, m.reach, m.base]);
                p.extend([m.strength, 0.0, 0.0, 0.0]);
            }
            None => p.extend([0.0; 8]),
        }
        queue.write_buffer(&gpu.params, 0, &floats(&p));
        let mut lb: Vec<f32> = Vec::with_capacity(MAX_LIGHTS * 8);
        for l in &lights {
            let i = l.intensity.max(0.0);
            lb.extend([l.position.x, l.position.y, l.position.z, l.falloff]);
            lb.extend([
                l.color[0] * i,
                l.color[1] * i,
                l.color[2] * i,
                f32::from(u8::from(l.interior)),
            ]);
        }
        if !lb.is_empty() {
            queue.write_buffer(&gpu.lights, 0, &floats(&lb));
        }

        let bind = |picture: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("high-fidelity relight"),
                layout: &gpu.relight_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(picture),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&rt.normal),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(cx.depth),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&shadow.2),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&gpu.cmp),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: gpu.params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: gpu.lights.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(lamp_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: wgpu::BindingResource::TextureView(room_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 9,
                        resource: wgpu::BindingResource::TextureView(cloud_view),
                    },
                ],
            })
        };
        let copy_bind = bind(&rt.hdr);
        let relight_bind = bind(&src);
        {
            let mut pass = colour_pass(cx.encoder, &src, "lit copy", None);
            cx.seam.count_pass();
            pass.set_pipeline(&gpu.copy);
            pass.set_bind_group(0, &copy_bind, &[]);
            pass.draw(0..3, 0..1);
        }
        let surface = cx.resources.texture(
            &device,
            crate::resources::ResourceName::GiSurface,
            crate::resources::TextureSpec {
                width: size.0,
                height: size.1,
                format: HDR,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
            },
        )?;
        {
            let writes = cx.timer.writes("relight");
            let attach = |view| {
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })
            };
            let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("relight"),
                color_attachments: &[attach(&rt.hdr), attach(&surface)],
                depth_stencil_attachment: None,
                timestamp_writes: writes,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            cx.seam.count_pass();
            pass.set_pipeline(&gpu.relight);
            pass.set_bind_group(0, &relight_bind, &[]);
            pass.draw(0..3, 0..1);
        }
        if lamp_picture.is_some()
            && self.lamps.draw_halos(
                &device, &queue, cx.encoder, frame, cx.depth, &rt.hdr, viewport, cx.timer,
            )
        {
            cx.seam.count_pass();
        }
        Ok(())
    }
}

fn colour_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    label: &str,
    timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: timestamps,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

#[derive(Debug)]
struct PostGpu {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    down: wgpu::RenderPipeline,
    up: wgpu::RenderPipeline,
    tonemap: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    params: Vec<wgpu::Buffer>,
    exposure_layout: wgpu::BindGroupLayout,
    exposure: wgpu::ComputePipeline,
    exposure_params: wgpu::Buffer,
    /// The exposure in force, the frame's mean light and the exposure it is adjusting toward.
    exposure_state: wgpu::Buffer,
    /// What the exposure reads where the outdoors was not measured this frame.
    no_measure: wgpu::Buffer,
}

/// Bloom, exposure and the tone map.
#[derive(Debug, Default)]
pub struct LightingPost {
    /// The device objects, built ahead by the warm-up.
    pre_gpu: Prebuilt<PostGpu>,
    gpu: Option<PostGpu>,
    bloom: Option<(Held, Vec<wgpu::TextureView>, u32)>,
    /// When the last frame was tone mapped, on the scene's clock.
    last: Option<f64>,
    /// When the last frame indoors was tone mapped, on the scene's clock.
    last_indoors: Option<f64>,
}

/// How the exposure of a frame is reached: keyed to the frame's own light or taken from the sun
/// and the sky; at once, or eased toward from the last frame's over `dt` seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExposureStep {
    /// Keyed to the frame's own light (indoors), rather than the sun and the sky's.
    pub keyed: bool,
    /// Taken at once.
    pub at_once: bool,
    /// The seconds since the last frame, held to a quarter of a second.
    pub dt: f32,
}

/// How the exposure of a frame `since_last` seconds after the last one (`None` for the first),
/// and `since_indoors` after the last one indoors, is reached; `indoors` says whether the viewer
/// is in a building's or a dungeon's cell. Indoors it is keyed to the frame and eased toward;
/// outdoors it is the sun and the sky's, taken at once, unless the viewer has just come out, when
/// it is eased toward too. After a gap the eye has not kept its adaptation, and the exposure is
/// taken at once.
#[must_use]
pub fn exposure_step(
    indoors: bool,
    since_last: Option<f32>,
    since_indoors: Option<f32>,
) -> ExposureStep {
    let kept = since_last.is_some_and(|s| s <= ADAPTATION_KEPT);
    let adjusting = indoors || since_indoors.is_some_and(|s| s <= ADJUSTING_OUTDOORS);
    ExposureStep {
        keyed: indoors,
        at_once: !kept || !adjusting,
        dt: since_last.unwrap_or(0.0).clamp(0.0, 0.25),
    }
}

impl PostGpu {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let float = wgpu::TextureSampleType::Float { filterable: true };
        let d2 = wgpu::TextureViewDimension::D2;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("high-fidelity bloom and tone map"),
            entries: &[
                tex_entry(0, float, d2),
                tex_entry(1, float, d2),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                buf_entry(3, wgpu::ShaderStages::FRAGMENT, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let exposure_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("high-fidelity exposure"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                buf_entry(2, wgpu::ShaderStages::COMPUTE, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                storage_entry(5),
            ],
        });
        let exposure_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("high-fidelity exposure"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{EXPOSURE_WGSL}\n{TONE_WGSL}\n{COLOUR_WGSL}").into(),
            ),
        });
        let exposure_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("high-fidelity exposure"),
            bind_group_layouts: &[Some(&exposure_layout)],
            immediate_size: 0,
        });
        let exposure = device.build_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("high-fidelity exposure"),
            layout: Some(&exposure_pl),
            module: &exposure_module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        let exposure_state = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("high-fidelity exposure"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("high-fidelity bloom and tone map"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{POST_WGSL}\n{TONE_WGSL}\n{COLOUR_WGSL}").into(),
            ),
        });
        let hdr = |blend| wgpu::ColorTargetState {
            format: HDR,
            blend,
            write_mask: wgpu::ColorWrites::ALL,
        };
        let add = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::REPLACE,
        };
        Self {
            format,
            down: fullscreen(device, &layout, &module, "fs_down", hdr(None)),
            up: fullscreen(device, &layout, &module, "fs_up", hdr(Some(add))),
            tonemap: fullscreen(
                device,
                &layout,
                &module,
                "fs_tonemap",
                wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::COLOR,
                },
            ),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("high-fidelity bloom"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..wgpu::SamplerDescriptor::default()
            }),
            params: (0..(BLOOM_LEVELS * 2 + 1))
                .map(|_| uniform(device, "high-fidelity bloom", 96))
                .collect(),
            layout,
            exposure_layout,
            exposure,
            exposure_params: uniform(device, "high-fidelity exposure", 256),
            exposure_state,
            no_measure: measure_buffer(device),
        }
    }
}

impl HifiPass for LightingPost {
    fn name(&self) -> &'static str {
        "lighting post"
    }

    fn wanted(&self, s: &HifiSettings, _f: &HifiFrame, _caps: &Caps) -> bool {
        s.lighting.is_on() && (self.gpu.is_some() || !self.pre_gpu.waits())
    }

    fn warm(&mut self, cx: &mut WarmCx<'_>) {
        if cx.settings.lighting.is_on() && self.gpu.is_none() {
            let (device, format) = (cx.device.clone(), cx.format);
            self.pre_gpu
                .ask(cx.warm, move || PostGpu::new(&device, format));
        }
    }

    fn prepare(&mut self, _cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn encode(&mut self, cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        let Some(rt) = cx.reshade else {
            return Ok(());
        };
        if *cx.resolved || std::env::var_os("DERETH_HIFI_ROOM_SHOW").is_some() {
            return Ok(());
        }
        if cx.settings.debug == DebugView::Shadows {
            return Ok(());
        }
        let device = cx.seam.device().clone();
        let queue = cx.seam.queue().clone();
        let format = cx.seam.surface_format();
        if self.gpu.as_ref().is_none_or(|g| g.format != format) {
            let built = self.pre_gpu.take().filter(|g| g.format == format);
            self.gpu = Some(built.unwrap_or_else(|| PostGpu::new(&device, format)));
        }
        let Some(gpu) = self.gpu.as_ref() else {
            return Ok(());
        };
        let size = cx.seam.surface_size();
        let half = ((size.0 / 2).max(1), (size.1 / 2).max(1));
        if self.bloom.as_ref().is_none_or(|b| b.0.size != half) {
            let levels = BLOOM_LEVELS.min(32 - half.0.min(half.1).max(1).leading_zeros());
            let t = hdr_texture(&device, "high-fidelity bloom", half, levels);
            let mips = (0..levels)
                .map(|m| {
                    t.create_view(&wgpu::TextureViewDescriptor {
                        base_mip_level: m,
                        mip_level_count: Some(1),
                        ..wgpu::TextureViewDescriptor::default()
                    })
                })
                .collect();
            let view = t.create_view(&wgpu::TextureViewDescriptor::default());
            self.bloom = Some((
                Held {
                    _texture: t,
                    _view: view,
                    size: half,
                },
                mips,
                levels,
            ));
        }
        let Some((_, mips, levels)) = self.bloom.as_ref() else {
            return Ok(());
        };
        let levels = *levels;
        let frame = cx.frame;
        let sun_gain = env_f32("DERETH_HIFI_SUN", 2.6);
        let amb_gain = env_f32("DERETH_HIFI_AMBIENT", 0.85);
        let exposure = exposure(frame, sun_gain, amb_gain);
        // The eye adapts over the scene's time, not the time the frames took to draw. A clock
        // that went back (a new scene) is a gap.
        let now = frame.time;
        #[allow(clippy::cast_possible_truncation)] // the seconds between two frames
        let since = |t: Option<f64>| t.filter(|t| *t <= now).map(|t| (now - t) as f32);
        let indoors = !frame.sky.outdoor;
        let step = exposure_step(indoors, since(self.last), since(self.last_indoors));
        self.last = Some(now);
        if indoors {
            self.last_indoors = Some(now);
        }
        let agx = f32::from(u8::from(
            std::env::var("DERETH_HIFI_TONEMAP").is_ok_and(|v| v.eq_ignore_ascii_case("agx")),
        ));
        #[allow(clippy::cast_precision_loss)]
        let viewport = cx
            .seam
            .world_viewport()
            .map_or([0.0, 0.0, size.0 as f32, size.1 as f32], |v| {
                [v.x as f32, v.y as f32, v.width as f32, v.height as f32]
            });
        #[allow(clippy::cast_precision_loss)]
        let level_size = |m: u32| ((half.0 >> m).max(1) as f32, (half.1 >> m).max(1) as f32);
        // Off unless asked for: a white balance against the warm sun, and a tone curve that keeps
        // hue (the two ways of taking the warm sun's oversaturation out of foliage, to compare).
        let tint = sun_tint(frame);
        let balance = env_f32("DERETH_HIFI_WHITE_BALANCE", 0.0).clamp(0.0, 1.0);
        let hue = env_f32("DERETH_HIFI_TONE_HUE", 0.0).clamp(0.0, 1.0);
        let write = |i: usize, texel: (f32, f32), first: bool| {
            let v = [
                exposure,
                env_f32("DERETH_HIFI_BLOOM", 0.06),
                agx,
                levels as f32,
                1.0 / texel.0,
                1.0 / texel.1,
                env_f32("DERETH_HIFI_TOE", 0.36),
                0.0,
                env_f32("DERETH_HIFI_SATURATION", 1.08),
                env_f32("DERETH_HIFI_CONTRAST", 1.04),
                env_f32("DERETH_HIFI_VIGNETTE", 0.28),
                f32::from(u8::from(first)),
                viewport[0],
                viewport[1],
                viewport[2],
                viewport[3],
                tint[0],
                tint[1],
                tint[2],
                balance,
                hue,
                0.0,
                0.0,
                0.0,
            ];
            queue.write_buffer(&gpu.params[i], 0, &floats(&v));
        };
        let bind = |src: &wgpu::TextureView, bloom: &wgpu::TextureView, i: usize| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("high-fidelity bloom"),
                layout: &gpu.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(bloom),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&gpu.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: gpu.params[i].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: gpu.exposure_state.as_entire_binding(),
                    },
                ],
            })
        };
        #[allow(clippy::cast_precision_loss)]
        let full = (size.0 as f32, size.1 as f32);
        // Down: the picture into level 0, then each level into the next. The chain, down and
        // up, is timed as one span.
        let chain = (2 * levels as usize).saturating_sub(1);
        let mut slot = 0usize;
        for m in 0..levels {
            let (src, texel) = if m == 0 {
                (&rt.hdr, full)
            } else {
                (&mips[(m - 1) as usize], level_size(m - 1))
            };
            write(slot, texel, m == 0);
            let b = bind(src, &rt.normal, slot);
            let writes = cx.timer.span_writes("bloom", slot, chain);
            let mut pass = colour_pass_clear(cx.encoder, &mips[m as usize], "bloom down", writes);
            cx.seam.count_pass();
            pass.set_pipeline(&gpu.down);
            pass.set_bind_group(0, &b, &[]);
            pass.draw(0..3, 0..1);
            slot += 1;
        }
        // Up: each level added onto the one above.
        for m in (1..levels).rev() {
            write(slot, level_size(m), false);
            let b = bind(&mips[m as usize], &rt.normal, slot);
            let writes = cx.timer.span_writes("bloom", slot, chain);
            let mut pass = colour_pass(cx.encoder, &mips[(m - 1) as usize], "bloom up", writes);
            cx.seam.count_pass();
            pass.set_pipeline(&gpu.up);
            pass.set_bind_group(0, &b, &[]);
            pass.draw(0..3, 0..1);
            slot += 1;
        }
        write(slot, full, false);
        // The exposure, from the frame's light before the tone map. A room that shows the
        // outdoors is not exposed so far past the sun and the sky's exposure that the outdoors
        // it shows is blown out.
        let mut a = Adaptation::indoors();
        if frame.sky.sees_outside {
            a.most = a.most.min(exposure * OUTDOORS_SHOWN_EXPOSURE).max(a.least);
        }
        let ep = [
            viewport[0],
            viewport[1],
            viewport[2],
            viewport[3],
            f32::from(u8::from(step.keyed)),
            exposure,
            f32::from(u8::from(step.at_once)),
            step.dt,
            a.key,
            a.least,
            a.most,
            a.strength,
            a.into_dark,
            a.into_light,
            a.base,
            a.white,
        ];
        // Outdoors the frame is held to the ordinary picture's brightness, as the measure over the
        // opaque world found it: no more than a little brighter, and by day no darker.
        let measured = match cx
            .resources
            .touch(crate::resources::ResourceName::OutdoorMeasure)
        {
            Some(crate::resources::Resource::Buffer(b)) if !indoors => Some(b.clone()),
            _ => None,
        };
        let mut ep = ep.to_vec();
        ep.extend(outdoor_constants(
            frame,
            cx.settings.sky.is_on(),
            measured.is_some(),
        ));
        queue.write_buffer(&gpu.exposure_params, 0, &floats(&ep));
        let measured = measured.unwrap_or_else(|| gpu.no_measure.clone());
        {
            let eb = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("high-fidelity exposure"),
                layout: &gpu.exposure_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&rt.hdr),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(cx.depth),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: gpu.exposure_params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: gpu.exposure_state.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(&rt.hdr),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: measured.as_entire_binding(),
                    },
                ],
            });
            let mut pass = cx.encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("exposure"),
                timestamp_writes: cx.timer.compute_writes("exposure"),
            });
            pass.set_pipeline(&gpu.exposure);
            pass.set_bind_group(0, &eb, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        let b = bind(&rt.hdr, &mips[0], slot);
        let writes = cx.timer.writes("tone map");
        let mut pass = colour_pass(cx.encoder, cx.colour, "tone map", writes);
        cx.seam.count_pass();
        pass.set_viewport(viewport[0], viewport[1], viewport[2], viewport[3], 0.0, 1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        pass.set_scissor_rect(
            viewport[0] as u32,
            viewport[1] as u32,
            viewport[2] as u32,
            viewport[3] as u32,
        );
        pass.set_pipeline(&gpu.tonemap);
        pass.set_bind_group(0, &b, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        *cx.resolved = true;
        Ok(())
    }
}

/// A read-write storage binding at `binding` of a compute stage.
fn storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// The buffer the measure of the outdoor frame is written into.
fn measure_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("high-fidelity outdoor measure"),
        size: 48,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[derive(Debug)]
struct MeasureGpu {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
    params: wgpu::Buffer,
    state: wgpu::Buffer,
}

impl MeasureGpu {
    fn new(device: &wgpu::Device) -> Self {
        let float = wgpu::TextureSampleType::Float { filterable: false };
        let d2 = wgpu::TextureViewDimension::D2;
        let compute = wgpu::ShaderStages::COMPUTE;
        let texture = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: compute,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: d2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("high-fidelity outdoor measure"),
            entries: &[
                texture(0, float),
                texture(1, wgpu::TextureSampleType::Depth),
                buf_entry(2, compute, false),
                storage_entry(3),
                texture(4, float),
                storage_entry(5),
                texture(6, float),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("high-fidelity outdoor measure"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{EXPOSURE_WGSL}\n{TONE_WGSL}\n{COLOUR_WGSL}").into(),
            ),
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("high-fidelity outdoor measure"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.build_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("high-fidelity outdoor measure"),
            layout: Some(&pl),
            module: &module,
            entry_point: Some("measure"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        MeasureGpu {
            layout,
            pipeline,
            params: uniform(device, "high-fidelity outdoor measure", 256),
            state: measure_buffer(device),
        }
    }
}

/// The outdoor frame's brightness against the ordinary picture's, once its light, occlusion,
/// bounced light and air are drawn and before the weather: at each of a ladder of exposures
/// about the sun and the sky's, as the tone map would draw it, an art pack's models counting for
/// little. The tone map holds the outdoor exposure to it.
#[derive(Debug, Default)]
pub struct OutdoorMeasure {
    gpu: Option<MeasureGpu>,
    /// The same, built ahead by the warm-up.
    pre_gpu: Prebuilt<MeasureGpu>,
}

impl HifiPass for OutdoorMeasure {
    fn name(&self) -> &'static str {
        "outdoor measure"
    }

    fn wanted(&self, s: &HifiSettings, f: &HifiFrame, _caps: &Caps) -> bool {
        s.lighting.is_on() && f.sky.outdoor && (self.gpu.is_some() || !self.pre_gpu.waits())
    }

    fn warm(&mut self, cx: &mut WarmCx<'_>) {
        if cx.settings.lighting.is_on() && self.gpu.is_none() {
            let device = cx.device.clone();
            self.pre_gpu.ask(cx.warm, move || MeasureGpu::new(&device));
        }
    }

    fn prepare(&mut self, _cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        Ok(())
    }

    fn encode(&mut self, cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        if cx.reshade.is_none() || cx.settings.debug == DebugView::Shadows {
            return Ok(());
        }
        let ordinary = match cx.resources.touch(crate::resources::ResourceName::LitCopy) {
            Some(crate::resources::Resource::Texture(_, v)) => v.clone(),
            _ => return Ok(()),
        };
        let Some(rt) = cx.reshade else {
            return Ok(());
        };
        let device = cx.seam.device().clone();
        let queue = cx.seam.queue().clone();
        if cx
            .resources
            .touch(crate::resources::ResourceName::OutdoorMeasure)
            .is_none()
        {
            cx.resources.insert(
                crate::resources::ResourceName::OutdoorMeasure,
                crate::resources::Resource::Buffer(measure_buffer(&device)),
                48,
            )?;
        }
        let Some(crate::resources::Resource::Buffer(measured)) = cx
            .resources
            .get(crate::resources::ResourceName::OutdoorMeasure)
        else {
            return Ok(());
        };
        let measured = measured.clone();
        let pre = &mut self.pre_gpu;
        let gpu = self
            .gpu
            .get_or_insert_with(|| pre.take().unwrap_or_else(|| MeasureGpu::new(&device)));
        let frame = cx.frame;
        let size = cx.seam.surface_size();
        #[allow(clippy::cast_precision_loss)]
        let viewport = cx
            .seam
            .world_viewport()
            .map_or([0.0, 0.0, size.0 as f32, size.1 as f32], |v| {
                [v.x as f32, v.y as f32, v.width as f32, v.height as f32]
            });
        let exposure = exposure(
            frame,
            env_f32("DERETH_HIFI_SUN", 2.6),
            env_f32("DERETH_HIFI_AMBIENT", 0.85),
        );
        let mut p = viewport.to_vec();
        p.extend([0.0, exposure, 0.0, 0.0]);
        p.extend([0.0; 8]);
        p.extend(outdoor_constants(frame, cx.settings.sky.is_on(), true));
        queue.write_buffer(&gpu.params, 0, &floats(&p));
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("high-fidelity outdoor measure"),
            layout: &gpu.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&rt.hdr),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(cx.depth),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: gpu.params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: gpu.state.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&ordinary),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: measured.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&rt.normal),
                },
            ],
        });
        let mut pass = cx.encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("outdoor measure"),
            timestamp_writes: cx.timer.compute_writes("outdoor measure"),
        });
        pass.set_pipeline(&gpu.pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(1, 1, 1);
        Ok(())
    }
}

fn colour_pass_clear<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    label: &str,
    timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: timestamps,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

/// The lighting shaders as the device is handed them, for validation without a device.
#[must_use]
pub fn shaders() -> [(&'static str, String); 7] {
    [
        ("room light", cells::shader(false)),
        ("room light, traced", cells::shader(true)),
        ("gi", crate::passes::gi::GI_WGSL.to_owned()),
        ("shadow", SHADOW_WGSL.to_owned()),
        (
            "relight",
            format!("{RELIGHT_WGSL}\n{COLOUR_WGSL}\n{COPY_WGSL}"),
        ),
        ("post", format!("{POST_WGSL}\n{TONE_WGSL}\n{COLOUR_WGSL}")),
        (
            "exposure",
            format!("{EXPOSURE_WGSL}\n{TONE_WGSL}\n{COLOUR_WGSL}"),
        ),
    ]
}

#[cfg(test)]
mod cascade_reach_tests {
    //! Behaviour: none (experimental hi-fi presentation)
    use super::*;

    /// A camera over the middle of the longest draw distance's window, looking north and a little
    /// down, and the sun in the south-east, part way up.
    fn frame() -> HifiFrame {
        let eye = Vec3::new(25.5 * BLOCK_METRES, 25.5 * BLOCK_METRES, 60.0);
        let mut frame = HifiFrame::default();
        frame.camera.eye = eye;
        frame.camera.view = Mat4::look_at_rh(eye, eye + Vec3::new(0.0, 1.0, -0.2), Vec3::Z);
        frame.camera.projection = Mat4::perspective_rh(60f32.to_radians(), 16.0 / 9.0, 0.1, 4000.0);
        frame.camera.near = 0.1;
        frame.camera.far = 4000.0;
        frame.sky.sun_direction = Vec3::new(0.4, -0.3, 0.6).normalize();
        frame
    }

    /// Every block of the window, with flat ground at 40 m: what a cascade's map holds of the
    /// world from here.
    fn blocks() -> Vec<(Vec3, (f32, f32))> {
        (0..51)
            .flat_map(|x| (0..51).map(move |y| (x, y)))
            .map(|(x, y)| {
                let o = Vec3::new(x as f32 * BLOCK_METRES, y as f32 * BLOCK_METRES, 0.0);
                (o, (40.0, 40.0))
            })
            .collect()
    }

    fn inside(m: &Mat4, p: Vec3) -> bool {
        let q = *m * p.extend(1.0);
        let q = q.xyz() / q.w;
        (-1.0..=1.0).contains(&q.x) && (-1.0..=1.0).contains(&q.y) && (0.0..=1.0).contains(&q.z)
    }

    #[test]
    fn a_block_is_drawn_into_only_the_cascades_its_casters_can_reach() {
        let frame = frame();
        let cas = cascades(&frame, 2048);
        let blocks = blocks();
        let mut drawn = 0;
        for (origin, heights) in &blocks {
            let (lo, hi) = caster_bounds(*origin, *heights);
            for c in 0..CASCADES {
                let reach = reaches(&cas.matrices[c], lo, hi);
                drawn += usize::from(reach);
                // Anything of the box inside the cascade's map keeps the block in it: the box's
                // corners, its middle and points along its edges.
                for i in 0..=4 {
                    for j in 0..=4 {
                        for k in 0..=4 {
                            let t = Vec3::new(i as f32, j as f32, k as f32) / 4.0;
                            let p = lo + (hi - lo) * t;
                            assert!(
                                reach || !inside(&cas.matrices[c], p),
                                "cascade {c} holds {p} of a block left out of it"
                            );
                        }
                    }
                }
            }
        }
        assert!(
            drawn * 20 < blocks.len(),
            "{drawn} block-cascade draws for {} blocks in {CASCADES} cascades",
            blocks.len()
        );
        assert!(drawn > 0, "the blocks round the eye cast into the cascades");
    }
}
