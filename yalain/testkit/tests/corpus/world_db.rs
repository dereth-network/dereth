//! Behaviour: none (checks the world database compiled against the September 2013 install, and served)
//!
//! The database is read from `YALAIN_TEST_WORLD_DB` (a checkout), else from the release this build
//! pins in the per-user cache (`yalain world fetch`); without either, a test fails saying so.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::{require_install, TARGET};
use auberean_dat::DatFile;
use auberean_protocol::{queue, ServerMessage};
use auberean_script::library::{Library, RECORD};
use yalain_content::Kind;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world_db::compile::{compile, Tables};
use yalain_world_db::install::DatInstall;
use yalain_world_db::release::{cache_dir_for, Host, PINNED_TAG};
use yalain_world_db::{check, merge, rows, Files};

/// The variable naming a world database checkout for the tests.
const WORLD_DB_VAR: &str = "YALAIN_TEST_WORLD_DB";

fn database() -> PathBuf {
    if let Some(v) = std::env::var_os(WORLD_DB_VAR).filter(|v| !v.is_empty()) {
        return PathBuf::from(v);
    }
    let cached =
        cache_dir_for(Host::current(), |n| std::env::var_os(n)).map(|c| c.join(PINNED_TAG));
    match cached {
        Some(d) if d.join("world.toml").is_file() => d,
        _ => panic!(
            "no world database: set {WORLD_DB_VAR} to a checkout, or fetch the pinned release ({PINNED_TAG}) with `yalain world fetch`"
        ),
    }
}

fn load(root: &Path) -> Files {
    let mut out = Files::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d)
            .expect("the database's folder")
            .flatten()
        {
            let p = e.path();
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let rel = p
                .strip_prefix(root)
                .expect("inside")
                .to_string_lossy()
                .replace('\\', "/");
            if rel.ends_with(".toml") || rel == "world.lock" {
                out.insert(rel, std::fs::read_to_string(&p).expect("utf-8"));
            }
        }
    }
    out
}

struct Compiled {
    merged: merge::Merged,
    tables: Tables,
}

/// The pinned release in the per-user cache, if it is there.
fn pinned_database() -> Option<PathBuf> {
    cache_dir_for(Host::current(), |n| std::env::var_os(n))
        .map(|c| c.join(PINNED_TAG))
        .filter(|d| d.join("world.toml").is_file())
}

/// The database, checked, merged and compiled against the install, once.
fn compiled() -> &'static Compiled {
    static C: OnceLock<Compiled> = OnceLock::new();
    C.get_or_init(|| compile_at(&database()))
}

/// The pinned release compiled, once (the database itself when it is the pinned release).
fn pinned() -> &'static Compiled {
    static P: OnceLock<Compiled> = OnceLock::new();
    let dir = pinned_database().unwrap_or_else(|| {
        panic!("no pinned release: fetch it ({PINNED_TAG}) with `yalain world fetch`")
    });
    if dir == database() {
        return compiled();
    }
    P.get_or_init(|| compile_at(&dir))
}

/// The install a compile reads, with its text.
fn dat_install() -> DatInstall<'static> {
    let dir = require_install(TARGET);
    let found = install_dats(&dir).expect("the install");
    let open = |n: &DatName| {
        DatFile::open(&found.iter().find(|(m, _)| m == n).expect("a data file").1).expect("opens")
    };
    let portal = open(&DatName::Portal);
    let local = open(&DatName::Local("English".into()));
    let ctx = Context::from_portal(&portal).expect("context");
    let states = portal
        .entries_in(0x6F00_0000, 0x6FFF_FFFF)
        .map(|e| e.id)
        .collect();
    static LIBRARY: OnceLock<Library> = OnceLock::new();
    let lib = LIBRARY.get_or_init(|| {
        Library::from_record(&portal.read(RECORD).expect("the library")).expect("the library reads")
    });
    DatInstall::new(Box::new(move |id| portal.read(id).ok()), ctx, lib, states)
        .expect("install")
        .with_text(Box::new(move |id| local.read(id).ok()))
}

/// A database checked, merged and compiled against the install.
fn compile_at(root: &Path) -> Compiled {
    {
        let files = load(root);
        let problems = check::check(&files);
        assert!(
            problems.is_empty(),
            "{}",
            yalain_world_db::report(&problems)
        );
        let db = rows::read(&files).expect("read");
        let merged = merge::merge(&db).expect("merge");
        let install = dat_install();
        let tables = compile(&merged, &install)
            .unwrap_or_else(|p| panic!("{}", yalain_world_db::report(&p)));
        Compiled { merged, tables }
    }
}

