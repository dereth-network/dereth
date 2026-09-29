// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/FactionBits.cs
// @generated from ACE's `Source/ACE.Entity/Enum/FactionBits.cs`; do not edit by hand

/// ACE enum `FactionBits` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct FactionBits(pub i32);

#[allow(non_upper_case_globals)]
impl FactionBits {
    pub const None: Self = Self(0x0);
    pub const CelestialHand: Self = Self(0x1);
    pub const EldrytchWeb: Self = Self(0x2);
    pub const RadiantBlood: Self = Self(0x4);
    pub const ValidFactions: Self = Self(0x7);
}

impl FactionBits {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::CelestialHand, Self::EldrytchWeb, Self::RadiantBlood, Self::ValidFactions];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "CelestialHand", "EldrytchWeb", "RadiantBlood", "ValidFactions"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 0, 3, 4];
}

super::support::ace_enum!(FactionBits, i32, flags);
super::support::ace_enum_from!(FactionBits, i32 => i64);
