//! The trace recorder the physics trace gate is built from.
//!
//! A trace is the same simulation dumped every sub-step on both sides -- position (cell and
//! frame), velocity, cached velocity, contact plane, state words and transition outcome -- from a
//! fixed start and a fixed script of movement commands with fixed time deltas, then compared.
//!
//! The recorder is behind `feature = "trace"` because it costs a `Vec` push per sub-step and
//! nothing in normal play reads it.
//!
//! **What is here and what is not.** The recorder, the record shape and the comparison (with the
//! spec's tolerance rule) are implemented. The **fixtures are not**: `fixtures/physics/traces/`
//! does not exist, and producing it is the expensive half — a debugger script against the client,
//! or a `.pcap` of a scripted walk against the
//! local ACE server. `tests/cpu/instruments/recorded_traces.rs` therefore reports the scenario list as missing rather than
//! passing vacuously, which is the honest signal: a gate that passes when there is nothing to
//! compare is not a gate.

use dereth_primitives::{CellId, Quat, Vec3};

use crate::geom::plane::Plane;
use crate::transition::TransitionState;

/// One line of a trace: the state at the end of one **sub-step**.
///
/// The field set is the one `traces/<scenario>.jsonl` records.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceRecord {
    /// The time at the frame that produced this sub-step.
    pub t: f64,
    /// The object's own clock after the sub-step, i.e. the physics clock's current time.
    pub update_time: f64,
    pub cell: CellId,
    pub origin: Vec3,
    pub quat: Quat,
    /// `velocity_vector`.
    pub velocity: Vec3,
    /// `cached_velocity`, the achieved velocity.
    pub cached_velocity: Vec3,
    /// `None` when `contact_plane_valid` was false.
    pub contact_plane: Option<Plane>,
    pub state: u32,
    pub transient_state: u32,
    pub transition_state: TransitionState,
}

/// The recorder. Attach one to a `PhysicsWorld` driver and push a record per sub-step.
#[derive(Debug, Default, Clone)]
pub struct TraceRecorder {
    pub records: Vec<TraceRecord>,
    pub enabled: bool,
}

impl TraceRecorder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            enabled: true,
        }
    }

    pub fn push(&mut self, r: TraceRecord) {
        if self.enabled {
            self.records.push(r);
        }
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// One divergence between a recorded trace and a replayed one.
#[derive(Debug, Clone, PartialEq)]
pub struct Divergence {
    pub index: usize,
    pub field: &'static str,
    pub detail: String,
}

/// The spec's tolerance rule: `origin` and `velocity` compare with a relative epsilon of `1e-5`
/// per component for the first 100 sub-steps and `1e-4` thereafter, because the integrator feeds
/// its own output back in and unbounded strictness is a test of nothing. `cell`, both state words
/// and `transition_state` compare **exactly**.
#[must_use]
pub fn tolerance_for(index: usize) -> f32 {
    if index < 100 {
        1e-5
    } else {
        1e-4
    }
}

fn close(a: f32, b: f32, rel: f32) -> bool {
    let scale = a.abs().max(b.abs()).max(1.0);
    (a - b).abs() <= rel * scale
}

fn vec_close(a: Vec3, b: Vec3, rel: f32) -> bool {
    close(a.x, b.x, rel) && close(a.y, b.y, rel) && close(a.z, b.z, rel)
}

/// Compare a replayed trace against a recorded one, per the gate's rule.
#[must_use]
pub fn compare(recorded: &[TraceRecord], replayed: &[TraceRecord]) -> Vec<Divergence> {
    let mut out = Vec::new();
    if recorded.len() != replayed.len() {
        out.push(Divergence {
            index: 0,
            field: "length",
            detail: format!(
                "recorded {} sub-steps, replayed {}",
                recorded.len(),
                replayed.len()
            ),
        });
    }
    for (i, (a, b)) in recorded.iter().zip(replayed.iter()).enumerate() {
        let rel = tolerance_for(i);
        let mut bad = |field: &'static str, detail: String| {
            out.push(Divergence {
                index: i,
                field,
                detail,
            });
        };
        if a.cell != b.cell {
            bad("cell", format!("{:?} vs {:?}", a.cell, b.cell));
        }
        if a.state != b.state {
            bad("state", format!("{:#010X} vs {:#010X}", a.state, b.state));
        }
        if a.transient_state != b.transient_state {
            bad(
                "transient_state",
                format!("{:#010X} vs {:#010X}", a.transient_state, b.transient_state),
            );
        }
        if a.transition_state != b.transition_state {
            bad(
                "transition_state",
                format!("{:?} vs {:?}", a.transition_state, b.transition_state),
            );
        }
        if !vec_close(a.origin, b.origin, rel) {
            bad("origin", format!("{:?} vs {:?}", a.origin, b.origin));
        }
        if !vec_close(a.velocity, b.velocity, rel) {
            bad("velocity", format!("{:?} vs {:?}", a.velocity, b.velocity));
        }
        if !vec_close(a.cached_velocity, b.cached_velocity, rel) {
            bad(
                "cached_velocity",
                format!("{:?} vs {:?}", a.cached_velocity, b.cached_velocity),
            );
        }
        match (a.contact_plane, b.contact_plane) {
            (None, None) => {}
            (Some(p), Some(q)) => {
                if !vec_close(p.normal, q.normal, rel) || !close(p.d, q.d, rel) {
                    bad("contact_plane", format!("{p:?} vs {q:?}"));
                }
            }
            _ => bad(
                "contact_plane",
                format!("{:?} vs {:?}", a.contact_plane, b.contact_plane),
            ),
        }
    }
    out
}

