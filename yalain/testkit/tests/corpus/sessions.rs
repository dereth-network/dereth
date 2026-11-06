//! The January 2017 world sessions replayed whole against the server, for the families of moving
//! and seeing others: positions, head turns and looks, objects asked for again, selection,
//! examination, chat, the allegiance query, using objects.
//!
//! Each recorded world connection that made a player is replayed: the world holds every object the
//! recording made for the client (under its recorded id and template, where the recording first
//! placed it, drawn as recorded), the character is seeded from the recording, a bot logs in, enters
//! and sends the recorded client's world messages again in order at their recorded offsets (its
//! reported places carrying the teleport and instance stamps this server gave it, as the client
//! would). Then:
//!
//! - the server's messages keep the known-object and stamp invariants (no breach allowed);
//! - each request of these families is paired with its answer within a few seconds, in the
//!   recording and in the replay, and the outcomes are tallied: answered by both (with the
//!   answer's shape compared where the family has one), by one only (each such row named with its
//!   reason), by neither;
//! - every kind of message the server sent (its type; a script call's function and the kinds of
//!   its values; a quality's key) is tallied beside the recordings' own, as a census of what the
//!   recorded servers sent that this one does not, or sends in another shape.

use std::collections::BTreeMap;
use std::path::Path;

use auberean_protocol::login::{DataHeld, PlayCharacter};
use auberean_protocol::text::NarrowString;
use auberean_protocol::types::VisualDesc;
use auberean_protocol::wire::{Field, Writer};
use auberean_protocol::{queue, ClientBlob, ClientMessage, ServerMessage};
use rusqlite::{Connection, OpenFlags};
use yalain_content::{ContentObject, ContentPack, Description, Kind};
use yalain_store::{Character, Store};
use yalain_testkit::invariants::Checker;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};

const STEPS: usize = 400;

/// How long after a request its answer is looked for, in seconds.
const WINDOW: f64 = 3.0;

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

struct Rec {
    t: f64,
    to_client: bool,
    queue: u16,
    raw: Vec<u8>,
}

fn look_bytes(v: &VisualDesc) -> Vec<u8> {
    let mut w = Writer::default();
    v.write(&mut w);
    w.finish()
}

/// A request of one of the families, as the client sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Request {
    ForceCreate(u64),
    Select(u64),
    Examine(Option<u64>),
    Say,
    Tell,
    Emote(u32),
    AllegianceQuery,
    Use(u64),
    LookAt,
    Head,
    Report,
}

impl Request {
    fn family(&self) -> &'static str {
        match self {
            Self::ForceCreate(_) => "asked for an object again",
            Self::Select(_) => "selection",
            Self::Examine(Some(_)) => "examination",
            Self::Examine(None) => "examination: stop (or a kind the client answers itself)",
            Self::Say => "say",
            Self::Tell => "tell",
            Self::Emote(_) => "emote",
            Self::AllegianceQuery => "allegiance query",
            Self::Use(_) => "use an object",
            Self::LookAt => "look at",
            Self::Head => "head turn",
            Self::Report => "reported place",
        }
    }
}

/// The 64-bit value in a call's values (an object id).
fn long_in(v: &[Value]) -> Option<u64> {
    v.iter().find_map(|x| match x {
        Value::Long(l) => Some(*l),
        _ => None,
    })
}

/// The target of an examination request's packed form, when it names one (an object, or a target
/// to attack).
fn examined(v: &[Value]) -> Option<u64> {
    v.iter().find_map(|x| match x.root() {
        Some(Node::Native { ty: 154, bytes }) if bytes.len() >= 16 => {
            let w = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap_or_default());
            (w(0) == 0x4000_0002 || w(0) == 0x4000_000D)
                .then(|| u64::from(w(12)) << 32 | u64::from(w(8)))
        }
        _ => None,
    })
}

/// The 64-bit slot values of a script object (object ids among them).
fn object_ids_in(v: &[Value]) -> Vec<u64> {
    let mut out = Vec::new();
    for x in v {
        if let Value::Graph(g) = x {
            for (_, n) in g {
                if let Node::Script { slots, .. } = n {
                    for (tag, val) in calls::joined(slots) {
                        if tag == 0x82 && val >> 56 != 0 {
                            out.push(val);
                        }
                    }
                }
            }
        }
    }
    out
}

