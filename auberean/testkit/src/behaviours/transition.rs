//! Transition: how the shared swept-sphere move works in this world, where it differs from the
//! other world the shared code serves.
//!
//! A row claims what the September 2013 client does when it moves or places a body.

use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "transition.cells.a-body-in-a-doorway-out-reaches-the-20-m-land-cells-under-it",
        says: "A body inside a room that stands in a doorway out reaches the land outside: every \
               20 m land cell under the box around its spheres, each with its ground when its \
               block is in.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TRANSITION-DOORWAY-OUT"),
        station: "auberean-physics::cpu::motion::rules::a_body_in_a_doorway_out_reaches_the_20_m_land_cells_under_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "transition.cells.a-move-that-leaves-every-cell-falls-into-the-land-cell-under-it",
        says: "A move that has left every cell it knew falls out into the 20 m land cell its point \
               lies in, the cells of a block numbered across then up from 1; off the map it falls \
               nowhere.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TRANSITION-FALL-OUT"),
        station: "auberean-physics::cpu::motion::rules::a_move_that_leaves_every_cell_falls_into_the_land_cell_under_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "transition.contact.ground-on-the-land-is-the-landscape",
        says: "Ground found under a body on the land is the land's own: the contact plane is marked \
               as the landscape and not as an interior's floor; a floor in an interior cell's own \
               walls is marked as that, and a plane on an object as neither.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TRANSITION-LANDSCAPE"),
        station: "auberean-physics::cpu::motion::rules::ground_on_the_land_is_the_landscape",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "transition.land.the-ground-under-a-point-is-the-first-triangle-of-its-cell-over-it",
        says: "The ground under a point on the land is the first triangle of its 20 m land cell \
               the point lies over, within a hair of each edge, the cell's squares taken west to \
               east and south to north: a point on the line between two squares stands on the \
               first square's ground.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LAND-GROUND-ORDER"),
        station: "auberean-physics::cpu::motion::rules::the_ground_under_a_point_is_the_first_triangle_of_its_cell_over_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "transition.obstacle.a-body-slides-as-the-client-slides-it",
        says: "A body slides along a surface as the client slides it, to the bit, wherever the \
               transition slides it: along a surface's normal (taken as parallel to the plane it \
               stands on only within 0.0002 squared, the normal it collides with turned back \
               along its move), along the step-up normal when it cannot step up, and along a \
               physics tree's face, the face's normal turned into the world as the client turns it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OBSTACLE-SLIDE"),
        station: "auberean-physics::cpu::contact::obstacles::a_body_slides_as_the_client_slides_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "transition.obstacle.a-cylinder-is-met-as-the-client-meets-it",
        says: "A body meeting another object's upright cylinder answers, and leaves its transition \
               (where it is, the walk interpolation, the landing, the contact plane, the collision \
               normal), as the client's own code does, to the bit, in every arm: placed or passing \
               through, stepping down onto its top, checking where it may walk, settling onto its \
               top after a landing, stepping up onto it or sliding along it on the ground, meeting \
               it at a face or at its side when it clips its path, and landing on it in the air.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OBSTACLE-CYLINDER"),
        station: "auberean-physics::cpu::contact::obstacles::a_cylinder_is_met_as_the_client_meets_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "transition.obstacle.a-shape-is-placed-at-its-objects-position-and-scale",
        says: "A sphere or cylinder given in its object's frame is placed at the object's position \
               and its scale along each axis (a sphere's radius by the scale's height, a cylinder's \
               height by it and its radius by the mean of the two widths) before it is met, as the \
               client places it, to the bit.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OBSTACLE-PLACED"),
        station: "auberean-physics::cpu::contact::obstacles::a_shape_is_placed_at_its_objects_position_and_scale",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "transition.obstacle.a-sphere-is-met-as-the-client-meets-it",
        says: "A body meeting another object's sphere answers, and leaves its transition, as the \
               client's own code does, to the bit, in every arm: placed or passing through, stepping \
               down onto it (never onto a creature or missile), checking where it may walk, settling \
               onto it after a landing, stepping up onto it or sliding along it on the ground (a \
               slide against a parallel contact plane turning the step-up normal back along the \
               move), meeting it as a point when it clips its path, and landing on it in the air.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OBSTACLE-SPHERE"),
        station: "auberean-physics::cpu::contact::obstacles::a_sphere_is_met_as_the_client_meets_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "transition.place.a-body-put-beside-the-fir-stands-where-the-client-stands-it",
        says: "A body the server puts inside the start hall's fir stands where there is room nearest that place, where the client stands it: refused there and 0.1 m above it, it scatters in rings counted by nine tenths of its radius (4/9 m apart for the player's 0.5 m sphere), and stands 0.444 m aside, lifted onto the floor.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TRANSITION-PLACEMENT-SCATTER"),
        station: "auberean-physics::dat::placing::a_body_put_beside_the_fir_stands_where_the_client_stands_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "transition.rules.placements-retry-and-moves-turn-plainly",
        says: "A placement asks the cell it is in and then the others again after any adjustment, up \
               to three times; a move turns its sub-steps by a plain slerp, blending straight within \
               0.001 of no turn, and keeps a rotation of no length as it is.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TRANSITION-RULES"),
        station: "auberean-physics::cpu::motion::rules::placements_retry_the_other_cells_and_moves_turn_plainly",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "transition.rules.the-polygon-tests-take-only-an-exact-parallel-as-parallel",
        says: "A movement counts as lying along a polygon's surface only when it does exactly: a \
               body sunk a hair into a floor and stepping down into it by less than a fifth of a \
               millimeter is still lifted onto it, and the ground under a body is never refused \
               for lying along its up direction.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PHYS-PARALLEL"),
        station: "auberean-physics::cpu::motion::rules::the_polygon_tests_take_only_an_exact_parallel_as_parallel",
        tier: Tier::Cpu,
    },
];
