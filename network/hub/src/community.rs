//! The community server list: importing its `Servers.xml`, and the two compatibility feeds.
//!
//! The list is `<ArrayOfServerItem>` of `<ServerItem>` records, each a flat run of simple elements
//! (`id`, `name`, `description`, `emu`, `server_host`, `server_port`, `type`, `status`,
//! `website_url`, `discord_url`). The reader takes exactly that shape: no document type, no entity
//! declarations, no attributes it needs, at most 2 MiB and 2000 records. An entry's slug is its
//! name folded to the registry's slug form; one that does not fold becomes `world-` and the
//! first eight characters of its id.

use std::time::Duration;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::fetch::{any_url, AddressRule, Fetch, Url};
use crate::registry::{CommunityEntry, PlanItem, Registration};

/// Where the list comes from, recorded on every imported listing.
pub const COMMUNITY_SOURCE: &str = "https://github.com/acresources/serverslist";

/// Where the list itself is read from: the file on that repository's default branch.
pub const COMMUNITY_LIST_URL: &str =
    "https://raw.githubusercontent.com/acresources/serverslist/master/Servers.xml";

/// The most of a list that is read: a longer one is refused whole, as the reader refuses it.
pub const MAX_LIST_BYTES: usize = 2 * 1024 * 1024;

/// How long reading the list may take, redirects included.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Reads the community list from `url`: a `200` answer of at most [`MAX_LIST_BYTES`] in UTF-8,
/// within [`FETCH_TIMEOUT`], through at most three redirects, from addresses `rule` allows.
///
/// # Errors
/// Why the list was not read; nothing else has happened.
pub fn fetch_list(url: &str, rule: AddressRule) -> Result<String, String> {
    let fetch = Fetch {
        method: "GET",
        url: Url::parse(url).map_err(|e| format!("{url}: {e}"))?,
        headers: vec![("Accept".into(), "application/xml, text/xml, */*".into())],
        body: Vec::new(),
        timeout: FETCH_TIMEOUT,
        // One byte more than is allowed, so that a longer list is seen to be longer.
        max_body: MAX_LIST_BYTES + 1,
        max_redirects: 3,
        rule,
        deny: Vec::new(),
        check: any_url,
        check_with: Vec::new(),
    };
    let r = fetch.send().map_err(|e| format!("{url}: {e}"))?;
    if r.status != 200 {
        return Err(format!("{url}: answered {}", r.status));
    }
    if r.body.len() > MAX_LIST_BYTES {
        return Err(format!("{url}: the list is longer than 2 MiB"));
    }
    String::from_utf8(r.body).map_err(|_| format!("{url}: the list is not UTF-8"))
}

/// One line saying what an import did (or, a dry run, would do): its counts, and the slugs of the
/// listings that conflicted, were taken out or came back, and the names of refused records.
#[must_use]
pub fn summary(plan: &[PlanItem], refused: &[(usize, String, String)]) -> String {
    let count = |action: &str| plan.iter().filter(|p| p.action == action).count();
    let listed = |names: Vec<&str>| {
        if names.is_empty() {
            String::new()
        } else {
            format!(" ({})", names.join(", "))
        }
    };
    let slugs = |action: &str| {
        listed(
            plan.iter()
                .filter(|p| p.action == action)
                .map(|p| p.slug.as_str())
                .collect(),
        )
    };
    let relisted: Vec<&str> = plan
        .iter()
        .filter(|p| p.relisted)
        .map(|p| p.slug.as_str())
        .collect();
    // A refused record's name is as the list wrote it: kept to one line.
    let refused: Vec<String> = refused
        .iter()
        .map(|r| {
            r.1.chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect()
        })
        .collect();
    format!(
        "community import: {} added, {} updated, {} unchanged, {} conflicts{}, {} unlisted{}, {} relisted{}; {} managed, {} missing, {} refused{}",
        count("add"),
        count("update"),
        count("unchanged"),
        count("conflict"),
        slugs("conflict"),
        count("unlist"),
        slugs("unlist"),
        relisted.len(),
        listed(relisted.clone()),
        count("skip"),
        count("missing"),
        refused.len(),
        listed(refused.iter().map(String::as_str).collect()),
    )
}

