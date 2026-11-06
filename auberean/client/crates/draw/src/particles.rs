//! Particle systems: the sparks, glows and mists an effect sets going, run as the client runs them
//! and drawn as camera-facing quads.
//!
//! A system (a `.psdesc` record) holds emitters. Each emitter, once its start time has passed,
//! discharges a blast of particles every birth interval (or every so many meters its system
//! moves), up to its limits. A particle is born somewhere in the emitter's shape (a point, a
//! square, a disc, a ball or a line), heads along the emitter's direction spread into a cone, and
//! then follows its keyframes: each keyframe can set or blend toward a size, a color and a mass,
//! and can change how the particle moves (thrown under gravity, along waveforms, through a list of
//! points). Positions advance only every physics step of the system; between steps the quads are
//! drawn blended between the last two positions, one step behind.
//!
//! What a system draws is ordinary [`ObjectBatch`]es: one quad per particle, its corners around the
//! particle along the camera's right and up, colored by the particle's keyframe color, with the
//! system's material.

use std::sync::Arc;

use auberean_assets::common::Waveform;
use auberean_assets::fx::{Emitter, EmitterKeyframe, Particles, WaveformVec3};
use auberean_primitives::random::Random;
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::inside::{particle_light, ParticleLight};
use auberean_world::math::{add, cross, dot, length, scale, sub, Mat4};
use auberean_world::records::WorldRecords;
use dereth_primitives::num::math;

use crate::material::{draw_layers, instance_material, DrawLayer, DEVICE_FLAGS};
use crate::mesh::MeshVertex;
use crate::scene::ObjectBatch;

/// The world's gravity, meters per second squared: a particle of mass 1 falls with it, one of
/// negative mass rises.
pub const GRAVITY: Vec3 = Vec3 {
    x: 0.0,
    y: 0.0,
    z: -9.8,
};

/// Below this a length, a time or a difference counts as zero.
const EPSILON: f32 = 0.0002;

/// What an emitter's particles carry, as bits: decided once from its description.
mod data {
    pub const TIME_CREATED: u32 = 0x2;
    pub const START_ORIGIN: u32 = 0x4;
    pub const PREVIOUS_ORIGIN: u32 = 0x10;
    pub const VELOCITY: u32 = 0x20;
    pub const KEYFRAME_FLAGS: u32 = 0x40;
    pub const KEYFRAMES: u32 = 0x80;
    pub const WAVEFORM: u32 = 0x100;
    pub const SCALE: u32 = 0x200;
    pub const ROTATION: u32 = 0x400;
    pub const START_ROTATION: u32 = 0x800;
    pub const ROTATE_VELOCITY: u32 = 0x1000;
    pub const STREAK: u32 = 0x2000;
    pub const STREAK_ORIGIN: u32 = 0x4000;
    pub const PREVIOUS_STREAK_ORIGIN: u32 = 0x8000;
    pub const WORLD_ROTATION: u32 = 0x10000;
    pub const START_WORLD_ROTATION: u32 = 0x20000;
    pub const POINT_INDEX: u32 = 0x40000;
    pub const POINT_SPEED: u32 = 0x80000;
    pub const WORLD_ORIGIN: u32 = 0x10_0000;
}

/// How a particle moves after a keyframe: the keyframe's particle kind.
mod motion {
    pub const PHYSICS: u32 = 1;
    pub const WAVEFORM: u32 = 2;
    pub const REPULSE: u32 = 3;
    pub const POINTS: u32 = 4;
    pub const POINTS_BLENDED: u32 = 5;
}

/// A keyframe's flags: which of its values it sets and which it is blended toward.
mod key {
    /// The size is blended toward this keyframe's.
    pub const SCALE_TO: u32 = 0x1;
    /// The color is blended toward this keyframe's.
    pub const COLOR_TO: u32 = 0x2;
    /// The mass is blended toward this keyframe's.
    pub const MASS_TO: u32 = 0x4;
    /// The size, color and mass keyframes a particle remembers (set at, or blended toward).
    pub const SCALE: u32 = 0x9;
    pub const COLOR: u32 = 0x12;
    pub const MASS: u32 = 0x24;
    /// The motion changes here.
    pub const MOTION: u32 = 0x80;
    /// The particle's gravity and wind switches change here.
    pub const SWITCHES: u32 = 0x100;
}

/// A particle's own switches, set by keyframes: gravity and wind pull it.
mod pull {
    pub const GRAVITY: u32 = 0x1;
    pub const WIND: u32 = 0x2;
    /// Points are taken at random rather than in turn.
    pub const RANDOM_POINT: u32 = 0x8;
}

/// A waveform's value at time `t` (seconds), rolling `rng` for the noise shape: the client's one
/// evaluation ([`auberean_world::wave::value`]).
pub fn waveform_at(w: &Waveform, t: f64, rng: &Random) -> f32 {
    auberean_world::wave::value(w, t, rng)
}

fn wave3(w: &WaveformVec3, t: f64, rng: &Random) -> Vec3 {
    // the client reads z, then y, then x
    let z = waveform_at(&w.z, t, rng);
    let y = waveform_at(&w.y, t, rng);
    let x = waveform_at(&w.x, t, rng);
    Vec3::new(x, y, z)
}

fn steady_is(w: &Waveform, value: f32) -> bool {
    w.kind == 1 && w.base == value
}

fn unit_or_zero(v: Vec3) -> Vec3 {
    let l = length(v);
    if l > 0.0 {
        scale(v, 1.0 / l)
    } else {
        v
    }
}

/// The rotation of Euler angles `a` (radians about x, y, z) as rows: each row the image of an
/// axis, so a vector `v` turns to `v.x row0 + v.y row1 + v.z row2`.
// D2-13288, D2-13283
#[must_use]
pub fn angle_rows(a: Vec3) -> [Vec3; 3] {
    let (sx, cx) = (math::sinf(-a.x), math::cosf(-a.x));
    let (sy, cy) = (math::sinf(-a.y), math::cosf(-a.y));
    let (sz, cz) = (math::sinf(-a.z), math::cosf(-a.z));
    [
        Vec3::new(cz * cy, sy, -(sz * cy)),
        Vec3::new(sz * sx - cz * sy * cx, cy * cx, cz * sx + sy * sz * cx),
        Vec3::new(sz * cx + cz * sy * sx, -(cy * sx), cz * cx - sy * sz * sx),
    ]
}

fn turn(rows: &[Vec3; 3], v: Vec3) -> Vec3 {
    add(
        add(scale(rows[0], v.x), scale(rows[1], v.y)),
        scale(rows[2], v.z),
    )
}

/// `v` turned by `angle` radians about the unit axis `axis`, right-handed.
fn turn_about(v: Vec3, axis: Vec3, angle: f32) -> Vec3 {
    let (s, c) = (math::sinf(angle), math::cosf(angle));
    // Rodrigues: v c + (k x v) s + k (k . v)(1 - c)
    add(
        add(scale(v, c), scale(cross(axis, v), s)),
        scale(axis, dot(axis, v) * (1.0 - c)),
    )
}

/// The frame looking along unit `along`: its right and up, up as near the world's up as the
/// direction allows (north when it points straight up or down).
fn look_frame(along: Vec3) -> (Vec3, Vec3) {
    let unit = |v: Vec3| {
        let l = length(v);
        (l >= EPSILON).then(|| scale(v, 1.0 / l))
    };
    let up = unit(sub(Vec3::new(0.0, 0.0, 1.0), scale(along, along.z)))
        .or_else(|| unit(sub(Vec3::new(0.0, 1.0, 0.0), scale(along, along.y))))
        .unwrap_or(Vec3::new(0.0, 0.0, 1.0));
    (cross(along, up), up)
}

/// What a particle carries: every value an emitter's data bits can ask for (its making time, its
/// start, current, previous and world origins, its velocity, streak and streak origins, its turns
/// and turning speed, its keyframe switches and keyframes, its waveform, its point and speed along
/// a path, its size), each read and written as a field; the bits say which of them the emitter uses.
// D2-10183, D2-10185, D2-10186, D2-10187, D2-10188, D2-10189, D2-10190, D2-10191, D2-10192
// D2-10193, D2-10194, D2-10195, D2-10196, D2-10197, D2-10198, D2-10199, D2-10200, D2-10201
// D2-10202, D2-10203, D2-10204, D2-10205, D2-10206, D2-10207, D2-10208, D2-10209, D2-10210
// D2-10211, D2-10212, D2-10213, D2-10214, D2-10215, D2-10216, D2-10217, D2-10218, D2-10219
// D2-10220, D2-10221, D2-10222, D2-10223
#[derive(Clone, Debug, PartialEq)]
struct Particle {
    created: f64,
    start_origin: Vec3,
    origin: Vec3,
    previous_origin: Vec3,
    world_origin: Vec3,
    velocity: Vec3,
    scale: f32,
    switches: u32,
    /// The last keyframe passed, if any.
    current_key: Option<usize>,
    /// The keyframes last setting or blending the size, the mass, the color, and the motion.
    last: [Option<usize>; 4],
    waveform: Option<WaveformVec3>,
    rotation: Vec3,
    start_rotation: Vec3,
    rotate_velocity: Vec3,
    world_rotation: Vec3,
    start_world_rotation: Vec3,
    point_index: Option<u16>,
    point_speed: f32,
    /// How far behind its head a streaking particle's tail trails, in seconds of its motion.
    streak: f32,
    streak_origin: Vec3,
    previous_streak_origin: Vec3,
}

