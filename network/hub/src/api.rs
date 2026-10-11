//! The operator API over HTTP (`docs/network/05-hub.md` section 2): Discord sign-in, sessions,
//! worlds, verification, approval, keys, reports and the admin routes, and the world's half of
//! verification in game.
//!
//! - **Sessions** are the cookie `__Host-dn_session` (`Secure`, `HttpOnly`, `SameSite=Lax`), set
//!   by the hub for its own host. The site calls the API from another origin on the same site, so
//!   the session routes answer CORS for the configured site origins only, with credentials.
//! - **A request that changes something** must carry an `Origin` the hub trusts (a site origin or
//!   its own), checked before the session is read; its body is JSON, at most 64 KiB.
//! - **Nothing slow runs under the registry's lock**: Discord's exchange, a page fetch, a name
//!   lookup and a challenge's wait each happen between two short holds of it.
//! - **Discord's `429`** is honored by the whole hub: until the wait it names is over, no sign-in
//!   calls Discord, and each answers `signin=busy`.
//! - **Verification in game** holds the world's call while the hub challenges its game endpoint
//!   ([`crate::challenge`]) and waits for the world to post the nonce back on a second route.
//!
//! No session id, key, token or page is logged.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

use dereth_network_protocol::Federation;
use serde_json::{json, Value};

use crate::fetch::{self, Fetch, FetchError, Url};
use crate::operators::{self, GameEndpoint, Operator};
use crate::registry::{Registration, RegistryError};
use crate::server::Shared;

/// The session cookie.
pub const SESSION_COOKIE: &str = "__Host-dn_session";
/// The cookie that binds a sign-in to the browser that started it, and carries it: its state,
/// where to return and when it lapses, signed by the hub, which keeps nothing of a sign-in in
/// progress.
pub const OAUTH_COOKIE: &str = "__Host-dn_oauth";
/// How long a sign-in may take, from the start to Discord's answer.
const OAUTH_TTL: Duration = Duration::from_secs(600);
/// The most finished sign-ins whose state the hub remembers, so none is used twice; past this the
/// oldest are forgotten (Discord's codes are single-use anyway).
pub(crate) const SPENT_STATES: usize = 10_000;
/// The largest body an operator route takes.
pub const MAX_BODY: usize = 64 * 1024;

const JSON: &str = "application/json; charset=utf-8";

/// A request as the API reads it.
#[derive(Debug)]
pub(crate) struct Req<'a> {
    pub method: &'a str,
    /// The path with its query.
    pub path: &'a str,
    pub headers: &'a [(String, String)],
    pub body: &'a [u8],
    /// The client address ([`crate::forwarded`]).
    pub ip: IpAddr,
}

impl Req<'_> {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn cookie(&self, name: &str) -> Option<String> {
        self.headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("cookie"))
            .flat_map(|(_, v)| v.split(';'))
            .filter_map(|c| c.trim().split_once('='))
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.trim().to_owned())
    }
}

/// Which CORS headers an answer carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Cors {
    /// `Access-Control-Allow-Origin: *`: the public routes.
    Any,
    /// The one origin, with credentials.
    Credentials(String),
    /// None.
    Nothing,
}

/// An answer.
#[derive(Debug)]
pub(crate) struct Reply {
    pub status: u16,
    pub body: String,
    pub kind: &'static str,
    pub headers: Vec<(String, String)>,
    pub cors: Cors,
}

impl Reply {
    fn json(status: u16, v: &Value, cors: &Cors) -> Self {
        Self {
            status,
            body: v.to_string(),
            kind: JSON,
            headers: Vec::new(),
            cors: cors.clone(),
        }
    }

    fn empty(status: u16, cors: &Cors) -> Self {
        Self {
            status,
            body: String::new(),
            kind: JSON,
            headers: Vec::new(),
            cors: cors.clone(),
        }
    }

    fn error(e: &RegistryError, cors: &Cors) -> Self {
        let message = if e.status >= 500 {
            crate::server::log("error", &e.message);
            "An internal error occurred"
        } else {
            e.message.as_str()
        };
        let mut err =
            json!({ "code": e.code.unwrap_or_else(|| code(e.status)), "message": message });
        if let Some(r) = e.reason {
            err["reason"] = json!(r);
        }
        if let Some(s) = e.retry_after {
            err["retryAfterSeconds"] = json!(s);
        }
        let mut r = Self::json(e.status, &json!({ "error": err }), cors);
        if let Some(s) = e.retry_after {
            r.headers.push(("Retry-After".into(), s.to_string()));
        }
        r
    }

    fn redirect(status: u16, location: &str) -> Self {
        Self {
            status,
            body: String::new(),
            kind: JSON,
            headers: vec![("Location".into(), location.to_owned())],
            cors: Cors::Nothing,
        }
    }

    fn cookie(mut self, c: String) -> Self {
        self.headers.push(("Set-Cookie".into(), c));
        self
    }
}

/// The error code a status is answered with.
#[must_use]
pub fn code(status: u16) -> &'static str {
    match status {
        400 => "INVALID_REQUEST",
        401 => "UNAUTHORIZED",
        403 => "FORBIDDEN",
        404 => "NOT_FOUND",
        405 => "METHOD_NOT_ALLOWED",
        408 => "TIMEOUT",
        411 => "LENGTH_REQUIRED",
        409 => "CONFLICT",
        413 => "TOO_LARGE",
        422 => "VERIFICATION_FAILED",
        429 => "RATE_LIMITED",
        503 => "UNAVAILABLE",
        s if s >= 500 => "INTERNAL_ERROR",
        _ => "REQUEST_FAILED",
    }
}