/// What the reader made of a list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommunityInput {
    pub entries: Vec<CommunityEntry>,
    /// `(record number, name, why)` for each record that was refused.
    pub errors: Vec<(usize, String, String)>,
    /// `(record number, name, what was changed)`.
    pub warnings: Vec<(usize, String, String)>,
    /// The ids of refused records that have one: the list still holds them.
    pub refused_ids: Vec<String>,
}

fn unescape(s: &str) -> Result<String, String> {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail.find(';').ok_or("an unterminated entity")?;
        let ent = &tail[1..end];
        let c = match ent {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ if ent.starts_with("#x") => u32::from_str_radix(&ent[2..], 16)
                .ok()
                .and_then(char::from_u32)
                .ok_or("a bad character reference")?,
            _ if ent.starts_with('#') => ent[1..]
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .ok_or("a bad character reference")?,
            _ => return Err(format!("an unknown entity &{ent};")),
        };
        out.push(c);
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The records of the document: each a list of `(element, text)`.
fn records(xml: &str) -> Result<Vec<Vec<(String, String)>>, String> {
    if xml.len() > 2 * 1024 * 1024 {
        return Err("Community XML exceeds 2 MiB".into());
    }
    let lower = xml.to_ascii_lowercase();
    if lower.contains("<!doctype") || lower.contains("<!entity") {
        return Err("DTD and entity declarations are not accepted".into());
    }
    let mut s = xml.trim_start_matches('\u{feff}').trim();
    if s.starts_with("<?xml") {
        let end = s.find("?>").ok_or("an unterminated declaration")?;
        s = s[end + 2..].trim_start();
    }
    let body = s
        .strip_prefix("<ArrayOfServerItem")
        .ok_or("Expected ArrayOfServerItem root")?;
    let open_end = body.find('>').ok_or("an unterminated root tag")?;
    if body[..open_end].ends_with('/') {
        return Ok(Vec::new());
    }
    let body = &body[open_end + 1..];
    let close = body
        .rfind("</ArrayOfServerItem>")
        .ok_or("the root is not closed")?;
    if !body[close + "</ArrayOfServerItem>".len()..]
        .trim()
        .is_empty()
    {
        return Err("content after the root".into());
    }
    let mut rest = &body[..close];
    let mut out = Vec::new();
    loop {
        rest = skip_comments(rest.trim_start())?;
        if rest.is_empty() {
            break;
        }
        let r = rest
            .strip_prefix("<ServerItem>")
            .ok_or("expected <ServerItem>")?;
        let end = r.find("</ServerItem>").ok_or("an unclosed ServerItem")?;
        out.push(fields(&r[..end])?);
        if out.len() > 2000 {
            return Err("At most 2000 listings can be imported at once".into());
        }
        rest = &r[end + "</ServerItem>".len()..];
    }
    Ok(out)
}

fn skip_comments(mut s: &str) -> Result<&str, String> {
    while let Some(r) = s.strip_prefix("<!--") {
        let end = r.find("-->").ok_or("an unterminated comment")?;
        s = r[end + 3..].trim_start();
    }
    Ok(s)
}

fn fields(mut s: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    loop {
        s = skip_comments(s.trim_start())?;
        if s.is_empty() {
            return Ok(out);
        }
        let r = s.strip_prefix('<').ok_or("expected an element")?;
        let gt = r.find('>').ok_or("an unterminated tag")?;
        let tag = &r[..gt];
        if let Some(name) = tag.strip_suffix('/') {
            out.push((name.trim().to_owned(), String::new()));
            s = &r[gt + 1..];
            continue;
        }
        let name = tag.split_whitespace().next().ok_or("an empty tag")?;
        if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(format!("a bad element name {name:?}"));
        }
        let after = &r[gt + 1..];
        let close = format!("</{name}>");
        let end = after
            .find(&close)
            .ok_or_else(|| format!("an unclosed <{name}>"))?;
        let text = &after[..end];
        if text.contains('<') {
            return Err(format!("<{name}> holds markup"));
        }
        out.push((name.to_owned(), unescape(text.trim())?));
        s = &after[end + close.len()..];
    }
}