/// One emitter as it runs.
#[derive(Clone, Debug)]
struct EmitterState {
    index: usize,
    data: u32,
    /// For each keyframe, the next one blending the size, the mass and the color (itself when
    /// none does).
    next_keys: Vec<[usize; 3]>,
    /// For each keyframe, the point the next particle starts at.
    next_point: Vec<u16>,
    capacity: usize,
    particles: Vec<Particle>,
    last_think: f64,
    next_birth: f64,
    last_distance_discharge: f64,
    discharged: u32,
    /// Where the system stood at the last discharge by distance, and the end of its line.
    last_place: Vec3,
    last_extent: Vec3,
    /// Whether the emitter was placed where its system first stood.
    placed: bool,
}

/// The particle keyframes' links: for each keyframe but the last, the next keyframe with each of
/// the blend-toward bits (size, mass, color), else the keyframe itself; the last links to itself.
// D2-10263
#[must_use]
pub fn next_keys(keys: &[EmitterKeyframe]) -> Vec<[usize; 3]> {
    let n = keys.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n.saturating_sub(1) {
        let find = |bit: u32| {
            keys.iter()
                .enumerate()
                .skip(i + 1)
                .find(|(_, k)| k.key_flags & bit != 0)
                .map_or(i, |(j, _)| j)
        };
        out.push([find(key::SCALE_TO), find(key::MASS_TO), find(key::COLOR_TO)]);
    }
    if n > 0 {
        out.push([n - 1; 3]);
    }
    out
}

/// What an emitter's particles must carry, from its description.
// D2-10236
fn data_flags(world_space: bool, e: &Emitter) -> u32 {
    let mut f = 0x8;
    if world_space {
        f |= data::WORLD_ORIGIN;
    }
    if !e.keyframes.is_empty() {
        f |= data::KEYFRAMES | data::PREVIOUS_ORIGIN | data::TIME_CREATED;
    }
    for k in &e.keyframes {
        let moved = data::KEYFRAME_FLAGS | data::PREVIOUS_ORIGIN | data::START_ORIGIN;
        f |= match k.particle_kind {
            motion::PHYSICS | motion::REPULSE => moved | data::VELOCITY,
            motion::WAVEFORM => {
                data::WAVEFORM | data::PREVIOUS_ORIGIN | data::START_ORIGIN | data::TIME_CREATED
            }
            motion::POINTS => moved | data::POINT_INDEX,
            motion::POINTS_BLENDED => moved | data::POINT_INDEX | data::POINT_SPEED,
            _ => 0,
        };
    }
    if !steady_is(&e.streak, 0.0) {
        f |= data::STREAK | data::STREAK_ORIGIN | data::PREVIOUS_STREAK_ORIGIN;
    }
    if !steady_is(&e.particle_scale, 1.0) {
        f |= data::SCALE;
    }
    let turns = |w: &WaveformVec3| [&w.x, &w.y, &w.z].iter().any(|w| !steady_is(w, 0.0));
    if turns(&e.world_rotation) {
        f |= data::WORLD_ROTATION;
    } else if turns(&e.rotation) {
        f |= data::ROTATION;
    }
    if turns(&e.rotate_velocity) {
        if f & data::WORLD_ROTATION != 0 {
            return f | data::START_WORLD_ROTATION | data::ROTATE_VELOCITY;
        }
        f |= data::ROTATION | data::START_ROTATION | data::ROTATE_VELOCITY;
    }
    f
}

/// One particle system set going at a place. Dropping it lets go of its material and its emitters
/// with their particles.
// D2-10278, D2-10285, D2-10233
#[derive(Clone, Debug)]
pub struct ParticleSet {
    desc: Arc<Particles>,
    layers: Vec<DrawLayer>,
    /// Where the system stands: its rotation and origin (no scale).
    place: Mat4,
    /// The object's scale, which the system draws with.
    scale: Vec3,
    created: f64,
    start: f64,
    started: bool,
    emitting: bool,
    emitters: Vec<EmitterState>,
    rng: Random,
    /// Whether the material asks for the particles to be lit.
    lit: bool,
    /// The lights about the system that light lit particles: outdoors, the sun.
    lights: Vec<ParticleLight>,
    /// The repulsors pushing its particles this frame.
    repulsors: Vec<Push>,
    /// It draws whole however far off it is.
    no_fade: bool,
    /// When the system was first found with nothing more to discharge.
    spent_since: Option<f64>,
}

/// The acceleration repulsor `r` gives a particle at `at`: none beyond its range; along the way
/// from it (none when the particle stands on it), `acceleration (1 - d / range)`; reversed for an
/// attractor; nothing along an axis the emitter constrains.
// D2-10242
#[must_use]
pub fn push_of(r: &Push, at: Vec3, constrain: [bool; 3]) -> Vec3 {
    let d = sub(at, r.origin);
    let dist = length(d);
    if dist >= r.range {
        return Vec3::ZERO;
    }
    let mut a = if dist.abs() < EPSILON {
        Vec3::ZERO
    } else {
        scale(d, (1.0 / dist) * (1.0 - dist / r.range) * r.acceleration)
    };
    if r.attracts {
        a = scale(a, -1.0);
    }
    if constrain[0] {
        a.x = 0.0;
    }
    if constrain[1] {
        a.y = 0.0;
    }
    if constrain[2] {
        a.z = 0.0;
    }
    a
}

/// A repulsor as a system's particles feel it: where it stands, in the frame the particles' places
/// are kept in, how far it reaches, how hard it pushes there, and whether it pulls instead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Push {
    pub origin: Vec3,
    pub range: f32,
    pub acceleration: f32,
    pub attracts: bool,
}

/// The description's scale type of a system drawn at its object's scale as well as its look's
/// particle scale: all but 2 of the data's 1,560.
const SCALES_WITH_OBJECT: u32 = 2;

impl ParticleSet {
    /// The system of record `id`, its material made ready, standing at `place` with the object's
    /// `scale`, made at time `created` (seconds of the client's running time), rolling `dice`.
    #[must_use]
    pub fn load(
        records: &WorldRecords,
        id: u32,
        place: Mat4,
        scale: Vec3,
        created: f64,
        dice: &Random,
    ) -> Option<Self> {
        let desc: Particles = records.get(id)?;
        let material = instance_material(records, desc.material.0);
        let lit = material
            .as_ref()
            .and_then(|m| m.layers.first())
            .is_some_and(|l| l.switches.particle_lighting);
        let layers = material
            .map(|m| draw_layers(&m, DEVICE_FLAGS))
            .unwrap_or_default();
        let mut set = Self::new(desc, layers, place, scale, created, dice);
        set.lit = lit;
        Some(set)
    }

    /// A system from its description and the layers it draws with, fresh (not started, no time
    /// reckoned); each emitter with room for as many particles as it may have alive. Its random
    /// rolls come from `dice`, the client's one generator.
    // D2-10284, D2-10292, D2-10232, D2-10264, D2-10265, D2-10272, D2-10231
    #[must_use]
    pub fn new(
        desc: Particles,
        layers: Vec<DrawLayer>,
        place: Mat4,
        scale: Vec3,
        created: f64,
        dice: &Random,
    ) -> Self {
        let emitters = desc
            .emitters
            .iter()
            .enumerate()
            .map(|(index, e)| {
                let capacity = if e.emission_limit != 0 && e.emission_limit <= e.max_particles {
                    e.emission_limit
                } else {
                    e.max_particles
                };
                EmitterState {
                    index,
                    data: data_flags(desc.world_space, e),
                    next_keys: next_keys(&e.keyframes),
                    next_point: vec![0; e.keyframes.len()],
                    capacity: if e.active {
                        usize::try_from(capacity).unwrap_or(0)
                    } else {
                        0
                    },
                    particles: Vec::new(),
                    last_think: 0.0,
                    next_birth: 0.0,
                    last_distance_discharge: 0.0,
                    discharged: 0,
                    last_place: Vec3::ZERO,
                    last_extent: Vec3::ZERO,
                    placed: false,
                }
            })
            .collect();
        Self {
            desc: Arc::new(desc),
            layers,
            place,
            scale,
            created,
            start: created,
            started: false,
            emitting: true,
            emitters,
            rng: dice.clone(),
            lit: false,
            lights: vec![ParticleLight {
                origin: Vec3::ZERO,
                color: Color::WHITE,
                range: None,
                flicker: None,
            }],
            repulsors: Vec::new(),
            no_fade: false,
            spent_since: None,
        }
    }

