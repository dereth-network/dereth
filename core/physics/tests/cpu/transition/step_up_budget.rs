//! Collision response against synthetic obstacle geometry.
//! Fixture: shared rotated surfaces, a flat block and spherical obstacles.

use crate::common::step_up_fixture::*;

/// The obstacles surface is walkable at first contact so a refusal can only be the budget.
#[test]
fn the_obstacles_surface_is_walkable_at_first_contact_so_a_refusal_can_only_be_the_budget() {
    let sum = OBSTACLE_RADIUS + 0.5;
    let floor_z = dereth_physics::get_walkable_z();
    for top in [CLIMBABLE_TOP, TOO_HIGH_TOP] {
        let nz = (sum - top) / sum;
        assert!(
            nz > floor_z + 0.1,
            "at a {top:.3} m top the obstacle's first contact normal has n.z = {nz:.4}, which is \
             not clear of the {floor_z:.4} walkable floor -- a refusal there would be the slope"
        );
    }
    // And the same statement in the direction that catches a shrunken obstacle: the tallest top
    // this obstacle could ever present as walkable ground.
    let ceiling = sum * (1.0 - floor_z);
    assert!(
        ceiling > TOO_HIGH_TOP + 0.3,
        "the walkable ceiling of a {OBSTACLE_RADIUS:.1} m obstacle is {ceiling:.3} m and must sit \
         well above both stations; at 1.0 m it is 0.504 m, under the budget itself, and every \
         refusal above it is the slope limit rather than the budget"
    );
}

/// Behaviour: physics.step-up.the-step-up-budget-decides-whether-an-obstacle-is-climbable
/// The step up budget is what separates a climbable obstacle from one that is not.
#[test]
fn the_step_up_budget_is_what_separates_a_climbable_obstacle_from_one_that_is_not() {
    let under = walk_at_an_obstacle(CLIMBABLE_TOP, STEP_UP_HEIGHT);
    let over = walk_at_an_obstacle(TOO_HIGH_TOP, STEP_UP_HEIGHT);
    eprintln!(
        "(b): budget {STEP_UP_HEIGHT:.3} m; top {CLIMBABLE_TOP:.3} rose {:.3} m (deepest \
         {:.4}); top {TOO_HIGH_TOP:.3} rose {:.3} m (deepest {:.4})",
        under.rise, under.deepest, over.rise, over.deepest
    );

    assert!(
        (under.rise - CLIMBABLE_TOP).abs() < 0.02,
        "an obstacle {CLIMBABLE_TOP:.3} m high, under the {STEP_UP_HEIGHT:.3} m budget, must be \
         climbed and stood on -- the body rose {:.3} m",
        under.rise
    );
    assert!(
        over.rise < 0.02,
        "an obstacle {TOO_HIGH_TOP:.3} m high is over the {STEP_UP_HEIGHT:.3} m budget and must \
         NOT be climbed -- the body rose {:.3} m",
        over.rise
    );
}

/// **The budget is the binding constraint, shown by moving the budget and leaving the geometry
/// alone.**
///
/// The obstacle refused at 0.600 m is climbed at 0.700 m, and the obstacle climbed at 0.600 m is
/// refused at 0.500 m -- same world, same spheres, same script, one number changed. A build that
/// ignored `step_up_height` would give the same answer in all four cells of this table, which is
/// exactly what could not be said before. It is also the direction `cell_statics` structurally
/// cannot run: a retail static is the height it is.
#[test]
fn raising_and_lowering_the_budget_alone_flips_both_verdicts() {
    let raised = walk_at_an_obstacle(TOO_HIGH_TOP, 0.700);
    let lowered = walk_at_an_obstacle(CLIMBABLE_TOP, 0.500);
    eprintln!(
        "(b): top {TOO_HIGH_TOP:.3} at a 0.700 m budget rose {:.3} m; top {CLIMBABLE_TOP:.3} \
         at a 0.500 m budget rose {:.3} m",
        raised.rise, lowered.rise
    );
    assert!(
        (raised.rise - TOO_HIGH_TOP).abs() < 0.02,
        "0.700 m of budget clears a {TOO_HIGH_TOP:.3} m obstacle; it rose {:.3} m",
        raised.rise
    );
    assert!(
        lowered.rise < 0.02,
        "0.500 m of budget does not clear a {CLIMBABLE_TOP:.3} m obstacle; it rose {:.3} m",
        lowered.rise
    );
}

