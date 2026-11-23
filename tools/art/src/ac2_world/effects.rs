//! AC2's placed objects that draw nothing but an effect (a fire, a chimney's smoke, a fog bank,
//! glowing bugs) as AC1 objects that play one: a setup of one empty part whose default script
//! starts a particle emitter for each of AC2's emitters, which AC1's static objects already run.
//!
//! An AC2 object plays its look's startup effects from the moment it is placed; each effect's
//! particle systems stand at the object, raised by the effect's offset. An AC2 system is emitters
//! whose particles are born in a shape, sent along a cone, and sized, colored and faded by
//! keyframes over their lives. AC1's emitter has fewer channels, so each is carried over as:
//!
//! - **where particles are born**: AC2's disc, ball or square as AC1's disc of the same reach;
//! - **how they move**: a cone about the upright as AC1's spreading emitter (a speed outward in a
//!   random direction across, a speed up and a pull), any other way as a straight throw under the
//!   pull; one that wanders by waveforms as AC1's swarm, its sway as the waveforms';
//! - **size and fade**: AC1 grows and fades a particle in a straight line over its life, from its
//!   size at birth to its size at death, and from a clearness whose mean over the life is AC2's;
//! - **color**: AC1 has no particle color, so the picture is tinted with AC2's color where the
//!   particle shows most, and drawn added to the scene or blended as AC2's material draws it.
//!
//! What AC1's emitters cannot hold is left: a particle's turn, a streak, a fade in after birth,
//! repulsors, and a system picked at random from several (the first is taken).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::ac2_look::source::Ac2;
use crate::ac2_world::art::{Ids, Records};
use auberean_assets::common::Waveform;
use auberean_assets::fx::FxNode;
use auberean_assets::fx::{Emitter, Particles};
use auberean_draw::effects::{node_for, object_effect_look, ObjectFxTable, RestRig, Rig};
use auberean_draw::material::{d3d, draw_layers, instance_material, DEVICE_FLAGS};
use auberean_draw::particles::next_keys;
use auberean_primitives::Vec3 as Vec3A;
use auberean_world::inside::{Light, LightKind};
use auberean_world::math::Mat4;
use auberean_world::records::WorldRecords;
use dereth_assets::geometry::{Placement, SetupOrder};
use dereth_assets::hook::{AnimHook, HookData};
use dereth_assets::motion::{GfxObjDegradeInfo, GfxObjInfo, ScriptStep};
use dereth_assets::{
    GfxObj, ParticleEmitterInfo, PhysicsScript, Polygon, RenderSurface, Setup, Sphere, Surface,
    SurfaceTexture, SwVertex, VertexArray,
};
use dereth_primitives::frame::V3;
use dereth_primitives::num::math;
use dereth_primitives::records::{HookCreateParticle, LightInfo};
use dereth_primitives::{DataId, EraId, Frame, Quat, Vec3};

/// The greatest side of a particle's picture taken, texels.
const PICTURE_LIMIT: u32 = 128;
/// The image category AC1's particle pictures carry.
const PARTICLE_IMAGE_CATEGORY: u32 = 4;
/// `A8R8G8B8`'s pixel format id.
const PFID_A8R8G8B8: u32 = 21;
/// AC1's surface bits: an image, blended by its alpha, added to the scene.
const SURFACE_IMAGE: u32 = 2;
const SURFACE_ALPHA: u32 = 0x100;
const SURFACE_ADDITIVE: u32 = 0x1_0000;
/// AC1's emitter kinds: births by the second; particles thrown along a local velocity under a
/// world pull, swarming, spreading outward.
const BIRTHS_BY_SECOND: i32 = 1;
const THROWN: i32 = 3;
const SWARM: i32 = 5;
const SPREADING: i32 = 6;
/// The part index that stands an emitter at the object itself.
const NO_PART: u32 = 0xFFFF_FFFF;
/// A detail level drawn facing the camera, and the level past the last that draws nothing.
const DEGRADE_FACING: i32 = 2;
const DEGRADE_NONE: i32 = 1;
/// AC1 scales a particle by no less and no more than these.
const MIN_SCALE: f32 = 0.1;
const MAX_SCALE: f32 = 10.0;
/// AC1 lets go of at most one particle a frame, so births closer than this are no closer.
const SHORTEST_BIRTH: f64 = 1.0 / 60.0;
/// The most particles one AC1 emitter is given.
const MOST_PARTICLES: u32 = 200;
/// How many ages over a particle's life its keyframes are read at.
const SAMPLES: usize = 64;
/// AC2's gravity, meters per second squared.
const GRAVITY: f32 = 9.8;
/// A cone this near the upright (its direction's height at least this) spreads as AC1 spreads.
const UPRIGHT: f32 = 0.9;
/// AC2's keyframe bits: the size and color a key sets or is blended toward, the mass, the motion
/// and the switches.
const KEY_SCALE: u32 = 0x9;
const KEY_SCALE_TO: u32 = 0x1;
const KEY_COLOR: u32 = 0x12;
const KEY_COLOR_TO: u32 = 0x2;
const KEY_MASS: u32 = 0x24;
const KEY_MOTION: u32 = 0x80;
const KEY_SWITCHES: u32 = 0x100;
const SWITCH_GRAVITY: u32 = 0x1;
/// AC2's particle motions: thrown, along waveforms, pushed by repulsors.
const MOTION_PHYSICS: u32 = 1;
const MOTION_WAVEFORM: u32 = 2;
const MOTION_REPULSE: u32 = 3;
/// AC2's particle shapes that draw: a triangle and a quad.
const DRAWN_SHAPES: [u32; 2] = [3, 4];
/// AC2's system scale kind that scales it with its object.
const SCALES_WITH_OBJECT: u32 = 2;
/// Direct3D's texture operations that double and quadruple.
const OP_MODULATE_2X: u32 = 5;
const OP_MODULATE_4X: u32 = 6;
/// Direct3D's operations that take one argument, and the argument that is the picture.
const OP_SELECT_1: u32 = 2;
const OP_SELECT_2: u32 = 3;
const ARG_TEXTURE: u32 = 2;

/// What an effect-only object became: its AC1 setup, and a line for the report.
#[derive(Clone, Debug)]
pub struct Made {
    pub setup: DataId,
    /// AC2's systems carried over and AC1's emitters made.
    pub systems: usize,
    pub emitters: usize,
    /// What it looks like, in a word or two: its color and how it moves.
    pub look: String,
    /// The lights it gives, as AC1's setup lights.
    pub lights: usize,
    /// What of AC2's effect AC1's emitters could not hold.
    pub lost: String,
}

/// Why an effect-only object is not made: it plays no effect, or none of its particles draws and
/// it gives no light.
pub const NO_PARTICLES: &str = "draws nothing: no effect of it plays particles";
pub const NONE_DRAWN: &str = "draws nothing: none of its particles draws, and it gives no light";

