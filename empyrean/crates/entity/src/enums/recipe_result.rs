// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/RecipeResult.cs
// @generated from ACE's `Source/ACE.Entity/Enum/RecipeResult.cs`; do not edit by hand

/// ACE enum `RecipeResult` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct RecipeResult(pub i32);

#[allow(non_upper_case_globals)]
impl RecipeResult {
    pub const SourceItemDestroyed: Self = Self(0x1);
    pub const TargetItemDestroyed: Self = Self(0x2);
    pub const SourceItemUsesDecrement: Self = Self(0x4);
    pub const TargetItemUsesDecrement: Self = Self(0x8);
    pub const SuccessItem1: Self = Self(0x10);
    pub const SuccessItem2: Self = Self(0x20);
    pub const FailureItem1: Self = Self(0x40);
    pub const FailureItem2: Self = Self(0x80);
}

impl RecipeResult {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::SourceItemDestroyed, Self::TargetItemDestroyed, Self::SourceItemUsesDecrement, Self::TargetItemUsesDecrement, Self::SuccessItem1, Self::SuccessItem2, Self::FailureItem1, Self::FailureItem2];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["SourceItemDestroyed", "TargetItemDestroyed", "SourceItemUsesDecrement", "TargetItemUsesDecrement", "SuccessItem1", "SuccessItem2", "FailureItem1", "FailureItem2"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[6, 7, 0, 2, 4, 5, 1, 3];
}

super::support::ace_enum!(RecipeResult, i32, flags);
super::support::ace_enum_from!(RecipeResult, i32 => i64);
