//! Playing the world: the editor's own Yalain, started on this machine's loopback address in the
//! world's playtest folder and serving the world as the editor holds it, and a client started into
//! it as one of the world's playtest characters ([`crate::characters`]).
//!
//! **The playtest folder** is `.playtest` in the world's folder, which neither the world's reader
//! nor the server reads as part of the world: `world/`, the world's files as the editor holds them,
//! saved or not, which the server serves over the world's base; `yalain.sqlite`, the one store
//! every playtest character lives in, each on an account of its own ([`account_of`]), so one
//! server plays them all and a character leaving the world never holds up another entering it;
//! `characters/`, each character's recipe as it was last made or given and the server's recipe it
//! was given from; the server's log (and the one before it); and the client's log, settings and
//! screenshots. One editor at a time plays a world: a second is refused while the first holds the
//! folder's lock.
//!
//! **Live.** Each change made while the server runs is published: the changed files are written
//! into `world/` and the server is told `reload` at its console. It compiles the world again and
//! takes it at once with players in the world (templates, loot, stores, quests, what people say,
//! the places maps name, the placed people and objects, the camps, the veins, and more).
//! [`Live::restart`] starts it again when asked: the server is stopped as its operator stops it,
//! keeping every character where it stands, and started again, and a client the editor started
//! enters again where its character stood.
//!
//! **The console.** What the editor wants of a character is asked at the server's console, one
//! command at a time, in turn, and its answer read from the log by the tag that ends it
//! ([`said`]): made from its recipe (`make-character`, deleted at once first so a name is always
//! free), given the part of its build that changed (`apply-build`, which keeps what it found and
//! learned), put where it enters next (`place`), made a hero or no longer one, sent through portal
//! space (`teleport`), written to a file for another world (`export-character`), or brought from
//! one (`import-character`). A character is given its build or a place only out of the world: a
//! step for one still in it waits until the log says it left. A character whose client went (the
//! editor ended it, or it was closed) is taken out of the world at the console (`forcelogoff`)
//! before anything is asked for it, so it goes at once rather than lingering until the server
//! hears its client has fallen silent. A client waiting to play starts once the steps before it
//! are done; a step refused drops the steps after it and the client.
//!
//! **Ports.** The server listens on 127.0.0.1 alone, on the first ports no program holds, looked
//! for from [`Programs::port`] moved on by a step of this process's own ([`first_port`]), so two
//! editors or test runs on one machine seldom look at the same ports first. A restart looks past
//! the ports it just let go. Another program may still take the ports between that look and the
//! server's start: the server then says so (`[ports-taken]`) and ends with its own exit code
//! ([`PORTS_TAKEN`]), and is started again on the next free ones ([`START_TRIES`] in all). Our
//! client reaches it from a loopback address: nothing is open to another computer, and no firewall
//! asks. A client that is Auberean logs in by itself as the character; the retail client is told
//! the server, account and ticket as the launcher tells it.
//!
//! **Stopping** is the server's own clean stop, `stop` at its console ([`crate::stop`]), after
//! which it keeps every character still in the world and ends; one that has not ended within
//! [`STOP_WAIT`] is ended. It is started to stop by itself, keeping everyone, when its console's
//! input ends: an editor that goes away without stopping it (a crash, a kill) never leaves it
//! holding the ports.

use std::collections::{BTreeSet, VecDeque};
use std::fs::File;
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::net::{Ipv4Addr, TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use auberean_dat::locate::DatName;
use auberean_dat::overlay::{WorldOverlay, WORLD_FOLDER};
use auberean_dat::DatFile;
use yalain_world_db::rows::Clearing;
use yalain_world_db::Files;

use crate::characters::{self, Making, Recipe};
use crate::stop;

/// The ticket every playtest character's account logs in with.
pub const TICKET: &str = "athenaeum";

/// The playtest folder, in the world's folder.
pub const FOLDER: &str = ".playtest";

/// The store every playtest character lives in, in the playtest folder.
pub const STORE: &str = "yalain.sqlite";

/// How long a server asked to stop is given before it is ended: a guard against a hang, far past
/// what keeping a playtest's characters takes.
pub const STOP_WAIT: Duration = Duration::from_secs(30);

/// How long a command asked at the console waits for its answer before the next is asked: a
/// guard against a line the server never answers, far past what an answer takes.
pub const ASK_WAIT: Duration = Duration::from_secs(60);

/// How long a step waits for its character to leave the world before it is asked all the same
/// (and the server says why not): a guard, far past the seconds a character lingers.
pub const LEAVE_WAIT: Duration = Duration::from_secs(90);

/// How many times the server is started on ports another program took meanwhile, in all.
pub const START_TRIES: u32 = 12;

/// The exit code of a server that could not take its ports because another program holds one.
pub const PORTS_TAKEN: i32 = 3;

/// How many first looks for ports there are, one a process by its id ([`first_port`]).
pub const PROCESS_SLOTS: u32 = 64;

/// How far apart two processes' first looks for ports are: a login port and the world's two above
/// it, with room.
pub const PROCESS_STEP: u16 = 8;

/// The login port process `process` looks from first, for the ports from `from`: moved on by a
/// step of its own, one of [`PROCESS_SLOTS`], so editors and test runs started together on one
/// machine seldom look at the same ports first. Even, as [`free_port`] takes them.
#[must_use]
pub fn first_port(from: u16, process: u32) -> u16 {
    let slot = u16::try_from(process % PROCESS_SLOTS).unwrap_or(0);
    (from & !1).saturating_add(slot * PROCESS_STEP)
}

/// The account a playtest character is played from: one of its own, its name in small letters
/// with `_` for each space (a name never holds `_`, so no two characters share one).
#[must_use]
pub fn account_of(name: &str) -> String {
    name.to_ascii_lowercase().replace(' ', "_")
}

/// What starting the server and a client needs.
#[derive(Debug, Clone, Default)]
pub struct Programs {
    /// The server.
    pub yalain: Option<PathBuf>,
    /// The install the server reads (a copy no client runs from).
    pub server_dats: Option<PathBuf>,
    /// Our client, and the install it reads.
    pub client: Option<PathBuf>,
    pub client_dats: Option<PathBuf>,
    /// The retail client, run from its own folder.
    pub retail: Option<PathBuf>,
    /// More arguments for whichever client starts.
    pub client_args: Vec<String>,
    /// The login port the look for free ones begins from, moved on by this process's own step
    /// ([`first_port`]); the world's is two above.
    pub port: u16,
}

/// A program beside this one, when there is one.
#[must_use]
pub fn beside_me(name: &str) -> Option<PathBuf> {
    let me = std::env::current_exe().ok()?;
    let p = me.with_file_name(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    p.is_file().then_some(p)
}

/// Where the player enters: a cell, a point in its landblock, a heading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry {
    pub cell: u32,
    pub at: [f32; 3],
    pub heading: f32,
}

impl Entry {
    /// As the server's recipe and its console write a place: the cell, the point, the heading.
    #[must_use]
    pub fn place(&self) -> (u32, [f32; 3], f32) {
        (self.cell, self.at, self.heading)
    }

    /// As the console writes it after `to`: the cell in hexadecimal, the point, the heading.
    fn words(&self) -> String {
        format!(
            "{:08X} {} {} {} {}",
            self.cell, self.at[0], self.at[1], self.at[2], self.heading
        )
    }
}

/// What one start of the server is told.
#[derive(Debug, Clone)]
pub struct Start<'a> {
    /// The world it serves, and the base it lies over.
    pub world: &'a Path,
    pub base: Option<&'a Path>,
    /// The store of every playtest character.
    pub store: &'a Path,
    pub port: u16,
}

