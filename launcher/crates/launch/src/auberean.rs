//! Auberean's realm, as the launcher knows it: the player's own Asheron's Call 2 install and its
//! check, and the command lines that start Yalain (a world on this computer), the Auberean client
//! and the retail client, with a world's friends service when it has one.
//!
//! # The library
//!
//! Nothing of Asheron's Call 2 ships with the launcher: the player names the folders of their own
//! installs, as for Asheron's Call ([`crate::library`]). A folder holding `AC2client.exe` is the
//! retail client ([`identify_retail`], one at a time), checked against its build's manifest on
//! request as `acclient.exe` is; a folder holding the data files is a data set ([`read_folder`]),
//! each file read for its iterations ([`scan_dir`]), the default set the one the programs are
//! given. The launcher checks a set before anything reads it. The check is the one the Library
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
//! player's cache. The client connects to a world's address with an account and its password.
//!
//! # Playing
//!
//! - **The clients.** Auberean, downloaded; or, on Windows, the retail client of September 2013
//!   when the player's install has it ([`find_retail`]), started from its own folder. The launcher
//!   never writes into the install.
//! - **The password.** The player's own, as for Asheron's Call's worlds: typed on the world's
//!   page, or the one kept with the player's other passwords when they asked for it to be
//!   ([`crate::vault`]). A world makes an account the first time it logs in, with the password it
//!   logs in with, so the account is the player's on any launcher, or the retail client alone.
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

use dereth_dat::{ContainerEra, ContainerLayout, DatFile, EntryLayout, HeaderLayout};

use crate::datset::{DatFileState, DatOrigin, DatRole, DatSet, Iterations, SetKind};
use crate::install::{ClientKind, IdentifyError, Installation};
use crate::launch::{Arg, LaunchPlan};
use crate::library::FolderFind;
use crate::state::LauncherState;
use crate::world::Game;

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
    check_head(&head, len, want)
}

