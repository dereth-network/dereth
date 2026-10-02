// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/ScrollLevelChance.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/ScrollLevelChance.cs`; do not edit by hand

//! The tables of ClassicACE's `ScrollLevelChance` under its Infiltration ruleset
//! (`Factories/Tables/ScrollLevelChance.cs`).

use crate::entity::ChanceTable;

/// ClassicACE `ScrollLevelChance.T1_ScrollLevelChances`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T1_SCROLL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.40),
    (2, 0.58),
    (3, 0.02),
]);

/// ClassicACE `ScrollLevelChance.T2_ScrollLevelChances`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T2_SCROLL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (3, 0.58),
    (4, 0.38),
    (5, 0.04),
]);

/// ClassicACE `ScrollLevelChance.T3_ScrollLevelChances`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T3_SCROLL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (4, 0.65),
    (5, 0.30),
    (6, 0.05),
]);

/// ClassicACE `ScrollLevelChance.T4_ScrollLevelChances`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T4_SCROLL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (4, 0.10),
    (5, 0.80),
    (6, 0.10),
]);

/// ClassicACE `ScrollLevelChance.T5_ScrollLevelChances`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T5_SCROLL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (5, 0.20),
    (6, 0.80),
]);

/// ClassicACE `ScrollLevelChance.T6_ScrollLevelChances`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T6_SCROLL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (6, 1.00),
]);

/// ClassicACE `ScrollLevelChance.T5_T8_ScrollLevelChances` (`ChanceTable<int>`).
pub static T5_T8_SCROLL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (7, 1.00),
]);

/// ClassicACE `ScrollLevelChance.scrollLevelChances`, as its Infiltration ruleset sets it (`List<ChanceTable<int>>`).
pub static SCROLL_LEVEL_CHANCES: [&ChanceTable<i32>; 8] = [
    &T1_SCROLL_LEVEL_CHANCES,
    &T2_SCROLL_LEVEL_CHANCES,
    &T3_SCROLL_LEVEL_CHANCES,
    &T4_SCROLL_LEVEL_CHANCES,
    &T5_SCROLL_LEVEL_CHANCES,
    &T6_SCROLL_LEVEL_CHANCES,
    &T6_SCROLL_LEVEL_CHANCES,
    &T6_SCROLL_LEVEL_CHANCES,
];
