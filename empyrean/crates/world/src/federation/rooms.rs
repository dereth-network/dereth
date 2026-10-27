//! Not ACE: the Global room, carried in one of the world's TurbineChat rooms, and the world's own
//! rooms mirrored into Discord, both as the hub says.
//!
//! - **Where.** The hub keeps which of the world's rooms carries the Global room, and which of
//!   its own rooms it mirrors into Discord, and tells the world at each welcome and whenever they
//!   change (`rooms`); the world applies them at once, joining and leaving the rooms on the link,
//!   and keeps the last it was told ([`KeptRooms`]), so that it starts right while the hub is away.
//!   Until a hub has told it, the world carries the Global room where `[federation] global_room`
//!   says, and mirrors none of its own rooms. A hub that holds no room for the world is sent that
//!   one, once on each link (its seed); a hub that holds one has the last word, and the setting is
//!   logged once if it differs. The operator moves it in game with `/dn admin global` (which asks
//!   the hub) or on the hub's operator page.
//! - **Out.** A line an opted-in character says in that room reaches the local listeners as any
//!   room line does, and goes on to the hub. A line in one of the world's own rooms the hub
//!   mirrors goes to the hub the same way, under its own name (`general`), and from there to
//!   Discord alone; the hub hands no line back into it, and a line a person says there from Discord
//!   comes only as the hub's `say` ([`super::say`]). A room the hub says it does not mirror keeps
//!   its lines here for a minute without sending them, so that it costs its speakers nothing.
//! - **In.** A line from another world reaches every local player who listens to the room that
//!   carries the Global room, as a `Communication_TurbineChat` (`0xF7DE`) room event under the
//!   sender's address and stand-in id (the retail client checks the id against its squelch list;
//!   the name link it draws always carries id 0). A room the operator has turned off
//!   (`chat_disable_<room>`) carries nothing from other worlds either.
//!
//! `/dng <text>` (and `/dn global <text>`) says a line in that room from wherever the character is
//! talking: the world takes it exactly as the client's own room line, so every rule of the room
//! applies to it.

use dereth_network_protocol::rooms::{OFF, OWN_ROOMS};
use dereth_network_protocol::text::{sanitize_line, sanitize_name};
use dereth_network_protocol::wire::GLOBAL_ROOM;
use dereth_network_protocol::{Address, Message, Outcome, RoomsMessage};
use dereth_protocol::turbine::{ExtraBlob, SendToRoomById};
use empyrean_entity::enums::{ChatNetworkBlobDispatchType, ChatNetworkBlobType};
use empyrean_entity::ObjectGuid;
use serde::{Deserialize, Serialize};

use super::{
    blocked, enabled, fresh_id, is_on, may_receive, may_send, now, own_address, remote_stand_in,
    send, tell_operator, tell_player, too_new, LinkState, PendingRoom, RoomSlot, Sends,
};
use crate::managers::{player_manager, property_manager};
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_turbine_chat::game_message_turbine_chat;
use crate::network::handlers::turbine_chat_handler::turbine_chat_received;
use crate::network::managers::inbound_message_manager::Payload;
use crate::world_objects::player_character;
use crate::world_objects::world_object_networking::shims;
use crate::World;

/// How long the lines of one of the world's own rooms stay here, unsent, after the hub says it
/// does not mirror the room into Discord, in seconds. The next line after it goes to the hub again,
/// so a room its operator has mirrored since is carried within the minute.
pub const UNCARRIED_SECS: f64 = 60.0;

/// The world's rooms as its hub last told them (`rooms`), kept with the network's records.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeptRooms {
    /// The room that carries the Global room (one of the world's own rooms, or `off`); none while
    /// the hub held none for the world.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global: Option<String>,
    /// The world's own rooms the hub mirrors into Discord for it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mirrored: Vec<String>,
}

/// The TurbineChat room that carries the Global room here now: where the hub last said, else where
/// the world's settings say; none carries no room.
#[must_use]
pub fn global_slot(w: &World) -> Option<RoomSlot> {
    let f = &w.federation;
    match f.store.rooms.as_ref().and_then(|r| r.global.as_deref()) {
        Some(OFF) => None,
        Some(room) => RoomSlot::parse(room).or(f.settings.global_room),
        None => f.settings.global_room,
    }
}

/// The world's own rooms whose lines go to the hub to be mirrored into Discord now, as the hub
/// last said: never the one carrying the Global room.
#[must_use]
pub fn mirrored_slots(w: &World) -> Vec<RoomSlot> {
    let global = global_slot(w);
    w.federation
        .store
        .rooms
        .iter()
        .flat_map(|r| r.mirrored.iter())
        .filter_map(|room| RoomSlot::parse(room))
        .filter(|slot| Some(*slot) != global)
        .collect()
}

