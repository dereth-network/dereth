//! The world's half of the Dereth Network's link, sans-IO: what an Asheron's Call 2 world says to
//! the hub and does with what the hub says, so that its players reach characters on other worlds,
//! of either game.
//!
//! **Depends on** the network's protocol (`dereth-network-protocol`: the link's messages,
//! addresses, the text rules, the rate limits and the hub's challenge), the store (`yalain-store`:
//! each character's setting and the operator's blocks) and `serde_json` (the hub's answer to a
//! verification). **Used by** the server executable (`yalain`), which carries
//! the frames over the link and feeds the clock.
//!
//! **Must never** touch a socket, a file or the clock: the host hands it the link's events and the
//! time, in seconds on any monotonic scale, and takes the frames it has to send. It knows nothing
//! of the game: the friends service, which faces the players, answers what it asks and shows what
//! it delivers through [`Friends`].
//!
//! **Specified in** `docs/network/02-link-protocol.md` (the messages, the order of a link, who
//! answers what, the rate limits), `03-world-behaviour.md` (the rules a world keeps: sections 1, 2,
//! 3, 4, 5, 7, 8, 9 and 10) and `04-text-rules.md`. It carries `hello`, `welcome`, `world`, `watch`,
//! `presence`, `tell`, `tell.result`, `error`, the channels', the persons', `reach`, the directory's `listing`, `listing.result` and `listing.off`, and the rooms'
//! `rooms`, `rooms.global`, `room.join`, `room.say`, `room.line`, `room.result`, `say` and
//! `say.result`.
//!
//! - **On the network.** Every character starts off it; its player puts it on (and the setting is
//!   kept in the store). Only a character on the network is seen online by other worlds, may tell
//!   them and be told by them. Friends and tells within the world are the friends service's alone
//!   and need none of this.
//! - **Present.** A character is present while it is in the world with a friends stream: only
//!   then is it online to other worlds, and only then can a tell be shown to it.
//! - **Tells** ([`Federation::tell`]) go to the hub and end, once its answer comes, with the
//!   outcome and the line the teller is shown ([`TellEnd`]). A tell from another world is shown
//!   by the friends service ([`Friends::show_tell`]) and answered at once. A tell to an away,
//!   linked character of this world goes the same way ([`Federation::away_linked`]).
//! - **Presence.** While a character on the network is present, its world watches each friend
//!   it lists on another linked world; the friend's world answers, and each change reaches the
//!   character ([`Friends::show_presence`]). Another world's watch of one of this world's
//!   characters is answered by this world's own rules, and every change told.
//! - **The operator's blocks** ([`Federation::operator`]) refuse a world, or one character of it,
//!   for everyone on this world, both ways.
//! - **The directory** (`directory`): a character on the network is listed with its profile, which
//!   the world composes and hands in at login, while it plays and at logout; it is taken out when
//!   it goes off the network.
//! - **Persons** (`persons`): `@dn link` and `@dn unlink` ask the hub on a character's behalf, the
//!   person view says whether it is linked and whom its person friends list, and `reach` lets a tell
//!   to it, away, go on to its person's Discord.
//! - **Channels** (`channels`): `@dn channel ...` asks the hub, and an invitation or a channel's
//!   line is shown as a tell would be.
//! - **Rooms** (`rooms`): the Global room in the chat room the hub, or else the world's settings,
//!   name, a line each way, `@dn global` and the world's own rooms mirrored into Discord. The host
//!   carries the lines to and from the game's chat service.
//! - **Verifying the world** (`verify`): the operator's `dn verify <token>`, at the console or
//!   said by an admin in game, goes to the hub through the host, and while it waits the world
//!   answers the hub's challenge to its game port, which the host hands in.
//!
//! The host calls [`Federation::on_link_up`], [`Federation::on_frame`],
//! [`Federation::on_link_down`], [`Federation::take_frames`], [`Federation::tick`] and
//! [`Federation::keep`]; a test drives the same calls on virtual time.

mod channels;
mod chatter;
pub mod directory;
mod operator;
mod persons;
mod rooms;
mod social;
mod tells;
mod verify;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use dereth_network_protocol::limits::{Bucket, Limits, Rate};
use dereth_network_protocol::rooms::KeptRooms;
use dereth_network_protocol::wire::{WireError, PROTOCOL};
use dereth_network_protocol::{
    Address, ChannelOp, DirectoryMessage, Message, PersonOp, ReachMessage, RoomsMessage,
    SayMessage, WorldInfo,
};
use yalain_store::{Store, StoreResult};

use chatter::Chatter;
use directory::Directory;