/// What is wrong with a file of `len` bytes starting `head`, against what its name says it is.
fn check_head(head: &[u8], len: u64, want: Kind) -> Option<String> {
    let header = match read_header(head) {
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
    verdict(dir, files, dir.is_dir())
}

/// What the check found of the `files` looked for in `dir`, which `exists` says is there.
fn verdict(dir: &Path, files: Vec<FileCheck>, exists: bool) -> InstallCheck {
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
    let summary = if !exists {
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

/// What the check finds of a set kept where the launcher read its files once, in a browser's
/// storage: what each file's reading said ([`file_in_storage`]).
#[must_use]
pub fn check_set(set: &DatSet) -> InstallCheck {
    let files = SET_FILES
        .iter()
        .map(|(role, name, _)| {
            let file = set.files.iter().find(|f| f.role == *role);
            FileCheck {
                name: (*name).to_owned(),
                required: *role != DatRole::Highres,
                found: file.is_some(),
                problem: file.and_then(|f| f.error.clone()),
            }
        })
        .collect();
    verdict(&set.path, files, true)
}

// ----- the library --------------------------------------------------------------------------------

/// This game's data file layout, as the shared container reads it: the header at `0x12C` in the
/// later files' order (with the data set and subset words), and 16-byte directory entries `{id,
/// first block, size, time}`. Neither the header nor an entry keeps an iteration: a file's
/// iterations are its `0xFFFF0001` list alone.
pub const LAYOUT: ContainerLayout = ContainerLayout {
    header: HeaderLayout {
        at: 0x12C,
        bytes: 0x54,
        file_size: 0x08,
        data_set: Some(0x0C),
        data_subset: Some(0x10),
        free_head: 0x14,
        free_tail: 0x18,
        free_count: 0x1C,
        btree_root: 0x20,
        young_lru: Some(0x24),
        old_lru: Some(0x28),
        use_lru: Some(0x2C),
        master_map_id: Some(0x30),
        eng_pack_vnum: Some(0x34),
        game_pack_vnum: Some(0x38),
        version_major: Some(0x40),
        version_minor: Some(0x50),
        iteration: None,
    },
    entry: EntryLayout {
        bytes: 16,
        bits: None,
        id: 0,
        offset: 4,
        size: 8,
        date: Some(12),
        iteration: None,
    },
    transaction: Some(dereth_dat::TransactionSlot {
        at: 0x100,
        bytes: 0x2C,
        journal: false,
    }),
    // The shared container's choice among the first game's record layouts; no reading here
    // decodes a record.
    era: ContainerEra::Modern,
};

/// The iterations every client reported to the servers in their last weeks, at the end of retail:
/// the portal file's 137, the first region's cells' 7, the English strings' 9 and the
/// high-resolution pictures' 7.
pub const END_OF_RETAIL: Iterations = Iterations {
    portal: Some(137),
    cell: Some(7),
    local: Some(9),
    highres: Some(7),
};

/// A set's files, by the role each plays in a set ([`DatRole`]), with what its header must say it
/// is. The cell file is the first region's, and the strings the English ones, as the client opens.
const SET_FILES: [(DatRole, &str, Kind); 4] = [
    (DatRole::Portal, "portal.dat", Kind::Portal),
    (DatRole::Cell, "cell_1.dat", Kind::Cell { region: 1 }),
    (DatRole::Local, "local_English.dat", Kind::Local),
    (DatRole::Highres, "highres.dat", Kind::HighRes),
];

/// The role a file named `name` plays in a set, in any case. `None` for a name that is no file of
/// a set.
#[must_use]
pub fn role_of(name: &str) -> Option<DatRole> {
    SET_FILES
        .iter()
        .find(|(_, n, _)| n.eq_ignore_ascii_case(name))
        .map(|(role, _, _)| *role)
}

/// Read a set's files in the folder `dir`: each that is there, by its size, time and the
/// iterations its list holds, or with why it is not the file its name says ([`check_install`]'s
/// words). A file whose size and time are those of its reading in `previous` keeps that reading:
/// a few small reads a file however large, and none for a file that has not changed.
#[must_use]
pub fn scan_dir(dir: &Path, previous: &[DatFileState]) -> Vec<DatFileState> {
    let mut out = Vec::new();
    for (role, name, kind) in SET_FILES {
        let Some(path) = find_file(dir, name) else {
            continue;
        };
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        let file_name = path
            .file_name()
            .map_or_else(|| name.to_owned(), |n| n.to_string_lossy().into_owned());
        let size = meta.len();
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        let read_only = meta.permissions().readonly();
        if let Some(p) = previous.iter().find(|p| {
            p.role == role
                && p.file_name == file_name
                && p.size == size
                && p.modified == modified
                && p.error.is_none()
        }) {
            out.push(DatFileState {
                read_only,
                ..p.clone()
            });
            continue;
        }
        let (iterations, error) = match check_file(&path, kind) {
            Some(why) => (None, Some(why)),
            None => match DatFile::read_iterations_as(&path, &LAYOUT) {
                Ok(read) => (Some(read.count), None),
                Err(e) => (
                    None,
                    Some(format!("its list of iterations does not read: {e}")),
                ),
            },
        };
        out.push(DatFileState {
            role,
            file_name,
            size,
            modified,
            read_only,
            iterations,
            error,
        });
    }
    out
}

/// One file of a set, named `name` and `size` bytes, read through `storage` from where the host
/// keeps it, as [`scan_dir`] reads one from a folder. `None` for a name that is no file of a set.
#[must_use]
pub fn file_in_storage(
    name: &str,
    size: u64,
    storage: Box<dyn dereth_dat::DatStorage>,
) -> Option<DatFileState> {
    let (role, _, kind) = SET_FILES
        .iter()
        .find(|(_, n, _)| n.eq_ignore_ascii_case(name))?;
    let mut head = vec![0u8; HEAD_LEN];
    let problem = match storage.read_exact_at(0, &mut head) {
        Ok(()) => check_head(&head, size, *kind),
        Err(_) => Some("could not be read".to_owned()),
    };
    let (iterations, error) = match problem {
        Some(why) => (None, Some(why)),
        None => match DatFile::read_iterations_from_as(name.into(), storage, &LAYOUT) {
            Ok(read) => (Some(read.count), None),
            Err(e) => (
                None,
                Some(format!("its list of iterations does not read: {e}")),
            ),
        },
    };
    Some(DatFileState {
        role: *role,
        file_name: name.to_owned(),
        size,
        modified: 0,
        read_only: false,
        iterations,
        error,
    })
}

/// Why files named `names` are not a set, in words for the player; `None` when they are: the
/// portal, cell and English files, with the high-resolution pictures when there are any.
#[must_use]
pub fn set_problem<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let names: Vec<&str> = names.into_iter().collect();
    let has = |n: &str| names.iter().any(|x| x.eq_ignore_ascii_case(n));
    let missing: Vec<&str> = SET_FILES
        .iter()
        .filter(|(role, name, _)| *role != DatRole::Highres && !has(name))
        .map(|(_, name, _)| *name)
        .collect();
    if missing.is_empty() {
        return None;
    }
    let any = SET_FILES.iter().any(|(_, name, _)| has(name));
    Some(set_problem_words(&missing, any))
}

/// Why a folder or a choice of files is not a set, by what it lacks and whether it has any of a
/// set's files.
fn set_problem_words(missing: &[&str], any: bool) -> String {
    if any {
        format!(
            "This folder has only some of the data files: {} {} missing.",
            missing.join(", "),
            if missing.len() == 1 { "is" } else { "are" }
        )
    } else {
        "No Asheron's Call 2 data files in this folder: a set is portal.dat, cell_1.dat and              local_English.dat, with highres.dat when there is one."
            .to_owned()
    }
}

/// The names of a set's files.
#[must_use]
pub fn set_file_names() -> Vec<&'static str> {
    SET_FILES.iter().map(|(_, name, _)| *name).collect()
}

/// Identify the retail client in `dir`, which must hold `AC2client.exe` (in any case): its build,
/// when the launcher knows it, else what the program says of itself. The quick step only;
/// [`crate::install::verify`] checks its folder against its build's manifest.
///
/// # Errors
/// [`IdentifyError::NoClient`] for a folder without the program.
pub fn identify_retail(dir: &Path) -> Result<Installation, IdentifyError> {
    let exe = find_file(dir, RETAIL_EXE).ok_or(IdentifyError::NoClient)?;
    let name = exe
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(IdentifyError::NoClient)?;
    crate::install::identify_exe(dir, name, ClientKind::Retail, "retail".into())
}

/// Read a folder for Auberean's realm's library: whether it holds the retail client, and whether
/// it holds a data set, as [`crate::library::read_folder`] reads one for Asheron's Call. A set is
/// the portal, cell and English files, with the high-resolution pictures when the folder has them;
/// a folder with only some of them is refused, naming what is missing.
#[must_use]
pub fn read_folder(path: &Path, state: &LauncherState) -> FolderFind {
    let retail = identify_retail(path).ok();
    let files = scan_dir(path, &[]);
    let missing: Vec<&str> = SET_FILES
        .iter()
        .filter(|(role, _, _)| *role != DatRole::Highres && !files.iter().any(|f| f.role == *role))
        .map(|(_, name, _)| *name)
        .collect();
    let dats = missing.is_empty().then(|| DatSet {
        id: state.new_id("a"),
        path: path.to_path_buf(),
        kind: SetKind::Auberean,
        origin: DatOrigin::Unassigned,
        files: files.clone(),
        last_patched_by_server: None,
        created_by_launcher: false,
    });
    let error = dats
        .is_none()
        .then(|| set_problem_words(&missing, !files.is_empty()));
    FolderFind {
        game: Game::Auberean,
        folder: path.display().to_string(),
        retail,
        dats,
        classic: None,
        error,
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

/// Whether Yalain's log says it is ready for logins: once it has read its data and compiled its
/// world it says where it listens, on a line ending in the tag `[listening]` that programs read
/// (the words before the tag may change; the tag does not).
#[must_use]
pub fn is_ready(log: &str) -> bool {
    log.lines().any(|l| l.trim_end().ends_with(" [listening]"))
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
// account and its password.

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
/// `server` as `account` with its secret (put in at spawn) in the retail client's own options,
/// which Auberean reads as the retail client does, and naming the world's friends service when it
/// answered. Auberean takes no library.
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

    use crate::testing::{write_auberean_file as container, write_auberean_set as set_of};

    /// A set kept where only a reader reaches it, a browser's storage, is read and checked as a
    /// folder's: the same iterations, and the same words for a file that is not what it says.
    #[test]
    fn a_file_read_through_storage_is_read_and_checked_as_a_folder_s() {
        let d = tmp("ac2-storage");
        set_of(&d, END_OF_RETAIL);
        let read = |name: &str| {
            let bytes = std::fs::read(d.join(name)).unwrap();
            let size = bytes.len() as u64;
            file_in_storage(name, size, Box::new(bytes)).unwrap()
        };
        let mut set = read_folder(&d, &LauncherState::default()).dats.unwrap();
        let from_folder = set.files.clone();
        set.files = set_file_names().into_iter().map(read).collect();
        assert_eq!(set.iterations(), END_OF_RETAIL);
        for (kept, folder) in set.files.iter().zip(&from_folder) {
            assert_eq!(
                (&kept.file_name, kept.size, kept.error.as_ref()),
                (&folder.file_name, folder.size, None)
            );
        }
        assert!(check_set(&set).ok);
        // The high-resolution pictures stay optional.
        set.files.retain(|f| f.role != DatRole::Highres);
        assert!(check_set(&set).ok);

        // A file cut short is named, in the folder check's words, and the set is not ready.
        let portal = std::fs::read(d.join("portal.dat")).unwrap();
        let cut = file_in_storage("portal.dat", 0x401, Box::new(portal)).unwrap();
        let why = cut.error.clone().unwrap();
        assert!(why.contains("incomplete"), "{why}");
        set.files[0] = cut;
        let check = check_set(&set);
        assert!(!check.ok);
        assert!(
            check.summary.starts_with("portal.dat is incomplete"),
            "{check:?}"
        );
        assert!(file_in_storage("elsewhere.dat", 1, Box::new(vec![0u8; 1])).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Files are refused as a set before anything is kept of them, in the folder's words.
    #[test]
    fn a_choice_of_files_that_is_no_set_says_what_it_lacks() {
        assert_eq!(
            set_problem(["portal.dat", "CELL_1.dat", "local_english.dat"]),
            None
        );
        let some = set_problem(["portal.dat"]).unwrap();
        assert!(
            some.contains("cell_1.dat, local_English.dat are missing"),
            "{some}"
        );
        let none = set_problem(["client_portal.dat"]).unwrap();
        assert!(none.starts_with("No Asheron's Call 2 data files"), "{none}");
    }

    #[test]
    fn a_folder_of_the_game_s_files_is_a_data_set_with_each_file_s_iterations() {
        let d = tmp("ac2-set");
        set_of(&d, END_OF_RETAIL);
        let find = read_folder(&d, &LauncherState::default());
        assert_eq!(find.game, Game::Auberean);
        assert_eq!((find.retail.as_ref(), find.error.as_deref()), (None, None));
        let set = find.dats.unwrap();
        assert_eq!(set.kind, SetKind::Auberean);
        assert_eq!(set.iterations(), END_OF_RETAIL);
        assert_eq!(
            set.files
                .iter()
                .map(|f| f.file_name.as_str())
                .collect::<Vec<_>>(),
            [
                "portal.dat",
                "cell_1.dat",
                "local_English.dat",
                "highres.dat"
            ]
        );
        // The installs check as sound, and the high-resolution pictures stay optional.
        assert!(check_install(&d).ok);
        std::fs::remove_file(d.join("highres.dat")).unwrap();
        let set = read_folder(&d, &LauncherState::default()).dats.unwrap();
        assert_eq!(set.iterations().highres, None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_set_read_again_takes_a_patched_file_s_new_iterations() {
        let d = tmp("ac2-set-again");
        set_of(&d, END_OF_RETAIL);
        let mut set = read_folder(&d, &LauncherState::default()).dats.unwrap();
        container(&d, "portal.dat", 1, 0, 140);
        // The rewrite can land in the same second at the same size; age the kept readings so this
        // tests the new reading, not the file system's clock.
        for f in &mut set.files {
            f.modified = 0;
        }
        assert!(
            !set.refresh(5),
            "no world of this game patches its players' files"
        );
        assert_eq!(set.iterations().portal, Some(140));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_folder_with_only_some_files_or_a_file_under_another_s_name_says_so() {
        let d = tmp("ac2-set-some");
        set_of(
            &d,
            Iterations {
                cell: None,
                highres: None,
                ..END_OF_RETAIL
            },
        );
        let find = read_folder(&d, &LauncherState::default());
        assert!(find.dats.is_none());
        assert_eq!(
            find.error.as_deref(),
            Some("This folder has only some of the data files: cell_1.dat is missing.")
        );
        // The portal file under the cells' name is listed, with why, and no iterations.
        container(&d, "cell_1.dat", 1, 0, 3);
        let files = scan_dir(&d, &[]);
        let cell = files.iter().find(|f| f.role == DatRole::Cell).unwrap();
        assert_eq!(cell.iterations, None);
        assert_eq!(
            cell.error.as_deref(),
            Some("is another of the game's data files under this name")
        );
        let empty = tmp("ac2-set-none");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(read_folder(&empty, &LauncherState::default())
            .error
            .unwrap()
            .starts_with("No Asheron's Call 2 data files in this folder"));
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn the_retail_client_and_its_files_go_into_auberean_s_library_and_never_asheron_s_call_s() {
        let one = tmp("ac2-lib-one");
        set_of(&one, END_OF_RETAIL);
        std::fs::write(one.join("AC2client.exe"), program(RETAIL_STAMP)).unwrap();
        let mut s = LauncherState::default();
        let find = read_folder(&one, &s);
        let retail = find.retail.clone().unwrap();
        assert_eq!(retail.kind, ClientKind::Retail);
        assert_eq!(retail.family(), "AC2client");
        assert_eq!(retail.path, one);
        crate::library::add_find(&mut s, find);
        assert_eq!(s.auberean.retail.as_ref().map(|r| &r.path), Some(&one));
        assert_eq!(s.auberean_default_set().map(|d| &d.path), Some(&one));
        assert!(s.retail.is_none() && s.dat_sets.is_empty());
        // The same folder again is read again, not added twice.
        let again = read_folder(&one, &s);
        crate::library::add_find(&mut s, again);
        assert_eq!(s.auberean.dat_sets.len(), 1);

        // A second set is not the default until it is made one; ids are never shared with the
        // other game's sets.
        let two = tmp("ac2-lib-two");
        set_of(&two, END_OF_RETAIL);
        let find = read_folder(&two, &s);
        crate::library::add_find(&mut s, find);
        let second = s.auberean.dat_sets[1].clone();
        assert_eq!(second.origin, DatOrigin::Unassigned);
        assert_ne!(second.id, s.auberean.dat_sets[0].id);
        assert!(s.set_default_set(&second.id));
        assert_eq!(s.auberean_default_set().map(|d| &d.id), Some(&second.id));
        assert_eq!(s.auberean.dat_sets[0].origin, DatOrigin::Unassigned);
        assert_eq!(s.new_id("a"), "a3");
        let _ = std::fs::remove_dir_all(&one);
        let _ = std::fs::remove_dir_all(&two);
    }

    #[test]
    fn an_older_launcher_s_install_folder_is_taken_into_the_library_once() {
        let d = tmp("ac2-adopt");
        set_of(&d, END_OF_RETAIL);
        std::fs::write(d.join("AC2client.exe"), program(RETAIL_STAMP)).unwrap();
        let json = format!(
            r#"{{"schema": 1, "auberean": {{"install": {}, "account": "player"}}}}"#,
            serde_json::to_string(&d).unwrap()
        );
        let mut s: LauncherState = serde_json::from_str(&json).unwrap();
        assert!(s.adopt_auberean_install());
        assert_eq!(s.auberean_default_set().map(|d| &d.path), Some(&d));
        assert_eq!(s.auberean.retail.as_ref().map(|r| &r.path), Some(&d));
        assert_eq!(s.auberean.account.as_deref(), Some("player"));
        assert!(!s.adopt_auberean_install(), "only once");
        let written = serde_json::to_string(&s).unwrap();
        assert!(!written.contains("\"install\""), "{written}");
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
            "the level table read\n2026-11-01T10:00:00Z  INFO yalain: listening on 127.0.0.1:9200/9201 \
             (login) and 127.0.0.1:9202/9203 (world, referred as 127.0.0.1) [listening]\r\n"
        ));
        // Read by its tag, not its words: the friends service's own line is not it.
        assert!(!is_ready(
            "2026-11-01T10:00:00Z  INFO yalain::friends: friends service listening on TCP \
             127.0.0.1:9200\n"
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
                "--host",
                "127.0.0.1",
                "--port",
                "9200",
                "--account",
                "player",
                "--glsticketdirect",
                "s3cret"
            ]
        );
        assert_eq!(plan.exe, program);
        assert_eq!(plan.working_dir, Path::new("/apps/auberean"));
        let shown = plan.redacted();
        assert!(
            shown.ends_with("--account player --glsticketdirect ***") && !shown.contains("s3cret"),
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
            argv[10..],
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
        let ticket = Ticket::decode(&argv[16]).unwrap();
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

    #[test]
    fn the_friends_ticket_is_the_streams_own_writing_and_one_it_cannot_carry_is_none() {
        assert_eq!(
            friends_ticket("player", "secret").as_deref(),
            Some("a1.cGxheWVy.c2VjcmV0")
        );
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
