//! Trying a world from outside: `POST /v1/probe`, which a world's host calls to learn whether
//! players elsewhere can reach the world (`docs/network/05-hub.md` section 1, "Trying a world
//! from outside").
//!
//! The call names a port and nothing else. The hub sends the caller's own address (its client
//! address, as [`crate::forwarded`] reads it) at that port both of the probe's questions
//! ([`dereth_network_protocol::probe`]): Empyrean's status ping and the server-tracker login,
//! [`COPIES`] times, [`EVERY`] apart, since UDP may lose any one. It answers as soon as the world
//! says how it is, and otherwise once [`WAIT`] has passed: `reached`, with what the world said
//! of itself when it said anything, or not.
//!
//! **Abuse.** It cannot be aimed at anyone else: it never takes an address, only a port, and only
//! from 1024 to 65535. A caller whose address the hub would never reach on the internet (private,
//! reserved, or one of the hub's own ranges) is refused. Each caller has [`PER_CALLER`] probes (by
//! its address, an IPv6 one by its /64), the hub [`OVERALL`] between every caller, and at most
//! [`MAX_PROBING`] run at once. The limits run on the registry's clock, so a test sets their time.
//! Nothing is written anywhere: the limits' counters are kept in memory only, and an answer is
//! never logged.

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use dereth_network_protocol::limits::{Bucket, Rate};
use dereth_network_protocol::probe::{self, Status};
use serde_json::{json, Value};

/// The route.
pub const PATH: &str = "/v1/probe";

/// The probes one caller has: five, then one every twelve minutes.
pub const PER_CALLER: Rate = Rate {
    burst: 5.0,
    per_sec: 1.0 / 720.0,
};

/// The probes the hub runs for every caller together: a hundred at once, then one a second.
pub const OVERALL: Rate = Rate {
    burst: 100.0,
    per_sec: 1.0,
};

/// The most probes waiting for an answer at once across the hub.
pub const MAX_PROBING: usize = 32;

/// How many times each question is sent.
pub const COPIES: u32 = 3;

/// The time between two copies.
pub const EVERY: Duration = Duration::from_secs(1);

/// How long a probe waits for an answer, from its first copy.
pub const WAIT: Duration = Duration::from_secs(4);

/// The lowest port the hub tries: below it are the system's own services.
pub const LOWEST_PORT: u16 = 1024;

/// What a world said when it was tried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// Something answered at the address and port.
    pub reached: bool,
    /// What the world said of itself (Empyrean's status ping); none when it said nothing more
    /// than that it is there.
    pub status: Option<Status>,
}

/// The waits of one probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// The time between two copies ([`EVERY`]).
    pub every: Duration,
    /// How long the probe waits, from its first copy ([`WAIT`]).
    pub wait: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            every: EVERY,
            wait: WAIT,
        }
    }
}

/// The socket a probe of `to` sends from: bound to loopback when `to` is a loopback address (a hub
/// tried on one machine), else to the family's unspecified address on a port the system picks, and
/// connected to `to`, so only `to` is heard.
fn socket_for(to: SocketAddr) -> io::Result<UdpSocket> {
    let ip = match (to.is_ipv6(), to.ip().is_loopback()) {
        (false, true) => IpAddr::V4(Ipv4Addr::LOCALHOST),
        (false, false) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        (true, true) => IpAddr::V6(Ipv6Addr::LOCALHOST),
        (true, false) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
    };
    let socket = UdpSocket::bind(SocketAddr::new(ip, 0))?;
    socket.connect(to)?;
    Ok(socket)
}

/// Tries the world at `to`: both questions, [`COPIES`] times, until it says how it is or the wait
/// is over. A world that answers only the server-tracker login is given one more interval to say
/// how it is too (an Empyrean answers both).
#[must_use]
pub fn probe(to: SocketAddr, timing: Timing) -> Answer {
    let mut answer = Answer {
        reached: false,
        status: None,
    };
    let Ok(socket) = socket_for(to) else {
        return answer;
    };
    let started = Instant::now();
    let mut deadline = started + timing.wait;
    let mut sent = 0;
    let mut next_copy = started;
    let mut asked = 0;
    let mut buf = [0u8; probe::MAX_ANSWER_LEN + 1];
    loop {
        let now = Instant::now();
        if now >= deadline {
            return answer;
        }
        if sent < COPIES && now >= next_copy {
            // A copy that cannot be sent is the next copy's to make up.
            let _ = socket.send(&probe::status_hello());
            let _ = socket.send(&probe::TRACKER_LOGIN);
            sent += 1;
            next_copy = now + timing.every;
        }
        let until = if sent < COPIES {
            next_copy.min(deadline)
        } else {
            deadline
        };
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() || socket.set_read_timeout(Some(left)).is_err() {
            continue;
        }
        let n = match socket.recv(&mut buf) {
            Ok(n) => n,
            // Nothing yet, or (on Windows) the system's word that nothing listens there, which the
            // next copy may still find otherwise.
            Err(_) => continue,
        };
        let datagram = &buf[..n];
        if let Some(status) = Status::parse(datagram) {
            answer.reached = true;
            answer.status = Some(status);
            return answer;
        }
        if let Some(token) = probe::status_token(datagram) {
            if asked < COPIES {
                let _ = socket.send(&probe::status_ask(&token));
                asked += 1;
            }
            continue;
        }
        if probe::is_packet(datagram) && !answer.reached {
            answer.reached = true;
            deadline = deadline.min(Instant::now() + timing.every);
        }
    }
}

