//! What the sound manager does with requests and the sounds playing: when a sound starts and how
//! loud, the budget, sounds that play one at a time or alone, distance, fades, the sliders, placed
//! sounds and music turned off.

use auberean_audio::manager::{stop, SoundPlayer};
use auberean_audio::request::{Channel, Looping, PlayRequest, PlayType};
use auberean_audio::settings::{KIND_SEGMENT, KIND_TRACK};
use auberean_audio::space::Listener;
use auberean_audio::Life;
use dereth_audio::voices::gain_from_millibels;

use crate::{wave_file, Files};

const WAVE: u32 = 0x0C00_0001;
const LONG_WAVE: u32 = 0x0C00_0002;
const FLAT: u32 = 0x2A00_0001;
const PLACED: u32 = 0x2A00_0002;
const ONE_AT_A_TIME: u32 = 0x2A00_0003;
const LOOPING: u32 = 0x2A00_0004;
const LONG_FLAT: u32 = 0x2A00_0005;
const PLACED_LOOPING: u32 = 0x2A00_0006;
const TRACK_SETTING: u32 = 0x2C00_0001;
const SEGMENT_SETTING: u32 = 0x2C00_0002;
const TRACK: u32 = 0x6100_0001;

/// The records: a short wave of four samples and a long one, at 100 frames a second; sound
/// settings flat, placed, one at a time and looping; a track and a piece of synthesized music.
fn files() -> Files {
    let mut f = Files::default();
    f.wave(WAVE, 100, &[16384, -16384, 8192, 0]);
    f.wave(LONG_WAVE, 100, &[8192; 1000]);
    f.sound(FLAT, 0, 1.0, 2.0, WAVE);
    f.sound(PLACED, 0x4, 1.0, 2.5, LONG_WAVE);
    f.sound(ONE_AT_A_TIME, 0x2, 1.0, 2.0, LONG_WAVE);
    f.sound(LOOPING, 0x1, 1.0, 2.0, WAVE);
    f.sound(LONG_FLAT, 0, 1.0, 2.0, LONG_WAVE);
    f.sound(PLACED_LOOPING, 0x5, 1.0, 2.0, LONG_WAVE);
    f.music(TRACK_SETTING, 0x5, KIND_TRACK, TRACK);
    f.track(TRACK, &wave_file(100, 1, &[4096; 400], &[]));
    f.music(SEGMENT_SETTING, 0, KIND_SEGMENT, 0x2400_0001);
    f
}

fn nobody(_: u64) -> Option<[f32; 3]> {
    None
}

/// Mix `frames` frames from the manager's mixer.
fn pull(m: &SoundPlayer, frames: usize) -> Vec<f32> {
    let mixer = m.mixer().expect("a device");
    let mut out = vec![0.0; frames * 2];
    mixer.lock().expect("mixer").mix(&mut out);
    out
}

fn voices(m: &SoundPlayer) -> usize {
    m.mixer().expect("a device").lock().expect("mixer").active()
}

/// Behaviour: audio.manager.a-sound-starts-on-the-next-turn-at-its-level
#[test]
fn a_sound_starts_on_the_next_turn_and_plays_its_wave_at_its_level() {
    let f = files();
    let mut m = SoundPlayer::new(Some(100));
    let h = m.play(&f, &PlayRequest::new(FLAT)).expect("accepted");
    assert_eq!(pull(&m, 4), vec![0.0; 8], "nothing until the next turn");
    m.use_time(0.0, Listener::default(), &nobody);
    // Full volume on a full slider: 0 millibels, the wave as it is, on both sides.
    assert_eq!(
        pull(&m, 4),
        vec![0.5, 0.5, -0.5, -0.5, 0.25, 0.25, 0.0, 0.0]
    );
    // Half volume: 25 decibels times a half down.
    let mut half = PlayRequest::new(FLAT);
    half.volume = 0.5;
    m.play(&f, &half).expect("accepted");
    m.use_time(0.04, Listener::default(), &nobody);
    let g = gain_from_millibels(-1250);
    assert_eq!(pull(&m, 2), vec![0.5 * g, 0.5 * g, -0.5 * g, -0.5 * g]);
    assert!(!m.has_finished(h));
    m.use_time(0.1, Listener::default(), &nobody);
    m.use_time(0.2, Listener::default(), &nobody);
    assert!(m.has_finished(h), "its wave ended and it went");
    assert!(m.has_finished(0), "no handle is finished");
}

