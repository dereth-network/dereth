//! The world's half of the Dereth Network's link, sans-IO: what an Asheron's Call 2 world says to
//! the hub and does with what the hub says, so that its players reach characters on other worlds,
//! of either game.
//!
//! **Depends on** the network's protocol (`dereth-network-protocol`: the link's messages,
//! addresses, the text rules and the rate limits) and the store (`yalain-store`: each character's
//! setting and the operator's blocks). **Used by** the server executable (`yalain`), which carries
//! the frames over the link and feeds the clock.
//!
//! **Must never** touch a socket, a file or the clock: the host hands it the link's events and the
//! time, in seconds on any monotonic scale, and takes the frames it has to send. It knows nothing
//! of the game: the friends service, which faces the players, answers what it asks and shows what
//! it delivers through [`Friends`].
//!
//! **Specified in** `docs/network/02-link-protocol.md` (the messages, the order of a link, who
//! answers what, the rate limits), `03-world-behaviour.md` (the rules a world keeps: sections 1, 2,
//! 5, 7, 8 and 10) and `04-text-rules.md`. It carries `hello`, `welcome`, `world`, `watch`,
//! `presence`, `tell`, `tell.result`, `error` and the directory's `listing`, `listing.result` and
//! `listing.off`, and skips every other family (rooms, channels, persons, reach, say), as a world
//! that does not carry a family may.
//!
//! - **On the network.** Every character starts off it; its player puts it on (and the setting is
//!   kept in the store). Only a character on the network is seen online by other worlds, may tell
//!   them and be told by them. Friends and tells within the world are the friends service's alone
//!   and need none of this.
//! - **Present.** A character is present while it is in the world with a friends stream: only
//!   then is it online to other worlds, and only then can a tell be shown to it.
//! - **Tells** ([`Federation::tell`]) go to the hub and end, once its answer comes, with the
//!   outcome and the line the teller is shown ([`TellEnd`]). A tell from another world is shown
//!   by the friends service ([`Friends::show_tell`]) and answered at once.
//! - **Presence.** While a character on the network is present, its world watches each friend
//!   it lists on another linked world; the friend's world answers, and each change reaches the
//!   character ([`Friends::show_presence`]). Another world's watch of one of this world's
//!   characters is answered by this world's own rules, and every change told.
//! - **The operator's blocks** ([`Federation::operator`]) refuse a world, or one character of it,
//!   for everyone on this world, both ways.
//! - **The directory** (`directory`): a character on the network is listed with its profile, which
//!   the world composes and hands in at login, while it plays and at logout; it is taken out when
//!   it goes off the network.
//!
//! The host calls [`Federation::on_link_up`], [`Federation::on_frame`],
//! [`Federation::on_link_down`], [`Federation::take_frames`], [`Federation::tick`] and
//! [`Federation::keep`]; a test drives the same calls on virtual time.

mod chatter;
pub mod directory;
mod operator;
mod social;
mod tells;

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use dereth_network_protocol::limits::{Bucket, Limits, Rate};
use dereth_network_protocol::wire::{WireError, PROTOCOL};
use dereth_network_protocol::{Address, DirectoryMessage, Message, WorldInfo};
use yalain_store::{Store, StoreResult};

use chatter::Chatter;
use directory::Directory;

pub use operator::HELP;

/// The most remote watchers one local character keeps from any one world, and from all worlds
/// together. A watcher the character lists as a friend is always kept.
pub const MAX_WATCHERS_PER_WORLD: usize = 100;
/// See [`MAX_WATCHERS_PER_WORLD`].
pub const MAX_WATCHERS: usize = 1_000;

/// The most watches and presence answers waiting for the link at once. They go after everything
/// else; a newer one for the same pair of characters takes the waiting one's place, and past this
/// the local character with the most waiting loses its oldest (a reconnect sends them again), so
/// no one character can push out everyone else's.
pub const MAX_CHATTER: usize = 2_000;

