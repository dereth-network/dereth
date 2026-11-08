//! The camera the player steers, as the client keeps it: where it wants to stand about the player
//! (a direction, a pitch and a distance), how the player's controls move it, the options that
//! shape it, its effects, and the targeting that turns all of that into a place in the world,
//! gliding from the old place to the new whenever the view changes.
//!
//! - **The direction.** The camera keeps a direction across the ground in the player's frame (in
//!   the world's frame during mouse look) and a pitch above level; behind the player is straight
//!   back, the pitch the tilt option (0.25 radians). It stands that direction times the distance
//!   (times the player's height scale) from the player's feet, raised by the player's height and
//!   the height option, and looks back along it.
//! - **The look-at point.** The world keeps the camera off walls and objects with lines that start
//!   from three quarters of the player's height, moved to the shoulder
//!   ([`CameraControl::target_offset`]).
//! - **Close in.** Nearer than 2.9 m the camera slides toward the player's right shoulder, up to
//!   0.7 of the player's radius, easing there over a second; it never stands nearer than half a
//!   meter across the ground. With the auto distance option, a camera 1.5 m or nearer pulls back
//!   toward 4 m after 2.5 seconds of running and comes back after a second standing.
//! - **Held keys** turn the camera a radian a second (5 with Shift), tilt it half a radian (1 with
//!   Shift), zoom it by 0.75 a second in and 1.5 out (squared with Shift), each times the rotate or
//!   zoom option; the wheel steps 1.5 m, between 0.11 m and 160 m.
//! - **The views**: the shoulder view 1.3 m out, first person at the eyes (nine tenths of the
//!   player's height, 0.4 m ahead), looking at the ground (1.37 radians), looking at oneself (in
//!   front, level), swinging out to either side, the map view 160 m out at 1.37 radians; the reset
//!   puts the camera behind at the options' distance and tilt. Each change of view glides over the
//!   transition time option (a second): each frame the camera moves from where it stood toward
//!   its new place by the share of that time gone.
//! - **Align to slope** (an option, on by default) eases the pitch by how fast the player climbs or
//!   falls each frame.
//! - **Mouse look** turns the camera a degree a mouse count times the sensitivity, smoothed over the
//!   newest ten counts weighted 1, 0.2, 0.04 and so on; moving the mouse toward the player tilts
//!   the view down unless inverted. With the steering option the character turns with it.
//! - **Effects** add to the field of view (65 degrees plus their sum, which is held between 10 and
//!   150), the distance, the blur and the screen's flash, each ramping up and down over its period.

use std::collections::VecDeque;

use dereth_primitives::num::math;
use dereth_primitives::{CellId, Position, Quat, Vec3};

/// The camera's actions, by the key map's ids.
pub mod action {
    pub const SWING_OUT_LEFT: u32 = 0x4000_0028;
    pub const SWING_OUT_RIGHT: u32 = 0x4000_0029;
    pub const SHOULDER_VIEW: u32 = 0x4000_002A;
    /// Mouse look while held.
    pub const INSTANT_MOUSE_LOOK: u32 = 0x4000_002C;
    /// One step nearer (the wheel).
    pub const INSTANT_ZOOM_IN: u32 = 0x4000_002D;
    pub const ROTATE_LEFT: u32 = 0x4000_002E;
    pub const ROTATE_RIGHT: u32 = 0x4000_002F;
    pub const RESET: u32 = 0x4000_0030;
    pub const FIRST_PERSON: u32 = 0x4000_00A2;
    pub const LOOK_AT_GROUND: u32 = 0x4000_00AE;
    pub const LOOK_AT_SELF: u32 = 0x4000_00B1;
    /// One step farther (the wheel).
    pub const INSTANT_ZOOM_OUT: u32 = 0x4000_00B2;
    pub const MAP_VIEW: u32 = 0x4000_00B3;
    pub const ZOOM_IN: u32 = 0x4000_00B4;
    pub const ZOOM_OUT: u32 = 0x4000_00B5;
    pub const TILT_UP: u32 = 0x4000_00B6;
    pub const TILT_DOWN: u32 = 0x4000_00B7;
    pub const TOGGLE_MOUSE_LOOK: u32 = 0x4000_00BA;
}

/// The tilt and distance options' defaults.
pub const DEFAULT_TILT: f32 = 0.25;
pub const DEFAULT_DISTANCE: f32 = 4.5;
/// The nearest and farthest the camera goes.
pub const MIN_DISTANCE: f32 = 0.11;
pub const MAX_DISTANCE: f32 = 160.0;
/// One notch of the wheel, meters.
pub const WHEEL_STEP: f32 = 1.5;
/// The shoulder view's distance.
pub const SHOULDER_DISTANCE: f32 = 1.3;
/// The first-person view's distance.
pub const FIRST_PERSON_DISTANCE: f32 = 0.01;
/// The pitch looking at the ground and in the map view, radians.
pub const STEEP_PITCH: f32 = 1.37;
/// The map view's distance.
pub const MAP_DISTANCE: f32 = 160.0;
/// The highest and lowest pitch, radians.
pub const PITCH_LIMIT: f32 = 1.374_446_7;
/// The swing-out view's direction about the character, before it is brought to unit length: x to
/// its right, y ahead, z up (the left swing mirrors x). The unit vector's z is the pitch the view
/// takes, radians.
pub const SWING: [f32; 3] = [5.9, -2.5, 4.75];
/// How many mouse samples mouse look smooths over, and each older sample's weight relative to the
/// next newer one.
pub const MOUSE_SAMPLES: usize = 10;
pub const MOUSE_DECAY: f32 = 0.2;
/// The share of the player's height the look-at point stands at.
pub const TARGET_HEIGHT: f32 = 0.75;
/// The camera's own field of view, degrees, before its effects.
pub const BASE_FOV: f32 = 65.0;
/// The mode a body is at peace in: out of combat.
pub const PEACE_MODE: u32 = 0x4000_0001;
/// The kind of bone the first-person camera rides while a behavior that moves it plays: the head.
pub const HEAD_BONE: u32 = 0x4000_0005;
/// Where on that bone the camera rides, in the bone's frame: ahead and up a fifth of a meter.
pub const HEAD_OFFSET: [f32; 3] = [0.0, 0.2, 0.2];

const EPS: f32 = 0.0002;
/// What the shake's starts along the noise are drawn from (the client draws them once a session).
const SHAKE_SEED: u32 = 0x5EED_5AC3;
/// Degrees in a radian, as the client writes it.
const DEGREES: f32 = 57.295_78;
/// Radians in a degree, as the client writes it.
const RADIANS: f32 = 0.017_453_292;

/// A camera control the player holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Held {
    pub rotate_left: bool,
    pub rotate_right: bool,
    pub tilt_up: bool,
    pub tilt_down: bool,
    pub zoom_in: bool,
    pub zoom_out: bool,
    /// Shift: the faster rates.
    pub fast: bool,
}

/// A camera control the player presses once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Reset,
    Shoulder,
    FirstPerson,
    TopDown,
    Reverse,
    SwingLeft,
    SwingRight,
    MapView,
    MouseLook,
    /// The wheel: positive notches toward the character.
    Wheel(i32),
}

/// Which view the camera's direction and distance make, as the keypad names them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Behind,
    Shoulder,
    FirstPerson,
    TopDown,
    Reverse,
    SwingLeft,
    SwingRight,
}

/// When the player's own body shows with the camera at its eyes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AvatarInFirstPerson {
    Never,
    Always,
    /// Unless the player is at peace: the client's default.
    #[default]
    DuringCombat,
}

impl AvatarInFirstPerson {
    /// The option's number in the settings: 0, 1 and 2.
    #[must_use]
    pub fn number(self) -> u32 {
        match self {
            Self::Never => 0,
            Self::Always => 1,
            Self::DuringCombat => 2,
        }
    }

    /// The option of a number.
    #[must_use]
    pub fn of_number(n: u32) -> Option<Self> {
        match n {
            0 => Some(Self::Never),
            1 => Some(Self::Always),
            2 => Some(Self::DuringCombat),
            _ => None,
        }
    }

    /// The option's name in the settings.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Never => "Never",
            Self::Always => "Always",
            Self::DuringCombat => "DuringCombat",
        }
    }

    /// Whether a player in mode `mode` shows: never, always, or unless at peace.
    #[must_use]
    pub fn shows(self, mode: u32) -> bool {
        match self {
            Self::Never => false,
            Self::Always => true,
            Self::DuringCombat => mode != PEACE_MODE,
        }
    }
}

/// The options that shape the camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    /// The default distance, meters.
    pub distance: f32,
    /// The default tilt, radians.
    pub tilt: f32,
    /// Multiplies the rotate and tilt speeds.
    pub rotate_speed: f32,
    /// Multiplies the zoom speed.
    pub zoom_speed: f32,
    /// Degrees per mouse count in mouse look, before the one-degree base.
    pub mouse_sensitivity: f32,
    pub invert_y: bool,
    /// Mouse look turns the character with the camera.
    pub mouse_look_steers: bool,
    /// Raises the camera, meters.
    pub height: f32,
    /// How long the camera glides from one view to the next, seconds.
    pub transition_time: f32,
    /// Pulls a close camera back while the player runs.
    pub auto_distance: bool,
    /// Eases the pitch by the slope the player climbs.
    pub align_to_slope: bool,
    /// When the player's own body shows with the camera at its eyes.
    pub avatar_in_first_person: AvatarInFirstPerson,
    /// The player's head follows the camera.
    pub look_around: bool,
    /// The player's head turns back toward a camera in front of it.
    pub look_at_camera: bool,
    /// The player looks at each new selection.
    pub look_at_selection: bool,
    /// The player's own body turns see-through under the pointer.
    pub avatar_hover_transparency: bool,
}

