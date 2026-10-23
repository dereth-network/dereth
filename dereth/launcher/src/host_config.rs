//! The settings a host changes on a world's Settings screen, read from and written into its
//! `empyrean.toml` with everything else in the file kept: its comments, its other keys and tables,
//! and a key's own comment when its value changes. A key the file does not have is written only
//! when its value is not the server's default, so a new world's file stays short.
//!
//! The era chooses the world database too: the end of retail plays `world.pack`, Infiltration
//! `world.pack.infiltration`, each fetched by Get world database for its era. A world whose
//! configuration names a world database of its own keeps it.
//!
//! The era also chooses the game data the world is drawn from, by the client's own rule
//! ([`dereth_launch::eras::required_set`]): an era before Throne of Destiny plays the Classic
//! files, any other the Modern ones. So the world and the players' clients never disagree.

use dereth_launch::datset::{self, DatRole, SetKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use toml_edit::{value, DocumentMut};

/// The eras a world can play, as `[era] profile` names them.
pub const ERAS: [&str; 2] = ["eor", "infiltration"];

/// The log levels `server.log_level` takes.
pub const LOG_LEVELS: [&str; 5] = ["error", "warn", "info", "debug", "trace"];

/// `[server.websocket]`: the endpoint browser players connect to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WebSocket {
    pub enabled: bool,
    /// `<ip>:<port>`.
    pub listen: String,
    /// The PEM certificate chain and its key, for `wss://`; both or neither.
    pub tls_certificate: String,
    pub tls_private_key: String,
    /// A proxy in front handles the secure connection, so plain `ws://` may be served.
    pub behind_tls_proxy: bool,
    /// The web pages allowed to connect, by origin.
    pub allowed_origins: Vec<String>,
    /// The address browsers connect to; empty works it out from `listen`.
    pub public_url: String,
}
impl Default for WebSocket {
    fn default() -> Self {
        Self {
            enabled: false,
            listen: "0.0.0.0:9443".into(),
            tls_certificate: String::new(),
            tls_private_key: String::new(),
            behind_tls_proxy: false,
            allowed_origins: Vec::new(),
            public_url: String::new(),
        }
    }
}

/// `[federation]`: the world's link to the Dereth Network. Its key is not a setting: the host
/// pastes it, and [`save_network_key`] keeps it in the world's folder.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Federation {
    pub enabled: bool,
    /// The world's name on the network's website, apart from its own name.
    pub slug: String,
}

/// The file in a world's folder that holds its Dereth Network key, as `[federation] key_file`
/// names it.
pub const NETWORK_KEY_FILE: &str = "network.key";

/// The first port a world's status endpoint may take.
const FIRST_STATUS_PORT: u16 = 9100;

/// `[dat_overlay]`'s patching: whether a Dereth client that joins without the world's data
/// overlay is sent it, and how fast.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Patching {
    /// `patching`.
    pub enabled: bool,
    pub records_per_minute: i64,
    pub bytes_per_second: i64,
}
impl Default for Patching {
    fn default() -> Self {
        Self {
            enabled: true,
            records_per_minute: 60_000,
            bytes_per_second: 1_048_576,
        }
    }
}

/// The world database each era plays, in the world's folder.
#[must_use]
pub fn era_pack(era: &str) -> &'static str {
    if era == "infiltration" {
        "world.pack.infiltration"
    } else {
        "world.pack"
    }
}

/// The ACE world release a world database was made from: its world, by the era it is, and the
/// release's tag.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WorldBase {
    /// `eor` for ACE's end-of-retail world, `infiltration` for its February 2005 world.
    pub era: &'static str,
    /// The release, as its tag names it: `v0.9.295`.
    pub version: String,
}

/// The start of each ACE world release's dump file name, and the era that world is.
const BASE_DUMPS: [(&str, &str); 2] = [
    ("ACE-World-Database-", "eor"),
    ("ACE-World-16PY-db-", "infiltration"),
];

/// The release a world database dump's file name names: ACE's end-of-retail world
/// (`ACE-World-Database-<tag>.sql`) or its February 2005 world (`ACE-World-16PY-db-<tag>.sql`).
/// `None` for any other name.
#[must_use]
pub fn world_base(dump: &str) -> Option<WorldBase> {
    BASE_DUMPS.iter().find_map(|(prefix, era)| {
        let tag = dump.strip_prefix(prefix)?;
        let tag = tag.strip_suffix(".sql.zip").or(tag.strip_suffix(".sql"))?;
        let digits = tag.strip_prefix('v')?;
        let parts: Vec<&str> = digits.split('.').collect();
        (parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.len() <= 9 && p.bytes().all(|b| b.is_ascii_digit())))
        .then(|| WorldBase {
            era,
            version: tag.to_owned(),
        })
    })
}

/// The dump file a world database was made from, as the server's import tool says when it fetches
/// one (`… is already cached: <folder>/ACE-World-Database-v0.9.295.sql`, or the release it
/// downloads and caches): the first such name it prints.
#[must_use]
pub fn fetched_dump(said: &str) -> Option<String> {
    BASE_DUMPS
        .iter()
        .filter_map(|(prefix, _)| {
            let at = said.find(prefix)?;
            let rest = &said[at..];
            let end = rest.find(".sql")? + ".sql".len();
            Some((at, rest[..end].to_owned()))
        })
        .min_by_key(|(at, _)| *at)
        .map(|(_, name)| name)
        .filter(|name| world_base(name).is_some())
}

/// The file beside a world database that names the dump it was made from: `<pack>.base`.
#[must_use]
pub fn base_record(pack: &Path) -> PathBuf {
    let mut name = pack.as_os_str().to_owned();
    name.push(".base");
    PathBuf::from(name)
}

/// The ACE world release the world database `pack` was made from, as its [`base_record`] says.
#[must_use]
pub fn recorded_base(pack: &Path) -> Option<WorldBase> {
    world_base(std::fs::read_to_string(base_record(pack)).ok()?.trim())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// `server.world_name`.
    pub name: String,
    /// `server.dat_files_directory`.
    pub dats: PathBuf,
    /// `server.network.host`: every address (other players allowed) or this computer's alone.
    pub bind: String,
    /// `server.network.port`: the game port; the world takes the one after it too.
    pub port: u16,
    /// The status endpoint's port, on this computer (`server.status_address`), where the launcher
    /// counts the world's players. The launcher chooses it when it makes the world
    /// ([`free_status_port`]) and keeps it; the page never sees it or sets it.
    #[serde(skip)]
    pub status_port: u16,
    /// `server.accounts.allow_auto_account_creation`.
    pub accounts: bool,
    /// `[era] profile`.
    pub era: String,
    /// `server.character_screen_message`: empty says `Welcome to <name>!`. Its lines are written
    /// as `\n` in the file.
    pub welcome: String,
    /// `server.network.maximum_allowed_sessions`.
    pub max_players: i64,
    /// `server.network.maximum_allowed_sessions_per_ip_address`: -1 for no limit.
    pub max_per_address: i64,
    /// `server.shutdown_interval`: the seconds of warning before a shutdown.
    pub shutdown_warning: i64,
    /// `server.log_level`.
    pub log_level: String,
    /// The `[era]` systems the world turns on or off over its era's own; a system not named here
    /// has the era's value.
    pub systems: BTreeMap<String, bool>,
    /// `[server.websocket]`.
    pub websocket: WebSocket,
    /// `[federation]`.
    pub federation: Federation,
    /// `[dat_overlay]`'s patching.
    pub patching: Patching,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            name: "My world".into(),
            dats: PathBuf::new(),
            bind: "127.0.0.1".into(),
            port: 9000,
            status_port: FIRST_STATUS_PORT,
            accounts: true,
            era: "eor".into(),
            welcome: String::new(),
            max_players: 128,
            max_per_address: -1,
            shutdown_warning: 60,
            log_level: "info".into(),
            systems: BTreeMap::new(),
            websocket: WebSocket::default(),
            federation: Federation::default(),
            patching: Patching::default(),
        }
    }
}

