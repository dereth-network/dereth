//! The controls: the game's action map's actions, pressed and let go as a key would press them,
//! and what the controls in force ask of the player's body and camera each frame.
//!
//! A control is one of the action map's actions, by its id ([`ControlId`]). Whatever drives the
//! game presses one ([`Control`] with [`ControlPhase::Begin`]) and lets it go
//! ([`ControlPhase::End`]): a window's keys and buttons through its key maps, an interface asking
//! for a movement as a key would, a script, a headless driver or a plugin. The client carries the
//! controls in force as the action map says each toggles ([`toggle`]): a held one from press to
//! release, a latched one from one press to the next (autorun), a single press for its frame (a
//! jump). By the modifier controls in force (Shift, Ctrl, Alt) each control has its class
//! ([`class`]), its value and its extent; each frame the movement and camera controls in force sum
//! into a [`ControlFrame`].
//!
//! A press is known by what pressed it ([`ControlSource`]): its release from the same source ends
//! it, whatever else was pressed in between. A control pressed as itself, as a script presses it,
//! has the source [`ControlSource::direct`].

/// One of the action map's actions, by its id: what a key names through the key maps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ControlId(pub u32);

/// Whether a control is pressed or let go.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ControlPhase {
    /// Pressed: a key going down.
    Begin,
    /// Let go: a key coming up.
    End,
}

/// What pressed a control: a front end's number for where the press came from (a window's
/// keyboard, its mouse) and the input there (the key's or the button's word). A release from the
/// same source ends the press it began.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ControlSource {
    pub device: u32,
    pub input: u32,
}

impl ControlSource {
    /// The source a control pressed as itself has: from the interface (device 1), its input the
    /// control's id. An interface asking for a movement as a key would, a script, a headless
    /// driver and a plugin press controls so.
    #[must_use]
    pub const fn direct(id: ControlId) -> Self {
        Self {
            device: 1,
            input: id.0,
        }
    }
}

/// A control pressed or let go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Control {
    pub id: ControlId,
    pub phase: ControlPhase,
    /// How far, on a press: 1 for a key. Its whole part is what is kept; a press of 0 is a
    /// release, as the game takes a key's value of 0.
    pub extent: f32,
    pub source: ControlSource,
}

impl Control {
    /// The control pressed as itself, at full extent.
    #[must_use]
    pub const fn begin(id: ControlId) -> Self {
        Self {
            id,
            phase: ControlPhase::Begin,
            extent: 1.0,
            source: ControlSource::direct(id),
        }
    }

    /// The control let go as itself.
    #[must_use]
    pub const fn end(id: ControlId) -> Self {
        Self {
            id,
            phase: ControlPhase::End,
            extent: 0.0,
            source: ControlSource::direct(id),
        }
    }

    /// The control pressed as itself at `extent`, or let go when it is 0: as a key's value says.
    #[must_use]
    pub fn at(id: ControlId, extent: f32) -> Self {
        if extent == 0.0 {
            Self::end(id)
        } else {
            Self {
                extent,
                ..Self::begin(id)
            }
        }
    }

    /// The same press or release from another source.
    #[must_use]
    pub const fn from(mut self, source: ControlSource) -> Self {
        self.source = source;
        self
    }

    /// The value the client takes the control with: its extent pressed, 0 let go.
    #[must_use]
    pub const fn value(&self) -> f32 {
        match self.phase {
            ControlPhase::Begin => self.extent,
            ControlPhase::End => 0.0,
        }
    }
}

/// How a control toggles, as the action map says.
pub mod toggle {
    /// In force from press to release.
    pub const HELD: u32 = 0x4000_0001;
    /// In force from one press to the next.
    pub const LATCH: u32 = 0x4000_0002;
    /// In force for the frame of the press.
    pub const IMPULSE: u32 = 0x4000_0003;
}

