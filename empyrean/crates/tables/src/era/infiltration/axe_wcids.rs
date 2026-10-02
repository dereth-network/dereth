// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/AxeWcids.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Wcids/Weapons/Legacy/AxeWcids.cs`; do not edit by hand

//! The tables of ClassicACE's `AxeWcids` under its Infiltration ruleset
//! (`Factories/Tables/Wcids/Weapons/Legacy/AxeWcids.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ClassicACE `AxeWcids.AxeWcids_Aluvian_T1`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static AXE_WCIDS_ALUVIAN_T1: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::axehand, 3.0),
    (WeenieClassName::axebattle, 0.5),
    (WeenieClassName::warhammer, 0.5),
]);

/// ClassicACE `AxeWcids.AxeWcids_Aluvian`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static AXE_WCIDS_ALUVIAN: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::axehand, 4.0),
    (WeenieClassName::axehandacid, 1.0),
    (WeenieClassName::axehandelectric, 1.0),
    (WeenieClassName::axehandfire, 1.0),
    (WeenieClassName::axehandfrost, 1.0),
    (WeenieClassName::axebattle, 4.0),
    (WeenieClassName::axebattleacid, 1.0),
    (WeenieClassName::axebattleelectric, 1.0),
    (WeenieClassName::axebattlefire, 1.0),
    (WeenieClassName::axebattlefrost, 1.0),
    (WeenieClassName::warhammer, 3.0),
    (WeenieClassName::warhammeracid, 0.75),
    (WeenieClassName::warhammerelectric, 0.75),
    (WeenieClassName::warhammerfire, 0.75),
    (WeenieClassName::warhammerfrost, 0.75),
]);

/// ClassicACE `AxeWcids.AxeWcids_Gharundim_T1`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static AXE_WCIDS_GHARUNDIM_T1: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::tungi, 3.0),
    (WeenieClassName::silifi, 0.5),
    (WeenieClassName::warhammer, 0.5),
]);

/// ClassicACE `AxeWcids.AxeWcids_Gharundim`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static AXE_WCIDS_GHARUNDIM: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::tungi, 4.0),
    (WeenieClassName::tungiacid, 1.0),
    (WeenieClassName::tungielectric, 1.0),
    (WeenieClassName::tungifire, 1.0),
    (WeenieClassName::tungifrost, 1.0),
    (WeenieClassName::silifi, 4.0),
    (WeenieClassName::silifiacid, 1.0),
    (WeenieClassName::silifielectric, 1.0),
    (WeenieClassName::silififire, 1.0),
    (WeenieClassName::silififrost, 1.0),
    (WeenieClassName::warhammer, 3.0),
    (WeenieClassName::warhammeracid, 0.75),
    (WeenieClassName::warhammerelectric, 0.75),
    (WeenieClassName::warhammerfire, 0.75),
    (WeenieClassName::warhammerfrost, 0.75),
]);

/// ClassicACE `AxeWcids.AxeWcids_Sho_T1`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static AXE_WCIDS_SHO_T1: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::shouono, 3.0),
    (WeenieClassName::ono, 0.5),
    (WeenieClassName::warhammer, 0.5),
]);

/// ClassicACE `AxeWcids.AxeWcids_Sho`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static AXE_WCIDS_SHO: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::shouono, 4.0),
    (WeenieClassName::shouonoacid, 1.0),
    (WeenieClassName::shouonoelectric, 1.0),
    (WeenieClassName::shouonofire, 1.0),
    (WeenieClassName::shouonofrost, 1.0),
    (WeenieClassName::ono, 4.0),
    (WeenieClassName::onoacid, 1.0),
    (WeenieClassName::onoelectric, 1.0),
    (WeenieClassName::onofire, 1.0),
    (WeenieClassName::onofrost, 1.0),
    (WeenieClassName::warhammer, 3.0),
    (WeenieClassName::warhammeracid, 0.75),
    (WeenieClassName::warhammerelectric, 0.75),
    (WeenieClassName::warhammerfire, 0.75),
    (WeenieClassName::warhammerfrost, 0.75),
]);
