//! ACE: Source/ACE.Server/WorldObjects/Player.cs::FinalizeLogout
//! Every change survives log off/on on MemShard and SqliteShard; 5-minute save on virtual clock;
//! dropped session still saves; soak relog check takes the last state.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::collections::BTreeSet;
use std::sync::Arc;

use dereth_primitives::NetQueue;
use dereth_primitives::ObjectId;
use dereth_protocol::login::{
    CharacterLoginCompleteNotification, LoginExecuteLogOff, LoginExecuteLogOffRequest,
    LoginSendEnterWorld, LoginSendEnterWorldRequest,
};
use empyrean_common::clock::VirtualClock;
use empyrean_content::models::world::Weenie;
use empyrean_content::MemContent;
use empyrean_dat::fake::sample;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    AccessLevel, PositionType, PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt,
    PropertyString, Skill, SkillAdvancementClass, WeenieType,
};
use empyrean_entity::{Biota, ObjectGuid};
use empyrean_net::enums::SessionTerminationReason;
use empyrean_net::{SessionId, SessionState};
use empyrean_store::adapter::biota_converter::BiotaConverter;
use empyrean_store::{ShardHandle, SqliteShard};
use empyrean_testkit::land;
use empyrean_testkit::{ClientId, TestServer};
use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
use empyrean_world::managers::player_manager;
use empyrean_world::managers::world_manager::WorldStatusState;
use empyrean_world::physics::phys_ext;
use empyrean_world::World;

const ACCOUNT: &str = "acct";
const PLAYER: u32 = 0x5000_0001;
const PLAYER_SETUP: u32 = 0x0200_0001;
const START: u16 = 0xA9B4;
const HEIGHT: u8 = 47;

/// The seeded possessions: A in the main pack, P a side pack holding C, W worn.
const A: u32 = 0x8000_0010;
const P: u32 = 0x8000_0011;
const C: u32 = 0x8000_0012;
const W: u32 = 0x8000_0014;

/// Which shard backend the server runs on.
#[derive(Debug, Clone, Copy)]
enum Backend {
    Mem,
    Sqlite,
}

struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }
    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

fn dats() -> Arc<empyrean_dat::DatManager> {
    empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_skill_table(sample::skill_table())
        .build()
        .expect("fake dats")
}

/// The player weenie heartbeats every 5 s (so `Player.Heartbeat` runs); items are plain objects.
fn content() -> MemContent {
    let item = |wcid: u32, name: &str, weenie_type: WeenieType| {
        Weenie::new(wcid, name, weenie_type)
            .with_string(PropertyString::Name, name)
            .with_did(PropertyDataId::Setup, land::TEST_SETUP)
            .with_int(PropertyInt::ItemType, 0x80)
    };
    MemContent::new()
        .weenie(
            Weenie::new(1, "human", WeenieType::Creature)
                .with_did(
                    empyrean_entity::enums::PropertyDataId::CombatTable,
                    0x3000_0000,
                )
                .with_int(PropertyInt::ItemsCapacity, 102)
                .with_int(PropertyInt::ContainersCapacity, 7)
                .with_float(PropertyFloat::HeartbeatInterval, 5.0),
        )
        .weenie(item(50, "Thing", WeenieType::Generic))
        .weenie(item(51, "Pack", WeenieType::Container).with_int(PropertyInt::ItemsCapacity, 24))
        .weenie(item(52, "Gem", WeenieType::Generic))
}

fn blocks() -> Vec<u16> {
    let mut v = Vec::new();
    for dx in [-1i32, 0, 1] {
        for dy in [-1i32, 0, 1] {
            let x = u16::try_from(i32::from(START >> 8) + dx).expect("on the map");
            let y = u16::try_from(i32::from(START & 0xFF) + dy).expect("on the map");
            v.push(x << 8 | y);
        }
    }
    v
}

