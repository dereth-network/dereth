//! Spell research: the early clients' formula test (`Magic_TestSpellFormula`, `0x004B`), on a
//! world with `EraFeatures::spell_research`.
//!
//! ACE has no such request; this is Empyrean's (V432, V440). The test is judged in this order:
//!
//! 1. **Legality.** A formula that cannot be spoken or cast (a component that is no spell
//!    component, a gap, no scarab, or not exactly one herb, powder, potion and talisman) is
//!    refused with "You've attempted an impossible spell path!".
//! 2. **Target.** A spell's formula is aimed as a cast of that spell is; a legal formula that makes
//!    no spell is aimed by its own components, as the research-era client aimed one. A target the
//!    formula cannot take is refused with "Incorrect target type", nothing spent, no spell named.
//! 3. **Match or fizzle.** The player's own formula of a spell (the formula a cast of it asks of
//!    this account, tapers included, in order) is cast, and teaches the spell only on a successful
//!    cast; too little skill fizzles it as any cast does. A legal formula that is no spell's, or
//!    another account's, is cast and always fizzles: the words are spoken, the gestures made, a
//!    fizzle's mana and components spent, and "Your spell fizzled." told.

use empyrean_dat::file_types::spell_table as dat_spell_table;
use empyrean_entity::enums::WeenieError;
use empyrean_entity::ObjectGuid;

use crate::entity::spell::Spell;
use crate::entity::spell_formula::{spell_components_table, spell_table};
use crate::world_objects::{era_gates, player_magic, player_spells, player_use};
use crate::World;

/// The spell base's self-targeted bit.
const SELF_TARGETED: u32 = 0x0008;

/// The spell the account's formulas make from `components` (component ids, unused slots zero), if
/// any. Slots after the formula's own length must be empty.
#[must_use]
pub fn spell_of_formula(w: &World, account: &str, components: &[u32]) -> Option<u32> {
    let laid: Vec<u32> = components.iter().copied().filter(|c| *c != 0).collect();
    if laid.is_empty() || components.iter().skip(laid.len()).any(|c| *c != 0) {
        return None;
    }
    let table = spell_table(w);
    table.spells.keys().copied().find(|&id| {
        dat_spell_table::get_spell_formula(table, id, account).is_ok_and(|formula| {
            formula
                .iter()
                .copied()
                .filter(|c| *c != 0)
                .eq(laid.iter().copied())
        })
    })
}

/// Whether `components` (unused slots zero) can be spoken and cast at all: all of them spell
/// components, laid without a gap, at least one scarab and exactly one herb, one powder, one potion
/// and one talisman, the rest tapers.
#[must_use]
pub fn formula_is_legal(w: &World, components: &[u32]) -> bool {
    use dat_spell_table::component_type as ty;
    let laid: Vec<u32> = components.iter().copied().take_while(|c| *c != 0).collect();
    if laid.is_empty() || components.iter().skip(laid.len()).any(|c| *c != 0) {
        return false;
    }
    let table = spell_components_table(w);
    let Some(types) = laid
        .iter()
        .map(|c| table.components.get(c).map(|c| c.component_type))
        .collect::<Option<Vec<u32>>>()
    else {
        return false;
    };
    let count = |t: u32| types.iter().filter(|&&x| x == t).count();
    count(ty::SCARAB) >= 1
        && [ty::HERB, ty::POWDER, ty::POTION, ty::TALISMAN]
            .iter()
            .all(|&t| count(t) == 1)
        && types.iter().all(|&t| {
            [
                ty::SCARAB,
                ty::HERB,
                ty::POWDER,
                ty::POTION,
                ty::TALISMAN,
                ty::TAPER,
            ]
            .contains(&t)
        })
}

/// The target type a formula aims at by its own components, as the research-era client read it:
/// from the slot before the first empty one at or past the fifth, for a formula of at least five.
#[must_use]
pub fn formula_target_type(components: &[u32]) -> u32 {
    let mut slots = [0u32; 8];
    for (slot, c) in slots.iter_mut().zip(components) {
        *slot = *c;
    }
    if slots[..5].contains(&0) {
        return 0;
    }
    let mut i = 5;
    while i < 8 && slots[i] != 0 {
        i += 1;
    }
    dereth_rules::weenie::spell_target_type_of_component(slots[i - 1])
}

