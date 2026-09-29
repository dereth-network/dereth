//! Ethereal on is unconditional; off takes immediately when clear, else is refused, latched and
//! retried every substep; parented/cell-less objects skip the overlap test; static objects never
//! hold it open; an object never blocks itself.
//! Fixture: synthetic state, geometry and reference vectors.

use std::sync::Arc;

use dereth_physics::geom::Sphere;
use dereth_physics::{EtherealResult, PhysHandle, PhysicsWorld, SetupGeometry};
use dereth_primitives::{CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};

const BLOCK: LandblockId = LandblockId(0xA9B4);
/// Ground height of a flat block at table index 10 with the linear `2 * i` table.
const GROUND: f32 = 20.0;

/// which land cell of the block a block-local point
/// falls in. An object registered into one cell and looked for in another is silently not there.
fn cell_at(p: Vec3) -> CellId {
    let mut c = BLOCK.cell(1);
    let mut o = p;
    assert!(dereth_physics::landdefs::adjust_to_outside(&mut c, &mut o));
    c
}

// A body: one 0.5 m sphere centred half a metre up, which is the shape every other test in this
// crate walks with.

fn door_geometry() -> Arc<SetupGeometry> {
    Arc::new(SetupGeometry {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 1.0), 1.0)],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 1.0), 1.6),
        radius: 1.0,
        height: 2.0,
        ..SetupGeometry::default()
    })
}

/// Put an object in the world at `at`, registered in its cells the way
/// the object's cross-cell calculation does.
fn place(
    w: &mut PhysicsWorld,
    id: u32,
    at: Vec3,
    g: Arc<SetupGeometry>,
    dynamic: bool,
) -> PhysHandle {
    let h = w.create(ObjectId(id), g, dynamic);
    let cell = cell_at(at);
    w.enter_cell(h, cell);
    let frame = Frame::new(at, Quat::IDENTITY);
    if let Some(o) = w.get_mut(h) {
        o.set_frame(frame);
        o.position = Position::new(cell, frame);
        o.update_time = 0.0;
    }
    w.calc_cross_cells(h, true);
    h
}

/// Move an object and re-register its shadows, which is what a body walking does.
fn move_to(w: &mut PhysicsWorld, h: PhysHandle, at: Vec3) {
    let cell = cell_at(at);
    w.leave_cell(h);
    w.enter_cell(h, cell);
    let frame = Frame::new(at, Quat::IDENTITY);
    if let Some(o) = w.get_mut(h) {
        o.set_frame(frame);
        o.position = Position::new(cell, frame);
    }
    w.calc_cross_cells(h, true);
}

fn is_ethereal(w: &PhysicsWorld, h: PhysHandle) -> bool {
    w.get(h).expect("live").state.is_ethereal()
}

fn latched(w: &PhysicsWorld, h: PhysHandle) -> bool {
    w.get(h).expect("live").transient_state.check_ethereal()
}

/// The door, and a body standing exactly in it.
///
/// **The door is created `dynamic`, i.e. *not* `STATIC_PS`, and that is load-bearing rather than
/// incidental.** The object's collision search ends with
///
/// `text
/// if (TVar12 != OK_TS && !step_down) {
///     if (state & STATIC_PS)            { ... collided_with_environment = 1; }   // TVar12 KEPT
///     else if (obstruction_ethereal ...) { TVar12 = OK_TS; ... }                  // exempted
/// `
///
/// so the ethereal exemption lives in the **else**-arm of the static test: an object that is both
/// `STATIC_PS` and `ETHEREAL_PS` still blocks. A door has a motion table and animates, so it is
/// not static — but this is written down because building the fixture the other way makes every
/// assertion below fail for a reason that has nothing to do with `set_ethereal`, which is how it
/// was found. See [`a_static_ethereal_candidate_still_blocks`].
fn door_with_a_body_inside() -> (PhysicsWorld, PhysHandle, PhysHandle) {
    let mut w = flat_world();
    let at = Vec3::new(90.0, 90.0, GROUND);
    let door = place(&mut w, 1, at, door_geometry(), true);
    let body = place(&mut w, 2, at, body_geometry(), true);
    (w, door, body)
}

// ---------------------------------------------------------------------------------------------
// 1. The easy direction
// ---------------------------------------------------------------------------------------------

/// The other arm just sets state bit `4` — there is no test on this arm at all.
///
/// A door that opens on top of somebody still becomes ethereal, which is what makes opening a
/// door safe in the first place.
#[test]
fn turning_ethereal_on_is_unconditional_even_with_a_body_standing_in_the_object() {
    let (mut w, door, body) = door_with_a_body_inside();
    assert!(!is_ethereal(&w, door), "the door starts solid");
    // The guard would fire if it were consulted: prove that before relying on the negative.
    assert!(
        w.ethereal_check_for_collisions(door),
        "the calibration half: with a body in the doorway the overlap test must read BLOCKED, \
         otherwise the assertion below is made by an instrument that sees nothing"
    );
    assert_eq!(w.set_ethereal(door, true, false), EtherealResult::Applied);
    assert!(is_ethereal(&w, door), "the ON arm is unconditional");
    assert!(!latched(&w, door), "nothing is queued for retry");
    let _ = body;
}