fn pos(cell: u32, x: f32, y: f32, z: f32) -> empyrean_entity::models::PropertiesPosition {
    empyrean_entity::models::PropertiesPosition {
        obj_cell_id: cell,
        position_x: x,
        position_y: y,
        position_z: z,
        rotation_w: 1.0,
        ..Default::default()
    }
}

fn item(
    guid: u32,
    wcid: u32,
    weenie_type: WeenieType,
    container: Option<u32>,
    wielder: Option<u32>,
) -> Biota {
    let mut b = Biota {
        id: guid,
        weenie_class_id: wcid,
        weenie_type,
        ..Default::default()
    };
    b.set_property(PropertyString::Name, format!("Item {guid:08X}"));
    b.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    b.set_property(PropertyInt::ItemType, 0x80);
    b.set_property(PropertyInt::EncumbranceVal, 10);
    if let Some(c) = container {
        b.set_property(PropertyInstanceId::Container, c);
    }
    if let Some(wi) = wielder {
        b.set_property(PropertyInstanceId::Wielder, wi);
        b.set_property(PropertyInt::CurrentWieldedLocation, 0x0000_0002); // ChestWear
        b.set_property(PropertyInt::ValidLocations, 0x0000_0002);
    }
    b
}

/// A unique temp file for one SQLite shard (removed by the test).
fn temp_db(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("u411-{tag}-{}.db", std::process::id()))
}

/// A server on `backend` with the character seeded (with its possessions) before start-up, and a
/// bot logged in at character select.
fn server(backend: Backend, db: Option<&std::path::Path>) -> (TestServer, ClientId, SessionId) {
    let db = db.map(std::path::Path::to_path_buf);
    let setup = move |w: &mut World| {
        if let (Backend::Sqlite, Some(path)) = (backend, db) {
            let _ = std::fs::remove_file(&path);
            let shard = SqliteShard::open(&path).expect("a SQLite shard in the temp dir");
            w.shard = ShardHandle::synchronous(Box::new(shard), Arc::new(VirtualClock::default()));
        }
        let account_id = w
            .auth
            .lock()
            .create_account(
                ACCOUNT,
                "pw",
                AccessLevel::Player,
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            )
            .expect("created")
            .account_id;
        let mut biota = Biota {
            id: PLAYER,
            weenie_class_id: 1,
            weenie_type: WeenieType::Creature,
            ..Default::default()
        };
        biota.set_property(
            empyrean_entity::enums::PropertyDataId::CombatTable,
            0x3000_0000,
        );
        biota.set_property(PropertyString::Name, "Aldric".to_owned());
        biota.set_property(PropertyDataId::Setup, PLAYER_SETUP);
        biota.set_property(PropertyInt::HeritageGroup, 1);
        biota.set_property(PropertyInt::ItemsCapacity, 102);
        biota.set_property(PropertyInt::ContainersCapacity, 7);
        biota.set_property(PropertyFloat::HeartbeatInterval, 5.0);
        biota.set_property_position(PositionType::Location, pos(0xA9B4_0019, 84.0, 7.1, 94.005));
        biota.properties_enchantment_registry = Some(Vec::new());
        let mut possessions = vec![
            item(A, 50, WeenieType::Generic, Some(PLAYER), None),
            item(P, 51, WeenieType::Container, Some(PLAYER), None),
            item(C, 50, WeenieType::Generic, Some(P), None),
            item(W, 50, WeenieType::Generic, None, Some(PLAYER)),
        ];
        let character = empyrean_store::models::shard::Character {
            id: PLAYER,
            account_id,
            name: "Aldric".to_owned(),
            ..Default::default()
        };
        assert!(w.shard.base_database().add_character_in_parallel(
            &mut biota,
            &mut possessions,
            &character
        ));
    };
    let mut ts = TestServer::with_setup(dats(), setup);
    ts.world.content = Arc::new(content());
    ts.world.world_manager.world_status = WorldStatusState::Open;
    guid_manager::initialize(&mut ts.world, &mut EmptyShard);
    land::use_flat_land_with_setup(
        &mut ts.world,
        &blocks(),
        HEIGHT,
        PLAYER_SETUP,
        land::test_setup_geometry(),
    );
    phys_ext::register_setup(&mut ts.world, land::TEST_SETUP, land::test_setup_geometry());
    let id = ts.connect(ACCOUNT, "pw");
    let session = ts
        .world
        .sessions
        .iter()
        .map(|(s, _)| s)
        .next()
        .expect("session");
    assert!(ts.run_until(1.0, |ts| ts
        .world
        .sessions
        .get(session)
        .is_some_and(|s| s.state == SessionState::AuthConnected)));
    (ts, id, session)
}

