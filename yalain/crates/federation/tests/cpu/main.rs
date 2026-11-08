//! The world's half of the network, driven as the host drives it, on virtual time, with a friends
//! service of the test's own.

mod channels;
mod directory;
mod hub_bounds;
mod operator;
mod persons;
mod presence;
mod rooms;
mod tells;
mod verify;
mod watches;

use std::collections::BTreeMap;

use dereth_network_protocol::limits::Limits;
use dereth_network_protocol::wire::PROTOCOL;
use dereth_network_protocol::{Address, DirectoryMessage, Game, Message, Outcome, WorldInfo};
use yalain_federation::{Federation, Friends, Present, Settings, State, TellEnd, TellId};
use yalain_store::{Character, MemoryStore, Store};

/// This world's slug.
pub const SLUG: &str = "dawnsong";

/// A friends service of the test's own: who is present, what each lists and ignores, and what
/// it was asked to show.
#[derive(Debug, Default)]
pub struct Service {
    pub present: BTreeMap<String, Present>,
    pub lists: BTreeMap<String, Vec<Address>>,
    /// Ignored identities (an address's key, or a world's slug), by character.
    pub ignored: BTreeMap<String, Vec<String>>,
    pub tells: Vec<(String, Address, String)>,
    pub states: Vec<(String, Address, bool, bool)>,
    pub ended: Vec<(TellId, TellEnd)>,
    /// Characters in the world without a stream, by name in small letters.
    pub in_world: Vec<String>,
    /// The network's lines shown, with the character each was for.
    pub lines: Vec<(u64, String)>,
    /// The hub's answers to linked characters' adds: the friend, and whether it refused.
    pub adds_answered: Vec<(Address, bool)>,
    /// A client's own add waits on the hub's answer.
    pub add_waits: bool,
    /// The characters a person view came for.
    pub views: Vec<String>,
}

impl Friends for Service {
    fn present(&self, name: &str) -> Option<Present> {
        self.present.get(&name.to_lowercase()).cloned()
    }

    fn everyone_present(&self) -> Vec<String> {
        self.present.values().map(|p| p.name.clone()).collect()
    }

    fn remote_friends(&self, character: &str) -> Vec<Address> {
        self.lists
            .get(&character.to_lowercase())
            .cloned()
            .unwrap_or_default()
    }

    fn ignores(&self, character: &str, who: &Address) -> bool {
        self.ignored
            .get(&character.to_lowercase())
            .is_some_and(|v| v.iter().any(|k| *k == who.key() || k == who.world()))
    }

    fn ignores_here(&self, character: &str, who: &str) -> bool {
        self.ignored
            .get(&character.to_lowercase())
            .is_some_and(|v| v.iter().any(|k| k.eq_ignore_ascii_case(who)))
    }

    fn show_tell(&mut self, to: &str, from: &Address, text: &str) {
        self.tells
            .push((to.to_owned(), from.clone(), text.to_owned()));
    }

    fn show_presence(&mut self, to: &str, friend: &Address, online: bool, news: bool) {
        self.states
            .push((to.to_owned(), friend.clone(), online, news));
    }

    fn tell_ended(&mut self, tell: TellId, end: TellEnd) {
        self.ended.push((tell, end));
    }

    fn in_world(&self, name: &str) -> bool {
        let name = name.to_lowercase();
        self.present.contains_key(&name) || self.in_world.contains(&name)
    }

    fn in_sight(&self, name: &str) -> bool {
        self.in_world(name)
    }

    fn everyone_in_sight(&self) -> Vec<String> {
        let mut names = self.everyone_present();
        names.extend(self.in_world.iter().cloned());
        names
    }

    fn show_line(&mut self, character: u64, line: &str) {
        self.lines.push((character, line.to_owned()));
    }

    fn kept_count(&self, character: &str) -> usize {
        self.lists
            .get(&character.to_lowercase())
            .map_or(0, Vec::len)
    }

    fn release_friend(&mut self, character: &str, friend: &Address) {
        if let Some(l) = self.lists.get_mut(&character.to_lowercase()) {
            l.retain(|f| !f.same(friend));
        }
    }

    fn keep_friend(&mut self, character: &str, friend: &Address) {
        self.lists
            .entry(character.to_lowercase())
            .or_default()
            .push(friend.clone());
    }

    fn befriend_answered(&mut self, _: &str, friend: &Address, refused: bool) -> bool {
        self.adds_answered.push((friend.clone(), refused));
        self.add_waits
    }

    fn person_view_came(&mut self, character: &str) {
        self.views.push(character.to_owned());
    }
}

/// A world on the network, its friends service, its store and its clock.
#[derive(Debug)]
pub struct World {
    pub fed: Federation,
    pub service: Service,
    pub store: MemoryStore,
    pub t: f64,
    pub ids: BTreeMap<String, u64>,
    /// The directory's listings the world sent, as the hub reads an Asheron's Call 2 world's.
    pub listings: Vec<DirectoryMessage>,
}

pub fn addr(s: &str) -> Address {
    Address::parse(s).expect("an address")
}

impl Default for World {
    fn default() -> Self {
        Self::with(true)
    }
}

impl World {
    /// A world on the network as `dawnsong`, its link down.
    pub fn new() -> Self {
        Self::with(true)
    }

