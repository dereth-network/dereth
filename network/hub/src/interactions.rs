//! The bot's `/dn` command: a person speaks in a world operator's mirror of a room as one of its
//! own linked characters, through that character's world (`docs/network/05-hub.md` section 5.9).
//!
//! - **In.** Discord sends each use of the command to [`PATH`] ([`handle`]), signed with the
//!   application's key ([`signed`]), which the hub reads from Discord at its start, when it also
//!   registers the command ([`commands`]). No Gateway connection, no privileged intent. Every
//!   answer is the person's alone (ephemeral), and calls nobody.
//! - **`/dn identity`** shows the person its characters on the channel's world and a menu to choose
//!   among them; the choice is kept per person per world ([`Registry::set_identity`]), only ever
//!   one of the person's own characters there, and goes when the character stops being the
//!   person's.
//! - **`/dn say`** asks the world to say the line as the person's character there: its one
//!   character, or the one it chose. A person with several there and none chosen is answered with
//!   the same menu, and the line waits for the choice, which says it and is kept. The line goes
//!   through the routing ([`crate::core::Hub::ask_say`]), within the person's pace
//!   ([`SAYS_PER_PERSON`]); the answer is deferred, and edited once the world has answered
//!   ([`State::answered`]), on a thread of its own.
//!
//! Nothing here logs a line, an interaction's token, or a Discord user beside a character.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use dereth_network_protocol::limits::{Bucket, Rate};
use dereth_network_protocol::say::{reason, VIA_DISCORD};
use dereth_network_protocol::text::sanitize_line;
use dereth_network_protocol::{Address, Federation};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::bridge::{for_channel, Bridged, SHORT_WAIT};
use crate::broker::{BrokerError, Service};
use crate::core::{Said, SayAnswer, SayAsk};
use crate::registry::{iso, Registry, RegistryError};
use crate::server::{log, Shared};

/// The route Discord sends the application's interactions to: the Interactions Endpoint URL the
/// hub's operator sets in Discord's developer portal is the hub's public address and this.
pub const PATH: &str = "/v1/discord/interactions";

/// How far a signed request's timestamp may be from the hub's clock, in seconds; and how long an
/// interaction the hub took is remembered, so that one sent again is refused.
pub const MAX_SKEW_SECS: u64 = 300;

/// Each person's pace of lines said from Discord: a burst of 3, then one every 5 seconds, as a
/// character's room lines.
pub const SAYS_PER_PERSON: Rate = Rate {
    burst: 3.0,
    per_sec: 0.2,
};

/// How long Discord lets the hub edit its answer to an interaction.
pub const TOKEN_LIFE: Duration = Duration::from_secs(15 * 60);

/// The command's name.
pub const COMMAND: &str = "dn";

/// The `custom_id` of the menu `/dn identity` answers with.
pub const IDENTITY_MENU: &str = "dn-identity";

/// The start of the `custom_id` of the menu `/dn say` answers with when the person has several
/// characters on the world and has chosen none; the say's own interaction id follows it, naming
/// the line that waits for the choice.
pub const SAY_MENU: &str = "dn-say:";

/// How long a line waits for the person to choose who says it.
pub const LINE_WAITS: Duration = TOKEN_LIFE;

/// The most lines that may wait for their persons to choose; past it, the oldest goes.
const WAITING_LINES: usize = 1_000;

/// The longest line `/dn say` takes, in characters: the longest the worlds show.
pub const MAX_SAY_CHARS: usize = dereth_network_protocol::text::MAX_LINE_CHARS;

/// Discord's flag for an answer only its asker sees.
pub const EPHEMERAL: u64 = 1 << 6;

/// The most edits that may wait to be made; one past it is not made.
const QUEUE_EDITS: usize = 1_000;

/// The least time between two log lines about one kind of trouble.
const LOG_EVERY: Duration = Duration::from_secs(60);

/// The schema identities add to the registry (its version 8): per person and world, the
/// character it speaks as there, gone with the character's link, the person, or the world's links.
pub(crate) const SCHEMA: &str = "
    CREATE TABLE person_identities (
      person_id TEXT NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
      world TEXT NOT NULL,
      address_key TEXT NOT NULL REFERENCES person_characters(address_key) ON DELETE CASCADE,
      set_at TEXT NOT NULL,
      PRIMARY KEY (person_id, world));
    CREATE INDEX person_identities_by_character ON person_identities(address_key);
";

/// The command the hub registers: `/dn identity` and `/dn say <text>`, in servers alone.
#[must_use]
pub fn commands() -> Value {
    json!([{
        "name": COMMAND,
        "type": 1,
        "description": "Speak in this Dereth Network mirror as one of your own characters",
        "contexts": [0],
        "integration_types": [0],
        "options": [
            {
                "type": 1,
                "name": "say",
                "description": "Say a line in this channel's room as the character you chose",
                "options": [{
                    "type": 3,
                    "name": "text",
                    "description": "What your character says",
                    "required": true,
                    "min_length": 1,
                    "max_length": MAX_SAY_CHARS,
                }],
            },
            {
                "type": 1,
                "name": "identity",
                "description": "Choose which of your characters on this channel's world you speak as",
            },
        ],
    }])
}

/// `hex` as bytes; none unless it is an even number of hex digits.
fn unhex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect()
}

/// The application's public key, from the hex Discord gives it in.
#[must_use]
pub fn public_key(hex: &str) -> Option<[u8; 32]> {
    unhex(hex)?.try_into().ok()
}

/// Whether `token` may be an interaction's token: what Discord gives is URL-safe, and goes in a
/// path.
#[must_use]
pub fn is_interaction_token(token: &str) -> bool {
    (1..=1_024).contains(&token.len())
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Whether `signature` (hex) is the Ed25519 signature, under `key`, of `timestamp` followed by
/// `body`, as Discord signs an interaction.
#[must_use]
pub fn signed(key: &[u8; 32], timestamp: &str, body: &[u8], signature: &str) -> bool {
    let Some(sig) = unhex(signature).filter(|s| s.len() == 64) else {
        return false;
    };
    let mut message = Vec::with_capacity(timestamp.len() + body.len());
    message.extend_from_slice(timestamp.as_bytes());
    message.extend_from_slice(body);
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key)
        .verify(&message, &sig)
        .is_ok()
}

/// Whether `timestamp` (whole seconds since 1970) is within [`MAX_SKEW_SECS`] of `now`.
#[must_use]
pub fn fresh(timestamp: &str, now: SystemTime) -> bool {
    let Ok(at) = timestamp.trim().parse::<u64>() else {
        return false;
    };
    let now = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    now.abs_diff(at) <= MAX_SKEW_SECS
}

// ---- the registry's identities -------------------------------------------------------------------