fn enter(ts: &mut TestServer, id: ClientId) {
    ts.send_message(id, NetQueue::Logon, &LoginSendEnterWorldRequest);
    ts.advance(0.1);
    ts.send_message(
        id,
        NetQueue::Logon,
        &LoginSendEnterWorld {
            character: ObjectId(PLAYER),
            account: ACCOUNT.to_owned(),
        },
    );
    ts.advance(0.5);
    assert!(
        player_manager::get_online_player(&ts.world, PLAYER).is_some(),
        "in the world"
    );
}

fn player(ts: &mut TestServer) -> &mut empyrean_world::world_objects::world_object::WorldObject {
    ts.world
        .objects
        .get_mut(ObjectGuid::new(PLAYER))
        .expect("the player is in the world")
}

/// A biota as an order-free set of lines, one per property, row or record (an absent collection
/// and an empty one read the same, as the shard stores neither).
fn canon(b: &Biota) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    out.insert(format!(
        "id {} wcid {} type {:?}",
        b.id, b.weenie_class_id, b.weenie_type
    ));
    macro_rules! dict {
        ($f:ident) => {
            for (k, v) in b.$f.iter().flat_map(|d| d.iter()) {
                out.insert(format!("{} {k:?} = {v:?}", stringify!($f)));
            }
        };
    }
    macro_rules! list {
        ($f:ident) => {
            for v in b.$f.iter().flat_map(|d| d.iter()) {
                out.insert(format!("{} {v:?}", stringify!($f)));
            }
        };
    }
    dict!(properties_bool);
    dict!(properties_did);
    dict!(properties_float);
    dict!(properties_iid);
    dict!(properties_int);
    dict!(properties_int64);
    dict!(properties_string);
    dict!(properties_position);
    dict!(properties_spell_book);
    dict!(properties_attribute);
    dict!(properties_attribute_2nd);
    dict!(properties_skill);
    dict!(properties_allegiance);
    dict!(house_permissions);
    list!(properties_anim_part);
    list!(properties_palette);
    list!(properties_texture_map);
    list!(properties_book_page_data);
    list!(properties_enchantment_registry);
    for v in b.properties_create_list.iter().flat_map(|d| d.iter()) {
        out.insert(format!("create_list {v:?}"));
    }
    for v in b.properties_emote.iter().flat_map(|d| d.iter()) {
        out.insert(format!("emote {v:?}"));
    }
    for v in b.properties_generator.iter().flat_map(|d| d.iter()) {
        out.insert(format!("generator {v:?}"));
    }
    for (k, v) in b.properties_body_part.iter().flat_map(|d| d.iter()) {
        out.insert(format!("body_part {k:?} = {v:?}"));
    }
    if let Some(book) = &b.properties_book {
        out.insert(format!("book {book:?}"));
    }
    out
}

fn shard_biota(ts: &TestServer, id: u32) -> Biota {
    let b = ts
        .world
        .shard
        .base_database()
        .get_biota(id, false)
        .unwrap_or_else(|| panic!("{id:08X} is in the shard"));
    BiotaConverter::convert_to_entity_biota(&b, false)
}