impl Default for Options {
    /// The client's defaults.
    // D2-10689
    fn default() -> Self {
        Self {
            distance: DEFAULT_DISTANCE,
            tilt: DEFAULT_TILT,
            rotate_speed: 1.0,
            zoom_speed: 1.0,
            mouse_sensitivity: 1.0,
            invert_y: false,
            mouse_look_steers: true,
            height: 0.0,
            transition_time: 1.0,
            auto_distance: true,
            align_to_slope: true,
            avatar_in_first_person: AvatarInFirstPerson::DuringCombat,
            look_around: true,
            look_at_camera: true,
            look_at_selection: true,
            avatar_hover_transparency: false,
        }
    }
}

/// What the camera follows this frame: the player's body as it is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Subject {
    /// The feet, in the frame of the body's landblock.
    pub origin: Vec3,
    /// Degrees clockwise from north.
    pub heading: f32,
    /// The body's height and radius (its setup's, times its height scale).
    pub height: f32,
    pub radius: f32,
    /// The body's height scale.
    pub scale_z: f32,
    /// Its velocity in its own frame (y ahead).
    pub local_velocity: Vec3,
}

impl Subject {
    /// A vector in the body's own frame turned into its landblock's.
    fn global_of(&self, v: Vec3) -> Vec3 {
        let (s, c) = math::sin_cosf(self.heading.to_radians());
        Vec3::new(v.x * c + v.y * s, -v.x * s + v.y * c, v.z)
    }

    fn local_of(&self, v: Vec3) -> Vec3 {
        let (s, c) = math::sin_cosf(self.heading.to_radians());
        Vec3::new(v.x * c - v.y * s, v.x * s + v.y * c, v.z)
    }

    fn facing(heading: f32) -> Self {
        Self {
            heading,
            ..Self::default()
        }
    }
}

/// Where the camera stands (or wants to) and the way it looks, in the frame of the player's
/// landblock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewer {
    pub origin: Vec3,
    /// A unit vector.
    pub look: Vec3,
    /// How the camera is turned, when it is more than a look with no lean: a glide between two
    /// views, or a bone it rides on.
    pub rotation: Option<Quat>,
}

impl Viewer {
    /// The frame it stands in, turned as it is.
    fn frame(&self) -> dereth_primitives::Frame {
        let mut f = dereth_primitives::Frame::new(self.origin, Quat::IDENTITY);
        match self.rotation {
            Some(q) => f.rotation = q,
            None => auberean_physics::geom::set_vector_heading(&mut f, self.look),
        }
        f
    }

    /// A place of the camera in cell `cell`, turned as this view is, standing at `origin`.
    #[must_use]
    pub fn stand_at(&self, cell: CellId, origin: Vec3) -> Position {
        let mut f = self.frame();
        f.origin = origin;
        Position::new(cell, f)
    }
}

/// The camera takes place `placed`, or stays where it stood with none: where it stood becomes where
/// it stood before, and with `at_once` it was never anywhere else. Placed the first time with no
/// line to keep it off the world, it takes `unlined`. A frame that isn't one (a number not a number,
/// or a rotation not of unit length) is no place. The place taken, if any.
// D2-10647
pub fn take_place(
    stands: &mut Option<Stands>,
    placed: Option<Position>,
    unlined: Position,
    at_once: bool,
) -> Option<Position> {
    let placed = placed.filter(|at| auberean_physics::geom::frame_is_valid(&at.frame));
    match (stands.as_mut(), placed) {
        (Some(s), Some(at)) => {
            s.set_previous(s.current);
            s.set_current(at);
            if at_once {
                s.set_previous(at);
            }
        }
        (Some(s), None) => s.set_previous(s.current),
        (None, Some(at)) => *stands = Some(Stands::at(at)),
        (None, None) => {
            if auberean_physics::geom::frame_is_valid(&unlined.frame) {
                *stands = Some(Stands::at(unlined));
            }
        }
    }
    placed
}

/// The id no object has.
pub const NO_OBJECT: u64 = 0;

/// What the camera reads of an object it follows, hangs from or looks at.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Facts {
    pub is_player: bool,
    /// One of the camera's own stand-ins hung from the player: it adds no height.
    pub is_dummy: bool,
    /// Not drawn.
    pub no_draw: bool,
    /// Its height: its setup's times its height scale.
    pub height: f32,
    /// Its height scale (of its whole scale).
    pub scale_z: f32,
    /// Its velocity in its own frame (y ahead).
    pub local_velocity: Vec3,
    pub element: u32,
}

/// The objects the camera may follow, hang from or look at, as the world has them this frame,
/// each place in the frame of the player's landblock.
pub trait Objects {
    /// Where object `id` stands as drawn, its look its frame's forward axis: at its first bone of
    /// kind `bone` when that is not 0 (where the object stands when it has none). `None` when
    /// there is no such object.
    fn place(&self, id: u64, bone: u32) -> Option<Viewer>;
    /// What the camera reads of object `id`, when there is one.
    fn facts(&self, id: u64) -> Option<Facts>;
}

/// The player alone, as a world of one object: every id but [`NO_OBJECT`] names it.
impl Objects for Subject {
    fn place(&self, id: u64, _bone: u32) -> Option<Viewer> {
        (id != NO_OBJECT).then(|| Viewer {
            origin: self.origin,
            look: self.global_of(Vec3::new(0.0, 1.0, 0.0)),
            rotation: None,
        })
    }

    fn facts(&self, id: u64) -> Option<Facts> {
        (id != NO_OBJECT).then_some(Facts {
            is_player: true,
            is_dummy: false,
            no_draw: false,
            height: self.height,
            scale_z: self.scale_z,
            local_velocity: self.local_velocity,
            element: 0,
        })
    }
}

/// The way a frame faces when its forward axis is set along `v`: `v` brought to unit length,
/// north for a vector too short to have a way.
fn heading_along(v: Vec3) -> Vec3 {
    let sq = v.z * v.z + v.y * v.y + v.x * v.x;
    let len = sq.sqrt();
    if sq < 3.999_999_6e-8 || len < 0.0002 {
        return Vec3::new(0.0, 1.0, 0.0);
    }
    scale(v, 1.0 / len)
}

/// The heading of a direction across the ground, degrees clockwise from +y; 0 for none.
fn heading_of(x: f32, y: f32) -> f32 {
    let len = (x * x + y * y).sqrt();
    if len < EPS {
        return 0.0;
    }
    let k = 1.0 / len;
    let r = std::f32::consts::FRAC_PI_2 * 5.0 - math::atan2f(y * k, x * k);
    (r * DEGREES).rem_euclid(360.0)
}

fn normalize(v: Vec3) -> Vec3 {
    let len = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if len == 0.0 {
        v
    } else {
        Vec3::new(v.x / len, v.y / len, v.z / len)
    }
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn scale(a: Vec3, k: f32) -> Vec3 {
    Vec3::new(a.x * k, a.y * k, a.z * k)
}

fn length(a: Vec3) -> f32 {
    (a.x * a.x + a.y * a.y + a.z * a.z).sqrt()
}

fn near(a: Vec3, b: Vec3) -> bool {
    (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS && (a.z - b.z).abs() < EPS
}

/// The client's gradient noise, made once.
fn noise() -> &'static auberean_draw::portal::Noise {
    static NOISE: std::sync::OnceLock<auberean_draw::portal::Noise> = std::sync::OnceLock::new();
    NOISE.get_or_init(auberean_draw::portal::Noise::new)
}

/// Three numbers between 0 and 10 drawn from `seed`.
#[must_use]
pub fn roll_shake_seeds(seed: u32) -> [f32; 3] {
    let mut state = seed;
    let mut roll = || {
        state = state.wrapping_mul(214_013).wrapping_add(2_531_011);
        let r = u16::try_from((state >> 16) & 0x7FFF).unwrap_or(0);
        f32::from(r) / 32_767.0 * 10.0
    };
    [roll(), roll(), roll()]
}

/// What a camera effect changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    FieldOfView,
    /// The screen flashes this color.
    Flash([f32; 4]),
    Blur,
    Shake,
    Distance,
}

/// One camera effect under way: what it changes, how strongly, and its ramp.
// D2-10691
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Track {
    /// How far the effect has ramped, 0 to 1.
    pub weight: f32,
    pub intensity: f32,
    pub period: f32,
    pub ramp_up: f32,
    pub ramp_down: f32,
    pub time: f32,
    pub kind: Effect,
    /// The object it comes from.
    pub source: u64,
}

impl Track {
    /// `dt` on: the weight climbs over the ramp up, holds, and falls over the ramp down.
    // D2-10744
    fn update(&mut self, dt: f32) {
        self.time += dt;
        let t = self.time;
        if t < self.ramp_up {
            self.weight = t / self.ramp_up;
        } else if self.ramp_up < t && t < self.period - self.ramp_down {
            self.weight = 1.0;
        } else if self.period - self.ramp_down < t {
            self.weight = (self.period - t) / self.ramp_down;
        }
    }
}

