// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Cantrips/WandCantrips.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Cantrips/WandCantrips.cs`; do not edit by hand

//! The literal data of ACE's `WandCantrips` (`Factories/Tables/Cantrips/WandCantrips.cs`).
//!
//! Runtime state, hand-ported in `crate::logic` (not literal here):
//! - `Table` (`SpellId[][]`)

use crate::entity::ChanceTable;
use empyrean_entity::enums::SpellId;

/// ACE `WandCantrips.spells` (`List<SpellId>`).
pub static SPELLS: [SpellId; 13] = [
    SpellId::CANTRIPFOCUS1,
    SpellId::CANTRIPWILLPOWER1,
    SpellId::CANTRIPCREATUREENCHANTMENTAPTITUDE1,
    SpellId::CANTRIPITEMENCHANTMENTAPTITUDE1,
    SpellId::CANTRIPLIFEMAGICAPTITUDE1,
    SpellId::CANTRIPWARMAGICAPTITUDE1,
    SpellId::CantripVoidMagicAptitude1,
    SpellId::CANTRIPARCANEPROWESS1,
    SpellId::CANTRIPMANACONVERSIONPROWESS1,
    SpellId::CantripSneakAttackProwess1,
    SpellId::CANTRIPDEFENDER1,
    SpellId::CantripHermeticLink1,
    SpellId::CantripSpiritThirst1,
];

/// ACE `WandCantrips.NumLevels` (`int`).
pub const NUM_LEVELS: i32 = 4;

/// ACE `WandCantrips.casterCantrips` (`ChanceTable<SpellId>`).
pub static CASTER_CANTRIPS: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::CANTRIPCREATUREENCHANTMENTAPTITUDE1, 0.05),
    (SpellId::CANTRIPITEMENCHANTMENTAPTITUDE1, 0.05),
    (SpellId::CANTRIPLIFEMAGICAPTITUDE1, 0.05),
    (SpellId::CANTRIPWARMAGICAPTITUDE1, 0.05),
    (SpellId::CANTRIPDEFENDER1, 0.05),
    (SpellId::CantripHermeticLink1, 0.05),
    (SpellId::CantripSpiritThirst1, 0.05),
    (SpellId::CANTRIPMANACONVERSIONPROWESS1, 0.05),
    (SpellId::CANTRIPWILLPOWER1, 0.05),
    (SpellId::CANTRIPSTRENGTH1, 0.04),
    (SpellId::CANTRIPENDURANCE1, 0.04),
    (SpellId::CANTRIPFOCUS1, 0.04),
    (SpellId::CANTRIPARMOR1, 0.03),
    (SpellId::CANTRIPACIDWARD1, 0.01),
    (SpellId::CANTRIPBLUDGEONINGWARD1, 0.01),
    (SpellId::CANTRIPFLAMEWARD1, 0.01),
    (SpellId::CANTRIPFROSTWARD1, 0.01),
    (SpellId::CANTRIPPIERCINGWARD1, 0.01),
    (SpellId::CANTRIPSLASHINGWARD1, 0.01),
    (SpellId::CANTRIPSTORMWARD1, 0.01),
    (SpellId::CANTRIPIMPREGNABILITY1, 0.03),
    (SpellId::CANTRIPINVULNERABILITY1, 0.03),
    (SpellId::CANTRIPMAGICRESISTANCE1, 0.03),
    (SpellId::CANTRIPARCANEPROWESS1, 0.03),
    (SpellId::CANTRIPARMOREXPERTISE1, 0.01),
    (SpellId::CANTRIPCOOKINGPROWESS1, 0.01),
    (SpellId::CANTRIPDECEPTIONPROWESS1, 0.01),
    (SpellId::CANTRIPFEALTY1, 0.01),
    (SpellId::CANTRIPHEALINGPROWESS1, 0.01),
    (SpellId::CANTRIPITEMEXPERTISE1, 0.01),
    (SpellId::CANTRIPJUMPINGPROWESS1, 0.01),
    (SpellId::CANTRIPLEADERSHIP1, 0.01),
    (SpellId::CANTRIPLOCKPICKPROWESS1, 0.01),
    (SpellId::CANTRIPMAGICITEMEXPERTISE1, 0.01),
    (SpellId::CANTRIPMONSTERATTUNEMENT1, 0.01),
    (SpellId::CANTRIPPERSONATTUNEMENT1, 0.01),
    (SpellId::CantripSalvaging1, 0.01),
    (SpellId::CANTRIPSPRINT1, 0.01),
    (SpellId::CANTRIPWEAPONEXPERTISE1, 0.01),
    (SpellId::CANTRIPCOORDINATION1, 0.01),
    (SpellId::CANTRIPQUICKNESS1, 0.01),
    (SpellId::CantripSneakAttackProwess1, 0.02),
    (SpellId::CantripSummoningProwess1, 0.02),
]);
