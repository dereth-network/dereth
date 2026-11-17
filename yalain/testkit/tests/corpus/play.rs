//! Playing in the start area held to the client's library and to the January 2017 recordings: the
//! calls this server sends for fights, deaths, loot, experience and levels, stores and quests read
//! back in the client's own library; and the recorded families of those messages, by shape and
//! order, against the shapes this server sends, every difference counted and named.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::{require_install, TARGET};
use auberean_dat::DatFile;
use auberean_protocol::script::FunctionId;
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::{decode, expected_tags, Arg};
use auberean_script::graph::Body;
use auberean_script::library::{Library, RECORD};
use rusqlite::{Connection, OpenFlags};
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::graph::{Builder, Slot};

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

fn use_of(item: u64, player: u64) -> Arg {
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
    Arg::Object(b.graph())
}

/// A whole play of the test world: a fight to the death (a level), the corpse opened, its item
/// taken, the rest turned into money; returns what the client received.
fn a_play() -> Vec<ServerMessage> {
    let mut h = Harness::new();
    h.add_creatures(2);
    h.server.levels = yalain_world::levels::LevelTable {
        levels: vec![
            yalain_world::levels::Level {
                xp: 0,
                health: 100,
                vigor: 100,
                skill_credits: 0,
                hero_credits: 0,
                hero_credit_cap: 0,
                armor_cap: 0,
                shield_cap: 0,
            },
            yalain_world::levels::Level {
                xp: 10,
                health: 130,
                vigor: 130,
                skill_credits: 1,
                hero_credits: 0,
                hero_credit_cap: 0,
                armor_cap: 0,
                shield_cap: 0,
            },
        ],
    };
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    h.arm(b, me, 0x4700_051C);
    let beetle = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("a beetle");
    // The day plays a won fight: the beetles hit as softly as a level 1 player's armor let a
    // beetle's hits through before armor was measured (a quarter), so the player outlasts them.
    for t in h.server.things.by_id.values_mut() {
        if let Some(c) = t.creature.as_mut() {
            c.stats.natural_damage = 2.6;
        }
    }
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8204_0008, &[Arg::LongInt(beetle)]),
    );
    assert!(h.run_until(6000, |h| h.world_messages(b, 0).iter().any(
        |m| matches!(m, ServerMessage::CreateObject(o) if o.weenie.entity_did == 0x4700_0539)
    )));
    let corpse = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == 0x4700_0539 => Some(o.id),
            _ => None,
        })
        .expect("a corpse");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8216_0016, &[use_of(corpse, me)]),
    );
    h.run(20);
    let item = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == yalain_testkit::TEST_ITEM => {
                Some(o.id)
            }
            _ => None,
        })
        .expect("an item in the corpse");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8216_0016, &[use_of(item, me)]),
    );
    h.run(60);
    let mut out = in_sending_order(&h, b);
    out.extend(a_visit());
    out.extend(a_hard_day());
    out.extend(a_day_of_specials());
    out.extend(a_day_of_heals_and_crafts());
    out
}

/// What a client received from the world, in the order the server sent it (the transport may
/// carry a packet with acknowledgements ahead of the bulk before it).
fn in_sending_order(h: &Harness, b: usize) -> Vec<ServerMessage> {
    let mut got: Vec<_> = h.bots[b].inbox.iter().filter(|r| r.world).collect();
    got.sort_by_key(|r| r.blob_id & 0xFFFF_FFFF);
    got.into_iter()
        .filter_map(yalain_testkit::bot::Received::message)
        .collect()
}

/// Run the world for so many seconds.
fn wait(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

/// A day of heals, buffs, crafts and a bow: Ease Suffering on itself, Dash run out, a blessing
/// from a quest step, the Arwic mines' recipe learned and made with twenty iron, a bow's run on a
/// beetle that stands, Deadeye cut short by moving; then back in the world with its blessing.
fn a_day_of_heals_and_crafts() -> Vec<ServerMessage> {
    use yalain_content::quests::{Step, Trigger, Update};
    use yalain_world::player::quality;
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, me) = h.player_in_world("heals", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    let beetle = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("the camp's creature");
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.movement.perception = 0.0;
        c.stats.scale = 0.0;
    }
    h.arm(b, me, 0x4700_051B);
    if let Some(p) = h.server.players.get_mut(&world) {
        yalain_world::quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 400);
        yalain_world::quals::set_int(&mut p.qualities, quality::VIGOR, 400);
        yalain_world::quals::set_int(&mut p.qualities, quality::HEALTH, 50);
    }
    let use_skill = |h: &mut Harness, id: u32, skill: u32, target: u64| {
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(
                yalain_world::skill_use::function::USE,
                &[Arg::Int(id), Arg::Int(skill), Arg::LongInt(target)],
            ),
        );
    };
    use_skill(&mut h, 1, yalain_testkit::EASE_SUFFERING, 0);
    wait(&mut h, 3.0);
    use_skill(&mut h, 2, yalain_testkit::DASH, me);
    wait(&mut h, 35.0);
    // A blessing from a quest step.
    let quest = 0x4000_0016;
    h.server
        .quests
        .quests
        .push(yalain_content::quests::QuestDef {
            duration_by_calendar: false,
            id: quest,
            name: yalain_content::Words::Ref(1, 2),
            description: (3, 4),
            icon: 0,
            play_fx: false,
            journals: vec![(5, 6); 5],
            challenge: 4,
            duration: 0.0,
            timed_phase: 0,
            retry: 0.0,
            xp: 0,
            cancellable: true,
            placed: "made up for the play".into(),
            hidden: false,
            effects: Vec::new(),
            subquests: Vec::new(),
            min_level: 0,
            fails_on_death: false,
            fails_on_logout: false,
            fails_on_teleport: false,
            once: false,
            fellows: yalain_content::quests::Fellows::default(),
            ranges: Vec::new(),
            hero_only: false,
            pay: yalain_content::quests::CompletionPay::default(),
            required: None,
            cancel_wait: None,
        });
    let step = Step {
        out_of_turn: false,
        key: String::new(),
        quest,
        needs: 0,
        trigger: Trigger::Use(0x4700_31ED),
        tell: None,
        update: Update::Bestow,
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: vec![yalain_testkit::COURAGE_EFFECT],
        text: Some((0x912E_FF1D, 0x2500_10E8)),
        recipes: Vec::new(),
        placed: "made up for the play".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        races: Vec::new(),
        effects_last: false,
        effects_quiet: false,
        wants: None,
        fellows: false,
        levels: None,
        xp_share: 0.0,
        to: None,
        transforms: None,
        chance: None,
        to_fellows: false,
    };
    let clock = h.clock();
    yalain_world::quests::run(&mut h.server, &mut h.net, world, &step, None, clock);
    // The Arwic mines' recipe and twenty iron, put in and made.
    for t in [yalain_testkit::IRON, yalain_testkit::MINES_SWORD] {
        h.server.catalog.insert(yalain_testkit::test_template(
            t,
            0x200,
            yalain_content::Props::default(),
        ));
    }
    let clock = h.clock();
    yalain_world::effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        yalain_testkit::GRANT_MINES_RECIPE,
        yalain_world::effects::Cast {
            caster: me,
            spellcraft: 1.0,
            ..yalain_world::effects::Cast::default()
        },
        clock,
    );
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, yalain_testkit::IRON, 20);
    h.run(2);
    let iron = h
        .server
        .players
        .get(&world)
        .and_then(|p| {
            p.play
                .items
                .iter()
                .find(|i| i.template == yalain_testkit::IRON)
        })
        .map(|i| i.id)
        .expect("the iron");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            yalain_world::craft::function::ADD_INGREDIENT,
            &[
                Arg::Int(0),
                Arg::LongInt(iron),
                Arg::Int(1),
                Arg::Int(yalain_testkit::MINES_RECIPE),
            ],
        ),
    );
    h.run(4);
    let mut g = Builder::new();
    g.native(
        8,
        yalain_world::graph::hash(
            auberean_script::library::Elem::Word,
            auberean_script::library::Elem::Long,
            0,
            &[(
                auberean_script::native_object::Item::Word(1),
                auberean_script::native_object::Item::Long(iron),
            )],
        ),
    );
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            yalain_world::craft::function::EXECUTE,
            &[
                Arg::LongInt(0),
                Arg::Object(g.graph()),
                Arg::Int(0),
                Arg::Int(yalain_testkit::MINES_RECIPE),
            ],
        ),
    );
    wait(&mut h, 6.0);
    // A bow's run on the beetle 6 m off.
    let at = h
        .server
        .players
        .get(&world)
        .map(|p| p.position)
        .expect("in the world");
    if let Some(t) = h.server.things.get_mut(beetle) {
        let mut place = at;
        place.origin[0] += 6.0;
        t.at = Some(place);
    }
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8204_0008, &[Arg::LongInt(beetle)]),
    );
    wait(&mut h, 5.0);
    h.bots[b].send(true, queue::WEENIE, &server_call(0x8204_0010, &[]));
    wait(&mut h, 3.0);
    // Deadeye cut short by moving after its story.
    use_skill(&mut h, 3, 207, beetle);
    h.run(6);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &yalain_testkit::play::report(
            at.cell,
            auberean_primitives::Vec3::new(at.origin[0], at.origin[1], at.origin[2]),
            0,
            yalain_world::world::motion(127, 255, 127),
            0,
        ),
    );
    wait(&mut h, 3.0);
    let mut out = h.world_messages(b, 0);
    // Back in the world, its blessing in the session start.
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &auberean_protocol::ClientMessage::LogOff(auberean_protocol::login::CharacterRef {
            character: me,
        }),
    );
    assert!(h.run_until(400, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "heals", me);
    out.extend(h.world_messages(b, from));
    out
}

