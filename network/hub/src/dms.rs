//! Tells into a person's Discord, and the person's replies back (`docs/network/05-hub.md` section
//! 5.8). Worlds speak the link protocol as they always have; they are asked one question more
//! (`reach`), and run no Discord code.
//!
//! - **Who.** A tell to a linked character reaches its person's Discord while the character is not
//!   online, with no setting to turn on. A refusal by Discord (the person shares no server with
//!   the bot, or lets none of its members message it) is remembered for
//!   [`crate::broker::Broker::refused_for`] ([`REFUSED_FOR`]): no tell is tried in that person's
//!   direct messages until it is over.
//! - **Out.** The routing hands over each tell whose world says it may go there
//!   ([`crate::core::DiscordTell`]), unless the character told is online. A thread of its own says
//!   it as the bot in the person's direct messages with the bot ([`dm_text`]: the teller, and the
//!   character it was for), within the person's pace ([`crate::broker::Broker::dm_pace`]) and the
//!   waits Discord names, the short ones waited out ([`crate::bridge::SHORT_WAIT`]). A tell that
//!   cannot go now is held back: counted, and logged at most once a minute, never dropped
//!   silently.
//! - **In.** Another thread reads, as the bot, the direct messages of each person a tell went to
//!   within the reply window ([`crate::broker::Broker::reply_window`]), after the last line it
//!   read, no more often than [`READS_PER_SECOND`] allows, all persons together. A line the person
//!   wrote there answers the tell it replies to (Discord's Reply), or else the latest one, and
//!   goes back to the routing as a tell from the character that tell was for, to its sender
//!   ([`crate::core::Hub::tell_from_discord`]). Nothing else is read: no window, no read; the bot's
//!   own lines, and anything but a line the person wrote or a reply, are never carried.
//! - **The answer** to a reply goes back to the person as a line of the bot's when the game would
//!   have told the character something ([`note_for`]).
//!
//! Nothing here keeps a tell or a line past the moment it is said or handed on, or logs one; the
//! reply windows (which tell went to whom, and when) are kept in memory alone.

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use dereth_network_protocol::limits::{Bucket, Rate};
use dereth_network_protocol::text::sanitize_line;
use dereth_network_protocol::{Address, Federation, Outcome};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::bridge::{for_channel, SHORT_WAIT};
use crate::broker::{dm_route, BrokerError, Service, OPEN_DM_ROUTE};
use crate::core::{DiscordAnswer, DiscordTell, TellsPerson};
use crate::registry::{Registry, RegistryError};
use crate::server::{log, Shared};

/// How long a person may answer a tell that went to its Discord.
pub const REPLY_WINDOW: Duration = Duration::from_secs(24 * 3_600);

/// How long, after Discord refuses a direct message to a person, no tell is tried in its direct
/// messages: the person may join a server the bot is in, or let its members message it, meanwhile.
pub const REFUSED_FOR: Duration = Duration::from_secs(3_600);

/// Each person's pace of tells into its Discord: a burst of 10, then one every 6 seconds.
pub const DMS_PER_PERSON: Rate = Rate {
    burst: 10.0,
    per_sec: 1.0 / 6.0,
};

/// The most tells that may wait to be said in persons' direct messages; one past it is held back.
pub const QUEUE_TELLS: usize = 1_000;

/// The world a tell a person answered from Discord is routed from, as the routing keeps it:
/// `discord`, which no world may take.
pub const DISCORD_WORLD: &str = "discord";

/// How often the hub reads a person's direct messages while a reply window is open.
pub const POLL: Duration = Duration::from_secs(3);

/// How many of a person's direct messages one read asks for.
pub const READ_LINES: u32 = 100;

/// The most reads of persons' direct messages the hub makes in a second, all persons together: a
/// tenth of the 50 requests a second Discord allows a bot. Past this many open reply windows at
/// [`POLL`], each is read less often.
pub const READS_PER_SECOND: u64 = 5;

/// The most tells one person's reply window remembers; the oldest goes first.
const MAX_SENT: usize = 100;

/// The least time between two log lines about one kind of trouble.
const LOG_EVERY: Duration = Duration::from_secs(60);

/// What became of a person's reply from Discord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carry {
    /// Handed to the world it goes to.
    Carried,
    /// Over the paces of its character's tells: not handed on.
    HeldBack,
    /// Its character is no longer the person's, or the person's account is gone: not handed on.
    NotCarried,
}

/// What the tells into persons' Discord carried since the hub started, each way.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub to_discord: u64,
    pub from_discord: u64,
    pub held_to_discord: u64,
    pub held_from_discord: u64,
    pub not_carried: u64,
}

