//! Not ACE: what a player's spellbook cast is worth on a world with
//! [`EraFeatures::spell_charge`](empyrean_common::era::EraFeatures) or `spell_economy`.
//!
//! DIVERGE: V502. A character's charge on a spell is kept in the spell book's value for that spell,
//! which a player's book otherwise holds at 2.0 and never reads, so it is saved with the character.
//! It regains charge only while the character is in the world, because the save freezes it at
//! logout (ac1999's choice, not a known retail rule).

use std::collections::HashMap;

use dereth_rules::spell_charge;
use empyrean_entity::ObjectGuid;

use crate::world_objects::player_magic;
use crate::World;

/// What one cast is worth, read when it is launched.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpellPotency {
    pub charge: f32,
    pub economy: f32,
}

/// The band a cast's duration stays in.
const DURATION_BAND: (f64, f64) = (0.75, 1.25);

impl SpellPotency {
    /// The two multiply.
    #[must_use]
    pub fn effect(self) -> f64 {
        f64::from(self.charge) * f64::from(self.economy)
    }

    /// Damage, a boost or harm, what a transfer gives.
    #[must_use]
    pub fn magnitude(self, amount: f64) -> f64 {
        amount * self.effect()
    }

    /// What a transfer takes: never more than the spell's own amount.
    #[must_use]
    pub fn transfer_drain(self, amount: f64) -> f64 {
        amount * self.effect().min(1.0)
    }

    /// An enchantment's duration, kept in the band. One with no time of its own has no band.
    #[must_use]
    pub fn duration(self, base: f64) -> f64 {
        if base <= 0.0 {
            return base;
        }
        (base * self.effect()).clamp(base * DURATION_BAND.0, base * DURATION_BAND.1)
    }
}

/// A character's charge state that is not saved: when each spell's stored charge was last brought
/// up to date. Time before the character came into the world does not count.
#[derive(Debug, Default)]
pub struct ChargeClock {
    online_since: f64,
    settled_at: HashMap<u32, f64>,
}

/// The stored value as a charge: a spell the character has not cast holds the book's 2.0.
fn stored_charge(w: &World, player: ObjectGuid, spell: u32) -> Option<f32> {
    w.objects
        .get(player)?
        .biota
        .properties_spell_book
        .as_ref()?
        .get(&spell.cast_signed())
        .map(|v| v.clamp(spell_charge::MIN, spell_charge::MAX))
}

/// The charge on a spell the player knows, regained to now; `None` for one it does not know.
#[must_use]
pub fn current_charge(w: &World, player: ObjectGuid, spell: u32) -> Option<f32> {
    let stored = stored_charge(w, player, spell)?;
    let clock = &player_magic::fields(w, player).charge_clock;
    let since = clock
        .settled_at
        .get(&spell)
        .copied()
        .unwrap_or(0.0)
        .max(clock.online_since);
    Some(spell_charge::regenerated(stored, w.now.unix_time - since))
}

/// The character came into the world: charge regains from now.
pub fn on_enter_world(w: &mut World, player: ObjectGuid) {
    let now = w.now.unix_time;
    let clock = &mut player_magic::fields_mut(w, player).charge_clock;
    clock.online_since = now;
    clock.settled_at.clear();
}

fn store(w: &mut World, player: ObjectGuid, spell: u32, charge: f32) {
    let now = w.now.unix_time;
    player_magic::fields_mut(w, player)
        .charge_clock
        .settled_at
        .insert(spell, now);
    let Some(o) = w.objects.get_mut(player) else {
        return;
    };
    if let Some(value) = o
        .biota
        .properties_spell_book
        .as_mut()
        .and_then(|b| b.get_mut(&spell.cast_signed()))
    {
        // A full charge stays as the book's own value, so a character who never spent one saves
        // what it always did.
        *value = if charge >= spell_charge::MAX {
            2.0
        } else {
            charge
        };
    }
    crate::world_objects::player_spells::set_changes_detected(w, player);
}

/// A successful spellbook cast spends the spell's charge.
pub fn spend_charge(w: &mut World, player: ObjectGuid, spell: u32) {
    if let Some(charge) = current_charge(w, player, spell) {
        store(w, player, spell, spell_charge::decayed(charge));
    }
}

/// The character is leaving: what it regained stays with it.
pub fn settle_charges(w: &mut World, player: ObjectGuid) {
    if !w.era.features.spell_charge {
        return;
    }
    let spells: Vec<u32> = w
        .objects
        .get(player)
        .and_then(|o| o.biota.properties_spell_book.as_ref())
        .map(|b| b.iter().map(|(k, _)| k.cast_unsigned()).collect())
        .unwrap_or_default();
    for spell in spells {
        if let Some(charge) = current_charge(w, player, spell) {
            store(w, player, spell, charge);
        }
    }
}

/// What a spellbook cast of `spell` is worth now; `None` on a world where casts are all alike.
#[must_use]
pub fn capture(w: &World, player: ObjectGuid, spell: u32) -> Option<SpellPotency> {
    let features = w.era.features;
    if !features.spell_charge && !features.spell_economy {
        return None;
    }
    Some(SpellPotency {
        charge: if features.spell_charge {
            current_charge(w, player, spell).unwrap_or(spell_charge::MAX)
        } else {
            1.0
        },
        economy: if features.spell_economy {
            w.spell_economy.value(spell, w.now.unix_time)
        } else {
            1.0
        },
    })
}

/// A successful spellbook cast spends the world's value on the spell and says how strong the magic
/// felt: the line follows the value the cast used, after the spell's words.
pub fn after_successful_cast(
    w: &mut World,
    player: ObjectGuid,
    spell: &crate::entity::spell::Spell,
    used: SpellPotency,
) {
    if !w.era.features.spell_economy {
        return;
    }
    let now = w.now.unix_time;
    let active = usize::try_from(crate::managers::player_manager::get_online_count(w)).unwrap_or(1);
    let weight = spell.spell_economy_mod();
    w.spell_economy.spend(spell.id(), weight, active, now);
    let text = crate::world_objects::spell_economy::line(used.economy);
    player_magic::send_system_chat(
        w,
        player,
        text,
        empyrean_entity::enums::ChatMessageType::Magic,
    );
}

/// A number scaled by the potency of the cast underway, if any.
#[must_use]
pub fn scaled(w: &World, amount: f64) -> f64 {
    w.cast_potency.map_or(amount, |p| p.magnitude(amount))
}
