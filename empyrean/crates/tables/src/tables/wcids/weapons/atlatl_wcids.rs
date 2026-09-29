// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/AtlatlWcids.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/AtlatlWcids.cs`; do not edit by hand

//! The literal data of ACE's `AtlatlWcids` (`Factories/Tables/Wcids/Weapons/AtlatlWcids.cs`).
//!
//! Runtime state, hand-ported in `crate::logic` (not literal here):
//! - `_combined` (`Dictionary<WeenieClassName, TreasureWeaponType>`)

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `AtlatlWcids.T1_T4_Chances` (`ChanceTable<WeenieClassName>`).
pub static T1_T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::atlatl, 0.50),
    (WeenieClassName::atlatlroyal, 0.50),
]);

/// ACE `AtlatlWcids.T5_Chances` (`ChanceTable<WeenieClassName>`).
pub static T5_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::atlatl, 0.25),
    (WeenieClassName::atlatlroyal, 0.26),
    (WeenieClassName::atlatlslashing, 0.035),
    (WeenieClassName::atlatlpiercing, 0.035),
    (WeenieClassName::atlatlblunt, 0.035),
    (WeenieClassName::atlatlacid, 0.035),
    (WeenieClassName::atlatlfire, 0.035),
    (WeenieClassName::atlatlfrost, 0.035),
    (WeenieClassName::atlatlelectric, 0.035),
    (WeenieClassName::ace31812_slashingslingshot, 0.035),
    (WeenieClassName::ace31818_piercingslingshot, 0.035),
    (WeenieClassName::ace31814_bluntslingshot, 0.035),
    (WeenieClassName::ace31813_acidslingshot, 0.035),
    (WeenieClassName::ace31816_fireslingshot, 0.035),
    (WeenieClassName::ace31817_frostslingshot, 0.035),
    (WeenieClassName::ace31815_electricslingshot, 0.035),
]);

/// ACE `AtlatlWcids.T6_T8_Chances` (`ChanceTable<WeenieClassName>`).
pub static T6_T8_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::atlatlslashing, 0.075),
    (WeenieClassName::atlatlpiercing, 0.075),
    (WeenieClassName::atlatlblunt, 0.07),
    (WeenieClassName::atlatlacid, 0.07),
    (WeenieClassName::atlatlfire, 0.07),
    (WeenieClassName::atlatlfrost, 0.07),
    (WeenieClassName::atlatlelectric, 0.07),
    (WeenieClassName::ace31812_slashingslingshot, 0.075),
    (WeenieClassName::ace31818_piercingslingshot, 0.075),
    (WeenieClassName::ace31814_bluntslingshot, 0.07),
    (WeenieClassName::ace31813_acidslingshot, 0.07),
    (WeenieClassName::ace31816_fireslingshot, 0.07),
    (WeenieClassName::ace31817_frostslingshot, 0.07),
    (WeenieClassName::ace31815_electricslingshot, 0.07),
]);

/// ACE `AtlatlWcids.atlatlTiers` (`List<ChanceTable<WeenieClassName>>`).
pub static ATLATL_TIERS: [&ChanceTable<WeenieClassName>; 8] = [
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T5_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
];
