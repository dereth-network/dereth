// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/JewelryWcids.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Wcids/JewelryWcids.cs`; do not edit by hand

//! The tables of ClassicACE's `JewelryWcids` under its Infiltration ruleset
//! (`Factories/Tables/Wcids/JewelryWcids.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ClassicACE `JewelryWcids.T1_T2_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T1_T2_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::amulet, 0.10),
    (WeenieClassName::bracelet, 0.30),
    (WeenieClassName::braceletheavy, 0.10),
    (WeenieClassName::necklace, 0.20),
    (WeenieClassName::ring, 0.25),
    (WeenieClassName::ringjeweled, 0.05),
]);

/// ClassicACE `JewelryWcids.T3_T4_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T3_T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::amulet, 0.10),
    (WeenieClassName::bracelet, 0.15),
    (WeenieClassName::braceletheavy, 0.15),
    (WeenieClassName::gorget, 0.10),
    (WeenieClassName::necklace, 0.15),
    (WeenieClassName::necklaceheavy, 0.05),
    (WeenieClassName::ring, 0.15),
    (WeenieClassName::ringjeweled, 0.15),
]);

/// ClassicACE `JewelryWcids.T5_T6_Chances`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static T5_T6_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::amulet, 0.05),
    (WeenieClassName::bracelet, 0.05),
    (WeenieClassName::braceletheavy, 0.20),
    (WeenieClassName::crown, 0.10),
    (WeenieClassName::gorget, 0.10),
    (WeenieClassName::necklace, 0.05),
    (WeenieClassName::necklaceheavy, 0.15),
    (WeenieClassName::ring, 0.10),
    (WeenieClassName::ringjeweled, 0.20),
]);

/// ClassicACE `JewelryWcids.tierChances`, as its Infiltration ruleset sets it (`List<ChanceTable<WeenieClassName>>`).
pub static TIER_CHANCES: [&ChanceTable<WeenieClassName>; 6] = [
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
    &T3_T4_CHANCES,
    &T3_T4_CHANCES,
    &T5_T6_CHANCES,
    &T5_T6_CHANCES,
];
