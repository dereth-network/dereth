//! Every physics global, with its value and behavioral use.
//!
//! Taken from the client's own physics-globals and landscape-definitions code.
//!
//! Nothing here is recomputed at run time. Two of these values are hardware-cosine results in the
//! original (`floor_z` and the sledding slope threshold) and asking a modern libm to reproduce the
//! FPU's large-argument range reduction is not a promise anyone can make. The values are baked
//! and the tests below pin them by bit pattern.

/// The physics globals' gravity, static data.
pub const GRAVITY: f32 = dereth_primitives::num::consts::GRAVITY;

/// The physics globals' floor Z = `cos(3437.746770784939)` = 48.381 degrees.
///
/// `is_valid_walkable(n)` is exactly `n.z >= FLOOR_Z` with no tolerance
/// so one ulp is the difference between standing
/// and sliding.
pub const FLOOR_Z: f32 = dereth_primitives::num::consts::FLOOR_Z;

/// The universal comparison epsilon, `2e-4`, used by equality and every plane and sphere test.
pub const EPSILON: f32 = dereth_primitives::num::consts::EPSILON;

/// `EPSILON * EPSILON`, the rejection threshold for a degenerate offset in the transitional
/// and placement position searches.
///
/// Written as the product rather than as the decimal so that it is the same float the original
/// computes, `3.9999996e-08`.
pub const EPSILON_SQ: f32 = EPSILON * EPSILON;

/// `small_velocity`: the contact-plane cut-off.
pub const SMALL_VELOCITY: f32 = 0.25;
/// `small_velocity` squared. Physics updates zero velocities below it.
pub const SMALL_VELOCITY_SQ: f32 = 0.0625;
/// `max_velocity`.
pub const MAX_VELOCITY: f32 = 50.0;
/// `max_velocity` squared.
pub const MAX_VELOCITY_SQ: f32 = 2500.0;

/// `LandingZ` = `sin(5 degrees)`. The default: anything with
/// `N.z <= LANDING_Z` is a wall as far as step-up is concerned.
pub const LANDING_Z: f32 = 0.0871557;

/// The default step height used by ordinary transition insertion and by its step-down
/// probe when the object is not already on a walkable surface.
pub const DEFAULT_STEP_HEIGHT: f32 = 0.04;

/// Sledding: below 1.25 m/s (squared) friction becomes `1.0` and the sled stops.
pub const SLED_SLOW_SQ: f32 = 1.5625;
/// Sledding: at or above 2.5 m/s (squared) the low-friction branch becomes eligible.
pub const SLED_FAST_SQ: f32 = 6.25;
/// `cos(pi/18)` = `cos(10 degrees)`. The client takes the low-friction sledding branch when the
/// contact plane normal's Z is **below** this, i.e. on slopes steeper than 10 degrees.
///
/// ACE tests `> 0.99999536f`; both the direction and the constant are wrong there. See
/// `docs/CORRECTIONS.md`.
pub const SLED_SLOPE_COS: f32 = 0.984_807_7;
/// The sledding low-friction value.
pub const SLED_LOW_FRICTION: f32 = 0.2;

/// `MIN_QUANTUM`. A `double` in the original.
pub const MIN_QUANTUM: f64 = 1.0 / 30.0;

/// How far short of [`MIN_QUANTUM`] the time since the last physics tick may fall and still open
/// the next one. This client's own rule (CD-042); the final client waited for the whole quantum.
///
/// A tick stamps the time it was taken and discards the rest, so on a display refreshing at a
/// multiple of 30 Hz two frames (or four, or eight) add up to the quantum itself, and whether
/// they reach it is decided by the clock's last bits and the frame's own jitter: the world steps
/// after two frames or after three, irregularly, at 20 to 30 Hz. Two milliseconds absorbs that,
/// so such a display steps the world on a steady cadence, every second frame at 60 Hz and every
/// fourth at 120. A frame rate whose frames do not add up to the quantum keeps the cadence it had,
/// and no tick is ever shorter than this much under it.
///
/// A tick covers the time that has passed since the one before, so bodies move as far per second
/// as they did; only when the steps fall changes.
pub const TICK_TOLERANCE: f64 = 0.002;

/// Whether `elapsed` seconds since the last physics tick open the next one: the gate every tick of
/// the world, and of the animated statics stepped with it, waits on.
#[must_use]
pub fn tick_is_due(elapsed: f64) -> bool {
    elapsed >= MIN_QUANTUM - TICK_TOLERANCE
}
/// `MAX_QUANTUM`. ACE uses `0.1` and its own comment calls that buggy.
pub const MAX_QUANTUM: f64 = 0.2;
/// The "huge quantum" literal in the physics update; anything above it is discarded.
pub const HUGE_QUANTUM: f64 = 2.0;
/// The minimum step literal in `update_object`; at or below it the frame is skipped.
pub const MIN_STEP: f64 = 0.0002;

/// `DEFAULT_VIEW_RADIUS`. Renderer/LOD only; collision never reads it.
pub const DEFAULT_VIEW_RADIUS: f32 = 0.100_600_004;