impl Counts {
    /// As the hub's answers show it.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "toDiscord": self.to_discord,
            "fromDiscord": self.from_discord,
            "heldBackToDiscord": self.held_to_discord,
            "heldBackFromDiscord": self.held_from_discord,
            "notCarried": self.not_carried,
        })
    }
}

impl Registry {
    /// The person's Discord user id, while its account is not disabled.
    ///
    /// # Errors
    /// Database errors only.
    pub(crate) fn tells_of(&self, person: &str) -> Result<Option<String>, RegistryError> {
        Ok(self
            .db
            .query_row(
                "SELECT discord_id FROM operators WHERE id = ? AND disabled_at IS NULL",
                [person],
                |r| r.get::<_, String>(0),
            )
            .optional()?)
    }

    /// The person of `who` while it links `who`, on a world on the network, `who` on the network
    /// itself (a link pauses while its character is off), its account not disabled: a tell to
    /// `who` may reach its Discord, and the person's reply comes back from it.
    ///
    /// # Errors
    /// Database errors only.
    pub fn tells_person(&self, who: &Address) -> Result<Option<TellsPerson>, RegistryError> {
        Ok(self
            .db
            .query_row(
                &format!(
                    "SELECT c.person_id FROM person_characters c
                       JOIN operators o ON o.id = c.person_id
                       JOIN servers s ON s.slug = c.world
                     WHERE c.address_key = ?1 AND o.disabled_at IS NULL AND s.federation = ?2
                       AND {}",
                    crate::directory::on_network_sql("c.address_key", "?3")
                ),
                params![
                    who.key(),
                    Federation::On.word(),
                    crate::registry::iso(self.now())
                ],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .map(|person| TellsPerson { person }))
    }
}

/// A tell said in a person's direct messages, which the person may answer within the window.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Sent {
    /// The line's id.
    id: u64,
    /// Who told.
    from: Address,
    /// The person's character it was for, which a reply answers as.
    to: Address,
    at: Instant,
}

/// One person's direct messages with the bot, as the hub keeps them while it runs.
#[derive(Debug, Default)]
struct Conversation {
    /// The person's Discord user id, the only author whose lines are read.
    user: String,
    /// The direct messages' channel, once opened.
    channel: Option<String>,
    /// The last line read.
    read_to: u64,
    /// The tells said there within the reply window, oldest first.
    sent: VecDeque<Sent>,
}

/// What waits to be said in a person's direct messages.
#[derive(Debug)]
enum Job {
    Tell(DiscordTell),
    /// A line of the bot's about a reply's answer.
    Note {
        person: String,
        text: String,
    },
}

/// What the tells into Discord keep while the hub runs.
pub(crate) struct State {
    queue: SyncSender<Job>,
    waiting: Mutex<Option<Receiver<Job>>>,
    persons: Mutex<HashMap<String, Conversation>>,
    paces: Mutex<HashMap<String, Bucket>>,
    /// The persons Discord refused a direct message to, and when.
    refused: Mutex<HashMap<String, Instant>>,
    counts: Mutex<Counts>,
    logged: Mutex<HashMap<&'static str, Instant>>,
    started: Instant,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("persons", &lock(&self.persons).len())
            .finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl State {
    pub(crate) fn new() -> Self {
        let (queue, waiting) = mpsc::sync_channel(QUEUE_TELLS);
        Self {
            queue,
            waiting: Mutex::new(Some(waiting)),
            persons: Mutex::new(HashMap::new()),
            paces: Mutex::new(HashMap::new()),
            refused: Mutex::new(HashMap::new()),
            counts: Mutex::new(Counts::default()),
            logged: Mutex::new(HashMap::new()),
            started: Instant::now(),
        }
    }

    /// The routing's tells for persons' Discord: each waits to be said, or is held back when
    /// [`QUEUE_TELLS`] wait already.
    pub(crate) fn hand(&self, tells: Vec<DiscordTell>) {
        for t in tells {
            if let Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) =
                self.queue.try_send(Job::Tell(t))
            {
                self.count(|c| c.held_to_discord += 1);
                self.log_now_and_then(
                    "dm queue",
                    &format!(
                        "holding back tells into Discord: {QUEUE_TELLS} wait to be said already"
                    ),
                );
            }
        }
    }

    /// The answers to replies from Discord: a line of the bot's for each the game would have
    /// told the character of.
    pub(crate) fn answered(&self, answers: Vec<DiscordAnswer>) {
        for a in answers {
            if let Some(text) = note_for(&a) {
                let _ = self.queue.try_send(Job::Note {
                    person: a.person,
                    text,
                });
            }
        }
    }

    /// A reply from Discord the routing took (or held back, or did not carry).
    pub(crate) fn taken(&self, carry: Carry) {
        match carry {
            Carry::Carried => self.count(|c| c.from_discord += 1),
            Carry::HeldBack => self.count(|c| c.held_from_discord += 1),
            Carry::NotCarried => self.count(|c| c.not_carried += 1),
        }
    }

