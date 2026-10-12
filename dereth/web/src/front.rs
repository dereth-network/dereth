//! The front page's launcher: the worlds and what each says, what the player chose and added, the
//! data sets the player keeps in this browser and which of them a world takes, and what the client
//! is told about the world. All of it is the desktop launcher's own logic (`dereth-launch`): the
//! world list read from its sources in their order (the page fetches each address it is given),
//! the table of worlds that run a client of their own, a world shown as its window shows it, its
//! sets matched by iterations as its library matches them, and the client told about the world in
//! the words its command line carries.
//!
//! The page keeps the launcher's state record between visits (as JSON in the browser's storage),
//! and the day's copy of the list; this holds them while the page is open and answers the page's
//! questions as JSON. A browser cannot ask a server over UDP whether it is up, so a world with no
//! status document is shown with its state unknown.

use std::collections::BTreeMap;
use std::path::PathBuf;

use dereth_launch::choices;
use dereth_launch::datset::{self, DatOrigin, DatSet, SetKind};
use dereth_launch::eras;
use dereth_launch::launch;
use dereth_launch::serverlist::{self, ListCache, ListFetch};
use dereth_launch::state::{LauncherState, WorldPrefs};
use dereth_launch::status::{self, LiveStatus};
use dereth_launch::world::{Emulator, World};
use dereth_launch::worlds;
use serde_json::{json, Value};

/// The launcher behind the front page.
#[derive(Debug, Default)]
pub struct Front {
    list: Vec<World>,
    state: LauncherState,
    live: BTreeMap<String, LiveStatus>,
    /// Worlds whose status document did not answer: shown as down.
    unanswered: BTreeMap<String, bool>,
    /// The list being read, while the page fetches it.
    fetch: Option<ListFetch>,
}

