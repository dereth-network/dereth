// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionRemoveShortcut.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionRemoveShortcut.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_character;
use crate::World;

// ACE: GameActionRemoveShortcut.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::login::CharacterRemoveShortCut>()?;
    let index = m.index;

    let player = session_player(w, session);
    player_character::handle_action_remove_shortcut(w, player, index);
    Ok(())
}
