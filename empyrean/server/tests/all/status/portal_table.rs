//! Divergence: V458
//! Divergence: V565
//! The status endpoint's portal table and places: `/portals` and `/places` are not served unless
//! `server.publish_portals` is on, and when it is, the portal table lists every placed portal with
//! a destination, in from-cell then class order, with land cells for bare landblocks, level limits,
//! escaped names and what else each asks; the places list every placed lifestone, portal, vendor
//! and creature that cannot be attacked, in the client's place-store format; the portals spawners
//! make are listed in both, and the towns the world names in the places; and the status document
//! names both. A world's own places document (its overlay folder's `places.tsv`) is published in
//! place of the world's places; without one the places are the world's alone, marked as the
//! world's own names when its era's world database names them (Infiltration and Launch, not the
//! end of retail). A world that places its own things publishes no portal or place of the world
//! database's.
//! Fixture: a small in-memory world of placed portals, a portal with no destination and a
//! non-portal, and for the places a lifestone, a vendor, people, a monster, a hidden person and a
//! creature that is no creature by its item type; and a world of a quest-gated portal, a spawner
//! that makes one while its event runs, a named landmark and the vendors of its town; a world's
//! own places document of a town and a landmark, and a temporary overlay folder per test.

use std::sync::Arc;

use dereth_navigation::{EdgeSource, PlaceKind, PlaceStore};
use empyrean_content::models::world::{
    Event, LandblockInstance, PointsOfInterest, Weenie, WeeniePropertiesGenerator,
};
use empyrean_content::MemContent;
use empyrean_entity::enums::{PositionType, PropertyBool, PropertyInt, PropertyString, WeenieType};
use empyrean_server::portal_table::{self, PortalTable};
use empyrean_server::status_endpoint::{respond, respond_with_portals, StatusSnapshot};

const NORTH: [f32; 4] = [1.0, 0.0, 0.0, 0.0];

fn world() -> MemContent {
    MemContent::new()
        // A portal with a level range, a tab and a backslash in its name, into a dungeon cell.
        .weenie(
            Weenie::new(1000, "portalranged", WeenieType::Portal)
                .with_string(PropertyString::Name, "Ranged\tPortal \\ One")
                .with_int(PropertyInt::MinLevel, 10)
                .with_int(PropertyInt::MaxLevel, 50)
                .with_position(
                    PositionType::Destination,
                    0x01AC_0118,
                    [60.0, -50.25, 0.5],
                    NORTH,
                ),
        )
        // A portal with no limits whose destination is stored as a bare landblock.
        .weenie(
            Weenie::new(999, "portalplain", WeenieType::Portal)
                .with_string(PropertyString::Name, "Plain Portal")
                .with_position(
                    PositionType::Destination,
                    0xC6A9_0000,
                    [30.0, 100.0, 12.5],
                    NORTH,
                ),
        )
        // A portal that leads nowhere (its destination is set at run time) is left out.
        .weenie(
            Weenie::new(1001, "portalnowhere", WeenieType::Portal)
                .with_string(PropertyString::Name, "Nowhere"),
        )
        // Something else with a destination is not a portal.
        .weenie(
            Weenie::new(1002, "notaportal", WeenieType::Generic)
                .with_string(PropertyString::Name, "Sign")
                .with_position(PositionType::Destination, 0x0101_0101, [1.0; 3], NORTH),
        )
        .landblock_instance(LandblockInstance::new(
            0x7A9B_4001,
            1000,
            0xA9B4_0021,
            [84.0, 7.5, 94.0],
        ))
        .landblock_instance(LandblockInstance::new(
            0x7A9B_4002,
            999,
            0xA9B4_0021,
            [86.0, 8.0, 94.0],
        ))
        // Stored as a bare landblock: x 191.9 is the eighth column, y 0.5 the first row.
        .landblock_instance(LandblockInstance::new(
            0x7123_4001,
            999,
            0x1234_0000,
            [191.9, 0.5, 10.0],
        ))
        .landblock_instance(LandblockInstance::new(
            0x7123_4002,
            1001,
            0x1234_0002,
            [30.0, 30.0, 0.0],
        ))
        .landblock_instance(LandblockInstance::new(
            0x7123_4003,
            1002,
            0x1234_0002,
            [31.0, 31.0, 0.0],
        ))
}

