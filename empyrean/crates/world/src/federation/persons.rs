//! Not ACE: persons, the network's link between a player's characters and their Discord account.
//!
//! The hub keeps persons; the world asks on its own characters' behalf and shows what the hub
//! answers (`docs/network/03-world-behaviour.md` section 9):
//!
//! - **Linking.** `/dn link` asks the hub for a one-time code for the character, and shows the
//!   link the hub gives with it: opening it on the site, signed in with Discord, links the
//!   character. `/dn unlink` undoes it. A link holds only for the character that asked for its
//!   code ([`super::CharacterRecord::asked_link`]): a character made under the name of a deleted
//!   one is not linked to the deleted one's person, and is unlinked.
//! - **Friends.** A linked character's friends are its person's, kept by the hub: adding one asks
//!   the hub to list it (`befriend`), and the friends a character had before it was linked are
//!   carried over to the hub when the view first says it is linked (`carry`). While a character is
//!   online and on the network, the world watches its person view (`person.watch`), and lists
//!   every character the view's person friends name as a remote friend of the character, as if it
//!   had added each, until the view stops naming it or the character leaves. Such a friend is
//!   marked in the record ([`super::CharacterRecord::from_person`], with the handle it came by), so it is
//!   never confused with one kept in the record, and it is never kept past the character's time
//!   online. A friend of its own the view lists as a handle becomes such a friend, so none is
//!   listed twice. When a character is unlinked, the hub hands its friends back in its next view,
//!   and they become its own again.

use std::collections::BTreeMap;

use dereth_network_protocol::{Address, Message, PersonFriend, PersonOp, PersonOutcome};
use empyrean_entity::ObjectGuid;

use super::{
    enabled, existing_record_mut, fresh_id, is_on, may_send, now, own_address, record_mut,
    remote_stand_in, send, social, stand_in, tell_player, LinkState, Sends, MAX_REMOTE_FRIENDS,
};
use crate::managers::player_manager;
use crate::network::game_event::events::game_event_friends_list_update::FriendsUpdateTypeFlag;
use crate::World;

/// What a character not linked to a person is told when it asks for something only a person has.
const LINK_FIRST: &str = "Link this character first: type @dn link.";

/// What a linked character adding a friend is told when its person lists as many friends across
/// the network as the hub allows.
pub const FRIENDS_FULL: &str =
    "You have as many friends on the Dereth Network as it allows. Remove one first.";

/// What the hub's person view last said of one character online: whether it is linked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersonState {
    pub linked: bool,
}

/// Why a person request was made, which decides what its answer shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Purpose {
    /// A `/dn` command, or adding a friend: its answer is shown.
    Command,
    /// A row the player removed, or friends carried over: nothing is shown.
    Quiet,
}

/// A person request not yet answered.
#[derive(Debug, Clone)]
pub(super) struct PendingPerson {
    pub player: ObjectGuid,
    pub op: PersonOp,
    pub target: Option<Address>,
    pub purpose: Purpose,
    /// When it was handed to the link; none while it waits for one.
    pub sent: Option<f64>,
}

/// What the hub's view last said of `character`, while it is online; none until it says.
#[must_use]
pub fn state(w: &World, character: u32) -> Option<PersonState> {
    w.federation.persons.get(&character).copied()
}

/// Asks the hub for `player`'s person view, or stops: only while the link is up (it is asked again
/// after every welcome).
fn watch(w: &mut World, player: ObjectGuid, on: bool) {
    if w.federation.link != LinkState::Up {
        return;
    }
    if let Some(who) = own_address(w, player) {
        send(w, Message::PersonWatch { who, on });
    }
}

/// A character on the network came online (or onto the network): its person view is asked for.
pub(super) fn arrived(w: &mut World, player: ObjectGuid) {
    watch(w, player, true);
}

