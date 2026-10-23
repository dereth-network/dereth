//! The worlds already running on this computer: Empyrean, ACE and GDLE servers, recognized from
//! their processes, with where each keeps its configuration, its data and its logs.
//!
//! The launcher lists the processes, the ports each listens on and what started each one; this
//! module is what it makes of them, over what it found, so every system's answer is tested on every
//! system. Each server is recognized by its program and laid out as its own sources lay it out:
//!
//! | server | program | configuration | data | log |
//! |---|---|---|---|---|
//! | Empyrean | `empyrean-server` | `--config <file>`, else `empyrean.toml` where it was started, else beside the program | its databases, beside the configuration unless it says otherwise | none of its own (it writes to standard error): the newest `.log` beside the configuration or the program, or in a `logs` folder there |
//! | ACE | `ACE.Server` (or `dotnet ACE.Server.dll`) | `Config.js` where it was started, else beside the program | MySQL, so no folder | the `<file>` of `log4net.config` (where it was started, else beside the program), relative to the program; `ACE_Log.txt` beside the program otherwise |
//! | GDLE | `GDLEnhanced` | `-c`/`--config <file>`, else `server.cfg`, else `debug_server.cfg`, where it was started | `Data` where it was started | the `FILENAME` of `GDLELogging.conf` there, `server.log` otherwise |
//!
//! Each listens on two UDP ports, its game port and the one after it.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A server the launcher recognizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Software {
    Empyrean,
    Ace,
    Gdle,
}

impl Software {
    /// The name a person reads.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Empyrean => "Empyrean",
            Self::Ace => "ACE",
            Self::Gdle => "GDLE",
        }
    }
}

/// A running process, as the system lists it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Process {
    pub pid: u32,
    pub parent: Option<u32>,
    /// Its program: the full path when the system says it, else its file name.
    pub program: PathBuf,
    /// Its command line, the program first, when the system lets it be read.
    pub args: Vec<String>,
    /// The folder it was started in, when the system says it.
    pub cwd: Option<PathBuf>,
}

/// The last part of a path written with either separator, as a server on another system may be
/// named.
fn last_part(s: &str) -> &str {
    s.rsplit(['/', '\\']).next().unwrap_or(s)
}

/// A program's name without its folder or `.exe`, in lower case.
fn stem(program: &str) -> String {
    let name = last_part(program).to_ascii_lowercase();
    name.strip_suffix(".exe")
        .map_or(name.clone(), str::to_owned)
}

/// Which server `p` is, if any.
#[must_use]
pub fn recognize(p: &Process) -> Option<Software> {
    let program = stem(&p.program.to_string_lossy());
    match program.as_str() {
        "empyrean-server" => Some(Software::Empyrean),
        "ace.server" => Some(Software::Ace),
        "dotnet" if ace_assembly(p).is_some() => Some(Software::Ace),
        _ if program.starts_with("gdlenhanced") => Some(Software::Gdle),
        _ => None,
    }
}

/// The `ACE.Server.dll` that `dotnet` runs.
fn ace_assembly(p: &Process) -> Option<&str> {
    p.args
        .iter()
        .skip(1)
        .map(String::as_str)
        .find(|a| last_part(a).eq_ignore_ascii_case("ACE.Server.dll"))
}

/// What the launcher asks of the disk, so the reading here can be tested over files that are not
/// there.
pub trait Files {
    /// A file's text; `None` when it is not there or will not read.
    fn read(&self, path: &Path) -> Option<String>;
    /// Whether `path` is a file.
    fn is_file(&self, path: &Path) -> bool;
    /// The files directly in `dir`, each with when it was last written (seconds since 1970).
    fn list(&self, dir: &Path) -> Vec<(PathBuf, u64)>;
}

/// The disk itself.
#[derive(Debug, Clone, Copy)]
pub struct Disk;

impl Files for Disk {
    fn read(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }
    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }
    fn list(&self, dir: &Path) -> Vec<(PathBuf, u64)> {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                let at = meta
                    .modified()
                    .ok()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()?
                    .as_secs();
                meta.is_file().then(|| (e.path(), at))
            })
            .collect()
    }
}

/// Where a server keeps its things, and what its configuration says about it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Layout {
    /// The server's own folder: its configuration's, else where it was started, else its
    /// program's.
    pub folder: Option<PathBuf>,
    pub config: Option<PathBuf>,
    /// Where its world's own data is kept (Empyrean's databases, GDLE's `Data`); none for ACE, which
    /// keeps it in MySQL.
    pub data: Option<PathBuf>,
    /// The game's data files it reads.
    pub dats: Option<PathBuf>,
    /// Its log file.
    pub log: Option<PathBuf>,
    /// The world's name, as its configuration gives it.
    pub name: Option<String>,
    /// The address it listens on, as its configuration gives it.
    pub host: Option<String>,
    pub port: Option<u16>,
    /// Whether anyone who logs in is given an account.
    pub open_accounts: Option<bool>,
    /// Whether it reads commands on its standard input (Empyrean's `interactive_console`).
    pub console: Option<bool>,
    /// The world's slug on the Dereth Network, as its configuration gives it (Empyrean's
    /// `[federation] slug`).
    pub federation_slug: Option<String>,
}

