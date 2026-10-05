//! The community's list of worlds, `Servers.xml` in the `acresources/serverslist` repository, and
//! the copy of it the launcher keeps for a day.
//!
//! The list is the world set: one `<ServerItem>` per world, with an id that never changes, a name,
//! a description, the emulator (`ACE` or `GDL`), the host and port, the rules (`PvE` or `PvP`), how
//! settled the world is (`Stable`, `Development` or `Experimental`) and its website and Discord.
//! It says nothing about a world's era, its data files, its software's version or whether it is up.
//!
//! With the servers the player adds by hand, the list is every world the launcher shows. What the
//! list does not say (era, systems, version, players) comes from the world itself when it answers
//! (Empyrean's status document) or from the player.
//!
//! Reading is tolerant: a row without a name is skipped, a malformed port is no endpoint, and
//! nothing in one row can cost the player another.

use std::collections::{BTreeMap, HashSet};
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::world::{Emulator, Endpoint, Links, World};

/// Where the list is read from unless the launcher is told otherwise.
pub const COMMUNITY_LIST_URL: &str =
    "https://raw.githubusercontent.com/acresources/serverslist/master/Servers.xml";

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
    let mut slug = slug_for(name);
    if slug.is_empty() {
        slug = id.clone().unwrap_or_else(|| "world".to_owned());
    }
    // Two names with one spelling: the later one is told apart by a number.
    if taken.contains(&slug) {
        let base = slug.clone();
        slug = (2u32..)
            .map(|n| format!("{base}-{n}"))
            .find(|s| !taken.contains(s))
            .expect("some slug is free");
    }
    taken.insert(slug.clone());

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

/// The day's copy: the list as it was fetched, and when.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ListCache {
    /// Seconds since the epoch.
    pub fetched_at: u64,
    /// `Servers.xml`, as fetched.
    pub list: String,
}

impl ListCache {
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
        parse_servers_xml(self.list.as_bytes())
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
    use super::*;

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
        std::fs::write(d.join(CACHE_FILE), b"{").unwrap();
        assert_eq!(ListCache::load(&d), None, "a damaged copy is no copy");
        let _ = std::fs::remove_dir_all(&d);
    }
}