/// The server's arguments for a start: the world over its base, the playtest store, loopback on
/// `port` and the three above it, and a stop of its own when its console's input ends.
#[must_use]
pub fn server_args(dats: &Path, s: &Start<'_>) -> Vec<String> {
    let mut a = vec![
        "--dats".to_string(),
        dats.display().to_string(),
        "--world-db".into(),
        s.world.display().to_string(),
    ];
    if let Some(b) = s.base {
        a.extend(["--world-base".into(), b.display().to_string()]);
    }
    a.extend([
        "--data".into(),
        s.store.display().to_string(),
        "--bind".into(),
        "127.0.0.1".into(),
        "--login-port".into(),
        s.port.to_string(),
        "--world-port".into(),
        (s.port + 2).to_string(),
        "--world-name".into(),
        "Athenaeum".into(),
        "--stop-at-console-end".into(),
    ]);
    a
}

/// A name or a file's path as the console reads it whole: between double quotes.
fn quoted(s: &str) -> String {
    format!("\"{s}\"")
}

/// The console's line sending character `name`, in the world, to `to`.
#[must_use]
pub fn teleport_line(name: &str, to: &Entry) -> String {
    format!("teleport {} to {}", quoted(name), to.words())
}

/// The console's line putting character `name`, out of the world, at `to` for its next entry.
#[must_use]
pub fn place_line(name: &str, to: &Entry) -> String {
    format!("place {} to {}", quoted(name), to.words())
}

/// The console's line making character `name` a hero, or no longer one.
#[must_use]
pub fn hero_line(name: &str, on: bool) -> String {
    if on {
        format!("hero {}", quoted(name))
    } else {
        format!("hero {} off", quoted(name))
    }
}

/// The console's line making the character a server's recipe file describes.
#[must_use]
pub fn make_line(file: &Path) -> String {
    format!("make-character {}", quoted(&file.display().to_string()))
}

/// The console's line giving the build a server's recipe file describes to its character.
#[must_use]
pub fn build_line(file: &Path) -> String {
    format!("apply-build {}", quoted(&file.display().to_string()))
}

/// The console's line taking character `name` out of the world at once, kept where it stands,
/// its login ended.
#[must_use]
pub fn logoff_line(name: &str) -> String {
    format!("forcelogoff {}", quoted(name))
}

/// The console's line deleting character `name` at once, its name free again straight away.
#[must_use]
pub fn delete_line(name: &str) -> String {
    format!("deletecharacter {} now", quoted(name))
}

/// The console's line writing character `name` to `file`.
#[must_use]
pub fn export_line(name: &str, file: &Path) -> String {
    format!(
        "export-character {} {}",
        quoted(name),
        quoted(&file.display().to_string())
    )
}

/// The console's line bringing the character in `file` in as `name`, on that name's account.
#[must_use]
pub fn import_line(file: &Path, name: &str) -> String {
    format!(
        "import-character {} {} {}",
        quoted(&file.display().to_string()),
        account_of(name),
        quoted(name)
    )
}

/// Whether the server's ports from `login` on are free on loopback: the login's and the world's
/// (each with the port above it) for datagrams, and the login's for the friends service's stream.
fn ports_free(login: u16) -> bool {
    let udp: Vec<_> = (0..4)
        .map_while(|i| UdpSocket::bind((Ipv4Addr::LOCALHOST, login + i)).ok())
        .collect();
    udp.len() == 4 && TcpListener::bind((Ipv4Addr::LOCALHOST, login)).is_ok()
}

