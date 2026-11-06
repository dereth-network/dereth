//! Chat held to the January 2017 recordings: what this server builds for a tell, an emote, a typed
//! emote, the allegiance and channel lists, and the chat service's lines and answers is what the
//! 2017 servers sent, apart from the ids a packed graph numbers its entries with.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::{require_install, TARGET};
use auberean_dat::DatFile;
use auberean_protocol::login::ChatPacket;
use auberean_protocol::{ClientBlob, ClientMessage, ServerMessage};
use auberean_script::args::{decode, Arg};
use auberean_script::graph::{Body, Graph};
use auberean_script::library::{Library, RECORD};
use auberean_script::native_object::{Item, NativeObject};
use rusqlite::{Connection, OpenFlags};
use yalain_world::calls::{self, Node, Value};
use yalain_world::chat::{self, function, EMOTES, NEW_CHANNELS};

fn library() -> Option<&'static Library> {
    static L: OnceLock<Option<Library>> = OnceLock::new();
    L.get_or_init(|| {
        let dir = require_install(TARGET);
        let (_, path) = install_dats(&dir)
            .ok()?
            .into_iter()
            .find(|(n, _)| *n == DatName::Portal)?;
        let dat = DatFile::open(&path).ok()?;
        dat.read_checked(RECORD, Library::from_record)
            .ok()
            .map(|c| c.value)
    })
    .as_ref()
}

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

/// A graph with its entries numbered 1, 2, 3... in order, every reference renumbered with them.
fn renumbered(g: &Graph) -> Graph {
    let map: BTreeMap<u32, u32> = g.ids.iter().zip(1..).map(|(a, b)| (*a, b)).collect();
    let re = |id: u32| map.get(&id).copied().unwrap_or(id);
    let item = |i: &Item| match *i {
        Item::Ref(id) => Item::Ref(re(id)),
        Item::HashedRef { id, hash, use_hash } => Item::HashedRef {
            id: re(id),
            hash,
            use_hash,
        },
        other => other,
    };
    let mut out = g.clone();
    out.ids = (1..=u32::try_from(g.ids.len()).unwrap_or(0)).collect();
    for e in &mut out.entries {
        if let Body::Object(o) = &mut e.body {
            for (s, t) in o.slots.iter_mut().zip(&o.tags) {
                if *t == 1 {
                    *s = re(*s);
                }
            }
            if let Some(NativeObject::Hash(h)) = &mut o.native {
                for b in &mut h.buckets {
                    for (k, v) in b.iter_mut() {
                        *k = item(k);
                        *v = item(v);
                    }
                }
            }
            if let Some(NativeObject::Sequence { items, .. }) = &mut o.native {
                for i in items.iter_mut() {
                    *i = item(i);
                }
            }
        }
    }
    out
}

fn args_of(m: &ServerMessage, lib: &Library) -> Option<(u32, Vec<Arg>)> {
    let ServerMessage::PlayerCall(c) = m else {
        return None;
    };
    let a = decode(&c.args.as_ref()?.0, lib).ok()?;
    Some((
        c.function.0,
        a.into_iter()
            .map(|x| match x {
                Arg::Object(g) => Arg::Object(renumbered(&g)),
                other => other,
            })
            .collect(),
    ))
}

#[derive(Default)]
struct Tally(BTreeMap<String, (usize, usize)>);

impl Tally {
    fn count(&mut self, what: &str, same: bool) {
        let t = self.0.entry(what.to_string()).or_default();
        t.0 += usize::from(same);
        t.1 += 1;
    }
}

