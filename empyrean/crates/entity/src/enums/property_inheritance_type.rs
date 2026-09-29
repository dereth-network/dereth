// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PropertyInheritanceType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PropertyInheritanceType.cs`; do not edit by hand

/// ACE enum `PropertyInheritanceType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PropertyInheritanceType(pub i32);

#[allow(non_upper_case_globals)]
impl PropertyInheritanceType {
    pub const ClassOnly: Self = Self(0);
    pub const InstanceOnly: Self = Self(1);
    pub const Either: Self = Self(2);
}

impl PropertyInheritanceType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::ClassOnly, Self::InstanceOnly, Self::Either];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["ClassOnly", "InstanceOnly", "Either"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[0, 2, 1];
}

super::support::ace_enum!(PropertyInheritanceType, i32, plain);
super::support::ace_enum_from!(PropertyInheritanceType, i32 => i64);
