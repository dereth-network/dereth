// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/RecipeSourceType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/RecipeSourceType.cs`; do not edit by hand

/// ACE enum `RecipeSourceType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct RecipeSourceType(pub i32);

#[allow(non_upper_case_globals)]
impl RecipeSourceType {
    pub const Player: Self = Self(0);
    pub const Source: Self = Self(1);
    pub const Dye: Self = Self(60);
}

impl RecipeSourceType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Player, Self::Source, Self::Dye];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Player", "Source", "Dye"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 0, 1];
}

super::support::ace_enum!(RecipeSourceType, i32, plain);
super::support::ace_enum_from!(RecipeSourceType, i32 => i64);