/// The most answers this world owes any one other world at once. A tell from a world already owed
/// this many is not shown and not answered (the hub answers it `unavailable`), so a world flooding
/// this one with tells delays only its own answers.
pub const MAX_ANSWERS_PER_WORLD: usize = 100;

/// How many state changes a character is shown for one friend on another world at once, then how
/// often: a few at once, then one every five seconds. A friend's world that changes its answer
/// faster is shown only its latest state, as the pace allows.
pub const PRESENCE_PACE: Rate = Rate {
    burst: 4.0,
    per_sec: 0.2,
};

/// The away text a character away gives a teller on another world when it has set none.
pub const AWAY_TEXT: &str = "Away from keyboard.";

/// The away text of a character not to be disturbed, when it has set none.
pub const DO_NOT_DISTURB_TEXT: &str = "Not to be disturbed.";

/// What a character off the network is told when it reaches for another world.
pub const NOT_ON: &str = "You are not on the Dereth Network. Say @dn on to join it.";

/// What a character is told when it reaches for another world from a world off the network.
pub const WORLD_OFF: &str = "This world is not on the Dereth Network.";

/// What a character sending faster than its own share of the hub's limits is told.
pub const TOO_QUICKLY: &str =
    "You are sending to the Dereth Network too quickly. Wait a moment and try again.";

/// What a teller is told when its tell waited too long without a link.
pub const UNAVAILABLE: &str = "The Dereth Network is unavailable.";

/// The key under which another world's watches are paced, beside the local characters it tells.
const WATCHES: &str = "@watch";

/// The world's settings for the network.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settings {
    /// The operator turned the network on for this world. Off, every call answers as if the
    /// network did not exist, and a tell to another world is refused.
    pub enabled: bool,
    /// The world's slug in the hub's registry: the world half of its characters' addresses.
    pub slug: String,
    /// What the world calls itself in its `hello` (its program and version).
    pub software: String,
}

/// A character of this world present with a friends stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Present {
    /// Its name, as the world writes it.
    pub name: String,
    pub state: State,
}

/// The state a present character chose for itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Available,
    /// Away, with its away text if it set one.
    Away(Option<String>),
    /// Not to be disturbed, with its text if it set one.
    DoNotDisturb(Option<String>),
}

/// The friends service, as the link's half sees it: who is present and what each lists and
/// ignores, and where what comes from other worlds is shown.
pub trait Friends {
    /// The character of this world of this name (any case), while it is present: in the world
    /// with a friends stream.
    fn present(&self, name: &str) -> Option<Present>;

    /// The name of every character present.
    fn everyone_present(&self) -> Vec<String>;

    /// The friends a character of this world lists, by their network addresses, whether or not
    /// it is present. Friends of this world's own may be among them; the link's half leaves them
    /// to the service.
    fn remote_friends(&self, character: &str) -> Vec<Address>;

    /// Whether a character of this world ignores `who`, a character of another world (or its
    /// whole world).
    fn ignores(&self, character: &str, who: &Address) -> bool;

    /// Show a present character a tell from another world. The sender and the text have passed
    /// the network's text rules.
    fn show_tell(&mut self, to: &str, from: &Address, text: &str);

    /// Show a present character the state of a friend on another world, named as it listed the
    /// friend. `news`: a change from a state it was already shown; otherwise the first state
    /// after the friend was added, or after its world (or the hub) came back.
    fn show_presence(&mut self, to: &str, friend: &Address, online: bool, news: bool);

    /// A tell a character sent another world has ended ([`Federation::tell`]).
    fn tell_ended(&mut self, tell: TellId, end: TellEnd);
}

/// The friends service's mark for one of its tells, handed back when it ends.
pub type TellId = u64;

/// How a tell to another world ended, and the line its teller is shown, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TellEnd {
    pub ended: Ended,
    /// The world's own line for the teller (the client shows nothing of its own for these).
    pub line: Option<String>,
}

