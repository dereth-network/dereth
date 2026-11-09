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
use yalain_world_db::release::{pinned_in_cache, PINNED_TAG};
use yalain_world_db::{check, merge, rows, Files};

/// The variable naming a world database checkout for the tests.
const WORLD_DB_VAR: &str = "YALAIN_TEST_WORLD_DB";

fn database() -> PathBuf {
    if let Some(v) = std::env::var_os(WORLD_DB_VAR).filter(|v| !v.is_empty()) {
        return PathBuf::from(v);
    }
    pinned_in_cache().unwrap_or_else(|why| {
        panic!(
            "no world database: {why}. Set {WORLD_DB_VAR} to a checkout, or fetch the pinned release ({PINNED_TAG}) with `yalain world fetch`"
        )
    })
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

/// The database, checked, merged and compiled against the install, once.
fn compiled() -> &'static Compiled {
    static C: OnceLock<Compiled> = OnceLock::new();
    C.get_or_init(|| compile_at(&database()))
}

/// The item rows the database's world plays by: its rows', the server's own for each kind it
/// gives none of.
pub(crate) fn item_rows() -> yalain_content::item_rows::ItemRows {
    yalain_world::item_use::world_item_rows(compiled().tables.items.clone()).0
}

/// The treasure numbers the database's world plays by: its rows', the server's own for each kind
/// it gives none of.
pub(crate) fn treasure_tables() -> yalain_world::treasure::TreasureTables {
    yalain_world::treasure::TreasureTables::with_rows(&compiled().tables.treasure).0
}

/// The pinned release compiled, once (the database itself when it is the pinned release).
fn pinned() -> &'static Compiled {
    static P: OnceLock<Compiled> = OnceLock::new();
    let dir = pinned_in_cache().unwrap_or_else(|why| {
        panic!("no pinned release: {why}. Fetch it ({PINNED_TAG}) with `yalain world fetch`")
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
        portal
            .read_checked(RECORD, Library::from_record)
            .expect("the library reads")
            .value
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
    assert_eq!(t.quests.gives.len(), count("give"));
    assert_eq!(t.talk.lines.len(), count("talk"));
    // Every object is drawn as something.
    for o in &t.content.objects {
        assert_ne!(o.description.look, 0, "{:08X}", o.template);
    }
}

/// A world's ward rows hold the six wards of fire, what lowers vigor, poison and what lowers armor,
/// each with the classes and chance its description gives, entry for entry; its other rows name
/// what their effects' own data names nothing for.
/// Behaviour: server.effects.ward-rows-hold-the-six
#[test]
fn the_worlds_ward_rows_hold_the_six_wards_entry_for_entry() {
    let wards = &compiled().tables.effect_rules.wards;
    for (state, six) in yalain_testkit::test_wards() {
        assert_eq!(wards.get(&state), Some(&six), "{state:08X}");
    }
    assert!(wards
        .values()
        .all(|w| !(w.classes.is_empty() && w.effects.is_empty())));
}

/// A world's mine and map region rows hold the server's own, entry for entry: each mining skill's
/// behavior, tools, answers, delving node, lodestones and finds, each region's nodes by their
/// effects, and what each mining tool yields unmade; so a world with them mines as one without.
/// Behaviour: server.craft.gathering-rows-hold-the-servers-own
#[test]
fn the_worlds_mine_and_map_region_rows_hold_the_servers_own_entry_for_entry() {
    let t = &compiled().tables;
    let mut craft = yalain_world::craft::Crafting {
        tool_yields: t.crafting.tool_yields.clone().into_iter().collect(),
        ..yalain_world::craft::Crafting::default()
    };
    yalain_world::mining::take_world_gathering(&mut craft, &t.gathering);
    let own = yalain_testkit::test_gathering();
    for (skill, mine) in &own.mines {
        assert_eq!(craft.gathering.mines.get(skill), Some(mine), "{skill:08X}");
    }
    for (region, nodes) in &own.map_regions {
        assert_eq!(
            craft.gathering.map_regions.get(region),
            Some(nodes),
            "{region}"
        );
    }
    for (tool, unmade) in yalain_testkit::test_mining_tool_yields() {
        assert_eq!(craft.tool_yields.get(&tool), Some(&unmade), "{tool:08X}");
    }
}

/// A world's item use, book and first gift rows hold the server's own, entry for entry: the
/// forager's food's and the training hall's potions' effects, behavior and wear, what the cards and
/// the Shi show, and the guide every new character is given; so a world with them plays as one
/// without.
/// Behaviour: server.items.item-rows-hold-the-servers-own
#[test]
fn the_worlds_item_use_book_and_start_item_rows_hold_the_servers_own_entry_for_entry() {
    let world = item_rows();
    let own = yalain_testkit::test_item_rows();
    for (t, u) in &own.uses {
        assert_eq!(world.uses.get(t), Some(u), "{t:08X}");
    }
    for (t, b) in &own.books {
        assert_eq!(world.books.get(t), Some(b), "{t:08X}");
    }
    assert_eq!(world.start_items, own.start_items);
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

/// Nothing a store sells goes into the money bag for what it cost: every sale of the world is in
/// coin, and each piece costs more than the install says it is worth.
/// Behaviour: server.store.nothing-sold-pays-back-its-price
#[test]
fn nothing_a_store_sells_is_worth_what_it_costs() {
    let portal = yalain_testkit::authored::Portal::open();
    let mut worth = std::collections::HashMap::new();
    let sales = &compiled().tables.stores.sales;
    assert!(sales.len() > 100, "{}", sales.len());
    for sale in sales {
        assert_eq!(sale.currency, 0x4700_0538, "{:#x}", sale.product);
        let value = *worth.entry(sale.product).or_insert_with(|| {
            let t = portal.template(sale.product);
            t.props.int("Item_Value").unwrap_or(0)
        });
        #[allow(clippy::cast_precision_loss)]
        let value = value as f32;
        assert!(
            value < sale.cost,
            "{:#x} costs {} and is worth {value}",
            sale.product,
            sale.cost
        );
    }
}

/// The chests recorded with treasure draw it at their kind's level and band; none pays more than
/// its kind was seen to, and those of an unknown level stay as their templates have them.
/// Behaviour: server.places.a-chests-draw-is-treasure-of-its-level
#[test]
fn the_recorded_chests_draw_treasure_within_what_2017_showed() {
    let t = &compiled().tables;
    let got: std::collections::BTreeMap<&str, (i32, (u32, u32))> = t
        .loot
        .profiles
        .iter()
        .filter_map(|p| p.chest.map(|c| (p.name.as_str(), (c.level, c.value))))
        .collect();
    // Each kind's greatest piece in 2017.
    for (name, most) in [
        ("GP_Chest_SigilHuman_Standard_LowHoard", 20),
        ("GP_Chest_SigilHuman_Weapons_LowHoard", 11),
        ("GP_Chest_SigilHuman_UberHoard", 35),
        ("GP_Chest_SigilLugian_ArmorHoard", 86),
        ("GP_Chest_SigilLugian_WeaponsHoard", 81),
        ("GP_Chest_SigilLugian_UberHoard", 213),
        ("GP_Chest_SigilTumerok_UberHoard", 95),
        ("GP_SHRETHBaseUltraTreasureHoard", 231),
    ] {
        let (_, (low, high)) = got[name];
        assert!(low > 0 && high <= most, "{name}: {low} to {high}");
    }
    assert_eq!(got.len(), 8);
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
    // What its quests give can be made, as on a real start.
    let gifts = t.quests.steps.iter().flat_map(|s| s.gives.iter());
    for gift in gifts.chain(t.quests.gives.iter().flat_map(|g| g.gives.iter())) {
        if h.server.catalog.get(*gift).is_none() {
            let props = yalain_content::Props::default();
            let made = yalain_testkit::test_template(*gift, 0x1067, props);
            h.server.catalog.insert(made);
        }
    }
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
        portal
            .read_checked(
                auberean_script::library::RECORD,
                auberean_script::library::Library::from_record,
            )
            .expect("the library reads")
            .value
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
    // Of a level no quest of the sample holds back.
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, 302, 60);
    }
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

/// A step of a play of a quest a person gives: what the player does, and the phase its quest is
/// at after (0: done).
#[derive(Clone, Copy)]
enum Do {
    Use(u32),
    Kill(u32),
}

/// Quests the January 2017 servers gave at a person's use, moved by kills (and uses between), and
/// took back at the same person's use, played in the order a recording played them: the quest, then
/// each thing done with the phase it leaves, 0 for done. A kill at a last phase that waits moves
/// nothing.
fn given_sample() -> Vec<(u32, Vec<(Do, u32)>)> {
    use Do::{Kill, Use};
    let hunt = |who: u32, creature: u32, kills: u32| {
        let mut v = vec![(Use(who), 1)];
        v.extend((1..=kills).map(|n| (Kill(creature), n + 1)));
        v.push((Kill(creature), kills + 1));
        v.push((Use(who), 0));
        v
    };
    vec![
        // Vorgas, Thanos.
        (0x4000_01F2, hunt(0x4700_2FF9, 0x4700_2AD9, 1)),
        // Sanduto, ten Glissna Nefane.
        (0x8000_0385, hunt(0x4700_3275, 0x4700_2889, 10)),
        // Lupe, ten rabbits, a last phase of no range.
        (0x4000_028D, hunt(0x4700_3353, 0x4700_0828, 10)),
        // Stubby's five rabbits, each at its own phase.
        (
            0x4000_0335,
            vec![
                (Use(0x4700_335B), 1),
                (Kill(0x4700_082D), 2),
                (Kill(0x4700_29B9), 3),
                (Kill(0x4700_29C1), 4),
                (Kill(0x4700_29BA), 5),
                (Kill(0x4700_29A2), 6),
                (Use(0x4700_335B), 0),
            ],
        ),
        // Shi Kahuawa: his routine (a subquest's ten kills), two uses, him, a last use.
        (0x8000_0374, {
            let mut v = vec![(Use(0x4700_3279), 1)];
            v.extend((1..10).map(|_| (Kill(0x4700_21B9), 1)));
            v.extend([
                (Kill(0x4700_21B9), 2),
                (Use(0x4700_3279), 3),
                (Use(0x4700_3279), 4),
                (Kill(0x4700_2C2E), 5),
                (Use(0x4700_3279), 0),
            ]);
            v
        }),
        // Fraidy: two subquests of ten kills each.
        (0x8000_0353, {
            let mut v = vec![(Use(0x4700_2FAE), 1)];
            v.extend((0..10).map(|_| (Kill(0x4700_2441), 1)));
            v.extend((1..10).map(|_| (Kill(0x4700_2439), 1)));
            v.extend([(Kill(0x4700_2439), 2), (Use(0x4700_2FAE), 0)]);
            v
        }),
    ]
}

/// **A person gives a kill quest and takes it back as 2017's did**: a sample of quests the
/// recordings hold from their giving to their end, played through the server over the database's
/// rows and the creatures' own death effects: each update is one the January 2017 servers sent of
/// it (reason, kind, id, name, description, icon, challenge, status, phase, journal). A world older
/// than these rows has nothing to play.
/// Behaviour: server.quests.a-person-gives-and-takes-back-kill-quests
#[test]
fn a_person_gives_a_kill_quest_and_takes_it_back_as_the_recordings_show() {
    use auberean_testkit::captures::{self, carrier};
    use yalain_world::quests::status;
    const UPDATE_QUEST: u32 = 0x8213_0024;
    let t = &compiled().tables;
    let sample = given_sample();
    assert!(
        t.quests.steps.iter().any(|s| s.quest == sample[0].0),
        "the world has rows for the sample"
    );
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
    let lib = portal
        .read_checked(
            auberean_script::library::RECORD,
            auberean_script::library::Library::from_record,
        )
        .expect("the library reads")
        .value;
    let index = auberean_testkit::data::require_capture_index();
    let mut recorded: Vec<Said> = Vec::new();
    captures::messages(&index, &[carrier::TO_PLAYER], &[2017], |m| {
        let Some(call) = m.script_call() else { return };
        if call.function == UPDATE_QUEST {
            if let Some(s) = said_by(&call.args, &lib) {
                if sample.iter().any(|(q, _)| *q == s.2) {
                    recorded.push(s);
                }
            }
        }
    })
    .expect("the capture index reads");
    let install = dat_install();
    let mut wrong = Vec::new();
    let mut compared = 0usize;
    for (quest, play) in &sample {
        let (mut h, b, me) = served(t);
        h.server.quest_ids = quest_ids();
        // Of a level no quest of the sample holds back.
        for p in h.server.players.values_mut() {
            yalain_world::quals::set_int(&mut p.qualities, 302, 60);
        }
        for (i, (what, after)) in play.iter().enumerate() {
            let from = h.bots[b].inbox.len();
            match *what {
                Do::Use(who) => {
                    let o = t
                        .content
                        .objects
                        .iter()
                        .position(|o| o.template == who)
                        .unwrap_or_else(|| panic!("{who:08X} placed"));
                    // A person with several quests gives one of them at random: ask until it is
                    // this one, starting afresh.
                    let mut tries = 0;
                    use_person(&mut h, b, me, t, o);
                    while i == 0 && tries < 60 && !holds(&h, *quest) {
                        for p in h.server.players.values_mut() {
                            p.play.quests.clear();
                        }
                        use_person(&mut h, b, me, t, o);
                        tries += 1;
                    }
                }
                Do::Kill(creature) => killed_as(&mut h, me, creature, &install),
            }
            let sent: Vec<Said> = h
                .world_messages(b, from)
                .iter()
                .filter_map(|m| match m {
                    ServerMessage::PlayerCall(c) if c.function.0 == UPDATE_QUEST => {
                        said_by(&c.args.as_ref()?.0, &lib)
                    }
                    _ => None,
                })
                .filter(|x| x.2 == *quest)
                .collect();
            for s in sent {
                compared += 1;
                if !recorded.contains(&s) {
                    wrong.push((i, s));
                }
            }
            let held = h.server.players.values().next().and_then(|p| {
                p.play
                    .quests
                    .iter()
                    .find(|x| x.quest == *quest)
                    .map(|x| (x.status, x.phase))
            });
            if *after == 0 {
                assert_eq!(
                    held.map(|x| x.0),
                    Some(status::COMPLETED),
                    "{quest:08X} done at {i}"
                );
            } else {
                assert_eq!(held, Some((status::UNDERWAY, *after)), "{quest:08X} at {i}");
            }
        }
    }
    println!("{compared} updates compared, {} differ", wrong.len());
    assert!(compared > 50, "{compared} updates compared");
    assert!(wrong.is_empty(), "{:08X?}", &wrong[..wrong.len().min(6)]);
}

/// Does the player hold this quest under way?
fn holds(h: &Harness, quest: u32) -> bool {
    h.server.players.values().next().is_some_and(|p| {
        p.play
            .quests
            .iter()
            .any(|x| x.quest == quest && x.phase != 0)
    })
}

/// **A board's postings are given one at a time**: the Postings board gives one of the quests its
/// description lists, says its line, and gives no other while that one is under way. A world older
/// than these rows has nothing to play.
/// Behaviour: server.quests.a-board-gives-what-it-lists
#[test]
fn a_postings_board_gives_one_of_the_quests_it_lists_at_a_time() {
    let t = &compiled().tables;
    const BOARD: u32 = 0x4700_3182;
    let listed: Vec<u32> = t
        .quests
        .steps
        .iter()
        .filter(|s| s.needs == 0 && s.trigger == yalain_content::quests::Trigger::Use(BOARD))
        .map(|s| s.quest)
        .collect();
    assert!(listed.len() > 5, "{} postings", listed.len());
    let (mut h, b, me) = served(t);
    set_level(&mut h, 72);
    let o = t
        .content
        .objects
        .iter()
        .position(|o| o.template == BOARD)
        .expect("the board is placed");
    let said = use_person(&mut h, b, me, t, o);
    let held = |h: &Harness| -> Vec<u32> {
        h.server
            .players
            .values()
            .next()
            .map(|p| {
                // Its subquests come with it, hidden.
                p.play
                    .quests
                    .iter()
                    .map(|x| x.quest)
                    .filter(|q| listed.contains(q))
                    .collect()
            })
            .unwrap_or_default()
    };
    let first = held(&h);
    assert_eq!(first.len(), 1, "one posting taken");
    assert!(
        said.iter().any(|(_, s)| *s == Some(0x9177_9F7E)),
        "it says its line: {said:08X?}"
    );
    use_person(&mut h, b, me, t, o);
    assert_eq!(held(&h), first, "none other while that one is under way");
}

/// Every vault's shard names the quest of its vault: hidden, with its setup's pay and points.
/// Behaviour: server.story.shard-names-its-vaults-quest
#[test]
fn a_vaults_shard_completes_the_quest_of_its_vault() {
    let t = &compiled().tables;
    assert_eq!(
        t.story.shard_quests.len(),
        22,
        "the 21 vaults and Soulbound"
    );
    let def = |q: &u32| t.quests.quest(*q).expect("the quest has a row");
    for (object, q) in &t.story.shard_quests {
        assert!(t.story.shards.contains_key(object));
        assert!(def(q).hidden, "{q:08X} is told to nobody");
        assert!(def(q).pay.points > 0, "{q:08X} adds to the rating");
    }
    // What 2017 paid as the player came out of each.
    for (object, xp, points, challenge) in [
        (0x4000_BD00_0000_0001u64, 800, 400, 8),
        (0x4001_F900_0000_0001, 4500, 700, 13),
        (0x4001_C400_0000_0001, 30_000_000, 2500, 54),
        (0x4006_F600_0000_0001, 10_000_000, 80_000, 40),
    ] {
        let d = def(&t.story.shard_quests[&object]);
        assert_eq!((d.xp, d.pay.points, d.challenge), (xp, points, challenge));
    }
}

/// What creatures' death effects make are creatures, and those effects are what the install's
/// creatures name.
/// Behaviour: server.creatures.death-spawns-rows
#[test]
fn the_death_spawns_name_creatures() {
    let t = &compiled().tables;
    assert_eq!(t.death_spawns.len(), 9, "{:?}", t.death_spawns);
    let portal = yalain_testkit::authored::Portal::open();
    let stats = |id: u32| {
        let template = portal.template(id);
        yalain_content::CreatureStats::from_props(&template.props)
    };
    for (name, want) in [
        ("Eff_FortTears_SpawnDrudgeHelper1", 0.7),
        ("Eff_SpawnShrethHelpers", 1.0),
    ] {
        let spawn = t.death_spawns[name];
        assert!((spawn.chance - want).abs() < f32::EPSILON, "{name}");
        assert!(stats(spawn.template).is_some(), "{name} makes a creature");
    }
    let sentry = stats(0x4700_239B).expect("the sentry");
    assert!(sentry
        .death_effects
        .iter()
        .any(|e| t.death_spawns.contains_key(e)));
}

/// A Gauntlet: its band's use of the Master gives it, the Portal sends to the first arena, each kill
/// the next, a use of the Master before the end fails it, and what it pays rises with the kills.
/// Behaviour: server.quests.gauntlets
#[test]
fn the_gauntlets_lead_from_arena_to_arena_and_a_use_before_the_end_fails() {
    use yalain_content::quests::{Trigger, Update};
    const MASTER: u32 = 0x4700_2FD2;
    const PORTAL: u32 = 0x4700_2FD4;
    let t = &compiled().tables;
    let ids = quest_ids();
    let q = ids["QuestGauntlet45plus"];
    let steps: Vec<_> = t.quests.steps.iter().filter(|s| s.quest == q).collect();
    let give = steps
        .iter()
        .find(|s| s.update == Update::Bestow)
        .expect("a giving");
    assert_eq!(give.trigger, Trigger::Use(MASTER));
    assert_eq!(give.levels, Some((45, i32::MAX)));
    let enter = steps
        .iter()
        .find(|s| s.trigger == Trigger::Use(PORTAL))
        .expect("the portal");
    assert_eq!(enter.update, Update::Nothing);
    assert_eq!(enter.needs, 1);
    assert!(enter.to.is_some(), "leads to the first arena");
    let kills: Vec<_> = steps
        .iter()
        .filter(|s| matches!(s.trigger, Trigger::Kill(_)))
        .collect();
    assert_eq!(kills.len(), 5);
    assert_eq!(
        kills.iter().filter(|s| s.to.is_some()).count(),
        4,
        "each but the last leads on"
    );
    // What 2017's failures paid, from the phase they failed at.
    let fail_at = |phase: u32| {
        steps
            .iter()
            .find(|s| s.update == Update::Fail && s.needs == phase)
            .expect("a failing step")
            .xp_share
    };
    assert!(fail_at(1).abs() < f32::EPSILON);
    assert!((fail_at(4) - 0.01).abs() < 1e-6);
    assert!((fail_at(5) - 0.0125).abs() < 1e-6);
    let band = |n: &str, level: i32| {
        t.quests.steps.iter().any(|s| {
            s.quest == ids[n]
                && s.update == Update::Bestow
                && s.levels.is_some_and(|(a, b)| a <= level && level <= b)
        })
    };
    assert!(band("QuestGauntlet20to24", 23));
    assert!(band("QuestGauntlet30to34", 33));
    assert!(!band("QuestGauntlet30to34", 35));
    assert!(t.quests.quest(q).expect("a row").fails_on_death);
    // Each arena is one of a row of rooms, and each room holds a camp of what's killed there.
    assert_eq!(t.quests.rooms.len(), 16);
    let rooms: usize = t.quests.rooms.iter().map(Vec::len).sum();
    assert_eq!(rooms, 50);
    let camps = |cell: u32| t.spawns.camps.iter().filter(|c| c.cell == cell).count();
    for room in t.quests.rooms.iter().flatten() {
        assert_eq!(camps(room.holds), 1, "{:#x}", room.holds);
    }
    let arenas = ids["QuestGauntlet20to24"]..=ids["QuestGauntlet45plus"];
    for step in t
        .quests
        .steps
        .iter()
        .filter(|s| s.to.is_some() && arenas.contains(&s.quest))
    {
        let to = step.to.expect("led to");
        assert!(
            t.quests.rooms.iter().flatten().any(|r| r.place == to),
            "{:#x} is one of a row",
            to.cell
        );
    }
}

/// A practice gem goes by the nearest ten to the player's level, one in a week whatever the player
/// does with it, by a hidden quest done as it is handed over; its use gives the gem's own quest.
/// Behaviour: server.quests.practice-gems
#[test]
fn the_practice_gems_go_by_level() {
    use yalain_content::quests::{Trigger, Update};
    const PERSON: u32 = 0x4700_326D;
    let t = &compiled().tables;
    let control = quest_ids()["Quest_DeadlyDummyControl"];
    let gem_at = |level: i32| -> Vec<u32> {
        t.quests
            .steps
            .iter()
            .filter(|s| {
                s.quest == control
                    && s.trigger == Trigger::Use(PERSON)
                    && s.update == Update::Complete
                    && s.needs == 0
                    && s.levels.is_some_and(|(a, b)| a <= level && level <= b)
            })
            .flat_map(|s| s.gives.clone())
            .collect()
    };
    assert_eq!(gem_at(26), vec![0x4700_1853]);
    assert_eq!(gem_at(37), vec![0x4700_1854]);
    assert_eq!(gem_at(41), vec![0x4700_1854]);
    assert_eq!(gem_at(50), vec![0x4700_1855]);
    assert_eq!(gem_at(70), vec![0x4700_1857]);
    assert!(gem_at(24).is_empty());
    let def = t.quests.quest(control).expect("a row");
    assert!(def.hidden);
    assert!((def.retry - 604_800.0).abs() < 1.0);
    let portal = yalain_testkit::authored::Portal::open();
    let gem = portal.template(0x4700_1853);
    assert_eq!(
        gem.props.text("Usage_Effect1"),
        Some("Eff_BestowQuest_DeadlyDummy30")
    );
    // Each gem's use makes the dummy whose death moves that gem's own quest.
    let portal = yalain_testkit::authored::Portal::open();
    let spawns = &t.death_spawns;
    for (gem, tier) in (0x4700_1853..=0x4700_1857).zip([30, 40, 50, 60, 70]) {
        let summon = format!("Eff_SummonDeadlyDummy{tier}");
        assert_eq!(
            portal.template(gem).props.text("Usage_Effect2"),
            Some(summon.as_str())
        );
        let row = spawns.get(&summon).expect("a row");
        assert!((row.offset[1] - 2.0).abs() < 1e-6, "2 m ahead");
        assert_eq!(
            portal.template(row.template).props.text("NPC_DeathEffect1"),
            Some(format!("Eff_AdvanceQuest_DeadlyDummy{tier}Phase2").as_str())
        );
    }
}

/// Cragstone's yard holds no camp of creatures seen there only on the servers' last day, or of the
/// clones a player's item makes.
/// Behaviour: server.camps.last-day-crowds-left-out
#[test]
fn cragstone_keeps_no_last_day_crowd() {
    let t = &compiled().tables;
    let in_yard = |template: u32| {
        t.spawns
            .camps
            .iter()
            .filter(|c| c.cell >> 16 == 0x6FA9)
            .any(|c| c.templates.iter().any(|(id, _)| *id == template))
    };
    let anywhere = |template: u32| {
        t.spawns
            .camps
            .iter()
            .any(|c| c.templates.iter().any(|(id, _)| *id == template))
    };
    for (template, what) in [
        (0x4700_3437, "a player's Grievver"),
        (0x4700_0894, "a player's clones"),
        (0x4700_3459, "a player's lights"),
        (0x4700_21EE, "a player's balloons"),
    ] {
        assert!(!anywhere(template), "{what}");
    }
    assert!(!in_yard(0x4700_0826), "a perk's creatures");
    assert!(anywhere(0x4700_0826), "the same creature in the wild stays");
    let rabbits =
        t.spawns.camps.iter().any(|c| {
            c.cell >> 16 == 0x09F2 && c.templates.iter().any(|(id, _)| *id == 0x4700_29A5)
        });
    assert!(rabbits, "a camp of wild creatures seen on that day stays");
}

/// The install's level table.
fn level_table() -> yalain_world::levels::LevelTable {
    let dir = require_install(TARGET);
    let found = install_dats(&dir).expect("the install");
    let path = &found
        .iter()
        .find(|(m, _)| *m == DatName::Portal)
        .expect("a data file")
        .1;
    let portal = DatFile::open(path).expect("opens");
    let ctx = Context::from_portal(&portal).expect("context");
    let lib = portal
        .read_checked(RECORD, Library::from_record)
        .expect("the library reads")
        .value;
    let id = yalain_world::levels::LEVEL_TABLE;
    let st: auberean_assets::script::ScriptState =
        auberean_assets::decode(id, &portal.read(id).expect("reads"), &ctx).expect("decodes");
    let g = auberean_script::graph::Graph::decode(&st.graph, &lib).expect("graph");
    yalain_world::levels::LevelTable::from_state(&lib, &g).expect("the levels")
}

/// A quest with no setup pays what 2017 paid for it, whole whatever the player's level, as an amount
/// or a share up to its limit; one not recorded pays what a guide of the time gives.
/// Behaviour: server.quests.pay-with-no-setup
#[test]
fn the_quests_with_no_setup_pay_what_2017_paid_whole() {
    let t = &compiled().tables;
    let ids = quest_ids();
    let levels = level_table();
    let paid = |name: &str, level: i32| {
        let def = t
            .quests
            .quest(ids[name])
            .unwrap_or_else(|| panic!("{name} has a row"));
        yalain_world::quests::completion_pay(&levels, def, level)
    };
    for (name, level, want) in [
        ("QuestBarnesLumari", 30, 43_000),
        ("QuestBarnesLumari", 50, 43_000),
        ("Quest_DankRelics_2", 42, 200_000),
        ("QuestHeroicSlayer6", 49, 5_000_000),
        ("QuestRytheransBane", 43, 25_000_000),
        ("QuestImperatorWildHare", 42, 2_707_670),
        ("QuestImperatorWildHare", 50, 6_352_899),
        ("QuestStubby3", 42, 541_534),
        ("QuestStubby5", 50, 5_007_346),
        ("QuestStubby5", 48, 2_914_975),
        ("QuestStubby1", 50, 1_251_836),
        ("Quest_KQ3E_ShadeMatron", 60, 142_874_598),
        ("Quest_KQ3E_ShadeMatron", 70, 142_874_598),
    ] {
        assert_eq!(paid(name, level), want, "{name} at {level}");
    }
}

/// A quest with no setup adds what 2017's ratings moved by as it was completed.
/// Behaviour: server.quests.points-with-no-setup
#[test]
fn the_quests_measured_with_no_setup_add_what_2017_added() {
    let t = &compiled().tables;
    let ids = quest_ids();
    let points = |name: &str| {
        t.quests
            .quest(ids[name])
            .unwrap_or_else(|| panic!("{name} has a row"))
            .pay
            .points
    };
    for (name, want) in [
        ("QuestBarnesLostWish", 50),
        ("QuestBarnesLumari", 750),
        ("QuestBarnesOriad", 2500),
        ("QuestIcefire", 50_000),
        ("QuestRytheransBane", 60_000),
        ("QuestKanten1", 50),
        ("QuestLupe1", 1200),
        ("QuestArmorLords", 30_000),
    ] {
        assert_eq!(points(name), want, "{name}");
    }
    assert_eq!(points("QuestRytheransReward"), 0, "measured at none");
    assert_eq!(points("QuestBeetleHunt"), 15, "a setup's still its own");
}

/// A player's rating moved by the points of what it completed between two of its profiles, a
/// vault's shard's quest among them, over every 2017 run of the client.
/// Behaviour: server.quests.ratings-as-2017
#[test]
fn a_players_rating_moved_in_2017_by_the_points_of_what_it_completed() {
    use auberean_testkit::captures::{self, carrier};
    use yalain_world::requests::function;
    const UPDATE_QUEST: u32 = 0x8213_0024;
    const PROFILE: u32 = 0x820A_0009;
    const PLAYER_OBJECT: u32 = 0x10060;
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
    let t = &compiled().tables;
    let index = auberean_testkit::data::require_capture_index();
    let rating_var = yalain_world::profile::var::QUEST_RATING.to_le_bytes();
    // A run's character, its rating last seen, what it has completed since, the shard last used.
    let mut capture = -1;
    let mut own = 0u64;
    let mut last: Option<i64> = None;
    let mut since: Vec<u32> = Vec::new();
    let mut shard: Option<(u64, f64)> = None;
    let (mut agree, mut more, mut less, mut profiles) = (0, Vec::new(), Vec::new(), 0);
    captures::messages_in_time_order(
        &index,
        &[carrier::TO_PLAYER, carrier::TO_SERVER, PLAYER_OBJECT],
        &[2017],
        |m| {
            if m.capture != capture {
                capture = m.capture;
                own = 0;
                last = None;
                since.clear();
                shard = None;
            }
            if m.message_type == PLAYER_OBJECT {
                if let Ok(ServerMessage::PlayerObject(p)) = ServerMessage::decode(&m.raw) {
                    if p.id != own {
                        last = None;
                        since.clear();
                    }
                    own = p.id;
                }
                return;
            }
            let Some(call) = m.script_call() else { return };
            if call.function == function::USE {
                for (object, quest) in &t.story.shard_quests {
                    let again = shard.is_some_and(|(o, at)| o == *object && m.time - at < 60.0);
                    let named = call.args.windows(8).any(|w| w == object.to_le_bytes());
                    if named && !again {
                        since.push(*quest);
                        shard = Some((*object, m.time));
                    }
                }
            } else if call.function == UPDATE_QUEST {
                if let Some(s) = said_by(&call.args, &lib) {
                    if s.7 >> 28 == 3 && s.0 == 0 {
                        since.push(s.2);
                    }
                }
            } else if call.function == PROFILE && own != 0 {
                let Some(at) = call.args.windows(4).position(|w| w == rating_var) else {
                    return;
                };
                if !call.args[..at].windows(8).any(|w| w == own.to_le_bytes()) {
                    return;
                }
                let word = call.args.get(at + 8..at + 12).expect("the value");
                let now = i64::from(u32::from_le_bytes(word.try_into().expect("four")));
                profiles += 1;
                if let Some(before) = last {
                    let points =
                        |q: &u32| t.quests.quest(*q).map_or(0, |d| i64::from(d.pay.points));
                    let added: i64 = since.iter().map(points).sum();
                    let moved = now - before;
                    if moved == added {
                        agree += usize::from(moved != 0);
                    } else if moved < added {
                        less.push((m.session, before, now, since.clone()));
                    } else {
                        more.push((m.session, before, now, since.clone()));
                    }
                }
                last = Some(now);
                since.clear();
            }
        },
    )
    .expect("the capture index reads");
    println!("2017 ratings: {profiles} own profiles, {agree} moves agree");
    assert!(agree >= 100, "{agree} moves");
    assert!(less.is_empty(), "moved by less: {less:x?}");
    assert!(more.is_empty(), "moved by more: {more:x?}");
}

/// The install's effects a quest's updates put on its player, read as a server reads them.
fn effects_of(def: &yalain_content::quests::QuestDef) -> yalain_world::effect_defs::EffectTable {
    let states: Vec<u32> = def.effects.iter().map(|(_, e)| *e).collect();
    effects_read(&states, &compiled().tables.effect_rules)
}

/// The install's effects of these saved states, read as a server reads them with these rules.
fn effects_read(
    states: &[u32],
    rules: &yalain_world::effect_defs::EffectRules,
) -> yalain_world::effect_defs::EffectTable {
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
    let lib = portal
        .read_checked(RECORD, Library::from_record)
        .expect("the library reads")
        .value;
    yalain_world::effect_defs::EffectTable::read(
        &lib,
        states,
        &|id| {
            let bytes = portal.read(id).ok()?;
            let st: auberean_assets::script::ScriptState =
                auberean_assets::decode(id, &bytes, &ctx).ok()?;
            auberean_script::graph::Graph::decode(&st.graph, &lib).ok()
        },
        rules,
    )
}

/// Put a character at a level.
fn set_level(h: &mut Harness, level: i32) {
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, yalain_world::examine::LEVEL, level);
    }
}

