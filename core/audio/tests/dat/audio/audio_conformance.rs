//! Every wave preserves its raw header and decodes; the one MP3 is referenced once; attenuation and pan sweeps
//! exact; the voice pool is round-robin and drops the 17th; the last sound-table row is
//! unreachable; a seeded ambient schedule; tweaked-hook field order; preference polarity; the
//! probability gate at 1.0.
//! Fixture: the shipped retail DAT records and recorded inputs.

use std::sync::Arc;

use dereth_assets::audio::{SoundTable, Wave, WAVE_FORMAT_MPEGLAYER3, WAVE_FORMAT_PCM};
use dereth_assets::hook::HookData;
use dereth_assets::motion::PhysicsScript;
use dereth_assets::{Animation, Decode};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::num::rng::Ran2;
use dereth_primitives::{DataId, LocalTime, Vec3};

use dereth_audio::ambient::place::{get_sound_pos, DirSet};
use dereth_audio::ambient::weight::Direction;
use dereth_audio::ambient::{Ambient, AmbientPlay};
use dereth_audio::mixer::{StartOutcome, VoicePool, NUM_VOICES};
use dereth_audio::table::row_index;
use dereth_audio::testing::test_region;
use dereth_audio::{attenuation, pan, table, Category, Prefs, SoundFeatures};

// ---------------------------------------------------------------------------------------------
// Retail-data helpers
// ---------------------------------------------------------------------------------------------

fn store() -> RetailDatStore {
    dereth_dat::testing::open_store_or_fail()
}

// ---------------------------------------------------------------------------------------------
// 1. Census
// ---------------------------------------------------------------------------------------------

/// Every wave preserves its format fields and payload boundaries.
#[test]
fn every_wave_format_matches_its_raw_header_and_payload() {
    let s = store();
    let ids = s.ids_of(DbType::Wave);
    assert!(!ids.is_empty(), "waves are exercised");
    let mut pcm = 0;
    let mut mp3 = 0;
    for id in ids {
        let bytes = s.read_portal(id).expect("wave record reads");
        let wave = Wave::decode_payload(id, &bytes).expect("wave decodes");
        let format = wave.format.expect("every wave has a format header");
        let word = |offset| u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
        let dword = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        assert_eq!(wave.header_size, dword(4));
        assert_eq!(wave.data_size, dword(8));
        assert_eq!(format.format_tag, word(12));
        assert_eq!(format.channels, word(14));
        assert_eq!(format.samples_per_sec, dword(16));
        assert_eq!(format.bits_per_sample, word(26));
        assert_eq!(wave.data_offset, 12 + wave.header_size as usize);
        assert_eq!(wave.data_offset + wave.data_size as usize, bytes.len());
        assert!(format.channels == 1 || format.channels == 2);
        assert!(format.samples_per_sec > 0);
        match format.format_tag {
            WAVE_FORMAT_PCM => {
                pcm += 1;
                assert_eq!(wave.header_size, 18);
                assert!(format.bits_per_sample == 8 || format.bits_per_sample == 16);
                assert_eq!(
                    format.block_align,
                    format.channels * format.bits_per_sample / 8
                );
                assert_eq!(
                    format.avg_bytes_per_sec,
                    format.samples_per_sec * u32::from(format.block_align)
                );
            }
            WAVE_FORMAT_MPEGLAYER3 => {
                mp3 += 1;
                assert_eq!(wave.header_size, 30);
                assert_eq!(format.bits_per_sample, 0);
            }
            other => panic!("{id}: unsupported format {other}"),
        }
    }
    assert!(pcm > 0 && mp3 > 0, "both supported codecs are exercised");
}

