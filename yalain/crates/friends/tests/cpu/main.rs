//! The friends service driven as the host drives it, on a clock of the test's own: clients that
//! speak the stream's frames, a store in memory, and, for other worlds, the link's half with a
//! hub played by the test.

mod commands;
mod friends;
mod network;
mod presence;
mod streams;
mod tells;

use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use auberean_friends::{
    Answer, ByeReason, Cx, Element, Event, FrameReader, FriendStatus, Hello, Hr, Mode, Request,
    Ticket, PREAMBLE, PROTO,
};
use dereth_network_protocol::limits::Limits;
use dereth_network_protocol::wire::{WireError, PROTOCOL};
use dereth_network_protocol::{Address, Message, RoomsMessage, WorldInfo};
use yalain_federation::{Federation, GlobalLine, Settings};
use yalain_friends::{Config, Out, Service, StreamId, GAGGED};
use yalain_store::{Character, MemoryStore, Store};

/// This world's name, as its login shows it, and its slug.
pub const NAME: &str = "Dawnsong";
pub const SLUG: &str = "dawnsong";

/// A world's friends service, its store, its link's half (when it is on the network) and its
/// clock; and what each client was sent.
#[derive(Debug)]
pub struct World {
    pub service: Service,
    pub store: MemoryStore,
    pub fed: Option<Federation>,
    pub now: Duration,
    next_stream: StreamId,
    readers: BTreeMap<StreamId, FrameReader>,
    pub got: BTreeMap<StreamId, Vec<Element>>,
    pub closed: BTreeSet<StreamId>,
    pub lines: Vec<(u64, String)>,
    next_cx: u64,
}

pub fn addr(s: &str) -> Address {
    Address::parse(s).expect("an address")
}

/// The world's own address for one of its characters.
pub fn here(name: &str) -> String {
    format!("{name}@{NAME}")
}

