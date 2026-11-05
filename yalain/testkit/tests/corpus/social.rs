//! What players ask of one another, the story's scenes, books and the market, held to the client's
//! library and to the January 2017 recordings: every call this server sends for them reads back
//! in the client's own library; the requests the recorded clients sent read in this server; the
//! recorded families of these answers, by shape and order, against the shapes this server sends;
//! the allegiance's profile asked for a member answered, and asked for nothing not.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::{require_install, TARGET};
use auberean_dat::DatFile;
use auberean_protocol::script::FunctionId;
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_script::args::{decode, expected_tags, Arg};
use auberean_script::graph::Body;
use auberean_script::library::{Library, RECORD};
use auberean_script::native_object::{native_type, NativeObject};
use rusqlite::{Connection, OpenFlags};
use yalain_content::story::{Keyframe, Scene, StoryTable};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Place, Prop, Props};
use yalain_testkit::play::use_of;
use yalain_testkit::{test_template, Harness};
use yalain_world::calls::{self, Node, Value};

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

const SHARD: u64 = 0x4002_9900_0000_0059;
const BROKER: u64 = 0x40FE_FA00_0000_0003;
const BROKER_TEMPLATE: u32 = 0x4700_2E8B;
const GUIDE: u32 = 0x4700_06CD;
const GEM: u32 = 0x4700_0AFD;

fn content() -> ContentPack {
    let object =
        |id: u64, template: u32, origin: [f32; 3], destination: Option<Place>| ContentObject {
            object: Some(id),
            template,
            description: Description {
                package: 0x19C,
                look: 0x1F00_0ED5,
                enter_fx: 0,
                scale: None,
                name: None,
                numbers: [0; 6],
                source: "authored for a test".into(),
            },
            cell: 0x0299_0109,
            origin,
            rotation: [1.0, 0.0, 0.0, 0.0],
            kind: Kind::Static,
            label: String::new(),
            placed: "authored for a test".into(),
            destination,
            facts: Facts::default(),
            walks: None,
            gate: None,
        };
    ContentPack {
        objects: vec![
            object(
                SHARD,
                0x4700_1BFF,
                [59.5, 240.2, -44.6],
                Some(Place {
                    cell: 0x7000_0001,
                    origin: [10.0, 10.0, 50.0],
                    rotation: [1.0, 0.0, 0.0, 0.0],
                }),
            ),
            object(BROKER, BROKER_TEMPLATE, [60.0, 240.2, -44.6], None),
        ],
        ..ContentPack::default()
    }
}

fn story() -> StoryTable {
    let mut t = StoryTable::default();
    t.scenes.insert(
        0x4000_0021,
        Scene {
            id: 0x4000_0021,
            act: 0x4000_0008,
            number: 1,
            name: (0xD413_85E6, 0x2500_18A3),
            hidden: false,
            playable: true,
            keyframes: vec![Keyframe {
                seconds: 3.0,
                picture: 0x4100_49E6,
                music: 0,
                voice: 0x2C00_0190,
                subtitle: Some((0xC96D_8B37, 0x2500_18A3)),
                tracks: 0x15,
            }],
        },
    );
    t.shards.insert(SHARD, 0x4000_0021);
    t
}

fn put(h: &mut Harness, id: u64) {
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == id)
        .expect("in the world");
    p.position.cell = 0x0299_0109;
    p.position.origin = [59.06, 240.2, -44.6];
}

fn hold(h: &mut Harness, id: u64, template: u32, quantity: u32) -> u64 {
    let item = yalain_world::inventory::make_item(&mut h.server, template, quantity, 0)
        .expect("in the catalog");
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == id)
        .expect("in the world");
    let slot = u32::try_from(p.play.items.len()).unwrap_or(0);
    p.play.items.push(yalain_world::state::Held {
        id: item,
        template,
        slot,
        location: 0,
        quantity,
    });
    if let Some(t) = h.server.things.get_mut(item) {
        t.container = Some((id, slot));
    }
    item
}

fn wait(h: &mut Harness, seconds: f64) {
    let t = h.now.0;
    h.run_to(yalain_net::LocalTime(t + seconds));
    h.run(5);
}

