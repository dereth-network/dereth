// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventInscriptionResponse.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventInscriptionResponse.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::items as proto;
use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_event::game_event_message::session_player;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::world_objects::world_object::WorldObject;
use crate::World;

// ACE: GameEventInscriptionResponse.GameEventInscriptionResponse
/// **Retail's layout (V382):** the object, then a dword, then the three strings,
/// as the client's `0xC3` arm reads them (it parses and discards the event). ACE wrote the player's
/// guid after the inscription; it now fills the dword before it, the client's unnamed field. No
/// server sends this event (ACE's only call site is commented out: never seen from retail).
#[must_use]
pub fn game_event_inscription_response(
    w: &mut World,
    session: SessionId,
    world_object: &WorldObject,
) -> GameMessage {
    let mut msg = game_event_message(
        GameEventType::GetInscriptionResponse,
        GameMessageGroup::UIQueue,
        session_data(w, session),
    );
    let inscription = world_object.inscription();
    let scribe_name = world_object.scribe_name();
    let scribe_account = world_object.scribe_account();
    let body = proto::ItemGetInscriptionResponse {
        object: ObjectId(world_object.guid.full()),
        unknown: session_player(w, session).full(),
        inscription: ace_str(inscription.as_deref()),
        scribe_name: ace_str(scribe_name.as_deref()),
        scribe_account: ace_str(scribe_account.as_deref()),
    };
    msg.write_proto_strings(
        &body,
        &[
            inscription.as_deref().unwrap_or(""),
            scribe_name.as_deref().unwrap_or(""),
            scribe_account.as_deref().unwrap_or(""),
        ],
    );
    msg
}
