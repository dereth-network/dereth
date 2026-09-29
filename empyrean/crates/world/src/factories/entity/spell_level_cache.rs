// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Entity/SpellLevelCache.cs
//! Port of `Source/ACE.Server/Factories/Entity/SpellLevelCache.cs`.
//!
//! ACE's static `ConcurrentDictionary` is the world's (`World.loot_tables.spell_levels`): a
//! spell's client level depends only on the portal dat, which one server loads once, but a test
//! process builds worlds over different dats.

use std::sync::PoisonError;

use crate::entity::spell::Spell;
use crate::World;

/// The client (scarab) level of a spell (`Spell.Formula.Level`), cached by id.
///
/// # Panics
/// As ACE throws a `NullReferenceException`, when the portal dat has no such spell.
// ACE: SpellLevelCache.GetSpellLevel
#[must_use]
pub fn get_spell_level(w: &World, spell_id: i32) -> i32 {
    if let Some(&spell_level) = w
        .loot_tables
        .spell_levels
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&spell_id)
    {
        return spell_level;
    }
    let spell = Spell::from_int(w, spell_id, true);
    let spell_level =
        i32::try_from(spell.formula_ref().level()).expect("a scarab level fits an int");
    w.loot_tables
        .spell_levels
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(spell_id, spell_level);
    spell_level
}