fn string_arg(s: &str) -> Arg {
    let mut b = yalain_world::graph::Builder::new();
    #[allow(clippy::cast_possible_truncation)]
    b.native(
        native_type::WP_STRING as u16,
        NativeObject::Text(s.encode_utf16().collect()),
    );
    Arg::Object(b.graph())
}

/// A day of what players ask of one another: an oath sworn and taken, a profile asked for, the
/// oath broken; a fellowship made, a recruit joining, the fellows' updates, a recruit leaving, the
/// fellowship disbanded; a trade asked, opened, an offer, both accepting; a book read; a story
/// shard used and its scenes done; a broker's catalog, an item put up, bought by the other player,
/// its price collected. Returns what each client received, in order.
fn a_social_day() -> Vec<Vec<ServerMessage>> {
    use yalain_world::{allegiance, fellowship, market, requests, scenes, trade};
    let mut h = Harness::new();
    h.server.content = content();
    h.server.story = story();
    h.server.brokers.insert(
        BROKER_TEMPLATE,
        market::Broker {
            catalog: 0x6F00_1503,
            location: (0x0D3C_90C4, 0x2500_0CE6),
        },
    );
    let mut book = Props::default();
    book.set("Book_Source", Prop::Str(0, 0x2500_1CEE));
    h.server.catalog.insert(test_template(GUIDE, 0x280B, book));
    h.server
        .catalog
        .insert(test_template(GEM, 0x1067, Props::default()));
    let (a, ida) = h.player_in_world("one", "Vassalia");
    let (b, idb) = h.player_in_world("two", "Patronus");
    put(&mut h, ida);
    put(&mut h, idb);
    // The allegiance.
    h.ask(a, allegiance::function::SWEAR, &[Arg::LongInt(idb)], 10);
    h.ask(
        b,
        allegiance::function::ACCEPT_VASSAL,
        &[Arg::LongInt(ida)],
        10,
    );
    h.ask(a, allegiance::function::QUERY, &[Arg::LongInt(idb)], 10);
    h.ask(
        b,
        allegiance::function::TREE_EXPORT,
        &[string_arg("allegianceexport.xml")],
        10,
    );
    h.ask(a, allegiance::function::BREAK, &[Arg::LongInt(idb)], 10);
    // A fellowship.
    h.ask(
        b,
        fellowship::function::CREATE,
        &[Arg::Int(1), Arg::Int(0), string_arg("Band")],
        10,
    );
    h.ask(b, fellowship::function::RECRUIT, &[Arg::LongInt(ida)], 10);
    h.ask(a, fellowship::function::ACCEPT, &[Arg::LongInt(idb)], 10);
    wait(&mut h, 8.0);
    h.ask(a, fellowship::function::LEAVE, &[], 10);
    h.ask(b, fellowship::function::DISBAND, &[], 10);
    // A trade.
    let bow = hold(&mut h, ida, GEM, 1);
    h.ask(a, trade::function::OPEN, &[Arg::LongInt(idb)], 10);
    h.ask(b, trade::function::OPEN, &[Arg::LongInt(ida)], 10);
    h.ask(
        a,
        trade::function::OFFER,
        &[Arg::Int(1), Arg::LongInt(bow)],
        10,
    );
    h.ask(a, trade::function::ACCEPT, &[], 10);
    h.ask(b, trade::function::ACCEPT, &[], 10);
    h.ask(b, trade::function::CLOSE, &[], 10);
    // A book.
    let guide = hold(&mut h, ida, GUIDE, 1);
    h.ask(a, requests::function::USE, &[use_of(guide, ida)], 10);
    // The market.
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == ida)
        .expect("in the world");
    yalain_world::quals::set_int(&mut p.qualities, 3100, 1_000);
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == idb)
        .expect("in the world");
    yalain_world::quals::set_int(&mut p.qualities, 3100, 5_000);
    h.ask(a, requests::function::USE, &[use_of(BROKER, ida)], 10);
    h.ask(
        a,
        market::function::ENTER_CONSIGNMENT,
        &[Arg::LongInt(BROKER)],
        10,
    );
    let gem = hold(&mut h, ida, GEM, 8);
    h.ask(
        a,
        market::function::SELL,
        &[
            Arg::Int(0x6F00_150B),
            Arg::Int(100),
            Arg::Int(8),
            Arg::LongInt(gem),
            Arg::LongInt(BROKER),
        ],
        10,
    );
    h.ask(
        b,
        market::function::ENTER_STORE,
        &[Arg::Int(0x6F00_150B), Arg::LongInt(BROKER)],
        10,
    );
    let mut g = yalain_world::graph::Builder::new();
    g.object(
        0x1B76,
        &[
            yalain_world::graph::Slot::Long(gem),
            yalain_world::graph::Slot::Ref(None),
            yalain_world::graph::Slot::Long(idb),
            yalain_world::graph::Slot::Int(8),
            yalain_world::graph::Slot::Long(BROKER),
            yalain_world::graph::Slot::Int(0),
            yalain_world::graph::Slot::Int(0),
        ],
    );
    h.ask(
        b,
        yalain_world::store::function::PURCHASE,
        &[Arg::Object(g.graph())],
        10,
    );
    let sale = h.server.market.list.first().map_or(0, |c| c.sale);
    h.ask(
        a,
        market::function::COLLECT,
        &[Arg::Int(1), Arg::Int(sale), Arg::LongInt(BROKER)],
        10,
    );
    // A story shard, its scenes done.
    put(&mut h, ida);
    h.ask(a, requests::function::USE, &[use_of(SHARD, ida)], 10);
    wait(&mut h, 14.0);
    h.ask(a, scenes::function::DONE, &[], 10);
    wait(&mut h, 4.0);
    vec![h.world_messages(a, 0), h.world_messages(b, 0)]
}

