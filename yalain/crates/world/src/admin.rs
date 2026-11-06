//! The operator's moderation and the admin model: admin accounts, bans, gags, boots, and accounts
//! made and changed while the world runs.
//!
//! - **The admin flag** per account ([`Account::admin`]), as the game's own model has it: one admin
//!   class, no levels. Every new account is a player's. Only the operator raises one (the console,
//!   or the server's configuration); no account is made an admin for being the first, since an open
//!   world makes an account for anyone who logs in. An account with no secret cannot be raised: an
//!   open world gives such an account to the first ticket it sees.
//! - **Admin characters** are separate characters of an admin's account, made from the admin
//!   template ([`crate::player::ADMIN_ENTITY`]): asked for at creation (refused with the client's own
//!   answer for any other account), or made so at the console (`set-characteradmin`). A character
//!   enters as one only while its account is an admin's; every check is on the account's flag, never
//!   on anything a client says.
//! - **Commands** ([`Command`], [`parse`]), with Empyrean's words and answers: `accountcreate`,
//!   `accountget`, `accountlist`, `set-accountpassword`, `set-accountadmin`, `set-characteradmin`,
//!   `ban`, `unban`, `banlist`, `gag`, `ungag`, `boot` and `gamecast`. Each needs an admin's account;
//!   the console is one. Each change is kept in the store at once, and every command is logged with
//!   who gave it (never a secret). Each line of an answer is what was done or why nothing was
//!   ([`answered`]), which the console's tags tell apart.
//! - **A broadcast** (`gamecast`) shows every player in the world `Broadcast from <who>>
//!   <message>`, as Empyrean's, `<who>` the issuer's character or `System` from the console.
//! - **A ban** keeps an account out until it ends: its game login is booted with the client's
//!   line that the account is banned ([`BANNED`]), and its friends ticket refused with the friends
//!   stream's banned code, which the client shows as the same line. A ban that has run out is lifted at the
//!   account's next login, as Empyrean lifts one. Banning an account boots it.
//! - **A gag** stops a character's chat for five minutes of its time in the world, as Empyrean's:
//!   what it says aloud, its tells (the game's and the friends service's, to this world or
//!   another), its emotes and its lines to the chat rooms. It is told as the gag starts, as it
//!   enters while gagged, and when the gag is over. A gagged character out of the world keeps the
//!   rest of its gag until it comes back.
//! - **A boot** ends a login: its client is sent the message that ends a login (`AccountBooted`)
//!   with the client's own line that its character was booted ([`BOOTED`]), and its connection is
//!   closed a moment later; its character then leaves the world as when its client goes away.
//!
//! **In game.** The September 2013 client has admin commands of its own (`/boot`, `/ban` ...),
//! but only for a character of the admin's class, and it sends them encrypted under a key only
//! Turbine's servers held, so a server cannot read them. The commands are said instead, as the
//! Dereth Network's are: a line said aloud or into a room that begins with `@` and a command's
//! word (`@ban ...`, `@boot char ...`), by a character of an admin's account, is heard by nobody and
//! answered in its chat window ([`said`]). From a player's account the line is said as any other,
//! and logged, but for one command any account may give:
//! - **`@passwd <old> <new>`** changes the account's own password, as Empyrean's does
//!   ([`passwd`]): the old one must match, the new one is kept as a bcrypt secret, and the command
//!   may be given once every five seconds. It is given in game only.

use std::collections::HashMap;
use std::fmt;

use auberean_protocol::login::Text;
use auberean_protocol::text::StringInfo;
use auberean_protocol::{queue, ServerMessage};
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::{Account, Ban, Store};

use crate::{Clock, Server, Stage, Timer};

/// Who gives a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issuer {
    /// The server's console: an admin always.
    Console,
    /// An account, by its character in the world.
    Account {
        name: String,
        /// The account's flag, as the server keeps it.
        admin: bool,
        /// Its named groups.
        groups: Vec<String>,
        character: String,
    },
}

impl Issuer {
    /// May the issuer give admin commands?
    #[must_use]
    pub const fn is_admin(&self) -> bool {
        match self {
            Self::Console => true,
            Self::Account { admin, .. } => *admin,
        }
    }

    /// Who a ban names as its maker.
    fn maker(&self) -> String {
        match self {
            Self::Console => "CONSOLE".to_owned(),
            Self::Account { name, .. } => format!("account {name}"),
        }
    }
}

impl fmt::Display for Issuer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Console => f.write_str("the console"),
            Self::Account { name, admin, .. } => {
                let what = if *admin { "admin" } else { "player" };
                write!(f, "account {name} ({what})")
            }
        }
    }
}

/// How an account's rights read, as Empyrean's answers name them.
const fn rights(admin: bool) -> &'static str {
    if admin {
        "an Admin"
    } else {
        "a Player"
    }
}

/// What a boot names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootBy {
    /// A character's name.
    Character,
    /// An account's name.
    Account,
    /// A character's object id (`0x...`).
    Id,
}

impl BootBy {
    const fn what(self) -> &'static str {
        match self {
            Self::Character => "character",
            Self::Account => "account",
            Self::Id => "instance id",
        }
    }
}

/// An operator's command.
#[derive(Clone, PartialEq)]
pub enum Command {
    /// `accountcreate <name> <secret> [admin]`.
    AccountCreate {
        name: String,
        ticket: String,
        admin: bool,
    },
    /// `accountget <name>`.
    AccountGet { name: String },
    /// `accountlist`.
    AccountList,
    /// `set-accountpassword <name> <secret>`.
    SetAccountPassword { name: String, ticket: String },
    /// `set-accountadmin <name> [on|off]` (on when neither is given).
    SetAccountAdmin { name: String, admin: bool },
    /// `set-characteradmin <name> [on|off]` (on when neither is given).
    SetCharacterAdmin { name: String, admin: bool },
    /// `set-accountgroups <name> [group,group...]` (none when none are given).
    SetAccountGroups { name: String, groups: Vec<String> },
    /// `ban <account> <days> <hours> <minutes> [reason]`.
    Ban {
        name: String,
        days: f64,
        hours: f64,
        minutes: f64,
        reason: Option<String>,
    },
    /// `unban <account>`.
    Unban { name: String },
    /// `banlist`.
    BanList,
    /// `gag <character>`.
    Gag { name: String },
    /// `ungag <character>`.
    Ungag { name: String },
    /// `boot {char|account|iid} <who>[, reason]`.
    Boot {
        by: BootBy,
        who: String,
        reason: Option<String>,
    },
    /// `gamecast <message>`.
    Gamecast { message: String },
}