/// How a tell to another world ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// Shown to the recipient.
    Delivered,
    /// Shown; the recipient is away (the line says so, with its text).
    Away,
    /// Not shown: the recipient ignores the teller. The teller is shown what a delivered tell
    /// shows.
    Squelched,
    /// Not shown: the recipient is not online, not on the network, does not exist, or its world
    /// or the operator refuses it, alike.
    Offline,
    /// Not shown: the recipient's world is on the network but cannot be reached now.
    Unavailable,
    /// Not shown: the recipient's world is not on the network.
    NotOnNetwork,
    /// Not sent or not shown: a limit the whole world shares held it back.
    RateLimited,
    /// Not sent: the teller is not on the network.
    NotOn,
    /// Not sent: this world is not on the network.
    WorldOff,
    /// Not sent: the teller sends faster than its own pace.
    TooQuickly,
    /// Not sent, or not answered: the link was down too long (the line says the network is
    /// unavailable), or no answer came in time (the line says the recipient's world cannot be
    /// reached).
    NetworkUnavailable,
}

impl Ended {
    /// Whether the receiving world took the tell: the teller's client is answered as for a
    /// delivered tell.
    #[must_use]
    pub fn reached(self) -> bool {
        matches!(self, Self::Delivered | Self::Away | Self::Squelched)
    }

    /// Whether a pace held the tell back: the teller is told to wait.
    #[must_use]
    pub fn busy(self) -> bool {
        matches!(self, Self::RateLimited | Self::TooQuickly)
    }
}

/// Where the link is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    /// No connection: tells wait for [`Limits::hold_secs`].
    Down,
    /// Connected; `hello` sent, `welcome` awaited.
    Greeting,
    /// Welcomed.
    Up,
}

/// A character on the network: its id in the store, and its name as the world writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OnNetwork {
    id: u64,
    name: String,
}

/// A queued message and when it was queued.
#[derive(Debug, Clone)]
struct Queued {
    at: f64,
    message: Message,
}

/// An answer this world owes another world, as its frame, and when it was owed.
#[derive(Debug, Clone)]
struct Owed {
    at: f64,
    frame: String,
}

/// A tell not yet answered.
#[derive(Debug, Clone)]
struct Pending {
    tell: TellId,
    to: Address,
    /// When it was handed to the link; none while it waits for one.
    sent: Option<f64>,
}

/// What a local character has been shown of one friend on another world, beside what the
/// friend's world last said. See [`PRESENCE_PACE`].
#[derive(Debug, Clone, Copy, PartialEq)]
struct Status {
    /// What the friend's world last said.
    online: bool,
    /// What the character was last shown; none until it has been shown this state's first report.
    shown: Option<bool>,
    /// What the character may still be shown now.
    pace: Bucket,
}

/// The network's state on the world.
#[derive(Debug)]
pub struct Federation {
    settings: Settings,
    /// The characters on the network, by their names in small letters.
    on: BTreeMap<String, OnNetwork>,
    /// The operator's blocks: world slugs, and identity keys of single characters.
    blocked: BTreeSet<String>,
    link: LinkState,
    /// The other federated worlds the hub has named, by slug.
    worlds: BTreeMap<String, WorldInfo>,
    limits: Limits,
    /// `hello`, while greeting.
    control: Vec<Message>,
    /// This world's own tells, in order.
    outbox: VecDeque<Queued>,
    /// The answers owed to each other world: sent after the outbox, a world at a time in turn.
    answers: BTreeMap<String, VecDeque<Owed>>,
    /// The world whose answer went last.
    answered: String,
    /// Watches and presence answers: sent after the answers.
    chatter: Chatter,
    /// Tells handed to the link and not yet answered, sent again first if the link goes before
    /// they reach the hub.
    in_flight: Vec<Message>,
    pending: HashMap<String, Pending>,
    /// What each friend's world last said to each local character that lists it, and what the
    /// character was shown, by the character's name in small letters and the friend's identity
    /// key; none while it is unknown.
    presence: HashMap<(String, String), Status>,
    /// Statuses whose latest state waits to be shown.
    presence_due: BTreeSet<(String, String)>,
    /// Remote watchers of each local character, by its name in small letters.
    watchers: HashMap<String, BTreeMap<String, Address>>,
    /// Each local character's own tells, paced as the hub paces them.
    sends: HashMap<String, Bucket>,
    /// What each other world may still bring, by world and what it brings (tells to one local
    /// character, or watches).
    received: HashMap<(String, String), Bucket>,
    /// The ids of this run start with this, so that a restart reuses none the hub remembers.
    run: String,
    next_id: u64,
    /// What the hub's per-world limit lets this world send now.
    pace: Option<Bucket>,
    /// The directory's listings of this world's characters.
    directory: Directory,
}

