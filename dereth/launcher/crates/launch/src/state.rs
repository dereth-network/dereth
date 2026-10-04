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
//! or a full disk mid-write leaves the previous state intact rather than half a file. A file that
//! cannot be read at all is set aside as `launcher-state.json.bad` and the launcher starts fresh; a
//! player loses their favourites, not their ability to play.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::datset::{DatOrigin, DatSet, SetKind, SHARED_SET_ID};
use crate::eras::EraChoice;
use crate::install::{ClientKind, Installation};
use crate::world::{Emulator, Endpoint, World};

pub const STATE_FILE: &str = "launcher-state.json";
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
}

impl CustomWorld {
    /// The world record the rest of the launcher works with. Its status and software are not
    /// known; its rules are what the player said.
    pub fn to_world(&self) -> World {
        let mut w = World::new(self.slug.clone(), self.name.clone());
        w.ruleset.clone_from(&self.ruleset);
        w.emulator = self.emulator;
        w.endpoint = Some(Endpoint {
            address: self.host.clone(),
            port: self.port,
            transport: None,
        });
        w.description = Some(format!("Added by you: {}:{}", self.host, self.port));
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
            CustomWorldError::NoHost => "Enter the server's host name or address.",
            CustomWorldError::BadHost => "The host cannot contain spaces, a colon or a slash.",
            CustomWorldError::BadPort => "The port must be a number from 1 to 65535.",
        })
    }
}

impl std::error::Error for CustomWorldError {}

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
}

fn yes() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            library_dir: None,
            allow_play_anyway: false,
            auto_update: true,
        }
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
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub favourites: Vec<Favourite>,
    /// The last few combinations played, most recent first, each once.
    #[serde(default)]
    pub recent: Vec<Recent>,
    /// Servers the player added by hand.
    #[serde(default)]
    pub custom_worlds: Vec<CustomWorld>,
    #[serde(default)]
    pub world_prefs: BTreeMap<String, WorldPrefs>,
    /// The era and systems the player chose, per world, for worlds that do not say theirs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub world_eras: BTreeMap<String, EraChoice>,
    #[serde(default)]
    pub settings: Settings,
}

impl Default for LauncherState {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            retail: None,
            dat_sets: Vec::new(),
            accounts: Vec::new(),
            favourites: Vec::new(),
            recent: Vec::new(),
            custom_worlds: Vec::new(),
            world_prefs: BTreeMap::new(),
            world_eras: BTreeMap::new(),
            settings: Settings::default(),
        }
    }
}

