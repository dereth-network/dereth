//! The hub asks the Empyrean worlds it lists how they are, with the status ping
//! (`docs/networking/06-status-ping.md`), and lists what each said (`docs/network/05-hub.md`
//! section 1, "What a world says when the hub asks it").
//!
//! The status ping tells what a world's heartbeat does not: the era it plays, its systems, its
//! software and that software's version. A launcher on the desktop asks each world itself; a
//! browser cannot send a datagram, so a launcher's web page reads what the hub heard instead.
//!
//! **Which worlds.** Only a world of the public lists that runs Asheron's Call, names `Empyrean`
//! as its emulator, and has an address and a game port from 1024 to 65535. Every other server
//! drops the status ping unread, so the hub never sends it to them. The address is looked up
//! through the hub's own resolver and tried only when the hub would reach it on the internet
//! ([`crate::probe::may_try`]), so a listing cannot aim the hub at a private network.
//!
//! **How often.** A world that answered is asked again [`EVERY`] later; one that did not, after
//! [`AFTER_SILENCE`]. Each ask is the status ping alone, sent at most [`crate::probe::COPIES`]
//! times a second apart, as the probe's ([`crate::probe::status_ping`]): one hello and one ask a
//! minute for a world that answers, within Empyrean's own limits of four of each a minute from
//! one address. At most [`AT_ONCE`] are asked at a time.
//!
//! **What is kept.** Each world's last answer, in memory only, with the endpoint it came from and
//! when. It is listed for [`COUNTS_FOR`] after it was given, as a heartbeat counts, and only while
//! the world's endpoint is the one that answered. The world's own words (its era, software,
//! version and name) are listed with their hidden characters taken out ([`crate::text`]). Nothing
//! is written or logged.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use dereth_network_protocol::probe::Status;
use serde_json::{json, Value};

use crate::registry::{iso, HEARTBEAT_TTL_SECS};
use crate::server::Shared;

/// How long after a world answered it is asked again.
pub const EVERY: Duration = Duration::from_secs(60);

/// How long after a world said nothing it is asked again.
pub const AFTER_SILENCE: Duration = Duration::from_secs(600);

/// How long an answer is listed after it was given: as long as a heartbeat counts.
pub const COUNTS_FOR: Duration = Duration::from_secs(HEARTBEAT_TTL_SECS);

/// The most worlds asked at once.
pub const AT_ONCE: usize = 4;

/// One world to ask: its slug, and the address and game port its listing names.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Target {
    pub slug: String,
    pub host: String,
    pub port: u16,
}

/// The world a public listing (`world`, as the lists render it) names to ask, when it is one the
/// hub asks: a world of Asheron's Call (a listing naming no game runs it) whose emulator is
/// Empyrean, with an address and a game port from 1024.
#[must_use]
pub fn target(world: &Value) -> Option<Target> {
    let game = world.get("game").and_then(Value::as_str);
    if game.is_some_and(|g| g != dereth_network_protocol::Game::Dereth.word()) {
        return None;
    }
    let family = world.pointer("/emulator/family")?.as_str()?.trim();
    if !family.eq_ignore_ascii_case("Empyrean") {
        return None;
    }
    let host = world
        .pointer("/endpoint/address")?
        .as_str()?
        .trim()
        .to_owned();
    let port = world
        .pointer("/endpoint/port")?
        .as_u64()
        .and_then(|p| u16::try_from(p).ok())
        .filter(|p| *p >= crate::probe::LOWEST_PORT)?;
    if host.is_empty() {
        return None;
    }
    Some(Target {
        slug: world.get("slug")?.as_str()?.to_owned(),
        host,
        port,
    })
}

/// What one world said, and where and when.
#[derive(Debug, Clone)]
struct Said {
    /// The address and port its listing named when it was asked.
    from: (String, u16),
    status: Status,
    /// When it answered, on the registry's clock.
    at: SystemTime,
}

#[derive(Debug, Default)]
struct World {
    /// When it is asked next; `None` while it is being asked.
    next: Option<Instant>,
    said: Option<Said>,
}

