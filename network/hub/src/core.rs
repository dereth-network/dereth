//! The hub's routing, as a state machine with no I/O: links in, messages to send out.
//!
//! [`Hub`] holds what the hub knows while it runs: which worlds have a link up, which rooms each
//! carries, the tells it is waiting on, the tells it holds for a world whose link is down, the
//! rate-limit buckets, the channels, and each world's `person.watch`es with the view it was last
//! sent. Persons themselves are the registry's ([`Directory`]): the routing asks it, and when a
//! person changes, looks again at the watched views that are its characters' or name one of them,
//! and sends each that changed. Every input names the time, in seconds on the caller's monotonic
//! clock, and returns the [`Out`]s to act on. Nothing here keeps a message body past the moment it
//! is handed on, except a tell held for a world that is reconnecting (at most
//! [`Limits::hold_secs`]), and nothing here logs one.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use dereth_network_protocol::limits::{Bucket, Limits};
use dereth_network_protocol::person::MAX_PERSON_CHARACTERS;
use dereth_network_protocol::wire::{ErrorCode, MAX_MESSAGE_BYTES, PROTOCOL};
use dereth_network_protocol::{
    Address, Alts, ChannelOp, ChannelOutcome, Federation, Message, Outcome, PersonFriend, PersonOp,
    PersonOutcome, WorldInfo,
};
use serde::{Deserialize, Serialize};

/// What the hub knows of a world from the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub federation: Federation,
}

/// One character's person view, as the hub sends it to the character's world
/// (`docs/network/02-link-protocol.md` section 6).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PersonView {
    pub linked: bool,
    pub alts: Alts,
    pub persons: Vec<PersonFriend>,
}

/// A person request's answer, and the characters of each person it changed (none when it changed
/// nothing): their views, and every view that names one of them, may differ now. Those characters
/// are the routing's alone, to find the views to look at again; no world is told them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonAnswer {
    pub outcome: PersonOutcome,
    pub changed: Vec<Address>,
}

impl PersonAnswer {
    /// An answer that changed nothing.
    #[must_use]
    pub fn unchanged(outcome: PersonOutcome) -> Self {
        Self {
            outcome,
            changed: Vec::new(),
        }
    }
}

/// The registry, as the routing reads it, and the person layer, as the routing asks it. A
/// directory that keeps no persons (the defaults) answers as a hub that links no character.
pub trait Directory {
    /// The world `slug`, if the registry has it.
    fn lookup(&self, slug: &str) -> Option<DirEntry>;

    /// The digest of the world's current key, if it has one. A link opened with a key whose
    /// digest is no longer this is closed (the key was rotated or revoked).
    fn key(&self, slug: &str) -> Option<String> {
        let _ = slug;
        None
    }

    /// A link code for `who`, replacing any it had, and how many seconds it lasts.
    ///
    /// # Errors
    /// The outcome to answer instead.
    fn person_code(&self, who: &Address) -> Result<(String, u32), PersonOutcome> {
        let _ = who;
        Err(PersonOutcome::Unavailable)
    }

    /// Unlinks `who`.
    fn person_unlink(&self, who: &Address) -> PersonAnswer {
        let _ = who;
        PersonAnswer::unchanged(PersonOutcome::NotLinked)
    }

    /// Sets the alts setting of `who`'s person.
    fn person_alts(&self, who: &Address, shown: bool) -> PersonAnswer {
        let _ = (who, shown);
        PersonAnswer::unchanged(PersonOutcome::NotLinked)
    }

    /// Lists `target`'s person as a person friend of `who`'s.
    fn person_befriend(&self, who: &Address, target: &Address) -> PersonAnswer {
        let _ = (who, target);
        PersonAnswer::unchanged(PersonOutcome::NotLinked)
    }

    /// Removes the person friend whose handle is `target`, where it holds for `who`.
    fn person_unfriend(&self, who: &Address, target: &Address) -> PersonAnswer {
        let _ = (who, target);
        PersonAnswer::unchanged(PersonOutcome::NotLinked)
    }

    /// Lists each of `targets`' persons as a person friend of `who`'s, until the limit.
    fn person_carry(&self, who: &Address, targets: &[Address]) -> PersonAnswer {
        let _ = (who, targets);
        PersonAnswer::unchanged(PersonOutcome::NotLinked)
    }

    /// `who`'s world was sent the friends handed back to it when it was unlinked: they are
    /// forgotten.
    fn returns_sent(&self, who: &Address) {
        let _ = who;
    }

    /// The characters of `target`'s person while it shows its alts; none otherwise.
    fn person_characters(&self, target: &Address) -> Vec<Address> {
        let _ = target;
        Vec::new()
    }

    /// `who`'s person view.
    fn person_view(&self, who: &Address) -> PersonView {
        let _ = who;
        PersonView::default()
    }
}

/// Something for the hub's I/O to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    /// Send a message on a world's link.
    Send(String, Message),
    /// Close a world's link, for the reason given (logged).
    Close(String, &'static str),
    /// A world's heartbeat, for the registry.
    Heartbeat {
        slug: String,
        state: String,
        players_online: Option<u32>,
    },
    /// The channels changed; save [`Hub::channels`].
    ChannelsChanged,
}

/// The rooms the hub carries.
pub const ROOMS: &[&str] = &[dereth_network_protocol::wire::GLOBAL_ROOM];

/// The most channels one character may own, and the most members one channel may have.
pub const MAX_CHANNELS_OWNED: usize = 10;
/// See [`MAX_CHANNELS_OWNED`].
pub const MAX_CHANNEL_MEMBERS: usize = 200;

/// The most channels the characters of one world may own together. A world names its own
/// characters, so the per-character limit alone would hold only for an honest world.
pub const MAX_CHANNELS_PER_WORLD: usize = 100;

/// The most channels one character may belong to.
pub const MAX_CHANNELS_JOINED: usize = 50;

/// The most invitations one channel may have outstanding.
pub const MAX_CHANNEL_INVITES: usize = 200;

/// How long an invitation stands before it lapses, in seconds of the hub's running time (an
/// invitation the hub saved lapses this long after it starts again).
pub const INVITE_SECS: f64 = 7.0 * 24.0 * 3600.0;

/// How long the hub remembers an id it has seen, so a resent message is not delivered twice, and
/// the answer it gave, so a resend is answered as the first was.
pub const DEDUP_SECS: f64 = 300.0;

/// How long the hub waits for a target world's answer to a tell before answering the sender
/// itself that the world is unavailable.
pub const ANSWER_SECS: f64 = dereth_network_protocol::ANSWER_SECS;

/// The most `person.watch`es the hub keeps for one world.
pub const MAX_PERSON_WATCHES: usize = 2_000;

/// The least time, in seconds, between two rounds of sending the watched characters' views again
/// after a person changed.
pub const PERSON_VIEWS_EVERY: f64 = 1.0;

/// The most watched views one round looks at again; the rest wait for the next round, in the
/// order their persons changed. Each is read from the registry while the routing holds it, so this
/// bounds how long a round holds it, however many views a change reaches.
pub const PERSON_VIEWS_PER_ROUND: usize = 100;

/// One channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub owner: Address,
    /// Members by identity key, with their address as written.
    pub members: BTreeMap<String, Address>,
    /// Invitations by the invitee's identity key.
    pub invites: BTreeSet<String>,
}

/// Every channel, by name. Saved by the hub's I/O whenever [`Out::ChannelsChanged`] is returned.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channels {
    pub channels: BTreeMap<String, Channel>,
}

#[derive(Debug)]
struct Link {
    name: String,
    /// `hello` has been received.
    greeted: bool,
    rooms: BTreeSet<String>,
    /// The digest of the key the link was opened with, when the I/O names one.
    key: Option<String>,
}

/// A tell forwarded to its target world, waiting for the answer.
#[derive(Debug, Clone)]
struct Waiting {
    origin: String,
    origin_id: String,
    target: String,
    until: f64,
}

/// A tell held for a federated world whose link is down.
#[derive(Debug, Clone)]
struct Held {
    origin: String,
    tell: Message,
    target: String,
    until: f64,
}

/// An id a world sent: when it was first seen, and the answer the hub gave it once it has. A world
/// whose link went before the answer reached it sends the request again; the resend is answered
/// with this rather than applied twice.
#[derive(Debug, Clone)]
struct Seen {
    at: f64,
    answer: Option<Message>,
}

#[derive(Debug, Default)]
struct Pair {
    /// Send times, from the lower key to the higher and back.
    forth: VecDeque<f64>,
    back: VecDeque<f64>,
    blocked_until: f64,
}

/// The hub's routing state.
#[derive(Debug)]
pub struct Hub {
    pub limits: Limits,
    software: String,
    links: BTreeMap<String, Link>,
    waiting: HashMap<String, Waiting>,
    held: Vec<Held>,
    seen: HashMap<(String, String), Seen>,
    tell_buckets: HashMap<String, Bucket>,
    room_buckets: HashMap<String, Bucket>,
    world_buckets: HashMap<String, Bucket>,
    /// Room lines by sending world, all its characters together.
    world_room_buckets: HashMap<String, Bucket>,
    /// Tells by sending world and recipient, all the world's characters together.
    recipient_buckets: HashMap<(String, String), Bucket>,
    /// Tells by sending world and target world (see [`Limits::tells_per_world_pair`]).
    pair_world_buckets: HashMap<(String, String), Bucket>,
    /// Watches by sending world and target world (see [`Limits::watches_per_world_pair`]).
    watch_pair_buckets: HashMap<(String, String), Bucket>,
    pairs: HashMap<(String, String), Pair>,
    next_id: u64,
    /// Every channel.
    pub channels: Channels,
    /// The channels changed since [`Hub::take_changed_channels`] was last called.
    changed_channels: BTreeSet<String>,
    /// When each outstanding invitation was made (channel, invitee key), in this run's time.
    invited_at: HashMap<(String, String), f64>,
    next_invite_sweep: f64,
    /// The page a link code is entered on; none for a hub that links no character.
    link_url: Option<String>,
    /// The page a person shows or hides its characters on, named when a world asks to show them.
    person_url: Option<String>,
    /// Person requests by character.
    person_buckets: HashMap<String, Bucket>,
    /// Each world's `person.watch`es, by world and the character's identity key: the character,
    /// and the view it was last sent.
    person_watches: BTreeMap<(String, String), (Address, PersonView)>,
    /// The watches by the identity key of every character each names: the watched character, and
    /// each handle and character of the view it was last sent.
    watches_naming: HashMap<String, BTreeSet<(String, String)>>,
    /// The watches to look at again, in the order their persons changed, each once.
    views_due: VecDeque<(String, String)>,
    /// See [`Hub::views_due`].
    views_due_set: HashSet<(String, String)>,
    /// When the next round of those may run.
    next_person_views: f64,
}

/// The identity keys a watch of `who` whose view is `view` names: `who`'s, and each handle's and
/// character's in the view.
fn named_by(who: &Address, view: &PersonView) -> BTreeSet<String> {
    std::iter::once(who.key())
        .chain(
            view.persons
                .iter()
                .flat_map(|p| std::iter::once(&p.handle).chain(&p.characters))
                .map(Address::key),
        )
        .collect()
}

/// Files the watch `key` under every character it names.
fn index_watch(
    index: &mut HashMap<String, BTreeSet<(String, String)>>,
    key: &(String, String),
    who: &Address,
    view: &PersonView,
) {
    for name in named_by(who, view) {
        index.entry(name).or_default().insert(key.clone());
    }
}

/// Takes the watch `key` out from under every character it named.
fn unindex_watch(
    index: &mut HashMap<String, BTreeSet<(String, String)>>,
    key: &(String, String),
    who: &Address,
    view: &PersonView,
) {
    for name in named_by(who, view) {
        if let Some(watches) = index.get_mut(&name) {
            watches.remove(key);
            if watches.is_empty() {
                index.remove(&name);
            }
        }
    }
}

impl Hub {
    /// A hub with no links, the given channels and limits. `software` is its name and version, as
    /// `welcome` reports it.
    #[must_use]
    pub fn new(software: &str, channels: Channels, limits: Limits) -> Self {
        Self {
            limits,
            software: software.to_owned(),
            links: BTreeMap::new(),
            waiting: HashMap::new(),
            held: Vec::new(),
            seen: HashMap::new(),
            tell_buckets: HashMap::new(),
            room_buckets: HashMap::new(),
            world_buckets: HashMap::new(),
            world_room_buckets: HashMap::new(),
            recipient_buckets: HashMap::new(),
            pair_world_buckets: HashMap::new(),
            watch_pair_buckets: HashMap::new(),
            pairs: HashMap::new(),
            next_id: 1,
            channels,
            changed_channels: BTreeSet::new(),
            invited_at: HashMap::new(),
            next_invite_sweep: 0.0,
            link_url: None,
            person_url: None,
            person_buckets: HashMap::new(),
            person_watches: BTreeMap::new(),
            watches_naming: HashMap::new(),
            views_due: VecDeque::new(),
            views_due_set: HashSet::new(),
            next_person_views: 0.0,
        }
    }

    /// Sets the page a link code is entered on (the site's `/characters/`); a hub without one links
    /// no character, and answers a code request `unavailable`.
    pub fn set_link_url(&mut self, url: Option<String>) {
        self.link_url = url.filter(|u| dereth_network_protocol::person::is_page_url(u));
    }

    /// Sets the page a person shows or hides its characters on (the site's `/characters/`), which
    /// the answer to a world asking to show them names.
    pub fn set_person_url(&mut self, url: Option<String>) {
        self.person_url = url.filter(|u| dereth_network_protocol::person::is_page_url(u));
    }

    /// The persons of `characters` changed (a character linked or unlinked, a setting, a person
    /// friend), and `characters` are all of theirs, before and after the change: the watched views
    /// of those characters, and those that name one of them, are looked at again from the next
    /// tick, and sent where they changed. No other view can have changed, so no other is read.
    pub fn persons_changed(&mut self, characters: &[Address]) {
        for c in characters {
            let Some(watches) = self.watches_naming.get(&c.key()) else {
                continue;
            };
            for w in watches {
                if self.views_due_set.insert(w.clone()) {
                    self.views_due.push_back(w.clone());
                }
            }
        }
    }

    /// How many watched views wait to be looked at again.
    #[must_use]
    pub fn views_due(&self) -> usize {
        self.views_due.len()
    }

