//! Music made of instruments, over files made in memory: the synthesizer's notes, levels,
//! envelopes and regions on rendered buffers; the performance's timing, bands, triggers, styles
//! and scripts; and a piece played through the sound manager.

use std::sync::Arc;

use auberean_assets::tables::{FileIdEntry, FileIdTable, FileIdType};
use auberean_audio::manager::SoundPlayer;
use auberean_audio::music::dls;
use auberean_audio::music::library::{Library, Piece};
use auberean_audio::music::perf::{Boundary, Performance, PlayOptions, Rules};
use auberean_audio::music::synth::{Event, Patch, Synth};
use auberean_audio::request::PlayRequest;
use auberean_audio::settings::KIND_SEGMENT;
use auberean_audio::space::Listener;
use auberean_primitives::DataId;
use dereth_primitives::num::{math, to_i32_f64};

use crate::Files;

// ---- RIFF builders -------------------------------------------------------------------------

fn ck(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut v = tag.to_vec();
    v.extend_from_slice(&u32::try_from(data.len()).expect("small").to_le_bytes());
    v.extend_from_slice(data);
    if data.len() % 2 == 1 {
        v.push(0);
    }
    v
}

fn list(kind: &[u8; 4], form: &[u8; 4], kids: &[Vec<u8>]) -> Vec<u8> {
    let mut body = form.to_vec();
    for k in kids {
        body.extend_from_slice(k);
    }
    ck(kind, &body)
}

