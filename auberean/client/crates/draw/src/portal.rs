//! Portal space: the tunnel a traveller rushes through while the world is out of sight, entering
//! the world and on every trip.
//!
//! The effect runs by state, each phase a second long:
//!
//! | state | drawn | over time `t` in the phase |
//! |---|---|---|
//! | coming in | the world, a black veil over it, then the nebula and the tunnel | the world's view narrows (`1 - 0.8 t²`); the veil, the nebula and the tunnel fade in, the nearest rings last, the camera coming up the tube from 100 m back |
//! | in | the nebula and the tunnel | |
//! | going out | the nebula, the tunnel, white over all | the tunnel's view widens (`1 + 1.4 t²`); the white fades in |
//! | coming out | the world, white over all | the world's view widens and returns (`1 + 1.4 (1 - t)²`); the white fades out |
//!
//! Started at once (entering the world) it goes straight in. While in, the world is not drawn.
//!
//! **The tunnel** is 150 rings 3 m apart, each turned a little from the one before by smooth noise
//! (a new tunnel each time) with a radius of `10 + 6 sin(0.01 d)` at its distance `d` along the
//! tube; the camera rushes along it at 120 m/s, and the rings it passes are made again at the far
//! end. Two tubes are drawn, an outer one 1.2 times as wide behind the inner, each ring's 17 points
//! carrying its distance and its place round the ring as picture coordinates, and an alpha that
//! fades the rings from 1.0 nearest to 0.8 at the sixteenth and on to nothing at the far end. The
//! nebula is a sphere about the camera, turned with the world.

use std::sync::Arc;

use auberean_primitives::random::Random;
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::math::Mat4;
use auberean_world::place::Part;
use auberean_world::records::WorldRecords;
use auberean_world::sky::DayLight;
use dereth_primitives::num::math;

use crate::material::{draw_layers, instance_material, DEVICE_FLAGS};
use crate::mesh::MeshVertex;
use crate::scene::{Camera, Frame, ObjectBatch, SceneBuilder};

/// The rings of the tunnel.
pub const RINGS: usize = 150;
/// Points round a ring, and the seam's copy of the first.
const POINTS: usize = 16;
/// Meters between rings.
pub const SPACING: f32 = 3.0;
/// How fast the camera rushes along the tunnel, meters a second.
pub const SPEED: f32 = 120.0;
/// Each phase's length, seconds.
pub const PHASE: f64 = 1.0;
/// The tunnel camera's own vertical field of view, degrees.
pub const FOV: f32 = 65.0;
/// The inner tube's material, the outer's, and the nebula's mesh.
pub const INNER: u32 = 0x3100_0350;
pub const OUTER: u32 = 0x3100_0351;
pub const NEBULA: u32 = 0x0600_03EC;
/// The outer tube's width against the inner's.
const OUTER_SCALE: f32 = 1.2;
/// How far back along the tube the camera starts as the tunnel fades in, meters.
const PULL_BACK: f32 = 100.0;

/// The sounds of portal space, by the data's enumeration: the one played as it starts, the two
/// kept playing while it lasts, and the one played as it stops.
pub const START_SOUND: u32 = 0x4000_0018;
pub const LOOP_SOUNDS: [u32; 2] = [0x4000_001D, 0x4000_001E];
pub const STOP_SOUND: u32 = 0x4000_001C;
/// The enumeration's mapping.
pub const SOUND_MAPPING: u32 = 0x4000_0002;

/// What the effect asks of the sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassageSound {
    /// It started: its start sound, and its two loops kept.
    Start,
    /// It stopped: its stop sound, and its loops stopped at once.
    Stop,
}

/// Where the effect is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Off,
    ComingIn,
    In,
    GoingOut,
    ComingOut,
}

/// One ring: its distance along the tube, radius and frame (rows: its x, y (along the tube) and z
/// axes, then its place).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Ring {
    at: f32,
    radius: f32,
    frame: Mat4,
}

/// What the effect draws this moment.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub draw_world: bool,
    /// The world's field of view, times its own.
    pub world_fov: f32,
    pub draw_tunnel: bool,
    /// The tunnel's field of view, times its own.
    pub tunnel_fov: f32,
    /// How much of the tunnel shows, 0 to 1.
    pub visibility: f32,
    pub nebula: f32,
    /// The veil: black over the world only, under the tunnel; or white over everything.
    pub under: Option<[f32; 4]>,
    pub over: Option<[f32; 4]>,
}

