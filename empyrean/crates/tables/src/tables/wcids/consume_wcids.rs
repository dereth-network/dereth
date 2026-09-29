// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/ConsumeWcids.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/ConsumeWcids.cs`; do not edit by hand

//! The literal data of ACE's `ConsumeWcids` (`Factories/Tables/Wcids/ConsumeWcids.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `ConsumeWcids.T1_Chances` (`ChanceTable<WeenieClassName>`).
pub static T1_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::apple, 0.01),
    (WeenieClassName::bread, 0.01),
    (WeenieClassName::cabbage, 0.01),
    (WeenieClassName::cheese, 0.01),
    (WeenieClassName::chicken, 0.01),
    (WeenieClassName::egg, 0.01),
    (WeenieClassName::fish, 0.01),
    (WeenieClassName::grapes, 0.01),
    (WeenieClassName::beefside, 0.01),
    (WeenieClassName::mushroom, 0.01),
    (WeenieClassName::healthdraught, 0.25),
    (WeenieClassName::manadraught, 0.25),
    (WeenieClassName::staminapotion, 0.16),
    (WeenieClassName::healthpotion, 0.08),
    (WeenieClassName::manapotion, 0.08),
    (WeenieClassName::staminatincture, 0.08),
]);

/// ACE `ConsumeWcids.T2_Chances` (`ChanceTable<WeenieClassName>`).
pub static T2_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::healthdraught, 0.08),
    (WeenieClassName::manadraught, 0.08),
    (WeenieClassName::staminapotion, 0.08),
    (WeenieClassName::healthpotion, 0.17),
    (WeenieClassName::manapotion, 0.17),
    (WeenieClassName::staminatincture, 0.18),
    (WeenieClassName::healthtincture, 0.08),
    (WeenieClassName::manatincture, 0.08),
    (WeenieClassName::staminaelixir, 0.08),
]);

/// ACE `ConsumeWcids.T3_Chances` (`ChanceTable<WeenieClassName>`).
pub static T3_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::healthpotion, 0.08),
    (WeenieClassName::manapotion, 0.08),
    (WeenieClassName::staminatincture, 0.08),
    (WeenieClassName::healthtincture, 0.17),
    (WeenieClassName::manatincture, 0.17),
    (WeenieClassName::staminaelixir, 0.18),
    (WeenieClassName::healthelixir, 0.08),
    (WeenieClassName::manaelixir, 0.08),
    (WeenieClassName::staminabrew, 0.08),
]);

/// ACE `ConsumeWcids.T4_Chances` (`ChanceTable<WeenieClassName>`).
pub static T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::healthtincture, 0.08),
    (WeenieClassName::manatincture, 0.08),
    (WeenieClassName::staminaelixir, 0.08),
    (WeenieClassName::healthelixir, 0.17),
    (WeenieClassName::manaelixir, 0.17),
    (WeenieClassName::staminabrew, 0.18),
    (WeenieClassName::healthtonic, 0.08),
    (WeenieClassName::manatonic, 0.08),
    (WeenieClassName::staminatonic, 0.08),
]);

/// ACE `ConsumeWcids.T5_Chances` (`ChanceTable<WeenieClassName>`).
pub static T5_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::healthelixir, 0.08),
    (WeenieClassName::manaelixir, 0.08),
    (WeenieClassName::staminabrew, 0.08),
    (WeenieClassName::healthtonic, 0.17),
    (WeenieClassName::manatonic, 0.17),
    (WeenieClassName::staminatonic, 0.18),
    (WeenieClassName::healthphiltre, 0.08),
    (WeenieClassName::manaphiltre, 0.08),
    (WeenieClassName::staminaphiltre, 0.08),
]);

/// ACE `ConsumeWcids.T6_T8_Chances` (`ChanceTable<WeenieClassName>`).
pub static T6_T8_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::healthtonic, 0.08),
    (WeenieClassName::manatonic, 0.08),
    (WeenieClassName::staminatonic, 0.09),
    (WeenieClassName::healthphiltre, 0.25),
    (WeenieClassName::manaphiltre, 0.25),
    (WeenieClassName::staminaphiltre, 0.25),
]);

/// ACE `ConsumeWcids.consumeTiers` (`List<ChanceTable<WeenieClassName>>`).
pub static CONSUME_TIERS: [&ChanceTable<WeenieClassName>; 8] = [
    &T1_CHANCES,
    &T2_CHANCES,
    &T3_CHANCES,
    &T4_CHANCES,
    &T5_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
];