/// Behaviour: audio.manager.the-budget-refuses-the-thirty-third
#[test]
fn a_non_looping_counted_sound_past_the_budget_is_refused() {
    let f = files();
    let mut m = SoundPlayer::new(Some(100));
    // Thirty-three before a turn are all accepted; the turn plays thirty-two and stops the last.
    for _ in 0..33 {
        assert!(m.play(&f, &PlayRequest::new(LONG_FLAT)).is_some());
    }
    m.use_time(0.0, Listener::default(), &nobody);
    assert_eq!(voices(&m), 32);
    m.use_time(0.01, Listener::default(), &nobody);
    assert_eq!(m.playing_count(), 32);
    // With thirty-two playing the next is refused; a looping one is not counted.
    assert_eq!(m.play(&f, &PlayRequest::new(LONG_FLAT)), None);
    assert_eq!(m.stats.over_budget, 1);
    assert!(m.play(&f, &PlayRequest::new(PLACED_LOOPING)).is_some());
    // Asked for alongside anything without counting, it is not refused either.
    let mut uncounted = PlayRequest::new(LONG_FLAT);
    uncounted.kind = PlayType::NonExclusive;
    assert!(m.play(&f, &uncounted).is_some());
}

/// Behaviour: audio.manager.one-at-a-time
#[test]
fn a_self_exclusive_setting_plays_once_at_a_time() {
    let f = files();
    let mut m = SoundPlayer::new(Some(100));
    let h = m
        .play(&f, &PlayRequest::new(ONE_AT_A_TIME))
        .expect("accepted");
    assert_eq!(m.play(&f, &PlayRequest::new(ONE_AT_A_TIME)), None);
    assert_eq!(m.stats.already_playing, 1);
    m.use_time(0.0, Listener::default(), &nobody);
    m.stop(h, 0.0);
    m.use_time(0.1, Listener::default(), &nobody);
    assert!(
        m.play(&f, &PlayRequest::new(ONE_AT_A_TIME)).is_some(),
        "again once it has gone"
    );
}

/// Behaviour: audio.manager.music-off
#[test]
fn music_off_refuses_music_on_the_music_channels_and_stops_what_plays() {
    let f = files();
    let mut m = SoundPlayer::new(Some(100));
    let mut track = PlayRequest::new(TRACK_SETTING);
    track.channel = Channel::Player;
    let h = m.play(&f, &track).expect("accepted");
    m.use_time(0.0, Listener::default(), &nobody);
    assert_eq!(voices(&m), 1);
    m.set_music_off(true);
    m.use_time(0.1, Listener::default(), &nobody);
    assert!(m.has_finished(h), "the music playing stopped");
    assert_eq!(m.play(&f, &track), None);
    track.channel = Channel::Music;
    assert_eq!(m.play(&f, &track), None);
    assert_eq!(m.stats.music_off, 2);
    // On the main channel a music setting still plays, as the screens' music does.
    track.channel = Channel::Main;
    assert!(m.play(&f, &track).is_some());
    m.set_music_off(false);
    track.channel = Channel::Player;
    assert_eq!(
        m.play(&f, &track),
        None,
        "one at a time: the main channel's is playing"
    );
    assert_eq!(stop::MUSIC, 3);
}

/// Behaviour: audio.manager.alone
#[test]
fn a_sound_that_plays_alone_silences_the_others_and_the_music() {
    let f = files();
    let mut m = SoundPlayer::new(Some(100));
    let other = m.play(&f, &PlayRequest::new(LONG_FLAT)).expect("accepted");
    let mut alone = PlayRequest::new(TRACK_SETTING);
    alone.kind = PlayType::AllExclusive;
    alone.looping = Looping::ForceOn;
    let h = m.play(&f, &alone).expect("accepted");
    m.use_time(0.0, Listener::default(), &nobody);
    assert_eq!(voices(&m), 1, "only the one that plays alone");
    assert_eq!(m.volumes().music, 0.0, "the music is silenced");
    let out = pull(&m, 2);
    assert_eq!(out, vec![0.125; 4], "the track's 4096 out of 32768");
    m.stop(h, 0.0);
    m.use_time(0.1, Listener::default(), &nobody);
    assert_eq!(m.volumes().music, 1.0, "the music's slider back");
    assert_eq!(voices(&m), 1, "the other plays again");
    assert!(!m.has_finished(other));
}

