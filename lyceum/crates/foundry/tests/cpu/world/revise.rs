//! Behaviour: none (tooling: a world published again as the next revision of its overlay)

use std::path::Path;

use dereth_assets::world::LandblockInfo;
use dereth_assets::Decode;
use dereth_dat::overlay::OverlayDir;
use dereth_dat::ModernDat;
use dereth_primitives::{DataId, EraId};
use lyceum_foundry::{compile, revise};

use crate::common::{self, Fixture, HOME};

/// The statics of `HOME` as the world reads them through the overlay in `out/overlay`.
fn statics(f: &Fixture, out: &Path) -> LandblockInfo {
    let id = (u32::from(HOME) << 16) | 0xFFFE;
    let b = common::read_through(&f.dats(), out, ModernDat::Cell, id).unwrap();
    LandblockInfo::decode_payload(DataId(id), &b).unwrap()
}

/// The cell file's own revisions in the overlay in `out/overlay`.
fn cell_revisions(f: &Fixture, out: &Path) -> Vec<u32> {
    let set = lyceum_foundry::dats::DatSet::open(&f.dats()).unwrap();
    OverlayDir::new(&out.join("overlay"))
        .unwrap()
        .layer_over(ModernDat::Cell, &set.cell, None)
        .unwrap()
        .unwrap()
        .own_iterations()
        .to_vec()
}

fn copy_overlay(from: &Path, to: &Path) {
    std::fs::create_dir_all(to.join("overlay")).unwrap();
    for e in std::fs::read_dir(from.join("overlay")).unwrap() {
        let p = e.unwrap().path();
        std::fs::copy(&p, to.join("overlay").join(p.file_name().unwrap())).unwrap();
    }
}

#[test]
fn a_world_built_after_its_last_publish_is_that_publishs_next_revision_and_reads_as_the_new_build()
{
    let f = Fixture::new("revise-after");
    let world = f.world();
    f.write(
        "sample/edits/landblocks/0101.toml",
        "[[static]]\nname = \"post\"\nmodel = 0x01000001\nat = [1.0, 2.0, 3.0]\n\n\
         [[static]]\nname = \"stone\"\nmodel = 0x02000001\nat = [10.0, 20.0, 0.0]\n",
    );
    // The first publish: built, and numbered after none (no overlay there yet).
    let first = f.path().join("publish-1");
    compile::build(&world, &f.dats(), Some(&first), false).unwrap();
    let done = revise::after(
        &f.dats(),
        EraId::Eor,
        &f.path().join("none"),
        &first.join("overlay"),
        "test-world",
    )
    .unwrap();
    assert!(done.restarted.is_none());
    // The base's cell file has iterations 1 to 3.
    assert_eq!(cell_revisions(&f, &first), [4]);

    // The post moved, the stone gone, a new stone.
    f.write(
        "sample/edits/landblocks/0101.toml",
        "[[static]]\nname = \"post\"\nmodel = 0x01000001\nat = [5.0, 2.0, 3.0]\n\n\
         [[static]]\nname = \"new-stone\"\nmodel = 0x02000001\nat = [30.0, 20.0, 0.0]\n",
    );
    let built = f.path().join("built-2");
    compile::build(&world, &f.dats(), Some(&built), false).unwrap();
    let second = f.path().join("publish-2");
    copy_overlay(&built, &second);
    let done = revise::after(
        &f.dats(),
        EraId::Eor,
        &first.join("overlay"),
        &second.join("overlay"),
        "test-world",
    )
    .unwrap();
    let cell = &done
        .files
        .iter()
        .find(|(t, _)| *t == ModernDat::Cell)
        .unwrap()
        .1;
    assert_eq!(cell.revision, Some(5));
    assert_eq!(cell_revisions(&f, &second), [4, 5]);
    // It reads as the new build does.
    let now = statics(&f, &second);
    assert_eq!(now.objects, statics(&f, &built).objects);
    assert_ne!(now.objects, statics(&f, &first).objects);
    assert_eq!(
        now.objects.len(),
        4,
        "the base's two, the post and the new stone"
    );
    assert!(!second.join(".revising").exists() && !f.path().join(".revising").exists());
}

