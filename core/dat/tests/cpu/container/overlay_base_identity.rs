//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! Which base an overlay opens over: the file it was made against, known by its records rather
//! than by where they lie. The same records written in another order, on another day, are the same
//! base; a file with one record changed is not, however it is laid out; and an overlay written
//! before overlays named their bases by their records still opens over its own file, and names
//! its base's records once it is written again.
//! Fixture: small portal containers made in a temp folder with the dat writer.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dereth_dat::container::PORTAL_DATFILE;
use dereth_dat::overlay::{self, OverlayDir, OverlayError, MANIFEST};
use dereth_dat::write::DatWriter;
use dereth_dat::{DatFile, ModernDat, ITERATION_LIST};
use dereth_primitives::DataId;

const KEPT: DataId = DataId(0x0600_0001);
const REPLACED: DataId = DataId(0x0600_0002);
const OTHER: DataId = DataId(0x0600_0003);
const WORLD: &str = "a world";

/// The base's records, in the order the overlay's maker wrote them.
const RECORDS: [(DataId, &[u8]); 3] = [
    (KEPT, b"kept"),
    (REPLACED, b"old"),
    (OTHER, b"another record"),
];

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dereth-base-id-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A portal file in `dir` holding the iteration list and `records`, written in the order given,
/// each stamped `date`; the list is written last, so its place moves with the order too.
fn portal(dir: &Path, records: &[(DataId, &[u8])], date: u32) -> DatFile {
    std::fs::create_dir_all(dir).unwrap();
    let path = ModernDat::Portal.in_dir(dir);
    {
        let mut w = DatWriter::create(&path, 0x400, PORTAL_DATFILE, 0, 0x400 + 0x400 * 64).unwrap();
        for (id, bytes) in records {
            w.save(*id, bytes, 1, 5, date).unwrap();
        }
        let all: Vec<u32> = (1..=10).collect();
        w.save(
            ITERATION_LIST,
            &dereth_dat::iteration::encode(&all),
            1,
            0,
            date,
        )
        .unwrap();
    }
    DatFile::open(&path).unwrap()
}

/// The maker's overlay over `base`: `REPLACED` replaced in revision 11.
fn make_overlay(dir: &OverlayDir, base: &DatFile) {
    let mut w = dir
        .writer(ModernDat::Portal, base, "client_portal.dat", WORLD, 1)
        .unwrap();
    w.save(base, REPLACED, b"new", 1, 11, 1).unwrap();
    w.add_iteration(11, 1).unwrap();
    w.flush(1).unwrap();
}

fn reversed() -> Vec<(DataId, &'static [u8])> {
    RECORDS.iter().rev().copied().collect()
}

