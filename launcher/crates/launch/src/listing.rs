//! The world list a launcher shows, kept and read alike in every launcher.
//!
//! At start the day's copy is shown, and the list is read from its sources ([`crate::serverlist`])
//! when there is no copy or it is not the day's; the player can have it read again. A list read
//! whole is shown and becomes the day's copy. When no source answers with its list, the list shown
//! before stands. A copy that does not read is no copy, and nor is one read from other sources (a
//! launcher started on another hub's list, or on a list the player names, never shows the copy of
//! the usual one, and the other way about). While the list shown is the hub's, the hub's word on
//! each world is read beside it ([`crate::hubstatus`]).
//!
//! The launcher's window and the web client's front page keep one each and fetch every address it
//! names their own way, so the two show the same worlds from the same answers. What each adds of
//! its own is how a world it cannot otherwise hear from is asked whether it is up: the window asks
//! a server over UDP, which a browser cannot.

use crate::hubstatus::HubWatch;
use crate::serverlist::{self, ListCache, ListFetch, ListSource, MalformedList};
use crate::state::LauncherState;
use crate::status::LiveStatus;
use crate::world::World;
use crate::worlds;

/// Sources' addresses as a person reads them: in order, or that none is recorded.
fn names(urls: &[String]) -> String {
    if urls.is_empty() {
        "sources it does not record".to_owned()
    } else {
        urls.join(", ")
    }
}

/// The world list a launcher shows, its day's copy, and the hub's word on its worlds.
#[derive(Debug, Clone)]
pub struct WorldList {
    sources: Vec<ListSource>,
    worlds: Vec<World>,
    copy: Option<ListCache>,
    hub: HubWatch,
    /// Whether this launcher asks each world how it is itself (the desktop's status ping); a
    /// browser cannot, and has only the hub's word.
    asks_worlds: bool,
}

impl Default for WorldList {
    /// The list read from the usual sources ([`serverlist::sources`]), with no copy yet.
    fn default() -> Self {
        Self::new(serverlist::sources(None), None)
    }
}

impl WorldList {
    /// The list read from `sources`, showing `copy` (the day's copy kept from before) until it is
    /// read again. A copy that does not read is no copy, and nor is one read from other sources.
    #[must_use]
    pub fn new(sources: Vec<ListSource>, copy: Option<ListCache>) -> Self {
        let mut list = Self {
            hub: HubWatch::new(&sources),
            sources,
            worlds: Vec::new(),
            copy: None,
            asks_worlds: false,
        };
        if let Some(copy) = copy {
            // A copy that does not read leaves the list empty, to be read.
            let _ = list.show_copy(copy);
        }
        list
    }

    /// This launcher asks each world how it is itself, and says so in `heard` ([`Self::shown`]).
    pub fn set_asks_worlds(&mut self, asks: bool) {
        self.asks_worlds = asks;
    }

    /// Where the list is read from, in the order the sources are tried.
    #[must_use]
    pub fn sources(&self) -> &[ListSource] {
        &self.sources
    }

    /// The list is read from `sources` from now on.
    pub fn set_sources(&mut self, sources: Vec<ListSource>) {
        self.hub.set_sources(&sources);
        self.sources = sources;
    }

    /// Show `copy`, the day's copy kept from before. Answers how many worlds it lists.
    ///
    /// # Errors
    /// The copy was read from other sources than the list's, or does not read; the list shown
    /// before stands.
    pub fn show_copy(&mut self, copy: ListCache) -> Result<usize, String> {
        if !copy.is_from(&self.sources) {
            return Err(format!(
                "the copy is of the list read from {}, not from {}",
                names(&copy.sources),
                names(&serverlist::source_urls(&self.sources))
            ));
        }
        self.worlds = copy.worlds().map_err(|e: MalformedList| e.to_string())?;
        self.hub.copy_shown(&copy);
        self.copy = Some(copy);
        Ok(self.worlds.len())
    }

    /// Whether the list is to be read at `now` (seconds since the epoch) when a launcher starts:
    /// there is no copy, it is not the day's, or it was read from other sources than the list's.
    #[must_use]
    pub fn wants_reading(&self, now: u64) -> bool {
        !self
            .copy
            .as_ref()
            .is_some_and(|c| c.is_fresh(now) && c.is_from(&self.sources))
    }

