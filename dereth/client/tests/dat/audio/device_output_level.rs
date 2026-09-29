//! Sound reaches the device buffer audibly: the startup UI sound and the intro movie's soundtrack
//! are measured on the block the resampler hands the endpoint (after the 11 025 -> 48 000
//! conversion), against absolute levels rather than values the test stored, and the movie's
//! soundtrack plays whatever the sound preferences say.
//!
//! Fixture: the retail UI sound table and waves, and the shipped `turbine_logo_ac.avi` beside the
//! dats. The output stream cannot be opened on a headless runner, so the resampler is driven with
//! the same closure the device start installs; the device open itself is not exercised here.

use dereth_client::audio::{
    Audio, Resampler, SOUND_UI_BUTTON_PRESS, UI_SOUND_TABLE_ENUM, UI_SOUND_TABLE_GROUP,
};
use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_primitives::DataId;
use std::sync::Arc;

/// The rate this machine's endpoint actually reported. Hard-coded rather than probed because a
/// test must not depend on the sound card it happens to run beside; 48 kHz is the shared-mode
/// default on every Windows this rebuild targets, and it is the worst case for the resampler
/// (a non-integer 11025 → 48000 ratio, unlike the 44 100 the unit tests use).
const DEVICE_RATE: u32 = 48_000;

/// One second of device audio, interleaved stereo.
const DEVICE_BLOCK: usize = (DEVICE_RATE as usize) * 2;

/// The level below which a sound effect is, in practice, not heard over a desktop's noise floor.
///
/// −40 dBFS. This is deliberately **far** below the level a correct UI sound reaches (0 dB of
/// attenuation, i.e. the wave's own amplitude) so that the test fails only on a real defect and
/// not on a quiet wave. The attenuation calculation returns `None` — *drop the
/// sound entirely* — below `VOL_MIN_DB`, so the client itself agrees there is a floor under which
/// playing is pointless.
const AUDIBLE: f32 = 0.01;

fn store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store_or_fail())
}

/// A silent `Audio` with the seed pinned, so the probability roll is reproducible.
fn audio() -> Audio {
    Audio::new(dereth_audio::Prefs::default(), 1, false)
}

fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

/// Load the UI sound table exactly as `App::start_shell` does, and return it with its rows resident.
fn ui_table(audio: &mut Audio, store: &Arc<RetailDatStore>) -> DataId {
    let table =
        dereth_client::assets::enum_did(&**store, UI_SOUND_TABLE_GROUP, UI_SOUND_TABLE_ENUM)
            .expect("the UI sound table resolves by enum");
    assert!(
        audio.load_sound_table(store, table),
        "the UI sound table loads"
    );
    let rows: Vec<DataId> = audio
        .assets()
        .table(table)
        .and_then(|t| dereth_audio::table::lookup(t, SOUND_UI_BUTTON_PRESS))
        .expect("the table has the button-press sound")
        .iter()
        .map(|r| r.sound_id)
        .collect();
    assert!(
        !rows.is_empty(),
        "the button-press sound names at least one wave"
    );
    for id in rows {
        audio.create_sound(store, id);
    }
    table
}

/// Drive the resampler with the **same closure `Audio::start_device` installs**, and return the
/// peak of everything it would have submitted to the endpoint.
///
/// `blocks` is in 1/10ths of a second of device time, which is roughly what a `cpal` callback
/// asks for and is long enough for a UI blip to be wholly contained.
fn submitted_peak(audio: &Audio, blocks: usize) -> f32 {
    let mut r = Resampler::new(dereth_audio::MIX_RATE, DEVICE_RATE, 2);
    let mut out = vec![0.0f32; DEVICE_BLOCK / 10];
    let mut worst = 0.0f32;
    for _ in 0..blocks {
        out.fill(0.0);
        r.fill(&mut out, |src| audio.mix(src));
        worst = worst.max(peak(&out));
    }
    worst
}

// ---------------------------------------------------------------------------------------------
// Station 1 — the sound the client plays without being asked.
// ---------------------------------------------------------------------------------------------

