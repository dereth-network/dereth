//! The January 2017 logins replayed: each login connection's client messages sent again, in
//! order, to the server, and its answers compared with the recorded server's.
//!
//! The account's characters are seeded from the first character list the recording holds (ids,
//! names, looks, deletions), so the recorded client's requests name characters the server knows;
//! a character the recording creates is mapped from its recorded id to the id the server gives
//! it. Messages are compared as their shape: type, queue and the fields that say what happened
//! (character ids and greyed-out marks in the lists, creation and refusal reasons, the kind and id
//! a data answer is about and whether it was served). The server serves records from the
//! install's own data files. Chat-service traffic is left out: the server does not run it yet.
//! Beside the replay, every data record the recorded clients asked for (`DatRequest` `0x10034`)
//! is asked of the server's own answer, and held to the recorded answer (`DatData` `0x10035`,
//! `DatRequestFailed` `0x10036`).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read as _;
use std::path::Path;

use auberean_protocol::login::{CharacterList, DataRecord, DataRequest};
use auberean_protocol::text::NarrowString;
use auberean_protocol::types::VisualDesc;
use auberean_protocol::wire::{Field, Writer};
use auberean_protocol::{ClientBlob, ClientMessage, ServerMessage};
use rusqlite::{Connection, OpenFlags};
use yalain_store::{Character, Store};
use yalain_testkit::Harness;
use yalain_world::download::{answers, DataFiles};

/// Steps run after each replayed message: the server answers within a few.
const QUIET: usize = 12;

const ACCOUNT: &str = "replay";

fn open(path: &Path) -> Connection {
    let uri = format!(
        "file:{}?mode=ro&immutable=1",
        path.to_string_lossy().replace('\\', "/")
    );
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_URI;
    Connection::open_with_flags(uri, flags).expect("the capture index opens read-only")
}

/// One recorded message.
struct Rec {
    /// Seconds from the session's start.
    t: f64,
    to_client: bool,
    queue: u16,
    /// The blob id it travelled under: its low half orders a queue's messages as sent.
    blob: u64,
    raw: Vec<u8>,
}

/// What a server message says, for comparing.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Plain(u16, u32),
    Names(Vec<u64>),
    List(Vec<(u64, bool)>),
    DataRefused(u32, u32),
    DataServed(u32, u32),
    Created(u32),
    Deleted(u64),
    Refused(u32),
}

fn token(queue: u16, raw: &[u8], map: &dyn Fn(u64) -> u64) -> Option<Token> {
    let m = ServerMessage::decode(raw).ok()?;
    Some(match m {
        ServerMessage::Chat(_) => return None,
        ServerMessage::CharacterNames(n) => Token::Names(n.ids.iter().map(|i| map(*i)).collect()),
        ServerMessage::CharacterList(l) => Token::List(
            l.characters
                .iter()
                .map(|c| (map(c.id), c.seconds_greyed_out > 0))
                .collect(),
        ),
        ServerMessage::DataRefused(r) => Token::DataRefused(r.kind, r.id),
        ServerMessage::DataRecord(r) => Token::DataServed(r.kind, r.id),
        ServerMessage::CharacterCreated(c) => Token::Created(c.response),
        ServerMessage::CharacterDeleted(d) => Token::Deleted(map(d.character)),
        ServerMessage::CharacterRefused(r) => Token::Refused(r.reason),
        other => Token::Plain(queue, other.message_type()),
    })
}

fn look_bytes(v: &VisualDesc) -> Vec<u8> {
    let mut w = Writer::default();
    v.write(&mut w);
    w.finish()
}

/// How one session's replay went.
#[derive(Debug, Default)]
struct Outcome {
    same: bool,
    retail_still_in_world: bool,
    server_full: bool,
    creation_not_captured: bool,
    ended_during_log_off: bool,
    referrals: (usize, usize),
    first_difference: Option<String>,
}

