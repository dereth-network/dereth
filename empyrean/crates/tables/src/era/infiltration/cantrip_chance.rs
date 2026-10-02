// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Cantrips/CantripChance.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Cantrips/CantripChance.cs`; do not edit by hand

//! The tables of ClassicACE's `CantripChance` under its Infiltration ruleset
//! (`Factories/Tables/Cantrips/CantripChance.cs`).

use crate::entity::ChanceTable;

/// ClassicACE `CantripChance.T1_NumCantrips`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T1_NUM_CANTRIPS: ChanceTable<i32> = ChanceTable::new(&[
    (0, 1.0),
]);

/// ClassicACE `CantripChance.T2_NumCantrips`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T2_NUM_CANTRIPS: ChanceTable<i32> = ChanceTable::new(&[
    (0, 1.0),
]);

/// ClassicACE `CantripChance.T3_NumCantrips`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T3_NUM_CANTRIPS: ChanceTable<i32> = ChanceTable::new(&[
    (0, 0.98),
    (1, 0.02),
]);

/// ClassicACE `CantripChance.T4_NumCantrips`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T4_NUM_CANTRIPS: ChanceTable<i32> = ChanceTable::new(&[
    (0, 0.95),
    (1, 0.05),
]);

/// ClassicACE `CantripChance.T5_NumCantrips`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T5_NUM_CANTRIPS: ChanceTable<i32> = ChanceTable::new(&[
    (0, 0.95),
    (1, 0.04),
    (2, 0.01),
]);

/// ClassicACE `CantripChance.T6_NumCantrips`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T6_NUM_CANTRIPS: ChanceTable<i32> = ChanceTable::new(&[
    (0, 0.930),
    (1, 0.050),
    (2, 0.015),
    (3, 0.005),
]);

/// ClassicACE `CantripChance.T7_T8_NumCantrips`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T7_T8_NUM_CANTRIPS: ChanceTable<i32> = ChanceTable::new(&[
    (0, 0.930),
    (1, 0.050),
    (2, 0.015),
    (3, 0.005),
]);

/// ClassicACE `CantripChance._numCantrips`, as its Infiltration ruleset sets it (`List<ChanceTable<int>>`).
pub static _NUM_CANTRIPS: [&ChanceTable<i32>; 8] = [
    &T1_NUM_CANTRIPS,
    &T2_NUM_CANTRIPS,
    &T3_NUM_CANTRIPS,
    &T4_NUM_CANTRIPS,
    &T5_NUM_CANTRIPS,
    &T6_NUM_CANTRIPS,
    &T7_T8_NUM_CANTRIPS,
    &T7_T8_NUM_CANTRIPS,
];

/// ClassicACE `CantripChance.T1_T2_CantripLevel`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T1_T2_CANTRIP_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 1.0),
]);

/// ClassicACE `CantripChance.T3_CantripLevel`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T3_CANTRIP_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 1.0),
]);

/// ClassicACE `CantripChance.T4_CantripLevel`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T4_CANTRIP_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.90),
    (2, 0.10),
]);

/// ClassicACE `CantripChance.T5_CantripLevel`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T5_CANTRIP_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.85),
    (2, 0.15),
]);

/// ClassicACE `CantripChance.T6_CantripLevel`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T6_CANTRIP_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.80),
    (2, 0.20),
]);

/// ClassicACE `CantripChance.T7_CantripLevel`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T7_CANTRIP_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.80),
    (2, 0.20),
]);

/// ClassicACE `CantripChance.T8_CantripLevel`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T8_CANTRIP_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.80),
    (2, 0.20),
]);

/// ClassicACE `CantripChance._cantripLevels`, as its Infiltration ruleset sets it (`List<ChanceTable<int>>`).
pub static _CANTRIP_LEVELS: [&ChanceTable<i32>; 8] = [
    &T1_T2_CANTRIP_LEVEL,
    &T1_T2_CANTRIP_LEVEL,
    &T3_CANTRIP_LEVEL,
    &T4_CANTRIP_LEVEL,
    &T5_CANTRIP_LEVEL,
    &T6_CANTRIP_LEVEL,
    &T7_CANTRIP_LEVEL,
    &T8_CANTRIP_LEVEL,
];