/// The script objects of a call's graphs, as (package, slots).
fn objects(args: &[Arg]) -> Vec<(u32, usize)> {
    let mut out = Vec::new();
    for a in args {
        if let Arg::Object(g) = a {
            for e in &g.entries {
                if let Body::Object(o) = &e.body {
                    if o.package != 0xFFFF {
                        out.push((u32::from(o.package), o.slots.len()));
                    }
                }
            }
        }
    }
    out
}

/// Every call this server sends for the allegiance, fellowships, trades, books, the story's
/// scenes and the market reads back in the client's own library: the values carry the tags of
/// the handler's declared parameters, and every script object is of its package's size.
/// Behaviour: server.social.calls-read-in-the-client-library
#[test]
fn every_call_of_a_social_day_reads_back_against_the_client_library() {
    let Some(lib) = library() else {
        println!("no September 2013 install here");
        return;
    };
    let mut checked: BTreeMap<String, usize> = BTreeMap::new();
    for m in a_social_day().into_iter().flatten() {
        let c = match m {
            ServerMessage::PlayerCall(c) => c,
            ServerMessage::VisualObjectCall(o) | ServerMessage::CellObjectCall(o) => o.call,
            _ => continue,
        };
        let name = lib
            .function_name(FunctionId(c.function.0))
            .unwrap_or_else(|| panic!("the library names {:#x}", c.function.0));
        let bytes = c.args.map(|a| a.0).unwrap_or_default();
        let values = decode(&bytes, lib).unwrap_or_else(|e| panic!("{name}: {e}"));
        let want = expected_tags(lib, &name, false).unwrap_or_else(|| panic!("{name} has a frame"));
        let got: Vec<u32> = values.iter().map(Arg::tag).collect();
        assert_eq!(got, want, "{name}");
        for (pkg, slots) in objects(&values) {
            assert_eq!(
                lib.package_size(pkg),
                Some(u32::try_from(slots).expect("small")),
                "{name}: package {pkg:#x}"
            );
        }
        *checked.entry(name).or_default() += 1;
    }
    for (name, n) in &checked {
        println!("{n:>4} {name}");
    }
    for want in [
        "WM_Allegiance::PostCEvt_UpdateAllegianceProfile",
        "WM_Allegiance::PostCEvt_RequestVassal",
        "WM_Allegiance::PostCEvt_Patron_SwearDone",
        "WM_Allegiance::PostCEvt_Vassal_SwearDone",
        "WM_Allegiance::PostCEvt_UpdateAllegianceChatDone",
        "WM_Allegiance::PostCEvt_HandleAllegianceHierarchyForExport",
        "WM_Fellowship::PostCEvt_UpdateFellowship",
        "WM_Fellowship::PostCEvt_UpdateFellow",
        "WM_Fellowship::PostCEvt_RequestRecruitment",
        "WM_Fellowship::PostCEvt_ClearFellow",
        "WM_Fellowship::PostCEvt_ClearFellowship",
        "WM_Fellowship::PostCEvt_UpdateFellowCellID",
        "WM_Trade::PostCEvt_Client_Trade_BeRegistered",
        "WM_Trade::PostCEvt_Client_Trade_BeOpened",
        "WM_Trade::PostCEvt_Client_Trade_BeOffered",
        "WM_Trade::PostCEvt_Client_Trade_BeAccepted",
        "WM_Trade::PostCEvt_Client_Trade_BeReset",
        "WM_Trade::PostCEvt_Client_Trade_BeClosed",
        "WM_Usage::PostCEvt_Usage_UseBook",
        "WM_Quest::PostCEvt_UpdateStoryQuest_Done",
        "WM_Quest::PostCEvt_PlayScenes",
        "WM_Store::PostCEvt_Store_EnterCatalog",
        "WM_Store::PostCEvt_Store_EnterConsignment",
        "WM_Store::PostCEvt_Store_UpdateConsignment",
        "WM_Store::PostCEvt_Store_InitSaleReminders",
        "WM_Store::PostCEvt_Store_EnterStore",
        "WM_Store::PostCEvt_Store_UpdateSale",
    ] {
        assert!(checked.contains_key(want), "{want} was sent");
    }
}

