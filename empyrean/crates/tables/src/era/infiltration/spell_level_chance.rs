// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/SpellLevelChance.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/SpellLevelChance.cs`; do not edit by hand

//! The tables of ClassicACE's `SpellLevelChance` under its Infiltration ruleset
//! (`Factories/Tables/SpellLevelChance.cs`).

use crate::entity::ChanceTable;

/// ClassicACE `SpellLevelChance.T1_SpellLevelChances`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T1_SPELL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.40),
    (2, 0.58),
    (3, 0.02),
]);

/// ClassicACE `SpellLevelChance.T2_SpellLevelChances`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T2_SPELL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (3, 0.58),
    (4, 0.38),
    (5, 0.04),
]);

/// ClassicACE `SpellLevelChance.T3_SpellLevelChances`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T3_SPELL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (4, 0.65),
    (5, 0.30),
    (6, 0.05),
]);

/// ClassicACE `SpellLevelChance.T4_SpellLevelChances`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T4_SPELL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (4, 0.10),
    (5, 0.80),
    (6, 0.10),
]);

/// ClassicACE `SpellLevelChance.T5_SpellLevelChances`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T5_SPELL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (5, 0.75),
    (6, 0.25),
]);

/// ClassicACE `SpellLevelChance.T6_SpellLevelChances`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T6_SPELL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (5, 0.20),
    (6, 0.80),
]);

/// ClassicACE `SpellLevelChance.T7_SpellLevelChances`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T7_SPELL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (6, 0.95),
    (7, 0.05),
]);

/// ClassicACE `SpellLevelChance.T8_SpellLevelChances`, as its Infiltration ruleset sets it (`ChanceTable<int>`).
pub static T8_SPELL_LEVEL_CHANCES: ChanceTable<i32> = ChanceTable::new(&[
    (6, 0.75),
    (7, 0.25),
]);

/// ClassicACE `SpellLevelChance.spellLevelChances`, as its Infiltration ruleset sets it (`List<ChanceTable<int>>`).
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