/// The camera's effects and what they add up to this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effects {
    pub tracks: Vec<Track>,
    /// Where along the noise the shake's three axes start, each drawn once between 0 and 10.
    pub shake_seeds: [f32; 3],
    /// Degrees added to the field of view, held between 10 and 150.
    pub fov: f32,
    pub flash: [f32; 4],
    pub blur: f32,
    pub shake: f32,
    /// Meters added to the camera's distance.
    pub distance: f32,
}

impl Effects {
    /// Starts an effect from `source`: its intensity times `scale`, over `period` seconds with its
    /// ramps.
    // D2-10743, D2-10749, D2-10750, D2-10751, D2-10752, D2-10753
    #[allow(clippy::too_many_arguments)]
    pub fn add(
        &mut self,
        kind: Effect,
        intensity: f32,
        scale: f32,
        period: f32,
        ramp_up: f32,
        ramp_down: f32,
        source: u64,
    ) {
        self.tracks.insert(
            0,
            Track {
                weight: 0.0,
                intensity: intensity * scale,
                period,
                ramp_up,
                ramp_down,
                time: 0.0,
                kind,
                source,
            },
        );
    }

    /// How far the shakes under way move the camera this frame, nothing while they add up to
    /// nothing: for each shake, three layers of noise sixty times its seconds along (from each
    /// axis's start), its intensity times its weight over its source's distance from the player
    /// (at least a meter), up and down three times as much. A shake whose source or player is not
    /// here moves nothing. (`player` and `source` give places on one grid.)
    // D2-10746
    #[must_use]
    pub fn distortion(&self, player: Option<Vec3>, source: impl Fn(u64) -> Option<Vec3>) -> Vec3 {
        if self.shake == 0.0 {
            return Vec3::ZERO;
        }
        let noise = noise();
        let [sx, sy, sz] = self.shake_seeds;
        let fbm = |p: f32| noise.fbm1(f64::from(p), 0.5, 2.0, 3.0);
        let mut out = Vec3::ZERO;
        for t in &self.tracks {
            if t.kind != Effect::Shake {
                continue;
            }
            let along = t.time * 60.0;
            let (x, y, z) = (fbm(sx + along), fbm(along + sy), fbm(along + sz));
            let (Some(me), Some(from)) = (player, source(t.source)) else {
                continue;
            };
            let d = length(sub(from, me)).max(1.0);
            let k = (t.intensity / d) * t.weight;
            out = Vec3::new(x * k + out.x, k * y + out.y, z * 3.0 * k + out.z);
        }
        out
    }

    // D2-10754
    pub fn stop_all(&mut self) {
        self.tracks.clear();
    }

    /// `dt` on: every effect ramps, those past their period end, and the sums are made again.
    // D2-10748
    pub fn use_time(&mut self, dt: f32) {
        self.fov = 0.0;
        self.flash = [0.0; 4];
        self.blur = 0.0;
        self.shake = 0.0;
        self.distance = 0.0;
        let mut kept = Vec::with_capacity(self.tracks.len());
        for mut t in self.tracks.drain(..) {
            t.update(dt);
            if t.time > t.period {
                continue;
            }
            let k = t.intensity * t.weight;
            match t.kind {
                Effect::FieldOfView => self.fov += k,
                Effect::Flash(c) => {
                    for (s, c) in self.flash.iter_mut().zip(c) {
                        *s += c * t.weight * t.intensity;
                    }
                }
                Effect::Blur => self.blur += k,
                Effect::Shake => self.shake += k,
                Effect::Distance => self.distance += k,
            }
            kept.push(t);
        }
        self.tracks = kept;
        self.fov = self.fov.clamp(10.0, 150.0);
        self.blur = self.blur.min(1.0);
        for c in &mut self.flash {
            *c = c.min(1.0);
        }
    }
}

/// The targeting: the distance the camera's place is worked out at, and the glide from where the
/// camera stood to its new place.
#[derive(Clone, Debug, PartialEq)]
pub struct Targeting {
    /// Gliding.
    pub is_on: bool,
    pub ramp_time: f32,
    pub time: f32,
    pub period: f32,
    /// How far the glide has come, 0 to 1.
    pub weight: f32,
    pub weight_rate: f32,
    /// The height option.
    pub camera_height: f32,
    /// The transition time option.
    pub transition_time: f32,
    pub time_running: f32,
    pub time_not_running: f32,
    /// The distance the place is worked out at.
    pub distance: f32,
    pub hold: bool,
    pub viewer_offset: Vec3,
    pub old_viewer_offset: Vec3,
    /// The player, the object the camera looks at and the object it hangs from (with the bones
    /// of theirs it uses, 0 for none), and the ones before them.
    pub player: u64,
    pub target: u64,
    pub parent: u64,
    pub old_parent: u64,
    pub old_target: u64,
    pub target_bone: u32,
    pub parent_bone: u32,
    pub old_target_bone: u32,
    pub old_parent_bone: u32,
    /// Where the object hung from and the one looked at last stood.
    pub last_parent: Option<Viewer>,
    pub last_target: Option<Viewer>,
    /// The auto distance option.
    pub auto_distance: bool,
}

impl Default for Targeting {
    // D2-10670
    fn default() -> Self {
        Self {
            is_on: false,
            ramp_time: 0.1,
            time: 0.0,
            period: 1.0,
            weight: 0.0,
            weight_rate: 0.0,
            camera_height: 0.0,
            transition_time: 1.0,
            time_running: 0.0,
            time_not_running: 0.0,
            distance: 3.5,
            hold: false,
            viewer_offset: Vec3::new(0.0, -3.5, 0.0),
            old_viewer_offset: Vec3::new(0.0, -3.5, 0.0),
            player: NO_OBJECT,
            target: NO_OBJECT,
            parent: NO_OBJECT,
            old_parent: NO_OBJECT,
            old_target: NO_OBJECT,
            target_bone: 0,
            parent_bone: 0,
            old_target_bone: 0,
            old_parent_bone: 0,
            last_parent: None,
            last_target: None,
            auto_distance: true,
        }
    }
}

impl Targeting {
    /// A glide over `ramp` seconds (a `period` of -1 holds until told).
    // D2-10733
    fn turn_on(&mut self, period: f32, ramp: f32) {
        self.is_on = true;
        self.time = 0.0;
        if period == -1.0 {
            self.period = 1.0;
            self.hold = true;
        } else {
            self.period = period;
            self.hold = false;
        }
        self.ramp_time = ramp;
        if ramp == 0.0 {
            self.weight = 1.0;
            self.weight_rate = 0.0;
        } else {
            self.weight = 0.0;
            self.weight_rate = 1.0 / ramp;
        }
    }

    /// The view changed: the camera glides from where it stands over the transition time (it hangs
    /// from nothing, and the place it glides from is worked out with no object, so it is the
    /// camera's own).
    // D2-10736, D2-10688
    fn fake_transition(&mut self) {
        self.old_target = NO_OBJECT;
        self.old_parent = self.parent;
        self.parent = NO_OBJECT;
        self.turn_on(self.transition_time, self.transition_time);
    }

    /// The player is `id`: it is also the object looked at, now and before.
    // D2-10732
    fn set_player(&mut self, id: u64) {
        self.player = id;
        self.old_target = id;
        self.target = id;
    }

    /// Where object `id` stands (at its bone `bone` when not 0); `None` for no object.
    // D2-10737
    fn position(objects: &dyn Objects, id: u64, bone: u32) -> Option<Viewer> {
        if id == NO_OBJECT {
            return None;
        }
        objects.place(id, bone)
    }

    /// The camera looks at `id` (at its bone `bone` when not 0): where the one it looked at stands
    /// is kept, it becomes the one before, and, when that place is known, the camera glides over
    /// `ramp` seconds and holds the new view for `period` (-1: until told).
    // D2-10738
    fn store_target(&mut self, objects: &dyn Objects, id: u64, bone: u32, period: f32, ramp: f32) {
        if self.target == id && self.target_bone == bone {
            return;
        }
        if let Some(p) = Self::position(objects, self.target, self.target_bone) {
            self.last_target = Some(p);
        }
        self.old_target = self.target;
        self.old_target_bone = self.target_bone;
        self.target = id;
        self.target_bone = bone;
        if self.last_target.is_some() {
            self.turn_on(period, ramp);
        }
    }

    /// The camera hangs from `id` (from its bone `bone` when not 0): as [`Targeting::store_target`],
    /// for a change of object only.
    // D2-10739
    fn store_parent(&mut self, objects: &dyn Objects, id: u64, bone: u32, period: f32, ramp: f32) {
        if self.parent == id {
            return;
        }
        if let Some(p) = Self::position(objects, self.parent, self.parent_bone) {
            self.last_parent = Some(p);
        }
        self.old_parent = self.parent;
        self.parent = id;
        self.old_parent_bone = self.parent_bone;
        self.parent_bone = bone;
        if self.last_parent.is_some() {
            self.turn_on(period, ramp);
        }
    }

