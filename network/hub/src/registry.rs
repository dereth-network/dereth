//! The registry: every world the network knows, its key, its status and its federation flag.
//!
//! One SQLite file. A world is registered by its operator (and given a key), or imported from the
//! community list as an unclaimed listing that an operator later claims. A key is `drth_` and 43
//! characters of base64url (32 random bytes); only its SHA-256 is stored, and a presented key is
//! compared by digest in constant time. A world's heartbeat, over HTTP or over its link, keeps its
//! status fresh for [`HEARTBEAT_TTL_SECS`]. The federation flag starts unset; the hub's operator
//! sets it, and a world's own operator turns it off, or on once the world is verified
//! ([`crate::operators`]). A world registered through the operator API is `pending`, and in none of
//! the public answers, until it is verified.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dereth_network_protocol::address::is_network_slug;
use dereth_network_protocol::Federation;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// How long a heartbeat keeps a world's status fresh.
pub const HEARTBEAT_TTL_SECS: u64 = 180;

/// The registry's schema version (`PRAGMA user_version`). A hub refuses a registry newer than it.
pub const SCHEMA_VERSION: i64 = 7;

/// Registry slugs: the platform's own words, never a world's.
pub const RESERVED_SLUGS: &[&str] = &[
    "www",
    "api",
    "auth",
    "status",
    "admin",
    "account",
    "accounts",
    "login",
    "logout",
    "register",
    "servers",
    "characters",
    "profiles",
    "docs",
    "developer",
    "developers",
    "assets",
    "static",
    "cdn",
    "media",
    "images",
    "files",
    "downloads",
    "support",
    "help",
    "about",
    "legal",
    "privacy",
    "terms",
    "blog",
    "news",
    "forum",
    "forums",
    "mail",
    "email",
    "smtp",
    "imap",
    "pop",
    "autodiscover",
    "autoconfig",
    "webmail",
    "ns1",
    "ns2",
    "dns",
    "ftp",
    "ssh",
    "vpn",
    "metrics",
    "monitor",
    "health",
    "staging",
    "stage",
    "test",
    "dev",
    "localhost",
    "internal",
    "origin",
    "tunnel",
    "game",
    "play",
    "connect",
    "oidc",
    "oauth",
    "sso",
    "id",
    "identity",
    "console",
];

/// An error the registry answers with: a status code and a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryError {
    pub status: u16,
    pub message: String,
    /// Why a verification failed (`token_not_found`, ...), for a `422`.
    pub reason: Option<&'static str>,
    /// When to try again, in seconds, for a `429`.
    pub retry_after: Option<u64>,
    /// The error's code where its status alone does not give it (`LINK_FAILED` for a `422`).
    pub code: Option<&'static str>,
}

impl RegistryError {
    pub(crate) fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            reason: None,
            retry_after: None,
            code: None,
        }
    }
    pub(crate) fn bad(message: impl Into<String>) -> Self {
        Self::new(400, message)
    }
    pub(crate) fn not_found() -> Self {
        Self::new(404, "Server not found")
    }
    pub(crate) fn conflict(message: impl Into<String>) -> Self {
        Self::new(409, message)
    }
    pub(crate) fn forbidden(message: impl Into<String>) -> Self {
        Self::new(403, message)
    }
    /// A verification that failed, and why.
    pub(crate) fn unverified(reason: &'static str, message: impl Into<String>) -> Self {
        Self {
            reason: Some(reason),
            ..Self::new(422, message)
        }
    }
    /// A limit that is spent until `retry_after` seconds from now.
    pub(crate) fn limited(message: impl Into<String>, retry_after: u64) -> Self {
        Self {
            retry_after: Some(retry_after.max(1)),
            ..Self::new(429, message)
        }
    }
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.status)
    }
}

impl std::error::Error for RegistryError {}

impl From<rusqlite::Error> for RegistryError {
    fn from(e: rusqlite::Error) -> Self {
        Self::new(500, format!("database: {e}"))
    }
}

type Result<T> = std::result::Result<T, RegistryError>;

/// Whether `slug` is a registry slug: 2 to 40 of `a-z`, `0-9` and single inner hyphens, and not
/// one of the platform's words. (A world on the network needs a shorter one: see
/// [`is_network_slug`].)
#[must_use]
pub fn is_registry_slug(slug: &str) -> bool {
    (2..=40).contains(&slug.len())
        && slug.split('-').all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
        && !RESERVED_SLUGS.contains(&slug)
}

/// A world's endpoint as its operator registers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase", deny_unknown_fields)]
pub enum Endpoint {
    /// A fixed address (an IP or a host name) and port.
    Manual { address: String, port: u16 },
    /// The address the world's heartbeats come from, and this port.
    Observed {
        port: u16,
        #[serde(rename = "addressFamily", default = "ipv4")]
        address_family: String,
    },
}

fn ipv4() -> String {
    "ipv4".to_owned()
}

/// The emulator a world runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Emulator {
    pub family: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// A world's registration, as its operator writes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub ruleset: String,
    #[serde(default = "unspecified")]
    pub region: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discord: Option<String>,
    #[serde(default = "unspecified")]
    pub maturity: String,
    #[serde(rename = "isDemo", default)]
    pub is_demo: bool,
    pub endpoint: Endpoint,
    pub emulator: Emulator,
}

fn unspecified() -> String {
    "Unspecified".to_owned()
}

/// The first release's registration form (`ip`, `port`, `software`), still accepted.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyRegistration {
    slug: String,
    name: String,
    #[serde(default)]
    description: String,
    ruleset: String,
    #[serde(default = "unspecified")]
    region: String,
    #[serde(default)]
    website: Option<String>,
    #[serde(default)]
    discord: Option<String>,
    #[serde(default = "unspecified")]
    maturity: String,
    #[serde(rename = "isDemo", default)]
    is_demo: bool,
    ip: String,
    port: u16,
    software: String,
}

impl Registration {
    /// Reads a registration in either form and checks it.
    ///
    /// # Errors
    /// A 400 naming the first field that is wrong.
    pub fn from_json(value: &Value) -> Result<Self> {
        let r = match serde_json::from_value::<Registration>(value.clone()) {
            Ok(r) => r,
            Err(first) => match serde_json::from_value::<LegacyRegistration>(value.clone()) {
                Ok(l) => Registration {
                    slug: l.slug,
                    name: l.name,
                    description: l.description,
                    ruleset: l.ruleset,
                    region: l.region,
                    website: l.website,
                    discord: l.discord,
                    maturity: l.maturity,
                    is_demo: l.is_demo,
                    endpoint: Endpoint::Manual {
                        address: l.ip,
                        port: l.port,
                    },
                    emulator: Emulator {
                        family: l.software,
                        version: None,
                    },
                },
                Err(_) => return Err(RegistryError::bad(first.to_string())),
            },
        };
        r.check()?;
        Ok(r)
    }

    fn check(&self) -> Result<()> {
        let chars = |s: &str| s.trim().chars().count();
        if !is_registry_slug(&self.slug) {
            return Err(RegistryError::bad(
                "slug: use 2-40 lowercase letters, numbers and single hyphens, not a reserved word",
            ));
        }
        if !(2..=80).contains(&chars(&self.name)) {
            return Err(RegistryError::bad("name: 2-80 characters"));
        }
        // A world's name reaches every player on the network: no angle brackets, which a client
        // reads as name-link markup, and (as every field) nothing hidden.
        if self.name.contains(['<', '>']) {
            return Err(RegistryError::bad("name: no angle brackets"));
        }
        let mut fields = vec![
            ("name", self.name.as_str(), false),
            ("description", self.description.as_str(), true),
            ("region", self.region.as_str(), false),
            ("maturity", self.maturity.as_str(), false),
            ("emulator.family", self.emulator.family.as_str(), false),
        ];
        for (field, value) in [
            ("website", &self.website),
            ("discord", &self.discord),
            ("emulator.version", &self.emulator.version),
        ] {
            if let Some(v) = value {
                fields.push((field, v.as_str(), false));
            }
        }
        for (field, value, lines) in fields {
            if !crate::text::is_plain(value, lines) {
                return Err(RegistryError::bad(format!(
                    "{field}: no control, bidirectional or zero-width characters{}",
                    if lines { " (line feeds are kept)" } else { "" }
                )));
            }
        }
        if chars(&self.description) > 2000 {
            return Err(RegistryError::bad("description: at most 2000 characters"));
        }
        if !["PvE", "PvP", "PvP-lite", "Custom"].contains(&self.ruleset.as_str()) {
            return Err(RegistryError::bad("ruleset: PvE, PvP, PvP-lite or Custom"));
        }
        if !(2..=60).contains(&chars(&self.region)) || chars(&self.maturity) > 60 {
            return Err(RegistryError::bad("region or maturity"));
        }
        for url in [&self.website, &self.discord].into_iter().flatten() {
            if !is_web_url(url) {
                return Err(RegistryError::bad(
                    "website/discord: an HTTP(S) URL without credentials",
                ));
            }
        }
        if let Endpoint::Manual { address, port } = &self.endpoint {
            if *port == 0 || !is_address(address) {
                return Err(RegistryError::bad(
                    "endpoint: an IP address or host name and a port",
                ));
            }
        }
        if let Endpoint::Observed {
            port,
            address_family,
        } = &self.endpoint
        {
            if *port == 0 || !["ipv4", "ipv6"].contains(&address_family.as_str()) {
                return Err(RegistryError::bad("endpoint: a port and ipv4 or ipv6"));
            }
        }
        if !(1..=60).contains(&chars(&self.emulator.family)) {
            return Err(RegistryError::bad("emulator.family: 1-60 characters"));
        }
        if self
            .emulator
            .version
            .as_deref()
            .is_some_and(|v| !(1..=60).contains(&chars(v)))
        {
            return Err(RegistryError::bad("emulator.version: 1-60 characters"));
        }
        Ok(())
    }
}

fn is_web_url(s: &str) -> bool {
    let rest = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"));
    let Some(rest) = rest else { return false };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    s.len() <= 500 && !host.is_empty() && !host.contains('@')
}

/// An IP address, or a host name of dotted labels ending in a 2-63 letter top-level label.
#[must_use]
pub fn is_address(s: &str) -> bool {
    if s.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    let labels: Vec<&str> = s.split('.').collect();
    s.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
        && labels.last().is_some_and(|t| {
            (2..=63).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_alphabetic())
        })
}

/// A heartbeat, as a world reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Heartbeat {
    pub state: String,
    #[serde(rename = "playersOnline", default)]
    pub players_online: Option<u32>,
    #[serde(default)]
    pub message: String,
    #[serde(rename = "playersKind", default = "unknown")]
    pub players_kind: String,
    #[serde(
        rename = "emulatorVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub emulator_version: Option<String>,
    #[serde(
        rename = "uptimeSeconds",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub uptime_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<Adapter>,
}

fn unknown() -> String {
    "unknown".to_owned()
}

/// What sent a heartbeat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Adapter {
    pub kind: String,
    pub version: String,
}

impl Heartbeat {
    /// Checks a heartbeat as the registry accepts it.
    ///
    /// # Errors
    /// A 400 for a state, count, kind or message out of range.
    pub fn check(&self) -> Result<()> {
        if !["online", "degraded", "maintenance", "offline"].contains(&self.state.as_str()) {
            return Err(RegistryError::bad(
                "state: online, degraded, maintenance or offline",
            ));
        }
        if self.players_online.is_some_and(|n| n > 100_000) {
            return Err(RegistryError::bad("playersOnline: 0-100000"));
        }
        if self.state == "offline" && self.players_online.is_some_and(|n| n > 0) {
            return Err(RegistryError::bad(
                "Offline servers cannot report online players",
            ));
        }
        if self.message.chars().count() > 240 {
            return Err(RegistryError::bad("message: at most 240 characters"));
        }
        if self
            .emulator_version
            .as_deref()
            .is_some_and(|v| v.chars().count() > 60)
        {
            return Err(RegistryError::bad("emulatorVersion: at most 60 characters"));
        }
        for (field, value) in [
            ("message", Some(self.message.as_str())),
            ("emulatorVersion", self.emulator_version.as_deref()),
            (
                "adapter.version",
                self.adapter.as_ref().map(|a| a.version.as_str()),
            ),
        ] {
            if value.is_some_and(|v| !crate::text::is_plain(v, false)) {
                return Err(RegistryError::bad(format!(
                    "{field}: no control, bidirectional or zero-width characters"
                )));
            }
        }
        if !["connected-characters", "accounts", "unknown"].contains(&self.players_kind.as_str()) {
            return Err(RegistryError::bad("playersKind"));
        }
        if let Some(a) = &self.adapter {
            if !["ace-mod", "agent", "manual", "empyrean"].contains(&a.kind.as_str())
                || a.version.is_empty()
                || a.version.len() > 40
            {
                return Err(RegistryError::bad("adapter"));
            }
        }
        Ok(())
    }
}