/// The scenarios the gate must contain, each with the behaviour it exercises. The fixture
/// generator has to produce a `traces/<name>.jsonl` and `<name>.input.json` for each.
pub const REQUIRED_SCENARIOS: &[(&str, &str)] = &[
    ("walk_up_48_degree_slope", "the walkable-floor threshold"),
    ("sled_9_degree_slope", "the slope friction branch"),
    ("sled_11_degree_slope", "the slope friction branch"),
    ("step_onto_4cm_lip", "step-up onto a low edge"),
    (
        "step_onto_lip_1mm_over_step_up_height",
        "step-up refused just over the step height",
    ),
    ("walk_off_narrow_ledge", "the edge-slide halving probe"),
    (
        "swim_into_deep_sea",
        "deep water collides rather than swims",
    ),
    (
        "cross_landblock_boundary_diagonally",
        "landblock-edge cell continuity",
    ),
    (
        "walk_through_portal_into_dungeon",
        "portal cell expansion into a dungeon",
    ),
    (
        "collide_with_moving_object",
        "cached velocity goes to zero on collision",
    ),
    ("same_walk_at_250fps", "frame-rate quantum clamping"),
    ("same_walk_at_30fps", "frame-rate quantum clamping"),
    ("same_walk_at_4fps", "frame-rate quantum clamping"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(i: usize, z: f32) -> TraceRecord {
        TraceRecord {
            t: i as f64 * 0.1,
            update_time: i as f64 * 0.1,
            cell: CellId(0xA9B4_0001),
            origin: Vec3::new(1.0, 2.0, z),
            quat: Quat::IDENTITY,
            velocity: Vec3::ZERO,
            cached_velocity: Vec3::ZERO,
            contact_plane: None,
            state: 0x0040_0C08,
            transient_state: 0x81,
            transition_state: TransitionState::Ok,
        }
    }

    #[test]
    fn the_tolerance_widens_after_a_hundred_sub_steps() {
        assert_eq!(tolerance_for(0), 1e-5);
        assert_eq!(tolerance_for(99), 1e-5);
        assert_eq!(tolerance_for(100), 1e-4);
    }

    #[test]
    fn an_identical_trace_has_no_divergences() {
        let a: Vec<TraceRecord> = (0..10).map(|i| rec(i, 20.0)).collect();
        assert!(compare(&a, &a).is_empty());
    }

    #[test]
    fn the_state_words_and_the_cell_compare_exactly() {
        let a = vec![rec(0, 20.0)];
        let mut b = a.clone();
        b[0].transient_state |= 0x100;
        let d = compare(&a, &b);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].field, "transient_state");

        let mut b = a.clone();
        b[0].cell = CellId(0xA9B4_0002);
        assert_eq!(compare(&a, &b)[0].field, "cell");

        let mut b = a.clone();
        b[0].transition_state = TransitionState::Collided;
        assert_eq!(compare(&a, &b)[0].field, "transition_state");
    }

    #[test]
    fn the_origin_tolerance_accepts_a_relative_error_below_the_threshold() {
        let a = vec![rec(0, 20.0)];
        let mut b = a.clone();
        // 20 * 1e-6 is well inside the 1e-5 relative epsilon.
        b[0].origin.z = 20.000_02;
        assert!(compare(&a, &b).is_empty());
        // 20 * 1e-3 is not.
        b[0].origin.z = 20.02;
        assert_eq!(compare(&a, &b)[0].field, "origin");
    }

    #[test]
    fn a_length_mismatch_is_reported_before_anything_else() {
        let a: Vec<TraceRecord> = (0..3).map(|i| rec(i, 20.0)).collect();
        let b: Vec<TraceRecord> = (0..2).map(|i| rec(i, 20.0)).collect();
        let d = compare(&a, &b);
        assert_eq!(d[0].field, "length");
    }

    #[test]
    fn the_required_scenario_list_names_each_scenario_once_with_its_behaviour() {
        let names: Vec<&str> = REQUIRED_SCENARIOS.iter().map(|(n, _)| *n).collect();
        assert_eq!(names.len(), 13);
        let unique: std::collections::BTreeSet<&str> = names.iter().copied().collect();
        assert_eq!(unique.len(), names.len(), "a scenario is listed twice");
        for (name, target) in REQUIRED_SCENARIOS {
            assert!(!target.is_empty(), "scenario {name} names no behaviour");
        }
        for behaviour in [
            "walkable-floor",
            "step-up",
            "friction",
            "halving probe",
            "cached velocity",
            "deep water",
        ] {
            assert!(
                REQUIRED_SCENARIOS
                    .iter()
                    .any(|(_, t)| t.contains(behaviour)),
                "no scenario exercises {behaviour}"
            );
        }
    }

    #[test]
    fn a_recorder_can_be_disabled() {
        let mut r = TraceRecorder::new();
        r.push(rec(0, 20.0));
        assert_eq!(r.len(), 1);
        r.enabled = false;
        r.push(rec(1, 20.0));
        assert_eq!(r.len(), 1);
        r.clear();
        assert!(r.is_empty());
    }
}
