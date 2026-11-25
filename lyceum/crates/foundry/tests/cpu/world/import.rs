//! Behaviour: none (tooling: the import over a small set of files)

use std::path::{Path, PathBuf};

use dereth_assets::world::CellLandblock;
use dereth_assets::{Decode, SurfaceTexture};
use dereth_dat::decompose::Iterations;
use dereth_dat::{DatWriter, ModernDat};
use dereth_primitives::DataId;
use lyceum_foundry::{compile, import};

use crate::common::{self, Fixture, GFXOBJ, PICTURE};

/// A copy of the fixture's files in `name`.
fn copy(f: &Fixture, name: &str) -> PathBuf {
    let dir = f.path().join(name);
    std::fs::create_dir_all(&dir).unwrap();
    for t in [ModernDat::Portal, ModernDat::Cell] {
        std::fs::copy(t.in_dir(&f.dats()), t.in_dir(&dir)).unwrap();
    }
    dir
}

/// A base with one record more than the fixture's, and a world made from it by another tool: a
/// changed model, a new texture, a changed landblock, the extra record removed.
fn base_and_world(f: &Fixture) -> (PathBuf, PathBuf) {
    let base = copy(f, "base");
    let mut w = DatWriter::open(&ModernDat::Portal.in_dir(&base)).unwrap();
    w.save(DataId(0x0100_0077), &[7; 8], 2, 1, 0).unwrap();
    drop(w);
    let world = copy(f, "world");
    let mut w = DatWriter::open(&ModernDat::Portal.in_dir(&world)).unwrap();
    w.save(DataId(GFXOBJ), &[9; 24], 2, 5, 0).unwrap();
    let texture = SurfaceTexture {
        id: DataId(0x0500_0020),
        data_category: 0,
        unknown_byte: 2,
        source_levels: vec![DataId(PICTURE)],
    };
    w.save(DataId(0x0500_0020), &texture.encode(), 2, 5, 0)
        .unwrap();
    drop(w);
    let mut w = DatWriter::open(&ModernDat::Cell.in_dir(&world)).unwrap();
    let mut t = common::terrain(0x0202, false);
    t.height[40] = 123;
    w.save(DataId(0x0202_FFFF), &t.encode(), 2, 5, 0).unwrap();
    drop(w);
    (base, world)
}

fn terrain_through(dats: &Path, overlay: &Path, lb: u32) -> CellLandblock {
    let set = lyceum_foundry::dats::DatSet::open(dats)
        .unwrap()
        .with_overlay(overlay, None)
        .unwrap();
    let id = (lb << 16) | 0xFFFF;
    CellLandblock::decode_payload(DataId(id), &set.cell.read(DataId(id)).unwrap()).unwrap()
}

#[test]
fn an_import_reads_back_as_the_files_another_tool_wrote() {
    let f = Fixture::new("pack-import");
    let (base, world) = base_and_world(&f);
    let out = f.path().join("imported");
    let files = import::import(
        &base,
        &world,
        &out,
        "test-world",
        Iterations::Revision,
        true,
    )
    .unwrap();
    let portal = files.iter().find(|x| x.file == ModernDat::Portal).unwrap();
    assert_eq!(portal.diff.added, [0x0500_0020]);
    assert_eq!(portal.diff.changed, [GFXOBJ]);
    assert_eq!(portal.diff.removed, [0x0100_0077]);
    assert!(portal.verified.is_some(), "it reads back as the world's");
    let cell = files.iter().find(|x| x.file == ModernDat::Cell).unwrap();
    assert_eq!(cell.diff.changed, [0x0202_FFFF]);
    assert_eq!(terrain_through(&base, &out, 0x0202).height[40], 123);
}

