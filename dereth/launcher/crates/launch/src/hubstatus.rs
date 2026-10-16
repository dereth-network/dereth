//! What the Dereth Network's hub says of the worlds it lists: whether each is up and how many are
//! on, from the heartbeat a linked world (Empyrean, or ACE with the network's mod) sends the hub
//! about once a minute.
//!
//! Every row of the hub's list (`/v1/servers`) carries its world's status. The day's copy of the
//! list is kept for the worlds it names, and its status is stale by the time the copy is read
//! again, so the status is read apart from it: the same list, a page at a time, read again about
//! once a minute while the worlds are shown and whenever the list itself is fetched, and kept in
//! memory only. A status is taken only from a heartbeat (`source` `server-heartbeat`), only in a
//! state the world reported (`online`, `degraded`, `maintenance` or `offline`; never `unknown`, a
//! world that never reported, or `stale`), and only until the heartbeat's `expiresAt`: past that it
//! is as if the hub had said nothing. A read that fails changes nothing, and the next is due a
//! minute later.
//!
//! A row's status goes to the world shown under the launcher's key for it, [`slug_for`] its name,
//! as the list's worlds are kept. Two worlds whose names spell one key are told apart in the list
//! by the order it happened to be read in, which a later read need not repeat, so neither takes a
//! status. A world renamed, or missing from a later read, takes none from it.
//!
//! Only the hub's list says this. While the worlds shown are another list's (the `Servers.xml` the
//! player names, or the community's own when the hub did not answer), nothing is read and no
//! status is kept.
//!
//! What a world says of itself (Empyrean's status ping or document) goes before the hub's word,
//! and the hub's before whether the world answered a probe ([`crate::worlds::shown`]).
//!
//! The same read brings where a browser reaches each world, as the hub names it while the world
//! is linked ([`crate::direct`]): the hub's word on that goes before the day's copy, which a
//! world's link coming or going since has made stale.
//!
//! [`slug_for`]: crate::serverlist::slug_for

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::serverlist::{
    hub_key, hub_rows, ListCache, ListFetch, ListFormat, ListSource, MalformedList,
};
use crate::world::WorldState;

/// How long after one read of the hub's list for status the next is due, in seconds.
pub const READ_EVERY_SECS: u64 = 60;

/// A world's status as the hub has it from the world's own heartbeat, and where a browser reaches
/// the world, as the hub names it while the world is linked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HubStatus {
    pub state: WorldState,
    /// How many are on, when the world said.
    pub players: Option<u32>,
    /// When the heartbeat stops counting, in seconds since the epoch.
    pub expires_at: u64,
    /// The world's WebSocket endpoint ([`crate::direct`]).
    pub websocket_url: Option<String>,
    /// The world's status document.
    pub status_url: Option<String>,
}

impl HubStatus {
    /// A row's `status` read. `None` unless it is a heartbeat's, in a state the world reported,
    /// with an expiry that reads. The states are taken as Empyrean's own are: `maintenance` (a
    /// world that is up and lets nobody in, as Empyrean reports a world it has not opened) is
    /// starting, and `degraded` is up.
    #[must_use]
    pub fn read(status: &Value) -> Option<Self> {
        if status.get("source")?.as_str()? != "server-heartbeat" {
            return None;
        }
        let state = match status.get("state")?.as_str()? {
            "online" | "degraded" => WorldState::Online,
            "maintenance" => WorldState::Starting,
            "offline" => WorldState::Offline,
            _ => return None,
        };
        let expires_at = parse_time(status.get("expiresAt")?.as_str()?)?;
        let players = status
            .get("playersOnline")
            .and_then(Value::as_u64)
            .map(|n| u32::try_from(n).unwrap_or(u32::MAX));
        Some(Self {
            state,
            players,
            expires_at,
            websocket_url: None,
            status_url: None,
        })
    }