// ---------------------------------------------------------------------------------------------
// 2. The hard direction
// ---------------------------------------------------------------------------------------------

/// With the doorway clear, `set_ethereal(false)` takes on the spot.
#[test]
fn turning_it_off_with_the_doorway_clear_takes_immediately() {
    let mut w = flat_world();
    let door = place(
        &mut w,
        1,
        Vec3::new(90.0, 90.0, GROUND),
        door_geometry(),
        true,
    );
    // Ten metres away: registered in the world, in the shadow list of some cell, and nowhere near.
    let body = place(
        &mut w,
        2,
        Vec3::new(100.0, 90.0, GROUND),
        body_geometry(),
        true,
    );
    assert_eq!(w.set_ethereal(door, true, false), EtherealResult::Applied);
    assert!(
        !w.ethereal_check_for_collisions(door),
        "the calibration half: with the doorway clear the overlap test must read CLEAR"
    );
    assert_eq!(w.set_ethereal(door, false, false), EtherealResult::Applied);
    assert!(!is_ethereal(&w, door), "the door is solid again");
    assert!(!latched(&w, door), "and nothing is queued");
    let _ = body;
}

/// **The guard.** `ethereal_check_for_collisions` hits, so the bit goes back and `0x100` is
/// raised. This is the assertion that says a door cannot close on top of a player.
#[test]
fn turning_it_off_with_a_body_inside_is_refused_and_latched() {
    let (mut w, door, _body) = door_with_a_body_inside();
    assert_eq!(w.set_ethereal(door, true, false), EtherealResult::Applied);
    let before = w.ethereal_deferrals();

    assert_eq!(
        w.set_ethereal(door, false, false),
        EtherealResult::Deferred,
        "the client returns 0 here"
    );
    assert!(
        is_ethereal(&w, door),
        "the bit was put BACK, not left clear"
    );
    assert!(
        latched(&w, door),
        "CHECK_ETHEREAL_TS is raised so the frame loop retries"
    );
    assert_eq!(
        w.ethereal_deferrals(),
        before + 1,
        "and the refusal is counted"
    );
}

