//! How many players each world has, from the best source there is, for the public lists
//! (`docs/network/05-hub.md` section 1, "How many players a world has").
//!
//! A world's own word comes first: what it said when the hub asked it ([`crate::ping`]), then its
//! heartbeat. A world with neither has what the clients that logged in to it were told there (a
//! Dereth client at login; an Auberean client when the world says so in a line of text), then, for
//! a world of Asheron's Call, TreeStats' public count. Each source counts for its own lifetime, and
//! the first in that order still counting is the one listed, with its name and its age in seconds.
//!
//! **What a client's report proves.** Nothing a liar could not forge: a world signs nothing a client
//! could pass on, and the client's source is public. So a report is held to a plain rule instead:
//! it names the address and port of a listed world of the client's game, it can never be listed
//! over the world's own word, it does not reach the totals, the order of the lists or the compatibility feeds, and each
//! address has only a few. What is kept is the latest count and its time, for each world, in memory
//! only; the address is kept only as the limit's counter, for as long as that limit lasts, and is
//! never written or logged.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use dereth_network_protocol::limits::Rate;
use dereth_network_protocol::Game;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::fetch::{any_url, AddressRule, Fetch, Url};
use crate::probe::Limits;
use crate::registry::parse_iso;
use crate::server::Shared;

/// Where a client reports the count it was told at login.
pub const PATH: &str = "/v1/players";

/// How long a client's report counts after it was made.
pub const CLIENT_COUNTS_FOR: Duration = Duration::from_secs(2 * 3600);

/// How long a TreeStats count counts after TreeStats took it.
pub const TREESTATS_COUNTS_FOR: Duration = Duration::from_secs(6 * 3600);

/// The most players a report or a TreeStats row may say a world has; past it, the row is no count.
pub const MOST_PLAYERS: u32 = 5_000;

/// The most host names one report that names an address has looked up, to find its world.
pub const MOST_NAMES_LOOKED_UP: usize = 16;

/// Reports for the whole hub together: two hundred at once, then five a second.
pub const OVERALL: Rate = Rate {
    burst: 200.0,
    per_sec: 5.0,
};

/// Where TreeStats publishes its counts.
pub const TREESTATS_URL: &str = "https://treestats.net/player_counts-latest.json";

/// How long after a read of TreeStats the next is made.
pub const TREESTATS_EVERY: Duration = Duration::from_secs(15 * 60);

/// How long after a failed read of TreeStats the next is made.
pub const TREESTATS_AFTER_FAILURE: Duration = Duration::from_secs(3600);

/// The most of TreeStats' answer that is read.
const TREESTATS_MOST_BYTES: usize = 256 * 1024;

/// The most worlds kept from TreeStats' answer.
const TREESTATS_MOST_ROWS: usize = 2_048;

/// A count and when it was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Count {
    players: u32,
    at: SystemTime,
}

/// A count as the lists show it.
fn listing(players: u64, source: &str, at: SystemTime, now: SystemTime) -> Value {
    let age = now.duration_since(at).map_or(0, |d| d.as_secs());
    json!({ "players": players, "source": source, "ageSeconds": age })
}

/// `count` as the lists show it, while it still counts at `now`.
fn listed(count: Option<Count>, source: &str, lasts: Duration, now: SystemTime) -> Option<Value> {
    let c = count.filter(|c| c.at + lasts > now)?;
    Some(listing(u64::from(c.players), source, c.at, now))
}

