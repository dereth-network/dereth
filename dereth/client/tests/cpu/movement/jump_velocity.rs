//! A failed jump-velocity inquiry is not a zero-skill jump: it gives no vertical velocity, the
//! no-weenie fallback is its own arm, and a refused jump permission rejects the charge whatever the
//! velocity says. Fixture: none; the animation crate's motion interpreter with a synthetic
//! environment.
//!
//! Behaviour: none (a contract of the motion interpreter's velocity inquiry).

#[test]
fn failed_jump_velocity_inquiry_is_not_a_valid_zero_skill_minimum() {
    let mut interp = dereth_animation::motion::MotionInterp::new();
    interp.jump_extent = 0.6;
    let mut env = dereth_animation::motion::MotionEnv::default();
    assert_eq!(
        interp.get_jump_v_z(&env).to_bits(),
        2.619_160_2_f32.to_bits()
    );
    env.jump_velocity_available = false;
    assert_eq!(interp.get_jump_v_z(&env), 0.0);
    env.has_weenie = false;
    assert_eq!(
        interp.get_jump_v_z(&env),
        10.0,
        "native no-weenie fallback is a different arm"
    );
    env.has_weenie = true;
    env.jump_permission = Some(false);
    assert_eq!(
        interp.charge_jump(0.0, &env),
        0x49,
        "a missing player-description CanJump quality rejects independently of velocity inquiry"
    );
}
