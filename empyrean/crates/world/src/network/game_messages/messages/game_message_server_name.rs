// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageServerName.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageServerName.cs`.

use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessageServerName.GameMessageServerName
/// ACE's defaults: `currentConnections = 0`, `maxConnections = -1`.
#[must_use]
pub fn game_message_server_name(
    server_name: &str,
    current_connections: i32,
    max_connections: i32,
) -> GameMessage {
    // 28 is the max seen in retail pcaps
    let mut msg = GameMessage::new(GameMessageOpcode::ServerName, GameMessageGroup::UIQueue);
    msg.write_proto_strings(
        &dereth_protocol::login::LoginWorldInfo {
            connections: current_connections,
            max_connections,
            world_name: ace_str(server_name),
        },
        &[server_name],
    );
    msg
}