    /// The description the system runs.
    #[must_use]
    pub fn desc(&self) -> &Particles {
        &self.desc
    }

    /// Whether any of its emitters' particles move by repulsors: such a system neither thinks nor
    /// draws while the volumetric fog effects preference is off.
    // D2-10236
    #[must_use]
    pub fn repulsed(&self) -> bool {
        self.desc.emitters.iter().any(|e| {
            e.keyframes
                .iter()
                .any(|k| k.particle_kind == motion::REPULSE)
        })
    }

    /// Whether its material was found.
    #[must_use]
    pub fn has_material(&self) -> bool {
        !self.layers.is_empty()
    }

    /// Whether its material asks for the particles to be lit.
    #[must_use]
    pub fn lit(&self) -> bool {
        self.lit
    }

    /// Where the system stands: its origin and turn.
    #[must_use]
    pub fn place(&self) -> Mat4 {
        self.place
    }

    /// The scale it draws with, standing on an object of scale `object` whose look gives its
    /// systems `particle`: by that alone, unless the system's description scales it with its object.
    #[must_use]
    pub fn drawn_scale(&self, object: Vec3, particle: f32) -> Vec3 {
        if self.desc.scale_type == SCALES_WITH_OBJECT {
            Vec3::new(
                object.x * particle,
                object.y * particle,
                object.z * particle,
            )
        } else {
            Vec3::new(particle, particle, particle)
        }
    }

    /// Whether its description scales it with the object it stands on.
    #[must_use]
    pub fn scales_with_object(&self) -> bool {
        self.desc.scale_type == SCALES_WITH_OBJECT
    }

    /// The scale of the object it stands on.
    #[must_use]
    pub fn scale(&self) -> Vec3 {
        self.scale
    }

    /// How many particles are alive.
    #[must_use]
    pub fn alive(&self) -> usize {
        self.emitters.iter().map(|e| e.particles.len()).sum()
    }

    /// Light the particles of a lit material with `light` alone, alike everywhere (outdoors, the
    /// sun's color times its brightness); unlit materials ignore it. Their alpha stays.
    pub fn set_light(&mut self, light: Color) {
        self.lights = vec![ParticleLight {
            origin: Vec3::ZERO,
            color: light,
            range: None,
            flicker: None,
        }];
    }

    /// Light the particles of a lit material with the lights about the system, in the order they
    /// were kept (the first five light them), their places in the frame the system's place is in;
    /// unlit materials ignore them.
    pub fn set_lights(&mut self, lights: Vec<ParticleLight>) {
        self.lights = lights;
    }

    /// The repulsors pushing the particles this frame (those active about it, as seen this frame
    /// or the last).
    pub fn set_repulsors(&mut self, repulsors: Vec<Push>) {
        self.repulsors = repulsors;
    }

    /// Stop discharging: the particles alive live out their lives.
    pub fn stop(&mut self) {
        self.emitting = false;
    }

    /// Stand the system somewhere else: its object moved. Particles living in the world stay where
    /// they are; the system's own move with it.
    pub fn set_place(&mut self, place: Mat4, scale: Vec3) {
        self.place = place;
        self.scale = scale;
    }

    /// Whether the system is drawn even where its place is not seen (its record asks so).
    // D2-10271
    #[must_use]
    pub fn force_draw(&self) -> bool {
        self.desc.force_draw
    }

    /// Where lights about the system are reckoned from: the place it is drawn at, for a system
    /// whose particles live in its own frame; nowhere in particular (the origin) for one whose
    /// particles live in the world, each of which is lit where it is.
    // D2-10281
    #[must_use]
    pub fn lighting_origin(&self) -> Vec3 {
        if self.desc.world_space {
            Vec3::ZERO
        } else {
            self.place.point(Vec3::ZERO)
        }
    }

    /// Place the emitters at the system's place the first time it is queued, `now` the client's
    /// running time: for a system whose particles live in the world, each emitter not yet placed
    /// starts its distance reckoning there (and a line emitter's far end along its line), so its
    /// first discharge by distance does not span the way from nowhere. A system living in its own
    /// frame never moves in it.
    // D2-10257
    pub fn place_emitters(&mut self, now: f64) {
        if !self.desc.world_space {
            return;
        }
        let place = self.world_origin();
        for k in 0..self.emitters.len() {
            if self.emitters[k].placed {
                continue;
            }
            let extent = if self.emitter(k).shape == 4 {
                let off = self.extent(k, now);
                add(place, self.world().direction(off))
            } else {
                place
            };
            let s = &mut self.emitters[k];
            s.placed = true;
            s.last_place = place;
            s.last_extent = extent;
        }
    }

    /// Whether emitter `k` has nothing more to discharge at `now`: it is not active, its time
    /// limit has passed, or its emission limit is reached.
    // D2-10235
    fn emitter_spent(&self, k: usize, now: f64) -> bool {
        let e = self.emitter(k);
        let end = self.start + f64::from(e.start_time) + f64::from(e.time_limit);
        let running = e.active
            && (e.time_limit.abs() < EPSILON || end > now)
            && (e.emission_limit == 0 || self.emitters[k].discharged < e.emission_limit);
        !running
    }

    /// Whether the system can go at `now`: none of its emitters has anything more to discharge
    /// (or it was stopped), and its last particles have died, or ten thousand seconds have passed
    /// since it ran out.
    // D2-10277
    pub fn ready_to_die(&mut self, now: f64) -> bool {
        if self.emitting && (0..self.emitters.len()).any(|k| !self.emitter_spent(k, now)) {
            return false;
        }
        let since = *self.spent_since.get_or_insert(now);
        !(self.alive() != 0 && now - since <= 10_000.0)
    }

    /// The frame the particles live in: the system's place for one that lives in the world, else
    /// the system's own (the place is applied when it is drawn).
    fn world(&self) -> Mat4 {
        if self.desc.world_space {
            self.place
        } else {
            Mat4::IDENTITY
        }
    }

    fn world_origin(&self) -> Vec3 {
        self.world().point(Vec3::ZERO)
    }

    /// Start the clock: the system's time begins its fast-forward before it was made, and each
    /// emitter's clocks at its own start time.
    // D2-10279, D2-10224
    fn begin(&mut self) {
        self.started = true;
        #[allow(clippy::cast_possible_truncation)] // the client starts in single precision
        let start = f64::from(self.created as f32 - self.desc.fast_forward_time);
        self.start = start;
        for e in &mut self.emitters {
            #[allow(clippy::cast_possible_truncation)]
            let t = f64::from(self.desc.emitters[e.index].start_time + start as f32);
            e.last_think = t;
            e.next_birth = t;
            e.last_distance_discharge = t;
        }
    }

    /// One frame of the client at time `now`: each emitter discharges what is due, then its
    /// particles move if a physics step has passed.
    // D2-10293, D2-10280
    pub fn think(&mut self, now: f64) {
        if !self.started {
            self.begin();
        }
        let now = now.max(self.created);
        let duration = f64::from(self.desc.physics_duration);
        if duration != 0.0 && now - self.start > duration {
            return;
        }
        for k in 0..self.emitters.len() {
            if self.emitting {
                self.emit(k, now);
            }
            self.think_emitter(k, now);
        }
    }

    /// Run the system from its making to `until`, a frame every `frame` seconds.
    pub fn run(&mut self, until: f64, frame: f64) {
        let frame = frame.max(1e-3);
        let mut t = self.created;
        self.place_emitters(t);
        loop {
            self.think(t);
            if t >= until {
                break;
            }
            t = (t + frame).min(until);
        }
    }

    // D2-10234
    fn emitter(&self, k: usize) -> &Emitter {
        &self.desc.emitters[self.emitters[k].index]
    }

    /// The line emitter's reach at time `t`: its direction, unit, times its scale.
    fn extent(&mut self, k: usize, t: f64) -> Vec3 {
        let desc = Arc::clone(&self.desc);
        let e = &desc.emitters[self.emitters[k].index];
        let d = unit_or_zero(wave3(&e.direction, t, &self.rng));
        scale(d, waveform_at(&e.scale, t, &self.rng))
    }

