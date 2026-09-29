//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! A missile target id makes other creatures pass (client missiles still hit creatures); an absent
//! gate keeper closes a cell unless an installed host answers; a committed host sweep moves body,
//! cell, contact and shadows.
//! Fixture: synthetic state, geometry and reference vectors.

use std::sync::{Arc, Mutex};

use dereth_physics::obj::WeenieRestrictions;
use dereth_physics::{PhysHandle, PhysicsState, PhysicsWorld};
use dereth_primitives::{Frame, LandblockId, ObjectId, Position, Quat, Vec3};

// A world of nine flat landblocks around `(0xA9, 0xB4)`, ground at z = 20.

fn at(x: f32, y: f32, z: f32) -> Position {
    let mut c = LandblockId::new(0xA9, 0xB4).cell(1);
    let mut origin = Vec3::new(x, y, z);
    dereth_physics::landdefs::adjust_to_outside(&mut c, &mut origin);
    Position::new(c, Frame::new(Vec3::new(x, y, z), Quat::IDENTITY))
}

fn place(w: &mut PhysicsWorld, id: u32, pos: Position) -> PhysHandle {
    let h = w.create(ObjectId(id), body_geometry(), true);
    w.enter_cell(h, pos.cell);
    w.get_mut(h).expect("live").position = pos;
    w.calc_cross_cells(h, false);
    h
}

/// `0x20748`, an arrow's state word from the locked corpus: `INELASTIC | GRAVITY | PATHCLIPPED |
/// ALIGNPATH | MISSILE | REPORT_COLLISIONS`.
const ARROW: u32 = 0x0002_0748;

/// What a missile sweeping from x = 96 to x = 108 makes of a body standing at x = 102.
#[derive(Debug, PartialEq)]
enum Outcome {
    Ignored,
    Blocked,
}

fn sweep(target: u32, blocker: Option<WeenieRestrictions>, blocker_state: u32) -> Outcome {
    let mut w = flat_world();
    let missile = place(&mut w, 1, at(96.0, 100.0, 21.0));
    let wall = place(&mut w, 2, at(102.0, 100.0, 20.5));
    {
        let o = w.get_mut(missile).expect("live");
        o.state = PhysicsState(ARROW);
        o.projectile_target_id = ObjectId(target);
    }
    w.get_mut(wall).expect("live").state = PhysicsState(blocker_state);
    w.set_weenie_restrictions(wall, blocker);
    let t = w.transition(
        missile,
        &at(96.0, 100.0, 21.0),
        &at(108.0, 100.0, 21.0),
        false,
    );
    let t = t.expect("the sweep completes");
    let passed = t.sphere_path.curr_pos.frame.origin.x > 107.0;
    let ignored = t.counters.objects_missile_ignored > 0;
    assert_eq!(
        passed, ignored,
        "the ignore counter and the end position agree ({:?})",
        t.sphere_path.curr_pos
    );
    if passed {
        Outcome::Ignored
    } else {
        Outcome::Blocked
    }
}

fn creature() -> Option<WeenieRestrictions> {
    Some(WeenieRestrictions {
        is_creature: true,
        ..WeenieRestrictions::default()
    })
}
fn item() -> Option<WeenieRestrictions> {
    Some(WeenieRestrictions::default())
}
const DYNAMIC: u32 = 0x0000_0408; // REPORT_COLLISIONS | GRAVITY: a plain dynamic body

#[test]
fn a_missile_with_a_target_passes_every_other_weenie_creature() {
    assert_eq!(
        sweep(0x77, creature(), DYNAMIC),
        Outcome::Ignored,
        "a non-target creature is passed through"
    );
    assert_eq!(
        sweep(2, creature(), DYNAMIC),
        Outcome::Blocked,
        "the target itself is hit"
    );
}

#[test]
fn a_missile_without_a_target_hits_creatures_as_the_client_does() {
    // The client never writes the target: zero, and the creature arm cannot fire.
    assert_eq!(sweep(0, creature(), DYNAMIC), Outcome::Blocked);
}

#[test]
fn the_creature_arm_needs_a_weenie_that_is_a_creature() {
    assert_eq!(
        sweep(0x77, item(), DYNAMIC),
        Outcome::Blocked,
        "a weenie that is not a creature"
    );
    assert_eq!(
        sweep(0x77, None, DYNAMIC),
        Outcome::Blocked,
        "no weenie at all"
    );
}

// ---------------------------------------------------------------------------------- A13

/// A13: a sweep run with `transition` and then committed with `commit_transition` leaves the body
/// where the sweep ended, in the sweep's cell, with the contact the sweep found and its shadows in
/// the cells it now overlaps. The control, the same sweep uncommitted, changes nothing.
#[test]
fn a_committed_sweep_moves_the_body_its_cell_contact_and_shadows() {
    let run = |commit: bool| {
        let mut w = flat_world();
        // x = 120 is the seam between outdoor cells (4, 4) and (5, 4)
        let h = place(&mut w, 1, at(110.0, 100.0, 20.0));
        let start_cell = w.get(h).expect("live").cell;
        let t = w
            .transition(h, &at(110.0, 100.0, 20.0), &at(130.0, 100.0, 20.0), false)
            .expect("the sweep completes");
        let end = t.sphere_path.curr_pos;
        if commit {
            w.commit_transition(h, &t);
        }
        (w, h, start_cell, end)
    };

    let (w, h, start_cell, end) = run(true);
    let o = w.get(h).expect("live");
    assert!(
        (end.frame.origin.x - 130.0).abs() < 1e-3,
        "the sweep reaches its end on flat ground: {end:?}"
    );
    assert_eq!(
        o.position.frame.origin, end.frame.origin,
        "the frame is the sweep's"
    );
    assert_eq!(o.cell, Some(end.cell), "the cell is the sweep's");
    assert_ne!(o.cell, start_cell, "and a different one from the start");
    assert!(
        o.transient_state.in_contact() && o.transient_state.on_walkable(),
        "standing on the ground it found"
    );
    assert!(
        o.contact_plane.normal.z > 0.99,
        "the flat ground's plane: {:?}",
        o.contact_plane
    );
    assert!(
        o.shadow_objects.iter().any(|s| s.cell_id == end.cell),
        "a shadow in the new cell"
    );
    assert!(
        w.transition_ctx(None).shadow_objects(end.cell).contains(&h),
        "the new cell lists the body"
    );

    let (w, h, start_cell, _) = run(false);
    let o = w.get(h).expect("live");
    assert!(
        (o.position.frame.origin.x - 110.0).abs() < 1e-6,
        "uncommitted: the body has not moved"
    );
    assert_eq!(o.cell, start_cell);
    assert!(
        !o.transient_state.in_contact(),
        "and has not taken the sweep's contact"
    );
}

