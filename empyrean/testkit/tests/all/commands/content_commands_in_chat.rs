//! ACE: Source/ACE.Server/Command/Handlers/DeveloperContentCommands.cs::DeveloperContentCommands
//! @import-sql/@clearcache/@createinst/@removeinst/@nudge/@addenc/@export-sql/@export-json over
//! the overlay: new weenie spawns, instances persist across restart, export-import round trip, no
//! overlay means no writes.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dereth_assets::tables::SkillFormula;
use dereth_primitives::NetQueue;
use dereth_primitives::ObjectId;
use dereth_protocol::comms::{CommunicationTalk, CommunicationTextboxString};
use dereth_protocol::login::{
    CharacterLoginCompleteNotification, LoginCharacterSet, LoginSendEnterWorld,
    LoginSendEnterWorldRequest,
};
use empyrean_command::command_manager;
use empyrean_common::dotnet::DotNetDict;
use empyrean_content::import::patch::{sources, Input, InputKind};
use empyrean_content::import::{build_from, default_now};
use empyrean_content::overlay::{BaseInputs, ContentOverlay};
use empyrean_content::pack::Pack;
use empyrean_content::PackContent;
use empyrean_dat::file_types::SecondaryAttributeTable;
use empyrean_dat::{file_id, FakeDats};
use empyrean_entity::enums::{
    AccessLevel, ChatMessageType, PositionType, PropertyAttribute, PropertyAttribute2nd,
    PropertyBool, PropertyDataId, PropertyInt, PropertyInt64, PropertyString, WeenieType,
};
use empyrean_entity::models::{PropertiesAttribute, PropertiesAttribute2nd, PropertiesPosition};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_store::models::shard::Character;
use empyrean_testkit::{land, ClientId, TestServer};
use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
use empyrean_world::managers::{player_manager, property_manager};

/// A landblock the fixture dump has no instances in.
const HOME: u16 = 0x2B2B;
const ADMIN: u32 = 0x5000_0081;
const STATUE: u32 = 7001;
const GENERATOR: u32 = 7002;

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
        id: dereth_primitives::DataId(file_id::SECONDARY_ATTRIBUTE_TABLE),
        health: f(2, 2),
        stamina: f(2, 1),
        mana: f(6, 1),
    };
    empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_portal(file_id::SECONDARY_ATTRIBUTE_TABLE, table)
        .with_xp_table(empyrean_dat::fake::sample::xp_table())
        .build()
        .expect("fake dats")
}

fn base_sql() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/content/tests/fixtures/patches/base.sql")
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "empyrean-testkit-content-commands-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("content")).unwrap();
    d
}

/// The setup patch over the fixture dump: the player's weenie, a stuck statue with the test body,
/// and a generator of statues.
fn setup_sql() -> String {
    format!(
        "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\n\
         VALUES (1, 'human', 10, '2021-11-01 00:00:00'), ({STATUE}, 'u8b2statue', 1, '2021-11-01 00:00:00'), ({GENERATOR}, 'u8b2generator', 1, '2021-11-01 00:00:00');\n\
         INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)\n\
         VALUES (1, 4, 805306368), ({STATUE}, 1, {setup}), ({GENERATOR}, 1, {setup});\n\
         INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)\n\
         VALUES ({STATUE}, 1, True);\n\
         INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)\n\
         VALUES ({STATUE}, 1, 'Statue'), ({GENERATOR}, 1, 'Statue Generator');\n\
         INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)\n\
         VALUES ({STATUE}, 1, 128), ({STATUE}, 19, 10);\n\
         INSERT INTO `weenie_properties_generator` (`object_Id`, `probability`, `weenie_Class_Id`, `delay`, `init_Create`, `max_Create`, `when_Create`, `where_Create`)\n\
         VALUES ({GENERATOR}, -1, {STATUE}, 0, 1, 1, 1, 4);\n",
        setup = land::TEST_SETUP
    )
}

