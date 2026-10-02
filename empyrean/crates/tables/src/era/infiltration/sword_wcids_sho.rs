// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/SwordWcids_Sho.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/SwordWcids_Sho.cs`; do not edit by hand

//! The tables of ClassicACE's `SwordWcids_Sho` under its Infiltration ruleset
//! (`Factories/Tables/Wcids/Weapons/Legacy/SwordWcids_Sho.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ClassicACE `SwordWcids_Sho.T1_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::yaoji, 3.0),
    (WeenieClassName::scimitar, 0.5),
    (WeenieClassName::ken, 0.5),
    (WeenieClassName::tachi, 0.5),
    (WeenieClassName::swordrapier, 0.25),
]);

/// ClassicACE `SwordWcids_Sho.T1_T2_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_T2_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::yaoji, 4.0),
    (WeenieClassName::yaojiacid, 1.0),
    (WeenieClassName::yaojielectric, 1.0),
    (WeenieClassName::yaojifire, 1.0),
    (WeenieClassName::yaojifrost, 1.0),
    (WeenieClassName::scimitar, 4.0),
    (WeenieClassName::scimitaracid, 1.0),
    (WeenieClassName::scimitarelectric, 1.0),
    (WeenieClassName::scimitarfire, 1.0),
    (WeenieClassName::scimitarfrost, 1.0),
    (WeenieClassName::ken, 4.0),
    (WeenieClassName::kenacid, 1.0),
    (WeenieClassName::kenelectric, 1.0),
    (WeenieClassName::kenfire, 1.0),
    (WeenieClassName::kenfrost, 1.0),
    (WeenieClassName::tachi, 4.0),
    (WeenieClassName::tachiacid, 1.0),
    (WeenieClassName::tachielectric, 1.0),
    (WeenieClassName::tachifire, 1.0),
    (WeenieClassName::tachifrost, 1.0),
    (WeenieClassName::swordrapier, 2.0),
]);

/// ClassicACE `SwordWcids_Sho.T3_T4_Chances` (`ChanceTable<WeenieClassName>`).
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

/// ClassicACE `SwordWcids_Sho.T5_T6_Chances` (`ChanceTable<WeenieClassName>`).
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

/// ClassicACE `SwordWcids_Sho.weaponTiers`, as its Infiltration ruleset sets it (`List<ChanceTable<WeenieClassName>>`).
pub static WEAPON_TIERS: [&ChanceTable<WeenieClassName>; 6] = [
    &T1_CHANCES,
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
];