impl Federation {
    /// The network's state with nothing kept: every character off the network, nothing blocked.
    /// `unix` is the time the world starts, in seconds since 1970: the ids of its messages begin
    /// with it.
    #[must_use]
    pub fn new(settings: Settings, unix: i64) -> Self {
        Self {
            settings,
            on: BTreeMap::new(),
            blocked: BTreeSet::new(),
            link: LinkState::Down,
            worlds: BTreeMap::new(),
            limits: Limits::DEFAULT,
            control: Vec::new(),
            outbox: VecDeque::new(),
            answers: BTreeMap::new(),
            answered: String::new(),
            chatter: Chatter::default(),
            in_flight: Vec::new(),
            pending: HashMap::new(),
            presence: HashMap::new(),
            presence_due: BTreeSet::new(),
            watchers: HashMap::new(),
            sends: HashMap::new(),
            received: HashMap::new(),
            run: unix.to_string(),
            next_id: 1,
            pace: None,
            directory: Directory::default(),
        }
    }

    /// The network's state as the store keeps it: the characters on the network (those deleted
    /// left out), the operator's blocks, and the listings owed that take characters out of the
    /// directory.
    ///
    /// # Errors
    /// The store failed.
    pub fn load(settings: Settings, unix: i64, store: &mut impl Store) -> StoreResult<Self> {
        let mut f = Self::new(settings, unix);
        for id in store.network_characters()? {
            if let Some(c) = store.character(id)?.filter(|c| c.deleted_at.is_none()) {
                f.on.insert(c.name.to_lowercase(), OnNetwork { id, name: c.name });
            }
        }
        f.blocked = store.network_blocks()?.into_iter().collect();
        f.directory = Directory::load(store.world_record(directory::RECORD)?.as_deref());
        Ok(f)
    }

