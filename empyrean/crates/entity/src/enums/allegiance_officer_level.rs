// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AllegianceOfficerLevel.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AllegianceOfficerLevel.cs`; do not edit by hand

/// ACE enum `AllegianceOfficerLevel`, underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AllegianceOfficerLevel(pub u32);

#[allow(non_upper_case_globals)]
impl AllegianceOfficerLevel {
    pub const Undef: Self = Self(0);
    pub const Speaker: Self = Self(1);
    pub const Seneschal: Self = Self(2);
    pub const Castellan: Self = Self(3);
}

impl AllegianceOfficerLevel {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Speaker, Self::Seneschal, Self::Castellan];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Speaker", "Seneschal", "Castellan"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 2, 1, 0];
}

super::support::ace_enum!(AllegianceOfficerLevel, u32, plain);
super::support::ace_enum_from!(AllegianceOfficerLevel, u32 => u64, i64);