    /// The person's direct messages are closed to the bot (or it is forgotten): its replies are
    /// read no more.
    pub(crate) fn forget(&self, person: &str) {
        lock(&self.persons).remove(person);
    }

    /// Discord refused a direct message to `person` at `now`: none is tried for `wait`.
    fn refuse(&self, person: &str, now: Instant, wait: Duration) {
        let mut refused = lock(&self.refused);
        refused.retain(|_, at| now.duration_since(*at) < wait);
        refused.insert(person.to_owned(), now);
    }

    /// Whether Discord refused a direct message to `person` within `wait` of `now`.
    fn refused_lately(&self, person: &str, now: Instant, wait: Duration) -> bool {
        lock(&self.refused)
            .get(person)
            .is_some_and(|at| now.duration_since(*at) < wait)
    }

    /// What was carried since the hub started.
    pub(crate) fn counts(&self) -> Counts {
        *lock(&self.counts)
    }

    /// How many persons' replies are read now.
    pub(crate) fn open_windows(&self, window: Duration) -> usize {
        let now = Instant::now();
        lock(&self.persons)
            .values()
            .filter(|c| c.sent.iter().any(|s| now.duration_since(s.at) < window))
            .count()
    }

    fn count(&self, f: impl FnOnce(&mut Counts)) {
        f(&mut lock(&self.counts));
    }

    /// Takes one of the person's pace at `now`: false when it is spent.
    fn pace_takes(&self, person: &str, rate: Rate) -> bool {
        let now = self.started.elapsed().as_secs_f64();
        let mut paces = lock(&self.paces);
        if paces.len() >= 10_000 {
            paces.retain(|_, b| !b.full_at(now));
        }
        paces
            .entry(person.to_owned())
            .or_insert_with(|| Bucket::new(rate, now))
            .take(now)
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

    /// The channel of the person's direct messages, as opened before.
    fn channel_of(&self, person: &str) -> Option<String> {
        lock(&self.persons)
            .get(person)
            .and_then(|c| c.channel.clone())
    }

    /// The tell `sent` was said in the person's direct messages, `channel`: it may be answered
    /// within `window` of it. A window that opens now reads only what follows the tell.
    fn said(&self, person: &str, user: &str, channel: &str, sent: Sent, window: Duration) {
        let mut persons = lock(&self.persons);
        let c = persons.entry(person.to_owned()).or_default();
        c.sent.retain(|s| sent.at.duration_since(s.at) < window);
        if c.sent.is_empty() || c.channel.as_deref() != Some(channel) {
            c.read_to = c.read_to.max(sent.id);
        }
        c.user = user.to_owned();
        c.channel = Some(channel.to_owned());
        c.sent.push_back(sent);
        while c.sent.len() > MAX_SENT {
            c.sent.pop_front();
        }
    }

    /// The persons whose replies are read now, each with its channel: its window open (a tell
    /// said there within it). Tells past it are forgotten.
    fn open(&self, now: Instant, window: Duration) -> Vec<(String, String)> {
        let mut persons = lock(&self.persons);
        let mut out = Vec::new();
        for (person, c) in persons.iter_mut() {
            c.sent.retain(|s| now.duration_since(s.at) < window);
            if let (false, Some(channel)) = (c.sent.is_empty(), &c.channel) {
                out.push((person.clone(), channel.clone()));
            }
        }
        out
    }
}

/// A tell as it is said in a person's direct messages: who told which of its characters, and what,
/// through the line rules with Discord's markdown escaped ([`for_channel`]), and how to answer.
#[must_use]
pub fn dm_text(from: &Address, to: &Address, text: &str) -> String {
    format!(
        "{} tells {}, \"{}\"\n-# Reply to answer as {}.",
        for_channel(&from.to_string()),
        for_channel(&to.to_string()),
        for_channel(text),
        for_channel(&to.to_string()),
    )
}

/// The bot's line about a reply's answer, when the game would have told the character something:
/// that the recipient is away (with its away text), not available, or that the network holds its
/// tells back; none for a reply shown, or one the recipient squelches (the game says nothing of
/// that either).
#[must_use]
pub fn note_for(a: &DiscordAnswer) -> Option<String> {
    let to = for_channel(&a.to.to_string());
    match a.outcome {
        Outcome::Delivered | Outcome::Squelched | Outcome::Ok | Outcome::NotMember => None,
        Outcome::Afk => Some(match a.message.as_deref().map(for_channel) {
            Some(away) if !away.is_empty() => format!("{to} is away: {away}"),
            _ => format!("{to} is away."),
        }),
        Outcome::RateLimited => Some(format!(
            "The Dereth Network is holding back tells to {to} for now. Wait a moment and try again."
        )),
        Outcome::Unavailable => Some(format!(
            "{to} is not available: the Dereth Network can't reach {} right now.",
            a.to.world()
        )),
        Outcome::NotOnNetwork | Outcome::Offline => Some(format!("{to} is not available.")),
    }
}

/// Whether a line read from a person's direct messages is one `user`, the person, wrote: neither
/// a bot's (the hub's own tells and notes among them) nor a webhook's nor Discord's, and a line
/// or a reply, nothing else (a pin, a call).
#[must_use]
pub fn persons_line(line: &Value, user: &str) -> bool {
    let author = &line["author"];
    line["webhook_id"].is_null()
        && author["bot"] != true
        && author["system"] != true
        && author["id"].as_str() == Some(user)
        && matches!(line["type"].as_u64(), Some(0 | 19))
}

/// The tell a person's line answers: the one it replies to (Discord's Reply), while within the
/// window; or else the latest within it. None when no tell is.
fn answering<'a>(
    sent: &'a VecDeque<Sent>,
    line: &Value,
    now: Instant,
    window: Duration,
) -> Option<&'a Sent> {
    let open = |s: &&Sent| now.duration_since(s.at) < window;
    let replied: Option<u64> = (line["type"].as_u64() == Some(19))
        .then(|| line["message_reference"]["message_id"].as_str())
        .flatten()
        .and_then(|i| i.parse().ok());
    match replied.and_then(|id| sent.iter().find(|s| s.id == id)) {
        Some(s) => Some(s).filter(open),
        None => sent.iter().rev().find(open),
    }
}

