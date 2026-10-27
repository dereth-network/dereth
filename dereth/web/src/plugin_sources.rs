//! The front page's plugin sources: the desktop launcher's own shelf of sources and the plugins
//! installed from them ([`Shelf`]), kept in this browser, with the page doing the fetching.
//!
//! Every rule is the launcher's, from its code: a source is added by its `https://` address (the
//! page has the player confirm it first), and Dereth's own is always there; a zip is kept only when it is the
//! release its source lists; ids under `network.dereth.` install only from Dereth's source; and an
//! installed plugin updates only from its own source, only to a newer release, on its own as the
//! setting says. Reading every source and applying those updates is the
//! launcher's walk ([`Refresh`]), which the page runs when it opens and when Refresh is pressed.
//!
//! What is the page's own is where things are kept, and what a page cannot fetch. The sources,
//! the last list each gave and the settings are kept in the origin's private storage beside the
//! client's settings file ([`SOURCES_PATH`]). Each installed plugin's zip, and the record of what
//! is installed ([`INSTALLED_PATH`]), are kept in the folder of plugins the client finds as it
//! starts, so removing every plugin there removes the record too. This side keeps the shelf and
//! says what to write ([`PageShelf::take_writes`]); the worker writes it.
//!
//! A page reads another host's files only when that host lets any page read them
//! (`Access-Control-Allow-Origin`), and cannot tell a host that does not from one that is offline:
//! a fetch that brought nothing at all is said so plainly ([`UNREADABLE_SOURCE`]), and the rest
//! goes on. GitHub's release downloads never let a page read them, so a plugin whose zip is one is
//! sent to the desktop launcher before anything is fetched.

use dereth_launch::plugins::{
    self, AutoUpdate, Download, Keep, PluginError, Refresh, Shelf, INSTALLED_FILE, SOURCES_FILE,
};
use serde_json::{json, Value};

/// Where the page keeps its sources, the last list each gave and the settings, in the origin's
/// private storage: beside the client's settings file.
pub const SOURCES_PATH: &str = "plugin-sources.json";

/// Where the page keeps the record of what it installed: in the folder of plugins the client
/// finds, beside the zips.
pub const INSTALLED_PATH: &str = "plugins/installed.json";

/// What the page says of a source's list it could not fetch at all.
pub const UNREADABLE_SOURCE: &str =
    "Couldn't read this source here. Its host may not let web pages read it.";

/// Where the page keeps plugin `id`'s zip: among the plugins the client finds.
#[must_use]
pub fn zip_path(id: &str) -> String {
    format!("plugins/{}.zip", plugins::folder_name(id))
}

/// Whether a zip at `url` is a GitHub release download, which no page can fetch: GitHub sends
/// them with no header that lets another site's page read them.
#[must_use]
pub fn desktop_only(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    let parts: Vec<&str> = path
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .split('/')
        .collect();
    (host == "github.com" || host == "www.github.com")
        && parts.get(2) == Some(&"releases")
        && (parts.get(3) == Some(&"download")
            || (parts.get(3) == Some(&"latest") && parts.get(4) == Some(&"download")))
}

/// What a fetch brought, as the worker reports it: the `body`, the `error` the host answered with,
/// or neither when the page could not reach it at all (said as `unreachable` says).
fn fetched(
    body: Option<Vec<u8>>,
    error: Option<String>,
    unreachable: impl FnOnce() -> String,
) -> Result<Vec<u8>, String> {
    match (body, error) {
        (Some(body), _) => Ok(body),
        (None, Some(e)) => Err(e),
        (None, None) => Err(unreachable()),
    }
}

/// What the page says of a source whose last read failed with `error`.
fn problem(error: &str) -> String {
    if error == UNREADABLE_SOURCE {
        error.to_owned()
    } else {
        format!("Couldn't read this source: {error}")
    }
}

/// What the page says of a plugin's zip it could not fetch at all.
fn unreadable_zip(name: &str) -> String {
    format!("Couldn't download {name} here. Its host may not let web pages read it.")
}

/// What the page says of a plugin whose zip is a GitHub release download.
fn desktop_words(name: &str) -> String {
    format!(
        "{name} downloads from a GitHub release, which web pages can't fetch. Install it with the \
         desktop launcher."
    )
}

/// The page's keep: the records as the worker read them when the page opened, and the writes to
/// make, in order.
#[derive(Debug, Default, Clone)]
pub struct PageKeep {
    sources: Option<Vec<u8>>,
    installed: Option<Vec<u8>>,
    /// Each a path in the origin's private storage and its bytes, or `None` to delete the file.
    writes: Vec<(String, Option<Vec<u8>>)>,
}

