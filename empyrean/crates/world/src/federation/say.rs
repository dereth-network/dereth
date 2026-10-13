//! Not ACE: a line one of this world's characters says from elsewhere, at the hub's asking
//! (`docs/network/02-link-protocol.md` section 9): the character's person said it in a Discord
//! channel that mirrors a room this world carries, as that character.
//!
//! The hub asks `say`; the world says the line as the character, whether or not it is online,
//! only under the rules a line the character says itself meets: the character is this world's
//! own, on the network and linked by itself (it asked for its code), its account not banned, the
//! character not gagged and old enough by the operator's rules, the room carried here and not
//! turned off, the character one the room lets speak (an Olthoi player may use none of the rooms
//! a line from Discord can be said in, nor may a character below the level the operator asks of
//! their speakers), and its pace of room lines not spent. Otherwise the world answers `refused` with
//! the reason, and shows and sends nothing.
//!
//! A line it says goes to the room's local listeners as the room's own line from the character,
//! under its own id (so a listener's squelch of the character holds) and its name marked with
//! where it was said (`Ann (Discord)`), and to the room's chat log where the operator keeps one,
//! as that line; then to the hub as a `room.say` carrying the same mark,
//! under every rule a line of the room meets, so that it reaches the other worlds and every mirror
//! of the room. The character is told nothing: its player said it.
//!
//! The answer is owed to the hub, which asked, and waits with the answers this world owes: under
//! a key no world's slug can be ([`HUB`]), at most [`super::MAX_ANSWERS_PER_WORLD`] of them (a say
//! past that is not answered, and the hub tells the person so), dropped once the hub has stopped
//! waiting for it, or when the link goes.

use dereth_network_protocol::say::{reason, SayMessage, SayOutcome};
use dereth_network_protocol::text::sanitize_line;
use dereth_network_protocol::wire::GLOBAL_ROOM;
use dereth_network_protocol::{Address, Message};
use empyrean_entity::enums::{
    ChatMessageType, ChatNetworkBlobDispatchType, ChatNetworkBlobType, HeritageGroup, PropertyBool,
    PropertyInt,
};
use empyrean_entity::ObjectGuid;

use super::{
    fresh_id, is_on, may_owe, may_send, owe, send, LinkState, PendingRoom, RoomSlot, Sends,
};
use crate::entity::i_player::{self, IPlayer};
use crate::managers::{player_manager, property_manager};
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_turbine_chat::game_message_turbine_chat;
use crate::world_objects::managers::squelch_manager;
use crate::world_objects::player_character;
use crate::world_objects::world_object_networking::shims;
use crate::World;

/// The key the answers owed to the hub's own requests wait under, among the answers owed to each
/// other world: no world's slug, which is letters, digits and hyphens.
pub(super) const HUB: &str = "@hub";

/// A name marked with where its line was said: `Ann (Discord)` for a line said in Discord, and
/// for a place this version does not name, its word with its first letter capitalized.
#[must_use]
pub fn marked(name: &str, via: &str) -> String {
    let place = if via == dereth_network_protocol::say::VIA_DISCORD {
        "Discord".to_owned()
    } else {
        let mut c = via.chars();
        c.next()
            .map(|first| first.to_uppercase().chain(c).collect())
            .unwrap_or_default()
    };
    format!("{name} ({place})")
}

/// One of the say messages from the hub: a say is answered; an answer, which only a world sends,
/// is ignored.
pub(super) fn on_message(w: &mut World, m: SayMessage) {
    let SayMessage::Ask {
        id,
        room,
        who,
        text,
        via,
    } = m
    else {
        return;
    };
    if w.federation.link != LinkState::Up {
        return;
    }
    if !may_owe(w, HUB) {
        log::debug!("Dereth Network: the hub is owed its share of answers; a say is not taken");
        return;
    }
    let refused = say(w, &room, &who, &text, &via).err();
    let answer = SayMessage::Result {
        id,
        outcome: if refused.is_some() {
            SayOutcome::Refused
        } else {
            SayOutcome::Said
        },
        reason: refused.flatten().map(str::to_owned),
    };
    owe(w, HUB, answer.to_json());
}

