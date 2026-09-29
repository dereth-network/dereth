// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionRemoveSpellFavorite.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionRemoveSpellFavorite.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_character;
use crate::World;

// ACE: GameActionRemoveSpellFavorite.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::combat::CharacterRemoveSpellFavorite>()?;
    let spell_id = m.spell_id;
    let spell_bar_id: u32 = m.spell_bank.cs_cast();

    let player = session_player(w, session);
    player_character::handle_action_remove_spell_favorite(w, player, spell_id, spell_bar_id);
    Ok(())
}