fn replay(con: &Connection, session: i64) -> Option<Outcome> {
    let mut stmt = con
        .prepare("SELECT t, dir, queue, blob_id, raw FROM message WHERE session_id = ?1 AND status = 'ac2' ORDER BY idx")
        .ok()?;
    let recs: Vec<Rec> = stmt
        .query_map([session], |r| {
            Ok(Rec {
                t: r.get(0)?,
                to_client: r.get::<_, String>(1)? == "s2c",
                queue: r.get(2)?,
                blob: r.get::<_, i64>(3)?.cast_unsigned(),
                raw: r.get(4)?,
            })
        })
        .ok()?
        .collect::<Result<_, _>>()
        .ok()?;
    // The referrals that answered an entry into the world: the first after each entry request.
    // Later ones on the login connection handed a player to another world server mid-play.
    let referral_times: Vec<f64> = con
        .prepare("SELECT t FROM packet WHERE session_id = ?1 AND (flags & 2048) != 0 AND duplicate = 0 ORDER BY t")
        .ok()?
        .query_map([session], |r| r.get(0))
        .ok()?
        .collect::<Result<_, _>>()
        .ok()?;
    let entries: Vec<f64> = recs
        .iter()
        .filter(|r| {
            !r.to_client
                && matches!(
                    ClientMessage::decode(&r.raw),
                    Ok(ClientMessage::PlayCharacter(_))
                )
        })
        .map(|r| r.t)
        .collect();
    let recorded_referrals = entries
        .iter()
        .enumerate()
        .filter(|(i, t)| {
            let until = entries.get(i + 1).copied().unwrap_or(f64::MAX);
            referral_times.iter().any(|r| r >= *t && *r < until)
        })
        .count();

    let mut out = Outcome::default();
    // A login the recorded server refused for want of room: nothing to replay.
    if recs.first().is_some_and(|r| {
        r.to_client
            && ServerMessage::decode(&r.raw)
                .is_ok_and(|m| matches!(m, ServerMessage::CharacterRefused(c) if c.reason == 19))
    }) {
        out.server_full = true;
        return Some(out);
    }
    let first_list: CharacterList =
        recs.iter()
            .filter(|r| r.to_client)
            .find_map(|r| match ServerMessage::decode(&r.raw) {
                Ok(ServerMessage::CharacterList(l)) => Some(l),
                _ => None,
            })?;

    let mut h = Harness::new();
    h.server.data = Some(Box::new(install_files().clone()));
    let now_unix = h.unix;
    let account = h
        .server
        .store
        .create_account(ACCOUNT, None, now_unix)
        .ok()?;
    let mut top = 0u64;
    let n = i64::try_from(first_list.characters.len()).unwrap_or(0);
    for (i, c) in first_list.characters.iter().enumerate() {
        // The recorded order is the order of last play: seed it so.
        let last_played = now_unix - n + i64::try_from(i).unwrap_or(0);
        top = top.max(c.id & 0xFFFF_FFFF);
        let deleted_at =
            (c.seconds_greyed_out > 0).then(|| now_unix - (3600 - i64::from(c.seconds_greyed_out)));
        let ch = Character {
            id: c.id,
            account: account.id,
            name: c.name.text(),
            // A list names no race: any real one enters, as the recorded characters did.
            race: 1,
            sex: yalain_world::chargen::MALE,
            class: 0,
            implement: 0,
            start_area: 0,
            entity_did: 0,
            physique: Vec::new(),
            look: look_bytes(&c.visual),
            created_at: now_unix,
            deleted_at,
            last_played,
        };
        h.server.store.insert_character(&ch).ok()?;
    }
    while h.server.store.next_character_number().ok()? <= top {}

    // Each recorded creation's recorded answer, by the creation's place among the client's
    // messages.
    let requests = recs
        .iter()
        .filter(|r| {
            !r.to_client
                && matches!(
                    ClientMessage::decode(&r.raw),
                    Ok(ClientMessage::CreateCharacter(_))
                )
        })
        .count();
    let mut created_ids: VecDeque<u64> = VecDeque::new();
    for r in recs.iter().filter(|r| r.to_client) {
        if let Ok(ServerMessage::CharacterCreated(c)) = ServerMessage::decode(&r.raw) {
            created_ids.push_back(c.identity.id);
        }
    }

    if created_ids.len() > requests {
        out.creation_not_captured = true;
        return Some(out);
    }
    let last_client = recs
        .iter()
        .rev()
        .find(|r| !r.to_client && r.queue == 4)
        .and_then(|r| ClientMessage::decode(&r.raw).ok());
    let b = h.add_bot();
    let now = h.now;
    h.bots[b].log_in(ACCOUNT, b"ticket", now);
    if !h.run_until(400, |h| {
        h.bots[b]
            .messages(false)
            .iter()
            .any(|m| matches!(m, ServerMessage::DataQuery(_)))
    }) {
        out.first_difference = Some("no data query after the login".into());
        return Some(out);
    }
    // The recording's clock: each client message goes when it went, in the virtual time.
    let start = h.now.0 - recs.first().map_or(0.0, |r| r.t);
    // Recorded id -> the server's id, for characters made during the replay.
    let mut ids: HashMap<u64, u64> = HashMap::new();
    for r in recs.iter().filter(|r| !r.to_client) {
        let bytes = match ClientBlob::decode(r.queue, &r.raw) {
            Ok(ClientBlob::Message(m)) => {
                let back = |id: u64| ids.get(&id).copied().unwrap_or(id);
                let m = match m {
                    ClientMessage::PlayCharacter(mut p) => {
                        p.character = back(p.character);
                        p.account = NarrowString::from(ACCOUNT);
                        ClientMessage::PlayCharacter(p)
                    }
                    ClientMessage::LogOff(mut l) => {
                        l.character = back(l.character);
                        ClientMessage::LogOff(l)
                    }
                    ClientMessage::DeleteCharacter(mut d) => {
                        d.character = back(d.character);
                        d.account = NarrowString::from(ACCOUNT);
                        ClientMessage::DeleteCharacter(d)
                    }
                    ClientMessage::CreateCharacter(mut c) => {
                        c.account = NarrowString::from(ACCOUNT);
                        ClientMessage::CreateCharacter(c)
                    }
                    other => other,
                };
                m.encode()
            }
            _ => r.raw.clone(),
        };
        let creating = matches!(
            ClientMessage::decode(&bytes),
            Ok(ClientMessage::CreateCharacter(_))
        );
        h.run_to(yalain_net::LocalTime(start + r.t));
        h.bots[b].send_bytes(false, r.queue, bytes);
        h.run(QUIET);
        if creating {
            let ours = h.bots[b]
                .messages(false)
                .into_iter()
                .rev()
                .find_map(|m| match m {
                    ServerMessage::CharacterCreated(c) => Some(c.identity.id),
                    _ => None,
                });
            if let (Some(ours), Some(recorded)) = (ours, created_ids.pop_front()) {
                ids.insert(recorded, ours);
            }
        }
    }

    // A log-off last in the recording: its character's leaving takes this long.
    h.run_to(yalain_net::LocalTime(h.now.0 + 25.0));

    let forward: HashMap<u64, u64> = ids.clone();
    let map = move |id: u64| forward.get(&id).copied().unwrap_or(id);
    // Each queue in its own order (the transport keeps a queue's order; across queues it does not).
    let recorded: Vec<&Rec> = recs.iter().filter(|r| r.to_client).collect();
    let ours = h.bots[b]
        .inbox
        .iter()
        .filter(|r| !r.world)
        .cloned()
        .collect::<Vec<_>>();
    let mut recorded_events: Vec<&&Rec> = recorded.iter().filter(|r| r.queue == 1).collect();
    recorded_events.sort_by_key(|r| r.blob & 0xFFFF_FFFF);
    let want: Vec<Token> = recorded_events
        .iter()
        .filter_map(|r| token(r.queue, &r.raw, &map))
        .collect();
    let got: Vec<Token> = ours
        .iter()
        .filter(|r| r.queue == 1)
        .filter_map(|r| token(r.queue, &r.bytes, &|i| i))
        .collect();
    // Data: every record served or refused as recorded. (The recorded server answered some out
    // of order.)
    let data = |t: &Token| matches!(t, Token::DataRefused(..) | Token::DataServed(..));
    let mut want_data: Vec<Token> = recorded
        .iter()
        .filter(|r| r.queue == 5)
        .filter_map(|r| token(r.queue, &r.raw, &map))
        .filter(data)
        .collect();
    let mut got_data: Vec<Token> = ours
        .iter()
        .filter(|r| r.queue == 5)
        .filter_map(|r| token(r.queue, &r.bytes, &|i| i))
        .filter(data)
        .collect();
    want_data.sort_by_key(|t| format!("{t:?}"));
    got_data.sort_by_key(|t| format!("{t:?}"));
    out.retail_still_in_world = want.contains(&Token::Refused(11));
    out.referrals = (recorded_referrals, h.bots[b].referrals.len());
    out.same = want == got && want_data == got_data;
    // The recording ended before the recorded server answered the client's last log-off (it
    // answered some seconds later): ours ends with that answer.
    let exit_tail = [Token::Plain(1, 0x10027)];
    out.ended_during_log_off = !out.same
        && want_data == got_data
        && matches!(last_client, Some(ClientMessage::LogOff(_)))
        && got.len() == want.len() + 3
        && got[..want.len()] == want[..]
        && got[want.len()..want.len() + 1] == exit_tail;
    if !out.same {
        let (want, got) = if want == got {
            (&want_data, &got_data)
        } else {
            (&want, &got)
        };
        let at = want
            .iter()
            .zip(got)
            .position(|(a, b)| a != b)
            .unwrap_or(want.len().min(got.len()));
        out.first_difference = Some(format!(
            "at {at} of {} recorded / {} ours: recorded {:?}, ours {:?}",
            want.len(),
            got.len(),
            want.get(at),
            got.get(at)
        ));
    }
    Some(out)
}

