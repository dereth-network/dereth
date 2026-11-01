//! Auberean's realm, as the launcher knows it: the player's own Asheron's Call 2 install and its
//! check, and the command lines that start Yalain (a world on this computer), the Auberean client
//! and the retail client, with a world's friends service when it has one.
//!
//! # The install
//!
//! Nothing of Asheron's Call 2 ships with the launcher: the player names the folder of their own
//! install, and the launcher checks it before anything reads it. The check is the one the Library
//! makes of Asheron's Call's data files ([`crate::dat`]): each file the client and the server open
//! is there, and its header says it is that file. An Asheron's Call 2 data file keeps its header
//! further in than an Asheron's Call one, with the same container under it: a mark, the size the
//! file was written at (so a file cut short by a copy or a download is caught), which data set the
//! file holds (the portal's, a region's cells, a language's strings) and which of that set's files
//! it is. The end-of-retail install's files are the portal file, the first region's cells and the
//! English strings, which every world needs, and the high-resolution pictures, read when the
//! install has them. A handful of bytes is read from each, however large: the check can run on
//! every start.
//!
//! # The programs
//!
//! Yalain and the client are separate programs the launcher downloads ([`crate::components`]). A
//! world on this computer is Yalain on [`LOCAL_LOGIN_PORT`] (and the three ports after it), over
//! the install, keeping its accounts and characters in one database file in the launcher's data
//! folder, and serving the world database its release pins, which `yalain world fetch` puts in the
//! player's cache. The client connects to a world's address with an account and its secret.
//!
//! # Playing
//!
//! - **The clients.** Auberean, downloaded; or, on Windows, the retail client of September 2013
//!   when the player's install has it ([`find_retail`]), started from its own folder. The launcher
//!   never writes into the install.
//! - **The secret.** A world makes an account the first time it logs in, with the secret it logs
//!   in with, and admits only that secret from then on. So the launcher keeps one secret for each
//!   world and account, with the player's other passwords, and gives it to either client at every
//!   launch ([`secret_for`]): the one the player typed, else one it made. A world on this computer
//!   asks the player for none.
//! - **The friends service.** Before each launch the launcher asks the world whether it serves the
//!   friends stream on its login port ([`FriendsProbe`], over the host's connection). When it does, the client is given the
//!   friends options in the retail client's own spelling and the friends ticket, made from the
//!   account and its secret at spawn ([`Arg::FriendsTicket`]); the retail client is given the
//!   friends library the launcher carries too ([`FRIENDS_DLL`]). When it does not, the client plays
//!   without one.

use std::ffi::OsString;
use std::io::Read;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use auberean_friends::{ClientConfig, ClientOut, ClientSession, Ticket, Welcome};
use serde::{Deserialize, Serialize};

use crate::launch::{Arg, LaunchPlan};
use crate::vault::{self, Vault};

/// Where an Asheron's Call 2 data file's header starts, and the mark it starts with.
const HEADER_AT: usize = 0x12C;
const MAGIC: u32 = 0x5442;
/// How far into a file the check reads: past the header's fields it looks at.
const HEAD_LEN: usize = HEADER_AT + 0x14;

/// The high-resolution pictures' file's subset word, `"HiFi"` read as a little-endian word.
const HIGHRES_SUBSET: u32 = 0x6946_6948;

/// Which of an install's data files a header says it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Everything the whole world shares.
    Portal,
    /// The high-resolution pictures laid over the portal file's.
    HighRes,
    /// One region's landscape and interiors.
    Cell { region: u32 },
    /// One language's strings.
    Local,
}

impl Kind {
    /// The kind a header's data set and subset name, if they name one.
    #[must_use]
    pub fn from_header(data_set: u32, subset: u32) -> Option<Self> {
        match (data_set, subset) {
            (1, 0) => Some(Kind::Portal),
            (1, HIGHRES_SUBSET) => Some(Kind::HighRes),
            (2, region) => Some(Kind::Cell { region }),
            (3, _) => Some(Kind::Local),
            _ => None,
        }
    }
}

/// The data files every world needs, each with what its header must say it is.
pub const REQUIRED: [(&str, Kind); 3] = [
    ("portal.dat", Kind::Portal),
    ("cell_1.dat", Kind::Cell { region: 1 }),
    ("local_English.dat", Kind::Local),
];

/// The data files read when the install has them.
pub const OPTIONAL: [(&str, Kind); 1] = [("highres.dat", Kind::HighRes)];

/// What one data file's header says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub kind: Option<Kind>,
    /// The size the file was written at.
    pub size: u32,
}

/// The header in a data file's first bytes.
///
/// # Errors
/// In words for a person, when the bytes are too few or do not start an Asheron's Call 2 data
/// file.
pub fn read_header(head: &[u8]) -> Result<Header, String> {
    let word = |at: usize| {
        head.get(HEADER_AT + at..HEADER_AT + at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let (Some(magic), Some(size), Some(set), Some(subset)) =
        (word(0), word(0x08), word(0x0C), word(0x10))
    else {
        return Err("is too short to be a data file".to_owned());
    };
    if magic != MAGIC {
        return Err("is not an Asheron's Call 2 data file".to_owned());
    }
    Ok(Header {
        kind: Kind::from_header(set, subset),
        size,
    })
}

/// What the check found of one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileCheck {
    pub name: String,
    pub required: bool,
    /// Whether the file is there.
    pub found: bool,
    /// What is wrong with a file that is there, in words for a person; `None` when it is sound.
    pub problem: Option<String>,
}

/// What the check found of a folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstallCheck {
    pub dir: PathBuf,
    pub files: Vec<FileCheck>,
    /// Every required file is there and sound, and every optional one there is sound.
    pub ok: bool,
    /// The verdict in a sentence for the page.
    pub summary: String,
}

