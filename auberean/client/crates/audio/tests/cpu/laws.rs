//! The laws the sound manager plays by: the volume law, what the settings' bits make of a request,
//! and the positional law.

use auberean_assets::sound::{MusicInfo, SoundInfo};
use auberean_audio::request::{Channel, Looping, PathKind, PlayRequest, PlayType, Timing, Track};
use auberean_audio::settings::{self, Setting};
use auberean_audio::space::{self, Listener, Placement};
use auberean_audio::volume::{self, ChannelVolumes};

/// Behaviour: audio.volume.level-is-linear-in-decibels
#[test]
fn the_level_falls_25_decibels_over_slider_times_volume_and_0_is_silent() {
    // (level, volume) -> millibels.
    let table = [
        ((1.0, 1.0), 0),
        ((1.0, 0.5), -1250),
        ((0.5, 0.5), -1875),
        ((1.0, 0.1), -2250),
        ((1.0, 0.0001), -2500),
        ((1.0, 0.002), -2495),
        ((1.0, 2.0), 0),
        ((1.0, 0.0), -9600),
        ((0.0, 1.0), -9600),
        ((1.0, -1.0), -9600),
        ((0.3, 0.3), -2275),
    ];
    for ((level, vol), mb) in table {
        assert_eq!(volume::millibels(level, vol), mb, "{level} x {vol}");
    }
}

/// Behaviour: audio.volume.music-and-placed-paths-scale-the-slider
#[test]
fn the_music_slider_counts_for_04_and_a_placed_path_for_four() {
    let v = ChannelVolumes::default();
    assert_eq!(volume::channel_level(&v, Channel::Main, false), 1.0);
    let music = volume::channel_level(&v, Channel::Music, false);
    assert!((music - 0.4).abs() < 1e-7);
    assert_eq!(
        volume::millibels(music, 1.0),
        -1500,
        "music on a full slider"
    );
    assert_eq!(volume::channel_level(&v, Channel::Player, true), 4.0);
    // Placed: full level until the product falls under a quarter.
    assert_eq!(volume::millibels(4.0, 0.25), 0);
    assert_eq!(volume::millibels(4.0, 0.2), -500);
    let mut quiet = v;
    assert!(quiet.set(Channel::Ambient, 0.5));
    assert!(
        !quiet.set(Channel::Ambient, 1.5),
        "a slider past 1 is refused"
    );
    assert_eq!(volume::channel_level(&quiet, Channel::Ambient, false), 0.5);
}

fn sound_info(flags: u32) -> SoundInfo {
    SoundInfo {
        flags,
        priority: 0.75,
        min_distance: 3.0,
        sound: 0x0C00_0001,
        ..SoundInfo::default()
    }
}

fn music_info(flags: u32, kind: u32) -> MusicInfo {
    MusicInfo {
        flags,
        priority: 1.0,
        min_distance: 5.0,
        kind,
        music: 0x2400_0001,
    }
}

/// Behaviour: audio.settings.sound-bits
#[test]
fn a_sound_settings_bits_give_its_looping_type_and_path() {
    let req = PlayRequest::new(0x2A00_0001);
    let s = settings::from_sound_info(&req, &sound_info(0x1 | 0x4), 1.5);
    assert!(s.looping);
    assert_eq!(s.kind, PlayType::NonExclusiveBudgeted);
    assert_eq!(s.track, Track::Secondary);
    assert_eq!((s.start, s.stop), (Timing::Immediate, Timing::Immediate));
    assert_eq!(s.length, Some(1.5));
    assert_eq!((s.priority, s.min_distance), (0.75, 3.0));
    assert_eq!(
        settings::path_kind(Setting::Sound(&sound_info(0x4))),
        PathKind::PlacedSound
    );
    assert_eq!(
        settings::path_kind(Setting::Sound(&sound_info(0x10))),
        PathKind::FlatSound
    );
    let s = settings::from_sound_info(&req, &sound_info(0x2), 1.0);
    assert!(!s.looping);
    assert_eq!(s.kind, PlayType::SelfExclusiveBudgeted);
    // The request's own choices win.
    let mut forced = req;
    forced.looping = Looping::ForceOff;
    forced.kind = PlayType::AllExclusive;
    let s = settings::from_sound_info(&forced, &sound_info(0x1), 1.0);
    assert!(!s.looping);
    assert_eq!(s.kind, PlayType::AllExclusive);
}

