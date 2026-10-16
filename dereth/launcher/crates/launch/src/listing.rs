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
        };
        if let Some(copy) = copy {
            // A copy that does not read leaves the list empty, to be read.
            let _ = list.show_copy(copy);
        }
        list
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
    pub fn hub_mut(&mut self) -> &mut HubWatch {
        &mut self.hub
    }

    /// Every world as it is shown at `now`: the list's, then the servers the player added
    /// (`state`), each with what it says of itself and whether it answered when asked (`heard`,
    /// by the world's key), else the hub's word ([`worlds::shown`]).
    pub fn shown<'a, F>(
        &'a self,
        state: &'a LauncherState,
        now: u64,
        heard: F,
    ) -> impl Iterator<Item = World> + 'a
    where
        F: Fn(&str) -> (Option<&'a LiveStatus>, Option<bool>) + 'a,
    {
        worlds::all(&self.worlds, state).map(move |w| {
            let (live, answered) = heard(&w.slug);
            let hub = self.hub.current(&w.slug, now);
            worlds::shown(w, answered, live, hub, state)
        })
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the world list every launcher shows; each launcher's own test checks it
    //! shows the same).
    use super::*;
    use crate::serverlist::{COMMUNITY_LIST_URL, HUB_LIST_URL};
    use crate::testing::world_list as fixture;
    use crate::world::WorldState;

    fn summaries(list: &WorldList, now: u64) -> Vec<fixture::Summary> {
        list.shown(&LauncherState::default(), now, |_| (None, None))
            .map(|w| fixture::summary(&w))
            .collect()
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
            .shown(&LauncherState::default(), now, |_| (None, None))
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

    /// The day's copy is shown at start, and the list is read only once the copy is a day old; a
    /// copy that does not read is no copy, and the list is read.
    #[test]
    fn the_days_copy_is_shown_at_start_and_one_that_does_not_read_is_no_copy() {
        let now = fixture::READ_AT;
        let mut first = WorldList::default();
        let copy = first.read_with(now, fixture::fetch(true)).unwrap().clone();

        let again = WorldList::new(serverlist::sources(None), Some(copy.clone()));
        let shown: Vec<_> = again
            .shown(&LauncherState::default(), now, |_| (None, None))
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
