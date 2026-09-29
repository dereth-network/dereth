// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/SkillAdvancementClass.cs
// @generated from ACE's `Source/ACE.Entity/Enum/SkillAdvancementClass.cs`; do not edit by hand

/// ACE enum `SkillAdvancementClass`, underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SkillAdvancementClass(pub u32);

#[allow(non_upper_case_globals)]
impl SkillAdvancementClass {
    pub const Inactive: Self = Self(0);
    pub const Untrained: Self = Self(1);
    pub const Trained: Self = Self(2);
    pub const Specialized: Self = Self(3);
}

impl SkillAdvancementClass {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Inactive, Self::Untrained, Self::Trained, Self::Specialized];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Inactive", "Untrained", "Trained", "Specialized"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[0, 3, 2, 1];
}

super::support::ace_enum!(SkillAdvancementClass, u32, plain);
super::support::ace_enum_from!(SkillAdvancementClass, u32 => u64, i64);