/// Says `text` as `who` in `room`, marked `via`; the reason it is refused otherwise (none for one
/// this version gives no word for).
fn say(
    w: &mut World,
    room: &str,
    who: &Address,
    text: &str,
    via: &str,
) -> Result<(), Option<&'static str>> {
    let slug = w.federation.settings.slug.clone();
    if who.world() != slug {
        return Err(Some(reason::NO_CHARACTER));
    }
    let (Some(player), _) = player_manager::find_by_name(w, who.name()) else {
        return Err(Some(reason::NO_CHARACTER));
    };
    if i_player::is_deleted(w, player) || i_player::is_pending_deletion(w, player) {
        return Err(Some(reason::NO_CHARACTER));
    }
    let guid = player.guid();
    let Some(name) = i_player::name(w, player) else {
        return Err(Some(reason::NO_CHARACTER));
    };
    if !is_on(w, guid.full()) {
        return Err(Some(reason::NOT_ON_NETWORK));
    }
    // A link holds only for the character that asked for its code: a person linked to an earlier
    // character of this name speaks for no later one.
    if !w
        .federation
        .store
        .characters
        .get(&guid.full())
        .is_some_and(|r| r.asked_link)
    {
        return Err(Some(reason::NOT_LINKED));
    }
    if banned(w, player) {
        return Err(Some(reason::BANNED));
    }
    if i_player::get_property(w, player, PropertyBool::IsGagged).unwrap_or(false) {
        return Err(Some(reason::GAGGED));
    }
    if super::too_new_player(w, player) || too_new_for_rooms(w, player) {
        return Err(Some(reason::TOO_NEW));
    }
    let Some(slot) = carried(w, room) else {
        return Err(Some(reason::ROOM_OFF));
    };
    // The rooms open to every player are closed to an Olthoi player, and to a character below the
    // level the operator asks of their speakers, in game and so from Discord.
    if is_olthoi(w, player) || below_room_level(w, player) {
        return Err(Some(reason::NOT_ALLOWED));
    }
    let text = sanitize_line(text);
    if text.is_empty() {
        return Err(None);
    }
    let Ok(from) = Address::new(&name, &slug) else {
        return Err(Some(reason::NO_CHARACTER));
    };
    if !may_send(w, guid, Sends::RoomLines) {
        return Err(Some(reason::RATE_LIMITED));
    }
    let shown_as = marked(&name, via);
    show(w, player, slot, &shown_as, &text);
    crate::network::handlers::turbine_chat_handler::log_turbine_chat(
        w,
        slot.channel(),
        &shown_as,
        &text,
        guid.full(),
        slot.chat_type(),
    );
    let id = fresh_id(w, 'r');
    w.federation.pending_rooms.insert(
        id.clone(),
        PendingRoom {
            player: guid,
            room: room.to_owned(),
            sent: None,
            quiet: true,
        },
    );
    send(
        w,
        Message::RoomSay {
            id,
            room: room.to_owned(),
            from,
            text,
            via: Some(via.to_owned()),
        },
    );
    Ok(())
}

/// The TurbineChat room that carries `room` here, while it may carry a line: the Global room's
/// slot for `global`, or one of the world's own rooms the hub mirrors into Discord; none for a
/// room this world does not carry, one the operator has turned off, or with the world's
/// TurbineChat rooms off or echoing a line to its speaker alone.
fn carried(w: &World, room: &str) -> Option<RoomSlot> {
    let slot = if room == GLOBAL_ROOM {
        super::global_slot(w)?
    } else {
        super::mirrored_slots(w)
            .into_iter()
            .find(|r| r.room() == room)?
    };
    let on = |key: &str| property_manager::get_bool(w, key, false, true).item;
    let open = on("use_turbine_chat") && !on("chat_echo_only") && !on(slot.disable_property());
    open.then_some(slot)
}

/// Whether the character's account is banned now, as the account store has it (a ban made since
/// the character was loaded counts).
fn banned(w: &World, p: IPlayer) -> bool {
    let Some(account) = i_player::account(w, p) else {
        return false;
    };
    let now = w.now.utc;
    let current = w
        .auth
        .lock()
        .get_account_by_id(account.account_id)
        .unwrap_or(account);
    current.ban_expire_time.is_some_and(|until| now < until)
}