/// A value of a waveform about its base: its least and greatest.
fn range(w: &Waveform) -> (f32, f32) {
    let a = w.amplitude.abs();
    match w.kind {
        3..=5 => (w.base - a, w.base + a),
        6 => (w.base, w.base + a),
        _ => (w.base, w.base),
    }
}

fn mid(w: &Waveform) -> f32 {
    let (lo, hi) = range(w);
    (lo + hi) * 0.5
}

/// A particle's size (across, up) and color (red, green, blue, alpha) at `age`, as its keyframes
/// set them: each blended from the last key that sets it toward the next blended to.
fn look_at(e: &Emitter, next: &[[usize; 3]], age: f32) -> ((f32, f32), [f32; 4]) {
    let keys = &e.keyframes;
    let last = |bits: u32| {
        keys.iter()
            .enumerate()
            .take_while(|(_, k)| k.time <= age)
            .filter(|(_, k)| k.key_flags & bits != 0)
            .map(|(i, _)| i)
            .last()
    };
    let share = |a: usize, b: usize, bit: u32| {
        let (ka, kb) = (&keys[a], &keys[b]);
        if kb.key_flags & bit == 0 {
            return None;
        }
        let span = kb.time - ka.time;
        Some(if span <= 1e-4 {
            1.0
        } else {
            ((age - ka.time) / span).clamp(0.0, 1.0)
        })
    };
    let size = last(KEY_SCALE).map_or((1.0, 1.0), |a| {
        let b = next[a][0];
        let (ka, kb) = (&keys[a], &keys[b]);
        share(a, b, KEY_SCALE_TO).map_or((ka.scale_x, ka.scale_y), |f| {
            (
                f * kb.scale_x + (1.0 - f) * ka.scale_x,
                f * kb.scale_y + (1.0 - f) * ka.scale_y,
            )
        })
    });
    let color = last(KEY_COLOR).map_or([1.0; 4], |a| {
        let b = next[a][2];
        let (ka, kb) = (&keys[a].color, &keys[b].color);
        let f = share(a, b, KEY_COLOR_TO).unwrap_or(0.0);
        [
            kb.r * f + ka.r * (1.0 - f),
            kb.g * f + ka.g * (1.0 - f),
            kb.b * f + ka.b * (1.0 - f),
            kb.a * f + ka.a * (1.0 - f),
        ]
    });
    (size, color)
}

/// How a particle moves from its birth: its motion, whether gravity pulls it and its mass, as
/// its first keys set them.
fn motion_at_birth(e: &Emitter) -> (u32, bool, f32) {
    let first = |bits: u32| {
        e.keyframes
            .iter()
            .take_while(|k| k.time <= 1e-4)
            .filter(move |k| k.key_flags & bits != 0)
            .last()
    };
    let motion = first(KEY_MOTION).map_or(0, |k| k.particle_kind);
    let gravity = first(KEY_SWITCHES).is_some_and(|k| k.keyframe_flags & SWITCH_GRAVITY != 0);
    let mass = first(KEY_MASS).map_or(1.0, |k| k.mass);
    (motion, gravity, mass)
}

fn unit(v: Vec3) -> Option<Vec3> {
    let l = v.dot(v).sqrt();
    (l > 1e-4).then(|| v.mul(1.0 / l))
}