fn fail(status: u16, message: &str) -> RegistryError {
    RegistryError::new(status, message)
}

/// HMAC-SHA256 of `message` under `key`.
fn hmac(key: &[u8; 32], message: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let (mut inner, mut outer) = ([0x36u8; 64], [0x5cu8; 64]);
    for (i, k) in key.iter().enumerate() {
        inner[i] ^= k;
        outer[i] ^= k;
    }
    let first = Sha256::new()
        .chain_update(inner)
        .chain_update(message)
        .finalize();
    Sha256::new()
        .chain_update(outer)
        .chain_update(first)
        .finalize()
        .into()
}

fn b64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn unb64(text: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(text)
        .ok()
}

/// A sign-in's cookie: its state, when it lapses (seconds since 1970) and where to return, signed
/// under `key`.
pub(crate) fn sign_in_cookie(key: &[u8; 32], state: &str, lapses: u64, return_to: &str) -> String {
    let body = format!("{state}~{lapses}~{}", b64(return_to.as_bytes()));
    format!("{body}~{}", b64(&hmac(key, body.as_bytes())))
}

/// A sign-in's cookie read back, while it holds: its state and where to return. None for one the
/// hub did not sign under `key`, one altered, and one lapsed by `now` (seconds since 1970).
pub(crate) fn read_sign_in_cookie(
    key: &[u8; 32],
    cookie: &str,
    now: u64,
) -> Option<(String, String)> {
    let (body, mac) = cookie.rsplit_once('~')?;
    let want = hmac(key, body.as_bytes());
    let got = unb64(mac)?;
    if got.len() != want.len() || got.iter().zip(want).fold(0u8, |acc, (a, b)| acc | (a ^ b)) != 0 {
        return None;
    }
    let mut parts = body.split('~');
    let (state, lapses, return_to) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || lapses.parse::<u64>().ok()? <= now {
        return None;
    }
    let return_to = String::from_utf8(unb64(return_to)?).ok()?;
    Some((state.to_owned(), return_to))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Whether `path` is one the API answers.
#[must_use]
pub(crate) fn is_api(path: &str) -> bool {
    let p = path.split('?').next().unwrap_or("");
    p.starts_with("/v1/auth/")
        || p == "/v1/operator"
        || p.starts_with("/v1/operator/")
        || p == "/v1/reports"
        || p.starts_with("/v1/admin/")
        || p == "/v1/person"
        || p.starts_with("/v1/person/")
        || (p.starts_with("/v1/servers/") && p.ends_with("/verify") && p.matches('/').count() == 4)
        || (p.starts_with("/v1/servers/")
            && p.ends_with("/verify/answer")
            && p.matches('/').count() == 5)
}

fn session_cookie(token: &str) -> String {
    format!(
        "{SESSION_COOKIE}={token}; Path=/; Max-Age={}; Secure; HttpOnly; SameSite=Lax",
        operators::SESSION_SECS
    )
}

fn clear_cookie(name: &str) -> String {
    format!("{name}=; Path=/; Max-Age=0; Secure; HttpOnly; SameSite=Lax")
}

/// `scheme://host[:port]` of a URL.
fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let host = rest.split('/').next()?;
    (!host.is_empty()).then(|| {
        format!(
            "{}://{}",
            scheme.to_ascii_lowercase(),
            host.to_ascii_lowercase()
        )
    })
}

/// A path on the site to return to after sign-in: `/` unless `r` is one.
fn return_path(r: Option<&String>) -> String {
    match r {
        Some(p)
            if p.starts_with('/')
                && !p.starts_with("//")
                && p.len() <= 200
                && p.bytes().all(|b| b.is_ascii_graphic() && b != b'\\') =>
        {
            p.clone()
        }
        _ => "/".to_owned(),
    }
}

