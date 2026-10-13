//! World operators: their accounts and sessions, the worlds they register or claim, verifying that
//! they run them, approval onto the network, keys, reports and the limits on all of it.
//!
//! An operator is whoever signs in with Discord; the hub keeps their Discord id and names, and
//! nothing else of Discord's. A session is 32 random bytes kept as their SHA-256. A world an
//! operator registers is `pending` (in no public answer) until it is verified; a community listing
//! an operator claims stays as it is until the claim is verified. A verification is a one-time
//! token, kept as its SHA-256, that the hub finds on a page on the world's own host, or that the
//! world sends with its key and then proves it answers at its game endpoint ([`crate::challenge`]);
//! the hub's operator may also vouch for a world on the command line. A verified world is approved
//! onto the network within daily allowances. Every limit is counted from the events this module records, so a
//! restart resets none of them. No key, token or session id is written anywhere but as a digest.
//!
//! Everything here is the registry's database; the HTTP side is [`crate::api`].

use std::net::IpAddr;
use std::time::{Duration, SystemTime};

use dereth_network_protocol::address::is_network_slug;
use dereth_network_protocol::Federation;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::registry::{
    digest, hex, iso, new_id, new_key, parse_iso, random_bytes, same_digest, unhex, Endpoint,
    Registration, Registry, RegistryError, Row,
};
use crate::settings::Setting;

type Result<T> = std::result::Result<T, RegistryError>;

/// How long a session lasts.
pub const SESSION_SECS: u64 = 30 * 86_400;
/// How long a verification token is good for.
pub const TOKEN_SECS: u64 = 7 * 86_400;

const HOUR: u64 = 3_600;
const DAY: u64 = 86_400;

/// The operator API's limits (`docs/network/05-hub.md` section 2.9). The hub's admins are held to
/// the hub's ceilings and to `REPORTS_PER_DAY`, and to none of the others.
pub mod limits {
    /// Registrations by one operator a day.
    pub const REGISTRATIONS_PER_DAY: usize = 3;
    /// Worlds and open claims one operator holds at once.
    pub const HELD: usize = 10;
    /// Open claims one operator holds at once.
    pub const OPEN_CLAIMS: usize = 3;
    /// Claims one operator opens a day, withdrawn or not.
    pub const CLAIMS_PER_DAY: usize = 5;
    /// New verification tokens for one world a day.
    pub const TOKENS_PER_DAY: usize = 10;
    /// Verification attempts for one world an hour.
    pub const ATTEMPTS_PER_HOUR: usize = 10;
    /// Page fetches for one operator an hour, and for the whole hub: a ceiling well above what
    /// operators' own limits let them reach together, so it is never what holds one back.
    pub const FETCHES_PER_HOUR: usize = 30;
    /// See [`FETCHES_PER_HOUR`].
    pub const HUB_FETCHES_PER_HOUR: usize = 3_000;
    /// Approvals onto the network for one operator a day.
    pub const APPROVALS_PER_DAY: usize = 3;
    /// New keys for one world a day.
    pub const KEYS_PER_DAY: usize = 10;
    /// Registration changes for one world a day.
    pub const UPDATES_PER_DAY: usize = 30;
    /// Reports by one operator a day, the hub's admins included.
    pub const REPORTS_PER_DAY: usize = 5;
}

/// What the server tells the registry about addresses. The approvals a day, the age a Discord
/// account needs and the days a world may stay `pending` are the registry's own settings
/// ([`crate::settings`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Policy {
    /// Whether loopback addresses count as public: for tests and a hub developed on one machine.
    pub loopback_allowed: bool,
}

/// When Discord made the account `discord_id`, from its snowflake (milliseconds since the start
/// of 2015 in its top 42 bits); none for an id that is not one.
#[must_use]
pub fn discord_account_made(discord_id: &str) -> Option<SystemTime> {
    const DISCORD_EPOCH_MS: u64 = 1_420_070_400_000;
    let id: u64 = discord_id.parse().ok()?;
    Some(SystemTime::UNIX_EPOCH + Duration::from_millis((id >> 22) + DISCORD_EPOCH_MS))
}

impl Policy {
    /// Whether an endpoint, or a page, may be at `ip`.
    #[must_use]
    pub fn allows(&self, ip: IpAddr) -> bool {
        crate::fetch::is_public(ip) || (self.loopback_allowed && ip.is_loopback())
    }
}

/// A signed-in operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operator {
    pub id: String,
    pub discord_id: String,
    pub name: String,
    pub signed_in_at: Option<String>,
}

impl Operator {
    /// As `GET /v1/operator` shows it.
    #[must_use]
    pub fn to_json(&self, admin: bool) -> Value {
        json!({
            "id": self.id, "discordId": self.discord_id, "name": self.name,
            "signedInAt": self.signed_in_at, "admin": admin,
        })
    }
}

/// How a world was verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// The token on a page on the world's host.
    Page,
    /// The token sent by the world with its key, and the hub's challenge answered at its game
    /// endpoint.
    Game,
    /// The hub's operator vouched for it on the command line.
    Admin,
}

impl Method {
    fn word(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::Game => "game",
            Self::Admin => "admin",
        }
    }
}

/// How approval onto the network went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    Approved,
    NotNetworkSlug,
    Locked,
    Unlisted,
    RateLimited,
}

impl Approval {
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::NotNetworkSlug => "not_network_slug",
            Self::Locked => "locked",
            Self::Unlisted => "unlisted",
            Self::RateLimited => "rate_limited",
        }
    }
}

/// A verification as the operator is shown it once: the token, until when, and how to use it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issued {
    pub token: String,
    pub expires_at: String,
    /// The hosts a verification page may be on (none for an observed endpoint).
    pub hosts: Vec<String>,
}

impl Issued {
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "token": self.token, "expiresAt": self.expires_at,
            "page": { "hosts": self.hosts },
            "game": format!("@dn verify {}", self.token),
        })
    }
}

/// An operator's relation to a world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Owner,
    Claimant,
}

/// One open verification.
#[derive(Debug, Clone)]
struct Open {
    id: String,
    operator_id: String,
    /// A claim's own key; none when the world's key is the one.
    key_hash: Option<String>,
    token_hash: String,
    expires_at: String,
}

/// A fresh verification token: `dnv_` and 32 characters of base64url (24 random bytes).
#[must_use]
pub fn new_token() -> String {
    use base64::Engine as _;
    format!(
        "dnv_{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random_bytes::<24>())
    )
}