/// A name as it is matched between lists: its letters and digits, folded, in lower case.
fn name_key(name: &str) -> String {
    name.chars()
        .flat_map(|c| crate::fold::fold_char(c).chars().collect::<Vec<_>>())
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The game `world` (a public listing) plays: one that names none plays Asheron's Call.
fn game_of(world: &Value) -> Option<Game> {
    world["game"]
        .as_str()
        .map_or(Some(Game::Dereth), |g| g.parse().ok())
}

/// The address and port `world` (a public listing) is played at, when it is a world of `game`.
fn endpoint_of(world: &Value, game: Game) -> Option<(&str, u16)> {
    if game_of(world) != Some(game) {
        return None;
    }
    let port = world.pointer("/endpoint/port").and_then(Value::as_u64)?;
    let address = world.pointer("/endpoint/address").and_then(Value::as_str)?;
    Some((address.trim(), u16::try_from(port).ok()?))
}

/// Whether `world` (a public listing) is the world of `game` at `host` and `port`.
fn is_at(world: &Value, game: Game, host: &str, port: u16) -> bool {
    endpoint_of(world, game).is_some_and(|(a, p)| p == port && a.eq_ignore_ascii_case(host))
}

/// The world's own word: what it said when the hub asked it, then its heartbeat.
fn own_word(world: &Value, now: SystemTime) -> Option<Value> {
    let ping = &world["statusPing"];
    if let (Some(n), Some(at)) = (
        ping["playersOnline"].as_u64(),
        ping["askedAt"].as_str().and_then(parse_iso),
    ) {
        return Some(listing(n, "ping", at, now));
    }
    let status = &world["status"];
    let at = status["lastSeenAt"].as_str().and_then(parse_iso)?;
    Some(listing(
        status["playersOnline"].as_u64()?,
        "heartbeat",
        at,
        now,
    ))
}

/// What is known of each world's count beside its own word, in memory only.
#[derive(Debug)]
pub struct Counts {
    /// The latest report for each world, by its game and slug, so a report for a world of one
    /// game is never another game's, even under a slug that changed hands.
    reports: Mutex<HashMap<(Game, String), Count>>,
    /// TreeStats' latest count for each name ([`name_key`]).
    treestats: Mutex<HashMap<String, Count>>,
    limits: Limits,
}

impl Counts {
    /// Counts with reports limited to `per_caller` for each address and `overall` for the hub.
    #[must_use]
    pub fn new(per_caller: Rate, overall: Rate) -> Self {
        Self {
            reports: Mutex::new(HashMap::new()),
            treestats: Mutex::new(HashMap::new()),
            limits: Limits::new(per_caller, overall),
        }
    }

    fn report(&self, game: Game, slug: &str) -> Option<Count> {
        self.reports
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&(game, slug.to_owned()))
            .copied()
    }

    /// A report that `slug`, a world of `game`, has `players` at `at`, in place of the last.
    /// Reports past their lifetime are forgotten, so a world no longer listed leaves nothing
    /// behind.
    pub fn record_report(&self, game: Game, slug: &str, players: u32, at: SystemTime) {
        let mut reports = self
            .reports
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reports.retain(|_, c| c.at + CLIENT_COUNTS_FOR > at);
        reports.insert((game, slug.to_owned()), Count { players, at });
    }

    /// TreeStats' answer just read, in place of the last.
    pub fn record_treestats(&self, counts: HashMap<String, (u32, SystemTime)>) {
        *self
            .treestats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = counts
            .into_iter()
            .map(|(name, (players, at))| (name, Count { players, at }))
            .collect();
    }

    /// A count from what is known beside the world's own word: its clients' reports, then, for a
    /// world of Asheron's Call, TreeStats'.
    fn borrowed(&self, world: &Value, now: SystemTime) -> Option<Value> {
        let game = game_of(world)?;
        let report = self.report(game, world["slug"].as_str()?);
        listed(report, "client", CLIENT_COUNTS_FOR, now).or_else(|| {
            if !game.is_dereth() {
                return None;
            }
            let name = name_key(world["name"].as_str()?);
            let treestats = self
                .treestats
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&name)
                .copied();
            listed(treestats, "treestats", TREESTATS_COUNTS_FOR, now)
        })
    }

    /// Sets `world`'s (a public listing's, its `statusPing` already set) `population`: the first
    /// source in the order of trust still counting at `now`, or `null`. A world that says it is
    /// offline has none.
    pub fn list(&self, world: &mut Value, now: SystemTime) {
        let count = if world["status"]["state"] == "offline" {
            None
        } else {
            own_word(world, now).or_else(|| self.borrowed(world, now))
        };
        world["population"] = count.unwrap_or(Value::Null);
    }
}

/// A client's report.
#[derive(Debug, Deserialize)]
struct Report {
    host: String,
    port: u16,
    players: u32,
    /// The game the client plays: a report that names none is a Dereth client's.
    #[serde(default)]
    game: Game,
}

