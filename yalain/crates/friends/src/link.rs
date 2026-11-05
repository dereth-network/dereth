//! The link's half asks the service who is present and what each lists, and hands it what other
//! worlds send.

use auberean_friends::{Answer, Event, Mode};
use dereth_network_protocol::Address;
use yalain_federation::{Friends, Present, TellEnd, TellId};
use yalain_store::NetworkList;

use crate::names::{read_entry, world_entry, Target};
use crate::{KeptChange, Service, FRIEND_REFUSED};

impl Friends for Service {
    // A hidden character is not present to the network: it is shown online to nobody, told nothing
    // from another world, and watches nobody.
    fn present(&self, name: &str) -> Option<Present> {
        self.here
            .get(&name.to_lowercase())
            .filter(|h| !self.hidden.contains(&h.character))
            .map(|h| Present {
                name: h.name.clone(),
                state: h.state.clone(),
            })
    }

    fn everyone_present(&self) -> Vec<String> {
        self.here
            .values()
            .filter(|h| !self.hidden.contains(&h.character))
            .map(|h| h.name.clone())
            .collect()
    }

    fn remote_friends(&self, character: &str) -> Vec<Address> {
        let Some(id) = self.id_of(character) else {
            return Vec::new();
        };
        self.lists.get(&id).map_or_else(Vec::new, |l| {
            l.friends
                .iter()
                .filter_map(|e| match read_entry(e) {
                    Target::Remote(a) => Some(a),
                    _ => None,
                })
                .collect()
        })
    }

    fn ignores(&self, character: &str, who: &Address) -> bool {
        let Some(id) = self.id_of(character) else {
            return false;
        };
        let world = world_entry(who.world());
        self.lists.get(&id).is_some_and(|l| {
            l.ignored.iter().any(|e| {
                *e == world
                    || matches!(read_entry(e), Target::Remote(a) | Target::Channel(a) if a.same(who))
            })
        })
    }

    fn ignores_here(&self, character: &str, who: &str) -> bool {
        self.id_of(character)
            .is_some_and(|id| self.ignores_local(id, who))
    }

    fn show_tell(&mut self, to: &str, from: &Address, text: &str) {
        // A character of this world answering from elsewhere is written as the world writes its
        // own, so the client takes a reply to it as one within the world.
        let user = if from.world() == self.config.slug {
            self.written_local(from.name())
        } else {
            from.to_string()
        };
        if let Some(stream) = self.here.get(&to.to_lowercase()).map(|h| h.stream) {
            self.event(
                stream,
                Event::UTellEv {
                    user,
                    text: text.to_owned(),
                    extra: None,
                },
            );
        }
    }

    fn show_presence(&mut self, to: &str, friend: &Address, online: bool, news: bool) {
        let key = (to.to_lowercase(), friend.key());
        if self.adding.contains_key(&key) {
            self.added(&key, online);
            return;
        }
        // An add shown offline for want of an answer: the answer, late, only if it is news.
        if self.assumed_offline.remove(&key) && !online {
            return;
        }
        // The client was told offline when it was told its friends, and prints a line for every
        // state: a first report of offline is nothing to it.
        if !online && !news {
            return;
        }
        if let Some(stream) = self.here.get(&to.to_lowercase()).map(|h| h.stream) {
            self.event(
                stream,
                Event::FriendStat {
                    user: friend.to_string(),
                    mode: if online { Mode::ONLINE } else { Mode::OFFLINE },
                },
            );
        }
    }

    fn tell_ended(&mut self, tell: TellId, end: TellEnd) {
        self.tell_done(tell, end);
    }

    fn in_world(&self, name: &str) -> bool {
        self.here.contains_key(&name.to_lowercase())
            || self
                .in_world
                .values()
                .any(|w| w.name.eq_ignore_ascii_case(name))
    }

    fn show_line(&mut self, character: u64, line: &str) {
        self.line(character, line);
    }

    fn kept_count(&self, character: &str) -> usize {
        self.id_of(character)
            .map_or(0, |id| self.count(id, NetworkList::Friends))
    }

    fn release_friend(&mut self, character: &str, friend: &Address) {
        self.keep_in_memory(character, friend, false);
    }

    fn keep_friend(&mut self, character: &str, friend: &Address) {
        self.keep_in_memory(character, friend, true);
    }

    fn befriend_answered(&mut self, character: &str, friend: &Address, refused: bool) -> bool {
        let key = (character.to_lowercase(), friend.key());
        if !self.adding.contains_key(&key) {
            return false;
        }
        if refused {
            if let Some(a) = self.adding.remove(&key) {
                self.answer(a.stream, a.cx, FRIEND_REFUSED, Answer::FriendAdd);
            }
        }
        true
    }

    fn person_view_came(&mut self, character: &str) {
        let name = character.to_lowercase();
        let now = self.now;
        for l in self.lists_waiting.iter_mut().filter(|l| l.name == name) {
            l.until = now;
        }
    }
}

impl Service {
    /// Keeps a friend on another world on a character's list, or takes it off, here now and in the
    /// store at the next pass: the link's half has no store to hand.
    fn keep_in_memory(&mut self, character: &str, friend: &Address, on: bool) {
        let Some(id) = self.id_of(character) else {
            return;
        };
        let entry = friend.to_string();
        let key = entry.to_lowercase();
        let l = self.lists.entry(id).or_default();
        if on {
            if !l.friends.iter().any(|e| e.to_lowercase() == key) {
                l.friends.push(entry.clone());
            }
        } else {
            l.friends.retain(|e| e.to_lowercase() != key);
        }
        self.kept_changes.push(KeptChange {
            character: id,
            entry,
            on,
        });
    }
}
