// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/StaffWcids.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/StaffWcids.cs`; do not edit by hand

//! The tables of ClassicACE's `StaffWcids` under its Infiltration ruleset
//! (`Factories/Tables/Wcids/Weapons/Legacy/StaffWcids.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ClassicACE `StaffWcids.StaffWcids_Aluvian_T1`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static STAFF_WCIDS_ALUVIAN_T1: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::quarterstaffnew, 1.0),
]);

/// ClassicACE `StaffWcids.StaffWcids_Aluvian`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static STAFF_WCIDS_ALUVIAN: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::quarterstaffnew, 4.0),
    (WeenieClassName::quarterstaffacidnew, 1.0),
    (WeenieClassName::quarterstaffelectricnew, 1.0),
    (WeenieClassName::quarterstaffflamenew, 1.0),
    (WeenieClassName::quarterstafffrostnew, 1.0),
]);

/// ClassicACE `StaffWcids.StaffWcids_Gharundim_T1`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static STAFF_WCIDS_GHARUNDIM_T1: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::nabutnew, 1.0),
]);

/// ClassicACE `StaffWcids.StaffWcids_Gharundim`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static STAFF_WCIDS_GHARUNDIM: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::nabutnew, 4.0),
    (WeenieClassName::nabutacidnew, 1.0),
    (WeenieClassName::nabutelectricnew, 1.0),
    (WeenieClassName::nabutfirenew, 1.0),
    (WeenieClassName::nabutfrostnew, 1.0),
]);

/// ClassicACE `StaffWcids.StaffWcids_Sho_T1`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static STAFF_WCIDS_SHO_T1: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::jonew, 1.0),
]);

/// ClassicACE `StaffWcids.StaffWcids_Sho`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static STAFF_WCIDS_SHO: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::jonew, 4.0),
    (WeenieClassName::joacidnew, 1.0),
    (WeenieClassName::joelectricnew, 1.0),
    (WeenieClassName::jofirenew, 1.0),
    (WeenieClassName::jofrostnew, 1.0),
]);
