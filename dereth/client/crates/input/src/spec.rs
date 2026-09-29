//! The control model and the name tables.
//!
//! `ControlCode`, `ControlChord`, `DeviceType`, `SubControlIndex`, the activation
//! numbering, and `ControlNames`.

use crate::names;

/// A single physical control, packed into a `u32` exactly as the client packs it.
///
/// MSVC bitfields, LSB first: bits 0–7 the device index, bits 8–15 the sub-control, bits 16–31
/// the key offset.
/// `0xFFFFFFFF` is the invalid sentinel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ControlCode(pub u32);

impl ControlCode {
    /// The sentinel the client uses for "no control".
    pub const INVALID: Self = Self(0xFFFF_FFFF);

    #[must_use]
    pub const fn new(device: u8, sub: SubControlIndex, offset: u16) -> Self {
        Self((device as u32) | ((sub.raw() as u32) << 8) | ((offset as u32) << 16))
    }

    #[must_use]
    pub const fn device_index(self) -> u8 {
        // The low byte is a field of the packed key, not a lossy narrowing.
        #[allow(clippy::cast_possible_truncation)]
        {
            self.0 as u8
        }
    }

    #[must_use]
    pub const fn sub_control(self) -> SubControlIndex {
        SubControlIndex::from_raw(((self.0 >> 8) & 0xFF) as u8)
    }

    #[must_use]
    pub const fn offset(self) -> u16 {
        // Bits 16..31 are the key offset; the mask makes the narrowing exact.
        #[allow(clippy::cast_possible_truncation)]
        {
            (self.0 >> 16) as u16
        }
    }
}

/// `DeviceType` — writes these four names into the `.keymap` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DeviceType {
    Keyboard = 1,
    Mouse = 2,
    Joystick = 3,
    Virtual = 4,
}

impl DeviceType {
    #[must_use]
    pub const fn from_raw(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Keyboard),
            2 => Some(Self::Mouse),
            3 => Some(Self::Joystick),
            4 => Some(Self::Virtual),
            _ => None,
        }
    }

    /// The device type's name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Keyboard => "Keyboard",
            Self::Mouse => "Mouse",
            Self::Joystick => "Joystick",
            Self::Virtual => "Virtual",
        }
    }

    /// The device type a name selects.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "Keyboard" => Some(Self::Keyboard),
            "Mouse" => Some(Self::Mouse),
            "Joystick" => Some(Self::Joystick),
            "Virtual" => Some(Self::Virtual),
            _ => None,
        }
    }
}

/// `SubControlIndex` — the axis/POV selector in bits 8–15.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SubControlIndex {
    None = 0,
    PositiveAxis = 1,
    NegativeAxis = 2,
    PovUp = 3,
    PovRight = 4,
    PovDown = 5,
    PovLeft = 6,
    /// Anything the client would not recognise. Carried so a malformed keymap round-trips rather
    /// than panicking; returns false for it.
    Other(u8),
}

impl SubControlIndex {
    #[must_use]
    pub const fn from_raw(v: u8) -> Self {
        match v {
            0 => Self::None,
            1 => Self::PositiveAxis,
            2 => Self::NegativeAxis,
            3 => Self::PovUp,
            4 => Self::PovRight,
            5 => Self::PovDown,
            6 => Self::PovLeft,
            other => Self::Other(other),
        }
    }

    #[must_use]
    pub const fn raw(self) -> u8 {
        match self {
            Self::None => 0,
            Self::PositiveAxis => 1,
            Self::NegativeAxis => 2,
            Self::PovUp => 3,
            Self::PovRight => 4,
            Self::PovDown => 5,
            Self::PovLeft => 6,
            Self::Other(v) => v,
        }
    }

    /// `None` has no name — the `.keymap` writer simply
    /// omits the third node.
    #[must_use]
    pub const fn name(self) -> Option<&'static str> {
        match self {
            Self::None | Self::Other(_) => None,
            Self::PositiveAxis => Some("AxisPositive"),
            Self::NegativeAxis => Some("AxisNegative"),
            Self::PovUp => Some("POVUp"),
            Self::PovRight => Some("POVRight"),
            Self::PovDown => Some("POVDown"),
            Self::PovLeft => Some("POVLeft"),
        }
    }

    /// The sub-control index a name selects.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "AxisPositive" => Some(Self::PositiveAxis),
            "AxisNegative" => Some(Self::NegativeAxis),
            "POVUp" => Some(Self::PovUp),
            "POVRight" => Some(Self::PovRight),
            "POVDown" => Some(Self::PovDown),
            "POVLeft" => Some(Self::PovLeft),
            _ => None,
        }
    }
}