fn fold_slug(name: &str, id: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for c in name.chars() {
        let c = crate::fold::fold_char(c);
        for c in c.chars() {
            if c.is_ascii_alphanumeric() {
                slug.push(c.to_ascii_lowercase());
                dash = false;
            } else if !dash && !slug.is_empty() {
                slug.push('-');
                dash = true;
            }
        }
    }
    let mut slug: String = slug.trim_end_matches('-').chars().take(40).collect();
    while slug.ends_with('-') {
        slug.pop();
    }
    if crate::registry::is_registry_slug(&slug) {
        slug
    } else {
        let head: String = slug.chars().take(30).collect();
        let head = head.trim_end_matches('-');
        format!(
            "{}-{}",
            if head.is_empty() { "world" } else { head },
            &id[..8]
        )
    }
}

fn is_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && parts.iter().map(|p| p.len()).eq([8, 4, 4, 4, 12])
        && parts
            .iter()
            .all(|p| p.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Reads a community list. Records that fail are listed, not imported.
///
/// # Errors
/// When the document as a whole is not the list's shape.
pub fn parse(xml: &str) -> Result<CommunityInput, String> {
    let mut input = CommunityInput::default();
    let mut ids = std::collections::BTreeSet::new();
    for (i, rec) in records(xml)?.into_iter().enumerate() {
        let n = i + 1;
        let get = |k: &str| {
            rec.iter()
                .find(|(name, _)| name == k)
                .map_or("", |(_, v)| v.as_str())
                .to_owned()
        };
        let name = get("name");
        let id = get("id").to_ascii_lowercase();
        let result = (|| -> Result<CommunityEntry, String> {
            if !is_uuid(&id) {
                return Err("id: not a UUID".into());
            }
            if !ids.insert(id.clone()) {
                return Err("Duplicate community ID".into());
            }
            let mut url = |key: &str| -> Result<Option<String>, String> {
                let raw = get(key);
                if raw.is_empty() {
                    return Ok(None);
                }
                if ["wip", "n/a", "none"].contains(&raw.to_ascii_lowercase().as_str()) {
                    input.warnings.push((
                        n,
                        name.clone(),
                        format!("{key}: omitted placeholder \"{raw}\""),
                    ));
                    return Ok(None);
                }
                if !raw.contains(':') && raw.contains('.') && !raw.contains(char::is_whitespace) {
                    input.warnings.push((
                        n,
                        name.clone(),
                        format!("{key}: added https:// to \"{raw}\""),
                    ));
                    return Ok(Some(format!("https://{raw}")));
                }
                Ok(Some(raw))
            };
            let website = url("website_url")?;
            let discord = url("discord_url")?;
            let legacy = get("type");
            let ruleset = match legacy.to_ascii_lowercase().as_str() {
                "pve" => "PvE",
                "pvp" | "pk" => "PvP",
                "pvp-lite" => "PvP-lite",
                _ => "Custom",
            };
            let port: u16 = get("server_port")
                .parse()
                .map_err(|_| "server_port".to_owned())?;
            let emu = get("emu");
            let status = get("status");
            // The list's descriptions were typed anywhere: line breaks are kept as line feeds and
            // tabs read as spaces; any other hidden character refuses the record.
            let description = get("description")
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .replace('\t', " ");
            let registration = Registration::from_json(&json!({
                "slug": fold_slug(&name, &id),
                "name": name,
                "description": description,
                "ruleset": ruleset,
                "maturity": if status.is_empty() { "Unspecified".to_owned() } else { status },
                "website": website,
                "discord": discord,
                "endpoint": { "mode": "manual", "address": get("server_host"), "port": port },
                "emulator": { "family": if emu.is_empty() { "Unspecified".to_owned() } else { emu } },
            }))
            .map_err(|e| e.message)?;
            let source_hash = hex(&Sha256::digest(
                json!({ "metadata": registration, "legacyType": legacy }).to_string(),
            ));
            Ok(CommunityEntry {
                external_id: id.clone(),
                registration,
                source_hash,
                legacy_type: legacy,
            })
        })();
        match result {
            Ok(e) => input.entries.push(e),
            Err(why) => {
                if is_uuid(&id) {
                    input.refused_ids.push(id);
                }
                input.errors.push((n, name.clone(), why));
            }
        }
    }
    Ok(input)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn escape(s: &str) -> String {
    crate::text::xml_safe(s)
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The registry as a community list, for launchers that read that format: every non-demo world
/// with an address.
#[must_use]
pub fn thwarg_xml(servers: &[Value]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ArrayOfServerItem>\n");
    for s in servers {
        if s["isDemo"].as_bool() == Some(true) || s["endpoint"]["address"].is_null() {
            continue;
        }
        let str_of = |v: &Value| v.as_str().unwrap_or("").to_owned();
        let legacy = s["provenance"]["legacyType"].as_str();
        let ty = legacy.map_or_else(|| str_of(&s["ruleset"]), str::to_owned);
        let id = s["provenance"]["externalId"]
            .as_str()
            .map_or_else(|| str_of(&s["id"]), str::to_owned);
        out.push_str("  <ServerItem>\n");
        for (k, v) in [
            ("id", id),
            ("name", str_of(&s["name"])),
            ("description", str_of(&s["description"])),
            ("emu", str_of(&s["emulator"]["family"])),
            ("server_host", str_of(&s["endpoint"]["address"])),
            ("server_port", s["endpoint"]["port"].to_string()),
            ("type", ty),
            ("status", str_of(&s["maturity"])),
            ("website_url", str_of(&s["website"])),
            ("discord_url", str_of(&s["discord"])),
        ] {
            out.push_str(&format!("    <{k}>{}</{k}>\n", escape(&v)));
        }
        out.push_str("  </ServerItem>\n");
    }
    out.push_str("</ArrayOfServerItem>\n");
    out
}

/// Population counts in the shape the character-statistics site reads: one row per non-demo
/// world reporting a count.
#[must_use]
pub fn treestats_counts(servers: &[Value], now: std::time::SystemTime) -> Value {
    let rows: Vec<Value> = servers
        .iter()
        .filter(|s| {
            s["isDemo"].as_bool() != Some(true)
                && !s["status"]["playersOnline"].is_null()
                && !s["status"]["lastSeenAt"].is_null()
        })
        .map(|s| {
            let seen = s["status"]["lastSeenAt"].as_str().unwrap_or("");
            let age = crate::registry::parse_iso(seen)
                .and_then(|t| now.duration_since(t).ok())
                .map_or(0, |d| d.as_secs());
            json!({
                "server": s["name"],
                "count": s["status"]["playersOnline"],
                "date": seen.replace('T', " ").split('.').next().map(|d| format!("{d} UTC")),
                "age": if age < 60 { format!("{age} seconds ago") } else { format!("{} minutes ago", age / 60) },
            })
        })
        .collect();
    Value::Array(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Endpoint;

    const LIST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<ArrayOfServerItem>
  <ServerItem>
    <id>00000000-0000-4000-8000-0000000000a1</id>
    <name>Frost &amp; Fire</name>
    <description>A test world.</description>
    <emu>ACE</emu>
    <server_host>192.0.2.10</server_host>
    <server_port>9000</server_port>
    <type>PK</type>
    <status>Stable</status>
    <website_url>example.org</website_url>
    <discord_url>WIP</discord_url>
  </ServerItem>
  <!-- a comment between records -->
  <ServerItem>
    <id>not-a-uuid</id>
    <name>Broken</name>
    <server_host>192.0.2.11</server_host>
    <server_port>9000</server_port>
  </ServerItem>
  <ServerItem>
    <id>00000000-0000-4000-8000-0000000000a3</id>
    <name>Ærø!!</name>
    <server_host>192.0.2.12</server_host>
    <server_port>9050</server_port>
    <type>PvE</type>
  </ServerItem>
</ArrayOfServerItem>
"#;

    /// Spec: registry: the community list is read record by record; a bad record is listed, not
    /// imported.
    #[test]
    fn the_community_list_is_read_record_by_record() {
        let input = parse(LIST).expect("the list's shape");
        assert_eq!(input.entries.len(), 2);
        assert_eq!(input.errors.len(), 1);
        assert_eq!(input.errors[0].1, "Broken");
        let first = &input.entries[0];
        assert_eq!(first.registration.slug, "frost-fire");
        assert_eq!(first.registration.name, "Frost & Fire");
        assert_eq!(first.registration.ruleset, "PvP");
        assert_eq!(
            first.registration.website.as_deref(),
            Some("https://example.org")
        );
        assert_eq!(first.registration.discord, None);
        assert_eq!(input.warnings.len(), 2);
        assert_eq!(
            input.entries[1].registration.slug, "aero",
            "folded to the slug form"
        );
        assert!(matches!(
            input.entries[1].registration.endpoint,
            Endpoint::Manual { port: 9050, .. }
        ));
    }

    /// Spec: registry: a document with a type declaration or entities is refused whole.
    #[test]
    fn a_document_with_declarations_is_refused() {
        assert!(parse("<!DOCTYPE x [<!ENTITY a \"b\">]><ArrayOfServerItem/>").is_err());
        assert!(parse("<Other/>").is_err());
        assert_eq!(
            parse("<ArrayOfServerItem/>").map(|i| i.entries.len()),
            Ok(0)
        );
    }

    /// Spec: registry: the list exported again reads back to the same listings.
    #[test]
    fn the_exported_list_reads_back() {
        let input = parse(LIST).expect("parsed");
        let servers: Vec<Value> = input
            .entries
            .iter()
            .map(|e| {
                let mut v = serde_json::to_value(&e.registration).expect("value");
                v["endpoint"]["address"] = json!("192.0.2.10");
                v["provenance"] =
                    json!({ "externalId": e.external_id, "legacyType": e.legacy_type });
                v
            })
            .collect();
        let again = parse(&thwarg_xml(&servers)).expect("re-read");
        assert_eq!(again.entries.len(), 2);
        assert_eq!(again.entries[0].external_id, input.entries[0].external_id);
        assert_eq!(again.entries[0].registration.name, "Frost & Fire");
    }

    /// Spec: registry: an imported description keeps its line breaks as line feeds and reads tabs
    /// as spaces; any other hidden character, typed or as a character reference, refuses the
    /// record.
    #[test]
    fn an_imported_record_with_a_hidden_character_is_refused() {
        let item = |id: &str, name: &str, description: &str| {
            format!(
                "<ServerItem><id>00000000-0000-4000-8000-0000000000{id}</id><name>{name}</name>\
                 <description>{description}</description><server_host>192.0.2.10</server_host>\
                 <server_port>9000</server_port></ServerItem>"
            )
        };
        let list = format!(
            "<ArrayOfServerItem>{}{}{}{}</ArrayOfServerItem>",
            item("b1", "Lines", "One\r\ntwo\rthree\tfour"),
            item("b2", "Back\u{202e}wards", "x"),
            item("b3", "Joined", "a&#x200B;b"),
            item("b4", "Belled", "a&#7;b"),
        );
        let input = parse(&list).expect("the list's shape");
        assert_eq!(input.entries.len(), 1, "{:?}", input.errors);
        assert_eq!(
            input.entries[0].registration.description,
            "One\ntwo\nthree four"
        );
        let refused: Vec<&str> = input.errors.iter().map(|e| e.1.as_str()).collect();
        assert_eq!(refused, ["Back\u{202e}wards", "Joined", "Belled"]);
    }

    /// Spec: registry: the community list the hub writes holds no character XML 1.0 forbids,
    /// whatever a listing kept from before holds.
    #[test]
    fn the_written_list_drops_characters_xml_forbids() {
        let input = parse(LIST).expect("parsed");
        let mut v = serde_json::to_value(&input.entries[0].registration).expect("value");
        v["endpoint"]["address"] = json!("192.0.2.10");
        v["provenance"] = json!({ "externalId": input.entries[0].external_id });
        v["name"] = json!("Frost\u{1} & Fire\u{fffe}");
        v["description"] = json!("A\u{0} test\u{b} world.\u{ffff}");
        let xml = thwarg_xml(&[v]);
        assert!(
            !xml.contains(['\u{0}', '\u{1}', '\u{b}', '\u{fffe}', '\u{ffff}']),
            "{xml:?}"
        );
        let again = parse(&xml).expect("re-read");
        assert_eq!(again.entries[0].registration.name, "Frost & Fire");
        assert_eq!(again.entries[0].registration.description, "A test world.");
    }
}