fn world_biota(ts: &TestServer, id: u32) -> Biota {
    ts.world
        .objects
        .get(ObjectGuid::new(id))
        .unwrap_or_else(|| panic!("{id:08X} is in the world"))
        .biota
        .clone()
}

/// What the test changes in the world: a property, the position (in the position cache only, as
/// movement does), a skill, a possession's property, a new item in the main pack, and a
/// Character option. Returns the new item's guid.
fn change_everything(ts: &mut TestServer) -> u32 {
    let p = player(ts);
    p.set_property(PropertyInt::CreatureKills, 7);
    p.set_property(PropertyString::Title, "Persisted".to_owned());
    let location = p
        .get_position_mut(PositionType::Location)
        .expect("a location");
    let mut xyz = location.pos();
    xyz.x += 1.5;
    location.set_pos(xyz);
    let skill = p.biota.get_or_add_skill(Skill::MeleeDefense).0;
    skill.sac = SkillAdvancementClass::Trained;
    skill.pp = 1234;
    skill.init_level = 10;
    let pd = &mut p.player.as_mut().expect("a player").player_database;
    pd.character_changes_detected = true;
    let character = p
        .player
        .as_mut()
        .expect("a player")
        .player
        .character
        .as_mut()
        .expect("a Character");
    character.character_options_1 |= 0x0000_0002;

    ts.world
        .objects
        .get_mut(ObjectGuid::new(C))
        .expect("C")
        .set_property(PropertyInt::StackSize, 3);

    let gem = empyrean_world::entity::landblock::world_object_factory_create_new_world_object(
        &mut ts.world,
        52,
    )
    .expect("a gem");
    let gem_guid = gem.guid;
    ts.world.objects.insert(gem).expect("a new guid");
    assert!(
        empyrean_world::world_objects::container::try_add_to_inventory(
            &mut ts.world,
            ObjectGuid::new(PLAYER),
            gem_guid,
            1,
            false,
            false
        )
    );
    gem_guid.full()
}