/// Starts the two threads: one says tells (and notes) in persons' direct messages, one reads their
/// replies.
///
/// # Errors
/// When a thread cannot start.
pub(crate) fn start(shared: &Arc<Shared>) -> std::io::Result<Vec<JoinHandle<()>>> {
    if shared.broker.is_none() {
        return Ok(Vec::new());
    }
    let (s1, s2) = (Arc::clone(shared), Arc::clone(shared));
    Ok(vec![
        std::thread::Builder::new()
            .name("hub-dms".into())
            .spawn(move || say_tells(&s1))?,
        std::thread::Builder::new()
            .name("hub-dm-replies".into())
            .spawn(move || read_replies(&s2))?,
    ])
}

/// Says the routing's tells and the notes about replies, one at a time, in order.
fn say_tells(shared: &Arc<Shared>) {
    let Some(service) = shared.broker.as_ref() else {
        return;
    };
    let Some(waiting) = lock(&service.tells.waiting).take() else {
        return;
    };
    while !shared.stopping() {
        match waiting.recv_timeout(Duration::from_millis(100)) {
            Ok(Job::Tell(t)) => say_tell(shared, service, &t),
            Ok(Job::Note { person, text }) => say_note(shared, service, &person, &text),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Says `t` in its person's direct messages, or holds it back.
fn say_tell(shared: &Shared, service: &Service, t: &DiscordTell) {
    let st = &service.tells;
    let settings = service.rest.broker();
    let not_carried = |why: &str| {
        st.count(|c| c.not_carried += 1);
        st.log_now_and_then(
            "dm not carried",
            &format!("a tell into a person's Discord was not carried: {why}"),
        );
    };
    let held = |why: &str| {
        st.count(|c| c.held_to_discord += 1);
        st.log_now_and_then(
            "dm held",
            &format!("holding back tells into persons' Discord: {why}"),
        );
    };
    let known = match shared.registry().tells_of(&t.person) {
        Ok(k) => k,
        Err(e) => return not_carried(&e.message),
    };
    let Some(user) = known else {
        return not_carried("the person's account is gone or disabled");
    };
    if st.refused_lately(&t.person, Instant::now(), settings.refused_for) {
        return not_carried("Discord refused a direct message to the person lately");
    }
    if !st.pace_takes(&t.person, settings.dm_pace) {
        return held("over the person's pace");
    }
    let content = dm_text(&t.from, &t.to, &t.text);
    match deliver(shared, service, &t.person, &user, &content) {
        Ok((channel, id)) => {
            st.count(|c| c.to_discord += 1);
            st.said(
                &t.person,
                &user,
                &channel,
                Sent {
                    id,
                    from: t.from.clone(),
                    to: t.to.clone(),
                    at: Instant::now(),
                },
                settings.reply_window,
            );
        }
        Err(BrokerError::Wait(secs)) => {
            held(&format!("Discord asks the hub to wait {secs} seconds"));
        }
        Err(BrokerError::NoAccess) => {
            st.refuse(&t.person, Instant::now(), settings.refused_for);
            st.forget(&t.person);
            not_carried("Discord refused the direct message; none is tried for a while");
        }
        Err(e) => not_carried(&e.to_string()),
    }
}

/// Says a note about a reply's answer in the person's direct messages; nothing when it cannot.
fn say_note(shared: &Shared, service: &Service, person: &str, text: &str) {
    let wait = service.rest.broker().refused_for;
    let user = match shared.registry().tells_of(person) {
        Ok(Some(user)) if !service.tells.refused_lately(person, Instant::now(), wait) => user,
        _ => return,
    };
    if let Err(e) = deliver(shared, service, person, &user, text) {
        service.tells.log_now_and_then(
            "dm note",
            &format!("a note in a person's Discord was not said: {e}"),
        );
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

/// A wait Discord names on `route`, waited out when it is short; [`BrokerError::Wait`] when it is
/// longer than [`SHORT_WAIT`].
fn wait_out(shared: &Shared, service: &Service, route: &str) -> Result<(), BrokerError> {
    if let Some(left) = service.rest.waiting(route) {
        if left > SHORT_WAIT {
            return Err(BrokerError::Wait(
                left.as_secs() + u64::from(left.subsec_nanos() > 0),
            ));
        }
        pause(shared, left);
    }
    Ok(())
}

/// Says `content` in `person`'s direct messages with the bot (its Discord user `user`), opening
/// them first when the hub has not yet, and once again should Discord no longer have them: the
/// channel and the line's id.
fn deliver(
    shared: &Shared,
    service: &Service,
    person: &str,
    user: &str,
    content: &str,
) -> Result<(String, u64), BrokerError> {
    for _ in 0..2 {
        let channel = match service.tells.channel_of(person) {
            Some(c) => c,
            None => {
                wait_out(shared, service, OPEN_DM_ROUTE)?;
                let c = service.rest.open_dm(user)?;
                let mut persons = lock(&service.tells.persons);
                let conv = persons.entry(person.to_owned()).or_default();
                conv.user = user.to_owned();
                conv.channel = Some(c.clone());
                c
            }
        };
        let mut sent = Err(BrokerError::Wait(0));
        for _ in 0..3 {
            wait_out(shared, service, &dm_route(&channel))?;
            sent = service.rest.send_dm(&channel, content);
            match &sent {
                Err(BrokerError::Wait(secs)) if Duration::from_secs(*secs) <= SHORT_WAIT => {}
                _ => break,
            }
        }
        match sent {
            Ok(id) => {
                let id = id
                    .parse()
                    .map_err(|_| BrokerError::Failed("Discord gave no line id".into()))?;
                return Ok((channel, id));
            }
            Err(BrokerError::NotFound) => {
                // Discord no longer has them: they are opened again.
                if let Some(c) = lock(&service.tells.persons).get_mut(person) {
                    c.channel = None;
                    c.sent.clear();
                }
            }
            Err(e) => return Err(e),
        }
    }
    Err(BrokerError::NotFound)
}

/// Reads the direct messages of each person whose reply window is open, each within its poll, no
/// more often than [`READS_PER_SECOND`] allows all of them together.
fn read_replies(shared: &Arc<Shared>) {
    let Some(service) = shared.broker.as_ref() else {
        return;
    };
    let settings = service.rest.broker().clone();
    let mut due: HashMap<String, Instant> = HashMap::new();
    while !shared.stopping() {
        let now = Instant::now();
        let open = service.tells.open(now, settings.reply_window);
        due.retain(|p, _| open.iter().any(|(o, _)| o == p));
        let poll = settings.poll.max(Duration::from_millis(
            u64::try_from(open.len()).unwrap_or(u64::MAX) * 1_000 / READS_PER_SECOND,
        ));
        for (person, channel) in open {
            if now < *due.entry(person.clone()).or_insert(now) {
                continue;
            }
            let next = match read_dm(shared, service, &person, &channel, settings.reply_window) {
                Ok(true) => Duration::ZERO,
                Ok(false) => poll,
                Err(BrokerError::Wait(secs)) => poll.max(Duration::from_secs(secs)),
                Err(e @ (BrokerError::NotFound | BrokerError::NoAccess)) => {
                    // Discord no longer lets the bot read them: the window closes.
                    service.tells.log_now_and_then(
                        "dm read",
                        &format!(
                            "reading a person's direct messages: {e}; its replies are read no more"
                        ),
                    );
                    service.tells.forget(&person);
                    poll
                }
                Err(e) => {
                    service.tells.log_now_and_then(
                        "dm read",
                        &format!("reading a person's direct messages: {e}"),
                    );
                    poll.max(settings.retry)
                }
            };
            due.insert(person, Instant::now() + next);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Reads `person`'s direct messages `channel` after the last line read, and hands the routing each
/// line the person wrote there that answers a tell within the window. True when the read was full,
/// and more may wait.
fn read_dm(
    shared: &Shared,
    service: &Service,
    person: &str,
    channel: &str,
    window: Duration,
) -> Result<bool, BrokerError> {
    let Some(read_to) = lock(&service.tells.persons).get(person).map(|c| c.read_to) else {
        return Ok(false);
    };
    let id_of = |l: &Value| l["id"].as_str().and_then(|i| i.parse::<u64>().ok());
    let mut lines: Vec<(u64, Value)> = service
        .rest
        .channel_lines(channel, Some(read_to), READ_LINES)?
        .into_iter()
        .filter_map(|l| Some((id_of(&l)?, l)))
        .filter(|(id, _)| *id > read_to)
        .collect();
    lines.sort_by_key(|(id, _)| *id);
    let full = lines.len() >= usize::try_from(READ_LINES).unwrap_or(usize::MAX);
    for (id, line) in lines {
        let answers = {
            let mut persons = lock(&service.tells.persons);
            let Some(c) = persons.get_mut(person) else {
                return Ok(false);
            };
            c.read_to = c.read_to.max(id);
            if !persons_line(&line, &c.user) {
                continue;
            }
            answering(&c.sent, &line, Instant::now(), window)
                .map(|s| (s.from.clone(), s.to.clone()))
        };
        let text = channel_text(&line);
        match answers.filter(|_| !text.is_empty()) {
            // The person answers as the character the tell was for, to the one who told it.
            Some((teller, character)) => {
                shared.tell_from_discord(person.to_owned(), character, teller, text);
            }
            None => service.tells.count(|c| c.not_carried += 1),
        }
    }
    Ok(full)
}

pub fn plain_text(content: &str, mentions: &Value) -> String {
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        match tail
            .find('>')
            .and_then(|end| Some((end, plain_markup(&tail[1..end], mentions)?)))
        {
            Some((end, plain)) => {
                out.push_str(&plain);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('<');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// One piece of Discord's markup, `inner` being what is between its `<` and `>`; none when it is
/// not one.
fn plain_markup(inner: &str, mentions: &Value) -> Option<String> {
    let digits = |s: &str| !s.is_empty() && s.len() <= 24 && s.bytes().all(|b| b.is_ascii_digit());
    if let Some(id) = inner.strip_prefix("@&") {
        return digits(id).then(|| "@role".to_owned());
    }
    if let Some(id) = inner.strip_prefix('@') {
        let id = id.strip_prefix('!').unwrap_or(id);
        if !digits(id) {
            return None;
        }
        let user = mentions
            .as_array()
            .and_then(|m| m.iter().find(|u| u["id"] == id));
        let name = user
            .and_then(|u| {
                [&u["member"]["nick"], &u["global_name"], &u["username"]]
                    .into_iter()
                    .find_map(Value::as_str)
            })
            .unwrap_or("someone");
        return Some(format!("@{name}"));
    }
    if let Some(id) = inner.strip_prefix('#') {
        return digits(id).then(|| "#channel".to_owned());
    }
    let emoji = inner
        .strip_prefix("a:")
        .or_else(|| inner.strip_prefix(':'))?;
    let (name, id) = emoji.split_once(':')?;
    let is_name = !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    (is_name && digits(id)).then(|| format!(":{name}:"))
}

/// A person's line read from its direct messages as the game is shown it: its markup made plain
/// ([`plain_text`]), led by a note of its attachments (`[attachment]`, `[3 attachments]`), through
/// the line rules. Its embeds and stickers are not carried.
#[must_use]
pub fn channel_text(line: &Value) -> String {
    let text = plain_text(line["content"].as_str().unwrap_or(""), &line["mentions"]);
    let text = match line["attachments"].as_array().map_or(0, Vec::len) {
        0 => text,
        1 => format!("[attachment] {text}"),
        n => format!("[{n} attachments] {text}"),
    };
    sanitize_line(&text)
}

/// The tells into Discord as the admin route shows them: what was carried since the hub started,
/// and how many persons' replies are read now.
pub(crate) fn admin_view(service: &Service) -> Value {
    let mut v = service.tells.counts().to_json();
    v["replyWindows"] = json!(service
        .tells
        .open_windows(service.rest.broker().reply_window));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &str) -> Address {
        Address::parse(s).expect("an address")
    }

    /// Spec: hub: a tell said in a person's direct messages names who told and the character it
    /// was for, its text through the line rules with Discord's markdown escaped and mentions
    /// broken, and how to answer.
    #[test]
    fn a_direct_message_names_the_teller_and_the_character_it_was_for() {
        assert_eq!(
            dm_text(&a("Bob@coldeve"), &a("Ann@frost"), "still playing?"),
            "Bob@coldeve tells Ann@frost, \"still playing?\"\n-# Reply to answer as Ann@frost."
        );
        let marked = dm_text(
            &a("Sir Bob@cold-eve"),
            &a("Ann@frost"),
            "**loud** @everyone <Tell:IIDString:1:Asheron>Asheron<\\Tell>\nnext",
        );
        assert!(
            marked.starts_with(
                "Sir Bob@cold\\-eve tells Ann@frost, \"\\*\\*loud\\*\\* @\u{200B}everyone Asheron next\""
            ),
            "{marked}"
        );
    }

    /// Spec: hub: the answer to a reply from Discord is told the person as the game tells the
    /// character: away with its away text, not available, or held back; nothing for a reply shown
    /// or squelched.
    #[test]
    fn a_replys_answer_is_told_as_the_game_tells_it() {
        let answer = |outcome, message: Option<&str>| DiscordAnswer {
            person: "p".into(),
            from: a("Ann@frost"),
            to: a("Bob@coldeve"),
            outcome,
            message: message.map(str::to_owned),
        };
        assert_eq!(note_for(&answer(Outcome::Delivered, None)), None);
        assert_eq!(note_for(&answer(Outcome::Squelched, None)), None);
        assert_eq!(
            note_for(&answer(Outcome::Afk, Some("back in *5*"))),
            Some("Bob@coldeve is away: back in \\*5\\*".into())
        );
        for o in [Outcome::Offline, Outcome::NotOnNetwork] {
            assert_eq!(
                note_for(&answer(o, None)),
                Some("Bob@coldeve is not available.".into())
            );
        }
        assert!(note_for(&answer(Outcome::Unavailable, None))
            .is_some_and(|n| n.contains("can't reach coldeve")));
        assert!(note_for(&answer(Outcome::RateLimited, None))
            .is_some_and(|n| n.contains("holding back")));
    }

    /// Spec: hub: of a person's direct messages, only a line or a reply the person wrote is read;
    /// the bot's own, a webhook's, Discord's and any other kind never are.
    #[test]
    fn only_the_persons_own_lines_are_read_from_its_direct_messages() {
        let person = json!({ "id": "7", "username": "nelly" });
        let line = |extra: Value| {
            let mut v = json!({ "id": "10", "type": 0, "content": "hi", "author": person });
            for (k, x) in extra.as_object().expect("an object") {
                v[k] = x.clone();
            }
            v
        };
        assert!(persons_line(&line(json!({})), "7"));
        assert!(persons_line(&line(json!({ "type": 19 })), "7"), "a reply");
        assert!(!persons_line(&line(json!({})), "8"), "another author");
        assert!(!persons_line(
            &line(json!({ "author": { "id": "7", "bot": true } })),
            "7"
        ));
        assert!(!persons_line(
            &line(json!({ "author": { "id": "99", "username": "the bot", "bot": true } })),
            "7"
        ));
        assert!(!persons_line(&line(json!({ "webhook_id": "5" })), "7"));
        assert!(!persons_line(
            &line(json!({ "author": { "id": "7", "system": true } })),
            "7"
        ));
        assert!(!persons_line(&line(json!({ "type": 6 })), "7"), "a pin");
    }

    /// Spec: hub: a person's line answers the tell it replies to while that one is within the
    /// window, or else the latest within it, and none past it.
    #[test]
    fn a_line_answers_the_tell_it_replies_to_or_the_latest_within_the_window() {
        let t0 = Instant::now();
        let window = Duration::from_secs(100);
        let sent: VecDeque<Sent> = [
            (10, "Bob@coldeve", 0),
            (20, "Cat@frost", 50),
            (30, "Dan@leaf", 90),
        ]
        .into_iter()
        .map(|(id, from, secs)| Sent {
            id,
            from: a(from),
            to: a("Ann@aa"),
            at: t0 + Duration::from_secs(secs),
        })
        .collect();
        let plain = json!({ "id": "40", "type": 0 });
        let reply_to =
            |id: &str| json!({ "id": "41", "type": 19, "message_reference": { "message_id": id } });
        let at = |secs: u64| t0 + Duration::from_secs(secs);
        let who =
            |l: &Value, secs| answering(&sent, l, at(secs), window).map(|s| s.from.to_string());
        assert_eq!(who(&plain, 95), Some("Dan@leaf".into()), "the latest");
        assert_eq!(who(&reply_to("20"), 95), Some("Cat@frost".into()));
        assert_eq!(who(&reply_to("10"), 95), Some("Bob@coldeve".into()));
        assert_eq!(
            who(&reply_to("10"), 120),
            None,
            "a reply to a tell past the window answers none"
        );
        assert_eq!(
            who(&reply_to("99"), 120),
            Some("Dan@leaf".into()),
            "a reply to anything else answers the latest"
        );
        assert_eq!(who(&plain, 190), None, "past every tell's window");
    }

    /// Spec: hub: a tell to a linked character may reach its person's Discord with no setting
    /// turned on, while the character is on the network; a character off the network (its link
    /// paused until it is back on), one on a world off the network, one nobody linked and a
    /// disabled person's take none.
    #[test]
    fn a_linked_characters_tells_reach_its_persons_discord_without_a_setting() {
        let r = Registry::open(
            ":memory:",
            "dereth.network",
            Box::new(std::time::SystemTime::now),
        )
        .expect("a registry");
        for slug in ["aa", "bb", "cc"] {
            let reg = crate::registry::Registration::from_json(&json!({
                "slug": slug, "name": format!("World {slug}"), "ruleset": "PvE",
                "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
                "emulator": { "family": "ACE" },
            }))
            .expect("a registration");
            r.register(&reg).expect("registered");
            if slug != "cc" {
                r.set_federation(slug, Federation::On).expect("on");
            }
        }
        let op = r.signed_in("200", "Nelly").expect("signed in");
        let profile = dereth_network_protocol::Profile {
            level: 20,
            heritage: 1,
            gender: 1,
            ..dereth_network_protocol::Profile::default()
        };
        for who in ["Ann@aa", "Ann@bb", "Ann@cc"] {
            // Its world has it on the network, and so lists it.
            r.listing(&a(who), Some(&profile)).expect("listed");
            let (code, _) = r.person_code(&a(who)).expect("a code");
            r.redeem_link_code(&op, &code).expect("linked");
        }
        let person = |who: &str| r.tells_person(&a(who)).expect("asked");
        let nelly = Some(TellsPerson {
            person: op.id.clone(),
        });
        assert_eq!(person("Ann@aa"), nelly, "with no setting turned on");
        assert_eq!(person("Ann@bb"), nelly);
        assert_eq!(person("Ann@cc"), None, "a world off the network");
        assert_eq!(person("Nobody@aa"), None);
        r.listing(&a("Ann@bb"), None).expect("off");
        assert_eq!(person("Ann@bb"), None, "off the network: its link paused");
        r.listing(&a("Ann@bb"), Some(&profile)).expect("back on");
        assert_eq!(person("Ann@bb"), nelly, "back on");
        assert_eq!(r.tells_of(&op.id).expect("read"), Some("200".into()));
        r.set_operator_disabled("200", true).expect("disabled");
        assert_eq!(person("Ann@aa"), None, "a disabled person takes none");
        assert_eq!(r.tells_of(&op.id).expect("read"), None);
    }

    /// Spec: hub: Discord's refusal of a direct message to a person holds back the tells to it
    /// for an hour, then they are tried again.
    #[test]
    fn a_refused_person_is_tried_again_after_an_hour() {
        let st = State::new();
        let t0 = Instant::now();
        assert!(!st.refused_lately("p", t0, REFUSED_FOR));
        st.refuse("p", t0, REFUSED_FOR);
        assert!(st.refused_lately("p", t0 + Duration::from_secs(3_599), REFUSED_FOR));
        assert!(
            !st.refused_lately("q", t0, REFUSED_FOR),
            "another person's are its own"
        );
        assert!(!st.refused_lately("p", t0 + REFUSED_FOR, REFUSED_FOR));
    }

    /// Spec: hub: a person's reply comes to the game with Discord's markup made plain and a note of
    /// its attachments, through the line rules.
    #[test]
    fn a_server_channel_line_comes_plain_with_a_note_of_its_attachments() {
        let mentions = json!([
            { "id": "11", "username": "otto", "global_name": "Otto" },
            { "id": "12", "username": "cy", "member": { "nick": "Cyrus" } },
        ]);
        assert_eq!(
            plain_text(
                "hi <@11>, <@!12> and <@13> in <#14> for <@&15> <:wave:16> <a:dance:17> <t:1700000000:R> <3",
                &mentions
            ),
            "hi @Otto, @Cyrus and @someone in #channel for @role :wave: :dance: <t:1700000000:R> <3"
        );
        let line = |content: &str, attachments: usize| json!({ "content": content, "attachments": vec![json!({ "id": "1" }); attachments] });
        assert_eq!(channel_text(&line("look", 0)), "look");
        assert_eq!(channel_text(&line("look", 1)), "[attachment] look");
        assert_eq!(channel_text(&line("", 3)), "[3 attachments]");
        assert_eq!(channel_text(&line("a\nb \u{1F642}", 0)), "a b ?");
        assert_eq!(channel_text(&line("", 0)), "");
    }
}
