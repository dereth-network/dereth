//! ACE: Source/ACE.Server/Command/Handlers/AdminCommands.cs::HandleSave
//! Admin chat commands: teleport to player/location, create items/objects, smite leaves a corpse,
//! heal refuses creatures, under-privileged refused silently, shutdown countdown and cancel on
//! virtual time.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::sync::Arc;

use dereth_assets::tables::SkillFormula;
use dereth_primitives::NetQueue;
use dereth_primitives::{DataId, ObjectId};
use dereth_protocol::comms::{CommunicationTalk, CommunicationTextboxString};
use dereth_protocol::login::{
    CharacterLoginCompleteNotification, LoginCharacterSet, LoginSendEnterWorld,
    LoginSendEnterWorldRequest,
};
use dereth_protocol::objects::{EffectsPlayerTeleport, ItemCreateObject};
use empyrean_command::command_manager;
use empyrean_common::dotnet::{DotNetDateTime, DotNetDict};
use empyrean_content::models::world::{PointsOfInterest, Weenie};
use empyrean_content::MemContent;
use empyrean_dat::file_types::SecondaryAttributeTable;
use empyrean_dat::{file_id, FakeDats};
use empyrean_entity::enums::{
    ChatMessageType, PositionType, PropertyAttribute, PropertyAttribute2nd, PropertyBool,
    PropertyDataId, PropertyInt, PropertyString, WeenieType,
};
use empyrean_entity::models::{PropertiesAttribute, PropertiesAttribute2nd};
use empyrean_entity::{LandblockId, ObjectGuid, Position};
use empyrean_testkit::{land, ClientId, TestServer};
use empyrean_world::dispatch::Class;
use empyrean_world::entity::damage_history::DamageHistory;
use empyrean_world::managers::guid_manager;
use empyrean_world::managers::{landblock_manager as lm, player_manager};
use empyrean_world::world_objects::entity::creature_attribute::{CreatureAttribute, StatCtx};
use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::world_objects::{container, world_object_tick};

const HOME: u16 = 0xA9B4;
const ALPHA: u32 = 0x5000_0001;
const BRAVO: u32 = 0x5000_0002;
const MONSTER: ObjectGuid = ObjectGuid::new(0x8000_0100);
const GEM_WCID: u32 = 3001;
const PORTAL_WCID: u32 = 3002;
const CORPSE_WCID: u32 = 21;
const STATUE_WCID: u32 = 3003;

fn outdoor(x: f32, y: f32, z: f32) -> Position {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cell = (x / 24.0) as u32 * 8 + (y / 24.0) as u32 + 1;
    Position::from_components(
        u32::from(HOME) << 16 | cell,
        x,
        y,
        z,
        0.0,
        0.0,
        0.0,
        1.0,
        false,
    )
}

fn dats() -> Arc<empyrean_dat::DatManager> {
    let f = |attr1: u32, z: u32| SkillFormula {
        w: 0,
        x: 1,
        y: 0,
        z,
        attr1,
        attr2: 0,
    };
    let table = SecondaryAttributeTable {
        id: DataId(file_id::SECONDARY_ATTRIBUTE_TABLE),
        health: f(2, 2),
        stamina: f(2, 1),
        mana: f(6, 1),
    };
    empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_portal(file_id::SECONDARY_ATTRIBUTE_TABLE, table)
        .build()
        .expect("fake dats")
}

fn content() -> MemContent {
    MemContent::new()
        .weenie(Weenie::new(1, "human", WeenieType::Creature).with_did(
            empyrean_entity::enums::PropertyDataId::CombatTable,
            0x3000_0000,
        ))
        .weenie(
            Weenie::new(CORPSE_WCID, "corpse", WeenieType::Corpse)
                .with_string(PropertyString::Name, "Corpse")
                .with_did(PropertyDataId::Setup, land::TEST_SETUP),
        )
        .weenie(
            Weenie::new(GEM_WCID, "u62gem", WeenieType::Generic)
                .with_string(PropertyString::Name, "Test Gem")
                .with_did(PropertyDataId::Setup, land::TEST_SETUP)
                .with_int(PropertyInt::EncumbranceVal, 5),
        )
        .weenie(
            Weenie::new(STATUE_WCID, "u62statue", WeenieType::Generic)
                .with_string(PropertyString::Name, "Statue")
                .with_bool(PropertyBool::Stuck, true),
        )
        .weenie(
            Weenie::new(PORTAL_WCID, "u62portal", WeenieType::Portal)
                .with_string(PropertyString::Name, "Market Portal")
                .with_position(
                    PositionType::Destination,
                    u32::from(HOME) << 16 | 0x0020,
                    [60.0, 170.0, 20.0],
                    [1.0, 0.0, 0.0, 0.0],
                ),
        )
        .point_of_interest(PointsOfInterest {
            id: 1,
            name: "Market".to_owned(),
            weenie_class_id: PORTAL_WCID,
            last_modified: DotNetDateTime::UNIX_EPOCH,
        })
}

