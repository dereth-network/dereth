// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionChangeCombatMode.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionChangeCombatMode.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_combat;
use crate::World;

// ACE: GameActionChangeCombatMode.Handle
/// Game action 0x0053: `uint newCombatMode`, then `HandleActionChangeCombatMode((CombatMode)newCombatMode)`.
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let new_combat_mode = message
        .decode::<proto::combat::CombatChangeCombatMode>()?
        .combat_mode;

    let player = session_player(w, session);
    player_combat::handle_action_change_combat_mode(
        w,
        player,
        empyrean_entity::enums::CombatMode(new_combat_mode.cs_cast()),
        false,
        None,
    );
    Ok(())
}
