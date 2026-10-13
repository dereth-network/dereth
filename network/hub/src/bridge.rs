//! Rooms mirrored into Discord (`docs/network/05-hub.md` section 5.7): the Global room, a network
//! channel, or a world's own room, each into channels of Discord servers. A **mirror** is one room
//! in one channel; a room may have many (the hub's admins' own, and each world operator's of the
//! Global room). A mirror carries the game into Discord: the hub says the room's lines there and
//! reads nothing back.
//!
//! - **Out.** Every line the routing takes in a mirrored room ([`crate::core::Hub::take_bridged`])
//!   goes to each of the room's mirrors, said through the mirror's webhook (the bot's own, which
//!   it makes and keeps in the channel, or one the mirror's asker made there), shown under the
//!   speaker's address, with its mark when its person said it from Discord ([`webhook_name`]),
//!   through the line rules with Discord's markdown escaped
//!   ([`for_channel`]), calling nobody, within the waits Discord names for the webhook, the short
//!   ones waited out ([`SHORT_WAIT`]). A mirror's lines wait, at most [`QUEUE_LINES`], on one of
//!   [`SAY_THREADS`] threads, so that one server's waits hold up no other's. A line that cannot be
//!   said now is held back: counted, and logged when holding begins and when it ends, never
//!   dropped silently.
//! - **The bot's webhooks.** Another thread finds or makes the bot's webhook in each mirror's
//!   channel at the start, and looks for it again every [`crate::broker::Broker::recheck`].
//! - **What is mirrored** is the registry's: the hub's admins' mirrors of the network's rooms and
//!   channels, and each world operator's of its own rooms and of the Global room ([`Placement`]),
//!   kept while the world is in the registry and mirrored while it is on the network. A channel is
//!   its asker's to mirror into only where both the asker and the bot may manage its webhooks
//!   ([`check_channel`]), or through a webhook its asker made there ([`check_webhook`]).
//!
//! A person speaks into a world operator's mirror only with the bot's `/dn` command, as one of its
//! own characters, through that character's world ([`crate::interactions`]); the hub reads no
//! mirror's channel. Nothing here keeps a line past the moment it is said, or logs one; nothing
//! here logs a token, or answers the webhook an asker gave.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use dereth_network_protocol::address::CHANNEL_WORLD;
use dereth_network_protocol::say::VIA_DISCORD;
use dereth_network_protocol::text::sanitize_line;
use dereth_network_protocol::{Address, Federation};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::broker::{webhook_address, webhook_route, BrokerError, Service, Webhook};
use crate::core::ROOMS;
use crate::operators::{Operator, Role};
use crate::registry::{iso, Registry, RegistryError, Row};
use crate::server::{log, Shared};

/// The rooms of its own a world's operator may mirror, beside the Global room: the TurbineChat
/// rooms a world joins by name, but the one that carries the Global room there
/// ([`crate::rooms`]).
pub use dereth_network_protocol::rooms::OWN_ROOMS;

/// The most lines that may wait to be said on each of the [`SAY_THREADS`]; one past it is held
/// back.
pub const QUEUE_LINES: usize = 1_000;

/// How many threads say the lines into the mirrors, each mirror's always on the same one.
pub const SAY_THREADS: usize = 4;

/// The longest wait Discord names for a webhook that a line waits out; past it, the line is held
/// back.
pub const SHORT_WAIT: Duration = Duration::from_secs(5);

/// The name of the webhook the bot makes in a mirror's channel, which a line shows when its
/// speaker's address cannot be its name.
pub const WEBHOOK_NAME: &str = "Dereth Network";

/// Discord's permissions, as the hub checks them in a server channel.
pub const ADMINISTRATOR: u64 = 1 << 3;
/// See [`ADMINISTRATOR`].
pub const VIEW_CHANNEL: u64 = 1 << 10;
/// See [`ADMINISTRATOR`].
pub const MANAGE_WEBHOOKS: u64 = 1 << 29;

/// What the bot needs in a mirror's channel, and what the mirror's asker needs there, as it would
/// to set such a webhook up itself: View Channel and Manage Webhooks.
pub const MIRROR_PERMISSIONS: u64 = VIEW_CHANNEL | MANAGE_WEBHOOKS;

/// The `hub_state` row the picture each mirrored line shows is kept in.
const AVATAR: &str = "discord_avatar_url";

/// How often the mirrors are read again from the registry.
const REFRESH: Duration = Duration::from_secs(10);

/// How often the log sums up what the mirrors carried, when they carried anything.
const SUMMARY_EVERY: Duration = Duration::from_secs(3_600);

/// The least time between two log lines about one kind of trouble.
const LOG_EVERY: Duration = Duration::from_secs(60);

/// A room mirrored into Discord, as the routing knows it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Bridged {
    /// One of the network's rooms (`global`).
    Room(String),
    /// A network channel, by its name.
    Channel(String),
    /// A room of one world's own, which that world alone carries.
    World { slug: String, room: String },
}

impl Bridged {
    /// A room the hub's admins mirror: one of the network's rooms, or a channel's address
    /// (`oldguard@channel`).
    ///
    /// # Errors
    /// When it is neither.
    pub fn named(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if ROOMS.contains(&s) {
            return Ok(Self::Room(s.to_owned()));
        }
        match Address::parse(s) {
            Ok(a) if a.is_channel() => Ok(Self::Channel(a.name().to_owned())),
            _ => Err(format!(
                "{s}: one of the hub's rooms ({}) or a channel's address (<name>@{CHANNEL_WORLD})",
                ROOMS.join(", ")
            )),
        }
    }

    /// A room a world's operator mirrors: one of the network's rooms, or one of [`OWN_ROOMS`],
    /// the world's own.
    ///
    /// # Errors
    /// When it is neither.
    pub fn of_world(slug: &str, room: &str) -> Result<Self, String> {
        if ROOMS.contains(&room) {
            Ok(Self::Room(room.to_owned()))
        } else if OWN_ROOMS.contains(&room) {
            Ok(Self::World {
                slug: slug.to_owned(),
                room: room.to_owned(),
            })
        } else {
            Err(format!(
                "{room}: one of {} or {}",
                ROOMS.join(", "),
                OWN_ROOMS.join(", ")
            ))
        }
    }

    /// The room as the mirrors' routes name it: `global`, `oldguard@channel`, `general`.
    #[must_use]
    pub fn route_name(&self) -> String {
        match self {
            Self::Room(room) | Self::World { room, .. } => room.clone(),
            Self::Channel(name) => format!("{name}@{CHANNEL_WORLD}"),
        }
    }
}

/// As the log names it: `global`, `oldguard@channel`, `coldeve/general`.
impl fmt::Display for Bridged {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Room(room) => f.write_str(room),
            Self::Channel(name) => write!(f, "{name}@{CHANNEL_WORLD}"),
            Self::World { slug, room } => write!(f, "{slug}/{room}"),
        }
    }
}

/// Where a mirror goes: its channel, the channel's server, and the webhook its asker made there,
/// when the bot is not in the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub channel: String,
    pub server: String,
    /// The asker's own webhook, which the hub says lines through; none for the bot's.
    pub hook: Option<Webhook>,
}

impl Placement {
    /// A mirror through the bot's own webhook in `channel` of `server`.
    #[must_use]
    pub fn bot(channel: &str, server: &str) -> Self {
        Self {
            channel: channel.to_owned(),
            server: server.to_owned(),
            hook: None,
        }
    }

    /// Who says its lines: `bot` or `webhook`, as the routes name it.
    #[must_use]
    pub fn through(&self) -> &'static str {
        if self.hook.is_some() {
            "webhook"
        } else {
            "bot"
        }
    }
}

/// A mirror, as the registry keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mirror {
    /// The world whose operator set it, by registry id and slug; none for the hub's admins'.
    pub world: Option<(String, String)>,
    /// The room as the routes name it ([`Bridged::route_name`]).
    pub room: String,
    pub placement: Placement,
    /// When it was set.
    pub set_at: String,
}

impl Mirror {
    /// The room it mirrors, as the routing knows it; none for a row that names none.
    #[must_use]
    pub fn mirrored(&self) -> Option<Bridged> {
        match &self.world {
            Some((_, slug)) => Bridged::of_world(slug, &self.room).ok(),
            None => Bridged::named(&self.room).ok(),
        }
    }

    /// Its channel.
    #[must_use]
    pub fn channel(&self) -> &str {
        &self.placement.channel
    }

    /// As the routes show it: never its asker's webhook's token or address.
    fn to_json(&self, counts: &Counts, hook: Option<&str>) -> Value {
        let p = &self.placement;
        json!({
            "room": self.room,
            "channel": p.channel,
            "server": p.server,
            "through": p.through(),
            "webhook": p.hook.as_ref().map(|h| h.id.as_str()).or(hook),
            "setAt": self.set_at,
            "lines": counts.to_json(),
        })
    }
}

/// A line said in a mirrored room, for its mirrors: `via` marks one its speaker's person said
/// elsewhere (`discord`), which its mirrors show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeLine {
    pub to: Bridged,
    pub from: Address,
    pub text: String,
    pub via: Option<String>,
}

