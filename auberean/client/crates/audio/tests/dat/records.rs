//! Every sound setting's wave and every music setting's track of the end-of-retail data files,
//! and a track through the manager.

use std::sync::Arc;

use auberean_audio::manager::SoundPlayer;
use auberean_audio::request::PlayRequest;
use auberean_audio::settings::{KIND_SEGMENT, KIND_TRACK};
use auberean_audio::space::Listener;
use auberean_audio::{records, SoundFiles};
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::DatFile;

/// The portal file and the English language file.
pub struct Dats(Vec<Arc<DatFile>>);

impl SoundFiles for Dats {
    fn read(&self, id: u32) -> Option<Vec<u8>> {
        self.0.iter().find_map(|f| f.read(id).ok())
    }
}

pub fn dats() -> Dats {
    let dir = auberean_dat::testing::require_target_install();
    let found = install_dats(&dir).expect("the install's folder lists");
    let open = |want: DatName| {
        let (_, p) = found
            .iter()
            .find(|(n, _)| *n == want)
            .expect("the file is in the set");
        Arc::new(DatFile::open(p).expect("opens"))
    };
    Dats(vec![
        open(DatName::Portal),
        open(DatName::Local("English".into())),
    ])
}

pub fn ids(d: &Dats, top: u32) -> Vec<u32> {
    d.0[0]
        .entries_in(top << 24, (top << 24) | 0x00FF_FFFF)
        .map(|e| e.id)
        .collect()
}

/// Behaviour: audio.records.every-sound-setting-plays-its-wave
#[test]
fn every_sound_setting_plays_its_wave() {
    let d = dats();
    let settings = ids(&d, 0x2A);
    let mut seconds = 0.0f64;
    for &id in &settings {
        let s = records::sound_info(&d, id).unwrap_or_else(|| panic!("{id:08X} reads"));
        let (source, len) =
            records::sound_source(&d, &s).unwrap_or_else(|e| panic!("{id:08X}: {e}"));
        assert!(source.len_frames() > 0, "{id:08X} has samples");
        assert!(
            (f64::from(len) - source.seconds()).abs() < 1e-3,
            "{id:08X}: its length is its wave's"
        );
        seconds += f64::from(len);
    }
    assert_eq!(settings.len(), 1970);
    assert!(seconds > 60.0, "{seconds} seconds of sound");
}

/// Behaviour: audio.records.every-track-decodes
#[test]
fn every_encoded_track_decodes_and_instrument_music_has_none() {
    let d = dats();
    let (mut tracks, mut segments, mut minutes) = (0, 0, 0.0f64);
    for id in ids(&d, 0x2C) {
        let m = records::music_info(&d, id).unwrap_or_else(|| panic!("{id:08X} reads"));
        match m.kind {
            KIND_TRACK => {
                let s = records::music_source(&d, &m)
                    .unwrap_or_else(|e| panic!("{id:08X}: {e}"))
                    .expect("a track");
                assert!(s.len_frames() > 0);
                minutes += s.seconds() / 60.0;
                tracks += 1;
            }
            KIND_SEGMENT => {
                assert!(records::music_source(&d, &m).expect("no track").is_none());
                segments += 1;
            }
            k => panic!("{id:08X}: kind {k:#x}"),
        }
    }
    assert_eq!((tracks, segments), (37, 479));
    assert!(minutes > 50.0, "{minutes} minutes of tracks");
}

/// Behaviour: audio.manager.a-track-plays-through-the-manager
#[test]
fn a_track_plays_through_the_manager_on_the_next_turn() {
    let d = dats();
    // The first encoded track's setting.
    let id = ids(&d, 0x2C)
        .into_iter()
        .find(|&id| records::music_info(&d, id).is_some_and(|m| m.kind == KIND_TRACK))
        .expect("a track's setting");
    let mut m = SoundPlayer::new(Some(48000));
    let h = m.play(&d, &PlayRequest::new(id)).expect("accepted");
    m.use_time(0.0, Listener::default(), &|_| None);
    let mixer = m.mixer().expect("a device");
    let mut out = vec![0.0; 48000];
    mixer.lock().expect("mixer").mix(&mut out);
    assert!(
        out.iter().any(|s| s.abs() > 0.01),
        "a second of it is heard"
    );
    assert!(!m.has_finished(h));
}
