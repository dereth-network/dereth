// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/ConsumeWcids.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Wcids/ConsumeWcids.cs`; do not edit by hand

//! The tables of ClassicACE's `ConsumeWcids` under its Infiltration ruleset
//! (`Factories/Tables/Wcids/ConsumeWcids.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ClassicACE `ConsumeWcids.T1_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::apple, 0.06),
    (WeenieClassName::bread, 0.06),
    (WeenieClassName::cabbage, 0.05),
    (WeenieClassName::cheese, 0.05),
    (WeenieClassName::chicken, 0.05),
    (WeenieClassName::egg, 0.05),
    (WeenieClassName::fish, 0.05),
    (WeenieClassName::grapes, 0.05),
    (WeenieClassName::beefside, 0.05),
    (WeenieClassName::mushroom, 0.05),
    (WeenieClassName::healthdraught, 0.12),
    (WeenieClassName::manadraught, 0.12),
    (WeenieClassName::staminapotion, 0.12),
    (WeenieClassName::healthpotion, 0.04),
    (WeenieClassName::manapotion, 0.04),
    (WeenieClassName::staminatincture, 0.04),
]);

/// ClassicACE `ConsumeWcids.T2_Chances` (`ChanceTable<WeenieClassName>`).
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

/// ClassicACE `ConsumeWcids.T3_Chances` (`ChanceTable<WeenieClassName>`).
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

/// ClassicACE `ConsumeWcids.T4_Chances` (`ChanceTable<WeenieClassName>`).
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

/// ClassicACE `ConsumeWcids.T5_Chances` (`ChanceTable<WeenieClassName>`).
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

/// ClassicACE `ConsumeWcids.T6_T8_Chances` (`ChanceTable<WeenieClassName>`).
pub static T6_T8_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::healthtonic, 0.08),
    (WeenieClassName::manatonic, 0.08),
    (WeenieClassName::staminatonic, 0.09),
    (WeenieClassName::healthphiltre, 0.25),
    (WeenieClassName::manaphiltre, 0.25),
    (WeenieClassName::staminaphiltre, 0.25),
]);

/// ClassicACE `ConsumeWcids.consumeTiers`, as its Infiltration ruleset sets it (`List<ChanceTable<WeenieClassName>>`).
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
