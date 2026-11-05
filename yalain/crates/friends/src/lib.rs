//! The world's friends service, sans-IO: what an Asheron's Call 2 world serves its players over
//! the friends stream, beside its login. Friends with presence, an ignore list, an away state, and
//! tells to `Name@world`, within the world and, through the world's link, to other worlds of
//! either game.
//!
//! **Depends on** the friends stream (`auberean-friends`: its elements and the world's half of one
//! stream), the network's protocol (`dereth-network-protocol`: addresses and the paces tells are
//! held to), the link's world half (`yalain-federation`, through its `Friends` boundary, which
//! this crate implements) and the store (`yalain-store`: each character's lists). **Used by** the
//! server executable (`yalain`), which owns the sockets, checks tickets by its login's rule, and
//! tells the service who is in the world.
//!
//! **Must never** touch a socket, a file or the clock: the host hands it bytes, the time and the
//! world's comings and goings, and takes the bytes to write and the lines to show.
//!
//! **Specified in** `docs/network/08-friends-stream.md` (the stream) and
//! `docs/network/07-ac2-world-behaviour.md` (what the world does with it).
//!
//! - **A stream, an account, a character.** A stream is bound to the account its ticket names,
//!   and to that account's character while it is in the world. A second stream for the account
//!   replaces the first. A character with a stream is **present**: online to its friends, and told
//!   through the service.
//! - **A hand-over.** A character whose stream came while it was already in the world (a new
//!   client taking over from one gone, whose entry waits for the old character to leave) stays
//!   present for [`HAND_OVER`] after it leaves; entering again within it, its friends see no
//!   change.
//! - **Within the world** friends, presence, ignore, away and tells need nothing more: every
//!   player with a stream has them, on the network or not.
//! - **Other worlds' characters** need the character on the Dereth Network (`@dn on`, said in
//!   chat) and the world linked; the link's half does the rest ([`Service::said`]).
//! - **A linked character's friends on other worlds** are its person's, kept by the hub: an add goes
//!   there and the link's half lists what its person friends name, beside what this service keeps.
//!   The client's friends page holds what it was last given, and only a list it asks for rebuilds it.
//!   So its first ask waits, a moment, for the character's first person view; a row a view adds in
//!   play comes with the next list, and one that goes stays, offline, until then, its removal
//!   answered as done.
//! - **A hidden character** ([`Service::set_hidden`], a cloaked admin) is not there to anyone: its
//!   friends here and on other worlds see it offline, a tell to it is answered as one to a
//!   character offline is, and it stops watching its friends on other worlds, whose watches would
//!   say it is here. Its client keeps its stream, and it may still tell whom it chooses.
//! - **A gagged character** ([`Service::set_gagged`]) tells nobody, here or on another world: it
//!   is shown [`GAGGED`], and its client that the tell could not be sent.
//! - **A renamed character** ([`Service::renamed`]) keeps its stream and its state. Every character's
//!   lists hold it by the new name; a friend present sees the old name go and the new one come.
//! - **Names.** A name whose world part is the world's own name (any case) or its slug is a
//!   character of the world, and is written back as `Name@` and the world's name exactly, so the
//!   client knows its own world. Every other character is written as its network address.
//!
//! The host calls [`Service::accept`], [`Service::on_bytes`], [`Service::closed`],
//! [`Service::take_tickets`] with [`Service::admit`] or [`Service::refuse`],
//! [`Service::entered`], [`Service::left`], [`Service::said`] and [`Service::poll`], then takes
//! [`Service::take_out`] and [`Service::take_lines`]. A test drives the same calls on a clock of
//! its own.

mod commands;
mod link;
mod names;
mod requests;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::net::IpAddr;
use std::time::Duration;

use auberean_friends::{
    ByeReason, Cx, Event, FriendStatus, Hr, Mode, ServerConfig, ServerEvent, ServerOut,
    ServerStream, Ticket,
};
use dereth_network_protocol::limits::Bucket;
use yalain_federation::{Federation, State, TellId};
use yalain_store::{NetworkList, Store, StoreResult};

pub use commands::HELP;
pub use names::Target;
pub use requests::tell_code;

/// The most streams one address may hold open at once (an IPv6 address counted by its /64).
pub const MAX_STREAMS_PER_ADDRESS: usize = 8;

/// The most streams the world holds open at once, by default.
pub const MAX_STREAMS: usize = 2_000;

/// The most names one character keeps on each of its lists.
pub const MAX_LISTED: usize = 100;

/// What a gagged character is told when it tells anyone through the service: the world's own line
/// for a gagged character's chat.
pub const GAGGED: &str =
    "You are unable to talk locally, globally, or send tells because you have been gagged.";

/// Refused tickets from one address within [`LOCKOUT`] past which its next are refused unchecked.
pub const REFUSALS: usize = 5;

/// See [`REFUSALS`]: the window refusals are counted in, and how long the address is refused
/// unchecked after.
pub const LOCKOUT: Duration = Duration::from_secs(60);

/// How long an added friend on another world waits for its world's answer before it is shown
/// offline.
pub const ADD_WAIT: Duration = Duration::from_secs(3);

/// How long a character handed over to a new client stays present after it leaves the world:
/// the logoff linger's length, so the new client has as long to enter after the old character
/// goes as the old one took to go.
pub const HAND_OVER: Duration = Duration::from_secs(19);

/// The name is already on the list.
pub const ALREADY_IN_LIST: Hr = Hr(0xC112_1009);

