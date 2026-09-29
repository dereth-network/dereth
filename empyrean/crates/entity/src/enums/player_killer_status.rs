// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PlayerKillerStatus.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PlayerKillerStatus.cs`; do not edit by hand

/// ACE enum `PlayerKillerStatus` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PlayerKillerStatus(pub u32);

#[allow(non_upper_case_globals)]
impl PlayerKillerStatus {
    pub const Undef: Self = Self(0x0);
    pub const Protected: Self = Self(0x1);
    pub const NPK: Self = Self(0x2);
    pub const PK: Self = Self(0x4);
    pub const Unprotected: Self = Self(0x8);
    pub const RubberGlue: Self = Self(0x10);
    pub const Free: Self = Self(0x20);
    pub const PKLite: Self = Self(0x40);
    pub const Creature: Self = Self(0x8);
    pub const Trap: Self = Self(0x8);
    pub const NPC: Self = Self(0x1);
    pub const Vendor: Self = Self(0x10);
    pub const Baelzharon: Self = Self(0x20);
}

impl PlayerKillerStatus {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Protected, Self::NPC, Self::NPK, Self::PK, Self::Unprotected, Self::Creature, Self::Trap, Self::RubberGlue, Self::Vendor, Self::Free, Self::Baelzharon, Self::PKLite];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Protected", "NPC", "NPK", "PK", "Unprotected", "Creature", "Trap", "RubberGlue", "Vendor", "Free", "Baelzharon", "PKLite"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[11, 6, 10, 2, 3, 4, 12, 1, 8, 7, 0, 5, 9];
}

super::support::ace_enum!(PlayerKillerStatus, u32, flags);
super::support::ace_enum_from!(PlayerKillerStatus, u32 => u64, i64);
