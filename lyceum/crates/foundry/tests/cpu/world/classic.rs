//! Behaviour: none (tooling: a world built on the files from before Throne of Destiny)
//! A world that names Infiltration's era is built over that era's `portal.dat` and `cell.dat`,
//! writes its records in their layout into the usual overlay over them, and is refused over the
//! later files; a world of the end of retail is refused over the older ones.
//! Fixture: a small set of files of each layout, built by the test.

use std::path::Path;

use dereth_assets::world::{CellLandblock, LandblockInfo};
use dereth_assets::{Decode, GfxObj, RenderSurface, Surface};
use dereth_dat::overlay::{ContainerManifest, OverlayDir, MANIFEST};
use dereth_dat::{ContainerEra, ModernDat, RetailDatStore};
use dereth_primitives::{DataId, EraId};
use lyceum_foundry::compile;
use lyceum_foundry::dats::DatSet;
use lyceum_foundry::lock::Lock;
use lyceum_foundry::symbol::Kind;
use lyceum_foundry::Error;
use serde_json::json;

use crate::common::{self, Fixture, HOME, SETUP};

const ERA: EraId = EraId::Infiltration;

/// The fixture's world, built on Infiltration's era.
fn world(f: &Fixture) -> std::path::PathBuf {
    f.world();
    f.write(
        "world.toml",
        "key = \"test-world\"\nera = \"infiltration\"\npacks = [\"sample\"]\nout = \"build\"\n",
    )
}

fn read(f: &Fixture, t: ModernDat, id: u32) -> Vec<u8> {
    common::read_through(&f.dats(), &f.out(), t, id).unwrap()
}

fn lock(f: &Fixture) -> Lock {
    Lock::read(&f.path().join("world.lock")).unwrap()
}

/// A picture with one see-through pixel, its texture and a surface laying it, a static and a
/// raised vertex.
fn pack(f: &Fixture) {
    f.write(
        "sample/records/stripes.toml",
        "[[render-surface]]\nname = \"stripes\"\npicture = \"art/stripes.png\"\nformat = \"a8r8g8b8\"\n\n\
         [[surface-texture]]\nname = \"stripes\"\nlevels = [\"stripes\"]\n\n\
         [[surface]]\nname = \"stripes\"\ntype = 2\ntexture = \"stripes\"\n",
    );
    let png = common::png(
        2,
        2,
        &[
            0xFF, 0x80, 0x00, 0xFF, 0x10, 0x20, 0x30, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0x00,
        ],
    );
    std::fs::create_dir_all(f.path().join("sample/art")).unwrap();
    std::fs::write(f.path().join("sample/art/stripes.png"), png).unwrap();
    f.write(
        "sample/edits/landblocks/0101.toml",
        "[[static]]\nname = \"post\"\nmodel = 0x02000001\nat = [12.0, 34.0, 56.0]\n\n\
         [[vertex]]\nat = [3, 4]\nheight = 99\n",
    );
}

#[test]
fn an_infiltration_world_writes_the_older_records_into_the_overlay_over_the_older_files() {
    let f = Fixture::classic("classic-build");
    let w = world(&f);
    pack(&f);
    let built = compile::build(&w, &f.dats(), None, false).unwrap();
    assert_eq!(built.containers.len(), 2, "the portal's and the cell's");

    // The containers name the older files as their base, so the overlay is read as theirs.
    let od = OverlayDir::new(&f.out().join("overlay")).unwrap();
    assert_eq!(od.base_era(), Some(ContainerEra::Classic));
    for (t, name) in [
        (ModernDat::Portal, "portal.dat"),
        (ModernDat::Cell, "cell.dat"),
    ] {
        let m = ContainerManifest::decode(&od.open(t).unwrap().read(MANIFEST).unwrap()).unwrap();
        assert_eq!(m.base_name, name);
    }

    let l = lock(&f);
    // The picture is the older image, three bytes a pixel in red, green, blue order.
    let picture = l.get(Kind::RenderSurface, "sample:stripes").unwrap();
    let bytes = read(&f, ModernDat::Portal, picture);
    assert_eq!(bytes.len(), 12 + 12);
    assert_eq!(&bytes[12..15], [0xFF, 0x80, 0x00]);
    RenderSurface::decode_payload_for(ERA, DataId(picture), &bytes).unwrap();
    // The texture holds the picture's pixels itself, in 16 bits with alpha (one pixel is clear).
    let texture = l.get(Kind::SurfaceTexture, "sample:stripes").unwrap();
    let bytes = read(&f, ModernDat::Portal, texture);
    let rs = RenderSurface::from_classic_texture(DataId(texture), &bytes).unwrap();
    assert_eq!((rs.width, rs.height, rs.format), (2, 2, 26));
    assert_eq!(&rs.payload(&bytes).unwrap()[..2], &0xFF80u16.to_le_bytes());
    // The surface starts with its own id.
    let surface = l.get(Kind::Surface, "sample:stripes").unwrap();
    let bytes = read(&f, ModernDat::Portal, surface);
    assert_eq!(&bytes[..4], &surface.to_le_bytes());
    let s = Surface::decode_payload_for(ERA, DataId(surface), &bytes).unwrap();
    assert_eq!(s.orig_texture_id, Some(DataId(texture)));

    // The landblock's statics and terrain, laid out alike in both eras.
    let id = (u32::from(HOME) << 16) | 0xFFFE;
    let info =
        LandblockInfo::decode_payload_for(ERA, DataId(id), &read(&f, ModernDat::Cell, id)).unwrap();
    assert_eq!(info.objects.len(), 3);
    assert_eq!(info.objects[2].id, DataId(SETUP));
    let id = (u32::from(HOME) << 16) | 0xFFFF;
    let land =
        CellLandblock::decode_payload_for(ERA, DataId(id), &read(&f, ModernDat::Cell, id)).unwrap();
    assert_eq!(land.height[3 * 9 + 4], 99);

    // The client's store over the older files reads the overlay as the world.
    let store = RetailDatStore::open_classic_dir(&f.dats())
        .unwrap()
        .with_overlay(&od, Some("test-world"))
        .unwrap();
    assert_eq!(store.era_of(DataId(texture)), ContainerEra::Classic);
    assert_eq!(
        store.read_portal(DataId(texture)).unwrap(),
        read(&f, ModernDat::Portal, texture)
    );
    // The same sources and files give the same bytes.
    assert!(compile::build(&w, &f.dats(), None, true).unwrap().current);
}