/// The straight line of a particle's width over its life, from birth (`0`) to death (`1`), nearest
/// `samples` taken at even ages, each counted by its alpha: its width at birth and at death. With
/// no alpha anywhere, the widths at birth and death themselves.
fn size_line(
    samples: &[((f32, f32), [f32; 4])],
    size: &dyn Fn((f32, f32)) -> (f32, f32),
) -> (f32, f32) {
    let n = samples.len();
    let (mut sw, mut su, mut sy, mut suu, mut suy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for (i, s) in samples.iter().enumerate() {
        let w = f64::from(s.1[3].max(0.0));
        // LINT-OK: a sample's place among a few dozen.
        #[allow(clippy::cast_precision_loss)]
        let u = i as f64 / (n - 1).max(1) as f64;
        let y = f64::from(size(s.0).0);
        sw += w;
        su += w * u;
        sy += w * y;
        suu += w * u * u;
        suy += w * u * y;
    }
    let ends = || (size(samples[0].0).0, size(samples[n - 1].0).0);
    let d = sw * suu - su * su;
    if sw <= 1e-6 || d.abs() <= 1e-12 {
        return ends();
    }
    let b = (sw * suy - su * sy) / d;
    let a = (sy - b * su) / sw;
    // LINT-OK: widths of a few meters.
    #[allow(clippy::cast_possible_truncation)]
    (a.max(0.0) as f32, (a + b).max(0.0) as f32)
}

/// One AC2 emitter as AC1 draws it: its emitter (without its model), the tint and size of the
/// model its particles draw, where it stands, and what was lost.
struct Carried {
    info: ParticleEmitterInfo,
    /// Red, green, blue the picture is multiplied by.
    tint: [f32; 3],
    /// The quad's side across and up, meters.
    quad: (f32, f32),
    origin: Vec3,
    start: f64,
    lost: Vec<&'static str>,
    /// For the report: whether it rises, and its color.
    rises: bool,
}

/// Emitter `e` drawn with `drawn`, scaled by `k` (positions) and `size_k` (sizes); `None` when it
/// draws nothing AC1 can draw (no quads, never active, or born only by moving).
fn carry(e: &Emitter, k: f32, size_k: f32, drawn: &Drawn) -> Option<Carried> {
    if !e.active || !DRAWN_SHAPES.contains(&e.particle_shape) || e.birth_rate <= 0.0 {
        return None;
    }
    let blasts = f64::from(e.blast_count.max(1));
    let (life_lo, life_hi) = range(&e.lifespan);
    let life = f64::from((life_lo + life_hi) * 0.5);
    if life <= 0.0 {
        return None;
    }
    let mut lost = Vec::new();
    // Size and color over the life.
    let next = next_keys(&e.keyframes);
    #[allow(clippy::cast_precision_loss)]
    let samples: Vec<((f32, f32), [f32; 4])> = (0..=SAMPLES)
        .map(|i| {
            #[allow(clippy::cast_possible_truncation)]
            let age = (life * (i as f64 / SAMPLES as f64) * 0.999) as f32;
            look_at(e, &next, age)
        })
        .collect();
    let p_scale = mid(&e.particle_scale);
    let size = |s: (f32, f32)| (s.0 * p_scale * size_k, s.1 * p_scale * size_k);
    let widest = samples
        .iter()
        .map(|s| size(s.0))
        .fold((0.0f32, 0.0f32), |a, s| (a.0.max(s.0), a.1.max(s.1)));
    if widest.0 <= 1e-4 || widest.1 <= 1e-4 {
        return None;
    }
    // AC1 grows a particle in a straight line over its life: the line nearest AC2's sizes where
    // the particle shows, each age counted by how opaque it is then.
    let (first, last) = size_line(&samples, &size);
    // The quad between the two sizes, so both scales stay in AC1's range.
    let ends = (first.max(widest.0 * 0.01), last.max(widest.0 * 0.01));
    let side = (ends.0 * ends.1).sqrt();
    let quad = (side, side * widest.1 / widest.0);
    let scale = |w: f32| (w / side).clamp(MIN_SCALE, MAX_SCALE);
    #[allow(clippy::cast_precision_loss)]
    let mean_alpha = samples.iter().map(|s| s.1[3]).sum::<f32>() / samples.len() as f32;
    let alpha_end = samples[SAMPLES].1[3];
    let alpha_start = 2.0 * mean_alpha - alpha_end;
    if mean_alpha <= 1e-3 {
        return None;
    }
    let peak = samples.iter().map(|s| s.1[3]).fold(0.0f32, f32::max);
    if samples
        .iter()
        .position(|s| s.1[3] >= peak * 0.99)
        .unwrap_or(0)
        > SAMPLES / 8
    {
        lost.push("a fade in after birth");
    }
    // The color where it shows most, brightened as the material's stage brightens it, as bright
    // as a picture holds: past that its hue is kept, and a particle added to the scene is made
    // as much more opaque instead, which adds the same light.
    let weight: f32 = samples.iter().map(|s| s.1[3]).sum();
    let tint = [0, 1, 2]
        .map(|c| samples.iter().map(|s| s.1[c] * s.1[3]).sum::<f32>() / weight * drawn.brighten);
    let over = tint.iter().copied().fold(1.0f32, f32::max);
    let tint = tint.map(|c| (c / over).clamp(0.0, 1.0));
    let boost = if drawn.added { over } else { 1.0 };
    let (alpha_start, alpha_end) = (
        (alpha_start * boost).clamp(0.0, 1.0),
        (alpha_end * boost).clamp(0.0, 1.0),
    );
    // Where they are born: in the shape, and as far again across as the origin's waveforms
    // carry it.
    let reach = mid(&e.scale) * 0.5 * k;
    let sway = |w: &Waveform| {
        let (lo, hi) = range(w);
        (hi - lo) * 0.5
    };
    let swayed = sway(&e.origin.x).max(sway(&e.origin.y)) * k;
    let (min_offset, max_offset) = match e.shape {
        1..=3 => (if e.inclusive_shape { 0.0 } else { reach }, reach),
        _ => (0.0, 0.0),
    };
    let max_offset = max_offset + swayed;
    if e.shape == 3 {
        lost.push("a ball's height");
    }
    // How they move.
    let (motion, gravity, mass) = motion_at_birth(e);
    let pull = if gravity && matches!(motion, MOTION_PHYSICS | MOTION_REPULSE) {
        -GRAVITY * mass * k
    } else {
        0.0
    };
    let speed = mid(&e.velocity) * k;
    let dir = unit(Vec3::new(
        e.direction.x.base,
        e.direction.y.base,
        e.direction.z.base,
    ))
    .unwrap_or(Vec3::new(0.0, 1.0, 0.0));
    let spread = ((mid(&e.min_spread) + mid(&e.max_spread)) * 0.5).to_radians();
    let still = |v: Vec3| (v, 1.0, 1.0);
    let (particle_type, a, b, c) = if motion == MOTION_WAVEFORM {
        // A wander by waveforms: AC1's swarm, its sway each waveform's.
        let wave = e
            .keyframes
            .iter()
            .find(|kf| kf.particle_kind == MOTION_WAVEFORM)
            .map(|kf| kf.position);
        let (amp, freq) = wave.map_or((Vec3::ZERO, Vec3::ZERO), |w| {
            (
                Vec3::new(
                    w.x.amplitude.abs(),
                    w.y.amplitude.abs(),
                    w.z.amplitude.abs(),
                )
                .mul(k),
                Vec3::new(
                    w.x.frequency.abs().max(0.1),
                    w.y.frequency.abs().max(0.1),
                    w.z.frequency.abs().max(0.1),
                ),
            )
        });
        (SWARM, still(dir.mul(speed)), still(freq), still(amp))
    } else if e.exploding {
        // Outward from the shape's middle, every way.
        (
            SPREADING,
            still(Vec3::new(speed, 0.0, 0.0)),
            still(Vec3::new(0.0, 0.0, pull * 0.5)),
            still(Vec3::new(1.0, 1.0, if e.shape == 2 { 0.0 } else { 1.0 })),
        )
    } else if dir.z.abs() >= UPRIGHT {
        // A cone about the upright: outward across at the cone's speed, up (or down) along it.
        let across = speed * math::sinf(spread);
        let up = speed * math::cosf(spread) * dir.z.signum();
        (
            SPREADING,
            still(Vec3::new(across, 0.0, up)),
            still(Vec3::new(0.0, 0.0, pull * 0.5)),
            still(Vec3::new(1.0, 1.0, 0.0)),
        )
    } else {
        if spread > 0.05 {
            lost.push("a cone off the upright");
        }
        (
            THROWN,
            still(dir.mul(speed)),
            still(Vec3::new(0.0, 0.0, pull)),
            still(Vec3::ZERO),
        )
    };
    if motion == MOTION_REPULSE {
        lost.push("repulsors");
    }
    if !(range(&e.rotate_velocity.x).1 == 0.0
        && range(&e.rotate_velocity.y).1 == 0.0
        && range(&e.rotate_velocity.z).1 == 0.0
        && range(&e.rotation.z).1 == 0.0
        && range(&e.world_rotation.z).1 == 0.0)
    {
        lost.push("a turn");
    }
    if range(&e.streak).1 != 0.0 {
        lost.push("a streak");
    }
    let birthrate = (f64::from(e.birth_rate) / blasts).max(SHORTEST_BIRTH);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let alive = ((f64::from(life_hi) / birthrate).ceil() as u32 + 1)
        .min(e.max_particles.max(1))
        .min(MOST_PARTICLES);
    let origin = Vec3::new(e.origin.x.base, e.origin.y.base, e.origin.z.base).mul(k);
    let info = ParticleEmitterInfo {
        id: DataId(0),
        unknown_skipped: 0,
        emitter_type: BIRTHS_BY_SECOND,
        particle_type,
        gfxobj_id: DataId(0),
        hw_gfxobj_id: DataId(0),
        birthrate,
        max_particles: i32::try_from(alive).unwrap_or(1),
        initial_particles: 0,
        total_particles: i32::try_from(e.emission_limit).unwrap_or(0),
        total_seconds: f64::from(e.time_limit.max(0.0)),
        lifespan: life,
        lifespan_rand: f64::from((life_hi - life_lo) * 0.5),
        offset_dir: Vec3::new(0.0, 0.0, 1.0),
        min_offset,
        max_offset,
        a: a.0,
        min_a: a.1,
        max_a: a.2,
        b: b.0,
        min_b: b.1,
        max_b: b.2,
        c: c.0,
        min_c: c.1,
        max_c: c.2,
        start_scale: scale(ends.0),
        final_scale: scale(ends.1),
        scale_rand: 0.0,
        start_trans: 1.0 - alpha_start,
        final_trans: 1.0 - alpha_end,
        trans_rand: 0.0,
        is_parent_local: 0,
    };
    Some(Carried {
        info,
        tint,
        quad,
        origin,
        start: f64::from(e.start_time.max(0.0)),
        lost,
        rises: a.0.z > 0.05 || (particle_type == SPREADING && b.0.z > 0.0),
    })
}

/// A word for a color: what it looks like most.
fn color_word(t: [f32; 3]) -> &'static str {
    let (r, g, b) = (t[0], t[1], t[2]);
    let hi = r.max(g).max(b);
    let lo = r.min(g).min(b);
    if hi < 0.15 {
        "dark"
    } else if hi - lo < 0.15 * hi {
        if hi > 0.6 {
            "white"
        } else {
            "gray"
        }
    } else if r >= g && g >= b {
        if g > 0.75 * r {
            "yellow"
        } else {
            "orange"
        }
    } else if g >= r && g >= b {
        "green"
    } else if b >= r && b >= g {
        if r > 0.6 * b {
            "violet"
        } else {
            "blue"
        }
    } else {
        "red"
    }
}

