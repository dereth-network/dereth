// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/WeaponTypeChance.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/WeaponTypeChance.cs`; do not edit by hand

//! The tables of ClassicACE's `WeaponTypeChance` under its Infiltration ruleset
//! (`Factories/Tables/WeaponTypeChance.cs`).

use crate::entity::ChanceTable;
use crate::enums::TreasureWeaponType;

/// ClassicACE `WeaponTypeChance.T1_T4_Chances` (`ChanceTable<TreasureWeaponType>`).
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

/// ClassicACE `WeaponTypeChance.T5_T6_Chances` (`ChanceTable<TreasureWeaponType>`).
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

/// ClassicACE `WeaponTypeChance.RetailChances`, as its Infiltration ruleset sets it (`ChanceTable<TreasureWeaponType>`).
pub static RETAIL_CHANCES: ChanceTable<TreasureWeaponType> = ChanceTable::new(&[
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

/// ClassicACE `WeaponTypeChance.MeleeChances`, as its Infiltration ruleset sets it (`ChanceTable<TreasureWeaponType>`).
pub static MELEE_CHANCES: ChanceTable<TreasureWeaponType> = ChanceTable::new_weighted(&[
    (TreasureWeaponType::Sword, 1.0),
    (TreasureWeaponType::Mace, 1.0),
    (TreasureWeaponType::Axe, 1.0),
    (TreasureWeaponType::Spear, 1.0),
    (TreasureWeaponType::Unarmed, 1.0),
    (TreasureWeaponType::Staff, 1.0),
    (TreasureWeaponType::Dagger, 1.0),
]);

/// ClassicACE `WeaponTypeChance.MissileChances` (`ChanceTable<TreasureWeaponType>`).
pub static MISSILE_CHANCES: ChanceTable<TreasureWeaponType> = ChanceTable::new(&[
    (TreasureWeaponType::Bow, 0.34),
    (TreasureWeaponType::Crossbow, 0.33),
    (TreasureWeaponType::Atlatl, 0.33),
]);

/// ClassicACE `WeaponTypeChance.weaponTiers` (`List<ChanceTable<TreasureWeaponType>>`).
pub static WEAPON_TIERS: [&ChanceTable<TreasureWeaponType>; 6] = [
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T5_T6_CHANCES,
    &T5_T6_CHANCES,
];