/// The recorded clients' requests, by function, from the January 2017 world sessions: each call's
/// values as this server reads them.
fn recorded_requests(con: &Connection, functions: &[u32]) -> BTreeMap<u32, Vec<Vec<Value>>> {
    let mut out: BTreeMap<u32, Vec<Vec<Value>>> = BTreeMap::new();
    let mut stmt = con
        .prepare(
            "SELECT m.raw FROM message m JOIN session s USING (session_id) JOIN capture c \
             USING (capture_id) WHERE c.file LIKE '2017-%' AND m.dir = 'c2s' AND m.status = 'ac2'",
        )
        .expect("sql");
    let rows = stmt
        .query_map([], |r| r.get::<_, Vec<u8>>(0))
        .expect("rows");
    for raw in rows.flatten() {
        let Ok(ClientMessage::ServerCall(c)) = ClientMessage::decode(&raw) else {
            continue;
        };
        if !functions.contains(&c.function.0) {
            continue;
        }
        let v = calls::values(&c.args.0).unwrap_or_default();
        out.entry(c.function.0).or_default().push(v);
    }
    out
}

/// The requests the recorded 2017 clients sent for these panels read in this server: a
/// fellowship's name, experience setting and looting; a recruit's, an oath's and a trade's other
/// player; an offer's item and how many; a trade's acceptance with what the trade held.
/// Behaviour: server.social.recorded-requests-read
#[test]
fn the_recorded_clients_requests_read_in_this_server() {
    use yalain_world::{allegiance, fellowship, trade};
    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let f = [
        fellowship::function::CREATE,
        fellowship::function::RECRUIT,
        fellowship::function::ACCEPT,
        allegiance::function::SWEAR,
        allegiance::function::ACCEPT_VASSAL,
        trade::function::OPEN,
        trade::function::OFFER,
        trade::function::ACCEPT,
    ];
    let got = recorded_requests(&con, &f);
    let long = |v: &[Value]| v.iter().any(|x| matches!(x, Value::Long(l) if *l != 0));
    let mut tally: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    for (func, calls) in &got {
        for v in calls {
            let ok = match *func {
                fellowship::function::CREATE => {
                    matches!(v.first(), Some(Value::Int(_)))
                        && matches!(v.get(1), Some(Value::Int(_)))
                        && v.iter()
                            .any(|x| matches!(x.root(), Some(Node::Text(t)) if !t.is_empty()))
                }
                trade::function::OFFER => {
                    matches!(v.first(), Some(Value::Int(n)) if *n > 0) && long(v)
                }
                trade::function::ACCEPT => trade::record_items(v).is_some(),
                _ => long(v),
            };
            let t = tally.entry(*func).or_default();
            if ok {
                t.0 += 1;
            } else {
                t.1 += 1;
            }
        }
    }
    for (func, (ok, bad)) in &tally {
        println!("{func:#010x}: {ok} read, {bad} not");
    }
    for (func, floor) in [
        (fellowship::function::CREATE, 30),
        (fellowship::function::RECRUIT, 55),
        (fellowship::function::ACCEPT, 60),
        (allegiance::function::SWEAR, 6),
        (allegiance::function::ACCEPT_VASSAL, 6),
        (trade::function::OPEN, 100),
        (trade::function::OFFER, 330),
        (trade::function::ACCEPT, 165),
    ] {
        let (ok, bad) = tally.get(&func).copied().unwrap_or_default();
        assert!(
            ok >= floor && bad == 0,
            "{func:#010x}: {ok} read, {bad} not"
        );
    }
}

