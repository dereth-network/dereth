//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! A traced body leaves one trace line per physics sub-step, with the sweep's outcome.
//! Fixture: flat synthetic land and a one-sphere body walking on scripted offsets.

#![cfg(feature = "trace")]

use dereth_physics::{Plane, ScriptedMotion, TransitionState};
use dereth_primitives::{CellId, Frame, LocalTime, ObjectId, Position, Quat, Vec3};

use crate::common::physics_fixture::{flat_world, player_geometry};

/// A frame of a quarter second is two sub-steps (the longest and the rest), and each leaves a line:
/// standing still the sub-step sweeps nothing, walking it moves.
#[test]
fn a_traced_body_leaves_one_line_per_sub_step_with_the_sweeps_outcome() {
    let mut w = flat_world();
    let cell = CellId(0xA9B4_000A);
    let h = w.create(ObjectId(0x5000_0001), player_geometry(), true);
    w.enter_cell(h, cell);
    {
        let o = w.get_mut(h).expect("live");
        o.position = Position::new(
            cell,
            Frame::new(Vec3::new(30.0, 30.0, 20.0), Quat::IDENTITY),
        );
        // Two sub-steps standing, then two walking north.
        let still = Frame::default();
        let walk = Frame::new(Vec3::new(0.0, 0.25, 0.0), Quat::IDENTITY);
        o.set_motion(Box::new(ScriptedMotion::new(
            vec![still, still, walk, walk],
            true,
        )));
        o.transient_state.set_active_bit(true);
        o.transient_state.set_contact(true);
        o.transient_state.set_on_walkable_bit(true);
        o.contact_plane = Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: -20.0,
        };
        o.contact_plane_cell_id = cell;
        o.calc_acceleration();
        o.update_time = 1.0;
    }
    w.calc_cross_cells(h, false);
    w.trace_object(h);
    for t in [1.25, 1.5] {
        w.use_time(LocalTime(t), false);
    }
    let trace = w.take_trace();
    assert_eq!(trace.len(), 4, "two frames of two sub-steps: {trace:?}");
    let quanta: Vec<f64> = std::iter::once(1.0)
        .chain(trace.iter().map(|r| r.update_time))
        .collect::<Vec<_>>()
        .windows(2)
        .map(|p| p[1] - p[0])
        .collect();
    for (q, want) in quanta.iter().zip([0.2, 0.05, 0.2, 0.05]) {
        assert!((q - want).abs() < 1e-9, "the sub-steps' quanta: {quanta:?}");
    }
    assert_eq!(trace[0].t, 1.25, "a line carries its frame's time");
    assert_eq!(trace[3].t, 1.5);
    assert_eq!(
        trace[0].transition_state,
        TransitionState::Invalid,
        "standing still sweeps nothing"
    );
    assert_eq!(trace[2].transition_state, TransitionState::Ok);
    assert!(
        trace[3].origin.y > trace[1].origin.y + 0.4,
        "the body walked: {:?} -> {:?}",
        trace[1].origin,
        trace[3].origin
    );
    assert!(
        trace[3].contact_plane.is_some(),
        "a body in contact has its plane"
    );
}
