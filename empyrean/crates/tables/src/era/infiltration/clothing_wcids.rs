// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/ClothingWcids.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Wcids/ClothingWcids.cs`; do not edit by hand

//! The tables of ClassicACE's `ClothingWcids` under its Infiltration ruleset
//! (`Factories/Tables/Wcids/ClothingWcids.cs`).

use crate::entity::ChanceTable;
use crate::enums::WeenieClassName;

/// ClassicACE `ClothingWcids.ClothingWcids_Aluvian`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static CLOTHING_WCIDS_ALUVIAN: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::shirt, 4.0),
    (WeenieClassName::doublet, 4.0),
    (WeenieClassName::tunic, 4.0),
    (WeenieClassName::smock, 4.0),
    (WeenieClassName::shirtbaggy, 4.0),
    (WeenieClassName::tunicbaggy, 4.0),
    (WeenieClassName::pants, 4.0),
    (WeenieClassName::trousers, 4.0),
    (WeenieClassName::breecheswide, 4.0),
    (WeenieClassName::capcloth, 1.0),
    (WeenieClassName::cowlcloth, 1.0),
    (WeenieClassName::glovescloth, 1.0),
    (WeenieClassName::shoes, 1.0),
]);

/// ClassicACE `ClothingWcids.ClothingWcids_Gharundim`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static CLOTHING_WCIDS_GHARUNDIM: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::jerkin, 4.0),
    (WeenieClassName::smock, 4.0),
    (WeenieClassName::shirtloose, 4.0),
    (WeenieClassName::shirtpuffy, 4.0),
    (WeenieClassName::tunicpuffy, 4.0),
    (WeenieClassName::tunicbaggy, 4.0),
    (WeenieClassName::breechesbaggy, 4.0),
    (WeenieClassName::pantsbaggy, 4.0),
    (WeenieClassName::pantaloons, 4.0),
    (WeenieClassName::capfez, 1.0),
    (WeenieClassName::qafiya, 1.0),
    (WeenieClassName::turban, 1.0),
    (WeenieClassName::glovescloth, 1.0),
    (WeenieClassName::sandals, 1.0),
    (WeenieClassName::shoes, 1.0),
    (WeenieClassName::slippers, 1.0),
]);

/// ClassicACE `ClothingWcids.ClothingWcids_Sho`, as its Infiltration ruleset sets it (`ChanceTable<WeenieClassName>`).
pub static CLOTHING_WCIDS_SHO: ChanceTable<WeenieClassName> = ChanceTable::new_weighted(&[
    (WeenieClassName::shirtbaggy, 4.0),
    (WeenieClassName::shirtflared, 4.0),
    (WeenieClassName::tunicflared, 4.0),
    (WeenieClassName::doublet, 4.0),
    (WeenieClassName::shirtloose, 4.0),
    (WeenieClassName::tunicloose, 4.0),
    (WeenieClassName::pantsflared, 4.0),
    (WeenieClassName::breechesloose, 4.0),
    (WeenieClassName::pantsloose, 4.0),
    (WeenieClassName::capcloth, 1.0),
    (WeenieClassName::capsho, 1.0),
    (WeenieClassName::glovescloth, 1.0),
    (WeenieClassName::shoes, 1.0),
    (WeenieClassName::slippers, 1.0),
]);

/// ClassicACE `ClothingWcids.ClothingWcids_Viamontian` (`ChanceTable<WeenieClassName>`).
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