/// The world database: the fixture dump plus the setup patch, as `empyrean-import` builds it, with
/// the overlay at `dir/overlay.sqlite` in front of it. (Called again to "restart".)
fn world_database(dir: &Path) -> (PackContent, Arc<ContentOverlay>) {
    let setup = dir.join("setup.sql");
    if !setup.exists() {
        std::fs::write(&setup, setup_sql()).unwrap();
    }
    let patches = vec![Input {
        kind: InputKind::Sql,
        path: setup,
    }];
    let dump = std::fs::File::open(base_sql()).unwrap();
    let (bytes, _) = build_from(dump, sources(&patches), default_now()).unwrap();
    let db = PackContent::new(Pack::from_bytes(bytes).unwrap());
    let base = BaseInputs {
        sql: base_sql(),
        patches,
        era: empyrean_common::era::EraId::Eor,
    };
    let overlay = ContentOverlay::open(
        &dir.join("overlay.sqlite"),
        base,
        db.base().pack().header(),
        default_now(),
    )
    .unwrap()
    .shared();
    db.base().attach_overlay(overlay.clone()).unwrap();
    (db, overlay)
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

fn seed(ts: &TestServer, account: &str, guid: u32, name: &str, at: Position) {
    let account_id = ts
        .auth()
        .create_account(
            account,
            "pw",
            AccessLevel::Player,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .expect("created")
        .account_id;
    let mut biota = empyrean_entity::Biota {
        id: guid,
        weenie_class_id: 1,
        weenie_type: WeenieType::Creature,
        ..Default::default()
    };
    biota.set_property(PropertyDataId::CombatTable, 0x3000_0000);
    biota.set_property(PropertyString::Name, name.to_owned());
    biota.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    biota.set_property(PropertyBool::ReportCollisions, true);
    biota.set_property(PropertyBool::IgnoreCollisions, false);
    biota.set_property(PropertyInt::ItemsCapacity, 102);
    biota.set_property(PropertyInt::ContainersCapacity, 7);
    biota.set_property(PropertyInt::Level, 1);
    biota.set_property(PropertyInt64::TotalExperience, 0);
    biota.set_property(PropertyInt64::AvailableExperience, 0);
    let attributes = biota
        .properties_attribute
        .get_or_insert_with(DotNetDict::new);
    for a in &PropertyAttribute::ALL[1..] {
        attributes.insert(
            *a,
            PropertiesAttribute {
                init_level: 100,
                ..PropertiesAttribute::default()
            },
        );
    }
    let vitals = biota
        .properties_attribute_2nd
        .get_or_insert_with(DotNetDict::new);
    for v in [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ] {
        vitals.insert(
            v,
            PropertiesAttribute2nd {
                init_level: 100,
                current_level: 100,
                ..PropertiesAttribute2nd::default()
            },
        );
    }
    let position = PropertiesPosition {
        obj_cell_id: at.cell(),
        position_x: at.position_x,
        position_y: at.position_y,
        position_z: at.position_z,
        rotation_w: at.rotation_w,
        rotation_x: at.rotation_x,
        rotation_y: at.rotation_y,
        rotation_z: at.rotation_z,
    };
    biota
        .properties_position
        .get_or_insert_with(DotNetDict::new)
        .insert(PositionType::Location, position);
    let character = Character {
        id: guid,
        account_id,
        name: name.to_owned(),
        ..Character::default()
    };
    assert!(ts
        .shard()
        .add_character_in_parallel(&mut biota, &mut [], &character));
}

/// A server whose world database has the overlay, with the admin in the world at (100, 100) of
/// HOME, and `content_folder` set to `dir/content`.
fn server(dir: &Path, with_overlay: bool) -> (TestServer, ClientId, Arc<ContentOverlay>) {
    let mut ts = TestServer::with_dats(dats());
    land::use_flat_land_with_test_setup(&mut ts.world, &[HOME], 10);
    let (db, overlay) = world_database(dir);
    ts.world.content = if with_overlay {
        Arc::new(db)
    } else {
        Arc::new(PackContent::new(
            Pack::from_bytes(db.base().pack().bytes().to_vec()).unwrap(),
        ))
    };
    guid_manager::initialize(&mut ts.world, &mut EmptyShard);
    seed(
        &ts,
        "u8b2admin",
        ADMIN,
        "Content Admin",
        outdoor(100.0, 100.0, 20.0),
    );
    player_manager::initialize(&mut ts.world);
    command_manager::initialize(None);
    let content = dir.join("content");
    assert!(property_manager::modify_string(
        &ts.world,
        "content_folder",
        content.to_str().unwrap()
    ));

    let id = ts.connect("u8b2admin", "pw");
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
            character: ObjectId(ADMIN),
            account: "u8b2admin".to_owned(),
        },
    );
    let g = ObjectGuid::new(ADMIN);
    assert!(
        ts.run_until(1.0, |ts| ts
            .world
            .objects
            .get(g)
            .is_some_and(|o| o.current_landblock.is_some())),
        "the admin enters the world"
    );
    ts.send_game_action(id, &CharacterLoginCompleteNotification);
    ts.advance(0.1);
    ts.world.objects.get_mut(g).unwrap().set_is_admin_prop(true);
    let _ = TestServer::take_not_ported();
    (ts, id, overlay)
}