/// How a system's particles are drawn, as its material's first layer says.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Drawn {
    /// The picture list.
    picture: u32,
    /// What the stage multiplies the picture and the particle's color by.
    brighten: f32,
    /// Added to the scene, else blended by its alpha.
    added: bool,
    /// Whether the picture's own alpha shows: else the particle's alpha alone does.
    picture_alpha: bool,
}

/// How system `ps` draws its particles; `None` when its material draws no picture.
fn particle_material(wr: &WorldRecords, ps: &Particles) -> Option<Drawn> {
    let m = instance_material(wr, ps.material.0)?;
    let layer = draw_layers(&m, DEVICE_FLAGS).into_iter().next()?;
    let stage = layer.stages.first()?;
    if stage.texture & 0x000F_FFFF == 0 {
        return None;
    }
    let brighten = match stage.color_op {
        OP_MODULATE_2X => 2.0,
        OP_MODULATE_4X => 4.0,
        _ => 1.0,
    };
    let picture_alpha = match stage.alpha_op {
        OP_SELECT_1 => stage.alpha_arg1 == ARG_TEXTURE,
        OP_SELECT_2 => stage.alpha_arg2 == ARG_TEXTURE,
        _ => true,
    };
    Some(Drawn {
        picture: stage.texture,
        brighten,
        added: layer.dest_blend == d3d::BLEND_ONE,
        picture_alpha,
    })
}

/// What the effect objects share: the tinted pictures made (by picture list, tint and whether
/// their alpha is kept, the image texture), and the part every one of them has.
#[derive(Default)]
pub struct Shared {
    tinted: BTreeMap<(u32, [u8; 3], bool), DataId>,
    empty: Option<DataId>,
}

/// The part an effect object stands on: a model with nothing in it. AC1 makes no object of a
/// setup with no parts, so the object that plays the effect needs one, and this one draws and
/// collides with nothing.
fn empty_part(shared: &mut Shared, ids: &mut Ids, out: &mut Records) -> Result<DataId, String> {
    if let Some(id) = shared.empty {
        return Ok(id);
    }
    let id = ids.next(0x01);
    let g = GfxObj {
        id,
        flags: 0,
        surfaces: Vec::new(),
        vertex_array: VertexArray {
            vertex_type: 1,
            vertices: Vec::new(),
        },
        physics_polygons: Vec::new(),
        physics_bsp: None,
        sort_center: Vec3::ZERO,
        polygons: Vec::new(),
        drawing_bsp: None,
        did_degrade: None,
    };
    out.add(
        id,
        g.encode_for(EraId::Eor)
            .map_err(|e| format!("{id:?}: {e}"))?,
    );
    shared.empty = Some(id);
    Ok(id)
}

/// Picture list `list` multiplied by `tint`, as an AC1 image texture: with its own alpha when
/// `keep_alpha`, else opaque.
fn tinted(
    ac2: &Ac2,
    list: u32,
    tint: [f32; 3],
    keep_alpha: bool,
    made: &mut Shared,
    ids: &mut Ids,
    out: &mut Records,
) -> Option<DataId> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let key = tint.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    if let Some(&id) = made.tinted.get(&(list, key, keep_alpha)) {
        return Some(id);
    }
    let p = ac2.picture(list, PICTURE_LIMIT)?;
    let bgra: Vec<u8> = p
        .texels
        .chunks(4)
        .flat_map(|t| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let m = |v: u8, c: u8| ((u16::from(v) * u16::from(c) + 127) / 255) as u8;
            let a = if keep_alpha { t[3] } else { 255 };
            [m(t[2], key[2]), m(t[1], key[1]), m(t[0], key[0]), a]
        })
        .collect();
    let (rs_id, st_id) = (ids.next(0x06), ids.next(0x05));
    let rs = RenderSurface {
        id: rs_id,
        data_category: PARTICLE_IMAGE_CATEGORY,
        width: p.width,
        height: p.height,
        format: PFID_A8R8G8B8,
        image_size: u32::try_from(bgra.len()).unwrap_or(u32::MAX),
        data_offset: 0x18,
        default_palette_id: None,
    };
    out.add(rs_id, rs.encode(&bgra));
    let st = SurfaceTexture {
        id: st_id,
        data_category: 0,
        unknown_byte: 2,
        source_levels: vec![rs_id],
    };
    out.add(st_id, st.encode());
    made.tinted.insert((list, key, keep_alpha), st_id);
    Some(st_id)
}