/// What a control does, by its class.
pub mod class {
    pub const MOVEMENT: u32 = 0x4000_0001;
    pub const BEHAVIOR: u32 = 0x4000_0002;
    pub const CAMERA: u32 = 0x4000_0004;
    pub const MODIFIER: u32 = 0x4000_0005;
    pub const INTERFACE: u32 = 0x4000_0006;
}

/// A movement control's value: what it moves.
pub mod motion {
    pub const FORWARD: u32 = 0x4000_0001;
    pub const TURN_RIGHT: u32 = 0x4000_0003;
    pub const TURN_LEFT: u32 = 0x4000_0004;
    pub const STRAFE_RIGHT: u32 = 0x4000_0005;
    pub const STRAFE_LEFT: u32 = 0x4000_0006;
    pub const JUMP: u32 = 0x4000_0007;
}

/// The camera controls' values.
pub mod camera {
    pub const SWING_OUT_LEFT: u32 = 0x4000_0028;
    pub const SWING_OUT_RIGHT: u32 = 0x4000_0029;
    pub const SHOULDER: u32 = 0x4000_002A;
    pub const ROTATE_LEFT: u32 = 0x4000_002E;
    pub const ROTATE_RIGHT: u32 = 0x4000_002F;
    pub const RESET: u32 = 0x4000_0030;
    pub const LOOK_AT_GROUND: u32 = 0x4000_00AE;
    pub const LOOK_AT_SELF: u32 = 0x4000_00B1;
    pub const MAP_VIEW: u32 = 0x4000_00B3;
    pub const ZOOM_IN: u32 = 0x4000_00B4;
    pub const ZOOM_OUT: u32 = 0x4000_00B5;
    pub const TILT_UP: u32 = 0x4000_00B6;
    pub const TILT_DOWN: u32 = 0x4000_00B7;
    /// The interface's camera values the controls carry out when no interface took them.
    pub const FIRST_PERSON: u32 = 0x4000_00A2;
    pub const TOGGLE_MOUSE_LOOK: u32 = 0x4000_00BA;
    pub const INSTANT_ZOOM_IN: u32 = 0x4000_00B8;
    pub const INSTANT_ZOOM_OUT: u32 = 0x4000_00B9;
}

/// The interface's values the controls carry out when no interface took them: using, selecting
/// nothing, and the selection keys ([`action`](interface::action)).
pub mod interface {
    use crate::{Action, SelectKind};

    pub const ESC: u32 = 0x4000_001E;
    pub const USE: u32 = 0x4100_000D;
    pub const SELECT_OFF: u32 = 0x4100_0029;

    /// The selection keys' values.
    pub mod select {
        /// The player itself (or, already selected, the last selection).
        pub const SELF: u32 = 0x4100_0018;
        pub const PREVIOUS_CREATURE: u32 = 0x4100_0019;
        pub const NEXT_CREATURE: u32 = 0x4100_001A;
        pub const NEAREST_CREATURE: u32 = 0x4100_001B;
        pub const PREVIOUS_ITEM: u32 = 0x4100_001C;
        pub const NEXT_ITEM: u32 = 0x4100_001D;
        pub const NEAREST_ITEM: u32 = 0x4100_001E;
        pub const PREVIOUS_PLAYER: u32 = 0x4100_001F;
        pub const NEXT_PLAYER: u32 = 0x4100_0020;
        pub const NEAREST_PLAYER: u32 = 0x4100_0021;
        pub const PREVIOUS_FOE: u32 = 0x4100_0022;
        pub const NEXT_FOE: u32 = 0x4100_0023;
        pub const NEAREST_FOE: u32 = 0x4100_0024;
        pub const PREVIOUS_FELLOW: u32 = 0x4100_0025;
        pub const NEXT_FELLOW: u32 = 0x4100_0026;
        pub const NEAREST_FELLOW: u32 = 0x4100_0027;
        /// The last selection.
        pub const LAST: u32 = 0x4100_0028;
        /// The last to attack the player.
        pub const LAST_ATTACKER: u32 = 0x4100_002A;
        pub const PREVIOUS_PET: u32 = 0x4100_005C;
        pub const NEXT_PET: u32 = 0x4100_005D;
        pub const NEAREST_PET: u32 = 0x4100_005E;
    }