/// Every listed Empyrean world's last answer, and when each is asked next.
#[derive(Debug, Default)]
pub struct Answers {
    worlds: Mutex<HashMap<String, World>>,
}

impl Answers {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, World>> {
        self.worlds
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Of `targets` (every world to ask now), those due at `now`, at most `most`, each marked as
    /// being asked. A world no longer among them is forgotten, its answer with it.
    pub fn due(&self, targets: &[Target], now: Instant, most: usize) -> Vec<Target> {
        let mut worlds = self.lock();
        worlds.retain(|slug, _| targets.iter().any(|t| &t.slug == slug));
        let mut due = Vec::new();
        for t in targets {
            if due.len() >= most {
                break;
            }
            let w = worlds.entry(t.slug.clone()).or_insert_with(|| World {
                next: Some(now),
                said: None,
            });
            if w.next.is_some_and(|n| n <= now) {
                w.next = None;
                due.push(t.clone());
            }
        }
        due
    }

    /// `target` was asked: what it said (`None` for nothing), at `at` on the registry's clock and
    /// `now` on the hub's own. It is asked again `every` later when it answered, and `silence`
    /// later when it did not; an answer that was not given leaves the last one to run out.
    pub fn record(
        &self,
        target: &Target,
        status: Option<Status>,
        at: SystemTime,
        now: Instant,
        (every, silence): (Duration, Duration),
    ) {
        let mut worlds = self.lock();
        let w = worlds.entry(target.slug.clone()).or_default();
        w.next = Some(now + if status.is_some() { every } else { silence });
        if let Some(status) = status {
            w.said = Some(Said {
                from: (target.host.clone(), target.port),
                status,
                at,
            });
        }
    }