/// A particle's model: one quad `quad` meters across and up, in the x-z plane facing the
/// camera, drawn with `texture`, added to the scene or blended; seen out to `distance` meters.
fn particle_model(
    texture: DataId,
    added: bool,
    quad: (f32, f32),
    distance: f32,
    ids: &mut Ids,
    out: &mut Records,
) -> Result<DataId, String> {
    let surface = Surface {
        surface_type: SURFACE_IMAGE | SURFACE_ALPHA | if added { SURFACE_ADDITIVE } else { 0 },
        orig_texture_id: Some(texture),
        orig_palette_id: Some(DataId(0)),
        color_value: None,
        translucency: 0.0,
        luminosity: 1.0,
        diffuse: 1.0,
    };
    let surface_id = ids.next(0x08);
    out.add(surface_id, surface.encode());
    let (hx, hz) = (quad.0 * 0.5, quad.1 * 0.5);
    let vertex = |id: i16, x: f32, z: f32, u: f32, v: f32| SwVertex {
        id,
        position: Vec3::new(x, 0.0, z),
        normal: Vec3::new(0.0, -1.0, 0.0),
        uvs: vec![(u, v)],
    };
    let va = VertexArray {
        vertex_type: 1,
        vertices: vec![
            vertex(0, hx, -hz, 1.0, 1.0),
            vertex(1, hx, hz, 1.0, 0.0),
            vertex(2, -hx, hz, 0.0, 0.0),
            vertex(3, -hx, -hz, 0.0, 1.0),
        ],
    };
    let polygons = vec![Polygon {
        poly_id: 0,
        num_pts: 4,
        stippling: 0,
        sides_type: 1,
        pos_surface: 0,
        neg_surface: 0,
        vertex_ids: vec![0, 1, 2, 3],
        pos_uv_indices: Some(vec![0; 4]),
        neg_uv_indices: Some(vec![0; 4]),
    }];
    let drawing = dereth_builder::bsp::drawing(&va, &polygons);
    let (id, degrade) = (ids.next(0x01), ids.next(0x11));
    let g = GfxObj {
        id,
        flags: 2 | 8,
        surfaces: vec![surface_id],
        vertex_array: va,
        physics_polygons: Vec::new(),
        physics_bsp: None,
        sort_center: Vec3::ZERO,
        polygons,
        drawing_bsp: Some(drawing),
        did_degrade: Some(degrade),
    };
    out.add(
        id,
        g.encode_for(EraId::Eor)
            .map_err(|e| format!("{id:?}: {e}"))?,
    );
    let info = GfxObjDegradeInfo {
        id: degrade,
        degrades: vec![
            GfxObjInfo {
                gfxobj_id: id,
                degrade_mode: DEGRADE_FACING,
                min_dist: distance,
                ideal_dist: distance,
                max_dist: distance,
            },
            GfxObjInfo {
                gfxobj_id: DataId(0),
                degrade_mode: DEGRADE_NONE,
                min_dist: f32::MAX,
                ideal_dist: f32::MAX,
                max_dist: f32::MAX,
            },
        ],
    };
    out.add(
        degrade,
        info.encode_for(EraId::Eor)
            .map_err(|e| format!("{degrade:?}: {e}"))?,
    );
    Ok(id)
}

/// Where on an object an effect entry stands its systems, in the object's own frame at its own
/// size: on the bone of the kind it names (raised by its offset), else at the holding location it
/// names, as the object's body first stands, else at its offset; `None` for a bone the object
/// lacks. An object with no body has no bones or holding locations, and its systems stand at
/// their offset.
fn stands_on(rig: Option<&RestRig>, node: &FxNode, lifted: Vec3) -> Option<Vec3> {
    let lifted_m = Mat4::translation(Vec3A::new(lifted.x, lifted.y, lifted.z));
    let on = match (
        rig,
        node.bone.filter(|k| *k != 0),
        node.holding_location.filter(|k| *k != 0),
    ) {
        (Some(rig), Some(kind), _) => lifted_m.then(&rig.bone(kind)?),
        (Some(rig), None, Some(kind)) => {
            let at = rig.holding_location(kind).unwrap_or(Mat4::IDENTITY);
            rig.holding_bone(kind).map_or(at, |b| at.then(&b))
        }
        _ => lifted_m,
    };
    let p = on.point(Vec3A::ZERO);
    Some(Vec3::new(p.x, p.y, p.z))
}

/// What an object's effects become in AC1: the script that starts an emitter for each of its
/// particle emitters (none when none of them draws), its lights as a setup's, how far its
/// particles reach, and what the report says of it.
#[derive(Clone, Debug)]
pub struct Played {
    pub script: Option<DataId>,
    pub lights: BTreeMap<u32, LightInfo>,
    pub reach: f32,
    pub systems: usize,
    pub emitters: usize,
    /// What it looks like, in a word or two: its color and how it moves.
    pub look: String,
    /// What of AC2's effect AC1's emitters could not hold.
    pub lost: String,
}

