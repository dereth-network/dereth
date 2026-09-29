// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ContainerType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ContainerType.cs`; do not edit by hand

/// ACE enum `ContainerType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ContainerType(pub i32);

#[allow(non_upper_case_globals)]
impl ContainerType {
    pub const NonContainer: Self = Self(0);
    pub const Container: Self = Self(1);
    pub const Foci: Self = Self(2);
}

impl ContainerType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::NonContainer, Self::Container, Self::Foci];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["NonContainer", "Container", "Foci"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 0];
}

super::support::ace_enum!(ContainerType, i32, plain);
super::support::ace_enum_from!(ContainerType, i32 => i64);