const EXPECTED: &str = "dereth-navigation-store\t3\n\
world\tTest World \\\\ One\n\
portal\tpublished\t999\tPlain Portal\t0x12340039 191.900 0.500 10.000\t0xC6A9000D 30.000 100.000 12.500\t0\t0\t0\t-\t-\t-\t-\t-\n\
portal\tpublished\t999\tPlain Portal\t0xA9B40021 86.000 8.000 94.000\t0xC6A9000D 30.000 100.000 12.500\t0\t0\t0\t-\t-\t-\t-\t-\n\
portal\tpublished\t1000\tRanged Portal \\\\ One\t0xA9B40021 84.000 7.500 94.000\t0x01AC0118 60.000 -50.250 0.500\t0\t0\t0\t10\t50\t-\t-\t-\n";

fn snapshot() -> Option<StatusSnapshot> {
    None
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("utf-8 response")
}

#[test]
fn the_portal_table_lists_placed_portals_in_order_with_land_cells_limits_and_escaped_names() {
    assert_eq!(
        portal_table::document("Test\tWorld \\ One", &world()),
        EXPECTED
    );
}

#[test]
fn portals_are_not_served_unless_publishing_is_on() {
    assert!(
        !empyrean_common::master_configuration::MasterConfiguration::default()
            .server
            .publish_portals,
        "off by default"
    );
    let off = text(&respond("GET /portals HTTP/1.1", snapshot));
    assert!(off.starts_with("HTTP/1.0 404 Not Found\r\n"), "{off}");
    let off = text(&respond_with_portals(
        "GET /portals HTTP/1.1",
        snapshot,
        None,
    ));
    assert!(off.starts_with("HTTP/1.0 404 Not Found\r\n"), "{off}");

    let config = empyrean_common::toml_config::from_toml_str("[server]\npublish_portals = true\n")
        .unwrap()
        .config;
    assert!(
        config.server.publish_portals,
        "the key is server.publish_portals"
    );
}

#[test]
fn published_portals_are_served_as_tab_separated_values() {
    let table = PortalTable::new("Test\tWorld \\ One", Arc::new(world()));
    assert_eq!(
        table.document(),
        EXPECTED,
        "built once, off the endpoint's thread"
    );
    let r = text(&respond_with_portals(
        "GET /portals HTTP/1.1",
        snapshot,
        Some(&table),
    ));
    assert!(r.starts_with("HTTP/1.0 200 OK\r\n"), "{r}");
    assert!(
        r.contains("Content-Type: text/tab-separated-values; charset=utf-8\r\n"),
        "{r}"
    );
    assert!(
        r.contains(&format!("Content-Length: {}\r\n", EXPECTED.len())),
        "{r}"
    );
    assert!(r.ends_with(&format!("\r\n\r\n{EXPECTED}")), "{r}");
    // The other paths are unchanged.
    assert!(text(&respond_with_portals(
        "GET /nope HTTP/1.1",
        snapshot,
        Some(&table)
    ))
    .starts_with("HTTP/1.0 404 "));
    let head = text(&respond_with_portals(
        "HEAD /portals HTTP/1.1",
        snapshot,
        Some(&table),
    ));
    assert!(head.ends_with("\r\n\r\n"), "HEAD has no body: {head}");
}