// ---------------------------------------------------------------------------------- A12

/// A host that answers every entry question with `allow`, and records each question it was asked.
struct Gate {
    allow: bool,
    asked: Mutex<Vec<(ObjectId, ObjectId)>>,
}

impl dereth_physics::transition::EntryRestrictionHost for Gate {
    fn can_move_into(&self, restriction: ObjectId, mover: ObjectId) -> bool {
        self.asked
            .lock()
            .expect("unpoisoned")
            .push((restriction, mover));
        self.allow
    }
}

const GATE_KEEPER: ObjectId = ObjectId(0x7000_0777);

/// A player walking from x = 110 across the seam at x = 120 into a cell whose gate keeper is
/// [`GATE_KEEPER`] (absent from the world), under `host`. Answers the x it reached.
fn cross_into_restricted_cell(host: Option<Arc<Gate>>) -> f32 {
    let mut w = flat_world();
    let h = place(&mut w, 1, at(110.0, 100.0, 20.0));
    w.set_weenie_restrictions(
        h,
        Some(WeenieRestrictions {
            is_player: true,
            ..WeenieRestrictions::default()
        }),
    );
    w.set_cell_restriction(at(130.0, 100.0, 20.0).cell, Some(GATE_KEEPER));
    if let Some(host) = host {
        w.set_entry_restriction_host(Some(host));
    }
    let t = w.transition(h, &at(110.0, 100.0, 20.0), &at(130.0, 100.0, 20.0), false);
    t.map_or(110.0, |t| t.sphere_path.curr_pos.frame.origin.x)
}

#[test]
fn without_a_host_an_absent_gate_keeper_closes_the_cell() {
    // The client's lookup: a gate keeper that cannot be found is a closed gate.
    assert!(cross_into_restricted_cell(None) < 120.0);
}

/// The refusal record of the same crossing: `restricted_by` and whether a collision normal was
/// set.
fn refusal_of_crossing(host: Option<Arc<Gate>>) -> (Option<ObjectId>, bool) {
    let mut w = flat_world();
    let h = place(&mut w, 1, at(110.0, 100.0, 20.0));
    w.set_weenie_restrictions(
        h,
        Some(WeenieRestrictions {
            is_player: true,
            ..WeenieRestrictions::default()
        }),
    );
    w.set_cell_restriction(at(130.0, 100.0, 20.0).cell, Some(GATE_KEEPER));
    if let Some(host) = host {
        w.set_entry_restriction_host(Some(host));
    }
    let t = w
        .transition(h, &at(110.0, 100.0, 20.0), &at(130.0, 100.0, 20.0), false)
        .expect("a transition");
    (
        t.collision_info.restricted_by,
        t.collision_info.collision_normal_valid,
    )
}

/// An absent gate keeper closes the cell without the restriction handler.
#[test]
fn an_absent_gate_keeper_closes_the_cell_without_the_restriction_handler() {
    let (restricted_by, _) = refusal_of_crossing(None);
    assert_eq!(
        restricted_by, None,
        "no refusal record for a gate keeper that is not there"
    );
    assert!(
        cross_into_restricted_cell(None) < 120.0,
        "and the cell stays closed"
    );

    // a gate keeper that is there and refuses (here the host's answer) still gets both
    let closed = Arc::new(Gate {
        allow: false,
        asked: Mutex::new(Vec::new()),
    });
    let (restricted_by, normal) = refusal_of_crossing(Some(closed));
    assert_eq!(restricted_by, Some(GATE_KEEPER));
    assert!(normal);
}

#[test]
fn an_installed_host_answers_the_entry_question() {
    let open = Arc::new(Gate {
        allow: true,
        asked: Mutex::new(Vec::new()),
    });
    assert!(
        cross_into_restricted_cell(Some(Arc::clone(&open))) > 129.0,
        "the host opens the gate"
    );
    let asked = open.asked.lock().expect("unpoisoned").clone();
    assert!(!asked.is_empty(), "the host was asked");
    assert!(
        asked.iter().all(|&q| q == (GATE_KEEPER, ObjectId(1))),
        "about this gate keeper and mover: {asked:?}"
    );

    let closed = Arc::new(Gate {
        allow: false,
        asked: Mutex::new(Vec::new()),
    });
    assert!(
        cross_into_restricted_cell(Some(Arc::clone(&closed))) < 120.0,
        "the host closes the gate"
    );
    assert!(!closed.asked.lock().expect("unpoisoned").is_empty());
}

use crate::common::physics_fixture::{flat_world, player_geometry as body_geometry};