use std::collections::VecDeque;
use std::sync::OnceLock;

/// The install's data files, opened once for every replayed login.
fn install_files() -> &'static yalain_world::download::InstallFiles {
    static F: OnceLock<yalain_world::download::InstallFiles> = OnceLock::new();
    F.get_or_init(yalain_testkit::authored::install_files)
}

/// Every January 2017 login connection, replayed: the server answers each recorded login with the
/// recorded messages' shape, every data record served or refused as recorded. The differences allowed are named: the recorded login server still
/// held a character in the world for a moment after it left (its refusals with reason 11, which
/// this server, letting go at once, does not repeat), and logins the recorded server refused for
/// want of room.
/// Behaviour: server.replay.recorded-logins-answered-as-recorded
#[test]
fn every_recorded_2017_login_is_answered_as_recorded() {
    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let sessions: Vec<(i64, String)> = con
        .prepare(
            "SELECT s.session_id, s.session_key FROM session s JOIN capture c USING (capture_id) \
             WHERE s.game = 'ac2' AND s.login = 1 AND c.file LIKE '2017-%' ORDER BY s.session_id",
        )
        .expect("prepares")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("runs")
        .collect::<Result<_, _>>()
        .expect("rows");
    let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
    let mut unexplained = Vec::new();
    let (mut referrals_recorded, mut referrals_ours) = (0, 0);
    for (sid, key) in &sessions {
        let Some(o) = replay(&con, *sid) else {
            *tally
                .entry("not replayable (no character list recorded)")
                .or_default() += 1;
            continue;
        };
        if o.server_full {
            *tally
                .entry("refused for want of room by the recorded server")
                .or_default() += 1;
        } else if o.creation_not_captured {
            *tally
                .entry("not replayable (a creation request missing from the capture)")
                .or_default() += 1;
        } else if o.ended_during_log_off {
            *tally
                .entry("the recording ends before the answer to its last log-off")
                .or_default() += 1;
            referrals_recorded += o.referrals.0;
            referrals_ours += o.referrals.1;
        } else if o.same {
            *tally.entry("the recorded shape").or_default() += 1;
            referrals_recorded += o.referrals.0;
            referrals_ours += o.referrals.1;
            if o.referrals.0 != o.referrals.1 {
                println!(
                    "  referrals differ in {key}: recorded {}, ours {}",
                    o.referrals.0, o.referrals.1
                );
            }
        } else if o.retail_still_in_world {
            *tally
                .entry("differs where the recorded server still held the character in the world")
                .or_default() += 1;
        } else {
            *tally.entry("differs, unexplained").or_default() += 1;
            unexplained.push(format!("{key}: {}", o.first_difference.unwrap_or_default()));
        }
    }
    println!("{} January 2017 logins replayed:", sessions.len());
    for (k, n) in &tally {
        println!("  {n:4}  {k}");
    }
    println!("referrals in the sessions of the recorded shape: recorded {referrals_recorded}, ours {referrals_ours}");
    for u in &unexplained {
        println!("  unexplained: {u}");
    }
    assert!(!sessions.is_empty());
    assert!(
        unexplained.is_empty(),
        "{} logins differ unexplained",
        unexplained.len()
    );
    assert_eq!(referrals_recorded, referrals_ours);
}