/// Reads a report, held to what a report may say.
fn read_report(body: &[u8]) -> Result<Report, String> {
    let mut r: Report = serde_json::from_slice(body)
        .map_err(|_| "Expected {host, port, players} and an optional game".to_owned())?;
    r.host = r.host.trim().to_owned();
    if r.host.is_empty() || r.host.len() > 253 {
        return Err("host: an address of at most 253 characters".to_owned());
    }
    if r.port == 0 {
        return Err("port: 1-65535".to_owned());
    }
    if r.players > MOST_PLAYERS {
        return Err(format!("players: 0-{MOST_PLAYERS}"));
    }
    Ok(r)
}

/// The slugs of the worlds of `game` at `host` and `port`, among `worlds` (the public listings). A
/// world is named by where the client connected, so two listings of one address each take the
/// report.
fn slugs_at(worlds: &[Value], game: Game, host: &str, port: u16) -> Vec<String> {
    worlds
        .iter()
        .filter(|w| is_at(w, game, host, port))
        .filter_map(|w| Some(w["slug"].as_str()?.to_owned()))
        .collect()
}

/// The slugs of the worlds of `game` listed by a host name at `port` whose name `look_up` finds at
/// `ip`: a client given the address its world's name stands for reports that address. At most
/// [`MOST_NAMES_LOOKED_UP`] names are looked up; `look_up` answers none for a name it cannot find,
/// or not in time.
fn slugs_resolving_to(
    worlds: &[Value],
    game: Game,
    ip: IpAddr,
    port: u16,
    mut look_up: impl FnMut(&str) -> Vec<IpAddr>,
) -> Vec<String> {
    let ip = crate::forwarded::canonical(ip);
    let mut looked_up: HashMap<String, bool> = HashMap::new();
    let mut slugs = Vec::new();
    for w in worlds {
        let Some((name, p)) = endpoint_of(w, game) else {
            continue;
        };
        if p != port || name.trim_matches(['[', ']']).parse::<IpAddr>().is_ok() {
            continue;
        }
        let name = name.to_ascii_lowercase();
        let there = match looked_up.get(&name) {
            Some(&there) => there,
            None if looked_up.len() < MOST_NAMES_LOOKED_UP => {
                let there = look_up(&name)
                    .into_iter()
                    .any(|a| crate::forwarded::canonical(a) == ip);
                looked_up.insert(name, there);
                there
            }
            None => false,
        };
        if there {
            slugs.extend(w["slug"].as_str().map(str::to_owned));
        }
    }
    slugs
}

/// A client's report: anything well-formed is answered alike, whether or not it named a world.
pub(crate) fn handle(shared: &Shared, method: &str, body: &[u8], ip: IpAddr) -> crate::api::Reply {
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
    let report = match read_report(body) {
        Ok(report) => report,
        Err(why) => return error(400, &why),
    };
    let at = shared.registry().now();
    let seconds = at
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64());
    match shared.counts.limits.take(ip, seconds) {
        Ok(underway) => drop(underway),
        Err(refused) => return crate::server::too_many(refused.wait, crate::api::Cors::Any),
    }
    let worlds = shared.public_worlds();
    let mut slugs = slugs_at(&worlds, report.game, &report.host, report.port);
    if slugs.is_empty() {
        if let Ok(ip) = report.host.trim_matches(['[', ']']).parse::<IpAddr>() {
            // The names looked up share one deadline, so a report never waits longer than one.
            let until = Instant::now() + shared.config.resolve_deadline;
            slugs = slugs_resolving_to(&worlds, report.game, ip, report.port, |name| {
                let left = until.saturating_duration_since(Instant::now());
                match shared.resolver.resolve_within(name, left) {
                    crate::resolver::Resolved::Found(found) => found,
                    crate::resolver::Resolved::Unknown => Vec::new(),
                }
            });
        }
    }
    for slug in slugs {
        shared
            .counts
            .record_report(report.game, &slug, report.players, at);
    }
    reply(200, json!({}))
}

/// TreeStats' time, `2026-11-06 08:04:48 UTC`.
fn treestats_time(s: &str) -> Option<SystemTime> {
    let t = s.trim().strip_suffix(" UTC")?;
    let (day, time) = t.split_once(' ')?;
    (day.len() == 10 && time.len() == 8).then_some(())?;
    parse_iso(&format!("{day}T{time}Z"))
}