    /// Whether the heartbeat still counts at `now` (seconds since the epoch).
    #[must_use]
    pub fn is_current(&self, now: u64) -> bool {
        now < self.expires_at
    }
}

/// The status the hub's list, its pages in order, gives each world, by the key the launcher keeps
/// the world under. A row the launcher does not show gives none, and neither do two rows whose
/// names spell one key.
///
/// # Errors
/// [`MalformedList`] when a page is not a page of the list.
pub fn read_pages(pages: &[String]) -> Result<BTreeMap<String, HubStatus>, MalformedList> {
    let mut by_key: BTreeMap<String, Option<HubStatus>> = BTreeMap::new();
    let mut namesakes = BTreeSet::new();
    for row in hub_rows(pages)? {
        let Some(key) = hub_key(&row) else {
            continue;
        };
        let status = row.get("status").and_then(HubStatus::read).map(|s| {
            let (websocket_url, status_url) = crate::serverlist::hub_web_addresses(&row);
            HubStatus {
                websocket_url,
                status_url,
                ..s
            }
        });
        if by_key.insert(key.clone(), status).is_some() {
            namesakes.insert(key);
        }
    }
    Ok(by_key
        .into_iter()
        .filter(|(key, _)| !namesakes.contains(key))
        .filter_map(|(key, status)| Some((key, status?)))
        .collect())
}

/// The hub's status for the worlds shown, kept in memory, and when to read it again.
///
/// Whoever shows the worlds tells it which list is on screen ([`Self::copy_shown`] for the day's
/// copy, [`Self::list_read`] for a list just fetched), asks it for a read when one is due
/// ([`Self::start`]), fetches each page the read names as the list's own are fetched, hands the
/// read back ([`Self::finish`]), and asks it for a world's status as the world is shown
/// ([`Self::current`]). The desktop launcher and the web page keep one each.
#[derive(Debug, Clone, Default)]
pub struct HubWatch {
    /// The hub's list, which the status is read from; `None` when the list is read from elsewhere
    /// (the `Servers.xml` the player names).
    source: Option<ListSource>,
    /// Whether the worlds on screen are the hub's list's.
    hubs_list_shown: bool,
    statuses: BTreeMap<String, HubStatus>,
    /// When the latest read started, in seconds since the epoch.
    last_read: Option<u64>,
    /// Whether a read is under way.
    reading: bool,
}

impl HubWatch {
    /// A watch for the list read from `sources` ([`crate::serverlist::sources`]): over the hub's
    /// list among them, and over nothing when none is the hub's.
    #[must_use]
    pub fn new(sources: &[ListSource]) -> Self {
        let mut watch = Self::default();
        watch.set_sources(sources);
        watch
    }

    /// The list is read from `sources` from now on. When that changes which list the status is
    /// read from, what was read from the other is forgotten.
    pub fn set_sources(&mut self, sources: &[ListSource]) {
        let source = sources
            .iter()
            .find(|s| s.format == ListFormat::Hub)
            .cloned();
        if source != self.source {
            self.source = source;
            self.statuses.clear();
            self.last_read = None;
        }
    }

    /// The day's copy of the list is on screen. The status in it is never taken, however fresh the
    /// copy; a copy of another list ends the hub's word for the worlds.
    pub fn copy_shown(&mut self, copy: &ListCache) {
        self.hubs_list_shown = copy.is_hubs();
        if !self.hubs_list_shown {
            self.statuses.clear();
        }
    }

    /// A list just fetched at `now` is on screen. The hub's carries each world's status as it is
    /// now, which is taken, and counts as a read.
    pub fn list_read(&mut self, list: &ListCache, now: u64) {
        self.copy_shown(list);
        if self.hubs_list_shown && self.source.is_some() {
            if let Ok(statuses) = read_pages(&list.hub) {
                self.statuses = statuses;
                self.last_read = Some(now);
            }
        }
    }

