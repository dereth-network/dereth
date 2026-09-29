// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/WorkmanshipChance.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/WorkmanshipChance.cs`; do not edit by hand

//! The literal data of ACE's `WorkmanshipChance` (`Factories/Tables/WorkmanshipChance.cs`).

use crate::entity::ChanceTable;

/// ACE `WorkmanshipChance.T1_Chances` (`ChanceTable<int>`).
pub static T1_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.05),
    (2, 0.15),
    (3, 0.3),
    (4, 0.3),
    (5, 0.2),
]);

/// ACE `WorkmanshipChance.T2_Chances` (`ChanceTable<int>`).
pub static T2_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (2, 0.05),
    (3, 0.15),
    (4, 0.3),
    (5, 0.3),
    (6, 0.2),
]);

/// ACE `WorkmanshipChance.T3_Chances` (`ChanceTable<int>`).
pub static T3_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (3, 0.05),
    (4, 0.15),
    (5, 0.3),
    (6, 0.3),
    (7, 0.2),
]);

/// ACE `WorkmanshipChance.T4_Chances` (`ChanceTable<int>`).
pub static T4_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (3, 0.01),
    (4, 0.05),
    (5, 0.15),
    (6, 0.3),
    (7, 0.29),
    (8, 0.2),
]);

/// ACE `WorkmanshipChance.T5_Chances` (`ChanceTable<int>`).
pub static T5_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (3, 0.01),
    (4, 0.03),
    (5, 0.05),
    (6, 0.25),
    (7, 0.46),
    (8, 0.15),
    (9, 0.05),
]);

/// ACE `WorkmanshipChance.T6_Chances` (`ChanceTable<int>`).
pub static T6_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (4, 0.01),
    (5, 0.04),
    (6, 0.25),
    (7, 0.25),
    (8, 0.25),
    (9, 0.15),
    (10, 0.05),
]);

/// ACE `WorkmanshipChance.workmanshipChances` (`List<ChanceTable<int>>`).
pub static WORKMANSHIP_CHANCES: [&ChanceTable<i32>; 6] = [
    &T1_CHANCES,
    &T2_CHANCES,
    &T3_CHANCES,
    &T4_CHANCES,
    &T5_CHANCES,
    &T6_CHANCES,
];
