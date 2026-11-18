//! Behaviour: none (the builder's reading of a world database into its portal table and places).

use super::*;
use crate::content::{id, spawn_where, Spawn, Spot, Weenie, WorldContent};

const PORTAL: u32 = id::kind::PORTAL;

fn portal(wcid: u32, name: &str, to: Option<Spot>) -> Weenie {
    let w = Weenie::new(wcid, &format!("portal{wcid}"), PORTAL).with_string(id::string::NAME, name);
    match to {
        Some(to) => w.with_position(id::position::DESTINATION, to),
        None => w,
    }
}

fn spawner(wcid: u32, spawns: Vec<Spawn>) -> Weenie {
    let mut w = Weenie::new(wcid, &format!("gen{wcid}"), id::kind::GENERIC);
    w.spawns = spawns;
    w
}

fn spawn(wcid: u32, where_create: u32, at: Spot) -> Spawn {
    Spawn {
        wcid,
        probability: -1.0,
        init_create: 1,
        max_create: 1,
        when_create: 2,
        where_create,
        at,
    }
}

const DUNGEON: Spot = Spot::at(0x01AC_0118, [60.0, -50.25, 0.5]);

fn line(table: &str, wcid: u32) -> Vec<Vec<String>> {
    table
        .lines()
        .filter(|l| l.starts_with("portal\t"))
        .map(|l| l.split('\t').map(str::to_owned).collect::<Vec<_>>())
        .filter(|f| f[2] == wcid.to_string())
        .collect()
}

#[test]
fn a_portal_leads_where_its_weenie_says_else_to_its_marker_else_by_its_offset() {
    let world = WorldContent::new()
        .with_weenie(portal(1000, "Marked", None))
        .with_weenie(Weenie::new(1, "portaldestination", id::kind::GENERIC))
        .with_weenie(portal(1001, "Offset", None).with_position(
            id::position::RELATIVE_DESTINATION,
            Spot::at(0, [10.0, -20.0, 1.0]),
        ))
        .with_weenie(portal(1002, "Nowhere", None))
        // The portal, linked to the marker that says where it leads.
        .with_placement(
            0x7A9B_4001,
            1000,
            Spot::at(0xA9B4_0021, [84.0, 7.5, 94.0]),
            &[0x7A9B_4002],
        )
        .with_placement(
            0x7A9B_4002,
            1,
            Spot::at(0x0007_0143, [70.0, -60.0, 0.0]),
            &[],
        )
        // Its offset carries it over the landblock's south edge.
        .with_placement(
            0x7A9B_4003,
            1001,
            Spot::at(0xA9B4_0001, [5.0, 5.0, 10.0]),
            &[],
        )
        .with_placement(
            0x7A9B_4004,
            1002,
            Spot::at(0xA9B4_0001, [6.0, 6.0, 10.0]),
            &[],
        );
    let table = WorldPlaces::of(&world).portal_table("W", Source::Published);
    assert!(
        table.starts_with("dereth-navigation-store\t2\nworld\tW\n"),
        "{table}"
    );
    assert_eq!(line(&table, 1000)[0][5], "0x00070143 70.000 -60.000 0.000");
    assert_eq!(line(&table, 1001)[0][5], "0xA9B30008 15.000 177.000 11.000");
    assert!(
        line(&table, 1002).is_empty(),
        "a portal leading nowhere is left out"
    );
    // The places say the same of where each leads.
    let places = WorldPlaces::of(&world).places_document("W", Source::Hint);
    assert!(places.contains("place\tportal\thint\t1000\tMarked\t0xA9B40021 84.000 7.500 94.000\t0\t0\t-\t0x00070143 70.000 -60.000 0.000\n"), "{places}");
}