    // D2-10268
    fn emit(&mut self, k: usize, now: f64) {
        let desc = Arc::clone(&self.desc);
        let e = &desc.emitters[self.emitters[k].index];
        if !e.active {
            return;
        }
        let since = now - self.start;
        let started = f64::from(e.start_time) <= since;
        let limit_end = f64::from(e.time_limit) + f64::from(e.start_time);
        if started && (e.time_limit.abs() < EPSILON || since <= limit_end) {
            self.emit_timed(k, now);
            self.emit_by_distance(k, now);
        } else if !started {
            let place = self.world_origin();
            let extent = if e.shape == 4 {
                let off = self.extent(k, now);
                add(place, self.world().direction(off))
            } else {
                place
            };
            let s = &mut self.emitters[k];
            s.last_place = place;
            s.last_extent = extent;
        }
    }

    /// The particles due by the birth interval: a blast every interval from the next birth up to
    /// `now`, skipping those that would already have died, within the room left and the limit.
    // D2-10266
    fn emit_timed(&mut self, k: usize, now: f64) {
        let desc = Arc::clone(&self.desc);
        let e = &desc.emitters[self.emitters[k].index];
        if now < self.emitters[k].next_birth || e.birth_rate == 0.0 {
            return;
        }
        let rate = f64::from(e.birth_rate);
        let elapsed = now - self.emitters[k].next_birth;
        if dereth_primitives::num::to_i32_f64(elapsed / rate)
            .wrapping_mul(i32::try_from(e.blast_count).unwrap_or(0))
            == 0
        {
            return;
        }
        let mut room = self.emitters[k]
            .capacity
            .saturating_sub(self.emitters[k].particles.len());
        let life = f64::from(waveform_at(
            &e.lifespan,
            self.emitters[k].next_birth,
            &self.rng,
        ));
        if life < elapsed {
            let _ = waveform_at(&e.lifespan, self.emitters[k].next_birth, &self.rng);
            let skip_to = now - life;
            let skipped =
                dereth_primitives::num::to_i32_f64((skip_to - self.emitters[k].next_birth) / rate);
            let s = &mut self.emitters[k];
            s.next_birth = skip_to;
            s.discharged = s.discharged.wrapping_add(
                e.blast_count
                    .wrapping_mul(u32::try_from(skipped).unwrap_or(0)),
            );
        }
        let limit = e.emission_limit;
        if !((limit == 0 || self.emitters[k].discharged <= limit)
            && self.emitters[k].next_birth < now)
        {
            return;
        }
        loop {
            if room == 0 {
                self.emitters[k].next_birth = now;
                return;
            }
            let mut count = i64::from(e.blast_count);
            let discharged = i64::from(self.emitters[k].discharged);
            if limit != 0 && i64::from(limit) < discharged + count {
                count = i64::from(limit) - discharged;
            }
            if count < 1 {
                break;
            }
            let count = usize::try_from(count).unwrap_or(0).min(room);
            self.emitters[k].discharged = self.emitters[k]
                .discharged
                .wrapping_add(u32::try_from(count).unwrap_or(0));
            let extent = if e.shape == 4 {
                self.extent(k, now)
            } else {
                Vec3::ZERO
            };
            let birth = self.emitters[k].next_birth;
            for _ in 0..count {
                self.discharge(k, now, birth, Vec3::ZERO, extent);
            }
            room -= count;
            #[allow(clippy::cast_possible_truncation)] // the client adds in single precision
            let next = f64::from(e.birth_rate + self.emitters[k].next_birth as f32);
            self.emitters[k].next_birth = next;
            if now <= next {
                return;
            }
        }
        self.emitters[k].next_birth = now;
    }

    /// The particles due by distance: a blast every emission distance the system has moved since
    /// the last one, spread back along the way it came.
    // D2-10267
    fn emit_by_distance(&mut self, k: usize, now: f64) {
        let desc = Arc::clone(&self.desc);
        let e = &desc.emitters[self.emitters[k].index];
        let previous = self.emitters[k].last_distance_discharge;
        self.emitters[k].last_distance_discharge = now;
        #[allow(clippy::cast_possible_truncation)]
        let mut time_step = (previous - now) as f32;
        if e.emission_distance == 0.0
            || (e.emission_limit != 0 && self.emitters[k].discharged > e.emission_limit)
        {
            return;
        }
        let place = self.world_origin();
        let extent_now = if e.shape == 4 {
            let off = self.extent(k, now);
            add(place, self.world().direction(off))
        } else {
            place
        };
        let moved = sub(place, self.emitters[k].last_place);
        let moved_extent = sub(extent_now, self.emitters[k].last_extent);
        let (len, len_extent) = (length(moved), length(moved_extent));
        if len < EPSILON && len_extent < EPSILON {
            return;
        }
        let n = dereth_primitives::num::to_i32(len.max(len_extent) / e.emission_distance);
        if n == 0 {
            return;
        }
        #[allow(clippy::cast_precision_loss)]
        let inv = 1.0 / n as f32;
        time_step *= inv;
        let step = if len < EPSILON {
            Vec3::ZERO
        } else {
            scale(moved, inv)
        };
        let step_extent = if len_extent < EPSILON {
            Vec3::ZERO
        } else {
            scale(moved_extent, inv)
        };
        let mut room = self.emitters[k]
            .capacity
            .saturating_sub(self.emitters[k].particles.len());
        if room == 0 {
            return;
        }
        let mut birth = previous;
        loop {
            birth += f64::from(time_step);
            let s = &self.emitters[k];
            if length(sub(place, s.last_place)) < e.emission_distance
                && length(sub(extent_now, s.last_extent)) < e.emission_distance
            {
                return;
            }
            let mut count = i64::from(e.blast_count);
            let discharged = i64::from(s.discharged);
            if e.emission_limit != 0 && i64::from(e.emission_limit) < discharged + count {
                count = i64::from(e.emission_limit) - discharged;
            }
            let s = &mut self.emitters[k];
            s.last_place = add(s.last_place, step);
            s.last_extent = add(s.last_extent, step_extent);
            if count < 1 {
                break;
            }
            let count = usize::try_from(count).unwrap_or(0).min(room);
            s.discharged = s.discharged.wrapping_add(u32::try_from(count).unwrap_or(0));
            let reach = sub(s.last_extent, s.last_place);
            let back = sub(s.last_place, place);
            let local_reach = self.world_inverse_direction(reach);
            for _ in 0..count {
                birth = birth.min(now);
                self.discharge(k, now, birth, back, local_reach);
            }
            room -= count;
            if room == 0 {
                return;
            }
        }
        self.emitters[k].last_distance_discharge = now;
    }

    fn world_inverse_direction(&self, v: Vec3) -> Vec3 {
        let m = self.world().0;
        // the rotation's rows are the axes' images: its inverse takes the dot with each
        Vec3::new(
            v.x * m[0][0] + v.y * m[0][1] + v.z * m[0][2],
            v.x * m[1][0] + v.y * m[1][1] + v.z * m[1][2],
            v.x * m[2][0] + v.y * m[2][1] + v.z * m[2][2],
        )
    }

    /// Where in the emitter's shape a particle starts, about its origin: a point, a square (on
    /// its edges unless the shape is filled), a disc or a ball (on their rims unless filled), or
    /// along the line of `extent`.
    // D2-10238
    fn initial_origin(&mut self, k: usize, t: f64, extent: Vec3) -> Vec3 {
        let desc = Arc::clone(&self.desc);
        let e = &desc.emitters[self.emitters[k].index];
        let size = waveform_at(&e.scale, t, &self.rng);
        let rng = &self.rng;
        match e.shape {
            1 => {
                let (lo, hi) = (size * -0.5, size * 0.5);
                let y = rng.roll_float(lo, hi);
                let x = rng.roll_float(lo, hi);
                let mut p = Vec3::new(x, y, 0.0);
                if !e.inclusive_shape {
                    match rng.roll_int(0, 3) {
                        0 => p.x = lo,
                        1 => p.y = lo,
                        2 => p.x = hi,
                        3 => p.y = hi,
                        _ => {}
                    }
                }
                p
            }
            2 | 3 => {
                let p = if e.shape == 2 {
                    let y = rng.roll_float(-1.0, 1.0);
                    let x = rng.roll_float(-1.0, 1.0);
                    Vec3::new(x, y, 0.0)
                } else {
                    let z = rng.roll_float(-1.0, 1.0);
                    let y = rng.roll_float(-1.0, 1.0);
                    let x = rng.roll_float(-1.0, 1.0);
                    Vec3::new(x, y, z)
                };
                let mut p = unit_or_zero(p);
                if e.inclusive_shape {
                    p = scale(p, rng.roll_float(0.0, 1.0));
                }
                scale(p, size * 0.5)
            }
            4 => {
                if dot(extent, extent) < EPSILON {
                    return Vec3::ZERO;
                }
                scale(unit_or_zero(extent), rng.roll_float(0.0, size))
            }
            _ => Vec3::ZERO,
        }
    }

