//! Not ACE: the Dereth Network's world half, which links this world to other worlds through a
//! hub, so that its players can tell, befriend and talk with characters on other worlds with the
//! unmodified retail client.
//!
//! It is off unless the operator turns it on (`[federation]` in `empyrean.toml`); off, every hook
//! below answers "not mine" and the world behaves as ACE does. On, it reaches the retail client
//! through messages the client already reads:
//!
//! - **Tells.** A tell whose name holds `@` is a network address (`Bob@coldeve`) and goes to the
//!   hub, by name (`Communication_TalkDirectByName`, `0x005D`) or, for a reply, by the stand-in
//!   id the remote sender was shown under (`Communication_TalkDirect`, `0x0032`). A remote tell
//!   arrives as `Communication_HearDirectSpeech` (`0x02BD`) from the sender's stand-in id, which
//!   makes the name clickable and the sender the client's reply target. The sender sees the
//!   echo and errors a local tell gives, once the hub answers.
//! - **Friends.** A remote friend is a row of `Social_FriendsUpdate` (`0x0021`) under its
//!   stand-in id, online as the friend's home world reports, with the online and offline lines.
//! - **Rooms.** The Global room is carried in one of the world's TurbineChat rooms
//!   (`Communication_TurbineChat`, `0xF7DE`), and the world's other rooms its operator mirrors into
//!   Discord go to the hub, both as the hub says ([`rooms`]); a channel is a tell address
//!   (`oldguard@channel`).
//! - **Squelch.** A remote character, a whole world, or (for the operator) any of these for
//!   everyone.
//! - **Persons.** A character may be linked to its player's Discord account, through the hub; the
//!   characters its person friends name are listed as its friends while it is online
//!   ([`persons`]).
//! - **The directory.** A character on the network has its profile in the network's directory,
//!   sent to the hub as it changes ([`directory`]).
//! - **Tells into Discord.** The hub asks whether a character not online would have been shown a
//!   tell before it carries the tell to the character's person elsewhere; the world answers by the
//!   character's squelches and the operator's blocks ([`reach`]).
//! - **Lines said from Discord.** The hub asks the world to say a line its character's person said
//!   in a Discord mirror of a room, as that character; the world says it under the room's own
//!   rules, marked as said there, or answers why not ([`say`]).
//!
//! Every remote line passes the network's text rules first ([`dereth_network_protocol::text`]).
//!
//! The link itself (the WebSocket to the hub) is the host's: the server binary runs it on a thread
//! of its own and calls [`on_link_up`], [`on_link_frame`], [`on_link_down`], [`take_frames`] and
//! [`tick`] on the world thread. A test drives the same five calls. What is to be saved
//! ([`NetworkStore`]) is the host's to write, whenever [`FederationState::dirty`] is set.
//!
//! `/dn verify` is the host's too, over HTTPS ([`take_verify_requests`], [`on_verify_answer`]).
//! While one waits for the hub, the hub challenges the world's game port; the transport keeps the
//! challenge for the world (`ServerNet::hub_challenges`), and [`tick`] hands its nonce to the host
//! to post back ([`take_challenge_answers`]). A challenge with no verification waiting is ignored.

pub mod commands;
pub mod directory;
pub mod persons;
pub mod reach;
mod rooms;
mod say;
mod social;
mod tells;

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Arc;

use dereth_network_protocol::limits::{Bucket, Limits, Rate};
use dereth_network_protocol::standin::StandIns;
use dereth_network_protocol::wire::{GLOBAL_ROOM, PROTOCOL};
use dereth_network_protocol::{Address, CharacterSettings, Message, WorldInfo};
use empyrean_entity::enums::{CharacterOption, ChatMessageType, ChatType, PropertyInt};
use empyrean_entity::ObjectGuid;
use serde::{Deserialize, Serialize};

use crate::entity::turbine_chat_channel;
use crate::World;

pub use dereth_network_protocol::address::looks_remote;
pub use directory::{deleted, renamed};
pub use rooms::{global_slot, mirrored_slots, room_said, KeptRooms, UNCARRIED_SECS};
pub use social::{
    add_friend, friend_rows, on_login, on_logout, remove_all_friends, remove_friend,
    settings_changed, squelch,
};
pub use tells::{tell_by_id, tell_by_name, Routed};

/// The TurbineChat rooms the Global room may be carried in, and the world's own rooms that may go
/// to Discord. The retail client has ten fixed room slots and drops a line for any other room, so
/// the network's room borrows one of the world's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoomSlot {
    General,
    Trade,
    Lfg,
    Roleplay,
}

impl RoomSlot {
    /// The slot named in the configuration.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "general" => Some(Self::General),
            "trade" => Some(Self::Trade),
            "lfg" => Some(Self::Lfg),
            "roleplay" => Some(Self::Roleplay),
            _ => None,
        }
    }

    /// The name the world carries the room under on the link, as one of its own rooms.
    #[must_use]
    pub fn room(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Trade => "trade",
            Self::Lfg => "lfg",
            Self::Roleplay => "roleplay",
        }
    }

    /// The room's name as players know it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Trade => "Trade",
            Self::Lfg => "LFG",
            Self::Roleplay => "Roleplay",
        }
    }

    /// The room's TurbineChat id.
    #[must_use]
    pub fn channel(self) -> u32 {
        match self {
            Self::General => turbine_chat_channel::GENERAL,
            Self::Trade => turbine_chat_channel::TRADE,
            Self::Lfg => turbine_chat_channel::LFG,
            Self::Roleplay => turbine_chat_channel::ROLEPLAY,
        }
    }

    /// The chat type its lines carry.
    #[must_use]
    pub fn chat_type(self) -> ChatType {
        match self {
            Self::General => ChatType::General,
            Self::Trade => ChatType::Trade,
            Self::Lfg => ChatType::LFG,
            Self::Roleplay => ChatType::Roleplay,
        }
    }

    /// The character option that says a player listens to it.
    #[must_use]
    pub fn listen_option(self) -> CharacterOption {
        match self {
            Self::General => CharacterOption::ListenToGeneralChat,
            Self::Trade => CharacterOption::ListenToTradeChat,
            Self::Lfg => CharacterOption::ListenToLFGChat,
            Self::Roleplay => CharacterOption::ListenToRoleplayChat,
        }
    }

    /// The server property that turns the room off for the whole world.
    #[must_use]
    pub fn disable_property(self) -> &'static str {
        match self {
            Self::General => "chat_disable_general",
            Self::Trade => "chat_disable_trade",
            Self::Lfg => "chat_disable_lfg",
            Self::Roleplay => "chat_disable_roleplay",
        }
    }
}

/// The most remote watchers one local character keeps from any one world, and from all worlds
/// together. A watcher the character lists as a friend is always kept.
pub const MAX_WATCHERS_PER_WORLD: usize = 100;
/// See [`MAX_WATCHERS_PER_WORLD`].
pub const MAX_WATCHERS: usize = 1_000;