/// A day of specials beside a practice target: Deadeye refused with an empty hand, used with the
/// starting bow, refused again before its recovery; rested, the recall of a player never bound.
fn a_day_of_specials() -> Vec<ServerMessage> {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let beetle = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("a beetle");
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.fights = false;
    }
    let use_skill = |h: &mut Harness, id: u32, skill: u32, target: u64, steps: usize| {
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(
                0x8204_000B,
                &[Arg::Int(id), Arg::Int(skill), Arg::LongInt(target)],
            ),
        );
        h.run(steps);
    };
    use_skill(&mut h, 1, 207, beetle, 10);
    h.arm(b, me, 0x4700_051B);
    use_skill(&mut h, 2, 207, beetle, 60);
    use_skill(&mut h, 3, 207, beetle, 60);
    // Rested (a recall takes all of the player's vigor), the recall.
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, 259, 100);
    }
    use_skill(&mut h, 4, 835, 0, 400);
    h.world_messages(b, 0)
}

/// A hard day: shot dead by a creature that shoots (its missiles made deadly), its corpse made,
/// resurrected; then a skill trained and raised.
fn a_hard_day() -> Vec<ServerMessage> {
    use std::collections::HashMap;
    use yalain_content::Prop;
    use yalain_world::skills::{SkillDef, SkillTable};
    let mut h = Harness::new();
    let mut p = yalain_content::Props::default();
    p.set("GameplayStatistics_Level", Prop::Int(3))
        .set("GameplayStatistics_MaxHealth", Prop::Int(60))
        .set("AICombat_NaturalDamage", Prop::Float(1000.0))
        .set("AI_PerceptionRadius", Prop::Float(30.0))
        .set("NPC_StartingSkill1", Prop::Word(1861))
        .set("NPC_StartingSkill1_IsBase", Prop::Bool(true));
    let mut catalog = yalain_testkit::test_catalog();
    catalog.insert(yalain_testkit::test_template(0x4700_2225, 0x1169, p));
    h.server.catalog = catalog;
    let spot = [81.0, 240.2, -44.6];
    h.server.spawns.camps.push(yalain_content::Camp {
        id: "shooters".into(),
        cell: 0x0299_0109,
        origin: spot,
        radius: 0.0,
        population: 1,
        regen: 60.0,
        templates: vec![(0x4700_2225, 1)],
        spots: vec![(0x0299_0109, spot)],
        placed: "authored for a test".into(),
        confidence: "authored".into(),
        moves: yalain_content::CampMoves::default(),
        name: None,
    });
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
    let mut skills = HashMap::new();
    skills.insert(
        15,
        SkillDef {
            id: 15,
            name: (0xA860_7CEC, 0x2500_085B),
            cost: 1,
            min_level: 1,
            cap: 50,
            table: 0x6F00_000E,
            flags: 1,
            ..SkillDef::default()
        },
    );
    let mut tables = HashMap::new();
    tables.insert(
        0x6F00_000E,
        (0..=50u64).map(|n| 2 * n * n.saturating_sub(1)).collect(),
    );
    h.server.skill_table = SkillTable {
        skills,
        tables,
        ..SkillTable::default()
    };
    let (b, _) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    for p in h.server.players.values_mut() {
        p.play.credits = 1;
        yalain_world::quals::set_long(&mut p.qualities, 301, 100);
    }
    let dead = |h: &Harness| {
        h.world_messages(b, 0)
            .iter()
            .any(|m| matches!(m, ServerMessage::PlayerCall(c) if c.function.0 == 0x8207_0033))
    };
    assert!(h.run_until(6000, dead), "shot dead, its corpse made");
    h.bots[b].send(true, queue::WEENIE, &server_call(0x8207_000E, &[]));
    h.run(60);
    for f in [0x8214_0002, 0x8214_000C] {
        h.bots[b].send(true, queue::WEENIE, &server_call(f, &[Arg::Int(15)]));
        h.run(10);
    }
    h.world_messages(b, 0)
}

