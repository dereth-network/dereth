// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Spells/MeleeSpells.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Spells/MeleeSpells.cs`; do not edit by hand

//! The literal data of ACE's `MeleeSpells` (`Factories/Tables/Spells/MeleeSpells.cs`).
//!
//! Runtime state, hand-ported in `crate::logic` (not literal here):
//! - `Table` (`SpellId[][]`)
//! - `CreatureLifeTable` (`List<SpellId>`)

use empyrean_entity::enums::SpellId;

/// ACE `MeleeSpells.spells` (`List<SpellId>`).
pub static SPELLS: [SpellId; 12] = [
    SpellId::StrengthSelf1,
    SpellId::EnduranceSelf1,
    SpellId::CoordinationSelf1,
    SpellId::QuicknessSelf1,
    SpellId::BloodDrinkerSelf1,
    SpellId::DefenderSelf1,
    SpellId::HeartSeekerSelf1,
    SpellId::SwiftKillerSelf1,
    SpellId::DirtyFightingMasterySelf1,
    SpellId::DualWieldMasterySelf1,
    SpellId::RecklessnessMasterySelf1,
    SpellId::SneakAttackMasterySelf1,
];

/// ACE `MeleeSpells.NumTiers` (`int`).
pub const NUM_TIERS: i32 = 8;

/// ACE `MeleeSpells.weaponMeleeSpells` (`List<(SpellId, float)>`).
pub static WEAPON_MELEE_SPELLS: [(SpellId, f32); 4] = [
    (SpellId::DefenderSelf1, 0.25),
    (SpellId::BloodDrinkerSelf1, 1.00),
    (SpellId::SwiftKillerSelf1, 0.30),
    (SpellId::HeartSeekerSelf1, 0.25),
];
