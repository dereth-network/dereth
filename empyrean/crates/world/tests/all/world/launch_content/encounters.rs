//! Divergence: V577
//! On the Launch world, an encounter row fires only where its vertex's slot survives in the 1999
//! scenes, once per slot. Fixture: the hash-pinned launch dats and pack of
//! `EMPYREAN_TEST_LAUNCH_FIXTURE`.

use crate::world::encounter_slots::{tally, Tally};

use super::world;

#[test]
fn on_the_launch_world_a_row_fires_only_where_a_slot_survives() {
    // The rows whose slot is the neighbor's are the second copy of a shared edge point, whose
    // neighbor's own row fires there.
    assert_eq!(
        tally(&world()),
        Tally {
            rows: 180_132,
            live_in_own_block: 150_102,
            live_only_in_neighbor: 29_677,
            no_live_slot: 353,
            fired: 150_102,
            fired_from_neighbor: 0,
        }
    );
}