/// **What other quests require has rows**: the quests the setups of others name as required stand
/// in the world, so a requirement asks of a quest a player can hold.
/// Behaviour: server.quests.required-quests-have-rows
#[test]
fn the_quests_other_quests_require_are_in_the_world() {
    let t = &compiled().tables;
    for q in [
        0x4000_0189u32, // the three portals of the bosses' mines
        0x4000_0186,
        0x4000_0183,
        0x4000_0258, // the bug hunts' control
        0x4000_026C, // the two chains' second and fourth
        0x4000_026E,
        0x4000_0271,
        0x4000_0273,
    ] {
        assert!(t.quests.quest(q).is_some(), "{q:08X} has a row");
    }
}

/// **An obelisk search leads its seeker from stone to stone by the portal beside each**: a giving
/// and each stone's advance moves the quest and sends no one, and the portal beside it goes on to the
/// next stone, last to the explorer, as 2017's seekers went (4 to 7 sessions each). A world older
/// than these rows has nothing to play.
/// Behaviour: server.quests.an-obelisk-search-leads-on
#[test]
fn an_obelisk_search_moves_on_at_each_stone_and_the_portal_beside_it_goes_on() {
    let t = &compiled().tables;
    type Stop = (u32, u32, u32);
    const SEARCHES: [(u32, [Stop; 4]); 3] = [
        (
            0x4000_0018,
            [
                (0x4700_31E8, 1, 0x6CB9_0029),
                (0x4700_31EF, 2, 0x5EB4_0022),
                (0x4700_31F0, 3, 0x5FA5_003E),
                (0x4700_31F1, 4, 0x66AF_000F),
            ],
        ),
        (
            0x4000_0017,
            [
                (0x4700_31EA, 1, 0x8CC3_000A),
                (0x4700_31F2, 2, 0x97C3_002F),
                (0x4700_31F3, 3, 0x9ABB_0038),
                (0x4700_31F4, 4, 0x8EB6_000B),
            ],
        ),
        (
            0x4000_0016,
            [
                (0x4700_31E6, 1, 0x7FBB_000B),
                (0x4700_31EC, 2, 0x8AB5_0012),
                (0x4700_31ED, 3, 0x7DAB_0001),
                (0x4700_31EE, 4, 0x76B8_0027),
            ],
        ),
    ];
    for (quest, stops) in SEARCHES {
        let (mut h, b, me) = served(t);
        for (who, phase, landing) in stops {
            let i = t
                .content
                .objects
                .iter()
                .position(|o| o.template == who)
                .expect("placed");
            use_person(&mut h, b, me, t, i);
            let p = h.server.players.values().next().expect("a player");
            assert_eq!(
                yalain_world::quests::phase_of(&p.play.quests, quest),
                phase,
                "{quest:08X} moved on by {who:08X}"
            );
            let at = t.content.objects[i].cell;
            let until = h.now.0 + 12.0;
            h.run_to(yalain_net::LocalTime(until));
            let p = h.server.players.values().next().expect("a player");
            assert_eq!(p.position.cell, at, "{who:08X} sends no one");
            let portal = t
                .content
                .objects
                .iter()
                .position(|o| {
                    o.landblock() == t.content.objects[i].landblock()
                        && o.destination.as_ref().is_some_and(|d| d.cell == landing)
                })
                .unwrap_or_else(|| panic!("a portal beside {who:08X} to {landing:08X}"));
            use_person(&mut h, b, me, t, portal);
            let until = h.now.0 + 12.0;
            h.run_to(yalain_net::LocalTime(until));
            let p = h.server.players.values().next().expect("a player");
            assert_eq!(p.position.cell, landing, "the portal beside {who:08X}");
        }
        let explorer = stops[0].0;
        let i = t
            .content
            .objects
            .iter()
            .position(|o| o.template == explorer)
            .expect("placed");
        let at = t.content.objects[i].cell;
        use_person(&mut h, b, me, t, i);
        let until = h.now.0 + 12.0;
        h.run_to(yalain_net::LocalTime(until));
        let p = h.server.players.values().next().expect("a player");
        assert!(
            p.play
                .quests
                .iter()
                .any(|x| x.quest == quest && x.status == yalain_world::quests::status::COMPLETED),
            "{quest:08X} done"
        );
        assert_eq!(p.position.cell, at, "the last use sends no one");
    }
}

