// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/JewelryWcids.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/JewelryWcids.cs`; do not edit by hand

//! The literal data of ACE's `JewelryWcids` (`Factories/Tables/Wcids/JewelryWcids.cs`).
//!
//! Runtime state, hand-ported in `crate::logic` (not literal here):
//! - `_combined` (`HashSet<WeenieClassName>`)

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `JewelryWcids.T1_T2_Chances` (`ChanceTable<WeenieClassName>`).
pub static T1_T2_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::amulet, 0.085),
    (WeenieClassName::bracelet, 0.255),
    (WeenieClassName::braceletheavy, 0.085),
    (WeenieClassName::necklace, 0.17),
    (WeenieClassName::ring, 0.2295),
    (WeenieClassName::ringjeweled, 0.0255),
    (WeenieClassName::ace41483_compass, 0.025),
    (WeenieClassName::ace41484_goggles, 0.025),
    (WeenieClassName::ace41487_mechanicalscarab, 0.025),
    (WeenieClassName::ace41486_puzzlebox, 0.025),
    (WeenieClassName::ace41485_pocketwatch, 0.025),
    (WeenieClassName::ace41488_top, 0.025),
]);

/// ACE `JewelryWcids.T3_T4_Chances` (`ChanceTable<WeenieClassName>`).
pub static T3_T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::amulet, 0.085),
    (WeenieClassName::bracelet, 0.1275),
    (WeenieClassName::braceletheavy, 0.1275),
    (WeenieClassName::gorget, 0.085),
    (WeenieClassName::necklace, 0.1275),
    (WeenieClassName::necklaceheavy, 0.0425),
    (WeenieClassName::ring, 0.1275),
    (WeenieClassName::ringjeweled, 0.1275),
    (WeenieClassName::ace41483_compass, 0.025),
    (WeenieClassName::ace41484_goggles, 0.025),
    (WeenieClassName::ace41487_mechanicalscarab, 0.025),
    (WeenieClassName::ace41486_puzzlebox, 0.025),
    (WeenieClassName::ace41485_pocketwatch, 0.025),
    (WeenieClassName::ace41488_top, 0.025),
]);

/// ACE `JewelryWcids.T5_T6_Chances` (`ChanceTable<WeenieClassName>`).
pub static T5_T6_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::amulet, 0.0425),
    (WeenieClassName::bracelet, 0.0425),
    (WeenieClassName::braceletheavy, 0.17),
    (WeenieClassName::crown, 0.015),
    (WeenieClassName::ace31864_teardropcrown, 0.014),
    (WeenieClassName::ace31865_circlet, 0.014),
    (WeenieClassName::ace31866_coronet, 0.014),
    (WeenieClassName::ace31867_diadem, 0.014),
    (WeenieClassName::ace31868_signetcrown, 0.014),
    (WeenieClassName::gorget, 0.085),
    (WeenieClassName::necklace, 0.0425),
    (WeenieClassName::necklaceheavy, 0.1275),
    (WeenieClassName::ring, 0.051),
    (WeenieClassName::ringjeweled, 0.204),
    (WeenieClassName::ace41483_compass, 0.025),
    (WeenieClassName::ace41484_goggles, 0.025),
    (WeenieClassName::ace41487_mechanicalscarab, 0.025),
    (WeenieClassName::ace41486_puzzlebox, 0.025),
    (WeenieClassName::ace41485_pocketwatch, 0.025),
    (WeenieClassName::ace41488_top, 0.025),
]);

/// ACE `JewelryWcids.tierChances` (`List<ChanceTable<WeenieClassName>>`).
pub static TIER_CHANCES: [&ChanceTable<WeenieClassName>; 6] = [
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
    &T3_T4_CHANCES,
    &T3_T4_CHANCES,
    &T5_T6_CHANCES,
    &T5_T6_CHANCES,
];
