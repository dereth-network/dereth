// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/PetDeviceChance.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/PetDeviceChance.cs`; do not edit by hand

//! The literal data of ACE's `PetDeviceChance` (`Factories/Tables/PetDeviceChance.cs`).

use crate::entity::ChanceTable;

/// ACE `PetDeviceChance.T1_T3_PetLevelChances` (`ChanceTable<int>`).
pub static T1_T3_PET_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (50, 1.0),
]);

/// ACE `PetDeviceChance.T4_PetLevelChances` (`ChanceTable<int>`).
pub static T4_PET_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (50, 0.75),
    (80, 0.25),
]);

/// ACE `PetDeviceChance.T5_PetLevelChances` (`ChanceTable<int>`).
pub static T5_PET_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (50, 0.15),
    (80, 0.65),
    (100, 0.20),
]);

/// ACE `PetDeviceChance.T6_PetLevelChances` (`ChanceTable<int>`).
pub static T6_PET_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (80, 0.15),
    (100, 0.25),
    (125, 0.50),
    (150, 0.10),
]);

/// ACE `PetDeviceChance.T7_PetLevelChances` (`ChanceTable<int>`).
pub static T7_PET_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (100, 0.15),
    (125, 0.25),
    (150, 0.50),
    (180, 0.10),
]);

/// ACE `PetDeviceChance.T8_PetLevelChances` (`ChanceTable<int>`).
pub static T8_PET_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (100, 0.0125),
    (125, 0.025),
    (150, 0.05),
    (180, 0.50),
    (200, 0.4125),
]);

/// ACE `PetDeviceChance.petLevelChances` (`List<ChanceTable<int>>`).
pub static PET_LEVEL_CHANCES: [&ChanceTable<i32>; 8] = [
    &T1_T3_PET_LEVEL_CHANCES,
    &T1_T3_PET_LEVEL_CHANCES,
    &T1_T3_PET_LEVEL_CHANCES,
    &T4_PET_LEVEL_CHANCES,
    &T5_PET_LEVEL_CHANCES,
    &T6_PET_LEVEL_CHANCES,
    &T7_PET_LEVEL_CHANCES,
    &T8_PET_LEVEL_CHANCES,
];
