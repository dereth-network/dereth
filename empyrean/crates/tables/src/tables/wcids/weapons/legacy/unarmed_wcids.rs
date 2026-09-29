// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/UnarmedWcids.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/UnarmedWcids.cs`; do not edit by hand

//! The literal data of ACE's `UnarmedWcids` (`Factories/Tables/Wcids/Weapons/Legacy/UnarmedWcids.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `UnarmedWcids.UnarmedWcids_Aluvian` (`ChanceTable<WeenieClassName>`).
pub static UNARMED_WCIDS_ALUVIAN: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::cestus, 0.40),
    (WeenieClassName::cestusacid, 0.15),
    (WeenieClassName::cestuselectric, 0.15),
    (WeenieClassName::cestusfire, 0.15),
    (WeenieClassName::cestusfrost, 0.15),
]);

/// ACE `UnarmedWcids.UnarmedWcids_Gharundim` (`ChanceTable<WeenieClassName>`).
pub static UNARMED_WCIDS_GHARUNDIM: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::katar, 0.40),
    (WeenieClassName::kataracid, 0.15),
    (WeenieClassName::katarelectric, 0.15),
    (WeenieClassName::katarfire, 0.15),
    (WeenieClassName::katarfrost, 0.15),
]);

/// ACE `UnarmedWcids.UnarmedWcids_Sho` (`ChanceTable<WeenieClassName>`).
pub static UNARMED_WCIDS_SHO: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::nekode, 0.40),
    (WeenieClassName::nekodeacid, 0.15),
    (WeenieClassName::nekodeelectric, 0.15),
    (WeenieClassName::nekodefire, 0.15),
    (WeenieClassName::nekodefrost, 0.15),
]);