#[test]
fn the_portal_table_is_built_off_the_endpoint_and_answered_as_not_ready_until_it_is() {
    let (release, gate) = std::sync::mpsc::channel::<()>();
    let table = PortalTable::building(move || {
        let _ = gate.recv_timeout(std::time::Duration::from_secs(5));
        portal_table::documents("Test\tWorld \\ One", &world())
    });
    // While it is being built, the endpoint answers without waiting for it: the table is not
    // ready, and the other paths are as ever.
    let r = text(&respond_with_portals(
        "GET /portals HTTP/1.1",
        snapshot,
        Some(&table),
    ));
    assert!(r.starts_with("HTTP/1.0 503 "), "{r}");
    let r = text(&respond_with_portals(
        "GET /places HTTP/1.1",
        snapshot,
        Some(&table),
    ));
    assert!(r.starts_with("HTTP/1.0 503 "), "{r}");
    assert!(text(&respond_with_portals(
        "GET /health HTTP/1.1",
        snapshot,
        Some(&table)
    ))
    .starts_with("HTTP/1.0 503 "));
    release.send(()).unwrap();
    assert_eq!(table.document(), EXPECTED);
    for path in ["/portals", "/places"] {
        let r = text(&respond_with_portals(
            &format!("GET {path} HTTP/1.1"),
            snapshot,
            Some(&table),
        ));
        assert!(r.starts_with("HTTP/1.0 200 OK\r\n"), "{path}: {r}");
    }
}

/// [`world`], with the other places a world puts down, and things that are not places.
fn places_world() -> MemContent {
    let person = |class_id: u32, name: &str, weenie_type: WeenieType| {
        Weenie::new(class_id, name, weenie_type)
            .with_string(PropertyString::Name, name)
            .with_int(PropertyInt::ItemType, 0x10)
            .with_bool(PropertyBool::Attackable, false)
    };
    world()
        .weenie(
            Weenie::new(509, "lifestone", WeenieType::LifeStone)
                .with_string(PropertyString::Name, "Life Stone"),
        )
        .weenie(person(1100, "Shopkeep", WeenieType::Vendor))
        .weenie(person(1101, "Town Crier", WeenieType::Creature))
        // A creature with nothing said of attacking it may be attacked: a monster.
        .weenie(
            Weenie::new(1102, "drudge", WeenieType::Creature)
                .with_string(PropertyString::Name, "Drudge")
                .with_int(PropertyInt::ItemType, 0x10),
        )
        .weenie(
            person(1103, "Hidden Watcher", WeenieType::Creature)
                .with_bool(PropertyBool::UiHidden, true),
        )
        .weenie(person(1104, "Cow", WeenieType::Cow))
        // A creature weenie whose item type is not a creature's.
        .weenie(
            Weenie::new(1105, "statue", WeenieType::Creature)
                .with_string(PropertyString::Name, "Statue")
                .with_int(PropertyInt::ItemType, 0x80)
                .with_bool(PropertyBool::Attackable, false),
        )
        .landblock_instance(LandblockInstance::new(
            0x7A9B_4003,
            509,
            0xA9B4_0021,
            [80.0, 5.0, 94.0],
        ))
        .landblock_instance(LandblockInstance::new(
            0x7A9B_4004,
            1100,
            0xA9B4_0021,
            [82.0, 6.0, 94.0],
        ))
        .landblock_instance(LandblockInstance::new(
            0x7A9B_4005,
            1101,
            0xA9B4_0021,
            [83.0, 9.0, 94.0],
        ))
        .landblock_instance(LandblockInstance::new(
            0x7A9B_4006,
            1102,
            0xA9B4_0021,
            [90.0, 9.0, 94.0],
        ))
        .landblock_instance(LandblockInstance::new(
            0x7A9B_4007,
            1103,
            0xA9B4_0021,
            [91.0, 9.0, 94.0],
        ))
        .landblock_instance(LandblockInstance::new(
            0x7123_4004,
            1104,
            0x1234_0002,
            [40.0, 40.0, 0.0],
        ))
        .landblock_instance(LandblockInstance::new(
            0x7123_4005,
            1105,
            0x1234_0002,
            [41.0, 40.0, 0.0],
        ))
}