/// How many new stand-in ids each remote world may cause this world to give: a burst, then a
/// steady rate. A tell, a room line or a channel line from an identity that would need an id past
/// that is not shown, nor is a character of that world a person view names, so one world
/// inventing names cannot grow this world's table without end.
pub const NEW_STAND_INS_PER_WORLD: dereth_network_protocol::limits::Rate =
    dereth_network_protocol::limits::Rate {
        burst: 200.0,
        per_sec: 200.0 / 3_600.0,
    };

/// The most watches and presence answers waiting for the link at once. They go after everything
/// else; a newer one for the same pair of characters takes the waiting one's place, and past this
/// the local character with the most waiting loses its oldest (a reconnect sends them again), so
/// no one character can push out everyone else's.
pub const MAX_CHATTER: usize = 2_000;

/// The most answers this world owes any one other world at once. A tell from a world already owed
/// this many is not shown and not answered (the hub answers it `unavailable`), so a world flooding
/// this one with tells delays only its own answers. Answers go after this world's own tells, a
/// world at a time in turn.
pub const MAX_ANSWERS_PER_WORLD: usize = 100;

/// The most remote friends one character may list (the retail client's own limit for its list).
pub const MAX_REMOTE_FRIENDS: usize = 100;

/// The most remote characters and channels one character may squelch.
pub const MAX_REMOTE_SQUELCHES: usize = 100;

/// The most whole worlds one character may squelch.
pub const MAX_SQUELCHED_WORLDS: usize = 50;

/// How long a stand-in id no character's record names (as a friend or a squelch) is kept after
/// it was last used, in seconds: a day. Past that its identity is forgotten (the id itself is
/// never given again), so the table holds what the world's characters keep and what was seen
/// lately, not everyone ever seen.
pub const STAND_IN_IDLE_SECS: f64 = 24.0 * 3_600.0;

/// The most stand-in ids no record names that the table keeps; past this the least lately used
/// are forgotten first, however recent.
pub const MAX_IDLE_STAND_INS: usize = 10_000;

/// How often the stand-in table is looked over for identities to forget, in seconds.
const RECLAIM_EVERY_SECS: f64 = 60.0;

/// How many status changes a character is shown for one remote friend at once, then how often: a
/// few at once (a friend added, then listing back, then appearing offline, are three), then one
/// every five seconds. A friend's world that changes its answer faster is shown only its latest
/// state, as the pace allows.
pub const PRESENCE_PACE: Rate = Rate {
    burst: 4.0,
    per_sec: 0.2,
};

/// The key under which another world's watches are paced, beside the rooms and local characters
/// [`may_receive`] paces by (it can be neither).
const WATCHES: &str = "@watch";

/// What a character off the network is told when it asks the network for something.
pub(crate) const NOT_ON: &str = "You are not on the Dereth Network. Type @dn on to join it.";

/// The settings `[federation]` gives.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FederationSettings {
    /// The network is on for this world.
    pub enabled: bool,
    /// The world's slug in the registry: the world half of its characters' addresses.
    pub slug: String,
    /// The TurbineChat room the settings carry the Global room in (none: no room), until the hub
    /// says where: the room a hub that holds none is sent once ([`rooms`]).
    pub global_room: Option<RoomSlot>,
    /// The settings name [`FederationSettings::global_room`] themselves, rather than leave it to
    /// its default.
    pub global_room_named: bool,
    /// A character must have played this many seconds before it may send to the network.
    pub min_character_age_secs: u32,
    /// Where a browser reaches the world, told to the hub in its `hello`: its WebSocket endpoint,
    /// when it has one ([`dereth_network_protocol::web`]).
    pub websocket_url: Option<String>,
    /// Its status document, when it serves one at an address it names.
    pub status_url: Option<String>,
}

/// One character's network record: whether it is on the network, its remote friends and squelches.
/// Held shared in [`NetworkStore`], so a copy of the store for its writer copies no record.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterRecord {
    pub settings: CharacterSettings,
    /// Remote friends by identity key.
    #[serde(default)]
    pub friends: BTreeMap<String, Address>,
    /// Remote characters (and channels) this character squelches, by identity key.
    #[serde(default)]
    pub squelched: BTreeMap<String, Address>,
    /// Whole worlds this character squelches.
    #[serde(default)]
    pub squelched_worlds: BTreeSet<String>,
    /// The friends its person friends listed, by identity key, with the handle each came by. They
    /// are among `friends` only while the character is online, and are taken out when it leaves
    /// and when the store is loaded ([`persons`]).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub from_person: BTreeMap<String, Address>,
    /// The character asked the hub for a link code (`/dn link`). A link holds only for a character
    /// that did: one the hub says is linked otherwise was linked under its name before it, by a
    /// character since deleted ([`persons`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub asked_link: bool,
    /// The character's listing in the directory, while it is listed ([`directory`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listing: Option<directory::Listing>,
    /// Why the character was taken out of the directory while it was away, to tell it at its next
    /// login.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taken_out: Option<directory::TakenOut>,
    /// Which of this world's characters it squelched for tells as it last left the world, kept for
    /// a character on the network and linked ([`reach`]); none for one that has not left since.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub away_squelches: Option<LocalSquelches>,
}

/// The tell squelches a character had of its own world's characters as it last left the world:
/// characters by id, accounts by name, or everyone. While it is away the world cannot ask it, so a
/// tell from one of them is answered by these.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalSquelches {
    #[serde(default)]
    pub characters: BTreeSet<u32>,
    #[serde(default)]
    pub accounts: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub everyone: bool,
}

impl LocalSquelches {
    /// Whether a tell from the character `id`, of the account `account`, would be held back.
    #[must_use]
    pub fn covers(&self, id: u32, account: Option<&str>) -> bool {
        self.everyone
            || self.characters.contains(&id)
            || account.is_some_and(|a| self.accounts.contains(a))
    }
}

/// What the network keeps across restarts: the stand-in ids (never reused), each character's
/// record, and the operator's blocks. The table and each record are shared, so a copy of the store
/// (for its writer) copies none of them; the world copies one only to change it while the writer
/// still holds it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkStore {
    #[serde(default)]
    pub stand_ins: Arc<StandIns>,
    /// By character id.
    #[serde(default)]
    pub characters: BTreeMap<u32, Arc<CharacterRecord>>,
    /// The operator's blocks: world slugs, and identity keys of single characters.
    #[serde(default)]
    pub blocked: BTreeSet<String>,
    /// The listings that take a character out of the directory, owed to the hub until it answers
    /// them, by the identity key of the address each names ([`directory`]).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unlistings: BTreeMap<String, Address>,
    /// The key the directory knows each allegiance by, by its monarch's character id: a random
    /// value, kept while the monarch is the same ([`directory`]).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub allegiance_keys: BTreeMap<u32, String>,
    /// The world's rooms as its hub last told them, so that it starts right while the hub is away
    /// ([`rooms`]); none until a hub has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rooms: Option<KeptRooms>,
}

/// Where the link is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LinkState {
    /// No connection. Tells and room lines queue for [`Limits::hold_secs`].
    Down,
    /// Connected; `hello` sent, `welcome` awaited.
    Greeting,
    /// Welcomed.
    Up,
}

