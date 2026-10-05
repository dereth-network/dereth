//! A creature's attack hook: every shipped `AttackHook` cone decodes to a symmetric wedge on a
//! non-reflex branch, and an `AnimEvent::Attack` reaches physics attack detection. A landed cone
//! runs the attacker's own default physics script, gated on the attacker's
//! `SCRIPTED_COLLISION_PS`; nothing reaches the wire, the target is not touched, and a swing at
//! empty air hits nothing. Detection tests shadow objects except the attacker and `STATIC_PS`,
//! rejects parented, `IGNORE_COLLISIONS_PS` and `REPORT_COLLISIONS_AS_ENVIRONMENT_PS` objects,
//! and reports hits immediately.
//! Fixture: the retail `client_portal.dat` animations (census, no device), and a `WorldScene`
//! with the Aluvian male character on a software device (seam).

use std::sync::Arc;

use dereth_assets::{Animation, Decode, HookData};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::detect::{cone_is_reflex, AttackCone};
use dereth_primitives::num::math;

// ---------------------------------------------------------------------------------------------
// 1. The corpus. Every number below is measured from the dat.
// ---------------------------------------------------------------------------------------------

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// Every `AttackHook` in `client_portal.dat`: 1,053 instances over 716 of the 2,066 animations,
/// none on the reflex-cone branch, each a wedge symmetric about +Y with one of four half-angles.
/// The denominator is asserted too, so a broken decoder cannot pass by reading fewer animations.
#[test]
fn the_shipped_attack_cones() {
    let store = store();
    let ids = store.ids_of(DbType::Anim);
    assert!(
        ids.len() > 2000,
        "only {} animation(s): not the retail portal.dat",
        ids.len()
    );

    let (mut parsed, mut failed) = (0u32, 0u32);
    let mut anims_with = 0u32;
    let mut cones: Vec<AttackCone> = Vec::new();
    for id in &ids {
        let Ok(bytes) = store.read_typed(DbType::Anim, *id) else {
            failed += 1;
            continue;
        };
        let Ok(anim) = Animation::decode_payload(*id, &bytes) else {
            failed += 1;
            continue;
        };
        parsed += 1;
        let before = cones.len();
        for frame in &anim.part_frames {
            for hook in &frame.hooks {
                if let HookData::Attack {
                    part_index,
                    left,
                    right,
                    radius,
                    height,
                } = &hook.data
                {
                    #[allow(clippy::cast_possible_wrap)]
                    cones.push(AttackCone {
                        part_index: *part_index as i32,
                        left: *left,
                        right: *right,
                        radius: *radius,
                        height: *height,
                    });
                }
            }
        }
        if cones.len() > before {
            anims_with += 1;
        }
    }

    // The denominator, and the animation and instance counts.
    assert_eq!(parsed, 2066, "the denominator");
    assert_eq!(failed, 0, "no animation may be inconclusive");
    assert_eq!(anims_with, 716, "the `Attack` animation count");
    assert_eq!(cones.len(), 1053, "the `Attack` instance count");

    // No shipped cone is reflex: `cross(left, right) < 0` is the ordinary wedge, not a >180-degree
    // one.
    let reflex = cones.iter().filter(|c| cone_is_reflex(c)).count();
    assert_eq!(
        reflex, 0,
        "the reflex-cone branch — the >180-degree wedge — is taken by no shipped cone; if this is \
         non-zero the sense of cone_is_reflex has been flipped back"
    );

    // Every cone is a wedge symmetric about +Y with `left` on -X, which is what "left" means.
    for c in &cones {
        assert!(c.left.0 <= 0.0, "left edge on -X: {c:?}");
        assert!(c.right.0 >= 0.0, "right edge on +X: {c:?}");
        assert!(
            (c.left.0 + c.right.0).abs() < 1e-6,
            "symmetric about +Y: {c:?}"
        );
        assert!(
            (c.left.1 - c.right.1).abs() < 1e-6,
            "symmetric about +Y: {c:?}"
        );
        assert!(c.radius > 0.0 && c.radius <= 25.0, "{c:?}");
        assert!(
            c.height >= 0.0,
            "a negative cone height would be an unconditional miss: {c:?}"
        );
    }

    // Four half-angles, and no others.
    let mut angles: Vec<i32> = cones
        .iter()
        .map(|c| {
            #[allow(clippy::cast_possible_truncation)]
            let a = math::atan2f(c.right.0, c.right.1).to_degrees().round() as i32;
            a
        })
        .collect();
    angles.sort_unstable();
    angles.dedup();
    assert_eq!(angles, vec![10, 30, 40, 45], "the shipped half-angles");

    // The attack part index is signed 32-bit; the common wire value 0xFFFFFFFF means -1.
    // Reinterpreting its bits preserves that sentinel; checked conversion would refuse these 681.
    let no_part = cones.iter().filter(|c| c.part_index == -1).count();
    assert_eq!(no_part, 681, "cones with part_index 0xFFFFFFFF");

    eprintln!(
        "attack-hook census: {} cone(s) over {anims_with}/{parsed} animations; {} reflex; half-angles \
         {angles:?}; {no_part} with no part",
        cones.len(),
        reflex
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The seam
// ---------------------------------------------------------------------------------------------

#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
mod seam {
    use dereth_scene::world_scene::SceneWrites;
    use std::sync::Arc;

    use dereth_animation::{AnimAssets, MotionCommand};
    use dereth_client_runtime::character::CharacterInput;
    use dereth_dat::{DbType, RetailDatStore};
    use dereth_physics::geom::Sphere;
    use dereth_physics::{SetupGeometry, V3 as _};
    use dereth_primitives::{DataId, Frame, LocalTime, ObjectId, Position, Quat, Vec3};
    use dereth_world_data::anim_assets::DatAnimAssets;
    use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

    /// The target body's object id. The attack gesture itself is found by playing the dat:
    /// [`an_attack_motion`] finds the motion table whose `ATTACK_MED1` raises an
    /// `AnimEvent::Attack` on the Aluvian male setup (`0x09000006`, carrying the commonest shipped
    /// cone, `left (-0.5, 0.8660254)`, `right (0.5, 0.8660254)`, `radius 2`, `height 1`).
    const TARGET: ObjectId = ObjectId(0x8000_0F68);

    fn store() -> Arc<RetailDatStore> {
        crate::common::dats()
    }

    /// The first retail motion table whose `ATTACK_MED1` raises an `AnimEvent::Attack` on the
    /// Aluvian male setup, and the cone it carries, found in the data rather than named.
    fn an_attack_motion(store: &Arc<RetailDatStore>) -> (DataId, dereth_animation::AttackCone) {
        let assets: Arc<dyn AnimAssets> = Arc::new(DatAnimAssets::new(Arc::clone(store)));
        let setup = AnimAssets::setup(
            assets.as_ref(),
            dereth_client_runtime::character::ALUVIAN_MALE_SETUP,
        )
        .expect("the Aluvian male setup");
        for mt in store.ids_of(DbType::MTable) {
            let mut d = dereth_animation::MotionDriver::new(Arc::clone(&assets));
            if !d.set_setup(Arc::clone(&setup)) || !d.set_motion_table(mt) {
                continue;
            }
            d.env.in_cell = true;
            d.do_interpreted_motion(
                MotionCommand::ATTACK_MED1,
                &dereth_animation::motion::MovementParameters::default(),
            );
            let mut t = 0.0;
            for _ in 0..180 {
                t += 1.0 / 30.0;
                dereth_physics::MotionSource::advance(&mut d, 1.0 / 30.0);
                dereth_physics::MotionSource::tick_movement(&mut d, LocalTime(t));
                dereth_physics::MotionSource::process_hooks(&mut d);
                for e in d.take_events() {
                    if let dereth_animation::AnimEvent::Attack { cone } = e {
                        return (mt, cone);
                    }
                }
            }
        }
        panic!("no retail motion table's ATTACK_MED1 raises an attack event hook; the emitter is broken");
    }

    /// The first retail `PhysicsScriptTable` and script type whose script **creates a particle
    /// emitter**, so the observable below is an actual particle object, not just a queued-script count.
    ///
    /// The script queue is not a usable observable: a script whose steps all sit at `start_time` 0
    /// is added and drained inside the same `MotionDriver::update_scripts`, so it reads empty on
    /// every frame boundary. An emitter can outlive that queue entry, so the per-frame peak emitter
    /// count observes the resulting effect object. No rendered pixels are compared.
    fn a_particle_script(store: &Arc<RetailDatStore>) -> (DataId, u32) {
        let assets: Arc<dyn AnimAssets> = Arc::new(DatAnimAssets::new(Arc::clone(store)));
        for id in store.ids_of(DbType::PhysicsScriptTable) {
            let Some(table) = assets.script_table(id) else {
                continue;
            };
            for t in 0u32..64 {
                let Some(script) = dereth_animation::get_script(&table, t, 1.0) else {
                    continue;
                };
                let Some(s) = assets.script(script) else {
                    continue;
                };
                let makes_emitter = s.steps.iter().any(|step| {
                    matches!(
                        step.hook.kind,
                        dereth_animation::HookKind::CreateParticle { .. }
                            | dereth_animation::HookKind::CreateBlockingParticle { .. }
                    )
                });
                if makes_emitter {
                    return (id, t);
                }
            }
        }
        panic!("no retail PhysicsScriptTable resolves a particle script; the dat index is broken");
    }

    /// A 0.5 m x 1 m target body, the same shape `dereth-physics`' own suites walk with.
    fn target_geometry() -> Arc<SetupGeometry> {
        Arc::new(SetupGeometry {
            spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
            sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 0.5), 1.0),
            radius: 0.5,
            height: 1.0,
            ..SetupGeometry::default()
        })
    }

    struct Run {
        /// `SceneStats::attack`, after the swing.
        stats: dereth_scene::world_scene::AttackStats,
        /// **The observable: particle emitters on the attacker.**
        /// Default-script playback queues the object's own script, and the selected script creates an
        /// emitter — which is what a brazier's smoke, a lamp's glow and an impact's flash are. Peak
        /// rather than final, because an emitter can be destroyed by a later step of the same script.
        emitters: usize,
        /// The target's state word and origin before and after, so that "the client does not touch
        /// the thing you hit" is asserted rather than assumed.
        target_before: Option<(u32, Vec3)>,
        target_after: Option<(u32, Vec3)>,
    }

    /// One swing. `with_target` puts a body a metre in front; `scripted` sets the attacker's
    /// `SCRIPTED_COLLISION_PS`, which gates the impact-effect callback.
    fn swing(with_target: bool, scripted: bool) -> Run {
        let mut gpu = crate::common::test_gpu(800, 600);
        let store = store();
        let region = dereth_client_runtime::landblock::load_region(&store).expect("region");
        let (mtable, cone) = an_attack_motion(&store);
        let (table_id, script_type) = a_particle_script(&store);
        eprintln!(
            "attack seam: motion table {:#010X} ATTACK_MED1, cone {cone:?}; script table {:#010X} \
             type {script_type}",
            mtable.0, table_id.0
        );

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
        // Settle the body onto the terrain so that it has a cell, required for attack detection.
        step(&mut scene, &mut t, 10);
        assert_eq!(
            scene.draw.stats.attack,
            Default::default(),
            "no attack hook has fired yet"
        );

        // Where is the body, and which way is it facing? Sphere/cone intersection uses the attacker's
        // own frame, so the target has to go in front of it rather than at a compass direction.
        let (body_pos, forward) = {
            let c = scene.character.as_ref().expect("attached");
            let b = c.world.get(c.handle).expect("live body");
            let m = dereth_physics::math::l2g(b.position.frame.rotation);
            (
                b.position,
                dereth_physics::math::localtoglobalvec(m, Vec3::new(0.0, 1.0, 0.0)),
            )
        };

        let mut target_before = None;
        if with_target {
            let at = body_pos.frame.origin.add(forward.mul(1.0));
            let mut cell = body_pos.cell;
            let mut local = at;
            assert!(
                dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut local),
                "the target's spot is on the loaded terrain"
            );
            let c = scene.character.as_mut().expect("attached");
            let h = c.world.create(TARGET, target_geometry(), true);
            c.world.enter_cell(h, cell);
            let frame = Frame::new(at, Quat::IDENTITY);
            let o = c.world.get_mut(h).expect("created");
            o.set_frame(frame);
            o.position = Position::new(cell, frame);
            c.world.calc_cross_cells(h, true);
            let o = c.world.get(h).expect("created");
            target_before = Some((o.state.0, o.position.frame.origin));
        }

        {
            let c = scene.character.as_mut().expect("attached");
            if scripted {
                c.world
                    .get_mut(c.handle)
                    .expect("live")
                    .state
                    .set_scripted_collision(true);
            }
            let h = c.handle;
            assert_eq!(
                c.world.get(h).expect("live").state.has_scripted_collision(),
                scripted,
                "the scripted-collision bit is set as this run wants it"
            );
            // Load the attack motion table, then supply a real physics-script table and default
            // script so that the impact-effect receiver has something to queue.
            let mut d = c.driver_mut();
            assert!(
                d.set_motion_table(mtable),
                "the attack table would not load onto the body"
            );
            d.script_table = Some(table_id);
            d.default_script = script_type;
            d.default_script_intensity = 1.0;
            d.scripts.clear();
            d.do_interpreted_motion(
                MotionCommand::ATTACK_MED1,
                &dereth_animation::motion::MovementParameters::default(),
            );
        }

        // **The sample has to be per frame, not at the end.** A physics script drains as its hooks
        // fire, so a script-queue read 150 frames later can be empty on a run where one was
        // queued.
        let mut emitters = 0usize;
        for _ in 0..150 {
            step(&mut scene, &mut t, 1);
            let c = scene.character.as_ref().expect("attached");
            emitters = emitters.max(c.driver_mut().particles.len());
        }

        let target_after = scene
            .character
            .as_ref()
            .and_then(|c| c.world.by_object_id(TARGET).and_then(|h| c.world.get(h)))
            .map(|o| (o.state.0, o.position.frame.origin));

        Run {
            stats: scene.draw.stats.attack,
            emitters,
            target_before,
            target_after,
        }
    }

    /// Behaviour: combat.attack-hook.a-landed-attack-hook-plays-the-attackers-default-script
    ///
    /// A retail attack motion, played on the body with a body standing a
    /// metre in front of it, reaches physics attack detection, finds the target, and runs the
    /// impact-effect callback: the attacker's own default physics script is queued.
    #[test]
    fn an_attack_hook_that_lands_plays_the_attackers_default_script() {
        let r = swing(true, true);
        eprintln!("attack seam, landed: {:?}", r.stats);
        assert!(
            r.stats.hooks > 0,
            "no AnimEvent::Attack reached the seam at all"
        );
        assert_eq!(
            r.stats.no_body, 0,
            "the hook fired and found no physics body to swing from"
        );
        assert!(
            r.stats.objects_hit > 0,
            "the hook reached the seam and the physics-object attack test reported nothing: the cone found \
             a body one metre in front of the attacker, well inside the shipped radius of 2. \
             {:?}",
            r.stats
        );
        assert!(
            r.emitters > 0,
            "the attack landed and the attacker grew no particle emitter. \
             Object collision handling runs play_default_script when \
             SCRIPTED_COLLISION_PS is set, and that is the whole of the client's response to a \
             landed attack cone. {:?}",
            r.stats
        );
        assert!(
            r.stats.scripts_played > 0,
            "and default-script playback succeeded: {:?}",
            r.stats
        );
        // **The client does not touch the thing you hit.** The effect callback reads the attacker's
        // physical body; attack reporting and collision callbacks do not write to the target, and
        // damage is the server's.
        assert_eq!(
            r.target_before, r.target_after,
            "the target's state word and origin must be untouched by being attacked"
        );
        assert!(
            r.target_after.is_some(),
            "and it must still exist: nothing destroys it either"
        );
    }

    /// The same swing still hits the target, but clearing the
    /// attacker's scripted-collision bit prevents playback. Without this, a hit could always
    /// play the default script regardless of the state gate. The next test separately removes the target.
    #[test]
    fn an_attack_hook_that_lands_on_nothing_plays_no_script() {
        let r = swing(true, false);
        eprintln!("attack seam, landed but unscripted: {:?}", r.stats);
        assert!(
            r.stats.objects_hit > 0,
            "precondition: the cone still found the target"
        );
        assert!(
            r.emitters == 0,
            "the attacker has no SCRIPTED_COLLISION_PS, so the impact-effect gate skips playback and \
             nothing happens at all. This is the ordinary case for a player swinging a sword. \
             {} emitter(s), {:?}",
            r.emitters,
            r.stats
        );
        assert_eq!(r.stats.scripts_played, 0, "{:?}", r.stats);
        assert_eq!(
            r.stats.scripts_unplayed, 0,
            "a hit whose attacker lacks the bit is counted in neither: the receiver was not \
             reached, as opposed to reached and finding no table. {:?}",
            r.stats
        );
    }

    /// A swing into empty air, with the scripted-collision bit set, reports nothing and queues
    /// nothing: the cone does not hit whatever is nearest, and the branch every shipped cone takes
    /// does not panic.
    #[test]
    fn a_swing_at_empty_air_finds_nothing_and_queues_nothing() {
        let r = swing(false, true);
        eprintln!("attack seam, empty air: {:?}", r.stats);
        assert!(r.stats.hooks > 0, "the hook still fired: {:?}", r.stats);
        assert_eq!(
            r.stats.objects_hit, 0,
            "nothing was in front of the attacker: {:?}",
            r.stats
        );
        assert_eq!(
            r.emitters, 0,
            "and so nothing appeared on the attacker: {:?}",
            r.stats
        );
        assert_eq!(r.stats.scripts_played, 0);
        assert_eq!(r.target_before, None);
        assert_eq!(r.target_after, None);
    }
}
