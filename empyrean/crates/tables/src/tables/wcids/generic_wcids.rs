// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/GenericWcids.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/GenericWcids.cs`; do not edit by hand

//! The literal data of ACE's `GenericWcids` (`Factories/Tables/Wcids/GenericWcids.cs`).
//!
//! Runtime state, hand-ported in `crate::logic` (not literal here):
//! - `_combined` (`HashSet<WeenieClassName>`)

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `GenericWcids.T1_T2_Chances` (`ChanceTable<WeenieClassName>`).
pub static T1_T2_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::bowl, 0.09),
    (WeenieClassName::chalice, 0.00),
    (WeenieClassName::cup, 0.14),
    (WeenieClassName::ewer, 0.03),
    (WeenieClassName::flagon, 0.08),
    (WeenieClassName::flasksimple, 0.13),
    (WeenieClassName::goblet, 0.07),
    (WeenieClassName::mug, 0.12),
    (WeenieClassName::ornamentalbowl, 0.00),
    (WeenieClassName::dinnerplate, 0.08),
    (WeenieClassName::stoup, 0.13),
    (WeenieClassName::tankard, 0.13),
]);

/// ACE `GenericWcids.T3_T4_Chances` (`ChanceTable<WeenieClassName>`).
pub static T3_T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::bowl, 0.08),
    (WeenieClassName::chalice, 0.06),
    (WeenieClassName::cup, 0.05),
    (WeenieClassName::ewer, 0.11),
    (WeenieClassName::flagon, 0.11),
    (WeenieClassName::flasksimple, 0.05),
    (WeenieClassName::goblet, 0.14),
    (WeenieClassName::mug, 0.14),
    (WeenieClassName::ornamentalbowl, 0.08),
    (WeenieClassName::dinnerplate, 0.08),
    (WeenieClassName::stoup, 0.05),
    (WeenieClassName::tankard, 0.05),
]);

/// ACE `GenericWcids.T5_T6_Chances` (`ChanceTable<WeenieClassName>`).
pub static T5_T6_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::bowl, 0.00),
    (WeenieClassName::chalice, 0.23),
    (WeenieClassName::cup, 0.00),
    (WeenieClassName::ewer, 0.13),
    (WeenieClassName::flagon, 0.09),
    (WeenieClassName::flasksimple, 0.00),
    (WeenieClassName::goblet, 0.23),
    (WeenieClassName::mug, 0.00),
    (WeenieClassName::ornamentalbowl, 0.19),
    (WeenieClassName::dinnerplate, 0.13),
    (WeenieClassName::stoup, 0.00),
    (WeenieClassName::tankard, 0.00),
]);

/// ACE `GenericWcids.tierChances` (`List<ChanceTable<WeenieClassName>>`).
pub static TIER_CHANCES: [&ChanceTable<WeenieClassName>; 6] = [
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
    &T3_T4_CHANCES,
    &T3_T4_CHANCES,
    &T5_T6_CHANCES,
    &T5_T6_CHANCES,
];
