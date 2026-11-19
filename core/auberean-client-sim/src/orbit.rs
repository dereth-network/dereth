//! The orbit camera, which an interface may choose in place of the game's own, and the player's
//! movement under it (the settings and the two ways of moving are the contract's
//! [`auberean_client_contract::orbit`]).
//!
//! The camera circles a point above the player's feet ([`pivot_height`]) at a distance the wheel
//! and the zoom controls set, between [`MIN_DISTANCE`] and [`MAX_DISTANCE`]. The mouse turns it
//! while a mouse button is held, and the camera's rotate and tilt controls turn and tilt it while
//! held; each turns the other way when the settings reverse it. Held, the look-at-front control
//! turns the view round to the player's front, leaving the way the player moves as it was. It is
//! set behind the player the first time it follows them, and again when they are put somewhere
//! new ([`OrbitCamera::place_anew`]).
//!
//! Each frame ([`OrbitCamera::step`]) it takes its inputs, then the movement the player's controls
//! asked is turned into the body's steering ([`OrbitCamera::steer`]):
//! - **camera-based:** with any movement asked, the body faces the way the controls point against
//!   where the camera looks (forward its way, backward toward the camera, the turns and strafes
//!   across its view, two together between) and runs forward that way, at the pace asked; with
//!   none, it stands as it faces.
//! - **character-based:** the movement as the game takes it. Moving or turning, with no mouse
//!   button held, brings the camera round behind the player in [`RECENTRE`] seconds. With the
//!   right button held the body faces where the camera looks and the turns step sideways.
//!
//! While the server is moving the body to a place, the camera never turns it.
//!
//! What it gives is where the camera would stand and the way it looks ([`OrbitCamera::eye`]);
//! whoever draws it keeps it off the world's walls and floors as it does the game's own camera.

use auberean_client_contract::control::CameraHeld;
pub use auberean_client_contract::orbit::{MovementMode, OrbitSettings};
use dereth_primitives::num::math;
use dereth_primitives::Vec3;

use crate::player::Steer;

/// How far above the player's feet the camera looks, as a share of the body's height.
pub const PIVOT_SHARE: f32 = 0.85;
/// How far above the feet it looks, meters, for a body whose height is not known.
pub const PIVOT_HEIGHT: f32 = 1.5;
/// How close and how far the wheel brings the camera, meters, and where it starts.
pub const MIN_DISTANCE: f32 = 1.0;
pub const MAX_DISTANCE: f32 = 15.0;
pub const DEFAULT_DISTANCE: f32 = 6.0;
/// How far one step of the wheel or a zoom press moves it, meters.
pub const ZOOM_STEP: f32 = 1.0;
/// Steps a second a held zoom control moves it.
pub const ZOOM_RATE: f32 = 6.0;
/// Its tilt, radians, positive looking up: where it starts, a little above the player looking
/// down, and how far it goes.
pub const DEFAULT_PITCH: f32 = -0.3;
pub const PITCH_MIN: f32 = -1.35;
pub const PITCH_MAX: f32 = 0.9;
/// Seconds it takes to come round behind the player under character-based movement.
pub const RECENTRE: f32 = 0.2;
/// The swing round behind is a critically damped spring at this over [`RECENTRE`]: in that time
/// it closes all but a hundredth of the way, never swinging past.
const RECENTRE_SPRING: f32 = 6.638_352;

/// How far above its feet the camera looks for a body `height` meters tall (0 when not known).
#[must_use]
pub fn pivot_height(height: f32) -> f32 {
    if height > 0.0 && height.is_finite() {
        height * PIVOT_SHARE
    } else {
        PIVOT_HEIGHT
    }
}

/// What the orbit camera hears from the devices this frame, beside the camera controls held.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OrbitInput {
    /// The mouse's motion, in its counts (x right, y toward the player).
    pub mouse: (f32, f32),
    /// The mouse buttons held: left, right.
    pub buttons: (bool, bool),
    /// Zoom steps taken (the wheel's, the zoom presses'): positive toward the player.
    pub zoom: i32,
    /// The look-at-front control held.
    pub front_view: bool,
}

