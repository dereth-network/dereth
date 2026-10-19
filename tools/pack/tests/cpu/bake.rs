//! Behaviour: none (tooling: the bake over a small set of files)

use dereth_dat::{DatFile, ModernDat};
use dereth_pack::{bake, compile, Error};
use dereth_primitives::DataId;

use super::common::Fixture;

fn built(f: &Fixture) {
    let world = f.world();
    f.write(
        "sample/edits/landblocks/0102.toml",
        "[[static]]\nmodel = 0x01000001\nat = [1.0, 2.0, 3.0]\n\n[[vertex]]\nat = [4, 4]\nheight = 77\n",
    );
    f.write(
        "sample/records/a.toml",
        "[[surface-texture]]\nname = \"a\"\nlevels = [0x06000010]\n",
    );
    compile::build(&world, &f.dats(), None, false).unwrap();
}

#[test]
fn a_bake_writes_the_overlay_into_copies_and_leaves_the_files_alone() {
    let f = Fixture::new("pack-bake");
    built(&f);
    let before: Vec<Vec<u8>> = [ModernDat::Portal, ModernDat::Cell]
        .iter()
        .map(|t| std::fs::read(t.in_dir(&f.dats())).unwrap())
        .collect();
    let to = f.path().join("baked");
    let b = bake::bake(&f.out().join("overlay"), &f.dats(), &to, false).unwrap();
    assert_eq!(b.world_key, "test-world");
    assert_eq!(b.written.len(), 2, "the portal and cell files");
    for (t, bytes) in [ModernDat::Portal, ModernDat::Cell].iter().zip(&before) {
        assert_eq!(
            &std::fs::read(t.in_dir(&f.dats())).unwrap(),
            bytes,
            "{t:?} untouched"
        );
    }
    let layered = dereth_pack::dats::DatSet::open(&f.dats())
        .unwrap()
        .with_overlay(&f.out().join("overlay"), None)
        .unwrap();
    let cell = DatFile::open(&ModernDat::Cell.in_dir(&to)).unwrap();
    for id in [0x0102_FFFE, 0x0102_FFFF, 0x0101_FFFE] {
        assert_eq!(
            cell.read(DataId(id)).unwrap(),
            layered.cell.read(DataId(id)).unwrap(),
            "{id:#010X}"
        );
    }
    // The base's iterations and the overlay's revision, as one run from 1.
    assert_eq!(cell.iteration_list().unwrap(), [1, 2, 3, 4]);
    let raw = cell.read(dereth_dat::ITERATION_LIST).unwrap();
    assert_eq!(
        raw,
        [4u32, (-4i32).cast_unsigned(), 1]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>()
    );
    let portal = DatFile::open(&ModernDat::Portal.in_dir(&to)).unwrap();
    assert_eq!(portal.iteration_list().unwrap(), [1, 2, 3]);
    assert!(portal.contains(DataId(0x0500_0011)));
    let manifest = std::fs::read_to_string(to.join("dereth-bake.toml")).unwrap();
    assert!(manifest.contains("world = \"test-world\""), "{manifest}");
    assert!(!to.join("client_cell_1.dat.baking").exists());
}

#[test]
fn a_bake_with_only_the_changed_files_copies_nothing_else() {
    let f = Fixture::new("pack-bake-touched");
    let world = f.world();
    f.write(
        "sample/edits/landblocks/0102.toml",
        "[[vertex]]\nat = [4, 4]\nheight = 77\n",
    );
    compile::build(&world, &f.dats(), None, false).unwrap();
    let to = f.path().join("baked");
    let b = bake::bake(&f.out().join("overlay"), &f.dats(), &to, true).unwrap();
    assert_eq!(b.written.len(), 1);
    assert!(b.copied.is_empty());
    assert!(ModernDat::Cell.in_dir(&to).is_file());
    assert!(!ModernDat::Portal.in_dir(&to).exists());
}

#[test]
fn a_bake_into_the_files_own_folder_is_refused() {
    let f = Fixture::new("pack-bake-refused");
    built(&f);
    let before = std::fs::read(ModernDat::Cell.in_dir(&f.dats())).unwrap();
    let e = bake::bake(&f.out().join("overlay"), &f.dats(), &f.dats(), false).unwrap_err();
    assert!(matches!(e, Error::Refused(_)), "{e}");
    assert_eq!(
        std::fs::read(ModernDat::Cell.in_dir(&f.dats())).unwrap(),
        before
    );
}

#[test]
fn a_bake_over_other_files_than_the_overlays_base_is_refused() {
    let f = Fixture::new("pack-bake-other-base");
    built(&f);
    // Other files: the base with one more record.
    let other = f.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    for t in [ModernDat::Portal, ModernDat::Cell] {
        std::fs::copy(t.in_dir(&f.dats()), t.in_dir(&other)).unwrap();
    }
    let mut w = dereth_dat::DatWriter::open(&ModernDat::Cell.in_dir(&other)).unwrap();
    w.save(DataId(0x0303_FFFF), &[0; 252], 2, 1, 0).unwrap();
    drop(w);
    let e = bake::bake(
        &f.out().join("overlay"),
        &other,
        &f.path().join("baked"),
        false,
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("was made against"), "{e}");
}