/// **The budget is exclusive by the epsilon, and the boundary is asserted rather than left to a
/// reader's guess.**'s comparison is `step_up_height < (sum + EPSILON) - delta.z` with
/// `EPSILON = 2e-4`, so an obstacle standing at *exactly* the budget is refused, by two ten
/// thousandths of a metre. A build that wrote `<=`, or that dropped the epsilon, would climb it.
#[test]
fn an_obstacle_standing_at_exactly_the_budget_is_refused_by_the_epsilon() {
    let exact = walk_at_an_obstacle(STEP_UP_HEIGHT, STEP_UP_HEIGHT);
    let hair_under = walk_at_an_obstacle(STEP_UP_HEIGHT - 0.001, STEP_UP_HEIGHT);
    eprintln!(
        "(b): top == budget == {STEP_UP_HEIGHT:.3} rose {:.3} m; one millimetre under rose \
         {:.3} m",
        exact.rise, hair_under.rise
    );
    assert!(
        exact.rise < 0.02,
        "exactly at the budget is over it: rose {:.3} m",
        exact.rise
    );
    assert!(
        (hair_under.rise - (STEP_UP_HEIGHT - 0.001)).abs() < 0.02,
        "one millimetre under the budget is under it: rose {:.3} m",
        hair_under.rise
    );
}

/// The ground the budget pair walks on is flat and at the height the obstacles are placed from.
#[test]
fn the_ground_the_budget_pair_walks_on_is_flat_and_at_the_height_the_obstacles_are_placed_from() {
    let w = flat_world();
    let block = w
        .land()
        .landblock(LandblockId::new(0xA9, 0xB4))
        .expect("resident");
    assert!(
        !block.polygons.is_empty(),
        "an empty block would make every assertion below vacuous"
    );
    for p in &block.polygons {
        let n = p.plane.normal;
        assert!(
            (n.z - 1.0).abs() < 1e-4,
            "a flat block's every polygon faces straight up: {n:?}"
        );
        for v in &p.vertices {
            assert!(
                (v.z - GROUND).abs() < 1e-3,
                "the floor must be at z = {GROUND}, which is what CLIMBABLE_TOP and TOO_HIGH_TOP \
                 are measured from: {v:?}"
            );
        }
    }
}