/// Behaviour: physics.doors.turning-ethereal-off-waits-until-the-doorway-is-clear
/// **The retry**, which is the half that makes the guard a deferral rather than a refusal.
///
///  re-issues `set_ethereal(this, 0, 0)` at the top of **every**
/// sub-step while `0x100` is set. The door therefore becomes solid on its own the moment the body
/// steps out, with nothing else asking it to — which is exactly what the project owner described
/// from retail: *"it might've kept the door ethereal for all players until everyone was out of
/// the way."*
#[test]
fn the_latched_retry_fires_every_substep_and_takes_when_the_body_leaves() {
    let (mut w, door, body) = door_with_a_body_inside();
    if let Some(o) = w.get_mut(door) {
        o.transient_state.set_active_bit(true);
        o.update_time = 0.0;
    }
    assert_eq!(w.set_ethereal(door, true, false), EtherealResult::Applied);
    assert_eq!(w.set_ethereal(door, false, false), EtherealResult::Deferred);

    // Ten frames with the body still in the doorway: the retry runs and keeps refusing.
    //
    // `use_time`'s return is **checked**, not discarded: below `MIN_QUANTUM` it sweeps nothing at
    // all and answers `false`, and a retry test whose sweep never ran would pass its "still
    // ethereal" assertions for entirely the wrong reason. 0.05 s is comfortably above the gate;
    // 1/30 accumulated in `f64` is not, which is how this was found.
    const DT: f64 = 0.05;
    let mut t = 0.0;
    for i in 0..10 {
        t += DT;
        assert!(
            w.use_time(LocalTime(t), false),
            "frame {i}: physics swept nothing"
        );
    }
    let refusals = w.ethereal_deferrals();
    assert!(
        refusals >= 2,
        "only {refusals} refusal(s): the retry is not running at all"
    );
    assert!(
        is_ethereal(&w, door),
        "the door must stay ethereal while the body is in it"
    );
    assert!(latched(&w, door), "and must stay latched");

    // The body steps out. Nothing tells the door anything.
    move_to(&mut w, body, Vec3::new(100.0, 90.0, GROUND));
    t += DT;
    assert!(
        w.use_time(LocalTime(t), false),
        "the sweep that must clear it never ran"
    );

    assert!(
        !is_ethereal(&w, door),
        "the retry never took: the door is ethereal for ever"
    );
    assert!(!latched(&w, door), "and the pending bit was never cleared");
    assert_eq!(
        w.ethereal_deferrals(),
        refusals,
        "the successful retry must not count as another refusal"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The guard's own guards
// ---------------------------------------------------------------------------------------------

/// The guard runs only with no parent and a cell — a **held** object skips the overlap
/// test entirely, so a wielded item's hooks apply unconditionally in both directions.
#[test]
fn a_parented_object_skips_the_overlap_test() {
    let (mut w, door, body) = door_with_a_body_inside();
    assert_eq!(w.set_ethereal(door, true, false), EtherealResult::Applied);
    if let Some(o) = w.get_mut(door) {
        o.parent = Some(body);
    }
    assert_eq!(
        w.set_ethereal(door, false, false),
        EtherealResult::Applied,
        "with a parent the guard is not consulted, however occupied the object is"
    );
    assert!(!is_ethereal(&w, door));
    assert!(!latched(&w, door));
}

/// The other half of the same `if`: an object in **no** cell skips it too.
#[test]
fn an_object_in_no_cell_skips_the_overlap_test() {
    let (mut w, door, _body) = door_with_a_body_inside();
    assert_eq!(w.set_ethereal(door, true, false), EtherealResult::Applied);
    w.leave_cell(door);
    w.remove_shadows_from_cells(door);
    assert_eq!(w.set_ethereal(door, false, false), EtherealResult::Applied);
    assert!(!is_ethereal(&w, door));
}

/// The collision check opens with
/// a return of 0 when state bit `1` is set, and the state is the **candidate standing in the door**'s,
/// not the door. So a baked static sharing the doorway never holds a door open.
///
/// This matters in a dungeon: loading an interior cell's static objects puts hundreds of
/// `STATIC_PS` placements in the very cells doors stand in, and without this clause almost no
/// interior door could ever return to solid.
#[test]
fn a_static_object_in_the_doorway_never_holds_the_door_open() {
    let mut w = flat_world();
    let at = Vec3::new(90.0, 90.0, GROUND);
    let door = place(&mut w, 1, at, door_geometry(), true);
    let squatter = place(&mut w, 2, at, body_geometry(), true);

    // First with it dynamic: the guard fires, which proves the geometry really does overlap.
    assert!(
        w.ethereal_check_for_collisions(door),
        "the two volumes must overlap to begin with"
    );

    if let Some(o) = w.get_mut(squatter) {
        o.state.set_static(true);
    }
    assert!(
        !w.ethereal_check_for_collisions(door),
        "a STATIC_PS candidate must answer `no collision` outright"
    );
    assert_eq!(w.set_ethereal(door, true, false), EtherealResult::Applied);
    assert_eq!(w.set_ethereal(door, false, false), EtherealResult::Applied);
}

/// An object never blocks itself.
#[test]
fn an_object_never_blocks_itself() {
    let mut w = flat_world();
    let door = place(
        &mut w,
        1,
        Vec3::new(90.0, 90.0, GROUND),
        door_geometry(),
        true,
    );
    assert!(
        !w.ethereal_check_for_collisions(door),
        "the door is in its own cells' shadow lists and must be skipped there"
    );
    assert_eq!(w.set_ethereal(door, true, false), EtherealResult::Applied);
    assert_eq!(w.set_ethereal(door, false, false), EtherealResult::Applied);
    assert!(!is_ethereal(&w, door));
}

/// The reason the bit is cleared **before** the overlap test rather than after it.
///
/// `find_obj_collisions_geom`'s second branch answers `OK_TS` for an ethereal candidate. If
/// `set_ethereal` tested first and cleared afterwards, the door would still be ethereal during
/// its own test, every test would read clear, and the guard would be an instrument that cannot
/// fail. Asserted directly: with the bit still set, the overlap test reads clear over exactly the
/// same geometry that reads blocked with it clear.
#[test]
fn the_overlap_test_reads_clear_while_the_object_is_still_ethereal() {
    let (mut w, door, _body) = door_with_a_body_inside();
    assert!(
        w.ethereal_check_for_collisions(door),
        "solid: the body is in the doorway"
    );
    if let Some(o) = w.get_mut(door) {
        o.state.set_ethereal_bit(true);
    }
    assert!(
        !w.ethereal_check_for_collisions(door),
        "an ethereal candidate is invisible to FindObjCollisions -- which is why set_ethereal \
         clears the bit first, and why swapping those two statements silently disables the guard"
    );
}

/// A static ethereal candidate still blocks.
#[test]
fn a_static_ethereal_candidate_still_blocks() {
    let mut w = flat_world();
    let at = Vec3::new(90.0, 90.0, GROUND);
    let door = place(&mut w, 1, at, door_geometry(), false); // STATIC_PS
    let _body = place(&mut w, 2, at, body_geometry(), true);
    assert!(
        w.get(door).expect("live").state.is_static(),
        "the fixture really is static"
    );

    assert!(
        w.ethereal_check_for_collisions(door),
        "solid and static: blocked"
    );
    if let Some(o) = w.get_mut(door) {
        o.state.set_ethereal_bit(true);
    }
    assert!(
        w.ethereal_check_for_collisions(door),
        "a STATIC_PS candidate is NOT exempted by ETHEREAL_PS -- the exemption is in the \
         else-arm of the static test"
    );
    // And the pair that *is* exempted, one branch earlier: ethereal AND ignores-collisions.
    if let Some(o) = w.get_mut(door) {
        o.state.set_ignores_collisions(true);
    }
    assert!(
        !w.ethereal_check_for_collisions(door),
        "ethereal + IGNORE_COLLISIONS_PS is the mutual pass-through branch and comes first"
    );
}

use crate::common::physics_fixture::{flat_world, player_geometry as body_geometry};