/// **People who send whoever uses them**: the Virindi's envoy says its line and sends a player to
/// Dominion Center, where 16 of 2017's 18 uses landed; the Portal Device where 3 of 3 did.
/// Behaviour: server.quests.a-person-sends-whoever-uses-it
#[test]
fn a_person_who_sends_whoever_uses_it_says_its_line_and_sends_them_where_2017_landed() {
    let t = &compiled().tables;
    for (who, landing, lines) in [
        (
            0x4700_302Du32,
            0x0499_0126u32,
            &[0xA2FE_F207u32, 0x9291_E909][..],
        ),
        (0x4700_305E, 0x04A2_0104, &[][..]),
    ] {
        let (mut h, b, me) = served(t);
        let i = t
            .content
            .objects
            .iter()
            .position(|o| o.template == who)
            .expect("placed");
        let said = use_person(&mut h, b, me, t, i);
        assert!(
            lines.is_empty()
                || said
                    .iter()
                    .any(|(_, s)| s.is_some_and(|s| lines.contains(&s))),
            "{who:08X} says its line: {said:08X?}"
        );
        let until = h.now.0 + 12.0;
        h.run_to(yalain_net::LocalTime(until));
        let p = h.server.players.values().next().expect("a player");
        assert_eq!(p.position.cell, landing, "{who:08X}'s trip");
    }
}

/// **A person says the line numbered for the quest it gives**: Brother Groli's table numbers his
/// three quests, and each giving comes with its own first line and each use after with its own
/// reminder, in every one of a few worlds (he picks at random).
/// Behaviour: server.quests.a-giving-says-its-numbered-line
#[test]
fn a_person_says_the_line_numbered_for_the_quest_it_gives() {
    const GROLI: u32 = 0x4700_308E;
    const LINES: [(u32, u32, u32); 3] = [
        (0x8000_00D3, 0x2AE6_124E, 0xCDEB_1FB2),
        (0x8000_00D4, 0x2AE6_1248, 0xCDEB_2FB2),
        (0x8000_00D5, 0x2AE6_124A, 0xCDEB_3FB2),
    ];
    let t = &compiled().tables;
    let i = t
        .content
        .objects
        .iter()
        .position(|o| o.template == GROLI)
        .expect("placed");
    let mut given = std::collections::HashSet::new();
    for _ in 0..6 {
        let (mut h, b, me) = served(t);
        // Of a level none of his quests holds back.
        for p in h.server.players.values_mut() {
            yalain_world::quals::set_int(&mut p.qualities, 302, 60);
        }
        for _ in 0..given.len() {
            h.server.rng.below(7);
        }
        let said = use_person(&mut h, b, me, t, i);
        let (quest, line, reminder) = LINES
            .iter()
            .find(|(q, _, _)| holds(&h, *q))
            .copied()
            .expect("one of his quests given");
        assert!(
            said.iter().any(|(_, s)| *s == Some(line)),
            "{quest:08X}: its own line {line:08X} in {said:08X?}"
        );
        let said = use_person(&mut h, b, me, t, i);
        assert!(
            said.iter().any(|(_, s)| *s == Some(reminder)),
            "{quest:08X}: its own reminder {reminder:08X} in {said:08X?}"
        );
        given.insert(quest);
    }
}

/// **A posting from its giving to its wait**: the board gives its quest and with it, unseen, its
/// `Underway`; gives no other, saying it sees none, until it is done; the last of 30 kills completes
/// it and leaves `QuestPostingComplete` done; the board then gives nothing for 20 hours, and then
/// gives again. A player under its least level is given nothing.
/// Behaviour: server.quests.a-posting-waits-its-timer
#[test]
fn a_posting_is_given_finished_and_waited_for_as_its_definition_says() {
    use yalain_content::quests::Trigger;
    use yalain_world::quests::status;
    const BOARD: u32 = 0x4700_3182;
    const FELSISTERS: u32 = 0x8000_0216;
    const THRALLS: u32 = 0x8000_023B;
    const UNDERWAY: u32 = 0x8000_028E;
    const COMPLETE: u32 = 0x8000_028D;
    const TORMENT: u32 = 0x4700_2305;
    const NONE_NOW: u32 = 0x83DF_378C;
    let t = &compiled().tables;
    let (mut h, b, me) = served(t);
    h.server.quest_ids = quest_ids();
    h.server.effect_table = effects_of(t.quests.quest(FELSISTERS).expect("a row"));
    h.server
        .quests
        .steps
        .retain(|s| s.trigger != Trigger::Use(BOARD) || s.quest == FELSISTERS);
    let i = t
        .content
        .objects
        .iter()
        .position(|o| o.template == BOARD)
        .expect("placed");
    let said = use_person(&mut h, b, me, t, i);
    assert!(!holds(&h, FELSISTERS), "under its least level");
    assert!(
        said.iter().any(|(_, s)| *s == Some(NONE_NOW)),
        "the board sees none: {said:08X?}"
    );
    set_level(&mut h, 72);
    let said = use_person(&mut h, b, me, t, i);
    assert!(holds(&h, FELSISTERS) && holds(&h, UNDERWAY), "both held");
    assert!(
        said.iter().any(|(_, s)| *s == Some(0x9177_9F7E)),
        "the board's line: {said:08X?}"
    );
    let said = use_person(&mut h, b, me, t, i);
    assert!(
        said.iter().any(|(_, s)| *s == Some(NONE_NOW)),
        "no other while it is under way: {said:08X?}"
    );
    let install = dat_install();
    for _ in 0..30 {
        killed_as(&mut h, me, TORMENT, &install);
    }
    let state = |h: &Harness, q: u32| -> Option<u32> {
        let p = h.server.players.values().next()?;
        p.play
            .quests
            .iter()
            .find(|x| x.quest == q)
            .map(|x| x.status)
    };
    assert_eq!(state(&h, FELSISTERS), Some(status::COMPLETED), "30 kills");
    assert_eq!(state(&h, UNDERWAY), Some(status::COMPLETED), "over");
    assert_eq!(state(&h, COMPLETE), Some(status::COMPLETED), "the timer");
    // Another of the board's quests waits for the timer too.
    h.server.quests = t.quests.clone();
    h.server
        .quests
        .steps
        .retain(|s| s.trigger != Trigger::Use(BOARD) || s.quest == THRALLS);
    h.unix += 3600;
    let said = use_person(&mut h, b, me, t, i);
    assert!(
        !holds(&h, THRALLS) && said.iter().any(|(_, s)| *s == Some(NONE_NOW)),
        "the timer is running: {said:08X?}"
    );
    h.unix += 20 * 3600;
    use_person(&mut h, b, me, t, i);
    assert!(holds(&h, THRALLS), "given once it is over");
}

/// A person says its under-level line to a player too low for what it gives, and gives it from the
/// level recorded: 2017 heard Thin the Boglurk Armoredillos's person refuse a level 34 and give to a
/// level 35. The line stays out of what it says to anyone else: a player who has done the quest is
/// told it has no task, as 2017 heard at level 37. A world older than these rows has nothing to
/// play.
/// Behaviour: server.quests.under-level-line-is-the-persons
#[test]
fn a_person_says_its_under_level_line_to_a_player_too_low_and_to_no_one_else() {
    const PERSON: u32 = 0x4700_3074;
    const QUEST: u32 = 0x8000_00BE;
    const TOO_LOW: u32 = 0x68F4_92A3;
    const NO_TASK: u32 = 0x0920_0EFE;
    let t = &compiled().tables;
    let (mut h, b, me) = served(t);
    let o = t
        .content
        .objects
        .iter()
        .position(|o| o.template == PERSON)
        .expect("the person is placed");
    let level = |h: &mut Harness, level: i32| {
        for p in h.server.players.values_mut() {
            yalain_world::quals::set_int(&mut p.qualities, 302, level);
        }
    };
    level(&mut h, 34);
    let said = use_person(&mut h, b, me, t, o);
    assert!(
        said.iter().any(|(_, s)| *s == Some(TOO_LOW)),
        "its refusal: {said:08X?}"
    );
    assert!(!holds(&h, QUEST), "not given");
    level(&mut h, 35);
    let said = use_person(&mut h, b, me, t, o);
    assert!(holds(&h, QUEST), "given: {said:08X?}");
    // Done, and waiting to be given again: it says what it says to anyone.
    for p in h.server.players.values_mut() {
        p.play.quests.retain(|q| q.quest != QUEST);
        p.play.quests.push(yalain_world::state::QuestState {
            quest: QUEST,
            status: yalain_world::quests::status::COMPLETED,
            phase: 0,
            started: i64::MAX / 2,
            updated: i64::MAX / 2,
        });
    }
    for _ in 0..16 {
        let said = use_person(&mut h, b, me, t, o);
        assert!(
            !said.iter().any(|(_, s)| *s == Some(TOO_LOW)),
            "never to one it would give to: {said:08X?}"
        );
        assert!(
            said.iter().any(|(_, s)| *s == Some(NO_TASK)),
            "it has no task for them: {said:08X?}"
        );
    }
}

/// The quests 2017 listed with no words of their own are shown, not hidden: three entries held
/// the postings' two done quests, with the strings their definitions name, which are empty. A quest
/// is hidden for having no string, not for an empty one.
/// Behaviour: server.quests.empty-words-are-still-listed
#[test]
fn the_quests_2017_listed_with_empty_words_are_listed() {
    use yalain_world::quests::{is_hidden, list, status};
    use yalain_world::state::QuestState;
    let t = &compiled().tables;
    for (quest, name, journal) in [
        (0x8000_028D_u32, 0xC2D0_C9C9_u32, 0xFAA2_0995_u32),
        (0x8000_028B, 0xDB7E_E1BD, 0xFAA2_0995),
    ] {
        const TABLE: u32 = 0x2500_1654;
        let d = t.quests.quest(quest).expect("the quest has a row");
        assert_eq!(d.name, (name, TABLE));
        assert_eq!(d.description, (journal, TABLE));
        assert_eq!(d.icon, 0x4100_034B);
        assert!(!is_hidden(&t.quests, d), "{quest:08X} is shown");
        let held = [QuestState {
            quest,
            status: status::COMPLETED,
            phase: 0,
            started: 0,
            updated: 0,
        }];
        assert_eq!(
            list(&t.quests, &held, 0.0, 0.0)[..4],
            1u32.to_le_bytes(),
            "{quest:08X} is in the list"
        );
    }
    let hidden = [QuestState {
        quest: 0x4000_01CA,
        status: status::COMPLETED,
        phase: 0,
        started: 0,
        updated: 0,
    }];
    assert_eq!(
        list(&t.quests, &hidden, 0.0, 0.0)[..4],
        0u32.to_le_bytes(),
        "a quest with no description is not"
    );
}

/// The postings pay a share of what the player's level takes to reach the next, at most what it
/// comes to at level 72: the 115,274,269 a player's guide of the time gives at that level. A world
/// older than these rows has nothing to play.
/// Behaviour: server.quests.postings-pay-a-share
#[test]
fn a_posting_pays_the_share_the_guide_gives() {
    let t = &compiled().tables;
    let root = t.quests.quest(0x8000_0216).expect("a posting");
    let p = yalain_testkit::authored::Portal::open();
    let levels = p.levels();
    assert_eq!(
        yalain_world::quests::completion_pay(&levels, root, 72),
        115_274_269
    );
    assert_eq!(
        yalain_world::quests::completion_pay(&levels, root, 80),
        115_274_269,
        "no more above it"
    );
    assert!(
        yalain_world::quests::completion_pay(&levels, root, 70) < 115_274_269,
        "less below it"
    );
}

/// Hand an item to a person, as a server takes it: the item made in the pack, then what a give of
/// it triggers.
fn hand_in(h: &mut Harness, portal: &yalain_testkit::authored::Portal, item: u32, to: u32) {
    use yalain_content::quests::Trigger;
    let world = *h.server.players.keys().next().expect("a player");
    let wanted = h
        .server
        .quests
        .steps
        .iter()
        .find(|s| s.trigger == Trigger::Give(item, to) && s.wants.is_some())
        .and_then(|s| s.wants)
        .unwrap_or(1);
    if !h.server.players[&world]
        .play
        .items
        .iter()
        .any(|i| i.template == item)
    {
        h.server.catalog.insert(portal.template(item));
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, item, wanted);
        h.run(2);
    }
    let id = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == item)
        .expect("held")
        .id;
    let clock = h.clock();
    yalain_world::quests::triggered_by_give(
        &mut h.server,
        &mut h.net,
        world,
        Trigger::Give(item, to),
        None,
        yalain_world::gives::Given {
            item: id,
            template: item,
            quantity: wanted,
            target: 0,
        },
        clock,
    );
    h.run(2);
}

/// A quest's status and phase, as its player holds it.
fn quest_state(h: &Harness, quest: u32) -> Option<(u32, u32)> {
    state_of(h, quest)
}

/// A player beside a person, offering it `count` of the `stack` it holds of an item: the lines it
/// told, the stack's quantity left, and whether its pack still holds it.
fn offered(
    h: &mut Harness,
    b: usize,
    me: u64,
    t: &Tables,
    (who, item): (u32, u32),
    (stack, count): (u32, u32),
) -> (Vec<u32>, Option<u32>) {
    use yalain_world::inventory::{move_graph, Move};
    let i = t
        .content
        .objects
        .iter()
        .position(|o| o.template == who)
        .unwrap_or_else(|| panic!("{who:08X} placed"));
    let o = &t.content.objects[i];
    let world = *h.server.players.keys().next().expect("a player");
    {
        let p = h.server.players.get_mut(&world).expect("a player");
        p.position.cell = o.cell;
        p.position.origin = [o.origin[0] + 1.0, o.origin[1], o.origin[2]];
        let mine: Vec<u64> = p
            .play
            .items
            .iter()
            .filter(|x| x.template == item)
            .map(|x| x.id)
            .collect();
        p.play.items.retain(|x| !mine.contains(&x.id));
    }
    let portal = yalain_testkit::authored::Portal::open();
    h.server.catalog.insert(portal.template(item));
    let id =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, item, stack).expect("made");
    h.run(3);
    let slot = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|x| x.id == id)
        .map_or(0, |x| x.slot);
    let slot = i32::try_from(slot).unwrap_or(0);
    let target = yalain_world::presence::content_id(&t.content.objects, i);
    let m = Move {
        move_type: yalain_world::gives::GIVE,
        merge: true,
        unlock: true,
        check_take: true,
        all_taken: true,
        grab: true,
        contain: true,
        auto_merge: true,
        target,
        actual_target: target,
        merge_container: target,
        from: me,
        actual_from: me,
        item: id,
        split_item: id,
        quantity: count,
        quantity_left: count,
        from_slot: slot,
        actual_from_slot: slot,
        ..Move::default()
    };
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::drops::function::MOVE_ITEM, &[move_graph(&m)]),
    );
    h.run(150);
    let told = h
        .calls(b, from)
        .into_iter()
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(yalain_world::calls::Node::TextInfo(t)) => Some(t.string_id),
                _ => None,
            })
        })
        .collect();
    let left = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|x| x.id == id)
        .map(|x| x.quantity);
    (told, left)
}

fn put_at_phase(h: &mut Harness, quest: u32, phase: u32) {
    for p in h.server.players.values_mut() {
        p.play.quests.retain(|q| q.quest != quest);
        p.play.quests.push(yalain_world::state::QuestState {
            quest,
            status: yalain_world::quests::status::UNDERWAY,
            phase,
            started: 0,
            updated: 0,
        });
    }
}

fn state_of(h: &Harness, quest: u32) -> Option<(u32, u32)> {
    h.server.players.values().next().and_then(|p| {
        p.play
            .quests
            .iter()
            .find(|q| q.quest == quest)
            .map(|q| (q.status, q.phase))
    })
}

fn placed(t: &Tables, template: u32) -> usize {
    t.content
        .objects
        .iter()
        .position(|o| o.template == template)
        .unwrap_or_else(|| panic!("{template:08X} placed"))
}

/// **People give what 2017 recorded them giving**: each with its line for offering it; a person's
/// reminder while it runs; its under-level line to a player too low, and no giving; and what its
/// journal asks for handed in finishes it, directly or by its subquests, the job offer there and
/// back through the person it goes to. A world older than these rows has nothing to play.
/// Behaviour: server.quests.recorded-givings
#[test]
fn people_give_the_quests_2017_recorded_and_say_their_lines_for_them() {
    use yalain_world::quests::status;
    const BEGIN: u32 = 0x2AE6_122C;
    const TOO_LOW: u32 = 0x68F4_92A3;
    const REMINDER: u32 = 0x8B6B_186F;
    const FLICK: u32 = 0x4700_32C6;
    const HAKATA: u32 = 0x8000_02EE;
    const LITHIUM: u32 = 0x4700_08E9;
    let t = &compiled().tables;
    let portal = yalain_testkit::authored::Portal::open();
    for (who, quest) in [
        (0x4700_2FB7, 0x8000_0319),
        (0x4700_3293, 0x4000_0101),
        (FLICK, HAKATA),
        (0x4700_32C7, 0x8000_02E3),
        (0x4700_32C3, 0x8000_02F4),
        (0x4700_32CD, 0x8000_02FA),
        (0x4700_32CE, 0x8000_02FB),
        (0x4700_32D2, 0x8000_0303),
    ] {
        let (mut h, b, me) = served(t);
        set_level(&mut h, 60);
        let said = use_person(&mut h, b, me, t, placed(t, who));
        assert!(holds(&h, quest), "{who:08X} gives {quest:08X}: {said:08X?}");
        assert!(
            said.iter().any(|(_, s)| *s == Some(BEGIN)),
            "{who:08X}'s line: {said:08X?}"
        );
    }
    // Under what 2017 gave at and over what it refused at; its reminder; its one hand-in.
    let (mut h, b, me) = served(t);
    let o = placed(t, FLICK);
    set_level(&mut h, 34);
    let said = use_person(&mut h, b, me, t, o);
    assert!(said.iter().any(|(_, s)| *s == Some(TOO_LOW)), "{said:08X?}");
    assert!(!holds(&h, HAKATA), "too low for it");
    set_level(&mut h, 35);
    use_person(&mut h, b, me, t, o);
    assert!(holds(&h, HAKATA), "given");
    let said = use_person(&mut h, b, me, t, o);
    assert!(
        said.iter().any(|(_, s)| *s == Some(REMINDER)),
        "reminded: {said:08X?}"
    );
    hand_in(&mut h, &portal, LITHIUM, FLICK);
    assert_eq!(
        quest_state(&h, HAKATA).map(|q| q.0),
        Some(status::COMPLETED)
    );
    // A quest of subquests: each of its items, and it is done with the last.
    let (mut h, b, me) = served(t);
    set_level(&mut h, 60);
    const DAX: u32 = 0x4700_32C3;
    const ARMOR: u32 = 0x8000_02F4;
    use_person(&mut h, b, me, t, placed(t, DAX));
    let items = [0x4700_08D7, 0x4700_06C6, 0x4700_1CB0, 0x4700_18EB];
    for item in items {
        assert_eq!(
            quest_state(&h, ARMOR),
            Some((status::UNDERWAY, 1)),
            "before {item:08X}"
        );
        hand_in(&mut h, &portal, item, DAX);
    }
    assert_eq!(quest_state(&h, ARMOR).map(|q| q.0), Some(status::COMPLETED));
    // The job offer: to the person it goes to, and his answer back.
    let (mut h, b, me) = served(t);
    const THRIBBLE: u32 = 0x4700_2FB9;
    const DRANDY: u32 = 0x4700_2FBA;
    const OFFER: u32 = 0x8000_031A;
    set_level(&mut h, 42);
    let said = use_person(&mut h, b, me, t, placed(t, THRIBBLE));
    assert!(said.iter().any(|(_, s)| *s == Some(TOO_LOW)), "{said:08X?}");
    set_level(&mut h, 43);
    h.server.catalog.insert(portal.template(0x4700_1415));
    h.server.catalog.insert(portal.template(0x4700_1416));
    use_person(&mut h, b, me, t, placed(t, THRIBBLE));
    assert_eq!(quest_state(&h, OFFER), Some((status::UNDERWAY, 1)));
    let held = |h: &Harness, item: u32| {
        h.server
            .players
            .values()
            .next()
            .is_some_and(|p| p.play.items.iter().any(|i| i.template == item))
    };
    h.run(40);
    assert!(held(&h, 0x4700_1415), "he hands over the offer");
    hand_in(&mut h, &portal, 0x4700_1415, DRANDY);
    assert_eq!(quest_state(&h, OFFER), Some((status::UNDERWAY, 2)));
    h.run(40);
    assert!(held(&h, 0x4700_1416), "he hands over his answer");
    hand_in(&mut h, &portal, 0x4700_1416, THRIBBLE);
    assert_eq!(quest_state(&h, OFFER).map(|q| q.0), Some(status::COMPLETED));
}

