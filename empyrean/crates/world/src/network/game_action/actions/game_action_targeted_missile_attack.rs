// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionTargetedMissileAttack.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionTargetedMissileAttack.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_missile;
use crate::World;

// ACE: GameActionTargetedMissileAttack.Handle
/// Game action 0x000A: `uint targetGuid`, `uint attackHeight`, `float accuracyLevel`.
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    // The three reads ACE makes, in its order; a short payload fails as its reads do.
    let m = message.decode::<proto::combat::CombatTargetedMissileAttack>()?;
    let (target_guid, attack_height, accuracy_level) = (m.target.0, m.attack_height, m.power_level);

    let player = session_player(w, session);
    player_missile::handle_action_targeted_missile_attack(
        w,
        player,
        target_guid,
        attack_height,
        accuracy_level,
    );
    Ok(())
}