/// A character left the world or the network: its view is no longer asked for, and the friends
/// its person friends listed go. `client_stays` (the character went off the network but is still
/// in the world) also takes their rows out of its friends panel.
pub(super) fn left(w: &mut World, player: ObjectGuid, client_stays: bool) {
    watch(w, player, false);
    w.federation.persons.remove(&player.full());
    let gone = strip(w, player.full());
    if client_stays {
        for f in gone {
            if let Some(id) = stand_in(w, &f) {
                social::update_row(
                    w,
                    player,
                    FriendsUpdateTypeFlag::FriendRemoved,
                    id,
                    &f.to_string(),
                    false,
                );
            }
        }
    }
}

/// Takes every friend a person friend listed out of `character`'s record: the friends taken.
fn strip(w: &mut World, character: u32) -> Vec<Address> {
    let Some(r) = existing_record_mut(w, character) else {
        return Vec::new();
    };
    if r.from_person.is_empty() {
        return Vec::new();
    }
    let keys: Vec<String> = std::mem::take(&mut r.from_person).into_keys().collect();
    let gone: Vec<Address> = keys.iter().filter_map(|k| r.friends.remove(k)).collect();
    w.federation.dirty = true;
    w.federation
        .presence
        .retain(|(c, k), _| *c != character || !keys.contains(k));
    gone
}

/// The store as saved: a world that stopped while its characters were online, a crash included,
/// keeps none of what their person friends listed.
pub(super) fn strip_saved(store: &mut super::NetworkStore) {
    for r in store.characters.values_mut() {
        if r.from_person.is_empty() {
            continue;
        }
        let r = std::sync::Arc::make_mut(r);
        for k in std::mem::take(&mut r.from_person).into_keys() {
            r.friends.remove(&k);
        }
    }
}

/// Every character online and on the network asks for its view again (after a welcome).
pub(super) fn watch_everyone(w: &mut World) {
    for player in player_manager::get_all_online(w) {
        if is_on(w, player.full()) {
            watch(w, player, true);
        }
    }
}

