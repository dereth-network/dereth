//! What the January 2017 servers listed of a character's worn and wielded items at entry, held to
//! what this server builds: every recorded session start's inventory tables, read through the
//! September 2013 client library.

use std::path::Path;
use std::sync::OnceLock;

use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::{require_install, TARGET};
use auberean_dat::DatFile;
use auberean_protocol::ServerMessage;
use auberean_script::args::{decode, Arg};
use auberean_script::graph::{Body, Graph, PackedObject};
use auberean_script::library::{Library, RECORD};
use auberean_script::wire::Out;
use rusqlite::{Connection, OpenFlags};
use yalain_world::session::{
    filled_places, function, package, Registry, SessionStart, Worn, NEW_SEGMENTS,
};

fn library() -> Option<&'static Library> {
    static L: OnceLock<Option<Library>> = OnceLock::new();
    L.get_or_init(|| {
        let dir = require_install(TARGET);
        let (_, path) = install_dats(&dir)
            .ok()?
            .into_iter()
            .find(|(n, _)| *n == DatName::Portal)?;
        let dat = DatFile::open(&path).ok()?;
        Library::from_record(&dat.read(RECORD).ok()?).ok()
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

fn object(g: &Graph, id: u32) -> Option<&PackedObject> {
    let i = g.ids.iter().position(|x| *x == id)?;
    match &g.entries[i].body {
        Body::Object(o) => Some(o),
        _ => None,
    }
}

/// An inventory profile and what it reaches, without the graph's own ids: the profile's tags and
/// its slots after its look, the look's tags, and each of the look's natives packed.
type Shape = (Vec<u8>, Vec<u32>, Vec<u8>, Vec<Vec<u8>>);

fn shape(g: &Graph, profile: &PackedObject) -> Option<Shape> {
    let look = object(g, *profile.slots.first()?)?;
    let natives = look
        .slots
        .iter()
        .map(|id| {
            let n = object(g, *id)?.native.as_ref()?;
            let mut w = Out::default();
            n.write(&mut w).ok()?;
            Some(w.buf)
        })
        .collect::<Option<Vec<_>>>()?;
    Some((
        profile.tags.clone(),
        profile.slots[1..].to_vec(),
        look.tags.clone(),
        natives,
    ))
}

/// The profiles of an inventory table's graph.
fn profiles(g: &Graph) -> Vec<&PackedObject> {
    g.entries
        .iter()
        .filter_map(|e| match &e.body {
            Body::Object(o) if o.package == package::INVENTORY_PROFILE.0 => Some(o),
            _ => None,
        })
        .collect()
}

/// This server's session start with one worn item.
fn ours(w: Worn) -> Vec<Arg> {
    SessionStart {
        server_faction: 0,
        local_faction: 1,
        skills: Vec::new(),
        options: Vec::new(),
        segments: NEW_SEGMENTS.to_vec(),
        money: 0,
        contents: Vec::new(),
        quests: Vec::new(),
        credits: 0,
        effects: Registry::default(),
        scenes: Vec::new(),
        worn: vec![w],
        last_used: Vec::new(),
    }
    .args()
}

#[derive(Debug, Default)]
struct Tally {
    starts: usize,
    filled_agree: usize,
    weapons: usize,
    weapons_agree: usize,
    others: usize,
    others_agree: usize,
    mismatches: Vec<String>,
}

/// Every recorded session start lists its worn items as this server lists them: the places filled
/// are every item's places together, and each item's entry in both tables (its look, the places
/// it takes, where it is, its implement, the item) is what this server builds from those values,
/// byte for byte apart from the graph's ids: a wielded weapon with an empty look, an item worn on
/// the body with the appearance it gives its wearer (or none).
/// Behaviour: server.world.worn-items-listed-as-2017-listed-them
#[test]
fn every_recorded_worn_or_wielded_item_is_listed_as_this_server_lists_it() {
    let path = auberean_testkit::data::require_capture_index();
    let Some(lib) = library() else {
        panic!("the client library does not read from the install");
    };
    let con = open(&path);
    let mut st = con
        .prepare(
            "SELECT m.raw FROM message m JOIN session s USING (session_id) JOIN capture c USING (capture_id) \
             WHERE c.file LIKE '2017-%' AND m.dir = 's2c' AND m.status = 'ac2' AND m.mtype = 65638",
        )
        .expect("the query prepares");
    let rows = st
        .query_map([], |r| r.get::<_, Vec<u8>>(0))
        .expect("the query runs");
    let mut t = Tally::default();
    for raw in rows.flatten() {
        let Ok(ServerMessage::PlayerCall(c)) = ServerMessage::decode(&raw) else {
            continue;
        };
        if c.function.0 != function::SESSION_START {
            continue;
        }
        let Some(args) = c.args.as_ref().and_then(|a| decode(&a.0, lib).ok()) else {
            t.mismatches
                .push("a session start that does not read".into());
            continue;
        };
        t.starts += 1;
        let (Some(Arg::Int(filled)), Some(Arg::Object(by_place)), Some(Arg::Object(by_item))) =
            (args.get(6), args.get(7), args.get(8))
        else {
            t.mismatches.push("a session start of another shape".into());
            continue;
        };
        let worn: Vec<Worn> = profiles(by_item)
            .iter()
            .map(|p| Worn {
                taken: p.slots[1],
                location: p.slots[2],
                implement: p.slots[3],
                item: u64::from(p.slots[5]) << 32 | u64::from(p.slots[4]),
                look: None,
            })
            .collect();
        if filled_places(&worn) == *filled {
            t.filled_agree += 1;
        }
        for (table, recorded) in [(7, by_place), (8, by_item)] {
            for p in profiles(recorded) {
                let mut w = Worn {
                    taken: p.slots[1],
                    location: p.slots[2],
                    implement: p.slots[3],
                    item: u64::from(p.slots[5]) << 32 | u64::from(p.slots[4]),
                    look: None,
                };
                let weapon = matches!(w.location, 64 | 128);
                if weapon {
                    t.weapons += 1;
                } else {
                    // Worn on the body: its look is the appearance it lists.
                    t.others += 1;
                    w.look = shape(recorded, p).and_then(|s| look_of(&s.3[1]));
                }
                let built = ours(w.clone());
                let Some(Arg::Object(g)) = built.get(table) else {
                    continue;
                };
                let mine = profiles(g).first().and_then(|q| shape(g, q));
                if mine.is_some() && mine == shape(recorded, p) {
                    if weapon {
                        t.weapons_agree += 1;
                    } else {
                        t.others_agree += 1;
                    }
                } else if t.mismatches.len() < 8 {
                    t.mismatches.push(format!(
                        "table {table}, item {:#x}: {:?} against {:?}",
                        w.item,
                        mine,
                        shape(recorded, p)
                    ));
                }
            }
        }
    }
    println!("{t:#?}");
    assert!(t.starts > 0, "the index holds the 2017 session starts");
    assert_eq!(t.filled_agree, t.starts, "the places filled: {t:#?}");
    assert!(t.weapons > 0, "wielded weapons recorded");
    assert_eq!(t.weapons_agree, t.weapons, "every weapon's entry: {t:#?}");
    assert!(t.others > 0, "worn armor recorded");
    assert_eq!(t.others_agree, t.others, "every worn item's entry: {t:#?}");
}

/// The look a recorded profile's appearance native lists: its one appearance and values (none
/// for an empty table).
fn look_of(packed: &[u8]) -> Option<(u32, auberean_protocol::table::Table<f32>)> {
    use auberean_protocol::table::Table;
    use auberean_protocol::wire::{Field, Reader};
    let mut r = Reader::new(packed);
    let t = Table::<Table<f32>>::read(&mut r).ok()?;
    r.finish().ok()?;
    t.entries.into_iter().next()
}