/// `path` as a server reads it: as written when absolute, else from `base`.
fn resolve(base: Option<&Path>, path: &str) -> Option<PathBuf> {
    let p = PathBuf::from(path);
    // A Windows path (`C:\...`) is absolute wherever it is read.
    let drive = path.as_bytes().get(1) == Some(&b':') && path.as_bytes()[0].is_ascii_alphabetic();
    if p.is_absolute() || drive || path.starts_with(['/', '\\']) {
        return Some(p);
    }
    // A home folder is the server's to expand; the launcher does not guess whose.
    if path.starts_with('~') {
        return None;
    }
    base.map(|b| b.join(p))
}

/// The value after `--config` (also `--config=<file>`), or after any of `names`.
fn flag_value<'a>(args: &'a [String], names: &[&str]) -> Option<&'a str> {
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        if names.contains(&a.as_str()) {
            return it.next().map(String::as_str);
        }
        for name in names {
            if let Some(v) = a.strip_prefix(name).and_then(|r| r.strip_prefix('=')) {
                return Some(v);
            }
        }
    }
    None
}

fn parent(p: &Path) -> Option<PathBuf> {
    p.parent()
        .filter(|d| !d.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

/// The newest file in `dirs` whose name `wanted` takes.
fn newest(files: &dyn Files, dirs: &[PathBuf], wanted: &dyn Fn(&str) -> bool) -> Option<PathBuf> {
    dirs.iter()
        .flat_map(|d| files.list(d))
        .filter(|(p, _)| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| wanted(&n.to_ascii_lowercase()))
        })
        .max_by_key(|(_, at)| *at)
        .map(|(p, _)| p)
}

/// Where the server `software`, running as `p`, keeps its things.
#[must_use]
pub fn layout(software: Software, p: &Process, files: &dyn Files) -> Layout {
    match software {
        Software::Empyrean => empyrean(p, files),
        Software::Ace => ace(p, files),
        Software::Gdle => gdle(p, files),
    }
}

fn empyrean(p: &Process, files: &dyn Files) -> Layout {
    let program_dir = parent(&p.program);
    let config = match flag_value(&p.args, &["--config"]) {
        Some(c) => resolve(p.cwd.as_deref().or(program_dir.as_deref()), c),
        None => [p.cwd.clone(), program_dir.clone()]
            .into_iter()
            .flatten()
            .map(|d| d.join("empyrean.toml"))
            .find(|c| files.is_file(c)),
    };
    let folder = config
        .as_deref()
        .and_then(parent)
        .or_else(|| p.cwd.clone())
        .or_else(|| program_dir.clone());
    let mut out = Layout {
        folder: folder.clone(),
        config: config.clone(),
        ..Layout::default()
    };
    let table = config
        .as_deref()
        .and_then(|c| files.read(c))
        .and_then(|t| t.parse::<toml::Table>().ok())
        .unwrap_or_default();
    let get = |path: &[&str]| -> Option<&toml::Value> {
        let mut at = table.get(path[0])?;
        for key in &path[1..] {
            at = at.get(key)?;
        }
        Some(at)
    };
    let text = |path: &[&str]| get(path).and_then(toml::Value::as_str).map(str::to_owned);
    out.name = text(&["server", "world_name"]);
    out.host = text(&["server", "network", "host"]);
    out.port = get(&["server", "network", "port"])
        .and_then(toml::Value::as_integer)
        .and_then(|p| u16::try_from(p).ok())
        .or(Some(9000));
    out.open_accounts = Some(
        get(&["server", "accounts", "allow_auto_account_creation"])
            .and_then(toml::Value::as_bool)
            .unwrap_or(true),
    );
    out.console = Some(
        get(&["server", "interactive_console"])
            .and_then(toml::Value::as_bool)
            .unwrap_or(true),
    );
    out.federation_slug = text(&["federation", "slug"]).filter(|s| !s.trim().is_empty());
    out.dats = text(&["server", "dat_files_directory"])
        .filter(|d| !d.is_empty())
        .and_then(|d| resolve(folder.as_deref(), &d));
    out.data = match text(&["database", "shard_db_path"]) {
        Some(db) => resolve(folder.as_deref(), &db).and_then(|db| parent(&db)),
        None => folder.clone(),
    };
    let mut dirs: Vec<PathBuf> = [folder, program_dir].into_iter().flatten().collect();
    dirs.dedup();
    let logs: Vec<PathBuf> = dirs.iter().map(|d| d.join("logs")).collect();
    dirs.extend(logs);
    out.log = newest(files, &dirs, &|n| n.ends_with(".log"));
    out
}

