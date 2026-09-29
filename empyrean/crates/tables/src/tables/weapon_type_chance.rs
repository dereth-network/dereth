// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/WeaponTypeChance.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/WeaponTypeChance.cs`; do not edit by hand

//! The literal data of ACE's `WeaponTypeChance` (`Factories/Tables/WeaponTypeChance.cs`).

use crate::entity::ChanceTable;
use crate::enums::TreasureWeaponType;

/// ACE `WeaponTypeChance.T1_T4_Chances` (`ChanceTable<TreasureWeaponType>`).
pub static T1_T4_CHANCES: ChanceTable<TreasureWeaponType> = ChanceTable::new(&[
    (TreasureWeaponType::Sword, 0.12),
    (TreasureWeaponType::Mace, 0.12),
    (TreasureWeaponType::Axe, 0.12),
    (TreasureWeaponType::Spear, 0.12),
    (TreasureWeaponType::Unarmed, 0.12),
    (TreasureWeaponType::Staff, 0.12),
    (TreasureWeaponType::Dagger, 0.12),
    (TreasureWeaponType::Bow, 0.04),
    (TreasureWeaponType::Crossbow, 0.04),
    (TreasureWeaponType::Atlatl, 0.04),
    (TreasureWeaponType::Caster, 0.04),
]);

/// ACE `WeaponTypeChance.T5_T6_Chances` (`ChanceTable<TreasureWeaponType>`).
pub static T5_T6_CHANCES: ChanceTable<TreasureWeaponType> = ChanceTable::new(&[
    (TreasureWeaponType::Sword, 0.09),
    (TreasureWeaponType::Mace, 0.09),
    (TreasureWeaponType::Axe, 0.09),
    (TreasureWeaponType::Spear, 0.09),
    (TreasureWeaponType::Unarmed, 0.09),
    (TreasureWeaponType::Staff, 0.09),
    (TreasureWeaponType::Dagger, 0.09),
    (TreasureWeaponType::Bow, 0.09),
    (TreasureWeaponType::Crossbow, 0.09),
    (TreasureWeaponType::Atlatl, 0.09),
    (TreasureWeaponType::Caster, 0.10),
]);

/// ACE `WeaponTypeChance.RetailChances` (`ChanceTable<TreasureWeaponType>`).
pub static RETAIL_CHANCES: ChanceTable<TreasureWeaponType> = ChanceTable::new(&[
    (TreasureWeaponType::Sword, 0.09),
    (TreasureWeaponType::Mace, 0.09),
    (TreasureWeaponType::Axe, 0.09),
    (TreasureWeaponType::Spear, 0.09),
    (TreasureWeaponType::Unarmed, 0.09),
    (TreasureWeaponType::Staff, 0.09),
    (TreasureWeaponType::Dagger, 0.09),
    (TreasureWeaponType::Bow, 0.07),
    (TreasureWeaponType::Crossbow, 0.07),
    (TreasureWeaponType::Atlatl, 0.06),
    (TreasureWeaponType::Caster, 0.07),
    (TreasureWeaponType::TwoHandedWeapon, 0.10),
]);

/// ACE `WeaponTypeChance.MeleeChances` (`ChanceTable<TreasureWeaponType>`).
pub static MELEE_CHANCES: ChanceTable<TreasureWeaponType> = ChanceTable::new(&[
    (TreasureWeaponType::Sword, 0.125),
    (TreasureWeaponType::Mace, 0.125),
    (TreasureWeaponType::Axe, 0.125),
    (TreasureWeaponType::Spear, 0.125),
    (TreasureWeaponType::Unarmed, 0.125),
    (TreasureWeaponType::Staff, 0.125),
    (TreasureWeaponType::Dagger, 0.125),
    (TreasureWeaponType::TwoHandedWeapon, 0.125),
]);

/// ACE `WeaponTypeChance.MissileChances` (`ChanceTable<TreasureWeaponType>`).
pub static MISSILE_CHANCES: ChanceTable<TreasureWeaponType> = ChanceTable::new(&[
    (TreasureWeaponType::Bow, 0.334),
    (TreasureWeaponType::Crossbow, 0.333),
    (TreasureWeaponType::Atlatl, 0.333),
]);

/// ACE `WeaponTypeChance.weaponTiers` (`List<ChanceTable<TreasureWeaponType>>`).
pub static WEAPON_TIERS: [&ChanceTable<TreasureWeaponType>; 6] = [
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T5_T6_CHANCES,
    &T5_T6_CHANCES,
];