/// A room as the link names where the Global room is carried: one of the world's own rooms, or
/// `off`.
fn link_word(slot: Option<RoomSlot>) -> &'static str {
    slot.map_or(OFF, RoomSlot::room)
}

/// A room as players know it, or "no room".
fn shown(slot: Option<RoomSlot>) -> &'static str {
    slot.map_or("no room", RoomSlot::name)
}

/// Where a line said in `room` goes past this world, as its speaker is told.
fn reach(room: &str) -> &'static str {
    if room == GLOBAL_ROOM {
        "the other worlds"
    } else {
        "Discord"
    }
}

/// What a speaker is told when its line stayed on this world because it sent too quickly.
fn too_quickly(room: &str) -> String {
    format!(
        "Your line did not reach {}: you are sending too quickly.",
        reach(room)
    )
}

/// What a speaker is told when its line stayed on this world because the hub refused it: the
/// speaker's own pace was kept here, so it is the world's share of the room that is spent.
fn held_back(room: &str) -> String {
    format!(
        "Your line did not reach {}: the Dereth Network is holding back this world's lines for now.",
        reach(room)
    )
}

/// What a speaker is told when its line stayed on this world because the character is younger
/// than the operator's minimum age.
fn too_new_here(room: &str) -> String {
    format!(
        "Your line did not reach {}: this character is too new to send to the Dereth Network yet.",
        reach(room)
    )
}

/// Whether the operator has turned the room off for this world.
fn disabled(w: &World, slot: RoomSlot) -> bool {
    property_manager::get_bool(w, slot.disable_property(), false, true).item
}

/// The room TurbineChat room `channel` passes on to the hub, by its name on the link: the Global
/// room, or one of the world's own rooms the hub mirrors into Discord.
fn carried(w: &World, channel: u32) -> Option<(RoomSlot, &'static str)> {
    if let Some(slot) = global_slot(w).filter(|g| g.channel() == channel) {
        return Some((slot, GLOBAL_ROOM));
    }
    mirrored_slots(w)
        .into_iter()
        .find(|r| r.channel() == channel)
        .map(|r| (r, r.room()))
}

/// At each welcome: the world's own rooms the hub last said it mirrors are joined by their names,
/// what the hub said of them before is forgotten, since this hub may mirror them now, and the hub
/// is to tell the world its rooms again.
pub(super) fn join_own(w: &mut World) {
    let f = &mut w.federation;
    f.uncarried.clear();
    f.rooms_told = false;
    f.seeded = false;
    for slot in mirrored_slots(w) {
        send(
            w,
            Message::RoomJoin {
                room: slot.room().into(),
                on: true,
            },
        );
    }
}

/// The link went: the requests not yet sent are dropped, and an operator waiting for the hub's
/// answer is told it will not come.
pub(super) fn link_down(w: &mut World) {
    w.federation.requests.clear();
    let asks: Vec<Option<ObjectGuid>> = w
        .federation
        .global_asks
        .drain()
        .map(|(_, who)| who)
        .collect();
    for who in asks {
        tell_operator(
            w,
            who,
            "The link to the Dereth Network's hub went before it answered. Type @dn admin global again once it is back.",
        );
    }
}

/// One of the rooms messages from the hub: the world's rooms are applied; a request, which only a
/// world sends, is ignored.
pub(super) fn on_message(w: &mut World, m: RoomsMessage) {
    if let RoomsMessage::Setup {
        id,
        global,
        mirrored,
    } = m
    {
        on_rooms(w, id.as_deref(), global.as_deref(), &mirrored);
    }
}

