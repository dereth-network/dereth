//! The list of worlds, where it is read from, and the copy of it the launcher keeps for a day.
//!
//! The list comes from the Dereth Network's hub (`/v1/servers`, read a page at a time), which
//! imports the community's list (`Servers.xml` in the `acresources/serverslist` repository) daily
//! and adds the worlds registered at the hub. When the hub does not answer with its list, the
//! community's own `Servers.xml` is read instead. A list the player names is a `Servers.xml`, read
//! alone.
//!
//! Either way the list is the world set: per world an id that never changes (the community list's,
//! which the hub carries over), a name, a description, the emulator (`ACE` or `GDL`), the host and
//! port, the rules (`PvE` or `PvP`), how settled the world is (`Stable`, `Development` or
//! `Experimental`) and its website and Discord. It says nothing about a world's era or data files,
//! and whether a world is up is asked of the world itself.
//!
//! A world is kept under [`slug_for`] its name, whichever list it came from, so what is remembered
//! for it (its accounts, passwords and choices) stays with it when one list stands in for the
//! other.
//!
//! With the servers the player adds by hand, the list is every world the launcher shows. What the
//! list does not say (era, systems, version, players) comes from the world itself when it answers
//! (Empyrean's status document) or from the player. Whether a world is up and how many are on also
//! comes from the hub's list read again while the worlds are shown ([`crate::hubstatus`]), not
//! from this copy, which can be a day old.
//!
//! Reading is tolerant: a row without a name is skipped, a malformed port is no endpoint, and
//! nothing in one row can cost the player another. Of the hub's rows, one with no address to
//! connect to is not shown, and a world the hub lists for demonstration is shown as a test world.

use std::collections::{BTreeMap, HashSet};
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::world::{Emulator, Endpoint, Links, World};

/// Where the list is read from unless the launcher is told otherwise: the Dereth Network's hub.
pub const HUB_LIST_URL: &str = "https://hub.dereth.network/v1/servers";

/// Where the list is read from when the hub does not answer with its list: the community's own.
pub const COMMUNITY_LIST_URL: &str =
    "https://raw.githubusercontent.com/acresources/serverslist/master/Servers.xml";

/// How settled a world the hub lists for demonstration is said to be: a test world.
pub const TEST_WORLD: &str = "Test";

/// How many worlds each page of the hub's list asks for: the most the hub gives on one.
pub const HUB_PAGE_SIZE: u64 = 100;

/// The most pages of the hub's list read, so that a list that never ends still ends.
const HUB_MAX_PAGES: usize = 100;

/// How a list is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListFormat {
    /// The hub's `/v1/servers`: JSON, a page at a time.
    Hub,
    /// The community's `Servers.xml`.
    ServersXml,
}

/// Where a list is read from, and how it is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListSource {
    pub url: String,
    pub format: ListFormat,
    /// Whether a list that shows no world is taken as it comes. A list the player names is; one of
    /// the usual sources that shows none is passed over as no answer is, so an empty list never
    /// takes the place of the worlds on screen or of the day's copy.
    pub empty_is_an_answer: bool,
}

/// Where the list is read from, in the order the sources are tried: the `Servers.xml` `named`
/// alone when one is named, and otherwise the hub's list, then the community's own. An empty name
/// names none.
pub fn sources(named: Option<&str>) -> Vec<ListSource> {
    sources_from(named, None)
}

/// [`sources`], with the list of another hub than the network's (`hub`, the address of its
/// `/v1/servers`: a hub of one's own, such as a release's trial on a copy of the registry) read in
/// place of the network's hub. An empty address names none.
pub fn sources_from(named: Option<&str>, hub: Option<&str>) -> Vec<ListSource> {
    let source = |url: &str, format, empty_is_an_answer| ListSource {
        url: url.to_owned(),
        format,
        empty_is_an_answer,
    };
    let hub = hub
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .unwrap_or(HUB_LIST_URL);
    match named.map(str::trim).filter(|n| !n.is_empty()) {
        Some(named) => vec![source(named, ListFormat::ServersXml, true)],
        None => vec![
            source(hub, ListFormat::Hub, false),
            source(COMMUNITY_LIST_URL, ListFormat::ServersXml, false),
        ],
    }
}

/// The address of the page of the hub's list `base` that starts at `offset`.
fn hub_page_url(base: &str, offset: u64) -> String {
    let join = if base.contains('?') { '&' } else { '?' };
    format!("{base}{join}limit={HUB_PAGE_SIZE}&offset={offset}")
}

/// Reading the list: each source in turn until one answers with its whole list, the hub's a page
/// at a time. Whoever reads it fetches each address [`ListFetch::next_url`] names and hands back
/// what came ([`ListFetch::take`]), so the desktop launcher and the browser read it alike.
#[derive(Debug, Clone)]
pub struct ListFetch {
    sources: Vec<ListSource>,
    /// The source being read.
    at: usize,
    /// What it has answered so far, a page at a time.
    pages: Vec<String>,
    /// Where its latest page starts, for the hub.
    offset: u64,
    /// The address to fetch next; `None` once a list is read whole or every source has failed.
    next: Option<String>,
    /// Why each source passed over did not answer with its list.
    failures: Vec<String>,
}

impl ListFetch {
    /// Start reading from `sources`, in order.
    pub fn new(sources: Vec<ListSource>) -> Self {
        let mut fetch = Self {
            sources,
            at: 0,
            pages: Vec::new(),
            offset: 0,
            next: None,
            failures: Vec::new(),
        };
        fetch.next = fetch.first_url();
        fetch
    }

    /// The first address of the source being read.
    fn first_url(&self) -> Option<String> {
        let source = self.sources.get(self.at)?;
        Some(match source.format {
            ListFormat::Hub => hub_page_url(&source.url, 0),
            ListFormat::ServersXml => source.url.clone(),
        })
    }

    /// The address to fetch next; `None` once a list is read whole or every source has failed.
    pub fn next_url(&self) -> Option<&str> {
        self.next.as_deref()
    }

    /// The source being read, or the one that answered.
    pub fn source(&self) -> Option<&ListSource> {
        self.sources.get(self.at)
    }

    /// What fetching [`Self::next_url`] brought: its body, or why there is none (no answer, an
    /// error status). A body that does not read as the source's list fails the source as no answer
    /// does, and so does a whole list that shows no world from a source that does not take one
    /// ([`ListSource::empty_is_an_answer`]); the next source is then read from its start.
    pub fn take(&mut self, fetched: Result<Vec<u8>, String>) {
        let Some(url) = self.next.take() else {
            return;
        };
        let read = fetched
            .and_then(|body| self.keep(body).map_err(|e| format!("{url}: {e}")))
            .and_then(|next| {
                if next.is_none() && self.shows_no_world() {
                    return Err(format!("{url}: the world list shows no world"));
                }
                Ok(next)
            });
        match read {
            Ok(next) => self.next = next,
            Err(e) => {
                self.failures.push(e);
                self.pages.clear();
                self.offset = 0;
                self.at += 1;
                self.next = self.first_url();
            }
        }
    }

