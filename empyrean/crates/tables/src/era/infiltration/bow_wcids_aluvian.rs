// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/BowWcids_Aluvian.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/BowWcids_Aluvian.cs`; do not edit by hand

//! The tables of ClassicACE's `BowWcids_Aluvian` under its Infiltration ruleset
//! (`Factories/Tables/Wcids/Weapons/BowWcids_Aluvian.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ClassicACE `BowWcids_Aluvian.T1_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::bowshort, 3.0),
    (WeenieClassName::bowlong, 1.0),
]);

/// ClassicACE `BowWcids_Aluvian.T1_T4_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::bowshort, 1.0),
    (WeenieClassName::bowlong, 1.0),
]);

/// ClassicACE `BowWcids_Aluvian.T5_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T5_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::bowlong, 4.0),
    (WeenieClassName::bowslashing, 1.0),
    (WeenieClassName::bowpiercing, 1.0),
    (WeenieClassName::bowblunt, 1.0),
    (WeenieClassName::bowacid, 1.0),
    (WeenieClassName::bowfire, 1.0),
    (WeenieClassName::bowfrost, 1.0),
    (WeenieClassName::bowelectric, 1.0),
]);

/// ClassicACE `BowWcids_Aluvian.T6_T8_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T6_T8_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::bowslashing, 1.0),
    (WeenieClassName::bowpiercing, 1.0),
    (WeenieClassName::bowblunt, 1.0),
    (WeenieClassName::bowacid, 1.0),
    (WeenieClassName::bowfire, 1.0),
    (WeenieClassName::bowfrost, 1.0),
    (WeenieClassName::bowelectric, 1.0),
]);

/// ClassicACE `BowWcids_Aluvian.bowTiers`, as its Infiltration ruleset sets it (`List<ChanceTable<WeenieClassName>>`).
pub static BOW_TIERS: [&ChanceTable<WeenieClassName>; 8] = [
    &T1_CHANCES,
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T5_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
];