impl Registry {
    /// The person whose Discord account is `discord_id`, while it is not disabled.
    ///
    /// # Errors
    /// Database errors only.
    pub fn person_by_discord(&self, discord_id: &str) -> Result<Option<String>, RegistryError> {
        Ok(self
            .db
            .query_row(
                "SELECT id FROM operators WHERE discord_id = ? AND disabled_at IS NULL",
                [discord_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// The person's characters on the world `slug` that it may speak as, in the order they were
    /// linked: those on the network, since a link pauses while its character is off.
    ///
    /// # Errors
    /// Database errors only.
    pub fn characters_on(&self, person: &str, slug: &str) -> Result<Vec<Address>, RegistryError> {
        let mut q = self.db.prepare_cached(&format!(
            "SELECT address FROM person_characters WHERE person_id = ?1 AND world = ?2 AND {}
             ORDER BY linked_at, rowid",
            crate::directory::on_network_sql("address_key", "?3")
        ))?;
        let rows = q
            .query_map(params![person, slug, iso(self.now())], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows.iter().filter_map(|a| Address::parse(a).ok()).collect())
    }

    /// Whether the person links any character on the world `slug`, on the network or off it.
    ///
    /// # Errors
    /// Database errors only.
    pub fn links_on(&self, person: &str, slug: &str) -> Result<bool, RegistryError> {
        Ok(self.db.query_row(
            "SELECT count(*) > 0 FROM person_characters WHERE person_id = ? AND world = ?",
            params![person, slug],
            |r| r.get(0),
        )?)
    }

    /// The character the person speaks as on the world `slug`, while it is still the person's
    /// there and on the network. A choice of a character off the network is kept, and holds again
    /// once the character is back on.
    ///
    /// # Errors
    /// Database errors only.
    pub fn identity(&self, person: &str, slug: &str) -> Result<Option<Address>, RegistryError> {
        let found: Option<String> = self
            .db
            .query_row(
                &format!(
                    "SELECT c.address FROM person_identities i
                       JOIN person_characters c ON c.address_key = i.address_key
                         AND c.person_id = i.person_id AND c.world = i.world
                     WHERE i.person_id = ?1 AND i.world = ?2 AND {}",
                    crate::directory::on_network_sql("c.address_key", "?3")
                ),
                params![person, slug, iso(self.now())],
                |r| r.get(0),
            )
            .optional()?;
        Ok(found.and_then(|a| Address::parse(&a).ok()))
    }

    /// The person speaks as `who` on the world `slug` from now on, or as nobody: false, and nothing
    /// kept, when `who` is not one of the person's characters there.
    ///
    /// # Errors
    /// Database errors only.
    pub fn set_identity(
        &self,
        person: &str,
        slug: &str,
        who: Option<&Address>,
    ) -> Result<bool, RegistryError> {
        let Some(who) = who else {
            self.db.execute(
                "DELETE FROM person_identities WHERE person_id = ? AND world = ?",
                params![person, slug],
            )?;
            return Ok(true);
        };
        if who.world() != slug
            || !self
                .characters_on(person, slug)?
                .iter()
                .any(|c| c.same(who))
        {
            return Ok(false);
        }
        self.db.execute(
            "INSERT INTO person_identities(person_id, world, address_key, set_at) VALUES (?, ?, ?, ?)
             ON CONFLICT(person_id, world) DO UPDATE SET address_key = excluded.address_key,
               set_at = excluded.set_at",
            params![person, slug, who.key(), iso(self.now())],
        )?;
        Ok(true)
    }

    /// The person no longer speaks as `who` on its world: its world says it has no such character,
    /// or that it is not the one that asked for its link.
    ///
    /// # Errors
    /// Database errors only.
    pub fn drop_identity(&self, person: &str, who: &Address) -> Result<(), RegistryError> {
        self.db.execute(
            "DELETE FROM person_identities WHERE person_id = ? AND world = ? AND address_key = ?",
            params![person, who.world(), who.key()],
        )?;
        Ok(())
    }

    /// The character `key` is the person `person`'s now: no other person speaks as it.
    ///
    /// # Errors
    /// Database errors only.
    pub(crate) fn identities_moved(&self, key: &str, person: &str) -> Result<(), RegistryError> {
        self.db.execute(
            "DELETE FROM person_identities WHERE address_key = ? AND person_id != ?",
            params![key, person],
        )?;
        Ok(())
    }
}

// ---- what the hub keeps while it runs ------------------------------------------------------------

/// A deferred answer, waiting for its world: the application and the interaction's token it is
/// edited with, the person, the world, by name as the answer shows it, and what the answer says
/// before how the line ended (the choice a menu made).
#[derive(Clone)]
struct Pending {
    app: String,
    token: String,
    person: String,
    world: String,
    lead: Option<String>,
    at: Instant,
}

/// A line said with `/dn say` by a person with several characters on the world and none chosen,
/// waiting for the choice: in memory alone, until it is chosen or [`LINE_WAITS`] passes.
struct WaitingLine {
    person: String,
    slug: String,
    text: String,
    at: Instant,
}

impl std::fmt::Debug for Pending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pending")
            .field("world", &self.world)
            .finish_non_exhaustive()
    }
}

/// An answer to edit.
struct Edit {
    app: String,
    token: String,
    content: String,
}

/// What the command keeps while the hub runs.
pub(crate) struct State {
    /// The application's public key, once Discord has given it.
    key: Mutex<Option<[u8; 32]>>,
    /// The deferred answers, by the interaction's id.
    pending: Mutex<HashMap<String, Pending>>,
    /// The lines waiting for their persons to choose who says them, by the say's interaction id.
    lines: Mutex<HashMap<String, WaitingLine>>,
    /// The interactions taken lately, by id, so that none is taken twice.
    seen: Mutex<HashMap<String, Instant>>,
    /// Each person's pace of lines ([`SAYS_PER_PERSON`]).
    paces: Mutex<HashMap<String, Bucket>>,
    edits: SyncSender<Edit>,
    waiting: Mutex<Option<Receiver<Edit>>>,
    logged: Mutex<HashMap<&'static str, Instant>>,
    started: Instant,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("pending", &lock(&self.pending).len())
            .finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl State {
    pub(crate) fn new() -> Self {
        let (edits, waiting) = mpsc::sync_channel(QUEUE_EDITS);
        Self {
            key: Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
            lines: Mutex::new(HashMap::new()),
            seen: Mutex::new(HashMap::new()),
            paces: Mutex::new(HashMap::new()),
            edits,
            waiting: Mutex::new(Some(waiting)),
            logged: Mutex::new(HashMap::new()),
            started: Instant::now(),
        }
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

    /// Takes one of the person's pace: false when it is spent.
    fn pace_takes(&self, person: &str) -> bool {
        let now = self.started.elapsed().as_secs_f64();
        let mut paces = lock(&self.paces);
        if paces.len() >= 10_000 {
            paces.retain(|_, b| !b.full_at(now));
        }
        paces
            .entry(person.to_owned())
            .or_insert_with(|| Bucket::new(SAYS_PER_PERSON, now))
            .take(now)
    }

    /// Keeps `line` until its person chooses who says it, under `key`; the oldest goes when
    /// [`WAITING_LINES`] wait already.
    fn wait(&self, key: &str, line: WaitingLine) {
        let mut lines = lock(&self.lines);
        lines.retain(|_, l| l.at.elapsed() < LINE_WAITS);
        if lines.len() >= WAITING_LINES {
            if let Some(oldest) = lines
                .iter()
                .min_by_key(|(_, l)| l.at)
                .map(|(k, _)| k.clone())
            {
                lines.remove(&oldest);
            }
        }
        lines.insert(key.to_owned(), line);
    }

    /// The line waiting under `key` for `person` on the world `slug`, taken; none once it has
    /// waited [`LINE_WAITS`], or for another person or world.
    fn take_line(&self, key: &str, person: &str, slug: &str) -> Option<String> {
        let mut lines = lock(&self.lines);
        let ours = lines
            .get(key)
            .is_some_and(|l| l.person == person && l.slug == slug);
        let line = if ours { lines.remove(key) } else { None };
        line.filter(|l| l.at.elapsed() < LINE_WAITS).map(|l| l.text)
    }

    /// Takes the interaction `id`: false when it was taken within [`MAX_SKEW_SECS`].
    fn first_time(&self, id: &str) -> bool {
        let now = Instant::now();
        let window = Duration::from_secs(MAX_SKEW_SECS);
        let mut seen = lock(&self.seen);
        seen.retain(|_, at| now.duration_since(*at) < window);
        if seen.contains_key(id) {
            return false;
        }
        seen.insert(id.to_owned(), now);
        true
    }

    /// The answers to lines said from Discord: each person's deferred answer is edited to say how
    /// it ended. A world that says it has no such character, or that the character never asked for
    /// its link, ends the person's choice of it.
    pub(crate) fn answered(&self, shared: &Shared, answers: Vec<SayAnswer>) {
        for a in answers {
            let Some(p) = lock(&self.pending).remove(&a.asker) else {
                continue;
            };
            if let Said::Refused(Some(r)) = &a.said {
                if r == reason::NO_CHARACTER || r == reason::NOT_LINKED {
                    if let Err(e) = shared.registry().drop_identity(&p.person, &a.who) {
                        log("error", &format!("a person's choice of character: {e}"));
                    }
                }
            }
            let said = answer_text(&a.who, &p.world, &a.said);
            let content = match p.lead {
                Some(lead) => format!("{lead} {said}"),
                None => said,
            };
            if let Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) =
                self.edits.try_send(Edit {
                    app: p.app,
                    token: p.token,
                    content,
                })
            {
                self.log_now_and_then(
                    "edits",
                    "an answer in Discord was not edited: too many wait already",
                );
            }
        }
    }
}

// ---- what the person is told ---------------------------------------------------------------------

/// A name as an answer shows it: Discord's markdown escaped, as a mirror shows a line.
fn shown(s: &str) -> String {
    for_channel(s)
}

/// The answer in a channel that mirrors no room.
pub const NO_MIRROR: &str = "This channel mirrors no room of the Dereth Network.";

/// The answer when there is nothing left of a line to say.
pub const NOTHING: &str = "There is nothing to say.";

/// The answer past the person's pace.
pub const TOO_QUICKLY: &str = "You are speaking too quickly. Wait a moment and try again.";

/// The answer when the hub cannot answer now.
const TROUBLE: &str = "The Dereth Network's hub could not answer just now. Try again later.";

/// The answer in one of the admins' mirrors, which are read only.
#[must_use]
pub fn read_only(room: &Bridged) -> String {
    match room {
        Bridged::Channel(_) => format!(
            "This channel mirrors the channel {} for everyone to read. Speak in it in game.",
            shown(&room.route_name())
        ),
        _ => "This channel mirrors the network's Global room for everyone to read. Speak in it from your own world's Discord, or in game.".to_owned(),
    }
}

/// The answer to a person who links no character on the world, or is no person.
#[must_use]
pub fn not_linked(world: &str) -> String {
    format!(
        "You have no character linked on {}. To link one, type @dn link in game and open the link it gives you.",
        shown(world)
    )
}

/// The answer to a person whose characters linked on the world are all off the network: their
/// links are paused, and `@dn on` in game resumes one.
#[must_use]
pub fn all_off(world: &str) -> String {
    format!(
        "Your characters linked on {} are off the Dereth Network. Type @dn on in game to speak as one.",
        shown(world)
    )
}

/// What `/dn identity` says, above its menu.
#[must_use]
pub fn identity_text(world: &str, now: Option<&Address>) -> String {
    let choose = format!(
        "Choose which of your characters on {} you speak as.",
        shown(world)
    );
    match now {
        Some(who) => format!("{choose} You speak as {} now.", shown(&who.to_string())),
        None => choose,
    }
}

/// What `/dn say` says above its menu, when the person has several characters on the world and
/// has chosen none.
#[must_use]
pub fn say_menu_text(world: &str) -> String {
    format!(
        "{} Your line is said as the one you choose.",
        identity_text(world, None)
    )
}

/// What a choice in the menu says.
#[must_use]
pub fn chosen_text(world: &str, who: &Address) -> String {
    format!(
        "You speak as {} on {} now.",
        shown(&who.to_string()),
        shown(world)
    )
}

/// What a choice in `/dn say`'s menu says when its line waited too long.
const LINE_GONE: &str = "Your line waited too long and was not said. Say it again with /dn say.";

/// The words a world's refusal is told in, after the character's name.
#[must_use]
pub fn reason_words(world: &str, why: Option<&str>) -> String {
    let world = shown(world);
    match why {
        Some(reason::NO_CHARACTER) => {
            format!("{world} has no such character now. Choose another with /dn identity.")
        }
        Some(reason::NOT_ON_NETWORK) => {
            "it is not on the Dereth Network. Type @dn on in game.".to_owned()
        }
        Some(reason::NOT_LINKED) => format!(
            "{world} says it is not the character you linked. Link it again with @dn link in game."
        ),
        Some(reason::BANNED) => format!("its account is banned on {world}."),
        Some(reason::GAGGED) => format!("it is gagged on {world}."),
        Some(reason::TOO_NEW) => "it is too new to speak on the Dereth Network yet.".to_owned(),
        Some(reason::RATE_LIMITED) => {
            "it is speaking too quickly. Wait a moment and try again.".to_owned()
        }
        Some(reason::ROOM_OFF) => format!("{world} does not carry this room now."),
        Some(reason::NOT_ALLOWED) => format!("it may not speak in this room on {world}."),
        _ => format!("{world} refused it."),
    }
}

/// What a person's deferred answer is edited to say once the line has ended.
#[must_use]
pub fn answer_text(who: &Address, world: &str, said: &Said) -> String {
    let name = shown(&who.to_string());
    match said {
        Said::Said => format!("Said as {name}."),
        Said::Refused(why) => format!(
            "{name} did not say it: {}",
            reason_words(world, why.as_deref())
        ),
        Said::Unavailable => format!("{} can't be reached right now.", shown(world)),
        Said::NotCarried => format!("{} does not carry this room now.", shown(world)),
        Said::NoAnswer => format!("{} did not answer in time.", shown(world)),
    }
}

/// An answer only its asker sees, calling nobody.
fn message(content: &str) -> Value {
    json!({ "type": 4, "data": {
        "content": content, "flags": EPHEMERAL, "allowed_mentions": { "parse": [] },
    } })
}

/// The answer to a line for a world: deferred, only its asker sees it.
fn deferred() -> Value {
    json!({ "type": 5, "data": { "flags": EPHEMERAL } })
}

/// The menu's message, changed to say what was chosen, the menu gone.
fn updated(content: &str) -> Value {
    json!({ "type": 7, "data": {
        "content": content, "components": [], "allowed_mentions": { "parse": [] },
    } })
}

/// The menu of the person's characters on the world, the one it speaks as now chosen, under
/// `content`: `/dn identity`'s answer (`custom_id` [`IDENTITY_MENU`]), and `/dn say`'s when the
/// person has several and none chosen ([`SAY_MENU`] and the say's id).
fn identity_menu(
    content: &str,
    custom_id: &str,
    characters: &[Address],
    now: Option<&Address>,
) -> Value {
    let options: Vec<Value> = characters
        .iter()
        .map(|c| {
            json!({
                "label": c.to_string(), "value": c.to_string(),
                "default": now.is_some_and(|n| n.same(c)),
            })
        })
        .collect();
    json!({ "type": 4, "data": {
        "content": content,
        "flags": EPHEMERAL,
        "allowed_mentions": { "parse": [] },
        "components": [{ "type": 1, "components": [{
            "type": 3, "custom_id": custom_id, "options": options,
            "placeholder": "Speak as...",
        }] }],
    } })
}

// ---- the route -----------------------------------------------------------------------------------

/// An interaction, as far as the hub reads it.
struct Interaction<'a> {
    id: &'a str,
    app: &'a str,
    token: &'a str,
    guild: Option<&'a str>,
    channel: Option<&'a str>,
    user: Option<&'a str>,
    data: &'a Value,
}

impl<'a> Interaction<'a> {
    fn of(v: &'a Value) -> Option<Self> {
        let snowflake = |v: &'a Value| v.as_str().filter(|s| crate::broker::is_snowflake(s));
        Some(Self {
            id: snowflake(&v["id"])?,
            app: snowflake(&v["application_id"])?,
            token: v["token"].as_str().filter(|t| is_interaction_token(t))?,
            guild: snowflake(&v["guild_id"]),
            channel: snowflake(&v["channel_id"]).or_else(|| snowflake(&v["channel"]["id"])),
            user: snowflake(&v["member"]["user"]["id"]).or_else(|| snowflake(&v["user"]["id"])),
            data: &v["data"],
        })
    }
}

/// Where a use of the command was made: the mirror's world, by slug and name, and its room; and the
/// person, with its characters there.
struct Place {
    slug: String,
    world: String,
    federation: Federation,
    room: String,
    person: String,
    characters: Vec<Address>,
}

/// The mirror in the interaction's channel, a world operator's, and the person who used it, linking
/// characters on its world; otherwise the answer to give.
fn place(shared: &Shared, i: &Interaction<'_>) -> Result<Place, String> {
    let (Some(channel), Some(user)) = (i.channel, i.user) else {
        return Err(NO_MIRROR.to_owned());
    };
    let trouble = |e: RegistryError| {
        log("error", &format!("the /dn command: {e}"));
        TROUBLE.to_owned()
    };
    let reg = shared.registry();
    let mirror = reg.mirror_in_channel(channel).map_err(trouble)?;
    let Some(mirror) = mirror.filter(|m| i.guild == Some(m.placement.server.as_str())) else {
        return Err(NO_MIRROR.to_owned());
    };
    let Some((_, slug)) = mirror.world.clone() else {
        return Err(mirror
            .mirrored()
            .map_or_else(|| NO_MIRROR.to_owned(), |b| read_only(&b)));
    };
    let (world, federation) = reg
        .federation(&slug)
        .map_err(trouble)?
        .unwrap_or_else(|| (slug.clone(), Federation::Off));
    let Some(person) = reg.person_by_discord(user).map_err(trouble)? else {
        return Err(not_linked(&world));
    };
    let characters = reg.characters_on(&person, &slug).map_err(trouble)?;
    if characters.is_empty() {
        return Err(if reg.links_on(&person, &slug).map_err(trouble)? {
            all_off(&world)
        } else {
            not_linked(&world)
        });
    }
    Ok(Place {
        slug,
        world,
        federation,
        room: mirror.room,
        person,
        characters,
    })
}

/// `/dn identity`: the menu of the person's characters on the channel's world.
fn identity(shared: &Shared, i: &Interaction<'_>) -> Value {
    let p = match place(shared, i) {
        Ok(p) => p,
        Err(answer) => return message(&answer),
    };
    match shared.registry().identity(&p.person, &p.slug) {
        Ok(now) => identity_menu(
            &identity_text(&p.world, now.as_ref()),
            IDENTITY_MENU,
            &p.characters,
            now.as_ref(),
        ),
        Err(e) => {
            log("error", &format!("the /dn command: {e}"));
            message(TROUBLE)
        }
    }
}

/// A choice in `/dn identity`'s menu, or in `/dn say`'s: kept, when it is one of the person's
/// characters on the channel's world. A choice in `/dn say`'s says the line that waits for it.
fn choose(shared: &Shared, service: &Service, i: &Interaction<'_>) -> Value {
    let p = match place(shared, i) {
        Ok(p) => p,
        Err(answer) => return updated(&answer),
    };
    let not_yours = || {
        updated(&format!(
            "That is not one of your characters on {}.",
            shown(&p.world)
        ))
    };
    let Ok(who) = Address::parse(i.data["values"][0].as_str().unwrap_or("")) else {
        return not_yours();
    };
    match shared
        .registry()
        .set_identity(&p.person, &p.slug, Some(&who))
    {
        Ok(true) => {}
        Ok(false) => return not_yours(),
        Err(e) => {
            log("error", &format!("the /dn command: {e}"));
            return updated(TROUBLE);
        }
    }
    let chosen = chosen_text(&p.world, &who);
    let Some(key) = i.data["custom_id"]
        .as_str()
        .and_then(|c| c.strip_prefix(SAY_MENU))
    else {
        return updated(&chosen);
    };
    let Some(text) = service.speak.take_line(key, &p.person, &p.slug) else {
        return updated(&format!("{chosen} {LINE_GONE}"));
    };
    match say_as(shared, service, i, p, who, text, Some(chosen.clone())) {
        Ok(()) => updated(&format!("{chosen} Saying your line...")),
        Err(refused) => updated(&format!("{chosen} {refused}")),
    }
}

/// `/dn say <text>`: the line goes to the world of the channel's mirror, said as the person's one
/// character there or the one it chose, and the answer waits for it. A person with several there
/// and none chosen is answered with the menu, and the line waits for the choice.
fn say(shared: &Shared, service: &Service, i: &Interaction<'_>, text: &str) -> Value {
    let p = match place(shared, i) {
        Ok(p) => p,
        Err(answer) => return message(&answer),
    };
    let text = sanitize_line(&crate::dms::plain_text(text, &Value::Null));
    if text.is_empty() {
        return message(NOTHING);
    }
    let who = match shared.registry().identity(&p.person, &p.slug) {
        Ok(Some(who)) => who,
        Ok(None) if p.characters.len() == 1 => p.characters[0].clone(),
        Ok(None) => {
            service.speak.wait(
                i.id,
                WaitingLine {
                    person: p.person.clone(),
                    slug: p.slug.clone(),
                    text,
                    at: Instant::now(),
                },
            );
            return identity_menu(
                &say_menu_text(&p.world),
                &format!("{SAY_MENU}{}", i.id),
                &p.characters,
                None,
            );
        }
        Err(e) => {
            log("error", &format!("the /dn command: {e}"));
            return message(TROUBLE);
        }
    };
    match say_as(shared, service, i, p, who, text, None) {
        Ok(()) => deferred(),
        Err(refused) => message(&refused),
    }
}

/// Asks the world of `p` to say `text` as `who`, answering the interaction `i` once it has
/// answered (`lead` before how it ended); the answer to give at once when the line is not asked:
/// over the person's pace, or a world off the network.
fn say_as(
    shared: &Shared,
    service: &Service,
    i: &Interaction<'_>,
    p: Place,
    who: Address,
    text: String,
    lead: Option<String>,
) -> Result<(), String> {
    if !service.speak.pace_takes(&p.person) {
        return Err(TOO_QUICKLY.to_owned());
    }
    if p.federation != Federation::On {
        return Err(answer_text(&who, &p.world, &Said::Unavailable));
    }
    lock(&service.speak.pending).insert(
        i.id.to_owned(),
        Pending {
            app: i.app.to_owned(),
            token: i.token.to_owned(),
            person: p.person,
            world: p.world,
            lead,
            at: Instant::now(),
        },
    );
    shared.say(SayAsk {
        asker: i.id.to_owned(),
        room: p.room,
        who,
        text,
        via: VIA_DISCORD.to_owned(),
    });
    Ok(())
}

/// A JSON answer to Discord.
fn reply(status: u16, v: &Value) -> crate::api::Reply {
    crate::api::Reply {
        status,
        body: v.to_string(),
        kind: "application/json",
        headers: Vec::new(),
        cors: crate::api::Cors::Nothing,
    }
}

/// An error answer to Discord.
fn refused(status: u16, message: &str) -> crate::api::Reply {
    reply(
        status,
        &json!({ "error": { "code": crate::api::code(status), "message": message } }),
    )
}

/// The value of the header `name`.
fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.trim())
}

