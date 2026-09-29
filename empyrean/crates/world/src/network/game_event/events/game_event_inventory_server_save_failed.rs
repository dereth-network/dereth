// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventInventoryServerSaveFailed.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventInventoryServerSaveFailed.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::objects as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::WeenieError;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventInventoryServerSaveFailed.GameEventInventoryServerSaveFailed
/// `errorType` defaults to `WeenieError.None` in ACE.
#[must_use]
pub fn game_event_inventory_server_save_failed(
    session: &mut SessionData,
    item_guid: u32,
    error_type: WeenieError,
) -> GameMessage {
    // `reason`: client doesn't show this error mostly, and defaults to specific error messages,
    // depending on the item name + action
    // there are some exceptions, such as WeenieError.ActionCancelled being appended
    game_event_from_proto(
        GameEventType::InventoryServerSaveFailed,
        GameMessageGroup::UIQueue,
        session,
        &proto::CharacterServerSaysAttemptFailed {
            object: ObjectId(item_guid),
            reason: error_type.0.cs_cast(),
        },
    )
}
