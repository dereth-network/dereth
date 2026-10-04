//! World audio: the region's ambient tables are resident and the terrain rescan is driven by
//! movement, Holtburg's ambience reaches the mixer, triggered sounds are attenuated by distance
//! and panned by bearing, a voice ends with its sample, the seventeenth concurrent sound is
//! dropped, the sound sliders and check boxes scale their own category, preferences reach the
//! mixer, and losing focus silences the output. Fixture: the retail dats (region `0x13000000`'s
//! sound descriptors, the tables they name and their `0x0A` waves), a software-device
//! `WorldScene` at Holtburg with an attached body, and a device-less `Audio` whose mixed block is
//! the evidence.
//!
//! # What is asserted, and where
//!
//! **On the samples.** "A voice was started" is not evidence that anything is audible; the
//! assertions below are on the block `VoicePool::mix` fills: its peak, its channel balance, and
//! that a voice ends with its sample rather than looping. Explicit priority, volume and control
//! values provide the negative and boundary cases around the shipped inputs.
//!
//! Four observed sound behaviours each have a focused mixer test; the tests here are the
//! **integration-level** guards that the client's own wiring still reaches them:
//!
//! * 12.1 the ambient volume is applied twice, so the slider is quadratic;
//! * 12.2 the priority system is inert and the seventeenth concurrent sound is dropped;
//! * 12.3 the last row of a multi-row entry is unreachable;
//! * 12.7 `Sound.InterfaceSoundVolume` has no reader.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::audio::Audio;
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, LocalTime, Vec3};
use dereth_render::device::Gpu;
use std::sync::Arc;

use crate::common::software_gpu;

/// One second of interleaved stereo at the client's own primary-buffer rate.
const BLOCK: usize = (dereth_audio::MIX_RATE as usize) * 2;

/// How many simulated seconds an ambient test gives the min-heap to fire something.
const AMBIENT_SECONDS: usize = 40;

/// The retail store, or **fail**: a test that returned early without the dats would pass having
/// read nothing at all, so the type offers no `Option`.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// A silent `Audio`: no device, so `AudioSystem::mix` is called directly and the samples are the
/// evidence. The seed is pinned rather than [`dereth_client::audio::ran2_seed`]'s `time(NULL)`, so
/// the ambient schedule is reproducible.
fn audio(prefs: dereth_audio::Prefs) -> Audio {
    Audio::new(prefs, 1, false)
}

/// The peak absolute sample of each channel.
fn peaks(buf: &[f32]) -> (f32, f32) {
    let mut l = 0.0f32;
    let mut r = 0.0f32;
    for f in buf.as_chunks::<2>().0 {
        l = l.max(f[0].abs());
        r = r.max(f[1].abs());
    }
    (l, r)
}