/// Types `line` in chat and returns the text of the chat lines the client received for it.
fn say(ts: &mut TestServer, id: ClientId, line: &str) -> Vec<String> {
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
        .inspect(|m| {
            assert!(
                m.text_type == ChatMessageType::Broadcast.0,
                "{} is a broadcast",
                m.text
            )
        })
        .map(|m| m.text.clone())
        .collect()
}

/// JSON text without its changelog's `created` stamp (the clock at the export).
fn without_created(json: &str) -> String {
    json.lines()
        .filter(|l| !l.trim_start().starts_with("\"created\":"))
        .collect::<Vec<_>>()
        .join(
            "
",
        )
}

/// The objects of weenie `wcid` on the admin's landblock.
fn objects_of(ts: &TestServer, wcid: u32) -> Vec<ObjectGuid> {
    let lb = ts
        .world
        .objects
        .get(ObjectGuid::new(ADMIN))
        .unwrap()
        .current_landblock
        .unwrap();
    let l = ts.world.landblock_manager.landblocks.get(lb).unwrap();
    l.get_all_world_objects_for_diagnostics()
        .into_iter()
        .filter(|g| {
            ts.world
                .objects
                .get(*g)
                .is_some_and(|o| o.biota.weenie_class_id == wcid)
        })
        .collect()
}

/// `{content}{sep}sql{sep}weenies{sep}` as ACE builds it.
fn folder(dir: &Path, parts: &[&str]) -> String {
    let sep = std::path::MAIN_SEPARATOR;
    let mut s = std::path::absolute(dir.join("content"))
        .unwrap()
        .to_string_lossy()
        .into_owned();
    for p in parts {
        s.push(sep);
        s.push_str(p);
    }
    s.push(sep);
    s
}

/// The statue's SQL as ACE's WeenieSQLWriter writes it, named `name`.
fn statue_sql(name: &str) -> String {
    format!(
        "DELETE FROM `weenie` WHERE `class_Id` = {STATUE};\n\n\
         INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\n\
         VALUES ({STATUE}, 'u8b2statue', 1, '2021-11-01 00:00:00') /* Generic */;\n\n\
         INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)\n\
         VALUES ({STATUE},   1,        128) /* ItemType - Misc */\n     , ({STATUE},  19,         25) /* Value */;\n\n\
         INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)\n\
         VALUES ({STATUE},   1, True ) /* Stuck */;\n\n\
         INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)\n\
         VALUES ({STATUE},   1, '{name}') /* Name */;\n\n\
         INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)\n\
         VALUES ({STATUE},   1, {setup}) /* Setup */;\n",
        setup = land::TEST_SETUP
    )
}