/// The alpha of the `j`th ring ahead of the camera when the nearest `hidden` are hidden: 1.0 at
/// the nearest shown, falling to 0.8 at the sixteenth and on to nothing at the far end.
// D2-11050
#[must_use]
pub fn ring_alpha(j: usize, hidden: usize) -> f32 {
    if j < hidden {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let x = (j - hidden) as f32;
    if x < 16.0 {
        0.8 + 0.2 * (1.0 - x / 15.0)
    } else {
        0.8 * (1.0 - x / 149.0).powi(2)
    }
}

/// The effect.
#[derive(Clone, Debug)]
pub struct Passage {
    pub state: State,
    want: bool,
    since: f64,
    last: f64,
    /// The camera's distance along the tube, and the ring it last passed.
    at: f32,
    passed: usize,
    rings: Vec<Ring>,
    random: f32,
    dice: Random,
    sounds: Vec<PassageSound>,
}

pub use auberean_world::noise::Noise;

fn rot(axis: usize, a: f32) -> Mat4 {
    let (s, c) = math::sin_cosf(a);
    let m = match axis {
        0 => [[1.0, 0.0, 0.0], [0.0, c, s], [0.0, -s, c]],
        1 => [[c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c]],
        _ => [[c, s, 0.0], [-s, c, 0.0], [0.0, 0.0, 1.0]],
    };
    Mat4([
        [m[0][0], m[0][1], m[0][2], 0.0],
        [m[1][0], m[1][1], m[1][2], 0.0],
        [m[2][0], m[2][1], m[2][2], 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

fn row(m: &Mat4, i: usize) -> Vec3 {
    Vec3::new(m.0[i][0], m.0[i][1], m.0[i][2])
}

fn lerp(a: Vec3, b: Vec3, f: f32) -> Vec3 {
    Vec3::new(
        a.x + (b.x - a.x) * f,
        a.y + (b.y - a.y) * f,
        a.z + (b.z - a.z) * f,
    )
}

fn unit(v: Vec3) -> Vec3 {
    let l = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if l > 0.0 {
        Vec3::new(v.x / l, v.y / l, v.z / l)
    } else {
        v
    }
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

impl Passage {
    /// An effect that is off, rolling its tunnels with `dice`, the client's one generator.
    #[must_use]
    pub fn new(dice: Random) -> Self {
        Self {
            state: State::Off,
            want: false,
            since: 0.0,
            last: 0.0,
            at: 0.0,
            passed: 0,
            rings: Vec::new(),
            random: 0.0,
            dice,
            sounds: Vec::new(),
        }
    }

    /// Roll the tunnels with `dice` from now on.
    pub fn set_dice(&mut self, dice: Random) {
        self.dice = dice;
    }

    /// The rings ahead of the camera, nearest first: each one's distance along the tube, radius and
    /// place.
    #[must_use]
    pub fn rings(&self) -> Vec<(f32, f32, Vec3)> {
        (0..self.rings.len())
            .map(|k| {
                let r = &self.rings[(self.passed + k) % RINGS];
                (r.at, r.radius, row(&r.frame, 3))
            })
            .collect()
    }

    /// How far along the tube the camera is.
    #[must_use]
    pub fn distance(&self) -> f32 {
        self.at
    }

    /// Whether the effect is on in any phase.
    #[must_use]
    pub fn on(&self) -> bool {
        self.state != State::Off
    }

    /// Ask for portal space or for the world, at time `now`: `at_once` goes straight in (or out),
    /// else the effect comes in (goes out) over its phases.
    // D2-11049, D2-11056, D2-11051
    pub fn want(&mut self, want: bool, at_once: bool, now: f64) {
        self.want = want;
        if at_once {
            if want && self.state != State::In {
                self.start(State::In, now);
            } else if !want {
                self.state = State::Off;
                self.since = now;
                self.sounds.push(PassageSound::Stop);
            }
        }
    }

    /// What it asks of the sound since last asked, in order.
    pub fn take_sounds(&mut self) -> Vec<PassageSound> {
        std::mem::take(&mut self.sounds)
    }

    // D2-11047
    fn start(&mut self, state: State, now: f64) {
        self.sounds.push(PassageSound::Start);
        self.state = state;
        self.since = now;
        self.last = now;
        self.passed = 0;
        self.random = self.dice.roll_float(0.0, 10_000.0);
        let first = Ring {
            at: 0.0,
            radius: 0.0,
            frame: Mat4::IDENTITY,
        };
        self.rings = vec![first; RINGS];
        self.make(0, RINGS);
        self.at = self.rings[0].at;
    }

    /// Make rings `start` to `start + count - 1`, each following the one before.
    // D2-11046
    fn make(&mut self, start: usize, count: usize) {
        let noise = noise();
        let phase = self.random * std::f32::consts::PI;
        for i in start..start + count {
            // D2-11045
            let i = i % RINGS;
            let prev = self.rings[(i + RINGS - 1) % RINGS];
            let at = prev.at + SPACING;
            let radius = 10.0 + 6.0 * math::sinf(0.01 * at);
            let ax = 0.35 * noise.at(at * 0.005 + phase);
            let ay = 0.25 * noise.at(at * 0.002 + phase);
            let az = 0.35 * noise.at(at * 0.005 + phase);
            let mut r = rot(2, az).then(&rot(1, ay)).then(&rot(0, ax));
            r.0[3] = [0.0, SPACING, 0.0, 1.0];
            self.rings[i] = Ring {
                at,
                radius,
                frame: r.then(&prev.frame),
            };
        }
    }

    /// The effect's moment at `now`: its phases move on, the camera rushes along the tunnel, the
    /// rings passed are made again at the far end. Returns what it draws.
    // D2-11052
    pub fn think(&mut self, now: f64) -> Look {
        let dt = (now - self.last).max(0.0);
        self.last = now;
        if self.state == State::Off && self.want {
            self.start(State::ComingIn, now);
        }
        if self.state == State::In && !self.want {
            self.state = State::GoingOut;
            self.since = now;
            self.sounds.push(PassageSound::Stop);
        }
        if now - self.since > PHASE {
            let next = match self.state {
                State::ComingIn => Some(State::In),
                State::GoingOut => Some(State::ComingOut),
                State::ComingOut => Some(State::Off),
                State::In | State::Off => None,
            };
            if let Some(s) = next {
                self.state = s;
                self.since = now;
            }
        }
        #[allow(clippy::cast_possible_truncation)]
        let t = ((now - self.since) as f32).clamp(0.0, 1.0);
        let mut look = Look {
            draw_world: false,
            world_fov: 1.0,
            draw_tunnel: false,
            tunnel_fov: 1.0,
            visibility: 1.0,
            nebula: 1.0,
            under: None,
            over: None,
        };
        match self.state {
            State::Off => {
                look.draw_world = true;
                return look;
            }
            State::ComingIn => {
                look.draw_world = true;
                look.draw_tunnel = true;
                look.world_fov = 1.0 - 0.8 * t * t;
                look.nebula = t;
                look.visibility = t;
                look.under = Some([0.0, 0.0, 0.0, t]);
            }
            State::In => look.draw_tunnel = true,
            State::GoingOut => {
                look.draw_tunnel = true;
                look.tunnel_fov = 1.0 + 1.4 * t * t;
                look.over = Some([1.0, 1.0, 1.0, t]);
            }
            State::ComingOut => {
                let u = 1.0 - t;
                look.draw_world = true;
                look.world_fov = 1.0 + 1.4 * u * u;
                look.over = Some([1.0, 1.0, 1.0, u]);
            }
        }
        // Along the tunnel.
        #[allow(clippy::cast_possible_truncation)]
        let ahead = self.at + dt as f32 * SPEED;
        let found = (0..RINGS)
            .map(|k| (self.passed + k) % RINGS)
            .find(|&j| self.rings[j].at > ahead);
        if let Some(j) = found {
            self.at = ahead;
            let now_passed = (j + RINGS - 1) % RINGS;
            if now_passed != self.passed {
                let from = self.passed;
                if now_passed > from {
                    self.make(0, now_passed);
                } else {
                    self.make(from, RINGS - from);
                    if now_passed > 0 {
                        self.make(0, now_passed);
                    }
                }
                self.passed = now_passed;
            }
        }
        look
    }

    /// The camera: between the ring last passed and the next, pulled back along the tube while the
    /// tunnel is not yet all there. Its place and its axes (rows: right, ahead, up).
    fn camera(&self, visibility: f32) -> (Vec3, [Vec3; 3]) {
        let a = &self.rings[self.passed];
        let b = &self.rings[(self.passed + 1) % RINGS];
        let span = b.at - a.at;
        let f = if span > 0.0 {
            ((self.at - a.at) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut at = lerp(row(&a.frame, 3), row(&b.frame, 3), f);
        let ahead = unit(lerp(row(&a.frame, 1), row(&b.frame, 1), f));
        if visibility < 1.0 {
            let back = PULL_BACK * (1.0 - visibility);
            at = Vec3::new(
                at.x - ahead.x * back,
                at.y - ahead.y * back,
                at.z - ahead.z * back,
            );
        }
        let up0 = lerp(row(&a.frame, 2), row(&b.frame, 2), f);
        let right = unit(cross(ahead, up0));
        let up = unit(cross(right, ahead));
        (at, [right, ahead, up])
    }

    /// The tube's vertices, as the camera sees them (its right x, ahead y, up z): each ring's
    /// points at `scale` times its radius, its distance and place round the ring as picture
    /// coordinates, faded with distance; and the triangles joining each ring to the next, the far
    /// end first.
    // D2-11050, D2-11055
    fn tube(
        &self,
        scale: f32,
        visibility: f32,
        eye: Vec3,
        axes: &[Vec3; 3],
    ) -> (Vec<MeshVertex>, Vec<u16>) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let hidden = ((1.0 - visibility) * 149.0) as usize;
        let mut verts = Vec::with_capacity(RINGS * (POINTS + 1));
        for j in 0..RINGS {
            let ring = &self.rings[(self.passed + j) % RINGS];
            let alpha = ring_alpha(j, hidden);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let a = ((alpha.clamp(0.0, 1.0) * 255.0) as u32) << 24;
            for k in 0..=POINTS {
                #[allow(clippy::cast_precision_loss)]
                let ang = (k % POINTS) as f32 * std::f32::consts::PI / 8.0;
                let r = ring.radius * scale;
                let local = Vec3::new(math::cosf(ang) * r, 0.0, math::sinf(ang) * r);
                let w = ring.frame.point(local);
                let d = Vec3::new(w.x - eye.x, w.y - eye.y, w.z - eye.z);
                #[allow(clippy::cast_precision_loss)]
                let v = k as f32 * 0.0625;
                verts.push(MeshVertex {
                    position: [dot(d, axes[0]), dot(d, axes[1]), dot(d, axes[2])],
                    normal: [0.0, 0.0, 1.0],
                    diffuse: a | 0x00FF_FFFF,
                    uv: [[ring.at, v], [ring.at, v], [0.0; 2], [0.0; 2]],
                    tangent_u: [0.0; 3],
                    tangent_v: [0.0; 3],
                    to_light: [0.0; 3],
                });
            }
        }
        let n = POINTS + 1;
        let mut idx = Vec::with_capacity((RINGS - 1) * POINTS * 6);
        for j in (0..RINGS - 1).rev() {
            for m in 0..POINTS {
                let s = |j: usize, m: usize| u16::try_from(j * n + m).unwrap_or(0);
                idx.extend([
                    s(j + 1, m),
                    s(j + 1, m + 1),
                    s(j, m),
                    s(j + 1, m + 1),
                    s(j, m + 1),
                    s(j, m),
                ]);
            }
        }
        (verts, idx)
    }

    /// The frame portal space draws at `now` (what [`Self::think`] returned as `look`), seen with a
    /// field of view of `fov` degrees; `None` while the tunnel is not drawn. The camera sits at the
    /// frame's origin looking north, the tunnel and the nebula turned about it.
    // D2-11053
    #[must_use]
    pub fn frame(
        &self,
        records: &WorldRecords,
        builder: &mut SceneBuilder<'_>,
        look: &Look,
        now: f64,
    ) -> Option<Frame> {
        if !look.draw_tunnel || self.rings.is_empty() {
            return None;
        }
        let (eye, axes) = self.camera(look.visibility);
        // The nebula, about the camera and turned with the world.
        let turn = Mat4([
            [axes[0].x, axes[1].x, axes[2].x, 0.0],
            [axes[0].y, axes[1].y, axes[2].y, 0.0],
            [axes[0].z, axes[1].z, axes[2].z, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]);
        let nebula = Part {
            mesh: auberean_primitives::DataId(NEBULA),
            transform: turn,
            detail: None,
            look: None,
        };
        let camera = Camera {
            position: Vec3::new(0.0, 0.0, 0.0),
            heading: 0.0,
            pitch: 0.0,
            fov_y: FOV * look.tunnel_fov,
            near: 0.1,
            far: 2000.0,
            orthographic: None,
        };
        let mut light = DayLight::DEFAULT;
        light.fog_min = 0.0;
        light.fog_max = 0.0;
        let mut frame = builder.parts_frame(&[nebula], Vec::new(), camera, light, None);
        for b in &mut frame.objects {
            b.tint = Color::new(1.0, 1.0, 1.0, look.nebula);
        }
        // The tubes' pictures scroll with the client's clock.
        for (material, scale) in [(OUTER, OUTER_SCALE), (INNER, 1.0)] {
            let Some(m) = instance_material(records, material) else {
                continue;
            };
            let (v, i) = self.tube(scale, look.visibility, eye, &axes);
            let (v, i) = (Arc::new(v), Arc::new(i));
            for mut layer in draw_layers(&m, DEVICE_FLAGS) {
                layer.move_on(now, &self.dice);
                frame.objects.push(ObjectBatch {
                    vertices: v.clone(),
                    posed: None,
                    indices: i.clone(),
                    transform: Mat4::IDENTITY,
                    layer,
                    tint: Color::WHITE,
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
                    step: None,
                    visible: None,
                    block: None,
                    far: false,
                    ground: None,
                    lod: None,
                });
            }
        }
        // Portal space is no world of objects: each draw goes by its layer, in its order.
        for b in &mut frame.objects {
            let step = b.by_layer();
            b.step.get_or_insert(step);
        }
        Some(frame)
    }
}

fn noise() -> &'static Noise {
    static NOISE: std::sync::OnceLock<Noise> = std::sync::OnceLock::new();
    NOISE.get_or_init(Noise::new)
}