    /// A particle born at `birth`: placed in the emitter's shape, sent along the emitter's
    /// direction (or outward, for an exploding emitter) turned into the spread's cone at its
    /// velocity, with its size, turn and streak; then brought to the emitter's last physics step.
    // D2-10262
    fn discharge(&mut self, k: usize, now: f64, birth: f64, offset: Vec3, extent: Vec3) {
        let desc = Arc::clone(&self.desc);
        let e = &desc.emitters[self.emitters[k].index];
        #[allow(clippy::cast_possible_truncation)]
        let t = f64::from(birth as f32 - (e.start_time + self.start as f32));
        let life = waveform_at(&e.lifespan, t, &self.rng);
        #[allow(clippy::cast_possible_truncation)]
        let dies = life + birth as f32;
        #[allow(clippy::cast_possible_truncation)]
        if dies <= now as f32 || self.emitters[k].particles.len() >= self.emitters[k].capacity {
            return;
        }
        let world = self.world();
        let mut dir = world.direction(wave3(&e.direction, t, &self.rng));
        let mut local = self.initial_origin(k, t, extent);
        if e.exploding {
            dir = local;
            if local.x.abs() < EPSILON && local.y.abs() < EPSILON && local.z.abs() < EPSILON {
                let z = self.rng.roll_float(-1.0, 1.0);
                let y = self.rng.roll_float(-1.0, 1.0);
                let x = self.rng.roll_float(-1.0, 1.0);
                dir = Vec3::new(x, y, z);
            }
        }
        let l = length(dir);
        dir = if l < EPSILON {
            Vec3::new(0.0, 1.0, 0.0)
        } else {
            scale(dir, 1.0 / l)
        };
        local = add(local, wave3(&e.origin, t, &self.rng));
        let lo = waveform_at(&e.min_spread, t, &self.rng);
        let hi = waveform_at(&e.max_spread, t, &self.rng);
        if hi - lo > EPSILON {
            let angle = self.rng.roll_float(lo, hi) * 0.017_453_292;
            if angle != 0.0 {
                let (right, _) = look_frame(dir);
                let tipped = turn_about(dir, right, angle);
                let spin = self.rng.roll_float(0.0, 6.283_185_5);
                dir = turn_about(tipped, dir, spin);
            }
        }
        let origin = add(world.point(local), offset);
        let velocity = scale(dir, waveform_at(&e.velocity, t, &self.rng));
        let flags = self.emitters[k].data;
        let mut p = Particle {
            created: birth,
            start_origin: origin,
            origin,
            previous_origin: origin,
            world_origin: world.point(Vec3::ZERO),
            velocity,
            scale: 1.0,
            switches: 0,
            current_key: None,
            last: [None; 4],
            waveform: None,
            rotation: Vec3::ZERO,
            start_rotation: Vec3::ZERO,
            rotate_velocity: Vec3::ZERO,
            world_rotation: Vec3::ZERO,
            start_world_rotation: Vec3::ZERO,
            point_index: None,
            point_speed: length(velocity),
            streak: 0.0,
            streak_origin: origin,
            previous_streak_origin: origin,
        };
        if flags & data::STREAK != 0 {
            p.streak = waveform_at(&e.streak, t, &self.rng);
        }
        if flags & data::SCALE != 0 {
            p.scale = waveform_at(&e.particle_scale, t, &self.rng);
        }
        if flags & data::ROTATE_VELOCITY != 0 {
            p.rotate_velocity = wave3(&e.rotate_velocity, t, &self.rng);
        }
        if flags & data::ROTATION != 0 {
            p.rotation = wave3(&e.rotation, t, &self.rng);
            p.start_rotation = p.rotation;
        }
        if flags & data::WORLD_ROTATION != 0 {
            p.world_rotation = wave3(&e.world_rotation, t, &self.rng);
            p.start_world_rotation = p.world_rotation;
        }
        let last_think = self.emitters[k].last_think;
        #[allow(clippy::cast_possible_truncation)]
        let dt = (last_think - birth) as f32;
        self.particle_think(k, &mut p, dt, last_think);
        p.previous_origin = p.origin;
        p.previous_streak_origin = p.streak_origin;
        self.emitters[k].particles.push(p);
    }

    /// One particle of emitter `k` let go at `now` at `at`, whatever it would have discharged by
    /// itself. An emitter the system does not have lets nothing go.
    // D2-10262
    pub fn discharge_at(&mut self, k: usize, now: f64, at: Vec3) {
        if k < self.emitters.len() {
            self.discharge(k, now, now, at, Vec3::ZERO);
        }
    }

    /// The system draws whole however far off it is.
    // D2-15840
    pub fn set_no_fade(&mut self, on: bool) {
        self.no_fade = on;
    }

    /// The emitter's particles moved to `now`, once a physics step has passed since the last
    /// time; those past their lifespan go (the last one takes the place of each that goes).
    // D2-10261
    fn think_emitter(&mut self, k: usize, now: f64) {
        let last = self.emitters[k].last_think;
        // The time passed is held to the step at full precision, not as it is handed on.
        if now - last < f64::from(self.desc.physics_time_step) {
            return;
        }
        #[allow(clippy::cast_possible_truncation)]
        let dt = (now - last) as f32;
        let mut i = 0;
        while i < self.emitters[k].particles.len() {
            let mut p = self.emitters[k].particles[i].clone();
            if self.particle_think(k, &mut p, dt, now) {
                self.emitters[k].particles[i] = p;
                i += 1;
            } else {
                self.emitters[k].particles.swap_remove(i);
            }
        }
        self.emitters[k].last_think = now;
    }

    /// One particle brought to `now`: dead past its lifespan; else its keyframes passed, then
    /// moved by the motion of its last motion keyframe, and turned.
    // D2-10260
    fn particle_think(&mut self, k: usize, p: &mut Particle, dt: f32, now: f64) -> bool {
        let desc = Arc::clone(&self.desc);
        let e = &desc.emitters[self.emitters[k].index];
        let flags = self.emitters[k].data;
        #[allow(clippy::cast_possible_truncation)]
        let age = (now - p.created) as f32;
        #[allow(clippy::cast_possible_truncation)]
        let begun = self.start as f32 + e.start_time;
        let life = waveform_at(&e.lifespan, p.created - f64::from(begun), &self.rng);
        if life < age {
            return false;
        }
        let (mut kind, mut key_time) = (0, 0.0);
        if flags & data::KEYFRAMES != 0 {
            self.apply_keyframes(k, p, now);
            if let Some(m) = p.last[3] {
                kind = e.keyframes[m].particle_kind;
                key_time = e.keyframes[m].time;
            }
        }
        if flags & data::PREVIOUS_ORIGIN != 0 {
            p.previous_origin = p.origin;
        }
        if flags & data::PREVIOUS_STREAK_ORIGIN != 0 {
            p.previous_streak_origin = p.streak_origin;
        }
        let since_key = age - key_time;
        match kind {
            motion::PHYSICS => self.physics(k, p, since_key, age),
            motion::WAVEFORM => self.waveform_motion(k, p, since_key),
            motion::REPULSE => self.repulse(k, p, age, now),
            motion::POINTS => self.points(k, p),
            motion::POINTS_BLENDED => {
                #[allow(clippy::cast_possible_truncation)]
                let dt = (now - self.emitters[k].last_think) as f32;
                self.points_blended(k, p, dt);
            }
            _ => {}
        }
        if flags & data::ROTATE_VELOCITY != 0 {
            let turned = scale(p.rotate_velocity, age);
            if flags & data::ROTATION != 0 {
                p.rotation = add(p.start_rotation, turned);
            }
            if flags & data::WORLD_ROTATION != 0 {
                p.world_rotation = add(p.start_world_rotation, turned);
            }
        }
        if dt > 0.25 {
            if flags & data::PREVIOUS_ORIGIN != 0 {
                p.previous_origin = p.origin;
            }
            if flags & data::PREVIOUS_STREAK_ORIGIN != 0 {
                p.previous_streak_origin = p.streak_origin;
            }
        }
        true
    }

    /// Every keyframe the particle has reached since the last one it passed.
    // D2-10259
    fn apply_keyframes(&mut self, k: usize, p: &mut Particle, now: f64) {
        let desc = Arc::clone(&self.desc);
        let e = &desc.emitters[self.emitters[k].index];
        #[allow(clippy::cast_possible_truncation)]
        let age = ((now - p.created) as f32).max(0.0);
        let n = e.keyframes.len();
        let mut i = p.current_key.map_or(0, |c| c + 1);
        while i < n {
            if age < e.keyframes[i].time {
                p.current_key = i.checked_sub(1);
                break;
            }
            self.apply_keyframe(k, p, i);
            i += 1;
        }
        if i == n {
            p.current_key = n.checked_sub(1);
        }
    }

