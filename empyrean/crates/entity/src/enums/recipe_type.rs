// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/RecipeType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/RecipeType.cs`; do not edit by hand

/// these are not from the client, but rather a classification of how a "Use A on B" formula is intended to work. this logic drives the basic flow of how these interations work
///
/// ACE enum `RecipeType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct RecipeType(pub i32);

#[allow(non_upper_case_globals)]
impl RecipeType {
    pub const None: Self = Self(0);
    pub const CreateItem: Self = Self(1);
    pub const Healing: Self = Self(2);
    pub const Tinkering: Self = Self(3);
    pub const Dyeing: Self = Self(4);
    pub const Unlocking: Self = Self(5);
    pub const ManaStone: Self = Self(6);
}

impl RecipeType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::CreateItem, Self::Healing, Self::Tinkering, Self::Dyeing, Self::Unlocking, Self::ManaStone];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "CreateItem", "Healing", "Tinkering", "Dyeing", "Unlocking", "ManaStone"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 4, 2, 6, 0, 3, 5];
}

super::support::ace_enum!(RecipeType, i32, plain);
super::support::ace_enum_from!(RecipeType, i32 => i64);