/// A queued message and when it was queued.
#[derive(Debug, Clone)]
struct Queued {
    at: f64,
    message: Message,
}

/// An answer this world owes, as its frame for the link, and when it was owed: to another world's
/// tell, or to the hub's question about one ([`reach`]).
#[derive(Debug, Clone)]
struct Owed {
    at: f64,
    frame: String,
}

/// A tell not yet answered.
#[derive(Debug, Clone)]
struct PendingTell {
    player: ObjectGuid,
    to: Address,
    text: String,
    /// When it was handed to the link; none while it waits for one.
    sent: Option<f64>,
}

/// A channel request not yet answered.
#[derive(Debug, Clone)]
struct PendingChannel {
    player: ObjectGuid,
    op: dereth_network_protocol::ChannelOp,
    who: Option<Address>,
    /// When it was handed to the link; none while it waits for one.
    sent: Option<f64>,
}

/// A room line not yet answered.
#[derive(Debug, Clone)]
struct PendingRoom {
    player: ObjectGuid,
    /// The room it was said in, by its name on the link.
    room: String,
    /// When it was handed to the link; none while it waits for one.
    sent: Option<f64>,
    /// Its speaker is told nothing of how the hub took it: its person said it elsewhere ([`say`]).
    quiet: bool,
}

/// Watches and presence answers waiting for the link, each about one local character (a watch's
/// owner, a presence's subject) and for one other world. See [`MAX_CHATTER`]. They go a world at a
/// time in turn, each world's at most at its share of the pace ([`Limits::watches_per_world_pair`],
/// a tenth under), so a world that floods this one with watches delays only its own answers.
#[derive(Debug, Default)]
struct Chatter {
    next: u64,
    /// In order: the local character's identity key, the pair's slot, the world it goes to, and
    /// the message.
    queue: BTreeMap<u64, (String, String, String, Message)>,
    /// The waiting message for each pair, by slot.
    slots: HashMap<String, u64>,
    /// What waits for each local character.
    by_local: HashMap<String, BTreeSet<u64>>,
    /// What waits for each other world, in order.
    by_world: BTreeMap<String, BTreeSet<u64>>,
    /// The world whose message went last.
    turn: String,
    /// What each other world may still be sent now.
    paces: HashMap<String, Bucket>,
}

impl Chatter {
    fn len(&self) -> usize {
        self.queue.len()
    }

    fn clear(&mut self) {
        self.queue.clear();
        self.slots.clear();
        self.by_local.clear();
        self.by_world.clear();
        self.paces.clear();
    }

    /// The local character a message is about, the pair it is for, and the world it goes to.
    fn keys(m: &Message) -> Option<(String, String, String)> {
        match m {
            Message::Watch { owner, target, .. } => Some((
                owner.key(),
                format!("w {} {}", owner.key(), target.key()),
                target.world().to_owned(),
            )),
            Message::Presence { to, of, .. } => Some((
                of.key(),
                format!("p {} {}", of.key(), to.key()),
                to.world().to_owned(),
            )),
            _ => None,
        }
    }

    fn push(&mut self, m: Message) {
        let Some((local, slot, world)) = Self::keys(&m) else {
            return;
        };
        if let Some(seq) = self.slots.get(&slot) {
            if let Some(waiting) = self.queue.get_mut(seq) {
                waiting.3 = m;
            }
            return;
        }
        if self.queue.len() >= MAX_CHATTER {
            let most = self
                .by_local
                .iter()
                .max_by_key(|(_, s)| s.len())
                .and_then(|(_, s)| s.first().copied());
            if let Some(seq) = most {
                self.remove(seq);
            }
        }
        let seq = self.next;
        self.next += 1;
        self.slots.insert(slot.clone(), seq);
        self.by_local.entry(local.clone()).or_default().insert(seq);
        self.by_world.entry(world.clone()).or_default().insert(seq);
        self.queue.insert(seq, (local, slot, world, m));
    }

    fn remove(&mut self, seq: u64) -> Option<Message> {
        let (local, slot, world, m) = self.queue.remove(&seq)?;
        self.slots.remove(&slot);
        if let Some(s) = self.by_local.get_mut(&local) {
            s.remove(&seq);
            if s.is_empty() {
                self.by_local.remove(&local);
            }
        }
        if let Some(s) = self.by_world.get_mut(&world) {
            s.remove(&seq);
            if s.is_empty() {
                self.by_world.remove(&world);
            }
        }
        Some(m)
    }

    /// The next message for the world after the one served last that is within its pace at `t`
    /// (`rate`), if any is.
    fn pop_ready(&mut self, t: f64, rate: Rate) -> Option<Message> {
        let worlds: Vec<String> = self
            .by_world
            .range::<str, _>((
                std::ops::Bound::Excluded(self.turn.as_str()),
                std::ops::Bound::Unbounded,
            ))
            .chain(self.by_world.range::<str, _>((
                std::ops::Bound::Unbounded,
                std::ops::Bound::Included(self.turn.as_str()),
            )))
            .map(|(k, _)| k.clone())
            .collect();
        for world in worlds {
            let pace = self
                .paces
                .entry(world.clone())
                .or_insert_with(|| Bucket::new(rate, t));
            if !pace.take(t) {
                continue;
            }
            let seq = *self.by_world.get(&world)?.first()?;
            self.turn = world;
            return self.remove(seq);
        }
        None
    }

    /// Forgets the paces of worlds that have nothing waiting and have refilled.
    fn settle(&mut self, t: f64) {
        let waiting = &self.by_world;
        self.paces
            .retain(|w, b| waiting.contains_key(w) || !b.full_at(t));
    }
}

/// What a local character's client has been shown of one remote friend, beside what the friend's
/// home world last said. See [`PRESENCE_PACE`].
#[derive(Debug, Clone, Copy, PartialEq)]
struct Status {
    /// What the friend's home world last said.
    online: bool,
    /// What the client was last shown; none until it has been shown this state's first report.
    shown: Option<bool>,
    /// What the client may still be shown now.
    pace: Bucket,
}

/// What a local character's own sends are paced by, before they reach the hub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Sends {
    Tells,
    RoomLines,
    ChannelRequests,
}