/// The effects AC2's `object` placed at `scale` plays from the moment it is placed, as AC1 plays
/// them ([`Played`]), or why there are none: it plays no effect, or none of its particles draws
/// and it gives no light.
#[allow(clippy::too_many_arguments)]
pub fn effects_of(
    wr: &WorldRecords,
    ac2: &Ac2,
    object: u32,
    scale: auberean_primitives::Vec3,
    shared: &mut Shared,
    ids: &mut Ids,
    out: &mut Records,
) -> Result<Result<Played, &'static str>, String> {
    let Some((look, look_scale)) = object_effect_look(wr, object) else {
        return Ok(Err(NO_PARTICLES));
    };
    let table = ObjectFxTable::of_group_with(wr, &look.global);
    let object_k = Vec3::new(
        scale.x * look_scale.x,
        scale.y * look_scale.y,
        scale.z * look_scale.z,
    );
    // The bones and holding locations an entry may stand its systems on, as the object's body
    // first stands: a torch's flame at its head, a brazier's in its bowl.
    let rig = RestRig::of_look(wr, &look);
    let mut steps = Vec::new();
    let (mut systems, mut emitters) = (0, 0);
    let mut lost: Vec<&'static str> = Vec::new();
    let mut words: BTreeMap<String, usize> = BTreeMap::new();
    let mut reach = 0.5f32;
    let mut lights: BTreeMap<u32, LightInfo> = BTreeMap::new();
    for (effect, _) in &look.global.startup_fx.entries {
        for data in table.fx_data(wr, *effect) {
            let node = node_for(&data, 0);
            let lifted = node
                .bone_offset
                .filter(|o| !(o.x.is_nan() || o.y.is_nan() || o.z.is_nan()))
                .map_or(Vec3::ZERO, |o| Vec3::new(o.x, o.y, o.z));
            let scaled = |v: Vec3| Vec3::new(v.x * object_k.x, v.y * object_k.y, v.z * object_k.z);
            let raised = match stands_on(rig.as_ref(), node, lifted) {
                Some(at) => scaled(at),
                // An entry on a bone the object lacks plays nothing.
                None => continue,
            };
            // A light the effect sets on the object: AC1's setup light where it stands.
            if let Some((true, l)) = &node.light {
                let props = l.entity.properties.clone().unwrap_or_default();
                let (light, range, intensity) = Light::of_properties(0, Vec3A::ZERO, &props);
                if light.kind == LightKind::Point && range.base > 0.0 {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let c = |v: f32| u32::from((v.clamp(0.0, 1.0) * 255.0).round() as u8);
                    let key = u32::try_from(lights.len()).unwrap_or(u32::MAX);
                    lights.insert(
                        key,
                        LightInfo {
                            frame: Frame::new(scaled(lifted), Quat::IDENTITY),
                            color_argb: 0xFF00_0000
                                | c(light.color.r) << 16
                                | c(light.color.g) << 8
                                | c(light.color.b),
                            intensity: intensity.base,
                            falloff: range.base * object_k.z,
                            cone_angle: 0.0,
                        },
                    );
                }
            }
            let Some(list) = node.particles.as_ref().filter(|p| !p.is_empty()) else {
                continue;
            };
            if list.len() > 1 {
                lost.push("one of several systems picked at random");
            }
            let Some(ps) = wr.get::<Particles>(list[0].0) else {
                continue;
            };
            let Some(drawn) = particle_material(wr, &ps) else {
                continue;
            };
            let k = if ps.scale_type == SCALES_WITH_OBJECT {
                object_k.x.max(object_k.y).max(object_k.z) * look.particle_scale
            } else {
                look.particle_scale
            };
            let size_k = if ps.world_space { k } else { 1.0 };
            let mut any = false;
            for e in &ps.emitters {
                let Some(c) = carry(e, k, size_k, &drawn) else {
                    continue;
                };
                let Some(texture) = tinted(
                    ac2,
                    drawn.picture,
                    c.tint,
                    drawn.picture_alpha,
                    shared,
                    ids,
                    out,
                ) else {
                    continue;
                };
                let model = particle_model(
                    texture,
                    drawn.added,
                    c.quad,
                    ps.stop_fade_distance.max(1.0),
                    ids,
                    out,
                )?;
                let id = ids.next(0x32);
                let info = ParticleEmitterInfo {
                    id,
                    gfxobj_id: model,
                    hw_gfxobj_id: model,
                    ..c.info
                };
                out.add(
                    id,
                    info.encode_for(EraId::Eor)
                        .map_err(|e| format!("{id:?}: {e}"))?,
                );
                let at = raised.add(c.origin);
                reach = reach.max(at.dot(at).sqrt() + c.info.max_offset + c.quad.0);
                steps.push(ScriptStep {
                    start_time: c.start,
                    hook: AnimHook {
                        hook_type: 13,
                        direction: 0,
                        data: HookData::CreateParticle(HookCreateParticle {
                            emitter_info_id: id,
                            part_index: NO_PART,
                            offset: Frame::new(at, Quat::IDENTITY),
                            emitter_id: 0,
                        }),
                    },
                });
                lost.extend(c.lost);
                let word = format!(
                    "{} {}{}",
                    color_word(c.tint),
                    if drawn.added { "glow" } else { "haze" },
                    if c.rises { ", rising" } else { "" }
                );
                *words.entry(word).or_default() += 1;
                emitters += 1;
                any = true;
            }
            systems += usize::from(any);
        }
    }
    if steps.is_empty() && lights.is_empty() {
        return Ok(Err(NONE_DRAWN));
    }
    let script = if steps.is_empty() {
        None
    } else {
        // The script's steps in time order, as AC1 runs them.
        steps.sort_by(|a, b| a.start_time.total_cmp(&b.start_time));
        let script = ids.next(0x33);
        let ps = PhysicsScript {
            id: script,
            script_data: steps,
        };
        out.add(
            script,
            ps.encode_for(EraId::Eor)
                .map_err(|e| format!("{script:?}: {e}"))?,
        );
        Some(script)
    };
    lost.sort_unstable();
    lost.dedup();
    let mut look_words = String::new();
    for (w, n) in &words {
        if !look_words.is_empty() {
            look_words.push_str("; ");
        }
        let _ = write!(look_words, "{w}");
        if *n > 1 {
            let _ = write!(look_words, " x{n}");
        }
    }
    if look_words.is_empty() {
        look_words.push_str("a light");
    }
    Ok(Ok(Played {
        script,
        lights,
        reach,
        systems,
        emitters,
        look: look_words,
        lost: if lost.is_empty() {
            "-".into()
        } else {
            lost.join(", ")
        },
    }))
}

