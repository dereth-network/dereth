//! Auberean's realm, as the launcher knows it: the player's own Asheron's Call 2 install and its
//! check, and the command lines that start Yalain (a world on this computer) and the Auberean
//! client.
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
//! player's cache. The client connects to a world's address with an account; a world on this
//! computer makes an account the first time a name logs in, and asks for no password.

use std::ffi::OsString;
use std::io::Read;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::Serialize;

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

/// The arguments that start the Auberean client over `install`, logging in to the world at
/// `server` as `account` with `password` (empty for a world on this computer, which asks none).
#[must_use]
pub fn client_args(
    install: &Path,
    server: SocketAddr,
    account: &str,
    password: &str,
) -> Vec<OsString> {
    vec![
        "--dats".into(),
        install.into(),
        "--server".into(),
        server.to_string().into(),
        "--account".into(),
        format!("{account}:{password}").into(),
    ]
}

/// The client's command line as the launcher shows and logs it: the password after `--account`,
/// the secret after `--glsticketdirect` and the friends ticket after `--gunticketoverride`
/// replaced by `***`.
#[must_use]
pub fn shown_client_args(args: &[OsString]) -> String {
    let mut out = Vec::new();
    let mut before = String::new();
    for a in args {
        let a = a.to_string_lossy();
        out.push(match before.as_str() {
            "--account" if a.contains(':') => {
                format!("{}:***", a.split(':').next().unwrap_or(""))
            }
            "--glsticketdirect" | "--gunticketoverride" => "***".to_owned(),
            _ => a.clone().into_owned(),
        });
        before = a.into_owned();
    }
    out.join(" ")
}

// ----- the friends service ------------------------------------------------------------------------
//
// A world of Asheron's Call 2's may run a friends service beside its login, which both clients
// reach with the friends options and a ticket. The launcher names it only when the world answers
// for it (the probe, which waits on the friends stream's handshake), and the client plays without
// it otherwise, as retail did from 2013 to 2017.

/// The retail 0930 client's friends library, which the launcher carries beside its own program and
/// names by its full path: never a file in the player's install. Until a launcher carries it, the
/// retail client starts without the friends options.
pub const FRIENDS_DLL: &str = "auberean_friends.dll";

/// The friends ticket for `account` and `secret`: the game login's own account and secret, each
/// base64url without padding, as `a1.<account>.<secret>`. Only the world reads it; the clients pass
/// it on. It is a secret, as the password is.
#[must_use]
pub fn friends_ticket(account: &str, secret: &str) -> String {
    format!(
        "a1.{}.{}",
        base64url(account.as_bytes()),
        base64url(secret.as_bytes())
    )
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

/// Where a world's friends service is, as the launcher names it to a client: the world's host and
/// login port, and for the retail client the friends library's path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Friends {
    pub host: String,
    pub port: u16,
    /// The friends library, absolute (the retail client's alone).
    pub dll: Option<PathBuf>,
}

/// The friends options in the clients' own spelling, with the ticket for `account` and `secret`:
/// the library when there is one, the service's host and port, the request for its events, and the
/// ticket.
#[must_use]
pub fn friends_args(friends: &Friends, account: &str, secret: &str) -> Vec<OsString> {
    let mut out: Vec<OsString> = Vec::new();
    if let Some(dll) = &friends.dll {
        out.extend(["--gunclientdll".into(), dll.into()]);
    }
    out.extend([
        "--gunserver".into(),
        friends.host.as_str().into(),
        "--gunport".into(),
        friends.port.to_string().into(),
        "--gunticket".into(),
        "--gunticketoverride".into(),
        friends_ticket(account, secret).into(),
    ]);
    out
}

/// The arguments that start the retail 0930 client, from its own install folder, logging in to the
/// world at `host` and `port` as `account` with `secret`, and with the friends options when the
/// world's friends service answered.
#[must_use]
pub fn retail_args(
    host: &str,
    port: u16,
    account: &str,
    secret: &str,
    friends: Option<&Friends>,
) -> Vec<OsString> {
    let mut out: Vec<OsString> = vec![
        "--host".into(),
        host.into(),
        "--port".into(),
        port.to_string().into(),
        "--account".into(),
        account.into(),
        "--glsticketdirect".into(),
        secret.into(),
    ];
    if let Some(f) = friends {
        out.extend(friends_args(f, account, secret));
    }
    out
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
    fn the_client_logs_in_with_its_account_and_the_password_is_never_shown() {
        let server: SocketAddr = "127.0.0.1:9200".parse().unwrap();
        let args = client_args(Path::new("ac2"), server, "player", "secret");
        let shown = shown_client_args(&args);
        assert_eq!(
            shown,
            "--dats ac2 --server 127.0.0.1:9200 --account player:***"
        );
        assert!(!shown.contains("secret"));
        assert_eq!(args[5], OsString::from("player:secret"));
        assert_eq!(account_name("  Asheron "), Ok("Asheron"));
        assert_eq!(account_name(" "), Err(AccountError::Empty));
        assert_eq!(account_name("a:b"), Err(AccountError::Colon));
        assert_eq!(account_name(&"x".repeat(33)), Err(AccountError::TooLong));
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

    #[test]
    fn the_friends_ticket_is_the_account_and_secret_in_base64url_without_padding() {
        assert_eq!(friends_ticket("player", "secret"), "a1.cGxheWVy.c2VjcmV0");
        assert_eq!(base64url(b"a"), "YQ");
        assert_eq!(base64url(b"ab"), "YWI");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
        assert_eq!(base64url(b""), "");
        let ticket = friends_ticket("Ölaf", "p@ss word:1");
        assert!(
            ticket
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)),
            "{ticket}"
        );
    }

    #[test]
    fn the_retail_client_logs_in_with_its_secret_and_names_the_friends_service_only_when_asked() {
        let plain = retail_args("play.example", 9000, "player", "secret", None);
        let shown = shown_client_args(&plain);
        assert_eq!(
            shown,
            "--host play.example --port 9000 --account player --glsticketdirect ***"
        );
        let friends = Friends {
            host: "play.example".into(),
            port: 9000,
            dll: Some(PathBuf::from("C:/launcher/auberean_friends.dll")),
        };
        let with = retail_args("play.example", 9000, "player", "secret", Some(&friends));
        let with: Vec<String> = with
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            &with[8..],
            [
                "--gunclientdll",
                "C:/launcher/auberean_friends.dll",
                "--gunserver",
                "play.example",
                "--gunport",
                "9000",
                "--gunticket",
                "--gunticketoverride",
                "a1.cGxheWVy.c2VjcmV0"
            ]
        );
        let shown = shown_client_args(&retail_args("h", 1, "player", "secret", Some(&friends)));
        assert!(
            !shown.contains("secret") && !shown.contains("c2VjcmV0"),
            "{shown}"
        );
        // Auberean takes no library: the same options, without it.
        let own = friends_args(
            &Friends {
                dll: None,
                ..friends
            },
            "player",
            "secret",
        );
        assert_eq!(own[0], OsString::from("--gunserver"));
        assert_eq!(FRIENDS_DLL, "auberean_friends.dll");
    }

    #[test]
    fn a_log_s_tail_is_its_last_lines() {
        assert_eq!(tail("a\nb\nc\n", 2), ["b", "c"]);
        assert_eq!(tail("a", 5), ["a"]);
        assert!(tail("", 3).is_empty());
    }
}