/// The network's state on the world.
#[derive(Debug)]
pub struct FederationState {
    pub settings: FederationSettings,
    pub store: NetworkStore,
    /// The store changed and should be saved.
    pub dirty: bool,
    link: LinkState,
    /// The federated worlds the hub has named, by slug.
    worlds: BTreeMap<String, WorldInfo>,
    limits: Limits,
    /// Hello, while greeting.
    control: Vec<Message>,
    /// This world's own tells, room lines, channel requests and heartbeat, in order.
    outbox: VecDeque<Queued>,
    /// The answers owed to each other world, by world: sent after the outbox, a world at a time
    /// in turn, at most [`MAX_ANSWERS_PER_WORLD`] for one world. The hub's questions about a
    /// world's tells wait with that world's answers.
    answers: BTreeMap<String, VecDeque<Owed>>,
    /// The world whose answer went last.
    answered: String,
    /// Watches and presence answers: sent after the answers, at most [`MAX_CHATTER`] waiting.
    chatter: Chatter,
    /// Tells, room lines and channel requests handed to the link and not yet answered, so a link
    /// that goes before they reach the hub sends them again.
    in_flight: Vec<Message>,
    pending: HashMap<String, PendingTell>,
    pending_channels: HashMap<String, PendingChannel>,
    pending_rooms: HashMap<String, PendingRoom>,
    /// The world's own rooms the hub said it does not mirror into Discord, until when their lines
    /// stay here without being sent ([`rooms`]). Emptied at each welcome.
    uncarried: HashMap<String, f64>,
    /// The hub has told the world its rooms on this link.
    rooms_told: bool,
    /// The world has sent its settings' Global room on this link, to a hub that held none.
    seeded: bool,
    /// The world's own requests of the rooms messages, as frames, sent before everything else
    /// while the link is up, and dropped when it goes.
    requests: VecDeque<String>,
    /// The operator's `/dn admin global` waiting for the hub's answer, by the request's id: whom to
    /// tell (the console when none).
    global_asks: HashMap<String, Option<ObjectGuid>>,
    /// The log has said that the settings' Global room differs from the hub's.
    settings_differ_logged: bool,
    /// The last status each remote friend's home world gave each local character that lists it,
    /// and what that character was shown, by character id and the friend's identity key; none
    /// while it is unknown (never reported, or its world or the hub has gone since). A home world
    /// answers each watcher by that watcher's own standing, so one character's answer is never
    /// another's.
    presence: HashMap<(u32, String), Status>,
    /// Statuses whose latest state waits to be shown ([`PRESENCE_PACE`]).
    presence_due: BTreeSet<(u32, String)>,
    /// When each stand-in id was last used (given, shown or named), this run.
    stand_ins_used: HashMap<u32, f64>,
    next_reclaim: f64,
    /// Remote watchers of each local character, by character id.
    watchers: HashMap<u32, BTreeMap<String, Address>>,
    /// What each remote world may still cause in new stand-in ids.
    new_stand_ins: HashMap<String, dereth_network_protocol::limits::Bucket>,
    /// Each local character's own sends, paced as the hub paces them.
    sends: HashMap<(u32, Sends), dereth_network_protocol::limits::Bucket>,
    /// What each remote world may still bring, paced by the hub's per-world limits, by world and
    /// what it brings (a room's lines, or tells to one local character).
    received: HashMap<(String, String), dereth_network_protocol::limits::Bucket>,
    /// The ids of this run start with this, so that a restart reuses none the hub remembers.
    run: String,
    next_id: u64,
    next_status: f64,
    /// What the hub's per-world limit lets this world send now (refilled at its rate).
    pace: Option<dereth_network_protocol::limits::Bucket>,
    /// `/dn verify` requests for the host to send to the hub.
    verify_requests: Vec<VerifyRequest>,
    /// A verification sent and not yet answered.
    verifying: bool,
    /// When the verification waiting was asked for, on the network's clock.
    verify_asked_at: Option<f64>,
    /// The nonces of the hub's challenges answered for the verification waiting.
    answered_nonces: Vec<[u8; dereth_network_protocol::challenge::NONCE_LEN]>,
    /// Answers to the hub's challenges for the host to post.
    challenge_answers: Vec<ChallengeAnswer>,
    /// What the hub's person view last said of each character online, by character id.
    persons: HashMap<u32, persons::PersonState>,
    /// Person requests not yet answered.
    pending_persons: HashMap<String, persons::PendingPerson>,
    /// The directory's listings: what is due, what is on the link, and their paces.
    directory: directory::DirectoryState,
}

/// The answer to a challenge the hub sent this world's game port while a `/dn verify` waits: the
/// host posts its nonce to the hub's answer route over HTTPS, with the world's key, from whatever
/// address its calls leave by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeAnswer {
    /// The challenge's nonce, as 32 hex digits.
    pub nonce: String,
}

/// A `/dn verify` the host sends to the hub's verification route, with the world's key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyRequest {
    /// The token the hub gave the world's operator.
    pub token: String,
    /// Who typed it, for the hub's log.
    pub by: String,
    /// Whom to show the answer to: a character, or the console.
    pub who: Option<ObjectGuid>,
}

impl Default for FederationState {
    fn default() -> Self {
        Self {
            settings: FederationSettings::default(),
            store: NetworkStore::default(),
            dirty: false,
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
            pending_channels: HashMap::new(),
            pending_rooms: HashMap::new(),
            uncarried: HashMap::new(),
            rooms_told: false,
            seeded: false,
            requests: VecDeque::new(),
            global_asks: HashMap::new(),
            settings_differ_logged: false,
            presence: HashMap::new(),
            presence_due: BTreeSet::new(),
            stand_ins_used: HashMap::new(),
            next_reclaim: 0.0,
            watchers: HashMap::new(),
            new_stand_ins: HashMap::new(),
            sends: HashMap::new(),
            received: HashMap::new(),
            run: String::new(),
            next_id: 1,
            next_status: 0.0,
            pace: None,
            verify_requests: Vec::new(),
            verifying: false,
            verify_asked_at: None,
            answered_nonces: Vec::new(),
            challenge_answers: Vec::new(),
            persons: HashMap::new(),
            pending_persons: HashMap::new(),
            directory: directory::DirectoryState::default(),
        }
    }
}

impl FederationState {
    /// Where the link is.
    #[must_use]
    pub fn link(&self) -> LinkState {
        self.link
    }

    /// How many messages wait for the link.
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
    pub fn watcher_count(&self, character: u32) -> usize {
        self.watchers.get(&character).map_or(0, BTreeMap::len)
    }

    /// How many presence states this world holds, for all its characters together.
    #[must_use]
    pub fn presence_count(&self) -> usize {
        self.presence.len()
    }

    /// The status a local character's client was last shown for a remote friend, by the friend's
    /// identity key; none while it has been shown none.
    #[must_use]
    pub fn shown_status(&self, character: u32, friend_key: &str) -> Option<bool> {
        self.presence
            .get(&(character, friend_key.to_owned()))
            .and_then(|s| s.shown)
    }

    /// A character's record, if it has one.
    #[must_use]
    pub fn record(&self, character: u32) -> Option<&CharacterRecord> {
        self.store.characters.get(&character).map(|r| &**r)
    }
}

/// The world's clock, in seconds, as the network counts time.
fn now(w: &World) -> f64 {
    w.now.monotonic.as_secs_f64()
}

/// Whether the network is on for this world.
#[must_use]
pub fn enabled(w: &World) -> bool {
    w.federation.settings.enabled && !w.federation.settings.slug.is_empty()
}

/// Turns the network on with `settings` and the saved `store` (the host's start-up).
pub fn configure(w: &mut World, settings: FederationSettings, mut store: NetworkStore) {
    persons::strip_saved(&mut store);
    w.federation.settings = settings;
    w.federation.store = store;
    w.federation.run = run_prefix(w);
    // The hub's challenges on the game port are kept for the world from now on.
    w.net.hub_challenges = Some(VecDeque::new());
}

