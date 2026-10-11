//! Not ACE: the Global room, carried in one TurbineChat room of the world's choosing.
//!
//! A line an opted-in character says in that room reaches the local listeners as any room line
//! does, and goes on to the hub. A line from another world reaches every local player who listens
//! to that room, as a `Communication_TurbineChat` (`0xF7DE`) room event under the sender's address
//! and stand-in id (the retail client checks the id against its squelch list; the name link it
//! draws always carries id 0). A room the operator has turned off (`chat_disable_<room>`) carries
//! nothing from other worlds either.
//!
//! `/dng <text>` (and `/dn global <text>`) says a line in that room from wherever the character is
//! talking: the world takes it exactly as the client's own room line, so every rule of the room
//! applies to it.

use dereth_network_protocol::text::{sanitize_line, sanitize_name};
use dereth_network_protocol::wire::GLOBAL_ROOM;
use dereth_network_protocol::{Address, Message, Outcome};
use dereth_protocol::turbine::SendToRoomById;
use empyrean_entity::enums::{ChatNetworkBlobDispatchType, ChatNetworkBlobType};
use empyrean_entity::ObjectGuid;

use super::{
    blocked, enabled, fresh_id, is_on, may_receive, may_send, own_address, remote_stand_in, send,
    tell_player, too_new, PendingRoom, RoomSlot, Sends,
};
use crate::managers::{player_manager, property_manager};
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_turbine_chat::game_message_turbine_chat;
use crate::network::handlers::turbine_chat_handler::turbine_chat_received;
use crate::network::managers::inbound_message_manager::Payload;
use crate::world_objects::player_character;
use crate::world_objects::world_object_networking::shims;
use crate::World;

/// What a speaker is told when its line stayed on this world because it sent too quickly.
const TOO_QUICKLY: &str = "Your line did not reach the other worlds: you are sending too quickly.";

/// What a speaker is told when its line stayed on this world because the hub refused it: the
/// speaker's own pace was kept here, so it is the world's share of the room that is spent.
const HELD_BACK: &str = "Your line did not reach the other worlds: the Dereth Network is holding back this world's lines for now.";

/// Whether the operator has turned the room off for this world.
fn disabled(w: &World, slot: RoomSlot) -> bool {
    property_manager::get_bool(w, slot.disable_property(), false, true).item
}

/// A local line in TurbineChat room `channel`, after it has gone to the local listeners. It goes
/// to the network when the room carries the Global room and the speaker is on the network.
pub fn room_said(w: &mut World, player: ObjectGuid, channel: u32, text: &str) {
    if !enabled(w) {
        return;
    }
    let Some(slot) = w.federation.settings.global_room else {
        return;
    };
    if slot.channel() != channel || !is_on(w, player.full()) || disabled(w, slot) {
        return;
    }
    if too_new(w, player) {
        tell_player(
            w,
            player,
            "Your line did not reach the other worlds: this character is too new to send to the Dereth Network yet.",
        );
        return;
    }
    let Some(from) = own_address(w, player) else {
        return;
    };
    if !may_send(w, player, Sends::RoomLines) {
        tell_player(w, player, TOO_QUICKLY);
        return;
    }
    let id = fresh_id(w, 'r');
    w.federation
        .pending_rooms
        .insert(id.clone(), PendingRoom { player, sent: None });
    send(
        w,
        Message::RoomSay {
            id,
            room: GLOBAL_ROOM.into(),
            from,
            text: text.to_owned(),
        },
    );
}

/// What `/dng` says on a world that carries no Global room.
pub const NOT_CARRIED: &str = "This world does not carry the Global room.";

/// `/dng <text>`: `player` says `text` in the room that carries the Global room, as if the client
/// had sent it there. The world's own room handling shows it to the local listeners and, through
/// [`room_said`], passes it to the network under every rule a line said in the room meets. The
/// request carries context 0, which the client never gives a request of its own, so the answer the
/// room sends for it matches nothing the client is waiting on.
pub fn say_global(w: &mut World, player: ObjectGuid, text: &str) {
    let Some(slot) = w.federation.settings.global_room else {
        tell_player(w, player, NOT_CARRIED);
        return;
    };
    let text = text.trim();
    if text.is_empty() {
        tell_player(w, player, "/dng <text>: say a line in the Global room.");
        return;
    }
    if !is_on(w, player.full()) {
        tell_player(
            w,
            player,
            "You are not on the Dereth Network. Type /dn on to join it.",
        );
        return;
    }
    let Some(session) = shims::player_session(w, player) else {
        return;
    };
    let request = SendToRoomById {
        context: 0,
        room: slot.channel(),
        text: text.to_owned(),
        sender: player.full(),
        chat_type: slot.chat_type().0.cast_unsigned(),
    };
    let Ok(bytes) = dereth_protocol::write_blob(&request) else {
        return;
    };
    let _ = turbine_chat_received(w, &mut Payload::new(&bytes), session);
}

/// How the hub took one of this world's room lines: only a refusal is news to the speaker.
pub(super) fn on_result(w: &mut World, id: &str, outcome: Outcome) {
    let Some(p) = w.federation.pending_rooms.remove(id) else {
        return;
    };
    if outcome == Outcome::RateLimited
        && player_manager::get_online_player(w, p.player.full()).is_some()
    {
        tell_player(w, p.player, HELD_BACK);
    }
}

fn is_olthoi_player(w: &World, guid: ObjectGuid) -> bool {
    w.objects
        .get(guid)
        .and_then(|o| o.player.as_ref())
        .is_some_and(|p| p.player_properties.is_olthoi_player)
}

/// A line from another world's character in the Global room. Nothing is given (not even the
/// speaker's stand-in id) unless someone here is shown it.
pub(super) fn on_line(w: &mut World, room: &str, from: &Address, text: &str) {
    let Some(slot) = w.federation.settings.global_room else {
        return;
    };
    if room != GLOBAL_ROOM
        || blocked(w, from)
        || from.world() == w.federation.settings.slug
        || disabled(w, slot)
    {
        return;
    }
    let Ok(name) = sanitize_name(&from.to_string()) else {
        return;
    };
    let text = sanitize_line(text);
    if text.is_empty() {
        return;
    }
    // One world's lines are held to the room's per-world pace here too.
    let rate = w.federation.limits.room_lines_per_world;
    if !may_receive(w, from.world(), GLOBAL_ROOM, rate) {
        return;
    }
    let listeners: Vec<ObjectGuid> = player_manager::get_all_online(w)
        .into_iter()
        .filter(|&recipient| {
            player_character::get_character_option(w, recipient, slot.listen_option())
                && !is_olthoi_player(w, recipient)
                && !w
                    .federation
                    .store
                    .characters
                    .get(&recipient.full())
                    .is_some_and(|r| {
                        r.squelched.contains_key(&from.key())
                            || r.squelched_worlds.contains(from.world())
                    })
        })
        .collect();
    if listeners.is_empty() {
        return;
    }
    let Ok(sender_id) = remote_stand_in(w, from) else {
        return;
    };
    let msg = game_message_turbine_chat(
        ChatNetworkBlobType::NETBLOB_EVENT_BINARY,
        ChatNetworkBlobDispatchType::ASYNCMETHOD_SENDTOROOMBYNAME,
        slot.channel(),
        &name,
        &text,
        sender_id,
        slot.chat_type(),
    );
    for recipient in listeners {
        if let Some(s) = shims::player_session(w, recipient) {
            enqueue_send(w, s, msg.clone());
        }
    }
}