/// A community-list entry, ready to import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityEntry {
    pub external_id: String,
    pub registration: Registration,
    pub source_hash: String,
    pub legacy_type: String,
}

/// One line of an import plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanItem {
    #[serde(rename = "externalId")]
    pub external_id: String,
    pub slug: String,
    pub name: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The listing was out of the public lists because the list had dropped it, and is back.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub relisted: bool,
}

/// Why the import took a listing out of the public lists: the community list no longer holds it.
const GONE_UPSTREAM: &str = "gone-upstream";

/// How a community import runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportOptions {
    /// Change the registry; otherwise the plan only.
    pub apply: bool,
    /// Apply everything else when some entries conflict, and report the conflicts, rather than
    /// change nothing.
    pub skip_conflicts: bool,
    /// Take an unclaimed listing out of the public lists once this many applied imports in a row
    /// have not held it; none leaves listings the list dropped as they are.
    pub retire_after: Option<u32>,
    /// The ids of records the list holds that were refused: still on the list, never counted
    /// missing.
    pub also_on_list: Vec<String>,
}

/// A listing back on the community list, in its log.
fn relisted_event(tx: &rusqlite::Transaction<'_>, id: &str, now: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO registry_events(server_id, kind, received_at, details) VALUES (?, 'relisted', ?, ?)",
        params![id, now, json!({ "reason": "Back on the community list" }).to_string()],
    )?;
    Ok(())
}

/// Why a game endpoint at `address` is refused, if it is: one of `reserved` (the names the hub and
/// its site answer on) is refused for everyone, and any other name under `domain` for all but the
/// hub's operator.
fn endpoint_refusal(
    reserved: &[String],
    domain: &str,
    address: &str,
    hub_operator: bool,
) -> Option<String> {
    let a = host_name(address);
    if reserved.contains(&a) {
        return Some("Game endpoint must not be a name the hub or its site answers on".into());
    }
    let under = a == domain || a.ends_with(&format!(".{domain}"));
    (under && !hub_operator)
        .then(|| format!("Game endpoint under {domain} is for the hub's operator only"))
}

/// A host name as compared: lowercase, without a trailing dot.
fn host_name(host: &str) -> String {
    host.trim_end_matches('.').to_ascii_lowercase()
}

/// The host of `scheme://host[:port][/path]`, without its port or brackets.
fn url_host(url: &str) -> Option<&str> {
    let (_, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next()?,
        None => authority.split(':').next()?,
    };
    (!host.is_empty()).then_some(host)
}

/// The time source the registry stamps with.
pub type Clock = Box<dyn Fn() -> SystemTime + Send>;

/// The registry's database.
pub struct Registry {
    pub(crate) db: Connection,
    now: Clock,
    domain: String,
    /// The hub's own address, where a world's status and resolution are read.
    public_url: String,
    /// The site's origin, whose world list a world's web link opens.
    site: String,
    /// The host names the hub and its site answer on: never a world's endpoint, whoever asks.
    reserved_hosts: Vec<String>,
    /// How the operator API approves and checks (set by the server from its configuration).
    pub(crate) policy: crate::operators::Policy,
    /// The Discord ids of the hub's admins, who register under the hub's own domain as its
    /// operator does (set by the server from its configuration).
    pub(crate) admins: Vec<String>,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("domain", &self.domain)
            .finish_non_exhaustive()
    }
}