fn request(m: &ClientMessage) -> Option<Request> {
    Some(match m {
        ClientMessage::ForceCreate(t) => Request::ForceCreate(t.0),
        ClientMessage::LookAt(_) => Request::LookAt,
        ClientMessage::TurnHead(_) => Request::Head,
        ClientMessage::Position(_) => Request::Report,
        ClientMessage::ServerCall(c) => {
            let v = calls::values(&c.args.0).unwrap_or_default();
            match c.function.0 {
                0x8212_0016 => Request::Select(long_in(&v).unwrap_or(0)),
                0x820A_0007 => Request::Examine(examined(&v)),
                0x8205_0006 => Request::Say,
                0x8205_0002 => Request::Tell,
                0x8205_000B => Request::Emote(match v.first() {
                    Some(Value::Int(i)) => *i,
                    _ => 0,
                }),
                0x8203_0005 => Request::AllegianceQuery,
                0x8216_0016 => Request::Use(object_ids_in(&v).first().copied().unwrap_or(0)),
                _ => return None,
            }
        }
        _ => return None,
    })
}

/// The script call a server message carries: its function and values.
fn call_of(m: &ServerMessage) -> Option<(u32, Option<u64>, Vec<Value>)> {
    match m {
        ServerMessage::PlayerCall(c) | ServerMessage::PlayerCallAlternate(c) => Some((
            c.function.0,
            None,
            c.args
                .as_ref()
                .and_then(|a| calls::values(&a.0).ok())
                .unwrap_or_default(),
        )),
        ServerMessage::CellObjectCall(c) | ServerMessage::VisualObjectCall(c) => Some((
            c.call.function.0,
            Some(c.object.id),
            c.call
                .args
                .as_ref()
                .and_then(|a| calls::values(&a.0).ok())
                .unwrap_or_default(),
        )),
        _ => None,
    }
}

/// Is a server message the answer to a request? (The player is `me`.)
fn answers(r: &Request, m: &ServerMessage, me: u64) -> bool {
    let call = call_of(m);
    let f = call.as_ref().map(|c| c.0);
    match r {
        Request::ForceCreate(id) => matches!(m, ServerMessage::CreateObject(o) if o.id == *id),
        Request::Select(id) => {
            f == Some(0x8212_002C) && call.as_ref().and_then(|c| long_in(&c.2)) == Some(*id)
        }
        Request::Examine(_) => f == Some(0x820A_0009),
        Request::Say => f == Some(0x8205_0016) && call.as_ref().and_then(|c| c.1) == Some(me),
        Request::Tell => matches!(f, Some(0x8205_0017 | 0x8205_0011)),
        Request::Emote(_) => {
            matches!(m, ServerMessage::PlayBehavior(e) if e.sender.id == me)
                || matches!(f, Some(0x8205_0013 | 0x8205_001F))
        }
        Request::AllegianceQuery => matches!(f, Some(0x8203_002E | 0x8203_0030)),
        Request::Use(_) => f == Some(0x8216_0021),
        Request::LookAt => matches!(m, ServerMessage::LookAt(e) if e.sender.id == me),
        Request::Head => matches!(m, ServerMessage::TurnHead(e) if e.sender.id == me),
        Request::Report => {
            matches!(m, ServerMessage::Position(p) | ServerMessage::PositionCell(p) if p.sender.id == me)
        }
    }
}