/// Every row of the database compiles against the install: each template, store listing,
/// quest definition and profile it names is there, and the quests' subquests are added. The only
/// rows left out are objects of templates the client draws as nothing.
#[test]
fn every_row_of_the_world_database_compiles_against_the_install() {
    let c = compiled();
    let count = |k: &str| c.merged.rows.iter().filter(|l| l.row.kind() == k).count();
    let t = &c.tables;
    let left_out = t
        .warnings
        .iter()
        .filter(|w| w.message.starts_with("not described"))
        .count();
    assert_eq!(t.content.objects.len() + left_out, count("object"));
    assert!(
        t.warnings
            .iter()
            .all(|w| w.message.contains("has no visual description")),
        "{}",
        yalain_world_db::report(&t.warnings)
    );
    assert_eq!(t.spawns.camps.len(), count("camp"));
    assert_eq!(t.loot.profiles.len(), count("loot"));
    assert_eq!(t.stores.keepers.len(), count("keeper"));
    assert_eq!(t.stores.sales.len(), count("sale"));
    // Beyond the rows' quests, the subquests their definitions name, each hidden.
    let (subquests, rows): (Vec<_>, Vec<_>) = t
        .quests
        .quests
        .iter()
        .partition(|q| q.placed.starts_with("derived subquest"));
    assert_eq!(rows.len(), count("quest"));
    assert!(subquests.iter().all(|q| q.hidden));
    assert_eq!(t.quests.steps.len(), count("step"));
    assert_eq!(t.talk.lines.len(), count("talk"));
    // Every object is drawn as something.
    for o in &t.content.objects {
        assert_ne!(o.description.look, 0, "{:08X}", o.template);
    }
}

/// What the rows leave out comes from the install: the training hall quest's definition (its
/// eight journal entries), the Human Weapon Vendor's store and its listings' products and prices,
/// every loot profile's name, and the people's names for reading.
#[test]
fn what_the_rows_leave_out_is_read_from_the_install() {
    let t = &compiled().tables;
    let q = t
        .quests
        .quests
        .iter()
        .find(|q| q.id == 0x4000_0088)
        .expect("the training hall quest");
    assert_eq!(
        (q.journals.len(), q.icon, q.challenge, q.duration),
        (8, 0x4100_034B, -999, 72000.0)
    );
    let keeper = t
        .stores
        .keepers
        .iter()
        .find(|k| k.template == 0x4700_33D0)
        .expect("the Human Weapon Vendor");
    assert_eq!(keeper.store, 0x6F00_16F6);
    let sales = t.stores.sales_of(0x6F00_16F6);
    assert!(sales.len() >= 30, "{}", sales.len());
    assert!(sales
        .iter()
        .all(|s| s.currency == 0x4700_0538 && s.cost > 0.0));
    assert!(t.loot.profiles.iter().all(|p| p.name.starts_with("GP_")));
    let people: Vec<_> = t
        .content
        .objects
        .iter()
        .filter(|o| o.kind == Kind::Npc)
        .collect();
    let unnamed = people.iter().filter(|o| o.label.is_empty()).count();
    assert!(people.len() > 100);
    assert!(
        unnamed * 100 <= people.len(),
        "{unnamed} of {} people have no name to read",
        people.len()
    );
}

/// Who a store's sales are for comes from the install: the honey shop and the fur trader hide
/// what a player may not have, the sealed orders do not; the honey shop's last two sales need a
/// quest finished, each of the fur trader's glasses its race, each sealed order its quest under
/// way. The pinned release's stores that hide them are the honey shop and the mercenaries'.
/// Behaviour: server.store.sales-for-others-from-the-install
#[test]
fn who_a_stores_sales_are_for_is_read_from_the_install() {
    use yalain_world_db::compile::Install as _;
    let install = dat_install();
    let gates = |store: u32| install.store_gates(store);
    let (honey, hides) = gates(0x6F00_16EC);
    assert!(hides);
    let quest = |q: u32, st: u32| yalain_content::stores::SaleGate {
        quests: vec![(q, st)],
        ..Default::default()
    };
    assert_eq!(
        honey,
        [
            (2, quest(0x8000_0342, 0x3000_0000)),
            (3, quest(0x8000_0342, 0x3000_0000))
        ]
    );
    let (furs, hides) = gates(0x6F00_16ED);
    assert!(hides);
    let races: Vec<(u32, u32)> = furs.iter().map(|(o, g)| (*o, g.race)).collect();
    assert_eq!(races, [(0, 2), (1, 4), (2, 1), (3, 16)]);
    let (orders, hides) = gates(0x6F00_16EB);
    assert!(!hides);
    assert_eq!(orders.len(), 4);
    assert!(orders
        .iter()
        .all(|(_, g)| g.quests.len() == 1 && g.quests[0].1 == 0x1000_0000));
    assert_eq!(
        gates(0x6F00_16F6),
        (Vec::new(), false),
        "a weapon shop sells to all"
    );
    let mut hiding = pinned().tables.stores.hiding.clone();
    hiding.sort_unstable();
    assert_eq!(hiding, [0x6F00_16EC, 0x6F00_16ED, 0x6F00_16EE]);
}

