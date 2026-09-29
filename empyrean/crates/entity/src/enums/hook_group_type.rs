// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/HookGroupType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/HookGroupType.cs`; do not edit by hand

/// ACE enum `HookGroupType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct HookGroupType(pub i32);

#[allow(non_upper_case_globals)]
impl HookGroupType {
    pub const Undef: Self = Self(0);
    pub const NoisemakingItems: Self = Self(1);
    pub const TestItems: Self = Self(2);
    pub const PortalItems: Self = Self(4);
    pub const WritableItems: Self = Self(8);
    pub const SpellCastingItems: Self = Self(16);
    pub const SpellTeachingItems: Self = Self(32);
}

impl HookGroupType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::NoisemakingItems, Self::TestItems, Self::PortalItems, Self::WritableItems, Self::SpellCastingItems, Self::SpellTeachingItems];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "NoisemakingItems", "TestItems", "PortalItems", "WritableItems", "SpellCastingItems", "SpellTeachingItems"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 3, 5, 6, 2, 0, 4];
}

super::support::ace_enum!(HookGroupType, i32, plain);
super::support::ace_enum_from!(HookGroupType, i32 => i64);
