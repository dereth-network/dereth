//! Animation hooks fired inside the frame reach the physics body. An ethereal hook
//! (`AnimEvent::SetEthereal`) sets and clears the body's ethereal state, in both directions; a
//! set-omega hook (`AnimEvent::SetOmega`) replaces the body's angular velocity with the hook's own
//! axis -- a set, not an add, so the last hook crossed in a step wins -- and physics integration
//! then turns the body across simulated time. Fixture: a `WorldScene` with an attached body on the
//! retail dats and a software GPU device. The ethereal tests play the On and Off motions of the
//! first retail motion table (found by scanning, for the door setup `0x0200024F`) whose hooks raise
//! a matched ethereal pair; the set-omega tests push the hook onto the body's animation-hook queue
//! (decoding hooks out of a motion table is `dereth-animation`'s to test). The remote-object lookup
//! a live door takes is `dat::objects::door_opens_ethereal`'s. Absent dats or device fail.

#![cfg(gpu)]

use dereth_scene::world_scene::SceneWrites;
use std::sync::Arc;

use dereth_animation::hooks::{AnimHook, HookKind};
use dereth_animation::{AnimAssets, AnimEvent, MotionCommand, MotionDriver};
use dereth_client_runtime::character::CharacterInput;
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::V3;
use dereth_primitives::{DataId, LocalTime, Vec3};
use dereth_world_data::anim_assets::DatAnimAssets;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

/// The door setup used while scanning for a table with the required hook pair.
const DOOR_SETUP: u32 = 0x0200_024F;

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// The first retail motion table whose `On` raises `SetEthereal(true)` and whose `Off` raises
/// `SetEthereal(false)`, found by playing each one. Data, not a constant.
fn an_ethereal_motion_table(store: &Arc<RetailDatStore>) -> DataId {
    let assets: Arc<dyn AnimAssets> = Arc::new(DatAnimAssets::new(Arc::clone(store)));
    let setup = AnimAssets::setup(assets.as_ref(), DataId(DOOR_SETUP)).expect("the door setup");
    let play = |driver: &mut MotionDriver, cmd: MotionCommand| -> Vec<bool> {
        driver.do_interpreted_motion(
            cmd,
            &dereth_animation::motion::MovementParameters::default(),
        );
        let mut out = Vec::new();
        let mut t = 0.0;
        for _ in 0..90 {
            t += 1.0 / 30.0;
            dereth_physics::MotionSource::advance(driver, 1.0 / 30.0);
            dereth_physics::MotionSource::tick_movement(driver, LocalTime(t));
            dereth_physics::MotionSource::process_hooks(driver);
            out.extend(driver.take_events().into_iter().filter_map(|e| match e {
                AnimEvent::SetEthereal(on) => Some(on),
                _ => None,
            }));
        }
        out
    };
    for mt in store.ids_of(DbType::MTable) {
        let mut d = MotionDriver::new(Arc::clone(&assets));
        if !d.set_setup(Arc::clone(&setup)) || !d.set_motion_table(mt) {
            continue;
        }
        if play(&mut d, MotionCommand::ON).first() == Some(&true)
            && play(&mut d, MotionCommand::OFF).first() == Some(&false)
        {
            return mt;
        }
    }
    panic!("no retail motion table raises a matched ethereal pair; the emitter is broken");
}