/// Behaviour: audio.manager.too-far
#[test]
fn a_sound_250_meters_away_stops_and_a_looping_one_pauses() {
    let f = files();
    let mut m = SoundPlayer::new(Some(100));
    let mut far = PlayRequest::new(PLACED);
    far.emitter = Some(7);
    let gone = m.play(&f, &far).expect("accepted");
    let mut looping = PlayRequest::new(PLACED_LOOPING);
    looping.emitter = Some(8);
    let kept = m.play(&f, &looping).expect("accepted");
    let at = |d: f32| move |e: u64| (e == 7 || e == 8).then_some([0.0, d, 0.0]);
    m.use_time(0.0, Listener::default(), &at(249.0));
    assert_eq!(voices(&m), 2, "under 250 meters both play");
    m.use_time(0.2, Listener::default(), &at(260.0));
    assert_eq!(voices(&m), 0);
    m.use_time(0.4, Listener::default(), &at(10.0));
    assert!(
        m.has_finished(gone),
        "the one that does not loop stopped for good"
    );
    assert!(!m.has_finished(kept));
    assert_eq!(voices(&m), 1, "the looping one plays again once nearer");
}

/// Behaviour: audio.manager.fade
#[test]
fn a_stop_with_a_fade_falls_to_silence_and_then_ends() {
    let f = files();
    let mut m = SoundPlayer::new(Some(100));
    let h = m.play(&f, &PlayRequest::new(LONG_FLAT)).expect("accepted");
    m.use_time(0.0, Listener::default(), &nobody);
    assert_eq!(m.gains(h), Some([1.0, 1.0]));
    m.stop(h, 2.0);
    m.use_time(1.0, Listener::default(), &nobody);
    let g = gain_from_millibels(-4800);
    assert_eq!(m.gains(h), Some([g, g]), "halfway down to -96 decibels");
    assert!(m.playing().iter().any(|p| p.0 == h && p.2 == Life::Fading));
    m.use_time(2.5, Listener::default(), &nobody);
    m.use_time(2.6, Listener::default(), &nobody);
    assert!(m.has_finished(h));
}

/// Behaviour: audio.manager.sliders-reach-sounds-playing
#[test]
fn a_slider_change_reaches_the_sounds_playing() {
    let f = files();
    let mut m = SoundPlayer::new(Some(100));
    let h = m.play(&f, &PlayRequest::new(LONG_FLAT)).expect("accepted");
    m.use_time(0.0, Listener::default(), &nobody);
    m.set_channel_volume(Channel::Main, 0.5);
    let g = gain_from_millibels(-1250);
    assert_eq!(m.gains(h), Some([g, g]));
    assert_eq!(pull(&m, 1), vec![0.25 * g, 0.25 * g]);
    m.set_volume(h, 0.5);
    let g = gain_from_millibels(-1875);
    assert_eq!(m.gains(h), Some([g, g]));
}

fn life(m: &SoundPlayer, h: u32) -> Option<Life> {
    m.playing().iter().find(|p| p.0 == h).map(|p| p.2)
}

/// Behaviour: audio.manager.a-silent-makers-sounds-stop-and-play-again
#[test]
fn a_silent_makers_sounds_stop_and_play_again() {
    let f = files();
    let mut m = SoundPlayer::new(Some(100));
    let mut looping = PlayRequest::new(PLACED_LOOPING);
    looping.emitter = Some(9);
    let held = m.play(&f, &looping).expect("accepted");
    let mut other = PlayRequest::new(PLACED_LOOPING);
    other.emitter = Some(8);
    let beside = m.play(&f, &other).expect("accepted");
    let at = |_: u64| Some([0.0, 1.0, 0.0]);
    m.use_time(0.0, Listener::default(), &at);
    assert_eq!(life(&m, held), Some(Life::Playing));
    // Its maker goes silent: no voice, but the sound is kept. Another maker's plays on.
    m.set_muted([9].into_iter().collect());
    m.use_time(0.1, Listener::default(), &at);
    assert_eq!(life(&m, held), Some(Life::KeepAlive));
    assert_eq!(life(&m, beside), Some(Life::Playing));
    assert_eq!(voices(&m), 1);
    // It stays so while its maker is silent.
    m.use_time(0.2, Listener::default(), &at);
    assert_eq!(life(&m, held), Some(Life::KeepAlive));
    // Heard again, it plays again.
    m.set_muted(std::collections::BTreeSet::new());
    m.use_time(0.3, Listener::default(), &at);
    assert_eq!(life(&m, held), Some(Life::Playing));
    assert_eq!(voices(&m), 2);
}

