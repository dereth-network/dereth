// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/DaggerWcids_Gharundim.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/DaggerWcids_Gharundim.cs`; do not edit by hand

//! The literal data of ACE's `DaggerWcids_Gharundim` (`Factories/Tables/Wcids/Weapons/Legacy/DaggerWcids_Gharundim.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `DaggerWcids_Gharundim.T1_T3_Chances` (`ChanceTable<WeenieClassName>`).
pub static T1_T3_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::jambiya, 0.16),
    (WeenieClassName::jambiyaacid, 0.04),
    (WeenieClassName::jambiyaelectric, 0.04),
    (WeenieClassName::jambiyafire, 0.04),
    (WeenieClassName::jambiyafrost, 0.04),
    (WeenieClassName::khanjar, 0.16),
    (WeenieClassName::khanjaracid, 0.04),
    (WeenieClassName::khanjarelectric, 0.04),
    (WeenieClassName::khanjarfire, 0.04),
    (WeenieClassName::khanjarfrost, 0.04),
    (WeenieClassName::dirk, 0.16),
    (WeenieClassName::dirkacid, 0.05),
    (WeenieClassName::dirkelectric, 0.05),
    (WeenieClassName::dirkfire, 0.05),
    (WeenieClassName::dirkfrost, 0.05),
]);

/// ACE `DaggerWcids_Gharundim.T4_Chances` (`ChanceTable<WeenieClassName>`).
pub static T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::jambiya, 0.06),
    (WeenieClassName::jambiyaacid, 0.01),
    (WeenieClassName::jambiyaelectric, 0.01),
    (WeenieClassName::jambiyafire, 0.01),
    (WeenieClassName::jambiyafrost, 0.01),
    (WeenieClassName::khanjar, 0.06),
    (WeenieClassName::khanjaracid, 0.01),
    (WeenieClassName::khanjarelectric, 0.01),
    (WeenieClassName::khanjarfire, 0.01),
    (WeenieClassName::khanjarfrost, 0.01),
    (WeenieClassName::dirk, 0.40),
    (WeenieClassName::dirkacid, 0.10),
    (WeenieClassName::dirkelectric, 0.10),
    (WeenieClassName::dirkfire, 0.10),
    (WeenieClassName::dirkfrost, 0.10),
]);

/// ACE `DaggerWcids_Gharundim.T5_T6_Chances` (`ChanceTable<WeenieClassName>`).
pub static T5_T6_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::jambiya, 0.01),
    (WeenieClassName::jambiyaacid, 0.01),
    (WeenieClassName::jambiyaelectric, 0.01),
    (WeenieClassName::jambiyafire, 0.01),
    (WeenieClassName::jambiyafrost, 0.01),
    (WeenieClassName::khanjar, 0.01),
    (WeenieClassName::khanjaracid, 0.01),
    (WeenieClassName::khanjarelectric, 0.01),
    (WeenieClassName::khanjarfire, 0.01),
    (WeenieClassName::khanjarfrost, 0.01),
    (WeenieClassName::dirk, 0.42),
    (WeenieClassName::dirkacid, 0.12),
    (WeenieClassName::dirkelectric, 0.12),
    (WeenieClassName::dirkfire, 0.12),
    (WeenieClassName::dirkfrost, 0.12),
]);

/// ACE `DaggerWcids_Gharundim.weaponTiers` (`List<ChanceTable<WeenieClassName>>`).
pub static WEAPON_TIERS: [&ChanceTable<WeenieClassName>; 6] = [
    &T1_T3_CHANCES,
    &T1_T3_CHANCES,
    &T1_T3_CHANCES,
    &T4_CHANCES,
    &T5_T6_CHANCES,
    &T5_T6_CHANCES,
];