#[test]
fn spawners_make_portals_at_their_points_offsets_and_scatters_and_through_spawners_they_make() {
    // Turned a quarter about z: an offset of +10 east is +10 north.
    let s = std::f32::consts::FRAC_1_SQRT_2;
    let turned = Spot {
        rotation: [s, 0.0, 0.0, s],
        ..Spot::at(0xA9B4_000A, [30.0, 30.0, 10.0])
    };
    let world = WorldContent::new()
        .with_weenie(portal(2000, "At a point", Some(DUNGEON)))
        .with_weenie(portal(2001, "Offset", Some(DUNGEON)))
        .with_weenie(portal(2002, "Scattered", Some(DUNGEON)))
        .with_weenie(portal(2003, "Deep", Some(DUNGEON)))
        .with_weenie(portal(2004, "In a chest", Some(DUNGEON)))
        .with_weenie(spawner(
            3000,
            vec![
                spawn(
                    2000,
                    spawn_where::SPECIFIC,
                    Spot::at(0xA9B4_0022, [80.0, 30.0, 5.0]),
                ),
                spawn(2001, spawn_where::SPECIFIC, Spot::at(0, [10.0, 0.0, 0.0])),
                spawn(2002, spawn_where::SCATTER, Spot::at(0, [0.0, 0.0, 2.0])),
                spawn(3001, spawn_where::SPECIFIC, Spot::at(0, [0.0, 0.0, 0.0])),
                spawn(2004, spawn_where::CONTAIN, Spot::at(0, [0.0; 3])),
                // The same entry twice makes one portal.
                spawn(2002, spawn_where::SCATTER, Spot::at(0, [0.0, 0.0, 2.0])),
            ],
        ))
        .with_weenie(spawner(
            3001,
            vec![spawn(2003, spawn_where::ON_TOP, Spot::at(0, [0.0; 3]))],
        ))
        .with_placement(0x7A9B_4100, 3000, turned, &[]);
    let places = WorldPlaces::of(&world);
    let from = |wcid: u32| {
        places
            .portals
            .iter()
            .filter(|p| p.wcid == wcid)
            .map(|p| (position(&p.from), p.standing))
            .collect::<Vec<_>>()
    };
    let by = Standing::Spawned(0x7A9B_4100);
    assert_eq!(
        from(2000),
        [("0xA9B40022 80.000 30.000 5.000".to_owned(), by)]
    );
    assert_eq!(
        from(2001),
        [("0xA9B4000A 30.000 40.000 10.000".to_owned(), by)]
    );
    assert_eq!(
        from(2002),
        [("0xA9B4000A 30.000 30.000 12.000".to_owned(), by)]
    );
    assert_eq!(
        from(2003),
        [("0xA9B4000A 30.000 30.000 10.000".to_owned(), by)]
    );
    assert!(
        from(2004).is_empty(),
        "made inside a container, it stands nowhere"
    );
    // Each is a portal place too.
    let doc = places.places_document("W", Source::Published);
    assert_eq!(doc.matches("place\tportal\t").count(), 4, "{doc}");
}

#[test]
fn a_portal_says_whom_it_admits_and_when_it_stands() {
    let world = WorldContent::new()
        .with_event("Snowreap", 3)
        .with_event("FairRunning", 4)
        // Admits no one.
        .with_weenie(portal(1, "Closed", Some(DUNGEON)).with_int(id::int::PORTAL_BITMASK, 0))
        // Turns away player killers and their lite kind, vitae and new accounts; asks a flag.
        .with_weenie(
            portal(2, "Arena", Some(DUNGEON))
                .with_int(
                    id::int::PORTAL_BITMASK,
                    0x1 | 0x2 | 0x4 | 0x100 | 0x200 | 0x10,
                )
                .with_string(id::string::QUEST_RESTRICTION, "ArenaFlag")
                .with_int(id::int::MIN_LEVEL, 40)
                .with_int(id::int::MAX_LEVEL, 80),
        )
        .with_weenie(
            portal(3, "Advocates", Some(DUNGEON))
                .with_bool(id::bool::ADVOCATE_QUEST, true)
                .with_int(id::int::ACCOUNT_REQUIREMENTS, 3)
                .with_int(id::int::LIFESPAN, 300),
        )
        // Placed under a spawner that runs with an event that is off, and at night.
        .with_weenie(portal(4, "Festival", Some(DUNGEON)))
        .with_weenie(
            spawner(
                5,
                vec![spawn(4, spawn_where::SPECIFIC, Spot::at(0, [0.0; 3]))],
            )
            .with_string(id::string::GENERATOR_EVENT, "Snowreap")
            .with_int(id::int::GENERATOR_TIME_TYPE, 4),
        )
        // A spawner whose event runs, and one whose event the database does not hold.
        .with_weenie(portal(6, "Fair", Some(DUNGEON)))
        .with_weenie(
            spawner(
                7,
                vec![spawn(6, spawn_where::ON_TOP, Spot::at(0, [0.0; 3]))],
            )
            .with_string(id::string::GENERATOR_EVENT, "fairrunning"),
        )
        .with_weenie(portal(8, "Plain", Some(DUNGEON)))
        .with_weenie(
            spawner(
                9,
                vec![spawn(8, spawn_where::ON_TOP, Spot::at(0, [0.0; 3]))],
            )
            .with_string(id::string::GENERATOR_EVENT, "NoSuchEvent"),
        )
        .with_placement(10, 1, Spot::at(0x0101_0101, [1.0; 3]), &[])
        .with_placement(11, 2, Spot::at(0x0101_0102, [1.0; 3]), &[])
        .with_placement(12, 3, Spot::at(0x0101_0103, [1.0; 3]), &[])
        .with_placement(13, 5, Spot::at(0x0101_0104, [1.0; 3]), &[14])
        .with_placement(14, 4, Spot::at(0x0101_0105, [1.0; 3]), &[])
        .with_placement(15, 7, Spot::at(0x0101_0106, [1.0; 3]), &[])
        .with_placement(16, 9, Spot::at(0x0101_0107, [1.0; 3]), &[]);
    let table = WorldPlaces::of(&world).portal_table("W", Source::Published);
    let requires = |wcid: u32| {
        line(&table, wcid)
            .into_iter()
            .map(|f| (f[9].clone(), f[10].clone(), f[12].clone()))
            .collect::<Vec<_>>()
    };
    let only = |a: &str, b: &str, c: &str| vec![(a.to_owned(), b.to_owned(), c.to_owned())];
    assert_eq!(requires(1), only("-", "-", "closed"));
    assert_eq!(
        requires(2),
        only(
            "40",
            "80",
            "quest ArenaFlag;no-pk;no-pklite;no-vitae;account-age"
        )
    );
    assert_eq!(requires(3), only("-", "-", "account 3;advocate;temporary"));
    // The portal the spawner makes and the one linked under it both ask for its event and the
    // night.
    assert_eq!(
        requires(4),
        [
            ("-".into(), "-".into(), "event Snowreap off;night".into()),
            ("-".into(), "-".into(), "event Snowreap off;night".into())
        ]
    );
    assert_eq!(requires(6), only("-", "-", "event fairrunning on"));
    assert_eq!(requires(8), only("-", "-", "-"));
}

