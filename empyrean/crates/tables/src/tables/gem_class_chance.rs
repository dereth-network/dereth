// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/GemClassChance.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/GemClassChance.cs`; do not edit by hand

//! The literal data of ACE's `GemClassChance` (`Factories/Tables/GemClassChance.cs`).

use crate::entity::ChanceTable;

/// ACE `GemClassChance.T1_Chances` (`ChanceTable<int>`).
pub static T1_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.9),
    (2, 0.1),
]);

/// ACE `GemClassChance.T2_Chances` (`ChanceTable<int>`).
pub static T2_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.6),
    (2, 0.3),
    (3, 0.1),
]);

/// ACE `GemClassChance.T3_Chances` (`ChanceTable<int>`).
pub static T3_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.2),
    (2, 0.5),
    (3, 0.2),
    (4, 0.1),
]);

/// ACE `GemClassChance.T4_Chances` (`ChanceTable<int>`).
pub static T4_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.05),
    (2, 0.25),
    (3, 0.3),
    (4, 0.25),
    (5, 0.15),
]);

/// ACE `GemClassChance.T5_Chances` (`ChanceTable<int>`).
pub static T5_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (3, 0.2),
    (4, 0.4),
    (5, 0.2),
    (6, 0.2),
]);

/// ACE `GemClassChance.T6_Chances` (`ChanceTable<int>`).
pub static T6_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (4, 0.2),
    (5, 0.3),
    (6, 0.5),
]);

/// ACE `GemClassChance.gemClassChances` (`List<ChanceTable<int>>`).
pub static GEM_CLASS_CHANCES: [&ChanceTable<i32>; 6] = [
    &T1_CHANCES,
    &T2_CHANCES,
    &T3_CHANCES,
    &T4_CHANCES,
    &T5_CHANCES,
    &T6_CHANCES,
];
