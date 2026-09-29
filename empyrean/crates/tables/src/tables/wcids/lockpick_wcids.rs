// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/LockpickWcids.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/LockpickWcids.cs`; do not edit by hand

//! The literal data of ACE's `LockpickWcids` (`Factories/Tables/Wcids/LockpickWcids.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `LockpickWcids.T1_Chances` (`ChanceTable<WeenieClassName>`).
pub static T1_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::lockpickplain, 0.75),
    (WeenieClassName::lockpickreliable, 0.25),
]);

/// ACE `LockpickWcids.T2_Chances` (`ChanceTable<WeenieClassName>`).
pub static T2_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::lockpickplain, 0.25),
    (WeenieClassName::lockpickreliable, 0.50),
    (WeenieClassName::lockpickgood, 0.25),
]);

/// ACE `LockpickWcids.T3_Chances` (`ChanceTable<WeenieClassName>`).
pub static T3_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::lockpickreliable, 0.25),
    (WeenieClassName::lockpickgood, 0.50),
    (WeenieClassName::lockpickexcell, 0.25),
]);

/// ACE `LockpickWcids.T4_Chances` (`ChanceTable<WeenieClassName>`).
pub static T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::lockpickgood, 0.25),
    (WeenieClassName::lockpickexcell, 0.50),
    (WeenieClassName::lockpicksuperb, 0.25),
]);

/// ACE `LockpickWcids.T5_Chances` (`ChanceTable<WeenieClassName>`).
pub static T5_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::lockpickexcell, 0.25),
    (WeenieClassName::lockpicksuperb, 0.50),
    (WeenieClassName::lockpickpeer, 0.25),
]);

/// ACE `LockpickWcids.T6_T8_Chances` (`ChanceTable<WeenieClassName>`).
pub static T6_T8_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::lockpicksuperb, 0.25),
    (WeenieClassName::lockpickpeer, 0.75),
]);

/// ACE `LockpickWcids.lockpickTiers` (`List<ChanceTable<WeenieClassName>>`).
pub static LOCKPICK_TIERS: [&ChanceTable<WeenieClassName>; 8] = [
    &T1_CHANCES,
    &T2_CHANCES,
    &T3_CHANCES,
    &T4_CHANCES,
    &T5_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
];