#[test]
fn import_sql_then_clearcache_then_a_spawn_shows_the_new_weenie() {
    let dir = tmp("importing");
    let (mut ts, id, overlay) = server(&dir, true);

    // The statue and the generator as the pack has them, cached.
    let name = |ts: &TestServer, wcid: u32| {
        ts.world
            .content
            .get_cached_weenie(wcid)
            .unwrap()
            .properties_string
            .as_ref()
            .unwrap()
            .get(&PropertyString::Name)
            .cloned()
    };
    let before = ts.world.content.get_cached_weenie(STATUE).unwrap();
    assert_eq!(name(&ts, STATUE).as_deref(), Some("Statue"));
    assert_eq!(name(&ts, GENERATOR).as_deref(), Some("Statue Generator"));

    // The file ACE's import-sql reads: <content>/sql/weenies/07001 *.sql. It also renames the
    // generator, which only the whole weenie cache's clearing picks up.
    let sql_folder = folder(&dir, &["sql", "weenies"]);
    std::fs::create_dir_all(&sql_folder).unwrap();
    let text = statue_sql("Bronze Statue") + &format!("\nUPDATE `weenie_properties_string` SET `value` = 'Bronze Generator' WHERE `object_Id` = {GENERATOR} AND `type` = 1;\n");
    std::fs::write(format!("{sql_folder}07001 Statue.sql"), text).unwrap();

    assert_eq!(
        say(&mut ts, id, "@import-sql 7001"),
        [
            "Imported 07001 Statue.sql",
            "Converted 07001 Statue.sql to 07001 - Statue.json"
        ],
    );
    // The file went to the overlay, and ImportSQLWeenie dropped the cached weenie.
    assert_eq!(overlay.journal().unwrap().len(), 1);
    assert!(!Arc::ptr_eq(
        &ts.world.content.get_cached_weenie(STATUE).unwrap(),
        &before
    ));

    // sql2json wrote the JSON beside it, as ACE's exporter serializes the weenie.
    let json_folder = folder(&dir, &["json", "weenies"]);
    let json = std::fs::read_to_string(format!("{json_folder}07001 - Statue.json")).unwrap();
    let weenie = ts.world.content.get_weenie(STATUE).unwrap();
    let (expected, _) = empyrean_content::export::json::try_convert_ace_weenie_to_lsd_json(
        &weenie,
        ts.world.now.utc,
    )
    .unwrap();
    assert_eq!(without_created(&json), without_created(&expected));
    assert!(json.contains("\"value\": \"Bronze Statue\""), "{json}");

    // The generator is still the cached one, as in ACE, until the caches are cleared.
    assert_eq!(name(&ts, GENERATOR).as_deref(), Some("Statue Generator"));
    assert_eq!(
        say(&mut ts, id, "@clearcache"),
        [
            "Clearing landblock instance cache",
            "Clearing recipe cache",
            "Clearing spell cache",
            "Clearing weenie cache",
            "Clearing wielded treasure cache"
        ],
    );

    // A spawn now shows the new weenies.
    for wcid in [STATUE, GENERATOR] {
        let replies = say(&mut ts, id, &format!("@create {wcid}"));
        assert!(replies.is_empty(), "{replies:?}");
    }
    let spawned: Vec<_> = [STATUE, GENERATOR]
        .into_iter()
        .flat_map(|wcid| objects_of(&ts, wcid))
        .map(|g| {
            ts.world
                .objects
                .get(g)
                .unwrap()
                .get_property(PropertyString::Name)
        })
        .collect();
    assert_eq!(
        spawned,
        [
            Some("Bronze Statue".to_owned()),
            Some("Bronze Generator".to_owned())
        ]
    );
    assert_eq!(
        ts.world
            .content
            .get_cached_weenie(STATUE)
            .unwrap()
            .properties_int
            .as_ref()
            .unwrap()
            .get(&PropertyInt::Value),
        Some(&25)
    );
}