/// Behaviour: movement.hooks.an-ethereal-hook-reaches-the-physics-object
/// A DAT ethereal hook fired during WorldScene::update must change the body's physics state
/// in both directions. Check the bit as well as applied/no-body counters: counting events alone
/// cannot prove they reached a body. The independent remote-object lookup arm remains outside
/// this local-character substitution.
#[test]
fn an_ethereal_hook_fired_inside_the_frame_reaches_the_physics_object() {
    let mut gpu = crate::common::test_gpu(800, 600);
    let store = store();
    let region = dereth_world_data::landblock::load_region(&store).expect("region");
    let mtable = an_ethereal_motion_table(&store);
    eprintln!(
        "ethereal hook: driving the body with retail motion table {:#010X}",
        mtable.0
    );

    let mut scene = WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the character attaches");

    let step = |scene: &mut WorldScene, t: &mut f64, frames: u32| {
        for _ in 0..frames {
            *t += 1.0 / 30.0;
            scene.update(
                dereth_client_runtime::camera::CameraInput::default(),
                CharacterInput::default(),
                LocalTime(*t),
                1.0 / 30.0,
            );
        }
    };
    let ethereal = |scene: &WorldScene| -> bool {
        let c = scene.character.as_ref().expect("attached");
        c.world.get(c.handle).expect("live").state.is_ethereal()
    };

    let mut t = 0.0;
    step(&mut scene, &mut t, 10);
    assert!(!ethereal(&scene), "the body starts solid");
    assert_eq!(
        scene.draw.stats.ethereal.hooks, 0,
        "no ethereal hook has fired yet"
    );

    // Install the discovered door motion table on the body, then play its On command.
    {
        let c = scene.character.as_ref().expect("attached");
        let mut d = c.driver_mut();
        assert!(
            d.set_motion_table(mtable),
            "the motion table would not load onto the body"
        );
        d.do_interpreted_motion(
            MotionCommand::ON,
            &dereth_animation::motion::MovementParameters::default(),
        );
    }
    step(&mut scene, &mut t, 120);

    let s = scene.draw.stats.ethereal;
    eprintln!(
        "ethereal hook after ON: {} hook(s) ({} asking for ethereal ON), {} applied, {} deferred, \
         {} reached no body",
        s.hooks, s.hooks_on, s.applied, s.deferred, s.no_body
    );
    assert!(s.hooks > 0, "no SetEthereal event reached the seam at all");
    assert_eq!(
        s.no_body, 0,
        "the hook reached the seam and found no physics object to apply to"
    );
    assert!(s.applied > 0, "the seam counted hooks and applied none");
    assert!(
        ethereal(&scene),
        "the hook fired and was counted, and ETHEREAL_PS is still clear -- the seam is not \
         reaching the physics object's intangible-state setter"
    );

    // Request Off with no blocking overlap expected, then allow the same 120-frame interval.
    // The final state check proves closure within that interval, not the exact application frame.
    {
        let c = scene.character.as_ref().expect("attached");
        c.driver_mut().do_interpreted_motion(
            MotionCommand::OFF,
            &dereth_animation::motion::MovementParameters::default(),
        );
    }
    step(&mut scene, &mut t, 120);
    let s = scene.draw.stats.ethereal;
    eprintln!(
        "ethereal hook after OFF: {} hook(s) ({} ON), {} applied, {} deferred, {} no body",
        s.hooks, s.hooks_on, s.applied, s.deferred, s.no_body
    );
    assert!(
        s.hooks > s.hooks_on,
        "no hook asking for ethereal OFF ever arrived"
    );
    assert!(
        !ethereal(&scene),
        "the closing hook fired and the body is still ethereal -- a door that opens and never \
         closes is the failure this direction exists to catch"
    );
    assert_eq!(s.no_body, 0);
}

/// `PI / 2` rad/s about z — a quarter turn per simulated second, which the frame cannot reach by
/// accident and which does not wrap inside the window this test watches.
fn quarter_turn_per_second() -> Vec3 {
    Vec3::new(0.0, 0.0, std::f32::consts::FRAC_PI_2)
}

