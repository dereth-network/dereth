// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/SwordWcids_Sho.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/SwordWcids_Sho.cs`; do not edit by hand

//! The literal data of ACE's `SwordWcids_Sho` (`Factories/Tables/Wcids/Weapons/Legacy/SwordWcids_Sho.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `SwordWcids_Sho.T1_T2_Chances` (`ChanceTable<WeenieClassName>`).
pub static T1_T2_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::swordrapier, 0.10),
    (WeenieClassName::yaoji, 0.12),
    (WeenieClassName::yaojiacid, 0.03),
    (WeenieClassName::yaojielectric, 0.03),
    (WeenieClassName::yaojifire, 0.03),
    (WeenieClassName::yaojifrost, 0.03),
    (WeenieClassName::scimitar, 0.10),
    (WeenieClassName::scimitaracid, 0.03),
    (WeenieClassName::scimitarelectric, 0.03),
    (WeenieClassName::scimitarfire, 0.03),
    (WeenieClassName::scimitarfrost, 0.03),
    (WeenieClassName::ken, 0.10),
    (WeenieClassName::kenacid, 0.03),
    (WeenieClassName::kenelectric, 0.03),
    (WeenieClassName::kenfire, 0.03),
    (WeenieClassName::kenfrost, 0.03),
    (WeenieClassName::tachi, 0.10),
    (WeenieClassName::tachiacid, 0.03),
    (WeenieClassName::tachielectric, 0.03),
    (WeenieClassName::tachifire, 0.03),
    (WeenieClassName::tachifrost, 0.03),
]);

/// ACE `SwordWcids_Sho.T3_T4_Chances` (`ChanceTable<WeenieClassName>`).
pub static T3_T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::swordrapier, 0.06),
    (WeenieClassName::yaoji, 0.06),
    (WeenieClassName::yaojiacid, 0.01),
    (WeenieClassName::yaojielectric, 0.01),
    (WeenieClassName::yaojifire, 0.01),
    (WeenieClassName::yaojifrost, 0.01),
    (WeenieClassName::scimitar, 0.12),
    (WeenieClassName::scimitaracid, 0.04),
    (WeenieClassName::scimitarelectric, 0.04),
    (WeenieClassName::scimitarfire, 0.04),
    (WeenieClassName::scimitarfrost, 0.04),
    (WeenieClassName::ken, 0.12),
    (WeenieClassName::kenacid, 0.04),
    (WeenieClassName::kenelectric, 0.04),
    (WeenieClassName::kenfire, 0.04),
    (WeenieClassName::kenfrost, 0.04),
    (WeenieClassName::tachi, 0.12),
    (WeenieClassName::tachiacid, 0.04),
    (WeenieClassName::tachielectric, 0.04),
    (WeenieClassName::tachifire, 0.04),
    (WeenieClassName::tachifrost, 0.04),
]);

/// ACE `SwordWcids_Sho.T5_T6_Chances` (`ChanceTable<WeenieClassName>`).
pub static T5_T6_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::swordrapier, 0.01),
    (WeenieClassName::yaoji, 0.01),
    (WeenieClassName::yaojiacid, 0.01),
    (WeenieClassName::yaojielectric, 0.01),
    (WeenieClassName::yaojifire, 0.01),
    (WeenieClassName::yaojifrost, 0.01),
    (WeenieClassName::scimitar, 0.14),
    (WeenieClassName::scimitaracid, 0.04),
    (WeenieClassName::scimitarelectric, 0.04),
    (WeenieClassName::scimitarfire, 0.04),
    (WeenieClassName::scimitarfrost, 0.04),
    (WeenieClassName::ken, 0.14),
    (WeenieClassName::kenacid, 0.04),
    (WeenieClassName::kenelectric, 0.04),
    (WeenieClassName::kenfire, 0.04),
    (WeenieClassName::kenfrost, 0.04),
    (WeenieClassName::tachi, 0.14),
    (WeenieClassName::tachiacid, 0.05),
    (WeenieClassName::tachielectric, 0.05),
    (WeenieClassName::tachifire, 0.05),
    (WeenieClassName::tachifrost, 0.05),
]);

/// ACE `SwordWcids_Sho.weaponTiers` (`List<ChanceTable<WeenieClassName>>`).
pub static WEAPON_TIERS: [&ChanceTable<WeenieClassName>; 6] = [
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
    &T3_T4_CHANCES,
    &T3_T4_CHANCES,
    &T5_T6_CHANCES,
    &T5_T6_CHANCES,
];