fn seed(ts: &TestServer, account: &str, guid: u32, name: &str, at: Position) {
    crate::support::command_characters::seed(ts, account, guid, name, at, false);
}

/// The server with "Alpha Admin" and "Bravo Player" in the shard, and the command table.
fn server() -> TestServer {
    let mut ts = TestServer::with_dats(dats());
    land::use_flat_land_with_test_setup(&mut ts.world, &[HOME], 10);
    ts.world.content = Arc::new(content());
    guid_manager::initialize(&mut ts.world, &mut EmptyShard);
    seed(
        &ts,
        "u62alpha",
        ALPHA,
        "Alpha Admin",
        outdoor(100.0, 100.0, 20.0),
    );
    seed(
        &ts,
        "u62bravo",
        BRAVO,
        "Bravo Player",
        outdoor(150.0, 60.0, 20.0),
    );
    player_manager::initialize(&mut ts.world);
    command_manager::initialize(None);
    ts
}

/// Logs `account` in and enters the world with `guid`, out of the login pink bubble.
fn enter(ts: &mut TestServer, account: &str, guid: u32) -> ClientId {
    let id = ts.connect(account, "pw");
    assert!(
        ts.run_until(1.0, |ts| !ts.received::<LoginCharacterSet>(id).is_empty()),
        "the character list arrives"
    );
    ts.send_message(id, NetQueue::Logon, &LoginSendEnterWorldRequest);
    ts.advance(0.1);
    ts.send_message(
        id,
        NetQueue::Logon,
        &LoginSendEnterWorld {
            character: ObjectId(guid),
            account: account.to_owned(),
        },
    );
    let g = ObjectGuid::new(guid);
    assert!(
        ts.run_until(1.0, |ts| ts
            .world
            .objects
            .get(g)
            .is_some_and(|o| o.current_landblock.is_some())),
        "{account} enters the world"
    );
    ts.send_game_action(id, &CharacterLoginCompleteNotification);
    ts.advance(0.1);
    id
}

/// The server with Alpha (an admin) and Bravo (a player) in the world.
fn two_players() -> (TestServer, ClientId, ClientId) {
    let mut ts = server();
    let alpha = enter(&mut ts, "u62alpha", ALPHA);
    let bravo = enter(&mut ts, "u62bravo", BRAVO);
    ts.world
        .objects
        .get_mut(ObjectGuid::new(ALPHA))
        .unwrap()
        .set_is_admin_prop(true);
    let _ = TestServer::take_not_ported();
    (ts, alpha, bravo)
}

/// Types `line` in chat and returns the chat lines the client received for it, with their types.
fn say(ts: &mut TestServer, id: ClientId, line: &str) -> Vec<(String, u32)> {
    let before = ts.received::<CommunicationTextboxString>(id).len();
    ts.send_game_action(
        id,
        &CommunicationTalk {
            message: line.to_owned(),
        },
    );
    ts.advance(0.5);
    ts.received::<CommunicationTextboxString>(id)[before..]
        .iter()
        .map(|m| (m.text.clone(), m.text_type))
        .collect()
}

/// The chat lines of one type the client has received since `mark`.
fn chat_since(ts: &TestServer, id: ClientId, mark: usize, t: ChatMessageType) -> Vec<String> {
    ts.received::<CommunicationTextboxString>(id)[mark..]
        .iter()
        .filter(|m| m.text_type == t.0)
        .map(|m| m.text.clone())
        .collect()
}