/// The compiled world, served: a new character starts in the training hall copy the database
/// fills, finds its people there, and the Shelter Guard gives the training quest whose definition
/// the install holds.
#[test]
fn a_new_character_starts_in_the_compiled_hall_and_its_guard_gives_the_quest() {
    let c = compiled();
    let t = &c.tables;
    let (mut h, b, me) = served(t);
    let i = person(t, 0x4700_2DFD);
    let guard = yalain_world::presence::content_id(&t.content.objects, i);
    let seen = h.world_messages(b, 0);
    assert!(
        seen.iter()
            .any(|m| matches!(m, ServerMessage::CreateObject(o) if o.id == guard)),
        "the guard is in view: the character starts in the copy the database fills"
    );
    let calls = use_person(&mut h, b, me, t, i);
    assert!(
        calls.iter().any(|(f, _)| *f == 0x8205_0017),
        "the guard speaks: {calls:08X?}"
    );
    assert!(
        calls.iter().any(|(f, _)| *f == 0x8213_0024),
        "the quest is given: {calls:08X?}"
    );
}

/// The training hall quest's first steps, as the database gives them, move the quest on: the
/// Shelter Guard gives it, the Lieutenant and the Sentry each move it on a phase, each with the
/// line the step names, and a person used out of turn moves nothing.
#[test]
fn the_hall_quest_s_first_steps_from_the_database_move_it_on() {
    let t = &compiled().tables;
    let (mut h, b, me) = served(t);
    let phase = |h: &Harness| {
        h.server.players.values().next().map_or(0, |p| {
            yalain_world::quests::phase_of(&p.play.quests, 0x4000_0088)
        })
    };
    // The Sentry before the Lieutenant: nothing.
    let sentry = person(t, 0x4700_2E05);
    let calls = use_person(&mut h, b, me, t, sentry);
    assert!(
        !calls.iter().any(|(f, _)| *f == 0x8213_0024),
        "{calls:08X?}"
    );
    for (template, phase_after) in [(0x4700_2DFD, 1), (0x4700_2DFF, 2), (0x4700_2E05, 3)] {
        let step = t
            .quests
            .steps
            .iter()
            .find(|s| {
                s.quest == 0x4000_0088
                    && s.trigger == yalain_content::quests::Trigger::Use(template)
            })
            .expect("a step of the person");
        let calls = use_person(&mut h, b, me, t, person(t, template));
        let tells: Vec<u32> = calls
            .iter()
            .filter(|(f, _)| *f == 0x8205_0017)
            .filter_map(|(_, s)| *s)
            .collect();
        if let Some(yalain_content::Words::Ref(s, _)) = step.tell {
            assert_eq!(
                tells.first(),
                Some(&s),
                "{template:08X} says its step's line"
            );
        }
        assert!(calls.iter().any(|(f, _)| *f == 0x8213_0024));
        assert_eq!(phase(&h), phase_after, "after {template:08X}");
    }
}