/// A JSON document that may carry `//` and `/* */` comments and trailing commas, as ACE's
/// `Config.js` may, made plain JSON.
#[must_use]
pub fn plain_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut last = ' ';
                for next in chars.by_ref() {
                    if last == '*' && next == '/' {
                        break;
                    }
                    last = next;
                }
            }
            _ => out.push(c),
        }
    }
    // A comma before a closing bracket, whatever space lies between.
    let mut cleaned = String::with_capacity(out.len());
    let bytes: Vec<char> = out.chars().collect();
    let mut in_string = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            cleaned.push(c);
            if c == '\\' && i + 1 < bytes.len() {
                cleaned.push(bytes[i + 1]);
                i += 1;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            cleaned.push(c);
        } else if c == ',' {
            let next = bytes[i + 1..].iter().find(|c| !c.is_whitespace());
            if !matches!(next, Some('}' | ']')) {
                cleaned.push(c);
            }
        } else {
            cleaned.push(c);
        }
        i += 1;
    }
    cleaned
}

fn ace(p: &Process, files: &dyn Files) -> Layout {
    let folder = match ace_assembly(p) {
        Some(dll) => resolve(p.cwd.as_deref(), dll).and_then(|d| parent(&d)),
        None => parent(&p.program),
    };
    let here = |name: &str| {
        [p.cwd.clone(), folder.clone()]
            .into_iter()
            .flatten()
            .map(|d| d.join(name))
            .find(|f| files.is_file(f))
    };
    let config = here("Config.js");
    let json: serde_json::Value = config
        .as_deref()
        .and_then(|c| files.read(c))
        .and_then(|t| serde_json::from_str(&plain_json(&t)).ok())
        .unwrap_or_default();
    let server = &json["Server"];
    let log = here("log4net.config")
        .and_then(|c| files.read(&c))
        .and_then(|t| xml_value(&t, "file"))
        .map_or_else(|| Some("ACE_Log.txt".to_owned()), Some)
        .and_then(|f| resolve(folder.as_deref(), &f));
    Layout {
        folder: folder.clone(),
        config,
        data: None,
        dats: server["DatFilesDirectory"]
            .as_str()
            .filter(|d| !d.is_empty())
            .and_then(|d| resolve(folder.as_deref(), d)),
        log,
        name: server["WorldName"].as_str().map(str::to_owned),
        host: server["Network"]["Host"].as_str().map(str::to_owned),
        port: server["Network"]["Port"]
            .as_u64()
            .and_then(|p| u16::try_from(p).ok())
            .or(Some(9000)),
        open_accounts: Some(
            server["Accounts"]["AllowAutoAccountCreation"]
                .as_bool()
                .unwrap_or(true),
        ),
        console: None,
        federation_slug: None,
    }
}

/// The `value` of the first `<name value="..."/>` element in an XML text (log4net's appender
/// file).
fn xml_value(text: &str, name: &str) -> Option<String> {
    let open = format!("<{name}");
    let mut rest = text;
    while let Some(at) = rest.find(&open) {
        rest = &rest[at + open.len()..];
        if !rest.starts_with(|c: char| c.is_whitespace()) {
            continue;
        }
        let tag = &rest[..rest.find('>')?];
        let v = tag.find("value=")?;
        let quoted = &tag[v + "value=".len()..];
        let quote = quoted.chars().next().filter(|q| *q == '"' || *q == '\'')?;
        let body = &quoted[1..];
        return Some(body[..body.find(quote)?].to_owned());
    }
    None
}

/// A GDLE configuration's `key=value` lines. The key is everything before the first `=`, as GDLE
/// reads it; a line that begins with `;`, `#` or `/` is a comment.
fn gdle_value(text: &str, key: &str) -> Option<String> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with([';', '#', '/']))
        .find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k == key).then(|| v.trim().to_owned())
        })
}

