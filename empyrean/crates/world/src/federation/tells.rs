//! Not ACE: tells to and from other worlds' characters, and channel lines.
//!
//! A local player's tell to an address goes to the hub, and the player sees what a local tell
//! would give once the answer comes back: the `You tell ...` echo, the away message, or
//! `Character not available` (`Communication_WeenieError`, `0x028A`, `0x052B`). A remote tell is
//! delivered as `Communication_HearDirectSpeech` (`0x02BD`) from the sender's stand-in id, to any
//! character on the network, after its squelches and the operator's blocks have had their say.

use dereth_network_protocol::text::{sanitize_line, sanitize_name, MAX_TEXT_CHARS};
use dereth_network_protocol::{Address, Message, Outcome};
use empyrean_entity::enums::{ChatMessageType, WeenieError, WeenieErrorWithString};
use empyrean_entity::ObjectGuid;

use super::{
    blocked, enabled, fresh_id, is_on, may_owe, may_receive, may_send, own_address,
    remote_stand_in, send, send_answer, stand_in_for, tell_player, too_new, LinkState, NotShown,
    PendingTell, Sends, NOT_ON,
};

/// What a character too new for the operator's minimum age is told when it sends to the network.
pub(super) const TOO_NEW: &str = "This character is too new to send to the Dereth Network yet.";

/// What a character sending faster than its own share of the hub's limits is told.
pub(super) const TOO_QUICKLY: &str =
    "You are sending to the Dereth Network too quickly. Wait a moment and try again.";

/// What a sender is told when a limit the whole world shares held its tell back (the hub's, or the
/// receiving world's).
fn held_back(to: &str) -> String {
    format!(
        "The Dereth Network is holding back tells from this world to {to} for now. Wait a moment and try again."
    )
}
use crate::managers::player_manager;
use crate::network::game_event::events::game_event_tell::game_event_tell_from;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_event::events::game_event_weenie_error_with_string::game_event_weenie_error_with_string;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::player_networking;
use crate::world_objects::world_object_networking::shims;
use crate::World;

/// Where a tell by name goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Routed {
    /// The network took it (or refused it, and told the sender).
    Handled,
    /// It names a character of this world by its address: the local tell goes to this name.
    Local(String),
    /// Not a network address, or the network is off: the local tell as it is.
    NotMine,
}

fn session(w: &World, player: ObjectGuid) -> Option<empyrean_net::SessionId> {
    shims::player_session(w, player)
}

fn not_available(w: &mut World, player: ObjectGuid) {
    if let Some(s) = session(w, player) {
        let m = game_event_weenie_error(session_data(w, s), WeenieError::CharacterNotAvailable);
        enqueue_send(w, s, m);
    }
}

fn echo(w: &mut World, player: ObjectGuid, to: &str, text: &str) {
    if let Some(s) = session(w, player) {
        enqueue_send(
            w,
            s,
            game_message_system_chat(
                &format!("You tell {to}, \"{text}\""),
                ChatMessageType::OutgoingTell,
            ),
        );
    }
}

/// A tell by name (`Communication_TalkDirectByName`). The sender's gag has been checked.
pub fn tell_by_name(w: &mut World, player: ObjectGuid, target: &str, text: &str) -> Routed {
    if !enabled(w) || !dereth_network_protocol::address::looks_remote(target) {
        return Routed::NotMine;
    }
    let Ok(to) = Address::parse(target) else {
        not_available(w, player);
        return Routed::Handled;
    };
    if to.world() == w.federation.settings.slug {
        return Routed::Local(to.name().to_owned());
    }
    send_tell(w, player, to, text);
    Routed::Handled
}

/// A tell by id (`Communication_TalkDirect`): a reply to a remote sender goes by the stand-in id
/// it was shown under. True when the id was one, and the network took the tell.
pub fn tell_by_id(w: &mut World, player: ObjectGuid, target: u32, text: &str) -> bool {
    if !enabled(w) || !dereth_network_protocol::standin::is_stand_in(target) {
        return false;
    }
    let Some(to) = stand_in_for(w, target) else {
        not_available(w, player);
        return true;
    };
    send_tell(w, player, to, text);
    true
}

