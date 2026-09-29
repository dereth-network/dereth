// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Properties/PropertyDataId.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Properties/PropertyDataId.cs`; do not edit by hand

/// ACE enum `PropertyDataId`, underlying `ushort`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PropertyDataId(pub u16);

#[allow(non_upper_case_globals)]
impl PropertyDataId {
    pub const Undef: Self = Self(0);
    pub const Setup: Self = Self(1);
    pub const MotionTable: Self = Self(2);
    pub const SoundTable: Self = Self(3);
    pub const CombatTable: Self = Self(4);
    pub const QualityFilter: Self = Self(5);
    pub const PaletteBase: Self = Self(6);
    pub const ClothingBase: Self = Self(7);
    pub const Icon: Self = Self(8);
    pub const EyesTexture: Self = Self(9);
    pub const NoseTexture: Self = Self(10);
    pub const MouthTexture: Self = Self(11);
    pub const DefaultEyesTexture: Self = Self(12);
    pub const DefaultNoseTexture: Self = Self(13);
    pub const DefaultMouthTexture: Self = Self(14);
    pub const HairPalette: Self = Self(15);
    pub const EyesPalette: Self = Self(16);
    pub const SkinPalette: Self = Self(17);
    pub const HeadObject: Self = Self(18);
    pub const ActivationAnimation: Self = Self(19);
    pub const InitMotion: Self = Self(20);
    pub const ActivationSound: Self = Self(21);
    pub const PhysicsEffectTable: Self = Self(22);
    pub const UseSound: Self = Self(23);
    pub const UseTargetAnimation: Self = Self(24);
    pub const UseTargetSuccessAnimation: Self = Self(25);
    pub const UseTargetFailureAnimation: Self = Self(26);
    pub const UseUserAnimation: Self = Self(27);
    pub const Spell: Self = Self(28);
    pub const SpellComponent: Self = Self(29);
    pub const PhysicsScript: Self = Self(30);
    pub const LinkedPortalOne: Self = Self(31);
    pub const WieldedTreasureType: Self = Self(32);
    pub const InventoryTreasureType: Self = Self(33);
    pub const ShopTreasureType: Self = Self(34);
    pub const DeathTreasureType: Self = Self(35);
    pub const MutateFilter: Self = Self(36);
    pub const ItemSkillLimit: Self = Self(37);
    pub const UseCreateItem: Self = Self(38);
    pub const DeathSpell: Self = Self(39);
    pub const VendorsClassId: Self = Self(40);
    pub const ItemSpecializedOnly: Self = Self(41);
    pub const HouseId: Self = Self(42);
    pub const AccountHouseId: Self = Self(43);
    pub const RestrictionEffect: Self = Self(44);
    pub const CreationMutationFilter: Self = Self(45);
    pub const TsysMutationFilter: Self = Self(46);
    pub const LastPortal: Self = Self(47);
    pub const LinkedPortalTwo: Self = Self(48);
    pub const OriginalPortal: Self = Self(49);
    pub const IconOverlay: Self = Self(50);
    pub const IconOverlaySecondary: Self = Self(51);
    pub const IconUnderlay: Self = Self(52);
    pub const AugmentationMutationFilter: Self = Self(53);
    pub const AugmentationEffect: Self = Self(54);
    pub const ProcSpell: Self = Self(55);
    pub const AugmentationCreateItem: Self = Self(56);
    pub const AlternateCurrency: Self = Self(57);
    pub const BlueSurgeSpell: Self = Self(58);
    pub const YellowSurgeSpell: Self = Self(59);
    pub const RedSurgeSpell: Self = Self(60);
    pub const OlthoiDeathTreasureType: Self = Self(61);
    pub const PCAPRecordedWeenieHeader: Self = Self(8001);
    pub const PCAPRecordedWeenieHeader2: Self = Self(8002);
    pub const PCAPRecordedObjectDesc: Self = Self(8003);
    pub const PCAPRecordedPhysicsDesc: Self = Self(8005);
    pub const PCAPRecordedParentLocation: Self = Self(8009);
    pub const PCAPRecordedDefaultScript: Self = Self(8019);
    pub const PCAPRecordedTimestamp0: Self = Self(8020);
    pub const PCAPRecordedTimestamp1: Self = Self(8021);
    pub const PCAPRecordedTimestamp2: Self = Self(8022);
    pub const PCAPRecordedTimestamp3: Self = Self(8023);
    pub const PCAPRecordedTimestamp4: Self = Self(8024);
    pub const PCAPRecordedTimestamp5: Self = Self(8025);
    pub const PCAPRecordedTimestamp6: Self = Self(8026);
    pub const PCAPRecordedTimestamp7: Self = Self(8027);
    pub const PCAPRecordedTimestamp8: Self = Self(8028);
    pub const PCAPRecordedTimestamp9: Self = Self(8029);
    pub const PCAPRecordedMaxVelocityEstimated: Self = Self(8030);
    pub const PCAPPhysicsDIDDataTemplatedFrom: Self = Self(8044);
}