/// A person view from the hub, for one of this world's characters online. The characters its
/// person friends name become the character's remote friends, except itself, this world's own
/// characters, any it lists itself, and any past the friends a character may list; those it no
/// longer names go.
pub(super) fn on_view(w: &mut World, to: &Address, linked: bool, persons: &[PersonFriend]) {
    if to.world() != w.federation.settings.slug {
        return;
    }
    let Some(player) = player_manager::get_online_player_by_name(w, to.name()) else {
        return;
    };
    let character = player.full();
    if !is_on(w, character) {
        return;
    }
    // A link holds only for the character that asked for its code: one that never asked carries a
    // name a deleted character was linked under. It is taken as unlinked, and the hub told so.
    let asked = w
        .federation
        .store
        .characters
        .get(&character)
        .is_some_and(|r| r.asked_link);
    // A view of a character nobody links names the friends handed back to it when it was
    // unlinked: they become its own, if the link was its own.
    let (linked, persons, returned) = if linked && !asked {
        request(w, player, PersonOp::Unlink, None, Purpose::Quiet);
        (false, &[][..], &[][..])
    } else if !linked {
        (false, &[][..], if asked { persons } else { &[][..] })
    } else {
        (linked, persons, &[][..])
    };
    let before = w
        .federation
        .persons
        .insert(character, PersonState { linked });
    if linked && before.is_some_and(|b| !b.linked) {
        tell_player(
            w,
            player,
            "This character is now linked to a Discord account.",
        );
    }
    if linked && before.is_none_or(|b| !b.linked) {
        carry(w, player);
    }
    // A handed-back friend listed until now through a person friend stays, as the character's own.
    for p in returned {
        if let Some(r) = existing_record_mut(w, character) {
            if r.from_person.remove(&p.handle.key()).is_some() {
                w.federation.dirty = true;
            }
        }
    }
    let slug = w.federation.settings.slug.clone();
    // In the view's order, each character once, with the first handle that names it. A character
    // of this world (the character itself among them) is its own world's to list.
    let mut want: BTreeMap<String, (Address, Address)> = BTreeMap::new();
    let mut order = Vec::new();
    for p in persons {
        for c in &p.characters {
            if c.is_channel() || c.world() == slug || want.contains_key(&c.key()) {
                continue;
            }
            order.push(c.key());
            want.insert(c.key(), (c.clone(), p.handle.clone()));
        }
    }
    let derived: BTreeMap<String, Address> = w
        .federation
        .store
        .characters
        .get(&character)
        .map(|r| r.from_person.clone())
        .unwrap_or_default();
    // Those no longer named go.
    let mut gone = Vec::new();
    for key in derived.keys().filter(|k| !want.contains_key(*k)) {
        let removed = existing_record_mut(w, character).and_then(|r| {
            r.from_person.remove(key);
            r.friends.remove(key)
        });
        if let Some(f) = removed {
            w.federation.presence.remove(&(character, key.clone()));
            gone.push(f);
        }
    }
    for f in &gone {
        if let Some(id) = stand_in(w, f) {
            social::update_row(
                w,
                player,
                FriendsUpdateTypeFlag::FriendRemoved,
                id,
                &f.to_string(),
                false,
            );
        }
    }
    // Those still named keep the handle they come by now; new ones are listed.
    let mut added = Vec::new();
    for key in order {
        let Some((c, handle)) = want.remove(&key) else {
            continue;
        };
        if derived.contains_key(&key) {
            if let Some(r) = existing_record_mut(w, character) {
                r.from_person.insert(key, handle);
            }
            continue;
        }
        // A friend of its own the view lists as a person friend's handle is the person's now: it
        // is listed once, and no longer kept in the record.
        let own_handle = handle.key() == key
            && w.federation
                .store
                .characters
                .get(&character)
                .is_some_and(|r| r.friends.contains_key(&key));
        if own_handle {
            if let Some(r) = existing_record_mut(w, character) {
                r.from_person.insert(key, handle);
                w.federation.dirty = true;
            }
            continue;
        }
        let (listed, full) =
            w.federation
                .store
                .characters
                .get(&character)
                .map_or((false, false), |r| {
                    (
                        r.friends.contains_key(&key),
                        r.friends.len() >= MAX_REMOTE_FRIENDS,
                    )
                });
        if listed || full {
            continue;
        }
        // Another world's character: a new id for it is held to its world's allowance of new
        // names, as a tell's sender's is. One past it is listed when a later view names it.
        let Ok(id) = remote_stand_in(w, &c) else {
            continue;
        };
        if let Some(r) = existing_record_mut(w, character) {
            r.friends.insert(key.clone(), c.clone());
            r.from_person.insert(key, handle);
        }
        social::update_row(
            w,
            player,
            FriendsUpdateTypeFlag::FriendAdded,
            id,
            &c.to_string(),
            false,
        );
        added.push(c);
    }
    for p in returned {
        if p.handle.is_channel() || p.handle.world() == slug {
            continue;
        }
        let key = p.handle.key();
        let (listed, full) =
            w.federation
                .store
                .characters
                .get(&character)
                .map_or((false, false), |r| {
                    (
                        r.friends.contains_key(&key),
                        r.friends.len() >= MAX_REMOTE_FRIENDS,
                    )
                });
        if listed || full {
            continue;
        }
        let Ok(id) = remote_stand_in(w, &p.handle) else {
            continue;
        };
        record_mut(w, character)
            .friends
            .insert(key, p.handle.clone());
        social::update_row(
            w,
            player,
            FriendsUpdateTypeFlag::FriendAdded,
            id,
            &p.handle.to_string(),
            false,
        );
        added.push(p.handle.clone());
    }
    if !gone.is_empty() || !added.is_empty() {
        w.federation.dirty = true;
    }
    if !gone.is_empty() {
        social::friendship_ended(w, player, &gone);
    }
    if !added.is_empty() {
        social::listed_more(w, player, &added);
    }
}

