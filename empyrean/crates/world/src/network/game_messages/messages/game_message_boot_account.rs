// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageBootAccount.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageBootAccount.cs`.

use dereth_protocol::login as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessageBootAccount.GameMessageBootAccount
/// Tells the client the player has been booted from the server. `reason`: typically starts with a
/// space, because the client does not add one; with `None` the client shows " for Code of Conduct
/// Violations".
#[must_use]
pub fn game_message_boot_account(reason: Option<&str>) -> GameMessage {
    let mut msg = GameMessage::new(GameMessageOpcode::AccountBoot, GameMessageGroup::UIQueue);
    msg.write_proto_strings(
        &proto::LoginAccountBooted {
            reason: reason.map(ace_str),
        },
        &[reason.unwrap_or("")],
    );
    msg
}