// A secret is never written out, even to a debug line.
impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AccountCreate { name, admin, .. } => f
                .debug_struct("AccountCreate")
                .field("name", name)
                .field("admin", admin)
                .finish_non_exhaustive(),
            Self::SetAccountPassword { name, .. } => f
                .debug_struct("SetAccountPassword")
                .field("name", name)
                .finish_non_exhaustive(),
            Self::AccountGet { name } => write!(f, "AccountGet({name})"),
            Self::AccountList => f.write_str("AccountList"),
            Self::SetAccountAdmin { name, admin } => {
                write!(f, "SetAccountAdmin({name}, {admin})")
            }
            Self::SetCharacterAdmin { name, admin } => {
                write!(f, "SetCharacterAdmin({name}, {admin})")
            }
            Self::SetAccountGroups { name, groups } => {
                write!(f, "SetAccountGroups({name}, {groups:?})")
            }
            Self::Ban {
                name,
                days,
                hours,
                minutes,
                reason,
            } => write!(f, "Ban({name}, {days}, {hours}, {minutes}, {reason:?})"),
            Self::Unban { name } => write!(f, "Unban({name})"),
            Self::BanList => f.write_str("BanList"),
            Self::Gag { name } => write!(f, "Gag({name})"),
            Self::Ungag { name } => write!(f, "Ungag({name})"),
            Self::Boot { by, who, reason } => write!(f, "Boot({by:?}, {who}, {reason:?})"),
            Self::Gamecast { message } => write!(f, "Gamecast({message})"),
        }
    }
}

impl Command {
    /// Is this the operator's alone, at the console? What makes an admin, or changes its groups: an
    /// admin kept out of a command by its groups could otherwise put itself in them. So is a new
    /// account made an admin's.
    #[must_use]
    pub const fn console_only(&self) -> bool {
        matches!(
            self,
            Self::SetAccountAdmin { .. }
                | Self::SetCharacterAdmin { .. }
                | Self::SetAccountGroups { .. }
                | Self::AccountCreate { admin: true, .. }
        )
    }

    /// The command's word, as [`WORDS`] has it.
    #[must_use]
    pub const fn word(&self) -> &'static str {
        match self {
            Self::AccountCreate { .. } => "accountcreate",
            Self::AccountGet { .. } => "accountget",
            Self::AccountList => "accountlist",
            Self::SetAccountPassword { .. } => "set-accountpassword",
            Self::SetAccountAdmin { .. } => "set-accountadmin",
            Self::SetCharacterAdmin { .. } => "set-characteradmin",
            Self::SetAccountGroups { .. } => "set-accountgroups",
            Self::Ban { .. } => "ban",
            Self::Unban { .. } => "unban",
            Self::BanList => "banlist",
            Self::Gag { .. } => "gag",
            Self::Ungag { .. } => "ungag",
            Self::Boot { .. } => "boot",
            Self::Gamecast { .. } => "gamecast",
        }
    }
}

/// The commands and how each is written, as `help` lists them.
pub const HELP: &str = "accountcreate <name> <secret> [admin], accountget <name>, accountlist, \
                        set-accountpassword <name> <secret>, set-accountadmin <name> [on|off], \
                        set-characteradmin <name> [on|off], set-accountgroups <name> \
                        [group,group...], ban <account> <days> <hours> <minutes> \
                        [reason], unban <account>, banlist, gag <character>, ungag <character>, \
                        boot {char|account|iid} <who>[, reason], gamecast <message>";

/// The words of the commands this module reads.
pub const WORDS: [&str; 15] = [
    "accountcreate",
    "accountget",
    "accountlist",
    "set-accountpassword",
    "set-accountadmin",
    "set-characteradmin",
    "set-accountgroups",
    "set-accountaccess",
    "ban",
    "unban",
    "banlist",
    "gag",
    "ungag",
    "boot",
    "gamecast",
];

/// A word of a command's line: a run without spaces, or a run between double quotes (spaces
/// and all, the quotes taken off), so a name with spaces can be one word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub text: String,
    /// It was written between quotes.
    pub quoted: bool,
}

/// The words of a line (see [`Word`]); a quote left open runs to the line's end.
#[must_use]
pub fn words(line: &str) -> Vec<Word> {
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '"' {
            chars.next();
            let text: String = chars.by_ref().take_while(|c| *c != '"').collect();
            out.push(Word { text, quoted: true });
        } else {
            let mut text = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                text.push(c);
                chars.next();
            }
            out.push(Word {
                text,
                quoted: false,
            });
        }
    }
    out
}