/// The world the recording saw: every object it made for the client but players, as content
/// under its recorded id, where it was first placed, drawn as recorded.
fn world_of(recorded: &[ServerMessage]) -> ContentPack {
    let mut seen = std::collections::BTreeSet::new();
    let mut pack = ContentPack::default();
    for m in recorded {
        let ServerMessage::CreateObject(o) = m else {
            continue;
        };
        if o.id >> 48 == 0x2130 || o.physics.bits & 0x8000 == 0 || !seen.insert(o.id) {
            continue;
        }
        let w = &o.weenie;
        let p = &o.physics.position;
        let numbers: [i32; 6] = w.numbers[5..11].try_into().unwrap_or_default();
        pack.objects.push(ContentObject {
            object: Some(o.id),
            template: w.entity_did,
            description: Description {
                name_text: None,
                package: w.package_id,
                look: o.visual.parent,
                enter_fx: 0,
                scale: (w.mask & 0x80 != 0).then_some(w.scale),
                name: (w.mask & 0x4 != 0 && w.name.literal.is_none())
                    .then_some((w.name.string_id, w.name.table_id)),
                numbers,
                source: "as recorded".into(),
            },
            cell: p.cell,
            origin: [p.frame.origin.x, p.frame.origin.y, p.frame.origin.z],
            rotation: [
                p.frame.rotation.w,
                p.frame.rotation.x,
                p.frame.rotation.y,
                p.frame.rotation.z,
            ],
            kind: if o.id >> 56 == 0x40 {
                Kind::Static
            } else {
                Kind::Creature
            },
            label: String::new(),
            placed: "as recorded".into(),
            destination: None,
            facts: yalain_content::Facts::default(),
            walks: None,
            gate: None,
        });
    }
    pack
}

/// A message's kind, for the census: its type, with a script call's function or a quality's key.
fn kind_of(m: &ServerMessage) -> String {
    let t = m.message_type();
    if let Some((f, _, _)) = call_of(m) {
        return format!("{t:#07X} call {f:08X}");
    }
    let key = match m {
        ServerMessage::UpdateInt(q) | ServerMessage::UpdateBool(q) => Some(q.quality),
        ServerMessage::UpdateIntVisual(q) | ServerMessage::UpdateBoolVisual(q) => Some(q.quality),
        ServerMessage::UpdateFloat(q) => Some(q.quality),
        ServerMessage::UpdateFloatVisual(q) => Some(q.quality),
        ServerMessage::UpdateLongInt(q) | ServerMessage::UpdateInstanceId(q) => Some(q.quality),
        ServerMessage::UpdateLongIntVisual(q) | ServerMessage::UpdateInstanceIdVisual(q) => {
            Some(q.quality)
        }
        ServerMessage::UpdateTimestamp(q) => Some(q.quality),
        ServerMessage::UpdateTimestampVisual(q) => Some(q.quality),
        ServerMessage::UpdateStringInfo(q) => Some(q.quality),
        ServerMessage::UpdateStringInfoVisual(q) => Some(q.quality),
        ServerMessage::UpdateDataId(q) => Some(q.quality),
        ServerMessage::UpdateDataIdVisual(q) => Some(q.quality),
        _ => None,
    };
    key.map_or_else(|| format!("{t:#07X}"), |k| format!("{t:#07X} quality {k}"))
}