/// The first even login port from `from` on whose ports no program holds.
///
/// # Errors
/// None free among the next thousand.
pub fn free_port(from: u16) -> Result<u16, String> {
    let first = from.max(1024) & !1;
    (0..500)
        .map(|i| first.saturating_add(i * 2))
        .take_while(|p| *p < u16::MAX - 4)
        .find(|p| ports_free(*p))
        .ok_or_else(|| format!("no free ports on 127.0.0.1 from {first} on"))
}

/// Write the world's `files` into `dir` where they differ from what was `written` there before,
/// and take away those it no longer has. Answers how many it wrote or took away.
///
/// # Errors
/// A file that will not write.
pub fn write_world(dir: &Path, files: &Files, written: &mut Files) -> Result<usize, String> {
    let mut n = 0;
    for (rel, text) in files {
        if written.get(rel) == Some(text) {
            continue;
        }
        let p = dir.join(rel);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))?;
        written.insert(rel.clone(), text.clone());
        n += 1;
    }
    let gone: Vec<String> = written
        .keys()
        .filter(|k| !files.contains_key(*k))
        .cloned()
        .collect();
    for rel in gone {
        let p = dir.join(&rel);
        if p.exists() {
            std::fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        }
        written.remove(&rel);
        n += 1;
    }
    Ok(n)
}

/// What a line of the server's log says in the fixed form programs read: the tag that ends it, and
/// the words before the tag (for people, and free to change).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    /// A command did what the words say: the command's own name.
    Done {
        command: String,
        words: String,
    },
    /// A command did nothing, for the reason the words give.
    Refused {
        command: String,
        words: String,
    },
    /// A character entered the world, or left it: its name as the world shows it.
    Entered(String),
    Left(String),
    /// The server listens for clients and its console.
    Listening,
    /// Another program holds a port the server listens on: it ends.
    PortsTaken,
}

/// What the server itself said on a line of its log (a line from `yalain`, after its time and
/// level), never one of the world's crates.
fn from_server(line: &str) -> Option<&str> {
    let line = line.trim_end();
    let (_, rest) = line.split_once(char::is_whitespace)?;
    let (level, rest) = rest.trim_start().split_once(' ')?;
    if !matches!(level, "TRACE" | "DEBUG" | "INFO" | "WARN" | "ERROR") {
        return None;
    }
    rest.strip_prefix("yalain: ")
}

/// A tag's character name and id: `"<name>" 0x<id>`.
fn named(rest: &str) -> Option<String> {
    let inner = rest.strip_prefix('"')?;
    let end = inner.rfind('"')?;
    let id = inner[end + 1..].trim();
    id.strip_prefix("0x")
        .filter(|h| !h.is_empty() && h.chars().all(|c| c.is_ascii_hexdigit()))?;
    Some(inner[..end].to_string())
}

/// What a line of the server's log says, when it ends in one of the tags programs read: the line's
/// last ` [` to the `]` that ends it, on a line from the server itself.
#[must_use]
pub fn said(line: &str) -> Option<Said> {
    let text = from_server(line)?;
    let inner = text.strip_suffix(']')?;
    let at = inner.rfind(" [")?;
    let (words, tag) = (inner[..at].to_string(), &inner[at + 2..]);
    match tag {
        "listening" => return Some(Said::Listening),
        "ports-taken" => return Some(Said::PortsTaken),
        _ => {}
    }
    if let Some(command) = tag.strip_prefix("ok ") {
        return Some(Said::Done {
            command: command.to_string(),
            words,
        });
    }
    if let Some(command) = tag.strip_prefix("refused ") {
        return Some(Said::Refused {
            command: command.to_string(),
            words,
        });
    }
    if let Some(rest) = tag.strip_prefix("entered ") {
        return named(rest).map(Said::Entered);
    }
    tag.strip_prefix("left ").and_then(named).map(Said::Left)
}

/// Whether a server that ended could not take its ports (another program holds one): its exit
/// code says so ([`PORTS_TAKEN`]), or, when the code is not known, its log's `[ports-taken]`.
#[must_use]
pub fn ports_taken(code: Option<i32>, log: &str) -> bool {
    match code {
        Some(c) => c == PORTS_TAKEN,
        None => log.lines().any(|l| said(l) == Some(Said::PortsTaken)),
    }
}

/// What the server answered a `reload`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reloaded {
    /// It compiled: the tables taken at once, and those changed that wait for a restart (each
    /// listed when the answer's words name them).
    Taken {
        applied: Vec<String>,
        restart: Vec<String>,
    },
    /// It did not compile, and the world is as it was: why.
    Refused(String),
}

/// A `reload`'s answer from what the server said of it.
#[must_use]
pub fn reloaded(done: bool, words: &str) -> Reloaded {
    if !done {
        const UNCHANGED: &str = "the world is unchanged: ";
        let why = words
            .find(UNCHANGED)
            .map_or(words, |i| &words[i + UNCHANGED.len()..]);
        return Reloaded::Refused(why.trim().to_string());
    }
    let list = |s: &str| -> Vec<String> {
        let s = s.trim();
        if s == "nothing" {
            Vec::new()
        } else {
            s.split(", ").map(str::to_string).collect()
        }
    };
    let lists = words.find(": reloaded: ").and_then(|i| {
        let rest = &words[i + ": reloaded: ".len()..];
        let (applied, rest) = rest.split_once("; needs a restart: ")?;
        let (restart, _) = rest.split_once("; unchanged: ")?;
        Some((list(applied), list(restart)))
    });
    let (applied, restart) = lists.unwrap_or_default();
    Reloaded::Taken { applied, restart }
}

