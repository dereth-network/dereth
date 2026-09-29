// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/EquipmentSetChance.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/EquipmentSetChance.cs`; do not edit by hand

//! The literal data of ACE's `EquipmentSetChance` (`Factories/Tables/EquipmentSetChance.cs`).

use crate::entity::ChanceTable;
use empyrean_entity::enums::EquipmentSet;

/// ACE `EquipmentSetChance.armorSetChance` (`ChanceTable<bool>`).
pub static ARMOR_SET_CHANCE: ChanceTable<bool> = ChanceTable::new(&[
    (false, 0.66),
    (true, 0.34),
]);

/// ACE `EquipmentSetChance.armorSets` (`List<EquipmentSet>`).
pub static ARMOR_SETS: [EquipmentSet; 17] = [
    EquipmentSet::Soldiers,
    EquipmentSet::Adepts,
    EquipmentSet::Archers,
    EquipmentSet::Defenders,
    EquipmentSet::Tinkers,
    EquipmentSet::Crafters,
    EquipmentSet::Hearty,
    EquipmentSet::Dexterous,
    EquipmentSet::Wise,
    EquipmentSet::Swift,
    EquipmentSet::Hardened,
    EquipmentSet::Reinforced,
    EquipmentSet::Interlocking,
    EquipmentSet::Flameproof,
    EquipmentSet::Acidproof,
    EquipmentSet::Coldproof,
    EquipmentSet::Lightningproof,
];