    /// The distance the place is worked out at: the camera's, or, close in with the auto distance
    /// option, pulled back toward 4 m after 2.5 s of running (each frame its distance plus one is
    /// raised to 1.07, held to 5) and brought back after a second standing (to 0.97).
    // D2-10734
    fn calc_camera_distance(&mut self, wanted: f32, steady: bool, subject: &Subject, dt: f32) {
        // Negated so that an unordered distance takes the plain path, as the client's compare does.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        let far = !(wanted <= 1.5);
        if far || !self.auto_distance {
            self.distance = wanted;
            return;
        }
        let v = subject.local_velocity;
        let running = v.x * v.x + v.y * v.y + v.z * v.z > 2.0;
        if running {
            self.time_not_running = 0.0;
            self.time_running += dt;
        } else {
            self.time_running = 0.0;
            self.time_not_running += dt;
        }
        self.time_running = self.time_running.clamp(0.0, 6.0);
        if !steady {
            self.distance = wanted;
            return;
        }
        let want1 = wanted + 1.0;
        let cur1 = self.distance + 1.0;
        self.distance = cur1;
        if self.time_running > 2.5 && cur1 <= 5.5 && running {
            self.distance = math::powf(cur1, 1.07);
            if self.distance > 5.0 && want1 < 5.0 {
                self.distance = 5.0;
            }
        } else if !running && cur1 != want1 && self.time_not_running > 1.0 {
            self.distance = math::powf(cur1, 0.97);
            if self.distance < want1 {
                self.distance = want1;
            }
        }
        self.distance -= 1.0;
    }

    /// `dt` on: out of a glide, where the object hung from and the one looked at stand is kept;
    /// the glide ends once whole, or with neither place known; a view held for its period goes
    /// back to the player, gliding two seconds and holding; then the glide's share of its ramp.
    // D2-10740
    fn update(&mut self, dt: f32, objects: &dyn Objects) {
        if !self.is_on {
            if let Some(p) = Self::position(objects, self.parent, self.parent_bone) {
                self.last_parent = Some(p);
            }
            if let Some(p) = Self::position(objects, self.target, self.target_bone) {
                self.last_target = Some(p);
            }
        }
        self.time += dt;
        let known = self.last_parent.is_some() || self.last_target.is_some();
        if !(self.weight < 1.0 && known) {
            self.is_on = false;
        }
        if self.period - self.ramp_time < self.time
            && !self.hold
            && (self.target != self.player || self.parent != NO_OBJECT)
        {
            self.store_target(objects, self.player, 0, -1.0, 2.0);
            self.store_parent(objects, NO_OBJECT, 0, -1.0, 2.0);
        }
        if self.ramp_time > self.time {
            self.weight = self.time / self.ramp_time;
        } else if self.ramp_time < self.time {
            self.weight = 1.0;
        }
        self.weight = self.weight.clamp(0.0, 1.0);
    }
}

/// The camera the player steers.
#[derive(Clone, Debug, PartialEq)]
pub struct CameraControl {
    pub options: Options,
    /// The vertical field of view, degrees.
    pub fov: f32,
    fov_rate: f32,
    /// From the player's feet to the look-at point.
    look_at: Vec3,
    /// The camera's direction across the ground (x to the player's right, y ahead; in the world's
    /// frame in mouse look) and, as z, its pitch above level in radians.
    pub offset: Vec3,
    /// Radians a second the pitch and the heading change, and what the distance is multiplied by
    /// each second.
    pitch_rate: f32,
    heading_rate: f32,
    zoom_rate: f32,
    pub targeting: Targeting,
    /// The player the camera follows ([`NO_OBJECT`] for none).
    pub player: u64,
    pub effects: Effects,
    pub mouse_look: bool,
    first_person: bool,
    map_view: bool,
    /// Meters from the player.
    pub distance: f32,
    shoulder_start: Vec3,
    shoulder_current: Vec3,
    shoulder_target: Vec3,
    shoulder_time: f32,
    pitch_from_slope: f32,
    z_velocity: f32,
    last_z: f32,
    /// The camera the map view replaced: its direction and distance.
    map_saved: Option<(Vec3, f32)>,
    /// The pitch this frame, the slope's added.
    pitch_now: f32,
    /// Where the camera stood last frame, which a glide moves on from.
    pub last: Option<Viewer>,
    held: Held,
    /// The newest mouse samples, newest first.
    samples: VecDeque<(f32, f32)>,
}

impl Default for CameraControl {
    fn default() -> Self {
        Self::new(Options::default())
    }
}

impl CameraControl {
    /// The camera made, its options applied, and set behind the player.
    // D2-10719
    #[must_use]
    pub fn new(options: Options) -> Self {
        let mut c = Self {
            options,
            fov: BASE_FOV,
            fov_rate: 0.0,
            look_at: Vec3::ZERO,
            offset: Vec3::new(0.0, -3.5, 0.0),
            pitch_rate: 0.0,
            heading_rate: 0.0,
            zoom_rate: 1.0,
            targeting: Targeting::default(),
            player: NO_OBJECT,
            effects: Effects {
                shake_seeds: roll_shake_seeds(SHAKE_SEED),
                ..Effects::default()
            },
            mouse_look: false,
            first_person: false,
            map_view: false,
            distance: 3.5,
            shoulder_start: Vec3::ZERO,
            shoulder_current: Vec3::ZERO,
            shoulder_target: Vec3::ZERO,
            shoulder_time: 0.0,
            pitch_from_slope: 0.0,
            z_velocity: 0.0,
            last_z: 0.0,
            map_saved: None,
            pitch_now: 0.0,
            last: None,
            held: Held::default(),
            samples: VecDeque::with_capacity(MOUSE_SAMPLES),
        };
        c.apply_options();
        c.set_defaults();
        c.pitch_now = c.offset.z;
        c.targeting.distance = c.distance;
        c
    }

    /// New options: a new default distance or tilt puts the camera there at once.
    // D2-10711
    #[allow(clippy::float_cmp)] // whether the preference changed at all
    pub fn set_options(&mut self, o: Options) {
        let moved = o.distance != self.options.distance || o.tilt != self.options.tilt;
        self.options = o;
        self.apply_options();
        if moved {
            self.set_defaults();
        }
    }

    /// The options taken into the camera's targeting.
    // D2-10711, D2-10697
    pub fn apply_options(&mut self) {
        let o = self.options;
        self.targeting.camera_height = o.height;
        self.targeting.transition_time = o.transition_time;
        self.targeting.auto_distance = o.auto_distance;
    }

    /// The camera behind the player at the options' distance and tilt (in mouse look, level the way
    /// it faces), gliding there unless in first person.
    // D2-10707
    pub fn set_defaults(&mut self) {
        if self.map_view {
            self.set_map_view(false);
        }
        if self.mouse_look {
            let h = heading_of(self.offset.x, self.offset.y).to_radians();
            let (s, c) = math::sin_cosf(h);
            self.offset = Vec3::new(s, c, 0.0);
        } else {
            self.offset = Vec3::new(0.0, -1.0, self.options.tilt);
        }
        if self.first_person {
            self.distance = FIRST_PERSON_DISTANCE;
        } else {
            self.targeting.fake_transition();
            self.distance = self.options.distance;
        }
    }

    /// A new player `id` to follow, `height` meters high: the camera looks at it (and takes it as
    /// its player when it has none), its look-at point at three quarters of the height, the camera
    /// set behind it. (`objects`: where the object it looked at before stands.)
    // D2-10648
    pub fn set_player(&mut self, objects: &dyn Objects, id: u64, height: f32) {
        self.set_target_object(objects, id, 1.0, 1.0);
        self.look_at = Vec3::new(0.0, 0.0, height * TARGET_HEIGHT);
        self.set_defaults();
        self.apply_options();
    }

    /// The camera looks at `id`, gliding over `ramp` seconds and holding the view for `period`;
    /// with no player yet, `id` becomes its player, the one its targeting follows, at the
    /// camera's distance.
    // D2-10694
    pub fn set_target_object(&mut self, objects: &dyn Objects, id: u64, period: f32, ramp: f32) {
        self.targeting.store_target(objects, id, 0, period, ramp);
        if self.player == NO_OBJECT {
            self.player = id;
            self.targeting.set_player(id);
            self.targeting.distance = self.distance;
        }
    }

    /// The camera hangs from `id`.
    // D2-10679
    pub fn set_parent_object(&mut self, objects: &dyn Objects, id: u64, period: f32, ramp: f32) {
        self.targeting.store_parent(objects, id, 0, period, ramp);
    }

    /// A free camera: it hangs from `id` and looks at nothing (along the way `id` faces).
    // D2-10682
    pub fn set_free_camera(&mut self, objects: &dyn Objects, id: u64, period: f32, ramp: f32) {
        self.targeting.store_parent(objects, id, 0, period, ramp);
        self.targeting
            .store_target(objects, NO_OBJECT, 0, period, ramp);
    }

    /// A targeted camera: it hangs from `id`'s bone of kind `parent_bone` and looks at its bone
    /// of kind `target_bone`.
    // D2-10683
    pub fn set_targeted_camera(
        &mut self,
        objects: &dyn Objects,
        id: u64,
        bones: (u32, u32),
        period: f32,
        ramp: f32,
    ) {
        self.targeting
            .store_parent(objects, id, bones.0, period, ramp);
        self.targeting
            .store_target(objects, id, bones.1, period, ramp);
    }