/// **An under-level line is no plain line**: no placed person who is left with another line keeps
/// its under-level line in what it says to anyone.
/// Behaviour: server.quests.under-level-line-out-of-the-plain-pool
#[test]
fn no_placed_person_with_anything_else_to_say_says_its_under_level_line_to_anyone() {
    const TOO_LOW: u32 = 0x68F4_92A3;
    let t = &compiled().tables;
    let plain: Vec<u32> = t
        .talk
        .lines
        .iter()
        .filter(|l| {
            matches!(l.words, yalain_content::talk::Words::Ref(s, _) if s == TOO_LOW)
                && l.quest.is_none()
                && t.content.objects.iter().any(|o| o.template == l.who)
        })
        .map(|l| l.who)
        .collect();
    assert!(plain.is_empty(), "{plain:08X?}");
}

/// **Lahry sends no one**: of the 40 recorded trips from beside him, 31 used the portal five meters
/// from him and 9 walked into it, and none followed from him. At his quest's fifth phase he says
/// his reminder and the player stays; the portal beside him does the sending. At its sixth he says
/// the sixth's reminder.
/// Behaviour: server.quests.a-reminder-at-its-phase-sends-no-one
#[test]
fn lahry_reminds_a_player_at_the_fifth_phase_and_the_portal_beside_him_sends_them() {
    const LAHRY: u32 = 0x4700_33D6;
    const RABBIT: u32 = 0x4000_0294;
    const FIFTH: u32 = 0xC964_3512;
    const SIXTH: u32 = 0x0964_3512;
    let t = &compiled().tables;
    let (mut h, b, me) = served(t);
    set_level(&mut h, 60);
    let o = placed(t, LAHRY);
    for phase in 1..=5 {
        use_person(&mut h, b, me, t, o);
        assert_eq!(
            quest_state(&h, RABBIT).map(|q| q.1),
            Some(phase),
            "phase {phase}"
        );
        let until = h.now.0 + 8.0;
        h.run_to(yalain_net::LocalTime(until));
    }
    let said = use_person(&mut h, b, me, t, o);
    assert!(
        said.iter().any(|(_, s)| *s == Some(FIFTH)),
        "his fifth's: {said:08X?}"
    );
    assert!(
        !said.iter().any(|(_, s)| *s == Some(SIXTH)),
        "not the sixth's: {said:08X?}"
    );
    let until = h.now.0 + 12.0;
    h.run_to(yalain_net::LocalTime(until));
    let p = h.server.players.values().next().expect("a player");
    assert_eq!(
        p.position.cell, t.content.objects[o].cell,
        "he sends no one"
    );
    for p in h.server.players.values_mut() {
        if let Some(q) = p.play.quests.iter_mut().find(|q| q.quest == RABBIT) {
            q.phase = 6;
        }
    }
    let said = use_person(&mut h, b, me, t, o);
    assert!(
        said.iter().any(|(_, s)| *s == Some(SIXTH)),
        "his sixth's: {said:08X?}"
    );
    let portal = t
        .content
        .objects
        .iter()
        .position(|x| x.template == 0x4700_1154 && x.landblock() == 0x75A5)
        .expect("the portal beside him");
    use_person(&mut h, b, me, t, portal);
    let until = h.now.0 + 12.0;
    h.run_to(yalain_net::LocalTime(until));
    let p = h.server.players.values().next().expect("a player");
    assert_eq!(p.position.cell, 0x09F2_0100, "the portal's trip");
}

/// **The three obelisks' order**: twelve touches in the one order recorded, each with its click,
/// and the last sends the player on. A touch out of order fails the hidden quest with the
/// obelisks' failure and the next touch of the first obelisk starts it over; before the order is
/// begun the others do nothing. A player under the quest's least level gets nothing of it.
/// Behaviour: server.quests.the-antechamber-code
#[test]
fn three_obelisks_touched_in_their_order_lead_on_and_a_wrong_touch_starts_over() {
    use yalain_world::quests::status;
    const CENTER: u32 = 0x4700_3000;
    const LEFT: u32 = 0x4700_3001;
    const RIGHT: u32 = 0x4700_3002;
    const CODE: [u32; 12] = [
        CENTER, LEFT, CENTER, LEFT, RIGHT, LEFT, RIGHT, CENTER, LEFT, LEFT, RIGHT, CENTER,
    ];
    const QUEST: u32 = 0x4000_0264;
    const CLICK: u32 = 0xE3DF_2CC2;
    const FAILED: u32 = 0x6A4B_D3CB;
    const NOTHING: u32 = 0x8B5B_AB29;
    let t = &compiled().tables;
    let (mut h, b, me) = served(t);
    set_level(&mut h, 44);
    use_person(&mut h, b, me, t, placed(t, CENTER));
    assert_eq!(quest_state(&h, QUEST), None, "under its least level");
    set_level(&mut h, 45);
    let said = use_person(&mut h, b, me, t, placed(t, LEFT));
    assert!(said.iter().any(|(_, s)| *s == Some(NOTHING)), "{said:08X?}");
    assert_eq!(quest_state(&h, QUEST), None, "not the order's first");
    use_person(&mut h, b, me, t, placed(t, CENTER));
    assert_eq!(quest_state(&h, QUEST), Some((status::UNDERWAY, 1)));
    let said = use_person(&mut h, b, me, t, placed(t, RIGHT));
    assert!(said.iter().any(|(_, s)| *s == Some(FAILED)), "{said:08X?}");
    assert_eq!(quest_state(&h, QUEST), Some((status::FAILED, 0)));
    for (n, who) in CODE.iter().enumerate() {
        let said = use_person(&mut h, b, me, t, placed(t, *who));
        assert!(
            said.iter().any(|(_, s)| *s == Some(CLICK)),
            "touch {n}: {said:08X?}"
        );
        if n + 1 < CODE.len() {
            assert_eq!(
                quest_state(&h, QUEST),
                Some((status::UNDERWAY, u32::try_from(n).expect("small") + 1))
            );
        }
    }
    assert_eq!(quest_state(&h, QUEST).map(|q| q.0), Some(status::COMPLETED));
    let until = h.now.0 + 12.0;
    h.run_to(yalain_net::LocalTime(until));
    let p = h.server.players.values().next().expect("a player");
    assert_eq!(p.position.cell, 0x09F3_0127, "where it leads");
}

/// **A reminder needs a quest under way**: two people's lines asking why the player is still there
/// are said with one of their quests under way, and never to a player with none.
/// Behaviour: server.talk.a-reminder-is-said-with-a-quest-under-way
#[test]
fn a_reminder_line_is_said_with_a_quest_under_way_and_not_without() {
    use yalain_content::talk::Words;
    use yalain_world::quests::status;
    let t = &compiled().tables;
    for (who, string, quest) in [
        (0x4700_335B, 0xBFE7_E842, 0x4000_037A),
        (0x4700_3353, 0xCA33_899B, 0x4000_028C),
    ] {
        let (mut h, _b, _me) = served(t);
        let world = *h.server.players.keys().next().expect("a player");
        for _ in 0..64 {
            let line = yalain_world::talk::at_use(&mut h.server, world, who, None);
            assert!(
                !line.is_some_and(|l| matches!(l.words, Words::Ref(s, _) if s == string)),
                "{who:08X} to a player with nothing under way"
            );
        }
        for p in h.server.players.values_mut() {
            p.play.quests.push(yalain_world::state::QuestState {
                quest,
                status: status::UNDERWAY,
                phase: 1,
                started: 0,
                updated: 0,
            });
        }
        let line = yalain_world::talk::at_use(&mut h.server, world, who, None).expect("a line");
        assert!(
            matches!(line.words, Words::Ref(s, _) if s == string),
            "{who:08X} with its quest under way: {:08X?}",
            line.words
        );
    }
}

/// **Points no recording measured, where something supports them**: other quests of people whose
/// measured quests all add one value, a guide's figure as far as it holds. Where measures disagree
/// with no rule between them the world says nothing.
/// Behaviour: server.quests.points-a-pattern-supports
#[test]
fn quests_no_recording_measured_add_what_their_pattern_or_a_guide_supports() {
    let t = &compiled().tables;
    let points = |q: u32| t.quests.quest(q).expect("a row").pay.points;
    for (q, want) in [
        (0x4000_0176, 50),
        (0x4000_0182, 50),
        (0x4000_01AF, 50),
        (0x4000_027D, 1200),
        (0x4000_028B, 1200),
        (0x4000_0299, 1200),
    ] {
        assert_eq!(points(q), want, "{q:08X}");
    }
    assert_eq!(points(0x4000_028D), 1850, "measured, kept");
    assert_eq!(points(0x4000_0119), 0, "no rule between three measures");
    assert_eq!(points(t_quest("QuestAncientPeriapt")), 4200);
    assert_eq!(points(t_quest("QuestRashansRevenge")), 2000);
}

fn t_quest(name: &str) -> u32 {
    *quest_ids().get(name).expect("named")
}

/// **A hand-in that names a count wants that count**: Sparks takes 3 of each element, split off a
/// bigger stack, says the install's line for any other number and hands it back; his quest is done
/// with the second element and not the first, whichever comes first. Lupe, wanting her fins one at
/// a time, refuses three plainly as 2017 heard and takes one; the Miller of Kehan, who names no
/// count, takes 82 stones whole as 2017 did. A world older than these rows has nothing to play.
/// Behaviour: server.gives.counted-hand-ins
#[test]
fn hand_ins_naming_a_count_want_it_and_the_wrong_number_has_its_line() {
    use yalain_world::quests::status;
    const SPARKS: u32 = 0x4700_32C5;
    const BARIUM: u32 = 0x4700_08ED;
    const STRONIUM: u32 = 0x4700_08EB;
    const SHOUSHI: u32 = 0x8000_02E4;
    const WRONG_NUMBER: u32 = 0x4F33_0AB6;
    const DONE: u32 = 0x68A0_7C4D;
    let t = &compiled().tables;

    for (first, second) in [(BARIUM, STRONIUM), (STRONIUM, BARIUM)] {
        let (mut h, b, me) = served(t);
        set_level(&mut h, 60);
        let at = t.content.objects.iter().position(|o| o.template == SPARKS);
        use_person(&mut h, b, me, t, at.expect("placed"));
        assert_eq!(state_of(&h, SHOUSHI), Some((status::UNDERWAY, 1)));
        for (stack, count) in [(5, 2), (5, 5), (2, 2)] {
            let (told, left) = offered(&mut h, b, me, t, (SPARKS, first), (stack, count));
            assert!(
                told.contains(&WRONG_NUMBER),
                "{count} of {stack}: {told:08X?}"
            );
            assert_eq!(left, Some(stack), "{count} of {stack} comes back");
        }
        let (told, left) = offered(&mut h, b, me, t, (SPARKS, first), (5, 3));
        assert!(!told.contains(&WRONG_NUMBER), "3 of 5: {told:08X?}");
        assert!(
            !told.contains(&DONE),
            "one element is not the quest: {told:08X?}"
        );
        assert_eq!(left, Some(2), "the rest stays");
        assert_eq!(state_of(&h, SHOUSHI), Some((status::UNDERWAY, 1)));
        let (told, left) = offered(&mut h, b, me, t, (SPARKS, second), (3, 3));
        assert!(told.contains(&DONE), "both are: {told:08X?}");
        assert_eq!(left, None, "the whole stack of three leaves the pack");
        assert_eq!(state_of(&h, SHOUSHI).map(|q| q.0), Some(status::COMPLETED));
    }

    // Lupe: three at once refused with her plain line, one at a time taken.
    const LUPE: u32 = 0x4700_3353;
    const FINS: u32 = 0x4700_1CC4;
    const FIN_QUEST: u32 = 0x4000_027E;
    const NOT_WANTED: u32 = 0x098E_5251;
    let (mut h, b, me) = served(t);
    put_at_phase(&mut h, FIN_QUEST, 1);
    let (told, left) = offered(&mut h, b, me, t, (LUPE, FINS), (3, 3));
    assert!(told.contains(&NOT_WANTED), "{told:08X?}");
    assert_eq!(left, Some(3));
    assert_eq!(state_of(&h, FIN_QUEST), Some((status::UNDERWAY, 1)));
    let (_, left) = offered(&mut h, b, me, t, (LUPE, FINS), (3, 1));
    assert_eq!(left, Some(2));
    assert_eq!(state_of(&h, FIN_QUEST), Some((status::UNDERWAY, 2)));

    // The Miller of Kehan names no count: 82 stones go whole, as 1 does.
    const MILLER: u32 = 0x4700_2FBC;
    const STONE: u32 = 0x4700_06C6;
    const WINDMILL: u32 = 0x4000_00C7;
    for (stack, count, left) in [(82, 82, None), (82, 1, Some(81))] {
        let (mut h, b, me) = served(t);
        put_at_phase(&mut h, WINDMILL, 2);
        let (_, now) = offered(&mut h, b, me, t, (MILLER, STONE), (stack, count));
        assert_eq!(now, left, "{count} of {stack}");
        assert_eq!(state_of(&h, WINDMILL), Some((status::UNDERWAY, 3)));
    }

    // "I said 25 gold!" is for the wrong number of coins, and for nothing else given him.
    const DRIZZLE: u32 = 0x4700_32C8;
    const COIN: u32 = 0x4700_0538;
    const SAID_25: u32 = 0x11EE_CD9A;
    let (mut h, b, me) = served(t);
    let (told, left) = offered(&mut h, b, me, t, (DRIZZLE, COIN), (30, 10));
    assert!(told.contains(&SAID_25), "{told:08X?}");
    assert_eq!(left, Some(30));
    let (told, _) = offered(&mut h, b, me, t, (DRIZZLE, STONE), (1, 1));
    assert!(!told.contains(&SAID_25), "{told:08X?}");
}

/// The rows that name a count, as their quests' journals and people say.
/// Behaviour: server.gives.counted-hand-ins
#[test]
fn every_hand_in_that_names_a_count_wants_it() {
    use yalain_content::quests::Trigger;
    let t = &compiled().tables;
    let wants = |quest: u32, item: u32| -> Vec<Option<u32>> {
        t.quests
            .steps
            .iter()
            .filter(|s| s.quest == quest && matches!(s.trigger, Trigger::Give(i, _) if i == item))
            .map(|s| s.wants)
            .collect()
    };
    let named = [
        (0x4000_0179, 0x4700_08D2, 250),
        (0x4000_0180, 0x4700_06C3, 20),
        (0x8000_0343, 0x4700_1779, 10),
        (0x8000_0359, 0x4700_1CAC, 4),
        (0x8000_035E, 0x4700_1BAA, 5),
        (0x8000_037B, 0x4700_11DE, 6),
        (0x8000_033F, 0x4700_1CB7, 2),
        (0x8000_0351, 0x4700_1CE1, 3),
        (0x8000_0352, 0x4700_1CE2, 3),
        (0x8000_02E5, 0x4700_0538, 25),
        (0x8000_02E6, 0x4700_0538, 25),
        (0x4000_032A, 0x4700_0538, 1000),
        (0x8000_02F0, 0x4700_06C6, 300),
        (0x8000_02FD, 0x4700_06CB, 200),
    ];
    for (quest, item, count) in named {
        assert_eq!(wants(quest, item), vec![Some(count)], "{quest:08X}");
    }
    assert_eq!(
        wants(0x4000_027E, 0x4700_1CC4),
        vec![Some(1); 3],
        "a fin a phase"
    );
    assert_eq!(
        wants(0x4000_00C7, 0x4700_06C6),
        vec![None],
        "a pile of stone"
    );
    assert!(
        wants(0x8000_02E4, 0x4700_08EB).is_empty(),
        "Sparks's quest is done by its subquests"
    );
}

/// The player's quest at a phase, as a save holds it.
fn quest_at(h: &mut Harness, quest: u32, phase: u32) {
    for p in h.server.players.values_mut() {
        p.play.quests.retain(|q| q.quest != quest);
        p.play.quests.push(yalain_world::state::QuestState {
            quest,
            status: yalain_world::quests::status::UNDERWAY,
            phase,
            started: 0,
            updated: 0,
        });
    }
}

/// The world a test's player is in.
fn player_world(h: &Harness) -> yalain_net::ConnId {
    *h.server.players.keys().next().expect("a player")
}

const COIN: u32 = 0x4700_0538;
const JUNK: u32 = 0x4700_2108;

/// The strings a person says of a given item, in order, as it takes it by a give moving no quest.
fn taking(h: &mut Harness, t: &Tables, who: u32, item: u32) -> Vec<u32> {
    let i = placed(t, who);
    yalain_world::talk::with_give(&mut h.server, who, Some(i), item)
        .iter()
        .map(|l| match l.words {
            yalain_content::talk::Words::Ref(s, _) => s,
            yalain_content::talk::Words::Literal(_) => 0,
        })
        .collect()
}

/// What a person says as it refuses a given item.
fn refusing(h: &mut Harness, t: &Tables, who: u32, item: u32) -> Option<u32> {
    let i = placed(t, who);
    yalain_world::talk::at_give(&mut h.server, who, Some(i), item).map(|l| match l.words {
        yalain_content::talk::Words::Ref(s, _) => s,
        yalain_content::talk::Words::Literal(_) => 0,
    })
}