/// The start area's quests come whole from the database: the training hall quest's steps to Arwic,
/// among them a held item used on an object that takes the player to the vault's chamber and an
/// arrival through the hall's shard; and the people and objects the other start-area quests are
/// moved on by (the other towns' explorers, the nine obelisks, the miller, the warders) stand in
/// the world.
#[test]
fn the_start_area_s_quests_and_the_people_they_name_come_from_the_database() {
    use yalain_content::quests::Trigger;
    let t = &compiled().tables;
    let hall: Vec<_> = t
        .quests
        .steps
        .iter()
        .filter(|s| s.quest == 0x4000_0088)
        .collect();
    assert_eq!(
        hall.len(),
        9,
        "the hall quest's steps, bestowal to completion"
    );
    let vault = hall
        .iter()
        .find(|s| s.trigger == Trigger::UseOn(0x4700_1AED, 0x4700_1CE8))
        .expect("the glyph used on the vault");
    assert_eq!(vault.takes, [0x4700_1AED]);
    assert_eq!(vault.to.map(|p| p.cell), Some(0x0298_014D));
    assert!(hall
        .iter()
        .any(|s| s.trigger == Trigger::Arrive(0x4700_1BFF)));
    let quests: std::collections::BTreeSet<u32> = t.quests.steps.iter().map(|s| s.quest).collect();
    assert!(quests.len() >= 18, "{} quests with steps", quests.len());
    let people = [
        0x4700_2F7E,
        0x4700_2FA9,
        0x4700_2FBC,
        0x4700_31E8,
        0x4700_31EA,
        0x4700_31EC,
        0x4700_31ED,
        0x4700_31EE,
        0x4700_31EF,
        0x4700_31F0,
        0x4700_31F1,
        0x4700_31F2,
        0x4700_31F3,
        0x4700_31F4,
    ];
    for template in people {
        assert!(
            t.quests
                .steps
                .iter()
                .any(|s| s.trigger == Trigger::Use(template)),
            "a step is made by using {template:08X}"
        );
        assert!(
            t.content.objects.iter().any(|o| o.template == template),
            "{template:08X} stands in the world"
        );
    }
}

/// A person of the database used with nothing more for the player says a line the database gives
/// it: the Shelter Guard, its quest already given, sends the player on.
#[test]
fn a_person_used_again_says_a_line_the_database_gives_it() {
    let t = &compiled().tables;
    assert!(t.talk.lines.len() > 5000, "{} lines", t.talk.lines.len());
    let (mut h, b, me) = served(t);
    let guard = person(t, 0x4700_2DFD);
    let first = use_person(&mut h, b, me, t, guard);
    assert!(
        first.iter().any(|(f, _)| *f == 0x8213_0024),
        "the quest is given"
    );
    let again = use_person(&mut h, b, me, t, guard);
    let said: Vec<u32> = again
        .iter()
        .filter(|(f, _)| *f == 0x8205_0017)
        .filter_map(|(_, s)| *s)
        .collect();
    let lines: Vec<u32> = t
        .talk
        .of(0x4700_2DFD, Some(guard))
        .filter(|l| l.when == yalain_content::talk::When::Use && l.update.is_none())
        .filter_map(|l| match l.words {
            yalain_content::Words::Ref(s, _) => Some(s),
            yalain_content::Words::Literal(_) => None,
        })
        .collect();
    assert_eq!(said.len(), 1, "one line: {said:08X?}");
    assert!(
        lines.contains(&said[0]),
        "{:08X} is one of {lines:08X?}",
        said[0]
    );
    assert!(
        !again.iter().any(|(f, _)| *f == 0x8213_0024),
        "no quest update"
    );
}

/// A world whose rows say nothing of how many times a quest may be done (the pinned release) still
/// gives a quest its setup says is done once only once: the training hall quest, done long ago,
/// is not given again by the Shelter Guard; a quest whose setup lets it be done again (Orlen
/// Jarlns') and the server's own errands (Stubby's) are not done once.
/// Behaviour: server.quests.done-once-without-rows
#[test]
fn a_quest_its_setup_says_is_done_once_is_not_given_again_whatever_the_rows_say() {
    let t = &compiled().tables;
    let once = |q: u32| t.quests.quest(q).map(|d| d.once);
    assert_eq!(once(0x4000_0088), Some(true), "the training hall quest");
    assert_eq!(once(0x4000_018F), Some(false), "Orlen Jarlns' quest");
    assert_eq!(once(0x4000_037A), Some(false), "Stubby's errand");
    let (mut h, b, me) = served(t);
    for p in h.server.players.values_mut() {
        p.play.quests = vec![yalain_world::state::QuestState {
            quest: 0x4000_0088,
            status: yalain_world::quests::status::COMPLETED,
            phase: 0,
            started: 0,
            updated: 0,
        }];
    }
    let i = person(t, 0x4700_2DFD);
    let calls = use_person(&mut h, b, me, t, i);
    assert!(
        !calls.iter().any(|(f, _)| *f == 0x8213_0024),
        "not given again: {calls:08X?}"
    );
}

