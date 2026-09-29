// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/CloakChance.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/CloakChance.cs`; do not edit by hand

//! The literal data of ACE's `CloakChance` (`Factories/Tables/CloakChance.cs`).

use crate::entity::ChanceTable;
use empyrean_entity::enums::{EquipmentSet, SpellId};

/// ACE `CloakChance.T1_ItemMaxLevel` (`ChanceTable<int>`).
pub static T1_ITEM_MAX_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 1.0),
]);

/// ACE `CloakChance.T2_ItemMaxLevel` (`ChanceTable<int>`).
pub static T2_ITEM_MAX_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.99),
    (2, 0.01),
]);

/// ACE `CloakChance.T3_T4_ItemMaxLevel` (`ChanceTable<int>`).
pub static T3_T4_ITEM_MAX_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.44),
    (2, 0.55),
    (3, 0.01),
]);

/// ACE `CloakChance.T5_ItemMaxLevel` (`ChanceTable<int>`).
pub static T5_ITEM_MAX_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.04),
    (2, 0.40),
    (3, 0.55),
    (4, 0.01),
]);

/// ACE `CloakChance.T6_ItemMaxLevel` (`ChanceTable<int>`).
pub static T6_ITEM_MAX_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.04),
    (2, 0.30),
    (3, 0.65),
    (4, 0.01),
]);

/// ACE `CloakChance.T7_T8_ItemMaxLevel` (`ChanceTable<int>`).
pub static T7_T8_ITEM_MAX_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (2, 0.45),
    (3, 0.50),
    (4, 0.04),
    (5, 0.01),
]);

/// ACE `CloakChance.cloakLevels` (`List<ChanceTable<int>>`).
pub static CLOAK_LEVELS: [&ChanceTable<i32>; 8] = [
    &T1_ITEM_MAX_LEVEL,
    &T2_ITEM_MAX_LEVEL,
    &T3_T4_ITEM_MAX_LEVEL,
    &T3_T4_ITEM_MAX_LEVEL,
    &T5_ITEM_MAX_LEVEL,
    &T6_ITEM_MAX_LEVEL,
    &T7_T8_ITEM_MAX_LEVEL,
    &T7_T8_ITEM_MAX_LEVEL,
];

/// ACE `CloakChance.cloakSets` (`List<EquipmentSet>`).
pub static CLOAK_SETS: [EquipmentSet; 35] = [
    EquipmentSet::CloakAlchemy,
    EquipmentSet::CloakArcaneLore,
    EquipmentSet::CloakArmorTinkering,
    EquipmentSet::CloakAssessPerson,
    EquipmentSet::CloakLightWeapons,
    EquipmentSet::CloakMissileWeapons,
    EquipmentSet::CloakCooking,
    EquipmentSet::CloakCreatureEnchantment,
    EquipmentSet::CloakFinesseWeapons,
    EquipmentSet::CloakDeception,
    EquipmentSet::CloakFletching,
    EquipmentSet::CloakHealing,
    EquipmentSet::CloakItemEnchantment,
    EquipmentSet::CloakItemTinkering,
    EquipmentSet::CloakLeadership,
    EquipmentSet::CloakLifeMagic,
    EquipmentSet::CloakLoyalty,
    EquipmentSet::CloakMagicDefense,
    EquipmentSet::CloakMagicItemTinkering,
    EquipmentSet::CloakManaConversion,
    EquipmentSet::CloakMeleeDefense,
    EquipmentSet::CloakMissileDefense,
    EquipmentSet::CloakSalvaging,
    EquipmentSet::CloakHeavyWeapons,
    EquipmentSet::CloakTwoHandedCombat,
    EquipmentSet::CloakVoidMagic,
    EquipmentSet::CloakWarMagic,
    EquipmentSet::CloakWeaponTinkering,
    EquipmentSet::CloakAssessCreature,
    EquipmentSet::CloakDirtyFighting,
    EquipmentSet::CloakDualWield,
    EquipmentSet::CloakRecklessness,
    EquipmentSet::CloakShield,
    EquipmentSet::CloakSneakAttack,
    EquipmentSet::CloakSummoning,
];

/// ACE `CloakChance.surgeSpells` (`List<SpellId>`).
pub static SURGE_SPELLS: [SpellId; 12] = [
    SpellId::AcidRing,
    SpellId::BladeRing,
    SpellId::FlameRing,
    SpellId::ForceRing,
    SpellId::FrostRing,
    SpellId::LightningRing,
    SpellId::ShockwaveRing,
    SpellId::NetherRing,
    SpellId::CloakAllSkill,
    SpellId::CloakMagicDLower,
    SpellId::CloakMeleeDLower,
    SpellId::CloakMissileDLower,
];