    /// One keyframe reached: at a motion keyframe the motion so far is folded into the start
    /// position (and turn), and the keyframe becomes the one the motion follows; the keyframe is
    /// remembered for each value it sets or blends; it may change the gravity and wind switches.
    // D2-10258
    fn apply_keyframe(&mut self, k: usize, p: &mut Particle, i: usize) {
        let desc = Arc::clone(&self.desc);
        let e = &desc.emitters[self.emitters[k].index];
        let flags = self.emitters[k].data;
        let kf = &e.keyframes[i];
        if flags & data::KEYFRAMES != 0 {
            if kf.key_flags & key::MOTION != 0 {
                if let Some(m) = p.last[3] {
                    let prev = &e.keyframes[m];
                    let span = kf.time - prev.time;
                    match prev.particle_kind {
                        motion::PHYSICS => {
                            let mass = p.last[1].map_or(1.0, |j| e.keyframes[j].mass);
                            let pull = self.pull(p.switches, mass);
                            let moved =
                                add(scale(p.velocity, span), scale(pull, span * span * 0.5));
                            p.start_origin = add(p.start_origin, moved);
                        }
                        motion::WAVEFORM => {
                            if let Some(w) = p.waveform {
                                let off = wave3(&w, f64::from(span), &self.rng);
                                p.start_origin = add(p.start_origin, off);
                            }
                        }
                        motion::POINTS => {
                            if !prev.points.is_empty() {
                                if p.point_index.is_none() {
                                    let next = self.emitters[k].next_point[m];
                                    p.point_index = Some(next);
                                    let count = u16::try_from(prev.points.len()).unwrap_or(1);
                                    self.emitters[k].next_point[m] = (next + 1) % count.max(1);
                                }
                                let at = usize::from(p.point_index.unwrap_or(0));
                                let point = prev.points.get(at).copied().unwrap_or(Vec3::ZERO);
                                p.start_origin = add(p.start_origin, self.world().direction(point));
                            }
                        }
                        motion::POINTS_BLENDED => {
                            #[allow(clippy::cast_possible_truncation)]
                            let dt = (p.created + f64::from(kf.time) - self.emitters[k].last_think)
                                as f32;
                            self.points_blended(k, p, dt);
                            p.start_origin = p.origin;
                        }
                        _ => {}
                    }
                    if flags & data::ROTATE_VELOCITY != 0 {
                        let turned = scale(p.rotate_velocity, kf.time);
                        if flags & data::START_WORLD_ROTATION != 0 {
                            p.start_world_rotation = add(p.start_world_rotation, turned);
                        } else if flags & data::START_ROTATION != 0 {
                            p.start_rotation = add(p.start_rotation, turned);
                        }
                    }
                }
                if kf.particle_kind == motion::WAVEFORM {
                    p.waveform = Some(kf.position);
                }
                p.last[3] = Some(i);
            }
            if kf.key_flags & key::SCALE != 0 {
                p.last[0] = Some(i);
            }
            if kf.key_flags & key::COLOR != 0 {
                p.last[2] = Some(i);
            }
            if kf.key_flags & key::MASS != 0 {
                p.last[1] = Some(i);
            }
        }
        if flags & data::KEYFRAME_FLAGS != 0 && kf.key_flags & key::SWITCHES != 0 {
            p.switches = kf.keyframe_flags;
        }
    }

    /// The pull on a particle of `mass` under its switches: gravity, and no wind.
    // D2-10273
    fn pull(&self, switches: u32, mass: f32) -> Vec3 {
        let mut a = Vec3::ZERO;
        if switches & pull::GRAVITY != 0 {
            a = add(a, scale(GRAVITY, mass));
        }
        // the client's wind is always still
        let _ = pull::WIND;
        a
    }

    /// The mass at `age`: blended from the last mass keyframe toward the next one that blends it.
    // D2-10240
    fn mass(&self, k: usize, p: &Particle, age: f32) -> f32 {
        let e = self.emitter(k);
        if self.emitters[k].data & data::KEYFRAMES == 0 {
            return 1.0;
        }
        let Some(a) = p.last[1] else {
            return 1.0;
        };
        let b = self.emitters[k].next_keys[a][1];
        let (ka, kb) = (&e.keyframes[a], &e.keyframes[b]);
        let span = kb.time - ka.time;
        let f = if span.abs() < EPSILON {
            0.0
        } else {
            ((age - ka.time) / span).clamp(0.0, 1.0)
        };
        kb.mass * f + (1.0 - f) * ka.mass
    }

    /// Thrown: from its start position along its velocity, pulled by gravity times its mass.
    // D2-10241
    fn physics(&self, k: usize, p: &mut Particle, t: f32, age: f32) {
        let mass = self.mass(k, p, age);
        let pull = self.pull(p.switches, mass);
        let velocity = p.velocity;
        let at = |t: f32| add(scale(velocity, t), scale(pull, t * t * 0.5));
        p.origin = add(p.start_origin, at(t));
        if self.emitters[k].data & data::STREAK_ORIGIN != 0 {
            // the tail is where the head was the streak's time ago
            p.streak_origin = add(p.start_origin, at((t - p.streak).max(0.0)));
        }
    }

    /// Along the motion keyframe's waveforms, from its start position.
    // D2-10239
    fn waveform_motion(&mut self, k: usize, p: &mut Particle, t: f32) {
        if let Some(w) = p.waveform {
            p.origin = add(p.start_origin, wave3(&w, f64::from(t), &self.rng));
            if self.emitters[k].data & data::STREAK_ORIGIN != 0 {
                let tail = f64::from((t - p.streak).max(0.0));
                p.streak_origin = add(p.start_origin, wave3(&w, tail, &self.rng));
            }
        }
    }

    /// Pushed by repulsors: stepped each physics step under gravity and each repulsor within its
    /// range (pushing along the way from it, harder the nearer, `a (1 - d / range)`; pulling for an
    /// attractor; not along the emitter's constrained axes), its velocity kept at 95 percent, and
    /// drawn back toward its start by the emitter's snap.
    // D2-10242
    fn repulse(&mut self, k: usize, p: &mut Particle, age: f32, now: f64) {
        let desc = Arc::clone(&self.desc);
        let e = &desc.emitters[self.emitters[k].index];
        let mass = self.mass(k, p, age);
        let step = self.desc.physics_time_step;
        let mut t = self.emitters[k].last_think;
        if now - t > 0.25 {
            t = now - 0.25;
        }
        while f64::from(step) <= now - t {
            t += f64::from(step);
            let mut pull = self.pull(p.switches, mass);
            for r in &self.repulsors {
                pull = add(pull, push_of(r, p.origin, e.constrain_axes));
            }
            let moved = add(scale(pull, step), scale(p.velocity, step));
            let snap = waveform_at(&e.particle_snap, f64::from(age), &self.rng) * step;
            let back = sub(p.start_origin, p.origin);
            p.origin = add(add(p.origin, moved), scale(back, snap));
            p.velocity = scale(moved, 0.95 / step);
        }
    }

    /// At one of the motion keyframe's points (in turn, or at random), about its start position.
    // D2-10243
    fn points(&mut self, k: usize, p: &mut Particle) {
        let Some(m) = p.last[3] else {
            return;
        };
        let desc = Arc::clone(&self.desc);
        let e = &desc.emitters[self.emitters[k].index];
        let kf = &e.keyframes[m];
        if kf.points.is_empty() {
            p.origin = p.start_origin;
            return;
        }
        let at = if let Some(i) = p.point_index {
            i
        } else {
            let next = self.emitters[k].next_point[m];
            p.point_index = Some(next);
            let count = u16::try_from(kf.points.len()).unwrap_or(1).max(1);
            self.emitters[k].next_point[m] = if p.switches & pull::RANDOM_POINT == 0 {
                (next + 1) % count
            } else {
                u16::try_from(self.rng.roll_int(0, i32::from(count) - 1)).unwrap_or(0)
            };
            next
        };
        let point = kf
            .points
            .get(usize::from(at))
            .copied()
            .unwrap_or(Vec3::ZERO);
        p.origin = add(p.start_origin, self.world().direction(point));
        if self.emitters[k].data & data::STREAK_ORIGIN != 0 {
            p.streak_origin = p.origin;
        }
    }

