// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/ScrollLevelChance.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/ScrollLevelChance.cs`; do not edit by hand

//! The literal data of ACE's `ScrollLevelChance` (`Factories/Tables/ScrollLevelChance.cs`).

use crate::entity::ChanceTable;

/// ACE `ScrollLevelChance.T1_ScrollLevelChances` (`ChanceTable<int>`).
pub static T1_SCROLL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.25),
    (2, 0.50),
    (3, 0.25),
]);

/// ACE `ScrollLevelChance.T2_ScrollLevelChances` (`ChanceTable<int>`).
pub static T2_SCROLL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (3, 0.25),
    (4, 0.50),
    (5, 0.25),
]);

/// ACE `ScrollLevelChance.T3_ScrollLevelChances` (`ChanceTable<int>`).
pub static T3_SCROLL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (5, 0.25),
    (6, 0.50),
    (7, 0.25),
]);

/// ACE `ScrollLevelChance.T4_ScrollLevelChances` (`ChanceTable<int>`).
pub static T4_SCROLL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (6, 0.50),
    (7, 0.50),
]);

/// ACE `ScrollLevelChance.T5_T8_ScrollLevelChances` (`ChanceTable<int>`).
pub static T5_T8_SCROLL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (7, 1.00),
]);

/// ACE `ScrollLevelChance.scrollLevelChances` (`List<ChanceTable<int>>`).
pub static SCROLL_LEVEL_CHANCES: [&ChanceTable<i32>; 8] = [
    &T1_SCROLL_LEVEL_CHANCES,
    &T2_SCROLL_LEVEL_CHANCES,
    &T3_SCROLL_LEVEL_CHANCES,
    &T4_SCROLL_LEVEL_CHANCES,
    &T5_T8_SCROLL_LEVEL_CHANCES,
    &T5_T8_SCROLL_LEVEL_CHANCES,
    &T5_T8_SCROLL_LEVEL_CHANCES,
    &T5_T8_SCROLL_LEVEL_CHANCES,
];
