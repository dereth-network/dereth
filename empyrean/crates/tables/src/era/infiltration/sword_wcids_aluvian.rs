// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/SwordWcids_Aluvian.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/SwordWcids_Aluvian.cs`; do not edit by hand

//! The tables of ClassicACE's `SwordWcids_Aluvian` under its Infiltration ruleset
//! (`Factories/Tables/Wcids/Weapons/Legacy/SwordWcids_Aluvian.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ClassicACE `SwordWcids_Aluvian.T1_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::swordshort, 3.0),
    (WeenieClassName::scimitar, 0.5),
    (WeenieClassName::swordlong, 0.5),
    (WeenieClassName::swordbroad, 0.5),
    (WeenieClassName::swordrapier, 0.25),
]);

/// ClassicACE `SwordWcids_Aluvian.T1_T2_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_T2_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::swordshort, 4.0),
    (WeenieClassName::swordshortacid, 1.0),
    (WeenieClassName::swordshortelectric, 1.0),
    (WeenieClassName::swordshortfire, 1.0),
    (WeenieClassName::swordshortfrost, 1.0),
    (WeenieClassName::scimitar, 4.0),
    (WeenieClassName::scimitaracid, 1.0),
    (WeenieClassName::scimitarelectric, 1.0),
    (WeenieClassName::scimitarfire, 1.0),
    (WeenieClassName::scimitarfrost, 1.0),
    (WeenieClassName::swordlong, 4.0),
    (WeenieClassName::swordlongacid, 1.0),
    (WeenieClassName::swordlongelectric, 1.0),
    (WeenieClassName::swordlongfire, 1.0),
    (WeenieClassName::swordlongfrost, 1.0),
    (WeenieClassName::swordbroad, 4.0),
    (WeenieClassName::swordbroadacid, 1.0),
    (WeenieClassName::swordbroadelectric, 1.0),
    (WeenieClassName::swordbroadfire, 1.0),
    (WeenieClassName::swordbroadfrost, 1.0),
    (WeenieClassName::swordrapier, 2.0),
]);

/// ClassicACE `SwordWcids_Aluvian.T3_T4_Chances` (`ChanceTable<WeenieClassName>`).
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

/// ClassicACE `SwordWcids_Aluvian.T5_T6_Chances` (`ChanceTable<WeenieClassName>`).
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

/// ClassicACE `SwordWcids_Aluvian.weaponTiers`, as its Infiltration ruleset sets it (`List<ChanceTable<WeenieClassName>>`).
pub static WEAPON_TIERS: [&ChanceTable<WeenieClassName>; 6] = [
    &T1_CHANCES,
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
];
