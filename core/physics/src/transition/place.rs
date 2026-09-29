//! Placement: `find_placement_position` and the placement scatter search.
//!
//! The scatter spirals over concentric rings out to 4 m (2 m for a very small object) with the
//! number of samples per ring growing with the ring index. Note `n = 4.0 / r` uses **4.0 even when
//! `max_dist` is 2.0**; that asymmetry is the client's and it changes how densely a small object
//! samples.

use dereth_primitives::Frame;

use crate::globals::{ATTEMPTS_NORMAL, EPSILON_SQ, LANDING_Z};
use crate::math::{self, V3};
use crate::obj::PhysicsState;
use crate::transition::insert::{
    adjust_offset, insert_into_cell, transitional_insert, validate_placement,
    validate_placement_transition,
};
use crate::transition::spherepath::InsertType;
use crate::transition::{Transition, TransitionCtx, TransitionState};

/// The placement-position search.
pub fn find_placement_position(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    _mover_state: PhysicsState,
) -> bool {
    t.sphere_path.check_pos = t.sphere_path.curr_pos;
    t.sphere_path.check_cell = t.sphere_path.curr_cell;
    t.sphere_path.cache_global_sphere(None);
    t.sphere_path.insert_type = InsertType::InitialPlacement;

    let resolver = ctx.resolver();
    let r = match t
        .sphere_path
        .check_cell
        .and_then(|id| resolver.get_visible(id))
    {
        None => TransitionState::Collided,
        Some(cell) => {
            let first = insert_into_cell(ctx, t, &cell, ATTEMPTS_NORMAL);
            if first == TransitionState::Ok {
                crate::transition::insert::check_other_cells(ctx, t, Some(&cell))
            } else {
                first
            }
        }
    };
    if validate_placement(ctx, t, r, true) != TransitionState::Ok {
        return false;
    }

    t.sphere_path.insert_type = InsertType::Placement;
    if !find_placement_pos(ctx, t) {
        return false;
    }

    if t.object_info.step_down {
        let mut z = t.object_info.step_down_height;
        t.sphere_path.walkable_allowance = LANDING_Z;
        t.sphere_path.save_check_pos();
        let backup = t.sphere_path.insert_type;
        t.sphere_path.insert_type = InsertType::Transition;
        let r0 = t.sphere_path.global_sphere[0].radius;
        if t.sphere_path.num_sphere < 2 && 2.0 * r0 < z {
            z = r0 * 0.5;
        }
        let ok = if z <= 2.0 * r0 {
            super::walk::step_down(ctx, t, z, LANDING_Z)
        } else {
            z *= 0.5;
            super::walk::step_down(ctx, t, z, LANDING_Z)
                || super::walk::step_down(ctx, t, z, LANDING_Z)
        };
        if !ok {
            t.sphere_path.restore_check_pos();
            t.collision_info.contact_plane_valid = false;
            t.collision_info.contact_plane_is_water = false;
        }
        t.sphere_path.insert_type = backup;
        t.sphere_path.walkable = None;
    }
    validate_placement(ctx, t, TransitionState::Ok, true) == TransitionState::Ok
}

/// One sample of the scatter spiral, for the test that reproduces the sequence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScatterSample {
    pub ring: u32,
    pub index: u32,
    pub distance: f32,
    pub heading_degrees: f32,
}

