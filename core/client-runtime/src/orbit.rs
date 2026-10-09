//! The orbit camera an interface may choose in place of the game's own: it circles the player at
//! a distance the wheel sets, turned by the mouse or the look keys, and the player's movement can
//! follow where it looks.
//!
//! It is a camera of its own, beside the game's: its state is kept apart, so an interface that
//! puts it down leaves the game's camera where that one was. Where it stands is still found by the
//! game's own swept viewer sphere, so it stops at walls and floors as the game's camera does.

use dereth_primitives::{Frame, Vec3};

use crate::camera::FreeCamera;

/// How the movement keys move the player while the orbit camera is in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum MovementMode {
    /// Forward runs where the camera looks; Back turns the player round at once and runs toward
    /// the camera; the turning keys step sideways, as the strafe keys do.
    #[default]
    Camera,
    /// The game's own: Forward and Back move along the player's facing, and the turning keys
    /// turn.
    Character,
}

/// What the interface chose for the orbit camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitSettings {
    pub movement: MovementMode,
    /// Turn the camera the other way when the pointer moves sideways.
    pub reverse_x: bool,
    /// Tilt the camera the other way when the pointer moves up or down.
    pub reverse_y: bool,
    /// Under camera-based movement, step sideways at the game's sidestep pace instead of running
    /// the way the keys point.
    pub sidestep: bool,
    /// Radians the camera turns for one pixel of pointer movement.
    pub mouse_turn: f32,
    /// Radians a second the look keys turn it.
    pub key_turn: f32,
    /// How far it tilts down and up, in radians (down is negative).
    pub pitch_min: f32,
    pub pitch_max: f32,
    /// Metres the point it looks at is raised above where it would be, to look over the
    /// player's head; 0 looks where it always has.
    pub height: f32,
}

impl Default for OrbitSettings {
    fn default() -> Self {
        Self {
            movement: MovementMode::default(),
            reverse_x: false,
            reverse_y: false,
            sidestep: false,
            mouse_turn: MOUSE_TURN,
            key_turn: KEY_TURN,
            pitch_min: PITCH_MIN,
            pitch_max: PITCH_MAX,
            height: 0.0,
        }
    }
}

/// The ranges the adjustable settings are held to, whatever a settings file says.
pub mod limits {
    /// Pointer turn, radians a pixel.
    pub const MOUSE_TURN: (f32, f32) = (0.001, 0.02);
    /// Key turn, radians a second.
    pub const KEY_TURN: (f32, f32) = (0.3, 6.0);
    /// The lowest the tilt may be set to go: from straight down to level.
    pub const PITCH_MIN: (f32, f32) = (-1.5, 0.0);
    /// The highest: from level to nearly straight up.
    pub const PITCH_MAX: (f32, f32) = (0.0, 1.5);
    /// The look point's raise, metres.
    pub const HEIGHT: (f32, f32) = (-1.0, 3.0);
}

impl OrbitSettings {
    /// These settings held to their ranges: a value out of range, or not a number, is brought
    /// back into it.
    #[must_use]
    pub fn held_to_ranges(self) -> Self {
        let hold = |v: f32, (lo, hi): (f32, f32), default: f32| {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                default
            }
        };
        Self {
            mouse_turn: hold(self.mouse_turn, limits::MOUSE_TURN, MOUSE_TURN),
            key_turn: hold(self.key_turn, limits::KEY_TURN, KEY_TURN),
            pitch_min: hold(self.pitch_min, limits::PITCH_MIN, PITCH_MIN),
            pitch_max: hold(self.pitch_max, limits::PITCH_MAX, PITCH_MAX),
            height: hold(self.height, limits::HEIGHT, 0.0),
            ..self
        }
    }
}

/// How far above the player's feet the camera looks, in metres.
pub const PIVOT_HEIGHT: f32 = 1.5;
/// How close and how far the wheel brings the camera, in metres: at its closest the head and
/// shoulders fill the lower half of the view; at its farthest the character stands among the
/// buildings around it, still plain to see.
pub const MIN_DISTANCE: f32 = 1.0;
pub const MAX_DISTANCE: f32 = 15.0;
/// Where it starts.
pub const DEFAULT_DISTANCE: f32 = 6.0;
/// How far one notch of the wheel moves it.
pub const ZOOM_STEP: f32 = 1.0;
/// Its tilt: where it starts, a little above the player looking down, and how far it goes (down
/// is negative).
pub const DEFAULT_PITCH: f32 = -0.3;
pub const PITCH_MIN: f32 = -1.35;
pub const PITCH_MAX: f32 = 0.9;
/// Radians the camera turns for one pixel of pointer movement.
pub const MOUSE_TURN: f32 = 0.005;
/// Radians a second the look keys turn it.
pub const KEY_TURN: f32 = 1.6;
/// How quickly what is shown closes on where the camera is set, per second: the turn and tilt
/// quickly, the distance more gently. Each frame takes
/// `1 - e^(-rate * dt)` of the gap, so the ease is the same at any frame rate.
pub const TURN_EASE: f32 = 22.0;
pub const DISTANCE_EASE: f32 = 9.0;
/// Notches a second a held zoom key moves it, once it has been held [`ZOOM_HOLD_DELAY`] seconds.
pub const ZOOM_RATE: f32 = 6.0;
pub const ZOOM_HOLD_DELAY: f32 = 0.3;

/// The orbit camera's state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitCamera {
    /// Where it looks, as the free camera's yaw: radians counter-clockwise from north.
    pub yaw: f32,
    /// Positive looks up.
    pub pitch: f32,
    /// How far it stands from the point it looks at, before walls bring it closer.
    pub distance: f32,
    /// Turned round to look at the player's front, while that is held.
    pub front_view: bool,
    pub settings: OrbitSettings,
    /// Whether it has been set behind the player yet.
    pub placed: bool,
    /// The game is turning the player itself (toward what they cast at, use or fight), and the
    /// camera comes round behind them as they turn.
    pub follow_behind: bool,
    /// The player turned it by turning themselves, and it goes on round until it is directly
    /// behind them, after the turning stops too, unless it is turned by hand meanwhile.
    pub settling_behind: bool,
    /// A zoom held: `1` closer, `-1` further, `0` none; and for how long.
    zoom_hold: f32,
    zoom_held_for: f32,
    /// What is shown: the turn, tilt and distance easing toward the ones set, and the point it
    /// looks at easing toward the player. `None` until the first frame is shown.
    shown: Option<Shown>,
}

/// The orbit camera as it is shown this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Shown {
    yaw: f32,
    pitch: f32,
    distance: f32,
    pivot: Vec3,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: DEFAULT_PITCH,
            distance: DEFAULT_DISTANCE,
            front_view: false,
            settings: OrbitSettings::default(),
            placed: false,
            follow_behind: false,
            settling_behind: false,
            zoom_hold: 0.0,
            zoom_held_for: 0.0,
            shown: None,
        }
    }
}

/// How fast the orbit camera is let back out once what was in its way is gone: the share of the
/// rest it closes in a second, as an ease rate.
pub const CLEAR_EASE: f32 = 5.0;

/// How far short of where it is set the orbit camera is shown, from how far short it was shown
/// last frame and how far short the way allows it now: in at once, out at [`CLEAR_EASE`].
#[must_use]
pub fn eased_shortfall(shown: f32, now: f32, dt: f32) -> f32 {
    if now >= shown {
        now
    } else {
        shown + (now - shown) * ease_share(CLEAR_EASE, dt)
    }
}

/// How fast the orbit camera comes round behind a player the game turns: the share of the rest
/// of the turn it closes in a second, as an ease rate.
pub const BEHIND_EASE: f32 = 2.5;

/// How close to directly behind, in radians, a camera settling after a turn counts as there.
const SETTLED: f32 = 0.002;

/// Whether the orbit camera comes round behind the player: while the game turns them itself
/// (`moved_by_game`, a move or turn toward something used, cast at or fought), unless the player
/// steers them (`steered`) or holds a mouse button, which keeps the camera where it is.
#[must_use]
pub fn follows_behind(steered: bool, button_held: bool, moved_by_game: bool) -> bool {
    moved_by_game && !steered && !button_held
}

/// The share of a gap an ease at `rate` closes in `dt` seconds.
fn ease_share(rate: f32, dt: f32) -> f32 {
    1.0 - dereth_primitives::num::math::expf(-rate * dt.max(0.0))
}

/// The free camera's yaw for a heading in degrees clockwise from north.
fn yaw_of_heading(degrees: f32) -> f32 {
    -degrees.to_radians()
}

/// `angle` brought into `(-pi, pi]`.
fn wrap(angle: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let a = angle.rem_euclid(tau);
    if a > std::f32::consts::PI {
        a - tau
    } else {
        a
    }
}

impl OrbitCamera {
    /// Stand behind a player facing `heading` (degrees clockwise from north), looking the way
    /// they face.
    pub fn place_behind(&mut self, heading: f32) {
        self.yaw = yaw_of_heading(heading);
        self.placed = true;
    }

    /// Turn it by a pointer movement of `(dx, dy)` pixels: moving right looks further right (the
    /// camera swings left round the player) and moving up looks further up (the camera sinks),
    /// each the other way when the settings reverse it.
    pub fn rotate(&mut self, dx: f32, dy: f32) {
        let x = if self.settings.reverse_x { -1.0 } else { 1.0 };
        let y = if self.settings.reverse_y { -1.0 } else { 1.0 };
        let s = self.settings;
        self.yaw = wrap(self.yaw - dx * s.mouse_turn * x);
        self.pitch = (self.pitch - dy * s.mouse_turn * y).clamp(s.pitch_min, s.pitch_max);
        if dx != 0.0 {
            self.settling_behind = false;
        }
    }

    /// Turn it for `dt` seconds of the look keys held: left, right, up, down.
    pub fn turn_held(&mut self, left: bool, right: bool, up: bool, down: bool, dt: f32) {
        if left || right {
            self.settling_behind = false;
        }
        let axis = |neg: bool, pos: bool| f32::from(u8::from(pos)) - f32::from(u8::from(neg));
        let s = self.settings;
        self.yaw = wrap(self.yaw + s.key_turn * dt * axis(right, left));
        self.pitch =
            (self.pitch + s.key_turn * dt * axis(down, up)).clamp(s.pitch_min, s.pitch_max);
    }