#[test]
fn a_world_is_refused_over_the_files_of_another_era() {
    let older = Fixture::classic("classic-refused");
    let later = Fixture::new("later-refused");
    let w = world(&later);
    let e = compile::build(&w, &later.dats(), None, false).unwrap_err();
    assert!(matches!(e, Error::Refused(_)), "{e}");
    let e = e.to_string();
    assert!(
        e.contains("Infiltration") && e.contains("portal.dat") && e.contains("client_portal.dat"),
        "{e}"
    );
    let w = older.world();
    let e = compile::build(&w, &older.dats(), None, false)
        .unwrap_err()
        .to_string();
    assert!(e.contains("end-of-retail") && e.contains("cell.dat"), "{e}");
    // A set opened without a world is read as the files it is.
    assert_eq!(DatSet::open(&older.dats()).unwrap().era, ERA);
    assert_eq!(DatSet::open(&later.dats()).unwrap().era, EraId::Eor);
    let e = later
        .write("w2.toml", "key = \"w\"\nera = \"dark-majesty\"\n")
        .to_string_lossy()
        .into_owned();
    let e = lyceum_foundry::world::World::read(Path::new(&e))
        .unwrap_err()
        .to_string();
    assert!(e.contains("not an era"), "{e}");
}

/// A model with a lower detail level, imported into an Infiltration world: its nearest level
/// names no detail record, and the record takes the id that level's own id reaches it by.
#[test]
fn an_imported_model_reaches_its_detail_record_by_its_own_id_in_the_older_files() {
    let f = Fixture::classic("classic-model");
    let w = world(&f);
    f.write(
        "sample/records/model.toml",
        "[[model]]\nname = \"arch\"\nfile = \"art/model.gltf\"\ncollision = \"none\"\n",
    );
    f.write(
        "sample/edits/landblocks/0102.toml",
        "[[static]]\nname = \"arch\"\nmodel = \"arch\"\nat = [20.0, 30.0, 5.0]\n",
    );
    let vertices: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 3.0, 0.0]];
    let bytes: Vec<u8> = vertices
        .into_iter()
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect();
    std::fs::create_dir_all(f.path().join("sample/art")).unwrap();
    std::fs::write(f.path().join("sample/art/mesh.bin"), bytes).unwrap();
    let doc = json!({
        "asset":{"version":"2.0"},
        "scene":0,
        "scenes":[{"nodes":[0, 1]}],
        "nodes":[{"name":"arch","mesh":0},{"name":"arch_lod1","mesh":0}],
        "buffers":[{"uri":"mesh.bin","byteLength":36}],
        "bufferViews":[{"buffer":0,"byteLength":36}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[2,3,0]}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0}]}],
        "materials":[{"pbrMetallicRoughness":{"baseColorFactor":[1.0,0.5,0.0,1.0]}}]
    });
    f.write("sample/art/model.gltf", &doc.to_string());
    compile::build(&w, &f.dats(), None, false).unwrap();
    let l = lock(&f);
    let near = l.get(Kind::GfxObj, "sample:arch-part-0-lod-0").unwrap();
    let degrade = l.get(Kind::Degrade, "sample:arch-part-0").unwrap();
    assert_eq!(degrade, 0x1100_0000 | (near & 0x00FF_FFFF));
    let g =
        GfxObj::decode_payload_for(ERA, DataId(near), &read(&f, ModernDat::Portal, near)).unwrap();
    assert_eq!(g.flags & 8, 0, "the older model names no detail record");
    assert_eq!(g.did_degrade, Some(DataId(degrade)));
    let info = dereth_assets::GfxObjDegradeInfo::decode_payload_for(
        ERA,
        DataId(degrade),
        &read(&f, ModernDat::Portal, degrade),
    )
    .unwrap();
    assert_eq!(info.degrades[0].gfxobj_id, DataId(near));
}

