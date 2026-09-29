// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionTargetedMeleeAttack.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionTargetedMeleeAttack.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_melee;
use crate::World;

// ACE: GameActionTargetedMeleeAttack.Handle
/// Game action 0x0008: `uint targetGuid`, `uint attackHeight`, `float powerLevel`.
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    // The three reads ACE makes, in its order; a short payload fails as its reads do.
    let m = message.decode::<proto::combat::CombatTargetedMeleeAttack>()?;
    let (target_guid, attack_height, power_level) = (m.target.0, m.attack_height, m.power_level);

    let player = session_player(w, session);
    player_melee::handle_action_targeted_melee_attack(
        w,
        player,
        target_guid,
        attack_height,
        power_level,
    );
    Ok(())
}
