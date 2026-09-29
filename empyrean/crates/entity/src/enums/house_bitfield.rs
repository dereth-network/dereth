// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/HouseBitfield.cs
// @generated from ACE's `Source/ACE.Entity/Enum/HouseBitfield.cs`; do not edit by hand

/// ACE enum `HouseBitfield` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct HouseBitfield(pub i32);

#[allow(non_upper_case_globals)]
impl HouseBitfield {
    pub const Undef: Self = Self(0x0);
    pub const Active: Self = Self(0x1);
    pub const RequiresMonarch: Self = Self(0x2);
}

impl HouseBitfield {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Active, Self::RequiresMonarch];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Active", "RequiresMonarch"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 0];
}

super::support::ace_enum!(HouseBitfield, i32, flags);
super::support::ace_enum_from!(HouseBitfield, i32 => i64);