/// Behaviour: movement.hooks.a-set-omega-hook-turns-the-body
/// **The join.** A `SetOmegaHook` executing inside `WorldScene::update`'s own step reaches
/// the physical body's angular-velocity vector, and the body's **frame then turns across simulated
/// time** — which is the assertion that matters, because a build that stored the omega and never
/// integrated it, or one that integrated it once and stopped, both pass a state-word test.
#[test]
fn a_set_omega_hook_fired_inside_the_frame_turns_the_physics_body() {
    let mut gpu = crate::common::test_gpu(800, 600);
    let store = store();
    let region = dereth_world_data::landblock::load_region(&store).expect("region");

    let mut scene = WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the character attaches");

    let mut t = 0.0;
    let step = |scene: &mut WorldScene, t: &mut f64, frames: u32| {
        for _ in 0..frames {
            *t += 1.0 / 30.0;
            scene.update(
                dereth_client_runtime::camera::CameraInput::default(),
                CharacterInput::default(),
                LocalTime(*t),
                1.0 / 30.0,
            );
        }
    };
    let omega = |scene: &WorldScene| -> Vec3 {
        let c = scene.character.as_ref().expect("attached");
        c.world.get(c.handle).expect("live").omega_vector
    };
    let heading = |scene: &WorldScene| -> f32 {
        let c = scene.character.as_ref().expect("attached");
        dereth_physics::math::get_heading(&c.world.get(c.handle).expect("live").position.frame)
    };

    // Settle, then establish that nothing here spins on its own. Without this the whole fixture
    // could be measuring the body's ordinary idle animation.
    step(&mut scene, &mut t, 15);
    assert_eq!(omega(&scene), Vec3::ZERO, "the body starts with no spin");
    assert_eq!(
        scene.draw.stats.ethereal.omega_hooks, 0,
        "and no omega hook has fired yet"
    );
    let settled = heading(&scene);
    step(&mut scene, &mut t, 30);
    assert!(
        (heading(&scene) - settled).abs() < 0.5,
        "an idle body holds its heading over a simulated second ({settled} -> {}); if it does not, \
         every turn this test measures is someone else's",
        heading(&scene)
    );

    // Queue one SetOmega hook with direction 0 so it executes regardless of frame direction.
    // The next update drains the body's animation-hook queue.
    let before = heading(&scene);
    scene
        .character
        .as_ref()
        .expect("attached")
        .driver_mut()
        .hooks
        .push(AnimHook::new(
            0,
            HookKind::SetOmega {
                axis: quarter_turn_per_second(),
            },
        ));

    step(&mut scene, &mut t, 1);

    // **The claim, in the order that matters.** The physical observable first, the counters after
    // as corroboration -- a counter can be incremented by wiring that does nothing.
    assert_eq!(
        omega(&scene),
        quarter_turn_per_second(),
        "the angular-velocity setter with the hook's own axis, verbatim -- a set, not an add"
    );

    // Three samples across a simulated second, each further round than the last: a build that
    // stored the omega and never integrated it, and one that integrated it once and stopped, both
    // pass a state-word test and fail here.
    let mut turned = Vec::new();
    for _ in 0..3 {
        step(&mut scene, &mut t, 10);
        // Unsigned turn, with the one wrap a quarter turn can cross unwrapped: heading is
        // degrees clockwise, so a positive omega about z makes it *decrease* and the
        // raw difference reads 330 where the body has turned 30.
        let d = (heading(&scene) - before).rem_euclid(360.0);
        turned.push(if d > 180.0 { 360.0 - d } else { d });
    }
    assert!(
        turned[0] > 2.0,
        "a third of a second in, the body has turned: {turned:?} deg"
    );
    assert!(
        turned[1] > turned[0] + 2.0,
        "and the turn grows with simulated time: {turned:?} deg"
    );
    assert!(
        turned[2] > turned[1] + 2.0,
        "and keeps growing: {turned:?} deg"
    );
    assert!(
        (turned[2] - 90.0).abs() < 12.0,
        "PI/2 rad/s for a simulated second is a quarter turn, less the 30 Hz gate's dropped \
         remainder: {turned:?} deg"
    );

    assert_eq!(
        scene.draw.stats.ethereal.omega_hooks, 1,
        "one `AnimEvent::SetOmega` was drained, and only one"
    );
    assert_eq!(
        scene.draw.stats.ethereal.omega_hooks_no_body, 0,
        "and it found the body: a run where every hook landed nowhere reads the same as an \
         unwired build without this counter"
    );
}

/// Behaviour: movement.hooks.the-last-set-omega-of-a-step-wins
/// The other half of "a *set*, not an add": two hooks crossed in one step leave the **second**
/// axis on the body: the setter replaces rather than accumulates, and hook processing runs the
/// queue in order. A build that added would leave their sum.
#[test]
fn the_last_set_omega_hook_of_a_step_wins() {
    let mut gpu = crate::common::test_gpu(800, 600);
    let store = store();
    let region = dereth_world_data::landblock::load_region(&store).expect("region");

    let mut scene = WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the character attaches");

    let mut t = 0.0;
    let step = |scene: &mut WorldScene, t: &mut f64, frames: u32| {
        for _ in 0..frames {
            *t += 1.0 / 30.0;
            scene.update(
                dereth_client_runtime::camera::CameraInput::default(),
                CharacterInput::default(),
                LocalTime(*t),
                1.0 / 30.0,
            );
        }
    };
    step(&mut scene, &mut t, 15);

    let first = Vec3::new(0.0, 0.0, 1.0);
    let second = Vec3::new(0.0, 0.0, 4.0);
    {
        let c = scene.character.as_ref().expect("attached");
        let mut d = c.driver_mut();
        d.hooks
            .push(AnimHook::new(0, HookKind::SetOmega { axis: first }));
        d.hooks
            .push(AnimHook::new(0, HookKind::SetOmega { axis: second }));
    }
    step(&mut scene, &mut t, 1);

    let w = {
        let c = scene.character.as_ref().expect("attached");
        c.world.get(c.handle).expect("live").omega_vector
    };
    assert_eq!(
        w,
        second,
        "the second hook's axis, not the sum {:?}",
        first.add(second)
    );
    assert_eq!(
        scene.draw.stats.ethereal.omega_hooks, 2,
        "both hooks executed -- the queue is FIFO"
    );
}