const PLACES: &str = "dereth-places-store\t1\n\
world\tTest World \\\\ One\n\
place\tportal\tpublished\t1001\tNowhere\t0x12340002 30.000 30.000 0.000\t0\t0\t-\t-\n\
place\tnpc\tpublished\t1104\tCow\t0x12340002 40.000 40.000 0.000\t0\t0\t-\t-\n\
place\tportal\tpublished\t999\tPlain Portal\t0x12340039 191.900 0.500 10.000\t0\t0\t-\t0xC6A9000D 30.000 100.000 12.500\n\
place\tlifestone\tpublished\t509\tLife Stone\t0xA9B40021 80.000 5.000 94.000\t0\t0\t-\t-\n\
place\tportal\tpublished\t999\tPlain Portal\t0xA9B40021 86.000 8.000 94.000\t0\t0\t-\t0xC6A9000D 30.000 100.000 12.500\n\
place\tportal\tpublished\t1000\tRanged Portal \\\\ One\t0xA9B40021 84.000 7.500 94.000\t0\t0\t-\t0x01AC0118 60.000 -50.250 0.500\n\
place\tvendor\tpublished\t1100\tShopkeep\t0xA9B40021 82.000 6.000 94.000\t0\t0\t-\t-\n\
place\tnpc\tpublished\t1101\tTown Crier\t0xA9B40021 83.000 9.000 94.000\t0\t0\t-\t-\n";

#[test]
fn the_places_list_lifestones_portals_vendors_and_people_but_no_monster_or_hidden_thing() {
    let content = places_world();
    assert_eq!(
        portal_table::places_document("Test\tWorld \\ One", &content),
        PLACES
    );
    assert_eq!(
        portal_table::document("Test\tWorld \\ One", &content),
        EXPECTED,
        "the portal table is unchanged by the other places"
    );
}

#[test]
fn the_places_read_as_the_clients_place_store_every_one_published() {
    let store = PlaceStore::from_text(PLACES).expect("the client reads the places");
    assert_eq!(store.world, "Test World \\ One");
    let places = store.places();
    assert_eq!(places.len(), 8);
    assert!(places.iter().all(|p| p.source == EdgeSource::Published));
    assert!(places.iter().all(|p| p.owner.is_none()));
    let kinds: Vec<PlaceKind> = places.iter().map(|p| p.kind).collect();
    assert_eq!(
        kinds,
        [
            PlaceKind::Portal,
            PlaceKind::Npc,
            PlaceKind::Portal,
            PlaceKind::Lifestone,
            PlaceKind::Portal,
            PlaceKind::Portal,
            PlaceKind::Vendor,
            PlaceKind::Npc,
        ]
    );
    let ranged = &places[5];
    assert_eq!(
        (ranged.name.as_str(), ranged.wcid),
        ("Ranged Portal \\ One", Some(1000))
    );
    assert_eq!(ranged.to.map(|t| t.cell.0), Some(0x01AC_0118));
    assert_eq!(places[0].to, None, "a portal that leads nowhere it says");
}

#[test]
fn the_places_are_served_and_the_status_document_names_both_documents() {
    let status = || {
        Some(StatusSnapshot {
            world_name: "W".to_owned(),
            version: "0".to_owned(),
            source_url: String::new(),
            uptime_seconds: 0,
            world_open: true,
            shutting_down: false,
            connections: 0,
            authenticated_connections: 0,
            players_online: 0,
            landblocks_loaded: 0,
            content_hash: None,
            corrections_digest: String::new(),
            era: "eor".to_owned(),
            features: empyrean_common::era::EraFeatures::default(),
            dats: empyrean_server::status_endpoint::DatIterations::default(),
            dat_patching: false,
            client_versions: Vec::new(),
            websocket_url: None,
            portals: None,
            places: None,
            not_ported: Vec::new(),
        })
    };
    let off = text(&respond_with_portals("GET /status HTTP/1.1", status, None));
    assert!(
        off.contains("\"portals\":null,\"places\":null"),
        "nothing named while off: {off}"
    );
    assert!(text(&respond("GET /places HTTP/1.1", snapshot)).starts_with("HTTP/1.0 404 "));

    let table = PortalTable::new("Test\tWorld \\ One", Arc::new(places_world()));
    assert_eq!(table.places(), PLACES);
    let on = text(&respond_with_portals(
        "GET /status HTTP/1.1",
        status,
        Some(&table),
    ));
    assert!(
        on.contains("\"portals\":\"/portals\",\"places\":\"/places\""),
        "{on}"
    );
    let r = text(&respond_with_portals(
        "GET /places HTTP/1.1",
        snapshot,
        Some(&table),
    ));
    assert!(r.starts_with("HTTP/1.0 200 OK\r\n"), "{r}");
    assert!(
        r.contains("Content-Type: text/tab-separated-values; charset=utf-8\r\n"),
        "{r}"
    );
    assert!(r.ends_with(&format!("\r\n\r\n{PLACES}")), "{r}");
}

