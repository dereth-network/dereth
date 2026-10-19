//! Behaviour: none (tooling: the pack compiler over a small set of files)

use std::path::Path;

use dereth_assets::world::{CellLandblock, LandblockInfo};
use dereth_assets::{Decode, RenderSurface, SurfaceTexture};
use dereth_dat::ModernDat;
use dereth_pack::compile::{self, heading_quat};
use dereth_pack::Error;
use dereth_primitives::DataId;

use super::common::{self, Fixture, GFXOBJ, HOME, PICTURE, SETUP};

fn build(f: &Fixture, world: &Path) -> Result<compile::Built, Error> {
    compile::build(world, &f.dats(), None, false)
}

fn statics(f: &Fixture, lb: u16) -> Option<LandblockInfo> {
    let id = (u32::from(lb) << 16) | 0xFFFE;
    common::read_through(&f.dats(), &f.out(), ModernDat::Cell, id)
        .map(|b| LandblockInfo::decode_payload(DataId(id), &b).unwrap())
}

fn terrain(f: &Fixture, lb: u16) -> CellLandblock {
    let id = (u32::from(lb) << 16) | 0xFFFF;
    let b = common::read_through(&f.dats(), &f.out(), ModernDat::Cell, id).unwrap();
    CellLandblock::decode_payload(DataId(id), &b).unwrap()
}

#[test]
fn a_static_placed_where_there_is_no_statics_record_makes_one_and_flags_the_terrain() {
    let f = Fixture::new("pack-new-lbi");
    let world = f.world();
    f.write(
        "sample/edits/landblocks/0102.toml",
        "[[static]]\nmodel = 0x02000001\nat = [12.0, 34.0, 56.0]\nheading = 90\n",
    );
    build(&f, &world).unwrap();
    let info = statics(&f, 0x0102).expect("the statics record is made");
    assert_eq!(info.num_cells, 0);
    assert_eq!(info.objects.len(), 1);
    assert_eq!(info.objects[0].id, DataId(SETUP));
    let o = info.objects[0].frame;
    assert_eq!((o.origin.x, o.origin.y, o.origin.z), (12.0, 34.0, 56.0));
    assert_eq!(o.rotation, heading_quat(90.0));
    assert_eq!(
        terrain(&f, 0x0102).lbi_exists,
        1,
        "the terrain says it has one"
    );
    // The landblocks the pack did not touch are the base's.
    assert!(statics(&f, 0x0201).is_none());
    assert_eq!(terrain(&f, 0x0201).lbi_exists, 0);
}

#[test]
fn a_heading_turns_a_model_clockwise_from_north() {
    // Unturned, a model faces +y (north); a heading of 90 faces it east, +x.
    let q = heading_quat(90.0);
    // The forward vector (0, 1, 0) turned by q.
    let (w, z) = (q.w, q.z);
    let forward = (-2.0 * w * z, 1.0 - 2.0 * z * z);
    assert!(
        (forward.0 - 1.0).abs() < 1e-6 && forward.1.abs() < 1e-6,
        "{forward:?}"
    );
}

#[test]
fn base_statics_are_removed_and_moved_by_index_and_new_ones_follow_them() {
    let f = Fixture::new("pack-statics");
    let world = f.world();
    f.write(
        "sample/edits/landblocks/0101.toml",
        "[[remove-static]]\nindex = 0\nmodel = 0x01000001\n\n\
         [[move-static]]\nindex = 1\nat = [99.0, 98.0, 7.0]\n\n\
         [[static]]\nname = \"post\"\nmodel = 0x01000001\nat = [1.0, 2.0, 3.0]\n\n\
         [[static]]\nname = \"gone\"\nmodel = 0x01000001\nat = [4.0, 5.0, 6.0]\n\n\
         [[remove-static]]\nstatic = \"gone\"\n",
    );
    build(&f, &world).unwrap();
    let info = statics(&f, HOME).unwrap();
    let ids: Vec<u32> = info.objects.iter().map(|o| o.id.raw()).collect();
    assert_eq!(
        ids,
        [SETUP, GFXOBJ],
        "the moved base static, then the new one"
    );
    assert_eq!(info.objects[0].frame.origin.x, 99.0);
    assert_eq!(info.objects[0].frame.origin.z, 7.0);
    assert_eq!(info.objects[1].frame.origin.x, 1.0);
}