impl PageKeep {
    fn queue(&mut self, path: String, bytes: Option<Vec<u8>>) {
        // A later write of a file replaces one still waiting.
        self.writes.retain(|(p, _)| *p != path);
        self.writes.push((path, bytes));
    }
}

impl Keep for PageKeep {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        match name {
            SOURCES_FILE => self.sources.clone(),
            INSTALLED_FILE => self.installed.clone(),
            _ => None,
        }
    }

    fn write(&mut self, name: &str, bytes: &[u8]) -> Result<(), PluginError> {
        let path = match name {
            SOURCES_FILE => SOURCES_PATH,
            INSTALLED_FILE => INSTALLED_PATH,
            _ => return Err(PluginError::Io(format!("the page keeps no {name}"))),
        };
        self.queue(path.to_owned(), Some(bytes.to_vec()));
        Ok(())
    }

    fn put_zip(&mut self, id: &str, zip: &[u8]) -> Result<(), PluginError> {
        self.queue(zip_path(id), Some(zip.to_vec()));
        Ok(())
    }

    fn remove_zip(&mut self, id: &str) -> Result<(), PluginError> {
        self.queue(zip_path(id), None);
        Ok(())
    }
}

/// The name of the plugin whose zip `shelf` lists at `url`; the address itself when none does.
fn name_at(shelf: &Shelf<PageKeep>, url: &str) -> String {
    shelf
        .sources
        .iter()
        .flat_map(|s| s.catalog.plugins.iter().map(move |e| (s, e)))
        .find(|(s, e)| shelf.zip_of(&s.url, e.id.as_str()).as_deref() == Some(url))
        .map_or_else(|| url.to_owned(), |(_, e)| e.name.clone())
}

/// The front page's plugins: the shelf, and the refresh under way.
#[derive(Debug)]
pub struct PageShelf {
    shelf: Shelf<PageKeep>,
    refresh: Option<Refresh>,
    /// The address of the download the refresh asked for last.
    asked: Option<String>,
}

impl Default for PageShelf {
    fn default() -> Self {
        Self::open(None, None)
    }
}

impl PageShelf {
    /// The shelf the page kept: its sources' record and its installed record, as the worker read
    /// them (`None` for one it does not have).
    #[must_use]
    pub fn open(sources: Option<Vec<u8>>, installed: Option<Vec<u8>>) -> Self {
        Self {
            shelf: Shelf::load(PageKeep {
                sources,
                installed,
                writes: Vec::new(),
            }),
            refresh: None,
            asked: None,
        }
    }

    /// The writes to make now, in order: a path in the origin's private storage and its bytes, or
    /// `None` to delete the file. Each is answered once.
    pub fn take_writes(&mut self) -> Vec<(String, Option<Vec<u8>>)> {
        std::mem::take(&mut self.shelf.keep.writes)
    }