/// A world of a quest-gated portal to a town, a spawner that makes a portal while its event runs
/// (the event is off), the town as a named landmark, and the town's vendors.
fn asking_world() -> MemContent {
    let town = [84.0, 7.1, 94.0];
    let mut spawner = Weenie::new(3000, "gen", WeenieType::Generic)
        .with_string(PropertyString::GeneratorEvent, "Snowreap");
    spawner
        .weenie_properties_generator
        .push(WeeniePropertiesGenerator {
            id: 1,
            object_id: 3000,
            probability: -1.0,
            weenie_class_id: 2001,
            delay: None,
            init_create: 1,
            max_create: 1,
            when_create: 2,
            where_create: 4,
            stack_size: None,
            palette_id: None,
            shade: None,
            obj_cell_id: Some(0xA9B4_0022),
            origin_x: Some(80.0),
            origin_y: Some(30.0),
            origin_z: Some(5.0),
            angles_w: Some(1.0),
            angles_x: Some(0.0),
            angles_y: Some(0.0),
            angles_z: Some(0.0),
        });
    let mut content = MemContent::new()
        .weenie(
            Weenie::new(2000, "portalholtburg", WeenieType::Portal)
                .with_string(PropertyString::Name, "Portal to Holtburg")
                .with_string(PropertyString::QuestRestriction, "HoltburgFlag")
                .with_int(PropertyInt::PortalBitmask, 0x1 | 0x2)
                .with_position(PositionType::Destination, 0xA9B4_0019, town, NORTH),
        )
        .weenie(
            Weenie::new(2001, "portalfestival", WeenieType::Portal)
                .with_string(PropertyString::Name, "Festival")
                .with_position(
                    PositionType::Destination,
                    0x01AC_0118,
                    [60.0, -50.25, 0.5],
                    NORTH,
                ),
        )
        .weenie(spawner)
        .weenie(
            Weenie::new(1100, "shopkeep", WeenieType::Vendor)
                .with_string(PropertyString::Name, "Shopkeep"),
        )
        .event(Event {
            id: 1,
            name: "Snowreap".into(),
            start_time: -1,
            end_time: -1,
            state: 3,
            last_modified: Default::default(),
        })
        .point_of_interest(PointsOfInterest {
            id: 1,
            name: "Holtburg".into(),
            weenie_class_id: 2000,
            last_modified: Default::default(),
        })
        .landblock_instance(LandblockInstance::new(
            0x7123_4001,
            2000,
            0x1234_0002,
            [30.0, 30.0, 0.0],
        ))
        .landblock_instance(LandblockInstance::new(
            0x7A9B_4001,
            3000,
            0xA9B4_0021,
            [84.0, 7.5, 94.0],
        ));
    for (i, cell) in [0xA9B4_0021, 0xA9B3_0001, 0xAAB4_0001]
        .into_iter()
        .enumerate()
    {
        content = content.landblock_instance(LandblockInstance::new(
            0x7A9B_4100 + u32::try_from(i).unwrap(),
            1100,
            cell,
            [10.0, 10.0, 0.0],
        ));
    }
    content
}