/// The limits on probes: each caller's, and the hub's own, on a clock in seconds.
#[derive(Debug)]
pub struct Limits {
    per_caller: Rate,
    overall: Rate,
    callers: Mutex<HashMap<IpAddr, Bucket>>,
    hub: Mutex<Option<Bucket>>,
    running: AtomicUsize,
}

/// Why a probe is not run now, and how many seconds until it could be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused {
    pub wait: u64,
}

/// One probe under way, counted until it is dropped.
#[derive(Debug)]
pub struct Underway<'a> {
    limits: &'a Limits,
}

impl Drop for Underway<'_> {
    fn drop(&mut self) {
        self.limits.running.fetch_sub(1, Ordering::SeqCst);
    }
}

/// `secs` rounded up to whole seconds, at least one and at most a day.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn whole_seconds(secs: f64) -> u64 {
    if secs.is_finite() {
        secs.ceil().clamp(1.0, 86_400.0) as u64
    } else {
        86_400
    }
}

/// The address a caller is counted under: an IPv4 address as it is, an IPv6 address by its /64,
/// and an IPv4-mapped one as the IPv4 address.
fn counted_as(ip: IpAddr) -> IpAddr {
    match crate::forwarded::canonical(ip) {
        IpAddr::V6(v6) => {
            let s = v6.segments();
            IpAddr::V6(Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
        }
        v4 => v4,
    }
}

impl Limits {
    /// Limits of `per_caller` for each caller and `overall` for the hub.
    #[must_use]
    pub fn new(per_caller: Rate, overall: Rate) -> Self {
        Self {
            per_caller,
            overall,
            callers: Mutex::new(HashMap::new()),
            hub: Mutex::new(None),
            running: AtomicUsize::new(0),
        }
    }

    /// Takes one probe for `caller` at `now` (seconds on the limits' clock), from its own
    /// allowance and the hub's; neither is taken from when either is spent, or while
    /// [`MAX_PROBING`] probes run.
    ///
    /// # Errors
    /// When a probe cannot run now: how many seconds until one could.
    pub fn take(&self, caller: IpAddr, now: f64) -> Result<Underway<'_>, Refused> {
        let mut callers = self
            .callers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if callers.len() > 10_000 {
            callers.retain(|_, b| !b.full_at(now));
        }
        let mine = callers
            .entry(counted_as(caller))
            .or_insert_with(|| Bucket::new(self.per_caller, now));
        let mut hub = self
            .hub
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let hub = hub.get_or_insert_with(|| Bucket::new(self.overall, now));
        if !mine.has(now) || !hub.has(now) {
            return Err(Refused {
                wait: whole_seconds(mine.wait(now).max(hub.wait(now))),
            });
        }
        if self
            .running
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                (n < MAX_PROBING).then_some(n + 1)
            })
            .is_err()
        {
            return Err(Refused {
                wait: whole_seconds(EVERY.as_secs_f64() * f64::from(COPIES)),
            });
        }
        mine.take(now);
        hub.take(now);
        Ok(Underway { limits: self })
    }
}

/// What the call asked: the port, and nothing more.
#[derive(Debug, serde::Deserialize)]
struct Ask {
    port: u32,
}

/// The world's state, in words.
fn state_word(state: u8) -> &'static str {
    match state {
        1 => "open",
        2 => "starting",
        3 => "shutting-down",
        _ => "unknown",
    }
}

/// The answer's JSON: the address and port tried, whether something answered, and what the world
/// said of itself.
#[must_use]
pub fn answer_json(ip: IpAddr, port: u16, answer: &Answer) -> Value {
    let world = answer.status.as_ref().map(|s| {
        json!({
            "name": s.name, "players": s.players, "software": s.software,
            "version": s.version, "state": state_word(s.state),
        })
    });
    json!({ "address": ip.to_string(), "port": port, "reached": answer.reached, "world": world })
}