/// A name written as the rest of a line: one quoted word as it is, else the words joined by
/// single spaces.
#[must_use]
pub fn name_of(words: &[Word]) -> String {
    match words {
        [w] => w.text.clone(),
        _ => words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// Is a word `on` or `off`, in any case?
fn is_on_off(word: &str) -> bool {
    word.eq_ignore_ascii_case("on") || word.eq_ignore_ascii_case("off")
}

/// The command a line holds: `None` when its first word is none of [`WORDS`] (in any case);
/// otherwise the command, or how it is written when the line does not read. A name or a secret
/// with spaces is written between double quotes.
#[must_use]
pub fn parse(line: &str) -> Option<Result<Command, String>> {
    let mut words = line.split_whitespace();
    let word = words.next()?.to_ascii_lowercase();
    if !WORDS.contains(&word.as_str()) {
        return None;
    }
    let quoted = self::words(line);
    let args: Vec<&str> = quoted[1..].iter().map(|w| w.text.as_str()).collect();
    // What follows the command's word, as typed.
    let rest = line.trim_start()[word.len()..].trim().to_owned();
    let usage = |u: &str| Err(format!("{word} {u}"));
    let on_off = |w: Option<&Word>| match w {
        None => Some(true),
        Some(w) if !w.quoted && w.text.eq_ignore_ascii_case("on") => Some(true),
        Some(w) if !w.quoted && w.text.eq_ignore_ascii_case("off") => Some(false),
        Some(_) => None,
    };
    let number = |w: &str| w.parse::<f64>().ok().filter(|n| n.is_finite());
    Some(match (word.as_str(), &args[..]) {
        ("accountcreate", [name, ticket]) => Ok(Command::AccountCreate {
            name: (*name).to_owned(),
            ticket: (*ticket).to_owned(),
            admin: false,
        }),
        ("accountcreate", [name, ticket, w]) if w.eq_ignore_ascii_case("admin") => {
            Ok(Command::AccountCreate {
                name: (*name).to_owned(),
                ticket: (*ticket).to_owned(),
                admin: true,
            })
        }
        ("accountcreate", _) => usage("<name> <secret> [admin]"),
        ("accountget", [name]) => Ok(Command::AccountGet {
            name: (*name).to_owned(),
        }),
        ("accountget", _) => usage("<name>"),
        ("accountlist", []) => Ok(Command::AccountList),
        ("accountlist", _) => usage(""),
        ("set-accountpassword", [name, ticket]) => Ok(Command::SetAccountPassword {
            name: (*name).to_owned(),
            ticket: (*ticket).to_owned(),
        }),
        ("set-accountpassword", _) => usage("<name> <secret>"),
        ("set-accountaccess", _) => {
            Err("Access levels are gone: set-accountadmin <name> [on|off].".to_owned())
        }
        ("set-accountadmin", [_, ..]) => {
            let flag = quoted.last().filter(|w| !w.quoted && is_on_off(&w.text));
            let name = name_of(&quoted[1..quoted.len() - usize::from(flag.is_some())]);
            match on_off(flag) {
                Some(admin) if !name.is_empty() => Ok(Command::SetAccountAdmin { name, admin }),
                _ => usage("<name> [on|off]"),
            }
        }
        ("set-accountadmin", _) => usage("<name> [on|off]"),
        ("set-characteradmin", [_, ..]) => {
            let flag = quoted.last().filter(|w| !w.quoted && is_on_off(&w.text));
            let name = name_of(&quoted[1..quoted.len() - usize::from(flag.is_some())]);
            match on_off(flag) {
                Some(admin) if !name.is_empty() => Ok(Command::SetCharacterAdmin { name, admin }),
                _ => usage("<name> [on|off]"),
            }
        }
        ("set-characteradmin", _) => usage("<name> [on|off]"),
        ("set-accountgroups", [name, more @ ..]) if more.len() <= 1 => {
            Ok(Command::SetAccountGroups {
                name: (*name).to_owned(),
                groups: more
                    .first()
                    .map_or_else(Vec::new, |g| crate::admin_groups::read_list(g)),
            })
        }
        ("set-accountgroups", _) => usage("<name> [group,group...]"),
        ("ban", [name, d, h, m, reason @ ..]) => match (number(d), number(h), number(m)) {
            (Some(days), Some(hours), Some(minutes)) => Ok(Command::Ban {
                name: (*name).to_owned(),
                days,
                hours,
                minutes,
                reason: (!reason.is_empty()).then(|| reason.join(" ")),
            }),
            _ => usage("<account> <days> <hours> <minutes> [reason]"),
        },
        ("ban", _) => usage("<account> <days> <hours> <minutes> [reason]"),
        ("unban", [name]) => Ok(Command::Unban {
            name: (*name).to_owned(),
        }),
        ("unban", _) => usage("<account>"),
        ("banlist", []) => Ok(Command::BanList),
        ("banlist", _) => usage(""),
        ("gag" | "ungag", [_, ..]) => {
            let name = name_of(&quoted[1..]);
            Ok(if word == "gag" {
                Command::Gag { name }
            } else {
                Command::Ungag { name }
            })
        }
        ("gag" | "ungag", []) => usage("<character>"),
        ("boot", [kind, _, ..]) => {
            let by = match kind.to_ascii_lowercase().as_str() {
                "char" => Some(BootBy::Character),
                "account" => Some(BootBy::Account),
                "iid" => Some(BootBy::Id),
                _ => None,
            };
            match by {
                // The one named, then a reason after a comma, as Empyrean reads them.
                Some(by) => {
                    let after = rest[kind.len()..].trim();
                    let (who, reason) = match after.split_once(',') {
                        Some((w, r)) => (w.trim(), Some(r.trim()).filter(|r| !r.is_empty())),
                        None => (after, None),
                    };
                    Ok(Command::Boot {
                        by,
                        who: who.trim_matches('"').to_owned(),
                        reason: reason.map(str::to_owned),
                    })
                }
                None => Err(
                    "You must specify what you are booting with char, account, or iid \
                             as the first parameter."
                        .to_owned(),
                ),
            }
        }
        ("boot", _) => usage("{char|account|iid} <who>[, reason]"),
        ("gamecast", [_, ..]) => Ok(Command::Gamecast { message: rest }),
        ("gamecast", []) => usage("<message>"),
        _ => return None,
    })
}

// ---- refusals and lines ------------------------------------------------------------------------

/// The table of the client's strings a boot's line is in.
pub const STRINGS: u32 = 0x2500_0012;
/// "Your character has been booted from the world.": the line the September 2013 client shows
/// for a boot that names none of its own.
pub const BOOTED: u32 = 0xDA7D_EF7F;
/// "Your account is currently banned.": the client's line for a banned account, which its friends
/// ticket is refused with too.
pub const BANNED: u32 = 0x974F_243D;
/// Seconds between a boot's message and its connection's closing, so the message reaches the
/// client first.
pub const BOOT_CLOSE_AFTER: f64 = 1.0;
/// How long a gag runs, in seconds of the character's time in the world (Empyrean's five
/// minutes).
pub const GAG_SECONDS: f64 = 300.0;
/// What a gagged character is told when it tries to talk, as Empyrean tells it.
pub const GAGGED: &str =
    "You are unable to talk locally, globally, or send tells because you have been gagged.";
/// What a character is told as its gag starts, and as it enters while gagged.
pub const GAG_NOTICE: &str = "Your chat privileges have been suspended.";
/// What a character is told as its gag ends.
pub const UNGAG_NOTICE: &str = "Your chat privileges have been restored.";

// ---- gags -------------------------------------------------------------------------------------

/// The gags of the characters in the world: the seconds each has still to run, counted down as
/// the world runs. A character out of the world keeps the rest of its gag in the store.
#[derive(Debug, Default)]
pub struct Gags {
    left: HashMap<u64, f64>,
    /// When they were last counted down (the transport's seconds).
    counted: Option<f64>,
}

impl Gags {
    /// Is a character in the world gagged?
    #[must_use]
    pub fn gagged(&self, character: u64) -> bool {
        self.left.contains_key(&character)
    }

    /// The characters in the world that are gagged.
    #[must_use]
    pub fn characters(&self) -> std::collections::HashSet<u64> {
        self.left.keys().copied().collect()
    }

    /// The seconds a gagged character in the world has still to run.
    #[must_use]
    pub fn left(&self, character: u64) -> Option<f64> {
        self.left.get(&character).copied()
    }
}

/// A character entered the world: a gag it keeps runs on, and it is told.
pub(crate) fn entered<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(id) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    match s.store.gag(id) {
        Ok(Some(left)) => {
            s.gags.left.insert(id, left);
            crate::login::notice(net, world, GAG_NOTICE);
        }
        Ok(None) => {}
        Err(e) => tracing::warn!(id = format_args!("{id:#x}"), "the gag was not read: {e}"),
    }
}

/// A character is kept: the rest of its gag with it.
pub(crate) fn kept<S: Store>(s: &mut Server<S>, character: u64) {
    if let Some(left) = s.gags.left(character) {
        if let Err(e) = s.store.set_gag(character, Some(left)) {
            tracing::warn!(
                id = format_args!("{character:#x}"),
                "the gag was not kept: {e}"
            );
        }
    }
}

/// A character left the world (kept already): its gag waits in the store.
pub(crate) fn left<S: Store>(s: &mut Server<S>, character: u64) {
    s.gags.left.remove(&character);
}

/// Count the gags down by the time since they were last counted; a gag that has run its time is
/// lifted, kept so, and its character told.
pub(crate) fn tick<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    let now = clock.local.0;
    let since = s.gags.counted.map_or(0.0, |t| (now - t).max(0.0));
    s.gags.counted = Some(now);
    if s.gags.left.is_empty() || since == 0.0 {
        return;
    }
    let mut over = Vec::new();
    for (id, left) in &mut s.gags.left {
        *left -= since;
        if *left <= 0.0 {
            over.push(*id);
        }
    }
    for id in over {
        s.gags.left.remove(&id);
        if let Err(e) = s.store.set_gag(id, None) {
            tracing::warn!(
                id = format_args!("{id:#x}"),
                "the gag's end was not kept: {e}"
            );
        }
        if let Some((world, name)) = in_world(s, id) {
            tracing::info!(character = %name, "the gag has run its time");
            crate::login::notice(net, world, UNGAG_NOTICE);
        }
    }
}