fn roundtrip(backend: Backend, db: Option<&std::path::Path>) {
    let (mut ts, id, _) = server(backend, db);
    enter(&mut ts, id);
    let gem = change_everything(&mut ts);
    let x_in_world = ts
        .world
        .objects
        .get(ObjectGuid::new(PLAYER))
        .unwrap()
        .location()
        .unwrap()
        .pos()
        .x;

    // Log off: once the player is offline (FinalizeLogout ran; the session still holds it for the
    // 6-second log-off), the world's biotas are exactly the shard's.
    ts.send_message(
        id,
        NetQueue::Logon,
        &LoginExecuteLogOffRequest {
            character: ObjectId(PLAYER),
        },
    );
    assert!(
        ts.run_until(3.0, |ts| player_manager::get_online_player(
            &ts.world, PLAYER
        )
        .is_none()),
        "offline"
    );
    let ids = [PLAYER, A, P, C, W, gem];
    let at_logoff: Vec<Biota> = ids.iter().map(|&g| world_biota(&ts, g)).collect();
    for (g, b) in ids.iter().zip(&at_logoff) {
        assert_eq!(
            canon(&shard_biota(&ts, *g)),
            canon(b),
            "{backend:?}: {g:08X} saved as it was"
        );
    }
    let saved = shard_biota(&ts, PLAYER);
    assert_eq!(saved.get_property(PropertyInt::CreatureKills), Some(7));
    assert!(
        saved.get_property(PropertyFloat::LogoffTimestamp).is_some(),
        "SetPropertiesAtLogOut"
    );
    assert_eq!(
        saved
            .properties_position
            .as_ref()
            .unwrap()
            .get(&PositionType::Location)
            .unwrap()
            .position_x,
        x_in_world,
        "the cached position"
    );
    let character = ts
        .world
        .shard
        .base_database()
        .get_character(PLAYER)
        .expect("the character");
    assert_eq!(
        character.character_options_1 & 0x2,
        0x2,
        "{backend:?}: the Character"
    );
    assert!(
        ts.run_until(10.0, |ts| !ts.received::<LoginExecuteLogOff>(id).is_empty()),
        "logged off"
    );
    ts.advance(0.2);
    assert!(
        ts.world.objects.get(ObjectGuid::new(PLAYER)).is_none(),
        "released with its possessions"
    );
    assert!(ts.world.objects.get(ObjectGuid::new(gem)).is_none());

    // Enter again: the player and every possession come back from the shard with every change.
    enter(&mut ts, id);
    let back = world_biota(&ts, PLAYER);
    assert_eq!(back.get_property(PropertyInt::CreatureKills), Some(7));
    assert_eq!(
        back.get_property(PropertyString::Title).as_deref(),
        Some("Persisted")
    );
    assert_eq!(
        back.properties_position
            .as_ref()
            .unwrap()
            .get(&PositionType::Location)
            .unwrap()
            .position_x,
        x_in_world
    );
    let skill = back
        .properties_skill
        .as_ref()
        .unwrap()
        .get(&Skill::MeleeDefense)
        .expect("the skill");
    assert_eq!(
        (skill.sac, skill.pp, skill.init_level),
        (SkillAdvancementClass::Trained, 1234, 10)
    );
    for (g, b) in ids.iter().zip(&at_logoff).skip(1) {
        assert_eq!(
            canon(&world_biota(&ts, *g)),
            canon(b),
            "{backend:?}: possession {g:08X} is back as it was"
        );
    }
    // The player's own biota: the same, but for what entering the world changes (the login and
    // heartbeat bookkeeping).
    let volatile = |s: BTreeSet<String>| -> BTreeSet<String> {
        s.into_iter()
            .filter(|l| {
                ![
                    "Age",
                    "LoginTimestamp",
                    "HeartbeatTimestamp",
                    "CheckpointTimestamp",
                ]
                .iter()
                .any(|k| l.contains(k))
            })
            .collect()
    };
    assert_eq!(
        volatile(canon(&back)),
        volatile(canon(&at_logoff[0])),
        "{backend:?}: the player is back as it was"
    );
    let c = &back_character(&ts);
    assert_eq!(
        c.character_options_1 & 0x2,
        0x2,
        "{backend:?}: the Character option is back"
    );
    assert_eq!(c.spellbook_filters, 16383);
}

fn back_character(ts: &TestServer) -> empyrean_store::models::shard::Character {
    ts.world
        .objects
        .get(ObjectGuid::new(PLAYER))
        .unwrap()
        .player
        .as_ref()
        .unwrap()
        .player
        .character
        .clone()
        .unwrap()
}

#[test]
fn every_change_survives_log_off_and_back_on_mem_shard() {
    roundtrip(Backend::Mem, None);
}