#[test]
fn the_portal_table_says_what_each_portal_asks_and_lists_the_portals_spawners_make() {
    let content = asking_world();
    let table = portal_table::document("W", &content);
    assert_eq!(
        table,
        "dereth-navigation-store\t3\nworld\tW\n\
portal\tpublished\t2000\tPortal to Holtburg\t0x12340002 30.000 30.000 0.000\t0xA9B40019 84.000 7.100 94.000\t0\t0\t0\t-\t-\t-\tquest HoltburgFlag;no-pk\t-\n\
portal\tpublished\t2001\tFestival\t0xA9B40022 80.000 30.000 5.000\t0x01AC0118 60.000 -50.250 0.500\t0\t0\t0\t-\t-\t-\tevent Snowreap off\t-\n"
    );
    // The client reads what each asks.
    let store = dereth_navigation::PortalStore::from_text(&table).expect("the client reads it");
    let flagless = dereth_navigation::Traveller {
        quests: Some(std::collections::BTreeSet::new()),
        ..dereth_navigation::Traveller::at_level(Some(20))
    };
    assert!(store.routable_for(&flagless).is_empty());
    let places = PlaceStore::from_text(&portal_table::places_document("W", &content))
        .expect("the client reads the places");
    let named = |kind: PlaceKind| {
        places
            .places()
            .iter()
            .filter(|p| p.kind == kind)
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(named(PlaceKind::Portal), ["Portal to Holtburg", "Festival"]);
    assert_eq!(named(PlaceKind::Town), ["Holtburg"]);
    assert_eq!(named(PlaceKind::Vendor).len(), 3);
}

/// A world's own places document, as an overlay folder holds it.
const OWN_PLACES: &str = "dereth-places-store\t1\n\
world\tElsewhere\n\
place\ttown\tpublished\t-\tCragstone\t0x5D8D0013 114.000 48.000 15.000\t0\t0\t-\t-\n\
place\tlandmark\tpublished\t-\tThe Hovel (Dungeon, Level 4-7)\t0x6291000A 94.000 70.000 22.500\t0\t0\t-\t-\n";

/// A fresh, empty folder of its own for the test `name`.
fn folder(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("empyrean-own-places-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("overlay")).unwrap();
    dir
}

fn serving_overlay() -> empyrean_common::master_configuration::MasterConfiguration {
    let mut config = empyrean_common::master_configuration::MasterConfiguration::default();
    config.dat_overlay.path = "overlay".to_owned();
    config
}

#[test]
fn a_worlds_own_places_are_published_in_place_of_its_places() {
    let dir = folder("set");
    std::fs::write(
        dir.join("overlay")
            .join(empyrean_server::dat_directory::WORLD_PLACES_FILE),
        OWN_PLACES,
    )
    .unwrap();
    let base = empyrean_common::config_paths::PathBase::new(None, &dir, None, None);
    let own = empyrean_server::dat_directory::world_places(&serving_overlay(), &base).unwrap();
    assert_eq!(own.as_deref(), Some(OWN_PLACES));
    let table = PortalTable::with_own_places("Test\tWorld \\ One", Arc::new(places_world()), own);
    let own_lines: String = OWN_PLACES
        .lines()
        .skip(2)
        .map(|l| format!("{l}\n"))
        .collect();
    let header: String = PLACES.lines().take(2).map(|l| format!("{l}\n")).collect();
    assert_eq!(
        table.places(),
        format!("{header}naming\town\n{own_lines}"),
        "after the header, a line saying its towns and landmarks are its own names"
    );
    let store = PlaceStore::from_text(table.places()).expect("the client reads the places");
    assert!(
        store.own_names,
        "the client reads them as the world's own names"
    );
    assert_eq!(
        store.world, "Test World \\ One",
        "the world keeps its own name"
    );
    let named: Vec<(PlaceKind, &str)> = store
        .places()
        .iter()
        .filter(|p| matches!(p.kind, PlaceKind::Town | PlaceKind::Landmark))
        .map(|p| (p.kind, p.name.as_str()))
        .collect();
    assert_eq!(
        named,
        [
            (PlaceKind::Town, "Cragstone"),
            (PlaceKind::Landmark, "The Hovel (Dungeon, Level 4-7)")
        ]
    );
    assert_eq!(
        table.document(),
        EXPECTED,
        "the portal table is unchanged by the world's own places"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_its_own_places_file_a_world_publishes_only_its_places() {
    let dir = folder("absent");
    let base = empyrean_common::config_paths::PathBase::new(None, &dir, None, None);
    let own = empyrean_server::dat_directory::world_places(&serving_overlay(), &base).unwrap();
    assert_eq!(own, None);
    let none = empyrean_common::master_configuration::MasterConfiguration::default();
    assert_eq!(
        empyrean_server::dat_directory::world_places(&none, &base),
        Ok(None),
        "a world with no overlay has none"
    );
    let table = PortalTable::with_own_places("Test\tWorld \\ One", Arc::new(places_world()), own);
    assert_eq!(table.places(), PLACES);
    assert_eq!(
        portal_table::with_own_places(PLACES, "not a places document\nplace\tx\n"),
        PLACES,
        "a document of another kind changes nothing"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_world_that_places_its_own_things_publishes_only_its_own_places_and_no_portals() {
    let content = places_world();
    empyrean_content::WorldDatabase::hide_placements(&content);
    let table = PortalTable::with_own_places(
        "Test\tWorld \\ One",
        Arc::new(content),
        Some(OWN_PLACES.to_owned()),
    );
    let store = PlaceStore::from_text(table.places()).expect("the client reads the places");
    let named: Vec<&str> = store.places().iter().map(|p| p.name.as_str()).collect();
    assert_eq!(named, ["Cragstone", "The Hovel (Dungeon, Level 4-7)"]);
    assert!(store.own_names);
    assert!(
        !table.document().contains("\nportal\t"),
        "no placed portal of the world database's: {}",
        table.document()
    );
    let retail = PortalTable::new("Test\tWorld \\ One", Arc::new(places_world()));
    assert_eq!(
        retail.places(),
        PLACES,
        "a retail world's places are unchanged"
    );
    assert!(retail.document().contains("\nportal\t"));
    let retail = PlaceStore::from_text(retail.places()).expect("the client reads the places");
    assert!(
        !retail.own_names,
        "a retail world's towns and landmarks are not its own names"
    );
}

#[test]
fn an_era_that_names_its_own_places_publishes_its_towns_and_landmarks_as_the_worlds_own_names() {
    use empyrean_common::era::{EOR, INFILTRATION, LAUNCH};
    let towns = |store: &PlaceStore| -> Vec<(PlaceKind, String)> {
        store
            .places()
            .iter()
            .filter(|p| matches!(p.kind, PlaceKind::Town | PlaceKind::Landmark))
            .map(|p| (p.kind, p.name.clone()))
            .collect()
    };
    let unmarked = portal_table::places_document("W", &asking_world());
    for era in [&INFILTRATION, &LAUNCH] {
        let table = PortalTable::of_world("W", Arc::new(asking_world()), None, era);
        let mut lines = unmarked.lines();
        let head: String = lines.by_ref().take(2).map(|l| format!("{l}\n")).collect();
        let rest: String = lines.map(|l| format!("{l}\n")).collect();
        assert_eq!(
            table.places(),
            format!("{head}naming\town\n{rest}"),
            "{:?}: the world database's places, marked after the header as the world's own names",
            era.id
        );
        let store = PlaceStore::from_text(table.places()).expect("the client reads the places");
        assert!(
            store.own_names,
            "{:?}: the client reads them as its own",
            era.id
        );
        assert_eq!(towns(&store), [(PlaceKind::Town, "Holtburg".to_owned())]);
        assert_eq!(
            table.document(),
            portal_table::document("W", &asking_world()),
            "the portal table is unchanged"
        );
    }

    let retail = PortalTable::of_world("W", Arc::new(asking_world()), None, &EOR);
    assert_eq!(
        retail.places(),
        unmarked,
        "an end-of-retail world's places are unchanged"
    );
    let store = PlaceStore::from_text(retail.places()).expect("the client reads the places");
    assert!(
        !store.own_names,
        "an end-of-retail world's towns and landmarks leave the maps their notes"
    );

    let own = PortalTable::of_world(
        "W",
        Arc::new(asking_world()),
        Some(OWN_PLACES.to_owned()),
        &INFILTRATION,
    );
    let store = PlaceStore::from_text(own.places()).expect("the client reads the places");
    assert!(store.own_names);
    assert_eq!(
        towns(&store),
        [
            (PlaceKind::Town, "Cragstone".to_owned()),
            (
                PlaceKind::Landmark,
                "The Hovel (Dungeon, Level 4-7)".to_owned()
            )
        ],
        "the world's own places document wins over its era's names"
    );
    assert_eq!(
        portal_table::marked_own(&portal_table::marked_own(&unmarked)),
        portal_table::marked_own(&unmarked),
        "marked once"
    );
}