    /// A reading of the list from its sources, for the launcher to fetch an address at a time
    /// ([`ListFetch::next_url`], [`ListFetch::take`]) and hand back to [`Self::read`].
    #[must_use]
    pub fn reading(&self) -> ListFetch {
        ListFetch::new(self.sources.clone())
    }

    /// `fetch` read to its end at `now`: its list is shown and its copy is the day's, which the
    /// launcher keeps, and the hub's list brings each world's status as the hub has it now.
    /// Answers the copy.
    ///
    /// # Errors
    /// Each source's failure when none answered with its list; the list shown before stands.
    pub fn read(&mut self, fetch: ListFetch, now: u64) -> Result<&ListCache, String> {
        self.show_read(fetch.finish(now), now)
    }

    /// Read the list from its sources at `now`, fetching each address with `get`: its body, or why
    /// there is none. See [`Self::read`].
    ///
    /// # Errors
    /// Each source's failure when none answered with its list; the list shown before stands.
    pub fn read_with<F>(&mut self, now: u64, get: F) -> Result<&ListCache, String>
    where
        F: FnMut(&str) -> Result<Vec<u8>, String>,
    {
        let read = serverlist::fetch_first(&self.sources, now, get);
        self.show_read(read, now)
    }

    fn show_read(
        &mut self,
        read: Result<(ListCache, Vec<World>), String>,
        now: u64,
    ) -> Result<&ListCache, String> {
        let (copy, worlds) = read?;
        self.hub.list_read(&copy, now);
        self.worlds = worlds;
        Ok(self.copy.insert(copy))
    }

    /// The list's worlds, as their rows say.
    #[must_use]
    pub fn worlds(&self) -> &[World] {
        &self.worlds
    }

    /// The list's worlds, for a test to change.
    #[cfg(any(test, feature = "testing"))]
    #[doc(hidden)]
    pub fn worlds_mut(&mut self) -> &mut Vec<World> {
        &mut self.worlds
    }

    /// The day's copy, when there is one.
    #[must_use]
    pub fn copy(&self) -> Option<&ListCache> {
        self.copy.as_ref()
    }

    /// The hub's word on the worlds, read again about once a minute while the hub's list is shown.
    #[must_use]
    pub fn hub(&self) -> &HubWatch {
        &self.hub
    }

    /// The hub's word on the worlds, to read it again ([`HubWatch::start`]).
    pub fn hub_mut(&mut self) -> &mut HubWatch {
        &mut self.hub
    }

