//! Spell research: the early clients' formula test (`Magic_TestSpellFormula`, `0x004B`), on a
//! world with `EraFeatures::spell_research`.
//!
//! ACE has no such request; this is Empyrean's (V432). A formula is the player's own: the spell
//! whose formula for this account (the formula a cast of it asks for, tapers included) is the
//! laid components, in order. A formula that makes no spell is refused with "You've attempted an
//! impossible spell path!"; one that makes a spell the player does not know teaches it, as a
//! scroll does; then the spell is cast on the target through the ordinary cast, which checks and
//! consumes the components and answers the client as any cast does.

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
/// otherwise teach the spell if it is new and cast it.
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
    if !player_spells::spell_is_known(w, player, spell_id) {
        player_spells::learn_spell_with_networking(w, player, spell_id, true);
    }
    if bitfield & SELF_TARGETED != 0 {
        player_magic::handle_action_cast_targeted_spell(w, player, player.full(), spell_id, None);
    } else if target_type == 0 {
        player_magic::handle_action_magic_cast_un_targeted_spell(w, player, spell_id);
    } else {
        player_magic::handle_action_cast_targeted_spell(w, player, target_guid, spell_id, None);
    }
}