/// A line from the game as Discord is given it: through the line rules, so that Discord shows
/// what every world shows, with Discord's mentions (`@everyone`, `@here`, `<@...>`, `<#...>`)
/// broken by a zero-width space, so that no line calls anyone.
#[must_use]
pub fn for_discord(text: &str) -> String {
    sanitize_line(text)
        .replace("@everyone", "@\u{200B}everyone")
        .replace("@here", "@\u{200B}here")
        .replace("<@", "<\u{200B}@")
        .replace("<#", "<\u{200B}#")
}

/// The characters Discord's markdown gives a meaning to, which a line said in a channel escapes.
const MARKDOWN: &str = "\\*_~`|<>[]()#-";

/// A line from the game as it is said in a server's channel: through the line rules with Discord's
/// mentions broken ([`for_discord`]), and every character Discord's markdown gives a meaning to
/// escaped by a backslash, so the channel shows the characters the worlds show (no bold, no
/// heading, no masked link), except within a link (`http://...`, `https://...`), left as written so
/// that it still opens.
#[must_use]
pub fn for_channel(text: &str) -> String {
    let text = for_discord(text);
    let mut out = String::with_capacity(text.len() * 2);
    for (i, word) in text.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if word.starts_with("https://") || word.starts_with("http://") {
            out.push_str(word);
            continue;
        }
        for c in word.chars() {
            if MARKDOWN.contains(c) {
                out.push('\\');
            }
            out.push(c);
        }
    }
    out
}

/// A line's mark as a world shows it (`via` `discord`: `Discord`): its word, its first letter a
/// capital.
#[must_use]
pub fn mark_shown(via: &str) -> String {
    let mut c = via.chars();
    c.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(c).collect::<String>()
    })
}

/// A line's mark as a webhook's name shows it, after the speaker's address: `via /dn` for a line
/// said from Discord, since Discord refuses its own name in a webhook's; `via` and the word for
/// any other.
#[must_use]
pub fn mark_in_name(via: &str) -> String {
    if via == VIA_DISCORD {
        "via /dn".to_owned()
    } else {
        format!("via {via}")
    }
}

/// The name a line said through a webhook shows: its speaker's address (`Ann@coldeve`), followed by
/// its mark when its person said it elsewhere (`Ann@coldeve (via /dn)`), or none when the address
/// holds a word Discord refuses in a webhook's name (`discord`, `clyde`, in any case): such a line
/// shows the webhook's own name, and begins with its speaker's address and mark.
#[must_use]
pub fn webhook_name(from: &Address, via: Option<&str>) -> Option<String> {
    let name = from.to_string();
    let lower = name.to_lowercase();
    if ["discord", "clyde"].iter().any(|w| lower.contains(w)) {
        return None;
    }
    Some(match via {
        Some(via) => format!("{name} ({})", mark_in_name(via)),
        None => name,
    })
}

/// How a line names its speaker in its own text when its name cannot (`Clyde@coldeve: hello`), with
/// its mark as a world shows it (`Clyde@coldeve (Discord): hello`).
#[must_use]
pub fn speaker_in_line(from: &Address, via: Option<&str>) -> String {
    match via {
        Some(via) => format!("{from} ({})", mark_shown(via)),
        None => from.to_string(),
    }
}

/// The permissions `user` has in `channel` of `guild`, `member` being it as a member of the
/// server, as Discord works them out: the server's owner and an administrator have every one;
/// otherwise the server's everyone role's and its roles', then the channel's overwrites for
/// everyone, for its roles together, and for it.
#[must_use]
pub fn permissions(guild: &Value, user: &str, member: &Value, channel: &Value) -> u64 {
    let bits = |v: &Value| {
        v.as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| v.as_u64())
            .unwrap_or(0)
    };
    let guild_id = guild["id"].as_str().unwrap_or("");
    if guild["owner_id"].as_str() == Some(user) {
        return u64::MAX;
    }
    let roles: Vec<&str> = member["roles"]
        .as_array()
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut p = 0;
    for role in guild["roles"].as_array().into_iter().flatten() {
        let id = role["id"].as_str().unwrap_or("");
        if id == guild_id || roles.contains(&id) {
            p |= bits(&role["permissions"]);
        }
    }
    if p & ADMINISTRATOR != 0 {
        return u64::MAX;
    }
    let overwrites: Vec<&Value> = channel["permission_overwrites"]
        .as_array()
        .map(|o| o.iter().collect())
        .unwrap_or_default();
    let apply = |p: u64, o: (u64, u64)| (p & !o.1) | o.0;
    let of = |o: &Value| (bits(&o["allow"]), bits(&o["deny"]));
    if let Some(o) = overwrites.iter().find(|o| o["id"] == guild_id) {
        p = apply(p, of(o));
    }
    let by_roles = overwrites
        .iter()
        .filter(|o| o["type"] == 0)
        .filter(|o| {
            o["id"]
                .as_str()
                .is_some_and(|id| id != guild_id && roles.contains(&id))
        })
        .fold((0, 0), |(a, d), o| (a | of(o).0, d | of(o).1));
    p = apply(p, by_roles);
    if let Some(o) = overwrites
        .iter()
        .find(|o| o["type"] == 1 && o["id"] == user)
    {
        p = apply(p, of(o));
    }
    p
}

/// The names of the permissions in `bits` the hub checks, as Discord shows them.
fn permission_names(bits: u64) -> String {
    [
        (VIEW_CHANNEL, "View Channel"),
        (MANAGE_WEBHOOKS, "Manage Webhooks"),
    ]
    .iter()
    .filter(|(b, _)| bits & b != 0)
    .map(|(_, n)| *n)
    .collect::<Vec<_>>()
    .join(", ")
}

/// What one mirror carried since the hub started.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub to_discord: u64,
    pub held_to_discord: u64,
    pub not_carried: u64,
}

impl Counts {
    /// As the hub's answers show it.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "toDiscord": self.to_discord,
            "heldBackToDiscord": self.held_to_discord,
            "notCarried": self.not_carried,
        })
    }

    fn add(&mut self, other: &Self) {
        self.to_discord += other.to_discord;
        self.held_to_discord += other.held_to_discord;
        self.not_carried += other.not_carried;
    }
}

/// A line for one mirror, by its channel.
#[derive(Debug, Clone)]
struct Job {
    channel: String,
    line: BridgeLine,
}

/// One thread's lines into the mirrors: its queue, and the thread that says them takes the other
/// end.
struct Queue {
    queue: SyncSender<Job>,
    waiting: Mutex<Option<Receiver<Job>>>,
}

impl Queue {
    fn new() -> Self {
        let (queue, waiting) = mpsc::sync_channel(QUEUE_LINES);
        Self {
            queue,
            waiting: Mutex::new(Some(waiting)),
        }
    }
}

/// The thread a mirror's lines are said on: always the same one for one channel.
fn queue_of(channel: &str) -> usize {
    let n = channel
        .bytes()
        .fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b)));
    usize::try_from(n % SAY_THREADS as u64).unwrap_or(0)
}

/// What the mirrors keep while the hub runs.
pub(crate) struct State {
    queues: Vec<Queue>,
    mirrors: Mutex<Vec<Mirror>>,
    /// The picture each mirrored line shows.
    avatar: Mutex<Option<String>>,
    /// The bot's own user id, to tell its webhooks from others', as Discord first gave it.
    bot: Mutex<Option<String>>,
    /// The bot's own webhook in each mirror's channel, by the channel's id.
    hooks: Mutex<HashMap<String, Webhook>>,
    /// The channels whose webhook to look for again at once: Discord no longer had it when a line
    /// was said.
    lost: Mutex<HashSet<String>>,
    counts: Mutex<BTreeMap<String, Counts>>,
    logged: Mutex<HashMap<&'static str, Instant>>,
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State")
            .field("mirrors", &*lock(&self.mirrors))
            .finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl State {
    pub(crate) fn new() -> Self {
        Self {
            queues: (0..SAY_THREADS).map(|_| Queue::new()).collect(),
            mirrors: Mutex::new(Vec::new()),
            avatar: Mutex::new(None),
            bot: Mutex::new(None),
            hooks: Mutex::new(HashMap::new()),
            lost: Mutex::new(HashSet::new()),
            counts: Mutex::new(BTreeMap::new()),
            logged: Mutex::new(HashMap::new()),
        }
    }

    /// The routing's lines for Discord: each waits to be said in every mirror of its room, or is
    /// held back when [`QUEUE_LINES`] wait on that mirror's thread already.
    pub(crate) fn hand(&self, lines: Vec<BridgeLine>) {
        for line in lines {
            let channels: Vec<String> = lock(&self.mirrors)
                .iter()
                .filter(|m| m.mirrored().as_ref() == Some(&line.to))
                .map(|m| m.channel().to_owned())
                .collect();
            for channel in channels {
                let job = Job {
                    channel: channel.clone(),
                    line: line.clone(),
                };
                if let Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) =
                    self.queues[queue_of(&channel)].queue.try_send(job)
                {
                    self.count(&channel, |c| c.held_to_discord += 1);
                    self.log_now_and_then(
                        "queue",
                        &format!(
                            "holding back lines into Discord: {QUEUE_LINES} wait to be said already"
                        ),
                    );
                }
            }
        }
    }