/// **Gives 2017 kept that moved nothing the player could see**: an offering's chain, a shrine's
/// recipe, the mirras charged, a bucket filled, the alembic dipped again, the donation, the bribe, the
/// carcass: each runs for the count recorded, gives what came back, says its first line recorded.
/// Behaviour: server.gives.kept-in-2017
#[test]
fn gives_2017_kept_with_no_quest_moved_have_their_rows() {
    // Item, to, count, what came back, its first line.
    #[allow(clippy::type_complexity)]
    let kept: &[(u32, u32, u32, &[(u32, u32)], u32)] = &[
        (0x4700_1779, 0x4700_31D9, 1, &[], 0xC0F1_6AB7),
        (
            0x4700_1765,
            0x4700_31D9,
            1,
            &[(0x4700_176F, 1)],
            0x6082_3410,
        ),
        (
            0x4700_176F,
            0x4700_31E5,
            1,
            &[(0x4700_1777, 1)],
            0xBCEB_0FF2,
        ),
        (
            0x4700_1777,
            0x4700_31CC,
            1,
            &[(0x4700_1775, 1)],
            0xC66B_08F6,
        ),
        (
            0x4700_1775,
            0x4700_31D4,
            1,
            &[(0x4700_1758, 8)],
            0x5A61_9B97,
        ),
        (
            0x4700_1742,
            0x4700_31C3,
            1,
            &[(0x4700_1740, 1)],
            0x8839_3C97,
        ),
        (
            0x4700_1732,
            0x4700_31C3,
            1,
            &[(0x4700_1730, 1)],
            0xA13E_7640,
        ),
        (
            0x4700_1749,
            0x4700_31C3,
            1,
            &[(0x4700_1747, 1)],
            0xE53C_1549,
        ),
        (
            0x4700_185E,
            0x4700_31AC,
            1,
            &[(0x4700_185F, 1)],
            0x5D77_9E06,
        ),
        (COIN, 0x4700_327C, 50, &[], 0x61DE_D4C0),
        (COIN, 0x4700_3009, 5000, &[(0x4700_10DA, 1)], 0xA076_90AC),
        (
            0x4700_1BA1,
            0x4700_33D6,
            1,
            &[(0x4700_1B92, 1)],
            0x014B_8529,
        ),
    ];
    let t = &compiled().tables;
    let (mut h, _, _) = served(t);
    let world = player_world(&h);
    for &(item, to, count, back, first) in kept {
        let row = yalain_world::quests::give_for(&h.server, world, item, to, count)
            .unwrap_or_else(|| panic!("{item:08X} to {to:08X}"));
        let got: Vec<(u32, u32)> = row
            .gives
            .iter()
            .enumerate()
            .map(|(n, g)| (*g, row.counts.get(n).copied().unwrap_or(1)))
            .collect();
        assert_eq!(got, back, "{item:08X} to {to:08X}");
        assert_eq!(
            taking(&mut h, t, to, item).first(),
            Some(&first),
            "{item:08X} to {to:08X}"
        );
    }
    // The shrine's recipe, the healer's spell.
    let row = yalain_world::quests::give_for(&h.server, world, 0x4700_1779, 0x4700_31D9, 1);
    assert_eq!(row.map(|r| r.effects), Some(vec![0x6F00_1DEF]));
    let row = yalain_world::quests::give_for(&h.server, world, COIN, 0x4700_327C, 50);
    assert_eq!(row.map(|r| r.effects), Some(vec![0x6F00_1ED0]));
    // The carcass's lines, in the order they came: eight once the world holds the call that names
    // the giver, seven in a release before it.
    let lines = taking(&mut h, t, 0x4700_33D6, 0x4700_1BA1);
    assert!(lines.len() == 8 || lines.len() == 7, "{lines:x?}");
    if lines.len() == 8 {
        assert_eq!(
            lines[6], 0x9C2D_B3BC,
            "after the emote, before the last tell"
        );
        assert_eq!(lines[7], 0x014A_A529);
    }
    // A count not the one asked is refused with the person's own words for it.
    assert!(yalain_world::quests::give_for(&h.server, world, COIN, 0x4700_327C, 49).is_none());
    assert_eq!(refusing(&mut h, t, 0x4700_327C, COIN), Some(0xF961_FDEF));
    assert!(yalain_world::quests::give_for(&h.server, world, COIN, 0x4700_3009, 100).is_none());
    assert_eq!(refusing(&mut h, t, 0x4700_3009, COIN), Some(0x4FFF_35F3));
}

/// **The alembic dipped again**: at the quest's third phase the geysers fill it again and teach the
/// recipe again (32 of 32 such gives moved no quest); at no other phase but the first, which is the
/// quest's own step.
/// Behaviour: server.gives.kept-in-2017
#[test]
fn an_alembic_is_dipped_again_at_the_third_phase_alone() {
    const ALEMBIC: u32 = 0x4700_0B38;
    const ORLEN: u32 = 0x4000_018F;
    let t = &compiled().tables;
    let (mut h, _, _) = served(t);
    let world = player_world(&h);
    for (geyser, waters, recipe) in [
        (0x4700_31AC, 0x4700_0B39, 0x6F00_1DA6),
        (0x4700_31AE, 0x4700_0B3A, 0x6F00_1DA7),
    ] {
        for phase in [0, 2] {
            quest_at(&mut h, ORLEN, phase);
            assert!(
                yalain_world::quests::give_for(&h.server, world, ALEMBIC, geyser, 1).is_none(),
                "{geyser:08X} at {phase}"
            );
        }
        quest_at(&mut h, ORLEN, 3);
        let row = yalain_world::quests::give_for(&h.server, world, ALEMBIC, geyser, 1)
            .unwrap_or_else(|| panic!("{geyser:08X}"));
        assert_eq!(row.gives, vec![waters]);
        assert_eq!(row.effects, vec![recipe]);
    }
}

/// **The Warders take anything**, each thanking in its own land's words (2017: 5 of 5 of 3 items to
/// 3 Warders, every word as its land's), and leave no other land's line.
/// Behaviour: server.gives.kept-in-2017
#[test]
fn the_warders_take_any_item_and_thank_in_their_own_lands_words() {
    const OSTETH: u32 = 0x7D8F_7E9A;
    const OMISHAN: u32 = 0x893E_BF9D;
    let t = &compiled().tables;
    let (mut h, _, _) = served(t);
    let world = player_world(&h);
    for (who, line) in [
        (0x4700_3224, OSTETH),
        (0x4700_323F, OSTETH),
        (0x4700_3220, OMISHAN),
    ] {
        assert!(yalain_world::quests::give_for(&h.server, world, JUNK, who, 1).is_some());
        assert_eq!(taking(&mut h, t, who, JUNK), vec![line], "{who:08X}");
    }
    let lines = |who: u32| {
        t.talk
            .lines
            .iter()
            .filter(|l| l.who == who && l.when == yalain_content::talk::When::Give)
            .count()
    };
    assert_eq!(lines(0x4700_3220), 1, "its own land's line alone");
    let warders = t
        .quests
        .gives
        .iter()
        .filter(|g| g.item.is_none() && lines(g.to) == 1)
        .count();
    assert!(warders >= 20, "{warders}");
}

/// **A miner's toll**: the pyreal starts a hidden quest, its portals ask for that quest under way, and
/// a player who has paid hears the miner's line for a toll still good.
/// Behaviour: server.gives.mine-tolls
#[test]
fn a_miners_toll_is_a_hidden_quest_its_portals_ask_for() {
    let t = &compiled().tables;
    for (miner, pyreal, quest, portals, good) in [
        (
            0x4700_31B2,
            0x4700_0AFF,
            0x4000_0183,
            &[0x4079_9400_0000_0003_u64][..],
            0xF2AE_9DAA,
        ),
        (
            0x4700_31B0,
            0x4700_0B10,
            0x4000_0186,
            &[0x40BA_8700_0000_0013][..],
            0xF2AE_9DAA,
        ),
        (
            0x4700_31B6,
            0x4700_0AFD,
            0x4000_0189,
            &[0x4056_6000_0000_0004][..],
            0xF2AE_9DAA,
        ),
    ] {
        let def = t
            .quests
            .quest(quest)
            .unwrap_or_else(|| panic!("{quest:08X}"));
        assert!(def.hidden, "{quest:08X}");
        let step = t
            .quests
            .steps
            .iter()
            .find(|s| s.trigger == yalain_content::quests::Trigger::Give(pyreal, miner))
            .unwrap_or_else(|| panic!("{miner:08X}"));
        assert_eq!(
            (step.quest, step.needs, step.takes.as_slice()),
            (quest, 0, &[pyreal][..])
        );
        for id in portals {
            let o = t
                .content
                .objects
                .iter()
                .find(|o| o.object == Some(*id))
                .unwrap_or_else(|| panic!("{id:016X}"));
            let gate = o.gate.unwrap_or_else(|| panic!("{id:016X} is gated"));
            assert_eq!(gate.quest, quest, "{id:016X}");
        }
        let (mut h, b, me) = served(t);
        let o = placed(t, miner);
        let said = use_person(&mut h, b, me, t, o);
        assert!(!said.iter().any(|(_, s)| *s == Some(good)), "{said:08X?}");
        quest_at(&mut h, quest, 1);
        let said = use_person(&mut h, b, me, t, o);
        assert!(said.iter().any(|(_, s)| *s == Some(good)), "{said:08X?}");
    }
}

/// **The gem hand-in and the periapt**: the jade stone finishes its quest and pays its fee; the altars
/// give the three gems as the quest's subquests, the periapt made and given moves it to its last
/// phase, as one character's whole run of it was recorded.
/// Behaviour: server.gives.kept-in-2017
#[test]
fn a_gem_and_a_periapt_given_move_their_quests() {
    use yalain_world::quests::status;
    let t = &compiled().tables;
    let portal = yalain_testkit::authored::Portal::open();
    let (mut h, _, _) = served(t);
    quest_at(&mut h, 0x4000_02EE, 1);
    hand_in(&mut h, &portal, 0x4700_1045, 0x4700_2FD7);
    assert_eq!(
        quest_state(&h, 0x4000_02EE).map(|q| q.0),
        Some(status::COMPLETED)
    );
    let fee = t
        .quests
        .steps
        .iter()
        .find(|s| s.trigger == yalain_content::quests::Trigger::Give(0x4700_1045, 0x4700_2FD7))
        .map(|s| s.counts.clone());
    assert_eq!(fee, Some(vec![900]));

    const PERIAPT_Q: u32 = 0x4000_0071;
    const PERIAPT: u32 = 0x4700_0898;
    let (mut h, b, me) = served(t);
    set_level(&mut h, 60);
    use_person(&mut h, b, me, t, placed(t, 0x4700_2EF7));
    assert_eq!(
        quest_state(&h, PERIAPT_Q),
        Some((status::UNDERWAY, 1)),
        "given"
    );
    // Each altar its own subquest and gem, and none before it is used.
    for (altar, sub, gem) in [
        (0x4700_2EF8, 0x4000_0072, 0x4700_0895),
        (0x4700_2EF9, 0x4000_0074, 0x4700_0897),
        (0x4700_2EFA, 0x4000_0073, 0x4700_0896),
    ] {
        assert_eq!(
            quest_state(&h, PERIAPT_Q),
            Some((status::UNDERWAY, 1)),
            "before {altar:08X}"
        );
        assert_eq!(
            quest_state(&h, sub),
            Some((status::UNDERWAY, 1)),
            "before {altar:08X}"
        );
        let step = t
            .quests
            .steps
            .iter()
            .find(|s| s.quest == sub)
            .unwrap_or_else(|| panic!("{sub:08X}"));
        assert_eq!(step.gives, vec![gem], "{altar:08X}");
        use_person(&mut h, b, me, t, placed(t, altar));
        assert_eq!(
            quest_state(&h, sub).map(|q| q.0),
            Some(status::COMPLETED),
            "{altar:08X}"
        );
    }
    assert_eq!(
        quest_state(&h, PERIAPT_Q),
        Some((status::UNDERWAY, 2)),
        "the three gems"
    );
    let world = player_world(&h);
    h.server.catalog.insert(portal.template(PERIAPT));
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, PERIAPT, 1);
    h.run(2);
    let clock = h.clock();
    yalain_world::quests::triggered(
        &mut h.server,
        &mut h.net,
        world,
        yalain_content::quests::Trigger::Take(PERIAPT),
        None,
        clock,
    );
    assert_eq!(
        quest_state(&h, PERIAPT_Q),
        Some((status::UNDERWAY, 3)),
        "made"
    );
    hand_in(&mut h, &portal, PERIAPT, 0x4700_2EF7);
    assert_eq!(
        quest_state(&h, PERIAPT_Q),
        Some((status::UNDERWAY, 4)),
        "given"
    );
}

/// **A wrong give hears only what fits it**: a plain give hears none of the lines about one item
/// (2017: 0 of 30 wrong gives said one), and a line about one item comes for that item.
/// Behaviour: server.gives.a-line-for-an-item
#[test]
fn a_wrong_give_hears_no_line_about_another_item() {
    let t = &compiled().tables;
    let (mut h, _, _) = served(t);
    // A line about one item, and the people who have it.
    let named: &[(u32, u32, Option<u32>)] = &[
        (0x4700_2FBB, 0x8B5D_89B4, None),
        (0x4700_2FBB, 0x8B5D_89B8, None),
        (0x4700_3173, 0x6279_7D94, None),
        (0x4700_31B9, 0x7900_23E0, None),
        (0x4700_32BD, 0x1136_CD8C, None),
        (0x4700_32CB, 0x11EE_CD9A, None),
        (0x4700_32D0, 0x3DE6_3E47, None),
        (0x4700_32D0, 0x675D_C980, None),
        (0x4700_32D3, 0x2530_079C, None),
        (0x4700_32D3, 0x7D9E_3D81, None),
        (0x4700_32D3, 0xA3C0_6CC4, Some(0x4700_1902)),
        (0x4700_32D7, 0x79F7_73E8, None),
        (0x4700_2FB4, 0x217A_D287, Some(0x4700_08FB)),
        (0x4700_3367, 0x11EE_C3FA, Some(COIN)),
        (0x4700_3319, 0x7DE6_7DA2, Some(0x4700_0912)),
    ];
    for &(who, line, item) in named {
        for _ in 0..16 {
            assert_ne!(
                refusing(&mut h, t, who, JUNK),
                Some(line),
                "{who:08X}: {line:08X}"
            );
        }
        if let Some(item) = item {
            assert_eq!(
                refusing(&mut h, t, who, item),
                Some(line),
                "{who:08X}: {item:08X}"
            );
        }
    }
    // What the recordings heard of two items stays plain.
    assert_eq!(
        refusing(&mut h, t, 0x4700_31AE, JUNK),
        Some(0x2DDF_72B1),
        "the geyser's"
    );
    assert_eq!(
        refusing(&mut h, t, 0x4700_3353, JUNK),
        Some(0x098E_5251),
        "Lupe's"
    );
}