/// The pinned release names a place for each surveyor's map whose trip 2017 recorded: 116 maps of
/// the search areas a cartomancer sent players to. A world without those rows sends no one (the
/// cpu test of a map with no place).
/// Behaviour: server.gives.maps-named-by-the-release
#[test]
fn the_pinned_release_names_a_place_for_the_maps_2017_sent_players_by() {
    let t = &pinned().tables;
    let maps = |x: &u32| {
        (0x4700_0C23..=0x4700_0C81).contains(x)
            || (0x4700_1A77..=0x4700_1AD5).contains(x)
            || *x == 0x4700_1425
    };
    let named: Vec<&u32> = t.places_named.keys().filter(|x| maps(x)).collect();
    assert_eq!(named.len(), 116, "maps named: {named:08X?}");
}

/// The compiled world on a harness, with a character in it.
fn served(t: &Tables) -> (Harness, usize, u64) {
    let mut h = Harness::new();
    h.server.content = t.content.clone();
    h.server.stores = t.stores.clone();
    h.server.quests = t.quests.clone();
    h.server.talk = t.talk.clone();
    let (b, me) = h.player_in_world("world", "Rowan");
    (h, b, me)
}

/// The place in the content pack of the hall copy's person of a template.
fn person(t: &Tables, template: u32) -> usize {
    t.content
        .objects
        .iter()
        .position(|o| o.template == template && o.landblock() == 0x0298)
        .unwrap_or_else(|| panic!("{template:08X} in the hall"))
}

/// Step up to a person (a client walks there before it uses someone) and use it: the calls the
/// player got, each with the string id of the first text it carries.
fn use_person(h: &mut Harness, b: usize, me: u64, t: &Tables, i: usize) -> Vec<(u32, Option<u32>)> {
    let o = &t.content.objects[i];
    for p in h.server.players.values_mut() {
        p.position.cell = o.cell;
        p.position.origin = [o.origin[0] + 1.0, o.origin[1], o.origin[2]];
    }
    let id = yalain_world::presence::content_id(&t.content.objects, i);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8216_0016, &[use_of(id, me)]),
    );
    h.run(10);
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) => {
                let v = yalain_world::calls::values(
                    &c.args.as_ref().map(|a| a.0.clone()).unwrap_or_default(),
                )
                .unwrap_or_default();
                let text = v.iter().find_map(|x| match x.root() {
                    Some(yalain_world::calls::Node::TextInfo(t)) => Some(t.string_id),
                    _ => None,
                });
                Some((c.function.0, text))
            }
            _ => None,
        })
        .collect()
}

/// A use of an object (`TryToUseItem`'s argument), as the play tests send it.
fn use_of(item: u64, player: u64) -> auberean_script::args::Arg {
    use yalain_world::graph::{Builder, Slot};
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(player),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(316),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    auberean_script::args::Arg::Object(b.graph())
}

/// The training hall's last portal, from the database: a player at the step's phase holding the
/// glyph uses it on the portal from across the room, is walked there, plays the use, and goes
/// through portal space to the vault's chamber, the glyph taken and the quest moved on.
#[test]
fn the_hall_s_glyph_used_on_its_last_portal_takes_the_player_to_the_chamber() {
    use yalain_content::quests::Trigger;
    let t = &compiled().tables;
    let step = t
        .quests
        .steps
        .iter()
        .find(|s| s.quest == 0x4000_0088 && s.trigger == Trigger::UseOn(0x4700_1AED, 0x4700_1CE8))
        .expect("the glyph's step")
        .clone();
    let (mut h, b, me) = served(t);
    let world = *h.server.players.keys().next().expect("in");
    let now = h.server.players[&world]
        .play
        .quests
        .first()
        .map_or(0, |q| q.started);
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .play
        .quests
        .push(yalain_world::state::QuestState {
            quest: 0x4000_0088,
            status: yalain_world::quests::status::UNDERWAY,
            phase: step.needs,
            started: now,
            updated: now,
        });
    let portal_dat = yalain_testkit::authored::Portal::open();
    h.server.catalog = portal_dat.catalog(t);
    h.server.catalog.insert(portal_dat.template(0x4700_1AED));
    h.server.catalog.insert(portal_dat.template(0x4700_1CE8));
    let targets: Vec<(u32, u32)> = h
        .server
        .catalog
        .templates
        .values()
        .map(|x| (x.id, x.description.package))
        .collect();
    let templates: Vec<u32> = h.server.catalog.templates.keys().copied().collect();
    h.server.interactions = portal_dat.interactions(&templates, &targets);
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, 0x4700_1AED, 1);
    h.run(3);
    let glyph = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == 0x4700_1AED)
        .expect("the glyph held")
        .id;
    let i = t
        .content
        .objects
        .iter()
        .position(|o| o.template == 0x4700_1CE8 && o.landblock() == 0x0298)
        .expect("the hall's last portal");
    let o = &t.content.objects[i];
    let portal = yalain_world::presence::content_id(&t.content.objects, i);
    let away = |d: f32| [o.origin[0] + d, o.origin[1], o.origin[2]];
    let cell = o.cell;
    for p in h.server.players.values_mut() {
        p.position.cell = cell;
        p.position.origin = away(12.0);
    }
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8216_0016, &[use_on(glyph, me, portal)]),
    );
    h.run(3);
    // The client walks there and comes to rest a little beyond the use's reach.
    for p in h.server.players.values_mut() {
        p.position.origin = away(5.6);
        p.heading_motion = yalain_world::world::IDLE_MOTION;
    }
    let clock = h.clock();
    yalain_world::places::arrived(&mut h.server, &mut h.net, world, clock);
    h.run(3);
    let until = h.now.0 + 8.0;
    while h.now.0 < until {
        h.step();
    }
    let msgs = h.world_messages(b, from);
    assert!(
        msgs.iter()
            .any(|m| matches!(m, ServerMessage::PlayBehavior(_))),
        "the glyph's use plays"
    );
    let p = &h.server.players[&world];
    assert_eq!(
        p.position.cell,
        step.to.expect("a place").cell,
        "in the chamber"
    );
    assert!(
        !p.play.items.iter().any(|i| i.template == 0x4700_1AED),
        "the glyph taken"
    );
    assert!(
        yalain_world::quests::phase_of(&p.play.quests, 0x4000_0088) > step.needs,
        "the quest moved on"
    );
}