/// The name is not on the list.
pub const NOT_IN_LIST: Hr = Hr(0xC112_1401);

/// A refusal the client shows as its friends list's own error, with the world's line beside it.
pub const FRIEND_REFUSED: Hr = Hr(0x8000_4005);

/// What a character is told when its friends list is full.
pub const FRIENDS_FULL: &str = "Your friends list is full.";

/// What a character is told when its squelch list is full.
pub const IGNORED_FULL: &str = "Your squelch list is full.";

/// The host's mark for one connection.
pub type StreamId = u64;

/// What the world says of itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The world's name as its login shows it: what the client appends to a name of its own world.
    pub world_name: String,
    /// The world's network slug; empty for a world on no network.
    pub slug: String,
    /// The most streams open at once.
    pub max_streams: usize,
}

impl Config {
    /// A world of this name and slug, with the default bounds.
    #[must_use]
    pub fn new(world_name: &str, slug: &str) -> Self {
        Self {
            world_name: world_name.to_owned(),
            slug: slug.to_owned(),
            max_streams: MAX_STREAMS,
        }
    }
}

/// What the service asks of its host, for one connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Out {
    /// Write these bytes.
    Write(StreamId, Vec<u8>),
    /// Close the connection, after writing everything before this.
    Close(StreamId),
}

/// A ticket waiting for the host's check by the login's rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketCheck {
    pub stream: StreamId,
    pub cx: Cx,
    pub ticket: Ticket,
    /// The address it came from, as bounds count it.
    pub peer: String,
}

/// The account a stream is bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Account {
    id: u64,
}

/// One stream.
#[derive(Debug)]
struct Stream {
    session: ServerStream,
    /// The address it came from, as bounds count it.
    peer: String,
    /// The account its ticket admitted.
    account: Option<Account>,
}

/// A character in the world, by its account.
#[derive(Debug, Clone, PartialEq, Eq)]
struct InWorld {
    character: u64,
    name: String,
}

/// A character present: in the world with a stream.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Here {
    character: u64,
    name: String,
    stream: StreamId,
    state: State,
    /// Its stream came while it was already in the world: a client that took over from one gone
    /// (or a stream that came back). Its leaving is held for [`HAND_OVER`].
    late: bool,
}

/// A character handed over to a new client that left the world, still present until a time.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Held {
    name: String,
    until: Duration,
}

/// What a character lists, as the store keeps it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Lists {
    friends: Vec<String>,
    ignored: Vec<String>,
}

impl Lists {
    fn of(&self, list: NetworkList) -> &Vec<String> {
        match list {
            NetworkList::Friends => &self.friends,
            NetworkList::Ignored => &self.ignored,
        }
    }

    fn of_mut(&mut self, list: NetworkList) -> &mut Vec<String> {
        match list {
            NetworkList::Friends => &mut self.friends,
            NetworkList::Ignored => &mut self.ignored,
        }
    }
}

/// A tell through the link, waiting for its end.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Waiting {
    /// The stream, its account and the request that asked; none for the game's own tell, whose
    /// teller has been answered already.
    asked: Option<(StreamId, u64, Cx)>,
    teller: u64,
}

/// A friend on another world just added, its add answered once its world says its state.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Adding {
    stream: StreamId,
    cx: Cx,
    /// The friend as the character listed it.
    friend: dereth_network_protocol::Address,
    /// When it is shown offline if its world has not answered.
    until: Duration,
}

/// A client's ask for its friends, held for the character's first person view.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ListWait {
    stream: StreamId,
    cx: Cx,
    /// The character, in small letters.
    name: String,
    /// When it is answered if no view has come.
    until: Duration,
}

/// A change to a character's kept list the link's half made, for the store.
#[derive(Debug, Clone, PartialEq, Eq)]
struct KeptChange {
    character: u64,
    entry: String,
    on: bool,
}

/// The friends service.
#[derive(Debug)]
pub struct Service {
    config: Config,
    streams: HashMap<StreamId, Stream>,
    /// The stream bound to each account.
    by_account: HashMap<u64, StreamId>,
    /// The character each account has in the world, as the host says.
    in_world: HashMap<u64, InWorld>,
    /// The characters present, by their names in small letters.
    here: HashMap<String, Here>,
    /// The characters present though they left the world, by their accounts.
    held: HashMap<u64, Held>,
    /// Every character's lists, by id.
    lists: HashMap<u64, Lists>,
    /// The ids of characters with lists, or present, by their names in small letters.
    ids: HashMap<String, (u64, String)>,
    /// Each present character's tells within the world, paced as the network paces tells.
    sends: HashMap<u64, Bucket>,
    /// Tells to other worlds, by the service's mark.
    waiting: HashMap<TellId, Waiting>,
    /// Friends on other worlds being added, by the adding character's name in small letters and
    /// the friend's key.
    adding: HashMap<(String, String), Adding>,
    /// Adds shown offline for want of an answer: the late answer is shown only if it is online.
    assumed_offline: HashSet<(String, String)>,
    next_tell: TellId,
    /// Refused tickets, by address, and the addresses refused unchecked until a time.
    refusals: HashMap<String, Vec<Duration>>,
    locked: HashMap<String, Duration>,
    tickets: Vec<TicketCheck>,
    /// Streams with something for the host.
    dirty: BTreeSet<StreamId>,
    out: Vec<Out>,
    lines: Vec<(u64, String)>,
    /// The lists changed since the host last asked for the names each character ignores.
    ignores_changed: bool,
    /// The accounts with streams changed since the host last asked for them.
    accounts_changed: bool,
    /// The time of the call being served, for the link's callbacks.
    now: Duration,
    /// The characters the world has gagged, as the host says.
    gagged: HashSet<u64>,
    /// The characters no one is to know are here, as the host says.
    hidden: HashSet<u64>,
    /// Those in the world with a suffix, by the name a client shows, in small letters.
    suffixed: HashMap<String, String>,
    /// Clients' asks for their friends, waiting for a person view.
    lists_waiting: Vec<ListWait>,
    /// What the store has yet to hear of the kept lists.
    kept_changes: Vec<KeptChange>,
}

