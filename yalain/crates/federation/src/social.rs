//! Friends on other worlds, their online state, and who on other worlds may see this world's
//! characters.
//!
//! A friend's state is its own world's to give: while a character on the network is present, this
//! world asks with a `watch`, the friend's world answers with `presence`, and each change reaches
//! the character. A watch from another world for one of this world's characters is answered the
//! same way, from this side: online only while the character is present and on the network, and
//! the watcher not blocked; `offline` for anything else, alike.

use std::collections::BTreeMap;

use dereth_network_protocol::limits::Bucket;
use dereth_network_protocol::{Address, Message};
use yalain_store::{Store, StoreResult};

use crate::{
    Federation, Friends, OnNetwork, Status, MAX_WATCHERS, MAX_WATCHERS_PER_WORLD, PRESENCE_PACE,
    WATCHES,
};

/// Whether an operator's block of `key` (a world's slug, or a character's identity key) covers
/// the identity `who`.
pub(crate) fn covers(key: &str, who: &str) -> bool {
    who == key || who.rsplit_once('@').is_some_and(|(_, world)| world == key)
}

impl Federation {
    /// The friends a character lists on other worlds: those it keeps, and those its person friends
    /// list.
    #[must_use]
    pub fn remote_of(&self, friends: &impl Friends, name: &str) -> Vec<Address> {
        friends
            .remote_friends(name)
            .into_iter()
            .filter(|f| f.world() != self.settings.slug && !f.is_channel())
            .chain(self.person_friends(name))
            .collect()
    }

    /// Whether `watcher` may see the local character `name` online now: the character is on the
    /// network and present, and the operator does not block the watcher.
    fn visible_to(&self, friends: &impl Friends, name: &str, watcher: &Address) -> bool {
        self.is_on(name) && friends.present(name).is_some() && !self.blocked(watcher)
    }

    /// Watches every friend on `slug`'s world of every present character on the network.
    pub(crate) fn watch_everyone(&mut self, friends: &impl Friends, slug: &str) {
        let mut watches = Vec::new();
        for name in friends.everyone_present() {
            let Some(owner) = self.on_name(&name).and_then(|n| self.own_address(n)) else {
                continue;
            };
            for f in self.remote_of(friends, &name) {
                if f.world() == slug {
                    watches.push(Message::Watch {
                        owner: owner.clone(),
                        target: f,
                        on: true,
                    });
                }
            }
        }
        for m in watches {
            self.send_if_up(m);
        }
    }

    /// A world (or the whole hub) is gone: its characters show offline, and are unknown until their
    /// world says again. Its watchers are forgotten too; it
    /// watches again when it is back.
    pub(crate) fn world_gone(&mut self, friends: &mut impl Friends, slug: &str) {
        let suffix = format!("@{slug}");
        for m in self.watchers.values_mut() {
            m.retain(|k, _| !k.ends_with(&suffix));
        }
        self.watchers.retain(|_, m| !m.is_empty());
        self.forget_presence(friends, |k| k.ends_with(&suffix));
    }

    /// Every world is gone, the link with them: as [`Federation::world_gone`], at once.
    pub(crate) fn forget_every_world(&mut self, friends: &mut impl Friends) {
        self.watchers.clear();
        self.forget_presence(friends, |_| true);
    }

    /// Forgets what local characters were told of the friends whose identity keys `which`
    /// accepts, and shows those shown online offline.
    fn forget_presence(&mut self, friends: &mut impl Friends, which: impl Fn(&str) -> bool) {
        let known: Vec<((String, String), bool)> = self
            .presence
            .iter()
            .filter(|((_, k), _)| which(k))
            .map(|(k, s)| (k.clone(), s.shown == Some(true)))
            .collect();
        for ((character, key), shown_online) in known {
            self.presence_due.remove(&(character.clone(), key.clone()));
            self.presence.remove(&(character.clone(), key.clone()));
            if !shown_online {
                continue;
            }
            let Some(present) = friends.present(&character) else {
                continue;
            };
            let listed = self
                .remote_of(friends, &present.name)
                .into_iter()
                .find(|f| f.key() == key);
            if let Some(f) = listed {
                friends.show_presence(&present.name, &f, false, true);
            }
        }
    }

