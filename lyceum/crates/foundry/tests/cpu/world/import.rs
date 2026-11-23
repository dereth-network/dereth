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
    // The pack places nothing through the world database, so nothing is hidden.
    let built = compile::build(&world_file, &base, None, false).unwrap();
    assert!(!built
        .warnings
        .iter()
        .any(|w| w.contains("places its own things")));
    // A spawn in the pack: the import's rule hides it, and the build says so.
    f.write(
        "sample/server/sql/landblocks/0202 guide.sql",
        "INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`) \
         VALUES (0x70202001, 1, 0x02020001);\n",
    );
    let built = compile::build(&world_file, &base, None, false).unwrap();
    assert!(
        built.warnings.contains(
            &"this world's imported base places its own things, so the world database's \
              spawns in sample won't appear"
                .to_owned()
        ),
        "{:?}",
        built.warnings
    );
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