/// One landblock, in metres.
pub const BLOCK_LENGTH: f32 = 192.0;
/// Half a land cell.
pub const HALF_SQUARE_LENGTH: f32 = 12.0;
/// The sort/visibility sentinel.
pub const OUTSIDE_VAL: f32 = 1001.0;
/// One land cell, in metres.
pub const CELL_SIZE: f32 = 24.0;

/// The landscape definitions' `NumBlockLength` / `NumBlockWidth`.
pub const NUM_BLOCK_LENGTH: i32 = 0xFF;
/// Landscape block side length.
pub const BLOCK_SIDE: i32 = 8;
/// Landscape vertices per cell.
pub const VERTEX_PER_CELL: i32 = 1;
/// Maximum object height, used for the land block's maximum Z value.
pub const MAX_OBJECT_HEIGHT: f32 = 200.0;
/// Landscape sky height.
pub const SKY_HEIGHT: f32 = 1000.0;
/// Road half-width. The mirrored threshold also uses `24 - 5 = 19`.
pub const ROAD_WIDTH: f32 = 5.0;
/// The mirrored road threshold, `CELL_SIZE - ROAD_WIDTH`.
pub const ROAD_WIDTH_FAR: f32 = CELL_SIZE - ROAD_WIDTH;

/// Length of the land-height table.
pub const LAND_HEIGHT_TABLE_LEN: usize = 256;
/// The loader rejects a table with any entry outside `[0, 800]`.
pub const LAND_HEIGHT_MAX: f32 = 800.0;

/// Global land-cell coordinates run `0 .. 0x7F8` (2040 = 255 blocks times 8 cells).
pub const LCOORD_LIMIT: i32 = 0x7F8;

/// The activity radius: deactivates objects further than this from the
/// player, and interpolates only within it.
pub const ACTIVE_RADIUS: f32 = 96.0;

/// The autonomy blip distance outdoors.
pub const AUTONOMY_BLIP_OUTDOORS: f32 = 100.0;
/// As above, indoors.
pub const AUTONOMY_BLIP_INDOORS: f32 = 20.0;
/// As above, indoors, for the player.
pub const AUTONOMY_BLIP_INDOORS_PLAYER: f32 = 25.0;

/// The start-constraint distance outdoors.
pub const CONSTRAINT_START_OUTDOORS: f32 = 10.0;
/// As above, indoors.
pub const CONSTRAINT_START_INDOORS: f32 = 5.0;
/// The maximum constraint distance outdoors.
pub const CONSTRAINT_MAX_OUTDOORS: f32 = 50.0;
/// As above, indoors.
pub const CONSTRAINT_MAX_INDOORS: f32 = 20.0;

/// Detection rebuilds the global cell list only after this
/// much movement.
pub const DETECTION_RECELL_DISTANCE: f32 = 5.0;
/// The detection tick, in simulated seconds.
pub const DETECTION_TICK: f64 = 1.0;
/// The targeting tick, in simulated seconds.
pub const TARGET_TICK: f64 = 0.5;
/// Target timeout, measured against the **wall** clock, not the physics clock.
pub const TARGET_TIMEOUT: f64 = 10.0;
/// The lifetime of a collision record.
pub const COLLISION_RECORD_LIFETIME: f64 = 1.0;

/// The physics-object constructor's default `state`:
/// `EDGE_SLIDE | LIGHTING_ON | GRAVITY | REPORT_COLLISIONS`.
pub const DEFAULT_STATE: u32 = 0x0040_0C08;
/// The constructor's default `friction`.
pub const DEFAULT_FRICTION: f32 = 0.95;
/// The constructor's default `elasticity`. Clamped to `[0, 0.1]` by the elasticity setter.
pub const DEFAULT_ELASTICITY: f32 = 0.05;
/// The upper clamp applied by `set_elasticity`.
pub const MAX_ELASTICITY: f32 = 0.1;
/// The constructor's default `massinv`. Stored, never read by the client's own solver.
pub const DEFAULT_MASSINV: f32 = 1.0;

/// The transition pool hands out one of exactly ten pooled transitions and
/// returns NULL at depth ten.
pub const TRANSITION_POOL_SIZE: usize = 10;

/// The dummy sphere. Used for any object with no part array or no spheres.
pub const DUMMY_SPHERE_CENTER_Z: f32 = 0.1;
/// Radius of the same.
pub const DUMMY_SPHERE_RADIUS: f32 = 0.1;

/// The `walk_interp` floor used by the step-down paths (sphere, cylsphere, BSP and terrain).
pub const WALK_INTERP_FLOOR_STEP: f32 = -0.1;
/// The `walk_interp` floor used by plane adjustment.
pub const WALK_INTERP_FLOOR_PLANE: f32 = -0.5;

/// `transitional_insert`'s attempt budget from `find_transitional_position`.
pub const ATTEMPTS_NORMAL: u32 = 3;
/// `transitional_insert`'s attempt budget from inside `step_down`.
pub const ATTEMPTS_STEP_DOWN: u32 = 5;
/// `transitional_insert`'s attempt budget for the placement re-validation.
pub const ATTEMPTS_REVALIDATE: u32 = 1;