/// A visit to people: a quest given, a store opened and a sale bought.
fn a_visit() -> Vec<ServerMessage> {
    use yalain_content::quests::{QuestDef, QuestTable, Step, Trigger, Update};
    use yalain_content::stores::{Keeper, Sale, SaleGate, StoreTable};
    use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind};
    let person = |id: u64, template: u32| ContentObject {
        object: Some(id),
        template,
        description: Description {
            name_text: None,
            package: 0x13A4,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: Some((0x1F6E_D407, 0x2500_2AFC)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin: [61.0, 241.0, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Npc,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts::default(),
        walks: None,
        gate: None,
    };
    let (keeper, guard) = (0x8130_0000_0000_0101, 0x8130_0000_0000_0102);
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.content = ContentPack {
        objects: vec![person(keeper, 0x4700_33D0), person(guard, 0x4700_2DFD)],
        ..ContentPack::default()
    };
    h.server.stores = StoreTable {
        keepers: vec![Keeper {
            template: 0x4700_33D0,
            store: 0x6F00_16F6,
            shown: None,
            greeting: Some(yalain_content::Words::Ref(0x2824_938C, 0x2500_2AFC)),
            placed: "authored for a test".into(),
        }],
        sales: vec![Sale {
            store: 0x6F00_16F6,
            ordinal: 0,
            product: yalain_testkit::TEST_ITEM,
            cost: 10.0,
            currency: 0x4700_0538,
            stack: 1,
            icon: vec![(1000, 0x4100_4552, 0xFFFF_FFFF)],
            name: (0x01A2_D809, 0x2500_23D3),
            placed: "authored for a test".into(),
            gate: SaleGate::default(),
        }],
        hiding: Vec::new(),
    };
    h.server.quests = QuestTable {
        quests: vec![QuestDef {
            duration_by_calendar: false,
            id: 0x4000_0088,
            name: yalain_content::Words::Ref(0xA860_7CEC, 0x2500_17EE),
            description: (0x6C45_9364, 0x2500_17EE),
            icon: 0x4100_034B,
            play_fx: true,
            journals: vec![(0x2E5C_1E52, 0x2500_17EE)],
            challenge: -999,
            duration: 72000.0,
            timed_phase: 0,
            retry: 0.0,
            xp: 0,
            cancellable: false,
            placed: "authored for a test".into(),
            hidden: false,
            effects: Vec::new(),
            subquests: Vec::new(),
            min_level: 0,
            fails_on_death: false,
            fails_on_logout: false,
            fails_on_teleport: false,
            once: false,
            fellows: yalain_content::quests::Fellows::default(),
            ranges: Vec::new(),
            hero_only: false,
            pay: yalain_content::quests::CompletionPay::default(),
            required: None,
            cancel_wait: None,
        }],
        steps: vec![Step {
            out_of_turn: false,
            key: String::new(),
            quest: 0x4000_0088,
            needs: 0,
            trigger: Trigger::Use(0x4700_2DFD),
            tell: Some(yalain_content::Words::Ref(0x2824_FD4C, 0x2500_2881)),
            update: Update::Bestow,
            xp: 0,
            gives: Vec::new(),
            counts: Vec::new(),
            effects: Vec::new(),
            text: None,
            recipes: Vec::new(),
            placed: "authored for a test".into(),
            requires: Vec::new(),
            takes: Vec::new(),
            races: Vec::new(),
            effects_last: false,
            effects_quiet: false,
            wants: None,
            fellows: false,
            levels: None,
            xp_share: 0.0,
            to: None,
            transforms: None,
            chance: None,
            to_fellows: false,
        }],
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let (b, me) = h.player_in_world("two", "Rowan");
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, 3100, 50);
    }
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8216_0016, &[use_of(guard, me)]),
    );
    h.run(10);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8216_0016, &[use_of(keeper, me)]),
    );
    h.run(10);
    let stock = yalain_world::store::STOCK_BASE | 0x16F6 << 8;
    let mut g = Builder::new();
    g.object(
        0x1B76,
        &[
            Slot::Long(stock),
            Slot::Ref(None),
            Slot::Long(me),
            Slot::Int(1),
            Slot::Long(keeper),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x9B5E_0031, &[Arg::Object(g.graph())]),
    );
    h.run(10);
    h.world_messages(b, 0)
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

/// Every call this server sends in a play of the start area (a fight to a level, a corpse opened
/// and an item taken) reads back in the client's own library: the values carry the tags of the
/// handler's declared parameters, and every script object is of its package's size.
/// Behaviour: server.play.calls-read-in-the-client-library
#[test]
fn every_call_of_a_play_reads_back_against_the_client_library() {
    let Some(lib) = library() else {
        println!("no September 2013 install here");
        return;
    };
    let mut checked: BTreeMap<String, usize> = BTreeMap::new();
    for m in a_play() {
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
        "WM_Combat::PostCEvt_UpdateAttackState",
        "WM_Skill::PostCEvt_Skill_UpdateRepository",
        "WM_Inventory::PostCEvt_OpenContainer_Done",
        "WM_Inventory::PostCEvt_MoveItem_Done",
        "WM_Quest::PostCEvt_UpdateQuest_Done",
        "WM_Store::PostCEvt_Store_EnterStore",
        "WM_Store::PostCEvt_Store_UpdateStore",
        "WM_Skill::PostCEvt_Skill_UpdateInfo",
        "WM_Effect::PostCEvt_Effect_ClientAddEffect",
        "WM_Combat::PostCEvt_EndSpecialAttack",
        "WM_Combat::PostCEvt_Combat_AttackError",
        "WM_Effect::PostCEvt_Effect_ClientRemoveEffect",
        "WM_Craft::PostCEvt_CraftRefresh",
        "WM_Craft::PostCEvt_UpdateCraftSkills_Done",
        "WM_Craft::PostCEvt_AddRecipe_Done",
        "WM_Craft::PostCEvt_AddIngredient_Done",
        "WM_Craft::PostCEvt_ExecuteCraft_Done",
    ] {
        assert!(checked.contains_key(want), "{want} was sent");
    }
}

/// A token for a server message: its type, and what tells messages of a type apart (a call's
/// function, a quality's id, an effect's or a mode's word).
fn token(m: &ServerMessage) -> String {
    match m {
        ServerMessage::PlayerCall(c) => format!("call {:08X}", c.function.0),
        ServerMessage::UpdateInt(q) => format!("int {}", q.quality),
        ServerMessage::UpdateLongInt(q) => format!("long {}", q.quality),
        ServerMessage::UpdateBoolVisual(q) => format!("bool {}={}", q.quality, q.value),
        ServerMessage::UpdateInstanceIdVisual(q) => format!("iid {}", q.quality),
        ServerMessage::DoFx(e) => format!("fx {:08X}", e.action.fx),
        ServerMessage::StopFx(e) => format!("stopfx {:08X}", e.action.0),
        ServerMessage::DoMode(e) => format!("mode {:08X}", e.action.0),
        ServerMessage::PrivateFx(_) => "privatefx".into(),
        ServerMessage::Contain(_) => "contain".into(),
        ServerMessage::PlayBehavior(_) => "behavior".into(),
        ServerMessage::MoveSlider(_) => "slider".into(),
        ServerMessage::DoStory(e) => format!("story {:02X}", e.action.bits),
        ServerMessage::VisualObjectCall(c) => format!("ocall {:08X}", c.call.function.0),
        other => format!("type {:05X}", other.message_type()),
    }
}

/// The families held, each by the tokens of the messages of one server tick that open with a
/// marker, in order.
struct Family {
    name: &'static str,
    /// The token that opens an occurrence.
    opens: &'static str,
    /// A token the occurrence's tick must hold to count (else it is another family's).
    needs: Option<&'static str>,
    /// How long after its opening an occurrence lasts, in seconds (2017's answers came in one
    /// tick, or over a fraction of a second for a corpse opening).
    window: f64,
    /// Whether most recorded occurrences are expected in this server's order (else the recorded
    /// orders vary, and the tally names each).
    dominant: bool,
    /// The tokens this server sends for it, in order (a subsequence of the tick's).
    ours: &'static [&'static str],
}

