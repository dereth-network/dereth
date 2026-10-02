// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/AtlatlWcids.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/AtlatlWcids.cs`; do not edit by hand

//! The tables of ClassicACE's `AtlatlWcids` under its Infiltration ruleset
//! (`Factories/Tables/Wcids/Weapons/AtlatlWcids.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ClassicACE `AtlatlWcids.T1_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::atlatl, 3.0),
    (WeenieClassName::atlatlroyal, 1.0),
]);

/// ClassicACE `AtlatlWcids.T1_T4_Chances` (`ChanceTable<WeenieClassName>`).
pub static T1_T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::atlatl, 0.50),
    (WeenieClassName::atlatlroyal, 0.50),
]);

/// ClassicACE `AtlatlWcids.T5_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T5_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::atlatl, 4.0),
    (WeenieClassName::atlatlroyal, 4.0),
    (WeenieClassName::atlatlslashing, 1.0),
    (WeenieClassName::atlatlpiercing, 1.0),
    (WeenieClassName::atlatlblunt, 1.0),
    (WeenieClassName::atlatlacid, 1.0),
    (WeenieClassName::atlatlfire, 1.0),
    (WeenieClassName::atlatlfrost, 1.0),
    (WeenieClassName::atlatlelectric, 1.0),
]);

/// ClassicACE `AtlatlWcids.T6_T8_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T6_T8_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::atlatlslashing, 1.0),
    (WeenieClassName::atlatlpiercing, 1.0),
    (WeenieClassName::atlatlblunt, 1.0),
    (WeenieClassName::atlatlacid, 1.0),
    (WeenieClassName::atlatlfire, 1.0),
    (WeenieClassName::atlatlfrost, 1.0),
    (WeenieClassName::atlatlelectric, 1.0),
]);

/// ClassicACE `AtlatlWcids.atlatlTiers`, as its Infiltration ruleset sets it (`List<ChanceTable<WeenieClassName>>`).
pub static ATLATL_TIERS: [&ChanceTable<WeenieClassName>; 8] = [
    &T1_CHANCES,
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T5_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
];
