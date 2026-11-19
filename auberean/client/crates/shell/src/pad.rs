//! The host's pad as the shell reads it once a frame and hands it to the interface shown
//! ([`crate::interface::Game::pad`]): its buttons held, its two sticks and its two triggers, named
//! by where they sit on the pad rather than by what any one maker prints on them.
//!
//! The host fills a [`PadState`] from whatever pad it has ([`crate::Host::gamepad`]); a host with
//! no pad (the browser, a test) reads none. An interface turns it into the game's controls and
//! actions; nothing here decides what a button does, and no pad reaches the game itself.

/// A pad's button, by its place: the four face buttons by compass point, the shoulder buttons,
/// the two small buttons in the middle, the d-pad and the sticks pressed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PadButton {
    /// The bottom face button (A on an Xbox pad, Cross on a PlayStation one).
    South,
    /// The right face button (B, Circle).
    East,
    /// The left face button (X, Square).
    West,
    /// The top face button (Y, Triangle).
    North,
    LeftBumper,
    RightBumper,
    /// The left of the two middle buttons (Back, View, Select, Share).
    Back,
    /// The right of the two middle buttons (Start, Menu, Options).
    Start,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    LeftStick,
    RightStick,
}

impl PadButton {
    /// Every button, in the order of their bits in [`PadState::buttons`].
    pub const ALL: [Self; 14] = [
        Self::South,
        Self::East,
        Self::West,
        Self::North,
        Self::LeftBumper,
        Self::RightBumper,
        Self::Back,
        Self::Start,
        Self::DPadUp,
        Self::DPadDown,
        Self::DPadLeft,
        Self::DPadRight,
        Self::LeftStick,
        Self::RightStick,
    ];

    /// Its bit in [`PadState::buttons`].
    #[must_use]
    pub const fn bit(self) -> u16 {
        1 << (self as u16)
    }
}

/// One pad's state at one moment.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PadState {
    /// The buttons held, one bit each ([`PadButton::bit`]).
    pub buttons: u16,
    /// The left stick: right and up, each from -1 to 1.
    pub left: (f32, f32),
    /// The right stick: right and up, each from -1 to 1.
    pub right: (f32, f32),
    /// The triggers, each from 0 (let go) to 1 (pulled all the way).
    pub left_trigger: f32,
    pub right_trigger: f32,
}

impl PadState {
    /// Whether `button` is held.
    #[must_use]
    pub const fn held(&self, button: PadButton) -> bool {
        self.buttons & button.bit() != 0
    }

    /// Hold `button`, or let it go.
    pub fn set(&mut self, button: PadButton, held: bool) {
        if held {
            self.buttons |= button.bit();
        } else {
            self.buttons &= !button.bit();
        }
    }

    /// The state with `buttons` held and nothing else.
    #[must_use]
    pub fn holding(buttons: &[PadButton]) -> Self {
        let mut s = Self::default();
        for b in buttons {
            s.set(*b, true);
        }
        s
    }
}
