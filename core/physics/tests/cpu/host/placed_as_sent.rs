//! A body whose game record is a hook, a storage chest or a corpse goes into its cell exactly where
//! it is asked to be, on entering the world and on every later position set, even overlapping
//! another body, and it is registered in the cells it reaches again only when it changes cell. The
//! same body with any other game record, or with none, is placed: put down beside what it overlaps.
//! Fixture: nine flat landblocks and single-sphere bodies.

use dereth_physics::obj::WeenieRestrictions;
use dereth_physics::{landdefs, PhysHandle, PhysicsWorld};
use dereth_primitives::{CellId, Frame, LandblockId, ObjectId, Position, Quat, Vec3};

use crate::common::physics_fixture::{flat_world, player_geometry};

/// The flat world's ground.
const GROUND: f32 = 20.0;
/// Where the standing body is, and where the arrivals are asked to be: a quarter of a metre
/// along x, well inside the standing body's half-metre sphere.
const STANDING: (f32, f32) = (100.0, 100.0);
const OVERLAPPING: (f32, f32) = (100.25, 100.0);

fn at(x: f32, y: f32) -> Position {
    let mut cell = LandblockId::new(0xA9, 0xB4).cell(1);
    let mut origin = Vec3::new(x, y, GROUND);
    assert!(
        landdefs::adjust_to_outside(&mut cell, &mut origin),
        "({x}, {y}) is off the world"
    );
    Position::new(cell, Frame::new(Vec3::new(x, y, GROUND), Quat::IDENTITY))
}

/// The game records under test: the three that are placed as sent, and the controls.
fn hook() -> Option<WeenieRestrictions> {
    Some(WeenieRestrictions {
        is_hook: true,
        ..WeenieRestrictions::default()
    })
}
fn storage() -> Option<WeenieRestrictions> {
    Some(WeenieRestrictions {
        is_storage: true,
        ..WeenieRestrictions::default()
    })
}
fn corpse() -> Option<WeenieRestrictions> {
    Some(WeenieRestrictions {
        is_corpse: true,
        ..WeenieRestrictions::default()
    })
}
fn creature() -> Option<WeenieRestrictions> {
    Some(WeenieRestrictions {
        is_creature: true,
        ..WeenieRestrictions::default()
    })
}

/// A world with a solid body standing on the ground at [`STANDING`].
fn occupied() -> PhysicsWorld {
    let mut w = flat_world();
    let h = w.create(ObjectId(1), player_geometry(), true);
    w.set_weenie_restrictions(h, creature());
    assert!(w.enter_world(h, &at(STANDING.0, STANDING.1)));
    assert_eq!(
        w.get(h).expect("live").position.frame.origin,
        at(STANDING.0, STANDING.1).frame.origin,
        "the standing body was itself moved, so the overlap under test is not the one asked for"
    );
    w
}

/// A body of the same shape with `record`, not in the world yet.
fn arrival(w: &mut PhysicsWorld, record: Option<WeenieRestrictions>) -> PhysHandle {
    let h = w.create(ObjectId(2), player_geometry(), true);
    w.set_weenie_restrictions(h, record);
    h
}

/// How far from `asked` the body is standing, or `None` when it is in no cell.
fn off(w: &PhysicsWorld, h: PhysHandle, asked: &Position) -> Option<f32> {
    let o = w.get(h).expect("live");
    o.cell?;
    let d = o.position.frame.origin;
    let a = asked.frame.origin;
    Some(((d.x - a.x).powi(2) + (d.y - a.y).powi(2) + (d.z - a.z).powi(2)).sqrt())
}

