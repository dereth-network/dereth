//! Behaviour: none (tooling: the sample world built and baked over the retail files)

use std::path::{Path, PathBuf};

use dereth_assets::world::{CellLandblock, LandblockInfo};
use dereth_assets::{Decode, Region, SurfaceTexture};
use dereth_dat::testing::ScratchDir;
use dereth_dat::{DatFile, ModernDat};
use dereth_pack::{bake, compile};
use dereth_primitives::DataId;

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let p = e.unwrap().path();
        let dest = to.join(p.file_name().unwrap());
        if p.is_dir() {
            copy_tree(&p, &dest);
        } else {
            std::fs::copy(&p, &dest).unwrap();
        }
    }
}

/// The sample world copied into a scratch folder, its output kept there too.
fn sample(tag: &str) -> (ScratchDir, PathBuf) {
    dereth_dat::testing::require_dats();
    let dir = ScratchDir::new(tag).unwrap();
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("sample");
    copy_tree(&from.join("holtburg"), &dir.path().join("holtburg"));
    std::fs::copy(from.join("world.lock"), dir.path().join("world.lock")).unwrap();
    let world = std::fs::read_to_string(from.join("world.toml")).unwrap();
    let world: String = world
        .lines()
        .map(|l| {
            if l.starts_with("out ") {
                "out = \"build\""
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(dir.path().join("world.toml"), world).unwrap();
    let path = dir.path().join("world.toml");
    (dir, path)
}

#[test]
fn the_sample_world_builds_over_the_retail_files_with_the_ids_its_lockfile_keeps() {
    let (dir, world) = sample("pack-sample-build");
    let dats = dereth_dat::testing::dat_dir();
    let built = compile::build(&world, &dats, None, false).unwrap();
    assert!(
        built.numbered.is_empty(),
        "every id is in the lockfile: {:?}",
        built.numbered
    );
    let committed =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("sample/world.lock"))
            .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("world.lock")).unwrap(),
        committed,
        "the build leaves the lockfile as it is"
    );
    assert!(compile::build(&world, &dats, None, true).unwrap().current);
    let set = dereth_pack::dats::DatSet::open(&dats)
        .unwrap()
        .with_overlay(&dir.path().join("build/overlay"), Some("sample-world"))
        .unwrap();
    // The new picture's texture, under the id the lockfile gives it.
    let st = SurfaceTexture::decode_payload(
        DataId(0x0500_3359),
        &set.portal.read(DataId(0x0500_3359)).unwrap(),
    )
    .unwrap();
    assert_eq!(st.source_levels, [DataId(0x0600_7577)]);
    // West of Holtburg: a statics record made, and the terrain flagged for it.
    let info = LandblockInfo::decode_payload(
        DataId(0xA8B4_FFFE),
        &set.cell.read(DataId(0xA8B4_FFFE)).unwrap(),
    )
    .unwrap();
    assert_eq!(info.objects.len(), 3);
    let west = CellLandblock::decode_payload(
        DataId(0xA8B4_FFFF),
        &set.cell.read(DataId(0xA8B4_FFFF)).unwrap(),
    )
    .unwrap();
    assert_eq!(west.lbi_exists, 1);
    // The hill on Holtburg's west edge is in both landblocks.
    let holtburg = CellLandblock::decode_payload(
        DataId(0xA9B4_FFFF),
        &set.cell.read(DataId(0xA9B4_FFFF)).unwrap(),
    )
    .unwrap();
    assert_eq!((holtburg.height[1], holtburg.height[2]), (50, 50));
    assert_eq!((west.height[8 * 9 + 1], west.height[8 * 9 + 2]), (50, 50));
    // Holtburg keeps its 114 statics and gains three. The two placed on terrain vertices (1, 1)
    // and (2, 1) stand at those vertices' heights, as the region's height table gives them.
    let info = LandblockInfo::decode_payload(
        DataId(0xA9B4_FFFE),
        &set.cell.read(DataId(0xA9B4_FFFE)).unwrap(),
    )
    .unwrap();
    assert_eq!(info.objects.len(), 117);
    let region = Region::decode_payload(
        DataId(0x1300_0000),
        &set.portal.read(DataId(0x1300_0000)).unwrap(),
    )
    .unwrap();
    let table = dereth_terrain::land::mesh::height_table(&region);
    for (o, vertex) in [(114, 9 + 1), (115, 2 * 9 + 1)] {
        let at = info.objects[o].frame.origin;
        let want = table[usize::from(holtburg.height[vertex])];
        assert!((at.z - want).abs() < 0.001, "{at:?} stands at {want}");
    }
    // The SQL names the world's ids.
    let spawn = std::fs::read_to_string(
        dir.path()
            .join("build/sql/holtburg/landblocks/A9B4 guide.sql"),
    )
    .unwrap();
    assert!(
        spawn.contains("VALUES (0x7A9B4FFF, 100000, 0xA9B40009"),
        "{spawn}"
    );
}

#[test]
fn the_sample_world_bakes_into_copies_of_the_retail_files() {
    let (dir, world) = sample("pack-sample-bake");
    let dats = dereth_dat::testing::dat_dir();
    compile::build(&world, &dats, None, false).unwrap();
    let to = dir.path().join("baked");
    let baked = bake::bake(&dir.path().join("build/overlay"), &dats, &to, true).unwrap();
    assert_eq!(baked.world_key, "sample-world");
    assert_eq!(baked.written.len(), 2, "the portal and cell files");
    let cell = DatFile::open(&ModernDat::Cell.in_dir(&to)).unwrap();
    let portal = DatFile::open(&ModernDat::Portal.in_dir(&to)).unwrap();
    // The base's iterations and one more.
    assert_eq!(
        cell.iteration_list().unwrap(),
        (1..=983).collect::<Vec<_>>()
    );
    assert_eq!(
        portal.iteration_list().unwrap(),
        (1..=2073).collect::<Vec<_>>()
    );
    assert!(cell.contains(DataId(0xA8B4_FFFE)));
    assert!(portal.contains(DataId(0x0600_7577)));
    assert_eq!(cell.entry(DataId(0xA8B4_FFFE)).unwrap().iteration, 983);
}
