//! The front page's launcher: the worlds and what each says, what the player chose and added, the
//! data sets the player keeps in this browser and which of them a world takes, and what the client
//! is told about the world. All of it is the desktop launcher's own logic (`dereth-launch`): the
//! world list read and kept as its window reads and keeps it ([`WorldList`]: the day's copy, the
//! sources in their order, each address fetched by the page), the table of worlds that run a client
//! of their own, a world shown as its window shows it, its sets matched by iterations as its
//! library matches them, and the client told about the world in the words its command line
//! carries.
//!
//! The page keeps the launcher's state record between visits (as JSON in the browser's storage),
//! and the day's copy of the list; this holds them while the page is open and answers the page's
//! questions as JSON. While the worlds are shown, the hub's list is read again about once a minute
//! for whether each world is up and how many are on, by the desktop launcher's rules
//! ([`dereth_launch::hubstatus`]); the page fetches each page of it it is given. The same read
//! brings what each Empyrean world told the hub when it asked (its era, systems and version),
//! which the desktop launcher asks the world itself. A browser cannot ask a server over UDP, so a
//! world with no status document, of which the hub has no word, is shown with its state unknown,
//! and an Empyrean world that has told nobody its era is shown with its era being checked until
//! the hub's word is read, then unknown.
//!
//! **Both games.** The page shows every world, each naming its game. A world of Asheron's Call 2's
//! has its realm's own data sets ([`Front::add_auberean_sets`]), checked as the desktop launcher
//! checks them, and its client is told nothing about the world: what Auberean needs of a world is
//! its address.
//!
//! **Plugins.** The page installs plugins from plugin sources as the launcher does, with the
//! launcher's own shelf of sources: [`crate::plugin_sources`].

use std::collections::BTreeMap;
use std::path::PathBuf;

use dereth_launch::auberean;
use dereth_launch::choices;
use dereth_launch::datset::{self, DatOrigin, DatSet, SetKind};
use dereth_launch::eras;
use dereth_launch::install::ClientKind;
use dereth_launch::join::JoinLink;
use dereth_launch::launch;
use dereth_launch::listing::WorldList;
use dereth_launch::serverlist::{self, ListCache, ListFetch};
use dereth_launch::state::{Account, Favourite, LauncherState, Realm, Recent, WorldPrefs};
use dereth_launch::status::{self, LiveStatus};
use dereth_launch::world::{AccountModel, Emulator, Game, World};
use serde_json::{json, Value};

/// The launcher behind the front page.
#[derive(Debug, Default)]
pub struct Front {
    /// The world list, its day's copy, and the hub's word on its worlds.
    list: WorldList,
    state: LauncherState,
    live: BTreeMap<String, LiveStatus>,
    /// Worlds whose status document did not answer: shown as down.
    unanswered: BTreeMap<String, bool>,
    /// The list being read, while the page fetches it.
    fetch: Option<ListFetch>,
    /// The hub's list being read for status, while the page fetches it.
    hub_read: Option<ListFetch>,
}

/// The name of the folder a world's overlay is kept in, from the world's address (`host:port`):
/// the desktop client's per-server folder name, so the browser keys a world's overlay as the
/// desktop does.
#[must_use]
pub fn overlay_folder(address: &str) -> String {
    let spec = dereth_client_net::socket::parse_host_spec(address.trim(), 9000);
    dereth_client_runtime::world_overlay::folder_name(&spec.host, u32::from(spec.port))
}

impl Front {
    /// Take the day's copy of the world list as the page kept it (the desktop launcher's copy, as
    /// JSON), for the list read from the sources the page names: the `Servers.xml` `named` (its
    /// `list` parameter), or another hub's list (`hub`, its `hub` parameter). Answers how many
    /// worlds it lists.
    ///
    /// # Errors
    /// The copy does not read, or was read from other sources; the worlds read before stand.
    pub fn set_copy(
        &mut self,
        copy: &str,
        named: Option<&str>,
        hub: Option<&str>,
    ) -> Result<usize, String> {
        self.list.set_sources(serverlist::sources_from(named, hub));
        let copy: ListCache = serde_json::from_str(copy).map_err(|e| e.to_string())?;
        self.list.show_copy(copy)
    }

    /// Whether the list is to be read at `now` (seconds since the epoch), as the page opens: no
    /// copy of it is shown, or the copy is not the day's.
    #[must_use]
    pub fn list_wanted(&self, now: u64) -> bool {
        self.list.wants_reading(now)
    }

    /// Start reading the world list from its sources: the `Servers.xml` `named` alone (the page's
    /// `list` parameter) when one is named, and otherwise the hub's list (the network's, or the one
    /// `hub` names, the page's `hub` parameter), then the community's own. Answers the first
    /// address to fetch.
    pub fn list_start(&mut self, named: Option<&str>, hub: Option<&str>) -> Option<String> {
        self.list.set_sources(serverlist::sources_from(named, hub));
        let fetch = self.list.reading();
        let first = fetch.next_url().map(str::to_owned);
        self.fetch = Some(fetch);
        first
    }

    /// What fetching the address last given brought: its body, or why there is none. Answers the
    /// next address to fetch, or nothing once the reading is over.
    pub fn list_next(&mut self, fetched: Result<Vec<u8>, String>) -> Option<String> {
        let fetch = self.fetch.as_mut()?;
        let url = fetch.next_url().unwrap_or_default().to_owned();
        fetch.take(fetched.map_err(|e| format!("{url}: {e}")));
        fetch.next_url().map(str::to_owned)
    }

    /// The list read: it becomes the world list, and the hub's brings each world's status as the
    /// hub has it now. Answers how many worlds it lists, where it came from and the day's copy
    /// (fetched at `now`), for the page to keep, as JSON.
    ///
    /// # Errors
    /// Each source's failure when none answered with its list; the worlds read before stand.
    pub fn list_done(&mut self, now: u64) -> Result<String, String> {
        let fetch = self
            .fetch
            .take()
            .ok_or("the world list is not being read")?;
        let from = fetch.source().map(|s| s.url.clone());
        let copy = self.list.read(fetch, now)?.clone();
        Ok(json!({"worlds": self.list.worlds().len(), "from": from, "copy": copy}).to_string())
    }

    /// Start reading the hub's list (the network's, or the one `hub` names) for each world's
    /// status when a read is due at `now`: about a minute after the last, and only while the worlds
    /// shown are the hub's list's and the page names no list of its own (`named`, its `list`
    /// parameter). Answers the first address to fetch; nothing when no read is due. The page reads
    /// one at a time, so a read it left unfinished was lost, and ends as one that failed.
    pub fn hub_start(
        &mut self,
        named: Option<&str>,
        hub: Option<&str>,
        now: u64,
    ) -> Option<String> {
        if let Some(lost) = self.hub_read.take() {
            self.list.hub_mut().finish(lost, now);
        }
        self.list.set_sources(serverlist::sources_from(named, hub));
        let read = self.list.hub_mut().start(now)?;
        let first = read.next_url().map(str::to_owned);
        self.hub_read = Some(read);
        first
    }

    /// What fetching the address last given for the status brought: its body, or why there is
    /// none. Answers the next address to fetch, or nothing once the read is over.
    pub fn hub_next(&mut self, fetched: Result<Vec<u8>, String>) -> Option<String> {
        let read = self.hub_read.as_mut()?;
        let url = read.next_url().unwrap_or_default().to_owned();
        read.take(fetched.map_err(|e| format!("{url}: {e}")));
        read.next_url().map(str::to_owned)
    }

    /// The status read is over, at `now`. Answers whether it brought the hub's word; a read that
    /// failed changes nothing.
    pub fn hub_done(&mut self, now: u64) -> bool {
        self.hub_read
            .take()
            .is_some_and(|read| self.list.hub_mut().finish(read, now))
    }

    /// Take the launcher's state record as the page kept it (empty for none).
    ///
    /// # Errors
    /// A record that does not read; the state before stands.
    pub fn set_state(&mut self, json: &str) -> Result<(), String> {
        self.state = if json.trim().is_empty() {
            LauncherState::default()
        } else {
            serde_json::from_str(json).map_err(|e| e.to_string())?
        };
        Ok(())
    }