/// Every recorded record of a quest under way says it has time left exactly where the world's row
/// says its time runs: at the phase its duration runs in, else at every phase of a quest with one.
/// Behaviour: server.quests.timed-as-the-records-show-them
#[test]
fn every_phase_a_recorded_quest_shows_time_at_is_one_the_world_times() {
    use auberean_script::args::{decode, Arg};
    use auberean_script::graph::{Body, PackedObject};
    use auberean_script::native_object::{NativeObject, QuestInfo};
    use yalain_world::quests::status;
    use yalain_world::session::function::SESSION_START;
    const UPDATE_QUEST: u32 = 0x8213_0024;
    let t = &compiled().tables;
    let lib = super::keeping::library().expect("the client library reads");
    let con = super::keeping::open(&auberean_testkit::data::require_capture_index());
    let mut st = con
        .prepare(
            "SELECT m.raw FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) \
             WHERE c.file LIKE '2017-%' AND s.game = 'ac2' \
             AND m.dir = 's2c' AND m.status = 'ac2' AND m.mtype = 65638",
        )
        .expect("the query prepares");
    let mut held = 0usize;
    let mut wrong: Vec<String> = Vec::new();
    let raws = st
        .query_map([], |r| r.get::<_, Vec<u8>>(0))
        .expect("the query runs");
    for raw in raws.flatten() {
        let Ok(ServerMessage::PlayerCall(c)) = ServerMessage::decode(&raw) else {
            continue;
        };
        if c.function.0 != SESSION_START && c.function.0 != UPDATE_QUEST {
            continue;
        }
        let Some(args) = c.args.as_ref().and_then(|a| decode(&a.0, lib).ok()) else {
            continue;
        };
        let mut records: Vec<QuestInfo> = Vec::new();
        for a in &args {
            let Arg::Object(g) = a else { continue };
            for e in &g.entries {
                if let Body::Object(PackedObject {
                    native: Some(n), ..
                }) = &e.body
                {
                    match n {
                        NativeObject::QuestList(l) => records.extend(l.iter().cloned()),
                        NativeObject::Quest(q) => records.push((**q).clone()),
                        _ => {}
                    }
                }
            }
        }
        for q in records.iter().filter(|q| q.status == status::UNDERWAY) {
            let Some(def) = t.quests.quest(q.id) else {
                continue;
            };
            held += 1;
            let timed = def.duration > 0.0 && (def.timed_phase == 0 || def.timed_phase == q.phase);
            if timed != (q.expires != 0) {
                wrong.push(format!(
                    "{:08x} at phase {}: time {} recorded, {timed} here",
                    q.id, q.phase, q.expires
                ));
            }
        }
    }
    assert!(held >= 2000, "records under way: {held}");
    wrong.sort();
    wrong.dedup();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// **An Ulkas mine**: a guard's death gives a player who has paid its hidden quest and with it the
/// mine's, and one who has not nothing; each gem counts, and the last ends the quest and the toll,
/// so the next guard gives no more until another fee.
/// Behaviour: server.gives.ulkas-mines
#[test]
fn an_ulkas_mine_is_given_by_a_guard_after_its_toll_and_its_last_gem_ends_the_toll() {
    use yalain_content::quests::{Trigger, Update};
    use yalain_world::quests::{killed, moves, status, Move, QuestEffect};
    let t = &compiled().tables;
    for (toll, guard_quest, mine, guard, grant, gems, effect) in [
        (
            0x4000_0183,
            0x4000_0184,
            0x4000_0185,
            0x4700_2B5B,
            0x6F00_1DAA,
            8,
            "Eff_BestowQuestBossUlkasOsteth",
        ),
        (
            0x4000_0186,
            0x4000_0187,
            0x4000_0188,
            0x4700_2B59,
            0x6F00_1DA9,
            12,
            "Eff_BestowQuestBossUlkasOmishan",
        ),
        (
            0x4000_0189,
            0x4000_018A,
            0x4000_018B,
            0x4700_2CF3,
            0x6F00_1DA8,
            13,
            "Eff_BestowQuestBossUlkasLinvak",
        ),
    ] {
        let def = t.quests.quest(guard_quest).expect("the guard's quest");
        assert!(def.hidden, "{guard_quest:08X}");
        assert_eq!(def.required, Some((toll, status::UNDERWAY)));
        let step = t
            .quests
            .steps
            .iter()
            .find(|s| s.trigger == Trigger::Given(guard_quest))
            .unwrap_or_else(|| panic!("{guard_quest:08X}"));
        assert_eq!(
            (step.quest, step.needs, step.update),
            (mine, 0, Update::Bestow)
        );
        assert_eq!(step.effects, [grant]);
        assert!(step.effects_last, "the update told before the recipe");
        let (mut h, _, _) = served(t);
        set_level(&mut h, 60);
        let world = *h.server.players.keys().next().expect("a player");
        h.server.quest_ids.insert(
            effect.trim_start_matches("Eff_Bestow").to_string(),
            guard_quest,
        );
        let kill = |h: &mut yalain_testkit::Harness| {
            let clock = h.clock();
            killed(
                &mut h.server,
                &mut h.net,
                world,
                (0, guard, None),
                &[effect.to_string()],
                clock,
            );
            h.run(2);
        };
        kill(&mut h);
        assert_eq!(quest_state(&h, mine), None, "no toll, no mine");
        quest_at(&mut h, toll, 1);
        kill(&mut h);
        assert_eq!(quest_state(&h, mine), Some((status::UNDERWAY, 1)));
        for n in 1..=gems {
            let clock = h.clock();
            let e = QuestEffect {
                quest: mine,
                act: Move::Increment,
            };
            moves(&mut h.server, &mut h.net, world, e, true, clock);
            h.run(2);
            if n < gems {
                assert_eq!(quest_state(&h, mine), Some((status::UNDERWAY, n + 1)));
            }
        }
        assert_eq!(quest_state(&h, mine).map(|q| q.0), Some(status::COMPLETED));
        assert_eq!(quest_state(&h, toll).map(|q| q.0), Some(status::COMPLETED));
        kill(&mut h);
        assert_eq!(
            quest_state(&h, mine).map(|q| q.0),
            Some(status::COMPLETED),
            "the toll is spent"
        );
    }
    // The way down from the sapphire mines is used after the last gem, so it asks nothing.
    let down = t
        .content
        .objects
        .iter()
        .find(|o| o.object == Some(0x40BA_8700_0000_0014))
        .expect("the way down");
    assert!(down.gate.is_none());
}

/// **Ryvenna's gems**: 33 in the order characters did them: each given at the use after the one
/// before it is done, none given twice, any size taken and paid twice what that size is worth (the
/// one fee recorded, a plain jade's 900, is that), her next gem named, and her tool for the last.
/// Behaviour: server.gives.ryvenna-gems
#[test]
fn ryvennas_gems_follow_one_another_in_the_order_characters_did_them() {
    use yalain_content::quests::{Need, Trigger, Update};
    use yalain_world::quests::status;
    const RYVENNA: u32 = 0x4700_2FD7;
    // First, the jade recorded and the one recorded after it, last.
    const QUARTZ: u32 = 0x4000_02F7;
    const OBSIDIAN: u32 = 0x4000_02F3;
    const JADE: u32 = 0x4000_02EE;
    const GARNET: u32 = 0x4000_02ED;
    const STARSTONE: u32 = 0x4000_02FD;
    const OPAL: u32 = 0x4000_02F4;
    const COIN: u32 = 0x4700_0538;
    const CUTTER: u32 = 0x4700_14C0;
    const PLAIN_JADE: u32 = 0x4700_1045;
    let t = &compiled().tables;
    let portal = yalain_testkit::authored::Portal::open();
    let givings: Vec<_> = t
        .quests
        .steps
        .iter()
        .filter(|s| s.trigger == Trigger::Use(RYVENNA) && s.update == Update::Bestow)
        .collect();
    assert_eq!(givings.len(), 33);
    assert!(givings.iter().all(|s| s.needs == 0 && s.tell.is_some()));
    assert!(!givings.iter().any(|s| s.quest == OPAL), "nobody did it");
    for g in &givings {
        assert!(
            t.quests.quest(g.quest).is_some_and(|d| d.once),
            "{:08X} is done once",
            g.quest
        );
    }
    let after = |q: u32| {
        givings
            .iter()
            .find(|s| s.requires == [(q, Need::Done)])
            .map(|s| s.quest)
    };
    assert_eq!(after(QUARTZ), Some(OBSIDIAN));
    assert_eq!(after(JADE), Some(GARNET));
    assert_eq!(after(STARSTONE), None);
    let mut q = Some(QUARTZ);
    let mut n = 0;
    while let Some(at) = q {
        n += 1;
        q = after(at);
    }
    assert_eq!(n, 33, "one chain");
    for g in &givings {
        let taken: Vec<_> = t
            .quests
            .steps
            .iter()
            .filter(|s| {
                s.quest == g.quest && s.needs == 1 && matches!(s.trigger, Trigger::Give(_, RYVENNA))
            })
            .collect();
        assert_eq!(taken.len(), 4, "{:08X}: every size", g.quest);
        for s in taken {
            let Trigger::Give(gem, _) = s.trigger else {
                unreachable!("kept for a give")
            };
            let value = portal
                .template(gem)
                .props
                .int("Item_Value")
                .expect("a gem's value");
            assert_eq!(s.gives.first(), Some(&COIN), "{gem:08X}");
            let fee = u32::try_from(value * 2).expect("a fee");
            assert_eq!(s.counts.first().copied(), Some(fee), "{gem:08X}");
        }
    }
    let recorded = t
        .quests
        .steps
        .iter()
        .find(|s| matches!(s.trigger, Trigger::Give(PLAIN_JADE, RYVENNA)))
        .expect("the jade recorded");
    assert_eq!(recorded.counts, [900], "the fee recorded");
    let last: Vec<_> = t
        .quests
        .steps
        .iter()
        .filter(|s| s.quest == STARSTONE && matches!(s.trigger, Trigger::Give(_, RYVENNA)))
        .collect();
    assert_eq!(last.len(), 4);
    assert!(last.iter().all(|s| s.gives.contains(&CUTTER)), "her tool");

    let (mut h, b, me) = served(t);
    set_level(&mut h, 60);
    let o = placed(t, RYVENNA);
    use_person(&mut h, b, me, t, o);
    assert_eq!(quest_state(&h, QUARTZ), Some((status::UNDERWAY, 1)));
    use_person(&mut h, b, me, t, o);
    assert_eq!(quest_state(&h, OBSIDIAN), None, "one at a time");
    let huge = t
        .quests
        .steps
        .iter()
        .find_map(|s| match s.trigger {
            Trigger::Give(i, RYVENNA) if s.quest == QUARTZ => Some(i),
            _ => None,
        })
        .expect("a stone");
    hand_in(&mut h, &portal, huge, RYVENNA);
    assert_eq!(
        quest_state(&h, QUARTZ).map(|q| q.0),
        Some(status::COMPLETED)
    );
    use_person(&mut h, b, me, t, o);
    assert_eq!(quest_state(&h, OBSIDIAN), Some((status::UNDERWAY, 1)));
    assert_eq!(
        quest_state(&h, QUARTZ).map(|q| q.0),
        Some(status::COMPLETED),
        "not given twice"
    );
}

/// **Journal hand-ins**: what each person's line was waiting for now runs: the item given at the
/// journal's last phase finishes its quest, and the line tied to it is said.
/// Behaviour: server.gives.journal-hand-ins
#[test]
fn hand_ins_a_journal_names_finish_their_quests() {
    use yalain_world::quests::status;
    let t = &compiled().tables;
    let portal = yalain_testkit::authored::Portal::open();
    for (quest, phase, item, to) in [
        (0x8000_030F_u32, 1, 0x4700_1CCF_u32, 0x4700_32BD_u32),
        (0x8000_0300, 1, 0x4700_0CB3, 0x4700_32D0),
        (0x8000_0301, 1, 0x4700_1900, 0x4700_32D0),
        (0x8000_0307, 1, 0x4700_1906, 0x4700_32D3),
        (0x8000_0308, 1, 0x4700_1905, 0x4700_32D3),
        (0x8000_02E0, 2, 0x4700_1909, 0x4700_32D7),
        (0x8000_0349, 2, 0x4700_13FA, 0x4700_2FBB),
        (0x8000_0282, 5, 0x4700_1679, 0x4700_3173),
        (0x8000_00DF, 2, 0x4700_0912, 0x4700_3319),
        (0x8000_00E0, 2, 0x4700_0917, 0x4700_331B),
        (0x8000_00E1, 2, 0x4700_091C, 0x4700_331D),
        (0x8000_00E2, 2, 0x4700_0921, 0x4700_331F),
        (0x8000_00E3, 2, 0x4700_0926, 0x4700_3321),
    ] {
        let (mut h, _, _) = served(t);
        quest_at(&mut h, quest, phase);
        // A crafter's pair of errands: each is taken while the other is under way.
        let other = match quest {
            0x8000_0300 => Some(0x8000_0301),
            0x8000_0301 => Some(0x8000_0300),
            0x8000_0307 => Some(0x8000_0308),
            0x8000_0308 => Some(0x8000_0307),
            _ => None,
        };
        if let Some(other) = other {
            quest_at(&mut h, other, 1);
        }
        hand_in(&mut h, &portal, item, to);
        assert_eq!(
            quest_state(&h, quest).map(|q| q.0),
            Some(status::COMPLETED),
            "{quest:08X}"
        );
    }
    // Five horns are what Gormery wants; each carenzi takes the nut.
    for carenzi in [
        0x4700_31B9,
        0x4700_31BB,
        0x4700_31BD,
        0x4700_31BF,
        0x4700_31C1,
    ] {
        let (mut h, _, _) = served(t);
        quest_at(&mut h, 0x8000_038C, 4);
        hand_in(&mut h, &portal, 0x4700_172B, carenzi);
        assert_eq!(
            quest_state(&h, 0x8000_038C),
            Some((status::UNDERWAY, 5)),
            "{carenzi:08X}"
        );
    }
    let horns = t
        .quests
        .steps
        .iter()
        .find(|s| s.quest == 0x8000_030F && s.needs == 1)
        .expect("the horns");
    assert_eq!(horns.wants, Some(5));
    let coins = t
        .quests
        .steps
        .iter()
        .find(|s| s.quest == 0x8000_02E8 && s.needs == 0)
        .expect("Kleptor's fee");
    assert_eq!(coins.wants, Some(25));
}

/// A mine's recipe reads as the data defines it: the gems it may give, and the mine it must be made at.
/// Behaviour: server.craft.recipe-charges-and-targets-read
#[test]
fn a_mines_recipe_reads_its_charges_and_its_mine() {
    let p = yalain_testkit::authored::Portal::open();
    let c = p.crafting(&[0x6F00_06A0], &[0x6F00_02D2, 0x6F00_02D3, 0x6F00_02D4]);
    for (r, charges, mine) in [
        (0x6F00_02D2, 13, 0x4700_0B3B),
        (0x6F00_02D3, 12, 0x4700_0B3C),
        (0x6F00_02D4, 8, 0x4700_0B3D),
    ] {
        let d = &c.recipes[&r];
        assert_eq!((d.charges, d.targets.as_slice()), (charges, &[mine][..]));
    }
}

/// A template with the properties the install gives it.
fn as_installed(portal: &yalain_testkit::authored::Portal, item: u32) -> yalain_content::Template {
    use yalain_world_db::compile::Install;
    let install = dat_install();
    let mut t = portal.template(item);
    t.props = yalain_content::Props::of(item, &|id| install.entity(id), &|id| {
        install.property_name(id)
    });
    t
}

/// Run what a use of a person does, by its trigger.
fn used_person(h: &mut Harness, who: u32) {
    use yalain_content::quests::Trigger;
    let world = player_world(h);
    let clock = h.clock();
    yalain_world::quests::triggered(
        &mut h.server,
        &mut h.net,
        world,
        Trigger::Use(who),
        None,
        clock,
    );
    h.run(2);
}

/// **Clackety's and Click's second halves**: a person the journal names moves the quest on, the last
/// item handed to that person brings the item the crafter wants and the next phase, and the crafter
/// takes it. Crotchet's jug is filled at Cedar's cask, and his coupon is good at Cedar's.
/// Behaviour: server.quests.crafters-second-halves
#[test]
fn a_crafters_second_half_runs_as_its_journal_says() {
    use yalain_content::quests::Trigger;
    use yalain_world::quests::status;
    let t = &compiled().tables;
    let portal = yalain_testkit::authored::Portal::open();
    for (quest, asked, items, wanted, crafter) in [
        (
            0x8000_02FB_u32,
            0x4700_32D0_u32,
            [
                (0x8000_0300_u32, 0x4700_0CB3_u32),
                (0x8000_0301, 0x4700_1900),
            ],
            0x4700_18FF_u32,
            0x4700_32CE_u32,
        ),
        (
            0x8000_0303,
            0x4700_32D3,
            [(0x8000_0307, 0x4700_1906), (0x8000_0308, 0x4700_1905)],
            0x4700_1904,
            0x4700_32D2,
        ),
    ] {
        let (mut h, _, _) = served(t);
        for x in [wanted, 0x4700_1902, 0x4700_1901] {
            h.server.catalog.insert(portal.template(x));
        }
        quest_at(&mut h, quest, 2);
        used_person(&mut h, asked);
        assert_eq!(quest_state(&h, quest), Some((status::UNDERWAY, 3)));
        hand_in(&mut h, &portal, items[0].1, asked);
        assert_eq!(
            quest_state(&h, items[0].0).map(|q| q.0),
            Some(status::COMPLETED)
        );
        assert_eq!(
            quest_state(&h, quest),
            Some((status::UNDERWAY, 3)),
            "one to go"
        );
        let holds_wanted = |h: &Harness| {
            h.server.players[&player_world(h)]
                .play
                .items
                .iter()
                .any(|i| i.template == wanted)
        };
        assert!(!holds_wanted(&h));
        hand_in(&mut h, &portal, items[1].1, asked);
        assert_eq!(quest_state(&h, quest), Some((status::UNDERWAY, 4)));
        assert!(holds_wanted(&h), "the last brings it");
        hand_in(&mut h, &portal, wanted, crafter);
        assert_eq!(quest_state(&h, quest).map(|q| q.0), Some(status::COMPLETED));
    }
    let filled = t
        .quests
        .steps
        .iter()
        .find(|s| s.trigger == Trigger::UseOn(0x4700_1902, 0x4700_1903))
        .expect("the jug at the cask");
    assert_eq!(filled.gives, [0x4700_1905]);
    let coupon = t
        .quests
        .gives
        .iter()
        .find(|g| g.item == Some(0x4700_1901) && g.to == 0x4700_33FB)
        .expect("the coupon at Cedar's");
    assert_eq!(coupon.gives, [0x4700_1906]);
}

/// **Seskri's talismans**: each talisman's own effect moves her quest on from the phase before; she
/// names what is left at each phase, and takes the last.
/// Behaviour: server.quests.seskris-talismans
#[test]
fn seskris_quest_moves_by_its_talismans_and_ends_with_the_last() {
    use yalain_world::quests::status;
    let t = &compiled().tables;
    let portal = yalain_testkit::authored::Portal::open();
    const QUEST: u32 = 0x4000_0101;
    let (mut h, b, me) = served(t);
    h.server.quest_ids = quest_ids();
    let world = player_world(&h);
    for (item, from, to) in [
        (0x4700_18A4_u32, 1, 2),
        (0x4700_18A6, 2, 3),
        (0x4700_18AC, 3, 4),
        (0x4700_18AE, 4, 5),
        (0x4700_18A8, 5, 6),
    ] {
        h.server.catalog.insert(as_installed(&portal, item));
        quest_at(&mut h, QUEST, from);
        let clock = h.clock();
        yalain_world::quests::took(&mut h.server, &mut h.net, world, item, clock);
        assert_eq!(
            quest_state(&h, QUEST),
            Some((status::UNDERWAY, to)),
            "{item:08X}"
        );
    }
    let o = placed(t, 0x4700_3293);
    for (phase, line) in [(1, 0xF2AE_9DAA_u32), (3, 0xF2BE_9DAA), (6, 0xF296_9DAA)] {
        quest_at(&mut h, QUEST, phase);
        let said = use_person(&mut h, b, me, t, o);
        assert!(
            said.iter().any(|(_, s)| *s == Some(line)),
            "{phase} {said:08X?}"
        );
    }
    quest_at(&mut h, QUEST, 6);
    hand_in(&mut h, &portal, 0x4700_18A8, 0x4700_3293);
    assert_eq!(quest_state(&h, QUEST).map(|q| q.0), Some(status::COMPLETED));
}

/// **Aua's maintenance**: she gives it, its shards and stibnite move it by their own effects, the
/// alembic takes each, and she takes the infusion.
/// Behaviour: server.quests.aua-gives-the-skyport-maintenance
#[test]
fn aua_gives_her_quest_and_the_alembic_makes_the_infusion() {
    use yalain_content::quests::Trigger;
    use yalain_world::quests::status;
    let t = &compiled().tables;
    let portal = yalain_testkit::authored::Portal::open();
    const QUEST: u32 = 0x8000_0282;
    const ALEMBIC: u32 = 0x4700_3171;
    let (mut h, _, _) = served(t);
    h.server.quest_ids = quest_ids();
    let world = player_world(&h);
    used_person(&mut h, 0x4700_3173);
    assert_eq!(quest_state(&h, QUEST), Some((status::UNDERWAY, 1)));
    for (item, to) in [(0x4700_1677_u32, 2), (0x4700_167A, 3)] {
        h.server.catalog.insert(as_installed(&portal, item));
        let clock = h.clock();
        yalain_world::quests::took(&mut h.server, &mut h.net, world, item, clock);
        assert_eq!(
            quest_state(&h, QUEST),
            Some((status::UNDERWAY, to)),
            "{item:08X}"
        );
    }
    let def = t.quests.quest(QUEST).expect("the quest");
    assert_eq!(def.retry, 172_800.0);
    for (item, before, after) in [(0x4700_1677_u32, 3, 4), (0x4700_167A, 4, 5)] {
        let step = t
            .quests
            .steps
            .iter()
            .find(|s| s.trigger == Trigger::UseOn(item, ALEMBIC))
            .unwrap_or_else(|| panic!("{item:08X}"));
        assert_eq!(step.needs, before);
        assert_eq!(step.update, yalain_content::quests::Update::Advance(after));
    }
    quest_at(&mut h, QUEST, 5);
    hand_in(&mut h, &portal, 0x4700_1679, 0x4700_3173);
    assert_eq!(quest_state(&h, QUEST).map(|q| q.0), Some(status::COMPLETED));
}

/// **The Ulkas mines' other portals**: each way down leads to the mine's base, each second way in
/// where the first does, asking for the same toll.
/// Behaviour: server.quests.mine-ways-down
#[test]
fn every_ulkas_mine_portal_leads_somewhere() {
    let t = &compiled().tables;
    let of = |id: u64| {
        t.content
            .objects
            .iter()
            .find(|o| o.object == Some(id))
            .unwrap_or_else(|| panic!("{id:016X}"))
    };
    for id in [
        0x407A_9500_0000_0001_u64,
        0x4057_6000_0000_0001,
        0x4056_5E00_0000_0010,
    ] {
        assert!(of(id).destination.is_some(), "{id:016X}");
        assert!(of(id).gate.is_none(), "{id:016X} is a way out");
    }
    assert_eq!(
        of(0x4057_6000_0000_0001).destination,
        of(0x4056_5E00_0000_0010).destination
    );
    for (second, first) in [
        (0x4079_9400_0000_0004_u64, 0x4079_9400_0000_0003_u64),
        (0x4056_6000_0000_0003, 0x4056_6000_0000_0004),
    ] {
        assert_eq!(
            of(second).destination,
            of(first).destination,
            "{second:016X}"
        );
        assert_eq!(
            of(second).gate.map(|g| g.quest),
            of(first).gate.map(|g| g.quest),
            "{second:016X}"
        );
    }
}

/// **The Rouge mirra** goes as the others: cleansed at the font, charged at Exelaris, handed to the
/// golem.
/// Behaviour: server.quests.the-rouge-mirra
#[test]
fn the_rouge_mirra_goes_as_the_other_mirras() {
    use yalain_world::quests::status;
    let t = &compiled().tables;
    let portal = yalain_testkit::authored::Portal::open();
    const QUEST: u32 = 0x4000_02A2;
    let (mut h, _, _) = served(t);
    h.server.catalog.insert(portal.template(0x4700_1750));
    hand_in(&mut h, &portal, 0x4700_174B, 0x4700_2FFD);
    assert_eq!(
        quest_state(&h, QUEST),
        Some((status::UNDERWAY, 2)),
        "given, and moved by what it gives"
    );
    let charge = t
        .quests
        .gives
        .iter()
        .find(|g| g.item == Some(0x4700_1750) && g.to == 0x4700_31C3)
        .expect("charged at Exelaris");
    assert_eq!(charge.gives, [0x4700_174E]);
    hand_in(&mut h, &portal, 0x4700_174E, 0x4700_3331);
    assert_eq!(quest_state(&h, QUEST), Some((status::UNDERWAY, 3)));
    hand_in(&mut h, &portal, 0x4700_076A, 0x4700_3331);
    assert_eq!(quest_state(&h, QUEST).map(|q| q.0), Some(status::COMPLETED));
}

/// Every summoning skill a player can train names a pet the world's rows make, every row's template
/// is a creature in the install at a summon's level, and every limit is of a skill that summons.
/// Behaviour: server.pets.every-summoning-skill-makes-its-pet
#[test]
fn every_summoning_skill_names_a_pet_the_worlds_rows_make() {
    use yalain_world::effect_defs::Kind;
    let t = &compiled().tables;
    let portal = yalain_testkit::authored::Portal::open();
    let table = portal.skills();
    let named: Vec<u32> = table
        .skills
        .values()
        .filter_map(|d| d.usage.as_ref())
        .flat_map(|u| u.user_effects.iter().chain(&u.target_effects).copied())
        .collect();
    let effects = portal.effects(&named);
    let summons = |d: &yalain_world::skills::SkillDef| {
        d.usage.as_ref().is_some_and(|u| {
            u.user_effects
                .iter()
                .chain(&u.target_effects)
                .any(|e| matches!(effects.get(*e).map(|x| &x.kind), Some(Kind::Pet { .. })))
        })
    };
    let mut skills = 0;
    for (id, d) in table.skills.iter().filter(|(_, d)| d.flags & 1 != 0) {
        let Some(u) = d.usage.as_ref() else { continue };
        for e in u.user_effects.iter().chain(&u.target_effects) {
            if matches!(effects.get(*e).map(|x| &x.kind), Some(Kind::Pet { .. })) {
                skills += 1;
                assert!(t.pet_kinds.contains_key(e), "skill {id}: effect {e:08X}");
            }
        }
    }
    assert!(skills >= 50, "{skills}");
    for k in t.pet_kinds.values() {
        let mut props = portal.template(k.template).props;
        props.set(
            yalain_content::props::creature_property::LEVEL,
            yalain_content::Prop::Int(20),
        );
        assert!(
            yalain_content::CreatureStats::from_props(&props).is_some(),
            "{:08X}",
            k.template
        );
    }
    for skill in t.pet_limits.keys() {
        assert!(table.skills.get(skill).is_some_and(summons), "{skill}");
    }
}

/// The world's armor for the kinds of pet 2017 recorded struck puts a creature's plain hit on each
/// where 2017's were: by kind and level, 2017's mean over ours within a fifth either way, our hits
/// drawn by the rule for the creatures' natural damage 2017 recorded. A world whose pets have no
/// armor rows keeps the share by level and is held to nothing here. Each entry: the pet's template,
/// its level, and (natural, variance, 2017's mean hit, hits) by band of natural damage.
/// Behaviour: server.pets.armor-rows-as-2017
#[test]
fn creatures_hits_on_pets_take_what_2017_recorded_by_kind_and_level() {
    type Band = (f32, f32, f32, u32);
    const RECORDED: &[(u32, i32, &[Band])] = &[
        (
            0x4700_0846,
            29,
            &[(83.0, 0.31, 51.0, 37), (113.0, 0.30, 74.0, 48)],
        ),
        (
            0x4700_0846,
            52,
            &[
                (264.0, 0.12, 50.0, 232),
                (386.0, 0.17, 106.0, 245),
                (455.0, 0.19, 160.0, 172),
                (521.0, 0.26, 204.0, 54),
            ],
        ),
        (
            0x4700_0846,
            53,
            &[
                (260.0, 0.12, 27.0, 127),
                (381.0, 0.16, 93.0, 483),
                (443.0, 0.16, 150.0, 251),
                (508.0, 0.25, 191.0, 156),
            ],
        ),
        (
            0x4700_0846,
            54,
            &[
                (156.0, 0.24, 46.0, 30),
                (224.0, 0.19, 52.0, 174),
                (359.0, 0.15, 89.0, 174),
                (442.0, 0.14, 163.0, 110),
                (530.0, 0.17, 224.0, 75),
            ],
        ),
        (
            0x4700_084A,
            56,
            &[(253.0, 0.21, 31.0, 177), (335.0, 0.12, 92.0, 34)],
        ),
        (
            0x4700_084A,
            59,
            &[(270.0, 0.12, 29.0, 34), (370.0, 0.10, 114.0, 25)],
        ),
        (
            0x4700_084A,
            72,
            &[
                (284.0, 0.17, 28.0, 93),
                (321.0, 0.12, 37.0, 77),
                (445.0, 0.12, 113.0, 27),
                (540.0, 0.10, 209.0, 26),
            ],
        ),
        (0x4700_084B, 55, &[(256.0, 0.20, 128.0, 74)]),
        (
            0x4700_084B,
            72,
            &[(290.0, 0.17, 90.0, 20), (361.0, 0.12, 160.0, 21)],
        ),
        (
            0x4700_0851,
            58,
            &[
                (291.0, 0.10, 30.0, 23),
                (384.0, 0.11, 106.0, 37),
                (414.0, 0.17, 120.0, 53),
            ],
        ),
        (
            0x4700_0863,
            58,
            &[
                (75.0, 0.35, 8.0, 41),
                (159.0, 0.21, 58.0, 96),
                (249.0, 0.20, 131.0, 186),
                (335.0, 0.19, 218.0, 53),
            ],
        ),
        (
            0x4700_0863,
            60,
            &[(178.0, 0.25, 55.0, 45), (240.0, 0.22, 118.0, 62)],
        ),
        (
            0x4700_0863,
            76,
            &[(362.0, 0.18, 171.0, 50), (477.0, 0.21, 271.0, 24)],
        ),
    ];
    const DRAWS: u32 = 400;
    let t = &compiled().tables;
    if t.pet_kinds.values().all(|k| k.armor.is_empty()) {
        return;
    }
    let mut rng = yalain_world::rng::Rng::new(7);
    for (template, level, bands) in RECORDED {
        let kinds: Vec<_> = t
            .pet_kinds
            .values()
            .filter(|k| k.template == *template)
            .collect();
        assert!(!kinds.is_empty(), "{template:08X}");
        for k in kinds {
            let armor = k.armor_at(*level);
            assert!(armor.is_some(), "{:08X} at {level}: no armor", k.effect);
            #[allow(clippy::cast_precision_loss)]
            let armor = armor.unwrap_or(0) as f32;
            let (mut recorded, mut ours) = (0.0_f64, 0.0_f64);
            for &(natural, variance, mean, hits) in *bands {
                let n = yalain_world::rules::Natural {
                    damage: natural,
                    variance,
                    added: 0.0,
                    factor: 1.0,
                };
                let sum: i64 = (0..DRAWS)
                    .map(|_| {
                        i64::from(yalain_world::rules::creature_hit_on_armor(
                            &mut rng, n, armor, false,
                        ))
                    })
                    .sum();
                #[allow(clippy::cast_precision_loss)]
                let our_mean = sum as f64 / f64::from(DRAWS);
                recorded += f64::from(mean) * f64::from(hits);
                ours += our_mean * f64::from(hits);
            }
            let r = recorded / ours;
            assert!(
                (0.8..1.25).contains(&r),
                "{:08X} at {level}, armor {armor}: 2017 over ours {r:.2}",
                k.effect
            );
        }
    }
}

/// The Ultra Chest, the one chest standing whose kind names a use effect, names the completion of the
/// quest that keeps it, by a name the server reads; no row of the world gives that quest.
/// Behaviour: server.places.a-chests-use-comes-on-its-opener
#[test]
fn the_ultra_chests_effect_completes_the_quest_that_keeps_it() {
    use yalain_world::quests::{effect_named, Move};
    const ULTRA_CHEST: u32 = 0x4700_0F39;
    let portal = yalain_testkit::authored::Portal::open();
    let t = portal.template(ULTRA_CHEST);
    let ids = quest_ids();
    let quest = t
        .props
        .word("Usage_RequiredQuest")
        .expect("kept by a quest");
    let name = t.props.text("Usage_Effect1").expect("names an effect");
    let e = effect_named(name, |q| ids.get(q).copied()).expect("a quest's");
    assert_eq!((e.quest, e.act), (quest, Move::Complete));
    assert_eq!(quest, ids["QuestSHRETHBaseUltraChest"]);
    let tables = &compiled().tables;
    assert!(
        !tables.quests.steps.iter().any(|s| s.quest == quest),
        "nothing gives it yet"
    );
}

/// Each practice gem's dummy stands as long as its gem says and its quest runs, and on the real path
/// a gem used at its place makes it, 2 m ahead, for that long; used elsewhere, it's refused and kept.
/// Behaviour: server.creatures.a-gems-dummy-lasts-its-time
#[test]
fn a_practice_gem_makes_its_dummy_at_its_place_for_its_ninety_seconds() {
    const GEM: u32 = 0x4700_1853;
    const DUMMY: u32 = 0x4700_2DB0;
    let t = &compiled().tables;
    let ids = quest_ids();
    for tier in [30, 40, 50, 60, 70] {
        let row = t
            .death_spawns
            .get(&format!("Eff_SummonDeadlyDummy{tier}"))
            .expect("a row");
        let quest = t
            .quests
            .quest(ids[&format!("Quest_DeadlyDummy{tier}")])
            .expect("its quest");
        assert_eq!(row.lasts.map(f64::from), Some(quest.duration), "{tier}");
    }
    let portal = yalain_testkit::authored::Portal::open();
    let (mut h, b, me) = served(t);
    // Above the least level its quest is given at.
    set_level(&mut h, 30);
    h.server.catalog.insert(portal.template(GEM));
    h.server.catalog.insert(portal.template(DUMMY));
    h.server.item_uses = portal.item_uses(&[GEM]);
    h.server.death_spawns = t
        .death_spawns
        .iter()
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    let quest = ids["Quest_DeadlyDummy30"];
    h.server.quest_ids = ids;
    let gem_template = h.server.catalog.get(GEM).expect("the gem");
    assert!(
        yalain_world::item_use::spends(&h.server.item_rows, GEM, &gem_template.props),
        "its kind is used up"
    );
    h.run(4);
    let world = *h.server.players.keys().next().expect("in");
    let gem =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, GEM, 1).expect("given");
    h.run(2);
    let dummies = |h: &Harness| -> Vec<u64> {
        h.server
            .things
            .by_id
            .values()
            .filter(|t| t.template == DUMMY)
            .map(|t| t.id)
            .collect()
    };
    let use_gem = |h: &mut Harness| {
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(0x8216_0016, &[use_of(gem, me)]),
        );
        h.run(10);
    };
    use_gem(&mut h);
    assert!(dummies(&h).is_empty(), "not at its place");
    assert!(h.server.things.get(gem).is_some(), "kept");
    let at = {
        let p = h.server.players.get_mut(&world).expect("in");
        p.position.cell = 0xC6A2_001A;
        p.position.origin = [62.0, 40.0, 135.0];
        p.position
    };
    use_gem(&mut h);
    h.run(100);
    let made = dummies(&h);
    assert_eq!(made.len(), 1, "at its place");
    assert!(h.server.things.get(gem).is_none(), "spent");
    let p = &h.server.players[&world];
    assert_eq!(
        yalain_world::quests::phase_of(&p.play.quests, quest),
        1,
        "its quest given"
    );
    let there = h
        .server
        .things
        .get(made[0])
        .and_then(|t| t.at)
        .expect("stands");
    let (dx, dy) = (
        there.origin[0] - at.origin[0],
        there.origin[1] - at.origin[1],
    );
    assert!(((dx * dx + dy * dy) - 4.0).abs() < 0.05, "{there:?}");
    h.advance(80.0);
    h.run(2);
    assert_eq!(dummies(&h), made, "still there");
    h.advance(11.0);
    h.run(2);
    assert!(dummies(&h).is_empty(), "gone");
}