/// The hub's word on the world's rooms: the room that carries the Global room (none while the hub
/// holds none) and the own rooms it mirrors into Discord. They hold at once and are kept: the rooms
/// that stop being carried are left on the link, and those that start are joined. A hub that holds
/// no room is sent the one the world carries the Global room in now, once on each link; a hub
/// that holds one has the last word, and the settings are logged once if they differ. An
/// operator's `/dn admin global` the message answers (`id`) is told where the room is now.
fn on_rooms(w: &mut World, id: Option<&str>, global: Option<&str>, mirrored: &[String]) {
    let (global_before, mirrored_before) = (global_slot(w), mirrored_slots(w));
    let f = &mut w.federation;
    f.rooms_told = true;
    let kept = KeptRooms {
        global: global
            .map(str::to_owned)
            .or_else(|| f.store.rooms.as_ref().and_then(|r| r.global.clone())),
        mirrored: mirrored
            .iter()
            .filter(|r| OWN_ROOMS.contains(&r.as_str()))
            .cloned()
            .collect(),
    };
    if f.store.rooms.as_ref() != Some(&kept) {
        f.store.rooms = Some(kept);
        f.dirty = true;
    }
    let (global_now, mirrored_now) = (global_slot(w), mirrored_slots(w));
    if global_now != global_before {
        log::info!(
            "Dereth Network: the Global room is carried in {} now",
            shown(global_now)
        );
        if global_now.is_some() != global_before.is_some() {
            send(
                w,
                Message::RoomJoin {
                    room: GLOBAL_ROOM.into(),
                    on: global_now.is_some(),
                },
            );
        }
    }
    for slot in &mirrored_before {
        if !mirrored_now.contains(slot) {
            send(
                w,
                Message::RoomJoin {
                    room: slot.room().into(),
                    on: false,
                },
            );
        }
    }
    for slot in &mirrored_now {
        if !mirrored_before.contains(slot) {
            w.federation.uncarried.remove(slot.room());
            send(
                w,
                Message::RoomJoin {
                    room: slot.room().into(),
                    on: true,
                },
            );
        }
    }
    let f = &mut w.federation;
    let settings = f.settings.global_room;
    match global {
        None if !f.seeded => {
            f.seeded = true;
            let room = link_word(global_now);
            log::info!(
                "Dereth Network: the hub holds no room for this world's Global room; it is told {room}, as this world carries it now"
            );
            let id = fresh_id(w, 'g');
            w.federation.requests.push_back(
                RoomsMessage::Global {
                    id,
                    room: room.to_owned(),
                    seed: true,
                }
                .to_json(),
            );
        }
        Some(hub)
            if f.settings.global_room_named
                && hub != link_word(settings)
                && !f.settings_differ_logged =>
        {
            f.settings_differ_logged = true;
            log::info!(
                "Dereth Network: the hub carries this world's Global room in {hub}, so federation.global_room ({}) is not read: it was read until the hub kept a room, and goes in a later release. Move the room with @dn admin global, or on the hub's operator page",
                link_word(settings)
            );
        }
        _ => {}
    }
    if let Some(who) = id.and_then(|id| w.federation.global_asks.remove(id)) {
        let line = format!(
            "The Dereth Network's hub carries this world's Global room in {} now.",
            shown(global_now)
        );
        tell_operator(w, who, &line);
    }
}

/// Where this world carries the Global room, and whence it knows, as `/dn admin global` says it.
fn global_status(w: &World) -> String {
    let room = shown(global_slot(w));
    let from = if w
        .federation
        .store
        .rooms
        .as_ref()
        .is_some_and(|r| r.global.is_some())
    {
        "as the Dereth Network's hub keeps it"
    } else {
        "as federation.global_room says, until the Dereth Network's hub keeps a room"
    };
    format!(
        "This world carries the Global room in {room}, {from}. Type @dn admin global <general|trade|lfg|roleplay|off> to move it."
    )
}

/// What `/dn admin global` says of a room it does not know: its help line.
fn global_usage(who: Option<ObjectGuid>) -> String {
    let line = super::commands::usage("@dn admin global <");
    if who.is_some() {
        line
    } else {
        line.replacen("@dn", "dn", 1)
    }
}

/// `/dn admin global [<room>]`: where this world carries the Global room, or asks the hub to carry
/// it in `room` (or nowhere, `off`) from now on. The hub's answer goes to `who` (the console when
/// none); the line returned is the immediate one.
pub(super) fn admin_global(w: &mut World, who: Option<ObjectGuid>, args: &[&str]) -> String {
    let room = match args {
        [] => return global_status(w),
        [word] => {
            let word = word.to_ascii_lowercase();
            if word == OFF || word == "none" {
                None
            } else if let Some(slot) = RoomSlot::parse(&word) {
                Some(slot)
            } else {
                return global_usage(who);
            }
        }
        _ => return global_usage(who),
    };
    if w.federation.link != LinkState::Up {
        return "The Dereth Network's hub can't be reached now. Try again once the link is up."
            .to_owned();
    }
    if !w.federation.rooms_told {
        return "This network's hub does not keep where a world carries the Global room: this world carries it as federation.global_room says.".to_owned();
    }
    let id = fresh_id(w, 'g');
    w.federation.requests.push_back(
        RoomsMessage::Global {
            id: id.clone(),
            room: link_word(room).to_owned(),
            seed: false,
        }
        .to_json(),
    );
    w.federation.global_asks.insert(id, who);
    format!(
        "Asking the Dereth Network's hub to carry this world's Global room in {}...",
        shown(room)
    )
}