    /// Along the motion keyframe's points at the particle's own speed over `dt`, one after
    /// another (the speed's reach is measured against squared distances, as the client does).
    // D2-10244
    fn points_blended(&mut self, k: usize, p: &mut Particle, dt: f32) {
        let Some(m) = p.last[3] else {
            return;
        };
        let desc = Arc::clone(&self.desc);
        let e = &desc.emitters[self.emitters[k].index];
        let kf = &e.keyframes[m];
        if kf.points.is_empty() {
            p.origin = p.start_origin;
            return;
        }
        let n = kf.points.len();
        if p.point_index.is_none() {
            let first = if p.switches & pull::RANDOM_POINT != 0 {
                u16::try_from(
                    self.rng
                        .roll_int(0, i32::try_from(n).unwrap_or(1).saturating_sub(1)),
                )
                .unwrap_or(0)
            } else {
                0
            };
            p.point_index = Some(first);
            self.points(k, p);
        }
        let world = self.world();
        let mut i = usize::from(p.point_index.unwrap_or(0)) % n;
        let mut budget = p.point_speed * dt;
        // Points that all coincide would hold the client here forever; a lap is enough.
        let mut laps = 0;
        while budget > EPSILON && laps <= n {
            laps += 1;
            let target = add(p.start_origin, world.direction(kf.points[i]));
            let to = sub(target, p.origin);
            let d2 = dot(to, to);
            if budget <= d2 {
                let d = d2.sqrt();
                p.origin = if d < EPSILON {
                    target
                } else {
                    add(p.origin, scale(to, budget.sqrt() / d))
                };
                budget = 0.0;
            } else {
                p.origin = target;
                budget -= d2;
            }
            p.point_index = u16::try_from(i).ok();
            i = (i + 1) % n;
        }
    }

    /// How much an emitter's particles show at `now`: faded in after the fast-forward over the
    /// fade-in time, faded out from its fade-out time to its time limit.
    // D2-10251
    fn fade(&self, k: usize, now: f64) -> f32 {
        let e = self.emitter(k);
        let ff = self.desc.fast_forward_time;
        let fade_in = ff + e.fade_in;
        let fade_out = ff + e.fade_out;
        #[allow(clippy::cast_possible_truncation)]
        let t = now as f32 - (e.start_time + self.start as f32);
        let mut f = 1.0;
        if fade_in > EPSILON && t > ff && t < fade_in {
            f = if (fade_in - ff).abs() < EPSILON {
                0.0
            } else {
                ((t - ff) / (fade_in - ff)).clamp(0.0, 1.0)
            };
        }
        if e.time_limit > EPSILON && fade_out > EPSILON && fade_out < t {
            let span = e.time_limit - fade_out;
            f = if span.abs() < EPSILON {
                1.0
            } else {
                (1.0 - (t - fade_out) / span).clamp(0.0, 1.0)
            };
        }
        f
    }

    /// The size and color of a particle at `age`: each blended from its last keyframe toward the
    /// next that blends it, or held.
    fn look(&self, k: usize, p: &Particle, age: f32) -> ((f32, f32), Color) {
        let e = self.emitter(k);
        let s = &self.emitters[k];
        let blend = |a: usize, b: usize, bit: u32| {
            let (ka, kb) = (&e.keyframes[a], &e.keyframes[b]);
            if kb.key_flags & bit == 0 {
                return None;
            }
            let span = kb.time - ka.time;
            let f = if span <= EPSILON {
                1.0
            } else {
                ((age - ka.time) / span).clamp(0.0, 1.0)
            };
            Some(f)
        };
        let size = p.last[0].map_or((1.0, 1.0), |a| {
            let b = s.next_keys[a][0];
            let (ka, kb) = (&e.keyframes[a], &e.keyframes[b]);
            match blend(a, b, key::SCALE_TO) {
                Some(f) => (
                    f * kb.scale_x + (1.0 - f) * ka.scale_x,
                    f * kb.scale_y + (1.0 - f) * ka.scale_y,
                ),
                None => (ka.scale_x, ka.scale_y),
            }
        });
        let color = p.last[2].map_or(Color::WHITE, |a| {
            let b = s.next_keys[a][2];
            let (ka, kb) = (&e.keyframes[a].color, &e.keyframes[b].color);
            match blend(a, b, key::COLOR_TO) {
                Some(f) => Color::new(
                    kb.r * f + ka.r * (1.0 - f),
                    kb.g * f + ka.g * (1.0 - f),
                    kb.b * f + ka.b * (1.0 - f),
                    ka.a * (1.0 - f) + kb.a * f,
                ),
                None => Color::new(ka.r, ka.g, ka.b, ka.a),
            }
        });
        (size, color)
    }

    /// What the system draws at `now` seen from `eye`, whose right, forward and up are given: one
    /// quad per particle about its position (blended between its last two physics steps), sized
    /// and colored by its keyframes, faded with distance, drawn with the system's material. A
    /// system past its stop-fade distance draws nothing.
    // D2-10288, D2-10287, D2-10251, D2-10228, D2-10230, D2-10227, D2-10226, D2-10246
    // D2-10225, D2-10237, D2-10245, D2-10247, D2-10248, D2-10249
    #[must_use]
    pub fn batches(
        &self,
        now: f64,
        eye: Vec3,
        right: Vec3,
        forward: Vec3,
        up: Vec3,
    ) -> Vec<ObjectBatch> {
        self.batches_at(self.place, self.scale, now, eye, right, forward, up)
    }