    fn count(&self, channel: &str, f: impl FnOnce(&mut Counts)) {
        f(lock(&self.counts).entry(channel.to_owned()).or_default());
    }

    fn counts_of(&self, channel: &str) -> Counts {
        lock(&self.counts).get(channel).copied().unwrap_or_default()
    }

    fn total(&self) -> Counts {
        let mut all = Counts::default();
        for c in lock(&self.counts).values() {
            all.add(c);
        }
        all
    }

    /// The mirrors carried now.
    pub(crate) fn mirrors(&self) -> Vec<Mirror> {
        lock(&self.mirrors).clone()
    }

    /// The mirror in `channel`, carried now.
    fn mirror_in(&self, channel: &str) -> Option<Mirror> {
        lock(&self.mirrors)
            .iter()
            .find(|m| m.channel() == channel)
            .cloned()
    }

    /// The webhook the bot keeps in `channel`.
    fn hook(&self, channel: &str) -> Option<Webhook> {
        lock(&self.hooks).get(channel).cloned()
    }

    /// Logs `msg` unless a line about `what` was logged within [`LOG_EVERY`].
    fn log_now_and_then(&self, what: &'static str, msg: &str) {
        let now = Instant::now();
        let mut logged = lock(&self.logged);
        if logged
            .get(what)
            .is_none_or(|t| now.duration_since(*t) >= LOG_EVERY)
        {
            logged.insert(what, now);
            drop(logged);
            log("info", msg);
        }
    }

    /// The bot's own user id, asked of Discord the first time.
    fn bot(&self, service: &Service) -> Result<String, BrokerError> {
        if let Some(bot) = lock(&self.bot).clone() {
            return Ok(bot);
        }
        let bot = service.rest.application()?.bot_id;
        *lock(&self.bot) = Some(bot.clone());
        Ok(bot)
    }
}

/// Reads the mirrors again from the registry, and tells the routing when the rooms mirrored
/// changed.
pub(crate) fn refresh(shared: &Shared) {
    let Some(service) = shared.broker.as_ref() else {
        return;
    };
    let (now, avatar) = {
        let reg = shared.registry();
        (reg.mirrored_now(), reg.discord_avatar())
    };
    let now = match now {
        Ok(m) => m,
        Err(e) => return log("error", &format!("the mirrors into Discord: {e}")),
    };
    if let Ok(avatar) = avatar {
        *lock(&service.bridge.avatar) = avatar;
    }
    let mut mirrors = lock(&service.bridge.mirrors);
    if *mirrors == now {
        return;
    }
    // A channel the bot no longer keeps a webhook in forgets it.
    for gone in mirrors.iter() {
        let channel = gone.channel();
        if !now
            .iter()
            .any(|n| n.channel() == channel && n.placement.hook.is_none())
        {
            lock(&service.bridge.hooks).remove(channel);
        }
    }
    let rooms: BTreeSet<Bridged> = now.iter().filter_map(Mirror::mirrored).collect();
    *mirrors = now;
    drop(mirrors);
    shared.bridged_changed(rooms);
}

/// The rooms mirrored as the hub starts, for the routing's first view of them.
pub(crate) fn first_mirrored(shared: &Shared) -> BTreeSet<Bridged> {
    let Some(service) = shared.broker.as_ref() else {
        return BTreeSet::new();
    };
    let (now, avatar) = {
        let reg = shared.registry();
        (reg.mirrored_now(), reg.discord_avatar())
    };
    let now = now.unwrap_or_else(|e| {
        log("error", &format!("the mirrors into Discord: {e}"));
        Vec::new()
    });
    *lock(&service.bridge.avatar) = avatar.unwrap_or_default();
    let rooms = now.iter().filter_map(Mirror::mirrored).collect();
    *lock(&service.bridge.mirrors) = now;
    rooms
}

/// Starts the mirrors' threads: one keeps the bot's webhooks, and [`SAY_THREADS`] say the game's
/// lines into the mirrors.
///
/// # Errors
/// When a thread cannot start.
pub(crate) fn start(shared: &Arc<Shared>) -> std::io::Result<Vec<JoinHandle<()>>> {
    if shared.broker.is_none() {
        return Ok(Vec::new());
    }
    let s = Arc::clone(shared);
    let mut threads = vec![std::thread::Builder::new()
        .name("hub-mirrors".into())
        .spawn(move || keep(&s))?];
    for i in 0..SAY_THREADS {
        let s = Arc::clone(shared);
        threads.push(
            std::thread::Builder::new()
                .name(format!("hub-mirror-lines-{i}"))
                .spawn(move || say_lines(&s, i))?,
        );
    }
    Ok(threads)
}

/// How long to wait before asking again for a channel Discord refused `refused` times in a row:
/// [`crate::broker::Broker::retry`], doubled for each refusal before, at most
/// [`crate::broker::Broker::recheck`], so that a channel the bot lost cannot have the hub ask
/// Discord in vain every few seconds.
fn backoff(settings: &crate::broker::Broker, refused: u32) -> Duration {
    settings
        .retry
        .saturating_mul(1 << refused.min(6))
        .min(settings.recheck.max(settings.retry))
}