/// The activation bits.
///
/// **Their numeric order decides which binding wins**, because
/// compares activations numerically. Renumbering them
/// changes which binding wins.
pub mod activation {
    pub const DOWN: u32 = 0x01;
    pub const UP: u32 = 0x02;
    /// `Down | Up` — the value in every retail binding.
    pub const CLICK: u32 = 0x03;
    pub const TAP: u32 = 0x04;
    pub const DBL_CLICK_DOWN: u32 = 0x08;
    pub const DBL_CLICK_UP: u32 = 0x10;
    pub const DBL_CLICK: u32 = 0x18;
    pub const NEARBY_DOWN: u32 = 0x20;
    pub const NEARBY_UP: u32 = 0x40;
    pub const MOUSE_DBL_CLICK: u32 = 0x60;
    pub const ANALOG: u32 = 0x80;
    /// `Analog | NearbyDown | DblClickDown | Down` — the "down-ish" test `fire_action_event` and
    /// `fire_input_event` both use.
    pub const DOWNISH_MASK: u32 = 0xA9;
    /// Set **only** on a live event, never on a stored binding. This is the bit that makes
    /// `operator==` asymmetric.
    pub const LIVE: u32 = 0x8000_0000;

    /// in the order it tests.
    pub const NAMES: &[(u32, &str)] = &[
        (DOWN, "Down"),
        (UP, "Up"),
        (CLICK, "Click"),
        (TAP, "Tap"),
        (DBL_CLICK_DOWN, "DblClickDown"),
        (DBL_CLICK_UP, "DblClickUp"),
        (DBL_CLICK, "DblClick"),
        (NEARBY_DOWN, "NearbyDown"),
        (NEARBY_UP, "NearbyUp"),
        (MOUSE_DBL_CLICK, "MouseDblClick"),
        (ANALOG, "Analog"),
    ];

    /// The activation type's name.
    #[must_use]
    pub fn to_name(v: u32) -> Option<&'static str> {
        NAMES.iter().find(|(k, _)| *k == v).map(|(_, n)| *n)
    }

    /// The activation type a name selects.
    #[must_use]
    pub fn from_name(s: &str) -> Option<u32> {
        NAMES.iter().find(|(_, n)| *n == s).map(|(k, _)| *k)
    }
}

/// A control plus the context it was pressed in — 12 bytes in the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ControlChord {
    pub control: ControlCode,
    /// The modifier bitmask in effect. Which bit means what is **data**, not a constant: it comes
    /// from the master input map's meta-key list.
    pub meta_mode: u32,
    pub activation: u32,
}

impl ControlChord {
    #[must_use]
    pub const fn new(control: ControlCode, meta_mode: u32, activation: u32) -> Self {
        Self {
            control,
            meta_mode,
            activation,
        }
    }

    /// Qualified-control matching treats `self` as the **live event** (bit 31 set),
    /// `binding` is a **stored binding** (bit 31 clear). The comparison is deliberately asymmetric.
    #[must_use]
    pub fn matches(&self, binding: &Self) -> bool {
        self.control == binding.control
            && (self.meta_mode & binding.meta_mode) == binding.meta_mode
            && (self.activation & binding.activation) != 0
    }

    /// Exact equality of two qualified controls.
    #[must_use]
    pub fn is_exactly_equal(&self, other: &Self) -> bool {
        self == other
    }

    /// Same key, same metamode, overlapping
    /// activation. This is what the key-binding options page uses to detect clashes.
    #[must_use]
    pub fn is_conflicting(&self, other: &Self) -> bool {
        self.control == other.control
            && self.meta_mode == other.meta_mode
            && (self.activation & other.activation) != 0
    }

    /// **higher activation wins; on a tie, the
    /// binding matching more modifier bits wins**. `self` is the live event.
    ///
    /// That is how `Ctrl+Left` beats plain `Left` and `MouseDblClick` (0x60) beats `Click` (0x03).
    /// Note the consequence: `Analog` (0x80) would beat a double-click binding on the same control.
    #[must_use]
    pub fn is_better_match(&self, candidate: &Self, best: &Self) -> bool {
        if candidate.activation != best.activation {
            return candidate.activation > best.activation;
        }
        // The client counts the high bits of the shared meta-mode mask.
        (self.meta_mode & candidate.meta_mode).count_ones()
            > (self.meta_mode & best.meta_mode).count_ones()
    }
}

/// `ControlNames` -- the four `DIK_*`/`DIMOFS_*`/`DIJOFS_*`/`DIV_*`
/// name tables the client's key-semantic load fills.
#[derive(Debug, Clone, Copy)]
pub struct ControlNames;