/// Tell a gagged character, in the world, that it may not talk: true when it is gagged.
pub(crate) fn refuse_gagged<S: Store>(s: &Server<S>, net: &mut ServerNet, world: ConnId) -> bool {
    let gagged = s
        .players
        .get(&world)
        .is_some_and(|p| s.gags.gagged(p.character));
    if gagged {
        crate::login::notice(net, world, GAGGED);
    }
    gagged
}

/// A character in the world by id: its world connection and name.
fn in_world<S: Store>(s: &Server<S>, character: u64) -> Option<(ConnId, String)> {
    s.players
        .iter()
        .find(|(_, p)| p.character == character)
        .map(|(w, p)| (*w, p.name.clone()))
}

// ---- bans -------------------------------------------------------------------------------------

/// Is an account banned now? A ban that has run out is lifted, and kept so.
pub fn banned<S: Store>(store: &mut S, account: &Account, now: i64) -> bool {
    let Some(ban) = &account.ban else {
        return false;
    };
    if now < ban.until {
        return true;
    }
    match store.set_account_ban(account.id, None) {
        Ok(()) => {
            tracing::info!(account = %account.name, "the account's ban has run out and is lifted")
        }
        Err(e) => tracing::warn!(account = %account.name, "the ban's end was not kept: {e}"),
    }
    false
}

/// What a friends ticket is answered with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ticket {
    /// Admitted, as this account.
    Admitted(Account),
    /// Refused, as a wrong secret is.
    Refused,
    /// Its secret matched a banned account: refused as banned.
    Banned,
}

/// Check a friends ticket by the login's rule ([`crate::login::admits`]), from an address as the
/// login keeps it ([`crate::login::address_key`]): a banned account's is refused as banned, and only
/// once its secret matched. It counts among the secrets checked as a login does: its host checks
/// one only when [`crate::login::secret_check_due`] says.
pub fn ticket<S: Store>(
    s: &mut Server<S>,
    name: &str,
    secret: &[u8],
    address: &str,
    now: i64,
) -> Ticket {
    if !crate::login::may_check_secret(s, address, name) {
        return Ticket::Refused;
    }
    crate::login::secret_checked(s);
    match crate::login::admits(
        s.cfg.accounts,
        s.cfg.secret_cost,
        &mut s.store,
        name,
        secret,
        now,
    ) {
        None => Ticket::Refused,
        Some(a) if banned(&mut s.store, &a, now) => Ticket::Banned,
        Some(a) => Ticket::Admitted(a),
    }
}