/// Behaviour: movement.remote-body.a-hook-a-storage-chest-or-a-corpse-is-put-down-exactly-where-sent
#[test]
fn a_hook_a_storage_chest_or_a_corpse_enters_the_world_exactly_where_asked_over_another_body() {
    let asked = at(OVERLAPPING.0, OVERLAPPING.1);
    for (name, record) in [
        ("hook", hook()),
        ("storage", storage()),
        ("corpse", corpse()),
    ] {
        let mut w = occupied();
        let h = arrival(&mut w, record);
        assert!(w.enter_world(h, &asked), "the {name} entered the world");
        let o = w.get(h).expect("live");
        assert_eq!(
            o.position, asked,
            "the {name} is exactly where it was asked to be"
        );
        assert_eq!(
            o.cell,
            Some(asked.cell),
            "the {name} is in the cell it was asked for"
        );
        assert!(
            o.transient_state.is_active(),
            "the {name} is active like any other non-static arrival"
        );
    }
    // The controls: the same body as a creature, as an object with a game record that is none of
    // the three, and with no game record at all, is put down beside the body it overlaps.
    for (name, record) in [
        ("creature", creature()),
        ("plain record", Some(WeenieRestrictions::default())),
        ("no record", None),
    ] {
        let mut w = occupied();
        let h = arrival(&mut w, record);
        assert!(w.enter_world(h, &asked), "the {name} entered the world");
        let moved = off(&w, h, &asked).expect("the arrival is in a cell");
        assert!(
            moved > 0.1 && moved <= 4.0,
            "the {name} was put down {moved:.4} m from where it was asked, not beside the body"
        );
    }
}

/// Behaviour: movement.remote-body.a-hook-a-storage-chest-or-a-corpse-is-put-down-exactly-where-sent
#[test]
fn every_later_position_set_puts_it_exactly_where_asked_too() {
    let clear = at(90.0, 100.0);
    let asked = at(OVERLAPPING.0, OVERLAPPING.1);
    for (name, record, exact) in [
        ("corpse", corpse(), true),
        ("storage", storage(), true),
        ("creature", creature(), false),
    ] {
        // The plain position set, and the teleport arm of a position update, which is one.
        for teleport in [false, true] {
            let mut w = occupied();
            let h = arrival(&mut w, record.clone());
            assert!(w.enter_world(h, &clear));
            assert_eq!(
                off(&w, h, &clear),
                Some(0.0),
                "the {name} entered in the clear"
            );
            if teleport {
                assert!(
                    w.move_or_teleport(h, &asked, true, true).placed(),
                    "the {name} was teleported into a cell"
                );
            } else {
                assert!(w.set_position(h, &asked), "the {name}'s position was set");
            }
            let moved = off(&w, h, &asked).expect("the body is in a cell");
            if exact {
                assert_eq!(
                    w.get(h).expect("live").position,
                    asked,
                    "the {name} is exactly where it was asked to be (teleport: {teleport})"
                );
            } else {
                assert!(
                    moved > 0.1,
                    "the {name} was left {moved:.4} m from the asked spot (teleport: {teleport})"
                );
            }
        }
    }
}

/// Behaviour: movement.remote-body.a-hook-a-storage-chest-or-a-corpse-is-put-down-exactly-where-sent
#[test]
fn it_is_registered_again_only_when_it_changes_cell() {
    // Land cells are 24 m: x = 96 is a cell line. The body's 1 m sorting sphere reaches over it
    // from x = 96.5, and from x = 104 it does not.
    let mut w = flat_world();
    let h = arrival(&mut w, corpse());
    let first = at(96.5, 100.0);
    assert!(w.enter_world(h, &first));
    let cells = |w: &PhysicsWorld| -> Vec<CellId> {
        let mut v: Vec<CellId> = w
            .get(h)
            .expect("live")
            .shadow_objects
            .iter()
            .map(|s| s.cell_id)
            .collect();
        v.sort_by_key(|c| c.0);
        v
    };
    let registered = cells(&w);
    let west = at(95.0, 100.0).cell;
    assert!(
        registered.contains(&first.cell) && registered.contains(&west),
        "entering the world registered it in its own cell and the one its sphere reaches over          the line: {registered:?}"
    );

    // Within its cell, clear of the line: the frame moves, the registration stays.
    let within = at(104.0, 100.0);
    assert_eq!(
        within.cell, first.cell,
        "the second spot is in the same cell"
    );
    assert!(w.set_position(h, &within));
    assert_eq!(w.get(h).expect("live").position.frame, within.frame);
    assert_eq!(
        cells(&w),
        registered,
        "a move within its cell keeps the cells it was registered in"
    );

    // Into another cell: it changes cell and is registered where it now reaches.
    let across = at(80.0, 100.0);
    assert_ne!(across.cell, first.cell, "the third spot is in another cell");
    assert!(w.set_position(h, &across));
    let o = w.get(h).expect("live");
    assert_eq!(o.cell, Some(across.cell));
    assert_eq!(o.position, across);
    let now = cells(&w);
    assert!(
        now.contains(&across.cell) && !now.contains(&first.cell),
        "it is registered in the cells it reaches from its new cell: {now:?}"
    );
}
