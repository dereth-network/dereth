// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Properties/PropertyInt64.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Properties/PropertyInt64.cs`; do not edit by hand

/// ACE enum `PropertyInt64`, underlying `ushort`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PropertyInt64(pub u16);

#[allow(non_upper_case_globals)]
impl PropertyInt64 {
    pub const Undef: Self = Self(0);
    pub const TotalExperience: Self = Self(1);
    pub const AvailableExperience: Self = Self(2);
    pub const AugmentationCost: Self = Self(3);
    pub const ItemTotalXp: Self = Self(4);
    pub const ItemBaseXp: Self = Self(5);
    pub const AvailableLuminance: Self = Self(6);
    pub const MaximumLuminance: Self = Self(7);
    pub const InteractionReqs: Self = Self(8);
    pub const AllegianceXPCached: Self = Self(9000);
    pub const AllegianceXPGenerated: Self = Self(9001);
    pub const AllegianceXPReceived: Self = Self(9002);
    pub const VerifyXp: Self = Self(9003);
}

impl PropertyInt64 {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::TotalExperience, Self::AvailableExperience, Self::AugmentationCost, Self::ItemTotalXp, Self::ItemBaseXp, Self::AvailableLuminance, Self::MaximumLuminance, Self::InteractionReqs, Self::AllegianceXPCached, Self::AllegianceXPGenerated, Self::AllegianceXPReceived, Self::VerifyXp];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "TotalExperience", "AvailableExperience", "AugmentationCost", "ItemTotalXp", "ItemBaseXp", "AvailableLuminance", "MaximumLuminance", "InteractionReqs", "AllegianceXPCached", "AllegianceXPGenerated", "AllegianceXPReceived", "VerifyXp"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[9, 10, 11, 3, 2, 6, 8, 5, 4, 7, 1, 0, 12];

    /// The members marked `[AssessmentProperty]`, in declaration order (ACE's reflection order).
    pub const ASSESSMENT_PROPERTY: &'static [Self] = &[Self::AugmentationCost, Self::ItemTotalXp, Self::ItemBaseXp];
    /// Whether this value is marked `[AssessmentProperty]` (by value, like ACE's `HashSet` lookups).
    pub fn is_assessment_property(self) -> bool {
        Self::ASSESSMENT_PROPERTY.contains(&self)
    }

    /// The members marked `[SendOnLogin]`, in declaration order (ACE's reflection order).
    pub const SEND_ON_LOGIN: &'static [Self] = &[Self::TotalExperience, Self::AvailableExperience, Self::AvailableLuminance, Self::MaximumLuminance];
    /// Whether this value is marked `[SendOnLogin]` (by value, like ACE's `HashSet` lookups).
    pub fn is_send_on_login(self) -> bool {
        Self::SEND_ON_LOGIN.contains(&self)
    }
}

super::support::ace_enum!(PropertyInt64, u16, plain);
super::support::ace_enum_from!(PropertyInt64, u16 => u32, u64, i32, i64);
