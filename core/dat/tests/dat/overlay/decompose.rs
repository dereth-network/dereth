//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! The decompose tool against two retail builds: the September 2013 files as the base and the
//! end-of-retail files as the world. Bytes decide membership, and the counts are the patch
//! retail itself sent a client moving between the two (866 portal and 83 language records, every
//! one whose iteration moved, of which the byte-identical re-stamps are left to the base). Laid
//! over the 2013 files, the overlay reads as the end-of-retail files.
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and the September 2013 capture
//! (`DERETH_TEST_DAT_CAPTURES_DIR`).

use std::path::PathBuf;
use std::sync::Arc;

use dereth_dat::decompose;
use dereth_dat::overlay::{Layer, OverlayDir};
use dereth_dat::{DatFile, ModernDat, ITERATION_LIST};

/// The September 2013 capture's file `name` (the captures spell names in lower case).
fn capture(name: &str) -> PathBuf {
    dereth_dat::testing::dat_captures_or_fail()
        .into_iter()
        .find(|(folder, file, _)| folder == "2013-09-06" && file.eq_ignore_ascii_case(name))
        .map(|(_, _, p)| p)
        .unwrap_or_else(|| panic!("the 2013-09-06 capture has no {name}"))
}

fn files(target: ModernDat) -> (DatFile, DatFile) {
    let base = DatFile::open(&capture(target.file_name())).expect("the 2013 file opens");
    let world = DatFile::open(&dereth_dat::testing::dat_file(target)).expect("the retail file");
    (base, world)
}

/// Lay the decomposed overlay over `base` and check it reads as `world` for every id `check`
/// names, and holds as many records.
fn assert_reads_as_world(target: ModernDat, base: &DatFile, world: &DatFile, check: &[u32]) {
    let scratch = dereth_dat::testing::ScratchDir::new("decompose").expect("scratch");
    let dir = OverlayDir::new(&scratch.path().join("overlay")).expect("an overlay folder");
    let d = decompose::diff(base, world).expect("the diff");
    let revision = decompose::write(
        base,
        world,
        &d,
        &dir.container(target),
        target.file_name(),
        "the end of retail over 2013",
        target == ModernDat::Cell,
        decompose::Iterations::Revision,
    )
    .expect("the overlay is written");
    assert_eq!(revision, decompose::revision_over(base));
    let layer = Layer::over(
        base,
        DatFile::open(&dir.container(target)).expect("the overlay opens"),
        Some("the end of retail over 2013"),
    )
    .expect("the overlay lies over its base");
    let layered = base.layered(Arc::new(layer));
    assert_eq!(layered.len(), world.len(), "as many records as the world");
    for id in check {
        let id = dereth_primitives::DataId(*id);
        if id == ITERATION_LIST {
            continue;
        }
        assert_eq!(
            layered.read(id).ok(),
            world.read(id).ok(),
            "{id:?} reads as the world's"
        );
    }
    assert!(
        layered.iteration_list().unwrap().contains(&revision),
        "the overlay's revision is among the file's iterations"
    );
}

#[test]
fn the_portal_decomposes_into_the_records_retail_patched_and_reads_as_the_end_of_retail() {
    let (base, world) = files(ModernDat::Portal);
    let d = decompose::diff(&base, &world).expect("the diff");
    assert_eq!(
        (d.added.len(), d.changed.len(), d.removed.len(), d.restamped),
        (733, 109, 0, 24)
    );
    assert_eq!(
        d.added.len() + d.changed.len() + d.restamped,
        866,
        "every record retail's patch sent"
    );
    // Every record the overlay holds, and a stride of the ones it leaves to the base.
    let mut check: Vec<u32> = d.added.iter().chain(&d.changed).copied().collect();
    check.extend(world.iter_ids().step_by(97).map(|i| i.raw()));
    assert_reads_as_world(ModernDat::Portal, &base, &world, &check);
}

#[test]
fn the_language_file_decomposes_into_the_records_retail_patched() {
    let (base, world) = files(ModernDat::Local);
    let d = decompose::diff(&base, &world).expect("the diff");
    assert_eq!((d.added.len(), d.changed.len(), d.removed.len()), (0, 6, 0));
    assert_eq!(
        d.changed.len() + d.restamped,
        83,
        "every record retail's patch sent"
    );
    let all: Vec<u32> = world.iter_ids().map(|i| i.raw()).collect();
    assert_reads_as_world(ModernDat::Local, &base, &world, &all);
}

#[test]
fn an_overlay_with_the_worlds_own_iterations_reports_exactly_the_worlds_list() {
    let (base, world) = files(ModernDat::Local);
    let d = decompose::diff(&base, &world).expect("the diff");
    let scratch = dereth_dat::testing::ScratchDir::new("decompose-world").expect("scratch");
    let dir = OverlayDir::new(&scratch.path().join("overlay")).expect("an overlay folder");
    decompose::write(
        &base,
        &world,
        &d,
        &dir.container(ModernDat::Local),
        ModernDat::Local.file_name(),
        "a world of its own numbering",
        false,
        decompose::Iterations::World,
    )
    .expect("the overlay is written");
    let layer = Layer::over(
        &base,
        DatFile::open(&dir.container(ModernDat::Local)).expect("the overlay opens"),
        None,
    )
    .expect("the overlay lies over its base");
    assert!(layer.manifest().exact_iterations);
    let layered = base.layered(Arc::new(layer));
    assert_eq!(
        layered.iteration_list().unwrap(),
        world.iteration_list().unwrap(),
        "exactly the world's list"
    );
    for id in &d.changed {
        let id = dereth_primitives::DataId(*id);
        assert_eq!(
            layered.entry(id).map(|e| e.iteration),
            world.entry(id).map(|e| e.iteration),
            "{id:?} keeps its world iteration"
        );
    }
}

#[test]
fn the_cell_file_decomposes_with_its_removed_rooms_as_tombstones() {
    let (base, world) = files(ModernDat::Cell);
    let d = decompose::diff(&base, &world).expect("the diff");
    assert_eq!(
        (d.added.len(), d.changed.len(), d.removed.len()),
        (7575, 165, 100)
    );
    let mut check: Vec<u32> = d
        .added
        .iter()
        .chain(&d.changed)
        .chain(&d.removed)
        .copied()
        .collect();
    check.extend(world.iter_ids().step_by(211).map(|i| i.raw()));
    assert_reads_as_world(ModernDat::Cell, &base, &world, &check);
}