/// Every shipped wave decodes to pcm.
#[test]
fn every_shipped_wave_decodes_to_pcm() {
    let s = store();
    let mut decoded = 0usize;
    let ids = s.ids_of(DbType::Wave);
    assert!(!ids.is_empty(), "waves are exercised");
    let input_count = ids.len();
    for id in ids {
        let bytes = s.read_portal(id).expect("wave record reads");
        let w = Wave::decode_payload(id, &bytes).expect("wave decodes");
        let sample = dereth_audio::wave::decode(&w, &bytes).unwrap_or_else(|e| panic!("{id}: {e}"));
        assert!(!sample.frames.is_empty(), "{id} decoded to nothing");
        assert_eq!(
            sample.frames.len() % 2,
            0,
            "{id}: output must be interleaved stereo"
        );
        // Every sample is in range: an out-of-range value means the bit depth was misread.
        assert!(
            sample.frames.iter().all(|v| (-1.0..=1.0).contains(v)),
            "{id}: a sample fell outside [-1, 1]"
        );
        let format = w.format.unwrap();
        assert_eq!(sample.source_rate, format.samples_per_sec);
        assert!(sample.seconds() > 0.0);
        if format.format_tag == WAVE_FORMAT_PCM {
            let source_frames = w.data_size as f64 / f64::from(format.block_align);
            let expected_seconds = source_frames / f64::from(format.samples_per_sec);
            assert!(
                (sample.seconds() - expected_seconds).abs()
                    <= 2.0 / f64::from(dereth_audio::wave::MIX_RATE),
                "{id}: PCM duration follows the input samples"
            );
        }
        decoded += 1;
    }
    assert_eq!(decoded, input_count, "every input wave decodes");
}

/// The one mp3 is referenced only by sound table 20000048.
#[test]
fn the_one_mp3_is_referenced_only_by_sound_table_20000048() {
    let s = store();
    let mp3 = DataId(0x0A00_0393);
    let mut referencing = Vec::new();
    for id in s.ids_of(DbType::STable) {
        let bytes = s.read_portal(id).expect("table reads");
        let t = SoundTable::decode_payload(id, &bytes).expect("table decodes");
        for node in &t.nodes {
            for row in &node.data {
                if row.sound_id == mp3 {
                    referencing.push((id, node.key, *row));
                }
            }
        }
    }
    assert_eq!(referencing.len(), 1, "exactly one reference to the MP3");
    let (table_id, stype, row) = referencing[0];
    assert_eq!(table_id, DataId(0x2000_0048));
    assert_eq!(stype, 0x23, "the first scratch sound type");
    assert_eq!(row.priority, 0.3);
    assert_eq!(row.probability, 0.8);
    assert_eq!(row.volume, 1.0);
}

// ---------------------------------------------------------------------------------------------
// 2. Attenuation
// ---------------------------------------------------------------------------------------------

/// Behaviour: audio.mixer.attenuation-and-pan-follow-the-recovered-tables
#[test]
fn the_attenuation_sweep_is_exact() {
    let p = Prefs::default();
    let db = |d: f32| attenuation(d, 1.0, Category::Effect, &p);

    // The near field is flat.
    for d in [0.0f32, 0.5, 1.0, 2.0, 3.0, 4.0, 4.999, 5.0] {
        assert_eq!(db(d), Some(0), "dist {d}");
    }
    for (d, want) in [
        (6.0f32, -3),
        (7.0, -5),
        (10.0, -12),
        (20.0, -24),
        (40.0, -36),
        (80.0, -48),
    ] {
        assert_eq!(db(d), Some(want), "dist {d}");
    }
    // -12 dB per doubling, across the whole audible range.
    let mut d = 5.0f32;
    while d * 2.0 <= 80.0 {
        let a = db(d).expect("audible");
        let b = db(d * 2.0).expect("audible");
        assert_eq!(b - a, -12, "doubling from {d} m");
        d *= 2.0;
    }
    // The floor saturates and then the sound is dropped.
    assert_eq!(db(94.18), Some(-50));
    assert_eq!(db(94.19), None);
    for d in [95.0f32, 100.0, 200.0, 1000.0] {
        assert_eq!(db(d), None, "dist {d}");
    }
    // Every value in a fine sweep is an integer in [-50, 0] or a drop, and the sequence is monotone.
    let mut last = 0;
    let mut i = 0u32;
    while i < 12_000 {
        let d = f64::from(i) / 100.0;
        #[allow(clippy::cast_possible_truncation)] // LINT-OK: a sweep parameter, not engine input
        let d = d as f32;
        match db(d) {
            Some(v) => {
                assert!((-50..=0).contains(&v), "dist {d} gave {v}");
                assert!(
                    v <= last,
                    "the curve must never rise: {last} then {v} at {d}"
                );
                last = v;
            }
            None => assert!(
                d >= 94.18,
                "dropped at {d}, which is inside the audible range"
            ),
        }
        i += 1;
    }
}