#[test]
fn the_same_records_in_another_block_order_are_the_same_base() {
    let root = scratch("reordered");
    let made = portal(&root.join("made"), &RECORDS, 1);
    // A copy of the maker's file written in the other order and patched on another day.
    let copy = portal(&root.join("copy"), &reversed(), 7);
    assert_ne!(
        overlay::fingerprint(&made),
        overlay::fingerprint(&copy),
        "the copy is laid out otherwise"
    );
    assert_eq!(
        overlay::records_digest(&made).unwrap(),
        overlay::records_digest(&copy).unwrap(),
        "and holds the same records"
    );

    let dir = OverlayDir::new(&root.join("overlay")).unwrap();
    make_overlay(&dir, &made);
    let layer = dir
        .layer_over(ModernDat::Portal, &copy, Some(WORLD))
        .expect("taken over the copy")
        .expect("a container");
    let world = copy.layered(Arc::new(layer));
    assert_eq!(world.read(REPLACED).unwrap(), b"new");
    assert_eq!(world.read(KEPT).unwrap(), b"kept");
    assert_eq!(
        world.iteration_list().unwrap(),
        (1..=11).collect::<Vec<_>>()
    );

    // Written again over the copy, the overlay is kept over the copy's layout and still names the
    // same records, so it opens over the maker's file too.
    let w = dir
        .writer(ModernDat::Portal, &copy, "client_portal.dat", WORLD, 2)
        .expect("the writer takes the copy");
    assert_eq!(w.manifest().base_fingerprint, overlay::fingerprint(&copy));
    assert_eq!(
        w.manifest().base_records,
        Some(overlay::records_digest(&made).unwrap())
    );
    drop(w);
    assert!(dir
        .layer_over(ModernDat::Portal, &made, Some(WORLD))
        .unwrap()
        .is_some());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_base_with_one_record_changed_is_refused_whatever_its_layout() {
    let root = scratch("changed");
    let made = portal(&root.join("made"), &RECORDS, 1);
    let dir = OverlayDir::new(&root.join("overlay")).unwrap();
    make_overlay(&dir, &made);
    // The other order again, with one record's bytes changed and its size kept.
    let mut changed = reversed();
    changed[2] = (KEPT, b"kepT");
    let other = portal(&root.join("other"), &changed, 7);

    let refused = dir.layer_over(ModernDat::Portal, &other, Some(WORLD));
    match refused {
        Err(OverlayError::BaseMismatch {
            expected, found, ..
        }) => {
            assert!(expected.starts_with("records "), "{expected}");
            assert!(
                found.starts_with("records ") && found != expected,
                "{found}"
            );
        }
        other => panic!("refused as another base, not {other:?}"),
    }
    assert!(matches!(
        dir.writer(ModernDat::Portal, &other, "client_portal.dat", WORLD, 2),
        Err(OverlayError::BaseMismatch { .. })
    ));
    let _ = std::fs::remove_dir_all(&root);
}

/// The manifest as overlays wrote it before they named their bases by their records: the first
/// layout, naming the base by its layout alone.
fn first_layout_manifest(base: &DatFile, hashes: &[(DataId, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"DOVL");
    out.extend_from_slice(&1u32.to_le_bytes());
    for s in [WORLD, "client_portal.dat"] {
        out.extend_from_slice(&u16::try_from(s.len()).unwrap().to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    }
    out.extend_from_slice(&overlay::fingerprint(base));
    out.extend_from_slice(&10u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&u32::try_from(hashes.len()).unwrap().to_le_bytes());
    for (id, bytes) in hashes {
        out.extend_from_slice(&id.raw().to_le_bytes());
        out.extend_from_slice(&overlay::record_hash(bytes));
    }
    out
}

#[test]
fn an_overlay_written_before_bases_were_named_by_their_records_still_opens() {
    let root = scratch("first-layout");
    let made = portal(&root.join("made"), &RECORDS, 1);
    let copy = portal(&root.join("copy"), &reversed(), 7);
    let dir = OverlayDir::new(&root.join("overlay")).unwrap();
    make_overlay(&dir, &made);
    {
        let mut w = DatWriter::open(&dir.container(ModernDat::Portal)).unwrap();
        w.save(
            MANIFEST,
            &first_layout_manifest(&made, &[(REPLACED, b"new")]),
            1,
            0,
            1,
        )
        .unwrap();
    }

    let layer = dir
        .layer_over(ModernDat::Portal, &made, Some(WORLD))
        .expect("the old overlay opens over its own file")
        .expect("a container");
    assert_eq!(layer.manifest().base_records, None);
    assert_eq!(
        made.layered(Arc::new(layer)).read(REPLACED).unwrap(),
        b"new"
    );
    // It names no records, so a copy laid out otherwise is not known to be its base.
    assert!(matches!(
        dir.layer_over(ModernDat::Portal, &copy, Some(WORLD)),
        Err(OverlayError::BaseMismatch { .. })
    ));

    // Written again over its own file, it names its base's records, and the copy takes it.
    let w = dir
        .writer(ModernDat::Portal, &made, "client_portal.dat", WORLD, 2)
        .unwrap();
    assert_eq!(
        w.manifest().base_records,
        Some(overlay::records_digest(&made).unwrap())
    );
    drop(w);
    let layer = dir
        .layer_over(ModernDat::Portal, &copy, Some(WORLD))
        .expect("now taken over the copy")
        .expect("a container");
    assert_eq!(
        copy.layered(Arc::new(layer)).read(REPLACED).unwrap(),
        b"new"
    );
    let _ = std::fs::remove_dir_all(&root);
}