/// Behaviour: audio.output.the-startup-sound-reaches-the-device-buffer-audibly
/// **The button-press sound, the one sound `main` plays on every launch, is audible at the
/// device.**
///
/// `App::start_shell` resolves it and `main.rs` plays it before the frame loop, so this is the
/// first thing a player hears and the cheapest proof that the chain works. The assertion is on the
/// resampled block, at an absolute level.
#[test]
fn the_startup_ui_sound_reaches_the_device_boundary_audibly() {
    let store = store();
    let mut a = audio();
    let table = ui_table(&mut a, &store);

    a.play_ui_sound(dereth_audio::UiSoundRef::Table {
        table,
        stype: SOUND_UI_BUTTON_PRESS,
    });
    assert!(a.active_voices() > 0, "the UI sound started a voice");

    let p = submitted_peak(&a, 20);
    assert!(
        p >= AUDIBLE,
        "the startup UI sound reaches the endpoint at peak {p:.5}, which is below the {AUDIBLE} \
         floor -- nothing would be heard"
    );
}

// ---------------------------------------------------------------------------------------------
// Station 2 — the resampler is not the thing that eats it.
// ---------------------------------------------------------------------------------------------

/// **The 11 025 → 48 000 conversion preserves amplitude.**
///
/// Mixer tests measure `AudioSystem::mix`, upstream of the resampler. If the conversion
/// attenuated -- a mis-scaled interpolation weight, a block whose tail is never filled -- they
/// would stay green and nothing would be heard. A full-scale square wave in must come out
/// full-scale.
#[test]
fn the_device_rate_conversion_does_not_attenuate() {
    let mut r = Resampler::new(dereth_audio::MIX_RATE, DEVICE_RATE, 2);
    let mut out = vec![0.0f32; DEVICE_BLOCK / 10];
    let mut worst = 0.0f32;
    for _ in 0..10 {
        out.fill(0.0);
        r.fill(&mut out, |src| src.fill(1.0));
        worst = worst.max(peak(&out));
    }
    assert!(
        worst >= 0.99,
        "a full-scale input came out of the 11025 -> {DEVICE_RATE} conversion at {worst:.5}"
    );
}

// ---------------------------------------------------------------------------------------------
// Station 3 — the level the endpoint gets is the wave's own, and the client adds no loss.
// ---------------------------------------------------------------------------------------------

/// **A UI sound arrives at the endpoint at the amplitude the dat holds, to within the resampler.**
///
/// A quiet button click is a property of the shipped data, not a defect. The button-press sound
/// is one row —
/// `sound_id 0x0A0003B3, priority 0.9, probability 1.0, **volume 0.1**` — and entry 7
/// passes `row.volume_` as the volume, so attenuation is `ceil(20*log10(0.1))` = **-20 dB** and
/// `gain_from_db(-20)` is 0.1. Retail's own button click was 20 dB down; this one is too, and
/// that is correct rather than a bug to fix.
///
/// The assertion is therefore on the *formula*, against two values read out of the dat — the
/// wave's own peak and the row's own volume — and not against a number this test chose. It still
/// catches the gain chain losing level for any other reason, which is what it is here for.
#[test]
fn the_endpoint_receives_the_waves_amplitude_times_the_rows_own_volume() {
    let store = store();
    let mut a = audio();
    let table = ui_table(&mut a, &store);

    let rows = a
        .assets()
        .table(table)
        .and_then(|t| dereth_audio::table::lookup(t, SOUND_UI_BUTTON_PRESS))
        .expect("the table has the button-press sound")
        .to_vec();
    // Retail's attenuation lookup quantises to whole decibels, rounded up, so the expectation carries the
    // same rounding rather than the raw product.
    //
    // **`6.0206`, not `20*log10`.** The client's dB conversion is `log2(x) * 6.0206`, and that
    // factor is a *rounded*
    // copy of `20*log10(2) = 6.020599913...`, so the curve is fractionally steeper than the
    // textbook one. It matters here and only just: `log2(0.1) * 6.0206` is `-20.0006`, which
    // ceils to **-20**, while the textbook `20*log10(0.1)` is `-19.999999999999996`, which ceils
    // to -19 and predicts a sound 1 dB too loud. This constant is the oracle, written out
    // independently rather than called out of the code under test.
    const DB_PER_OCTAVE: f64 = 6.0206;
    let expected: Vec<f32> = rows
        .iter()
        .map(|r| {
            let s = dereth_audio::AudioAssets::sample(a.assets(), r.sound_id)
                .expect("the row's wave decodes");
            let db = (math::log2(f64::from(r.volume)) * DB_PER_OCTAVE).ceil();
            peak(&s.frames) * (math::pow(10f64, db / 20.0) as f32)
        })
        .collect();

    a.play_ui_sound(dereth_audio::UiSoundRef::Table {
        table,
        stype: SOUND_UI_BUTTON_PRESS,
    });
    let submitted = submitted_peak(&a, 20);

    // 3% covers the linear interpolation between two source frames, which cannot overshoot and
    // can undershoot a single-sample peak by less than the step.
    let matched = expected
        .iter()
        .any(|&w| (submitted - w).abs() <= 0.03 * w.max(1e-6));
    assert!(
        matched,
        "the endpoint got peak {submitted:.5}; the dat's rows {rows:?} predict {expected:?} \
         -- the client is losing level between the dat and the device"
    );
}

