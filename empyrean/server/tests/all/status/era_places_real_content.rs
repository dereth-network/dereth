//! Divergence: V458
//! The places a launcher builds for a world from the public database of the world's era: the
//! server publishes the same documents of the same content, byte for byte; each era's database
//! gives its own portals, what they ask and its towns at their drop points; and where each era's
//! database is fetched from is where the server's import tool fetches it.
//! Fixture: ACE's world-database dump and the `world.pack` built from it (`EMPYREAN_WORLD_SQL`,
//! `EMPYREAN_TEST_WORLD_PACK`), the February 2005 dump and the Launch world's SQL in the per-user
//! world-database cache (`EMPYREAN_TEST_INFILTRATION_SQL`, `EMPYREAN_TEST_LAUNCH_SQL`), with the
//! `real-content` feature.

use std::path::PathBuf;

use dereth_navigation::{PlaceKind, PlaceStore, PortalStore, Requirement, Traveller};
use dereth_world_places::eras::{self, EraDatabase};
use dereth_world_places::{sql, Source, WorldContent, WorldPlaces};
use empyrean_common::world_release;
use empyrean_content::WorldDatabase;
use empyrean_server::portal_table;

/// The era's database: `var`, else the release the era names in the per-user cache (the newest
/// one there for an era that follows its newest).
fn database(db: &EraDatabase, var: &str) -> PathBuf {
    if let Some(p) = std::env::var_os(var).filter(|v| !v.is_empty()) {
        return PathBuf::from(p);
    }
    let dir = world_release::cache_dir().unwrap_or_default();
    let found = if db.pinned() {
        Some(dir.join(db.sql_name(db.tag))).filter(|p| p.is_file())
    } else {
        let mut sql: Vec<PathBuf> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(db.asset_prefix) && n.ends_with(".sql"))
            })
            .collect();
        sql.sort();
        sql.pop()
    };
    found.unwrap_or_else(|| {
        panic!(
            "the {} era's world database is not in {} ({var}; `cargo run -p dereth-places -- fetch --era {}` keeps it there)",
            db.era.name(),
            dir.display(),
            db.era.name()
        )
    })
}

fn read(path: &PathBuf) -> WorldContent {
    let file = std::fs::File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    sql::read(std::io::BufReader::with_capacity(1 << 20, file))
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn portal_line<'a>(table: &'a str, wcid: u32, from: &str) -> Vec<&'a str> {
    let wcid = wcid.to_string();
    table
        .lines()
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .find(|f| f[0] == "portal" && f[2] == wcid && f[4].starts_with(from))
        .unwrap_or_else(|| panic!("no portal {wcid} from {from}"))
}

fn towns(places: &WorldPlaces) -> Vec<(&str, String)> {
    places
        .towns
        .iter()
        .filter(|t| t.kind == dereth_world_places::towns::TownKind::Town)
        .map(|t| (t.name.as_str(), dereth_world_places::text::position(&t.at)))
        .collect()
}

fn town<'a>(towns: &'a [(&str, String)], name: &str) -> Option<&'a str> {
    towns
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, at)| at.as_str())
}

#[test]
fn the_server_publishes_what_a_launcher_builds_from_the_same_database() {
    if !cfg!(feature = "real-content") {
        return;
    }
    let pack_path = empyrean_common::test_paths::world_pack();
    let pack = empyrean_content::PackContent::open(&pack_path).unwrap_or_else(|e| {
        panic!(
            "the real-content tier needs world.pack at {} (EMPYREAN_TEST_WORLD_PACK): {e}",
            pack_path.display()
        )
    });
    let pack: &dyn WorldDatabase = &pack;
    let sql_path = empyrean_common::test_paths::world_sql();
    let started = std::time::Instant::now();
    let built = WorldPlaces::of(&read(&sql_path)).documents("Dereth", Source::Published);
    eprintln!("read and built from the dump in {:?}", started.elapsed());
    let started = std::time::Instant::now();
    let served = portal_table::documents("Dereth", pack);
    eprintln!("built from the pack in {:?}", started.elapsed());
    for (what, a, b) in [
        ("portal table", &served.portals, &built.portals),
        ("places", &served.places, &built.places),
    ] {
        if a != b {
            let first = a.lines().zip(b.lines()).position(|(x, y)| x != y);
            panic!(
                "the {what} differ ({} and {} lines), first at line {first:?}: {:?} / {:?}",
                a.lines().count(),
                b.lines().count(),
                first.and_then(|i| a.lines().nth(i)),
                first.and_then(|i| b.lines().nth(i)),
            );
        }
    }
    eprintln!(
        "identical: {} and {} bytes",
        served.portals.len(),
        served.places.len()
    );
}

