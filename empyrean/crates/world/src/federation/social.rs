//! Not ACE: remote friends, their online status, and remote squelches.
//!
//! A remote friend is kept in the character's network record (ACE's friend list holds local
//! characters only) and shown in the friends panel as a `Social_FriendsUpdate` (`0x0021`) row under
//! its stand-in id. Its status is its home world's to give: this world asks with a `watch` while
//! the character is online, the home world answers with `presence` (online while the friend is on
//! the network and online, to anyone its operator does not block), and each change is a status
//! row and an online or offline line. A watch from another world for one of this world's
//! characters is answered the same way, from this side.

use dereth_network_protocol::{Address, Message};
use empyrean_entity::enums::{ChatMessageType, WeenieError};
use empyrean_entity::ObjectGuid;

use super::{
    answer_if_up, blocked, enabled, existing_record_mut, is_on, may_receive, now, own_address,
    presence_line, record_mut, send_if_up, stand_in, stand_in_for, tell_player, LocalSquelches,
    Status, MAX_REMOTE_FRIENDS, MAX_REMOTE_SQUELCHES, MAX_WATCHERS, MAX_WATCHERS_PER_WORLD, NOT_ON,
    PRESENCE_PACE, WATCHES,
};
use crate::managers::player_manager;
use crate::network::game_event::events::game_event_friends_list_update::{
    game_event_friends_list_update_rows, FriendsUpdateTypeFlag,
};
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::enqueue_send;
use crate::world_objects::managers::squelch_manager;
use crate::world_objects::world_object_networking::shims;
use crate::World;

/// The status a character's home world last gave `player` for its friend `friend`.
fn known(w: &World, player: ObjectGuid, friend: &Address) -> Option<bool> {
    w.federation
        .presence
        .get(&(player.full(), friend.key()))
        .map(|s| s.online)
}

/// The rows a character's full friends list gains: each remote friend, under its stand-in id, with
/// the status its home world last gave this character (offline until it gives one).
#[must_use]
pub fn friend_rows(w: &mut World, player: ObjectGuid) -> Vec<(u32, String, bool)> {
    if !enabled(w) {
        return Vec::new();
    }
    let friends: Vec<Address> = w
        .federation
        .store
        .characters
        .get(&player.full())
        .map(|r| r.friends.values().cloned().collect())
        .unwrap_or_default();
    friends
        .into_iter()
        .filter_map(|f| {
            let online = known(w, player, &f).unwrap_or(false);
            stand_in(w, &f).map(|id| (id, f.to_string(), online))
        })
        .collect()
}

pub(super) fn update_row(
    w: &mut World,
    player: ObjectGuid,
    kind: FriendsUpdateTypeFlag,
    id: u32,
    name: &str,
    online: bool,
) {
    if let Some(s) = shims::player_session(w, player) {
        let m =
            game_event_friends_list_update_rows(session_data(w, s), kind, &[(id, name, online)]);
        enqueue_send(w, s, m);
    }
}

