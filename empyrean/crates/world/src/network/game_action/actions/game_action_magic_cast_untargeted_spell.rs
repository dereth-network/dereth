// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionMagicCastUntargetedSpell.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionMagicCastUntargetedSpell.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_magic;
use crate::World;

// ACE: GameActionMagicCastUnTargetedSpell.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::combat::MagicCastUntargetedSpell>()?;
    let spell_id = m.spell_id;

    let player = session_player(w, session);
    player_magic::handle_action_magic_cast_un_targeted_spell(w, player, spell_id);
    Ok(())
}