fn broadcast(text: &str) -> (String, u32) {
    (text.to_owned(), ChatMessageType::Broadcast.0)
}

fn location(ts: &TestServer, guid: u32) -> Position {
    ts.world
        .objects
        .get(ObjectGuid::new(guid))
        .and_then(WorldObject::location)
        .expect("a location")
}

#[test]
fn an_admin_teleports_to_a_player_and_to_a_named_location() {
    let (mut ts, alpha, _) = two_players();

    assert_eq!(
        say(&mut ts, alpha, "@teleto Nobody Here"),
        [broadcast("Player Nobody Here was not found.")]
    );

    // teleto: Player.Teleport to Bravo's location (portal space, then the position update)
    let bravo_at = location(&ts, BRAVO);
    assert!(say(&mut ts, alpha, "@teleto bravo player").is_empty());
    assert_eq!(
        ts.received::<EffectsPlayerTeleport>(alpha).len(),
        1,
        "PlayerTeleport"
    );
    let l = location(&ts, ALPHA);
    assert_eq!(
        (l.cell(), l.position_x, l.position_y),
        (bravo_at.cell(), bravo_at.position_x, bravo_at.position_y)
    );
    assert!(
        (l.position_z - (bravo_at.position_z + 0.005)).abs() < 1e-4,
        "Teleport lifts by 0.005: {}",
        l.position_z
    );
    assert!(
        ts.world
            .objects
            .get(ObjectGuid::new(ALPHA))
            .unwrap()
            .wo
            .world_object
            .teleporting
    );
    ts.send_game_action(alpha, &CharacterLoginCompleteNotification);
    ts.advance(0.1);

    // telepoi: the POI's portal weenie's Destination
    assert_eq!(
        say(&mut ts, alpha, "@telepoi Nowhere"),
        [broadcast(
            "Location: \"Nowhere\" not found. Use \"list\" to display all valid locations."
        )]
    );
    // V360/V362 (a fix): the failed lookup left an empty entry under its name in the cache, which
    // the list leaves out (ACE's listed it: "market, nowhere")
    assert_eq!(
        say(&mut ts, alpha, "@telepoi list"),
        [broadcast("All POIs: market")]
    );
    assert!(say(&mut ts, alpha, "@telepoi MARKET").is_empty());
    assert_eq!(ts.received::<EffectsPlayerTeleport>(alpha).len(), 2);
    let l = location(&ts, ALPHA);
    assert_eq!(
        (l.cell() >> 16, l.position_x, l.position_y),
        (u32::from(HOME), 60.0, 170.0)
    );
}