/// Whether a formula aimed at `mask` may be tested on `target`: the type check the research-era
/// client made before a cast. No type wants no target; a weapon, armour, clothing or caster
/// enchantment may be aimed at a creature, which carries it.
fn target_fits_mask(w: &World, player: ObjectGuid, target: ObjectGuid, mask: u32) -> bool {
    /// The item types an enchantment aimed at a creature is redirected from: melee and missile
    /// weapons, armour, clothing, casters.
    const REDIRECTED: u32 = 0x8107;
    /// `ItemType::Creature`.
    const CREATURE: u32 = 0x10;
    if mask == 0 {
        return true;
    }
    let Some(o) = w.objects.get(target) else {
        return false;
    };
    if target == player {
        return mask & (REDIRECTED | CREATURE) != 0;
    }
    o.item_type().0 & mask != 0 || (mask & REDIRECTED != 0 && o.is_creature())
}

/// The test: refuse it on a world without spell research, then judge it as the module says.
pub fn handle_test_spell_formula(
    w: &mut World,
    player: ObjectGuid,
    components: &[u32],
    target_guid: u32,
) {
    let research = w.era.features.spell_research;
    if !era_gates::has(w, player, research, "spell research") {
        player_use::send_use_done_event(w, player, WeenieError::None);
        return;
    }
    let account = crate::managers::player_manager::player_session(w, player)
        .and_then(|s| w.sessions.get(s))
        .and_then(|s| s.account.clone())
        .unwrap_or_default();
    if !formula_is_legal(w, components) {
        player_use::send_use_done_event(
            w,
            player,
            WeenieError::YouveAttemptedAnImpossibleSpellPath,
        );
        return;
    }
    let refuse = |w: &mut World| {
        player_use::send_use_done_event(w, player, WeenieError::IncorrectTargetType);
    };
    let Some(spell_id) = spell_of_formula(w, &account, components) else {
        let mask = formula_target_type(components);
        if mask != 0 && !target_fits_mask(w, player, ObjectGuid::new(target_guid), mask) {
            refuse(w);
            return;
        }
        let laid: Vec<u32> = components.iter().copied().take_while(|c| *c != 0).collect();
        player_magic::fizzle_research_formula(w, player, &laid);
        return;
    };
    let (bitfield, target_type) = spell_table(w)
        .spells
        .get(&spell_id)
        .map_or((0, 0), |s| (s.bitfield, s.non_component_target_type));
    let target = if bitfield & SELF_TARGETED != 0 {
        Some(player.full())
    } else if target_type == 0 {
        None
    } else {
        Some(target_guid)
    };
    if let Some(target) = target {
        let target = ObjectGuid::new(target);
        let spell = Spell::new(w, spell_id, true);
        if !w.objects.contains(target) || player_magic::is_invalid_target(w, player, &spell, target)
        {
            refuse(w);
            return;
        }
    }
    player_magic::cast_research_spell(w, player, target, spell_id);
}

/// Completes only the formula test attached to this successful cast. Known spells still send
/// their update so the client can distinguish a successful test from a refused use.
pub(crate) fn complete_successful_cast(w: &mut World, player: ObjectGuid, spell_id: u32) {
    let state = &mut player_magic::fields_mut(w, player).magic_state;
    if state.research_spell != Some(spell_id) {
        return;
    }
    state.research_spell = None;
    if !player_spells::spell_is_known(w, player, spell_id) {
        player_spells::learn_spell_with_networking(w, player, spell_id, true);
    } else if let Some(session) = crate::managers::player_manager::player_session(w, player) {
        use crate::network::game_event::events::game_event_magic_update_spell::game_event_magic_update_spell;
        use crate::network::game_event::game_event_message::session_data;
        use crate::network::game_messages::game_message::enqueue_send;
        let spell = u16::try_from(spell_id).expect("a research spell fits its update message");
        let message = game_event_magic_update_spell(session_data(w, session), spell, 0);
        enqueue_send(w, session, message);
    }
}
