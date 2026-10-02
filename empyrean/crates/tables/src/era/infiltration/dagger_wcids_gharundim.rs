// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/DaggerWcids_Gharundim.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/DaggerWcids_Gharundim.cs`; do not edit by hand

//! The tables of ClassicACE's `DaggerWcids_Gharundim` under its Infiltration ruleset
//! (`Factories/Tables/Wcids/Weapons/Legacy/DaggerWcids_Gharundim.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ClassicACE `DaggerWcids_Gharundim.T1_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::jambiya, 3.0),
    (WeenieClassName::khanjar, 3.0),
    (WeenieClassName::dirk, 0.5),
]);

/// ClassicACE `DaggerWcids_Gharundim.T1_T3_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_T3_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::jambiya, 4.0),
    (WeenieClassName::jambiyaacid, 1.0),
    (WeenieClassName::jambiyaelectric, 1.0),
    (WeenieClassName::jambiyafire, 1.0),
    (WeenieClassName::jambiyafrost, 1.0),
    (WeenieClassName::khanjar, 4.0),
    (WeenieClassName::khanjaracid, 1.0),
    (WeenieClassName::khanjarelectric, 1.0),
    (WeenieClassName::khanjarfire, 1.0),
    (WeenieClassName::khanjarfrost, 1.0),
    (WeenieClassName::dirk, 4.0),
    (WeenieClassName::dirkacid, 1.0),
    (WeenieClassName::dirkelectric, 1.0),
    (WeenieClassName::dirkfire, 1.0),
    (WeenieClassName::dirkfrost, 1.0),
]);

/// ClassicACE `DaggerWcids_Gharundim.T4_Chances` (`ChanceTable<WeenieClassName>`).
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

/// ClassicACE `DaggerWcids_Gharundim.T5_T6_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T5_T6_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::jambiya, 1.0),
    (WeenieClassName::jambiyaacid, 0.25),
    (WeenieClassName::jambiyaelectric, 0.25),
    (WeenieClassName::jambiyafire, 0.25),
    (WeenieClassName::jambiyafrost, 0.25),
    (WeenieClassName::khanjar, 1.0),
    (WeenieClassName::khanjaracid, 0.25),
    (WeenieClassName::khanjarelectric, 0.25),
    (WeenieClassName::khanjarfire, 0.25),
    (WeenieClassName::khanjarfrost, 0.25),
    (WeenieClassName::dirk, 4.0),
    (WeenieClassName::dirkacid, 1.0),
    (WeenieClassName::dirkelectric, 1.0),
    (WeenieClassName::dirkfire, 1.0),
    (WeenieClassName::dirkfrost, 1.0),
]);

/// ClassicACE `DaggerWcids_Gharundim.weaponTiers`, as its Infiltration ruleset sets it (`List<ChanceTable<WeenieClassName>>`).
pub static WEAPON_TIERS: [&ChanceTable<WeenieClassName>; 6] = [
    &T1_CHANCES,
    &T1_T3_CHANCES,
    &T1_T3_CHANCES,
    &T1_T3_CHANCES,
    &T5_T6_CHANCES,
    &T5_T6_CHANCES,
];
