// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Cantrips/MeleeCantrips.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Cantrips/MeleeCantrips.cs`; do not edit by hand

//! The literal data of ACE's `MeleeCantrips` (`Factories/Tables/Cantrips/MeleeCantrips.cs`).
//!
//! Runtime state, hand-ported in `crate::logic` (not literal here):
//! - `Table` (`SpellId[][]`)

use crate::entity::ChanceTable;
use empyrean_entity::enums::SpellId;

/// ACE `MeleeCantrips.spells` (`List<SpellId>`).
pub static SPELLS: [SpellId; 12] = [
    SpellId::CANTRIPSTRENGTH1,
    SpellId::CANTRIPENDURANCE1,
    SpellId::CANTRIPCOORDINATION1,
    SpellId::CANTRIPQUICKNESS1,
    SpellId::CANTRIPBLOODTHIRST1,
    SpellId::CANTRIPHEARTTHIRST1,
    SpellId::CANTRIPDEFENDER1,
    SpellId::CANTRIPSWIFTHUNTER1,
    SpellId::CantripDualWieldAptitude1,
    SpellId::CantripDirtyFightingProwess1,
    SpellId::CantripRecklessnessProwess1,
    SpellId::CantripSneakAttackProwess1,
];

/// ACE `MeleeCantrips.NumLevels` (`int`).
pub const NUM_LEVELS: i32 = 4;

/// ACE `MeleeCantrips.meleeCantrips` (`ChanceTable<SpellId>`).
pub static MELEE_CANTRIPS: ChanceTable<SpellId> = ChanceTable::new(&[
    (SpellId::CANTRIPLIGHTWEAPONSAPTITUDE1, 0.11),
    (SpellId::CANTRIPBLOODTHIRST1, 0.06),
    (SpellId::CANTRIPDEFENDER1, 0.06),
    (SpellId::CANTRIPHEARTTHIRST1, 0.06),
    (SpellId::CANTRIPSWIFTHUNTER1, 0.05),
    (SpellId::CANTRIPSTRENGTH1, 0.05),
    (SpellId::CANTRIPENDURANCE1, 0.05),
    (SpellId::CANTRIPCOORDINATION1, 0.05),
    (SpellId::CANTRIPQUICKNESS1, 0.05),
    (SpellId::CANTRIPARCANEPROWESS1, 0.04),
    (SpellId::CANTRIPIMPREGNABILITY1, 0.03),
    (SpellId::CANTRIPINVULNERABILITY1, 0.03),
    (SpellId::CANTRIPMAGICRESISTANCE1, 0.03),
    (SpellId::CANTRIPARMOR1, 0.02),
    (SpellId::CantripSummoningProwess1, 0.02),
    (SpellId::CANTRIPALCHEMICALPROWESS1, 0.01),
    (SpellId::CANTRIPARMOREXPERTISE1, 0.01),
    (SpellId::CANTRIPCOOKINGPROWESS1, 0.01),
    (SpellId::CANTRIPDECEPTIONPROWESS1, 0.01),
    (SpellId::CANTRIPFEALTY1, 0.01),
    (SpellId::CANTRIPFLETCHINGPROWESS1, 0.01),
    (SpellId::CANTRIPHEALINGPROWESS1, 0.01),
    (SpellId::CANTRIPITEMEXPERTISE1, 0.01),
    (SpellId::CANTRIPJUMPINGPROWESS1, 0.01),
    (SpellId::CANTRIPLEADERSHIP1, 0.01),
    (SpellId::CANTRIPLOCKPICKPROWESS1, 0.01),
    (SpellId::CANTRIPMAGICITEMEXPERTISE1, 0.01),
    (SpellId::CANTRIPMONSTERATTUNEMENT1, 0.01),
    (SpellId::CANTRIPPERSONATTUNEMENT1, 0.01),
    (SpellId::CANTRIPSPRINT1, 0.01),
    (SpellId::CANTRIPWEAPONEXPERTISE1, 0.01),
    (SpellId::CantripDirtyFightingProwess1, 0.01),
    (SpellId::CantripRecklessnessProwess1, 0.01),
    (SpellId::CantripSalvaging1, 0.01),
    (SpellId::CantripSneakAttackProwess1, 0.01),
    (SpellId::CANTRIPACIDWARD1, 0.01),
    (SpellId::CANTRIPBLUDGEONINGWARD1, 0.01),
    (SpellId::CANTRIPFLAMEWARD1, 0.01),
    (SpellId::CANTRIPFROSTWARD1, 0.01),
    (SpellId::CANTRIPPIERCINGWARD1, 0.01),
    (SpellId::CANTRIPSLASHINGWARD1, 0.01),
    (SpellId::CANTRIPSTORMWARD1, 0.01),
    (SpellId::CANTRIPFOCUS1, 0.01),
    (SpellId::CANTRIPWILLPOWER1, 0.01),
]);