#[test]
fn createinst_and_removeinst_persist_across_a_restart() {
    let dir = tmp("createinst");
    let (mut ts, id, overlay) = server(&dir, true);
    let guid = 0x7000_0000 | (u32::from(HOME) << 12);

    let replies = say(&mut ts, id, "@createinst 7001");
    assert_eq!(replies.len(), 1, "{replies:?}");
    assert!(
        replies[0].starts_with(
            "Creating new landblock instance @ 0x2B2B0025 [100.000000 100.000000 20.000000]"
        ),
        "{}",
        replies[0]
    );
    assert!(
        replies[0].ends_with("\n7001 - Statue (72B2B000)"),
        "{}",
        replies[0]
    );
    assert!(
        ts.world.objects.get(ObjectGuid::new(guid)).is_some(),
        "the statue is in the world"
    );
    // SyncInstances wrote <content>/sql/landblocks/2B2B.sql and imported it.
    let file = std::fs::read_to_string(format!(
        "{}{}2B2B.sql",
        folder(&dir, &["sql", "landblocks"]),
        std::path::MAIN_SEPARATOR
    ))
    .unwrap();
    assert!(
        file.starts_with("DELETE FROM `landblock_instance` WHERE `landblock` = 0x2B2B;"),
        "{file}"
    );
    assert_eq!(overlay.journal().unwrap().len(), 1);

    // A restart: the pack reopened, the overlay read back.
    drop(overlay);
    let (db, _) = world_database(&dir);
    let instances = db.get_cached_instances_by_landblock(HOME);
    assert_eq!(
        instances
            .iter()
            .map(|i| (i.guid, i.weenie_class_id))
            .collect::<Vec<_>>(),
        [(guid, STATUE)]
    );
    assert!(
        (instances[0].origin_z - 20.05).abs() < 1e-4,
        "spawned 0.05 above the admin: {}",
        instances[0].origin_z
    );

    // Remove it (the last appraised object).
    ts.world
        .objects
        .get_mut(ObjectGuid::new(ADMIN))
        .unwrap()
        .set_requested_appraisal_target(Some(guid));
    assert_eq!(
        say(&mut ts, id, "@removeinst"),
        ["Removed 7001 - Statue (0x72B2B000) from landblock instances"]
    );
    assert!(ts.world.objects.get(ObjectGuid::new(guid)).is_none());
    let (db, overlay) = world_database(&dir);
    assert!(
        db.get_cached_instances_by_landblock(HOME).is_empty(),
        "the removal survives a restart too"
    );
    assert_eq!(overlay.journal().unwrap().len(), 2);
}

