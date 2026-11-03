//! Everything the player has told the launcher, in one file: `launcher-state.json`.
//!
//! The retail client (at most one), the library of dat sets, accounts (names only), favourites and
//! the per-world "remember my choices". The Dereth client is not recorded: it ships beside the
//! launcher, so the launcher finds it each time it starts. **Never a password**: those live only in the operating system's vault
//! ([`crate::vault`]), and a test below holds this file to that.
//!
//! # Where it lives
//!
//! In the launcher's settings folder, never beside the executable, which an update replaces and a
//! player may move or delete; the private dat sets go in its data folder. See [`crate::folders`].
//!
//! # Writing
//!
//! Atomically: the new contents go to a temporary file which then replaces the old one, so a crash
//! or a full disk mid-write leaves the previous state intact rather than half a file.
//!
//! # Two files
//!
//! Auberean's realm keeps its own part apart, in `auberean-state.json` beside it
//! ([`AUBEREAN_STATE_FILE`]): its Library, and the worlds the player added for it, their accounts,
//! and every recent combination, favourite and remembered choice that names one of those worlds or
//! the Auberean client. The launcher holds both as one state. A launcher from before Auberean's
//! realm (Dereth 0.4.9) sharing the folder reads `launcher-state.json` whole, since it holds nothing
//! that launcher does not know, and never touches the other file. A state that still holds
//! Auberean's entries in `launcher-state.json` has them moved out the first time it is read
//! ([`LauncherState::load_moving`]), and a world added for Auberean is given a name of its own,
//! `auberean-<n>`, so the older launcher never names a world of its own the same.
//!
//! # Reading
//!
//! A file with a part that does not read (one an older or newer launcher wrote differently) keeps
//! every part that does: only the parts that do not are reset, each named with why
//! ([`Loaded::Partly`]), and the file as it was is copied aside as `launcher-state.json.bad`. A
//! file that is not a state at all is set aside under that name and the launcher starts fresh; a
//! player loses their favourites, not their ability to play.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::artpack::{read_art_pack, ArtPack};
use crate::datset::{DatOrigin, DatSet, SetKind, SHARED_SET_ID};
use crate::install::{ClientKind, Installation};
use crate::world::{Emulator, Endpoint, Game, World};

pub const STATE_FILE: &str = "launcher-state.json";

/// Auberean's realm's part of the state, beside [`STATE_FILE`] (see "Two files" above).
pub const AUBEREAN_STATE_FILE: &str = "auberean-state.json";

/// What a world the player added for Auberean is named, followed by a number.
pub const AUBEREAN_WORLD_PREFIX: &str = "auberean-";

/// Auberean's realm's file: its Library, and its worlds' entries, as the state's own parts are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct AubereanFile {
    #[serde(default, skip_serializing_if = "AubereanSettings::is_empty")]
    library: AubereanSettings,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    accounts: Vec<Account>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    favourites: Vec<Favourite>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    recent: Vec<Recent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    custom_worlds: Vec<CustomWorld>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    world_prefs: BTreeMap<String, WorldPrefs>,
}

impl AubereanFile {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The file's `parts`, each read on its own, as the state's are
    /// ([`LauncherState::from_parts`]); whether any did not read.
    fn from_parts(parts: &serde_json::Map<String, serde_json::Value>) -> (Self, bool) {
        let mut f = Self::default();
        let mut failed = false;
        let mut take = |part: &str, read: &mut dyn FnMut(serde_json::Value) -> bool| {
            if let Some(v) = parts.get(part) {
                failed |= !read(v.clone());
            }
        };
        take("library", &mut |v| set(&mut f.library, v));
        take("accounts", &mut |v| set(&mut f.accounts, v));
        take("favourites", &mut |v| set(&mut f.favourites, v));
        take("recent", &mut |v| set(&mut f.recent, v));
        take("custom_worlds", &mut |v| set(&mut f.custom_worlds, v));
        take("world_prefs", &mut |v| set(&mut f.world_prefs, v));
        (f, failed)
    }
}

/// `into` read from `value`; whether it read.
fn set<T: serde::de::DeserializeOwned>(into: &mut T, value: serde_json::Value) -> bool {
    match serde_json::from_value(value) {
        Ok(v) => {
            *into = v;
            true
        }
        Err(_) => false,
    }
}

/// A world the player added for Auberean that was given a name of its own when it moved, `from`
/// its old one: its saved passwords and its hosted world's link follow it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renamed {
    pub from: String,
    pub to: String,
}

/// Write `bytes` to `path` whole or not at all: to a temporary file beside it, synced, then
/// renamed over it.
///
/// # Errors
/// The file could not be written or renamed.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}
pub const SCHEMA: u32 = 1;

/// An account on one world. The password is in the vault, under [`crate::vault::target`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub world_slug: String,
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Whether the password is kept in the vault between runs.
    #[serde(default)]
    pub remember: bool,
    /// First used on a world that makes accounts on first login: the launcher warned, and the
    /// player went ahead.
    #[serde(default)]
    pub created_by_launcher: bool,
}

/// A saved combination: world, account, client and, for the Dereth client, the data files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Favourite {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub world_slug: String,
    pub account: String,
    pub client: ClientKind,
    /// The Dereth client's data files. A retail client plays with the ones beside it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dat_set_id: Option<String>,
    /// The Dereth client's Classic set, when one was chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classic_set_id: Option<String>,
}

/// One combination played recently: world, account, client and, for the Dereth client, the data
/// files. Home lists the last few, and a favourite is a pinned copy of one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recent {
    pub world_slug: String,
    pub account: String,
    pub client: ClientKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dat_set_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classic_set_id: Option<String>,
    /// Seconds since the epoch.
    pub last_played: u64,
}

impl Recent {
    /// Whether `other` is the same combination, whenever it was played.
    pub fn same_as(
        &self,
        world_slug: &str,
        account: &str,
        client: ClientKind,
        dat_set_id: Option<&str>,
    ) -> bool {
        self.world_slug == world_slug
            && self.account.eq_ignore_ascii_case(account)
            && self.client == client
            && self.dat_set_id.as_deref() == dat_set_id
    }
}

/// A server the player added by hand: a name, a host and a port, and nothing else known about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomWorld {
    /// Its identity among the worlds, `custom-<n>`, so its accounts and choices are its own.
    pub slug: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    /// The rules the player said it plays (`PvE` or `PvP`), if they said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ruleset: Option<String>,
    /// The emulator the player said it runs; unknown when they did not say.
    #[serde(default)]
    pub emulator: Emulator,
    /// The address the player added it by, when that was a URL: its WebSocket endpoint, or its
    /// status document, which names one ([`crate::direct`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The game it runs: Dereth's unless the player added it to Auberean's realm, or a join link
    /// named Auberean.
    #[serde(default, skip_serializing_if = "is_dereth_game")]
    pub game: Game,
}

fn is_dereth_game(game: &Game) -> bool {
    *game == Game::Dereth
}

impl CustomWorld {
    /// The world record the rest of the launcher works with. Its status and software are not
    /// known; its rules are what the player said.
    pub fn to_world(&self) -> World {
        let mut w = World::new(self.slug.clone(), self.name.clone());
        w.ruleset.clone_from(&self.ruleset);
        w.emulator = self.emulator;
        w.game = self.game;
        w.endpoint = Some(Endpoint {
            address: self.host.clone(),
            port: self.port,
            transport: None,
        });
        w.description = Some(format!("Added by you: {}:{}", self.host, self.port));
        if let Some(url) = &self.url {
            if crate::direct::is_websocket_url(url) {
                w.websocket_url = Some(url.trim().to_owned());
            } else if crate::direct::is_status_url(url) {
                w.status_url = Some(url.trim().to_owned());
            }
        }
        w
    }
}

/// What is wrong with a server typed by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomWorldError {
    NoHost,
    BadHost,
    BadPort,
}

impl core::fmt::Display for CustomWorldError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            CustomWorldError::NoHost => "Enter the world's host name or address.",
            CustomWorldError::BadHost => "The host cannot contain spaces, a colon or a slash.",
            CustomWorldError::BadPort => "The port must be a number from 1 to 65535.",
        })
    }
}

impl std::error::Error for CustomWorldError {}

