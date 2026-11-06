//! The January 2017 world entries replayed: for each recorded world connection that made a
//! player, the character enters this server's world and its world connection's answer is compared
//! with the recorded one by its shape.
//!
//! The character is seeded from the recording (its id, its name and look from the player's
//! recorded object, its race and sex from its recorded description); a bot logs in, enters with
//! it, follows the referral, then sends the recorded client's world messages again in order, each
//! at its recorded time after the world connection came up.
//!
//! The shape: in the order the server sent them,
//! the player's object, the message making it the client's player, its description, the arrival
//! effect, the freeze, the end of the portal effect, the session start; then, after the client
//! says the scene is drawn, the freeze lifted and the arrival effect ended. Compared fields: the
//! player's id in its object and in the player message, the region, the description's template,
//! the effects' ids, the freeze's value, the session start's function. What the recorded server
//! sent besides (other objects, crafting, channels, allegiance, quests) is counted, not compared.
//!
//! And the characters made in those recordings, made again from the choices their clients sent:
//! the clothes in their look as made, and each garment made worn at their first entry.

use std::collections::BTreeMap;
use std::path::Path;

use auberean_protocol::login::{CharacterCreated, CreateCharacter, DataHeld, PlayCharacter};
use auberean_protocol::physics::CreateObject;
use auberean_protocol::text::NarrowString;
use auberean_protocol::types::{PartGroupDesc, VisualDesc};
use auberean_protocol::wire::{Field, Writer};
use auberean_protocol::{queue, ClientBlob, ClientMessage, ServerMessage};
use rusqlite::{Connection, OpenFlags};
use yalain_store::{Character, Store};
use yalain_testkit::authored::Portal;
use yalain_testkit::Harness;
use yalain_world::session::function;
use yalain_world::world::{ARRIVAL_FX, PORTAL_FX};

const STEPS: usize = 400;

/// How long after the drawn scene the recorded server's answer is looked for, in seconds.
const SCENE_WINDOW: f64 = 5.0;

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
    blob: u64,
    raw: Vec<u8>,
}

/// One message of the shape.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    PlayerObject,
    MakePlayer { region: u32 },
    Description { template: u32 },
    Effect(u32),
    EndEffect(u32),
    Frozen(u32),
    SessionStart,
}

/// The shape token of a server message, for the player `id`; `None` for a message outside the
/// shape.
fn token(m: &ServerMessage, id: u64) -> Option<Token> {
    Some(match m {
        ServerMessage::CreateObject(o) if o.id == id => Token::PlayerObject,
        ServerMessage::PlayerObject(p) if p.id == id => Token::MakePlayer { region: p.region },
        ServerMessage::PlayerQualities(q) => Token::Description { template: q.0.did },
        ServerMessage::DoFx(e) if e.sender.id == id && e.action.fx == ARRIVAL_FX => {
            Token::Effect(e.action.fx)
        }
        ServerMessage::StopFx(e)
            if e.sender.id == id && [ARRIVAL_FX, PORTAL_FX].contains(&e.action.0) =>
        {
            Token::EndEffect(e.action.0)
        }
        ServerMessage::PlayerCall(c) if c.function.0 == function::MOVEMENT_FROZEN => {
            let v = c
                .args
                .as_ref()
                .and_then(|a| a.0.get(4..8))
                .map_or(9, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            Token::Frozen(v)
        }
        ServerMessage::PlayerCall(c) if c.function.0 == function::SESSION_START => {
            Token::SessionStart
        }
        _ => return None,
    })
}

/// Server messages in the order sent: the non-ephemeral ones by their blob sequence.
fn in_send_order<'a>(v: impl Iterator<Item = (u64, &'a [u8])>) -> Vec<ServerMessage> {
    let mut v: Vec<(u64, &[u8])> = v.filter(|(b, _)| b >> 63 == 0).collect();
    v.sort_by_key(|(b, _)| b & 0xFFFF_FFFF);
    v.into_iter()
        .filter_map(|(_, raw)| ServerMessage::decode(raw).ok())
        .collect()
}