// ---------------------------------------------------------------------------------------------
// Station 4 — the intro movie's soundtrack.
// ---------------------------------------------------------------------------------------------

/// The shipped logo movie, beside the retail dats.
fn logo_avi() -> std::path::PathBuf {
    dereth_dat::testing::dat_dir().join("turbine_logo_ac.avi")
}

/// **`turbine_logo_ac.avi` has an audio stream, and this client decodes it.**
///
/// The file ships `WAVE_FORMAT_PCM, 2 ch, 16 bit, 48000 Hz`, 7.31 seconds of it — confirmed
/// independently with `ffmpeg -i`. A demuxer that walks only the `vids` stream and collects only
/// `##dc`/`##db` chunks plays the logo in silence, leaving a 25 ms button click at -23 dBFS as the
/// loudest sound of the first ten seconds.
#[test]
fn the_intro_movie_has_a_soundtrack_and_it_decodes() {
    let path = logo_avi();
    let movie = dereth_audio::video::Movie::open(&path)
        .unwrap_or_else(|| panic!("{} opens", path.display()));

    let info = movie
        .audio_info()
        .expect("the AVI declares an audio stream");
    assert_eq!(info.format_tag, dereth_audio::video::WAVE_FORMAT_PCM);
    assert_eq!(info.channels, 2, "stereo");
    assert_eq!(info.sample_rate, 48_000);
    assert_eq!(info.bits_per_sample, 16);

    let track = movie.audio().expect("the soundtrack decodes");
    // A demuxer that found the `strf` but none of the `##wb` chunks would return an empty track,
    // and every assertion above it would still hold.
    let seconds = (track.frames.len() / 2) as f64 / f64::from(dereth_audio::MIX_RATE);
    assert!(
        seconds > 7.0 && seconds < 7.6,
        "the decoded soundtrack is {seconds:.2} s, not the file's 7.31 s"
    );
    let p = peak(&track.frames);
    assert!(
        p > 0.5,
        "the decoded soundtrack peaks at {p:.4}, which is not a logo sting"
    );
}

// ---------------------------------------------------------------------------------------------
// Station 5 — and it reaches the device.
// ---------------------------------------------------------------------------------------------

/// Behaviour: audio.movie.the-intro-movie-soundtrack-plays-regardless-of-sound-preferences
/// **The soundtrack reaches the endpoint, loudly, and answers to no `Sound.*` preference.**
///
/// Retail plays the movie through the system's media player, whose audio renderer is its own, so
/// the track never enters the game's sound mixer and none of the eight preferences applies to it —
/// `dereth_audio::video`'s module header records exactly that. The second half of this test makes
/// that falsifiable: with **all three volumes at zero and all three categories disabled**, a state
/// in which no game sound can be heard at all, the logo must still play.
#[test]
fn the_movie_soundtrack_reaches_the_device_and_ignores_the_sound_preferences() {
    let path = logo_avi();
    let movie = dereth_audio::video::Movie::open(&path)
        .unwrap_or_else(|| panic!("{} opens", path.display()));
    let track = movie.audio().expect("the soundtrack decodes");

    let silenced = dereth_audio::Prefs {
        effect_volume: 0.0,
        ambient_volume: 0.0,
        interface_volume: 0.0,
        effects_enabled: false,
        ambient_enabled: false,
        interface_enabled: false,
        ..dereth_audio::Prefs::default()
    };
    let mut a = Audio::new(silenced, 1, false);
    a.play_movie_audio(&track);

    let p = submitted_peak(&a, 20);
    assert!(
        p >= 0.5,
        "with every Sound.* preference off the movie's soundtrack reached the endpoint at peak \
         {p:.5} -- the logo is silent"
    );
}