pub use dereth_network_protocol::rooms::NOT_CARRIED;
pub use operator::{HELP, IN_GAME_HELP};
pub use persons::{ASKED_RECORD, NOW_LINKED, PAUSED};
pub use rooms::{Character, GlobalLine, Heard, Room, SayAsk, Speaker, DEFAULT_ROOM};
pub use verify::{
    answer_line, is_verification_token, unreachable_line, ChallengeAnswer, VerifyRequest, ASKING,
    KEY_UNKNOWN, MAX_BY_CHARS, MAX_MESSAGE_CHARS, NOT_SENT, ON_ITS_WAY, VERIFY_HELP,
};

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

/// What a character younger than its operator asks is told when it sends to the network.
pub const TOO_NEW: &str = "This character is too new to send to the Dereth Network yet.";

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
    /// Where a browser reaches the world, told to the hub in its `hello`: its WebSocket endpoint,
    /// when it serves one.
    pub websocket_url: Option<String>,
    /// The link's name for the room that carries the Global room until the hub keeps one
    /// (`federation.global_room`); none carries no room.
    pub global_room: Option<&'static str>,
    /// The settings name [`Settings::global_room`] themselves, rather than leave it to its default.
    pub global_room_named: bool,
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

    /// Whether a character of this world ignores `who`, a character of this world (by name), whether
    /// or not the character is present.
    fn ignores_here(&self, character: &str, who: &str) -> bool;

    /// Show a present character a tell from another world, or one its own world's character
    /// answered from elsewhere. The sender and the text have passed the network's text rules.
    fn show_tell(&mut self, to: &str, from: &Address, text: &str);

    /// Show a present character the state of a friend on another world, named as it listed the
    /// friend. `news`: a change from a state it was already shown; otherwise the first state
    /// after the friend was added, after the character arrived, or after its world (or the hub)
    /// came back.
    fn show_presence(&mut self, to: &str, friend: &Address, online: bool, news: bool);

    /// A tell a character sent another world has ended ([`Federation::tell`]).
    fn tell_ended(&mut self, tell: TellId, end: TellEnd);

    /// Whether a character of this world (by name, any case) is in the world, with a friends
    /// stream or not.
    fn in_world(&self, name: &str) -> bool;

    /// How many names a character keeps on its friends list, of any world.
    fn kept_count(&self, character: &str) -> usize;

    /// A friend a character kept is its person's now: it keeps it no longer. The link's half lists
    /// it from here on, and nothing changes for the character.
    fn release_friend(&mut self, character: &str, friend: &Address);

    /// A friend the hub handed back to a character that was unlinked: it keeps it from now on.
    fn keep_friend(&mut self, character: &str, friend: &Address);

    /// The hub answered a linked character's add. Its client's own add, when one waits on it, is
    /// answered if the hub `refused` it; true when one waited.
    fn befriend_answered(&mut self, character: &str, friend: &Address, refused: bool) -> bool;

    /// A person view came for a character: what its person friends list is known, so what waited
    /// for it may go.
    fn person_view_came(&mut self, character: &str);

    /// Show a character of this world one of the network's lines, if it is in the world.
    fn show_line(&mut self, character: u64, line: &str);
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
    /// The teller is younger than its operator asks.
    TooNew,
    /// Not shown: the teller is not a member of the channel it told (the line says so).
    NotMember,
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

/// A player's request of the hub not yet answered.
#[derive(Debug, Clone)]
struct Ask {
    character: u64,
    kind: AskKind,
    /// When it was handed to the link; none while it waits for one.
    sent: Option<f64>,
}