/// The orbit camera's state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitCamera {
    /// The way it looks, degrees clockwise from north (the look-at-front control left out).
    pub heading: f32,
    /// Its tilt, radians, positive looking up.
    pub pitch: f32,
    /// How far it stands from the point it looks at, before walls bring it closer.
    pub distance: f32,
    /// Turned round to look at the player's front, while that is held.
    pub front_view: bool,
    pub settings: OrbitSettings,
    /// Whether it has been set behind the player yet.
    pub placed: bool,
    /// How fast it is swinging round behind the player, degrees a second.
    swing: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self::new(OrbitSettings::default())
    }
}

/// `degrees` brought into `(-180, 180]`.
fn wrap(degrees: f32) -> f32 {
    let a = degrees.rem_euclid(360.0);
    if a > 180.0 {
        a - 360.0
    } else {
        a
    }
}

/// `-1`, `0` or `1`, as `x` is below, at or above 0.
fn sign(x: f32) -> f32 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

impl OrbitCamera {
    /// A camera with `settings`, not yet set behind the player.
    #[must_use]
    pub fn new(settings: OrbitSettings) -> Self {
        Self {
            heading: 0.0,
            pitch: DEFAULT_PITCH,
            distance: DEFAULT_DISTANCE,
            front_view: false,
            settings: settings.held_to_ranges(),
            placed: false,
            swing: 0.0,
        }
    }

    /// Take `settings`, their rates held to their ranges; where it stands is kept.
    pub fn apply_settings(&mut self, settings: OrbitSettings) {
        self.settings = settings.held_to_ranges();
    }

    /// Stand behind a player facing `heading` (degrees clockwise from north), looking their way.
    pub fn place_behind(&mut self, heading: f32) {
        self.heading = heading.rem_euclid(360.0);
        self.placed = true;
        self.swing = 0.0;
    }

    /// Set it behind the player afresh when it next follows them: they were put somewhere new.
    /// Its tilt and distance are kept.
    pub fn place_anew(&mut self) {
        self.placed = false;
        self.swing = 0.0;
    }

    /// Turn it by a mouse motion of `(dx, dy)` counts: right looks further right and up (away from
    /// the player) looks further up, each the other way when the settings reverse it.
    pub fn rotate(&mut self, dx: f32, dy: f32) {
        let s = self.settings;
        let x = if s.reverse_x { -1.0 } else { 1.0 };
        let y = if s.reverse_y { -1.0 } else { 1.0 };
        if dx != 0.0 {
            self.swing = 0.0;
        }
        self.heading = (self.heading + (dx * s.mouse_turn * x).to_degrees()).rem_euclid(360.0);
        self.pitch = (self.pitch - dy * s.mouse_turn * y).clamp(PITCH_MIN, PITCH_MAX);
    }

    /// Turn, tilt and zoom it for `dt` seconds of the camera controls `held`: rotate left looks
    /// further left, tilt up looks further up, zoom in comes closer.
    pub fn turn_held(&mut self, held: CameraHeld, dt: f32) {
        let axis = |neg: bool, pos: bool| f32::from(u8::from(pos)) - f32::from(u8::from(neg));
        let turn = axis(held.rotate_left, held.rotate_right);
        if turn != 0.0 {
            self.swing = 0.0;
        }
        let rate = self.settings.key_turn * dt;
        self.heading = (self.heading + (turn * rate).to_degrees()).rem_euclid(360.0);
        self.pitch =
            (self.pitch + axis(held.tilt_down, held.tilt_up) * rate).clamp(PITCH_MIN, PITCH_MAX);
        self.zoom(axis(held.zoom_out, held.zoom_in) * ZOOM_RATE * dt);
    }