/// Behaviour: audio.settings.music-bits
#[test]
fn a_music_settings_bits_give_its_track_type_timings_and_path() {
    let req = PlayRequest::new(0x2C00_0001);
    // Primary: never looped, alongside anything, the primary track.
    let s = settings::from_music_info(&req, &music_info(0x2 | 0x1, settings::KIND_SEGMENT));
    assert!(!s.looping);
    assert_eq!(s.kind, PlayType::NonExclusive);
    assert_eq!(s.track, Track::Primary);
    assert_eq!(s.length, None);
    assert_eq!((s.start, s.stop), (Timing::OnMeasure, Timing::Immediate));
    // A looping self-exclusive secondary piece on the beat, stopping on the measure.
    let s = settings::from_music_info(
        &req,
        &music_info(0x1 | 0x4 | 0x40 | 0x4000, settings::KIND_SEGMENT),
    );
    assert!(s.looping);
    assert_eq!(s.kind, PlayType::SelfExclusive);
    assert_eq!(s.track, Track::Secondary);
    assert_eq!((s.start, s.stop), (Timing::OnBeat, Timing::OnMeasure));
    assert!(!s.control);
    let s = settings::from_music_info(
        &req,
        &music_info(0x200 | 0x400 | 0x8000, settings::KIND_SEGMENT),
    );
    assert_eq!((s.start, s.stop), (Timing::OnMarker, Timing::OnMarker));
    assert!(s.control, "a secondary control piece");
    let s = settings::from_music_info(
        &req,
        &music_info(0x20 | 0x80 | 0x800, settings::KIND_SEGMENT),
    );
    assert_eq!((s.start, s.stop), (Timing::OnEnd, Timing::OnEnd));
    // Paths: placed for its bit, else a track is a sound and a segment music, heard everywhere.
    let path = |f, k| settings::path_kind(Setting::Music(&music_info(f, k)));
    assert_eq!(path(0x10, settings::KIND_SEGMENT), PathKind::PlacedMusic);
    assert_eq!(path(0x10, settings::KIND_TRACK), PathKind::PlacedMusic);
    assert_eq!(path(0, settings::KIND_TRACK), PathKind::FlatSound);
    assert_eq!(path(0, settings::KIND_SEGMENT), PathKind::FlatMusic);
}

/// Behaviour: audio.space.place-in-the-listeners-frame
#[test]
fn a_place_is_turned_into_the_listeners_frame() {
    // A listener at (10, 10, 2) looking east: its right is south, its ahead east.
    let l = Listener {
        position: [10.0, 10.0, 2.0],
        right: [0.0, -1.0, 0.0],
        ahead: [1.0, 0.0, 0.0],
        up: [0.0, 0.0, 1.0],
    };
    // A point 4 east, 3 north and 1 up: 4 ahead, 3 to the left, 1 up (right, up, ahead).
    assert_eq!(l.local([14.0, 13.0, 3.0]), [-3.0, 1.0, 4.0]);
    assert_eq!(l.turn([0.0, 0.0, 5.0]), [0.0, 5.0, 0.0]);
}

/// Behaviour: audio.space.distance-law
#[test]
fn a_placed_sound_falls_6_decibels_a_doubling_beyond_its_minimum_distance() {
    assert_eq!(space::distance_gain([0.0, 0.0, 1.0], 2.0), 1.0);
    assert_eq!(space::distance_gain([0.0, 0.0, 2.0], 2.0), 1.0);
    assert_eq!(space::distance_gain([0.0, 0.0, 4.0], 2.0), 0.5);
    assert_eq!(space::distance_gain([3.0, 0.0, 4.0], 2.5), 0.5);
    assert_eq!(space::distance_gain([0.0, 0.0, 80.0], 2.5), 2.5 / 80.0);
    // At the listener a sound is at full level whatever its minimum distance.
    assert_eq!(space::distance_gain([0.0; 3], 0.0), 1.0);
}

/// Behaviour: audio.space.pan-law
#[test]
fn a_placed_sound_pans_wholly_to_one_side_at_a_right_angle_and_ignores_height() {
    assert_eq!(space::pan_gains([0.0, 0.0, 5.0]), [1.0, 1.0], "ahead");
    assert_eq!(space::pan_gains([0.0, 0.0, -5.0]), [1.0, 1.0], "behind");
    assert_eq!(
        space::pan_gains([5.0, 0.0, 0.0]),
        [0.0, 1.0],
        "to the right"
    );
    assert_eq!(
        space::pan_gains([-5.0, 0.0, 0.0]),
        [1.0, 0.0],
        "to the left"
    );
    assert_eq!(space::pan_gains([0.0, 9.0, 0.0]), [1.0, 1.0], "overhead");
    assert_eq!(
        space::pan_gains([3.0, 7.0, 4.0]),
        [0.8, 1.0],
        "height ignored"
    );
    assert_eq!(
        space::pan_gains([-3.0, 0.0, -4.0]),
        [1.0, 0.8],
        "behind, to the left"
    );
}

/// Behaviour: audio.space.doppler
#[test]
fn a_placed_sound_closing_on_the_listener_is_heard_higher() {
    // Ahead, coming straight at the listener at a tenth of the speed of sound.
    let closing = space::doppler([0.0, 0.0, 10.0], [0.0, 0.0, -34.3]);
    assert!((closing - 1.0 / 0.9).abs() < 1e-6);
    let leaving = space::doppler([0.0, 0.0, 10.0], [0.0, 0.0, 34.3]);
    assert!((leaving - 1.0 / 1.1).abs() < 1e-6);
    assert_eq!(
        space::doppler([0.0, 0.0, 10.0], [5.0, 0.0, 0.0]),
        1.0,
        "crossing"
    );
    assert_eq!(
        space::doppler([0.0; 3], [5.0, 0.0, 0.0]),
        1.0,
        "at the listener"
    );
    let p = Placement::of([0.0, 0.0, 8.0], [0.0; 3], 2.0);
    assert_eq!(p.distance, 0.25);
    assert_eq!(p.sides, [1.0, 1.0]);
}
