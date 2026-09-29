// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageAccountBanned.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageAccountBanned.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_common::dotnet::DotNetDateTime;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::World;

// ACE: GameMessageAccountBanned.GameMessageAccountBanned
/// Tells the client the player has been banned from the server. `DateTime.UtcNow` is `w.now.utc`.
///
/// **Retail's layout (V240):** the reason string is always written (empty when
/// there is none), as the client always reads it. ACE omitted it for a ban without a reason, so the
/// client read past the end of the message and showed no ban dialog.
#[must_use]
pub fn game_message_account_banned(
    w: &World,
    ban_expiration: DotNetDateTime,
    reason: Option<&str>,
) -> GameMessage {
    let mut msg = GameMessage::new(GameMessageOpcode::AccountBanned, GameMessageGroup::UIQueue);
    let ts_ban_expiration = ban_expiration - w.now.utc;
    let seconds: u32 = ts_ban_expiration.total_seconds().cs_cast();
    let body = dereth_protocol::login::LoginAccountBanned {
        expiry: seconds.cast_signed(),
        reason: ace_str(reason),
    };
    msg.write_proto_strings(&body, &[reason.unwrap_or("")]);
    msg
}