fn send_tell(w: &mut World, player: ObjectGuid, to: Address, text: &str) {
    let character = player.full();
    if !is_on(w, character) {
        tell_player(w, player, NOT_ON);
        return;
    }
    if blocked(w, &to) {
        not_available(w, player);
        return;
    }
    if too_new(w, player) {
        tell_player(w, player, TOO_NEW);
        return;
    }
    let Some(from) = own_address(w, player) else {
        not_available(w, player);
        return;
    };
    if !may_send(w, player, Sends::Tells) {
        tell_player(w, player, TOO_QUICKLY);
        return;
    }
    // Whether the sender lists the recipient: worlds of the first version let a character take
    // tells from its mutual friends alone, and read it.
    let friend = w
        .federation
        .store
        .characters
        .get(&character)
        .is_some_and(|r| r.friends.contains_key(&to.key()));
    let id = fresh_id(w, 't');
    w.federation.pending.insert(
        id.clone(),
        PendingTell {
            player,
            to: to.clone(),
            text: text.to_owned(),
            sent: None,
        },
    );
    send(
        w,
        Message::Tell {
            id,
            from,
            to,
            text: text.to_owned(),
            friend,
        },
    );
}

/// The answer to one of this world's tells.
pub(super) fn on_result(w: &mut World, id: &str, outcome: Outcome, message: Option<&str>) {
    let Some(p) = w.federation.pending.remove(id) else {
        return;
    };
    // The sender may have logged off while it was in flight.
    if player_manager::get_online_player(w, p.player.full()).is_none() {
        return;
    }
    let to = p.to.to_string();
    match outcome {
        // Not ACE's (retail, as Empyrean answers a local squelch): a squelched sender sees the
        // echo and nothing else.
        Outcome::Delivered | Outcome::Squelched | Outcome::Ok => echo(w, p.player, &to, &p.text),
        Outcome::Afk => {
            echo(w, p.player, &to, &p.text);
            if let Some(s) = session(w, p.player) {
                let text = away_line(&p.to, message);
                let m = game_event_weenie_error_with_string(
                    session_data(w, s),
                    WeenieErrorWithString::AFK,
                    &text,
                );
                enqueue_send(w, s, m);
            }
        }
        Outcome::Offline => not_available(w, p.player),
        Outcome::Unavailable => {
            not_available(w, p.player);
            tell_player(
                w,
                p.player,
                &format!("The Dereth Network can't reach {} right now.", p.to.world()),
            );
        }
        Outcome::NotOnNetwork => {
            not_available(w, p.player);
            tell_player(
                w,
                p.player,
                &format!("{} isn't on the network.", p.to.world()),
            );
        }
        // The character's own pace was kept here before the tell went, so a refusal is the world's
        // share, or the loop guard's.
        Outcome::RateLimited => tell_player(w, p.player, &held_back(&to)),
        Outcome::NotMember => tell_player(w, p.player, &format!("You are not a member of {to}.")),
    }
}

/// The away line a sender sees: always `<to> is away: <text>`, built here from the away text
/// alone, so another world cannot put a line of its own choosing in front of the sender. A text
/// that repeats the start of the line itself is taken without it.
fn away_line(to: &Address, message: Option<&str>) -> String {
    let text = message.map(sanitize_line).unwrap_or_default();
    let prefix = format!("{to} is away:");
    let text = if text.len() >= prefix.len()
        && text.is_char_boundary(prefix.len())
        && text[..prefix.len()].eq_ignore_ascii_case(&prefix)
    {
        text[prefix.len()..].trim().to_owned()
    } else {
        text
    };
    let text = if text.is_empty() {
        player_networking::DEFAULT_AFK_MESSAGE.to_owned()
    } else {
        text
    };
    sanitize_line(&format!("{to} is away: {text}"))
}

/// A tell that will not be answered: the link is down (`hub_down`), or the answer is too late.
pub(super) fn give_up(w: &mut World, id: &str, hub_down: bool) {
    let Some(p) = w.federation.pending.remove(id) else {
        return;
    };
    if player_manager::get_online_player(w, p.player.full()).is_none() {
        return;
    }
    not_available(w, p.player);
    let line = if hub_down {
        "The Dereth Network is unavailable.".to_owned()
    } else {
        format!("The Dereth Network can't reach {} right now.", p.to.world())
    };
    tell_player(w, p.player, &line);
}

/// The answer owed to `world` for its tell `id`.
fn answer(w: &mut World, world: &str, id: String, outcome: Outcome, message: Option<String>) {
    send_answer(
        w,
        world,
        &Message::TellResult {
            id,
            outcome,
            message,
        },
    );
}

/// A remote sender's display name, or `None` when the network's rules refuse it.
pub(super) fn display_name(from: &Address) -> Option<String> {
    sanitize_name(&from.to_string()).ok()
}

/// Whether `character` squelches `who` (the character or its whole world).
pub(super) fn squelches(w: &World, character: u32, who: &Address) -> bool {
    w.federation
        .store
        .characters
        .get(&character)
        .is_some_and(|r| {
            r.squelched.contains_key(&who.key()) || r.squelched_worlds.contains(who.world())
        })
}