/// A use of a held item on an object (`TryToUseItem`'s argument).
fn use_on(item: u64, player: u64, on: u64) -> auberean_script::args::Arg {
    use yalain_world::graph::{Builder, Slot};
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(player),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(on),
            Slot::Int(0),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(316),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    auberean_script::args::Arg::Object(b.graph())
}

/// The quests a sample of recorded kills moved: the quest, a creature whose death gives it (when one
/// does), and one whose death moves it on.
const KILL_SAMPLE: [(u32, Option<u32>, u32); 10] = [
    (0x4000_0159, Some(0x4700_2205), 0x4700_21F8),
    (0x4000_00D9, Some(0x4700_2BD0), 0x4700_2BB2),
    (0x4000_009F, Some(0x4700_28B2), 0x4700_28A2),
    (0x4000_0156, Some(0x4700_2CBF), 0x4700_2C82),
    (0x4000_0157, Some(0x4700_2203), 0x4700_21F3),
    (0x4000_0009, None, 0x4700_2225),
    (0x4000_000A, None, 0x4700_2963),
    (0x8000_00B7, None, 0x4700_282F),
    (0x8000_00BE, None, 0x4700_21B3),
    (0x8000_00BF, None, 0x4700_27BF),
];

/// What a quest update says, read from a call's arguments: why (0 for a move), the update's kind,
/// and the record's id, name, description, icon, challenge, status, phase and journal entry.
type Said = (
    u32,
    u32,
    u32,
    (u32, u32),
    (u32, u32),
    u32,
    i32,
    u32,
    u32,
    (u32, u32),
);

fn said_by(a: &[u8], lib: &auberean_script::library::Library) -> Option<Said> {
    use auberean_script::args::{self, Arg};
    use auberean_script::graph::Body;
    use auberean_script::native_object::NativeObject;
    let v = args::decode(a, lib).ok()?;
    let (Arg::Int(why), Arg::Object(g), Arg::Int(kind)) = (v.first()?, v.get(1)?, v.get(2)?) else {
        return None;
    };
    let q = g.entries.iter().find_map(|e| match &e.body {
        Body::Object(o) => match &o.native {
            Some(NativeObject::Quest(q)) => Some(q),
            _ => None,
        },
        _ => None,
    })?;
    let t = |x: &auberean_script::native_object::TextInfo| (x.string_id, x.table);
    Some((
        *why,
        *kind,
        q.id,
        t(&q.name),
        t(&q.description),
        q.icon,
        q.challenge,
        q.status,
        q.phase,
        t(&q.journal),
    ))
}

