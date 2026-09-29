// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/HealKitWcids.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/HealKitWcids.cs`; do not edit by hand

//! The literal data of ACE's `HealKitWcids` (`Factories/Tables/Wcids/HealKitWcids.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `HealKitWcids.T1_Chances` (`ChanceTable<WeenieClassName>`).
pub static T1_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::healingkitcrude, 0.75),
    (WeenieClassName::healingkitplain, 0.25),
]);

/// ACE `HealKitWcids.T2_Chances` (`ChanceTable<WeenieClassName>`).
pub static T2_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::healingkitcrude, 0.25),
    (WeenieClassName::healingkitplain, 0.50),
    (WeenieClassName::healingkitgood, 0.25),
]);

/// ACE `HealKitWcids.T3_Chances` (`ChanceTable<WeenieClassName>`).
pub static T3_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::healingkitplain, 0.25),
    (WeenieClassName::healingkitgood, 0.50),
    (WeenieClassName::healingkitexcellent, 0.25),
]);

/// ACE `HealKitWcids.T4_Chances` (`ChanceTable<WeenieClassName>`).
pub static T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::healingkitgood, 0.25),
    (WeenieClassName::healingkitexcellent, 0.50),
    (WeenieClassName::healingkitpeerless, 0.25),
]);

/// ACE `HealKitWcids.T5_Chances` (`ChanceTable<WeenieClassName>`).
pub static T5_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::healingkitexcellent, 0.25),
    (WeenieClassName::healingkitpeerless, 0.50),
    (WeenieClassName::healingkittreated, 0.25),
]);

/// ACE `HealKitWcids.T6_T8_Chances` (`ChanceTable<WeenieClassName>`).
pub static T6_T8_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::healingkitpeerless, 0.25),
    (WeenieClassName::healingkittreated, 0.75),
]);

/// ACE `HealKitWcids.healKitTiers` (`List<ChanceTable<WeenieClassName>>`).
pub static HEAL_KIT_TIERS: [&ChanceTable<WeenieClassName>; 8] = [
    &T1_CHANCES,
    &T2_CHANCES,
    &T3_CHANCES,
    &T4_CHANCES,
    &T5_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
];