/// A world's host as the player gives it, trimmed: a host name or an IPv4 address, with no space,
/// colon or slash in it.
///
/// # Errors
/// [`CustomWorldError::NoHost`] for none, [`CustomWorldError::BadHost`] for one that is not a host.
pub fn custom_host(host: &str) -> Result<&str, CustomWorldError> {
    let host = host.trim();
    if host.is_empty() {
        return Err(CustomWorldError::NoHost);
    }
    if host.contains(|c: char| c.is_whitespace() || c == ':' || c == '/') {
        return Err(CustomWorldError::BadHost);
    }
    Ok(host)
}

/// A world's game port as the player gives it: a number from 1 to 65535.
///
/// # Errors
/// [`CustomWorldError::BadPort`] for anything else.
pub fn custom_port(port: &str) -> Result<u16, CustomWorldError> {
    port.trim()
        .parse::<u16>()
        .ok()
        .filter(|p| *p != 0)
        .ok_or(CustomWorldError::BadPort)
}

/// How many recent combinations are kept.
pub const RECENT_LIMIT: usize = 5;

/// "Remember my choices for this world". A favourite is a pinned copy of one of these.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WorldPrefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<ClientKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dat_set_id: Option<String>,
    /// The Classic set chosen for the Dereth client, if one was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classic_set_id: Option<String>,
    /// The art pack chosen for the Dereth client; empty when none was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub art_pack_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// Seconds since the epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_played: Option<u64>,
}

/// Launcher-wide settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Where private dat sets go. `None` is `library` in the launcher's data folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_dir: Option<PathBuf>,
    /// Offer "Play anyway" on a blocked check. For when the registry is wrong.
    #[serde(default)]
    pub allow_play_anyway: bool,
    #[serde(default = "yes")]
    pub auto_update: bool,
    /// "Show what I'm playing on Discord": while a game the launcher started is running, the
    /// Discord app on this computer shows its world, client and era ([`crate::discord`]).
    #[serde(default = "yes")]
    pub discord_presence: bool,
    /// The game the page last showed, which it opens in again ([`Realm`]).
    #[serde(default, skip_serializing_if = "Realm::is_dereth")]
    pub realm: Realm,
}

fn yes() -> bool {
    true
}

/// The realm the launcher's page shows: Dereth's or Auberean's. Every launcher offers both; a first
/// run opens in the one its package names ([`crate::defaults`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Realm {
    #[default]
    Dereth,
    Auberean,
}

impl Realm {
    fn is_dereth(&self) -> bool {
        *self == Realm::Dereth
    }

    /// The game whose worlds the realm lists.
    #[must_use]
    pub fn game(self) -> Game {
        match self {
            Realm::Dereth => Game::Dereth,
            Realm::Auberean => Game::Auberean,
        }
    }

    /// Whether the realm lists `world`: a world of its own game, and no other.
    #[must_use]
    pub fn shows(self, world: &World) -> bool {
        world.game == self.game()
    }
}

/// Read leniently: a realm this launcher does not know, from a newer one, is Dereth, rather than
/// the whole file set aside.
impl<'de> Deserialize<'de> for Realm {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match Option::<String>::deserialize(d)?.as_deref() {
            Some("auberean") => Realm::Auberean,
            _ => Realm::Dereth,
        })
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            library_dir: None,
            allow_play_anyway: false,
            auto_update: true,
            discord_presence: true,
            realm: Realm::Dereth,
        }
    }
}

/// What the player told the launcher for Auberean's realm: its library (the retail client and the
/// data sets of their own Asheron's Call 2 installs, as Asheron's Call's are kept), and the account
/// they play a world on this computer with. Never a password.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AubereanSettings {
    /// The one install folder a launcher kept before the realm had a library: read, taken into the
    /// library at start ([`LauncherState::adopt_auberean_install`]), and never written again.
    #[serde(default, skip_serializing)]
    pub install: Option<PathBuf>,
    /// The retail client, `AC2client.exe` ([`crate::auberean::identify_retail`]). It plays with
    /// the data files beside it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retail: Option<Installation>,
    /// Asheron's Call 2's data sets ([`SetKind::Auberean`]), the default one
    /// ([`DatOrigin::Shared`]) the one Auberean, Yalain and Athenaeum are given unless another is
    /// chosen.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dat_sets: Vec<DatSet>,
    /// The account name the player last played with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// The client the player last played with: Auberean, or the retail client in the install.
    #[serde(default, skip_serializing_if = "is_auberean")]
    pub client: crate::auberean::Client,
}

fn is_auberean(client: &crate::auberean::Client) -> bool {
    *client == crate::auberean::Client::Auberean
}

impl AubereanSettings {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The whole file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LauncherState {
    pub schema: u32,
    /// The player's retail client. It plays with the dats in its own folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retail: Option<Installation>,
    #[serde(default)]
    pub dat_sets: Vec<DatSet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub art_packs: Vec<ArtPack>,
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub favourites: Vec<Favourite>,
    /// The worlds the player starred in the world list, by key, which the list shows first. They
    /// are apart from the Play page's favorites: each is kept and removed on its own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub favorite_worlds: Vec<String>,
    /// The last few combinations played, most recent first, each once.
    #[serde(default)]
    pub recent: Vec<Recent>,
    /// Servers the player added by hand.
    #[serde(default)]
    pub custom_worlds: Vec<CustomWorld>,
    #[serde(default)]
    pub world_prefs: BTreeMap<String, WorldPrefs>,
    #[serde(default)]
    pub settings: Settings,
    /// Auberean's realm's: the player's install and account.
    #[serde(default, skip_serializing_if = "AubereanSettings::is_empty")]
    pub auberean: AubereanSettings,
}

impl Default for LauncherState {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            retail: None,
            dat_sets: Vec::new(),
            art_packs: Vec::new(),
            accounts: Vec::new(),
            favourites: Vec::new(),
            favorite_worlds: Vec::new(),
            recent: Vec::new(),
            custom_worlds: Vec::new(),
            world_prefs: BTreeMap::new(),
            settings: Settings::default(),
            auberean: AubereanSettings::default(),
        }
    }
}

/// How a load went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Loaded {
    /// There was no file: a first run.
    Fresh,
    Read,
    /// Some parts of the file did not read: they are reset and every other part kept, and the
    /// file as it was is copied aside under `bad`.
    Partly {
        bad: PathBuf,
        reset: Vec<Unread>,
    },
    /// The file is not a state at all: it was set aside under this name.
    SetAside(PathBuf),
}

/// A part of the state file that did not read, and why, in serde's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unread {
    /// Its key in the file ([`PARTS`]).
    pub part: &'static str,
    pub error: String,
}

/// The state file's parts, by their keys, each read on its own when the whole does not read.
pub const PARTS: [&str; 11] = [
    "schema",
    "retail",
    "dat_sets",
    "art_packs",
    "accounts",
    "favourites",
    "recent",
    "custom_worlds",
    "world_prefs",
    "settings",
    "auberean",
];

/// A part of the state in words for the player: `Some saved settings couldn't be read and were
/// reset: your accounts.`
#[must_use]
pub fn part_name(part: &str) -> &'static str {
    match part {
        "schema" => "the file's version",
        "retail" => "your retail client",
        "dat_sets" => "your data files",
        "art_packs" => "your art packs",
        "accounts" => "your accounts",
        "favourites" => "your favorites",
        "recent" => "your recent games",
        "custom_worlds" => "the worlds you added",
        "world_prefs" => "each world's last choices",
        "settings" => "your settings",
        "auberean" => "Auberean's Library",
        _ => "a part of them",
    }
}

