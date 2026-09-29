// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/MapScope.cs
// @generated from ACE's `Source/ACE.Entity/Enum/MapScope.cs`; do not edit by hand

/// ACE enum `MapScope`, underlying `byte`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct MapScope(pub u8);

#[allow(non_upper_case_globals)]
impl MapScope {
    pub const Outdoors: Self = Self(0);
    pub const IndoorsSmall: Self = Self(1);
    pub const IndoorsLarge: Self = Self(2);
}

impl MapScope {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Outdoors, Self::IndoorsSmall, Self::IndoorsLarge];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Outdoors", "IndoorsSmall", "IndoorsLarge"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 1, 0];
}

super::support::ace_enum!(MapScope, u8, plain);
super::support::ace_enum_from!(MapScope, u8 => u16, u32, u64, i32, i64);