/// What a request asks, which decides what its answer shows.
#[derive(Debug, Clone)]
enum AskKind {
    /// A person request. Quiet: the world's own, whose answer shows nothing. With the character's
    /// name and the friend it names, when it names one.
    Person {
        op: PersonOp,
        quiet: bool,
        target: Option<(String, Address)>,
    },
    Channel {
        op: ChannelOp,
        who: Option<Address>,
    },
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
    /// The operator's `dn verify` waiting for the hub.
    verification: verify::Verification,
    /// Person and channel requests not yet answered.
    asks: HashMap<String, Ask>,
    /// The characters that asked for a link (by id), kept: a link holds only for them.
    asked_link: BTreeSet<u64>,
    /// What the person view last said of each present character on the network, by its name in
    /// small letters: whether it is linked.
    linked: HashMap<String, bool>,
    /// What each present character's person friends list, by its name in small letters: never kept.
    person_rows: HashMap<String, Vec<persons::PersonRow>>,
    /// The friends listed through a person friend that went in play, by the character's name in small
    /// letters: its client may show each until it next asks for its friends.
    gone_rows: HashMap<String, BTreeSet<String>>,
    /// Person view requests: sent after the tells and answers, only while the link is up (a welcome
    /// sends them again).
    person_watches: VecDeque<Message>,
    /// The world's rooms as its hub last told them, kept in the store; none until one has.
    rooms: Option<KeptRooms>,
    /// [`Federation::rooms`] changed since the store was written.
    rooms_dirty: bool,
    /// This link's hub has told the world its rooms: it keeps them.
    rooms_told: bool,
    /// This link's hub, holding no room for the world, has been told the one it carries.
    seeded: bool,
    /// The log has said that the hub's room is not the one the settings name.
    settings_differ_logged: bool,
    /// When each of the world's own rooms the hub does not mirror may try the hub again, by name.
    uncarried: HashMap<String, f64>,
    /// The characters in the world younger than the operator asks, by their names in lower case.
    too_new: HashSet<String>,
    /// `rooms.global` frames: sent first, and only while the link is up.
    requests: VecDeque<String>,
    /// Who asked the hub to move the Global room, by the request's id (none: the console).
    global_asks: HashMap<String, Option<u64>>,
    /// Room lines handed to the hub and not yet answered.
    pending_rooms: HashMap<String, rooms::PendingRoom>,
    /// Each local character's own room lines, paced as the hub paces them.
    room_sends: HashMap<String, Bucket>,
    /// Lines for the players in a room, for the host to show.
    heard: Vec<Heard>,
    /// `@dng` lines for the host to say.
    global_lines: Vec<GlobalLine>,
    /// The hub's asks to say a line, for the host to look each character up.
    says: Vec<SayAsk>,
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
            verification: verify::Verification::default(),
            asks: HashMap::new(),
            asked_link: BTreeSet::new(),
            linked: HashMap::new(),
            person_rows: HashMap::new(),
            gone_rows: HashMap::new(),
            person_watches: VecDeque::new(),
            rooms: None,
            rooms_dirty: false,
            rooms_told: false,
            seeded: false,
            settings_differ_logged: false,
            uncarried: HashMap::new(),
            too_new: HashSet::new(),
            requests: VecDeque::new(),
            global_asks: HashMap::new(),
            pending_rooms: HashMap::new(),
            room_sends: HashMap::new(),
            heard: Vec::new(),
            global_lines: Vec::new(),
            says: Vec::new(),
        }
    }

    /// The network's state as the store keeps it: the characters on the network (those deleted
    /// left out), the operator's blocks, the rooms the hub last told the world, and the listings
    /// owed that take characters out of the directory.
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
        f.asked_link = persons::read_asked(store.world_record(ASKED_RECORD)?.as_deref());
        f.load_rooms(store)?;
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

    /// The characters in the world younger than the operator asks of one sending to the network,
    /// by their names in lower case, as the host says.
    pub fn set_too_new(&mut self, names: HashSet<String>) {
        self.too_new = names;
    }

    /// Is a character in the world younger than the operator asks?
    pub(crate) fn too_new(&self, name: &str) -> bool {
        self.too_new.contains(&name.to_lowercase())
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
        self.owe_frame(t, world, message.to_json());
    }

    /// Queues an answer owed to `world`, as its frame.
    fn owe_frame(&mut self, t: f64, world: &str, frame: String) {
        self.answers
            .entry(world.to_owned())
            .or_default()
            .push_back(Owed { at: t, frame });
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
            websocket_url: self.settings.websocket_url.clone(),
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
        self.rooms_link_down(friends);
        self.link = LinkState::Down;
        self.control.clear();
        self.chatter.clear();
        self.directory_down();
        // The hub answers for this world every tell it had not answered when the link went.
        self.answers.clear();
        self.person_watches.clear();
        for m in std::mem::take(&mut self.in_flight).into_iter().rev() {
            let waiting = match &m {
                Message::Tell { id, .. } => {
                    self.pending.get_mut(id).map(|p| p.sent = None).is_some()
                }
                Message::Channel { id, .. } | Message::Person { id, .. } => {
                    self.asks.get_mut(id).map(|a| a.sent = None).is_some()
                }
                Message::RoomSay { id, .. } => self
                    .pending_rooms
                    .get_mut(id)
                    .map(|p| p.sent = None)
                    .is_some(),
                _ => false,
            };
            if waiting {
                self.outbox.push_front(Queued { at: t, message: m });
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
                    if let Some(frame) = self.requests.pop_front() {
                        pace.take(t);
                        out.push(frame);
                        continue;
                    }
                    if let Some(q) = self.outbox.pop_front() {
                        pace.take(t);
                        match &q.message {
                            Message::Tell { id, .. } => {
                                if let Some(p) = self.pending.get_mut(id) {
                                    p.sent = Some(t);
                                }
                                self.in_flight.push(q.message.clone());
                            }
                            Message::Channel { id, .. } | Message::Person { id, .. } => {
                                if let Some(a) = self.asks.get_mut(id) {
                                    a.sent = Some(t);
                                }
                                self.in_flight.push(q.message.clone());
                            }
                            Message::RoomSay { id, .. } => {
                                if let Some(p) = self.pending_rooms.get_mut(id) {
                                    p.sent = Some(t);
                                }
                                self.in_flight.push(q.message.clone());
                            }
                            _ => {}
                        }
                        out.push(q.message.to_json());
                        continue;
                    }
                    if let Some(frame) = self.next_answer() {
                        pace.take(t);
                        out.push(frame);
                        continue;
                    }
                    if let Some(m) = self.person_watches.pop_front() {
                        pace.take(t);
                        out.push(m.to_json());
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
        let skipped = |e: &WireError| {
            tracing::debug!("Dereth Network: skipped a frame from the hub: {e}");
        };
        match Message::from_json(text) {
            Ok(m) => return self.on_message(friends, t, m),
            Err(WireError::Unknown(_)) => {}
            Err(e) => return skipped(&e),
        }
        match DirectoryMessage::from_json(text) {
            Ok(m) => return self.on_directory(friends, t, m),
            Err(WireError::Unknown(_)) => {}
            Err(e) => return skipped(&e),
        }
        match ReachMessage::from_json(text) {
            Ok(m) => return self.on_reach(friends, t, m),
            Err(WireError::Unknown(_)) => {}
            Err(e) => return skipped(&e),
        }
        match SayMessage::from_json(text) {
            Ok(m) => return self.on_say_message(m),
            Err(WireError::Unknown(_)) => {}
            Err(e) => return skipped(&e),
        }
        match RoomsMessage::from_json(text) {
            Ok(m) => self.on_rooms_message(friends, t, m),
            Err(WireError::Unknown(_)) => {}
            Err(e) => skipped(&e),
        }
    }

    fn on_message(&mut self, friends: &mut impl Friends, t: f64, m: Message) {
        match m {
            Message::Welcome {
                world,
                worlds,
                rooms,
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
                self.rooms_welcomed(t, &rooms);
                self.directory_welcomed(t);
                self.watch_every_person(friends);
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
            Message::PersonResult {
                id,
                outcome,
                code,
                expires_secs,
                url,
                ..
            } => self.on_person_result(
                friends,
                &id,
                outcome,
                code.as_deref(),
                url.as_deref(),
                expires_secs,
            ),
            Message::PersonFriends {
                to,
                linked,
                persons,
                ..
            } => self.on_person_view(friends, t, &to, linked, &persons),
            Message::ChannelResult {
                id,
                outcome,
                channel,
                members,
                channels,
            } => self.on_channel_result(friends, &id, outcome, &channel, &members, &channels),
            Message::ChannelInvite { channel, by, to } => {
                self.on_channel_invite(friends, t, &channel, &by, &to);
            }
            Message::ChannelLine {
                channel,
                from,
                to,
                text,
            } => self.on_channel_line(friends, t, &channel, &from, &to, &text),
            Message::RoomLine {
                room,
                from,
                text,
                via,
            } => self.on_room_line(t, &room, &from, &text, via.as_deref()),
            Message::RoomResult { id, outcome } => {
                self.on_room_result(friends, t, &id, outcome);
            }
            Message::Error { code, message } => {
                tracing::warn!("Dereth Network: the hub reports {code}: {message}");
            }
            // Messages only a world sends.
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
            match m {
                Message::Tell { id, .. } => self.give_up(friends, &id),
                Message::Channel { id, .. } | Message::Person { id, .. } => {
                    self.give_up_ask(friends, &id);
                }
                Message::RoomSay { id, .. } => {
                    self.pending_rooms.remove(&id);
                }
                _ => {}
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
        let late: Vec<String> = self
            .asks
            .iter()
            .filter(|(_, a)| a.sent.is_some_and(|s| t - s >= timeout))
            .map(|(k, _)| k.clone())
            .collect();
        for id in late {
            self.give_up_ask(friends, &id);
        }
        self.rooms_tick(t, timeout);
        let (pending, asks, lines) = (&self.pending, &self.asks, &self.pending_rooms);
        self.in_flight.retain(|m| match m {
            Message::Tell { id, .. } => pending.contains_key(id),
            Message::Channel { id, .. } | Message::Person { id, .. } => asks.contains_key(id),
            Message::RoomSay { id, .. } => lines.contains_key(id),
            _ => false,
        });
        self.sends.retain(|_, b| !b.full_at(t));
        self.received.retain(|_, b| !b.full_at(t));
        self.directory_settle(t);
        self.chatter.settle(t);
        self.show_due(friends, t);
    }
}