    /// Bring it `notches` steps closer (further for a negative count).
    pub fn zoom(&mut self, notches: f32) {
        self.distance = (self.distance - notches * ZOOM_STEP).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    /// Swing it `dt` seconds of the way round behind a player facing `heading`, in [`RECENTRE`]
    /// seconds: setting off and slowing smoothly, never past directly behind, the same at any
    /// frame rate.
    pub fn ease_behind(&mut self, heading: f32, dt: f32) {
        let off = wrap(self.heading - heading);
        let rate = RECENTRE_SPRING / RECENTRE;
        let dt = dt.max(0.0);
        let fade = math::expf(-rate * dt);
        let push = (self.swing + rate * off) * dt;
        let mut next = (off + push) * fade;
        self.swing = (self.swing - rate * push) * fade;
        if next * off <= 0.0 {
            next = 0.0;
            self.swing = 0.0;
        }
        self.heading = (heading + next).rem_euclid(360.0);
    }

    /// One frame of `dt` seconds: set behind a player facing `heading` the first time; turned by
    /// the mouse while a button is held, by the camera controls `held` and the zoom steps; turned
    /// round while the look-at-front control is held; under character-based movement brought
    /// round behind the player while the movement `asked` moves or turns them and no button is
    /// held.
    pub fn step(
        &mut self,
        held: CameraHeld,
        input: &OrbitInput,
        asked: &Steer,
        heading: f32,
        dt: f32,
    ) {
        if !self.placed {
            self.place_behind(heading);
        }
        let button = input.buttons.0 || input.buttons.1;
        if button {
            self.rotate(input.mouse.0, input.mouse.1);
        }
        self.turn_held(held, dt);
        #[allow(clippy::cast_precision_loss)] // a frame's few steps
        self.zoom(input.zoom as f32);
        self.front_view = input.front_view;
        let moves = asked.forward != 0.0 || asked.turn != 0.0;
        if self.settings.movement == MovementMode::Character && moves && !button {
            self.ease_behind(heading, dt);
        }
    }

    /// The way it looks this frame, degrees clockwise from north: its own, or back at the player
    /// while the look-at-front control is held.
    #[must_use]
    pub fn view_heading(&self) -> f32 {
        if self.front_view {
            (self.heading + 180.0).rem_euclid(360.0)
        } else {
            self.heading
        }
    }

    /// Where it would stand looking at `pivot` (in the frame of the player's landblock), and the
    /// way it looks, a unit vector: before walls bring it closer.
    #[must_use]
    pub fn eye(&self, pivot: Vec3) -> (Vec3, Vec3) {
        let h = self.view_heading().to_radians();
        let (sh, ch) = (math::sinf(h), math::cosf(h));
        let (sp, cp) = (math::sinf(self.pitch), math::cosf(self.pitch));
        let look = Vec3::new(sh * cp, ch * cp, sp);
        let origin = Vec3::new(
            pivot.x - look.x * self.distance,
            pivot.y - look.y * self.distance,
            pivot.z - look.z * self.distance,
        );
        (origin, look)
    }

    /// The body's steering for the movement the player's controls `asked`, under the settings'
    /// movement (see the module documentation); `moving_to` while the server moves the body to a
    /// place, which the camera never turns.
    #[must_use]
    pub fn steer(&self, asked: Steer, input: &OrbitInput, moving_to: bool) -> Steer {
        let facing = |h: f32| (!moving_to).then_some(h.rem_euclid(360.0));
        match self.settings.movement {
            MovementMode::Camera => {
                let ahead = asked.forward;
                let right = (asked.sideways - asked.turn).clamp(-1.0, 1.0);
                if ahead == 0.0 && right == 0.0 {
                    return Steer {
                        forward: 0.0,
                        sideways: 0.0,
                        turn: 0.0,
                        heading: None,
                        camera_steers: false,
                        ..asked
                    };
                }
                let pace = if ahead == 0.0 {
                    right.abs()
                } else {
                    ahead.abs()
                };
                let off = math::atan2f(sign(right), sign(ahead)).to_degrees();
                Steer {
                    forward: pace,
                    sideways: 0.0,
                    turn: 0.0,
                    heading: facing(self.heading + off),
                    camera_steers: true,
                    ..asked
                }
            }
            MovementMode::Character if input.buttons.1 => Steer {
                sideways: (asked.sideways - asked.turn).clamp(-1.0, 1.0),
                turn: 0.0,
                heading: facing(self.heading),
                camera_steers: true,
                ..asked
            },
            MovementMode::Character => Steer {
                heading: None,
                camera_steers: false,
                ..asked
            },
        }
    }
}