/// A data answer's record as the client stores it: inflated when it came compressed.
fn record(r: &DataRecord) -> Vec<u8> {
    if r.size == 0 {
        return r.bytes.clone();
    }
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(&r.bytes[..])
        .read_to_end(&mut out)
        .expect("a compressed record inflates");
    assert_eq!(
        out.len(),
        r.size as usize,
        "{:08X} inflates to its size",
        r.id
    );
    out
}

/// What a recorded or computed answer said about one record.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Answer {
    Refused(u32),
    Served { dat_file: u64, record: Vec<u8> },
}

fn answer_of(m: &ServerMessage) -> Option<((u32, u32), Answer)> {
    match m {
        ServerMessage::DataRefused(r) => Some(((r.kind, r.id), Answer::Refused(r.error))),
        ServerMessage::DataRecord(r) => Some((
            (r.kind, r.id),
            Answer::Served {
                dat_file: r.dat_file,
                record: record(r),
            },
        )),
        _ => None,
    }
}

/// Every record a 2017 client asked for is answered as the 2017 server answered it: served from
/// the same data file with the same bytes when the server's files hold it, refused with error 1
/// when they do not; and a landblock brings the landblock information the 2017 server sent with
/// it, unasked.
/// Behaviour: server.data.recorded-requests-answered-as-2017
#[test]
fn every_recorded_2017_data_request_is_answered_as_the_recorded_server_answered_it() {
    let con = open(&auberean_testkit::data::require_capture_index());
    let files = yalain_testkit::authored::install_files();
    let mut stmt = con
        .prepare(
            "SELECT m.dir, m.raw FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) \
             WHERE m.mtype IN (65588, 65589, 65590) AND c.file LIKE '2017-%' \
             ORDER BY m.session_id, m.idx",
        )
        .expect("prepares");
    let rows: Vec<(String, Vec<u8>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("runs")
        .collect::<Result<_, _>>()
        .expect("rows");
    let mut asked: BTreeSet<(u32, u32)> = BTreeSet::new();
    let mut answers_recorded: BTreeMap<(u32, u32), Answer> = BTreeMap::new();
    let mut requests = 0usize;
    // Whether the recorded server sent each record compressed.
    let mut compressed: BTreeMap<(u32, u32), bool> = BTreeMap::new();
    for (dir, raw) in &rows {
        if dir == "c2s" {
            if let Ok(ClientMessage::DataRequest(r)) = ClientMessage::decode(raw) {
                asked.insert((r.kind, r.id));
                requests += 1;
            }
            continue;
        }
        let m = ServerMessage::decode(raw).expect("every recorded data answer decodes");
        if let ServerMessage::DataRecord(r) = &m {
            compressed.insert((r.kind, r.id), r.size != 0);
        }
        let (key, a) = answer_of(&m).expect("a data answer");
        if let Some(before) = answers_recorded.insert(key, a.clone()) {
            assert_eq!(
                before, a,
                "{:08X} {:08X}: the recorded server answered it one way",
                key.0, key.1
            );
        }
    }
    assert!(requests > 0, "the index holds the 2017 data requests");

    let files: &dyn DataFiles = &files;
    let (mut served, mut refused, mut followed, mut same_form) = (0, 0, 0, 0);
    for &(kind, id) in &asked {
        let want = answers_recorded
            .get(&(kind, id))
            .unwrap_or_else(|| panic!("{kind:08X} {id:08X}: the recorded server answered it"));
        let ours = answers(Some(files), DataRequest { kind, id });
        if let ServerMessage::DataRecord(r) = &ours[0] {
            same_form += usize::from(compressed.get(&(kind, id)) == Some(&(r.size != 0)));
        }
        let (key, first) = answer_of(&ours[0]).expect("a data answer");
        assert_eq!(key, (kind, id));
        assert_eq!(&first, want, "{kind:08X} {id:08X}");
        match want {
            Answer::Refused(_) => refused += 1,
            Answer::Served { .. } => served += 1,
        }
        for extra in &ours[1..] {
            let (key, a) = answer_of(extra).expect("a data answer");
            assert!(!asked.contains(&key), "{:08X} was asked for itself", key.1);
            assert_eq!(
                answers_recorded.get(&key),
                Some(&a),
                "{:08X} {:08X}: sent unasked with its landblock as recorded",
                key.0,
                key.1
            );
            followed += 1;
        }
    }
    // Every record the recorded server sent unasked came with a landblock asked for.
    let unasked = answers_recorded
        .keys()
        .filter(|k| !asked.contains(k))
        .count();
    assert_eq!(
        unasked, followed,
        "every unasked record is a landblock's information"
    );
    println!(
        "{requests} recorded requests for {} records: {served} served, {refused} refused, \
         {followed} sent unasked with their landblocks, each as recorded; {same_form} of the \
         {served} served compressed or not as recorded",
        asked.len()
    );
}

struct Dice(u64);

impl Dice {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n.max(1) as u64)).unwrap_or(0)
    }
}