#[test]
fn a_last_publish_of_another_world_starts_the_revisions_again_and_says_why() {
    let f = Fixture::new("revise-other");
    let world = f.world();
    f.write(
        "sample/edits/landblocks/0101.toml",
        "[[static]]\nmodel = 0x01000001\nat = [1.0, 2.0, 3.0]\n",
    );
    let first = f.path().join("publish-1");
    compile::build(&world, &f.dats(), Some(&first), false).unwrap();
    let second = f.path().join("publish-2");
    compile::build(&world, &f.dats(), Some(&second), false).unwrap();
    let done = revise::after(
        &f.dats(),
        EraId::Eor,
        &first.join("overlay"),
        &second.join("overlay"),
        "another-world",
    );
    // The new build is the world's own: it is refused as another world's.
    assert!(done.is_err(), "{done:?}");
    let done = revise::after(
        &f.dats(),
        EraId::Eor,
        &first.join("overlay"),
        &second.join("overlay"),
        "test-world",
    )
    .unwrap();
    assert!(done.restarted.is_none(), "the same world follows on");
    assert_eq!(
        cell_revisions(&f, &second),
        [4],
        "nothing changed: no revision"
    );

    // The last publish belongs to another world: the revisions start again.
    std::fs::write(
        f.path().join("world.toml"),
        "key = \"renamed-world\"\npacks = [\"sample\"]\nout = \"build\"\n",
    )
    .unwrap();
    let third = f.path().join("publish-3");
    compile::build(&world, &f.dats(), Some(&third), false).unwrap();
    let done = revise::after(
        &f.dats(),
        EraId::Eor,
        &second.join("overlay"),
        &third.join("overlay"),
        "renamed-world",
    )
    .unwrap();
    let why = done.restarted.expect("restarted");
    assert!(why.contains("test-world"), "{why}");
    assert_eq!(cell_revisions(&f, &third), [4]);
}

#[test]
fn a_world_on_infiltrations_files_is_published_again_as_the_next_revision_over_them() {
    let f = Fixture::classic("revise-classic");
    f.world();
    let world = f.write(
        "world.toml",
        "key = \"test-world\"
era = \"infiltration\"
packs = [\"sample\"]
out = \"build\"
",
    );
    f.write(
        "sample/edits/landblocks/0101.toml",
        "[[static]]
name = \"stone\"
model = 0x02000001
at = [10.0, 20.0, 0.0]
",
    );
    let first = f.path().join("publish-1");
    compile::build(&world, &f.dats(), Some(&first), false).unwrap();
    revise::after(
        &f.dats(),
        EraId::Infiltration,
        &f.path().join("none"),
        &first.join("overlay"),
        "test-world",
    )
    .unwrap();
    // The base's cell.dat is at iteration 3.
    assert_eq!(cell_revisions(&f, &first), [4]);
    f.write(
        "sample/edits/landblocks/0101.toml",
        "[[static]]
name = \"stone\"
model = 0x02000001
at = [30.0, 20.0, 0.0]
",
    );
    let second = f.path().join("publish-2");
    compile::build(&world, &f.dats(), Some(&second), false).unwrap();
    let id = (u32::from(HOME) << 16) | 0xFFFE;
    let built = common::read_through(&f.dats(), &second, ModernDat::Cell, id);
    let done = revise::after(
        &f.dats(),
        EraId::Infiltration,
        &first.join("overlay"),
        &second.join("overlay"),
        "test-world",
    )
    .unwrap();
    assert!(done.restarted.is_none());
    assert_eq!(cell_revisions(&f, &second), [4, 5]);
    let now = common::read_through(&f.dats(), &second, ModernDat::Cell, id);
    assert_eq!(now, built, "it reads as the new build");
    assert_ne!(
        now,
        common::read_through(&f.dats(), &first, ModernDat::Cell, id)
    );
    // The overlay names its base as the file it lies over.
    let set = lyceum_foundry::dats::DatSet::open(&f.dats()).unwrap();
    let layer = OverlayDir::new(&second.join("overlay"))
        .unwrap()
        .layer_over(ModernDat::Cell, &set.cell, None)
        .unwrap()
        .unwrap();
    assert_eq!(layer.manifest().base_name, "cell.dat");
    // A world on the end of retail's files is refused over these.
    assert!(revise::after(
        &f.dats(),
        EraId::Eor,
        &first.join("overlay"),
        &second.join("overlay"),
        "test-world",
    )
    .is_err());
}
