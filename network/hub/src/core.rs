//! The hub's routing, as a state machine with no I/O: links in, messages to send out.
//!
//! [`Hub`] holds what the hub knows while it runs: which worlds have a link up, which rooms each
//! carries, the tells it is waiting on, the tells it holds for a world whose link is down, the
//! rate-limit buckets, the channels, and each world's `person.watch`es with the view it was last
//! sent. Persons themselves are the registry's ([`Directory`]): the routing asks it, and when a
//! person changes, looks again at the watched views that are its characters' or name one of them,
//! and sends each that changed. The directory's listings are the registry's too: the routing hands
//! each on at the hub's pace, and tells each world the listings its operator took out. A tell to a
//! linked character goes to its person's Discord too when its world says the character is not
//! online but reachable (`reach`) ([`Hub::take_discord_tells`]), and the person's reply comes back
//! as a tell from that character
//! ([`Hub::tell_from_discord`]). A line a person said in a mirror of a room from Discord goes to its
//! character's world, which says it as that character under its own rules, or refuses
//! ([`Hub::ask_say`]); its answer goes back to the person ([`Hub::take_say_answers`]). Each world
//! is told its rooms (where it carries the Global room, and its own rooms mirrored into Discord) at
//! its `welcome` and whenever they change, and may set the first itself ([`Hub::receive_rooms`]).
//! Every input names the time, in seconds on the caller's monotonic clock, and returns the
//! [`Out`]s to act on.
//! Nothing here keeps a message body past the moment it is handed on, except a tell held for a
//! world that is reconnecting (at most [`Limits::hold_secs`]) and a tell that may go to Discord,
//! until its world has answered whether it may (at most twice [`ANSWER_SECS`]), and nothing here
//! logs one.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use dereth_network_protocol::directory::{Refusal, LISTINGS_PER_CHARACTER, LISTINGS_PER_WORLD};
use dereth_network_protocol::limits::{Bucket, Limits, Rate};
use dereth_network_protocol::person::{
    within_view_limits, MAX_PERSON_CHARACTERS, MAX_PERSON_FRIENDS,
};
use dereth_network_protocol::rooms::OWN_ROOMS;
use dereth_network_protocol::web;
use dereth_network_protocol::wire::{ErrorCode, MAX_MESSAGE_BYTES, PROTOCOL};
use dereth_network_protocol::{
    Address, ChannelOp, ChannelOutcome, DirectoryMessage, Federation, ListingOutcome, Message,
    OffBy, Outcome, PersonFriend, PersonOp, PersonOutcome, Profile, ReachMessage, RoomsMessage,
    SayMessage, SayOutcome, WorldInfo,
};

use crate::bridge::{BridgeLine, Bridged};
use crate::dms::{Carry, DISCORD_WORLD};
use crate::rooms::GlobalSet;
use serde::{Deserialize, Serialize};

/// What the hub knows of a world from the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub federation: Federation,
}

/// One character's person view, as the hub sends it to the character's world
/// (`docs/network/02-link-protocol.md` section 6). The registry gives it whole, every person
/// friend in the order they were added; the routing cuts it to a view's limits ([`cut_view`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PersonView {
    pub linked: bool,
    pub persons: Vec<PersonFriend>,
}

/// What the routing knows of a character's presence on the network, while it runs: it decides
/// which person friends a view names when its person lists more than a view holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Presence {
    /// Online and on the network now: its world watches its view.
    Online,
    /// Last online this many seconds ago, or added as a person friend then.
    Seen(f64),
}

/// The person friends a view holds when its person lists more: those online first, then those
/// seen most lately, then those added most lately, each friend as present as the most present of
/// the characters it lists ([`Presence`], none for one not seen since the hub started). The ones
/// it holds keep the order they were added in, and the view is then cut to its limits as any view
/// is ([`within_view_limits`]).
#[must_use]
pub fn cut_view(view: PersonView, presence: impl Fn(&Address) -> Option<Presence>) -> PersonView {
    let PersonView { linked, persons } = view;
    if persons.len() <= MAX_PERSON_FRIENDS {
        return PersonView {
            linked,
            persons: within_view_limits(persons),
        };
    }
    // Lower ranks first: online; then seen, the latest first; then never seen, the latest added
    // first.
    let rank = |i: usize, p: &PersonFriend| -> (u8, f64, i64) {
        let best = p
            .characters
            .iter()
            .filter_map(&presence)
            .fold(None, |best, now| match (best, now) {
                (Some(Presence::Online), _) | (_, Presence::Online) => Some(Presence::Online),
                (Some(Presence::Seen(a)), Presence::Seen(b)) => Some(Presence::Seen(a.min(b))),
                (None, seen) => Some(seen),
            });
        let index = i64::try_from(i).unwrap_or(i64::MAX);
        match best {
            Some(Presence::Online) => (0, 0.0, index),
            Some(Presence::Seen(ago)) => (1, ago, index),
            None => (2, 0.0, -index),
        }
    };
    let mut ranked: Vec<((u8, f64, i64), usize)> = persons
        .iter()
        .enumerate()
        .map(|(i, p)| (rank(i, p), i))
        .collect();
    ranked.sort_by(|(a, _), (b, _)| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut kept: Vec<usize> = ranked
        .into_iter()
        .take(MAX_PERSON_FRIENDS)
        .map(|(_, i)| i)
        .collect();
    kept.sort_unstable();
    let mut persons: Vec<Option<PersonFriend>> = persons.into_iter().map(Some).collect();
    PersonView {
        linked,
        persons: within_view_limits(kept.into_iter().filter_map(|i| persons[i].take()).collect()),
    }
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

/// A listing's answer, as the registry gives it: how it ended, why it was refused, and the
/// character's page once it is listed; and, when the listing put a linked character on the network
/// or took it off (resuming or pausing its link), the characters of its person, whose views, and
/// every view naming one of them, may differ now. Those are the routing's alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingAnswer {
    pub outcome: ListingOutcome,
    pub reason: Option<Refusal>,
    pub url: Option<String>,
    pub changed: Vec<Address>,
}

impl ListingAnswer {
    /// The answer of a hub that keeps no directory.
    #[must_use]
    pub fn unavailable() -> Self {
        Self {
            outcome: ListingOutcome::Unavailable,
            reason: None,
            url: None,
            changed: Vec::new(),
        }
    }
}

/// The person of a linked character, whose Discord a tell to the character may reach, as the
/// routing asks the registry for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TellsPerson {
    /// The person, by the registry's id for it; no world is told it.
    pub person: String,
}

/// A tell for a person's Discord: the recipient's world said the recipient is not online and would
/// have been shown it, and the recipient is not online on the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordTell {
    pub person: String,
    pub from: Address,
    pub to: Address,
    pub text: String,
}

/// A line a person said in a mirror of a room from Discord, for its character's world to say:
/// `asker` is the I/O's own name for the person's answer, which the routing hands back with the
/// world's ([`SayAnswer`]), and never sends a world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SayAsk {
    pub asker: String,
    pub room: String,
    pub who: Address,
    pub text: String,
    /// Where the person said it (`discord`).
    pub via: String,
}

/// How a line asked from elsewhere ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    /// The world said it, and sent it on.
    Said,
    /// The world refused it, for the reason it gave, if it gave one.
    Refused(Option<String>),
    /// The world is not on the network, its link is down, or its link went before it answered.
    Unavailable,
    /// The world does not carry the room now.
    NotCarried,
    /// The world did not answer within [`SAY_ANSWER_SECS`].
    NoAnswer,
}

/// The answer to a [`SayAsk`], for its asker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SayAnswer {
    pub asker: String,
    pub who: Address,
    pub said: Said,
}

/// How a tell a person answered from Discord ended: `from` is the character the person answered
/// as, `to` the character it went to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordAnswer {
    pub person: String,
    pub from: Address,
    pub to: Address,
    pub outcome: Outcome,
    pub message: Option<String>,
}

