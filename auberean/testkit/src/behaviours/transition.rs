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
        station: "auberean-physics::cpu::rules::a_body_in_a_doorway_out_reaches_the_20_m_land_cells_under_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "transition.cells.a-move-that-leaves-every-cell-falls-into-the-land-cell-under-it",
        says: "A move that has left every cell it knew falls out into the 20 m land cell its point \
               lies in, the cells of a block numbered across then up from 1; off the map it falls \
               nowhere.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TRANSITION-FALL-OUT"),
        station: "auberean-physics::cpu::rules::a_move_that_leaves_every_cell_falls_into_the_land_cell_under_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "transition.contact.ground-on-the-land-is-the-landscape",
        says: "Ground found under a body on the land is the land's own: the contact plane is marked \
               as the landscape and not as an interior's floor; a floor in an interior cell's own \
               walls is marked as that, and a plane on an object as neither.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TRANSITION-LANDSCAPE"),
        station: "auberean-physics::cpu::rules::ground_on_the_land_is_the_landscape",
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
        station: "auberean-physics::cpu::rules::placements_retry_the_other_cells_and_moves_turn_plainly",
        tier: Tier::Cpu,
    },
];