    /// The camera back on the player: it hangs from nothing and looks at the player, gliding there
    /// at once over a second.
    // D2-10684
    pub fn restore_player_camera(&mut self, objects: &dyn Objects) {
        self.targeting.store_parent(objects, NO_OBJECT, 0, 1.0, 0.0);
        self.targeting
            .store_target(objects, self.player, 0, 1.0, 0.0);
    }

    /// The camera looks at its stand-in that the player's animations move (when there is one).
    // D2-10686
    pub fn set_camera_anim_target(&mut self, objects: &dyn Objects, period: f32, ramp: f32) {
        let id = auberean_client_sdk::model::ids::camera_target();
        if objects.facts(id).is_some() {
            self.targeting.store_target(objects, id, 0, period, ramp);
        }
    }

    /// The camera hangs from its stand-in that the player's animations move (when there is one).
    // D2-10687
    pub fn set_camera_anim_parent(&mut self, objects: &dyn Objects, period: f32, ramp: f32) {
        let id = auberean_client_sdk::model::ids::camera_parent();
        if objects.facts(id).is_some() {
            self.targeting.store_parent(objects, id, 0, period, ramp);
        }
    }

    /// The camera follows no one: it has no player, it forgets where it stood and its glide stops
    /// (the world's camera goes back to the origin).
    // D2-10681
    pub fn reset_camera(&mut self) {
        self.player = NO_OBJECT;
        self.targeting.set_player(NO_OBJECT);
        self.last = None;
        self.targeting.is_on = false;
    }

    /// Every camera effect ends.
    // D2-10690
    pub fn stop_all_camera_fx(&mut self) {
        self.effects.stop_all();
    }

    /// A camera effect record played from `source` at `scale`: each effect it names starts
    /// (shake, the screen's flash, the field of view, blur, the distance), over its period and
    /// ramps (a second and a hundredth of one when the record names none), at its intensity (1)
    /// times `scale`; then the camera itself: free (hanging from `source`, looking at nothing),
    /// back on the player, hung from `source`, looking at `source`, each over the record's period
    /// with its ramp down. A record for the player only does nothing from another source.
    // D2-10698, D2-10723
    pub fn do_camera_fx(
        &mut self,
        objects: &dyn Objects,
        fx: &auberean_assets::fx::CameraFx,
        scale: f32,
        source: u64,
    ) {
        if fx.player_only == Some(true) && source != self.player {
            return;
        }
        let intensity = fx.intensity.unwrap_or(1.0);
        let period = fx.period.unwrap_or(1.0);
        let up = fx.ramp_up.unwrap_or(0.01);
        let down = fx.ramp_down.unwrap_or(0.01);
        let mut start = |kind: Effect| {
            self.effects
                .add(kind, intensity, scale, period, up, down, source);
        };
        if fx.shake == Some(true) {
            start(Effect::Shake);
        }
        if let Some((true, c)) = fx.screen_flash {
            let channel = |shift: u32| f32::from(((c.0 >> shift) & 0xFF) as u8) / 255.0;
            start(Effect::Flash([
                channel(16),
                channel(8),
                channel(0),
                channel(24),
            ]));
        }
        if fx.field_of_view == Some(true) {
            start(Effect::FieldOfView);
        }
        if fx.blur == Some(true) {
            start(Effect::Blur);
        }
        if fx.camera_distance == Some(true) {
            start(Effect::Distance);
        }
        if fx.free_camera == Some(true) {
            self.set_free_camera(objects, source, period, down);
        }
        if fx.restore_camera == Some(true) {
            self.restore_player_camera(objects);
        }
        if fx.parent_camera == Some(true) {
            self.targeting
                .store_parent(objects, source, 0, period, down);
        }
        if fx.target_camera == Some(true) {
            self.set_target_object(objects, source, period, down);
        }
    }

    /// Whether first person has settled at the eyes: on, following the player itself, at its
    /// distance.
    // D2-10674
    #[must_use]
    pub fn first_person_is_ready(&self) -> bool {
        self.first_person && self.distance == FIRST_PERSON_DISTANCE
    }

    /// Whether the player's own body (and what it holds) is drawn, the player in mode `mode`:
    /// always, unless first person has settled at the eyes and the avatar option hides it (never,
    /// or at peace during combat only).
    #[must_use]
    pub fn draws_the_player(&self, mode: u32) -> bool {
        !self.first_person_is_ready() || self.options.avatar_in_first_person.shows(mode)
    }

    /// Whether the camera rides the player's head instead of standing where the targeting puts
    /// it: in first person, when the avatar option shows the player in mode `mode`, while a
    /// behavior that moves the first-person camera plays (`moving`).
    // D2-10700
    #[must_use]
    pub fn rides_the_head(&self, mode: u32, moving: bool) -> bool {
        self.first_person && self.options.avatar_in_first_person.shows(mode) && moving
    }

    /// The camera's distance with its effects'.
    // D2-10692
    #[must_use]
    pub fn camera_distance(&self) -> f32 {
        if self.first_person || self.map_view {
            self.distance
        } else {
            self.distance + self.effects.distance
        }
    }

    // D2-10677
    fn zoom_in(&mut self) {
        if !self.first_person {
            self.distance = (self.distance - WHEEL_STEP).max(MIN_DISTANCE);
            self.targeting.distance = self.distance;
        }
    }

    // D2-10678
    fn zoom_out(&mut self) {
        if !self.first_person {
            self.distance = (self.distance + WHEEL_STEP).min(MAX_DISTANCE);
            self.targeting.distance = self.distance;
        }
    }

    // D2-10693
    fn shoulder_view(&mut self) {
        if self.map_view || self.first_person || self.mouse_look {
            return;
        }
        if self.distance == SHOULDER_DISTANCE {
            self.distance = self.options.distance;
        } else {
            self.offset = Vec3::new(0.0, -1.0, self.options.tilt);
            self.distance = SHOULDER_DISTANCE;
        }
        self.targeting.distance = self.distance;
        self.targeting.fake_transition();
    }

    // D2-10702, D2-10703
    fn swing_out(&mut self, right: bool) {
        let [x, y, z] = SWING;
        let to = normalize(Vec3::new(if right { x } else { -x }, y, z));
        if self.map_view || self.first_person || self.mouse_look {
            return;
        }
        if length(sub(self.offset, to)) > EPS {
            self.offset = to;
        } else {
            self.offset = Vec3::new(0.0, -1.0, self.options.tilt);
        }
        self.targeting.fake_transition();
    }

    /// The map view on (160 m out, pitched 1.37 radians, straight behind) or off (the camera it
    /// replaced back); not in first person.
    // D2-10704
    pub fn set_map_view(&mut self, on: bool) {
        if self.first_person {
            return;
        }
        if on {
            self.map_saved = Some((self.offset, self.distance));
            self.offset = Vec3::new(0.0, -1.0, STEEP_PITCH);
            self.targeting.fake_transition();
            self.distance = MAP_DISTANCE;
            self.map_view = true;
        } else {
            if let Some((offset, distance)) = self.map_saved.take() {
                self.distance = distance;
                self.targeting.distance = distance;
                self.offset = offset;
            }
            self.targeting.fake_transition();
            self.map_view = false;
        }
    }

    // D2-10708
    fn look_at_ground(&mut self) {
        let map = self.map_view;
        if map {
            self.set_map_view(false);
        }
        if !self.first_person {
            let o = self.offset;
            let pitch = if length(Vec3::new(o.x, o.y + 1.0, o.z - STEEP_PITCH)) <= EPS {
                self.options.tilt
            } else {
                STEEP_PITCH
            };
            self.offset = Vec3::new(0.0, -1.0, pitch);
            self.targeting.fake_transition();
        } else if map {
            self.set_map_view(map);
        }
    }

    // D2-10701
    fn look_at_self(&mut self) {
        if self.map_view || self.first_person || self.mouse_look {
            return;
        }
        let o = self.offset;
        if o.x.abs() < EPS && (o.y - 1.0).abs() < EPS && o.z.abs() < EPS {
            self.offset = Vec3::new(0.0, -1.0, self.options.tilt);
        } else {
            self.offset = Vec3::new(0.0, 1.0, 0.0);
        }
        self.targeting.fake_transition();
    }

    /// Mouse look on or off: the camera's direction kept where it points, turned between the
    /// player's frame and the world's by the player's heading (degrees clockwise from north).
    // D2-10709
    pub fn set_mouse_look(&mut self, on: bool, heading: f32) {
        if self.map_view {
            self.set_map_view(false);
        }
        if self.mouse_look == on {
            return;
        }
        self.mouse_look = on;
        self.samples.clear();
        let s = Subject::facing(heading);
        self.offset = if on {
            s.global_of(self.offset)
        } else {
            s.local_of(self.offset)
        };
    }

    /// First person on (at the eyes, at the tilt option's pitch, facing ahead) or off (back at the
    /// options' distance when it was at the eyes).
    // D2-10710
    pub fn set_first_person(&mut self, on: bool) {
        if self.first_person == on {
            return;
        }
        if self.map_view {
            self.set_map_view(false);
        }
        if on {
            self.distance = FIRST_PERSON_DISTANCE;
            self.offset = Vec3::new(0.0, -1.0, self.options.tilt);
        } else if (self.distance - FIRST_PERSON_DISTANCE).abs() < EPS {
            self.distance = self.options.distance;
        }
        self.first_person = on;
    }