#[test]
fn every_change_survives_log_off_and_back_on_sqlite_shard() {
    let path = temp_db("roundtrip");
    roundtrip(Backend::Sqlite, Some(&path));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_player_is_saved_every_five_minutes_of_virtual_time() {
    let (mut ts, id, _) = server(Backend::Mem, None);
    enter(&mut ts, id);
    // The client's login completes (out of portal space): a player left in portal space is
    // logged off by its heartbeat once five minutes have passed.
    ts.send_game_action(id, &CharacterLoginCompleteNotification);
    ts.advance(0.1);
    let entered = ts.seconds();
    player(&mut ts).set_property(PropertyInt::CreatureKills, 42);
    let kills = |ts: &TestServer| shard_biota(ts, PLAYER).get_property(PropertyInt::CreatureKills);

    // The first heartbeat stamps LastRequestedDatabaseSave; the save is due 300 s later, at the
    // first heartbeat after that (heartbeats are 5 s apart).
    ts.advance(290.0);
    assert_eq!(kills(&ts), None, "not before five minutes");
    assert!(
        ts.run_until(20.0, |ts| kills(ts) == Some(42)),
        "saved by the heartbeat"
    );
    let saved_at = ts.seconds() - entered;
    assert!(
        (300.0..=311.0).contains(&saved_at),
        "saved {saved_at} s after entering"
    );

    // And again five minutes later.
    player(&mut ts).set_property(PropertyInt::CreatureKills, 43);
    ts.advance(290.0);
    assert_eq!(kills(&ts), Some(42));
    assert!(ts.run_until(20.0, |ts| kills(ts) == Some(43)));
}

#[test]
fn a_dropped_session_still_saves_the_player() {
    let (mut ts, id, session) = server(Backend::Mem, None);
    enter(&mut ts, id);
    player(&mut ts).set_property(PropertyInt::CreatureKills, 9);
    ts.world
        .objects
        .get_mut(ObjectGuid::new(A))
        .expect("A")
        .set_property(PropertyInt::StackSize, 5);

    // The client vanishes: the transport times the session out and drops it (Session.DropSession
    // -> LogOffPlayer -> LogOut), and the log-out finishes and saves without it.
    let now = ts.world.now;
    ts.world.net.terminate(
        session,
        SessionTerminationReason::NetworkTimeout,
        None,
        String::new(),
        now,
    );
    assert!(
        ts.run_until(10.0, |ts| player_manager::get_online_player(
            &ts.world, PLAYER
        )
        .is_none()),
        "logged out"
    );
    assert_eq!(
        shard_biota(&ts, PLAYER).get_property(PropertyInt::CreatureKills),
        Some(9)
    );
    assert_eq!(
        shard_biota(&ts, A).get_property(PropertyInt::StackSize),
        Some(5)
    );
    assert!(
        player_manager::get_offline_player(&ts.world, PLAYER).is_some(),
        "offline, with the saved biota"
    );
    ts.advance(1.0);
    assert!(
        ts.world.sessions.get(session).is_none()
            && ts.world.objects.get(ObjectGuid::new(PLAYER)).is_none(),
        "released"
    );
}

/// The soak relog check takes the players last state at the release.
/// V219.
#[test]
fn the_soak_relog_check_takes_the_players_last_state_at_the_release() {
    use empyrean_testkit::soak_invariants::Checks;
    let (mut ts, id, _) = server(Backend::Mem, None);
    enter(&mut ts, id);
    ts.advance(1.0);
    let mut checks = Checks::default();
    ts.send_message(
        id,
        NetQueue::Logon,
        &LoginExecuteLogOffRequest {
            character: ObjectId(PLAYER),
        },
    );
    assert!(
        ts.run_until(3.0, |ts| player_manager::get_online_player(
            &ts.world, PLAYER
        )
        .is_none()),
        "offline"
    );
    checks.track(&ts.world, PLAYER);

    // the death, after the last snapshot
    player(&mut ts).set_property(PropertyInt::NumDeaths, 1);
    player(&mut ts).set_property(PropertyInstanceId::Killer, 0x8000_0123);
    assert!(
        ts.run_until(10.0, |ts| ts
            .world
            .objects
            .get(ObjectGuid::new(PLAYER))
            .is_none()),
        "released"
    );
    assert_eq!(
        shard_biota(&ts, PLAYER).get_property(PropertyInt::NumDeaths),
        None,
        "the shard has the log-off save only"
    );
    checks.on_released(&ts.world, PLAYER);
    assert_eq!(
        (
            checks.saved_ok,
            checks.saved_mismatch,
            checks.changed_after_last_snapshot,
            checks.changed_after_save
        ),
        (1, 0, 1, 1),
        "{:?}",
        checks.failures
    );

    enter(&mut ts, id);
    assert_eq!(
        world_biota(&ts, PLAYER).get_property(PropertyInt::NumDeaths),
        Some(1),
        "back with the death, from the OfflinePlayer"
    );
    checks.on_entered(&ts.world, PLAYER);
    assert_eq!(
        (checks.reload_ok, checks.reload_mismatch),
        (1, 0),
        "{:?}",
        checks.failures
    );
}