/// Whether a copy of the list fetched at `fetched_at` (seconds since the epoch) is still the
/// day's at `now`: the desktop launcher's rule.
#[must_use]
pub fn list_is_fresh(fetched_at: u64, now: u64) -> bool {
    ListCache {
        fetched_at,
        ..ListCache::default()
    }
    .is_fresh(now)
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
    /// JSON). Answers how many worlds it lists.
    ///
    /// # Errors
    /// The copy does not read; the worlds read before stand.
    pub fn set_copy(&mut self, copy: &str) -> Result<usize, String> {
        let copy: ListCache = serde_json::from_str(copy).map_err(|e| e.to_string())?;
        self.list = copy.worlds().map_err(|e| e.to_string())?;
        Ok(self.list.len())
    }

    /// Start reading the world list from its sources: the `Servers.xml` `named` alone (the page's
    /// `list` parameter) when one is named, and otherwise the hub's list, then the community's own.
    /// Answers the first address to fetch.
    pub fn list_start(&mut self, named: Option<&str>) -> Option<String> {
        let fetch = ListFetch::new(serverlist::sources(named));
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

    /// The list read: it becomes the world list. Answers how many worlds it lists, where it came
    /// from and the day's copy (fetched at `now`), for the page to keep, as JSON.
    ///
    /// # Errors
    /// Each source's failure when none answered with its list; the worlds read before stand.
    pub fn list_done(&mut self, now: u64) -> Result<String, String> {
        let fetch = self
            .fetch
            .take()
            .ok_or("the world list is not being read")?;
        let from = fetch.source().map(|s| s.url.clone());
        let (copy, worlds) = fetch.finish(now)?;
        self.list = worlds;
        Ok(json!({"worlds": self.list.len(), "from": from, "copy": copy}).to_string())
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

    fn shown(&self) -> impl Iterator<Item = World> + '_ {
        worlds::all(&self.list, &self.state).map(|w| {
            let probed = self.unanswered.get(&w.slug).copied();
            let live = self.live.get(&w.slug);
            worlds::shown(w, probed, live, &self.state)
        })
    }

    fn world_of(&self, slug: &str) -> Option<World> {
        self.shown().find(|w| w.slug == slug)
    }

    /// Every world, with the eras and systems the page offers for those that do not say theirs.
    #[must_use]
    pub fn worlds(&self) -> Value {
        let custom: Vec<&str> = self
            .state
            .custom_worlds
            .iter()
            .map(|c| c.slug.as_str())
            .collect();
        let worlds: Vec<Value> = self
            .shown()
            .map(|w| {
                let mut v = serde_json::to_value(&w).unwrap_or(Value::Null);
                v["added"] = json!(custom.contains(&w.slug.as_str()));
                v["address"] = json!(address(&w));
                v
            })
            .collect();
        json!({
            "worlds": worlds,
            "eras": eras::eras(),
            "features": eras::features(),
        })
    }

    /// One world's page: the world, the data sets it takes (and why each other one does not do),
    /// the set its world is drawn from, what the client is told about it, the player's last
    /// choices there, and the folder its overlay is kept in. `None` for a world not shown.
    #[must_use]
    pub fn world(&self, slug: &str) -> Option<Value> {
        let w = self.world_of(slug)?;
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
            "world": w,
            "dat_sets": taken,
            "refused_sets": refused,
            "classic_sets": choices::classic_sets_for(&self.state),
            "launch": launch::describe(&w),
            "args": launch::describe(&w).args(),
            "prefs": self.state.world_prefs.get(slug),
            "address": address(&w),
            "overlay_folder": address(&w).map(|a| overlay_folder(&a)),
        }))
    }

    /// Add a server by its host and port.
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
        let ruleset = Some(ruleset).filter(|r| !r.trim().is_empty());
        self.state
            .add_custom_world(name, host, port, ruleset, Emulator::parse(emulator))
            .map_err(|e| e.to_string())
    }

    /// Take a server the player added off the list, with what was kept for it.
    pub fn remove_world(&mut self, slug: &str) {
        self.state.remove_custom_world(slug);
        self.live.remove(slug);
        self.unanswered.remove(slug);
    }

    /// The era the player chose for a world that does not say its own.
    pub fn set_era(&mut self, slug: &str, era: &str) {
        self.state
            .set_world_era(slug, Some(era).filter(|e| !e.is_empty()));
    }

    /// Turn one of a world's systems on or off. Answers whether `name` is a system's.
    pub fn set_feature(&mut self, slug: &str, name: &str, on: bool) -> bool {
        self.state.set_world_feature(slug, name, on)
    }

    /// Remember the player's choices on a world: the account and the data sets.
    ///
    /// # Errors
    /// Choices that do not read.
    pub fn remember(&mut self, slug: &str, prefs: &str) -> Result<(), String> {
        let prefs: WorldPrefs = serde_json::from_str(prefs).map_err(|e| e.to_string())?;
        self.state.remember_launch(slug, prefs);
        Ok(())
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
                        match kind {
                            SetKind::Modern => modern.push(f),
                            SetKind::Classic => classic.push(f),
                        }
                    }
                    break;
                }
            }
        }
        let mut ids = Vec::new();
        for (kind, files) in [(SetKind::Modern, modern), (SetKind::Classic, classic)] {
            let existing = self
                .state
                .dat_sets
                .iter()
                .position(|s| s.path == path && s.kind == kind);
            if files.is_empty() {
                if let Some(i) = existing {
                    self.state.dat_sets.remove(i);
                }
                continue;
            }
            match existing {
                Some(i) => {
                    self.state.dat_sets[i].files = files;
                    ids.push(self.state.dat_sets[i].id.clone());
                }
                None => {
                    let id = self.state.new_id("web-");
                    self.state.dat_sets.push(DatSet {
                        id: id.clone(),
                        path: path.clone(),
                        kind,
                        origin: DatOrigin::Unassigned,
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

    /// Forget every set kept in the browser's folder `folder`.
    pub fn remove_sets(&mut self, folder: &str) {
        let path = PathBuf::from(folder);
        self.state.dat_sets.retain(|s| s.path != path);
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

    /// The day's copy of `xml`, as the page has always kept a `Servers.xml` (and kept every copy
    /// before the hub's list was read).
    fn copy_of(xml: &str) -> String {
        json!({"fetched_at": 1, "list": xml}).to_string()
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
        let hub = f.list_start(None).unwrap();
        assert!(hub.starts_with(HUB_LIST_URL), "{hub}");
        assert_eq!(f.list_next(Ok(page.into_bytes())), None);
        let done: Value = serde_json::from_str(&f.list_done(5).unwrap()).unwrap();
        assert_eq!(
            (done["worlds"].as_u64(), done["from"].as_str()),
            (Some(1), Some(HUB_LIST_URL))
        );
        assert_eq!(f.worlds()["worlds"][0]["slug"], "unfamiliar-shores");
        assert_eq!(f.worlds()["worlds"][0]["logon_version"], "c118");
        let mut again = Front::default();
        assert_eq!(
            again.set_copy(&done["copy"].to_string()),
            Ok(1),
            "the copy reads again"
        );

        assert_eq!(f.list_start(None), Some(hub.clone()));
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
        f.list_start(None);
        let empty = json!({"data": [], "total": 0, "nextOffset": null}).to_string();
        assert_eq!(
            f.list_next(Ok(empty.into_bytes())).as_deref(),
            Some(COMMUNITY_LIST_URL)
        );
        f.list_next(Ok(LIST.as_bytes().to_vec()));
        assert_eq!(f.list_done(7).map(|_| ()), Ok(()));

        f.list_start(None);
        f.list_next(Err("HTTP 502".into()));
        assert_eq!(f.list_next(Ok(b"<html/>".to_vec())), None);
        let e = f.list_done(7).unwrap_err();
        assert!(
            e.contains(&format!("{hub}: HTTP 502")) && e.contains(COMMUNITY_LIST_URL),
            "{e}"
        );
        assert_eq!(
            f.worlds()["worlds"].as_array().unwrap().len(),
            2,
            "the last list stands"
        );

        let named = "https://lists.example/Servers.xml";
        assert_eq!(f.list_start(Some(named)).as_deref(), Some(named));
        assert_eq!(
            f.list_next(Err("HTTP 404".into())),
            None,
            "nothing else is fetched"
        );
        assert!(f.list_done(8).unwrap_err().starts_with(named));
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
        assert_eq!(f.set_copy(&copy_of(LIST)), Ok(2));
        assert!(f.set_copy(&copy_of("<html/>")).is_err(), "not the list");
        assert!(f.set_copy("not a copy").is_err());
        let all = f.worlds();
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
        let page = f.world("seedsow").unwrap();
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
        let cfg = crate::play::world_config(&words, None).unwrap();
        assert_eq!(cfg.era, Some(dereth_primitives::EraId::Infiltration));
        assert_eq!(
            cfg.world_base,
            Some(dereth_primitives::ContainerEra::Modern)
        );
        assert_eq!(cfg.logon_version, "1802");
        assert_eq!(page["launch"]["set"], "modern");
        assert_eq!(page["launch"]["logon_version"], "1802");
        assert!(f.world("nowhere").is_none());
    }

    #[test]
    fn a_world_added_by_address_and_its_status_and_choices_are_kept_in_the_state_record() {
        let mut f = Front::default();
        f.set_state("").unwrap();
        assert!(f.add_world("", "bad host", "9000", "", "").is_err());
        let slug = f
            .add_world("", "127.0.0.1", "19960", "PvE", "empyrean")
            .unwrap();
        f.set_era(&slug, "infiltration");
        let kept = f.state_json();
        let mut again = Front::default();
        again.set_state(&kept).unwrap();
        let w = &again.worlds()["worlds"][0];
        assert_eq!(
            (w["slug"].as_str(), w["added"].as_bool()),
            (Some(slug.as_str()), Some(true))
        );
        assert_eq!(
            (w["era"].as_str(), w["era_source"].as_str()),
            (Some("infiltration"), Some("player"))
        );
        // Its status: the world's own word wins, and it is shown as up.
        let doc = br#"{"world_name":"Infiltration","world_open":true,"players_online":2,"era":"infiltration","features":{"trade":false},"dats":{"portal":2112,"patching":true}}"#;
        assert!(again.set_status(&slug, doc));
        let w = &again.worlds()["worlds"][0];
        assert_eq!(w["name"], "Infiltration");
        assert_eq!(
            (w["state"].as_str(), w["era_source"].as_str()),
            (Some("online"), Some("world"))
        );
        assert_eq!(w["dats"]["patches_over_wire"], true);
        again.status_failed(&slug);
        assert_eq!(again.worlds()["worlds"][0]["state"], "offline");
        again
            .remember(&slug, r#"{"account":"player","dat_set_id":"web-1"}"#)
            .unwrap();
        assert_eq!(again.world(&slug).unwrap()["prefs"]["account"], "player");
        again.remove_world(&slug);
        assert!(again.worlds()["worlds"].as_array().unwrap().is_empty());
    }

    #[test]
    fn sets_kept_in_the_browser_are_matched_to_a_world_by_their_iterations() {
        let mut f = Front::default();
        f.set_copy(&copy_of(LIST)).unwrap();
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
        let plain = f.world("achard").unwrap();
        assert_eq!(plain["dat_sets"][0]["id"], ids[0]);
        assert_eq!(plain["classic_sets"][0]["id"], ids[1]);
        let own = f.world("seedsow").unwrap();
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
        let own = f.world("seedsow").unwrap();
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
        assert!(f.world("seedsow").unwrap()["dat_sets"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn the_lists_copy_is_the_days_and_an_overlay_is_kept_by_the_servers_address() {
        assert!(list_is_fresh(1_000, 1_000 + 3_600));
        assert!(!list_is_fresh(1_000, 1_000 + 86_400));
        assert_eq!(overlay_folder("127.0.0.1:19960"), "127.0.0.1-19960");
        assert_eq!(overlay_folder("play.example.org"), "play.example.org-9000");
    }
}
