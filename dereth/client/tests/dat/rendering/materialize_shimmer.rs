//! The player's hide/materialise shimmer follows fourteen body parts: each emitter samples its own
//! part's live position for every later particle.
//!
//! Fixture: the shipped hide script and its particle hooks, run through `MotionDriver` on the
//! Aluvian male setup; no emitter is constructed by the test.

use std::collections::BTreeSet;
use std::sync::Arc;

use dereth_animation::{AnimAssets, MotionDriver};
use dereth_client::anim_assets::DatAnimAssets;
use dereth_primitives::{DataId, Frame, ServerTime, Vec3};

const PS_HIDE: u32 = 116;
const PLAYER_SCRIPT_TABLE: DataId = DataId(0x3400_0004);

fn player_driver() -> MotionDriver {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let assets: Arc<dyn AnimAssets> = Arc::new(DatAnimAssets::new(store));
    let setup = assets
        .setup(dereth_client::character::ALUVIAN_MALE_SETUP)
        .expect("the shipped Aluvian male setup");
    let mut driver = MotionDriver::new(Arc::clone(&assets));
    assert!(driver.set_setup(setup));
    driver.script_table = Some(PLAYER_SCRIPT_TABLE);
    driver.env.in_cell = true;
    driver.update_parts(&Frame::new(Vec3::new(20.0, 30.0, 40.0), Default::default()));
    driver
}

/// Behaviour: rendering.particles.the-player-materialise-shimmer-follows-each-body-part
/// Particle initialization selects the named parent part's live position. The particle-update path
/// repeats that selection for every parent-local particle and every later birth.
#[test]
fn the_player_shimmer_updates_each_emitter_from_its_own_live_part() {
    let mut driver = player_driver();
    driver.cur_time = ServerTime(0.0);
    assert!(
        driver.play_script_type(PS_HIDE, 1.0),
        "the player's hide-script row resolves"
    );
    driver.update_scripts();

    assert_eq!(
        driver.particles.len(),
        14,
        "the shipped script creates fourteen emitters"
    );
    let parts = driver
        .particles
        .iter()
        .map(|emitter| {
            assert_ne!(emitter.part_index, dereth_animation::particles::NO_PART);
            emitter.part_index
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        parts.len(),
        14,
        "the script distributes them over fourteen distinct body parts"
    );

    // Move every authored part by a different vector after the initial burst. At t=1 the shipped
    // 0.5/0.75-second particles have died and each infinite emitter makes a later particle. That
    // later birth must sample the emitter's own *current* part rather than part zero.
    for &part in &parts {
        let pose = &mut driver.part_array.parts[usize::try_from(part).expect("part index")].pos;
        pose.origin.x += 100.0 + part as f32 * 3.0;
        pose.origin.y += 200.0 + part as f32 * 5.0;
        pose.origin.z += 300.0 + part as f32 * 7.0;
    }
    driver.cur_time = ServerTime(1.0);
    driver.update_particles(true);

    for emitter in driver.particles.iter() {
        let expected = driver.part_array.parts[usize::try_from(emitter.part_index).unwrap()].pos;
        let later = emitter
            .live()
            .find(|particle| particle.birthtime == 1.0)
            .expect("the infinite emitter made a later particle after its initial one expired");
        assert_eq!(
            later.start_frame, expected,
            "emitter {} on part {} sampled a different body part",
            emitter.id, emitter.part_index,
        );
    }
}
