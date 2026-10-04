//! Spell research: the early clients' formula test (`Magic_TestSpellFormula`, `0x004B`), on a
//! world with `EraFeatures::spell_research`.
//!
//! ACE has no such request; this is Empyrean's (V432). A formula is the player's own: the spell
//! whose formula for this account (the formula a cast of it asks for, tapers included) is the
//! laid components, in order. A formula that makes no spell is refused with "You've attempted an
//! impossible spell path!". A recognized formula follows normal target and cast checks. Only a
//! successful cast teaches the spell and acknowledges the tested formula.

use empyrean_dat::file_types::spell_table as dat_spell_table;
use empyrean_entity::enums::WeenieError;
use empyrean_entity::ObjectGuid;

use crate::entity::spell_formula::spell_table;
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

/// The test: refuse it on a world without spell research or for a formula that makes no spell;
/// otherwise cast it and teach it only on success.
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
    let Some(spell_id) = spell_of_formula(w, &account, components) else {
        player_use::send_use_done_event(
            w,
            player,
            WeenieError::YouveAttemptedAnImpossibleSpellPath,
        );
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
