// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/BowWcids_Sho.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/BowWcids_Sho.cs`; do not edit by hand

//! The literal data of ACE's `BowWcids_Sho` (`Factories/Tables/Wcids/Weapons/BowWcids_Sho.cs`).
//!
//! Runtime state, hand-ported in `crate::logic` (not literal here):
//! - `_combined` (`Dictionary<WeenieClassName, TreasureWeaponType>`)

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `BowWcids_Sho.T1_T4_Chances` (`ChanceTable<WeenieClassName>`).
pub static T1_T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::bowshort, 0.50),
    (WeenieClassName::shouyumi, 0.25),
    (WeenieClassName::yumi, 0.25),
]);

/// ACE `BowWcids_Sho.T5_Chances` (`ChanceTable<WeenieClassName>`).
pub static T5_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::bowshort, 0.25),
    (WeenieClassName::shouyumi, 0.13),
    (WeenieClassName::yumi, 0.13),
    (WeenieClassName::bowslashing, 0.035),
    (WeenieClassName::bowpiercing, 0.035),
    (WeenieClassName::bowblunt, 0.035),
    (WeenieClassName::bowacid, 0.035),
    (WeenieClassName::bowfire, 0.035),
    (WeenieClassName::bowfrost, 0.035),
    (WeenieClassName::bowelectric, 0.035),
    (WeenieClassName::ace31798_slashingcompoundbow, 0.035),
    (WeenieClassName::ace31804_piercingcompoundbow, 0.035),
    (WeenieClassName::ace31800_bluntcompoundbow, 0.035),
    (WeenieClassName::ace31799_acidcompoundbow, 0.035),
    (WeenieClassName::ace31802_firecompoundbow, 0.035),
    (WeenieClassName::ace31803_frostcompoundbow, 0.035),
    (WeenieClassName::ace31801_electriccompoundbow, 0.035),
]);

/// ACE `BowWcids_Sho.T6_T8_Chances` (`ChanceTable<WeenieClassName>`).
pub static T6_T8_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::bowslashing, 0.075),
    (WeenieClassName::bowpiercing, 0.075),
    (WeenieClassName::bowblunt, 0.07),
    (WeenieClassName::bowacid, 0.07),
    (WeenieClassName::bowfire, 0.07),
    (WeenieClassName::bowfrost, 0.07),
    (WeenieClassName::bowelectric, 0.07),
    (WeenieClassName::ace31798_slashingcompoundbow, 0.075),
    (WeenieClassName::ace31804_piercingcompoundbow, 0.075),
    (WeenieClassName::ace31800_bluntcompoundbow, 0.07),
    (WeenieClassName::ace31799_acidcompoundbow, 0.07),
    (WeenieClassName::ace31802_firecompoundbow, 0.07),
    (WeenieClassName::ace31803_frostcompoundbow, 0.07),
    (WeenieClassName::ace31801_electriccompoundbow, 0.07),
]);

/// ACE `BowWcids_Sho.bowTiers` (`List<ChanceTable<WeenieClassName>>`).
pub static BOW_TIERS: [&ChanceTable<WeenieClassName>; 8] = [
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T5_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
];
