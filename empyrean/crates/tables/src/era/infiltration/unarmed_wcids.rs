// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/UnarmedWcids.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/UnarmedWcids.cs`; do not edit by hand

//! The tables of ClassicACE's `UnarmedWcids` under its Infiltration ruleset
//! (`Factories/Tables/Wcids/Weapons/Legacy/UnarmedWcids.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ClassicACE `UnarmedWcids.UnarmedWcids_Aluvian_T1`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static UNARMED_WCIDS_ALUVIAN_T1: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::cestus, 1.00),
]);

/// ClassicACE `UnarmedWcids.UnarmedWcids_Aluvian`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static UNARMED_WCIDS_ALUVIAN: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::cestus, 4.00),
    (WeenieClassName::cestusacid, 1.00),
    (WeenieClassName::cestuselectric, 1.00),
    (WeenieClassName::cestusfire, 1.00),
    (WeenieClassName::cestusfrost, 1.00),
]);

/// ClassicACE `UnarmedWcids.UnarmedWcids_Gharundim_T1`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static UNARMED_WCIDS_GHARUNDIM_T1: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::katar, 1.00),
]);

/// ClassicACE `UnarmedWcids.UnarmedWcids_Gharundim`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static UNARMED_WCIDS_GHARUNDIM: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::katar, 4.00),
    (WeenieClassName::kataracid, 1.00),
    (WeenieClassName::katarelectric, 1.00),
    (WeenieClassName::katarfire, 1.00),
    (WeenieClassName::katarfrost, 1.00),
]);

/// ClassicACE `UnarmedWcids.UnarmedWcids_Sho_T1`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static UNARMED_WCIDS_SHO_T1: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::nekode, 1.00),
]);

/// ClassicACE `UnarmedWcids.UnarmedWcids_Sho`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static UNARMED_WCIDS_SHO: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::nekode, 4.00),
    (WeenieClassName::nekodeacid, 1.00),
    (WeenieClassName::nekodeelectric, 1.00),
    (WeenieClassName::nekodefire, 1.00),
    (WeenieClassName::nekodefrost, 1.00),
]);