/// `@friends add <address>` (`Social_AddFriend`, `0x0018`, which carries the name only). True when
/// the name was an address the network took; false leaves it to the local friend list. A linked
/// character's friend is its person's: the hub lists it, and its row comes with the person view.
/// Any other character's is its own, kept in its record.
pub fn add_friend(w: &mut World, player: ObjectGuid, name: &str) -> bool {
    if !enabled(w) || !dereth_network_protocol::address::looks_remote(name) {
        return false;
    }
    let Ok(friend) = Address::parse(name) else {
        tell_player(w, player, "That character does not exist");
        return true;
    };
    if friend.world() == w.federation.settings.slug {
        // A local character by its address: the local list, by its name.
        let local = friend.name().to_owned();
        crate::world_objects::player_character::handle_action_add_friend(w, player, &local);
        return true;
    }
    if friend.is_channel() {
        tell_player(
            w,
            player,
            "A channel is not a character; it cannot be a friend.",
        );
        return true;
    }
    let character = player.full();
    if !is_on(w, character) {
        tell_player(w, player, NOT_ON);
        return true;
    }
    if w.federation
        .store
        .characters
        .get(&character)
        .is_some_and(|r| r.friends.contains_key(&friend.key()))
    {
        tell_player(w, player, "That character is already in your friends list");
        return true;
    }
    if w.federation
        .store
        .characters
        .get(&character)
        .is_some_and(|r| r.friends.len() >= MAX_REMOTE_FRIENDS)
    {
        friends_full(w, player);
        return true;
    }
    if super::persons::linked(w, player) {
        super::persons::befriend(w, player, &friend);
        return true;
    }
    let Some(id) = stand_in(w, &friend) else {
        tell_player(
            w,
            player,
            "This world cannot show another character from the Dereth Network.",
        );
        return true;
    };
    record_mut(w, character)
        .friends
        .insert(friend.key(), friend.clone());
    w.federation.dirty = true;
    update_row(
        w,
        player,
        FriendsUpdateTypeFlag::FriendAdded,
        id,
        &friend.to_string(),
        false,
    );
    tell_player(
        w,
        player,
        &format!("{friend} has been added to your friends list."),
    );
    if let Some(owner) = own_address(w, player) {
        send_if_up(
            w,
            Message::Watch {
                owner,
                target: friend.clone(),
                on: true,
            },
        );
    }
    true
}

/// The answer to adding a friend past the most a character may list: the client's full list.
pub(super) fn friends_full(w: &mut World, player: ObjectGuid) {
    if let Some(s) = shims::player_session(w, player) {
        let m = game_event_weenie_error(session_data(w, s), WeenieError::MaxFriendsExceeded);
        enqueue_send(w, s, m);
    }
}

/// `player` lists `added` now, through its person friends: it watches them.
pub(super) fn listed_more(w: &mut World, player: ObjectGuid, added: &[Address]) {
    if let Some(owner) = own_address(w, player) {
        for f in added {
            send_if_up(
                w,
                Message::Watch {
                    owner: owner.clone(),
                    target: f.clone(),
                    on: true,
                },
            );
        }
    }
}

/// `Social_RemoveFriend` (`0x0017`) for a stand-in id. True when the id was a remote friend's.
pub fn remove_friend(w: &mut World, player: ObjectGuid, id: u32) -> bool {
    if !enabled(w) || !dereth_network_protocol::standin::is_stand_in(id) {
        return false;
    }
    let Some(friend) = stand_in_for(w, id) else {
        return false;
    };
    let removed = existing_record_mut(w, player.full()).and_then(|r| {
        let handle = r.from_person.remove(&friend.key());
        r.friends.remove(&friend.key()).map(|f| (f, handle))
    });
    let Some((friend, handle)) = removed else {
        tell_player(w, player, "That character is not in your friends list!");
        return true;
    };
    // A row a person friend listed: the person friend goes, and with it the rows it listed.
    if let Some(h) = handle {
        super::persons::unfriend_quietly(w, player, &[h]);
    }
    w.federation.dirty = true;
    w.federation.presence.remove(&(player.full(), friend.key()));
    update_row(
        w,
        player,
        FriendsUpdateTypeFlag::FriendRemoved,
        id,
        &friend.to_string(),
        false,
    );
    tell_player(
        w,
        player,
        &format!("{friend} has been removed from your friends list."),
    );
    friendship_ended(w, player, &[friend]);
    true
}

/// `@friends clear`: the remote friends go with the local ones.
pub fn remove_all_friends(w: &mut World, player: ObjectGuid) {
    if !enabled(w) {
        return;
    }
    if w.federation
        .store
        .characters
        .get(&player.full())
        .is_none_or(|r| r.friends.is_empty())
    {
        return;
    }
    let (friends, handles): (Vec<Address>, Vec<Address>) = existing_record_mut(w, player.full())
        .map(|r| {
            (
                std::mem::take(&mut r.friends).into_values().collect(),
                std::mem::take(&mut r.from_person).into_values().collect(),
            )
        })
        .unwrap_or_default();
    super::persons::unfriend_quietly(w, player, &handles);
    w.federation.dirty = true;
    let character = player.full();
    w.federation.presence.retain(|(c, _), _| *c != character);
    friendship_ended(w, player, &friends);
}

