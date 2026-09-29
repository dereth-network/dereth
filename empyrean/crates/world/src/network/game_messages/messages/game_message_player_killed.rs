// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePlayerKilled.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePlayerKilled.cs`.

use dereth_protocol::combat as proto;
use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessagePlayerKilled.GameMessagePlayerKilled
/// The player broadcasts this when they die, including to self.
#[must_use]
pub fn game_message_player_killed(
    death_message: &str,
    victim_id: ObjectGuid,
    killer_id: ObjectGuid,
) -> GameMessage {
    // 144 is the max seen in retail pcaps
    let body = proto::CombatHandlePlayerDeathEvent {
        message: ace_str(death_message),
        killed: victim_id.into(),
        killer: killer_id.into(),
    };
    let mut msg = GameMessage::new(GameMessageOpcode::PlayerKilled, GameMessageGroup::UIQueue);
    msg.write_proto_strings(&body, &[death_message]);
    msg
}
