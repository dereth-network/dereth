//! Behaviour: none (tooling: the overlay the world editor draws, kept edit by edit)

use std::collections::BTreeMap;
use std::path::Path;

use dereth_dat::{DatFile, ModernDat};
use dereth_primitives::{DataId, Vec3};
use lyceum_foundry::compile::{Field, StaticOrigin};
use lyceum_foundry::edit::{Op, Session, VertexSet, Weenie};
use lyceum_foundry::scratch::Scratch;
use lyceum_foundry::source::{Height, Place, Rotation};

use crate::common::{Fixture, GFXOBJ, HOME, SETUP};

/// The class id the test's server weenie is, and the SQL that names its setup and its name.
const DRUDGE: u32 = 7924;
const WEENIES: &str = "\
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (7924, 1, 0x02000001) /* Setup */;
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (7924, 1, 'Test Drudge') /* Name */;
";

/// Every record of every container in the overlay folder `dir`: its header fields and its bytes,
/// by container and id.
type Held = BTreeMap<&'static str, BTreeMap<u32, (u32, u32, u32, Vec<u8>)>>;

fn held(dir: &Path) -> Held {
    let mut out = Held::new();
    for t in ModernDat::ALL {
        let name = dereth_dat::overlay::container_name(t);
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let file = DatFile::open(&path).unwrap();
        let records = file
            .iter_entries()
            .map(|(id, e)| {
                (
                    id.raw(),
                    (e.bits, e.date, e.iteration, file.read(id).unwrap()),
                )
            })
            .collect();
        out.insert(name, records);
    }
    out
}

#[test]
fn the_overlay_kept_edit_by_edit_holds_what_a_whole_write_holds_record_for_record() {
    let f = Fixture::new("scratch-steps");
    let world = f.world();
    let mut s = Session::open(&world, &f.dats(), None).unwrap();
    s.set_today("2026-10-18 00:00:00");
    s.learn_weenies(WEENIES);
    let (mut scratch, first) = Scratch::create(&f.path().join("scratch"), &s).unwrap();
    assert!(first.whole);
    let spawn = f.path().join(format!(
        "sample/server/sql/landblocks/0201 weenie-{DRUDGE}.sql"
    ));
    let ops = vec![
        Op::PlaceStatic {
            landblock: 0x0202,
            model: DataId(GFXOBJ),
            at: Place {
                x: 10.0,
                y: 10.0,
                z: Height::Ground,
            },
            rotation: Rotation::Heading(0.0),
            name: None,
        },
        // A hill over the corner the four landblocks share.
        Op::SetVertices(
            [(15, 15), (16, 16), (17, 16)]
                .into_iter()
                .map(|(gx, gy)| VertexSet {
                    gx,
                    gy,
                    field: Field::Height,
                    value: 60,
                })
                .collect(),
        ),
        Op::MoveStatic {
            landblock: HOME,
            which: StaticOrigin::Base(0),
            at: Some(Place {
                x: 50.0,
                y: 60.0,
                z: Height::At(1.0),
            }),
            rotation: Some(Rotation::Heading(10.0)),
        },
        // A spawn drawn by its setup in a landblock with no statics record: the scratch overlay
        // makes one for it.
        Op::PlaceSpawn {
            landblock: 0x0201,
            weenie: Weenie::Class(DRUDGE),
            origin: Vec3::new(100.0, 100.0, 0.0),
            heading: 0.0,
        },
        Op::MoveSpawn {
            file: spawn.clone(),
            guid: "{{guid:weenie-7924@0201}}".into(),
            origin: Vec3::new(120.0, 40.0, 0.0),
            heading: 90.0,
        },
        Op::RemoveStatic {
            landblock: HOME,
            which: StaticOrigin::Base(1),
        },
    ];
    let mut n = 0;
    // The landblocks the edit before reached: the folder written now catches up on them too.
    let mut last = std::collections::BTreeSet::new();
    let mut check =
        |s: &Session, scratch: &mut Scratch, touched: &std::collections::BTreeSet<u16>| {
            let before = scratch.dir().to_path_buf();
            let w = scratch.update(s, touched).unwrap();
            assert!(!w.whole || n == 0, "only a folder's first write is whole");
            assert_ne!(
                w.dir, before,
                "the folder the store reads is not the one written"
            );
            let reached: std::collections::BTreeSet<u16> = touched.union(&last).copied().collect();
            if !w.whole {
                assert!(
                    w.put + w.reverted <= 2 * reached.len(),
                    "{} put and {} taken out for {} landblock(s)",
                    w.put,
                    w.reverted,
                    reached.len()
                );
            }
            last.clone_from(touched);
            n += 1;
            let whole = f.path().join(format!("whole-{n}"));
            s.write_scratch(&whole).unwrap();
            assert_eq!(held(&w.dir), held(&whole), "after step {n}");
            w
        };
    for op in &ops {
        let applied = s.apply(op).unwrap();
        check(&s, &mut scratch, &applied.touched.landblocks);
    }
    // Taking the spawn and its move back leaves 0x0201 with no statics record, as the files
    // have it: the one the spawn's preview made is taken out of the folder again.
    let mut reverted = 0;
    for _ in 0..3 {
        let touched = s.undo().unwrap();
        reverted += check(&s, &mut scratch, &touched.landblocks).reverted;
    }
    assert!(reverted >= 1, "the spawn's statics record is gone again");
    let touched = s.redo().unwrap();
    check(&s, &mut scratch, &touched.landblocks);
    // Written again for the same landblocks, unchanged since: each folder catches up once, and
    // after that holds every record as it is and writes none.
    for _ in 0..2 {
        check(&s, &mut scratch, &touched.landblocks);
    }
    let again = check(&s, &mut scratch, &touched.landblocks);
    assert_eq!((again.put, again.reverted), (0, 0), "{again:?}");
    assert!(again.kept > 0);
    // Written on another thread, begun and finished on this one: the same folder.
    let touched = s.undo().unwrap();
    let writing = scratch.begin(&s, &touched.landblocks).unwrap();
    let done = std::thread::spawn(move || writing.run()).join().unwrap();
    let w = scratch.finish(done).unwrap();
    assert_eq!(scratch.dir(), w.dir);
    let whole = f.path().join("whole-thread");
    s.write_scratch(&whole).unwrap();
    assert_eq!(held(&w.dir), held(&whole));
}

#[test]
fn a_server_weenies_name_names_the_setup_it_is_drawn_with() {
    let f = Fixture::new("scratch-names");
    let world = f.world();
    let mut s = Session::open(&world, &f.dats(), None).unwrap();
    s.learn_weenies(WEENIES);
    let names = s.weenie_names_by_setup();
    assert_eq!(names.get(&SETUP), Some(&vec!["Test Drudge".to_owned()]));
    assert_eq!(names.len(), 1);
}
