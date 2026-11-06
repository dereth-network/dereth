//! When the sounds a body makes are silent.

use auberean_physics::body::{Body, BodyConfig};
use auberean_physics::land::land_cell_id;
use dereth_physics::geom::Sphere;
use dereth_primitives::{Frame, Position, Quat, Vec3};

fn body() -> Body {
    let config = BodyConfig {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.48), 0.48)],
        step_up: 0.6,
        step_down: 0.6,
        scale: 1.0,
        ground: Default::default(),
        air: Default::default(),
    };
    let at = Position::new(
        land_cell_id(0x40, 0x40, 3, 3),
        Frame::new(Vec3::new(70.0, 70.0, 10.0), Quat::IDENTITY),
    );
    Body::new(config, at, 0.0)
}

fn nobody<'a>(_: u64) -> Option<&'a Body> {
    None
}

/// Behaviour: motion.sound.what-silences-a-bodys-sounds
#[test]
fn a_moving_body_is_silent_out_of_the_world_or_out_of_range() {
    let mut b = body();
    assert!(!b.should_mute_sound(&nobody));
    b.set_out_of_physics_range(true);
    assert!(b.should_mute_sound(&nobody));
    b.set_out_of_physics_range(false);
    assert!(!b.should_mute_sound(&nobody));
    b.in_world = false;
    assert!(b.should_mute_sound(&nobody));
    b.in_world = true;
    assert!(!b.should_mute_sound(&nobody));
}

/// Behaviour: motion.sound.what-silences-a-bodys-sounds
#[test]
fn a_static_body_is_silent_while_its_cell_is_among_the_free_blocks() {
    let mut b = body();
    b.set_state(b.state() | 0x1);
    assert!(b.is_static());
    // Range and the free blocks: a static body asks only the second.
    b.set_out_of_physics_range(true);
    assert!(!b.should_mute_sound(&nobody));
    b.set_cell_on_free_list(true);
    assert!(b.should_mute_sound(&nobody));
    b.set_cell_on_free_list(false);
    assert!(!b.should_mute_sound(&nobody));
    // Out of the world and of the workspace, as any body with no parent.
    b.in_world = false;
    assert!(b.should_mute_sound(&nobody));
    b.set_in_workspace(true);
    assert!(!b.should_mute_sound(&nobody));
}

/// Behaviour: motion.sound.what-silences-a-bodys-sounds
#[test]
fn what_a_parent_holds_is_heard_as_its_parent_is() {
    let mut holder = body();
    let mut held = body();
    held.has_parent = true;
    held.parent = 7;
    held.in_world = false;
    {
        let find = |id: u64| (id == 7).then_some(&holder);
        assert!(!held.should_mute_sound(&find));
    }
    holder.in_world = false;
    let find = |id: u64| (id == 7).then_some(&holder);
    assert!(held.should_mute_sound(&find));
    // A parent nobody knows silences nothing; nor does a chain that loops for ever.
    assert!(!held.should_mute_sound(&nobody));
    let mut looped = body();
    looped.has_parent = true;
    looped.parent = 7;
    let find = |_: u64| Some(&looped);
    assert!(!looped.should_mute_sound(&find));
}