/// A beetle of the harness made a creature of this template, by the install's own properties, hurt
/// to death by this character.
fn killed_as(h: &mut Harness, me: u64, template: u32, install: &DatInstall<'_>) {
    use yalain_world_db::compile::Install;
    let props = yalain_content::Props::of(template, &|id| install.entity(id), &|id| {
        install.property_name(id)
    });
    let stats = yalain_content::CreatureStats::from_props(&props).expect("a creature's template");
    for p in h.server.players.values_mut() {
        p.position.cell = 0x0299_0109;
        p.position.origin = [59.06, 240.2, -44.6];
    }
    h.add_creatures(1);
    h.run(20);
    let id = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| {
            t.creature
                .as_ref()
                .is_some_and(|c| c.mode != yalain_world::things::Mode::Dead)
        })
        .map(|t| t.id)
        .max()
        .expect("a creature");
    let t = h.server.things.get_mut(id).expect("the creature");
    t.template = template;
    t.health = 10;
    t.max_health = 10;
    if let Some(c) = t.creature.as_mut() {
        c.stats.attacks_first = false;
        c.stats.death_effects = stats.death_effects;
    }
    let clock = h.clock();
    yalain_world::combat::hurt_creature(&mut h.server, &mut h.net, id, me, 50, clock);
    h.run(2);
}

/// The quest's id by its name, from the install.
fn quest_ids() -> std::collections::HashMap<String, u32> {
    let dir = require_install(TARGET);
    let found = install_dats(&dir).expect("the install");
    let portal = DatFile::open(
        &found
            .iter()
            .find(|(m, _)| *m == DatName::Portal)
            .expect("a portal")
            .1,
    )
    .expect("opens");
    let ctx = Context::from_portal(&portal).expect("context");
    let names: auberean_assets::strings::EnumMap = auberean_assets::decode(
        0x2300_0090,
        &portal.read(0x2300_0090).expect("the quests' names"),
        &ctx,
    )
    .expect("the quests' names read");
    names.names.into_iter().map(|(v, n)| (n, v)).collect()
}

/// **A kill sends what a recorded kill was sent**: every update the January 2017 servers sent of a
/// sample of kill quests, its record and its kind, played by killing a creature of the install's
/// that names the quest with the player's quest where the update found it: a giving, a move to each
/// phase, a completion, a refusal. A quest whose last phase waits is told nothing there.
/// Behaviour: server.quests.a-kill-sends-what-2017-sent
#[test]
fn a_kill_sends_the_updates_recorded_kills_were_sent() {
    use auberean_testkit::captures::{self, carrier};
    use yalain_world::quests::{refused, status, update};
    use yalain_world::state::QuestState;
    const UPDATE_QUEST: u32 = 0x8213_0024;
    let lib = {
        let dir = require_install(TARGET);
        let found = install_dats(&dir).expect("the install");
        let portal = DatFile::open(
            &found
                .iter()
                .find(|(m, _)| *m == DatName::Portal)
                .expect("a portal")
                .1,
        )
        .expect("opens");
        auberean_script::library::Library::from_record(
            &portal
                .read(auberean_script::library::RECORD)
                .expect("the library's record"),
        )
        .expect("the library reads")
    };
    let lib = &lib;
    let t = &compiled().tables;
    let index = auberean_testkit::data::require_capture_index();
    let mut recorded: Vec<Said> = Vec::new();
    captures::messages(&index, &[carrier::TO_PLAYER], &[2017], |m| {
        let Some(call) = m.script_call() else { return };
        if call.function != UPDATE_QUEST {
            return;
        }
        if let Some(s) = said_by(&call.args, lib) {
            if KILL_SAMPLE.iter().any(|(q, _, _)| *q == s.2) {
                recorded.push(s);
            }
        }
    })
    .expect("the capture index reads");
    assert!(
        recorded.len() > 150,
        "{} updates of the sample",
        recorded.len()
    );
    let install = dat_install();
    let (mut h, b, me) = served(t);
    h.server.quest_ids = quest_ids();
    let mut wrong = Vec::new();
    let mut played = 0usize;
    for s in &recorded {
        let (why, kind, quest) = (s.0, s.1, s.2);
        let (_, gives, moves) = KILL_SAMPLE
            .iter()
            .find(|(q, _, _)| *q == quest)
            .expect("in the sample");
        let last = t.quests.quest(quest).expect("a row").journals.len();
        let last = u32::try_from(last).expect("a few phases");
        // Where the update found the quest.
        let (held, template) = match (why, kind & 0xF000_0000) {
            (0, update::BESTOW) => match gives {
                Some(g) => (None, *g),
                None => continue,
            },
            (0, update::ADVANCE) => (Some((kind & 0xFFFF) - 1), *moves),
            // A last phase that waits is ended by someone used, not by a kill.
            (0, update::COMPLETE) if t.quests.quest(quest).is_some_and(|d| d.last_waits) => {
                continue
            }
            (0, update::COMPLETE) => (Some(last), *moves),
            (refused::UNDER_WAY, update::BESTOW) => match gives {
                Some(g) => (Some(1), *g),
                None => continue,
            },
            _ => continue,
        };
        for p in h.server.players.values_mut() {
            p.play.quests.retain(|q| q.quest != quest);
            if let Some(phase) = held {
                p.play.quests.push(QuestState {
                    quest,
                    status: status::UNDERWAY,
                    phase,
                    started: 0,
                    updated: 0,
                });
            }
        }
        let from = h.bots[b].inbox.len();
        killed_as(&mut h, me, template, &install);
        let sent: Vec<Said> = h
            .world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::PlayerCall(c) if c.function.0 == UPDATE_QUEST => {
                    said_by(&c.args.as_ref()?.0, lib)
                }
                _ => None,
            })
            .filter(|x| x.2 == quest)
            .collect();
        played += 1;
        if sent != [*s] {
            wrong.push((*s, sent));
        }
    }
    println!(
        "{} recorded, {played} played, {} differ",
        recorded.len(),
        wrong.len()
    );
    assert!(played > 100, "{played} updates played");
    assert!(wrong.is_empty(), "{:08X?}", &wrong[..wrong.len().min(6)]);
    // A last phase that waits: the first kill quest of the people's is told nothing more.
    let waits = 0x8000_00B7;
    assert!(t.quests.quest(waits).is_some_and(|d| d.last_waits));
    let last = t.quests.quest(waits).expect("a row").journals.len();
    let last = u32::try_from(last).expect("a few phases");
    for p in h.server.players.values_mut() {
        p.play.quests.retain(|q| q.quest != waits);
        p.play.quests.push(QuestState {
            quest: waits,
            status: status::UNDERWAY,
            phase: last,
            started: 0,
            updated: 0,
        });
    }
    let from = h.bots[b].inbox.len();
    killed_as(&mut h, me, 0x4700_282F, &install);
    assert!(!h.functions(b, from).contains(&UPDATE_QUEST));
}