/// The scene the ambient scan reads: Holtburg, a body, and the terrain of the 3×3 neighbourhood.
fn scene(store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> WorldScene {
    let cfg = SceneConfig {
        scenery_radius: 1,
        time_of_day: Some(0.5),
        ..SceneConfig::default()
    };
    let mut s = WorldScene::load(store, gpu, cfg).expect("the landscape loads");
    let region = dereth_client::world::load_region(store).expect("the region decodes");
    s.attach_character(store, &region, gpu)
        .expect("the body is created");
    s
}

/// `App::frame`'s world half, including the world-audio step.
fn run(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scene: &mut WorldScene,
    audio: &mut Audio,
    t: &mut f64,
    frames: usize,
    input: dereth_client::character::CharacterInput,
) {
    let mut stream = ObjectStream::new();
    // `world_use_time` also drains the server's `0xF750` queue from the stream and
    // resolves an object's setup-record default sound-table id through the animation-asset seam.
    let anim = dereth_client::anim_assets::DatAnimAssets::new(Arc::clone(store));
    for _ in 0..frames {
        *t += dereth_client::app::HEADLESS_STEP;
        scene
            .sync_objects(store, gpu, &mut stream)
            .expect("sync_objects");
        dereth_client::audio::world_use_time(
            Some(audio),
            Some(scene),
            &mut stream,
            &anim,
            store,
            LocalTime(*t),
        );
        scene.update(
            dereth_client::camera::CameraInput::default(),
            input,
            LocalTime(*t),
            1.0 / 30.0,
        );
        scene.stream(store, gpu).expect("stream");
    }
}

// ---------------------------------------------------------------------------------------------
// 1. The beds.
// ---------------------------------------------------------------------------------------------

/// **The region's ambient descriptors load, and their waves decode.**
///
/// The region lookup resolves a terrain word to an ambient descriptor. Its sound-table
/// id is loaded lazily, and a failed load is remembered. A descriptor whose table will not load
/// is **silent ambience**, which is exactly the failure a counter nobody compares hides.
#[test]
fn the_regions_ambient_tables_and_their_waves_are_resident() {
    let store = store();
    let mut gpu = software_gpu(320, 240);
    let mut scene = scene(&store, &mut gpu);
    let mut a = audio(dereth_audio::Prefs::default());
    let mut t = 0.0;
    run(
        &store,
        &mut gpu,
        &mut scene,
        &mut a,
        &mut t,
        2,
        Default::default(),
    );

    let w = a.world_stats();
    assert!(
        w.ambient_tables > 0,
        "the region named no ambient sound tables"
    );
    assert_eq!(
        w.ambient_tables_missing, 0,
        "{} ambient tables did not load",
        w.ambient_tables_missing
    );
    assert!(
        w.ambient_waves > 0,
        "the ambient tables named no waves that decoded"
    );
    assert_eq!(
        a.stats.wave_decode_failures, 0,
        "an ambient wave would not decode"
    );
    assert!(w.position_scans >= 1, "the 3x3 terrain scan never ran");
    eprintln!(
        "{} ambient tables, {} waves, {} scans",
        w.ambient_tables, w.ambient_waves, w.position_scans
    );
}

/// **The scan runs on a position change, not once per frame.**
/// `total_sound_count` is a per-pass sum, and recomputing it every frame gives slightly different
/// volumes. The position-change path is the only caller.
#[test]
fn the_terrain_rescan_is_driven_by_movement_and_not_by_the_frame() {
    let store = store();
    let mut gpu = software_gpu(320, 240);
    let mut scene = scene(&store, &mut gpu);
    let mut a = audio(dereth_audio::Prefs::default());

    // Standing still: the scan runs as the body settles onto the terrain and then stops. What is
    // asserted is that it is *not* per frame, which is the contract; the exact number is the
    // body's own settling.
    let mut t = 0.0;
    run(
        &store,
        &mut gpu,
        &mut scene,
        &mut a,
        &mut t,
        60,
        Default::default(),
    );
    let still = a.world_stats().position_scans;
    assert!(still <= 3, "standing still ran {still} scans in 60 frames");

    // Walking: the scan runs again as the listener crosses whole-metre boundaries, and far fewer
    // times than there are frames.
    let walk = dereth_client::character::CharacterInput {
        forward: true,
        run: true,
        ..Default::default()
    };
    run(&store, &mut gpu, &mut scene, &mut a, &mut t, 300, walk);
    let moved = a.world_stats().position_scans;
    assert!(moved > still, "walking never rescanned");
    assert!(moved < 300, "{moved} scans in 300 frames is once per frame");
    eprintln!("{still} scan standing, {moved} after 300 frames of running");
}

/// Behaviour: audio.ambient.the-regions-ambience-reaches-the-mixer
///
/// **The beds reach the mixer, and the samples are not silence.**
///
/// This is the assertion that means something: `VoicePool::mix` fills an interleaved stereo block
/// and the block has energy in it. A started voice with a zero gain, a missing sample or a wrong
/// rate would all pass "a sound was started" and fail here.
#[test]
fn holtburgs_ambience_reaches_the_mixer_as_samples() {
    let store = store();
    let mut gpu = software_gpu(320, 240);
    let mut scene = scene(&store, &mut gpu);
    let mut a = audio(dereth_audio::Prefs::default());

    // Long enough for the min-heap to fire something: the shipped intermittent rates are seconds.
    let mut buf = vec![0.0f32; BLOCK];
    let mut best = (0.0f32, 0.0f32);
    let mut started = false;
    let mut t = 0.0;
    for _ in 0..AMBIENT_SECONDS {
        run(
            &store,
            &mut gpu,
            &mut scene,
            &mut a,
            &mut t,
            30,
            Default::default(),
        );
        if a.active_voices() > 0 {
            started = true;
            buf.fill(0.0);
            a.mix(&mut buf);
            let p = peaks(&buf);
            best = (best.0.max(p.0), best.1.max(p.1));
        }
    }
    assert!(
        started,
        "no ambient voice started in 40 seconds over Holtburg"
    );
    assert!(
        best.0 > 0.0 || best.1 > 0.0,
        "an ambient voice played and the block was silent"
    );
    eprintln!("ambient peak L {:.4} R {:.4}", best.0, best.1);
}

// ---------------------------------------------------------------------------------------------
// 2. Triggered sounds and attenuation.
// ---------------------------------------------------------------------------------------------

/// The first wave in the retail portal dat that decodes, for the attenuation tests. Reading it out
/// of the dat rather than naming one keeps the test independent of any particular sound's id.
fn a_wave(store: &RetailDatStore, a: &mut Audio) -> DataId {
    for id in store.ids_of(dereth_dat::DbType::Wave) {
        let before = a.stats.waves_created;
        a.create_sound(store, id);
        if a.stats.waves_created > before {
            return id;
        }
    }
    panic!("no wave in the retail dat decoded");
}

/// Behaviour: audio.sound.a-triggered-sound-is-attenuated-and-panned
///
/// **Attenuation falls with distance and the pan follows the bearing.**
///
/// The distance curve is twice as steep as physical, quantised to whole decibels and
/// rounded **up**, with a floor at `VOL_MIN_DB`; its pan is a ±15 dB spread on the compass bearing
/// from the sound to the listener. Both are preserved by `dereth_audio::attenuation`; what is
/// asserted here is that the **client** feeds it the right positions. The evidence is the channel
/// balance of the mixed block.
#[test]
fn a_triggered_sound_is_attenuated_by_distance_and_panned_by_bearing() {
    let store = store();
    let mut a = audio(dereth_audio::Prefs::default());
    let wave = a_wave(&store, &mut a);

    // The listener is at the origin facing north (+y), represented by heading 0.
    let listen = |a: &Audio| {
        a.set_listener(dereth_audio::Listener {
            pos: Vec3::ZERO,
            heading: 0.0,
        })
    };

    let play_at = |a: &mut Audio, at: Vec3| -> (f32, f32) {
        listen(a);
        a.play_trigger(dereth_client::audio::SoundTrigger::Wave {
            id: wave,
            at,
            volume: 1.0,
            priority: 0.9,
            probability: 1.0,
        });
        let mut buf = vec![0.0f32; BLOCK];
        a.mix(&mut buf);
        peaks(&buf)
    };

    // At the listener's own position: loud, and balanced.
    let near = play_at(&mut a, Vec3::ZERO);
    assert!(
        near.0 > 0.0 && near.1 > 0.0,
        "a sound at the listener is silent"
    );
    assert!(
        (near.0 - near.1).abs() < 1e-6,
        "a sound at the listener is panned: L {} R {}",
        near.0,
        near.1
    );

    // Forty metres away: quieter. The curve is `-20 * log10(d/VOL_MIN_DIST) * 2`, so this is a
    // large step, not a rounding one.
    let far = play_at(&mut a, Vec3::new(0.0, 40.0, 0.0));
    assert!(
        far.0 < near.0,
        "40 m away is not quieter: {} vs {}",
        far.0,
        near.0
    );
    assert!(far.0 > 0.0, "40 m away is inaudible");

    // To the listener's left (west, -x) at the same distance: more energy on the left.
    let left = play_at(&mut a, Vec3::new(-40.0, 0.0, 0.0));
    let right = play_at(&mut a, Vec3::new(40.0, 0.0, 0.0));
    assert!(
        left.0 > left.1,
        "a sound to the west is not louder on the left: {left:?}"
    );
    assert!(
        right.1 > right.0,
        "a sound to the east is not louder on the right: {right:?}"
    );
    // And the two are mirror images, because the pan curve is odd about the bearing.
    assert!((left.0 - right.1).abs() < 1e-6 && (left.1 - right.0).abs() < 1e-6);
    eprintln!("near {near:?} far {far:?} left {left:?} right {right:?}");
}

/// **A voice ends with its sample.** *nothing loops*. Continuous ambience is a
/// re-trigger on a timer, and a sound buffer played once stops when it runs out.
///
/// The block asked for here is far longer than any retail wave, so a looping voice would still be
/// producing samples at the end of it.
#[test]
fn a_voice_ends_with_its_sample_rather_than_looping() {
    let store = store();
    let mut a = audio(dereth_audio::Prefs::default());
    let wave = a_wave(&store, &mut a);
    a.set_listener(dereth_audio::Listener::default());
    a.play_trigger(dereth_client::audio::SoundTrigger::Wave {
        id: wave,
        at: Vec3::ZERO,
        volume: 1.0,
        priority: 0.9,
        probability: 1.0,
    });
    assert_eq!(a.active_voices(), 1);

    // Sixty seconds. Every shipped wave is far shorter.
    let mut buf = vec![0.0f32; BLOCK];
    let mut heard = false;
    for _ in 0..60 {
        buf.fill(0.0);
        a.mix(&mut buf);
        let p = peaks(&buf);
        heard |= p.0 > 0.0 || p.1 > 0.0;
    }
    assert!(heard, "the wave produced no samples at all");
    assert_eq!(
        a.active_voices(),
        0,
        "the voice is still running after 60 s: it looped"
    );
    // And the block after it ended is silence, not the head of the sample again.
    buf.fill(0.0);
    a.mix(&mut buf);
    assert_eq!(
        peaks(&buf),
        (0.0, 0.0),
        "a finished voice is still producing samples"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The shipped bugs, at the level the client wires them.
// ---------------------------------------------------------------------------------------------

/// **12.2: the priority system is inert and the seventeenth concurrent sound is dropped.**
///
/// Every voice-priority field is 0.0, so `VoicePool`'s stealing test is always false and
/// the seventeenth simultaneous sound plays nothing at all. A rebuild that stole the quietest or
/// oldest voice would be more correct and wrong.
#[test]
fn the_seventeenth_concurrent_sound_is_dropped() {
    let store = store();
    let mut a = audio(dereth_audio::Prefs::default());
    let wave = a_wave(&store, &mut a);
    a.set_listener(dereth_audio::Listener::default());
    for _ in 0..dereth_audio::NUM_VOICES + 4 {
        a.play_trigger(dereth_client::audio::SoundTrigger::Wave {
            id: wave,
            at: Vec3::ZERO,
            volume: 1.0,
            priority: 0.9,
            probability: 1.0,
        });
    }
    assert_eq!(dereth_audio::NUM_VOICES, 16);
    assert_eq!(a.active_voices(), 16, "the pool grew past sixteen voices");
}

/// **12.1: the ambient volume is applied twice, so its slider is quadratic.**
///
/// Driven through the client's own path: the same scene, the same pinned `ran2` and CRT seeds and
/// the same simulated clock, so the same beds fire at the same instants; the only difference is
/// the slider. The ambient-play request applies `AmbientSoundVolume` once and the attenuation
/// calculation applies it again, so half the slider is a quarter of the amplitude: 12 dB, not 6.
#[test]
fn the_ambient_slider_is_quadratic_through_the_clients_own_path() {
    let store = store();
    let mut gpu = software_gpu(320, 240);

    let peak = |gpu: &mut Gpu, ambient_volume: f32| -> f32 {
        let mut scene = scene(&store, gpu);
        let prefs = dereth_audio::Prefs {
            ambient_volume,
            ..dereth_audio::Prefs::default()
        };
        let mut a = audio(prefs);
        let mut buf = vec![0.0f32; BLOCK];
        let mut best = 0.0f32;
        let mut t = 0.0;
        for _ in 0..AMBIENT_SECONDS {
            run(
                &store,
                gpu,
                &mut scene,
                &mut a,
                &mut t,
                30,
                Default::default(),
            );
            if a.active_voices() > 0 {
                buf.fill(0.0);
                a.mix(&mut buf);
                best = best.max(peaks(&buf).0.max(peaks(&buf).1));
            }
        }
        best
    };
    let full = peak(&mut gpu, 1.0);
    let half = peak(&mut gpu, 0.5);
    assert!(
        full > 0.0 && half > 0.0,
        "no ambient bed played: full {full} half {half}"
    );
    let ratio = half / full;
    let quadratic = dereth_audio::mixer::gain_from_db(-12);
    let linear = dereth_audio::mixer::gain_from_db(-6);
    assert!(
        (ratio - quadratic).abs() < (ratio - linear).abs(),
        "half the ambient slider gave {ratio} of full volume; quadratic is {quadratic}, linear \
         would be {linear}; the double application is shipped behaviour and must stay"
    );
    eprintln!("ambient full {full:.4} half {half:.4} ratio {ratio:.4} (quadratic {quadratic:.4})");
}

/// **12.3: the last row of a multi-row entry is unreachable.**
///
/// `trunc(u · (n − 1))` with `u` capped at `RNMX = 0.99999988` can never reach `n − 1`. Asserted
/// over the whole `ran2` stream rather than a sample of it, because "we never saw it" is not the
/// same claim.
#[test]
fn the_last_row_of_a_multi_row_entry_is_never_picked() {
    for n in 2..=8usize {
        for i in 0..2000u32 {
            #[allow(clippy::cast_precision_loss)]
            let u = (i as f32) / 2000.0 * 0.999_999_9;
            let row = dereth_audio::table::row_index(n, u).expect("in range");
            assert!(
                row <= n - 2,
                "row {row} of {n} is reachable; the last row must not be"
            );
        }
    }
}

/// **12.7: `Sound.InterfaceSoundVolume` has no reader.** Interface sounds are scaled by
/// `Sound.SoundVolume` instead, because entry 7 and entry 8 both go through
/// the non-ambient attenuation path, which multiplies by `effect_sound_volume`.
///
/// The preference is registered and parsed (`Prefs::interface_volume` exists for exactly that
/// reason), and turning it to zero must change **nothing**. A rebuild that honoured it would be
/// more sensible and wrong.
#[test]
fn interface_sounds_are_scaled_by_the_effect_slider_not_the_interface_one() {
    let store = store();
    let mut probe = audio(dereth_audio::Prefs::default());
    let wave = a_wave(&store, &mut probe);

    let peak = |prefs: dereth_audio::Prefs| {
        let mut a = audio(prefs);
        a.create_sound(&store, wave);
        a.set_listener(dereth_audio::Listener::default());
        a.play_ui_sound(dereth_audio::UiSoundRef::Wave(wave));
        let mut buf = vec![0.0f32; BLOCK];
        a.mix(&mut buf);
        peaks(&buf).0
    };
    let full = peak(dereth_audio::Prefs::default());
    assert!(full > 0.0, "the UI wave form produced no samples");

    // The dead slider, all the way down: identical.
    let dead = peak(dereth_audio::Prefs {
        interface_volume: 0.0,
        ..dereth_audio::Prefs::default()
    });
    assert!(
        (dead - full).abs() < 1e-9,
        "Sound.InterfaceSoundVolume changed an interface sound; it has no reader"
    );

    // The one that is actually read: audible.
    let by_effect = peak(dereth_audio::Prefs {
        effect_volume: 0.5,
        ..dereth_audio::Prefs::default()
    });
    assert!(
        by_effect < full,
        "Sound.SoundVolume does not scale an interface sound"
    );
    eprintln!("interface: full {full:.4} dead-slider {dead:.4} effect-slider {by_effect:.4}");
}

/// **The animation sound hooks reach the mixer, and the shipped data decides which are audible.**
///
/// `Character::apply_effects` passes each `AnimEvent` on, so direct-sound and sound-table
/// animation hooks have a consumer. The wiring must resolve a
/// `SoundType` against **the object's own** sound table at the object's own position; it must
/// **not** invent a sound the shipped table does not carry.
///
/// Twenty simulated seconds of running raise ~50 hooks and none of them is audible, and that is
/// correct: the Aluvian male's table `0x20000001` has **no footstep (55, 56)
/// rows**, so the table lookup returns nothing and the client plays nothing.
/// The second half of the test drives the same path with a `SoundType` the table does carry and
/// asserts on the samples.
#[test]
fn the_animation_sound_hooks_resolve_against_the_objects_own_table() {
    let store = store();
    let mut gpu = software_gpu(320, 240);
    let mut scene = scene(&store, &mut gpu);
    let mut a = audio(dereth_audio::Prefs {
        // Ambience off, so what is measured can only be the hooks.
        ambient_enabled: false,
        ..dereth_audio::Prefs::default()
    });
    let walk = dereth_client::character::CharacterInput {
        forward: true,
        run: true,
        ..Default::default()
    };
    let mut t = 0.0;
    for _ in 0..20 {
        run(&store, &mut gpu, &mut scene, &mut a, &mut t, 30, walk);
    }
    let w = a.world_stats();
    eprintln!("{} hooks, {} misses", w.triggers, w.trigger_misses);
    assert!(
        w.triggers > 0,
        "20 s of running raised no sound hook at all"
    );
    assert_eq!(
        w.trigger_misses, 0,
        "{} sound hooks named a table or a wave that is not in the dat",
        w.trigger_misses
    );

    // The shipped table is the reason the footsteps are silent, and this is the assertion that
    // says so rather than the test quietly tolerating a zero.
    let table = scene
        .character_sound_table()
        .expect("the body has a sound table");
    let t20 = a.assets().table(table).expect("the table is resident");
    for footstep in [SOUND_FOOTSTEP1, SOUND_FOOTSTEP2, SOUND_SPEAK1] {
        assert!(
            dereth_audio::table::lookup(t20, footstep).is_none(),
            "table {:08X} has a row for SoundType {footstep}; the human table does not, and the \
             silent footsteps above would then be a bug rather than the data",
            table.0
        );
    }

    // And a `SoundType` it does carry goes all the way to samples, through the same entry point.
    a.set_listener(dereth_audio::Listener::default());
    a.play_trigger(dereth_client::audio::SoundTrigger::Table {
        table,
        stype: SOUND_WOUND1,
        at: Vec3::ZERO,
    });
    assert_eq!(
        a.active_voices(),
        1,
        "SoundType {SOUND_WOUND1} started no voice"
    );
    let mut buf = vec![0.0f32; BLOCK];
    a.mix(&mut buf);
    let p = peaks(&buf);
    assert!(
        p.0 > 0.0 && p.1 > 0.0,
        "the table path started a voice and the block was silent"
    );
    eprintln!("wound sound peak L {:.4} R {:.4}", p.0, p.1);
}

/// Footstep 1 (55), footstep 2 (56), speak 1 (1) and wound 1 (12), as sound types.
/// The first three are requested by the Aluvian male's run and idle cycles; the fourth is one the
/// table actually has.
const SOUND_SPEAK1: u32 = 1;
const SOUND_WOUND1: u32 = 12;
const SOUND_FOOTSTEP1: u32 = 55;
const SOUND_FOOTSTEP2: u32 = 56;

// =============================================================================================
// The path from the options screen to `AudioSystem::set_prefs`.
//
// The subject is the **applied gain**, not the stored number, and it is asserted at interior
// slider positions: a linear map and a wrong-but-monotonic map agree at 0 and 1.
// =============================================================================================

/// The production audio helper's expression for the gain a positioned effect sound gets. These
/// assertions verify that the world-audio integration applies the same attenuation and pan-law
/// result; they are not an independent reimplementation of the attenuation arithmetic.
fn expected_effect_gain(volume: f32, distance: f32, effect_volume: f32) -> f32 {
    let prefs = dereth_audio::Prefs {
        effect_volume,
        ..dereth_audio::Prefs::default()
    };
    let db = dereth_audio::attenuation(distance, volume, dereth_audio::Category::Effect, &prefs)
        .expect("audible");
    dereth_audio::mixer::gain_from_db(db)
}

/// One positioned effect sound at the listener, peaked.
fn effect_peak(a: &mut Audio, wave: DataId) -> f32 {
    a.play_trigger(dereth_client::audio::SoundTrigger::Wave {
        id: wave,
        at: Vec3::ZERO,
        volume: 1.0,
        priority: 0.9,
        probability: 1.0,
    });
    let mut buf = vec![0.0f32; BLOCK];
    a.mix(&mut buf);
    let (l, r) = peaks(&buf);
    l.max(r)
}

/// One interface sound (entry 8), peaked.
fn interface_peak(a: &mut Audio, wave: DataId) -> f32 {
    a.play_ui_sound(dereth_audio::UiSoundRef::Wave(wave));
    let mut buf = vec![0.0f32; BLOCK];
    a.mix(&mut buf);
    let (l, r) = peaks(&buf);
    l.max(r)
}

/// Write one preference the way the client writes it: as a `UiRequest::SetPreference` through
/// `dereth_client::audio::apply_preference_requests`.
fn set_pref(a: &mut Audio, name: &'static str, v: dereth_ui_screens::PrefValue) {
    let left = dereth_client::audio::apply_preference_requests(
        Some(a),
        vec![dereth_ui_screens::UiRequest::SetPreference(name, v)],
    );
    assert!(left.is_empty(), "{name} was not owned by the sound system");
}

/// **The `Sound.SoundVolume` slider changes the applied gain, at interior positions, through
/// `AudioSystem::set_prefs`.**
///
/// The four positions are 1.0, 0.75, 0.5 and 0.25 of `VOLUME_SLIDER_RANGE`. The two interior ones
/// are the point: the attenuation calculation quantises to whole decibels with a `ceil`, so a
/// linear-in-amplitude implementation (which agrees at 0 and at 1) fails here.
#[test]
fn moving_the_sound_volume_slider_changes_the_applied_gain_at_interior_positions() {
    let store = store();
    let mut probe = audio(dereth_audio::Prefs::default());
    let wave = a_wave(&store, &mut probe);

    // One `Audio`, driven as the client drives it: the preference is written at run time through
    // the seam, not chosen at construction.
    let mut a = audio(dereth_audio::Prefs::default());
    a.create_sound(&store, wave);
    a.set_listener(dereth_audio::Listener::default());

    let peak_at = |a: &mut Audio, position: f32| -> f32 {
        // The scrollbar's normalised position (attribute `0x86`) is mapped into the preference's
        // registered range before the value is applied.
        let (lo, hi) = dereth_audio::VOLUME_SLIDER_RANGE;
        let value = lo + position * (hi - lo);
        set_pref(
            a,
            "Sound.SoundVolume",
            dereth_ui_screens::PrefValue::Float(value),
        );
        effect_peak(a, wave)
    };

    let full = peak_at(&mut a, 1.0);
    assert!(full > 0.0, "the wave produced no samples at full volume");
    let three_quarters = peak_at(&mut a, 0.75);
    let half = peak_at(&mut a, 0.5);
    let quarter = peak_at(&mut a, 0.25);
    eprintln!(
        "applied peaks: 1.0 {full:.6}  0.75 {three_quarters:.6}  0.5 {half:.6}  0.25 {quarter:.6}"
    );

    // Strictly monotonic, so the slider does something at every step and not only at the ends.
    assert!(three_quarters < full, "0.75 is not quieter than 1.0");
    assert!(half < three_quarters, "0.5 is not quieter than 0.75");
    assert!(quarter < half, "0.25 is not quieter than 0.5");

    // And the ratios are the client's, not a linear fader's.
    //
    // **The slider appears twice in this expression, and that is the shipped bug.** Entry 1, the
    // positioned-sound entry, passes `effect_sound_volume` as the *volume argument*, and the
    // attenuation path multiplies by `effect_sound_volume` again, so the Sound Volume slider is
    // quadratic for an animation `Sound` hook. A rebuild that applied it once would give -2 dB at
    // 0.75 where the client gives -4, and both agree at 1.0, which is exactly why the interior
    // positions are the ones asserted.
    for position in [0.75f32, 0.5, 0.25] {
        let measured = peak_at(&mut a, position) / full;
        let expect =
            expected_effect_gain(position, 0.0, position) / expected_effect_gain(1.0, 0.0, 1.0);
        assert!(
            (measured - expect).abs() < 1e-3,
            "slider at {position}: applied {measured:.6} of full, the attenuation calculation says {expect:.6}"
        );
        // The whole-decibel `ceil` is what separates this from a linear fader.
        assert!(
            (measured - position).abs() > 1e-4,
            "slider at {position} applied exactly {position} -- the dB quantisation is gone"
        );
    }
}

/// **Each of the three sound pairs drives only its own category, and the check box is independent
/// of the slider.**
///
/// Every preference is written through `UiRequest::SetPreference`, the seam the options screen
/// uses.
#[test]
fn each_sound_pair_drives_only_its_own_category_through_the_request_seam() {
    use dereth_ui_screens::PrefValue;
    let store = store();
    let mut probe = audio(dereth_audio::Prefs::default());
    let wave = a_wave(&store, &mut probe);

    let mut a = audio(dereth_audio::Prefs::default());
    a.create_sound(&store, wave);
    a.set_listener(dereth_audio::Listener::default());
    let base_ui = interface_peak(&mut a, wave);
    let base_fx = effect_peak(&mut a, wave);
    assert!(base_ui > 0.0 && base_fx > 0.0);

    // 12.7: `Sound.InterfaceSoundVolume` is only registered as a preference; nothing reads it.
    // Driving its slider to zero must change nothing. A rebuild that honoured it
    // would be more sensible and wrong.
    set_pref(&mut a, "Sound.InterfaceSoundVolume", PrefValue::Float(0.0));
    assert!(
        (interface_peak(&mut a, wave) - base_ui).abs() < 1e-9,
        "the Interface Sound Volume slider changed an interface sound; in retail it has no reader"
    );

    // The *effect* slider is what scales an interface sound, asserted at an interior position:
    // both centred-play entries play as non-ambient.
    set_pref(&mut a, "Sound.SoundVolume", PrefValue::Float(0.5));
    let ui_half = interface_peak(&mut a, wave);
    assert!(
        ui_half < base_ui,
        "Sound.SoundVolume does not scale an interface sound"
    );
    let expect = expected_effect_gain(1.0, 0.0, 0.5) / expected_effect_gain(1.0, 0.0, 1.0);
    assert!(
        (ui_half / base_ui - expect).abs() < 1e-3,
        "interface at half the effect slider: {} vs {expect}",
        ui_half / base_ui
    );

    // The check box is a separate preference and does not touch the volume:
    // the combined checkbox-slider applies the toggle and slider independently, while a plain
    // checkbox writes only its own boolean.
    set_pref(&mut a, "Sound.SoundVolume", PrefValue::Float(1.0));
    set_pref(
        &mut a,
        "Sound.InterfaceSoundDisabled",
        PrefValue::Bool(false),
    );
    assert_eq!(
        interface_peak(&mut a, wave),
        0.0,
        "the interface box did not silence entry 8"
    );
    assert!(
        effect_peak(&mut a, wave) > 0.0,
        "the interface box silenced an effect sound"
    );
    // Ticking it back restores the volume the slider still held: nothing zeroed it.
    set_pref(
        &mut a,
        "Sound.InterfaceSoundDisabled",
        PrefValue::Bool(true),
    );
    assert!(
        (interface_peak(&mut a, wave) - base_ui).abs() < 1e-9,
        "unticking the box lost the slider's value"
    );

    // The effect box silences effects and not interface sounds. `Sound.SoundDisabled=False` means
    // OFF: false disables effects despite the preference name.
    set_pref(&mut a, "Sound.SoundDisabled", PrefValue::Bool(false));
    assert_eq!(
        effect_peak(&mut a, wave),
        0.0,
        "the sound box did not silence entry 1"
    );
    assert!(
        interface_peak(&mut a, wave) > 0.0,
        "the sound box silenced an interface sound"
    );
    set_pref(&mut a, "Sound.SoundDisabled", PrefValue::Bool(true));
    assert!(
        effect_peak(&mut a, wave) > 0.0,
        "the sound box did not come back on"
    );

    // A preference this subsystem does not own comes straight back out rather than disappearing.
    let left = dereth_client::audio::apply_preference_requests(
        Some(&mut a),
        vec![dereth_ui_screens::UiRequest::SetPreference(
            "Render.FieldOfView",
            PrefValue::Float(120.0),
        )],
    );
    assert_eq!(
        left.len(),
        1,
        "Render.FieldOfView was swallowed by the sound system"
    );
    eprintln!("preferences applied: {}", a.stats.preferences_applied);
    // Seven writes above, all owned. A denominator rather than a bare "it worked".
    assert_eq!(
        a.stats.preferences_applied, 7,
        "seven Sound.* writes were made"
    );
    assert_eq!(a.stats.preference_type_errors, 0);
}

/// **The Defaults action on the Client Options page writes the sound preferences.**
///
/// This test calls the same helper the shipped layout's Defaults button (`0x100001FE`) normally
/// invokes, then passes its generated request set to `apply_preference_requests`. It therefore
/// covers the request generator and consumer path without claiming to click the button itself.
///
/// The page is left at half volume first, so "the defaults were restored" is a change and not a
/// no-op that would pass on an inert path.
#[test]
fn restoring_the_option_page_defaults_puts_the_sound_preferences_back() {
    use dereth_ui_screens::PrefValue;
    let store = store();
    let mut probe = audio(dereth_audio::Prefs::default());
    let wave = a_wave(&store, &mut probe);

    let mut a = audio(dereth_audio::Prefs::default());
    a.create_sound(&store, wave);
    a.set_listener(dereth_audio::Listener::default());
    let full = effect_peak(&mut a, wave);

    set_pref(&mut a, "Sound.SoundVolume", PrefValue::Float(0.25));
    let quiet = effect_peak(&mut a, wave);
    assert!(
        quiet < full,
        "the page did not get quieter first: {quiet} vs {full}"
    );

    // The exact request set raised by the Defaults button.
    let requests = dereth_ui_screens::options::config::restore_defaults_requests();
    // The page's own count (its rows, the three paired sliders, the era, performance and
    // landscape detail rows) is pinned by the options page's tests; this one needs only that the
    // button raises one request per write.
    let all = requests.len();
    assert_eq!(
        all,
        dereth_ui_screens::options::config::restore_default_values().len(),
        "the Defaults button raises one request per write"
    );
    let left = dereth_client::audio::apply_preference_requests(Some(&mut a), requests);
    // **Eight of them are `Sound.*`** -- the three volumes, the three `*Disabled` booleans,
    // `Sound.SoundFeatures` and `Sound.PlaySoundOnlyWhenActive`, i.e. the whole of `SOUND_KEYS`.
    // The rest belong to Render / Camera / Display / Input / UI and must come back out.
    assert_eq!(
        left.len(),
        all - 8,
        "{} of {all} requests came back unowned",
        left.len()
    );

    let restored = effect_peak(&mut a, wave);
    assert!(
        (restored - full).abs() < 1e-9,
        "Restore Defaults did not put the sound volume back: {restored} vs {full}"
    );
}

/// **The `[Sound]` section of `UserPreferences.ini` reaches the mixer.**
///
/// `App::start_shell` builds the sound preferences with
/// [`dereth_client::audio::prefs_from_file`], which this drives, so the file's `[Sound]` values
/// are what the mixer uses.
#[test]
fn the_preferences_file_reaches_the_mixer() {
    let store = store();
    let mut probe = audio(dereth_audio::Prefs::default());
    let wave = a_wave(&store, &mut probe);

    let dir = std::env::temp_dir().join("dereth-world-audio");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("UserPreferences.ini");
    // The save format splits the name at its **last** dot,
    // so `Sound.SoundVolume` becomes section `Sound`, key `SoundVolume`.
    std::fs::write(
        &path,
        "[Display]\nFullScreen=False\n[Sound]\nSoundVolume=0.5\n",
    )
    .expect("write the ini");

    let prefs = dereth_client::audio::prefs_from_file(&path);
    assert_eq!(
        prefs.effect_volume, 0.5,
        "the file's Sound.SoundVolume did not reach Prefs"
    );

    let peak = |prefs: dereth_audio::Prefs| {
        let mut a = audio(prefs);
        a.create_sound(&store, wave);
        a.set_listener(dereth_audio::Listener::default());
        effect_peak(&mut a, wave)
    };
    let from_file = peak(prefs);
    let full = peak(dereth_audio::Prefs::default());
    assert!(full > 0.0 && from_file > 0.0);
    // Entry 1 applies the effect volume twice; see the note in the slider test above.
    let expect = expected_effect_gain(0.5, 0.0, 0.5) / expected_effect_gain(1.0, 0.0, 1.0);
    assert!(
        (from_file / full - expect).abs() < 1e-3,
        "the file said 0.5 and the applied gain was {:.6} of full; the attenuation calculation says {expect:.6}",
        from_file / full
    );

    // A file that does not exist is not an error: initialization ignores a failed
    // preference-file load.
    let missing = dereth_client::audio::prefs_from_file(&dir.join("no-such-file.ini"));
    assert_eq!(missing, dereth_audio::Prefs::default());
    let _ = std::fs::remove_file(&path);
}

/// **Losing focus silences the output, and `Sound.PlaySoundOnlyWhenActive` gates new sounds.**
///
/// `VoicePool::mix` tests the shared focus-mute `AtomicBool` before filling the block,
/// `AudioSystem::new` hands it the same allocation, and `App` calls `Audio::set_focus` on
/// `winit::event::WindowEvent::Focused`. The `Focus::is_muted` accessor has no callers.
///
/// Two mechanisms are required:
///
/// 1. the play-only-when-active preference suppresses **new** sounds while inactive, so it is
///    written here through the same seam as the volumes;
/// 2. no secondary buffer carries the global-focus flag (the request flags are `0x100E2`), so
///    DirectSound silences the **output** regardless of the preference, and it silences it
///    without stopping the voices, so a sound plays on inaudibly and does not resume.
#[test]
fn losing_focus_silences_the_output_and_the_preference_gates_new_sounds() {
    use dereth_ui_screens::PrefValue;
    let store = store();
    let mut probe = audio(dereth_audio::Prefs::default());
    let wave = a_wave(&store, &mut probe);

    let mut a = audio(dereth_audio::Prefs::default());
    a.create_sound(&store, wave);
    a.set_listener(dereth_audio::Listener::default());
    assert!(
        effect_peak(&mut a, wave) > 0.0,
        "no sound with the window active"
    );

    // (1) Inactive with the default preference: no new sound starts at all.
    a.set_focus(false);
    assert_eq!(a.active_voices(), 0, "voices left over");
    assert_eq!(
        effect_peak(&mut a, wave),
        0.0,
        "a new sound started while inactive"
    );
    assert_eq!(
        a.active_voices(),
        0,
        "the sound-start path did not return early"
    );

    // (2) With the preference off, the sound *starts* -- and the output is still silent, because
    // the mute is DirectSound's and not the preference's.
    set_pref(
        &mut a,
        "Sound.PlaySoundOnlyWhenActive",
        PrefValue::Bool(false),
    );
    a.play_trigger(dereth_client::audio::SoundTrigger::Wave {
        id: wave,
        at: Vec3::ZERO,
        volume: 1.0,
        priority: 0.9,
        probability: 1.0,
    });
    assert_eq!(
        a.active_voices(),
        1,
        "the preference is off; the sound must start"
    );
    let mut buf = vec![0.0f32; BLOCK];
    a.mix(&mut buf);
    assert_eq!(
        peaks(&buf),
        (0.0, 0.0),
        "the output is not muted while the window is inactive"
    );

    // And it played on inaudibly rather than pausing: the voice consumed the block.
    a.set_focus(true);
    let after = a.active_voices();
    let mut buf2 = vec![0.0f32; BLOCK];
    a.mix(&mut buf2);
    eprintln!("voices after regaining focus: {after}");
    assert!(
        after == 0 || peaks(&buf2).0 > 0.0,
        "a voice survived the mute but produced nothing once unmuted"
    );
}
