//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! Every shipped animation hook converts, every shipped setup builds a part array, every emitter
//! builds and bursts reproducibly and consumes the documented number of random draws.
//! Fixture: the shipped retail DAT records and recorded inputs.

use super::common;

use std::collections::BTreeMap;

use dereth_animation::data::{AnimAssets, ParticleType};
use dereth_animation::particles::{draw_birth_params, EmitterContext, ParticleEmitter, NO_PART};
use dereth_animation::parts::PartArray;
use dereth_animation::seq::Sequence;
use dereth_dat::DbType;
use dereth_primitives::num::rng::Ran2;
use dereth_primitives::Frame;

/// Every hook in every shipped animation converts.
#[test]
fn every_hook_in_every_shipped_animation_converts() {
    let assets = common::open();
    let ids = assets.ids_of(DbType::Anim);
    assert!(!ids.is_empty(), "the fixture contains animations");

    let mut per_type: BTreeMap<u32, usize> = BTreeMap::new();
    let mut with_pos_frames = 0usize;
    let mut total_frames = 0usize;
    for id in ids {
        let a = assets.animation(id).expect("animation decodes");
        assert_eq!(
            a.part_frames.len(),
            usize::try_from(a.num_frames).expect("frames"),
            "{id}: frame count"
        );
        if let Some(pos) = &a.pos_frames {
            with_pos_frames += 1;
            assert_eq!(
                pos.len(),
                usize::try_from(a.num_frames).expect("frames"),
                "{id}: pos_frames count"
            );
        }
        total_frames += a.part_frames.len();
        for f in &a.part_frames {
            assert_eq!(
                f.frames.len(),
                usize::try_from(a.num_parts).expect("parts"),
                "{id}: a frame does not describe every part"
            );
            for h in &f.hooks {
                *per_type.entry(h.hook_type()).or_default() += 1;
                assert_ne!(
                    h.hook_type(),
                    4,
                    "{id}: a data-driven AnimationDone survived"
                );
                assert!(
                    h.hook_type() <= 26,
                    "{id}: unknown hook type {}",
                    h.hook_type()
                );
                // `-2` is the pre-unpack default and would mean a hook that never fires.
                assert!(
                    h.direction == 0 || h.direction == 1 || h.direction == -1,
                    "{id}: direction_ {}",
                    h.direction
                );
            }
        }
    }
    let total: usize = per_type.values().sum();
    eprintln!(
        "animation corpus: {total_frames} frames, {with_pos_frames} animations with root motion, \
         {total} hooks by type {per_type:?}"
    );
    assert!(total > 0, "the fixture exercises animation hooks");

    let mut tweaked = per_type.get(&21).copied().unwrap_or(0);
    let mut max_probability = 0.0_f32;
    let mut max_priority = 0.0_f32;
    let mut check = |h: &dereth_animation::hooks::AnimHook| {
        if let dereth_animation::hooks::HookKind::SoundTweaked(
            dereth_primitives::records::HookSoundTweaked {
                probability,
                priority,
                ..
            },
        ) = h.kind
        {
            assert!(
                (0.0..=1.0).contains(&probability),
                "each hook carries a probability"
            );
            assert!(priority.is_finite(), "each hook carries a finite priority");
            max_probability = max_probability.max(probability);
            max_priority = max_priority.max(priority);
        }
    };
    for id in assets.ids_of(DbType::Anim) {
        let a = assets.animation(id).expect("animation");
        for f in &a.part_frames {
            for h in &f.hooks {
                check(h);
            }
        }
    }
    for id in assets.ids_of(DbType::Setup) {
        let s = assets.setup(id).expect("setup");
        for p in s.placement_frames.values() {
            for h in &p.hooks {
                if h.hook_type() == 21 {
                    tweaked += 1;
                }
                check(h);
            }
        }
    }
    for id in assets.ids_of(DbType::PhysicsScript) {
        let s = assets.script(id).expect("script");
        for step in &s.steps {
            if step.hook.hook_type() == 21 {
                tweaked += 1;
            }
            check(&step.hook);
        }
    }
    eprintln!(
        "SoundTweaked: {tweaked} hooks, max probability {max_probability}, \
         max priority {max_priority}"
    );
    assert!(tweaked > 0, "the input exercises sound-adjustment hooks");
    assert!(max_probability <= 1.0, "the first float is a probability");
    assert!(
        max_priority > 1.0,
        "the second float is a priority and exceeds 1.0"
    );
}