/// A body refused a lift is stopped at the obstacles face.
#[test]
fn a_body_refused_a_lift_is_stopped_at_the_obstacles_face() {
    let sum = OBSTACLE_RADIUS + 0.5;

    // The control, and the premise that it is one: the obstacle must be out of play.
    let free = walk_past_an_obstacle(TOO_HIGH_TOP, STEP_UP_HEIGHT, OUT_OF_REACH);
    assert!(
        free.gap > OUT_OF_REACH - sum - 1.0,
        "the control's obstacle must be unreachable, or it is not a control: gap {:.3}",
        free.gap
    );
    let unimpeded = free.end.x - 100.0;
    assert!(
        unimpeded > 7.0,
        "an unimpeded walk covers {unimpeded:.3} m in {WALK_SECONDS} seconds"
    );

    assert!(
        free.last_second > 0.9,
        "an unimpeded body is still walking at the end -- {:.4} m in its last second -- and that is \
         what the stopped bodies below are measured against",
        free.last_second
    );

    for top in [STEP_UP_HEIGHT, TOO_HIGH_TOP] {
        let w = walk_past_an_obstacle(top, STEP_UP_HEIGHT, OFF_AXIS);
        let dz = 0.5 - top + OBSTACLE_RADIUS;
        let dx = (sum * sum - dz * dz).sqrt();
        let face_x = 105.0 - dx;
        // The premise `OFF_AXIS` exists for: below this the crease cancels to under EPSILON and
        // the sub-step is abandoned instead of slid. See [`OFF_AXIS`].
        assert!(
            OFF_AXIS > dereth_physics::globals::EPSILON * dx / SCRIPT_STEP,
            "OFF_AXIS must clear {:.4} m at a {top:.3} m top, or this station is measuring the \
             degeneracy rather than the stop",
            dereth_physics::globals::EPSILON * dx / SCRIPT_STEP
        );
        eprintln!(
            "top {top:.3} at {OFF_AXIS} m off the axis: rose {:.3} m, deepest {:.4}, rest \
             gap {:.4}, ended x {:.3} (face at {face_x:.3}), moved {:.4} m in the last second",
            w.rise, w.deepest, w.gap, w.end.x, w.last_second
        );

        assert!(
            w.rise < 0.02,
            "a {top:.3} m top is at or over the {STEP_UP_HEIGHT:.3} m budget and must not be \
             climbed; it rose {:.3} m",
            w.rise
        );
        assert!(
            w.deepest >= 0.0,
            "the refused body must never be inside the obstacle on ANY frame of the walk; its \
             deepest was {:.4} m",
            w.deepest
        );
        assert!(
            w.last_second * 20.0 < free.last_second,
            "the refused body must come to rest; it moved {:.4} m over its last second against \
             the control's {:.4} m",
            w.last_second,
            free.last_second
        );
        assert!(
            w.gap < 0.10,
            "and it must rest ON the obstacle's face rather than short of it; the gap is {:.4} m",
            w.gap
        );
        assert!(
            (w.end.x - face_x).abs() < 0.10,
            "it must stop where the two surfaces meet, x = {face_x:.3}; it stopped at {:.3}",
            w.end.x
        );
        assert!(
            w.end.x - 100.0 < unimpeded - 4.0,
            "and that is well short of the {unimpeded:.3} m an unimpeded walk covers; it managed \
             {:.3} m",
            w.end.x - 100.0
        );
    }

    // The legal case, at the same offset, unchanged.
    for top in [CLIMBABLE_TOP, STEP_UP_HEIGHT - 0.001] {
        let w = walk_past_an_obstacle(top, STEP_UP_HEIGHT, OFF_AXIS);
        eprintln!(
            "top {top:.3} at {OFF_AXIS} m off the axis: rose {:.3} m, moved {:.4} m in the \
             last second",
            w.rise, w.last_second
        );
        assert!(
            (w.rise - top).abs() < 0.02,
            "a {top:.3} m top is under the budget and must still be climbed off the axis; it rose \
             {:.3} m",
            w.rise
        );
        assert!(
            w.last_second > 0.5,
            "and having climbed it the body walks on, so a change that stops everything fails \
             here: it moved {:.4} m in its last second",
            w.last_second
        );
    }
}