fn gdle(p: &Process, files: &dyn Files) -> Layout {
    let folder = p.cwd.clone().or_else(|| parent(&p.program));
    let config = match flag_value(&p.args, &["-c", "--config"]) {
        Some(c) => resolve(folder.as_deref(), c),
        None => ["server.cfg", "debug_server.cfg"]
            .into_iter()
            .filter_map(|n| folder.as_ref().map(|d| d.join(n)))
            .find(|c| files.is_file(c)),
    };
    let text = config
        .as_deref()
        .and_then(|c| files.read(c))
        .unwrap_or_default();
    let log = folder.as_ref().and_then(|d| {
        let named = files
            .read(&d.join("GDLELogging.conf"))
            .and_then(|t| {
                t.lines().find_map(|l| {
                    let (k, v) = l.split_once('=')?;
                    (k.trim() == "FILENAME").then(|| v.trim().trim_matches('"').to_owned())
                })
            })
            .unwrap_or_else(|| "server.log".to_owned());
        resolve(Some(d), &named)
            .filter(|f| files.is_file(f))
            .or_else(|| newest(files, &[d.join("Logs")], &|n| n.starts_with("console_")))
    });
    Layout {
        folder: folder.clone(),
        config,
        data: folder.as_ref().map(|d| d.join("Data")),
        dats: folder.as_ref().map(|d| d.join("Data")),
        log,
        name: gdle_value(&text, "world_name").filter(|n| !n.is_empty()),
        host: Some(gdle_value(&text, "bind_ip").unwrap_or_else(|| "127.0.0.1".into())),
        port: Some(
            gdle_value(&text, "bind_port")
                .and_then(|v| v.parse().ok())
                .unwrap_or(9050),
        ),
        open_accounts: Some(gdle_value(&text, "auto_create_accounts").is_none_or(|v| v != "0")),
        console: None,
        federation_slug: None,
    }
}

// ----- an Empyrean world's admin channel -----------------------------------------------------------

/// The file an Empyrean world writes its admin channel's address and secret to, in its folder.
pub const ADMIN_CHANNEL_FILE: &str = "admin-channel.json";

/// An Empyrean world's admin channel: the console for a program on the same computer. Its
/// address is on loopback; a connection's first line is the secret, then each line sent is a
/// console command, and each line read is a JSON object ([`channel_line`]).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AdminChannel {
    pub address: std::net::SocketAddr,
    pub secret: String,
    /// The world's process, so a file a world that stopped left behind is passed over.
    pub pid: u32,
}

/// The admin channel of the Empyrean world whose folder is `folder` and whose process is `pid`;
/// `None` when it has none, or the file is not that process's, does not read, names an address off
/// this computer, or a secret that is not one.
#[must_use]
pub fn admin_channel(folder: &Path, pid: u32, files: &dyn Files) -> Option<AdminChannel> {
    let text = files.read(&folder.join(ADMIN_CHANNEL_FILE))?;
    let channel: AdminChannel = serde_json::from_str(&text).ok()?;
    (channel.pid == pid
        && channel.address.ip().is_loopback()
        && channel.secret.len() == 64
        && channel.secret.bytes().all(|b| b.is_ascii_hexdigit()))
    .then_some(channel)
}

/// One line an admin channel sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelLine {
    /// The first: the world's software, version and name.
    Hello {
        version: Option<String>,
        world: Option<String>,
    },
    /// A line the world's console showed: a reply or a log line.
    Console(String),
}

/// Reads one line an admin channel sent; `None` for one that does not read.
#[must_use]
pub fn channel_line(text: &str) -> Option<ChannelLine> {
    let v: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    if let Some(line) = v.get("line").and_then(serde_json::Value::as_str) {
        return Some(ChannelLine::Console(line.to_owned()));
    }
    v.get("hello")?;
    let text = |k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    Some(ChannelLine::Hello {
        version: text("version"),
        world: text("world"),
    })
}

/// The game port of a server that its configuration says is `configured` and that listens on
/// `listening`: the configured one when the server listens there or nothing is known of what it
/// listens on, else the first of two neighbours it listens on, else the lowest.
#[must_use]
pub fn game_port(configured: Option<u16>, listening: &[u16]) -> Option<u16> {
    if let Some(c) = configured {
        if listening.is_empty() || listening.contains(&c) {
            return Some(c);
        }
    }
    let mut ports = listening.to_vec();
    ports.sort_unstable();
    ports.dedup();
    ports
        .iter()
        .find(|p| p.checked_add(1).is_some_and(|n| ports.contains(&n)))
        .or(ports.first())
        .copied()
}

/// The address to ask a server listening on `host`: the address itself, or this computer's own
/// for a server listening on every address (or one that does not say).
#[must_use]
pub fn ask_at(host: Option<&str>) -> IpAddr {
    match host.and_then(|h| h.split(',').next()?.trim().parse::<IpAddr>().ok()) {
        Some(ip) if !ip.is_unspecified() => ip,
        _ => IpAddr::V4(Ipv4Addr::LOCALHOST),
    }
}