impl PropertyDataId {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Setup, Self::MotionTable, Self::SoundTable, Self::CombatTable, Self::QualityFilter, Self::PaletteBase, Self::ClothingBase, Self::Icon, Self::EyesTexture, Self::NoseTexture, Self::MouthTexture, Self::DefaultEyesTexture, Self::DefaultNoseTexture, Self::DefaultMouthTexture, Self::HairPalette, Self::EyesPalette, Self::SkinPalette, Self::HeadObject, Self::ActivationAnimation, Self::InitMotion, Self::ActivationSound, Self::PhysicsEffectTable, Self::UseSound, Self::UseTargetAnimation, Self::UseTargetSuccessAnimation, Self::UseTargetFailureAnimation, Self::UseUserAnimation, Self::Spell, Self::SpellComponent, Self::PhysicsScript, Self::LinkedPortalOne, Self::WieldedTreasureType, Self::InventoryTreasureType, Self::ShopTreasureType, Self::DeathTreasureType, Self::MutateFilter, Self::ItemSkillLimit, Self::UseCreateItem, Self::DeathSpell, Self::VendorsClassId, Self::ItemSpecializedOnly, Self::HouseId, Self::AccountHouseId, Self::RestrictionEffect, Self::CreationMutationFilter, Self::TsysMutationFilter, Self::LastPortal, Self::LinkedPortalTwo, Self::OriginalPortal, Self::IconOverlay, Self::IconOverlaySecondary, Self::IconUnderlay, Self::AugmentationMutationFilter, Self::AugmentationEffect, Self::ProcSpell, Self::AugmentationCreateItem, Self::AlternateCurrency, Self::BlueSurgeSpell, Self::YellowSurgeSpell, Self::RedSurgeSpell, Self::OlthoiDeathTreasureType, Self::PCAPRecordedWeenieHeader, Self::PCAPRecordedWeenieHeader2, Self::PCAPRecordedObjectDesc, Self::PCAPRecordedPhysicsDesc, Self::PCAPRecordedParentLocation, Self::PCAPRecordedDefaultScript, Self::PCAPRecordedTimestamp0, Self::PCAPRecordedTimestamp1, Self::PCAPRecordedTimestamp2, Self::PCAPRecordedTimestamp3, Self::PCAPRecordedTimestamp4, Self::PCAPRecordedTimestamp5, Self::PCAPRecordedTimestamp6, Self::PCAPRecordedTimestamp7, Self::PCAPRecordedTimestamp8, Self::PCAPRecordedTimestamp9, Self::PCAPRecordedMaxVelocityEstimated, Self::PCAPPhysicsDIDDataTemplatedFrom];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Setup", "MotionTable", "SoundTable", "CombatTable", "QualityFilter", "PaletteBase", "ClothingBase", "Icon", "EyesTexture", "NoseTexture", "MouthTexture", "DefaultEyesTexture", "DefaultNoseTexture", "DefaultMouthTexture", "HairPalette", "EyesPalette", "SkinPalette", "HeadObject", "ActivationAnimation", "InitMotion", "ActivationSound", "PhysicsEffectTable", "UseSound", "UseTargetAnimation", "UseTargetSuccessAnimation", "UseTargetFailureAnimation", "UseUserAnimation", "Spell", "SpellComponent", "PhysicsScript", "LinkedPortalOne", "WieldedTreasureType", "InventoryTreasureType", "ShopTreasureType", "DeathTreasureType", "MutateFilter", "ItemSkillLimit", "UseCreateItem", "DeathSpell", "VendorsClassId", "ItemSpecializedOnly", "HouseId", "AccountHouseId", "RestrictionEffect", "CreationMutationFilter", "TsysMutationFilter", "LastPortal", "LinkedPortalTwo", "OriginalPortal", "IconOverlay", "IconOverlaySecondary", "IconUnderlay", "AugmentationMutationFilter", "AugmentationEffect", "ProcSpell", "AugmentationCreateItem", "AlternateCurrency", "BlueSurgeSpell", "YellowSurgeSpell", "RedSurgeSpell", "OlthoiDeathTreasureType", "PCAPRecordedWeenieHeader", "PCAPRecordedWeenieHeader2", "PCAPRecordedObjectDesc", "PCAPRecordedPhysicsDesc", "PCAPRecordedParentLocation", "PCAPRecordedDefaultScript", "PCAPRecordedTimestamp0", "PCAPRecordedTimestamp1", "PCAPRecordedTimestamp2", "PCAPRecordedTimestamp3", "PCAPRecordedTimestamp4", "PCAPRecordedTimestamp5", "PCAPRecordedTimestamp6", "PCAPRecordedTimestamp7", "PCAPRecordedTimestamp8", "PCAPRecordedTimestamp9", "PCAPRecordedMaxVelocityEstimated", "PCAPPhysicsDIDDataTemplatedFrom"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[43, 19, 21, 57, 56, 54, 53, 58, 7, 4, 45, 39, 35, 12, 14, 13, 16, 9, 15, 18, 42, 8, 50, 51, 52, 20, 33, 37, 41, 47, 31, 48, 2, 11, 36, 10, 61, 49, 79, 67, 78, 64, 66, 65, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 62, 63, 6, 22, 30, 55, 5, 60, 44, 1, 34, 17, 3, 28, 29, 46, 0, 38, 23, 24, 26, 25, 27, 40, 32, 59];

    /// The members marked `[AssessmentProperty]`, in declaration order (ACE's reflection order).
    pub const ASSESSMENT_PROPERTY: &'static [Self] = &[Self::EyesTexture, Self::NoseTexture, Self::MouthTexture, Self::HairPalette, Self::EyesPalette, Self::SkinPalette, Self::ItemSpecializedOnly, Self::ProcSpell];
    /// Whether this value is marked `[AssessmentProperty]` (by value, like ACE's `HashSet` lookups).
    pub fn is_assessment_property(self) -> bool {
        Self::ASSESSMENT_PROPERTY.contains(&self)
    }

    /// The members marked `[SendOnLogin]`, in declaration order (ACE's reflection order).
    pub const SEND_ON_LOGIN: &'static [Self] = &[Self::MotionTable, Self::CombatTable];
    /// Whether this value is marked `[SendOnLogin]` (by value, like ACE's `HashSet` lookups).
    pub fn is_send_on_login(self) -> bool {
        Self::SEND_ON_LOGIN.contains(&self)
    }
}

super::support::ace_enum!(PropertyDataId, u16, plain);
super::support::ace_enum_from!(PropertyDataId, u16 => u32, u64, i32, i64);