impl ControlNames {
    /// The table would have chosen, by the name's **third character**.
    #[must_use]
    pub fn table_for(device: DeviceType) -> &'static [(&'static str, u16)] {
        match device {
            DeviceType::Keyboard => names::KEYBOARD_NAMES,
            DeviceType::Mouse => names::MOUSE_NAMES,
            DeviceType::Joystick => names::JOYSTICK_NAMES,
            DeviceType::Virtual => names::VIRTUAL_NAMES,
        }
    }

    /// A name-keyed hash lookup across **all
    /// four** tables. The client keeps one hash per device class but the `.keymap` reader searches
    /// the one the device index selects; searching all four here is equivalent because no name is
    /// registered in two tables (the third character decides, and it is unique per prefix).
    #[must_use]
    pub fn semantic_by_name(name: &str) -> Option<(DeviceType, u16)> {
        for d in [
            DeviceType::Keyboard,
            DeviceType::Mouse,
            DeviceType::Joystick,
            DeviceType::Virtual,
        ] {
            if let Some((_, v)) = Self::table_for(d).iter().find(|(n, _)| *n == name) {
                return Some((d, *v));
            }
        }
        None
    }

    /// The name of a control, by semantic.
    ///
    /// The name choice for duplicate offsets remains unverified. The original **linearly scans the
    /// hash table** for the first entry whose
    /// value equals the offset, so when an offset has several names (16 keyboard offsets do —
    /// `DIK_BACK`/`DIK_BACKSPACE`, `DIK_NEXT`/`DIK_PGDN`, …) the winner is decided by hash-bucket
    /// order, which depends on the string hash and the table's growth history and was not
    /// recovered. This returns the **first registered** name, which is the primary spelling in
    /// every case in the client's key-semantic load order, and is what the shipped keymaps use.
    #[must_use]
    pub fn name_by_semantic(device: DeviceType, offset: u16) -> Option<&'static str> {
        Self::table_for(device)
            .iter()
            .find(|(_, v)| *v == offset)
            .map(|(n, _)| *n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered keymap format §2.1's worked example —
    /// `0x002A0000` is keyboard, button, `DIK_LSHIFT` (0x2A).
    #[test]
    fn packs_the_worked_example() {
        let cs = ControlCode(0x002A_0000);
        assert_eq!(cs.device_index(), 0);
        assert_eq!(cs.sub_control(), SubControlIndex::None);
        assert_eq!(cs.offset(), 0x2A);
        assert_eq!(cs, ControlCode::new(0, SubControlIndex::None, 0x2A));
        assert_eq!(
            ControlNames::name_by_semantic(DeviceType::Keyboard, 0x2A),
            Some("DIK_LSHIFT")
        );
    }

    /// Oracle: the client's key-semantic load, 248 registrations.
    /// The knowledge base said 174/12/88; retail has 153/12/82 plus one virtual.
    #[test]
    fn the_name_tables_are_the_binarys() {
        assert_eq!(names::KEYBOARD_NAMES.len(), 153);
        assert_eq!(names::MOUSE_NAMES.len(), 12);
        assert_eq!(names::JOYSTICK_NAMES.len(), 82);
        assert_eq!(names::VIRTUAL_NAMES.len(), 1);
        assert_eq!(names::VIRTUAL_NAMES[0], ("DIV_MOUSELOOK", 1));
        // The key-semantic registration picks by the third character, which is why DIMOFS_WHEEL is a mouse name.
        assert_eq!(
            ControlNames::semantic_by_name("DIMOFS_WHEEL"),
            Some((DeviceType::Mouse, 8))
        );
    }

    /// Oracle: the recovered input pipeline §1.2 — the operator== rule, event against binding.
    #[test]
    fn matching_is_asymmetric() {
        let ctrl = ControlCode::new(0, SubControlIndex::None, 0xCB); // DIK_LEFT
        let plain = ControlChord::new(ctrl, 0, activation::CLICK);
        let with_ctrl = ControlChord::new(ctrl, 0x4000_0000, activation::CLICK);

        // A Ctrl-held event matches both bindings ...
        let event = ControlChord::new(ctrl, 0x4000_0000, activation::DOWN | activation::LIVE);
        assert!(event.matches(&plain));
        assert!(event.matches(&with_ctrl));
        // .. and the modifier-bearing one wins on the tie-break.
        assert!(event.is_better_match(&with_ctrl, &plain));
        assert!(!event.is_better_match(&plain, &with_ctrl));

        // An unmodified event matches only the unmodified binding.
        let bare = ControlChord::new(ctrl, 0, activation::DOWN | activation::LIVE);
        assert!(bare.matches(&plain));
        assert!(!bare.matches(&with_ctrl));
    }

    /// Oracle: the recovered input pipeline §12 — higher activation wins outright, so MouseDblClick
    /// (0x60) beats Click (0x03) and Analog (0x80) would beat MouseDblClick.
    #[test]
    fn higher_activation_wins_outright() {
        let ctrl = ControlCode::new(1, SubControlIndex::None, 0x0C);
        let click = ControlChord::new(ctrl, 0, activation::CLICK);
        let dbl = ControlChord::new(ctrl, 0, activation::MOUSE_DBL_CLICK);
        let analog = ControlChord::new(ctrl, 0, activation::ANALOG);
        let ev = ControlChord::new(ctrl, 0, 0xFF | activation::LIVE);
        assert!(ev.is_better_match(&dbl, &click));
        assert!(ev.is_better_match(&analog, &dbl));
    }
}