fn same_bytes(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// The rule a page fetch is held to.
fn page_rule(loopback: bool) -> fetch::AddressRule {
    fn public_or_loopback(ip: IpAddr) -> bool {
        fetch::is_public(ip) || ip.is_loopback()
    }
    if loopback {
        public_or_loopback
    } else {
        fetch::is_public
    }
}

fn parse_body<T: serde::de::DeserializeOwned>(req: &Req<'_>) -> Result<T, RegistryError> {
    if req.body.len() > MAX_BODY {
        return Err(fail(413, "Request body too large"));
    }
    // `application/json` exactly, with parameters (`; charset=utf-8`) allowed.
    let json_type = req.header("content-type").is_some_and(|t| {
        t.split(';')
            .next()
            .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"))
    });
    if !json_type {
        return Err(fail(
            400,
            "The body is JSON (Content-Type: application/json)",
        ));
    }
    serde_json::from_slice(req.body).map_err(|e| fail(400, &e.to_string()))
}

fn query(path: &str) -> HashMap<String, String> {
    crate::server::query(path)
}

/// The CORS an operator route answers `origin` with: credentials for a site origin, none for any
/// other.
pub(crate) fn cors_for(shared: &Shared, origin: Option<&str>) -> Cors {
    origin
        .filter(|o| shared.config.site_origins.iter().any(|s| s == o))
        .map_or(Cors::Nothing, |o| Cors::Credentials(o.to_owned()))
}

/// Answers an API request.
pub(crate) fn handle(shared: &Shared, req: &Req<'_>) -> Reply {
    let path = req.path.split('?').next().unwrap_or("");
    let segs: Vec<&str> = path.trim_matches('/').split('/').collect();
    if let ["v1", "servers", slug, "verify", rest @ ..] = segs.as_slice() {
        if req.method != "POST" {
            return Reply::error(&fail(405, "POST only"), &Cors::Nothing);
        }
        let slug = slug.to_ascii_lowercase();
        return match rest {
            [] => world_verify(shared, req, &slug),
            ["answer"] => world_answer(shared, req, &slug),
            _ => Reply::error(&fail(404, "Route not found"), &Cors::Nothing),
        };
    }
    let config = &shared.config;
    let origin = req.header("origin").map(str::to_owned);
    let cors = cors_for(shared, origin.as_deref());
    if req.method == "OPTIONS" {
        return if cors != Cors::Nothing {
            let mut r = Reply::empty(204, &cors);
            r.headers.extend([
                (
                    "Access-Control-Allow-Methods".into(),
                    "GET, POST, PUT, DELETE".into(),
                ),
                ("Access-Control-Allow-Headers".into(), "Content-Type".into()),
                ("Access-Control-Max-Age".into(), "600".into()),
            ]);
            r
        } else {
            Reply::error(
                &fail(403, "This origin may not call the operator API"),
                &Cors::Nothing,
            )
        };
    }
    let (Some(discord), Some(public_url)) = (config.discord.as_ref(), config.public_url.as_deref())
    else {
        return Reply::error(
            &fail(503, "Operator sign-in is not configured on this hub"),
            &cors,
        );
    };
    if config.site_origins.is_empty() {
        return Reply::error(
            &fail(503, "Operator sign-in is not configured on this hub"),
            &cors,
        );
    }
    let redirect_uri = format!(
        "{}/v1/auth/discord/callback",
        public_url.trim_end_matches('/')
    );
    match (req.method, segs.as_slice()) {
        ("GET", ["v1", "auth", "discord"]) => {
            // Nothing is kept here: the cookie carries the sign-in, signed, so no number of
            // sign-ins started can push another out.
            // Back to the site the sign-in was started from, when it is one of the hub's own,
            // else to the first: no other address can be made a place a sign-in returns to.
            let q = query(req.path);
            let return_to = format!(
                "{}{}",
                started_from(&config.site_origins, q.get("site")),
                return_path(q.get("return"))
            );
            // While Discord asks the hub to wait, no sign-in could be finished: none is started.
            if shared.discord_waiting() {
                return sign_in_busy(&return_to);
            }
            let state = b64(&crate::registry::random_bytes::<24>());
            let cookie = sign_in_cookie(
                &shared.sign_in_key,
                &state,
                unix_now() + OAUTH_TTL.as_secs(),
                &return_to,
            );
            Reply::redirect(302, &discord.authorize_url(&state, &redirect_uri)).cookie(format!(
                "{OAUTH_COOKIE}={cookie}; Path=/; Max-Age={}; Secure; HttpOnly; SameSite=Lax",
                OAUTH_TTL.as_secs()
            ))
        }
        ("GET", ["v1", "auth", "discord", "callback"]) => {
            callback(shared, req, discord, &redirect_uri, &config.site_origins)
        }
        _ => session_route(shared, req, &segs, &cors, origin.as_deref(), public_url),
    }
}

/// A sign-in while Discord asks the hub to wait: back to `return_to` (a site's address) with
/// `signin=busy`, to try again shortly.
fn sign_in_busy(return_to: &str) -> Reply {
    let sep = if return_to.contains('?') { '&' } else { '?' };
    Reply::redirect(303, &format!("{return_to}{sep}signin=busy")).cookie(clear_cookie(OAUTH_COOKIE))
}

/// The site origin a sign-in was started from (`site` in its query), when it is one of the
/// hub's site origins; otherwise the first of them.
fn started_from<'a>(sites: &'a [String], site: Option<&String>) -> &'a str {
    site.and_then(|o| sites.iter().find(|s| *s == o))
        .or_else(|| sites.first())
        .map_or("", String::as_str)
}

/// Where a finished sign-in goes: the address its signed cookie carries, when that is under one
/// of the hub's site origins (a path alone, from a cookie made before sites were carried, is
/// under the first); otherwise the first site's front page.
fn back_to(sites: &[String], kept: &str) -> String {
    let first = sites.first().map_or("", String::as_str);
    if kept.starts_with('/') {
        return format!("{first}{kept}");
    }
    let under_a_site = sites.iter().any(|s| {
        kept.strip_prefix(s.as_str())
            .is_some_and(|rest| rest.starts_with('/'))
    });
    if under_a_site {
        kept.to_owned()
    } else {
        format!("{first}/")
    }
}

