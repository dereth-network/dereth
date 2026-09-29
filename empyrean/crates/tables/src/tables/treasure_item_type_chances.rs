// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/TreasureItemTypeChances.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/TreasureItemTypeChances.cs`; do not edit by hand

//! The literal data of ACE's `TreasureItemTypeChances` (`Factories/Tables/TreasureItemTypeChances.cs`).

use crate::entity::ChanceTable;
use crate::enums::TreasureItemType;

/// ACE `TreasureItemTypeChances.DefaultMagical` (`ChanceTable<TreasureItemType>`).
pub static DEFAULT_MAGICAL: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Gem, 0.14),
    (TreasureItemType::Armor, 0.24),
    (TreasureItemType::Weapon, 0.30),
    (TreasureItemType::Clothing, 0.13),
    (TreasureItemType::Cloak, 0.01),
    (TreasureItemType::Jewelry, 0.18),
]);

/// ACE `TreasureItemTypeChances.DefaultNonMagical` (`ChanceTable<TreasureItemType>`).
pub static DEFAULT_NON_MAGICAL: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Gem, 0.14),
    (TreasureItemType::Armor, 0.24),
    (TreasureItemType::Weapon, 0.30),
    (TreasureItemType::Clothing, 0.13),
    (TreasureItemType::Cloak, 0.01),
    (TreasureItemType::Jewelry, 0.10),
    (TreasureItemType::ArtObject, 0.08),
]);

/// ACE `TreasureItemTypeChances.Armor` (`ChanceTable<TreasureItemType>`).
pub static ARMOR: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Armor, 1.0),
]);

/// ACE `TreasureItemTypeChances.Weapons` (`ChanceTable<TreasureItemType>`).
pub static WEAPONS: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Weapon, 1.0),
]);

/// ACE `TreasureItemTypeChances.Jewelry` (`ChanceTable<TreasureItemType>`).
pub static JEWELRY: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Jewelry, 1.0),
]);

/// ACE `TreasureItemTypeChances.MixedMagicEquipment` (`ChanceTable<TreasureItemType>`).
pub static MIXED_MAGIC_EQUIPMENT: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Armor, 0.30),
    (TreasureItemType::Weapon, 0.35),
    (TreasureItemType::Jewelry, 0.20),
    (TreasureItemType::Clothing, 0.14),
    (TreasureItemType::Cloak, 0.01),
]);