    /// Bring it `notches` of the wheel closer (or further, for a negative count).
    pub fn zoom(&mut self, notches: f32) {
        self.distance = (self.distance - notches * ZOOM_STEP).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    /// A zoom begun, closer for `closer`: one notch at once, as a turn of the wheel gives, and
    /// on at a steady rate while it stays held, as a key gives.
    pub fn start_zoom(&mut self, closer: bool) {
        let dir = if closer { 1.0 } else { -1.0 };
        self.zoom(dir);
        self.zoom_hold = dir;
        self.zoom_held_for = 0.0;
    }

    /// A zoom let go.
    pub fn stop_zoom(&mut self) {
        self.zoom_hold = 0.0;
    }

    /// `dt` seconds of a held zoom.
    pub fn step_zoom(&mut self, dt: f32) {
        if self.zoom_hold == 0.0 {
            return;
        }
        self.zoom_held_for += dt;
        if self.zoom_held_for > ZOOM_HOLD_DELAY {
            self.zoom(self.zoom_hold * ZOOM_RATE * dt);
        }
    }

    /// Bring it `dt` seconds of the way round behind a player facing `heading` (degrees clockwise
    /// from north), gently, at the same pace at any frame rate.
    pub fn ease_behind(&mut self, heading: f32, dt: f32) {
        let gap = wrap(yaw_of_heading(heading) - self.yaw);
        self.yaw = wrap(self.yaw + gap * ease_share(BEHIND_EASE, dt));
    }

    /// Bring it `dt` seconds on round behind a player facing `heading`, while it is settling after
    /// a turn of theirs: it goes on after the turning stops, and settles once directly behind.
    pub fn settle_behind(&mut self, heading: f32, dt: f32) {
        if !self.settling_behind {
            return;
        }
        self.ease_behind(heading, dt);
        if wrap(yaw_of_heading(heading) - self.yaw).abs() < SETTLED {
            self.yaw = yaw_of_heading(heading);
            self.settling_behind = false;
        }
    }

    /// Turn it with the player, who turned by `degrees` clockwise: it keeps its place behind them
    /// as they turn. What is shown turns at once too, so it rides with them without lagging.
    pub fn turn_with_player(&mut self, degrees: f32) {
        self.settling_behind = true;
        let r = -degrees.to_radians();
        self.yaw = wrap(self.yaw + r);
        if let Some(s) = &mut self.shown {
            s.yaw = wrap(s.yaw + r);
        }
    }

    /// Ease what is shown `dt` seconds toward where the camera is set, looking at `pivot`, which
    /// it follows exactly; the first frame is taken at once.
    pub fn ease(&mut self, pivot: Vec3, dt: f32) {
        let target = Shown {
            yaw: self.view_yaw(),
            pitch: self.pitch,
            distance: self.distance,
            pivot,
        };
        let Some(s) = &mut self.shown else {
            self.shown = Some(target);
            return;
        };
        // The point it looks at is the body where it is drawn, which moves smoothly every frame:
        // followed exactly, so the body stands still on the screen while the world goes by.
        s.pivot = pivot;
        let k = ease_share(TURN_EASE, dt);
        s.yaw = wrap(s.yaw + wrap(target.yaw - s.yaw) * k);
        s.pitch += (target.pitch - s.pitch) * k;
        s.distance += (target.distance - s.distance) * ease_share(DISTANCE_EASE, dt);
    }

    /// Where it is shown standing and looking this frame, as [`Self::ease`] left it; where it is
    /// set, looking at `pivot`, before the first ease.
    #[must_use]
    pub fn shown_eye(&self, pivot: Vec3) -> Frame {
        let Some(s) = self.shown else {
            return self.eye(pivot);
        };
        let look = FreeCamera::new(Vec3::ZERO, s.yaw, s.pitch).forward();
        let origin = Vec3::new(
            s.pivot.x - look.x * s.distance,
            s.pivot.y - look.y * s.distance,
            s.pivot.z - look.z * s.distance,
        );
        FreeCamera::new(origin, s.yaw, s.pitch).frame()
    }

    /// The point it is shown looking at this frame.
    #[must_use]
    pub fn shown_pivot(&self, pivot: Vec3) -> Vec3 {
        self.shown.map_or(pivot, |s| s.pivot)
    }

    /// The way it looks as it is shown, in degrees clockwise from north: turned round while the
    /// front view is held, and where its easing has brought it this frame.
    #[must_use]
    pub fn facing_degrees(&self) -> f32 {
        let yaw = self.shown.map_or_else(|| self.view_yaw(), |s| s.yaw);
        (-yaw.to_degrees()).rem_euclid(360.0)
    }

    /// Where it looks this frame: its own way, or back at the player while the front view is held.
    #[must_use]
    pub fn view_yaw(&self) -> f32 {
        if self.front_view {
            wrap(self.yaw + std::f32::consts::PI)
        } else {
            self.yaw
        }
    }

    /// The point it looks at for a player whose feet are at `feet`.
    #[must_use]
    pub fn pivot(feet: Vec3) -> Vec3 {
        Vec3::new(feet.x, feet.y, feet.z + PIVOT_HEIGHT)
    }

    /// The point it looks at for a player whose feet are at `feet`, raised by the settings' height.
    #[must_use]
    pub fn raised_pivot(&self, feet: Vec3) -> Vec3 {
        let p = Self::pivot(feet);
        Vec3::new(p.x, p.y, p.z + self.settings.height)
    }

    /// Take `settings`, held to their ranges, with the tilt brought inside the limits they set.
    pub fn apply_settings(&mut self, settings: OrbitSettings) {
        let s = settings.held_to_ranges();
        self.settings = s;
        self.pitch = self.pitch.clamp(s.pitch_min, s.pitch_max);
    }

    /// Where it would stand looking at `pivot`, and which way it looks: the frame the renderer
    /// draws from, before walls bring it closer.
    #[must_use]
    pub fn eye(&self, pivot: Vec3) -> Frame {
        let yaw = self.view_yaw();
        let look = FreeCamera::new(Vec3::ZERO, yaw, self.pitch).forward();
        let origin = Vec3::new(
            pivot.x - look.x * self.distance,
            pivot.y - look.y * self.distance,
            pivot.z - look.z * self.distance,
        );
        FreeCamera::new(origin, yaw, self.pitch).frame()
    }

    /// The heading, in degrees clockwise from north, a player moving with the camera faces: where
    /// it looks, or straight away from it when `away`. The front view is left out, so holding it
    /// does not turn the player's way round.
    #[must_use]
    pub fn movement_heading(&self, away: bool) -> f32 {
        let heading = -self.yaw.to_degrees() + if away { 180.0 } else { 0.0 };
        heading.rem_euclid(360.0)
    }
}

/// The movement keys held, as the orbit camera's movement reads them, and both mouse buttons held
/// together, which run forward.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HeldMovement {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    /// Both mouse buttons are held: forward, where the camera looks.
    pub mouse: bool,
}

impl HeldMovement {
    /// Whether any of them is held.
    #[must_use]
    pub fn any(&self) -> bool {
        self.ahead() != 0 || self.forward || self.back || self.side() != 0
    }

    /// Whether the player runs away from the camera: Back held without Forward.
    #[must_use]
    pub fn away(&self) -> bool {
        self.back && !self.forward && !self.mouse
    }

    /// Forward against back: `1`, `0` or `-1`.
    fn ahead(&self) -> i8 {
        i8::from(self.forward || self.mouse) - i8::from(self.back)
    }

    /// Right against left: `1`, `0` or `-1`.
    fn side(&self) -> i8 {
        i8::from(self.right) - i8::from(self.left)
    }

    /// Note one movement key's action: which of the four it holds.
    pub fn note(&mut self, id: dereth_client_contract::actions::ActionId, down: bool) {
        use dereth_client_contract::actions::movement as a;
        match id {
            a::MOVE_FORWARD => self.forward = down,
            a::MOVE_BACKWARD => self.back = down,
            a::TURN_LEFT | a::STRAFE_LEFT => self.left = down,
            a::TURN_RIGHT | a::STRAFE_RIGHT => self.right = down,
            _ => {}
        }
    }

    /// The heading, in degrees clockwise from north, a player moving with a camera looking along
    /// `camera_heading` faces, under camera-based movement; `None` with nothing held.
    ///
    /// Running the way the keys point, the player faces that way: forward is the camera's way,
    /// back toward the camera, left and right across it, and two together between. Stepping
    /// sideways instead, they face the camera's way, or away from it with Back held alone.
    #[must_use]
    pub fn heading(&self, camera_heading: f32, sidestep: bool) -> Option<f32> {
        if !self.any() {
            return None;
        }
        let turn = if sidestep {
            if self.away() {
                180.0
            } else {
                0.0
            }
        } else {
            let (f, r) = (f32::from(self.ahead()), f32::from(self.side()));
            if f == 0.0 && r == 0.0 {
                return None;
            }
            dereth_primitives::num::math::atan2f(r, f).to_degrees()
        };
        Some((camera_heading + turn).rem_euclid(360.0))
    }
}

/// What the movement keys do while the orbit camera is in use, turned into the game's movement:
/// the keys held, what has been asked of the game for them, and the run/walk latch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MovementKeys {
    pub held: HeldMovement,
    /// The game is running the player forward for the keys.
    running: bool,
    /// The sidestep (or, under character-based movement, the turn) each side's key asked for,
    /// left then right, while it is held.
    stepping: [Option<dereth_client_contract::actions::ActionId>; 2],
    /// Walking rather than running: the run/walk key flips it.
    pub walking: bool,
    /// The mouse buttons: left, right.
    pub buttons: (bool, bool),
    /// A spell is being cast.
    casting: bool,
    /// The player's facing is held where it is ([`Self::locked`]).
    locked: bool,
    /// The turning keys held, left then right.
    turns: [bool; 2],
    /// The sidestep keys held, left then right.
    strafes: [bool; 2],
    /// What the game has been asked to keep doing while the facing is held under camera-based
    /// movement: forward, back, and a step to each side.
    sent: SentMovement,
}

/// The game's own movement asked for, held while its keys are.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct SentMovement {
    forward: bool,
    back: bool,
    sides: [bool; 2],
}