/// Discord sends the browser back here.
fn callback(
    shared: &Shared,
    req: &Req<'_>,
    discord: &crate::discord::Discord,
    redirect_uri: &str,
    sites: &[String],
) -> Reply {
    let q = query(req.path);
    let state = q.get("state").cloned().unwrap_or_default();
    let signed = req
        .cookie(OAUTH_COOKIE)
        .and_then(|c| read_sign_in_cookie(&shared.sign_in_key, &c, unix_now()));
    let return_to = signed
        .as_ref()
        .map_or_else(|| back_to(sites, "/"), |(_, r)| back_to(sites, r));
    let failed = |why: &str| {
        crate::server::log("info", &format!("a sign-in failed: {why}"));
        let sep = if return_to.contains('?') { '&' } else { '?' };
        Reply::redirect(303, &format!("{return_to}{sep}signin=failed"))
            .cookie(clear_cookie(OAUTH_COOKIE))
    };
    let Some((cookie_state, _)) = signed else {
        return failed("no sign-in cookie the hub signed, or one that has lapsed");
    };
    if state.is_empty() || !same_bytes(&cookie_state, &state) {
        return failed("the state is not this browser's");
    }
    if !shared.spend_state(&state) {
        return failed("a state already used");
    }
    if q.contains_key("error") {
        return failed("Discord refused");
    }
    let Some(code) = q.get("code").filter(|c| !c.is_empty() && c.len() <= 256) else {
        return failed("no code");
    };
    // Discord answered `429`: until its wait is over, the hub asks it nothing.
    if shared.discord_waiting() {
        return sign_in_busy(&return_to);
    }
    let user = match discord.user_for_code(code, redirect_uri) {
        Ok(u) => u,
        Err(crate::discord::DiscordError::Wait(secs)) => {
            shared.discord_wait(secs);
            return sign_in_busy(&return_to);
        }
        Err(crate::discord::DiscordError::Failed(e)) => return failed(&e),
    };
    let started = {
        let reg = shared.registry();
        reg.signed_in(&user.id, &user.name)
            .and_then(|op| reg.new_session(&op))
    };
    match started {
        Ok(token) => Reply::redirect(303, &return_to)
            .cookie(session_cookie(&token))
            .cookie(clear_cookie(OAUTH_COOKIE)),
        Err(e) => failed(&e.message),
    }
}

