// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventChannelIndex.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventChannelIndex.cs`.

use dereth_protocol::comms as proto;
use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_event::game_event_message::session_player;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::World;

// ACE: GameEventChannelIndex.GameEventChannelIndex
#[must_use]
pub fn game_event_channel_index(w: &mut World, session: SessionId) -> GameMessage {
    let mut msg = game_event_message(
        GameEventType::ChannelIndex,
        GameMessageGroup::UIQueue,
        session_data(w, session),
    );
    write_event_body(w, session, &mut msg);
    msg
}

// ACE: GameEventChannelIndex.WriteEventBody
/// **Retail's framing (V238):** exactly one count-prefixed list, as the client
/// reads it. ACE's three independent checks wrote one list per role (several lists back to back
/// for a player with several roles) and no body at all for a player with none, which the client
/// read past the end of. The list is the highest role's (admin, then arch/sentinel, then advocate:
/// each of ACE's lists contains the next), and an empty list for a player with no role. The lists'
/// contents are ACE's; retail captures may yet show different ones.
fn write_event_body(w: &World, session: SessionId, msg: &mut GameMessage) {
    // TODO: this probably could be done better but I'm not sure if there's a point, it's not like the client out of the box supports making up new channels
    let player = session_player(w, session);
    let player = w
        .objects
        .get(player)
        .expect("ACE: Session.Player is null (NullReferenceException)");

    let names: &[&str] = if player.is_admin_prop() {
        &[
            "Abuse", "Admin", "Audit", "Av1", "Av2", "Av3", "Sentinel", "Help",
        ]
    } else if player.is_arch() || player.is_sentinel_prop() {
        &["Abuse", "Audit", "Av1", "Av2", "Av3", "Sentinel", "Help"]
    } else if player.is_advocate() {
        &["Abuse", "Av1", "Av2", "Av3", "Help"]
    } else {
        &[]
    };
    let body = proto::CommunicationChannelIndexRecv {
        names: names.iter().map(|n| ace_str(*n)).collect(),
    };
    msg.write_proto(&body);
}