/// The chat messages this server builds are the 2017 servers': the empty allegiance profile and a
/// new character's channel list (graph ids aside), each emote's behavior (its context aside), each
/// typed emote another player made, the tell confirmations and refusals, and the chat service's
/// echoes and answers to every recorded line, byte for byte; every recorded line reads.
/// Behaviour: server.chat.messages-as-2017-sent-them
#[test]
fn chat_messages_are_built_as_the_2017_servers_sent_them() {
    let path = auberean_testkit::data::require_capture_index();
    let Some(lib) = library() else {
        println!("no September 2013 install here");
        return;
    };
    let con = open(&path);
    let mut st = con
        .prepare(
            "SELECT m.session_id, m.dir, m.raw, s.world_login FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) WHERE c.file LIKE '2017-%' AND m.status = 'ac2' \
             AND m.mtype IN (65619, 65638, 65639, 65663, 65747) ORDER BY m.session_id, m.idx",
        )
        .expect("prepares");
    let mut rows = st.query([]).expect("runs");
    let mut t = Tally::default();
    let ours_profile = args_of(&chat::no_allegiance(), lib).expect("ours reads");
    // The channels as one new character's world (its name is the recorded list's own).
    let mut pending_emote: Option<u32> = None;
    let mut pending_tell: Option<Vec<Value>> = None;
    let mut pending_line: Option<chat::RoomLine> = None;
    let mut last_session = -1i64;
    while let Some(r) = rows.next().expect("a row") {
        let sid: i64 = r.get(0).expect("session");
        if sid != last_session {
            (pending_emote, pending_tell, pending_line) = (None, None, None);
            last_session = sid;
        }
        let dir: String = r.get(1).expect("dir");
        let raw: Vec<u8> = r.get(2).expect("raw");
        if dir == "c2s" {
            match ClientBlob::decode(3, &raw) {
                Ok(ClientBlob::Message(ClientMessage::ServerCall(c))) => {
                    let v = calls::values(&c.args.0).unwrap_or_default();
                    match c.function.0 {
                        0x8205_000B => {
                            pending_emote = match v.first() {
                                Some(Value::Int(i)) => Some(*i),
                                _ => None,
                            }
                        }
                        0x8205_0002 => pending_tell = Some(v),
                        _ => {}
                    }
                }
                Ok(ClientBlob::Message(ClientMessage::Chat(ChatPacket(p)))) => {
                    let line = chat::read_room_line(&p);
                    t.count("a client's room line reads", line.is_some());
                    pending_line = line;
                }
                _ => {}
            }
            continue;
        }
        let Ok(m) = ServerMessage::decode(&raw) else {
            continue;
        };
        match &m {
            // The emote's behavior: the first after the request that carries an emote's id.
            ServerMessage::PlayBehavior(e)
                if pending_emote.is_some() && e.action.bits & chat::EMOTE_ID_BIT != 0 =>
            {
                let id = pending_emote.take().unwrap_or(0);
                let Some(want) = EMOTES.iter().find(|x| x.id == id) else {
                    t.count("an emote in the table", false);
                    continue;
                };
                let b = &e.action;
                let same = b.behavior == want.behavior
                    && b.words[12] == id
                    && (b.bits & 0x20 != 0) == want.cancels_move
                    && (b.bits & 0x100 != 0) == want.fades_children
                    && (b.bits & 0x10_0000 != 0) == (want.fx != 0)
                    && (want.fx == 0 || b.fx == want.fx);
                // The ten dances (behaviors 0x410000A6..AF, each with its effect): once 2017
                // played another dance than the one asked for.
                let dance = |x: u32| (0x4100_00A6..=0x4100_00AF).contains(&x);
                if !same && dance(b.behavior) && dance(want.behavior) {
                    t.count("a dance answered with another dance", true);
                    continue;
                }
                t.count("an emote's behavior as the table gives it", same);
            }
            ServerMessage::PlayerCall(c)
                if c.function.0 == 0x8205_0011 && pending_tell.is_some() =>
            {
                // The confirmation or refusal of the tell just sent: rebuild it from the request.
                let v = pending_tell.take().unwrap_or_default();
                let got = calls::values(&c.args.as_ref().map(|a| a.0.clone()).unwrap_or_default())
                    .unwrap_or_default();
                let kind = match got.get(1) {
                    Some(Value::Int(k)) => *k,
                    _ => 0,
                };
                let text = |x: &Value| match x.root() {
                    Some(Node::TextInfo(t)) => Some(t.clone()),
                    _ => None,
                };
                let (Some(words), Some(tellee)) =
                    (v.get(1).and_then(text), v.get(2).and_then(text))
                else {
                    continue;
                };
                let mut want = auberean_script::native_object::TextInfo {
                    string_id: if kind == 32 {
                        chat::strings::YOU_TELL
                    } else {
                        chat::strings::UNAVAILABLE
                    },
                    table: chat::strings::TABLE,
                    ..Default::default()
                };
                let nested = |id, t| auberean_script::native_object::Variable {
                    id,
                    kind: 9,
                    value: auberean_script::native_object::VariableValue::Nested(Box::new(t)),
                };
                if kind == 32 {
                    want.set_variable(nested(chat::strings::TELLEE, tellee));
                    want.set_variable(nested(chat::strings::WORDS, words));
                } else {
                    want.set_variable(nested(chat::strings::NAME, tellee));
                }
                let same = got.first().and_then(text) == Some(want);
                t.count(
                    if kind == 32 {
                        "a tell's confirmation built from the tell"
                    } else {
                        "a tell's refusal built from the tell"
                    },
                    same,
                );
            }
            ServerMessage::CellObjectCall(c) if c.call.function.0 == function::HEAR_TYPED_EMOTE => {
                // Another player's typed emote: ours from its id, name and words.
                let v = c
                    .call
                    .args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())
                    .unwrap_or_default();
                let text = |x: Option<&Value>| match x.and_then(Value::root) {
                    Some(Node::Text(t)) => Some(t.clone()),
                    _ => None,
                };
                // The values come last first: the words, the name, the id.
                let (Some(words), Some(name)) = (text(v.first()), text(v.get(1))) else {
                    t.count("a typed emote reads", false);
                    continue;
                };
                let ours = chat::heard_typed_emote(c.object, &name, &words);
                let decoded = |m: &ServerMessage| match m {
                    ServerMessage::CellObjectCall(c) => {
                        decode(&c.call.args.as_ref()?.0, lib).ok().map(|a| {
                            a.into_iter()
                                .map(|x| match x {
                                    Arg::Object(g) => Arg::Object(renumbered(&g)),
                                    other => other,
                                })
                                .collect::<Vec<_>>()
                        })
                    }
                    _ => None,
                };
                let same = v.get(2) == Some(&Value::Long(c.object.id))
                    && decoded(&ours).is_some()
                    && decoded(&ours) == decoded(&m);
                t.count("a typed emote built from its emoter, name and words", same);
            }
            ServerMessage::PlayerCall(c) if c.function.0 == function::ALLEGIANCE_PROFILE => {
                let got = args_of(&m, lib);
                let empty = got.as_ref().is_some_and(
                    |(_, a)| matches!(a.first(), Some(Arg::Object(g)) if g.entries.len() == 1),
                );
                if empty {
                    t.count(
                        "an empty allegiance profile as ours",
                        got.as_ref() == Some(&ours_profile),
                    );
                }
            }
            ServerMessage::PlayerCall(c) if c.function.0 == function::CHANNELS => {
                // A list of the four channels a new character had: ours for that world's name.
                let got = args_of(&m, lib);
                let Some((_, a)) = &got else { continue };
                let Some(Arg::Object(g)) = a.first() else {
                    continue;
                };
                if g.entries.len() != 9 {
                    continue;
                }
                let name = g.entries.iter().find_map(|e| match &e.body {
                    Body::Object(o) => match &o.native {
                        Some(NativeObject::Text(u)) => Some(String::from_utf16_lossy(u)),
                        _ => None,
                    },
                    _ => None,
                });
                let names: Vec<String> = g
                    .entries
                    .iter()
                    .filter_map(|e| match &e.body {
                        Body::Object(o) => match &o.native {
                            Some(NativeObject::Text(u)) => Some(String::from_utf16_lossy(u)),
                            _ => None,
                        },
                        _ => None,
                    })
                    .collect();
                let world = name
                    .as_deref()
                    .and_then(|n| n.strip_suffix(" PvP"))
                    .unwrap_or_default()
                    .to_string();
                let wanted: Vec<String> = NEW_CHANNELS
                    .iter()
                    .map(|c| format!("{world} {}", c.name))
                    .collect();
                if names != wanted {
                    continue;
                }
                let ours = args_of(&chat::channels(&world, &NEW_CHANNELS), lib);
                t.count("a new character's channels as ours", got == ours);
            }
            ServerMessage::Chat(ChatPacket(p)) => {
                let Ok(inc) = dereth_protocol::turbine::decode_incoming(
                    &[&u32::try_from(p.len()).unwrap_or(0).to_le_bytes()[..], p].concat(),
                ) else {
                    t.count("a service packet reads", false);
                    continue;
                };
                let word = inc.header[3];
                match inc.payload {
                    dereth_protocol::turbine::IncomingPayload::RoomEvent(e) => {
                        let text: Vec<u16> = e.text.encode_utf16().collect();
                        let ours = chat::room_event(word, e.room, &e.name, &text, &e.extra);
                        t.count("a room line built as recorded", ours == *p);
                        if let Some(l) = &pending_line {
                            if l.text == text && l.room == e.room {
                                t.count(
                                    "the echo carries the sender's line and blob",
                                    l.extra == e.extra,
                                );
                            }
                        }
                    }
                    dereth_protocol::turbine::IncomingPayload::RoomResponse(r) => {
                        let ours = chat::room_response(word, r.context, r.result);
                        t.count("an answer built as recorded", ours == *p);
                        if let Some(l) = pending_line.take() {
                            t.count(
                                "the answer carries the line's context",
                                l.context == r.context,
                            );
                        }
                    }
                    dereth_protocol::turbine::IncomingPayload::Unknown => {
                        t.count("a service packet of a known kind", false);
                    }
                }
            }
            _ => {}
        }
    }
    println!("chat held to the 2017 recordings (same / judged):");
    let mut bad = Vec::new();
    for (k, (same, n)) in &t.0 {
        println!("  {same:7} / {n:7}  {k}");
        if same != n {
            bad.push(k.clone());
        }
    }
    assert!(!t.0.is_empty());
    assert!(bad.is_empty(), "not as recorded: {bad:?}");
}

