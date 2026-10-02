// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/SwordWcids_Gharundim.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/SwordWcids_Gharundim.cs`; do not edit by hand

//! The tables of ClassicACE's `SwordWcids_Gharundim` under its Infiltration ruleset
//! (`Factories/Tables/Wcids/Weapons/Legacy/SwordWcids_Gharundim.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ClassicACE `SwordWcids_Gharundim.T1_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::simi, 3.0),
    (WeenieClassName::kaskara, 0.5),
    (WeenieClassName::shamshir, 0.5),
    (WeenieClassName::takuba, 0.5),
    (WeenieClassName::swordrapier, 0.25),
]);

/// ClassicACE `SwordWcids_Gharundim.T1_T2_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_T2_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::simi, 4.0),
    (WeenieClassName::simiacid, 1.0),
    (WeenieClassName::simielectric, 1.0),
    (WeenieClassName::simifire, 1.0),
    (WeenieClassName::simifrost, 1.0),
    (WeenieClassName::kaskara, 4.0),
    (WeenieClassName::kaskaraacid, 1.0),
    (WeenieClassName::kaskaraelectric, 1.0),
    (WeenieClassName::kaskarafire, 1.0),
    (WeenieClassName::kaskarafrost, 1.0),
    (WeenieClassName::shamshir, 4.0),
    (WeenieClassName::shamshiracid, 1.0),
    (WeenieClassName::shamshirelectric, 1.0),
    (WeenieClassName::shamshirfire, 1.0),
    (WeenieClassName::shamshirfrost, 1.0),
    (WeenieClassName::takuba, 4.0),
    (WeenieClassName::takubaacid, 1.0),
    (WeenieClassName::takubaelectric, 1.0),
    (WeenieClassName::takubafire, 1.0),
    (WeenieClassName::takubafrost, 1.0),
    (WeenieClassName::swordrapier, 2.0),
]);

/// ClassicACE `SwordWcids_Gharundim.T3_T4_Chances` (`ChanceTable<WeenieClassName>`).
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

/// ClassicACE `SwordWcids_Gharundim.T5_T6_Chances` (`ChanceTable<WeenieClassName>`).
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

/// ClassicACE `SwordWcids_Gharundim.weaponTiers`, as its Infiltration ruleset sets it (`List<ChanceTable<WeenieClassName>>`).
pub static WEAPON_TIERS: [&ChanceTable<WeenieClassName>; 6] = [
    &T1_CHANCES,
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
];