    /// What the system draws at `now` standing at `place` with the object's `scale`, seen from
    /// `eye` (a system serving several objects draws at each), as [`ParticleSet::batches`].
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn batches_at(
        &self,
        place: Mat4,
        scale3: Vec3,
        now: f64,
        eye: Vec3,
        right: Vec3,
        forward: Vec3,
        up: Vec3,
    ) -> Vec<ObjectBatch> {
        let place_origin = place.point(Vec3::ZERO);
        let distance = length(sub(place_origin, eye));
        if !self.no_fade && distance > self.desc.stop_fade_distance {
            return Vec::new();
        }
        let mut opacity = 1.0;
        let span = self.desc.stop_fade_distance - self.desc.start_fade_distance;
        if !self.no_fade && distance > self.desc.start_fade_distance && span > EPSILON {
            opacity = 1.0 - (distance - self.desc.start_fade_distance) / span;
        }
        // A system in the world draws its particles where they are; one of its own draws them at
        // its place, by the object's scale, without the place's turn, so its quads face the camera
        // however the object is turned.
        let (to_world, size_scale) = if self.desc.world_space {
            let s = scale3;
            (Mat4::IDENTITY, s.x.max(s.y).max(s.z))
        } else {
            let s = Mat4([
                [scale3.x, 0.0, 0.0, 0.0],
                [0.0, scale3.y, 0.0, 0.0],
                [0.0, 0.0, scale3.z, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ]);
            (s.then(&Mat4::translation(place_origin)), 1.0)
        };
        let world_scale = if self.desc.world_space {
            scale3
        } else {
            Vec3::new(1.0, 1.0, 1.0)
        };
        let lighting_origin = self.lighting_origin();
        let mut verts: Vec<MeshVertex> = Vec::new();
        for (k, s) in self.emitters.iter().enumerate() {
            if self.emitter(k).particle_shape != 4 && self.emitter(k).particle_shape != 3 {
                continue;
            }
            let corners = if self.emitter(k).particle_shape == 3 {
                3
            } else {
                4
            };
            let fade = self.fade(k, now);
            #[allow(clippy::cast_possible_truncation)]
            let blend = ((now - s.last_think) / f64::from(self.desc.physics_time_step)) as f32;
            for p in &s.particles {
                let mut at = if s.data & data::PREVIOUS_ORIGIN != 0 {
                    add(
                        scale(p.previous_origin, 1.0 - blend),
                        scale(p.origin, blend),
                    )
                } else {
                    p.origin
                };
                #[allow(clippy::cast_possible_truncation)]
                let age = (now - p.created) as f32;
                let ((sx, sy), mut color) = if s.data & (data::KEYFRAMES | data::TIME_CREATED) != 0
                {
                    self.look(k, p, age)
                } else {
                    ((1.0, 1.0), Color::WHITE)
                };
                let mut sx = sx * size_scale;
                let mut sy = sy * size_scale;
                if s.data & data::SCALE != 0 {
                    sx *= p.scale;
                    sy *= p.scale;
                }
                color.a *= fade;
                if s.data & data::WORLD_ORIGIN != 0 {
                    let o = p.world_origin;
                    at = Vec3::new(
                        o.x + (at.x - o.x) * world_scale.x,
                        o.y + (at.y - o.y) * world_scale.y,
                        o.z + (at.z - o.z) * world_scale.z,
                    );
                }
                // Each lit particle is lit where it is, taken from the lighting origin (a system
                // of its own frame is lit as if unturned and unscaled there).
                if self.lit {
                    let l = particle_light(&self.lights, add(at, lighting_origin), now, &self.rng);
                    color = Color::new(color.r * l.r, color.g * l.g, color.b * l.b, color.a);
                }
                let color = color.clamped();
                let q = |c: f32| {
                    u32::try_from(dereth_primitives::num::to_i32(c * 255.0)).unwrap_or(0) & 0xFF
                };
                let diffuse = q(color.a) << 24 | q(color.r) << 16 | q(color.g) << 8 | q(color.b);
                let (hx, hy) = (sx * 0.5, sy * 0.5);
                // The corners in the camera's frame (right, along the view, up): top left,
                // bottom left, bottom right, top right.
                let quad = [
                    Vec3::new(-hx, 0.0, hy),
                    Vec3::new(-hx, 0.0, -hy),
                    Vec3::new(hx, 0.0, -hy),
                    Vec3::new(hx, 0.0, hy),
                ];
                const UV: [[f32; 2]; 4] = [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];
                // A streaking particle stretches from its tail to its head instead.
                if s.data & data::STREAK != 0 && s.data & data::WORLD_ROTATION == 0 {
                    let tail = if s.data & data::PREVIOUS_STREAK_ORIGIN != 0 {
                        add(
                            scale(p.previous_streak_origin, 1.0 - blend),
                            scale(p.streak_origin, blend),
                        )
                    } else {
                        p.streak_origin
                    };
                    let view = |q: Vec3| {
                        let d = sub(q, eye);
                        Vec3::new(dot(d, right), dot(d, forward), dot(d, up))
                    };
                    let streaked = streak_corners(view(at), view(tail), hx, hy);
                    for (c, uv) in streaked.iter().zip(UV).take(corners) {
                        let local = add(
                            eye,
                            add(add(scale(right, c.x), scale(forward, c.y)), scale(up, c.z)),
                        );
                        let w = to_world.point(local);
                        verts.push(MeshVertex {
                            position: [w.x, w.y, w.z],
                            normal: [0.0, 0.0, 1.0],
                            diffuse,
                            uv: [uv, uv, uv, uv],
                            tangent_u: [0.0; 3],
                            tangent_v: [0.0; 3],
                            to_light: [0.0; 3],
                        });
                    }
                    continue;
                }
                for (c, uv) in quad.iter().zip(UV).take(corners) {
                    let offset = if s.data & data::WORLD_ROTATION != 0 {
                        // turned in the world, then seen as it is
                        turn(&angle_rows(p.world_rotation), *c)
                    } else {
                        let c = if s.data & data::ROTATION != 0 {
                            turn(&angle_rows(p.rotation), *c)
                        } else {
                            *c
                        };
                        add(add(scale(right, c.x), scale(forward, c.y)), scale(up, c.z))
                    };
                    let local = add(at, offset);
                    verts.push(MeshVertex {
                        position: {
                            let w = to_world.point(local);
                            [w.x, w.y, w.z]
                        },
                        normal: [0.0, 0.0, 1.0],
                        diffuse,
                        uv: [uv, uv, uv, uv],
                        tangent_u: [0.0; 3],
                        tangent_v: [0.0; 3],
                        to_light: [0.0; 3],
                    });
                }
            }
        }
        if verts.is_empty() {
            return Vec::new();
        }
        // Quads as two triangles each (first, second, third; first, third, fourth), in runs a
        // 16-bit index reaches.
        let corners_of = |k: usize| {
            if self.emitter(k).particle_shape == 3 {
                3
            } else {
                4
            }
        };
        let mut sizes: Vec<usize> = Vec::new();
        for (k, s) in self.emitters.iter().enumerate() {
            let shape = self.emitter(k).particle_shape;
            if shape == 3 || shape == 4 {
                sizes.extend(std::iter::repeat_n(corners_of(k), s.particles.len()));
            }
        }
        let tint = Color::new(1.0, 1.0, 1.0, opacity);
        let mut out = Vec::new();
        let mut at = 0;
        let mut run_verts: Vec<MeshVertex> = Vec::new();
        let mut run_idx: Vec<u16> = Vec::new();
        let flush = |run_verts: &mut Vec<MeshVertex>,
                     run_idx: &mut Vec<u16>,
                     out: &mut Vec<ObjectBatch>| {
            if run_idx.is_empty() {
                return;
            }
            let v = Arc::new(std::mem::take(run_verts));
            let i = Arc::new(std::mem::take(run_idx));
            for layer in &self.layers {
                out.push(ObjectBatch {
                    vertices: v.clone(),
                    posed: None,
                    played: None,
                    indices: i.clone(),
                    transform: Mat4::IDENTITY,
                    layer: layer.clone(),
                    tint,
                    points: crate::scene::PointLights::NONE,
                    mesh_type: 0,
                    shade: None,
                    outside: false,
                    cull: crate::scene::Cull::Never,
                    mesh: 0,
                    casts_volume: false,
                    caster: None,
                    seen_through: false,
                    kept_by: None,
                    anchor: None,
                    step: Some(crate::scene::Pass::Particles),
                    visible: None,
                    block: None,
                    far: false,
                    ground: None,
                    lod: None,
                });
            }
        };
        for n in sizes {
            if run_verts.len() + n > usize::from(u16::MAX) {
                flush(&mut run_verts, &mut run_idx, &mut out);
            }
            let base = u16::try_from(run_verts.len()).unwrap_or(0);
            run_verts.extend_from_slice(&verts[at..at + n]);
            if n == 4 {
                run_idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            } else {
                run_idx.extend([base, base + 1, base + 2]);
            }
            at += n;
        }
        flush(&mut run_verts, &mut run_idx, &mut out);
        out
    }
}

/// A streaking particle's corners in the camera's frame (right, along the view, up), from its
/// head `head` to its tail `tail`: the head's two corners on the side away from the tail and the
/// tail's two on the side away from the head, across the way the streak runs on the screen (up
/// and down when it runs more up than across). A streak with no length on the screen is an
/// ordinary quad.
// D2-10229
#[must_use]
pub fn streak_corners(head: Vec3, tail: Vec3, hx: f32, hy: f32) -> [Vec3; 4] {
    let (dx, dz) = (head.x - tail.x, head.z - tail.z);
    let len = (dx * dx + dz * dz).sqrt();
    let at = |p: Vec3, x: f32, z: f32| Vec3::new(p.x + x, p.y, p.z + z);
    if len < EPSILON {
        return [
            at(head, -hx, hy),
            at(head, -hx, -hy),
            at(head, hx, -hy),
            at(head, hx, hy),
        ];
    }
    let (ux, uz) = (dx / len, dz / len);
    if ux.abs() <= uz.abs() {
        if uz > 0.0 {
            [
                at(head, hx, hy),
                at(head, -hx, hy),
                at(tail, -hx, -hy),
                at(tail, hx, -hy),
            ]
        } else {
            [
                at(head, hx, -hy),
                at(head, -hx, -hy),
                at(tail, -hx, hy),
                at(tail, hx, hy),
            ]
        }
    } else if ux <= 0.0 {
        [
            at(head, -hx, hy),
            at(head, -hx, -hy),
            at(tail, hx, -hy),
            at(tail, hx, hy),
        ]
    } else {
        [
            at(head, hx, hy),
            at(head, hx, -hy),
            at(tail, -hx, -hy),
            at(tail, -hx, hy),
        ]
    }
}

/// About how long a system runs, in seconds, or -1 when it runs until stopped: a system with a
/// physics duration, or with an emitter that has neither an emission limit nor a time limit. An
/// emitter's span is its start, its longest lifespan, and the longer of its time limit and the
/// time its emission limit takes in blasts at its birth rate, less the system's fast-forward.
// D2-06945
#[must_use]
pub fn approximate_lifetime(desc: &Particles) -> f32 {
    if desc.physics_duration > 0.0 {
        return -1.0;
    }
    let mut longest = 0.0f32;
    for e in &desc.emitters {
        let mut life = e.lifespan.base;
        if e.lifespan.kind != 1 {
            life += e.lifespan.amplitude.abs();
        }
        #[allow(clippy::float_cmp)]
        if e.emission_limit == 0 && e.time_limit == 0.0 {
            return -1.0;
        }
        // A limit with no blast count would stop the original with a division by zero; no
        // system in the data has one.
        let blasts = e.emission_limit.checked_div(e.blast_count).unwrap_or(0);
        #[allow(clippy::cast_precision_loss)]
        let mut span = blasts as f32 * e.birth_rate;
        if span < e.time_limit {
            span = e.time_limit;
        }
        let total = life + e.start_time - desc.fast_forward_time + span;
        if longest < total {
            longest = total;
        }
    }
    longest
}

/// The camera's right, forward and up for a heading (degrees clockwise from north) and a pitch
/// (degrees above the horizon), as the frame's camera looks.
#[must_use]
pub fn camera_axes(heading: f32, pitch: f32) -> (Vec3, Vec3, Vec3) {
    leaned_camera_axes(heading, pitch, 0.0)
}

/// [`camera_axes`] for a view leaning `roll` degrees about the way it looks.
#[must_use]
pub fn leaned_camera_axes(heading: f32, pitch: f32, roll: f32) -> (Vec3, Vec3, Vec3) {
    let (h, p) = (heading.to_radians(), pitch.to_radians());
    let forward = Vec3::new(
        math::sinf(h) * math::cosf(p),
        math::cosf(h) * math::cosf(p),
        math::sinf(p),
    );
    let (right, up) = crate::scene::view_axes(forward, roll);
    (right, forward, up)
}