    /// Whether the source being read, read whole, shows no world where it must show one.
    fn shows_no_world(&self) -> bool {
        let source = &self.sources[self.at];
        if source.empty_is_an_answer {
            return false;
        }
        let worlds = match source.format {
            ListFormat::Hub => parse_hub_pages(&self.pages),
            ListFormat::ServersXml => self
                .pages
                .first()
                .map_or(Ok(Vec::new()), |list| parse_servers_xml(list.as_bytes())),
        };
        worlds.map_or(true, |w| w.is_empty())
    }

    /// Keep a page of the source being read; answers the address of its next page, if it has one.
    fn keep(&mut self, body: Vec<u8>) -> Result<Option<String>, MalformedList> {
        let text = String::from_utf8(body).map_err(|e| MalformedList(e.to_string()))?;
        let source = &self.sources[self.at];
        let next = match source.format {
            ListFormat::ServersXml => {
                parse_servers_xml(text.as_bytes())?;
                None
            }
            ListFormat::Hub => match hub_page(&text)?.1 {
                Some(next) if next <= self.offset => {
                    return Err(MalformedList("the next page starts no further on".into()));
                }
                Some(next) if self.pages.len() + 1 < HUB_MAX_PAGES => {
                    self.offset = next;
                    Some(hub_page_url(&source.url, next))
                }
                _ => None,
            },
        };
        self.pages.push(text);
        Ok(next)
    }

    /// The list read, as the day's copy fetched at `now`, and the worlds it holds.
    ///
    /// # Errors
    /// Each source's failure, in order, when none answered with its list.
    pub fn finish(self, now: u64) -> Result<(ListCache, Vec<World>), String> {
        if self.next.is_some() {
            return Err("the world list was not read to its end".into());
        }
        let Some(source) = self.sources.get(self.at) else {
            if self.failures.is_empty() {
                return Err("no world list is named".into());
            }
            return Err(self.failures.join("; "));
        };
        let sources = source_urls(&self.sources);
        let cache = match source.format {
            ListFormat::Hub => ListCache {
                fetched_at: now,
                list: String::new(),
                hub: self.pages,
                sources,
            },
            ListFormat::ServersXml => ListCache {
                fetched_at: now,
                list: self.pages.into_iter().next().unwrap_or_default(),
                hub: Vec::new(),
                sources,
            },
        };
        let worlds = cache.worlds().map_err(|e| format!("{}: {e}", source.url))?;
        Ok((cache, worlds))
    }
}

/// The list from the first of `sources` that answers with its list, as the day's copy fetched at
/// `now` and the worlds it holds. `get` fetches an address: its body, or why there is none.
///
/// # Errors
/// Each source's failure, in order, when none answers with its list.
pub fn fetch_first<F>(
    sources: &[ListSource],
    now: u64,
    mut get: F,
) -> Result<(ListCache, Vec<World>), String>
where
    F: FnMut(&str) -> Result<Vec<u8>, String>,
{
    let mut fetch = ListFetch::new(sources.to_vec());
    while let Some(url) = fetch.next_url().map(str::to_owned) {
        fetch.take(get(&url));
    }
    fetch.finish(now)
}

/// The day's copy, in the launcher's data folder.
pub const CACHE_FILE: &str = "world-list.json";

/// How long a copy is used before the list is fetched again: a day.
pub const MAX_AGE_SECS: u64 = 24 * 60 * 60;

/// Why the list could not be read at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MalformedList(pub String);

impl core::fmt::Display for MalformedList {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "the world list is unreadable: {}", self.0)
    }
}

impl std::error::Error for MalformedList {}

/// A world's identity from its name: lower case, every run of other characters one dash, none at
/// either end (`Asheron4Fun.com` is `asheron4fun-com`). Accounts, passwords and remembered choices
/// are kept under it. It is the spelling the launcher's earlier world source used, so what was kept
/// under a world before stays with it.
pub fn slug_for(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if dash && !out.is_empty() {
                out.push('-');
            }
            dash = false;
            out.push(c.to_ascii_lowercase());
        } else {
            dash = true;
        }
    }
    out
}

/// Read `Servers.xml`: its worlds, in the list's order.
///
/// # Errors
/// [`MalformedList`] when the body is not text or holds no `ArrayOfServerItem`.
pub fn parse_servers_xml(body: &[u8]) -> Result<Vec<World>, MalformedList> {
    let text = std::str::from_utf8(body).map_err(|e| MalformedList(e.to_string()))?;
    let text = text.trim_start_matches('\u{feff}');
    let text = strip_comments(text);
    if !text.contains("<ArrayOfServerItem") {
        return Err(MalformedList("no ArrayOfServerItem".into()));
    }
    let mut taken = HashSet::new();
    Ok(elements(&text, "ServerItem")
        .into_iter()
        .filter_map(|item| world_of(&children(item), &mut taken))
        .collect())
}

fn world_of(row: &BTreeMap<String, String>, taken: &mut HashSet<String>) -> Option<World> {
    let field = |k: &str| row.get(k).map(|s| s.trim()).filter(|s| !s.is_empty());
    let name = field("name")?;
    let id = field("id").map(str::to_owned);
    let slug = take_slug(name, id.as_deref(), taken);

    let mut w = World::new(slug, name);
    w.list_id = id;
    w.description = field("description").map(str::to_owned);
    w.emulator = field("emu").map(Emulator::parse).unwrap_or_default();
    w.ruleset = field("type").map(str::to_owned);
    w.development_status = field("status").map(str::to_owned);
    let port = field("server_port")
        .and_then(|p| p.parse::<u16>().ok())
        .filter(|p| *p > 0);
    if let (Some(address), Some(port)) = (field("server_host"), port) {
        w.endpoint = Some(Endpoint {
            address: address.to_owned(),
            port,
            transport: None,
        });
    }
    w.links = Links {
        website: field("website_url").map(str::to_owned),
        discord: field("discord_url").map(str::to_owned),
        ..Links::default()
    };
    // What the list cannot say about the few worlds that run a client of their own.
    crate::known::apply(&mut w);
    Some(w)
}

/// The key a world named `name` is kept under before it is told apart from another of the same
/// spelling: [`slug_for`] its name, or `fallback` (its id) for a name with no letter or digit in
/// it.
fn key_for(name: &str, fallback: Option<&str>) -> String {
    let slug = slug_for(name);
    if slug.is_empty() {
        fallback.unwrap_or("world").to_owned()
    } else {
        slug
    }
}

