//! Announcing the relay to the hub, so that the hub's list of worlds names it as the world's
//! WebSocket address and the web client connects to it directly.
//!
//! Every [`EVERY`] the relay posts its public `wss://` address to the hub's
//! `POST /v1/servers/<slug>/web` with the world's registry key; the hub lists it for as long as a
//! heartbeat keeps a world's status fresh, so a relay that stops is delisted within minutes.
//! Without a hub nothing changes: players add the world by the relay's address themselves.
//!
//! The key goes in the request's `Authorization` header and nowhere else: never in a log line,
//! never in an error, never on the command line ([`crate::config`]).

use std::time::Duration;

use crate::log::Log;

/// The Dereth Network's hub.
pub const NETWORK_HUB: &str = "https://hub.dereth.network";

/// How often the relay announces itself: well inside the hub's three minutes.
pub const EVERY: Duration = Duration::from_secs(60);

/// A world's registry key, kept out of every `Debug` and log line.
#[derive(Clone, PartialEq, Eq)]
pub struct Key(String);

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Key(..)")
    }
}

impl Key {
    /// A key of the registry's shape: `drth_` and 43 characters of base64url.
    #[must_use]
    pub fn new(text: &str) -> Option<Self> {
        let rest = text.strip_prefix("drth_")?;
        (rest.len() == 43
            && rest
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
        .then(|| Self(text.to_owned()))
    }

    /// The key itself, for the one header that carries it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// What the relay announces, where, and with which key.
#[derive(Debug, Clone)]
pub struct Announce {
    hub: String,
    world: String,
    public_url: String,
    key: Key,
}

impl Announce {
    /// Checks each part: the hub's address is `https://`, or `http://` to this machine (the key
    /// crosses it); the world is a slug; the public URL is one the hub keeps (`wss://`, or `ws://`
    /// to this machine, at most 200 bytes).
    ///
    /// # Errors
    /// The part that is not, with why.
    pub fn new(hub: &str, world: &str, public_url: &str, key: Key) -> Result<Self, String> {
        let hub = hub.trim().trim_end_matches('/');
        let secure = hub.starts_with("https://");
        let local = hub
            .strip_prefix("http://")
            .and_then(|rest| rest.split(['/', ':']).next())
            .is_some_and(|host| {
                host == "localhost"
                    || host
                        .parse::<std::net::Ipv4Addr>()
                        .is_ok_and(|ip| ip.is_loopback())
            });
        if !secure && !local {
            return Err(format!(
                "--hub {hub}: the hub is reached over https:// (or http:// on this machine), \
                 since the world's key goes with each announcement"
            ));
        }
        if world.is_empty()
            || world.len() > 63
            || !world
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(format!(
                "--world {world}: the world's slug at the hub (lowercase letters, digits and -)"
            ));
        }
        if !dereth_network_protocol::web::is_websocket_url(public_url) {
            return Err(format!(
                "--public-url {public_url}: the address players reach this relay at, a wss:// URL \
                 (or ws:// to this machine) of at most {} bytes",
                dereth_network_protocol::web::MAX_URL_BYTES
            ));
        }
        Ok(Self {
            hub: hub.to_owned(),
            world: world.to_owned(),
            public_url: public_url.to_owned(),
            key,
        })
    }

    #[must_use]
    pub const fn key(&self) -> &Key {
        &self.key
    }

    #[must_use]
    pub fn public_url(&self) -> &str {
        &self.public_url
    }

    /// The route announcements go to.
    #[must_use]
    pub fn route(&self) -> String {
        format!("{}/v1/servers/{}/web", self.hub, self.world)
    }

    /// Announces the public URL once (or, with `withdraw`, takes it back): the hub's status and
    /// its answer, or why the hub could not be reached.
    ///
    /// # Errors
    /// The hub could not be reached.
    pub fn send(&self, withdraw: bool) -> Result<(u16, String), String> {
        let body = if withdraw {
            r#"{"websocketUrl":null}"#.to_owned()
        } else {
            // The URL is printable ASCII without a quote or a backslash (the hub's rule, checked
            // in `new`), so it needs no escaping.
            format!(r#"{{"websocketUrl":"{}"}}"#, self.public_url)
        };
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .user_agent(concat!("dereth-web-relay/", env!("CARGO_PKG_VERSION")))
            .timeout_global(Some(Duration::from_secs(20)))
            .http_status_as_error(false)
            .max_redirects(0)
            .build()
            .into();
        let mut answer = agent
            .post(&self.route())
            .header("Authorization", &format!("Bearer {}", self.key.expose()))
            .header("Content-Type", "application/json")
            .send(body.as_str())
            .map_err(|e| self.scrub(&e.to_string()))?;
        let status = answer.status().as_u16();
        let text = answer
            .body_mut()
            .with_config()
            .limit(16 * 1024)
            .read_to_string()
            .unwrap_or_default();
        Ok((status, self.scrub(&text)))
    }

    /// `text` with the key taken out, should anything ever echo it.
    fn scrub(&self, text: &str) -> String {
        text.replace(self.key.expose(), "<key>")
    }

    /// One announcement, and its log line when it says something new: the first success, any
    /// failure, and the first success after one. Answers whether it succeeded.
    pub fn announce_and_log(&self, log: &Log, was_ok: Option<bool>) -> bool {
        let line = match self.send(false) {
            Ok((200, _)) => {
                if was_ok != Some(true) {
                    log.line(&format!(
                        "announced {} to {} for {}",
                        self.public_url, self.hub, self.world
                    ));
                }
                return true;
            }
            Ok((status, body)) => format!(
                "the hub refused the announcement for {} ({status}): {}",
                self.world,
                hub_message(&body)
            ),
            Err(e) => format!("the hub could not be reached ({}): {e}", self.hub),
        };
        log.line(&line);
        false
    }

    /// Announces now and every [`EVERY`] after, on a thread of its own.
    pub fn spawn(self, log: Log) {
        let spawned = std::thread::Builder::new()
            .name("dereth-web-relay-announce".into())
            .spawn(move || {
                let mut ok = None;
                loop {
                    ok = Some(self.announce_and_log(&log, ok));
                    std::thread::sleep(EVERY);
                }
            });
        if let Err(e) = spawned {
            eprintln!("no thread to announce the relay: {e}");
        }
    }
}

/// The hub's error message from its JSON answer, or the answer itself, cut short.
fn hub_message(body: &str) -> String {
    let message = body
        .split("\"message\":\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or(body);
    message.chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    const KEY: &str = "drth_kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk";

    fn key() -> Key {
        Key::new(KEY).expect("a key")
    }

    /// A hub on loopback that answers each request with `status` and `body`, and hands the test
    /// each request it got.
    fn fake_hub(answers: Vec<(u16, String)>) -> (String, std::sync::mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let addr = listener.local_addr().expect("its address");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for (status, body) in answers {
                let (stream, _) = listener.accept().expect("a request");
                let mut reader = BufReader::new(stream);
                let mut head = String::new();
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).expect("a line");
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().expect("a length");
                    }
                    head.push_str(&line);
                    if line == "\r\n" {
                        break;
                    }
                }
                let mut payload = vec![0; length];
                reader.read_exact(&mut payload).expect("the body");
                head.push_str(&String::from_utf8_lossy(&payload));
                tx.send(head).expect("the test listens");
                let mut stream = reader.into_inner();
                write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .expect("answered");
            }
        });
        (format!("http://{addr}"), rx)
    }

    #[test]
    fn a_key_has_the_registrys_shape() {
        assert!(Key::new(KEY).is_some());
        assert!(Key::new("drth_short").is_none());
        assert!(Key::new(&KEY.replace("drth_", "xxxx_")).is_none());
        assert!(Key::new(&format!("{}!", &KEY[..KEY.len() - 1])).is_none());
        assert_eq!(format!("{:?}", key()), "Key(..)");
    }

    /// The hub is reached over https://, or http:// on this machine only; the world is a slug and
    /// the public URL is one the hub keeps.
    #[test]
    fn an_announcement_names_a_secure_hub_a_slug_and_a_wss_address() {
        let ok = |hub: &str, world: &str, url: &str| Announce::new(hub, world, url, key()).is_ok();
        assert!(ok(NETWORK_HUB, "coldeve", "wss://relay.example.org/"));
        assert!(ok(
            "http://127.0.0.1:8650",
            "coldeve",
            "ws://127.0.0.1:9180/"
        ));
        assert!(!ok(
            "http://hub.example.org",
            "coldeve",
            "wss://relay.example.org/"
        ));
        assert!(!ok(
            "ftp://hub.example.org",
            "coldeve",
            "wss://relay.example.org/"
        ));
        assert!(!ok(NETWORK_HUB, "Cold Eve", "wss://relay.example.org/"));
        assert!(!ok(NETWORK_HUB, "coldeve", "ws://relay.example.org/"));
        assert!(!ok(NETWORK_HUB, "coldeve", "https://relay.example.org/"));
        assert_eq!(
            Announce::new(
                "https://hub.example.org/",
                "coldeve",
                "wss://r.example.org/",
                key()
            )
            .expect("ok")
            .route(),
            "https://hub.example.org/v1/servers/coldeve/web"
        );
    }

    /// An announcement carries the key in its `Authorization` header and the address in its body;
    /// no log line names the key, a refusal's included, even when the hub echoes it.
    #[test]
    fn the_key_goes_to_the_hub_and_never_into_a_log_line() {
        let echo = format!(r#"{{"error":{{"message":"bad key {KEY}"}}}}"#);
        let (hub, requests) = fake_hub(vec![
            (
                200,
                r#"{"websocketUrl":"wss://relay.example.org/"}"#.to_owned(),
            ),
            (401, echo),
            (200, "{}".to_owned()),
        ]);
        let a = Announce::new(&hub, "coldeve", "wss://relay.example.org/", key()).expect("ok");
        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let kept = Arc::clone(&lines);
        let log = Log::to(move |l| kept.lock().expect("lines").push(l.to_owned()));

        assert!(a.announce_and_log(&log, None));
        let request = requests.recv().expect("a request");
        assert!(
            request.starts_with("POST /v1/servers/coldeve/web "),
            "{request}"
        );
        assert!(request.contains(&format!("Bearer {KEY}")), "{request}");
        assert!(
            request.ends_with(r#"{"websocketUrl":"wss://relay.example.org/"}"#),
            "{request}"
        );

        assert!(!a.announce_and_log(&log, Some(true)), "refused");
        assert!(a.announce_and_log(&log, Some(false)), "again");
        let lines = lines.lock().expect("lines").clone();
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert!(
            lines[1].contains("401") && lines[1].contains("<key>"),
            "{lines:?}"
        );
        for l in &lines {
            assert!(!l.contains(KEY) && !l.contains("drth_k"), "{l}");
        }
    }
}