/// The view first says `player`'s character is linked: the friends it keeps in its record are
/// carried over to its person, which then lists them (as many as a person may list; the rest stay
/// its own).
fn carry(w: &mut World, player: ObjectGuid) {
    let targets: Vec<Address> = w
        .federation
        .store
        .characters
        .get(&player.full())
        .map(|r| {
            r.friends
                .iter()
                .filter(|(k, _)| !r.from_person.contains_key(*k))
                .map(|(_, f)| f.clone())
                .take(dereth_network_protocol::person::MAX_CARRIED)
                .collect()
        })
        .unwrap_or_default();
    if !targets.is_empty() {
        send_request(w, player, PersonOp::Carry, None, targets, Purpose::Quiet);
    }
}

/// The handle a friend of `character` came by, when a person friend listed it.
#[must_use]
pub fn handle_of(w: &World, character: u32, friend_key: &str) -> Option<Address> {
    w.federation
        .store
        .characters
        .get(&character)
        .and_then(|r| r.from_person.get(friend_key).cloned())
}

/// Sends a person request for `player`, within its pace; false when it was not sent (and the
/// player was told why).
pub(super) fn request(
    w: &mut World,
    player: ObjectGuid,
    op: PersonOp,
    target: Option<Address>,
    purpose: Purpose,
) -> bool {
    send_request(w, player, op, target, Vec::new(), purpose)
}

/// [`request`], naming `targets` too.
fn send_request(
    w: &mut World,
    player: ObjectGuid,
    op: PersonOp,
    target: Option<Address>,
    targets: Vec<Address>,
    purpose: Purpose,
) -> bool {
    if purpose == Purpose::Command && !is_on(w, player.full()) {
        tell_player(w, player, super::NOT_ON);
        return false;
    }
    // An `unfriend` only takes away, and one held back would leave a person friend the player has
    // already let go: it is not held to the character's pace.
    if op != PersonOp::Unfriend && !may_send(w, player, Sends::ChannelRequests) {
        if purpose == Purpose::Command {
            tell_player(w, player, super::tells::TOO_QUICKLY);
        }
        return false;
    }
    let Some(who) = own_address(w, player) else {
        return false;
    };
    let id = fresh_id(w, 'p');
    w.federation.pending_persons.insert(
        id.clone(),
        PendingPerson {
            player,
            op,
            target: target.clone(),
            purpose,
            sent: None,
        },
    );
    send(
        w,
        Message::Person {
            id,
            op,
            who,
            target,
            targets,
            shown: None,
        },
    );
    true
}

/// `/dn link`: the character asked to be linked, so a link the hub then reports is its own.
pub fn link(w: &mut World, player: ObjectGuid) {
    if request(w, player, PersonOp::Code, None, Purpose::Command) {
        let r = record_mut(w, player.full());
        if !r.asked_link {
            r.asked_link = true;
            w.federation.dirty = true;
        }
    }
}

/// `/dn unlink`.
pub fn unlink(w: &mut World, player: ObjectGuid) {
    request(w, player, PersonOp::Unlink, None, Purpose::Command);
}

/// A linked character adds a friend: the hub lists `friend`'s person as its person's friend, and
/// the person view brings the row.
pub(super) fn befriend(w: &mut World, player: ObjectGuid, friend: &Address) {
    request(
        w,
        player,
        PersonOp::Befriend,
        Some(friend.clone()),
        Purpose::Command,
    );
}

/// The player removed a friend a person friend listed, or cleared its friends: the person friends
/// it came by are removed at the hub, quietly.
pub(super) fn unfriend_quietly(w: &mut World, player: ObjectGuid, handles: &[Address]) {
    let mut seen = std::collections::BTreeSet::new();
    for h in handles {
        if seen.insert(h.key()) {
            request(
                w,
                player,
                PersonOp::Unfriend,
                Some(h.clone()),
                Purpose::Quiet,
            );
        }
    }
}