/// A script call's shape: the kinds of its values, an object value by its root.
fn shape_of(m: &ServerMessage) -> String {
    let Some((_, _, values)) = call_of(m) else {
        return String::new();
    };
    values
        .iter()
        .map(|v| match v {
            Value::Int(_) => "i".to_string(),
            Value::Long(_) => "l".to_string(),
            Value::Float(_) => "f".to_string(),
            Value::LongFloat(_) => "d".to_string(),
            Value::Graph(_) => match v.root() {
                Some(Node::Script { package, .. }) => format!("s{package:X}"),
                Some(Node::Text(_)) => "t".to_string(),
                Some(Node::TextInfo(_)) => "x".to_string(),
                Some(Node::Native { ty, .. }) => format!("n{ty:X}"),
                Some(Node::Other | Node::Singleton(_)) | None => "o".to_string(),
            },
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// The census of message kinds: per kind, per shape, how many messages.
type Census = BTreeMap<String, BTreeMap<String, usize>>;

fn count(census: &mut Census, m: &ServerMessage) {
    *census
        .entry(kind_of(m))
        .or_default()
        .entry(shape_of(m))
        .or_default() += 1;
}

#[derive(Debug, Default)]
struct Outcome {
    /// Per family and outcome, how many requests.
    tally: BTreeMap<(String, String), usize>,
    checker: Checker,
    note: Option<String>,
    /// What the server counted as not handled.
    unhandled: BTreeMap<String, u64>,
    /// The kinds of message the recording's server sent, and this server.
    recorded: Census,
    ours: Census,
}

fn replay(con: &Connection, session: i64) -> Option<Outcome> {
    let recs: Vec<Rec> = con
        .prepare("SELECT t, dir, queue, raw FROM message WHERE session_id = ?1 AND status = 'ac2' ORDER BY idx")
        .ok()?
        .query_map([session], |r| {
            Ok(Rec {
                t: r.get(0)?,
                to_client: r.get::<_, String>(1)? == "s2c",
                queue: r.get(2)?,
                raw: r.get(3)?,
            })
        })
        .ok()?
        .collect::<Result<_, _>>()
        .ok()?;
    let recorded: Vec<(f64, ServerMessage)> = recs
        .iter()
        .filter(|r| r.to_client)
        .filter_map(|r| Some((r.t, ServerMessage::decode(&r.raw).ok()?)))
        .collect();
    let id = recorded.iter().find_map(|(_, m)| match m {
        ServerMessage::PlayerObject(p) => Some(p.id),
        _ => None,
    })?;
    let object = recorded.iter().find_map(|(_, m)| match m {
        ServerMessage::CreateObject(o) if o.id == id => Some(o.clone()),
        _ => None,
    })?;
    let (race, sex) = recorded
        .iter()
        .find_map(|(_, m)| match m {
            ServerMessage::PlayerQualities(q) => {
                Some((q.0.ints.get(290).copied()?, q.0.ints.get(291).copied()?))
            }
            _ => None,
        })
        .unwrap_or((1, 0x1000));
    let name = object
        .weenie
        .name
        .literal
        .as_ref()
        .map(auberean_protocol::text::WideString::text)
        .unwrap_or_default();
    let name = name.split(" [").next().unwrap_or_default().to_string();
    let mut out = Outcome::default();

    let mut h = Harness::new();
    h.server.content = world_of(&recorded.iter().map(|(_, m)| m.clone()).collect::<Vec<_>>());
    let account = h.server.store.create_account("replay", None, h.unix).ok()?;
    #[allow(clippy::cast_sign_loss)]
    let ch = Character {
        id,
        account: account.id,
        name,
        race: race as u32,
        sex: sex as u32,
        class: 0,
        implement: 0,
        start_area: 0,
        entity_did: 0,
        physique: Vec::new(),
        look: look_bytes(&object.visual),
        created_at: h.unix,
        deleted_at: None,
        last_played: h.unix,
    };
    h.server.store.insert_character(&ch).ok()?;
    // Enter where the recording placed the player.
    let at = object.physics.position;
    let mut st = h
        .server
        .store
        .world_state(id)
        .ok()
        .flatten()
        .unwrap_or_default();
    st.position = Some(yalain_store::StoredPosition {
        cell: at.cell,
        origin: [at.frame.origin.x, at.frame.origin.y, at.frame.origin.z],
        rotation: [
            at.frame.rotation.w,
            at.frame.rotation.x,
            at.frame.rotation.y,
            at.frame.rotation.z,
        ],
    });
    h.server.store.set_world_state(id, &st).ok()?;
    let b = h.add_bot();
    let now = h.now;
    h.bots[b].log_in("replay", b"ticket", now);
    if !h.run_until(STEPS, |h| {
        h.bots[b]
            .messages(false)
            .iter()
            .any(|m| matches!(m, ServerMessage::DataQuery(_)))
    }) {
        out.note = Some("no data query".into());
        return Some(out);
    }
    h.bots[b].send(
        false,
        queue::DATABASE,
        &ClientMessage::DataHeld(DataHeld {
            language: 0x8000_0001,
            files: Vec::new(),
        }),
    );
    h.run(10);
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::PlayCharacter(PlayCharacter {
            character: id,
            account: NarrowString::from("replay"),
        }),
    );
    if !h.run_until(STEPS, |h| {
        h.bots[b]
            .messages(true)
            .iter()
            .any(|m| matches!(m, ServerMessage::PlayerObject(_)))
    }) {
        out.note = Some("no player made".into());
        return Some(out);
    }
    h.run(20);
    let world_up = h.now.0;
    let first_t = recs.first().map_or(0.0, |r| r.t);
    // Our messages, each with its time since the world came up.
    let mut seen_upto = 0usize;
    let mut ours: Vec<(f64, ServerMessage)> = Vec::new();
    let mut requests: Vec<(f64, f64, Request)> = Vec::new();
    let collect = |h: &Harness,
                   ours: &mut Vec<(f64, ServerMessage)>,
                   upto: &mut usize,
                   stamps: &mut (u16, u16)| {
        for r in &h.bots[b].inbox[*upto..] {
            if !r.world {
                continue;
            }
            let Some(m) = r.message() else { continue };
            match &m {
                ServerMessage::Position(p) if p.sender.id == id => {
                    *stamps = (p.teleport_stamp, p.sender.instance_stamp);
                }
                ServerMessage::CreateObject(c) if c.id == id => {
                    *stamps = (c.physics.stamps[2], c.physics.instance_stamp);
                }
                _ => {}
            }
            ours.push((h.now.0 - world_up, m));
        }
        *upto = h.bots[b].inbox.len();
    };
    let mut stamps = (0u16, 1u16);
    for r in recs.iter().filter(|r| !r.to_client) {
        let Ok(ClientBlob::Message(mut m)) = ClientBlob::decode(r.queue, &r.raw) else {
            continue;
        };
        if !yalain_world::world::is_world_message(&m) {
            continue;
        }
        h.run_to(yalain_net::LocalTime(world_up + (r.t - first_t)));
        collect(&h, &mut ours, &mut seen_upto, &mut stamps);
        if let ClientMessage::Position(p) = &mut m {
            (p.teleport_stamp, p.instance_stamp) = stamps;
        }
        if let Some(q) = request(&m) {
            requests.push((r.t - first_t, h.now.0 - world_up, q));
        }
        h.bots[b].send(true, r.queue, &m);
        h.run(3);
        // Our answers carry the time they arrived, not the time of the next request.
        collect(&h, &mut ours, &mut seen_upto, &mut stamps);
    }
    h.run(60);
    collect(&h, &mut ours, &mut seen_upto, &mut stamps);
    for (_, m) in &ours {
        out.checker.see(m);
        count(&mut out.ours, m);
    }
    for (_, m) in &recorded {
        count(&mut out.recorded, m);
    }
    out.unhandled.clone_from(h.server.unhandled.counts());
    // Whether this server's world holds a request's object: a player of the world (only the
    // replayed one here) or one of the objects the recording made.
    let holds = |id: u64| {
        id == 0
            || id == object.id
            || h.server
                .content
                .objects
                .iter()
                .any(|o| o.object == Some(id))
    };
    let rec_rel: Vec<(f64, &ServerMessage)> =
        recorded.iter().map(|(t, m)| (t - first_t, m)).collect();
    for (rt, ot, q) in &requests {
        let rec = rec_rel
            .iter()
            .filter(|(t, _)| *t >= *rt && *t <= rt + WINDOW)
            .any(|(_, m)| answers(q, m, id));
        let our = ours
            .iter()
            .filter(|(t, _)| *t >= *ot && *t <= ot + WINDOW)
            .any(|(_, m)| answers(q, m, id));
        let target = match q {
            Request::ForceCreate(id) | Request::Select(id) | Request::Use(id) => Some(*id),
            Request::Examine(id) => *id,
            _ => None,
        };
        let held = target.is_none_or(&holds);
        let placed_static = target.is_some_and(|id| {
            h.server
                .content
                .objects
                .iter()
                .any(|o| o.object == Some(id) && o.kind == Kind::Static)
        });
        let what = match (rec, our, held) {
            (true, true, _) => "answered by both",
            (true, false, false) => "answered in the recording only: an object this world lacks",
            (true, false, true) if matches!(q, Request::Use(_)) => {
                "answered in the recording only: the replay's world gives no object a use"
            }
            (true, false, true) if matches!(q, Request::Select(_)) && placed_static => {
                "answered in the recording only: an agent the replay's world places as a static"
            }
            (true, false, true) if matches!(q, Request::Examine(None)) => {
                "answered in the recording only: a push of the profile examined before"
            }
            (true, false, true) => "answered in the recording only",
            (false, true, _) => "answered by this server only",
            (false, false, _) => "answered by neither",
        };
        *out.tally
            .entry((q.family().to_string(), what.to_string()))
            .or_default() += 1;
    }
    Some(out)
}

/// Every January 2017 world session replayed whole: the server keeps the known-object and stamp
/// invariants throughout, and the requests of the families are answered as the tallies show,
/// every difference named.
/// Behaviour: server.replay.recorded-world-sessions-replayed-whole
#[test]
fn every_recorded_2017_world_session_replays_with_the_invariants_kept() {
    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let sessions: Vec<(i64, String)> = con
        .prepare(
            "SELECT s.session_id, s.session_key FROM session s JOIN capture c USING (capture_id) \
             WHERE s.game = 'ac2' AND s.world_login = 1 AND c.file LIKE '2017-%' \
             AND EXISTS (SELECT 1 FROM message m WHERE m.session_id = s.session_id AND m.mtype = 65632) \
             ORDER BY s.session_id",
        )
        .expect("prepares")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("runs")
        .collect::<Result<_, _>>()
        .expect("rows");
    let limit: usize = std::env::var("YALAIN_TEST_REPLAY_LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);
    let mut tally: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut breaches: BTreeMap<String, usize> = BTreeMap::new();
    let mut notes: BTreeMap<String, usize> = BTreeMap::new();
    let mut unhandled: BTreeMap<String, u64> = BTreeMap::new();
    let mut recorded = Census::new();
    let mut ours = Census::new();
    let mut replayed = 0;
    for (sid, key) in sessions.iter().take(limit) {
        let Some(o) = replay(&con, *sid) else {
            *notes.entry("not replayable".into()).or_default() += 1;
            continue;
        };
        replayed += 1;
        if let Some(n) = o.note {
            *notes.entry(n).or_default() += 1;
        }
        for (k, n) in o.tally {
            *tally.entry(k).or_default() += n;
        }
        for (k, n) in o.unhandled {
            *unhandled.entry(k).or_default() += n;
        }
        for (into, from) in [(&mut recorded, o.recorded), (&mut ours, o.ours)] {
            for (k, shapes) in from {
                for (sh, n) in shapes {
                    *into.entry(k.clone()).or_default().entry(sh).or_default() += n;
                }
            }
        }
        for (k, n) in &o.checker.breaches {
            *breaches.entry(k.clone()).or_default() += n;
            if *n > 0 {
                println!("  {key}: {k} x{n}");
            }
        }
    }
    println!("{replayed} January 2017 world sessions replayed whole");
    for (k, n) in &notes {
        println!("  {n:6}  {k}");
    }
    let mut family = String::new();
    for ((f, what), n) in &tally {
        if *f != family {
            println!("{f}:");
            family.clone_from(f);
        }
        println!("  {n:8}  {what}");
    }
    println!("not handled by this server (counted by it):");
    for (k, n) in &unhandled {
        println!("  {n:8}  {k}");
    }
    println!("kinds of message sent: recorded, this server's, this server's in a recorded shape:");
    let mut kinds: Vec<&String> = recorded.keys().chain(ours.keys()).collect();
    kinds.sort();
    kinds.dedup();
    let total = |c: &Census, k: &String| c.get(k).map_or(0, |s| s.values().sum::<usize>());
    kinds.sort_by_key(|k| std::cmp::Reverse((total(&recorded, k), total(&ours, k))));
    for k in kinds {
        let shaped: usize = ours.get(k).map_or(0, |s| {
            s.iter()
                .filter(|(sh, _)| recorded.get(k).is_some_and(|r| r.contains_key(*sh)))
                .map(|(_, n)| n)
                .sum()
        });
        println!(
            "  {:8} {:8} {:8}  {k}",
            total(&recorded, k),
            total(&ours, k),
            shaped
        );
    }
    println!(
        "breaches of the invariants in this server's messages: {}",
        breaches.values().sum::<usize>()
    );
    for (k, n) in &breaches {
        println!("  {n:8}  {k}");
    }
    assert!(replayed > 0);
    assert!(breaches.is_empty(), "the server breaks the invariants");
}