/// The scatter sequence walks, computed without touching the
/// world. Exposed so the sequence itself can be asserted in order.
#[must_use]
pub fn scatter_sequence(radius: f32) -> Vec<ScatterSample> {
    let mut r = radius;
    let mut max_dist = 4.0_f32;
    let mut small = false;
    if r < 0.125 {
        small = true;
        max_dist = 2.0;
    } else if r < 0.48 {
        r = 0.48;
    }

    // Note: always 4.0, even when max_dist is 2.0.
    let mut n = 4.0 / r;
    if small {
        n *= 0.5;
    }
    if n < 1.0 {
        return Vec::new();
    }
    #[allow(clippy::cast_sign_loss)]
    let rings = dereth_primitives::num::to_i32(n.ceil()).max(0) as u32;
    #[allow(clippy::cast_precision_loss)]
    let ring_step = max_dist / rings as f32;
    let ang_step = (ring_step / r) * std::f32::consts::PI;

    let mut out = Vec::new();
    let mut dist = 0.0_f32;
    let mut ang = 0.0_f32;
    for ring in 0..rings {
        dist += ring_step;
        ang += ang_step;
        #[allow(clippy::cast_sign_loss)]
        let k = 2 * dereth_primitives::num::to_i32(ang.ceil()).max(0) as u32;
        if k == 0 {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let deg_per_sample = 360.0 / k as f32;
        for i in 0..k {
            #[allow(clippy::cast_precision_loss)]
            out.push(ScatterSample {
                ring,
                index: i,
                distance: dist,
                heading_degrees: i as f32 * deg_per_sample,
            });
        }
    }
    out
}

/// The spiral used when an object cannot be placed
/// exactly where it was asked to go.
pub fn find_placement_pos(ctx: &TransitionCtx<'_>, t: &mut Transition) -> bool {
    t.sphere_path.check_pos = t.sphere_path.curr_pos;
    t.sphere_path.check_cell = t.sphere_path.curr_cell;
    t.sphere_path.cache_global_sphere(None);
    t.collision_info.sliding_normal_valid = false;
    t.collision_info.contact_plane_valid = false;
    t.collision_info.contact_plane_is_water = false;

    let first = transitional_insert(ctx, t, ATTEMPTS_NORMAL);
    if validate_placement_transition(t, first) == TransitionState::Ok {
        return true;
    }
    if !t.sphere_path.placement_allows_sliding {
        return false;
    }

    for sample in scatter_sequence(t.sphere_path.local_sphere[0].radius) {
        t.sphere_path.check_pos = t.sphere_path.curr_pos;
        t.sphere_path.check_cell = t.sphere_path.curr_cell;
        t.sphere_path.cache_global_sphere(None);

        let mut frame = Frame::default();
        math::set_heading(&mut frame, sample.heading_degrees);
        let dir = math::get_vector_heading(&frame);
        let offset = adjust_offset(t, dir.mul(sample.distance));
        // `EpsilonSq = 3.9999996e-08` rejection.
        if offset.mag2() <= EPSILON_SQ {
            continue;
        }
        t.sphere_path.cell_array_valid = false;
        t.sphere_path.check_pos.frame.origin = t.sphere_path.check_pos.frame.origin.add(offset);
        t.sphere_path.cache_global_sphere(Some(offset));
        t.collision_info.sliding_normal_valid = false;
        t.collision_info.contact_plane_valid = false;
        t.collision_info.contact_plane_is_water = false;
        let r = transitional_insert(ctx, t, ATTEMPTS_NORMAL);
        if validate_placement_transition(t, r) == TransitionState::Ok {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::Vec3;

    // Oracle: the find_placement_pos description in
    // the recovered collision and transition behavior.

    #[test]
    fn the_scatter_spiral_grows_outward_and_denser() {
        // A 0.5 m sphere: r stays 0.5, maxDist 4, n = 8, rings = 8, ringStep = 0.5.
        let seq = scatter_sequence(0.5);
        assert!(!seq.is_empty());
        // Distances are strictly increasing per ring and the first ring is one ringStep out.
        let ring0: Vec<&ScatterSample> = seq.iter().filter(|s| s.ring == 0).collect();
        assert!((ring0[0].distance - 0.5).abs() < 1e-5, "{:?}", ring0[0]);
        let last_ring = seq.last().expect("non-empty").ring;
        assert_eq!(last_ring, 7, "8 rings, indexed 0..7");
        let outer: Vec<&ScatterSample> = seq.iter().filter(|s| s.ring == last_ring).collect();
        assert!(
            (outer[0].distance - 4.0).abs() < 1e-4,
            "the outermost ring reaches maxDist"
        );
        // Sample counts grow with the ring index.
        let counts: Vec<usize> = (0..=last_ring)
            .map(|r| seq.iter().filter(|s| s.ring == r).count())
            .collect();
        assert!(counts.windows(2).all(|w| w[1] >= w[0]), "{counts:?}");
        assert!(counts[last_ring as usize] > counts[0]);
    }

    #[test]
    fn every_ring_covers_a_full_circle_of_headings() {
        for radius in [0.1_f32, 0.3, 0.5, 1.0, 2.0] {
            let seq = scatter_sequence(radius);
            if seq.is_empty() {
                continue;
            }
            let rings = seq.last().expect("non-empty").ring + 1;
            for r in 0..rings {
                let mut headings: Vec<f32> = seq
                    .iter()
                    .filter(|s| s.ring == r)
                    .map(|s| s.heading_degrees)
                    .collect();
                assert!(!headings.is_empty());
                headings.sort_by(f32::total_cmp);
                assert!((headings[0] - 0.0).abs() < 1e-5, "radius {radius} ring {r}");
                let step = 360.0 / headings.len() as f32;
                for (i, h) in headings.iter().enumerate() {
                    #[allow(clippy::cast_precision_loss)]
                    let expect = i as f32 * step;
                    assert!(
                        (h - expect).abs() < 1e-3,
                        "radius {radius} ring {r} sample {i}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_very_small_object_scatters_only_two_metres_but_uses_the_four_metre_ring_count() {
        // radius < 0.125 sets maxDist = 2 and halves n - but n itself is still 4.0 / r.
        let seq = scatter_sequence(0.1);
        let last = seq.last().expect("non-empty");
        assert!(
            (last.distance - 2.0).abs() < 1e-4,
            "the outer ring stops at 2 m: {last:?}"
        );
        // n = (4 / 0.1) * 0.5 = 20 rings.
        assert_eq!(last.ring, 19);
    }

    #[test]
    fn a_small_but_not_tiny_radius_is_floored_at_zero_point_four_eight() {
        // Two radii either side of 0.48 that both floor to it must give identical sequences.
        let a = scatter_sequence(0.2);
        let b = scatter_sequence(0.48);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert!((x.distance - y.distance).abs() < 1e-6);
            assert!((x.heading_degrees - y.heading_degrees).abs() < 1e-6);
        }
        // ..and 0.125 (which is NOT small) uses the floor, not the small-object path.
        let c = scatter_sequence(0.125);
        assert_eq!(c.len(), b.len());
    }

    #[test]
    fn a_huge_object_scatters_not_at_all() {
        // n = 4 / r < 1 for r > 4: the search gives up before it starts.
        assert!(scatter_sequence(5.0).is_empty());
        assert!(!scatter_sequence(3.9).is_empty());
    }

    /// The `EpsilonSq = 3.9999996e-08` rejection: an offset whose squared length is at or below it
    /// is skipped. This pins the magnitude at which that fires.
    #[test]
    fn the_epsilon_squared_rejection_fires_at_the_documented_magnitude() {
        assert_eq!(EPSILON_SQ.to_bits(), 3.999_999_6e-08_f32.to_bits());
        // A displacement of 2e-4 has squared length exactly EPSILON_SQ and is rejected (`<=`).
        let just_at = Vec3::new(crate::globals::EPSILON, 0.0, 0.0);
        assert!(just_at.mag2() <= EPSILON_SQ);
        let just_over = Vec3::new(crate::globals::EPSILON * 1.01, 0.0, 0.0);
        assert!(just_over.mag2() > EPSILON_SQ);
    }
}
