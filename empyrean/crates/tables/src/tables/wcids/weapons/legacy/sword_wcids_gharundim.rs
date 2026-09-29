// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/SwordWcids_Gharundim.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/SwordWcids_Gharundim.cs`; do not edit by hand

//! The literal data of ACE's `SwordWcids_Gharundim` (`Factories/Tables/Wcids/Weapons/Legacy/SwordWcids_Gharundim.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `SwordWcids_Gharundim.T1_T2_Chances` (`ChanceTable<WeenieClassName>`).
pub static T1_T2_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::swordrapier, 0.10),
    (WeenieClassName::simi, 0.12),
    (WeenieClassName::simiacid, 0.03),
    (WeenieClassName::simielectric, 0.03),
    (WeenieClassName::simifire, 0.03),
    (WeenieClassName::simifrost, 0.03),
    (WeenieClassName::kaskara, 0.10),
    (WeenieClassName::kaskaraacid, 0.03),
    (WeenieClassName::kaskaraelectric, 0.03),
    (WeenieClassName::kaskarafire, 0.03),
    (WeenieClassName::kaskarafrost, 0.03),
    (WeenieClassName::shamshir, 0.10),
    (WeenieClassName::shamshiracid, 0.03),
    (WeenieClassName::shamshirelectric, 0.03),
    (WeenieClassName::shamshirfire, 0.03),
    (WeenieClassName::shamshirfrost, 0.03),
    (WeenieClassName::takuba, 0.10),
    (WeenieClassName::takubaacid, 0.03),
    (WeenieClassName::takubaelectric, 0.03),
    (WeenieClassName::takubafire, 0.03),
    (WeenieClassName::takubafrost, 0.03),
]);

/// ACE `SwordWcids_Gharundim.T3_T4_Chances` (`ChanceTable<WeenieClassName>`).
pub static T3_T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::swordrapier, 0.06),
    (WeenieClassName::simi, 0.06),
    (WeenieClassName::simiacid, 0.01),
    (WeenieClassName::simielectric, 0.01),
    (WeenieClassName::simifire, 0.01),
    (WeenieClassName::simifrost, 0.01),
    (WeenieClassName::kaskara, 0.12),
    (WeenieClassName::kaskaraacid, 0.04),
    (WeenieClassName::kaskaraelectric, 0.04),
    (WeenieClassName::kaskarafire, 0.04),
    (WeenieClassName::kaskarafrost, 0.04),
    (WeenieClassName::shamshir, 0.12),
    (WeenieClassName::shamshiracid, 0.04),
    (WeenieClassName::shamshirelectric, 0.04),
    (WeenieClassName::shamshirfire, 0.04),
    (WeenieClassName::shamshirfrost, 0.04),
    (WeenieClassName::takuba, 0.12),
    (WeenieClassName::takubaacid, 0.04),
    (WeenieClassName::takubaelectric, 0.04),
    (WeenieClassName::takubafire, 0.04),
    (WeenieClassName::takubafrost, 0.04),
]);

/// ACE `SwordWcids_Gharundim.T5_T6_Chances` (`ChanceTable<WeenieClassName>`).
pub static T5_T6_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::swordrapier, 0.01),
    (WeenieClassName::simi, 0.01),
    (WeenieClassName::simiacid, 0.01),
    (WeenieClassName::simielectric, 0.01),
    (WeenieClassName::simifire, 0.01),
    (WeenieClassName::simifrost, 0.01),
    (WeenieClassName::kaskara, 0.14),
    (WeenieClassName::kaskaraacid, 0.04),
    (WeenieClassName::kaskaraelectric, 0.04),
    (WeenieClassName::kaskarafire, 0.04),
    (WeenieClassName::kaskarafrost, 0.04),
    (WeenieClassName::shamshir, 0.14),
    (WeenieClassName::shamshiracid, 0.04),
    (WeenieClassName::shamshirelectric, 0.04),
    (WeenieClassName::shamshirfire, 0.04),
    (WeenieClassName::shamshirfrost, 0.04),
    (WeenieClassName::takuba, 0.14),
    (WeenieClassName::takubaacid, 0.05),
    (WeenieClassName::takubaelectric, 0.05),
    (WeenieClassName::takubafire, 0.05),
    (WeenieClassName::takubafrost, 0.05),
]);

/// ACE `SwordWcids_Gharundim.weaponTiers` (`List<ChanceTable<WeenieClassName>>`).
pub static WEAPON_TIERS: [&ChanceTable<WeenieClassName>; 6] = [
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
    &T3_T4_CHANCES,
    &T3_T4_CHANCES,
    &T5_T6_CHANCES,
    &T5_T6_CHANCES,
];