/// The older files ship a few landblocks whose statics their terrain does not name, which the
/// checks pass as the files' own; a world that changes one of those statics records makes the
/// terrain name it, so its statics are drawn.
#[test]
fn a_statics_record_the_terrain_does_not_name_is_named_once_a_world_changes_it() {
    let f = Fixture::classic("classic-unnamed");
    // 0202's statics record, its terrain not naming it.
    let lone = LandblockInfo {
        id: DataId(0x0202_FFFE),
        ..common::home_statics()
    };
    let mut cell = vec![
        (
            (u32::from(HOME) << 16) | 0xFFFE,
            common::home_statics().encode(),
        ),
        (lone.id.raw(), lone.encode()),
    ];
    for lb in [0x0101u16, 0x0102, 0x0201, 0x0202] {
        cell.push((
            (u32::from(lb) << 16) | 0xFFFF,
            common::terrain(lb, lb == HOME).encode(),
        ));
    }
    let bytes = dereth_dat::testing::classic_container(0x100, 3, &cell);
    std::fs::write(dereth_dat::ClassicDat::Cell.in_dir(&f.dats()), bytes).unwrap();
    let set = DatSet::open(&f.dats()).unwrap();
    let report = lyceum_foundry::validate::validate(&set, &lyceum_foundry::validate::Scope::All);
    assert!(report.problems.is_empty(), "{:?}", report.problems);

    let w = world(&f);
    f.write(
        "sample/edits/landblocks/0202.toml",
        "[[static]]\nmodel = 0x02000001\nat = [10.0, 10.0, 5.0]\n",
    );
    compile::build(&w, &f.dats(), None, false).unwrap();
    let id = 0x0202_FFFF;
    let land =
        CellLandblock::decode_payload_for(ERA, DataId(id), &read(&f, ModernDat::Cell, id)).unwrap();
    assert_eq!(land.lbi_exists, 1, "the terrain names the statics now");
}

/// Another tool's world made of the older files, `portal.dat` and `cell.dat`, is taken apart
/// against its base into the usual overlay, which reads as the world, and an Infiltration world
/// lays it under its packs.
#[test]
fn a_world_of_the_older_files_is_imported_into_an_overlay_and_built_on() {
    let f = Fixture::classic("classic-import");
    let world_dir = f.path().join("other-world");
    std::fs::create_dir_all(&world_dir).unwrap();
    std::fs::copy(
        dereth_dat::ClassicDat::Portal.in_dir(&f.dats()),
        dereth_dat::ClassicDat::Portal.in_dir(&world_dir),
    )
    .unwrap();
    let mut changed = common::terrain(0x0202, false);
    changed.height[40] = 123;
    let mut cell = vec![(
        (u32::from(HOME) << 16) | 0xFFFE,
        common::home_statics().encode(),
    )];
    for lb in [0x0101u16, 0x0102, 0x0201] {
        cell.push((
            (u32::from(lb) << 16) | 0xFFFF,
            common::terrain(lb, lb == HOME).encode(),
        ));
    }
    cell.push((0x0202_FFFF, changed.encode()));
    let bytes = dereth_dat::testing::classic_container(0x100, 3, &cell);
    std::fs::write(dereth_dat::ClassicDat::Cell.in_dir(&world_dir), bytes).unwrap();

    let out = f.path().join("imported");
    let done = lyceum_foundry::import::import(
        &f.dats(),
        &world_dir,
        &out,
        "test-world",
        dereth_dat::decompose::Iterations::Revision,
        true,
    )
    .unwrap();
    assert_eq!(done.len(), 2, "the portal file and the cell file");
    assert!(done[0].written.is_none(), "the portal file is its base");
    assert_eq!(done[1].verified.map(|n| n > 0), Some(true));
    let od = OverlayDir::new(&out).unwrap();
    assert_eq!(od.base_era(), Some(ContainerEra::Classic));

    // Laid under an Infiltration world's packs, the world's terrain is the imported one.
    let w = world(&f);
    let text = std::fs::read_to_string(&w).unwrap();
    std::fs::write(&w, format!("{text}imports = [\"imported\"]\n")).unwrap();
    compile::build(&w, &f.dats(), None, false).unwrap();
    let id = 0x0202_FFFF;
    let land =
        CellLandblock::decode_payload_for(ERA, DataId(id), &read(&f, ModernDat::Cell, id)).unwrap();
    assert_eq!(land.height[40], 123);
}