/// A member's profile asked for by id was answered within a second in 2017, as this server
/// answers it; asked for nothing (id 0), it was not, and this server does not.
/// Behaviour: server.allegiance.profiles-asked-for
#[test]
fn recorded_profile_queries_by_id_were_answered_and_by_nothing_were_not() {
    use yalain_world::allegiance::function::{PROFILE, QUERY};
    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let sessions: Vec<i64> = con
        .prepare(
            "SELECT s.session_id FROM session s JOIN capture c USING (capture_id) \
             WHERE c.file LIKE '2017-%' AND s.game = 'ac2' AND s.login = 0",
        )
        .expect("sql")
        .query_map([], |r| r.get(0))
        .expect("sql")
        .collect::<Result<_, _>>()
        .expect("rows");
    let mut stmt = con
        .prepare(
            "SELECT t, dir, raw FROM message WHERE session_id = ?1 AND status = 'ac2' ORDER BY idx",
        )
        .expect("sql");
    let (mut id_answered, mut id_not, mut zero_answered, mut zero_not) = (0, 0, 0, 0);
    for sid in &sessions {
        let msgs: Vec<(f64, String, Vec<u8>)> = stmt
            .query_map([sid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .expect("rows")
            .collect::<Result<_, _>>()
            .expect("rows");
        let profiles: Vec<f64> = msgs
            .iter()
            .filter(|(_, d, _)| d == "s2c")
            .filter_map(|(t, _, raw)| match ServerMessage::decode(raw) {
                Ok(ServerMessage::PlayerCall(c)) if c.function.0 == PROFILE => Some(*t),
                _ => None,
            })
            .collect();
        let entered = profiles.first().copied().unwrap_or(f64::MAX);
        for (t, d, raw) in &msgs {
            if d != "c2s" {
                continue;
            }
            let Ok(ClientMessage::ServerCall(c)) = ClientMessage::decode(raw) else {
                continue;
            };
            if c.function.0 != QUERY {
                continue;
            }
            let target = calls::values(&c.args.0)
                .unwrap_or_default()
                .iter()
                .find_map(|x| match x {
                    Value::Long(l) => Some(*l),
                    _ => None,
                })
                .unwrap_or(0);
            let answered = profiles.iter().any(|p| *p >= *t && *p - *t < 1.0);
            // The entry's own profile is not an answer.
            let entry = (entered - *t).abs() < 1.0;
            match (target != 0, answered && !entry) {
                (true, true) => id_answered += 1,
                (true, false) => id_not += 1,
                (false, true) => zero_answered += 1,
                (false, false) => zero_not += 1,
            }
        }
    }
    println!("by id: {id_answered} answered, {id_not} not; by nothing: {zero_answered} answered, {zero_not} not");
    // 54 of 59 in 2017; the other five came within a second of the entry's own profile, which is
    // not counted as an answer here.
    assert!(id_answered >= 50 && id_not <= 6, "by id");
    assert!(
        zero_not >= 1_500 && zero_answered * 10 < zero_not,
        "by nothing"
    );
}

/// The recorded 2017 server calls of these functions, each call's values as this server reads them.
fn recorded_calls(con: &Connection, functions: &[u32]) -> Vec<(u32, Vec<Value>)> {
    let mut stmt = con
        .prepare(
            "SELECT m.raw FROM message m JOIN session s USING (session_id) JOIN capture c \
             USING (capture_id) WHERE c.file LIKE '2017-%' AND m.dir = 's2c' AND m.status = 'ac2'",
        )
        .expect("sql");
    let rows = stmt
        .query_map([], |r| r.get::<_, Vec<u8>>(0))
        .expect("rows");
    let mut out = Vec::new();
    for raw in rows.flatten() {
        let Ok(ServerMessage::PlayerCall(c)) = ServerMessage::decode(&raw) else {
            continue;
        };
        if functions.contains(&c.function.0) {
            let bytes = c.args.map(|a| a.0).unwrap_or_default();
            out.push((c.function.0, calls::values(&bytes).unwrap_or_default()));
        }
    }
    out
}

/// A native of a graph by its id: its type and bytes.
fn native_of(g: &[(u32, Node)], id: u32) -> Option<(u16, &[u8])> {
    g.iter().find_map(|(i, n)| match n {
        Node::Native { ty, bytes } if *i == id => Some((*ty, bytes.as_slice())),
        _ => None,
    })
}

fn word(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("four bytes"))
}

/// Every recorded 2017 allegiance profile names its monarch among its officers, and flags as an
/// officer exactly the members in its officer set, as this server's profiles do; every recorded
/// export of the tree files each member's name under the name's hash, as this server's export
/// does.
/// Behaviour: server.allegiance.recorded-officers-and-names
#[test]
fn recorded_profiles_flag_their_officers_and_exports_file_names_by_their_hash() {
    use auberean_script::native_object::BUCKET_SIZES;
    use yalain_world::allegiance::{function, name_hash};
    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let (mut profiles, mut flags, mut flags_off, mut monarchs, mut names, mut names_off) =
        (0, 0, 0, 0, 0, 0);
    for (f, v) in recorded_calls(&con, &[function::PROFILE, function::EXPORT]) {
        for g in v.iter().filter_map(|x| match x {
            Value::Graph(g) => Some(g),
            _ => None,
        }) {
            let Some((
                _,
                Node::Script {
                    package,
                    slots: root,
                },
            )) = g.first()
            else {
                continue;
            };
            if f == function::PROFILE && *package == 0x1B5 {
                let Some((0x1AC8, set)) = native_of(g, root[12].1) else {
                    continue;
                };
                profiles += 1;
                let officers: Vec<u64> = set[4..]
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .map(|c| u64::from_le_bytes(*c))
                    .collect();
                for (i, n) in g {
                    let Node::Script {
                        package: 0x1B1,
                        slots,
                    } = n
                    else {
                        continue;
                    };
                    let id = u64::from(slots[10].1) << 32 | u64::from(slots[9].1);
                    if (slots[8].1 != 0) == officers.contains(&id) {
                        flags += 1;
                    } else {
                        flags_off += 1;
                    }
                    if *i == root[3].1 || root[3].1 == u32::MAX && *i == root[2].1 {
                        monarchs += usize::from(officers.contains(&id));
                    }
                }
            }
            if f == function::EXPORT && *package == 0x1B2 {
                let Some((0x4D, table)) = native_of(g, root[8].1) else {
                    continue;
                };
                let buckets = BUCKET_SIZES[(word(table, 0) >> 24) as usize];
                for e in table[4..].as_chunks::<16>().0 {
                    let name = g.iter().find_map(|(i, n)| match n {
                        Node::Text(t) if *i == word(e, 0) => Some(t.clone()),
                        _ => None,
                    });
                    match name {
                        Some(n) if name_hash(&n) % buckets == word(e, 4) && word(e, 8) == 0 => {
                            names += 1;
                        }
                        _ => names_off += 1,
                    }
                }
            }
        }
    }
    println!(
        "{profiles} profiles: {flags} officer flags agree, {flags_off} not; the monarch an \
         officer in {monarchs}; export names filed by their hash {names}, not {names_off}"
    );
    assert!(
        profiles >= 400 && flags >= 1_400 && flags_off == 0,
        "officer flags"
    );
    assert!(
        monarchs * 10 >= profiles * 8,
        "the monarch among the officers"
    );
    assert!(names >= 20 && names_off == 0, "names filed by their hash");
}

/// A token for a server message: its type, and what tells messages of a type apart.
fn token(m: &ServerMessage) -> String {
    match m {
        ServerMessage::PlayerCall(c) => format!("call {:08X}", c.function.0),
        ServerMessage::UpdateInt(q) => format!("int {}", q.quality),
        ServerMessage::UpdateLongInt(q) => format!("long {}", q.quality),
        ServerMessage::UpdateInstanceIdVisual(q) => format!("iid {}", q.quality),
        ServerMessage::DoFx(e) => format!("fx {:08X}", e.action.fx),
        ServerMessage::StopFx(e) => format!("stopfx {:08X}", e.action.0),
        ServerMessage::Contain(_) => "contain".into(),
        ServerMessage::PlayBehavior(_) => "behavior".into(),
        ServerMessage::MoveTo(_) => "moveto".into(),
        other => format!("type {:05X}", other.message_type()),
    }
}

/// A family of messages held: the token that opens an occurrence, one its tick must hold, how
/// long it lasts, whether most recorded occurrences are expected in this server's order, and this
/// server's tokens for it in order.
struct Family {
    name: &'static str,
    opens: &'static str,
    needs: Option<&'static str>,
    window: f64,
    dominant: bool,
    ours: &'static [&'static str],
}