/// `POST /v1/discord/interactions`: one of Discord's interactions, taken only when Discord signed
/// it, once. `PING` is answered `PONG`; `/dn identity` with its menu, and a choice in a menu, at
/// once; `/dn say` by deferring, or with the menu when the person has several characters on the
/// world and has chosen none. Every interaction comes from Discord's own addresses, so one
/// Discord signed takes nothing from its client address's allowance; a request that is not one
/// takes from `ip`'s, and is answered `429` once that is spent.
pub(crate) fn handle(
    shared: &Shared,
    method: &str,
    headers: &[(String, String)],
    body: &[u8],
    ip: IpAddr,
) -> crate::api::Reply {
    // A request Discord did not sign is held to its address's allowance, as any other request is.
    let unsigned = |reply: crate::api::Reply| match shared.allowed(ip) {
        Ok(()) => reply,
        Err(wait) => crate::server::too_many(wait, crate::api::Cors::Nothing),
    };
    let Some(service) = shared.broker.as_ref().filter(|_| shared.sign_in_on()) else {
        return unsigned(refused(
            503,
            "This hub takes no Discord interactions: it has no bot token, or no sign-in",
        ));
    };
    if method != "POST" {
        return unsigned(refused(405, "POST an interaction here"));
    }
    let state = &service.speak;
    let Some(key) = *lock(&state.key) else {
        state.log_now_and_then(
            "unsigned",
            "refused a Discord interaction: the hub has not read the application's public key from Discord yet",
        );
        return unsigned(refused(401, "The hub cannot check Discord's signature yet"));
    };
    let (Some(signature), Some(timestamp)) = (
        header(headers, "x-signature-ed25519"),
        header(headers, "x-signature-timestamp"),
    ) else {
        state.log_now_and_then(
            "signature",
            "refused a request at the interactions route: it is not signed (logged once a minute)",
        );
        return unsigned(refused(401, "Not signed by Discord"));
    };
    if !signed(&key, timestamp, body, signature) {
        state.log_now_and_then(
            "signature",
            "refused a request at the interactions route: its signature is not Discord's (logged once a minute)",
        );
        return unsigned(refused(401, "Not signed by Discord"));
    }
    if !fresh(timestamp, SystemTime::now()) {
        state.log_now_and_then(
            "stale",
            "refused a Discord interaction: its timestamp is over five minutes from the hub's clock (logged once a minute)",
        );
        return unsigned(refused(
            401,
            "The signature's time is too far from the hub's",
        ));
    }
    let Ok(v) = serde_json::from_slice::<Value>(body) else {
        return refused(400, "Not an interaction");
    };
    if v["type"] == 1 {
        return reply(200, &json!({ "type": 1 }));
    }
    let Some(i) = Interaction::of(&v) else {
        return refused(400, "Not an interaction");
    };
    if !state.first_time(i.id) {
        return refused(401, "This interaction was taken already");
    }
    let answer = match (v["type"].as_u64(), i.data["name"].as_str()) {
        (Some(2), Some(COMMAND)) => {
            let sub = &i.data["options"][0];
            match sub["name"].as_str() {
                Some("identity") => identity(shared, &i),
                Some("say") => {
                    let text = sub["options"]
                        .as_array()
                        .and_then(|o| o.iter().find(|o| o["name"] == "text"))
                        .and_then(|o| o["value"].as_str())
                        .unwrap_or("");
                    say(shared, service, &i, text)
                }
                _ => return refused(400, "Not a command of the hub's"),
            }
        }
        (Some(3), _)
            if i.data["custom_id"] == IDENTITY_MENU
                || i.data["custom_id"]
                    .as_str()
                    .is_some_and(|c| c.starts_with(SAY_MENU)) =>
        {
            choose(shared, service, &i)
        }
        _ => return refused(400, "Not a command of the hub's"),
    };
    reply(200, &answer)
}