#[test]
fn each_eras_database_gives_its_portals_what_they_ask_and_its_towns() {
    if !cfg!(feature = "real-content") {
        return;
    }
    // The drop point of Holtburg, and its hideout's portal, are the same in every era.
    let holtburg = "0xA9B40019 84.000 7.100 94.000";
    let hideout = |table: &str| {
        let f = portal_line(table, 2068, "0xA9B30002 ");
        assert_eq!(f[5], "0x019E0114 10.000 -40.000 0.000");
        (f[9].to_owned(), f[10].to_owned(), f[12].to_owned())
    };

    // End of retail.
    let eor = WorldPlaces::of(&read(&empyrean_common::test_paths::world_sql()));
    let table = eor.portal_table("Dereth", Source::Hint);
    assert_eq!(hideout(&table), ("-".into(), "-".into(), "-".into()));
    let vip = portal_line(&table, 38998, "0x10350027 ");
    assert_eq!(vip[5], "0xA9B2001D 84.000 102.000 94.000");
    assert_eq!(vip[12], "quest CasinoVIPPortalAccess");
    let spawned = eor
        .portals
        .iter()
        .filter(|p| {
            matches!(
                p.standing,
                dereth_world_places::documents::Standing::Spawned(_)
            )
        })
        .count();
    assert!(
        eor.portals.len() > 4500 && spawned > 500,
        "{} portals, {spawned} spawned",
        eor.portals.len()
    );
    // The portals destroyed by the end of retail lead nowhere; Zaikhal's working one is there.
    assert!(
        eor.portals
            .iter()
            .all(|p| !p.name.starts_with("Destroyed ")),
        "a destroyed portal"
    );
    assert!(eor.portals.iter().any(|p| p.name == "Zaikhal Portal"));
    let held = |r: fn(&Requirement) -> bool| {
        eor.portals
            .iter()
            .filter(|p| p.requires.iter().any(r))
            .count()
    };
    assert!(held(|r| matches!(r, Requirement::Quest(_))) > 300);
    assert!(held(|r| matches!(r, Requirement::Event { .. })) > 200);
    assert!(held(|r| *r == Requirement::NoOlthoi) > 20);
    let t = towns(&eor);
    assert!(t.len() >= 45, "{t:?}");
    assert_eq!(town(&t, "Holtburg"), Some(holtburg));
    for later in ["Sanamar", "Silyun", "Fiun"] {
        assert!(town(&t, later).is_some(), "{later}: {t:?}");
    }
    // The client reads both documents, and routes a character without the casino's flag past
    // its portal.
    let store = PortalStore::from_text(&table).expect("the client reads the portal table");
    let flagless = Traveller {
        quests: Some(std::collections::BTreeSet::new()),
        ..Traveller::at_level(Some(100))
    };
    assert!(store
        .routable_for(&flagless)
        .iter()
        .all(|e| e.wcid != Some(38998)));
    assert!(store
        .routable(Some(100))
        .iter()
        .any(|e| e.wcid == Some(38998)));
    let places = PlaceStore::from_text(&eor.places_document("Dereth", Source::Hint))
        .expect("the client reads the places");
    assert!(places
        .places()
        .iter()
        .any(|p| p.kind == PlaceKind::Town && p.name == "Holtburg"));

    // February 2005: Yanshi stands where it stood before it moved, and the later towns are not.
    let path = database(&eras::INFILTRATION, "EMPYREAN_TEST_INFILTRATION_SQL");
    let infiltration = WorldPlaces::of(&read(&path));
    let table = infiltration.portal_table("Dereth", Source::Hint);
    assert_eq!(hideout(&table), ("-".into(), "-".into(), "-".into()));
    let t = towns(&infiltration);
    assert!(t.len() >= 40, "{t:?}");
    assert_eq!(town(&t, "Holtburg"), Some(holtburg));
    assert!(
        town(&t, "Yanshi").is_some_and(|at| at.starts_with("0xB972")),
        "{t:?}"
    );
    for later in ["Sanamar", "Silyun", "Fiun"] {
        assert_eq!(town(&t, later), None, "{later}");
    }

    // November 1999: the hideout admits levels 1 to 4, and towns founded later are not there.
    let path = database(&eras::LAUNCH, "EMPYREAN_TEST_LAUNCH_SQL");
    let launch = WorldPlaces::of(&read(&path));
    let table = launch.portal_table("Dereth", Source::Hint);
    assert_eq!(hideout(&table), ("1".into(), "4".into(), "-".into()));
    let t = towns(&launch);
    assert!(t.len() >= 25, "{t:?}");
    assert_eq!(town(&t, "Holtburg"), Some(holtburg));
    for later in ["Ayan Baqur", "Linvak Tukal", "Sanamar"] {
        assert_eq!(town(&t, later), None, "{later}");
    }
}

#[test]
fn each_eras_database_is_fetched_from_where_the_import_tool_fetches_it() {
    for (db, line) in [
        (&eras::EOR, &world_release::PATCHES),
        (&eras::INFILTRATION, &world_release::SIXTEEN_PY),
        (&eras::LAUNCH, &world_release::AC1999),
    ] {
        assert_eq!(db.era, line.era);
        assert_eq!(db.repository, line.repository);
        assert_eq!(db.asset_prefix, line.asset_prefix);
        assert_eq!(db.tag, line.pinned_tag);
        assert_eq!(db.zip_sha256, line.pinned_zip_sha256);
        if db.pinned() {
            assert_eq!(db.sql_name(db.tag), line.sql_name(line.pinned_tag));
            assert_eq!(db.download_url(db.tag), line.download_url(line.pinned_tag));
        }
    }
}
