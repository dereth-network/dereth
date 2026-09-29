// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventAllegianceUpdate.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventAllegianceUpdate.cs`.

use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::entity::allegiance_node::NodeRef;
use crate::network::game_event::game_event_message::game_event_message_with_capacity;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::structure::allegiance_profile;
use crate::World;

// ACE: GameEventAllegianceUpdate.GameEventAllegianceUpdate
/// Returns info related to a player's monarch, patron, and vassals. `allegiance` is the
/// `Allegiance` object's guid and `node` the `AllegianceNode` (see `entity/allegiance_node.rs`).
/// 512 bytes: 398 is the average seen in retail pcaps, 1,040 is the max seen in retail pcaps.
#[must_use]
pub fn game_event_allegiance_update(
    w: &mut World,
    session: SessionId,
    allegiance: Option<ObjectGuid>,
    node: Option<NodeRef>,
) -> GameMessage {
    game_event_allegiance_update_with_view(w, session, allegiance, node).0
}

/// Not ACE's (retail, V289; the retail captures): the update, and the player's view of
/// the allegiance it shows ([`allegiance_view_snapshot`]), which the server keeps to tell when the
/// view changes.
#[must_use]
pub fn game_event_allegiance_update_with_view(
    w: &mut World,
    session: SessionId,
    allegiance: Option<ObjectGuid>,
    node: Option<NodeRef>,
) -> (GameMessage, Vec<u8>) {
    let mut msg = game_event_message_with_capacity(
        GameEventType::AllegianceUpdate,
        GameMessageGroup::UIQueue,
        session_data(w, session),
        512,
    );

    // uint - rank - this player's rank within their allegiance
    // AllegianceProfile - prof
    let rank = crate::world_objects::allegiance::node_rank(w, node);
    //Console.WriteLine("Rank: " + rank);
    msg.data.write_u32(rank);

    let prof = allegiance_profile::allegiance_profile_new(w, allegiance, node);
    allegiance_profile::write(&mut msg.data, &prof);

    //Console.WriteLine("Allegiance bytes written: " + totalBytes);
    let view = snapshot(rank, &prof);
    (msg, view)
}

/// Not ACE's (retail, V289; the retail captures): the player's view of the allegiance, as
/// the update's body (rank and profile) with the fields that move with the clock cleared. Two
/// snapshots are equal when an update would show the player nothing new.
#[must_use]
pub fn allegiance_view_snapshot(
    w: &mut World,
    allegiance: Option<ObjectGuid>,
    node: Option<NodeRef>,
) -> Vec<u8> {
    let rank = crate::world_objects::allegiance::node_rank(w, node);
    let prof = allegiance_profile::allegiance_profile_new(w, allegiance, node);
    snapshot(rank, &prof)
}

fn snapshot(rank: u32, prof: &allegiance_profile::AllegianceProfile) -> Vec<u8> {
    let mut view = Vec::new();
    view.write_u32(rank);
    allegiance_profile::write(&mut view, &allegiance_profile::without_clock_fields(prof));
    view
}