    /// Tells every remote watcher of `name`, and every friend on another world it lists (whose
    /// watch this world may not have kept), what each may see of it now: `online`, or what each
    /// may see when that is none.
    fn tell_watchers(&mut self, friends: &impl Friends, name: &str, online: Option<bool>) {
        let Some(of) = self.on_name(name).and_then(|n| self.own_address(n)) else {
            return;
        };
        let mut to: BTreeMap<String, Address> = self
            .watchers
            .get(&name.to_lowercase())
            .cloned()
            .unwrap_or_default();
        for f in self.remote_of(friends, name) {
            to.entry(f.key()).or_insert(f);
        }
        for to in to.into_values() {
            if self.blocked(&to) {
                continue;
            }
            let shown = online.unwrap_or_else(|| self.visible_to(friends, name, &to));
            self.send_if_up(Message::Presence {
                to,
                of: of.clone(),
                online: shown,
            });
        }
    }

    /// Watches (or stops watching) every friend on another world `name` lists; a watch goes only
    /// to a world whose link is up (one that comes up later is watched then).
    fn watch_friends_of(&mut self, friends: &impl Friends, name: &str, on: bool) {
        let Some(owner) = self.on_name(name).and_then(|n| self.own_address(n)) else {
            return;
        };
        for f in self.remote_of(friends, name) {
            if on && !self.world_online(f.world()) {
                continue;
            }
            self.send_if_up(Message::Watch {
                owner: owner.clone(),
                target: f,
                on,
            });
        }
    }

    /// A character became present (it is in the world with a friends stream): if it is on the
    /// network, its watchers and its friends on other worlds see it online, and it watches its
    /// friends.
    pub fn arrived(&mut self, friends: &impl Friends, name: &str) {
        if !self.enabled() || !self.is_on(name) {
            return;
        }
        self.tell_watchers(friends, name, None);
        self.watch_friends_of(friends, name, true);
        self.watch_person(name, true);
    }

    /// A character is no longer present: what its friends' worlds told it is forgotten, and if
    /// it is on the network, its watchers and friends see it offline and it stops watching.
    pub fn left(&mut self, friends: &impl Friends, name: &str) {
        if !self.enabled() {
            return;
        }
        let character = name.to_lowercase();
        self.presence.retain(|(c, _), _| *c != character);
        self.presence_due.retain(|(c, _)| *c != character);
        if !self.is_on(name) {
            return;
        }
        self.tell_watchers(friends, name, Some(false));
        self.watch_friends_of(friends, name, false);
        // Still in the world, its stream gone: what its person view says still holds.
        if !friends.in_sight(name) {
            self.watch_person(name, false);
            self.gone_rows.remove(&character);
        }
    }

    /// A character in the world that is not present, having no friends stream, is on the network:
    /// it watches its person view all the same, to hear of its link.
    pub fn watch_view_of(&mut self, friends: &impl Friends, name: &str) {
        if self.enabled() && self.is_on(name) && friends.in_sight(name) {
            self.watch_person(name, true);
        }
    }

    /// A character stops watching its person view: it left the world without a friends stream, or
    /// is hidden now. What the view said is forgotten.
    pub fn stop_view_of(&mut self, name: &str) {
        if self.enabled() && self.is_on(name) {
            self.watch_person(name, false);
            self.gone_rows.remove(&name.to_lowercase());
        }
    }

    /// Puts a character of this world on the network, or takes it off, at once: kept in the
    /// store, and, while it is present, its watchers and friends on other worlds see it online or
    /// offline, and it watches its friends or stops. Put on, the world is asked for its profile
    /// for the directory ([`Federation::profiles_wanted`]); taken off, it is taken out of the
    /// directory. Taking a character off when it is deleted keeps a later character of its name
    /// from being on the network by mistake. True when the setting changed.
    ///
    /// # Errors
    /// The store failed; nothing changed.
    pub fn set_on(
        &mut self,
        store: &mut impl Store,
        friends: &impl Friends,
        id: u64,
        name: &str,
        on: bool,
    ) -> StoreResult<bool> {
        if self.is_on(name) == on {
            return Ok(false);
        }
        store.set_network_on(id, on)?;
        if on {
            let present = self.enabled() && friends.present(name).is_some();
            self.on.insert(
                name.to_lowercase(),
                OnNetwork {
                    id,
                    name: name.to_owned(),
                },
            );
            if present {
                self.arrived(friends, name);
            } else {
                self.watch_view_of(friends, name);
            }
            self.want_profile(id, name);
        } else {
            self.went_off(friends, name);
            self.listing_off(name);
        }
        Ok(true)
    }