const FAMILIES: &[Family] = &[
    Family {
        name: "a heal, as the healed one tells it",
        opens: "story 02",
        needs: Some("int 258"),
        window: 0.06,
        dominant: true,
        ours: &["story 02", "int 258"],
    },
    Family {
        name: "a speed buff run out",
        opens: "call 82080016",
        needs: Some("type 10092"),
        window: 0.06,
        dominant: true,
        ours: &["call 82080016", "type 10092", "type 10093"],
    },
    Family {
        name: "a craft's result",
        opens: "contain",
        needs: Some("call 8206001C"),
        window: 0.06,
        dominant: false,
        ours: &["contain", "call 820E005E", "call 8206001C"],
    },
    Family {
        name: "a bow's thrown swing",
        opens: "story 36",
        needs: Some("behavior"),
        window: 0.06,
        dominant: true,
        ours: &["story 36", "behavior"],
    },
    Family {
        name: "a use cut short after its story",
        opens: "call 8214000D",
        needs: Some("call 8204001A"),
        window: 0.06,
        dominant: true,
        ours: &[
            "call 8214000D",
            "call 82040019",
            "call 8204001A",
            "call 8204001B",
        ],
    },
    Family {
        name: "a skill's use refused",
        opens: "call 82040019",
        needs: Some("call 8204001A"),
        window: 0.06,
        dominant: true,
        ours: &["call 82040019", "call 8204001A"],
    },
    Family {
        name: "a skill's use ended from rest",
        opens: "call 8214000D",
        needs: Some("mode 40000001"),
        window: 0.06,
        dominant: true,
        ours: &["call 8214000D", "call 8204001B", "mode 40000001"],
    },
    Family {
        name: "a recall's end",
        opens: "call 8214000D",
        needs: Some("type 10077"),
        window: 0.06,
        dominant: true,
        ours: &[
            "call 8214000D",
            "type 10077",
            "call 8204001B",
            "mode 40000001",
        ],
    },
    Family {
        name: "a kill's award (solo, with experience)",
        opens: "call 82050011",
        needs: None,
        window: 0.5,
        dominant: true,
        ours: &["call 82050011", "long 300", "long 301", "call 82050011"],
    },
    Family {
        name: "a level-up",
        opens: "int 302",
        needs: None,
        window: 0.06,
        dominant: true,
        // The congratulations text comes before or after the maxima (2017 did both).
        ours: &[
            "int 302",
            "call 82140011",
            "int 263",
            "int 264",
            "int 258",
            "int 259",
        ],
    },
    Family {
        name: "a creature's death, as seen",
        opens: "bool 279=1",
        needs: None,
        window: 0.06,
        dominant: true,
        ours: &[
            "bool 279=1",
            "fx 41000002",
            "bool 624=0",
            "slider",
            "behavior",
        ],
    },
    Family {
        name: "a corpse opened",
        opens: "fx 410005D0",
        needs: Some("call 820E005D"),
        window: 0.8,
        dominant: true,
        ours: &[
            "fx 410005D0",
            "mode 41000002",
            "call 820E005D",
            "bool 702=1",
            "stopfx 41000004",
        ],
    },
    Family {
        name: "an item taken",
        opens: "contain",
        needs: Some("call 820E005E"),
        window: 0.06,
        dominant: false,
        ours: &["contain", "call 820E005E", "iid 258"],
    },
    Family {
        name: "an attack begun",
        opens: "call 8204001B",
        needs: None,
        window: 0.06,
        dominant: true,
        ours: &["call 8204001B", "mode"],
    },
    Family {
        name: "a store entered",
        opens: "call 82050017",
        needs: Some("call 9B5E0056"),
        window: 0.06,
        dominant: true,
        ours: &["call 82050017", "call 9B5E0056"],
    },
    Family {
        name: "a quest moved on by a person",
        opens: "call 82050017",
        needs: Some("call 82130024"),
        window: 0.06,
        dominant: true,
        ours: &["call 82050017", "call 82130024"],
    },
    Family {
        name: "a missile's hit, as its target tells it",
        opens: "int 258",
        needs: Some("story 32"),
        window: 0.06,
        dominant: true,
        ours: &["int 258", "story 32"],
    },
    Family {
        name: "a player's death, as seen",
        opens: "bool 279=1",
        needs: Some("ocall 82070030"),
        window: 0.06,
        dominant: true,
        ours: &[
            "bool 279=1",
            "fx 41000002",
            "ocall 82070030",
            "bool 624=0",
            "slider",
            "behavior",
        ],
    },
    Family {
        name: "a player's corpse made",
        opens: "int 259",
        needs: Some("call 82070033"),
        window: 0.06,
        dominant: true,
        ours: &["int 259", "call 82070033"],
    },
    Family {
        name: "a resurrection's return",
        opens: "call 8212001B",
        needs: Some("call 82070033"),
        window: 0.06,
        dominant: true,
        ours: &[
            "call 8212001B",
            "call 8212001F",
            "int 258",
            "int 259",
            "call 82070033",
        ],
    },
    Family {
        name: "a skill trained",
        opens: "call 82140011",
        needs: Some("call 8214000D"),
        window: 0.06,
        dominant: true,
        ours: &["call 82140011", "call 8214000D", "call 82050011"],
    },
    Family {
        name: "a skill raised",
        opens: "call 8214000D",
        needs: Some("long 301"),
        window: 0.06,
        dominant: true,
        ours: &["call 8214000D", "long 301", "call 82050011"],
    },
    Family {
        name: "a sale bought",
        opens: "int 3100",
        needs: Some("call 9B5E0049"),
        window: 0.06,
        dominant: false,
        ours: &[
            // The storekeeper's opening is a visual message: its place among the private ones
            // varies.
            "int 3100",
            "contain",
            "call 820E005E",
            "call 9B5E0049",
        ],
    },
];

/// Is `want` a subsequence of `have` (tokens of `want` that end in a space match by prefix)?
fn in_order(have: &[String], want: &[&str]) -> Result<(), String> {
    let mut i = 0;
    for w in want {
        let found = have[i..]
            .iter()
            .position(|h| h == w || (w.len() < h.len() && h.starts_with(w)));
        match found {
            Some(k) => i += k + 1,
            None => return Err(format!("no {w}")),
        }
    }
    Ok(())
}

