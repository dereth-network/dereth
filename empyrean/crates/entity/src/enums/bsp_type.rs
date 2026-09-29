// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/BSPType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/BSPType.cs`; do not edit by hand

/// ACE enum `BSPType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct BSPType(pub i32);

#[allow(non_upper_case_globals)]
impl BSPType {
    pub const Drawing: Self = Self(0);
    pub const Physics: Self = Self(1);
    pub const Cell: Self = Self(2);
}

impl BSPType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Drawing, Self::Physics, Self::Cell];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Drawing", "Physics", "Cell"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 0, 1];
}

super::support::ace_enum!(BSPType, i32, plain);
super::support::ace_enum_from!(BSPType, i32 => i64);
