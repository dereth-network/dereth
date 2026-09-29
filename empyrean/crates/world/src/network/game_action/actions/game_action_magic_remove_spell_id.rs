// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionMagicRemoveSpellId.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionMagicRemoveSpellId.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_spells;
use crate::World;

// ACE: GameActionMagicRemoveSpellId.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::qualities::MagicRemoveSpell>()?;
    let spell_id = m.layered_spell_id;

    let player = session_player(w, session);
    player_spells::handle_action_magic_remove_spell_id(w, player, spell_id);
    Ok(())
}