/// The recorded families of the January 2017 world sessions hold the shapes this server sends:
/// each occurrence's tick holds this server's messages in this server's order, or is counted with
/// what it lacks.
/// Behaviour: server.play.recorded-families-by-shape
#[test]
fn the_recorded_play_families_hold_the_shapes_this_server_sends() {
    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let ours = a_play();
    let tokens: Vec<String> = ours.iter().map(token).collect();
    // Our own play holds each family's shape.
    for f in FAMILIES {
        assert!(
            in_order(&tokens, f.ours).is_ok(),
            "ours: {} ({:?})",
            f.name,
            in_order(&tokens, f.ours)
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
                if tok != f.opens && !(f.opens.len() < tok.len() && tok.starts_with(f.opens)) {
                    continue;
                }
                // The kill family opens with its experience text: only an award text counts.
                if f.name.starts_with("a kill")
                    && toks.get(i + 1).map(|x| x.1.as_str()) != Some("long 300")
                {
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

mod crafts {
    //! Every recorded craft of January 2017 played through the server at its spinner, with the recipes
    //! as the data defines them: the slots it sent are the ones the spinner asks for, the server
    //! answers as the recording did, and each ingredient goes down by what the recording shows.

    use std::collections::{BTreeMap, BTreeSet, HashMap};

    use auberean_protocol::{queue, ServerMessage};
    use auberean_script::args::Arg;
    use auberean_script::library::Elem;
    use auberean_script::native_object::Item;
    use auberean_testkit::captures::{self, carrier, Message};
    use auberean_testkit::data::require_capture_index;
    use yalain_testkit::authored::Portal;
    use yalain_testkit::play::server_call;
    use yalain_testkit::Harness;
    use yalain_world::calls::{self, Value};
    use yalain_world::craft::{self, error, function, Action, RecipeDef};
    use yalain_world::state::EffectState;

    /// The message types the recording is read from, beside the script calls.
    const UPDATE_INT_VISUAL: u32 = 0x1_006A;
    const CREATE_OBJECT: u32 = 0x1_0058;

    /// One recorded craft: what it asked, how it was answered, what each slot went down by.
    #[derive(Debug, Default)]
    struct Recorded {
        recipe: u32,
        spinner: u32,
        slots: Vec<(u32, u64)>,
        answer: Option<u32>,
        answered_at: f64,
        used: BTreeMap<u32, u32>,
        templates: BTreeMap<u32, u32>,
    }

    /// The crafts of the recordings, in order.
    fn recorded() -> Vec<Recorded> {
        let index = require_capture_index();
        let mut with_crafts = BTreeSet::new();
        captures::messages(&index, &[carrier::TO_SERVER], &[2017], |m| {
            if m.script_call()
                .is_some_and(|c| c.function == function::EXECUTE)
            {
                with_crafts.insert(m.capture);
            }
        })
        .expect("the capture index reads");
        let mut out = Vec::new();
        for capture in with_crafts {
            let mut quantity: HashMap<u64, u32> = HashMap::new();
            let mut template: HashMap<u64, u32> = HashMap::new();
            let mut open: Option<Recorded> = None;
            let types = [
                carrier::TO_SERVER,
                carrier::TO_PLAYER,
                UPDATE_INT_VISUAL,
                CREATE_OBJECT,
            ];
            captures::capture_by_id_in_time_order(&index, &types, capture, |m: Message| {
                if open
                    .as_ref()
                    .is_some_and(|o| o.answer.is_some() && m.time > o.answered_at + 1.0)
                {
                    out.extend(open.take());
                }
                match m.message_type {
                    carrier::TO_SERVER | carrier::TO_PLAYER => {
                        let Some(c) = m.script_call() else { return };
                        if c.function == function::EXECUTE {
                            let values = calls::values(&c.args).expect("a craft's values read");
                            let [Value::Long(_), Value::Graph(g), Value::Int(spinner), Value::Int(recipe)] =
                                values.as_slice()
                            else {
                                panic!("a craft's request: {values:?}");
                            };
                            out.extend(open.take());
                            let slots = craft::slots_in(g);
                            let templates = slots
                                .iter()
                                .filter_map(|(o, i)| template.get(i).map(|t| (*o, *t)))
                                .collect();
                            open = Some(Recorded {
                                recipe: *recipe,
                                spinner: *spinner,
                                slots,
                                templates,
                                ..Recorded::default()
                            });
                        } else if c.function == function::EXECUTED {
                            let values = calls::values(&c.args).expect("an answer's values read");
                            if let (Some(o), Some(Value::Int(err))) = (open.as_mut(), values.get(1)) {
                                if o.answer.is_none() {
                                    o.answer = Some(*err);
                                    o.answered_at = m.time;
                                }
                            }
                        }
                    }
                    UPDATE_INT_VISUAL => {
                        let Ok(ServerMessage::UpdateIntVisual(q)) = ServerMessage::decode(&m.raw)
                        else {
                            return;
                        };
                        if q.quality != craft::QUANTITY {
                            return;
                        }
                        let id = q.object.id;
                        let value = u32::try_from(q.value).unwrap_or(0);
                        if let Some(o) = open.as_mut() {
                            if let (Some(slot), Some(before)) = (
                                o.slots.iter().find(|(_, i)| *i == id).map(|(s, _)| *s),
                                quantity.get(&id),
                            ) {
                                *o.used.entry(slot).or_default() += before.saturating_sub(value);
                            }
                        }
                        quantity.insert(id, value);
                    }
                    CREATE_OBJECT => {
                        if let Ok(ServerMessage::CreateObject(o)) = ServerMessage::decode(&m.raw) {
                            template.insert(o.id, o.weenie.entity_did);
                        }
                    }
                    _ => {}
                }
            })
            .expect("the capture reads");
            out.extend(open.take());
        }
        out
    }

    /// Ask to make a recipe, as the client packs it.
    fn ask(h: &mut Harness, b: usize, recipe: u32, spinner: u32, slots: &[(u32, u64)]) -> usize {
        let mut g = yalain_world::graph::Builder::new();
        let entries: Vec<(Item, Item)> = slots
            .iter()
            .map(|(s, i)| (Item::Word(*s), Item::Long(*i)))
            .collect();
        g.native(
            8,
            yalain_world::graph::hash(Elem::Word, Elem::Long, 0, &entries),
        );
        let from = h.bots[b].inbox.len();
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(
                function::EXECUTE,
                &[
                    Arg::LongInt(0),
                    Arg::Object(g.graph()),
                    Arg::Int(spinner),
                    Arg::Int(recipe),
                ],
            ),
        );
        from
    }

    /// The answer the server gave since `from`.
    fn answered(h: &Harness, b: usize, from: usize) -> Option<u32> {
        h.calls(b, from)
            .into_iter()
            .find(|(f, _)| *f == function::EXECUTED)
            .and_then(|(_, v)| match v.get(1) {
                Some(Value::Int(e)) => Some(*e),
                _ => None,
            })
    }

    fn workshop_record(state: u32) -> EffectState {
        EffectState {
            eid: 0xFFFF_0000,
            state,
            cast: 0.0,
            timeout: f32::MAX,
            caster: 0x4057_6600_0000_0025,
            spellcraft: 1.0,
            pk: false,
            acting: 0,
            skill: 0,
            item: 0x4057_6600_0000_0025,
            flags: yalain_world::workshops::RECORD_FLAGS,
            categories: yalain_world::workshops::RECORD_CATEGORIES,
        }
    }

    /// Every recorded craft with an answer the server makes goes through the server at its spinner:
    /// its slots are what the spinner asks for when it was made; it is answered as recorded (made or
    /// failed its check alike: the roll is its own); and made, each slot goes down by what the
    /// recording shows.
    /// Behaviour: server.craft.recorded-crafts-play
    #[test]
    fn every_recorded_craft_plays_at_its_spinner_as_recorded() {
        // An enchantment of the install's that spellbinding may take.
        const ENCHANTMENT: u32 = 0x6F00_0394;
        let crafts = recorded();
        let portal = Portal::open();
        let recipes: Vec<u32> = crafts
            .iter()
            .map(|c| c.recipe)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut crafting = portal.crafting(&[], &recipes);
        // Where a craft is made is not what is played here: the world here holds no mines.
        for r in crafting.recipes.values_mut() {
            r.targets.clear();
        }
        let lib = portal.library();

        let mut h = Harness::new();
        // The enchantment a taken-apart item is given below, as the install defines it: one
        // spellbinding may take.
        h.server
            .effect_table
            .effects
            .extend(portal.effects(&[ENCHANTMENT]).effects);
        let (b, me) = h.player_in_world("one", "Elara");
        h.bots[b].send(
            true,
            queue::WEENIE,
            &auberean_protocol::ClientMessage::SceneRenderingComplete(
                auberean_protocol::login::Empty,
            ),
        );
        h.run(5);
        let world = *h
            .server
            .players
            .iter()
            .find(|(_, p)| p.character == me)
            .expect("in the world")
            .0;

        // What the recipes ask for and make, and what the recordings put in slots that take a class.
        let mut templates: BTreeSet<u32> = crafting
            .recipes
            .values()
            .flat_map(|r: &RecipeDef| r.ingredients.iter())
            .flat_map(|i| i.items.iter().copied())
            .collect();
        let in_class: BTreeSet<u32> = crafts
            .iter()
            .flat_map(|c| c.templates.values().copied())
            .collect();
        let packages: Vec<(u32, u32)> = in_class
            .iter()
            .map(|t| (*t, portal.template(*t).description.package))
            .collect();
        crafting.resolve_classes(&packages, &|package, class| {
            lib.derives_from(package, class)
        });
        templates.extend(
            crafting
                .recipes
                .values()
                .flat_map(|r| r.ingredients.iter())
                .flat_map(|i| i.items.iter().copied()),
        );
        templates.extend(
            crafting
                .recipes
                .values()
                .flat_map(|r| r.checks.iter())
                .flat_map(|c| c.outcomes.iter())
                .flat_map(|o| o.actions.iter())
                .filter_map(|a| match a {
                    Action::Produce { template, .. } => Some(*template),
                    _ => None,
                }),
        );
        for t in &templates {
            h.server.catalog.insert(yalain_testkit::test_template(
                *t,
                0x200,
                yalain_content::Props::default(),
            ));
        }
        let tables = crafting.clone();
        h.server.crafting = crafting;

        let mut tally: BTreeMap<String, usize> = BTreeMap::new();
        let mut count = |k: &str| *tally.entry(k.to_owned()).or_default() += 1;
        let mut wrong = Vec::new();
        for c in &crafts {
            let Some(answer) = c.answer else {
                count("no answer recorded");
                continue;
            };
            if ![
                0,
                error::FAILED,
                error::INVALID_INGREDIENTS,
                error::MISSING_EFFECT,
            ]
            .contains(&answer)
            {
                count("an answer this test does not make");
                continue;
            }
            let Some(def) = tables.recipes.get(&c.recipe) else {
                count("a recipe that does not read");
                continue;
            };
            // The slots a making sent are the ones its spinner asks for.
            let asked: BTreeSet<u32> = def.ingredients_at(c.spinner).map(|i| i.ordinal).collect();
            let sent: BTreeSet<u32> = c.slots.iter().map(|(o, _)| *o).collect();
            if answer == 0 || answer == error::FAILED {
                count("made or failed its check");
                if asked != sent {
                    wrong.push(format!("{c:x?}: asked for {asked:?}"));
                }
            }
            let items: BTreeSet<u64> = c.slots.iter().map(|(_, i)| *i).collect();
            if items.len() < c.slots.len() {
                count("one item named for two slots");
            }
            // Stocked for it, with the money it costs, its workshop's record on but where the
            // recording had it off, and a pack with room for what it makes: one character here
            // makes what many did.
            if let Some(p) = h.server.players.get_mut(&world) {
                yalain_world::quals::set_int(&mut p.qualities, yalain_world::store::MONEY, 1 << 24);
            }
            h.server
                .players
                .get_mut(&world)
                .expect("in the world")
                .play
                .items
                .clear();
            let mut slots = Vec::new();
            let mut unknown = false;
            let mut wanted_of: BTreeMap<u32, u32> = BTreeMap::new();
            for (ordinal, _) in &c.slots {
                let Some(ing) = def.ingredients.iter().find(|i| i.ordinal == *ordinal) else {
                    unknown = true;
                    break;
                };
                let Some(t) = c
                    .templates
                    .get(ordinal)
                    .filter(|t| ing.items.contains(t))
                    .or_else(|| ing.items.first())
                    .copied()
                else {
                    unknown = true;
                    break;
                };
                // The slots of one template share a stack here, where the recording held an item for
                // each: it holds enough for them all.
                let need = tables
                    .required(ing, c.spinner)
                    .max(c.used.get(ordinal).copied().unwrap_or(0))
                    .max(1);
                let need = {
                    let wanted = wanted_of.entry(t).or_insert(0);
                    *wanted += need;
                    *wanted
                };
                let held = h.server.players[&world]
                    .play
                    .items
                    .iter()
                    .find(|i| i.template == t)
                    .map_or(0, |i| i.quantity.max(1));
                if held < need {
                    yalain_world::inventory::give_stacked(
                        &mut h.server,
                        &mut h.net,
                        world,
                        t,
                        need - held,
                    );
                }
                let id = h.server.players[&world]
                    .play
                    .items
                    .iter()
                    .find(|i| i.template == t)
                    .map(|i| i.id)
                    .expect("the stack");
                slots.push((*ordinal, id));
            }
            if unknown {
                count("a slot's item not known from the recording");
                continue;
            }
            h.run(1);
            // What is taken apart held an enchantment: the recording doesn't always say which.
            if matches!(
                def.custom,
                yalain_world::craft::Custom::Spellbind | yalain_world::craft::Custom::SpellbindAll
            ) {
                if let Some(t) = slots.first().and_then(|(_, i)| h.server.things.get_mut(*i)) {
                    t.enchantments = vec![(ENCHANTMENT, 1.0)];
                }
            }
            {
                let p = h.server.players.get_mut(&world).expect("in the world");
                p.play.recipes.retain(|r| r.recipe != c.recipe);
                p.play.recipes.push(craft::new_recipe(c.recipe));
                p.play.effects.retain(|e| e.eid != 0xFFFF_0000);
                if let (Some(effect), true) = (def.required_effect, answer != error::MISSING_EFFECT)
                {
                    p.play.effects.push(workshop_record(effect));
                }
                // Skilled enough that its check does not fail: what it uses up is what is compared.
                p.play.craft_skills.retain(|k| k.skill != def.skill);
                p.play
                    .craft_skills
                    .push(yalain_world::state::CraftSkillState {
                        skill: def.skill,
                        level: 1_000,
                        today: 0,
                        available: 0,
                    });
            }
            let before: BTreeMap<u32, u32> = slots
                .iter()
                .map(|(o, i)| {
                    let held = h.server.players[&world]
                        .play
                        .items
                        .iter()
                        .find(|x| x.id == *i)
                        .map_or(0, |x| x.quantity);
                    (*o, held)
                })
                .collect();
            let from = ask(&mut h, b, c.recipe, c.spinner, &slots);
            let until = h.now.0 + craft::timing(def.cycles).1 + 1.0;
            while h.now.0 < until && answered(&h, b, from).is_none() {
                h.step();
            }
            let ours = answered(&h, b, from);
            let same = match answer {
                0 | error::FAILED => matches!(ours, Some(0 | error::FAILED)),
                a => ours == Some(a),
            };
            if !same {
                wrong.push(format!("{c:x?}: answered {ours:?}"));
                continue;
            }
            count(&format!("answered as recorded: {answer}"));
            h.run(2);
            // What a recipe whose outcomes use up different things uses up is its roll's.
            let uses: BTreeSet<Vec<(u32, u32)>> = def
                .checks
                .iter()
                .flat_map(|k| k.outcomes.iter())
                .map(|o| {
                    let mut used: Vec<(u32, u32)> = o
                        .actions
                        .iter()
                        .filter_map(|a| tables.destroyed(a, c.spinner))
                        .collect();
                    used.sort_unstable();
                    used
                })
                .collect();
            if answer != 0 {
                continue;
            }
            if uses.len() > 1 {
                count("its outcomes use up different things: not compared");
                continue;
            }
            for (ordinal, recorded_used) in &c.used {
                let id = slots.iter().find(|(o, _)| o == ordinal).map(|(_, i)| *i);
                let left = h.server.players[&world]
                    .play
                    .items
                    .iter()
                    .find(|x| Some(x.id) == id)
                    .map_or(0, |x| x.quantity);
                let used = before.get(ordinal).copied().unwrap_or(0) - left;
                count("a slot's amount used compared");
                if used != *recorded_used {
                    wrong.push(format!(
                        "{c:x?}: slot {ordinal} used {used}, recorded {recorded_used}"
                    ));
                }
            }
        }
        println!("{tally:#?}");
        assert!(
            wrong.is_empty(),
            "{} of them off, the first: {:#?}",
            wrong.len(),
            &wrong[..wrong.len().min(10)]
        );
        assert!(tally["made or failed its check"] >= 950, "{tally:#?}");
        assert!(tally["a slot's amount used compared"] >= 300, "{tally:#?}");
        assert!(tally[&format!("answered as recorded: {}", error::INVALID_INGREDIENTS)] >= 10);
        assert!(tally[&format!("answered as recorded: {}", error::MISSING_EFFECT)] >= 4);
    }
}

/// What a hand shows of an item, held to the January 2017 recordings: every copy of an item in a
/// player's or a person's hand, and every move answered, named the entity the item's template is
/// drawn as and its appearance as it lies, as this server names them from the September 2013
/// install; and the training hall's digging tool, taken out of a corpse and given to a person,
/// shown in the hand as the recordings of it show.
mod hands {
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    use auberean_protocol::types::BehaviorParams;
    use auberean_protocol::{queue, ServerMessage};
    use yalain_content::{ContentObject, ContentPack, Kind};
    use yalain_testkit::authored::Portal;
    use yalain_testkit::play::{server_call, use_of};
    use yalain_testkit::{Harness, STEP, TEST_ITEM};
    use yalain_world::calls::{self, Node, Value};
    use yalain_world::inventory::{self, hand_look_of, move_graph, Move};
    use yalain_world::things::{Corpse, Thing};

    use super::super::keeping::open;

    const CREATE_OBJECT: i64 = 0x1_0058;
    const PLAY_BEHAVIOR: i64 = 0x1_007F;

    /// The training hall's digging tool, and the person it is handed to.
    const TOOL: u32 = 0x4700_1AF1;
    const SCOUT: u32 = 0x4700_2E03;

    /// What a hand shows of an item of each template, by the install (none for a template that does
    /// not describe).
    struct Looks {
        portal: Portal,
        by_template: BTreeMap<u32, Option<(u32, u32)>>,
    }

    impl Looks {
        fn new() -> Self {
            Self {
                portal: Portal::open(),
                by_template: BTreeMap::new(),
            }
        }

        fn of(&mut self, template: u32) -> Option<(u32, u32)> {
            let portal = &self.portal;
            *self.by_template.entry(template).or_insert_with(|| {
                portal
                    .described(template)
                    .map(|t| hand_look_of(Some(&t.props), t.description.look))
            })
        }
    }

    /// Every 2017 server message of the kinds asked, in each session's order: (session, raw).
    fn recorded(kinds: &str) -> Vec<(i64, Vec<u8>)> {
        let path = auberean_testkit::data::require_capture_index();
        let con = open(&path);
        let mut st = con
            .prepare(&format!(
                "SELECT m.session_id, m.raw FROM message m JOIN session s USING (session_id) \
                 JOIN capture c USING (capture_id) WHERE c.file LIKE '2017-%' AND m.dir = 's2c' \
                 AND m.status = 'ac2' AND ({kinds}) ORDER BY m.session_id, m.idx"
            ))
            .expect("the query prepares");
        let rows = st
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))
            .expect("the query runs");
        rows.flatten().collect()
    }

    /// The recorded behaviors toward an item with a copy of it in the hand: (session, the item, its
    /// parameters), with each session's objects' templates.
    type Copies = (Vec<(i64, u64, BehaviorParams)>, HashMap<(i64, u64), u32>);

    fn copies_in_hands() -> Copies {
        let mut templates = HashMap::new();
        let mut copies = Vec::new();
        let kinds =
            format!("m.mtype = {CREATE_OBJECT} OR (m.mtype = {PLAY_BEHAVIOR} AND m.len > 24)");
        for (session, raw) in recorded(&kinds) {
            match ServerMessage::decode(&raw) {
                Ok(ServerMessage::CreateObject(o)) => {
                    templates.insert((session, o.id), o.weenie.entity_did);
                }
                Ok(ServerMessage::PlayBehavior(e))
                    if e.action.bits & BehaviorParams::CLONE != 0 && e.action.target != 0 =>
                {
                    copies.push((session, e.action.target, e.action));
                }
                _ => {}
            }
        }
        (copies, templates)
    }

    /// Every copy of an item the 2017 servers put in a hand (taking, picking up, putting down, giving,
    /// handing back, using) named the entity the item's template is drawn as and its appearance as it
    /// lies, as this server reads them from the install for that template.
    ///
    /// Behaviour: server.hands.every-recorded-copy-is-the-item-as-its-template-draws-it
    #[test]
    fn every_recorded_copy_of_an_item_in_a_hand_is_the_item_as_its_template_draws_it() {
        let (copies, templates) = copies_in_hands();
        let mut looks = Looks::new();
        let (mut checked, mut kinds, mut missed) = (0, BTreeSet::new(), Vec::new());
        for (session, item, p) in &copies {
            let Some(template) = templates.get(&(*session, *item)) else {
                continue;
            };
            let Some(look) = looks.of(*template) else {
                continue;
            };
            checked += 1;
            kinds.insert(*template);
            if (p.clone_visual, p.clone_appearance) != look {
                missed.push((*template, p.clone_visual, p.clone_appearance, look));
            }
        }
        assert!(
            checked > 4000 && kinds.len() > 600,
            "copies of {} templates: {checked}",
            kinds.len()
        );
        assert!(missed.is_empty(), "named otherwise: {missed:x?}");
    }

    /// Every move the 2017 servers answered for an item (a pickup, a take, a put-down, a give, an item
    /// back, a reorganizing, a trade) named the entity the item's template is drawn as and its
    /// appearance as it lies, as this server reads them from the install.
    ///
    /// Behaviour: server.hands.every-recorded-answer-names-the-item-as-its-template-draws-it
    #[test]
    fn every_recorded_move_answer_names_the_item_as_its_template_draws_it() {
        let mut looks = Looks::new();
        let (mut checked, mut missed) = (0, Vec::new());
        // The move done, and the reorganization done.
        let kinds = "m.mtype = 65638 AND substr(m.raw, 5, 4) IN (x'5E000E82', x'55000E82')";
        for (_, raw) in recorded(kinds) {
            let Ok(ServerMessage::PlayerCall(c)) = ServerMessage::decode(&raw) else {
                continue;
            };
            let Some(s) = c
                .args
                .as_ref()
                .and_then(|a| calls::values(&a.0).ok())
                .and_then(|v| {
                    v.iter().find_map(|x: &Value| match x.root() {
                        Some(Node::Script { slots, .. }) => Some(slots.clone()),
                        _ => None,
                    })
                })
            else {
                continue;
            };
            let (template, look, appearance) = (s[22].1, s[38].1, s[6].1);
            let Some(given) = looks.of(template) else {
                continue;
            };
            checked += 1;
            if (look, appearance) != given {
                missed.push((template, look, appearance, given));
            }
        }
        assert!(checked > 10_000, "answers checked: {checked}");
        assert!(missed.is_empty(), "named otherwise: {missed:x?}");
    }

    fn steps(seconds: f64) -> usize {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = (seconds / STEP).ceil() as usize;
        n
    }

    /// What a client was told of a behavior toward an item: (behavior, bits, script, copy, its
    /// appearance).
    fn shown(p: &BehaviorParams) -> (u32, u32, u32, u32, u32) {
        (
            p.behavior,
            p.bits,
            p.fx_script,
            p.clone_visual,
            p.clone_appearance,
        )
    }

    /// The training hall's digging tool, taken out of a corpse and given to the person 2017's players
    /// gave it to, is shown in the hand as every 2017 recording of it shows: the taking behavior with its script,
    /// then the handing-over, each with a copy of the tool drawn as its template's entity, in its
    /// appearance as it lies. The install's own templates of the tool and the person; the person
    /// stands beside the player.
    ///
    /// Behaviour: server.hands.the-hall-tool-taken-and-given-as-recorded
    #[test]
    fn the_hall_tool_taken_from_a_corpse_and_given_is_shown_in_the_hand_as_recorded() {
        // What 2017 showed of the tool in a hand.
        let (copies, templates) = copies_in_hands();
        let recorded: BTreeSet<_> = copies
            .iter()
            .filter(|(session, item, _)| templates.get(&(*session, *item)) == Some(&TOOL))
            .map(|(_, _, p)| shown(p))
            .collect();
        let take = recorded
            .iter()
            .find(|r| r.0 == inventory::look::TAKING)
            .copied()
            .expect("a take of the tool recorded");
        let give = recorded
            .iter()
            .find(|r| r.0 == yalain_world::gives::HANDING)
            .copied()
            .expect("a give of the tool recorded");
        assert_eq!(recorded.len(), 2, "takes and gives alike: {recorded:x?}");

        let portal = Portal::open();
        let mut h = Harness::new();
        h.server.catalog = yalain_testkit::test_catalog();
        h.server.catalog.insert(portal.template(TOOL));
        let scout = portal.template(SCOUT);
        let person = 0x8130_0000_0000_0701;
        h.server.content = ContentPack {
            objects: vec![ContentObject {
                object: Some(person),
                template: SCOUT,
                description: scout.description.clone(),
                cell: 0x0299_0109,
                origin: [60.0, 241.0, -44.6],
                rotation: [1.0, 0.0, 0.0, 0.0],
                kind: Kind::Npc,
                label: String::new(),
                placed: "the install's person".into(),
                destination: None,
                facts: scout.facts,
                walks: None,
                gate: None,
            }],
            ..ContentPack::default()
        };
        let (b, me) = h.player_in_world("one", "Elara");
        h.bots[b].send(
            true,
            queue::WEENIE,
            &auberean_protocol::ClientMessage::SceneRenderingComplete(
                auberean_protocol::login::Empty,
            ),
        );
        h.run(5);
        let world = *h
            .server
            .players
            .iter()
            .find(|(_, p)| p.character == me)
            .map(|(c, _)| c)
            .expect("in the world");

        // The tool in a corpse beside the player, taken.
        let corpse_t = h
            .server
            .catalog
            .get(TEST_ITEM)
            .cloned()
            .expect("a template");
        let corpse = h.server.things.new_id();
        let mut c = Thing::of(corpse, &corpse_t, yalain_world::things::Kind::Corpse);
        c.at = Some(h.server.players[&world].position);
        let tool = inventory::make_item(&mut h.server, TOOL, 1, 0).expect("made");
        h.server.things.get_mut(tool).expect("the tool").container = Some(corpse);
        c.corpse = Some(Corpse {
            killer: me,
            contents: vec![tool],
            killer_level: 1,
            expires: 1.0e12,
            ..Default::default()
        });
        h.server.things.insert(c);
        let use_request =
            |id: u64| server_call(yalain_world::requests::function::USE, &[use_of(id, me)]);
        h.bots[b].send(true, queue::WEENIE, &use_request(corpse));
        h.run(5);
        let from = h.bots[b].inbox.len();
        h.bots[b].send(true, queue::WEENIE, &use_request(tool));
        h.run(5 + steps(inventory::TAKING_SECONDS + 0.5));
        let slot = h.server.players[&world]
            .play
            .items
            .iter()
            .find(|x| x.id == tool)
            .map(|x| i32::try_from(x.slot).unwrap_or(0))
            .expect("the tool taken");

        // Given to the person.
        let m = Move {
            move_type: yalain_world::gives::GIVE,
            merge: true,
            unlock: true,
            check_take: true,
            all_taken: true,
            grab: true,
            contain: true,
            auto_merge: true,
            target: person,
            actual_target: person,
            merge_container: person,
            from: me,
            actual_from: me,
            item: tool,
            split_item: tool,
            quantity: 1,
            quantity_left: 1,
            from_slot: slot,
            actual_from_slot: slot,
            ..Move::default()
        };
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(yalain_world::drops::function::MOVE_ITEM, &[move_graph(&m)]),
        );
        h.run(steps(yalain_world::gives::HANDED_AFTER) + 8);
        let ours: Vec<_> = h
            .world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::PlayBehavior(e) if e.sender.id == me && e.action.target == tool => {
                    Some(shown(&e.action))
                }
                _ => None,
            })
            .collect();
        assert_eq!(ours, vec![take, give], "the take, then the give");
    }
}