    /// Whether a read is due at `now`: the worlds on screen are the hub's list's, no read is under
    /// way, and the last started a minute ago or more (or in the future, a clock set back).
    #[must_use]
    pub fn is_due(&self, now: u64) -> bool {
        self.source.is_some()
            && self.hubs_list_shown
            && !self.reading
            && self
                .last_read
                .is_none_or(|t| now < t || now - t >= READ_EVERY_SECS)
    }

    /// Start a read when one is due at `now`: the hub's list, to fetch a page at a time
    /// ([`ListFetch::next_url`], [`ListFetch::take`]) and hand back to [`Self::finish`].
    pub fn start(&mut self, now: u64) -> Option<ListFetch> {
        if !self.is_due(now) {
            return None;
        }
        let source = self.source.clone()?;
        self.reading = true;
        self.last_read = Some(now);
        Some(ListFetch::new(vec![source]))
    }

    /// The read [`Self::start`] began, fetched to its end, at `now`. The status it brings replaces
    /// the last; a read that failed, or that ends after another list took the hub's place on
    /// screen, changes nothing. Answers whether the read brought the hub's word.
    pub fn finish(&mut self, read: ListFetch, now: u64) -> bool {
        self.reading = false;
        if !self.hubs_list_shown {
            return false;
        }
        let Ok(statuses) = read
            .finish(now)
            .and_then(|(list, _)| read_pages(&list.hub).map_err(|e| e.to_string()))
        else {
            return false;
        };
        self.statuses = statuses;
        true
    }

    /// The hub's status for the world kept under `key`, while its heartbeat counts at `now`.
    #[must_use]
    pub fn current(&self, key: &str, now: u64) -> Option<&HubStatus> {
        self.statuses.get(key).filter(|s| s.is_current(now))
    }
}

/// The ASCII digits of `s` at `at..at + len` as a number; `None` when they are not all digits.
fn digits(s: &str, at: usize, len: usize) -> Option<i64> {
    let d = s.get(at..at + len)?;
    if d.bytes().all(|c| c.is_ascii_digit()) {
        d.parse().ok()
    } else {
        None
    }
}