/// A console command whose answer the editor waits for, with the character or file it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ask {
    /// `make-character`: made from its recipe.
    Make(String),
    /// `apply-build`: given what changed of its build.
    Build(String),
    /// `place`: put where it enters next.
    Place(String),
    /// `hero`: made a hero or no longer one.
    Hero(String),
    /// `deletecharacter ... now`: deleted at once.
    Delete(String),
    /// `forcelogoff`: taken out of the world at once (refused: it is not in the world).
    Logoff(String),
    /// `teleport`: sent through portal space.
    Teleport(String),
    /// `export-character`: written to this file.
    Export(PathBuf),
    /// `import-character`: brought in under this name.
    Import(String),
}

impl Ask {
    /// The command's own name, as its answer's tag names it.
    #[must_use]
    pub fn command(&self) -> &'static str {
        match self {
            Self::Make(_) => "make-character",
            Self::Build(_) => "apply-build",
            Self::Place(_) => "place",
            Self::Hero(_) => "hero",
            Self::Delete(_) => "deletecharacter",
            Self::Logoff(_) => "forcelogoff",
            Self::Teleport(_) => "teleport",
            Self::Export(_) => "export-character",
            Self::Import(_) => "import-character",
        }
    }
}

/// A command waiting its turn at the console.
#[derive(Debug, Clone)]
struct Step {
    ask: Ask,
    line: String,
    /// Asked once this character is out of the world.
    leaves: Option<String>,
    /// A refusal that is no failure: the steps after it go on (a delete of a name the store may
    /// not hold).
    may_refuse: bool,
    /// Written down as made from this recipe once it is done.
    mark: Option<Recipe>,
}

impl Step {
    fn new(ask: Ask, line: String) -> Self {
        Self {
            ask,
            line,
            leaves: None,
            may_refuse: false,
            mark: None,
        }
    }

    fn once_out(mut self, name: &str) -> Self {
        self.leaves = Some(name.to_string());
        self
    }

    fn marking(mut self, r: &Recipe) -> Self {
        self.mark = Some(r.clone());
        self
    }
}

/// The playtest folder's lock, held while the server runs: a second editor on the same world is
/// refused.
///
/// # Errors
/// The folder will not make, or another editor holds it.
pub fn lock(folder: &Path) -> Result<File, String> {
    std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(folder.join("playtest.lock"))
        .map_err(|e| e.to_string())?;
    f.try_lock().map_err(|_| {
        format!(
            "another editor is playing this world (its playtest folder {} is in use)",
            folder.display()
        )
    })?;
    Ok(f)
}

/// Where a playtest of a world is kept, and what it lies over.
#[derive(Debug, Clone)]
pub struct Setup {
    pub programs: Programs,
    /// The playtest folder.
    pub folder: PathBuf,
    /// The world's base, when it lies over one.
    pub base: Option<PathBuf>,
    /// The world's key, and its clearings with the landblock each is written in.
    pub key: String,
    pub clearings: Vec<(Clearing, u16)>,
}

impl Setup {
    /// Make the overlay of the world the server serves, over the install it reads, when it changes
    /// or changed any of its records.
    ///
    /// # Errors
    /// A clearing the install has no landblock for, or the overlay not writing.
    pub fn write_overlay(&self) -> Result<(), String> {
        let Some(dats) = &self.programs.server_dats else {
            return Ok(());
        };
        let rows: Vec<(&Clearing, u16)> = self.clearings.iter().map(|(c, l)| (c, *l)).collect();
        athenaeum_foundry::overlay::write_rows(&rows, &self.key, dats, &self.world())?;
        Ok(())
    }

    /// The overlay our client reads over the install `dats`: the server's when it lies over those
    /// files, else the same clearings made over them; none for a world that changes no record.
    ///
    /// # Errors
    /// The overlay not reading or not writing.
    fn client_overlay(&self, dats: &Path) -> Result<Option<PathBuf>, String> {
        let world = self.world();
        let Some(overlay) = WorldOverlay::of_world(&world).map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        let cell = DatName::Cell(1).in_dir(dats);
        let file = DatFile::open(&cell).map_err(|e| format!("{}: {e}", cell.display()))?;
        if overlay.over(file).is_ok() {
            return Ok(Some(world.join(WORLD_FOLDER)));
        }
        let own = self.folder.join("client");
        let rows: Vec<(&Clearing, u16)> = self.clearings.iter().map(|(c, l)| (c, *l)).collect();
        let (folder, _) = athenaeum_foundry::overlay::write_rows(&rows, &self.key, dats, &own)?;
        Ok(Some(folder))
    }

    /// The world the server serves.
    #[must_use]
    pub fn world(&self) -> PathBuf {
        self.folder.join("world")
    }

    /// The store every playtest character lives in.
    #[must_use]
    pub fn store(&self) -> PathBuf {
        self.folder.join(STORE)
    }

    /// The server's log.
    #[must_use]
    pub fn log(&self) -> PathBuf {
        self.folder.join("yalain.log")
    }

    /// The log of the server that ran before it (a restart's first half, or a start on ports
    /// another program took).
    #[must_use]
    pub fn previous_log(&self) -> PathBuf {
        self.folder.join("yalain-previous.log")
    }
}

/// The server running.
#[derive(Debug)]
struct Server {
    child: Child,
    console: ChildStdin,
    port: u16,
    /// How many starts it took, the ports of each before it taken by another program.
    tries: u32,
    /// How far its log has been read, in bytes: up to the end of its last whole line.
    read: u64,
    /// Whether it has said it listens.
    up: bool,
    /// The characters in the world, by their names in small letters.
    in_world: BTreeSet<String>,
    /// Whether a `reload` waits for its answer.
    reloading: bool,
    /// The command asked and not yet answered, and when.
    asked: Option<(Step, Instant)>,
    /// When the step first in line began waiting for its character to leave the world.
    waiting: Option<Instant>,
}