/// Check one file at `path` against what its name says it is.
fn check_file(path: &Path, want: Kind) -> Option<String> {
    let mut head = vec![0u8; HEAD_LEN];
    let read = std::fs::File::open(path).and_then(|mut f| {
        let len = f.metadata()?.len();
        f.read_exact(&mut head)?;
        Ok(len)
    });
    let len = match read {
        Ok(len) => len,
        Err(_) => return Some("could not be read".to_owned()),
    };
    let header = match read_header(&head) {
        Ok(h) => h,
        Err(why) => return Some(why),
    };
    if header.kind != Some(want) {
        return Some("is another of the game's data files under this name".to_owned());
    }
    if u64::from(header.size) != len {
        return Some("is incomplete: it is not the size it was written at".to_owned());
    }
    None
}

/// The file in `dir` named `name` in any case, as the game's files are named on a system that
/// minds case and one that does not.
fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.is_file() {
        return Some(exact);
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path())
        .filter(|p| p.is_file())
}

/// Check the folder `dir` for an end-of-retail Asheron's Call 2 install's data files.
#[must_use]
pub fn check_install(dir: &Path) -> InstallCheck {
    let mut files = Vec::new();
    for (list, required) in [(&REQUIRED[..], true), (&OPTIONAL[..], false)] {
        for (name, kind) in list {
            let path = find_file(dir, name);
            files.push(FileCheck {
                name: (*name).to_owned(),
                required,
                found: path.is_some(),
                problem: path.and_then(|p| check_file(&p, *kind)),
            });
        }
    }
    let missing: Vec<&str> = files
        .iter()
        .filter(|f| f.required && !f.found)
        .map(|f| f.name.as_str())
        .collect();
    let broken: Vec<String> = files
        .iter()
        .filter_map(|f| f.problem.as_ref().map(|p| format!("{} {p}", f.name)))
        .collect();
    let ok = missing.is_empty() && broken.is_empty();
    let summary = if !dir.is_dir() {
        "That folder is not there.".to_owned()
    } else if ok {
        "Your Asheron's Call 2 files are ready.".to_owned()
    } else if missing.len() == REQUIRED.len() {
        "That folder holds no Asheron's Call 2 install: choose the folder with portal.dat in it."
            .to_owned()
    } else if !missing.is_empty() {
        format!("This install is missing {}.", missing.join(", "))
    } else {
        format!("{}.", broken.join("; "))
    };
    InstallCheck {
        dir: dir.to_path_buf(),
        files,
        ok,
        summary,
    }
}

/// The port a world on this computer takes for logins. Its world service takes the port two
/// above, and each takes the port after it too: four in all.
pub const LOCAL_LOGIN_PORT: u16 = 9200;

/// The name the world on this computer goes by.
pub const LOCAL_WORLD_NAME: &str = "This Computer";

/// The database of the world on this computer's accounts and characters, in the launcher's data
/// folder `data`.
#[must_use]
pub fn local_database(data: &Path) -> PathBuf {
    data.join("auberean").join("yalain.sqlite")
}

/// The arguments that start Yalain as the world on this computer: over `install`, keeping its
/// accounts and characters in `database`, listening for logins on `login_port` (on this computer
/// alone), serving the world database its release pins.
#[must_use]
pub fn yalain_args(install: &Path, database: &Path, login_port: u16) -> Vec<OsString> {
    vec![
        "--dats".into(),
        install.into(),
        "--data".into(),
        database.into(),
        "--login-port".into(),
        login_port.to_string().into(),
        "--world-port".into(),
        (login_port + 2).to_string().into(),
        "--world-name".into(),
        LOCAL_WORLD_NAME.into(),
    ]
}

/// The arguments that fetch the world database Yalain's release pins into the player's cache.
pub const WORLD_FETCH_ARGS: [&str; 2] = ["world", "fetch"];

/// Whether Yalain's log says it is ready for logins: it names where it listens once it has read
/// its data and compiled its world.
#[must_use]
pub fn is_ready(log: &str) -> bool {
    log.lines().any(|l| l.contains("listening on"))
}

/// What is wrong with an account name the player gave, in words for a person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountError {
    Empty,
    Colon,
    TooLong,
}

impl core::fmt::Display for AccountError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            AccountError::Empty => "Enter an account name.",
            AccountError::Colon => "An account name cannot hold a colon.",
            AccountError::TooLong => "An account name is at most 32 letters.",
        })
    }
}

impl std::error::Error for AccountError {}

/// An account name as the player gives it, trimmed: a few letters, with no colon (the client
/// takes the account and its password as one `account:password`).
///
/// # Errors
/// [`AccountError`] for one that is empty, holds a colon, or is too long.
pub fn account_name(raw: &str) -> Result<&str, AccountError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(AccountError::Empty);
    }
    if name.contains(':') {
        return Err(AccountError::Colon);
    }
    if name.chars().count() > 32 {
        return Err(AccountError::TooLong);
    }
    Ok(name)
}

// ----- the clients -------------------------------------------------------------------------------
//
// Two clients play a world of Asheron's Call 2's: Auberean, which the launcher downloads, and the
// retail client of September 2013 in the player's own install, on Windows. Each logs in with an
// account and its secret, which the launcher keeps for each world and account ([`secret_for`]).

/// Which client plays.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Client {
    #[default]
    Auberean,
    /// The retail client in the player's install.
    Retail,
}

/// The retail client's program in an install.
pub const RETAIL_EXE: &str = "AC2client.exe";

/// The link stamp in the September 2013 client's program header: the client the end-of-retail
/// files are for, and the one the friends library is made for.
pub const RETAIL_STAMP: u32 = 0x524A_35F5;

/// The retail client in an install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RetailClient {
    pub exe: PathBuf,
    /// Whether it is the September 2013 client.
    pub september_2013: bool,
}