    /// A character is about to take a new name, the friends service still knowing it by this one:
    /// while it is present and on the network, its address's watchers and its friends on other
    /// worlds see that address offline, and it stops watching under it.
    pub fn renaming(&mut self, friends: &impl Friends, name: &str) {
        self.out_of_sight(friends, name);
    }

    /// A character has taken a new name, which the friends service knows it by now. An address is a
    /// name, so to other worlds this is another character: nobody keeps watching the old one, and
    /// it is taken out of the directory. It stays on the network under the new name, and is listed
    /// and watches its friends again under it. What its friends' worlds told it stands, so its
    /// client is shown nothing new of them when they answer again. A character the hub holds out
    /// stays held out.
    pub fn renamed(&mut self, friends: &impl Friends, old: &str, new: &str) {
        let (from, to) = (old.to_lowercase(), new.to_lowercase());
        self.watchers.remove(&from);
        self.gone_rows.remove(&from);
        let Some(mut on) = self.on.remove(&from) else {
            return;
        };
        let id = on.id;
        new.clone_into(&mut on.name);
        self.on.insert(to.clone(), on);
        let told: Vec<(String, String)> = self
            .presence
            .keys()
            .filter(|(c, _)| *c == from)
            .cloned()
            .collect();
        for key in told {
            let moved = (to.clone(), key.1.clone());
            if self.presence_due.remove(&key) {
                self.presence_due.insert(moved.clone());
            }
            if let Some(status) = self.presence.remove(&key) {
                self.presence.insert(moved, status);
            }
        }
        self.listing_renamed(id, old, new);
        if friends.present(new).is_some() {
            self.arrived(friends, new);
        } else {
            self.watch_view_of(friends, new);
        }
    }

    /// A character is off the network now, its setting kept: while it is present, its watchers
    /// and friends on other worlds see it offline, and it stops watching its friends.
    pub(crate) fn went_off(&mut self, friends: &impl Friends, name: &str) {
        self.out_of_sight(friends, name);
        let character = name.to_lowercase();
        self.presence.retain(|(c, _), _| *c != character);
        self.presence_due.retain(|(c, _)| *c != character);
        self.on.remove(&character);
    }

    /// While a character is present, its watchers and friends on other worlds see it offline, and
    /// it stops watching its friends. While it is in the world at all, it stops watching its person
    /// view.
    fn out_of_sight(&mut self, friends: &impl Friends, name: &str) {
        if !self.enabled() {
            return;
        }
        if friends.present(name).is_some() {
            self.tell_watchers(friends, name, Some(false));
            self.watch_friends_of(friends, name, false);
        }
        if friends.in_sight(name) {
            self.watch_person(name, false);
        }
    }

    /// A present character on the network added a friend on another world: it is watched while
    /// the link and the friend's world are up (else when they are). Whether the watch went out
    /// now, so that the friend's world's answer is to come.
    pub fn friend_added(&mut self, friends: &impl Friends, name: &str, friend: &Address) -> bool {
        if !self.enabled()
            || friend.world() == self.settings.slug
            || friend.is_channel()
            || !self.is_on(name)
            || friends.present(name).is_none()
            || !self.world_online(friend.world())
        {
            return false;
        }
        let Some(owner) = self.on_name(name).and_then(|n| self.own_address(n)) else {
            return false;
        };
        self.send_if_up(Message::Watch {
            owner,
            target: friend.clone(),
            on: true,
        })
    }