/// AC2's `object` placed at `scale`, which draws nothing of its own, as an AC1 setup that plays
/// its effects ([`effects_of`]), or why not. The setup has one part, which draws nothing; its
/// default script starts an emitter for each of AC2's, and its lights are the effects'.
#[allow(clippy::too_many_arguments)]
pub fn effect_setup(
    wr: &WorldRecords,
    ac2: &Ac2,
    object: u32,
    scale: auberean_primitives::Vec3,
    shared: &mut Shared,
    ids: &mut Ids,
    out: &mut Records,
) -> Result<Result<Made, &'static str>, String> {
    let played = match effects_of(wr, ac2, object, scale, shared, ids, out)? {
        Ok(p) => p,
        Err(why) => return Ok(Err(why)),
    };
    let reach = played.reach;
    let sphere = Sphere {
        center: Vec3::new(0.0, 0.0, reach * 0.5),
        radius: reach,
    };
    let part = empty_part(shared, ids, out)?;
    let setup_id = ids.next(0x02);
    let light_count = played.lights.len();
    let setup = Setup {
        id: setup_id,
        flags: 4,
        parts: vec![part],
        parent_index: None,
        default_scale: None,
        allow_free_heading: true,
        has_physics_bsp: false,
        holding_locations: BTreeMap::new(),
        connection_points: BTreeMap::new(),
        placement_frames: BTreeMap::from([(
            0,
            Placement {
                frames: vec![Frame::new(Vec3::ZERO, Quat::IDENTITY)],
                hooks: Vec::new(),
            },
        )]),
        cylspheres: Vec::new(),
        spheres: Vec::new(),
        height: 0.0,
        radius: 0.0,
        step_up_height: 0.0,
        step_down_height: 0.0,
        sorting_sphere: sphere,
        selection_sphere: sphere,
        lights: played.lights.clone(),
        default_anim_id: DataId(0),
        default_script_id: played.script.unwrap_or(DataId(0)),
        default_mtable_id: DataId(0),
        default_stable_id: DataId(0),
        default_phstable_id: DataId(0),
        written_order: SetupOrder::default(),
    };
    crate::ac2_world::statics::add_setup(&setup, out)?;
    Ok(Ok(Made {
        setup: setup_id,
        systems: played.systems,
        emitters: played.emitters,
        look: played.look,
        lights: light_count,
        lost: played.lost,
    }))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's effect-only objects)
    use super::*;
    use auberean_assets::fx::{EmitterKeyframe, WaveformVec3};
    use dereth_assets::Decode;

    fn steady(v: f32) -> Waveform {
        Waveform {
            kind: 1,
            base: v,
            ..Waveform::default()
        }
    }

    fn key(time: f32, flags: u32, size: f32, alpha: f32) -> EmitterKeyframe {
        let mut k = EmitterKeyframe {
            time,
            key_flags: flags,
            scale_x: size,
            scale_y: size,
            ..EmitterKeyframe::default()
        };
        k.color.r = 1.0;
        k.color.g = 0.5;
        k.color.b = 0.0;
        k.color.a = alpha;
        k
    }

    /// A flame: born on a disc a meter across, sent up at a meter a second in a 20 degree cone,
    /// growing from a tenth of a meter to a meter and fading out over its two seconds.
    fn flame() -> Emitter {
        Emitter {
            max_particles: 500,
            shape: 2,
            particle_shape: 4,
            scale: steady(1.0),
            particle_scale: steady(1.0),
            inclusive_shape: true,
            active: true,
            birth_rate: 0.1,
            lifespan: steady(2.0),
            velocity: steady(1.0),
            direction: WaveformVec3 {
                x: steady(0.0),
                y: steady(0.0),
                z: steady(1.0),
            },
            min_spread: steady(20.0),
            max_spread: steady(20.0),
            blast_count: 1,
            keyframes: vec![
                key(0.0, 0x3b8, 0.1, 0.6),
                key(2.0, KEY_SCALE_TO | KEY_COLOR_TO, 1.0, 0.0),
            ],
            ..Emitter::default()
        }
    }

    /// A picture added to the scene as it is, its own alpha showing.
    const PLAIN: Drawn = Drawn {
        picture: 0x4000_0001,
        brighten: 1.0,
        added: true,
        picture_alpha: true,
    };

    fn system(e: Emitter) -> Particles {
        Particles {
            max_particles: 100,
            stop_fade_distance: 60.0,
            emitters: vec![e],
            ..Particles::default()
        }
    }

    #[test]
    fn a_cone_about_the_upright_spreads_out_across_and_rises_along_it() {
        let ps = system(flame());
        let c = carry(&ps.emitters[0], 1.0, 1.0, &PLAIN).expect("it draws");
        let i = c.info;
        assert_eq!(i.particle_type, SPREADING);
        let s = 20f32.to_radians();
        assert!((i.a.x - math::sinf(s)).abs() < 1e-5, "across {}", i.a.x);
        assert!((i.a.z - math::cosf(s)).abs() < 1e-5, "up {}", i.a.z);
        assert_eq!(i.c, Vec3::new(1.0, 1.0, 0.0), "outward across, every way");
        assert_eq!(
            (i.min_offset, i.max_offset),
            (0.0, 0.5),
            "anywhere on the disc"
        );
        assert!((i.birthrate - 0.1).abs() < 1e-6);
        assert_eq!(i.max_particles, 21, "as many as live at once, and one");
        assert_eq!(
            (i.total_particles, i.total_seconds),
            (0, 0.0),
            "for as long as it stands"
        );
    }

    #[test]
    fn a_particle_grows_from_its_birth_size_to_its_death_size_and_fades_as_much_overall() {
        let ps = system(flame());
        let c = carry(&ps.emitters[0], 1.0, 1.0, &PLAIN).expect("it draws");
        let i = c.info;
        // The quad stands between the two sizes; the scales take it to each.
        assert!(
            (c.quad.0 * i.start_scale - 0.1).abs() < 0.02,
            "{:?} {}",
            c.quad,
            i.start_scale
        );
        assert!(
            (c.quad.0 * i.final_scale - 1.0).abs() < 0.03,
            "{:?} {}",
            c.quad,
            i.final_scale
        );
        // AC2's alpha runs from 0.6 to 0; AC1's clearness from 0.4 to all but clear, the mean
        // the same.
        assert!((i.start_trans - 0.4).abs() < 0.02, "{}", i.start_trans);
        assert!(i.final_trans > 0.99, "{}", i.final_trans);
        assert!(c.lost.is_empty(), "{:?}", c.lost);
        assert_eq!(color_word(c.tint), "orange");
    }

    /// A stage that doubles the color: a picture holds no more than its whole, so the hue is
    /// kept and a particle added to the scene is twice as opaque, adding the same light; one
    /// blended by its alpha keeps its opacity.
    #[test]
    fn a_doubled_color_added_to_the_scene_is_carried_as_twice_the_opacity() {
        // Its alpha runs from 0.3 to 0.
        let mut e = flame();
        e.keyframes[0] = key(0.0, 0x3b8, 0.1, 0.3);
        let ps = system(e);
        let doubled = Drawn {
            brighten: 2.0,
            ..PLAIN
        };
        let c = carry(&ps.emitters[0], 1.0, 1.0, &doubled).expect("it draws");
        assert_eq!(c.tint, [1.0, 0.5, 0.0], "the hue kept");
        let opacity = 1.0 - c.info.start_trans;
        assert!((opacity - 0.6).abs() < 0.02, "{opacity}");
        let blended = Drawn {
            added: false,
            ..doubled
        };
        let c = carry(&ps.emitters[0], 1.0, 1.0, &blended).expect("it draws");
        let opacity = 1.0 - c.info.start_trans;
        assert!((opacity - 0.3).abs() < 0.02, "{opacity}");
    }

    #[test]
    fn a_particle_that_fades_in_after_birth_is_carried_with_that_said() {
        let mut e = flame();
        e.keyframes = vec![
            key(0.0, 0x3b8, 0.1, 0.0),
            key(1.5, KEY_SCALE_TO | KEY_COLOR_TO, 0.5, 0.5),
            key(2.0, KEY_SCALE_TO | KEY_COLOR_TO, 1.0, 0.0),
        ];
        let ps = system(e);
        let c = carry(&ps.emitters[0], 1.0, 1.0, &PLAIN).expect("it draws");
        assert_eq!(c.lost, vec!["a fade in after birth"]);
    }

    #[test]
    fn an_emitter_that_draws_no_quads_or_is_never_seen_is_not_carried() {
        let mut e = flame();
        e.particle_shape = 1;
        assert!(carry(&e, 1.0, 1.0, &PLAIN).is_none(), "no quads");
        let mut e = flame();
        e.keyframes = vec![key(0.0, 0x3b8, 0.1, 0.0), key(2.0, 0x3, 1.0, 0.0)];
        assert!(carry(&e, 1.0, 1.0, &PLAIN).is_none(), "never seen");
        let mut e = flame();
        e.birth_rate = 0.0;
        assert!(carry(&e, 1.0, 1.0, &PLAIN).is_none(), "born only by moving");
    }

    #[test]
    fn a_heavy_particle_thrown_sideways_falls_as_its_mass_pulls_it() {
        let mut e = flame();
        e.direction = WaveformVec3 {
            x: steady(1.0),
            y: steady(0.0),
            z: steady(0.0),
        };
        e.min_spread = steady(0.0);
        e.max_spread = steady(0.0);
        let mut first = key(0.0, 0x3b8, 0.1, 0.6);
        first.particle_kind = MOTION_PHYSICS;
        first.keyframe_flags = SWITCH_GRAVITY;
        first.mass = 2.0;
        e.keyframes[0] = first;
        let ps = system(e);
        let c = carry(&ps.emitters[0], 1.0, 1.0, &PLAIN).expect("it draws");
        assert_eq!(c.info.particle_type, THROWN);
        assert_eq!(c.info.a, Vec3::new(1.0, 0.0, 0.0));
        assert!((c.info.b.z + 19.6).abs() < 1e-4, "{:?}", c.info.b);
    }

    /// The camp's fire, from this machine's AC2 install: an object whose meshes draw nothing and
    /// whose look plays an orange flame becomes a setup of one empty part that starts, at its
    /// origin, an AC1 emitter of camera-facing quads added to the scene for each of the flame's
    /// emitters.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn an_object_that_draws_only_a_fire_becomes_a_setup_that_starts_its_flames() {
        let dir = auberean_dat::testing::require_target_install();
        let wr = crate::ac2_world::statics::world_records(&dir).expect("the install");
        let ac2 = Ac2::open(&dir).expect("the install");
        let (mut ids, mut out, mut tints) = (Ids::default(), Records::default(), Shared::default());
        let one = auberean_primitives::Vec3::new(1.0, 1.0, 1.0);
        let made = effect_setup(&wr, &ac2, 0x4700_033C, one, &mut tints, &mut ids, &mut out)
            .expect("reads")
            .expect("a fire");
        assert_eq!(
            (made.systems, made.emitters),
            (1, 2),
            "the flame's two emitters"
        );
        let setup = Setup::decode_payload(made.setup, &out.0[&made.setup]).expect("a setup");
        assert_eq!(setup.parts.len(), 1, "one part to stand on");
        let id = setup.parts[0];
        let empty = GfxObj::decode_payload(id, &out.0[&id]).expect("a model");
        assert!(
            empty.polygons.is_empty() && empty.physics_polygons.is_empty(),
            "nothing drawn and nothing to collide with"
        );
        let script = setup.default_script_id;
        let script = PhysicsScript::decode_payload(script, &out.0[&script]).expect("a script");
        assert_eq!(script.script_data.len(), 2);
        for step in &script.script_data {
            let HookData::CreateParticle(p) = &step.hook.data else {
                panic!("an emitter started: {:?}", step.hook);
            };
            assert_eq!(p.part_index, NO_PART, "at the object itself");
            let id = p.emitter_info_id;
            let e = ParticleEmitterInfo::decode_payload(id, &out.0[&id]).expect("an emitter");
            assert_eq!(e.particle_type, SPREADING, "up the flame's cone");
            let id = e.hw_gfxobj_id;
            let g = GfxObj::decode_payload(id, &out.0[&id]).expect("a model");
            assert_eq!(g.polygons.len(), 1, "one quad");
            let id = g.surfaces[0];
            let s = Surface::decode_payload(id, &out.0[&id]).expect("a surface");
            assert_eq!(
                s.surface_type,
                SURFACE_IMAGE | SURFACE_ALPHA | SURFACE_ADDITIVE
            );
            let d = g.did_degrade.expect("a detail record");
            let d = GfxObjDegradeInfo::decode_payload(d, &out.0[&d]).expect("its levels");
            assert_eq!(
                d.degrades[0].degrade_mode, DEGRADE_FACING,
                "facing the camera"
            );
        }
        assert!(made.look.contains("orange glow"), "{}", made.look);
    }

    /// A fire pit that draws its stones plays its flames where its entries raise them, in its
    /// bowl: the script starts each emitter at its entry's offset (and its emitter's origin).
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_fire_pits_flames_stand_in_its_bowl() {
        let dir = auberean_dat::testing::require_target_install();
        let wr = crate::ac2_world::statics::world_records(&dir).expect("the install");
        let ac2 = Ac2::open(&dir).expect("the install");
        let (mut ids, mut out, mut shared) =
            (Ids::default(), Records::default(), Shared::default());
        let one = auberean_primitives::Vec3::new(1.0, 1.0, 1.0);
        let played = effects_of(&wr, &ac2, 0x4700_0085, one, &mut shared, &mut ids, &mut out)
            .expect("reads")
            .expect("a flame");
        let script = played.script.expect("a script");
        let script = PhysicsScript::decode_payload(script, &out.0[&script]).expect("a script");
        let at: Vec<Vec3> = script
            .script_data
            .iter()
            .filter_map(|s| match &s.hook.data {
                HookData::CreateParticle(p) => Some(p.offset.origin),
                _ => None,
            })
            .collect();
        assert_eq!(at.len(), 4, "a flame for each of its four entries");
        assert!(
            at.iter()
                .all(|p| (p.y - 0.1).abs() < 1e-4 && (0.2..=0.7).contains(&p.z)),
            "{at:?}"
        );
    }

    /// An object that draws nothing and plays only a light becomes a setup of one empty part that
    /// gives the light and starts no script.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn an_object_that_only_gives_light_becomes_a_setup_with_that_light() {
        let dir = auberean_dat::testing::require_target_install();
        let wr = crate::ac2_world::statics::world_records(&dir).expect("the install");
        let ac2 = Ac2::open(&dir).expect("the install");
        let (mut ids, mut out, mut shared) =
            (Ids::default(), Records::default(), Shared::default());
        let one = auberean_primitives::Vec3::new(1.0, 1.0, 1.0);
        let made = effect_setup(&wr, &ac2, 0x4700_3D3D, one, &mut shared, &mut ids, &mut out)
            .expect("reads")
            .expect("a light");
        assert_eq!((made.emitters, made.lights), (0, 1));
        let setup = Setup::decode_payload(made.setup, &out.0[&made.setup]).expect("a setup");
        assert_eq!(setup.parts.len(), 1, "one part to stand on");
        assert_eq!(setup.default_script_id, DataId(0), "nothing to start");
        let light = setup.lights.values().next().expect("its light");
        assert!(light.falloff > 0.0 && light.intensity > 0.0, "{light:?}");
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn an_object_with_no_effects_is_not_made() {
        let dir = auberean_dat::testing::require_target_install();
        let wr = crate::ac2_world::statics::world_records(&dir).expect("the install");
        let ac2 = Ac2::open(&dir).expect("the install");
        let (mut ids, mut out, mut tints) = (Ids::default(), Records::default(), Shared::default());
        let one = auberean_primitives::Vec3::new(1.0, 1.0, 1.0);
        let made = effect_setup(&wr, &ac2, 0x4700_3C0E, one, &mut tints, &mut ids, &mut out)
            .expect("reads");
        assert_eq!(made.err(), Some(NO_PARTICLES));
        assert!(out.0.is_empty(), "nothing written");
    }
}
