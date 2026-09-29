// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/ObjectGuid.cs
//! Object identity. Every world-object reference in the server is an [`ObjectGuid`].

use std::fmt;

/// ACE: GuidType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GuidType {
    #[default]
    Undef,
    Player,
    Static,
    Dynamic,
}

/// ACE: ObjectGuid. Equality and hashing are on [`ObjectGuid::full`] only, as in ACE; the
/// type is derived from the value, so deriving them on both fields is equivalent.
// ACE: ObjectGuid.op_Equality, ObjectGuid.op_Inequality, ObjectGuid.Equals, ObjectGuid.GetHashCode
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct ObjectGuid {
    full: u32,
}

impl ObjectGuid {
    // ACE: ObjectGuid.Invalid
    pub const INVALID: ObjectGuid = ObjectGuid { full: 0 };

    // ACE: ObjectGuid.PlayerMin
    pub const PLAYER_MIN: u32 = 0x5000_0001;
    // ACE: ObjectGuid.PlayerMax
    pub const PLAYER_MAX: u32 = 0x5FFF_FFFF;
    // ACE: ObjectGuid.StaticObjectMin
    pub const STATIC_OBJECT_MIN: u32 = 0x7000_0000;
    // ACE: ObjectGuid.StaticObjectMax
    pub const STATIC_OBJECT_MAX: u32 = 0x7FFF_FFFF;
    // ACE: ObjectGuid.DynamicMin
    pub const DYNAMIC_MIN: u32 = 0x8000_0000;
    /// Ends at `E` because `uint.MaxValue` is reserved for "invalid".
    // ACE: ObjectGuid.DynamicMax
    pub const DYNAMIC_MAX: u32 = 0xFFFF_FFFE;

    // ACE: ObjectGuid.ObjectGuid
    pub const fn new(full: u32) -> Self {
        ObjectGuid { full }
    }

    // ACE: ObjectGuid.IsPlayer
    pub const fn is_player_guid(guid: u32) -> bool {
        guid >= Self::PLAYER_MIN && guid <= Self::PLAYER_MAX
    }

    // ACE: ObjectGuid.IsStatic
    pub const fn is_static_guid(guid: u32) -> bool {
        guid >= Self::STATIC_OBJECT_MIN && guid <= Self::STATIC_OBJECT_MAX
    }

    // ACE: ObjectGuid.IsDynamic
    pub const fn is_dynamic_guid(guid: u32) -> bool {
        guid >= Self::DYNAMIC_MIN && guid <= Self::DYNAMIC_MAX
    }

    // ACE: ObjectGuid.Full
    pub const fn full(self) -> u32 {
        self.full
    }

    // ACE: ObjectGuid.Low
    pub const fn low(self) -> u32 {
        self.full & 0xFF_FFFF
    }

    // ACE: ObjectGuid.High
    pub const fn high(self) -> u32 {
        self.full >> 24
    }

    // ACE: ObjectGuid.Type
    pub const fn guid_type(self) -> GuidType {
        if Self::is_player_guid(self.full) {
            GuidType::Player
        } else if Self::is_static_guid(self.full) {
            GuidType::Static
        } else if Self::is_dynamic_guid(self.full) {
            GuidType::Dynamic
        } else {
            GuidType::Undef
        }
    }

    // ACE: ObjectGuid.IsPlayer
    pub const fn is_player(self) -> bool {
        matches!(self.guid_type(), GuidType::Player)
    }

    // ACE: ObjectGuid.IsStatic
    pub const fn is_static(self) -> bool {
        matches!(self.guid_type(), GuidType::Static)
    }

    // ACE: ObjectGuid.IsDynamic
    pub const fn is_dynamic(self) -> bool {
        matches!(self.guid_type(), GuidType::Dynamic)
    }
}

impl From<u32> for ObjectGuid {
    fn from(full: u32) -> Self {
        ObjectGuid::new(full)
    }
}

/// `ToString()` is the full value as eight upper-case hex digits.
// ACE: ObjectGuid.ToString
impl fmt::Display for ObjectGuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:08X}", self.full)
    }
}

impl fmt::Debug for ObjectGuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ObjectGuid({:08X})", self.full)
    }
}