    /// The page's view, as JSON: the launcher's Plugins page ([`plugins::PluginsView`]), with each
    /// source's problem in the page's words (`problem`, null when it read), and whether the zip a
    /// source lists for each plugin, installed or not, is one only the desktop launcher can fetch
    /// (`desktop_only`).
    #[must_use]
    pub fn view(&self) -> Value {
        let mut v = serde_json::to_value(self.shelf.view()).unwrap_or(Value::Null);
        if let Some(sources) = v.get_mut("sources").and_then(Value::as_array_mut) {
            for s in sources {
                let said = s.get("error").and_then(Value::as_str).map(problem);
                s["problem"] = json!(said);
            }
        }
        for list in ["available", "installed"] {
            for a in v
                .get_mut(list)
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                let at = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
                let only = self
                    .shelf
                    .zip_of(&at("source"), &at("id"))
                    .is_some_and(|u| desktop_only(&u));
                a["desktop_only"] = json!(only);
            }
        }
        v
    }

    /// The name of plugin `id`: as installed, else as a source lists it, else its id.
    fn name_of(&self, id: &str) -> String {
        if let Some(p) = self.shelf.get(id) {
            return p.release.name.clone();
        }
        self.shelf
            .sources
            .iter()
            .find_map(|s| s.catalog.get(id))
            .map_or_else(|| id.to_owned(), |e| e.name.clone())
    }

    /// Add the source at `url`, which the page has the player confirm first. Answers the download
    /// of its list, which the page makes next ([`PageShelf::source_read`]).
    ///
    /// # Errors
    /// Not an `https://` address, or already a source, in words for the page.
    pub fn add_source(&mut self, url: &str) -> Result<Download, String> {
        let url = self.shelf.add_source(url).map_err(|e| e.to_string())?;
        Ok(Download {
            url,
            limit: plugins::MAX_LIST,
        })
    }

    /// What fetching the list of the source just added at `url` brought, at `now` (seconds since
    /// the epoch): its `body`, the `error` its host answered with, or neither when the page could
    /// not reach it. Answers what to tell the player.
    pub fn source_read(
        &mut self,
        url: &str,
        body: Option<Vec<u8>>,
        error: Option<String>,
        now: u64,
    ) -> String {
        let read = fetched(body, error, || UNREADABLE_SOURCE.to_owned());
        if let Err(e) = self.shelf.record_read(url, read, now) {
            return e.to_string();
        }
        let Some(s) = self.shelf.source(url) else {
            return String::new();
        };
        match (&s.error, s.catalog.plugins.len()) {
            (Some(e), _) => format!("Added {}. {}", s.label(), problem(e)),
            (None, 1) => format!("Added {}: 1 plugin.", s.label()),
            (None, n) => format!("Added {}: {n} plugins.", s.label()),
        }
    }

    /// Remove the source at `url`; the plugins installed from it stay, with no updates.
    ///
    /// # Errors
    /// Dereth's own source, or not a source.
    pub fn remove_source(&mut self, url: &str) -> Result<(), String> {
        self.shelf.remove_source(url).map_err(|e| e.to_string())
    }

    /// The download to make to install `id` from `source`, or, with no source, to update the
    /// installed `id` from its own.
    ///
    /// # Errors
    /// What the launcher refuses ([`Shelf::plan_install`]), or a zip only the desktop launcher can
    /// fetch, in words for the page.
    pub fn plan(&self, source: Option<&str>, id: &str) -> Result<Download, String> {
        let download = match source {
            Some(s) => self.shelf.plan_install(s, id),
            None => self.shelf.plan_update(id),
        }
        .map_err(|e| e.to_string())?;
        if desktop_only(&download.url) {
            return Err(desktop_words(&self.name_of(id)));
        }
        Ok(download)
    }

    /// What fetching the download [`PageShelf::plan`] named brought, as [`PageShelf::source_read`]
    /// takes it: installed from `source`, or with no source updated from the plugin's own, when it
    /// is the release listed. Answers what to tell the player.
    ///
    /// # Errors
    /// Nothing came, or the release was refused, in words for the page.
    pub fn take(
        &mut self,
        source: Option<&str>,
        id: &str,
        body: Option<Vec<u8>>,
        error: Option<String>,
    ) -> Result<String, String> {
        let zip = fetched(body, error, || unreadable_zip(&self.name_of(id)))?;
        let done = match source {
            Some(s) => self.shelf.install(s, id, &zip),
            None => self.shelf.update(id, &zip),
        }
        .map_err(|e| e.to_string())?;
        Ok(done.said())
    }

    /// Remove the installed plugin `id` and its zip.
    ///
    /// # Errors
    /// Not installed.
    pub fn remove(&mut self, id: &str) -> Result<(), String> {
        self.shelf.remove(id).map_err(|e| e.to_string())
    }

    /// Which plugins update without the player: `none`, `all` or `first_party`.
    ///
    /// # Errors
    /// Not one of those.
    pub fn set_updates(&mut self, setting: &str) -> Result<(), String> {
        let setting = AutoUpdate::parse(setting)
            .ok_or_else(|| format!("{setting} is not an update setting"))?;
        self.shelf
            .set_auto_update(setting)
            .map_err(|e| e.to_string())
    }

    /// Start reading every source and applying the updates that apply on their own: the first
    /// download to make, or `None` when there is nothing to fetch.
    pub fn refresh_start(&mut self) -> Option<Download> {
        self.refresh = Some(Refresh::new(&self.shelf));
        self.asked = None;
        self.step(None, 0)
    }

    /// What the last download the refresh asked for brought, at `now`, as
    /// [`PageShelf::source_read`] takes it: the next download, or `None` once the refresh is over
    /// ([`PageShelf::refresh_done`]).
    pub fn refresh_next(
        &mut self,
        body: Option<Vec<u8>>,
        error: Option<String>,
        now: u64,
    ) -> Option<Download> {
        let asked = self.asked.take().unwrap_or_default();
        let read = fetched(body, error, || {
            if self.shelf.source(&asked).is_some() {
                UNREADABLE_SOURCE.to_owned()
            } else {
                unreadable_zip(&name_at(&self.shelf, &asked))
            }
        });
        self.step(Some(read), now)
    }

    /// One step of the walk: a download only the desktop launcher can make is answered at once.
    fn step(&mut self, mut read: Option<Result<Vec<u8>, String>>, now: u64) -> Option<Download> {
        let walk = self.refresh.as_mut()?;
        loop {
            let download = walk.next(&mut self.shelf, read.take(), now)?;
            if desktop_only(&download.url) {
                read = Some(Err(desktop_words(&name_at(&self.shelf, &download.url))));
                continue;
            }
            self.asked = Some(download.url.clone());
            return Some(download);
        }
    }

    /// What the refresh came to, a line each; empty when nothing is new.
    pub fn refresh_done(&mut self) -> String {
        self.asked = None;
        self.refresh
            .take()
            .map(|walk| walk.said(&self.shelf))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the front page's plugin sources are the desktop launcher's logic, kept in
    //! the browser).
    use super::*;
    use dereth_launch::plugins::FIRST_PARTY;
    use dereth_launch::testing::{plugin_list as list, plugin_zip as zip};

    const OTHER: &str = "https://example.github.io/plugins/plugins.json";
    const CHAT: &str = "[permissions]\n\"chat.send\" = \"Says hello.\"\n";

    fn api() -> String {
        dereth_client_sdk::plugin_api::ApiVersion::CURRENT.to_string()
    }

    /// A plugin's state in the page's view, by source and id.
    fn state(view: &Value, source: &str, id: &str) -> String {
        view["available"]
            .as_array()
            .expect("a list")
            .iter()
            .find(|a| a["source"] == source && a["id"] == id)
            .and_then(|a| a["state"].as_str())
            .unwrap_or("not listed")
            .to_owned()
    }

    /// The origin's private storage, as the page's writes leave it.
    #[derive(Debug, Default)]
    struct Disk(std::collections::BTreeMap<String, Vec<u8>>);

    impl Disk {
        /// Make the writes the page asks for; answers each path written, and the size written
        /// (`None` for a file deleted).
        fn write(&mut self, page: &mut PageShelf) -> Vec<(String, Option<usize>)> {
            page.take_writes()
                .into_iter()
                .map(|(path, bytes)| {
                    let size = bytes.as_ref().map(Vec::len);
                    match bytes {
                        Some(b) => self.0.insert(path.clone(), b),
                        None => self.0.remove(&path),
                    };
                    (path, size)
                })
                .collect()
        }

        /// The shelf the page opens on its next visit.
        fn open(&self) -> PageShelf {
            PageShelf::open(
                self.0.get(SOURCES_PATH).cloned(),
                self.0.get(INSTALLED_PATH).cloned(),
            )
        }
    }

    #[test]
    fn a_source_is_added_by_its_address_and_dereths_own_stays() {
        let mut disk = Disk::default();
        let mut page = disk.open();
        let view = page.view();
        assert_eq!(view["sources"][0]["url"], FIRST_PARTY);
        assert_eq!(view["sources"].as_array().map(Vec::len), Some(1));
        assert!(page.add_source("http://example.org/plugins.json").is_err());
        let added = page.add_source(OTHER).expect("added");
        assert_eq!(
            (added.url.as_str(), added.limit),
            (OTHER, plugins::MAX_LIST)
        );
        let meter = zip("org.example.meter", "1.0.0", &api(), CHAT);
        let said = page.source_read(OTHER, Some(list(&[&meter])), None, 1);
        assert_eq!(said, "Added example.github.io: 1 plugin.");
        let view = page.view();
        assert_eq!(state(&view, OTHER, "org.example.meter"), "install");
        assert_eq!(view["available"][0]["permissions"][0], "chat.send");
        assert_eq!(view["sources"][1]["problem"], Value::Null);
        assert!(page.remove_source(FIRST_PARTY).is_err());

        // Kept with the page's settings: the next visit has the source, and its last list.
        let written = disk.write(&mut page);
        assert_eq!(
            written.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
            [SOURCES_PATH]
        );
        assert!(page.take_writes().is_empty(), "each write is answered once");
        assert_eq!(
            state(&disk.open().view(), OTHER, "org.example.meter"),
            "install"
        );

        page.remove_source(OTHER).expect("removed");
        disk.write(&mut page);
        let mut again = disk.open();
        assert_eq!(again.view()["sources"].as_array().map(Vec::len), Some(1));
        assert!(again
            .add_source("https://plugins.example.net/list.json")
            .is_ok());
    }

    #[test]
    fn installs_follow_the_launchers_rules_and_are_kept_among_the_plugins_the_client_finds() {
        let mut disk = Disk::default();
        let mut page = disk.open();
        let own = zip("network.dereth.meter", "1.0.0", &api(), "");
        let claim = zip("network.dereth.meter", "9.0.0", &api(), "");
        let theirs = zip("org.example.meter", "1.0.0", &api(), "");
        let theirs2 = zip("org.example.meter", "2.0.0", &api(), "");
        let second = "https://second.example.org/plugins.json";
        page.add_source(OTHER).expect("added");
        page.add_source(second).expect("added");
        assert!(page.refresh_start().is_some());
        for body in [list(&[&own]), list(&[&claim, &theirs]), list(&[&theirs2])] {
            page.refresh_next(Some(body), None, 1);
        }
        assert_eq!(page.refresh_done(), "");
        disk.write(&mut page);
        let view = page.view();
        assert_eq!(
            state(&view, OTHER, "network.dereth.meter"),
            "first_party_only"
        );
        assert!(page
            .plan(Some(OTHER), "network.dereth.meter")
            .is_err_and(|e| e.contains("Dereth's own")));

        // Dereth's own, from Dereth's source: the zip beside the list, checked before it is kept.
        let plan = page
            .plan(Some(FIRST_PARTY), "network.dereth.meter")
            .expect("a plan");
        assert_eq!(
            plan.url,
            "https://plugins.dereth.network/network.dereth.meter.zip"
        );
        assert_eq!(plan.limit, own.len());
        let wrong = page.take(Some(FIRST_PARTY), "network.dereth.meter", Some(claim), None);
        assert!(wrong.is_err_and(|e| e.contains("was not installed")));
        assert!(page.take_writes().is_empty(), "nothing kept");
        let said = page.take(
            Some(FIRST_PARTY),
            "network.dereth.meter",
            Some(own.clone()),
            None,
        );
        assert_eq!(
            said.as_deref(),
            Ok("Installed Meter 1.0.0. Turn it on in the client.")
        );
        let written = disk.write(&mut page);
        assert_eq!(
            written.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
            ["plugins/network.dereth.meter.zip", INSTALLED_PATH]
        );
        assert_eq!(disk.0.get("plugins/network.dereth.meter.zip"), Some(&own));

        // Installed from one source, it updates only from there: another's newer release is not
        // its update, and no source offers it again.
        page.take(Some(OTHER), "org.example.meter", Some(theirs), None)
            .expect("installed");
        let view = page.view();
        assert_eq!(state(&view, second, "org.example.meter"), "not listed");
        assert_eq!(state(&view, OTHER, "org.example.meter"), "not listed");
        assert!(page
            .plan(None, "org.example.meter")
            .is_err_and(|e| e.contains("up to date")));
        assert!(page.plan(Some(second), "org.example.meter").is_err());

        // Kept: the next visit has both, from their sources; removed, its zip goes.
        disk.write(&mut page);
        let mut again = disk.open();
        let installed = again.view()["installed"].clone();
        assert_eq!(installed.as_array().map(Vec::len), Some(2));
        assert_eq!(installed[1]["source"], OTHER);
        again.remove("org.example.meter").expect("removed");
        let written = disk.write(&mut again);
        assert_eq!(
            written,
            [
                ("plugins/org.example.meter.zip".to_owned(), None),
                (INSTALLED_PATH.to_owned(), written[1].1),
            ]
        );
        assert!(!disk.0.contains_key("plugins/org.example.meter.zip"));
        assert_eq!(
            disk.open().view()["installed"].as_array().map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn a_host_the_page_cannot_read_is_said_plainly_and_the_rest_keep_working() {
        let mut page = PageShelf::default();
        page.add_source(OTHER).expect("added");
        // Neither a body nor an answer: no header letting pages read it, or offline.
        let said = page.source_read(OTHER, None, None, 1);
        assert_eq!(
            said,
            format!("Added example.github.io. {UNREADABLE_SOURCE}")
        );
        let own = zip("network.dereth.meter", "1.0.0", &api(), "");
        let first = page.refresh_start().expect("Dereth's source first");
        assert_eq!(first.url, FIRST_PARTY);
        let next = page
            .refresh_next(Some(list(&[&own])), None, 2)
            .expect("then the other");
        assert_eq!(next.url, OTHER);
        assert!(page.refresh_next(None, None, 2).is_none());
        assert_eq!(
            page.refresh_done(),
            format!("example.github.io: {UNREADABLE_SOURCE}")
        );
        let view = page.view();
        assert_eq!(view["sources"][1]["problem"], UNREADABLE_SOURCE);
        assert_eq!(view["sources"][0]["problem"], Value::Null);
        assert_eq!(state(&view, FIRST_PARTY, "network.dereth.meter"), "install");

        // A zip that does not come is said with the plugin's name; one that does installs.
        let plan = page
            .plan(Some(FIRST_PARTY), "network.dereth.meter")
            .expect("a plan");
        assert!(!plan.url.is_empty());
        assert_eq!(
            page.take(Some(FIRST_PARTY), "network.dereth.meter", None, None),
            Err("Couldn't download Meter here. Its host may not let web pages read it.".to_owned())
        );
        page.take(Some(FIRST_PARTY), "network.dereth.meter", Some(own), None)
            .expect("installed");

        // A host that answers, with an error, says what it answered.
        page.refresh_start();
        page.refresh_next(None, Some("HTTP 404".to_owned()), 3);
        page.refresh_done();
        assert_eq!(
            page.view()["sources"][0]["problem"],
            "Couldn't read this source: HTTP 404"
        );
        assert_eq!(page.view()["installed"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn a_zip_that_is_a_github_release_download_is_left_to_the_desktop_launcher() {
        let release = "https://github.com/someone/prices/releases/download/v1.0.4/prices.zip";
        assert!(desktop_only(release));
        assert!(desktop_only(
            "https://github.com/someone/prices/releases/latest/download/prices.zip"
        ));
        assert!(!desktop_only("https://someone.github.io/prices/prices.zip"));
        assert!(!desktop_only(
            "https://github.com/someone/prices/blob/main/prices.zip"
        ));

        let mut page = PageShelf::default();
        page.add_source(OTHER).expect("added");
        let v1 = zip("org.example.prices", "1.0.0", &api(), "");
        let v2 = zip("org.example.prices", "1.1.0", &api(), "");
        let at_release = |bytes: Vec<u8>| {
            String::from_utf8(bytes)
                .expect("text")
                .replace("\"org.example.prices.zip\"", &format!("\"{release}\""))
                .into_bytes()
        };
        page.source_read(OTHER, Some(at_release(list(&[&v1]))), None, 1);
        let view = page.view();
        assert_eq!(view["available"][0]["desktop_only"], true);
        assert!(page
            .plan(Some(OTHER), "org.example.prices")
            .is_err_and(|e| e.ends_with("Install it with the desktop launcher.")));

        // Installed while its zip was beside the list, its next release on GitHub is not fetched
        // here, even when it would update on its own.
        page.source_read(OTHER, Some(list(&[&v1])), None, 2);
        page.take(Some(OTHER), "org.example.prices", Some(v1), None)
            .expect("installed");
        page.set_updates("all").expect("set");
        let mut asked = vec![page.refresh_start().expect("a list").url];
        let mut read = Some(list(&[]));
        while let Some(d) = page.refresh_next(read.take(), None, 3) {
            read = Some(at_release(list(&[&v2])));
            asked.push(d.url);
        }
        assert_eq!(asked, [FIRST_PARTY, OTHER]);
        assert_eq!(
            page.refresh_done(),
            "Meter downloads from a GitHub release, which web pages can't fetch. Install it \
             with the desktop launcher."
        );
        let installed = &page.view()["installed"][0];
        assert_eq!(installed["update"]["to"], "1.1.0");
        assert_eq!(
            installed["desktop_only"], true,
            "its update is the launcher's"
        );
    }

    /// The page's policy lets it read Dereth's source and any other `https://` host a player adds.
    #[test]
    fn the_pages_policy_lets_it_read_any_https_source() {
        let headers = include_str!("../www/_headers");
        let policy = headers
            .lines()
            .find(|l| l.trim_start().starts_with("Content-Security-Policy:"))
            .expect("a policy");
        let connect = policy
            .split(';')
            .find(|d| d.trim().starts_with("connect-src"))
            .expect("connect-src");
        let url = FIRST_PARTY;
        let host_end = url["https://".len()..]
            .find('/')
            .map_or(url.len(), |i| i + "https://".len());
        assert_eq!(&url[..host_end], "https://plugins.dereth.network");
        assert!(
            connect.split_whitespace().any(|s| s == "https:"),
            "{connect}"
        );
    }
}