/// The faction channel of a packed channel list: its slots and tags but the name's reference, and
/// its name.
fn faction_channel(g: &Graph) -> Option<(Vec<u32>, Vec<u8>, String)> {
    let at = |id: u32| {
        g.ids
            .iter()
            .position(|x| *x == id)
            .and_then(|i| g.entries.get(i))
    };
    g.entries.iter().find_map(|e| match &e.body {
        Body::Object(o)
            if o.package == chat::CHANNEL_DATA.0 && o.slots.get(1) == Some(&0x40_0000) =>
        {
            let name = o
                .slots
                .get(6)
                .and_then(|id| at(*id))
                .and_then(|n| match &n.body {
                    Body::Object(t) => match &t.native {
                        Some(NativeObject::Text(u)) => Some(String::from_utf16_lossy(u)),
                        _ => None,
                    },
                    _ => None,
                })?;
            Some((o.slots[..6].to_vec(), o.tags.clone(), name))
        }
        _ => None,
    })
}

/// Every faction channel the 2017 servers sent a player in its channel list is the one this server
/// gives a player of the kingdom it names, slot for slot: the Shadow's room 3, the Order's 4, the
/// Dominion's 5, the neutral kingdom's 6, each with its kingdom and its name after the world's.
/// Behaviour: server.kingdoms.channel-of-the-kingdom
#[test]
fn every_recorded_faction_channel_is_ours_for_the_kingdom_it_names() {
    use yalain_world::chat::{channels_of, KINGDOM_CHANNELS};
    let path = auberean_testkit::data::require_capture_index();
    let Some(lib) = library() else {
        println!("no September 2013 install here");
        return;
    };
    let con = open(&path);
    let mut st = con
        .prepare(
            "SELECT m.raw FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) WHERE c.file LIKE '2017-%' AND m.status = 'ac2' \
             AND m.mtype = 65638 AND substr(m.raw, 5, 4) = X'1E000582'",
        )
        .expect("prepares");
    let mut rows = st.query([]).expect("runs");
    let mut by_kingdom: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    while let Some(r) = rows.next().expect("a row") {
        let raw: Vec<u8> = r.get(0).expect("raw");
        let Ok(m) = ServerMessage::decode(&raw) else {
            continue;
        };
        let Some((_, a)) = args_of(&m, lib) else {
            continue;
        };
        let Some(Arg::Object(g)) = a.first() else {
            continue;
        };
        let Some((slots, tags, name)) = faction_channel(g) else {
            continue;
        };
        let Some((world, &(kingdom, _, _))) = KINGDOM_CHANNELS.iter().find_map(|c| {
            name.strip_suffix(c.2)
                .map(|w| (w.trim_end().to_string(), c))
        }) else {
            continue;
        };
        let Some((_, ours)) = args_of(&chat::channels(&world, &channels_of(Some(kingdom))), lib)
        else {
            continue;
        };
        let Some(Arg::Object(og)) = ours.first() else {
            continue;
        };
        let e = by_kingdom.entry(kingdom).or_default();
        e.0 += usize::from(faction_channel(og) == Some((slots, tags, name)));
        e.1 += 1;
    }
    println!("2017 faction channels by kingdom (as ours, of): {by_kingdom:?}");
    assert_eq!(by_kingdom.len(), 4, "every kingdom's");
    for (k, (same, of)) in &by_kingdom {
        assert_eq!(same, of, "kingdom {k:#x}");
    }
}