    /// An action starts, `speed` its extent in the key map (1 or 5 rotating, 0.5 or 1 tilting, 1
    /// or 2 zooming); `heading` is the player's, for mouse look.
    // D2-10713
    pub fn start_action(&mut self, id: u32, speed: f32, heading: f32) {
        match id {
            action::SWING_OUT_LEFT => self.swing_out(false),
            action::SWING_OUT_RIGHT => self.swing_out(true),
            action::SHOULDER_VIEW => self.shoulder_view(),
            action::INSTANT_MOUSE_LOOK => self.set_mouse_look(true, heading),
            action::INSTANT_ZOOM_IN => self.zoom_in(),
            action::ROTATE_LEFT if !self.first_person => {
                self.heading_rate = speed * self.options.rotate_speed;
            }
            action::ROTATE_RIGHT if !self.first_person => {
                self.heading_rate = -(speed * self.options.rotate_speed);
            }
            action::RESET => {
                self.set_defaults();
                self.zoom_rate = 1.0;
                self.pitch_rate = 0.0;
                self.heading_rate = 0.0;
            }
            action::FIRST_PERSON => self.set_first_person(!self.first_person),
            action::LOOK_AT_GROUND => self.look_at_ground(),
            action::LOOK_AT_SELF => self.look_at_self(),
            action::INSTANT_ZOOM_OUT => self.zoom_out(),
            action::MAP_VIEW => self.set_map_view(!self.map_view),
            action::ZOOM_IN if !self.first_person => {
                self.zoom_rate = math::powf(0.75, speed * self.options.zoom_speed);
            }
            action::ZOOM_OUT if !self.first_person => {
                self.zoom_rate = math::powf(1.5, speed * self.options.zoom_speed);
            }
            action::TILT_UP => self.pitch_rate = speed * self.options.rotate_speed,
            action::TILT_DOWN => self.pitch_rate = -(speed * self.options.rotate_speed),
            action::TOGGLE_MOUSE_LOOK => self.set_mouse_look(!self.mouse_look, heading),
            _ => {}
        }
    }

    /// An action stops.
    // D2-10714
    pub fn stop_action(&mut self, id: u32, heading: f32) {
        match id {
            action::INSTANT_MOUSE_LOOK => self.set_mouse_look(false, heading),
            action::ROTATE_LEFT | action::ROTATE_RIGHT if !self.first_person => {
                self.heading_rate = 0.0;
            }
            action::ZOOM_IN | action::ZOOM_OUT if !self.first_person => self.zoom_rate = 1.0,
            action::TILT_UP | action::TILT_DOWN => self.pitch_rate = 0.0,
            _ => {}
        }
    }

    /// The player pressed a camera control, facing `heading` (degrees, for mouse look).
    pub fn press_facing(&mut self, p: Press, heading: f32) {
        let id = match p {
            Press::Reset => action::RESET,
            Press::Shoulder => action::SHOULDER_VIEW,
            Press::FirstPerson => action::FIRST_PERSON,
            Press::TopDown => action::LOOK_AT_GROUND,
            Press::Reverse => action::LOOK_AT_SELF,
            Press::SwingLeft => action::SWING_OUT_LEFT,
            Press::SwingRight => action::SWING_OUT_RIGHT,
            Press::MapView => action::MAP_VIEW,
            Press::MouseLook => action::TOGGLE_MOUSE_LOOK,
            Press::Wheel(n) => {
                let a = if n > 0 {
                    action::INSTANT_ZOOM_IN
                } else {
                    action::INSTANT_ZOOM_OUT
                };
                for _ in 0..n.unsigned_abs() {
                    self.start_action(a, 0.0, heading);
                }
                return;
            }
        };
        self.start_action(id, 0.0, heading);
    }

    /// The player pressed a camera control, facing north.
    pub fn press(&mut self, p: Press) {
        self.press_facing(p, 0.0);
    }

    /// The held controls: each one newly held (or held on as Shift changes) starts its action at
    /// its key map extent, each let go stops it. The camera moves by them in
    /// [`CameraControl::update`].
    // D2-09642, D2-09643
    pub fn hold(&mut self, h: Held) {
        let was = self.held;
        let pairs = [
            (
                was.rotate_left,
                h.rotate_left,
                action::ROTATE_LEFT,
                1.0,
                5.0,
            ),
            (
                was.rotate_right,
                h.rotate_right,
                action::ROTATE_RIGHT,
                1.0,
                5.0,
            ),
            (was.tilt_up, h.tilt_up, action::TILT_UP, 0.5, 1.0),
            (was.tilt_down, h.tilt_down, action::TILT_DOWN, 0.5, 1.0),
            (was.zoom_in, h.zoom_in, action::ZOOM_IN, 1.0, 2.0),
            (was.zoom_out, h.zoom_out, action::ZOOM_OUT, 1.0, 2.0),
        ];
        for (before, now, id, slow, quick) in pairs {
            if now && (!before || was.fast != h.fast) {
                self.start_action(id, if h.fast { quick } else { slow }, 0.0);
            } else if before && !now {
                self.stop_action(id, 0.0);
            }
        }
        self.held = h;
    }

    /// Mouse motion in mouse look, in counts (x right, y toward the player), smoothed over the
    /// newest ten samples: the camera turns about the player and tilts.
    // D2-10722, D2-09639, D2-04866
    pub fn mouse(&mut self, dx: f32, dy: f32) {
        if !self.mouse_look {
            return;
        }
        if self.samples.len() == MOUSE_SAMPLES {
            self.samples.pop_back();
        }
        self.samples.push_front((dx, dy));
        let (mut sx, mut sy, mut w, mut total) = (0.0, 0.0, 1.0, 0.0);
        for (x, y) in &self.samples {
            sx += x * w;
            sy += y * w;
            total += w;
            w *= MOUSE_DECAY;
        }
        let k = 1.0 / total;
        let sens = self.options.mouse_sensitivity;
        let turn = sx * k * sens;
        let up = k * sy * sens;
        let sign = if self.options.invert_y { 1.0 } else { -1.0 };
        let pitch = (up * sign * RADIANS - self.offset.z).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        if !self.map_view {
            let (s, c) = math::sin_cosf(turn * -RADIANS);
            let (x, y) = (self.offset.x, self.offset.y);
            self.offset = Vec3::new(x * c - y * s, x * s + y * c, -pitch);
        }
    }

    /// The way the camera looks across its offset: straight back along it.
    // D2-10685
    #[must_use]
    pub fn global_direction(&self) -> Vec3 {
        scale(self.offset, -1.0)
    }

    /// Whether mouse look steers the character: with the option, and always in first person.
    // D2-09649
    #[must_use]
    pub fn steers(&self) -> bool {
        self.mouse_look && (self.options.mouse_look_steers || self.first_person)
    }

    /// The heading mouse look gives the character this frame, when it steers it and no move the
    /// server ordered has the heading (`moving_to`): the way the camera looks, whatever way it looks.
    // D2-09649
    #[must_use]
    pub fn steering_heading(&self, moving_to: bool) -> Option<f32> {
        (self.steers() && !moving_to).then(|| {
            let d = self.global_direction();
            heading_of(d.x, d.y)
        })
    }

    /// The server forced the player's place: unless it is at the eyes, the camera glides to the
    /// new place from where it stands.
    // D2-06372, D2-10688
    pub fn player_forced_elsewhere(&mut self) {
        if !self.first_person() {
            self.targeting.fake_transition();
        }
    }

    /// Whether the camera is at the eyes.
    // D2-11866
    #[must_use]
    pub fn first_person(&self) -> bool {
        self.first_person
    }

    #[must_use]
    pub fn map_view(&self) -> bool {
        self.map_view
    }

    /// The view the camera's direction and distance make.
    #[must_use]
    pub fn view(&self) -> View {
        let o = self.offset;
        if self.first_person {
            View::FirstPerson
        } else if o.y > 0.5 {
            View::Reverse
        } else if o.x.abs() > 0.5 {
            if o.x > 0.0 {
                View::SwingRight
            } else {
                View::SwingLeft
            }
        } else if o.z == STEEP_PITCH && !self.map_view {
            View::TopDown
        } else if self.distance == SHOULDER_DISTANCE {
            View::Shoulder
        } else {
            View::Behind
        }
    }

    /// The camera's pitch above level, radians.
    #[must_use]
    pub fn pitch(&self) -> f32 {
        self.offset.z
    }

    /// Radians the camera is turned about the player from straight behind (positive to its left).
    #[must_use]
    pub fn yaw(&self) -> f32 {
        let h = heading_of(self.offset.x, self.offset.y);
        (h - 180.0).to_radians()
    }

    /// Whether the world leaves the camera where it wants to be: in first person, or while it looks
    /// at something other than the player.
    // D2-10676
    #[must_use]
    pub fn no_ray_trace(&self) -> bool {
        self.first_person || self.targeting.player != self.targeting.target
    }

    /// Whether only the walls and the ground stop the camera, no object: in the map view, or
    /// looking at a bone.
    // D2-10675
    #[must_use]
    pub fn check_env_only(&self) -> bool {
        self.map_view || self.targeting.target_bone != 0
    }

