// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/CrossbowWcids.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/CrossbowWcids.cs`; do not edit by hand

//! The tables of ClassicACE's `CrossbowWcids` under its Infiltration ruleset
//! (`Factories/Tables/Wcids/Weapons/CrossbowWcids.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ClassicACE `CrossbowWcids.T1_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::crossbowlight, 3.0),
    (WeenieClassName::crossbowheavy, 1.0),
]);

/// ClassicACE `CrossbowWcids.T1_T4_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::crossbowlight, 1.00),
    (WeenieClassName::crossbowheavy, 1.00),
]);

/// ClassicACE `CrossbowWcids.T5_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T5_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::crossbowlight, 4.0),
    (WeenieClassName::crossbowheavy, 4.0),
    (WeenieClassName::crossbowslashing, 1.0),
    (WeenieClassName::crossbowpiercing, 1.0),
    (WeenieClassName::crossbowblunt, 1.0),
    (WeenieClassName::crossbowacid, 1.0),
    (WeenieClassName::crossbowfire, 1.0),
    (WeenieClassName::crossbowfrost, 1.0),
    (WeenieClassName::crossbowelectric, 1.0),
]);

/// ClassicACE `CrossbowWcids.T6_T8_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T6_T8_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::crossbowslashing, 1.0),
    (WeenieClassName::crossbowpiercing, 1.0),
    (WeenieClassName::crossbowblunt, 1.0),
    (WeenieClassName::crossbowacid, 1.0),
    (WeenieClassName::crossbowfire, 1.0),
    (WeenieClassName::crossbowfrost, 1.0),
    (WeenieClassName::crossbowelectric, 1.0),
]);

/// ClassicACE `CrossbowWcids.crossbowTiers`, as its Infiltration ruleset sets it (`List<ChanceTable<WeenieClassName>>`).
pub static CROSSBOW_TIERS: [&ChanceTable<WeenieClassName>; 8] = [
    &T1_CHANCES,
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T1_T4_CHANCES,
    &T5_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
    &T6_T8_CHANCES,
];