/// A few recorded messages of each kind the 2017 clients sent: for a call, of each function.
fn recorded_shapes() -> Vec<(u16, Vec<u8>)> {
    let con = open(&auberean_testkit::data::require_capture_index());
    let mut st = con
        .prepare(
            "SELECT m.queue, m.mtype, m.raw FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) WHERE m.status = 'ac2' AND m.dir = 'c2s' \
             AND c.file LIKE '2017-%' ORDER BY m.message_id",
        )
        .expect("prepares");
    let mut seen: BTreeMap<(i64, u32), u32> = BTreeMap::new();
    let mut out = Vec::new();
    let mut rows = st.query([]).expect("runs");
    while let Some(r) = rows.next().expect("a row") {
        let (queue, mtype, raw): (i64, i64, Vec<u8>) = (
            r.get(0).expect("q"),
            r.get(1).expect("t"),
            r.get(2).expect("raw"),
        );
        let f = raw
            .get(4..8)
            .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let n = seen.entry((mtype, f)).or_default();
        *n += 1;
        if *n <= 4 {
            out.push((u16::try_from(queue).unwrap_or(0), raw));
        }
    }
    out
}

fn bite(d: &mut Dice, bytes: &mut Vec<u8>) {
    match d.below(4) {
        0 => {
            let cut = d.below(bytes.len() + 1);
            bytes.truncate(cut);
        }
        1 => {
            for _ in 0..1 + d.below(5) {
                let at = d.below(bytes.len().max(1));
                if let Some(b) = bytes.get_mut(at) {
                    *b ^= 1 << d.below(8);
                }
            }
        }
        2 => {
            let at = d.below(bytes.len().max(1));
            for k in 0..4 {
                if let Some(b) = bytes.get_mut(at + k) {
                    *b = 0xFF;
                }
            }
        }
        _ => {
            for _ in 0..d.below(32) {
                #[allow(clippy::cast_possible_truncation)]
                bytes.push(d.next() as u8);
            }
        }
    }
}

