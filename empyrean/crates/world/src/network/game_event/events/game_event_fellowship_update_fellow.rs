// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventFellowshipUpdateFellow.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventFellowshipUpdateFellow.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::social::{self as proto, Fellow};
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::FellowUpdateType;
use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::network::game_event::events::game_event_fellowship_full_update::fellow_record;
use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::{ace_str, GameMessage};
use crate::World;

// ACE: GameEventFellowshipUpdateFellow.GameEventFellowshipUpdateFellow
/// 84 is the max seen in retail pcaps (the capacity is only a hint). `fellowUpdateType` defaults
/// to `FellowUpdateType.Full` in ACE.
#[must_use]
pub fn game_event_fellowship_update_fellow(
    w: &mut World,
    session: SessionId,
    player: ObjectGuid,
    share_loot: bool,
    fellow_update_type: FellowUpdateType,
) -> GameMessage {
    let session = session_data(w, session);
    let mut msg = game_event_message(
        GameEventType::FellowshipUpdateFellow,
        GameMessageGroup::UIQueue,
        session,
    );

    let r = fellow_record(w, player);

    // Information about fellow being added
    // TODO: move this to Fellow network structure
    let fellow = Fellow {
        cp_cache: 0,  // cpCached - Perhaps cp stored up before distribution?
        lum_cache: 0, // lumCached - Perhaps luminance stored up before distribution?
        level: r.level.cast_unsigned(),
        max_health: r.max[0],
        max_stamina: r.max[1],
        max_mana: r.max[2],
        current_health: r.current[0],
        current_stamina: r.current[1],
        current_mana: r.current[2],
        share_loot: i32::from(share_loot) << 1,
        name: ace_str(r.name.as_str()),
    };

    let update_type: u32 = fellow_update_type.0.cs_cast();
    let body = proto::FellowshipUpdateFellow {
        fellow_id: ObjectId(player.full()),
        fellow,
        update_type,
    };
    msg.write_proto_strings(&body, &[r.name.as_str()]);
    msg
}