/// The key a world named `name` is kept under: [`key_for`] it. Two names with one spelling: the
/// later one is told apart by a number.
fn take_slug(name: &str, fallback: Option<&str>, taken: &mut HashSet<String>) -> String {
    let mut slug = key_for(name, fallback);
    if taken.contains(&slug) {
        let base = slug.clone();
        slug = (2u32..)
            .map(|n| format!("{base}-{n}"))
            .find(|s| !taken.contains(s))
            .expect("some slug is free");
    }
    taken.insert(slug.clone());
    slug
}

// ----- the hub's list ------------------------------------------------------------------------------

/// One page of the hub's list: its rows, and where the next page starts if there is one.
fn hub_page(text: &str) -> Result<(Vec<Value>, Option<u64>), MalformedList> {
    let mut page: Value = serde_json::from_str(text).map_err(|e| MalformedList(e.to_string()))?;
    let next = page.get("nextOffset").and_then(Value::as_u64);
    match page.get_mut("data").map(Value::take) {
        Some(Value::Array(rows)) => Ok((rows, next)),
        _ => Err(MalformedList("no list of servers".into())),
    }
}

/// The rows of the hub's list, its pages in order, each world once: a world that moved from one
/// page to the next while they were read is read where it was first seen.
///
/// # Errors
/// [`MalformedList`] when a page is not a page of the list.
pub(crate) fn hub_rows(pages: &[String]) -> Result<Vec<Value>, MalformedList> {
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    for page in pages {
        for row in hub_page(page)?.0 {
            let hub_slug = row.get("slug").and_then(Value::as_str).map(str::to_owned);
            if hub_slug.is_some_and(|s| !seen.insert(s)) {
                continue;
            }
            rows.push(row);
        }
    }
    Ok(rows)
}

/// Read the hub's list, its pages in order: the worlds to show, in its order.
///
/// # Errors
/// [`MalformedList`] when a page is not a page of the list.
pub fn parse_hub_pages(pages: &[String]) -> Result<Vec<World>, MalformedList> {
    let mut taken = HashSet::new();
    Ok(hub_rows(pages)?
        .iter()
        .filter_map(|row| hub_world(row, &mut taken))
        .collect())
}

/// A text field of a hub row, at the JSON pointer `at`; `None` when it is missing or blank.
fn hub_text<'a>(row: &'a Value, at: &str) -> Option<&'a str> {
    row.pointer(at)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// The name of the world a hub row shows, with the id to key it by when the name has no letter or
/// digit in it, and the address and port to connect to. `None` for a row the launcher does not
/// show: one with no name, and one with no address and port.
fn hub_shown(row: &Value) -> Option<(&str, Option<&str>, &str, u16)> {
    let name = hub_text(row, "/name")?;
    let address = hub_text(row, "/endpoint/address")?;
    let port = row
        .pointer("/endpoint/port")
        .and_then(Value::as_u64)
        .and_then(|p| u16::try_from(p).ok())
        .filter(|p| *p > 0)?;
    // The community list's id, which the hub keeps for a world it imported from it; else the
    // hub's slug.
    let fallback = hub_text(row, "/provenance/externalId").or(hub_text(row, "/slug"));
    Some((name, fallback, address, port))
}

/// The key the world a hub row shows is kept under before it is told apart from a namesake
/// ([`key_for`] its name); `None` for a row the launcher does not show.
pub(crate) fn hub_key(row: &Value) -> Option<String> {
    let (name, fallback, ..) = hub_shown(row)?;
    Some(key_for(name, fallback))
}

/// One world of the hub's list, read as the same world is from `Servers.xml`. `None` for a row
/// with no name, and one with no address and port to connect to.
fn hub_world(row: &Value, taken: &mut HashSet<String>) -> Option<World> {
    let text = |at: &str| hub_text(row, at);
    let (name, fallback, address, port) = hub_shown(row)?;
    // The community list's id, which the hub keeps for a world it imported from it.
    let list_id = text("/provenance/externalId").map(str::to_owned);
    // Kept under its name, not the hub's slug, as the community list's world is.
    let slug = take_slug(name, fallback, taken);

    let mut w = World::new(slug, name);
    w.list_id = list_id;
    w.hub_slug = text("/slug").map(str::to_owned);
    w.description = text("/description").map(str::to_owned);
    w.emulator = text("/emulator/family")
        .map(Emulator::parse)
        .unwrap_or_default();
    w.emulator_version = text("/emulator/version").map(str::to_owned);
    // The community list's own word for the rules, which the hub keeps beside its own.
    w.ruleset = text("/provenance/legacyType")
        .or_else(|| text("/ruleset"))
        .map(str::to_owned);
    w.development_status = text("/maturity")
        .filter(|m| !m.eq_ignore_ascii_case("Unspecified"))
        .map(str::to_owned);
    // A world the hub lists for demonstration says so where its status is shown.
    if row.get("isDemo").and_then(Value::as_bool) == Some(true) {
        w.development_status = Some(TEST_WORLD.to_owned());
    }
    w.endpoint = Some(Endpoint {
        address: address.to_owned(),
        port,
        transport: None,
    });
    w.links = Links {
        website: text("/website").map(str::to_owned),
        discord: text("/discord").map(str::to_owned),
        ..Links::default()
    };
    (w.websocket_url, w.status_url) = hub_web_addresses(row);
    crate::known::apply(&mut w);
    Some(w)
}

/// Where a browser reaches the world a hub row shows, as the hub names it while the world is
/// linked: its WebSocket endpoint and its status document, each only within the rule a launcher
/// connects by ([`crate::direct`]).
pub(crate) fn hub_web_addresses(row: &Value) -> (Option<String>, Option<String>) {
    let url =
        |at: &str, ok: fn(&str) -> bool| hub_text(row, at).filter(|u| ok(u)).map(str::to_owned);
    (
        url("/websocketUrl", crate::direct::is_websocket_url),
        url("/statusUrl", crate::direct::is_status_url),
    )
}

/// The addresses of `sources`, in order: what a copy of the list read from them records.
pub fn source_urls(sources: &[ListSource]) -> Vec<String> {
    sources.iter().map(|s| s.url.clone()).collect()
}

/// The day's copy: the list as it was fetched, when, and from which sources.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ListCache {
    /// Seconds since the epoch.
    pub fetched_at: u64,
    /// `Servers.xml`, as fetched, when the list was one; empty when it was the hub's.
    #[serde(default)]
    pub list: String,
    /// The hub's pages, as fetched, when the list was the hub's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hub: Vec<String>,
    /// The addresses of the sources the list was read from, in the order they were tried
    /// ([`source_urls`]): the copy stands in for a list read from those sources alone. A copy kept
    /// before copies recorded them records none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
}