    /// Keeps the watch `key` of `who`, with the view it was sent, in place of any it had.
    fn keep_watch(&mut self, key: (String, String), who: Address, view: PersonView) {
        self.drop_watch(&key);
        index_watch(&mut self.watches_naming, &key, &who, &view);
        self.person_watches.insert(key, (who, view));
    }

    /// Forgets the watch `key`.
    fn drop_watch(&mut self, key: &(String, String)) {
        if let Some((who, view)) = self.person_watches.remove(key) {
            unindex_watch(&mut self.watches_naming, key, &who, &view);
        }
    }

    /// How many `person.watch`es the hub keeps for `slug`.
    #[must_use]
    pub fn person_watches(&self, slug: &str) -> usize {
        self.watches_of(slug).count()
    }

    /// The keys of `slug`'s `person.watch`es.
    fn watches_of<'a>(&'a self, slug: &'a str) -> impl Iterator<Item = &'a (String, String)> + 'a {
        self.person_watches
            .range((slug.to_owned(), String::new())..)
            .map(|(k, _)| k)
            .take_while(move |(w, _)| w == slug)
    }

    /// The names of the channels changed (made, changed or ended) since the last call, for the
    /// I/O to save.
    pub fn take_changed_channels(&mut self) -> BTreeSet<String> {
        std::mem::take(&mut self.changed_channels)
    }

    /// The worlds with a link up, by slug.
    pub fn linked(&self) -> impl Iterator<Item = &str> {
        self.links.keys().map(String::as_str)
    }

    /// How many tells the hub holds for worlds whose link is down.
    #[must_use]
    pub fn held_count(&self) -> usize {
        self.held.len()
    }

    /// A world's link is up (authenticated, its federation flag on). A second link from the same
    /// world replaces the first, which is closed.
    pub fn connect(&mut self, slug: &str, name: &str) -> Vec<Out> {
        let out = self.link_up(slug, name, None);
        self.remember(&out);
        out
    }

    /// [`Hub::connect`] for a link opened with the key whose digest is `key`: it stays up only
    /// while that is the world's key ([`Directory::key`]).
    pub fn connect_keyed(&mut self, slug: &str, name: &str, key: &str) -> Vec<Out> {
        let out = self.link_up(slug, name, Some(key.to_owned()));
        self.remember(&out);
        out
    }

    fn link_up(&mut self, slug: &str, name: &str, key: Option<String>) -> Vec<Out> {
        let mut out = Vec::new();
        if self.links.contains_key(slug) {
            out.push(Out::Close(slug.to_owned(), "replaced by a newer link"));
            out.extend(self.link_down(slug));
        }
        self.links.insert(
            slug.to_owned(),
            Link {
                name: name.to_owned(),
                greeted: false,
                rooms: BTreeSet::new(),
                key,
            },
        );
        out
    }

    /// A world's link went down, or the I/O dropped it: the other worlds are told it is offline,
    /// and the tells it had not answered are answered for it.
    pub fn disconnect(&mut self, slug: &str) -> Vec<Out> {
        let out = self.link_down(slug);
        self.remember(&out);
        out
    }

    fn link_down(&mut self, slug: &str) -> Vec<Out> {
        let Some(link) = self.links.remove(slug) else {
            return Vec::new();
        };
        // Its `person.watch`es go with it; it sends them again on its next link.
        let watches: Vec<(String, String)> = self.watches_of(slug).cloned().collect();
        for w in &watches {
            self.drop_watch(w);
        }
        let mut out = Vec::new();
        if link.greeted {
            out.extend(self.broadcast_except(
                slug,
                &Message::World(WorldInfo {
                    slug: slug.to_owned(),
                    name: link.name.clone(),
                    online: false,
                }),
            ));
        }
        // Tells it had not answered are answered for it: it may or may not have shown them, and
        // sending them again could show one twice.
        let mine: Vec<String> = self
            .waiting
            .iter()
            .filter(|(_, w)| w.target == slug)
            .map(|(k, _)| k.clone())
            .collect();
        for k in mine {
            if let Some(w) = self.waiting.remove(&k) {
                out.push(Out::Send(
                    w.origin,
                    Message::TellResult {
                        id: w.origin_id,
                        outcome: Outcome::Unavailable,
                        message: None,
                    },
                ));
            }
        }
        out
    }

    /// `welcome` for `slug`, naming as many of `worlds` as fit in one frame; the rest follow as
    /// `world` messages, so no frame the hub sends is ever over the limit, however many worlds
    /// are linked or however long their names.
    fn welcome(&self, slug: &str, worlds: Vec<WorldInfo>) -> Vec<Out> {
        let mut welcome = Message::Welcome {
            proto: PROTOCOL,
            world: slug.to_owned(),
            hub: self.software.clone(),
            worlds: Vec::new(),
            rooms: ROOMS.iter().map(|r| (*r).to_owned()).collect(),
            limits: self.limits,
        };
        // A margin for the commas between entries and anything else that grows.
        let mut size = welcome.to_json().len() + 64;
        let mut rest = Vec::new();
        if let Message::Welcome { worlds: listed, .. } = &mut welcome {
            for info in worlds {
                let entry = serde_json::to_string(&info).map_or(MAX_MESSAGE_BYTES, |e| e.len() + 1);
                if rest.is_empty() && size + entry <= MAX_MESSAGE_BYTES {
                    size += entry;
                    listed.push(info);
                } else {
                    rest.push(info);
                }
            }
        }
        let mut out = vec![Out::Send(slug.to_owned(), welcome)];
        out.extend(
            rest.into_iter()
                .map(|info| Out::Send(slug.to_owned(), Message::World(info))),
        );
        out
    }

    fn broadcast_except(&self, except: &str, m: &Message) -> Vec<Out> {
        self.links
            .iter()
            .filter(|(s, l)| s.as_str() != except && l.greeted)
            .map(|(s, _)| Out::Send(s.clone(), m.clone()))
            .collect()
    }

    fn fresh_id(&mut self) -> String {
        let id = format!("h{}", self.next_id);
        self.next_id += 1;
        id
    }

    fn error(slug: &str, code: ErrorCode, message: &str) -> Out {
        Out::Send(
            slug.to_owned(),
            Message::Error {
                code: code.word().to_owned(),
                message: message.to_owned(),
            },
        )
    }

    /// A world's address may be used by server features: in the registry, flag on.
    fn federated(dir: &dyn Directory, slug: &str) -> bool {
        dir.lookup(slug).is_some_and(|e| e.federation.links())
    }

    /// A message from `slug`'s link.
    pub fn receive(&mut self, dir: &dyn Directory, slug: &str, m: Message, now: f64) -> Vec<Out> {
        let out = self.take(dir, slug, m, now);
        self.remember(&out);
        out
    }

    /// Keeps each answer among `outs` with the request it answers, while the hub remembers that
    /// request's id: a resend is answered with what the first was given. A channel's member list or
    /// a character's channel list is not kept (a resend of either is asked again: it changes
    /// nothing).
    fn remember(&mut self, outs: &[Out]) {
        for o in outs {
            let Out::Send(slug, m) = o else {
                continue;
            };
            let id = match m {
                Message::TellResult { id, .. }
                | Message::RoomResult { id, .. }
                | Message::PersonResult { id, .. } => id,
                Message::ChannelResult {
                    id,
                    members,
                    channels,
                    ..
                } if members.is_empty() && channels.is_empty() => id,
                _ => continue,
            };
            if let Some(seen) = self.seen.get_mut(&(slug.clone(), id.clone())) {
                seen.answer = Some(m.clone());
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn take(&mut self, dir: &dyn Directory, slug: &str, m: Message, now: f64) -> Vec<Out> {
        let mut out = Vec::new();
        let Some(link) = self.links.get(slug) else {
            return out;
        };
        if !link.greeted {
            let Message::Hello { proto, .. } = m else {
                // A link closed before it was greeted was never announced: it is simply gone.
                self.links.remove(slug);
                out.push(Self::error(slug, ErrorCode::Order, "hello first"));
                out.push(Out::Close(slug.to_owned(), "spoke before hello"));
                return out;
            };
            if proto != PROTOCOL {
                self.links.remove(slug);
                out.push(Self::error(
                    slug,
                    ErrorCode::Protocol,
                    &format!("this hub speaks version {PROTOCOL}"),
                ));
                out.push(Out::Close(slug.to_owned(), "protocol version"));
                return out;
            }
            let name = link.name.clone();
            if let Some(l) = self.links.get_mut(slug) {
                l.greeted = true;
            }
            let worlds: Vec<WorldInfo> = self
                .links
                .iter()
                .filter(|(_, l)| l.greeted)
                .map(|(s, l)| WorldInfo {
                    slug: s.clone(),
                    name: l.name.clone(),
                    online: true,
                })
                .collect();
            out.extend(self.welcome(slug, worlds));
            out.extend(self.broadcast_except(
                slug,
                &Message::World(WorldInfo {
                    slug: slug.to_owned(),
                    name,
                    online: true,
                }),
            ));
            // Tells held for this world go now.
            let (ready, keep): (Vec<Held>, Vec<Held>) = std::mem::take(&mut self.held)
                .into_iter()
                .partition(|h| h.target == slug);
            self.held = keep;
            for h in ready {
                out.extend(self.forward_tell(h.origin, h.tell, now));
            }
            return out;
        }

        let world_rate = self.limits.messages_per_world;
        if !self
            .world_buckets
            .entry(slug.to_owned())
            .or_insert_with(|| Bucket::new(world_rate, now))
            .take(now)
        {
            // Over the world's own limit: what can be answered is answered as rate-limited, the
            // rest is dropped.
            match m {
                Message::Tell { id, .. } => out.push(Out::Send(
                    slug.to_owned(),
                    Message::TellResult {
                        id,
                        outcome: Outcome::RateLimited,
                        message: None,
                    },
                )),
                Message::RoomSay { id, .. } => out.push(Out::Send(
                    slug.to_owned(),
                    Message::RoomResult {
                        id,
                        outcome: Outcome::RateLimited,
                    },
                )),
                Message::Person { id, .. } => out.push(Out::Send(
                    slug.to_owned(),
                    person_result(id, PersonOutcome::RateLimited),
                )),
                _ => {}
            }
            return out;
        }

        match m {
            Message::Hello { .. } => {
                out.push(Self::error(slug, ErrorCode::Order, "hello twice"));
            }
            Message::Status {
                state,
                players_online,
            } => out.push(Out::Heartbeat {
                slug: slug.to_owned(),
                state,
                players_online,
            }),
            Message::Watch { owner, target, on } => {
                if owner.world() != slug {
                    out.push(Self::error(slug, ErrorCode::Identity, "watch owner"));
                } else if self.is_greeted(target.world()) && Self::federated(dir, target.world()) {
                    // A watch is owed a `presence` answer that counts against the target's own
                    // pace, so one world may send another only a share of that pace in watches;
                    // one over it goes nowhere.
                    let rate = self.limits.watches_per_world_pair();
                    let within = !on
                        || self
                            .watch_pair_buckets
                            .entry((slug.to_owned(), target.world().to_owned()))
                            .or_insert_with(|| Bucket::new(rate, now))
                            .take(now);
                    if within {
                        out.push(Out::Send(
                            target.world().to_owned(),
                            Message::Watch { owner, target, on },
                        ));
                    }
                }
            }
            Message::Presence { to, of, online } => {
                if of.world() != slug {
                    out.push(Self::error(slug, ErrorCode::Identity, "presence of"));
                } else if self.is_greeted(to.world()) && Self::federated(dir, to.world()) {
                    out.push(Out::Send(
                        to.world().to_owned(),
                        Message::Presence { to, of, online },
                    ));
                }
            }
            Message::Tell {
                id,
                from,
                to,
                text,
                friend,
            } => {
                if from.world() != slug {
                    out.push(Self::error(slug, ErrorCode::Identity, "tell from"));
                    return out;
                }
                if let Some(again) = self.resent(slug, &id, now) {
                    return again;
                }
                let result = |outcome| {
                    Out::Send(
                        slug.to_owned(),
                        Message::TellResult {
                            id: id.clone(),
                            outcome,
                            message: None,
                        },
                    )
                };
                let tell_rate = self.limits.tells_per_character;
                if !self
                    .tell_buckets
                    .entry(from.key())
                    .or_insert_with(|| Bucket::new(tell_rate, now))
                    .take(now)
                {
                    out.push(result(Outcome::RateLimited));
                    return out;
                }
                if to.is_channel() {
                    out.extend(self.channel_tell(slug, &id, &from, &to, &text, now));
                    return out;
                }
                let target = to.world().to_owned();
                if !Self::federated(dir, &target) {
                    out.push(result(Outcome::NotOnNetwork));
                    return out;
                }
                // Every tell forwarded is owed an answer that counts against the target's own
                // pace, so one world may send another only a share of that pace.
                let pair_world_rate = self.limits.tells_per_world_pair();
                if !self
                    .pair_world_buckets
                    .entry((slug.to_owned(), target.clone()))
                    .or_insert_with(|| Bucket::new(pair_world_rate, now))
                    .take(now)
                {
                    out.push(result(Outcome::RateLimited));
                    return out;
                }
                let recipient_rate = self.limits.tells_per_recipient;
                if !self
                    .recipient_buckets
                    .entry((slug.to_owned(), to.key()))
                    .or_insert_with(|| Bucket::new(recipient_rate, now))
                    .take(now)
                {
                    out.push(result(Outcome::RateLimited));
                    return out;
                }
                if !self.pair_allows(&from, &to, now) {
                    out.push(result(Outcome::RateLimited));
                    return out;
                }
                let tell = Message::Tell {
                    id: id.clone(),
                    from,
                    to,
                    text,
                    friend,
                };
                if self.is_greeted(&target) {
                    out.extend(self.forward_tell(slug.to_owned(), tell, now));
                } else {
                    self.held.push(Held {
                        origin: slug.to_owned(),
                        tell,
                        target,
                        until: now + self.limits.hold_secs,
                    });
                }
            }
            Message::TellResult {
                id,
                outcome,
                message,
            } => {
                if let Some(w) = self.waiting.get(&id) {
                    if w.target == slug {
                        let w = self.waiting.remove(&id).expect("present");
                        out.push(Out::Send(
                            w.origin,
                            Message::TellResult {
                                id: w.origin_id,
                                outcome,
                                message,
                            },
                        ));
                    }
                }
            }
            Message::RoomJoin { room, on } => {
                if ROOMS.contains(&room.as_str()) {
                    if let Some(l) = self.links.get_mut(slug) {
                        if on {
                            l.rooms.insert(room);
                        } else {
                            l.rooms.remove(&room);
                        }
                    }
                }
            }
            Message::RoomSay {
                id,
                room,
                from,
                text,
            } => {
                if from.world() != slug {
                    out.push(Self::error(slug, ErrorCode::Identity, "room.say from"));
                    return out;
                }
                if let Some(again) = self.resent(slug, &id, now) {
                    return again;
                }
                let carries = self
                    .links
                    .get(slug)
                    .is_some_and(|l| l.rooms.contains(&room));
                let room_rate = self.limits.room_lines_per_character;
                let world_room_rate = self.limits.room_lines_per_world;
                let outcome = if !carries {
                    Outcome::NotMember
                } else if !self
                    .room_buckets
                    .entry(from.key())
                    .or_insert_with(|| Bucket::new(room_rate, now))
                    .take(now)
                    || !self
                        .world_room_buckets
                        .entry(slug.to_owned())
                        .or_insert_with(|| Bucket::new(world_room_rate, now))
                        .take(now)
                {
                    Outcome::RateLimited
                } else {
                    let line = Message::RoomLine {
                        room: room.clone(),
                        from,
                        text,
                    };
                    for (s, l) in &self.links {
                        if s != slug
                            && l.greeted
                            && l.rooms.contains(&room)
                            && Self::federated(dir, s)
                        {
                            out.push(Out::Send(s.clone(), line.clone()));
                        }
                    }
                    Outcome::Ok
                };
                out.push(Out::Send(
                    slug.to_owned(),
                    Message::RoomResult { id, outcome },
                ));
            }
            Message::Channel {
                id,
                op,
                channel,
                by,
                who,
            } => {
                if by.world() != slug {
                    out.push(Self::error(slug, ErrorCode::Identity, "channel by"));
                    return out;
                }
                // A list is asked again: it changes nothing, and its answer is not kept.
                if !matches!(op, ChannelOp::List | ChannelOp::Members) {
                    if let Some(again) = self.resent(slug, &id, now) {
                        return again;
                    }
                }
                out.extend(self.channel_request(dir, slug, id, op, &channel, &by, who, now));
            }
            Message::Person {
                id,
                op,
                who,
                target,
                targets,
                shown,
            } => {
                if who.world() != slug {
                    out.push(Self::error(slug, ErrorCode::Identity, "person who"));
                    return out;
                }
                if let Some(again) = self.resent(slug, &id, now) {
                    return again;
                }
                let answer = self.person_request(
                    dir,
                    &who,
                    Asked {
                        op,
                        target: target.as_ref(),
                        targets: &targets,
                        shown,
                    },
                    now,
                );
                out.push(Out::Send(slug.to_owned(), with_id(answer, id)));
            }
            Message::PersonWatch { who, on } => {
                if who.world() != slug {
                    out.push(Self::error(slug, ErrorCode::Identity, "person.watch who"));
                    return out;
                }
                let key = (slug.to_owned(), who.key());
                if !on {
                    self.drop_watch(&key);
                } else if self.person_watches.contains_key(&key)
                    || self.person_watches(slug) < MAX_PERSON_WATCHES
                {
                    let view = dir.person_view(&who);
                    out.push(Out::Send(slug.to_owned(), view_message(&who, &view)));
                    if !view.linked && !view.persons.is_empty() {
                        dir.returns_sent(&who);
                    }
                    self.keep_watch(key, who, view);
                }
            }
            // Only the hub sends these.
            Message::Welcome { .. }
            | Message::World(_)
            | Message::RoomLine { .. }
            | Message::RoomResult { .. }
            | Message::ChannelResult { .. }
            | Message::ChannelInvite { .. }
            | Message::ChannelLine { .. }
            | Message::PersonResult { .. }
            | Message::PersonFriends { .. } => {
                out.push(Self::error(
                    slug,
                    ErrorCode::Malformed,
                    "a message only the hub sends",
                ));
            }
            Message::Error { .. } => {}
        }
        out
    }

    fn is_greeted(&self, slug: &str) -> bool {
        self.links.get(slug).is_some_and(|l| l.greeted)
    }

    /// The answer to a person request from `who`'s world (its id is put on after), within the
    /// character's pace, which an `unfriend` is not held to: it only takes away, and one held back
    /// would leave a person friend the world has already let go. A request that changes a person
    /// has the watched views it may have changed looked at again.
    fn person_request(
        &mut self,
        dir: &dyn Directory,
        who: &Address,
        asked: Asked<'_>,
        now: f64,
    ) -> Message {
        let Asked {
            op,
            target,
            targets,
            shown,
        } = asked;
        let rate = self.limits.tells_per_character;
        if op != PersonOp::Unfriend
            && !self
                .person_buckets
                .entry(who.key())
                .or_insert_with(|| Bucket::new(rate, now))
                .take(now)
        {
            return person_result(String::new(), PersonOutcome::RateLimited);
        }
        let answer = match (op, target) {
            (PersonOp::Code, _) => {
                let Some(url) = self.link_url.clone() else {
                    return person_result(String::new(), PersonOutcome::Unavailable);
                };
                return match dir.person_code(who) {
                    Ok((code, secs)) => Message::PersonResult {
                        id: String::new(),
                        outcome: PersonOutcome::Ok,
                        code: Some(code),
                        expires_secs: Some(secs),
                        url: Some(url),
                        characters: Vec::new(),
                    },
                    Err(outcome) => person_result(String::new(), outcome),
                };
            }
            (PersonOp::Characters, Some(target)) => {
                let mut characters = dir.person_characters(target);
                characters.truncate(MAX_PERSON_CHARACTERS);
                return Message::PersonResult {
                    id: String::new(),
                    outcome: PersonOutcome::Ok,
                    code: None,
                    expires_secs: None,
                    url: None,
                    characters,
                };
            }
            (PersonOp::Unlink, _) => dir.person_unlink(who),
            // A world may hide a person's characters, never show them: the answer to showing them
            // names the page the person does it on.
            (PersonOp::Alts, _) => match dir.person_alts(who, shown.unwrap_or(false)) {
                PersonAnswer {
                    outcome: PersonOutcome::Refused,
                    ..
                } => {
                    return Message::PersonResult {
                        id: String::new(),
                        outcome: PersonOutcome::Refused,
                        code: None,
                        expires_secs: None,
                        url: self.person_url.clone(),
                        characters: Vec::new(),
                    }
                }
                answer => answer,
            },
            (PersonOp::Befriend, Some(target)) => dir.person_befriend(who, target),
            (PersonOp::Carry, _) => dir.person_carry(who, targets),
            (PersonOp::Unfriend, Some(target)) => dir.person_unfriend(who, target),
            // The wire refuses a request without the target its op needs.
            (PersonOp::Befriend | PersonOp::Unfriend | PersonOp::Characters, None) => {
                PersonAnswer::unchanged(PersonOutcome::Refused)
            }
        };
        // One that changed nothing (hiding hidden characters, removing a person friend not listed)
        // has no view looked at again.
        self.persons_changed(&answer.changed);
        person_result(String::new(), answer.outcome)
    }

    /// The watched views due, at most [`PERSON_VIEWS_PER_ROUND`] of them read, each sent again where
    /// it changed.
    fn send_person_views(&mut self, dir: &dyn Directory) -> Vec<Out> {
        let mut out = Vec::new();
        let mut read = 0;
        while read < PERSON_VIEWS_PER_ROUND {
            let Some(key) = self.views_due.pop_front() else {
                break;
            };
            self.views_due_set.remove(&key);
            // A watch turned off, or gone with its link, since its person changed: nothing is read,
            // so it counts nothing of the round.
            let Some(who) = self.person_watches.get(&key).map(|(who, _)| who.clone()) else {
                continue;
            };
            read += 1;
            let view = dir.person_view(&who);
            if self
                .person_watches
                .get(&key)
                .is_some_and(|(_, last)| *last != view)
            {
                out.push(Out::Send(key.0.clone(), view_message(&who, &view)));
                if !view.linked && !view.persons.is_empty() {
                    dir.returns_sent(&who);
                }
                self.keep_watch(key, who, view);
            }
        }
        out
    }

    /// What to send for a resend of `(world, id)` seen within [`DEDUP_SECS`]: the answer the first
    /// was given, or nothing while it waits for one. `None` for an id not seen, which is recorded.
    fn resent(&mut self, slug: &str, id: &str, now: f64) -> Option<Vec<Out>> {
        let key = (slug.to_owned(), id.to_owned());
        if let Some(seen) = self.seen.get(&key).filter(|s| now - s.at < DEDUP_SECS) {
            return Some(
                seen.answer
                    .iter()
                    .map(|a| Out::Send(slug.to_owned(), a.clone()))
                    .collect(),
            );
        }
        self.seen.insert(
            key,
            Seen {
                at: now,
                answer: None,
            },
        );
        None
    }

    /// The loop guard: records a tell from `from` to `to` and says whether it may go.
    fn pair_allows(&mut self, from: &Address, to: &Address, now: f64) -> bool {
        let (a, b) = (from.key(), to.key());
        let forward = a <= b;
        let key = if forward { (a, b) } else { (b, a) };
        let limits = self.limits;
        let pair = self.pairs.entry(key).or_default();
        if now < pair.blocked_until {
            return false;
        }
        let window = limits.pair_window_secs;
        for q in [&mut pair.forth, &mut pair.back] {
            while q.front().is_some_and(|&t| now - t > window) {
                q.pop_front();
            }
        }
        if forward {
            pair.forth.push_back(now);
        } else {
            pair.back.push_back(now);
        }
        let burst = usize::try_from(limits.pair_burst).unwrap_or(usize::MAX);
        if pair.forth.len() > burst && pair.back.len() > burst {
            pair.blocked_until = now + limits.pair_block_secs;
            pair.forth.clear();
            pair.back.clear();
            return false;
        }
        true
    }

    fn forward_tell(&mut self, origin: String, tell: Message, now: f64) -> Vec<Out> {
        let Message::Tell {
            id,
            from,
            to,
            text,
            friend,
        } = tell
        else {
            return Vec::new();
        };
        let target = to.world().to_owned();
        let hub_id = self.fresh_id();
        self.waiting.insert(
            hub_id.clone(),
            Waiting {
                origin,
                origin_id: id,
                target: target.clone(),
                until: now + ANSWER_SECS,
            },
        );
        vec![Out::Send(
            target,
            Message::Tell {
                id: hub_id,
                from,
                to,
                text,
                friend,
            },
        )]
    }

    /// The passage of time: held tells and unanswered tells whose time is up are answered
    /// `unavailable`; old ids and full buckets are forgotten; a linked world whose federation flag
    /// is no longer on is closed.
    pub fn tick(&mut self, dir: &dyn Directory, now: f64) -> Vec<Out> {
        let out = self.time_passes(dir, now);
        self.remember(&out);
        out
    }

    fn time_passes(&mut self, dir: &dyn Directory, now: f64) -> Vec<Out> {
        let mut out = Vec::new();
        let (expired, keep): (Vec<Held>, Vec<Held>) = std::mem::take(&mut self.held)
            .into_iter()
            .partition(|h| now >= h.until);
        self.held = keep;
        for h in expired {
            if let Message::Tell { id, .. } = h.tell {
                out.push(Out::Send(
                    h.origin,
                    Message::TellResult {
                        id,
                        outcome: Outcome::Unavailable,
                        message: None,
                    },
                ));
            }
        }
        let late: Vec<String> = self
            .waiting
            .iter()
            .filter(|(_, w)| now >= w.until)
            .map(|(k, _)| k.clone())
            .collect();
        for k in late {
            if let Some(w) = self.waiting.remove(&k) {
                out.push(Out::Send(
                    w.origin,
                    Message::TellResult {
                        id: w.origin_id,
                        outcome: Outcome::Unavailable,
                        message: None,
                    },
                ));
            }
        }
        self.seen.retain(|_, s| now - s.at < DEDUP_SECS);
        self.tell_buckets.retain(|_, b| !b.full_at(now));
        self.room_buckets.retain(|_, b| !b.full_at(now));
        self.world_buckets.retain(|_, b| !b.full_at(now));
        self.world_room_buckets.retain(|_, b| !b.full_at(now));
        self.recipient_buckets.retain(|_, b| !b.full_at(now));
        self.pair_world_buckets.retain(|_, b| !b.full_at(now));
        self.watch_pair_buckets.retain(|_, b| !b.full_at(now));
        self.person_buckets.retain(|_, b| !b.full_at(now));
        if !self.views_due.is_empty() && now >= self.next_person_views {
            self.next_person_views = now + PERSON_VIEWS_EVERY;
            out.extend(self.send_person_views(dir));
        }
        if now >= self.next_invite_sweep {
            self.next_invite_sweep = now + 60.0;
            self.lapse_invitations(now);
        }
        let window = self.limits.pair_window_secs;
        self.pairs.retain(|_, p| {
            now < p.blocked_until
                || p.forth.back().is_some_and(|&t| now - t <= window)
                || p.back.back().is_some_and(|&t| now - t <= window)
        });
        let off: Vec<String> = self
            .links
            .keys()
            .filter(|s| !Self::federated(dir, s))
            .cloned()
            .collect();
        for s in off {
            out.push(Out::Close(s.clone(), "federation no longer on"));
            out.extend(self.link_down(&s));
        }
        let rekeyed: Vec<String> = self
            .links
            .iter()
            .filter(|(s, l)| {
                l.key
                    .as_ref()
                    .is_some_and(|k| dir.key(s).as_ref() != Some(k))
            })
            .map(|(s, _)| s.clone())
            .collect();
        for s in rekeyed {
            out.push(Out::Close(s.clone(), "its key was rotated or revoked"));
            out.extend(self.link_down(&s));
        }
        out
    }

    /// Invitations older than [`INVITE_SECS`] lapse; one the hub has no time for (saved before it
    /// started) is timed from now.
    fn lapse_invitations(&mut self, now: f64) {
        let mut lapsed = Vec::new();
        for (name, channel) in &self.channels.channels {
            for key in &channel.invites {
                let at = *self
                    .invited_at
                    .entry((name.clone(), key.clone()))
                    .or_insert(now);
                if now - at >= INVITE_SECS {
                    lapsed.push((name.clone(), key.clone()));
                }
            }
        }
        for (name, key) in lapsed {
            if let Some(c) = self.channels.channels.get_mut(&name) {
                c.invites.remove(&key);
                self.changed_channels.insert(name.clone());
            }
            self.invited_at.remove(&(name, key));
        }
        let channels = &self.channels.channels;
        self.invited_at
            .retain(|(name, key), _| channels.get(name).is_some_and(|c| c.invites.contains(key)));
    }

    /// Whether a line from world `slug` to `to` is within the paces a tell to `to` is held to: the
    /// world pair's share and the per-recipient limit. Takes from both only when both allow it.
    fn within_tell_paces(&mut self, slug: &str, to: &Address, now: f64) -> bool {
        let (pair_rate, recipient_rate) = (
            self.limits.tells_per_world_pair(),
            self.limits.tells_per_recipient,
        );
        let pair = self
            .pair_world_buckets
            .entry((slug.to_owned(), to.world().to_owned()))
            .or_insert_with(|| Bucket::new(pair_rate, now));
        if !pair.has(now) {
            return false;
        }
        let recipient = self
            .recipient_buckets
            .entry((slug.to_owned(), to.key()))
            .or_insert_with(|| Bucket::new(recipient_rate, now));
        if !recipient.take(now) {
            return false;
        }
        self.pair_world_buckets
            .get_mut(&(slug.to_owned(), to.world().to_owned()))
            .is_some_and(|b| b.take(now))
    }

    /// A tell to a channel: a line to every other member whose world is linked, each held to the
    /// paces a tell to that member is. It is `delivered` unless every line was held back.
    fn channel_tell(
        &mut self,
        slug: &str,
        id: &str,
        from: &Address,
        to: &Address,
        text: &str,
        now: f64,
    ) -> Vec<Out> {
        let mut out = Vec::new();
        let outcome = match self.channels.channels.get(to.name()) {
            Some(channel) if channel.members.contains_key(&from.key()) => {
                let members: Vec<Address> = channel
                    .members
                    .values()
                    .filter(|m| !m.same(from) && self.is_greeted(m.world()))
                    .cloned()
                    .collect();
                let mut held = 0;
                for member in &members {
                    if !self.within_tell_paces(slug, member, now) {
                        held += 1;
                        continue;
                    }
                    out.push(Out::Send(
                        member.world().to_owned(),
                        Message::ChannelLine {
                            channel: to.name().to_owned(),
                            from: from.clone(),
                            to: member.clone(),
                            text: text.to_owned(),
                        },
                    ));
                }
                if held > 0 && held == members.len() {
                    Outcome::RateLimited
                } else {
                    Outcome::Delivered
                }
            }
            _ => Outcome::NotMember,
        };
        out.push(Out::Send(
            slug.to_owned(),
            Message::TellResult {
                id: id.to_owned(),
                outcome,
                message: None,
            },
        ));
        out
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn channel_request(
        &mut self,
        dir: &dyn Directory,
        slug: &str,
        id: String,
        op: ChannelOp,
        name: &str,
        by: &Address,
        who: Option<Address>,
        now: f64,
    ) -> Vec<Out> {
        let name = name.to_ascii_lowercase();
        let recipient_rate = self.limits.tells_per_recipient;
        let mut out = Vec::new();
        let mut members = Vec::new();
        let mut channels = Vec::new();
        let mut changed = false;
        let outcome = match op {
            ChannelOp::Create => {
                let owned = self
                    .channels
                    .channels
                    .values()
                    .filter(|c| c.owner.same(by))
                    .count();
                let world_owns = self
                    .channels
                    .channels
                    .values()
                    .filter(|c| c.owner.world() == slug)
                    .count();
                if self.channels.channels.contains_key(&name) {
                    ChannelOutcome::Exists
                } else if owned >= MAX_CHANNELS_OWNED
                    || world_owns >= MAX_CHANNELS_PER_WORLD
                    || self.channels_of(by) >= MAX_CHANNELS_JOINED
                {
                    ChannelOutcome::Refused
                } else {
                    let mut m = BTreeMap::new();
                    m.insert(by.key(), by.clone());
                    self.channels.channels.insert(
                        name.clone(),
                        Channel {
                            owner: by.clone(),
                            members: m,
                            invites: BTreeSet::new(),
                        },
                    );
                    changed = true;
                    ChannelOutcome::Ok
                }
            }
            ChannelOp::Invite => match (self.channels.channels.get_mut(&name), who) {
                (None, _) => ChannelOutcome::NoChannel,
                (Some(c), _) if !c.members.contains_key(&by.key()) => ChannelOutcome::NotMember,
                (Some(_), None) => ChannelOutcome::Refused,
                (Some(c), Some(who)) => {
                    if c.members.contains_key(&who.key()) {
                        ChannelOutcome::AlreadyMember
                    } else if who.is_channel()
                        || !Self::federated(dir, who.world())
                        || c.members.len() >= MAX_CHANNEL_MEMBERS
                        || (c.invites.len() >= MAX_CHANNEL_INVITES
                            && !c.invites.contains(&who.key()))
                    {
                        ChannelOutcome::Refused
                    } else if c.invites.contains(&who.key()) {
                        // Already invited: the invitation stands, and is not shown again.
                        ChannelOutcome::Ok
                    } else if !self
                        .recipient_buckets
                        .entry((slug.to_owned(), who.key()))
                        .or_insert_with(|| Bucket::new(recipient_rate, now))
                        .take(now)
                    {
                        // An invitation is shown as a tell is, and held to the same per-recipient
                        // pace.
                        ChannelOutcome::Refused
                    } else {
                        c.invites.insert(who.key());
                        changed = true;
                        self.invited_at.insert((name.clone(), who.key()), now);
                        if self.links.get(who.world()).is_some_and(|l| l.greeted) {
                            out.push(Out::Send(
                                who.world().to_owned(),
                                Message::ChannelInvite {
                                    channel: name.clone(),
                                    by: by.clone(),
                                    to: who,
                                },
                            ));
                        }
                        ChannelOutcome::Ok
                    }
                }
            },
            ChannelOp::Join => {
                let joined = self.channels_of(by);
                match self.channels.channels.get_mut(&name) {
                    None => ChannelOutcome::NoChannel,
                    Some(c) => {
                        if c.members.contains_key(&by.key()) {
                            ChannelOutcome::AlreadyMember
                        } else if joined >= MAX_CHANNELS_JOINED
                            || c.members.len() >= MAX_CHANNEL_MEMBERS
                        {
                            ChannelOutcome::Refused
                        } else if c.invites.remove(&by.key()) {
                            c.members.insert(by.key(), by.clone());
                            changed = true;
                            ChannelOutcome::Ok
                        } else {
                            ChannelOutcome::NotInvited
                        }
                    }
                }
            }
            ChannelOp::Leave => {
                let mut ended = false;
                let outcome = match self.channels.channels.get_mut(&name) {
                    None => ChannelOutcome::NoChannel,
                    Some(c) => {
                        if c.members.remove(&by.key()).is_some() {
                            changed = true;
                            if c.members.is_empty() {
                                ended = true;
                            } else if c.owner.same(by) {
                                // The first remaining member by key becomes the owner.
                                if let Some(next) = c.members.values().next() {
                                    c.owner = next.clone();
                                }
                            }
                            ChannelOutcome::Ok
                        } else if c.invites.remove(&by.key()) {
                            // Declining an invitation is leaving before joining.
                            changed = true;
                            ChannelOutcome::Ok
                        } else {
                            ChannelOutcome::NotMember
                        }
                    }
                };
                if ended {
                    self.channels.channels.remove(&name);
                }
                outcome
            }
            ChannelOp::List => {
                channels = self
                    .channels
                    .channels
                    .iter()
                    .filter(|(_, c)| c.members.contains_key(&by.key()))
                    .map(|(n, _)| n.clone())
                    .collect();
                ChannelOutcome::Ok
            }
            ChannelOp::Members => match self.channels.channels.get(&name) {
                None => ChannelOutcome::NoChannel,
                Some(c) if !c.members.contains_key(&by.key()) => ChannelOutcome::NotMember,
                Some(c) => {
                    members = c.members.values().cloned().collect();
                    ChannelOutcome::Ok
                }
            },
        };
        if changed {
            self.changed_channels.insert(name.clone());
        }
        out.push(Out::Send(
            slug.to_owned(),
            fit(Message::ChannelResult {
                id,
                outcome,
                channel: name,
                members,
                channels,
            }),
        ));
        if changed {
            out.push(Out::ChannelsChanged);
        }
        out
    }

    /// How many channels `who` belongs to.
    fn channels_of(&self, who: &Address) -> usize {
        let key = who.key();
        self.channels
            .channels
            .values()
            .filter(|c| c.members.contains_key(&key))
            .count()
    }
}

/// A person result with no code and no characters.
fn person_result(id: String, outcome: PersonOutcome) -> Message {
    Message::PersonResult {
        id,
        outcome,
        code: None,
        expires_secs: None,
        url: None,
        characters: Vec::new(),
    }
}

/// `m`, a person result, under the request's id.
fn with_id(mut m: Message, id: String) -> Message {
    if let Message::PersonResult { id: i, .. } = &mut m {
        *i = id;
    }
    m
}

/// What a person request asks, beside who it is for.
struct Asked<'a> {
    op: PersonOp,
    target: Option<&'a Address>,
    targets: &'a [Address],
    shown: Option<bool>,
}

/// The `person.friends` message of `who`'s view.
fn view_message(who: &Address, view: &PersonView) -> Message {
    Message::person_view(who.clone(), view.linked, view.alts, view.persons.clone())
}

/// `m` cut to fit one frame: a channel's member list, or a character's channel list, loses its last
/// entries until the frame is within the limit.
fn fit(mut m: Message) -> Message {
    while m.to_json().len() > MAX_MESSAGE_BYTES {
        match &mut m {
            Message::ChannelResult {
                members, channels, ..
            } if !members.is_empty() || !channels.is_empty() => {
                if members.pop().is_none() {
                    channels.pop();
                }
            }
            _ => break,
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap as Map;

    struct Dir(Map<String, Federation>);

    impl Directory for Dir {
        fn lookup(&self, slug: &str) -> Option<DirEntry> {
            self.0.get(slug).map(|&federation| DirEntry {
                name: slug.to_uppercase(),
                federation,
            })
        }
    }

    fn dir() -> Dir {
        Dir([
            ("aa".to_owned(), Federation::On),
            ("bb".to_owned(), Federation::On),
            ("cc".to_owned(), Federation::On),
            ("off".to_owned(), Federation::Off),
            ("unset".to_owned(), Federation::Unset),
        ]
        .into_iter()
        .collect())
    }

    fn a(s: &str) -> Address {
        Address::parse(s).expect("an address")
    }

    fn hub_with(worlds: &[&str]) -> Hub {
        hub_with_limits(worlds, Limits::DEFAULT)
    }

    fn hub_with_limits(worlds: &[&str], limits: Limits) -> Hub {
        let mut h = Hub::new("hub test", Channels::default(), limits);
        let d = dir();
        for w in worlds {
            h.connect(w, &w.to_uppercase());
            h.receive(
                &d,
                w,
                Message::Hello {
                    proto: PROTOCOL,
                    software: "t".into(),
                },
                0.0,
            );
        }
        h
    }

    fn tell(id: &str, from: &str, to: &str) -> Message {
        Message::Tell {
            id: id.into(),
            from: a(from),
            to: a(to),
            text: "hi".into(),
            friend: false,
        }
    }

    fn sent_to<'a>(out: &'a [Out], slug: &str) -> Vec<&'a Message> {
        out.iter()
            .filter_map(|o| match o {
                Out::Send(s, m) if s == slug => Some(m),
                _ => None,
            })
            .collect()
    }

    fn result_of(out: &[Out], slug: &str) -> Option<Outcome> {
        sent_to(out, slug).into_iter().find_map(|m| match m {
            Message::TellResult { outcome, .. } => Some(*outcome),
            _ => None,
        })
    }

    /// Spec: hub: a world speaks only after hello, and is welcomed with the other worlds.
    #[test]
    fn a_world_is_welcomed_after_hello_and_announced_to_the_others() {
        let d = dir();
        let mut h = hub_with(&["aa"]);
        h.connect("bb", "BB");
        let early = h.receive(&d, "bb", tell("1", "Bob@bb", "Ann@aa"), 0.0);
        assert!(early.contains(&Out::Close("bb".into(), "spoke before hello")));
        h.connect("bb", "BB");
        let out = h.receive(
            &d,
            "bb",
            Message::Hello {
                proto: PROTOCOL,
                software: "t".into(),
            },
            0.0,
        );
        let welcome = sent_to(&out, "bb");
        assert!(
            matches!(welcome[0], Message::Welcome { world, worlds, .. } if world == "bb" && worlds.len() == 2)
        );
        assert!(sent_to(&out, "aa")
            .iter()
            .any(|m| matches!(m, Message::World(w) if w.slug == "bb" && w.online)));
        let wrong = {
            let mut h = hub_with(&[]);
            h.connect("aa", "AA");
            h.receive(
                &d,
                "aa",
                Message::Hello {
                    proto: PROTOCOL + 1,
                    software: "t".into(),
                },
                0.0,
            )
        };
        assert!(wrong.contains(&Out::Close("aa".into(), "protocol version")));
    }

    /// Spec: hub: a tell goes to its target world under a hub id; the answer comes back under the
    /// sender's.
    #[test]
    fn a_tell_reaches_its_world_and_the_answer_returns_to_the_sender() {
        let d = dir();
        let mut h = hub_with(&["aa", "bb"]);
        let out = h.receive(&d, "aa", tell("t1", "Ann@aa", "Bob@bb"), 1.0);
        let [Message::Tell {
            id: hub_id, from, ..
        }] = sent_to(&out, "bb")[..]
        else {
            panic!("one tell to bb: {out:?}")
        };
        assert_ne!(
            hub_id, "t1",
            "the target sees the hub's id, not the sender's"
        );
        assert_eq!(from.to_string(), "Ann@aa");
        let back = h.receive(
            &d,
            "bb",
            Message::TellResult {
                id: hub_id.clone(),
                outcome: Outcome::Delivered,
                message: None,
            },
            1.1,
        );
        assert_eq!(
            sent_to(&back, "aa"),
            [&Message::TellResult {
                id: "t1".into(),
                outcome: Outcome::Delivered,
                message: None
            }]
        );
    }

    /// Spec: hub: a world can only speak for its own characters.
    #[test]
    fn a_world_cannot_send_as_another_worlds_character() {
        let d = dir();
        let mut h = hub_with(&["aa", "bb"]);
        let out = h.receive(&d, "aa", tell("t1", "Eve@bb", "Bob@bb"), 1.0);
        assert!(sent_to(&out, "bb").is_empty());
        assert!(
            matches!(sent_to(&out, "aa")[0], Message::Error { code, .. } if code == "identity")
        );
    }

    /// Spec: hub: a world off the network, opted out or never opted in, is not on the network.
    #[test]
    fn a_tell_to_a_world_not_on_the_network_is_answered_at_once() {
        let d = dir();
        let mut h = hub_with(&["aa"]);
        for to in ["Bob@off", "Bob@unset", "Bob@nowhere"] {
            let out = h.receive(&d, "aa", tell(to, "Ann@aa", to), 1.0);
            assert_eq!(result_of(&out, "aa"), Some(Outcome::NotOnNetwork), "{to}");
        }
    }

    /// Spec: hub: a tell for a federated world whose link is down is held a minute, then answered
    /// unavailable; delivered if the world returns in time.
    #[test]
    fn a_tell_for_a_reconnecting_world_is_held_for_a_minute() {
        let d = dir();
        let mut h = hub_with(&["aa"]);
        let out = h.receive(&d, "aa", tell("t1", "Ann@aa", "Bob@bb"), 0.0);
        assert!(out.is_empty(), "held, not answered");
        assert_eq!(h.held_count(), 1);
        h.connect("bb", "BB");
        let back = h.receive(
            &d,
            "bb",
            Message::Hello {
                proto: PROTOCOL,
                software: "t".into(),
            },
            30.0,
        );
        assert!(sent_to(&back, "bb")
            .iter()
            .any(|m| matches!(m, Message::Tell { .. })));
        assert_eq!(h.held_count(), 0);

        let mut h = hub_with(&["aa"]);
        h.receive(&d, "aa", tell("t2", "Ann@aa", "Bob@bb"), 0.0);
        assert!(h.tick(&d, 59.0).is_empty());
        assert_eq!(
            result_of(&h.tick(&d, 60.0), "aa"),
            Some(Outcome::Unavailable)
        );
    }

    /// Spec: hub: a target world that never answers is answered for.
    #[test]
    fn an_unanswered_tell_is_answered_unavailable_and_a_lost_link_answers_its_tells() {
        let d = dir();
        let mut h = hub_with(&["aa", "bb"]);
        h.receive(&d, "aa", tell("t1", "Ann@aa", "Bob@bb"), 0.0);
        assert_eq!(
            result_of(&h.tick(&d, ANSWER_SECS), "aa"),
            Some(Outcome::Unavailable)
        );
        h.receive(&d, "aa", tell("t2", "Ann@aa", "Bob@bb"), 40.0);
        let out = h.disconnect("bb");
        assert_eq!(result_of(&out, "aa"), Some(Outcome::Unavailable));
        assert!(sent_to(&out, "aa")
            .iter()
            .any(|m| matches!(m, Message::World(w) if w.slug == "bb" && !w.online)));
    }

    /// Spec: hub: a resent tell is delivered once.
    #[test]
    fn a_resent_tell_is_delivered_once() {
        let d = dir();
        let mut h = hub_with(&["aa", "bb"]);
        let first = h.receive(&d, "aa", tell("t1", "Ann@aa", "Bob@bb"), 0.0);
        let again = h.receive(&d, "aa", tell("t1", "Ann@aa", "Bob@bb"), 1.0);
        assert_eq!(sent_to(&first, "bb").len(), 1);
        assert!(again.is_empty());
    }

    /// Spec: rate limits: a character's tells, and the loop guard between two correspondents.
    #[test]
    fn a_characters_tells_are_rate_limited_and_a_reply_loop_is_cut() {
        let d = dir();
        let mut h = hub_with(&["aa", "bb"]);
        let burst = 10;
        assert!((Limits::DEFAULT.tells_per_character.burst - 10.0).abs() < f64::EPSILON);
        for i in 0..burst {
            let out = h.receive(&d, "aa", tell(&format!("t{i}"), "Ann@aa", "Bob@bb"), 0.0);
            assert_eq!(sent_to(&out, "bb").len(), 1, "tell {i} goes");
        }
        let over = h.receive(&d, "aa", tell("over", "Ann@aa", "Bob@bb"), 0.0);
        assert_eq!(result_of(&over, "aa"), Some(Outcome::RateLimited));

        // Two answerers, each well inside its own bucket, sending each other a tell a second.
        let mut h = hub_with(&["aa", "bb"]);
        let mut cut = None;
        for i in 0..60 {
            let t = f64::from(i) * 2.0;
            let x = h.receive(&d, "aa", tell(&format!("a{i}"), "Ann@aa", "Bob@bb"), t);
            let y = h.receive(
                &d,
                "bb",
                tell(&format!("b{i}"), "Bob@bb", "Ann@aa"),
                t + 1.0,
            );
            if result_of(&x, "aa") == Some(Outcome::RateLimited)
                || result_of(&y, "bb") == Some(Outcome::RateLimited)
            {
                cut = Some(i);
                break;
            }
        }
        assert!(cut.is_some(), "the pair is cut within a minute or two");
        let blocked = h.receive(&d, "aa", tell("later", "Ann@aa", "Bob@bb"), 200.0);
        assert_eq!(result_of(&blocked, "aa"), Some(Outcome::RateLimited));
    }

    /// Spec: hub: a room line reaches every other world carrying the room, and only those.
    #[test]
    fn a_room_line_reaches_the_other_worlds_carrying_the_room() {
        let d = dir();
        let mut h = hub_with(&["aa", "bb", "cc"]);
        for w in ["aa", "bb"] {
            h.receive(
                &d,
                w,
                Message::RoomJoin {
                    room: "global".into(),
                    on: true,
                },
                0.0,
            );
        }
        let say = |id: &str, from: &str| Message::RoomSay {
            id: id.into(),
            room: "global".into(),
            from: a(from),
            text: "party at 8".into(),
        };
        let out = h.receive(&d, "aa", say("r1", "Ann@aa"), 1.0);
        assert!(matches!(
            sent_to(&out, "bb")[..],
            [Message::RoomLine { .. }]
        ));
        assert!(sent_to(&out, "cc").is_empty(), "cc does not carry the room");
        assert!(matches!(
            sent_to(&out, "aa")[..],
            [Message::RoomResult {
                outcome: Outcome::Ok,
                ..
            }]
        ));
        let not_carried = h.receive(&d, "cc", say("r2", "Cid@cc"), 1.0);
        assert!(matches!(
            sent_to(&not_carried, "cc")[..],
            [Message::RoomResult {
                outcome: Outcome::NotMember,
                ..
            }]
        ));
    }

    fn channel(
        h: &mut Hub,
        w: &str,
        op: ChannelOp,
        by: &str,
        who: Option<&str>,
    ) -> (ChannelOutcome, Vec<Out>) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let out = h.receive(
            &dir(),
            w,
            Message::Channel {
                id: format!("c{id}"),
                op,
                channel: "oldguard".into(),
                by: a(by),
                who: who.map(a),
            },
            0.0,
        );
        let outcome = sent_to(&out, w)
            .into_iter()
            .find_map(|m| match m {
                Message::ChannelResult { outcome, .. } => Some(*outcome),
                _ => None,
            })
            .expect("a channel result");
        (outcome, out)
    }

    /// Spec: hub: a channel is joined by invitation, and a tell to it reaches every other member.
    #[test]
    fn a_channel_is_joined_by_invitation_and_its_lines_reach_every_other_member() {
        let d = dir();
        let mut h = hub_with(&["aa", "bb", "cc"]);
        assert_eq!(
            channel(&mut h, "aa", ChannelOp::Create, "Ann@aa", None).0,
            ChannelOutcome::Ok
        );
        assert_eq!(
            channel(&mut h, "aa", ChannelOp::Create, "Ann@aa", None).0,
            ChannelOutcome::Exists
        );
        assert_eq!(
            channel(&mut h, "bb", ChannelOp::Join, "Bob@bb", None).0,
            ChannelOutcome::NotInvited
        );
        let (ok, out) = channel(&mut h, "aa", ChannelOp::Invite, "Ann@aa", Some("Bob@bb"));
        assert_eq!(ok, ChannelOutcome::Ok);
        assert!(matches!(
            sent_to(&out, "bb")[..],
            [Message::ChannelInvite { .. }]
        ));
        assert!(out.contains(&Out::ChannelsChanged));
        assert_eq!(
            channel(&mut h, "bb", ChannelOp::Join, "Bob@bb", None).0,
            ChannelOutcome::Ok
        );

        let out = h.receive(&d, "bb", tell("t", "Bob@bb", "oldguard@channel"), 1.0);
        assert_eq!(result_of(&out, "bb"), Some(Outcome::Delivered));
        assert!(matches!(
            sent_to(&out, "aa")[..],
            [Message::ChannelLine { to, from, .. }] if to.to_string() == "Ann@aa" && from.to_string() == "Bob@bb"
        ));
        assert!(
            sent_to(&out, "bb").len() == 1,
            "the sender gets only the result"
        );
        let outsider = h.receive(&d, "cc", tell("t", "Cid@cc", "oldguard@channel"), 1.0);
        assert_eq!(result_of(&outsider, "cc"), Some(Outcome::NotMember));

        assert_eq!(
            channel(&mut h, "bb", ChannelOp::Leave, "Bob@bb", None).0,
            ChannelOutcome::Ok
        );
        assert_eq!(
            channel(&mut h, "aa", ChannelOp::Leave, "Ann@aa", None).0,
            ChannelOutcome::Ok
        );
        assert!(
            h.channels.channels.is_empty(),
            "the last member leaving ends the channel"
        );
    }

    /// Spec: federation: a world whose flag is turned off loses its link.
    #[test]
    fn a_linked_world_whose_flag_goes_off_is_closed() {
        let mut d = dir();
        let mut h = hub_with(&["aa", "bb"]);
        d.0.insert("bb".into(), Federation::Off);
        let out = h.tick(&d, 1.0);
        assert!(out.contains(&Out::Close("bb".into(), "federation no longer on")));
        assert_eq!(h.linked().collect::<Vec<_>>(), ["aa"]);
        let to_off = h.receive(&d, "aa", tell("t", "Ann@aa", "Bob@bb"), 2.0);
        assert_eq!(result_of(&to_off, "aa"), Some(Outcome::NotOnNetwork));
    }

    /// Spec: hub: presence and watches go only to the world named, and only from the world named.
    #[test]
    fn presence_and_watches_go_between_the_two_worlds_named() {
        let d = dir();
        let mut h = hub_with(&["aa", "bb", "cc"]);
        let watch = h.receive(
            &d,
            "aa",
            Message::Watch {
                owner: a("Ann@aa"),
                target: a("Bob@bb"),
                on: true,
            },
            0.0,
        );
        assert_eq!(sent_to(&watch, "bb").len(), 1);
        assert!(sent_to(&watch, "cc").is_empty());
        let spoof = h.receive(
            &d,
            "cc",
            Message::Presence {
                to: a("Ann@aa"),
                of: a("Bob@bb"),
                online: true,
            },
            0.0,
        );
        assert!(sent_to(&spoof, "aa").is_empty(), "cc cannot speak for bb");
    }

    /// Letters only, different for each `i`.
    fn letters(i: usize) -> String {
        let mut n = String::new();
        let mut i = i;
        loop {
            n.push(char::from(b'a' + u8::try_from(i % 26).expect("a letter")));
            i /= 26;
            if i == 0 {
                break n;
            }
        }
    }

    /// The registry with keys: a slug's flag and the digest of its current key.
    struct Keyed(std::cell::RefCell<Map<String, Option<String>>>);

    impl Directory for Keyed {
        fn lookup(&self, slug: &str) -> Option<DirEntry> {
            self.0.borrow().contains_key(slug).then(|| DirEntry {
                name: slug.to_uppercase(),
                federation: Federation::On,
            })
        }
        fn key(&self, slug: &str) -> Option<String> {
            self.0.borrow().get(slug).cloned().flatten()
        }
    }

    /// Spec: hub: a link stays up only while the key it was opened with is the world's key.
    #[test]
    fn a_link_whose_key_is_rotated_or_revoked_is_closed() {
        let d = Keyed(std::cell::RefCell::new(
            [
                ("aa".to_owned(), Some("k1".to_owned())),
                ("bb".to_owned(), Some("k2".to_owned())),
            ]
            .into_iter()
            .collect(),
        ));
        let mut h = Hub::new("hub test", Channels::default(), Limits::DEFAULT);
        h.connect_keyed("aa", "AA", "k1");
        h.connect_keyed("bb", "BB", "k2");
        assert!(h.tick(&d, 1.0).is_empty(), "both keys are current");
        d.0.borrow_mut().insert("aa".into(), None);
        d.0.borrow_mut().insert("bb".into(), Some("k3".into()));
        let out = h.tick(&d, 2.0);
        assert!(out.contains(&Out::Close("aa".into(), "its key was rotated or revoked")));
        assert!(out.contains(&Out::Close("bb".into(), "its key was rotated or revoked")));
        assert_eq!(h.linked().count(), 0);
    }

    /// Spec: link protocol: every frame the hub sends is within the limit, however many worlds
    /// are linked and however long their names.
    #[test]
    fn the_welcome_fits_a_frame_with_hundreds_of_long_named_worlds() {
        let d = dir();
        let mut h = Hub::new("hub test", Channels::default(), Limits::DEFAULT);
        let name = "Ü".repeat(80);
        let slugs: Vec<String> = (0..300).map(|i| format!("world-{i:06}")).collect();
        for s in &slugs {
            h.connect(s, &name);
            h.receive(
                &d,
                s,
                Message::Hello {
                    proto: PROTOCOL,
                    software: "t".into(),
                },
                0.0,
            );
        }
        h.connect("aa", "AA");
        let out = h.receive(
            &d,
            "aa",
            Message::Hello {
                proto: PROTOCOL,
                software: "t".into(),
            },
            0.0,
        );
        let to_aa = sent_to(&out, "aa");
        assert!(matches!(to_aa[0], Message::Welcome { .. }));
        let mut named: Vec<String> = Vec::new();
        for m in &to_aa {
            let frame = m.to_json();
            assert!(frame.len() <= MAX_MESSAGE_BYTES, "{} bytes", frame.len());
            assert!(Message::from_json(&frame).is_ok());
            match m {
                Message::Welcome { worlds, .. } => {
                    named.extend(worlds.iter().map(|w| w.slug.clone()))
                }
                Message::World(w) => named.push(w.slug.clone()),
                _ => {}
            }
        }
        assert_eq!(
            named.len(),
            301,
            "every world is named, in the welcome or after it"
        );
    }

    /// Spec: rate limits: a world that invents its characters' names is still held to its
    /// world's share of room lines and of tells to one recipient.
    #[test]
    fn a_world_inventing_names_is_held_to_its_world_limits() {
        let d = dir();
        let mut h = hub_with(&["aa", "bb"]);
        for w in ["aa", "bb"] {
            h.receive(
                &d,
                w,
                Message::RoomJoin {
                    room: "global".into(),
                    on: true,
                },
                0.0,
            );
        }
        let mut lines = 0;
        let mut tells = 0;
        for i in 0..100u32 {
            let name = format!("Mint{}", letters(i as usize));
            let out = h.receive(
                &d,
                "aa",
                Message::RoomSay {
                    id: format!("r{i}"),
                    room: "global".into(),
                    from: a(&format!("{name}@aa")),
                    text: "free gold".into(),
                },
                1.0,
            );
            lines += sent_to(&out, "bb").len();
            let out = h.receive(
                &d,
                "aa",
                tell(&format!("t{i}"), &format!("{name}@aa"), "Bob@bb"),
                1.0,
            );
            tells += sent_to(&out, "bb").len();
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (room_burst, tell_burst) = (
            Limits::DEFAULT.room_lines_per_world.burst as usize,
            Limits::DEFAULT.tells_per_recipient.burst as usize,
        );
        assert_eq!(lines, room_burst);
        assert_eq!(tells, tell_burst);
    }

    /// Spec: hub: one world may send any one other world only a quarter of the per-world pace in
    /// tells, so it can never send more than that world can answer; other worlds are untouched.
    #[test]
    fn one_world_sends_another_only_a_share_of_its_pace_in_tells() {
        let d = dir();
        let mut h = hub_with(&["aa", "bb", "cc"]);
        let share = Limits::DEFAULT.tells_per_world_pair();
        assert!(
            share.per_sec * 2.0 < Limits::DEFAULT.messages_per_world.per_sec * 0.9,
            "two worlds at their share leave a target room to answer them and its own"
        );
        let (mut forwarded, mut limited) = (0usize, 0usize);
        // Invented senders to invented recipients: every per-character and per-recipient bucket
        // is fresh, so only the world pair's share holds.
        for i in 0..150usize {
            let out = h.receive(
                &d,
                "aa",
                tell(
                    &format!("t{i}"),
                    &format!("Mint{}@aa", letters(i)),
                    &format!("Nobody{}@bb", letters(i)),
                ),
                1.0,
            );
            forwarded += sent_to(&out, "bb").len();
            limited += usize::from(result_of(&out, "aa") == Some(Outcome::RateLimited));
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let burst = share.burst as usize;
        assert_eq!((forwarded, limited), (burst, 150 - burst));
        let out = h.receive(&d, "aa", tell("to-cc", "Ann@aa", "Cid@cc"), 1.0);
        assert_eq!(
            sent_to(&out, "cc").len(),
            1,
            "a third world still gets aa's tells"
        );
        let out = h.receive(&d, "aa", tell("later", "Ann@aa", "Bob@bb"), 2.0);
        assert_eq!(sent_to(&out, "bb").len(), 1, "the share refills");
    }

    /// Spec: hub: an invitation is handed on once; a world's channels and a channel's invitations
    /// are capped, and invitations lapse.
    #[test]
    fn invitations_are_handed_on_once_capped_and_lapse() {
        let d = dir();
        // A world's own message limit out of the way, to reach the channel limits.
        let roomy = Limits {
            messages_per_world: dereth_network_protocol::limits::Rate {
                burst: 1e6,
                per_sec: 1e6,
            },
            ..Limits::DEFAULT
        };
        let mut h = hub_with_limits(&["aa", "bb"], roomy);
        assert_eq!(
            channel(&mut h, "aa", ChannelOp::Create, "Ann@aa", None).0,
            ChannelOutcome::Ok
        );
        let mut forwarded = 0;
        for _ in 0..5 {
            let (ok, out) = channel(&mut h, "aa", ChannelOp::Invite, "Ann@aa", Some("Bob@bb"));
            assert_eq!(ok, ChannelOutcome::Ok);
            forwarded += sent_to(&out, "bb").len();
        }
        assert_eq!(forwarded, 1, "repeats are not shown again");
        for i in 0..(MAX_CHANNEL_INVITES + 5) {
            let who = format!("M{}@bb", letters(i));
            channel(&mut h, "aa", ChannelOp::Invite, "Ann@aa", Some(&who));
        }
        assert_eq!(
            h.channels.channels["oldguard"].invites.len(),
            MAX_CHANNEL_INVITES
        );
        // A week on, every invitation has lapsed.
        h.tick(&d, 1.0);
        h.tick(&d, INVITE_SECS + 2.0);
        assert!(h.channels.channels["oldguard"].invites.is_empty());
        assert!(h.take_changed_channels().contains("oldguard"));

        let mut h = hub_with_limits(&["aa"], roomy);
        let mut made = 0;
        for i in 0..(MAX_CHANNELS_PER_WORLD + 10) {
            let out = h.receive(
                &d,
                "aa",
                Message::Channel {
                    id: format!("c{i}"),
                    op: ChannelOp::Create,
                    channel: format!("channel{i}"),
                    by: a(&format!("Own{}@aa", letters(i))),
                    who: None,
                },
                0.0,
            );
            if out.contains(&Out::ChannelsChanged) {
                made += 1;
            }
        }
        assert_eq!(
            made, MAX_CHANNELS_PER_WORLD,
            "invented owners do not lift the world's cap"
        );
    }

    /// Spec: link protocol: a resent channel request is applied once.
    #[test]
    fn a_resent_channel_request_is_applied_once() {
        let d = dir();
        let mut h = hub_with(&["aa"]);
        let req = Message::Channel {
            id: "c1".into(),
            op: ChannelOp::Create,
            channel: "oldguard".into(),
            by: a("Ann@aa"),
            who: None,
        };
        let first = h.receive(&d, "aa", req.clone(), 0.0);
        assert!(first.contains(&Out::ChannelsChanged));
        let again = h.receive(&d, "aa", req, 1.0);
        assert!(!again.contains(&Out::ChannelsChanged), "applied once");
        assert_eq!(
            sent_to(&again, "aa"),
            sent_to(&first, "aa"),
            "answered as the first was (ok, not exists)"
        );
    }

    /// Spec: link protocol: a resend of a tell, a room line or a channel request whose answer the
    /// sender's world missed (its link was down when the answer came) is answered as the first
    /// was, not dropped unanswered.
    #[test]
    fn a_resend_whose_answer_was_missed_is_answered_as_the_first_was() {
        let d = dir();
        let mut h = hub_with(&["aa"]);
        // bb is away: the tell is held for it.
        assert!(h
            .receive(&d, "aa", tell("t1", "Ann@aa", "Bob@bb"), 0.0)
            .is_empty());
        h.receive(
            &d,
            "aa",
            Message::RoomJoin {
                room: "global".into(),
                on: true,
            },
            0.0,
        );
        let say = Message::RoomSay {
            id: "r1".into(),
            room: "global".into(),
            from: a("Ann@aa"),
            text: "party at 8".into(),
        };
        let create = Message::Channel {
            id: "c1".into(),
            op: ChannelOp::Create,
            channel: "oldguard".into(),
            by: a("Ann@aa"),
            who: None,
        };
        h.receive(&d, "aa", say.clone(), 0.5);
        h.receive(&d, "aa", create.clone(), 0.5);
        // aa's link goes; bb comes back, is handed the tell and shows it.
        h.disconnect("aa");
        h.connect("bb", "BB");
        let back = h.receive(
            &d,
            "bb",
            Message::Hello {
                proto: PROTOCOL,
                software: "t".into(),
            },
            10.0,
        );
        let [Message::Tell { id: hub_id, .. }] = sent_to(&back, "bb")
            .into_iter()
            .filter(|m| matches!(m, Message::Tell { .. }))
            .collect::<Vec<_>>()[..]
        else {
            panic!("the held tell goes to bb: {back:?}")
        };
        let answered = h.receive(
            &d,
            "bb",
            Message::TellResult {
                id: hub_id.clone(),
                outcome: Outcome::Delivered,
                message: None,
            },
            10.5,
        );
        assert_eq!(result_of(&answered, "aa"), Some(Outcome::Delivered));
        // aa is back and sends again what it never had an answer to.
        h.connect("aa", "AA");
        h.receive(
            &d,
            "aa",
            Message::Hello {
                proto: PROTOCOL,
                software: "t".into(),
            },
            11.0,
        );
        let again = h.receive(&d, "aa", tell("t1", "Ann@aa", "Bob@bb"), 11.5);
        assert!(sent_to(&again, "bb").is_empty(), "not shown twice");
        assert_eq!(
            sent_to(&again, "aa"),
            [&Message::TellResult {
                id: "t1".into(),
                outcome: Outcome::Delivered,
                message: None
            }]
        );
        let again = h.receive(&d, "aa", say, 11.5);
        assert_eq!(
            sent_to(&again, "aa"),
            [&Message::RoomResult {
                id: "r1".into(),
                outcome: Outcome::Ok
            }]
        );
        let again = h.receive(&d, "aa", create, 11.5);
        assert!(matches!(
            sent_to(&again, "aa")[..],
            [Message::ChannelResult {
                outcome: ChannelOutcome::Ok,
                ..
            }]
        ));
        assert!(!again.contains(&Out::ChannelsChanged));
        // Once the hub has forgotten the id, it is a new request.
        h.tick(&d, DEDUP_SECS + 1.0);
        let later = h.receive(&d, "aa", tell("t1", "Ann@aa", "Bob@bb"), DEDUP_SECS + 1.0);
        assert_eq!(sent_to(&later, "bb").len(), 1);
    }

    /// Spec: hub: a channel line, and an invitation, is held to the paces a tell to its recipient
    /// is: the per-recipient limit and the world pair's share.
    #[test]
    fn channel_lines_and_invitations_are_held_to_the_paces_of_a_tell() {
        let d = dir();
        let roomy = Limits {
            messages_per_world: dereth_network_protocol::limits::Rate {
                burst: 1e6,
                per_sec: 1e6,
            },
            tells_per_character: dereth_network_protocol::limits::Rate {
                burst: 1e6,
                per_sec: 1e6,
            },
            ..Limits::DEFAULT
        };
        let mut h = hub_with_limits(&["aa", "bb"], roomy);
        channel(&mut h, "bb", ChannelOp::Create, "Bob@bb", None);
        channel(&mut h, "bb", ChannelOp::Invite, "Bob@bb", Some("Ann@aa"));
        channel(&mut h, "aa", ChannelOp::Join, "Ann@aa", None);
        // bb invents members, who flood the channel.
        for i in 0..30 {
            let m = format!("Mint{}@bb", letters(i));
            channel(&mut h, "bb", ChannelOp::Invite, "Bob@bb", Some(&m));
            channel(&mut h, "bb", ChannelOp::Join, &m, None);
        }
        let mut lines = 0;
        let mut last = None;
        for i in 0..60 {
            let from = format!("Mint{}@bb", letters(i % 30));
            let out = h.receive(
                &d,
                "bb",
                tell(&format!("cl{i}"), &from, "oldguard@channel"),
                1.0,
            );
            lines += sent_to(&out, "aa").len();
            last = result_of(&out, "bb");
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let burst = Limits::DEFAULT.tells_per_recipient.burst as usize;
        assert_eq!(lines, burst, "Ann gets bb's per-recipient burst, no more");
        assert_eq!(
            last,
            Some(Outcome::RateLimited),
            "a line every member's pace holds back is refused as over a limit"
        );

        // Invitations to one character from one world, each from a channel of its own.
        let mut h = hub_with_limits(&["aa", "bb"], roomy);
        let mut shown = 0;
        for i in 0..40usize {
            let owner = format!("Own{}@bb", letters(i));
            let out = h.receive(
                &d,
                "bb",
                Message::Channel {
                    id: format!("k{i}"),
                    op: ChannelOp::Create,
                    channel: format!("channel{i}"),
                    by: a(&owner),
                    who: None,
                },
                1.0,
            );
            assert!(out.contains(&Out::ChannelsChanged));
            let out = h.receive(
                &d,
                "bb",
                Message::Channel {
                    id: format!("i{i}"),
                    op: ChannelOp::Invite,
                    channel: format!("channel{i}"),
                    by: a(&owner),
                    who: Some(a("Ann@aa")),
                },
                1.0,
            );
            shown += sent_to(&out, "aa").len();
        }
        assert_eq!(
            shown, burst,
            "invitations are held to the per-recipient pace"
        );
    }

    /// Spec: hub: one world may send another only a quarter of the per-world pace in watches, each
    /// being owed an answer; other worlds are untouched, and a stop always goes.
    #[test]
    fn one_world_sends_another_only_a_share_of_its_pace_in_watches() {
        let d = dir();
        let mut h = hub_with(&["aa", "bb", "cc"]);
        let watch = |i: usize, on: bool| Message::Watch {
            owner: a(&format!("Mint{}@aa", letters(i))),
            target: a("Bob@bb"),
            on,
        };
        let mut forwarded = 0;
        for i in 0..150 {
            forwarded += sent_to(&h.receive(&d, "aa", watch(i, true), 1.0), "bb").len();
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let share = Limits::DEFAULT.watches_per_world_pair().burst as usize;
        assert_eq!(forwarded, share);
        assert_eq!(
            sent_to(&h.receive(&d, "aa", watch(0, false), 1.0), "bb").len(),
            1,
            "a stop is owed nothing, and goes"
        );
        let to_cc = h.receive(
            &d,
            "aa",
            Message::Watch {
                owner: a("Ann@aa"),
                target: a("Cid@cc"),
                on: true,
            },
            1.0,
        );
        assert_eq!(sent_to(&to_cc, "cc").len(), 1, "another world is untouched");
        assert_eq!(
            sent_to(&h.receive(&d, "aa", watch(500, true), 2.0), "bb").len(),
            1,
            "the share refills"
        );
    }

    /// Spec: link protocol: no frame the hub hands on is over the limit, whatever a world sends
    /// within the field rules: the longest ids, addresses and texts, of the characters JSON writes
    /// longest, through every message the hub forwards.
    #[test]
    fn every_frame_the_hub_hands_on_is_within_the_limit() {
        let d = dir();
        let mut h = hub_with(&["aa", "bb"]);
        for w in ["aa", "bb"] {
            h.receive(
                &d,
                w,
                Message::RoomJoin {
                    room: "global".into(),
                    on: true,
                },
                0.0,
            );
        }
        let long_name = |w: &str| {
            a(&format!(
                "Ü{}x@{w}",
                "Ü".repeat(dereth_network_protocol::address::MAX_NAME_CHARS - 2)
            ))
        };
        let id = |c: &str| c.repeat(dereth_network_protocol::wire::MAX_ID_CHARS);
        let text = "\u{1}".repeat(dereth_network_protocol::text::MAX_TEXT_CHARS);
        let (ann, bob) = (long_name("aa"), long_name("bb"));
        channel(&mut h, "aa", ChannelOp::Create, "Ann@aa", None);
        let mut outs = Vec::new();
        outs.extend(h.receive(
            &d,
            "aa",
            Message::Watch {
                owner: ann.clone(),
                target: bob.clone(),
                on: true,
            },
            1.0,
        ));
        outs.extend(h.receive(
            &d,
            "aa",
            Message::Presence {
                to: bob.clone(),
                of: ann.clone(),
                online: true,
            },
            1.0,
        ));
        let forwarded = h.receive(
            &d,
            "aa",
            Message::Tell {
                id: id("t"),
                from: ann.clone(),
                to: bob.clone(),
                text: text.clone(),
                friend: true,
            },
            1.0,
        );
        let hub_id = sent_to(&forwarded, "bb")
            .into_iter()
            .find_map(|m| match m {
                Message::Tell { id, .. } => Some(id.clone()),
                _ => None,
            })
            .expect("the tell goes to bb");
        outs.extend(forwarded);
        outs.extend(h.receive(
            &d,
            "bb",
            Message::TellResult {
                id: hub_id,
                outcome: Outcome::Afk,
                message: Some(text.clone()),
            },
            1.0,
        ));
        outs.extend(h.receive(
            &d,
            "aa",
            Message::RoomSay {
                id: id("r"),
                room: "global".into(),
                from: ann.clone(),
                text: text.clone(),
            },
            1.0,
        ));
        outs.extend(h.receive(
            &d,
            "aa",
            Message::Channel {
                id: "inv".into(),
                op: ChannelOp::Invite,
                channel: "oldguard".into(),
                by: a("Ann@aa"),
                who: Some(bob.clone()),
            },
            1.0,
        ));
        outs.extend(h.receive(
            &d,
            "bb",
            Message::Channel {
                id: "join".into(),
                op: ChannelOp::Join,
                channel: "oldguard".into(),
                by: bob.clone(),
                who: None,
            },
            1.0,
        ));
        outs.extend(h.receive(
            &d,
            "aa",
            Message::Tell {
                id: id("c"),
                from: a("Ann@aa"),
                to: a("oldguard@channel"),
                text,
                friend: false,
            },
            1.0,
        ));
        let kinds: BTreeSet<&str> = outs
            .iter()
            .filter_map(|o| match o {
                Out::Send(_, m) => Some(m.kind()),
                _ => None,
            })
            .collect();
        for k in [
            "watch",
            "presence",
            "tell",
            "tell.result",
            "room.line",
            "channel.invite",
            "channel.line",
        ] {
            assert!(kinds.contains(k), "{k} was handed on: {kinds:?}");
        }
        for o in &outs {
            if let Out::Send(_, m) = o {
                let frame = m.to_json();
                assert!(
                    frame.len() <= MAX_MESSAGE_BYTES,
                    "{}: {} bytes",
                    m.kind(),
                    frame.len()
                );
            }
        }
    }

    /// Spec: hub: a link closed for speaking before `hello` is forgotten, never announced.
    #[test]
    fn a_link_closed_before_hello_is_forgotten() {
        let d = dir();
        let mut h = hub_with(&["aa"]);
        h.connect("bb", "BB");
        let out = h.receive(&d, "bb", tell("1", "Bob@bb", "Ann@aa"), 0.0);
        assert!(out.contains(&Out::Close("bb".into(), "spoke before hello")));
        assert_eq!(h.linked().collect::<Vec<_>>(), ["aa"]);
        assert!(h.disconnect("bb").is_empty(), "nothing to tell anyone");
    }

    /// Spec: link protocol: a channel's member list too long for one frame is cut to fit.
    #[test]
    fn a_member_list_too_long_for_a_frame_is_cut_to_fit() {
        let members: Vec<Address> = (0..400u32)
            .map(|i| {
                a(&format!(
                    "{}x{}@coldeve",
                    "Ü".repeat(28),
                    letters(i as usize)
                ))
            })
            .collect();
        let m = fit(Message::ChannelResult {
            id: "c".into(),
            outcome: ChannelOutcome::Ok,
            channel: "big".into(),
            members,
            channels: Vec::new(),
        });
        assert!(m.to_json().len() <= MAX_MESSAGE_BYTES);
        assert!(matches!(m, Message::ChannelResult { members, .. } if !members.is_empty()));
    }

    /// A directory with the person layer: a code to give, views to send, and what was asked.
    struct PersonDir {
        worlds: Dir,
        view: std::cell::RefCell<PersonView>,
        codes: std::cell::Cell<u32>,
        asked: std::cell::RefCell<Vec<String>>,
    }

    impl Directory for PersonDir {
        fn lookup(&self, slug: &str) -> Option<DirEntry> {
            self.worlds.lookup(slug)
        }
        fn person_code(&self, who: &Address) -> Result<(String, u32), PersonOutcome> {
            self.codes.set(self.codes.get() + 1);
            self.asked.borrow_mut().push(format!("code {who}"));
            Ok((
                dereth_network_protocol::person::link_code_from([
                    0,
                    0,
                    0,
                    0,
                    u8::try_from(self.codes.get()).unwrap_or(0),
                ]),
                600,
            ))
        }
        fn person_befriend(&self, who: &Address, target: &Address) -> PersonAnswer {
            self.asked
                .borrow_mut()
                .push(format!("befriend {who} {target}"));
            changed(who)
        }
        fn person_unfriend(&self, who: &Address, target: &Address) -> PersonAnswer {
            self.asked
                .borrow_mut()
                .push(format!("unfriend {who} {target}"));
            changed(who)
        }
        fn person_carry(&self, who: &Address, targets: &[Address]) -> PersonAnswer {
            let targets: Vec<String> = targets.iter().map(ToString::to_string).collect();
            self.asked
                .borrow_mut()
                .push(format!("carry {who} {}", targets.join(",")));
            changed(who)
        }
        fn returns_sent(&self, who: &Address) {
            self.asked.borrow_mut().push(format!("returned {who}"));
        }
        fn person_characters(&self, target: &Address) -> Vec<Address> {
            vec![target.clone()]
        }
        fn person_view(&self, who: &Address) -> PersonView {
            self.asked.borrow_mut().push(format!("view {who}"));
            self.view.borrow().clone()
        }
        fn person_alts(&self, who: &Address, shown: bool) -> PersonAnswer {
            self.asked.borrow_mut().push(format!("alts {who} {shown}"));
            if shown {
                PersonAnswer::unchanged(PersonOutcome::Refused)
            } else {
                changed(who)
            }
        }
    }

    /// `ok`, having changed the person of `who`, whose only character it is.
    fn changed(who: &Address) -> PersonAnswer {
        PersonAnswer {
            outcome: PersonOutcome::Ok,
            changed: vec![who.clone()],
        }
    }

    fn person_dir() -> PersonDir {
        PersonDir {
            worlds: dir(),
            view: std::cell::RefCell::new(PersonView::default()),
            codes: std::cell::Cell::new(0),
            asked: std::cell::RefCell::new(Vec::new()),
        }
    }

    fn person(id: &str, op: PersonOp, who: &str, target: Option<&str>) -> Message {
        Message::Person {
            id: id.into(),
            op,
            who: a(who),
            target: target.map(a),
            targets: Vec::new(),
            shown: None,
        }
    }

    fn person_outcome(out: &[Out], slug: &str) -> Option<(String, PersonOutcome, Option<String>)> {
        sent_to(out, slug).into_iter().find_map(|m| match m {
            Message::PersonResult {
                id, outcome, code, ..
            } => Some((id.clone(), *outcome, code.clone())),
            _ => None,
        })
    }

    /// Spec: link protocol: a person request or a `person.watch` for another world's character is
    /// refused as an identity error, and the hub asks nothing about it.
    #[test]
    fn a_person_request_for_another_worlds_character_is_refused() {
        let d = person_dir();
        let mut h = hub_with(&["aa", "bb"]);
        h.set_link_url(Some("https://site.test/characters/".into()));
        for m in [
            person("p1", PersonOp::Code, "Bob@bb", None),
            Message::PersonWatch {
                who: a("Bob@bb"),
                on: true,
            },
        ] {
            let out = h.receive(&d, "aa", m, 1.0);
            assert!(
                sent_to(&out, "aa")
                    .iter()
                    .any(|m| matches!(m, Message::Error { code, .. } if code == "identity")),
                "{out:?}"
            );
            assert!(person_outcome(&out, "aa").is_none());
            assert!(sent_to(&out, "bb").is_empty());
        }
        assert!(d.asked.borrow().is_empty(), "{:?}", d.asked.borrow());
    }

    /// Spec: link protocol: a hub with no page to enter a code on links no character: a code
    /// request is answered `unavailable`; with one, it is the code, its lapse and the page.
    #[test]
    fn a_link_code_needs_the_page_to_enter_it_on() {
        let d = person_dir();
        let mut h = hub_with(&["aa"]);
        let out = h.receive(&d, "aa", person("p1", PersonOp::Code, "Ann@aa", None), 1.0);
        assert_eq!(
            person_outcome(&out, "aa"),
            Some(("p1".into(), PersonOutcome::Unavailable, None))
        );
        h.set_link_url(Some("https://site.test/characters/".into()));
        let out = h.receive(&d, "aa", person("p2", PersonOp::Code, "Ann@aa", None), 1.0);
        let [Message::PersonResult {
            id,
            outcome: PersonOutcome::Ok,
            code: Some(code),
            expires_secs: Some(600),
            url: Some(url),
            ..
        }] = sent_to(&out, "aa")[..]
        else {
            panic!("a code: {out:?}")
        };
        assert_eq!(id, "p2");
        assert!(dereth_network_protocol::person::is_link_code(code));
        assert_eq!(url, "https://site.test/characters/");
    }

    /// Spec: link protocol: a world asking to show its character's person's characters is refused,
    /// with the page the person shows them on; hiding them is `ok`.
    #[test]
    fn showing_a_persons_characters_is_refused_with_the_page_it_is_done_on() {
        let d = person_dir();
        let mut h = hub_with(&["aa"]);
        h.set_person_url(Some("https://site.test/characters/".into()));
        let alts = |id: &str, shown: bool| Message::Person {
            id: id.into(),
            op: PersonOp::Alts,
            who: a("Ann@aa"),
            target: None,
            targets: Vec::new(),
            shown: Some(shown),
        };
        let out = h.receive(&d, "aa", alts("p1", true), 1.0);
        let [Message::PersonResult {
            id,
            outcome: PersonOutcome::Refused,
            url: Some(url),
            code: None,
            ..
        }] = sent_to(&out, "aa")[..]
        else {
            panic!("refused: {out:?}")
        };
        assert_eq!(
            (id.as_str(), url.as_str()),
            ("p1", "https://site.test/characters/")
        );
        let out = h.receive(&d, "aa", alts("p2", false), 2.0);
        assert_eq!(
            person_outcome(&out, "aa"),
            Some(("p2".into(), PersonOutcome::Ok, None))
        );
        assert_eq!(*d.asked.borrow(), ["alts Ann@aa true", "alts Ann@aa false"]);
    }

    /// Spec: link protocol: a resent person request is answered as the first was, and is not
    /// asked of the registry again.
    #[test]
    fn a_resent_person_request_is_answered_as_the_first_was() {
        let d = person_dir();
        let mut h = hub_with(&["aa"]);
        h.set_link_url(Some("https://site.test/characters/".into()));
        let first = h.receive(&d, "aa", person("p1", PersonOp::Code, "Ann@aa", None), 1.0);
        let again = h.receive(&d, "aa", person("p1", PersonOp::Code, "Ann@aa", None), 2.0);
        assert_eq!(person_outcome(&first, "aa"), person_outcome(&again, "aa"));
        assert_eq!(d.codes.get(), 1, "one code");
    }

    /// Spec: link protocol: person requests are held to the character's pace of tells.
    #[test]
    fn person_requests_are_held_to_the_characters_pace() {
        let d = person_dir();
        let mut h = hub_with(&["aa"]);
        let burst = 10;
        assert!((Limits::DEFAULT.tells_per_character.burst - 10.0).abs() < f64::EPSILON);
        for i in 0..burst {
            let out = h.receive(
                &d,
                "aa",
                person(
                    &format!("p{i}"),
                    PersonOp::Characters,
                    "Ann@aa",
                    Some("Bob@bb"),
                ),
                1.0,
            );
            assert_eq!(
                person_outcome(&out, "aa").map(|o| o.1),
                Some(PersonOutcome::Ok)
            );
        }
        let out = h.receive(
            &d,
            "aa",
            person("over", PersonOp::Befriend, "Ann@aa", Some("Bob@bb")),
            1.0,
        );
        assert_eq!(
            person_outcome(&out, "aa").map(|o| o.1),
            Some(PersonOutcome::RateLimited)
        );
        assert!(
            !d.asked.borrow().iter().any(|q| q.starts_with("befriend")),
            "nothing asked past the pace"
        );
        for i in 0..20 {
            let out = h.receive(
                &d,
                "aa",
                person(
                    &format!("u{i}"),
                    PersonOp::Unfriend,
                    "Ann@aa",
                    Some("Bob@bb"),
                ),
                1.0,
            );
            assert_eq!(
                person_outcome(&out, "aa").map(|o| o.1),
                Some(PersonOutcome::Ok),
                "an unfriend only takes away, and is not held to the pace"
            );
        }
        let out = h.receive(
            &d,
            "aa",
            person("other", PersonOp::Characters, "Cat@aa", Some("Bob@bb")),
            1.0,
        );
        assert_eq!(
            person_outcome(&out, "aa").map(|o| o.1),
            Some(PersonOutcome::Ok),
            "another character's pace is its own"
        );
    }

    /// Spec: link protocol: a `carry` goes to the person layer with every target, and an unlinked
    /// character's view naming the friends handed back to it is sent once: the hub forgets them
    /// as it sends it.
    #[test]
    fn carried_friends_reach_the_person_layer_and_handed_back_ones_are_sent_once() {
        let d = person_dir();
        let mut h = hub_with(&["aa", "bb"]);
        let carry = Message::Person {
            id: "c1".into(),
            op: PersonOp::Carry,
            who: a("Ann@aa"),
            target: None,
            targets: vec![a("Bob@bb"), a("Cat@bb")],
            shown: None,
        };
        let out = h.receive(&d, "aa", carry, 1.0);
        assert_eq!(
            person_outcome(&out, "aa").map(|(_, o, _)| o),
            Some(PersonOutcome::Ok)
        );
        assert!(d
            .asked
            .borrow()
            .contains(&"carry Ann@aa Bob@bb,Cat@bb".to_owned()));
        *d.view.borrow_mut() = PersonView {
            linked: false,
            alts: Alts::Hidden,
            persons: vec![PersonFriend {
                handle: a("Bob@bb"),
                characters: vec![a("Bob@bb")],
            }],
        };
        let watch = Message::PersonWatch {
            who: a("Ann@aa"),
            on: true,
        };
        let out = h.receive(&d, "aa", watch, 2.0);
        assert!(matches!(
            sent_to(&out, "aa")[..],
            [Message::PersonFriends { linked: false, persons, .. }] if persons.len() == 1
        ));
        assert!(d.asked.borrow().contains(&"returned Ann@aa".to_owned()));
        d.asked.borrow_mut().clear();
        *d.view.borrow_mut() = PersonView::default();
        h.receive(
            &d,
            "aa",
            Message::PersonWatch {
                who: a("Cat@aa"),
                on: true,
            },
            3.0,
        );
        assert!(
            !d.asked.borrow().iter().any(|x| x.starts_with("returned")),
            "a view handing nothing back forgets nothing"
        );
    }

    /// Spec: link protocol: a watched character's view goes at once, again only when a person
    /// changed and the view with it, only to its own world, and not after the world's link goes.
    #[test]
    fn a_watched_view_goes_at_once_and_again_when_it_changes() {
        let d = person_dir();
        let mut h = hub_with(&["aa", "bb"]);
        let watch = Message::PersonWatch {
            who: a("Ann@aa"),
            on: true,
        };
        let out = h.receive(&d, "aa", watch.clone(), 1.0);
        assert!(matches!(
            sent_to(&out, "aa")[..],
            [Message::PersonFriends { linked: false, .. }]
        ));
        h.persons_changed(&[a("Ann@aa")]);
        assert!(
            h.tick(&d, 2.0)
                .iter()
                .all(|o| !matches!(o, Out::Send(_, Message::PersonFriends { .. }))),
            "an unchanged view is not sent again"
        );
        *d.view.borrow_mut() = PersonView {
            linked: true,
            alts: Alts::Hidden,
            persons: vec![PersonFriend {
                handle: a("Bob@bb"),
                characters: vec![a("Bob@bb")],
            }],
        };
        assert!(
            h.tick(&d, 3.0)
                .iter()
                .all(|o| !matches!(o, Out::Send(_, Message::PersonFriends { .. }))),
            "nothing changed as far as the hub was told"
        );
        h.persons_changed(&[a("Ann@aa")]);
        let out = h.tick(&d, 4.0);
        assert!(matches!(
            sent_to(&out, "aa")[..],
            [Message::PersonFriends { linked: true, .. }]
        ));
        assert!(
            sent_to(&out, "bb").is_empty(),
            "only to the character's world"
        );
        h.disconnect("aa");
        *d.view.borrow_mut() = PersonView::default();
        h.persons_changed(&[a("Ann@aa")]);
        assert!(
            h.tick(&d, 6.0).is_empty(),
            "the link went, and its watches with it"
        );
        assert_eq!(h.person_watches("aa"), 0);
    }

    /// Spec: link protocol: the hub keeps at most 2,000 `person.watch`es for one world.
    #[test]
    fn a_person_watch_past_the_worlds_limit_is_not_answered() {
        let d = person_dir();
        let mut h = hub_with(&["aa"]);
        // Within the world's own pace of messages (50 a second).
        let mut t = 0.0;
        for i in 0..MAX_PERSON_WATCHES {
            t += 0.025;
            let who = format!("Ann {}@aa", letters(i));
            let out = h.receive(
                &d,
                "aa",
                Message::PersonWatch {
                    who: a(&who),
                    on: true,
                },
                t,
            );
            assert_eq!(sent_to(&out, "aa").len(), 1, "{who}");
        }
        let out = h.receive(
            &d,
            "aa",
            Message::PersonWatch {
                who: a("One More@aa"),
                on: true,
            },
            t + 1.0,
        );
        assert!(sent_to(&out, "aa").is_empty());
        assert_eq!(h.person_watches("aa"), MAX_PERSON_WATCHES);
    }

    /// A directory of persons by character: each character's view, the characters of each
    /// character's person, and every view it was asked for. Its `befriend` changes the asker's
    /// person; its `unfriend` and its `alts` change nothing, as removing a person friend not listed
    /// and hiding characters already hidden do.
    struct ViewsDir {
        worlds: Dir,
        views: std::cell::RefCell<Map<String, PersonView>>,
        persons: Map<String, Vec<Address>>,
        read: std::cell::RefCell<Vec<String>>,
    }

    impl Directory for ViewsDir {
        fn lookup(&self, slug: &str) -> Option<DirEntry> {
            self.worlds.lookup(slug)
        }
        fn person_befriend(&self, who: &Address, _: &Address) -> PersonAnswer {
            PersonAnswer {
                outcome: PersonOutcome::Ok,
                changed: self.persons.get(&who.key()).cloned().unwrap_or_default(),
            }
        }
        fn person_unfriend(&self, _: &Address, _: &Address) -> PersonAnswer {
            PersonAnswer::unchanged(PersonOutcome::Ok)
        }
        fn person_alts(&self, _: &Address, _: bool) -> PersonAnswer {
            PersonAnswer::unchanged(PersonOutcome::Ok)
        }
        fn person_view(&self, who: &Address) -> PersonView {
            self.read.borrow_mut().push(who.to_string());
            self.views
                .borrow()
                .get(&who.key())
                .cloned()
                .unwrap_or_default()
        }
    }

    /// A linked character's view listing `friends`, each as its handle and the characters named.
    fn view_of(friends: &[&[&str]]) -> PersonView {
        PersonView {
            linked: true,
            alts: Alts::Hidden,
            persons: friends
                .iter()
                .map(|names| PersonFriend {
                    handle: a(names[0]),
                    characters: names.iter().map(|n| a(n)).collect(),
                })
                .collect(),
        }
    }

    /// The views read since the last call, in order.
    fn read_since(d: &ViewsDir) -> Vec<String> {
        std::mem::take(&mut *d.read.borrow_mut())
    }

    fn views_sent(out: &[Out]) -> Vec<(String, String)> {
        out.iter()
            .filter_map(|o| match o {
                Out::Send(slug, Message::PersonFriends { to, .. }) => {
                    Some((slug.clone(), to.to_string()))
                }
                _ => None,
            })
            .collect()
    }

    /// Spec: hub: a person request that changes nothing has no view looked at again, and a
    /// person's change has only the views that are its characters' or name one of them looked at
    /// again, never another person's.
    #[test]
    fn a_change_looks_again_only_at_the_views_that_name_the_changed_persons_characters() {
        // Ann has characters on aa and bb; Bob on bb and cc; Cat on cc. Dan's view names Bob's
        // character on cc, as a character Bob@bb's handle lists.
        let d = ViewsDir {
            worlds: dir(),
            views: std::cell::RefCell::new(
                [
                    ("ann@aa", view_of(&[&["Eve@cc"]])),
                    ("bob@bb", view_of(&[])),
                    ("cat@cc", view_of(&[])),
                    ("dan@aa", view_of(&[&["Bob@bb", "Bob@cc"]])),
                ]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect(),
            ),
            persons: [
                ("ann@aa", vec![a("Ann@aa"), a("Ann@bb")]),
                ("bob@cc", vec![a("Bob@bb"), a("Bob@cc")]),
                ("cat@cc", vec![a("Cat@cc")]),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
            read: std::cell::RefCell::new(Vec::new()),
        };
        let mut h = hub_with(&["aa", "bb", "cc"]);
        for (slug, who) in [
            ("aa", "Ann@aa"),
            ("bb", "Bob@bb"),
            ("cc", "Cat@cc"),
            ("aa", "Dan@aa"),
        ] {
            let watch = Message::PersonWatch {
                who: a(who),
                on: true,
            };
            assert_eq!(views_sent(&h.receive(&d, slug, watch, 1.0)).len(), 1);
        }
        read_since(&d);
        // Answered `ok`, and nothing changed.
        let out = h.receive(
            &d,
            "aa",
            person("n1", PersonOp::Unfriend, "Ann@aa", Some("Zed@cc")),
            2.0,
        );
        assert_eq!(
            person_outcome(&out, "aa").map(|o| o.1),
            Some(PersonOutcome::Ok)
        );
        let hide = Message::Person {
            id: "n2".into(),
            op: PersonOp::Alts,
            who: a("Ann@aa"),
            target: None,
            targets: Vec::new(),
            shown: Some(false),
        };
        let out = h.receive(&d, "aa", hide, 2.0);
        assert_eq!(
            person_outcome(&out, "aa").map(|o| o.1),
            Some(PersonOutcome::Ok)
        );
        assert_eq!(h.views_due(), 0);
        assert!(views_sent(&h.tick(&d, 3.0)).is_empty());
        assert!(
            read_since(&d).is_empty(),
            "a request that changed nothing reads no view"
        );
        // Cat's person changes: Cat's view alone is read, and sent.
        d.views
            .borrow_mut()
            .insert("cat@cc".into(), view_of(&[&["Fay@aa"]]));
        let out = h.receive(
            &d,
            "cc",
            person("c1", PersonOp::Befriend, "Cat@cc", Some("Fay@aa")),
            4.0,
        );
        assert_eq!(
            person_outcome(&out, "cc").map(|o| o.1),
            Some(PersonOutcome::Ok)
        );
        let out = h.tick(&d, 5.0);
        assert_eq!(read_since(&d), ["Cat@cc"]);
        assert_eq!(views_sent(&out), [("cc".to_owned(), "Cat@cc".to_owned())]);
        // Bob's person changes from bb: Bob's view, and Dan's, which names Bob@cc, and no other.
        d.views
            .borrow_mut()
            .insert("dan@aa".into(), view_of(&[&["Bob@bb"]]));
        let out = h.receive(
            &d,
            "cc",
            person("b1", PersonOp::Befriend, "Bob@cc", Some("Gil@aa")),
            6.0,
        );
        assert_eq!(
            person_outcome(&out, "cc").map(|o| o.1),
            Some(PersonOutcome::Ok)
        );
        let out = h.tick(&d, 7.0);
        let mut read = read_since(&d);
        read.sort();
        assert_eq!(read, ["Bob@bb", "Dan@aa"]);
        assert_eq!(views_sent(&out), [("aa".to_owned(), "Dan@aa".to_owned())]);
        // Dan's view, as last sent, names Bob@bb and no longer Bob@cc.
        h.persons_changed(&[a("Bob@cc")]);
        h.tick(&d, 8.0);
        assert!(read_since(&d).is_empty(), "no view names Bob@cc now");
        h.persons_changed(&[a("Bob@bb"), a("Bob@cc")]);
        h.tick(&d, 9.0);
        let mut read = read_since(&d);
        read.sort();
        assert_eq!(read, ["Bob@bb", "Dan@aa"]);
        // A watch turned off is not looked at again.
        h.receive(
            &d,
            "aa",
            Message::PersonWatch {
                who: a("Dan@aa"),
                on: false,
            },
            9.5,
        );
        h.persons_changed(&[a("Bob@bb")]);
        h.tick(&d, 10.0);
        assert_eq!(read_since(&d), ["Bob@bb"]);
    }

    /// Spec: hub: the views one change reaches are looked at again at most a round's worth a
    /// second, in turn, each once however often it is reached; one whose watch has gone since
    /// counts nothing of a round.
    #[test]
    fn the_views_a_change_reaches_are_looked_at_again_a_rounds_worth_a_second() {
        let many = PERSON_VIEWS_PER_ROUND * 3;
        let d = ViewsDir {
            worlds: dir(),
            views: std::cell::RefCell::new(Map::new()),
            persons: Map::new(),
            read: std::cell::RefCell::new(Vec::new()),
        };
        let mut h = hub_with(&["aa"]);
        // Within the world's own pace of messages (50 a second).
        let mut t = 0.0;
        for i in 0..many {
            t += 0.025;
            let who = format!("Fan {}@aa", letters(i));
            d.views
                .borrow_mut()
                .insert(a(&who).key(), view_of(&[&["Bob@bb"]]));
            h.receive(
                &d,
                "aa",
                Message::PersonWatch {
                    who: a(&who),
                    on: true,
                },
                t,
            );
        }
        read_since(&d);
        for v in d.views.borrow_mut().values_mut() {
            *v = view_of(&[&["Bob@bb", "Bob@cc"]]);
        }
        h.persons_changed(&[a("Bob@bb"), a("Bob@cc")]);
        h.persons_changed(&[a("Bob@bb")]);
        assert_eq!(h.views_due(), many, "each once");
        // The first half round's watches in turn are turned off before the round.
        let mut turn: Vec<String> = (0..many)
            .map(|i| format!("Fan {}@aa", letters(i)))
            .collect();
        turn.sort_by_key(|w| a(w).key());
        let off = PERSON_VIEWS_PER_ROUND / 2;
        for who in &turn[..off] {
            t += 0.025;
            let watch = Message::PersonWatch {
                who: a(who),
                on: false,
            };
            h.receive(&d, "aa", watch, t);
        }
        t += 1.0;
        let out = h.tick(&d, t);
        let read = read_since(&d);
        assert_eq!(read, turn[off..off + PERSON_VIEWS_PER_ROUND], "in turn");
        assert_eq!(views_sent(&out).len(), PERSON_VIEWS_PER_ROUND);
        assert!(h.tick(&d, t + 0.5).is_empty(), "not within the second");
        assert!(read_since(&d).is_empty());
        h.tick(&d, t + 1.0);
        assert_eq!(read_since(&d).len(), PERSON_VIEWS_PER_ROUND);
        h.tick(&d, t + 2.0);
        assert_eq!(
            read_since(&d).len(),
            many - off - 2 * PERSON_VIEWS_PER_ROUND
        );
        assert_eq!(h.views_due(), 0);
        h.tick(&d, t + 3.0);
        assert!(read_since(&d).is_empty());
    }
}