/// **A quest with no setup that no recording shows reaching fellows moves for its killer alone**: a
/// quest kept from the install's definition with no word on fellows, a kill of a creature naming it,
/// the killer's fellow beside it holding the quest at the same phase.
/// Behaviour: server.quests.an-unrecorded-quest-reaches-no-fellow
#[test]
fn a_quest_with_no_setup_and_no_recording_moves_its_killers_quest_and_no_fellows() {
    use auberean_script::args::Arg;
    use auberean_script::native_object::{native_type, NativeObject};
    use yalain_world::fellowship::function;
    use yalain_world::quests::{phase_of, status};
    use yalain_world::state::QuestState;
    const QUEST: u32 = 0x8000_01C4;
    const NAMES_IT: u32 = 0x4700_2300;
    let t = &compiled().tables;
    assert!(
        !t.quests.quest(QUEST).expect("a row").fellows.shared,
        "no word on fellows"
    );
    let install = dat_install();
    let (mut h, a, me) = served(t);
    h.server.quest_ids = quest_ids();
    let (b, them) = h.player_in_world("two", "Haskel");
    let mut name = yalain_world::graph::Builder::new();
    #[allow(clippy::cast_possible_truncation)]
    name.native(
        native_type::WP_STRING as u16,
        NativeObject::Text("Band".encode_utf16().collect()),
    );
    h.ask(
        a,
        function::CREATE,
        &[Arg::Int(1), Arg::Int(0), Arg::Object(name.graph())],
        10,
    );
    for p in h.server.players.values_mut() {
        p.position.cell = 0x0299_0109;
        p.position.origin = [59.06, 240.2, -44.6];
        p.play.quests = vec![QuestState {
            quest: QUEST,
            status: status::UNDERWAY,
            phase: 1,
            started: 0,
            updated: 0,
        }];
    }
    h.ask(a, function::RECRUIT, &[Arg::LongInt(them)], 10);
    h.ask(b, function::ACCEPT, &[Arg::LongInt(me)], 10);
    let f = h.server.fellowships.of(me).expect("a fellowship");
    assert_eq!(h.server.fellowships.of(them), Some(f), "fellows");
    killed_as(&mut h, me, NAMES_IT, &install);
    let phase = |h: &Harness, who: u64| {
        let p = h.server.players.values().find(|p| p.character == who);
        phase_of(&p.expect("in the world").play.quests, QUEST)
    };
    assert_eq!((phase(&h, me), phase(&h, them)), (2, 1));
}