/// TreeStats' answer: an array of `{"server", "count", "date"}`, by the name it gives each world.
/// `None` when it is not one; a row that does not read is left out.
#[must_use]
pub fn read_treestats(body: &[u8]) -> Option<HashMap<String, (u32, SystemTime)>> {
    let rows: Value = serde_json::from_slice(body).ok()?;
    Some(
        rows.as_array()?
            .iter()
            .take(TREESTATS_MOST_ROWS)
            .filter_map(|row| {
                let name = name_key(row["server"].as_str()?);
                let count = u32::try_from(row["count"].as_u64()?).ok()?;
                let at = treestats_time(row["date"].as_str()?)?;
                (!name.is_empty() && count <= MOST_PLAYERS).then_some((name, (count, at)))
            })
            .collect(),
    )
}

/// Reads TreeStats once. `None` when it is gone or answers anything else.
fn fetch_treestats(shared: &Shared, url: &Url) -> Option<HashMap<String, (u32, SystemTime)>> {
    let config = &shared.config;
    let rule: AddressRule = if config.loopback_allowed {
        |_| true
    } else {
        crate::fetch::is_public
    };
    let r = Fetch {
        method: "GET",
        url: url.clone(),
        headers: vec![(
            "User-Agent".to_owned(),
            concat!("dereth-network-hub/", env!("CARGO_PKG_VERSION")).to_owned(),
        )],
        body: Vec::new(),
        timeout: config.fetch_timeout,
        max_body: TREESTATS_MOST_BYTES,
        max_redirects: 2,
        rule,
        deny: config.deny_fetch.clone(),
        check: any_url,
        check_with: Vec::new(),
    }
    .send()
    .ok()?;
    if r.status != 200 {
        return None;
    }
    read_treestats(&r.body)
}

/// The thread that reads TreeStats, until the hub stops; none when the hub is told not to.
pub(crate) fn start(shared: &Arc<Shared>) -> std::io::Result<Vec<JoinHandle<()>>> {
    let Some(url) = shared
        .config
        .treestats_url
        .as_deref()
        .and_then(|u| Url::parse(u).ok())
    else {
        return Ok(Vec::new());
    };
    let s = Arc::clone(shared);
    let thread = std::thread::Builder::new()
        .name("hub-treestats".into())
        .spawn(move || run(&s, &url))?;
    Ok(vec![thread])
}

