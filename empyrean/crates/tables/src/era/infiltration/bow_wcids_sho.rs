// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/BowWcids_Sho.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/BowWcids_Sho.cs`; do not edit by hand

//! The tables of ClassicACE's `BowWcids_Sho` under its Infiltration ruleset
//! (`Factories/Tables/Wcids/Weapons/BowWcids_Sho.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ClassicACE `BowWcids_Sho.T1_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::shouyumi, 3.0),
    (WeenieClassName::yumi, 1.0),
]);

/// ClassicACE `BowWcids_Sho.T1_T4_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::shouyumi, 1.0),
    (WeenieClassName::yumi, 1.0),
]);

/// ClassicACE `BowWcids_Sho.T5_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T5_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::yumi, 4.0),
    (WeenieClassName::bowslashing, 1.0),
    (WeenieClassName::bowpiercing, 1.0),
    (WeenieClassName::bowblunt, 1.0),
    (WeenieClassName::bowacid, 1.0),
    (WeenieClassName::bowfire, 1.0),
    (WeenieClassName::bowfrost, 1.0),
    (WeenieClassName::bowelectric, 1.0),
]);

/// ClassicACE `BowWcids_Sho.T6_T8_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T6_T8_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::bowslashing, 1.0),
    (WeenieClassName::bowpiercing, 1.0),
    (WeenieClassName::bowblunt, 1.0),
    (WeenieClassName::bowacid, 1.0),
    (WeenieClassName::bowfire, 1.0),
    (WeenieClassName::bowfrost, 1.0),
    (WeenieClassName::bowelectric, 1.0),
]);

/// ClassicACE `BowWcids_Sho.bowTiers`, as its Infiltration ruleset sets it (`List<ChanceTable<WeenieClassName>>`).
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