    /// What an interface value asks of the game: use what is selected, select nothing, or select
    /// as its selection key does; `None` for any other value.
    #[must_use]
    pub const fn action(value: u32) -> Option<Action> {
        use select as s;
        let (kind, step) = match value {
            USE => return Some(Action::UseSelected),
            ESC | SELECT_OFF => return Some(Action::Select { target: 0 }),
            s::SELF => return Some(Action::SelectSelf),
            s::LAST => return Some(Action::SelectLast),
            s::LAST_ATTACKER => return Some(Action::SelectLastAttacker),
            s::PREVIOUS_CREATURE..=s::NEAREST_FELLOW => {
                let kind = match (value - s::PREVIOUS_CREATURE) / 3 {
                    0 => SelectKind::Creature,
                    1 => SelectKind::Item,
                    2 => SelectKind::Player,
                    3 => SelectKind::Foe,
                    _ => SelectKind::Fellow,
                };
                (kind, (value - s::PREVIOUS_CREATURE) % 3)
            }
            s::PREVIOUS_PET..=s::NEAREST_PET => (SelectKind::Pet, value - s::PREVIOUS_PET),
            _ => return None,
        };
        Some(match step {
            0 => Action::SelectCycle {
                kind,
                previous: true,
            },
            1 => Action::SelectCycle {
                kind,
                previous: false,
            },
            _ => Action::SelectNearest { kind },
        })
    }
}

/// The controls that move the player.
pub mod movement {
    use super::ControlId;

    pub const FORWARD: ControlId = ControlId(0x4000_0020);
    pub const BACKWARD: ControlId = ControlId(0x4000_0021);
    pub const STRAFE_LEFT: ControlId = ControlId(0x4000_0022);
    pub const STRAFE_RIGHT: ControlId = ControlId(0x4000_0023);
    pub const TURN_LEFT: ControlId = ControlId(0x4000_0024);
    pub const TURN_RIGHT: ControlId = ControlId(0x4000_0025);
    pub const JUMP: ControlId = ControlId(0x4000_0026);
    /// Autorun: latched, it runs the player until pressed again.
    pub const RUN_LOCK: ControlId = ControlId(0x4000_0027);
    /// Stop: with forward and backward, pressed it ends a latched run.
    pub const STOP: ControlId = ControlId(0x4000_00A3);
}

/// The camera controls held this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CameraHeld {
    pub rotate_left: bool,
    pub rotate_right: bool,
    pub tilt_up: bool,
    pub tilt_down: bool,
    pub zoom_in: bool,
    pub zoom_out: bool,
    /// Shift: the faster rates.
    pub fast: bool,
}

/// A camera control pressed once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraPress {
    Reset,
    Shoulder,
    FirstPerson,
    TopDown,
    Reverse,
    SwingLeft,
    SwingRight,
    MapView,
    /// Mouse look switched on or off.
    MouseLook,
    /// Zoom steps: positive toward the character.
    Zoom(i32),
}

/// What the controls in force ask this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ControlFrame {
    /// Ahead positive, -1 to 1.
    pub forward: f32,
    /// Right positive, -1 to 1.
    pub sideways: f32,
    /// Left positive: -1, 0 or 1.
    pub turn: f32,
    /// Jump this frame.
    pub jump: bool,
    /// The camera controls held.
    pub camera: CameraHeld,
    /// The camera controls pressed since the last frame, in order.
    pub presses: Vec<CameraPress>,
    /// The movement controls begun since the last frame, in order.
    pub started: Vec<ControlId>,
    /// The behaviors the behavior controls began (`true`) or ended (`false`) since the last
    /// frame, by the behavior's value.
    pub behaviors: Vec<(u32, bool)>,
}