/// Behaviour: audio.manager.placed-sounds-follow-their-maker
#[test]
fn a_placed_sound_follows_its_maker_a_tenth_of_a_second_at_a_time() {
    let f = files();
    let mut m = SoundPlayer::new(Some(100));
    let mut req = PlayRequest::new(PLACED);
    req.emitter = Some(9);
    let h = m.play(&f, &req).expect("accepted");
    // The listener at the origin looking north; the maker 10 meters north, standing still.
    let at = |p: [f32; 3]| move |e: u64| (e == 9).then_some(p);
    m.use_time(0.0, Listener::default(), &at([0.0, 10.0, 0.0]));
    // A placed path takes four times the slider: still 0 millibels; 2.5 over 10 for the distance.
    assert_eq!(m.gains(h), Some([0.25, 0.25]));
    assert_eq!(m.rate(h), Some(100.0));
    // Moved 5 meters east within the tenth: nothing changes yet.
    m.use_time(0.05, Listener::default(), &at([5.0, 10.0, 0.0]));
    assert_eq!(m.gains(h), Some([0.25, 0.25]));
    // The next tenth: ahead and to the right now, its left side at the cosine of its angle.
    m.use_time(0.15, Listener::default(), &at([10.0, 10.0, 0.0]));
    let d = 2.5 / 200f32.sqrt();
    let [l, r] = m.gains(h).expect("playing");
    assert!((r - d).abs() < 1e-6, "{r} vs {d}");
    assert!((l - d * 10.0 / 200f32.sqrt()).abs() < 1e-6);
    // Coming straight at the listener at 34.3 meters a second.
    m.use_time(0.3, Listener::default(), &at([10.0, 10.0, 0.0]));
    m.use_time(
        0.5,
        Listener::default(),
        &at([10.0 - 4.85, 10.0 - 4.85, 0.0]),
    );
    let rate = m.rate(h).expect("playing");
    assert!((rate - 100.0 / 0.9).abs() < 0.05, "{rate}");
}

/// Behaviour: audio.manager.no-device-plays-nothing
#[test]
fn with_no_device_every_request_is_refused() {
    let f = files();
    let mut m = SoundPlayer::new(None);
    assert_eq!(m.play(&f, &PlayRequest::new(FLAT)), None);
    assert!(m.mixer().is_none());
    m.use_time(0.0, Listener::default(), &nobody);
    assert_eq!(m.stats.requests, 1);
    assert_eq!(m.stats.accepted, 0);
}

/// Behaviour: audio.manager.a-sounds-frequency-changes-while-it-plays
#[test]
fn a_playing_sounds_frequency_changes_its_rate() {
    let f = files();
    let mut m = SoundPlayer::new(Some(100));
    let h = m.play(&f, &PlayRequest::new(LONG_FLAT)).expect("accepted");
    // Not yet playing: no voice to change.
    m.set_frequency(h, 2.0);
    m.use_time(0.0, Listener::default(), &nobody);
    assert_eq!(m.rate(h), Some(100.0));
    m.set_frequency(h, 1.5);
    assert_eq!(m.rate(h), Some(150.0));
    // A change of less than 0.0002, and a negative frequency, are ignored.
    m.set_frequency(h, 1.5001);
    assert_eq!(m.rate(h), Some(150.0));
    m.set_frequency(h, -1.0);
    assert_eq!(m.rate(h), Some(150.0));
    // It goes by the path: an encoded track's music setting plays on the path of sounds heard
    // everywhere, and takes one too.
    let t = m
        .play(&f, &PlayRequest::new(TRACK_SETTING))
        .expect("accepted");
    m.use_time(0.01, Listener::default(), &nobody);
    assert_eq!(m.rate(t), Some(100.0));
    m.set_frequency(t, 2.0);
    assert_eq!(m.rate(t), Some(200.0));
}