/// Bitten recorded messages read as an error or as something: never a panic, or a hang, in the
/// parser or in the world.
/// Behaviour: server.lying-client.recorded-messages-bitten-do-no-harm
#[test]
fn every_kind_of_recorded_message_bitten_panics_nothing() {
    let shapes = recorded_shapes();
    assert!(shapes.len() > 100, "{} kinds of message", shapes.len());
    let mut d = Dice(0x5EED_5000);
    // The parser alone, many times over.
    let mut read = 0usize;
    for _ in 0..40 {
        for (queue, raw) in &shapes {
            let mut b = raw.clone();
            bite(&mut d, &mut b);
            if let Ok(auberean_protocol::ClientBlob::Message(
                auberean_protocol::ClientMessage::ServerCall(c),
            )) = auberean_protocol::ClientBlob::decode(*queue, &b)
            {
                read += 1;
                let _ = yalain_world::calls::values(&c.args.0);
                let _ = yalain_world::uploads::values(&c.args.0);
            }
        }
    }
    assert!(read > 1000, "{read} bitten calls still read as calls");
    // And into a world, a player in it.
    let mut h = Harness::new();
    let (a, me) = h.player_in_world("one", "Liar");
    for (queue, raw) in shapes.iter().cycle().take(6000) {
        let mut b = raw.clone();
        bite(&mut d, &mut b);
        h.bots[a].send_bytes(true, *queue, b);
        h.run(1);
    }
    let t = h.now.0 + 30.0;
    h.run_to(yalain_net::LocalTime(t));
    assert!(h.server.players.len() <= 1, "{me:#x} is one player");
}

