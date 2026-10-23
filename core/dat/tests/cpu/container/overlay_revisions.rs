//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! A world published again as the next revision of its overlay: what reads as it did keeps its
//! revision, what changed or is new goes into the next one, what is gone is deleted in it, and the
//! earlier deletions stay. A client patched to the last revision and given only the next one, and
//! a client given every revision, then read exactly the new world. A world that changes nothing
//! adds no revision; one that stops changing a file has it read as the base again.
//! Fixture: small containers made in a temp folder with the dat writer, and a client overlay
//! patched by revision as a server patches one.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dereth_dat::container::{CELL_DATFILE, LOCAL_DATFILE, PORTAL_DATFILE};
use dereth_dat::overlay::{Layer, OverlayDir, OverlayWriter};
use dereth_dat::revise::{self, ReviseError};
use dereth_dat::write::DatWriter;
use dereth_dat::{DatFile, ModernDat, ITERATION_LIST};
use dereth_primitives::DataId;

const WORLD: &str = "a world published twice";
const A: DataId = DataId(0x0600_0001);
const B: DataId = DataId(0x0600_0002);
const C: DataId = DataId(0x0600_0003);
const X: DataId = DataId(0x0600_0010);
const Y: DataId = DataId(0x0600_0011);
const LANDBLOCK: DataId = DataId(0xA9B4_FFFF);
const ROOM_1: DataId = DataId(0xA9B4_0100);
const ROOM_2: DataId = DataId(0xA9B4_0101);
const ELSEWHERE: DataId = DataId(0xA9B5_FFFF);

/// The base: ten iterations, the pictures A, B and C, and two landblocks, one with two rooms.
fn base_set(dir: &Path) {
    let make = |name: &str, block: u32, set: u32, subset: u32, records: &[(DataId, &[u8])]| {
        let mut w =
            DatWriter::create(&dir.join(name), block, set, subset, 0x400 + block * 64).unwrap();
        let all: Vec<u32> = (1..=10).collect();
        w.save(
            ITERATION_LIST,
            &dereth_dat::iteration::encode(&all),
            1,
            0,
            1,
        )
        .unwrap();
        for (id, bytes) in records {
            w.save(*id, bytes, 1, 5, 1).unwrap();
        }
    };
    make(
        ModernDat::Portal.file_name(),
        0x400,
        PORTAL_DATFILE,
        0,
        &[(A, b"a"), (B, b"b"), (C, b"c")],
    );
    make(
        ModernDat::Cell.file_name(),
        0x100,
        CELL_DATFILE,
        1,
        &[
            (LANDBLOCK, b"land"),
            (ROOM_1, b"room one"),
            (ROOM_2, b"room two"),
            (ELSEWHERE, b"elsewhere"),
        ],
    );
    make(ModernDat::Local.file_name(), 0x400, LOCAL_DATFILE, 1, &[]);
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dereth-revisions-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("base")).unwrap();
    d
}