impl LauncherState {
    pub fn load(dir: &Path) -> (Self, Loaded) {
        let path = dir.join(STATE_FILE);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return (Self::default(), Loaded::Fresh)
            }
            Err(_) => return (Self::default(), Loaded::Fresh),
        };
        if let Ok(s) = serde_json::from_slice::<Self>(&bytes) {
            return (s, Loaded::Read);
        }
        let bad = dir.join(format!("{STATE_FILE}.bad"));
        match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(serde_json::Value::Object(parts)) => {
                let (state, reset) = Self::from_parts(&parts);
                // Kept as it was, for whoever looks into what could not be read.
                let _ = std::fs::write(&bad, &bytes);
                (state, Loaded::Partly { bad, reset })
            }
            _ => {
                let _ = std::fs::rename(&path, &bad);
                (Self::default(), Loaded::SetAside(bad))
            }
        }
    }

    /// The state from the file's `parts`, each read on its own: every part that reads, and the
    /// default for each that does not, named with why. A part the file does not have is the
    /// default, as when the whole reads.
    fn from_parts(parts: &serde_json::Map<String, serde_json::Value>) -> (Self, Vec<Unread>) {
        fn take<T: serde::de::DeserializeOwned>(
            parts: &serde_json::Map<String, serde_json::Value>,
            part: &'static str,
            into: &mut T,
            reset: &mut Vec<Unread>,
        ) {
            let Some(value) = parts.get(part) else {
                return;
            };
            match serde_json::from_value(value.clone()) {
                Ok(v) => *into = v,
                Err(e) => reset.push(Unread {
                    part,
                    error: e.to_string(),
                }),
            }
        }
        let mut s = Self::default();
        let mut reset = Vec::new();
        take(parts, "schema", &mut s.schema, &mut reset);
        take(parts, "retail", &mut s.retail, &mut reset);
        take(parts, "dat_sets", &mut s.dat_sets, &mut reset);
        take(parts, "art_packs", &mut s.art_packs, &mut reset);
        take(parts, "accounts", &mut s.accounts, &mut reset);
        take(parts, "favourites", &mut s.favourites, &mut reset);
        take(parts, "recent", &mut s.recent, &mut reset);
        take(parts, "custom_worlds", &mut s.custom_worlds, &mut reset);
        take(parts, "world_prefs", &mut s.world_prefs, &mut reset);
        take(parts, "settings", &mut s.settings, &mut reset);
        take(parts, "auberean", &mut s.auberean, &mut reset);
        (s, reset)
    }

    /// [`Self::load`], and Auberean's realm's file beside it with it ([`AUBEREAN_STATE_FILE`]):
    /// the two as one state. Auberean's entries found in the state file itself (written before the
    /// realm had a file of its own) join those of its file, and both files are written again at
    /// once, so the state file holds none from then on. A world the player added for Auberean
    /// under another name is given one of its own ([`AUBEREAN_WORLD_PREFIX`]); the renames, for
    /// the caller to move what is kept by a world's name elsewhere. A file of Auberean's that does
    /// not read is copied aside as `auberean-state.json.bad`, and said as its Library's.
    pub fn load_moving(dir: &Path) -> (Self, Loaded, Vec<Renamed>) {
        let (mut state, mut loaded) = Self::load(dir);
        let held = state.auberean_part();
        let moved_out = !held.is_empty();
        let path = dir.join(AUBEREAN_STATE_FILE);
        let own = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<AubereanFile>(&bytes) {
                Ok(f) => f,
                Err(e) => {
                    let bad = dir.join(format!("{AUBEREAN_STATE_FILE}.bad"));
                    let _ = std::fs::write(&bad, &bytes);
                    let parts = serde_json::from_slice::<serde_json::Value>(&bytes).ok();
                    let f = match parts {
                        Some(serde_json::Value::Object(parts)) => {
                            AubereanFile::from_parts(&parts).0
                        }
                        _ => AubereanFile::default(),
                    };
                    let unread = Unread {
                        part: "auberean",
                        error: e.to_string(),
                    };
                    loaded = match loaded {
                        Loaded::Partly { bad, mut reset } => {
                            reset.push(unread);
                            Loaded::Partly { bad, reset }
                        }
                        Loaded::SetAside(bad) => Loaded::SetAside(bad),
                        Loaded::Fresh | Loaded::Read => Loaded::Partly {
                            bad,
                            reset: vec![unread],
                        },
                    };
                    f
                }
            },
            Err(_) => AubereanFile::default(),
        };
        state.join_auberean(own, held);
        let renamed = state.name_auberean_worlds();
        if (moved_out || !renamed.is_empty()) && !matches!(loaded, Loaded::SetAside(_)) {
            let _ = state.save(dir);
        }
        (state, loaded, renamed)
    }

    /// Whether the world the player added as `slug` is one of Auberean's.
    fn is_auberean_world(&self, slug: &str) -> bool {
        self.custom_worlds
            .iter()
            .any(|w| w.slug == slug && w.game == Game::Auberean)
    }

    /// Whether an entry for `slug` played with `client` is Auberean's realm's: the Auberean
    /// client, or a world added for Auberean.
    fn is_auberean_entry(&self, slug: &str, client: Option<ClientKind>) -> bool {
        client == Some(ClientKind::Auberean) || self.is_auberean_world(slug)
    }

    /// Auberean's realm's part of the state, as its file holds it.
    fn auberean_part(&self) -> AubereanFile {
        let ours = |slug: &str, client| self.is_auberean_entry(slug, client);
        AubereanFile {
            library: self.auberean.clone(),
            accounts: self
                .accounts
                .iter()
                .filter(|a| self.is_auberean_world(&a.world_slug))
                .cloned()
                .collect(),
            favourites: self
                .favourites
                .iter()
                .filter(|f| ours(&f.world_slug, Some(f.client)))
                .cloned()
                .collect(),
            recent: self
                .recent
                .iter()
                .filter(|r| ours(&r.world_slug, Some(r.client)))
                .cloned()
                .collect(),
            custom_worlds: self
                .custom_worlds
                .iter()
                .filter(|w| w.game == Game::Auberean)
                .cloned()
                .collect(),
            world_prefs: self
                .world_prefs
                .iter()
                .filter(|(slug, p)| ours(slug, p.client))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        }
    }

    /// The state without Auberean's realm's part: what the state file holds, which a launcher from
    /// before the realm reads whole.
    fn without_auberean(&self) -> Self {
        let mut s = self.clone();
        let aub: std::collections::HashSet<String> = self
            .custom_worlds
            .iter()
            .filter(|w| w.game == Game::Auberean)
            .map(|w| w.slug.clone())
            .collect();
        let ours = |slug: &str, client: Option<ClientKind>| {
            client == Some(ClientKind::Auberean) || aub.contains(slug)
        };
        s.auberean = AubereanSettings::default();
        s.accounts.retain(|a| !aub.contains(&a.world_slug));
        s.favourites
            .retain(|f| !ours(&f.world_slug, Some(f.client)));
        s.recent.retain(|r| !ours(&r.world_slug, Some(r.client)));
        s.custom_worlds.retain(|w| w.game != Game::Auberean);
        s.world_prefs.retain(|slug, p| !ours(slug, p.client));
        s
    }

    /// Take in Auberean's realm's file `own`, and the part of it the state file still `held`
    /// (written before the realm had a file of its own): each entry once, the file's where both
    /// have it, the recent combinations newest first.
    fn join_auberean(&mut self, own: AubereanFile, held: AubereanFile) {
        let mut state = self.without_auberean();
        state.auberean = if own.library.is_empty() {
            held.library
        } else {
            own.library
        };
        for a in own.accounts.into_iter().chain(held.accounts) {
            if state.account(&a.world_slug, &a.username).is_none() {
                state.accounts.push(a);
            }
        }
        for f in own.favourites.into_iter().chain(held.favourites) {
            if state.favourites.iter().all(|x| x.id != f.id) {
                state.favourites.push(f);
            }
        }
        for r in own.recent.into_iter().chain(held.recent) {
            let known = state
                .recent
                .iter()
                .any(|x| x.same_as(&r.world_slug, &r.account, r.client, r.dat_set_id.as_deref()));
            if !known {
                state.recent.push(r);
            }
        }
        state
            .recent
            .sort_by_key(|r| core::cmp::Reverse(r.last_played));
        state.recent.truncate(RECENT_LIMIT);
        for w in own.custom_worlds.into_iter().chain(held.custom_worlds) {
            if state.custom_worlds.iter().all(|x| x.slug != w.slug) {
                state.custom_worlds.push(w);
            }
        }
        for (slug, p) in held.world_prefs.into_iter().chain(own.world_prefs) {
            state.world_prefs.insert(slug, p);
        }
        *self = state;
    }

    /// Give each world the player added for Auberean that is named otherwise a name of its own,
    /// `auberean-<n>`, its entries following it. The renames.
    fn name_auberean_worlds(&mut self) -> Vec<Renamed> {
        let old: Vec<String> = self
            .custom_worlds
            .iter()
            .filter(|w| w.game == Game::Auberean && !w.slug.starts_with(AUBEREAN_WORLD_PREFIX))
            .map(|w| w.slug.clone())
            .collect();
        let mut renamed = Vec::new();
        for from in old {
            let to = self.free_slug(AUBEREAN_WORLD_PREFIX);
            for w in &mut self.custom_worlds {
                if w.slug == from {
                    w.slug.clone_from(&to);
                }
            }
            for a in &mut self.accounts {
                if a.world_slug == from {
                    a.world_slug.clone_from(&to);
                }
            }
            for f in &mut self.favourites {
                if f.world_slug == from {
                    f.world_slug.clone_from(&to);
                }
            }
            for r in &mut self.recent {
                if r.world_slug == from {
                    r.world_slug.clone_from(&to);
                }
            }
            if let Some(p) = self.world_prefs.remove(&from) {
                self.world_prefs.insert(to.clone(), p);
            }
            renamed.push(Renamed { from, to });
        }
        renamed
    }

    /// The first name `<prefix><n>` no world the player added has.
    fn free_slug(&self, prefix: &str) -> String {
        (1u32..)
            .map(|n| format!("{prefix}{n}"))
            .find(|s| self.custom_worlds.iter().all(|w| &w.slug != s))
            .expect("some slug is free")
    }

    /// Write atomically: a temporary file, then a rename over the old one. Auberean's realm's part
    /// goes in its own file first ([`AUBEREAN_STATE_FILE`], gone when there is none), then the
    /// rest in the state file.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let own = self.auberean_part();
        let path = dir.join(AUBEREAN_STATE_FILE);
        if own.is_empty() {
            match std::fs::remove_file(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        } else {
            let json = serde_json::to_vec_pretty(&own).map_err(io::Error::other)?;
            write_atomic(&path, &json)?;
        }
        let json = serde_json::to_vec_pretty(&self.without_auberean()).map_err(io::Error::other)?;
        write_atomic(&dir.join(STATE_FILE), &json)
    }

    /// Where private dat sets go, given the launcher's data folder.
    pub fn library_dir(&self, data_dir: &Path) -> PathBuf {
        self.settings
            .library_dir
            .clone()
            .unwrap_or_else(|| data_dir.join(crate::folders::LIBRARY_DIR))
    }

    pub fn dat_set(&self, id: &str) -> Option<&DatSet> {
        self.dat_sets.iter().find(|s| s.id == id)
    }

    pub fn dat_set_mut(&mut self, id: &str) -> Option<&mut DatSet> {
        self.dat_sets.iter_mut().find(|s| s.id == id)
    }

    /// The default Modern set.
    pub fn shared_set(&self) -> Option<&DatSet> {
        self.default_set(SetKind::Modern)
    }

    /// The default set of a kind.
    pub fn default_set(&self, kind: SetKind) -> Option<&DatSet> {
        self.dat_sets
            .iter()
            .find(|s| s.kind == kind && s.origin == DatOrigin::Shared)
    }

    /// The art pack `id`.
    pub fn art_pack(&self, id: &str) -> Option<&ArtPack> {
        self.art_packs.iter().find(|p| p.id == id)
    }

    /// Add the art pack at `path` to the Library, the first of them the default. One already
    /// there is left as it is.
    ///
    /// # Errors
    /// Why it is not an art pack this client can draw, as a sentence.
    pub fn add_art_pack(&mut self, path: &Path) -> Result<(), String> {
        let name = read_art_pack(path)?;
        if self.art_packs.iter().all(|p| p.path != path) {
            self.art_packs.push(ArtPack {
                id: self.new_id("pack"),
                name,
                path: path.to_path_buf(),
                is_default: self.art_packs.is_empty(),
            });
        }
        Ok(())
    }

    /// Make the art pack `id` the default. Whether there is one.
    pub fn set_default_art_pack(&mut self, id: &str) -> bool {
        if self.art_pack(id).is_none() {
            return false;
        }
        for p in &mut self.art_packs {
            p.is_default = p.id == id;
        }
        true
    }

    /// Take the art pack `id` out of the Library. Its file stays where it is.
    pub fn remove_art_pack(&mut self, id: &str) {
        self.art_packs.retain(|p| p.id != id);
    }

    /// The art pack a game of the world `slug` draws: `chosen`, empty for none; else what was
    /// chosen there last; else the default.
    pub fn art_pack_for(&self, slug: &str, chosen: Option<&str>) -> Option<&ArtPack> {
        let last = self
            .world_prefs
            .get(slug)
            .and_then(|p| p.art_pack_id.as_deref());
        match chosen.or(last) {
            Some("") => None,
            Some(id) => self
                .art_pack(id)
                .or_else(|| self.art_packs.iter().find(|p| p.is_default)),
            None => self.art_packs.iter().find(|p| p.is_default),
        }
    }

    /// The private set for a world, if one has been made.
    pub fn private_set_for(&self, slug: &str) -> Option<&DatSet> {
        self.dat_sets.iter().find(|s| s.owner_world() == Some(slug))
    }

    pub fn accounts_for<'a>(&'a self, slug: &'a str) -> impl Iterator<Item = &'a Account> + 'a {
        self.accounts.iter().filter(move |a| a.world_slug == slug)
    }

    pub fn account(&self, slug: &str, username: &str) -> Option<&Account> {
        self.accounts
            .iter()
            .find(|a| a.world_slug == slug && a.username.eq_ignore_ascii_case(username))
    }

    /// Add or update an account. Account names are case-insensitive, as the client lower-cases them.
    pub fn upsert_account(&mut self, account: Account) {
        match self.accounts.iter_mut().find(|a| {
            a.world_slug == account.world_slug && a.username.eq_ignore_ascii_case(&account.username)
        }) {
            Some(a) => *a = account,
            None => self.accounts.push(account),
        }
    }

    /// Forget an account and every favourite that used it. The caller deletes its vault entry.
    pub fn forget_account(&mut self, slug: &str, username: &str) {
        self.accounts
            .retain(|a| !(a.world_slug == slug && a.username.eq_ignore_ascii_case(username)));
        self.favourites
            .retain(|f| !(f.world_slug == slug && f.account.eq_ignore_ascii_case(username)));
        self.recent
            .retain(|r| !(r.world_slug == slug && r.account.eq_ignore_ascii_case(username)));
        if let Some(p) = self.world_prefs.get_mut(slug) {
            if p.account
                .as_deref()
                .is_some_and(|a| a.eq_ignore_ascii_case(username))
            {
                p.account = None;
            }
        }
    }

    /// Forget the retail client. Nothing on disk is touched; a favourite that used it stays and
    /// shows the check's verdict instead.
    pub fn forget_retail(&mut self) {
        self.retail = None;
    }

    /// The data sets of `game`'s library: Asheron's Call's, or Auberean's realm's own.
    pub fn sets(&self, game: Game) -> &Vec<DatSet> {
        match game {
            Game::Dereth => &self.dat_sets,
            Game::Auberean => &self.auberean.dat_sets,
        }
    }

    /// [`LauncherState::sets`], to change.
    pub fn sets_mut(&mut self, game: Game) -> &mut Vec<DatSet> {
        match game {
            Game::Dereth => &mut self.dat_sets,
            Game::Auberean => &mut self.auberean.dat_sets,
        }
    }

    /// The retail client of `game`'s library, to change: `acclient.exe`, or `AC2client.exe`.
    pub fn retail_mut(&mut self, game: Game) -> &mut Option<Installation> {
        match game {
            Game::Dereth => &mut self.retail,
            Game::Auberean => &mut self.auberean.retail,
        }
    }

    /// An Asheron's Call 2 data set, by id.
    pub fn auberean_set(&self, id: &str) -> Option<&DatSet> {
        self.auberean.dat_sets.iter().find(|s| s.id == id)
    }

    /// The default Asheron's Call 2 data set: the one the programs are given unless another is
    /// chosen.
    pub fn auberean_default_set(&self) -> Option<&DatSet> {
        self.auberean
            .dat_sets
            .iter()
            .find(|s| s.origin == DatOrigin::Shared)
    }

    /// Take the one install folder an older launcher kept for Auberean's realm into its library:
    /// its retail client and its data set, as if the player had added the folder there. Whether it
    /// was taken.
    pub fn adopt_auberean_install(&mut self) -> bool {
        let Some(dir) = self.auberean.install.take() else {
            return false;
        };
        if !self.auberean.dat_sets.is_empty() || self.auberean.retail.is_some() {
            return false;
        }
        let find = crate::auberean::read_folder(&dir, self);
        crate::library::add_find(self, find);
        true
    }

    /// Worlds played, most recent first.
    pub fn recents(&self) -> Vec<(&str, &WorldPrefs)> {
        let mut v: Vec<_> = self
            .world_prefs
            .iter()
            .filter(|(_, p)| p.last_played.is_some())
            .map(|(k, p)| (k.as_str(), p))
            .collect();
        v.sort_by_key(|(_, p)| core::cmp::Reverse(p.last_played));
        v
    }

    /// Record a launch as this world's remembered choice.
    pub fn remember_launch(&mut self, slug: &str, prefs: WorldPrefs) {
        self.world_prefs.insert(slug.to_owned(), prefs);
    }

    /// Add a server typed by hand. The name defaults to the host. The same host and port added again
    /// is the same server, renamed.
    ///
    /// # Errors
    /// [`CustomWorldError`] for a missing or malformed host, or a port outside 1..=65535.
    /// Say which game the world the player added as `slug` runs. False when there is no such world.
    pub fn set_custom_world_game(&mut self, slug: &str, game: Game) -> bool {
        match self.custom_worlds.iter_mut().find(|w| w.slug == slug) {
            Some(w) => {
                w.game = game;
                true
            }
            None => false,
        }
    }

    pub fn add_custom_world(
        &mut self,
        name: &str,
        host: &str,
        port: &str,
        ruleset: Option<&str>,
        emulator: Emulator,
    ) -> Result<String, CustomWorldError> {
        self.add_custom_world_for(Game::Dereth, name, host, port, ruleset, emulator)
    }

    /// [`Self::add_custom_world`] for a world of `game`: one for Auberean is named
    /// `auberean-<n>` ([`AUBEREAN_WORLD_PREFIX`]).
    ///
    /// # Errors
    /// As [`Self::add_custom_world`].
    pub fn add_custom_world_for(
        &mut self,
        game: Game,
        name: &str,
        host: &str,
        port: &str,
        ruleset: Option<&str>,
        emulator: Emulator,
    ) -> Result<String, CustomWorldError> {
        let host = custom_host(host)?;
        let port = custom_port(port)?;
        let name = if name.trim().is_empty() {
            host
        } else {
            name.trim()
        }
        .to_owned();
        let ruleset = ruleset
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(str::to_owned);
        if let Some(w) = self
            .custom_worlds
            .iter_mut()
            .find(|w| w.host.eq_ignore_ascii_case(host) && w.port == port)
        {
            w.name = name;
            w.ruleset = ruleset;
            w.emulator = emulator;
            return Ok(w.slug.clone());
        }
        let slug = self.free_slug(match game {
            Game::Dereth => "custom-",
            Game::Auberean => AUBEREAN_WORLD_PREFIX,
        });
        self.custom_worlds.push(CustomWorld {
            slug: slug.clone(),
            name,
            host: host.to_owned(),
            port,
            ruleset,
            emulator,
            url: None,
            game,
        });
        Ok(slug)
    }

    /// The URL the server the player added as `slug` was added by (its WebSocket endpoint or its
    /// status document), or none. Nothing for a slug that is no server the player added.
    pub fn set_custom_world_url(&mut self, slug: &str, url: Option<&str>) {
        if let Some(w) = self.custom_worlds.iter_mut().find(|w| w.slug == slug) {
            w.url = url
                .map(str::trim)
                .filter(|u| !u.is_empty())
                .map(str::to_owned);
        }
    }

    /// Whether the world kept under `slug` is starred in the world list.
    #[must_use]
    pub fn is_favorite_world(&self, slug: &str) -> bool {
        self.favorite_worlds.iter().any(|s| s == slug)
    }

    /// Star the world kept under `slug` in the world list, or take its star away.
    pub fn set_favorite_world(&mut self, slug: &str, favorite: bool) {
        self.favorite_worlds.retain(|s| s != slug);
        if favorite {
            self.favorite_worlds.push(slug.to_owned());
        }
    }

    /// Remove a server the player added, with its accounts, favourites and remembered choices. The
    /// caller deletes the accounts' vault entries first.
    pub fn remove_custom_world(&mut self, slug: &str) {
        self.custom_worlds.retain(|w| w.slug != slug);
        self.accounts.retain(|a| a.world_slug != slug);
        self.favourites.retain(|f| f.world_slug != slug);
        self.favorite_worlds.retain(|s| s != slug);
        self.recent.retain(|r| r.world_slug != slug);
        self.world_prefs.remove(slug);
    }

    /// Forget the recent and favourite combinations whose world is no longer listed: not among
    /// `listed` and not a server the player added. Answers whether anything was forgotten. The
    /// caller passes a list it has just read whole, never an empty one from a failed fetch.
    pub fn forget_unlisted(&mut self, listed: &std::collections::HashSet<&str>) -> bool {
        let custom: std::collections::HashSet<String> =
            self.custom_worlds.iter().map(|w| w.slug.clone()).collect();
        let keep = |slug: &str| listed.contains(slug) || custom.contains(slug);
        let before = self.recent.len() + self.favourites.len() + self.favorite_worlds.len();
        self.recent.retain(|r| keep(&r.world_slug));
        self.favourites.retain(|f| keep(&f.world_slug));
        self.favorite_worlds.retain(|s| keep(s));
        before != self.recent.len() + self.favourites.len() + self.favorite_worlds.len()
    }

    /// Record a combination as the most recent, keeping each combination once and the last
    /// [`RECENT_LIMIT`].
    pub fn record_recent(&mut self, recent: Recent) {
        self.recent.retain(|r| {
            !r.same_as(
                &recent.world_slug,
                &recent.account,
                recent.client,
                recent.dat_set_id.as_deref(),
            )
        });
        self.recent.insert(0, recent);
        self.recent.truncate(RECENT_LIMIT);
    }

    /// Make the set `id` the default of its kind, the one the Dereth client is offered first (in
    /// Auberean's realm, the one its programs are given). Only a set the player added can be the
    /// default: a world's private copy or a custom set belongs to its world. The other kind's
    /// default is untouched.
    pub fn set_default_set(&mut self, id: &str) -> bool {
        let game = if self.dat_set(id).is_some() {
            Game::Dereth
        } else {
            Game::Auberean
        };
        let Some(kind) = self
            .sets(game)
            .iter()
            .find(|s| s.id == id)
            .filter(|s| matches!(s.origin, DatOrigin::Shared | DatOrigin::Unassigned))
            .map(|s| s.kind)
        else {
            return false;
        };
        for s in self.sets_mut(game) {
            if s.kind == kind && s.origin == DatOrigin::Shared {
                s.origin = DatOrigin::Unassigned;
            }
            if s.id == id {
                s.origin = DatOrigin::Shared;
            }
        }
        true
    }

    /// A fresh id, unique within this state. Not a UUID: the launcher has no randomness to spend on
    /// it and needs uniqueness only within one file.
    pub fn new_id(&self, prefix: &str) -> String {
        let taken = |id: &str| {
            id == SHARED_SET_ID
                || self.dat_sets.iter().any(|s| s.id == id)
                || self.art_packs.iter().any(|p| p.id == id)
                || self.auberean.dat_sets.iter().any(|s| s.id == id)
                || self.favourites.iter().any(|f| f.id == id)
        };
        (1u32..)
            .map(|n| format!("{prefix}{n}"))
            .find(|id| !taken(id))
            .expect("some id is free")
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the launcher's state file: what it keeps, and how it reads an older one).
    use super::*;
    use crate::datset::tests::tmp;

    fn account(slug: &str, user: &str) -> Account {
        Account {
            world_slug: slug.into(),
            username: user.into(),
            label: None,
            remember: true,
            created_by_launcher: false,
        }
    }

    /// A state the 0.4.9 launcher wrote, every part filled through its own calls and saved by its
    /// own code (its paths made plain).
    const STATE_0_4_9: &str = include_str!("../fixtures/launcher-state-0.4.9.json");

    /// Whether every value `written` holds is in `read` as it was, at the same place: what a
    /// launcher wrote reads unchanged, whatever a newer one adds beside it.
    fn holds(read: &serde_json::Value, written: &serde_json::Value) -> bool {
        match (read, written) {
            (serde_json::Value::Object(r), serde_json::Value::Object(w)) => w
                .iter()
                .all(|(k, v)| r.get(k).is_some_and(|got| holds(got, v))),
            (serde_json::Value::Array(r), serde_json::Value::Array(w)) => {
                r.len() == w.len() && r.iter().zip(w).all(|(a, b)| holds(a, b))
            }
            _ => read == written,
        }
    }

    /// A state the 0.4.9 launcher wrote reads whole in this launcher, every part as it was.
    #[test]
    fn a_state_the_0_4_9_launcher_wrote_reads_unchanged() {
        let d = tmp("state-0.4.9");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(STATE_FILE), STATE_0_4_9).unwrap();
        let (s, how) = LauncherState::load(&d);
        assert_eq!(how, Loaded::Read);
        let written: serde_json::Value = serde_json::from_str(STATE_0_4_9).unwrap();
        let read = serde_json::to_value(&s).unwrap();
        assert!(holds(&read, &written), "{read:#}");
        assert_eq!(s.accounts.len(), 2);
        assert_eq!(s.recent.len(), 2);
        assert_eq!(s.custom_worlds[0].name, "Home");
        assert!(!s.settings.auto_update && !s.settings.discord_presence);
        assert_eq!(s.settings.realm, Realm::Auberean);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A state with parts this launcher cannot read keeps every other part: only those are reset,
    /// each named with why, and the file as it was is copied aside, where it stays.
    #[test]
    fn a_part_that_does_not_read_is_reset_and_every_other_part_kept() {
        let d = tmp("state-partly");
        std::fs::create_dir_all(&d).unwrap();
        let mut v: serde_json::Value = serde_json::from_str(STATE_0_4_9).unwrap();
        v["recent"][0]["client"] = "a client from elsewhere".into();
        v["settings"]["auto_update"] = "not a yes or a no".into();
        v["from_a_newer_launcher"] = serde_json::json!({ "anything": [1, 2] });
        let bytes = serde_json::to_vec_pretty(&v).unwrap();
        std::fs::write(d.join(STATE_FILE), &bytes).unwrap();
        let (s, how) = LauncherState::load(&d);
        let Loaded::Partly { bad, reset } = how else {
            panic!("{how:?}");
        };
        assert_eq!(
            reset.iter().map(|r| r.part).collect::<Vec<_>>(),
            ["recent", "settings"]
        );
        assert!(reset[0].error.contains("unknown variant"), "{reset:?}");
        assert_eq!(std::fs::read(&bad).unwrap(), bytes, "kept as it was");
        assert!(d.join(STATE_FILE).is_file(), "the file itself stays");
        let whole: LauncherState = serde_json::from_str(STATE_0_4_9).unwrap();
        assert_eq!(s.recent, Vec::new());
        assert_eq!(s.settings, Settings::default());
        assert_eq!(
            (&s.retail, &s.dat_sets, &s.accounts, &s.favourites),
            (
                &whole.retail,
                &whole.dat_sets,
                &whole.accounts,
                &whole.favourites
            )
        );
        assert_eq!(
            (&s.custom_worlds, &s.world_prefs),
            (&whole.custom_worlds, &whole.world_prefs)
        );
        assert_eq!(part_name("recent"), "your recent games");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Every part of the file is one the load reads on its own.
    #[test]
    fn every_part_of_the_file_is_read_on_its_own() {
        let mut s: LauncherState = serde_json::from_str(STATE_0_4_9).unwrap();
        s.auberean.account = Some("player".into());
        s.art_packs.push(ArtPack {
            id: "a1".into(),
            name: "Pack".into(),
            path: PathBuf::from("pack.dat"),
            is_default: true,
        });
        let v = serde_json::to_value(&s).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        let mut parts = PARTS.to_vec();
        parts.sort_unstable();
        let mut keys = keys;
        keys.sort_unstable();
        assert_eq!(keys, parts);
        for part in PARTS {
            assert_ne!(part_name(part), "a part of them", "{part}");
        }
    }

    /// The 0.4.9 launcher's state with Auberean's realm played beside it: its Library, a world
    /// added for it (`slug`) with an account, a recent combination, a favourite and a remembered
    /// choice there, and a recent combination with Auberean on a listed world.
    fn with_auberean_played(slug: &str) -> LauncherState {
        let mut s: LauncherState = serde_json::from_str(STATE_0_4_9).unwrap();
        s.auberean.account = Some("player".into());
        s.auberean.client = crate::auberean::Client::Retail;
        s.custom_worlds.push(CustomWorld {
            slug: slug.into(),
            name: "Trial".into(),
            host: "127.0.0.1".into(),
            port: 9200,
            ruleset: None,
            emulator: Emulator::Yalain,
            url: None,
            game: Game::Auberean,
        });
        s.upsert_account(account(slug, "trialist"));
        s.favourites.push(Favourite {
            id: "f-ac2".into(),
            name: None,
            world_slug: slug.into(),
            account: "trialist".into(),
            client: ClientKind::Retail,
            dat_set_id: None,
            classic_set_id: None,
        });
        for (world, at) in [(slug, 30), ("shardfall", 40)] {
            s.record_recent(Recent {
                world_slug: world.into(),
                account: "trialist".into(),
                client: ClientKind::Auberean,
                dat_set_id: Some("a1".into()),
                classic_set_id: None,
                last_played: at,
            });
        }
        s.remember_launch(
            slug,
            WorldPrefs {
                client: Some(ClientKind::Auberean),
                account: Some("trialist".into()),
                last_played: Some(30),
                ..Default::default()
            },
        );
        s
    }

    /// After Auberean's realm is played, the state file holds exactly what the 0.4.9 launcher
    /// wrote, nothing it cannot read, so that launcher reads it whole; Auberean's part is in its
    /// own file, and the two read back as the one state.
    #[test]
    fn auberean_s_part_is_kept_apart_where_the_0_4_9_launcher_never_reads() {
        let d = tmp("state-apart");
        let s = with_auberean_played("auberean-1");
        s.save(&d).unwrap();
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(d.join(STATE_FILE)).unwrap()).unwrap();
        let before: serde_json::Value = serde_json::from_str(STATE_0_4_9).unwrap();
        assert_eq!(
            written, before,
            "the state file is the 0.4.9 launcher's own"
        );
        let own = std::fs::read_to_string(d.join(AUBEREAN_STATE_FILE)).unwrap();
        for part in [
            "\"library\"",
            "auberean-1",
            "trialist",
            "shardfall",
            "\"yalain\"",
        ] {
            assert!(own.contains(part), "{part}: {own}");
        }
        let (back, how, renamed) = LauncherState::load_moving(&d);
        assert_eq!((how, renamed), (Loaded::Read, Vec::new()));
        assert_eq!(back, s);
        // With no Auberean part left, its file goes.
        LauncherState::default().save(&d).unwrap();
        assert!(!d.join(AUBEREAN_STATE_FILE).exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A state written before Auberean's realm had a file of its own, its part in the state file
    /// and its world named `custom-<n>`, has that part moved to the realm's file the first time it
    /// is read, its world given a name of its own with everything that names it; the second read
    /// moves nothing.
    #[test]
    fn auberean_s_part_in_the_state_file_moves_out_on_the_first_read() {
        let d = tmp("state-move");
        std::fs::create_dir_all(&d).unwrap();
        let old = with_auberean_played("custom-2");
        std::fs::write(d.join(STATE_FILE), serde_json::to_vec_pretty(&old).unwrap()).unwrap();
        let (s, how, renamed) = LauncherState::load_moving(&d);
        assert_eq!(how, Loaded::Read);
        assert_eq!(
            renamed,
            [Renamed {
                from: "custom-2".into(),
                to: "auberean-1".into(),
            }]
        );
        assert_eq!(s, with_auberean_played("auberean-1"));
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(d.join(STATE_FILE)).unwrap()).unwrap();
        assert_eq!(
            written,
            serde_json::from_str::<serde_json::Value>(STATE_0_4_9).unwrap()
        );
        let (again, _, renamed) = LauncherState::load_moving(&d);
        assert!(renamed.is_empty());
        assert_eq!(again, s);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A world the 0.4.9 launcher adds while Auberean's realm has its own takes the first free
    /// `custom-<n>`, which no world of Auberean's is ever named: both are kept, each with its own.
    #[test]
    fn a_world_the_0_4_9_launcher_adds_never_meets_one_of_auberean_s() {
        let d = tmp("state-names");
        let mut s = with_auberean_played("auberean-1");
        s.save(&d).unwrap();
        // The 0.4.9 launcher reads its file, adds a world, and writes it back.
        let mut older: LauncherState =
            serde_json::from_slice(&std::fs::read(d.join(STATE_FILE)).unwrap()).unwrap();
        let slug = older
            .add_custom_world("Mine", "play.example", "9000", None, Emulator::Ace)
            .unwrap();
        assert_eq!(slug, "custom-2");
        std::fs::write(
            d.join(STATE_FILE),
            serde_json::to_vec_pretty(&older).unwrap(),
        )
        .unwrap();
        let (back, _, renamed) = LauncherState::load_moving(&d);
        assert!(renamed.is_empty());
        assert!(back.is_auberean_world("auberean-1"));
        assert!(back
            .custom_worlds
            .iter()
            .any(|w| w.slug == "custom-2" && w.game == Game::Dereth));
        // A world added for Auberean now is named as its own.
        assert_eq!(
            s.add_custom_world_for(
                Game::Auberean,
                "Next",
                "127.0.0.1",
                "9300",
                None,
                Emulator::Yalain
            )
            .unwrap(),
            "auberean-2"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn state_round_trips_and_the_write_is_atomic() {
        let d = tmp("state");
        let mut s = LauncherState::default();
        s.upsert_account(account("eulmore", "player"));
        s.world_prefs.insert(
            "eulmore".into(),
            WorldPrefs {
                art_pack_id: None,
                last_played: Some(5),
                ..Default::default()
            },
        );
        s.save(&d).unwrap();
        assert!(
            !d.join(format!("{STATE_FILE}.tmp")).exists(),
            "the temporary file is renamed away"
        );
        let (back, how) = LauncherState::load(&d);
        assert_eq!(how, Loaded::Read);
        assert_eq!(back, s);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_first_run_is_fresh_and_a_damaged_file_is_set_aside() {
        let d = tmp("state-bad");
        assert_eq!(LauncherState::load(&d).1, Loaded::Fresh);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(STATE_FILE), b"{ not json").unwrap();
        let (s, how) = LauncherState::load(&d);
        assert_eq!(s, LauncherState::default());
        assert!(matches!(how, Loaded::SetAside(p) if p.exists()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_older_file_with_fields_missing_still_reads() {
        let s: LauncherState = serde_json::from_str(r#"{"schema":1}"#).unwrap();
        assert!(
            s.settings.auto_update,
            "absent means the default, which is on"
        );
        assert!(
            s.settings.discord_presence,
            "showing the game on Discord is on until the player turns it off"
        );
        let s: LauncherState =
            serde_json::from_str(r#"{"schema":1,"settings":{"discord_presence":false}}"#).unwrap();
        assert!(!s.settings.discord_presence && s.settings.auto_update);
        assert_eq!(
            s.settings.realm,
            Realm::Dereth,
            "Dereth until another is picked"
        );
    }

    /// Each realm lists the worlds of its own game alone: Dereth's the worlds that name no game
    /// or `dereth`, Auberean's the `auberean` ones.
    #[test]
    fn each_realm_lists_only_its_own_games_worlds() {
        let mut dereth = World::new("coldeve", "Coldeve");
        dereth.game = Game::Dereth;
        let mut auberean = World::new("shardfall", "Shardfall");
        auberean.game = Game::Auberean;
        assert!(Realm::Dereth.shows(&dereth) && !Realm::Dereth.shows(&auberean));
        assert!(Realm::Auberean.shows(&auberean) && !Realm::Auberean.shows(&dereth));
        assert!(Realm::Dereth.shows(&World::new("plain", "Plain")));
        assert_eq!(Realm::default().game(), Game::Dereth);
    }

    /// A world starred in the world list is a favorite; a Play favorite is not a starred world,
    /// and each is kept and removed on its own. Starred worlds are kept in the launcher's file,
    /// written only when there are any, and forgotten with the world.
    #[test]
    fn a_starred_world_is_a_favorite_apart_from_the_play_favorites_and_is_kept() {
        let mut s = LauncherState::default();
        assert!(!s.is_favorite_world("coldeve"));
        s.set_favorite_world("coldeve", true);
        s.set_favorite_world("coldeve", true);
        assert_eq!(s.favorite_worlds, ["coldeve"], "starred once");
        s.favourites.push(Favourite {
            id: "f1".into(),
            name: None,
            world_slug: "seedsow".into(),
            account: "player".into(),
            client: ClientKind::Dereth,
            dat_set_id: None,
            classic_set_id: None,
        });
        assert!(
            !s.is_favorite_world("seedsow"),
            "a Play favorite is not a star"
        );
        s.set_favorite_world("seedsow", true);
        s.set_favorite_world("seedsow", false);
        assert!(!s.is_favorite_world("seedsow"));
        assert_eq!(s.favourites.len(), 1, "the Play favorite is left alone");

        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains(r#""favorite_worlds":["coldeve"]"#), "{json}");
        let back: LauncherState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.favorite_worlds, ["coldeve"]);
        assert!(
            !serde_json::to_string(&LauncherState::default())
                .unwrap()
                .contains("favorite_worlds"),
            "none: not written"
        );
        let older: LauncherState = serde_json::from_str(r#"{"schema":1}"#).unwrap();
        assert!(older.favorite_worlds.is_empty(), "a file from before reads");

        let listed: std::collections::HashSet<&str> = ["seedsow"].into_iter().collect();
        assert!(s.forget_unlisted(&listed));
        assert!(s.favorite_worlds.is_empty(), "a world no longer listed");
    }

    #[test]
    fn the_realm_round_trips_and_one_this_launcher_does_not_know_reads_as_dereth() {
        let mut s = LauncherState::default();
        s.settings.realm = Realm::Auberean;
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains(r#""realm":"auberean""#), "{json}");
        let back: LauncherState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.settings.realm, Realm::Auberean);
        assert!(
            !serde_json::to_string(&LauncherState::default())
                .unwrap()
                .contains("realm"),
            "Dereth, the default, is not written"
        );
        let newer: LauncherState = serde_json::from_str(
            r#"{"schema":1,"settings":{"realm":"somewhere","discord_presence":false}}"#,
        )
        .unwrap();
        assert_eq!(newer.settings.realm, Realm::Dereth);
        assert!(
            !newer.settings.discord_presence,
            "the rest of the file is kept"
        );
    }

    #[test]
    fn account_names_are_case_insensitive_and_forgetting_one_forgets_its_favourites() {
        let mut s = LauncherState::default();
        s.upsert_account(account("eulmore", "Player"));
        s.upsert_account(account("eulmore", "player"));
        assert_eq!(s.accounts.len(), 1);
        s.favourites.push(Favourite {
            id: "f1".into(),
            name: None,
            world_slug: "eulmore".into(),
            account: "player".into(),
            client: ClientKind::Dereth,
            dat_set_id: None,
            classic_set_id: None,
        });
        s.world_prefs.insert(
            "eulmore".into(),
            WorldPrefs {
                art_pack_id: None,
                account: Some("PLAYER".into()),
                ..Default::default()
            },
        );
        s.forget_account("eulmore", "player");
        assert!(s.accounts.is_empty() && s.favourites.is_empty());
        assert_eq!(s.world_prefs["eulmore"].account, None);
    }

    #[test]
    fn a_recent_combination_is_kept_once_and_only_the_last_few_are_kept() {
        let mut s = LauncherState::default();
        let r = |world: &str, t: u64| Recent {
            world_slug: world.into(),
            account: "player".into(),
            client: ClientKind::Dereth,
            dat_set_id: Some("eor".into()),
            classic_set_id: None,
            last_played: t,
        };
        s.record_recent(r("a", 1));
        s.record_recent(r("b", 2));
        s.record_recent(Recent {
            account: "PLAYER".into(),
            ..r("a", 3)
        });
        assert_eq!(
            s.recent
                .iter()
                .map(|x| x.world_slug.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        for (i, w) in ["c", "d", "e", "f", "g"].iter().enumerate() {
            s.record_recent(r(w, 10 + i as u64));
        }
        assert_eq!(s.recent.len(), RECENT_LIMIT);
        assert_eq!(s.recent[0].world_slug, "g");
    }

    #[test]
    fn a_typed_server_is_checked_kept_once_and_removed_with_what_used_it() {
        let mut s = LauncherState::default();
        assert_eq!(
            s.add_custom_world("", "", "9000", None, Emulator::Unknown),
            Err(CustomWorldError::NoHost)
        );
        assert_eq!(
            s.add_custom_world("", "a:1", "9000", None, Emulator::Unknown),
            Err(CustomWorldError::BadHost)
        );
        assert_eq!(
            s.add_custom_world("", "a", "0", None, Emulator::Unknown),
            Err(CustomWorldError::BadPort)
        );
        let slug = s
            .add_custom_world("", " play.example ", "9000", Some("PvP"), Emulator::Unknown)
            .unwrap();
        assert_eq!(slug, "custom-1");
        assert_eq!(s.custom_worlds[0].name, "play.example");
        assert_eq!(
            s.add_custom_world(
                "Mine",
                "PLAY.example",
                "9000",
                Some(" PvE "),
                Emulator::ClassicAce
            )
            .unwrap(),
            "custom-1"
        );
        assert_eq!(
            (s.custom_worlds.len(), s.custom_worlds[0].name.as_str()),
            (1, "Mine")
        );
        let w = s.custom_worlds[0].to_world();
        assert_eq!(w.ruleset.as_deref(), Some("PvE"), "the rules said last");
        assert_eq!(w.emulator, Emulator::ClassicAce, "and the emulator");
        assert_eq!(
            w.endpoint.as_ref().map(|e| (e.address.as_str(), e.port)),
            Some(("play.example", 9000))
        );

        s.upsert_account(account(&slug, "player"));
        s.remove_custom_world(&slug);
        assert!(s.custom_worlds.is_empty() && s.accounts.is_empty());
    }

    /// A state file from a launcher that asked the player for a world's era still reads, with
    /// everything else in it; the eras and systems chosen then are passed over, and gone from the
    /// file once it is written again.
    #[test]
    fn eras_once_chosen_by_the_player_are_read_past_and_dropped() {
        let d = tmp("state-eras");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join(STATE_FILE),
            r#"{"schema":1,"custom_worlds":[{"slug":"custom-1","name":"Home","host":"127.0.0.1","port":9000}],
               "world_eras":{"leafcull":{"era":"infiltration","features":{"aetheria":true}},
                             "coldeve":{"features":{"trade":false}}}}"#,
        )
        .unwrap();
        let (s, how) = LauncherState::load(&d);
        assert_eq!(how, Loaded::Read, "not set aside as unreadable");
        assert_eq!(
            s.custom_worlds[0].name, "Home",
            "the rest of the file stands"
        );
        s.save(&d).unwrap();
        let written = std::fs::read_to_string(d.join(STATE_FILE)).unwrap();
        assert!(
            !written.contains("world_eras") && !written.contains("infiltration"),
            "{written}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn each_kind_of_set_has_its_own_default() {
        let set = |id: &str, kind: SetKind, origin: DatOrigin| DatSet {
            id: id.into(),
            path: PathBuf::from(id),
            kind,
            origin,
            files: vec![],
            last_patched_by_server: None,
            created_by_launcher: false,
        };
        let mut s = LauncherState {
            dat_sets: vec![
                set("eor", SetKind::Modern, DatOrigin::Shared),
                set("m2", SetKind::Modern, DatOrigin::Unassigned),
                set("c1", SetKind::Classic, DatOrigin::Shared),
                set("c2", SetKind::Classic, DatOrigin::Unassigned),
            ],
            ..Default::default()
        };
        let id = |d: Option<&DatSet>| d.map(|d| d.id.clone());
        assert_eq!(id(s.shared_set()), Some("eor".into()));
        assert_eq!(id(s.default_set(SetKind::Classic)), Some("c1".into()));
        assert!(s.set_default_set("c2"));
        assert_eq!(id(s.default_set(SetKind::Classic)), Some("c2".into()));
        assert_eq!(
            id(s.shared_set()),
            Some("eor".into()),
            "the Modern default stays"
        );
        assert!(s.set_default_set("m2"));
        assert_eq!(id(s.shared_set()), Some("m2".into()));
        assert_eq!(id(s.default_set(SetKind::Classic)), Some("c2".into()));
    }

    #[test]
    fn play_again_forgets_worlds_no_longer_listed_and_keeps_the_players_own_servers() {
        let mut s = LauncherState::default();
        let slug = s
            .add_custom_world("", "127.0.0.1", "9000", None, Emulator::Unknown)
            .unwrap();
        for world in ["leafcull", "gone", slug.as_str()] {
            s.record_recent(Recent {
                world_slug: world.into(),
                account: "player".into(),
                client: ClientKind::Dereth,
                dat_set_id: None,
                classic_set_id: None,
                last_played: 1,
            });
            s.favourites.push(Favourite {
                id: format!("f-{world}"),
                name: None,
                world_slug: world.into(),
                account: "player".into(),
                client: ClientKind::Dereth,
                dat_set_id: None,
                classic_set_id: None,
            });
        }
        let listed: std::collections::HashSet<&str> = ["leafcull"].into_iter().collect();
        assert!(s.forget_unlisted(&listed));
        let mut recent: Vec<&str> = s.recent.iter().map(|r| r.world_slug.as_str()).collect();
        let mut favs: Vec<&str> = s.favourites.iter().map(|f| f.world_slug.as_str()).collect();
        let mut want = vec!["leafcull", slug.as_str()];
        recent.sort_unstable();
        favs.sort_unstable();
        want.sort_unstable();
        assert_eq!(recent, want);
        assert_eq!(favs, want);
        assert!(!s.forget_unlisted(&listed), "nothing more to forget");
    }

    #[test]
    fn recents_are_most_recent_first() {
        let mut s = LauncherState::default();
        for (slug, t) in [("a", 1), ("b", 3), ("c", 2)] {
            s.world_prefs.insert(
                slug.into(),
                WorldPrefs {
                    art_pack_id: None,
                    last_played: Some(t),
                    ..Default::default()
                },
            );
        }
        s.world_prefs.insert("never".into(), WorldPrefs::default());
        let order: Vec<_> = s.recents().iter().map(|(k, _)| *k).collect();
        assert_eq!(order, ["b", "c", "a"]);
    }

    #[test]
    fn new_ids_do_not_collide_or_take_the_shared_sets_name() {
        let s = LauncherState::default();
        assert_eq!(s.new_id("d"), "d1");
        assert_ne!(s.new_id("eo"), "eor");
    }

    /// Auberean's realm's library and account are kept, and a file without them reads as it did;
    /// nothing of them is written until they are given.
    #[test]
    fn the_auberean_library_and_account_round_trip_and_are_written_only_once_given() {
        let mut s = LauncherState::default();
        assert!(!serde_json::to_string(&s).unwrap().contains("auberean"));
        s.auberean.dat_sets.push(DatSet {
            id: "a1".into(),
            path: PathBuf::from("ac2"),
            kind: SetKind::Auberean,
            origin: DatOrigin::Shared,
            files: Vec::new(),
            last_patched_by_server: None,
            created_by_launcher: false,
        });
        s.auberean.account = Some("player".into());
        let json = serde_json::to_string(&s).unwrap();
        let back: LauncherState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.auberean, s.auberean);
        let older: LauncherState = serde_json::from_str(r#"{"schema":1}"#).unwrap();
        assert_eq!(older.auberean, AubereanSettings::default());
    }

    /// The rule this file exists to keep.
    #[test]
    fn no_password_field_exists_anywhere_in_the_file() {
        let mut s = LauncherState::default();
        s.upsert_account(account("eulmore", "player"));
        let json = serde_json::to_string(&s).unwrap().to_ascii_lowercase();
        assert!(!json.contains("password"), "{json}");
    }

    /// A world added to Auberean's realm, by hand or from a join link, is listed there and kept
    /// as Auberean's; one added as before is Dereth's, and its saved form says no game.
    #[test]
    fn an_added_world_keeps_the_game_it_was_added_for() {
        let mut s = LauncherState::default();
        let dereth = s
            .add_custom_world("A", "a.example", "9000", None, Emulator::Unknown)
            .unwrap();
        let auberean = s
            .add_custom_world("B", "b.example", "9200", None, Emulator::Unknown)
            .unwrap();
        assert!(s.set_custom_world_game(&auberean, Game::Auberean));
        assert!(!s.set_custom_world_game("custom-99", Game::Auberean));
        let json = serde_json::to_string(&s).unwrap();
        let back: LauncherState = serde_json::from_str(&json).unwrap();
        let world = |slug: &str| {
            back.custom_worlds
                .iter()
                .find(|w| w.slug == slug)
                .unwrap()
                .to_world()
        };
        assert_eq!(world(&dereth).game, Game::Dereth);
        assert_eq!(world(&auberean).game, Game::Auberean);
        assert!(Realm::Auberean.shows(&world(&auberean)));
        assert!(!Realm::Dereth.shows(&world(&auberean)));
        assert_eq!(json.matches("\"game\"").count(), 1, "{json}");
    }
}