#[test]
fn nudge_and_rotate_move_the_instance_and_save_it() {
    let dir = tmp("nudge");
    let (mut ts, id, _overlay) = server(&dir, true);
    let guid = 0x7000_0000 | (u32::from(HOME) << 12);
    assert_eq!(say(&mut ts, id, "@createinst 7001").len(), 1);
    ts.world
        .objects
        .get_mut(ObjectGuid::new(ADMIN))
        .unwrap()
        .set_requested_appraisal_target(Some(guid));

    let replies = say(&mut ts, id, "@nudge n 2");
    assert_eq!(replies.len(), 1, "{replies:?}");
    assert!(
        replies[0].starts_with("Statue (72B2B000) - moved from 2B2B0025 [100 100 "),
        "{}",
        replies[0]
    );
    assert!(
        replies[0].ends_with(" to 2B2B0025 [100 102 20.05]"),
        "{}",
        replies[0]
    );
    let (db, _) = world_database(&dir);
    let i = &db.get_cached_instances_by_landblock(HOME)[0];
    assert!(
        (i.origin_y - 102.0).abs() < 1e-3,
        "moved north by 2: {}",
        i.origin_y
    );

    let replies = say(&mut ts, id, "@rotate 90");
    assert_eq!(replies.len(), 1, "{replies:?}");
    assert!(
        replies[0].starts_with("Statue (72B2B000) new rotation: {X:0 Y:0 Z:0.70710"),
        "{}",
        replies[0]
    );
    let (db, overlay) = world_database(&dir);
    let i = &db.get_cached_instances_by_landblock(HOME)[0];
    assert!(
        (i.angles_z - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5
            && (i.angles_w - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5,
        "{i:?}"
    );
    assert_eq!(overlay.journal().unwrap().len(), 3);
}

#[test]
fn addenc_and_removeenc_edit_the_encounters() {
    let dir = tmp("addenc");
    let (mut ts, id, _overlay) = server(&dir, true);

    let replies = say(&mut ts, id, "@addenc 7002");
    assert_eq!(
        replies,
        ["Creating new encounter @ landblock 2B2B, cellX=4, cellY=4\n7002 - Statue Generator"]
    );
    let (db, _) = world_database(&dir);
    let e = db.get_cached_encounters_by_landblock(HOME);
    assert_eq!(
        e.iter()
            .map(|e| (e.cell_x, e.cell_y, e.weenie_class_id))
            .collect::<Vec<_>>(),
        [(4, 4, GENERATOR)]
    );
    assert_eq!(
        say(&mut ts, id, "@addenc 7002"),
        ["This cell already contains an encounter!"]
    );

    let generator = objects_of(&ts, GENERATOR)
        .first()
        .copied()
        .expect("the generator spawned");
    ts.world
        .objects
        .get_mut(ObjectGuid::new(ADMIN))
        .unwrap()
        .set_requested_appraisal_target(Some(generator.full()));
    assert_eq!(
        say(&mut ts, id, "@removeenc"),
        ["Removing encounter @ landblock 2B2B, cellX=4, cellY=4\n7002 - Statue Generator"]
    );
    assert!(ts.world.objects.get(generator).is_none());
    let (db, _) = world_database(&dir);
    assert!(db.get_cached_encounters_by_landblock(HOME).is_empty());
}

#[test]
fn export_sql_then_import_sql_round_trips_a_weenie() {
    let dir = tmp("exporting");
    let (mut ts, id, overlay) = server(&dir, true);
    let sql_folder = folder(&dir, &["sql", "weenies"]);

    assert_eq!(
        say(&mut ts, id, "@export-sql 7001"),
        [format!("Exported {sql_folder}07001 Statue.sql")]
    );
    let exported = std::fs::read_to_string(format!("{sql_folder}07001 Statue.sql")).unwrap();
    assert!(
        exported.contains("/* Stuck */"),
        "ACE's labels are written: {exported}"
    );

    let before = ts.world.content.get_weenie(STATUE).unwrap();
    assert_eq!(
        say(&mut ts, id, "@import-sql 7001"),
        [
            "Imported 07001 Statue.sql",
            "Converted 07001 Statue.sql to 07001 - Statue.json"
        ]
    );
    assert_eq!(overlay.journal().unwrap().len(), 1);
    let after = ts.world.content.get_weenie(STATUE).unwrap();
    // Record for record: every row is back with its values (new rows take new AUTO_INCREMENT ids).
    let rows = |w: &empyrean_content::models::world::Weenie| {
        (
            w.class_name.clone(),
            w.r#type,
            w.last_modified,
            w.weenie_properties_int
                .iter()
                .map(|r| (r.r#type, r.value))
                .collect::<Vec<_>>(),
            w.weenie_properties_bool
                .iter()
                .map(|r| (r.r#type, r.value))
                .collect::<Vec<_>>(),
            w.weenie_properties_string
                .iter()
                .map(|r| (r.r#type, r.value.clone()))
                .collect::<Vec<_>>(),
            w.weenie_properties_did
                .iter()
                .map(|r| (r.r#type, r.value))
                .collect::<Vec<_>>(),
        )
    };
    assert_eq!(rows(&after), rows(&before));
    // And exporting again writes the same file.
    assert_eq!(say(&mut ts, id, "@export-sql 7001").len(), 1);
    assert_eq!(
        std::fs::read_to_string(format!("{sql_folder}07001 Statue.sql")).unwrap(),
        exported
    );

    // export-json writes ACE's JSON (the format itself is vector-tested in empyrean-content).
    let json_folder = folder(&dir, &["json", "weenies"]);
    assert_eq!(
        say(&mut ts, id, "@export-json 7001"),
        [format!("Exported {json_folder}7001 - Statue.json")]
    );
    let (expected, _) = empyrean_content::export::json::try_convert_ace_weenie_to_lsd_json(
        &after,
        ts.world.now.utc,
    )
    .unwrap();
    assert_eq!(
        without_created(
            &std::fs::read_to_string(format!("{json_folder}7001 - Statue.json")).unwrap()
        ),
        without_created(&expected)
    );
}

#[test]
fn without_an_overlay_the_world_database_cannot_be_written() {
    let dir = tmp("no-overlay");
    let (mut ts, id, _) = server(&dir, false);
    let sql_folder = folder(&dir, &["sql", "weenies"]);
    std::fs::create_dir_all(&sql_folder).unwrap();
    std::fs::write(
        format!("{sql_folder}07001 Statue.sql"),
        statue_sql("Bronze Statue"),
    )
    .unwrap();
    assert_eq!(
        say(&mut ts, id, "@import-sql 7001"),
        ["There was an error importing the SQL:\n\nNo content overlay is configured (Server.WorldOverlayPath), so the world database cannot be written"],
    );
    assert_eq!(
        say(&mut ts, id, "@import-sql 99999"),
        [format!("Couldn't find {sql_folder}99999 *.sql")]
    );
    assert_eq!(
        say(&mut ts, id, "@import-sql 7001 bogus"),
        ["Unknown content type 'bogus'"]
    );
}

#[cfg(feature = "real-content")]
mod indoor_creation_real {
    //! ACE: Source/ACE.Server/Managers/WorldManager.cs::DoPlayerEnterWorld
    use crate::support::real_content_bot::real::*;

    /// A drudge @created 5 m ahead of the character, where the spot lies in the next cell of the
    /// Academy (`0x860201B0`, east of the entrance hall), is placed in that cell and enters the
    /// world (`AdminCommands.HandleCreate` -> `PositionExtensions.GetCell` -> `GetIndoorCell`).
    #[test]
    fn admin_create_places_an_object_in_another_indoor_cell() {
        let mut l = create_and_enter();
        let mut spot = l.location();
        spot.position_x = 10.5;
        spot.position_y = -21.0;
        l.walk_toward(&spot, 0.0);
        // face east (+x): a rotation of 270 degrees about z
        let mut at = l.location();
        (at.rotation_w, at.rotation_x, at.rotation_y, at.rotation_z) = (
            -std::f32::consts::FRAC_1_SQRT_2,
            0.0,
            0.0,
            std::f32::consts::FRAC_1_SQRT_2,
        );
        l.action(&autonomous(&at));
        l.advance(0.5);
        assert_eq!(l.location().cell(), ACADEMY_CELL);
        // `HandleCreate`'s spot for a creature, and the cell `GetCell` gives it (before the
        // physics placement, which would also find the cell from the hall's visible cells)
        let ahead = l.location().in_front_of(5.0, true);
        assert_eq!(
            empyrean_world::entity::position_extensions::get_cell(&l.ts.world, &ahead),
            0x8602_01B0,
            "GetIndoorCell finds the next cell"
        );
        l.admin_command("@create drudgeskulker");
        let drudges = l.nearby_wcid(DRUDGE_SKULKER);
        assert_eq!(drudges.len(), 1, "a drudge in front of the character");
        assert_eq!(
            l.location_of(drudges[0]).cell(),
            0x8602_01B0,
            "in the next cell"
        );
    }
}
