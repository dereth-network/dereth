// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/StaffWcids.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/StaffWcids.cs`; do not edit by hand

//! The literal data of ACE's `StaffWcids` (`Factories/Tables/Wcids/Weapons/Legacy/StaffWcids.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `StaffWcids.StaffWcids_Aluvian` (`ChanceTable<WeenieClassName>`).
pub static STAFF_WCIDS_ALUVIAN: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::quarterstaffnew, 0.40),
    (WeenieClassName::quarterstaffacidnew, 0.15),
    (WeenieClassName::quarterstaffelectricnew, 0.15),
    (WeenieClassName::quarterstaffflamenew, 0.15),
    (WeenieClassName::quarterstafffrostnew, 0.15),
]);

/// ACE `StaffWcids.StaffWcids_Gharundim` (`ChanceTable<WeenieClassName>`).
pub static STAFF_WCIDS_GHARUNDIM: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::nabutnew, 0.40),
    (WeenieClassName::nabutacidnew, 0.15),
    (WeenieClassName::nabutelectricnew, 0.15),
    (WeenieClassName::nabutfirenew, 0.15),
    (WeenieClassName::nabutfrostnew, 0.15),
]);

/// ACE `StaffWcids.StaffWcids_Sho` (`ChanceTable<WeenieClassName>`).
pub static STAFF_WCIDS_SHO: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::jonew, 0.40),
    (WeenieClassName::joacidnew, 0.15),
    (WeenieClassName::joelectricnew, 0.15),
    (WeenieClassName::jofirenew, 0.15),
    (WeenieClassName::jofrostnew, 0.15),
]);