#[test]
fn an_admin_creates_an_item_in_the_pack_and_objects_in_the_world() {
    let (mut ts, alpha, _) = two_players();
    let me = ObjectGuid::new(ALPHA);

    assert_eq!(
        say(&mut ts, alpha, "@ci nosuch"),
        [broadcast("nosuch is not a valid weenie.")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@ci u62gem 0"),
        [broadcast("stacksize must be number between 1 - 65535")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@ci u62statue"),
        [broadcast(
            "You cannot spawn u62statue in your inventory because it cannot be picked up"
        )]
    );
    assert_eq!(
        say(&mut ts, alpha, "@ci 3003"),
        [broadcast(
            "You cannot spawn u62statue in your inventory because it cannot be picked up"
        )]
    );

    let before = container::inventory(ts.world.objects.get(me).unwrap()).len();
    let creates = ts.received::<ItemCreateObject>(alpha).len();
    assert!(say(&mut ts, alpha, "@ci u62gem").is_empty());
    let pack: Vec<ObjectGuid> = container::inventory(ts.world.objects.get(me).unwrap())
        .keys()
        .copied()
        .collect();
    assert_eq!(pack.len(), before + 1, "one item in the pack");
    let gem = *pack.last().unwrap();
    let o = ts.world.objects.get(gem).unwrap();
    assert_eq!(o.biota.weenie_class_id, GEM_WCID);
    assert_eq!(o.container_id(), Some(ALPHA));
    assert!(o.location().is_none(), "in the pack, not on the ground");
    assert!(
        ts.received::<ItemCreateObject>(alpha)[creates..]
            .iter()
            .any(|c| c.0.id.0 == gem.full()),
        "the client is told"
    );

    // create: two objects in front of the admin (UseRadius 2 m), in the admin's landblock
    assert_eq!(
        say(&mut ts, alpha, "@create u62gem x"),
        [broadcast(
            "Amount to spawn must be a number between -2147483648 - 2147483647."
        )]
    );
    assert_eq!(
        say(&mut ts, alpha, "@create u62gem 0"),
        [broadcast("No object was created.")]
    );
    let lb = ts.world.objects.get(me).unwrap().current_landblock.unwrap();
    assert!(say(&mut ts, alpha, "@create u62gem 2").is_empty());
    let spawned: Vec<ObjectGuid> = ts
        .world
        .landblock_manager
        .landblocks
        .get(lb)
        .expect("loaded")
        .get_all_world_objects_for_diagnostics()
        .into_iter()
        .filter(|&g| {
            ts.world
                .objects
                .get(g)
                .is_some_and(|o| o.biota.weenie_class_id == GEM_WCID)
        })
        .collect();
    assert_eq!(spawned.len(), 2);
    let here = location(&ts, ALPHA);
    let mut ys: Vec<f32> = spawned
        .iter()
        .map(|&g| {
            let l = ts.world.objects.get(g).unwrap().location().unwrap();
            assert_eq!(
                l.position_x, here.position_x,
                "straight ahead (facing north)"
            );
            l.position_y
        })
        .collect();
    ys.sort_by(f32::total_cmp);
    // both are asked for 2 m ahead (`Math.Max(2, UseRadius ?? 2)`); the physics placement puts the
    // second beside the first
    assert_eq!(ys[0], here.position_y + 2.0);
    assert!(ys[1] > ys[0], "{ys:?}");
}

/// A creature as its constructor leaves it (100 in each vital), standing beside `(x, y)`.
fn monster(ts: &mut TestServer, x: f32, y: f32) {
    let w = &mut ts.world;
    let mut o = WorldObject::allocate(Class::Creature);
    o.guid = MONSTER;
    o.biota.id = MONSTER.full();
    o.biota.properties_enchantment_registry = Some(Vec::new());
    let vitals = [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ];
    let mut v2 = DotNetDict::new();
    for v in vitals {
        v2.insert(
            v,
            PropertiesAttribute2nd {
                init_level: 100,
                level_from_cp: 0,
                cp_spent: 0,
                current_level: 100,
            },
        );
    }
    o.biota.properties_attribute_2nd = Some(v2);
    for v in vitals {
        let cv = CreatureVital::new(&mut o, v);
        o.vitals_mut().insert(v, cv);
    }
    let attributes = [
        PropertyAttribute::Strength,
        PropertyAttribute::Endurance,
        PropertyAttribute::Coordination,
        PropertyAttribute::Quickness,
        PropertyAttribute::Focus,
        PropertyAttribute::Self_,
    ];
    let mut a1 = DotNetDict::new();
    for a in attributes {
        a1.insert(
            a,
            PropertiesAttribute {
                init_level: 60,
                level_from_cp: 0,
                cp_spent: 0,
            },
        );
    }
    o.biota.properties_attribute = Some(a1);
    for a in attributes {
        let ca = CreatureAttribute::new(&mut o, a);
        o.attributes_mut().insert(a, ca);
    }
    o.set_property(PropertyString::Name, "Drudge".to_owned());
    o.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    o.set_property(PropertyBool::Attackable, true);
    o.creature.as_mut().unwrap().creature_death.damage_history =
        DamageHistory::new(MONSTER, w.now.utc);
    o.set_location(Some(outdoor(x, y, 20.0)));
    o.set_heartbeat_interval(Some(0.0));
    world_object_tick::world_object_initialize_heartbeats(&mut o, w.now.unix_time);
    w.objects.insert(o).expect("fresh guid");
    assert!(
        lm::add_object(w, MONSTER, false),
        "the monster joins its landblock"
    );
}

#[test]
fn an_admin_smites_a_monster_which_dies_and_leaves_a_corpse() {
    let (mut ts, alpha, _) = two_players();
    monster(&mut ts, 104.0, 100.0);
    let me = ObjectGuid::new(ALPHA);

    assert_eq!(say(&mut ts, alpha, "@smite"), [broadcast(
        "Select a target and use @smite, or use @smite all to kill all creatures in radar range or @smite [players' name]."
    )]);

    // the health bar selection (HealthQueryTarget) names the monster
    ts.world
        .objects
        .get_mut(me)
        .unwrap()
        .set_health_query_target(Some(MONSTER.full()));
    let _ = say(&mut ts, alpha, "@smite");
    ts.advance(1.0);

    assert!(
        ts.world.objects.get(MONSTER).is_none(),
        "the monster is destroyed"
    );
    let lb = LandblockId::new(u32::from(HOME) << 16 | 0xFFFF);
    let corpse = ts
        .world
        .landblock_manager
        .landblocks
        .get(lb)
        .expect("loaded")
        .get_all_world_objects_for_diagnostics()
        .into_iter()
        .find(|&g| ts.world.objects.get(g).is_some_and(WorldObject::is_corpse))
        .expect("a corpse on the landblock");
    let c = ts.world.objects.get(corpse).unwrap();
    assert_eq!(
        c.get_property(PropertyString::Name).as_deref(),
        Some("Corpse of Drudge")
    );
    let l = c.location().unwrap();
    assert_eq!(
        (l.position_x, l.position_y),
        (104.0, 100.0),
        "where it died"
    );
}

#[test]
fn heal_restores_the_admins_vitals_and_refuses_a_creature() {
    let (mut ts, alpha, _) = two_players();
    monster(&mut ts, 104.0, 100.0);
    let me = ObjectGuid::new(ALPHA);

    let health = ts.world.objects.get(me).unwrap().health();
    let max = health.max_value(&mut StatCtx::in_world(&mut ts.world, me));
    {
        let o = ts.world.objects.get_mut(me).unwrap();
        health.set_current(o, 7);
    }
    assert!(max > 7);
    assert!(say(&mut ts, alpha, "@heal").is_empty());
    let o = ts.world.objects.get(me).unwrap();
    assert_eq!(health.current(o), max, "healed to full");

    ts.world
        .objects
        .get_mut(me)
        .unwrap()
        .set_health_query_target(Some(MONSTER.full()));
    assert_eq!(
        say(&mut ts, alpha, "@heal"),
        [broadcast(
            "You cannot heal Drudge because it is not a player."
        )]
    );
    ts.world
        .objects
        .get_mut(me)
        .unwrap()
        .set_health_query_target(Some(0x8000_0FFF));
    assert_eq!(
        say(&mut ts, alpha, "@heal"),
        [broadcast("Unable to locate what you have selected.")]
    );
}

#[test]
fn an_under_privileged_player_is_refused_silently() {
    let (mut ts, _, bravo) = two_players();
    monster(&mut ts, 150.0, 64.0);
    let them = ObjectGuid::new(BRAVO);
    let at = location(&ts, BRAVO);
    let pack = container::inventory(ts.world.objects.get(them).unwrap()).len();
    ts.world
        .objects
        .get_mut(them)
        .unwrap()
        .set_health_query_target(Some(MONSTER.full()));

    // NotAuthorized: no reply, and the handler does not run
    for line in [
        "@teleto Alpha Admin",
        "@ci u62gem",
        "@smite",
        "@heal",
        "@create u62gem",
        "@shutdown",
        "@telepoi market",
        "@myiid",
    ] {
        assert_eq!(say(&mut ts, bravo, line), [], "{line}");
    }
    ts.advance(1.0);
    assert_eq!(location(&ts, BRAVO).cell(), at.cell());
    assert!(ts.received::<EffectsPlayerTeleport>(bravo).is_empty());
    assert_eq!(
        container::inventory(ts.world.objects.get(them).unwrap()).len(),
        pack
    );
    assert!(ts.world.objects.get(MONSTER).is_some(), "not smitten");
    assert!(!ts.world.server_manager.shutdown_initiated);

    // an Envoy may smite and heal, not create or shut down
    ts.world.objects.get_mut(them).unwrap().set_is_envoy(true);
    assert_eq!(
        say(&mut ts, bravo, "@myiid"),
        [broadcast(
            "GUID: 1342177282  - Low: 2 - High: 80 - (0x50000002)"
        )]
    );
    assert_eq!(say(&mut ts, bravo, "@ci u62gem"), []);
    assert_eq!(say(&mut ts, bravo, "@shutdown"), []);
    assert!(!ts.world.server_manager.shutdown_initiated);
}

#[test]
fn a_shutdown_with_a_delay_counts_down_in_virtual_time() {
    let (mut ts, alpha, bravo) = two_players();

    assert_eq!(
        say(&mut ts, alpha, "@set-shutdown-interval 90"),
        [broadcast(
            "Shutdown Interval (seconds to shutdown server) has been set to 90."
        )]
    );
    let (mark_a, mark_b) = (
        ts.received::<CommunicationTextboxString>(alpha).len(),
        ts.received::<CommunicationTextboxString>(bravo).len(),
    );
    let _ = say(&mut ts, alpha, "@shutdown");
    assert!(ts.world.server_manager.shutdown_initiated);
    let started = "Broadcast from System> WARNING - This Asheron's Call Server will be shutting down in 1 minute and 30 seconds. Please log out.";
    assert_eq!(
        chat_since(&ts, bravo, mark_b, ChatMessageType::WorldBroadcast),
        [started]
    );
    assert_eq!(
        say(&mut ts, alpha, "@shutdown"),
        [broadcast("Shutdown is already in progress.")]
    );

    // ServerManager's countdown: a notice at each listed time left, at least 2 s apart
    ts.advance(83.0);
    let notices = chat_since(&ts, bravo, mark_b, ChatMessageType::WorldBroadcast);
    assert_eq!(notices, [
        started,
        "Broadcast from System> WARNING - This Asheron's Call Server will be shutting down in 1 minute. Please log out.",
        "Broadcast from System> WARNING - This Asheron's Call Server will be shutting down in 30 seconds! Please log out!",
        "Broadcast from System> WARNING - This Asheron's Call Server will be shutting down in 15 seconds! Please log out!",
        "Broadcast from System> WARNING - This Asheron's Call Server will be shutting down in 10 seconds! Please log out!",
    ]);
    assert_eq!(
        chat_since(&ts, alpha, mark_a, ChatMessageType::WorldBroadcast)[..5],
        notices[..]
    );
    assert_eq!(player_manager::get_online_count(&ts.world), 2);

    // at the shutdown time the players are logged off, and the sequence runs to its exit
    assert!(
        ts.run_until(120.0, |ts| ts.exited()),
        "the shutdown sequence exits"
    );
    assert_eq!(player_manager::get_online_count(&ts.world), 0);
    let last = chat_since(&ts, bravo, mark_b, ChatMessageType::WorldBroadcast);
    assert_eq!(last.last().map(String::as_str), Some("Broadcast from System> ATTENTION - This Asheron's Call Server is shutting down NOW!!!!"));
}

#[test]
fn a_cancelled_shutdown_says_so_and_stops_the_countdown() {
    let (mut ts, alpha, bravo) = two_players();

    assert_eq!(
        say(&mut ts, alpha, "@set-shutdown-interval 600"),
        [broadcast(
            "Shutdown Interval (seconds to shutdown server) has been set to 600."
        )]
    );
    let mark_b = ts.received::<CommunicationTextboxString>(bravo).len();
    let _ = say(&mut ts, alpha, "@shutdown Patch day");
    assert_eq!(chat_since(&ts, bravo, mark_b, ChatMessageType::WorldBroadcast), [
        "Broadcast from Alpha Admin> Patch day\nBroadcast from Alpha Admin> ATTENTION - This Asheron's Call Server will be shutting down in 10 minutes."
    ]);
    ts.advance(5.0);
    let _ = say(&mut ts, alpha, "@cancel-shutdown");
    assert!(!ts.world.server_manager.shutdown_initiated);
    ts.advance(700.0);
    assert!(!ts.exited());
    assert_eq!(player_manager::get_online_count(&ts.world), 2);
    assert_eq!(chat_since(&ts, bravo, mark_b, ChatMessageType::WorldBroadcast)[1..], [
        "Broadcast from System> ATTENTION - This Asheron's Call Server shut down has been cancelled."
    ]);
}

pub(crate) use crate::support::empty_shard::EmptyShard;