/// The systems an `[era]` table may turn on or off, as the client's era table names them.
fn system_names() -> Vec<&'static str> {
    dereth_launch::eras::features()
        .into_iter()
        .map(|f| f.name)
        .collect()
}

/// Whether `slug` can be a world's name on the Dereth Network: 2 to 12 of `a-z` and `0-9`, with
/// single hyphens between them.
fn is_slug(slug: &str) -> bool {
    (2..=12).contains(&slug.chars().count())
        && slug.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// Whether `origin` is a web page's origin: `http://` or `https://` and a host, with no path.
fn is_origin(origin: &str) -> bool {
    let rest = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"));
    rest.is_some_and(|host| {
        !host.is_empty()
            && !host.contains('/')
            && !host.chars().any(|c| c.is_whitespace() || c.is_control())
    })
}

/// Whether `text` holds no control character but, where `lines` allows, the end of a line.
fn plain_text(text: &str, lines: bool) -> bool {
    !text
        .chars()
        .any(|c| c.is_control() && !(lines && c == '\n'))
}

/// Whether `key` is a Dereth Network key as Empyrean reads one from its key file: `drth_` and 43
/// more of `A-Z`, `a-z`, `0-9`, `-` and `_`.
fn is_network_key(key: &str) -> bool {
    key.len() == 48
        && key.starts_with("drth_")
        && key[5..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The key the host pasted, without the space around it, when it is a Dereth Network key.
///
/// # Errors
/// Why it is not, in words for a person. The paste itself is never in it.
pub fn network_key(pasted: &str) -> Result<&str, String> {
    let key = pasted.trim();
    if key.is_empty() {
        return Err("Paste the world's key from the Dereth Network first.".into());
    }
    if !is_network_key(key) {
        return Err("That is not a Dereth Network key. Paste the whole key the network gave you: it starts with drth_ and is 48 characters long.".into());
    }
    Ok(key)
}

/// Whether the file at `path` holds a Dereth Network key, as Empyrean reads it.
fn holds_network_key(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() <= 1024)
        && std::fs::read_to_string(path).is_ok_and(|text| is_network_key(text.trim()))
}

/// Keeps the world's network key, `pasted`, in [`NETWORK_KEY_FILE`] in its folder `dir`, readable
/// by this user alone where the system allows: on Unix the file is its owner's only; on Windows it
/// takes the permissions of the world's folder, which is in the user's own profile (and open to
/// the world's service account while the world runs in the background). Returns the
/// configuration `text` with `[federation] key_file` naming that file, its comments kept.
///
/// # Errors
/// Why the key or the configuration could not be kept, in words for a person. The key itself is
/// never in it.
pub fn save_network_key(dir: &Path, text: &str, pasted: &str) -> Result<String, String> {
    let key = network_key(pasted)?;
    let mut doc = text.parse::<DocumentMut>().map_err(|e| e.to_string())?;
    if doc.get("federation").is_none() {
        doc["federation"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    if !doc["federation"].is_table() {
        return Err("federation must be a table.".into());
    }
    set(&mut doc["federation"]["key_file"], NETWORK_KEY_FILE);
    write_private(&dir.join(NETWORK_KEY_FILE), format!("{key}\n").as_bytes())
        .map_err(|e| format!("Could not save the key: {e}"))?;
    Ok(doc.to_string())
}

/// Writes `bytes` to `path` whole, through a new file beside it that only its owner may read on
/// Unix.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut temp = path.as_os_str().to_owned();
    temp.push(".new");
    let temp = PathBuf::from(temp);
    let _ = std::fs::remove_file(&temp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let written = options.open(&temp).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    let result = written.and_then(|()| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// The port a new world's status endpoint takes: the first from 9100 up that no other world of
/// the launcher's names (`taken`) and nothing on this computer holds now (`free`).
pub fn free_status_port(taken: &[u16], free: impl Fn(u16) -> bool) -> Option<u16> {
    (FIRST_STATUS_PORT..u16::MAX).find(|p| !taken.contains(p) && free(*p))
}

impl WebSocket {
    fn validate(&self) -> Result<(), String> {
        let listen = self.listen.parse::<SocketAddr>().map_err(|_| {
            "Enter the address browser players connect to as <ip>:<port>, such as 0.0.0.0:9443."
        })?;
        if self.tls_certificate.is_empty() != self.tls_private_key.is_empty() {
            return Err("Give both the certificate and its key, or neither.".into());
        }
        if !plain_text(&self.tls_certificate, false) || !plain_text(&self.tls_private_key, false) {
            return Err("Enter the certificate and key as paths to their files.".into());
        }
        if self.enabled
            && self.tls_certificate.is_empty()
            && !self.behind_tls_proxy
            && !listen.ip().is_loopback()
        {
            return Err("Playing in a browser needs a secure connection: give a certificate and its key, or check Behind a TLS proxy.".into());
        }
        if let Some(bad) = self.allowed_origins.iter().find(|o| !is_origin(o)) {
            return Err(format!(
                "{bad} is not a web page's address. Write each as https://play.example.org, one a line."
            ));
        }
        let scheme = self.public_url.starts_with("wss://") || self.public_url.starts_with("ws://");
        if (!self.public_url.is_empty() && !scheme)
            || !plain_text(&self.public_url, false)
            || self.public_url.contains(' ')
        {
            return Err("The public address starts with wss://.".into());
        }
        Ok(())
    }
}

impl Federation {
    fn validate(&self) -> Result<(), String> {
        if (self.enabled || !self.slug.is_empty()) && !is_slug(&self.slug) {
            return Err("Give the world's slug: 2 to 12 lowercase letters and digits, with single hyphens between them.".into());
        }
        Ok(())
    }
}

/// Whether the folder `dats` holds the data files a world of `era` is drawn from, by the client's
/// rule: the Classic pair (`portal.dat` and `cell.dat`) for an era before Throne of Destiny, the
/// Modern files for any other.
///
/// # Errors
/// What to choose instead, in words for a person.
pub fn check_game_data(era: &str, dats: &Path) -> Result<(), String> {
    match dereth_launch::eras::required_set(Some(era)) {
        SetKind::Classic => {
            if datset::is_classic_pair(&datset::scan_classic_dir(dats)) {
                Ok(())
            } else {
                Err(format!(
                    "{} is drawn on the land from before Throne of Destiny: choose Classic data files (portal.dat and cell.dat) for it.",
                    dereth_launch::eras::label(Some(era))
                ))
            }
        }
        SetKind::Modern => {
            let files = datset::scan_dir(dats);
            if [DatRole::Portal, DatRole::Cell, DatRole::Local]
                .iter()
                .all(|r| files.iter().any(|f| f.role == *r))
            {
                Ok(())
            } else {
                Err(format!(
                    "{} plays the Modern data files (client_portal.dat and the rest): choose those for it.",
                    dereth_launch::eras::label(Some(era))
                ))
            }
        }
    }
}

/// What a world plays besides its settings, as its configuration and folder say.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Plays {
    /// Which world database: `era` (its era's own, `world.pack` or `world.pack.infiltration`),
    /// `lyceum` (one a Lyceum world was published as) or `own` (another its configuration names).
    pub database: &'static str,
    /// That world database, as the configuration names it.
    pub pack: String,
    /// The data overlay folder it plays, when it plays one.
    pub dat_overlay: Option<String>,
    /// The content overlay, where changes to content made in the world are kept, when it has one.
    pub content_overlay: Option<String>,
    /// When the Lyceum world it plays was published, in seconds since 1970.
    pub published: Option<u64>,
    /// Its Dereth Network key is kept: the file `[federation] key_file` names holds one.
    pub network_key: bool,
    /// The ACE world release its world database was made from, when that is known: the one Get
    /// world database fetched, or the one a published Lyceum world was built over.
    pub base: Option<WorldBase>,
    /// The releases the eras' own world databases in its folder were made from, those known.
    pub era_bases: Vec<WorldBase>,
}

/// What the world in `dir`, configured by `text`, plays. A Lyceum world is one whose data overlay
/// sits beside a `publication.json`.
#[must_use]
pub fn plays(text: &str, dir: &Path) -> Plays {
    let doc = text.parse::<DocumentMut>().unwrap_or_default();
    let string = |table: Option<&toml_edit::Item>, key: &str| {
        table
            .and_then(|t| t.get(key))
            .and_then(toml_edit::Item::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let server = doc.get("server");
    let resolve = |path: &str| {
        let path = Path::new(path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            dir.join(path)
        }
    };
    let pack = string(server, "world_pack_path").unwrap_or_else(|| "world.pack".into());
    let dat_overlay = string(doc.get("dat_overlay"), "path");
    let published = dat_overlay
        .as_deref()
        .and_then(|o| resolve(o).parent().map(|p| p.join("publication.json")))
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    let plays_era_pack = ERAS
        .iter()
        .any(|e| pack == era_pack(e) || pack.strip_prefix("./") == Some(era_pack(e)));
    let key_file =
        string(doc.get("federation"), "key_file").unwrap_or_else(|| NETWORK_KEY_FILE.into());
    // A published Lyceum world names the dump it was built over in its publication.
    let published_base = || {
        let overlay = resolve(dat_overlay.as_deref()?);
        let text = std::fs::read(overlay.parent()?.join("publication.json")).ok()?;
        let publication: serde_json::Value = serde_json::from_slice(&text).ok()?;
        world_base(publication.get("base")?.as_str()?)
    };
    let base = if published.is_some() {
        published_base()
    } else {
        recorded_base(&resolve(&pack))
    };
    Plays {
        database: if published.is_some() {
            "lyceum"
        } else if plays_era_pack {
            "era"
        } else {
            "own"
        },
        base,
        era_bases: ERAS
            .iter()
            .map(|e| (e, dir.join(era_pack(e))))
            .filter(|(_, pack)| pack.is_file())
            .filter_map(|(e, pack)| recorded_base(&pack).filter(|b| b.era == *e))
            .collect(),
        pack,
        dat_overlay,
        content_overlay: string(server, "world_overlay_path"),
        published,
        network_key: holds_network_key(&resolve(&key_file)),
    }
}

/// Whether `next` may replace the settings the configuration `text` of the world in `dir` holds.
/// A world links to the Dereth Network only once its key is kept. A world that plays a data
/// overlay, as a published Lyceum world does, keeps the era and game data the overlay was made
/// for; any other world plays the game data its era is drawn from ([`check_game_data`]).
///
/// # Errors
/// Why not, in words for a person.
pub fn check_change(text: &str, dir: &Path, next: &Settings) -> Result<(), String> {
    let before = Settings::from_config(text)?;
    if next.federation.enabled && !plays(text, dir).network_key {
        return Err("Save the world's key before linking it to the Dereth Network.".into());
    }
    if dat_overlay(text).is_some() {
        if before.era != next.era || before.dats != next.dats {
            return Err("The Lyceum world this world plays was built for its era and game data, so those stay as they are.".into());
        }
        return Ok(());
    }
    check_game_data(&next.era, &next.dats)
}

/// The configuration `text` of the world in `dir` with the settings `next` saved over it, as
/// [`check_change`] allows, the status endpoint kept where the launcher put it. A published Lyceum
/// world keeps sending its data to Dereth players: that is how they get it.
///
/// # Errors
/// Why not, in words for a person.
pub fn save_over(text: &str, dir: &Path, next: &Settings) -> Result<String, String> {
    let mut next = next.clone();
    next.status_port = Settings::from_config(text)?.status_port;
    if plays(text, dir).published.is_some() {
        next.patching.enabled = true;
    }
    check_change(text, dir, &next)?;
    next.update_config(text)
}

/// The data overlay folder `text` names, when it names one.
fn dat_overlay(text: &str) -> Option<String> {
    text.parse::<DocumentMut>()
        .ok()?
        .get("dat_overlay")?
        .get("path")?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty()
            || self.name.len() > 128
            || self.name.chars().any(char::is_control)
        {
            return Err("Give the world a name of up to 128 characters.".into());
        }
        self.bind
            .parse::<IpAddr>()
            .map_err(|_| "Enter an IP address to listen on.")?;
        if self.port == 0 || self.port == u16::MAX {
            return Err(
                "Choose a port from 1 to 65534: the world takes it and the one after it.".into(),
            );
        }
        if self.status_port == 0 {
            return Err("The world's status_address in empyrean.toml needs a port.".into());
        }
        if !self.dats.is_absolute() {
            return Err("Choose the full path to your game data folder.".into());
        }
        if !ERAS.contains(&self.era.as_str()) {
            return Err("Choose an era the world can play.".into());
        }
        if self.welcome.chars().count() > 500 || !plain_text(&self.welcome, true) {
            return Err("Keep the welcome message to 500 characters of text.".into());
        }
        if !(1..=10_000).contains(&self.max_players) {
            return Err("Allow from 1 to 10,000 players at once.".into());
        }
        if self.max_per_address != -1 && !(1..=10_000).contains(&self.max_per_address) {
            return Err("Allow from 1 to 10,000 players from one address, or no limit.".into());
        }
        if !(0..=99_999).contains(&self.shutdown_warning) {
            return Err("Give a shutdown warning of 0 to 99,999 seconds.".into());
        }
        if !LOG_LEVELS.contains(&self.log_level.as_str()) {
            return Err("Choose a log level.".into());
        }
        let names = system_names();
        if let Some(unknown) = self.systems.keys().find(|k| !names.contains(&k.as_str())) {
            return Err(format!("{unknown} is not one of the era's systems."));
        }
        self.websocket.validate()?;
        self.federation.validate()?;
        if !(0..=i64::from(u32::MAX)).contains(&self.patching.records_per_minute)
            || !(1..=i64::from(u32::MAX)).contains(&self.patching.bytes_per_second)
        {
            return Err("Give the patching rates as whole numbers.".into());
        }
        Ok(())
    }
    pub fn from_config(text: &str) -> Result<Self, String> {
        let doc = text.parse::<DocumentMut>().map_err(|e| e.to_string())?;
        let none = toml_edit::Item::None;
        let server = doc
            .get("server")
            .ok_or("The configuration has no server table.")?;
        let network = server.get("network").unwrap_or(&none);
        let get = |key: &str| server.get(key).unwrap_or(&none);
        let net = |key: &str| network.get(key).unwrap_or(&none);
        let status = get("status_address")
            .as_str()
            .unwrap_or("127.0.0.1:9100")
            .parse::<SocketAddr>()
            .map_err(|_| "The status address is invalid.")?;
        if status.ip() != IpAddr::V4(std::net::Ipv4Addr::LOCALHOST) {
            return Err("The world's status_address must be on 127.0.0.1.".into());
        }
        let port = match network.get("port") {
            None => 9000,
            Some(item) => item
                .as_integer()
                .and_then(|p| u16::try_from(p).ok())
                .ok_or("The game port is invalid.")?,
        };
        let defaults = Self::default();
        let era = doc
            .get("era")
            .and_then(|e| e.get("profile"))
            .and_then(toml_edit::Item::as_str)
            .unwrap_or("eor");
        let systems = system_names()
            .into_iter()
            .filter_map(|name| {
                let on = doc.get("era")?.get(name)?.as_bool()?;
                Some((name.to_owned(), on))
            })
            .collect();
        let table = |path: &[&str]| {
            path.iter()
                .try_fold(doc.as_item(), |item, key| item.get(key))
                .unwrap_or(&none)
        };
        let text = |item: &toml_edit::Item, key: &str, default: &str| {
            item.get(key)
                .and_then(toml_edit::Item::as_str)
                .unwrap_or(default)
                .to_owned()
        };
        let flag = |item: &toml_edit::Item, key: &str, default: bool| {
            item.get(key)
                .and_then(toml_edit::Item::as_bool)
                .unwrap_or(default)
        };
        let number = |item: &toml_edit::Item, key: &str, default: i64| {
            item.get(key)
                .and_then(toml_edit::Item::as_integer)
                .unwrap_or(default)
        };
        let ws = table(&["server", "websocket"]);
        let ws_defaults = WebSocket::default();
        let websocket = WebSocket {
            enabled: flag(ws, "enabled", ws_defaults.enabled),
            listen: text(ws, "listen", &ws_defaults.listen),
            tls_certificate: text(ws, "tls_certificate", ""),
            tls_private_key: text(ws, "tls_private_key", ""),
            behind_tls_proxy: flag(ws, "behind_tls_proxy", false),
            allowed_origins: ws
                .get("allowed_origins")
                .and_then(toml_edit::Item::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
            public_url: text(ws, "public_url", ""),
        };
        let fed = table(&["federation"]);
        let federation = Federation {
            enabled: flag(fed, "enabled", false),
            slug: text(fed, "slug", ""),
        };
        let overlay = table(&["dat_overlay"]);
        let patch_defaults = Patching::default();
        let patching = Patching {
            enabled: flag(overlay, "patching", patch_defaults.enabled),
            records_per_minute: number(
                overlay,
                "records_per_minute",
                patch_defaults.records_per_minute,
            ),
            bytes_per_second: number(overlay, "bytes_per_second", patch_defaults.bytes_per_second),
        };
        Ok(Self {
            name: get("world_name").as_str().unwrap_or("My world").into(),
            dats: PathBuf::from(get("dat_files_directory").as_str().unwrap_or("")),
            bind: net("host").as_str().unwrap_or("127.0.0.1").into(),
            port,
            status_port: status.port(),
            accounts: server
                .get("accounts")
                .and_then(|a| a.get("allow_auto_account_creation"))
                .unwrap_or(&none)
                .as_bool()
                .unwrap_or(true),
            era: era.into(),
            welcome: get("character_screen_message")
                .as_str()
                .unwrap_or("")
                .into(),
            max_players: net("maximum_allowed_sessions")
                .as_integer()
                .unwrap_or(defaults.max_players),
            max_per_address: net("maximum_allowed_sessions_per_ip_address")
                .as_integer()
                .unwrap_or(defaults.max_per_address),
            shutdown_warning: get("shutdown_interval")
                .as_integer()
                .unwrap_or(defaults.shutdown_warning),
            log_level: get("log_level").as_str().unwrap_or("info").into(),
            systems,
            websocket,
            federation,
            patching,
        })
    }
    pub fn update_config(&self, original: &str) -> Result<String, String> {
        self.validate()?;
        let mut doc = original.parse::<DocumentMut>().map_err(|e| e.to_string())?;
        for table in ["server", "database", "era"] {
            if doc.get(table).is_none() {
                doc[table] = toml_edit::Item::Table(toml_edit::Table::new());
            }
            if !doc[table].is_table() {
                return Err(format!("{table} must be a table."));
            }
        }
        for table in ["network", "accounts"] {
            if doc["server"].get(table).is_none() {
                doc["server"][table] = toml_edit::Item::Table(toml_edit::Table::new());
            }
            if !doc["server"][table].is_table() {
                return Err(format!("server.{table} must be a table."));
            }
        }
        set(&mut doc["server"]["world_name"], &self.name);
        set(
            &mut doc["server"]["dat_files_directory"],
            self.dats.to_string_lossy().as_ref(),
        );
        set(
            &mut doc["server"]["status_address"],
            format!("127.0.0.1:{}", self.status_port),
        );
        set(&mut doc["server"]["interactive_console"], true);
        set(&mut doc["server"]["update"], "off");
        set(&mut doc["server"]["network"]["host"], &self.bind);
        set(&mut doc["server"]["network"]["port"], i64::from(self.port));
        set(
            &mut doc["server"]["accounts"]["allow_auto_account_creation"],
            self.accounts,
        );
        set(&mut doc["era"]["profile"], &self.era);
        set(
            &mut doc["server"]["character_screen_message"],
            escaped(&self.welcome)?,
        );
        // The era's systems: those the world names over the era's own, and no others.
        for name in system_names() {
            match self.systems.get(name) {
                Some(on) => set(&mut doc["era"][name], *on),
                None => {
                    if let Some(era) = doc["era"].as_table_like_mut() {
                        era.remove(name);
                    }
                }
            }
        }
        let ws = &self.websocket;
        let ws_defaults = WebSocket::default();
        let at = ["server", "websocket"];
        put(&mut doc, &at, "enabled", ws.enabled, ws_defaults.enabled)?;
        put(
            &mut doc,
            &at,
            "listen",
            ws.listen.as_str(),
            ws_defaults.listen.as_str(),
        )?;
        put(
            &mut doc,
            &at,
            "tls_certificate",
            ws.tls_certificate.as_str(),
            "",
        )?;
        put(
            &mut doc,
            &at,
            "tls_private_key",
            ws.tls_private_key.as_str(),
            "",
        )?;
        put(
            &mut doc,
            &at,
            "behind_tls_proxy",
            ws.behind_tls_proxy,
            false,
        )?;
        put(
            &mut doc,
            &at,
            "allowed_origins",
            ws.allowed_origins
                .iter()
                .map(String::as_str)
                .collect::<toml_edit::Array>(),
            toml_edit::Array::new(),
        )?;
        put(&mut doc, &at, "public_url", ws.public_url.as_str(), "")?;
        let fed = &self.federation;
        put(&mut doc, &["federation"], "enabled", fed.enabled, false)?;
        put(&mut doc, &["federation"], "slug", fed.slug.as_str(), "")?;
        let patching = &self.patching;
        let defaults = Patching::default();
        let at = ["dat_overlay"];
        put(
            &mut doc,
            &at,
            "patching",
            patching.enabled,
            defaults.enabled,
        )?;
        put(
            &mut doc,
            &at,
            "records_per_minute",
            patching.records_per_minute,
            defaults.records_per_minute,
        )?;
        put(
            &mut doc,
            &at,
            "bytes_per_second",
            patching.bytes_per_second,
            defaults.bytes_per_second,
        )?;
        set(
            &mut doc["server"]["network"]["maximum_allowed_sessions"],
            self.max_players,
        );
        set(
            &mut doc["server"]["network"]["maximum_allowed_sessions_per_ip_address"],
            self.max_per_address,
        );
        set(
            &mut doc["server"]["shutdown_interval"],
            self.shutdown_warning,
        );
        set(&mut doc["server"]["log_level"], &self.log_level);
        // The era's own world database, unless the world names one of its own.
        let pack = doc["server"]
            .get("world_pack_path")
            .and_then(toml_edit::Item::as_str)
            .map(str::to_owned);
        let ours = |p: &str| {
            ERAS.iter()
                .any(|e| p == era_pack(e) || p.strip_prefix("./") == Some(era_pack(e)))
        };
        if pack.as_deref().is_none_or(ours) {
            set(&mut doc["server"]["world_pack_path"], era_pack(&self.era));
        }
        for (table, key, default) in [
            ("server", "world_pack_path", "world.pack"),
            ("database", "auth_db_path", "auth.db"),
            ("database", "shard_db_path", "shard.db"),
        ] {
            if doc[table].get(key).is_none() {
                doc[table][key] = value(default);
            }
        }
        Ok(doc.to_string())
    }
}

/// Whether two values are the same value, however each is written.
fn same(a: &toml_edit::Value, b: &toml_edit::Value) -> bool {
    use toml_edit::Value as V;
    match (a, b) {
        (V::String(x), V::String(y)) => x.value() == y.value(),
        (V::Integer(x), V::Integer(y)) => x.value() == y.value(),
        (V::Boolean(x), V::Boolean(y)) => x.value() == y.value(),
        (V::Array(x), V::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| same(a, b))
        }
        _ => false,
    }
}

/// `text` as a TOML basic string, its line ends written `\n`, as `empyrean.toml`'s own comments
/// ask for a message of several lines.
fn escaped(text: &str) -> Result<toml_edit::Value, String> {
    // A JSON string's escapes are all TOML basic-string escapes.
    serde_json::to_string(text)
        .map_err(|e| e.to_string())?
        .parse::<toml_edit::Value>()
        .map_err(|e| e.to_string())
}

/// Sets `key` in the table at `path` to `next`, as [`set`] does. A key the file does not have is
/// left out while `next` is the server's `default`, and so is a table it would be the first in.
fn put(
    doc: &mut DocumentMut,
    path: &[&str],
    key: &str,
    next: impl Into<toml_edit::Value>,
    default: impl Into<toml_edit::Value>,
) -> Result<(), String> {
    let (next, default) = (next.into(), default.into());
    let quiet = same(&next, &default);
    let mut item = doc.as_item_mut();
    for (i, part) in path.iter().enumerate() {
        if item.get(part).is_none() {
            if quiet {
                return Ok(());
            }
            item[part] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        item = &mut item[part];
        if !item.is_table_like() {
            return Err(format!("{} must be a table.", path[..=i].join(".")));
        }
    }
    if item.get(key).is_none() && quiet {
        return Ok(());
    }
    set(&mut item[key], next);
    Ok(())
}

/// Sets `item` to `next`, keeping its comment; a value that is already `next` is left as written.
fn set(item: &mut toml_edit::Item, next: impl Into<toml_edit::Value>) {
    let next = next.into();
    if item.as_value().is_some_and(|v| same(v, &next)) {
        return;
    }
    let decor = item.as_value().map(|v| v.decor().clone());
    *item = value(next);
    if let Some(decor) = decor {
        *item.as_value_mut().expect("assigned value").decor_mut() = decor;
    }
}

pub fn validate_managed(text: &str, dir: &std::path::Path) -> Result<(), String> {
    let doc = text.parse::<DocumentMut>().map_err(|e| e.to_string())?;
    let server = doc.get("server").ok_or("The server table is missing.")?;
    if server
        .get("interactive_console")
        .and_then(toml_edit::Item::as_bool)
        == Some(false)
    {
        return Err(
            "The launcher needs interactive_console = true to stop the world cleanly.".into(),
        );
    }
    if server
        .get("update")
        .and_then(toml_edit::Item::as_str)
        .unwrap_or("off")
        != "off"
    {
        return Err("The launcher updates this server itself: server.update must be 'off'.".into());
    }
    for key in ["auth_db_path", "shard_db_path"] {
        let path = doc
            .get("database")
            .and_then(|d| d.get(key))
            .and_then(toml_edit::Item::as_str)
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                dir.join(if key == "auth_db_path" {
                    "auth.db"
                } else {
                    "shard.db"
                })
            });
        let path = if path.is_absolute() {
            path
        } else {
            dir.join(path)
        };
        if !path.starts_with(dir)
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(format!("{key} must stay inside this world's folder."));
        }
    }
    Ok(())
}

pub fn redact(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if ["password", "authorization", "secret", "token", "credential"]
        .iter()
        .any(|s| lower.contains(s))
    {
        "[credential details hidden]".into()
    } else {
        line.chars().take(8192).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn settings() -> Settings {
        Settings {
            dats: std::env::temp_dir(),
            ..Settings::default()
        }
    }
    #[test]
    fn common_settings_keep_custom_tables_comments_and_database_locations() {
        let input = "# operator notes\n[server]\nworld_name = 'Old' # world label\ncustom = 7 # keep this\n[database]\nauth_db_path = 'accounts/custom.db'\n[era]\nprofile = 'infiltration'\n";
        let text = Settings {
            era: "infiltration".into(),
            ..settings()
        }
        .update_config(input)
        .unwrap();
        assert!(text.contains("# operator notes"));
        assert!(text.contains("# world label"));
        assert!(text.contains("custom = 7 # keep this"));
        assert!(text.contains("accounts/custom.db"));
        assert!(text.contains("profile = 'infiltration'"));
        let read = Settings::from_config(&text).unwrap();
        assert_eq!(read.name, "My world");
        assert_eq!(read.port, 9000);
        assert!(text.contains("update = \"off\""));
    }
    #[test]
    fn managed_hosts_require_valid_names_ports_addresses_and_absolute_data_paths() {
        let mut s = settings();
        s.validate().unwrap();
        s.port = 65535;
        assert!(s.validate().is_err());
        s.port = 9000;
        s.bind = "example.com".into();
        assert!(s.validate().is_err());
        s.bind = "127.0.0.1".into();
        s.dats = "relative".into();
        assert!(s.validate().is_err());
        s.dats = std::env::temp_dir();
        s.name = "\n".into();
        assert!(s.validate().is_err());
    }
    #[test]
    fn credential_lines_are_hidden_before_display_or_export() {
        for line in [
            "password=abc",
            "Authorization: Bearer abc",
            "hub token abc",
            "secret: abc",
        ] {
            assert_eq!(redact(line), "[credential details hidden]");
        }
        assert_eq!(redact("World ready"), "World ready");
    }
    #[test]
    fn invalid_ports_and_unreachable_status_addresses_are_not_silently_replaced() {
        for port in ["-1", "65536", "'9000'"] {
            assert!(Settings::from_config(&format!("[server.network]\nport={port}\n")).is_err());
        }
        assert!(Settings::from_config("[server]\nstatus_address='[::1]:9100'\n").is_err());
    }
    #[test]
    fn managed_hosts_refuse_an_independent_updater_or_external_character_databases() {
        let dir = std::env::temp_dir().join("managed-host");
        let good = settings().update_config("").unwrap();
        validate_managed(&good, &dir).unwrap();
        assert!(validate_managed(
            &good.replace("update = \"off\"", "update = \"patch\""),
            &dir
        )
        .is_err());
        assert!(validate_managed(&good.replace("auth.db", "../someone-else.db"), &dir).is_err());
        assert!(validate_managed(
            &good.replace("interactive_console = true", "interactive_console = false"),
            &dir
        )
        .is_err());
        assert!(Settings::from_config("").is_err());
        Settings::from_config("[server]\nworld_name='A'\n").unwrap();
    }

    /// The Settings screen's keys are read and written where `empyrean.toml` keeps them, every
    /// other key and comment kept, and a value that does not change stays as it was written.
    #[test]
    fn every_setting_is_read_and_written_in_its_place() {
        let input = "# notes\n[server]\nworld_name = 'Old' # its name\nlog_level = 'info'\ncustom = 7\n[server.network]\nmaximum_allowed_sessions = 64\n[era]\nprofile = 'eor'\ncloaks = false\n";
        let mut s = Settings::from_config(input).unwrap();
        assert_eq!(s.max_players, 64);
        assert_eq!(s.era, "eor");
        s.dats = std::env::temp_dir();
        s.name = "New".into();
        s.welcome = "Be kind".into();
        s.max_per_address = 2;
        s.shutdown_warning = 300;
        s.log_level = "debug".into();
        let text = s.update_config(input).unwrap();
        assert!(
            text.contains("# notes") && text.contains("custom = 7"),
            "{text}"
        );
        assert!(text.contains("world_name = \"New\" # its name"), "{text}");
        assert!(
            text.contains("profile = 'eor'"),
            "unchanged, as written: {text}"
        );
        assert!(text.contains("cloaks = false"), "{text}");
        let back = Settings::from_config(&text).unwrap();
        assert_eq!(back, s);
    }

    /// Each era plays its own world database, unless the world names one of its own.
    #[test]
    fn each_era_plays_its_own_world_database() {
        let mut s = settings();
        let text = s.update_config("").unwrap();
        assert!(text.contains("world_pack_path = \"world.pack\""), "{text}");
        s.era = "infiltration".into();
        let text = s.update_config(&text).unwrap();
        assert!(
            text.contains("world_pack_path = \"world.pack.infiltration\""),
            "{text}"
        );
        assert!(text.contains("profile = \"infiltration\""), "{text}");
        let own = "[server]\nworld_pack_path = 'published/snapshot-1/world.pack'\n";
        let text = s.update_config(own).unwrap();
        assert!(text.contains("published/snapshot-1/world.pack"), "{text}");
        assert_eq!(era_pack("eor"), "world.pack");
        s.era = "gold".into();
        assert!(s.validate().is_err(), "no such era");
    }

    /// The Settings screen's numbers and words are held to what the server takes.
    #[test]
    fn the_settings_are_held_to_what_the_server_takes() {
        let ok = settings();
        ok.validate().unwrap();
        for bad in [
            Settings {
                max_players: 0,
                ..settings()
            },
            Settings {
                max_per_address: 0,
                ..settings()
            },
            Settings {
                shutdown_warning: -1,
                ..settings()
            },
            Settings {
                log_level: "loud".into(),
                ..settings()
            },
            Settings {
                welcome: "a\tb".into(),
                ..settings()
            },
            Settings {
                welcome: "a".repeat(501),
                ..settings()
            },
            Settings {
                systems: BTreeMap::from([("flying".to_owned(), true)]),
                ..settings()
            },
            Settings {
                patching: Patching {
                    bytes_per_second: 0,
                    ..Patching::default()
                },
                ..settings()
            },
        ] {
            assert!(bad.validate().is_err(), "{bad:?}");
        }
        Settings {
            max_per_address: -1,
            ..settings()
        }
        .validate()
        .unwrap();
    }

    /// The welcome message keeps its lines, written `\n` in the file as its comment asks.
    #[test]
    fn a_welcome_of_several_lines_is_written_with_newline_escapes() {
        let s = Settings {
            welcome: "Welcome, \"traveller\".\nBe kind.".into(),
            ..settings()
        };
        let text = s.update_config("").unwrap();
        assert!(
            text.contains(r#"character_screen_message = "Welcome, \"traveller\".\nBe kind.""#),
            "{text}"
        );
        assert_eq!(Settings::from_config(&text).unwrap().welcome, s.welcome);
        // A message written by hand in a multi-line string, unchanged, stays as it was written.
        let by_hand = "[server]\ncharacter_screen_message = \"\"\"\nHi\nthere\"\"\"\n";
        let mut read = Settings::from_config(by_hand).unwrap();
        assert_eq!(read.welcome, "Hi\nthere");
        read.dats = std::env::temp_dir();
        assert!(read
            .update_config(by_hand)
            .unwrap()
            .contains("\"\"\"\nHi\nthere\"\"\""));
    }

    /// The era's systems a world turns on or off are its `[era]` keys over the profile: one
    /// changed is written, one given back to the profile loses its key, and keys the launcher
    /// does not know are kept.
    #[test]
    fn era_systems_are_the_keys_over_the_profile_and_others_are_kept() {
        let input =
            "[server]\n[era]\nprofile = 'eor'\ncloaks = false # no cloaks here\nmystery = 1\n";
        let mut s = Settings::from_config(input).unwrap();
        assert_eq!(s.systems, BTreeMap::from([("cloaks".to_owned(), false)]));
        s.dats = std::env::temp_dir();
        s.systems = BTreeMap::from([("aetheria".to_owned(), false), ("trade".to_owned(), true)]);
        let text = s.update_config(input).unwrap();
        assert!(!text.contains("cloaks"), "{text}");
        assert!(
            text.contains("aetheria = false") && text.contains("trade = true"),
            "{text}"
        );
        assert!(text.contains("mystery = 1"), "{text}");
        assert_eq!(Settings::from_config(&text).unwrap(), s);
    }

    /// The WebSocket endpoint, the Dereth Network and patching are read and written in their
    /// tables; a new world's file names none of them while they keep the server's defaults. The
    /// status endpoint's public address and the network key's file are not settings: a value the
    /// file gives them is kept as written.
    #[test]
    fn websocket_network_and_patching_settings_are_written_only_when_set() {
        let plain = settings().update_config("").unwrap();
        for absent in [
            "websocket",
            "federation",
            "dat_overlay",
            "status_public_url",
        ] {
            assert!(!plain.contains(absent), "{absent}: {plain}");
        }
        let input = "[server]\nstatus_public_url = 'https://status.example.org/status'\n[federation]\n# from the operators page\nslug = 'old'\nkey_file = 'keys/world.key'\n";
        let s = Settings {
            websocket: WebSocket {
                enabled: true,
                tls_certificate: "certs/play.pem".into(),
                tls_private_key: "certs/play.key".into(),
                allowed_origins: vec!["https://play.example.org".into()],
                public_url: "wss://play.example.org/ws".into(),
                ..WebSocket::default()
            },
            federation: Federation {
                enabled: true,
                slug: "my-world".into(),
            },
            patching: Patching {
                enabled: false,
                ..Patching::default()
            },
            ..settings()
        };
        let text = s.update_config(input).unwrap();
        assert!(text.contains("[server.websocket]"), "{text}");
        assert!(
            text.contains("allowed_origins = [\"https://play.example.org\"]"),
            "{text}"
        );
        assert!(
            text.contains("# from the operators page\nslug = \"my-world\""),
            "{text}"
        );
        assert!(text.contains("[dat_overlay]\npatching = false"), "{text}");
        assert!(!text.contains("listen"), "defaults: {text}");
        assert!(
            text.contains("status_public_url = 'https://status.example.org/status'")
                && text.contains("key_file = 'keys/world.key'"),
            "kept as written: {text}"
        );
        assert_eq!(Settings::from_config(&text).unwrap(), s);
    }

    #[test]
    fn browser_players_need_a_secure_connection_and_allowed_pages() {
        let ws = |f: fn(&mut WebSocket)| {
            let mut w = WebSocket {
                enabled: true,
                behind_tls_proxy: true,
                ..WebSocket::default()
            };
            f(&mut w);
            w.validate()
        };
        ws(|_| {}).unwrap();
        ws(|w| w.listen = "127.0.0.1:9180".into()).unwrap();
        ws(|w| {
            w.behind_tls_proxy = false;
            w.listen = "127.0.0.1:9180".into();
        })
        .unwrap();
        assert!(
            ws(|w| w.behind_tls_proxy = false).is_err(),
            "plain on every address"
        );
        ws(|w| {
            w.behind_tls_proxy = false;
            w.tls_certificate = "a.pem".into();
            w.tls_private_key = "a.key".into();
        })
        .unwrap();
        assert!(ws(|w| w.tls_certificate = "a.pem".into()).is_err(), "half");
        assert!(ws(|w| w.listen = "9443".into()).is_err());
        assert!(ws(|w| w.allowed_origins = vec!["play.example.org".into()]).is_err());
        assert!(ws(|w| w.allowed_origins = vec!["https://a.example/page".into()]).is_err());
        assert!(ws(|w| w.public_url = "https://play.example.org".into()).is_err());
    }

    #[test]
    fn a_network_name_is_two_to_twelve_lowercase_letters_and_digits() {
        for good in ["ab", "my-world", "coldeve2", "a-b-c"] {
            assert!(is_slug(good), "{good}");
        }
        for bad in [
            "a",
            "My-World",
            "-ab",
            "ab-",
            "a--b",
            "thirteen-char",
            "a b",
        ] {
            assert!(!is_slug(bad), "{bad}");
        }
        let on = Federation {
            enabled: true,
            ..Federation::default()
        };
        assert!(on.validate().is_err(), "on, with no name");
        Federation::default().validate().unwrap();
    }

    /// A world plays the data files its era is drawn from, by the client's rule: Infiltration the
    /// Classic pair, the end of retail the Modern files.
    #[test]
    fn an_era_before_throne_of_destiny_plays_classic_data_files() {
        let temp = tempfile::tempdir().unwrap();
        let modern = temp.path().join("modern");
        let classic = temp.path().join("classic");
        dereth_launch::testing::write_dat_set(&modern, dereth_launch::Iterations::END_OF_RETAIL);
        dereth_launch::testing::write_classic_set(&classic, 2112, 1593);
        check_game_data("eor", &modern).unwrap();
        check_game_data("infiltration", &classic).unwrap();
        let wrong = check_game_data("infiltration", &modern).unwrap_err();
        assert!(wrong.contains("Classic data files"), "{wrong}");
        assert!(check_game_data("eor", &classic).is_err());
    }

    /// A world that plays a Lyceum world keeps the era and game data it was published for; any
    /// other world may change them, to data files its era is drawn from.
    #[test]
    fn a_published_world_keeps_its_era_and_game_data() {
        let temp = tempfile::tempdir().unwrap();
        let modern = temp.path().join("modern");
        let classic = temp.path().join("classic");
        dereth_launch::testing::write_dat_set(&modern, dereth_launch::Iterations::END_OF_RETAIL);
        dereth_launch::testing::write_classic_set(&classic, 2112, 1593);
        let world = Settings {
            dats: modern.clone(),
            ..Settings::default()
        };
        let text = world.update_config("").unwrap();
        let infiltration = |dats: &Path| Settings {
            era: "infiltration".into(),
            dats: dats.to_path_buf(),
            ..world.clone()
        };
        let dir = temp.path();
        check_change(&text, dir, &infiltration(&classic)).unwrap();
        assert!(check_change(&text, dir, &infiltration(&modern)).is_err());
        let published = format!("{text}\n[dat_overlay]\npath = 'published/snapshot-0/overlay'\n");
        assert!(check_change(&published, dir, &infiltration(&classic)).is_err());
        assert!(check_change(
            &published,
            dir,
            &Settings {
                dats: classic.clone(),
                ..world.clone()
            }
        )
        .is_err());
        check_change(
            &published,
            dir,
            &Settings {
                name: "Renamed".into(),
                ..world.clone()
            },
        )
        .unwrap();
    }

    /// What a world plays: its era's world database, its own, or a published Lyceum world's, with
    /// when that was published; and whether its network key file is there.
    #[test]
    fn a_world_plays_its_eras_database_its_own_or_a_lyceum_worlds() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        let era = plays(
            "[server]\nworld_pack_path = './world.pack.infiltration'\n",
            dir,
        );
        assert_eq!((era.database, era.network_key), ("era", false));
        let own = plays(
            "[server]\nworld_pack_path = 'D:/packs/mine.pack'\nworld_overlay_path = 'content.db'\n",
            dir,
        );
        assert_eq!(own.database, "own");
        assert_eq!(own.content_overlay.as_deref(), Some("content.db"));
        let snapshot = dir.join("published/snapshot-0");
        std::fs::create_dir_all(snapshot.join("overlay")).unwrap();
        std::fs::write(snapshot.join("publication.json"), b"{}").unwrap();
        std::fs::write(dir.join("network.key"), format!("{KEY}\n")).unwrap();
        let lyceum = plays(
            "[server]\nworld_pack_path = 'published/snapshot-0/world.pack'\n[dat_overlay]\npath = 'published/snapshot-0/overlay'\n",
            dir,
        );
        assert_eq!(lyceum.database, "lyceum");
        assert!(lyceum.published.is_some() && lyceum.network_key);
        assert_eq!(
            lyceum.dat_overlay.as_deref(),
            Some("published/snapshot-0/overlay")
        );
    }

    /// A world database's base is the ACE release its dump names: read from what the import tool
    /// says when it fetches one, kept beside the pack, and shown for a world that plays the era's
    /// own database or a Lyceum world built over one; a name of any other shape is no release.
    #[test]
    fn a_world_databases_base_is_the_ace_release_its_dump_names() {
        assert_eq!(
            world_base("ACE-World-Database-v0.9.295.sql"),
            Some(WorldBase {
                era: "eor",
                version: "v0.9.295".into()
            })
        );
        assert_eq!(
            world_base("ACE-World-16PY-db-v0.8.8.sql.zip").map(|b| (b.era, b.version)),
            Some(("infiltration", "v0.8.8".into()))
        );
        for other in [
            "mine.sql",
            "ACE-World-Database-v0.9.sql",
            "ACE-World-Database-0.9.295.sql",
            "ACE-World-Database-v0.9.295x.sql",
            "ACE-World-Database-v0.9.295",
        ] {
            assert_eq!(world_base(other), None, "{other}");
        }
        // What `fetch` prints, cached or downloaded.
        let cached = "v0.9.295 is already cached: C:\\Users\\you\\AppData\\Local\\Empyrean\\world-database\\ACE-World-Database-v0.9.295.sql\nwrote world.pack";
        assert_eq!(
            fetched_dump(cached).as_deref(),
            Some("ACE-World-Database-v0.9.295.sql")
        );
        let downloaded = "downloading https://github.com/ACEmulator/ACE-World-16PY/releases/download/v0.8.8/ACE-World-16PY-db-v0.8.8.sql.zip\nzip SHA-256 0a matches the pin\ncached /home/you/.cache/empyrean/world-database/ACE-World-16PY-db-v0.8.8.sql (1 bytes) in 1.0s\n";
        assert_eq!(
            fetched_dump(downloaded).as_deref(),
            Some("ACE-World-16PY-db-v0.8.8.sql")
        );
        assert_eq!(fetched_dump("nothing fetched"), None);

        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        std::fs::write(dir.join("world.pack"), b"pack").unwrap();
        std::fs::write(
            base_record(&dir.join("world.pack")),
            "ACE-World-Database-v0.9.295.sql\n",
        )
        .unwrap();
        // A record left beside a pack that is gone says nothing.
        std::fs::write(
            base_record(&dir.join("world.pack.infiltration")),
            "ACE-World-16PY-db-v0.8.8.sql\n",
        )
        .unwrap();
        let era = plays("[server]\nworld_pack_path = 'world.pack'\n", dir);
        assert_eq!(
            era.base.as_ref().map(|b| b.version.as_str()),
            Some("v0.9.295")
        );
        assert_eq!(era.era_bases.len(), 1);
        assert_eq!(era.era_bases[0].era, "eor");
        let snapshot = dir.join("published/snapshot-0");
        std::fs::create_dir_all(snapshot.join("overlay")).unwrap();
        std::fs::write(
            snapshot.join("publication.json"),
            br#"{"base":"ACE-World-16PY-db-v0.8.8.sql"}"#,
        )
        .unwrap();
        let lyceum = plays(
            "[server]\nworld_pack_path = 'published/snapshot-0/world.pack'\n[dat_overlay]\npath = 'published/snapshot-0/overlay'\n",
            dir,
        );
        assert_eq!(
            lyceum.base.map(|b| (b.era, b.version)),
            Some(("infiltration", "v0.8.8".into()))
        );
        let unknown = plays("[server]\nworld_pack_path = 'D:/packs/mine.pack'\n", dir);
        assert_eq!(unknown.base, None);
    }

    /// A key of the shape Empyrean reads, made up for these tests.
    const KEY: &str = "drth_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_xyzAB";

    /// A pasted key is taken without the space around it when it has the shape Empyrean reads
    /// from its key file, and refused in plain words otherwise, the paste never repeated.
    #[test]
    fn a_pasted_network_key_is_held_to_the_shape_empyrean_reads() {
        assert_eq!(KEY.len(), 48);
        assert_eq!(network_key(&format!("  {KEY}\r\n")), Ok(KEY));
        let short = &KEY[..47];
        let spaced = format!("{} {}", &KEY[..20], &KEY[21..]);
        let other = format!("drth-{}", &KEY[5..]);
        let symbol = format!("{}!", &KEY[..47]);
        for bad in ["", "   ", short, &spaced, &other, &symbol] {
            let said = network_key(bad).unwrap_err();
            assert!(
                !said.contains("AbCdEf"),
                "the paste is never repeated: {said}"
            );
        }
    }

    /// The key is kept in `network.key` in the world's folder, which `[federation] key_file`
    /// then names, the file's comments kept; only a file holding a key counts as one kept, and
    /// on Unix only its owner may read it.
    #[test]
    fn a_saved_network_key_is_kept_in_the_worlds_folder_and_named_by_its_configuration() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        let input = "# notes\n[server]\nworld_name = 'Mine'\n[federation]\nslug = 'mine' # its slug\nkey_file = 'old.key' # was here\n";
        assert!(!plays(input, dir).network_key);
        let text = save_network_key(dir, input, &format!("{KEY}\n")).unwrap();
        assert!(
            text.contains("# notes") && text.contains("# its slug"),
            "{text}"
        );
        assert!(
            text.contains("key_file = \"network.key\" # was here"),
            "{text}"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(NETWORK_KEY_FILE)).unwrap(),
            format!("{KEY}\n")
        );
        assert!(plays(&text, dir).network_key);
        assert!(!dir.join("network.key.new").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join(NETWORK_KEY_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        // A configuration with no [federation] table gains one naming the file.
        let bare = save_network_key(dir, "[server]\n", KEY).unwrap();
        assert!(
            bare.contains("[federation]\nkey_file = \"network.key\""),
            "{bare}"
        );
        // A malformed paste changes nothing.
        assert!(save_network_key(dir, input, "drth_short").is_err());
        assert_eq!(
            std::fs::read_to_string(dir.join(NETWORK_KEY_FILE)).unwrap(),
            format!("{KEY}\n")
        );
        // A key file holding something else is no key.
        std::fs::write(dir.join(NETWORK_KEY_FILE), b"not a key").unwrap();
        assert!(!plays(&text, dir).network_key);
    }

    /// A world links to the Dereth Network only once its key is kept.
    #[test]
    fn linking_to_the_network_waits_for_the_worlds_key() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        let modern = dir.join("modern");
        dereth_launch::testing::write_dat_set(&modern, dereth_launch::Iterations::END_OF_RETAIL);
        let world = Settings {
            dats: modern,
            ..Settings::default()
        };
        let text = world.update_config("").unwrap();
        let linked = Settings {
            federation: Federation {
                enabled: true,
                slug: "my-world".into(),
            },
            ..world.clone()
        };
        let said = check_change(&text, dir, &linked).unwrap_err();
        assert!(said.contains("key"), "{said}");
        let text = save_network_key(dir, &text, KEY).unwrap();
        check_change(&text, dir, &linked).unwrap();
    }

    /// A new world's status endpoint takes the first port from 9100 up that no other world names
    /// and nothing holds; the page neither sees that port nor sets it, and saving keeps it.
    #[test]
    fn the_status_port_is_chosen_once_by_the_launcher_and_kept() {
        assert_eq!(free_status_port(&[], |_| true), Some(9100));
        assert_eq!(free_status_port(&[9100, 9101], |p| p != 9102), Some(9103));
        assert_eq!(free_status_port(&[], |_| false), None);
        let page = serde_json::to_value(settings()).unwrap();
        assert!(page.get("status_port").is_none(), "{page}");
        let mut from_page: Settings =
            serde_json::from_value(serde_json::json!({ "status_port": 1234 })).unwrap();
        assert_eq!(from_page.status_port, Settings::default().status_port);
        let temp = tempfile::tempdir().unwrap();
        let modern = temp.path().join("modern");
        dereth_launch::testing::write_dat_set(&modern, dereth_launch::Iterations::END_OF_RETAIL);
        let text = Settings {
            status_port: 9107,
            dats: modern.clone(),
            ..Settings::default()
        }
        .update_config("")
        .unwrap();
        assert!(
            text.contains("status_address = \"127.0.0.1:9107\""),
            "{text}"
        );
        from_page.dats = modern;
        let saved = save_over(&text, temp.path(), &from_page).unwrap();
        assert!(
            saved.contains("status_address = \"127.0.0.1:9107\""),
            "{saved}"
        );
    }

    /// Settings turn sending the world's data to Dereth players off for any world but one that
    /// plays a published Lyceum world: that is how its players get it.
    #[test]
    fn a_published_lyceum_world_keeps_sending_its_data_to_dereth_players() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        let modern = dir.join("modern");
        dereth_launch::testing::write_dat_set(&modern, dereth_launch::Iterations::END_OF_RETAIL);
        let plain = Settings {
            dats: modern.clone(),
            ..Settings::default()
        }
        .update_config("")
        .unwrap();
        let off = Settings {
            dats: modern,
            patching: Patching {
                enabled: false,
                ..Patching::default()
            },
            ..Settings::default()
        };
        let saved = save_over(&plain, dir, &off).unwrap();
        assert!(!Settings::from_config(&saved).unwrap().patching.enabled);

        let snapshot = dir.join("published/snapshot-0");
        std::fs::create_dir_all(snapshot.join("overlay")).unwrap();
        std::fs::write(snapshot.join("publication.json"), b"{}").unwrap();
        let published = format!(
            "{plain}\n[dat_overlay]\npath = 'published/snapshot-0/overlay'\npatching = true\n"
        );
        let saved = save_over(&published, dir, &off).unwrap();
        assert!(
            Settings::from_config(&saved).unwrap().patching.enabled,
            "{saved}"
        );
    }
}
