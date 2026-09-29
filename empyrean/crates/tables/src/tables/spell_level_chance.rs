// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/SpellLevelChance.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/SpellLevelChance.cs`; do not edit by hand

//! The literal data of ACE's `SpellLevelChance` (`Factories/Tables/SpellLevelChance.cs`).

use crate::entity::ChanceTable;

/// ACE `SpellLevelChance.T1_SpellLevelChances` (`ChanceTable<int>`).
pub static T1_SPELL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.25),
    (2, 0.50),
    (3, 0.25),
]);

/// ACE `SpellLevelChance.T2_SpellLevelChances` (`ChanceTable<int>`).
pub static T2_SPELL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (3, 0.25),
    (4, 0.50),
    (5, 0.25),
]);

/// ACE `SpellLevelChance.T3_SpellLevelChances` (`ChanceTable<int>`).
pub static T3_SPELL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (4, 0.25),
    (5, 0.50),
    (6, 0.25),
]);

/// ACE `SpellLevelChance.T4_SpellLevelChances` (`ChanceTable<int>`).
pub static T4_SPELL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (5, 0.75),
    (6, 0.25),
]);

/// ACE `SpellLevelChance.T5_SpellLevelChances` (`ChanceTable<int>`).
pub static T5_SPELL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (5, 0.30),
    (6, 0.50),
    (7, 0.20),
]);

/// ACE `SpellLevelChance.T6_SpellLevelChances` (`ChanceTable<int>`).
pub static T6_SPELL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (6, 0.60),
    (7, 0.40),
]);

/// ACE `SpellLevelChance.T7_SpellLevelChances` (`ChanceTable<int>`).
pub static T7_SPELL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (6, 0.25),
    (7, 0.50),
    (8, 0.25),
]);

/// ACE `SpellLevelChance.T8_SpellLevelChances` (`ChanceTable<int>`).
pub static T8_SPELL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (6, 0.15),
    (7, 0.50),
    (8, 0.35),
]);

/// ACE `SpellLevelChance.spellLevelChances` (`List<ChanceTable<int>>`).
pub static SPELL_LEVEL_CHANCES: [&ChanceTable<i32>; 8] = [
    &T1_SPELL_LEVEL_CHANCES,
    &T2_SPELL_LEVEL_CHANCES,
    &T3_SPELL_LEVEL_CHANCES,
    &T4_SPELL_LEVEL_CHANCES,
    &T5_SPELL_LEVEL_CHANCES,
    &T6_SPELL_LEVEL_CHANCES,
    &T7_SPELL_LEVEL_CHANCES,
    &T8_SPELL_LEVEL_CHANCES,
];
