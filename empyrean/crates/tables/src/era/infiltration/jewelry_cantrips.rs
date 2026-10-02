// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Cantrips/JewelryCantrips.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Cantrips/JewelryCantrips.cs`; do not edit by hand

//! The tables of ClassicACE's `JewelryCantrips` under its Infiltration ruleset
//! (`Factories/Tables/Cantrips/JewelryCantrips.cs`).

use crate::entity::ChanceTable;
use empyrean_entity::enums::SpellId;

/// ClassicACE `JewelryCantrips.jewelryCantrips`, as its Infiltration ruleset sets it (`ChanceTable<SpellId>`).
pub static JEWELRY_CANTRIPS: ChanceTable<SpellId> = ChanceTable::new_weighted(&[
    (SpellId::CANTRIPARMOR1, 3.0),
    (SpellId::CANTRIPACIDWARD1, 3.0),
    (SpellId::CANTRIPBLUDGEONINGWARD1, 3.0),
    (SpellId::CANTRIPFLAMEWARD1, 3.0),
    (SpellId::CANTRIPFROSTWARD1, 3.0),
    (SpellId::CANTRIPPIERCINGWARD1, 3.0),
    (SpellId::CANTRIPSLASHINGWARD1, 3.0),
    (SpellId::CANTRIPSTORMWARD1, 3.0),
    (SpellId::CANTRIPLIGHTWEAPONSAPTITUDE1, 2.0),
    (SpellId::CANTRIPFINESSEWEAPONSAPTITUDE1, 2.0),
    (SpellId::CANTRIPMACEAPTITUDE1, 2.0),
    (SpellId::CANTRIPSPEARAPTITUDE1, 2.0),
    (SpellId::CANTRIPSTAFFAPTITUDE1, 2.0),
    (SpellId::CANTRIPHEAVYWEAPONSAPTITUDE1, 2.0),
    (SpellId::CANTRIPUNARMEDAPTITUDE1, 2.0),
    (SpellId::CANTRIPMISSILEWEAPONSAPTITUDE1, 2.0),
    (SpellId::CANTRIPCROSSBOWAPTITUDE1, 2.0),
    (SpellId::CANTRIPTHROWNAPTITUDE1, 2.0),
    (SpellId::CANTRIPIMPREGNABILITY1, 2.0),
    (SpellId::CANTRIPINVULNERABILITY1, 2.0),
    (SpellId::CANTRIPMAGICRESISTANCE1, 2.0),
    (SpellId::CANTRIPCREATUREENCHANTMENTAPTITUDE1, 2.0),
    (SpellId::CANTRIPITEMENCHANTMENTAPTITUDE1, 2.0),
    (SpellId::CANTRIPLIFEMAGICAPTITUDE1, 2.0),
    (SpellId::CANTRIPWARMAGICAPTITUDE1, 2.0),
    (SpellId::CANTRIPALCHEMICALPROWESS1, 2.0),
    (SpellId::CANTRIPARCANEPROWESS1, 2.0),
    (SpellId::CANTRIPARMOREXPERTISE1, 2.0),
    (SpellId::CANTRIPCOOKINGPROWESS1, 2.0),
    (SpellId::CANTRIPDECEPTIONPROWESS1, 2.0),
    (SpellId::CANTRIPFEALTY1, 2.0),
    (SpellId::CANTRIPFLETCHINGPROWESS1, 2.0),
    (SpellId::CANTRIPHEALINGPROWESS1, 2.0),
    (SpellId::CANTRIPITEMEXPERTISE1, 2.0),
    (SpellId::CANTRIPJUMPINGPROWESS1, 2.0),
    (SpellId::CANTRIPLEADERSHIP1, 2.0),
    (SpellId::CANTRIPLOCKPICKPROWESS1, 2.0),
    (SpellId::CANTRIPMAGICITEMEXPERTISE1, 2.0),
    (SpellId::CANTRIPMANACONVERSIONPROWESS1, 2.0),
    (SpellId::CANTRIPMONSTERATTUNEMENT1, 2.0),
    (SpellId::CANTRIPPERSONATTUNEMENT1, 2.0),
    (SpellId::CANTRIPSPRINT1, 2.0),
    (SpellId::CANTRIPWEAPONEXPERTISE1, 2.0),
    (SpellId::CantripSalvaging1, 2.0),
]);