#[test]
fn towns_are_named_landmarks_and_the_named_drop_points_vendors_stand_round() {
    let holtburg = Spot::at(0xA9B4_0019, [84.0, 7.1, 94.0]);
    let castle = Spot::at(0xBDD0_0006, [16.9, 120.5, 30.0]);
    let yaraq = Spot::at(0x7D64_000D, [31.9, 104.6, 10.0]);
    let mut world = WorldContent::new()
        .with_weenie(portal(1, "Portal to Holtburg", Some(holtburg)))
        .with_weenie(portal(2, "Bandit Castle Portal", Some(castle)))
        .with_weenie(portal(3, "Yaraq Portal", Some(yaraq)))
        // A dungeon's exit near Yaraq, named for no one place.
        .with_weenie(portal(4, "Surface", Some(Spot::at(0x7D65_0001, [1.0; 3]))))
        .with_weenie(portal(5, "Surface", Some(Spot::at(0x1010_0001, [1.0; 3]))))
        // Named for a place within two landblocks of Yaraq, by one portal of four.
        .with_weenie(portal(
            6,
            "Yaraq Outskirts Portal",
            Some(Spot::at(0x7D65_0002, [1.0; 3])),
        ))
        .with_weenie(Weenie::new(7, "vendor", id::kind::VENDOR))
        // A bare name: a town where vendors stand round its drop point, nothing where none do.
        .with_weenie(portal(
            8,
            "Danby's Outpost",
            Some(Spot::at(0x5A9C_0004, [23.5, 77.1, 6.0])),
        ))
        .with_weenie(portal(
            9,
            "Lonely Hut",
            Some(Spot::at(0x2020_0001, [1.0; 3])),
        ))
        .with_landmark("Holtburg", 1)
        .with_landmark("Bandit Castle", 2);
    let mut guid = 100;
    let mut place = |w: WorldContent, wcid: u32, cell: u32| {
        guid += 1;
        w.with_placement(guid, wcid, Spot::at(cell, [10.0; 3]), &[])
    };
    for wcid in [3, 3, 3, 4, 5, 6, 8, 9] {
        world = place(world, wcid, 0x0101_0100 + wcid);
    }
    for cell in [
        0xA9B4_0001,
        0xA9B3_0001,
        0xAAB5_0001,
        0x7D64_0001,
        0x7D63_0001,
        0x7C65_0001,
        0x5A9C_0001,
        0x5A9B_0001,
        0x5B9C_0001,
    ] {
        world = place(world, 7, cell);
    }
    let towns = WorldPlaces::of(&world).towns;
    let got: Vec<(TownKind, &str, String, usize, bool)> = towns
        .iter()
        .map(|t| {
            (
                t.kind,
                t.name.as_str(),
                position(&t.at),
                t.vendors,
                t.landmark,
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            (
                TownKind::Landmark,
                "Bandit Castle",
                position(&castle),
                0,
                true
            ),
            (
                TownKind::Town,
                "Danby's Outpost",
                "0x5A9C0004 23.500 77.100 6.000".to_owned(),
                3,
                false
            ),
            (TownKind::Town, "Holtburg", position(&holtburg), 3, true),
            (TownKind::Town, "Yaraq", position(&yaraq), 3, false),
        ]
    );
    let doc = WorldPlaces::of(&world).places_document("W", Source::Hint);
    assert!(
        doc.contains("place\ttown\thint\t-\tYaraq\t0x7D64000D 31.900 104.600 10.000\t0\t0\t-\t-\n"),
        "{doc}"
    );
    assert!(
        doc.contains("place\tlandmark\thint\t-\tBandit Castle\t"),
        "{doc}"
    );
}