/// Reads the call's body: the port, from 1024 to 65535.
///
/// # Errors
/// A body that is not `{"port": <number>}`, or a port outside that range, with what to say.
pub fn port_of(body: &[u8]) -> Result<u16, String> {
    let ask: Ask = serde_json::from_slice(body)
        .map_err(|_| "Send the world's game port: {\"port\": 9000}".to_owned())?;
    u16::try_from(ask.port)
        .ok()
        .filter(|p| *p >= LOWEST_PORT)
        .ok_or_else(|| format!("The port must be from {LOWEST_PORT} to 65535"))
}

/// Whether the hub tries `ip` at all: an address on the internet (loopback too on a hub that
/// allows it, for tests), and none of the ranges the hub never reaches (its own).
#[must_use]
pub fn may_try(ip: IpAddr, loopback_allowed: bool, deny: &[crate::forwarded::ProxyRange]) -> bool {
    let ip = crate::forwarded::canonical(ip);
    crate::operators::Policy { loopback_allowed }.allows(ip)
        && !crate::forwarded::is_trusted(deny, ip)
}

/// `POST /v1/probe` from `ip`, the caller's client address: the port read, the address checked,
/// a probe taken from the limits on the registry's clock, and the world tried.
pub(crate) fn handle(
    shared: &crate::server::Shared,
    method: &str,
    body: &[u8],
    ip: IpAddr,
) -> crate::api::Reply {
    let reply = |status: u16, v: Value| crate::api::Reply {
        status,
        body: v.to_string(),
        kind: "application/json; charset=utf-8",
        headers: Vec::new(),
        cors: crate::api::Cors::Any,
    };
    let error = |status: u16, message: &str| {
        reply(
            status,
            json!({ "error": { "code": crate::api::code(status), "message": message } }),
        )
    };
    if method != "POST" {
        return error(405, "Use POST");
    }
    let port = match port_of(body) {
        Ok(port) => port,
        Err(why) => return error(400, &why),
    };
    let config = &shared.config;
    if !may_try(ip, config.loopback_allowed, &config.deny_fetch) {
        return error(
            403,
            "The hub tries only an address on the internet, and this request did not come from one",
        );
    }
    let now = shared
        .registry()
        .now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64());
    let underway = match shared.probes.take(ip, now) {
        Ok(underway) => underway,
        Err(refused) => return crate::server::too_many(refused.wait, crate::api::Cors::Any),
    };
    let to = SocketAddr::new(crate::forwarded::canonical(ip), port);
    let answer = probe(to, config.probe_timing);
    drop(underway);
    reply(200, answer_json(to.ip(), port, &answer))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().expect("an address")
    }

    /// Spec: hub: a caller has five probes, then one every twelve minutes, counted on the limits'
    /// clock; the refusal says how long until the next.
    #[test]
    fn a_caller_has_five_probes_then_one_every_twelve_minutes() {
        let limits = Limits::new(PER_CALLER, OVERALL);
        let caller = ip("203.0.113.9");
        for _ in 0..5 {
            drop(limits.take(caller, 0.0).expect("one of the five"));
        }
        let refused = limits.take(caller, 0.0).expect_err("the sixth");
        assert_eq!(refused.wait, 720);
        assert!(limits.take(caller, 719.0).is_err(), "not yet");
        drop(limits.take(caller, 720.0).expect("twelve minutes on"));
        assert!(limits.take(caller, 720.0).is_err());
        drop(
            limits
                .take(ip("198.51.100.4"), 720.0)
                .expect("another caller has its own"),
        );
    }

    /// Spec: hub: an IPv6 caller is counted by its /64, and an IPv4-mapped one as its IPv4
    /// address.
    #[test]
    fn an_ipv6_caller_is_counted_by_its_64() {
        let limits = Limits::new(
            Rate {
                burst: 1.0,
                per_sec: 0.0,
            },
            OVERALL,
        );
        drop(limits.take(ip("2001:db8:1:2::1"), 0.0).expect("the first"));
        assert!(limits.take(ip("2001:db8:1:2::ffff"), 0.0).is_err());
        drop(
            limits
                .take(ip("2001:db8:1:3::1"), 0.0)
                .expect("another /64"),
        );
        drop(limits.take(ip("192.0.2.1"), 0.0).expect("an IPv4 caller"));
        assert!(limits.take(ip("::ffff:192.0.2.1"), 0.0).is_err());
    }

    /// Spec: hub: the hub's own allowance is shared by every caller, and a refusal by it takes
    /// nothing from the caller's.
    #[test]
    fn the_hubs_allowance_is_shared_and_a_refusal_takes_nothing() {
        let limits = Limits::new(
            Rate {
                burst: 2.0,
                per_sec: 0.0,
            },
            Rate {
                burst: 2.0,
                per_sec: 0.5,
            },
        );
        drop(limits.take(ip("192.0.2.1"), 0.0).expect("one"));
        drop(limits.take(ip("192.0.2.2"), 0.0).expect("two"));
        let refused = limits
            .take(ip("192.0.2.3"), 0.0)
            .expect_err("the hub's are spent");
        assert_eq!(refused.wait, 2);
        drop(
            limits
                .take(ip("192.0.2.3"), 2.0)
                .expect("the hub's refilled"),
        );
        drop(
            limits
                .take(ip("192.0.2.3"), 4.0)
                .expect("its own second, untouched by the refusal"),
        );
    }

    /// Spec: hub: at most so many probes run at once; one more waits for one of them to end.
    #[test]
    fn no_more_probes_run_at_once_than_the_hub_allows() {
        let limits = Limits::new(
            Rate {
                burst: 1.0,
                per_sec: 0.0,
            },
            Rate {
                burst: 1_000.0,
                per_sec: 0.0,
            },
        );
        let running: Vec<Underway<'_>> = (0u32..)
            .take(MAX_PROBING)
            .map(|i| {
                limits
                    .take(IpAddr::V4(Ipv4Addr::from(0xC000_0200 + i)), 0.0)
                    .expect("under the limit")
            })
            .collect();
        let caller = ip("198.51.100.1");
        assert!(limits.take(caller, 0.0).is_err());
        drop(running);
        drop(limits.take(caller, 0.0).expect("its own, untouched"));
    }

    /// Spec: hub: a probe names a port from 1024 to 65535 and nothing else is read.
    #[test]
    fn a_probe_names_a_port_from_1024() {
        assert_eq!(port_of(br#"{"port":9000}"#), Ok(9000));
        assert_eq!(
            port_of(br#"{"port":9000,"address":"192.0.2.1"}"#),
            Ok(9000),
            "an address in the call is never read"
        );
        assert_eq!(port_of(br#"{"port":1024}"#), Ok(1024));
        assert_eq!(port_of(br#"{"port":65535}"#), Ok(65535));
        for bad in [
            &br#"{"port":1023}"#[..],
            br#"{"port":65536}"#,
            br#"{"port":-1}"#,
            br#"{"port":"9000"}"#,
            br#"{}"#,
            b"not json",
        ] {
            assert!(port_of(bad).is_err(), "{}", String::from_utf8_lossy(bad));
        }
    }

    /// Spec: hub: the hub tries only an address on the internet, never one of its own ranges.
    #[test]
    fn only_an_address_on_the_internet_is_tried() {
        let own: Vec<crate::forwarded::ProxyRange> = vec![
            "198.18.0.0/15".parse().expect("a range"),
            "8.8.4.0/24".parse().expect("a range"),
        ];
        assert!(may_try(ip("8.8.8.8"), false, &own));
        assert!(!may_try(ip("8.8.4.4"), false, &own), "the hub's own range");
        for private in [
            "10.0.0.1",
            "192.168.1.20",
            "100.64.0.1",
            "fd00::1",
            "127.0.0.1",
        ] {
            assert!(!may_try(ip(private), false, &[]), "{private}");
        }
        assert!(
            may_try(ip("127.0.0.1"), true, &[]),
            "loopback on a test hub"
        );
        assert!(!may_try(ip("10.0.0.1"), true, &[]));
    }

    /// Spec: hub: the answer names the address and port tried, whether anything answered, and what
    /// the world said of itself.
    #[test]
    fn the_answer_names_the_address_and_what_the_world_said() {
        let reached = Answer {
            reached: true,
            status: Some(Status {
                state: 1,
                players: 3,
                software: "Empyrean".into(),
                version: "0.3.3".into(),
                name: "Stand-in".into(),
            }),
        };
        let v = answer_json(ip("203.0.113.9"), 9000, &reached);
        assert_eq!(v["address"], "203.0.113.9");
        assert_eq!(v["port"], 9000);
        assert_eq!(v["reached"], true);
        assert_eq!(v["world"]["name"], "Stand-in");
        assert_eq!(v["world"]["players"], 3);
        assert_eq!(v["world"]["state"], "open");
        let silent = Answer {
            reached: false,
            status: None,
        };
        let v = answer_json(ip("203.0.113.9"), 9000, &silent);
        assert_eq!(v["reached"], false);
        assert!(v["world"].is_null());
    }
}