/// A tell from another world for a character of this one.
pub(super) fn on_tell(w: &mut World, id: String, from: &Address, to: &Address, text: &str) {
    if w.federation.link != LinkState::Up {
        return;
    }
    let origin = from.world().to_owned();
    // This world's own characters never reach it through the hub: a tell claiming one is refused.
    if origin == w.federation.settings.slug {
        answer(w, &origin, id, Outcome::Offline, None);
        return;
    }
    // A world already owed its share of answers is not answered again until it is owed fewer;
    // the hub answers this one for it.
    if !may_owe(w, &origin) {
        log::debug!("Dereth Network: {origin} is owed its share of answers; a tell is not taken");
        return;
    }
    let target = (to.world() == w.federation.settings.slug)
        .then(|| player_manager::get_online_player_by_name(w, to.name()))
        .flatten();
    let Some(target) = target else {
        answer(w, &origin, id, Outcome::Offline, None);
        return;
    };
    let character = target.full();
    let Some(name) = display_name(from) else {
        answer(w, &origin, id, Outcome::Offline, None);
        return;
    };
    if !is_on(w, character) || blocked(w, from) {
        answer(w, &origin, id, Outcome::Offline, None);
        return;
    }
    if squelches(w, character, from) {
        answer(w, &origin, id, Outcome::Squelched, None);
        return;
    }
    let Some(target_session) = session(w, target) else {
        answer(w, &origin, id, Outcome::Offline, None);
        return;
    };
    // Only a tell that would be shown counts against the paces below, so a refusal says nothing
    // a delivered tell would not.
    // One world's tells to one character are held to the per-recipient pace here too.
    let rate = w.federation.limits.tells_per_recipient;
    if !may_receive(w, &origin, &to.key(), rate) {
        answer(w, &origin, id, Outcome::RateLimited, None);
        return;
    }
    let sender_id = match remote_stand_in(w, from) {
        Ok(id) => id,
        Err(NotShown::Budget) => {
            answer(w, &origin, id, Outcome::RateLimited, None);
            return;
        }
        Err(NotShown::Exhausted) => {
            answer(w, &origin, id, Outcome::Offline, None);
            return;
        }
    };
    let text = sanitize_line(text);
    let tell = game_event_tell_from(
        session_data(w, target_session),
        &text,
        &name,
        sender_id,
        character,
        ChatMessageType::Tell,
    );
    enqueue_send(w, target_session, tell);
    // Only the away text goes back: the sender's world writes the line around it. The wire carries
    // at most the network's text limit, so a longer one is cut to it.
    let away =
        w.objects
            .get(target)
            .filter(|o| o.is_afk())
            .map(|o| match o.afk_message().as_deref() {
                Some(m) if !m.chars().all(char::is_whitespace) => {
                    m.chars().take(MAX_TEXT_CHARS).collect()
                }
                _ => player_networking::DEFAULT_AFK_MESSAGE.to_owned(),
            });
    match away {
        Some(m) => answer(w, &origin, id, Outcome::Afk, Some(m)),
        None => answer(w, &origin, id, Outcome::Delivered, None),
    }
}

/// A channel line for a member on this world: a tell from the channel's own address, with the
/// speaker's address at the start of the text. It is held to the pace a tell from the speaker's
/// world to that member is.
pub(super) fn on_channel_line(
    w: &mut World,
    channel: &str,
    from: &Address,
    to: &Address,
    text: &str,
) {
    let Ok(channel_address) =
        Address::new(channel, dereth_network_protocol::address::CHANNEL_WORLD)
    else {
        return;
    };
    let Some(target) = (to.world() == w.federation.settings.slug)
        .then(|| player_manager::get_online_player_by_name(w, to.name()))
        .flatten()
    else {
        return;
    };
    let character = target.full();
    if !is_on(w, character)
        || blocked(w, from)
        || squelches(w, character, from)
        || squelches(w, character, &channel_address)
    {
        return;
    }
    let (Some(speaker), Some(session)) = (display_name(from), session(w, target)) else {
        return;
    };
    let rate = w.federation.limits.tells_per_recipient;
    if !may_receive(w, from.world(), &to.key(), rate) {
        return;
    }
    let Ok(sender_id) = remote_stand_in(w, &channel_address) else {
        return;
    };
    let line = sanitize_line(&format!("{speaker}: {}", sanitize_line(text)));
    let tell = game_event_tell_from(
        session_data(w, session),
        &line,
        &channel_address.to_string(),
        sender_id,
        character,
        ChatMessageType::Tell,
    );
    enqueue_send(w, session, tell);
}
