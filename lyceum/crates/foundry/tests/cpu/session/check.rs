//! Behaviour: none (tooling: the world editor's checks, run again over only what an edit changed)

use dereth_primitives::DataId;
use lyceum_foundry::compile::StaticOrigin;
use lyceum_foundry::edit::{Op, Session};
use lyceum_foundry::scratch::Scratch;
use lyceum_foundry::source::{Height, Place, Rotation};

use crate::common::{Fixture, GFXOBJ, HOME};

/// What a check found, in a form two checks compare by: each fault's text and whether it warns,
/// sorted.
fn found(s: &Session, dir: &std::path::Path) -> Vec<(String, bool)> {
    let mut v: Vec<(String, bool)> = s
        .check(dir)
        .unwrap()
        .into_iter()
        .map(|f| (f.text, f.warning))
        .collect();
    v.sort();
    v
}

/// The checks run again after each edit, over only the landblocks it compiled again, find what a
/// session opened afresh on the same files finds: a static naming a model the files lack is
/// found where it is placed, and gone once it is moved off a model the files hold, then undone.
#[test]
fn the_checks_run_again_after_an_edit_find_what_a_fresh_check_finds() {
    let f = Fixture::new("check-again");
    let world = f.world();
    let mut s = Session::open(&world, &f.dats(), None).unwrap();
    s.set_today("2026-10-18 00:00:00");
    let (mut scratch, _) = Scratch::create(&f.path().join("scratch"), &s).unwrap();
    let clean = found(&s, scratch.dir());
    let fresh = |s: &mut Session, n: usize| {
        s.save().unwrap();
        let mut again = Session::open(&world, &f.dats(), None).unwrap();
        again.set_today("2026-10-18 00:00:00");
        let (scratch, _) = Scratch::create(&f.path().join(format!("fresh-{n}")), &again).unwrap();
        found(&again, scratch.dir())
    };
    let missing = DataId(0x0100_7777);
    let ops = [
        Op::PlaceStatic {
            landblock: HOME,
            model: missing,
            at: Place {
                x: 20.0,
                y: 20.0,
                z: Height::Ground,
            },
            rotation: Rotation::Heading(0.0),
            name: None,
        },
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
        Op::RemoveStatic {
            landblock: HOME,
            which: StaticOrigin::Base(0),
        },
    ];
    for (n, op) in ops.iter().enumerate() {
        let applied = s.apply(op).unwrap();
        scratch.update(&s, &applied.touched.landblocks).unwrap();
        let now = found(&s, scratch.dir());
        assert_eq!(now, fresh(&mut s, n), "after edit {}", n + 1);
        if n == 0 {
            assert!(
                now.iter().any(|(t, _)| t.contains("0x01007777")),
                "the missing model is found: {now:?}"
            );
        }
    }
    for _ in 0..ops.len() {
        let touched = s.undo().unwrap();
        scratch.update(&s, &touched.landblocks).unwrap();
    }
    assert_eq!(found(&s, scratch.dir()), clean);
}