/// `player` no longer lists `former`: it stops watching them. What they see of it is unchanged,
/// since it is seen online by anyone.
pub(super) fn friendship_ended(w: &mut World, player: ObjectGuid, former: &[Address]) {
    let Some(owner) = own_address(w, player) else {
        return;
    };
    for f in former {
        send_if_up(
            w,
            Message::Watch {
                owner: owner.clone(),
                target: f.clone(),
                on: false,
            },
        );
    }
}

/// Whether `watcher` may see `character` online now: the character is on the network, online and
/// not appearing offline, and the operator does not block the watcher.
fn visible_to(w: &World, character: ObjectGuid, watcher: &Address) -> bool {
    is_on(w, character.full())
        && player_manager::get_online_player(w, character.full()).is_some()
        && !blocked(w, watcher)
        && !crate::world_objects::player_character::get_appear_offline(w, character)
}

/// Keeps `owner` as a watcher of `character`, within the caps (a watcher the character lists is
/// always kept).
fn keep_watcher(w: &mut World, character: u32, owner: &Address) {
    let listed = w
        .federation
        .store
        .characters
        .get(&character)
        .is_some_and(|r| r.friends.contains_key(&owner.key()));
    let m = w.federation.watchers.entry(character).or_default();
    if m.contains_key(&owner.key()) {
        return;
    }
    if !listed {
        let suffix = format!("@{}", owner.world());
        let from_world = m.keys().filter(|k| k.ends_with(&suffix)).count();
        if from_world >= MAX_WATCHERS_PER_WORLD || m.len() >= MAX_WATCHERS {
            return;
        }
    }
    m.insert(owner.key(), owner.clone());
}

/// A watch from another world for an address of this one. Every watch within the other world's
/// pace is answered, and alike: online only for a character on the network, online, not appearing
/// offline, and the watcher not blocked; `offline` for anything else, a name this world does not
/// have, a character off the network and a blocked watcher included, named as the watch named it.
/// So a watch tells another world nothing a tell would not. A watcher of a character on the
/// network is kept, a blocked one too (told nothing while the block lasts, so lifting it can show
/// it what it may see).
pub(super) fn on_watch(w: &mut World, owner: Address, target: &Address, on: bool) {
    if target.world() != w.federation.settings.slug {
        return;
    }
    let (found, _) = player_manager::find_by_name(w, target.name());
    if !on {
        if let Some(m) = found.and_then(|f| w.federation.watchers.get_mut(&f.guid().full())) {
            m.remove(&owner.key());
        }
        return;
    }
    // Each watch is owed an answer: another world is held to its share of this world's pace.
    let rate = w.federation.limits.watches_per_world_pair();
    if !may_receive(w, owner.world(), WATCHES, rate) {
        log::debug!(
            "Dereth Network: {} sends watches faster than its share; one is not answered",
            owner.world()
        );
        return;
    }
    let mut online = false;
    if let Some(found) = found.filter(|f| is_on(w, f.guid().full())) {
        let character = found.guid();
        keep_watcher(w, character.full(), &owner);
        online = found.is_online() && visible_to(w, character, &owner);
    }
    answer_if_up(
        w,
        Message::Presence {
            to: owner,
            of: target.clone(),
            online,
        },
    );
}

/// A remote character's status, from its home world, for the one local character it names. It
/// is kept only while that character is online and lists the remote one, and shown at the pace
/// [`PRESENCE_PACE`] gives: a world that changes its answer faster is shown its latest state.
pub(super) fn on_presence(w: &mut World, to: &Address, of: &Address, online: bool) {
    if to.world() != w.federation.settings.slug || blocked(w, of) {
        return;
    }
    let Some(watcher) = player_manager::get_online_player_by_name(w, to.name()) else {
        return;
    };
    if !w
        .federation
        .store
        .characters
        .get(&watcher.full())
        .is_some_and(|r| r.friends.contains_key(&of.key()))
    {
        return;
    }
    let t = now(w);
    let key = (watcher.full(), of.key());
    let status = w.federation.presence.entry(key.clone()).or_insert(Status {
        online,
        shown: None,
        pace: dereth_network_protocol::limits::Bucket::new(PRESENCE_PACE, t),
    });
    status.online = online;
    if status.shown == Some(online) {
        // Back to what the client shows: nothing to show, nothing waiting.
        w.federation.presence_due.remove(&key);
    } else if status.pace.take(t) {
        w.federation.presence_due.remove(&key);
        show_status(w, watcher, &key.1);
    } else {
        w.federation.presence_due.insert(key);
    }
}