/// Whether the world's own rules for its chat rooms keep the character from them for its age: its
/// account not fifteen days old, or younger than the operator asks, or the character not played as
/// long as the operator asks of a speaker there.
fn too_new_for_rooms(w: &World, p: IPlayer) -> bool {
    let long = |key: &str| property_manager::get_long(w, key, 0, true).item;
    if property_manager::get_bool(w, "chat_requires_account_15days", false, true).item
        && !i_player::get_property(w, p, PropertyBool::Account15Days).unwrap_or(false)
    {
        return true;
    }
    let account_secs = long("chat_requires_account_time_seconds");
    if account_secs > 0 {
        let young = i_player::account(w, p).is_some_and(|a| {
            #[allow(clippy::cast_precision_loss)]
            let young = (w.now.utc - a.create_time).total_seconds() < account_secs as f64;
            young
        });
        if young {
            return true;
        }
    }
    let min_age = long("chat_requires_player_age");
    min_age > 0
        && i_player::get_property(w, p, PropertyInt::Age).is_some_and(|v| i64::from(v) < min_age)
}

/// Whether the character is below the level the operator asks of a speaker in the world's chat
/// rooms.
fn below_room_level(w: &World, p: IPlayer) -> bool {
    let min = property_manager::get_long(w, "chat_requires_player_level", 0, true).item;
    min > 0 && i_player::level(w, p).is_some_and(|level| i64::from(level) < min)
}

/// Shows the line to the room's local listeners as the room's own line from the character (`p`),
/// under its own id and the marked `name`: each listener who listens to the room, is not an
/// olthoi, and does not squelch the character.
fn show(w: &mut World, p: IPlayer, slot: RoomSlot, name: &str, text: &str) {
    let speaker = p.guid();
    let listeners: Vec<ObjectGuid> = player_manager::get_all_online(w)
        .into_iter()
        .filter(|&recipient| {
            player_character::get_character_option(w, recipient, slot.listen_option())
                && !is_olthoi_player(w, recipient)
                && !squelches(w, recipient, p)
        })
        .collect();
    if listeners.is_empty() {
        return;
    }
    let msg = game_message_turbine_chat(
        ChatNetworkBlobType::NETBLOB_EVENT_BINARY,
        ChatNetworkBlobDispatchType::ASYNCMETHOD_SENDTOROOMBYNAME,
        slot.channel(),
        name,
        text,
        speaker.full(),
        slot.chat_type(),
    );
    for recipient in listeners {
        if let Some(s) = shims::player_session(w, recipient) {
            enqueue_send(w, s, msg.clone());
        }
    }
}

/// Whether the character is an Olthoi player, by its heritage as the world keeps it, online or not.
fn is_olthoi(w: &World, p: IPlayer) -> bool {
    i_player::heritage(w, p).is_some_and(|h| {
        let h = HeritageGroup(h);
        h == HeritageGroup::Olthoi || h == HeritageGroup::OlthoiAcid
    })
}

fn is_olthoi_player(w: &World, guid: ObjectGuid) -> bool {
    w.objects
        .get(guid)
        .and_then(|o| o.player.as_ref())
        .is_some_and(|p| p.player_properties.is_olthoi_player)
}

/// Whether `recipient` squelches the character `p` in every room, as a local room line from it
/// would be held back: its account, the character itself, or everyone. An online character is
/// asked of the squelches as any speaker is; one not online by its account's name and its id.
fn squelches(w: &World, recipient: ObjectGuid, p: IPlayer) -> bool {
    if p.is_online() {
        return squelch_manager::squelches_contains(
            w,
            recipient,
            Some(p.guid()),
            ChatMessageType::AllChannels,
        );
    }
    let db = squelch_manager::squelches(w, recipient);
    let mask = ChatMessageType::AllChannels.to_mask();
    let by_account =
        i_player::account(w, p).is_some_and(|a| db.accounts.contains_key(&a.account_name));
    let by_character = db
        .characters
        .get(&p.guid().full())
        .is_some_and(|i| i.filters.first().is_some_and(|f| f.contains(mask)));
    let everyone = db.globals.filters.first().is_some_and(|f| f.contains(mask));
    by_account || by_character || everyone
}