#[test]
fn a_removal_that_names_the_wrong_model_is_refused_with_its_line() {
    let f = Fixture::new("pack-wrong-model");
    let world = f.world();
    f.write(
        "sample/edits/landblocks/0101.toml",
        "# the second static is the Setup\n[[remove-static]]\nindex = 1\nmodel = 0x01000001\n",
    );
    let e = build(&f, &world).unwrap_err().to_string();
    assert!(e.contains("0101.toml:2") && e.contains("0x02000001"), "{e}");
}

#[test]
fn an_edge_vertex_is_written_in_every_landblock_that_holds_it() {
    let f = Fixture::new("pack-corner");
    let world = f.world();
    // (8, 8) of 0101 is the corner it shares with 0102, 0201 and 0202.
    f.write(
        "sample/edits/landblocks/0101.toml",
        "[[vertex]]\nat = [8, 8]\nheight = 200\nroad = 1\n",
    );
    build(&f, &world).unwrap();
    for (lb, x, y) in [
        (0x0101, 8, 8),
        (0x0201, 0, 8),
        (0x0102, 8, 0),
        (0x0202, 0, 0),
    ] {
        let t = terrain(&f, lb);
        assert_eq!(t.height[x * 9 + y], 200, "{lb:04X} ({x}, {y})");
        assert_eq!(t.road(x, y), 1, "{lb:04X}");
        assert_eq!(t.terrain_type(x, y), 1, "the terrain type is kept");
    }
    // Its neighbors along the edge are not.
    assert_eq!(terrain(&f, 0x0201).height[7], 10 + 7);
}

#[test]
fn two_files_setting_one_vertex_differently_are_a_conflict() {
    let f = Fixture::new("pack-conflict");
    let world = f.world();
    f.write(
        "sample/edits/landblocks/0101.toml",
        "[[vertex]]\nat = [8, 3]\nheight = 50\n",
    );
    // The same vertex, as 0201 holds it.
    f.write(
        "sample/edits/landblocks/0201.toml",
        "[[vertex]]\nat = [0, 3]\nheight = 51\n",
    );
    let e = build(&f, &world).unwrap_err();
    assert!(matches!(e, Error::Conflict { .. }), "{e}");
    let e = e.to_string();
    assert!(e.contains("0101.toml") && e.contains("0201.toml"), "{e}");
    // The same value from both is no conflict.
    f.write(
        "sample/edits/landblocks/0201.toml",
        "[[vertex]]\nat = [0, 3]\nheight = 50\n",
    );
    build(&f, &world).unwrap();
}

#[test]
fn a_picture_becomes_a_render_surface_and_its_texture_names_it() {
    let f = Fixture::new("pack-picture");
    let world = f.world();
    let rgba: Vec<u8> = (0..4u8)
        .flat_map(|i| [i * 10, i * 10 + 1, i * 10 + 2, 200 + i])
        .collect();
    std::fs::create_dir_all(f.path().join("sample/art")).unwrap();
    std::fs::write(f.path().join("sample/art/p.png"), common::png(2, 2, &rgba)).unwrap();
    f.write(
        "sample/records/art.toml",
        "[[render-surface]]\nname = \"p\"\npicture = \"art/p.png\"\n\n\
         [[surface-texture]]\nname = \"p\"\nlevels = [\"p\"]\n",
    );
    let built = build(&f, &world).unwrap();
    let ids: Vec<(String, u32)> = built
        .numbered
        .iter()
        .map(|(k, s, id)| (format!("{k} {s}"), *id))
        .collect();
    // The next ids past the base's own in each span.
    assert_eq!(
        ids,
        [
            ("render-surface sample:p".to_owned(), PICTURE + 1),
            ("surface-texture sample:p".to_owned(), 0x0500_0011),
        ]
    );
    let b = common::read_through(&f.dats(), &f.out(), ModernDat::Portal, PICTURE + 1).unwrap();
    let rs = RenderSurface::decode_payload(DataId(PICTURE + 1), &b).unwrap();
    assert_eq!(
        (rs.width, rs.height, rs.format, rs.data_category),
        (2, 2, 21, 4)
    );
    // Blue, green, red, alpha.
    assert_eq!(
        &rs.payload(&b).unwrap()[..8],
        &[2, 1, 0, 200, 12, 11, 10, 201]
    );
    let b = common::read_through(&f.dats(), &f.out(), ModernDat::Portal, 0x0500_0011).unwrap();
    let st = SurfaceTexture::decode_payload(DataId(0x0500_0011), &b).unwrap();
    assert_eq!(st.source_levels, [DataId(PICTURE + 1)]);
}