/// How a load went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Loaded {
    /// There was no file: a first run.
    Fresh,
    Read,
    /// The file could not be read and was set aside under this name.
    SetAside(PathBuf),
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
        match serde_json::from_slice::<Self>(&bytes) {
            Ok(s) => (s, Loaded::Read),
            Err(_) => {
                let bad = dir.join(format!("{STATE_FILE}.bad"));
                let _ = std::fs::rename(&path, &bad);
                (Self::default(), Loaded::SetAside(bad))
            }
        }
    }

    /// Write atomically: a temporary file, then a rename over the old one.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let json = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        let tmp = dir.join(format!("{STATE_FILE}.tmp"));
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&json)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, dir.join(STATE_FILE))
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

    /// Choose a world's era (`None`: not chosen), forgetting the choice when nothing is left of it.
    pub fn set_world_era(&mut self, slug: &str, era: Option<&str>) {
        self.world_eras
            .entry(slug.to_owned())
            .or_default()
            .set_era(era);
        self.drop_empty_era_choice(slug);
    }

    /// Turn one of a world's systems on or off over its chosen era's table. Answers whether
    /// `name` is a system's.
    pub fn set_world_feature(&mut self, slug: &str, name: &str, on: bool) -> bool {
        let known = self
            .world_eras
            .entry(slug.to_owned())
            .or_default()
            .set_feature(name, on);
        self.drop_empty_era_choice(slug);
        known
    }

    fn drop_empty_era_choice(&mut self, slug: &str) {
        if self.world_eras.get(slug).is_some_and(EraChoice::is_empty) {
            self.world_eras.remove(slug);
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
    pub fn add_custom_world(
        &mut self,
        name: &str,
        host: &str,
        port: &str,
        ruleset: Option<&str>,
        emulator: Emulator,
    ) -> Result<String, CustomWorldError> {
        let host = host.trim();
        if host.is_empty() {
            return Err(CustomWorldError::NoHost);
        }
        if host.contains(|c: char| c.is_whitespace() || c == ':' || c == '/') {
            return Err(CustomWorldError::BadHost);
        }
        let port = port
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|p| *p != 0)
            .ok_or(CustomWorldError::BadPort)?;
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
        let slug = (1u32..)
            .map(|n| format!("custom-{n}"))
            .find(|s| self.custom_worlds.iter().all(|w| &w.slug != s))
            .expect("some slug is free");
        self.custom_worlds.push(CustomWorld {
            slug: slug.clone(),
            name,
            host: host.to_owned(),
            port,
            ruleset,
            emulator,
        });
        Ok(slug)
    }

    /// Remove a server the player added, with its accounts, favourites and remembered choices. The
    /// caller deletes the accounts' vault entries first.
    pub fn remove_custom_world(&mut self, slug: &str) {
        self.custom_worlds.retain(|w| w.slug != slug);
        self.accounts.retain(|a| a.world_slug != slug);
        self.favourites.retain(|f| f.world_slug != slug);
        self.recent.retain(|r| r.world_slug != slug);
        self.world_prefs.remove(slug);
        self.world_eras.remove(slug);
    }

    /// Forget the recent and favourite combinations whose world is no longer listed: not among
    /// `listed` and not a server the player added. Answers whether anything was forgotten. The
    /// caller passes a list it has just read whole, never an empty one from a failed fetch.
    pub fn forget_unlisted(&mut self, listed: &std::collections::HashSet<&str>) -> bool {
        let custom: std::collections::HashSet<String> =
            self.custom_worlds.iter().map(|w| w.slug.clone()).collect();
        let keep = |slug: &str| listed.contains(slug) || custom.contains(slug);
        let before = self.recent.len() + self.favourites.len();
        self.recent.retain(|r| keep(&r.world_slug));
        self.favourites.retain(|f| keep(&f.world_slug));
        before != self.recent.len() + self.favourites.len()
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

    /// Make the set `id` the default of its kind, the one the Dereth client is offered first. Only
    /// a set the player added can be the default: a world's private copy or a custom set belongs to
    /// its world. The other kind's default is untouched.
    pub fn set_default_set(&mut self, id: &str) -> bool {
        let Some(kind) = self
            .dat_set(id)
            .filter(|s| matches!(s.origin, DatOrigin::Shared | DatOrigin::Unassigned))
            .map(|s| s.kind)
        else {
            return false;
        };
        for s in &mut self.dat_sets {
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

    #[test]
    fn state_round_trips_and_the_write_is_atomic() {
        let d = tmp("state");
        let mut s = LauncherState::default();
        s.upsert_account(account("eulmore", "player"));
        s.world_prefs.insert(
            "eulmore".into(),
            WorldPrefs {
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
        s.set_world_era(&slug, Some("infiltration"));
        s.remove_custom_world(&slug);
        assert!(s.custom_worlds.is_empty() && s.accounts.is_empty());
        assert!(s.world_eras.is_empty(), "its era choice goes with it");
    }

    #[test]
    fn a_worlds_era_and_systems_are_kept_per_world_and_round_trip() {
        let d = tmp("state-eras");
        let mut s = LauncherState::default();
        s.set_world_era("leafcull", Some("infiltration"));
        assert!(s.set_world_feature("leafcull", "aetheria", true));
        assert!(!s.set_world_feature("leafcull", "nothing", true));
        assert!(s.set_world_feature("coldeve", "trade", false));
        s.save(&d).unwrap();
        let (back, _) = LauncherState::load(&d);
        assert_eq!(back.world_eras, s.world_eras);
        let c = &back.world_eras["leafcull"];
        assert_eq!(c.era.as_deref(), Some("infiltration"));
        assert_eq!(c.features_text().as_deref(), Some("aetheria=true"));
        assert_eq!(
            back.world_eras["coldeve"].era, None,
            "systems without an era are over the end of retail's table"
        );

        // Undoing every choice forgets the world's entry.
        s.set_world_feature("coldeve", "trade", true);
        assert!(!s.world_eras.contains_key("coldeve"));
        s.set_world_feature("leafcull", "aetheria", false);
        s.set_world_era("leafcull", None);
        assert!(s.world_eras.is_empty());
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

    /// The rule this file exists to keep.
    #[test]
    fn no_password_field_exists_anywhere_in_the_file() {
        let mut s = LauncherState::default();
        s.upsert_account(account("eulmore", "player"));
        let json = serde_json::to_string(&s).unwrap().to_ascii_lowercase();
        assert!(!json.contains("password"), "{json}");
    }
}
