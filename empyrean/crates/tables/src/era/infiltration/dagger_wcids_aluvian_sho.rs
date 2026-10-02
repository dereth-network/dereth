// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/DaggerWcids_Aluvian_Sho.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/DaggerWcids_Aluvian_Sho.cs`; do not edit by hand

//! The tables of ClassicACE's `DaggerWcids_Aluvian_Sho` under its Infiltration ruleset
//! (`Factories/Tables/Wcids/Weapons/Legacy/DaggerWcids_Aluvian_Sho.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ClassicACE `DaggerWcids_Aluvian_Sho.T1_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::knife, 3.0),
    (WeenieClassName::dagger, 3.0),
    (WeenieClassName::dirk, 0.5),
]);

/// ClassicACE `DaggerWcids_Aluvian_Sho.T1_T3_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_T3_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::knife, 4.0),
    (WeenieClassName::knifeacid, 1.0),
    (WeenieClassName::knifeelectric, 1.0),
    (WeenieClassName::knifefire, 1.0),
    (WeenieClassName::knifefrost, 1.0),
    (WeenieClassName::dagger, 4.0),
    (WeenieClassName::daggeracid, 1.0),
    (WeenieClassName::daggerelectric, 1.0),
    (WeenieClassName::daggerfire, 1.0),
    (WeenieClassName::daggerfrost, 1.0),
    (WeenieClassName::dirk, 4.0),
    (WeenieClassName::dirkacid, 1.0),
    (WeenieClassName::dirkelectric, 1.0),
    (WeenieClassName::dirkfire, 1.0),
    (WeenieClassName::dirkfrost, 1.0),
]);

/// ClassicACE `DaggerWcids_Aluvian_Sho.T4_Chances` (`ChanceTable<WeenieClassName>`).
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

/// ClassicACE `DaggerWcids_Aluvian_Sho.T5_T6_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T5_T6_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::knife, 1.0),
    (WeenieClassName::knifeacid, 0.25),
    (WeenieClassName::knifeelectric, 0.25),
    (WeenieClassName::knifefire, 0.25),
    (WeenieClassName::knifefrost, 0.25),
    (WeenieClassName::dagger, 1.0),
    (WeenieClassName::daggeracid, 0.25),
    (WeenieClassName::daggerelectric, 0.25),
    (WeenieClassName::daggerfire, 0.25),
    (WeenieClassName::daggerfrost, 0.25),
    (WeenieClassName::dirk, 4.0),
    (WeenieClassName::dirkacid, 1.0),
    (WeenieClassName::dirkelectric, 1.0),
    (WeenieClassName::dirkfire, 1.0),
    (WeenieClassName::dirkfrost, 1.0),
]);

/// ClassicACE `DaggerWcids_Aluvian_Sho.weaponTiers`, as its Infiltration ruleset sets it (`List<ChanceTable<WeenieClassName>>`).
pub static WEAPON_TIERS: [&ChanceTable<WeenieClassName>; 6] = [
    &T1_CHANCES,
    &T1_T3_CHANCES,
    &T1_T3_CHANCES,
    &T1_T3_CHANCES,
    &T5_T6_CHANCES,
    &T5_T6_CHANCES,
];