/// What the best transmutation perk pays for what a store sells is no more than its price, so no
/// buying and selling back makes money. Every store of the world database, each sale in coins.
/// Behaviour: server.inventory.money-bag-pays-a-perk
#[test]
fn no_store_sale_pays_back_more_than_it_cost_at_the_best_transmutation_perk() {
    let t = &compiled().tables;
    let portal = yalain_testkit::authored::Portal::open();
    let mut checked = 0;
    let mut over = Vec::new();
    for sale in t
        .stores
        .sales
        .iter()
        .filter(|s| s.currency == yalain_world::pack::COINS)
    {
        let Some(value) = portal.template(sale.product).props.int("Item_Value") else {
            continue;
        };
        checked += 1;
        let back = (f64::from(i32::try_from(value).unwrap_or(0)) * 1.18).floor();
        if back > f64::from(sale.cost) {
            over.push((sale.store, sale.product, sale.cost, value));
        }
    }
    assert!(checked > 100, "{checked}");
    assert!(over.is_empty(), "{} pay back more: {over:x?}", over.len());
}

/// Every template a world's rows make reads from the install, so a real start can describe it, and
/// a row a death or a use makes by is a creature.
/// Behaviour: server.makes.the-rows-name-what-they-make
#[test]
fn everything_the_worlds_rows_make_reads_from_the_install() {
    let t = &compiled().tables;
    let portal = yalain_testkit::authored::Portal::open();
    let items = item_rows();
    let named = yalain_world::makes::by_the_rows(yalain_world::makes::Rows {
        objects: &t.content.objects,
        stores: &t.stores,
        quests: &t.quests,
        crafting: &t.crafting,
        death_spawns: &t.death_spawns,
        pets: &t.pet_kinds,
        items: &items,
    });
    for template in named {
        assert_eq!(portal.template(template).id, template);
    }
    for (name, spawn) in &t.death_spawns {
        let mut props = portal.template(spawn.template).props;
        props.set(
            yalain_content::props::creature_property::LEVEL,
            yalain_content::Prop::Int(20),
        );
        assert!(
            yalain_content::CreatureStats::from_props(&props).is_some(),
            "{name}"
        );
    }
}

/// A corpse 2017 recorded opened: the dead creature's template, and what the corpse held at its
/// first opening, each item's template and value, and each item's object id.
pub(super) struct OpenedCorpse {
    pub(super) creature: u32,
    pub(super) items: Vec<(u32, u32)>,
    pub(super) ids: Vec<u64>,
}