/// `drth_` and 43 characters of base64url.
#[must_use]
pub fn is_key_shape(key: &str) -> bool {
    key.len() == 48
        && key.starts_with("drth_")
        && key[5..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub(crate) fn digest(key: &str) -> [u8; 32] {
    Sha256::digest(key.as_bytes()).into()
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn unhex(s: &str) -> Option<[u8; 32]> {
    let mut out = [0u8; 32];
    if s.len() != 64 {
        return None;
    }
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

/// The digest a key is kept as, in the form [`crate::core::Directory::key`] reports it.
#[must_use]
pub fn key_digest(key: &str) -> String {
    hex(&digest(key))
}

/// Equal-length comparison that takes the same time wherever the bytes differ.
pub(crate) fn same_digest(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub(crate) fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("the OS random source failed");
    b
}

/// A fresh key.
#[must_use]
pub fn new_key() -> String {
    use base64::Engine as _;
    format!(
        "drth_{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random_bytes::<32>())
    )
}

pub(crate) fn new_id() -> String {
    let b = random_bytes::<16>();
    let h = hex(&b);
    format!(
        "{}-{}-4{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[13..16],
        &h[16..20],
        &h[20..32]
    )
}

/// Seconds since 1970 as an RFC 3339 UTC time, to the millisecond.
#[must_use]
pub fn iso(t: SystemTime) -> String {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs();
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Days to a civil date (proleptic Gregorian), after Howard Hinnant's algorithm.
    let z = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        d.subsec_millis()
    )
}

/// Reads a time [`iso`] wrote.
#[must_use]
pub fn parse_iso(s: &str) -> Option<SystemTime> {
    // The registry reads only times it wrote: `YYYY-MM-DDTHH:MM:SS.mmmZ`.
    let n = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let (y, mo, d) = (n(0, 4)?, n(5, 7)?, n(8, 10)?);
    let (h, mi, se, ms) = (n(11, 13)?, n(14, 16)?, n(17, 19)?, n(20, 23).unwrap_or(0));
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if mo > 2 { mo - 3 } else { mo + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = u64::try_from(days * 86_400 + h * 3600 + mi * 60 + se).ok()?;
    Some(UNIX_EPOCH + Duration::from_secs(secs) + Duration::from_millis(u64::try_from(ms).ok()?))
}

/// The registry file at `path`, opened read-only: nothing is created, nothing is moved forward to
/// this build's schema, and nothing is written but what SQLite's readers share.
fn open_existing(path: &std::path::Path) -> Result<Connection> {
    if !path.is_file() {
        return Err(RegistryError::new(
            404,
            format!("there is no registry at {}", path.display()),
        ));
    }
    let db = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.busy_timeout(Duration::from_secs(5))?;
    Ok(db)
}

/// The schema version of the registry file at `path`, read without opening it as a registry.
///
/// # Errors
/// 404 when there is no file at `path`; database errors.
pub fn schema_of_file(path: &std::path::Path) -> Result<i64> {
    Ok(open_existing(path)?.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

/// Writes the consistent copy [`Registry::backup`] writes, of the registry file at `path`, to `to`
/// (a new file), without opening it as a registry (it is only read, whatever its schema), while a
/// hub runs on it or not. The schema version of the copy.
///
/// # Errors
/// 404 when there is no file at `path`; 409 when `to` exists; database errors.
pub fn backup_file(path: &std::path::Path, to: &std::path::Path) -> Result<i64> {
    if to.exists() {
        return Err(RegistryError::conflict(format!(
            "{} exists: name a new file",
            to.display()
        )));
    }
    let db = open_existing(path)?;
    db.execute("VACUUM INTO ?", [to.to_string_lossy()])?;
    drop(db);
    schema_of_file(to)
}

#[derive(Debug, Clone)]
pub(crate) struct Row {
    pub(crate) id: String,
    pub(crate) slug: String,
    pub(crate) metadata: Registration,
    pub(crate) key_hash: Option<String>,
    created_at: String,
    last_seen_at: Option<String>,
    heartbeat: Option<Heartbeat>,
    pub(crate) registry: Value,
    pub(crate) federation: Federation,
    /// The operator the world belongs to, if any.
    pub(crate) operator_id: Option<String>,
}

impl Row {
    /// `pending`, `managed` or `unclaimed`.
    pub(crate) fn state(&self) -> &str {
        self.registry["state"].as_str().unwrap_or("managed")
    }

    /// Whether the world is listed in the public answers: every world but a `pending`, a locked
    /// or an unlisted one.
    pub(crate) fn is_public(&self) -> bool {
        self.state() != "pending" && !self.is_locked() && !self.is_unlisted()
    }

    /// Whether the hub's operator has taken the world out of the public answers.
    pub(crate) fn is_unlisted(&self) -> bool {
        self.registry["unlisted"] == true
    }

    /// Whether the hub's operator has locked the world (revoked it after a report).
    pub(crate) fn is_locked(&self) -> bool {
        !self.registry["locked"].is_null()
    }
}

impl Registry {
    /// Opens (creating if need be) the registry at `path`; `:memory:` for one in memory.
    ///
    /// # Errors
    /// When the file cannot be opened or its schema is newer than this build's.
    pub fn open(path: &str, domain: &str, now: Clock) -> Result<Self> {
        if path != ":memory:" {
            if let Some(dir) = std::path::Path::new(path).parent() {
                if !dir.as_os_str().is_empty() {
                    std::fs::create_dir_all(dir)
                        .map_err(|e| RegistryError::new(500, format!("{}: {e}", dir.display())))?;
                }
            }
        }
        let db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(RegistryError::new(
                500,
                "The registry's schema is newer than this hub",
            ));
        }
        if version == 0 {
            db.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE servers (
                   id TEXT PRIMARY KEY,
                   slug TEXT NOT NULL UNIQUE,
                   metadata TEXT NOT NULL,
                   key_hash TEXT,
                   created_at TEXT NOT NULL,
                   last_seen_at TEXT,
                   heartbeat TEXT,
                   registry TEXT NOT NULL DEFAULT '{}',
                   federation TEXT NOT NULL DEFAULT 'unset'
                 );
                 CREATE UNIQUE INDEX community_identity
                   ON servers(json_extract(registry, '$.provenance.externalId'));
                 CREATE TABLE registry_events (
                   id INTEGER PRIMARY KEY,
                   server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                   kind TEXT NOT NULL, received_at TEXT NOT NULL, details TEXT NOT NULL);
                 CREATE INDEX server_events ON registry_events(server_id, id);
                 CREATE TABLE hub_state (name TEXT PRIMARY KEY, value TEXT NOT NULL);
                 PRAGMA user_version = 1;
                 COMMIT;",
            )?;
        }
        if version < 2 {
            // Each channel its own row, so a change writes that channel alone.
            db.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE IF NOT EXISTS hub_channels (name TEXT PRIMARY KEY, value TEXT NOT NULL);
                 PRAGMA user_version = 2;
                 COMMIT;",
            )?;
        }
        if version < 3 {
            // Channels were called clubs before the network's first release; a registry from then
            // keeps its rows under the new name.
            let old: bool = db.query_row(
                "SELECT count(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = 'hub_clubs'",
                [],
                |r| r.get(0),
            )?;
            db.execute_batch(if old {
                "BEGIN IMMEDIATE;
                 DROP TABLE IF EXISTS hub_channels;
                 ALTER TABLE hub_clubs RENAME TO hub_channels;
                 PRAGMA user_version = 3;
                 COMMIT;"
            } else {
                "PRAGMA user_version = 3;"
            })?;
        }
        if version < 4 {
            // World operators: their accounts and sessions, verifications and claims, reports, and
            // who did each registry event.
            db.execute_batch(
                "BEGIN IMMEDIATE;
                 ALTER TABLE servers ADD COLUMN operator_id TEXT;
                 ALTER TABLE registry_events ADD COLUMN actor TEXT;
                 CREATE TABLE operators (
                   id TEXT PRIMARY KEY,
                   discord_id TEXT NOT NULL UNIQUE,
                   name TEXT NOT NULL,
                   created_at TEXT NOT NULL,
                   signed_in_at TEXT,
                   disabled_at TEXT);
                 CREATE TABLE operator_sessions (
                   id_hash TEXT PRIMARY KEY,
                   operator_id TEXT NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
                   created_at TEXT NOT NULL,
                   expires_at TEXT NOT NULL);
                 CREATE TABLE operator_events (
                   id INTEGER PRIMARY KEY,
                   operator_id TEXT NOT NULL,
                   kind TEXT NOT NULL, at TEXT NOT NULL, details TEXT NOT NULL);
                 CREATE INDEX operator_event_kinds ON operator_events(kind, at);
                 CREATE TABLE verifications (
                   id TEXT PRIMARY KEY,
                   server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                   operator_id TEXT NOT NULL REFERENCES operators(id),
                   key_hash TEXT,
                   token_hash TEXT NOT NULL,
                   token_expires_at TEXT NOT NULL,
                   created_at TEXT NOT NULL,
                   state TEXT NOT NULL);
                 CREATE INDEX server_verifications ON verifications(server_id, state);
                 CREATE TABLE reports (
                   id TEXT PRIMARY KEY,
                   server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                   reporter_id TEXT NOT NULL REFERENCES operators(id),
                   reason TEXT NOT NULL, details TEXT NOT NULL,
                   created_at TEXT NOT NULL, state TEXT NOT NULL,
                   resolved_at TEXT, resolution TEXT, note TEXT);
                 PRAGMA user_version = 4;
                 COMMIT;",
            )?;
        }
        if version < 5 {
            // Persons: characters linked to a Discord account, the alts setting, link codes and
            // person friends.
            db.execute_batch(&format!(
                "BEGIN IMMEDIATE; {} PRAGMA user_version = 5; COMMIT;",
                crate::persons::SCHEMA
            ))?;
        }
        if version < 6 {
            // The link codes given in a recent window, by when, for each world's allowance.
            db.execute_batch(&format!(
                "BEGIN IMMEDIATE; {} PRAGMA user_version = 6; COMMIT;",
                crate::persons::SCHEMA_6
            ))?;
        }
        if version < 7 {
            // The friends handed back to an unlinked character, until its world is sent them.
            db.execute_batch(&format!(
                "BEGIN IMMEDIATE; {} PRAGMA user_version = 7; COMMIT;",
                crate::persons::SCHEMA_7
            ))?;
        }
        let mut registry = Self {
            db,
            now,
            domain: domain.to_owned(),
            public_url: String::new(),
            site: String::new(),
            reserved_hosts: Vec::new(),
            policy: crate::operators::Policy::default(),
            admins: Vec::new(),
        };
        registry.set_web(None, &[]);
        Ok(registry)
    }

    /// Names the hub's public URL and its site's origins (the first is the site): a world's links
    /// point at them, and their hosts are reserved beside the domain itself, `hub.<domain>` and
    /// `api.<domain>`. Without them the hub is at `https://hub.<domain>` and the site at
    /// `https://<domain>`.
    pub fn set_web(&mut self, public_url: Option<&str>, site_origins: &[String]) {
        let d = &self.domain;
        self.public_url = public_url.map_or_else(
            || format!("https://hub.{d}"),
            |u| u.trim_end_matches('/').to_owned(),
        );
        self.site = site_origins.first().map_or_else(
            || format!("https://{d}"),
            |o| o.trim_end_matches('/').to_owned(),
        );
        let own = [d.clone(), format!("hub.{d}"), format!("api.{d}")];
        let mut hosts: Vec<String> = own
            .iter()
            .map(String::as_str)
            .chain(public_url.into_iter().filter_map(url_host))
            .chain(site_origins.iter().filter_map(|o| url_host(o)))
            .map(host_name)
            .collect();
        hosts.sort();
        hosts.dedup();
        self.reserved_hosts = hosts;
    }

    /// Whether `operator` is one of the hub's admins.
    pub(crate) fn is_admin(&self, operator: &crate::operators::Operator) -> bool {
        self.admins.contains(&operator.discord_id)
    }

    /// The time, to the millisecond the registry writes times in, so that a time read back is
    /// the time that was meant.
    pub(crate) fn now(&self) -> SystemTime {
        let t = (self.now)().duration_since(UNIX_EPOCH).unwrap_or_default();
        UNIX_EPOCH + Duration::from_millis(u64::try_from(t.as_millis()).unwrap_or(u64::MAX))
    }

    /// The columns [`Self::read_row`] reads, in its order.
    const COLUMNS: &'static str = "id, slug, metadata, key_hash, created_at, last_seen_at, heartbeat, registry, federation, operator_id";

    pub(crate) fn row(&self, slug: &str) -> Result<Option<Row>> {
        let slug = slug.to_ascii_lowercase();
        let r = self
            .db
            .query_row(
                &format!("SELECT {} FROM servers WHERE slug = ?", Self::COLUMNS),
                [slug],
                Self::read_row,
            )
            .optional()?;
        Ok(r)
    }

    /// One world by its id.
    pub(crate) fn row_by_id(&self, id: &str) -> Result<Option<Row>> {
        Ok(self
            .db
            .query_row(
                &format!("SELECT {} FROM servers WHERE id = ?", Self::COLUMNS),
                [id],
                Self::read_row,
            )
            .optional()?)
    }

    fn read_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
        let metadata: String = r.get(2)?;
        let heartbeat: Option<String> = r.get(6)?;
        let registry: String = r.get(7)?;
        let federation: String = r.get(8)?;
        Ok(Row {
            id: r.get(0)?,
            slug: r.get(1)?,
            metadata: serde_json::from_str(&metadata).expect("the registry wrote this metadata"),
            key_hash: r.get(3)?,
            created_at: r.get(4)?,
            last_seen_at: r.get(5)?,
            heartbeat: heartbeat.and_then(|h| serde_json::from_str(&h).ok()),
            registry: serde_json::from_str(&registry).unwrap_or_else(|_| json!({})),
            federation: federation.parse().unwrap_or(Federation::Unset),
            operator_id: r.get(9)?,
        })
    }

    pub(crate) fn all_rows(&self) -> Result<Vec<Row>> {
        let mut stmt = self.db.prepare(&format!(
            "SELECT {} FROM servers ORDER BY slug",
            Self::COLUMNS
        ))?;
        let rows = stmt
            .query_map([], Self::read_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Every world the public answers show: all but the `pending`, locked and unlisted ones.
    fn public_rows(&self) -> Result<Vec<Row>> {
        Ok(self
            .all_rows()?
            .into_iter()
            .filter(Row::is_public)
            .collect())
    }

    pub(crate) fn event(&self, server_id: &str, kind: &str, details: &Value) -> Result<()> {
        self.event_by(server_id, kind, None, details)
    }

    /// A registry event, with who did it: an operator's id, `hub` for the hub itself, or `None`
    /// for the hub's operator on the command line.
    pub(crate) fn event_by(
        &self,
        server_id: &str,
        kind: &str,
        actor: Option<&str>,
        details: &Value,
    ) -> Result<()> {
        self.db.execute(
            "INSERT INTO registry_events(server_id, kind, received_at, details, actor) VALUES (?, ?, ?, ?, ?)",
            params![server_id, kind, iso(self.now()), details.to_string(), actor],
        )?;
        Ok(())
    }

    fn validate_endpoint(&self, r: &Registration, hub_operator: bool) -> Result<()> {
        if let Endpoint::Manual { address, .. } = &r.endpoint {
            if let Some(why) =
                endpoint_refusal(&self.reserved_hosts, &self.domain, address, hub_operator)
            {
                return Err(RegistryError::bad(why));
            }
        }
        Ok(())
    }

    /// Registers a world as the hub's operator: it is managed at once, and its key is returned
    /// (once; only its digest is kept).
    ///
    /// # Errors
    /// 400 for a bad registration, 409 when the slug is taken.
    pub fn register(&self, r: &Registration) -> Result<(Value, String)> {
        self.register_as(r, true)
    }

    /// [`Self::register`], by the hub's operator (or one of its admins) or by another operator.
    pub(crate) fn register_as(
        &self,
        r: &Registration,
        hub_operator: bool,
    ) -> Result<(Value, String)> {
        r.check()?;
        self.validate_endpoint(r, hub_operator)?;
        self.expire_pending()?;
        if self.row(&r.slug)?.is_some() {
            return Err(RegistryError::conflict(format!(
                "The slug \"{}\" is already registered",
                r.slug
            )));
        }
        let key = new_key();
        let id = new_id();
        let now = iso(self.now());
        let registry = json!({
            "state": "managed", "verifiedAt": null, "provenance": null,
            "observedAddress": null,
            "endpointUpdatedAt": matches!(r.endpoint, Endpoint::Manual { .. }).then(|| now.clone()),
            "verification": { "method": "review", "verifiedAt": now, "endpointVerified": true },
            "keyIssuedAt": now,
        });
        self.db.execute(
            "INSERT INTO servers(id, slug, metadata, key_hash, created_at, registry) VALUES (?, ?, ?, ?, ?, ?)",
            params![
                id,
                r.slug,
                serde_json::to_string(r).expect("serialisable"),
                hex(&digest(&key)),
                now,
                registry.to_string()
            ],
        )?;
        self.event(&id, "registered", &json!({}))?;
        Ok((self.server(&r.slug)?.expect("just registered"), key))
    }

    /// Replaces a managed world's registration (its slug names it). A changed endpoint forgets the
    /// last heartbeat.
    ///
    /// # Errors
    /// 404, or 409 for an unclaimed listing.
    pub fn update(&self, r: &Registration) -> Result<Value> {
        self.update_by(r, None, true)
    }

    /// [`Self::update`], by `actor` ([`Self::event_by`]), who is the hub's operator (or one of its
    /// admins) or another operator. A changed endpoint is no longer the one the world was verified
    /// at.
    pub(crate) fn update_by(
        &self,
        r: &Registration,
        actor: Option<&str>,
        hub_operator: bool,
    ) -> Result<Value> {
        r.check()?;
        self.validate_endpoint(r, hub_operator)?;
        let row = self.row(&r.slug)?.ok_or_else(RegistryError::not_found)?;
        if row.registry["state"] == "unclaimed" {
            return Err(RegistryError::conflict(
                "Review and claim this listing before editing it",
            ));
        }
        let mut registry = row.registry.clone();
        if row.metadata.endpoint != r.endpoint {
            registry["observedAddress"] = Value::Null;
            registry["endpointUpdatedAt"] = match r.endpoint {
                Endpoint::Manual { .. } => Value::String(iso(self.now())),
                Endpoint::Observed { .. } => Value::Null,
            };
            if registry["verification"].is_object() {
                registry["verification"]["endpointVerified"] = json!(false);
            }
            self.db.execute(
                "UPDATE servers SET heartbeat = NULL, last_seen_at = NULL WHERE id = ?",
                [&row.id],
            )?;
            self.event_by(
                &row.id,
                "endpoint-configured",
                actor,
                &json!({ "endpoint": r.endpoint }),
            )?;
        }
        self.db.execute(
            "UPDATE servers SET metadata = ?, registry = ? WHERE id = ?",
            params![
                serde_json::to_string(r).expect("serialisable"),
                registry.to_string(),
                row.id
            ],
        )?;
        Ok(self.server(&r.slug)?.expect("present"))
    }

    /// A new key for a managed world; the old one stops working.
    ///
    /// # Errors
    /// 404, or 409 for an unclaimed listing.
    pub fn rotate_key(&self, slug: &str) -> Result<String> {
        self.rotate_key_by(slug, None)
    }

    /// [`Self::rotate_key`], by `actor` ([`Self::event_by`]).
    pub(crate) fn rotate_key_by(&self, slug: &str, actor: Option<&str>) -> Result<String> {
        let row = self.row(slug)?.ok_or_else(RegistryError::not_found)?;
        if row.registry["state"] == "unclaimed" {
            return Err(RegistryError::conflict(
                "Claim verification is required before issuing a key",
            ));
        }
        if row.is_locked() {
            return Err(RegistryError::conflict(
                "This world is locked: unlock it before it is given a key",
            ));
        }
        let key = new_key();
        let mut registry = row.registry.clone();
        registry["keyIssuedAt"] = json!(iso(self.now()));
        self.db.execute(
            "UPDATE servers SET key_hash = ?, registry = ? WHERE id = ?",
            params![hex(&digest(&key)), registry.to_string(), row.id],
        )?;
        self.event_by(&row.id, "key-rotated", actor, &json!({}))?;
        Ok(key)
    }

    /// Revokes a world's key: nothing authenticates as it until a new one is issued.
    ///
    /// # Errors
    /// 404.
    pub fn revoke_key(&self, slug: &str) -> Result<()> {
        let row = self.row(slug)?.ok_or_else(RegistryError::not_found)?;
        self.db
            .execute("UPDATE servers SET key_hash = NULL WHERE id = ?", [&row.id])?;
        self.event(&row.id, "key-revoked", &json!({}))?;
        Ok(())
    }

    /// Whether `key` is `slug`'s current key. Takes the same time for a wrong key, a revoked key
    /// and an unknown world. The key of a world whose operator the hub's operator has disabled
    /// works for nothing until the operator is enabled again.
    #[must_use]
    pub fn authenticate(&self, slug: &str, key: &str) -> bool {
        let stored = self.live_key(slug).and_then(|h| unhex(&h));
        let presented = digest(key);
        let matches = same_digest(&presented, &stored.unwrap_or([0u8; 32]));
        stored.is_some() && matches
    }

    /// The digest of `slug`'s key while it may be used: none for a world with no key, and none
    /// while the world's operator is disabled.
    pub(crate) fn live_key(&self, slug: &str) -> Option<String> {
        self.db
            .query_row(
                "SELECT s.key_hash FROM servers s LEFT JOIN operators o ON o.id = s.operator_id
                 WHERE s.slug = ? AND o.disabled_at IS NULL",
                [slug.to_ascii_lowercase()],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()
            .ok()
            .flatten()
            .flatten()
    }

    /// Records the ownership review of an imported listing and issues its first key.
    ///
    /// # Errors
    /// 400 for too little evidence, 404, 409 when already managed.
    pub fn claim(&self, slug: &str, evidence: &str) -> Result<(Value, String)> {
        let evidence = evidence.trim();
        if evidence.chars().count() < 10 || evidence.chars().count() > 2000 {
            return Err(RegistryError::bad(
                "Record 10-2000 characters describing the ownership evidence you reviewed",
            ));
        }
        if !crate::text::is_plain(evidence, true) {
            return Err(RegistryError::bad(
                "evidence: no control, bidirectional or zero-width characters",
            ));
        }
        let row = self.row(slug)?.ok_or_else(RegistryError::not_found)?;
        if row.registry["state"] != "unclaimed" {
            return Err(RegistryError::conflict("Listing is already managed"));
        }
        if row.is_locked() {
            return Err(RegistryError::conflict(
                "This listing is locked: unlock it before it is claimed",
            ));
        }
        let mut registry = row.registry.clone();
        registry["state"] = json!("managed");
        registry["verifiedAt"] = json!(iso(self.now()));
        registry["verification"] =
            json!({ "method": "review", "verifiedAt": iso(self.now()), "endpointVerified": true });
        self.db.execute(
            "UPDATE servers SET registry = ? WHERE id = ?",
            params![registry.to_string(), row.id],
        )?;
        self.event(
            &row.id,
            "ownership-reviewed",
            &json!({ "evidence": evidence }),
        )?;
        // Any operator's claim of it is overtaken.
        self.db.execute(
            "UPDATE verifications SET state = 'closed' WHERE server_id = ? AND state = 'open'",
            [&row.id],
        )?;
        let key = self.rotate_key(slug)?;
        Ok((self.server(slug)?.expect("present"), key))
    }

    /// Sets a world's federation flag. `on` needs a managed world whose slug is short enough to be
    /// a network address.
    ///
    /// # Errors
    /// 404, or 409 when the world cannot be on the network.
    pub fn set_federation(&self, slug: &str, flag: Federation) -> Result<()> {
        self.set_federation_by(slug, flag, None)
    }

    /// [`Self::set_federation`], by `actor` ([`Self::event_by`]).
    pub(crate) fn set_federation_by(
        &self,
        slug: &str,
        flag: Federation,
        actor: Option<&str>,
    ) -> Result<()> {
        let row = self.row(slug)?.ok_or_else(RegistryError::not_found)?;
        if flag == Federation::On {
            if row.registry["state"] == "unclaimed" {
                return Err(RegistryError::conflict(
                    "Claim this listing before it joins the network",
                ));
            }
            if row.state() == "pending" {
                return Err(RegistryError::conflict(
                    "Verify this world before it joins the network",
                ));
            }
            if row.is_locked() {
                return Err(RegistryError::conflict(
                    "This world is locked: the hub's operator revoked it",
                ));
            }
            if row.is_unlisted() {
                return Err(RegistryError::conflict(
                    "This world is unlisted: the hub's operator took it out of the lists",
                ));
            }
            if !is_network_slug(&row.slug) {
                return Err(RegistryError::conflict(
                    "A world on the network needs a slug of at most 12 characters that is not reserved",
                ));
            }
        }
        self.db.execute(
            "UPDATE servers SET federation = ? WHERE id = ?",
            params![flag.word(), row.id],
        )?;
        self.event_by(
            &row.id,
            "federation",
            actor,
            &json!({ "flag": flag.word() }),
        )?;
        Ok(())
    }

    /// A world's federation flag and display name.
    ///
    /// # Errors
    /// Database errors only.
    pub fn federation(&self, slug: &str) -> Result<Option<(String, Federation)>> {
        Ok(self.row(slug)?.map(|r| (r.metadata.name, r.federation)))
    }

    /// Records a heartbeat from a world. For an observed endpoint, `source` is the address it came
    /// from, which becomes the world's address.
    ///
    /// # Errors
    /// 400 for a bad heartbeat or a missing source, 404, 409 for an unclaimed listing.
    pub fn heartbeat(&self, slug: &str, h: &Heartbeat, source: Option<&str>) -> Result<Value> {
        h.check()?;
        let row = self.row(slug)?.ok_or_else(RegistryError::not_found)?;
        if row.registry["state"] == "unclaimed" {
            return Err(RegistryError::conflict(
                "Unclaimed listings cannot publish telemetry",
            ));
        }
        if row.state() == "pending" {
            return Err(RegistryError::conflict(
                "Verify this world before it reports",
            ));
        }
        if let Endpoint::Observed { port, .. } = row.metadata.endpoint {
            // The address heartbeats come from becomes the world's: a public one only.
            let Some(addr) = source.filter(|a| {
                a.parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| self.policy.allows(crate::forwarded::canonical(ip)))
            }) else {
                return Err(RegistryError::bad(
                    "Observed endpoint requires a public address of the configured family",
                ));
            };
            if row.registry["observedAddress"] != json!(addr) {
                let mut registry = row.registry.clone();
                registry["observedAddress"] = json!(addr);
                registry["endpointUpdatedAt"] = json!(iso(self.now()));
                self.db.execute(
                    "UPDATE servers SET registry = ? WHERE id = ?",
                    params![registry.to_string(), row.id],
                )?;
                self.event(
                    &row.id,
                    "endpoint-observed",
                    &json!({ "address": addr, "port": port }),
                )?;
            }
        }
        self.db.execute(
            "UPDATE servers SET heartbeat = ?, last_seen_at = ? WHERE id = ?",
            params![
                serde_json::to_string(h).expect("serialisable"),
                iso(self.now()),
                row.id
            ],
        )?;
        Ok(self.server(slug)?.expect("present")["status"].clone())
    }

    fn status(&self, row: &Row, at: SystemTime) -> Value {
        let expires = row
            .last_seen_at
            .as_deref()
            .and_then(parse_iso)
            .map(|t| t + Duration::from_secs(HEARTBEAT_TTL_SECS));
        let fresh = expires.is_some_and(|e| e > at);
        let h = row.heartbeat.as_ref();
        json!({
            "state": match h { None => "unknown", Some(h) if fresh => h.state.as_str(), Some(_) => "stale" },
            "lastSeenAt": row.last_seen_at,
            "expiresAt": expires.map(iso),
            "playersOnline": if fresh { h.and_then(|h| h.players_online) } else { None },
            "message": if fresh { h.map_or("", |h| h.message.as_str()) } else { "" },
            "source": "server-heartbeat",
            "playersKind": h.map_or("unknown", |h| h.players_kind.as_str()),
            "reachability": "unknown",
            "reported": h.map(|h| json!({
                "state": h.state, "playersOnline": h.players_online,
                "emulatorVersion": h.emulator_version, "uptimeSeconds": h.uptime_seconds,
                "adapter": h.adapter,
            })),
        })
    }

    fn render(&self, row: &Row, at: SystemTime) -> Value {
        let m = &row.metadata;
        let address = match &m.endpoint {
            Endpoint::Manual { address, .. } => Some(address.clone()),
            Endpoint::Observed { .. } => {
                row.registry["observedAddress"].as_str().map(str::to_owned)
            }
        };
        let (mode, port) = match &m.endpoint {
            Endpoint::Manual { port, .. } => ("manual", *port),
            Endpoint::Observed { port, .. } => ("observed", *port),
        };
        let source = match (&m.endpoint, &address) {
            (Endpoint::Observed { .. }, Some(_)) => json!("heartbeat-source"),
            (Endpoint::Observed { .. }, None) => Value::Null,
            _ if row.registry["state"] == "unclaimed" && !row.registry["provenance"].is_null() => {
                json!("community-list")
            }
            _ => json!("operator"),
        };
        let mut v = serde_json::to_value(m).expect("serialisable");
        let ip = address
            .as_deref()
            .filter(|a| a.parse::<std::net::IpAddr>().is_ok())
            .map(str::to_owned);
        let software = [Some(m.emulator.family.clone()), m.emulator.version.clone()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        let family = match &m.endpoint {
            Endpoint::Observed { address_family, .. } => Value::String(address_family.clone()),
            Endpoint::Manual { .. } => Value::Null,
        };
        let extra = json!({
            "id": row.id,
            "endpoint": {
                "mode": mode, "address": address, "port": port, "transport": "udp",
                "addressFamily": family,
                "source": source, "updatedAt": row.registry["endpointUpdatedAt"],
            },
            "ownership": {
                "state": row.registry["state"], "verifiedAt": row.registry["verifiedAt"],
                "endpointVerified": row.registry["verification"]["endpointVerified"].as_bool().unwrap_or(false),
            },
            "provenance": row.registry["provenance"],
            "federation": row.federation.word(),
            "ip": ip, "port": port, "software": software, "transport": "udp",
            "createdAt": row.created_at,
            "status": self.status(row, at),
            "links": {
                "web": format!("{}/worlds/?q={}", self.site, row.slug),
                "status": format!("{}/v1/servers/{}/status", self.public_url, row.slug),
                "resolve": format!("{}/v1/resolve/{}", self.public_url, row.slug),
            },
        });
        if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            for (k, val) in e {
                o.insert(k.clone(), val.clone());
            }
        }
        v
    }

    /// One world as the API shows it.
    ///
    /// # Errors
    /// Database errors only.
    pub fn server(&self, slug: &str) -> Result<Option<Value>> {
        let at = self.now();
        Ok(self.row(slug)?.map(|r| self.render(&r, at)))
    }

    /// One world as the public API shows it: none for a `pending` world.
    ///
    /// # Errors
    /// Database errors only.
    pub fn public_server(&self, slug: &str) -> Result<Option<Value>> {
        let at = self.now();
        Ok(self
            .row(slug)?
            .filter(Row::is_public)
            .map(|r| self.render(&r, at)))
    }

    /// One world as the operator API shows it ([`Self::render`]).
    pub(crate) fn render_row(&self, row: &Row) -> Value {
        self.render(row, self.now())
    }

    /// Every world, by slug.
    ///
    /// # Errors
    /// Database errors only.
    pub fn all(&self) -> Result<Vec<Value>> {
        let at = self.now();
        Ok(self
            .public_rows()?
            .iter()
            .map(|r| self.render(r, at))
            .collect())
    }

    /// Every world, `pending` ones included, by slug: for the hub's operator.
    ///
    /// # Errors
    /// Database errors only.
    pub fn everything(&self) -> Result<Vec<Value>> {
        let at = self.now();
        Ok(self
            .all_rows()?
            .iter()
            .map(|r| self.render(r, at))
            .collect())
    }

    /// A page of worlds matching `q` (in name, slug or description) and `ruleset`: those reporting
    /// online first, then by players, then by slug.
    ///
    /// # Errors
    /// Database errors only.
    pub fn list(&self, q: &str, ruleset: &str, limit: usize, offset: usize) -> Result<Value> {
        self.list_filtered(q, ruleset, "", limit, offset)
    }

    /// [`Self::list`], only the worlds whose federation flag is `federation` when it names one
    /// (`on`, `off`, `unset`).
    ///
    /// # Errors
    /// Database errors only.
    pub fn list_filtered(
        &self,
        q: &str,
        ruleset: &str,
        federation: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Value> {
        Ok(page_of(&self.all()?, q, ruleset, federation, limit, offset))
    }

    /// The network's totals: worlds by state and players online among those reporting.
    ///
    /// # Errors
    /// Database errors only.
    pub fn network_status(&self) -> Result<Value> {
        Ok(totals_of(&self.all()?, self.now()))
    }
}

/// A page of `worlds` (rendered, as [`Registry::all`] gives them) matching `q` (in name, slug or
/// description), `ruleset` and `federation` (`on`, `off`, `unset`) where they name one: those
/// reporting online first, then by players, then by slug.
#[must_use]
pub fn page_of(
    worlds: &[Value],
    q: &str,
    ruleset: &str,
    federation: &str,
    limit: usize,
    offset: usize,
) -> Value {
    let federation: Option<Federation> = federation.parse().ok();
    let q = q.trim().to_lowercase();
    let text = |v: &Value, k: &str| v[k].as_str().unwrap_or("").to_owned();
    let mut rows: Vec<Value> = worlds
        .iter()
        .filter(|v| {
            (q.is_empty()
                || format!(
                    "{} {} {}",
                    text(v, "name"),
                    text(v, "slug"),
                    text(v, "description")
                )
                .to_lowercase()
                .contains(&q))
                && (ruleset.is_empty() || text(v, "ruleset") == ruleset)
                && federation.is_none_or(|f| text(v, "federation") == f.word())
        })
        .cloned()
        .collect();
    let rank = |v: &Value| match v["status"]["state"].as_str() {
        Some("online") => 0,
        Some("degraded") => 1,
        Some("maintenance") => 2,
        Some("offline") => 3,
        _ => 4,
    };
    rows.sort_by(|a, b| {
        rank(a)
            .cmp(&rank(b))
            .then_with(|| {
                let p = |v: &Value| v["status"]["playersOnline"].as_i64().unwrap_or(-1);
                p(b).cmp(&p(a))
            })
            .then_with(|| a["slug"].as_str().cmp(&b["slug"].as_str()))
    });
    let total = rows.len();
    let data: Vec<Value> = rows.into_iter().skip(offset).take(limit).collect();
    json!({
        "data": data, "total": total, "limit": limit, "offset": offset,
        "nextOffset": (offset + limit < total).then_some(offset + limit),
    })
}

/// The network's totals over `worlds` (rendered): worlds by state and players online among those
/// reporting, at `at`.
#[must_use]
pub fn totals_of(worlds: &[Value], at: SystemTime) -> Value {
    let mut counts = serde_json::Map::new();
    for s in [
        "online",
        "degraded",
        "maintenance",
        "offline",
        "stale",
        "unknown",
    ] {
        counts.insert(s.to_owned(), json!(0));
    }
    let (mut players, mut reporting, mut demo, mut federated) = (0u64, 0u64, 0u64, 0u64);
    for v in worlds {
        let st = &v["status"];
        let state = st["state"].as_str().unwrap_or("unknown").to_owned();
        if let Some(c) = counts.get_mut(&state) {
            *c = json!(c.as_u64().unwrap_or(0) + 1);
        }
        if let Some(p) = st["playersOnline"].as_u64() {
            players += p;
            reporting += 1;
        }
        demo += u64::from(v["isDemo"].as_bool().unwrap_or(false));
        federated += u64::from(v["federation"] == Federation::On.word());
    }
    json!({
        "generatedAt": iso(at), "total": worlds.len(), "counts": counts,
        "playersOnline": players, "reportingServers": reporting, "demoServers": demo,
        "federatedServers": federated, "heartbeatTtlSeconds": HEARTBEAT_TTL_SECS,
    })
}

impl Registry {
    /// The registry's audit log for a world, newest first (the last 100).
    ///
    /// # Errors
    /// 404.
    pub fn audit(&self, slug: &str) -> Result<Value> {
        let row = self.row(slug)?.ok_or_else(RegistryError::not_found)?;
        self.events_of(&row.id, 100, None)
    }

    /// Writes a consistent copy of the registry to `path` (a new file), while the hub runs: one
    /// transaction's view, whatever is being written meanwhile.
    ///
    /// # Errors
    /// 409 when `path` exists; database errors.
    pub fn backup(&self, path: &str) -> Result<()> {
        if std::path::Path::new(path).exists() {
            return Err(RegistryError::conflict(format!(
                "{path} exists: name a new file"
            )));
        }
        self.db.execute("VACUUM INTO ?", [path])?;
        Ok(())
    }

    /// A world's last `limit` registry events, newest first, each with who did it: only those by
    /// `only`, when it names an actor.
    pub(crate) fn events_of(
        &self,
        server_id: &str,
        limit: usize,
        only: Option<&str>,
    ) -> Result<Value> {
        let mut stmt = self.db.prepare(
            "SELECT kind, received_at, details, actor FROM registry_events
             WHERE server_id = ?1 AND (?3 IS NULL OR actor = ?3) ORDER BY id DESC LIMIT ?2",
        )?;
        let events: Vec<Value> = stmt
            .query_map(params![server_id, i64::try_from(limit).unwrap_or(100), only], |r| {
                let actor: Option<String> = r.get(3)?;
                Ok(json!({
                    "kind": r.get::<_, String>(0)?,
                    "at": r.get::<_, String>(1)?,
                    "actor": actor.unwrap_or_else(|| "hub-operator".to_owned()),
                    "details": serde_json::from_str::<Value>(&r.get::<_, String>(2)?).unwrap_or(Value::Null),
                }))
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(Value::Array(events))
    }

    /// Imports community-list entries: a listing already imported is updated when its source
    /// changed, a managed listing is never overwritten, and a slug held by another identity is a
    /// conflict. A dry run (`apply` false) changes nothing; an applied import with any conflict
    /// changes nothing either.
    ///
    /// # Errors
    /// 409 when applying an import that has conflicts.
    pub fn import_community(
        &mut self,
        entries: &[CommunityEntry],
        apply: bool,
    ) -> Result<Vec<PlanItem>> {
        self.import_community_with(
            entries,
            &ImportOptions {
                apply,
                ..ImportOptions::default()
            },
        )
    }

    /// [`Self::import_community`] with its options: conflicts skipped rather than stopping the
    /// import, and unclaimed listings the list no longer holds taken out of the public lists after
    /// [`ImportOptions::retire_after`] applied imports in a row, and put back when they return.
    /// A listing the hub's operator unlisted, and a managed one, are never listed or unlisted here.
    ///
    /// # Errors
    /// 409 when applying an import that has conflicts and does not skip them.
    #[allow(clippy::too_many_lines)]
    pub fn import_community_with(
        &mut self,
        entries: &[CommunityEntry],
        options: &ImportOptions,
    ) -> Result<Vec<PlanItem>> {
        let apply = options.apply;
        let now = iso(self.now());
        // An applied import takes the write lock at once, so a write the serving hub makes
        // between its reads and its writes cannot make it fail; the hub's own writes wait for it.
        let tx = if apply {
            self.db
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?
        } else {
            self.db.transaction()?
        };
        let mut plan = Vec::new();
        for e in entries {
            let existing: Option<(String, String, String, String)> = tx
                .query_row(
                    "SELECT id, slug, registry, metadata FROM servers WHERE json_extract(registry, '$.provenance.externalId') = ?",
                    [&e.external_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()?;
            let slug = existing
                .as_ref()
                .map_or_else(|| e.registration.slug.clone(), |x| x.1.clone());
            let mut item = PlanItem {
                external_id: e.external_id.clone(),
                slug: slug.clone(),
                name: e.registration.name.clone(),
                action: String::new(),
                reason: None,
                relisted: false,
            };
            let old: Option<Value> = existing
                .as_ref()
                .and_then(|x| serde_json::from_str(&x.2).ok());
            if old.as_ref().is_some_and(|o| o["state"] == "managed") {
                item.action = "skip".into();
                item.reason = Some("Operator-managed listing; upstream cannot overwrite it".into());
                plan.push(item);
                continue;
            }
            // Back on the list: a listing this import took out goes back in, and a listing that
            // was counted missing starts its count again.
            let back = old
                .as_ref()
                .is_some_and(|o| o["unlistedBecause"] == GONE_UPSTREAM);
            let was_missing = old.as_ref().is_some_and(|o| !o["missingRuns"].is_null());
            let returned = |registry: &mut Value| {
                if let Some(o) = registry.as_object_mut() {
                    o.remove("missingRuns");
                    if back {
                        o.remove("unlisted");
                        o.remove("unlistedBecause");
                    }
                }
            };
            if old
                .as_ref()
                .is_some_and(|o| o["sourceHash"] == json!(e.source_hash))
            {
                item.action = "unchanged".into();
                if let (Some(x), Some(mut registry)) = (&existing, old.clone()) {
                    if back || was_missing {
                        returned(&mut registry);
                        tx.execute(
                            "UPDATE servers SET registry = ? WHERE id = ?",
                            params![registry.to_string(), x.0],
                        )?;
                    }
                    if back {
                        relisted_event(&tx, &x.0, &now)?;
                        item.relisted = true;
                    }
                }
                plan.push(item);
                continue;
            }
            let mut reg = e.registration.clone();
            reg.slug.clone_from(&slug);
            if let Endpoint::Manual { address, .. } = &reg.endpoint {
                if let Some(why) =
                    endpoint_refusal(&self.reserved_hosts, &self.domain, address, false)
                {
                    item.action = "conflict".into();
                    item.reason = Some(why);
                    plan.push(item);
                    continue;
                }
            }
            if existing.is_none() {
                let taken: Option<String> = tx
                    .query_row("SELECT id FROM servers WHERE slug = ?", [&slug], |r| {
                        r.get(0)
                    })
                    .optional()?;
                if taken.is_some() {
                    item.action = "conflict".into();
                    item.reason =
                        Some("Slug belongs to a different identity; no automatic merge".into());
                    plan.push(item);
                    continue;
                }
            }
            let same_endpoint = existing.as_ref().is_some_and(|x| {
                serde_json::from_str::<Registration>(&x.3).is_ok_and(|m| m.endpoint == reg.endpoint)
            });
            // What the import owns of the listing's record; the rest (the hub's operator's unlist
            // or lock, a claim under way) stays as it was.
            let mut registry = old.clone().unwrap_or_else(|| json!({}));
            let imported = json!({
                "state": "unclaimed", "verifiedAt": null,
                "provenance": {
                    "source": "community-list",
                    "url": crate::community::COMMUNITY_SOURCE,
                    "externalId": e.external_id,
                    "importedAt": now,
                    "legacyType": e.legacy_type,
                },
                "sourceHash": e.source_hash,
                "observedAddress": null,
                "endpointUpdatedAt": if same_endpoint {
                    old.as_ref().map_or(Value::Null, |o| o["endpointUpdatedAt"].clone())
                } else { json!(now) },
            });
            if let (Some(o), Some(i)) = (registry.as_object_mut(), imported.as_object()) {
                for (k, v) in i {
                    o.insert(k.clone(), v.clone());
                }
            }
            returned(&mut registry);
            let metadata = serde_json::to_string(&reg).expect("serialisable");
            let (id, action) = if let Some(x) = &existing {
                tx.execute(
                    "UPDATE servers SET metadata = ?, registry = ? WHERE id = ?",
                    params![metadata, registry.to_string(), x.0],
                )?;
                (x.0.clone(), "update")
            } else {
                let id = new_id();
                tx.execute(
                    "INSERT INTO servers(id, slug, metadata, key_hash, created_at, registry) VALUES (?, ?, ?, NULL, ?, ?)",
                    params![id, slug, metadata, now, registry.to_string()],
                )?;
                (id, "add")
            };
            tx.execute(
                "INSERT INTO registry_events(server_id, kind, received_at, details) VALUES (?, ?, ?, ?)",
                params![
                    id,
                    if action == "add" { "community-imported" } else { "community-updated" },
                    now,
                    json!({ "externalId": e.external_id }).to_string()
                ],
            )?;
            if back {
                relisted_event(&tx, &id, &now)?;
                item.relisted = true;
            }
            item.action = action.into();
            plan.push(item);
        }
        if let Some(limit) = options.retire_after {
            let on_list: std::collections::BTreeSet<&str> = entries
                .iter()
                .map(|e| e.external_id.as_str())
                .chain(options.also_on_list.iter().map(String::as_str))
                .collect();
            let imported: Vec<(String, String, String, String)> = {
                let mut q = tx.prepare(
                    "SELECT id, slug, metadata, registry FROM servers
                     WHERE json_extract(registry, '$.provenance.source') = 'community-list'
                       AND json_extract(registry, '$.state') = 'unclaimed'
                     ORDER BY slug",
                )?;
                let rows = q
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                rows
            };
            for (id, slug, metadata, registry) in imported {
                let Ok(mut registry) = serde_json::from_str::<Value>(&registry) else {
                    continue;
                };
                let external = registry["provenance"]["externalId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                // Gone already, or taken out by the hub's operator: nothing to count.
                if on_list.contains(external.as_str()) || registry["unlisted"] == true {
                    continue;
                }
                let runs = registry["missingRuns"].as_u64().unwrap_or(0) + 1;
                let name = serde_json::from_str::<Registration>(&metadata)
                    .map(|m| m.name)
                    .unwrap_or_default();
                let gone = runs >= u64::from(limit);
                registry["missingRuns"] = json!(runs);
                let reason =
                    format!("Missing from the community list for {runs} import(s) in a row");
                if gone {
                    registry["unlisted"] = json!(true);
                    registry["unlistedBecause"] = json!(GONE_UPSTREAM);
                }
                tx.execute(
                    "UPDATE servers SET registry = ? WHERE id = ?",
                    params![registry.to_string(), id],
                )?;
                if gone {
                    tx.execute(
                        "INSERT INTO registry_events(server_id, kind, received_at, details) VALUES (?, 'unlisted', ?, ?)",
                        params![id, now, json!({ "reason": reason }).to_string()],
                    )?;
                }
                plan.push(PlanItem {
                    external_id: external,
                    slug,
                    name,
                    action: if gone { "unlist" } else { "missing" }.into(),
                    reason: Some(reason),
                    relisted: false,
                });
            }
        }
        let conflicts = plan.iter().any(|p| p.action == "conflict");
        if apply && conflicts && !options.skip_conflicts {
            tx.rollback()?;
            return Err(RegistryError::conflict(
                "Import has identity conflicts. Nothing was changed; review the dry run.",
            ));
        }
        if apply {
            tx.commit()?;
        } else {
            tx.rollback()?;
        }
        Ok(plan)
    }

    /// Every saved channel, by name, as the hub wrote it.
    ///
    /// # Errors
    /// Database errors only.
    pub fn channels(&self) -> Result<Vec<(String, String)>> {
        let mut q = self
            .db
            .prepare("SELECT name, value FROM hub_channels ORDER BY name")?;
        let rows = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<Vec<(String, String)>, _>>()?;
        Ok(rows)
    }

    /// Saves one channel (`Some`), or forgets it (`None`).
    ///
    /// # Errors
    /// Database errors only.
    pub fn set_channel(&self, name: &str, value: Option<&str>) -> Result<()> {
        match value {
            Some(v) => self.db.execute(
                "INSERT INTO hub_channels(name, value) VALUES (?, ?) ON CONFLICT(name) DO UPDATE SET value = excluded.value",
                params![name, v],
            )?,
            None => self
                .db
                .execute("DELETE FROM hub_channels WHERE name = ?", [name])?,
        };
        Ok(())
    }

    /// Saves (`Some`) or forgets (`None`) each of `channels` in one transaction: one write to the
    /// disk however many changed, and none of them if any fails.
    ///
    /// # Errors
    /// Database errors only.
    pub fn set_channels(&mut self, channels: &[(String, Option<String>)]) -> Result<()> {
        let tx = self.db.transaction()?;
        {
            let mut upsert = tx.prepare(
                "INSERT INTO hub_channels(name, value) VALUES (?, ?) ON CONFLICT(name) DO UPDATE SET value = excluded.value",
            )?;
            let mut delete = tx.prepare("DELETE FROM hub_channels WHERE name = ?")?;
            for (name, value) in channels {
                match value {
                    Some(v) => upsert.execute(params![name, v])?,
                    None => delete.execute([name])?,
                };
            }
        }
        tx.commit()?;
        Ok(())
    }
}

impl crate::core::Directory for Registry {
    fn lookup(&self, slug: &str) -> Option<crate::core::DirEntry> {
        self.federation(slug)
            .ok()
            .flatten()
            .map(|(name, federation)| crate::core::DirEntry { name, federation })
    }

    fn key(&self, slug: &str) -> Option<String> {
        self.live_key(slug)
    }

    fn person_code(
        &self,
        who: &dereth_network_protocol::Address,
    ) -> std::result::Result<(String, u32), dereth_network_protocol::PersonOutcome> {
        Registry::person_code(self, who)
    }

    fn person_unlink(&self, who: &dereth_network_protocol::Address) -> crate::core::PersonAnswer {
        answered(Registry::person_unlink(self, who))
    }

    fn person_alts(
        &self,
        who: &dereth_network_protocol::Address,
        shown: bool,
    ) -> crate::core::PersonAnswer {
        answered(Registry::person_alts(self, who, shown))
    }

    fn person_befriend(
        &self,
        who: &dereth_network_protocol::Address,
        target: &dereth_network_protocol::Address,
    ) -> crate::core::PersonAnswer {
        answered(Registry::person_befriend(self, who, target))
    }

    fn person_unfriend(
        &self,
        who: &dereth_network_protocol::Address,
        target: &dereth_network_protocol::Address,
    ) -> crate::core::PersonAnswer {
        answered(Registry::person_unfriend(self, who, target))
    }

    fn person_carry(
        &self,
        who: &dereth_network_protocol::Address,
        targets: &[dereth_network_protocol::Address],
    ) -> crate::core::PersonAnswer {
        answered(Registry::person_carry(self, who, targets))
    }

    fn returns_sent(&self, who: &dereth_network_protocol::Address) {
        if let Err(e) = Registry::returns_sent(self, who) {
            crate::server::log("error", &format!("friends handed back: {}", e.message));
        }
    }

    fn person_characters(
        &self,
        target: &dereth_network_protocol::Address,
    ) -> Vec<dereth_network_protocol::Address> {
        Registry::person_characters(self, target).unwrap_or_else(|e| {
            crate::server::log("error", &format!("a person's characters: {}", e.message));
            Vec::new()
        })
    }

    fn person_view(&self, who: &dereth_network_protocol::Address) -> crate::core::PersonView {
        Registry::person_view(self, who).unwrap_or_else(|e| {
            crate::server::log("error", &format!("a person view: {}", e.message));
            crate::core::PersonView::default()
        })
    }
}

/// A person request's answer, or `unavailable` (logged) when the registry failed.
fn answered(r: Result<crate::core::PersonAnswer>) -> crate::core::PersonAnswer {
    r.unwrap_or_else(|e| {
        crate::server::log("error", &format!("a person request: {}", e.message));
        crate::core::PersonAnswer::unchanged(dereth_network_protocol::PersonOutcome::Unavailable)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn clock() -> (Arc<Mutex<SystemTime>>, Clock) {
        let t = Arc::new(Mutex::new(UNIX_EPOCH + Duration::from_secs(1_790_000_000)));
        let c = Arc::clone(&t);
        (t, Box::new(move || *c.lock().expect("clock")))
    }

    fn reg(slug: &str) -> Registration {
        Registration::from_json(&json!({
            "slug": slug, "name": "Test World", "description": "A test.",
            "ruleset": "PvE",
            "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
            "emulator": { "family": "ACE" },
        }))
        .expect("a registration")
    }

    /// The commits in a WAL file: frames whose header gives the database's size after a commit.
    fn wal_commits(path: &std::path::Path) -> usize {
        let wal = std::fs::read(path).unwrap_or_default();
        if wal.len() < 32 {
            return 0;
        }
        let page = u32::from_be_bytes(wal[8..12].try_into().expect("a page size")) as usize;
        wal[32..]
            .chunks(24 + page)
            .filter(|f| f.len() == 24 + page && f[4..8] != [0, 0, 0, 0])
            .count()
    }

    /// Spec: hub: the channels changed since the last save are written together, one commit however
    /// many changed.
    #[test]
    fn the_changed_channels_are_saved_in_one_commit() {
        let dir = std::env::temp_dir().join(format!("hub-channels-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let db = dir.join("hub.sqlite");
        let (_, c) = clock();
        let mut r =
            Registry::open(db.to_str().expect("a path"), "dereth.network", c).expect("open");
        r.set_channel("gone", Some("{}")).expect("saved");
        let wal = dir.join("hub.sqlite-wal");
        let before = wal_commits(&wal);
        let mut rows: Vec<(String, Option<String>)> = (0..50)
            .map(|i| (format!("channel{i}"), Some(format!("{{\"n\":{i}}}"))))
            .collect();
        rows.push(("gone".into(), None));
        r.set_channels(&rows).expect("saved");
        assert_eq!(wal_commits(&wal), before + 1, "one commit for 51 changes");
        let saved = r.channels().expect("read");
        assert_eq!(saved.len(), 50);
        assert!(
            saved.iter().all(|(n, _)| n != "gone"),
            "a channel ended is forgotten"
        );
        drop(r);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Spec: registry: a registry of the schema before friends were handed back (version 6) is
    /// moved forward, keeping its persons, and hands friends back once moved.
    #[test]
    fn a_version_six_registry_gains_the_friends_handed_back() {
        let dir = std::env::temp_dir().join(format!("hub-schema-6-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let db = dir.join("hub.sqlite");
        let path = db.to_str().expect("a path");
        let ann = dereth_network_protocol::Address::parse("Ann@coldeve").expect("an address");
        let bob = dereth_network_protocol::Address::parse("Bob@frost").expect("an address");
        {
            let (_, c) = clock();
            let r = Registry::open(path, "dereth.network", c).expect("opened");
            r.register(&reg("coldeve")).expect("registered");
            r.set_federation("coldeve", Federation::On).expect("on");
            let op = r.signed_in("100", "Ann").expect("signed in");
            let (code, _) = r.person_code(&ann).expect("a code");
            r.redeem_link_code(&op, &code).expect("linked");
            r.person_befriend(&ann, &bob).expect("befriended");
            r.db.execute_batch("DROP TABLE person_returns; PRAGMA user_version = 6;")
                .expect("the schema before");
        }
        let (_, c) = clock();
        let r = Registry::open(path, "dereth.network", c).expect("reopened");
        let version: i64 =
            r.db.query_row("PRAGMA user_version", [], |row| row.get(0))
                .expect("read");
        assert_eq!(version, SCHEMA_VERSION);
        assert!(
            r.person_view(&ann).expect("a view").linked,
            "the link is kept"
        );
        r.person_unlink(&ann).expect("unlinked");
        let back = r.person_view(&ann).expect("a view");
        assert_eq!(back.persons.len(), 1);
        assert_eq!(back.persons[0].handle, bob);
        drop(r);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Spec: registry: a newer hub moves an older registry's schema forward and keeps what it
    /// holds; it refuses a registry newer than itself.
    #[test]
    fn an_older_registry_is_moved_forward_and_a_newer_one_refused() {
        let dir = std::env::temp_dir().join(format!("hub-schema-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let db = dir.join("hub.sqlite");
        let path = db.to_str().expect("a path");
        {
            // The registry as the hub left it before channels had their name, with a world.
            let db = Connection::open(path).expect("open");
            db.execute_batch(
                "CREATE TABLE servers (
                   id TEXT PRIMARY KEY, slug TEXT NOT NULL UNIQUE, metadata TEXT NOT NULL,
                   key_hash TEXT, created_at TEXT NOT NULL, last_seen_at TEXT, heartbeat TEXT,
                   registry TEXT NOT NULL DEFAULT '{}', federation TEXT NOT NULL DEFAULT 'unset');
                 CREATE TABLE registry_events (
                   id INTEGER PRIMARY KEY,
                   server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                   kind TEXT NOT NULL, received_at TEXT NOT NULL, details TEXT NOT NULL);
                 CREATE TABLE hub_state (name TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE hub_clubs (name TEXT PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO hub_clubs(name, value) VALUES ('oldguard', '{}');
                 PRAGMA user_version = 2;",
            )
            .expect("an older schema");
            let r = reg("coldeve");
            db.execute(
                "INSERT INTO servers(id, slug, metadata, key_hash, created_at, registry) VALUES ('w1', 'coldeve', ?, NULL, '2026-01-01T00:00:00.000Z', '{\"state\":\"managed\"}')",
                [serde_json::to_string(&r).expect("json")],
            )
            .expect("a world");
        }
        let (_, c) = clock();
        let r = Registry::open(path, "dereth.network", c).expect("reopened");
        assert_eq!(
            r.channels().expect("read"),
            vec![("oldguard".to_owned(), "{}".to_owned())]
        );
        let w = r.server("coldeve").expect("read").expect("the world");
        assert_eq!(w["ownership"]["state"], "managed", "the world is kept");
        assert!(r
            .operators()
            .expect("read")
            .as_array()
            .is_some_and(Vec::is_empty));
        r.db.execute_batch(&format!("PRAGMA user_version = {};", SCHEMA_VERSION + 1))
            .expect("a newer schema");
        drop(r);
        let (_, c) = clock();
        assert_eq!(
            Registry::open(path, "dereth.network", c)
                .map(|_| ())
                .map_err(|e| e.status),
            Err(500)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Spec: registry: a copy of the registry taken while it runs holds what it held, and opens as
    /// a registry.
    #[test]
    fn a_backup_taken_while_the_registry_is_open_holds_everything() {
        let dir = std::env::temp_dir().join(format!("hub-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let live = dir.join("hub.sqlite");
        let copy = dir.join("copy.sqlite");
        let (_, c) = clock();
        let r = Registry::open(live.to_str().expect("a path"), "dereth.network", c).expect("open");
        let (_, key) = r.register(&reg("coldeve")).expect("registered");
        r.set_channel("oldguard", Some("{}")).expect("a channel");
        r.backup(copy.to_str().expect("a path")).expect("backed up");
        assert_eq!(
            r.backup(copy.to_str().expect("a path"))
                .map_err(|e| e.status),
            Err(409),
            "never over a file"
        );
        drop(r);
        let (_, c) = clock();
        let b = Registry::open(copy.to_str().expect("a path"), "dereth.network", c)
            .expect("the copy opens");
        assert!(b.authenticate("coldeve", &key));
        assert_eq!(b.channels().expect("read").len(), 1);
        drop(b);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Spec: registry: a world's name holds no control character, line break or markup bracket.
    #[test]
    fn a_world_name_with_a_line_break_or_markup_is_refused() {
        let (_, c) = clock();
        let r = Registry::open(":memory:", "dereth.network", c).expect("open");
        for bad in [
            "Coldeve\nYou have been banned",
            "<Tell:IIDString:1:Admin>Admin<\\Tell>",
        ] {
            let mut x = reg("coldeve");
            x.name = bad.to_owned();
            assert!(r.register(&x).is_err(), "{bad:?}");
        }
        assert!(r.register(&reg("coldeve")).is_ok());
    }

    /// Spec: registry: a key is issued once, kept as a digest, and stops working when rotated or
    /// revoked.
    #[test]
    fn a_key_authenticates_its_world_until_it_is_rotated_or_revoked() {
        let (_, c) = clock();
        let r = Registry::open(":memory:", "dereth.network", c).expect("open");
        let (_, key) = r.register(&reg("coldeve")).expect("registered");
        assert!(is_key_shape(&key));
        assert!(r.authenticate("coldeve", &key));
        assert!(!r.authenticate("coldeve", &new_key()));
        assert!(
            !r.authenticate("frost", &key),
            "a key is for its own world only"
        );
        let stored: String =
            r.db.query_row("SELECT key_hash FROM servers", [], |x| x.get(0))
                .expect("hash");
        assert!(!stored.contains(&key[5..]), "only the digest is kept");
        let rotated = r.rotate_key("coldeve").expect("rotated");
        assert!(!r.authenticate("coldeve", &key) && r.authenticate("coldeve", &rotated));
        r.revoke_key("coldeve").expect("revoked");
        assert!(!r.authenticate("coldeve", &rotated));
        assert_eq!(
            r.register(&reg("coldeve")).map(|_| ()),
            Err(RegistryError::conflict(
                "The slug \"coldeve\" is already registered"
            ))
        );
    }

    /// Spec: registry: a heartbeat keeps a world's status fresh for three minutes.
    #[test]
    fn a_heartbeat_is_fresh_for_three_minutes_then_stale() {
        let (t, c) = clock();
        let r = Registry::open(":memory:", "dereth.network", c).expect("open");
        r.register(&reg("coldeve")).expect("registered");
        assert_eq!(
            r.server("coldeve").expect("q").expect("w")["status"]["state"],
            "unknown"
        );
        let h = Heartbeat {
            state: "online".into(),
            players_online: Some(27),
            message: String::new(),
            players_kind: "connected-characters".into(),
            emulator_version: None,
            uptime_seconds: Some(5),
            adapter: None,
        };
        r.heartbeat("coldeve", &h, None).expect("accepted");
        let s = r.server("coldeve").expect("q").expect("w");
        assert_eq!(s["status"]["state"], "online");
        assert_eq!(s["status"]["playersOnline"], 27);
        *t.lock().expect("clock") += Duration::from_secs(HEARTBEAT_TTL_SECS + 1);
        let s = r.server("coldeve").expect("q").expect("w");
        assert_eq!(s["status"]["state"], "stale");
        assert!(s["status"]["playersOnline"].is_null());
        let bad = Heartbeat {
            state: "offline".into(),
            ..h
        };
        assert!(
            r.heartbeat("coldeve", &bad, None).is_err(),
            "offline with players"
        );
    }

    /// Spec: federation: the flag starts unset, and `on` needs a managed world with a network slug.
    #[test]
    fn the_federation_flag_starts_unset_and_on_needs_a_short_managed_slug() {
        let (_, c) = clock();
        let mut r = Registry::open(":memory:", "dereth.network", c).expect("open");
        r.register(&reg("coldeve")).expect("registered");
        r.register(&reg("a-very-long-world-name"))
            .expect("registered");
        assert_eq!(
            r.federation("coldeve").expect("q").map(|x| x.1),
            Some(Federation::Unset)
        );
        r.set_federation("coldeve", Federation::On).expect("on");
        assert_eq!(
            r.server("coldeve").expect("q").expect("w")["federation"],
            "on"
        );
        assert_eq!(
            r.set_federation("a-very-long-world-name", Federation::On)
                .map_err(|e| e.status),
            Err(409)
        );
        let entry = CommunityEntry {
            external_id: "59544be2-9435-400d-a589-30b2a39ed071".into(),
            registration: reg("listed"),
            source_hash: "h".into(),
            legacy_type: "PvE".into(),
        };
        r.import_community(&[entry], true).expect("imported");
        assert_eq!(
            r.set_federation("listed", Federation::On)
                .map_err(|e| e.status),
            Err(409),
            "an unclaimed listing cannot join"
        );
        r.set_federation("listed", Federation::Off)
            .expect("off is always allowed");
    }

    /// Spec: registry: an import is a dry run unless applied, never overwrites a managed listing
    /// and refuses a slug held by another identity.
    #[test]
    fn a_community_import_is_dry_unless_applied_and_respects_managed_listings() {
        let (_, c) = clock();
        let mut r = Registry::open(":memory:", "dereth.network", c).expect("open");
        let entry = |id: &str, slug: &str, hash: &str| CommunityEntry {
            external_id: id.into(),
            registration: reg(slug),
            source_hash: hash.into(),
            legacy_type: "PvE".into(),
        };
        let a = "00000000-0000-4000-8000-000000000001";
        let dry = r
            .import_community(&[entry(a, "alpha", "1")], false)
            .expect("dry run");
        assert_eq!(dry[0].action, "add");
        assert!(
            r.server("alpha").expect("q").is_none(),
            "a dry run changes nothing"
        );
        r.import_community(&[entry(a, "alpha", "1")], true)
            .expect("applied");
        let s = r.server("alpha").expect("q").expect("w");
        assert_eq!(s["ownership"]["state"], "unclaimed");
        assert_eq!(s["endpoint"]["source"], "community-list");
        let again = r
            .import_community(&[entry(a, "alpha", "1")], true)
            .expect("again");
        assert_eq!(again[0].action, "unchanged");
        let (_, key) = r
            .claim("alpha", "reviewed the operator's discord post")
            .expect("claimed");
        assert!(r.authenticate("alpha", &key));
        let managed = r
            .import_community(&[entry(a, "alpha", "2")], true)
            .expect("import");
        assert_eq!(managed[0].action, "skip");
        r.register(&reg("beta")).expect("registered");
        let clash = r.import_community(
            &[entry("00000000-0000-4000-8000-000000000002", "beta", "1")],
            true,
        );
        assert_eq!(clash.map_err(|e| e.status), Err(409));
    }

    fn listing(n: u32, slug: &str, hash: &str) -> CommunityEntry {
        CommunityEntry {
            external_id: format!("00000000-0000-4000-8000-{n:012}"),
            registration: reg(slug),
            source_hash: hash.into(),
            legacy_type: "PvE".into(),
        }
    }

    fn applied(retire_after: u32) -> ImportOptions {
        ImportOptions {
            apply: true,
            skip_conflicts: true,
            retire_after: Some(retire_after),
            also_on_list: Vec::new(),
        }
    }

    fn actions(plan: &[PlanItem]) -> Vec<(String, String)> {
        plan.iter()
            .map(|p| (p.slug.clone(), p.action.clone()))
            .collect()
    }

    fn pair(slug: &str, action: &str) -> (String, String) {
        (slug.to_owned(), action.to_owned())
    }

    /// Spec: registry: an import that skips conflicts applies everything else and reports the
    /// conflicts; one that does not changes nothing.
    #[test]
    fn an_import_that_skips_conflicts_applies_the_rest() {
        let (_, c) = clock();
        let mut r = Registry::open(":memory:", "dereth.network", c).expect("open");
        r.register(&reg("beta")).expect("registered");
        let list = [listing(1, "alpha", "1"), listing(2, "beta", "1")];
        let whole = r.import_community_with(
            &list,
            &ImportOptions {
                apply: true,
                ..ImportOptions::default()
            },
        );
        assert_eq!(whole.map_err(|e| e.status), Err(409));
        assert!(r.server("alpha").expect("q").is_none(), "nothing changed");
        let plan = r
            .import_community_with(&list, &applied(3))
            .expect("applied");
        assert_eq!(
            actions(&plan),
            [pair("alpha", "add"), pair("beta", "conflict")]
        );
        assert!(r.public_server("alpha").expect("q").is_some());
        let beta = r.server("beta").expect("q").expect("w");
        assert_eq!(
            beta["ownership"]["state"], "managed",
            "the slug's holder untouched"
        );
        assert!(beta["provenance"].is_null());
    }

    /// Spec: registry: an unclaimed listing missing from the list for the configured number of
    /// applied imports in a row is taken out of the public lists, with the reason in its log, and
    /// is listed again when it returns; a dry run counts nothing.
    #[test]
    fn a_listing_gone_from_the_list_is_unlisted_after_n_runs_and_relisted_on_return() {
        let (_, c) = clock();
        let mut r = Registry::open(":memory:", "dereth.network", c).expect("open");
        let (a, b) = (listing(1, "alpha", "1"), listing(2, "beta", "1"));
        r.import_community_with(&[a.clone(), b.clone()], &applied(3))
            .expect("imported");
        let dry = ImportOptions {
            apply: false,
            ..applied(3)
        };
        for _ in 0..3 {
            let plan = r
                .import_community_with(std::slice::from_ref(&a), &dry)
                .expect("dry");
            assert_eq!(
                actions(&plan),
                [pair("alpha", "unchanged"), pair("beta", "missing")]
            );
        }
        // Missing twice, back, then missing again: the count starts over.
        for _ in 0..2 {
            let plan = r
                .import_community_with(std::slice::from_ref(&a), &applied(3))
                .expect("run");
            assert_eq!(plan[1].action, "missing");
        }
        r.import_community_with(&[a.clone(), b.clone()], &applied(3))
            .expect("back");
        for run in 1..=3 {
            let plan = r
                .import_community_with(std::slice::from_ref(&a), &applied(3))
                .expect("run");
            let beta = plan.iter().find(|p| p.slug == "beta").expect("beta");
            let public = r.public_server("beta").expect("q").is_some();
            if run < 3 {
                assert_eq!(beta.action, "missing", "run {run}");
                assert!(public, "still listed after {run} run(s)");
            } else {
                assert_eq!(beta.action, "unlist");
                assert!(!public, "unlisted after the third run");
            }
        }
        let log = r.audit("beta").expect("log");
        let unlisted = log
            .as_array()
            .expect("events")
            .iter()
            .find(|e| e["kind"] == "unlisted")
            .expect("an unlisted event");
        assert!(
            unlisted["details"]["reason"]
                .as_str()
                .is_some_and(|s| s.contains("Missing from the community list")),
            "{unlisted}"
        );
        assert!(r.server("beta").expect("q").is_some(), "never deleted");
        let later = r
            .import_community_with(std::slice::from_ref(&a), &applied(3))
            .expect("run");
        assert_eq!(later.len(), 1, "an unlisted listing is counted no further");
        let back = r
            .import_community_with(&[a.clone(), b.clone()], &applied(3))
            .expect("back");
        let beta = back.iter().find(|p| p.slug == "beta").expect("beta");
        assert_eq!((beta.action.as_str(), beta.relisted), ("unchanged", true));
        assert!(
            r.public_server("beta").expect("q").is_some(),
            "listed again"
        );
        assert!(r
            .audit("beta")
            .expect("log")
            .as_array()
            .expect("events")
            .iter()
            .any(|e| e["kind"] == "relisted"));
        let quiet = r
            .import_community_with(&[a, b], &applied(3))
            .expect("again");
        assert!(quiet.iter().all(|p| !p.relisted && p.action == "unchanged"));
    }

    /// Spec: registry: an import never counts, unlists or relists a managed listing, nor relists
    /// one the hub's operator unlisted, and an update keeps the operator's unlisting.
    #[test]
    fn an_import_leaves_managed_and_operator_unlisted_listings_alone() {
        let (_, c) = clock();
        let mut r = Registry::open(":memory:", "dereth.network", c).expect("open");
        let (a, b, d) = (
            listing(1, "alpha", "1"),
            listing(2, "beta", "1"),
            listing(3, "delta", "1"),
        );
        r.import_community_with(&[a.clone(), b.clone(), d.clone()], &applied(1))
            .expect("imported");
        r.claim("beta", "reviewed the operator's post")
            .expect("claimed");
        r.set_listed("delta", false).expect("unlisted by hand");
        let before = r.server("beta").expect("q");
        for _ in 0..4 {
            let plan = r
                .import_community_with(std::slice::from_ref(&a), &applied(1))
                .expect("run");
            assert_eq!(actions(&plan), [pair("alpha", "unchanged")]);
        }
        assert_eq!(r.server("beta").expect("q"), before, "managed: untouched");
        assert!(r.public_server("beta").expect("q").is_some());
        let changed = listing(3, "delta", "2");
        let plan = r
            .import_community_with(&[a, b, changed], &applied(1))
            .expect("run");
        assert_eq!(
            actions(&plan),
            [
                pair("alpha", "unchanged"),
                pair("beta", "skip"),
                pair("delta", "update")
            ]
        );
        assert!(plan.iter().all(|p| !p.relisted));
        assert!(
            r.public_server("delta").expect("q").is_none(),
            "the operator's unlisting stays through an update"
        );
    }

    /// Spec: registry: a listing the import took out and the hub's operator puts back by hand
    /// counts its absence from the list afresh.
    #[test]
    fn a_listing_relisted_by_hand_counts_its_absence_afresh() {
        let (_, c) = clock();
        let mut r = Registry::open(":memory:", "dereth.network", c).expect("open");
        let (a, b) = (listing(1, "alpha", "1"), listing(2, "beta", "1"));
        r.import_community_with(&[a.clone(), b], &applied(2))
            .expect("imported");
        for _ in 0..2 {
            r.import_community_with(std::slice::from_ref(&a), &applied(2))
                .expect("run");
        }
        assert!(r.public_server("beta").expect("q").is_none());
        r.set_listed("beta", true).expect("relisted by hand");
        let plan = r.import_community_with(&[a], &applied(2)).expect("run");
        assert_eq!(
            actions(&plan),
            [pair("alpha", "unchanged"), pair("beta", "missing")]
        );
        assert!(r.public_server("beta").expect("q").is_some());
    }

    /// Spec: registry: a refused record the list still holds is never counted missing.
    #[test]
    fn a_refused_record_still_on_the_list_is_not_missing() {
        let (_, c) = clock();
        let mut r = Registry::open(":memory:", "dereth.network", c).expect("open");
        let (a, b) = (listing(1, "alpha", "1"), listing(2, "beta", "1"));
        r.import_community_with(&[a.clone(), b.clone()], &applied(1))
            .expect("imported");
        let plan = r
            .import_community_with(
                &[a],
                &ImportOptions {
                    also_on_list: vec![b.external_id],
                    ..applied(1)
                },
            )
            .expect("run");
        assert_eq!(actions(&plan), [pair("alpha", "unchanged")]);
        assert!(r.public_server("beta").expect("q").is_some());
    }

    /// Spec: registry: times are written as RFC 3339 UTC and read back exactly.
    #[test]
    fn times_are_written_in_rfc_3339_and_read_back() {
        let t = UNIX_EPOCH + Duration::from_millis(1_790_000_000_123);
        let s = iso(t);
        assert_eq!(s, "2026-09-21T14:13:20.123Z");
        assert_eq!(parse_iso(&s), Some(t));
        assert_eq!(iso(UNIX_EPOCH), "1970-01-01T00:00:00.000Z");
    }

    fn at(slug: &str, address: &str) -> Registration {
        let mut r = reg(slug);
        r.endpoint = Endpoint::Manual {
            address: address.to_owned(),
            port: 9000,
        };
        r
    }

    /// A registry whose hub is at `https://hub.test/api` and whose site is `https://site.test:8443`.
    fn deployed() -> Registry {
        let (_, c) = clock();
        let mut r = Registry::open(":memory:", "dereth.network", c).expect("open");
        r.set_web(
            Some("https://hub.test/api"),
            &[
                "https://site.test:8443".to_owned(),
                "https://other.test".to_owned(),
            ],
        );
        r
    }

    /// The names the hub and its site answer on, as a world's endpoint might spell them.
    const RESERVED: [&str; 6] = [
        "dereth.network",
        "hub.dereth.network",
        "API.dereth.network",
        "hub.test",
        "site.test",
        "other.test",
    ];

    /// Spec: hub: the hub's operator, on the command line, registers and moves a world to any name
    /// under the hub's own domain but the names the hub and its site answer on.
    #[test]
    fn the_hubs_operator_may_use_the_domain_but_not_its_reserved_names() {
        let r = deployed();
        r.register(&at("test-one", "test1.dereth.network"))
            .expect("the hub's operator registers under the domain");
        r.update(&at("test-one", "Test2.Dereth.Network"))
            .expect("and moves the world within it");
        for name in RESERVED {
            assert_eq!(
                r.register(&at("refused", name))
                    .map(|_| ())
                    .map_err(|e| e.status),
                Err(400),
                "{name}"
            );
            assert_eq!(
                r.update(&at("test-one", name))
                    .map(|_| ())
                    .map_err(|e| e.status),
                Err(400),
                "{name}"
            );
        }
    }

    /// Spec: hub: an operator who is not the hub's own registers no name under the hub's domain,
    /// nor one the hub or its site answers on; any other name is theirs to use.
    #[test]
    fn an_ordinary_operator_is_refused_any_name_under_the_domain() {
        let r = deployed();
        for name in RESERVED.iter().chain(&["test1.dereth.network"]) {
            assert_eq!(
                r.register_as(&at("refused", name), false)
                    .map(|_| ())
                    .map_err(|e| e.status),
                Err(400),
                "{name}"
            );
        }
        r.register_as(&at("theirs", "play.example.org"), false)
            .expect("a name of their own");
        assert_eq!(
            r.update_by(&at("theirs", "test1.dereth.network"), Some("op"), false)
                .map(|_| ())
                .map_err(|e| e.status),
            Err(400)
        );
        r.update_by(&at("theirs", "play2.example.org"), Some("op"), false)
            .expect("moved to another name of their own");
    }

    /// Spec: hub: a community listing under the hub's domain, or at a name the hub or its site
    /// answers on, is a conflict, never imported.
    #[test]
    fn an_import_refuses_a_listing_under_the_domain() {
        let mut r = deployed();
        let mut list = vec![listing(1, "outside", "1")];
        for (n, name) in RESERVED.iter().chain(&["test1.dereth.network"]).enumerate() {
            let mut e = listing(
                u32::try_from(n).expect("few") + 2,
                &format!("inside-{n}"),
                "1",
            );
            e.registration.endpoint = Endpoint::Manual {
                address: (*name).to_owned(),
                port: 9000,
            };
            list.push(e);
        }
        let plan = r
            .import_community_with(&list, &applied(3))
            .expect("applied");
        assert_eq!(plan[0].action, "add");
        for p in &plan[1..] {
            assert_eq!(p.action, "conflict", "{}", p.slug);
            assert!(r.server(&p.slug).expect("q").is_none());
        }
    }

    /// Spec: hub: a world's links are the site's world list searched for it and the hub's own
    /// status and resolve routes for it, at the configured addresses; the hub names no host for
    /// it under its domain.
    #[test]
    fn a_worlds_links_follow_the_hubs_and_the_sites_addresses() {
        let r = deployed();
        r.register(&reg("coldeve")).expect("registered");
        let w = r.server("coldeve").expect("q").expect("w");
        assert_eq!(
            w["links"],
            json!({
                "web": "https://site.test:8443/worlds/?q=coldeve",
                "status": "https://hub.test/api/v1/servers/coldeve/status",
                "resolve": "https://hub.test/api/v1/resolve/coldeve",
            })
        );
        assert!(w.get("fqdn").is_none());
        let (_, c) = clock();
        let plain = Registry::open(":memory:", "example.org", c).expect("open");
        plain.register(&reg("coldeve")).expect("registered");
        assert_eq!(
            plain.server("coldeve").expect("q").expect("w")["links"],
            json!({
                "web": "https://example.org/worlds/?q=coldeve",
                "status": "https://hub.example.org/v1/servers/coldeve/status",
                "resolve": "https://hub.example.org/v1/resolve/coldeve",
            }),
            "without them, the hub and the site are at the domain's own names"
        );
    }
}