pub fn status(user: &str, mode: Mode) -> FriendStatus {
    FriendStatus::new(user, mode)
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// A world on no network.
    pub fn new() -> Self {
        Self::with(None)
    }

    /// A world on the network as `dawnsong`, its link up and welcomed, `coldeve` linked.
    pub fn linked() -> Self {
        let fed = Federation::new(
            Settings {
                enabled: true,
                slug: SLUG.into(),
                software: "yalain test".into(),
                ..Settings::default()
            },
            1_790_000_000,
        );
        let mut w = Self::with(Some(fed));
        let t = w.now.as_secs_f64();
        let f = w.fed.as_mut().expect("the link's half");
        f.on_link_up();
        let _hello = f.take_frames(t);
        w.hub(&Message::Welcome {
            proto: PROTOCOL,
            world: SLUG.into(),
            hub: "test hub".into(),
            worlds: vec![WorldInfo {
                slug: "coldeve".into(),
                name: "Coldeve".into(),
                online: true,
            }],
            rooms: Vec::new(),
            limits: Limits::DEFAULT,
        });
        w.frames();
        w
    }

    fn with(fed: Option<Federation>) -> Self {
        let slug = if fed.is_some() { SLUG } else { "" };
        Self {
            service: Service::new(Config::new(NAME, slug)),
            store: MemoryStore::default(),
            fed,
            now: Duration::from_secs(1_000),
            next_stream: 1,
            readers: BTreeMap::new(),
            got: BTreeMap::new(),
            closed: BTreeSet::new(),
            lines: Vec::new(),
            next_cx: 0x100,
        }
    }

    /// Makes an account and a character of it in the store: the account's id and the
    /// character's.
    pub fn character(&mut self, name: &str) -> (u64, u64) {
        let account = self
            .store
            .create_account(&format!("acct-{name}"), None, 0)
            .expect("an account")
            .id;
        let id = 0x5000_0000 + self.store.next_character_number().expect("a number");
        self.store
            .insert_character(&Character {
                id,
                account,
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
        (account, id)
    }

    /// The account and character ids of a character.
    pub fn ids(&mut self, name: &str) -> (u64, u64) {
        let c = self
            .store
            .character_named(name)
            .expect("reads")
            .expect("a character");
        (c.account, c.id)
    }

    /// A connection from `ip`, greeted: its id.
    pub fn connect_from(&mut self, ip: IpAddr) -> StreamId {
        let id = self.next_stream;
        self.next_stream += 1;
        self.readers.insert(id, FrameReader::new());
        self.service.accept(self.now, id, ip);
        let mut bytes = PREAMBLE.to_vec();
        Element::Hello(Hello {
            proto: PROTO,
            software: "test/1".into(),
            locale: None,
        })
        .write_frame(&mut bytes)
        .expect("fits");
        self.bytes(id, &bytes);
        id
    }

    /// A connection from this machine, greeted.
    pub fn connect(&mut self) -> StreamId {
        self.connect_from(IpAddr::V4(Ipv4Addr::LOCALHOST))
    }

    /// Bytes from a client.
    pub fn bytes(&mut self, id: StreamId, bytes: &[u8]) {
        self.service
            .on_bytes(self.now, id, bytes, &mut self.store, self.fed.as_mut());
        self.flush();
    }

    /// An element from a client.
    pub fn send(&mut self, id: StreamId, e: &Element) {
        let mut bytes = Vec::new();
        e.write_frame(&mut bytes).expect("fits");
        self.bytes(id, &bytes);
    }

    /// A request from a client: its mark.
    pub fn request(&mut self, id: StreamId, request: Request) -> Cx {
        self.next_cx += 1;
        let cx = Cx(self.next_cx);
        self.send(id, &Element::Request { cx, request });
        cx
    }

    /// Presents a ticket for `account` on a stream: its mark. The host's check is left waiting.
    pub fn present(&mut self, id: StreamId, account: &str) -> Cx {
        let ticket = Ticket::new(account, "secret").encode().expect("a ticket");
        self.request(
            id,
            Request::PresentTicket {
                email: String::new(),
                ticket,
            },
        )
    }

    /// The host checks every waiting ticket: an account in the store is admitted, any other
    /// refused.
    pub fn check_tickets(&mut self) {
        for t in self.service.take_tickets() {
            match self
                .store
                .account_by_name(&t.ticket.account)
                .expect("reads")
            {
                Some(a) => self.service.admit(
                    self.now,
                    t.stream,
                    t.cx,
                    a.id,
                    &mut self.store,
                    self.fed.as_mut(),
                ),
                None => self.service.refuse(
                    self.now,
                    t.stream,
                    t.cx,
                    false,
                    &mut self.store,
                    self.fed.as_mut(),
                ),
            }
        }
        self.flush();
    }

    /// A stream for a character's account, admitted.
    pub fn stream(&mut self, name: &str) -> StreamId {
        let id = self.connect();
        self.present(id, &format!("acct-{name}"));
        self.check_tickets();
        self.take(id);
        id
    }

    /// A character enters the world.
    pub fn enter(&mut self, name: &str) {
        let (account, character) = self.ids(name);
        self.service.entered(
            self.now,
            account,
            character,
            name,
            &mut self.store,
            self.fed.as_mut(),
        );
        self.flush();
    }

    /// A character leaves the world.
    pub fn leave(&mut self, name: &str) {
        let (account, _) = self.ids(name);
        self.service
            .left(self.now, account, &mut self.store, self.fed.as_mut());
        self.flush();
    }

    /// An admin renames a character: the store first, then the service, as the host tells it.
    pub fn rename(&mut self, old: &str, new: &str) {
        let (_, character) = self.ids(old);
        self.store
            .rename_character(character, new)
            .expect("renamed");
        self.service.renamed(
            self.now,
            character,
            old,
            new,
            &mut self.store,
            self.fed.as_mut(),
        );
        self.flush();
    }

    /// A character made, with a stream, in the world: its stream, its welcome taken.
    pub fn player(&mut self, name: &str) -> StreamId {
        self.character(name);
        let id = self.stream(name);
        self.enter(name);
        self.take(id);
        id
    }

    /// The client's connection ended.
    pub fn drop_stream(&mut self, id: StreamId) {
        self.service
            .closed(self.now, id, &mut self.store, self.fed.as_mut());
        self.flush();
    }

    /// Time passes; the clocks run.
    pub fn advance(&mut self, secs: u64) {
        self.now += Duration::from_secs(secs);
        let t = self.now.as_secs_f64();
        if let Some(f) = self.fed.as_mut() {
            f.tick(&mut self.service, t);
        }
        self.flush();
    }

    /// What the service has for the host, routed to the clients.
    pub fn flush(&mut self) {
        self.service
            .poll(self.now, &mut self.store, self.fed.as_mut());
        for o in self.service.take_out() {
            match o {
                Out::Write(id, bytes) => {
                    let r = self.readers.get_mut(&id).expect("a client");
                    r.push(&bytes);
                    while let Some(text) = r.next_frame().expect("a frame") {
                        let e = Element::parse(&text).expect("an element");
                        self.got.entry(id).or_default().push(e);
                    }
                }
                Out::Close(id) => {
                    self.closed.insert(id);
                }
            }
        }
        self.lines.extend(self.service.take_lines());
    }

    /// What a client was sent since last asked, keep-alive left out.
    pub fn take(&mut self, id: StreamId) -> Vec<Element> {
        self.got
            .remove(&id)
            .unwrap_or_default()
            .into_iter()
            .filter(|e| !matches!(e, Element::Ping { .. } | Element::Pong { .. }))
            .collect()
    }

    /// The lines a character was shown since last asked.
    pub fn lines_of(&mut self, name: &str) -> Vec<String> {
        let (_, id) = self.ids(name);
        let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.lines)
            .into_iter()
            .partition(|(c, _)| *c == id);
        self.lines = rest;
        mine.into_iter().map(|(_, l)| l).collect()
    }

    /// A message from the hub.
    pub fn hub(&mut self, m: &Message) {
        let t = self.now.as_secs_f64();
        if let Some(f) = self.fed.as_mut() {
            f.on_frame(&mut self.service, t, &m.to_json());
        }
        self.flush();
    }

    /// What the world sends the hub now, the directory's listings aside.
    pub fn frames(&mut self) -> Vec<Message> {
        let t = self.now.as_secs_f64();
        self.fed.as_mut().map_or_else(Vec::new, |f| {
            f.take_frames(t)
                .iter()
                .filter_map(|f| match Message::from_json(f) {
                    Err(WireError::Unknown(what)) if what == "listing" => None,
                    m => Some(m.expect("a message the hub reads")),
                })
                .collect()
        })
    }

    /// The hub says the world carries the Global room in `room`.
    pub fn carry_global_in(&mut self, room: &str) {
        let frame = RoomsMessage::Setup {
            id: None,
            global: Some(room.to_owned()),
            mirrored: Vec::new(),
        }
        .to_json();
        let t = self.now.as_secs_f64();
        let f = self.fed.as_mut().expect("the link's half");
        f.on_frame(&mut self.service, t, &frame);
    }

    /// The `@dng` lines kept for the host to say.
    pub fn global_lines(&mut self) -> Vec<GlobalLine> {
        self.fed
            .as_mut()
            .expect("the link's half")
            .take_global_lines()
    }

    /// What a character's `@dn` and the words after it answer.
    pub fn said(&mut self, name: &str, words: &str) -> Vec<String> {
        let (_, id) = self.ids(name);
        let lines = self.service.said(
            self.now,
            &mut self.store,
            self.fed.as_mut(),
            id,
            name,
            words,
        );
        self.flush();
        lines
    }

    /// Puts a character on the network, as `@dn on` does.
    pub fn on(&mut self, name: &str) {
        let lines = self.said(name, "on");
        assert!(
            lines[0].starts_with("You are on the Dereth Network as"),
            "{lines:?}"
        );
    }
}

/// The one answer with a mark among elements: its code and body.
pub fn answer(got: &[Element], cx: Cx) -> (Hr, Answer) {
    let found: Vec<(Hr, Answer)> = got
        .iter()
        .filter_map(|e| match e {
            Element::Answer { cx: c, hr, answer } if *c == cx => Some((*hr, answer.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(found.len(), 1, "one answer for {cx}: {got:?}");
    found.into_iter().next().expect("one")
}

/// The events among elements.
pub fn events(got: &[Element]) -> Vec<Event> {
    got.iter()
        .filter_map(|e| match e {
            Element::Event(e) => Some(e.clone()),
            _ => None,
        })
        .collect()
}

/// A friend's state as an event.
pub fn stat(user: &str, mode: Mode) -> Event {
    Event::FriendStat {
        user: user.into(),
        mode,
    }
}

/// Whether a client was told goodbye for `reason`.
pub fn bye(got: &[Element], reason: ByeReason) -> bool {
    got.iter()
        .any(|e| matches!(e, Element::Bye { reason: r } if *r == reason))
}