fn le32(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

fn wide(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn reference(class: u32, file: &str) -> Vec<u8> {
    let mut refh = le32(&[class, 0, 0, 0]);
    refh.extend_from_slice(&le32(&[0x10]));
    list(
        b"LIST",
        b"DMRF",
        &[ck(b"refh", &refh), ck(b"file", &wide(file))],
    )
}

/// A collection of one instrument (bank 0, program `program`): regions `(low key, high key,
/// wave)`, waves of 16-bit samples at 22,050 Hz with their unity note and an optional loop over
/// the whole wave, and the instrument's articulation `(source, control, destination, scale)`.
fn dls_file(
    program: u32,
    regions: &[(u16, u16, u32)],
    waves: &[(Vec<i16>, u16, bool)],
    art: &[(u16, u16, u16, i32)],
) -> Vec<u8> {
    let mut rgns = Vec::new();
    for &(lo, hi, w) in regions {
        let mut rgnh = Vec::new();
        for v in [lo, hi, 0, 127, 0, 0] {
            rgnh.extend_from_slice(&v.to_le_bytes());
        }
        let mut wlnk = 0u16.to_le_bytes().to_vec();
        wlnk.extend_from_slice(&0u16.to_le_bytes());
        wlnk.extend_from_slice(&le32(&[1, w]));
        rgns.push(list(
            b"LIST",
            b"rgn ",
            &[ck(b"rgnh", &rgnh), ck(b"wlnk", &wlnk)],
        ));
    }
    let mut conns = le32(&[8, u32::try_from(art.len()).expect("small")]);
    for &(src, ctl, dst, scale) in art {
        for v in [src, ctl, dst, 0] {
            conns.extend_from_slice(&v.to_le_bytes());
        }
        conns.extend_from_slice(&scale.to_le_bytes());
    }
    let ins = list(
        b"LIST",
        b"ins ",
        &[
            ck(
                b"insh",
                &le32(&[u32::try_from(regions.len()).expect("small"), 0, program]),
            ),
            list(b"LIST", b"lrgn", &rgns),
            list(b"LIST", b"lart", &[ck(b"art1", &conns)]),
        ],
    );
    let mut pool = Vec::new();
    let mut cues = Vec::new();
    for (samples, unity, looped) in waves {
        cues.push(u32::try_from(pool.len()).expect("small"));
        let mut fmt = 1u16.to_le_bytes().to_vec();
        fmt.extend_from_slice(&1u16.to_le_bytes());
        fmt.extend_from_slice(&le32(&[22_050, 44_100]));
        fmt.extend_from_slice(&2u16.to_le_bytes());
        fmt.extend_from_slice(&16u16.to_le_bytes());
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut wsmp = le32(&[20]);
        wsmp.extend_from_slice(&unity.to_le_bytes());
        wsmp.extend_from_slice(&0i16.to_le_bytes());
        wsmp.extend_from_slice(&le32(&[0, 0, u32::from(*looped)]));
        if *looped {
            wsmp.extend_from_slice(&le32(&[
                16,
                0,
                0,
                u32::try_from(samples.len()).expect("small"),
            ]));
        }
        pool.extend_from_slice(&list(
            b"LIST",
            b"wave",
            &[ck(b"fmt ", &fmt), ck(b"wsmp", &wsmp), ck(b"data", &data)],
        ));
    }
    let mut ptbl = le32(&[8, u32::try_from(cues.len()).expect("small")]);
    ptbl.extend_from_slice(&le32(&cues));
    let mut wvpl = b"wvpl".to_vec();
    wvpl.extend_from_slice(&pool);
    list(
        b"RIFF",
        b"DLS ",
        &[
            ck(b"colh", &le32(&[1])),
            list(b"LIST", b"lins", &[ins]),
            ck(b"ptbl", &ptbl),
            ck(b"LIST", &wvpl),
        ],
    )
}

fn track(class: u32, body: Vec<u8>) -> Vec<u8> {
    let mut trkh = le32(&[class, 0, 0, 0, 0, 1]);
    trkh.extend_from_slice(&[0; 8]);
    list(b"RIFF", b"DMTK", &[ck(b"trkh", &trkh), body])
}

fn tempo_track(bpm: f64) -> Vec<u8> {
    let mut items = le32(&[16, 0, 0]);
    items.extend_from_slice(&bpm.to_le_bytes());
    track(0xD2AC_2885, ck(b"tetr", &items))
}

fn timesig_track(beats: u8, beat: u8, grids: u16) -> Vec<u8> {
    let mut items = le32(&[8, 0]);
    items.extend_from_slice(&[beats, beat]);
    items.extend_from_slice(&grids.to_le_bytes());
    track(0xD2AC_2888, list(b"LIST", b"TIMS", &[ck(b"tims", &items)]))
}

/// A band: (pchannel, program, volume) from collection `dls`.
fn band(instruments: &[(u32, u32, u8)], dls: &str) -> Vec<u8> {
    let mut lbins = Vec::new();
    for &(pch, program, volume) in instruments {
        let mut bins = le32(&[program, 0, 0, 0, 0, 0, pch, 0x1 | 0x20 | 0x40]);
        bins.extend_from_slice(&[64, volume, 0, 0]);
        bins.extend_from_slice(&le32(&[0]));
        bins.extend_from_slice(&2i16.to_le_bytes());
        bins.extend_from_slice(&[0, 0]);
        lbins.push(list(
            b"LIST",
            b"lbin",
            &[ck(b"bins", &bins), reference(0x480F_F4B0, dls)],
        ));
    }
    list(b"RIFF", b"DMBD", &[list(b"LIST", b"lbil", &lbins)])
}

fn band_track(instruments: &[(u32, u32, u8)], dls: &str) -> Vec<u8> {
    let item = list(
        b"LIST",
        b"lbnd",
        &[
            ck(b"bd2h", &le32(&[0, 0u32.wrapping_sub(200)])),
            band(instruments, dls),
        ],
    );
    track(
        0xD2AC_2894,
        list(b"RIFF", b"DMBT", &[list(b"LIST", b"lbdl", &[item])]),
    )
}

/// A sequence track of notes: (time, duration, pchannel, key, velocity).
fn sequence_track(notes: &[(i32, i32, u32, u8, u8)]) -> Vec<u8> {
    let mut items = le32(&[20]);
    for &(t, d, pch, key, vel) in notes {
        items.extend_from_slice(&t.to_le_bytes());
        items.extend_from_slice(&d.to_le_bytes());
        items.extend_from_slice(&pch.to_le_bytes());
        items.extend_from_slice(&0i16.to_le_bytes());
        items.extend_from_slice(&[0x90, key, vel, 0, 0, 0]);
    }
    track(0xD2AC_2886, ck(b"seqt", &ck(b"evtl", &items)))
}

fn trigger_track(time: i32, file: &str) -> Vec<u8> {
    let mut sgih = time.to_le_bytes().to_vec();
    sgih.extend_from_slice(&le32(&[time as u32, 0x80, 0]));
    let item = list(
        b"LIST",
        b"lseg",
        &[ck(b"sgih", &sgih), reference(0xD2AC_2882, file)],
    );
    track(
        0xBAE4_D665,
        list(
            b"LIST",
            b"segt",
            &[ck(b"sgth", &le32(&[0])), list(b"LIST", b"lsgl", &[item])],
        ),
    )
}

fn style_track(file: &str) -> Vec<u8> {
    track(
        0xD2AC_288D,
        list(
            b"LIST",
            b"sttr",
            &[list(
                b"LIST",
                b"strf",
                &[ck(b"stmp", &le32(&[0])), reference(0xD2AC_288A, file)],
            )],
        ),
    )
}

fn script_track(file: &str, routine: &str) -> Vec<u8> {
    let event = list(
        b"LIST",
        b"scre",
        &[
            ck(b"scrh", &le32(&[2, 0, 0])),
            reference(0x810B_5013, file),
            ck(b"scrn", &wide(routine)),
        ],
    );
    track(
        0x4108_FA85,
        list(b"LIST", b"scrt", &[list(b"LIST", b"scrl", &[event])]),
    )
}

fn segment_body(length: u32, tracks: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut segh = le32(&[0, length, 0, 0, 0, 0]);
    segh.extend_from_slice(&[0; 32]);
    vec![ck(b"segh", &segh), list(b"LIST", b"trkl", &tracks)]
}

fn segment_file(length: u32, tracks: Vec<Vec<u8>>) -> Vec<u8> {
    list(b"RIFF", b"DMSG", &segment_body(length, tracks))
}

/// A pattern: its groove range, its part's channel, and the part's notes (grid, key).
type PatternSpec = (u8, u8, u32, Vec<(i32, u16)>);

/// A style in 4/4 with four grids a beat: one part per pattern, each pattern `(groove low, groove
/// high, pchannel, notes (grid, key))`, all one measure long and fixed.
fn style_file(patterns: &[PatternSpec]) -> Vec<u8> {
    let mut kids = vec![ck(b"styh", &{
        let mut h = vec![4, 4, 4, 0];
        h.extend_from_slice(&120f64.to_le_bytes());
        h
    })];
    for (k, (_, _, _, notes)) in patterns.iter().enumerate() {
        let mut prth = vec![4, 4, 4, 0];
        for _ in 0..32 {
            prth.extend_from_slice(&le32(&[0x7FFF_FFFF]));
        }
        let mut guid = [0u8; 16];
        guid[0] = u8::try_from(k + 1).expect("small");
        prth.extend_from_slice(&guid);
        prth.extend_from_slice(&1u16.to_le_bytes());
        prth.extend_from_slice(&[0, 127, 0, 0, 0, 0]);
        prth.extend_from_slice(&le32(&[0]));
        let mut items = le32(&[24]);
        for &(grid, key) in notes {
            items.extend_from_slice(&grid.to_le_bytes());
            items.extend_from_slice(&le32(&[u32::MAX, 96]));
            items.extend_from_slice(&0i16.to_le_bytes());
            items.extend_from_slice(&key.to_le_bytes());
            items.extend_from_slice(&[100, 0, 0, 0, 0, 16, 0, 0]);
        }
        kids.push(list(
            b"LIST",
            b"part",
            &[ck(b"prth", &prth), ck(b"note", &items)],
        ));
    }
    for (k, (lo, hi, pch, _)) in patterns.iter().enumerate() {
        let mut ptnh = vec![4, 4, 4, 0, *lo, *hi, 0, 0];
        ptnh.extend_from_slice(&1u16.to_le_bytes());
        ptnh.extend_from_slice(&[1, 100]);
        ptnh.extend_from_slice(&le32(&[0]));
        let mut guid = [0u8; 16];
        guid[0] = u8::try_from(k + 1).expect("small");
        let mut prfc = guid.to_vec();
        prfc.extend_from_slice(&[0, 0, 0, 0, 100, 1, 0, 0]);
        prfc.extend_from_slice(&pch.to_le_bytes());
        kids.push(list(
            b"LIST",
            b"pttn",
            &[
                ck(b"ptnh", &ptnh),
                list(b"LIST", b"pref", &[ck(b"prfc", &prfc)]),
            ],
        ));
    }
    list(b"RIFF", b"DMST", &kids)
}

/// A script whose text is `source`, holding pieces by name.
fn script_file(source: &str, pieces: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let objs: Vec<Vec<u8>> = pieces
        .iter()
        .map(|(alias, seg)| {
            list(
                b"LIST",
                b"cobl",
                &[
                    ck(b"coba", &wide(alias)),
                    ck(b"cobh", &[0; 28]),
                    seg.clone(),
                ],
            )
        })
        .collect();
    list(
        b"RIFF",
        b"DMSC",
        &[
            list(b"RIFF", b"DMCN", &[list(b"LIST", b"cosl", &objs)]),
            ck(b"scla", &wide("AudioVBScript")),
            ck(b"scsr", &wide(source)),
        ],
    )
}

/// The music files as records with a file table naming them.
fn library_files(files: &mut Files, named: &[(u32, &str, Vec<u8>)]) {
    let mut entries = Vec::new();
    for (id, name, bytes) in named {
        let mut r = le32(&[*id, u32::try_from(bytes.len()).expect("small")]);
        r.extend_from_slice(bytes);
        r.extend_from_slice(&le32(&[0x8000_0001, 0]));
        files.0.insert(*id, r);
        entries.push(FileIdEntry {
            id: DataId(*id),
            unused: String::new(),
            name: (*name).to_owned(),
        });
    }
    let table = FileIdTable {
        types: vec![FileIdType {
            record_type: 0x4000_0029,
            name: "MUSIC".to_owned(),
            entries,
        }],
    };
    files
        .0
        .insert(0x0E00_0003, auberean_assets::encode(0x0E00_0003, &table));
}

// ---- the synthesizer -------------------------------------------------------------------------

const DC: i16 = 16384;

fn patch(c: Vec<u8>) -> Patch {
    Patch {
        collection: Arc::new(dls::parse(&c).expect("reads")),
        index: 0,
    }
}

/// A looped constant wave rooted at key 60, with `art` as its articulation.
fn steady(art: &[(u16, u16, u16, i32)]) -> Patch {
    patch(dls_file(
        0,
        &[(0, 127, 0)],
        &[(vec![DC; 64], 60, true)],
        art,
    ))
}

fn mono_synth(p: Patch) -> Synth {
    let mut s = Synth::new(22_050, true);
    s.apply(&Event::Program {
        channel: 0,
        patch: Some(p),
    });
    s.apply(&Event::Control {
        channel: 0,
        controller: 7,
        value: 127,
    });
    s
}

fn render(s: &mut Synth, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames];
    s.render(&mut out);
    out
}

/// Time cents (stored) for a time in seconds.
fn tc(seconds: f64) -> i32 {
    to_i32_f64(1200.0 * math::log2(seconds) * 65536.0)
}

/// A level against 0.5 (the test waves' full level), in decibels.
fn db(x: f32) -> f64 {
    20.0 * math::log10(f64::from(x) / 0.5)
}

/// Behaviour: audio.music.synth.note-plays-its-region
#[test]
fn a_note_plays_the_wave_of_the_region_that_holds_its_key() {
    let p = patch(dls_file(
        0,
        &[(0, 59, 0), (60, 127, 1)],
        &[(vec![DC; 64], 60, true), (vec![-DC; 64], 60, true)],
        &[],
    ));
    let mut s = mono_synth(p);
    s.apply(&Event::NoteOn {
        channel: 0,
        key: 48,
        velocity: 127,
    });
    let low = render(&mut s, 256);
    s.apply(&Event::AllNotesOff);
    render(&mut s, 64);
    let mut s2 = mono_synth(patch(dls_file(
        0,
        &[(0, 59, 0), (60, 127, 1)],
        &[(vec![DC; 64], 60, true), (vec![-DC; 64], 60, true)],
        &[],
    )));
    s2.apply(&Event::NoteOn {
        channel: 0,
        key: 72,
        velocity: 127,
    });
    let high = render(&mut s2, 256);
    assert!(
        (low[200] - 0.5).abs() < 1e-3,
        "key 48 plays the first wave at full level: {}",
        low[200]
    );
    assert!(
        (high[200] + 0.5).abs() < 1e-3,
        "key 72 plays the second: {}",
        high[200]
    );
}

/// Behaviour: audio.music.synth.pitch-follows-the-key
#[test]
fn a_note_an_octave_above_the_root_reads_its_wave_twice_as_fast() {
    // A ramp wave without a loop: it ends after its samples at the note's speed.
    let ramp: Vec<i16> = (0..2000)
        .map(|i| i16::try_from(i * 8).expect("small"))
        .collect();
    for (key, frames) in [(60u8, 2000usize), (72, 1000), (48, 4000)] {
        let mut s = mono_synth(patch(dls_file(
            0,
            &[(0, 127, 0)],
            &[(ramp.clone(), 60, false)],
            &[],
        )));
        s.apply(&Event::NoteOn {
            channel: 0,
            key,
            velocity: 127,
        });
        let out = render(&mut s, 5000);
        let last = out.iter().rposition(|x| *x != 0.0).expect("it sounded");
        assert!(
            last.abs_diff(frames) <= 2,
            "key {key} ends after {last} frames, not {frames}"
        );
        assert_eq!(s.active(), 0, "a wave without a loop ends its voice");
    }
}

/// Behaviour: audio.music.synth.velocity-and-volume-curve
#[test]
fn velocity_and_volume_each_scale_the_level_by_the_square_of_their_fraction() {
    let level = |velocity: u8, volume: u8| {
        let mut s = mono_synth(steady(&[]));
        s.apply(&Event::Control {
            channel: 0,
            controller: 7,
            value: volume,
        });
        s.apply(&Event::NoteOn {
            channel: 0,
            key: 60,
            velocity,
        });
        render(&mut s, 400)[300]
    };
    let full = level(127, 127);
    assert!((full - 0.5).abs() < 1e-3);
    let half = level(64, 127) / full;
    assert!((half - (64.0f32 / 127.0).powi(2)).abs() < 1e-3, "{half}");
    let vol = level(127, 100) / full;
    assert!((vol - (100.0f32 / 127.0).powi(2)).abs() < 1e-3, "{vol}");
}

/// Behaviour: audio.music.synth.volume-envelope
#[test]
fn the_volume_envelope_rises_through_its_attack_and_falls_96_db_over_its_release() {
    // Attack 0.1 s, release 0.2 s, sustain full.
    let mut s = mono_synth(steady(&[
        (0, 0, dls::dst::EG1_ATTACK, tc(0.1)),
        (0, 0, dls::dst::EG1_RELEASE, tc(0.2)),
    ]));
    s.apply(&Event::NoteOn {
        channel: 0,
        key: 60,
        velocity: 127,
    });
    let attack = render(&mut s, 4410);
    // Halfway through the attack the level is half (linear in amplitude).
    let mid = attack[1102];
    assert!((mid / 0.5 - 0.5).abs() < 0.03, "half way up: {mid}");
    let held = render(&mut s, 2205);
    assert!((held[2000] - 0.5).abs() < 1e-3, "sustained at full level");
    s.apply(&Event::NoteOff {
        channel: 0,
        key: 60,
    });
    let release = render(&mut s, 4410 + 200);
    // Half the release is 48 decibels down.
    let down = db(release[2205]);
    assert!(
        (down + 48.0).abs() < 1.5,
        "half way through the release: {down} dB"
    );
    assert_eq!(s.active(), 0, "the voice ends at the release's end");
}

/// Behaviour: audio.music.synth.decay-to-sustain
#[test]
fn the_decay_falls_to_the_sustain_level_and_holds_while_the_key_is_down() {
    // Decay 1 s to a sustain of 50% (48 decibels down).
    let mut s = mono_synth(steady(&[
        (0, 0, dls::dst::EG1_DECAY, tc(1.0)),
        (0, 0, dls::dst::EG1_SUSTAIN, 500 * 65536),
    ]));
    s.apply(&Event::NoteOn {
        channel: 0,
        key: 60,
        velocity: 127,
    });
    let out = render(&mut s, 22_050 * 2);
    assert!(
        (db(out[5512]) + 24.0).abs() < 1.0,
        "a quarter second in: {}",
        db(out[5512])
    );
    assert!(
        (db(out[30_000]) + 48.0).abs() < 1.0,
        "held at the sustain: {}",
        db(out[30_000])
    );
    assert_eq!(s.active(), 1);
}

/// Behaviour: audio.music.synth.loops-while-held
#[test]
fn a_looped_wave_sounds_past_its_length_while_the_key_is_held() {
    let mut s = mono_synth(patch(dls_file(
        0,
        &[(0, 127, 0)],
        &[(vec![DC; 32], 60, true)],
        &[],
    )));
    s.apply(&Event::NoteOn {
        channel: 0,
        key: 60,
        velocity: 127,
    });
    let out = render(&mut s, 1000);
    assert!(
        out[900].abs() > 0.4,
        "still sounding after 30 lengths of its wave"
    );
}

/// Behaviour: audio.music.synth.pan
#[test]
fn a_stereo_synthesizer_pans_by_constant_power() {
    let mut s = Synth::new(22_050, false);
    s.apply(&Event::Program {
        channel: 0,
        patch: Some(steady(&[])),
    });
    s.apply(&Event::Control {
        channel: 0,
        controller: 7,
        value: 127,
    });
    s.apply(&Event::Control {
        channel: 0,
        controller: 10,
        value: 127,
    });
    s.apply(&Event::NoteOn {
        channel: 0,
        key: 60,
        velocity: 127,
    });
    let mut out = vec![0.0; 800];
    s.render(&mut out);
    assert!(
        out[700 - 1] > 0.49 && out[700 - 2].abs() < 0.01,
        "hard right: {} {}",
        out[698],
        out[699]
    );
}

// ---- the performance -------------------------------------------------------------------------

/// A library over `files`, and a piece by id.
fn piece(files: &Files, id: u32) -> Arc<Piece> {
    let mut lib = Library::new(files);
    Arc::new(lib.piece(files, id).expect("the piece and its needs read"))
}

fn sequence_files(notes: &[(i32, i32, u32, u8, u8)]) -> Files {
    let mut f = Files::default();
    library_files(
        &mut f,
        &[
            (
                0x2400_0010,
                "inst.dls",
                dls_file(5, &[(0, 127, 0)], &[(vec![DC; 64], 60, true)], &[]),
            ),
            (
                0x2400_0011,
                "band.sgt",
                segment_file(768, vec![band_track(&[(0, 5, 127)], "inst.dls")]),
            ),
            (
                0x2400_0012,
                "piece.sgt",
                segment_file(
                    768 * 8,
                    vec![
                        tempo_track(120.0),
                        trigger_track(0, "band.sgt"),
                        sequence_track(notes),
                    ],
                ),
            ),
        ],
    );
    f
}

/// The first frame a mono path's output is not silent.
fn onset(out: &[f32]) -> Option<usize> {
    out.iter().position(|x| x.abs() > 1e-4)
}

/// Behaviour: audio.music.perf.notes-on-time
#[test]
fn a_sequence_note_sounds_at_its_music_time_at_the_tempo() {
    // A note on beat 3 (1,536 ticks) at 120 beats a minute: one second in.
    let f = sequence_files(&[(1536, 384, 0, 60, 127)]);
    let mut perf = Performance::new(22_050, Rules::default(), 1);
    let path = perf.add_path(true);
    perf.play(piece(&f, 0x2400_0012), path, PlayOptions::default());
    let mut out = vec![0.0; 22_050 * 2];
    perf.read(path, &mut out);
    let at = onset(&out).expect("the note sounds");
    assert!(at.abs_diff(22_050) <= 64, "it starts at frame {at}");
    // It lasts its duration (an eighth note: a quarter of a second at this tempo), then ends.
    assert!(out[22_050 + 5000].abs() > 0.4);
    assert!(
        out[22_050 + 5700].abs() < 0.01,
        "released after its duration"
    );
}

/// Behaviour: audio.music.perf.band-before-notes
#[test]
fn a_band_triggered_at_the_start_sets_the_instrument_before_the_first_note() {
    let f = sequence_files(&[(0, 384, 0, 60, 127)]);
    let mut perf = Performance::new(22_050, Rules::default(), 1);
    let path = perf.add_path(true);
    perf.play(piece(&f, 0x2400_0012), path, PlayOptions::default());
    let mut out = vec![0.0; 2205];
    perf.read(path, &mut out);
    assert_eq!(
        onset(&out),
        Some(0),
        "the note at time 0 sounds from the first frame"
    );
}

/// Behaviour: audio.music.perf.secondary-on-beat
#[test]
fn a_piece_started_on_the_beat_waits_for_the_next_beat() {
    let f = sequence_files(&[(0, 384, 0, 60, 127)]);
    let mut perf = Performance::new(22_050, Rules::default(), 1);
    let path = perf.add_path(true);
    // A tenth of a second in, then on the beat: the next beat is half a second in.
    let mut out = vec![0.0; 2205];
    perf.read(path, &mut out);
    perf.play(
        piece(&f, 0x2400_0012),
        path,
        PlayOptions {
            boundary: Boundary::Beat,
            ..PlayOptions::default()
        },
    );
    let mut out = vec![0.0; 22_050];
    perf.read(path, &mut out);
    let at = onset(&out).expect("sounds") + 2205;
    assert!(at.abs_diff(11_025) <= 64, "starts at frame {at}");
}

/// Behaviour: audio.music.perf.piece-ends
#[test]
fn a_piece_is_done_at_its_length() {
    let f = sequence_files(&[(0, 384, 0, 60, 127)]);
    let mut perf = Performance::new(22_050, Rules::default(), 1);
    let path = perf.add_path(true);
    let st = perf.play(piece(&f, 0x2400_0012), path, PlayOptions::default());
    // Eight beats at 120 beats a minute: four seconds.
    perf.render(22_050 * 4 - 256);
    assert!(!perf.is_done(st));
    perf.render(512);
    assert!(perf.is_done(st));
}

fn style_piece(patterns: &[PatternSpec]) -> Files {
    let mut f = Files::default();
    library_files(
        &mut f,
        &[
            (
                0x2400_0020,
                "inst.dls",
                dls_file(5, &[(0, 127, 0)], &[(vec![DC; 64], 60, true)], &[]),
            ),
            (
                0x2400_0021,
                "band.sgt",
                segment_file(
                    768,
                    vec![band_track(&[(0, 5, 127), (1, 5, 127)], "inst.dls")],
                ),
            ),
            (0x2400_0022, "s.sty", style_file(patterns)),
            (
                0x2400_0023,
                "piece.sgt",
                segment_file(
                    3072,
                    vec![
                        tempo_track(120.0),
                        trigger_track(0, "band.sgt"),
                        style_track("s.sty"),
                    ],
                ),
            ),
        ],
    );
    f
}

/// Behaviour: audio.music.perf.style-pattern-by-groove
#[test]
fn a_style_plays_the_pattern_whose_groove_range_holds_the_groove_level() {
    // Low groove: key 40 on channel 0; high groove: key 80 on channel 1.
    let f = style_piece(&[(1, 50, 0, vec![(0, 40)]), (51, 100, 1, vec![(4, 80)])]);
    // With no leading piece's command the groove level is the default, which the master groove
    // level does not move.
    for (default_groove, master, key, time) in
        [(20, 0, 40u8, 0.0), (70, 0, 80, 768.0), (20, 60, 40, 0.0)]
    {
        let rules = Rules {
            default_groove,
            ..Rules::default()
        };
        let mut perf = Performance::new(22_050, rules, 9);
        perf.keep_log();
        perf.set_master_groove(master);
        let path = perf.add_path(true);
        perf.play(piece(&f, 0x2400_0023), path, PlayOptions::default());
        perf.render(22_050 * 2);
        let notes: Vec<(u8, f64)> = perf.log().iter().map(|n| (n.key, n.time)).collect();
        assert_eq!(notes, vec![(key, time)], "at groove {}", perf.groove());
    }
}

/// Behaviour: audio.music.perf.script-chooses-by-groove
#[test]
fn a_place_script_plays_the_piece_its_groove_test_chooses() {
    let source = "Dim g\r\nSub ChooseGroove\r\n  g = GetMasterGrooveLevel()\r\n  If g < 9 Then\r\n    Low.play(AtFinish)\r\n  End If\r\n  If g > 16 Then\r\n    High.play(AtFinish)\r\n  ElseIf g > 8 Then\r\n    Mid.play(AtFinish)\r\n  End If\r\nEnd Sub\r\n";
    let piece_of = |key: u8| {
        list(
            b"RIFF",
            b"DMSG",
            &segment_body(
                768,
                vec![
                    trigger_track(0, "band.sgt"),
                    sequence_track(&[(0, 96, 0, key, 100)]),
                ],
            ),
        )
    };
    let mut f = Files::default();
    library_files(
        &mut f,
        &[
            (
                0x2400_0030,
                "inst.dls",
                dls_file(5, &[(0, 127, 0)], &[(vec![DC; 64], 60, true)], &[]),
            ),
            (
                0x2400_0031,
                "band.sgt",
                segment_file(768, vec![band_track(&[(0, 5, 127)], "inst.dls")]),
            ),
            (
                0x2400_0032,
                "place.spt",
                script_file(
                    source,
                    &[
                        ("Low", piece_of(40)),
                        ("Mid", piece_of(50)),
                        ("High", piece_of(60)),
                    ],
                ),
            ),
            (
                0x2400_0033,
                "cue.sgt",
                segment_file(
                    768,
                    vec![
                        timesig_track(4, 4, 4),
                        script_track("place.spt", "ChooseGroove"),
                    ],
                ),
            ),
        ],
    );
    let rules = Rules {
        default_groove: 0,
        ..Rules::default()
    };
    for (master, key) in [(0, 40u8), (12, 50), (20, 60)] {
        let mut perf = Performance::new(22_050, rules, 3);
        perf.keep_log();
        perf.set_master_groove(master);
        let path = perf.add_path(false);
        perf.play(piece(&f, 0x2400_0033), path, PlayOptions::default());
        perf.render(22_050);
        let keys: Vec<u8> = perf.log().iter().map(|n| n.key).collect();
        assert_eq!(keys, vec![key], "at master groove {master}");
    }
}

// ---- through the sound manager ---------------------------------------------------------------

/// Behaviour: audio.manager.instrument-music-plays
#[test]
fn a_music_setting_of_instruments_plays_through_the_mixer_and_ends_with_its_piece() {
    let mut f = sequence_files(&[(0, 384, 0, 60, 127)]);
    // Placed music, starting at once, on the music channel.
    f.music(
        0x2C00_0001,
        0x10 | 0x100 | 0x4_0000,
        KIND_SEGMENT,
        0x2400_0012,
    );
    let mut m = SoundPlayer::new(Some(22_050));
    let h = m
        .play(&f, &PlayRequest::new(0x2C00_0001))
        .expect("accepted");
    m.use_time(0.0, Listener::default(), &|_| None);
    let mixer = m.mixer().expect("a device");
    let mut out = vec![0.0; 2 * 2205];
    mixer.lock().expect("mixer").mix(&mut out);
    assert!(out.iter().any(|x| x.abs() > 0.05), "the piece is heard");
    assert!(!m.has_finished(h));
    // Four seconds on (the piece's length), it is done.
    for k in 1..=45 {
        let mut out = vec![0.0; 2 * 2205];
        mixer.lock().expect("mixer").mix(&mut out);
        m.use_time(f64::from(k) * 0.1, Listener::default(), &|_| None);
    }
    m.use_time(4.6, Listener::default(), &|_| None);
    assert!(m.has_finished(h), "done when its piece has ended");
    assert_eq!(m.stats.pieces, 1);
}

/// Behaviour: audio.manager.primary-track-queue
#[test]
fn a_primary_piece_waits_for_the_one_playing_which_repeats_until_another_waits() {
    // Two primary pieces heard everywhere, each four seconds long, starting at once.
    let mut f = sequence_files(&[(0, 384, 0, 60, 127)]);
    f.music(0x2C00_0001, 0x2 | 0x100, KIND_SEGMENT, 0x2400_0012);
    f.music(0x2C00_0002, 0x2 | 0x100, KIND_SEGMENT, 0x2400_0012);
    let mut m = SoundPlayer::new(Some(22_050));
    let mixer = m.mixer().expect("a device");
    let mut now = 0.0;
    let mut turn = |m: &mut SoundPlayer, seconds: f64| {
        let steps = to_i32_f64((seconds * 10.0).round());
        for _ in 0..steps {
            let mut out = vec![0.0; 2 * 2205];
            mixer.lock().expect("mixer").mix(&mut out);
            now += 0.1;
            m.use_time(now, Listener::default(), &|_| None);
        }
    };
    let first = m
        .play(&f, &PlayRequest::new(0x2C00_0001))
        .expect("accepted");
    turn(&mut m, 1.0);
    // The first plays past its length: it starts again.
    turn(&mut m, 5.0);
    assert!(!m.has_finished(first), "the playing primary piece repeats");
    assert_eq!(m.stats.pieces, 2);
    let second = m
        .play(&f, &PlayRequest::new(0x2C00_0002))
        .expect("accepted");
    turn(&mut m, 1.0);
    assert_eq!(m.stats.pieces, 2, "the second waits for the first to end");
    assert!(!m.has_finished(second));
    turn(&mut m, 3.0);
    assert!(
        m.has_finished(first),
        "the first is done when the waiting one starts"
    );
    assert_eq!(m.stats.pieces, 3);
    assert!(!m.has_finished(second));
}

/// Behaviour: audio.manager.a-stopped-primary-piece-waits-no-longer
#[test]
fn a_waiting_primary_piece_that_is_stopped_leaves_the_one_playing_to_repeat() {
    let mut f = sequence_files(&[(0, 384, 0, 60, 127)]);
    f.music(0x2C00_0001, 0x2 | 0x100, KIND_SEGMENT, 0x2400_0012);
    f.music(0x2C00_0002, 0x2 | 0x100, KIND_SEGMENT, 0x2400_0012);
    let mut m = SoundPlayer::new(Some(22_050));
    let mixer = m.mixer().expect("a device");
    let mut now = 0.0;
    let mut turn = |m: &mut SoundPlayer, seconds: f64| {
        let steps = to_i32_f64((seconds * 10.0).round());
        for _ in 0..steps {
            let mut out = vec![0.0; 2 * 2205];
            mixer.lock().expect("mixer").mix(&mut out);
            now += 0.1;
            m.use_time(now, Listener::default(), &|_| None);
        }
    };
    let first = m
        .play(&f, &PlayRequest::new(0x2C00_0001))
        .expect("accepted");
    turn(&mut m, 1.0);
    let second = m
        .play(&f, &PlayRequest::new(0x2C00_0002))
        .expect("accepted");
    turn(&mut m, 0.5);
    assert_eq!(m.stats.pieces, 1, "the second waits");
    m.stop(second, 0.0);
    turn(&mut m, 5.0);
    assert!(m.has_finished(second), "stopped, it is gone");
    assert!(!m.has_finished(first), "the one playing goes on");
    assert_eq!(m.stats.pieces, 2, "and starts again as if none had waited");
}

/// Behaviour: audio.music.chord.music-values
#[test]
fn a_style_notes_music_value_becomes_its_key_over_the_chord_and_scale() {
    use auberean_audio::music::chord::{mode, music_to_midi, ChordKey};

    let c_major = ChordKey {
        chord_pattern: 0x91,
        scale_pattern: 0b1010_1011_0101_1010_1011_0101,
        chord_root: 0,
        scale_root: 0,
    };
    let d_minor = ChordKey {
        chord_root: 2,
        chord_pattern: 0x89,
        ..c_major
    };
    let always = mode::CHORD_ROOT | mode::SCALE_INTERVALS | mode::CHORD_INTERVALS;
    let melodic = mode::CHORD_ROOT | mode::SCALE_INTERVALS;
    // (value, mode, chord, key): octave 2 is the root's octave up; chord positions, scale steps,
    // past the chord's notes, an accidental, the fixed mode, a key brought into range.
    for (value, m, chord, key) in [
        (0x2000, always, &c_major, 12),
        (0x2100, always, &c_major, 16),
        (0x2210, always, &c_major, 21),
        (0x2300, always, &c_major, 23),
        (0x2400, always, &c_major, 26),
        (0x2101, always, &c_major, 17),
        (0x210F, always, &c_major, 15),
        (0x2210, melodic, &c_major, 21),
        (0x2100, always, &d_minor, 17),
        (0x2300, always, &d_minor, 24),
        (60, mode::FIXED, &c_major, 60),
        (0x1001, mode::FIXED, &c_major, 125),
        (0x0010, always, &c_major, 2),
    ] {
        assert_eq!(
            music_to_midi(value, m, chord),
            Some(key),
            "{value:#06x} under mode {m}"
        );
    }
}

/// Behaviour: audio.music.paths-compress
#[test]
fn a_music_paths_compressor_lets_through_a_ratio_of_the_excess_and_adds_its_gain() {
    use auberean_audio::music::effects::{Compressor, CompressorSettings};

    let s = CompressorSettings {
        gain: 10.0,
        attack: 10.0,
        release: 200.0,
        threshold: -30.0,
        ratio: 2.0,
        predelay: 4.0,
    };
    let level_after = |input: f32| {
        let mut c = Compressor::new(s, 1, 22_050);
        let mut block = vec![input; 22_050];
        c.run(&mut block);
        // Held back by the predelay (88 frames), then settled.
        assert_eq!(block[80], 0.0);
        20.0 * math::log10(f64::from(block[20_000] / 1.0))
    };
    // 10 decibels over the threshold leaves 5 over, and the gain adds 10: -15.
    let loud = level_after(math::powf(10.0, -20.0 / 20.0));
    assert!((loud + 15.0).abs() < 0.2, "{loud}");
    // Below the threshold only the gain: -40 becomes -30.
    let quiet = level_after(math::powf(10.0, -40.0 / 20.0));
    assert!((quiet + 30.0).abs() < 0.2, "{quiet}");
}
