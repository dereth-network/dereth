// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/ClothingWcids.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Wcids/ClothingWcids.cs`; do not edit by hand

//! The literal data of ACE's `ClothingWcids` (`Factories/Tables/Wcids/ClothingWcids.cs`).
//!
//! Runtime state, hand-ported in `crate::logic` (not literal here):
//! - `_combined` (`HashSet<WeenieClassName>`)

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ACE `ClothingWcids.ClothingWcids_Aluvian` (`ChanceTable<WeenieClassName>`).
pub static CLOTHING_WCIDS_ALUVIAN: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::shirtbaggy, 0.06),
    (WeenieClassName::tunicbaggy, 0.06),
    (WeenieClassName::capcloth, 0.07),
    (WeenieClassName::cowlcloth, 0.07),
    (WeenieClassName::doublet, 0.05),
    (WeenieClassName::glovescloth, 0.08),
    (WeenieClassName::pants, 0.08),
    (WeenieClassName::shirt, 0.06),
    (WeenieClassName::shoes, 0.20),
    (WeenieClassName::smock, 0.06),
    (WeenieClassName::trousers, 0.08),
    (WeenieClassName::tunic, 0.06),
    (WeenieClassName::breecheswide, 0.07),
]);

/// ACE `ClothingWcids.ClothingWcids_Gharundim` (`ChanceTable<WeenieClassName>`).
pub static CLOTHING_WCIDS_GHARUNDIM: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::breechesbaggy, 0.07),
    (WeenieClassName::pantsbaggy, 0.07),
    (WeenieClassName::tunicbaggy, 0.06),
    (WeenieClassName::capfez, 0.07),
    (WeenieClassName::glovescloth, 0.09),
    (WeenieClassName::jerkin, 0.06),
    (WeenieClassName::shirtloose, 0.05),
    (WeenieClassName::pantaloons, 0.07),
    (WeenieClassName::shirtpuffy, 0.05),
    (WeenieClassName::tunicpuffy, 0.06),
    (WeenieClassName::qafiya, 0.06),
    (WeenieClassName::sandals, 0.05),
    (WeenieClassName::shoes, 0.06),
    (WeenieClassName::slippers, 0.06),
    (WeenieClassName::smock, 0.05),
    (WeenieClassName::turban, 0.07),
]);

/// ACE `ClothingWcids.ClothingWcids_Sho` (`ChanceTable<WeenieClassName>`).
pub static CLOTHING_WCIDS_SHO: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::shirtbaggy, 0.06),
    (WeenieClassName::capcloth, 0.08),
    (WeenieClassName::doublet, 0.06),
    (WeenieClassName::pantsflared, 0.08),
    (WeenieClassName::shirtflared, 0.06),
    (WeenieClassName::tunicflared, 0.05),
    (WeenieClassName::glovescloth, 0.09),
    (WeenieClassName::capsho, 0.08),
    (WeenieClassName::breechesloose, 0.08),
    (WeenieClassName::pantsloose, 0.08),
    (WeenieClassName::shirtloose, 0.06),
    (WeenieClassName::tunicloose, 0.05),
    (WeenieClassName::shoes, 0.10),
    (WeenieClassName::slippers, 0.07),
]);

/// ACE `ClothingWcids.ClothingWcids_Viamontian` (`ChanceTable<WeenieClassName>`).
pub static CLOTHING_WCIDS_VIAMONTIAN: ChanceTable<WeenieClassName> = ChanceTable::new(&[
    (WeenieClassName::shirtviamontfancy, 0.11),
    (WeenieClassName::shirtviamontpoet, 0.11),
    (WeenieClassName::shirtviamontvest, 0.11),
    (WeenieClassName::leggingsviamont, 0.24),
    (WeenieClassName::hatberet, 0.06),
    (WeenieClassName::hatbandana, 0.06),
    (WeenieClassName::ace44975_hood, 0.06),
    (WeenieClassName::shoesviamontloafers, 0.075),
    (WeenieClassName::bootsviamont, 0.075),
    (WeenieClassName::glovescloth, 0.10),
]);

/// The arguments of the `BuildCombined(..)` calls that make up ACE's `static ClothingWcids()`, in
/// call order.
pub static STATIC_CTOR_BUILD_COMBINED_ARGS: [&ChanceTable<WeenieClassName>; 4] = [
    &CLOTHING_WCIDS_ALUVIAN,
    &CLOTHING_WCIDS_GHARUNDIM,
    &CLOTHING_WCIDS_SHO,
    &CLOTHING_WCIDS_VIAMONTIAN,
];