#[test]
fn a_picture_whose_sides_are_not_powers_of_two_is_refused() {
    let f = Fixture::new("pack-npot");
    let world = f.world();
    std::fs::create_dir_all(f.path().join("sample/art")).unwrap();
    std::fs::write(
        f.path().join("sample/art/p.png"),
        common::png(3, 2, &[0; 24]),
    )
    .unwrap();
    f.write(
        "sample/records/art.toml",
        "[[render-surface]]\nname = \"p\"\npicture = \"art/p.png\"\n",
    );
    let e = build(&f, &world).unwrap_err().to_string();
    assert!(
        e.contains("powers of two") && e.contains("art.toml:1"),
        "{e}"
    );
}

#[test]
fn symbols_keep_their_ids_through_the_lockfile_and_new_ones_take_free_ids() {
    let f = Fixture::new("pack-lock");
    let world = f.world();
    f.write(
        "sample/records/a.toml",
        "[[surface-texture]]\nname = \"b\"\nlevels = [0x06000010]\n",
    );
    build(&f, &world).unwrap();
    let lock = std::fs::read_to_string(f.path().join("world.lock")).unwrap();
    assert!(lock.contains("\"sample:b\" = 0x05000011"), "{lock}");
    // A new symbol that sorts first does not take the old one's id; the old keeps its own.
    f.write(
        "sample/records/a.toml",
        "[[surface-texture]]\nname = \"a\"\nlevels = [0x06000010]\n\n\
         [[surface-texture]]\nname = \"b\"\nlevels = [0x06000010]\n",
    );
    build(&f, &world).unwrap();
    let lock = std::fs::read_to_string(f.path().join("world.lock")).unwrap();
    assert!(lock.contains("\"sample:a\" = 0x05000012"), "{lock}");
    assert!(lock.contains("\"sample:b\" = 0x05000011"), "{lock}");
    // A symbol no source uses any more keeps its id in the lockfile.
    f.write("sample/records/a.toml", "");
    build(&f, &world).unwrap();
    let lock = std::fs::read_to_string(f.path().join("world.lock")).unwrap();
    assert!(lock.contains("\"sample:a\" = 0x05000012"), "{lock}");
}

#[test]
fn sql_tokens_are_filled_with_the_worlds_ids() {
    let f = Fixture::new("pack-sql");
    let world = f.world();
    f.write(
        "sample/records/a.toml",
        "[[surface-texture]]\nname = \"skin\"\nlevels = [0x06000010]\n",
    );
    f.write(
        "sample/server/sql/weenies/guide.sql",
        "DELETE FROM `weenie` WHERE `class_Id` = {{wcid:guide}};\n\
         INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)\n\
         VALUES ({{wcid:guide}}, 3, 0x050018E5, {{ surface-texture:skin }});\n",
    );
    f.write(
        "sample/server/sql/instances/guide.sql",
        "DELETE FROM `landblock_instance` WHERE `guid` = {{guid:guide@a9b4}};\n\
         INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`) VALUES ({{guid:guide@A9B4}}, {{wcid:sample:guide}});\n",
    );
    build(&f, &world).unwrap();
    let weenie = std::fs::read_to_string(f.out().join("sql/sample/weenies/guide.sql")).unwrap();
    assert!(weenie.contains("`class_Id` = 100000;"), "{weenie}");
    assert!(
        weenie.contains("VALUES (100000, 3, 0x050018E5, 0x05000011);"),
        "{weenie}"
    );
    let spawn = std::fs::read_to_string(f.out().join("sql/sample/instances/guide.sql")).unwrap();
    assert!(spawn.contains("`guid` = 0x7A9B4FFF;"), "{spawn}");
    assert!(spawn.contains("VALUES (0x7A9B4FFF, 100000);"), "{spawn}");
}