impl ListCache {
    /// Whether the copy is the hub's list.
    pub fn is_hubs(&self) -> bool {
        !self.hub.is_empty()
    }

    /// Whether the copy was read from `sources`, and so may stand in for reading them: a copy of
    /// another hub's list, or of a list the player named, is not the list these sources give.
    pub fn is_from(&self, sources: &[ListSource]) -> bool {
        self.sources == source_urls(sources)
    }

    /// Whether the copy is less than a day old. A copy from the future (a clock set back) is not.
    pub fn is_fresh(&self, now: u64) -> bool {
        now >= self.fetched_at && now - self.fetched_at < MAX_AGE_SECS
    }

    /// The copy in `dir`, if there is a readable one.
    pub fn load(dir: &Path) -> Option<Self> {
        let bytes = std::fs::read(dir.join(CACHE_FILE)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Write the copy into `dir`, atomically.
    ///
    /// # Errors
    /// Any failure to create the folder or write the file.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let json = serde_json::to_vec(self).map_err(io::Error::other)?;
        let tmp = dir.join(format!("{CACHE_FILE}.tmp"));
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, dir.join(CACHE_FILE))
    }

    /// The worlds the copy holds.
    ///
    /// # Errors
    /// [`MalformedList`] when the list cannot be read.
    pub fn worlds(&self) -> Result<Vec<World>, MalformedList> {
        if self.hub.is_empty() {
            parse_servers_xml(self.list.as_bytes())
        } else {
            parse_hub_pages(&self.hub)
        }
    }
}

// ----- the small XML reader ------------------------------------------------------------------------

/// The text with every `<!-- ... -->` taken out.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("<!--") {
        out.push_str(&rest[..at]);
        rest = rest[at..]
            .find("-->")
            .map_or("", |end| &rest[at + end + 3..]);
    }
    out.push_str(rest);
    out
}

/// Where the tag `name` opens at `at` in `text`: `<name>`, `<name ...>` or `<name/>`, and not a
/// longer name that begins the same way.
fn opens(text: &str, at: usize, name: &str) -> bool {
    text[at..].starts_with('<')
        && text[at + 1..].starts_with(name)
        && text[at + 1 + name.len()..]
            .chars()
            .next()
            .is_some_and(|c| c == '>' || c == '/' || c.is_whitespace())
}

/// The inner text of every `<name>...</name>` in `text`, in order. A self-closed `<name/>` is empty.
fn elements<'a>(text: &'a str, name: &str) -> Vec<&'a str> {
    let close = format!("</{name}>");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = text[from..].find('<') {
        let at = from + rel;
        if !opens(text, at, name) {
            from = at + 1;
            continue;
        }
        let Some(gt) = text[at..].find('>').map(|g| at + g) else {
            break;
        };
        if text[..gt].ends_with('/') {
            out.push("");
            from = gt + 1;
            continue;
        }
        let Some(end) = text[gt + 1..].find(&close).map(|e| gt + 1 + e) else {
            break;
        };
        out.push(&text[gt + 1..end]);
        from = end + close.len();
    }
    out
}

/// Every child element of an item, by name, with its text decoded. Children are text only in this
/// list, so a child's inner markup is not looked into.
fn children(item: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut from = 0;
    while let Some(rel) = item[from..].find('<') {
        let at = from + rel;
        let tag: String = item[at + 1..]
            .chars()
            .take_while(|c| !(c.is_whitespace() || *c == '>' || *c == '/'))
            .collect();
        if tag.is_empty() || tag.starts_with(['!', '?']) {
            from = at + 1;
            continue;
        }
        let Some(gt) = item[at..].find('>').map(|g| at + g) else {
            break;
        };
        if item[..gt].ends_with('/') {
            out.entry(tag).or_insert_with(String::new);
            from = gt + 1;
            continue;
        }
        let close = format!("</{tag}>");
        let Some(end) = item[gt + 1..].find(&close).map(|e| gt + 1 + e) else {
            break;
        };
        out.entry(tag).or_insert_with(|| decode(&item[gt + 1..end]));
        from = end + close.len();
    }
    out
}

