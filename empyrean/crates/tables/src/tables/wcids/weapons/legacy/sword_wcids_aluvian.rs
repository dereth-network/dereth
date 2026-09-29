// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/SwordWcids_Aluvian.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/SwordWcids_Aluvian.cs`; do not edit by hand

//! The literal data of ACE's `SwordWcids_Aluvian` (`Factories/Tables/Wcids/Weapons/Legacy/SwordWcids_Aluvian.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `SwordWcids_Aluvian.T1_T2_Chances` (`ChanceTable<WeenieClassName>`).
pub static T1_T2_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::swordrapier, 0.10),
    (WeenieClassName::swordshort, 0.12),
    (WeenieClassName::swordshortacid, 0.03),
    (WeenieClassName::swordshortelectric, 0.03),
    (WeenieClassName::swordshortfire, 0.03),
    (WeenieClassName::swordshortfrost, 0.03),
    (WeenieClassName::scimitar, 0.10),
    (WeenieClassName::scimitaracid, 0.03),
    (WeenieClassName::scimitarelectric, 0.03),
    (WeenieClassName::scimitarfire, 0.03),
    (WeenieClassName::scimitarfrost, 0.03),
    (WeenieClassName::swordlong, 0.10),
    (WeenieClassName::swordlongacid, 0.03),
    (WeenieClassName::swordlongelectric, 0.03),
    (WeenieClassName::swordlongfire, 0.03),
    (WeenieClassName::swordlongfrost, 0.03),
    (WeenieClassName::swordbroad, 0.10),
    (WeenieClassName::swordbroadacid, 0.03),
    (WeenieClassName::swordbroadelectric, 0.03),
    (WeenieClassName::swordbroadfire, 0.03),
    (WeenieClassName::swordbroadfrost, 0.03),
]);

/// ACE `SwordWcids_Aluvian.T3_T4_Chances` (`ChanceTable<WeenieClassName>`).
pub static T3_T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::swordrapier, 0.06),
    (WeenieClassName::swordshort, 0.06),
    (WeenieClassName::swordshortacid, 0.01),
    (WeenieClassName::swordshortelectric, 0.01),
    (WeenieClassName::swordshortfire, 0.01),
    (WeenieClassName::swordshortfrost, 0.01),
    (WeenieClassName::scimitar, 0.12),
    (WeenieClassName::scimitaracid, 0.04),
    (WeenieClassName::scimitarelectric, 0.04),
    (WeenieClassName::scimitarfire, 0.04),
    (WeenieClassName::scimitarfrost, 0.04),
    (WeenieClassName::swordlong, 0.12),
    (WeenieClassName::swordlongacid, 0.04),
    (WeenieClassName::swordlongelectric, 0.04),
    (WeenieClassName::swordlongfire, 0.04),
    (WeenieClassName::swordlongfrost, 0.04),
    (WeenieClassName::swordbroad, 0.12),
    (WeenieClassName::swordbroadacid, 0.04),
    (WeenieClassName::swordbroadelectric, 0.04),
    (WeenieClassName::swordbroadfire, 0.04),
    (WeenieClassName::swordbroadfrost, 0.04),
]);

/// ACE `SwordWcids_Aluvian.T5_T6_Chances` (`ChanceTable<WeenieClassName>`).
pub static T5_T6_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::swordrapier, 0.01),
    (WeenieClassName::swordshort, 0.01),
    (WeenieClassName::swordshortacid, 0.01),
    (WeenieClassName::swordshortelectric, 0.01),
    (WeenieClassName::swordshortfire, 0.01),
    (WeenieClassName::swordshortfrost, 0.01),
    (WeenieClassName::scimitar, 0.14),
    (WeenieClassName::scimitaracid, 0.04),
    (WeenieClassName::scimitarelectric, 0.04),
    (WeenieClassName::scimitarfire, 0.04),
    (WeenieClassName::scimitarfrost, 0.04),
    (WeenieClassName::swordlong, 0.14),
    (WeenieClassName::swordlongacid, 0.04),
    (WeenieClassName::swordlongelectric, 0.04),
    (WeenieClassName::swordlongfire, 0.04),
    (WeenieClassName::swordlongfrost, 0.04),
    (WeenieClassName::swordbroad, 0.14),
    (WeenieClassName::swordbroadacid, 0.05),
    (WeenieClassName::swordbroadelectric, 0.05),
    (WeenieClassName::swordbroadfire, 0.05),
    (WeenieClassName::swordbroadfrost, 0.05),
]);

/// ACE `SwordWcids_Aluvian.weaponTiers` (`List<ChanceTable<WeenieClassName>>`).
pub static WEAPON_TIERS: [&ChanceTable<WeenieClassName>; 6] = [
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
    &T3_T4_CHANCES,
    &T3_T4_CHANCES,
    &T5_T6_CHANCES,
    &T5_T6_CHANCES,
];
