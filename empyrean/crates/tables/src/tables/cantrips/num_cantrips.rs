// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Cantrips/NumCantrips.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Cantrips/NumCantrips.cs`; do not edit by hand

//! The literal data of ACE's `NumCantrips` (`Factories/Tables/Cantrips/NumCantrips.cs`).

use crate::entity::ChanceTable;

/// ACE `NumCantrips.T1_NumCantrips` (`ChanceTable<int>`).
pub static T1_NUM_CANTRIPS: ChanceTable<i32> = ChanceTable::new(&[
    (0, 0.95),
    (1, 0.05),
]);

/// ACE `NumCantrips.T2_NumCantrips` (`ChanceTable<int>`).
pub static T2_NUM_CANTRIPS: ChanceTable<i32> = ChanceTable::new(&[
    (0, 0.90),
    (1, 0.10),
]);

/// ACE `NumCantrips.T3_NumCantrips` (`ChanceTable<int>`).
pub static T3_NUM_CANTRIPS: ChanceTable<i32> = ChanceTable::new(&[
    (0, 0.725),
    (1, 0.250),
    (2, 0.025),
]);

/// ACE `NumCantrips.T4_NumCantrips` (`ChanceTable<int>`).
pub static T4_NUM_CANTRIPS: ChanceTable<i32> = ChanceTable::new(&[
    (0, 0.62),
    (1, 0.32),
    (2, 0.055),
    (3, 0.005),
]);

/// ACE `NumCantrips.T5_NumCantrips` (`ChanceTable<int>`).
pub static T5_NUM_CANTRIPS: ChanceTable<i32> = ChanceTable::new(&[
    (0, 0.40),
    (1, 0.42),
    (2, 0.155),
    (3, 0.024),
    (4, 0.001),
]);

/// ACE `NumCantrips.T6_NumCantrips` (`ChanceTable<int>`).
pub static T6_NUM_CANTRIPS: ChanceTable<i32> = ChanceTable::new(&[
    (0, 0.25),
    (1, 0.40),
    (2, 0.25),
    (3, 0.08),
    (4, 0.019),
    (5, 0.001),
]);

/// ACE `NumCantrips.T7_T8_NumCantrips` (`ChanceTable<int>`).
pub static T7_T8_NUM_CANTRIPS: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.81),
    (2, 0.17),
    (3, 0.016),
    (4, 0.004),
]);

/// ACE `NumCantrips.numCantrips` (`List<ChanceTable<int>>`).
pub static NUM_CANTRIPS: [&ChanceTable<i32>; 8] = [
    &T1_NUM_CANTRIPS,
    &T2_NUM_CANTRIPS,
    &T3_NUM_CANTRIPS,
    &T4_NUM_CANTRIPS,
    &T5_NUM_CANTRIPS,
    &T6_NUM_CANTRIPS,
    &T7_T8_NUM_CANTRIPS,
    &T7_T8_NUM_CANTRIPS,
];

/// ACE `NumCantrips.T1_T2_CantripLevel` (`ChanceTable<int>`).
pub static T1_T2_CANTRIP_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 1.0),
]);

/// ACE `NumCantrips.T3_CantripLevel` (`ChanceTable<int>`).
pub static T3_CANTRIP_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.97),
    (2, 0.03),
]);

/// ACE `NumCantrips.T4_CantripLevel` (`ChanceTable<int>`).
pub static T4_CANTRIP_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.90),
    (2, 0.10),
]);

/// ACE `NumCantrips.T5_CantripLevel` (`ChanceTable<int>`).
pub static T5_CANTRIP_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.85),
    (2, 0.15),
]);

/// ACE `NumCantrips.T6_CantripLevel` (`ChanceTable<int>`).
pub static T6_CANTRIP_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.80),
    (2, 0.20),
]);

/// ACE `NumCantrips.T7_CantripLevel` (`ChanceTable<int>`).
pub static T7_CANTRIP_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.15),
    (2, 0.60),
    (3, 0.25),
]);

/// ACE `NumCantrips.T8_CantripLevel` (`ChanceTable<int>`).
pub static T8_CANTRIP_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.02),
    (2, 0.46),
    (3, 0.42),
    (4, 0.10),
]);

/// ACE `NumCantrips.cantripLevels` (`List<ChanceTable<int>>`).
pub static CANTRIP_LEVELS: [&ChanceTable<i32>; 8] = [
    &T1_T2_CANTRIP_LEVEL,
    &T1_T2_CANTRIP_LEVEL,
    &T3_CANTRIP_LEVEL,
    &T4_CANTRIP_LEVEL,
    &T5_CANTRIP_LEVEL,
    &T6_CANTRIP_LEVEL,
    &T7_CANTRIP_LEVEL,
    &T8_CANTRIP_LEVEL,
];