#[test]
fn a_token_that_does_not_read_stops_the_build_at_its_line() {
    let f = Fixture::new("pack-sql-bad");
    let world = f.world();
    f.write(
        "sample/server/sql/a.sql",
        "-- one\n-- two\nSELECT {{weenie:guide}};\n",
    );
    let e = build(&f, &world).unwrap_err().to_string();
    assert!(e.contains("a.sql:3") && e.contains("not a kind"), "{e}");
    f.write(
        "sample/server/sql/a.sql",
        "SELECT {{surface-texture:nowhere}};\n",
    );
    let e = build(&f, &world).unwrap_err().to_string();
    assert!(
        e.contains("no pack defines surface-texture sample:nowhere"),
        "{e}"
    );
    f.write("sample/server/sql/a.sql", "SELECT {{guid:guide}};\n");
    let e = build(&f, &world).unwrap_err().to_string();
    assert!(e.contains("names its landblock"), "{e}");
}

#[test]
fn the_same_world_builds_the_same_bytes() {
    let f = Fixture::new("pack-same-bytes");
    let world = f.world();
    f.write(
        "sample/edits/landblocks/0102.toml",
        "[[static]]\nmodel = 0x01000001\nat = [1.0, 2.0, 3.0]\n\n[[vertex]]\nat = [2, 2]\nto = [3, 4]\nheight = 99\n",
    );
    f.write(
        "sample/records/a.toml",
        "[[surface-texture]]\nname = \"a\"\nlevels = [0x06000010]\n",
    );
    build(&f, &world).unwrap();
    let first: Vec<Vec<u8>> = ["overlay_portal.dat", "overlay_cell.dat"]
        .iter()
        .map(|n| std::fs::read(f.out().join("overlay").join(n)).unwrap())
        .collect();
    let again = compile::build(&world, &f.dats(), None, true).unwrap();
    assert!(again.current, "the build is current");
    let other = f.path().join("elsewhere");
    compile::build(&world, &f.dats(), Some(&other), false).unwrap();
    for (n, bytes) in ["overlay_portal.dat", "overlay_cell.dat"]
        .iter()
        .zip(&first)
    {
        assert_eq!(
            &std::fs::read(other.join("overlay").join(n)).unwrap(),
            bytes,
            "{n}"
        );
    }
    // A changed source is not current.
    f.write(
        "sample/records/a.toml",
        "[[surface-texture]]\nname = \"a\"\nlevels = [0x06000010, 0x06000010]\n",
    );
    assert!(
        !compile::build(&world, &f.dats(), None, true)
            .unwrap()
            .current
    );
}

#[test]
fn a_static_whose_model_the_files_do_not_hold_fails_the_checks() {
    let f = Fixture::new("pack-missing-model");
    let world = f.world();
    f.write(
        "sample/edits/landblocks/0101.toml",
        "[[static]]\nmodel = 0x01000099\nat = [1.0, 2.0, 3.0]\n",
    );
    let e = build(&f, &world).unwrap_err();
    let Error::Invalid(problems) = &e else {
        panic!("{e}");
    };
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("0x0101FFFE") && problems[0].contains("0x01000099"),
        "{e}"
    );
    assert!(!f.out().join("overlay").exists(), "nothing is written");
}

#[test]
fn a_texture_whose_image_the_files_do_not_hold_fails_the_checks() {
    let f = Fixture::new("pack-missing-image");
    let world = f.world();
    f.write(
        "sample/records/a.toml",
        "[[surface-texture]]\nname = \"a\"\nlevels = [0x06000099]\n",
    );
    let e = build(&f, &world).unwrap_err().to_string();
    assert!(e.contains("names image 0x06000099"), "{e}");
}