fn run(shared: &Shared, url: &Url) {
    let mut next = Instant::now();
    while !shared.stopping() {
        if Instant::now() < next {
            std::thread::sleep(Duration::from_millis(250));
            continue;
        }
        // A failed read leaves the last to run out, and the next is an hour off.
        let read = fetch_treestats(shared, url);
        next = Instant::now()
            + if read.is_some() {
                TREESTATS_EVERY
            } else {
                TREESTATS_AFTER_FAILURE
            };
        if let Some(counts) = read {
            shared.counts.record_treestats(counts);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    const NOW: u64 = 1_800_000_000;

    fn counts() -> Counts {
        Counts::new(crate::probe::PER_CALLER, OVERALL)
    }

    fn world(slug: &str, name: &str) -> Value {
        json!({
            "slug": slug, "name": name, "game": "dereth",
            "endpoint": {"address": format!("{slug}.example.org"), "port": 9000},
            "status": {"state": "unknown", "playersOnline": null, "lastSeenAt": null},
            "statusPing": null,
        })
    }

    fn population(counts: &Counts, w: &mut Value, now: u64) -> Value {
        counts.list(w, at(now));
        w["population"].clone()
    }

    fn treestats(counts: &Counts, name: &str, players: u32, when: u64) {
        counts.record_treestats(HashMap::from([(name_key(name), (players, at(when)))]));
    }

    /// Spec: hub: a world's count comes from what it said when asked, then its heartbeat, then a
    /// client's report, then TreeStats, each with its source and age.
    #[test]
    fn the_worlds_own_word_goes_before_a_clients_and_a_clients_before_treestats() {
        let counts = counts();
        let mut w = world("coldeve", "Coldeve");
        assert_eq!(population(&counts, &mut w, NOW), Value::Null, "nothing yet");

        treestats(&counts, "Coldeve", 630, NOW - 2000);
        assert_eq!(
            population(&counts, &mut w, NOW),
            json!({"players": 630, "source": "treestats", "ageSeconds": 2000})
        );

        counts.record_report(Game::Dereth, "coldeve", 12, at(NOW - 40));
        assert_eq!(
            population(&counts, &mut w, NOW),
            json!({"players": 12, "source": "client", "ageSeconds": 40})
        );

        w["status"] = json!({"state": "online", "playersOnline": 7,
            "lastSeenAt": crate::registry::iso(at(NOW - 30))});
        assert_eq!(
            population(&counts, &mut w, NOW),
            json!({"players": 7, "source": "heartbeat", "ageSeconds": 30})
        );

        w["statusPing"] = json!({"playersOnline": 8,
            "askedAt": crate::registry::iso(at(NOW - 5))});
        assert_eq!(
            population(&counts, &mut w, NOW),
            json!({"players": 8, "source": "ping", "ageSeconds": 5})
        );
    }

    /// Spec: hub: a report lasts two hours and a TreeStats count six, and an older one is not
    /// listed; the next source down takes its place.
    #[test]
    fn a_stale_count_gives_way_and_then_leaves_the_list() {
        let counts = counts();
        let mut w = world("coldeve", "Coldeve");
        counts.record_report(Game::Dereth, "coldeve", 12, at(NOW));
        treestats(&counts, "Coldeve", 630, NOW);

        let lasts = CLIENT_COUNTS_FOR.as_secs();
        let then = population(&counts, &mut w, NOW + lasts - 1);
        assert_eq!(then["source"], "client");
        let then = population(&counts, &mut w, NOW + lasts);
        assert_eq!(then["source"], "treestats", "the report ran out");

        let lasts = TREESTATS_COUNTS_FOR.as_secs();
        assert_eq!(population(&counts, &mut w, NOW + lasts - 1)["players"], 630);
        assert_eq!(population(&counts, &mut w, NOW + lasts), Value::Null);
    }

    /// Spec: hub: the latest report for a world replaces the last, whatever it says.
    #[test]
    fn the_latest_report_is_the_count() {
        let counts = counts();
        let mut w = world("coldeve", "Coldeve");
        counts.record_report(Game::Dereth, "coldeve", 12, at(NOW));
        counts.record_report(Game::Dereth, "coldeve", 3, at(NOW + 1));
        assert_eq!(population(&counts, &mut w, NOW + 1)["players"], 3);
    }

    /// Spec: hub: a report past its lifetime is forgotten when the next is made, so the table holds
    /// no world long after it was last played.
    #[test]
    fn an_old_report_leaves_nothing_behind() {
        let counts = counts();
        counts.record_report(Game::Dereth, "gone", 12, at(NOW));
        counts.record_report(
            Game::Dereth,
            "coldeve",
            3,
            at(NOW + CLIENT_COUNTS_FOR.as_secs()),
        );
        assert!(counts.report(Game::Dereth, "gone").is_none());
    }

    fn ac2_world(slug: &str, name: &str) -> Value {
        let mut w = world(slug, name);
        w["game"] = "auberean".into();
        w
    }

    /// Spec: hub: a world that says it is offline has no count.
    #[test]
    fn an_offline_world_has_no_borrowed_count() {
        let counts = counts();
        counts.record_report(Game::Dereth, "coldeve", 12, at(NOW));
        let mut off = world("coldeve", "Coldeve");
        off["status"]["state"] = "offline".into();
        assert_eq!(population(&counts, &mut off, NOW), Value::Null);
    }

    /// Spec: hub: a world of Asheron's Call 2 takes its own clients' reports and never TreeStats',
    /// a report for one game never lands on a world of the other, and a linked world's own word
    /// goes first in either game.
    #[test]
    fn a_world_of_asherons_call_2_takes_its_clients_reports_and_never_treestats() {
        let counts = counts();
        let mut ironwood = ac2_world("ironwood", "Ironwood");
        treestats(&counts, "Ironwood", 5, NOW);
        counts.record_report(Game::Dereth, "ironwood", 12, at(NOW));
        assert_eq!(
            population(&counts, &mut ironwood, NOW),
            Value::Null,
            "neither TreeStats' count nor a Dereth client's"
        );

        counts.record_report(Game::Auberean, "ironwood", 3, at(NOW - 60));
        assert_eq!(
            population(&counts, &mut ironwood, NOW),
            json!({"players": 3, "source": "client", "ageSeconds": 60})
        );
        let mut coldeve = world("coldeve", "Coldeve");
        counts.record_report(Game::Auberean, "coldeve", 9, at(NOW));
        assert_eq!(
            population(&counts, &mut coldeve, NOW),
            Value::Null,
            "an Auberean client's report is no Asheron's Call world's"
        );

        let lasts = CLIENT_COUNTS_FOR.as_secs();
        assert_eq!(
            population(&counts, &mut ironwood, NOW + lasts - 61)["players"],
            3
        );
        assert_eq!(
            population(&counts, &mut ironwood, NOW + lasts - 60),
            Value::Null,
            "two hours, as a Dereth client's"
        );

        ironwood["status"] = json!({"state": "online", "playersOnline": 2,
            "lastSeenAt": crate::registry::iso(at(NOW))});
        assert_eq!(
            population(&counts, &mut ironwood, NOW)["source"],
            "heartbeat",
            "a linked world of any game reports for itself"
        );
    }

    /// Spec: hub: a report goes to the listed world of the client's game at the address and port
    /// the client connected to, in any case, and never to another game's.
    #[test]
    fn a_report_goes_to_the_world_of_its_game_at_its_address_and_port() {
        let mut same_place = ac2_world("other", "Other");
        same_place["endpoint"]["address"] = "coldeve.example.org".into();
        let worlds = [
            world("coldeve", "Coldeve"),
            ac2_world("ironwood", "Ironwood"),
            same_place,
        ];
        let at = |game, host, port| slugs_at(&worlds, game, host, port);
        assert_eq!(
            at(Game::Dereth, "COLDEVE.example.org", 9000),
            ["coldeve"],
            "by its address"
        );
        assert_eq!(
            at(Game::Auberean, "coldeve.example.org", 9000),
            ["other"],
            "a world of the other game at the same address and port"
        );
        assert!(at(Game::Dereth, "coldeve.example.org", 9001).is_empty());
        assert!(at(Game::Dereth, "ironwood.example.org", 9000).is_empty());
        assert_eq!(
            at(Game::Auberean, "ironwood.example.org", 9000),
            ["ironwood"]
        );
    }

    /// Spec: hub: a report naming an address goes to the worlds of its game listed by a name that
    /// stands for that address at that port; a world listed by its address takes it only by that
    /// address, and only so many names are looked up.
    #[test]
    fn a_report_naming_an_address_finds_the_world_whose_name_stands_for_it() {
        let mut by_address = ac2_world("by-address", "By Address");
        by_address["endpoint"]["address"] = "203.0.113.9".into();
        let worlds = [
            ac2_world("icefire", "Icefire"),
            world("coldeve", "Coldeve"),
            by_address,
        ];
        let asked = std::cell::RefCell::new(Vec::new());
        let mut look_up = |name: &str| {
            asked.borrow_mut().push(name.to_owned());
            match name {
                "icefire.example.org" | "coldeve.example.org" => {
                    vec!["203.0.113.9".parse().unwrap()]
                }
                _ => Vec::new(),
            }
        };
        let ip: IpAddr = "203.0.113.9".parse().unwrap();
        assert_eq!(
            slugs_resolving_to(&worlds, Game::Auberean, ip, 9000, &mut look_up),
            ["icefire"]
        );
        assert_eq!(
            *asked.borrow(),
            ["icefire.example.org"],
            "only names of the game's worlds at that port"
        );
        let mapped: IpAddr = "::ffff:203.0.113.9".parse().unwrap();
        assert_eq!(
            slugs_resolving_to(&worlds, Game::Auberean, mapped, 9000, &mut look_up),
            ["icefire"],
            "the same address written for IPv6"
        );
        assert!(slugs_resolving_to(&worlds, Game::Auberean, ip, 9001, &mut look_up).is_empty());
        assert!(slugs_resolving_to(
            &worlds,
            Game::Auberean,
            "198.51.100.1".parse().unwrap(),
            9000,
            &mut look_up
        )
        .is_empty());

        let many: Vec<Value> = (0..MOST_NAMES_LOOKED_UP + 4)
            .map(|i| ac2_world(&format!("w{i}"), "W"))
            .collect();
        let mut asked = 0;
        let found = slugs_resolving_to(&many, Game::Auberean, ip, 9000, |_| {
            asked += 1;
            vec![ip]
        });
        assert_eq!(asked, MOST_NAMES_LOOKED_UP);
        assert_eq!(found.len(), MOST_NAMES_LOOKED_UP);
    }

    /// Spec: hub: a report is an address, a port from 1, a count of at most 5,000 and the game
    /// the client plays, Asheron's Call when it names none.
    #[test]
    fn a_report_says_what_a_report_may_say() {
        let read = |v: Value| read_report(v.to_string().as_bytes());
        let ok = read(json!({"host": " a.example.org ", "port": 9000, "players": 5000})).unwrap();
        assert_eq!(
            (ok.host.as_str(), ok.port, ok.players, ok.game),
            ("a.example.org", 9000, 5000, Game::Dereth)
        );
        let ac2 = json!({"host": "a", "port": 8610, "players": 3, "game": "auberean"});
        assert_eq!(read(ac2).unwrap().game, Game::Auberean);
        assert!(read(json!({"host": "a", "port": 9000, "players": 1, "game": "Dereth"})).is_err());
        assert!(read(json!({"host": "a", "port": 9000, "players": 1, "game": "ac2"})).is_err());
        for bad in [
            json!({"host": "a", "port": 9000, "players": 5001}),
            json!({"host": "a", "port": 9000, "players": -1}),
            json!({"host": "a", "port": 0, "players": 1}),
            json!({"host": "a", "port": 70000, "players": 1}),
            json!({"host": "  ", "port": 9000, "players": 1}),
            json!({"host": "a".repeat(254), "port": 9000, "players": 1}),
            json!({"port": 9000, "players": 1}),
        ] {
            assert!(read(bad.clone()).is_err(), "{bad}");
        }
        assert!(read_report(b"not json").is_err());
    }

    /// Spec: hub: TreeStats' public answer is read by name, a row that does not read is left out,
    /// and an answer that is no list is none.
    #[test]
    fn treestats_answer_reads_by_name_and_tolerates_bad_rows() {
        let body = br#"[
            {"server": "Coldeve", "count": 630, "date": "2026-11-06 08:04:48 UTC", "age": "34 minutes ago"},
            {"server": "Thistle Down", "count": 0, "date": "2026-11-05 01:00:00 UTC"},
            {"server": "NoDate", "count": 4},
            {"server": "Liar", "count": 9000000, "date": "2026-11-06 08:04:48 UTC"},
            {"server": "BadDate", "count": 4, "date": "yesterday"},
            {"count": 1, "date": "2026-11-06 08:04:48 UTC"},
            "nonsense"
        ]"#;
        let read = read_treestats(body).expect("a list");
        assert_eq!(read.len(), 2, "{read:?}");
        assert_eq!(read["coldeve"].0, 630);
        assert_eq!(
            read["coldeve"].1,
            parse_iso("2026-11-06T08:04:48Z").expect("a time")
        );
        assert_eq!(read["thistledown"].0, 0);
        assert!(read_treestats(b"{}").is_none());
        assert!(read_treestats(b"<html>gone</html>").is_none());
    }

    /// Spec: hub: a world is matched to TreeStats' row by name, whatever its spacing and case.
    #[test]
    fn a_world_matches_treestats_by_name_whatever_its_spacing() {
        let counts = counts();
        counts.record_treestats(
            read_treestats(
                br#"[{"server": "ThistleDown", "count": 9, "date": "2026-11-06 08:04:48 UTC"}]"#,
            )
            .expect("a list"),
        );
        let when = parse_iso("2026-11-06T08:04:48Z").expect("a time");
        let mut w = world("thistle", "Thistle Down");
        counts.list(&mut w, when + Duration::from_secs(60));
        assert_eq!(
            w["population"],
            json!({"players": 9, "source": "treestats", "ageSeconds": 60})
        );
    }

    /// Spec: hub: one address has five reports at once, then one every twelve minutes.
    #[test]
    fn one_address_has_only_a_few_reports() {
        let counts = counts();
        let ip: IpAddr = "203.0.113.9".parse().expect("an address");
        for _ in 0..5 {
            assert!(counts.limits.take(ip, 100.0).is_ok());
        }
        assert!(counts.limits.take(ip, 100.0).is_err(), "the sixth at once");
        assert!(
            counts.limits.take(ip, 100.0 + 720.0).is_ok(),
            "twelve minutes on"
        );
    }
}