/// What happened since the last [`Live::tick`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The server listens.
    Up,
    /// The server could not take its ports (another program took them): it starts again on these.
    Moved(u16),
    /// A client started.
    Playing,
    /// The server answered a `reload`.
    Reloaded(Reloaded),
    /// The server answered a command asked at its console: what it did, or why not.
    Answered(Ask, Result<String, String>),
    /// The server stopped by itself: the log's last line.
    Stopped(String),
}

/// The editor's own server and the client it started.
#[derive(Debug, Default)]
pub struct Live {
    lock: Option<File>,
    server: Option<Server>,
    client: Option<Child>,
    /// The character the client plays, while one runs, and whether it is the retail client.
    playing_as: Option<String>,
    retail: bool,
    /// Commands to ask at the console in turn.
    steps: VecDeque<Step>,
    /// A client to start once the server listens and the steps before it are done: its character,
    /// and whether it is the retail client.
    pending: Option<(String, bool)>,
    /// The world as last written into the playtest folder.
    written: Files,
    /// What the last start said, for the status line.
    pub said: String,
}

/// Type `line` at a server's console.
fn type_line(console: &mut ChildStdin, line: &str) -> Result<(), String> {
    writeln!(console, "{line}")
        .and_then(|()| console.flush())
        .map_err(|e| format!("the server's console: {e}"))
}