    /// A world whose operator has (`enabled`) or has not turned the network on.
    pub fn with(enabled: bool) -> Self {
        Self {
            fed: Federation::new(
                Settings {
                    enabled,
                    slug: SLUG.into(),
                    software: "yalain test".into(),
                    ..Settings::default()
                },
                1_790_000_000,
            ),
            service: Service::default(),
            store: MemoryStore::default(),
            t: 1_000.0,
            ids: BTreeMap::new(),
            listings: Vec::new(),
        }
    }

    /// Makes a character of this world in the store; its id.
    pub fn character(&mut self, name: &str) -> u64 {
        let id = self.store.next_character_number().expect("a number");
        self.store
            .insert_character(&Character {
                id,
                account: id,
                name: name.into(),
                race: 4,
                sex: 0x2000,
                class: 4,
                implement: 8,
                start_area: 0x4000_0005,
                entity_did: 0x4700_110F,
                physique: Vec::new(),
                look: Vec::new(),
                created_at: 0,
                deleted_at: None,
                last_played: 0,
            })
            .expect("kept");
        self.ids.insert(name.to_owned(), id);
        id
    }

    /// Puts a character on the network (or takes it off).
    pub fn set_on(&mut self, name: &str, on: bool) {
        let id = self.ids[name];
        self.fed
            .set_on(&mut self.store, &self.service, id, name, on)
            .expect("kept");
    }

    /// A character of this world, on the network, present and available.
    pub fn on_and_present(&mut self, name: &str) {
        self.character(name);
        self.set_on(name, true);
        self.enter(name, State::Available);
    }

    /// A character becomes present.
    pub fn enter(&mut self, name: &str, state: State) {
        self.service.present.insert(
            name.to_lowercase(),
            Present {
                name: name.into(),
                state,
            },
        );
        self.fed.arrived(&self.service, name);
    }

    /// A character is present no longer.
    pub fn leave(&mut self, name: &str) {
        self.service.present.remove(&name.to_lowercase());
        self.fed.left(&self.service, name);
    }

    /// A character lists a friend on another world.
    pub fn lists(&mut self, name: &str, friend: &str) {
        self.service
            .lists
            .entry(name.to_lowercase())
            .or_default()
            .push(addr(friend));
    }

    /// The link comes up and the hub welcomes the world, naming these worlds and whether each is
    /// linked; the frames the world sends after the welcome.
    pub fn link(&mut self, worlds: &[(&str, bool)]) -> Vec<Message> {
        self.link_with(worlds, Limits::DEFAULT)
    }

    /// [`World::link`] with the hub's own limits.
    pub fn link_with(&mut self, worlds: &[(&str, bool)], limits: Limits) -> Vec<Message> {
        self.fed.on_link_up();
        let hello = self.frames();
        assert!(
            matches!(hello.as_slice(), [Message::Hello { proto, .. }] if *proto == PROTOCOL),
            "hello goes first, alone: {hello:?}"
        );
        self.hub(&Message::Welcome {
            proto: PROTOCOL,
            world: SLUG.into(),
            hub: "test hub".into(),
            worlds: worlds
                .iter()
                .map(|(slug, online)| WorldInfo {
                    slug: (*slug).into(),
                    name: (*slug).into(),
                    online: *online,
                })
                .collect(),
            rooms: vec!["global".into()],
            limits,
        });
        self.frames()
    }

    /// A message from the hub.
    pub fn hub(&mut self, m: &Message) {
        self.frame(&m.to_json());
    }

    /// A frame from the hub, as text.
    pub fn frame(&mut self, text: &str) {
        self.fed.on_frame(&mut self.service, self.t, text);
    }

    /// What the world sends now; the directory's listings among it are kept in
    /// [`World::listings`].
    pub fn frames(&mut self) -> Vec<Message> {
        let mut out = Vec::new();
        for f in self.fed.take_frames(self.t) {
            match Message::from_json(&f) {
                Ok(m) => out.push(m),
                Err(_) => self.listings.push(
                    DirectoryMessage::from_json_for(Game::Auberean, &f)
                        .expect("a message the hub reads"),
                ),
            }
        }
        out
    }

    /// Time passes.
    pub fn advance(&mut self, secs: f64) {
        self.t += secs;
        self.fed.tick(&mut self.service, self.t);
    }

    /// A tell from another world to this one's `to`; the hub's id for it.
    pub fn told(&mut self, id: &str, from: &str, to: &str, text: &str) {
        self.hub(&Message::Tell {
            id: id.into(),
            from: addr(from),
            to: addr(to),
            text: text.into(),
            friend: false,
        });
    }

    /// The answers this world sends now, by tell id.
    pub fn answers(&mut self) -> Vec<(String, Outcome, Option<String>)> {
        self.frames()
            .into_iter()
            .filter_map(|m| match m {
                Message::TellResult {
                    id,
                    outcome,
                    message,
                } => Some((id, outcome, message)),
                _ => None,
            })
            .collect()
    }

    /// A `dn` command typed at the console; its lines.
    pub fn operator(&mut self, words: &[&str]) -> Vec<String> {
        self.fed
            .operator(&mut self.store, &mut self.service, self.t, words)
    }

    /// A tell from a present character of this world to another world's character.
    pub fn tell(&mut self, tell: TellId, from: &str, to: &str, text: &str) {
        self.fed
            .tell(&mut self.service, self.t, tell, from, &addr(to), text);
    }
}