/// This run's id prefix: the start time in whole seconds.
fn run_prefix(w: &World) -> String {
    run_prefix_of(w.now.unix_time)
}

fn run_prefix_of(unix_time: f64) -> String {
    format!("{unix_time:.0}")
}

fn fresh_id(w: &mut World, kind: char) -> String {
    let unix_time = w.now.unix_time;
    let f = &mut w.federation;
    if f.run.is_empty() {
        f.run = run_prefix_of(unix_time);
    }
    let id = format!("{kind}{}-{}", f.run, f.next_id);
    f.next_id += 1;
    id
}

/// Queues a message for the hub: sent at once when the link is up, else held for
/// [`Limits::hold_secs`].
fn send(w: &mut World, message: Message) {
    let at = now(w);
    w.federation.outbox.push_back(Queued { at, message });
}

/// Sends a watch or a presence answer, only while the link is up (a reconnect sends them again),
/// after everything else waiting. Nothing goes to a world or character the operator blocks.
fn send_if_up(w: &mut World, message: Message) {
    let to_blocked = match &message {
        Message::Watch { target, .. } => blocked(w, target),
        Message::Presence { to, .. } => blocked(w, to),
        _ => false,
    };
    if !to_blocked {
        answer_if_up(w, message);
    }
}

/// [`send_if_up`] for the answer to a watch, which goes to a blocked watcher too: it is the same
/// `offline` any watch for a character the watcher may not see is answered with.
fn answer_if_up(w: &mut World, message: Message) {
    let f = &mut w.federation;
    if f.link == LinkState::Up {
        f.chatter.push(message);
    }
}

/// Whether this world may take on one more answer for `world` now.
fn may_owe(w: &World, world: &str) -> bool {
    w.federation
        .answers
        .get(world)
        .is_none_or(|q| q.len() < MAX_ANSWERS_PER_WORLD)
}

/// Queues an answer owed to `world`.
fn send_answer(w: &mut World, world: &str, message: &Message) {
    owe(w, world, message.to_json());
}

/// Queues the frame of an answer owed to `world`, or to the hub about one of `world`'s tells.
fn owe(w: &mut World, world: &str, frame: String) {
    let at = now(w);
    w.federation
        .answers
        .entry(world.to_owned())
        .or_default()
        .push_back(Owed { at, frame });
}

/// The next answer owed, from the world after the one answered last.
fn next_answer(f: &mut FederationState) -> Option<String> {
    let world = f
        .answers
        .range::<str, _>((
            std::ops::Bound::Excluded(f.answered.as_str()),
            std::ops::Bound::Unbounded,
        ))
        .chain(f.answers.iter())
        .find(|(_, q)| !q.is_empty())
        .map(|(k, _)| k.clone())?;
    let q = f.answers.get_mut(&world)?;
    let m = q.pop_front().map(|q| q.frame);
    if q.is_empty() {
        f.answers.remove(&world);
    }
    f.answered = world;
    m
}

/// This world's address for a local character in the world.
fn own_address(w: &World, player: ObjectGuid) -> Option<Address> {
    w.objects.get(player)?;
    let name = crate::dispatch::name::name(w, player)?;
    Address::new(&name, &w.federation.settings.slug).ok()
}

/// Whether a local character's own send of `kind` is within the hub's pace for it; takes one.
fn may_send(w: &mut World, player: ObjectGuid, kind: Sends) -> bool {
    let t = now(w);
    let f = &mut w.federation;
    let rate = match kind {
        Sends::Tells | Sends::ChannelRequests => f.limits.tells_per_character,
        Sends::RoomLines => f.limits.room_lines_per_character,
    };
    f.sends
        .entry((player.full(), kind))
        .or_insert_with(|| dereth_network_protocol::limits::Bucket::new(rate, t))
        .take(t)
}

/// Whether another world is within the hub's per-world limit `rate` for what it brings (`what`:
/// a room, or a local character it tells); takes one. The hub enforces the same limits, so this
/// holds only for a hub that does not.
fn may_receive(
    w: &mut World,
    world: &str,
    what: &str,
    rate: dereth_network_protocol::limits::Rate,
) -> bool {
    let t = now(w);
    w.federation
        .received
        .entry((world.to_owned(), what.to_owned()))
        .or_insert_with(|| dereth_network_protocol::limits::Bucket::new(rate, t))
        .take(t)
}

/// Whether a local character is younger than the operator's minimum age for sending to the
/// network.
fn too_new(w: &World, player: ObjectGuid) -> bool {
    let min_age = w.federation.settings.min_character_age_secs;
    min_age > 0
        && w.objects
            .get(player)
            .and_then(crate::world_objects::world_object::WorldObject::age)
            .is_none_or(|age| i64::from(age) < i64::from(min_age))
}

/// [`too_new`] for a character whether or not it is online.
fn too_new_player(w: &World, p: crate::entity::i_player::IPlayer) -> bool {
    let min_age = w.federation.settings.min_character_age_secs;
    min_age > 0
        && crate::entity::i_player::get_property(w, p, PropertyInt::Age)
            .is_none_or(|age| i64::from(age) < i64::from(min_age))
}

fn record_mut(w: &mut World, character: u32) -> &mut CharacterRecord {
    Arc::make_mut(w.federation.store.characters.entry(character).or_default())
}

/// A character's record to change, if it has one.
fn existing_record_mut(w: &mut World, character: u32) -> Option<&mut CharacterRecord> {
    w.federation
        .store
        .characters
        .get_mut(&character)
        .map(Arc::make_mut)
}

/// Whether a local character is on the network.
fn is_on(w: &World, character: u32) -> bool {
    w.federation
        .store
        .characters
        .get(&character)
        .is_some_and(|r| r.settings.on)
}

/// The stand-in id of a remote identity, given now if it has none (the store is marked to save).
/// None once the block is used up: an identity with no id of its own is never shown, rather than
/// shown under another's.
fn stand_in(w: &mut World, who: &Address) -> Option<u32> {
    let t = now(w);
    let f = &mut w.federation;
    let unchanged = f.store.stand_ins.get(who).filter(|&id| {
        f.store
            .stand_ins
            .who(id)
            .is_some_and(|a| a.to_string() == who.to_string())
    });
    let id = match unchanged {
        Some(id) => Some(id),
        None => {
            let id = Arc::make_mut(&mut f.store.stand_ins).id_for(who).ok();
            f.dirty = true;
            id
        }
    };
    match id {
        Some(id) => {
            f.stand_ins_used.insert(id, t);
        }
        None => log::warn!("Dereth Network: every stand-in id has been given; {who} is not shown"),
    }
    id
}

/// The identity a stand-in id stands for, marked used now.
fn stand_in_for(w: &mut World, id: u32) -> Option<Address> {
    let who = w.federation.store.stand_ins.who(id).cloned()?;
    let t = now(w);
    w.federation.stand_ins_used.insert(id, t);
    Some(who)
}