fn kill(c: &mut Option<Child>) {
    if let Some(mut c) = c.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

/// The server's log from byte `from` on, up to the end of its last whole line (a line it is
/// writing waits for the next read), and where that end is.
fn whole_lines_from(setup: &Setup, from: u64) -> (String, u64) {
    let mut bytes = Vec::new();
    if let Ok(mut f) = File::open(setup.log()) {
        if f.seek(SeekFrom::Start(from)).is_ok() {
            let _ = f.read_to_end(&mut bytes);
        }
    }
    let end = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    bytes.truncate(end);
    let to = from + u64::try_from(end).unwrap_or(0);
    (String::from_utf8_lossy(&bytes).into_owned(), to)
}

impl Live {
    /// Whether the server runs.
    pub fn serving(&mut self) -> bool {
        match &mut self.server {
            Some(s) => matches!(s.child.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// Whether the server has said it listens.
    #[must_use]
    pub fn up(&self) -> bool {
        self.server.as_ref().is_some_and(|s| s.up)
    }

    /// The login port, while the server runs.
    #[must_use]
    pub fn port(&self) -> Option<u16> {
        self.server.as_ref().map(|s| s.port)
    }

    /// Whether a client the editor started runs.
    pub fn playing(&mut self) -> bool {
        let running = match &mut self.client {
            Some(c) => matches!(c.try_wait(), Ok(None)),
            None => false,
        };
        if !running {
            self.playing_as = None;
        }
        running
    }

    /// The character the client the editor started plays, while it runs.
    pub fn playing_as(&mut self) -> Option<String> {
        if self.playing() {
            self.playing_as.clone()
        } else {
            None
        }
    }

    /// Whether the client the editor started last is the retail one.
    #[must_use]
    pub fn retail(&self) -> bool {
        self.retail
    }

    /// Whether the server's log says character `name` is in the world.
    #[must_use]
    pub fn in_world(&self, name: &str) -> bool {
        self.server
            .as_ref()
            .is_some_and(|s| s.in_world.contains(&name.to_ascii_lowercase()))
    }

    /// Whether a `reload` waits for its answer.
    #[must_use]
    pub fn reloading(&self) -> bool {
        self.server.as_ref().is_some_and(|s| s.reloading)
    }

    /// Whether a command asked at the console, or one waiting its turn, is unanswered.
    #[must_use]
    pub fn asking(&self) -> bool {
        !self.steps.is_empty() || self.server.as_ref().is_some_and(|s| s.asked.is_some())
    }

    /// Start the server on the world's `files`, on the first free ports from this process's first
    /// look ([`first_port`] of the setup's), unless it runs already. Its characters are asked for
    /// afterwards ([`Live::prepare`]).
    ///
    /// # Errors
    /// No server program or install, the folder in use or not writable, no free ports, or the
    /// server does not start.
    pub fn start(&mut self, setup: &Setup, files: &Files) -> Result<(), String> {
        if self.serving() {
            return Ok(());
        }
        let port = free_port(first_port(setup.programs.port, std::process::id()))?;
        self.start_at(setup, files, port)
    }

    /// Start the server as [`Live::start`] does, on `port` without looking whether it is free:
    /// held by another program, the server says so and is started again on the next free ones.
    ///
    /// # Errors
    /// As [`Live::start`].
    pub fn start_at(&mut self, setup: &Setup, files: &Files, port: u16) -> Result<(), String> {
        self.stop_server();
        if self.lock.is_none() {
            self.lock = Some(lock(&setup.folder)?);
        }
        let world = setup.world();
        let _ = std::fs::remove_dir_all(&world);
        self.written.clear();
        write_world(&world, files, &mut self.written)?;
        setup.write_overlay()?;
        characters::forget_unstored(&setup.folder)?;
        self.spawn(setup, port, 1)
    }

    /// Start the server process on `port`, its log begun anew (the one before kept).
    fn spawn(&mut self, setup: &Setup, port: u16, tries: u32) -> Result<(), String> {
        let yalain = setup
            .programs
            .yalain
            .clone()
            .ok_or("no server program: give --yalain <yalain.exe>")?;
        let dats = setup
            .programs
            .server_dats
            .clone()
            .ok_or("no install for the server: give --server-dats <install>")?;
        let (world, store) = (setup.world(), setup.store());
        let args = server_args(
            &dats,
            &Start {
                world: &world,
                base: setup.base.as_deref(),
                store: &store,
                port,
            },
        );
        if setup.log().exists() {
            let _ = std::fs::rename(setup.log(), setup.previous_log());
        }
        let log =
            File::create(setup.log()).map_err(|e| format!("{}: {e}", setup.log().display()))?;
        let err = log.try_clone().map_err(|e| e.to_string())?;
        let mut cmd = Command::new(&yalain);
        cmd.args(&args)
            .current_dir(&setup.folder)
            .stdin(Stdio::piped())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(err));
        stop::windowless(&mut cmd);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("{}: {e}", yalain.display()))?;
        let console = child.stdin.take().ok_or("the server's console")?;
        self.said = format!("Yalain on 127.0.0.1:{port} (log {})", setup.log().display());
        self.server = Some(Server {
            child,
            console,
            port,
            tries,
            read: 0,
            up: false,
            in_world: BTreeSet::new(),
            reloading: false,
            asked: None,
            waiting: None,
        });
        Ok(())
    }

    /// Ask the server for `recipe`'s character as it plays next, entering at `entry` (`None`:
    /// where it last left the world): made when it has not been (or was made of another body),
    /// given the part of its build that changed since it was made, made a hero or no longer one,
    /// and put at `entry`. Each is asked once the server listens, and once the character is out of
    /// the world: one no client of the editor's plays is taken out of it first. The character's
    /// server recipe is written into its folder.
    ///
    /// # Errors
    /// Its folder or server recipe will not write.
    pub fn prepare(
        &mut self,
        setup: &Setup,
        recipe: &Recipe,
        entry: Option<Entry>,
    ) -> Result<(), String> {
        let name = &recipe.name;
        self.logoff_if_left(name);
        let place = entry.map(|e| e.place());
        let file = characters::server_recipe(&setup.folder, name);
        let write = |text: String| -> Result<(), String> {
            let dir = characters::made_dir(&setup.folder, name);
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            std::fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))
        };
        match characters::making(&setup.folder, recipe) {
            Making::Make => {
                write(recipe.server_text(place))?;
                self.steps.push_back(Step {
                    may_refuse: true,
                    ..Step::new(Ask::Delete(name.clone()), delete_line(name))
                });
                self.steps.push_back(
                    Step::new(Ask::Make(name.clone()), make_line(&file))
                        .once_out(name)
                        .marking(recipe),
                );
            }
            Making::Build(made) => {
                write(recipe.changes_text(&made, place))?;
                self.steps.push_back(
                    Step::new(Ask::Build(name.clone()), build_line(&file))
                        .once_out(name)
                        .marking(recipe),
                );
            }
            Making::Same(made) => {
                if made.hero != recipe.hero {
                    self.steps.push_back(
                        Step::new(Ask::Hero(name.clone()), hero_line(name, recipe.hero))
                            .marking(recipe),
                    );
                }
                if let Some(to) = entry {
                    self.steps.push_back(
                        Step::new(Ask::Place(name.clone()), place_line(name, &to)).once_out(name),
                    );
                }
            }
        }
        Ok(())
    }

    /// Ask the server to take character `name` out of the world at once when the log has it in
    /// the world and no client of the editor's plays it (and it is not asked already). A refusal
    /// (it left meanwhile) is no failure.
    fn logoff_if_left(&mut self, name: &str) {
        let ask = Ask::Logoff(name.to_string());
        let asked = self.steps.iter().any(|s| s.ask == ask);
        let ours = self.playing_as.as_deref() == Some(name) && self.client.is_some();
        if self.in_world(name) && !ours && !asked {
            self.steps.push_back(Step {
                may_refuse: true,
                ..Step::new(ask, logoff_line(name))
            });
        }
    }

    /// Ask the server to make `recipe`'s character anew from its recipe, entering at `entry`:
    /// deleted at once (taken out of the world first when it is in it), then made.
    ///
    /// # Errors
    /// As [`Live::prepare`].
    pub fn reset(
        &mut self,
        setup: &Setup,
        recipe: &Recipe,
        entry: Option<Entry>,
    ) -> Result<(), String> {
        characters::unmark(&setup.folder, &recipe.name)?;
        self.prepare(setup, recipe, entry)
    }

    /// Ask the server to delete character `name` at once (one it does not hold is no matter).
    pub fn delete(&mut self, name: &str) {
        self.steps.push_back(Step {
            may_refuse: true,
            ..Step::new(Ask::Delete(name.to_string()), delete_line(name))
        });
    }

    /// Ask the server to bring in the character in `file` (another world's export) as `recipe`'s,
    /// on its own account: a character of that name in the store is deleted first. Written down as
    /// made from `recipe` once the server has brought it in.
    pub fn import(&mut self, recipe: &Recipe, file: &Path) {
        self.delete(&recipe.name);
        self.steps.push_back(
            Step::new(
                Ask::Import(recipe.name.clone()),
                import_line(file, &recipe.name),
            )
            .marking(recipe),
        );
    }

    /// Ask the server to send character `name`, in the world, through portal space to `to`.
    pub fn teleport(&mut self, name: &str, to: Entry) {
        self.steps.push_back(Step::new(
            Ask::Teleport(name.to_string()),
            teleport_line(name, &to),
        ));
    }

    /// Ask the server to write character `name` to `file`, in place of one written there before.
    ///
    /// # Errors
    /// The folder will not make, or an earlier file will not go.
    pub fn export(&mut self, name: &str, file: &Path) -> Result<(), String> {
        if let Some(d) = file.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        if file.exists() {
            // The server writes a new file, never over another.
            std::fs::remove_file(file).map_err(|e| format!("{}: {e}", file.display()))?;
        }
        self.steps.push_back(Step::new(
            Ask::Export(file.to_path_buf()),
            export_line(name, file),
        ));
        Ok(())
    }

    /// Start a client as character `name` once the server listens and the steps asked before
    /// this are done.
    pub fn play_when_ready(&mut self, name: &str, retail: bool) {
        self.pending = Some((name.to_string(), retail));
    }

    /// Type a line at the server's console.
    ///
    /// # Errors
    /// No server, or its console closed.
    pub fn command(&mut self, line: &str) -> Result<(), String> {
        let s = self.server.as_mut().ok_or("no server runs")?;
        type_line(&mut s.console, line)
    }

    /// Each frame: what the server's log said since (it listens, characters entered and left, a
    /// `reload` or a command answered, characters written down as made), the server started again
    /// on other ports when another program took its own, the next command asked, a client waiting
    /// for the steps before it started, or the server stopped by itself.
    ///
    /// # Errors
    /// A character's record that will not write, or the client does not start.
    pub fn tick(&mut self, setup: &Setup) -> Result<Vec<Event>, String> {
        let mut events = Vec::new();
        let Some(s) = &mut self.server else {
            return Ok(events);
        };
        let (text, to) = whole_lines_from(setup, s.read);
        s.read = to;
        for line in text.lines() {
            let (done, command, words) = match said(line) {
                None => continue,
                Some(Said::Listening) => {
                    if !s.up {
                        s.up = true;
                        events.push(Event::Up);
                    }
                    continue;
                }
                // Read with its exit, below.
                Some(Said::PortsTaken) => continue,
                Some(Said::Entered(n)) => {
                    s.in_world.insert(n.to_ascii_lowercase());
                    continue;
                }
                Some(Said::Left(n)) => {
                    s.in_world.remove(&n.to_ascii_lowercase());
                    continue;
                }
                Some(Said::Done { command, words }) => (true, command, words),
                Some(Said::Refused { command, words }) => (false, command, words),
            };
            if command == "reload" && s.reloading {
                s.reloading = false;
                events.push(Event::Reloaded(reloaded(done, &words)));
                continue;
            }
            // Only the editor types at this console, one command at a time: an answer under the
            // asked command's name is its answer.
            if !s
                .asked
                .as_ref()
                .is_some_and(|(a, _)| a.ask.command() == command)
            {
                continue;
            }
            let Some((step, _)) = s.asked.take() else {
                continue;
            };
            if let Ask::Logoff(name) = &step.ask {
                // Out of the world now, or (refused) not in it.
                s.in_world.remove(&name.to_ascii_lowercase());
            }
            if done {
                if let Ask::Delete(name) = &step.ask {
                    s.in_world.remove(&name.to_ascii_lowercase());
                    characters::unmark(&setup.folder, name)?;
                } else if let Some(r) = &step.mark {
                    characters::mark_made(&setup.folder, r)?;
                }
                events.push(Event::Answered(step.ask, Ok(words)));
            } else {
                if !step.may_refuse {
                    // What waited on it waits for nothing now.
                    self.steps.clear();
                    self.pending = None;
                }
                events.push(Event::Answered(step.ask, Err(words)));
            }
        }
        let exit = s.child.try_wait();
        if !matches!(exit, Ok(None)) {
            let log = whole_lines_from(setup, 0).0;
            let code = exit.ok().flatten().and_then(|e| e.code());
            if ports_taken(code, &log) && s.tries < START_TRIES {
                // Another program took the ports between the look and the start: on to the next.
                let (port, tries) = (s.port, s.tries);
                if let Some((step, _)) = s.asked.take() {
                    self.steps.push_front(step);
                }
                self.server = None;
                let next = free_port(port + 2)?;
                self.spawn(setup, next, tries + 1)?;
                events.push(Event::Moved(next));
                return Ok(events);
            }
            let tail = log
                .lines()
                .rev()
                .find(|x| !x.trim().is_empty())
                .unwrap_or_default()
                .to_string();
            self.server = None;
            self.pending = None;
            self.steps.clear();
            self.lock = None;
            events.push(Event::Stopped(tail));
            return Ok(events);
        }
        if let Some((step, when)) = s.asked.take() {
            if when.elapsed() > ASK_WAIT {
                if !step.may_refuse {
                    self.steps.clear();
                    self.pending = None;
                }
                events.push(Event::Answered(
                    step.ask,
                    Err("Yalain did not answer".into()),
                ));
            } else {
                s.asked = Some((step, when));
            }
        }
        if s.up && s.asked.is_none() {
            let held = self.steps.front().and_then(|f| {
                f.leaves
                    .as_ref()
                    .filter(|n| s.in_world.contains(&n.to_ascii_lowercase()))
            });
            let go = match held {
                Some(_) => {
                    let since = *s.waiting.get_or_insert_with(Instant::now);
                    since.elapsed() > LEAVE_WAIT
                }
                None => true,
            };
            if go {
                if let Some(step) = self.steps.pop_front() {
                    s.waiting = None;
                    type_line(&mut s.console, &step.line)?;
                    s.asked = Some((step, Instant::now()));
                }
            }
        }
        if self.up() && !self.asking() {
            if let Some((name, retail)) = self.pending.take() {
                self.play(setup, &name, retail)?;
                events.push(Event::Playing);
            }
        }
        Ok(events)
    }

    /// Publish the world's `files`: write what changed into the playtest folder and ask the server
    /// to `reload`. Answers whether it was published (`false`: the server is not up yet, or a
    /// reload still runs; publish again later). Nothing changed asks nothing.
    ///
    /// # Errors
    /// A file that will not write, or the console closed.
    pub fn publish(&mut self, setup: &Setup, files: &Files) -> Result<bool, String> {
        if !self.up() || self.reloading() {
            return Ok(false);
        }
        // Before the rows: a clearing that can't be made leaves the server on what it has.
        setup.write_overlay()?;
        if write_world(&setup.world(), files, &mut self.written)? == 0 {
            return Ok(true);
        }
        self.command("reload")?;
        if let Some(s) = &mut self.server {
            s.reloading = true;
        }
        Ok(true)
    }

    /// Start a client into the server as character `name`: ours, or the retail one with `retail`.
    ///
    /// # Errors
    /// No server, no such client, or it does not start.
    pub fn play(&mut self, setup: &Setup, name: &str, retail: bool) -> Result<(), String> {
        kill(&mut self.client);
        let port = match &self.server {
            Some(s) => s.port.to_string(),
            None => return Err("no server runs".into()),
        };
        let account = account_of(name);
        let programs = &setup.programs;
        let run = &setup.folder;
        let mut cmd = if retail {
            let exe = programs
                .retail
                .as_deref()
                .ok_or("no retail client: give --retail <client.exe>")?;
            let mut c = Command::new(exe);
            c.args([
                "--host",
                "127.0.0.1",
                "--port",
                &port,
                "--account",
                &account,
                "--glsticketdirect",
                TICKET,
                "--user",
                name,
            ])
            .args(&programs.client_args)
            .current_dir(exe.parent().unwrap_or(Path::new(".")));
            c
        } else {
            let exe = programs
                .client
                .as_deref()
                .ok_or("no client: give --client <auberean.exe>")?;
            let dats = programs
                .client_dats
                .as_deref()
                .or(programs.server_dats.as_deref())
                .ok_or("no install for the client: give --client-dats <install>")?;
            let settings = run.join("client-settings");
            std::fs::create_dir_all(&settings).map_err(|e| e.to_string())?;
            let overlay = setup.client_overlay(dats)?;
            let log = File::create(run.join("client.log")).map_err(|e| e.to_string())?;
            let err = log.try_clone().map_err(|e| e.to_string())?;
            let mut c = Command::new(exe);
            c.args([
                "--dats".to_string(),
                dats.display().to_string(),
                "--server".into(),
                format!("127.0.0.1:{port}"),
                "--dev-login".into(),
                format!("{account}:{TICKET}"),
                "--dev-character".into(),
                name.to_string(),
                "--shots".into(),
                run.join("shots").display().to_string(),
            ])
            .args(
                overlay
                    .iter()
                    .flat_map(|o| ["--overlay-dat-dir".to_string(), o.display().to_string()]),
            )
            .args(&programs.client_args)
            .env("AUBEREAN_SETTINGS_DIR", &settings)
            .current_dir(run)
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(err));
            c
        };
        stop::windowless(&mut cmd);
        let child = cmd.spawn().map_err(|e| format!("the client: {e}"))?;
        self.retail = retail;
        self.client = Some(child);
        self.playing_as = Some(name.to_string());
        self.said = format!(
            "Yalain on 127.0.0.1:{port}; {} client playing as {name}",
            if retail { "the retail" } else { "our" }
        );
        Ok(())
    }

    /// Start the server again on the world's `files`, every character where it stands, on the free
    /// ports past the ones it ran on, and a client the editor started with it; the commands still
    /// to ask are asked of the new one.
    ///
    /// # Errors
    /// As [`Live::start`].
    pub fn restart(&mut self, setup: &Setup, files: &Files) -> Result<(), String> {
        let Some(port) = self.port() else {
            return Err("no server runs".into());
        };
        let replay = match (self.playing_as(), self.pending.take()) {
            (Some(name), _) => Some((name, self.retail)),
            (None, waiting) => waiting,
        };
        kill(&mut self.client);
        self.playing_as = None;
        // Past the ports it just let go, which the system may not give again at once.
        let port = {
            self.stop_server();
            free_port(port.saturating_add(4))?
        };
        self.start_at(setup, files, port)?;
        self.pending = replay;
        Ok(())
    }

    /// Stop the client the editor started; its character, in the world, is taken out of it at
    /// once.
    pub fn stop_client(&mut self) {
        self.pending = None;
        let gone = self.playing_as.take();
        kill(&mut self.client);
        if let Some(name) = gone.filter(|_| self.up()) {
            self.logoff_if_left(&name);
        }
    }

    /// Stop the server cleanly, keeping its characters: `stop` at its console. Answers whether it
    /// stopped by itself (past [`STOP_WAIT`] it is ended).
    pub fn stop_server(&mut self) -> bool {
        let Some(mut s) = self.server.take() else {
            return true;
        };
        if let Some((step, _)) = s.asked.take() {
            self.steps.push_front(step);
        }
        stop::ask(&mut s.child, &mut s.console, STOP_WAIT)
    }

    /// Stop everything the editor started, and let the playtest folder go.
    pub fn stop_all(&mut self) {
        self.stop_client();
        self.stop_server();
        self.steps.clear();
        self.lock = None;
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.stop_all();
    }
}