/// The link stamp in a Windows program's first bytes, when they start one.
#[must_use]
pub fn link_stamp(head: &[u8]) -> Option<u32> {
    let word = |at: usize| {
        head.get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    if head.get(..2) != Some(b"MZ".as_slice()) {
        return None;
    }
    let pe = usize::try_from(word(0x3C)?).ok()?;
    if head.get(pe..pe + 4) != Some(b"PE\0\0".as_slice()) {
        return None;
    }
    word(pe + 8)
}

/// The retail client in the install `dir`, if it has one.
#[must_use]
pub fn find_retail(dir: &Path) -> Option<RetailClient> {
    let exe = find_file(dir, RETAIL_EXE)?;
    let mut head = Vec::new();
    let _ = std::fs::File::open(&exe).and_then(|f| f.take(4096).read_to_end(&mut head));
    Some(RetailClient {
        september_2013: link_stamp(&head) == Some(RETAIL_STAMP),
        exe,
    })
}

/// Whether the retail client takes `secret` whole on its command line, which it splits at spaces
/// and in which a double quote only turns quoting on or off: printable, and no double quote.
#[must_use]
pub fn retail_takes(secret: &str) -> bool {
    !secret.is_empty() && !secret.chars().any(|c| c == '"' || c.is_control())
}

// ----- the secret ---------------------------------------------------------------------------------

/// The name the world on this computer's accounts are kept under, as a listed world's are under its
/// slug.
pub const LOCAL_SLUG: &str = "auberean-this-computer";

/// A new secret from 16 random bytes (the host's random source's): URL-safe base64 without
/// padding, 22 letters, digits, `-` and `_`, which any command line carries as is.
#[must_use]
pub fn secret_from(random: &[u8; 16]) -> String {
    base64url(random)
}

/// `bytes` in base64url, without padding.
fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..=chunk.len() {
            out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
        }
    }
    out
}

/// The secret `account` logs in to the world `slug` with: the same at every launch, for both
/// clients. A world makes an account the first time it logs in, and from then on admits only the
/// secret it was made with, so the secret is kept with the player's other passwords ([`Vault`]):
/// the one the player typed, which is kept from then on; else the one kept; else a new one
/// (`fresh`), which is kept before it is used. A new one the store cannot keep (a system with no
/// secret store) is kept in `spare` instead, the launcher's own file ([`vault::SecretFile`]), and
/// found there at the next launch; a typed one never is, as no typed password is.
///
/// # Errors
/// In words for a person, when a new secret cannot be made or kept: one not kept would lock the
/// account out at the next launch.
pub fn secret_for(
    vault: &mut dyn Vault,
    spare: &mut dyn Vault,
    slug: &str,
    account: &str,
    typed: Option<&str>,
    fresh: &mut dyn FnMut() -> Result<String, String>,
) -> Result<String, String> {
    let key = vault::target(slug, account);
    if let Some(typed) = typed.filter(|t| !t.is_empty()) {
        // Kept if it can be; the player can type it again if not.
        let _ = vault.set(&key, account, typed);
        return Ok(typed.to_owned());
    }
    let kept = |v: &dyn Vault| v.get(&key).ok().flatten().filter(|k| !k.is_empty());
    if let Some(kept) = kept(vault).or_else(|| kept(spare)) {
        return Ok(kept);
    }
    let secret = fresh()?;
    if let Err(e) = vault.set(&key, account, &secret) {
        spare
            .set(&key, account, &secret)
            .map_err(|f| format!("The account's secret could not be kept: {e} {f}"))?;
    }
    Ok(secret)
}

// ----- the friends service ------------------------------------------------------------------------
//
// A world of Asheron's Call 2's may run a friends service beside its login, which both clients
// reach with the friends options and a ticket. The launcher names it only when the world answers
// for it (the probe, which waits on the friends stream's greeting), and the client plays without
// it otherwise, as retail did from 2013 to 2017.

/// The retail client's friends library, which the launcher carries beside its own program and
/// names by its full path: never a file in the player's install.
pub const FRIENDS_DLL: &str = "auberean_friends.dll";

/// The friends ticket for `account` and `secret`: the game login's own account and secret, as the
/// friends stream writes them (`a1.<account>.<secret>`). Only the world reads it; the clients pass
/// it on. It is a secret, as the password is. `None` for a ticket the stream cannot carry: no
/// account, or past its length.
#[must_use]
pub fn friends_ticket(account: &str, secret: &str) -> Option<String> {
    Ticket::new(account, secret).encode().ok()
}

/// How long the probe waits for a world's friends service to greet it before a launch.
pub const PROBE_WAIT: Duration = Duration::from_secs(2);

/// What the probe says of itself in its greeting.
const PROBE_SOFTWARE: &str = concat!("dereth-launch/", env!("CARGO_PKG_VERSION"));

/// What the probe heard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heard {
    /// Not yet a whole answer.
    Nothing,
    /// The world's greeting: it serves the friends stream.
    Welcome(Welcome),
    /// Anything else: a goodbye, or what is not the stream.
    Refused,
}

/// Asking a world whether it serves the friends stream, with no I/O: its host opens a connection to
/// the world's login port, writes [`FriendsProbe::opening`], passes in what it reads until
/// [`FriendsProbe::read`] says, and writes [`FriendsProbe::goodbye`] before it closes. It is the
/// stream's own client session, which presents no ticket and makes no request.
#[derive(Debug)]
pub struct FriendsProbe {
    session: ClientSession,
}

impl Default for FriendsProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl FriendsProbe {
    #[must_use]
    pub fn new() -> Self {
        let mut session = ClientSession::new(ClientConfig {
            software: PROBE_SOFTWARE.to_owned(),
            locale: None,
        });
        session.connect(Duration::ZERO);
        let _dial = session.poll(Duration::ZERO);
        Self { session }
    }

    /// The bytes to write once the connection is open: the stream's preamble and greeting.
    pub fn opening(&mut self) -> Vec<u8> {
        self.session.opened(Duration::ZERO);
        self.written()
    }