// ---- the commands -----------------------------------------------------------------------------

/// What an admin command from anyone else is answered with.
pub const NOT_AN_ADMIN: &str = "That command needs an admin's account.";

/// What a command that is the operator's alone is answered with in game.
pub const CONSOLE_ONLY: &str = "That command is the operator's, at the console.";

/// A line of a command's answer: what was done, or why nothing was.
pub type Answer = Result<String, String>;

/// Run a command for its issuer: the lines that answer it. A command from anyone but an admin does
/// nothing. Every command run is logged with its issuer and answer.
pub fn run<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    clock: Clock,
    by: &Issuer,
    c: &Command,
) -> Vec<String> {
    answered(s, net, clock, by, c)
        .into_iter()
        .map(|a| a.unwrap_or_else(|why| why))
        .collect()
}

/// Run a command for its issuer, as [`run`]: each line of its answer, as what was done or why
/// nothing was, for a program that tells the two apart (the console's tags).
pub fn answered<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    clock: Clock,
    by: &Issuer,
    c: &Command,
) -> Vec<Answer> {
    if !by.is_admin() {
        tracing::warn!(issuer = %by, command = ?c, "refused: not an admin's account");
        return vec![Err(NOT_AN_ADMIN.to_owned())];
    }
    if c.console_only() && !matches!(by, Issuer::Console) {
        tracing::warn!(issuer = %by, command = ?c, "refused: the operator's, at the console");
        return vec![Err(CONSOLE_ONLY.to_owned())];
    }
    if let Some(refused) = crate::admin_groups::refusal(s, by, c.word()) {
        tracing::warn!(issuer = %by, command = ?c, "refused: {refused}");
        return vec![Err(refused)];
    }
    let lines = match c {
        Command::AccountCreate {
            name,
            ticket,
            admin,
        } => vec![account_create(s, clock, name, ticket, *admin)],
        Command::AccountGet { name } => vec![account_get(s, name, clock)],
        Command::AccountList => account_list(s, clock),
        Command::SetAccountPassword { name, ticket } => {
            vec![set_account_password(s, by, name, ticket)]
        }
        Command::SetAccountAdmin { name, admin } => vec![set_account_admin(s, net, name, *admin)],
        Command::SetCharacterAdmin { name, admin } => {
            vec![set_character_admin(s, net, name, *admin)]
        }
        Command::SetAccountGroups { name, groups } => vec![set_account_groups(s, name, groups)],
        Command::Ban {
            name,
            days,
            hours,
            minutes,
            reason,
        } => ban(
            s,
            net,
            clock,
            by,
            name,
            [*days, *hours, *minutes],
            reason.as_deref(),
        ),
        Command::Unban { name } => vec![unban(s, name)],
        Command::BanList => ban_list(s, clock),
        Command::Gag { name } => vec![gag(s, net, name, true)],
        Command::Ungag { name } => vec![gag(s, net, name, false)],
        Command::Boot {
            by: kind,
            who,
            reason,
        } => {
            vec![boot(s, net, clock, *kind, who, reason.as_deref())]
        }
        Command::Gamecast { message } => vec![gamecast(s, net, by, message)],
    };
    // The console logs each answer itself, in the fixed form.
    if !matches!(by, Issuer::Console) {
        for line in &lines {
            let (Ok(line) | Err(line)) = line;
            tracing::info!(issuer = %by, "admin: {line}");
        }
    }
    lines
}

/// `gamecast <message>`, as Empyrean's: every player in the world is shown `Broadcast from
/// <who>> <message>`, `<who>` the issuer's character, or `System` from the console.
fn gamecast<S: Store>(s: &Server<S>, net: &mut ServerNet, by: &Issuer, message: &str) -> Answer {
    let from = match by {
        Issuer::Console => "System",
        Issuer::Account { character, .. } => character.as_str(),
    };
    let text = format!("Broadcast from {from}> {message}");
    let mut worlds: Vec<ConnId> = s.players.keys().copied().collect();
    worlds.sort_unstable();
    for world in &worlds {
        crate::login::notice(net, *world, &text);
    }
    Ok(format!(
        "{text} (shown to {} in the world)",
        match worlds.len() {
            1 => "the one player".to_owned(),
            n => format!("{n} players"),
        }
    ))
}

fn account_create<S: Store>(
    s: &mut Server<S>,
    clock: Clock,
    name: &str,
    ticket: &str,
    admin: bool,
) -> Answer {
    if matches!(s.store.account_by_name(name), Ok(Some(_))) {
        return Err("Account already exists. Try a new name.".to_owned());
    }
    let made = crate::login::add_account(
        &mut s.store,
        name,
        ticket.as_bytes(),
        s.cfg.secret_cost,
        clock.unix,
    )
    .and_then(|a| {
        s.store.set_account_admin(a.id, admin)?;
        Ok(a)
    });
    match made {
        Ok(a) => Ok(format!(
            "Account successfully created for {} ({}) with access rights as {}.",
            a.name,
            a.id,
            rights(admin)
        )),
        Err(e) => Err(format!("Account {name} was not created: {e}")),
    }
}

/// An account's line: its name, id, whether an admin's, and any ban.
fn account_line(a: &Account, now: i64) -> String {
    let ban = match &a.ban {
        Some(b) if now < b.until => format!(", banned until {}", utc(b.until)),
        _ => String::new(),
    };
    format!(
        "User: {}, ID: {}, Access: {}{}{ban}",
        a.name,
        a.id,
        if a.admin { "Admin" } else { "Player" },
        if a.groups.is_empty() {
            String::new()
        } else {
            format!(", groups {}", a.groups.join(", "))
        }
    )
}

fn account_get<S: Store>(s: &mut Server<S>, name: &str, clock: Clock) -> Answer {
    match s.store.account_by_name(name) {
        Ok(Some(a)) => Ok(account_line(&a, clock.unix)),
        Ok(None) => Err(format!("Account {name} does not exist.")),
        Err(e) => Err(format!("Account {name} could not be read: {e}")),
    }
}