/// Character data: CDATA sections as they are, and the five named entities and numeric character
/// references decoded. An entity that is none of these is left as written.
fn decode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<![CDATA[") {
            let end = after.find("]]>").unwrap_or(after.len());
            out.push_str(&after[..end]);
            rest = after.get(end + 3..).unwrap_or("");
            continue;
        }
        let c = rest.chars().next().expect("not empty");
        if c == '&' {
            if let Some(semi) = rest[..rest.len().min(12)].find(';') {
                let entity = &rest[1..semi];
                let decoded = match entity {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    _ => entity
                        .strip_prefix("#x")
                        .or_else(|| entity.strip_prefix("#X"))
                        .and_then(|h| u32::from_str_radix(h, 16).ok())
                        .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                        .and_then(char::from_u32),
                };
                if let Some(d) = decoded {
                    out.push(d);
                    rest = &rest[semi + 1..];
                    continue;
                }
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the launcher's world list: where it is read from, its readers and its copy).
    use super::*;
    use crate::world::WorldState;

    /// Two rows as the list publishes them, one with an entity, one with its links left out.
    const LIST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<ArrayOfServerItem>
  <!-- <ServerItem><name>Commented out</name></ServerItem> -->
  <ServerItem>
    <id>9d17ce44-7db5-40c1-b7bb-a01c9de21d00</id>
    <name>AChard</name>
    <description>ACE EOR PVP Server &amp; PK rankings</description>
    <emu>ACE</emu>
    <server_host>a-chard.ddns.net</server_host>
    <server_port>9000</server_port>
    <type>PvP</type>
    <status>Stable</status>
    <website_url>http://a-chard.ddns.net/</website_url>
    <discord_url></discord_url>
  </ServerItem>
  <ServerItem>
    <id>b5bde7a5-537c-4665-8714-b7b097e73964</id>
    <name>Asheron4Fun.com</name>
    <description><![CDATA[Five <worlds>]]></description>
    <emu>GDL</emu>
    <server_host>play.a4f.example</server_host>
    <server_port>not a port</server_port>
    <type>PvE</type>
    <status>Development</status>
    <website_url/>
  </ServerItem>
  <ServerItem><description>no name</description></ServerItem>
</ArrayOfServerItem>"#;

    #[test]
    fn the_community_list_reads_as_worlds_with_slugs_from_their_names() {
        let w = parse_servers_xml(LIST.as_bytes()).unwrap();
        assert_eq!(
            w.len(),
            2,
            "the commented row and the nameless row are not worlds"
        );
        let a = &w[0];
        assert_eq!((a.slug.as_str(), a.name.as_str()), ("achard", "AChard"));
        assert_eq!(
            a.list_id.as_deref(),
            Some("9d17ce44-7db5-40c1-b7bb-a01c9de21d00")
        );
        assert_eq!(
            a.description.as_deref(),
            Some("ACE EOR PVP Server & PK rankings")
        );
        assert_eq!(a.emulator, Emulator::Ace);
        assert_eq!(a.ruleset.as_deref(), Some("PvP"));
        assert_eq!(a.development_status.as_deref(), Some("Stable"));
        assert_eq!(
            a.endpoint.as_ref().map(|e| (e.address.as_str(), e.port)),
            Some(("a-chard.ddns.net", 9000))
        );
        assert_eq!(a.links.website.as_deref(), Some("http://a-chard.ddns.net/"));
        assert_eq!(a.links.discord, None, "an empty link is no link");
        assert_eq!(a.era, None, "the list never names an era");

        let b = &w[1];
        assert_eq!(b.slug, "asheron4fun-com");
        assert_eq!(b.emulator, Emulator::Gdle, "the list spells GDLE as GDL");
        assert_eq!(b.description.as_deref(), Some("Five <worlds>"));
        assert_eq!(b.endpoint, None, "a port that is no number is no endpoint");
        assert_eq!(b.links.website, None);
    }

    #[test]
    fn a_listed_world_the_launcher_knows_is_read_with_its_logon_files_and_rules() {
        let xml = "<ArrayOfServerItem><ServerItem>\
                   <id>3f1f41ec-c7fd-4ed9-b47d-25b0d94219c1</id><name>Unfamiliar Shores</name>\
                   <emu>ACE</emu><server_host>192.0.2.7</server_host><server_port>9000</server_port>\
                   </ServerItem><ServerItem><id>394C58D0-885D-466B-B17F-D7E0B96FE3E2</id>\
                   <name>Seedsow</name><emu>GDL</emu></ServerItem></ArrayOfServerItem>";
        let w = parse_servers_xml(xml.as_bytes()).unwrap();
        assert_eq!(
            w[0].emulator,
            Emulator::ClassicAce,
            "the list says plain ACE"
        );
        assert_eq!(w[0].logon_version.as_deref(), Some("c118"));
        assert_eq!(w[0].world_profile.as_deref(), Some("classicace-customdm"));
        assert_eq!(w[0].era.as_deref(), Some("infiltration"));
        assert!(w[0].dats.custom.is_some());
        assert_eq!(w[1].world_profile.as_deref(), Some("classicdereth"));
        assert_eq!(w[1].logon_version.as_deref(), Some("1802"));
    }

    #[test]
    fn another_hubs_list_is_read_in_place_of_the_networks_and_a_named_list_alone_still() {
        let urls = |s: Vec<ListSource>| s.into_iter().map(|s| s.url).collect::<Vec<_>>();
        assert_eq!(
            urls(sources_from(
                None,
                Some("https://dev-hub.example.org/v1/servers")
            )),
            ["https://dev-hub.example.org/v1/servers", COMMUNITY_LIST_URL]
        );
        assert_eq!(
            urls(sources_from(None, Some(" "))),
            [HUB_LIST_URL, COMMUNITY_LIST_URL]
        );
        assert_eq!(sources_from(None, None), sources(None));
        assert_eq!(
            urls(sources_from(
                Some("https://x.example/Servers.xml"),
                Some("https://dev")
            )),
            ["https://x.example/Servers.xml"]
        );
    }

    #[test]
    fn slugs_follow_the_name_and_two_alike_are_told_apart() {
        assert_eq!(slug_for("Buadren AC"), "buadren-ac");
        assert_eq!(slug_for("  -Frost--Fell!  "), "frost-fell");
        let xml = "<ArrayOfServerItem><ServerItem><name>Twin</name></ServerItem>\
                   <ServerItem><name>twin</name></ServerItem>\
                   <ServerItem><id>abc</id><name>!!!</name></ServerItem></ArrayOfServerItem>";
        let w = parse_servers_xml(xml.as_bytes()).unwrap();
        let slugs: Vec<_> = w.iter().map(|w| w.slug.as_str()).collect();
        assert_eq!(slugs, ["twin", "twin-2", "abc"]);
    }

    #[test]
    fn something_that_is_not_the_list_is_an_error() {
        assert!(parse_servers_xml(b"<html></html>").is_err());
        assert!(parse_servers_xml(&[0xff, 0xfe, 0x00]).is_err());
        assert_eq!(
            parse_servers_xml(b"<ArrayOfServerItem/>").unwrap(),
            Vec::<World>::new()
        );
    }

    /// A `Servers.xml` of one world, named `name`.
    fn list_of(name: &str) -> Vec<u8> {
        format!(
            "<ArrayOfServerItem><ServerItem><name>{name}</name></ServerItem></ArrayOfServerItem>"
        )
        .into_bytes()
    }

    /// A row of the hub's list as `/v1/servers` gives it: the world `name`, which the hub lists as
    /// `slug`, imported from no list.
    fn hub_row(slug: &str, name: &str) -> Value {
        serde_json::json!({
            "slug": slug, "name": name, "description": "", "ruleset": "PvE",
            "maturity": "Unspecified", "region": "Unspecified", "discord": null,
            "emulator": {"family": "ACE"}, "software": "ACE", "isDemo": false, "federation": "unset",
            "endpoint": {"mode": "manual", "address": format!("{slug}.example"), "port": 9000,
                         "transport": "udp", "addressFamily": null, "source": "operator"},
            "ip": null, "port": 9000, "provenance": null,
            "status": {"state": "unknown", "playersOnline": null, "lastSeenAt": null}
        })
    }

    /// A page of the hub's list holding `rows`, with the next page starting at `next`.
    fn hub_page_of(rows: &[Value], next: Option<u64>) -> Vec<u8> {
        serde_json::json!({"data": rows, "limit": 100, "offset": 0, "nextOffset": next,
                           "total": rows.len()})
        .to_string()
        .into_bytes()
    }

    /// What a fetch read: the day's copy and its worlds, or why no source answered with its list.
    type Read = Result<(ListCache, Vec<World>), String>;

    /// Fetch from `sources` with stand-in answers, one per address; an address without one does
    /// not answer. Answers what was read and every address asked, in order.
    fn fetch_with(
        sources: &[ListSource],
        answers: &[(&str, Result<Vec<u8>, &str>)],
    ) -> (Read, Vec<String>) {
        let mut asked = Vec::new();
        let read = fetch_first(sources, 7, |url| {
            asked.push(url.to_owned());
            answers.iter().find(|(at, _)| *at == url).map_or_else(
                || Err(format!("{url}: no answer")),
                |(_, a)| a.clone().map_err(|e| format!("{url}: {e}")),
            )
        });
        (read, asked)
    }

    fn names(worlds: &[World]) -> Vec<&str> {
        worlds.iter().map(|w| w.name.as_str()).collect()
    }

    #[test]
    fn the_hubs_list_is_read_first_a_page_at_a_time_and_alone_when_it_answers() {
        let order: Vec<_> = sources(None)
            .into_iter()
            .map(|s| (s.url, s.format))
            .collect();
        assert_eq!(
            order,
            [
                (HUB_LIST_URL.to_owned(), ListFormat::Hub),
                (COMMUNITY_LIST_URL.to_owned(), ListFormat::ServersXml)
            ]
        );
        let first = hub_page_url(HUB_LIST_URL, 0);
        let second = hub_page_url(HUB_LIST_URL, 100);
        assert_eq!(
            first,
            "https://hub.dereth.network/v1/servers?limit=100&offset=0"
        );
        let (read, asked) = fetch_with(
            &sources(None),
            &[
                (
                    &first,
                    Ok(hub_page_of(&[hub_row("coldeve", "Coldeve")], Some(100))),
                ),
                (
                    &second,
                    Ok(hub_page_of(&[hub_row("frost", "Frostfell")], None)),
                ),
                (COMMUNITY_LIST_URL, Ok(list_of("Community World"))),
            ],
        );
        let (cache, worlds) = read.unwrap();
        assert_eq!(asked, [first, second], "the community list is not asked");
        assert_eq!(names(&worlds), ["Coldeve", "Frostfell"]);
        assert_eq!(cache.fetched_at, 7);
        assert_eq!(cache.hub.len(), 2, "the copy is the hub's pages");
        assert!(cache.list.is_empty());
        assert_eq!(cache.worlds().unwrap(), worlds);
        assert_eq!(
            cache.sources,
            [HUB_LIST_URL, COMMUNITY_LIST_URL],
            "the copy records the sources it was read from"
        );
    }

    #[test]
    fn a_copy_is_from_the_sources_it_was_read_from_and_no_others() {
        const DEV: &str = "https://dev-hub.example.org/v1/servers";
        const NAMED: &str = "https://lists.example/Servers.xml";
        let read = |from: &[ListSource]| {
            let answers: Vec<(String, Result<Vec<u8>, &str>)> = vec![
                (
                    hub_page_url(HUB_LIST_URL, 0),
                    Ok(hub_page_of(&[hub_row("coldeve", "Coldeve")], None)),
                ),
                (
                    hub_page_url(DEV, 0),
                    Ok(hub_page_of(&[hub_row("devworld", "Dev World")], None)),
                ),
                (NAMED.to_owned(), Ok(list_of("Named World"))),
            ];
            let answers: Vec<(&str, Result<Vec<u8>, &str>)> = answers
                .iter()
                .map(|(u, a)| (u.as_str(), a.clone()))
                .collect();
            fetch_with(from, &answers).0.unwrap().0
        };
        let usual = sources(None);
        let dev = sources_from(None, Some(DEV));
        let named = sources(Some(NAMED));
        let from_usual = read(&usual);
        let from_dev = read(&dev);
        let from_named = read(&named);
        assert!(from_usual.is_from(&usual) && from_dev.is_from(&dev) && from_named.is_from(&named));
        assert!(
            !from_usual.is_from(&dev),
            "the network's list is not the dev hub's"
        );
        assert!(!from_dev.is_from(&usual), "nor the other way about");
        assert!(!from_named.is_from(&usual) && !from_usual.is_from(&named));
        assert_eq!(from_dev.sources, [DEV, COMMUNITY_LIST_URL]);
        // A copy kept before copies said where they came from is from nowhere it can be told.
        let older: ListCache =
            serde_json::from_value(serde_json::json!({"fetched_at": 7, "list": LIST})).unwrap();
        assert!(older.sources.is_empty() && !older.is_from(&usual));
    }

    #[test]
    fn a_hub_that_does_not_answer_with_its_list_falls_back_to_the_community_list() {
        let first = hub_page_url(HUB_LIST_URL, 0);
        let second = hub_page_url(HUB_LIST_URL, 100);
        let one = [hub_row("coldeve", "Coldeve")];
        let failures: [(Result<Vec<u8>, &str>, bool); 7] = [
            (Err("connection refused"), false),
            (Err("http status: 503"), false),
            (Ok(b"<html><body>Bad gateway</body></html>".to_vec()), false),
            (Ok(vec![0xff, 0xfe, 0x00]), false),
            (Ok(br#"{"error":{"code":"INTERNAL"}}"#.to_vec()), false),
            (Ok(hub_page_of(&one, Some(0))), false),
            // The first page, and no second.
            (Ok(hub_page_of(&one, Some(100))), true),
        ];
        for (hub, paged) in failures {
            let (read, asked) = fetch_with(
                &sources(None),
                &[
                    (&first, hub.clone()),
                    (&second, Err("timed out")),
                    (COMMUNITY_LIST_URL, Ok(list_of("Community World"))),
                ],
            );
            let (cache, worlds) = read.unwrap_or_else(|e| panic!("{hub:?}: {e}"));
            let mut expected = vec![first.as_str()];
            if paged {
                expected.push(&second);
            }
            expected.push(COMMUNITY_LIST_URL);
            assert_eq!(asked, expected, "{hub:?}");
            assert_eq!(names(&worlds), ["Community World"], "{hub:?}");
            assert_eq!(cache.list.as_bytes(), list_of("Community World"));
            assert!(
                cache.hub.is_empty(),
                "{hub:?}: nothing of the hub's is kept"
            );
        }
    }

    #[test]
    fn when_neither_list_answers_the_failure_names_both() {
        let first = hub_page_url(HUB_LIST_URL, 0);
        let (read, asked) = fetch_with(
            &sources(None),
            &[
                (&first, Err("timed out")),
                (COMMUNITY_LIST_URL, Ok(b"<html/>".to_vec())),
            ],
        );
        assert_eq!(asked, [first.as_str(), COMMUNITY_LIST_URL]);
        let e = read.unwrap_err();
        assert!(
            e.contains(&format!("{first}: timed out"))
                && e.contains(&format!(
                    "{COMMUNITY_LIST_URL}: the world list is unreadable"
                )),
            "{e}"
        );
    }

    #[test]
    fn a_hub_list_that_shows_no_world_falls_back_and_an_empty_fallback_is_no_answer() {
        let first = hub_page_url(HUB_LIST_URL, 0);
        let mut unseen = hub_row("unseen", "Not Yet Seen");
        unseen["endpoint"]["address"] = Value::Null;
        // No world at all, and none but one that is not shown.
        for hub in [hub_page_of(&[], None), hub_page_of(&[unseen], None)] {
            let (read, asked) = fetch_with(
                &sources(None),
                &[
                    (&first, Ok(hub.clone())),
                    (COMMUNITY_LIST_URL, Ok(list_of("Community World"))),
                ],
            );
            assert_eq!(asked, [first.as_str(), COMMUNITY_LIST_URL]);
            let (cache, worlds) = read.unwrap();
            assert_eq!(names(&worlds), ["Community World"]);
            assert!(cache.hub.is_empty(), "the empty list is not the copy");
        }
        // A list of test worlds alone is not empty.
        let mut demo = hub_row("demo", "Demonstration");
        demo["isDemo"] = true.into();
        let (read, asked) = fetch_with(
            &sources(None),
            &[
                (&first, Ok(hub_page_of(&[demo], None))),
                (COMMUNITY_LIST_URL, Ok(list_of("Community World"))),
            ],
        );
        assert_eq!(asked, [first.as_str()]);
        assert_eq!(names(&read.unwrap().1), ["Demonstration"]);
        // The community list showing none either is no answer: nothing to put in place of the
        // worlds on screen or the day's copy.
        let (read, _) = fetch_with(
            &sources(None),
            &[
                (&first, Ok(hub_page_of(&[], None))),
                (COMMUNITY_LIST_URL, Ok(b"<ArrayOfServerItem/>".to_vec())),
            ],
        );
        let e = read.unwrap_err();
        assert!(
            e.contains(&format!("{first}: the world list shows no world"))
                && e.contains(&format!(
                    "{COMMUNITY_LIST_URL}: the world list shows no world"
                )),
            "{e}"
        );
        // A list the player names is taken as it comes.
        const NAMED: &str = "https://lists.example/Servers.xml";
        let (read, asked) = fetch_with(
            &sources(Some(NAMED)),
            &[(NAMED, Ok(b"<ArrayOfServerItem/>".to_vec()))],
        );
        assert_eq!(asked, [NAMED]);
        let (cache, worlds) = read.unwrap();
        assert!(worlds.is_empty());
        assert_eq!(cache.list, "<ArrayOfServerItem/>");
    }

    #[test]
    fn a_named_list_is_read_alone_and_never_falls_back() {
        const NAMED: &str = "https://lists.example/Servers.xml";
        let named = |n: &str| {
            sources(Some(n))
                .into_iter()
                .map(|s| (s.url, s.format))
                .collect::<Vec<_>>()
        };
        assert_eq!(named(NAMED), [(NAMED.to_owned(), ListFormat::ServersXml)]);
        assert_eq!(named(&format!("  {NAMED}\n")), named(NAMED));
        assert_eq!(
            sources(Some("  ")),
            sources(None),
            "an empty name names no list"
        );

        let (read, asked) = fetch_with(
            &sources(Some(NAMED)),
            &[(NAMED, Ok(list_of("Named World")))],
        );
        assert_eq!(names(&read.unwrap().1), ["Named World"]);
        assert_eq!(asked, [NAMED]);

        let hub = hub_page_url(HUB_LIST_URL, 0);
        for answer in [Err("connection refused"), Ok(b"<html/>".to_vec())] {
            let (read, asked) = fetch_with(
                &sources(Some(NAMED)),
                &[
                    (NAMED, answer.clone()),
                    (
                        &hub,
                        Ok(hub_page_of(&[hub_row("coldeve", "Coldeve")], None)),
                    ),
                    (COMMUNITY_LIST_URL, Ok(list_of("Community World"))),
                ],
            );
            assert_eq!(asked, [NAMED], "{answer:?}: no other list is asked");
            assert!(
                read.unwrap_err().starts_with(NAMED),
                "{answer:?}: the named list's failure"
            );
        }
    }

    /// The same three worlds as the community list and the hub give them. The hub lists Unfamiliar
    /// Shores under a slug of its own, keeps Seedsow's id in lower case, and calls a type it does
    /// not know `Custom` beside the community list's own word.
    #[test]
    fn a_world_from_the_hub_is_kept_and_read_as_the_same_world_from_the_community_list() {
        let xml = r"<ArrayOfServerItem>
  <ServerItem><id>3f1f41ec-c7fd-4ed9-b47d-25b0d94219c1</id><name>Unfamiliar Shores</name>
    <description>Infiltration, as it was</description><emu>ACE</emu>
    <server_host>192.0.2.7</server_host><server_port>9000</server_port><type>PvE RP</type>
    <status>Stable</status><website_url>https://shores.example/</website_url>
    <discord_url>https://discord.gg/shores</discord_url></ServerItem>
  <ServerItem><id>394C58D0-885D-466B-B17F-D7E0B96FE3E2</id><name>Seedsow</name>
    <description>Classic Dereth</description><emu>GDL</emu>
    <server_host>seedsow.example</server_host><server_port>9060</server_port><type>PvE</type>
    <status>Stable</status></ServerItem>
  <ServerItem><id>9d17ce44-7db5-40c1-b7bb-a01c9de21d00</id><name>AChard</name>
    <description>PK rankings</description><emu>ACE</emu>
    <server_host>a-chard.example</server_host><server_port>9000</server_port><type>PvP</type>
    <status>Development</status></ServerItem>
</ArrayOfServerItem>";
        let imported = |slug: &str, name: &str, id: &str, ty: &str, ruleset: &str| {
            let mut row = hub_row(slug, name);
            row["ruleset"] = ruleset.into();
            row["provenance"] = serde_json::json!({"source": "community-list", "externalId": id,
                "legacyType": ty, "url": "https://github.com/acresources/serverslist"});
            row["endpoint"]["source"] = "community-list".into();
            row
        };
        let mut shores = imported(
            "ushores",
            "Unfamiliar Shores",
            "3f1f41ec-c7fd-4ed9-b47d-25b0d94219c1",
            "PvE RP",
            "Custom",
        );
        shores["description"] = "Infiltration, as it was".into();
        shores["maturity"] = "Stable".into();
        shores["website"] = "https://shores.example/".into();
        shores["discord"] = "https://discord.gg/shores".into();
        shores["endpoint"]["address"] = "192.0.2.7".into();
        let mut seedsow = imported(
            "seedsow",
            "Seedsow",
            "394c58d0-885d-466b-b17f-d7e0b96fe3e2",
            "PvE",
            "PvE",
        );
        seedsow["description"] = "Classic Dereth".into();
        seedsow["maturity"] = "Stable".into();
        seedsow["emulator"]["family"] = "GDL".into();
        seedsow["endpoint"]["port"] = 9060.into();
        let mut achard = imported(
            "achard",
            "AChard",
            "9d17ce44-7db5-40c1-b7bb-a01c9de21d00",
            "PvP",
            "PvP",
        );
        achard["description"] = "PK rankings".into();
        achard["maturity"] = "Development".into();
        achard["endpoint"]["address"] = "a-chard.example".into();
        let page = String::from_utf8(hub_page_of(&[shores, seedsow, achard], None)).unwrap();

        let from_list = parse_servers_xml(xml.as_bytes()).unwrap();
        let from_hub = parse_hub_pages(&[page]).unwrap();
        let slugs = |w: &[World]| w.iter().map(|w| w.slug.clone()).collect::<Vec<_>>();
        assert_eq!(slugs(&from_hub), ["unfamiliar-shores", "seedsow", "achard"]);
        assert_eq!(
            slugs(&from_hub),
            slugs(&from_list),
            "one key from either list"
        );
        // The table of worlds that run a client of their own knows both, by either spelling.
        assert_eq!(from_hub[0].logon_version.as_deref(), Some("c118"));
        assert_eq!(from_hub[1].world_profile.as_deref(), Some("classicdereth"));
        // The hub's own slug is kept, for its world alone.
        assert_eq!(
            from_hub
                .iter()
                .map(|w| w.hub_slug.as_deref())
                .collect::<Vec<_>>(),
            [Some("ushores"), Some("seedsow"), Some("achard")]
        );
        assert!(from_list.iter().all(|w| w.hub_slug.is_none()));
        // Ids compare without regard to case; everything else is the same world.
        let same = |w: &[World]| {
            w.iter()
                .cloned()
                .map(|mut w| {
                    w.list_id = w.list_id.map(|id| id.to_ascii_lowercase());
                    w.hub_slug = None;
                    w
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(same(&from_hub), same(&from_list));
    }

    #[test]
    fn a_hub_demonstration_world_is_shown_as_a_test_world_and_one_with_no_address_is_not() {
        let mut demo = hub_row("demo", "Demonstration");
        demo["isDemo"] = true.into();
        demo["maturity"] = "Stable".into();
        let mut unseen = hub_row("unseen", "Not Yet Seen");
        unseen["endpoint"] =
            serde_json::json!({"mode": "observed", "address": null, "port": 9000, "source": null});
        let mut observed = hub_row("seen", "Seen World");
        observed["endpoint"] = serde_json::json!({"mode": "observed", "address": "203.0.113.5",
            "port": 9001, "addressFamily": "ipv4", "source": "heartbeat-source"});
        let mut no_port = hub_row("noport", "No Port");
        no_port["endpoint"]["port"] = Value::Null;
        let nameless = serde_json::json!({"slug": "nameless",
            "endpoint": {"address": "n.example", "port": 9000}});
        let mut busy = hub_row("busy", "Busy World");
        busy["emulator"] = serde_json::json!({"family": "Empyrean", "version": "0.4.1"});
        busy["status"] = serde_json::json!({"state": "online", "playersOnline": 12});
        busy["website"] = "https://busy.example".into();
        let first = String::from_utf8(hub_page_of(
            &[demo, unseen, observed, no_port, nameless, busy.clone()],
            Some(100),
        ))
        .unwrap();
        // The world moved down a page while the pages were read.
        let second = String::from_utf8(hub_page_of(&[busy], None)).unwrap();

        let w = parse_hub_pages(&[first, second]).unwrap();
        assert_eq!(names(&w), ["Demonstration", "Seen World", "Busy World"]);
        assert_eq!(
            w[0].development_status.as_deref(),
            Some("Test"),
            "a test world says so where its status is shown, whatever its maturity"
        );
        assert_eq!(w[0].slug, "demonstration");
        let w = &w[1..];
        assert_eq!(
            w[0].endpoint.as_ref().map(|e| (e.address.as_str(), e.port)),
            Some(("203.0.113.5", 9001)),
            "the address the hub saw the world's heartbeats come from is its address"
        );
        let b = &w[1];
        assert_eq!(b.slug, "busy-world");
        assert_eq!(
            b.list_id, None,
            "a world registered at the hub has no list id"
        );
        assert_eq!(
            (b.emulator, b.emulator_version.as_deref()),
            (Emulator::Empyrean, Some("0.4.1"))
        );
        assert_eq!(b.development_status, None, "Unspecified says nothing");
        assert_eq!(b.ruleset.as_deref(), Some("PvE"));
        assert_eq!(b.links.website.as_deref(), Some("https://busy.example"));
        assert_eq!(b.description, None);
        assert_eq!(
            (b.state, b.players),
            (WorldState::Unknown, None),
            "whether it is up is asked of the world, not read from a copy up to a day old"
        );
    }

    #[test]
    fn decoding_handles_entities_references_and_leaves_strays_alone() {
        assert_eq!(
            decode("a &lt;b&gt; &quot;c&quot; &apos;d&apos;"),
            "a <b> \"c\" 'd'"
        );
        assert_eq!(decode("&#65;&#x42;"), "AB");
        assert_eq!(decode("fish & chips &bogus;"), "fish & chips &bogus;");
    }

    #[test]
    fn the_copy_is_used_for_a_day_and_round_trips() {
        let c = ListCache {
            fetched_at: 1_000,
            list: LIST.into(),
            hub: Vec::new(),
            sources: source_urls(&sources(None)),
        };
        assert!(c.is_fresh(1_000));
        assert!(c.is_fresh(1_000 + MAX_AGE_SECS - 1));
        assert!(
            !c.is_fresh(1_000 + MAX_AGE_SECS),
            "a day old is fetched again"
        );
        assert!(!c.is_fresh(999), "a copy from the future is not trusted");

        let d =
            std::env::temp_dir().join(format!("dereth-launch-listcache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(ListCache::load(&d), None, "no copy yet");
        c.save(&d).unwrap();
        assert_eq!(ListCache::load(&d).as_ref(), Some(&c));
        assert_eq!(c.worlds().unwrap().len(), 2);
        // A copy written while the directory was read with the list still reads.
        std::fs::write(
            d.join(CACHE_FILE),
            serde_json::json!({"fetched_at": 5, "list": LIST, "directory": ["{}"]}).to_string(),
        )
        .unwrap();
        assert_eq!(ListCache::load(&d).map(|c| c.fetched_at), Some(5));
        // The hub's pages are kept and read again as they were fetched.
        let hub = ListCache {
            fetched_at: 9,
            list: String::new(),
            hub: vec![
                String::from_utf8(hub_page_of(&[hub_row("coldeve", "Coldeve")], None)).unwrap(),
            ],
            sources: Vec::new(),
        };
        hub.save(&d).unwrap();
        let again = ListCache::load(&d).unwrap();
        assert_eq!(again, hub);
        assert_eq!(names(&again.worlds().unwrap()), ["Coldeve"]);
        std::fs::write(d.join(CACHE_FILE), b"{").unwrap();
        assert_eq!(ListCache::load(&d), None, "a damaged copy is no copy");
        let _ = std::fs::remove_dir_all(&d);
    }
}