/// Shows `player` a remote friend's latest status, if it is not what the client shows: a status
/// row, and the online or offline line for a change from a state the client was already shown
/// (not for the first report after a friend is added or a world or the hub comes back, which is
/// a state, not news).
fn show_status(w: &mut World, player: ObjectGuid, friend_key: &str) {
    let key = (player.full(), friend_key.to_owned());
    let Some(status) = w.federation.presence.get(&key).copied() else {
        return;
    };
    if status.shown == Some(status.online) {
        return;
    }
    // The friend as this character listed it: the spelling the client holds, which its own
    // removal matches.
    let Some(listed) = w
        .federation
        .store
        .characters
        .get(&player.full())
        .and_then(|r| r.friends.get(friend_key).cloned())
    else {
        return;
    };
    let Some(id) = stand_in(w, &listed) else {
        return;
    };
    if let Some(s) = w.federation.presence.get_mut(&key) {
        s.shown = Some(status.online);
    }
    update_row(
        w,
        player,
        FriendsUpdateTypeFlag::FriendStatusChanged,
        id,
        &listed.to_string(),
        status.online,
    );
    if status.shown.is_some() {
        let name = listed.to_string();
        // A character a person friend listed, other than its handle, comes online as its person.
        let line = match super::persons::handle_of(w, player.full(), friend_key) {
            Some(handle) if status.online && !handle.same(&listed) => {
                format!("{handle} is online as <Tell:IIDString:{id}:{name}>{name}<\\Tell>.")
            }
            _ => presence_line(id, &name, status.online),
        };
        crate::world_objects::player::send_message(w, player, &line, ChatMessageType::Broadcast);
    }
}

/// Shows each status whose time has come its latest state.
pub(super) fn show_due(w: &mut World, t: f64) {
    if w.federation.presence_due.is_empty() {
        return;
    }
    let due: Vec<(u32, String)> = w
        .federation
        .presence_due
        .iter()
        .filter(|k| w.federation.presence.get(k).is_none_or(|s| s.pace.has(t)))
        .cloned()
        .collect();
    for key in due {
        w.federation.presence_due.remove(&key);
        let Some(s) = w.federation.presence.get_mut(&key) else {
            continue;
        };
        s.pace.take(t);
        if let Some(player) = player_manager::get_online_player(w, key.0) {
            show_status(w, player, &key.1);
        }
    }
}

/// A world (or the whole hub) is gone: its characters show offline, without the lines, and
/// their state is unknown until their world says again. Its watchers are forgotten too; it
/// watches again when it is back.
pub(super) fn world_gone(w: &mut World, slug: &str) {
    let suffix = format!("@{slug}");
    for m in w.federation.watchers.values_mut() {
        m.retain(|k, _| !k.ends_with(&suffix));
    }
    w.federation.watchers.retain(|_, m| !m.is_empty());
    let known: Vec<((u32, String), bool)> = w
        .federation
        .presence
        .iter()
        .filter(|((_, k), _)| k.ends_with(&suffix))
        .map(|(k, s)| (k.clone(), s.shown == Some(true)))
        .collect();
    for ((character, key), shown_online) in known {
        w.federation.presence.remove(&(character, key.clone()));
        if !shown_online {
            continue;
        }
        let Some(player) = player_manager::get_online_player(w, character) else {
            continue;
        };
        let friend = w
            .federation
            .store
            .characters
            .get(&character)
            .and_then(|r| r.friends.get(&key).cloned());
        if let Some(f) = friend {
            if let Some(id) = stand_in(w, &f) {
                update_row(
                    w,
                    player,
                    FriendsUpdateTypeFlag::FriendStatusChanged,
                    id,
                    &f.to_string(),
                    false,
                );
            }
        }
    }
}

