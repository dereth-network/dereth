// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionSpellbookFilter.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionSpellbookFilter.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_spells;
use crate::World;

// ACE: GameActionSpellbookFilter.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::combat::CharacterSpellbookFilterEvent>()?;
    let filters = empyrean_entity::enums::SpellBookFilterOptions(m.filter_mask);

    let player = session_player(w, session);
    player_spells::handle_spellbook_filters(w, player, filters);
    Ok(())
}