    /// Sets `world`'s (a public listing's) `statusPing`: what it said when it was last asked, while
    /// that still counts at `now` and its listing names the endpoint that answered; `null`
    /// otherwise.
    pub fn list(&self, world: &mut Value, now: SystemTime) {
        let listed = target(world).and_then(|t| {
            let worlds = self.lock();
            let said = worlds.get(&t.slug)?.said.as_ref()?;
            let expires = said.at + COUNTS_FOR;
            (said.from == (t.host, t.port) && now < expires).then(|| listing(said, expires))
        });
        world["statusPing"] = listed.unwrap_or(Value::Null);
    }
}

/// An answer as the lists show it.
fn listing(said: &Said, expires: SystemTime) -> Value {
    let s = &said.status;
    let text = |t: &str| {
        let t = crate::text::without_hidden(t.trim());
        (!t.is_empty()).then_some(t)
    };
    // The systems as the Dereth client's `--era-features` takes them: the table's version, a
    // colon, the bits in hex. A table of version 0 names none.
    let systems = (s.table > 0).then(|| {
        let hex: String = s.systems.iter().map(|b| format!("{b:02x}")).collect();
        format!("{}:{hex}", s.table)
    });
    json!({
        "state": crate::probe::state_word(s.state),
        "playersOnline": s.players,
        "era": text(&s.era),
        "eraFeatures": systems,
        "software": text(&s.software),
        "version": text(&s.version),
        "name": text(&s.name),
        "askedAt": iso(said.at),
        "expiresAt": iso(expires),
    })
}

/// The address to ask `host` at: the first the hub's resolver finds that the hub may try, an IPv4
/// one before an IPv6 one. `None` when there is none, or the lookup did not finish.
fn address(shared: &Shared, host: &str) -> Option<IpAddr> {
    let crate::resolver::Resolved::Found(ips) = shared.resolver.resolve(host) else {
        return None;
    };
    let config = &shared.config;
    let ok =
        |ip: &&IpAddr| crate::probe::may_try(**ip, config.loopback_allowed, &config.deny_fetch);
    let v4 = ips.iter().filter(|ip| ip.is_ipv4()).find(ok);
    v4.or_else(|| ips.iter().find(ok)).copied()
}

/// Asks `target` and records what it said.
fn ask(shared: &Shared, target: &Target) {
    let config = &shared.config;
    let status = address(shared, &target.host).and_then(|ip| {
        let to = SocketAddr::new(crate::forwarded::canonical(ip), target.port);
        crate::probe::status_ping(to, config.probe_timing).status
    });
    let at = shared.registry().now();
    shared.pings.record(
        target,
        status,
        at,
        Instant::now(),
        (config.status_ping_every, config.status_ping_after_silence),
    );
}

/// The thread that asks the listed Empyrean worlds, until the hub stops.
pub(crate) fn start(shared: &Arc<Shared>) -> std::io::Result<Vec<JoinHandle<()>>> {
    let s = Arc::clone(shared);
    let thread = std::thread::Builder::new()
        .name("hub-status-ping".into())
        .spawn(move || run(&s))?;
    Ok(vec![thread])
}

fn run(shared: &Shared) {
    while !shared.stopping() {
        let targets: Vec<Target> = shared.public_worlds().iter().filter_map(target).collect();
        let due = shared.pings.due(&targets, Instant::now(), AT_ONCE);
        if due.is_empty() {
            std::thread::sleep(Duration::from_millis(250));
            continue;
        }
        std::thread::scope(|scope| {
            for t in &due {
                scope.spawn(move || ask(shared, t));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(slug: &str, family: &str, address: &str, port: u64) -> Value {
        json!({"slug": slug, "game": "dereth", "emulator": {"family": family},
               "endpoint": {"address": address, "port": port}})
    }

    fn status() -> Status {
        Status {
            state: 1,
            players: 2,
            era: "infiltration".into(),
            table: 1,
            systems: vec![0x00, 0x00, 0xff],
            software: "Empyrean".into(),
            version: "0.3.7".into(),
            name: "Ironwood\u{200b}".into(),
        }
    }

    /// Spec: hub: only a listed world of Asheron's Call whose emulator is Empyrean, with an address
    /// and a game port from 1024, is asked with the status ping.
    #[test]
    fn only_a_listed_empyrean_world_with_an_address_and_a_game_port_is_asked() {
        let t = target(&world("inf", "Empyrean", "inf.example.org", 9000)).expect("asked");
        assert_eq!(
            (t.slug.as_str(), t.host.as_str(), t.port),
            ("inf", "inf.example.org", 9000)
        );
        assert!(target(&world("inf", "empyrean", "inf.example.org", 9000)).is_some());
        let mut no_game = world("inf", "Empyrean", "inf.example.org", 9000);
        no_game.as_object_mut().unwrap().remove("game");
        assert!(
            target(&no_game).is_some(),
            "a listing naming no game runs Asheron's Call"
        );
        for (w, why) in [
            (
                world("ace", "ACE", "ace.example.org", 9000),
                "ACE drops it unread",
            ),
            (world("gdl", "GDL", "gdl.example.org", 9000), "so does GDLE"),
            (world("inf", "Empyrean", "", 9000), "no address"),
            (world("inf", "Empyrean", "inf.example.org", 0), "no port"),
            (
                world("inf", "Empyrean", "inf.example.org", 80),
                "below 1024",
            ),
            (
                world("inf", "Empyrean", "inf.example.org", 70_000),
                "no port",
            ),
        ] {
            assert!(target(&w).is_none(), "{why}");
        }
        let mut other_game = world("y", "Empyrean", "y.example.org", 9000);
        other_game["game"] = "auberean".into();
        assert!(target(&other_game).is_none(), "another game's world");
        let mut unseen = world("seen", "Empyrean", "x", 9000);
        unseen["endpoint"]["address"] = Value::Null;
        assert!(
            target(&unseen).is_none(),
            "an observed endpoint not seen yet"
        );
    }

    /// Spec: hub: a world that answered is asked again a minute later, one that said nothing ten
    /// minutes later, a world no longer listed is forgotten, and no more are asked at once than
    /// the hub allows.
    #[test]
    fn a_world_is_asked_again_a_minute_after_an_answer_and_ten_after_silence() {
        let answers = Answers::default();
        let start = Instant::now();
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let targets: Vec<Target> = ["a", "b", "c"]
            .iter()
            .map(|s| target(&world(s, "Empyrean", &format!("{s}.example.org"), 9000)).unwrap())
            .collect();
        let first = answers.due(&targets, start, 2);
        assert_eq!(first.len(), 2, "at most two at once here");
        assert_eq!(
            answers.due(&targets, start, 2).len(),
            1,
            "the third, not the two asked"
        );
        assert!(
            answers.due(&targets, start, 2).is_empty(),
            "none asked twice while it is asked"
        );
        let schedule = (EVERY, AFTER_SILENCE);
        answers.record(&targets[0], Some(status()), at, start, schedule);
        answers.record(&targets[1], None, at, start, schedule);
        answers.record(&targets[2], Some(status()), at, start, schedule);
        assert!(answers.due(&targets, start + EVERY / 2, 9).is_empty());
        let again = answers.due(&targets, start + EVERY, 9);
        assert_eq!(
            again.iter().map(|t| t.slug.as_str()).collect::<Vec<_>>(),
            ["a", "c"]
        );
        let silent = answers.due(&targets, start + AFTER_SILENCE, 9);
        assert_eq!(
            silent.iter().map(|t| t.slug.as_str()).collect::<Vec<_>>(),
            ["b"]
        );
        // `c` is no longer listed: forgotten, and asked at once when it is listed again.
        answers.due(&targets[..2], start + AFTER_SILENCE, 9);
        assert_eq!(answers.due(&targets, start + AFTER_SILENCE, 9).len(), 1);
    }

    /// Spec: hub: a world's answer is listed for as long as a heartbeat counts, with its state,
    /// players, era, systems, software, version and name, hidden characters taken out, and only
    /// while its listing names the endpoint that answered.
    #[test]
    fn an_answer_is_listed_while_it_counts_and_from_the_endpoint_listed() {
        let answers = Answers::default();
        let start = Instant::now();
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let mut w = world("inf", "Empyrean", "inf.example.org", 9000);
        let t = target(&w).unwrap();
        answers.list(&mut w, at);
        assert!(w["statusPing"].is_null(), "not asked yet");
        answers.due(std::slice::from_ref(&t), start, 1);
        answers.record(&t, Some(status()), at, start, (EVERY, AFTER_SILENCE));
        answers.list(&mut w, at + Duration::from_secs(10));
        let said = &w["statusPing"];
        assert_eq!(said["state"], "open");
        assert_eq!(said["playersOnline"], 2);
        assert_eq!(said["era"], "infiltration");
        assert_eq!(said["eraFeatures"], "1:0000ff");
        assert_eq!(said["software"], "Empyrean");
        assert_eq!(said["version"], "0.3.7");
        assert_eq!(said["name"], "Ironwood", "the hidden character taken out");
        assert_eq!(said["askedAt"], iso(at));
        assert_eq!(said["expiresAt"], iso(at + COUNTS_FOR));

        // Silence later leaves the answer to run out.
        answers.record(&t, None, at + EVERY, start + EVERY, (EVERY, AFTER_SILENCE));
        answers.list(&mut w, at + COUNTS_FOR - Duration::from_secs(1));
        assert_eq!(w["statusPing"]["era"], "infiltration");
        answers.list(&mut w, at + COUNTS_FOR);
        assert!(w["statusPing"].is_null(), "run out");

        // An answer from the endpoint the listing named before is not listed.
        answers.record(&t, Some(status()), at, start, (EVERY, AFTER_SILENCE));
        let mut moved = world("inf", "Empyrean", "new.example.org", 9000);
        answers.list(&mut moved, at);
        assert!(moved["statusPing"].is_null());

        // A world that names no era and no table says nothing of them.
        let bare = Status {
            era: String::new(),
            table: 0,
            systems: Vec::new(),
            ..status()
        };
        answers.record(&t, Some(bare), at, start, (EVERY, AFTER_SILENCE));
        answers.list(&mut w, at);
        assert!(w["statusPing"]["era"].is_null());
        assert!(w["statusPing"]["eraFeatures"].is_null());
    }
}
