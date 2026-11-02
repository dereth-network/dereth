//! Every music file of the end-of-retail data files read, and every piece's needs found.

use std::collections::BTreeMap;

use auberean_audio::music::form::TrackKind;
use auberean_audio::music::library::{Library, Object};

use crate::records::{dats, ids};

/// Behaviour: audio.music.every-music-file-reads
#[test]
fn every_music_file_reads_by_its_form() {
    let d = dats();
    let mut lib = Library::new(&d);
    assert!(
        lib.len() >= 1036,
        "the file table names the music files ({})",
        lib.len()
    );
    let mut forms: BTreeMap<&str, usize> = BTreeMap::new();
    let (mut instruments, mut regions, mut waves, mut patterns, mut parts) = (0, 0, 0, 0, 0);
    for id in ids(&d, 0x24) {
        let o = lib.load(&d, id).unwrap_or_else(|e| panic!("{e}"));
        let f = match &o {
            Object::Segment(_) => "segment",
            Object::Style(s) => {
                patterns += s.patterns.len();
                parts += s.parts.len();
                "style"
            }
            Object::Collection(c) => {
                instruments += c.instruments.len();
                regions += c.instruments.iter().map(|i| i.regions.len()).sum::<usize>();
                waves += c.waves.len();
                for i in &c.instruments {
                    for r in &i.regions {
                        assert!(
                            (r.wave as usize) < c.waves.len(),
                            "{id:08X}: a region's wave is in the pool"
                        );
                    }
                }
                "collection"
            }
            Object::Script(_) => "script",
            Object::Other(_) => "other",
        };
        *forms.entry(f).or_default() += 1;
    }
    assert_eq!(forms.get("segment"), Some(&575));
    assert_eq!(forms.get("style"), Some(&379));
    assert_eq!(forms.get("collection"), Some(&60));
    assert_eq!(forms.get("script"), Some(&13));
    assert_eq!(
        forms.get("other"),
        Some(&9),
        "the audio path configurations"
    );
    assert_eq!((instruments, regions, waves), (61, 334, 319));
    assert_eq!((patterns, parts), (1164, 1024));
}

/// Behaviour: audio.music.every-piece-finds-its-needs
#[test]
fn every_piece_finds_everything_it_names() {
    let d = dats();
    let mut lib = Library::new(&d);
    let mut pieces = 0;
    let mut tracks: BTreeMap<String, usize> = BTreeMap::new();
    for id in ids(&d, 0x24) {
        if !matches!(lib.load(&d, id), Ok(Object::Segment(_))) {
            continue;
        }
        let p = lib
            .piece(&d, id)
            .unwrap_or_else(|e| panic!("{id:08X}: {e}"));
        pieces += 1;
        for t in &p.segment.tracks {
            let k = match &t.kind {
                TrackKind::Other(f) => format!("other {}", String::from_utf8_lossy(f)),
                k => format!("{k:?}")
                    .split(['(', ' ', '{'])
                    .next()
                    .unwrap_or("")
                    .to_owned(),
            };
            *tracks.entry(k).or_default() += 1;
        }
    }
    assert_eq!(pieces, 575);
    assert!(
        !tracks.keys().any(|k| k.starts_with("other")),
        "every track is read: {tracks:?}"
    );
}

/// Behaviour: audio.music.every-place-script-runs
#[test]
fn every_place_script_runs_and_plays_a_piece_it_holds() {
    use auberean_audio::music::script;
    let d = dats();
    let mut lib = Library::new(&d);
    let mut scripts = 0;
    for id in ids(&d, 0x24) {
        let Ok(Object::Script(s)) = lib.load(&d, id) else {
            continue;
        };
        scripts += 1;
        let r = script::parse(&s.source).unwrap_or_else(|e| panic!("{id:08X}: {e}"));
        assert_eq!(r.names().len(), 1, "{id:08X}: one routine");
        // Every groove level plays exactly one piece the script holds.
        for groove in 0..=100 {
            let calls = r.run(r.names()[0], groove).expect("the routine");
            assert_eq!(calls.len(), 1, "{id:08X} at groove {groove}: {calls:?}");
            assert!(
                s.objects
                    .iter()
                    .any(|o| o.alias.eq_ignore_ascii_case(&calls[0].name) && o.segment.is_some()),
                "{id:08X}: it holds {}",
                calls[0].name
            );
        }
    }
    assert_eq!(scripts, 13);
}

/// Behaviour: audio.music.object-pieces-play-notes
#[test]
fn the_pieces_of_object_and_creature_sounds_play_notes_on_instruments() {
    use std::sync::Arc;

    use auberean_audio::music::perf::{Performance, PlayOptions, Rules};
    use auberean_audio::records;
    use auberean_audio::settings::{KIND_SEGMENT, MUSIC_PLACED};

    let d = dats();
    let mut lib = Library::new(&d);
    let (mut pieces, mut heard) = (0, 0);
    for id in ids(&d, 0x2C) {
        let m = records::music_info(&d, id).expect("reads");
        if m.kind != KIND_SEGMENT || m.flags & MUSIC_PLACED == 0 {
            continue;
        }
        pieces += 1;
        let piece = Arc::new(
            lib.piece(&d, m.music)
                .unwrap_or_else(|e| panic!("{id:08X}: {e}")),
        );
        let mut perf = Performance::new(22_050, Rules::default(), id);
        let path = perf.add_path(true);
        let st = perf.play(
            piece,
            path,
            PlayOptions {
                repeats: Some(0),
                ..PlayOptions::default()
            },
        );
        let mut buf = vec![0.0f32; 22_050];
        let mut loud = false;
        for _ in 0..60 {
            perf.read(path, &mut buf);
            loud |= buf.iter().any(|x| x.abs() > 1e-3);
            if perf.is_done(st) && perf.voices(path) == 0 {
                break;
            }
        }
        heard += usize::from(loud);
    }
    println!("{heard} of {pieces} placed pieces heard");
    assert_eq!(pieces, 442);
    assert!(
        heard * 10 >= pieces * 9,
        "{heard} of {pieces} placed pieces are heard in a minute"
    );
}
