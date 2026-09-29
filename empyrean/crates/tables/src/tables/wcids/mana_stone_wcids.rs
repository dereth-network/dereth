// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/ManaStoneWcids.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/ManaStoneWcids.cs`; do not edit by hand

//! The literal data of ACE's `ManaStoneWcids` (`Factories/Tables/Wcids/ManaStoneWcids.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `ManaStoneWcids.T1_Chances` (`ChanceTable<WeenieClassName>`).
pub static T1_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::manastoneminor, 0.75),
    (WeenieClassName::manastonelesser, 0.25),
]);

/// ACE `ManaStoneWcids.T2_Chances` (`ChanceTable<WeenieClassName>`).
pub static T2_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::manastoneminor, 0.25),
    (WeenieClassName::manastonelesser, 0.50),
    (WeenieClassName::manastone, 0.25),
]);

/// ACE `ManaStoneWcids.T3_Chances` (`ChanceTable<WeenieClassName>`).
pub static T3_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::manastonelesser, 0.25),
    (WeenieClassName::manastone, 0.50),
    (WeenieClassName::manastonemedium, 0.25),
]);

/// ACE `ManaStoneWcids.T4_Chances` (`ChanceTable<WeenieClassName>`).
pub static T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::manastone, 0.25),
    (WeenieClassName::manastonemedium, 0.50),
    (WeenieClassName::manastonegreater, 0.25),
]);

/// ACE `ManaStoneWcids.T5_Chances` (`ChanceTable<WeenieClassName>`).
pub static T5_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::manastonemedium, 0.25),
    (WeenieClassName::manastonegreater, 0.50),
    (WeenieClassName::manastonemajor, 0.25),
]);

/// ACE `ManaStoneWcids.T6_T8_Chances` (`ChanceTable<WeenieClassName>`).
pub static T6_T8_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::manastonegreater, 0.25),
    (WeenieClassName::manastonemajor, 0.75),
]);

/// ACE `ManaStoneWcids.manaStoneTiers` (`List<ChanceTable<WeenieClassName>>`).
pub static MANA_STONE_TIERS: [&ChanceTable<WeenieClassName>; 8] = [
    &T1_CHANCES,
    &T2_CHANCES,
    &T3_CHANCES,
    &T4_CHANCES,
    &T5_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
];
