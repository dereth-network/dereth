// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/HARBitfield.cs
// @generated from ACE's `Source/ACE.Entity/Enum/HARBitfield.cs`; do not edit by hand

/// ACE enum `HARBitfield` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct HARBitfield(pub i32);

#[allow(non_upper_case_globals)]
impl HARBitfield {
    pub const Undef: Self = Self(0x0);
    pub const OpenHouse: Self = Self(0x1);
    pub const AllegianceGuests: Self = Self(0x2);
    pub const AllegianceStorage: Self = Self(0x4);
}

impl HARBitfield {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::OpenHouse, Self::AllegianceGuests, Self::AllegianceStorage];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "OpenHouse", "AllegianceGuests", "AllegianceStorage"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 3, 1, 0];
}

super::support::ace_enum!(HARBitfield, i32, flags);
super::support::ace_enum_from!(HARBitfield, i32 => i64);