/// A time as the hub writes it (RFC 3339: `2026-10-11T18:02:11.000Z`, or with an offset such as
/// `+02:00`) in whole seconds since the epoch, a fraction dropped. `None` for anything else, and
/// for a time before 1970.
fn parse_time(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || !matches!(b[10], b'T' | b't')
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let (y, mo, d) = (digits(s, 0, 4)?, digits(s, 5, 2)?, digits(s, 8, 2)?);
    let (h, mi, se) = (digits(s, 11, 2)?, digits(s, 14, 2)?, digits(s, 17, 2)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    let mut rest = s.get(19..)?;
    if let Some(fraction) = rest.strip_prefix('.') {
        let n = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if n == 0 {
            return None;
        }
        rest = fraction.get(n..)?;
    }
    let offset = match rest {
        "Z" | "z" => 0,
        _ => {
            let sign = match rest.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            if rest.len() != 6 || rest.as_bytes()[3] != b':' {
                return None;
            }
            let (oh, om) = (digits(rest, 1, 2)?, digits(rest, 4, 2)?);
            if oh > 23 || om > 59 {
                return None;
            }
            sign * (oh * 3600 + om * 60)
        }
    };
    // Days since the epoch of a civil date (proleptic Gregorian), after Howard Hinnant's
    // algorithm.
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if mo > 2 { mo - 3 } else { mo + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + h * 3600 + mi * 60 + se - offset).ok()
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the launcher's status from the hub: what counts, for which world, when).
    use super::*;
    use crate::serverlist::{sources, COMMUNITY_LIST_URL, HUB_LIST_URL};
    use serde_json::json;

    /// 2026-10-11T18:00:00Z, in seconds since the epoch.
    const NOW: u64 = 1_791_741_600;

    /// `secs` after [`NOW`] (less than six hours), as the hub writes a time.
    fn at(secs: u64) -> String {
        format!(
            "2026-10-11T{:02}:{:02}:{:02}.000Z",
            18 + secs / 3600,
            secs / 60 % 60,
            secs % 60
        )
    }

    /// A heartbeat's status: `state`, with `players` on, counting until `expires` seconds after
    /// [`NOW`].
    fn heartbeat(state: &str, players: Option<u32>, expires: u64) -> Value {
        json!({"state": state, "playersOnline": players, "playersKind": "connected-characters",
               "source": "server-heartbeat", "lastSeenAt": at(0), "expiresAt": at(expires),
               "reachability": "unknown", "message": "", "reported": null})
    }

    /// A row of the hub's list: the world `name`, listed as `slug`, with `status`.
    fn row(slug: &str, name: &str, status: &Value) -> Value {
        json!({"slug": slug, "name": name, "emulator": {"family": "ACE"}, "isDemo": false,
               "endpoint": {"mode": "manual", "address": format!("{slug}.example"), "port": 9000},
               "status": status})
    }

    /// A page of the hub's list holding `rows`, the next starting at `next`.
    fn page(rows: &[Value], next: Option<u64>) -> String {
        json!({"data": rows, "limit": 100, "offset": 0, "nextOffset": next,
               "total": rows.len()})
        .to_string()
    }

    /// The hub's list, read whole, as the day's copy.
    fn hub_copy(pages: &[String]) -> ListCache {
        ListCache {
            fetched_at: NOW,
            list: String::new(),
            hub: pages.to_vec(),
            sources: Vec::new(),
        }
    }

    #[test]
    fn a_heartbeats_status_reads_and_an_unknown_stale_or_unsourced_one_does_not() {
        let read = |s: &Value| HubStatus::read(s).map(|s| (s.state, s.players));
        assert_eq!(
            read(&heartbeat("online", Some(12), 180)),
            Some((WorldState::Online, Some(12)))
        );
        assert_eq!(
            read(&heartbeat("degraded", Some(3), 180)),
            Some((WorldState::Online, Some(3)))
        );
        assert_eq!(
            read(&heartbeat("maintenance", Some(0), 180)),
            Some((WorldState::Starting, Some(0))),
            "up and letting nobody in, as Empyrean's own word for a world not open is read"
        );
        assert_eq!(
            read(&heartbeat("offline", None, 180)),
            Some((WorldState::Offline, None))
        );
        assert_eq!(
            HubStatus::read(&heartbeat("online", None, 180)).map(|s| s.expires_at),
            Some(NOW + 180)
        );
        // What the hub says of a world that never reported, or stopped.
        for state in ["unknown", "stale", "Online", ""] {
            assert_eq!(read(&heartbeat(state, None, 180)), None, "{state}");
        }
        let unlinked = json!({"state": "unknown", "playersOnline": null, "lastSeenAt": null,
                              "expiresAt": null, "source": "server-heartbeat"});
        assert_eq!(read(&unlinked), None);
        let mut other = heartbeat("online", Some(5), 180);
        other["source"] = "operator".into();
        assert_eq!(read(&other), None, "only a heartbeat counts");
        let mut unsourced = heartbeat("online", Some(5), 180);
        unsourced.as_object_mut().unwrap().remove("source");
        assert_eq!(read(&unsourced), None);
        let mut no_expiry = heartbeat("online", Some(5), 180);
        no_expiry["expiresAt"] = "soon".into();
        assert_eq!(
            read(&no_expiry),
            None,
            "a status with no expiry could count forever"
        );
    }

    #[test]
    fn a_status_counts_until_its_heartbeat_expires_and_never_after() {
        let mut watch = HubWatch::new(&sources(None));
        let pages = [page(
            &[
                row("coldeve", "Coldeve", &heartbeat("online", Some(40), 180)),
                row("frost", "Frostfell", &heartbeat("online", Some(2), 30)),
            ],
            None,
        )];
        watch.list_read(&hub_copy(&pages), NOW);
        assert_eq!(
            watch.current("coldeve", NOW).map(|s| s.players),
            Some(Some(40))
        );
        assert!(watch.current("frostfell", NOW + 29).is_some());
        assert_eq!(
            watch.current("frostfell", NOW + 30),
            None,
            "past its expiry, as if the hub had said nothing"
        );
        assert!(watch.current("coldeve", NOW + 179).is_some());
        assert_eq!(watch.current("coldeve", NOW + 180), None);
        assert_eq!(watch.current("coldeve", NOW + 86_400), None);
    }

    #[test]
    fn a_status_goes_to_the_world_kept_under_its_name_and_namesakes_take_none() {
        let online = heartbeat("online", Some(7), 180);
        let mut unshown = row("unseen", "Not Yet Seen", &online);
        unshown["endpoint"]["address"] = Value::Null;
        let mut symbols = row("symbols", "!!!", &online);
        symbols["provenance"] = json!({"externalId": "abc-123"});
        let first = page(
            &[
                row("ushores", "Unfamiliar Shores", &online),
                row("twin", "Twin", &online),
                row("twin2", "twin", &heartbeat("offline", None, 180)),
                unshown,
                symbols,
                row(
                    "quiet",
                    "Quiet",
                    &json!({"state": "unknown", "playersOnline": null}),
                ),
                row("moved", "Moved Down", &heartbeat("online", Some(1), 180)),
            ],
            Some(100),
        );
        // The world moved down a page while the pages were read: its first sighting counts.
        let second = page(
            &[row("moved", "Moved Down", &heartbeat("offline", None, 180))],
            None,
        );
        let statuses = read_pages(&[first, second]).unwrap();
        assert_eq!(
            statuses.keys().map(String::as_str).collect::<Vec<_>>(),
            ["abc-123", "moved-down", "unfamiliar-shores"],
            "the hub's own slug is not the key; namesakes, an unshown world and one that never \
             reported give none"
        );
        assert_eq!(statuses["unfamiliar-shores"].players, Some(7));
        assert_eq!(statuses["moved-down"].state, WorldState::Online);
        assert!(read_pages(&["<html/>".to_owned()]).is_err());
    }

    #[test]
    fn a_read_brings_where_a_browser_reaches_a_linked_world_within_the_rule() {
        let online = heartbeat("online", Some(2), 180);
        let mut linked = row("linked", "Linked", &online);
        linked["websocketUrl"] = json!("wss://linked.example.org/ws");
        linked["statusUrl"] = json!("https://linked.example.org/status");
        let mut plain = row("plain", "Plain", &online);
        plain["websocketUrl"] = json!("ws://plain.example.org/ws");
        plain["statusUrl"] = json!("http://plain.example.org/status");
        let statuses =
            read_pages(&[page(&[linked, plain, row("bare", "Bare", &online)], None)]).unwrap();
        let web = |key: &str| {
            let s = &statuses[key];
            (s.websocket_url.as_deref(), s.status_url.as_deref())
        };
        assert_eq!(
            web("linked"),
            (
                Some("wss://linked.example.org/ws"),
                Some("https://linked.example.org/status")
            )
        );
        assert_eq!(web("plain"), (None, None), "plain to another machine");
        assert_eq!(web("bare"), (None, None));
    }

    #[test]
    fn a_world_renamed_or_gone_from_a_later_read_takes_nothing_from_it() {
        let mut watch = HubWatch::new(&sources(None));
        let online = heartbeat("online", Some(9), 180);
        watch.list_read(
            &hub_copy(&[page(
                &[row("a", "Asheron", &online), row("b", "Bael", &online)],
                None,
            )]),
            NOW,
        );
        let mut read = watch.start(NOW + 60).expect("due");
        read.take(Ok(
            page(&[row("a", "Asheron Reborn", &online)], None).into_bytes()
        ));
        assert!(watch.finish(read, NOW + 60));
        assert_eq!(watch.current("asheron", NOW + 60), None, "renamed");
        assert_eq!(watch.current("bael", NOW + 60), None, "gone");
        assert!(watch.current("asheron-reborn", NOW + 60).is_some());
    }

    #[test]
    fn the_hubs_list_is_read_about_once_a_minute_and_a_list_just_fetched_counts_as_a_read() {
        let mut watch = HubWatch::new(&sources(None));
        assert!(
            !watch.is_due(NOW),
            "nothing is read before the hub's list is on screen"
        );
        // The day's copy is on screen: its status is not taken, and a read is due at once.
        let copy = hub_copy(&[page(
            &[row("a", "Asheron", &heartbeat("online", Some(3), 3_000))],
            None,
        )]);
        watch.copy_shown(&copy);
        assert_eq!(watch.current("asheron", NOW), None, "never from the copy");
        assert!(watch.is_due(NOW));
        let read = watch.start(NOW).expect("due");
        assert_eq!(
            read.next_url(),
            Some("https://hub.dereth.network/v1/servers?limit=100&offset=0")
        );
        assert!(!watch.is_due(NOW + 120), "one read at a time");
        assert!(watch.start(NOW + 120).is_none());
        watch.finish(read, NOW + 2);
        assert!(!watch.is_due(NOW + 59));
        assert!(watch.is_due(NOW + 60), "a minute after the last began");
        // A list fetched (the refresh button, or the day's fetch) is a read of its own.
        watch.list_read(&copy, NOW + 100);
        assert_eq!(
            watch.current("asheron", NOW + 100).map(|s| s.players),
            Some(Some(3))
        );
        assert!(!watch.is_due(NOW + 159));
        assert!(watch.is_due(NOW + 160));
        assert!(
            watch.is_due(NOW + 99),
            "a clock set back does not stop the reads"
        );
    }

    #[test]
    fn a_status_read_pages_through_the_list_and_a_failed_one_keeps_what_was_read() {
        let mut watch = HubWatch::new(&sources(None));
        let a = heartbeat("online", Some(3), 600);
        watch.list_read(&hub_copy(&[page(&[row("a", "Asheron", &a)], None)]), NOW);

        let mut read = watch.start(NOW + 60).unwrap();
        assert_eq!(
            read.next_url(),
            Some(format!("{HUB_LIST_URL}?limit=100&offset=0").as_str())
        );
        read.take(Ok(page(
            &[row("a", "Asheron", &heartbeat("online", Some(20), 600))],
            Some(100),
        )
        .into_bytes()));
        assert_eq!(
            read.next_url(),
            Some(format!("{HUB_LIST_URL}?limit=100&offset=100").as_str()),
            "the next page"
        );
        read.take(Ok(page(
            &[row("b", "Bael", &heartbeat("maintenance", Some(0), 600))],
            None,
        )
        .into_bytes()));
        assert_eq!(read.next_url(), None);
        assert!(watch.finish(read, NOW + 61));
        assert_eq!(
            watch.current("asheron", NOW + 61).map(|s| s.players),
            Some(Some(20))
        );
        assert_eq!(
            watch.current("bael", NOW + 61).map(|s| s.state),
            Some(WorldState::Starting)
        );

        // No answer, a page that is not the list, a list that stops half way, and an empty list:
        // what was read stands, and the next read is due a minute later.
        let failures: [&[Result<String, &str>]; 4] = [
            &[Err("connection refused")],
            &[Ok("<html>Bad gateway</html>".to_owned())],
            &[
                Ok(page(
                    &[row("a", "Asheron", &heartbeat("offline", None, 600))],
                    Some(100),
                )),
                Err("timed out"),
            ],
            &[Ok(page(&[], None))],
        ];
        for (i, answers) in failures.iter().enumerate() {
            let now = NOW + 120 + 60 * i as u64;
            let mut read = watch.start(now).expect("due a minute after the last");
            let mut asked = 0;
            for answer in answers.iter() {
                assert!(read.next_url().is_some());
                asked += 1;
                read.take(
                    answer
                        .clone()
                        .map(String::into_bytes)
                        .map_err(str::to_owned),
                );
            }
            assert_eq!(read.next_url(), None, "{i}: no other source is read");
            assert_eq!(asked, answers.len());
            assert!(!watch.finish(read, now), "{i}");
            assert_eq!(
                watch.current("asheron", now).map(|s| (s.state, s.players)),
                Some((WorldState::Online, Some(20))),
                "{i}: what was read stands"
            );
            assert!(!watch.is_due(now + 59) && watch.is_due(now + 60), "{i}");
        }
    }

    #[test]
    fn no_status_comes_from_a_named_list_or_the_community_list() {
        let online = heartbeat("online", Some(4), 600);
        let pages = [page(&[row("a", "Asheron", &online)], None)];
        // A list the player names: nothing is ever read, whatever is on screen.
        let mut named = HubWatch::new(&sources(Some("https://lists.example/Servers.xml")));
        named.list_read(&hub_copy(&pages), NOW);
        assert_eq!(named.current("asheron", NOW), None);
        assert!(!named.is_due(NOW) && named.start(NOW).is_none());

        // The community's list, read when the hub did not answer, ends the hub's word.
        let mut watch = HubWatch::new(&sources(None));
        watch.list_read(&hub_copy(&pages), NOW);
        assert!(watch.current("asheron", NOW).is_some());
        let community = ListCache {
            fetched_at: NOW,
            list: "<ArrayOfServerItem><ServerItem><name>Asheron</name></ServerItem>\
                   </ArrayOfServerItem>"
                .into(),
            hub: Vec::new(),
            sources: Vec::new(),
        };
        let mut late = watch.start(NOW + 60).unwrap();
        watch.list_read(&community, NOW + 60);
        assert_eq!(watch.current("asheron", NOW + 60), None);
        assert!(
            !watch.is_due(NOW + 600),
            "no read while another list is shown"
        );
        // A read under way when the community's list took the hub's place brings nothing.
        late.take(Ok(pages[0].clone().into_bytes()));
        assert!(!watch.finish(late, NOW + 61));
        assert_eq!(watch.current("asheron", NOW + 61), None);
        watch.copy_shown(&community);
        assert!(!watch.is_due(NOW + 600));

        // A read is the hub's list alone: when it does not answer, nothing else is asked.
        let mut watch = HubWatch::new(&sources(None));
        watch.copy_shown(&hub_copy(&pages));
        let mut read = watch.start(NOW).unwrap();
        read.take(Err("connection refused".into()));
        assert_eq!(read.next_url(), None, "not {COMMUNITY_LIST_URL}");
    }

    #[test]
    fn the_hubs_times_read_as_seconds_since_the_epoch() {
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_time("2026-10-11T18:00:00.000Z"), Some(NOW));
        assert_eq!(parse_time("2026-10-11T18:00:00Z"), Some(NOW));
        assert_eq!(parse_time("2026-10-11T20:00:00.5+02:00"), Some(NOW));
        assert_eq!(parse_time("2026-10-11T13:30:00-04:30"), Some(NOW));
        assert_eq!(parse_time("2024-02-29T00:00:00Z"), Some(1_709_164_800));
        for bad in [
            "",
            "2026-10-11",
            "2026-10-11 18:00:00Z",
            "2026-13-11T18:00:00Z",
            "2026-10-11T24:00:00Z",
            "2026-10-11T18:00:00",
            "2026-10-11T18:00:00.Z",
            "2026-10-11T18:00:00+2:00",
            "+026-10-11T18:00:00Z",
            "1969-12-31T23:59:59Z",
            "2026-10-11T18:00:00Zé",
        ] {
            assert_eq!(parse_time(bad), None, "{bad:?}");
        }
    }
}