fn account_list<S: Store>(s: &mut Server<S>, clock: Clock) -> Vec<Answer> {
    match s.store.accounts() {
        Ok(all) if all.is_empty() => vec![Ok("There are no accounts.".to_owned())],
        Ok(all) => {
            let mut lines = vec![Ok(format!("{} accounts:", all.len()))];
            lines.extend(all.iter().map(|a| Ok(account_line(a, clock.unix))));
            lines
        }
        Err(e) => vec![Err(format!("The accounts could not be read: {e}"))],
    }
}

fn set_account_password<S: Store>(
    s: &mut Server<S>,
    by: &Issuer,
    name: &str,
    ticket: &str,
) -> Answer {
    let Ok(Some(a)) = s.store.account_by_name(name) else {
        return Err(format!("Account {name} does not exist."));
    };
    // Another admin's secret would be another admin's rights, groups and all.
    if a.admin && !matches!(by, Issuer::Console) {
        tracing::warn!(issuer = %by, account = %a.name, "refused: an admin's secret, at the console");
        return Err(CONSOLE_ONLY.to_owned());
    }
    let Some(secret) = crate::login::secret_for(ticket.as_bytes(), s.cfg.secret_cost) else {
        return Err(format!(
            "Account password for {name} was not changed: no secret could be made."
        ));
    };
    match s.store.set_account_secret(a.id, secret) {
        Ok(()) => Ok(format!(
            "Account password for {} successfully changed.",
            a.name
        )),
        Err(e) => Err(format!("Account password for {name} was not changed: {e}")),
    }
}

fn set_account_admin<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    name: &str,
    admin: bool,
) -> Answer {
    let Ok(Some(a)) = s.store.account_by_name(name) else {
        return Err(format!("Account {name} does not exist."));
    };
    // An open world gives an account without a secret to the first ticket it sees.
    if admin && a.secret.is_none() {
        return Err(format!(
            "Account {} has no secret, so anyone could claim it: set one with \
             set-accountpassword first.",
            a.name
        ));
    }
    if let Err(e) = s.store.set_account_admin(a.id, admin) {
        return Err(format!("Account {name} was not updated: {e}"));
    }
    // Its logins hold the new flag at once, and its characters' rights follow it.
    for session in s.sessions.values_mut() {
        if session.account.id == a.id {
            session.account.admin = admin;
        }
    }
    if !admin {
        let worlds: Vec<ConnId> = s
            .players
            .iter()
            .filter(|(_, p)| {
                s.sessions
                    .get(&p.login)
                    .is_some_and(|x| x.account.id == a.id)
            })
            .map(|(w, _)| *w)
            .collect();
        for world in worlds {
            crate::admin_calls::revoke(s, net, world);
        }
        for c in s.store.characters(a.id).unwrap_or_default() {
            crate::admin_calls::forget_kept(s, c.id);
        }
    }
    Ok(format!(
        "Account {} updated with access rights set as {}.",
        a.name,
        rights(admin)
    ))
}

fn set_character_admin<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    name: &str,
    admin: bool,
) -> Answer {
    let Ok(Some(c)) = s.store.character_named(name) else {
        return Err(format!("Character {name} does not exist."));
    };
    if c.deleted_at.is_some() {
        return Err(format!("Character {name} does not exist."));
    }
    if admin {
        let account = s.store.account(c.account).ok().flatten();
        if !account.is_some_and(|a| a.admin) {
            return Err(format!(
                "{}'s account is not an admin's: set-accountadmin first.",
                c.name
            ));
        }
    }
    let entity = if admin {
        crate::player::ADMIN_ENTITY
    } else {
        crate::player::PLAYER_ENTITY
    };
    if let Err(e) = s.store.set_character_entity(c.id, entity) {
        return Err(format!("Character {name} was not updated: {e}"));
    }
    let in_world = s
        .players
        .iter_mut()
        .find(|(_, p)| p.character == c.id)
        .map(|(w, p)| {
            // Its admin commands stop at once; its look as an admin character changes as it enters.
            p.admin_character = p.admin_character && admin;
            *w
        });
    if !admin {
        crate::admin_calls::forget_kept(s, c.id);
    }
    let told = in_world.map_or("", |world| {
        if !admin {
            crate::admin_calls::revoke(s, net, world);
        }
        " It takes effect the next time it enters the world."
    });
    Ok(format!(
        "{} is {} admin character.{told}",
        c.name,
        if admin { "now an" } else { "no longer an" }
    ))
}

fn set_account_groups<S: Store>(s: &mut Server<S>, name: &str, groups: &[String]) -> Answer {
    let Ok(Some(a)) = s.store.account_by_name(name) else {
        return Err(format!("Account {name} does not exist."));
    };
    if let Err(e) = s.store.set_account_groups(a.id, groups) {
        return Err(format!("Account {name} was not updated: {e}"));
    }
    for session in s.sessions.values_mut() {
        if session.account.id == a.id {
            session.account.groups = groups.to_vec();
        }
    }
    Ok(if groups.is_empty() {
        format!("Account {} is in no group.", a.name)
    } else {
        format!("Account {} is in the groups {}.", a.name, groups.join(", "))
    })
}

/// A number of days, hours or minutes as Empyrean writes one: a whole number without a point.
fn amount(n: f64) -> String {
    format!("{n}")
}