/// The routes that need a session.
fn session_route(
    shared: &Shared,
    req: &Req<'_>,
    segs: &[&str],
    cors: &Cors,
    origin: Option<&str>,
    public_url: &str,
) -> Reply {
    let config = &shared.config;
    let mutating = matches!(req.method, "POST" | "PUT" | "DELETE");
    if mutating {
        let own = origin_of(public_url);
        let trusted = origin.is_some_and(|o| {
            config.site_origins.iter().any(|s| s == o) || own.as_deref() == Some(o)
        });
        if !trusted {
            return Reply::error(&fail(403, "This request must come from the site"), cors);
        }
    }
    let token = req.cookie(SESSION_COOKIE);
    if let (["v1", "auth", "signout"], "POST") = (segs, req.method) {
        if let Some(t) = token {
            if let Err(e) = shared.registry().end_session(&t) {
                return Reply::error(&e, cors);
            }
        }
        return Reply::empty(204, cors).cookie(clear_cookie(SESSION_COOKIE));
    }
    let operator = match token.map(|t| shared.registry().session(&t)) {
        Some(Ok(Some(op))) => op,
        Some(Err(e)) => return Reply::error(&e, cors),
        _ => return Reply::error(&fail(401, "Sign in first"), cors),
    };
    let admin = config.admins.contains(&operator.discord_id);
    if let (["v1", "person"], "DELETE") = (segs, req.method) {
        // Forgetting the person may remove its account, and with it the session.
        let forgotten = {
            let reg = shared.registry();
            reg.characters_of(&operator)
                .and_then(|characters| Ok((reg.forget_person(&operator)?, characters)))
        };
        return match forgotten {
            Ok((gone, characters)) => {
                shared.persons_changed(characters);
                let r = Reply::empty(204, cors);
                if gone {
                    r.cookie(clear_cookie(SESSION_COOKIE))
                } else {
                    r
                }
            }
            Err(e) => Reply::error(&e, cors),
        };
    }
    let result = route(shared, req, segs, &operator, admin);
    match result {
        Ok((204, _)) => Reply::empty(204, cors),
        Ok((status, v)) => Reply::json(status, &v, cors),
        Err(e) => Reply::error(&e, cors),
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct VerifyPage {
    url: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FlagBody {
    flag: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportBody {
    slug: String,
    reason: String,
    #[serde(default)]
    details: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LinkBody {
    code: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AltsBody {
    shown: bool,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveBody {
    action: String,
    #[serde(default)]
    note: String,
}

/// A registration from the body, its host name (if it has one) resolving to public addresses
/// only. A name that does not resolve yet, or not in time, is taken: the world cannot be verified
/// at it until it resolves.
fn registration(shared: &Shared, req: &Req<'_>) -> Result<Registration, RegistryError> {
    let v: Value = parse_body(req)?;
    let r = Registration::from_json(&v)?;
    if let crate::registry::Endpoint::Manual { address, .. } = &r.endpoint {
        if address.parse::<IpAddr>().is_err() {
            let policy = operators::Policy {
                loopback_allowed: shared.config.loopback_allowed,
                ..operators::Policy::default()
            };
            if let crate::resolver::Resolved::Found(found) = shared.resolver.resolve(address) {
                if found
                    .iter()
                    .any(|ip| !policy.allows(crate::forwarded::canonical(*ip)))
                {
                    return Err(RegistryError::bad(
                        "endpoint: a host name whose every address is on the public internet",
                    ));
                }
            }
        }
    }
    Ok(r)
}

fn route(
    shared: &Shared,
    req: &Req<'_>,
    segs: &[&str],
    op: &Operator,
    admin: bool,
) -> Result<(u16, Value), RegistryError> {
    let slug_of = |s: &str| s.to_ascii_lowercase();
    match (req.method, segs) {
        ("GET", ["v1", "operator"]) => Ok((200, json!({ "operator": op.to_json(admin) }))),
        ("GET", ["v1", "operator", "worlds"]) => Ok((200, shared.registry().operator_worlds(op)?)),
        ("POST", ["v1", "operator", "worlds"]) => {
            let r = registration(shared, req)?;
            let (world, key, issued) = shared.registry().register_for(op, &r)?;
            Ok((
                201,
                json!({ "world": world, "key": key, "verification": issued.to_json() }),
            ))
        }
        ("GET", ["v1", "operator", "worlds", slug]) => Ok((
            200,
            shared
                .registry()
                .operator_world_detail(op, &slug_of(slug))?,
        )),
        ("PUT", ["v1", "operator", "worlds", slug]) => {
            let r = registration(shared, req)?;
            let world = shared.registry().update_for(op, &slug_of(slug), &r)?;
            Ok((200, json!({ "world": world })))
        }
        ("DELETE", ["v1", "operator", "worlds", slug]) => {
            shared.registry().withdraw(op, &slug_of(slug))?;
            Ok((204, Value::Null))
        }
        ("POST", ["v1", "operator", "worlds", slug, "claim"]) => {
            let (world, key, issued) = shared.registry().claim_for(op, &slug_of(slug))?;
            Ok((
                201,
                json!({ "world": world, "key": key, "verification": issued.to_json() }),
            ))
        }
        ("POST", ["v1", "operator", "worlds", slug, "verification"]) => {
            let issued = shared.registry().new_verification(op, &slug_of(slug))?;
            Ok((200, json!({ "verification": issued.to_json() })))
        }
        ("POST", ["v1", "operator", "worlds", slug, "verify"]) => {
            let body: VerifyPage = parse_body(req)?;
            verify_page(shared, op, &slug_of(slug), &body.url)
        }
        ("POST", ["v1", "operator", "worlds", slug, "key"]) => {
            let key = shared.registry().rotate_key_for(op, &slug_of(slug))?;
            Ok((200, json!({ "key": key })))
        }
        ("PUT", ["v1", "operator", "worlds", slug, "federation"]) => {
            let body: FlagBody = parse_body(req)?;
            let flag = match body.flag.as_str() {
                "on" => Federation::On,
                "off" => Federation::Off,
                _ => return Err(fail(400, "flag: on or off")),
            };
            let slug = slug_of(slug);
            let reg = shared.registry();
            let approval = reg.federation_for(op, &slug, flag)?;
            let now = reg.federation(&slug)?.map_or(Federation::Unset, |(_, f)| f);
            Ok((
                200,
                json!({ "federation": now.word(), "approval": approval.word() }),
            ))
        }
        ("POST", ["v1", "reports"]) => {
            let body: ReportBody = parse_body(req)?;
            let report =
                shared
                    .registry()
                    .report(op, &slug_of(&body.slug), &body.reason, &body.details)?;
            Ok((201, json!({ "report": report })))
        }
        ("GET", ["v1", "operator", "reports"]) => Ok((200, shared.registry().my_reports(op)?)),
        ("GET", ["v1", "person"]) => {
            Ok((200, json!({ "person": shared.registry().person_json(op)? })))
        }
        // Each change has the routing look again at the views of the person's characters, those it
        // has now and any it had before, and at those naming one of them; a change that changed
        // nothing, at none.
        ("POST", ["v1", "person", "link"]) => {
            let body: LinkBody = parse_body(req)?;
            let reg = shared.registry();
            let linked = reg.redeem_link_code(op, &body.code)?;
            shared.persons_changed(reg.characters_of(op)?);
            Ok((200, linked))
        }
        ("DELETE", ["v1", "person", "characters", address]) => {
            let reg = shared.registry();
            let characters = reg.characters_of(op)?;
            reg.unlink_for(op, &crate::server::percent_decode(address))?;
            shared.persons_changed(characters);
            Ok((204, Value::Null))
        }
        ("PUT", ["v1", "person", "alts"]) => {
            let body: AltsBody = parse_body(req)?;
            let reg = shared.registry();
            let (person, changed) = reg.set_alts_for(op, body.shown)?;
            if changed {
                shared.persons_changed(reg.characters_of(op)?);
            }
            Ok((200, json!({ "person": person })))
        }
        ("DELETE", ["v1", "person", "friends", handle]) => {
            let reg = shared.registry();
            reg.unfriend_for(op, &crate::server::percent_decode(handle))?;
            shared.persons_changed(reg.characters_of(op)?);
            Ok((204, Value::Null))
        }
        (_, ["v1", "admin", ..]) if !admin => Err(fail(403, "Admins only")),
        ("GET", ["v1", "admin", "reports"]) => {
            let all = query(req.path).get("state").is_some_and(|s| s == "all");
            Ok((200, shared.registry().reports(all)?))
        }
        ("POST", ["v1", "admin", "reports", id]) => {
            let body: ResolveBody = parse_body(req)?;
            let out =
                shared
                    .registry()
                    .resolve_report(id, &body.action, &body.note, Some(&op.id))?;
            Ok((200, out))
        }
        _ => Err(fail(404, "Route not found")),
    }
}

/// What a page check that did not find the token says, however it failed: one answer, so the hub
/// tells nobody what it reached, how, or what answered (the hub's own log says, for its operator).
fn not_shown(slug: &str, why: &str) -> RegistryError {
    crate::server::log(
        "info",
        &format!("a page check for {slug} did not find the token: {why}"),
    );
    RegistryError::unverified("token_not_found", "The page did not show the token")
}

/// Why a fetch failed, for the hub's own log: never an address or a page.
fn fetch_failure(e: &FetchError) -> &'static str {
    match e {
        FetchError::NotAllowed(_) => "a redirect off the world's host",
        FetchError::AddressNotAllowed => "an address the hub may not reach",
        FetchError::Redirect => "a redirect the hub does not follow",
        FetchError::Timeout => "no answer in time",
        FetchError::Failed(_) => "no connection, TLS, or an answer that is not HTTP",
    }
}

/// Verifying by a page: count the attempt, read the page (no lock held), then look for the token.
fn verify_page(
    shared: &Shared,
    op: &Operator,
    slug: &str,
    url: &str,
) -> Result<(u16, Value), RegistryError> {
    let url = Url::parse(url).map_err(|e| fail(400, &format!("url: {e}")))?;
    // A page that is not on the world's host is refused before anything is counted: only a page
    // the hub fetches takes from the allowances.
    let hosts = shared.registry().page_hosts(op, slug)?;
    if let Err(why) = operators::on_world_host(&url, &hosts) {
        return Err(RegistryError::unverified("host_mismatch", why));
    }
    let hosts = shared.registry().page_attempt(op, slug)?;
    let checked = hosts.clone();
    let fetched = Fetch {
        method: "GET",
        url,
        headers: vec![(
            "Accept".into(),
            "text/plain, text/html;q=0.9, */*;q=0.1".into(),
        )],
        body: Vec::new(),
        timeout: shared.config.fetch_timeout,
        max_body: 64 * 1024,
        max_redirects: 3,
        rule: page_rule(shared.config.loopback_allowed),
        deny: shared.config.deny_fetch.clone(),
        check: operators::on_world_host,
        check_with: hosts,
    }
    .send();
    let reg = shared.registry();
    let outcome = match fetched {
        Err(e) => Err(not_shown(slug, fetch_failure(&e))),
        Ok(r) if r.status != 200 => Err(not_shown(slug, "an answer other than 200")),
        Ok(r) => reg.page_found(op, slug, &operators::tokens_in(&r.body), &checked),
    };
    match outcome {
        Ok((world, approval)) => Ok((200, json!({ "world": world, "approval": approval.word() }))),
        Err(e) => Err(reg.failed(slug, &op.id, e)),
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WorldVerify {
    token: String,
    #[serde(default)]
    by: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WorldAnswer {
    nonce: String,
}

/// The world's key, from `Authorization: Bearer drth_...`.
fn world_key(req: &Req<'_>) -> Option<String> {
    req.header("authorization")
        .and_then(|v| v.split_once(' '))
        .filter(|(scheme, k)| {
            scheme.eq_ignore_ascii_case("bearer") && crate::registry::is_key_shape(k.trim())
        })
        .map(|(_, k)| k.trim().to_owned())
}

/// A second verification of a world while one waits for its answer.
fn in_progress() -> RegistryError {
    RegistryError {
        reason: Some("in_progress"),
        ..RegistryError::conflict("A verification of this world is already waiting for its answer")
    }
}

/// The world's verification call, with its key, from any address: the key, the token and the
/// attempts are checked; then the hub challenges the world's game endpoint and holds the call until
/// the world posts the challenge's nonce back ([`world_answer`]), or until it gives up waiting.
fn world_verify(shared: &Shared, req: &Req<'_>, slug: &str) -> Reply {
    let cors = Cors::Nothing;
    let Some(key) = world_key(req) else {
        return Reply::error(&fail(401, "A valid key for this server is required"), &cors);
    };
    let body: WorldVerify = match parse_body(req) {
        Ok(b) => b,
        Err(e) => return Reply::error(&e, &cors),
    };
    // The key first: nothing is looked up or counted for a call without one.
    let known = shared.registry().game_endpoint(slug, &key);
    if let Err(e) = known {
        return Reply::error(&e, &cors);
    }
    if shared.challenges.waits_for(slug) {
        return Reply::error(&in_progress(), &cors);
    }
    let checked = shared.registry().game_check(slug, &key, &body.token);
    let check = match checked {
        Ok(c) => c,
        Err(e) => {
            let e = shared.registry().failed(slug, "world", e);
            return Reply::error(&e, &cors);
        }
    };
    // The lookup, the datagrams and the wait, with no lock held.
    let answered = challenge(shared, slug, &check, req.ip);
    let reg = shared.registry();
    match answered.and_then(|at| reg.game_answered(slug, &check, at, &body.by)) {
        Ok(approval) => {
            let flag = reg
                .federation(slug)
                .ok()
                .flatten()
                .map_or(Federation::Unset, |(_, f)| f);
            Reply::json(
                200,
                &json!({ "verified": true, "slug": slug, "federation": flag.word(),
                         "approval": approval.word() }),
                &cors,
            )
        }
        Err(e) => {
            let e = reg.failed(slug, "world", e);
            Reply::error(&e, &cors)
        }
    }
}

/// Whether the hub may send a challenge to `ip`: an address on the public internet (or loopback,
/// on a hub developed on one machine), outside every range its operator denies, as for a page.
fn may_challenge(shared: &Shared, ip: IpAddr) -> bool {
    page_rule(shared.config.loopback_allowed)(ip)
        && !crate::forwarded::is_trusted(&shared.config.deny_fetch, ip)
}

/// Challenges the world's game endpoint for `check`: the addresses its host resolves to (an
/// address is itself; an observed endpoint is `caller`, the call's own address), the first few the
/// hub may reach each sent its challenge, and the world's answer waited for. The address it
/// answered at.
fn challenge(
    shared: &Shared,
    slug: &str,
    check: &operators::GameCheck,
    caller: IpAddr,
) -> Result<IpAddr, RegistryError> {
    use crate::challenge::{shown_at, Refused, Sender, Target, COPIES, MAX_ADDRESSES};
    use crate::forwarded::canonical;
    use crate::resolver::Resolved;
    let port = check.endpoint.port();
    let (named, found): (Option<&str>, Vec<IpAddr>) = match &check.endpoint {
        GameEndpoint::Observed { .. } => (None, vec![canonical(caller)]),
        GameEndpoint::Host { host, .. } => match host.parse::<IpAddr>() {
            Ok(ip) => (None, vec![canonical(ip)]),
            Err(_) => match shared
                .resolver
                .resolve_within(host, shared.config.challenge_lookup_deadline)
            {
                Resolved::Found(f) if !f.is_empty() => {
                    (Some(host.as_str()), f.into_iter().map(canonical).collect())
                }
                Resolved::Found(_) => {
                    return Err(RegistryError::unverified(
                        "not_resolved",
                        format!("{host} does not resolve to any address: check its DNS records"),
                    ))
                }
                Resolved::Unknown => {
                    return Err(RegistryError::unverified(
                        "not_resolved",
                        format!("{host} did not resolve in time: try again in a minute"),
                    ))
                }
            },
        },
    };
    let mut once: Vec<IpAddr> = Vec::new();
    for ip in found {
        if !once.contains(&ip) {
            once.push(ip);
        }
    }
    let list = |ips: &[IpAddr]| {
        ips.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let tried: Vec<IpAddr> = once
        .iter()
        .copied()
        .filter(|ip| may_challenge(shared, *ip))
        .take(MAX_ADDRESSES)
        .collect();
    if tried.is_empty() {
        let all = list(&once);
        let message = match (&check.endpoint, named) {
            (GameEndpoint::Observed { .. }, _) => {
                format!("The call came from {all}, which is not an address on the public internet")
            }
            (_, Some(host)) => format!(
                "{host} resolves to no address the hub may reach on the public internet ({all})"
            ),
            (_, None) => {
                format!("{all} is not an address the hub may reach on the public internet")
            }
        };
        return Err(RegistryError::unverified("address_not_public", message));
    }
    let targets: Vec<Target> = tried.iter().map(|ip| Target::new(*ip, port)).collect();
    let ticket = match shared.challenges.open(slug, check.key, targets.clone()) {
        Ok(t) => t,
        Err(Refused::Busy) => return Err(in_progress()),
        Err(Refused::Full) => {
            return Err(RegistryError::limited(
                "The hub is checking too many worlds at once",
                5,
            ))
        }
    };
    let sender = Sender::for_targets(&targets);
    let (every, wait) = (shared.config.challenge_every, shared.config.challenge_wait);
    let started = std::time::Instant::now();
    let mut answered = None;
    for copy in 1..=COPIES {
        sender.send_round(slug, &targets);
        let next = if copy == COPIES {
            started + wait
        } else {
            started + every * u32::try_from(copy).unwrap_or(u32::MAX)
        };
        answered = ticket.answered_by(next);
        if answered.is_some() {
            break;
        }
    }
    drop(ticket);
    let to = match named {
        Some(host) => format!("{host}:{port} ({})", list(&tried)),
        None => shown_at(tried[0], port),
    };
    if let Some(at) = answered {
        crate::server::log(
            "info",
            &format!("{slug}'s verification in game was answered at {at}"),
        );
        return Ok(at.ip());
    }
    crate::server::log(
        "info",
        &format!("{slug}'s verification in game: no answer from {to}"),
    );
    let came_from = if matches!(check.endpoint, GameEndpoint::Observed { .. }) {
        ", the address this call came from,"
    } else {
        ""
    };
    Err(RegistryError::unverified(
        "no_answer",
        format!(
            "The hub's challenge to {to}{came_from} got no answer: is the game port reachable from the internet?"
        ),
    ))
}

/// The world's answer to the hub's challenge: the nonce it read on its game port, posted with its
/// key from any address. It answers only a challenge waiting for this world, sent for a call that
/// came with the same key.
fn world_answer(shared: &Shared, req: &Req<'_>, slug: &str) -> Reply {
    let cors = Cors::Nothing;
    let Some(key) = world_key(req) else {
        return Reply::error(&fail(401, "A valid key for this server is required"), &cors);
    };
    let body: WorldAnswer = match parse_body(req) {
        Ok(b) => b,
        Err(e) => return Reply::error(&e, &cors),
    };
    let known = shared.registry().game_endpoint(slug, &key);
    if let Err(e) = known {
        return Reply::error(&e, &cors);
    }
    let answered = dereth_network_protocol::challenge::parse_nonce_hex(body.nonce.trim())
        .is_some_and(|n| {
            shared
                .challenges
                .answer(slug, &crate::registry::digest(&key), &n)
        });
    if answered {
        Reply::json(200, &json!({ "answered": true }), &cors)
    } else {
        Reply::error(
            &RegistryError::unverified(
                "wrong_nonce",
                "That is not the nonce of a challenge waiting for this world",
            ),
            &cors,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec: hub: a finished sign-in goes back under one of the hub's sites, and a sign-in begun
    /// before sites were carried (its cookie holds a path alone) goes back to the first.
    #[test]
    fn a_finished_sign_in_goes_back_only_under_one_of_the_hubs_sites() {
        let sites = vec![
            "https://site.test".to_owned(),
            "https://dev.site.test".to_owned(),
        ];
        assert_eq!(
            back_to(&sites, "/characters/"),
            "https://site.test/characters/"
        );
        assert_eq!(
            back_to(&sites, "https://dev.site.test/characters/"),
            "https://dev.site.test/characters/"
        );
        for elsewhere in [
            "https://evil.test/characters/",
            "https://dev.site.test.evil.test/",
            "https://site.testevil/",
            "https://dev.site.test",
        ] {
            assert_eq!(
                back_to(&sites, elsewhere),
                "https://site.test/",
                "{elsewhere}"
            );
        }
        assert_eq!(
            started_from(&sites, Some(&"https://dev.site.test".to_owned())),
            "https://dev.site.test"
        );
        assert_eq!(
            started_from(&sites, Some(&"https://evil.test".to_owned())),
            "https://site.test"
        );
        assert_eq!(started_from(&sites, None), "https://site.test");
    }

    /// Spec: hub: sign-in returns only to a path on the site.
    #[test]
    fn sign_in_returns_only_to_a_path_on_the_site() {
        let p = |s: &str| return_path(Some(&s.to_owned()));
        assert_eq!(p("/operator/worlds?x=1"), "/operator/worlds?x=1");
        for bad in [
            "//evil.example/",
            "https://evil.example/",
            "/\\evil.example",
            "operator",
            "/a b",
            "/a\r\nSet-Cookie: x=1",
        ] {
            assert_eq!(p(bad), "/", "{bad:?}");
        }
        assert_eq!(return_path(None), "/");
        assert_eq!(p(&format!("/{}", "a".repeat(250))), "/");
    }

    /// Spec: hub: the operator API's routes, and the world's verification call and its answer.
    #[test]
    fn the_api_answers_its_own_routes_only() {
        for yes in [
            "/v1/auth/discord?return=/",
            "/v1/operator",
            "/v1/operator/worlds/coldeve",
            "/v1/reports",
            "/v1/admin/reports",
            "/v1/servers/coldeve/verify",
            "/v1/servers/coldeve/verify/answer",
            "/v1/person",
            "/v1/person/characters/Ann@frost",
        ] {
            assert!(is_api(yes), "{yes}");
        }
        for no in [
            "/v1/servers",
            "/v1/persons",
            "/v1/servers/coldeve",
            "/v1/servers/coldeve/heartbeat",
            "/v1/servers/coldeve/verify/answer/more",
            "/v1/operators",
            "/healthz",
        ] {
            assert!(!is_api(no), "{no}");
        }
    }

    /// Spec: hub: a sign-in in progress is its cookie's, signed by the hub: it holds while it is
    /// as the hub wrote it and has not lapsed, and is refused once altered, lapsed or signed under
    /// another key.
    #[test]
    fn a_sign_in_cookie_holds_only_as_the_hub_signed_it() {
        let key = [7u8; 32];
        let cookie = sign_in_cookie(&key, "the-state", 2_000, "/characters/");
        assert_eq!(
            read_sign_in_cookie(&key, &cookie, 1_999),
            Some(("the-state".to_owned(), "/characters/".to_owned()))
        );
        assert_eq!(read_sign_in_cookie(&key, &cookie, 2_000), None, "lapsed");
        assert_eq!(
            read_sign_in_cookie(&[8u8; 32], &cookie, 1_000),
            None,
            "another key"
        );
        let (body, mac) = cookie.rsplit_once('~').expect("signed");
        for altered in [
            cookie.replacen("the-state", "an-other", 1),
            cookie.replacen("~2000~", "~9999~", 1),
            format!("{body}~{}", &mac[1..]),
            format!("{body}x~{mac}"),
            String::new(),
            "no tildes".to_owned(),
        ] {
            assert_eq!(
                read_sign_in_cookie(&key, &altered, 1_000),
                None,
                "{altered}"
            );
        }
    }
}
