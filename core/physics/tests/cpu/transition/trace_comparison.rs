//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! Trace comparisons detect differences in every recorded field.
//! Fixture: synthetic state, geometry and reference vectors.

#![cfg(feature = "trace")]

/// A changed field is reported by the trace comparison.
#[test]
fn the_comparison_notices_every_field_it_is_supposed_to() {
    use dereth_physics::trace::{compare, TraceRecord};
    use dereth_physics::TransitionState;
    use dereth_primitives::{CellId, Quat, Vec3};

    let base = TraceRecord {
        t: 0.0,
        update_time: 0.0,
        cell: CellId(0xA9B4_0001),
        origin: Vec3::new(12.0, 12.0, 20.0),
        quat: Quat::IDENTITY,
        velocity: Vec3::new(1.0, 0.0, 0.0),
        cached_velocity: Vec3::new(1.0, 0.0, 0.0),
        contact_plane: Some(dereth_physics::Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: -20.0,
        }),
        state: 0x0040_0C08,
        transient_state: 0x0000_0083,
        transition_state: TransitionState::Ok,
    };
    let recorded = vec![base.clone()];

    type Mutation = (&'static str, fn(&mut TraceRecord));
    let mutate: &[Mutation] = &[
        ("cell", |r| r.cell = CellId(0xA9B4_0002)),
        ("origin", |r| r.origin.z += 1.0),
        ("velocity", |r| r.velocity.x += 1.0),
        ("cached_velocity", |r| r.cached_velocity.x += 1.0),
        ("contact_plane", |r| r.contact_plane = None),
        ("state", |r| r.state |= 0x0080_0000),
        ("transient_state", |r| r.transient_state |= 0x0000_0004),
        ("transition_state", |r| {
            r.transition_state = TransitionState::Collided
        }),
    ];
    for (field, f) in mutate {
        let mut replayed = recorded.clone();
        f(&mut replayed[0]);
        let d = compare(&recorded, &replayed);
        assert!(
            d.iter().any(|x| x.field == *field),
            "a divergence in `{field}` was not reported: {d:?}"
        );
    }
}