/// Every corpse the January 2017 recordings show opened, once (its first opening), with the
/// creature it was of.
pub(super) fn opened_corpses(lib: &Library) -> Vec<OpenedCorpse> {
    use auberean_script::args::{decode, Arg};
    use auberean_script::native_object::Item;
    use std::collections::{BTreeMap, HashMap, HashSet};
    use yalain_world::inventory::function::OPEN_DONE;
    const CORPSE: u32 = 0x4700_0539;
    let con = rusqlite::Connection::open_with_flags(
        auberean_testkit::data::require_capture_index(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("the capture index opens");
    let mut st = con
        .prepare(
            "SELECT m.session_id, m.raw FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) WHERE c.file LIKE '2017-%' AND m.status = 'ac2' \
             AND m.dir = 's2c' AND m.mtype IN (65624, 65638, 65738) ORDER BY m.session_id, m.idx",
        )
        .expect("prepares");
    let mut rows = st.query([]).expect("runs");
    // One session's objects (template, value), corpses (their creature) and openings.
    #[derive(Default)]
    struct Session {
        objects: HashMap<u64, (u32, u32)>,
        corpses: HashMap<u64, u64>,
        opened: Vec<(u64, Vec<u64>)>,
    }
    let mut out: BTreeMap<u64, OpenedCorpse> = BTreeMap::new();
    let mut seen: HashSet<u64> = HashSet::new();
    let mut close = |s: &mut Session| {
        for (corpse, contents) in std::mem::take(&mut s.opened) {
            let Some(creature) = s.corpses.get(&corpse).and_then(|o| s.objects.get(o)) else {
                continue;
            };
            if !seen.insert(corpse) {
                continue;
            }
            let items = contents
                .iter()
                .map(|i| s.objects.get(i).copied().unwrap_or((0, 0)))
                .collect();
            out.insert(
                corpse,
                OpenedCorpse {
                    creature: creature.0,
                    items,
                    ids: contents,
                },
            );
        }
        *s = Session::default();
    };
    let (mut at, mut s) = (-1_i64, Session::default());
    while let Some(r) = rows.next().expect("a row") {
        let sid: i64 = r.get(0).expect("session");
        if sid != at {
            close(&mut s);
            at = sid;
        }
        let raw: Vec<u8> = r.get(1).expect("raw");
        match ServerMessage::decode(&raw) {
            Ok(ServerMessage::CreateObject(o)) => {
                let value = u32::try_from(o.weenie.numbers[1]).unwrap_or(0);
                s.objects
                    .entry(o.id)
                    .or_insert((o.weenie.entity_did, value));
                if o.weenie.entity_did == CORPSE && o.weenie.objects[3] != 0 {
                    s.corpses.entry(o.id).or_insert(o.weenie.objects[3]);
                }
            }
            Ok(ServerMessage::PlayerCall(c) | ServerMessage::PlayerCallAlternate(c))
                if c.function.0 == OPEN_DONE =>
            {
                let bytes = c.args.map(|a| a.0).unwrap_or_default();
                let Ok(args) = decode(&bytes, lib) else {
                    continue;
                };
                let (Some(Arg::Int(0)), Some(Arg::Object(g)), Some(Arg::LongInt(corpse))) =
                    (args.first(), args.get(2), args.get(3))
                else {
                    continue;
                };
                let nav = yalain_world::start::Nav::new(lib, g);
                let contents = g
                    .root()
                    .and_then(|r| nav.items(r).ok())
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|i| match i {
                        Item::Long(v) => Some(v),
                        _ => None,
                    })
                    .collect();
                s.opened.push((*corpse, contents));
            }
            _ => {}
        }
    }
    close(&mut s);
    out.into_values().collect()
}

/// A level band: tens of levels, 70 and up as one.
fn band_of(level: i32) -> usize {
    usize::try_from(level.clamp(0, 70) / 10).unwrap_or(0)
}

const BANDS: usize = 8;

/// A mean, with its 95% interval as a normal approximation of the mean's spread.
#[derive(Default, Clone, Copy)]
struct Tally {
    n: f64,
    sum: f64,
    squares: f64,
}

impl Tally {
    fn add(&mut self, x: f64, weight: f64) {
        self.n += weight;
        self.sum += x * weight;
        self.squares += x * x * weight;
    }

    fn mean(&self) -> f64 {
        self.sum / self.n.max(1e-9)
    }

    fn interval(&self) -> (f64, f64) {
        let m = self.mean();
        let var = (self.squares / self.n - m * m).max(0.0) * self.n / (self.n - 1.0).max(1.0);
        let half = 1.96 * (var / self.n).sqrt();
        (m - half, m + half)
    }
}

/// What a corpse holds by this server's measure: the items beside the creature's trophies, and the
/// value of the treasure among them (neither trophies nor the profile's own extras).
fn measured(items: &[(u32, u32)], trophies: &[u32], extras: &[u32]) -> (f64, f64) {
    let held: Vec<&(u32, u32)> = items
        .iter()
        .filter(|(t, _)| !trophies.contains(t))
        .collect();
    let value: u32 = held
        .iter()
        .filter(|(t, _)| !extras.contains(t))
        .map(|(_, v)| *v)
        .sum();
    #[allow(clippy::cast_precision_loss)]
    (held.len() as f64, f64::from(value))
}

/// The measures the recorded creatures' corpses made again are held inside 2017's interval on, in
/// every band of 20 corpses or more: value only where the world gives treasure values (with none,
/// treasure is worth the server's own by level band, which is printed and not held). The world's
/// creatures by population are printed, inside or out, and not held.
const HELD: &[&str] = &["items", "value"];

/// The world's loot draw, run again for every creature of a corpse the January 2017 recordings show
/// opened, gives items and value a corpse by level band inside 2017's 95% interval of each, where
/// the band is held; the world's creatures, by their camps' populations, are printed beside, with
/// what share of their items would bring each band outside inside. The value is the treasure's own,
/// what the corpse's items carry beside their templates'; the server reads which items keep their
/// template's from the install, as it does here.
/// Behaviour: server.loot.inside-2017s-intervals
#[test]
fn the_loot_draw_holds_items_and_value_a_corpse_inside_2017s_intervals() {
    use std::collections::BTreeMap;
    use yalain_world_db::compile::Install;
    const REPS: usize = 50;
    const WORLD_REPS: usize = 200;
    let t = &compiled().tables;
    let portal = yalain_testkit::authored::Portal::open();
    let lib = portal.library();
    let install = dat_install();
    let mut h = Harness::new();
    h.add_creatures(1);
    h.run(20);
    let mut creature = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.creature.is_some())
        .cloned()
        .expect("a creature");
    h.server.loot = t.loot.clone();
    h.server.rng = yalain_world::rng::Rng::new(716);
    // Every template the world's loot can leave, in the catalog, and the trumps' packages among
    // theirs, as the server reads them from the install.
    let trumps: Vec<u32> = yalain_world::treasure::TRUMPS
        .iter()
        .filter_map(|n| lib.package_id(n))
        .collect();
    let leaves: std::collections::BTreeSet<u32> = t
        .loot
        .profiles
        .iter()
        .flat_map(|p| p.items.iter().map(|(t, _)| *t))
        .chain(t.loot.pools.pools.values().flat_map(|p| {
            p.entries.iter().filter_map(|e| match e.to {
                yalain_content::pools::Target::Template(t) => Some(t),
                yalain_content::pools::Target::Pool(_) => None,
            })
        }))
        .collect();
    for template in leaves {
        let x = portal.template(template);
        if trumps
            .iter()
            .any(|b| lib.derives_from(x.description.package, *b))
        {
            h.server.treasure.trumps.insert(x.description.package);
        }
        h.server.catalog.insert(x);
    }
    assert_eq!(
        trumps.len(),
        yalain_world::treasure::TRUMPS.len(),
        "the install has the trumps"
    );
    let mut stats_of: BTreeMap<u32, Option<yalain_content::CreatureStats>> = BTreeMap::new();
    let mut stats = |template: u32, h: &mut Harness| {
        stats_of
            .entry(template)
            .or_insert_with(|| {
                let s =
                    yalain_content::CreatureStats::from_props(&portal.template(template).props)?;
                let v = s.loot_profile?;
                let name = install.profile_name(v)?;
                h.server.catalog.profiles.insert(v, name);
                Some(s)
            })
            .clone()
    };
    let pooled = |h: &Harness, s: &yalain_content::CreatureStats| {
        s.loot_profile
            .and_then(|v| h.server.catalog.profile_name(v))
            .is_some_and(|n| yalain_world::treasure_pools::draws_from_pools(&h.server, n))
    };
    let extras = |h: &Harness, s: &yalain_content::CreatureStats| -> Vec<u32> {
        s.loot_profile
            .and_then(|v| h.server.catalog.profile_name(v))
            .and_then(|n| h.server.loot.profile(n))
            .map(|p| p.extras.iter().map(|(t, _)| *t).collect())
            .unwrap_or_default()
    };
    let mut roll = |h: &mut Harness, s: &yalain_content::CreatureStats| -> Vec<(u32, u32)> {
        if let Some(c) = creature.creature.as_mut() {
            c.stats = s.clone();
            c.camp = None;
        }
        yalain_world::death::roll_loot(&mut h.server, &creature, 0)
            .into_iter()
            .map(|(t, v)| (t, v.unwrap_or(0)))
            .collect()
    };
    // 2017's corpses and the same creatures' corpses made again: [items, value] by band.
    let mut recorded = [[Tally::default(); 2]; BANDS];
    let mut again = [[Tally::default(); 2]; BANDS];
    let mut again_pooled = [0.0_f64; BANDS];
    let corpses = opened_corpses(&lib);
    let mut used = 0;
    for c in &corpses {
        let Some(s) = stats(c.creature, &mut h) else {
            continue;
        };
        used += 1;
        let band = band_of(s.level);
        let trophies: Vec<u32> = s.trophies.iter().map(|(t, _)| *t).collect();
        let extras = extras(&h, &s);
        let (items, value) = measured(&c.items, &trophies, &extras);
        recorded[band][0].add(items, 1.0);
        recorded[band][1].add(value, 1.0);
        again_pooled[band] += f64::from(u8::from(pooled(&h, &s)));
        for _ in 0..REPS {
            let made = roll(&mut h, &s);
            let (items, value) = measured(&made, &trophies, &extras);
            again[band][0].add(items, 1.0);
            again[band][1].add(value, 1.0);
        }
    }
    // The world's creatures with a loot profile, each by its share of its camps' populations:
    // [all, seed rows, pool rows] by band, of [items, value].
    let mut population: BTreeMap<u32, f64> = BTreeMap::new();
    for camp in &t.spawns.camps {
        let total: u32 = camp.templates.iter().map(|(_, w)| *w).sum();
        for (template, w) in &camp.templates {
            *population.entry(*template).or_default() +=
                f64::from(camp.population) * f64::from(*w) / f64::from(total.max(1));
        }
    }
    let mut world = [[[Tally::default(); 2]; 3]; BANDS];
    for (template, people) in &population {
        let Some(s) = stats(*template, &mut h) else {
            continue;
        };
        let band = band_of(s.level);
        let trophies: Vec<u32> = s.trophies.iter().map(|(t, _)| *t).collect();
        let extras = extras(&h, &s);
        let kind = if pooled(&h, &s) { 2 } else { 1 };
        #[allow(clippy::cast_precision_loss)]
        let weight = people / WORLD_REPS as f64;
        for _ in 0..WORLD_REPS {
            let made = roll(&mut h, &s);
            let (items, value) = measured(&made, &trophies, &extras);
            for k in [0, kind] {
                world[band][k][0].add(items, weight);
                world[band][k][1].add(value, weight);
            }
        }
    }
    println!(
        "{used} of {} recorded corpses of creatures with a loot profile",
        corpses.len()
    );
    let mut outside = Vec::new();
    for (m, measure) in ["items", "value"].iter().enumerate() {
        let held = HELD.contains(measure) && (*measure != "value" || !t.loot.values.is_empty());
        println!(
            "{measure} a corpse by level band | 2017: corpses, mean [95%] | made again{} \
             (pool rows' share) | world creatures by population: all (seed rows / pool rows, \
             pool rows' share)",
            if held { ", held" } else { "" }
        );
        for band in 0..BANDS {
            let r = recorded[band][m];
            if r.n < 1.0 {
                continue;
            }
            let (lo, hi) = r.interval();
            let a = again[band][m].mean();
            let w = world[band];
            let judge = |x: f64| {
                if r.n < 20.0 {
                    "few".to_string()
                } else if (lo..=hi).contains(&x) {
                    "inside".to_string()
                } else {
                    format!("OUTSIDE: x{:.2}-x{:.2} of it inside", lo / x, hi / x)
                }
            };
            // The pool rows' mean that brings the world's band inside, its seed rows as they are.
            let share = w[2][m].n / w[0][m].n.max(1e-9);
            let need = |x: f64| (x - (1.0 - share) * w[1][m].mean()) / share;
            let pools_for = if r.n < 20.0 || (lo..=hi).contains(&w[0][m].mean()) {
                String::new()
            } else if share >= 0.02 && need(hi) >= 0.0 {
                format!("; pool rows at {:.2}-{:.2}", need(lo).max(0.0), need(hi))
            } else {
                "; not by the pool rows alone".to_string()
            };
            println!(
                "{measure} {:>2}s | {:>5} {:>7.2} [{:.2}, {:.2}] | {:>7.2} ({:.2}) {} | {:>7.2} ({:.2} / {:.2}, {:.2}) {}{}",
                band * 10,
                r.n,
                r.mean(),
                lo,
                hi,
                a,
                again_pooled[band] / r.n,
                judge(a),
                w[0][m].mean(),
                w[1][m].mean(),
                w[2][m].mean(),
                share,
                judge(w[0][m].mean()),
                pools_for,
            );
            if held && r.n >= 20.0 && !(lo..=hi).contains(&a) {
                outside.push(format!(
                    "{measure} {}s: {a:.2} not in [{lo:.2}, {hi:.2}]",
                    band * 10
                ));
            }
        }
    }
    assert!(used > 3_000, "the recordings hold the corpses: {used}");
    assert!(outside.is_empty(), "{outside:#?}");
}

/// The world's treasure numbers, words, looks, potions and enchantments, its rows' or the server's
/// own for each kind it gives none of, draw the seeded drops exactly as they were recorded: the world's treasure
/// rows are 2017's.
/// Behaviour: server.loot.treasure-numbers-from-rows
#[test]
fn the_worlds_treasure_numbers_draw_the_seeded_drops_as_recorded() {
    let t = &compiled().tables;
    let (tables, fell_back) = yalain_world::treasure::TreasureTables::with_rows(&t.treasure);
    println!("the world's treasure numbers, the server's own for: {fell_back:?}");
    let p = yalain_testkit::authored::Portal::open();
    yalain_testkit::treasure_drops::seeded_drops(&p, tables).hold_recorded();
}

/// Each practice gem's effect names a row of the world's that makes a creature, and a start asks
/// for it, the tier-60 gem's dummy among them.
/// Behaviour: server.makes.the-rows-name-what-they-make
#[test]
fn every_practice_gem_names_a_row_a_start_describes() {
    let t = &compiled().tables;
    let catalog = yalain_testkit::authored::Portal::open().catalog(t);
    for tier in [30, 40, 50, 60, 70] {
        let row = t
            .death_spawns
            .get(&format!("Eff_SummonDeadlyDummy{tier}"))
            .unwrap_or_else(|| panic!("the row of the tier-{tier} gem"));
        assert!(catalog.get(row.template).is_some(), "{tier}");
    }
    assert!(catalog.get(0x4700_2DAE).is_some(), "the tier-60 dummy");
}

/// The portal to the far continent stands where its recording does, and leads to the one landing
/// recorded; its hub's gateway home leads back beside it.
/// Behaviour: server.world-db.far-continent-portal-leads-in
#[test]
fn the_portal_to_the_far_continent_leads_to_its_recorded_landing() {
    let t = &compiled().tables;
    let of = |id: u64| {
        t.content
            .objects
            .iter()
            .find(|o| o.object == Some(id))
            .unwrap_or_else(|| panic!("{id:016X}"))
    };
    let portal = of(0x40A0_9700_0000_0001);
    assert_eq!(portal.cell, 0xA097_0024);
    let landing = portal.destination.as_ref().expect("it leads somewhere");
    assert_eq!(landing.cell, 0xAB57_002B);
    assert_eq!(landing.origin, [110.0, 56.0, 151.5]);
    let home = of(0x4006_C000_0000_0005);
    assert_eq!(
        home.destination.as_ref().expect("the way home").cell >> 16,
        0xA097
    );
}

/// Each hunt and bounty of the far continent these rows open has its giver standing, a use that
/// gives it, a use that ends it, and a camp keeping alive the creature whose death moves it.
/// Behaviour: server.world-db.far-continent-hunts-can-be-finished
#[test]
fn the_far_continents_hunts_each_have_a_giver_a_return_and_their_creature() {
    use yalain_content::quests::{Trigger, Update};
    let t = &compiled().tables;
    let catalog = yalain_testkit::authored::Portal::open().catalog(t);
    for (quest, giver, creature) in [
        (0x8000_0182_u32, 0x4700_30B1_u32, 0x4700_2725_u32),
        (0x8000_0185, 0x4700_30B7, 0x4700_2986),
        (0x8000_0192, 0x4700_30D0, 0x4700_2D21),
        (0x8000_0195, 0x4700_30D6, 0x4700_2306),
        (0x8000_01F4, 0x4700_3124, 0x4700_286F),
        (0x8000_01F5, 0x4700_3126, 0x4700_274D),
        (0x8000_01F7, 0x4700_312A, 0x4700_2532),
        (0x8000_01F8, 0x4700_312C, 0x4700_2B4E),
        (0x8000_01F9, 0x4700_312E, 0x4700_27DA),
        (0x8000_01FC, 0x4700_3134, 0x4700_2BA6),
        (0x8000_01FF, 0x4700_313A, 0x4700_2552),
        (0x8000_0202, 0x4700_3140, 0x4700_28D0),
        (0x8000_0206, 0x4700_3148, 0x4700_2594),
    ] {
        let steps = || t.quests.steps.iter().filter(|s| s.quest == quest);
        assert!(
            steps().any(|s| s.needs == 0
                && s.update == Update::Bestow
                && s.trigger == Trigger::Use(giver)),
            "{quest:08X} is given"
        );
        assert!(
            steps().any(|s| s.needs > 0
                && s.update == Update::Complete
                && s.trigger == Trigger::Use(giver)),
            "{quest:08X} is taken back"
        );
        assert!(
            t.content.objects.iter().any(|o| o.template == giver),
            "{quest:08X}'s giver stands"
        );
        assert!(
            t.spawns
                .camps
                .iter()
                .any(|c| c.templates.iter().any(|&(t, _)| t == creature)),
            "{quest:08X}'s creature has a camp"
        );
        assert!(
            catalog.get(creature).is_some(),
            "{creature:08X} is described"
        );
    }
}

/// A mushroom hunt's five mushrooms each lie in a camp and go to the vendor, who gives the hunt and
/// ends it with the stew; a start describes them all.
/// Behaviour: server.world-db.mushrooms-go-to-their-vendor
#[test]
fn a_mushroom_hunt_gives_lies_hands_in_and_ends_with_its_stew() {
    use yalain_content::quests::{Trigger, Update};
    let t = &compiled().tables;
    let catalog = yalain_testkit::authored::Portal::open().catalog(t);
    const VENDOR: u32 = 0x4700_3156;
    const ROOT: u32 = 0x8000_0276;
    const STEW: u32 = 0x4700_08BC;
    let end = t
        .quests
        .steps
        .iter()
        .find(|s| s.quest == ROOT && s.update == Update::Complete)
        .expect("it ends");
    assert_eq!(end.trigger, Trigger::Use(VENDOR));
    assert_eq!(end.gives, [STEW]);
    assert!(catalog.get(STEW).is_some());
    for (sub, mushroom) in [
        (0x8000_0263_u32, 0x4700_08B7_u32),
        (0x8000_0260, 0x4700_08B8),
        (0x8000_0264, 0x4700_08BA),
        (0x8000_0261, 0x4700_08B9),
        (0x8000_0262, 0x4700_08BB),
    ] {
        assert!(
            t.quests.steps.iter().any(|s| s.quest == sub
                && s.update == Update::Complete
                && s.trigger == Trigger::Give(mushroom, VENDOR)
                && s.takes == [mushroom]),
            "{sub:08X}"
        );
        assert!(
            t.spawns
                .camps
                .iter()
                .any(|c| c.templates.iter().any(|&(t, _)| t == mushroom)),
            "{mushroom:08X} lies somewhere"
        );
        assert!(catalog.get(mushroom).is_some(), "{mushroom:08X}");
    }
}

/// **A cancel waits as its world says**: every 2017 cancel's answer names the wait the world's rows
/// give its quest before it may be taken again (a world naming none, the server's own), five hours
/// for eleven quests and the rule's five minutes for the rest.
/// Behaviour: server.quests.cancel-waits-as-rows-say
#[test]
fn every_recorded_cancel_names_the_wait_the_world_gives_its_quest() {
    use auberean_script::args::{decode, Arg};
    use auberean_script::graph::{Body, PackedObject};
    use auberean_script::native_object::NativeObject;
    use yalain_world::quests::{cancel_waits_or_2017, status, wait_after, UPDATE_QUEST};
    let mut quests = compiled().tables.quests.clone();
    cancel_waits_or_2017(&mut quests);
    let lib = super::keeping::library().expect("the client library reads");
    let con = super::keeping::open(&auberean_testkit::data::require_capture_index());
    let mut st = con
        .prepare(
            "SELECT m.raw FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) \
             WHERE c.file LIKE '2017-%' AND s.game = 'ac2' \
             AND m.dir = 's2c' AND m.status = 'ac2' AND m.mtype = 65638",
        )
        .expect("the query prepares");
    let raws = st
        .query_map([], |r| r.get::<_, Vec<u8>>(0))
        .expect("the query runs");
    let (mut cancels, mut long) = (0usize, 0usize);
    let mut wrong: Vec<String> = Vec::new();
    for raw in raws.flatten() {
        let Ok(ServerMessage::PlayerCall(c)) = ServerMessage::decode(&raw) else {
            continue;
        };
        if c.function.0 != UPDATE_QUEST {
            continue;
        }
        let Some(args) = c.args.as_ref().and_then(|a| decode(&a.0, lib).ok()) else {
            continue;
        };
        for a in &args {
            let Arg::Object(g) = a else { continue };
            for e in &g.entries {
                let Body::Object(PackedObject {
                    native: Some(NativeObject::Quest(q)),
                    ..
                }) = &e.body
                else {
                    continue;
                };
                if q.status != status::CANCELLED {
                    continue;
                }
                cancels += 1;
                let recorded = f64::from_bits(q.seconds_until_retry);
                long += usize::from(recorded > 300.0);
                let ours = quests.quest(q.id).map(|d| wait_after(d, status::CANCELLED));
                if ours.is_none_or(|w| (w - recorded).abs() > 1.0) {
                    wrong.push(format!("{:08x}: {recorded} recorded, {ours:?} here", q.id));
                }
            }
        }
    }
    assert!(cancels >= 29, "cancels: {cancels}");
    assert!(long >= 11, "five-hour cancels: {long}");
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// **The hero's transformation and Aun Aulatah's tap at start**: as the server reads the world at
/// start, Elysa's last step transforms its player 25.59 s after the give with 2017's appearances,
/// behavior and text; the tap reads as taking a tenth of vitae with its state's own appearance; and
/// the transformation's own effect, put on, does nothing more.
/// Behaviour: server.gives.hero-transformation
#[test]
fn the_heros_transformation_and_the_vitae_tap_read_as_the_server_starts_them() {
    use yalain_world::effect_defs::{vitae_taps_or_2017, Kind};
    let t = &compiled().tables;
    let mut quests = t.quests.clone();
    yalain_world::gives::transformations_or_2017(&mut quests);
    let mut rules = t.effect_rules.clone();
    vitae_taps_or_2017(&mut rules);
    let hero = quests
        .steps
        .iter()
        .find(|s| {
            s.quest == 0x4000_024A
                && s.trigger == yalain_content::quests::Trigger::Give(0x4700_1505, 0x4700_3049)
        })
        .expect("Elysa's last step");
    assert_eq!(
        hero.transforms,
        Some(yalain_world::gives::transformation_2017())
    );
    let others = quests
        .steps
        .iter()
        .filter(|s| s.transforms.is_some())
        .count();
    assert_eq!(others, 1, "the one hero's step");
    let states: Vec<u32> = quests.effect_states().collect();
    let effects = effects_read(&states, &rules);
    let tap = effects.get(0x6F00_1C64).expect("the tap");
    assert_eq!(tap.kind, Kind::VitaeTap(0.1));
    assert_eq!(tap.fx, 0x4100_030B, "its own appearance");
    assert!(!tap.lasts());
    let heroize = effects
        .get(0x6F00_1C6F)
        .expect("the transformation's effect");
    assert_eq!(heroize.kind, Kind::Other);
    assert!(!heroize.lasts() && heroize.fx == 0, "nothing more");
}