/// Keeps the bot's webhook in each of its mirrors' channels: found or made at the start, looked
/// for again every [`crate::broker::Broker::recheck`], and tried again after
/// [`crate::broker::Broker::retry`] (doubling for each refusal in a row), or the wait Discord
/// names, when it could not be.
fn keep(shared: &Arc<Shared>) {
    let Some(service) = shared.broker.as_ref() else {
        return;
    };
    let settings = service.rest.broker().clone();
    let mut due: BTreeMap<String, Instant> = BTreeMap::new();
    let mut refused: HashMap<String, u32> = HashMap::new();
    let mut next_refresh = Instant::now() + REFRESH;
    let mut summary = (Instant::now() + SUMMARY_EVERY, Counts::default());
    while !shared.stopping() {
        let now = Instant::now();
        if now >= next_refresh {
            refresh(shared);
            next_refresh = now + REFRESH;
        }
        let mirrors = service.bridge.mirrors();
        let kept: Vec<&Mirror> = mirrors
            .iter()
            .filter(|m| m.placement.hook.is_none())
            .collect();
        due.retain(|k, _| kept.iter().any(|m| m.channel() == k));
        refused.retain(|k, _| kept.iter().any(|m| m.channel() == k));
        for channel in std::mem::take(&mut *lock(&service.bridge.lost)) {
            due.insert(channel, now);
        }
        for m in kept {
            let channel = m.channel();
            if now < *due.entry(channel.to_owned()).or_insert(now) {
                continue;
            }
            let to = m
                .mirrored()
                .map_or_else(|| m.room.clone(), |b| b.to_string());
            let next = match keep_webhook(service, &to, channel) {
                Ok(()) => {
                    refused.remove(channel);
                    settings.recheck
                }
                Err(BrokerError::Wait(secs)) => settings.retry.max(Duration::from_secs(secs)),
                Err(e) => {
                    let n = refused.entry(channel.to_owned()).or_insert(0);
                    *n += 1;
                    let next = backoff(&settings, *n - 1);
                    log(
                        "warn",
                        &format!(
                            "the Discord channel {channel} of {to}: {e}; trying again in {} seconds",
                            next.as_secs()
                        ),
                    );
                    next
                }
            };
            due.insert(channel.to_owned(), Instant::now() + next);
        }
        if Instant::now() >= summary.0 {
            let total = service.bridge.total();
            if total != summary.1 {
                let d = |a: u64, b: u64| a.saturating_sub(b);
                let last = summary.1;
                log(
                    "info",
                    &format!(
                        "the mirrors into Discord in the last hour: {} lines said, {} held back, {} not carried",
                        d(total.to_discord, last.to_discord),
                        d(total.held_to_discord, last.held_to_discord),
                        d(total.not_carried, last.not_carried),
                    ),
                );
            }
            summary = (Instant::now() + SUMMARY_EVERY, total);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Keeps the bot's webhook in the channel `channel`, which mirrors `to`: the bot's own webhook in
/// it (the oldest, should there be more), found again after a restart, or else a new one, named
/// [`WEBHOOK_NAME`].
fn keep_webhook(service: &Service, to: &str, channel: &str) -> Result<(), BrokerError> {
    let bot = service.bridge.bot(service)?;
    let known = service.bridge.hook(channel).map(|h| h.id);
    let ours = service
        .rest
        .webhooks(channel)?
        .iter()
        .filter(|h| h["type"] == 1 && h["user"]["id"].as_str() == Some(bot.as_str()))
        .filter_map(Webhook::of)
        .min_by_key(|h| h.id.parse::<u64>().unwrap_or(u64::MAX));
    let hook = match ours {
        Some(hook) => {
            if known.as_deref() != Some(hook.id.as_str()) {
                log(
                    "info",
                    &format!(
                        "the Discord webhook {} mirrors {to} into the channel {channel} again",
                        hook.id
                    ),
                );
            }
            hook
        }
        None => {
            let hook = service.rest.create_webhook(channel, WEBHOOK_NAME)?;
            log(
                "info",
                &format!(
                    "made the Discord webhook {} for {to} in the channel {channel}",
                    hook.id
                ),
            );
            hook
        }
    };
    lock(&service.bridge.hooks).insert(channel.to_owned(), hook);
    Ok(())
}

/// How many lines into Discord one thread held back in a row.
#[derive(Debug, Default)]
struct Holding {
    held: u64,
}

impl Holding {
    /// A line was held back in `channel`, for `why`: logged when holding begins.
    fn hold(&mut self, service: &Service, channel: &str, why: &str) {
        service.bridge.count(channel, |c| c.held_to_discord += 1);
        if self.held == 0 {
            log("info", &format!("holding back lines into Discord: {why}"));
        }
        self.held += 1;
    }

    /// A line was said: holding, if it was, ends, and the log says how many lines it held back.
    fn said(&mut self) {
        if self.held > 0 {
            log(
                "info",
                &format!("lines go into Discord again: {} were held back", self.held),
            );
            self.held = 0;
        }
    }
}

/// Says the lines waiting on thread `i`, one at a time, in order.
fn say_lines(shared: &Arc<Shared>, i: usize) {
    let Some(service) = shared.broker.as_ref() else {
        return;
    };
    let Some(waiting) = lock(&service.bridge.queues[i].waiting).take() else {
        return;
    };
    let mut holding = Holding::default();
    while !shared.stopping() {
        match waiting.recv_timeout(Duration::from_millis(100)) {
            Ok(job) => say(shared, service, &job, &mut holding),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Waits `d`, or until the hub stops.
fn pause(shared: &Shared, d: Duration) {
    let until = Instant::now() + d;
    while !shared.stopping() {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        std::thread::sleep(left.min(Duration::from_millis(50)));
    }
}

/// Says `job`'s line in its mirror's channel through the mirror's webhook, shown under its
/// speaker's address, or holds it back. A wait Discord names for the webhook of at most
/// [`SHORT_WAIT`] is waited out; a longer one holds the line back.
fn say(shared: &Shared, service: &Service, job: &Job, holding: &mut Holding) {
    let (line, channel) = (&job.line, job.channel.as_str());
    let to = &line.to;
    let not_carried = |why: &str| {
        service.bridge.count(channel, |c| c.not_carried += 1);
        service.bridge.log_now_and_then(
            "not carried",
            &format!("a line into Discord in {to} was not carried: {why}"),
        );
    };
    let Some(m) = service
        .bridge
        .mirror_in(channel)
        .filter(|m| m.mirrored().as_ref() == Some(to))
    else {
        return not_carried("the room is mirrored there no more");
    };
    let given = m.placement.hook.clone();
    let Some(hook) = given.clone().or_else(|| service.bridge.hook(channel)) else {
        return holding.hold(
            service,
            channel,
            &format!("the webhook of the channel {channel} of {to} is not made yet"),
        );
    };
    if for_discord(&line.text).is_empty() {
        return not_carried("nothing is left of it once through the line rules");
    }
    let mut name = webhook_name(&line.from, line.via.as_deref());
    let shown_by_name = for_channel(&line.text);
    let shown_in_line = format!(
        "{}: {shown_by_name}",
        speaker_in_line(&line.from, line.via.as_deref())
    );
    let avatar = lock(&service.bridge.avatar).clone();
    let route = webhook_route(&hook.id);
    let wait = |d: Duration| {
        format!(
            "Discord asks the hub to wait {} seconds",
            d.as_secs() + u64::from(d.subsec_nanos() > 0)
        )
    };
    for _ in 0..3 {
        if let Some(left) = service.rest.waiting(&route) {
            if left > SHORT_WAIT {
                return holding.hold(service, channel, &wait(left));
            }
            pause(shared, left);
        }
        let content = if name.is_some() {
            &shown_by_name
        } else {
            &shown_in_line
        };
        match service
            .rest
            .execute_webhook(&hook, content, name.as_deref(), avatar.as_deref())
        {
            Ok(_) => {
                service.bridge.count(channel, |c| c.to_discord += 1);
                return holding.said();
            }
            Err(BrokerError::Wait(secs)) if Duration::from_secs(secs) > SHORT_WAIT => {
                return holding.hold(service, channel, &wait(Duration::from_secs(secs)));
            }
            Err(BrokerError::Wait(_)) => {}
            Err(BrokerError::NotFound) if given.is_some() => {
                // The webhook its asker gave is gone: only its asker can give another.
                return not_carried(&format!(
                    "Discord no longer has the webhook given for the channel {channel}: its mirror needs another"
                ));
            }
            Err(BrokerError::NotFound) => {
                // Someone took the bot's webhook away: the bot makes another at once.
                lock(&service.bridge.hooks).remove(channel);
                lock(&service.bridge.lost).insert(channel.to_owned());
                return holding.hold(
                    service,
                    channel,
                    &format!("Discord no longer has the webhook of the channel {channel} of {to}"),
                );
            }
            Err(BrokerError::NameRefused) if name.is_some() => {
                service.bridge.log_now_and_then(
                    "name",
                    "Discord refused a speaker's address as a webhook's name: the line names its speaker itself",
                );
                name = None;
            }
            Err(e) => return not_carried(&e.to_string()),
        }
    }
    holding.hold(service, channel, "Discord asks the hub to wait");
}

// ---- the registry's mirrors ----------------------------------------------------------------------

/// The schema the mirrors add to the registry (its version 8): every mirror, the hub's admins'
/// (no world) and each world operator's, one to a channel and one to each room of an owner's; and
/// the log of the hub's own Discord settings, which belong to no world.
pub(crate) const SCHEMA: &str = "
    CREATE TABLE discord_mirrors (
      server_id TEXT REFERENCES servers(id) ON DELETE CASCADE,
      room TEXT NOT NULL,
      channel_id TEXT NOT NULL UNIQUE,
      guild_id TEXT NOT NULL,
      webhook_id TEXT,
      webhook_token TEXT,
      set_at TEXT NOT NULL);
    CREATE UNIQUE INDEX discord_mirrors_of_worlds ON discord_mirrors(server_id, room)
      WHERE server_id IS NOT NULL;
    CREATE UNIQUE INDEX discord_mirrors_of_the_hub ON discord_mirrors(room)
      WHERE server_id IS NULL;
    CREATE TABLE discord_events (
      id INTEGER PRIMARY KEY,
      kind TEXT NOT NULL, at TEXT NOT NULL, actor TEXT, details TEXT NOT NULL);
";

/// The columns a [`Mirror`] is read from, of `discord_mirrors` as `d` joined to `servers` as `s`.
const MIRROR_COLUMNS: &str =
    "s.id, s.slug, d.room, d.channel_id, d.guild_id, d.webhook_id, d.webhook_token, d.set_at";

/// A mirror read from [`MIRROR_COLUMNS`].
fn mirror_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<Mirror> {
    let id: Option<String> = r.get(0)?;
    let slug: Option<String> = r.get(1)?;
    let (hook_id, hook_token): (Option<String>, Option<String>) = (r.get(5)?, r.get(6)?);
    let server: String = r.get(4)?;
    let hook = match (hook_id, hook_token) {
        (Some(id), Some(token)) => Webhook::kept(&id, &server, &token),
        _ => None,
    };
    Ok(Mirror {
        world: id.zip(slug),
        room: r.get(2)?,
        placement: Placement {
            channel: r.get(3)?,
            server,
            hook,
        },
        set_at: r.get(7)?,
    })
}

/// A mirror's registry event: what it is now, or that it is gone.
fn mirror_event(room: &str, placement: Option<&Placement>) -> Value {
    match placement {
        Some(p) => {
            let mut v = json!({
                "room": room, "mirrored": true, "channel": p.channel, "server": p.server,
                "through": p.through(),
            });
            if let Some(hook) = &p.hook {
                v["webhook"] = json!(hook.id);
            }
            v
        }
        None => json!({ "room": room, "mirrored": false }),
    }
}

/// A channel that carries another mirror already.
fn channel_taken() -> RegistryError {
    RegistryError {
        reason: Some("channel_taken"),
        ..RegistryError::conflict("This Discord channel mirrors another room already")
    }
}

impl Registry {
    /// Every mirror: the hub's admins' first, then each world's by slug, each by room.
    ///
    /// # Errors
    /// Database errors only.
    pub fn mirrors(&self) -> Result<Vec<Mirror>, RegistryError> {
        let mut stmt = self.db.prepare(&format!(
            "SELECT {MIRROR_COLUMNS} FROM discord_mirrors d LEFT JOIN servers s ON s.id = d.server_id
             ORDER BY s.slug IS NOT NULL, s.slug, d.room"
        ))?;
        let rows = stmt
            .query_map([], mirror_of)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// The mirrors carried now: the hub's admins', and those of the worlds on the network.
    ///
    /// # Errors
    /// Database errors only.
    pub(crate) fn mirrored_now(&self) -> Result<Vec<Mirror>, RegistryError> {
        let mut stmt = self.db.prepare(&format!(
            "SELECT {MIRROR_COLUMNS} FROM discord_mirrors d LEFT JOIN servers s ON s.id = d.server_id
             WHERE d.server_id IS NULL OR s.federation = ?
             ORDER BY s.slug IS NOT NULL, s.slug, d.room"
        ))?;
        let rows = stmt
            .query_map([Federation::On.word()], mirror_of)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// The mirror in the Discord channel `channel`, whether its world is on the network or not.
    ///
    /// # Errors
    /// Database errors only.
    pub fn mirror_in_channel(&self, channel: &str) -> Result<Option<Mirror>, RegistryError> {
        Ok(self
            .db
            .query_row(
                &format!(
                    "SELECT {MIRROR_COLUMNS} FROM discord_mirrors d LEFT JOIN servers s ON s.id = d.server_id
                     WHERE d.channel_id = ?"
                ),
                [channel],
                mirror_of,
            )
            .optional()?)
    }

    /// The mirrors of the world `server_id`, or the hub's admins' when none.
    ///
    /// # Errors
    /// Database errors only.
    pub(crate) fn mirrors_of(&self, server_id: Option<&str>) -> Result<Vec<Mirror>, RegistryError> {
        Ok(self
            .mirrors()?
            .into_iter()
            .filter(|m| m.world.as_ref().map(|(id, _)| id.as_str()) == server_id)
            .collect())
    }

    /// Whether the channel `channel` carries a mirror other than `room` of the world `server_id`
    /// (of the hub's admins', when none).
    ///
    /// # Errors
    /// Database errors only.
    pub(crate) fn channel_taken_by_another(
        &self,
        channel: &str,
        server_id: Option<&str>,
        room: &str,
    ) -> Result<bool, RegistryError> {
        let found: Option<(Option<String>, String)> = self
            .db
            .query_row(
                "SELECT server_id, room FROM discord_mirrors WHERE channel_id = ?",
                [channel],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(found.is_some_and(|(id, r)| id.as_deref() != server_id || r != room))
    }

    /// Mirrors `room` of the world `slug` (the hub's admins' when none) as `placement` says, at
    /// `actor`'s asking, as a registry event of the world's, or of the hub's own Discord log:
    /// nothing changes when it is so already. The checks with Discord are the caller's.
    ///
    /// # Errors
    /// 404 for no such world; 409 for a locked world, for a channel that mirrors another room
    /// (`channel_taken`), or for a world's own room that carries the Global room there
    /// (`carries_global`); database errors.
    pub fn set_mirror(
        &self,
        slug: Option<&str>,
        room: &str,
        placement: &Placement,
        actor: Option<&str>,
    ) -> Result<(), RegistryError> {
        let row = match slug {
            Some(slug) => Some(self.row(slug)?.ok_or_else(RegistryError::not_found)?),
            None => None,
        };
        let id = row.as_ref().map(|r| r.id.as_str());
        let was = self
            .mirrors_of(id)?
            .into_iter()
            .find(|m| m.room == room)
            .map(|m| m.placement);
        if was.as_ref() == Some(placement) {
            return Ok(());
        }
        if row.as_ref().is_some_and(Row::is_locked) {
            return Err(RegistryError::conflict(
                "The hub's operator has locked this world",
            ));
        }
        if let Some(id) = id {
            self.check_own_room(id, room)?;
        }
        if self.channel_taken_by_another(&placement.channel, id, room)? {
            return Err(channel_taken());
        }
        self.db.execute(
            "DELETE FROM discord_mirrors WHERE server_id IS ?1 AND room = ?2",
            params![id, room],
        )?;
        let hook = placement.hook.as_ref();
        self.db.execute(
            "INSERT INTO discord_mirrors(server_id, room, channel_id, guild_id, webhook_id,
               webhook_token, set_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                room,
                placement.channel,
                placement.server,
                hook.map(|h| h.id.as_str()),
                hook.map(Webhook::token),
                iso(self.now()),
            ],
        )?;
        self.mirror_changed(id, room, Some(placement), actor)
    }

    /// Stops mirroring `room` of the world `slug` (of the hub's admins', when none): false when it
    /// was not mirrored.
    ///
    /// # Errors
    /// 404 for no such world; database errors.
    pub fn drop_mirror(
        &self,
        slug: Option<&str>,
        room: &str,
        actor: Option<&str>,
    ) -> Result<bool, RegistryError> {
        let id = match slug {
            Some(slug) => Some(self.row(slug)?.ok_or_else(RegistryError::not_found)?.id),
            None => None,
        };
        let gone = self.db.execute(
            "DELETE FROM discord_mirrors WHERE server_id IS ?1 AND room = ?2",
            params![id, room],
        )?;
        if gone > 0 {
            self.mirror_changed(id.as_deref(), room, None, actor)?;
        }
        Ok(gone > 0)
    }

    /// A mirror changed: an event of the world `server_id`'s, or of the hub's own Discord log.
    fn mirror_changed(
        &self,
        server_id: Option<&str>,
        room: &str,
        placement: Option<&Placement>,
        actor: Option<&str>,
    ) -> Result<(), RegistryError> {
        let details = mirror_event(room, placement);
        match server_id {
            Some(id) => self.event_by(id, "discord-mirror", actor, &details),
            None => self.discord_event("discord-mirror", actor, &details),
        }
    }

    /// An event of the hub's own Discord settings, which belong to no world.
    fn discord_event(
        &self,
        kind: &str,
        actor: Option<&str>,
        details: &Value,
    ) -> Result<(), RegistryError> {
        self.db.execute(
            "INSERT INTO discord_events(kind, at, actor, details) VALUES (?, ?, ?, ?)",
            params![kind, iso(self.now()), actor, details.to_string()],
        )?;
        Ok(())
    }

    /// The hub's own Discord log, newest first, at most `limit` events.
    ///
    /// # Errors
    /// Database errors only.
    pub fn discord_log(&self, limit: usize) -> Result<Vec<Value>, RegistryError> {
        let mut stmt = self.db.prepare(
            "SELECT kind, at, actor, details FROM discord_events ORDER BY id DESC LIMIT ?",
        )?;
        let rows = stmt
            .query_map([i64::try_from(limit).unwrap_or(i64::MAX)], |r| {
                let details: String = r.get(3)?;
                let actor: Option<String> = r.get(2)?;
                Ok(json!({
                    "kind": r.get::<_, String>(0)?,
                    "at": r.get::<_, String>(1)?,
                    "actor": actor.unwrap_or_else(|| "hub-operator".to_owned()),
                    "details": serde_json::from_str::<Value>(&details).unwrap_or(Value::Null),
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// The picture each mirrored line shows; none until an admin sets one.
    ///
    /// # Errors
    /// Database errors only.
    pub fn discord_avatar(&self) -> Result<Option<String>, RegistryError> {
        Ok(self
            .db
            .query_row(
                "SELECT value FROM hub_state WHERE name = ?",
                [AVATAR],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Sets the picture each mirrored line shows (none to show the webhook's own), at `actor`'s
    /// asking, as an event of the hub's own Discord log.
    ///
    /// # Errors
    /// 400 for an address that is not `https://`, past 500 bytes, or holding white space or a
    /// control character; database errors.
    pub fn set_discord_avatar(
        &self,
        url: Option<&str>,
        actor: Option<&str>,
    ) -> Result<(), RegistryError> {
        if let Some(u) = url {
            if !u.starts_with("https://")
                || u.len() > 500
                || u.chars().any(|c| c.is_whitespace() || c.is_control())
            {
                return Err(RegistryError::bad(
                    "url: an https:// address of at most 500 bytes, or null",
                ));
            }
        }
        if self.discord_avatar()?.as_deref() == url {
            return Ok(());
        }
        match url {
            Some(u) => self.db.execute(
                "INSERT INTO hub_state(name, value) VALUES (?1, ?2)
                 ON CONFLICT(name) DO UPDATE SET value = excluded.value",
                params![AVATAR, u],
            )?,
            None => self
                .db
                .execute("DELETE FROM hub_state WHERE name = ?", [AVATAR])?,
        };
        self.discord_event("discord-avatar", actor, &json!({ "url": url }))
    }
}

// ---- the routes ----------------------------------------------------------------------------------

/// What a `PUT` asks of a mirror: the channel the bot says its lines in, or the address of a
/// webhook its asker made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ask {
    Channel(String),
    Webhook(String),
}

impl Ask {
    /// A request's body: `{"channel": "<channel id>"}` or `{"webhook": "<a webhook's address>"}`.
    ///
    /// # Errors
    /// 400 for anything else.
    pub(crate) fn from_json(v: &Value) -> Result<Self, RegistryError> {
        let bad = || {
            RegistryError::bad(
                "The body is {\"channel\": a Discord channel's id} or {\"webhook\": the address of a webhook you made in the channel}",
            )
        };
        let o = v.as_object().filter(|o| o.len() == 1).ok_or_else(bad)?;
        match (o.get("channel"), o.get("webhook")) {
            (Some(Value::String(id)), None) if crate::broker::is_snowflake(id) => {
                Ok(Self::Channel(id.clone()))
            }
            (None, Some(Value::String(url))) => Ok(Self::Webhook(url.clone())),
            _ => Err(bad()),
        }
    }
}

/// The hub cannot ask Discord now.
fn discord_trouble(e: BrokerError) -> RegistryError {
    match e {
        BrokerError::Wait(secs) => RegistryError::limited(
            "Discord asks the hub to wait before it looks at the channel",
            secs,
        ),
        e => RegistryError::new(503, format!("Discord: {e}")),
    }
}

/// Whether the channel `channel` may carry a mirror at the asking of the Discord user `asker`: the
/// bot can see it, it is a server's text or announcement channel, and both the asker, a member of
/// its server, and the bot may view it and manage its webhooks ([`MIRROR_PERMISSIONS`]). Its
/// server's id.
///
/// # Errors
/// 409 when the bot cannot see it or lacks a permission in it; 403 when the asker may not manage
/// its webhooks; 400 for another kind of channel; 429 while Discord asks the hub to wait.
pub(crate) fn check_channel(
    service: &Service,
    channel: &str,
    asker: &str,
) -> Result<String, RegistryError> {
    let unseen = || {
        RegistryError {
        reason: Some("bot_cannot_see"),
        ..RegistryError::conflict(format!(
            "The hub's bot cannot see this Discord channel: add it to the channel's server (this page's invite), and give it {} on this channel alone",
            permission_names(MIRROR_PERMISSIONS)
        ))
    }
    };
    let seen = |r: Result<Value, BrokerError>| match r {
        Ok(v) => Ok(v),
        Err(BrokerError::NotFound | BrokerError::NoAccess) => Err(unseen()),
        Err(e) => Err(discord_trouble(e)),
    };
    let bot = service.bridge.bot(service).map_err(discord_trouble)?;
    let ch = seen(service.rest.channel(channel))?;
    let Some(guild_id) = ch["guild_id"].as_str().map(str::to_owned) else {
        return Err(RegistryError::bad("Not a Discord server's channel"));
    };
    if !matches!(ch["type"].as_u64(), Some(0 | 5)) {
        return Err(RegistryError::bad(
            "A room is mirrored into a server's text or announcement channel",
        ));
    }
    let guild = seen(service.rest.guild(&guild_id))?;
    let member = |user: &str| match service.rest.guild_member(&guild_id, user) {
        Ok(m) => Ok(m),
        Err(BrokerError::NoAccess) => Err(unseen()),
        Err(e) => Err(discord_trouble(e)),
    };
    let theirs = member(asker)?.is_some_and(|m| {
        permissions(&guild, asker, &m, &ch) & MIRROR_PERMISSIONS == MIRROR_PERMISSIONS
    });
    if !theirs {
        return Err(RegistryError {
            reason: Some("not_your_channel"),
            ..RegistryError::forbidden(
                "You may not manage this Discord channel's webhooks, so you may not mirror a room into it",
            )
        });
    }
    let member_bot = member(&bot)?.ok_or_else(unseen)?;
    let missing = MIRROR_PERMISSIONS & !permissions(&guild, &bot, &member_bot, &ch);
    if missing != 0 {
        return Err(RegistryError {
            reason: Some("bot_lacks_permissions"),
            ..RegistryError::conflict(format!(
                "The hub's bot lacks {} on this Discord channel: give it them there (the channel's settings, Permissions)",
                permission_names(missing)
            ))
        });
    }
    Ok(guild_id)
}

/// The webhook at `address`, which its asker made in a channel, read from Discord's own API with
/// its token: the webhook and its channel. Making a webhook needs Manage Webhooks in its channel,
/// so the address proves the asker's right to it, and the bot need not be in the server.
///
/// # Errors
/// 400 (`not_a_webhook`) for an address that is not one of Discord's webhooks'; 409
/// (`unknown_webhook`) for one Discord does not have; 429 or 503 when Discord cannot be asked.
pub(crate) fn check_webhook(
    service: &Service,
    address: &str,
) -> Result<(Webhook, String), RegistryError> {
    let Some((id, token)) = webhook_address(address) else {
        return Err(RegistryError {
            reason: Some("not_a_webhook"),
            ..RegistryError::bad(
                "Not a Discord webhook's address: in the channel's settings, Integrations, Webhooks, make one and copy its URL",
            )
        });
    };
    match service.rest.webhook(&id, &token) {
        Ok(found) => Ok(found),
        Err(BrokerError::NotFound | BrokerError::NoAccess) => Err(RegistryError {
            reason: Some("unknown_webhook"),
            ..RegistryError::conflict("Discord has no such webhook")
        }),
        Err(e) => Err(discord_trouble(e)),
    }
}

/// The broker, or `503` on a hub without it.
fn broker_of(shared: &Shared) -> Result<&Service, RegistryError> {
    shared.broker.as_ref().ok_or_else(|| {
        RegistryError::new(
            503,
            "This hub mirrors nothing into Discord: it has no bot token",
        )
    })
}

/// The address the hub's bot is invited to a Discord server at, with no permissions, and with its
/// `/dn` command (the scope `applications.commands`): each mirror's channel grants it what that
/// mirror needs, so the bot sees no other channel beyond what any member sees. None on a hub
/// without sign-in.
fn invite(shared: &Shared) -> Option<String> {
    let d = shared.config.discord.as_ref()?;
    Some(format!(
        "{}?client_id={}&scope=bot%20applications.commands&permissions=0",
        d.authorize,
        crate::discord::form_encode(&d.client_id)
    ))
}

/// The counts of `m`, and the bot's webhook in its channel, as the routes show them.
fn shown(service: &Service, m: &Mirror) -> Value {
    let counts = service.bridge.counts_of(m.channel());
    let hook = service.bridge.hook(m.channel()).map(|h| h.id);
    m.to_json(&counts, hook.as_deref())
}

/// A world's mirrors, as its operator sees them, with the room that carries the Global room there
/// (none while the hub holds none), which is not one of the rooms it may mirror.
fn world_view(shared: &Shared, service: &Service, row: &Row) -> Result<Value, RegistryError> {
    let (mirrors, global) = {
        let reg = shared.registry();
        (reg.mirrors_of(Some(&row.id))?, reg.global_room_of(&row.id)?)
    };
    let mut rooms: Vec<&str> = ROOMS.to_vec();
    rooms.extend(OWN_ROOMS.iter().filter(|r| global.as_deref() != Some(**r)));
    Ok(json!({
        "mirrors": mirrors.iter().map(|m| shown(service, m)).collect::<Vec<_>>(),
        "rooms": rooms,
        "globalRoom": global,
        "invite": invite(shared),
    }))
}

/// Mirrors `room` of the world `slug` (the hub's admins' when none) as `body` asks, at `op`'s
/// asking, once every check with Discord has passed.
///
/// # Errors
/// As [`check_channel`] and [`check_webhook`]; 400 for another body; 409 (`channel_taken`) for a
/// channel that mirrors another room; as [`Registry::set_mirror`].
fn put_mirror(
    shared: &Shared,
    service: &Service,
    slug: Option<&str>,
    room: &str,
    body: &Value,
    op: &Operator,
) -> Result<(), RegistryError> {
    let ask = Ask::from_json(body)?;
    let taken = |channel: &str| -> Result<(), RegistryError> {
        let reg = shared.registry();
        let id = match slug {
            Some(s) => Some(reg.row(s)?.ok_or_else(RegistryError::not_found)?.id),
            None => None,
        };
        if reg.channel_taken_by_another(channel, id.as_deref(), room)? {
            return Err(channel_taken());
        }
        Ok(())
    };
    let placement = match ask {
        Ask::Webhook(address) => {
            let (hook, channel) = check_webhook(service, &address)?;
            taken(&channel)?;
            Placement {
                channel,
                server: hook.guild.clone(),
                hook: Some(hook),
            }
        }
        Ask::Channel(channel) => {
            taken(&channel)?;
            let server = check_channel(service, &channel, &op.discord_id)?;
            Placement::bot(&channel, &server)
        }
    };
    shared
        .registry()
        .set_mirror(slug, room, &placement, Some(&op.id))?;
    refresh(shared);
    Ok(())
}

/// The operator route: `/v1/operator/worlds/<slug>/discord/mirrors[/<room>]`, for the world's
/// owner alone: its mirrors (`GET`), mirroring one as `body` asks (`PUT <room>`, never the room
/// that carries the Global room there), or stopping one (`DELETE <room>`).
///
/// # Errors
/// 404 or 403 for a world that is not the operator's; 503 without the broker; as
/// [`put_mirror`].
pub(crate) fn operator_mirrors(
    shared: &Shared,
    op: &Operator,
    method: &str,
    slug: &str,
    rest: &[&str],
    body: &Value,
) -> Result<(u16, Value), RegistryError> {
    let (row, role) = shared.registry().operator_world(op, slug)?;
    if role != Role::Owner {
        return Err(RegistryError::forbidden(
            "Verify your claim before you manage this world",
        ));
    }
    let service = broker_of(shared)?;
    match (method, rest) {
        ("GET", []) => Ok((200, world_view(shared, service, &row)?)),
        ("PUT", [room]) => {
            Bridged::of_world(&row.slug, room).map_err(RegistryError::bad)?;
            // Refused before Discord is asked anything.
            shared.registry().check_own_room(&row.id, room)?;
            put_mirror(shared, service, Some(&row.slug), room, body, op)?;
            Ok((200, world_view(shared, service, &row)?))
        }
        ("DELETE", [room]) => {
            if !shared
                .registry()
                .drop_mirror(Some(&row.slug), room, Some(&op.id))?
            {
                return Err(RegistryError::new(404, "This room is not mirrored"));
            }
            refresh(shared);
            Ok((204, Value::Null))
        }
        _ => Err(RegistryError::new(404, "Route not found")),
    }
}

/// Every mirror, the hub's admins' and each world's, as an admin sees them.
fn all_mirrors(shared: &Shared, service: &Service) -> Result<Vec<Value>, RegistryError> {
    let mirrors = shared.registry().mirrors()?;
    Ok(mirrors
        .iter()
        .map(|m| {
            let mut v = shown(service, m);
            v["world"] = json!(m.world.as_ref().map(|(_, slug)| slug));
            v
        })
        .collect())
}

/// The admin routes under `/v1/admin/discord`: what the mirrors and the tells into Discord carried
/// (`GET`), the hub's own Discord log (`GET log`), the picture mirrored lines show (`PUT avatar`),
/// the admins' own mirrors (`PUT`, `DELETE mirrors/<room>`), and any world operator's mirror taken
/// away (`DELETE worlds/<slug>/mirrors/<room>`).
///
/// # Errors
/// 503 without the broker; 404 for no such route, world or mirror; as [`put_mirror`].
pub(crate) fn admin(
    shared: &Shared,
    op: &Operator,
    method: &str,
    rest: &[&str],
    body: &Value,
) -> Result<(u16, Value), RegistryError> {
    let service = broker_of(shared)?;
    let view = |service: &Service| -> Result<Value, RegistryError> {
        Ok(json!({
            "invite": invite(shared),
            "avatarUrl": shared.registry().discord_avatar()?,
            "mirrors": all_mirrors(shared, service)?,
            "tells": crate::dms::admin_view(service),
        }))
    };
    let actor = Some(op.id.as_str());
    match (method, rest) {
        ("GET", []) => Ok((200, view(service)?)),
        ("GET", ["log"]) => Ok((
            200,
            json!({ "events": shared.registry().discord_log(200)? }),
        )),
        ("PUT", ["avatar"]) => {
            let url = match body
                .as_object()
                .filter(|o| o.len() == 1)
                .and_then(|o| o.get("url"))
            {
                Some(Value::String(u)) => Some(u.as_str()),
                Some(Value::Null) => None,
                _ => {
                    return Err(RegistryError::bad(
                        "The body is {\"url\": an https:// address, or null}",
                    ))
                }
            };
            shared.registry().set_discord_avatar(url, actor)?;
            refresh(shared);
            Ok((
                200,
                json!({ "avatarUrl": shared.registry().discord_avatar()? }),
            ))
        }
        ("PUT", ["mirrors", room]) => {
            let room = crate::server::percent_decode(room);
            let to = Bridged::named(&room).map_err(RegistryError::bad)?;
            put_mirror(shared, service, None, &to.route_name(), body, op)?;
            Ok((200, view(service)?))
        }
        ("DELETE", ["mirrors", room]) => {
            let room = crate::server::percent_decode(room);
            let to = Bridged::named(&room).map_err(RegistryError::bad)?;
            if !shared
                .registry()
                .drop_mirror(None, &to.route_name(), actor)?
            {
                return Err(RegistryError::new(404, "This room is not mirrored"));
            }
            refresh(shared);
            Ok((204, Value::Null))
        }
        ("DELETE", ["worlds", slug, "mirrors", room]) => {
            let slug = slug.to_ascii_lowercase();
            if !shared.registry().drop_mirror(Some(&slug), room, actor)? {
                return Err(RegistryError::new(404, "This room is not mirrored"));
            }
            refresh(shared);
            Ok((204, Value::Null))
        }
        _ => Err(RegistryError::new(404, "Route not found")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec: hub: a line from the game goes into Discord through the line rules, with Discord's
    /// mentions broken.
    #[test]
    fn a_line_into_discord_passes_the_text_rules_and_calls_nobody() {
        assert_eq!(for_discord("hello\nthere"), "hello there");
        assert_eq!(
            for_discord("hi <Tell:IIDString:1:Asheron>Asheron<\\Tell>"),
            "hi Asheron"
        );
        assert_eq!(for_discord("ping @everyone"), "ping @\u{200B}everyone");
        assert_eq!(for_discord("@here now"), "@\u{200B}here now");
        assert_eq!(
            for_discord("<@1234> and <#99>"),
            "<\u{200B}@1234> and <\u{200B}#99>"
        );
        assert_eq!(for_discord("   "), "");
        assert_eq!(for_discord(&"x".repeat(300)).chars().count(), 255);
    }

    /// Spec: hub: a line said in a server channel escapes every character Discord's markdown gives
    /// a meaning to, except within a link, and calls nobody.
    #[test]
    fn a_line_into_a_server_channel_shows_its_characters_as_written() {
        assert_eq!(for_channel("plain words"), "plain words");
        assert_eq!(
            for_channel("# loud *bold* __u__ ~~s~~ ||spoiler|| `code` > quote"),
            "\\# loud \\*bold\\* \\_\\_u\\_\\_ \\~\\~s\\~\\~ \\|\\|spoiler\\|\\| \\`code\\` \\> quote"
        );
        assert_eq!(
            for_channel("[free gold](https://example.test/x_y)"),
            "\\[free gold\\]\\(https://example.test/x\\_y\\)",
            "no masked link"
        );
        assert_eq!(
            for_channel("see https://example.test/a_b*c for it"),
            "see https://example.test/a_b*c for it",
            "a link as written"
        );
        assert_eq!(
            for_channel("-# small, and a back\\slash"),
            "\\-\\# small, and a back\\\\slash"
        );
        assert_eq!(
            for_channel("<@1234> @everyone"),
            "\\<\u{200B}@1234\\> @\u{200B}everyone"
        );
    }

    /// Spec: hub: a line said through a webhook shows its speaker's address, unless the address
    /// holds a word Discord refuses in a webhook's name.
    #[test]
    fn a_webhook_line_shows_its_speakers_address_unless_discord_refuses_it() {
        let name = |s: &str| webhook_name(&Address::parse(s).expect("an address"), None);
        assert_eq!(name("Ann@coldeve"), Some("Ann@coldeve".into()));
        assert_eq!(name("Sir Bob@frost"), Some("Sir Bob@frost".into()));
        assert_eq!(name("Clyde@coldeve"), None);
        assert_eq!(name("Ann@discordia"), None);
        assert_eq!(name("Bob DISCORD@frost"), None);
    }

    /// Spec: hub: a line said from Discord shows its mark after its speaker's address, in words
    /// Discord takes in a webhook's name, and where the address cannot be the name, the line
    /// begins with the address and the mark as a world shows it.
    #[test]
    fn a_line_said_from_discord_shows_its_mark_in_its_name() {
        let a = |s: &str| Address::parse(s).expect("an address");
        let name = webhook_name(&a("Ann@coldeve"), Some("discord")).expect("a name");
        assert_eq!(name, "Ann@coldeve (via /dn)");
        assert!(!name.to_lowercase().contains("discord"), "Discord takes it");
        assert_eq!(
            webhook_name(&a("Ann@coldeve"), Some("site")).as_deref(),
            Some("Ann@coldeve (via site)")
        );
        assert_eq!(webhook_name(&a("Clyde@coldeve"), Some("discord")), None);
        assert_eq!(
            speaker_in_line(&a("Clyde@coldeve"), Some("discord")),
            "Clyde@coldeve (Discord)"
        );
        assert_eq!(speaker_in_line(&a("Clyde@coldeve"), None), "Clyde@coldeve");
        assert_eq!(mark_shown("discord"), "Discord");
        assert_eq!(mark_shown(""), "");
    }

    /// Spec: hub: a member's permissions in a server channel are the server's owner's every one,
    /// an administrator's every one, and otherwise its roles' with the channel's overwrites for
    /// everyone, its roles and itself applied in that order.
    #[test]
    fn permissions_in_a_channel_are_worked_out_as_discord_does() {
        let s = |bits: u64| json!(bits.to_string());
        let guild = json!({
            "id": "100", "owner_id": "1",
            "roles": [
                { "id": "100", "permissions": s(VIEW_CHANNEL) },
                { "id": "200", "permissions": s(MANAGE_WEBHOOKS) },
                { "id": "300", "permissions": s(ADMINISTRATOR) },
                { "id": "400", "permissions": s(0) },
            ],
        });
        let member = |roles: &[&str]| json!({ "roles": roles });
        let open = json!({ "id": "9", "permission_overwrites": [] });
        assert_eq!(permissions(&guild, "1", &member(&[]), &open), u64::MAX);
        assert_eq!(permissions(&guild, "2", &member(&["300"]), &open), u64::MAX);
        assert_eq!(permissions(&guild, "2", &member(&[]), &open), VIEW_CHANNEL);
        assert_eq!(
            permissions(&guild, "2", &member(&["200"]), &open),
            MIRROR_PERMISSIONS
        );
        let closed = json!({ "id": "9", "permission_overwrites": [
            { "id": "100", "type": 0, "allow": "0", "deny": s(VIEW_CHANNEL) },
            { "id": "400", "type": 0, "allow": s(VIEW_CHANNEL), "deny": "0" },
            { "id": "5", "type": 1, "allow": "0", "deny": s(MANAGE_WEBHOOKS) },
        ] });
        assert_eq!(
            permissions(&guild, "2", &member(&["200"]), &closed) & VIEW_CHANNEL,
            0,
            "everyone denied the channel"
        );
        assert_eq!(
            permissions(&guild, "2", &member(&["200", "400"]), &closed),
            MIRROR_PERMISSIONS,
            "a role's overwrite lets it back in"
        );
        assert_eq!(
            permissions(&guild, "5", &member(&["200", "400"]), &closed) & MANAGE_WEBHOOKS,
            0,
            "the member's own overwrite last"
        );
        assert_eq!(
            permission_names(MANAGE_WEBHOOKS | VIEW_CHANNEL),
            "View Channel, Manage Webhooks"
        );
    }

    /// Spec: hub: a mirror is asked for by its channel or by a webhook its asker made; anything
    /// else is refused.
    #[test]
    fn a_mirror_is_asked_for_by_its_channel_or_by_a_webhook() {
        assert_eq!(
            Ask::from_json(&json!({ "channel": "1558376960051126342" })),
            Ok(Ask::Channel("1558376960051126342".into()))
        );
        assert_eq!(
            Ask::from_json(&json!({ "webhook": "https://discord.com/api/webhooks/1/x" })),
            Ok(Ask::Webhook("https://discord.com/api/webhooks/1/x".into()))
        );
        for bad in [
            Value::Null,
            json!({}),
            json!({ "channel": "general" }),
            json!({ "channel": 1_558_376_960_051_126_342_u64 }),
            json!({ "channel": "1", "way": "two" }),
            json!({ "way": "two" }),
            json!({ "channel": "1", "webhook": "https://discord.com/api/webhooks/1/x" }),
            json!([]),
        ] {
            assert_eq!(
                Ask::from_json(&bad).map_err(|e| e.status),
                Err(400),
                "{bad}"
            );
        }
    }

    /// Spec: hub: the admins mirror one of the network's rooms or a channel's address; a world's
    /// operator the Global room or one of its world's own four rooms, kept by name.
    #[test]
    fn a_mirrored_room_is_a_network_room_a_channel_or_a_worlds_own() {
        assert_eq!(Bridged::named("global"), Ok(Bridged::Room("global".into())));
        assert_eq!(
            Bridged::named("OldGuard@channel"),
            Ok(Bridged::Channel("oldguard".into()))
        );
        for bad in ["trade", "Bob@coldeve", "general@discord", ""] {
            assert!(Bridged::named(bad).is_err(), "{bad}");
        }
        assert_eq!(
            Bridged::of_world("coldeve", "global"),
            Ok(Bridged::Room("global".into()))
        );
        let own = Bridged::of_world("coldeve", "general").expect("its own");
        assert_eq!(own.to_string(), "coldeve/general");
        assert_eq!(own.route_name(), "general");
        for bad in ["society", "oldguard@channel", "General", ""] {
            assert!(Bridged::of_world("coldeve", bad).is_err(), "{bad}");
        }
        assert_eq!(
            Bridged::Channel("oldguard".into()).route_name(),
            "oldguard@channel"
        );
    }

    /// Spec: hub: a world's mirrors are its operator's to set, one to each room and one to each
    /// channel across the hub, the admins' too; asking for what is so changes nothing; each change
    /// is an event of the world's; they go with the world, a world registered later under the slug
    /// inherits none, and they are mirrored while the world is on the network; the admins' own
    /// changes go to the hub's own Discord log; a webhook's token is kept for its mirror alone.
    #[test]
    fn a_worlds_mirrors_are_kept_while_the_world_is_in_the_registry() {
        let r = Registry::open(
            ":memory:",
            "dereth.network",
            Box::new(std::time::SystemTime::now),
        )
        .expect("a registry");
        let registration = |slug: &str| {
            crate::registry::Registration::from_json(&json!({
                "slug": slug, "name": format!("World {slug}"), "ruleset": "PvE",
                "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
                "emulator": { "family": "ACE" },
            }))
            .expect("a registration")
        };
        let reg = registration("aa");
        r.register(&reg).expect("registered");
        r.register(&registration("bb")).expect("registered");
        r.set_federation("aa", Federation::On).expect("on");
        let row = r.row("aa").expect("read").expect("the world");
        let bot = |channel: &str| Placement::bot(channel, "500");
        r.set_mirror(Some("aa"), "general", &bot("71"), Some("op_1"))
            .expect("mirrored");
        r.set_mirror(Some("aa"), "general", &bot("71"), Some("op_1"))
            .expect("asked again, nothing changes");
        r.set_mirror(Some("aa"), "global", &bot("72"), Some("op_1"))
            .expect("Global, beside the admins'");
        r.set_mirror(None, "global", &bot("73"), Some("op_admin"))
            .expect("the admins' own");
        assert_eq!(
            r.set_mirror(Some("bb"), "general", &bot("71"), None)
                .map_err(|e| (e.status, e.reason)),
            Err((409, Some("channel_taken"))),
            "a channel carries one mirror"
        );
        assert_eq!(
            r.set_mirror(None, "oldguard@channel", &bot("72"), None)
                .map_err(|e| (e.status, e.reason)),
            Err((409, Some("channel_taken"))),
            "the admins' too"
        );
        let hook = Webhook::kept("9001", "500", "secret-token_1").expect("a webhook");
        let given = Placement {
            channel: "74".into(),
            server: "500".into(),
            hook: Some(hook),
        };
        r.set_mirror(Some("aa"), "trade", &given, Some("op_1"))
            .expect("through its own webhook");
        r.set_mirror(Some("aa"), "general", &bot("75"), None)
            .expect("into another channel");
        let mirrors = r.mirrors().expect("read");
        assert_eq!(
            mirrors
                .iter()
                .map(|m| (
                    m.world.as_ref().map(|(_, s)| s.as_str()),
                    m.room.as_str(),
                    m.channel(),
                    m.placement.through(),
                ))
                .collect::<Vec<_>>(),
            [
                (None, "global", "73", "bot"),
                (Some("aa"), "general", "75", "bot"),
                (Some("aa"), "global", "72", "bot"),
                (Some("aa"), "trade", "74", "webhook"),
            ]
        );
        assert_eq!(
            mirrors[3].placement.hook.as_ref().map(Webhook::token),
            Some("secret-token_1")
        );
        let audit = r.audit("aa").expect("the log").to_string();
        assert!(audit.contains(r#""kind":"discord-mirror""#), "{audit}");
        assert!(audit.contains(r#""through":"webhook""#), "{audit}");
        assert!(!audit.contains("secret-token_1"), "no token in the log");
        assert_eq!(
            r.discord_log(10).expect("the hub's log")[0]["details"],
            json!({ "room": "global", "mirrored": true, "channel": "73", "server": "500",
                    "through": "bot" })
        );
        assert_eq!(r.mirrored_now().expect("read").len(), 4);
        r.set_federation("aa", Federation::Off).expect("off");
        assert_eq!(
            r.mirrored_now().expect("read").len(),
            1,
            "mirrored while on the network"
        );
        assert_eq!(r.drop_mirror(Some("aa"), "trade", None), Ok(true));
        assert_eq!(r.drop_mirror(Some("aa"), "trade", None), Ok(false));
        r.remove("aa").expect("removed");
        assert_eq!(r.mirrors().expect("read").len(), 1, "gone with the world");
        r.register(&reg).expect("registered again");
        let again = r.row("aa").expect("read").expect("the world");
        assert_ne!(again.id, row.id);
        assert!(r.mirrors_of(Some(&again.id)).expect("read").is_empty());
        r.set_discord_avatar(Some("https://hub.test/a.png"), Some("op_admin"))
            .expect("set");
        assert_eq!(
            r.discord_avatar().expect("read").as_deref(),
            Some("https://hub.test/a.png")
        );
        assert_eq!(
            r.set_discord_avatar(Some("http://hub.test/a.png"), None)
                .map_err(|e| e.status),
            Err(400)
        );
        r.set_discord_avatar(None, Some("op_admin")).expect("unset");
        assert_eq!(r.discord_avatar().expect("read"), None);
        assert_eq!(r.discord_log(10).expect("the hub's log").len(), 3);
    }
}