/// Forgets the identities of stand-in ids no record names and nobody has used for
/// [`STAND_IN_IDLE_SECS`], and the least lately used past [`MAX_IDLE_STAND_INS`]. Their ids are
/// never given again: an id a client still holds names no one, never someone else.
fn reclaim_stand_ins(w: &mut World, t: f64) {
    let f = &mut w.federation;
    let named: BTreeSet<String> = f
        .store
        .characters
        .values()
        .flat_map(|r| r.friends.keys().chain(r.squelched.keys()).cloned())
        .collect();
    let mut idle: Vec<(f64, u32)> = Vec::new();
    for (id, who) in f.store.stand_ins.iter() {
        if named.contains(&who.key()) {
            continue;
        }
        // An id from before this run is counted as used when the run began.
        let used = *f.stand_ins_used.entry(id).or_insert(t);
        idle.push((used, id));
    }
    idle.sort_by(|a, b| a.0.total_cmp(&b.0));
    let over = idle.len().saturating_sub(MAX_IDLE_STAND_INS);
    let forget: Vec<u32> = idle
        .iter()
        .enumerate()
        .filter(|(i, (used, _))| *i < over || t - used >= STAND_IN_IDLE_SECS)
        .map(|(_, (_, id))| *id)
        .collect();
    if forget.is_empty() {
        return;
    }
    let table = Arc::make_mut(&mut f.store.stand_ins);
    for id in &forget {
        table.forget(*id);
        f.stand_ins_used.remove(id);
    }
    f.dirty = true;
    log::debug!(
        "Dereth Network: forgot {} stand-in id(s) nothing names any more",
        forget.len()
    );
}

/// Why an identity another world brought is not shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotShown {
    /// Its world has brought all the new names it may for now.
    Budget,
    /// Every stand-in id has been given.
    Exhausted,
}

/// The stand-in id of an identity another world brought here (a tell, a room line or a channel
/// line from it, or a person view naming it), as [`stand_in`]; a new one only while that world is
/// within [`NEW_STAND_INS_PER_WORLD`].
fn remote_stand_in(w: &mut World, who: &Address) -> Result<u32, NotShown> {
    if let Some(id) = w.federation.store.stand_ins.get(who) {
        // The spelling as last written is what a reply or a row shows.
        return Ok(stand_in(w, who).unwrap_or(id));
    }
    let t = now(w);
    let room = w
        .federation
        .new_stand_ins
        .entry(who.world().to_owned())
        .or_insert_with(|| dereth_network_protocol::limits::Bucket::new(NEW_STAND_INS_PER_WORLD, t))
        .take(t);
    if !room {
        log::debug!(
            "Dereth Network: {} brought too many new names; one is not shown",
            who.world()
        );
        return Err(NotShown::Budget);
    }
    stand_in(w, who).ok_or(NotShown::Exhausted)
}

/// A line to one player, as the server's own system text.
fn tell_player(w: &mut World, player: ObjectGuid, text: &str) {
    crate::world_objects::player::send_message(w, player, text, ChatMessageType::Broadcast);
}

/// A line for whoever typed an operator's command: a character while it is online, else the log
/// (the console).
fn tell_operator(w: &mut World, who: Option<ObjectGuid>, line: &str) {
    match who {
        Some(player)
            if crate::managers::player_manager::get_online_player(w, player.full()).is_some() =>
        {
            tell_player(w, player, line);
        }
        _ => log::info!("Dereth Network: {line}"),
    }
}

/// A name a player gave (a tell's target, a squelch, a friend), without the mark a world shows
/// beside the speaker of a line said from Discord: `Ann (Discord)` is `Ann`, and `Ann@coldeve
/// (Discord)` is `Ann@coldeve`. No character's name holds a parenthesis, so a marked name a player
/// clicks or types names the character. With the network off, the name as it was given.
#[must_use]
pub fn given_name<'a>(w: &World, name: &'a str) -> &'a str {
    if enabled(w) {
        dereth_network_protocol::say::without_mark(name)
    } else {
        name
    }
}

/// Whether the operator blocks `who` (the character, or its whole world).
fn blocked(w: &World, who: &Address) -> bool {
    let b = &w.federation.store.blocked;
    b.contains(who.world()) || b.contains(&who.key())
}

/// The `/dn verify` requests for the host to send, oldest first.
pub fn take_verify_requests(w: &mut World) -> Vec<VerifyRequest> {
    std::mem::take(&mut w.federation.verify_requests)
}

/// The answers to the hub's challenges for the host to post, oldest first.
pub fn take_challenge_answers(w: &mut World) -> Vec<ChallengeAnswer> {
    std::mem::take(&mut w.federation.challenge_answers)
}

/// The hub's challenges the transport kept since the last turn. Each is answered only while a
/// `/dn verify` this world sent waits for the hub, within
/// [`WINDOW_SECS`](dereth_network_protocol::challenge::WINDOW_SECS) of its being typed, for this
/// world's own slug, once for each nonce and for at most
/// [`MAX_ANSWERS`](dereth_network_protocol::challenge::MAX_ANSWERS) nonces; any other is ignored.
/// It is never answered over UDP.
fn answer_hub_challenges(w: &mut World) {
    use dereth_network_protocol::challenge::{nonce_hex, MAX_ANSWERS, WINDOW_SECS};
    let Some(heard) = w.net.hub_challenges.as_mut().map(std::mem::take) else {
        return;
    };
    let t = now(w);
    let f = &mut w.federation;
    let waiting = f.verifying && f.verify_asked_at.is_some_and(|at| t - at <= WINDOW_SECS);
    for c in heard {
        if !waiting
            || c.slug != f.settings.slug
            || f.answered_nonces.contains(&c.nonce)
            || f.answered_nonces.len() >= MAX_ANSWERS
        {
            continue;
        }
        f.answered_nonces.push(c.nonce);
        f.challenge_answers.push(ChallengeAnswer {
            nonce: nonce_hex(&c.nonce),
        });
    }
}

/// The hub's answer to a `/dn verify`, as a line for whoever typed it.
pub fn on_verify_answer(w: &mut World, who: Option<ObjectGuid>, line: &str) {
    w.federation.verifying = false;
    w.federation.verify_asked_at = None;
    tell_operator(w, who, line);
}

// ---- the link ------------------------------------------------------------------------------------

/// The host connected to the hub: `hello` goes first, before anything queued.
pub fn on_link_up(w: &mut World) {
    let f = &mut w.federation;
    f.link = LinkState::Greeting;
    f.control = vec![Message::Hello {
        proto: PROTOCOL,
        software: concat!("empyrean ", env!("CARGO_PKG_VERSION")).to_owned(),
        websocket_url: f.settings.websocket_url.clone(),
        status_url: f.settings.status_url.clone(),
    }];
    log::info!("Dereth Network: connected to the hub; greeting");
}