/// Whether `s` has a verification token's shape.
#[must_use]
pub fn is_token_shape(s: &str) -> bool {
    s.len() == 36
        && s.starts_with("dnv_")
        && s[4..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Every token-shaped string in `text`, at most 50 of them.
#[must_use]
pub fn tokens_in(text: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let tok = |b: u8| b.is_ascii_alphanumeric() || b == b'-' || b == b'_';
    let mut i = 0;
    while i + 36 <= text.len() && out.len() < 50 {
        if &text[i..i + 4] == b"dnv_"
            && (i == 0 || !tok(text[i - 1]))
            && text[i + 4..i + 36].iter().all(|b| tok(*b))
            && text.get(i + 36).is_none_or(|b| !tok(*b))
        {
            out.push(String::from_utf8_lossy(&text[i..i + 36]).into_owned());
            i += 36;
        } else {
            i += 1;
        }
    }
    out
}

/// The host a verification page for `endpoint` may be on: the endpoint's own host name or address
/// (never a domain it is under); none for an observed endpoint.
#[must_use]
pub fn page_hosts(endpoint: &Endpoint) -> Vec<String> {
    match endpoint {
        Endpoint::Observed { .. } => Vec::new(),
        Endpoint::Manual { address, .. } => {
            vec![address.trim_end_matches('.').to_ascii_lowercase()]
        }
    }
}

/// Whether a verification page's URL is on one of `hosts`, on the web's own ports (80 or 443),
/// which only the host's administrator serves from. A loopback page, which only a hub developed on
/// one machine may read, may be on any port.
///
/// # Errors
/// Why not.
pub fn on_world_host(url: &crate::fetch::Url, hosts: &[String]) -> std::result::Result<(), String> {
    let loopback = url
        .host
        .parse::<IpAddr>()
        .is_ok_and(|ip| crate::forwarded::canonical(ip).is_loopback());
    if !(url.port == 80 || url.port == 443 || loopback) {
        return Err("the page is on port 80 or 443".into());
    }
    let host = match url.host.parse::<IpAddr>() {
        Ok(ip) => crate::forwarded::canonical(ip).to_string(),
        Err(_) => url.host.clone(),
    };
    if hosts.contains(&host) {
        Ok(())
    } else {
        Err(format!(
            "the page is not on the world's host ({})",
            hosts.join(", ")
        ))
    }
}

/// Where `@dn verify` challenges a world: its endpoint's host and game port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameEndpoint {
    /// A host name or an address to resolve, and the game port.
    Host { host: String, port: u16 },
    /// The address the world's heartbeats come from, which is the verification call's own, and
    /// the game port.
    Observed { port: u16 },
}

impl GameEndpoint {
    /// The game port.
    #[must_use]
    pub fn port(&self) -> u16 {
        match self {
            Self::Host { port, .. } | Self::Observed { port } => *port,
        }
    }
}

fn game_endpoint_of(endpoint: &Endpoint) -> GameEndpoint {
    match endpoint {
        Endpoint::Manual { address, port } => GameEndpoint::Host {
            host: address.clone(),
            port: *port,
        },
        Endpoint::Observed { port, .. } => GameEndpoint::Observed { port: *port },
    }
}

/// A verification call in game that passed its checks: its key, its token, the attempts. What the
/// hub challenges, and what the world's answer is held to.
#[derive(Debug, Clone)]
pub struct GameCheck {
    /// Where the world is challenged, as it was when the call was checked.
    pub endpoint: GameEndpoint,
    /// The digest of the key the call came with: the answer must come with the same one.
    pub key: [u8; 32],
    /// The open verification the call's token was, and that token's digest: a verification
    /// closed or given a new token while the challenge waited is not verified by it.
    open_id: String,
    token_hash: String,
}

/// A verification that checked an endpoint the world no longer has.
fn endpoint_changed() -> RegistryError {
    RegistryError::unverified(
        "endpoint_changed",
        "The world's endpoint changed during the check: check again",
    )
}

fn since(now: SystemTime, secs: u64) -> String {
    iso(now
        .checked_sub(Duration::from_secs(secs))
        .unwrap_or(SystemTime::UNIX_EPOCH))
}

/// Refuses past `limit` events among `times` (newest first) in the last `window` seconds, with how
/// long until the oldest of them leaves the window.
pub(crate) fn within(
    times: &[String],
    limit: usize,
    window: u64,
    now: SystemTime,
    what: &str,
) -> Result<()> {
    if limit == 0 {
        return Err(RegistryError::limited(what, window));
    }
    if times.len() < limit {
        return Ok(());
    }
    let oldest = times
        .get(limit - 1)
        .and_then(|t| parse_iso(t))
        .unwrap_or(now);
    let free_at = oldest + Duration::from_secs(window);
    let wait = free_at.duration_since(now).map_or(1, |d| d.as_secs() + 1);
    Err(RegistryError::limited(what, wait))
}

impl Registry {
    // ---- counting -------------------------------------------------------------------------------

    /// When `operator` (or, with `None`, every operator) recorded `kind` since `window` seconds
    /// ago, newest first: every one, so that a ceiling in the thousands is counted in full (the
    /// limit that counts a kind also bounds how many are recorded).
    pub(crate) fn operator_event_times(
        &self,
        operator: Option<&str>,
        kind: &str,
        window: u64,
    ) -> Result<Vec<String>> {
        let from = since(self.now(), window);
        let mut q = self.db.prepare(
            "SELECT at FROM operator_events WHERE kind = ?1 AND at >= ?2 AND (?3 IS NULL OR operator_id = ?3) ORDER BY at DESC",
        )?;
        let rows = q
            .query_map(params![kind, from, operator], |r| r.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;
        Ok(rows)
    }

    /// When `actor` recorded `kind` for a world since `window` seconds ago, newest first: what one
    /// operator (or the world itself) did, which another's attempts cannot spend.
    fn actor_event_times(
        &self,
        server_id: &str,
        kind: &str,
        actor: &str,
        window: u64,
    ) -> Result<Vec<String>> {
        let from = since(self.now(), window);
        let mut q = self.db.prepare(
            "SELECT received_at FROM registry_events WHERE server_id = ? AND kind = ? AND actor = ? AND received_at >= ? ORDER BY id DESC LIMIT 1000",
        )?;
        let rows = q
            .query_map(params![server_id, kind, actor, from], |r| r.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;
        Ok(rows)
    }

    /// When a world recorded `kind` since `window` seconds ago, newest first.
    fn world_event_times(&self, server_id: &str, kind: &str, window: u64) -> Result<Vec<String>> {
        let from = since(self.now(), window);
        let mut q = self.db.prepare(
            "SELECT received_at FROM registry_events WHERE server_id = ? AND kind = ? AND received_at >= ? ORDER BY id DESC LIMIT 1000",
        )?;
        let rows = q
            .query_map(params![server_id, kind, from], |r| r.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;
        Ok(rows)
    }

    /// Whether the operator with id `operator` is held to an operator's own allowances
    /// (`docs/network/05-hub.md` section 2.9): every operator but the hub's admins, who are held to
    /// the hub's own ceilings and their client address's allowance alone.
    fn has_own_allowances(&self, operator: &str) -> Result<bool> {
        Ok(!self.is_admin_id(operator)?)
    }

    pub(crate) fn operator_event(&self, operator: &str, kind: &str, details: &Value) -> Result<()> {
        self.db.execute(
            "INSERT INTO operator_events(operator_id, kind, at, details) VALUES (?, ?, ?, ?)",
            params![operator, kind, iso(self.now()), details.to_string()],
        )?;
        Ok(())
    }

    // ---- operators and sessions -----------------------------------------------------------------

    /// Records a sign-in by the Discord user `discord_id`, named `name`: a new operator, or the
    /// one it already is. A disabled operator is refused.
    ///
    /// # Errors
    /// 403 for a disabled operator.
    pub fn signed_in(&self, discord_id: &str, name: &str) -> Result<Operator> {
        let now = iso(self.now());
        let found: Option<(String, Option<String>)> = self
            .db
            .query_row(
                "SELECT id, disabled_at FROM operators WHERE discord_id = ?",
                [discord_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let id = match found {
            Some((_, Some(_))) => {
                return Err(RegistryError::forbidden(
                    "This account may not manage worlds on this hub",
                ))
            }
            Some((id, None)) => {
                self.db.execute(
                    "UPDATE operators SET name = ?, signed_in_at = ? WHERE id = ?",
                    params![name, now, id],
                )?;
                id
            }
            None => {
                let id = format!("op_{}", hex(&random_bytes::<8>()));
                self.db.execute(
                    "INSERT INTO operators(id, discord_id, name, created_at, signed_in_at) VALUES (?, ?, ?, ?, ?)",
                    params![id, discord_id, name, now, now],
                )?;
                // An account this Discord account had and forgot is held to what it did.
                self.take_back_counts(&id, discord_id)?;
                id
            }
        };
        self.operator_event(&id, "signed-in", &json!({}))?;
        Ok(Operator {
            id,
            discord_id: discord_id.to_owned(),
            name: name.to_owned(),
            signed_in_at: Some(now),
        })
    }

    /// A new session for `operator`: its id, shown once (only its digest is kept).
    ///
    /// # Errors
    /// Database errors only.
    pub fn new_session(&self, operator: &Operator) -> Result<String> {
        use base64::Engine as _;
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random_bytes::<32>());
        let now = self.now();
        self.db.execute(
            "DELETE FROM operator_sessions WHERE expires_at < ?",
            [iso(now)],
        )?;
        self.db.execute(
            "INSERT INTO operator_sessions(id_hash, operator_id, created_at, expires_at) VALUES (?, ?, ?, ?)",
            params![
                hex(&digest(&token)),
                operator.id,
                iso(now),
                iso(now + Duration::from_secs(SESSION_SECS))
            ],
        )?;
        Ok(token)
    }

    /// The operator a session belongs to, while it lasts and the operator is not disabled.
    ///
    /// # Errors
    /// Database errors only.
    pub fn session(&self, token: &str) -> Result<Option<Operator>> {
        if token.len() != 43
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Ok(None);
        }
        let found: Option<(String, String, String, Option<String>, String)> = self
            .db
            .query_row(
                "SELECT o.id, o.discord_id, o.name, o.signed_in_at, s.expires_at
                 FROM operator_sessions s JOIN operators o ON o.id = s.operator_id
                 WHERE s.id_hash = ? AND o.disabled_at IS NULL",
                [hex(&digest(token))],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
        Ok(
            found.and_then(|(id, discord_id, name, signed_in_at, expires)| {
                parse_iso(&expires)
                    .is_some_and(|e| e > self.now())
                    .then_some(Operator {
                        id,
                        discord_id,
                        name,
                        signed_in_at,
                    })
            }),
        )
    }

    /// Ends a session.
    ///
    /// # Errors
    /// Database errors only.
    pub fn end_session(&self, token: &str) -> Result<()> {
        let op: Option<String> = self
            .db
            .query_row(
                "SELECT operator_id FROM operator_sessions WHERE id_hash = ?",
                [hex(&digest(token))],
                |r| r.get(0),
            )
            .optional()?;
        self.db.execute(
            "DELETE FROM operator_sessions WHERE id_hash = ?",
            [hex(&digest(token))],
        )?;
        if let Some(op) = op {
            self.operator_event(&op, "signed-out", &json!({}))?;
        }
        Ok(())
    }

    /// Ends every session, as after a restore: everyone signs in again. How many ended.
    ///
    /// # Errors
    /// Database errors only.
    pub fn end_all_sessions(&self) -> Result<usize> {
        Ok(self.db.execute("DELETE FROM operator_sessions", [])?)
    }

    /// Every operator, for the hub's operator.
    ///
    /// # Errors
    /// Database errors only.
    pub fn operators(&self) -> Result<Value> {
        let mut q = self.db.prepare(
            "SELECT o.id, o.discord_id, o.name, o.created_at, o.signed_in_at, o.disabled_at,
                    (SELECT count(*) FROM servers s WHERE s.operator_id = o.id)
             FROM operators o ORDER BY o.created_at",
        )?;
        let rows = q
            .query_map([], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?, "discordId": r.get::<_, String>(1)?,
                    "name": r.get::<_, String>(2)?, "createdAt": r.get::<_, String>(3)?,
                    "signedInAt": r.get::<_, Option<String>>(4)?,
                    "disabledAt": r.get::<_, Option<String>>(5)?,
                    "worlds": r.get::<_, i64>(6)?,
                }))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Value::Array(rows))
    }

    /// Disables or enables the operator with Discord id `discord_id`. Disabling ends its sessions.
    ///
    /// # Errors
    /// 404 for an unknown operator.
    pub fn set_operator_disabled(&self, discord_id: &str, disabled: bool) -> Result<()> {
        let id: String = self
            .db
            .query_row(
                "SELECT id FROM operators WHERE discord_id = ?",
                [discord_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| RegistryError::new(404, "No operator has that Discord id"))?;
        let at = disabled.then(|| iso(self.now()));
        self.db.execute(
            "UPDATE operators SET disabled_at = ? WHERE id = ?",
            params![at, id],
        )?;
        if disabled {
            self.db
                .execute("DELETE FROM operator_sessions WHERE operator_id = ?", [&id])?;
            // Nothing it opened may finish, and none of its worlds stays on the network; their keys
            // work again only once it is enabled (`live_key`).
            self.db.execute(
                "UPDATE verifications SET state = 'closed' WHERE operator_id = ? AND state = 'open'",
                [&id],
            )?;
            let on: Vec<String> = {
                let mut q = self.db.prepare(
                    "SELECT slug FROM servers WHERE operator_id = ? AND federation = 'on'",
                )?;
                let rows = q
                    .query_map([&id], |r| r.get(0))?
                    .collect::<std::result::Result<Vec<String>, _>>()?;
                rows
            };
            for slug in on {
                self.set_federation_by(&slug, Federation::Off, None)?;
            }
        }
        self.operator_event(
            &id,
            if disabled { "disabled" } else { "enabled" },
            &json!({}),
        )?;
        Ok(())
    }

    fn operator_by_discord(&self, discord_id: &str) -> Result<String> {
        self.db
            .query_row(
                "SELECT id FROM operators WHERE discord_id = ?",
                [discord_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| {
                RegistryError::new(404, "No operator has that Discord id: they sign in first")
            })
    }

    /// Hands a managed world to the operator with Discord id `discord_id`.
    ///
    /// # Errors
    /// 404, or 409 for a world that is not managed.
    pub fn assign(&self, slug: &str, discord_id: &str) -> Result<()> {
        let row = self.row(slug)?.ok_or_else(RegistryError::not_found)?;
        if row.state() != "managed" {
            return Err(RegistryError::conflict(
                "Only a managed world can be handed to an operator",
            ));
        }
        let op = self.operator_by_discord(discord_id)?;
        self.db.execute(
            "UPDATE servers SET operator_id = ? WHERE id = ?",
            params![op, row.id],
        )?;
        self.event(&row.id, "assigned", &json!({ "operator": op }))?;
        Ok(())
    }

    // ---- worlds ---------------------------------------------------------------------------------

    /// The worlds `operator` holds, and its open claims: an account with either is an operator.
    pub(crate) fn held(&self, operator: &str) -> Result<(usize, usize)> {
        let worlds: i64 = self.db.query_row(
            "SELECT count(*) FROM servers WHERE operator_id = ?",
            [operator],
            |r| r.get(0),
        )?;
        let claims: i64 = self.db.query_row(
            "SELECT count(*) FROM verifications WHERE operator_id = ? AND state = 'open' AND key_hash IS NOT NULL",
            [operator],
            |r| r.get(0),
        )?;
        Ok((
            usize::try_from(worlds).unwrap_or(usize::MAX),
            usize::try_from(claims).unwrap_or(usize::MAX),
        ))
    }

    fn open_verification(&self, server_id: &str, operator: &str) -> Result<Option<Open>> {
        Ok(self
            .db
            .query_row(
                "SELECT id, operator_id, key_hash, token_hash, token_expires_at FROM verifications
                 WHERE server_id = ? AND operator_id = ? AND state = 'open'",
                params![server_id, operator],
                |r| {
                    Ok(Open {
                        id: r.get(0)?,
                        operator_id: r.get(1)?,
                        key_hash: r.get(2)?,
                        token_hash: r.get(3)?,
                        expires_at: r.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    fn open_verifications(&self, server_id: &str) -> Result<Vec<Open>> {
        let mut q = self.db.prepare(
            "SELECT id, operator_id, key_hash, token_hash, token_expires_at FROM verifications
             WHERE server_id = ? AND state = 'open'",
        )?;
        let rows = q
            .query_map([server_id], |r| {
                Ok(Open {
                    id: r.get(0)?,
                    operator_id: r.get(1)?,
                    key_hash: r.get(2)?,
                    token_hash: r.get(3)?,
                    expires_at: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// A fresh token on `server_id`'s open verification by `operator` (one is opened if there is
    /// none), with `claim_key`'s digest for a claim.
    fn issue(&self, row: &Row, operator: &str, claim_key: Option<&str>) -> Result<Issued> {
        let token = new_token();
        let expires = iso(self.now() + Duration::from_secs(TOKEN_SECS));
        match self.open_verification(&row.id, operator)? {
            Some(o) => {
                self.db.execute(
                    "UPDATE verifications SET token_hash = ?, token_expires_at = ? WHERE id = ?",
                    params![hex(&digest(&token)), expires, o.id],
                )?;
            }
            None => {
                self.db.execute(
                    "INSERT INTO verifications(id, server_id, operator_id, key_hash, token_hash, token_expires_at, created_at, state)
                     VALUES (?, ?, ?, ?, ?, ?, ?, 'open')",
                    params![
                        new_id(),
                        row.id,
                        operator,
                        claim_key.map(|k| hex(&digest(k))),
                        hex(&digest(&token)),
                        expires,
                        iso(self.now())
                    ],
                )?;
            }
        }
        self.event_by(&row.id, "verification-issued", Some(operator), &json!({}))?;
        Ok(Issued {
            token,
            expires_at: expires,
            hosts: page_hosts(&row.metadata.endpoint),
        })
    }

    /// Whether `endpoint` is allowed for a world an operator registers: an address must be public.
    fn check_endpoint(&self, r: &Registration) -> Result<()> {
        if let Endpoint::Manual { address, .. } = &r.endpoint {
            if let Ok(ip) = address.parse::<IpAddr>() {
                if !self.policy.allows(ip) {
                    return Err(RegistryError::bad(
                        "endpoint: an address on the public internet",
                    ));
                }
            } else if address.eq_ignore_ascii_case("localhost")
                || address.to_ascii_lowercase().ends_with(".localhost")
            {
                return Err(RegistryError::bad("endpoint: a public host name"));
            }
        }
        Ok(())
    }

    /// Registers a world for `operator`: `pending`, with its key (shown once) and a verification.
    ///
    /// # Errors
    /// 400, 409 for a taken slug, 429 past the limits.
    pub fn register_for(
        &self,
        operator: &Operator,
        r: &Registration,
    ) -> Result<(Value, String, Issued)> {
        self.old_enough(operator)?;
        self.expire_pending()?;
        if self.has_own_allowances(&operator.id)? {
            let times = self.operator_event_times(Some(&operator.id), "registered", DAY)?;
            within(
                &times,
                limits::REGISTRATIONS_PER_DAY,
                DAY,
                self.now(),
                "Too many registrations today",
            )?;
            let (worlds, claims) = self.held(&operator.id)?;
            if worlds + claims >= limits::HELD {
                return Err(RegistryError::conflict(format!(
                    "An operator holds at most {} worlds and claims",
                    limits::HELD
                )));
            }
        }
        self.check_endpoint(r)?;
        let (_, key) = self.register_as(r, self.is_admin(operator))?;
        let row = self.row(&r.slug)?.ok_or_else(RegistryError::not_found)?;
        let mut registry = row.registry.clone();
        registry["state"] = json!("pending");
        registry["verification"] = Value::Null;
        self.db.execute(
            "UPDATE servers SET registry = ?, operator_id = ? WHERE id = ?",
            params![registry.to_string(), operator.id, row.id],
        )?;
        // The registration's event names who registered it.
        self.db.execute(
            "UPDATE registry_events SET actor = ? WHERE server_id = ? AND kind = 'registered'",
            params![operator.id, row.id],
        )?;
        self.operator_event(&operator.id, "registered", &json!({ "slug": row.slug }))?;
        let row = self.row(&r.slug)?.ok_or_else(RegistryError::not_found)?;
        let issued = self.issue(&row, &operator.id, None)?;
        Ok((
            self.operator_view(&row, Role::Owner, &operator.id)?,
            key,
            issued,
        ))
    }

    /// Opens `operator`'s claim of an unclaimed listing: a key and a verification of its own.
    ///
    /// # Errors
    /// 404, 409 for a listing that is not unclaimed or already claimed by this operator, 429.
    pub fn claim_for(&self, operator: &Operator, slug: &str) -> Result<(Value, String, Issued)> {
        self.old_enough(operator)?;
        let row = self.row(slug)?.ok_or_else(RegistryError::not_found)?;
        // What is in no public answer is not found: another operator's `pending` world, and a
        // locked or unlisted listing, which nobody can claim until the hub's operator lifts that.
        let mine = row.operator_id.as_deref() == Some(operator.id.as_str());
        if !mine && !row.is_public() {
            return Err(RegistryError::not_found());
        }
        if row.state() != "unclaimed" {
            return Err(RegistryError::conflict(
                "Only an unclaimed listing can be claimed",
            ));
        }
        if self.open_verification(&row.id, &operator.id)?.is_some() {
            return Err(RegistryError::conflict(
                "You have already claimed this listing",
            ));
        }
        if self.has_own_allowances(&operator.id)? {
            // Claiming and withdrawing again and again would only churn the listing's events.
            let times = self.operator_event_times(Some(&operator.id), "claimed", DAY)?;
            within(
                &times,
                limits::CLAIMS_PER_DAY,
                DAY,
                self.now(),
                "Too many claims today",
            )?;
            let (worlds, claims) = self.held(&operator.id)?;
            if claims >= limits::OPEN_CLAIMS {
                return Err(RegistryError::conflict(format!(
                    "An operator holds at most {} open claims",
                    limits::OPEN_CLAIMS
                )));
            }
            if worlds + claims >= limits::HELD {
                return Err(RegistryError::conflict(format!(
                    "An operator holds at most {} worlds and claims",
                    limits::HELD
                )));
            }
        }
        let key = new_key();
        let issued = self.issue(&row, &operator.id, Some(&key))?;
        self.event_by(&row.id, "claim-opened", Some(&operator.id), &json!({}))?;
        self.operator_event(&operator.id, "claimed", &json!({ "slug": row.slug }))?;
        Ok((
            self.operator_view(&row, Role::Claimant, &operator.id)?,
            key,
            issued,
        ))
    }

    /// Refuses an operator whose Discord account is younger than the hub's minimum age, as the
    /// setting stands now, so that new accounts made by the dozen cannot register or claim worlds.
    fn old_enough(&self, operator: &Operator) -> Result<()> {
        let days = self.setting(Setting::MinAccountAgeDays)?;
        let min = Duration::from_secs(days.saturating_mul(DAY));
        let made = discord_account_made(&operator.discord_id).unwrap_or(self.now());
        if self.now().duration_since(made).unwrap_or_default() < min {
            return Err(RegistryError {
                reason: Some("account_too_new"),
                ..RegistryError::forbidden(format!(
                    "A Discord account must be at least {days} days old to register or claim a world"
                ))
            });
        }
        Ok(())
    }

    /// The world `slug` as `operator` may act on it: its owner, or a claimant. Another operator's
    /// `pending` world is not found; another's world otherwise is forbidden.
    ///
    /// # Errors
    /// 404 or 403.
    pub(crate) fn operator_world(&self, operator: &Operator, slug: &str) -> Result<(Row, Role)> {
        let row = self.row(slug)?.ok_or_else(RegistryError::not_found)?;
        if row.operator_id.as_deref() == Some(&operator.id) {
            return Ok((row, Role::Owner));
        }
        if row.state() == "unclaimed" && self.open_verification(&row.id, &operator.id)?.is_some() {
            return Ok((row, Role::Claimant));
        }
        if row.state() == "pending" {
            return Err(RegistryError::not_found());
        }
        Err(RegistryError::forbidden("This world is not yours"))
    }

    fn owned(&self, operator: &Operator, slug: &str) -> Result<Row> {
        match self.operator_world(operator, slug)? {
            (row, Role::Owner) => Ok(row),
            (_, Role::Claimant) => Err(RegistryError::forbidden(
                "Verify your claim before you manage this world",
            )),
        }
    }

    /// A world as its operator sees it (`docs/network/05-hub.md` section 2.3).
    ///
    /// # Errors
    /// Database errors only.
    pub(crate) fn operator_view(&self, row: &Row, role: Role, operator: &str) -> Result<Value> {
        let row = self.row_by_id(&row.id)?.unwrap_or_else(|| row.clone());
        let mut v = self.render_row(&row);
        let open = self.open_verification(&row.id, operator)?;
        let ver = &row.registry["verification"];
        v["operator"] = json!({
            "role": match role { Role::Owner => "owner", Role::Claimant => "claimant" },
            "verification": {
                "method": ver["method"], "verifiedAt": ver["verifiedAt"],
                "endpointVerified": ver["endpointVerified"].as_bool().unwrap_or(false),
                "open": open.is_some(),
                "expiresAt": open.map(|o| o.expires_at),
            },
            "key": { "issuedAt": row.registry["keyIssuedAt"] },
            "locked": row.registry["locked"],
        });
        Ok(v)
    }

    /// `operator`'s worlds and open claims.
    ///
    /// # Errors
    /// Database errors only.
    pub fn operator_worlds(&self, operator: &Operator) -> Result<Value> {
        self.expire_pending()?;
        let mut out = Vec::new();
        for row in self.all_rows()? {
            if row.operator_id.as_deref() == Some(&operator.id) {
                out.push(self.operator_view(&row, Role::Owner, &operator.id)?);
            } else if row.state() == "unclaimed"
                && self.open_verification(&row.id, &operator.id)?.is_some()
            {
                out.push(self.operator_view(&row, Role::Claimant, &operator.id)?);
            }
        }
        Ok(json!({ "worlds": out }))
    }

    /// One of `operator`'s worlds, with its last 50 events.
    ///
    /// # Errors
    /// 404 or 403.
    pub fn operator_world_detail(&self, operator: &Operator, slug: &str) -> Result<Value> {
        let (row, role) = self.operator_world(operator, slug)?;
        let audit = match role {
            // A claimant sees what it did, never another claimant's verification.
            Role::Claimant => self.events_of(&row.id, 50, Some(&operator.id))?,
            Role::Owner => {
                self.as_owner_reads(&row.id, &operator.id, self.events_of(&row.id, 50, None)?)?
            }
        };
        Ok(json!({
            "world": self.operator_view(&row, role, &operator.id)?,
            "audit": audit,
        }))
    }

    /// A world's events as its owner reads them: who did each is named when it was the owner, the
    /// hub, the world or the hub's operator, and never by another operator's id. An operator that
    /// once sought or held the world (it registered or claimed it, or was handed it) is
    /// `earlier-claimant`; any other, an admin resolving a report, is `hub-admin`. An account since
    /// forgotten is already named so in the log.
    fn as_owner_reads(&self, server_id: &str, owner: &str, mut events: Value) -> Result<Value> {
        let mut q = self.db.prepare_cached(
            "SELECT operator_id FROM verifications WHERE server_id = ?1
             UNION SELECT json_extract(details, '$.operator') FROM registry_events
             WHERE server_id = ?1 AND kind = 'assigned'",
        )?;
        let claimants = q
            .query_map([server_id], |r| r.get::<_, Option<String>>(0))?
            .filter_map(std::result::Result::transpose)
            .collect::<std::result::Result<std::collections::HashSet<String>, _>>()?;
        let shown = |id: &str| -> Value {
            if id == owner
                || matches!(
                    id,
                    "hub" | "world" | "hub-operator" | "earlier-claimant" | "hub-admin"
                )
            {
                json!(id)
            } else if claimants.contains(id) {
                json!("earlier-claimant")
            } else {
                json!("hub-admin")
            }
        };
        for e in events.as_array_mut().into_iter().flatten() {
            if let Some(actor) = e["actor"].as_str() {
                e["actor"] = shown(actor);
            }
            if let Some(to) = e["details"]["operator"].as_str() {
                e["details"]["operator"] = shown(to);
            }
        }
        Ok(events)
    }

    /// Replaces the registration of one of `operator`'s worlds.
    ///
    /// # Errors
    /// 400, 403, 404, 409, 429.
    pub fn update_for(&self, operator: &Operator, slug: &str, r: &Registration) -> Result<Value> {
        if !r.slug.eq_ignore_ascii_case(slug) {
            return Err(RegistryError::bad("slug: the world's own"));
        }
        let row = self.owned(operator, slug)?;
        if row.is_locked() {
            return Err(RegistryError {
                reason: Some("locked"),
                ..RegistryError::conflict("This world is locked: the hub's operator revoked it")
            });
        }
        if self.has_own_allowances(&operator.id)? {
            let times = self.world_event_times(&row.id, "updated", DAY)?;
            within(
                &times,
                limits::UPDATES_PER_DAY,
                DAY,
                self.now(),
                "Too many changes to this world today",
            )?;
        }
        self.check_endpoint(r)?;
        self.update_by(r, Some(&operator.id), self.is_admin(operator))?;
        self.event_by(&row.id, "updated", Some(&operator.id), &json!({}))?;
        // A verified world at a new endpoint leaves the network until the new one is verified.
        if row.metadata.endpoint != r.endpoint && row.federation == Federation::On {
            self.set_federation_by(&row.slug, Federation::Off, Some("hub"))?;
        }
        let row = self.row(slug)?.ok_or_else(RegistryError::not_found)?;
        self.operator_view(&row, Role::Owner, &operator.id)
    }

    /// Withdraws: removes `operator`'s `pending` world, or closes its claim.
    ///
    /// # Errors
    /// 403, 404, 409 for a managed world.
    pub fn withdraw(&self, operator: &Operator, slug: &str) -> Result<()> {
        let (row, role) = self.operator_world(operator, slug)?;
        match role {
            Role::Claimant => {
                self.db.execute(
                    "UPDATE verifications SET state = 'closed' WHERE server_id = ? AND operator_id = ? AND state = 'open'",
                    params![row.id, operator.id],
                )?;
                self.event_by(&row.id, "claim-withdrawn", Some(&operator.id), &json!({}))?;
                Ok(())
            }
            Role::Owner if row.state() == "pending" => {
                self.db
                    .execute("DELETE FROM servers WHERE id = ?", [&row.id])?;
                self.forget_worlds_links(&row.slug)?;
                self.operator_event(&operator.id, "withdrew", &json!({ "slug": row.slug }))?;
                Ok(())
            }
            Role::Owner => Err(RegistryError::conflict(
                "A verified world stays registered: the hub's operator removes one",
            )),
        }
    }

    /// A new token for `operator`'s verification of `slug` (the last one stops).
    ///
    /// # Errors
    /// 403, 404, 409 for a locked world, 429.
    pub fn new_verification(&self, operator: &Operator, slug: &str) -> Result<Issued> {
        let (row, _) = self.operator_world(operator, slug)?;
        if row.is_locked() {
            return Err(RegistryError::conflict(
                "This world is locked: the hub's operator revoked it",
            ));
        }
        if self.has_own_allowances(&operator.id)? {
            // Each operator's own: another claimant's tokens never spend this one's.
            let times =
                self.actor_event_times(&row.id, "verification-issued", &operator.id, DAY)?;
            within(
                &times,
                limits::TOKENS_PER_DAY,
                DAY,
                self.now(),
                "Too many verifications for this world today",
            )?;
        }
        self.issue(&row, &operator.id, None)
    }

    // ---- verification ---------------------------------------------------------------------------

    /// Counts one verification attempt for `row` by `actor` (the operator whose verification it
    /// is), or refuses one past the limit: each claimant's attempts are its own.
    fn attempt(&self, row: &Row, actor: &str, method: Method) -> Result<()> {
        if self.has_own_allowances(actor)? {
            let times = self.actor_event_times(&row.id, "verification-attempted", actor, HOUR)?;
            within(
                &times,
                limits::ATTEMPTS_PER_HOUR,
                HOUR,
                self.now(),
                "Too many verification attempts for this world this hour",
            )?;
        }
        self.event_by(
            &row.id,
            "verification-attempted",
            Some(actor),
            &json!({ "method": method.word() }),
        )
    }

    /// Records how an attempt failed, and hands the failure back.
    pub(crate) fn failed(&self, slug: &str, actor: &str, e: RegistryError) -> RegistryError {
        if let (Some(reason), Ok(Some(row))) = (e.reason, self.row(slug)) {
            let _ = self.event_by(
                &row.id,
                "verification-failed",
                Some(actor),
                &json!({ "reason": reason }),
            );
        }
        e
    }

    /// Before a page is fetched: `operator`'s open verification of `slug`, and the hosts the page
    /// may be on. Nothing is counted until the page is to be fetched ([`Self::page_attempt`]).
    ///
    /// # Errors
    /// 403, 404, 422 (no verification open, or expired, or an observed endpoint).
    pub fn page_hosts(&self, operator: &Operator, slug: &str) -> Result<Vec<String>> {
        let (row, _) = self.operator_world(operator, slug)?;
        let open = self
            .open_verification(&row.id, &operator.id)?
            .ok_or_else(|| {
                RegistryError::unverified(
                    "no_verification",
                    "No verification is open: ask for a token first",
                )
            })?;
        if parse_iso(&open.expires_at).is_none_or(|e| e <= self.now()) {
            return Err(RegistryError::unverified(
                "expired",
                "The token has expired: ask for a new one",
            ));
        }
        let hosts = page_hosts(&row.metadata.endpoint);
        if hosts.is_empty() {
            return Err(RegistryError::unverified(
                "host_mismatch",
                "A world whose endpoint is observed is verified in game",
            ));
        }
        Ok(hosts)
    }

    /// Before a page on the world's host is fetched: the attempt and the fetch counted, within
    /// the limits, and the hosts the page may be on.
    ///
    /// # Errors
    /// 403, 404, 422 (no verification open, or expired, or an observed endpoint), 429.
    pub fn page_attempt(&self, operator: &Operator, slug: &str) -> Result<Vec<String>> {
        let (row, _) = self.operator_world(operator, slug)?;
        let open = self
            .open_verification(&row.id, &operator.id)?
            .ok_or_else(|| {
                RegistryError::unverified(
                    "no_verification",
                    "No verification is open: ask for a token first",
                )
            })?;
        if parse_iso(&open.expires_at).is_none_or(|e| e <= self.now()) {
            return Err(RegistryError::unverified(
                "expired",
                "The token has expired: ask for a new one",
            ));
        }
        let hosts = page_hosts(&row.metadata.endpoint);
        if hosts.is_empty() {
            return Err(RegistryError::unverified(
                "host_mismatch",
                "A world whose endpoint is observed is verified in game",
            ));
        }
        if self.has_own_allowances(&operator.id)? {
            let mine = self.operator_event_times(Some(&operator.id), "page-fetched", HOUR)?;
            within(
                &mine,
                limits::FETCHES_PER_HOUR,
                HOUR,
                self.now(),
                "Too many page checks this hour",
            )?;
        }
        let all = self.operator_event_times(None, "page-fetched", HOUR)?;
        within(
            &all,
            limits::HUB_FETCHES_PER_HOUR,
            HOUR,
            self.now(),
            "The hub has checked too many pages this hour",
        )?;
        self.attempt(&row, &operator.id, Method::Page)?;
        self.operator_event(&operator.id, "page-fetched", &json!({ "slug": row.slug }))?;
        Ok(hosts)
    }

    /// After a page was fetched: verified when one of `found` (the token-shaped strings on it) is
    /// `operator`'s open token for `slug`, and `checked` (the hosts [`Self::page_attempt`] gave)
    /// are still the world's: an endpoint changed while the page was read is not verified by it.
    ///
    /// # Errors
    /// 422 `token_not_found`, `endpoint_changed`, and those of [`Self::page_attempt`].
    pub fn page_found(
        &self,
        operator: &Operator,
        slug: &str,
        found: &[String],
        checked: &[String],
    ) -> Result<(Value, Approval)> {
        let (row, _) = self.operator_world(operator, slug)?;
        if page_hosts(&row.metadata.endpoint) != checked {
            return Err(endpoint_changed());
        }
        let open = self
            .open_verification(&row.id, &operator.id)?
            .ok_or_else(|| {
                RegistryError::unverified("no_verification", "No verification is open")
            })?;
        let want = unhex(&open.token_hash).unwrap_or([0; 32]);
        let mut hit = false;
        for t in found {
            hit |= same_digest(&digest(t), &want);
        }
        if !hit {
            return Err(RegistryError::unverified(
                "token_not_found",
                "The page did not show the token",
            ));
        }
        if parse_iso(&open.expires_at).is_none_or(|e| e <= self.now()) {
            return Err(RegistryError::unverified(
                "expired",
                "The token has expired: ask for a new one",
            ));
        }
        let approval = self.complete(&row, &open, Method::Page, &json!({ "by": null }))?;
        let row = self
            .row_by_id(&row.id)?
            .ok_or_else(RegistryError::not_found)?;
        Ok((
            self.operator_view(&row, Role::Owner, &operator.id)?,
            approval,
        ))
    }

    /// Where the hub would challenge a world for its verification call: its endpoint's host and
    /// game port, once `key` is the world's or one of its open claims'. Nothing is looked up for a
    /// call without one; an unknown world is refused as a wrong key is.
    ///
    /// # Errors
    /// 401 for a key that is neither, or an unknown world.
    pub fn game_endpoint(&self, slug: &str, key: &str) -> Result<GameEndpoint> {
        let unauthorized = || RegistryError::new(401, "A valid key for this server is required");
        let row = self.row(slug)?.ok_or_else(unauthorized)?;
        let presented = digest(key);
        let mut known = row
            .key_hash
            .as_deref()
            .and_then(unhex)
            .is_some_and(|h| same_digest(&presented, &h));
        for o in self.open_verifications(&row.id)? {
            if let Some(h) = o.key_hash.as_deref().and_then(unhex) {
                known |= same_digest(&presented, &h);
            }
        }
        if !known {
            return Err(unauthorized());
        }
        Ok(game_endpoint_of(&row.metadata.endpoint))
    }

    /// The world's verification call, before the hub challenges it: `key` is its key or its
    /// claim's, and `token` that one's open token. The attempt is counted, within the limit.
    ///
    /// # Errors
    /// 401 for a key that is neither; 422 (`no_verification`, `wrong_token`, `expired`); 429.
    pub fn game_check(&self, slug: &str, key: &str, token: &str) -> Result<GameCheck> {
        let unauthorized = || RegistryError::new(401, "A valid key for this server is required");
        let row = self.row(slug)?.ok_or_else(unauthorized)?;
        let presented = digest(key);
        let opens = self.open_verifications(&row.id)?;
        // A claim's own key, or the world's key and its operator's open verification.
        let mut chosen: Option<Open> = None;
        for o in &opens {
            if let Some(h) = o.key_hash.as_deref().and_then(unhex) {
                if same_digest(&presented, &h) {
                    chosen = Some(o.clone());
                }
            }
        }
        let world_key = row
            .key_hash
            .as_deref()
            .and_then(unhex)
            .is_some_and(|h| same_digest(&presented, &h));
        if chosen.is_none() && !world_key {
            return Err(unauthorized());
        }
        let actor = chosen
            .as_ref()
            .map(|o| o.operator_id.clone())
            .or_else(|| row.operator_id.clone())
            .unwrap_or_else(|| "hub".to_owned());
        self.attempt(&row, &actor, Method::Game)?;
        let open = match chosen {
            Some(o) => o,
            None => opens
                .into_iter()
                .find(|o| {
                    o.key_hash.is_none() && row.operator_id.as_deref() == Some(&o.operator_id)
                })
                .ok_or_else(|| {
                    RegistryError::unverified(
                        "no_verification",
                        "No verification is open for this world",
                    )
                })?,
        };
        let want = unhex(&open.token_hash).unwrap_or([0; 32]);
        if !is_token_shape(token) || !same_digest(&digest(token), &want) {
            return Err(RegistryError::unverified(
                "wrong_token",
                "That is not this world's verification token",
            ));
        }
        if parse_iso(&open.expires_at).is_none_or(|e| e <= self.now()) {
            return Err(RegistryError::unverified(
                "expired",
                "The token has expired: ask for a new one",
            ));
        }
        Ok(GameCheck {
            endpoint: game_endpoint_of(&row.metadata.endpoint),
            key: presented,
            open_id: open.id,
            token_hash: open.token_hash,
        })
    }

    /// The world answered the hub's challenge at `answered`, for the call `check` passed: verified,
    /// while the verification is still the one checked, with the same token, and the endpoint the
    /// one challenged. An observed world's address becomes the one it answered at.
    ///
    /// # Errors
    /// 422 (`no_verification`, `wrong_token`, `expired`, `endpoint_changed`,
    /// `address_not_public`).
    pub fn game_answered(
        &self,
        slug: &str,
        check: &GameCheck,
        answered: IpAddr,
        by: &str,
    ) -> Result<Approval> {
        let closed = || {
            RegistryError::unverified(
                "no_verification",
                "The verification was closed during the check",
            )
        };
        let row = self.row(slug)?.ok_or_else(closed)?;
        let open = self
            .open_verifications(&row.id)?
            .into_iter()
            .find(|o| o.id == check.open_id)
            .ok_or_else(closed)?;
        if open.token_hash != check.token_hash {
            return Err(RegistryError::unverified(
                "wrong_token",
                "A new token was issued during the check: use it",
            ));
        }
        if parse_iso(&open.expires_at).is_none_or(|e| e <= self.now()) {
            return Err(RegistryError::unverified(
                "expired",
                "The token has expired: ask for a new one",
            ));
        }
        if game_endpoint_of(&row.metadata.endpoint) != check.endpoint {
            return Err(endpoint_changed());
        }
        let answered = crate::forwarded::canonical(answered);
        if !self.policy.allows(answered) {
            return Err(RegistryError::unverified(
                "address_not_public",
                format!("{answered} is not an address on the public internet"),
            ));
        }
        if let Endpoint::Observed { .. } = &row.metadata.endpoint {
            // The address the world answered at becomes its own.
            let mut registry = row.registry.clone();
            registry["observedAddress"] = json!(answered.to_string());
            self.db.execute(
                "UPDATE servers SET registry = ? WHERE id = ?",
                params![registry.to_string(), row.id],
            )?;
        }
        let by: String = crate::text::without_hidden(by).chars().take(32).collect();
        self.complete(
            &row,
            &open,
            Method::Game,
            &json!({ "by": by, "address": answered.to_string() }),
        )
    }

    /// A verification that succeeded: the world managed (a claim's listing the claimant's, with
    /// the claim's key), every other claim on it closed, and approval asked for.
    fn complete(
        &self,
        row: &Row,
        open: &Open,
        method: Method,
        details: &Value,
    ) -> Result<Approval> {
        let now = iso(self.now());
        // As it is now: the game call may have just recorded an observed world's address.
        let row = self
            .row_by_id(&row.id)?
            .ok_or_else(RegistryError::not_found)?;
        let mut registry = row.registry.clone();
        registry["state"] = json!("managed");
        registry["verifiedAt"] = json!(now);
        registry["verification"] =
            json!({ "method": method.word(), "verifiedAt": now, "endpointVerified": true });
        self.db.execute(
            "UPDATE verifications SET state = 'verified' WHERE id = ?",
            [&open.id],
        )?;
        if let Some(claim_key) = &open.key_hash {
            registry["keyIssuedAt"] = json!(now);
            self.db.execute(
                "UPDATE servers SET registry = ?, operator_id = ?, key_hash = ? WHERE id = ?",
                params![registry.to_string(), open.operator_id, claim_key, row.id],
            )?;
            self.db.execute(
                "UPDATE verifications SET state = 'closed' WHERE server_id = ? AND state = 'open'",
                [&row.id],
            )?;
        } else {
            self.db.execute(
                "UPDATE servers SET registry = ? WHERE id = ?",
                params![registry.to_string(), row.id],
            )?;
        }
        let mut details = details.clone();
        details["method"] = json!(method.word());
        self.event_by(&row.id, "verified", Some(&open.operator_id), &details)?;
        let row = self
            .row_by_id(&row.id)?
            .ok_or_else(RegistryError::not_found)?;
        self.approve(&row, &open.operator_id)
    }

    /// Turns federation on for a verified world, within the allowances.
    fn approve(&self, row: &Row, operator: &str) -> Result<Approval> {
        if row.federation == Federation::On {
            return Ok(Approval::Approved);
        }
        if !is_network_slug(&row.slug) {
            return Ok(Approval::NotNetworkSlug);
        }
        if row.is_locked() {
            return Ok(Approval::Locked);
        }
        if row.is_unlisted() {
            return Ok(Approval::Unlisted);
        }
        if self.approval_wait(operator)?.is_some() {
            return Ok(Approval::RateLimited);
        }
        self.set_federation_by(&row.slug, Federation::On, Some("hub"))?;
        self.operator_event(operator, "approved", &json!({ "slug": row.slug }))?;
        Ok(Approval::Approved)
    }

    /// How long until `operator` may have a world approved, when an allowance is spent: its own
    /// for the day (an admin has none), or the hub's, as its setting stands now.
    fn approval_wait(&self, operator: &str) -> Result<Option<u64>> {
        let now = self.now();
        let all = self.operator_event_times(None, "approved", DAY)?;
        let ceiling =
            usize::try_from(self.setting(Setting::ApprovalsPerDay)?).unwrap_or(usize::MAX);
        let mut checks = vec![within(&all, ceiling, DAY, now, "")];
        if self.has_own_allowances(operator)? {
            let mine = self.operator_event_times(Some(operator), "approved", DAY)?;
            checks.push(within(&mine, limits::APPROVALS_PER_DAY, DAY, now, ""));
        }
        let wait = checks
            .into_iter()
            .filter_map(|r| r.err().and_then(|e| e.retry_after))
            .max();
        Ok(wait)
    }

    /// `operator` turns federation on (within the approval rules) or off for one of its worlds.
    ///
    /// # Errors
    /// 403, 404, 409 (not verified, not a network slug, locked), 429 (the allowances).
    pub fn federation_for(
        &self,
        operator: &Operator,
        slug: &str,
        flag: Federation,
    ) -> Result<Approval> {
        let row = self.owned(operator, slug)?;
        match flag {
            Federation::On => {
                let refused = |reason: &'static str, message: &str| RegistryError {
                    reason: Some(reason),
                    ..RegistryError::conflict(message)
                };
                if row.state() != "managed" {
                    return Err(refused(
                        "not_verified",
                        "Verify this world before it joins the network",
                    ));
                }
                if row.registry["verification"]["endpointVerified"] == false {
                    return Err(refused(
                        "endpoint_not_verified",
                        "Verify the world's new endpoint before it joins the network again",
                    ));
                }
                match self.approve(&row, &operator.id)? {
                    Approval::Approved => Ok(Approval::Approved),
                    Approval::NotNetworkSlug => Err(refused(
                        "not_network_slug",
                        "A world on the network needs a slug of at most 12 characters that is not reserved",
                    )),
                    Approval::Locked => Err(refused(
                        "locked",
                        "This world is locked: the hub's operator revoked it",
                    )),
                    Approval::Unlisted => Err(refused(
                        "unlisted",
                        "This world is unlisted: the hub's operator took it out of the lists",
                    )),
                    Approval::RateLimited => Err(RegistryError {
                        reason: Some("rate_limited"),
                        ..RegistryError::limited(
                            "Today's allowance of worlds joining the network is spent",
                            self.approval_wait(&operator.id)?.unwrap_or(3_600),
                        )
                    }),
                }
            }
            Federation::Off | Federation::Unset => {
                self.set_federation_by(&row.slug, Federation::Off, Some(&operator.id))?;
                Ok(Approval::Approved)
            }
        }
    }

    /// A new key for one of `operator`'s verified worlds; the old one stops at once.
    ///
    /// # Errors
    /// 403, 404, 409 (not verified, locked), 429.
    pub fn rotate_key_for(&self, operator: &Operator, slug: &str) -> Result<String> {
        let row = self.owned(operator, slug)?;
        if row.state() != "managed" {
            return Err(RegistryError::conflict(
                "Verify this world before it is given a new key",
            ));
        }
        if row.is_locked() {
            return Err(RegistryError::conflict(
                "This world is locked: the hub's operator revoked it",
            ));
        }
        if self.has_own_allowances(&operator.id)? {
            let times = self.world_event_times(&row.id, "key-rotated", DAY)?;
            within(
                &times,
                limits::KEYS_PER_DAY,
                DAY,
                self.now(),
                "Too many new keys for this world today",
            )?;
        }
        self.rotate_key_by(&row.slug, Some(&operator.id))
    }

    // ---- reports --------------------------------------------------------------------------------

    /// `operator` reports a world.
    ///
    /// # Errors
    /// 400, 404, 409 for a second open report of the same world, 429.
    pub fn report(
        &self,
        operator: &Operator,
        slug: &str,
        reason: &str,
        details: &str,
    ) -> Result<Value> {
        if !["impersonation", "abuse", "spam", "other"].contains(&reason) {
            return Err(RegistryError::bad(
                "reason: impersonation, abuse, spam or other",
            ));
        }
        if details.chars().count() > 2_000 {
            return Err(RegistryError::bad("details: at most 2000 characters"));
        }
        if !crate::text::is_plain(details, true) {
            return Err(RegistryError::bad(
                "details: no control, bidirectional or zero-width characters",
            ));
        }
        let row = self
            .row(slug)?
            .filter(Row::is_public)
            .ok_or_else(RegistryError::not_found)?;
        let open: i64 = self.db.query_row(
            "SELECT count(*) FROM reports WHERE server_id = ? AND reporter_id = ? AND state = 'open'",
            params![row.id, operator.id],
            |r| r.get(0),
        )?;
        if open > 0 {
            return Err(RegistryError::conflict(
                "You have an open report of this world already",
            ));
        }
        let times = self.operator_event_times(Some(&operator.id), "reported", DAY)?;
        within(
            &times,
            limits::REPORTS_PER_DAY,
            DAY,
            self.now(),
            "Too many reports today",
        )?;
        let id = format!("rp_{}", hex(&random_bytes::<8>()));
        let now = iso(self.now());
        self.db.execute(
            "INSERT INTO reports(id, server_id, reporter_id, reason, details, created_at, state) VALUES (?, ?, ?, ?, ?, ?, 'open')",
            params![id, row.id, operator.id, reason, details.trim(), now],
        )?;
        // The world's own log does not say who reported it.
        self.event_by(
            &row.id,
            "reported",
            Some("hub"),
            &json!({ "reason": reason }),
        )?;
        self.operator_event(
            &operator.id,
            "reported",
            &json!({ "slug": row.slug, "report": id }),
        )?;
        Ok(json!({
            "id": id, "slug": row.slug, "reason": reason, "details": details.trim(),
            "createdAt": now, "state": "open", "resolvedAt": null, "resolution": null,
            "world": { "slug": row.slug, "name": row.metadata.name },
        }))
    }

    fn report_rows(&self, filter: &str, arg: Option<&str>) -> Result<Vec<Value>> {
        let mut q = self.db.prepare(&format!(
            "SELECT r.id, s.slug, r.reason, r.details, r.created_at, r.state, r.resolved_at, r.resolution, r.note,
                    o.id, o.discord_id, o.name, s.metadata
             FROM reports r JOIN servers s ON s.id = r.server_id JOIN operators o ON o.id = r.reporter_id
             {filter} ORDER BY r.created_at DESC LIMIT 500"
        ))?;
        let map = |r: &rusqlite::Row<'_>| {
            let metadata: String = r.get(12)?;
            let name = serde_json::from_str::<Value>(&metadata)
                .ok()
                .and_then(|m| m["name"].as_str().map(str::to_owned));
            let slug: String = r.get(1)?;
            Ok(json!({
                "id": r.get::<_, String>(0)?, "slug": slug,
                "reason": r.get::<_, String>(2)?, "details": r.get::<_, String>(3)?,
                "createdAt": r.get::<_, String>(4)?, "state": r.get::<_, String>(5)?,
                "resolvedAt": r.get::<_, Option<String>>(6)?,
                "resolution": r.get::<_, Option<String>>(7)?, "note": r.get::<_, Option<String>>(8)?,
                "reporter": { "id": r.get::<_, String>(9)?, "discordId": r.get::<_, String>(10)?,
                              "name": r.get::<_, String>(11)? },
                "world": { "slug": slug, "name": name },
            }))
        };
        let rows = match arg {
            Some(a) => q
                .query_map([a], map)?
                .collect::<std::result::Result<Vec<_>, _>>()?,
            None => q
                .query_map([], map)?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        };
        Ok(rows)
    }

    /// `operator`'s own reports.
    ///
    /// # Errors
    /// Database errors only.
    pub fn my_reports(&self, operator: &Operator) -> Result<Value> {
        let mut rows = self.report_rows("WHERE r.reporter_id = ?", Some(&operator.id))?;
        // The reporter is shown how its report ended; the admin's note, and the reporter (itself),
        // are for admins.
        for r in &mut rows {
            if let Some(o) = r.as_object_mut() {
                o.remove("reporter");
                o.remove("note");
            }
        }
        Ok(json!({ "reports": rows }))
    }

    /// The reports, open ones only unless `all`, for the hub's operator and admins.
    ///
    /// # Errors
    /// Database errors only.
    pub fn reports(&self, all: bool) -> Result<Value> {
        let rows = if all {
            self.report_rows("", None)?
        } else {
            self.report_rows("WHERE r.state = 'open'", None)?
        };
        Ok(json!({ "reports": rows }))
    }

    /// Closes a report: `dismiss`, or `revoke`, which also revokes the world (federation off, key
    /// revoked, open claims closed, locked). `actor` is an admin's operator id, or `None` for the
    /// command line.
    ///
    /// # Errors
    /// 400, 404, 409 for a report already closed.
    pub fn resolve_report(
        &self,
        id: &str,
        action: &str,
        note: &str,
        actor: Option<&str>,
    ) -> Result<Value> {
        if !["dismiss", "revoke"].contains(&action) {
            return Err(RegistryError::bad("action: dismiss or revoke"));
        }
        if !crate::text::is_plain(note, true) {
            return Err(RegistryError::bad(
                "note: no control, bidirectional or zero-width characters",
            ));
        }
        if note.trim().chars().count() > 2_000 {
            return Err(RegistryError::bad("note: at most 2000 characters"));
        }
        let note = note.trim();
        let found: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT server_id, state FROM reports WHERE id = ?",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (server_id, state) = found.ok_or_else(|| RegistryError::new(404, "No such report"))?;
        if state != "open" {
            return Err(RegistryError::conflict("This report is closed already"));
        }
        let now = iso(self.now());
        let resolution = if action == "revoke" {
            "revoked"
        } else {
            "dismissed"
        };
        self.db.execute(
            "UPDATE reports SET state = 'closed', resolved_at = ?, resolution = ?, note = ? WHERE id = ?",
            params![now, resolution, note, id],
        )?;
        self.event_by(
            &server_id,
            "report-resolved",
            actor,
            &json!({ "resolution": resolution }),
        )?;
        if action == "revoke" {
            if let Some(row) = self.row_by_id(&server_id)? {
                self.lock(&row, note, actor)?;
            }
        }
        let report = self
            .report_rows("WHERE r.id = ?", Some(id))?
            .pop()
            .unwrap_or(Value::Null);
        Ok(json!({ "report": report }))
    }

    fn lock(&self, row: &Row, reason: &str, actor: Option<&str>) -> Result<()> {
        let mut registry = row.registry.clone();
        registry["locked"] = json!({ "at": iso(self.now()), "reason": reason });
        self.db.execute(
            "UPDATE servers SET registry = ?, federation = 'off', key_hash = NULL WHERE id = ?",
            params![registry.to_string(), row.id],
        )?;
        self.db.execute(
            "UPDATE verifications SET state = 'closed' WHERE server_id = ? AND state = 'open'",
            [&row.id],
        )?;
        self.forget_worlds_listings(&row.slug)?;
        self.event_by(&row.id, "federation", actor, &json!({ "flag": "off" }))?;
        self.event_by(&row.id, "key-revoked", actor, &json!({}))?;
        self.event_by(&row.id, "locked", actor, &json!({ "reason": reason }))?;
        Ok(())
    }

    /// Lifts a lock: the world's operator may turn federation on and issue a key again.
    ///
    /// # Errors
    /// 404, 409 for a world that is not locked.
    /// Removes the `pending` worlds registered longer ago than the hub's setting allows now,
    /// freeing their slugs; each one's operator's log says so.
    ///
    /// # Errors
    /// Database errors only.
    pub(crate) fn expire_pending(&self) -> Result<()> {
        let days = Duration::from_secs(self.setting(Setting::PendingDays)?.saturating_mul(DAY));
        let Some(before) = self.now().checked_sub(days) else {
            return Ok(());
        };
        for row in self.all_rows()? {
            let made = self
                .db
                .query_row(
                    "SELECT created_at FROM servers WHERE id = ?",
                    [&row.id],
                    |r| r.get::<_, String>(0),
                )
                .ok()
                .and_then(|t| parse_iso(&t));
            if row.state() != "pending" || made.is_none_or(|m| m > before) {
                continue;
            }
            self.db
                .execute("DELETE FROM servers WHERE id = ?", [&row.id])?;
            self.forget_worlds_links(&row.slug)?;
            if let Some(op) = &row.operator_id {
                self.operator_event(op, "expired", &json!({ "slug": row.slug }))?;
            }
        }
        Ok(())
    }

    /// Removes a world for good, whatever its state: its key, links, claims, reports and log go
    /// with it, and its characters' links to persons, and its slug is free. Its operator's log says
    /// so.
    ///
    /// # Errors
    /// 404.
    pub fn remove(&self, slug: &str) -> Result<()> {
        let row = self.row(slug)?.ok_or_else(RegistryError::not_found)?;
        self.db
            .execute("DELETE FROM servers WHERE id = ?", [&row.id])?;
        self.forget_worlds_links(&row.slug)?;
        if let Some(op) = &row.operator_id {
            self.operator_event(op, "removed", &json!({ "slug": row.slug }))?;
        }
        Ok(())
    }

    /// Takes a world out of every public answer (`listed` false), federation off, or puts it back.
    /// Its operator keeps it, and cannot turn federation on while it is out.
    ///
    /// # Errors
    /// 404, 409 when it already is.
    pub fn set_listed(&self, slug: &str, listed: bool) -> Result<()> {
        let row = self.row(slug)?.ok_or_else(RegistryError::not_found)?;
        if row.is_unlisted() != listed {
            return Err(RegistryError::conflict(if listed {
                "This world is listed"
            } else {
                "This world is unlisted already"
            }));
        }
        let mut registry = row.registry.clone();
        registry["unlisted"] = if listed { Value::Null } else { json!(true) };
        if let Some(o) = registry.as_object_mut().filter(|_| listed) {
            // Put back by hand: a listing the community list dropped counts its absence afresh.
            o.remove("unlistedBecause");
            o.remove("missingRuns");
        }
        self.db.execute(
            "UPDATE servers SET registry = ? WHERE id = ?",
            params![registry.to_string(), row.id],
        )?;
        if !listed && row.federation == Federation::On {
            self.set_federation_by(&row.slug, Federation::Off, None)?;
        }
        self.event(
            &row.id,
            if listed { "relisted" } else { "unlisted" },
            &json!({}),
        )?;
        Ok(())
    }

    pub fn unlock(&self, slug: &str) -> Result<()> {
        let row = self.row(slug)?.ok_or_else(RegistryError::not_found)?;
        if !row.is_locked() {
            return Err(RegistryError::conflict("This world is not locked"));
        }
        let mut registry = row.registry.clone();
        registry["locked"] = Value::Null;
        self.db.execute(
            "UPDATE servers SET registry = ? WHERE id = ?",
            params![registry.to_string(), row.id],
        )?;
        self.event(&row.id, "unlocked", &json!({}))?;
        Ok(())
    }

    /// The hub's operator vouches for a world on the command line, having reviewed `evidence` that
    /// its operator runs it at its endpoint: the world is verified as a successful proof verifies
    /// it, by the method `admin`, with the evidence in its log as the hub's operator's. Its owner's
    /// open verification is spent, and approval onto the network is asked for the owner, as a
    /// proof asks it; a world with no owner keeps its federation flag.
    ///
    /// # Errors
    /// 400 for evidence that is not 10 to 2,000 plain characters; 404; 409 for a community listing
    /// (which `claim` takes) or a locked world.
    pub fn vouch(&self, slug: &str, evidence: &str) -> Result<Option<Approval>> {
        let evidence = evidence.trim();
        if evidence.chars().count() < 10 || evidence.chars().count() > 2000 {
            return Err(RegistryError::bad(
                "Record 10-2000 characters describing the evidence you reviewed",
            ));
        }
        if !crate::text::is_plain(evidence, true) {
            return Err(RegistryError::bad(
                "evidence: no control, bidirectional or zero-width characters",
            ));
        }
        self.expire_pending()?;
        let row = self.row(slug)?.ok_or_else(RegistryError::not_found)?;
        if row.state() == "unclaimed" {
            return Err(RegistryError::conflict(
                "A community listing is taken with claim, which records the evidence and issues its key",
            ));
        }
        if row.is_locked() {
            return Err(RegistryError::conflict(
                "This world is locked: unlock it before it is verified",
            ));
        }
        let now = iso(self.now());
        let mut registry = row.registry.clone();
        registry["state"] = json!("managed");
        registry["verifiedAt"] = json!(now);
        registry["verification"] = json!({
            "method": Method::Admin.word(), "verifiedAt": now, "endpointVerified": true,
        });
        self.db.execute(
            "UPDATE servers SET registry = ? WHERE id = ?",
            params![registry.to_string(), row.id],
        )?;
        if let Some(owner) = &row.operator_id {
            self.db.execute(
                "UPDATE verifications SET state = 'verified'
                 WHERE server_id = ? AND operator_id = ? AND state = 'open' AND key_hash IS NULL",
                params![row.id, owner],
            )?;
        }
        self.event(
            &row.id,
            "verified",
            &json!({ "method": Method::Admin.word(), "evidence": evidence }),
        )?;
        let row = self
            .row_by_id(&row.id)?
            .ok_or_else(RegistryError::not_found)?;
        match &row.operator_id {
            Some(owner) => Ok(Some(self.approve(&row, owner)?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::fetch::Url;
    use crate::registry::CommunityEntry;

    fn manual(address: &str) -> Endpoint {
        Endpoint::Manual {
            address: address.into(),
            port: 9000,
        }
    }

    /// The Discord id of the hub's admin in these tests.
    const ADMIN: &str = "900";

    struct Fixture {
        clock: Arc<Mutex<SystemTime>>,
        r: Registry,
    }

    impl Fixture {
        fn advance(&self, secs: u64) {
            *self.clock.lock().expect("the clock") += Duration::from_secs(secs);
        }
    }

    fn registration(slug: &str) -> Registration {
        Registration::from_json(&json!({
            "slug": slug, "name": format!("World {slug}"), "ruleset": "PvE",
            "endpoint": { "mode": "manual", "address": "8.8.8.8", "port": 9000 },
            "emulator": { "family": "ACE" },
        }))
        .expect("a registration")
    }

    /// A registry on a clock the test turns, whose admin is [`ADMIN`], with six community listings,
    /// `l0` to `l5`.
    fn fixture() -> Fixture {
        let clock = Arc::new(Mutex::new(
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_790_000_000),
        ));
        let c = Arc::clone(&clock);
        let mut r = Registry::open(
            ":memory:",
            "dereth.network",
            Box::new(move || *c.lock().expect("the clock")),
        )
        .expect("a registry");
        r.admins = vec![ADMIN.into()];
        let listings: Vec<CommunityEntry> = (0..6)
            .map(|i| CommunityEntry {
                external_id: format!("00000000-0000-4000-8000-00000000000{i}"),
                registration: registration(&format!("l{i}")),
                source_hash: "h".into(),
                legacy_type: "PvE".into(),
            })
            .collect();
        r.import_community(&listings, true).expect("imported");
        Fixture { clock, r }
    }

    /// The message of the first of `n` calls of `f` that is refused, if one is.
    fn first_refusal<T>(n: usize, mut f: impl FnMut(usize) -> Result<T>) -> Option<String> {
        (0..n).find_map(|i| f(i).err().map(|e| e.message))
    }

    /// Each of one operator's own allowances, pushed one past: what it is, and the message of the
    /// first request refused, if one was. `p` starts the slugs of the operator's worlds.
    fn past_each_own_allowance(
        f: &Fixture,
        op: &Operator,
        p: &str,
    ) -> Vec<(&'static str, Option<String>)> {
        let r = &f.r;
        let mut out = Vec::new();
        // Claims first, while the operator holds nothing: each withdrawn at once, then held open.
        let past = first_refusal(limits::CLAIMS_PER_DAY + 1, |i| {
            r.claim_for(op, &format!("l{i}"))?;
            r.withdraw(op, &format!("l{i}"))
        });
        out.push(("claims a day", past));
        f.advance(DAY + 1);
        let past = first_refusal(limits::OPEN_CLAIMS + 1, |i| {
            r.claim_for(op, &format!("l{i}"))
        });
        out.push(("open claims", past));
        for i in 0..=limits::OPEN_CLAIMS {
            let _ = r.withdraw(op, &format!("l{i}"));
        }
        let past = first_refusal(limits::REGISTRATIONS_PER_DAY + 1, |i| {
            r.register_for(op, &registration(&format!("{p}r{i}")))
        });
        out.push(("registrations a day", past));
        // More worlds, a day later whenever the day's registrations are spent.
        let (mut n, mut more) = (0, Vec::new());
        let past = first_refusal(limits::HELD + 1, |_| loop {
            n += 1;
            let slug = format!("{p}h{n}");
            match r.register_for(op, &registration(&slug)) {
                Err(e) if e.status == 429 => f.advance(DAY + 1),
                Err(e) => return Err(e),
                Ok(_) => {
                    more.push(slug);
                    return Ok(());
                }
            }
        });
        out.push(("worlds and claims held", past));
        let past = first_refusal(limits::TOKENS_PER_DAY + 1, |_| {
            r.new_verification(op, &format!("{p}r0"))
        });
        out.push(("new verification tokens", past));
        let past = first_refusal(limits::ATTEMPTS_PER_HOUR + 1, |_| {
            r.page_attempt(op, &format!("{p}r1"))
        });
        out.push(("verification attempts", past));
        // Each world's attempts never what refuses: ten pages of each of four worlds.
        let pages = [
            format!("{p}r2"),
            more[0].clone(),
            more[1].clone(),
            more[2].clone(),
        ];
        let past = first_refusal(limits::FETCHES_PER_HOUR + 1, |i| {
            r.page_attempt(op, &pages[i / limits::ATTEMPTS_PER_HOUR])
        });
        out.push(("page fetches", past));
        let approved = [
            format!("{p}r0"),
            format!("{p}r1"),
            format!("{p}r2"),
            more[0].clone(),
        ];
        let past = first_refusal(limits::APPROVALS_PER_DAY + 1, |i| {
            r.vouch(&approved[i], "Reviewed the world's own page.")?;
            r.federation_for(op, &approved[i], Federation::On)
        });
        out.push(("federation approvals", past));
        let past = first_refusal(limits::KEYS_PER_DAY + 1, |_| {
            r.rotate_key_for(op, &format!("{p}r0"))
        });
        out.push(("new keys", past));
        let past = first_refusal(limits::UPDATES_PER_DAY + 1, |_| {
            r.update_for(op, &format!("{p}r0"), &registration(&format!("{p}r0")))
        });
        out.push(("registration changes", past));
        out
    }

    /// Spec: hub: the hub's admins are held to none of one operator's own allowances (registrations,
    /// worlds and claims held, claims, verification tokens and attempts, page fetches, approvals,
    /// a world's new keys and changes); every other operator is refused past each of them.
    #[test]
    fn an_admin_is_held_to_no_operators_own_allowance() {
        let f = fixture();
        let ann = f.r.signed_in("100", "Ann").expect("signed in");
        let refused: Vec<_> = past_each_own_allowance(&f, &ann, "o")
            .into_iter()
            .map(|(what, past)| (what, past.unwrap_or_default()))
            .collect();
        assert_eq!(
            refused,
            [
                ("claims a day", "Too many claims today".to_owned()),
                (
                    "open claims",
                    "An operator holds at most 3 open claims".into()
                ),
                ("registrations a day", "Too many registrations today".into()),
                (
                    "worlds and claims held",
                    "An operator holds at most 10 worlds and claims".into()
                ),
                (
                    "new verification tokens",
                    "Too many verifications for this world today".into()
                ),
                (
                    "verification attempts",
                    "Too many verification attempts for this world this hour".into()
                ),
                ("page fetches", "Too many page checks this hour".into()),
                (
                    "federation approvals",
                    "Today's allowance of worlds joining the network is spent".into()
                ),
                ("new keys", "Too many new keys for this world today".into()),
                (
                    "registration changes",
                    "Too many changes to this world today".into()
                ),
            ],
            "an operator is refused past each"
        );
        let admin = f.r.signed_in(ADMIN, "Admin").expect("signed in");
        for (what, past) in past_each_own_allowance(&f, &admin, "a") {
            assert_eq!(past, None, "an admin past {what}");
        }
    }

    /// Spec: hub: the hub's admins are held to the hub's own ceilings (page fetches an hour and
    /// approvals a day, for every operator together), to one operator's reports a day and to one
    /// person's wrong link codes an hour, as every operator is.
    #[test]
    fn an_admin_is_held_to_the_hubs_ceilings_reports_and_wrong_codes() {
        let f = fixture();
        f.r.set_settings(&[(Setting::ApprovalsPerDay, 1)], None)
            .expect("set");
        let admin = f.r.signed_in(ADMIN, "Admin").expect("signed in");
        let ann = f.r.signed_in("100", "Ann").expect("signed in");
        for slug in ["a1", "a2"] {
            f.r.register_for(&admin, &registration(slug))
                .expect("registered");
        }
        for _ in 0..limits::HUB_FETCHES_PER_HOUR {
            f.r.operator_event(&ann.id, "page-fetched", &json!({}))
                .expect("counted");
        }
        let page =
            f.r.page_attempt(&admin, "a1")
                .expect_err("the hub's ceiling");
        assert_eq!(
            (page.status, page.message.as_str()),
            (429, "The hub has checked too many pages this hour")
        );
        let vouch = |slug: &str| f.r.vouch(slug, "Reviewed the world's own page.");
        assert_eq!(vouch("a1"), Ok(Some(Approval::Approved)));
        assert_eq!(
            vouch("a2"),
            Ok(Some(Approval::RateLimited)),
            "the hub's approvals a day are spent"
        );
        for i in 0..limits::REPORTS_PER_DAY {
            f.r.report(&admin, &format!("l{i}"), "spam", "")
                .expect("reported");
        }
        let report = f.r.report(&admin, "l5", "spam", "").expect_err("limited");
        assert_eq!(report.status, 429, "{report:?}");
        for _ in 0..crate::persons::limits::WRONG_CODES_PER_HOUR {
            let wrong =
                f.r.redeem_link_code(&admin, "QQQQ-QQQQ")
                    .expect_err("wrong");
            assert_eq!(wrong.status, 422);
        }
        let wrong =
            f.r.redeem_link_code(&admin, "QQQQ-QQQQ")
                .expect_err("limited");
        assert_eq!(wrong.status, 429, "{wrong:?}");
    }

    /// Spec: hub: a verification page is on the endpoint's own host, never a domain it is under,
    /// and on port 80 or 443.
    #[test]
    fn a_page_is_on_the_endpoints_own_host_and_the_webs_own_ports() {
        let hosts = page_hosts(&manual("Play.Coldeve.Example.org."));
        assert_eq!(hosts, ["play.coldeve.example.org"]);
        let on = |u: &str| on_world_host(&Url::parse(u).expect("a URL"), &hosts);
        assert!(on("https://play.coldeve.example.org/network.txt").is_ok());
        assert!(on("http://play.coldeve.example.org:80/network.txt").is_ok());
        assert!(on("https://play.coldeve.example.org:443/x").is_ok());
        for refused in [
            "https://coldeve.example.org/network.txt",
            "https://example.org/network.txt",
            "https://play.coldeve.example.org:8443/network.txt",
            "http://play.coldeve.example.org:1024/x",
            "https://other.example.org/x",
        ] {
            assert!(on(refused).is_err(), "{refused}");
        }
        let hosts = page_hosts(&manual("8.8.8.8"));
        let on = |u: &str| on_world_host(&Url::parse(u).expect("a URL"), &hosts);
        assert!(on("https://8.8.8.8/x").is_ok());
        assert!(on("https://8.8.8.8:8080/x").is_err());
        let hosts = page_hosts(&manual("127.0.0.1"));
        assert!(on_world_host(
            &Url::parse("http://127.0.0.1:8080/x").expect("a URL"),
            &hosts
        )
        .is_ok());
    }
}
