// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ConfirmationType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ConfirmationType.cs`; do not edit by hand

/// ACE enum `ConfirmationType`, underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ConfirmationType(pub u32);

#[allow(non_upper_case_globals)]
impl ConfirmationType {
    pub const Undefined: Self = Self(0);
    pub const SwearAllegiance: Self = Self(1);
    pub const AlterSkill: Self = Self(2);
    pub const AlterAttribute: Self = Self(3);
    pub const Fellowship: Self = Self(4);
    pub const CraftInteraction: Self = Self(5);
    pub const Augmentation: Self = Self(6);
    pub const Yes_No: Self = Self(7);
}

impl ConfirmationType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undefined, Self::SwearAllegiance, Self::AlterSkill, Self::AlterAttribute, Self::Fellowship, Self::CraftInteraction, Self::Augmentation, Self::Yes_No];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undefined", "SwearAllegiance", "AlterSkill", "AlterAttribute", "Fellowship", "CraftInteraction", "Augmentation", "Yes_No"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 2, 6, 5, 4, 1, 0, 7];
}

super::support::ace_enum!(ConfirmationType, u32, plain);
super::support::ace_enum_from!(ConfirmationType, u32 => u64, i64);
