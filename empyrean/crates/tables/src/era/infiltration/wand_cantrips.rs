// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Cantrips/WandCantrips.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Cantrips/WandCantrips.cs`; do not edit by hand

//! The tables of ClassicACE's `WandCantrips` under its Infiltration ruleset
//! (`Factories/Tables/Cantrips/WandCantrips.cs`).

use crate::entity::ChanceTable;
use empyrean_entity::enums::SpellId;

/// ClassicACE `WandCantrips.casterCantrips`, as its Infiltration ruleset sets it (`ChanceTable<SpellId>`).
pub static CASTER_CANTRIPS: ChanceTable<SpellId> = ChanceTable::new_weighted(&[
    (SpellId::CANTRIPCREATUREENCHANTMENTAPTITUDE1, 5.0),
    (SpellId::CANTRIPITEMENCHANTMENTAPTITUDE1, 5.0),
    (SpellId::CANTRIPLIFEMAGICAPTITUDE1, 5.0),
    (SpellId::CANTRIPWARMAGICAPTITUDE1, 5.0),
    (SpellId::CANTRIPDEFENDER1, 5.0),
    (SpellId::CantripHermeticLink1, 5.0),
    (SpellId::CANTRIPMANACONVERSIONPROWESS1, 5.0),
    (SpellId::CANTRIPWILLPOWER1, 5.0),
    (SpellId::CANTRIPSTRENGTH1, 4.0),
    (SpellId::CANTRIPENDURANCE1, 4.0),
    (SpellId::CANTRIPFOCUS1, 4.0),
    (SpellId::CANTRIPARMOR1, 3.0),
    (SpellId::CANTRIPACIDWARD1, 1.0),
    (SpellId::CANTRIPBLUDGEONINGWARD1, 1.0),
    (SpellId::CANTRIPFLAMEWARD1, 1.0),
    (SpellId::CANTRIPFROSTWARD1, 1.0),
    (SpellId::CANTRIPPIERCINGWARD1, 1.0),
    (SpellId::CANTRIPSLASHINGWARD1, 1.0),
    (SpellId::CANTRIPSTORMWARD1, 1.0),
    (SpellId::CANTRIPIMPREGNABILITY1, 3.0),
    (SpellId::CANTRIPINVULNERABILITY1, 3.0),
    (SpellId::CANTRIPMAGICRESISTANCE1, 3.0),
    (SpellId::CANTRIPALCHEMICALPROWESS1, 1.0),
    (SpellId::CANTRIPARCANEPROWESS1, 3.0),
    (SpellId::CANTRIPARMOREXPERTISE1, 1.0),
    (SpellId::CANTRIPCOOKINGPROWESS1, 1.0),
    (SpellId::CANTRIPDECEPTIONPROWESS1, 1.0),
    (SpellId::CANTRIPFEALTY1, 1.0),
    (SpellId::CANTRIPFLETCHINGPROWESS1, 1.0),
    (SpellId::CANTRIPHEALINGPROWESS1, 1.0),
    (SpellId::CANTRIPITEMEXPERTISE1, 1.0),
    (SpellId::CANTRIPJUMPINGPROWESS1, 1.0),
    (SpellId::CANTRIPLEADERSHIP1, 1.0),
    (SpellId::CANTRIPLOCKPICKPROWESS1, 1.0),
    (SpellId::CANTRIPMAGICITEMEXPERTISE1, 1.0),
    (SpellId::CANTRIPMONSTERATTUNEMENT1, 1.0),
    (SpellId::CANTRIPPERSONATTUNEMENT1, 1.0),
    (SpellId::CantripSalvaging1, 1.0),
    (SpellId::CANTRIPSPRINT1, 1.0),
    (SpellId::CANTRIPWEAPONEXPERTISE1, 1.0),
    (SpellId::CANTRIPCOORDINATION1, 1.0),
    (SpellId::CANTRIPQUICKNESS1, 1.0),
]);