/// The answer to a person request.
pub(super) fn on_result(
    w: &mut World,
    id: &str,
    outcome: PersonOutcome,
    code: Option<&str>,
    expires_secs: Option<u32>,
    url: Option<&str>,
) {
    let Some(p) = w.federation.pending_persons.remove(id) else {
        return;
    };
    if p.purpose == Purpose::Quiet
        || player_manager::get_online_player(w, p.player.full()).is_none()
    {
        return;
    }
    let target = p
        .target
        .as_ref()
        .map_or_else(String::new, ToString::to_string);
    let line = match (p.op, outcome) {
        (PersonOp::Code, PersonOutcome::Ok) => {
            let (Some(code), Some(url)) = (code, url) else {
                return;
            };
            link_line(code, url, expires_secs.unwrap_or(600))
        }
        (PersonOp::Code, PersonOutcome::RateLimited) => {
            "You have asked for too many links. Wait a while and try again.".to_owned()
        }
        (_, PersonOutcome::Unavailable) => {
            "The Dereth Network's hub does not link characters.".to_owned()
        }
        (_, PersonOutcome::RateLimited) => {
            "The Dereth Network is holding back your requests for now. Wait a while and try again."
                .to_owned()
        }
        (PersonOp::Code, _) => "The Dereth Network gave no link.".to_owned(),
        (PersonOp::Unlink, PersonOutcome::Ok) => {
            "This character is no longer linked to a Discord account.".to_owned()
        }
        (PersonOp::Unlink, _) => "This character is not linked to a Discord account.".to_owned(),
        (_, PersonOutcome::NotLinked) => LINK_FIRST.to_owned(),
        // The hub's limit on the friends a person may list across the network.
        (PersonOp::Befriend, PersonOutcome::Refused) => FRIENDS_FULL.to_owned(),
        (PersonOp::Befriend, _) => format!("{target} has been added to your friends list."),
        // Only ever asked quietly; the other two this world never asks.
        (PersonOp::Unfriend | PersonOp::Carry | PersonOp::Alts | PersonOp::Characters, _) => return,
    };
    let line = dereth_network_protocol::text::sanitize_line(&line);
    tell_player(w, p.player, &line);
}

/// The line `/dn link` shows: the link that links the character, from the hub's page and code,
/// and how long it lasts. A hub whose page does not carry the code is answered with the code to
/// enter there.
#[must_use]
pub fn link_line(code: &str, url: &str, expires_secs: u32) -> String {
    let minutes = expires_secs.div_ceil(60).max(1);
    let lasts = if minutes == 1 {
        "1 minute".to_owned()
    } else {
        format!("{minutes} minutes")
    };
    if url.contains(code) {
        format!(
            "Open {url} within {lasts} to link this character to your Discord account. Keep the link to yourself."
        )
    } else {
        format!(
            "Your link code is {code}. Enter it at {url} within {lasts} to link this character to your Discord account. Keep the code to yourself."
        )
    }
}

/// A person request that will not be answered.
pub(super) fn give_up(w: &mut World, id: &str) {
    if let Some(p) = w.federation.pending_persons.remove(id) {
        if p.purpose == Purpose::Command
            && player_manager::get_online_player(w, p.player.full()).is_some()
        {
            tell_player(w, p.player, "The Dereth Network is unavailable.");
        }
    }
}

/// The ids of person requests given to the link and not answered within `timeout` seconds.
pub(super) fn late(w: &World, timeout: f64) -> Vec<String> {
    let t = now(w);
    w.federation
        .pending_persons
        .iter()
        .filter(|(_, p)| p.sent.is_some_and(|s| t - s >= timeout))
        .map(|(k, _)| k.clone())
        .collect()
}

/// Whether the network is on and `player`'s character is linked, as the hub last said.
#[must_use]
pub fn linked(w: &World, player: ObjectGuid) -> bool {
    enabled(w) && state(w, player.full()).is_some_and(|s| s.linked)
}