    /// Whether the network is on for this world.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.settings.enabled && !self.settings.slug.is_empty()
    }

    /// The world's settings.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Where the link is.
    #[must_use]
    pub fn link(&self) -> LinkState {
        self.link
    }

    /// Whether a character of this world (by name, any case) is on the network.
    #[must_use]
    pub fn is_on(&self, name: &str) -> bool {
        self.on.contains_key(&name.to_lowercase())
    }

    /// Whether the operator blocks `who` (the character, or its whole world).
    #[must_use]
    pub fn blocked(&self, who: &Address) -> bool {
        self.blocked.contains(who.world()) || self.blocked.contains(&who.key())
    }

    /// The operator's blocks, in order.
    #[must_use]
    pub fn blocks(&self) -> Vec<String> {
        self.blocked.iter().cloned().collect()
    }

    /// Whether `slug`'s link is up, as the hub last said.
    #[must_use]
    pub fn world_online(&self, slug: &str) -> bool {
        self.worlds.get(slug).is_some_and(|i| i.online)
    }

    /// How many other worlds' links are up, as the hub last said.
    #[must_use]
    pub fn worlds_online(&self) -> usize {
        self.worlds.values().filter(|i| i.online).count()
    }

    /// How many messages wait for the link (the directory's listings aside).
    #[must_use]
    pub fn queued(&self) -> usize {
        self.outbox.len() + self.answers_owed() + self.chatter.len()
    }

    /// How many answers this world owes other worlds, waiting for the link.
    #[must_use]
    pub fn answers_owed(&self) -> usize {
        self.answers.values().map(VecDeque::len).sum()
    }

    /// How many remote watchers a local character has.
    #[must_use]
    pub fn watcher_count(&self, name: &str) -> usize {
        self.watchers
            .get(&name.to_lowercase())
            .map_or(0, BTreeMap::len)
    }

    /// The state a local character was last shown for a friend on another world; none while it
    /// has been shown none.
    #[must_use]
    pub fn shown(&self, name: &str, friend: &Address) -> Option<bool> {
        self.presence
            .get(&(name.to_lowercase(), friend.key()))
            .and_then(|s| s.shown)
    }

    /// This world's address for one of its characters.
    fn own_address(&self, name: &str) -> Option<Address> {
        Address::new(name, &self.settings.slug).ok()
    }

    /// The name of a character on the network as the world writes it.
    fn on_name(&self, name: &str) -> Option<&str> {
        self.on.get(&name.to_lowercase()).map(|o| o.name.as_str())
    }

    fn fresh_id(&mut self, kind: char) -> String {
        let id = format!("{kind}{}-{}", self.run, self.next_id);
        self.next_id += 1;
        id
    }

    /// Queues a message for the hub: sent once the link is up, held for [`Limits::hold_secs`]
    /// while it is not.
    fn send(&mut self, t: f64, message: Message) {
        self.outbox.push_back(Queued { at: t, message });
    }

    /// Sends a watch or a presence answer, only while the link is up (a reconnect sends them
    /// again), after everything else waiting. Nothing goes to a world or character the operator
    /// blocks. Whether it went.
    fn send_if_up(&mut self, message: Message) -> bool {
        let to_blocked = match &message {
            Message::Watch { target, .. } => self.blocked(target),
            Message::Presence { to, .. } => self.blocked(to),
            _ => false,
        };
        !to_blocked && self.answer_if_up(message)
    }

    /// [`Federation::send_if_up`] for the answer to a watch, which goes to a blocked watcher too:
    /// it is the same `offline` any watch for a character the watcher may not see is answered
    /// with. Whether it went.
    fn answer_if_up(&mut self, message: Message) -> bool {
        let up = self.link == LinkState::Up;
        if up {
            self.chatter.push(message);
        }
        up
    }

    /// Whether this world may take on one more answer for `world` now.
    fn may_owe(&self, world: &str) -> bool {
        self.answers
            .get(world)
            .is_none_or(|q| q.len() < MAX_ANSWERS_PER_WORLD)
    }

    /// Queues an answer owed to `world`.
    fn owe(&mut self, t: f64, world: &str, message: &Message) {
        self.answers
            .entry(world.to_owned())
            .or_default()
            .push_back(Owed {
                at: t,
                frame: message.to_json(),
            });
    }

    /// The next answer owed, from the world after the one answered last.
    fn next_answer(&mut self) -> Option<String> {
        let world = self
            .answers
            .range::<str, _>((
                std::ops::Bound::Excluded(self.answered.as_str()),
                std::ops::Bound::Unbounded,
            ))
            .chain(self.answers.iter())
            .find(|(_, q)| !q.is_empty())
            .map(|(k, _)| k.clone())?;
        let q = self.answers.get_mut(&world)?;
        let m = q.pop_front().map(|o| o.frame);
        if q.is_empty() {
            self.answers.remove(&world);
        }
        self.answered = world;
        m
    }

    /// Whether another world is within the per-world `rate` for what it brings (`what`: a local
    /// character it tells, or its watches); takes one. The hub enforces the same limits, so this
    /// holds only against a hub that does not.
    fn may_receive(&mut self, t: f64, world: &str, what: &str, rate: Rate) -> bool {
        self.received
            .entry((world.to_owned(), what.to_owned()))
            .or_insert_with(|| Bucket::new(rate, t))
            .take(t)
    }

    // ---- the link -------------------------------------------------------------------------------

    /// The host connected to the hub: `hello` goes first, before anything queued.
    pub fn on_link_up(&mut self) {
        self.link = LinkState::Greeting;
        self.control = vec![Message::Hello {
            proto: PROTOCOL,
            software: self.settings.software.clone(),
            websocket_url: None,
            status_url: None,
        }];
        tracing::info!("Dereth Network: connected to the hub; greeting");
    }

    /// The host lost the hub: queued tells wait, and every friend on another world shows offline.
    /// What was handed to the link and not yet answered waits again at the front, since it may
    /// never have reached the hub (which drops a repeat of one that did).
    pub fn on_link_down(&mut self, friends: &mut impl Friends, t: f64) {
        if self.link == LinkState::Down {
            return;
        }
        self.link = LinkState::Down;
        self.control.clear();
        self.chatter.clear();
        self.directory_down();
        // The hub answers for this world every tell it had not answered when the link went.
        self.answers.clear();
        for m in std::mem::take(&mut self.in_flight).into_iter().rev() {
            if let Message::Tell { id, .. } = &m {
                if let Some(p) = self.pending.get_mut(id) {
                    p.sent = None;
                    self.outbox.push_front(Queued { at: t, message: m });
                }
            }
        }
        tracing::info!("Dereth Network: the link to the hub is down");
        let worlds: Vec<String> = self.worlds.keys().cloned().collect();
        for slug in worlds {
            self.world_gone(friends, &slug);
        }
        // Every other world watches again when this one is back.
        self.watchers.clear();
        self.presence.clear();
        self.presence_due.clear();
        for info in self.worlds.values_mut() {
            info.online = false;
        }
    }

    /// What to send now: `hello` while greeting; once welcomed, what is queued, as fast as the
    /// hub's per-world limit allows (the rest waits for the next call, so a burst of watches at a
    /// reconnect is never dropped at the hub). This world's own tells go first, then the answers
    /// it owes other worlds (a world at a time in turn), then watches and presence answers (a
    /// world at a time, each at most at its share of the pace), then the directory's listings
    /// (within the hub's paces for listings), so no number of the later can hold up the
    /// earlier.
    #[must_use]
    pub fn take_frames(&mut self, t: f64) -> Vec<String> {
        match self.link {
            LinkState::Down => Vec::new(),
            LinkState::Greeting => self.control.drain(..).map(|m| m.to_json()).collect(),
            LinkState::Up => {
                // A tenth under the hub's own numbers, so the two clocks never disagree at the
                // edge.
                let under = |r: Rate| Rate {
                    burst: (r.burst * 0.9).max(1.0),
                    per_sec: r.per_sec * 0.9,
                };
                let rate = under(self.limits.messages_per_world);
                let share = under(self.limits.watches_per_world_pair());
                let mut pace = *self.pace.get_or_insert_with(|| Bucket::new(rate, t));
                let mut out = Vec::new();
                while pace.has(t) {
                    if let Some(q) = self.outbox.pop_front() {
                        pace.take(t);
                        if let Message::Tell { id, .. } = &q.message {
                            if let Some(p) = self.pending.get_mut(id) {
                                p.sent = Some(t);
                            }
                            self.in_flight.push(q.message.clone());
                        }
                        out.push(q.message.to_json());
                        continue;
                    }
                    if let Some(frame) = self.next_answer() {
                        pace.take(t);
                        out.push(frame);
                        continue;
                    }
                    if let Some(m) = self.chatter.pop_ready(t, share) {
                        pace.take(t);
                        out.push(m.to_json());
                        continue;
                    }
                    if !self.listings_paced(t) {
                        break;
                    }
                    let Some(frame) = self.next_listing(t) else {
                        break;
                    };
                    pace.take(t);
                    out.push(frame);
                }
                self.pace = Some(pace);
                out
            }
        }
    }

    /// One frame from the hub. A message of a family this world does not carry, or of a type it
    /// does not know, is skipped; one that does not read is logged and skipped.
    pub fn on_frame(&mut self, friends: &mut impl Friends, t: f64, text: &str) {
        match Message::from_json(text) {
            Ok(m) => self.on_message(friends, t, m),
            Err(WireError::Unknown(_)) => match DirectoryMessage::from_json(text) {
                Ok(m) => self.on_directory(friends, t, m),
                Err(WireError::Unknown(_)) => {}
                Err(e) => tracing::debug!("Dereth Network: skipped a frame from the hub: {e}"),
            },
            Err(e) => tracing::debug!("Dereth Network: skipped a frame from the hub: {e}"),
        }
    }

    fn on_message(&mut self, friends: &mut impl Friends, t: f64, m: Message) {
        match m {
            Message::Welcome {
                world,
                worlds,
                limits,
                ..
            } => {
                if world != self.settings.slug {
                    tracing::warn!(
                        "Dereth Network: the hub knows this world as {world:?}, not {:?}",
                        self.settings.slug
                    );
                }
                self.link = LinkState::Up;
                self.limits = limits;
                self.pace = None;
                self.worlds = worlds
                    .into_iter()
                    .filter(|i| !i.slug.is_empty())
                    .map(|i| (i.slug.clone(), i))
                    .collect();
                tracing::info!(
                    "Dereth Network: welcomed; {} world(s) online",
                    self.worlds_online()
                );
                self.directory_welcomed(t);
                // What was queued goes first (it is in the outbox already); the watches of
                // everyone present, for the worlds online now, after. A world the hub names
                // later is watched when it does.
                let online: Vec<String> = self
                    .worlds
                    .values()
                    .filter(|i| i.online)
                    .map(|i| i.slug.clone())
                    .collect();
                for slug in online {
                    self.watch_everyone(friends, &slug);
                }
            }
            Message::World(info) => {
                let online = info.online;
                let slug = info.slug.clone();
                if slug.is_empty() || slug == self.settings.slug {
                    return;
                }
                let was = self.world_online(&slug);
                self.worlds.insert(slug.clone(), info);
                if online && !was {
                    self.watch_everyone(friends, &slug);
                } else if !online {
                    self.world_gone(friends, &slug);
                }
            }
            Message::Watch { owner, target, on } => self.on_watch(friends, t, owner, &target, on),
            Message::Presence { to, of, online } => self.on_presence(friends, t, &to, &of, online),
            Message::Tell {
                id, from, to, text, ..
            } => self.on_tell(friends, t, id, &from, &to, &text),
            Message::TellResult {
                id,
                outcome,
                message,
            } => self.on_result(friends, &id, outcome, message.as_deref()),
            Message::Error { code, message } => {
                tracing::warn!("Dereth Network: the hub reports {code}: {message}");
            }
            // Families this world does not carry, and messages only a world sends.
            _ => {}
        }
    }

    /// The passage of time: tells queued too long without a link, or not answered in time, are
    /// given up on; answers the hub has stopped waiting for are dropped; paces that have filled
    /// again are forgotten; friends' states waiting for their pace are shown.
    pub fn tick(&mut self, friends: &mut impl Friends, t: f64) {
        if !self.enabled() {
            return;
        }
        let hold = self.limits.hold_secs;
        let timeout = self.limits.result_timeout_secs;
        let mut expired = Vec::new();
        self.outbox.retain(|q| {
            let keep = t - q.at < hold;
            if !keep {
                expired.push(q.message.clone());
            }
            keep
        });
        for m in expired {
            if let Message::Tell { id, .. } = m {
                self.give_up(friends, &id);
            }
        }
        // An answer the hub has stopped waiting for is of no use to anyone.
        for q in self.answers.values_mut() {
            q.retain(|a| t - a.at < dereth_network_protocol::ANSWER_SECS);
        }
        self.answers.retain(|_, q| !q.is_empty());
        // A tell is given up on only once it has been with the hub longer than the hub may take
        // to answer it (its hold for a reconnecting world and the target's answer window
        // together).
        let late: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, p)| p.sent.is_some_and(|s| t - s >= timeout))
            .map(|(k, _)| k.clone())
            .collect();
        for id in late {
            self.give_up(friends, &id);
        }
        let pending = &self.pending;
        self.in_flight.retain(|m| match m {
            Message::Tell { id, .. } => pending.contains_key(id),
            _ => false,
        });
        self.sends.retain(|_, b| !b.full_at(t));
        self.received.retain(|_, b| !b.full_at(t));
        self.directory_settle(t);
        self.chatter.settle(t);
        self.show_due(friends, t);
    }
}