/// The UDP sockets a Linux `/proc/net/udp` (or `udp6`) lists: each one's local address, its
/// port, and its inode, which names it among a process's open files.
#[must_use]
pub fn proc_net_udp(text: &str) -> Vec<(IpAddr, u16, u64)> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let (addr, port) = fields.get(1)?.split_once(':')?;
            let port = u16::from_str_radix(port, 16).ok()?;
            let inode = fields.get(9)?.parse().ok()?;
            let ip = match addr.len() {
                8 => IpAddr::V4(Ipv4Addr::from(
                    u32::from_str_radix(addr, 16).ok()?.swap_bytes(),
                )),
                32 => {
                    let mut bytes = [0u8; 16];
                    for (i, word) in bytes.chunks_mut(4).enumerate() {
                        let w = u32::from_str_radix(&addr[i * 8..i * 8 + 8], 16).ok()?;
                        word.copy_from_slice(&w.to_le_bytes());
                    }
                    IpAddr::V6(Ipv6Addr::from(bytes))
                }
                _ => return None,
            };
            Some((ip, port, inode))
        })
        .collect()
}

/// The inode of an open file a Linux process lists as a socket (`socket:[12345]`).
#[must_use]
pub fn socket_inode(link: &str) -> Option<u64> {
    link.strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

/// The systemd service a Linux process runs in, from its `/proc/<pid>/cgroup`, and whether it is
/// the person's own (a user service) rather than the system's.
#[must_use]
pub fn systemd_unit(cgroup: &str) -> Option<(String, bool)> {
    cgroup.lines().find_map(|line| {
        let path = line.rsplit(':').next()?;
        let unit = path.rsplit('/').next()?;
        (unit.ends_with(".service") && !unit.starts_with("user@"))
            .then(|| (unit.to_owned(), path.contains("/user@")))
    })
}

/// The UDP ports in `lsof -Fn` output (lines `n*:9000`, `n127.0.0.1:9000`, `n[::1]:9000`).
#[must_use]
pub fn lsof_ports(text: &str) -> Vec<u16> {
    text.lines()
        .filter_map(|l| l.strip_prefix('n')?.rsplit(':').next()?.parse().ok())
        .collect()
}

/// The service a process runs under: its own, or that of the process that started it, or of that
/// one's starter, as far as `depth` steps up. `parents` maps a process to its starter, and
/// `services` a service's process to the service's name.
#[must_use]
pub fn service_of(
    pid: u32,
    parents: &dyn Fn(u32) -> Option<u32>,
    services: &dyn Fn(u32) -> Option<String>,
    depth: usize,
) -> Option<String> {
    let mut at = pid;
    for _ in 0..=depth {
        if let Some(name) = services(at) {
            return Some(name);
        }
        at = parents(at).filter(|p| *p != at && *p != 0)?;
    }
    None
}

/// Where a server's log ends: its last `lines` lines, from the last `bytes` of `text`.
#[must_use]
pub fn tail(text: &str, lines: usize) -> Vec<String> {
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..]
        .iter()
        .map(|l| l.trim_end_matches('\r').to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Files that are not on any disk.
    #[derive(Default)]
    struct Fake(BTreeMap<PathBuf, (String, u64)>);

    impl Fake {
        fn with(mut self, path: &str, text: &str, at: u64) -> Self {
            self.0.insert(PathBuf::from(path), (text.to_owned(), at));
            self
        }
    }

    impl Files for Fake {
        fn read(&self, path: &Path) -> Option<String> {
            self.0.get(path).map(|(t, _)| t.clone())
        }
        fn is_file(&self, path: &Path) -> bool {
            self.0.contains_key(path)
        }
        fn list(&self, dir: &Path) -> Vec<(PathBuf, u64)> {
            self.0
                .iter()
                .filter(|(p, _)| p.parent() == Some(dir))
                .map(|(p, (_, at))| (p.clone(), *at))
                .collect()
        }
    }

    fn process(program: &str, args: &[&str], cwd: Option<&str>) -> Process {
        Process {
            pid: 42,
            parent: None,
            program: program.into(),
            args: std::iter::once(program)
                .chain(args.iter().copied())
                .map(str::to_owned)
                .collect(),
            cwd: cwd.map(PathBuf::from),
        }
    }

    #[test]
    fn each_server_is_recognized_by_its_program_and_nothing_else_is() {
        for (program, args, software) in [
            (
                "/srv/emp/empyrean-server",
                &[][..],
                Some(Software::Empyrean),
            ),
            (
                r"C:\Worlds\Empyrean-Server.exe",
                &[],
                Some(Software::Empyrean),
            ),
            (r"C:\ACE\ACE.Server.exe", &[], Some(Software::Ace)),
            ("/usr/bin/dotnet", &["ACE.Server.dll"], Some(Software::Ace)),
            (
                r"C:\dotnet\dotnet.exe",
                &[r"C:\ACE\ace.server.dll"],
                Some(Software::Ace),
            ),
            (r"C:\GDLE\Bin\GDLEnhanced.exe", &[], Some(Software::Gdle)),
            (
                r"C:\GDLE\Bin\GDLEnhanced - Debug.exe",
                &[],
                Some(Software::Gdle),
            ),
            (
                "/home/gdle/GDLEnhanced",
                &["--config", "server.cfg"],
                Some(Software::Gdle),
            ),
            ("/usr/bin/dotnet", &["Other.dll"], None),
            (r"C:\Windows\explorer.exe", &[], None),
            ("/usr/bin/empyrean-import", &["fetch"], None),
        ] {
            assert_eq!(
                recognize(&process(program, args, None)),
                software,
                "{program}"
            );
        }
    }

    #[test]
    fn an_empyrean_layout_follows_its_named_configuration_and_its_paths() {
        let files = Fake::default()
            .with(
                "/srv/world/empyrean.toml",
                "[server]\nworld_name = 'Lakeside'\ndat_files_directory = 'dats'\ninteractive_console = false\n[server.network]\nhost = '0.0.0.0'\nport = 9050\n[server.accounts]\nallow_auto_account_creation = false\n[database]\nshard_db_path = '/var/lib/emp/shard.db'\n",
                1,
            )
            .with("/srv/world/empyrean.log", "", 5)
            .with("/srv/world/logs/older.log", "", 2)
            .with("/srv/world/notes.txt", "", 9);
        let p = process(
            "/opt/emp/empyrean-server",
            &["--config", "/srv/world/empyrean.toml"],
            Some("/"),
        );
        let l = layout(Software::Empyrean, &p, &files);
        assert_eq!(
            l.config.as_deref(),
            Some(Path::new("/srv/world/empyrean.toml"))
        );
        assert_eq!(l.folder.as_deref(), Some(Path::new("/srv/world")));
        assert_eq!(l.name.as_deref(), Some("Lakeside"));
        assert_eq!(l.port, Some(9050));
        assert_eq!(l.host.as_deref(), Some("0.0.0.0"));
        assert_eq!(l.open_accounts, Some(false));
        assert_eq!(l.console, Some(false));
        assert_eq!(l.dats.as_deref(), Some(Path::new("/srv/world/dats")));
        assert_eq!(l.data.as_deref(), Some(Path::new("/var/lib/emp")));
        assert_eq!(l.log.as_deref(), Some(Path::new("/srv/world/empyrean.log")));
    }

    #[test]
    fn an_empyrean_without_a_named_configuration_reads_the_one_it_would_find() {
        let files = Fake::default().with("/opt/emp/empyrean.toml", "[server]\n", 1);
        let p = process("/opt/emp/empyrean-server", &[], Some("/home/me"));
        let l = layout(Software::Empyrean, &p, &files);
        assert_eq!(
            l.config.as_deref(),
            Some(Path::new("/opt/emp/empyrean.toml"))
        );
        assert_eq!(l.port, Some(9000), "the default port");
        assert_eq!(l.console, Some(true));
        assert_eq!(l.data.as_deref(), Some(Path::new("/opt/emp")));
        assert_eq!(l.log, None);
        // `--config=<file>`, relative to where it was started.
        let p = process(
            "/opt/emp/empyrean-server",
            &["--config=w/e.toml"],
            Some("/home/me"),
        );
        let l = layout(Software::Empyrean, &p, &Fake::default());
        assert_eq!(l.config.as_deref(), Some(Path::new("/home/me/w/e.toml")));
    }

    #[test]
    fn an_ace_layout_reads_its_commented_configuration_and_its_log_appender() {
        let files = Fake::default()
            .with(
                "/ace/Config.js",
                "{\n  // the world\n  \"Server\": {\n    \"WorldName\": \"Frostfell // test\", /* inline */\n    \"Network\": { \"Host\": \"0.0.0.0\", \"Port\": 9100, },\n    \"Accounts\": { \"AllowAutoAccountCreation\": false },\n    \"DatFilesDirectory\": \"c:\\\\ACE\\\\Dats\\\\\",\n  },\n}\n",
                1,
            )
            .with(
                "/ace/log4net.config",
                "<log4net><appender name=\"File\"><file value=\"Logs/ACE.txt\" /></appender></log4net>",
                1,
            );
        let p = process("/usr/bin/dotnet", &["ACE.Server.dll"], Some("/ace"));
        let l = layout(Software::Ace, &p, &files);
        assert_eq!(l.folder.as_deref(), Some(Path::new("/ace")));
        assert_eq!(l.config.as_deref(), Some(Path::new("/ace/Config.js")));
        assert_eq!(l.name.as_deref(), Some("Frostfell // test"));
        assert_eq!(l.port, Some(9100));
        assert_eq!(l.open_accounts, Some(false));
        assert_eq!(l.dats.as_deref(), Some(Path::new(r"c:\ACE\Dats\")));
        assert_eq!(l.data, None, "ACE keeps its data in MySQL");
        assert_eq!(l.log.as_deref(), Some(Path::new("/ace/Logs/ACE.txt")));
        // Without log4net.config the default appender's file, beside the program.
        let p = process("/ace/ACE.Server", &[], None);
        let l = layout(Software::Ace, &p, &Fake::default());
        assert_eq!(l.log.as_deref(), Some(Path::new("/ace/ACE_Log.txt")));
        assert_eq!(l.port, Some(9000));
    }

    #[test]
    fn a_gdle_layout_reads_its_key_value_configuration_from_where_it_was_started() {
        let files = Fake::default()
            .with(
                "/gdle/server.cfg",
                "; comment\nbind_ip=0.0.0.0\nbind_port=9000\nworld_name=GDLE\nauto_create_accounts=0\n",
                1,
            )
            .with("/gdle/GDLELogging.conf", "* GLOBAL:\n   FILENAME = \"server.log\"\n", 1)
            .with("/gdle/server.log", "", 1);
        let p = process(
            "/home/gdle/GDLEnhanced",
            &["--config", "server.cfg"],
            Some("/gdle"),
        );
        let l = layout(Software::Gdle, &p, &files);
        assert_eq!(l.config.as_deref(), Some(Path::new("/gdle/server.cfg")));
        assert_eq!(l.port, Some(9000));
        assert_eq!(l.name.as_deref(), Some("GDLE"));
        assert_eq!(l.open_accounts, Some(false));
        assert_eq!(l.data.as_deref(), Some(Path::new("/gdle/Data")));
        assert_eq!(l.log.as_deref(), Some(Path::new("/gdle/server.log")));
        // A key with a space before its `=` is not the key, as GDLE reads it; its default holds.
        let spaced = Fake::default().with("/gdle/debug_server.cfg", "bind_port = 9000\n", 1);
        let p = process("/gdle/GDLEnhanced", &[], Some("/gdle"));
        let l = layout(Software::Gdle, &p, &spaced);
        assert_eq!(
            l.config.as_deref(),
            Some(Path::new("/gdle/debug_server.cfg"))
        );
        assert_eq!(l.port, Some(9050));
    }

    #[test]
    fn the_game_port_is_the_configured_one_or_the_first_of_a_pair() {
        assert_eq!(game_port(Some(9000), &[]), Some(9000));
        assert_eq!(game_port(Some(9000), &[9001, 9000, 9100]), Some(9000));
        assert_eq!(game_port(Some(9000), &[9101, 9100]), Some(9100));
        assert_eq!(game_port(None, &[5353, 9051, 9050]), Some(9050));
        assert_eq!(game_port(None, &[5353]), Some(5353));
        assert_eq!(game_port(None, &[]), None);
    }

    #[test]
    fn a_server_on_every_address_is_asked_at_this_computers_own() {
        assert_eq!(ask_at(Some("0.0.0.0")), IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(ask_at(None), IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(
            ask_at(Some("192.0.2.4, 0.0.0.0")),
            "192.0.2.4".parse::<IpAddr>().unwrap()
        );
        assert_eq!(ask_at(Some("::")), IpAddr::V4(Ipv4Addr::LOCALHOST));
    }

    #[test]
    fn linux_socket_tables_and_service_groups_are_read() {
        let udp = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode ref pointer drops\n\
                   1: 00000000:2328 00000000:0000 07 00000000:00000000 00:00000000 00000000  1000        0 123456 2 0000000000000000 0\n\
                   2: 0100007F:2329 00000000:0000 07 00000000:00000000 00:00000000 00000000  1000        0 123457 2 0000000000000000 0\n";
        assert_eq!(
            proc_net_udp(udp),
            [
                (IpAddr::V4(Ipv4Addr::UNSPECIFIED), 9000, 123_456),
                (IpAddr::V4(Ipv4Addr::LOCALHOST), 9001, 123_457),
            ]
        );
        let udp6 = "header\n 0: 00000000000000000000000001000000:2328 00000000000000000000000000000000:0000 07 0:0 0:0 0 1000 0 99 2 0 0\n";
        assert_eq!(
            proc_net_udp(udp6),
            [(IpAddr::V6(Ipv6Addr::LOCALHOST), 9000, 99)]
        );
        assert_eq!(socket_inode("socket:[123456]"), Some(123_456));
        assert_eq!(socket_inode("/dev/null"), None);
        assert_eq!(
            systemd_unit("0::/system.slice/empyrean.service\n"),
            Some(("empyrean.service".into(), false))
        );
        assert_eq!(
            systemd_unit("0::/user.slice/user-1000.slice/user@1000.service/app.slice/dereth-host-1.service\n"),
            Some(("dereth-host-1.service".into(), true))
        );
        assert_eq!(
            systemd_unit(
                "0::/user.slice/user-1000.slice/user@1000.service/app.slice/vte-spawn-1.scope\n"
            ),
            None,
            "a terminal's process is no service"
        );
        assert_eq!(
            lsof_ports("p42\nf5\nn*:9000\nf6\nn127.0.0.1:9001\n"),
            [9000, 9001]
        );
    }

    #[test]
    fn a_service_is_found_up_the_line_of_processes_that_started_one_another() {
        let parents = |p: u32| match p {
            30 => Some(20),
            20 => Some(10),
            10 => Some(4),
            _ => None,
        };
        let services = |p: u32| (p == 10).then(|| "empyrean".to_owned());
        assert_eq!(
            service_of(30, &parents, &services, 3).as_deref(),
            Some("empyrean")
        );
        assert_eq!(service_of(30, &parents, &services, 1), None, "too far up");
        assert_eq!(service_of(4, &parents, &services, 3), None);
    }

    #[test]
    fn comments_and_trailing_commas_are_taken_out_and_strings_are_kept() {
        let text = "{\"a\": \"x // y, }\", // note\n \"b\": [1, 2,], /* c, */ }";
        let v: serde_json::Value = serde_json::from_str(&plain_json(text)).unwrap();
        assert_eq!(v["a"], "x // y, }");
        assert_eq!(v["b"], serde_json::json!([1, 2]));
    }

    #[test]
    fn a_log_tail_is_its_last_lines() {
        assert_eq!(tail("a\r\nb\nc\n", 2), ["b", "c"]);
        assert_eq!(tail("", 5), Vec::<String>::new());
    }

    #[test]
    fn an_empyrean_worlds_network_slug_is_read_from_its_configuration() {
        let files = Fake::default()
            .with(
                "/srv/emp/empyrean.toml",
                "[server]\nworld_name = 'Coldeve'\n[federation]\nslug = 'coldeve'\n",
                1,
            )
            .with(
                "/srv/other/empyrean.toml",
                "[server]\n[federation]\nslug = ''\n",
                1,
            );
        let at = |cwd: &str| {
            layout(
                Software::Empyrean,
                &process("/srv/bin/empyrean-server", &[], Some(cwd)),
                &files,
            )
        };
        assert_eq!(at("/srv/emp").federation_slug.as_deref(), Some("coldeve"));
        assert_eq!(
            at("/srv/other").federation_slug,
            None,
            "a blank slug is none"
        );
    }

    #[test]
    fn an_admin_channel_is_read_only_for_the_worlds_own_process_on_loopback() {
        let secret = "0123456789abcdef".repeat(4);
        let file = |address: &str, secret: &str, pid: u32| {
            format!(r#"{{"address":"{address}","secret":"{secret}","pid":{pid}}}"#)
        };
        let folder = Path::new("/srv/emp");
        let read = |text: String, pid: u32| {
            let files = Fake::default().with("/srv/emp/admin-channel.json", &text, 1);
            admin_channel(folder, pid, &files)
        };
        let channel = read(file("127.0.0.1:50123", &secret, 42), 42).expect("its channel");
        assert_eq!(channel.address.to_string(), "127.0.0.1:50123");
        assert_eq!(channel.secret, secret);
        assert_eq!(
            read(file("127.0.0.1:50123", &secret, 42), 43),
            None,
            "another process's"
        );
        assert_eq!(
            read(file("192.0.2.5:50123", &secret, 42), 42),
            None,
            "an address off this computer"
        );
        assert_eq!(read(file("127.0.0.1:50123", "short", 42), 42), None);
        assert_eq!(read("not json".into(), 42), None);
        assert_eq!(
            admin_channel(folder, 42, &Fake::default()),
            None,
            "none at all"
        );
    }

    #[test]
    fn an_admin_channels_lines_are_its_hello_and_the_consoles_lines() {
        assert_eq!(
            channel_line(r#"{"hello":"empyrean","version":"0.3.3","world":"Coldeve"}"#),
            Some(ChannelLine::Hello {
                version: Some("0.3.3".into()),
                world: Some("Coldeve".into()),
            })
        );
        assert_eq!(
            channel_line(r#"{"line":"Saved."}"#),
            Some(ChannelLine::Console("Saved.".into()))
        );
        assert_eq!(channel_line("Saved."), None);
        assert_eq!(channel_line(r#"{"other":1}"#), None);
    }
}
