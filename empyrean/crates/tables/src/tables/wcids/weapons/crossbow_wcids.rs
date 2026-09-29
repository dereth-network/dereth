// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/CrossbowWcids.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/CrossbowWcids.cs`; do not edit by hand

//! The literal data of ACE's `CrossbowWcids` (`Factories/Tables/Wcids/Weapons/CrossbowWcids.cs`).
//!
//! Runtime state, hand-ported in `crate::logic` (not literal here):
//! - `_combined` (`Dictionary<WeenieClassName, TreasureWeaponType>`)

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `CrossbowWcids.T1_T4_Chances` (`ChanceTable<WeenieClassName>`).
pub static T1_T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::crossbowlight, 0.50),
    (WeenieClassName::crossbowheavy, 0.25),
    (WeenieClassName::crossbowarbalest, 0.25),
]);

/// ACE `CrossbowWcids.T5_Chances` (`ChanceTable<WeenieClassName>`).
pub static T5_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::crossbowlight, 0.25),
    (WeenieClassName::crossbowheavy, 0.13),
    (WeenieClassName::crossbowarbalest, 0.13),
    (WeenieClassName::crossbowslashing, 0.035),
    (WeenieClassName::crossbowpiercing, 0.035),
    (WeenieClassName::crossbowblunt, 0.035),
    (WeenieClassName::crossbowacid, 0.035),
    (WeenieClassName::crossbowfire, 0.035),
    (WeenieClassName::crossbowfrost, 0.035),
    (WeenieClassName::crossbowelectric, 0.035),
    (WeenieClassName::ace31805_slashingcompoundcrossbow, 0.035),
    (WeenieClassName::ace31811_piercingcompoundcrossbow, 0.035),
    (WeenieClassName::ace31807_bluntcompoundcrossbow, 0.035),
    (WeenieClassName::ace31806_acidcompoundcrossbow, 0.035),
    (WeenieClassName::ace31809_firecompoundcrossbow, 0.035),
    (WeenieClassName::ace31810_frostcompoundcrossbow, 0.035),
    (WeenieClassName::ace31808_electriccompoundcrossbow, 0.035),
]);

/// ACE `CrossbowWcids.T6_T8_Chances` (`ChanceTable<WeenieClassName>`).
pub static T6_T8_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::crossbowslashing, 0.075),
    (WeenieClassName::crossbowpiercing, 0.075),
    (WeenieClassName::crossbowblunt, 0.07),
    (WeenieClassName::crossbowacid, 0.07),
    (WeenieClassName::crossbowfire, 0.07),
    (WeenieClassName::crossbowfrost, 0.07),
    (WeenieClassName::crossbowelectric, 0.07),
    (WeenieClassName::ace31805_slashingcompoundcrossbow, 0.075),
    (WeenieClassName::ace31811_piercingcompoundcrossbow, 0.075),
    (WeenieClassName::ace31807_bluntcompoundcrossbow, 0.07),
    (WeenieClassName::ace31806_acidcompoundcrossbow, 0.07),
    (WeenieClassName::ace31809_firecompoundcrossbow, 0.07),
    (WeenieClassName::ace31810_frostcompoundcrossbow, 0.07),
    (WeenieClassName::ace31808_electriccompoundcrossbow, 0.07),
]);

/// ACE `CrossbowWcids.crossbowTiers` (`List<ChanceTable<WeenieClassName>>`).
pub static CROSSBOW_TIERS: [&ChanceTable<WeenieClassName>; 8] = [
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T5_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
];
