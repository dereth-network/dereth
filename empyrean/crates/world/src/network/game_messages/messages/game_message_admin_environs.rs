// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageAdminEnvirons.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageAdminEnvirons.cs`.

use empyrean_entity::enums::EnvironChangeType;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessageAdminEnvirons.GameMessageAdminEnvirons
/// `environChange` defaults to `EnvironChangeType.Clear` in ACE; the unused `Session` is dropped.
#[must_use]
pub fn game_message_admin_environs(environ_change: EnvironChangeType) -> GameMessage {
    GameMessage::from_proto(
        GameMessageOpcode::AdminEnvirons,
        GameMessageGroup::UIQueue,
        &dereth_protocol::admin::AdminEnvirons {
            environ_option: environ_change.0,
        },
    )
}