/// Sends a watch for every remote friend of every character online, or of those on one world.
pub(super) fn watch_everyone(w: &mut World, only: Option<&str>) {
    let mut watches = Vec::new();
    for player in player_manager::get_all_online(w) {
        let Some(r) = w.federation.store.characters.get(&player.full()) else {
            continue;
        };
        if !r.settings.on {
            continue;
        }
        let friends: Vec<Address> = r
            .friends
            .values()
            .filter(|f| only.is_none_or(|s| f.world() == s))
            .cloned()
            .collect();
        if let Some(owner) = own_address(w, player) {
            for f in friends {
                watches.push(Message::Watch {
                    owner: owner.clone(),
                    target: f,
                    on: true,
                });
            }
        }
    }
    for m in watches {
        send_if_up(w, m);
    }
}

/// Tells every remote watcher of `player`, and every remote friend it lists (whose watch this
/// world may not have kept, while the two were not yet mutual), what each may see of it now.
fn tell_watchers(w: &mut World, player: ObjectGuid, online: Option<bool>) {
    let mut to: std::collections::BTreeMap<String, Address> = w
        .federation
        .watchers
        .get(&player.full())
        .cloned()
        .unwrap_or_default();
    if let Some(r) = w.federation.store.characters.get(&player.full()) {
        for (k, f) in &r.friends {
            to.entry(k.clone()).or_insert_with(|| f.clone());
        }
    }
    let Some(of) = own_address(w, player) else {
        return;
    };
    for to in to.into_values() {
        if blocked(w, &to) {
            continue;
        }
        let shown = online.unwrap_or_else(|| visible_to(w, player, &to));
        send_if_up(
            w,
            Message::Presence {
                to,
                of: of.clone(),
                online: shown,
            },
        );
    }
}

/// A character entered the world (`PlayerManager.SwitchPlayerFromOfflineToOnline`'s friend
/// updates): its watchers are told, and it watches its remote friends.
pub fn on_login(w: &mut World, player: ObjectGuid) {
    if !enabled(w) {
        return;
    }
    super::directory::told_at_login(w, player);
    if !is_on(w, player.full()) {
        return;
    }
    tell_watchers(w, player, None);
    let friends: Vec<Address> = w
        .federation
        .store
        .characters
        .get(&player.full())
        .map(|r| r.friends.values().cloned().collect())
        .unwrap_or_default();
    if let Some(owner) = own_address(w, player) {
        for f in friends {
            send_if_up(
                w,
                Message::Watch {
                    owner: owner.clone(),
                    target: f,
                    on: true,
                },
            );
        }
    }
    super::persons::arrived(w, player);
    super::directory::arrived(w, player);
}

/// A character left the world: its watchers see it offline, it stops watching, and what its
/// friends' worlds told it is forgotten, with the friends its person friends listed. A listed
/// one's profile goes to the directory if it changed. A linked one keeps the tell squelches it has
/// of this world's characters, by which a tell from one of them is answered while it is away
/// ([`super::reach`]).
pub fn on_logout(w: &mut World, player: ObjectGuid) {
    if enabled(w) {
        keep_away_squelches(w, player);
        super::directory::left(w, player);
    }
    leave(w, player, false);
}

/// The tell squelches a character on the network and linked has of this world's characters, kept
/// in its record as it leaves: characters whose squelch covers tells, accounts, and everyone when a
/// squelch of all covers tells. None is kept for a character the world no longer holds.
fn keep_away_squelches(w: &mut World, player: ObjectGuid) {
    let character = player.full();
    if !w
        .federation
        .record(character)
        .is_some_and(|r| r.settings.on && r.asked_link)
    {
        return;
    }
    let held = w.objects.get(player).is_some_and(|o| o.player.is_some());
    let kept = held.then(|| {
        let db = squelch_manager::squelches(w, player);
        let mask = ChatMessageType::Tell.to_mask();
        LocalSquelches {
            characters: db
                .characters
                .iter()
                .filter(|(_, i)| i.filters.first().is_some_and(|f| f.contains(mask)))
                .map(|(id, _)| *id)
                .collect(),
            accounts: db.accounts.keys().cloned().collect(),
            everyone: db.globals.filters.first().is_some_and(|f| f.contains(mask)),
        }
    });
    let r = record_mut(w, character);
    if r.away_squelches != kept {
        r.away_squelches = kept;
        w.federation.dirty = true;
    }
}