const TEXT: &str = "call 82050011";

const FAMILIES: &[Family] = &[
    Family {
        name: "an oath taken, as the patron sees it",
        opens: "call 8203002D",
        needs: None,
        window: 0.06,
        dominant: true,
        ours: &["call 8203002D", "iid 452", "call 8203002E"],
    },
    Family {
        name: "an oath taken, as the vassal sees it",
        opens: "call 82030030",
        needs: Some("call 82030033"),
        window: 0.06,
        dominant: true,
        ours: &[
            "call 82030030",
            "call 82030033",
            TEXT,
            "iid 452",
            "call 8203002E",
        ],
    },
    Family {
        name: "a fellow joining, as the others see it",
        opens: "call 820C0027",
        needs: None,
        window: 0.06,
        dominant: true,
        ours: &["call 820C0027", TEXT],
    },
    Family {
        name: "a fellow leaving, as the others see it",
        opens: "call 820C0026",
        needs: None,
        window: 0.06,
        dominant: true,
        ours: &["call 820C0026", TEXT],
    },
    Family {
        name: "a fellowship left or disbanded",
        opens: "call 820C0028",
        needs: None,
        window: 0.06,
        dominant: true,
        ours: &["call 820C0028", TEXT, TEXT],
    },
    Family {
        name: "a trade's items exchanged",
        opens: "call 82150050",
        needs: Some("call 82150052"),
        // The reset came a median 0.067 s after the second acceptance.
        window: 0.4,
        dominant: true,
        ours: &[
            "call 82150050",
            TEXT,
            "call 820E005E",
            "call 82150052",
            TEXT,
        ],
    },
    Family {
        name: "a story shard's scene",
        opens: "call 82130023",
        needs: Some("call 82130021"),
        window: 0.06,
        dominant: true,
        ours: &[
            "call 82130023",
            "call 82120021",
            "call 8212001F",
            "call 82080017",
            TEXT,
            "call 8212001F",
            "call 82120021",
            "call 82120020",
        ],
    },
    Family {
        name: "a book read from the pack",
        opens: "call 8216002E",
        needs: None,
        window: 0.06,
        dominant: true,
        ours: &["call 8216002E", "call 82160021"],
    },
    Family {
        name: "a broker's catalog",
        opens: "call 82160021",
        needs: Some("call 9B5E0052"),
        window: 0.06,
        dominant: true,
        ours: &["call 82160021", "call 9B5E0052"],
    },
    Family {
        name: "a consignment put up",
        opens: "int 3100",
        needs: Some("call 9B5E0057"),
        window: 0.06,
        dominant: true,
        ours: &[
            "int 3100",
            "call 820E005E",
            "call 9B5E005D",
            "call 9B5E0057",
        ],
    },
];