    /// What `bytes`, read from the connection, say.
    pub fn read(&mut self, bytes: &[u8]) -> Heard {
        self.session.on_bytes(Duration::ZERO, bytes);
        let mut heard = Heard::Nothing;
        for out in self.session.poll(Duration::ZERO) {
            match out {
                ClientOut::Up(welcome) if heard == Heard::Nothing => {
                    heard = Heard::Welcome(welcome);
                }
                ClientOut::Down { .. } => heard = Heard::Refused,
                _ => {}
            }
        }
        heard
    }

    /// The bytes to write before closing: the stream's goodbye.
    pub fn goodbye(&mut self) -> Vec<u8> {
        self.session.disconnect(Duration::ZERO);
        self.written()
    }

    fn written(&mut self) -> Vec<u8> {
        self.session
            .poll(Duration::ZERO)
            .into_iter()
            .filter_map(|o| match o {
                ClientOut::Write(bytes) => Some(bytes),
                _ => None,
            })
            .flatten()
            .collect()
    }
}

/// Where a world's friends service is, as the launcher names it to a client: the world's host and
/// login port, and for the retail client the friends library's path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Friends {
    pub host: String,
    pub port: u16,
    /// The friends library, absolute (the retail client's alone).
    pub dll: Option<PathBuf>,
}

/// The friends options in the clients' own spelling, with the ticket for `account`: the library
/// when there is one, the service's host and port, the request for its events, and the ticket.
fn friends_args(friends: &Friends, account: &str) -> Vec<Arg> {
    let mut out = Vec::new();
    if let Some(dll) = &friends.dll {
        out.extend([plain("--gunclientdll"), plain(dll.display().to_string())]);
    }
    out.extend([
        plain("--gunserver"),
        plain(friends.host.clone()),
        plain("--gunport"),
        plain(friends.port.to_string()),
        plain("--gunticket"),
        plain("--gunticketoverride"),
        Arg::FriendsTicket(account.to_owned()),
    ]);
    out
}

fn plain(s: impl Into<String>) -> Arg {
    Arg::Plain(s.into())
}

/// The plan that starts the Auberean client `program` over `install`, logging in to the world at
/// `server` as `account` (with its secret, put in at spawn), and naming the world's friends service
/// when it answered. Auberean takes no library.
#[must_use]
pub fn auberean_plan(
    program: &Path,
    install: &Path,
    server: SocketAddr,
    account: &str,
    friends: Option<&Friends>,
) -> LaunchPlan {
    let mut args = vec![
        plain("--dats"),
        plain(install.display().to_string()),
        plain("--server"),
        plain(server.to_string()),
        plain("--account"),
        Arg::AccountAndPassword(account.to_owned()),
    ];
    if let Some(f) = friends {
        let own = Friends {
            dll: None,
            ..f.clone()
        };
        args.extend(friends_args(&own, account));
    }
    LaunchPlan {
        exe: program.to_path_buf(),
        working_dir: program.parent().map(Path::to_path_buf).unwrap_or_default(),
        args,
    }
}

/// The plan that starts the retail client `exe` from its own install folder, logging in to the
/// world at `server` as `account` with its secret (put in at spawn), and with the friends options
/// and the library when the world's friends service answered. The client opens its data files
/// only to read them (its own `--rodat`): it changes nothing in the player's install, and a second
/// one can play from the same folder.
#[must_use]
pub fn retail_plan(
    exe: &Path,
    server: SocketAddr,
    account: &str,
    friends: Option<&Friends>,
) -> LaunchPlan {
    let mut args = vec![
        plain("--rodat"),
        plain("--host"),
        plain(server.ip().to_string()),
        plain("--port"),
        plain(server.port().to_string()),
        plain("--account"),
        plain(account),
        plain("--glsticketdirect"),
        Arg::Password,
    ];
    if let Some(f) = friends {
        args.extend(friends_args(f, account));
    }
    LaunchPlan {
        exe: exe.to_path_buf(),
        working_dir: exe.parent().map(Path::to_path_buf).unwrap_or_default(),
        args,
    }
}

/// The friends library a launcher in the folder `here` carries: beside its program, in a macOS
/// bundle's resources, or in a Linux package's (an AppImage's `usr/lib/Dereth`, its program in
/// `usr/bin`).
#[must_use]
pub fn friends_dll_beside(here: &Path) -> Option<PathBuf> {
    [
        here.join(FRIENDS_DLL),
        here.join("../Resources").join(FRIENDS_DLL),
        here.join("../lib/Dereth").join(FRIENDS_DLL),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

/// Whether the retail client loads `path` by the name it is given: printable ASCII, the system's
/// code page whatever it is, with no space and no double quote.
fn retail_reads(path: &Path) -> bool {
    path.to_str()
        .is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_graphic() && c != '"'))
}

/// The friends library `dll` as the retail client is given it: absolute, and with no space or
/// letter outside ASCII where that can be had. The path itself when it has none; else its short
/// form (`short`, the system's 8.3 name, where it has one); else a copy in the launcher's data
/// folder `data`, or that copy's short form; else the path itself, which the client keeps whole
/// when it is quoted.
#[must_use]
pub fn dll_for_retail(
    dll: &Path,
    data: &Path,
    short: &dyn Fn(&Path) -> Option<PathBuf>,
) -> PathBuf {
    let usable = |p: &Path| retail_reads(p).then(|| p.to_path_buf());
    if let Some(p) = usable(dll).or_else(|| short(dll).as_deref().and_then(usable)) {
        return p;
    }
    let dir = data.join("auberean");
    let copy = dir.join(FRIENDS_DLL);
    let same = matches!(
        (std::fs::read(dll), std::fs::read(&copy)),
        (Ok(ours), Ok(theirs)) if ours == theirs
    );
    let copied =
        same || (std::fs::create_dir_all(&dir).is_ok() && std::fs::copy(dll, &copy).is_ok());
    if copied {
        if let Some(p) = usable(&copy).or_else(|| short(&copy).as_deref().and_then(usable)) {
            return p;
        }
    }
    dll.to_path_buf()
}

