//! The link's half asks the service who is present and what each lists, and hands it what other
//! worlds send.

use auberean_friends::{Event, Mode};
use dereth_network_protocol::Address;
use yalain_federation::{Friends, Present, TellEnd, TellId};

use crate::names::{read_entry, world_entry, Target};
use crate::Service;

impl Friends for Service {
    fn present(&self, name: &str) -> Option<Present> {
        self.here.get(&name.to_lowercase()).map(|h| Present {
            name: h.name.clone(),
            state: h.state.clone(),
        })
    }

    fn everyone_present(&self) -> Vec<String> {
        self.here.values().map(|h| h.name.clone()).collect()
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

    fn show_tell(&mut self, to: &str, from: &Address, text: &str) {
        if let Some(stream) = self.here.get(&to.to_lowercase()).map(|h| h.stream) {
            self.event(
                stream,
                Event::UTellEv {
                    user: from.to_string(),
                    text: text.to_owned(),
                    extra: None,
                },
            );
        }
    }

    fn show_presence(&mut self, to: &str, friend: &Address, online: bool, _news: bool) {
        let key = (to.to_lowercase(), friend.key());
        if self.adding.contains_key(&key) {
            self.added(&key, online);
            return;
        }
        // An add shown offline for want of an answer: the answer, late, only if it is news.
        if self.assumed_offline.remove(&key) && !online {
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
}