/// Behaviour: audio.options.sliders-at-once-the-rest-when-applied
#[test]
fn the_sound_sliders_take_effect_as_they_change_and_the_rest_when_applied() {
    use auberean_audio::options::key;
    let f = files();
    let mut m = SoundPlayer::new(Some(100));
    m.handle_change(key::SOUND_VOLUME, 0.5);
    m.handle_change(key::MUSIC_VOLUME, 0.25);
    m.handle_change(key::PLAYER_MUSIC_VOLUME, 0.75);
    m.handle_change(key::AMBIENT_VOLUME, 0.125);
    let v = m.volumes();
    assert_eq!(
        (v.main, v.music, v.player, v.ambient),
        (0.5, 0.25, 0.75, 0.125)
    );
    // A slider applied changes nothing: it took effect as it changed.
    m.handle_apply(key::SOUND_VOLUME, 0.1);
    assert_eq!(m.volumes().main, 0.5);
    // Music off changed but not applied: music still plays.
    m.handle_change(key::MUSIC_OFF, 1.0);
    let mut music = PlayRequest::new(TRACK_SETTING);
    music.channel = Channel::Music;
    assert!(m.play(&f, &music).is_some());
    // Applied: the music stops and is refused.
    m.handle_apply(key::MUSIC_OFF, 1.0);
    assert!(m.play(&f, &music).is_none());
    // The features are kept when applied.
    m.handle_change(key::FEATURES, 2.0);
    assert_eq!(m.features(), 1, "hardware, the default, until applied");
    m.handle_apply(key::FEATURES, 2.0);
    assert_eq!(m.features(), 2);
    // With no device nothing is taken.
    let mut none = SoundPlayer::new(None);
    none.handle_change(key::SOUND_VOLUME, 0.5);
    assert_eq!(none.volumes().main, 1.0);
}

/// Behaviour: audio.options.playing-sounds-held-to-sixteen-to-thirty-two
#[test]
fn the_number_of_sounds_is_its_whole_part_held_between_sixteen_and_thirty_two() {
    use auberean_audio::options::key;
    let mut m = SoundPlayer::new(Some(100));
    assert_eq!(m.budget(), 32, "the default");
    for (value, budget) in [(8.0, 16), (16.0, 16), (20.7, 20), (32.0, 32), (64.0, 32)] {
        m.handle_change(key::PLAYING_SOUNDS, value);
        assert_eq!(m.budget(), budget, "{value}");
    }
}

/// Behaviour: audio.options.sound-off-silences-everything
#[test]
fn sound_off_silences_every_sound_until_it_is_on_again() {
    use auberean_audio::options::key;
    let f = files();
    let mut m = SoundPlayer::new(Some(100));
    let h = m.play(&f, &PlayRequest::new(LONG_FLAT)).expect("accepted");
    m.use_time(0.0, Listener::default(), &nobody);
    assert!(pull(&m, 4).iter().any(|v| *v != 0.0));
    m.handle_apply(key::SOUND_OFF, 1.0);
    assert!(m.sound_off());
    assert_eq!(pull(&m, 4), vec![0.0; 8]);
    // A sound asked for while off lives on without a sound.
    let later = m.play(&f, &PlayRequest::new(LONG_FLAT)).expect("accepted");
    m.use_time(0.01, Listener::default(), &nobody);
    assert_eq!(pull(&m, 4), vec![0.0; 8]);
    assert!(!m.has_finished(h) && !m.has_finished(later));
    m.handle_apply(key::SOUND_OFF, 0.0);
    assert!(pull(&m, 4).iter().any(|v| *v != 0.0));
}

/// Behaviour: audio.options.the-sound-section
#[test]
fn the_sound_section_holds_eight_preferences_in_the_managers_order() {
    use auberean_audio::options::{key, Kind, PREFERENCES, SECTION};
    assert_eq!(SECTION, 0x4000_0007);
    let keys: Vec<u32> = PREFERENCES.iter().map(|p| p.key).collect();
    assert_eq!(
        keys,
        vec![
            key::SOUND_VOLUME,
            key::MUSIC_VOLUME,
            key::PLAYER_MUSIC_VOLUME,
            key::AMBIENT_VOLUME,
            key::FEATURES,
            key::PLAYING_SOUNDS,
            key::SOUND_OFF,
            key::MUSIC_OFF,
        ]
    );
    assert_eq!(
        PREFERENCES[5].kind,
        Kind::Number {
            least: 8.0,
            greatest: 64.0,
            default: 32.0
        }
    );
    assert_eq!(PREFERENCES[4].kind, Kind::Features { default: 1 });
}