/// The pushes of January 2017 held to the impulse this server sends: every recorded knockback of
/// the monsters' "Impulse (Knockback)" attack and every recorded Ground Tremor goes as
/// `impulse_velocity` sends it for their effects' ways, and no recorded push goes faster than the
/// top speed.
mod impulses {
    use std::collections::HashMap;

    use auberean_protocol::ServerMessage;
    use auberean_testkit::captures;
    use yalain_store::StoredPosition;
    use yalain_world::effect_defs::impulse_way as way;
    use yalain_world::effects::{impulse_velocity, IMPULSE, IMPULSE_TOP_SPEED};

    const POSITION: u32 = 0x1_0086;
    const POSITION_CELL: u32 = 0x1_00C1;
    const STORY: u32 = 0x1_00A8;
    /// The monsters' knockback attack, whose effect (`0x700006C6`) pushes away from its caster.
    const KNOCKBACK: u32 = 981;
    /// Ground Tremor, whose effect (`0x6F000167`) pushes straight up.
    const GROUND_TREMOR: u32 = 1426;

    /// Every 2017 push named by the attack that made it: the attack's skill, the push, where the
    /// target and the attacker stood, and the target's heading.
    struct Push {
        skill: u32,
        velocity: [f32; 3],
        target: StoredPosition,
        attacker: StoredPosition,
        heading: u32,
    }

