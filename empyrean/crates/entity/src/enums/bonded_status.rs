// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/BondedStatus.cs
// @generated from ACE's `Source/ACE.Entity/Enum/BondedStatus.cs`; do not edit by hand

/// ACE enum `BondedStatus`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct BondedStatus(pub i32);

#[allow(non_upper_case_globals)]
impl BondedStatus {
    pub const Destroy: Self = Self(-2);
    pub const Slippery: Self = Self(-1);
    pub const Normal: Self = Self(0);
    pub const Bonded: Self = Self(1);
    pub const Sticky: Self = Self(2);
}

impl BondedStatus {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Normal, Self::Bonded, Self::Sticky, Self::Destroy, Self::Slippery];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Normal", "Bonded", "Sticky", "Destroy", "Slippery"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 3, 0, 4, 2];
}

super::support::ace_enum!(BondedStatus, i32, plain);
super::support::ace_enum_from!(BondedStatus, i32 => i64);