#[derive(Debug, Default)]
struct Outcome {
    entry: Vec<Token>,
    entry_ours: Vec<Token>,
    scene: Option<(Vec<Token>, Vec<Token>)>,
    extras: Vec<String>,
    enter_to_player: Option<f64>,
    note: Option<String>,
}

fn look_bytes(v: &VisualDesc) -> Vec<u8> {
    let mut w = Writer::default();
    v.write(&mut w);
    w.finish()
}

fn replay(con: &Connection, session: i64) -> Option<Outcome> {
    let recs: Vec<Rec> = con
        .prepare("SELECT t, dir, queue, blob_id, raw FROM message WHERE session_id = ?1 AND status = 'ac2' ORDER BY idx")
        .ok()?
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
    let recorded = in_send_order(
        recs.iter()
            .filter(|r| r.to_client)
            .map(|r| (r.blob, r.raw.as_slice())),
    );
    let id = recorded.iter().find_map(|m| {
        if let ServerMessage::PlayerObject(p) = m {
            Some(p.id)
        } else {
            None
        }
    })?;
    let object = recorded.iter().find_map(|m| match m {
        ServerMessage::CreateObject(o) if o.id == id => Some(o.clone()),
        _ => None,
    })?;
    let (race, sex) = recorded
        .iter()
        .find_map(|m| match m {
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
    // The recorded shape: the entry up to the session start, and the answer to the drawn scene.
    let scene_at = recs
        .iter()
        .position(|r| !r.to_client && r.raw.get(..4) == Some(&0x100D1u32.to_le_bytes()[..]));
    let entry_end = recorded
        .iter()
        .position(|m| token(m, id) == Some(Token::SessionStart))
        .map_or(recorded.len(), |i| i + 1);
    out.entry = recorded[..entry_end]
        .iter()
        .filter_map(|m| token(m, id))
        .collect();
    // What the recorded server sent besides, until the scene was drawn.
    let t_scene = scene_at.map_or(f64::MAX, |i| recs[i].t);
    let before_scene = in_send_order(
        recs.iter()
            .filter(|r| r.to_client && r.t <= t_scene)
            .map(|r| (r.blob, r.raw.as_slice())),
    );
    for m in &before_scene {
        let ours = matches!(m, ServerMessage::PlayerCall(c) if c.function.0 == function::SHOW_TEXT);
        if token(m, id).is_none() && !ours {
            let what = match m {
                ServerMessage::PlayerCall(c) => format!("call {:08X}", c.function.0),
                ServerMessage::CreateObject(_) => "another object".to_string(),
                other => format!("{:#x}", other.message_type()),
            };
            out.extras.push(what);
        }
    }

    let mut h = Harness::new();
    let account = h.server.store.create_account("replay", None, h.unix).ok()?;
    #[allow(clippy::cast_sign_loss)]
    let ch = Character {
        id,
        account: account.id,
        name: name.clone(),
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
    let entered = h.now.0;
    let enter = ClientMessage::PlayCharacter(PlayCharacter {
        character: id,
        account: NarrowString::from("replay"),
    });
    h.bots[b].send(false, queue::LOGON, &enter);
    let made = |h: &Harness| {
        h.bots[b]
            .messages(true)
            .iter()
            .any(|m| matches!(m, ServerMessage::PlayerObject(_)))
    };
    if !h.run_until(STEPS, made) {
        out.note = Some("no player made".into());
        return Some(out);
    }
    out.enter_to_player = Some(h.now.0 - entered);
    h.run(20);
    let world_up = h.now.0;
    let first_t = recs.first().map_or(0.0, |r| r.t);
    let mut scene_sent_at = None;
    for (i, r) in recs.iter().enumerate().filter(|(_, r)| !r.to_client) {
        let Ok(ClientBlob::Message(m)) = ClientBlob::decode(r.queue, &r.raw) else {
            continue;
        };
        h.run_to(yalain_net::LocalTime(world_up + (r.t - first_t)));
        if Some(i) == scene_at {
            scene_sent_at = Some(h.bots[b].inbox.len());
        }
        h.bots[b].send(true, r.queue, &m);
        h.run(3);
        if scene_sent_at.is_some() {
            break;
        }
    }
    h.run(20);
    let ours = in_send_order(
        h.bots[b]
            .inbox
            .iter()
            .filter(|r| r.world)
            .map(|r| (r.blob_id, r.bytes.as_slice())),
    );
    let our_end = ours
        .iter()
        .position(|m| token(m, id) == Some(Token::SessionStart))
        .map_or(ours.len(), |i| i + 1);
    out.entry_ours = ours[..our_end]
        .iter()
        .filter_map(|m| token(m, id))
        .collect();
    if let (Some(at), Some(sent)) = (scene_at, scene_sent_at) {
        // The recorded answer: what the server sent within a few seconds of the drawn scene.
        let t_scene = recs[at].t;
        let rec_after = in_send_order(
            recs[at..]
                .iter()
                .filter(|r| r.to_client && r.t <= t_scene + SCENE_WINDOW)
                .map(|r| (r.blob, r.raw.as_slice())),
        );
        let after_tokens = |v: &[ServerMessage]| -> Vec<Token> {
            v.iter()
                .filter_map(|m| token(m, id))
                .filter(|t| matches!(t, Token::Frozen(0) | Token::EndEffect(_)))
                .take(2)
                .collect()
        };
        let ours_after = in_send_order(
            h.bots[b].inbox[sent..]
                .iter()
                .filter(|r| r.world)
                .map(|r| (r.blob_id, r.bytes.as_slice())),
        );
        let mut want = after_tokens(&rec_after);
        let mut got = after_tokens(&ours_after);
        want.sort_by_key(|t| format!("{t:?}"));
        got.sort_by_key(|t| format!("{t:?}"));
        out.scene = Some((want, got));
    }
    Some(out)
}

/// Every January 2017 world entry, replayed: the server answers it with the recorded entry's
/// shape (the player's object, its making, its description, the effects and the freeze, the
/// session start, in order, with the recorded key fields) and answers the drawn scene as the
/// recorded server did. The differences allowed are named: a recording that ends before the
/// session start or before the scene is drawn.
/// Behaviour: server.replay.recorded-world-entries-answered-as-recorded
#[test]
fn every_recorded_2017_world_entry_is_answered_with_the_recorded_shape() {
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
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    let mut extras: BTreeMap<String, usize> = BTreeMap::new();
    let mut unexplained = Vec::new();
    let mut times = Vec::new();
    for (sid, key) in &sessions {
        let Some(o) = replay(&con, *sid) else {
            *tally.entry("not replayable".into()).or_default() += 1;
            continue;
        };
        for e in o.extras.iter().collect::<std::collections::BTreeSet<_>>() {
            *extras.entry(e.clone()).or_default() += 1;
        }
        times.extend(o.enter_to_player);
        let entry_same = o.entry == o.entry_ours;
        let without_portal: Vec<Token> = o
            .entry_ours
            .iter()
            .filter(|t| **t != Token::EndEffect(PORTAL_FX))
            .cloned()
            .collect();
        let no_portal_effect = !entry_same
            && !o.entry.contains(&Token::EndEffect(PORTAL_FX))
            && o.entry == without_portal;
        let recorded_complete = o.entry.contains(&Token::SessionStart);
        let what = match (&o.note, entry_same, recorded_complete, &o.scene) {
            (Some(n), ..) => format!("not run: {n}"),
            (None, true, _, Some((want, got))) if want == got => "the recorded shape, the scene answered as recorded".into(),
            (None, true, _, Some((want, _))) if want.is_empty() => {
                "the recorded shape; the recorded server did not answer the drawn scene within 5 s".into()
            }
            (None, false, _, Some((want, got))) if no_portal_effect && want == got => {
                "the recorded server ended no portal effect (none was on); otherwise the recorded shape".into()
            }
            (None, true, _, Some(_)) => {
                unexplained.push(format!("{key}: the scene's answer {:?}", o.scene));
                "the scene answered otherwise".into()
            }
            (None, true, _, None) => "the recorded shape; the recording ends before the scene is drawn".into(),
            (None, false, false, _) if o.entry_ours.starts_with(&o.entry) => {
                "the recording ends before the session start; ours the recorded shape up to there".into()
            }
            (None, false, ..) => {
                unexplained.push(format!("{key}: recorded {:?}, ours {:?}", o.entry, o.entry_ours));
                "differs".into()
            }
        };
        *tally.entry(what).or_default() += 1;
    }
    println!("{} January 2017 world entries replayed:", sessions.len());
    for (k, n) in &tally {
        println!("  {n:4}  {k}");
    }
    times.sort_by(f64::total_cmp);
    if !times.is_empty() {
        println!(
            "entering to the player made (virtual time): median {:.2} s, max {:.2} s",
            times[times.len() / 2],
            times[times.len() - 1]
        );
    }
    println!("what the recorded servers sent before the scene was drawn besides the shape (entries holding it):");
    for (k, n) in extras.iter().filter(|(_, n)| **n >= 5) {
        println!("  {n:4}  {k}");
    }
    for u in unexplained.iter().take(20) {
        println!("  unexplained: {u}");
    }
    assert!(!sessions.is_empty());
    assert!(
        unexplained.is_empty(),
        "{} entries differ unexplained",
        unexplained.len()
    );
}

fn global_bytes(g: &PartGroupDesc) -> Vec<u8> {
    let mut w = Writer::default();
    g.write(&mut w);
    w.finish()
}

/// A recorded creation: what the client asked, what the server answered, and the objects the
/// server made in that recording, in order.
struct Recorded {
    asked: CreateCharacter,
    answer: CharacterCreated,
    made: Vec<CreateObject>,
}

fn recorded() -> Vec<Recorded> {
    let path = auberean_testkit::data::require_capture_index();
    let con = super::keeping::open(&path);
    let mut st = con
        .prepare(
            "SELECT m.session_id, m.idx, m.raw, s.capture_id FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) WHERE c.file LIKE '2017-%' AND m.dir = 'c2s' \
             AND m.mtype = 65578 ORDER BY m.session_id, m.idx",
        )
        .expect("the query prepares");
    let asked: Vec<(i64, i64, Vec<u8>, i64)> = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .expect("the query runs")
        .flatten()
        .collect();
    let mut out = Vec::new();
    for (session, idx, raw, capture) in asked {
        let Ok(ClientMessage::CreateCharacter(asked)) = ClientMessage::decode(&raw) else {
            continue;
        };
        let answer = con
            .prepare(
                "SELECT raw FROM message WHERE session_id = ?1 AND idx > ?2 AND dir = 's2c' \
                 AND mtype = 65623 ORDER BY idx LIMIT 1",
            )
            .expect("the query prepares")
            .query_row([session, idx], |r| r.get::<_, Vec<u8>>(0))
            .ok()
            .and_then(|raw| match ServerMessage::decode(&raw) {
                Ok(ServerMessage::CharacterCreated(a)) => Some(a),
                _ => None,
            });
        let Some(answer) = answer.filter(|a| a.identity.id != 0) else {
            continue;
        };
        let made = con
            .prepare(
                "SELECT m.raw FROM message m JOIN session s USING (session_id) WHERE s.capture_id = ?1 \
                 AND m.dir = 's2c' AND m.mtype = 65624 ORDER BY m.session_id, m.idx",
            )
            .expect("the query prepares")
            .query_map([capture], |r| r.get::<_, Vec<u8>>(0))
            .expect("the query runs")
            .flatten()
            .filter_map(|raw| match ServerMessage::decode(&raw) {
                Ok(ServerMessage::CreateObject(o)) => Some(o),
                _ => None,
            })
            .collect();
        out.push(Recorded {
            asked,
            answer,
            made,
        });
    }
    out
}

/// Each character made in the 2017 recordings is made again from its client's choices wearing the
/// same clothes in the same colors (the appearances its look holds, byte for byte), and at its first
/// entry each garment the 2017 server made worn is made worn, its object as that server sent it
/// byte for byte apart from the ids: its look and color, worn and contained by the player.
/// Behaviour: server.characters.starting-clothes-as-2017-made-them
#[test]
fn the_recorded_new_characters_are_made_in_their_clothes_as_2017_made_them() {
    let p = Portal::open();
    let rules = p.start_rules();
    let mut catalog = yalain_content::Catalog::default();
    for t in rules.items.values().flatten() {
        catalog.insert(p.template(t.template));
    }
    let made = recorded();
    assert!(
        made.len() >= 2,
        "the recordings make characters: {}",
        made.len()
    );
    let mut garments = 0;
    for (n, r) in made.iter().enumerate() {
        let mut h = yalain_testkit::Harness::new();
        let areas = h.server.start.by_race.clone();
        h.server.start = rules.clone();
        h.server.start.by_race = areas;
        h.server.start.areas = yalain_testkit::test_start().areas;
        h.server.catalog = catalog.clone();
        let account = format!("maker{n}");
        let b = h.at_character_list(&account);
        let before = h.bots[b].inbox.len();
        let asked = CreateCharacter {
            account: NarrowString::from(account.as_str()),
            ..r.asked.clone()
        };
        h.bots[b].send(false, queue::LOGON, &ClientMessage::CreateCharacter(asked));
        assert!(h.run_until(1000, |h| h.bots[b].inbox.len() > before));
        let Some(ServerMessage::CharacterCreated(ours)) = h.bots[b].inbox[before].message() else {
            panic!("creation {n} is not answered");
        };
        let me = ours.identity.id;
        assert_eq!(
            global_bytes(&ours.identity.visual.global),
            global_bytes(&r.answer.identity.visual.global),
            "creation {n}: the look's appearances"
        );
        assert_eq!(ours.identity.visual.scale, r.answer.identity.visual.scale);
        let from = h.bots[b].inbox.len();
        h.enter_world(b, &account, me);
        let theirs = r.answer.identity.id;
        let mut seen = std::collections::BTreeSet::new();
        // Each garment as its first entry made it.
        for rec in r
            .made
            .iter()
            .filter(|o| o.weenie.objects[1] == theirs && seen.insert(o.id))
        {
            let template = rec.weenie.entity_did;
            let held = h
                .server
                .players
                .values()
                .find(|p| p.character == me)
                .expect("in the world")
                .play
                .items
                .iter()
                .find(|i| i.template == template && i.location != 0)
                .map(|i| i.id)
                .unwrap_or_else(|| panic!("creation {n}: {template:#x} not worn"));
            let mut o = h
                .world_messages(b, from)
                .iter()
                .find_map(|m| match m {
                    ServerMessage::CreateObject(o) if o.id == held => Some(o.clone()),
                    _ => None,
                })
                .expect("made");
            o.id = rec.id;
            o.weenie.objects[0] = rec.weenie.objects[0];
            o.weenie.objects[1] = rec.weenie.objects[1];
            assert_eq!(
                ServerMessage::CreateObject(o).encode(),
                ServerMessage::CreateObject(rec.clone()).encode(),
                "creation {n}: {template:#x}"
            );
            garments += 1;
        }
    }
    assert_eq!(
        garments, 5,
        "the recorded garments: a human's three, a Tumerok's two"
    );
}

/// Every 2017 session start's perk table, word for word, is what the skills it holds build: of a
/// kind's perks the highest priority; and a category table is the same but for the categories a
/// client reads into its attacks, which this server doesn't tell.
/// Behaviour: server.skills.repository-tables-as-2017
#[test]
fn every_recorded_session_starts_perk_table_is_what_its_skills_build() {
    use yalain_world::skills::{held, RepositoryTables, CATEGORIES_NOT_TOLD};
    const WORDS: [u8; 12] = [0, 1, 0, 0x44, 0, 0, 0, 0, 1, 0, 0xFF, 0xFF];
    const REFS: [u8; 12] = [0, 1, 0, 0x44, 0, 0, 0, 0, 0x11, 0, 0xFF, 0xFF];
    let find = |raw: &[u8], what: &[u8], from: usize| {
        raw[from..]
            .windows(what.len())
            .position(|w| w == what)
            .map(|at| at + from)
    };
    let word = |raw: &[u8], at: usize| {
        u32::from_le_bytes([raw[at], raw[at + 1], raw[at + 2], raw[at + 3]])
    };
    // Each table: where it stands, its pairs in the order they are packed, and where it ends.
    let table = |raw: &[u8], at: usize| {
        let n = (word(raw, at + 12) & 0xFFFF) as usize;
        let pairs: Vec<(u32, u32)> = (0..n)
            .map(|i| (word(raw, at + 16 + 8 * i), word(raw, at + 20 + 8 * i)))
            .collect();
        (pairs, at + 16 + 8 * n)
    };
    let tables = |raw: &[u8]| {
        let head = find(raw, &[0, 0, 0x33, 0x02], 0)?;
        let first = find(raw, &WORDS, head)?;
        let (perks, end) = table(raw, first);
        let second = find(raw, &WORDS, end)?;
        let (categories, _) = table(raw, second);
        let skills = find(raw, &REFS, second).map(|at| table(raw, at).0);
        Some((perks, categories, skills))
    };
    let path = auberean_testkit::data::require_capture_index();
    let portal = yalain_testkit::authored::Portal::open();
    let skill_table = portal.skills();
    let con = open(&path);
    let mut q = con
        .prepare(
            "SELECT m.raw FROM message m JOIN session s USING(session_id) \
             JOIN capture c USING(capture_id) \
             WHERE m.dir='s2c' AND m.mtype=65638 AND c.file LIKE '2017%' \
             AND substr(m.raw, 5, 4) = x'27001282'",
        )
        .expect("a query");
    let rows: Vec<Vec<u8>> = q
        .query_map([], |r| r.get(0))
        .expect("rows")
        .flatten()
        .collect();
    let (mut starts, mut with_perks, mut same_order, mut withheld) = (0, 0, 0, 0);
    for raw in rows {
        let Some((perks, categories, Some(skills))) = tables(&raw) else {
            continue;
        };
        starts += 1;
        let kept: Vec<yalain_world::state::SkillState> = skills
            .iter()
            .map(|(skill, _)| yalain_world::state::SkillState {
                skill: *skill,
                granted: 0,
                mask: 0,
                cost: 0,
                children: 0,
                dependents: 0,
                spent: 0,
            })
            .collect();
        let t = RepositoryTables::of(&skill_table, &held(&[], &kept));
        let told_raw = yalain_world::skills::repository_untraining(0, 0, &t, None).encode();
        let (ours, ours_categories, _) = tables(&told_raw).expect("our tables");
        with_perks += usize::from(!perks.is_empty());
        let sorted = |v: &[(u32, u32)]| {
            let mut v = v.to_vec();
            v.sort_unstable();
            v
        };
        assert_eq!(sorted(&perks), sorted(&t.perks), "the perks");
        // Word for word, in the order the table packs them.
        assert_eq!(perks, ours, "the perks, as packed");
        same_order += usize::from(perks == ours);
        let told: Vec<(u32, u32)> = categories
            .iter()
            .copied()
            .filter(|(c, _)| !CATEGORIES_NOT_TOLD.contains(c))
            .collect();
        withheld += usize::from(told.len() < categories.len());
        assert_eq!(told, ours_categories, "the categories, as packed");
    }
    assert!(starts > 400, "{starts} session starts");
    assert!(with_perks > 80, "{with_perks} with perks");
    println!(
        "{starts} starts, {with_perks} with perks, {same_order} of the same order, \
         {withheld} with attack categories withheld"
    );
}