/// Is `want` a subsequence of `have`?
fn in_order(have: &[String], want: &[&str]) -> Result<(), String> {
    let mut i = 0;
    for w in want {
        match have[i..].iter().position(|h| h == w) {
            Some(k) => i += k + 1,
            None => return Err(format!("no {w}")),
        }
    }
    Ok(())
}

/// The recorded families of the January 2017 world sessions hold the shapes this server sends:
/// each occurrence's tick holds this server's messages in this server's order, or is counted with
/// what it lacks.
/// Behaviour: server.social.recorded-families-by-shape
#[test]
fn the_recorded_social_families_hold_the_shapes_this_server_sends() {
    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let ours: Vec<Vec<String>> = a_social_day()
        .iter()
        .map(|v| v.iter().map(token).collect())
        .collect();
    for f in FAMILIES {
        assert!(
            ours.iter().any(|t| in_order(t, f.ours).is_ok()),
            "ours: {}",
            f.name
        );
    }
    let sessions: Vec<i64> = con
        .prepare(
            "SELECT s.session_id FROM session s JOIN capture c USING (capture_id) \
             WHERE c.file LIKE '2017-%' AND s.game = 'ac2' AND s.login = 0",
        )
        .expect("sql")
        .query_map([], |r| r.get(0))
        .expect("sql")
        .collect::<Result<_, _>>()
        .expect("rows");
    let mut tally: BTreeMap<(&str, String), usize> = BTreeMap::new();
    let mut stmt = con
        .prepare("SELECT t, raw FROM message WHERE session_id = ?1 AND dir = 's2c' AND status = 'ac2' ORDER BY idx")
        .expect("sql");
    for sid in &sessions {
        let msgs: Vec<(f64, Vec<u8>)> = stmt
            .query_map([sid], |r| Ok((r.get(0)?, r.get(1)?)))
            .expect("rows")
            .collect::<Result<_, _>>()
            .expect("rows");
        let toks: Vec<(f64, String)> = msgs
            .iter()
            .filter_map(|(t, raw)| Some((*t, token(&ServerMessage::decode(raw).ok()?))))
            .collect();
        for f in FAMILIES {
            for (i, (t, tok)) in toks.iter().enumerate() {
                if tok != f.opens {
                    continue;
                }
                let tick: Vec<String> = toks[i..]
                    .iter()
                    .take_while(|(u, _)| u - t < f.window)
                    .map(|(_, s)| s.clone())
                    .collect();
                if let Some(n) = f.needs {
                    if !tick.iter().any(|x| x == n) {
                        continue;
                    }
                }
                let outcome = match in_order(&tick, f.ours) {
                    Ok(()) => "in this server's order".to_string(),
                    Err(e) => e,
                };
                *tally.entry((f.name, outcome)).or_default() += 1;
            }
        }
    }
    for ((name, outcome), n) in &tally {
        println!("{n:>7}  {name}: {outcome}");
    }
    for f in FAMILIES {
        let ok = tally
            .get(&(f.name, "in this server's order".to_string()))
            .copied()
            .unwrap_or(0);
        let all: usize = tally
            .iter()
            .filter(|((n, _), _)| *n == f.name)
            .map(|(_, v)| *v)
            .sum();
        assert!(ok > 0, "{}: our shape is recorded ({ok} of {all})", f.name);
        if f.dominant {
            assert!(
                ok * 2 > all,
                "{}: most recorded occurrences hold our shape ({ok} of {all})",
                f.name
            );
        }
    }
}