// ---- the thread ----------------------------------------------------------------------------------

/// Starts the thread that reads the application's public key and registers the command, then edits
/// the deferred answers as their worlds answer.
///
/// # Errors
/// When the thread cannot start.
pub(crate) fn start(shared: &Arc<Shared>) -> std::io::Result<Vec<JoinHandle<()>>> {
    if shared.broker.is_none() || !shared.sign_in_on() {
        return Ok(Vec::new());
    }
    let s = Arc::clone(shared);
    Ok(vec![std::thread::Builder::new()
        .name("hub-interactions".into())
        .spawn(move || run(&s))?])
}

/// What the thread has done of its start: read the key, registered the command.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Ready {
    #[default]
    Nothing,
    Key,
    Registered,
}

fn run(shared: &Arc<Shared>) {
    let Some(service) = shared.broker.as_ref() else {
        return;
    };
    let Some(edits) = lock(&service.speak.waiting).take() else {
        return;
    };
    let settings = service.rest.broker().clone();
    let mut ready = Ready::Nothing;
    let mut next = Instant::now();
    let mut refused = 0u32;
    while !shared.stopping() {
        if ready != Ready::Registered && Instant::now() >= next {
            match set_up(shared, service, &mut ready) {
                Ok(()) => refused = 0,
                Err(BrokerError::Wait(secs)) => {
                    next = Instant::now() + settings.retry.max(Duration::from_secs(secs));
                }
                Err(e) => {
                    let wait = settings
                        .retry
                        .saturating_mul(1 << refused.min(6))
                        .min(settings.recheck.max(settings.retry));
                    refused += 1;
                    log(
                        "warn",
                        &format!(
                            "the /dn command: {e}; trying again in {} seconds",
                            wait.as_secs()
                        ),
                    );
                    next = Instant::now() + wait;
                }
            }
        }
        match edits.recv_timeout(Duration::from_millis(100)) {
            Ok(edit) => edit_answer(shared, service, &edit),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        lock(&service.speak.pending).retain(|_, p| p.at.elapsed() < TOKEN_LIFE);
        lock(&service.speak.lines).retain(|_, l| l.at.elapsed() < LINE_WAITS);
    }
}

/// Reads the application's public key, then registers the command, each once.
fn set_up(shared: &Shared, service: &Service, ready: &mut Ready) -> Result<(), BrokerError> {
    let app = service.rest.application()?;
    if *ready == Ready::Nothing {
        let key = app
            .verify_key
            .as_deref()
            .and_then(public_key)
            .ok_or_else(|| BrokerError::Failed("Discord gave no public key".into()))?;
        *lock(&service.speak.key) = Some(key);
        *ready = Ready::Key;
        log(
            "info",
            "the application's public key is read: Discord's interactions are taken",
        );
    }
    service.rest.register_commands(&app.id, &commands())?;
    *ready = Ready::Registered;
    log(
        "info",
        &format!(
            "the /dn command is registered; its interactions come to {}{PATH}",
            shared.config.public_url.as_deref().unwrap_or("")
        ),
    );
    Ok(())
}

/// Edits one deferred answer, waiting out a short wait Discord names; a longer one, or a refusal,
/// leaves it, logged at most once a minute. An answer Discord does not know yet is tried again a
/// second later: a world can answer before Discord has taken the deferral the edit changes.
fn edit_answer(shared: &Shared, service: &Service, edit: &Edit) {
    let route = format!("PATCH /webhooks/{}/messages/@original", edit.app);
    let mut unknown = 0;
    for _ in 0..4 {
        if let Some(left) = service.rest.waiting(&route) {
            if left > SHORT_WAIT {
                break;
            }
            let until = Instant::now() + left;
            while !shared.stopping() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        match service
            .rest
            .edit_original(&edit.app, &edit.token, &edit.content)
        {
            Ok(()) => return,
            Err(BrokerError::Wait(secs)) if Duration::from_secs(secs) <= SHORT_WAIT => {}
            Err(BrokerError::NotFound) if unknown == 0 => {
                unknown += 1;
                let until = Instant::now() + Duration::from_secs(1);
                while !shared.stopping() && Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            Err(e) => {
                service
                    .speak
                    .log_now_and_then("edit", &format!("an answer in Discord was not edited: {e}"));
                return;
            }
        }
    }
    service.speak.log_now_and_then(
        "edit",
        "an answer in Discord was not edited: Discord asks the hub to wait",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &str) -> Address {
        Address::parse(s).expect("an address")
    }

    /// RFC 8032's first test: its secret key's seed, its public key and its signature of nothing.
    const SEED: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
    const PUBLIC: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
    const SIGNATURE: &str = "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b";

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// Spec: hub: an interaction is taken only when its signature is Ed25519's, under the
    /// application's public key, of its timestamp followed by its body; any other byte changed,
    /// another key, or a signature that does not read is refused.
    #[test]
    fn a_signature_is_discords_only_over_the_timestamp_and_the_body_under_the_key() {
        let key = public_key(PUBLIC).expect("a key");
        assert!(signed(&key, "", b"", SIGNATURE), "RFC 8032's first test");
        let pair =
            ring::signature::Ed25519KeyPair::from_seed_unchecked(&unhex(SEED).expect("the seed"))
                .expect("a key pair");
        let body = br#"{"type":1}"#;
        let sig = hex(pair
            .sign(&[b"1790000000".as_slice(), body].concat())
            .as_ref());
        assert!(signed(&key, "1790000000", body, &sig));
        assert!(!signed(&key, "1790000001", body, &sig), "another time");
        assert!(
            !signed(&key, "1790000000", br#"{"type":2}"#, &sig),
            "another body"
        );
        let other = public_key(&"11".repeat(32)).expect("a key");
        assert!(!signed(&other, "1790000000", body, &sig), "another key");
        assert!(!signed(&key, "1790000000", body, &sig[..126]), "cut short");
        assert!(!signed(&key, "1790000000", body, "zz"));
        assert!(!signed(&key, "1790000000", body, ""));
        assert_eq!(public_key("abc"), None);
        assert_eq!(public_key(&"0".repeat(62)), None);
    }

    /// Spec: hub: a signed interaction is taken only within five minutes of the hub's clock.
    #[test]
    fn a_signature_is_taken_within_five_minutes_of_the_hubs_clock() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_790_000_000);
        assert!(fresh("1790000000", now));
        assert!(fresh("1789999700", now));
        assert!(fresh("1790000300", now));
        assert!(!fresh("1789999699", now));
        assert!(!fresh("1790000301", now));
        assert!(!fresh("soon", now));
        assert!(!fresh("", now));
    }

    /// Spec: hub: `/dn` is offered in servers alone, with `identity` and `say`, whose text is at
    /// most the longest line the worlds show.
    #[test]
    fn the_command_is_dn_with_identity_and_say_in_servers_alone() {
        let c = commands();
        let dn = &c[0];
        assert_eq!(c.as_array().map(Vec::len), Some(1));
        assert_eq!(dn["name"], "dn");
        assert_eq!(dn["contexts"], json!([0]));
        let subs: Vec<&str> = dn["options"]
            .as_array()
            .expect("subcommands")
            .iter()
            .filter_map(|o| o["name"].as_str())
            .collect();
        assert_eq!(subs, ["say", "identity"]);
        assert_eq!(dn["options"][0]["options"][0]["max_length"], 255);
        for d in [
            &dn["description"],
            &dn["options"][0]["description"],
            &dn["options"][1]["description"],
        ] {
            assert!(
                d.as_str().is_some_and(|d| (1..=100).contains(&d.len())),
                "{d}"
            );
        }
    }

    /// Spec: hub: every answer the person is given is its alone (ephemeral) and calls nobody, and
    /// the menu offers the person's characters on the world, the current one chosen; `/dn say`'s
    /// is the same menu, naming the line that waits for the choice.
    #[test]
    fn every_answer_is_the_persons_alone() {
        for v in [message("x"), deferred()] {
            assert_eq!(v["data"]["flags"], EPHEMERAL, "{v}");
        }
        assert_eq!(
            message("x")["data"]["allowed_mentions"],
            json!({ "parse": [] })
        );
        let characters = [a("Ann@coldeve"), a("Bo@coldeve")];
        let menu = identity_menu(
            &identity_text("Coldeve", Some(&characters[1])),
            IDENTITY_MENU,
            &characters,
            Some(&characters[1]),
        );
        assert_eq!(menu["data"]["flags"], EPHEMERAL);
        let options = |m: &Value| -> Vec<(String, bool)> {
            m["data"]["components"][0]["components"][0]["options"]
                .as_array()
                .expect("options")
                .iter()
                .map(|o| {
                    (
                        o["value"].as_str().unwrap_or("").to_owned(),
                        o["default"] == true,
                    )
                })
                .collect()
        };
        assert_eq!(
            options(&menu),
            [
                ("Ann@coldeve".to_owned(), false),
                ("Bo@coldeve".to_owned(), true)
            ]
        );
        assert_eq!(
            menu["data"]["content"],
            "Choose which of your characters on Coldeve you speak as. You speak as Bo@coldeve now."
        );
        let say = identity_menu(
            &say_menu_text("Coldeve"),
            &format!("{SAY_MENU}1558376951000000999"),
            &characters,
            None,
        );
        assert_eq!(say["data"]["flags"], EPHEMERAL);
        assert_eq!(
            options(&say),
            [
                ("Ann@coldeve".to_owned(), false),
                ("Bo@coldeve".to_owned(), false)
            ]
        );
        assert_eq!(
            say["data"]["content"],
            "Choose which of your characters on Coldeve you speak as. Your line is said as the one you choose."
        );
        assert_eq!(
            say["data"]["components"][0]["components"][0]["custom_id"],
            "dn-say:1558376951000000999"
        );
        assert_eq!(updated("x")["type"], 7);
        assert_eq!(updated("x")["data"]["components"], json!([]));
    }

    /// Spec: hub: a line waiting for its person's choice is taken once, by that person on that
    /// world alone, and not once it has waited too long.
    #[test]
    fn a_waiting_line_is_said_once_for_its_own_person() {
        let st = State::new();
        let line = |person: &str| WaitingLine {
            person: person.into(),
            slug: "aa".into(),
            text: "evening all".into(),
            at: Instant::now(),
        };
        st.wait("1", line("p"));
        assert_eq!(st.take_line("1", "q", "aa"), None, "another person's");
        assert_eq!(st.take_line("1", "p", "bb"), None, "another world's");
        assert_eq!(st.take_line("1", "p", "aa"), Some("evening all".into()));
        assert_eq!(st.take_line("1", "p", "aa"), None, "taken once");
        st.wait(
            "2",
            WaitingLine {
                at: Instant::now().checked_sub(LINE_WAITS).expect("a while ago"),
                ..line("p")
            },
        );
        assert_eq!(st.take_line("2", "p", "aa"), None, "waited too long");
    }

    /// Spec: hub: the person is told how its line ended in the words of the hub's specification,
    /// a world's reason by its word, any other as a refusal it cannot explain, names with
    /// Discord's markdown escaped.
    #[test]
    fn the_person_is_told_how_its_line_ended() {
        let ann = a("Ann@coldeve");
        let told = |said: Said| answer_text(&ann, "Coldeve", &said);
        assert_eq!(told(Said::Said), "Said as Ann@coldeve.");
        assert_eq!(
            told(Said::Refused(Some("gagged".into()))),
            "Ann@coldeve did not say it: it is gagged on Coldeve."
        );
        assert_eq!(
            told(Said::Refused(Some("a_later_word".into()))),
            "Ann@coldeve did not say it: Coldeve refused it."
        );
        assert_eq!(
            told(Said::Refused(None)),
            "Ann@coldeve did not say it: Coldeve refused it."
        );
        assert_eq!(told(Said::NoAnswer), "Coldeve did not answer in time.");
        assert_eq!(
            told(Said::Unavailable),
            "Coldeve can't be reached right now."
        );
        assert_eq!(
            told(Said::NotCarried),
            "Coldeve does not carry this room now."
        );
        for r in [
            reason::NO_CHARACTER,
            reason::NOT_ON_NETWORK,
            reason::NOT_LINKED,
            reason::BANNED,
            reason::GAGGED,
            reason::TOO_NEW,
            reason::RATE_LIMITED,
            reason::ROOM_OFF,
            reason::NOT_ALLOWED,
        ] {
            assert_ne!(
                reason_words("Coldeve", Some(r)),
                reason_words("Coldeve", None),
                "{r} has words of its own"
            );
        }
        assert_eq!(
            answer_text(&a("Ann@cold-eve"), "*Cold*", &Said::NoAnswer),
            "\\*Cold\\* did not answer in time."
        );
        assert_eq!(
            answer_text(&a("Ann@cold-eve"), "x", &Said::Said),
            "Said as Ann@cold\\-eve."
        );
        assert_eq!(
            read_only(&Bridged::Channel("oldguard".into())),
            "This channel mirrors the channel oldguard@channel for everyone to read. Speak in it in game."
        );
        assert_eq!(
            not_linked("Coldeve"),
            "You have no character linked on Coldeve. To link one, type @dn link in game and open the link it gives you."
        );
        assert_eq!(
            chosen_text("Coldeve", &ann),
            "You speak as Ann@coldeve on Coldeve now."
        );
    }

    /// The registry with the worlds `aa` and `bb` on the network, the person `200` linking
    /// `Ann@aa`, `Bo@aa` and `Ann@bb`, and the person `300` linking `Cy@aa`.
    fn registry() -> (
        Registry,
        crate::operators::Operator,
        crate::operators::Operator,
    ) {
        let r = Registry::open(":memory:", "dereth.network", Box::new(SystemTime::now))
            .expect("a registry");
        for slug in ["aa", "bb"] {
            let reg = crate::registry::Registration::from_json(&json!({
                "slug": slug, "name": format!("World {slug}"), "ruleset": "PvE",
                "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
                "emulator": { "family": "ACE" },
            }))
            .expect("a registration");
            r.register(&reg).expect("registered");
            r.set_federation(slug, Federation::On).expect("on");
        }
        let nelly = r.signed_in("200", "Nelly").expect("signed in");
        let otto = r.signed_in("300", "Otto").expect("signed in");
        for (op, who) in [
            (&nelly, "Ann@aa"),
            (&nelly, "Bo@aa"),
            (&nelly, "Ann@bb"),
            (&otto, "Cy@aa"),
        ] {
            on(&r, who);
            let (code, _) = r.person_code(&a(who)).expect("a code");
            r.redeem_link_code(op, &code).expect("linked");
        }
        (r, nelly, otto)
    }

    /// `who`'s world puts it on the network, and so lists it in the directory.
    fn on(r: &Registry, who: &str) {
        let profile = dereth_network_protocol::Profile {
            level: 20,
            heritage: 1,
            gender: 1,
            ..dereth_network_protocol::Profile::default()
        };
        r.listing(&a(who), Some(&profile)).expect("listed");
    }

    /// Spec: hub: a person speaks only as its characters on the network: one off it pauses its
    /// link, so the menu leaves it out and a choice of it holds no more until it is back on, kept
    /// with nothing to redo; with every character on the world off, the person is told to put one
    /// on.
    #[test]
    fn a_character_off_the_network_is_not_offered_until_it_is_back_on() {
        let (r, nelly, _) = registry();
        assert_eq!(r.set_identity(&nelly.id, "aa", Some(&a("Bo@aa"))), Ok(true));
        r.listing(&a("Bo@aa"), None).expect("off");
        assert_eq!(r.characters_on(&nelly.id, "aa"), Ok(vec![a("Ann@aa")]));
        assert_eq!(
            r.identity(&nelly.id, "aa"),
            Ok(None),
            "the choice holds no more"
        );
        assert_eq!(
            r.set_identity(&nelly.id, "aa", Some(&a("Bo@aa"))),
            Ok(false),
            "nor may it be chosen"
        );
        on(&r, "Bo@aa");
        assert_eq!(
            r.identity(&nelly.id, "aa"),
            Ok(Some(a("Bo@aa"))),
            "back on, chosen still"
        );
        r.listing(&a("Ann@bb"), None).expect("off");
        assert_eq!(r.characters_on(&nelly.id, "bb"), Ok(vec![]));
        assert_eq!(r.links_on(&nelly.id, "bb"), Ok(true), "linked still");
        assert_eq!(
            all_off("World bb"),
            "Your characters linked on World bb are off the Dereth Network. Type @dn on in game to speak as one."
        );
    }

    /// Spec: hub: a person's choice is one of its own characters on that world, kept per person
    /// per world; another person's character, a character on another world, and one nobody linked
    /// are refused, and nobody may be chosen.
    #[test]
    fn a_choice_is_only_ever_the_persons_own_character_on_that_world() {
        let (r, nelly, otto) = registry();
        assert_eq!(r.person_by_discord("200"), Ok(Some(nelly.id.clone())));
        assert_eq!(r.person_by_discord("999"), Ok(None));
        assert_eq!(
            r.characters_on(&nelly.id, "aa").expect("read"),
            [a("Ann@aa"), a("Bo@aa")]
        );
        assert_eq!(r.identity(&nelly.id, "aa"), Ok(None));
        assert_eq!(
            r.set_identity(&nelly.id, "aa", Some(&a("Cy@aa"))),
            Ok(false),
            "another's"
        );
        assert_eq!(
            r.set_identity(&nelly.id, "aa", Some(&a("Ann@bb"))),
            Ok(false),
            "another world's"
        );
        assert_eq!(
            r.set_identity(&nelly.id, "aa", Some(&a("Dee@aa"))),
            Ok(false),
            "nobody's"
        );
        assert_eq!(r.identity(&nelly.id, "aa"), Ok(None));
        assert_eq!(
            r.set_identity(&nelly.id, "aa", Some(&a("ann@AA"))),
            Ok(true)
        );
        assert_eq!(r.identity(&nelly.id, "aa"), Ok(Some(a("Ann@aa"))));
        assert_eq!(
            r.set_identity(&nelly.id, "bb", Some(&a("Ann@bb"))),
            Ok(true)
        );
        assert_eq!(r.set_identity(&otto.id, "aa", Some(&a("Cy@aa"))), Ok(true));
        assert_eq!(
            r.identity(&nelly.id, "aa"),
            Ok(Some(a("Ann@aa"))),
            "per person"
        );
        assert_eq!(
            r.identity(&nelly.id, "bb"),
            Ok(Some(a("Ann@bb"))),
            "per world"
        );
        assert_eq!(r.set_identity(&nelly.id, "aa", Some(&a("Bo@aa"))), Ok(true));
        assert_eq!(
            r.identity(&nelly.id, "aa"),
            Ok(Some(a("Bo@aa"))),
            "replaced"
        );
        assert_eq!(r.set_identity(&nelly.id, "aa", None), Ok(true));
        assert_eq!(r.identity(&nelly.id, "aa"), Ok(None), "nobody");
    }

    /// Spec: hub: a choice goes when its character stops being the person's: unlinked by its
    /// world or on the site, linked by another person, said by its world to be no such character,
    /// the person forgotten, or the world leaving the registry.
    #[test]
    fn a_choice_goes_when_its_character_stops_being_the_persons() {
        let (r, nelly, otto) = registry();
        let choose = |who: &str| {
            assert_eq!(
                r.set_identity(&nelly.id, a(who).world(), Some(&a(who))),
                Ok(true)
            );
        };
        choose("Ann@aa");
        r.person_unlink(&a("Ann@aa")).expect("unlinked");
        assert_eq!(r.identity(&nelly.id, "aa"), Ok(None), "the world's unlink");
        choose("Bo@aa");
        r.unlink_for(&nelly, "Bo@aa").expect("unlinked");
        assert_eq!(r.identity(&nelly.id, "aa"), Ok(None), "the site's unlink");
        choose("Ann@bb");
        let (code, _) = r.person_code(&a("Ann@bb")).expect("a code");
        r.redeem_link_code(&otto, &code).expect("linked by another");
        assert_eq!(
            r.identity(&nelly.id, "bb"),
            Ok(None),
            "another person links it"
        );
        assert_eq!(
            r.identity(&otto.id, "bb"),
            Ok(None),
            "nor is it the other's choice"
        );
        let (code, _) = r.person_code(&a("Ann@bb")).expect("a code");
        r.redeem_link_code(&nelly, &code).expect("linked back");
        choose("Ann@bb");
        r.drop_identity(&nelly.id, &a("Ann@bb")).expect("dropped");
        assert_eq!(
            r.identity(&nelly.id, "bb"),
            Ok(None),
            "its world has no such character"
        );
        choose("Ann@bb");
        r.remove("bb").expect("removed");
        assert_eq!(r.identity(&nelly.id, "bb"), Ok(None), "the world leaves");
        let (code, _) = r.person_code(&a("Ann@aa")).expect("a code");
        r.redeem_link_code(&nelly, &code).expect("linked again");
        choose("Ann@aa");
        r.forget_person(&nelly).expect("forgotten");
        assert_eq!(
            r.identity(&nelly.id, "aa"),
            Ok(None),
            "the person forgotten"
        );
        let left: i64 =
            r.db.query_row(
                "SELECT count(*) FROM person_identities WHERE person_id = ?",
                [&nelly.id],
                |row| row.get(0),
            )
            .expect("counted");
        assert_eq!(left, 0, "nothing kept");
    }

    /// Spec: hub: an interaction's token goes in a path only when it is URL-safe.
    #[test]
    fn an_interactions_token_goes_in_a_path_only_when_it_is_url_safe() {
        assert!(is_interaction_token("aW50ZXJhY3Rpb246MTIz.abc-DEF_9"));
        for bad in ["", "a/b", "a?b", "a b", "a%2Fb", &"x".repeat(1_025)] {
            assert!(!is_interaction_token(bad), "{bad}");
        }
    }
}