    /// The state record, for the page to keep.
    #[must_use]
    pub fn state_json(&self) -> String {
        serde_json::to_string(&self.state).unwrap_or_default()
    }

    /// What a world's status document says (`body`, as fetched). Answers whether it was one.
    pub fn set_status(&mut self, slug: &str, body: &[u8]) -> bool {
        match status::parse_world_document(body) {
            Some(s) => {
                self.unanswered.remove(slug);
                self.live.insert(slug.to_owned(), s);
                true
            }
            None => {
                self.status_failed(slug);
                false
            }
        }
    }

    /// A world's status document did not answer.
    pub fn status_failed(&mut self, slug: &str) {
        self.live.remove(slug);
        self.unanswered.insert(slug.to_owned(), false);
    }

    /// Every world as it is shown at `now` (seconds since the epoch), of either game: the page
    /// shows a realm's worlds in that realm. A world whose status document did not answer is shown
    /// as down.
    fn shown(&self, now: u64) -> impl Iterator<Item = World> + '_ {
        self.list.shown(&self.state, now, |slug| {
            (self.live.get(slug), self.unanswered.get(slug).copied())
        })
    }

    /// `w` for the page: the world, whether the player added it, and the address it is played at.
    fn world_json(&self, w: &World) -> Value {
        let mut v = serde_json::to_value(w).unwrap_or(Value::Null);
        v["added"] = json!(self.state.custom_worlds.iter().any(|c| c.slug == w.slug));
        v["address"] = json!(address(w));
        v
    }

    /// The world shown for `slug` at `now`: the world itself, or, for a server the player added
    /// that the list names too, the list's world.
    fn world_of(&self, slug: &str, now: u64) -> Option<World> {
        self.list.shown_as(
            &self.state,
            now,
            |slug| (self.live.get(slug), self.unanswered.get(slug).copied()),
            slug,
        )
    }

    /// Every world as it is shown at `now` (seconds since the epoch), with the eras and systems
    /// the page offers for those that do not say theirs.
    #[must_use]
    pub fn worlds(&self, now: u64) -> Value {
        let worlds: Vec<Value> = self.shown(now).map(|w| self.world_json(&w)).collect();
        json!({
            "worlds": worlds,
            "eras": eras::eras(),
            "features": eras::features(),
        })
    }

    /// One world's page: the world, the data sets it takes (and why each other one does not do),
    /// the set its world is drawn from, what the client is told about it, the player's last
    /// choices there, and the folder its overlay is kept in, as shown at `now` (seconds since the
    /// epoch). `None` for a world not shown.
    #[must_use]
    pub fn world(&self, slug: &str, now: u64) -> Option<Value> {
        let w = self.world_of(slug, now)?;
        // A world of Asheron's Call 2's plays one of its realm's own sets, and its client needs
        // only its address.
        if w.game == Game::Auberean {
            return Some(json!({
                "world": self.world_json(&w),
                "dat_sets": [],
                "refused_sets": [],
                "classic_sets": [],
                "args": [],
                "prefs": self.state.world_prefs.get(slug),
                "accounts": self.state.accounts_for(&w.slug).collect::<Vec<_>>(),
                "address": address(&w),
                "overlay_folder": null,
                "join_link": self.join_link(&w),
            }));
        }
        let taken = choices::dat_sets_for(&self.state, &w);
        let refused: Vec<Value> = self
            .state
            .dat_sets
            .iter()
            .filter(|s| s.kind == SetKind::Modern && !taken.iter().any(|t| t.id == s.id))
            .map(|s| {
                let why = choices::set_matches(&w, s)
                    .err()
                    .unwrap_or_else(|| format!("These data files are not offered for {}.", w.name));
                json!({ "id": s.id, "why": why })
            })
            .collect();
        Some(json!({
            "world": self.world_json(&w),
            "dat_sets": taken,
            "refused_sets": refused,
            "classic_sets": choices::classic_sets_for(&self.state, &w),
            "launch": launch::describe(&w),
            "args": launch::describe(&w).args(),
            "prefs": self.state.world_prefs.get(slug),
            "accounts": self.state.accounts_for(&w.slug).collect::<Vec<_>>(),
            "address": address(&w),
            "overlay_folder": address(&w).map(|a| overlay_folder(&a)),
            "join_link": self.join_link(&w),
        }))
    }

    /// The link a player sends a friend for world `w`: the site's join page with the world's game
    /// address and name. None for a world with no game address, or one the player added by a URL,
    /// whose port is the URL's rather than the game's.
    fn join_link(&self, w: &World) -> Option<String> {
        let added_by_url = self
            .state
            .custom_worlds
            .iter()
            .any(|c| c.slug == w.slug && c.url.is_some());
        let e = w.endpoint.as_ref().filter(|_| !added_by_url)?;
        Some(
            JoinLink {
                host: e.address.clone(),
                port: e.port,
                name: w.name.clone(),
                game: w.game,
            }
            .web_link(),
        )
    }

    /// Add a server of Asheron's Call's by its host and port.
    ///
    /// # Errors
    /// What is wrong with the host or the port, for the player.
    pub fn add_world(
        &mut self,
        name: &str,
        host: &str,
        port: &str,
        ruleset: &str,
        emulator: &str,
    ) -> Result<String, String> {
        self.add_world_for("dereth", name, host, port, ruleset, emulator)
    }

    /// [`Front::add_world`], for a server of `game`, as the hub writes it; any other word is
    /// Asheron's Call's.
    ///
    /// # Errors
    /// As [`Front::add_world`].
    pub fn add_world_for(
        &mut self,
        game: &str,
        name: &str,
        host: &str,
        port: &str,
        ruleset: &str,
        emulator: &str,
    ) -> Result<String, String> {
        let ruleset = Some(ruleset).filter(|r| !r.trim().is_empty());
        self.state
            .add_custom_world_for(
                Game::parse(game).unwrap_or_default(),
                name,
                host,
                port,
                ruleset,
                Emulator::parse(emulator),
            )
            .map_err(|e| e.to_string())
    }

    /// The URL the server the player added as `slug` was added by: its WebSocket endpoint, or its
    /// status document. A server the list names too, by either, is shown once, as the list's world.
    pub fn added_by(&mut self, slug: &str, url: &str) {
        self.state.set_custom_world_url(slug, Some(url));
    }

    /// Take a server the player added off the list, with what was kept for it.
    pub fn remove_world(&mut self, slug: &str) {
        self.state.remove_custom_world(slug);
        self.live.remove(slug);
        self.unanswered.remove(slug);
    }

    /// A start on world `slug` at `now` (seconds since the epoch), as the desktop launcher records
    /// one: the account among the world's accounts, the choices as the world's own, and the
    /// combination as the most recent. `prefs` (JSON) names the account and the data sets; the
    /// client is the one this browser runs for the world's game. No password is kept: the
    /// browser's password manager keeps it.
    ///
    /// # Errors
    /// Choices that do not read.
    pub fn record_launch(&mut self, slug: &str, prefs: &str, now: u64) -> Result<(), String> {
        let mut prefs: WorldPrefs = serde_json::from_str(prefs).map_err(|e| e.to_string())?;
        let client = if self
            .world_of(slug, now)
            .is_some_and(|w| w.game == Game::Auberean)
        {
            ClientKind::Auberean
        } else {
            ClientKind::Dereth
        };
        prefs.client = Some(client);
        prefs.last_played = Some(now);
        let account = prefs
            .account
            .as_deref()
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .map(str::to_owned);
        prefs.account.clone_from(&account);
        if let Some(account) = &account {
            let existing = self.state.account(slug, account).cloned();
            let creates = self
                .world_of(slug, now)
                .is_some_and(|w| w.account_model == AccountModel::AutoCreateOnFirstLogin);
            self.state.upsert_account(Account {
                world_slug: slug.to_owned(),
                username: account.clone(),
                label: existing.as_ref().and_then(|a| a.label.clone()),
                remember: false,
                created_by_launcher: existing.map_or(creates, |a| a.created_by_launcher),
            });
            self.state.record_recent(Recent {
                world_slug: slug.to_owned(),
                account: account.clone(),
                client,
                dat_set_id: prefs.dat_set_id.clone(),
                classic_set_id: prefs.classic_set_id.clone(),
                last_played: now,
            });
        }
        self.state.remember_launch(slug, prefs);
        Ok(())
    }

    /// Pin a combination to Home (JSON, a favourite; an empty id is given a new one). Answers its
    /// id.
    ///
    /// # Errors
    /// A favourite that does not read.
    pub fn save_favourite(&mut self, favourite: &str) -> Result<String, String> {
        let mut f: Favourite = serde_json::from_str(favourite).map_err(|e| e.to_string())?;
        if f.id.is_empty() {
            f.id = self.state.new_id("f");
        }
        self.state.favourites.retain(|x| x.id != f.id);
        let id = f.id.clone();
        self.state.favourites.push(f);
        Ok(id)
    }

    /// Unpin the favourite `id`.
    pub fn remove_favourite(&mut self, id: &str) {
        self.state.favourites.retain(|f| f.id != id);
    }

    /// Star the world `slug` in the world list, or take its star away.
    pub fn set_world_favorite(&mut self, slug: &str, favorite: bool) {
        self.state.set_favorite_world(slug, favorite);
    }

    /// Forget the account `username` on world `slug`, with the favourites and recent combinations
    /// that used it.
    pub fn forget_account(&mut self, slug: &str, username: &str) {
        self.state.forget_account(slug, username);
    }

    /// Forget every account, as [`Front::forget_account`] forgets one.
    pub fn forget_all_accounts(&mut self) {
        let all: Vec<(String, String)> = self
            .state
            .accounts
            .iter()
            .map(|a| (a.world_slug.clone(), a.username.clone()))
            .collect();
        for (slug, username) in all {
            self.state.forget_account(&slug, &username);
        }
    }

    /// Make the data set `id` the default of its kind, the one a world's page offers first.
    ///
    /// # Errors
    /// No such set.
    pub fn set_default_set(&mut self, id: &str) -> Result<(), String> {
        if self.state.set_default_set(id) {
            Ok(())
        } else {
            Err("Those data files are not kept in this browser.".into())
        }
    }

    /// The files kept in the browser's folder `folder`, read: the later set they hold and the
    /// older pair, each a data set of its own as the desktop library lists a folder holding both.
    /// A folder read before is read again in place. Answers the ids of its sets.
    pub fn add_sets(
        &mut self,
        folder: &str,
        files: Vec<(String, u64, Box<dyn dereth_dat::DatStorage>)>,
    ) -> Vec<String> {
        let path = PathBuf::from(folder);
        let mut modern = Vec::new();
        let mut classic = Vec::new();
        for (name, size, storage) in files {
            for kind in [SetKind::Modern, SetKind::Classic] {
                if datset::role_of(kind, &name).is_some() {
                    if let Some(f) = datset::file_in_storage(kind, &name, size, storage) {
                        if kind == SetKind::Classic {
                            classic.push(f);
                        } else {
                            modern.push(f);
                        }
                    }
                    break;
                }
            }
        }
        let mut ids = Vec::new();
        // A pair from 1999 is its own kind, which a Launch world needs.
        let pair = SetKind::of_pair(&classic);
        for (kind, files) in [(SetKind::Modern, modern), (pair, classic)] {
            let existing = self.state.dat_sets.iter().position(|s| {
                s.path == path && (s.kind == kind || s.kind.is_pair() && kind.is_pair())
            });
            if files.is_empty() {
                if let Some(i) = existing {
                    self.state.dat_sets.remove(i);
                }
                continue;
            }
            match existing {
                Some(i) => {
                    self.state.dat_sets[i].files = files;
                    self.state.dat_sets[i].kind = kind;
                    ids.push(self.state.dat_sets[i].id.clone());
                }
                None => {
                    let id = self.state.new_id("web-");
                    // The first set of its kind is its default, as the desktop library makes it.
                    let origin = if self.state.default_set(kind).is_none() {
                        DatOrigin::Shared
                    } else {
                        DatOrigin::Unassigned
                    };
                    self.state.dat_sets.push(DatSet {
                        id: id.clone(),
                        path: path.clone(),
                        kind,
                        origin,
                        files,
                        last_patched_by_server: None,
                        created_by_launcher: true,
                    });
                    ids.push(id);
                }
            }
        }
        ids
    }

    /// The files kept in the browser's folder `folder`, read as a data set of Asheron's Call 2's,
    /// as the desktop library reads a folder for that realm. The first is the realm's default. A
    /// folder read before is read again in place. Answers the set's id.
    pub fn add_auberean_sets(
        &mut self,
        folder: &str,
        files: Vec<(String, u64, Box<dyn dereth_dat::DatStorage>)>,
    ) -> Vec<String> {
        let path = PathBuf::from(folder);
        let files: Vec<_> = files
            .into_iter()
            .filter_map(|(name, size, storage)| {
                datset::file_in_storage(SetKind::Auberean, &name, size, storage)
            })
            .collect();
        if let Some(i) = self
            .state
            .auberean
            .dat_sets
            .iter()
            .position(|s| s.path == path)
        {
            let set = &mut self.state.auberean.dat_sets[i];
            set.files = files;
            return vec![set.id.clone()];
        }
        let id = self.state.new_id("web-a");
        let origin = if self.state.auberean_default_set().is_none() {
            DatOrigin::Shared
        } else {
            DatOrigin::Unassigned
        };
        self.state.auberean.dat_sets.push(DatSet {
            id: id.clone(),
            path,
            kind: SetKind::Auberean,
            origin,
            files,
            last_patched_by_server: None,
            created_by_launcher: true,
        });
        vec![id]
    }

    /// The default Asheron's Call 2 data set's check, as JSON: `{ ok, summary }`, what the page
    /// shows beside Play. With no set, it is not ready and says nothing.
    #[must_use]
    pub fn auberean_check(&self) -> Value {
        self.state.auberean_default_set().map_or_else(
            || json!({"ok": false, "summary": ""}),
            |set| {
                let check = auberean::check_set(set);
                json!({"ok": check.ok, "summary": check.summary})
            },
        )
    }

    /// The realm the page last showed: `dereth` or `auberean`.
    pub fn set_realm(&mut self, realm: &str) {
        self.state.settings.realm = if realm == "auberean" {
            Realm::Auberean
        } else {
            Realm::Dereth
        };
    }

    /// Forget every set kept in the browser's folder `folder`.
    pub fn remove_sets(&mut self, folder: &str) {
        let path = PathBuf::from(folder);
        self.state.dat_sets.retain(|s| s.path != path);
        self.state.auberean.dat_sets.retain(|s| s.path != path);
    }
}

