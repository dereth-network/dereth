//! This world's settings for the shared swept-sphere core.
//!
//! The core is one algorithm; a world chooses where ground stops being walkable, how long a
//! sub-step may be, how long its landblocks are, where placement starts scattering, and a few
//! small behaviors. These are this world's.

use dereth_physics::transition::{Slide, TransitionRules, Turning};

/// Where ground stops being walkable: the cosine of 45.5 degrees, taken once at startup and kept
/// as a 32-bit float.
pub const WALKABLE_Z: f32 = f32::from_bits(0x3F33_6ECA);

/// The side of a landblock, in meters: eight 20 m land cells.
pub const BLOCK_LENGTH: f32 = 160.0;

/// The longest sub-step of a move, as a fraction of the mover's sphere radius.
pub const STEP_FRACTION: f32 = 0.9;

/// The smallest radius placement scatters its candidate points by.
pub const PLACEMENT_RADIUS_FLOOR: f32 = 0.5;
/// The fraction of the radius the placement scatter counts its rings by.
pub const PLACEMENT_RING_FRACTION: f32 = 0.9;

/// How near to lying in a polygon's plane the polygon tests take a movement to be parallel to
/// it: only exactly, and an up vector is not tested.
// D2-04171, D2-04172, D2-04179, D2-04180, D2-04181
pub const PARALLEL_BAND: f32 = 0.0;

/// This world's transition settings.
// D2-05778, D2-05789, D2-05800, D2-05802, D2-05803, D2-05824, D2-05797, D2-05804
#[must_use]
pub const fn rules() -> TransitionRules {
    TransitionRules {
        walkable_z: WALKABLE_Z,
        step_fraction: STEP_FRACTION,
        block_length: BLOCK_LENGTH,
        placement_radius_floor: PLACEMENT_RADIUS_FLOOR,
        // A move brought to rest on the ground it last stood on keeps its velocity.
        stop_on_restored_contact: false,
        // Each attempt asks the cell once.
        retry_each_cell: false,
        // A landing is re-checked as a placement with one attempt.
        recheck_landing_with_budget: false,
        // An edge slide's probe goes back from the checked position to the current one across
        // landblocks.
        edge_slide_offset_across_blocks: true,
        // A step down with no drop left lowers the sphere rather than refusing.
        step_down_guards_zero_drop: true,
        // A placement asks its cell and then the others again after any adjustment.
        placement_retries_other_cells: true,
        // A move's sub-steps turn by a plain slerp.
        turning: Turning::Plain,
        // The placement scatter's rings are counted by nine tenths of the radius.
        placement_ring_fraction: PLACEMENT_RING_FRACTION,
        // A placement that may slide is tried 0.1 m up before the scatter.
        placement_lifts_before_scatter: true,
        // The polygon tests take only an exactly parallel movement as parallel, and do not test
        // the up vector at all.
        parallel_band: PARALLEL_BAND,
        // A mover in the air that an interior cell's tree stops or moves collided straight up.
        env_collision_normal_up: true,
        // The mover slides by this world's slide, and a physics tree's normal is turned into the
        // world, for its step up and its slide, in double precision.
        slide: Some(SLIDE),
    }
}

/// This world's slide along what the mover meets: its sphere's slide, worked in double precision
/// with the parallel test at 0.0002 squared, the normal it collides with written back; a physics
/// tree's normal turned into the world as the client turns it.
// D2-04118, D2-04119
pub const SLIDE: Slide = Slide {
    along: crate::obstacle::sphere::slide_turning,
    turn: crate::geom::local_to_global,
};