/// Every recorded client's messages, by when each was sent, fall inside a limit a tenth of the
/// server's: the limit is well above anything recorded.
/// Behaviour: server.operator.a-flood-of-calls-is-dropped
#[test]
fn no_recorded_2017_client_comes_near_the_limit_on_calls() {
    use yalain_net::LocalTime;
    use yalain_world::call_limit::{Bucket, BURST, PER_SECOND};
    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let mut stmt = con
        .prepare(
            "SELECT m.session_id, m.t FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) \
             WHERE m.status = 'ac2' AND m.dir = 'c2s' AND c.file LIKE '2017-%' \
             ORDER BY m.session_id, m.t",
        )
        .expect("prepares");
    let mut rows = stmt.query([]).expect("runs");
    let (mut session, mut bucket, mut messages, mut sessions) = (None, None, 0u64, 0u64);
    let mut refused = 0u64;
    while let Some(r) = rows.next().expect("a row") {
        let id: i64 = r.get(0).expect("a session");
        let t = LocalTime(r.get(1).expect("a time"));
        if session != Some(id) {
            session = Some(id);
            sessions += 1;
            bucket = Some(Bucket::with(BURST / 10.0, PER_SECOND / 10.0, t));
        }
        messages += 1;
        if !bucket.as_mut().expect("begun").allow(t) {
            refused += 1;
        }
    }
    println!(
        "{messages} client messages of {sessions} connections; {refused} past a tenth of the limit"
    );
    assert!(sessions > 1000 && messages > 1_000_000);
    assert_eq!(refused, 0);
}