#[test]
fn a_world_lays_an_import_under_its_packs_into_one_overlay() {
    let f = Fixture::new("pack-import-world");
    let (base, world) = base_and_world(&f);
    import::import(
        &base,
        &world,
        &f.path().join("imported"),
        "x",
        Iterations::Revision,
        false,
    )
    .unwrap();
    f.world();
    // The imported world's own rules and place names, beside its overlay.
    let rules = "dungeons_by_cells = true\nown_placements = true\n";
    f.write("imported/world.toml", rules);
    f.write("imported/places.tsv", "name\tx\ty\nHome\t1\t2\n");
    let world_file = f.write(
        "world.toml",
        "key = \"test-world\"\npacks = [\"sample\"]\nimports = [\"imported\"]\nout = \"build\"\n",
    );
    f.write(
        "sample/edits/landblocks/0202.toml",
        "[[static]]\nmodel = 0x01000001\nat = [1.0, 2.0, 3.0]\n",
    );
    compile::build(&world_file, &base, None, false).unwrap();
    let overlay = f.out().join("overlay");
    // The import's rules and place names are the built world's; it places nothing itself.
    assert_eq!(
        std::fs::read_to_string(overlay.join("world.toml")).unwrap(),
        rules
    );
    assert!(std::fs::read_to_string(overlay.join("places.tsv"))
        .unwrap()
        .contains("Home"));
    assert!(!overlay.join("placements.tsv").exists());
    // A spawn in the pack: the import's world places its own things, so the spawn is one of them.
    f.write(
        "sample/server/sql/landblocks/0202 guide.sql",
        "INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`) \
         VALUES (0x70202001, 1, 0x02020001);\n",
    );
    let built = compile::build(&world_file, &base, None, false).unwrap();
    assert!(
        !built.warnings.iter().any(|w| w.contains("won't appear")),
        "{:?}",
        built.warnings
    );
    let own = std::fs::read_to_string(overlay.join("placements.tsv")).unwrap();
    assert!(own.contains("0x70202001\t1\t0x02020001\t"), "{own}");
    // The import's terrain, with the pack's statics record flagged on it.
    let t = terrain_through(&base, &overlay, 0x0202);
    assert_eq!((t.height[40], t.lbi_exists), (123, 1));
    let set = lyceum_foundry::dats::DatSet::open(&base)
        .unwrap()
        .with_overlay(&overlay, Some("test-world"))
        .unwrap();
    assert!(
        set.portal.contains(DataId(0x0500_0020)),
        "the import's new record"
    );
    assert!(
        !set.portal.contains(DataId(0x0100_0077)),
        "the import's deletion"
    );
    assert_eq!(set.portal.read(DataId(GFXOBJ)).unwrap(), [9; 24]);
}

/// The server's import tool, as a test has it: it writes a world pack where it is told.
struct Tool;

impl lyceum_foundry::export::Importer for Tool {
    fn run(&self, args: &[std::ffi::OsString]) -> Result<String, String> {
        if args.first().is_some_and(|a| a == "--version") {
            return Ok("empyrean-import 9.9.9".into());
        }
        let out = args
            .iter()
            .position(|a| a == "--out")
            .and_then(|i| args.get(i + 1))
            .ok_or("no --out")?;
        std::fs::write(out, b"pack").map_err(|e| e.to_string())?;
        Ok("wrote".into())
    }
}