    /// Every world as it is shown at `now`: the list's, then the servers the player added
    /// (`state`), each with what it says of itself and whether it answered when asked (`heard`,
    /// by the world's key), else the hub's word ([`worlds::shown`]). What a world of the list said
    /// when the hub asked it ([`HubWatch::said`]) stands in for its own word while it has none;
    /// the hub never speaks for a server the player added. A server the player added that is a
    /// world of the list ([`worlds::same_world`]) is shown once, as the list's world.
    pub fn shown<'a, F>(
        &'a self,
        state: &'a LauncherState,
        now: u64,
        heard: F,
    ) -> impl Iterator<Item = World> + 'a
    where
        F: Fn(&str) -> (Option<&'a LiveStatus>, Option<bool>) + 'a,
    {
        let all = self.every_world(state, now, heard);
        let (listed, added) = all.split_at(self.worlds.len());
        let added: Vec<World> = added
            .iter()
            .filter(|a| !listed.iter().any(|l| worlds::same_world(l, a)))
            .cloned()
            .collect();
        let mut shown = listed.to_vec();
        shown.extend(added);
        shown.into_iter()
    }

    /// The world shown for `slug` at `now` ([`Self::shown`]): the world itself, or, for a server
    /// the player added that is a world of the list, that world. `None` for a slug no world has.
    pub fn shown_as<'a, F>(
        &'a self,
        state: &'a LauncherState,
        now: u64,
        heard: F,
        slug: &str,
    ) -> Option<World>
    where
        F: Fn(&str) -> (Option<&'a LiveStatus>, Option<bool>) + 'a,
    {
        let all = self.every_world(state, now, heard);
        let (listed, added) = all.split_at(self.worlds.len());
        if let Some(w) = listed.iter().find(|w| w.slug == slug) {
            return Some(w.clone());
        }
        let w = added.iter().find(|w| w.slug == slug)?;
        Some(
            listed
                .iter()
                .find(|l| worlds::same_world(l, w))
                .unwrap_or(w)
                .clone(),
        )
    }

    /// The list's worlds, then the servers the player added, each as shown at `now`.
    fn every_world<'a, F>(&'a self, state: &'a LauncherState, now: u64, heard: F) -> Vec<World>
    where
        F: Fn(&str) -> (Option<&'a LiveStatus>, Option<bool>) + 'a,
    {
        worlds::all(&self.worlds, state)
            .map(|w| {
                let (live, answered) = heard(&w.slug);
                let listed = !state.custom_worlds.iter().any(|c| c.slug == w.slug);
                let said = self.hub.said(&w.slug, now).filter(|_| listed);
                let hub = self.hub.current(&w.slug, now);
                let heard = live.or(said);
                // An answer may still come while this launcher has not asked the world itself, or,
                // for one that cannot, while the hub's word on the list is still to be taken.
                let may_answer = if self.asks_worlds {
                    answered.is_none()
                } else {
                    listed && self.hub.word_pending()
                };
                let own_count = live.is_some_and(|l| l.players.is_some());
                let count = self.hub.count(&w.slug, now).filter(|_| listed);
                let mut w = worlds::shown(w, answered, heard, hub, state);
                worlds::take_hub_count(&mut w, count.as_ref(), own_count);
                w.era_state = worlds::era_state(&w, heard.is_some(), may_answer);
                w.favorite = state.is_favorite_world(&w.slug);
                w
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the world list every launcher shows; each launcher's own test checks it
    //! shows the same).
    use super::*;
    use crate::serverlist::{COMMUNITY_LIST_URL, HUB_LIST_URL};
    use crate::status::LiveStatus;
    use crate::testing::world_list as fixture;
    use crate::world::{Emulator, Told, WorldState};

    fn summaries(list: &WorldList, now: u64) -> Vec<fixture::Summary> {
        list.shown(&crate::testing::without_relayed_worlds(), now, |_| {
            (None, None)
        })
        .map(|w| fixture::summary(&w))
        .collect()
    }

    /// The list marks the worlds the player starred, which a launcher shows first; a world a Play
    /// favorite names is not one unless starred.
    #[test]
    fn the_list_marks_the_worlds_the_player_starred() {
        let now = fixture::READ_AT;
        let mut list = WorldList::default();
        list.read_with(now, fixture::fetch(true)).unwrap();
        let mut state = crate::testing::without_relayed_worlds();
        state.set_favorite_world("ironwood", true);
        state.favourites.push(crate::state::Favourite {
            id: "f1".into(),
            name: None,
            world_slug: "coldeve".into(),
            account: "player".into(),
            client: crate::install::ClientKind::Dereth,
            dat_set_id: None,
            classic_set_id: None,
        });
        let favorites: Vec<String> = list
            .shown(&state, now, |_| (None, None))
            .filter(|w| w.favorite)
            .map(|w| w.slug)
            .collect();
        assert_eq!(favorites, ["ironwood"]);
    }

    /// The era of an Empyrean world nobody has heard from is being checked while an answer may
    /// still come: for a launcher that asks worlds itself, until it has asked; for one that
    /// cannot, until the hub's word on the list is taken. Then it is unknown, and the era a world
    /// says is known as soon as it says it. A world whose software the list does not name is
    /// taken to play the end of retail.
    #[test]
    fn an_empyrean_worlds_era_is_checked_until_an_answer_can_no_longer_come() {
        use crate::world::EraState;
        let now = fixture::READ_AT;
        let quiet_page = || {
            let mut page: serde_json::Value = serde_json::from_str(&fixture::hub_page()).unwrap();
            page["data"][3]["statusPing"] = serde_json::Value::Null;
            page["data"][0]["emulator"]["family"] = serde_json::json!(null);
            page.to_string()
        };
        let fetch = move |url: &str| -> Result<Vec<u8>, String> {
            if url.starts_with(HUB_LIST_URL) {
                Ok(quiet_page().into_bytes())
            } else {
                Err(format!("{url}: not found"))
            }
        };
        let era = |list: &WorldList, answered: Option<bool>, live: Option<&LiveStatus>| {
            list.shown_as(
                &crate::testing::without_relayed_worlds(),
                now,
                |_| (live, answered),
                "ironwood",
            )
            .unwrap()
            .era_state
        };
        let unnamed = |list: &WorldList| {
            list.shown_as(
                &crate::testing::without_relayed_worlds(),
                now,
                |_| (None, None),
                "coldeve",
            )
            .unwrap()
        };

        // A browser: the hub's word comes with the list it reads, and has nothing from Ironwood.
        let mut browser = WorldList::default();
        let copy = browser.read_with(now, fetch).unwrap().clone();
        assert_eq!(era(&browser, None, None), EraState::Unknown);
        // The day's copy shown again: the hub's word on it is still to come.
        let mut again = WorldList::default();
        again.show_copy(copy.clone()).unwrap();
        assert_eq!(era(&again, None, None), EraState::Checking);
        let w = unnamed(&again);
        assert_eq!((w.era_state, w.era), (EraState::Assumed, None));

        // A launcher that asks worlds itself: checking until it has asked.
        let mut desktop = WorldList::default();
        desktop.set_asks_worlds(true);
        desktop.show_copy(copy).unwrap();
        assert_eq!(era(&desktop, None, None), EraState::Checking);
        assert_eq!(
            era(&desktop, Some(true), None),
            EraState::Unknown,
            "asked; no status"
        );
        let says = LiveStatus {
            state: WorldState::Online,
            era: Some("infiltration".into()),
            ..LiveStatus::default()
        };
        assert_eq!(era(&desktop, Some(true), Some(&says)), EraState::Known);
    }

    /// The hub's list is read first and shown, with its word on each world; when the hub does not
    /// answer the community's own list is; when neither does, the list shown before stands.
    #[test]
    fn the_hubs_list_is_shown_the_community_list_stands_in_and_no_answer_keeps_the_last() {
        let now = fixture::READ_AT;
        let mut list = WorldList::default();
        assert!(list.wants_reading(now), "no copy yet");
        let copy = list
            .read_with(now, fixture::fetch(true))
            .expect("the hub answers")
            .clone();
        assert!(copy.is_hubs());
        assert_eq!(summaries(&list, now), fixture::from_the_hub());
        let coldeve = list
            .shown(&crate::testing::without_relayed_worlds(), now, |_| {
                (None, None)
            })
            .find(|w| w.slug == "coldeve")
            .unwrap();
        assert_eq!(
            (coldeve.state, coldeve.players),
            (WorldState::Online, Some(12))
        );
        assert!(!list.wants_reading(now), "the day's copy");

        let mut asked = Vec::new();
        let mut fetch = fixture::fetch(false);
        list.read_with(now, |url| {
            asked.push(url.to_owned());
            fetch(url)
        })
        .expect("the community answers");
        assert!(asked[0].starts_with(HUB_LIST_URL) && asked[1] == COMMUNITY_LIST_URL);
        assert_eq!(summaries(&list, now), fixture::from_the_community());

        let e = list
            .read_with(now, |url| Err(format!("{url}: offline")))
            .unwrap_err();
        assert!(
            e.contains(HUB_LIST_URL) && e.contains(COMMUNITY_LIST_URL),
            "{e}"
        );
        assert_eq!(summaries(&list, now), fixture::from_the_community());
    }

    /// A server the player added by the status document or the WebSocket endpoint the hub names
    /// for a linked world is shown once, as the hub's world, and asking for it finds that world;
    /// one the hub names nothing alike for is shown as it was added.
    #[test]
    fn a_server_added_by_a_listed_worlds_address_is_shown_once_as_the_listed_world() {
        let now = fixture::READ_AT;
        let mut list = WorldList::default();
        list.read_with(now, fixture::fetch(true))
            .expect("the hub answers");
        let mut state = crate::testing::without_relayed_worlds();
        let add = |state: &mut LauncherState, host: &str, url: &str| {
            let slug = state
                .add_custom_world("", host, "9000", None, crate::world::Emulator::Unknown)
                .unwrap();
            state.set_custom_world_url(&slug, Some(url));
            slug
        };
        let by_status = add(
            &mut state,
            "ironwood.example.org",
            "https://ironwood.example.org",
        );
        let by_ws = add(
            &mut state,
            "play.ironwood.example.org",
            fixture::IRONWOOD_WEBSOCKET,
        );
        let other = add(&mut state, "play.example.org", "wss://play.example.org/ws");
        let shown: Vec<String> = list
            .shown(&state, now, |_| (None, None))
            .map(|w| w.slug)
            .collect();
        assert_eq!(
            shown,
            [
                "coldeve",
                "frostfell",
                "seedsow",
                "ironwood",
                other.as_str()
            ],
            "one row for Ironwood, the hub's"
        );
        for slug in [&by_status, &by_ws, &"ironwood".to_owned()] {
            let w = list
                .shown_as(&state, now, |_| (None, None), slug)
                .expect("a world");
            assert_eq!(
                (w.slug.as_str(), w.websocket_url.as_deref()),
                ("ironwood", Some(fixture::IRONWOOD_WEBSOCKET)),
                "{slug}"
            );
        }
        assert_eq!(
            list.shown_as(&state, now, |_| (None, None), &other)
                .map(|w| w.slug),
            Some(other)
        );
        assert!(list
            .shown_as(&state, now, |_| (None, None), "nowhere")
            .is_none());
    }

    /// What a world of the list told the hub when it asked stands in for the world's own word:
    /// its era, systems and version are shown while it counts, the world's own answer goes before
    /// it, and a server the player added never takes it, even one kept under a key the hub's list
    /// has too.
    #[test]
    fn what_a_world_told_the_hub_is_shown_until_it_answers_and_never_for_a_server_the_player_added()
    {
        use serde_json::json;
        let now = fixture::READ_AT;
        let row = |name: &str, address: &str| {
            json!({"slug": serverlist::slug_for(name), "name": name, "isDemo": false,
                "emulator": {"family": "Empyrean"},
                "endpoint": {"mode": "manual", "address": address, "port": 9000},
                "websocketUrl": format!("wss://{address}/ws"),
                "status": {"state": "online", "playersOnline": 1,
                           "source": "server-heartbeat", "expiresAt": "2026-10-11T18:10:00Z"},
                "statusPing": {"state": "open", "playersOnline": 2, "era": "infiltration",
                               "eraFeatures": "1:0000ff", "software": "Empyrean",
                               "version": "0.3.7", "name": name,
                               "expiresAt": "2026-10-11T18:03:00Z"}})
        };
        let page = json!({"data": [row("Ironwood", "ironwood.example.org"),
                                   row("Custom 1", "custom.example.org")],
                          "nextOffset": null})
        .to_string();
        let mut list = WorldList::default();
        list.read_with(now, |url| {
            if url.starts_with(HUB_LIST_URL) {
                Ok(page.clone().into_bytes())
            } else {
                Err("not found".into())
            }
        })
        .expect("the hub answers");
        let mut state = crate::testing::without_relayed_worlds();
        let shown = |state: &LauncherState, at: u64, slug: &str| {
            list.shown_as(state, at, |_| (None, None), slug)
                .expect("a world")
        };

        let w = shown(&state, now, "ironwood");
        assert_eq!(
            (
                w.era.as_deref(),
                w.era_source,
                w.emulator_version.as_deref()
            ),
            (Some("infiltration"), Some(Told::World), Some("0.3.7"))
        );
        assert_eq!((w.state, w.players), (WorldState::Online, Some(2)));
        assert!(w.era_features.is_some() && w.features_source == Some(Told::World));
        let w = shown(&state, now + 180, "ironwood");
        assert_eq!(
            (w.era.as_deref(), w.emulator_version.as_deref(), w.players),
            (None, None, Some(1)),
            "past its expiry: the heartbeat alone"
        );

        // The world's own answer goes before what it told the hub.
        let own = LiveStatus {
            state: WorldState::Online,
            era: Some("eor".into()),
            version: Some("0.3.8".into()),
            ..LiveStatus::default()
        };
        let w = list
            .shown_as(&state, now, |_| (Some(&own), None), "ironwood")
            .unwrap();
        assert_eq!(
            (w.era.as_deref(), w.emulator_version.as_deref()),
            (Some("eor"), Some("0.3.8"))
        );

        // A server the player added by the listed world's WebSocket endpoint is that world, with
        // what it told the hub; one added by its game address alone is a server of its own, and
        // the hub says nothing of it, even kept under the key of another listed world.
        let by_address = state
            .add_custom_world("", "ironwood.example.org", "9000", None, Emulator::Unknown)
            .unwrap();
        assert_eq!(
            by_address, "custom-1",
            "the key the hub's Custom 1 is kept under"
        );
        let by_ws = state
            .add_custom_world("", "ironwood.example.org", "443", None, Emulator::Unknown)
            .unwrap();
        state.set_custom_world_url(&by_ws, Some("wss://ironwood.example.org/ws"));
        let w = shown(&state, now, &by_ws);
        assert_eq!(
            (w.slug.as_str(), w.era.as_deref()),
            ("ironwood", Some("infiltration"))
        );
        let all = list.every_world(&state, now, |_| (None, None));
        let added = &all[list.worlds().len()..];
        assert_eq!(
            added.iter().map(|w| w.slug.as_str()).collect::<Vec<_>>(),
            [by_address.as_str(), by_ws.as_str()]
        );
        for w in added {
            assert_eq!(
                (
                    w.era.as_deref(),
                    w.era_source,
                    w.emulator_version.as_deref()
                ),
                (None, None, None),
                "{}",
                w.slug
            );
        }
    }

    /// The day's copy is shown at start, and the list is read only once the copy is a day old; a
    /// copy that does not read is no copy, and the list is read.
    #[test]
    fn the_days_copy_is_shown_at_start_and_one_that_does_not_read_is_no_copy() {
        let now = fixture::READ_AT;
        let mut first = WorldList::default();
        let copy = first.read_with(now, fixture::fetch(true)).unwrap().clone();

        let again = WorldList::new(serverlist::sources(None), Some(copy.clone()));
        let shown: Vec<_> = again
            .shown(&crate::testing::without_relayed_worlds(), now, |_| {
                (None, None)
            })
            .map(|w| (w.slug, w.players))
            .collect();
        assert_eq!(shown.len(), fixture::from_the_hub().len());
        assert!(
            shown.iter().all(|(_, players)| players.is_none()),
            "the copy's status is never taken: {shown:?}"
        );
        assert!(!again.wants_reading(now + 3_600));
        assert!(again.wants_reading(now + serverlist::MAX_AGE_SECS));

        let damaged = ListCache {
            fetched_at: now,
            list: "<html/>".into(),
            hub: Vec::new(),
            sources: serverlist::source_urls(&serverlist::sources(None)),
        };
        let mut list = WorldList::new(serverlist::sources(None), Some(damaged.clone()));
        assert!(list.worlds().is_empty() && list.copy().is_none());
        assert!(list.wants_reading(now), "no copy: the list is read");
        list.show_copy(copy).unwrap();
        assert!(list.show_copy(damaged).is_err());
        assert_eq!(
            list.worlds().len(),
            fixture::from_the_hub().len(),
            "it stands"
        );
    }

    /// A copy is shown at start only by a launcher that reads the sources it was read from: one
    /// started on another hub's list reads that hub's at once, whatever the age of the copy of the
    /// network's, and the next started on the usual sources reads the network's again, whatever the
    /// age of the other hub's copy.
    #[test]
    fn a_copy_read_from_other_sources_is_not_shown_and_the_list_is_read_both_ways() {
        const DEV: &str = "https://dev-hub.example.org/v1/servers";
        let now = fixture::READ_AT;
        let usual = serverlist::sources(None);
        let dev = serverlist::sources_from(None, Some(DEV));
        let mut first = WorldList::new(usual.clone(), None);
        let networks = first.read_with(now, fixture::fetch(true)).unwrap().clone();

        // Started on the dev hub's list an hour later: the network's copy is not the dev hub's.
        let mut on_dev = WorldList::new(dev.clone(), Some(networks.clone()));
        assert!(on_dev.worlds().is_empty() && on_dev.copy().is_none());
        assert!(
            on_dev.wants_reading(now + 3_600),
            "the dev hub's list is read at once"
        );
        assert!(on_dev.show_copy(networks.clone()).is_err());
        let mut asked = Vec::new();
        let devs = on_dev
            .read_with(now + 3_600, |url| {
                asked.push(url.to_owned());
                fixture::fetch(true)(url)
            })
            .unwrap()
            .clone();
        assert!(asked[0].starts_with(DEV), "{asked:?}");
        assert!(devs.is_from(&dev));

        // The ordinary launcher after it: the dev hub's copy is not the network's.
        let again = WorldList::new(usual.clone(), Some(devs.clone()));
        assert!(again.copy().is_none());
        assert!(
            again.wants_reading(now + 7_200),
            "the network's list is read again"
        );
        // And each copy stands in for its own sources, as before.
        assert!(!WorldList::new(dev, Some(devs)).wants_reading(now + 7_200));
        assert!(!WorldList::new(usual.clone(), Some(networks)).wants_reading(now + 7_200));

        // A copy kept before copies recorded their sources is read again.
        let older = ListCache {
            sources: Vec::new(),
            ..first.copy().unwrap().clone()
        };
        let list = WorldList::new(usual, Some(older));
        assert!(list.copy().is_none() && list.wants_reading(now));
    }
}