/// The host lost the hub: queued messages wait, and every remote friend shows offline. What was
/// handed to the link and not yet answered waits again at the front, since it may never have
/// reached the hub (which drops a repeat of one that did).
pub fn on_link_down(w: &mut World) {
    if w.federation.link == LinkState::Down {
        return;
    }
    let t = now(w);
    rooms::link_down(w);
    let f = &mut w.federation;
    f.link = LinkState::Down;
    f.control.clear();
    f.chatter.clear();
    // The hub answers for this world every tell it had not answered when the link went.
    f.answers.clear();
    for m in std::mem::take(&mut f.in_flight).into_iter().rev() {
        let waiting = match &m {
            Message::Tell { id, .. } => f.pending.get_mut(id).map(|p| p.sent = None).is_some(),
            Message::Channel { id, .. } => f
                .pending_channels
                .get_mut(id)
                .map(|p| p.sent = None)
                .is_some(),
            Message::RoomSay { id, .. } => {
                f.pending_rooms.get_mut(id).map(|p| p.sent = None).is_some()
            }
            Message::Person { id, .. } => f
                .pending_persons
                .get_mut(id)
                .map(|p| p.sent = None)
                .is_some(),
            _ => false,
        };
        if waiting {
            f.outbox.push_front(Queued { at: t, message: m });
        }
    }
    log::info!("Dereth Network: the link to the hub is down");
    directory::link_down(w);
    let worlds: Vec<String> = w.federation.worlds.keys().cloned().collect();
    for slug in worlds {
        social::world_gone(w, &slug);
    }
    // Every other world watches again when this one is back.
    w.federation.watchers.clear();
    w.federation.presence.clear();
    w.federation.presence_due.clear();
    for info in w.federation.worlds.values_mut() {
        info.online = false;
    }
}

/// What to send now: `hello` while greeting; once welcomed, what is queued, as fast as the hub's
/// per-world limit allows (the rest waits for the next call, so a burst of watches at a reconnect
/// is never dropped at the hub). This world's own tells, room lines and channel requests go first,
/// then the answers it owes other worlds (a world at a time in turn), then watches and presence
/// answers, so no number of the later can hold up the earlier; the directory's listings take what
/// the pace leaves.
#[must_use]
pub fn take_frames(w: &mut World) -> Vec<String> {
    let t = now(w);
    let mut out = take_link_frames(w, t);
    if w.federation.link == LinkState::Up {
        directory::take(w, t, &mut out);
    }
    out
}

/// [`take_frames`] for the link's own messages.
fn take_link_frames(w: &mut World, t: f64) -> Vec<String> {
    let f = &mut w.federation;
    match f.link {
        LinkState::Down => Vec::new(),
        LinkState::Greeting => f.control.drain(..).map(|m| m.to_json()).collect(),
        LinkState::Up => {
            let rate = f.limits.messages_per_world;
            // A tenth under the hub's own numbers, so the two clocks never disagree at the edge.
            let rate = dereth_network_protocol::limits::Rate {
                burst: (rate.burst * 0.9).max(1.0),
                per_sec: rate.per_sec * 0.9,
            };
            let mut pace = *f
                .pace
                .get_or_insert_with(|| dereth_network_protocol::limits::Bucket::new(rate, t));
            let share = f.limits.watches_per_world_pair();
            let share = dereth_network_protocol::limits::Rate {
                burst: (share.burst * 0.9).max(1.0),
                per_sec: share.per_sec * 0.9,
            };
            let mut out = Vec::new();
            while pace.has(t) {
                if let Some(frame) = f.requests.pop_front() {
                    pace.take(t);
                    out.push(frame);
                    continue;
                }
                let message = match f.outbox.pop_front() {
                    Some(q) => q.message,
                    None => {
                        if let Some(frame) = next_answer(f) {
                            pace.take(t);
                            out.push(frame);
                            continue;
                        }
                        let Some(m) = f.chatter.pop_ready(t, share) else {
                            break;
                        };
                        m
                    }
                };
                pace.take(t);
                match &message {
                    Message::Tell { id, .. } => {
                        if let Some(p) = f.pending.get_mut(id) {
                            p.sent = Some(t);
                        }
                        f.in_flight.push(message.clone());
                    }
                    Message::Channel { id, .. } => {
                        if let Some(p) = f.pending_channels.get_mut(id) {
                            p.sent = Some(t);
                        }
                        f.in_flight.push(message.clone());
                    }
                    Message::RoomSay { id, .. } => {
                        if let Some(p) = f.pending_rooms.get_mut(id) {
                            p.sent = Some(t);
                        }
                        f.in_flight.push(message.clone());
                    }
                    Message::Person { id, .. } => {
                        if let Some(p) = f.pending_persons.get_mut(id) {
                            p.sent = Some(t);
                        }
                        f.in_flight.push(message.clone());
                    }
                    _ => {}
                }
                out.push(message.to_json());
            }
            f.pace = Some(pace);
            out
        }
    }
}

/// Forgets in-flight messages that have been answered or given up on.
fn settle_in_flight(f: &mut FederationState) {
    let (pending, channels, rooms, persons) = (
        &f.pending,
        &f.pending_channels,
        &f.pending_rooms,
        &f.pending_persons,
    );
    f.in_flight.retain(|m| match m {
        Message::Tell { id, .. } => pending.contains_key(id),
        Message::Channel { id, .. } => channels.contains_key(id),
        Message::RoomSay { id, .. } => rooms.contains_key(id),
        Message::Person { id, .. } => persons.contains_key(id),
        _ => false,
    });
}

/// One frame from the hub: a link message, or one of the directory's, the reach, the say or the
/// rooms messages, families of their own.
pub fn on_link_frame(w: &mut World, text: &str) {
    use dereth_network_protocol::wire::WireError;
    let skipped = |e: WireError| log::debug!("Dereth Network: skipped a frame from the hub: {e}");
    match Message::from_json(text) {
        Ok(m) => return on_message(w, m),
        Err(WireError::Unknown(_)) => {}
        Err(e) => return skipped(e),
    }
    match dereth_network_protocol::DirectoryMessage::from_json(text) {
        Ok(m) => return directory::on_message(w, m),
        Err(WireError::Unknown(_)) => {}
        Err(e) => return skipped(e),
    }
    match dereth_network_protocol::ReachMessage::from_json(text) {
        Ok(m) => return reach::on_message(w, m),
        Err(WireError::Unknown(_)) => {}
        Err(e) => return skipped(e),
    }
    match dereth_network_protocol::SayMessage::from_json(text) {
        Ok(m) => return say::on_message(w, m),
        Err(WireError::Unknown(_)) => {}
        Err(e) => return skipped(e),
    }
    match dereth_network_protocol::RoomsMessage::from_json(text) {
        Ok(m) => rooms::on_message(w, m),
        Err(e) => skipped(e),
    }
}