fn ban<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    clock: Clock,
    by: &Issuer,
    name: &str,
    [days, hours, minutes]: [f64; 3],
    reason: Option<&str>,
) -> Vec<Answer> {
    let Ok(Some(a)) = s.store.account_by_name(name) else {
        return vec![Err(format!(
            "Cannot ban \"{name}\" because that account cannot be found in database. Check \
             syntax/spelling and try again."
        ))];
    };
    for (n, what) in [(days, "Days"), (hours, "Hours"), (minutes, "Minutes")] {
        if n < 0.0 {
            return vec![Err(format!("{what} must not be less than 0."))];
        }
    }
    let seconds = days * 86_400.0 + hours * 3_600.0 + minutes * 60.0;
    #[allow(clippy::cast_possible_truncation)]
    let until = clock.unix.saturating_add(seconds.min(1e15).round() as i64);
    let reason = reason.map(str::trim).filter(|r| !r.is_empty());
    let kept = Ban {
        at: clock.unix,
        until,
        by: by.maker(),
        reason: reason.map(str::to_owned),
    };
    if let Err(e) = s.store.set_account_ban(a.id, Some(&kept)) {
        return vec![Err(format!("Account {name} was not banned: {e}"))];
    }
    let mut lines = Vec::new();
    if s.sessions.values().any(|x| x.account.id == a.id) {
        lines.push(boot(s, net, clock, BootBy::Account, &a.name, reason));
    }
    lines.push(Ok(format!(
        "Banned account {} for {} days, {} hours and {} minutes.{}",
        a.name,
        amount(days),
        amount(hours),
        amount(minutes),
        reason.map(|r| format!(" Reason: {r}")).unwrap_or_default()
    )));
    lines
}

fn unban<S: Store>(s: &mut Server<S>, name: &str) -> Answer {
    let Ok(Some(a)) = s.store.account_by_name(name) else {
        return Err(format!(
            "Cannot unban \"{name}\" because that account cannot be found in database. Check \
             spelling and try again."
        ));
    };
    if a.ban.is_none() {
        return Err(format!(
            "Cannot unban \"{name}\" because that account is not banned."
        ));
    }
    match s.store.set_account_ban(a.id, None) {
        Ok(()) => Ok(format!("UnBanned account {}.", a.name)),
        Err(e) => Err(format!("Account {name} was not unbanned: {e}")),
    }
}

fn ban_list<S: Store>(s: &mut Server<S>, clock: Clock) -> Vec<Answer> {
    let banned: Vec<Account> = match s.store.accounts() {
        Ok(all) => all
            .into_iter()
            .filter(|a| a.ban.as_ref().is_some_and(|b| clock.unix < b.until))
            .collect(),
        Err(e) => return vec![Err(format!("The accounts could not be read: {e}"))],
    };
    if banned.is_empty() {
        return vec![Ok("There are no accounts currently banned.".to_owned())];
    }
    let mut lines = vec![Ok("The following accounts are banned:".to_owned())];
    for a in banned {
        let Some(b) = a.ban else { continue };
        lines.push(Ok(format!(
            "{} -- banned by {} until {}{}",
            a.name,
            b.by,
            utc(b.until),
            b.reason
                .map(|r| format!(" -- Reason: {r}"))
                .unwrap_or_default()
        )));
    }
    lines
}

fn gag<S: Store>(s: &mut Server<S>, net: &mut ServerNet, name: &str, on: bool) -> Answer {
    let refused = if on {
        format!("Unable to gag a character named {name}, check the name and re-try the command.")
    } else {
        format!("Unable to ungag a character named {name}, check the name and re-try the command.")
    };
    let Ok(Some(c)) = s.store.character_named(name) else {
        return Err(refused);
    };
    if c.deleted_at.is_some() {
        return Err(refused);
    }
    let left = on.then_some(GAG_SECONDS);
    if let Err(e) = s.store.set_gag(c.id, left) {
        return Err(format!("{refused} ({e})"));
    }
    if let Some((world, _)) = in_world(s, c.id) {
        if on {
            s.gags.left.insert(c.id, GAG_SECONDS);
            crate::login::notice(net, world, GAG_NOTICE);
        } else if s.gags.left.remove(&c.id).is_some() {
            crate::login::notice(net, world, UNGAG_NOTICE);
        }
    }
    if on {
        Ok(format!("{} has been gagged for five minutes.", c.name))
    } else {
        Ok(format!("{} has been ungagged.", c.name))
    }
}

/// The login connections a boot names.
fn logins_named<S: Store>(s: &Server<S>, by: BootBy, who: &str) -> Result<Vec<ConnId>, String> {
    let character_login = |f: &dyn Fn(&crate::Player) -> bool| {
        s.players
            .values()
            .find(|p| f(p))
            .map(|p| p.login)
            .filter(|l| s.sessions.contains_key(l))
            .into_iter()
            .collect()
    };
    Ok(match by {
        BootBy::Character => character_login(&|p| p.name.eq_ignore_ascii_case(who)),
        BootBy::Account => {
            let mut v: Vec<ConnId> = s
                .sessions
                .iter()
                .filter(|(_, x)| x.account.name.eq_ignore_ascii_case(who))
                .map(|(c, _)| *c)
                .collect();
            v.sort_unstable();
            v
        }
        BootBy::Id => {
            let id = who
                .get(..2)
                .filter(|p| p.eq_ignore_ascii_case("0x"))
                .and_then(|_| u64::from_str_radix(&who[2..], 16).ok())
                .ok_or_else(|| {
                    "That is not a valid Instance ID (IID). IIDs are a character's id in hex, \
                     0x and sixteen digits."
                        .to_owned()
                })?;
            character_login(&|p| p.character == id)
        }
    })
}

fn boot<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    clock: Clock,
    by: BootBy,
    who: &str,
    reason: Option<&str>,
) -> Answer {
    let logins = logins_named(s, by, who)?;
    let what = by.what();
    if logins.is_empty() {
        return Err(format!(
            "Cannot boot \"{who}\" because that {what} is not currently online or cannot be \
             found. Check syntax/spelling and try again."
        ));
    }
    for login in logins {
        boot_login(s, net, login, clock);
    }
    Ok(format!(
        "Booting {what} {who}.{}",
        reason.map(|r| format!(" Reason: {r}")).unwrap_or_default()
    ))
}

/// End a login with one of the client's lines: the message that ends it, the connection closed a
/// moment later.
pub(crate) fn end_login<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    login: ConnId,
    line: u32,
    clock: Clock,
) {
    let m = ServerMessage::AccountBooted(Text(StringInfo {
        string_id: line,
        table_id: STRINGS,
        ..StringInfo::default()
    }));
    net.send(login, queue::EVENT, BlobKind::LOGIN, m.encode());
    s.after(clock, BOOT_CLOSE_AFTER, Timer::Close { conn: login });
}

