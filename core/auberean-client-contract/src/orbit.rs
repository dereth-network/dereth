//! The orbit camera an interface may choose in place of the game's own, and how the movement
//! controls move the player under it: the choice as plain settings.
//!
//! The orbit camera circles the player at a distance the wheel and the zoom controls set, turned
//! by the pointer while a mouse button is held and by the camera's rotate and tilt controls; held,
//! the look-at-front control turns it round to the player's front. Under it the movement controls
//! move the player one of two ways ([`MovementMode`]):
//! - **camera-based** (the default): the player runs the way the movement controls point against
//!   where the camera looks. Forward runs where it looks; backward turns the player round at once
//!   and runs toward the camera; the turns and the strafes run across its view, and two together
//!   run between.
//! - **character-based**: the game's own. Forward and backward move along the player's facing, the
//!   turns turn and the strafes step sideways; moving or turning brings the camera round behind the
//!   player. While the right mouse button is held the player faces where the camera looks and the
//!   turns step sideways.
//!
//! An interface without these settings shows the game's own camera, as the client always has.

/// How the movement controls move the player while the orbit camera is in use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MovementMode {
    /// Forward runs where the camera looks; backward turns the player round and runs toward the
    /// camera; the turns and the strafes run across its view.
    #[default]
    Camera,
    /// The game's own: forward and backward along the player's facing, the turns turn.
    Character,
}

impl MovementMode {
    /// Both, in the order an options page lists them.
    pub const ALL: [Self; 2] = [Self::Camera, Self::Character];

    /// Its value, as the client's preferences keep it.
    #[must_use]
    pub const fn value(self) -> i32 {
        match self {
            Self::Camera => 0,
            Self::Character => 1,
        }
    }

    /// The mode a value names; anything else is camera-based.
    #[must_use]
    pub const fn from_value(v: i32) -> Self {
        match v {
            1 => Self::Character,
            _ => Self::Camera,
        }
    }

    /// Its name, as the preferences file keeps it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Camera => "Camera",
            Self::Character => "Character",
        }
    }
}

/// Radians the orbit camera turns for one count of the mouse's motion, unless set otherwise.
pub const MOUSE_TURN: f32 = 0.005;
/// Radians a second the camera's rotate and tilt controls turn it, unless set otherwise.
pub const KEY_TURN: f32 = 1.6;

/// What an interface chose for the orbit camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitSettings {
    pub movement: MovementMode,
    /// Turn the camera the other way when the mouse moves sideways.
    pub reverse_x: bool,
    /// Tilt the camera the other way when the mouse moves up or down.
    pub reverse_y: bool,
    /// Radians the camera turns for one count of the mouse's motion.
    pub mouse_turn: f32,
    /// Radians a second the camera's rotate and tilt controls turn it.
    pub key_turn: f32,
}

impl Default for OrbitSettings {
    fn default() -> Self {
        Self {
            movement: MovementMode::default(),
            reverse_x: false,
            reverse_y: false,
            mouse_turn: MOUSE_TURN,
            key_turn: KEY_TURN,
        }
    }
}

/// The ranges the turning rates are held to, whatever an interface or a file asks.
pub mod limits {
    /// The mouse's turn, radians a count.
    pub const MOUSE_TURN: (f32, f32) = (0.001, 0.02);
    /// The controls' turn, radians a second.
    pub const KEY_TURN: (f32, f32) = (0.3, 6.0);
}

impl OrbitSettings {
    /// These settings with the turning rates held to their ranges: a rate out of range is brought
    /// into it, and one that is not a number is the default.
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
            ..self
        }
    }
}