    /// A character removed a friend on another world: it stops watching it, and what it was
    /// told of it is forgotten.
    pub fn friend_removed(&mut self, name: &str, friend: &Address) {
        if !self.enabled() || friend.world() == self.settings.slug {
            return;
        }
        let key = (name.to_lowercase(), friend.key());
        self.presence.remove(&key);
        self.presence_due.remove(&key);
        let Some(owner) = self.on_name(name).and_then(|n| self.own_address(n)) else {
            return;
        };
        self.send_if_up(Message::Watch {
            owner,
            target: friend.clone(),
            on: false,
        });
    }

    /// Keeps `owner` as a watcher of `name`, within the caps (a watcher the character lists is
    /// always kept).
    fn keep_watcher(&mut self, friends: &impl Friends, name: &str, owner: &Address) {
        let listed = self.remote_of(friends, name).iter().any(|f| f.same(owner));
        let m = self.watchers.entry(name.to_lowercase()).or_default();
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

    /// A watch from another world for an address of this one. Every watch within that world's
    /// pace is answered, and alike: online only for a character on the network and present, and
    /// the watcher not blocked; `offline` for anything else, a name this world does not have, a
    /// character off the network and a blocked watcher included, named as the watch named it. A
    /// watcher of a character on the network is kept, a blocked one too (told nothing while the
    /// block lasts, so lifting it can show it what it may see).
    pub(crate) fn on_watch(
        &mut self,
        friends: &impl Friends,
        t: f64,
        owner: Address,
        target: &Address,
        on: bool,
    ) {
        if target.world() != self.settings.slug || owner.world() == self.settings.slug {
            return;
        }
        if !on {
            if let Some(m) = self.watchers.get_mut(&target.name().to_lowercase()) {
                m.remove(&owner.key());
            }
            return;
        }
        // Each watch is owed an answer: another world is held to its share of this world's pace.
        let rate = self.limits.watches_per_world_pair();
        if !self.may_receive(t, owner.world(), WATCHES, rate) {
            tracing::debug!(
                "Dereth Network: {} sends watches faster than its share; one is not answered",
                owner.world()
            );
            return;
        }
        let mut online = false;
        if let Some(name) = self.on_name(target.name()).map(str::to_owned) {
            self.keep_watcher(friends, &name, &owner);
            online = self.visible_to(friends, &name, &owner);
        }
        self.answer_if_up(Message::Presence {
            to: owner,
            of: target.clone(),
            online,
        });
    }

    /// A friend's state, from its own world, for the one local character it names. It is kept
    /// only while that character is present, on the network and lists the friend, and shown at
    /// the pace [`PRESENCE_PACE`] gives: a world that changes its answer faster is shown its
    /// latest state.
    pub(crate) fn on_presence(
        &mut self,
        friends: &mut impl Friends,
        t: f64,
        to: &Address,
        of: &Address,
        online: bool,
    ) {
        if to.world() != self.settings.slug
            || of.world() == self.settings.slug
            || self.blocked(of)
            || !self.is_on(to.name())
        {
            return;
        }
        let Some(watcher) = friends.present(to.name()) else {
            return;
        };
        if !self
            .remote_of(friends, &watcher.name)
            .iter()
            .any(|f| f.same(of))
        {
            return;
        }
        let key = (watcher.name.to_lowercase(), of.key());
        let status = self.presence.entry(key.clone()).or_insert(Status {
            online,
            shown: None,
            pace: Bucket::new(PRESENCE_PACE, t),
        });
        status.online = online;
        if status.shown == Some(online) {
            // Back to what the character was shown: nothing to show, nothing waiting.
            self.presence_due.remove(&key);
        } else if status.pace.take(t) {
            self.presence_due.remove(&key);
            self.show_status(friends, &key);
        } else {
            self.presence_due.insert(key);
        }
    }

    /// Shows a local character a friend's latest state, if it is not what it was shown: news
    /// when it was already shown a state, else the first state after the friend was added or its
    /// world came back.
    fn show_status(&mut self, friends: &mut impl Friends, key: &(String, String)) {
        let Some(status) = self.presence.get(key).copied() else {
            return;
        };
        if status.shown == Some(status.online) {
            return;
        }
        let Some(present) = friends.present(&key.0) else {
            return;
        };
        // The friend as the character listed it: the spelling its friends list holds.
        let Some(listed) = self
            .remote_of(friends, &present.name)
            .into_iter()
            .find(|f| f.key() == key.1)
        else {
            return;
        };
        if let Some(s) = self.presence.get_mut(key) {
            s.shown = Some(status.online);
        }
        friends.show_presence(
            &present.name,
            &listed,
            status.online,
            status.shown.is_some(),
        );
        // A character a person friend listed, other than its handle, comes online as its person.
        if status.online && status.shown.is_some() {
            let handle = self.person_handle(&present.name, &listed);
            let id = self.on.get(&key.0).map(|o| o.id);
            if let (Some(h), Some(id)) = (handle.filter(|h| !h.same(&listed)), id) {
                friends.show_line(id, &format!("{h} is online as {listed}."));
            }
        }
    }

    /// Shows each state whose time has come its latest.
    pub(crate) fn show_due(&mut self, friends: &mut impl Friends, t: f64) {
        if self.presence_due.is_empty() {
            return;
        }
        let due: Vec<(String, String)> = self
            .presence_due
            .iter()
            .filter(|k| self.presence.get(k).is_none_or(|s| s.pace.has(t)))
            .cloned()
            .collect();
        for key in due {
            self.presence_due.remove(&key);
            let Some(s) = self.presence.get_mut(&key) else {
                continue;
            };
            s.pace.take(t);
            self.show_status(friends, &key);
        }
    }

    /// The operator is about to block `key`: everything the block covers sees this world's
    /// characters offline, once, and is told nothing after; its watches are kept but answered
    /// nothing while the block lasts, its characters show offline here, and this world's
    /// characters stop watching them.
    pub(crate) fn block_beginning(&mut self, friends: &mut impl Friends, key: &str) {
        for name in friends.everyone_present() {
            let Some(of) = self.on_name(&name).and_then(|n| self.own_address(n)) else {
                continue;
            };
            let mut told: BTreeMap<String, Address> = self
                .watchers
                .get(&name.to_lowercase())
                .map(|m| {
                    m.iter()
                        .filter(|(k, _)| covers(key, k))
                        .map(|(k, a)| (k.clone(), a.clone()))
                        .collect()
                })
                .unwrap_or_default();
            let covered: Vec<Address> = self
                .remote_of(friends, &name)
                .into_iter()
                .filter(|f| covers(key, &f.key()))
                .collect();
            for f in &covered {
                told.entry(f.key()).or_insert_with(|| f.clone());
            }
            for to in told.into_values() {
                self.send_if_up(Message::Presence {
                    to,
                    of: of.clone(),
                    online: false,
                });
            }
            for target in covered {
                self.send_if_up(Message::Watch {
                    owner: of.clone(),
                    target,
                    on: false,
                });
            }
        }
        self.forget_presence(friends, |k| covers(key, k));
    }

    /// The operator has lifted the block of `key`: what the block took away is given back. Every
    /// watcher it covered (kept while it lasted) and every friend it covered sees what it may
    /// now, and this world's present characters watch the friends it covered again.
    pub(crate) fn block_ended(&mut self, friends: &impl Friends, key: &str) {
        for name in friends.everyone_present() {
            let Some(of) = self.on_name(&name).and_then(|n| self.own_address(n)) else {
                continue;
            };
            let covered: Vec<Address> = self
                .remote_of(friends, &name)
                .into_iter()
                .filter(|f| covers(key, &f.key()))
                .collect();
            let mut told: BTreeMap<String, Address> = self
                .watchers
                .get(&name.to_lowercase())
                .map(|m| {
                    m.iter()
                        .filter(|(k, _)| covers(key, k))
                        .map(|(k, a)| (k.clone(), a.clone()))
                        .collect()
                })
                .unwrap_or_default();
            for f in &covered {
                told.entry(f.key()).or_insert_with(|| f.clone());
            }
            for to in told.into_values() {
                let online = self.visible_to(friends, &name, &to);
                self.send_if_up(Message::Presence {
                    to,
                    of: of.clone(),
                    online,
                });
            }
            for f in covered {
                if self.world_online(f.world()) {
                    self.send_if_up(Message::Watch {
                        owner: of.clone(),
                        target: f,
                        on: true,
                    });
                }
            }
        }
    }
}
