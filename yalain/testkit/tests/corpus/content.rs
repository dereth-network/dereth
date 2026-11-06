//! The world's data checked against the September 2013 install and the recordings: objects
//! described from their templates as the 2017 servers made them, the session start the server
//! builds read back by the client's own library, where new characters start.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

use auberean_assets::entity::Entity;
use auberean_assets::script::{Qualities, ScriptState};
use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::{require_install, TARGET};
use auberean_dat::DatFile;
use auberean_protocol::types::{PhysicsDesc, VisualDesc, WeenieDesc};
use auberean_protocol::ServerMessage;
use auberean_script::args::{decode, expected_tags};
use auberean_script::graph::Graph;
use auberean_script::library::{Library, RECORD};
use rusqlite::{Connection, OpenFlags};
use yalain_content::describe;
use yalain_world::options::Options;
use yalain_world::session::{package, SessionStart, NEW_SEGMENTS};
use yalain_world::StartRules;

struct Install {
    dat: DatFile,
    ctx: Context,
    lib: Library,
}

fn installed() -> Option<&'static Install> {
    static I: OnceLock<Option<Install>> = OnceLock::new();
    I.get_or_init(|| {
        let dir = require_install(TARGET);
        let (_, path) = install_dats(&dir)
            .ok()?
            .into_iter()
            .find(|(n, _)| *n == DatName::Portal)?;
        let dat = DatFile::open(&path).ok()?;
        let ctx = Context::from_portal(&dat).ok()?;
        let lib = dat.read_checked(RECORD, Library::from_record).ok()?.value;
        Some(Install { dat, ctx, lib })
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

fn entity(i: &Install, id: u32) -> Option<Entity> {
    auberean_assets::decode(id, &i.dat.read(id).ok()?, &i.ctx).ok()
}

fn qualities(i: &Install, id: u32) -> Option<Qualities> {
    auberean_assets::decode(id, &i.dat.read(id).ok()?, &i.ctx).ok()
}

/// The start rules, read from the install's character generation data.
fn start_rules(i: &Install) -> StartRules {
    let bytes = i
        .dat
        .read(0x7000_0390)
        .expect("the character generation data");
    let st: ScriptState = auberean_assets::decode(0x7000_0390, &bytes, &i.ctx).expect("reads");
    let g = Graph::decode(&st.graph, &i.lib).expect("its graph reads");
    StartRules::from_matrix(&i.lib, &g).expect("the rules read")
}

/// Every object the 2017 servers created that is drawn as its template's look alone (not a
/// player, not dressed): its template's description from the install gives the same look, the
/// same package and the same physics words, and the entry effect the server showed when one
/// appeared.
/// Behaviour: server.content.described-from-templates
#[test]
fn objects_are_described_from_their_templates_as_the_2017_servers_made_them() {
    let path = auberean_testkit::data::require_capture_index();
    let Some(inst) = installed() else {
        println!("no September 2013 install here");
        return;
    };
    let con = open(&path);
    let mut st = con
        .prepare(
            "SELECT m.raw FROM message m JOIN session s USING (session_id) JOIN capture c USING (capture_id) \
             WHERE c.file LIKE '2017-%' AND m.dir = 's2c' AND m.mtype = 65624",
        )
        .expect("prepares");
    let mut seen: BTreeMap<u32, (VisualDesc, PhysicsDesc, WeenieDesc)> = BTreeMap::new();
    let mut creates = 0usize;
    let rows = st.query_map([], |r| r.get::<_, Vec<u8>>(0)).expect("runs");
    for raw in rows.map_while(Result::ok) {
        let Ok(ServerMessage::CreateObject(o)) = ServerMessage::decode(&raw) else {
            continue;
        };
        if o.id >> 48 == 0x2130
            || o.visual.bits != VisualDesc::PARENT
            || o.weenie.mask & WeenieDesc::ENTITY == 0
        {
            continue;
        }
        creates += 1;
        seen.entry(o.weenie.entity_did)
            .or_insert((o.visual, o.physics, o.weenie));
    }
    let mut tally: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    let mut count = |what: &'static str, same: bool| {
        let t = tally.entry(what).or_default();
        t.0 += usize::from(same);
        t.1 += 1;
    };
    let mut undescribed = 0;
    for (template, (v, p, w)) in &seen {
        let Ok(d) = describe(*template, &|id| entity(inst, id), &|id| qualities(inst, id)) else {
            undescribed += 1;
            continue;
        };
        count("look", d.look == v.parent);
        count("package", d.package == w.package_id);
        count("physics words", d.numbers[..] == w.numbers[5..11]);
        if p.bits & PhysicsDesc::FX != 0 {
            count(
                "the entry effect, where one was shown",
                Some(d.enter_fx) == p.fx.first().map(|f| f.fx),
            );
        }
        count(
            "named by the template alone",
            w.mask & WeenieDesc::NAME == 0,
        );
        count(
            "scaled where the template scales",
            d.scale.is_some() == (w.mask & WeenieDesc::SCALE != 0),
        );
    }
    println!(
        "{creates} creates of {} templates; {undescribed} not described",
        seen.len()
    );
    for (what, (same, of)) in &tally {
        println!("  {what}: {same} of {of}");
    }
    assert!(seen.len() > 1000);
    let (look, of) = tally["look"];
    assert!(
        look * 100 >= of * 99,
        "the look of {look} of {of} templates"
    );
    let (package, of) = tally["package"];
    assert!(
        package * 100 >= of * 99,
        "the package of {package} of {of} templates"
    );
}

/// A template whose qualities name a visual description itself as its body (the haunted and mossy
/// Emperian doors) is drawn as that description, as the 2017 servers sent it.
/// Behaviour: server.content.body-a-visual-description
#[test]
fn a_body_that_is_a_visual_description_is_the_look() {
    let Some(inst) = installed() else {
        println!("no September 2013 install here");
        return;
    };
    for (template, look) in [(0x4700_0FC3, 0x1F00_0553), (0x4700_0FC4, 0x1F00_0554)] {
        let d = describe(template, &|id| entity(inst, id), &|id| qualities(inst, id))
            .expect("described");
        assert_eq!(d.look, look, "{template:08X}");
    }
}

/// The session start the server builds for a new character reads back against the client's own
/// library: fourteen values with the tags of the handler's declared parameters, every script
/// object of its package's size.
/// Behaviour: server.world.session-start-reads-in-the-client-library
#[test]
fn the_session_start_reads_back_against_the_client_library() {
    let Some(inst) = installed() else {
        println!("no September 2013 install here");
        return;
    };
    for (pkg, size) in package::ALL {
        assert_eq!(
            inst.lib.package_size(u32::from(pkg)),
            Some(u32::try_from(size).expect("small")),
            "package {pkg:#x}"
        );
    }
    let rules = start_rules(inst);
    let skills = rules.skills[&1].clone();
    let s = SessionStart {
        server_faction: 0,
        local_faction: 1,
        skills: yalain_world::skills::held(&skills, &[]),
        options: Options::new_character(&skills).to_packed(),
        segments: NEW_SEGMENTS.to_vec(),
        money: 0,
        contents: Vec::new(),
        quests: Vec::new(),
        credits: 0,
        hero_credits: 0,
        effects: yalain_world::session::Registry::default(),
        scenes: vec![(0x4000_0008, vec![0x4000_0021])],
        worn: vec![yalain_world::session::Worn {
            item: 0x8130_0000_0000_0001,
            location: 64,
            taken: 192,
            implement: 4,
            look: None,
        }],
        last_used: vec![(skills[0].0, 1.25e8)],
    };
    let bytes = auberean_script::args::encode(&s.args()).expect("packs");
    let values = decode(&bytes, &inst.lib).expect("the client's library reads it");
    let want = expected_tags(
        &inst.lib,
        "WM_Player::PostCEvt_HandleCharacterSessionStart",
        false,
    )
    .expect("a frame");
    let got: Vec<u32> = values.iter().map(auberean_script::args::Arg::tag).collect();
    assert_eq!(got, want);
}

/// The install's character generation data starts a human in one of the eighteen copies of the
/// training hall (landblocks 0x0288 to 0x0299), with its race's starting skills.
/// Behaviour: server.world.start-from-the-character-data
#[test]
fn new_humans_start_in_a_copy_of_the_training_hall_with_their_races_skills() {
    let Some(inst) = installed() else {
        println!("no September 2013 install here");
        return;
    };
    let rules = start_rules(inst);
    let area = rules.area_for(1, 0).expect("a human start area");
    assert_eq!(area.places.len(), 18);
    assert!(area
        .places
        .iter()
        .all(|p| (0x0288..=0x0299).contains(&(p.cell >> 16)) && p.cell & 0xFFFF == 0x0109));
    let skills = &rules.skills[&1];
    assert_eq!(&skills[..4], &[(835, 0), (96, 0), (176, 0), (207, 0)]);
    for race in [1, 2, 4, 8, 16] {
        assert!(
            rules.area_for(race, 0).is_some(),
            "race {race} starts somewhere"
        );
        assert!(!rules.skills[&race].is_empty(), "race {race} has skills");
    }
    // Whatever start area a client names, a character of a race it can make has a place: each of
    // its race's areas holds one, and any other falls back to the first.
    for (race, _, _) in yalain_world::chargen::BODY_ENTITIES {
        let kinds = &rules.by_race[&race];
        assert!(!kinds.is_empty(), "race {race} has start areas");
        for kind in kinds {
            let area = rules.area_for(race, *kind).expect("an area");
            assert!(
                !area.places.is_empty(),
                "race {race} area {kind} has places"
            );
        }
    }
    println!(
        "start areas: {}",
        rules
            .areas
            .values()
            .map(|a| format!("{} ({} places)", a.name, a.places.len()))
            .collect::<Vec<_>>()
            .join(", ")
    );
}