/// A character went off the network (`/dn off`) and stays in the world: as [`on_logout`], and the
/// rows its person friends listed leave its friends panel.
pub(super) fn went_off(w: &mut World, player: ObjectGuid) {
    leave(w, player, true);
}

fn leave(w: &mut World, player: ObjectGuid, client_stays: bool) {
    if !enabled(w) {
        return;
    }
    let character = player.full();
    w.federation.presence.retain(|(c, _), _| *c != character);
    if !is_on(w, character) {
        return;
    }
    tell_watchers(w, player, Some(false));
    let friends: Vec<Address> = w
        .federation
        .store
        .characters
        .get(&character)
        .map(|r| r.friends.values().cloned().collect())
        .unwrap_or_default();
    if let Some(owner) = own_address(w, player) {
        for f in friends {
            send_if_up(
                w,
                Message::Watch {
                    owner: owner.clone(),
                    target: f,
                    on: false,
                },
            );
        }
    }
    super::persons::left(w, player, client_stays);
}

/// A character's settings (its network settings, or its choice to appear offline) changed: its
/// watchers see what they may see now.
pub fn settings_changed(w: &mut World, player: ObjectGuid) {
    if !enabled(w) || !is_on(w, player.full()) {
        return;
    }
    tell_watchers(w, player, None);
}

/// Whether an operator's block of `key` (a world's slug, or a character's identity key) covers
/// `who`.
fn covers(key: &str, who: &str) -> bool {
    who == key || who.rsplit_once('@').is_some_and(|(_, world)| world == key)
}