    fn pushes() -> (Vec<Push>, Vec<[f32; 3]>) {
        let index = auberean_testkit::data::require_capture_index();
        let mut named = Vec::new();
        let mut every = Vec::new();
        let mut session = -1;
        let mut at: HashMap<u64, (StoredPosition, u32)> = HashMap::new();
        let mut struck: HashMap<u64, (f64, u64, u32)> = HashMap::new();
        captures::messages(&index, &[POSITION, POSITION_CELL, STORY], &[2017], |m| {
            if m.session != session {
                session = m.session;
                at.clear();
                struck.clear();
            }
            match ServerMessage::decode(&m.raw) {
                Ok(ServerMessage::Position(p) | ServerMessage::PositionCell(p)) => {
                    let place = StoredPosition {
                        cell: p.cell,
                        origin: [p.origin.x, p.origin.y, p.origin.z],
                        rotation: [1.0, 0.0, 0.0, 0.0],
                    };
                    if p.flags & IMPULSE != 0 {
                        let v = [p.velocity.x, p.velocity.y, p.velocity.z];
                        every.push(v);
                        if let Some(&(t, attacker, skill)) = struck.get(&p.sender.id) {
                            if m.time - t < 2.0 {
                                if let Some((from, _)) = at.get(&attacker) {
                                    named.push(Push {
                                        skill,
                                        velocity: v,
                                        target: place,
                                        attacker: *from,
                                        heading: p.heading_motion >> 24,
                                    });
                                }
                            }
                        }
                    }
                    at.insert(p.sender.id, (place, p.heading_motion >> 24));
                }
                Ok(ServerMessage::DoStory(e)) => {
                    for hook in &e.action.hooks {
                        if hook.target != 0 {
                            struck.insert(hook.target, (m.time, e.action.attacker, e.action.skill));
                        }
                    }
                }
                _ => {}
            }
        })
        .expect("the capture index reads");
        (named, every)
    }

