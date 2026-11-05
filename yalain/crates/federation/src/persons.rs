//! Persons, the network's link between a player's characters and their Discord account
//! (`docs/network/03-world-behaviour.md` section 9, and section 1's *A person's Discord*).
//!
//! The hub keeps persons; the world asks on its own characters' behalf and shows what the hub
//! answers.
//!
//! - **Linking.** `@dn link` asks the hub for a one-time code, and shows the link it gives;
//!   `@dn unlink` undoes it. A link holds only for the character that asked for its code
//!   ([`ASKED_RECORD`]): a character made under the name of a deleted one is not linked to the
//!   deleted one's person, and is unlinked, quietly.
//! - **The view.** While a character on the network is present the world watches its person view,
//!   which says whether it is linked and whom its person friends list.
//! - **Friends.** A linked character's friends are its person's, kept by the hub: adding one asks the
//!   hub to list it ([`Federation::befriend`]), and the friends it kept before it was linked are
//!   carried over when the view first says it is linked. The characters the view names are listed
//!   here, as friends on other worlds, only while the view names them and the character is here:
//!   they are never kept. A friend it kept that the view names as a handle is its person's from
//!   then on. When it is unlinked, the hub hands its friends back in its next view, and they are kept
//!   again. The client's friends page holds what it was last given, and only rebuilds when it asks
//!   for the list, so a row that goes in play stays there, offline, until then.
//! - **Reach.** When the hub asks whether a character, away, would have been shown a tell, the world
//!   answers true only for a character of its own, away, on the network, linked by itself, ignoring
//!   neither the teller nor its world and blocking neither, within the pace of that world's tells
//!   to it. A teller of this world reaches it too, unless the character ignores it, by the list it
//!   keeps here: the world always holds a character's lists, away or not.

use std::collections::BTreeSet;

use dereth_network_protocol::answers::person_line;
use dereth_network_protocol::limits::{Bucket, Rate};
use dereth_network_protocol::person::{view_listing, MAX_CARRIED, MAX_VIEW_CHARACTERS};
use dereth_network_protocol::text::{sanitize_line, sanitize_name};
use dereth_network_protocol::{
    Address, Message, PersonFriend, PersonOp, PersonOutcome, ReachMessage,
};
use yalain_store::Store;

use crate::{Ask, AskKind, Federation, Friends, LinkState, NOT_ON, TOO_QUICKLY, UNAVAILABLE};

/// The world's record of the characters that asked for a link, one id a line.
pub const ASKED_RECORD: &str = "dereth-network-asked-links";

/// What a character is told when the view first says it is linked, while it is here.
pub const NOW_LINKED: &str = "This character is now linked to a Discord account.";

/// What a linked character going off the network is told: its link pauses, and `@dn on` resumes it
/// with nothing to redo.
pub const PAUSED: &str =
    "This character's link to your Discord account is paused until you say @dn on.";

/// What a character not linked is told when the hub answers that a request needs it to be.
const LINK_FIRST: &str = "Link this character first: say @dn link.";

/// A character listed for one of this world's through a person friend: never kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PersonRow {
    pub friend: Address,
    /// The handle of the person friend it comes by.
    pub handle: Address,
}

/// How many person views a character takes, a few at once and then one a second. A hub says so when
/// a view changes; the newest of those past this waits for the pace.
pub const VIEW_PACE: Rate = Rate {
    burst: 8.0,
    per_sec: 1.0,
};

/// How many friends person views may change on the world's lists, which are kept in the store. A
/// character's whole list is 100: a few at once, then two lists' worth a second.
pub const VIEW_ROWS_PACE: Rate = Rate {
    burst: 400.0,
    per_sec: 200.0,
};

/// The newest person view past its pace.
#[derive(Debug, Clone)]
pub(crate) struct HeldView {
    to: Address,
    linked: bool,
    persons: Vec<PersonFriend>,
}

/// The characters a record says asked for a link.
pub(crate) fn read_asked(record: Option<&[u8]>) -> BTreeSet<u64> {
    record
        .map(String::from_utf8_lossy)
        .map(|r| r.lines().filter_map(|l| l.trim().parse().ok()).collect())
        .unwrap_or_default()
}

impl Federation {
    /// Whether a character is linked, as the hub last said. Not known while it is away.
    #[must_use]
    pub fn is_linked(&self, name: &str) -> bool {
        self.linked
            .get(&name.to_lowercase())
            .copied()
            .unwrap_or(false)
    }

    /// Whether a character asked for its link (by id).
    #[must_use]
    pub fn asked_link(&self, character: u64) -> bool {
        self.asked_link.contains(&character)
    }