/// The operator is about to block `key`: everything the block covers sees this world's characters
/// offline, once, and is told nothing after; its watches are kept but answered nothing while the
/// block lasts, its characters show offline here (without the lines), and this world's characters
/// stop watching them.
pub(super) fn block_beginning(w: &mut World, key: &str) {
    for player in player_manager::get_all_online(w) {
        let character = player.full();
        if !is_on(w, character) {
            continue;
        }
        let Some(of) = own_address(w, player) else {
            continue;
        };
        let mut told: std::collections::BTreeMap<String, Address> = w
            .federation
            .watchers
            .get(&character)
            .map(|m| {
                m.iter()
                    .filter(|(k, _)| covers(key, k))
                    .map(|(k, a)| (k.clone(), a.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let friends: Vec<Address> = w
            .federation
            .store
            .characters
            .get(&character)
            .map(|r| {
                r.friends
                    .iter()
                    .filter(|(k, _)| covers(key, k))
                    .map(|(_, a)| a.clone())
                    .collect()
            })
            .unwrap_or_default();
        for f in &friends {
            told.entry(f.key()).or_insert_with(|| f.clone());
        }
        for to in told.into_values() {
            send_if_up(
                w,
                Message::Presence {
                    to,
                    of: of.clone(),
                    online: false,
                },
            );
        }
        for target in friends {
            send_if_up(
                w,
                Message::Watch {
                    owner: of.clone(),
                    target,
                    on: false,
                },
            );
        }
    }
    let known: Vec<((u32, String), bool)> = w
        .federation
        .presence
        .iter()
        .filter(|((_, k), _)| covers(key, k))
        .map(|(k, s)| (k.clone(), s.shown == Some(true)))
        .collect();
    for ((character, k), shown_online) in known {
        w.federation.presence.remove(&(character, k.clone()));
        if !shown_online {
            continue;
        }
        let Some(player) = player_manager::get_online_player(w, character) else {
            continue;
        };
        let friend = w
            .federation
            .store
            .characters
            .get(&character)
            .and_then(|r| r.friends.get(&k).cloned());
        if let Some(f) = friend {
            if let Some(id) = stand_in(w, &f) {
                update_row(
                    w,
                    player,
                    FriendsUpdateTypeFlag::FriendStatusChanged,
                    id,
                    &f.to_string(),
                    false,
                );
            }
        }
    }
}

/// The operator has lifted the block of `key`: what the block took away is given back. Every
/// watcher it covered (kept while it lasted) and every friend it covered sees what it may now, and
/// this world's characters online watch the friends it covered again.
pub(super) fn block_ended(w: &mut World, key: &str) {
    for player in player_manager::get_all_online(w) {
        let character = player.full();
        if !is_on(w, character) {
            continue;
        }
        let Some(of) = own_address(w, player) else {
            continue;
        };
        let friends: Vec<Address> = w
            .federation
            .store
            .characters
            .get(&character)
            .map(|r| {
                r.friends
                    .iter()
                    .filter(|(k, _)| covers(key, k))
                    .map(|(_, a)| a.clone())
                    .collect()
            })
            .unwrap_or_default();
        let mut told: std::collections::BTreeMap<String, Address> = w
            .federation
            .watchers
            .get(&character)
            .map(|m| {
                m.iter()
                    .filter(|(k, _)| covers(key, k))
                    .map(|(k, a)| (k.clone(), a.clone()))
                    .collect()
            })
            .unwrap_or_default();
        for f in &friends {
            told.entry(f.key()).or_insert_with(|| f.clone());
        }
        for to in told.into_values() {
            let online = visible_to(w, player, &to);
            send_if_up(
                w,
                Message::Presence {
                    to,
                    of: of.clone(),
                    online,
                },
            );
        }
        for f in friends {
            send_if_up(
                w,
                Message::Watch {
                    owner: of.clone(),
                    target: f,
                    on: true,
                },
            );
        }
    }
}

/// `/squelch <address>` (`Communication_ModifyCharacterSquelch`, `0x0058`, which carries the name
/// with id 0) or a squelch of a stand-in id. True when the target was remote. Squelching a channel
/// leaves it.
pub fn squelch(w: &mut World, player: ObjectGuid, add: bool, id: u32, name: &str) -> bool {
    if !enabled(w) {
        return false;
    }
    let who = if dereth_network_protocol::standin::is_stand_in(id) {
        stand_in_for(w, id)
    } else if dereth_network_protocol::address::looks_remote(name) {
        match Address::parse(name) {
            Ok(a) => Some(a),
            Err(_) => {
                tell_player(w, player, &format!("{} not found.", name.trim()));
                return true;
            }
        }
    } else {
        return false;
    };
    let Some(who) = who else {
        tell_player(w, player, "Couldn't find player to squelch.");
        return true;
    };
    if who.world() == w.federation.settings.slug {
        return false;
    }
    let character = player.full();
    if add {
        let held = w
            .federation
            .store
            .characters
            .get(&character)
            .map(|r| (r.squelched.contains_key(&who.key()), r.squelched.len()));
        match held {
            Some((true, _)) => {
                tell_player(w, player, &format!("{who} is already squelched."));
                return true;
            }
            Some((false, n)) if n >= MAX_REMOTE_SQUELCHES => {
                tell_player(
                    w,
                    player,
                    &format!(
                        "You squelch {MAX_REMOTE_SQUELCHES} characters on other worlds already. Unsquelch one first."
                    ),
                );
                return true;
            }
            _ => {}
        }
        // The squelch names the identity, so its stand-in id is kept as long as it lasts.
        let _ = stand_in(w, &who);
        record_mut(w, character)
            .squelched
            .insert(who.key(), who.clone());
        w.federation.dirty = true;
        tell_player(w, player, &format!("{who} has been squelched."));
        if who.is_channel() {
            super::commands::leave_channel(w, player, who.name());
        }
    } else {
        let removed = w
            .federation
            .store
            .characters
            .get(&character)
            .is_some_and(|r| r.squelched.contains_key(&who.key()))
            .then(|| existing_record_mut(w, character).and_then(|r| r.squelched.remove(&who.key())))
            .flatten();
        if removed.is_some() {
            w.federation.dirty = true;
            tell_player(w, player, &format!("{who} has been unsquelched."));
        } else {
            tell_player(w, player, &format!("{who} is not squelched."));
        }
    }
    true
}