impl MovementKeys {
    /// Turn one action into what the game is asked for under `settings`.
    ///
    /// Everywhere, the run/walk key flips between running and walking at each press instead of
    /// walking while it is held, and while the player's facing is held ([`Self::locked`]: a spell
    /// being cast) the movement keys are the game's own with the turning keys stepping sideways:
    /// Forward walks ahead, Back backs up and the sides step, together as they are held, the
    /// player keeping the way they face. Under character-based movement the movement keys are the
    /// game's own (they are still noted, so the camera can turn with the player), except that the
    /// turning keys step sideways while the mouse steers the player. Otherwise, under
    /// camera-based movement:
    /// - running the way the keys point (the default), any of the four starts the player running
    ///   forward and the last let go stops them; which way they face is the keys' direction
    ///   against the camera ([`HeldMovement::heading`]), so sideways and back are run at the
    ///   running pace;
    /// - stepping sideways instead, Forward and Back both run forward (Back facing the camera),
    ///   and Left and Right step sideways, across the camera's view whichever way the player
    ///   faces: facing the camera, the steps are swapped, and a step held when the player turns
    ///   round is swapped then.
    #[must_use]
    pub fn convert(
        &mut self,
        e: dereth_client_contract::actions::Action,
        settings: OrbitSettings,
    ) -> Vec<dereth_client_contract::actions::Action> {
        use dereth_client_contract::actions::{movement as a, Action, ActionPhase};
        if e.id == a::TOGGLE_RUN_WALK {
            if e.phase != ActionPhase::Begin {
                return Vec::new();
            }
            self.walking = !self.walking;
            return vec![if self.walking {
                Action::begin(a::TOGGLE_RUN_WALK)
            } else {
                Action::end(a::TOGGLE_RUN_WALK)
            }];
        }
        let movement = matches!(
            e.id,
            a::MOVE_FORWARD
                | a::MOVE_BACKWARD
                | a::TURN_LEFT
                | a::TURN_RIGHT
                | a::STRAFE_LEFT
                | a::STRAFE_RIGHT
        );
        if !movement {
            return vec![e];
        }
        if e.phase != ActionPhase::Repeat {
            match e.id {
                a::TURN_LEFT => self.turns[0] = e.is_start(),
                a::TURN_RIGHT => self.turns[1] = e.is_start(),
                a::STRAFE_LEFT => self.strafes[0] = e.is_start(),
                a::STRAFE_RIGHT => self.strafes[1] = e.is_start(),
                _ => {}
            }
        }
        let mut out = if settings.movement == MovementMode::Character {
            self.held.note(e.id, e.is_start());
            self.character_turn(e)
        } else if self.locked {
            if e.phase != ActionPhase::Repeat {
                self.held.note(e.id, e.is_start());
            }
            self.send_held()
        } else {
            self.convert_camera(e, settings)
        };
        out.extend(self.relock(settings));
        out
    }

    /// Whether the player's facing is held where it is: from the moment a spell begins to be cast
    /// until it is over and the movement keys held through it are let go, so that a key held from
    /// the cast on does not turn the player once it ends.
    #[must_use]
    pub fn locked(&self) -> bool {
        self.locked
    }

    /// A spell began or stopped being cast (`casting`), under `settings`: the player's facing is
    /// held from now, or no longer once the keys held through the cast are let go.
    #[must_use]
    pub fn set_casting(
        &mut self,
        casting: bool,
        settings: OrbitSettings,
    ) -> Vec<dereth_client_contract::actions::Action> {
        self.casting = casting;
        self.relock(settings)
    }

    /// The heading, in degrees clockwise from north, the player is turned to face this frame under
    /// `settings`, with the camera looking along `camera_heading`; `None` leaves them facing as
    /// they do. Camera-based, they face the way the keys point against the camera; character-based,
    /// the camera's way while the mouse steers. While the facing is held ([`Self::locked`]) they
    /// are turned only by the mouse steering under character-based movement.
    #[must_use]
    pub fn facing(&self, settings: OrbitSettings, camera_heading: f32) -> Option<f32> {
        match settings.movement {
            MovementMode::Camera if self.locked => None,
            MovementMode::Camera => self.held.heading(camera_heading, settings.sidestep),
            MovementMode::Character => self.mouse_steers().then_some(camera_heading),
        }
    }

    /// Whether the camera comes round behind a player the game turns itself (`moved_by_game`),
    /// the player being faced `face` this frame ([`Self::facing`]): as [`follows_behind`] has it.
    /// While the facing is held that includes the game turning the player to a spell's target, so
    /// the camera is behind them as they back up and step away from it, and they face on the
    /// camera's way when the keys are let go.
    #[must_use]
    pub fn camera_follows_game_turn(&self, face: Option<f32>, moved_by_game: bool) -> bool {
        follows_behind(
            face.is_some(),
            self.buttons.0 || self.buttons.1,
            moved_by_game,
        )
    }

    /// Whether any movement key is held.
    fn keys_held(&self) -> bool {
        self.held.forward
            || self.held.back
            || self.turns.iter().any(|t| *t)
            || self.strafes.iter().any(|t| *t)
    }

    /// Hold the facing, or let it go, as the cast and the keys have it, and turn what the keys
    /// ask of the game over to the new way of reading them.
    fn relock(&mut self, settings: OrbitSettings) -> Vec<dereth_client_contract::actions::Action> {
        use dereth_client_contract::actions::{movement as a, Action};
        let lock = self.casting || (self.locked && self.keys_held());
        if lock == self.locked {
            return Vec::new();
        }
        self.locked = lock;
        if settings.movement == MovementMode::Character {
            return self.steering_changed(settings);
        }
        let mut out = Vec::new();
        if lock {
            // What camera-based movement had asked for ends; the keys held go on as the game's.
            if self.running {
                self.running = false;
                if self.held.forward || self.held.mouse {
                    // Forward goes on as it was, now along the way the player faces.
                    self.sent.forward = true;
                } else {
                    out.push(Action::end(a::MOVE_FORWARD));
                }
            }
            for side in 0..2 {
                if let Some(id) = self.stepping[side].take() {
                    out.push(Action::end(id));
                }
            }
            out.extend(self.send_held());
        } else {
            // Nothing is held now but, perhaps, both mouse buttons: what was asked for ends, and
            // camera-based movement takes the mouse's run over.
            let mouse = self.held.mouse;
            self.held.mouse = false;
            out.extend(self.send_held());
            self.held.mouse = mouse;
            out.extend(self.run_for_held(!settings.sidestep));
        }
        out
    }

    /// Bring what the game is asked for in line with the keys held, read as the game's own
    /// movement with the turning keys stepping sideways: Forward (or both mouse buttons) walks
    /// ahead, Back backs up, and each side steps.
    fn send_held(&mut self) -> Vec<dereth_client_contract::actions::Action> {
        use dereth_client_contract::actions::{movement as a, Action};
        let want = SentMovement {
            forward: self.held.forward || self.held.mouse,
            back: self.held.back,
            sides: [
                self.turns[0] || self.strafes[0],
                self.turns[1] || self.strafes[1],
            ],
        };
        let mut out = Vec::new();
        let mut change = |was: &mut bool, now: bool, id| {
            if *was != now {
                *was = now;
                out.push(if now {
                    Action::begin(id)
                } else {
                    Action::end(id)
                });
            }
        };
        change(&mut self.sent.forward, want.forward, a::MOVE_FORWARD);
        change(&mut self.sent.back, want.back, a::MOVE_BACKWARD);
        change(&mut self.sent.sides[0], want.sides[0], a::STRAFE_LEFT);
        change(&mut self.sent.sides[1], want.sides[1], a::STRAFE_RIGHT);
        out
    }

    /// One movement key under camera-based movement, read under `settings`.
    fn convert_camera(
        &mut self,
        e: dereth_client_contract::actions::Action,
        settings: OrbitSettings,
    ) -> Vec<dereth_client_contract::actions::Action> {
        use dereth_client_contract::actions::{movement as a, Action, ActionPhase};
        if e.phase == ActionPhase::Repeat {
            return Vec::new();
        }
        let was_away = self.held.away();
        self.held.note(e.id, e.is_start());
        let mut out = self.run_for_held(!settings.sidestep);
        if !settings.sidestep {
            return out;
        }
        let side = match e.id {
            a::TURN_LEFT | a::STRAFE_LEFT => Some(0),
            a::TURN_RIGHT | a::STRAFE_RIGHT => Some(1),
            _ => None,
        };
        let away = self.held.away();
        let step = |side: usize| match (side, away) {
            (0, false) | (1, true) => a::STRAFE_LEFT,
            _ => a::STRAFE_RIGHT,
        };
        if let Some(side) = side {
            if e.is_start() {
                let id = step(side);
                self.stepping[side] = Some(id);
                out.push(Action::begin(id));
            } else if let Some(id) = self.stepping[side].take() {
                out.push(Action::end(id));
            }
        } else if was_away != away {
            // Turned round with a step held: it goes on across the camera's view.
            for side in 0..2 {
                if let Some(old) = self.stepping[side] {
                    let new = step(side);
                    if new != old {
                        out.push(Action::end(old));
                        out.push(Action::begin(new));
                        self.stepping[side] = Some(new);
                    }
                }
            }
        }
        out
    }

    /// Whether the mouse steers the player, facing them where the camera looks: the right button
    /// held, or both.
    #[must_use]
    pub fn mouse_steers(&self) -> bool {
        self.buttons.1 || self.held.mouse
    }

    /// Whether, under `settings`, the player turning carries the orbit camera round with them:
    /// under character-based movement, with a turning key held, unless the mouse steers the
    /// player or the facing is held (the keys then step sideways) or the left button holds the
    /// camera where the pointer put it.
    #[must_use]
    pub fn camera_turns_with_player(&self, settings: OrbitSettings) -> bool {
        settings.movement == MovementMode::Character
            && !self.mouse_steers()
            && !self.locked
            && !self.buttons.0
            && (self.held.left || self.held.right)
    }