/// The time as the link's half counts it.
fn secs(now: Duration) -> f64 {
    now.as_secs_f64()
}

/// The key an address's bounds are counted under: an IPv4 address whole, an IPv6 one by its /64.
fn peer_key(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v) => v.to_string(),
        IpAddr::V6(v) => match v.to_ipv4_mapped() {
            Some(v4) => v4.to_string(),
            None => {
                let s = v.segments();
                format!("{:x}:{:x}:{:x}:{:x}::/64", s[0], s[1], s[2], s[3])
            }
        },
    }
}

/// A present character's mode: online, with its state.
fn mode_of(state: &State) -> Mode {
    match state {
        State::Available => Mode::ONLINE,
        State::Away(_) => Mode::ONLINE.with(Mode::AWAY),
        State::DoNotDisturb(_) => Mode::ONLINE.with(Mode::NOT_DISTURBED),
    }
}

impl Service {
    /// The service with nothing kept.
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self {
            config,
            streams: HashMap::new(),
            by_account: HashMap::new(),
            in_world: HashMap::new(),
            here: HashMap::new(),
            held: HashMap::new(),
            lists: HashMap::new(),
            ids: HashMap::new(),
            sends: HashMap::new(),
            waiting: HashMap::new(),
            adding: HashMap::new(),
            assumed_offline: HashSet::new(),
            next_tell: 1,
            refusals: HashMap::new(),
            locked: HashMap::new(),
            tickets: Vec::new(),
            dirty: BTreeSet::new(),
            out: Vec::new(),
            lines: Vec::new(),
            ignores_changed: true,
            accounts_changed: true,
            now: Duration::ZERO,
            gagged: HashSet::new(),
            hidden: HashSet::new(),
            suffixed: HashMap::new(),
            lists_waiting: Vec::new(),
            kept_changes: Vec::new(),
        }
    }

    /// The service with every character's lists as the store keeps them.
    ///
    /// # Errors
    /// The store failed.
    pub fn load(config: Config, store: &mut impl Store) -> StoreResult<Self> {
        let mut s = Self::new(config);
        for l in store.network_lists()? {
            if !s.ids.values().any(|(id, _)| *id == l.character) {
                match store.character(l.character)? {
                    Some(c) if c.deleted_at.is_none() => {
                        s.ids.insert(c.name.to_lowercase(), (c.id, c.name));
                    }
                    _ => continue,
                }
            }
            s.lists
                .entry(l.character)
                .or_default()
                .of_mut(l.list)
                .push(l.entry);
        }
        Ok(s)
    }

    /// The world's settings.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// How many streams are open.
    #[must_use]
    pub fn streams(&self) -> usize {
        self.streams.len()
    }

    /// Whether a character of this world (by name, any case) is present.
    #[must_use]
    pub fn is_present(&self, name: &str) -> bool {
        self.here.contains_key(&name.to_lowercase())
    }

    // ---- the streams ----------------------------------------------------------------------------

    /// The host accepted a connection from `ip`. Past the bounds it is told the world is full and
    /// closed.
    pub fn accept(&mut self, now: Duration, id: StreamId, ip: IpAddr) {
        self.now = now;
        let peer = peer_key(ip);
        let from_peer = self.streams.values().filter(|s| s.peer == peer).count();
        let full =
            self.streams.len() >= self.config.max_streams || from_peer >= MAX_STREAMS_PER_ADDRESS;
        let mut session = ServerStream::new(
            now,
            ServerConfig::new(&self.config.slug, &self.config.world_name),
        );
        if full {
            session.close(now, ByeReason::Full);
        }
        self.streams.insert(
            id,
            Stream {
                session,
                peer,
                account: None,
            },
        );
        self.dirty.insert(id);
    }

    /// Bytes read from a connection.
    pub fn on_bytes(
        &mut self,
        now: Duration,
        id: StreamId,
        bytes: &[u8],
        store: &mut impl Store,
        fed: Option<&mut Federation>,
    ) {
        self.now = now;
        if let Some(s) = self.streams.get_mut(&id) {
            s.session.on_bytes(now, bytes);
            self.dirty.insert(id);
        }
        self.settle(now, store, fed);
    }

    /// The host's connection ended.
    pub fn closed(
        &mut self,
        now: Duration,
        id: StreamId,
        store: &mut impl Store,
        fed: Option<&mut Federation>,
    ) {
        self.now = now;
        if let Some(s) = self.streams.get_mut(&id) {
            s.session.closed(now);
            self.dirty.insert(id);
        }
        self.settle(now, store, fed);
    }

    /// Run the clock: every stream's deadlines and keep-alive, and what the link's half handed in.
    pub fn poll(
        &mut self,
        now: Duration,
        store: &mut impl Store,
        mut fed: Option<&mut Federation>,
    ) {
        self.now = now;
        let late: Vec<(String, String)> = self
            .adding
            .iter()
            .filter(|(_, a)| a.until <= now)
            .map(|(k, _)| k.clone())
            .collect();
        for key in late {
            self.assumed_offline.insert(key.clone());
            self.added(&key, false);
        }
        self.answer_lists(fed.as_deref_mut());
        let gone: Vec<String> = self
            .held
            .values()
            .filter(|h| h.until <= now)
            .map(|h| h.name.clone())
            .collect();
        for name in gone {
            self.leave_stream_of(&name);
            self.depart(&name, fed.as_deref_mut());
        }
        self.dirty.extend(self.streams.keys().copied());
        self.settle(now, store, fed);
    }

    /// When the service next needs [`Service::poll`] if nothing arrives first.
    #[must_use]
    pub fn next_wake(&self) -> Option<Duration> {
        self.streams
            .values()
            .filter_map(|s| s.session.next_wake())
            .chain(self.adding.values().map(|a| a.until))
            .chain(self.lists_waiting.iter().map(|l| l.until))
            .chain(self.held.values().map(|h| h.until))
            .min()
    }

    /// The tickets waiting for the host's check.
    pub fn take_tickets(&mut self) -> Vec<TicketCheck> {
        std::mem::take(&mut self.tickets)
    }

    /// What to write and close, in order.
    pub fn take_out(&mut self) -> Vec<Out> {
        std::mem::take(&mut self.out)
    }

    /// The lines to show characters in their chat windows, as the world's own system text, by
    /// character id.
    pub fn take_lines(&mut self) -> Vec<(u64, String)> {
        std::mem::take(&mut self.lines)
    }

    /// The characters of this world each character ignores, by id, in small letters: what the
    /// game's own tell honours. `None` when nothing changed since the last call.
    pub fn local_ignores(&mut self) -> Option<HashMap<u64, HashSet<String>>> {
        if !std::mem::take(&mut self.ignores_changed) {
            return None;
        }
        Some(
            self.lists
                .iter()
                .map(|(id, l)| {
                    let names: HashSet<String> = l
                        .ignored
                        .iter()
                        .filter(|e| !e.contains('@'))
                        .map(|e| e.to_lowercase())
                        .collect();
                    (*id, names)
                })
                .filter(|(_, n)| !n.is_empty())
                .collect(),
        )
    }

    /// The accounts with an admitted stream. `None` when none came or went since the last call.
    pub fn accounts_with_streams(&mut self) -> Option<HashSet<u64>> {
        if !std::mem::take(&mut self.accounts_changed) {
            return None;
        }
        Some(self.by_account.keys().copied().collect())
    }

    /// The ticket a stream presented is good: it is bound to `account`, and to the account's
    /// character if it is in the world. A stream the account already had is replaced.
    pub fn admit(
        &mut self,
        now: Duration,
        id: StreamId,
        cx: Cx,
        account: u64,
        store: &mut impl Store,
        mut fed: Option<&mut Federation>,
    ) {
        self.now = now;
        let Some(s) = self.streams.get_mut(&id) else {
            return;
        };
        if !s.session.admit(now, cx) {
            return;
        }
        s.account = Some(Account { id: account });
        self.dirty.insert(id);
        self.accounts_changed = true;
        if let Some(old) = self.by_account.insert(account, id) {
            if let Some(o) = self.streams.get_mut(&old) {
                // Its end leaves the character present: the new stream takes its place.
                o.session.replace(now);
                self.dirty.insert(old);
            }
            if let Some(here) = self.here.values_mut().find(|h| h.stream == old) {
                here.stream = id;
                here.late = true;
                let name = here.name.clone();
                self.start(now, id, &name, fed.as_deref());
            }
        }
        if let Some(w) = self.in_world.get(&account).cloned() {
            if !self.here.contains_key(&w.name.to_lowercase()) {
                self.arrive(now, id, &w, true, fed.as_deref_mut());
            }
        }
        self.settle(now, store, fed);
    }

    /// The characters the world has gagged, as the world counts its gags: their tells through the
    /// service go nowhere.
    pub fn set_gagged(&mut self, characters: HashSet<u64>) {
        self.gagged = characters;
    }

    /// The characters in the world with a suffix, each as a client shows it and by its own name.
    pub fn set_suffixed(&mut self, characters: impl IntoIterator<Item = (String, String)>) {
        self.suffixed = characters
            .into_iter()
            .map(|(shown, own)| (shown.to_lowercase(), own))
            .collect();
    }

    /// The characters no one is to know are here, as the world says. Those newly so go offline to
    /// their friends, here and on other worlds, as if they had left; those no longer so come back
    /// as if they had entered. One that left the world but stays present after a hand-over stays
    /// as it was until it goes: it must not show online on its way out.
    pub fn set_hidden(
        &mut self,
        now: Duration,
        characters: HashSet<u64>,
        store: &mut impl Store,
        mut fed: Option<&mut Federation>,
    ) {
        self.now = now;
        let mut next = characters;
        for h in self.here.values() {
            let held = self
                .held
                .values()
                .any(|x| x.name.eq_ignore_ascii_case(&h.name));
            if held && self.hidden.contains(&h.character) {
                next.insert(h.character);
            }
        }
        if next == self.hidden {
            return;
        }
        let here: Vec<(u64, String, Mode)> = self
            .here
            .values()
            .map(|h| (h.character, h.name.clone(), mode_of(&h.state)))
            .collect();
        // In the world without a stream: nobody sees these come or go, but their person views do.
        let alone: Vec<(u64, String)> = self
            .in_world
            .values()
            .filter(|w| !self.here.contains_key(&w.name.to_lowercase()))
            .map(|w| (w.character, w.name.clone()))
            .collect();
        for (character, name, _) in &here {
            if next.contains(character) && !self.hidden.contains(character) {
                self.hide(name, fed.as_deref_mut());
            }
        }
        if let Some(f) = fed.as_deref_mut() {
            for (character, name) in &alone {
                if next.contains(character) && !self.hidden.contains(character) {
                    f.stop_view_of(name);
                }
            }
        }
        let lifted = std::mem::replace(&mut self.hidden, next);
        if let Some(f) = fed.as_deref_mut() {
            for (character, name) in &alone {
                if lifted.contains(character) && !self.hidden.contains(character) {
                    f.watch_view_of(&*self, name);
                }
            }
        }
        for (character, name, mode) in &here {
            if lifted.contains(character) && !self.hidden.contains(character) {
                self.tell_friends_of(name, *mode);
                if let Some(f) = fed.as_deref_mut() {
                    f.arrived(self, name);
                }
            }
        }
        self.settle(now, store, fed);
    }

    /// A present character is to be hidden: everyone sees it go. It stops watching its friends on
    /// other worlds, so what its client shows of them can't stand: it shows them offline too.
    fn hide(&mut self, name: &str, fed: Option<&mut Federation>) {
        self.tell_friends_of(name, Mode::OFFLINE);
        let Some(f) = fed else {
            return;
        };
        let shown: Vec<dereth_network_protocol::Address> = f
            .remote_of(&*self, name)
            .into_iter()
            .filter(|a| f.shown(name, a) == Some(true))
            .collect();
        f.left(self, name);
        f.stop_view_of(name);
        for friend in shown {
            yalain_federation::Friends::show_presence(self, name, &friend, false, true);
        }
    }

    /// Whether a character of this world is hidden, by its name.
    fn is_hidden(&self, name: &str) -> bool {
        self.id_of(name).is_some_and(|id| self.hidden.contains(&id))
    }

    /// The ticket a stream presented is refused (`banned`: the account is banned). Counted
    /// against the address it came from.
    pub fn refuse(
        &mut self,
        now: Duration,
        id: StreamId,
        cx: Cx,
        banned: bool,
        store: &mut impl Store,
        fed: Option<&mut Federation>,
    ) {
        self.now = now;
        let Some(s) = self.streams.get_mut(&id) else {
            return;
        };
        let hr = if banned {
            Hr::BANNED
        } else {
            Hr::TICKET_REFUSED
        };
        if !s.session.refuse(now, cx, hr) {
            return;
        }
        let peer = s.peer.clone();
        self.dirty.insert(id);
        let times = self.refusals.entry(peer.clone()).or_default();
        times.retain(|t| now.saturating_sub(*t) < LOCKOUT);
        times.push(now);
        if times.len() >= REFUSALS {
            times.clear();
            self.locked.insert(peer, now + LOCKOUT);
        }
        self.settle(now, store, fed);
    }

    // ---- the world ------------------------------------------------------------------------------

    /// An account's character entered the world. With a stream, it is present from now on.
    pub fn entered(
        &mut self,
        now: Duration,
        account: u64,
        character: u64,
        name: &str,
        store: &mut impl Store,
        mut fed: Option<&mut Federation>,
    ) {
        self.now = now;
        let w = InWorld {
            character,
            name: name.to_owned(),
        };
        self.ids
            .insert(name.to_lowercase(), (character, name.to_owned()));
        self.in_world.insert(account, w.clone());
        if let Some(held) = self.held.remove(&account) {
            if held.name.eq_ignore_ascii_case(name) {
                self.came_back(now, name, fed.as_deref());
                self.settle(now, store, fed);
                return;
            }
            self.leave_stream_of(&held.name);
            self.depart(&held.name, fed.as_deref_mut());
        }
        if let Some(&id) = self.by_account.get(&account) {
            self.arrive(now, id, &w, false, fed.as_deref_mut());
        } else if let Some(f) = fed.as_deref_mut() {
            f.watch_view_of(&*self, name);
        }
        self.settle(now, store, fed);
    }

    /// A character held after a hand-over entered again: it never stopped being present, so its
    /// friends are told nothing, and its stream, whose client now has the character, is sent its
    /// friends again.
    fn came_back(&mut self, now: Duration, name: &str, fed: Option<&Federation>) {
        let Some(h) = self.here.get_mut(&name.to_lowercase()) else {
            return;
        };
        h.late = false;
        let (stream, name) = (h.stream, h.name.clone());
        self.start(now, stream, &name, fed);
    }

    /// The stream of a present character hears that no character of its account is in the world.
    fn leave_stream_of(&mut self, name: &str) {
        let Some(stream) = self.here.get(&name.to_lowercase()).map(|h| h.stream) else {
            return;
        };
        if let Some(s) = self.streams.get_mut(&stream) {
            s.session.leave();
        }
    }

    /// An account's character left the world. It is present no longer, unless its stream came
    /// while it was in the world (a new client taking over from one gone, whose entry waited for
    /// it to leave): then it stays present for [`HAND_OVER`], and entering again within it, its
    /// friends see no change.
    pub fn left(
        &mut self,
        now: Duration,
        account: u64,
        store: &mut impl Store,
        mut fed: Option<&mut Federation>,
    ) {
        self.now = now;
        if let Some(w) = self.in_world.remove(&account) {
            if let Some(h) = self.here.get(&w.name.to_lowercase()) {
                if h.late {
                    self.held.insert(
                        account,
                        Held {
                            name: h.name.clone(),
                            until: now + HAND_OVER,
                        },
                    );
                } else {
                    self.leave_stream_of(&w.name);
                    self.depart(&w.name, fed.as_deref_mut());
                }
            } else if !self.hidden.contains(&w.character) {
                if let Some(f) = fed.as_deref_mut() {
                    f.stop_view_of(&w.name);
                }
            }
        }
        self.settle(now, store, fed);
    }

    /// A stream bound to an account whose character is in the world: the character is present,
    /// told its friends, and its friends told of it.
    fn arrive(
        &mut self,
        now: Duration,
        id: StreamId,
        w: &InWorld,
        late: bool,
        fed: Option<&mut Federation>,
    ) {
        let key = w.name.to_lowercase();
        if self.here.contains_key(&key) {
            return;
        }
        self.here.insert(
            key,
            Here {
                character: w.character,
                name: w.name.clone(),
                stream: id,
                state: State::Available,
                late,
            },
        );
        self.start(now, id, &w.name, fed.as_deref());
        self.tell_friends_of(&w.name, Mode::ONLINE);
        if let Some(f) = fed.filter(|_| !self.hidden.contains(&w.character)) {
            f.arrived(self, &w.name);
        }
    }

    /// A character took a new name, which the store has now, in everyone's lists too. It stays
    /// present with the stream and state it had. Whoever lists it and is present sees the old name
    /// leave and the new one arrive: a client shows no other way to change one name to another, and
    /// its friends window finds the new name only when it next asks for the list.
    pub fn renamed(
        &mut self,
        now: Duration,
        character: u64,
        old: &str,
        new: &str,
        store: &mut impl Store,
        mut fed: Option<&mut Federation>,
    ) {
        self.now = now;
        let (from, to) = (old.to_lowercase(), new.to_lowercase());
        if self.here.contains_key(&from) {
            self.tell_friends_of(old, Mode::OFFLINE);
        }
        if let Some(f) = fed.as_deref_mut() {
            f.renaming(self, old);
        }
        for l in self.lists.values_mut() {
            names::rename(&mut l.friends, &from, &to, new);
            names::rename(&mut l.ignored, &from, &to, new);
        }
        self.ignores_changed = true;
        self.ids.remove(&from);
        self.ids.insert(to.clone(), (character, new.to_owned()));
        for w in self
            .in_world
            .values_mut()
            .filter(|w| w.character == character)
        {
            new.clone_into(&mut w.name);
        }
        if let Some(mut h) = self.here.remove(&from) {
            new.clone_into(&mut h.name);
            self.here.insert(to.clone(), h);
        }
        let adding: Vec<(String, String)> = self
            .adding
            .keys()
            .filter(|(c, _)| *c == from)
            .cloned()
            .collect();
        for key in adding {
            if let Some(a) = self.adding.remove(&key) {
                self.adding.insert((to.clone(), key.1), a);
            }
        }
        let assumed: Vec<(String, String)> = self
            .assumed_offline
            .iter()
            .filter(|(c, _)| *c == from)
            .cloned()
            .collect();
        for key in assumed {
            self.assumed_offline.remove(&key);
            self.assumed_offline.insert((to.clone(), key.1));
        }
        for l in self.lists_waiting.iter_mut().filter(|l| l.name == from) {
            l.name.clone_from(&to);
        }
        if let Some(f) = fed.as_deref_mut() {
            f.renamed(self, old, new);
        }
        if let Some(mode) = self.here.get(&to).map(|h| mode_of(&h.state)) {
            self.tell_friends_of(new, mode);
        }
        self.settle(now, store, fed);
    }

    /// Sends a present character's stream its friends, each with its state.
    fn start(&mut self, now: Duration, id: StreamId, name: &str, fed: Option<&Federation>) {
        let friends = self.friends_of(name, fed);
        if let Some(s) = self.streams.get_mut(&id) {
            if let Err(e) = s.session.enter(now, friends) {
                tracing::debug!("friends service: the friends list was not sent: {e}");
            }
            self.dirty.insert(id);
        }
    }

    /// A character is present no longer: its friends see it offline, and it stops watching.
    fn depart(&mut self, name: &str, fed: Option<&mut Federation>) {
        let Some(h) = self.here.remove(&name.to_lowercase()) else {
            return;
        };
        self.held
            .retain(|_, held| !held.name.eq_ignore_ascii_case(&h.name));
        self.sends.remove(&h.character);
        // Its adds still waiting are answered (offline): the client waits for one answer each.
        let key = h.name.to_lowercase();
        let waiting: Vec<(String, String)> = self
            .adding
            .keys()
            .filter(|(c, _)| *c == key)
            .cloned()
            .collect();
        for k in waiting {
            self.added(&k, false);
        }
        self.assumed_offline.retain(|(c, _)| *c != key);
        self.tell_friends_of(&h.name, Mode::OFFLINE);
        if let Some(f) = fed.filter(|_| !self.hidden.contains(&h.character)) {
            f.left(self, &h.name);
        }
    }

    /// Every present character that lists `name` (a character of this world) is told its mode;
    /// nobody is told anything of a hidden one.
    fn tell_friends_of(&mut self, name: &str, mode: Mode) {
        if self.is_hidden(name) {
            return;
        }
        let key = name.to_lowercase();
        let user = self.written_local(name);
        let listing: Vec<StreamId> = self
            .here
            .values()
            .filter(|h| {
                self.lists.get(&h.character).is_some_and(|l| {
                    l.friends
                        .iter()
                        .any(|e| !e.contains('@') && e.to_lowercase() == key)
                })
            })
            .map(|h| h.stream)
            .collect();
        for stream in listing {
            self.event(
                stream,
                Event::FriendStat {
                    user: user.clone(),
                    mode,
                },
            );
        }
    }

    /// The friends a character lists, each with its state now: those it keeps, and those its person
    /// friends list.
    pub(crate) fn friends_of(&self, name: &str, fed: Option<&Federation>) -> Vec<FriendStatus> {
        let kept = self
            .id_of(name)
            .and_then(|id| self.lists.get(&id))
            .map(|l| l.friends.clone())
            .unwrap_or_default();
        let persons = fed.map_or_else(Vec::new, |f| f.person_friends(name));
        kept.into_iter()
            .chain(persons.iter().map(ToString::to_string))
            .map(|e| {
                let (user, mode) = self.status_of(name, &e, fed);
                FriendStatus::new(&user, mode)
            })
            .collect()
    }

    /// Answers the asks for friends whose time has come, or whose view has.
    fn answer_lists(&mut self, mut fed: Option<&mut Federation>) {
        let now = self.now;
        let due: Vec<ListWait> = {
            let (due, waiting) = std::mem::take(&mut self.lists_waiting)
                .into_iter()
                .partition(|l| l.until <= now);
            self.lists_waiting = waiting;
            due
        };
        for l in due {
            self.give_list(l.stream, l.cx, &l.name, fed.as_deref_mut());
        }
    }

    /// Answers a client's ask for its friends.
    pub(crate) fn give_list(
        &mut self,
        stream: StreamId,
        cx: Cx,
        name: &str,
        fed: Option<&mut Federation>,
    ) {
        let friends = self.friends_of(name, fed.as_deref());
        if let Some(f) = fed {
            f.list_given(name);
        }
        self.answer(
            stream,
            cx,
            Hr::OK,
            auberean_friends::Answer::GetFriendsList { friends },
        );
    }

    /// Tells the store what the link's half changed in the kept lists.
    fn store_kept_changes(&mut self, store: &mut impl Store) {
        for c in std::mem::take(&mut self.kept_changes) {
            if let Err(e) = store.set_listed(c.character, NetworkList::Friends, &c.entry, c.on) {
                tracing::warn!("friends service: the store: {e}");
            }
        }
    }

    /// A listed name as the client is shown it, with its mode.
    fn status_of(&self, owner: &str, entry: &str, fed: Option<&Federation>) -> (String, Mode) {
        match names::read_entry(entry) {
            Target::Local(n) => (self.written_local(&n), self.local_mode(&n)),
            Target::Remote(a) => {
                let online = fed.and_then(|f| f.shown(owner, &a)) == Some(true);
                (
                    a.to_string(),
                    if online { Mode::ONLINE } else { Mode::OFFLINE },
                )
            }
            Target::Channel(_) | Target::NoSuchUser => (entry.to_owned(), Mode::OFFLINE),
        }
    }

    /// A character of this world's mode now: a hidden one is offline.
    fn local_mode(&self, name: &str) -> Mode {
        self.here
            .get(&name.to_lowercase())
            .filter(|h| !self.hidden.contains(&h.character))
            .map_or(Mode::OFFLINE, |h| mode_of(&h.state))
    }

    /// Sends an event on a stream.
    fn event(&mut self, stream: StreamId, event: Event) {
        if let Some(s) = self.streams.get_mut(&stream) {
            if let Err(e) = s.session.event(self.now, event) {
                tracing::debug!("friends service: an event was not sent: {e}");
            }
            self.dirty.insert(stream);
        }
    }

    /// Answers a request on a stream.
    fn answer(&mut self, stream: StreamId, cx: Cx, hr: Hr, answer: auberean_friends::Answer) {
        if let Some(s) = self.streams.get_mut(&stream) {
            if let Err(e) = s.session.answer(self.now, cx, hr, answer) {
                tracing::debug!("friends service: an answer was not sent: {e}");
            }
            self.dirty.insert(stream);
        }
    }

    /// A line for a character's chat window.
    fn line(&mut self, character: u64, text: impl Into<String>) {
        self.lines.push((character, text.into()));
    }

    /// Drains every stream with something for the host, handling what each says, until nothing
    /// is left.
    fn settle(&mut self, now: Duration, store: &mut impl Store, mut fed: Option<&mut Federation>) {
        self.store_kept_changes(store);
        while let Some(id) = self.dirty.pop_first() {
            let Some(s) = self.streams.get_mut(&id) else {
                continue;
            };
            for o in s.session.poll(now) {
                match o {
                    ServerOut::Write(bytes) => self.out.push(Out::Write(id, bytes)),
                    ServerOut::Close => self.out.push(Out::Close(id)),
                    ServerOut::Event(e) => self.on_event(now, id, e, store, fed.as_deref_mut()),
                }
            }
        }
    }

    fn on_event(
        &mut self,
        now: Duration,
        id: StreamId,
        e: ServerEvent,
        store: &mut impl Store,
        fed: Option<&mut Federation>,
    ) {
        match e {
            ServerEvent::Hello(_) => {}
            ServerEvent::Ticket { cx, ticket } => self.ticket(now, id, cx, &ticket),
            ServerEvent::Request { cx, request } => {
                self.request(id, cx, request, store, fed);
            }
            ServerEvent::Ended(how) => {
                tracing::debug!("friends service: stream {id} ended: {how:?}");
                self.ended(id, fed);
            }
        }
    }

    /// A ticket presented: refused at once when it does not read or its address is refused
    /// unchecked for now; otherwise handed to the host.
    fn ticket(&mut self, now: Duration, id: StreamId, cx: Cx, text: &str) {
        let Some(peer) = self.streams.get(&id).map(|s| s.peer.clone()) else {
            return;
        };
        if self.locked.get(&peer).is_some_and(|until| now < *until) {
            if let Some(s) = self.streams.get_mut(&id) {
                s.session.refuse(now, cx, Hr::TICKET_REFUSED);
            }
            self.dirty.insert(id);
            return;
        }
        self.locked.remove(&peer);
        match Ticket::decode(text) {
            Ok(ticket) => self.tickets.push(TicketCheck {
                stream: id,
                cx,
                ticket,
                peer: peer.clone(),
            }),
            Err(_) => {
                // Counted as any refusal.
                let times = self.refusals.entry(peer.clone()).or_default();
                times.retain(|t| now.saturating_sub(*t) < LOCKOUT);
                times.push(now);
                if times.len() >= REFUSALS {
                    times.clear();
                    self.locked.insert(peer, now + LOCKOUT);
                }
                if let Some(s) = self.streams.get_mut(&id) {
                    s.session.refuse(now, cx, Hr::TICKET_REFUSED);
                }
                self.dirty.insert(id);
            }
        }
    }

    /// A stream is over: forgotten, and its character present no longer unless a newer stream
    /// took its place.
    fn ended(&mut self, id: StreamId, fed: Option<&mut Federation>) {
        let Some(s) = self.streams.remove(&id) else {
            return;
        };
        self.tickets.retain(|t| t.stream != id);
        self.waiting
            .retain(|_, w| w.asked.is_none_or(|(stream, _, _)| stream != id));
        self.adding.retain(|_, a| a.stream != id);
        self.lists_waiting.retain(|l| l.stream != id);
        let Some(account) = s.account else {
            return;
        };
        if self.by_account.get(&account.id) == Some(&id) {
            self.by_account.remove(&account.id);
            self.accounts_changed = true;
        }
        let gone = self
            .here
            .values()
            .find(|h| h.stream == id)
            .map(|h| h.name.clone());
        if let Some(name) = gone {
            self.depart(&name, fed);
        }
    }

    /// The present character a stream acts for.
    fn here_on(&self, stream: StreamId) -> Option<&Here> {
        self.here.values().find(|h| h.stream == stream)
    }

    /// A character's id by name (any case), if it has lists or is present.
    fn id_of(&self, name: &str) -> Option<u64> {
        self.ids.get(&name.to_lowercase()).map(|(id, _)| *id)
    }

    /// Whether a character of this world ignores another of this world (by name).
    fn ignores_local(&self, character: u64, who: &str) -> bool {
        let who = who.to_lowercase();
        self.lists.get(&character).is_some_and(|l| {
            l.ignored
                .iter()
                .any(|e| !e.contains('@') && e.to_lowercase() == who)
        })
    }

    /// The mark for a new tell to another world.
    fn fresh_tell(&mut self) -> TellId {
        let t = self.next_tell;
        self.next_tell += 1;
        t
    }

    /// Keeps a name on a character's list, or takes it off, in the store and here.
    fn keep(
        &mut self,
        store: &mut impl Store,
        character: u64,
        name: &str,
        list: NetworkList,
        entry: &str,
        on: bool,
    ) -> StoreResult<()> {
        store.set_listed(character, list, entry, on)?;
        self.ids
            .insert(name.to_lowercase(), (character, name.to_owned()));
        let l = self.lists.entry(character).or_default().of_mut(list);
        let key = entry.to_lowercase();
        if on {
            if !l.iter().any(|e| e.to_lowercase() == key) {
                l.push(entry.to_owned());
            }
        } else {
            l.retain(|e| e.to_lowercase() != key);
        }
        if list == NetworkList::Ignored {
            self.ignores_changed = true;
        }
        Ok(())
    }

    /// Whether a character lists a name (as its entry, any case).
    fn listed(&self, character: u64, list: NetworkList, entry: &str) -> bool {
        let key = entry.to_lowercase();
        self.lists
            .get(&character)
            .is_some_and(|l| l.of(list).iter().any(|e| e.to_lowercase() == key))
    }

    /// How many whole worlds a character squelches.
    fn count_worlds(&self, character: u64) -> usize {
        self.lists.get(&character).map_or(0, |l| {
            l.ignored
                .iter()
                .filter(|e| names::is_world_entry(e))
                .count()
        })
    }

    /// How many names a character lists.
    fn count(&self, character: u64, list: NetworkList) -> usize {
        self.lists.get(&character).map_or(0, |l| l.of(list).len())
    }
}