/// What one build of the world changes in a file: records and deletions.
#[derive(Clone, Default)]
struct Changes {
    put: Vec<(DataId, &'static [u8])>,
    gone: Vec<DataId>,
}

/// A build's overlay, as a world builder writes one: every change in one revision past the base.
fn build(dir: &Path, bases: &[(ModernDat, DatFile)], files: &[(ModernDat, Changes)]) -> OverlayDir {
    let _ = std::fs::remove_dir_all(dir);
    let out = OverlayDir::new(dir).unwrap();
    for (t, changes) in files {
        let base = &bases.iter().find(|(b, _)| b == t).unwrap().1;
        let revision = dereth_dat::decompose::revision_over(base);
        let mut w =
            OverlayWriter::open_or_create(&out.container(*t), base, t.file_name(), WORLD, 0)
                .unwrap();
        for id in &changes.gone {
            w.tombstone(*id, 0, revision).unwrap();
        }
        for (id, bytes) in &changes.put {
            w.put(*id, bytes, 3, revision, 0).unwrap();
        }
        w.add_iteration(revision, 0).unwrap();
        w.flush(0).unwrap();
    }
    out
}

fn bases(dir: &Path) -> Vec<(ModernDat, DatFile)> {
    [ModernDat::Portal, ModernDat::Cell, ModernDat::Local]
        .into_iter()
        .map(|t| (t, DatFile::open(&t.in_dir(dir)).unwrap()))
        .collect()
}

/// `base` with the overlay in `dir` over it, when the folder has one for this file.
fn read_through(dir: &OverlayDir, t: ModernDat, base: &DatFile) -> DatFile {
    match dir.layer_over(t, base, None).unwrap() {
        Some(layer) => base.layered(Arc::new(layer)),
        None => base.base(),
    }
}

/// Bring the client's overlay `client` to the server's `served`, revision by revision, as a server
/// patches a client: each revision the client lacks has its deletions made and its records saved
/// (the save rule refusing an older record), and is then recorded. Answers the revisions sent.
fn patch(client: &OverlayDir, served: &OverlayDir, bases: &[(ModernDat, DatFile)]) -> Vec<u32> {
    let mut sent = Vec::new();
    for (t, base) in bases {
        let Some(server) = served.layer_over(*t, base, Some(WORLD)).unwrap() else {
            continue;
        };
        let have: BTreeSet<u32> = read_through(client, *t, base)
            .iteration_list()
            .unwrap()
            .into_iter()
            .collect();
        let missing: Vec<u32> = server
            .iterations()
            .iter()
            .copied()
            .filter(|r| !have.contains(r))
            .collect();
        if missing.is_empty() {
            continue;
        }
        let mut w = client.writer(*t, base, t.file_name(), WORLD, 1).unwrap();
        for r in &missing {
            for x in server.tombstones().iter().filter(|x| x.iteration == *r) {
                w.tombstone(DataId(x.id), x.mask, *r).unwrap();
            }
            let records: Vec<DataId> = server
                .records()
                .filter(|(_, e)| e.iteration == *r)
                .map(|(id, _)| id)
                .collect();
            for id in records {
                let bytes = server.read(id).unwrap();
                w.save(base, id, &bytes, 3, *r, 1).unwrap();
            }
            w.add_iteration(*r, 1).unwrap();
            sent.push(*r);
        }
        w.flush(1).unwrap();
    }
    sent
}

/// The client's files read exactly as the world's: every record either side has, and as many
/// records. The iteration lists are each overlay's own.
fn assert_reads_as(client: &OverlayDir, world: &OverlayDir, bases: &[(ModernDat, DatFile)]) {
    for (t, base) in bases {
        let mine = read_through(client, *t, base);
        let theirs = read_through(world, *t, base);
        assert_eq!(mine.len(), theirs.len(), "{t:?}: as many records");
        let ids: BTreeSet<DataId> = mine.iter_ids().chain(theirs.iter_ids()).collect();
        for id in ids.into_iter().filter(|id| *id != ITERATION_LIST) {
            assert_eq!(mine.read(id).ok(), theirs.read(id).ok(), "{t:?} {id:?}");
        }
    }
}

/// The client holds every revision the published overlay has.
fn assert_iterations_of(
    client: &OverlayDir,
    published: &OverlayDir,
    bases: &[(ModernDat, DatFile)],
) {
    for (t, base) in bases {
        assert_eq!(
            read_through(client, *t, base).iteration_list().unwrap(),
            read_through(published, *t, base).iteration_list().unwrap(),
            "{t:?}"
        );
    }
}

/// The first build: A changed, X added and B deleted in the portal; the first room changed.
fn first() -> Vec<(ModernDat, Changes)> {
    vec![
        (
            ModernDat::Portal,
            Changes {
                put: vec![(A, b"a, first"), (X, b"x")],
                gone: vec![B],
            },
        ),
        (
            ModernDat::Cell,
            Changes {
                put: vec![(ROOM_1, b"room one, first")],
                gone: vec![],
            },
        ),
    ]
}

/// The second build: A changed again, B back as it was, X gone, Y added and C deleted; the first
/// room as it was in the first build, the second room deleted.
fn second() -> Vec<(ModernDat, Changes)> {
    vec![
        (
            ModernDat::Portal,
            Changes {
                put: vec![(A, b"a, second"), (Y, b"y")],
                gone: vec![C],
            },
        ),
        (
            ModernDat::Cell,
            Changes {
                put: vec![(ROOM_1, b"room one, first")],
                gone: vec![ROOM_2],
            },
        ),
    ]
}

#[test]
fn a_client_patched_to_the_last_publish_is_sent_the_next_revision_alone_and_reads_the_new_world() {
    let root = scratch("twice");
    base_set(&root.join("base"));
    let bases = bases(&root.join("base"));
    let first_build = build(&root.join("build-1"), &bases, &first());
    let publish_1 = OverlayDir::new(&root.join("publish-1")).unwrap();
    let r1 = revise::revise_folder(&bases, None, &first_build, &publish_1, WORLD).unwrap();
    assert_eq!(
        r1.iter().map(|(_, r)| r.revision).collect::<Vec<_>>(),
        [Some(11), Some(11), None],
        "a world's first publish is one revision past its base"
    );
    assert!(
        !publish_1.holds(ModernDat::Local),
        "a file it does not change"
    );
    assert_reads_as(&publish_1, &first_build, &bases);

    // A client patched to the first publish.
    let client = OverlayDir::new(&root.join("client")).unwrap();
    assert_eq!(patch(&client, &publish_1, &bases), [11, 11]);
    assert_reads_as(&client, &first_build, &bases);
    assert_iterations_of(&client, &publish_1, &bases);

    let second_build = build(&root.join("build-2"), &bases, &second());
    let publish_2 = OverlayDir::new(&root.join("publish-2")).unwrap();
    let r2 =
        revise::revise_folder(&bases, Some(&publish_1), &second_build, &publish_2, WORLD).unwrap();
    let portal = &r2[0].1;
    assert_eq!(portal.revision, Some(12));
    assert_eq!(portal.revisions, [11, 12]);
    // A again and Y; B as the base has it, over its earlier deletion.
    assert_eq!((portal.changed, portal.deleted, portal.kept), (3, 2, 0));
    let cell = &r2[1].1;
    assert_eq!(
        (cell.revision, cell.changed, cell.deleted, cell.kept),
        (Some(12), 0, 1, 1)
    );
    assert_reads_as(&publish_2, &second_build, &bases);
    // What reads as it did keeps its revision; the earlier deletion stays in its own.
    let layer = |t: ModernDat| -> Layer {
        publish_2
            .layer_over(t, &bases.iter().find(|(b, _)| *b == t).unwrap().1, None)
            .unwrap()
            .unwrap()
    };
    assert_eq!(layer(ModernDat::Cell).record(ROOM_1).unwrap().iteration, 11);
    assert_eq!(layer(ModernDat::Portal).record(A).unwrap().iteration, 12);
    let gone: Vec<(u32, u32)> = layer(ModernDat::Portal)
        .tombstones()
        .iter()
        .map(|t| (t.id, t.iteration))
        .collect();
    assert_eq!(gone, [(C.raw(), 12), (X.raw(), 12)]);

    // The client patched to the first publish is sent the second's revision alone.
    assert_eq!(patch(&client, &publish_2, &bases), [12, 12]);
    assert_reads_as(&client, &second_build, &bases);
    assert_iterations_of(&client, &publish_2, &bases);
    // A client with nothing is sent both, and reads the same world.
    let new_client = OverlayDir::new(&root.join("new-client")).unwrap();
    assert_eq!(patch(&new_client, &publish_2, &bases), [11, 12, 11, 12]);
    assert_reads_as(&new_client, &second_build, &bases);
    assert_iterations_of(&new_client, &publish_2, &bases);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_publish_that_changes_nothing_adds_no_revision_and_one_that_stops_changing_a_file_restores_it()
{
    let root = scratch("again");
    base_set(&root.join("base"));
    let bases = bases(&root.join("base"));
    let first_build = build(&root.join("build-1"), &bases, &first());
    let publish_1 = OverlayDir::new(&root.join("publish-1")).unwrap();
    revise::revise_folder(&bases, None, &first_build, &publish_1, WORLD).unwrap();
    let client = OverlayDir::new(&root.join("client")).unwrap();
    patch(&client, &publish_1, &bases);

    // The same world again: the same revisions, and nothing for the client.
    let same = OverlayDir::new(&root.join("publish-same")).unwrap();
    let r = revise::revise_folder(&bases, Some(&publish_1), &first_build, &same, WORLD).unwrap();
    assert!(r
        .iter()
        .all(|(_, r)| r.revision.is_none() && r.changed + r.deleted == 0));
    assert_eq!(r[0].1.revisions, [11]);
    assert!(patch(&client, &same, &bases).is_empty());
    assert_reads_as(&same, &first_build, &bases);

    // The world stops changing the cell file: its next revision reads as the base.
    let portal_only = build(&root.join("build-portal"), &bases, &first()[..1]);
    let publish_2 = OverlayDir::new(&root.join("publish-2")).unwrap();
    let r = revise::revise_folder(&bases, Some(&same), &portal_only, &publish_2, WORLD).unwrap();
    assert_eq!(r[0].1.revision, None, "the portal reads as it did");
    assert_eq!(r[1].1.revision, Some(12));
    assert!(
        publish_2.holds(ModernDat::Cell),
        "a revision to bring a client back to the base"
    );
    assert_eq!(patch(&client, &publish_2, &bases), [12]);
    assert_reads_as(&client, &portal_only, &bases);
    assert_iterations_of(&client, &publish_2, &bases);
    let cell = read_through(&client, ModernDat::Cell, &bases[1].1);
    assert_eq!(cell.read(ROOM_1).unwrap(), b"room one");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_overlay_with_its_worlds_own_iterations_is_not_revised() {
    let root = scratch("own");
    base_set(&root.join("base"));
    let bases = bases(&root.join("base"));
    let own = OverlayDir::new(&root.join("own")).unwrap();
    let portal = &bases[0].1;
    let mut w = OverlayWriter::open_or_create(
        &own.container(ModernDat::Portal),
        portal,
        "client_portal.dat",
        WORLD,
        0,
    )
    .unwrap();
    w.put(A, b"a", 3, 7, 0).unwrap();
    w.set_exact_iterations(&[1, 2, 7], 0).unwrap();
    w.flush(0).unwrap();
    drop(w);
    let out = OverlayDir::new(&root.join("out")).unwrap();
    let e = revise::revise_folder(&bases, None, &own, &out, WORLD).unwrap_err();
    assert!(matches!(e, ReviseError::OwnIterations(_)), "{e}");
    let _ = std::fs::remove_dir_all(&root);
}