    /// One movement key under character-based movement: the game's own, except that while the
    /// mouse steers the player the turning keys step sideways instead, the mouse having the
    /// turning.
    fn character_turn(
        &mut self,
        e: dereth_client_contract::actions::Action,
    ) -> Vec<dereth_client_contract::actions::Action> {
        use dereth_client_contract::actions::{movement as a, Action};
        let side = match e.id {
            a::TURN_LEFT => 0,
            a::TURN_RIGHT => 1,
            _ => return vec![e],
        };
        if e.is_start() && self.stepping[side].is_none() {
            self.stepping[side] = Some(self.character_side(side));
        }
        let id = self.stepping[side].unwrap_or(e.id);
        if !e.is_start() {
            self.stepping[side] = None;
        }
        vec![Action { id, ..e }]
    }

    /// What a turning key held on `side` (left `0`, right `1`) asks of the game under
    /// character-based movement: a turn, or a sidestep while the mouse steers or the facing is
    /// held.
    fn character_side(&self, side: usize) -> dereth_client_contract::actions::ActionId {
        use dereth_client_contract::actions::movement as a;
        match (side, self.mouse_steers() || self.locked) {
            (0, false) => a::TURN_LEFT,
            (0, true) => a::STRAFE_LEFT,
            (_, false) => a::TURN_RIGHT,
            (_, true) => a::STRAFE_RIGHT,
        }
    }

    /// The mouse began or stopped steering the player under `settings`: under character-based
    /// movement a turning key held then goes on as a sidestep, or as a turn again.
    #[must_use]
    pub fn steering_changed(
        &mut self,
        settings: OrbitSettings,
    ) -> Vec<dereth_client_contract::actions::Action> {
        use dereth_client_contract::actions::Action;
        let mut out = Vec::new();
        if settings.movement != MovementMode::Character {
            return out;
        }
        for side in 0..2 {
            if let Some(old) = self.stepping[side] {
                let new = self.character_side(side);
                if new != old {
                    out.push(Action::end(old));
                    out.push(Action::begin(new));
                    self.stepping[side] = Some(new);
                }
            }
        }
        out
    }

    /// Both mouse buttons held together (`on`) or not: the player runs forward where the camera
    /// looks while they are. What the game is asked for, under either movement.
    #[must_use]
    pub fn mouse_run(
        &mut self,
        on: bool,
        settings: OrbitSettings,
    ) -> Vec<dereth_client_contract::actions::Action> {
        use dereth_client_contract::actions::{movement as a, Action};
        if self.held.mouse == on {
            return Vec::new();
        }
        self.held.mouse = on;
        if settings.movement == MovementMode::Camera && self.locked {
            return self.send_held();
        }
        if settings.movement == MovementMode::Character {
            // The movement keys are the game's own here: the mouse's run starts and stops on its
            // own, and does not stop a run Forward is holding.
            return if on {
                vec![Action::begin(a::MOVE_FORWARD)]
            } else if self.held.forward {
                Vec::new()
            } else {
                vec![Action::end(a::MOVE_FORWARD)]
            };
        }
        self.run_for_held(!settings.sidestep)
    }