    /// The pitch eased by the slope: the player's rise each frame, smoothed, its opposite
    /// approached by its gap a second when more than 0.02 radians off, else at 0.05 radians a
    /// second.
    // D2-10672
    fn align_to_slope(&mut self, dt: f32, z: f32) {
        let dz = z - self.last_z;
        if dt <= 1.0 {
            let decayed = (1.0 - dt) * self.z_velocity;
            self.z_velocity = decayed;
            if EPS <= dz.abs() {
                if dz.abs() < 5.0 {
                    self.z_velocity = dz * dt + decayed;
                }
            } else {
                self.z_velocity = decayed * (1.0 - dt);
            }
        } else {
            self.z_velocity = dz;
        }
        self.z_velocity = self.z_velocity.clamp(-PITCH_LIMIT, PITCH_LIMIT);
        let goal = -self.z_velocity;
        self.last_z = z;
        let gap = self.pitch_from_slope - goal;
        if gap.abs() > 0.02 {
            self.pitch_from_slope -= gap * dt;
        } else if goal < self.pitch_from_slope {
            self.pitch_from_slope = (self.pitch_from_slope - dt * 0.05).max(goal);
        } else if self.pitch_from_slope < goal {
            self.pitch_from_slope = (dt * 0.05 + self.pitch_from_slope).min(goal);
        }
    }

    /// One update and one frame of `dt` seconds following `subject` alone (see
    /// [`CameraControl::update_with`] and [`CameraControl::use_time`]).
    pub fn update(&mut self, dt: f32, subject: &Subject) {
        self.update_with(dt, subject, subject);
        self.use_time(dt);
    }

    /// One update of `dt` seconds following `subject` (the player) among `objects`, as the client
    /// updates its camera each time the player's physics ticks (each frame in mouse look): the
    /// field of view, the zoom, the tilt and the slope, the turn, the shoulder's easing, the
    /// distance and the glide (and where the objects it hangs from and looks at stand).
    // D2-10700
    pub fn update_with(&mut self, dt: f32, subject: &Subject, objects: &dyn Objects) {
        self.fov += self.fov_rate;
        self.distance *= math::powf(self.zoom_rate, dt);
        if self.distance == 0.0 {
            self.distance = 0.1;
        }
        if self.distance > MAX_DISTANCE {
            self.distance = MAX_DISTANCE;
        }
        if !self.map_view && self.pitch_rate != 0.0 {
            self.offset.z = (dt * self.pitch_rate + self.offset.z).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        }
        let mut pitch = self.offset.z;
        if self.options.align_to_slope && !self.map_view && !self.mouse_look {
            self.align_to_slope(dt, subject.origin.z);
            pitch = (pitch + self.pitch_from_slope).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        }
        if !self.map_view && self.heading_rate != 0.0 {
            let h = (heading_of(self.offset.x, self.offset.y) + DEGREES * dt * self.heading_rate)
                .rem_euclid(360.0)
                .to_radians();
            let len = (self.offset.y * self.offset.y + self.offset.x * self.offset.x).sqrt();
            let (s, c) = math::sin_cosf(h);
            self.offset.x = s * len;
            self.offset.y = c * len;
        }
        if !near(self.shoulder_target, self.shoulder_current) {
            self.shoulder_time += dt;
            if self.shoulder_time >= 1.0 {
                self.shoulder_current = self.shoulder_target;
                self.shoulder_time = 0.0;
            } else {
                let t = self.shoulder_time;
                let way = sub(self.shoulder_target, self.shoulder_start);
                self.shoulder_current = add(self.shoulder_start, scale(way, t));
            }
        }
        self.pitch_now = pitch;
        let wanted = self.camera_distance();
        let steady = !self.first_person && !self.map_view;
        self.targeting
            .calc_camera_distance(wanted, steady, subject, dt);
        self.targeting.update(dt, objects);
    }

    /// One frame of `dt` seconds of the camera's effects: their time, and the field of view they
    /// make (65 degrees and their sum). Each frame, after the camera is drawn between its places.
    // D2-10673
    pub fn use_time(&mut self, dt: f32) {
        self.effects.use_time(dt);
        self.fov = self.effects.fov + BASE_FOV;
    }

    /// Where the shoulder offset puts the camera across the ground when it is `distance` out: to
    /// the side the camera is turned to by 0.7 of the player's radius times what is left of 2.9
    /// m, less as the camera comes round in front. At distance 0 the offset itself; else the
    /// offset the camera eases toward, over a second once it moves by more than about 0.22 m.
    // D2-10696
    fn shoulder_offset(&mut self, subject: &Subject, distance: f32) -> Vec3 {
        if self.first_person {
            return Vec3::ZERO;
        }
        let r = subject.radius * 0.7;
        let mut s = (1.0 - distance / 2.9) * r;
        // Negated so that an unordered offset takes the cap, as the client's compare does.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        let over = !(s <= r);
        if over {
            s = r;
        }
        if s < 0.0 {
            s = 0.0;
        }
        if !self.mouse_look && 0.0 < self.offset.y {
            let n = normalize(Vec3::new(self.offset.x, self.offset.y, 0.0));
            s *= 1.0 - n.y;
        }
        if self.offset.x < 0.0 {
            s = -s;
        }
        let g = subject.global_of(Vec3::new(s, 0.0, 0.0));
        if distance == 0.0 {
            return g;
        }
        let (dx, dy) = (g.x - self.shoulder_current.x, g.y - self.shoulder_current.y);
        if dy * dy + dx * dx <= 0.05 {
            if self.shoulder_time == 0.0 && near(self.shoulder_target, self.shoulder_current) {
                self.shoulder_current = g;
                self.shoulder_target = g;
                self.shoulder_start = g;
            }
        } else {
            self.shoulder_target = g;
            self.shoulder_start = self.shoulder_current;
            self.shoulder_time = 0.0;
        }
        self.shoulder_current
    }

    /// From the player's feet to where the world's lines keeping the camera off walls start: the
    /// look-at point (three quarters up the player) moved to the shoulder.
    // D2-10705
    pub fn target_offset(&mut self, subject: &Subject) -> Vec3 {
        let s = self.shoulder_offset(subject, 0.0);
        add(self.look_at, s)
    }

    /// Where the camera stands for one choice of objects: the object it hangs from (`parent`, at
    /// its bone `parent_bone` when not 0) and the one it looks at (`target`, likewise), `old` for
    /// the ones before (whose last places stand in for objects gone). The viewer offset is turned
    /// into the world's frame by the player's unless in mouse look; a distance is the targeting's,
    /// times the height scale of a target that is the player.
    ///
    /// - **Both:** at the object hung from, looking from it (raised by its height) to the one
    ///   looked at; hung from the player (and looking at no bone), back a third of the distance
    ///   from the player along that line, raised by the height option, with the shoulder offset;
    ///   raised by the height of the one hung from.
    /// - **Hung from one only:** at it, facing its way; out a third of the distance along the
    ///   viewer offset, with the shoulder offset, raised by its height.
    /// - **Looking at one only:** looking back along the viewer offset; in first person at the
    ///   eyes of a player drawn (nine tenths of its height, 0.4 m ahead, more while it moves ahead
    ///   or back, 1 m in element `0x40000003`); else, looking at no bone, the distance out along the
    ///   viewer offset, raised by the height option and its height, with the shoulder offset, never
    ///   nearer than half a meter across the ground; at a bone, at the bone.
    /// - **Neither:** where the camera stands (`fallback`).
    ///
    /// A camera stand-in adds no height.
    // D2-10741
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn internal_calc_viewer_pos(
        &mut self,
        old: bool,
        parent: (u64, u32),
        target: (u64, u32),
        subject: &Subject,
        objects: &dyn Objects,
        fallback: Option<Viewer>,
    ) -> Option<Viewer> {
        let facts = |id: u64| {
            if id == NO_OBJECT {
                None
            } else {
                objects.facts(id)
            }
        };
        let (pf, tf) = (facts(parent.0), facts(target.0));
        let mut pp = Targeting::position(objects, parent.0, parent.1);
        if pp.is_none() && old && parent.0 != NO_OBJECT {
            pp = self.targeting.last_parent;
        }
        let mut pt = Targeting::position(objects, target.0, target.1);
        if pt.is_none() && old && target.0 != NO_OBJECT {
            pt = self.targeting.last_target;
        }
        let mut vo = self.targeting.viewer_offset;
        if !self.mouse_look && facts(self.targeting.player).is_some() {
            vo = subject.global_of(vo);
        }
        let scale_z = tf.filter(|f| f.is_player).map_or(1.0, |f| f.scale_z);
        let has_parent = pf.is_some() || (old && parent.0 != NO_OBJECT);
        let has_target = tf.is_some() || (old && target.0 != NO_OBJECT);
        let nowhere = Viewer {
            origin: Vec3::ZERO,
            look: Vec3::new(0.0, 1.0, 0.0),
            rotation: None,
        };
        let d = self.targeting.distance;
        let third = 0.333_333_34;
        match (has_parent, has_target) {
            (true, true) => {
                let (pp, pt) = (pp.unwrap_or(nowhere), pt.unwrap_or(nowhere));
                let mut from = pp.origin;
                if let Some(f) = pf.filter(|f| !f.is_dummy) {
                    from.z += f.height;
                }
                let v = sub(pt.origin, from);
                let mut at = Viewer {
                    origin: pp.origin,
                    look: heading_along(v),
                    rotation: None,
                };
                let Some(f) = pf else {
                    return Some(at);
                };
                // (Worked out anew with the player hung from, the player also turns its head to
                // the object looked at; not done here.)
                if f.is_player && self.targeting.target_bone == 0 {
                    let n = normalize(scale(v, -1.0));
                    let k = |c: f32| c * d * scale_z * third;
                    at.origin.x += k(n.x);
                    at.origin.y += k(n.y);
                    at.origin.z += k(n.z) + self.targeting.camera_height;
                    at.origin = add(at.origin, self.shoulder_offset(subject, d));
                }
                if !f.is_dummy {
                    at.origin.z += f.height;
                }
                Some(at)
            }
            (true, false) => {
                let pp = pp.unwrap_or(nowhere);
                let mut at = Viewer {
                    origin: pp.origin,
                    look: heading_along(pp.look),
                    rotation: None,
                };
                let Some(f) = pf else {
                    return Some(at);
                };
                let k = |c: f32| c * d * scale_z * third;
                at.origin = add(at.origin, Vec3::new(k(vo.x), k(vo.y), k(vo.z)));
                at.origin = add(at.origin, self.shoulder_offset(subject, d));
                if !f.is_dummy {
                    at.origin.z += f.height;
                }
                Some(at)
            }
            (false, true) => {
                let pt = pt.unwrap_or(nowhere);
                let look = heading_along(scale(vo, -1.0));
                let mut at = Viewer {
                    origin: pt.origin,
                    look,
                    rotation: None,
                };
                let Some(f) = tf else {
                    return Some(at);
                };
                if self.first_person {
                    if !f.no_draw && f.is_player {
                        let fwd = normalize(Vec3::new(look.x, look.y, 0.0));
                        at.origin.z += f.height * 0.9;
                        let v = f.local_velocity;
                        let k = if f.element == 0x4000_0003 {
                            1.0
                        } else if v.y > 0.0002 || v.y < -0.0002 {
                            (v.y.abs() * 0.1 + 1.0) * 0.4
                        } else {
                            0.4
                        };
                        at.origin.x += fwd.x * k;
                        at.origin.y += fwd.y * k;
                    }
                } else if self.targeting.target_bone == 0 {
                    let k = d * f.scale_z;
                    at.origin.x += vo.x * k;
                    at.origin.y += vo.y * k;
                    at.origin.z += vo.z * k + self.targeting.camera_height;
                    if !f.is_dummy {
                        at.origin.z += f.height;
                    }
                    at.origin = add(at.origin, self.shoulder_offset(subject, d));
                    let (ax, ay) = (at.origin.x - pt.origin.x, at.origin.y - pt.origin.y);
                    let across = (ax * ax + ay * ay).sqrt();
                    if 0.0 < across && across < 0.5 {
                        at.origin.x = pt.origin.x + ax / across * 0.5;
                        at.origin.y = pt.origin.y + ay / across * 0.5;
                    }
                }
                Some(at)
            }
            (false, false) => fallback,
        }
    }