/// The registry, as the routing reads it, and the person layer and the directory, as the routing
/// asks them. A directory that keeps no persons and no listings (the defaults) answers as a hub
/// that links no character and publishes none.
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

    /// The answer to a world of the network's first version that asks to hide or show `who`'s
    /// person's characters: it changes nothing.
    fn person_alts(&self, who: &Address) -> PersonAnswer {
        let _ = who;
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

    /// The characters of `target`'s person; none for a character nobody linked.
    fn person_characters(&self, target: &Address) -> Vec<Address> {
        let _ = target;
        Vec::new()
    }

    /// `who`'s person view, whole: every person friend, in the order they were added. The routing
    /// cuts it to a view's limits ([`cut_view`]).
    fn person_view(&self, who: &Address) -> PersonView {
        let _ = who;
        PersonView::default()
    }

    /// `who`'s world lists it with `profile`, or takes it out of the directory (`None`).
    fn listing(&self, who: &Address, profile: Option<&Profile>) -> ListingAnswer {
        let _ = (who, profile);
        ListingAnswer::unavailable()
    }

    /// The characters of `slug` the hub's operator took out of the directory whose world has not yet
    /// said it has heard, and who took each out.
    fn owed_offs(&self, slug: &str) -> Vec<(Address, OffBy)> {
        let _ = slug;
        Vec::new()
    }

    /// Listings no world has sent for the directory's keeping time, and holds past their time, go.
    fn lapse_listings(&self) {}

    /// The person of `who` while it links `who`, on a world on the network, its account not
    /// disabled; none otherwise.
    fn discord_tells(&self, who: &Address) -> Option<TellsPerson> {
        let _ = who;
        None
    }

    /// Where `slug` carries the Global room: one of its own rooms, or `off`; none while the hub
    /// holds none for it.
    fn global_room(&self, slug: &str) -> Option<String> {
        let _ = slug;
        None
    }

    /// `slug` carries the Global room in `room` (one of its own rooms, or `off`) from now on, as
    /// the world itself asks; with `seed`, only while the hub holds none for it. What changed.
    fn set_global_room(&self, slug: &str, room: &str, seed: bool) -> GlobalSet {
        let _ = (slug, room, seed);
        GlobalSet::default()
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
    /// Where a browser reaches a world, as its `hello` named it, for the registry: each address
    /// only within the rule of [`dereth_network_protocol::web`], and none where the `hello` named
    /// none.
    WebAddresses {
        slug: String,
        websocket_url: Option<String>,
        status_url: Option<String>,
    },
}

/// The rooms the hub carries.
pub const ROOMS: &[&str] = &[dereth_network_protocol::wire::GLOBAL_ROOM];

/// The most rooms of its own one world's link may carry beside the network's: the rooms the hub
/// mirrors into Discord for that world alone.
pub const MAX_OWN_ROOMS: usize = 10;

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

/// How long the hub waits for a world's answer to a line it asked it to say, in seconds; past that
/// the person is told the world did not answer.
pub const SAY_ANSWER_SECS: f64 = 10.0;

/// The most `person.watch`es the hub keeps for one world.
pub const MAX_PERSON_WATCHES: usize = 2_000;

/// How long the routing remembers when a character was last online, in seconds of its running
/// time: in memory alone, to order a view whose person lists more person friends than it holds.
pub const SEEN_SECS: f64 = 30.0 * 24.0 * 3600.0;

/// The most characters the routing remembers when it last saw online; past it, the longest ago
/// are forgotten first.
pub const MAX_SEEN: usize = 100_000;

/// The least time, in seconds, between two rounds of sending the watched characters' views again
/// after a person changed.
pub const PERSON_VIEWS_EVERY: f64 = 1.0;

/// The most watched views one round looks at again; the rest wait for the next round, in the
/// order their persons changed. Each is read from the registry while the routing holds it, so this
/// bounds how long a round holds it, however many views a change reaches.
pub const PERSON_VIEWS_PER_ROUND: usize = 100;

/// Listings from every world together the hub takes: each is a write to the registry, so the
/// worlds together never hold the routing up with them.
pub const LISTINGS_PER_HUB: Rate = Rate {
    burst: 200.0,
    per_sec: 100.0,
};

/// How often the routing asks the registry for the listings each linked world owes turning off,
/// in seconds: those its hub's operator or a person made since, on the site or the command line.
pub const OFFS_EVERY: f64 = 5.0;

/// How often listings past their keeping time are let go, in seconds.
pub const LAPSE_EVERY: f64 = 3_600.0;

/// How often one world may set where it carries the Global room (its `rooms.global`, a seed
/// among them): each change is a registry event, so a world asking without end never fills the
/// registry. One past it changes nothing, and is answered with the world's rooms as they are.
pub const GLOBAL_ROOM_CHANGES: Rate = Rate {
    burst: 5.0,
    per_sec: 0.1,
};

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

/// A tell forwarded to its target world, waiting for the answer. `origin` is [`DISCORD_WORLD`]
/// for a tell a person answered from Discord.
#[derive(Debug, Clone)]
struct Waiting {
    origin: String,
    origin_id: String,
    target: String,
    until: f64,
    /// What goes to the recipient's person on Discord, should its world answer `offline`.
    discord: Option<Box<ForDiscord>>,
}

/// A tell to a linked character, which may reach its person's Discord.
#[derive(Debug, Clone)]
struct ForDiscord {
    from: Address,
    to: Address,
    text: String,
    person: TellsPerson,
}

/// A tell its world answered `offline`, whose world is asked whether its recipient is reachable.
#[derive(Debug, Clone)]
struct Asking {
    world: String,
    tell: ForDiscord,
    until: f64,
}

/// A tell a person answered from Discord, waiting for the answer of the world it went to.
#[derive(Debug, Clone)]
struct Reply {
    person: String,
    from: Address,
    to: Address,
}

/// A line a world was asked to say, waiting for its answer.
#[derive(Debug, Clone)]
struct Saying {
    world: String,
    asker: String,
    who: Address,
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
    /// The watches whose person lists more person friends than a view holds, by the identity key
    /// of every character those friends list, named in the view or not: a change of one's presence
    /// may change which friends the view holds.
    watches_weighing: HashMap<String, BTreeSet<(String, String)>>,
    /// What each of those watches weighed, the keys it is filed under there.
    weighed: HashMap<(String, String), BTreeSet<String>>,
    /// When each character was last online on the network, or added as a person friend, in this
    /// run's time, while it is not online now; at most [`MAX_SEEN`], none past [`SEEN_SECS`].
    last_seen: HashMap<String, f64>,
    /// The latest time the routing was handed, for what happens without one (a link that goes).
    clock: f64,
    /// The watches to look at again, in the order their persons changed, each once.
    views_due: VecDeque<(String, String)>,
    /// See [`Hub::views_due`].
    views_due_set: HashSet<(String, String)>,
    /// When the next round of those may run.
    next_person_views: f64,
    /// Listings by character.
    listing_buckets: HashMap<String, Bucket>,
    /// Listings by world, all its characters together.
    listing_world_buckets: HashMap<String, Bucket>,
    /// Listings from every world together ([`LISTINGS_PER_HUB`]).
    listings_bucket: Bucket,
    /// The `listing.off`s handed to each world's current link, by world and the character's
    /// identity key: each goes once a link until the world says the character is off.
    offs_sent: HashSet<(String, String)>,
    /// The directory's messages to send, by world, in order ([`Hub::take_listings`]).
    listings_out: Vec<(String, DirectoryMessage)>,
    /// When the routing next asks for the listings owed turning off.
    next_offs: f64,
    /// When listings past their time are next let go.
    next_lapse: f64,
    /// The rooms, channels and worlds' own rooms mirrored into Discord.
    bridged: BTreeSet<Bridged>,
    /// The lines for Discord since the last call ([`Hub::take_bridged`]), in order.
    bridge_out: Vec<BridgeLine>,
    /// Whether this hub carries tells to persons' Discord ([`Hub::set_discord_tells`]).
    discord_tells: bool,
    /// The tells whose world is asked whether their recipient is reachable, by the question's id.
    asking: HashMap<String, Asking>,
    /// The reach messages to send, by world, in order ([`Hub::take_reach`]).
    reach_out: Vec<(String, ReachMessage)>,
    /// The tells for persons' Discord since the last call ([`Hub::take_discord_tells`]).
    discord_out: Vec<DiscordTell>,
    /// The answers to tells persons answered from Discord ([`Hub::take_discord_answers`]).
    discord_answers: Vec<DiscordAnswer>,
    /// The tells persons answered from Discord, waiting for an answer, by the id the hub gave them.
    replies: HashMap<String, Reply>,
    /// The lines worlds were asked to say, waiting for their answers, by the id the hub gave them.
    saying: HashMap<String, Saying>,
    /// The say messages to send, by world, in order ([`Hub::take_say`]).
    say_out: Vec<(String, SayMessage)>,
    /// The answers to lines said from elsewhere ([`Hub::take_say_answers`]).
    say_answers: Vec<SayAnswer>,
    /// The rooms each world's link was last told: where it carries the Global room, and its own
    /// rooms mirrored for it.
    setups: HashMap<String, (Option<String>, Vec<String>)>,
    /// The worlds' rooms may have changed: each linked world is told its rooms, where they differ
    /// from what it was last told, at the next tick.
    setups_due: bool,
    /// The rooms messages to send, by world, in order ([`Hub::take_rooms`]).
    rooms_out: Vec<(String, RoomsMessage)>,
    /// A world's mirrors changed with where it carries the Global room
    /// ([`Hub::take_mirrors_changed`]).
    mirrors_changed: bool,
    /// Changes of where each world carries the Global room ([`GLOBAL_ROOM_CHANGES`]).
    global_buckets: HashMap<String, Bucket>,
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
            watches_weighing: HashMap::new(),
            weighed: HashMap::new(),
            last_seen: HashMap::new(),
            clock: 0.0,
            views_due: VecDeque::new(),
            views_due_set: HashSet::new(),
            next_person_views: 0.0,
            listing_buckets: HashMap::new(),
            listing_world_buckets: HashMap::new(),
            listings_bucket: Bucket::new(LISTINGS_PER_HUB, 0.0),
            offs_sent: HashSet::new(),
            listings_out: Vec::new(),
            next_offs: 0.0,
            next_lapse: 0.0,
            bridged: BTreeSet::new(),
            bridge_out: Vec::new(),
            discord_tells: false,
            asking: HashMap::new(),
            reach_out: Vec::new(),
            discord_out: Vec::new(),
            discord_answers: Vec::new(),
            replies: HashMap::new(),
            saying: HashMap::new(),
            say_out: Vec::new(),
            say_answers: Vec::new(),
            setups: HashMap::new(),
            setups_due: false,
            rooms_out: Vec::new(),
            mirrors_changed: false,
            global_buckets: HashMap::new(),
        }
    }

    /// Sets the rooms mirrored into Discord: from now on a line said in one of them goes to its
    /// mirrors too ([`Hub::take_bridged`]), and each world whose own rooms mirrored changed is
    /// told so at the next tick.
    pub fn set_bridged(&mut self, rooms: BTreeSet<Bridged>) {
        self.bridged = rooms;
        self.setups_due = true;
    }

    /// Where a world carries the Global room changed outside the routing (its operator set it on
    /// the site): each linked world is told its rooms, where they differ from what it was last
    /// told, at the next tick.
    pub fn rooms_changed(&mut self) {
        self.setups_due = true;
    }

    /// The rooms messages to send since the last call, each with its world, in order: a family of
    /// their own, kept apart from [`Out`] as the say messages are.
    pub fn take_rooms(&mut self) -> Vec<(String, RoomsMessage)> {
        std::mem::take(&mut self.rooms_out)
    }

    /// Whether a world's mirrors changed with where it carries the Global room since the last
    /// call: the I/O reads the mirrors again.
    pub fn take_mirrors_changed(&mut self) -> bool {
        std::mem::take(&mut self.mirrors_changed)
    }

    /// `slug`'s rooms as the hub holds them now: where it carries the Global room (none while the
    /// hub holds none), and its own rooms mirrored for it, never the one carrying the Global room.
    fn setup_of(&self, dir: &dyn Directory, slug: &str) -> (Option<String>, Vec<String>) {
        let global = dir.global_room(slug);
        let mirrored = OWN_ROOMS
            .iter()
            .filter(|room| global.as_deref() != Some(**room))
            .filter(|room| {
                self.bridged.contains(&Bridged::World {
                    slug: slug.to_owned(),
                    room: (**room).to_owned(),
                })
            })
            .map(|room| (*room).to_owned())
            .collect();
        (global, mirrored)
    }

    /// Tells `slug` its rooms: in answer to its `rooms.global` (`id`), or else when they differ
    /// from what its link was last told.
    fn tell_rooms(&mut self, dir: &dyn Directory, slug: &str, id: Option<String>) {
        let setup = self.setup_of(dir, slug);
        if id.is_none() && self.setups.get(slug) == Some(&setup) {
            return;
        }
        self.rooms_out.push((
            slug.to_owned(),
            RoomsMessage::Setup {
                id,
                global: setup.0.clone(),
                mirrored: setup.1.clone(),
            },
        ));
        self.setups.insert(slug.to_owned(), setup);
    }

    /// One of the rooms messages from `slug`'s link: the world asks to carry the Global room in one
    /// of its own rooms, or nowhere, or seeds it with the room it carried it in before (taken only
    /// while the hub holds none for it). It is held to the world's messages, then to
    /// [`GLOBAL_ROOM_CHANGES`], and answered with the world's rooms as they are after it
    /// ([`Hub::take_rooms`]); `rooms` from a world is refused, as a message only the hub sends.
    /// What this returns is the link's own (an error, a close).
    pub fn receive_rooms(
        &mut self,
        dir: &dyn Directory,
        slug: &str,
        m: RoomsMessage,
        now: f64,
    ) -> Vec<Out> {
        let mut out = Vec::new();
        let Some(link) = self.links.get(slug) else {
            return out;
        };
        if !link.greeted {
            self.links.remove(slug);
            out.push(Self::error(slug, ErrorCode::Order, "hello first"));
            out.push(Out::Close(slug.to_owned(), "spoke before hello"));
            return out;
        }
        let RoomsMessage::Global { id, room, seed } = m else {
            out.push(Self::error(
                slug,
                ErrorCode::Malformed,
                "a message only the hub sends",
            ));
            return out;
        };
        let world_rate = self.limits.messages_per_world;
        if !self
            .world_buckets
            .entry(slug.to_owned())
            .or_insert_with(|| Bucket::new(world_rate, now))
            .take(now)
        {
            return out;
        }
        if self
            .global_buckets
            .entry(slug.to_owned())
            .or_insert_with(|| Bucket::new(GLOBAL_ROOM_CHANGES, now))
            .take(now)
        {
            let set = dir.set_global_room(slug, &room, seed);
            self.mirrors_changed |= set.mirrors;
        }
        self.tell_rooms(dir, slug, Some(id));
        out
    }

    /// The lines for Discord since the last call, each with the room it was said in, in order.
    pub fn take_bridged(&mut self) -> Vec<BridgeLine> {
        std::mem::take(&mut self.bridge_out)
    }

    /// Sets whether this hub carries tells to persons' Discord: from now on a tell to a linked
    /// character, answered `offline`, has its world asked `reach`.
    pub fn set_discord_tells(&mut self, on: bool) {
        self.discord_tells = on;
    }

    /// The reach messages to send since the last call, each with its world, in order: a family of
    /// their own, kept apart from [`Out`] as the directory's are.
    pub fn take_reach(&mut self) -> Vec<(String, ReachMessage)> {
        std::mem::take(&mut self.reach_out)
    }

    /// The tells for persons' Discord since the last call, in order.
    pub fn take_discord_tells(&mut self) -> Vec<DiscordTell> {
        std::mem::take(&mut self.discord_out)
    }

    /// The answers to tells persons answered from Discord since the last call, in order.
    pub fn take_discord_answers(&mut self) -> Vec<DiscordAnswer> {
        std::mem::take(&mut self.discord_answers)
    }

    /// How many tells wait for their world to say whether their recipient is reachable.
    #[must_use]
    pub fn asking_count(&self) -> usize {
        self.asking.len()
    }

    /// The answer to a tell for whoever sent it: its world's link, or, for one a person answered
    /// from Discord, the person ([`Hub::take_discord_answers`]).
    fn answer_origin(
        &mut self,
        origin: String,
        id: String,
        outcome: Outcome,
        message: Option<String>,
    ) -> Option<Out> {
        if origin != DISCORD_WORLD {
            return Some(Out::Send(
                origin,
                Message::TellResult {
                    id,
                    outcome,
                    message,
                },
            ));
        }
        if let Some(r) = self.replies.remove(&id) {
            self.discord_answers.push(DiscordAnswer {
                person: r.person,
                from: r.from,
                to: r.to,
                outcome,
                message,
            });
        }
        None
    }

    /// Whether `who` is online on the network: its world watches its person view (`person.watch`,
    /// which a world sends for each of its characters online and on the network).
    fn plays(&self, who: &Address) -> bool {
        self.person_watches
            .contains_key(&(who.world().to_owned(), who.key()))
    }

    /// One of the reach messages from `slug`'s link: the world's answer to whether a tell's
    /// recipient, not online, is reachable. A tell whose world says so goes to the person's
    /// Discord, unless the recipient is online now. The answer
    /// counts against the world's messages, and one over them goes nowhere; a question a world
    /// sends is refused, as a message only the hub sends. What this returns is the link's own (an
    /// error, a close).
    pub fn receive_reach(&mut self, slug: &str, m: ReachMessage, now: f64) -> Vec<Out> {
        let mut out = Vec::new();
        let Some(link) = self.links.get(slug) else {
            return out;
        };
        if !link.greeted {
            self.links.remove(slug);
            out.push(Self::error(slug, ErrorCode::Order, "hello first"));
            out.push(Out::Close(slug.to_owned(), "spoke before hello"));
            return out;
        }
        let ReachMessage::Result { id, reachable } = m else {
            out.push(Self::error(
                slug,
                ErrorCode::Malformed,
                "a message only the hub sends",
            ));
            return out;
        };
        let world_rate = self.limits.messages_per_world;
        if !self
            .world_buckets
            .entry(slug.to_owned())
            .or_insert_with(|| Bucket::new(world_rate, now))
            .take(now)
        {
            return out;
        }
        if !self.asking.get(&id).is_some_and(|a| a.world == slug) {
            return out;
        }
        let Some(asked) = self.asking.remove(&id) else {
            return out;
        };
        if reachable && !self.plays(&asked.tell.to) {
            let t = asked.tell;
            self.discord_out.push(DiscordTell {
                person: t.person.person,
                from: t.from,
                to: t.to,
                text: t.text,
            });
        }
        out
    }

    /// A person answered from Discord, as its character `from`, a tell `to` sent `from` that went
    /// to the person's Discord: it goes to `to`'s world as a tell from `from`, as one `from`'s
    /// world would send, held to the paces that world's tells are held to (`from`'s own, the pair
    /// of worlds', the recipient's from that world, the loop guard), with `friend` true when a
    /// person friend of `from`'s person lists `to`, for worlds of the network's first version, which
    /// read it. It is not carried unless `from` is still the person's. Its answer, and a refusal
    /// here, go to the person ([`Hub::take_discord_answers`]).
    pub fn tell_from_discord(
        &mut self,
        dir: &dyn Directory,
        person: &str,
        from: &Address,
        to: &Address,
        text: &str,
        now: f64,
    ) -> (Vec<Out>, Carry) {
        let mut out = Vec::new();
        if !self.discord_tells
            || from.is_channel()
            || to.is_channel()
            || !dir.discord_tells(from).is_some_and(|p| p.person == person)
        {
            return (out, Carry::NotCarried);
        }
        let refused = |outcome| DiscordAnswer {
            person: person.to_owned(),
            from: from.clone(),
            to: to.clone(),
            outcome,
            message: None,
        };
        if !Self::federated(dir, from.world()) || !Self::federated(dir, to.world()) {
            self.discord_answers.push(refused(Outcome::NotOnNetwork));
            return (out, Carry::NotCarried);
        }
        let tell_rate = self.limits.tells_per_character;
        let within = self
            .tell_buckets
            .entry(from.key())
            .or_insert_with(|| Bucket::new(tell_rate, now))
            .take(now)
            && self.within_tell_paces(from.world(), to, now)
            && self.pair_allows(from, to, now);
        if !within {
            self.discord_answers.push(refused(Outcome::RateLimited));
            return (out, Carry::HeldBack);
        }
        let friend = person_friend(dir, from, to);
        let id = self.fresh_id();
        self.replies.insert(
            id.clone(),
            Reply {
                person: person.to_owned(),
                from: from.clone(),
                to: to.clone(),
            },
        );
        let tell = Message::Tell {
            id,
            from: from.clone(),
            to: to.clone(),
            text: text.to_owned(),
            friend,
        };
        let target = to.world().to_owned();
        if self.is_greeted(&target) {
            out.extend(self.forward_tell(dir, DISCORD_WORLD.to_owned(), tell, now));
        } else {
            self.held.push(Held {
                origin: DISCORD_WORLD.to_owned(),
                tell,
                target,
                until: now + self.limits.hold_secs,
            });
        }
        (out, Carry::Carried)
    }

    /// A person said a line in a mirror of a room from elsewhere, as its character `ask.who`: the
    /// character's world is asked to say it (`say`), when it is on the network, linked and
    /// carrying the room; otherwise the answer is at once. The answer, the world's or the hub's,
    /// waits in [`Hub::take_say_answers`]. The world's checks are its own: which of its characters
    /// may speak, and in which rooms; the hub's are the person's, made before it asks.
    pub fn ask_say(&mut self, dir: &dyn Directory, ask: SayAsk, now: f64) {
        let world = ask.who.world().to_owned();
        let answer = |said| SayAnswer {
            asker: ask.asker.clone(),
            who: ask.who.clone(),
            said,
        };
        if ask.who.is_channel() || !Self::federated(dir, &world) || !self.is_greeted(&world) {
            self.say_answers.push(answer(Said::Unavailable));
            return;
        }
        if !self
            .links
            .get(&world)
            .is_some_and(|l| l.rooms.contains(&ask.room))
        {
            self.say_answers.push(answer(Said::NotCarried));
            return;
        }
        let id = self.fresh_id();
        self.say_out.push((
            world.clone(),
            SayMessage::Ask {
                id: id.clone(),
                room: ask.room,
                who: ask.who.clone(),
                text: ask.text,
                via: ask.via,
            },
        ));
        self.saying.insert(
            id,
            Saying {
                world,
                asker: ask.asker,
                who: ask.who,
                until: now + SAY_ANSWER_SECS,
            },
        );
    }

    /// One of the say messages from `slug`'s link: the world's answer to a line it was asked to
    /// say, for the person who said it. The answer counts against the world's messages, and one
    /// over them goes nowhere; a `say` a world sends is refused, as a message only the hub sends.
    /// What this returns is the link's own (an error, a close).
    pub fn receive_say(&mut self, slug: &str, m: SayMessage, now: f64) -> Vec<Out> {
        let mut out = Vec::new();
        let Some(link) = self.links.get(slug) else {
            return out;
        };
        if !link.greeted {
            self.links.remove(slug);
            out.push(Self::error(slug, ErrorCode::Order, "hello first"));
            out.push(Out::Close(slug.to_owned(), "spoke before hello"));
            return out;
        }
        let SayMessage::Result {
            id,
            outcome,
            reason,
        } = m
        else {
            out.push(Self::error(
                slug,
                ErrorCode::Malformed,
                "a message only the hub sends",
            ));
            return out;
        };
        let world_rate = self.limits.messages_per_world;
        if !self
            .world_buckets
            .entry(slug.to_owned())
            .or_insert_with(|| Bucket::new(world_rate, now))
            .take(now)
        {
            return out;
        }
        if !self.saying.get(&id).is_some_and(|s| s.world == slug) {
            return out;
        }
        let Some(s) = self.saying.remove(&id) else {
            return out;
        };
        self.say_answers.push(SayAnswer {
            asker: s.asker,
            who: s.who,
            said: match outcome {
                SayOutcome::Said => Said::Said,
                SayOutcome::Refused => Said::Refused(reason),
            },
        });
        out
    }

    /// The say messages to send since the last call, each with its world, in order: a family of
    /// their own, kept apart from [`Out`] as the reach messages are.
    pub fn take_say(&mut self) -> Vec<(String, SayMessage)> {
        std::mem::take(&mut self.say_out)
    }

    /// The answers to lines said from elsewhere since the last call, in order.
    pub fn take_say_answers(&mut self) -> Vec<SayAnswer> {
        std::mem::take(&mut self.say_answers)
    }

    /// How many lines wait for their world's answer.
    #[must_use]
    pub fn saying_count(&self) -> usize {
        self.saying.len()
    }

    /// Sets the page that links a character with a code (the site's `/link/`), which the link a
    /// code request is answered with opens; a hub without one links no character, and answers a
    /// code request `unavailable`.
    pub fn set_link_url(&mut self, url: Option<String>) {
        self.link_url = url.filter(|u| dereth_network_protocol::person::is_page_url(u));
    }

    /// Sets the person's own page on the site (`/characters/`), which the answer to a world of the
    /// first version asking to hide or show a person's characters names.
    pub fn set_person_url(&mut self, url: Option<String>) {
        self.person_url = url.filter(|u| dereth_network_protocol::person::is_page_url(u));
    }

    /// The persons of `characters` changed (a character linked or unlinked, a setting, a person
    /// friend), and `characters` are all of theirs, before and after the change: the watched views
    /// of those characters, and those that name one of them, are looked at again from the next
    /// tick, and sent where they changed. No other view can have changed, so no other is read.
    pub fn persons_changed(&mut self, characters: &[Address]) {
        for c in characters {
            let key = c.key();
            for index in [&self.watches_naming, &self.watches_weighing] {
                for w in index.get(&key).into_iter().flatten() {
                    if self.views_due_set.insert(w.clone()) {
                        self.views_due.push_back(w.clone());
                    }
                }
            }
        }
    }

    /// The character `key` came online on the network or left it (or was added as a person
    /// friend): the views of persons listing more person friends than a view holds, among whose
    /// friends it is, are looked at again, since which friends they hold may change.
    fn presence_changed(&mut self, key: &str) {
        for w in self.watches_weighing.get(key).into_iter().flatten() {
            if self.views_due_set.insert(w.clone()) {
                self.views_due.push_back(w.clone());
            }
        }
    }

    /// The character `key` was online until `now`, or was added as a person friend then.
    fn seen(&mut self, key: String, now: f64) {
        if self.last_seen.len() >= MAX_SEEN && !self.last_seen.contains_key(&key) {
            let oldest = self
                .last_seen
                .iter()
                .min_by(|a, b| a.1.total_cmp(b.1))
                .map(|(k, _)| k.clone());
            if let Some(oldest) = oldest {
                self.last_seen.remove(&oldest);
            }
        }
        self.last_seen.insert(key, now);
    }

    /// What the routing knows of `who`'s presence at `now`.
    fn presence(&self, who: &Address, now: f64) -> Option<Presence> {
        if self.plays(who) {
            return Some(Presence::Online);
        }
        self.last_seen
            .get(&who.key())
            .map(|&t| Presence::Seen((now - t).max(0.0)))
    }

    /// `who`'s view as it is sent: the registry's, cut to a view's limits by who is online
    /// ([`cut_view`]); and, for a person listing more person friends than a view holds, every
    /// character its friends list, whose presence weighs in.
    fn view_for(
        &self,
        dir: &dyn Directory,
        who: &Address,
        now: f64,
    ) -> (PersonView, BTreeSet<String>) {
        let whole = dir.person_view(who);
        let weighed = if whole.persons.len() > MAX_PERSON_FRIENDS {
            whole
                .persons
                .iter()
                .flat_map(|p| &p.characters)
                .map(Address::key)
                .collect()
        } else {
            BTreeSet::new()
        };
        (cut_view(whole, |c| self.presence(c, now)), weighed)
    }

    /// How many watched views wait to be looked at again.
    #[must_use]
    pub fn views_due(&self) -> usize {
        self.views_due.len()
    }

    /// Keeps the watch `key` of `who`, with the view it was sent and what it weighed, in place of
    /// any it had.
    fn keep_watch(
        &mut self,
        key: (String, String),
        who: Address,
        view: PersonView,
        weighed: BTreeSet<String>,
    ) {
        self.drop_watch(&key);
        index_watch(&mut self.watches_naming, &key, &who, &view);
        for name in &weighed {
            self.watches_weighing
                .entry(name.clone())
                .or_default()
                .insert(key.clone());
        }
        if !weighed.is_empty() {
            self.weighed.insert(key.clone(), weighed);
        }
        self.person_watches.insert(key, (who, view));
    }

    /// Forgets the watch `key`.
    fn drop_watch(&mut self, key: &(String, String)) {
        if let Some((who, view)) = self.person_watches.remove(key) {
            unindex_watch(&mut self.watches_naming, key, &who, &view);
        }
        for name in self.weighed.remove(key).into_iter().flatten() {
            if let Some(watches) = self.watches_weighing.get_mut(&name) {
                watches.remove(key);
                if watches.is_empty() {
                    self.watches_weighing.remove(&name);
                }
            }
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
        // Its `person.watch`es go with it; it sends them again on its next link. Its characters
        // were online until now.
        let watches: Vec<(String, String)> = self.watches_of(slug).cloned().collect();
        for w in &watches {
            self.drop_watch(w);
            self.seen(w.1.clone(), self.clock);
            self.presence_changed(&w.1);
        }
        // What it was told to turn off and has not confirmed is told again on its next link, and
        // its rooms too.
        self.offs_sent.retain(|(w, _)| w != slug);
        self.setups.remove(slug);
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
                out.extend(self.answer_origin(w.origin, w.origin_id, Outcome::Unavailable, None));
            }
        }
        // Its answers to the reach questions it was asked will not come, nor to the lines.
        self.asking.retain(|_, a| a.world != slug);
        let unsaid: Vec<String> = self
            .saying
            .iter()
            .filter(|(_, s)| s.world == slug)
            .map(|(k, _)| k.clone())
            .collect();
        for k in unsaid {
            if let Some(s) = self.saying.remove(&k) {
                self.say_answers.push(SayAnswer {
                    asker: s.asker,
                    who: s.who,
                    said: Said::Unavailable,
                });
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
        self.clock = self.clock.max(now);
        let out = self.take(dir, slug, m, now);
        self.remember(&out);
        out
    }

    /// The directory's messages to send since the last call, each with its world, in order. They
    /// are kept apart from [`Out`]: a link message and a directory message are two families.
    pub fn take_listings(&mut self) -> Vec<(String, DirectoryMessage)> {
        std::mem::take(&mut self.listings_out)
    }

    /// One of the directory's messages from `slug`'s link. A listing names the world's own
    /// character, or is refused as another world's; one that lists is held to the character's,
    /// the world's and the hub's paces for listings, and one that takes a character out to none.
    /// A listing sets what the directory holds, so a resent one is applied again, to no change.
    /// Its answer waits in [`Hub::take_listings`]; what this returns is the link's own (an error,
    /// a close).
    pub fn receive_listing(
        &mut self,
        dir: &dyn Directory,
        slug: &str,
        m: DirectoryMessage,
        now: f64,
    ) -> Vec<Out> {
        let mut out = Vec::new();
        let Some(link) = self.links.get(slug) else {
            return out;
        };
        if !link.greeted {
            self.links.remove(slug);
            out.push(Self::error(slug, ErrorCode::Order, "hello first"));
            out.push(Out::Close(slug.to_owned(), "spoke before hello"));
            return out;
        }
        let DirectoryMessage::Listing {
            id,
            who,
            on,
            profile,
        } = m
        else {
            out.push(Self::error(
                slug,
                ErrorCode::Malformed,
                "a message only the hub sends",
            ));
            return out;
        };
        let result = |answer: ListingAnswer| {
            (
                slug.to_owned(),
                DirectoryMessage::ListingResult {
                    id: id.clone(),
                    outcome: answer.outcome,
                    reason: answer.reason,
                    url: answer.url,
                },
            )
        };
        let limited = ListingAnswer {
            outcome: ListingOutcome::RateLimited,
            reason: None,
            url: None,
            changed: Vec::new(),
        };
        let world_rate = self.limits.messages_per_world;
        if !self
            .world_buckets
            .entry(slug.to_owned())
            .or_insert_with(|| Bucket::new(world_rate, now))
            .take(now)
        {
            self.listings_out.push(result(limited));
            return out;
        }
        if who.world() != slug {
            out.push(Self::error(slug, ErrorCode::Identity, "listing who"));
            return out;
        }
        if on && !self.within_listing_paces(slug, &who, now) {
            self.listings_out.push(result(limited));
            return out;
        }
        if !on {
            // The world has heard the character is out: an off told later is new.
            self.offs_sent.remove(&(slug.to_owned(), who.key()));
        }
        let answer = dir.listing(&who, profile.as_deref().filter(|_| on));
        // A linked character that came onto the network or left it resumes or pauses its link: the
        // views naming its person's characters may differ.
        self.persons_changed(&answer.changed);
        self.listings_out.push(result(answer));
        out
    }

    /// Whether a listing of `who` from `slug` is within the character's, the world's and the hub's
    /// paces for listings. Takes from all three only when all three allow it.
    fn within_listing_paces(&mut self, slug: &str, who: &Address, now: f64) -> bool {
        let (character_rate, world_rate) = (LISTINGS_PER_CHARACTER, LISTINGS_PER_WORLD);
        let character = self
            .listing_buckets
            .entry(who.key())
            .or_insert_with(|| Bucket::new(character_rate, now));
        if !character.has(now) {
            return false;
        }
        let world = self
            .listing_world_buckets
            .entry(slug.to_owned())
            .or_insert_with(|| Bucket::new(world_rate, now));
        if !world.has(now) || !self.listings_bucket.has(now) {
            return false;
        }
        self.listings_bucket.take(now)
            && self
                .listing_world_buckets
                .get_mut(slug)
                .is_some_and(|b| b.take(now))
            && self
                .listing_buckets
                .get_mut(&who.key())
                .is_some_and(|b| b.take(now))
    }

    /// Queues the `listing.off`s `slug` owes and has not been handed on its current link.
    fn queue_offs(&mut self, dir: &dyn Directory, slug: &str) {
        for (who, by) in dir.owed_offs(slug) {
            if self.offs_sent.insert((slug.to_owned(), who.key())) {
                self.listings_out
                    .push((slug.to_owned(), DirectoryMessage::ListingOff { who, by }));
            }
        }
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
            let Message::Hello {
                proto,
                websocket_url,
                status_url,
                ..
            } = m
            else {
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
            // Kept before the welcome goes, so the world is listed with them once it is welcomed.
            out.push(Out::WebAddresses {
                slug: slug.to_owned(),
                websocket_url: websocket_url.filter(|u| web::is_websocket_url(u)),
                status_url: status_url.filter(|u| web::is_status_url(u)),
            });
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
                out.extend(self.forward_tell(dir, h.origin, h.tell, now));
            }
            self.queue_offs(dir, slug);
            // Its rooms follow its welcome, whatever it was told before.
            self.setups.remove(slug);
            self.tell_rooms(dir, slug, None);
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
                    out.extend(self.forward_tell(dir, slug.to_owned(), tell, now));
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
                        // A linked recipient not online: its world is asked whether it is
                        // reachable, and the sender is answered as ever.
                        if let (Outcome::Offline, Some(d)) = (outcome, w.discord) {
                            self.ask_reach(slug, *d, now);
                        }
                        out.extend(self.answer_origin(w.origin, w.origin_id, outcome, message));
                    }
                }
            }
            Message::RoomJoin { room, on } => {
                // A room of the world's own is kept too, so that it is mirrored into Discord as
                // soon as its operator asks for it.
                if let Some(l) = self.links.get_mut(slug) {
                    let own = l
                        .rooms
                        .iter()
                        .filter(|r| !ROOMS.contains(&r.as_str()))
                        .count();
                    if !on {
                        l.rooms.remove(&room);
                    } else if ROOMS.contains(&room.as_str()) || own < MAX_OWN_ROOMS {
                        l.rooms.insert(room);
                    }
                }
            }
            Message::RoomSay {
                id,
                room,
                from,
                text,
                via,
            } => {
                if from.world() != slug {
                    out.push(Self::error(slug, ErrorCode::Identity, "room.say from"));
                    return out;
                }
                if let Some(again) = self.resent(slug, &id, now) {
                    return again;
                }
                // The network's rooms go to every world carrying them; a room of the world's own
                // goes to Discord alone, while it is mirrored there for this world.
                let network = ROOMS.contains(&room.as_str());
                let own = Bridged::World {
                    slug: slug.to_owned(),
                    room: room.clone(),
                };
                let carries = self
                    .links
                    .get(slug)
                    .is_some_and(|l| l.rooms.contains(&room))
                    && (network || self.bridged.contains(&own));
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
                    let to_discord = if network {
                        Some(Bridged::Room(room.clone())).filter(|b| self.bridged.contains(b))
                    } else {
                        Some(own)
                    };
                    if let Some(to) = to_discord {
                        self.bridge_out.push(BridgeLine {
                            to,
                            from: from.clone(),
                            text: text.clone(),
                            via: via.clone(),
                        });
                    }
                    if network {
                        let line = Message::RoomLine {
                            room: room.clone(),
                            from,
                            text,
                            via,
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
                ..
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
                let was = self.person_watches.contains_key(&key);
                if !on {
                    self.drop_watch(&key);
                    if was {
                        self.seen(who.key(), now);
                        self.presence_changed(&who.key());
                    }
                } else if was || self.person_watches(slug) < MAX_PERSON_WATCHES {
                    let (view, weighed) = self.view_for(dir, &who, now);
                    out.push(Out::Send(slug.to_owned(), view_message(&who, &view)));
                    if !view.linked && !view.persons.is_empty() {
                        dir.returns_sent(&who);
                    }
                    let name = who.key();
                    self.keep_watch(key, who, view, weighed);
                    if !was {
                        self.presence_changed(&name);
                    }
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
                let Some(page) = self.link_url.clone() else {
                    return person_result(String::new(), PersonOutcome::Unavailable);
                };
                return match dir.person_code(who) {
                    Ok((code, secs)) => Message::PersonResult {
                        id: String::new(),
                        outcome: PersonOutcome::Ok,
                        // The link carries the code, so that opening it links the character.
                        url: Some(link_with_code(&page, &code)),
                        code: Some(code),
                        expires_secs: Some(secs),
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
            // Only a world of the first version asks it: nothing of a person is hidden now, so
            // nothing changes, and the answer names the person's page on the site.
            (PersonOp::Alts, _) => match dir.person_alts(who) {
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
            (PersonOp::Befriend, Some(target)) => {
                let answer = dir.person_befriend(who, target);
                // A friend just added counts as seen now, so that a view holding fewer friends
                // than its person lists holds it.
                if !answer.changed.is_empty() && !self.plays(target) {
                    self.seen(target.key(), now);
                }
                answer
            }
            (PersonOp::Carry, _) => dir.person_carry(who, targets),
            (PersonOp::Unfriend, Some(target)) => dir.person_unfriend(who, target),
            // The wire refuses a request without the target its op needs.
            (PersonOp::Befriend | PersonOp::Unfriend | PersonOp::Characters, None) => {
                PersonAnswer::unchanged(PersonOutcome::Refused)
            }
        };
        // One that changed nothing (removing a person friend not listed, listing one again) has no
        // view looked at again.
        self.persons_changed(&answer.changed);
        person_result(String::new(), answer.outcome)
    }

    /// The watched views due, at most [`PERSON_VIEWS_PER_ROUND`] of them read, each sent again where
    /// it changed.
    fn send_person_views(&mut self, dir: &dyn Directory, now: f64) -> Vec<Out> {
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
            let (view, weighed) = self.view_for(dir, &who, now);
            if self
                .person_watches
                .get(&key)
                .is_some_and(|(_, last)| *last != view)
            {
                out.push(Out::Send(key.0.clone(), view_message(&who, &view)));
                if !view.linked && !view.persons.is_empty() {
                    dir.returns_sent(&who);
                }
            }
            // Kept again even when it did not change, for what it weighed may have.
            self.keep_watch(key, who, view, weighed);
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

    fn forward_tell(
        &mut self,
        dir: &dyn Directory,
        origin: String,
        tell: Message,
        now: f64,
    ) -> Vec<Out> {
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
        // Only a tell to a linked character is kept until its world answers.
        let discord = self
            .discord_tells
            .then(|| dir.discord_tells(&to))
            .flatten()
            .map(|person| {
                Box::new(ForDiscord {
                    from: from.clone(),
                    to: to.clone(),
                    text: text.clone(),
                    person,
                })
            });
        self.waiting.insert(
            hub_id.clone(),
            Waiting {
                origin,
                origin_id: id,
                target: target.clone(),
                until: now + ANSWER_SECS,
                discord,
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

    /// Asks `world` whether the recipient of `tell`, which it answered `offline`, is reachable by
    /// its sender, holding the tell until it answers, at most [`ANSWER_SECS`].
    fn ask_reach(&mut self, world: &str, tell: ForDiscord, now: f64) {
        let id = self.fresh_id();
        self.reach_out.push((
            world.to_owned(),
            ReachMessage::Ask {
                id: id.clone(),
                from: tell.from.clone(),
                to: tell.to.clone(),
            },
        ));
        self.asking.insert(
            id,
            Asking {
                world: world.to_owned(),
                tell,
                until: now + ANSWER_SECS,
            },
        );
    }

    /// The passage of time: held tells and unanswered tells whose time is up are answered
    /// `unavailable`; old ids and full buckets are forgotten; a linked world whose federation flag
    /// is no longer on is closed.
    pub fn tick(&mut self, dir: &dyn Directory, now: f64) -> Vec<Out> {
        self.clock = self.clock.max(now);
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
                out.extend(self.answer_origin(h.origin, id, Outcome::Unavailable, None));
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
                out.extend(self.answer_origin(w.origin, w.origin_id, Outcome::Unavailable, None));
            }
        }
        // A world that never answers a reach question has its tell go nowhere else.
        self.asking.retain(|_, a| now < a.until);
        // A line a world never answers is answered for it, to its person.
        let late: Vec<String> = self
            .saying
            .iter()
            .filter(|(_, s)| now >= s.until)
            .map(|(k, _)| k.clone())
            .collect();
        for k in late {
            if let Some(s) = self.saying.remove(&k) {
                self.say_answers.push(SayAnswer {
                    asker: s.asker,
                    who: s.who,
                    said: Said::NoAnswer,
                });
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
        self.listing_buckets.retain(|_, b| !b.full_at(now));
        self.listing_world_buckets.retain(|_, b| !b.full_at(now));
        self.global_buckets.retain(|_, b| !b.full_at(now));
        if std::mem::take(&mut self.setups_due) {
            let greeted: Vec<String> = self
                .links
                .iter()
                .filter(|(_, l)| l.greeted)
                .map(|(s, _)| s.clone())
                .collect();
            for slug in greeted {
                self.tell_rooms(dir, &slug, None);
            }
        }
        if now >= self.next_offs {
            self.next_offs = now + OFFS_EVERY;
            let greeted: Vec<String> = self
                .links
                .iter()
                .filter(|(_, l)| l.greeted)
                .map(|(s, _)| s.clone())
                .collect();
            for slug in greeted {
                self.queue_offs(dir, &slug);
            }
        }
        if now >= self.next_lapse {
            self.next_lapse = now + LAPSE_EVERY;
            dir.lapse_listings();
            self.last_seen.retain(|_, t| now - *t < SEEN_SECS);
        }
        if !self.views_due.is_empty() && now >= self.next_person_views {
            self.next_person_views = now + PERSON_VIEWS_EVERY;
            out.extend(self.send_person_views(dir, now));
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
                let to_discord = Bridged::Channel(to.name().to_owned());
                if self.bridged.contains(&to_discord) {
                    self.bridge_out.push(BridgeLine {
                        to: to_discord,
                        from: from.clone(),
                        text: text.to_owned(),
                        via: None,
                    });
                    Outcome::Delivered
                } else if held > 0 && held == members.len() {
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
}

/// The `person.friends` message of `who`'s view.
fn view_message(who: &Address, view: &PersonView) -> Message {
    Message::person_view(who.clone(), view.linked, view.persons.clone())
}

/// The link a code request is answered with: the page that links a character, and the code after
/// `#`, which the page reads and no server is sent. The page alone, should the two be too long for
/// a result's page.
fn link_with_code(page: &str, code: &str) -> String {
    let link = format!("{page}#{code}");
    if dereth_network_protocol::person::is_page_url(&link) {
        link
    } else {
        page.to_owned()
    }
}

/// Whether `who` is one of the characters the person friends of `of`'s person list, while `of` is
/// linked: as `of`'s view names them, within its limits and one frame, as `of`'s world is sent it.
fn person_friend(dir: &dyn Directory, of: &Address, who: &Address) -> bool {
    let view = dir.person_view(of);
    if !view.linked {
        return false;
    }
    let Message::PersonFriends { persons, .. } = view_message(of, &view) else {
        return false;
    };
    persons
        .iter()
        .any(|p| p.characters.iter().any(|c| c.same(who)))
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
                    websocket_url: None,
                    status_url: None,
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

    /// Spec: hub: the web addresses a world's hello names go to the registry before its welcome,
    /// each only within the rule; a second hello, refused, changes nothing.
    #[test]
    fn a_hellos_web_addresses_are_kept_within_the_rule_before_the_welcome() {
        let d = dir();
        let hello = |ws: Option<&str>, status: Option<&str>| Message::Hello {
            proto: PROTOCOL,
            software: "t".into(),
            websocket_url: ws.map(str::to_owned),
            status_url: status.map(str::to_owned),
        };
        let addresses = |out: &[Out]| {
            out.iter()
                .position(|o| matches!(o, Out::WebAddresses { .. }))
                .map(|at| (at, out[at].clone()))
        };
        let mut h = hub_with(&[]);
        h.connect("aa", "AA");
        let out = h.receive(
            &d,
            "aa",
            hello(
                Some("wss://aa.example.org/ws"),
                Some("https://aa.example.org/status"),
            ),
            0.0,
        );
        let (at, kept) = addresses(&out).expect("the addresses go to the registry");
        assert_eq!(
            kept,
            Out::WebAddresses {
                slug: "aa".into(),
                websocket_url: Some("wss://aa.example.org/ws".into()),
                status_url: Some("https://aa.example.org/status".into()),
            }
        );
        let welcome = out
            .iter()
            .position(|o| matches!(o, Out::Send(_, Message::Welcome { .. })))
            .expect("welcomed");
        assert!(at < welcome, "{out:?}");
        let again = h.receive(&d, "aa", hello(None, None), 0.0);
        assert_eq!(addresses(&again), None, "a second hello is refused");

        let mut h = hub_with(&[]);
        h.connect("aa", "AA");
        let out = h.receive(
            &d,
            "aa",
            hello(
                Some("ws://aa.example.org/ws"),
                Some("http://aa.example.org/status"),
            ),
            0.0,
        );
        assert_eq!(
            addresses(&out).map(|(_, o)| o),
            Some(Out::WebAddresses {
                slug: "aa".into(),
                websocket_url: None,
                status_url: None,
            }),
            "plain addresses to another machine are not kept"
        );
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
                websocket_url: None,
                status_url: None,
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
                    websocket_url: None,
                    status_url: None,
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
                websocket_url: None,
                status_url: None,
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
            via: None,
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

    fn join(h: &mut Hub, slug: &str, room: &str) {
        h.receive(
            &dir(),
            slug,
            Message::RoomJoin {
                room: room.into(),
                on: true,
            },
            0.0,
        );
    }

    fn room_say(id: &str, room: &str, from: &str, text: &str) -> Message {
        Message::RoomSay {
            id: id.into(),
            room: room.into(),
            from: a(from),
            text: text.into(),
            via: None,
        }
    }

    fn room_outcome(out: &[Out], slug: &str) -> Option<Outcome> {
        sent_to(out, slug).into_iter().find_map(|m| match m {
            Message::RoomResult { outcome, .. } => Some(*outcome),
            _ => None,
        })
    }

    /// Spec: hub: a line said in a room mirrored into Discord goes to Discord as well as to every
    /// other world carrying the room; in a room not mirrored, to the worlds alone.
    #[test]
    fn a_line_in_a_carried_room_goes_to_discord_as_well_as_the_worlds() {
        let d = dir();
        let mut h = hub_with(&["aa", "bb"]);
        join(&mut h, "aa", "global");
        join(&mut h, "bb", "global");
        let out = h.receive(&d, "aa", room_say("r1", "global", "Ann@aa", "hi"), 1.0);
        assert_eq!(sent_to(&out, "bb").len(), 1);
        assert!(h.take_bridged().is_empty(), "not mirrored into Discord");
        h.set_bridged([Bridged::Room("global".into())].into());
        let out = h.receive(&d, "aa", room_say("r2", "global", "Ann@aa", "hello"), 1.0);
        assert_eq!(sent_to(&out, "bb").len(), 1, "the worlds as before");
        assert_eq!(
            h.take_bridged(),
            [BridgeLine {
                to: Bridged::Room("global".into()),
                from: a("Ann@aa"),
                text: "hello".into(),
                via: None,
            }]
        );
        assert!(h.take_bridged().is_empty(), "taken once");
        let resent = h.receive(&d, "aa", room_say("r2", "global", "Ann@aa", "hello"), 1.5);
        assert!(sent_to(&resent, "bb").is_empty());
        assert!(h.take_bridged().is_empty(), "a resend goes nowhere again");
    }

    /// Spec: hub: a world's own room goes to Discord alone, while it is mirrored for that world;
    /// another world's room of the same name is not, and a line in a room not mirrored is answered
    /// `not_member`. A link carries at most ten rooms of its own.
    #[test]
    fn a_worlds_own_room_goes_to_discord_alone_while_carried_for_it() {
        let d = dir();
        let mut h = hub_with(&["aa", "bb"]);
        join(&mut h, "aa", "general");
        join(&mut h, "bb", "general");
        let before = h.receive(&d, "aa", room_say("r1", "general", "Ann@aa", "x"), 1.0);
        assert_eq!(room_outcome(&before, "aa"), Some(Outcome::NotMember));
        let own = Bridged::World {
            slug: "aa".into(),
            room: "general".into(),
        };
        h.set_bridged([own.clone()].into());
        let out = h.receive(&d, "aa", room_say("r2", "general", "Ann@aa", "news"), 1.0);
        assert_eq!(room_outcome(&out, "aa"), Some(Outcome::Ok));
        assert!(sent_to(&out, "bb").is_empty(), "to no other world");
        assert_eq!(h.take_bridged().len(), 1);
        let other = h.receive(&d, "bb", room_say("r3", "general", "Bob@bb", "x"), 1.0);
        assert_eq!(room_outcome(&other, "bb"), Some(Outcome::NotMember));
        assert!(h.take_bridged().is_empty());
        for i in 0..20 {
            join(&mut h, "aa", &format!("room{i}"));
        }
        h.set_bridged(
            [Bridged::World {
                slug: "aa".into(),
                room: "room15".into(),
            }]
            .into(),
        );
        let over = h.receive(&d, "aa", room_say("r4", "room15", "Ann@aa", "x"), 1.0);
        assert_eq!(
            room_outcome(&over, "aa"),
            Some(Outcome::NotMember),
            "past ten rooms of its own, a join is not kept"
        );
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
                    websocket_url: None,
                    status_url: None,
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
                websocket_url: None,
                status_url: None,
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
                    via: None,
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
            via: None,
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
                websocket_url: None,
                status_url: None,
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
                websocket_url: None,
                status_url: None,
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
                via: None,
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
        fn person_alts(&self, who: &Address) -> PersonAnswer {
            self.asked.borrow_mut().push(format!("alts {who}"));
            PersonAnswer::unchanged(PersonOutcome::Refused)
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

    /// Spec: link protocol: a hub with no page that links a character links none: a code request
    /// is answered `unavailable`; with one, it is the code, its lapse and the link, the page with
    /// the code after `#`, so that opening it links the character.
    #[test]
    fn a_link_code_comes_as_a_link_that_carries_it() {
        let d = person_dir();
        let mut h = hub_with(&["aa"]);
        let out = h.receive(&d, "aa", person("p1", PersonOp::Code, "Ann@aa", None), 1.0);
        assert_eq!(
            person_outcome(&out, "aa"),
            Some(("p1".into(), PersonOutcome::Unavailable, None))
        );
        h.set_link_url(Some("https://site.test/link/".into()));
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
        assert_eq!(*url, format!("https://site.test/link/#{code}"));
        assert_eq!(
            link_with_code(&format!("https://{}/", "x".repeat(190)), "K7QD-2MXP"),
            format!("https://{}/", "x".repeat(190)),
            "the page alone when the two are too long for a result"
        );
    }

    /// Spec: link protocol: a world of the first version asking to hide or show its character's
    /// person's characters is refused either way, with the person's page on the site.
    #[test]
    fn hiding_or_showing_a_persons_characters_is_refused_with_the_persons_page() {
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
            Some(("p2".into(), PersonOutcome::Refused, None))
        );
        assert_eq!(*d.asked.borrow(), ["alts Ann@aa", "alts Ann@aa"]);
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
    /// and a world of the first version asking to hide characters do.
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
        fn person_alts(&self, _: &Address) -> PersonAnswer {
            PersonAnswer::unchanged(PersonOutcome::Refused)
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
            Some(PersonOutcome::Refused)
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

    /// `n` person friends, `Friend <letters>@bb` in the order added, each listing itself alone.
    fn many_friends(n: usize) -> PersonView {
        PersonView {
            linked: true,
            persons: (0..n)
                .map(|i| {
                    let handle = a(&format!("Friend {}@bb", letters(i)));
                    PersonFriend {
                        characters: vec![handle.clone()],
                        handle,
                    }
                })
                .collect(),
        }
    }

    fn handles_of(v: &PersonView) -> Vec<String> {
        v.persons.iter().map(|p| p.handle.to_string()).collect()
    }

    /// Spec: hub: a view holds every person friend, in the order added, while its person lists no
    /// more than a view holds; past that, those online first, then those seen most lately, then
    /// those added most lately, each friend as present as the most present character it lists,
    /// the ones held in the order they were added.
    #[test]
    fn a_view_past_its_limit_holds_the_friends_online_first_then_the_latest_seen() {
        let few = many_friends(MAX_PERSON_FRIENDS);
        assert_eq!(
            cut_view(few.clone(), |_| Some(Presence::Online)),
            cut_view(few.clone(), |_| None),
            "a view within its limit holds them all, whoever is online"
        );
        assert_eq!(
            handles_of(&cut_view(few.clone(), |_| None)),
            handles_of(&few)
        );
        let mut whole = many_friends(MAX_PERSON_FRIENDS + 50);
        // Friend 3 lists another character, which is online.
        whole.persons[3].characters.push(a("Alt@cc"));
        let name = |i: usize| format!("Friend {}@bb", letters(i));
        let presence = |who: &Address| -> Option<Presence> {
            let n = who.to_string();
            if n == "Alt@cc" || n == name(0) || n == name(10) {
                Some(Presence::Online)
            } else if n == name(1) {
                Some(Presence::Seen(5.0))
            } else if n == name(2) {
                Some(Presence::Seen(500.0))
            } else {
                None
            }
        };
        let held = handles_of(&cut_view(whole.clone(), presence));
        // The five present, then the latest added of those never seen, all in the order added.
        let mut expected: Vec<usize> = vec![0, 1, 2, 3];
        expected.extend((5..MAX_PERSON_FRIENDS + 50).filter(|i| *i == 10 || *i >= 55));
        assert_eq!(held, expected.into_iter().map(name).collect::<Vec<_>>());
        // When they do not all fit, seen lately wins over seen long ago.
        let whole = many_friends(MAX_PERSON_FRIENDS + 1);
        let all = handles_of(&whole);
        let held = handles_of(&cut_view(whole, |who| {
            let i = all.iter().position(|h| *h == who.to_string()).unwrap_or(0);
            Some(Presence::Seen(f64::from(u32::try_from(i).expect("small"))))
        }));
        assert_eq!(
            held,
            all[..MAX_PERSON_FRIENDS],
            "the one seen longest ago left out"
        );
    }

    /// Spec: hub: the routing remembers, while it runs, who was online and when: a view whose
    /// person lists more friends than it holds is looked at again when one of them comes online
    /// or leaves, or its world's link goes, and sent where that changes which it holds; a friend
    /// just added counts as seen. A view within its limit is not read again for anyone's presence.
    #[test]
    fn a_view_past_its_limit_follows_who_comes_online() {
        let d = ViewsDir {
            worlds: dir(),
            views: std::cell::RefCell::new(
                [
                    ("ann@aa", many_friends(MAX_PERSON_FRIENDS + 1)),
                    ("cat@aa", view_of(&[&["Friend a@bb"]])),
                ]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect(),
            ),
            persons: [("ann@aa", vec![a("Ann@aa")])]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect(),
            read: std::cell::RefCell::new(Vec::new()),
        };
        let mut h = hub_with(&["aa", "bb", "cc"]);
        let watch = |who: &str, on: bool| Message::PersonWatch { who: a(who), on };
        let held = |out: &[Out]| -> Option<Vec<String>> {
            out.iter().find_map(|o| match o {
                Out::Send(_, Message::PersonFriends { to, persons, .. }) if to.name() == "Ann" => {
                    Some(persons.iter().map(|p| p.handle.to_string()).collect())
                }
                _ => None,
            })
        };
        let first = held(&h.receive(&d, "aa", watch("Ann@aa", true), 1.0)).expect("a view");
        h.receive(&d, "aa", watch("Cat@aa", true), 1.0);
        let (oldest, next) = ("Friend a@bb".to_owned(), "Friend b@bb".to_owned());
        assert!(!first.contains(&oldest), "nobody seen: the latest added");
        read_since(&d);
        // The oldest friend comes online (its own view read as it is watched): Ann's view holds it
        // now, and Cat's is not read.
        h.receive(&d, "bb", watch("Friend a@bb", true), 2.0);
        assert_eq!(read_since(&d), ["Friend a@bb"]);
        let out = h.tick(&d, 3.0);
        assert_eq!(read_since(&d), ["Ann@aa"]);
        let now = held(&out).expect("sent again");
        assert!(now.contains(&oldest));
        assert!(!now.contains(&next), "the next oldest is left out");
        // It leaves, seen just now: still held, so the view is read and not sent.
        h.receive(&d, "bb", watch("Friend a@bb", false), 4.0);
        let out = h.tick(&d, 5.0);
        assert_eq!(read_since(&d), ["Ann@aa"]);
        assert_eq!(held(&out), None, "unchanged");
        // Its world's link goes with the friend online: seen until then.
        h.receive(&d, "bb", watch("Friend a@bb", true), 6.0);
        h.tick(&d, 7.0);
        read_since(&d);
        h.disconnect("bb");
        h.tick(&d, 8.0);
        assert_eq!(read_since(&d), ["Ann@aa"]);
        // A friend just added counts as seen now.
        let mut persons = many_friends(MAX_PERSON_FRIENDS + 1).persons;
        persons.push(PersonFriend {
            handle: a("Newest@cc"),
            characters: vec![a("Newest@cc")],
        });
        persons.swap(0, MAX_PERSON_FRIENDS + 1);
        d.views.borrow_mut().insert(
            "ann@aa".into(),
            PersonView {
                linked: true,
                persons,
            },
        );
        h.receive(
            &d,
            "aa",
            person("f1", PersonOp::Befriend, "Ann@aa", Some("Newest@cc")),
            9.0,
        );
        let out = h.tick(&d, 10.0);
        assert!(held(&out)
            .expect("sent again")
            .contains(&"Newest@cc".to_owned()));
    }

    /// Spec: hub: a listing that puts a linked character on the network or takes it off has the
    /// views that name its person's characters looked at again.
    #[test]
    fn a_listing_that_pauses_a_link_has_the_views_naming_its_person_looked_at_again() {
        struct PausingDir {
            views: ViewsDir,
            changed: Vec<Address>,
        }
        impl Directory for PausingDir {
            fn lookup(&self, slug: &str) -> Option<DirEntry> {
                self.views.lookup(slug)
            }
            fn person_view(&self, who: &Address) -> PersonView {
                self.views.person_view(who)
            }
            fn listing(&self, _: &Address, _: Option<&Profile>) -> ListingAnswer {
                ListingAnswer {
                    outcome: ListingOutcome::Ok,
                    reason: None,
                    url: None,
                    changed: self.changed.clone(),
                }
            }
        }
        let d = PausingDir {
            views: ViewsDir {
                worlds: dir(),
                views: std::cell::RefCell::new(
                    [("dan@aa", view_of(&[&["Bob@bb", "Bob@cc"]]))]
                        .into_iter()
                        .map(|(k, v)| (k.to_owned(), v))
                        .collect(),
                ),
                persons: Map::new(),
                read: std::cell::RefCell::new(Vec::new()),
            },
            changed: vec![a("Bob@bb"), a("Bob@cc")],
        };
        let mut h = hub_with(&["aa", "bb", "cc"]);
        h.receive(
            &d,
            "aa",
            Message::PersonWatch {
                who: a("Dan@aa"),
                on: true,
            },
            1.0,
        );
        read_since(&d.views);
        d.views
            .views
            .borrow_mut()
            .insert("dan@aa".into(), view_of(&[&["Bob@bb"]]));
        h.receive_listing(&d, "cc", listing("l1", "Bob@cc", false), 2.0);
        let out = h.tick(&d, 3.0);
        assert_eq!(read_since(&d.views), ["Dan@aa"]);
        assert_eq!(views_sent(&out), [("aa".to_owned(), "Dan@aa".to_owned())]);
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

    /// A registry stand-in that keeps listings: what each was asked, and the offs each world owes.
    struct Listings {
        worlds: Dir,
        asked: std::cell::RefCell<Vec<(String, bool)>>,
        owed: std::cell::RefCell<Vec<(Address, OffBy)>>,
    }

    impl Directory for Listings {
        fn lookup(&self, slug: &str) -> Option<DirEntry> {
            self.worlds.lookup(slug)
        }

        fn listing(&self, who: &Address, profile: Option<&Profile>) -> ListingAnswer {
            self.asked
                .borrow_mut()
                .push((who.to_string(), profile.is_some()));
            if profile.is_none() {
                self.owed.borrow_mut().retain(|(w, _)| !w.same(who));
            }
            ListingAnswer {
                outcome: ListingOutcome::Ok,
                reason: None,
                url: Some(format!(
                    "https://site.test/directory/character/?a={}",
                    who.name()
                )),
                changed: Vec::new(),
            }
        }

        fn owed_offs(&self, slug: &str) -> Vec<(Address, OffBy)> {
            self.owed
                .borrow()
                .iter()
                .filter(|(w, _)| w.world() == slug)
                .cloned()
                .collect()
        }
    }

    fn listings() -> Listings {
        Listings {
            worlds: dir(),
            asked: std::cell::RefCell::new(Vec::new()),
            owed: std::cell::RefCell::new(Vec::new()),
        }
    }

    fn listing(id: &str, who: &str, on: bool) -> DirectoryMessage {
        DirectoryMessage::Listing {
            id: id.into(),
            who: a(who),
            on,
            profile: on.then(|| {
                Box::new(Profile {
                    level: 10,
                    heritage: 1,
                    gender: 1,
                    ..Profile::default()
                })
            }),
        }
    }

    /// The directory's messages the hub queued for `slug` since the last look.
    fn listings_to(h: &mut Hub, slug: &str) -> Vec<DirectoryMessage> {
        h.take_listings()
            .into_iter()
            .filter(|(s, _)| s == slug)
            .map(|(_, m)| m)
            .collect()
    }

    /// How the last listing `slug` sent ended, as the hub queued its answer.
    fn listing_outcome(h: &mut Hub, slug: &str) -> Option<ListingOutcome> {
        listings_to(h, slug).into_iter().find_map(|m| match m {
            DirectoryMessage::ListingResult { outcome, .. } => Some(outcome),
            _ => None,
        })
    }

    /// Spec: hub: a listing that names another world's character is refused as speaking for
    /// another world, and the directory is never asked.
    #[test]
    fn a_listing_for_another_worlds_character_is_refused_and_never_kept() {
        let d = listings();
        let mut h = hub_with(&["aa", "bb"]);
        let out = h.receive_listing(&d, "aa", listing("l1", "Bob@bb", true), 1.0);
        assert!(
            matches!(sent_to(&out, "aa")[..], [Message::Error { code, .. }] if code == "identity"),
            "{out:?}"
        );
        assert!(listings_to(&mut h, "aa").is_empty(), "no answer");
        assert!(d.asked.borrow().is_empty(), "nothing kept");
        let out = h.receive_listing(&d, "aa", listing("l2", "Ann@aa", true), 1.0);
        assert!(out.is_empty(), "{out:?}");
        assert!(matches!(
            &listings_to(&mut h, "aa")[..],
            [DirectoryMessage::ListingResult { id, outcome: ListingOutcome::Ok, url: Some(_), .. }]
                if id == "l2"
        ));
        assert_eq!(*d.asked.borrow(), [("Ann@aa".to_owned(), true)]);
    }

    /// Spec: hub: a world's directory messages before its hello are refused, and a world may not
    /// send what only the hub sends.
    #[test]
    fn a_listing_before_hello_closes_the_link() {
        let d = listings();
        let mut h = hub_with(&["aa"]);
        h.connect("bb", "BB");
        let out = h.receive_listing(&d, "bb", listing("l1", "Bob@bb", true), 0.0);
        assert!(out.contains(&Out::Close("bb".into(), "spoke before hello")));
        let out = h.receive_listing(
            &d,
            "aa",
            DirectoryMessage::ListingOff {
                who: a("Ann@aa"),
                by: OffBy::Operator,
            },
            0.0,
        );
        assert!(
            matches!(sent_to(&out, "aa")[..], [Message::Error { code, .. }] if code == "malformed")
        );
        assert!(d.asked.borrow().is_empty());
        assert!(h.take_listings().is_empty());
    }

    /// Spec: hub: listings are held to the character's pace, and a listing that takes a character
    /// out never is.
    #[test]
    fn listings_are_held_to_the_characters_pace_but_an_off_never_is() {
        let d = listings();
        let mut h = hub_with(&["aa"]);
        let burst = LISTINGS_PER_CHARACTER.burst;
        let mut taken = 0;
        while f64::from(taken) < burst {
            h.receive_listing(&d, "aa", listing("l", "Ann@aa", true), 1.0);
            assert_eq!(listing_outcome(&mut h, "aa"), Some(ListingOutcome::Ok));
            taken += 1;
        }
        h.receive_listing(&d, "aa", listing("l", "Ann@aa", true), 1.0);
        assert_eq!(
            listing_outcome(&mut h, "aa"),
            Some(ListingOutcome::RateLimited)
        );
        h.receive_listing(&d, "aa", listing("l", "Bob@aa", true), 1.0);
        assert_eq!(
            listing_outcome(&mut h, "aa"),
            Some(ListingOutcome::Ok),
            "its own pace"
        );
        h.receive_listing(&d, "aa", listing("l", "Ann@aa", false), 1.0);
        assert_eq!(listing_outcome(&mut h, "aa"), Some(ListingOutcome::Ok));
        h.receive_listing(&d, "aa", listing("l", "Ann@aa", true), 61.0);
        assert_eq!(
            listing_outcome(&mut h, "aa"),
            Some(ListingOutcome::Ok),
            "a minute on"
        );
        let asked = d.asked.borrow();
        assert_eq!(
            asked.iter().filter(|(w, _)| w == "Ann@aa").count(),
            usize::try_from(taken).unwrap_or(0) + 2,
            "the one over the pace was never kept"
        );
    }

    /// Spec: hub: a world is told each character the hub's operator took out, or its person took
    /// off the network forgetting itself, at its hello and within seconds of one made later, once a
    /// link until it says it has heard.
    #[test]
    fn an_owed_off_goes_once_a_link_until_the_world_says_it_has_heard() {
        let d = listings();
        d.owed.borrow_mut().push((a("Ann@aa"), OffBy::Operator));
        let mut h = Hub::new("hub test", Channels::default(), Limits::DEFAULT);
        h.connect("aa", "AA");
        let hello = Message::Hello {
            proto: PROTOCOL,
            software: "t".into(),
            websocket_url: None,
            status_url: None,
        };
        h.receive(&d, "aa", hello.clone(), 0.0);
        assert!(matches!(
            &listings_to(&mut h, "aa")[..],
            [DirectoryMessage::ListingOff { who, by: OffBy::Operator }] if who.name() == "Ann"
        ));
        h.tick(&d, 10.0);
        assert!(listings_to(&mut h, "aa").is_empty(), "once a link");
        d.owed.borrow_mut().push((a("Bob@aa"), OffBy::Person));
        h.tick(&d, 20.0);
        assert!(matches!(
            &listings_to(&mut h, "aa")[..],
            [DirectoryMessage::ListingOff { who, by: OffBy::Person }] if who.name() == "Bob"
        ));
        h.disconnect("aa");
        h.connect("aa", "AA");
        h.receive(&d, "aa", hello.clone(), 30.0);
        assert_eq!(
            listings_to(&mut h, "aa").len(),
            2,
            "told again on the next link"
        );
        h.receive_listing(&d, "aa", listing("l", "Ann@aa", false), 31.0);
        h.take_listings();
        d.owed.borrow_mut().push((a("Ann@aa"), OffBy::Operator));
        h.tick(&d, 40.0);
        let told = listings_to(&mut h, "aa");
        assert!(
            matches!(&told[..], [DirectoryMessage::ListingOff { who, .. }] if who.name() == "Ann"),
            "an off made after the world's is new: {told:?}"
        );
    }

    // ---- tells into a person's Discord ---------------------------------------------------------

    /// The worlds `aa`, `bb` and `cc` on the network, and one person, `p1`, linking `Ann@bb` and
    /// `Ann@cc`, with a person friend that lists `Bob@aa`.
    struct TellsDir;

    impl Directory for TellsDir {
        fn lookup(&self, slug: &str) -> Option<DirEntry> {
            dir().lookup(slug)
        }

        fn discord_tells(&self, who: &Address) -> Option<TellsPerson> {
            ["ann@bb", "ann@cc"]
                .contains(&who.key().as_str())
                .then(|| TellsPerson {
                    person: "p1".into(),
                })
        }

        fn person_view(&self, who: &Address) -> PersonView {
            if who.key() != "ann@bb" {
                return PersonView::default();
            }
            PersonView {
                linked: true,
                persons: vec![PersonFriend {
                    handle: a("Bob@aa"),
                    characters: vec![a("Bob@aa")],
                }],
            }
        }
    }

    /// A hub carrying tells into Discord, with `aa`, `bb` and `cc` linked.
    fn tells_hub() -> Hub {
        let mut h = hub_with(&["aa", "bb", "cc"]);
        h.set_discord_tells(true);
        h
    }

    /// `from` tells `to`, and `to`'s world answers `outcome`: what the teller's world was sent,
    /// and the hub's reach questions since.
    fn told(
        h: &mut Hub,
        id: &str,
        from: &str,
        to: &str,
        outcome: Outcome,
        now: f64,
    ) -> (Vec<Out>, Vec<(String, ReachMessage)>) {
        told_with(h, &TellsDir, id, from, to, outcome, now)
    }

    /// [`told`], with the registry `d`.
    fn told_with(
        h: &mut Hub,
        d: &dyn Directory,
        id: &str,
        from: &str,
        to: &str,
        outcome: Outcome,
        now: f64,
    ) -> (Vec<Out>, Vec<(String, ReachMessage)>) {
        let (sender, target) = (a(from).world().to_owned(), a(to).world().to_owned());
        let out = h.receive(d, &sender, tell(id, from, to), now);
        let hub_id = sent_to(&out, &target)
            .into_iter()
            .find_map(|m| match m {
                Message::Tell { id, .. } => Some(id.clone()),
                _ => None,
            })
            .expect("the tell goes to its world");
        let back = h.receive(
            d,
            &target,
            Message::TellResult {
                id: hub_id,
                outcome,
                message: None,
            },
            now,
        );
        (back, h.take_reach())
    }

    /// [`TellsDir`], with `view` as Ann@bb's person view.
    struct ViewOfAnn(PersonView);

    impl Directory for ViewOfAnn {
        fn lookup(&self, slug: &str) -> Option<DirEntry> {
            TellsDir.lookup(slug)
        }

        fn discord_tells(&self, who: &Address) -> Option<TellsPerson> {
            TellsDir.discord_tells(who)
        }

        fn person_view(&self, who: &Address) -> PersonView {
            if who.key() == "ann@bb" {
                self.0.clone()
            } else {
                PersonView::default()
            }
        }
    }

    /// Spec: hub: a reply from Discord says `friend` when the character's person view, cut to its
    /// limits as its world would be sent it, names the teller among its person friends'
    /// characters, and only while the character is linked: worlds of the first version read it.
    #[test]
    fn a_reply_from_discord_says_friend_as_the_characters_view_would() {
        let friend = |characters: &[&str]| PersonFriend {
            handle: a(characters[0]),
            characters: characters.iter().map(|c| a(c)).collect(),
        };
        let filler = |i: usize| {
            let letters = b"abcdefghijklmnopqrstuvwxyz";
            let (first, second) = (char::from(letters[i / 26]), char::from(letters[i % 26]));
            friend(&[&format!("{}{second}@aa", first.to_ascii_uppercase())])
        };
        // A view as long as a view may be, and one person friend more: the first person friend's
        // other character and the last person friend are past its limits.
        let mut long = vec![friend(&["Bob@aa", "Bob@cc"])];
        long.extend((1..dereth_network_protocol::person::MAX_PERSON_FRIENDS).map(filler));
        long.push(friend(&["Late@aa"]));
        let view = |linked, persons| ViewOfAnn(PersonView { linked, persons });
        let cases = [
            (view(true, long.clone()), "Bob@aa", true),
            (view(true, long.clone()), "Ab@aa", true),
            (view(true, long.clone()), "Bob@cc", false),
            (view(true, long.clone()), "Late@aa", false),
            (view(true, long.clone()), "Cy@cc", false),
            (view(true, long[..1].to_vec()), "Bob@cc", true),
            (view(false, long[..1].to_vec()), "Bob@aa", false),
        ];
        let mut h = tells_hub();
        let mut at = 1.0;
        for (d, to, listed) in &cases {
            at += 1.0;
            let (out, carry) = h.tell_from_discord(d, "p1", &a("Ann@bb"), &a(to), "hi", at);
            assert_eq!(carry, Carry::Carried, "{to}");
            let friend = sent_to(&out, a(to).world())
                .into_iter()
                .find_map(|m| match m {
                    Message::Tell { friend, .. } => Some(*friend),
                    _ => None,
                });
            assert_eq!(friend, Some(*listed), "{to}");
        }
    }

    /// The world answers the reach question `asked`.
    fn reach_answer(h: &mut Hub, world: &str, asked: &ReachMessage, reachable: bool, now: f64) {
        let ReachMessage::Ask { id, .. } = asked else {
            panic!("a question: {asked:?}")
        };
        h.receive_reach(
            world,
            ReachMessage::Result {
                id: id.clone(),
                reachable,
            },
            now,
        );
    }

    /// Spec: hub: a tell to a linked character, answered `offline`, has its world asked whether the
    /// character is reachable, and goes to the person's Discord only when the world says so and
    /// the character is not online, whatever the person's other characters do; the teller is
    /// answered `offline` as for any character not online.
    #[test]
    fn a_tell_goes_to_discord_only_when_its_world_says_reachable_and_the_character_is_away() {
        let d = TellsDir;
        let mut h = tells_hub();
        let (back, asked) = told(&mut h, "t1", "Bob@aa", "Ann@bb", Outcome::Offline, 1.0);
        assert_eq!(
            sent_to(&back, "aa"),
            [&Message::TellResult {
                id: "t1".into(),
                outcome: Outcome::Offline,
                message: None
            }],
            "the teller's answer, and nothing more"
        );
        let [(world, ask)] = &asked[..] else {
            panic!("one question: {asked:?}")
        };
        assert_eq!(world, "bb");
        assert!(matches!(ask, ReachMessage::Ask { from, to, .. }
            if from.to_string() == "Bob@aa" && to.to_string() == "Ann@bb"));
        reach_answer(&mut h, "bb", ask, false, 1.1);
        assert!(h.take_discord_tells().is_empty(), "not reachable");

        let (_, asked) = told(&mut h, "t2", "Bob@aa", "Ann@bb", Outcome::Offline, 2.0);
        reach_answer(&mut h, "cc", &asked[0].1, true, 2.1);
        assert_eq!(
            h.asking_count(),
            1,
            "another world's answer is not the world's"
        );
        reach_answer(&mut h, "bb", &asked[0].1, true, 2.2);
        assert_eq!(
            h.take_discord_tells(),
            [DiscordTell {
                person: "p1".into(),
                from: a("Bob@aa"),
                to: a("Ann@bb"),
                text: "hi".into(),
            }]
        );

        // Another of the person's characters online on the network: the tell goes all the same.
        h.receive(
            &d,
            "cc",
            Message::PersonWatch {
                who: a("Ann@cc"),
                on: true,
            },
            3.0,
        );
        let (_, asked) = told(&mut h, "t3", "Bob@aa", "Ann@bb", Outcome::Offline, 3.0);
        reach_answer(&mut h, "bb", &asked[0].1, true, 3.1);
        assert_eq!(
            h.take_discord_tells().len(),
            1,
            "another of its characters plays"
        );
        // The character told online on the network by the time its world answers: nothing goes.
        let (_, asked) = told(&mut h, "t4", "Bob@aa", "Ann@bb", Outcome::Offline, 3.5);
        h.receive(
            &d,
            "bb",
            Message::PersonWatch {
                who: a("Ann@bb"),
                on: true,
            },
            3.6,
        );
        reach_answer(&mut h, "bb", &asked[0].1, true, 3.7);
        assert!(h.take_discord_tells().is_empty(), "the character plays");
        h.receive(
            &d,
            "bb",
            Message::PersonWatch {
                who: a("Ann@bb"),
                on: false,
            },
            4.0,
        );
        let (_, asked) = told(&mut h, "t4b", "Bob@aa", "Ann@bb", Outcome::Offline, 4.0);
        reach_answer(&mut h, "bb", &asked[0].1, true, 4.1);
        assert_eq!(h.take_discord_tells().len(), 1, "it plays no more");

        // Shown, squelched, or to a character nobody linked: nothing is asked.
        for (id, to, outcome) in [
            ("t5", "Ann@bb", Outcome::Delivered),
            ("t6", "Ann@bb", Outcome::Squelched),
            ("t7", "Cat@bb", Outcome::Offline),
        ] {
            let (_, asked) = told(&mut h, id, "Bob@aa", to, outcome, 5.0);
            assert!(asked.is_empty(), "{id}: {asked:?}");
        }
        h.set_discord_tells(false);
        let (_, asked) = told(&mut h, "t8", "Bob@aa", "Ann@bb", Outcome::Offline, 6.0);
        assert!(asked.is_empty(), "a hub that carries none asks nothing");
        h.set_discord_tells(true);

        // A question its world never answers lets the tell go after the answer's time.
        let (_, asked) = told(&mut h, "t9", "Bob@aa", "Ann@bb", Outcome::Offline, 10.0);
        assert_eq!(h.asking_count(), 1);
        h.tick(&d, 10.0 + ANSWER_SECS);
        assert_eq!(h.asking_count(), 0);
        reach_answer(&mut h, "bb", &asked[0].1, true, 11.0 + ANSWER_SECS);
        assert!(h.take_discord_tells().is_empty(), "too late");
        // And one whose world's link goes.
        let (_, asked) = told(&mut h, "t10", "Bob@aa", "Ann@bb", Outcome::Offline, 50.0);
        assert_eq!(asked.len(), 1);
        h.disconnect("bb");
        assert_eq!(h.asking_count(), 0);
    }

    /// Spec: hub: a person's reply from Discord goes to the teller's world as a tell from the
    /// character the tell was for, `friend` when a person friend that holds for it lists the
    /// teller, within that character's paces; its answer goes back to the person, and no world is
    /// sent anything for it; a reply from anyone but the character's person goes nowhere.
    #[test]
    fn a_reply_from_discord_is_a_tell_from_the_character_and_its_answer_goes_to_the_person() {
        let d = TellsDir;
        let mut h = tells_hub();
        let (out, carry) = h.tell_from_discord(&d, "p1", &a("Ann@bb"), &a("Bob@aa"), "yes!", 1.0);
        assert_eq!(carry, Carry::Carried);
        let [Message::Tell {
            id,
            from,
            to,
            text,
            friend: true,
        }] = sent_to(&out, "aa")[..]
        else {
            panic!("one tell to aa: {out:?}")
        };
        assert_eq!(
            (from.to_string(), to.to_string(), text.as_str()),
            ("Ann@bb".into(), "Bob@aa".into(), "yes!")
        );
        assert_eq!(out.len(), 1, "nothing else goes anywhere");
        let back = h.receive(
            &d,
            "aa",
            Message::TellResult {
                id: id.clone(),
                outcome: Outcome::Offline,
                message: None,
            },
            1.1,
        );
        assert!(back.is_empty(), "no world is told: {back:?}");
        assert_eq!(
            h.take_discord_answers(),
            [DiscordAnswer {
                person: "p1".into(),
                from: a("Ann@bb"),
                to: a("Bob@aa"),
                outcome: Outcome::Offline,
                message: None,
            }]
        );

        for (person, from) in [("p2", "Ann@bb"), ("p1", "Bob@cc")] {
            let (out, carry) = h.tell_from_discord(&d, person, &a(from), &a("Bob@aa"), "x", 2.0);
            assert_eq!(
                (out.len(), carry),
                (0, Carry::NotCarried),
                "{person} as {from}"
            );
        }
        let (_, carry) = h.tell_from_discord(&d, "p1", &a("Ann@bb"), &a("Bob@off"), "x", 2.0);
        assert_eq!(carry, Carry::NotCarried);
        assert_eq!(h.take_discord_answers()[0].outcome, Outcome::NotOnNetwork);

        // Within the paces of the character's own tells: its burst of ten, refilled since.
        let mut held = None;
        for i in 0..12 {
            let (_, carry) =
                h.tell_from_discord(&d, "p1", &a("Ann@bb"), &a("Bob@aa"), "again", 3.0);
            if carry == Carry::HeldBack {
                held = Some(i);
                break;
            }
        }
        assert_eq!(held, Some(10), "a burst of ten went");
        assert!(h
            .take_discord_answers()
            .iter()
            .any(|a| a.outcome == Outcome::RateLimited));

        // A reply for a world whose link is down is held, and answered for when it lapses.
        let mut h = tells_hub();
        h.disconnect("aa");
        let (out, carry) = h.tell_from_discord(&d, "p1", &a("Ann@bb"), &a("Bob@aa"), "hi", 1.0);
        assert_eq!((out.len(), carry, h.held_count()), (0, Carry::Carried, 1));
        h.tick(&d, 1.0 + Limits::DEFAULT.hold_secs);
        assert_eq!(h.take_discord_answers()[0].outcome, Outcome::Unavailable);
    }

    /// Spec: hub: a tell to a linked character from a character of its own world goes to the
    /// character's world and, answered `offline`, has that world asked `reach` and goes to the
    /// person's Discord when it says so, as one from another world does; the person's reply goes
    /// back to that world as a tell from the character, and its answer to the person.
    #[test]
    fn a_tell_from_the_characters_own_world_reaches_its_persons_discord_and_the_reply_comes_back() {
        let d = TellsDir;
        let mut h = tells_hub();
        let (back, asked) = told(&mut h, "t1", "Bob@bb", "Ann@bb", Outcome::Offline, 1.0);
        assert_eq!(
            sent_to(&back, "bb"),
            [&Message::TellResult {
                id: "t1".into(),
                outcome: Outcome::Offline,
                message: None
            }],
            "the teller is answered as for any character not online"
        );
        let [(world, ask)] = &asked[..] else {
            panic!("one question: {asked:?}")
        };
        assert!(
            world == "bb"
                && matches!(ask, ReachMessage::Ask { from, to, .. }
                    if from.to_string() == "Bob@bb" && to.to_string() == "Ann@bb"),
            "the character's own world is asked, naming the teller of that world: {asked:?}"
        );
        reach_answer(&mut h, "bb", ask, true, 1.1);
        assert_eq!(
            h.take_discord_tells(),
            [DiscordTell {
                person: "p1".into(),
                from: a("Bob@bb"),
                to: a("Ann@bb"),
                text: "hi".into(),
            }]
        );
        let (_, asked) = told(&mut h, "t2", "Bob@bb", "Ann@bb", Outcome::Offline, 2.0);
        reach_answer(&mut h, "bb", &asked[0].1, false, 2.1);
        assert!(
            h.take_discord_tells().is_empty(),
            "not when the world says no"
        );

        let (out, carry) = h.tell_from_discord(&d, "p1", &a("Ann@bb"), &a("Bob@bb"), "soon", 3.0);
        assert_eq!(carry, Carry::Carried);
        let [Message::Tell { id, from, to, .. }] = sent_to(&out, "bb")[..] else {
            panic!("one tell to bb: {out:?}")
        };
        assert_eq!(
            (from.to_string(), to.to_string()),
            ("Ann@bb".into(), "Bob@bb".into())
        );
        h.receive(
            &d,
            "bb",
            Message::TellResult {
                id: id.clone(),
                outcome: Outcome::Delivered,
                message: None,
            },
            3.1,
        );
        assert_eq!(
            h.take_discord_answers(),
            [DiscordAnswer {
                person: "p1".into(),
                from: a("Ann@bb"),
                to: a("Bob@bb"),
                outcome: Outcome::Delivered,
                message: None,
            }]
        );
    }

    /// `Ann@bb`, of the person `p1`, and `Bob@aa`, of the person `p2`.
    struct PairDir;

    impl Directory for PairDir {
        fn lookup(&self, slug: &str) -> Option<DirEntry> {
            dir().lookup(slug)
        }

        fn discord_tells(&self, who: &Address) -> Option<TellsPerson> {
            let person = match who.key().as_str() {
                "ann@bb" => "p1",
                "bob@aa" => "p2",
                _ => return None,
            };
            Some(TellsPerson {
                person: person.into(),
            })
        }
    }

    /// Spec: hub: a person's reply from Discord to a teller not online, who is linked too, goes on to that person's Discord when the teller's world says the replying
    /// character may reach it, as any tell to a character not online does; the replying person is
    /// answered as any teller is, the teller not available, whoever the teller is. Two persons
    /// answering each other so, both away, are held to the paces of their characters' tells and
    /// the loop guard, as any two characters are, and can go on no longer than they would.
    #[test]
    fn a_reply_to_an_away_teller_who_takes_tells_on_discord_goes_to_the_tellers_discord() {
        let d = PairDir;
        let mut h = tells_hub();
        // A reply from `person` as `from` to `to`, whose world answers `offline` and says, asked,
        // whether `from` may reach it: what the replying person is answered, the question asked,
        // and what goes to Discord.
        // What the replying person is answered, the questions asked, and what goes to Discord.
        type Hop = Option<(Outcome, Vec<(String, ReachMessage)>, Vec<DiscordTell>)>;
        let hop =
            |h: &mut Hub, person: &str, from: &str, to: &str, reachable: bool, now: f64| -> Hop {
                let (out, carry) =
                    h.tell_from_discord(&d, person, &a(from), &a(to), "still there?", now);
                if carry != Carry::Carried {
                    return None;
                }
                let target = a(to).world().to_owned();
                let id = sent_to(&out, &target)
                    .into_iter()
                    .find_map(|m| match m {
                        Message::Tell { id, .. } => Some(id.clone()),
                        _ => None,
                    })
                    .expect("the reply goes to the teller's world");
                h.receive(
                    &d,
                    &target,
                    Message::TellResult {
                        id,
                        outcome: Outcome::Offline,
                        message: None,
                    },
                    now,
                );
                let asked = h.take_reach();
                if let Some((world, ask)) = asked.first() {
                    reach_answer(h, world, ask, reachable, now);
                }
                let answers = h.take_discord_answers();
                Some((answers[0].outcome, asked, h.take_discord_tells()))
            };

        let (answer, asked, tells) =
            hop(&mut h, "p1", "Ann@bb", "Bob@aa", true, 1.0).expect("carried");
        assert_eq!(answer, Outcome::Offline, "answered as any teller is");
        assert!(
            matches!(&asked[..], [(w, ReachMessage::Ask { from, to, .. })]
                if w == "aa" && from.to_string() == "Ann@bb" && to.to_string() == "Bob@aa"),
            "the teller's world is asked whether the replying character may reach it: {asked:?}"
        );
        assert_eq!(
            tells,
            [DiscordTell {
                person: "p2".into(),
                from: a("Ann@bb"),
                to: a("Bob@aa"),
                text: "still there?".into(),
            }]
        );
        let (_, _, tells) = hop(&mut h, "p2", "Bob@aa", "Ann@bb", true, 1.5).expect("carried");
        assert_eq!(
            tells,
            [DiscordTell {
                person: "p1".into(),
                from: a("Bob@aa"),
                to: a("Ann@bb"),
                text: "still there?".into(),
            }],
            "and the answer back"
        );
        let (answer, _, tells) =
            hop(&mut h, "p1", "Ann@bb", "Bob@aa", false, 2.0).expect("carried");
        assert_eq!(answer, Outcome::Offline);
        assert!(tells.is_empty(), "the teller's world says no");
        let (answer, asked, tells) =
            hop(&mut h, "p1", "Ann@bb", "Cy@aa", true, 2.5).expect("carried");
        assert_eq!(
            (answer, asked.len(), tells.len()),
            (Outcome::Offline, 0, 0),
            "a teller nobody linked: the same answer, so nothing says the other went anywhere"
        );

        // Back and forth, each within the paces of its character's tells: the loop guard ends it,
        // and nothing goes to Discord past it.
        let mut h = tells_hub();
        let mut went = 0;
        let mut stopped = None;
        for i in 0..60_u32 {
            let now = 10.0 + f64::from(i) * 1.25;
            let (person, from, to) = if i % 2 == 0 {
                ("p1", "Ann@bb", "Bob@aa")
            } else {
                ("p2", "Bob@aa", "Ann@bb")
            };
            match hop(&mut h, person, from, to, true, now) {
                Some((_, _, tells)) => {
                    assert_eq!(tells.len(), 1);
                    went += 1;
                }
                None => {
                    stopped = Some(now);
                    break;
                }
            }
        }
        let pair_burst = Limits::DEFAULT.pair_burst;
        assert_eq!(went, 2 * pair_burst + 1, "the loop guard's burst each way");
        let stopped = stopped.expect("the loop guard ends it");
        assert!(h
            .take_discord_answers()
            .iter()
            .all(|a| a.outcome == Outcome::RateLimited));
        for (person, from, to) in [("p1", "Ann@bb", "Bob@aa"), ("p2", "Bob@aa", "Ann@bb")] {
            assert!(
                hop(&mut h, person, from, to, true, stopped + 60.0).is_none(),
                "{from} is held back while the pair is"
            );
        }
        assert!(h.take_discord_tells().is_empty());
        assert!(
            hop(
                &mut h,
                "p1",
                "Ann@bb",
                "Bob@aa",
                true,
                stopped + Limits::DEFAULT.pair_block_secs + 1.0
            )
            .is_some(),
            "and goes again once the guard lets it"
        );
    }

    // ---- lines said from Discord ---------------------------------------------------------------

    /// Ann@aa's person says `text` in `room` from Discord, under the asker `asker`.
    fn ask(h: &mut Hub, asker: &str, room: &str, who: &str, text: &str, now: f64) {
        h.ask_say(
            &dir(),
            SayAsk {
                asker: asker.into(),
                room: room.into(),
                who: a(who),
                text: text.into(),
                via: "discord".into(),
            },
            now,
        );
    }

    /// The world's answer to the say the hub sent it.
    fn say_answer(
        h: &mut Hub,
        world: &str,
        id: &str,
        outcome: SayOutcome,
        reason: Option<&str>,
    ) -> Vec<Out> {
        h.receive_say(
            world,
            SayMessage::Result {
                id: id.into(),
                outcome,
                reason: reason.map(str::to_owned),
            },
            2.0,
        )
    }

    /// Spec: hub: a line a person said from Discord goes to its character's world alone, as a
    /// `say` naming the room, the character, the text and the mark, and the world's answer, said
    /// or refused with its reason, goes back to the asker alone.
    #[test]
    fn a_line_said_from_discord_goes_to_its_characters_world_and_its_answer_to_the_asker() {
        let mut h = hub_with(&["aa", "bb"]);
        join(&mut h, "aa", "global");
        join(&mut h, "bb", "global");
        ask(&mut h, "i1", "global", "Ann@aa", "evening all", 1.0);
        let asked = h.take_say();
        let [(
            world,
            SayMessage::Ask {
                id,
                room,
                who,
                text,
                via,
            },
        )] = &asked[..]
        else {
            panic!("one say, to Ann's world: {asked:?}");
        };
        assert_eq!(
            (
                world.as_str(),
                room.as_str(),
                who.to_string(),
                text.as_str(),
                via.as_str()
            ),
            ("aa", "global", "Ann@aa".into(), "evening all", "discord")
        );
        assert!(
            h.take_say_answers().is_empty(),
            "nothing until the world answers"
        );
        assert!(
            say_answer(&mut h, "bb", id, SayOutcome::Said, None).is_empty()
                && h.take_say_answers().is_empty(),
            "another world's answer is nobody's"
        );
        say_answer(&mut h, "aa", id, SayOutcome::Said, None);
        assert_eq!(
            h.take_say_answers(),
            [SayAnswer {
                asker: "i1".into(),
                who: a("Ann@aa"),
                said: Said::Said,
            }]
        );
        assert_eq!(h.saying_count(), 0);
        ask(&mut h, "i2", "global", "Ann@aa", "again", 3.0);
        let Some((_, SayMessage::Ask { id, .. })) = h.take_say().pop() else {
            panic!("a say");
        };
        say_answer(&mut h, "aa", &id, SayOutcome::Refused, Some("gagged"));
        assert_eq!(
            h.take_say_answers()[0].said,
            Said::Refused(Some("gagged".into())),
            "the world's reason reaches the asker"
        );
        say_answer(&mut h, "aa", &id, SayOutcome::Said, None);
        assert!(h.take_say_answers().is_empty(), "answered once");
    }

    /// Spec: hub: a world off the network, without its link, or not carrying the room is not
    /// asked, and the asker is answered at once.
    #[test]
    fn a_world_that_cannot_say_the_line_is_not_asked() {
        let mut h = hub_with(&["aa", "off"]);
        join(&mut h, "aa", "global");
        ask(&mut h, "i1", "global", "Ann@cc", "x", 1.0);
        ask(&mut h, "i2", "global", "Ann@off", "x", 1.0);
        ask(&mut h, "i3", "trade", "Ann@aa", "x", 1.0);
        assert!(h.take_say().is_empty(), "no world is asked");
        assert_eq!(
            h.take_say_answers()
                .into_iter()
                .map(|a| (a.asker, a.said))
                .collect::<Vec<_>>(),
            [
                ("i1".into(), Said::Unavailable),
                ("i2".into(), Said::Unavailable),
                ("i3".into(), Said::NotCarried),
            ]
        );
    }

    /// Spec: hub: a line its world does not answer within ten seconds, or whose world's link goes
    /// first, is answered for it; a `say` from a world is refused as a message only the hub sends.
    #[test]
    fn a_line_its_world_never_answers_is_answered_for_it() {
        let d = dir();
        let mut h = hub_with(&["aa", "bb"]);
        join(&mut h, "aa", "global");
        join(&mut h, "bb", "global");
        ask(&mut h, "i1", "global", "Ann@aa", "x", 1.0);
        ask(&mut h, "i2", "global", "Bob@bb", "x", 1.0);
        h.tick(&d, 1.0 + SAY_ANSWER_SECS - 0.5);
        assert!(h.take_say_answers().is_empty());
        h.disconnect("bb");
        assert_eq!(
            h.take_say_answers()
                .into_iter()
                .map(|a| (a.asker, a.said))
                .collect::<Vec<_>>(),
            [("i2".into(), Said::Unavailable)],
            "its link went"
        );
        h.tick(&d, 1.0 + SAY_ANSWER_SECS);
        assert_eq!(
            h.take_say_answers()
                .into_iter()
                .map(|a| (a.asker, a.said))
                .collect::<Vec<_>>(),
            [("i1".into(), Said::NoAnswer)],
            "no answer in time"
        );
        let out = h.receive_say(
            "aa",
            SayMessage::Ask {
                id: "w1".into(),
                room: "global".into(),
                who: a("Ann@aa"),
                text: "x".into(),
                via: "discord".into(),
            },
            20.0,
        );
        assert!(
            matches!(&sent_to(&out, "aa")[..], [Message::Error { code, .. }] if code == "malformed"),
            "{out:?}"
        );
    }

    /// Spec: hub: a line a world says from Discord carries its mark to every other world carrying
    /// the room and to the room's mirrors; a line said in the game carries none.
    #[test]
    fn a_marked_room_line_goes_on_with_its_mark() {
        let d = dir();
        let mut h = hub_with(&["aa", "bb"]);
        join(&mut h, "aa", "global");
        join(&mut h, "bb", "global");
        h.set_bridged([Bridged::Room("global".into())].into());
        let marked = Message::RoomSay {
            id: "r1".into(),
            room: "global".into(),
            from: a("Ann@aa"),
            text: "evening all".into(),
            via: Some("discord".into()),
        };
        let out = h.receive(&d, "aa", marked, 1.0);
        assert_eq!(room_outcome(&out, "aa"), Some(Outcome::Ok));
        assert!(
            matches!(
                &sent_to(&out, "bb")[..],
                [Message::RoomLine { via: Some(v), .. }] if v == "discord"
            ),
            "{out:?}"
        );
        assert_eq!(h.take_bridged()[0].via.as_deref(), Some("discord"));
        let out = h.receive(&d, "aa", room_say("r2", "global", "Ann@aa", "hi"), 2.0);
        assert!(matches!(
            &sent_to(&out, "bb")[..],
            [Message::RoomLine { via: None, .. }]
        ));
        assert_eq!(h.take_bridged()[0].via, None);
    }

    // ---- a world's rooms ---------------------------------------------------------------------

    /// The registry's Global rooms, and every set the routing asked of it.
    struct RoomsDir {
        on: Dir,
        global: std::cell::RefCell<Map<String, String>>,
        asked: std::cell::RefCell<Vec<(String, String, bool)>>,
    }

    impl Directory for RoomsDir {
        fn lookup(&self, slug: &str) -> Option<DirEntry> {
            self.on.lookup(slug)
        }
        fn global_room(&self, slug: &str) -> Option<String> {
            self.global.borrow().get(slug).cloned()
        }
        fn set_global_room(&self, slug: &str, room: &str, seed: bool) -> GlobalSet {
            self.asked
                .borrow_mut()
                .push((slug.to_owned(), room.to_owned(), seed));
            let mut g = self.global.borrow_mut();
            if (seed && g.contains_key(slug)) || g.get(slug).map(String::as_str) == Some(room) {
                return GlobalSet::default();
            }
            g.insert(slug.to_owned(), room.to_owned());
            GlobalSet {
                changed: true,
                mirrors: false,
            }
        }
    }

    fn rooms_dir() -> RoomsDir {
        RoomsDir {
            on: dir(),
            global: std::cell::RefCell::new(Map::new()),
            asked: std::cell::RefCell::new(Vec::new()),
        }
    }

    fn greet(h: &mut Hub, d: &dyn Directory, slug: &str) -> Vec<Out> {
        h.connect(slug, &slug.to_uppercase());
        h.receive(
            d,
            slug,
            Message::Hello {
                proto: PROTOCOL,
                software: "t".into(),
                websocket_url: None,
                status_url: None,
            },
            0.0,
        )
    }

    fn setup(id: Option<&str>, global: Option<&str>, mirrored: &[&str]) -> RoomsMessage {
        RoomsMessage::Setup {
            id: id.map(str::to_owned),
            global: global.map(str::to_owned),
            mirrored: mirrored.iter().map(|r| (*r).to_owned()).collect(),
        }
    }

    fn own(slug: &str, room: &str) -> Bridged {
        Bridged::World {
            slug: slug.into(),
            room: room.into(),
        }
    }

    /// Spec: hub: a world is told its rooms after its welcome (with no Global room while the hub
    /// holds none), and again whenever where it carries the Global room or its own rooms mirrored
    /// change, never the room carrying the Global room among those mirrored, and not when nothing
    /// changed.
    #[test]
    fn a_world_is_told_its_rooms_at_its_welcome_and_whenever_they_change() {
        let d = rooms_dir();
        let mut h = Hub::new("hub test", Channels::default(), Limits::DEFAULT);
        let out = greet(&mut h, &d, "aa");
        assert!(matches!(
            &sent_to(&out, "aa")[..],
            [Message::Welcome { .. }]
        ));
        assert_eq!(h.take_rooms(), [("aa".to_owned(), setup(None, None, &[]))]);
        h.set_bridged(
            [
                own("aa", "general"),
                own("aa", "roleplay"),
                own("bb", "trade"),
            ]
            .into(),
        );
        h.tick(&d, 1.0);
        assert_eq!(
            h.take_rooms(),
            [("aa".to_owned(), setup(None, None, &["general", "roleplay"]))]
        );
        d.global.borrow_mut().insert("aa".into(), "roleplay".into());
        h.rooms_changed();
        h.tick(&d, 2.0);
        assert_eq!(
            h.take_rooms(),
            [("aa".to_owned(), setup(None, Some("roleplay"), &["general"]))],
            "the room carrying Global is never one of its own mirrored"
        );
        h.rooms_changed();
        h.set_bridged([own("aa", "general"), own("aa", "roleplay")].into());
        h.tick(&d, 3.0);
        assert!(h.take_rooms().is_empty(), "nothing changed for aa");
        greet(&mut h, &d, "bb");
        assert_eq!(
            h.take_rooms(),
            [("bb".to_owned(), setup(None, None, &[]))],
            "bb's own rooms are not mirrored now"
        );
        h.disconnect("aa");
        greet(&mut h, &d, "aa");
        assert_eq!(
            h.take_rooms(),
            [("aa".to_owned(), setup(None, Some("roleplay"), &["general"]))],
            "told again at each welcome"
        );
    }

    /// Spec: hub: a world sets where it carries the Global room itself, or seeds it while the hub
    /// holds none, held to its pace of changes; each is answered with its rooms under its id; a
    /// world may not send the hub's own rooms message, nor ask before hello.
    #[test]
    fn a_world_sets_or_seeds_its_global_room_and_is_answered_with_its_rooms() {
        let d = rooms_dir();
        let mut h = Hub::new("hub test", Channels::default(), Limits::DEFAULT);
        h.connect("aa", "AA");
        let early = h.receive_rooms(
            &d,
            "aa",
            RoomsMessage::Global {
                id: "g0".into(),
                room: "trade".into(),
                seed: false,
            },
            0.0,
        );
        assert!(early.contains(&Out::Close("aa".into(), "spoke before hello")));
        greet(&mut h, &d, "aa");
        h.take_rooms();
        let ask = |id: &str, room: &str, seed: bool| RoomsMessage::Global {
            id: id.into(),
            room: room.into(),
            seed,
        };
        h.receive_rooms(&d, "aa", ask("g1", "roleplay", true), 1.0);
        assert_eq!(
            h.take_rooms(),
            [("aa".to_owned(), setup(Some("g1"), Some("roleplay"), &[]))]
        );
        h.receive_rooms(&d, "aa", ask("g2", "trade", true), 2.0);
        assert_eq!(
            h.take_rooms(),
            [("aa".to_owned(), setup(Some("g2"), Some("roleplay"), &[]))],
            "a seed while one is held changes nothing"
        );
        h.receive_rooms(&d, "aa", ask("g3", "trade", false), 3.0);
        assert_eq!(
            h.take_rooms(),
            [("aa".to_owned(), setup(Some("g3"), Some("trade"), &[]))]
        );
        assert_eq!(
            d.asked.borrow().clone(),
            [
                ("aa".to_owned(), "roleplay".to_owned(), true),
                ("aa".to_owned(), "trade".to_owned(), true),
                ("aa".to_owned(), "trade".to_owned(), false),
            ]
        );
        // Past its burst, one every ten seconds.
        for i in 0..4 {
            h.receive_rooms(&d, "aa", ask(&format!("p{i}"), "general", false), 3.0);
        }
        h.take_rooms();
        let asked = d.asked.borrow().len();
        h.receive_rooms(&d, "aa", ask("g4", "lfg", false), 3.0);
        assert_eq!(
            d.asked.borrow().len(),
            asked,
            "past its pace, nothing is asked"
        );
        assert_eq!(
            h.take_rooms(),
            [("aa".to_owned(), setup(Some("g4"), Some("general"), &[]))],
            "and it is answered with its rooms as they are"
        );
        h.tick(&d, 13.0);
        h.receive_rooms(&d, "aa", ask("g5", "lfg", false), 13.0);
        assert_eq!(
            h.take_rooms(),
            [("aa".to_owned(), setup(Some("g5"), Some("lfg"), &[]))]
        );
        let out = h.receive_rooms(&d, "aa", setup(None, Some("off"), &[]), 14.0);
        assert!(
            matches!(&sent_to(&out, "aa")[..], [Message::Error { code, .. }] if code == "malformed"),
            "{out:?}"
        );
    }

    /// Spec: hub: a world that knows nothing of its rooms (an older world) is told them as any
    /// world is, skips them as a type it does not know, and carries the Global room as before.
    #[test]
    fn an_older_world_skips_its_rooms_and_carries_the_global_room_as_before() {
        let d = rooms_dir();
        let mut h = Hub::new("hub test", Channels::default(), Limits::DEFAULT);
        greet(&mut h, &d, "aa");
        greet(&mut h, &d, "bb");
        for (_, m) in h.take_rooms() {
            assert_eq!(
                Message::from_json(&m.to_json()),
                Err(dereth_network_protocol::wire::WireError::Unknown(
                    "rooms".into()
                ))
            );
        }
        join(&mut h, "aa", "global");
        join(&mut h, "bb", "global");
        let out = h.receive(&d, "aa", room_say("r1", "global", "Ann@aa", "hi"), 1.0);
        assert_eq!(room_outcome(&out, "aa"), Some(Outcome::Ok));
        assert!(matches!(
            &sent_to(&out, "bb")[..],
            [Message::RoomLine { .. }]
        ));
        assert!(
            d.asked.borrow().is_empty(),
            "nothing is set for a world that never asks"
        );
    }
}
