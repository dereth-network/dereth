// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Spells/WandSpells.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/Spells/WandSpells.cs`; do not edit by hand

//! The literal data of ACE's `WandSpells` (`Factories/Tables/Spells/WandSpells.cs`).
//!
//! Runtime state, hand-ported in `crate::logic` (not literal here):
//! - `Table` (`SpellId[][]`)
//! - `CreatureLifeTable` (`List<SpellId>`)

use empyrean_entity::enums::SpellId;

/// ACE `WandSpells.spells` (`List<SpellId>`).
pub static SPELLS: [SpellId; 13] = [
    SpellId::FocusSelf1,
    SpellId::WillpowerSelf1,
    SpellId::CreatureEnchantmentMasterySelf1,
    SpellId::ItemEnchantmentMasterySelf1,
    SpellId::LifeMagicMasterySelf1,
    SpellId::WarMagicMasterySelf1,
    SpellId::VoidMagicMasterySelf1,
    SpellId::DefenderSelf1,
    SpellId::HermeticLinkSelf1,
    SpellId::SpiritDrinkerSelf1,
    SpellId::ArcaneEnlightenmentSelf1,
    SpellId::ManaMasterySelf1,
    SpellId::SneakAttackMasterySelf1,
];

/// ACE `WandSpells.NumTiers` (`int`).
pub const NUM_TIERS: i32 = 8;

/// ACE `WandSpells.wandSpells` (`List<(SpellId, float)>`).
pub static WAND_SPELLS: [(SpellId, f32); 3] = [
    (SpellId::DefenderSelf1, 0.25),
    (SpellId::HermeticLinkSelf1, 1.0),
    (SpellId::SpiritDrinkerSelf1, 0.25),
];
