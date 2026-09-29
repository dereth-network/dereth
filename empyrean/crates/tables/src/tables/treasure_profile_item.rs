// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/TreasureProfile_Item.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/TreasureProfile_Item.cs`; do not edit by hand

//! The literal data of ACE's `TreasureProfile_Item` (`Factories/Tables/TreasureProfile_Item.cs`).

use crate::entity::ChanceTable;
use crate::enums::TreasureItemType;

/// ACE `TreasureProfile_Item.itemProfile1` (`ChanceTable<TreasureItemType>`).
pub static ITEM_PROFILE1: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Weapon, 1.0),
]);

/// ACE `TreasureProfile_Item.itemProfile2` (`ChanceTable<TreasureItemType>`).
pub static ITEM_PROFILE2: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Armor, 1.0),
]);

/// ACE `TreasureProfile_Item.itemProfile3` (`ChanceTable<TreasureItemType>`).
pub static ITEM_PROFILE3: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Scroll, 1.0),
]);

/// ACE `TreasureProfile_Item.itemProfile4` (`ChanceTable<TreasureItemType>`).
pub static ITEM_PROFILE4: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Clothing, 1.0),
]);

/// ACE `TreasureProfile_Item.itemProfile5` (`ChanceTable<TreasureItemType>`).
pub static ITEM_PROFILE5: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Jewelry, 1.0),
]);

/// ACE `TreasureProfile_Item.itemProfile6` (`ChanceTable<TreasureItemType>`).
pub static ITEM_PROFILE6: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Gem, 1.0),
]);

/// ACE `TreasureProfile_Item.itemProfile7` (`ChanceTable<TreasureItemType>`).
pub static ITEM_PROFILE7: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::ArtObject, 1.0),
]);

/// ACE `TreasureProfile_Item.itemProfile8` (`ChanceTable<TreasureItemType>`).
pub static ITEM_PROFILE8: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Weapon, 0.125),
    (TreasureItemType::Armor, 0.125),
    (TreasureItemType::Scroll, 0.125),
    (TreasureItemType::Clothing, 0.125),
    (TreasureItemType::Jewelry, 0.125),
    (TreasureItemType::Gem, 0.125),
    (TreasureItemType::ArtObject, 0.125),
    (TreasureItemType::PetDevice, 0.125),
]);

/// ACE `TreasureProfile_Item.itemProfile9` (`ChanceTable<TreasureItemType>`).
pub static ITEM_PROFILE9: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Weapon, 0.20),
    (TreasureItemType::Armor, 0.20),
    (TreasureItemType::Scroll, 0.20),
    (TreasureItemType::Clothing, 0.05),
    (TreasureItemType::Jewelry, 0.05),
    (TreasureItemType::Gem, 0.05),
    (TreasureItemType::ArtObject, 0.05),
    (TreasureItemType::PetDevice, 0.20),
]);

/// ACE `TreasureProfile_Item.itemProfile10` (`ChanceTable<TreasureItemType>`).
pub static ITEM_PROFILE10: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Weapon, 0.30),
    (TreasureItemType::Armor, 0.30),
    (TreasureItemType::Scroll, 0.20),
    (TreasureItemType::PetDevice, 0.20),
]);

/// ACE `TreasureProfile_Item.itemProfile11` (`ChanceTable<TreasureItemType>`).
pub static ITEM_PROFILE11: ChanceTable<TreasureItemType> = ChanceTable::new(&[
    (TreasureItemType::Clothing, 0.25),
    (TreasureItemType::Jewelry, 0.25),
    (TreasureItemType::Gem, 0.25),
    (TreasureItemType::ArtObject, 0.25),
]);

/// ACE `TreasureProfile_Item.itemProfiles` (`List<ChanceTable<TreasureItemType>>`).
pub static ITEM_PROFILES: [&ChanceTable<TreasureItemType>; 11] = [
    &ITEM_PROFILE1,
    &ITEM_PROFILE2,
    &ITEM_PROFILE3,
    &ITEM_PROFILE4,
    &ITEM_PROFILE5,
    &ITEM_PROFILE6,
    &ITEM_PROFILE7,
    &ITEM_PROFILE8,
    &ITEM_PROFILE9,
    &ITEM_PROFILE10,
    &ITEM_PROFILE11,
];
