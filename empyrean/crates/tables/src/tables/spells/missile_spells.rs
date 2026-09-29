// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Spells/MissileSpells.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Spells/MissileSpells.cs`; do not edit by hand

//! The literal data of ACE's `MissileSpells` (`Factories/Tables/Spells/MissileSpells.cs`).
//!
//! Runtime state, hand-ported in `crate::logic` (not literal here):
//! - `Table` (`SpellId[][]`)
//! - `CreatureLifeTable` (`List<SpellId>`)

use empyrean_entity::enums::SpellId;

/// ACE `MissileSpells.spells` (`List<SpellId>`).
pub static SPELLS: [SpellId; 11] = [
    SpellId::StrengthSelf1,
    SpellId::EnduranceSelf1,
    SpellId::CoordinationSelf1,
    SpellId::QuicknessSelf1,
    SpellId::BloodDrinkerSelf1,
    SpellId::HeartSeekerSelf1,
    SpellId::DefenderSelf1,
    SpellId::SwiftKillerSelf1,
    SpellId::DirtyFightingMasterySelf1,
    SpellId::RecklessnessMasterySelf1,
    SpellId::SneakAttackMasterySelf1,
];

/// ACE `MissileSpells.NumTiers` (`int`).
pub const NUM_TIERS: i32 = 8;

/// ACE `MissileSpells.weaponMissileSpells` (`List<(SpellId, float)>`).
pub static WEAPON_MISSILE_SPELLS: [(SpellId, f32); 3] = [
    (SpellId::SwiftKillerSelf1, 0.30),
    (SpellId::DefenderSelf1, 0.25),
    (SpellId::BloodDrinkerSelf1, 1.00),
];