/// The exactly head on approach is a degeneracy and the budget is not what decides it.
#[test]
fn the_exactly_head_on_approach_is_a_degeneracy_and_the_budget_is_not_what_decides_it() {
    let sum = OBSTACLE_RADIUS + 0.5;
    let floor_z = dereth_physics::get_walkable_z();

    // The second station is refused by the SLOPE, so `step_up_height` cannot be what decides it.
    // (Its other half -- that it stands far above any budget -- is [`UNCLIMBABLE_TOP`]'s own
    // compile-time assertion.)
    assert!(
        (sum - UNCLIMBABLE_TOP) / sum < floor_z,
        "the second station must be refused by the slope: n.z = {:.4} against a {floor_z:.4} floor",
        (sum - UNCLIMBABLE_TOP) / sum
    );

    for top in [TOO_HIGH_TOP, UNCLIMBABLE_TOP] {
        let head_on = walk_at_an_obstacle(top, STEP_UP_HEIGHT);
        let off_axis = walk_past_an_obstacle(top, STEP_UP_HEIGHT, OFF_AXIS);
        eprintln!(
            "top {top:.3}: head-on deepest {:.4} (this read -{top:.3} before the \
             failed-transition arm was corrected), {OFF_AXIS} m off the axis deepest {:.4}",
            head_on.deepest, off_axis.deepest
        );
        let dz = 0.5 - top + OBSTACLE_RADIUS;
        let dx = (sum * sum - dz * dz).sqrt();
        let floor_of_the_offset = dereth_physics::globals::EPSILON * dx / SCRIPT_STEP;
        let one_cm = walk_past_an_obstacle(top, STEP_UP_HEIGHT, 0.01);
        eprintln!(
            "top {top:.3}: the offset floor is {floor_of_the_offset:.4} m; at one \
             centimetre the body reached {:.4} m and moved {:.4} m over its last second",
            one_cm.deepest, one_cm.last_second
        );
        assert!(
            one_cm.deepest >= 0.0,
            "one centimetre off the axis the body must stay outside the obstacle: {:.4} m",
            one_cm.deepest
        );
        // Head-on the transition is refused on its first sub-step, and a refused transition moves
        // nothing: the body stops at the obstacle's face rather than passing through it.
        assert!(
            head_on.deepest >= 0.0,
            "head-on the body must be stopped by the obstacle, not carried {top:.3} m inside it; \
             it reached {:.4} m",
            head_on.deepest
        );
        assert!(
            head_on.last_second < 0.01,
            "and it must be held there rather than creeping: it moved {:.4} m over its last second",
            head_on.last_second
        );
        assert!(
            off_axis.deepest >= 0.0,
            "and one centimetre off that line the same obstacle stops it outright: {:.4} m",
            off_axis.deepest
        );
    }
}

/// **The off-axis station is a station and not a lucky value.** `OFF_AXIS` has to be small enough
/// that the body runs into the obstacle rather than past it and large enough that the crease no
/// longer cancels. Both edges are asserted, requiring a changed answer when the fixture is
/// perturbed in either direction.
#[test]
fn the_off_axis_offset_is_bracketed_on_both_sides() {
    let head_on = walk_at_an_obstacle(TOO_HIGH_TOP, STEP_UP_HEIGHT);
    assert!(
        head_on.deepest >= 0.0 && head_on.last_second < 0.01,
        "exact alignment holds the body against the obstacle: deepest {:.4} m, {:.4} m over its \
         last second",
        head_on.deepest,
        head_on.last_second
    );
    let free = walk_past_an_obstacle(TOO_HIGH_TOP, STEP_UP_HEIGHT, OUT_OF_REACH);
    for lateral in [OFF_AXIS, 0.05, 0.10] {
        let w = walk_past_an_obstacle(TOO_HIGH_TOP, STEP_UP_HEIGHT, lateral);
        assert!(
            w.deepest >= 0.0 && w.last_second * 8.0 < free.last_second,
            "at {lateral} m off the axis the body must stop outside the obstacle: deepest {:.4}, \
             last second {:.4} against the control's {:.4}",
            w.deepest,
            w.last_second,
            free.last_second
        );
    }
    // Above: by a quarter of a metre the body slides *around* the obstacle and keeps going, so a
    // much larger offset would be measuring a graze rather than a stop.
    let grazing = walk_past_an_obstacle(TOO_HIGH_TOP, STEP_UP_HEIGHT, 0.25);
    eprintln!(
        "at 0.25 m off the axis the body grazes past: ended x {:.3}, moved {:.4} m in the \
         last second",
        grazing.end.x, grazing.last_second
    );
    assert!(
        grazing.last_second > 0.1,
        "a quarter metre off the axis the body is still moving at the end -- it slid around the \
         obstacle -- so OFF_AXIS must stay well under it: {:.4} m",
        grazing.last_second
    );
}
