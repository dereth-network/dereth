// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/DaggerWcids_Aluvian_Sho.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/DaggerWcids_Aluvian_Sho.cs`; do not edit by hand

//! The literal data of ACE's `DaggerWcids_Aluvian_Sho` (`Factories/Tables/Wcids/Weapons/Legacy/DaggerWcids_Aluvian_Sho.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `DaggerWcids_Aluvian_Sho.T1_T3_Chances` (`ChanceTable<WeenieClassName>`).
pub static T1_T3_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::knife, 0.16),
    (WeenieClassName::knifeacid, 0.04),
    (WeenieClassName::knifeelectric, 0.04),
    (WeenieClassName::knifefire, 0.04),
    (WeenieClassName::knifefrost, 0.04),
    (WeenieClassName::dagger, 0.16),
    (WeenieClassName::daggeracid, 0.04),
    (WeenieClassName::daggerelectric, 0.04),
    (WeenieClassName::daggerfire, 0.04),
    (WeenieClassName::daggerfrost, 0.04),
    (WeenieClassName::dirk, 0.16),
    (WeenieClassName::dirkacid, 0.05),
    (WeenieClassName::dirkelectric, 0.05),
    (WeenieClassName::dirkfire, 0.05),
    (WeenieClassName::dirkfrost, 0.05),
]);

/// ACE `DaggerWcids_Aluvian_Sho.T4_Chances` (`ChanceTable<WeenieClassName>`).
pub static T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::knife, 0.06),
    (WeenieClassName::knifeacid, 0.01),
    (WeenieClassName::knifeelectric, 0.01),
    (WeenieClassName::knifefire, 0.01),
    (WeenieClassName::knifefrost, 0.01),
    (WeenieClassName::dagger, 0.06),
    (WeenieClassName::daggeracid, 0.01),
    (WeenieClassName::daggerelectric, 0.01),
    (WeenieClassName::daggerfire, 0.01),
    (WeenieClassName::daggerfrost, 0.01),
    (WeenieClassName::dirk, 0.40),
    (WeenieClassName::dirkacid, 0.10),
    (WeenieClassName::dirkelectric, 0.10),
    (WeenieClassName::dirkfire, 0.10),
    (WeenieClassName::dirkfrost, 0.10),
]);

/// ACE `DaggerWcids_Aluvian_Sho.T5_T6_Chances` (`ChanceTable<WeenieClassName>`).
pub static T5_T6_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::knife, 0.01),
    (WeenieClassName::knifeacid, 0.01),
    (WeenieClassName::knifeelectric, 0.01),
    (WeenieClassName::knifefire, 0.01),
    (WeenieClassName::knifefrost, 0.01),
    (WeenieClassName::dagger, 0.01),
    (WeenieClassName::daggeracid, 0.01),
    (WeenieClassName::daggerelectric, 0.01),
    (WeenieClassName::daggerfire, 0.01),
    (WeenieClassName::daggerfrost, 0.01),
    (WeenieClassName::dirk, 0.42),
    (WeenieClassName::dirkacid, 0.12),
    (WeenieClassName::dirkelectric, 0.12),
    (WeenieClassName::dirkfire, 0.12),
    (WeenieClassName::dirkfrost, 0.12),
]);

/// ACE `DaggerWcids_Aluvian_Sho.weaponTiers` (`List<ChanceTable<WeenieClassName>>`).
pub static WEAPON_TIERS: [&ChanceTable<WeenieClassName>; 6] = [
    &T1_T3_CHANCES,
    &T1_T3_CHANCES,
    &T1_T3_CHANCES,
    &T4_CHANCES,
    &T5_T6_CHANCES,
    &T5_T6_CHANCES,
];