fn on_message(w: &mut World, m: Message) {
    match m {
        Message::Welcome {
            world,
            worlds,
            limits,
            rooms,
            ..
        } => {
            if world != w.federation.settings.slug {
                log::warn!(
                    "Dereth Network: the hub knows this world as {world:?}, not {:?}",
                    w.federation.settings.slug
                );
            }
            let f = &mut w.federation;
            f.link = LinkState::Up;
            f.limits = limits;
            f.pace = None;
            f.worlds = worlds
                .into_iter()
                .filter(|i| !i.slug.is_empty())
                .map(|i| (i.slug.clone(), i))
                .collect();
            log::info!(
                "Dereth Network: welcomed; {} world(s) online",
                f.worlds.values().filter(|i| i.online).count()
            );
            // The rooms go first, then what was queued; the watches of everyone online, for the
            // worlds online now, go after (a world the hub names later is watched when it does).
            // The rooms are those the hub last told the world, until it tells them again.
            let queued: Vec<Queued> = f.outbox.drain(..).collect();
            if global_slot(w).is_some() && rooms.iter().any(|r| r == GLOBAL_ROOM) {
                send(
                    w,
                    Message::RoomJoin {
                        room: GLOBAL_ROOM.into(),
                        on: true,
                    },
                );
            }
            rooms::join_own(w);
            w.federation.outbox.extend(queued);
            let online: Vec<String> = w
                .federation
                .worlds
                .values()
                .filter(|i| i.online)
                .map(|i| i.slug.clone())
                .collect();
            for slug in online {
                social::watch_everyone(w, Some(&slug));
            }
            persons::watch_everyone(w);
            directory::welcomed(w);
        }
        Message::World(info) => {
            let online = info.online;
            let slug = info.slug.clone();
            let was = w.federation.worlds.get(&slug).is_some_and(|i| i.online);
            w.federation.worlds.insert(slug.clone(), info);
            if online && !was {
                social::watch_everyone(w, Some(&slug));
            } else if !online {
                social::world_gone(w, &slug);
            }
        }
        Message::Watch { owner, target, on } => social::on_watch(w, owner, &target, on),
        Message::Presence { to, of, online } => social::on_presence(w, &to, &of, online),
        Message::Tell {
            id, from, to, text, ..
        } => tells::on_tell(w, id, &from, &to, &text),
        Message::TellResult {
            id,
            outcome,
            message,
        } => tells::on_result(w, &id, outcome, message.as_deref()),
        Message::RoomLine {
            room,
            from,
            text,
            via,
        } => rooms::on_line(w, &room, &from, &text, via.as_deref()),
        Message::RoomResult { id, outcome } => rooms::on_result(w, &id, outcome),
        Message::ChannelResult {
            id,
            outcome,
            channel,
            members,
            channels,
        } => commands::on_channel_result(w, &id, outcome, &channel, &members, &channels),
        Message::ChannelInvite { channel, by, to } => {
            commands::on_channel_invite(w, &channel, &by, &to)
        }
        Message::ChannelLine {
            channel,
            from,
            to,
            text,
        } => tells::on_channel_line(w, &channel, &from, &to, &text),
        Message::PersonResult {
            id,
            outcome,
            code,
            expires_secs,
            url,
            ..
        } => persons::on_result(
            w,
            &id,
            outcome,
            code.as_deref(),
            expires_secs,
            url.as_deref(),
        ),
        Message::PersonFriends {
            to,
            linked,
            persons,
            ..
        } => persons::on_view(w, &to, linked, &persons),
        Message::Error { code, message } => {
            log::warn!("Dereth Network: the hub reports {code}: {message}");
        }
        Message::Hello { .. }
        | Message::Status { .. }
        | Message::RoomJoin { .. }
        | Message::RoomSay { .. }
        | Message::Channel { .. }
        | Message::Person { .. }
        | Message::PersonWatch { .. } => {}
    }
}

/// The passage of time: queued messages and unanswered requests whose time is up are given up on,
/// and the heartbeat goes once a minute while the link is up.
pub fn tick(w: &mut World) {
    if !enabled(w) {
        return;
    }
    answer_hub_challenges(w);
    let t = now(w);
    let hold = w.federation.limits.hold_secs;
    let timeout = w.federation.limits.result_timeout_secs;
    // Queued too long: dropped, and a tell's sender told.
    let mut expired = Vec::new();
    w.federation.outbox.retain(|q| {
        let keep = t - q.at < hold;
        if !keep {
            expired.push(q.message.clone());
        }
        keep
    });
    // An answer the hub has stopped waiting for is of no use to anyone.
    for q in w.federation.answers.values_mut() {
        q.retain(|a| t - a.at < dereth_network_protocol::ANSWER_SECS);
    }
    w.federation.answers.retain(|_, q| !q.is_empty());
    for m in expired {
        match m {
            Message::Tell { id, .. } => tells::give_up(w, &id, true),
            Message::Channel { id, .. } => commands::give_up_channel(w, &id),
            Message::RoomSay { id, .. } => {
                w.federation.pending_rooms.remove(&id);
            }
            Message::Person { id, .. } => persons::give_up(w, &id),
            _ => {}
        }
    }
    // A request is given up on only once it has been with the hub longer than the hub may take
    // to answer it (its hold for a reconnecting world and the target's answer window together).
    let late = |sent: Option<f64>| sent.is_some_and(|s| t - s >= timeout);
    let late_tells: Vec<String> = w
        .federation
        .pending
        .iter()
        .filter(|(_, p)| late(p.sent))
        .map(|(k, _)| k.clone())
        .collect();
    for id in late_tells {
        tells::give_up(w, &id, w.federation.link != LinkState::Up);
    }
    let late_channels: Vec<String> = w
        .federation
        .pending_channels
        .iter()
        .filter(|(_, p)| late(p.sent))
        .map(|(k, _)| k.clone())
        .collect();
    for id in late_channels {
        commands::give_up_channel(w, &id);
    }
    for id in persons::late(w, timeout) {
        persons::give_up(w, &id);
    }
    w.federation.pending_rooms.retain(|_, p| !late(p.sent));
    settle_in_flight(&mut w.federation);
    w.federation.sends.retain(|_, b| !b.full_at(t));
    w.federation.received.retain(|_, b| !b.full_at(t));
    w.federation.new_stand_ins.retain(|_, b| !b.full_at(t));
    w.federation.chatter.settle(t);
    social::show_due(w, t);
    directory::tick(w);
    if t >= w.federation.next_reclaim {
        w.federation.next_reclaim = t + RECLAIM_EVERY_SECS;
        reclaim_stand_ins(w, t);
    }
    if w.federation.link == LinkState::Up && t >= w.federation.next_status {
        w.federation.next_status = t + 60.0;
        let players = crate::managers::player_manager::get_online_count(w);
        let open =
            w.world_manager.world_status == crate::managers::world_manager::WorldStatusState::Open;
        send(
            w,
            Message::Status {
                state: if open { "online" } else { "maintenance" }.into(),
                players_online: u32::try_from(players).ok(),
            },
        );
    }
}

/// The friend-list line a status change gives, with the name a link the client can click to
/// start a tell (the retail client draws name links in any line, server text included).
fn presence_line(id: u32, name: &str, online: bool) -> String {
    format!(
        "<Tell:IIDString:{id}:{name}>{name}<\\Tell> has {}line.",
        if online { "come on" } else { "gone off" }
    )
}
