// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/GeneratorDestruct.cs
// @generated from ACE's `Source/ACE.Entity/Enum/GeneratorDestruct.cs`; do not edit by hand

/// ACE enum `GeneratorDestruct`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct GeneratorDestruct(pub i32);

#[allow(non_upper_case_globals)]
impl GeneratorDestruct {
    pub const Undef: Self = Self(0);
    pub const Nothing: Self = Self(1);
    pub const Destroy: Self = Self(2);
    pub const Kill: Self = Self(3);
}

impl GeneratorDestruct {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Nothing, Self::Destroy, Self::Kill];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Nothing", "Destroy", "Kill"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 3, 1, 0];
}

super::support::ace_enum!(GeneratorDestruct, i32, plain);
super::support::ace_enum_from!(GeneratorDestruct, i32 => i64);