// ---------------------------------------------------------------------------------------------
// 3. Pan
// ---------------------------------------------------------------------------------------------

/// Behaviour: audio.mixer.attenuation-and-pan-follow-the-recovered-tables
/// **Assertion 3.** The bearing sweep, including 0 inside 5 m and 0 in Mono.
///
/// Oracle: the recovered four-row bearing table and the client's pan arithmetic transcribed in
/// `src/atten.rs`.
///
/// The magnitude at a cardinal is **14, not 15**: the client's pi/180 is a 32-bit float, so `sin(90
/// * DEG_TO_RAD)` is `1 - 2^-53` and the integer conversion truncates `14.999999999999998` down.
/// The knowledge base said 15; it is corrected in place, and the playback path's +/-15 clamp is
/// therefore dead code.
#[test]
fn the_pan_sweep_is_exact() {
    let s = SoundFeatures::Stereo;
    // Cardinals, listener facing north. `bearing` runs from the sound to the listener.
    assert_eq!(pan(270.0, 0.0, 10.0, s), 14, "sound due east -> right");
    assert_eq!(pan(90.0, 0.0, 10.0, s), -14, "sound due west -> left");
    assert_eq!(pan(180.0, 0.0, 10.0, s), 0, "sound ahead -> centre");
    assert_eq!(
        pan(0.0, 0.0, 10.0, s),
        0,
        "sound behind -> centre, indistinguishable from ahead"
    );

    // Inside 5 m everything is centred, and the gate is on the truncated distance.
    for d in [0.0f32, 1.0, 4.0, 4.9999] {
        assert_eq!(pan(270.0, 0.0, d, s), 0, "dist {d}");
    }
    assert_eq!(pan(270.0, 0.0, 5.0, s), 14);

    // Mono forces 0 everywhere.
    for i in 0..360u32 {
        let b = f32::from(u16::try_from(i).expect("< 360"));
        assert_eq!(pan(b, 0.0, 50.0, SoundFeatures::Mono), 0, "bearing {b}");
    }

    // A full sweep: never outside +/-14, antisymmetric, and monotone from centre to each side.
    for i in 0..3600u32 {
        let b = f32::from(u16::try_from(i).expect("< 3600")) / 10.0;
        let v = pan(b, 0.0, 10.0, s);
        assert!((-14..=14).contains(&v), "bearing {b} gave {v}");
    }
    // East is positive at every listener heading.
    for h in 0..360u32 {
        let heading = f32::from(u16::try_from(h).expect("< 360"));
        let east_of_listener = (heading + 90.0 + 180.0) % 360.0;
        assert!(
            pan(east_of_listener, heading, 10.0, s) > 0,
            "heading {heading}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 4. Voices
// ---------------------------------------------------------------------------------------------

/// Behaviour: audio.voices.the-seventeenth-voice-is-dropped
#[test]
fn the_voice_pool_allocates_round_robin_and_drops_the_seventeenth() {
    let long: Arc<[f32]> = vec![0.25f32; 10_000].into();
    let mut p = VoicePool::default();

    // The scripted sequence: sixteen starts fill 0..15 in order and leave the cursor at 0.
    for expect in 0..NUM_VOICES {
        assert_eq!(
            p.start(Arc::clone(&long), 0, 0, 0.0),
            StartOutcome::Started(expect)
        );
        assert_eq!(p.cursor(), (expect + 1) & 15);
    }
    assert_eq!(p.active(), NUM_VOICES);

    // The seventeenth is dropped -- every priority is 0.0 and the test is strictly less-than.
    for priority in [0.0f32, -1.0] {
        assert_eq!(
            p.start(Arc::clone(&long), 0, 0, priority),
            StartOutcome::Dropped
        );
    }
    assert_eq!(p.active(), NUM_VOICES, "nothing was stolen");
    assert_eq!(p.cursor(), 0, "a dropped sound does not move the cursor");

    // Drain: mixing 10 000 frames finishes every voice.
    let mut out = vec![0.0f32; 20_000];
    p.mix(&mut out);
    assert_eq!(p.active(), 0);
    // Sixteen voices at gain 1.0 summing 0.25 each.
    assert!(
        (out[0] - 4.0).abs() < 1e-5,
        "sixteen voices summed to {}",
        out[0]
    );

    // And the scan restarts from the cursor, not from 0.
    let mut p = VoicePool::default();
    for _ in 0..5 {
        p.start(Arc::clone(&long), 0, 0, 0.0);
    }
    assert_eq!(p.cursor(), 5);
    assert_eq!(
        p.start(Arc::clone(&long), 0, 0, 0.0),
        StartOutcome::Started(5)
    );
}

// ---------------------------------------------------------------------------------------------
// 5. Row pick
// ---------------------------------------------------------------------------------------------

/// The last row is unreachable in every shipped table.
#[test]
fn the_last_row_is_unreachable_in_every_shipped_table() {
    let s = store();
    let ids = s.ids_of(DbType::STable);
    assert!(!ids.is_empty(), "sound tables are exercised");
    let input_count = ids.len();

    let mut nodes_at_depth0 = 0usize;
    let mut nodes_at_depth1 = 0usize;
    let mut multi_row = Vec::new();
    let mut single_row = 0usize;
    for id in ids {
        let bytes = s.read_portal(id).expect("table reads");
        let t = SoundTable::decode_payload(id, &bytes).expect("table decodes");
        nodes_at_depth0 += 1;
        nodes_at_depth1 += t.nodes[0].children.len();
        for node in &t.nodes {
            match node.data.len() {
                0 => {}
                1 => single_row += 1,
                n => multi_row.push((id, node.key, n)),
            }
        }
    }
    assert_eq!(nodes_at_depth0, input_count);
    assert!(nodes_at_depth1 > 0, "child nodes are exercised");
    assert!(single_row > 0, "single-row entries are exercised");
    assert!(!multi_row.is_empty(), "multi-row entries are exercised");

    // 100 000 draws from the real generator, per multi-row entry.
    let mut rng = Ran2::new(20_130_918);
    for (id, key, n) in &multi_row {
        let mut seen = vec![false; *n];
        for _ in 0..100_000 {
            let u = rng.roll_f32(0.0, 1.0);
            let i = row_index(*n, u).expect("the index is always in range for n >= 1");
            seen[i] = true;
        }
        assert!(
            !seen[*n - 1],
            "{id} SoundType {key}: the last of {n} rows was reached"
        );
        assert!(seen[0], "row 0 must be reachable");
    }

    // And the same holds for any n a future table might use.
    for n in 2..=16usize {
        for step in 0..=50_000u32 {
            let u = (f64::from(step) / 50_000.0).min(0.999_999_88);
            #[allow(clippy::cast_possible_truncation)] // LINT-OK: a sweep parameter
            let u = u as f32;
            assert!(
                row_index(n, u).expect("in range") < n - 1,
                "n = {n}, u = {u}"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// 6. Ambient schedule
// ---------------------------------------------------------------------------------------------

/// A seeded ran2 reproduces the ambient schedule including the draw order.
#[test]
fn a_seeded_ran2_reproduces_the_ambient_schedule_including_the_draw_order() {
    use dereth_audio::ambient::scan::{TerrainCell, TerrainNeighbourhood};

    // Sound-table initialization succeeds for every shipped descriptor.
    const LOADED: fn(DataId) -> bool = |_| true;

    let region = test_region();
    let seed = 20_130_918;

    // One intermittent scene 40 m due north of the listener, so the direction set has one entry and
    // the band is [30, 50].
    let cells = TerrainNeighbourhood {
        outdoors: true,
        cells: vec![TerrainCell {
            pos: Vec3::new(0.0, 40.0, 0.0),
            terrain_word: 3 << 2,
        }],
    };

    let mut amb = Ambient::new();
    let mut rng = Ran2::new(seed);
    let mut events: Vec<(f64, AmbientPlay)> = Vec::new();
    let mut now = LocalTime(0.0);
    for p in amb.on_position_changed(&region, Vec3::ZERO, &cells, now, true, &mut rng, &LOADED) {
        events.push((now.0, p));
    }
    // Advance in 0.25 s steps for 60 s, draining the heap as the client's per-frame `UseTime` does.
    for step in 1..=240u32 {
        now = LocalTime(f64::from(step) * 0.25);
        for p in amb.use_time(&region, now, true, &mut rng, &LOADED) {
            events.push((now.0, p));
        }
    }
    assert!(
        events.len() >= 10,
        "expected a run of events, got {}",
        events.len()
    );

    // Re-derive the same run by hand.
    let mut hand = Ran2::new(seed);
    let mut dirs = DirSet::default();
    dirs.add_dir(Direction::NorthOfViewer, 30.0, 50.0);
    // `base_chance` 1.0 at a terrain share of 1 gives `play_chance` 1.0, so `play_now` always passes.
    let mut want: Vec<(f64, Vec3, f32)> = Vec::new();
    let mut fired_at = 0.0f64;
    loop {
        // play_now (one draw), get_sound_pos (three), then get_play_interval (one).
        let played = hand.roll_f32(0.0, 1.0) < 1.0;
        assert!(played, "play_chance is 1.0");
        let off = get_sound_pos(&dirs, &mut hand).expect("one direction");
        let interval = hand.roll_f32(1.0, 5.0);
        want.push((fired_at, Vec3::new(off.x, off.y, 0.0), 0.6));
        // The scheduler re-inserts at `current_time + play_interval`, and
        // pops when `key < cur_time`, on the caller's 0.25 s frame grid.
        let due = fired_at + f64::from(interval);
        let next_frame = (due / 0.25).floor() * 0.25 + 0.25;
        if next_frame > 60.0 {
            break;
        }
        fired_at = next_frame;
    }

    assert_eq!(events.len(), want.len(), "event count");
    for (i, ((t, got), (wt, wpos, wvol))) in events.iter().zip(want.iter()).enumerate() {
        assert!(
            (t - wt).abs() < 1e-9,
            "event {i}: fired at {t}, expected {wt}"
        );
        assert_eq!(got.pos, Some(*wpos), "event {i}: position");
        assert!((got.volume - wvol).abs() < 1e-6, "event {i}: volume");
        assert_eq!(got.stype, 72, "event {i}: the scene's SoundType");
    }
    // The generators are in lockstep: no extra draw was made anywhere.
    assert_eq!(rng.next_f64(), hand.next_f64());
}

// ---------------------------------------------------------------------------------------------
// The SoundTweakedHook field order
// ---------------------------------------------------------------------------------------------

/// The client reads `SoundTweakedHook`'s two middle floats as **probability then priority**; ACE has
/// them the other way round.
///
/// Oracle: the retail `client_portal.dat`. `docs/CORRECTIONS.md` states the data half of the argument —
/// across all 541 tweaked-sound hooks the first float never exceeds 1.0 while the second reaches 3.0,
/// which cannot be a probability. This re-counts them: 541 hooks, 173 in `0x03` animations and 368 in
/// `0x33` physics scripts.
#[test]
fn every_shipped_tweaked_hook_has_a_probability_first_and_a_priority_second() {
    let s = store();

    let mut max_first = f32::NEG_INFINITY;
    let mut max_second = f32::NEG_INFINITY;
    let mut tally = |h: &HookData| {
        if let HookData::SoundTweaked {
            probability,
            priority,
            ..
        } = h
        {
            assert!(
                (0.0..=1.0).contains(probability),
                "each hook carries a probability"
            );
            assert!(priority.is_finite(), "each hook carries a finite priority");
            max_first = max_first.max(*probability);
            max_second = max_second.max(*priority);
            true
        } else {
            false
        }
    };

    let mut in_animations = 0usize;
    for id in s.ids_of(DbType::Anim) {
        let bytes = s.read_portal(id).expect("animation reads");
        let a = Animation::decode_payload(id, &bytes).expect("animation decodes");
        for f in &a.part_frames {
            for h in &f.hooks {
                if tally(&h.data) {
                    in_animations += 1;
                }
            }
        }
    }

    let mut in_scripts = 0usize;
    for id in s.ids_of(DbType::PhysicsScript) {
        let bytes = s.read_portal(id).expect("script reads");
        let p = PhysicsScript::decode_payload(id, &bytes).expect("script decodes");
        for step in &p.script_data {
            if tally(&step.hook.data) {
                in_scripts += 1;
            }
        }
    }

    let total = in_animations + in_scripts;
    assert!(total > 0, "sound-adjustment hooks are exercised");
    assert!(in_animations > 0, "animation hooks are exercised");
    assert!(in_scripts > 0, "script hooks are exercised");
    assert!(
        max_first <= 1.0,
        "the first float is a probability: max {max_first}"
    );
    assert!(
        max_second > 1.0,
        "the second is a priority: max {max_second}"
    );
    assert!(
        (max_second - 3.0).abs() < 1e-6,
        "CORRECTIONS.md records the maximum priority as 3.0, found {max_second}"
    );
}

/// The preference polarity is preserved.
#[test]
fn the_preference_polarity_is_preserved() {
    let ini = concat!(
        "[Display]\nFullScreen=True\n",
        "[Sound]\nSoundVolume=1.000000\nAmbientSoundVolume=1.000000\n",
        "InterfaceSoundVolume=1.000000\nSoundFeatures=Stereo\nSoundDisabled=True\n",
        "AmbientSoundDisabled=True\nInterfaceSoundDisabled=True\nPlaySoundOnlyWhenActive=True\n",
    );
    let p = Prefs::from_ini(ini);
    assert!(
        p.effects_enabled,
        "Sound.SoundDisabled=True means sound is ON"
    );
    assert!(p.ambient_enabled);
    assert!(p.interface_enabled);
    assert_eq!(p, Prefs::from_ini(&p.to_ini_section()), "round trip");
    assert_eq!(p, Prefs::default(), "the documented defaults, written out");
}

// ---------------------------------------------------------------------------------------------
// The probability gate (both generators)
// ---------------------------------------------------------------------------------------------

/// The probability roll uses the **CRT** generator, not `ran2`, and its scale
/// makes `probability_ = 1.0` always pass.
///
/// Oracle: the client's roll -- the CRT `rand()` scaled by a 32-bit constant and compared
/// strictly -- whose exact f32 value is `3.0518509447574615e-05`.
#[test]
fn the_probability_gate_uses_the_crt_generator_and_always_passes_at_one() {
    assert!(table::play_probability(32_767, 1.0));
    assert!(!table::play_probability(32_767, 0.999_99));
    assert!(!table::play_probability(0, 0.0));
    // Over the whole rand() range, p = 1.0 never fails.
    for r in 0..=32_767u16 {
        assert!(table::play_probability(r, 1.0), "rand() = {r}");
    }
}