    fn length(v: [f32; 3]) -> f32 {
        (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
    }

    /// Each recorded push of the monsters' knockback goes away from its attacker along their offset in
    /// three dimensions (within a few tenths of a degree), at its speed (the strongest at 50 m/s), and each recorded Ground Tremor
    /// straight up at 10, as this server sends them; no recorded push of any kind goes faster than
    /// 50 m/s.
    /// Behaviour: server.effects.impulse-matches-2017
    #[test]
    fn every_recorded_knockback_and_ground_tremor_goes_as_this_server_sends_it() {
        let (named, every) = pushes();
        let mut held = [0usize; 2];
        for p in &named {
            let speed = length(p.velocity);
            #[allow(clippy::cast_precision_loss)]
            let facing = p.heading as f32 * 360.0 / 255.0;
            let (ways, n) = match p.skill {
                KNOCKBACK => (way::FROM_CASTER, 0),
                GROUND_TREMOR => (way::UP, 1),
                _ => continue,
            };
            let (dx, dy, dz) = yalain_world::ai::offset(&p.attacker, &p.target);
            let want = impulse_velocity(ways, speed, facing, Some([dx, dy, dz])).expect("a push");
            // The attacker's place is the last its watchers were told, a moment old: the two agree in
            // direction within a few tenths of a degree, and in speed.
            let cos = want.iter().zip(p.velocity).map(|(a, b)| a * b).sum::<f32>()
                / (length(want) * speed);
            assert!(
                cos > 0.999 && (length(want) - speed).abs() < 0.01,
                "skill {}: sent {want:?}, recorded {:?}",
                p.skill,
                p.velocity
            );
            held[n] += 1;
        }
        assert!(held[0] >= 250, "knockbacks held: {}", held[0]);
        assert!(held[1] >= 40, "ground tremors held: {}", held[1]);
        assert!(every.len() > 1_700, "pushes recorded: {}", every.len());
        assert!(
            every.iter().all(|v| length(*v) <= IMPULSE_TOP_SPEED + 0.01),
            "none faster than the top speed"
        );
        assert!(
            every
                .iter()
                .filter(|v| (length(**v) - IMPULSE_TOP_SPEED).abs() < 0.01)
                .count()
                > 150,
            "the strongest at the top speed"
        );
    }
}