/// Placement insertion gives up after this many push-out iterations.
pub const PLACEMENT_INSERT_ITERATIONS: u32 = 20;

/// The four fixed water depths, from the cell's water calculation and the block's water-depth
/// step.
pub const WATER_DEPTH_ENTIRELY: f32 = 0.9;
/// Depth on a water vertex of a partially-water cell.
pub const WATER_DEPTH_PARTIAL_WATER_VERTEX: f32 = 0.45;
/// Depth on a solid vertex of a partially-water cell, and the fallback when the landblock is
/// not resident.
pub const WATER_DEPTH_PARTIAL_SOLID_VERTEX: f32 = 0.1;
/// Depth on dry land.
pub const WATER_DEPTH_NONE: f32 = 0.0;

/// The viewer sphere, set during camera initialization: centre `(0, 0, 0)`,
/// radius **0.3**. The camera's wall behaviour is entirely this sphere swept through the same
/// BSPs the player walks against, not a raycast.
pub const VIEWER_SPHERE_RADIUS: f32 = 0.3;

/// The transition object-state mask used for the camera viewer: `0x5C` for the player.
///
/// `0x5C` decomposes into `IS_VIEWER (0x04) | PATH_CLIPPED (0x08) | FREE_ROTATE (0x10) |
/// PERFECT_CLIP (0x40)`. It is the **only located writer of `PERFECT_CLIP`**, a bit that
/// otherwise looks read-but-never-written; the bit enters the transition from here and from
/// nowhere else.
pub const VIEWER_OBJECT_INFO_STATE: u32 = 0x5C;

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the recovered physics update behavior section
    // "Global constants", which tabulates each value with the address that sets it.

    #[test]
    fn floor_z_is_the_baked_startup_cosine() {
        // The tabulated double is 0.66417414618662751; as a float that is 0x3F2A0F1D.
        assert_eq!(FLOOR_Z.to_bits(), 0x3F2A_0751, "{FLOOR_Z:?}");
        #[allow(clippy::cast_possible_truncation)]
        let as_float_of_the_double = 0.664_174_146_186_627_5_f64 as f32;
        assert_eq!(FLOOR_Z.to_bits(), as_float_of_the_double.to_bits());
    }

    #[test]
    fn epsilon_family_matches_the_tabulated_decimals() {
        assert_eq!(EPSILON.to_bits(), 0.0002_f32.to_bits());
        // The spec quotes EpsilonSq as 3.9999996e-08; assert the product really is that float.
        assert_eq!(
            EPSILON_SQ.to_bits(),
            3.999_999_6e-08_f32.to_bits(),
            "{EPSILON_SQ:e}"
        );
    }

    #[test]
    fn velocity_limits_are_squares_of_each_other() {
        assert_eq!(SMALL_VELOCITY * SMALL_VELOCITY, SMALL_VELOCITY_SQ);
        assert_eq!(MAX_VELOCITY * MAX_VELOCITY, MAX_VELOCITY_SQ);
        assert_eq!(SLED_SLOW_SQ, 1.25 * 1.25);
        assert_eq!(SLED_FAST_SQ, 2.5 * 2.5);
    }

    #[test]
    fn landing_z_is_sin_five_degrees_and_sled_cos_is_cos_ten_degrees() {
        // Checked to float tolerance only: the point of baking them is that the *stored* value is
        // what ships, and this test just proves neither is a typo.
        let sin5 = dereth_primitives::num::math::sin(5.0_f64.to_radians());
        assert!((f64::from(LANDING_Z) - sin5).abs() < 1e-7, "{LANDING_Z}");
        let cos10 = dereth_primitives::num::math::cos(0.174_532_925_199_432_95_f64);
        assert!(
            (f64::from(SLED_SLOPE_COS) - cos10).abs() < 1e-7,
            "{SLED_SLOPE_COS}"
        );
    }

    #[test]
    fn quanta_are_the_client_values_not_aces() {
        assert_eq!(MIN_QUANTUM, 1.0 / 30.0);
        assert_eq!(
            MAX_QUANTUM, 0.2,
            "ACE uses 0.1 here; see docs/CORRECTIONS.md"
        );
        assert_eq!(HUGE_QUANTUM, 2.0);
    }

    #[test]
    fn default_state_decomposes_into_the_four_documented_bits() {
        // EDGE_SLIDE 0x400000 | LIGHTING_ON 0x800 | GRAVITY 0x400 | REPORT_COLLISIONS 0x8
        assert_eq!(
            DEFAULT_STATE,
            0x0040_0000 | 0x0000_0800 | 0x0000_0400 | 0x0000_0008
        );
    }

    #[test]
    fn landscape_constants_are_consistent() {
        assert_eq!(BLOCK_LENGTH, CELL_SIZE * 8.0);
        assert_eq!(HALF_SQUARE_LENGTH, CELL_SIZE * 0.5);
        assert_eq!(ROAD_WIDTH_FAR, 19.0);
        assert_eq!(LCOORD_LIMIT, 255 * 8);
    }
}