/// The last `n` lines of a log's text, without a trailing empty one.
#[must_use]
pub fn tail(text: &str, n: usize) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..]
        .iter()
        .map(|l| (*l).to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the launcher's check of an Asheron's Call 2 install, and the command lines
    //! it starts Yalain and the Auberean client with).
    use super::*;
    use crate::datset::tests::tmp;

    /// The first bytes of a data file whose header says `set` and `subset`, written at `size`.
    fn head(set: u32, subset: u32, size: u32) -> Vec<u8> {
        let mut b = vec![0u8; 0x400];
        let mut put = |at: usize, v: u32| {
            b[HEADER_AT + at..HEADER_AT + at + 4].copy_from_slice(&v.to_le_bytes())
        };
        put(0, MAGIC);
        put(0x04, 0x400);
        put(0x08, size);
        put(0x0C, set);
        put(0x10, subset);
        b
    }

    /// A data file at `dir/name` of the kind `set` and `subset` say, as long as its header says.
    fn file(dir: &Path, name: &str, set: u32, subset: u32) {
        std::fs::write(dir.join(name), head(set, subset, 0x400)).unwrap();
    }

    fn install(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        file(dir, "portal.dat", 1, 0);
        file(dir, "cell_1.dat", 2, 1);
        file(dir, "local_English.dat", 3, 0x8000_0001);
    }

    #[test]
    fn a_header_names_its_file_by_its_data_set_and_subset() {
        let kind = |set, subset| read_header(&head(set, subset, 9)).unwrap().kind;
        assert_eq!(kind(1, 0), Some(Kind::Portal));
        assert_eq!(kind(1, HIGHRES_SUBSET), Some(Kind::HighRes));
        assert_eq!(kind(2, 1), Some(Kind::Cell { region: 1 }));
        assert_eq!(kind(3, 0x8100_0003), Some(Kind::Local));
        assert_eq!(kind(4, 0), None);
        assert_eq!(read_header(&head(1, 0, 9)).unwrap().size, 9);
        // An Asheron's Call data file keeps its header elsewhere, and is not taken for one.
        let mut first_game = vec![0u8; 0x400];
        first_game[0x140..0x144].copy_from_slice(&MAGIC.to_le_bytes());
        assert!(read_header(&first_game).is_err());
        assert!(read_header(&[0u8; 16]).is_err());
    }

    #[test]
    fn an_install_with_every_file_sound_is_ready_and_the_high_resolution_file_is_optional() {
        let d = tmp("ac2-install");
        install(&d);
        let check = check_install(&d);
        assert!(check.ok, "{check:?}");
        assert_eq!(check.summary, "Your Asheron's Call 2 files are ready.");
        assert!(
            !check
                .files
                .iter()
                .find(|f| f.name == "highres.dat")
                .unwrap()
                .found
        );
        file(&d, "highres.dat", 1, HIGHRES_SUBSET);
        assert!(check_install(&d).ok);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_missing_cut_short_or_misnamed_file_is_named_and_the_install_is_not_ready() {
        let d = tmp("ac2-install-bad");
        install(&d);
        std::fs::remove_file(d.join("cell_1.dat")).unwrap();
        let check = check_install(&d);
        assert!(!check.ok);
        assert_eq!(check.summary, "This install is missing cell_1.dat.");

        install(&d);
        // Written at a larger size than it is now: a copy cut short.
        std::fs::write(d.join("portal.dat"), head(1, 0, 0x10_0000)).unwrap();
        let check = check_install(&d);
        assert!(!check.ok);
        assert!(
            check.summary.contains("portal.dat is incomplete"),
            "{}",
            check.summary
        );

        install(&d);
        // The portal file's header under the English file's name.
        file(&d, "local_English.dat", 1, 0);
        assert!(check_install(&d)
            .summary
            .contains("another of the game's data files"));

        install(&d);
        // A high-resolution file that is there must be sound too.
        std::fs::write(d.join("highres.dat"), b"not a data file").unwrap();
        assert!(!check_install(&d).ok);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_folder_with_none_of_the_files_says_which_folder_to_choose() {
        let d = tmp("ac2-install-empty");
        std::fs::create_dir_all(&d).unwrap();
        let check = check_install(&d);
        assert!(!check.ok);
        assert!(
            check.summary.contains("choose the folder with portal.dat"),
            "{}",
            check.summary
        );
        assert_eq!(
            check_install(&d.join("nowhere")).summary,
            "That folder is not there."
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_files_are_found_whatever_the_case_of_their_names() {
        let d = tmp("ac2-install-case");
        std::fs::create_dir_all(&d).unwrap();
        file(&d, "PORTAL.DAT", 1, 0);
        file(&d, "Cell_1.dat", 2, 1);
        file(&d, "local_english.dat", 3, 0x8000_0001);
        assert!(check_install(&d).ok);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn yalain_runs_over_the_install_with_its_database_and_four_ports() {
        let args = yalain_args(Path::new("ac2"), Path::new("data/yalain.sqlite"), 9200);
        let args: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "--dats",
                "ac2",
                "--data",
                "data/yalain.sqlite",
                "--login-port",
                "9200",
                "--world-port",
                "9202",
                "--world-name",
                LOCAL_WORLD_NAME
            ]
        );
        assert_eq!(WORLD_FETCH_ARGS, ["world", "fetch"]);
        assert_eq!(
            local_database(Path::new("d")),
            Path::new("d").join("auberean").join("yalain.sqlite")
        );
    }

    #[test]
    fn yalain_is_ready_once_its_log_says_where_it_listens() {
        assert!(!is_ready(
            "reading the install\ncompiling the world database\n"
        ));
        assert!(is_ready(
            "the level table read\nlistening on 127.0.0.1:9200/9201 (login) and 127.0.0.1:9202/9203 (world)\n"
        ));
    }

    fn local() -> SocketAddr {
        "127.0.0.1:9200".parse().unwrap()
    }

    fn friends(dll: Option<&str>) -> Friends {
        Friends {
            host: "127.0.0.1".into(),
            port: 9200,
            dll: dll.map(PathBuf::from),
        }
    }

    #[test]
    fn auberean_logs_in_with_its_account_and_secret_and_the_secret_is_never_shown() {
        let program = Path::new("/apps/auberean/auberean.exe");
        let plan = auberean_plan(program, Path::new("ac2"), local(), "player", None);
        assert_eq!(
            plan.argv("s3cret"),
            [
                "--dats",
                "ac2",
                "--server",
                "127.0.0.1:9200",
                "--account",
                "player:s3cret"
            ]
        );
        assert_eq!(plan.exe, program);
        assert_eq!(plan.working_dir, Path::new("/apps/auberean"));
        let shown = plan.redacted();
        assert!(
            shown.ends_with("--account player:***") && !shown.contains("s3cret"),
            "{shown}"
        );
        assert_eq!(account_name("  Asheron "), Ok("Asheron"));
        assert_eq!(account_name(" "), Err(AccountError::Empty));
        assert_eq!(account_name("a:b"), Err(AccountError::Colon));
        assert_eq!(account_name(&"x".repeat(33)), Err(AccountError::TooLong));
    }

    #[test]
    fn with_a_friends_service_auberean_gets_the_four_options_and_a_ticket_made_at_spawn() {
        // Auberean takes no library, whatever the launcher carries.
        let with = Some(friends(Some("C:/launcher/auberean_friends.dll")));
        let plan = auberean_plan(
            Path::new("auberean"),
            Path::new("ac2"),
            local(),
            "player",
            with.as_ref(),
        );
        let argv = plan.argv("secret");
        assert_eq!(
            argv[6..],
            [
                "--gunserver",
                "127.0.0.1",
                "--gunport",
                "9200",
                "--gunticket",
                "--gunticketoverride",
                "a1.cGxheWVy.c2VjcmV0"
            ]
        );
        let ticket = Ticket::decode(&argv[12]).unwrap();
        assert_eq!(
            (ticket.account.as_str(), ticket.secret.as_str()),
            ("player", "secret")
        );
        assert!(plan.args.contains(&Arg::FriendsTicket("player".into())));
        let shown = plan.redacted();
        assert!(
            shown.ends_with("--gunticketoverride ***")
                && !shown.contains("secret")
                && !shown.contains("c2VjcmV0"),
            "{shown}"
        );
    }

    #[test]
    fn the_retail_client_logs_in_with_its_secret_from_its_install_and_the_library_only_with_friends(
    ) {
        let exe = Path::new("/games/ac2/AC2client.exe");
        let plan = retail_plan(exe, local(), "player", None);
        assert_eq!(
            plan.argv("secret"),
            [
                "--rodat",
                "--host",
                "127.0.0.1",
                "--port",
                "9200",
                "--account",
                "player",
                "--glsticketdirect",
                "secret"
            ]
        );
        assert_eq!(plan.working_dir, Path::new("/games/ac2"));
        assert!(plan.redacted().ends_with("--glsticketdirect ***"));
        let with = friends(Some("C:/launcher/auberean_friends.dll"));
        let plan = retail_plan(exe, local(), "player", Some(&with));
        assert_eq!(
            plan.argv("secret")[9..],
            [
                "--gunclientdll",
                "C:/launcher/auberean_friends.dll",
                "--gunserver",
                "127.0.0.1",
                "--gunport",
                "9200",
                "--gunticket",
                "--gunticketoverride",
                "a1.cGxheWVy.c2VjcmV0"
            ]
        );
        let shown = plan.redacted();
        assert!(
            !shown.contains("secret") && !shown.contains("c2VjcmV0"),
            "{shown}"
        );
        assert_eq!(FRIENDS_DLL, "auberean_friends.dll");
        // What its command line cannot carry whole.
        assert!(retail_takes("p@ss word:1"));
        assert!(!retail_takes("say \"hi\""));
        assert!(!retail_takes(""));
    }

    /// A world admits an account only with the secret it was made with, so every launch for one
    /// world and account passes one secret, to either client and in its friends ticket.
    #[test]
    fn two_launches_for_the_same_world_and_account_pass_the_same_secret() {
        let mut vault = crate::vault::MemoryVault::default();
        let mut spare = crate::vault::MemoryVault::default();
        let mut made = 0;
        let mut fresh = || {
            made += 1;
            Ok(format!("made-{made}"))
        };
        let first = secret_for(
            &mut vault, &mut spare, LOCAL_SLUG, "Player", None, &mut fresh,
        )
        .unwrap();
        let second = secret_for(
            &mut vault, &mut spare, LOCAL_SLUG, "player", None, &mut fresh,
        )
        .unwrap();
        assert_eq!(first, second);
        assert_eq!(first, "made-1");
        let launch = |secret: &str| {
            let with = friends(Some("C:/launcher/auberean_friends.dll"));
            let own = auberean_plan(
                Path::new("auberean"),
                Path::new("ac2"),
                local(),
                "player",
                Some(&with),
            );
            let retail = retail_plan(Path::new("AC2client.exe"), local(), "player", Some(&with));
            (own.argv(secret), retail.argv(secret))
        };
        assert_eq!(launch(&first), launch(&second));
        // Another account, or the same name on another world, has its own.
        let other = secret_for(
            &mut vault, &mut spare, LOCAL_SLUG, "someone", None, &mut fresh,
        )
        .unwrap();
        let elsewhere = secret_for(
            &mut vault, &mut spare, "dawnsong", "player", None, &mut fresh,
        )
        .unwrap();
        assert_eq!((other.as_str(), elsewhere.as_str()), ("made-2", "made-3"));
        // A secret the player types is used and kept from then on.
        let typed = secret_for(
            &mut vault,
            &mut spare,
            "dawnsong",
            "player",
            Some("hunter2"),
            &mut fresh,
        );
        assert_eq!(typed.as_deref(), Ok("hunter2"));
        let again = secret_for(
            &mut vault,
            &mut spare,
            "dawnsong",
            "player",
            Some(""),
            &mut fresh,
        );
        assert_eq!(again.as_deref(), Ok("hunter2"));
        assert_eq!(made, 3);
        assert!(
            spare.list("").unwrap().is_empty(),
            "the store kept them all"
        );
    }

    /// A store that keeps nothing.
    #[derive(Debug)]
    struct Broken;

    impl Vault for Broken {
        fn get(&self, _: &str) -> Result<Option<String>, crate::vault::VaultError> {
            Ok(None)
        }
        fn set(&mut self, _: &str, _: &str, _: &str) -> Result<(), crate::vault::VaultError> {
            Err(crate::vault::VaultError("no store".into()))
        }
        fn delete(&mut self, _: &str) -> Result<(), crate::vault::VaultError> {
            Ok(())
        }
        fn list(&self, _: &str) -> Result<Vec<String>, crate::vault::VaultError> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn a_secret_that_cannot_be_kept_is_not_used_but_a_typed_one_is() {
        let mut fresh = || Ok("made".to_owned());
        let made = secret_for(
            &mut Broken,
            &mut Broken,
            LOCAL_SLUG,
            "player",
            None,
            &mut fresh,
        );
        assert!(made.unwrap_err().contains("could not be kept"));
        let typed = secret_for(
            &mut Broken,
            &mut Broken,
            "dawnsong",
            "player",
            Some("hunter2"),
            &mut fresh,
        );
        assert_eq!(typed.as_deref(), Ok("hunter2"));
    }

    /// With no secret store (a Linux desktop with no keyring), the secret the launcher makes is
    /// kept in its own file, and every later launch passes it; a typed one is not written there.
    #[test]
    fn with_no_secret_store_a_new_secret_is_kept_in_the_launchers_file_and_reused() {
        let mut spare = crate::vault::MemoryVault::default();
        let mut made = 0;
        let mut fresh = || {
            made += 1;
            Ok(format!("made-{made}"))
        };
        let first = secret_for(
            &mut Broken,
            &mut spare,
            LOCAL_SLUG,
            "player",
            None,
            &mut fresh,
        );
        let second = secret_for(
            &mut Broken,
            &mut spare,
            LOCAL_SLUG,
            "Player",
            None,
            &mut fresh,
        );
        assert_eq!(first.as_deref(), Ok("made-1"));
        assert_eq!(second, first);
        let key = vault::target(LOCAL_SLUG, "player");
        assert_eq!(spare.get(&key).unwrap().as_deref(), Some("made-1"));
        let typed = secret_for(
            &mut Broken,
            &mut spare,
            "dawnsong",
            "player",
            Some("hunter2"),
            &mut fresh,
        );
        assert_eq!(typed.as_deref(), Ok("hunter2"));
        assert_eq!(
            spare.get(&vault::target("dawnsong", "player")).unwrap(),
            None
        );
        // A store that has one wins over the file.
        let mut store = crate::vault::MemoryVault::default();
        store.set(&key, "player", "kept").unwrap();
        let kept = secret_for(
            &mut store, &mut spare, LOCAL_SLUG, "player", None, &mut fresh,
        );
        assert_eq!(kept.as_deref(), Ok("kept"));
        assert_eq!(made, 1);
    }

    #[test]
    fn a_new_secret_is_its_random_bytes_and_carried_by_any_command_line() {
        let a = secret_from(&[0; 16]);
        let b = secret_from(&[0xFF; 16]);
        assert_eq!(a, "AAAAAAAAAAAAAAAAAAAAAA");
        assert_eq!(b, "_____________________w");
        for s in [&a, &b] {
            assert_eq!(s.len(), 22, "{s}");
            assert!(retail_takes(s));
        }
    }

    #[test]
    fn the_friends_ticket_is_the_streams_own_writing_and_one_it_cannot_carry_is_none() {
        assert_eq!(
            friends_ticket("player", "secret").as_deref(),
            Some("a1.cGxheWVy.c2VjcmV0")
        );
        assert_eq!(base64url(b"a"), "YQ");
        assert_eq!(base64url(b"ab"), "YWI");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
        assert_eq!(base64url(b""), "");
        let ticket = friends_ticket("Ölaf", "p@ss word:1").unwrap();
        assert!(
            ticket
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)),
            "{ticket}"
        );
        assert_eq!(friends_ticket("", "secret"), None);
        assert_eq!(friends_ticket("player", &"x".repeat(600)), None);
        // Spawned with a secret the ticket cannot carry, the placeholder is empty rather than wrong.
        let plan = LaunchPlan {
            exe: PathBuf::new(),
            working_dir: PathBuf::new(),
            args: vec![Arg::FriendsTicket("player".into())],
        };
        assert_eq!(plan.argv(&"x".repeat(600)), [""]);
    }

    /// A world's friends service as the probe meets it: the stream's own world half, in memory.
    fn world() -> auberean_friends::ServerStream {
        auberean_friends::ServerStream::new(
            Duration::ZERO,
            auberean_friends::ServerConfig::new("dawnsong", "Dawnsong"),
        )
    }

    fn written(out: Vec<auberean_friends::ServerOut>) -> Vec<u8> {
        out.into_iter()
            .filter_map(|o| match o {
                auberean_friends::ServerOut::Write(b) => Some(b),
                _ => None,
            })
            .flatten()
            .collect()
    }

    #[test]
    fn the_probe_greets_a_world_hears_its_welcome_and_says_goodbye() {
        let mut probe = FriendsProbe::new();
        let mut w = world();
        let opening = probe.opening();
        assert_eq!(opening[..4], auberean_friends::PREAMBLE);
        w.on_bytes(Duration::ZERO, &opening);
        let answer = written(w.poll(Duration::ZERO));
        // A whole answer, however it arrives.
        let (first, rest) = answer.split_at(3);
        assert_eq!(probe.read(first), Heard::Nothing);
        let Heard::Welcome(welcome) = probe.read(rest) else {
            panic!("no welcome");
        };
        assert_eq!(
            (welcome.world.as_str(), welcome.name.as_str()),
            ("dawnsong", "Dawnsong")
        );
        w.on_bytes(Duration::ZERO, &probe.goodbye());
        let _ = w.poll(Duration::ZERO);
        assert!(w.is_ended());
        assert!(!w.is_admitted());
    }

    #[test]
    fn anything_but_a_welcome_is_no_friends_service() {
        let mut probe = FriendsProbe::new();
        let _ = probe.opening();
        assert_eq!(
            probe.read(b"HTTP/1.1 400 Bad Request\r\n\r\n"),
            Heard::Refused
        );
        let mut probe = FriendsProbe::new();
        let _ = probe.opening();
        let mut bye = Vec::new();
        auberean_friends::Element::Bye {
            reason: auberean_friends::ByeReason::Full,
        }
        .write_frame(&mut bye)
        .unwrap();
        assert_eq!(probe.read(&bye), Heard::Refused);
    }

    /// A Windows program's first bytes with the link stamp `stamp`.
    fn program(stamp: u32) -> Vec<u8> {
        let mut b = vec![0u8; 0x200];
        b[..2].copy_from_slice(b"MZ");
        b[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        b[0x88..0x8C].copy_from_slice(&stamp.to_le_bytes());
        b
    }

    #[test]
    fn the_retail_client_is_found_in_the_install_and_known_by_its_link_stamp() {
        let d = tmp("ac2-retail");
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(find_retail(&d), None);
        std::fs::write(d.join("ac2client.exe"), program(RETAIL_STAMP)).unwrap();
        let found = find_retail(&d).unwrap();
        assert!(found.september_2013);
        // By its own name on a system that minds case, by either on one that does not.
        assert!(found.exe.is_file());
        assert!(found
            .exe
            .to_string_lossy()
            .eq_ignore_ascii_case(&d.join("ac2client.exe").to_string_lossy()));
        // An older client of the game, or a file that is no program.
        std::fs::write(d.join("ac2client.exe"), program(0x430D_48DB)).unwrap();
        assert!(!find_retail(&d).unwrap().september_2013);
        std::fs::write(d.join("ac2client.exe"), b"not a program").unwrap();
        assert!(!find_retail(&d).unwrap().september_2013);
        assert_eq!(link_stamp(&program(7)), Some(7));
        assert_eq!(link_stamp(b"MZ"), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_library_is_found_beside_the_launcher_or_in_its_bundles_resources() {
        let d = tmp("ac2-dll-beside");
        let (macos, resources) = (d.join("MacOS"), d.join("Resources"));
        std::fs::create_dir_all(&macos).unwrap();
        std::fs::create_dir_all(&resources).unwrap();
        assert_eq!(friends_dll_beside(&macos), None);
        std::fs::write(resources.join(FRIENDS_DLL), b"dll").unwrap();
        assert!(friends_dll_beside(&macos).is_some_and(|p| p.is_file()));
        std::fs::write(macos.join(FRIENDS_DLL), b"dll").unwrap();
        assert_eq!(friends_dll_beside(&macos), Some(macos.join(FRIENDS_DLL)));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A Linux package (the AppImage) has the launcher in `usr/bin` and its resources in
    /// `usr/lib/Dereth`.
    #[test]
    fn the_library_is_found_in_a_linux_packages_resources() {
        let d = tmp("ac2-dll-linux");
        let (bin, lib) = (d.join("usr/bin"), d.join("usr/lib/Dereth"));
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&lib).unwrap();
        assert_eq!(friends_dll_beside(&bin), None);
        std::fs::write(lib.join(FRIENDS_DLL), b"dll").unwrap();
        let found = friends_dll_beside(&bin).expect("found");
        assert!(found.is_file() && found.ends_with(Path::new("Dereth").join(FRIENDS_DLL)));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_retail_client_is_given_the_library_by_a_path_without_spaces_where_there_is_one() {
        let none = |_: &Path| None;
        // A path it reads as it is.
        let plain = Path::new("C:/Dereth/auberean_friends.dll");
        assert_eq!(dll_for_retail(plain, Path::new("unused"), &none), plain);
        // Else its short form.
        let spaced = Path::new("C:/Program Files/Dereth Network/auberean_friends.dll");
        let short =
            |p: &Path| (p == spaced).then(|| PathBuf::from("C:/PROGRA~1/DERETH~1/AUBERE~1.DLL"));
        assert_eq!(
            dll_for_retail(spaced, Path::new("unused"), &short),
            Path::new("C:/PROGRA~1/DERETH~1/AUBERE~1.DLL")
        );
        assert!(!retail_reads(Path::new(
            "C:/Users/Åsa/auberean_friends.dll"
        )));
        assert!(!retail_reads(Path::new("C:/a\"b/auberean_friends.dll")));

        // Else a copy in the launcher's data folder, when that has none.
        let d = tmp("ac2-dll-copy");
        let from = d.join("with space");
        std::fs::create_dir_all(&from).unwrap();
        let dll = from.join(FRIENDS_DLL);
        std::fs::write(&dll, b"the library").unwrap();
        let data = d.join("data");
        let given = dll_for_retail(&dll, &data, &none);
        let copy = data.join("auberean").join(FRIENDS_DLL);
        assert_eq!(std::fs::read(&copy).unwrap(), b"the library");
        if retail_reads(&copy) {
            assert_eq!(given, copy);
        } else {
            assert_eq!(given, dll, "no plain path: the path itself");
        }
        // A newer library replaces the copy.
        std::fs::write(&dll, b"the library, newer").unwrap();
        let _ = dll_for_retail(&dll, &data, &none);
        assert_eq!(std::fs::read(&copy).unwrap(), b"the library, newer");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_log_s_tail_is_its_last_lines() {
        assert_eq!(tail("a\nb\nc\n", 2), ["b", "c"]);
        assert_eq!(tail("a", 5), ["a"]);
        assert!(tail("", 3).is_empty());
    }
}