/// A local line in TurbineChat room `channel`, after it has gone to the local listeners. It goes
/// to the network when the speaker is on the network and the room carries the Global room, or is
/// one of the world's own rooms for Discord that the hub has not lately said it does not carry.
pub fn room_said(w: &mut World, player: ObjectGuid, channel: u32, text: &str) {
    if !enabled(w) {
        return;
    }
    let Some((slot, room)) = carried(w, channel) else {
        return;
    };
    if !is_on(w, player.full()) || disabled(w, slot) {
        return;
    }
    let t = now(w);
    if w.federation
        .uncarried
        .get(room)
        .is_some_and(|&until| t < until)
    {
        return;
    }
    if too_new(w, player) {
        tell_player(w, player, &too_new_here(room));
        return;
    }
    let Some(from) = own_address(w, player) else {
        return;
    };
    if !may_send(w, player, Sends::RoomLines) {
        tell_player(w, player, &too_quickly(room));
        return;
    }
    let id = fresh_id(w, 'r');
    w.federation.pending_rooms.insert(
        id.clone(),
        PendingRoom {
            player,
            room: room.to_owned(),
            sent: None,
            quiet: false,
        },
    );
    send(
        w,
        Message::RoomSay {
            id,
            room: room.into(),
            from,
            text: text.to_owned(),
            via: None,
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
    let Some(slot) = global_slot(w) else {
        tell_player(w, player, NOT_CARRIED);
        return;
    };
    let text = text.trim();
    if text.is_empty() {
        tell_player(w, player, &super::commands::usage("@dng"));
        return;
    }
    if !is_on(w, player.full()) {
        tell_player(w, player, super::NOT_ON);
        return;
    }
    let Some(session) = shims::player_session(w, player) else {
        return;
    };
    let request = SendToRoomById {
        context: 0,
        room: slot.channel(),
        text: text.to_owned(),
        extra: ExtraBlob::Speaker {
            sender: player.full(),
            chat_type: slot.chat_type().0.cast_unsigned(),
        },
    };
    let Ok(bytes) = dereth_protocol::write_blob(&request) else {
        return;
    };
    let _ = turbine_chat_received(w, &mut Payload::new(&bytes), session);
}

/// How the hub took one of this world's room lines: only a refusal is news to the speaker. A
/// `not_member` for one of the world's own rooms says the hub does not mirror it into Discord now
/// (its operator has not mirrored it, or the hub mirrors nothing): the speaker is told nothing, and
/// the room's lines stay here for [`UNCARRIED_SECS`].
pub(super) fn on_result(w: &mut World, id: &str, outcome: Outcome) {
    let Some(p) = w.federation.pending_rooms.remove(id) else {
        return;
    };
    let own = p.room != GLOBAL_ROOM;
    match outcome {
        Outcome::RateLimited => {
            if !p.quiet && player_manager::get_online_player(w, p.player.full()).is_some() {
                tell_player(w, p.player, &held_back(&p.room));
            }
        }
        Outcome::NotMember if own => {
            let until = now(w) + UNCARRIED_SECS;
            if w.federation
                .uncarried
                .insert(p.room.clone(), until)
                .is_none()
            {
                log::info!(
                    "Dereth Network: the hub does not mirror this world's room {} into Discord; its lines stay here until the world's operator mirrors it on the hub's operator page",
                    p.room
                );
            }
        }
        Outcome::Ok if own => {
            w.federation.uncarried.remove(&p.room);
        }
        _ => {}
    }
}

fn is_olthoi_player(w: &World, guid: ObjectGuid) -> bool {
    w.objects
        .get(guid)
        .and_then(|o| o.player.as_ref())
        .is_some_and(|p| p.player_properties.is_olthoi_player)
}

/// A line from another world's character in the Global room. Nothing is given (not even the
/// speaker's stand-in id) unless someone here is shown it; a line in one of the world's own rooms,
/// which nothing comes back into, is never shown. A line its speaker's person said elsewhere
/// (`via`) shows the speaker's name marked with where (`Ann@coldeve (Discord)`); the stand-in id
/// and the squelches are the address's alone.
pub(super) fn on_line(w: &mut World, room: &str, from: &Address, text: &str, via: Option<&str>) {
    let Some(slot) = global_slot(w) else {
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
    let name = match via {
        Some(via) => super::say::marked(&name, via),
        None => name,
    };
    let text = sanitize_line(text);
    if text.is_empty() {
        return;
    }
    // One world's lines are held to the room's per-world pace here too.
    let rate = w.federation.limits.room_lines_per_world;
    if !may_receive(w, from.world(), room, rate) {
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