/// A world built on an imported world that places its own things: a server running it places
/// nothing of the world database's, so an export writes the packs' spawns into the world's own
/// placements, beside the import's. A spawn the pack writes again replaces the import's, one it
/// takes away is gone, and the others stay as the import has them.
#[test]
fn an_export_over_a_world_that_places_its_own_things_carries_the_packs_spawns_as_its_own() {
    use lyceum_foundry::export::{self, Era, Options, Target};
    let f = Fixture::new("pack-import-own-spawns");
    let (base, world) = base_and_world(&f);
    import::import(
        &base,
        &world,
        &f.path().join("imported"),
        "x",
        Iterations::Revision,
        false,
    )
    .unwrap();
    f.world();
    f.write(
        "imported/world.toml",
        "dungeons_by_cells = true\nown_placements = true\n",
    );
    f.write(
        "imported/placements.tsv",
        "guid\twcid\tcell\tx\ty\tz\tqw\tqx\tqy\tqz\tlinks\n\
         0x7020F000\t509\t0x02020009\t30\t40\t5\t1\t0\t0\t0\t\n\
         0x7020F001\t509\t0x02020009\t31\t40\t5\t1\t0\t0\t0\t\n\
         0x7020F002\t509\t0x02020009\t32\t40\t5\t1\t0\t0\t0\t\n",
    );
    let world_file = f.write(
        "world.toml",
        "key = \"test-world\"\npacks = [\"sample\"]\nimports = [\"imported\"]\nout = \"build\"\n",
    );
    f.write(
        "sample/server/sql/landblocks/0202 guide.sql",
        "DELETE FROM `landblock_instance` WHERE `guid` = 0x70202001;\n\n\
         INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, \
         `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, \
         `last_Modified`)\n\
         VALUES (0x70202001, 1, 0x02020001, 12.5, 7, 3, 1, 0, 0, 0, False, '2026-11-25 00:00:00');\n",
    );
    f.write(
        "sample/server/sql/landblocks/0202 overrides.sql",
        "DELETE FROM `landblock_instance` WHERE `guid` = 0x7020F001;\n\n\
         DELETE FROM `landblock_instance` WHERE `guid` = 0x7020F002;\n\n\
         INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, \
         `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, \
         `last_Modified`)\n\
         VALUES (0x7020F002, 509, 0x02020011, 50, 60, 5, 1, 0, 0, 0, False, '2026-11-25 00:00:00');\n",
    );
    let dump = f.path().join("ACE-World-Database-v0.9.295.sql");
    std::fs::write(&dump, "-- the world database").unwrap();
    let out = f.path().join("for-empyrean");
    let done = export::export(
        &world_file,
        &base,
        &Options {
            target: Target::Empyrean,
            out: &out,
            era: Era::Eor,
            world_sql: Some(&dump),
            importer: Some(&Tool),
            gdle_quests: None,
        },
    )
    .unwrap();
    assert!(
        !done.warnings.iter().any(|w| w.contains("won't appear")),
        "{:?}",
        done.warnings
    );
    let own = std::fs::read_to_string(out.join("overlay").join("placements.tsv")).unwrap();
    let lines: Vec<&str> = own.lines().collect();
    assert_eq!(
        lines,
        [
            "guid\twcid\tcell\tx\ty\tz\tqw\tqx\tqy\tqz\tlinks",
            "0x7020F000\t509\t0x02020009\t30\t40\t5\t1\t0\t0\t0\t",
            "0x70202001\t1\t0x02020001\t12.5\t7\t3\t1\t0\t0\t0\t",
            "0x7020F002\t509\t0x02020011\t50\t60\t5\t1\t0\t0\t0\t",
        ],
        "{own}"
    );
}

/// The world files Lyceum's own Play writes beside its overlay, from the session editing a world
/// built on an imported world that places its own things: the packs' spawns among the things it
/// places, as an export has them, and a spawn moved in the session where it stands now.
#[test]
fn play_over_a_world_that_places_its_own_things_hosts_the_packs_spawns_as_its_own() {
    use dereth_primitives::Vec3;
    use lyceum_foundry::edit::{Op, Session};
    let f = Fixture::new("pack-import-play-spawns");
    let (base, world) = base_and_world(&f);
    import::import(
        &base,
        &world,
        &f.path().join("imported"),
        "x",
        Iterations::Revision,
        false,
    )
    .unwrap();
    f.world();
    f.write(
        "imported/world.toml",
        "dungeons_by_cells = true\nown_placements = true\n",
    );
    let world_file = f.write(
        "world.toml",
        "key = \"test-world\"\npacks = [\"sample\"]\nimports = [\"imported\"]\nout = \"build\"\n",
    );
    f.write(
        "sample/server/sql/landblocks/0202 guide.sql",
        "INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, \
         `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, \
         `last_Modified`) VALUES (0x70202001, 1, 0x02020001, 10.0, 12.0, 0.0, 1.0, 0.0, 0.0, 0.0, \
         False, '2026-11-25 00:00:00');\n",
    );
    let mut s = Session::open(&world_file, &base, None).unwrap();
    let played = |s: &Session, tag: &str| {
        let dir = f.path().join(tag);
        compile::write_world_files(s.world(), &s.compiled().sql, &dir).unwrap();
        std::fs::read_to_string(dir.join("placements.tsv")).unwrap()
    };
    let own = played(&s, "play-0");
    assert!(own.contains("0x70202001\t1\t0x02020001\t10\t12\t"), "{own}");
    // Moved in the session: where it stands now, in the next overlay.
    let guide = s
        .spawns()
        .into_iter()
        .find(|x| x.guid == "0x70202001")
        .unwrap();
    s.apply(&Op::MoveSpawn {
        file: guide.file,
        guid: guide.guid,
        origin: Vec3::new(20.0, 22.0, 0.0),
        heading: 0.0,
    })
    .unwrap();
    let own = played(&s, "play-1");
    assert!(own.contains("0x70202001\t1\t0x02020001\t20\t22\t"), "{own}");
}
