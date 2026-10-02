// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/Cantrips/MissileCantrips.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/Cantrips/MissileCantrips.cs`; do not edit by hand

//! The tables of ClassicACE's `MissileCantrips` under its Infiltration ruleset
//! (`Factories/Tables/Cantrips/MissileCantrips.cs`).

use crate::entity::ChanceTable;
use empyrean_entity::enums::SpellId;

/// ClassicACE `MissileCantrips.missileCantrips`, as its Infiltration ruleset sets it (`ChanceTable<SpellId>`).
pub static MISSILE_CANTRIPS: ChanceTable<SpellId> = ChanceTable::new_weighted(&[
    (SpellId::CANTRIPMISSILEWEAPONSAPTITUDE1, 1.0),
    (SpellId::CANTRIPDEFENDER1, 0.7),
    (SpellId::CANTRIPSTRENGTH1, 0.7),
    (SpellId::CANTRIPCOORDINATION1, 0.7),
    (SpellId::CANTRIPBLOODTHIRST1, 0.6),
    (SpellId::CANTRIPSWIFTHUNTER1, 0.6),
    (SpellId::CANTRIPQUICKNESS1, 0.6),
    (SpellId::CANTRIPENDURANCE1, 0.5),
    (SpellId::CANTRIPARCANEPROWESS1, 0.4),
    (SpellId::CANTRIPIMPREGNABILITY1, 0.4),
    (SpellId::CANTRIPINVULNERABILITY1, 0.3),
    (SpellId::CANTRIPMAGICRESISTANCE1, 0.3),
    (SpellId::CANTRIPALCHEMICALPROWESS1, 0.1),
    (SpellId::CANTRIPARMOREXPERTISE1, 0.1),
    (SpellId::CANTRIPCOOKINGPROWESS1, 0.1),
    (SpellId::CANTRIPDECEPTIONPROWESS1, 0.1),
    (SpellId::CANTRIPFEALTY1, 0.1),
    (SpellId::CANTRIPFLETCHINGPROWESS1, 0.1),
    (SpellId::CANTRIPHEALINGPROWESS1, 0.1),
    (SpellId::CANTRIPITEMEXPERTISE1, 0.1),
    (SpellId::CANTRIPJUMPINGPROWESS1, 0.1),
    (SpellId::CANTRIPLEADERSHIP1, 0.1),
    (SpellId::CANTRIPLOCKPICKPROWESS1, 0.1),
    (SpellId::CANTRIPMAGICITEMEXPERTISE1, 0.1),
    (SpellId::CANTRIPMONSTERATTUNEMENT1, 0.1),
    (SpellId::CANTRIPPERSONATTUNEMENT1, 0.1),
    (SpellId::CANTRIPSPRINT1, 0.1),
    (SpellId::CANTRIPWEAPONEXPERTISE1, 0.1),
    (SpellId::CANTRIPARMOR1, 0.1),
    (SpellId::CANTRIPACIDWARD1, 0.1),
    (SpellId::CANTRIPBLUDGEONINGWARD1, 0.1),
    (SpellId::CANTRIPFLAMEWARD1, 0.1),
    (SpellId::CANTRIPFROSTWARD1, 0.1),
    (SpellId::CANTRIPPIERCINGWARD1, 0.1),
    (SpellId::CANTRIPSLASHINGWARD1, 0.1),
    (SpellId::CANTRIPSTORMWARD1, 0.1),
    (SpellId::CANTRIPFOCUS1, 0.1),
    (SpellId::CANTRIPWILLPOWER1, 0.1),
]);