    /// The camera's direction from what it looks at (its viewer offset), in the player's frame
    /// (the world's in mouse look): in a glide, the glide's share of the way from the one before.
    // D2-10695, D2-10735
    #[must_use]
    pub fn interp_viewer_offset(&self) -> Vec3 {
        let t = &self.targeting;
        if !t.is_on {
            return t.viewer_offset;
        }
        let (w, k) = (t.weight, 1.0 - t.weight);
        let (n, o) = (t.viewer_offset, t.old_viewer_offset);
        Vec3::new(w * n.x + k * o.x, w * n.y + k * o.y, w * n.z + k * o.z)
    }

    /// Where the camera wants to stand this frame following `subject` alone (see
    /// [`CameraControl::viewer_with`]).
    pub fn viewer(&mut self, subject: &Subject) -> Viewer {
        self.viewer_with(subject, subject)
    }

    /// Where the camera wants to stand this frame and the way it looks, `subject` the player among
    /// `objects`: its direction pitched by the pitch (and the slope) as the viewer offset; out of a
    /// glide, the place for the objects it hangs from and looks at; in a glide, the share of the
    /// glide gone from the place for the ones before them (where the camera stood, when there were
    /// none) to it.
    // D2-10742, D2-10695, D2-10735
    pub fn viewer_with(&mut self, subject: &Subject, objects: &dyn Objects) -> Viewer {
        let h = heading_of(self.offset.x, self.offset.y).to_radians();
        let (sh, ch) = math::sin_cosf(h);
        let (sp, cp) = math::sin_cosf(self.pitch_now);
        let dir = Vec3::new(sh * cp, ch * cp, sp);
        self.targeting.viewer_offset = dir;
        let fallback = self.last;
        let t = &self.targeting;
        let now = ((t.parent, t.parent_bone), (t.target, t.target_bone));
        let before = (
            (t.old_parent, t.old_parent_bone),
            (t.old_target, t.old_target_bone),
        );
        let nowhere = Viewer {
            origin: Vec3::ZERO,
            look: Vec3::new(0.0, 1.0, 0.0),
            rotation: None,
        };
        if !self.targeting.is_on {
            let at = self.internal_calc_viewer_pos(false, now.0, now.1, subject, objects, fallback);
            self.targeting.old_viewer_offset = dir;
            return at.unwrap_or(nowhere);
        }
        let old =
            self.internal_calc_viewer_pos(true, before.0, before.1, subject, objects, fallback);
        let new = self
            .internal_calc_viewer_pos(false, now.0, now.1, subject, objects, fallback)
            .unwrap_or(nowhere);
        let Some(old) = old else {
            return new;
        };
        let w = self.targeting.weight;
        // D2-10742
        let mut turned = dereth_primitives::Frame::new(Vec3::ZERO, Quat::IDENTITY);
        auberean_physics::geom::slerp_rotation(&mut turned, &old.frame(), &new.frame(), w);
        let m = auberean_physics::geom::rotation_axes(turned.rotation);
        Viewer {
            origin: add(old.origin, scale(sub(new.origin, old.origin), w)),
            look: Vec3::new(m[3], m[4], m[5]),
            rotation: Some(turned.rotation),
        }
    }
}

/// Where the camera stands, as the client keeps it between the player's physics ticks: where it
/// stood when last placed before ([`Stands::previous`]) and where it stands now
/// ([`Stands::current`]), each a cell and a frame in that cell's landblock that faces the way the
/// camera looks. A frame draws it between the two ([`Stands::slerp`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stands {
    pub previous: Position,
    pub current: Position,
}

impl Stands {
    /// Standing at `at`, nothing to draw it between.
    #[must_use]
    pub const fn at(at: Position) -> Self {
        Self {
            previous: at,
            current: at,
        }
    }

    // D2-10519
    pub fn set_previous(&mut self, at: Position) {
        self.previous = at;
    }

    // D2-10521
    pub fn set_current(&mut self, at: Position) {
        self.current = at;
    }

    /// Where a frame draws the camera, `t` of the way from where it stood before to where it
    /// stands now: the point along the line, in the cell it stood in before; the way it looks
    /// turned along the shorter arc.
    // D2-10516
    #[must_use]
    pub fn slerp(&self, t: f32) -> Position {
        auberean_physics::geom::slerp_position(&self.previous, &self.current, t)
    }
}

/// A place of the camera in cell `cell`: `origin` in the cell's landblock's frame, facing along
/// `look`.
#[must_use]
pub fn stand(cell: CellId, origin: Vec3, look: Vec3) -> Position {
    let mut at = Position::new(cell, dereth_primitives::Frame::new(origin, Quat::IDENTITY));
    auberean_physics::geom::set_vector_heading(&mut at.frame, look);
    at
}

/// The way a camera standing at `at` looks.
#[must_use]
pub fn look_of(at: &Position) -> Vec3 {
    let m = auberean_physics::geom::rotation_axes(at.frame.rotation);
    Vec3::new(m[3], m[4], m[5])
}

/// How far a camera standing at `at` leans about the way it looks, in degrees: its right going up.
// D2-10593
#[must_use]
pub fn lean_of(at: &Position) -> f32 {
    let m = auberean_physics::geom::rotation_axes(at.frame.rotation);
    let (right, forward) = (Vec3::new(m[0], m[1], m[2]), Vec3::new(m[3], m[4], m[5]));
    let cross = |a: Vec3, b: Vec3| {
        Vec3::new(
            a.y * b.z - a.z * b.y,
            a.z * b.x - a.x * b.z,
            a.x * b.y - a.y * b.x,
        )
    };
    let flat = normalize(cross(forward, Vec3::new(0.0, 0.0, 1.0)));
    let up = cross(flat, forward);
    let dot = |a: Vec3, b: Vec3| a.x * b.x + a.y * b.y + a.z * b.z;
    let lean = math::atan2f(dot(right, up), dot(right, flat)).to_degrees();
    if lean.abs() < 1e-3 {
        0.0
    } else {
        lean
    }
}

/// No objects at all.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoObjects;

impl Objects for NoObjects {
    fn place(&self, _id: u64, _bone: u32) -> Option<Viewer> {
        None
    }

    fn facts(&self, _id: u64) -> Option<Facts> {
        None
    }
}