    /// Start or stop the forward run as what is held asks: with `any_key`, any of them runs;
    /// otherwise only Forward, Back and the mouse do.
    fn run_for_held(&mut self, any_key: bool) -> Vec<dereth_client_contract::actions::Action> {
        use dereth_client_contract::actions::{movement as a, Action};
        let h = self.held;
        let want = if any_key {
            h.any()
        } else {
            h.forward || h.back || h.mouse
        };
        if want == self.running {
            return Vec::new();
        }
        self.running = want;
        vec![if want {
            Action::begin(a::MOVE_FORWARD)
        } else {
            Action::end(a::MOVE_FORWARD)
        }]
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (an interface's own camera, not the retail client's)
    use super::*;
    use dereth_client_contract::actions::movement as a;

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    /// The camera's eye and the player's pivot, both in the player's cell space.
    fn eye_and_pivot(c: &crate::character::Character) -> (Vec3, Vec3) {
        let body = c.position();
        let eye = dereth_physics::math::pos_localtoglobal(&body, &c.camera.viewer, Vec3::ZERO);
        (eye, OrbitCamera::pivot(body.frame.origin))
    }

    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
    fn on_a_body_it_stands_behind_the_player_at_its_distance_and_leaves_the_games_camera_alone() {
        use dereth_physics::math::V3 as _;
        let store = std::sync::Arc::new(dereth_dat::testing::open_store().expect("retail dats"));
        let region = dereth_world_data::landblock::load_region(&store).expect("the region");
        let mut c = crate::character::Character::new(
            &store,
            &region,
            dereth_world_data::landblock::DEFAULT_LANDBLOCK,
            (96.0, 96.0),
        )
        .expect("a body");
        let tick = |c: &mut crate::character::Character, n: u32| {
            for i in 0..n {
                c.update_camera(
                    crate::camera::CameraInput::default(),
                    dereth_primitives::LocalTime(10.0 + f64::from(i) / 30.0),
                    1.0 / 30.0,
                );
            }
        };
        tick(&mut c, 3);
        let game_offset = c.camera.manager.viewer_offset;
        c.camera.orbit_active = true;
        tick(&mut c, 3);
        let (eye, pivot) = eye_and_pivot(&c);
        let away = eye.sub(pivot);
        assert!(
            (away.mag2().sqrt() - DEFAULT_DISTANCE).abs() < 0.5,
            "at its distance on open ground: {away:?}"
        );
        let facing = c.camera.orbit.movement_heading(false).to_radians();
        let ahead = Vec3::new(
            dereth_primitives::num::math::sinf(facing),
            dereth_primitives::num::math::cosf(facing),
            0.0,
        );
        assert!(away.dot(ahead) < 0.0, "behind the player");
        assert_eq!(
            c.camera.manager.viewer_offset, game_offset,
            "the game's camera is left as it was"
        );
        // Held, the front view stands it in front.
        c.camera.orbit.front_view = true;
        tick(&mut c, 1);
        let (eye, pivot) = eye_and_pivot(&c);
        assert!(eye.sub(pivot).dot(ahead) > 0.0, "in front of the player");
    }

    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
    fn on_a_body_the_game_turns_it_comes_round_behind_unless_the_look_keys_turn_it() {
        let store = std::sync::Arc::new(dereth_dat::testing::open_store().expect("retail dats"));
        let region = dereth_world_data::landblock::load_region(&store).expect("the region");
        let mut c = crate::character::Character::new(
            &store,
            &region,
            dereth_world_data::landblock::DEFAULT_LANDBLOCK,
            (96.0, 96.0),
        )
        .expect("a body");
        c.camera.orbit_active = true;
        let frames = |c: &mut crate::character::Character, n: u32, input| {
            for i in 0..n {
                c.update_camera(
                    input,
                    dereth_primitives::LocalTime(10.0 + f64::from(i) / 60.0),
                    1.0 / 60.0,
                );
            }
        };
        frames(&mut c, 2, crate::camera::CameraInput::default());
        let set_behind = c.camera.orbit.yaw;
        // The game turns the player a quarter round.
        let heading = dereth_animation::frame::get_heading(&c.position().frame) + 90.0;
        c.face_heading(heading);
        let behind_now = yaw_of_heading(heading);
        // Without the game turning them, the camera stays where it was.
        frames(&mut c, 60, crate::camera::CameraInput::default());
        assert!((c.camera.orbit.yaw - set_behind).abs() < 1e-4);
        // The game turning them: round behind.
        c.camera.orbit.follow_behind = true;
        frames(&mut c, 240, crate::camera::CameraInput::default());
        assert!(
            wrap(c.camera.orbit.yaw - behind_now).abs() < 0.01,
            "behind them"
        );
        // A look key held that frame turns it the player's way instead.
        let before = c.camera.orbit.yaw;
        let left = crate::camera::CameraInput {
            look_left: true,
            ..crate::camera::CameraInput::default()
        };
        frames(&mut c, 30, left);
        assert!(
            wrap(c.camera.orbit.yaw - before) > 0.5,
            "turned by the key: {before} to {}",
            c.camera.orbit.yaw
        );
    }

    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
    fn after_a_quick_turn_of_the_game_s_it_goes_on_round_until_directly_behind() {
        let store = std::sync::Arc::new(dereth_dat::testing::open_store().expect("retail dats"));
        let region = dereth_world_data::landblock::load_region(&store).expect("the region");
        let mut c = crate::character::Character::new(
            &store,
            &region,
            dereth_world_data::landblock::DEFAULT_LANDBLOCK,
            (96.0, 96.0),
        )
        .expect("a body");
        c.camera.orbit_active = true;
        let mut t = 10.0;
        let mut frames = |c: &mut crate::character::Character, n: u32| {
            for _ in 0..n {
                c.update_camera(
                    crate::camera::CameraInput::default(),
                    dereth_primitives::LocalTime(t),
                    1.0 / 60.0,
                );
                t += 1.0 / 60.0;
            }
        };
        frames(&mut c, 2);
        // The game turns the player a quarter round to face what they fight, over a tenth of a
        // second, then the turn is over.
        let heading = dereth_animation::frame::get_heading(&c.position().frame) + 90.0;
        c.face_heading(heading);
        c.camera.orbit.follow_behind = true;
        frames(&mut c, 6);
        c.camera.orbit.follow_behind = false;
        frames(&mut c, 300);
        assert!(
            wrap(c.camera.orbit.yaw - yaw_of_heading(heading)).abs() < 0.01,
            "directly behind them: {} against {}",
            c.camera.orbit.yaw,
            yaw_of_heading(heading)
        );
    }

    /// A body running at 144 frames a second, its physics ticking at its own 30 Hz: where it is
    /// drawn each frame, and how far it is drawn ahead of the point the orbit camera shows itself
    /// looking at.
    fn running_frames(between_ticks: bool) -> Vec<(f32, f32, f32)> {
        let store = std::sync::Arc::new(dereth_dat::testing::open_store().expect("retail dats"));
        let region = dereth_world_data::landblock::load_region(&store).expect("the region");
        let mut c = crate::character::Character::new(
            &store,
            &region,
            dereth_world_data::landblock::DEFAULT_LANDBLOCK,
            (96.0, 96.0),
        )
        .expect("a body");
        c.camera.orbit_active = true;
        c.drawn_between_ticks = between_ticks;
        let fps = 144.0;
        let mut t = 10.0;
        let mut frame = |c: &mut crate::character::Character| {
            t += 1.0 / fps;
            c.update(dereth_primitives::LocalTime(t));
            c.update_camera(
                crate::camera::CameraInput::default(),
                dereth_primitives::LocalTime(t),
                1.0 / fps,
            );
            let o = c.position().frame.origin;
            let off = c.drawn_offset();
            let drawn = Vec3::new(o.x + off.x, o.y + off.y, o.z + off.z);
            let shown = c.camera.orbit.shown_pivot(OrbitCamera::pivot(drawn));
            // The parts are placed where the body is drawn.
            let block = c.position().cell.landblock();
            let space = c.set_viewer_block((i32::from(block.x()), i32::from(block.y())));
            c.place_parts(space);
            let part = c.driver().part_array.parts[0].pos.origin;
            (drawn, shown, part)
        };
        for _ in 0..60 {
            frame(&mut c);
        }
        c.input.forward = true;
        c.input.run = true;
        // Up to speed first.
        for _ in 0..200 {
            frame(&mut c);
        }
        let flat = |a: Vec3, b: Vec3| ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
        let (mut last, _, mut last_part) = frame(&mut c);
        (0..60)
            .map(|_| {
                let (drawn, shown, part) = frame(&mut c);
                let step = flat(drawn, last);
                let part_step = flat(part, last_part);
                last = drawn;
                last_part = part;
                (step, flat(drawn, shown), part_step)
            })
            .collect()
    }

    /// One frame of a run under the orbit camera, in the scene's fixed space: where the body is
    /// drawn, where the eye stands, the point it is shown looking at, and whether the body was
    /// drawn between ticks this frame.
    #[derive(Debug, Clone, Copy)]
    struct RunFrame {
        body: Vec3,
        eye: Vec3,
        look: Vec3,
        between: bool,
    }

    /// `frames` frames of a body running at `fps`, the camera turning `turn` degrees a second and
    /// the body turned each frame to face where the camera looks, as camera-based movement turns
    /// it.
    fn turning_run(fps: f64, turn: f32, frames: usize) -> Vec<RunFrame> {
        let store = std::sync::Arc::new(dereth_dat::testing::open_store().expect("retail dats"));
        let region = dereth_world_data::landblock::load_region(&store).expect("the region");
        let mut c = crate::character::Character::new(
            &store,
            &region,
            dereth_world_data::landblock::DEFAULT_LANDBLOCK,
            (96.0, 96.0),
        )
        .expect("a body");
        c.camera.orbit_active = true;
        c.drawn_between_ticks = true;
        let block = c.position().cell.landblock();
        let block = (i32::from(block.x()), i32::from(block.y()));
        let mut t = 10.0;
        let mut frame = |c: &mut crate::character::Character, turning: bool| {
            t += 1.0 / fps;
            if turning {
                #[allow(clippy::cast_possible_truncation)]
                c.camera.orbit.turn_held(false, false, false, false, 0.0);
                #[allow(clippy::cast_possible_truncation)]
                let r = -(turn.to_radians()) * (1.0 / fps) as f32;
                c.camera.orbit.yaw = wrap(c.camera.orbit.yaw + r);
            }
            if c.camera.orbit.placed {
                c.face_heading(c.camera.orbit.movement_heading(false));
            }
            c.update(dereth_primitives::LocalTime(t));
            c.update_camera(
                crate::camera::CameraInput::default(),
                dereth_primitives::LocalTime(t),
                1.0 / fps,
            );
            let space = c.set_viewer_block(block);
            c.place_parts(space);
            let off = c.drawn_offset();
            let o = c.render_frame().origin;
            let body = Vec3::new(o.x + off.x, o.y + off.y, o.z + off.z);
            let eye = c.render_frame_of(c.camera.viewer).origin;
            let pivot = OrbitCamera::pivot(body);
            let shown = c.camera.orbit.shown_pivot(pivot);
            RunFrame {
                body,
                eye,
                look: shown,
                between: off != Vec3::ZERO,
            }
        };
        for _ in 0..60 {
            frame(&mut c, false);
        }
        c.input.forward = true;
        c.input.run = true;
        for _ in 0..300 {
            frame(&mut c, false);
        }
        (0..frames).map(|_| frame(&mut c, true)).collect()
    }

    /// Walking forward from a dungeon room's back wall, the eye is let back out from where the
    /// wall held it over several frames, across the line into the room behind. Every frame the
    /// cell the world is drawn from is the cell the eye stands in: drawn from any other cell, the
    /// rooms in front of the eye are seen from behind their openings and none of them is drawn.
    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
    fn walking_out_from_a_dungeon_wall_the_eye_is_drawn_from_the_cell_it_stands_in() {
        use dereth_primitives::{CellId, Frame, Position, Quat};
        let store = std::sync::Arc::new(dereth_dat::testing::open_store().expect("retail dats"));
        let region = dereth_world_data::landblock::load_region(&store).expect("the region");
        let mut c = crate::character::Character::new(&store, &region, 0x019E, (96.0, 96.0))
            .expect("a body");
        c.teleport(Position::new(
            CellId(0x019E_0114),
            Frame::new(
                Vec3::new(8.812_276, -42.201_912, 0.005),
                Quat::new(0.999_46, 0.0, 0.0, -0.032_867),
            ),
        ));
        c.camera.orbit_active = true;
        let fps = 144.0;
        let mut t = 10.0;
        let mut frame = |c: &mut crate::character::Character| {
            t += 1.0 / fps;
            c.update(dereth_primitives::LocalTime(t));
            c.update_camera(
                crate::camera::CameraInput::default(),
                dereth_primitives::LocalTime(t),
                1.0 / fps,
            );
        };
        for _ in 0..60 {
            frame(&mut c);
        }
        c.input.forward = true;
        let mut cells = std::collections::BTreeSet::new();
        for n in 0..400 {
            frame(&mut c);
            let drawn_from = c.camera.viewer_cell.expect("a cell to draw from");
            let stands_in = c
                .world
                .adjust_position(&c.camera.viewer, Vec3::ZERO)
                .and_then(|(_, cell)| cell)
                .expect("the eye stands in a cell");
            assert_eq!(
                drawn_from, stands_in,
                "frame {n}: drawn from {drawn_from:?} with the eye at {:?} in {stands_in:?}",
                c.camera.viewer
            );
            cells.insert(drawn_from.0);
        }
        assert!(
            cells.len() > 1,
            "the eye crossed from one cell into another: {cells:X?}"
        );
    }

    fn sub(a: Vec3, b: Vec3) -> Vec3 {
        Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
    }

    /// The share of frames whose step is far from the mean step (under half, or over one and a
    /// half times), and the largest step against the mean.
    fn unevenness(points: &[Vec3]) -> (f32, f32) {
        let steps: Vec<f32> = points
            .windows(2)
            .map(|w| sub(w[1], w[0]).magnitude())
            .collect();
        #[allow(clippy::cast_precision_loss)]
        let mean = steps.iter().sum::<f32>() / steps.len() as f32;
        #[allow(clippy::cast_precision_loss)]
        let odd = steps
            .iter()
            .filter(|s| **s < mean * 0.5 || **s > mean * 1.5)
            .count() as f32
            / steps.len() as f32;
        (odd, steps.iter().copied().fold(0.0, f32::max) / mean)
    }

    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
    fn a_body_running_straight_or_as_the_camera_turns_is_drawn_and_followed_evenly_every_frame() {
        for (fps, turn) in [(200.0, 60.0), (144.0, 60.0), (200.0, 0.0)] {
            let f = turning_run(fps, turn, 200);
            assert!(
                f.iter().all(|r| r.between),
                "turned each frame, it is still drawn on its way between ticks ({fps} fps)"
            );
            let body: Vec<Vec3> = f.iter().map(|r| r.body).collect();
            let eye: Vec<Vec3> = f.iter().map(|r| r.eye).collect();
            assert!(
                f.iter()
                    .all(|r| sub(OrbitCamera::pivot(r.body), r.look).magnitude() < 1e-4),
                "it looks at the body where it is drawn"
            );
            let (odd, largest) = unevenness(&body);
            assert!(
                odd == 0.0 && largest < 1.6,
                "the body's steps even ({fps} fps): {odd} {largest}"
            );
            let (odd, largest) = unevenness(&eye);
            assert!(
                odd < 0.05 && largest < 1.6,
                "the eye's steps even ({fps} fps): {odd} {largest}"
            );
        }
    }

    /// A body running and turned by the game's own turning key under character-based movement,
    /// at 144 frames a second: each frame, how far the camera has come round, the way the body is
    /// drawn facing against the way the camera looks, and where the eye stands.
    fn turned_by_the_game(frames: usize) -> Vec<(f32, f32, Vec3)> {
        let store = std::sync::Arc::new(dereth_dat::testing::open_store().expect("retail dats"));
        let region = dereth_world_data::landblock::load_region(&store).expect("the region");
        let mut c = crate::character::Character::new(
            &store,
            &region,
            dereth_world_data::landblock::DEFAULT_LANDBLOCK,
            (96.0, 96.0),
        )
        .expect("a body");
        c.camera.orbit_active = true;
        c.drawn_between_ticks = true;
        let block = c.position().cell.landblock();
        let block = (i32::from(block.x()), i32::from(block.y()));
        let fps = 144.0;
        let mut t = 10.0;
        let mut frame = |c: &mut crate::character::Character| {
            t += 1.0 / fps;
            c.update(dereth_primitives::LocalTime(t));
            c.update_camera(
                crate::camera::CameraInput::default(),
                dereth_primitives::LocalTime(t),
                1.0 / fps,
            );
            let space = c.set_viewer_block(block);
            c.place_parts(space);
            let drawn = dereth_animation::frame::get_heading(&c.position().frame) + c.drawn_turn();
            let facing = c.camera.orbit.facing_degrees();
            let against = (drawn - facing + 540.0).rem_euclid(360.0) - 180.0;
            (facing, against, c.render_frame_of(c.camera.viewer).origin)
        };
        for _ in 0..60 {
            frame(&mut c);
        }
        c.input.forward = true;
        c.input.run = true;
        for _ in 0..200 {
            frame(&mut c);
        }
        c.input.turn_left = true;
        c.camera.orbit_turns_with_player = true;
        for _ in 0..100 {
            frame(&mut c);
        }
        (0..frames).map(|_| frame(&mut c)).collect()
    }

    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
    fn a_body_the_turning_keys_turn_is_drawn_turning_and_followed_evenly_every_frame() {
        let f = turned_by_the_game(200);
        let turns: Vec<f32> = f
            .windows(2)
            .map(|w| ((w[1].0 - w[0].0 + 540.0).rem_euclid(360.0) - 180.0).abs())
            .collect();
        #[allow(clippy::cast_precision_loss)]
        let mean = turns.iter().sum::<f32>() / turns.len() as f32;
        assert!(mean > 0.1, "it turns: {mean} degrees a frame");
        assert!(
            turns.iter().all(|s| *s > mean * 0.5 && *s < mean * 1.6),
            "the camera comes round a little every frame, not a tick's turn at once: {turns:?}"
        );
        let wobble = f
            .windows(2)
            .map(|w| (w[1].1 - w[0].1).abs())
            .fold(0.0, f32::max);
        assert!(
            wobble < 0.01,
            "the body keeps its way before the camera from frame to frame: {wobble}"
        );
        let eye: Vec<Vec3> = f.iter().map(|r| r.2).collect();
        let (odd, largest) = unevenness(&eye);
        assert!(
            odd < 0.05 && largest < 1.6,
            "the eye's steps even: {odd} {largest}"
        );
    }

    #[test]
    fn a_camera_held_short_comes_in_at_once_and_goes_back_out_gently() {
        assert!(
            (eased_shortfall(0.0, 0.4, 0.005) - 0.4).abs() < 1e-6,
            "in at once"
        );
        let out = eased_shortfall(0.4, 0.0, 0.005);
        assert!(out > 0.38 && out < 0.4, "out a little in a frame: {out}");
        let mut s = 0.4;
        for _ in 0..200 {
            s = eased_shortfall(s, 0.0, 0.005);
        }
        assert!(s < 0.01, "out in about a second: {s}");
    }

    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
    fn a_running_body_under_the_orbit_camera_is_drawn_moving_every_frame_and_steady_before_it() {
        let frames = running_frames(true);
        let steps: Vec<f32> = frames.iter().map(|(s, _, _)| *s).collect();
        let mean = steps.iter().sum::<f32>() / steps.len() as f32;
        assert!(mean > 0.005, "it runs: {mean}");
        // Every frame it moves: never still and never more than one tick's catch-up at once (the
        // run's own steps come in whole animation frames, so now and then a tick carries two).
        assert!(
            steps.iter().all(|s| *s > mean * 0.5 && *s < mean * 2.5),
            "a step every frame: {steps:?}"
        );
        let wobble = frames
            .windows(2)
            .map(|w| (w[1].1 - w[0].1).abs())
            .fold(0.0, f32::max);
        assert!(
            wobble < 0.001,
            "it stands still before the camera from frame to frame: {wobble}"
        );
        // Its parts go with it, every frame.
        assert!(frames.iter().all(|(_, _, p)| *p > mean * 0.5), "{frames:?}");
        // Drawn where physics has it, as the game's own camera draws it, it stands still between
        // ticks and jumps on them.
        let ticked_only = running_frames(false);
        assert!(ticked_only.iter().filter(|(_, _, p)| *p == 0.0).count() > 30);
    }

    #[test]
    fn placed_behind_a_player_it_looks_the_way_they_face_and_stands_behind_and_above() {
        let mut c = OrbitCamera::default();
        // Facing east.
        c.place_behind(90.0);
        let eye = c.eye(Vec3::new(0.0, 0.0, 1.5));
        assert!(eye.origin.x < -1.0, "behind, to the west: {:?}", eye.origin);
        assert!(eye.origin.z > 1.5, "above the pivot");
        assert!(near(c.movement_heading(false), 90.0));
        assert!(near(c.movement_heading(true), 270.0));
    }

    #[test]
    fn it_faces_the_way_the_player_moves_and_round_at_them_in_the_front_view() {
        let mut c = OrbitCamera::default();
        c.place_behind(90.0);
        assert!(
            near(c.facing_degrees(), 90.0),
            "set behind, it faces as the player does"
        );
        c.ease(Vec3::ZERO, 1.0 / 60.0);
        assert!(near(c.facing_degrees(), 90.0), "and as shown");
        c.front_view = true;
        for _ in 0..240 {
            c.ease(Vec3::ZERO, 1.0 / 60.0);
        }
        assert!(
            near(c.facing_degrees(), 270.0),
            "turned round to the player's front"
        );
    }

    #[test]
    fn its_turn_speeds_tilt_limits_and_height_come_from_its_settings() {
        // Twice the pointer speed turns twice as far for the same movement.
        let turned = |mouse_turn: f32| {
            let mut c = OrbitCamera::default();
            c.apply_settings(OrbitSettings {
                mouse_turn,
                ..OrbitSettings::default()
            });
            c.place_behind(0.0);
            c.rotate(100.0, 0.0);
            c.movement_heading(false)
        };
        assert!((turned(0.004) * 2.0 - turned(0.008)).abs() < 0.01);
        // The look keys at their own speed.
        let mut c = OrbitCamera::default();
        c.apply_settings(OrbitSettings {
            key_turn: 3.0,
            ..OrbitSettings::default()
        });
        c.place_behind(0.0);
        c.turn_held(false, true, false, false, 0.1);
        assert!(near(c.movement_heading(false), 0.3_f32.to_degrees()));
        // Tilted as far as the limits allow, and brought inside new ones at once.
        let mut c = OrbitCamera::default();
        c.apply_settings(OrbitSettings {
            pitch_min: -0.5,
            pitch_max: 0.2,
            ..OrbitSettings::default()
        });
        c.rotate(0.0, -10_000.0);
        assert!((c.pitch - 0.2).abs() < 1e-6, "no higher than the top limit");
        c.rotate(0.0, 10_000.0);
        assert!(
            (c.pitch + 0.5).abs() < 1e-6,
            "no lower than the bottom limit"
        );
        c.apply_settings(OrbitSettings {
            pitch_min: -0.2,
            ..OrbitSettings::default()
        });
        assert!((c.pitch + 0.2).abs() < 1e-6, "new limits bring it in");
        // The point it looks at, raised by the height.
        let feet = Vec3::new(10.0, 20.0, 30.0);
        let mut c = OrbitCamera::default();
        assert_eq!(c.raised_pivot(feet), OrbitCamera::pivot(feet));
        c.apply_settings(OrbitSettings {
            height: 0.75,
            ..OrbitSettings::default()
        });
        assert!((c.raised_pivot(feet).z - (30.0 + PIVOT_HEIGHT + 0.75)).abs() < 1e-5);
    }

    #[test]
    fn the_pointer_turns_it_and_each_axis_can_be_reversed() {
        let mut c = OrbitCamera::default();
        c.place_behind(0.0);
        c.rotate(100.0, 0.0);
        // Moving right looks right: the heading grows clockwise.
        assert!(c.movement_heading(false) > 0.0 && c.movement_heading(false) < 90.0);
        let pitch = c.pitch;
        c.rotate(0.0, -50.0);
        assert!(c.pitch > pitch, "moving up looks up");
        let mut r = OrbitCamera {
            settings: OrbitSettings {
                reverse_x: true,
                reverse_y: true,
                ..OrbitSettings::default()
            },
            ..OrbitCamera::default()
        };
        r.place_behind(0.0);
        r.rotate(100.0, -50.0);
        assert!(
            r.movement_heading(false) > 270.0,
            "reversed: moving right looks left"
        );
        assert!(r.pitch < DEFAULT_PITCH, "reversed: moving up looks down");
    }

    #[test]
    fn the_wheel_brings_it_closer_within_its_limits() {
        let mut c = OrbitCamera::default();
        c.zoom(2.0);
        assert!(near(c.distance, DEFAULT_DISTANCE - 2.0));
        c.zoom(100.0);
        assert!(near(c.distance, MIN_DISTANCE));
        assert!(near(MIN_DISTANCE, 1.0));
        c.zoom(-100.0);
        assert!(near(c.distance, MAX_DISTANCE));
        assert!(near(MAX_DISTANCE, 15.0));
    }

    #[test]
    fn a_zoom_begun_steps_a_notch_at_once_and_goes_on_only_while_held() {
        let mut c = OrbitCamera::default();
        c.start_zoom(true);
        assert!(
            near(c.distance, DEFAULT_DISTANCE - ZOOM_STEP),
            "a notch at once"
        );
        c.step_zoom(0.1);
        assert!(
            near(c.distance, DEFAULT_DISTANCE - ZOOM_STEP),
            "not yet held"
        );
        c.step_zoom(0.5);
        assert!(
            c.distance < DEFAULT_DISTANCE - ZOOM_STEP,
            "held, it goes on"
        );
        let at = c.distance;
        c.stop_zoom();
        c.step_zoom(1.0);
        assert!(near(c.distance, at), "let go, it stops");
        c.start_zoom(false);
        assert!(near(c.distance, at + ZOOM_STEP));
    }

    #[test]
    fn the_front_view_turns_the_camera_round_but_not_the_way_the_player_moves() {
        let mut c = OrbitCamera::default();
        c.place_behind(0.0);
        let behind = c.eye(Vec3::ZERO);
        c.front_view = true;
        let front = c.eye(Vec3::ZERO);
        assert!(behind.origin.y < 0.0 && front.origin.y > 0.0);
        assert!(near(c.movement_heading(false), 0.0));
    }

    use dereth_client_contract::actions::{Action, ActionId};

    fn camera(sidestep: bool) -> OrbitSettings {
        OrbitSettings {
            sidestep,
            ..OrbitSettings::default()
        }
    }

    fn ids(out: &[Action]) -> Vec<(ActionId, bool)> {
        out.iter().map(|e| (e.id, e.is_start())).collect()
    }

    #[test]
    fn running_the_way_the_keys_point_any_key_runs_forward_until_the_last_is_let_go() {
        let mut k = MovementKeys::default();
        let s = camera(false);
        assert_eq!(
            ids(&k.convert(Action::begin(a::TURN_LEFT), s)),
            [(a::MOVE_FORWARD, true)]
        );
        assert!(
            k.convert(Action::begin(a::MOVE_BACKWARD), s).is_empty(),
            "already running"
        );
        assert!(
            k.convert(Action::end(a::TURN_LEFT), s).is_empty(),
            "Back still held"
        );
        assert_eq!(
            ids(&k.convert(Action::end(a::MOVE_BACKWARD), s)),
            [(a::MOVE_FORWARD, false)]
        );
    }

    #[test]
    fn running_the_way_the_keys_point_the_player_faces_their_direction_against_the_camera() {
        let h = |forward, back, left, right| HeldMovement {
            forward,
            back,
            left,
            right,
            mouse: false,
        };
        // A camera looking east.
        assert!(near(
            h(true, false, false, false).heading(90.0, false).unwrap(),
            90.0
        ));
        assert!(near(
            h(false, true, false, false).heading(90.0, false).unwrap(),
            270.0
        ));
        assert!(near(
            h(false, false, true, false).heading(90.0, false).unwrap(),
            0.0
        ));
        assert!(near(
            h(false, false, false, true).heading(90.0, false).unwrap(),
            180.0
        ));
        assert!(near(
            h(true, false, true, false).heading(90.0, false).unwrap(),
            45.0
        ));
        assert!(near(
            h(false, true, false, true).heading(90.0, false).unwrap(),
            225.0
        ));
        assert_eq!(h(false, false, false, false).heading(90.0, false), None);
        // Stepping sideways: the camera's way, or away from it on Back alone.
        assert!(near(
            h(false, false, true, false).heading(90.0, true).unwrap(),
            90.0
        ));
        assert!(near(
            h(false, true, true, false).heading(90.0, true).unwrap(),
            270.0
        ));
    }

    #[test]
    fn stepping_sideways_facing_the_camera_left_and_right_stay_across_its_view() {
        let mut k = MovementKeys::default();
        let s = camera(true);
        assert_eq!(
            ids(&k.convert(Action::begin(a::TURN_LEFT), s)),
            [(a::STRAFE_LEFT, true)]
        );
        // Back: running forward, facing the camera, so the camera's left is the player's right.
        assert_eq!(
            ids(&k.convert(Action::begin(a::MOVE_BACKWARD), s)),
            [
                (a::MOVE_FORWARD, true),
                (a::STRAFE_LEFT, false),
                (a::STRAFE_RIGHT, true)
            ]
        );
        assert_eq!(
            ids(&k.convert(Action::end(a::TURN_LEFT), s)),
            [(a::STRAFE_RIGHT, false)]
        );
        assert_eq!(
            ids(&k.convert(Action::begin(a::TURN_RIGHT), s)),
            [(a::STRAFE_LEFT, true)],
            "the camera's right, facing it"
        );
    }

    #[test]
    fn under_character_based_movement_the_keys_are_the_games_own_but_still_noted() {
        let mut k = MovementKeys::default();
        let s = OrbitSettings {
            movement: MovementMode::Character,
            ..OrbitSettings::default()
        };
        assert_eq!(
            ids(&k.convert(Action::begin(a::MOVE_BACKWARD), s)),
            [(a::MOVE_BACKWARD, true)]
        );
        assert_eq!(
            ids(&k.convert(Action::begin(a::TURN_LEFT), s)),
            [(a::TURN_LEFT, true)]
        );
        assert!(k.held.left && k.held.back);
    }

    #[test]
    fn under_character_based_movement_the_turning_keys_step_sideways_while_the_mouse_steers() {
        let mut k = MovementKeys::default();
        let s = OrbitSettings {
            movement: MovementMode::Character,
            ..OrbitSettings::default()
        };
        // The right button held: the mouse steers, and a turning key steps that way instead.
        k.buttons.1 = true;
        assert!(k.steering_changed(s).is_empty(), "nothing held to change");
        assert_eq!(
            ids(&k.convert(Action::begin(a::TURN_LEFT), s)),
            [(a::STRAFE_LEFT, true)]
        );
        assert_eq!(
            ids(&k.convert(Action::repeat(a::TURN_LEFT, 1), s)),
            [(a::STRAFE_LEFT, true)]
        );
        // Let go, the key goes on as a turn; pressed again, a step again.
        k.buttons.1 = false;
        assert_eq!(
            ids(&k.steering_changed(s)),
            [(a::STRAFE_LEFT, false), (a::TURN_LEFT, true)]
        );
        k.buttons.1 = true;
        assert_eq!(
            ids(&k.steering_changed(s)),
            [(a::TURN_LEFT, false), (a::STRAFE_LEFT, true)]
        );
        assert_eq!(
            ids(&k.convert(Action::end(a::TURN_LEFT), s)),
            [(a::STRAFE_LEFT, false)]
        );
        // Both buttons running steer too.
        k.buttons = (false, false);
        k.held.mouse = true;
        assert_eq!(
            ids(&k.convert(Action::begin(a::TURN_RIGHT), s)),
            [(a::STRAFE_RIGHT, true)]
        );
        // Camera-based movement has its own sideways keys: a button changes nothing held.
        k.held.mouse = false;
        assert!(k.steering_changed(camera(false)).is_empty());
    }

    #[test]
    fn while_a_spell_is_cast_the_keys_back_up_and_step_and_the_facing_is_held_in_either_movement() {
        let character = OrbitSettings {
            movement: MovementMode::Character,
            ..OrbitSettings::default()
        };
        for s in [camera(false), camera(true), character] {
            let mut k = MovementKeys::default();
            assert!(k.set_casting(true, s).is_empty(), "{s:?}: nothing held");
            assert!(k.locked());
            // S and A held through the cast: backing up and stepping left, nothing turned.
            assert_eq!(
                ids(&k.convert(Action::begin(a::MOVE_BACKWARD), s)),
                [(a::MOVE_BACKWARD, true)],
                "{s:?}"
            );
            assert_eq!(
                ids(&k.convert(Action::begin(a::TURN_LEFT), s)),
                [(a::STRAFE_LEFT, true)],
                "{s:?}"
            );
            assert_eq!(k.facing(s, 90.0), None, "{s:?}: the facing is held");
            assert!(!k.camera_turns_with_player(s), "{s:?}");
            assert!(
                k.camera_follows_game_turn(k.facing(s, 90.0), true),
                "{s:?}: the camera comes behind as the game turns the player to the target"
            );
            // The cast ends with the keys still held: they go on as they were.
            assert!(k.set_casting(false, s).is_empty(), "{s:?}");
            assert!(k.locked(), "{s:?}: held until the keys are let go");
            assert_eq!(k.facing(s, 90.0), None, "{s:?}");
            assert_eq!(
                ids(&k.convert(Action::end(a::TURN_LEFT), s)),
                [(a::STRAFE_LEFT, false)],
                "{s:?}"
            );
            assert_eq!(
                ids(&k.convert(Action::end(a::MOVE_BACKWARD), s)),
                [(a::MOVE_BACKWARD, false)],
                "{s:?}"
            );
            assert!(!k.locked(), "{s:?}: let go");
        }
        // S alone backs up rather than turning round to run.
        let mut k = MovementKeys::default();
        let _ = k.set_casting(true, camera(false));
        assert_eq!(
            ids(&k.convert(Action::begin(a::MOVE_BACKWARD), camera(false))),
            [(a::MOVE_BACKWARD, true)]
        );
        assert_eq!(k.facing(camera(false), 90.0), None);
        // A cast begun running forward walks on ahead without a stop.
        let mut k = MovementKeys::default();
        assert_eq!(
            ids(&k.convert(Action::begin(a::MOVE_FORWARD), camera(false))),
            [(a::MOVE_FORWARD, true)]
        );
        assert!(k.set_casting(true, camera(false)).is_empty());
        assert_eq!(
            ids(&k.convert(Action::end(a::MOVE_FORWARD), camera(false))),
            [(a::MOVE_FORWARD, false)]
        );
        // Outside a cast, A runs (camera-based) or turns (character-based) as before.
        let mut k = MovementKeys::default();
        assert_eq!(
            ids(&k.convert(Action::begin(a::TURN_LEFT), camera(false))),
            [(a::MOVE_FORWARD, true)]
        );
        assert!(k.facing(camera(false), 90.0).is_some());
        assert!(MovementKeys::default().camera_follows_game_turn(None, true));
        let mut k = MovementKeys::default();
        assert_eq!(
            ids(&k.convert(Action::begin(a::TURN_LEFT), character)),
            [(a::TURN_LEFT, true)]
        );
        assert!(k.camera_turns_with_player(character));
    }

    /// A body backing up and stepping left at 144 frames a second while a spell is cast, under
    /// `s`, with the keys read as the runtime reads them: its heading and the camera's turn at the
    /// start and the end, and how far it went back and left against the way it faces.
    fn backing_left_while_casting(s: OrbitSettings) -> (f32, f32, f32, f32, f32, f32) {
        let store = std::sync::Arc::new(dereth_dat::testing::open_store().expect("retail dats"));
        let region = dereth_world_data::landblock::load_region(&store).expect("the region");
        let mut c = crate::character::Character::new(
            &store,
            &region,
            dereth_world_data::landblock::DEFAULT_LANDBLOCK,
            (96.0, 96.0),
        )
        .expect("a body");
        c.camera.orbit_active = true;
        c.drawn_between_ticks = true;
        let fps = 144.0;
        let mut t = 10.0;
        let mut keys = MovementKeys::default();
        let mut frame = |c: &mut crate::character::Character, keys: &MovementKeys| {
            t += 1.0 / fps;
            if c.camera.orbit.placed {
                let camera = c.camera.orbit.movement_heading(false);
                if let Some(face) = keys.facing(s, camera) {
                    c.face_heading(face);
                }
            }
            c.camera.orbit_turns_with_player = keys.camera_turns_with_player(s);
            c.update(dereth_primitives::LocalTime(t));
            c.update_camera(
                crate::camera::CameraInput::default(),
                dereth_primitives::LocalTime(t),
                1.0 / fps,
            );
        };
        for _ in 0..60 {
            frame(&mut c, &keys);
        }
        // The camera looks off to one side of the way the player faces.
        c.camera.orbit.yaw = wrap(c.camera.orbit.yaw + 1.0);
        for _ in 0..30 {
            frame(&mut c, &keys);
        }
        let start = c.position().frame.origin;
        let heading = dereth_animation::frame::get_heading(&c.position().frame);
        let yaw = c.camera.orbit.yaw;
        let mut asked = Vec::new();
        asked.extend(keys.set_casting(true, s));
        asked.extend(keys.convert(Action::begin(a::MOVE_BACKWARD), s));
        asked.extend(keys.convert(Action::begin(a::TURN_LEFT), s));
        for e in &asked {
            let on = e.is_start();
            match e.id {
                a::MOVE_FORWARD => c.input.forward = on,
                a::MOVE_BACKWARD => c.input.back = on,
                a::STRAFE_LEFT => c.input.step_left = on,
                a::STRAFE_RIGHT => c.input.step_right = on,
                a::TURN_LEFT => c.input.turn_left = on,
                a::TURN_RIGHT => c.input.turn_right = on,
                _ => {}
            }
        }
        for _ in 0..200 {
            frame(&mut c, &keys);
        }
        let end = c.position().frame.origin;
        let h = heading.to_radians();
        let (ahead, right) = (
            Vec3::new(
                dereth_primitives::num::math::sinf(h),
                dereth_primitives::num::math::cosf(h),
                0.0,
            ),
            Vec3::new(
                dereth_primitives::num::math::cosf(h),
                -dereth_primitives::num::math::sinf(h),
                0.0,
            ),
        );
        let moved = sub(end, start);
        (
            heading,
            dereth_animation::frame::get_heading(&c.position().frame),
            yaw,
            c.camera.orbit.yaw,
            -moved.dot(ahead),
            -moved.dot(right),
        )
    }

    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
    fn backing_up_and_stepping_left_while_a_spell_is_cast_keeps_the_facing_and_the_camera() {
        let character = OrbitSettings {
            movement: MovementMode::Character,
            ..OrbitSettings::default()
        };
        for s in [camera(false), character] {
            let (h0, h1, y0, y1, back, left) = backing_left_while_casting(s);
            assert!(
                ((h1 - h0 + 540.0).rem_euclid(360.0) - 180.0).abs() < 0.01,
                "{:?}: the facing is held: {h0} to {h1}",
                s.movement
            );
            assert!(
                wrap(y1 - y0).abs() < 1e-4,
                "{:?}: the camera is held: {y0} to {y1}",
                s.movement
            );
            assert!(
                back > 0.5 && left > 0.5,
                "{:?}: it went back and left: {back} back, {left} left",
                s.movement
            );
        }
    }

    #[test]
    fn under_character_based_movement_the_camera_turns_with_the_player_unless_a_button_is_held() {
        let mut k = MovementKeys::default();
        let s = OrbitSettings {
            movement: MovementMode::Character,
            ..OrbitSettings::default()
        };
        let _ = k.convert(Action::begin(a::TURN_LEFT), s);
        assert!(
            k.camera_turns_with_player(s),
            "a turning key carries it round"
        );
        k.buttons.0 = true;
        assert!(
            !k.camera_turns_with_player(s),
            "the left button holds it where the pointer put it"
        );
        k.buttons = (false, true);
        assert!(
            !k.camera_turns_with_player(s),
            "the right button steers: the keys step sideways"
        );
        k.buttons = (false, false);
        assert!(
            !k.camera_turns_with_player(camera(false)),
            "camera-based movement turns the player to the camera instead"
        );
        let _ = k.convert(Action::end(a::TURN_LEFT), s);
        assert!(!k.camera_turns_with_player(s), "nothing held");
    }

    #[test]
    fn the_run_walk_key_flips_at_each_press_and_its_release_and_repeats_do_nothing() {
        let mut k = MovementKeys::default();
        let s = camera(false);
        assert_eq!(
            ids(&k.convert(Action::begin(a::TOGGLE_RUN_WALK), s)),
            [(a::TOGGLE_RUN_WALK, true)]
        );
        assert!(k
            .convert(Action::repeat(a::TOGGLE_RUN_WALK, 1), s)
            .is_empty());
        assert!(k.convert(Action::end(a::TOGGLE_RUN_WALK), s).is_empty());
        assert!(k.walking);
        assert_eq!(
            ids(&k.convert(Action::begin(a::TOGGLE_RUN_WALK), s)),
            [(a::TOGGLE_RUN_WALK, false)]
        );
        assert!(!k.walking);
    }

    #[test]
    fn both_mouse_buttons_run_forward_while_held_and_do_not_stop_a_held_forward() {
        let mut k = MovementKeys::default();
        let s = camera(false);
        assert_eq!(ids(&k.mouse_run(true, s)), [(a::MOVE_FORWARD, true)]);
        assert!(k.held.heading(0.0, false).is_some_and(|h| near(h, 0.0)));
        assert_eq!(ids(&k.mouse_run(false, s)), [(a::MOVE_FORWARD, false)]);
        let c = OrbitSettings {
            movement: MovementMode::Character,
            ..OrbitSettings::default()
        };
        let _ = k.convert(Action::begin(a::MOVE_FORWARD), c);
        assert_eq!(ids(&k.mouse_run(true, c)), [(a::MOVE_FORWARD, true)]);
        assert!(k.mouse_run(false, c).is_empty(), "Forward still runs");
    }

    #[test]
    fn what_is_shown_eases_toward_where_it_is_set_at_the_same_pace_at_any_frame_rate() {
        let pivot = Vec3::new(10.0, 10.0, 1.5);
        let run = |steps: u32| {
            let mut c = OrbitCamera::default();
            c.place_behind(0.0);
            c.ease(pivot, 0.0);
            c.rotate(200.0, 0.0);
            c.zoom(-5.0);
            #[allow(clippy::cast_precision_loss)]
            let dt = 0.1 / steps as f32;
            for _ in 0..steps {
                c.ease(pivot, dt);
            }
            c
        };
        let (slow, fast) = (run(6), run(24));
        let (a, b) = (slow.shown.unwrap(), fast.shown.unwrap());
        assert!(
            near(a.yaw, b.yaw) && near(a.distance, b.distance),
            "{a:?} {b:?}"
        );
        assert!(a.yaw != slow.yaw, "partway there after a tenth of a second");
        assert!(a.distance > DEFAULT_DISTANCE && a.distance < slow.distance);
        // A jump of the point it looks at (a portal) is taken at once.
        let mut c = slow;
        c.ease(Vec3::new(100.0, 10.0, 1.5), 0.01);
        assert!(near(c.shown_pivot(Vec3::ZERO).x, 100.0));
    }

    #[test]
    fn it_comes_round_behind_a_player_the_game_turns_gently_at_any_frame_rate() {
        let behind = |fps: u32, seconds: f32, start: f32, heading: f32| {
            let mut c = OrbitCamera {
                yaw: start,
                ..OrbitCamera::default()
            };
            #[allow(clippy::cast_precision_loss)]
            let dt = 1.0 / fps as f32;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            for _ in 0..(seconds * fps as f32).round() as u32 {
                c.ease_behind(heading, dt);
            }
            c.yaw
        };
        // A quarter turn: part of the way in half a second, the same at 60 and 240 a second.
        let (slow, fast) = (behind(60, 0.5, 0.0, 90.0), behind(240, 0.5, 0.0, 90.0));
        assert!((slow - fast).abs() < 1e-3, "{slow} {fast}");
        let target = -std::f32::consts::FRAC_PI_2;
        assert!(slow < 0.0 && slow > target, "partway: {slow}");
        assert!(
            (behind(60, 4.0, 0.0, 90.0) - target).abs() < 1e-3,
            "behind in the end"
        );
        // Across south, the short way round rather than all the way back.
        let start = std::f32::consts::PI - 0.1;
        let after = behind(60, 0.1, start, 185.0);
        assert!(wrap(after - start).abs() < 0.2, "{start} to {after}");
    }

    #[test]
    fn it_follows_only_a_turn_the_game_makes_and_never_against_the_mouse_or_steering() {
        assert!(follows_behind(false, false, true));
        assert!(!follows_behind(false, true, true), "a mouse button held");
        assert!(!follows_behind(true, false, true), "the player steering");
        assert!(!follows_behind(false, false, false), "nothing turning them");
    }

    #[test]
    fn after_the_player_turns_it_goes_on_round_until_directly_behind_and_stops_for_a_hand() {
        // Set off to one side, then turned with the player: once the turning stops it still goes
        // on round until it is directly behind, at the same pace at any frame rate.
        let settle = |fps: u32, seconds: f32| {
            let mut c = OrbitCamera::default();
            c.place_behind(0.0);
            c.rotate(-200.0, 0.0);
            c.turn_with_player(30.0);
            #[allow(clippy::cast_precision_loss)]
            let dt = 1.0 / fps as f32;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            for _ in 0..(seconds * fps as f32).round() as u32 {
                c.settle_behind(30.0, dt);
            }
            c
        };
        let behind = yaw_of_heading(30.0);
        let (slow, fast) = (settle(60, 0.3), settle(240, 0.3));
        assert!(
            (slow.yaw - fast.yaw).abs() < 1e-3,
            "{} {}",
            slow.yaw,
            fast.yaw
        );
        assert!(
            slow.settling_behind && wrap(behind - slow.yaw).abs() > SETTLED,
            "on its way"
        );
        let done = settle(60, 5.0);
        assert!(!done.settling_behind, "settled");
        assert!(near(done.yaw, behind), "directly behind: {}", done.yaw);
        // Turned by hand meanwhile, it stays where the hand put it.
        let mut c = OrbitCamera::default();
        c.place_behind(0.0);
        c.turn_with_player(30.0);
        c.rotate(50.0, 0.0);
        let there = c.yaw;
        c.settle_behind(30.0, 1.0);
        assert!(near(c.yaw, there), "the hand wins");
        // So do the look keys.
        let mut c = OrbitCamera::default();
        c.turn_with_player(30.0);
        c.turn_held(true, false, false, false, 0.1);
        assert!(!c.settling_behind);
    }

    #[test]
    fn turned_with_the_player_it_keeps_its_place_behind_them_without_lagging() {
        let mut c = OrbitCamera::default();
        c.place_behind(0.0);
        c.ease(Vec3::ZERO, 0.0);
        c.turn_with_player(90.0);
        assert!(near(c.movement_heading(false), 90.0));
        assert!(near(c.shown.unwrap().yaw, c.yaw), "shown turned at once");
    }
}