    /// `@dn link` for a character at `t`: the line it is told at once, when it is refused here. The
    /// link the hub gives is shown as it answers.
    pub fn link_person(
        &mut self,
        store: &mut impl Store,
        t: f64,
        character: u64,
        name: &str,
    ) -> Option<String> {
        if let Err(why) = self.may_ask_person(t, name) {
            return Some(why.to_owned());
        }
        if self.asked_link.insert(character) {
            let record: String = self.asked_link.iter().map(|c| format!("{c}\n")).collect();
            if let Err(e) = store.set_world_record(ASKED_RECORD, record.as_bytes()) {
                tracing::warn!("Dereth Network: the store: {e}");
                self.asked_link.remove(&character);
                return Some("The Dereth Network can't keep that now. Try again later.".to_owned());
            }
        }
        self.ask_person(t, character, name, PersonOp::Code, false, None);
        None
    }

    /// `@dn unlink` for a character at `t`: the line it is told at once, when it is refused here.
    pub fn unlink_person(&mut self, t: f64, character: u64, name: &str) -> Option<String> {
        if let Err(why) = self.may_ask_person(t, name) {
            return Some(why.to_owned());
        }
        self.ask_person(t, character, name, PersonOp::Unlink, false, None);
        None
    }

    /// A player's person request: the character on the network, within its pace of requests.
    fn may_ask_person(&mut self, t: f64, name: &str) -> Result<(), &'static str> {
        if !self.is_on(name) {
            return Err(NOT_ON);
        }
        if !self.may_ask(t, name) {
            return Err(TOO_QUICKLY);
        }
        Ok(())
    }

    /// Takes one from a character's pace of requests of the hub, kept beside its tells' at the
    /// same rate.
    pub(crate) fn may_ask(&mut self, t: f64, name: &str) -> bool {
        let rate = self.limits.tells_per_character;
        self.sends
            .entry(format!("{}\n@asks", name.to_lowercase()))
            .or_insert_with(|| dereth_network_protocol::limits::Bucket::new(rate, t))
            .take(t)
    }

    fn ask_person(
        &mut self,
        t: f64,
        character: u64,
        name: &str,
        op: PersonOp,
        quiet: bool,
        target: Option<Address>,
    ) {
        self.ask_person_about(t, character, name, op, quiet, target, Vec::new());
    }

    #[allow(clippy::too_many_arguments)]
    fn ask_person_about(
        &mut self,
        t: f64,
        character: u64,
        name: &str,
        op: PersonOp,
        quiet: bool,
        target: Option<Address>,
        targets: Vec<Address>,
    ) {
        let Some(who) = self.on_name(name).and_then(|n| self.own_address(n)) else {
            return;
        };
        let id = self.fresh_id('p');
        self.asks.insert(
            id.clone(),
            Ask {
                character,
                kind: AskKind::Person {
                    op,
                    quiet,
                    target: target.clone().map(|a| (name.to_owned(), a)),
                },
                sent: None,
            },
        );
        self.send(
            t,
            Message::Person {
                id,
                op,
                who,
                target,
                targets,
                shown: None,
            },
        );
    }

    /// Whether a character's friends on other worlds are its person's: it is linked, as the hub last
    /// said, and the link is up to ask. With the link down it keeps a friend itself, and carries it
    /// over at its next arrival.
    #[must_use]
    pub fn friends_are_persons(&self, name: &str) -> bool {
        self.link == LinkState::Up && self.is_linked(name)
    }

    /// A linked character adds a friend at `t`: the hub lists `friend`'s person as its person's
    /// friend, and the view brings the row. The line it is told at once when it is refused here.
    ///
    /// # Errors
    /// Past the character's pace of requests.
    pub fn befriend(
        &mut self,
        t: f64,
        character: u64,
        name: &str,
        friend: &Address,
    ) -> Result<(), &'static str> {
        if !self.may_ask(t, name) {
            return Err(TOO_QUICKLY);
        }
        self.ask_person(
            t,
            character,
            name,
            PersonOp::Befriend,
            false,
            Some(friend.clone()),
        );
        Ok(())
    }

    /// The characters a character's person friends list for it now.
    #[must_use]
    pub fn person_friends(&self, name: &str) -> Vec<Address> {
        self.person_rows
            .get(&name.to_lowercase())
            .map(|rows| rows.iter().map(|r| r.friend.clone()).collect())
            .unwrap_or_default()
    }

    /// The handle a friend of a character comes by, when a person friend lists it.
    #[must_use]
    pub fn person_handle(&self, name: &str, friend: &Address) -> Option<Address> {
        self.person_rows
            .get(&name.to_lowercase())?
            .iter()
            .find(|r| r.friend.same(friend))
            .map(|r| r.handle.clone())
    }

    /// The character removed a friend a person friend listed: the person friend goes at the hub,
    /// quietly, and with it, at its next view, every row it listed. It is not held to the
    /// character's pace: one held back would leave a person friend the player has let go.
    pub fn unfriend_person(&mut self, t: f64, character: u64, name: &str, friend: &Address) {
        let Some(handle) = self.person_handle(name, friend) else {
            return;
        };
        if let Some(rows) = self.person_rows.get_mut(&name.to_lowercase()) {
            rows.retain(|r| !r.friend.same(friend));
        }
        self.friend_removed(name, friend);
        self.ask_person(t, character, name, PersonOp::Unfriend, true, Some(handle));
    }

    /// Whether the client may still show a row for `friend` that went in play: taken, so that its
    /// removal is answered as done, which takes the row off the page.
    pub fn take_gone_row(&mut self, name: &str, friend: &Address) -> bool {
        self.gone_rows
            .get_mut(&name.to_lowercase())
            .is_some_and(|keys| keys.remove(&friend.key()))
    }

    /// The character's client asked for its friends: it shows what it is given now, and no row that
    /// went before.
    pub fn list_given(&mut self, name: &str) {
        self.gone_rows.remove(&name.to_lowercase());
    }

    /// Whether a character that asked for a link is owed a person view: its first answer tells the
    /// world whom it lists, so the client's first ask for its friends is worth holding for it.
    #[must_use]
    pub fn view_due(&self, name: &str) -> bool {
        let key = name.to_lowercase();
        self.enabled()
            && self.link == LinkState::Up
            && self
                .on
                .get(&key)
                .is_some_and(|o| self.asked_link.contains(&o.id))
            && !self.linked.contains_key(&key)
    }

    /// Forgets what a character's person friends listed: the client may still show each.
    pub(crate) fn drop_person_rows(&mut self, name: &str) {
        let key = name.to_lowercase();
        if let Some(rows) = self.person_rows.remove(&key) {
            for r in rows {
                self.note_gone_row(&key, &r.friend);
            }
        }
    }

    fn note_gone_row(&mut self, key: &str, friend: &Address) {
        let keys = self.gone_rows.entry(key.to_owned()).or_default();
        if keys.len() < 2 * MAX_VIEW_CHARACTERS {
            keys.insert(friend.key());
        }
    }

    /// Asks the hub for a character's person view, or stops: only while the link is up (a welcome
    /// asks for everyone's again). Stopped, what the view said is forgotten.
    pub(crate) fn watch_person(&mut self, name: &str, on: bool) {
        if !on {
            self.linked.remove(&name.to_lowercase());
            self.views_due.remove(&name.to_lowercase());
            self.drop_person_rows(name);
        }
        if self.link != LinkState::Up {
            return;
        }
        if let Some(who) = self.on_name(name).and_then(|n| self.own_address(n)) {
            self.person_watches
                .push_back(Message::PersonWatch { who, on });
        }
    }

    /// Every character in the world on the network asks for its view again (after a welcome).
    pub(crate) fn watch_every_person(&mut self, friends: &impl Friends) {
        for name in friends.everyone_in_sight() {
            if self.is_on(&name) {
                self.watch_person(&name, true);
            }
        }
    }

    /// The answer to a person request.
    pub(crate) fn on_person_result(
        &mut self,
        friends: &mut impl Friends,
        id: &str,
        outcome: PersonOutcome,
        code: Option<&str>,
        url: Option<&str>,
        expires_secs: Option<u32>,
    ) {
        let Some(a) = self.asks.remove(id) else {
            return;
        };
        let AskKind::Person {
            op,
            quiet: false,
            target,
        } = a.kind
        else {
            return;
        };
        // The client's own add waits on the hub, and writes its own line when it is answered done.
        let waiting = target.as_ref().is_some_and(|(name, friend)| {
            friends.befriend_answered(name, friend, outcome != PersonOutcome::Ok)
        });
        if waiting && outcome == PersonOutcome::Ok {
            return;
        }
        let shown = target
            .as_ref()
            .map_or_else(String::new, |(_, friend)| friend.to_string());
        let line = if op == PersonOp::Befriend && outcome == PersonOutcome::NotLinked {
            Some(LINK_FIRST.to_owned())
        } else {
            person_line(op, outcome, &shown, code, url, expires_secs)
        };
        if let Some(line) = line {
            friends.show_line(a.character, &sanitize_line(&line));
        }
    }

    /// A person view from the hub for a character of this world in the world. A link only holds for
    /// the character that asked for it: one that never asked is taken as unlinked, and the hub told
    /// so, quietly. The characters its person friends name are listed for it, those no longer named
    /// go, and a view of a character nobody links hands back the friends it had.
    pub(crate) fn on_person_view(
        &mut self,
        friends: &mut impl Friends,
        t: f64,
        to: &Address,
        linked: bool,
        persons: Vec<PersonFriend>,
    ) {
        if to.world() != self.settings.slug || !friends.in_world(to.name()) {
            return;
        }
        let key = to.name().to_lowercase();
        if !self.on.contains_key(&key) {
            return;
        }
        let view = HeldView {
            to: to.clone(),
            linked,
            persons,
        };
        let waits = self.views_due.contains_key(&key) || !self.view_pace_allows(t, &key);
        if waits {
            self.views_due.insert(key, view);
        } else {
            self.apply_view(friends, t, &view);
        }
    }

    /// Whether a view of the character of this key may be taken now; takes one when it may.
    fn view_pace_allows(&mut self, t: f64, key: &str) -> bool {
        let rows = self
            .view_rows
            .get_or_insert_with(|| Bucket::new(VIEW_ROWS_PACE, t));
        if !rows.has(t) {
            return false;
        }
        self.view_paces
            .entry(key.to_owned())
            .or_insert_with(|| Bucket::new(VIEW_PACE, t))
            .take(t)
    }

    /// The views that waited, as far as their paces go now.
    pub(crate) fn views_at_their_pace(&mut self, friends: &mut impl Friends, t: f64) {
        self.view_paces.retain(|_, b| !b.full_at(t));
        let keys: Vec<String> = self.views_due.keys().cloned().collect();
        for key in keys {
            if !self.view_pace_allows(t, &key) {
                continue;
            }
            if let Some(view) = self.views_due.remove(&key) {
                self.apply_view(friends, t, &view);
            }
        }
    }

    /// A character takes its person view.
    fn apply_view(&mut self, friends: &mut impl Friends, t: f64, view: &HeldView) {
        let (to, linked, persons) = (&view.to, view.linked, view.persons.as_slice());
        if !friends.in_world(to.name()) {
            return;
        }
        let Some(on) = self.on.get(&to.name().to_lowercase()).cloned() else {
            return;
        };
        let asked = self.asked_link.contains(&on.id);
        if linked && !asked && self.may_ask(t, &on.name) {
            self.ask_person(t, on.id, &on.name, PersonOp::Unlink, true, None);
        }
        let (linked, persons, returned) = if linked && !asked {
            (false, &[][..], &[][..])
        } else if linked {
            (true, persons, &[][..])
        } else {
            (false, &[][..], if asked { persons } else { &[][..] })
        };
        let key = on.name.to_lowercase();
        let before = self.linked.insert(key.clone(), linked);
        if linked && before == Some(false) {
            friends.show_line(on.id, NOW_LINKED);
        }
        if linked && before != Some(true) {
            self.carry(friends, t, &on);
        }
        self.list_view(friends, t, &on, persons, returned);
        friends.person_view_came(&on.name);
    }

    /// The view first says a character is linked: the friends it keeps on other worlds go to its
    /// person, which then lists them (as many as a person may list; the rest stay its own).
    fn carry(&mut self, friends: &impl Friends, t: f64, on: &crate::OnNetwork) {
        let targets: Vec<Address> = friends
            .remote_friends(&on.name)
            .into_iter()
            .filter(|f| !f.is_channel() && f.world() != self.settings.slug)
            .take(MAX_CARRIED)
            .collect();
        if !targets.is_empty() && self.may_ask(t, &on.name) {
            self.ask_person_about(t, on.id, &on.name, PersonOp::Carry, true, None, targets);
        }
    }

    /// Lists what a view names for a character, as the view has it now.
    fn list_view(
        &mut self,
        friends: &mut impl Friends,
        t: f64,
        on: &crate::OnNetwork,
        persons: &[PersonFriend],
        returned: &[PersonFriend],
    ) {
        let key = on.name.to_lowercase();
        let mut changed = 0usize;
        let mut rows = self.person_rows.remove(&key).unwrap_or_default();
        // A handed-back friend listed until now through a person friend stays, as the character's
        // own.
        rows.retain(|r| {
            let handed_back = returned.iter().any(|p| p.handle.same(&r.friend));
            if handed_back {
                friends.keep_friend(&on.name, &r.friend);
                changed += 1;
            }
            !handed_back
        });
        let want = view_listing(persons, &self.settings.slug);
        // Those no longer named go; those still named keep the handle they come by now.
        let mut gone = Vec::new();
        rows.retain_mut(|r| {
            if let Some((_, handle)) = want.iter().find(|(c, _)| c.same(&r.friend)) {
                r.handle = handle.clone();
                true
            } else {
                gone.push(r.friend.clone());
                false
            }
        });
        let mut added = Vec::new();
        let kept_friends = friends.remote_friends(&on.name);
        for (c, handle) in want {
            if rows.iter().any(|r| r.friend.same(&c)) {
                continue;
            }
            let kept = kept_friends.iter().any(|f| f.same(&c));
            // A friend of its own the view lists as a handle is the person's now: listed once, and
            // no longer kept here.
            if kept && handle.same(&c) {
                friends.release_friend(&on.name, &c);
                changed += 1;
                rows.push(PersonRow { friend: c, handle });
                continue;
            }
            if kept || friends.kept_count(&on.name) + rows.len() >= MAX_VIEW_CHARACTERS {
                continue;
            }
            added.push(c.clone());
            rows.push(PersonRow { friend: c, handle });
        }
        if rows.is_empty() {
            self.person_rows.remove(&key);
        } else {
            self.person_rows.insert(key.clone(), rows);
        }
        for p in returned {
            let h = &p.handle;
            if h.is_channel()
                || h.world() == self.settings.slug
                || friends.remote_friends(&on.name).iter().any(|f| f.same(h))
                || friends.kept_count(&on.name) + self.person_friends(&on.name).len()
                    >= MAX_VIEW_CHARACTERS
            {
                continue;
            }
            friends.keep_friend(&on.name, h);
            changed += 1;
            added.push(h.clone());
        }
        // What it wrote comes out of the world's pace: past it, later views wait.
        let pace = self
            .view_rows
            .get_or_insert_with(|| Bucket::new(VIEW_ROWS_PACE, t));
        for _ in 0..changed {
            pace.take(t);
        }
        for f in gone {
            let shown_online = self.shown(&on.name, &f) == Some(true);
            self.friend_removed(&on.name, &f);
            self.note_gone_row(&key, &f);
            if shown_online {
                friends.show_presence(&on.name, &f, false, true);
            }
        }
        for f in added {
            if !self.friend_added(&*friends, &on.name, &f) {
                friends.show_presence(&on.name, &f, false, false);
            }
        }
    }

    /// A person or channel request that will not be answered.
    pub(crate) fn give_up_ask(&mut self, friends: &mut impl Friends, id: &str) {
        if let Some(a) = self.asks.remove(id) {
            if let AskKind::Person {
                target: Some((name, friend)),
                quiet: false,
                ..
            } = &a.kind
            {
                friends.befriend_answered(name, friend, true);
            }
            if !matches!(a.kind, AskKind::Person { quiet: true, .. }) {
                friends.show_line(a.character, UNAVAILABLE);
            }
        }
    }

    /// One of the reach messages from the hub: a question is answered; an answer, which only a world
    /// sends, is ignored. An answer waits with those owed to the teller's world.
    pub(crate) fn on_reach(&mut self, friends: &impl Friends, t: f64, m: ReachMessage) {
        let ReachMessage::Ask { id, from, to } = m else {
            return;
        };
        if self.link != LinkState::Up {
            return;
        }
        let origin = from.world().to_owned();
        if !self.may_owe(&origin) {
            tracing::debug!(
                "Dereth Network: {origin} is owed its share of answers; a question is not taken"
            );
            return;
        }
        let reachable = self.reachable(friends, t, &from, &to);
        self.owe_frame(t, &origin, ReachMessage::Result { id, reachable }.to_json());
    }

    /// Whether `to`, one of this world's characters away, would have been shown a tell from `from`;
    /// a `true` takes one from the pace of tells from `from`'s world to `to`.
    fn reachable(&mut self, friends: &impl Friends, t: f64, from: &Address, to: &Address) -> bool {
        if to.world() != self.settings.slug {
            return false;
        }
        let Some(on) = self.on.get(&to.name().to_lowercase()) else {
            return false;
        };
        let own_teller = from.world() == self.settings.slug;
        if own_teller && (from.same(to) || !self.is_on(from.name())) {
            return false;
        }
        let shown = self.asked_link.contains(&on.id)
            && !friends.in_world(to.name())
            && !self.blocked(from)
            && !friends.ignores(to.name(), from)
            && !(own_teller && friends.ignores_here(to.name(), from.name()))
            && sanitize_name(&from.to_string()).is_ok();
        let rate = self.limits.tells_per_recipient;
        shown && self.may_receive(t, from.world(), &to.key(), rate)
    }
}