/// A world's game address, `host:port`, when it names one.
fn address(w: &World) -> Option<String> {
    w.endpoint
        .as_ref()
        .map(|e| format!("{}:{}", e.address, e.port))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the front page's launcher is the desktop launcher's logic, run in the page).
    use super::*;
    use dereth_launch::dat::testdat;

    /// 2026-10-11T18:00:00Z, in seconds since the epoch: when the page shows its worlds.
    const NOW: u64 = 1_791_741_600;

    /// The community list as it publishes it: a plain world and one the launcher knows.
    const LIST: &str = r"<?xml version='1.0' encoding='utf-8'?>
<ArrayOfServerItem>
  <ServerItem>
    <id>9d17ce44-7db5-40c1-b7bb-a01c9de21d00</id><name>AChard</name>
    <description>ACE EOR PVP</description><emu>ACE</emu>
    <server_host>a-chard.example</server_host><server_port>9000</server_port>
    <type>PvP</type><status>Stable</status>
  </ServerItem>
  <ServerItem>
    <id>394C58D0-885D-466B-B17F-D7E0B96FE3E2</id><name>Seedsow</name><emu>GDL</emu>
    <server_host>play.seedsow.example</server_host><server_port>9050</server_port>
  </ServerItem>
</ArrayOfServerItem>";

    /// The usual sources' addresses, which a copy read from them records.
    fn usual() -> Vec<String> {
        serverlist::source_urls(&serverlist::sources(None))
    }

    /// The day's copy of `xml`, as the page keeps a `Servers.xml` read from the usual sources.
    fn copy_of(xml: &str) -> String {
        json!({"fetched_at": 1, "list": xml, "sources": usual()}).to_string()
    }

    /// The page reads the list as the desktop launcher does: the hub's first, the community's when
    /// the hub does not answer with its list, a list it names alone, and the last list it read when
    /// none answers.
    #[test]
    fn the_page_reads_the_hubs_list_first_and_the_community_list_when_the_hub_fails() {
        use serverlist::{COMMUNITY_LIST_URL, HUB_LIST_URL};
        let mut f = Front::default();
        let page = json!({"data": [{"slug": "ushores", "name": "Unfamiliar Shores",
            "isDemo": false, "emulator": {"family": "ACE"},
            "endpoint": {"mode": "manual", "address": "192.0.2.7", "port": 9000},
            "provenance": {"externalId": "3f1f41ec-c7fd-4ed9-b47d-25b0d94219c1",
                           "legacyType": "PvE"}}], "nextOffset": null})
        .to_string();
        let hub = f.list_start(None, None).unwrap();
        assert!(hub.starts_with(HUB_LIST_URL), "{hub}");
        assert_eq!(f.list_next(Ok(page.into_bytes())), None);
        let done: Value = serde_json::from_str(&f.list_done(5).unwrap()).unwrap();
        assert_eq!(
            (done["worlds"].as_u64(), done["from"].as_str()),
            (Some(1), Some(HUB_LIST_URL))
        );
        assert_eq!(f.worlds(NOW)["worlds"][0]["slug"], "unfamiliar-shores");
        assert_eq!(f.worlds(NOW)["worlds"][0]["logon_version"], "c118");
        let mut again = Front::default();
        assert_eq!(
            again.set_copy(&done["copy"].to_string(), None, None),
            Ok(1),
            "the copy reads again"
        );

        assert_eq!(f.list_start(None, None), Some(hub.clone()));
        assert_eq!(
            f.list_next(Err("Failed to fetch".into())).as_deref(),
            Some(COMMUNITY_LIST_URL)
        );
        assert_eq!(f.list_next(Ok(LIST.as_bytes().to_vec())), None);
        let done: Value = serde_json::from_str(&f.list_done(6).unwrap()).unwrap();
        assert_eq!(
            (done["worlds"].as_u64(), done["from"].as_str()),
            (Some(2), Some(COMMUNITY_LIST_URL))
        );

        // A hub that shows no world is passed over too.
        f.list_start(None, None);
        let empty = json!({"data": [], "total": 0, "nextOffset": null}).to_string();
        assert_eq!(
            f.list_next(Ok(empty.into_bytes())).as_deref(),
            Some(COMMUNITY_LIST_URL)
        );
        f.list_next(Ok(LIST.as_bytes().to_vec()));
        assert_eq!(f.list_done(7).map(|_| ()), Ok(()));

        f.list_start(None, None);
        f.list_next(Err("HTTP 502".into()));
        assert_eq!(f.list_next(Ok(b"<html/>".to_vec())), None);
        let e = f.list_done(7).unwrap_err();
        assert!(
            e.contains(&format!("{hub}: HTTP 502")) && e.contains(COMMUNITY_LIST_URL),
            "{e}"
        );
        assert_eq!(
            f.worlds(NOW)["worlds"].as_array().unwrap().len(),
            2,
            "the last list stands"
        );

        let named = "https://lists.example/Servers.xml";
        assert_eq!(f.list_start(Some(named), None).as_deref(), Some(named));
        assert_eq!(
            f.list_next(Err("HTTP 404".into())),
            None,
            "nothing else is fetched"
        );
        assert!(f.list_done(8).unwrap_err().starts_with(named));
    }

    /// The page shows the same worlds as the desktop launcher's window from the same answers: the
    /// hub's list, with how many are on as the hub has it, or the community's own when the hub does
    /// not answer.
    #[test]
    fn the_page_shows_the_worlds_every_launcher_shows_from_the_same_answers() {
        use dereth_launch::testing::world_list as fixture;
        let read = |f: &mut Front, hub_answers: bool| {
            let mut fetch = fixture::fetch(hub_answers);
            let mut url = f.list_start(None, None);
            while let Some(u) = url {
                url = f.list_next(fetch(&u));
            }
            f.list_done(fixture::READ_AT).unwrap();
            f.worlds(fixture::READ_AT)["worlds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| fixture::summary(&serde_json::from_value(w.clone()).unwrap()))
                .collect::<Vec<_>>()
        };
        let mut f = Front::default();
        assert_eq!(read(&mut f, true), fixture::from_the_hub());
        assert_eq!(read(&mut f, false), fixture::from_the_community());
    }

    /// The page reaches a world the hub names a WebSocket endpoint for directly, and a server the
    /// player added by that world's status document or WebSocket endpoint is one world with it:
    /// the list shows it once, and its page is the hub's world's.
    #[test]
    fn a_hub_world_with_a_websocket_is_reached_directly_and_shown_once() {
        use dereth_launch::testing::world_list as fixture;
        let mut f = Front::default();
        let mut fetch = fixture::fetch(true);
        let mut url = f.list_start(None, None);
        while let Some(u) = url {
            url = f.list_next(fetch(&u));
        }
        f.list_done(fixture::READ_AT).unwrap();
        let direct = |f: &Front| -> Vec<(String, Option<String>)> {
            f.worlds(fixture::READ_AT)["worlds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| {
                    (
                        w["slug"].as_str().unwrap().to_owned(),
                        w["websocket_url"].as_str().map(str::to_owned),
                    )
                })
                .collect()
        };
        let before = direct(&f);
        let added = f
            .add_world("", "ironwood.example.org", "9000", "", "unknown")
            .unwrap();
        f.added_by(&added, fixture::IRONWOOD_STATUS);
        assert_eq!(direct(&f), before, "one row for Ironwood");
        assert!(before.contains(&(
            "ironwood".to_owned(),
            Some(fixture::IRONWOOD_WEBSOCKET.to_owned())
        )));
        assert!(before.contains(&("frostfell".to_owned(), None)));
        let page = f.world(&added, fixture::READ_AT).expect("a page");
        assert_eq!(page["world"]["slug"], "ironwood");
        assert_eq!(page["world"]["websocket_url"], fixture::IRONWOOD_WEBSOCKET);
    }

    /// A row of the hub's list: the world `name`, its status `state`, a heartbeat's counting until
    /// `expires` when that is given.
    fn hub_row(name: &str, state: &str, players: Option<u32>, expires: Option<&str>) -> Value {
        let status = match expires {
            Some(at) => json!({"state": state, "playersOnline": players,
                               "source": "server-heartbeat", "expiresAt": at}),
            None => json!({"state": state, "playersOnline": null, "expiresAt": null}),
        };
        json!({"slug": serverlist::slug_for(name), "name": name, "isDemo": false,
               "emulator": {"family": "ACE"},
               "endpoint": {"mode": "manual", "address": "192.0.2.9", "port": 9000},
               "status": status})
    }

    /// A page of the hub's list holding `rows`, the next starting at `next`, as fetched.
    fn hub_page(rows: &[Value], next: Option<u64>) -> Result<Vec<u8>, String> {
        Ok(json!({"data": rows, "nextOffset": next})
            .to_string()
            .into_bytes())
    }

    /// The state and players the page shows for the world `slug` at `now`.
    fn seen(f: &Front, slug: &str, now: u64) -> (String, Value) {
        let w = f.world(slug, now).unwrap()["world"].clone();
        (
            w["state"].as_str().unwrap().to_owned(),
            w["players"].clone(),
        )
    }

    /// The page shows the hub's word on each world as the desktop launcher does: from the hub's
    /// list just fetched, then read again about once a minute a page at a time, kept while a read
    /// fails, never past the heartbeat's expiry, never over the world's own answer, and never
    /// from the day's copy, the community's list or a list the page names.
    /// The page offers both games' realms: of the hub's list it shows every world, each naming
    /// its game, and a world that names none is Asheron's Call's.
    #[test]
    fn the_page_shows_both_games_worlds_each_naming_its_game() {
        let mut f = Front::default();
        f.list_start(None, None).unwrap();
        let mut other = hub_row("Shardfall", "online", None, None);
        other["game"] = json!("auberean");
        let mut named = hub_row("Frostfell", "online", None, None);
        named["game"] = json!("dereth");
        assert_eq!(
            f.list_next(hub_page(
                &[hub_row("Coldeve", "online", None, None), other, named],
                None
            )),
            None
        );
        f.list_done(NOW).unwrap();
        let shown: Vec<(String, String)> = f.worlds(NOW)["worlds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| {
                (
                    w["slug"].as_str().unwrap().to_owned(),
                    w["game"].as_str().unwrap_or("dereth").to_owned(),
                )
            })
            .collect();
        let of = |slug: &str| {
            shown
                .iter()
                .find(|(s, _)| s == slug)
                .map(|(_, g)| g.as_str())
        };
        assert_eq!(shown.len(), 3, "{shown:?}");
        assert_eq!(of("coldeve"), Some("dereth"));
        assert_eq!(of("frostfell"), Some("dereth"));
        assert_eq!(of("shardfall"), Some("auberean"));
        assert!(f.world("coldeve", NOW).is_some());
        // A world of Asheron's Call 2's plays its realm's own sets, and its client is told nothing
        // about it.
        let page = f.world("shardfall", NOW).unwrap();
        assert_eq!(page["world"]["name"], "Shardfall");
        assert_eq!(page["args"], json!([]));
        assert_eq!(page["dat_sets"], json!([]));
        assert_eq!(page["overlay_folder"], json!(null));
    }

    /// A world added to Auberean's realm is that game's, a start on it is recorded for its client,
    /// and an account's combination plays there again.
    #[test]
    fn a_world_added_for_auberean_is_that_game_s_and_a_start_on_it_is_auberean_s() {
        let mut f = Front::default();
        let slug = f
            .add_world_for("auberean", "Mine", "127.0.0.1", "9200", "", "yalain")
            .unwrap();
        assert_eq!(f.world(&slug, NOW).unwrap()["world"]["game"], "auberean");
        f.record_launch(&slug, r#"{"account":"Tester","dat_set_id":"a1"}"#, NOW)
            .unwrap();
        let state: Value = serde_json::from_str(&f.state_json()).unwrap();
        assert_eq!(state["recent"][0]["client"], "auberean");
        assert_eq!(state["recent"][0]["dat_set_id"], "a1");
        // A world of the other game is Dereth's, as ever.
        let other = f
            .add_world("", "ace.example.org", "9000", "", "ace")
            .unwrap();
        f.record_launch(&other, r#"{"account":"Tester"}"#, NOW)
            .unwrap();
        let state: Value = serde_json::from_str(&f.state_json()).unwrap();
        assert_eq!(state["recent"][0]["client"], "dereth");
    }

    /// The realm's data files are kept in its own library, the first its default, and a file that
    /// is not what its name says shows in the check beside Play.
    #[test]
    fn asheron_s_call_2_s_files_make_a_set_of_its_realm_and_a_wrong_file_fails_its_check() {
        let mut f = Front::default();
        let check = f.auberean_check();
        assert_eq!(check["ok"], false, "no set is no check");
        let wrong = |name: &str| -> (String, u64, Box<dyn dereth_dat::DatStorage>) {
            (name.to_owned(), 1024, Box::new(vec![0u8; 1024]))
        };
        let ids = f.add_auberean_sets(
            "auberean/sets/files",
            vec![
                wrong("portal.dat"),
                wrong("cell_1.dat"),
                wrong("local_English.dat"),
                wrong("not-a-set-file.dat"),
            ],
        );
        assert_eq!(ids.len(), 1);
        let state: Value = serde_json::from_str(&f.state_json()).unwrap();
        let set = &state["auberean"]["dat_sets"][0];
        assert_eq!(set["origin"]["kind"], "shared", "{set}");
        assert_eq!(set["files"].as_array().unwrap().len(), 3);
        assert!(state["dat_sets"].as_array().unwrap().is_empty());
        let check = f.auberean_check();
        assert_eq!(check["ok"], false);
        assert!(
            check["summary"].as_str().unwrap().contains("portal.dat"),
            "{check}"
        );
        f.remove_sets("auberean/sets/files");
        let state: Value = serde_json::from_str(&f.state_json()).unwrap();
        assert!(state["auberean"]["dat_sets"].is_null());
    }

    /// The realm the page shows is kept with the launcher's state, which the page opens in next.
    #[test]
    fn the_realm_the_page_shows_is_kept() {
        let mut f = Front::default();
        f.set_realm("auberean");
        let state: Value = serde_json::from_str(&f.state_json()).unwrap();
        assert_eq!(state["settings"]["realm"], "auberean");
        f.set_realm("dereth");
        let state: Value = serde_json::from_str(&f.state_json()).unwrap();
        assert!(state["settings"]["realm"].is_null());
    }

    /// A world starred in the page's world list is a favorite, which the list shows first; a Play
    /// favorite does not make its world one, and each is removed on its own.
    #[test]
    fn a_world_starred_on_the_page_is_a_favorite_apart_from_the_play_favorites() {
        const IN_10_MIN: &str = "2026-10-11T18:10:00.000Z";
        let mut f = Front::default();
        f.list_start(None, None).unwrap();
        f.list_next(hub_page(
            &[
                hub_row("Coldeve", "online", Some(12), Some(IN_10_MIN)),
                hub_row("Seedsow", "online", Some(3), Some(IN_10_MIN)),
                hub_row("AChard", "online", Some(1), Some(IN_10_MIN)),
            ],
            None,
        ));
        f.list_done(NOW).unwrap();
        let favorites = |f: &Front| -> Vec<String> {
            f.worlds(NOW)["worlds"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|w| w["favorite"] == true)
                .map(|w| w["slug"].as_str().unwrap().to_owned())
                .collect()
        };
        assert!(favorites(&f).is_empty());
        f.set_world_favorite("seedsow", true);
        let pinned = json!({"id": "", "world_slug": "achard", "account": "player",
                            "client": "dereth", "dat_set_id": "web-1"})
        .to_string();
        f.save_favourite(&pinned).unwrap();
        assert_eq!(
            favorites(&f),
            ["seedsow"],
            "a Play favorite is not a starred world"
        );
        f.set_world_favorite("seedsow", false);
        assert!(favorites(&f).is_empty());
        assert!(
            kept_of(&f).iter().any(|(w, _, fav)| w == "achard" && *fav),
            "the Play favorite is left alone"
        );
    }

    /// The page shows the era, systems and version an Empyrean world told the hub, as the desktop
    /// launcher shows what the world tells it over UDP; an Empyrean world's era nobody has said as
    /// unknown once the hub has said all it will, never as the end of retail; and a world whose
    /// software nobody names as taken to play the end of retail.
    #[test]
    fn the_page_shows_what_a_world_told_the_hub_and_an_era_nobody_said_as_unknown() {
        const IN_3_MIN: &str = "2026-10-11T18:03:00.000Z";
        const IN_10_MIN: &str = "2026-10-11T18:10:00.000Z";
        let empyrean = |name: &str, said: Value| {
            let mut row = hub_row(name, "online", Some(0), Some(IN_10_MIN));
            row["emulator"]["family"] = json!("Empyrean");
            row["statusPing"] = said;
            row
        };
        let said = json!({"state": "open", "playersOnline": 1, "era": "infiltration",
            "eraFeatures": "1:0000ff", "software": "Empyrean", "version": "0.3.7",
            "name": "Infiltration", "askedAt": "2026-10-11T18:00:00.000Z", "expiresAt": IN_3_MIN});
        let mut f = Front::default();
        f.list_start(None, None).unwrap();
        f.list_next(hub_page(
            &[
                empyrean("Infiltration", said),
                empyrean("Retail", Value::Null),
                hub_row("Coldeve", "online", Some(12), Some(IN_10_MIN)),
            ],
            None,
        ));
        f.list_done(NOW).unwrap();
        let fact = |f: &Front, slug: &str, now: u64| {
            let w = f.world(slug, now).unwrap()["world"].clone();
            (
                w["era"].as_str().map(str::to_owned),
                w["emulator_version"].as_str().map(str::to_owned),
                w["era_state"].as_str().unwrap().to_owned(),
            )
        };
        assert_eq!(
            fact(&f, "infiltration", NOW),
            (
                Some("infiltration".into()),
                Some("0.3.7".into()),
                "known".into()
            )
        );
        let w = &f.world("infiltration", NOW).unwrap()["world"];
        assert_eq!(
            (w["era_source"].as_str(), w["features_source"].as_str()),
            (Some("world"), Some("world")),
            "the systems it has, as it said them"
        );
        assert_eq!(w["players"], 1);
        assert_eq!(
            fact(&f, "retail", NOW),
            (None, None, "unknown".into()),
            "not heard from, and the hub has said all it will"
        );
        assert_eq!(
            fact(&f, "coldeve", NOW),
            (None, None, "known".into()),
            "ACE plays the end of retail"
        );
        let listed: Vec<String> = f.worlds(NOW)["worlds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["era_state"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(listed, ["known", "unknown", "known"]);
        // Past the answer's expiry, the era is unknown again.
        assert_eq!(
            fact(&f, "infiltration", NOW + 180),
            (None, None, "unknown".into())
        );
        // A status document that names no era says the world plays the end of retail.
        assert!(f.set_status("retail", br#"{"world_open":true,"version":"0.3.7"}"#));
        assert_eq!(
            fact(&f, "retail", NOW),
            (None, Some("0.3.7".into()), "known".into())
        );
        // A server the player added, its software not named and not heard from: taken to play the
        // end of retail.
        let added = f
            .add_world("", "play.example.org", "9000", "", "unknown")
            .unwrap();
        assert_eq!(fact(&f, &added, NOW), (None, None, "assumed".into()));
        let added = f
            .add_world("", "ace.example.org", "9000", "", "ace")
            .unwrap();
        assert_eq!(fact(&f, &added, NOW), (None, None, "known".into()));
    }

    #[test]
    fn the_page_shows_the_hubs_word_on_each_world_by_the_desktop_launchers_rules() {
        use serverlist::{COMMUNITY_LIST_URL, HUB_LIST_URL};
        const IN_5_MIN: &str = "2026-10-11T18:05:00.000Z";
        const IN_10_MIN: &str = "2026-10-11T18:10:00.000Z";
        let mut f = Front::default();
        // The day's copy of the hub's list: what it said is not shown.
        let copy = json!({"fetched_at": NOW, "list": "", "hub": [String::from_utf8(
            hub_page(&[hub_row("Coldeve", "online", Some(99), Some(IN_10_MIN))], None).unwrap())
            .unwrap()], "sources": usual()});
        assert_eq!(f.set_copy(&copy.to_string(), None, None), Ok(1));
        assert_eq!(seen(&f, "coldeve", NOW), ("unknown".into(), Value::Null));

        // The hub's list fetched: an ACE world's state and players at once.
        let first = f.list_start(None, None).unwrap();
        assert_eq!(
            f.list_next(hub_page(
                &[
                    hub_row("Coldeve", "online", Some(12), Some(IN_5_MIN)),
                    hub_row("Frostfell", "unknown", None, None),
                ],
                None,
            )),
            None
        );
        f.list_done(NOW).unwrap();
        assert_eq!(seen(&f, "coldeve", NOW), ("online".into(), json!(12)));
        assert_eq!(seen(&f, "frostfell", NOW), ("unknown".into(), Value::Null));
        assert_eq!(f.worlds(NOW)["worlds"][0]["players"], 12);

        // Read again a minute after the last, a page at a time. A read the page lost on the way
        // ends as a failed one, and does not keep the next from starting.
        assert_eq!(
            f.hub_start(None, None, NOW + 59),
            None,
            "not within the minute"
        );
        assert_eq!(f.hub_start(None, None, NOW + 60), Some(first.clone()));
        assert_eq!(
            f.hub_start(None, None, NOW + 61),
            None,
            "a minute after the lost one"
        );
        assert_eq!(
            f.hub_next(hub_page(&[], None)),
            None,
            "nothing is being read"
        );
        assert_eq!(f.hub_start(None, None, NOW + 120), Some(first.clone()));
        assert_eq!(
            f.hub_next(hub_page(
                &[hub_row("Coldeve", "online", Some(14), Some(IN_5_MIN))],
                Some(100),
            ))
            .as_deref(),
            Some(format!("{HUB_LIST_URL}?game=all&limit=100&offset=100").as_str())
        );
        assert_eq!(
            f.hub_next(hub_page(
                &[hub_row(
                    "Frostfell",
                    "maintenance",
                    Some(0),
                    Some(IN_10_MIN)
                )],
                None,
            )),
            None
        );
        assert!(f.hub_done(NOW + 121));
        assert_eq!(seen(&f, "coldeve", NOW + 121), ("online".into(), json!(14)));
        assert_eq!(
            seen(&f, "frostfell", NOW + 121),
            ("starting".into(), json!(0))
        );
        // A status document that does not answer counts for less than the hub's word.
        f.status_failed("coldeve");
        assert_eq!(seen(&f, "coldeve", NOW + 121), ("online".into(), json!(14)));

        // A read that fails changes nothing; the hub alone is asked.
        assert_eq!(f.hub_start(None, None, NOW + 180), Some(first.clone()));
        assert_eq!(f.hub_next(Err("Failed to fetch".into())), None);
        assert!(!f.hub_done(NOW + 181));
        assert_eq!(seen(&f, "coldeve", NOW + 181), ("online".into(), json!(14)));

        // Past the heartbeat's expiry the hub's word is gone, and the world is left to what else
        // was heard of it.
        assert_eq!(seen(&f, "coldeve", NOW + 299).1, json!(14));
        assert_eq!(
            seen(&f, "coldeve", NOW + 300),
            ("offline".into(), Value::Null)
        );
        assert_eq!(
            seen(&f, "frostfell", NOW + 600),
            ("unknown".into(), Value::Null)
        );
        assert_eq!(seen(&f, "frostfell", NOW + 300).0, "starting");

        // The world's own answer goes before the hub's word.
        assert!(f.set_status("frostfell", br#"{"world_open":true,"players_online":3}"#));
        assert_eq!(
            seen(&f, "frostfell", NOW + 300),
            ("online".into(), json!(3))
        );

        // A list the page names is read alone: no status is read.
        assert_eq!(
            f.hub_start(Some("https://lists.example/Servers.xml"), None, NOW + 600),
            None
        );

        // The community's list, read when the hub does not answer, ends the hub's word.
        f.list_start(None, None);
        f.list_next(Err("HTTP 502".into()));
        f.list_next(Ok(LIST.as_bytes().to_vec()));
        f.list_done(NOW + 601).unwrap();
        assert_eq!(
            f.hub_start(None, None, NOW + 1_000),
            None,
            "not {COMMUNITY_LIST_URL}"
        );
        assert_eq!(seen(&f, "achard", NOW + 601).1, Value::Null);
    }

    fn file(name: &str, bytes: Vec<u8>) -> (String, u64, Box<dyn dereth_dat::DatStorage>) {
        let size = bytes.len() as u64;
        (name.to_owned(), size, Box::new(bytes))
    }

    /// The later set at the end of retail's iterations, and the older pair.
    fn end_of_retail_files() -> Vec<(String, u64, Box<dyn dereth_dat::DatStorage>)> {
        vec![
            file("client_portal.dat", testdat::eor(1, 0, 2072)),
            file("client_cell_1.dat", testdat::eor(2, 1, 982)),
            file("client_local_English.dat", testdat::eor(3, 1, 994)),
            file("portal.dat", testdat::pre_tod(false, 2112)),
            file("cell.dat", testdat::pre_tod(true, 2112)),
            file("notes.txt", b"not a dat".to_vec()),
        ]
    }

    #[test]
    fn the_list_and_the_table_read_in_the_page_as_in_the_desktop_launcher() {
        let mut f = Front::default();
        assert_eq!(f.set_copy(&copy_of(LIST), None, None), Ok(2));
        assert!(
            f.set_copy(&copy_of("<html/>"), None, None).is_err(),
            "not the list"
        );
        assert!(f.set_copy("not a copy", None, None).is_err());
        let all = f.worlds(NOW);
        let worlds = all["worlds"].as_array().unwrap();
        assert_eq!(worlds.len(), 2, "a list that does not read leaves the last");
        assert_eq!(worlds[0]["name"], "AChard");
        assert_eq!(
            worlds[0]["state"], "unknown",
            "no status, no probe: unknown"
        );
        assert_eq!(worlds[0]["address"], "a-chard.example:9000");
        // The table's world: its own client's logon, rules and files.
        let s = &worlds[1];
        assert_eq!(s["emulator"], "gdle");
        assert_eq!(s["era"], "infiltration");
        assert_eq!(s["logon_version"], "1802");
        assert_eq!(s["world_profile"], "classicdereth");
        assert!(s["dats"]["custom"]["license_note"]
            .as_str()
            .unwrap()
            .contains("ClassicDereth"));
        assert!(all["eras"].as_array().unwrap().len() >= 2);
        assert!(!all["features"].as_array().unwrap().is_empty());

        // What the client is told is the desktop's command line's tail.
        let page = f.world("seedsow", NOW).unwrap();
        let args: Vec<&str> = page["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect();
        assert_eq!(
            args[..4],
            ["--world-base", "modern", "--era", "infiltration"]
        );
        assert!(args.ends_with(&["--world-profile", "classicdereth"]));
        assert_eq!(page["overlay_folder"], "play.seedsow.example-9050");
        // And the client in the browser reads them as the desktop client reads its command line.
        let words: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        let mut cfg = dereth_client_runtime::config::Config::default();
        cfg.apply_args(&words).unwrap();
        assert_eq!(cfg.era, Some(dereth_primitives::EraId::Infiltration));
        assert_eq!(
            cfg.world_base,
            Some(dereth_primitives::ContainerEra::Modern)
        );
        assert_eq!(cfg.logon_version, "1802");
        assert_eq!(page["launch"]["set"], "modern");
        assert_eq!(page["launch"]["logon_version"], "1802");
        assert!(f.world("nowhere", NOW).is_none());
        // The table's world plays the table's era until it says its own; an era it names alone
        // has that era's whole table.
        assert!(f.set_status("seedsow", br#"{"world_open":true,"era":"eor"}"#));
        let s = f.world("seedsow", NOW).unwrap();
        assert_eq!(
            (&s["world"]["era"], &s["world"]["era_features"]),
            (&json!("eor"), &Value::Null)
        );
        assert!(!s["args"]
            .as_array()
            .unwrap()
            .contains(&json!("--era-features")));
    }

    #[test]
    fn a_world_added_by_address_and_its_status_and_choices_are_kept_in_the_state_record() {
        let mut f = Front::default();
        f.set_state("").unwrap();
        assert!(f.add_world("", "bad host", "9000", "", "").is_err());
        let slug = f
            .add_world("", "127.0.0.1", "19960", "PvE", "empyrean")
            .unwrap();
        let kept = f.state_json();
        let mut again = Front::default();
        again.set_state(&kept).unwrap();
        let w = &again.worlds(NOW)["worlds"][0];
        assert_eq!(
            (w["slug"].as_str(), w["added"].as_bool()),
            (Some(slug.as_str()), Some(true))
        );
        assert_eq!(
            (&w["era"], &w["era_source"]),
            (&Value::Null, &Value::Null),
            "nothing said yet: the end of retail"
        );
        assert!(again.world(&slug, NOW).unwrap()["args"]
            .as_array()
            .unwrap()
            .is_empty());
        // Its status: the world's own word wins, and it is shown as up.
        let doc = br#"{"world_name":"Infiltration","world_open":true,"players_online":2,"era":"infiltration","features":{"trade":false},"dats":{"portal":2112,"patching":true}}"#;
        assert!(again.set_status(&slug, doc));
        let w = &again.worlds(NOW)["worlds"][0];
        assert_eq!(w["name"], "Infiltration");
        assert_eq!(
            (
                w["state"].as_str(),
                w["era"].as_str(),
                w["era_source"].as_str()
            ),
            (Some("online"), Some("infiltration"), Some("world"))
        );
        assert_eq!(w["dats"]["patches_over_wire"], true);
        assert_eq!(
            again.world(&slug, NOW).unwrap()["args"].as_array().unwrap()[..2],
            [json!("--era"), json!("infiltration")]
        );
        again.status_failed(&slug);
        assert_eq!(again.worlds(NOW)["worlds"][0]["state"], "offline");
        again
            .record_launch(&slug, r#"{"account":"player","dat_set_id":"web-1"}"#, NOW)
            .unwrap();
        assert_eq!(
            again.world(&slug, NOW).unwrap()["prefs"]["account"],
            "player"
        );
        again.remove_world(&slug);
        assert!(again.worlds(NOW)["worlds"].as_array().unwrap().is_empty());
        assert!(
            again.state.accounts.is_empty() && again.state.recent.is_empty(),
            "its account and its start go with it"
        );
    }

    /// The (world, account, favourite) of each favourite, then of each recent combination, as kept.
    fn kept_of(f: &Front) -> Vec<(String, String, bool)> {
        let favourites = f.state.favourites.iter();
        let recent = f.state.recent.iter();
        favourites
            .map(|e| (e.world_slug.clone(), e.account.clone(), true))
            .chain(recent.map(|e| (e.world_slug.clone(), e.account.clone(), false)))
            .collect()
    }

    /// A start is recorded as the desktop launcher records one (the account, the world's choices,
    /// the combination as the most recent, each once whatever the account name's case), with no
    /// password kept; and pinning and forgetting change what's kept as the desktop's do.
    #[test]
    fn a_start_is_recorded_as_the_desktop_launcher_records_one_and_pins_and_forgets_as_it_does() {
        let mut f = Front::default();
        f.set_copy(&copy_of(LIST), None, None).unwrap();
        assert!(f.record_launch("achard", "not prefs", NOW).is_err());
        f.record_launch(
            "achard",
            r#"{"account":"player","dat_set_id":"web-1"}"#,
            NOW,
        )
        .unwrap();
        f.record_launch(
            "achard",
            r#"{"account":"Player ","dat_set_id":"web-1"}"#,
            NOW + 1,
        )
        .unwrap();
        let st: Value = serde_json::from_str(&f.state_json()).unwrap();
        assert_eq!(st["accounts"].as_array().unwrap().len(), 1);
        assert_eq!(st["accounts"][0]["remember"], false, "no password is kept");
        assert_eq!(st["recent"].as_array().unwrap().len(), 1);
        assert_eq!(st["recent"][0]["client"], "dereth");
        assert_eq!(st["world_prefs"]["achard"]["last_played"], NOW + 1);
        assert_eq!(
            f.world("achard", NOW).unwrap()["accounts"][0]["username"],
            "Player"
        );
        f.record_launch("seedsow", r#"{"account":"alt"}"#, NOW + 2)
            .unwrap();
        // A start on a world the list no longer shows is recorded too; the page decides what it offers.
        f.record_launch("nowhere", r#"{"account":"ghost"}"#, NOW + 3)
            .unwrap();
        let recent = [
            ("nowhere".into(), "ghost".into(), false),
            ("seedsow".into(), "alt".into(), false),
            ("achard".into(), "Player".into(), false),
        ];
        assert_eq!(kept_of(&f), recent);

        // Pinned, the combination is a favourite, kept apart from the recent ones.
        let pinned = json!({"id": "", "world_slug": "achard", "account": "player",
                            "client": "dereth", "dat_set_id": "web-1"})
        .to_string();
        let id = f.save_favourite(&pinned).unwrap();
        assert!(!id.is_empty());
        assert_eq!(kept_of(&f)[0], ("achard".into(), "player".into(), true));
        assert_eq!(kept_of(&f)[1..], recent);
        f.remove_favourite(&id);
        assert_eq!(kept_of(&f), recent);

        // Forgetting an account forgets its combinations; forgetting all forgets every one.
        f.save_favourite(&pinned).unwrap();
        f.forget_account("achard", "PLAYER");
        assert_eq!(kept_of(&f), recent[..2]);
        assert!(f.world("achard", NOW).unwrap()["prefs"]["account"].is_null());
        f.forget_all_accounts();
        assert!(kept_of(&f).is_empty());
        assert!(f.state.accounts.is_empty());
    }

    /// A world's page carries its join link, the site's page with the world's game address and
    /// name, as the desktop launcher builds it; a server added by a URL has none, its port being
    /// the URL's.
    #[test]
    fn a_worlds_join_link_names_its_game_address_and_a_server_added_by_url_has_none() {
        let mut f = Front::default();
        f.set_copy(&copy_of(LIST), None, None).unwrap();
        assert_eq!(
            f.world("achard", NOW).unwrap()["join_link"],
            "https://dereth.network/join#host=a-chard.example&port=9000&name=AChard"
        );
        let typed = f.add_world("Mine", "127.0.0.1", "9050", "", "").unwrap();
        assert_eq!(
            f.world(&typed, NOW).unwrap()["join_link"],
            "https://dereth.network/join#host=127.0.0.1&port=9050&name=Mine"
        );
        let by_url = f
            .add_world("", "play.example.org", "443", "", "empyrean")
            .unwrap();
        f.added_by(&by_url, "wss://play.example.org/");
        assert!(f.world(&by_url, NOW).unwrap()["join_link"].is_null());
    }

    /// The first set of each kind kept in the browser is that kind's default, as the desktop
    /// library makes it; another can be made the default in its place.
    #[test]
    fn the_first_set_of_a_kind_is_its_default_and_another_can_take_its_place() {
        let mut f = Front::default();
        let first = f.add_sets("sets/eor", end_of_retail_files());
        let second = f.add_sets(
            "sets/copy",
            vec![file("client_portal.dat", testdat::eor(1, 0, 2072))],
        );
        let origin = |f: &Front, id: &str| {
            let st: Value = serde_json::from_str(&f.state_json()).unwrap();
            st["dat_sets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["id"] == id)
                .unwrap()["origin"]["kind"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        assert_eq!(origin(&f, &first[0]), "shared");
        assert_eq!(
            origin(&f, &first[1]),
            "shared",
            "the older pair's own default"
        );
        assert_eq!(origin(&f, &second[0]), "unassigned");
        f.set_default_set(&second[0]).unwrap();
        assert_eq!(origin(&f, &second[0]), "shared");
        assert_eq!(origin(&f, &first[0]), "unassigned");
        assert_eq!(origin(&f, &first[1]), "shared", "the other kind's stays");
        assert!(f.set_default_set("nothing").is_err());
    }

    #[test]
    fn sets_kept_in_the_browser_are_matched_to_a_world_by_their_iterations() {
        let mut f = Front::default();
        f.set_copy(&copy_of(LIST), None, None).unwrap();
        let ids = f.add_sets("sets/eor", end_of_retail_files());
        assert_eq!(
            ids.len(),
            2,
            "the later set and the older pair, as two sets"
        );
        let state: Value = serde_json::from_str(&f.state_json()).unwrap();
        let sets = state["dat_sets"].as_array().unwrap();
        assert_eq!(sets[0]["kind"], "modern");
        assert_eq!(
            sets[0]["files"].as_array().unwrap().len(),
            3,
            "the text file is no dat"
        );
        assert_eq!(sets[1]["kind"], "classic");
        assert_eq!(sets[1]["files"][0]["iterations"], 2112);

        // An end-of-retail world takes the set; a world with files of its own does not, and says
        // what its files are and where they come from.
        let plain = f.world("achard", NOW).unwrap();
        assert_eq!(plain["dat_sets"][0]["id"], ids[0]);
        assert_eq!(plain["classic_sets"][0]["id"], ids[1]);
        let own = f.world("seedsow", NOW).unwrap();
        assert!(own["dat_sets"].as_array().unwrap().is_empty());
        let why = own["refused_sets"][0]["why"].as_str().unwrap();
        assert!(
            why.contains("2072/982/994/-") && why.contains("2072/4/-/-"),
            "{why}"
        );

        // The world's own files, added beside: they are its set.
        let ids2 = f.add_sets(
            "sets/seedsow",
            vec![
                file("client_portal.dat", testdat::eor(1, 0, 2072)),
                file("client_cell_1.dat", testdat::eor(2, 1, 4)),
                file("client_local_English.dat", testdat::eor(3, 1, 994)),
            ],
        );
        let own = f.world("seedsow", NOW).unwrap();
        assert_eq!(own["dat_sets"][0]["id"], ids2[0]);
        // Read again in place, and forgotten.
        assert_eq!(
            f.add_sets(
                "sets/seedsow",
                vec![file("client_cell_1.dat", testdat::eor(2, 1, 4))]
            ),
            ids2
        );
        f.remove_sets("sets/seedsow");
        assert!(f.world("seedsow", NOW).unwrap()["dat_sets"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    /// The page's `hub` parameter names another hub's list, read in place of the network's, for
    /// the worlds and for their status; a copy of the network's list is not shown on it, nor a copy
    /// of its list without it, nor a copy kept before copies recorded their sources.
    #[test]
    fn the_hub_parameter_reads_another_hubs_list_and_a_copy_of_another_list_is_not_shown() {
        use serverlist::{COMMUNITY_LIST_URL, HUB_LIST_URL};
        const DEV: &str = "https://dev-hub.example.org/v1/servers";
        let page = json!({"data": [{"slug": "devworld", "name": "Dev World", "isDemo": false,
            "emulator": {"family": "ACE"},
            "endpoint": {"mode": "manual", "address": "192.0.2.9", "port": 9000}}],
            "nextOffset": null})
        .to_string();
        let mut f = Front::default();
        assert!(
            f.set_copy(&copy_of(LIST), None, Some(DEV)).is_err(),
            "the network's copy is not the dev hub's list"
        );
        assert!(f.list_wanted(1), "so the list is read");
        let first = f.list_start(None, Some(DEV)).unwrap();
        assert!(first.starts_with(DEV), "{first}");
        assert_eq!(f.list_next(Ok(page.into_bytes())), None);
        let done: Value = serde_json::from_str(&f.list_done(NOW).unwrap()).unwrap();
        assert_eq!(done["from"], DEV);
        assert_eq!(done["copy"]["sources"], json!([DEV, COMMUNITY_LIST_URL]));
        assert_eq!(f.worlds(NOW)["worlds"][0]["slug"], "dev-world");
        // Its status is read from the same hub.
        let status = f.hub_start(None, Some(DEV), NOW + 60).unwrap();
        assert!(status.starts_with(DEV), "{status}");

        // Without the parameter, the dev hub's copy is not the list, and the network's is read.
        let mut later = Front::default();
        assert!(later
            .set_copy(&done["copy"].to_string(), None, None)
            .is_err());
        assert!(later.list_wanted(NOW));
        assert!(later
            .list_start(None, None)
            .unwrap()
            .starts_with(HUB_LIST_URL));
        // With it, the dev hub's copy stands in for its list.
        let mut again = Front::default();
        assert_eq!(
            again.set_copy(&done["copy"].to_string(), None, Some(DEV)),
            Ok(1)
        );
        assert!(!again.list_wanted(NOW + 60));
        // A copy the page kept before copies recorded their sources is read again.
        let older = json!({"fetched_at": NOW, "list": LIST}).to_string();
        assert!(Front::default().set_copy(&older, None, None).is_err());
    }

    #[test]
    fn the_lists_copy_is_the_days_and_an_overlay_is_kept_by_the_servers_address() {
        let mut f = Front::default();
        assert!(f.list_wanted(1_000), "no copy");
        f.set_copy(
            &json!({"fetched_at": 1_000, "list": LIST, "sources": usual()}).to_string(),
            None,
            None,
        )
        .unwrap();
        assert!(!f.list_wanted(1_000 + 3_600));
        assert!(f.list_wanted(1_000 + 86_400));
        assert!(f.set_copy(&copy_of("<html/>"), None, None).is_err());
        assert!(!f.list_wanted(1_000 + 3_600), "the copy shown stands");
        assert_eq!(overlay_folder("127.0.0.1:19960"), "127.0.0.1-19960");
        assert_eq!(overlay_folder("play.example.org"), "play.example.org-9000");
    }
}
