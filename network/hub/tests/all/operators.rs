//! The operator API on a running hub: Discord sign-in and sessions, the origins that may use them,
//! registering and verifying a world (by a page on its host, or in game by answering the hub's
//! challenge at its endpoint), approval, keys, reports and revocation, the limits, and the client
//! address behind a proxy.
//! Fixture: a hub on loopback over an in-memory registry on a clock the test turns, a stand-in for
//! Discord's API, a page server the hub reads verification pages from, and a socket on loopback
//! standing in for the worlds' game port, which answers the hub's challenges.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use dereth_network_conformance::hub::Link;
use dereth_network_hub::discord::{any_address, Discord};
use dereth_network_hub::registry::{Registration, Registry};
use dereth_network_protocol::challenge::{nonce_hex, Challenge};
use dereth_network_hub::server::{start_with, HubConfig, Running};
use serde_json::{json, Value};

pub(super) const SITE: &str = "https://site.test";
pub(super) const ADMIN: &str = "900";

// ---- a small HTTP server, for Discord and for verification pages -------------------------------

/// A request a stub server received.
#[derive(Debug, Clone)]
pub(super) struct Got {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Got {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

pub(super) type Handler = Arc<dyn Fn(&Got, &mut TcpStream) + Send + Sync>;

/// A server on an ephemeral loopback port, answering each connection on a thread of its own.
pub(super) struct Stub {
    pub(super) addr: SocketAddr,
    hits: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
}

impl Stub {
    pub(super) fn start(handler: Handler) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.set_nonblocking(true).expect("nonblocking");
        let addr = listener.local_addr().expect("an address");
        let hits = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (h, s) = (Arc::clone(&hits), Arc::clone(&stop));
        std::thread::spawn(move || {
            while !s.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        h.fetch_add(1, Ordering::SeqCst);
                        let handler = Arc::clone(&handler);
                        std::thread::spawn(move || {
                            let _ = stream.set_nonblocking(false);
                            if let Some(got) = read_request(&mut stream) {
                                handler(&got, &mut stream);
                            }
                        });
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        Self { addr, hits, stop }
    }

    pub(super) fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

impl Drop for Stub {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn read_request(stream: &mut TcpStream) -> Option<Got> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut buf = Vec::new();
    let mut b = [0u8; 1024];
    let head_end = loop {
        let n = stream.read(&mut b).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&b[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let first: Vec<&str> = lines.next()?.split(' ').collect();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect();
    let len: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = buf[head_end..].to_vec();
    while body.len() < len {
        let n = stream.read(&mut b).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&b[..n]);
    }
    Some(Got {
        method: first.first()?.to_string(),
        path: first.get(1)?.to_string(),
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

pub(super) fn answer(stream: &mut TcpStream, status: u16, extra: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n{body}",
        body.len()
    );
}

/// Discord's API as the hub uses it: `code-<id>` is exchanged for `tok-<id>`, which reads user
/// `<id>`; `code-busy` is answered `429`, asking for nothing for 30 seconds.
fn discord_stub() -> Stub {
    Stub::start(Arc::new(|got: &Got, s: &mut TcpStream| {
        match (got.method.as_str(), got.path.as_str()) {
            ("POST", "/api/oauth2/token") => {
                let form: HashMap<&str, &str> = got
                    .body
                    .split('&')
                    .filter_map(|kv| kv.split_once('='))
                    .collect();
                match (form.get("code"), form.get("client_secret")) {
                    (Some(&"code-busy"), _) => answer(
                        s,
                        429,
                        "Retry-After: 30\r\nContent-Type: application/json\r\n",
                        r#"{"message":"You are being rate limited.","retry_after":30.0,"global":false}"#,
                    ),
                    (Some(code), Some(&"the-secret")) if code.starts_with("code-") => answer(
                        s,
                        200,
                        "",
                        &json!({ "access_token": format!("tok-{}", &code[5..]), "token_type": "Bearer" })
                            .to_string(),
                    ),
                    _ => answer(s, 400, "", r#"{"error":"invalid_grant"}"#),
                }
            }
            ("GET", "/api/users/@me") => match got.header("authorization").and_then(|a| a.strip_prefix("Bearer tok-")) {
                Some(id) => answer(
                    s,
                    200,
                    "",
                    &json!({ "id": id, "username": format!("user{id}"), "global_name": format!("Operator {id}") })
                        .to_string(),
                ),
                None => answer(s, 401, "", "{}"),
            },
            _ => answer(s, 404, "", "{}"),
        }
    }))
}

// ---- the hub ------------------------------------------------------------------------------------

pub(super) struct Fixture {
    pub(super) running: Running,
    clock: Arc<Mutex<SystemTime>>,
    discord: Stub,
}

impl Fixture {
    pub(super) fn addr(&self) -> SocketAddr {
        self.running.local_addr()
    }

    /// How many requests the stand-in for Discord has had.
    pub(super) fn discord_calls(&self) -> usize {
        self.discord.hits()
    }

    pub(super) fn advance(&self, d: Duration) {
        *self.clock.lock().expect("the clock") += d;
    }
}

pub(super) fn hub_with(tweak: impl FnOnce(&mut HubConfig)) -> Fixture {
    let clock = Arc::new(Mutex::new(SystemTime::now()));
    let c = Arc::clone(&clock);
    let registry = Registry::open(
        ":memory:",
        "dereth.network",
        Box::new(move || *c.lock().expect("the clock")),
    )
    .expect("a registry");
    let discord = discord_stub();
    let mut config = HubConfig {
        listen: "127.0.0.1:0".parse().expect("an address"),
        db: ":memory:".into(),
        public_url: Some("https://hub.test".into()),
        site_origins: vec![SITE.into()],
        discord: Some(Discord {
            client_id: "app".into(),
            client_secret: "the-secret".into(),
            authorize: "https://discord.test/oauth2/authorize".into(),
            api: format!("http://{}/api", discord.addr),
            rule: any_address,
        }),
        admins: vec![ADMIN.into()],
        loopback_allowed: true,
        fetch_timeout: Duration::from_secs(2),
        http_per_minute: 100_000,
        challenge_every: Duration::from_millis(100),
        challenge_wait: Duration::from_millis(1_000),
        ..HubConfig::default()
    };
    tweak(&mut config);
    let running = start_with(config, registry).expect("the hub starts");
    Fixture {
        running,
        clock,
        discord,
    }
}

pub(super) fn hub() -> Fixture {
    hub_with(|_| {})
}

// ---- requests -----------------------------------------------------------------------------------

#[derive(Debug)]
pub(super) struct Answer {
    pub(super) status: u16,
    pub(super) headers: Vec<(String, String)>,
    pub(super) body: String,
}

impl Answer {
    pub(super) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub(super) fn cookie(&self, name: &str) -> Option<String> {
        self.headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
            .find(|(_, v)| v.starts_with(&format!("{name}=")))
            .map(|(_, v)| v.clone())
    }

    pub(super) fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }

    pub(super) fn reason(&self) -> String {
        self.json()["error"]["reason"]
            .as_str()
            .unwrap_or("")
            .to_owned()
    }
}

pub(super) fn call(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Answer {
    let mut s = TcpStream::connect(addr).expect("connect");
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: hub.test\r\n");
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
    s.write_all(head.as_bytes()).expect("write");
    s.write_all(body.as_bytes()).expect("write");
    let mut out = String::new();
    s.read_to_string(&mut out).expect("read");
    let (head, body) = out.split_once("\r\n\r\n").unwrap_or((&out, ""));
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect();
    Answer {
        status,
        headers,
        body: body.to_owned(),
    }
}

/// Starts a sign-in coming back to `path`: its state, and the cookie the hub set for it.
pub(super) fn start_sign_in(f: &Fixture, path: &str) -> (String, String) {
    let start = call(f.addr(), "GET", path, &[], "");
    let state = start
        .header("location")
        .and_then(|l| l.split('&').find_map(|kv| kv.strip_prefix("state=")))
        .expect("a state")
        .to_owned();
    let cookie = start
        .cookie("__Host-dn_oauth")
        .and_then(|c| c.split(';').next().map(str::to_owned))
        .expect("the sign-in cookie");
    (state, cookie)
}

/// Spec: hub: a sign-in returns to the site it was started from, when that is one of the hub's
/// site origins, and to the first otherwise, so no other address can be made the place a sign-in
/// returns to.
#[test]
fn a_sign_in_returns_to_the_site_it_was_started_from_when_the_hub_lists_it() {
    let f = hub_with(|c| c.site_origins.push("https://dev.site.test".to_owned()));
    let finish = |start: &str| {
        let (state, cookie) = start_sign_in(&f, start);
        call(
            f.addr(),
            "GET",
            &format!("/v1/auth/discord/callback?code=code-777&state={state}"),
            &[("Cookie", &cookie)],
            "",
        )
    };
    let dev = finish("/v1/auth/discord?return=/characters/&site=https%3A%2F%2Fdev.site.test");
    assert_eq!(
        dev.header("location"),
        Some("https://dev.site.test/characters/")
    );
    assert!(dev.cookie("__Host-dn_session").is_some(), "{dev:?}");
    let first = finish("/v1/auth/discord?return=/characters/&site=https%3A%2F%2Fsite.test");
    assert_eq!(
        first.header("location"),
        Some("https://site.test/characters/")
    );
    for elsewhere in [
        "https%3A%2F%2Fevil.test",
        "https%3A%2F%2Fdev.site.test.evil.test",
        "https%3A%2F%2Fdev.site.test%2F",
        "",
    ] {
        let back = finish(&format!(
            "/v1/auth/discord?return=/characters/&site={elsewhere}"
        ));
        assert_eq!(
            back.header("location"),
            Some("https://site.test/characters/"),
            "{elsewhere}"
        );
    }
    // A failed sign-in goes back to the same site, saying so.
    let (state, cookie) = start_sign_in(
        &f,
        "/v1/auth/discord?return=/characters/&site=https%3A%2F%2Fdev.site.test",
    );
    let failed = call(
        f.addr(),
        "GET",
        &format!("/v1/auth/discord/callback?code=code-778&state={state}x"),
        &[("Cookie", &cookie)],
        "",
    );
    assert_eq!(
        failed.header("location"),
        Some("https://dev.site.test/characters/?signin=failed")
    );
}

/// Signs the Discord user `id` in, through the hub and the stand-in: the session's id.
pub(super) fn sign_in(f: &Fixture, id: &str) -> String {
    let start = call(
        f.addr(),
        "GET",
        "/v1/auth/discord?return=/operator/",
        &[],
        "",
    );
    assert_eq!(start.status, 302, "{start:?}");
    let location = start.header("location").expect("a location").to_owned();
    assert!(
        location.starts_with("https://discord.test/oauth2/authorize?"),
        "{location}"
    );
    assert!(location.contains("scope=identify"), "{location}");
    let state = location
        .split('&')
        .find_map(|kv| kv.strip_prefix("state="))
        .expect("a state")
        .to_owned();
    let oauth = start.cookie("__Host-dn_oauth").expect("the state cookie");
    assert!(
        oauth.contains("HttpOnly") && oauth.contains("Secure") && oauth.contains("SameSite=Lax"),
        "{oauth}"
    );
    let cookie = oauth.split(';').next().expect("a value").to_owned();
    let back = call(
        f.addr(),
        "GET",
        &format!("/v1/auth/discord/callback?code=code-{id}&state={state}"),
        &[("Cookie", &cookie)],
        "",
    );
    assert_eq!(back.status, 303, "{back:?}");
    assert_eq!(back.header("location"), Some("https://site.test/operator/"));
    let session = back.cookie("__Host-dn_session").expect("a session cookie");
    session
        .split(';')
        .next()
        .and_then(|c| c.split_once('='))
        .map(|(_, v)| v.to_owned())
        .expect("a session id")
}

/// A session route, from the site, with the session.
pub(super) fn op(f: &Fixture, session: &str, method: &str, path: &str, body: &Value) -> Answer {
    let cookie = format!("__Host-dn_session={session}");
    let body = if body.is_null() {
        String::new()
    } else {
        body.to_string()
    };
    call(
        f.addr(),
        method,
        path,
        &[
            ("Cookie", &cookie),
            ("Origin", SITE),
            ("Content-Type", "application/json"),
        ],
        &body,
    )
}

/// A world at `address`, on this test's game port ([`game_port`]).
pub(super) fn world(slug: &str, address: &str) -> Value {
    json!({
        "slug": slug, "name": format!("World {slug}"), "ruleset": "PvE",
        "endpoint": { "mode": "manual", "address": address, "port": game_port() },
        "emulator": { "family": "ACE" },
    })
}

/// Registers `slug` for the operator of `session`: its key and token.
pub(super) fn register(f: &Fixture, session: &str, slug: &str, address: &str) -> (String, String) {
    let a = op(
        f,
        session,
        "POST",
        "/v1/operator/worlds",
        &world(slug, address),
    );
    assert_eq!(a.status, 201, "{a:?}");
    let v = a.json();
    assert_eq!(v["world"]["ownership"]["state"], "pending");
    (
        v["key"].as_str().expect("a key").to_owned(),
        v["verification"]["token"]
            .as_str()
            .expect("a token")
            .to_owned(),
    )
}

// ---- the worlds' game port ----------------------------------------------------------------------

thread_local! {
    /// The game port of this test's worlds: a socket on loopback standing in for it.
    static GAME_PORT: UdpSocket = UdpSocket::bind("127.0.0.1:0").expect("a game port");
}

/// The port this test's worlds are registered at, on loopback.
pub(super) fn game_port() -> u16 {
    GAME_PORT.with(|s| s.local_addr().expect("an address").port())
}

/// How the stand-in world answers the hub's challenges while a verification call waits.
#[derive(Debug, Clone, Default)]
pub(super) struct Answering {
    /// Headers each answer carries besides the key (a forwarded address, for an answer from
    /// another address than the call's).
    pub(super) headers: Vec<(String, String)>,
    /// Never answers: the challenges are only heard.
    pub(super) silent: bool,
    /// Answers this instead of the nonce it read.
    pub(super) nonce: Option<String>,
    /// Who the call says typed `/dn verify` (`Asheron` when none).
    pub(super) by: Option<String>,
}

/// What the stand-in heard while a call waited: each challenge for the world, with where it came
/// from, and the status of each answer it posted.
#[derive(Debug, Default)]
pub(super) struct Heard {
    pub(super) challenges: Vec<(SocketAddr, Challenge)>,
    pub(super) answers: Vec<u16>,
}

/// The world's answer to a challenge: `nonce`, posted with `key`.
pub(super) fn answer_challenge(
    addr: SocketAddr,
    slug: &str,
    key: &str,
    nonce: &str,
    extra: &[(String, String)],
) -> Answer {
    let auth = format!("Bearer {key}");
    let mut headers = vec![
        ("Authorization", auth.as_str()),
        ("Content-Type", "application/json"),
    ];
    headers.extend(extra.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    call(
        addr,
        "POST",
        &format!("/v1/servers/{slug}/verify/answer"),
        &headers,
        &json!({ "nonce": nonce }).to_string(),
    )
}

/// The world's verification call, with the stand-in on this test's game port answering the hub's
/// challenges for `slug` as `answering` says, each distinct nonce once: the hub's answer to the
/// call, and what the stand-in heard.
pub(super) fn verify_answering(
    f: &Fixture,
    slug: &str,
    key: &str,
    token: &str,
    extra: &[(&str, &str)],
    answering: &Answering,
) -> (Answer, Heard) {
    // Polled without blocking: a read that times out can lose a datagram on some systems.
    let socket = GAME_PORT.with(|s| s.try_clone().expect("the game port"));
    socket.set_nonblocking(true).expect("nonblocking");
    let stop = Arc::new(AtomicBool::new(false));
    let (hub, world, world_key, how, stopped) = (
        f.addr(),
        slug.to_owned(),
        key.to_owned(),
        answering.clone(),
        Arc::clone(&stop),
    );
    let listener = std::thread::spawn(move || {
        let mut heard = Heard::default();
        let mut posted = Vec::new();
        let mut buf = [0u8; 2048];
        let mut quiet_after_stop = 0;
        // Until the call is over, and then until the socket has been quiet a while, so that no
        // challenge is left for the next call to hear.
        while quiet_after_stop < 5 {
            let Ok((n, from)) = socket.recv_from(&mut buf) else {
                if stopped.load(Ordering::SeqCst) {
                    quiet_after_stop += 1;
                }
                std::thread::sleep(Duration::from_millis(10));
                continue;
            };
            let Some(c) = Challenge::parse(&buf[..n]) else {
                continue;
            };
            if c.slug != world {
                continue;
            }
            heard.challenges.push((from, c.clone()));
            if how.silent || stopped.load(Ordering::SeqCst) || posted.contains(&c.nonce) {
                continue;
            }
            posted.push(c.nonce);
            let nonce = how.nonce.clone().unwrap_or_else(|| nonce_hex(&c.nonce));
            let a = answer_challenge(hub, &world, &world_key, &nonce, &how.headers);
            heard.answers.push(a.status);
        }
        heard
    });
    let auth = format!("Bearer {key}");
    let mut headers = vec![
        ("Authorization", auth.as_str()),
        ("Content-Type", "application/json"),
    ];
    headers.extend_from_slice(extra);
    let a = call(
        f.addr(),
        "POST",
        &format!("/v1/servers/{slug}/verify"),
        &headers,
        &json!({ "token": token, "by": answering.by.as_deref().unwrap_or("Asheron") })
            .to_string(),
    );
    stop.store(true, Ordering::SeqCst);
    let heard = listener.join().expect("the stand-in world");
    (a, heard)
}

/// The world's verification call, with the world answering the hub's challenge on this test's
/// game port, from the address the call came from (`extra`'s forwarded address, if it has one).
pub(super) fn game_verify(
    f: &Fixture,
    slug: &str,
    key: &str,
    token: &str,
    extra: &[(&str, &str)],
) -> Answer {
    let answering = Answering {
        headers: extra
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        ..Answering::default()
    };
    verify_answering(f, slug, key, token, extra, &answering).0
}

pub(super) fn federation(f: &Fixture, slug: &str) -> String {
    f.running
        .registry()
        .federation(slug)
        .expect("read")
        .map(|(_, fl)| fl.word().to_owned())
        .unwrap_or_default()
}

// ---- sessions -----------------------------------------------------------------------------------

/// Spec: hub: once Discord answers `429`, the hub calls it for nothing until the wait it named is
/// over: a sign-in started, or coming back, answers `signin=busy` at once; after the wait, signing
/// in works again.
#[test]
fn after_discord_answers_429_sign_ins_wait_without_calling_it() {
    let f = hub();
    let (before, before_cookie) = start_sign_in(&f, "/v1/auth/discord?return=/operator/");
    let (state, cookie) = start_sign_in(&f, "/v1/auth/discord?return=/operator/");
    let back = call(
        f.addr(),
        "GET",
        &format!("/v1/auth/discord/callback?code=code-busy&state={state}"),
        &[("Cookie", &cookie)],
        "",
    );
    assert_eq!(back.status, 303, "{back:?}");
    let busy = Some("https://site.test/operator/?signin=busy");
    assert_eq!(back.header("location"), busy);
    assert!(back.cookie("__Host-dn_session").is_none());
    let calls = f.discord_calls();
    assert_eq!(calls, 1, "the code, refused");
    let start = call(
        f.addr(),
        "GET",
        "/v1/auth/discord?return=/operator/",
        &[],
        "",
    );
    assert_eq!(start.status, 303, "not sent to Discord: {start:?}");
    assert_eq!(start.header("location"), busy);
    let back = call(
        f.addr(),
        "GET",
        &format!("/v1/auth/discord/callback?code=code-601&state={before}"),
        &[("Cookie", &before_cookie)],
        "",
    );
    assert_eq!(back.header("location"), busy, "one started before");
    assert!(back.cookie("__Host-dn_session").is_none());
    assert_eq!(f.discord_calls(), calls, "Discord is called for nothing");
    f.advance(Duration::from_secs(29));
    assert_eq!(
        call(f.addr(), "GET", "/v1/auth/discord", &[], "").status,
        303,
        "within the wait"
    );
    f.advance(Duration::from_secs(1));
    let session = sign_in(&f, "601");
    assert_eq!(
        op(&f, &session, "GET", "/v1/operator", &Value::Null).json()["operator"]["discordId"],
        "601"
    );
    f.running.shutdown();
}

/// Spec: hub: signing in with Discord starts a session in an HTTP-only cookie for the hub's host;
/// the state binds the sign-in to its browser and is spent once; signing out ends the session.
#[test]
fn signing_in_with_discord_starts_a_session_bound_to_the_browser_and_signing_out_ends_it() {
    let f = hub();
    let session = sign_in(&f, "501");
    let me = op(&f, &session, "GET", "/v1/operator", &Value::Null);
    assert_eq!(me.status, 200, "{me:?}");
    assert_eq!(me.json()["operator"]["discordId"], "501");
    assert_eq!(me.json()["operator"]["name"], "Operator 501");
    assert_eq!(me.json()["operator"]["admin"], false);

    // A callback whose state is not the browser's, or is spent, starts nothing.
    let (state, cookie) = start_sign_in(&f, "/v1/auth/discord");
    let (_, others) = start_sign_in(&f, "/v1/auth/discord");
    for (cookie, why) in [
        (others, "another sign-in's cookie"),
        (
            "__Host-dn_oauth=someone-elses".to_owned(),
            "another browser's state",
        ),
        (String::new(), "no state cookie"),
    ] {
        let mut headers = Vec::new();
        if !cookie.is_empty() {
            headers.push(("Cookie", cookie.as_str()));
        }
        let back = call(
            f.addr(),
            "GET",
            &format!("/v1/auth/discord/callback?code=code-502&state={state}"),
            &headers,
            "",
        );
        assert_eq!(back.status, 303, "{why}");
        assert_eq!(
            back.header("location"),
            Some("https://site.test/?signin=failed"),
            "{why}"
        );
        assert!(back.cookie("__Host-dn_session").is_none(), "{why}");
    }
    let first = call(
        f.addr(),
        "GET",
        &format!("/v1/auth/discord/callback?code=code-502&state={state}"),
        &[("Cookie", &cookie)],
        "",
    );
    assert!(first.cookie("__Host-dn_session").is_some(), "{first:?}");
    let replay = call(
        f.addr(),
        "GET",
        &format!("/v1/auth/discord/callback?code=code-502&state={state}"),
        &[("Cookie", &cookie)],
        "",
    );
    assert!(
        replay.cookie("__Host-dn_session").is_none(),
        "a state is spent by its first use"
    );
    let refused = call(
        f.addr(),
        "GET",
        "/v1/auth/discord/callback?code=bad&state=x",
        &[("Cookie", "__Host-dn_oauth=x")],
        "",
    );
    assert!(
        refused.cookie("__Host-dn_session").is_none(),
        "an unknown state"
    );

    // The session's cookie.
    let (state, cookie) = start_sign_in(&f, "/v1/auth/discord?return=//evil.example/");
    let back = call(
        f.addr(),
        "GET",
        &format!("/v1/auth/discord/callback?code=code-503&state={state}"),
        &[("Cookie", &cookie)],
        "",
    );
    assert_eq!(
        back.header("location"),
        Some("https://site.test/"),
        "only a path on the site"
    );
    let c = back.cookie("__Host-dn_session").expect("a session cookie");
    for attr in ["Path=/", "Secure", "HttpOnly", "SameSite=Lax"] {
        assert!(c.contains(attr), "{c}");
    }
    assert!(
        !c.to_ascii_lowercase().contains("domain="),
        "a host-only cookie: {c}"
    );

    let out = op(&f, &session, "POST", "/v1/auth/signout", &Value::Null);
    assert_eq!(out.status, 204);
    assert!(out
        .cookie("__Host-dn_session")
        .is_some_and(|c| c.contains("Max-Age=0")));
    assert_eq!(
        op(&f, &session, "GET", "/v1/operator", &Value::Null).status,
        401
    );
    f.running.shutdown();
}

/// Spec: hub: a session lasts 30 days, ends when its operator is disabled, and is nothing but its
/// own id.
#[test]
fn a_session_lapses_after_thirty_days_and_ends_when_its_operator_is_disabled() {
    let f = hub();
    let session = sign_in(&f, "511");
    assert_eq!(
        op(&f, &session, "GET", "/v1/operator", &Value::Null).status,
        200
    );
    let other = if session.ends_with('A') { 'B' } else { 'A' };
    let forged = format!("{}{other}", &session[..42]);
    assert_eq!(
        op(&f, &forged, "GET", "/v1/operator", &Value::Null).status,
        401,
        "another id"
    );
    f.advance(Duration::from_secs(30 * 86_400 - 60));
    assert_eq!(
        op(&f, &session, "GET", "/v1/operator", &Value::Null).status,
        200
    );
    f.advance(Duration::from_secs(120));
    assert_eq!(
        op(&f, &session, "GET", "/v1/operator", &Value::Null).status,
        401,
        "lapsed"
    );

    let again = sign_in(&f, "511");
    f.running
        .registry()
        .set_operator_disabled("511", true)
        .expect("disabled");
    assert_eq!(
        op(&f, &again, "GET", "/v1/operator", &Value::Null).status,
        401,
        "ended"
    );
    let (state, cookie) = start_sign_in(&f, "/v1/auth/discord");
    let back = call(
        f.addr(),
        "GET",
        &format!("/v1/auth/discord/callback?code=code-511&state={state}"),
        &[("Cookie", &cookie)],
        "",
    );
    assert!(
        back.cookie("__Host-dn_session").is_none(),
        "a disabled operator cannot sign in"
    );
    f.running
        .registry()
        .set_operator_disabled("511", false)
        .expect("enabled");
    sign_in(&f, "511");
    f.running.shutdown();
}

/// Spec: hub: the operator routes answer CORS for the site's origins only, and a change is refused
/// unless it comes from the site or the hub itself.
#[test]
fn only_the_site_may_use_a_session_and_a_change_from_elsewhere_is_refused() {
    let f = hub();
    let session = sign_in(&f, "521");
    let pre = call(
        f.addr(),
        "OPTIONS",
        "/v1/operator/worlds",
        &[("Origin", SITE)],
        "",
    );
    assert_eq!(pre.status, 204);
    assert_eq!(pre.header("access-control-allow-origin"), Some(SITE));
    assert_eq!(pre.header("access-control-allow-credentials"), Some("true"));
    assert!(pre
        .header("access-control-allow-methods")
        .is_some_and(|m| m.contains("PUT")));
    let evil = call(
        f.addr(),
        "OPTIONS",
        "/v1/operator/worlds",
        &[("Origin", "https://evil.test")],
        "",
    );
    assert_eq!(evil.status, 403);
    assert!(evil.header("access-control-allow-origin").is_none());

    let me = op(&f, &session, "GET", "/v1/operator", &Value::Null);
    assert_eq!(me.header("access-control-allow-origin"), Some(SITE));
    assert_eq!(me.header("vary"), Some("Origin"));
    let cookie = format!("__Host-dn_session={session}");
    let read = call(
        f.addr(),
        "GET",
        "/v1/operator",
        &[("Cookie", &cookie), ("Origin", "https://evil.test")],
        "",
    );
    assert!(
        read.header("access-control-allow-origin").is_none(),
        "another origin cannot read it"
    );

    let body = world("crossed", "127.0.0.1").to_string();
    for origin in [
        Some("https://evil.test"),
        Some("https://other.dereth.network"),
        None,
    ] {
        let mut headers = vec![
            ("Cookie", cookie.as_str()),
            ("Content-Type", "application/json"),
        ];
        if let Some(o) = origin {
            headers.push(("Origin", o));
        }
        let a = call(f.addr(), "POST", "/v1/operator/worlds", &headers, &body);
        assert_eq!(a.status, 403, "{origin:?}");
    }
    assert!(
        f.running
            .registry()
            .server("crossed")
            .expect("read")
            .is_none(),
        "nothing was registered"
    );
    let own = call(
        f.addr(),
        "POST",
        "/v1/operator/worlds",
        &[
            ("Cookie", &cookie),
            ("Content-Type", "application/json"),
            ("Origin", "https://hub.test"),
        ],
        &body,
    );
    assert_eq!(own.status, 201, "the hub's own origin: {own:?}");
    let not_json = call(
        f.addr(),
        "POST",
        "/v1/operator/worlds",
        &[
            ("Cookie", &cookie),
            ("Content-Type", "text/plain"),
            ("Origin", SITE),
        ],
        &world("plain", "127.0.0.1").to_string(),
    );
    assert_eq!(not_json.status, 400, "a body is JSON");
    let public = call(
        f.addr(),
        "GET",
        "/v1/servers",
        &[("Origin", "https://evil.test")],
        "",
    );
    assert_eq!(
        public.header("access-control-allow-origin"),
        Some("*"),
        "the public routes stay open"
    );
    f.running.shutdown();
}

// ---- registering and verifying ------------------------------------------------------------------

/// Spec: hub: a world is verified in game only by a call with its key and its own open token, and
/// the hub's challenge answered at its endpoint; then it is managed and approved onto the network.
#[test]
fn a_world_is_verified_in_game_only_with_its_key_its_token_and_an_answer_at_its_endpoint() {
    let f = hub();
    let ann = sign_in(&f, "531");
    let bob = sign_in(&f, "532");
    let (ka, ta) = register(&f, &ann, "aa", "127.0.0.1");
    let (kb, tb) = register(&f, &bob, "bb", "127.0.0.1");
    assert!(
        call(f.addr(), "GET", "/v1/servers/aa", &[], "").status == 404,
        "a pending world is in no public answer"
    );
    assert_eq!(
        f.running
            .registry()
            .heartbeat(
                "aa",
                &serde_json::from_value(json!({"state": "online"})).expect("a beat"),
                None
            )
            .map_err(|e| e.status),
        Err(409)
    );

    assert_eq!(
        game_verify(&f, "aa", &kb, &ta, &[]).status,
        401,
        "another world's key"
    );
    assert_eq!(
        game_verify(&f, "aa", &format!("drth_{}", "x".repeat(43)), &ta, &[]).status,
        401
    );
    let a = game_verify(&f, "aa", &ka, &tb, &[]);
    assert_eq!(
        (a.status, a.reason()),
        (422, "wrong_token".into()),
        "another world's token"
    );
    let silent = Answering {
        silent: true,
        ..Answering::default()
    };
    let (a, _) = verify_answering(&f, "aa", &ka, &ta, &[], &silent);
    assert_eq!(
        (a.status, a.reason()),
        (422, "no_answer".into()),
        "the challenge was not answered"
    );
    assert_eq!(federation(&f, "aa"), "unset");

    let a = game_verify(&f, "bb", &kb, &tb, &[]);
    assert_eq!(a.status, 200, "{a:?}");
    assert_eq!(
        (a.json()["federation"].clone(), a.json()["approval"].clone()),
        (json!("on"), json!("approved"))
    );
    let w = op(&f, &bob, "GET", "/v1/operator/worlds/bb", &Value::Null).json();
    assert_eq!(w["world"]["ownership"]["state"], "managed");
    assert_eq!(w["world"]["operator"]["verification"]["method"], "game");
    let bob_id = op(&f, &bob, "GET", "/v1/operator", &Value::Null).json()["operator"]["id"].clone();
    let audit = w["audit"].as_array().expect("the audit log");
    let verified = audit
        .iter()
        .find(|e| e["kind"] == "verified")
        .expect("a verified event");
    assert_eq!(verified["details"]["by"], "Asheron");
    assert_eq!(verified["actor"], bob_id, "who verified it");
    assert!(verified["at"].as_str().is_some_and(|t| t.ends_with('Z')));
    for e in audit {
        let mut keys: Vec<&str> = e
            .as_object()
            .expect("an event")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["actor", "at", "details", "kind"], "{e}");
    }
    assert!(
        audit
            .iter()
            .any(|e| e["kind"] == "federation" && e["actor"] == "hub"),
        "approval is the hub's"
    );
    let again = game_verify(&f, "bb", &kb, &tb, &[]);
    assert_eq!(
        again.reason(),
        "no_verification",
        "a token is spent by its use"
    );

    // Another operator can see neither world's operator view; a pending one is not found.
    assert_eq!(
        op(&f, &bob, "GET", "/v1/operator/worlds/aa", &Value::Null).status,
        404
    );
    assert_eq!(
        op(&f, &ann, "GET", "/v1/operator/worlds/bb", &Value::Null).status,
        403
    );
    assert_eq!(
        op(&f, &ann, "POST", "/v1/operator/worlds/bb/key", &Value::Null).status,
        403
    );

    // A token lapses after seven days.
    let fresh = op(
        &f,
        &ann,
        "PUT",
        "/v1/operator/worlds/aa",
        &world("aa", "127.0.0.1"),
    );
    assert_eq!(fresh.status, 200, "{fresh:?}");
    f.advance(Duration::from_secs(7 * 86_400 + 1));
    assert_eq!(game_verify(&f, "aa", &ka, &ta, &[]).reason(), "expired");
    let t2 = op(
        &f,
        &ann,
        "POST",
        "/v1/operator/worlds/aa/verification",
        &Value::Null,
    )
    .json()["verification"]["token"]
        .as_str()
        .expect("a token")
        .to_owned();
    assert_eq!(
        game_verify(&f, "aa", &ka, &ta, &[]).reason(),
        "wrong_token",
        "the old token stopped"
    );
    assert_eq!(game_verify(&f, "aa", &ka, &t2, &[]).status, 200);
    f.running.shutdown();
}

/// Spec: hub: a world verified in game may call the hub from any address, as a world behind two
/// connections to the internet does: what counts is that it answers at its endpoint.
#[test]
fn a_world_calling_from_another_address_than_its_endpoint_is_verified() {
    let f = hub_with(|c| c.trusted_proxies = vec!["127.0.0.1".parse().expect("a proxy")]);
    let ann = sign_in(&f, "541");
    let (key, token) = register(&f, &ann, "twowan", "127.0.0.1");
    let elsewhere = Answering {
        headers: vec![("X-Forwarded-For".into(), "203.0.113.9".into())],
        ..Answering::default()
    };
    let (a, heard) = verify_answering(
        &f,
        "twowan",
        &key,
        &token,
        &[("X-Forwarded-For", "198.51.100.20")],
        &elsewhere,
    );
    assert_eq!(a.status, 200, "{a:?}");
    assert_eq!(heard.answers, [200], "{heard:?}");
    let w = op(&f, &ann, "GET", "/v1/operator/worlds/twowan", &Value::Null).json();
    let verified = w["audit"]
        .as_array()
        .expect("the audit log")
        .iter()
        .find(|e| e["kind"] == "verified")
        .cloned()
        .expect("a verified event");
    assert_eq!(
        (verified["details"]["method"].clone(), verified["details"]["address"].clone()),
        (json!("game"), json!("127.0.0.1")),
        "verified at the address it answered at, not either it called from"
    );
    f.running.shutdown();
}

/// The verification page server: `/token` holds the token (set later), `/away` redirects to
/// another host, `/private` redirects into a private range by another spelling of the host,
/// `/loop` redirects to itself, `/slow` never answers, `/huge` puts the token past what is read.
pub(super) fn page_server(token: Arc<Mutex<String>>) -> Stub {
    Stub::start(Arc::new(move |got: &Got, s: &mut TcpStream| {
        let t = token.lock().expect("the token").clone();
        match got.path.as_str() {
            "/token" => answer(
                s,
                200,
                "Content-Type: text/plain\r\n",
                &format!("dereth network: {t}\n"),
            ),
            "/away" => answer(s, 302, "Location: http://example.invalid/token\r\n", ""),
            "/hop" => answer(s, 301, "Location: /token\r\n", ""),
            "/private" => {
                let port = got
                    .header("host")
                    .and_then(|h| h.rsplit_once(':'))
                    .map_or("80", |(_, p)| p)
                    .to_owned();
                answer(
                    s,
                    302,
                    &format!("Location: http://[::ffff:127.0.0.1]:{port}/token\r\n"),
                    "",
                );
            }
            "/loop" => answer(s, 302, "Location: /loop\r\n", ""),
            "/slow" => std::thread::sleep(Duration::from_secs(10)),
            "/huge" => {
                let filler = "x".repeat(70 * 1024);
                answer(s, 200, "", &format!("{filler}\n{t}\n"));
            }
            "/gone" => answer(s, 404, "", "no"),
            _ => answer(s, 404, "", ""),
        }
    }))
}

pub(super) fn verify_page(f: &Fixture, session: &str, slug: &str, url: &str) -> Answer {
    op(
        f,
        session,
        "POST",
        &format!("/v1/operator/worlds/{slug}/verify"),
        &json!({ "url": url }),
    )
}

/// Spec: hub: a page on the world's own host verifies it; the hub reads only the world's host,
/// follows redirects only within the rules, reads at most 64 KiB and gives up in time.
#[test]
fn a_page_on_the_worlds_host_verifies_it_and_nothing_else_does() {
    let token = Arc::new(Mutex::new(String::new()));
    let pages = page_server(Arc::clone(&token));
    let f = hub();
    let ann = sign_in(&f, "551");
    let (_, t) = register(&f, &ann, "paged", "127.0.0.1");
    *token.lock().expect("the token") = t;
    let base = format!("http://127.0.0.1:{}", pages.addr.port());
    let other = format!("http://localhost:{}", pages.addr.port());

    let before = pages.hits();
    let a = verify_page(&f, &ann, "paged", &format!("{other}/token"));
    assert_eq!(a.reason(), "host_mismatch", "{a:?}");
    assert_eq!(pages.hits(), before, "a page on another host is never read");
    // However a page check fails once the hub reads, it says only that the page did not show the
    // token: nothing of where a redirect went, what answered, or what could not be reached.
    for path in ["/away", "/private", "/loop", "/huge", "/gone"] {
        let a = verify_page(&f, &ann, "paged", &format!("{base}{path}"));
        assert_eq!(
            (a.status, a.reason()),
            (422, "token_not_found".to_owned()),
            "{path}: {a:?}"
        );
        assert_eq!(
            a.json()["error"]["message"],
            "The page did not show the token",
            "{path}"
        );
    }
    let started = Instant::now();
    let a = verify_page(&f, &ann, "paged", &format!("{base}/slow"));
    assert_eq!(a.reason(), "token_not_found");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the hub gave up in time"
    );
    assert_eq!(federation(&f, "paged"), "unset");

    let a = verify_page(&f, &ann, "paged", &format!("{base}/hop"));
    assert_eq!(a.status, 200, "a redirect on the world's own host: {a:?}");
    assert_eq!(a.json()["approval"], "approved");
    assert_eq!(
        a.json()["world"]["operator"]["verification"]["method"],
        "page"
    );
    assert_eq!(federation(&f, "paged"), "on");
    let audit =
        op(&f, &ann, "GET", "/v1/operator/worlds/paged", &Value::Null).json()["audit"].clone();
    let failed: Vec<&str> = audit
        .as_array()
        .expect("events")
        .iter()
        .filter(|e| e["kind"] == "verification-failed")
        .filter_map(|e| e["details"]["reason"].as_str())
        .collect();
    assert!(
        failed
            .iter()
            .all(|r| ["host_mismatch", "token_not_found"].contains(r)),
        "the log says no more than the answers: {failed:?}"
    );
    assert!(
        !audit
            .to_string()
            .contains(&*token.lock().expect("the token")),
        "no token in the log"
    );
    f.running.shutdown();
}

/// Spec: hub: a public hub reads a page only from a public address, never loopback or a private
/// range, before it connects.
#[test]
fn a_public_hub_never_reads_a_page_from_a_private_address() {
    let token = Arc::new(Mutex::new(String::new()));
    let pages = page_server(Arc::clone(&token));
    let f = hub_with(|c| c.loopback_allowed = false);
    let ann = sign_in(&f, "561");
    assert_eq!(
        op(
            &f,
            &ann,
            "POST",
            "/v1/operator/worlds",
            &world("lan", "10.0.0.5")
        )
        .status,
        400,
        "an endpoint in a private range"
    );
    // The hub's operator registers a world at a loopback address and hands it to Ann.
    let r = Registration::from_json(&world("lan", "127.0.0.1")).expect("a registration");
    f.running.registry().register(&r).expect("registered");
    f.running.registry().assign("lan", "561").expect("assigned");
    let t = op(
        &f,
        &ann,
        "POST",
        "/v1/operator/worlds/lan/verification",
        &Value::Null,
    )
    .json()["verification"]["token"]
        .as_str()
        .expect("a token")
        .to_owned();
    *token.lock().expect("the token") = t;
    let before = pages.hits();
    let a = verify_page(
        &f,
        &ann,
        "lan",
        &format!("http://127.0.0.1:{}/token", pages.addr.port()),
    );
    assert_eq!(a.reason(), "token_not_found", "{a:?}");
    assert_eq!(
        pages.hits(),
        before,
        "nothing connected to the private address"
    );
    f.running.shutdown();
}

/// Spec: hub: a claim of a community listing is verified into the claimant's managed world, with
/// the claim's key, and closes every other claim on it.
#[test]
fn a_verified_claim_makes_the_listing_the_claimants_and_closes_the_others() {
    let f = hub();
    let entry = dereth_network_hub::registry::CommunityEntry {
        external_id: "00000000-0000-4000-8000-000000000042".into(),
        registration: Registration::from_json(&world("listed", "127.0.0.1"))
            .expect("a registration"),
        source_hash: "h".into(),
        legacy_type: "PvE".into(),
    };
    f.running
        .registry()
        .import_community(&[entry], true)
        .expect("imported");
    let ann = sign_in(&f, "571");
    let bob = sign_in(&f, "572");
    let ca = op(
        &f,
        &ann,
        "POST",
        "/v1/operator/worlds/listed/claim",
        &Value::Null,
    )
    .json();
    let cb = op(
        &f,
        &bob,
        "POST",
        "/v1/operator/worlds/listed/claim",
        &Value::Null,
    )
    .json();
    let (ka, ta) = (
        ca["key"].as_str().expect("a key").to_owned(),
        ca["verification"]["token"]
            .as_str()
            .expect("a token")
            .to_owned(),
    );
    let kb = cb["key"].as_str().expect("a key").to_owned();
    assert_eq!(
        call(f.addr(), "GET", "/v1/servers/listed", &[], "").status,
        200,
        "still public"
    );
    assert_eq!(
        game_verify(&f, "listed", &kb, &ta, &[]).reason(),
        "wrong_token",
        "Bob's key, Ann's token"
    );
    assert_eq!(game_verify(&f, "listed", &ka, &ta, &[]).status, 200);
    let w = op(&f, &ann, "GET", "/v1/operator/worlds/listed", &Value::Null).json();
    assert_eq!(
        (
            w["world"]["ownership"]["state"].clone(),
            w["world"]["operator"]["role"].clone()
        ),
        (json!("managed"), json!("owner"))
    );
    assert!(
        f.running.registry().authenticate("listed", &ka),
        "the claim's key is the world's"
    );
    assert!(!f.running.registry().authenticate("listed", &kb));
    assert_eq!(
        op(&f, &bob, "GET", "/v1/operator/worlds/listed", &Value::Null).status,
        403,
        "Bob's claim closed"
    );
    f.running.shutdown();
}

/// Spec: hub: a world's owner reads an earlier claimant's events as `earlier-claimant`, never by
/// its id; the hub's own command line still names it.
#[test]
fn an_owner_reads_an_earlier_claimants_events_without_its_id() {
    let f = hub();
    let entry = dereth_network_hub::registry::CommunityEntry {
        external_id: "00000000-0000-4000-8000-000000000043".into(),
        registration: Registration::from_json(&world("contested", "127.0.0.1"))
            .expect("a registration"),
        source_hash: "h".into(),
        legacy_type: "PvE".into(),
    };
    f.running
        .registry()
        .import_community(&[entry], true)
        .expect("imported");
    let ann = sign_in(&f, "581");
    let bob = sign_in(&f, "582");
    let id_of = |session: &str| {
        op(&f, session, "GET", "/v1/operator", &Value::Null).json()["operator"]["id"]
            .as_str()
            .expect("an id")
            .to_owned()
    };
    let (ann_id, bob_id) = (id_of(&ann), id_of(&bob));
    let claim = |session: &str| {
        op(
            &f,
            session,
            "POST",
            "/v1/operator/worlds/contested/claim",
            &Value::Null,
        )
        .json()
    };
    let kb = claim(&bob)["key"].as_str().expect("a key").to_owned();
    assert_eq!(
        game_verify(&f, "contested", &kb, "not the token", &[]).reason(),
        "wrong_token",
        "Bob's attempt"
    );
    let ca = claim(&ann);
    let (ka, ta) = (
        ca["key"].as_str().expect("a key").to_owned(),
        ca["verification"]["token"]
            .as_str()
            .expect("a token")
            .to_owned(),
    );
    assert_eq!(game_verify(&f, "contested", &ka, &ta, &[]).status, 200);
    let audit = op(
        &f,
        &ann,
        "GET",
        "/v1/operator/worlds/contested",
        &Value::Null,
    )
    .json()["audit"]
        .clone();
    let by = |kind: &str| -> Vec<String> {
        audit
            .as_array()
            .expect("the audit log")
            .iter()
            .filter(|e| e["kind"] == kind)
            .map(|e| e["actor"].as_str().expect("an actor").to_owned())
            .collect()
    };
    assert_eq!(by("claim-opened"), [ann_id.as_str(), "earlier-claimant"]);
    assert_eq!(
        by("verification-attempted"),
        [ann_id.as_str(), "earlier-claimant"]
    );
    assert_eq!(by("verified"), [ann_id.as_str()]);
    assert!(
        !audit.to_string().contains(&bob_id),
        "Bob's id is in no event Ann reads: {audit}"
    );
    let cli = f.running.registry().audit("contested").expect("the audit");
    assert!(
        cli.to_string().contains(&bob_id),
        "the hub's operator reads it"
    );
    f.running.shutdown();
}

// ---- keys, approval, reports --------------------------------------------------------------------

/// Spec: hub: a new key from the site stops the old one at once and closes the link opened with
/// it.
#[test]
fn a_key_rotated_from_the_site_stops_the_old_one_and_closes_its_link() {
    let f = hub();
    let ann = sign_in(&f, "581");
    let (key, token) = register(&f, &ann, "turned", "127.0.0.1");
    assert_eq!(game_verify(&f, "turned", &key, &token, &[]).status, 200);
    let base = format!("ws://{}", f.addr());
    let mut link = Link::open(&base, "turned", &key).expect("the world links");
    link.greet().expect("welcomed");
    let fresh = op(
        &f,
        &ann,
        "POST",
        "/v1/operator/worlds/turned/key",
        &Value::Null,
    );
    assert_eq!(fresh.status, 200, "{fresh:?}");
    let new = fresh.json()["key"].as_str().expect("a key").to_owned();
    assert!(
        link.closes_within(Duration::from_secs(5)),
        "the old key's link closed"
    );
    let beat = r#"{"state":"online","playersOnline":1}"#;
    let old = call(
        f.addr(),
        "POST",
        "/v1/servers/turned/heartbeat",
        &[
            ("Authorization", &format!("Bearer {key}")),
            ("Content-Type", "application/json"),
        ],
        beat,
    );
    assert_eq!(old.status, 401, "the old key stopped");
    let now = call(
        f.addr(),
        "POST",
        "/v1/servers/turned/heartbeat",
        &[
            ("Authorization", &format!("Bearer {new}")),
            ("Content-Type", "application/json"),
        ],
        beat,
    );
    assert_eq!(now.status, 200);
    let mut again = Link::open(&base, "turned", &new).expect("the new key links");
    again.greet().expect("welcomed");
    let audit = op(&f, &ann, "GET", "/v1/operator/worlds/turned", &Value::Null).body;
    assert!(
        !audit.contains(&new[5..]) && !audit.contains(&key[5..]),
        "no key in the answer or the log"
    );
    f.running.shutdown();
}

/// Spec: hub: approval onto the network is held to its allowances: a day's for one operator and
/// the hub's for all; past them the world is verified and stays off until asked again.
#[test]
fn approval_onto_the_network_is_held_to_the_daily_allowances() {
    let f = hub_with(|c| c.approvals_per_day = 4);
    let ann = sign_in(&f, "591");
    let bob = sign_in(&f, "592");
    let verify = |session: &str, slug: &str| {
        let (k, t) = register(&f, session, slug, "127.0.0.1");
        game_verify(&f, slug, &k, &t, &[]).json()["approval"]
            .as_str()
            .unwrap_or("")
            .to_owned()
    };
    assert_eq!(verify(&ann, "a1"), "approved");
    assert_eq!(verify(&ann, "a2"), "approved");
    assert_eq!(verify(&ann, "a3"), "approved");
    f.advance(Duration::from_secs(86_400 + 1));
    assert_eq!(verify(&ann, "a4"), "approved", "a new day");
    assert_eq!(verify(&bob, "b1"), "approved");
    assert_eq!(verify(&bob, "b2"), "approved");
    assert_eq!(verify(&bob, "b3"), "approved");
    assert_eq!(
        verify(&ann, "a5"),
        "rate_limited",
        "the hub's four a day are spent"
    );
    assert_eq!(federation(&f, "a5"), "unset");
    let ask = op(
        &f,
        &ann,
        "PUT",
        "/v1/operator/worlds/a5/federation",
        &json!({ "flag": "on" }),
    );
    assert_eq!((ask.status, ask.reason()), (429, "rate_limited".to_owned()));
    let wait: u64 = ask
        .header("retry-after")
        .and_then(|s| s.parse().ok())
        .expect("Retry-After");
    assert!(
        wait > 86_000 && wait <= 86_401,
        "until the day's first approval leaves: {wait}"
    );
    f.advance(Duration::from_secs(wait));
    let ask = op(
        &f,
        &ann,
        "PUT",
        "/v1/operator/worlds/a5/federation",
        &json!({ "flag": "on" }),
    );
    assert_eq!(
        (ask.status, ask.json()["federation"].clone()),
        (200, json!("on")),
        "{ask:?}"
    );
    let off = op(
        &f,
        &ann,
        "PUT",
        "/v1/operator/worlds/a5/federation",
        &json!({ "flag": "off" }),
    );
    assert_eq!(off.json()["federation"], "off");

    // A refusal says why, in the approval's words.
    let (_, _) = register(&f, &bob, "never-verified", "127.0.0.1");
    let refused = op(
        &f,
        &bob,
        "PUT",
        "/v1/operator/worlds/never-verified/federation",
        &json!({ "flag": "on" }),
    );
    assert_eq!(
        (refused.status, refused.reason()),
        (409, "not_verified".to_owned())
    );
    f.advance(Duration::from_secs(86_401));
    let (k, t) = register(&f, &bob, "a-slug-too-long-for-the-network", "127.0.0.1");
    let v = game_verify(&f, "a-slug-too-long-for-the-network", &k, &t, &[]);
    assert_eq!(v.json()["approval"], "not_network_slug");
    let refused = op(
        &f,
        &bob,
        "PUT",
        "/v1/operator/worlds/a-slug-too-long-for-the-network/federation",
        &json!({ "flag": "on" }),
    );
    assert_eq!(
        (refused.status, refused.reason()),
        (409, "not_network_slug".to_owned())
    );
    f.running.shutdown();
}

/// Spec: hub: registrations, verification attempts and reports are held to their limits, counted
/// from the log, each refused 429 with when to try again.
#[test]
fn registrations_attempts_and_reports_are_rate_limited() {
    let f = hub();
    let ann = sign_in(&f, "601");
    for slug in ["r1", "r2", "r3"] {
        register(&f, &ann, slug, "127.0.0.1");
    }
    let fourth = op(
        &f,
        &ann,
        "POST",
        "/v1/operator/worlds",
        &world("r4", "127.0.0.1"),
    );
    assert_eq!(fourth.status, 429, "{fourth:?}");
    let wait: u64 = fourth
        .header("retry-after")
        .and_then(|s| s.parse().ok())
        .expect("Retry-After");
    assert!(wait > 86_000 && wait <= 86_401, "{wait}");
    assert_eq!(fourth.json()["error"]["retryAfterSeconds"], json!(wait));
    // Withdrawing a world gives no registration back.
    assert_eq!(
        op(&f, &ann, "DELETE", "/v1/operator/worlds/r1", &Value::Null).status,
        204
    );
    assert_eq!(
        op(
            &f,
            &ann,
            "POST",
            "/v1/operator/worlds",
            &world("r5", "127.0.0.1")
        )
        .status,
        429
    );

    let key = f.running.registry().rotate_key("r2").expect("a key");
    for i in 0..10 {
        let a = game_verify(&f, "r2", &key, &format!("dnv_{}", "q".repeat(32)), &[]);
        assert_eq!(a.status, 422, "attempt {i}");
    }
    let a = game_verify(&f, "r2", &key, &format!("dnv_{}", "q".repeat(32)), &[]);
    assert_eq!(a.status, 429, "the eleventh attempt this hour");
    for _ in 0..20 {
        let bad = game_verify(&f, "r3", &format!("drth_{}", "z".repeat(43)), "dnv_x", &[]);
        assert_eq!(
            bad.status, 401,
            "a wrong key spends none of the world's attempts"
        );
    }

    let bob = sign_in(&f, "602");
    let reg = Registration::from_json(&world("target", "127.0.0.1")).expect("a registration");
    for slug in ["t1", "t2", "t3", "t4", "t5", "t6"] {
        let mut r = reg.clone();
        r.slug = slug.into();
        f.running.registry().register(&r).expect("registered");
    }
    for slug in ["t1", "t2", "t3", "t4", "t5"] {
        let a = op(
            &f,
            &bob,
            "POST",
            "/v1/reports",
            &json!({ "slug": slug, "reason": "spam", "details": "x" }),
        );
        assert_eq!(a.status, 201, "{a:?}");
    }
    let a = op(
        &f,
        &bob,
        "POST",
        "/v1/reports",
        &json!({ "slug": "t6", "reason": "spam" }),
    );
    assert_eq!(a.status, 429);
    let a = op(
        &f,
        &bob,
        "POST",
        "/v1/reports",
        &json!({ "slug": "t1", "reason": "spam" }),
    );
    assert_eq!(a.status, 409, "one open report of a world");
    f.running.shutdown();
}

/// Spec: hub: revoking a reported world turns federation off, revokes its key and locks it; only
/// the hub's operator lifts the lock. The reporter is shown to admins alone.
#[test]
fn revoking_a_reported_world_turns_it_off_and_locks_it_until_the_hubs_operator_unlocks_it() {
    let f = hub();
    let ann = sign_in(&f, "611");
    let (key, token) = register(&f, &ann, "rogue", "127.0.0.1");
    assert_eq!(game_verify(&f, "rogue", &key, &token, &[]).status, 200);
    // A Discord id no time or id in the log can hold by chance.
    let bob = sign_in(&f, "612049937701236644");
    let filed = op(
        &f,
        &bob,
        "POST",
        "/v1/reports",
        &json!({ "slug": "rogue", "reason": "impersonation", "details": "Not the real Rogue." }),
    );
    assert_eq!(filed.status, 201);
    let id = filed.json()["report"]["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let bob_id = op(&f, &bob, "GET", "/v1/operator", &Value::Null).json()["operator"]["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let seen = op(&f, &ann, "GET", "/v1/operator/worlds/rogue", &Value::Null).body;
    assert!(
        seen.contains("reported"),
        "the report is in the world's log"
    );
    assert!(
        !seen.contains(&bob_id) && !seen.contains("612049937701236644"),
        "the reported world's operator is not told who reported it"
    );
    assert_eq!(
        op(&f, &bob, "GET", "/v1/admin/reports", &Value::Null).status,
        403,
        "not an admin"
    );
    let admin = sign_in(&f, ADMIN);
    let list = op(&f, &admin, "GET", "/v1/admin/reports", &Value::Null).json();
    assert_eq!(
        list["reports"][0]["reporter"]["discordId"],
        "612049937701236644"
    );
    let done = op(
        &f,
        &admin,
        "POST",
        &format!("/v1/admin/reports/{id}"),
        &json!({ "action": "revoke", "note": "Impersonation." }),
    );
    assert_eq!(done.status, 200, "{done:?}");
    assert_eq!(federation(&f, "rogue"), "off");
    assert!(
        !f.running.registry().authenticate("rogue", &key),
        "its key is revoked"
    );
    let w = op(&f, &ann, "GET", "/v1/operator/worlds/rogue", &Value::Null).json();
    assert_eq!(w["world"]["operator"]["locked"]["reason"], "Impersonation.");
    let refused = op(
        &f,
        &ann,
        "PUT",
        "/v1/operator/worlds/rogue/federation",
        &json!({ "flag": "on" }),
    );
    assert_eq!(
        (refused.status, refused.reason()),
        (409, "locked".to_owned())
    );
    assert_eq!(
        op(
            &f,
            &ann,
            "POST",
            "/v1/operator/worlds/rogue/key",
            &Value::Null
        )
        .status,
        409
    );
    // The reporter sees how it ended; the admin sees the note, the reporter and the world.
    let mine =
        op(&f, &bob, "GET", "/v1/operator/reports", &Value::Null).json()["reports"][0].clone();
    assert_eq!(
        (mine["state"].clone(), mine["resolution"].clone()),
        (json!("closed"), json!("revoked"))
    );
    assert_eq!(
        mine["world"],
        json!({ "slug": "rogue", "name": "World rogue" })
    );
    assert!(
        mine.get("note").is_none() && mine.get("reporter").is_none(),
        "{mine}"
    );
    let all = op(
        &f,
        &admin,
        "GET",
        "/v1/admin/reports?state=all",
        &Value::Null,
    )
    .json()["reports"][0]
        .clone();
    assert_eq!(all["note"], "Impersonation.");
    assert_eq!(all["reporter"]["name"], "Operator 612049937701236644");
    assert_eq!(all["world"]["slug"], "rogue");
    assert!(
        op(&f, &admin, "GET", "/v1/admin/reports", &Value::Null).json()["reports"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "no report is open"
    );
    f.running.registry().unlock("rogue").expect("unlocked");
    assert_eq!(
        op(
            &f,
            &ann,
            "POST",
            "/v1/operator/worlds/rogue/key",
            &Value::Null
        )
        .status,
        200
    );
    assert_eq!(
        op(
            &f,
            &ann,
            "PUT",
            "/v1/operator/worlds/rogue/federation",
            &json!({ "flag": "on" })
        )
        .status,
        200
    );
    f.running.shutdown();
}

/// Spec: hub: a request over its allowance is told when to try again, in `Retry-After` (which a
/// page on another origin may read) and in the body, on a public route and an operator route.
#[test]
fn a_request_over_its_allowance_is_told_when_to_try_again() {
    let f = hub_with(|c| c.http_per_minute = 2);
    for _ in 0..2 {
        assert_eq!(call(f.addr(), "GET", "/v1/status", &[], "").status, 200);
    }
    let public = call(
        f.addr(),
        "GET",
        "/v1/status",
        &[("Origin", "https://anyone.test")],
        "",
    );
    assert_eq!(public.status, 429);
    let wait: u64 = public
        .header("retry-after")
        .and_then(|s| s.parse().ok())
        .expect("Retry-After");
    assert!((1..=30).contains(&wait), "{wait}");
    assert_eq!(public.json()["error"]["retryAfterSeconds"], json!(wait));
    assert_eq!(public.header("access-control-allow-origin"), Some("*"));
    assert_eq!(
        public.header("access-control-expose-headers"),
        Some("Retry-After")
    );
    let site = call(f.addr(), "GET", "/v1/operator", &[("Origin", SITE)], "");
    assert_eq!(site.status, 429);
    assert_eq!(
        site.header("access-control-allow-origin"),
        Some(SITE),
        "the site may read it"
    );
    assert_eq!(
        site.header("access-control-expose-headers"),
        Some("Retry-After")
    );
    assert!(site.json()["error"]["retryAfterSeconds"].as_u64().is_some());
    f.running.shutdown();
}

/// Spec: hub: the public list can be held to the worlds on the network, and a world keeps its
/// endpoint's address family through an edit.
#[test]
fn the_list_filters_by_federation_and_an_endpoint_keeps_its_address_family() {
    let f = hub();
    let ann = sign_in(&f, "621");
    let (k, t) = register(&f, &ann, "linked", "127.0.0.1");
    assert_eq!(game_verify(&f, "linked", &k, &t, &[]).status, 200);
    let r = Registration::from_json(&world("listed-only", "8.8.8.8")).expect("a registration");
    f.running.registry().register(&r).expect("registered");
    let slugs = |q: &str| -> Vec<String> {
        call(f.addr(), "GET", &format!("/v1/servers{q}"), &[], "").json()["data"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|w| w["slug"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    assert_eq!(slugs("?federation=on"), ["linked"]);
    assert_eq!(slugs("?federation=unset"), ["listed-only"]);
    assert_eq!(slugs("").len(), 2);

    let mut v6 = world("six", "127.0.0.1");
    v6["endpoint"] = json!({ "mode": "observed", "port": 9000, "addressFamily": "ipv6" });
    let a = op(&f, &ann, "POST", "/v1/operator/worlds", &v6);
    assert_eq!(a.status, 201, "{a:?}");
    assert_eq!(a.json()["world"]["endpoint"]["addressFamily"], "ipv6");
    let shown =
        op(&f, &ann, "GET", "/v1/operator/worlds/six", &Value::Null).json()["world"].clone();
    assert_eq!(shown["endpoint"]["addressFamily"], "ipv6");
    // The site edits the world from what it was shown.
    let mut edit = v6.clone();
    edit["description"] = json!("Now with a description.");
    edit["endpoint"] = json!({
        "mode": shown["endpoint"]["mode"], "port": shown["endpoint"]["port"],
        "addressFamily": shown["endpoint"]["addressFamily"],
    });
    let a = op(&f, &ann, "PUT", "/v1/operator/worlds/six", &edit);
    assert_eq!(a.status, 200, "{a:?}");
    assert_eq!(a.json()["world"]["endpoint"]["addressFamily"], "ipv6");
    f.running.shutdown();
}

/// Spec: hub: a hub without sign-in configured answers the operator routes 503 and says so in its
/// configuration.
#[test]
fn a_hub_without_sign_in_answers_the_operator_routes_unavailable() {
    let f = hub_with(|c| c.discord = None);
    assert_eq!(call(f.addr(), "GET", "/v1/operator", &[], "").status, 503);
    assert_eq!(
        call(f.addr(), "GET", "/v1/auth/discord", &[], "").status,
        503
    );
    let config = call(f.addr(), "GET", "/v1/config", &[], "").json();
    assert_eq!(config["authEnabled"], false);
    f.running.shutdown();
    let f = hub();
    assert_eq!(
        call(f.addr(), "GET", "/v1/config", &[], "").json()["authEnabled"],
        true
    );
    f.running.shutdown();
}

/// Spec: hub: behind a trusted proxy each request takes from its client's allowance, not the
/// proxy's; without one, a forwarded header changes nothing.
#[test]
fn the_request_allowance_counts_the_client_behind_a_trusted_proxy() {
    let f = hub_with(|c| {
        c.http_per_minute = 5;
        c.trusted_proxies = vec!["127.0.0.0/8".parse().expect("a range")];
    });
    let get = |xff: &str| {
        call(
            f.addr(),
            "GET",
            "/v1/status",
            &[("X-Forwarded-For", xff)],
            "",
        )
        .status
    };
    for _ in 0..5 {
        assert_eq!(get("198.51.100.1"), 200);
    }
    assert_eq!(get("198.51.100.1"), 429);
    assert_eq!(
        get("198.51.100.2"),
        200,
        "another client behind the same proxy"
    );
    f.running.shutdown();
    let f = hub_with(|c| c.http_per_minute = 5);
    let get = |xff: &str| {
        call(
            f.addr(),
            "GET",
            "/v1/status",
            &[("X-Forwarded-For", xff)],
            "",
        )
        .status
    };
    for i in 0..5 {
        assert_eq!(get(&format!("198.51.100.{i}")), 200);
    }
    assert_eq!(get("198.51.100.9"), 429, "untrusted: the header is ignored");
    f.running.shutdown();
}

/// Spec: hub: an admin registers and moves a world to a name under the hub's own domain, as the
/// hub's operator does; any other operator is refused one; and nobody registers the names the hub
/// or its site answer on. A world's public links are the configured site's and hub's, and the hub
/// answers its status link.
#[test]
fn only_an_admin_registers_under_the_hubs_domain_and_links_follow_the_configuration() {
    let f = hub_with(|c| {
        c.lookup = dereth_network_hub::resolver::Lookup(Arc::new(|_| {
            Ok(vec!["8.8.8.8".parse().expect("an address")])
        }));
    });
    let admin = sign_in(&f, ADMIN);
    let ann = sign_in(&f, "621");
    let a = op(
        &f,
        &admin,
        "POST",
        "/v1/operator/worlds",
        &world("test-one", "test1.dereth.network"),
    );
    assert_eq!(a.status, 201, "{a:?}");
    let a = op(
        &f,
        &admin,
        "PUT",
        "/v1/operator/worlds/test-one",
        &world("test-one", "test2.dereth.network"),
    );
    assert_eq!(a.status, 200, "{a:?}");
    assert_eq!(
        a.json()["world"]["endpoint"]["address"],
        "test2.dereth.network"
    );
    let a = op(
        &f,
        &ann,
        "POST",
        "/v1/operator/worlds",
        &world("theirs", "test3.dereth.network"),
    );
    assert_eq!(a.status, 400, "{a:?}");
    register(&f, &ann, "theirs", "8.8.4.4");
    let a = op(
        &f,
        &ann,
        "PUT",
        "/v1/operator/worlds/theirs",
        &world("theirs", "test3.dereth.network"),
    );
    assert_eq!(a.status, 400, "{a:?}");
    for (session, name) in [
        (&admin, "hub.dereth.network"),
        (&admin, "api.dereth.network"),
        (&admin, "dereth.network"),
        (&admin, "hub.test"),
        (&admin, "site.test"),
        (&ann, "site.test"),
    ] {
        let a = op(
            &f,
            session,
            "POST",
            "/v1/operator/worlds",
            &world("reserved", name),
        );
        assert_eq!(a.status, 400, "{name}: {a:?}");
    }

    let r = Registration::from_json(&world("listed", "8.8.8.8")).expect("a registration");
    f.running.registry().register(&r).expect("registered");
    let w = call(f.addr(), "GET", "/v1/servers/listed", &[], "").json();
    assert_eq!(w["links"]["web"], "https://site.test/worlds/?q=listed");
    assert_eq!(w["links"]["resolve"], "https://hub.test/v1/resolve/listed");
    let status = w["links"]["status"].as_str().expect("a status link");
    let path = status.strip_prefix("https://hub.test").expect("on the hub");
    let s = call(f.addr(), "GET", path, &[], "");
    assert_eq!(s.status, 200, "{s:?}");
    assert_eq!(s.json()["slug"], "listed");
    let resolve = w["links"]["resolve"].as_str().expect("a resolve link");
    let s = call(
        f.addr(),
        "GET",
        resolve
            .strip_prefix("https://hub.test")
            .expect("on the hub"),
        &[],
        "",
    );
    assert_ne!(s.status, 404, "{s:?}");
    f.running.shutdown();
}
