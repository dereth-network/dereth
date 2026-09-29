// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionCancelAttack.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionCancelAttack.cs`.

use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_melee;
use crate::World;

// ACE: GameActionCancelAttack.Handle
/// Game action 0x01B7: no body; `HandleActionCancelAttack()`.
pub fn handle(w: &mut World, _message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let player = session_player(w, session);
    player_melee::handle_action_cancel_attack(w, player, empyrean_entity::enums::WeenieError::None);
    Ok(())
}
