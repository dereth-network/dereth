// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/CoalescedManaWcids.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/CoalescedManaWcids.cs`; do not edit by hand

//! The literal data of ACE's `CoalescedManaWcids` (`Factories/Tables/Wcids/CoalescedManaWcids.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `CoalescedManaWcids.T1_T2_Chances` (`ChanceTable<WeenieClassName>`).
pub static T1_T2_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace42518_coalescedmana, 1.0),
]);

/// ACE `CoalescedManaWcids.T3_Chances` (`ChanceTable<WeenieClassName>`).
pub static T3_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace42518_coalescedmana, 0.75),
    (WeenieClassName::ace42517_coalescedmana, 0.25),
]);

/// ACE `CoalescedManaWcids.T4_Chances` (`ChanceTable<WeenieClassName>`).
pub static T4_CHANCES: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::ace42518_coalescedmana, 0.25),
    (WeenieClassName::ace42517_coalescedmana, 0.50),
    (WeenieClassName::ace42516_coalescedmana, 0.25),
]);

/// ACE `CoalescedManaWcids.tierChances` (`List<ChanceTable<WeenieClassName>>`).
pub static TIER_CHANCES: [&ChanceTable<WeenieClassName>; 4] = [
    &T1_T2_CHANCES,
    &T1_T2_CHANCES,
    &T3_CHANCES,
    &T4_CHANCES,
];