/// Every shipped setup builds a part array.
#[test]
fn every_shipped_setup_builds_a_part_array() {
    let assets = common::open();
    let ids = assets.ids_of(DbType::Setup);
    assert!(!ids.is_empty(), "the fixture contains setups");
    let input_count = ids.len();

    let mut built = 0usize;
    let mut failed_init = 0usize;
    let mut with_placement = 0usize;
    let mut with_spheres = 0usize;
    for id in ids {
        let s = assets.setup(id).expect("setup decodes");
        let mut seq = Sequence::new();
        let Some(pa) = PartArray::create_setup(s.clone(), true, &mut seq, &assets) else {
            assert!(
                s.parts.is_empty() || matches!(id.0, 0x0200_1C4F | 0x0200_1C50),
                "{id}: create_setup failed with {} parts",
                s.parts.len()
            );
            failed_init += 1;
            continue;
        };
        built += 1;
        assert_eq!(pa.parts.len(), s.parts.len(), "{id}");
        assert_eq!(
            pa.radius(),
            s.radius,
            "{id}: scale 1 means the setup's own radius"
        );
        assert_eq!(pa.height(), s.height, "{id}");
        assert_eq!(pa.step_up_height(), s.step_up_height, "{id}");
        assert_eq!(pa.step_down_height(), s.step_down_height, "{id}");
        assert_eq!(pa.spheres().len(), s.spheres.len(), "{id}");
        assert_eq!(pa.cylspheres().len(), s.cylspheres.len(), "{id}");
        if !s.spheres.is_empty() {
            with_spheres += 1;
        }
        if seq.get_curr_animframe().is_some() {
            with_placement += 1;
        }
        if let Some(pi) = &s.parent_index {
            assert_eq!(pi.len(), s.parts.len(), "{id}");
        }
    }
    eprintln!(
        "setup corpus: {built} part arrays, {with_placement} with a placement frame, \
         {with_spheres} with collision spheres, {failed_init} refused by part creation"
    );
    assert!(built > 0, "the input builds part arrays");
    assert_eq!(
        built + failed_init,
        input_count,
        "every setup is accounted for"
    );
}

/// Every shipped emitter builds and bursts reproducibly.
#[test]
fn every_shipped_emitter_builds_and_bursts_reproducibly() {
    let assets = common::open();
    let ids = assets.ids_of(DbType::ParticleEmitter);
    assert!(!ids.is_empty(), "the fixture contains particle emitters");
    let input_count = ids.len();

    let ctx = EmitterContext {
        parent_frame: Frame::default(),
        part_frame: None,
        emitter_origin: dereth_primitives::Vec3::ZERO,
        now: 0.0,
        should_draw: true,
    };
    let mut built = 0usize;
    let mut per_type: BTreeMap<i32, usize> = BTreeMap::new();
    let mut no_mesh = 0usize;
    for id in ids {
        let i = assets.emitter_info(id).expect("emitter decodes");
        *per_type.entry(i.particle_type as i32).or_default() += 1;
        assert_ne!(
            i.particle_type,
            ParticleType::Unknown,
            "{id}: unknown ParticleType"
        );

        let mut rng = Ran2::new(1234);
        match ParticleEmitter::new(1, i.clone(), NO_PART, Frame::default(), &ctx, &mut rng) {
            None => {
                no_mesh += 1;
                assert_eq!(i.hw_gfxobj_id.0, 0, "{id}: creation failed with a mesh");
            }
            Some(e) => {
                built += 1;
                assert_eq!(
                    e.particles.len(),
                    usize::try_from(i.max_particles.max(0)).expect("slots"),
                    "{id}"
                );
                assert_eq!(
                    e.total_emitted,
                    i.initial_particles.min(i.max_particles.max(0)),
                    "{id}: the initial burst emits initial_particles, capped by the slot count"
                );
                // The same seed gives the same burst, draw for draw.
                let mut rng2 = Ran2::new(1234);
                let e2 =
                    ParticleEmitter::new(1, i.clone(), NO_PART, Frame::default(), &ctx, &mut rng2)
                        .expect("emitter");
                let a: Vec<_> = e
                    .live()
                    .map(|p| (p.a, p.b, p.c, p.offset, p.lifespan))
                    .collect();
                let b: Vec<_> = e2
                    .live()
                    .map(|p| (p.a, p.b, p.c, p.offset, p.lifespan))
                    .collect();
                assert_eq!(a, b, "{id}: a seeded burst is not reproducible");
            }
        }
    }
    eprintln!(
        "emitter corpus: {built} built, {no_mesh} with no hardware mesh, by type {per_type:?}"
    );
    assert!(built > 0, "the input builds emitters");
    assert_eq!(
        built + no_mesh,
        input_count,
        "every emitter is accounted for"
    );
}

/// Every emitter consumes the documented number of random draws.
#[test]
fn every_emitter_consumes_the_documented_number_of_random_draws() {
    let assets = common::open();
    let ids = assets.ids_of(DbType::ParticleEmitter);
    assert!(!ids.is_empty(), "the input contains particle emitters");
    for id in ids {
        let i = assets.emitter_info(id).expect("emitter");
        let mut a = Ran2::new(7);
        let _ = draw_birth_params(&i, &mut a);
        let mut b = Ran2::new(7);
        for _ in 0..5 {
            b.roll_f32(-1.0, 1.0); // lifespan, finalTrans, startTrans, finalScale, startScale
        }
        for _ in 0..3 {
            b.roll_f32(0.0, 1.0); // C, B, A
        }
        for _ in 0..3 {
            b.roll_f32(-1.0, 1.0); // the offset's z, y, x
        }
        // The fourth offset draw happens only on the non-degenerate path; accept either, but
        // require that it is one of exactly those two states.
        let after_a = a.next_f64();
        let mut b4 = b.clone();
        b4.roll_f32(0.0, 1.0);
        assert!(
            after_a == b.next_f64() || after_a == b4.next_f64(),
            "{id}: the emit path consumed an unexpected number of draws"
        );
    }
}