/// End a login with the server's own words: the message that ends it, the connection closed a
/// moment later.
pub(crate) fn end_login_saying<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    login: ConnId,
    words: &str,
    clock: Clock,
) {
    let m = ServerMessage::AccountBooted(Text(StringInfo::literal(words)));
    net.send(login, queue::EVENT, BlobKind::LOGIN, m.encode());
    s.after(clock, BOOT_CLOSE_AFTER, Timer::Close { conn: login });
}

/// End a login: its client told its character was booted, its connection closed a moment later.
fn boot_login<S: Store>(s: &mut Server<S>, net: &mut ServerNet, login: ConnId, clock: Clock) {
    end_login(s, net, login, BOOTED, clock);
    // The character in the world goes as one whose client went away, once the login closes.
    if let Some(x) = s.sessions.get(&login) {
        if let Stage::InWorld { character, .. } = x.stage {
            tracing::info!(id = format_args!("{character:#x}"), "booted from the world");
        }
    }
}

/// A time as the operator reads it: the UTC date and time.
#[must_use]
pub fn utc(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let t = secs.rem_euclid(86_400);
    // Days since 1970 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        t / 3600,
        t % 3600 / 60,
        t % 60
    )
}

// ---- in game ----------------------------------------------------------------------------------

/// A line a character in the world said (aloud or into a room): when it is `@` and a command's
/// word, from an admin's account, the command is run and answered in its chat window, and nobody
/// hears the line; true then. From any other account the line is said as usual and counted: it was
/// an admin command, refused. The first from a character in each stay in the world is logged too,
/// since a client can send them as fast as it can talk. Anything else is said as usual.
pub(crate) fn said<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    text: &str,
    clock: Clock,
) -> bool {
    let Some(rest) = text.trim_start().strip_prefix('@') else {
        return false;
    };
    let Some(account) = s
        .players
        .get(&world)
        .and_then(|p| s.sessions.get(&p.login))
        .map(|x| x.account.clone())
    else {
        return false;
    };
    if let Some(args) = passwd_args(rest) {
        crate::login::notices(net, world, &passwd(s, world, &account, &args, clock));
        return true;
    }
    let Some(parsed) = parse(rest) else {
        return false;
    };
    let character = s
        .players
        .get(&world)
        .map(|p| p.name.clone())
        .unwrap_or_default();
    let by = Issuer::Account {
        name: account.name,
        admin: account.admin,
        groups: account.groups,
        character,
    };
    if !by.is_admin() {
        s.admin_calls.refused_lines += 1;
        let first = s
            .players
            .get_mut(&world)
            .is_some_and(|p| !std::mem::replace(&mut p.refused_admin_line, true));
        if first {
            s.admin_calls.logged_lines += 1;
            // The first word alone: what follows may hold a secret.
            let word = rest.split_whitespace().next().unwrap_or_default();
            tracing::warn!(
                issuer = %by,
                word,
                "refused: not an admin's account (any more from it this stay are only counted)"
            );
        }
        return false;
    }
    let lines = match parsed {
        Ok(c) => run(s, net, clock, &by, &c),
        Err(usage) => {
            tracing::info!(issuer = %by, "admin: a command that does not read: {usage}");
            vec![usage]
        }
    };
    crate::login::notices(net, world, &lines);
    true
}

// ---- a player's own password ---------------------------------------------------------------------

/// How long after one `@passwd` the next may be given, in seconds (Empyrean's).
pub const PASSWD_WAIT: f64 = 5.0;

/// The words after `passwd` (in any case), when a line is that command.
fn passwd_args(line: &str) -> Option<Vec<String>> {
    let w = words(line);
    let (first, rest) = w.split_first()?;
    (!first.quoted && first.text.eq_ignore_ascii_case("passwd"))
        .then(|| rest.iter().map(|w| w.text.clone()).collect())
}

/// `@passwd <old> <new>`: a character's account changes its own password, as Empyrean's command
/// does. Fewer than two words is answered with the count and the command's use; a second one
/// within [`PASSWD_WAIT`] of the last (whatever came of it) is refused; an old password that does
/// not match is refused; otherwise the new one is kept as a bcrypt secret. The lines to answer
/// with; no password is ever written out.
pub fn passwd<S: Store>(
    s: &mut Server<S>,
    world: ConnId,
    account: &Account,
    args: &[String],
    clock: Clock,
) -> Vec<String> {
    let [old, new, ..] = args else {
        return vec![
            format!("Invalid parameter count, got {}, expected 2!", args.len()),
            "@passwd - Change your account password.".to_owned(),
            "Usage: @passwd oldpassword newpassword".to_owned(),
        ];
    };
    let now = clock.local.0;
    let Some(p) = s.players.get_mut(&world) else {
        return Vec::new();
    };
    if p.passwd_at.is_some_and(|t| now - t < PASSWD_WAIT) {
        return vec![format!(
            "This command may only be run once every {PASSWD_WAIT} seconds."
        )];
    }
    p.passwd_at = Some(now);
    tracing::info!(character = %p.name, "changing its account's password");
    let Ok(Some(a)) = s.store.account_by_name(&account.name) else {
        return vec![format!(
            "Account {} ({}) wasn't found in the database! How are you in world without a valid \
             account?",
            account.name, account.id
        )];
    };
    let matches = a
        .secret
        .as_deref()
        .is_some_and(|k| crate::login::ticket_matches(k, old.as_bytes()));
    if !matches {
        return vec![
            "Unable to change password: Password provided in first parameter does not match \
             current account password for this account!"
                .to_owned(),
        ];
    }
    let Some(secret) = crate::login::secret_for(new.as_bytes(), s.cfg.secret_cost) else {
        return vec!["Account password was not changed: no secret could be made.".to_owned()];
    };
    match s.store.set_account_secret(a.id, secret) {
        Ok(()) => vec!["Account password successfully changed.".to_owned()],
        Err(e) => vec![format!("Account password was not changed: {e}")],
    }
}
