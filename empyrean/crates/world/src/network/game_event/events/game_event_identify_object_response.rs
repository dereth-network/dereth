// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventIdentifyObjectResponse.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventIdentifyObjectResponse.cs`.

use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_event::game_event_message::session_player;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::structure::appraise_info;
use crate::sessions::SessionData;
use crate::World;

// ACE: GameEventIdentifyObjectResponse.GameEventIdentifyObjectResponse
/// `GameEventIdentifyObjectResponse(Session session, WorldObject obj, bool success)`:
/// `new AppraiseInfo(obj, session.Player, success)` and its writer.
#[must_use]
pub fn game_event_identify_object_response(
    w: &mut World,
    session: SessionId,
    obj: ObjectGuid,
    success: bool,
) -> GameMessage {
    let mut msg = game_event_message(
        GameEventType::IdentifyObjectResponse,
        GameMessageGroup::UIQueue,
        session_data(w, session),
    );
    let examiner = session_player(w, session);
    let appraise_info = appraise_info::appraise_info_new(w, obj, examiner, success);
    msg.data.write_u32(obj.full());
    appraise_info::write(&mut msg.data, &appraise_info);
    msg
}

/// `GameEventIdentifyObjectResponse(Session session, uint objectGuid)`: the empty appraisal
/// response, for when you only have a guid and nothing else.
#[must_use]
pub fn game_event_identify_object_response_empty(
    session: &mut SessionData,
    object_guid: u32,
) -> GameMessage {
    let mut msg = game_event_message(
        GameEventType::IdentifyObjectResponse,
        GameMessageGroup::UIQueue,
        session,
    );
    let appraise_info = appraise_info::appraise_info_empty();
    msg.data.write_u32(object_guid);
    appraise_info::write(&mut msg.data, &appraise_info);
    msg
}
