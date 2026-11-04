//! The sound manager: the one way every sound is played, and what it does with the sounds playing
//! each frame.
//!
//! A request ([`PlayRequest`]) names a sound setting or a music setting. The manager refuses it
//! when there is no sound device, when a sound of the same setting that plays one at a time is
//! already playing, when it is music on the music or player music channel and music is turned
//! off, or when it is a sound counted against the budget (of 32 sounds) that does not loop and the
//! budget is spent. Otherwise it is given a handle and joins the sounds playing; it starts with the
//! next frame's turn.
//!
//! Each frame ([`SoundPlayer::use_time`]) the sounds that are done go; each playing sound has its
//! turn (its length run out, its fade over); each is placed relative to the listener and given its
//! priority (its setting's priority against its distance: lower first); and then, in priority
//! order, either the first plays alone (a sound that plays alone silences every other sound and
//! the music), or each sound plays unless it is 250 meters or more away or the budget is spent
//! (a looping sound is only paused, any other stopped).
//!
//! A piece of music made of instruments plays on the performance ([`crate::music::perf`]): a
//! placed piece on a path of its own, a piece heard everywhere on the one path all such music
//! shares. It starts on the boundary its setting says and stops on the one it says; it is done
//! when the piece ends. Pieces on the primary track play one at a time: one asked for while
//! another plays waits for it to end (replacing any already waiting), and the one playing starts
//! again when it ends with none waiting, until it is stopped.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use auberean_assets::sound::{MusicInfo, SoundInfo};
use dereth_audio::voices::{gain_from_millibels, Mixer, Source, Stream, VoiceId, VoiceSettings};
use dereth_primitives::num::{to_i32, to_i32_f64};

use crate::music::library::{Library, Piece};
use crate::music::perf::{Boundary, PathId, Performance, PlayOptions, Rules, StateId};
use crate::music::synth::RATE as MUSIC_RATE;
use crate::records::{self, SettingType, SoundFiles};
use crate::request::{Channel, PathKind, PlayRequest, PlayType, Timing, Track};
use crate::settings::{self, PlaySettings, Setting, SOUND_LOOPS};
use crate::space::{Listener, Placement, Vec3, COMMIT_INTERVAL, MAX_FREQUENCY};
use crate::volume::{self, ChannelVolumes, SILENT_MB};

/// How many sounds counted against the budget may play at once, by default (the preference's
/// default; 8 to 64).
pub const DEFAULT_BUDGET: u32 = 32;

/// A sound this far away or more, by its priority-weighted distance, does not play.
pub const TOO_FAR: f32 = 250.0;

/// Seconds a wave's playing sound outlives its wave.
pub const LENGTH_MARGIN: f32 = 0.5;

/// How many voices the mixer has.
pub const VOICES: usize = 128;

/// The most sounds the device lets the budget count.
pub const DEVICE_SOUNDS: u32 = 32;

/// The fewest sounds the budget counts, whatever the preference says.
pub const LEAST_BUDGET: u32 = 16;

/// Which sounds [`SoundPlayer::stop_sounds`] stops: music heard everywhere, music placed, sounds
/// heard everywhere, sounds placed.
pub mod stop {
    pub const FLAT_MUSIC: u32 = 0x1;
    pub const PLACED_MUSIC: u32 = 0x2;
    pub const FLAT_SOUNDS: u32 = 0x4;
    pub const PLACED_SOUNDS: u32 = 0x8;
    /// All music, as turning music off stops it.
    pub const MUSIC: u32 = FLAT_MUSIC | PLACED_MUSIC;
}

/// Where an emitter is: its place in the world's axes, by its object's id (`None` when it is not
/// where the listener's world is drawn; its last place stands).
// D2-06032
pub type Emitters<'a> = &'a dyn Fn(u64) -> Option<Vec3>;

/// What the manager did, in numbers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoundStats {
    /// Requests made.
    pub requests: u64,
    /// Requests that became playing sounds.
    pub accepted: u64,
    /// Refused: the budget was spent.
    pub over_budget: u64,
    /// Refused: music was off.
    pub music_off: u64,
    /// Refused: the setting was missing, or its wave or track would not decode.
    pub missing: u64,
    /// Not played: one of the same setting that plays one at a time was playing.
    pub already_playing: u64,
    /// Voices started in the mixer.
    pub voices: u64,
    /// Starts the mixer refused, every voice busy.
    pub mixer_full: u64,
    /// Pieces of music made of instruments started on the performance.
    pub pieces: u64,
}

/// A playing sound's life: kept (made, paused or waiting its turn), playing, fading out, waiting
/// for its end, or done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Life {
    KeepAlive,
    Playing,
    Fading,
    Ending,
    Done,
}

#[derive(Clone, Copy, Debug)]
struct Fade {
    start: f64,
    seconds: f32,
    die: bool,
    from: i32,
}

/// A decoded setting and what it plays, kept for reuse.
#[derive(Clone, Debug)]
struct Instance {
    sound: Option<SoundInfo>,
    music: Option<MusicInfo>,
    /// What it plays; `None` for music made of instruments.
    source: Option<Source>,
    /// The piece of music made of instruments it plays, with what the piece needs.
    piece: Option<Arc<Piece>>,
    /// A wave's length in seconds.
    seconds: f32,
    last_used: f64,
}

/// One playing sound.
#[derive(Debug)]
struct Playing {
    handle: u32,
    id: u32,
    settings: PlaySettings,
    path: PathKind,
    source: Option<Source>,
    voice: Option<VoiceId>,
    life: Life,
    has_played: bool,
    disabled: bool,
    /// Its maker is silent: it neither starts nor goes on until the maker is heard.
    muted: bool,
    start: f64,
    /// Seconds it lasts from its start (its wave's length and a margin), below 0 for music.
    length: f32,
    volume: f32,
    fade: Option<Fade>,
    frequency: f32,
    /// Its place and velocity relative to the listener, in the device's axes.
    local: Vec3,
    velocity: Vec3,
    /// Its maker's last place in the world, and when.
    last_world: Option<(Vec3, f64)>,
    priority: f32,
    placement: Placement,
    /// Its piece on the performance, when it plays music made of instruments.
    music: Option<MusicRun>,
}

/// A piece of music made of instruments as a sound plays it: its path, whether that path is the
/// one shared by music heard everywhere, and the piece playing on it.
#[derive(Clone, Copy, Debug)]
struct MusicRun {
    path: PathId,
    shared: bool,
    state: Option<StateId>,
}

/// A path's frames as the mixer reads them, from the performance.
#[derive(Debug)]
struct PathStream {
    perf: Arc<Mutex<Performance>>,
    path: PathId,
    mono: bool,
    scratch: Vec<f32>,
}

impl Stream for PathStream {
    fn rate(&self) -> u32 {
        MUSIC_RATE
    }

    fn read(&mut self, out: &mut [f32]) -> usize {
        let frames = out.len() / 2;
        let Ok(mut perf) = self.perf.lock() else {
            out.fill(0.0);
            return frames;
        };
        if self.mono {
            self.scratch.resize(frames, 0.0);
            perf.read(self.path, &mut self.scratch);
            for (f, x) in self.scratch.iter().enumerate() {
                out[2 * f] = *x;
                out[2 * f + 1] = *x;
            }
        } else {
            perf.read(self.path, &mut out[..frames * 2]);
        }
        // A path's stream never ends; its voice is stopped when its sound is done.
        frames
    }
}

/// The performance and what the manager keeps for it.
#[derive(Debug)]
struct MusicEngine {
    perf: Arc<Mutex<Performance>>,
    library: Option<Library>,
    /// The path all music heard everywhere shares, and its voice.
    flat: Option<(PathId, Option<VoiceId>)>,
    /// The primary track: the piece playing and the one waiting, by handle.
    primary: Option<u32>,
    next_primary: Option<u32>,
}

impl Playing {
    // D2-07765
    fn budgeted(&self) -> bool {
        matches!(
            self.settings.kind,
            PlayType::NonExclusiveBudgeted | PlayType::SelfExclusiveBudgeted
        )
    }

    // D2-07763
    fn self_exclusive(&self) -> bool {
        matches!(
            self.settings.kind,
            PlayType::SelfExclusive | PlayType::SelfExclusiveBudgeted
        )
    }

    // D2-07764
    fn all_exclusive(&self) -> bool {
        self.settings.kind == PlayType::AllExclusive
    }

    // D2-07754
    fn is_playing(&self) -> bool {
        self.has_played && matches!(self.life, Life::Playing | Life::Fading | Life::KeepAlive)
    }

    // D2-07760
    fn looping(&self) -> bool {
        self.settings.looping
    }
}

/// The sound manager; what it plays and holds goes with it.
// D2-07719
#[derive(Debug)]
pub struct SoundPlayer {
    /// The mixer the device plays; `None` with no device, when nothing plays.
    mixer: Option<Arc<Mutex<Mixer>>>,
    volumes: ChannelVolumes,
    /// The music slider kept while a sound that plays alone has the music at 0.
    music_volume: f32,
    music_off: bool,
    /// The sound is off: nothing is heard.
    sound_off: bool,
    /// The sound features chosen.
    features: i32,
    budget: u32,
    /// Sounds counted against the budget that do not loop, playing as of the last turn.
    budgeted_playing: u32,
    next_handle: u32,
    /// The settings of sounds that play one at a time, playing.
    exclusive: BTreeSet<u32>,
    instances: HashMap<u32, Instance>,
    playing: Vec<Playing>,
    listener: Listener,
    /// The objects whose sounds are silent.
    muted: BTreeSet<u64>,
    last_commit: f64,
    now: f64,
    music: MusicEngine,
    pub stats: SoundStats,
}

impl SoundPlayer {
    /// A sound manager playing through a mixer at `rate` frames a second, or, with no device
    /// (`None`), one that plays nothing. The preferences' defaults: every slider at 1, music on, a
    /// budget of 32.
    // D2-07716, D2-07685, D2-07680
    #[must_use]
    pub fn new(rate: Option<u32>) -> Self {
        let volumes = ChannelVolumes::default();
        Self {
            mixer: rate.map(|r| Arc::new(Mutex::new(Mixer::new(r, VOICES)))),
            music_volume: volumes.music,
            volumes,
            music_off: false,
            sound_off: false,
            features: 1,
            budget: DEFAULT_BUDGET,
            budgeted_playing: 0,
            next_handle: 1,
            exclusive: BTreeSet::new(),
            instances: HashMap::new(),
            playing: Vec::new(),
            listener: Listener::default(),
            muted: BTreeSet::new(),
            last_commit: f64::NEG_INFINITY,
            now: 0.0,
            music: MusicEngine {
                perf: Arc::new(Mutex::new(Performance::new(
                    MUSIC_RATE,
                    Rules::default(),
                    0x5EED,
                ))),
                library: None,
                flat: None,
                primary: None,
                next_primary: None,
            },
            stats: SoundStats::default(),
        }
    }

    /// The performance music made of instruments plays on.
    #[must_use]
    pub fn performance(&self) -> Arc<Mutex<Performance>> {
        self.music.perf.clone()
    }

    /// Set the master groove level, which the music's patterns and scripts follow.
    // D2-07684, D2-07789
    pub fn set_groove(&mut self, level: i32) {
        if let Ok(mut p) = self.music.perf.lock() {
            p.set_master_groove(level);
        }
    }

    /// The master groove level.
    #[must_use]
    pub fn groove(&self) -> i32 {
        self.music.perf.lock().map_or(0, |p| p.master_groove())
    }

    /// The mixer the device fills its blocks from, when there is a device.
    #[must_use]
    pub fn mixer(&self) -> Option<Arc<Mutex<Mixer>>> {
        self.mixer.clone()
    }

    /// The sliders.
    #[must_use]
    pub fn volumes(&self) -> ChannelVolumes {
        self.volumes
    }

    /// How many sounds are playing (made and not yet done).
    #[must_use]
    pub fn playing_count(&self) -> usize {
        self.playing.len()
    }

    /// The settings of the sounds playing, by handle, with their lives.
    #[must_use]
    pub fn playing(&self) -> Vec<(u32, u32, Life)> {
        self.playing
            .iter()
            .map(|p| (p.handle, p.id, p.life))
            .collect()
    }

    /// The objects whose sounds are silent from the next turn on: each sound playing stops, kept
    /// to play again; the rest, when an object is no longer among them.
    // D2-06153
    pub fn set_muted(&mut self, muted: BTreeSet<u64>) {
        self.muted = muted;
    }

    /// The objects making the sounds playing.
    #[must_use]
    pub fn emitters(&self) -> Vec<u64> {
        let mut e: Vec<u64> = self
            .playing
            .iter()
            .filter_map(|p| p.settings.emitter)
            .collect();
        e.sort_unstable();
        e.dedup();
        e
    }

    /// A playing sound's left and right gains as last set, by handle.
    #[must_use]
    pub fn gains(&self, handle: u32) -> Option<[f32; 2]> {
        let p = self.playing.iter().find(|p| p.handle == handle)?;
        Some(self.voice_settings(p).gains)
    }

    /// The rate a playing sound's wave is read at, by handle.
    #[must_use]
    pub fn rate(&self, handle: u32) -> Option<f64> {
        let p = self.playing.iter().find(|p| p.handle == handle)?;
        Some(self.voice_settings(p).rate)
    }

    /// Play a request: its handle, or `None` when it was refused or one of the same setting that
    /// plays one at a time is already playing.
    // D2-07723, D2-07721, D2-07722
    pub fn play(&mut self, files: &dyn SoundFiles, req: &PlayRequest) -> Option<u32> {
        self.stats.requests += 1;
        tracing::debug!(
            id = format_args!("{:08X}", req.id),
            emitter = ?req.emitter,
            volume = req.volume,
            channel = ?req.channel,
            "sound asked for"
        );
        self.mixer.as_ref()?;
        if self.exclusive.contains(&req.id) {
            self.stats.already_playing += 1;
            return None;
        }
        if req.id == 0 {
            return None;
        }
        if self.music_off && matches!(req.channel, Channel::Music | Channel::Player) {
            self.stats.music_off += 1;
            return None;
        }
        let handle = self.next_handle;
        self.next_handle = self.next_handle.wrapping_add(1).max(1);
        let instance = match self.instances.get_mut(&req.id) {
            Some(i) => {
                if !Self::check_budget(self.budget, self.budgeted_playing, req, i.sound.as_ref()) {
                    self.stats.over_budget += 1;
                    return None;
                }
                i.last_used = self.now;
                i.clone()
            }
            None => {
                let i = match self.load(files, req) {
                    Ok(i) => i,
                    Err(refusal) => {
                        match refusal {
                            Refusal::Budget => self.stats.over_budget += 1,
                            Refusal::Missing(why) => {
                                self.stats.missing += 1;
                                tracing::debug!(id = format_args!("{:08X}", req.id), "{why}");
                            }
                        }
                        return None;
                    }
                };
                self.add_instance(req.id, i.clone());
                i
            }
        };
        let (settings, path) = match (&instance.sound, &instance.music) {
            (Some(s), _) => (
                settings::from_sound_info(req, s, instance.seconds),
                settings::path_kind(Setting::Sound(s)),
            ),
            (None, Some(m)) => (
                settings::from_music_info(req, m),
                settings::path_kind(Setting::Music(m)),
            ),
            (None, None) => return None,
        };
        let p = Playing {
            handle,
            id: req.id,
            path,
            length: settings.length.map_or(-1.0, |l| l + LENGTH_MARGIN),
            volume: settings.volume,
            frequency: settings.frequency,
            settings,
            music: instance.piece.as_ref().map(|_| MusicRun {
                path: PathId(0),
                shared: path == PathKind::FlatMusic,
                state: None,
            }),
            source: instance.source,
            voice: None,
            life: Life::KeepAlive,
            has_played: false,
            disabled: false,
            muted: false,
            start: self.now,
            fade: None,
            local: [0.0; 3],
            velocity: [0.0; 3],
            last_world: None,
            priority: 1.0,
            placement: Placement::AT_LISTENER,
        };
        if p.self_exclusive() {
            self.exclusive.insert(req.id);
        }
        self.playing.push(p);
        self.stats.accepted += 1;
        Some(handle)
    }

    /// A sound setting counted against the budget that does not loop is refused when the budget is
    /// spent; nothing else is.
    // D2-07688
    fn check_budget(budget: u32, playing: u32, req: &PlayRequest, s: Option<&SoundInfo>) -> bool {
        let Some(s) = s else { return true };
        let counted = matches!(
            req.kind,
            PlayType::NonExclusiveBudgeted | PlayType::SelfExclusiveBudgeted | PlayType::Inherit
        );
        !(!s.test_bit_mask(SOUND_LOOPS) && counted && budget <= playing)
    }

    fn load(&mut self, files: &dyn SoundFiles, req: &PlayRequest) -> Result<Instance, Refusal> {
        let missing = |why: String| Refusal::Missing(why);
        match records::setting_type(req.id) {
            Some(SettingType::Sound) => {
                let s = records::sound_info(files, req.id)
                    .ok_or_else(|| missing("no such sound setting".into()))?;
                if !Self::check_budget(self.budget, self.budgeted_playing, req, Some(&s)) {
                    return Err(Refusal::Budget);
                }
                let (source, seconds) = records::sound_source(files, &s).map_err(missing)?;
                Ok(Instance {
                    sound: Some(s),
                    music: None,
                    piece: None,
                    source: Some(source),
                    seconds,
                    last_used: self.now,
                })
            }
            Some(SettingType::Music) => {
                let m = records::music_info(files, req.id)
                    .ok_or_else(|| missing("no such music setting".into()))?;
                let source = records::music_source(files, &m).map_err(missing)?;
                let piece = if source.is_none() {
                    let library = self
                        .music
                        .library
                        .get_or_insert_with(|| Library::new(files));
                    Some(Arc::new(library.piece(files, m.music).map_err(missing)?))
                } else {
                    None
                };
                Ok(Instance {
                    sound: None,
                    music: Some(m),
                    piece,
                    source,
                    seconds: 0.0,
                    last_used: self.now,
                })
            }
            None => Err(missing("not a sound or music setting".into())),
        }
    }

    /// Keep a decoded setting; past four times the budget the least recently used goes.
    // D2-07718, D2-07683
    fn add_instance(&mut self, id: u32, i: Instance) {
        if self.instances.len() > (self.budget as usize) * 4 {
            if let Some(oldest) = self
                .instances
                .iter()
                .min_by(|a, b| a.1.last_used.total_cmp(&b.1.last_used))
                .map(|(k, _)| *k)
            {
                self.instances.remove(&oldest);
            }
        }
        self.instances.insert(id, i);
    }

    /// Whether the sound of `handle` is done (a handle of 0 is).
    // D2-07697, D2-07706
    #[must_use]
    pub fn has_finished(&self, handle: u32) -> bool {
        handle == 0 || !self.playing.iter().any(|p| p.handle == handle)
    }

    /// Stop the sound of `handle`, fading out over `fade` seconds (at once for 0); it is done
    /// when it has stopped.
    // D2-01716, D2-07783, D2-07698
    pub fn stop(&mut self, handle: u32, fade: f32) {
        if let Some(i) = self.playing.iter().position(|p| p.handle == handle) {
            self.stop_internal(i, fade, true, false);
        }
    }

    /// Change the volume of the sound of `handle`.
    // D2-06888, D2-07779
    pub fn set_volume(&mut self, handle: u32, volume: f32) {
        if let Some(i) = self.playing.iter().position(|p| p.handle == handle) {
            let v = volume.clamp(0.0, 1.0);
            if (v - self.playing[i].volume).abs() >= 0.0002 {
                self.playing[i].volume = v;
                self.apply(i);
            }
        }
    }

    /// Change the frequency the sound of `handle` plays at (1 plays it as recorded): only a sound
    /// setting's sound with a voice, never music; a negative frequency, or a change of less than
    /// 0.0002, is ignored.
    // D2-07774
    pub fn set_frequency(&mut self, handle: u32, frequency: f32) {
        let Some(i) = self.playing.iter().position(|p| p.handle == handle) else {
            return;
        };
        let p = &self.playing[i];
        if !matches!(p.path, PathKind::FlatSound | PathKind::PlacedSound)
            || p.voice.is_none()
            || frequency < 0.0
            || (p.frequency - frequency).abs() < 0.0002
        {
            return;
        }
        self.playing[i].frequency = frequency;
        self.apply(i);
    }

    /// Stop the sounds a mask names ([`stop`]); how many.
    // D2-07695
    pub fn stop_sounds(&mut self, mask: u32) -> u32 {
        let mut n = 0;
        for i in 0..self.playing.len() {
            let p = &self.playing[i];
            let hit = match self.instances.get(&p.id) {
                Some(Instance { sound: Some(s), .. }) => {
                    if s.test_bit_mask(settings::SOUND_PLACED) {
                        mask & stop::PLACED_SOUNDS != 0
                    } else {
                        mask & stop::FLAT_SOUNDS != 0
                    }
                }
                Some(Instance { music: Some(m), .. }) => {
                    if m.test_bit_mask(settings::MUSIC_PLACED) {
                        mask & stop::PLACED_MUSIC != 0
                    } else {
                        mask & stop::FLAT_MUSIC != 0
                    }
                }
                _ => match p.path {
                    PathKind::PlacedSound => mask & stop::PLACED_SOUNDS != 0,
                    PathKind::FlatSound => mask & stop::FLAT_SOUNDS != 0,
                    PathKind::PlacedMusic => mask & stop::PLACED_MUSIC != 0,
                    PathKind::FlatMusic => mask & stop::FLAT_MUSIC != 0,
                },
            };
            if hit {
                self.stop_internal(i, 0.0, true, false);
                n += 1;
            }
        }
        n
    }

    /// Stop at once every sound an object makes; how many.
    // D2-07696
    pub fn stop_for_emitter(&mut self, emitter: u64) -> u32 {
        let mut n = 0;
        for i in 0..self.playing.len() {
            if self.playing[i].settings.emitter == Some(emitter) {
                self.stop_internal(i, 0.0, true, true);
                n += 1;
            }
        }
        n
    }

    /// Set a channel's slider (0 to 1); every playing sound takes it at once. The music slider is
    /// kept for when a sound that plays alone has finished.
    // D2-07699
    pub fn set_channel_volume(&mut self, channel: Channel, value: f32) {
        if channel == Channel::Music && (0.0..=1.0).contains(&value) {
            self.music_volume = value;
        }
        self.set_device_volume(channel, value);
    }

    // D2-07794, D2-07780
    fn set_device_volume(&mut self, channel: Channel, value: f32) {
        if self.volumes.of(channel) == value {
            return;
        }
        if self.volumes.set(channel, value) {
            for i in 0..self.playing.len() {
                self.apply(i);
            }
        }
    }

    /// Turn music off (every piece of music stops, and music is refused) or on.
    // D2-07699
    pub fn set_music_off(&mut self, off: bool) {
        self.music_off = off;
        if off {
            self.stop_sounds(stop::MUSIC);
        }
    }

    /// Turn the sound off (nothing is heard: the device is let go of, and every sound keeps its
    /// life without a sound) or on again (every sound heard again).
    // D2-07818
    pub fn set_sound_off(&mut self, off: bool) {
        if self.sound_off == off {
            return;
        }
        self.sound_off = off;
        for i in 0..self.playing.len() {
            self.apply(i);
        }
    }

    /// Whether the sound is off.
    #[must_use]
    pub fn sound_off(&self) -> bool {
        self.sound_off
    }

    /// How many sounds counted against the budget may play at once: the preference's whole part,
    /// at most the device's [`DEVICE_SOUNDS`], and at least [`LEAST_BUDGET`].
    // D2-07699
    pub fn set_playing_sounds(&mut self, value: f32) {
        let n = u32::try_from(to_i32(value)).unwrap_or(u32::MAX);
        self.budget = n.clamp(LEAST_BUDGET, DEVICE_SOUNDS);
    }

    /// How many sounds counted against the budget may play at once.
    #[must_use]
    pub fn budget(&self) -> u32 {
        self.budget
    }

    /// The sound features chosen ([`crate::options::FEATURES`]): kept; every choice plays alike
    /// here, through the one mixer.
    #[must_use]
    pub fn features(&self) -> i32 {
        self.features
    }

    /// A sound preference changed (`value`: the number, the features' number, or 1 or 0 for yes
    /// or no): the volume sliders and the number of sounds take it at once; the others wait to be
    /// applied. Nothing without a device.
    // D2-07701, D2-07699
    pub fn handle_change(&mut self, key: u32, value: f32) {
        self.handle_preference(key, value, false);
    }

    /// A sound preference applied: sound off, music off and the features take it. Nothing without
    /// a device.
    // D2-07700
    pub fn handle_apply(&mut self, key: u32, value: f32) {
        self.handle_preference(key, value, true);
    }

    // D2-07699
    fn handle_preference(&mut self, key: u32, value: f32, applied: bool) {
        use crate::options::key as k;
        if self.mixer.is_none() {
            return;
        }
        match (key, applied) {
            (k::SOUND_OFF, true) => self.set_sound_off(value != 0.0),
            (k::MUSIC_OFF, true) => self.set_music_off(value != 0.0),
            (k::PLAYER_MUSIC_VOLUME, false) => self.set_channel_volume(Channel::Player, value),
            (k::SOUND_VOLUME, false) => self.set_channel_volume(Channel::Main, value),
            (k::MUSIC_VOLUME, false) => self.set_channel_volume(Channel::Music, value),
            (k::AMBIENT_VOLUME, false) => self.set_channel_volume(Channel::Ambient, value),
            // D2-07786
            (k::FEATURES, true) => self.features = to_i32(value),
            (k::PLAYING_SOUNDS, false) => self.set_playing_sounds(value),
            _ => {}
        }
    }

    /// The frame's turn: the sounds that are done go; each playing sound's turn; each placed
    /// relative to `listener` (emitters found by `emitters`); then the sounds played in priority
    /// order.
    // D2-07720
    pub fn use_time(&mut self, now: f64, listener: Listener, emitters: Emitters<'_>) {
        self.now = now;
        if self.mixer.is_none() {
            return;
        }
        self.flush();
        // The device's turn: what ended, then each sound's own turn.
        // D2-07797
        for i in 0..self.playing.len() {
            self.ended(i);
            self.sound_use_time(i);
        }
        self.primary_turn();
        self.update_listener(listener, emitters);
        if self.playing.is_empty() {
            return;
        }
        // D2-07682
        self.playing
            .sort_by(|a, b| a.priority.total_cmp(&b.priority));
        if self.playing[0].all_exclusive() {
            for i in 1..self.playing.len() {
                self.enable(i, false);
            }
            self.set_device_volume(Channel::Music, 0.0);
            if !self.playing[0].is_playing() {
                self.play_internal(0);
            }
            self.enable(0, true);
            return;
        }
        self.set_device_volume(Channel::Music, self.music_volume);
        self.allocate(false);
        self.allocate(true);
    }

    /// The sounds that are done go, and with them the settings they kept from playing again.
    // D2-07717
    fn flush(&mut self) {
        let mut i = 0;
        while i < self.playing.len() {
            if self.playing[i].life == Life::Done {
                let p = self.playing.swap_remove(i);
                if p.self_exclusive() {
                    self.exclusive.remove(&p.id);
                }
                self.release(&p);
            } else {
                i += 1;
            }
        }
    }

    /// In priority order: a sound too far away, or one past the budget, stops (or, looping,
    /// pauses) in the first pass; any other plays in the second. The budget counts what the second
    /// pass played that does not loop.
    // D2-07687
    fn allocate(&mut self, second: bool) {
        let mut counted = 0u32;
        let mut not_looping = 0u32;
        for i in 0..self.playing.len() {
            let p = &self.playing[i];
            let over = p.priority >= TOO_FAR || (p.budgeted() && self.budget <= counted);
            if over {
                if !second {
                    if self.playing[i].looping() {
                        self.enable(i, false);
                    } else {
                        self.stop_internal(i, 0.0, true, false);
                    }
                }
                continue;
            }
            if second {
                if !self.playing[i].is_playing() {
                    self.play_internal(i);
                }
                self.enable(i, true);
            }
            let p = &self.playing[i];
            if p.budgeted() {
                counted += 1;
                if !p.looping() {
                    not_looping += 1;
                }
            }
        }
        self.budgeted_playing = not_looping;
    }

    /// Each sound's place relative to the listener, and its priority; the device hears the places
    /// once a tenth of a second.
    // D2-07796
    fn update_listener(&mut self, listener: Listener, emitters: Emitters<'_>) {
        self.listener = listener;
        for i in 0..self.playing.len() {
            self.update_position(i, emitters);
        }
        if self.now - self.last_commit > COMMIT_INTERVAL {
            self.last_commit = self.now;
            for i in 0..self.playing.len() {
                self.commit(i);
            }
        }
    }

    /// A sound's place and velocity relative to the listener (from its maker's place now and the
    /// last time), and its priority: its distance (1 with no maker) times two less its setting's
    /// priority; a sound that plays alone comes first.
    // D2-07772
    fn update_position(&mut self, i: usize, emitters: Emitters<'_>) {
        let now = self.now;
        let listener = self.listener;
        let p = &mut self.playing[i];
        if matches!(p.life, Life::Ending | Life::Done) {
            return;
        }
        let mut distance = 1.0;
        if let Some(e) = p.settings.emitter {
            if let Some(world) = emitters(e) {
                let velocity = match p.last_world {
                    Some((was, at)) if now > at => {
                        #[allow(clippy::cast_possible_truncation)]
                        // LINT-OK: a frame's length in seconds, a fraction.
                        let dt = (now - at) as f32;
                        [
                            (world[0] - was[0]) / dt,
                            (world[1] - was[1]) / dt,
                            (world[2] - was[2]) / dt,
                        ]
                    }
                    _ => [0.0; 3],
                };
                p.last_world = Some((world, now));
                p.local = listener.local(world);
                p.velocity = listener.turn(velocity);
            }
            let l = p.local;
            distance = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
        }
        p.priority = if p.all_exclusive() {
            -1.0
        } else {
            (2.0 - p.settings.priority) * distance
        };
    }

    /// What the device makes of a sound's place, set on its voice.
    // D2-07753
    fn commit(&mut self, i: usize) {
        let p = &mut self.playing[i];
        p.placement = if p.path.placed() {
            Placement::of(p.local, p.velocity, p.settings.min_distance)
        } else {
            Placement::AT_LISTENER
        };
        self.apply(i);
    }

    /// The millibels a sound plays at now: its channel's level and its volume, falling to silence
    /// over its fade.
    fn millibels(&self, p: &Playing) -> i32 {
        let level = volume::channel_level(&self.volumes, p.settings.channel, p.path.placed());
        let mb = volume::millibels(level, p.volume);
        match p.fade {
            Some(f) if p.life == Life::Fading && f.seconds > 0.0 => {
                let t = ((self.now - f.start) / f64::from(f.seconds)).clamp(0.0, 1.0);
                let from = f64::from(f.from);
                to_i32_f64(from + (f64::from(SILENT_MB) - from) * t)
            }
            _ => mb,
        }
    }

    fn voice_settings(&self, p: &Playing) -> VoiceSettings {
        let g = if self.sound_off {
            0.0
        } else {
            gain_from_millibels(self.millibels(p)) * p.placement.distance
        };
        let base = match (&p.source, p.music) {
            (Some(s), _) => f64::from(s.rate()),
            (None, Some(_)) => f64::from(MUSIC_RATE),
            (None, None) => 0.0,
        };
        // D2-07758
        let rate = f64::from(to_i32_f64(base * f64::from(p.frequency)).max(0))
            * f64::from(p.placement.doppler);
        VoiceSettings {
            gains: [g * p.placement.sides[0], g * p.placement.sides[1]],
            rate: rate.min(MAX_FREQUENCY),
            looping: p.looping(),
        }
    }

    /// Set a sound's gains and rate on its voice.
    // D2-07773
    fn apply(&mut self, i: usize) {
        let p = &self.playing[i];
        let Some(v) = p.voice else { return };
        let s = self.voice_settings(p);
        if let Some(m) = &self.mixer {
            if let Ok(mut m) = m.lock() {
                m.set_gains(v, s.gains);
                m.set_rate(v, s.rate);
            }
        }
    }

    /// Start a sound kept waiting: its voice in the mixer, placed at once, at its volume and rate.
    // D2-07777
    fn play_internal(&mut self, i: usize) {
        let now = self.now;
        {
            let p = &mut self.playing[i];
            if p.life != Life::KeepAlive {
                return;
            }
            if !p.has_played {
                p.start = now;
                p.has_played = true;
            }
            if p.disabled || p.muted {
                return;
            }
        }
        self.commit_placement_now(i);
        if self.playing[i].music.is_some() {
            self.play_music(i);
            return;
        }
        let p = &self.playing[i];
        let settings = self.voice_settings(p);
        let voice = match (&p.source, &self.mixer) {
            (Some(src), Some(m)) => m
                .lock()
                .ok()
                .and_then(|mut m| m.start(src.clone(), settings)),
            _ => None,
        };
        let p = &mut self.playing[i];
        p.voice = voice;
        p.life = Life::Playing;
        if voice.is_some() {
            self.stats.voices += 1;
        } else {
            self.stats.mixer_full += 1;
        }
    }

    /// Start a sound's piece of music: on its own placed path or the shared one, from the
    /// boundary its setting says, repeating for ever when it loops (a piece on the primary track
    /// is started again by the primary track instead); a piece on the primary track waits while
    /// another plays.
    // D2-07894, D2-07892
    fn play_music(&mut self, i: usize) {
        let handle = self.playing[i].handle;
        if self.playing[i].settings.track == Track::Primary {
            match self.music.primary {
                Some(cur) if cur != handle && self.playing.iter().any(|p| p.handle == cur) => {
                    // It waits; any piece already waiting is dropped.
                    if let Some(old) = self.music.next_primary.replace(handle) {
                        if old != handle {
                            if let Some(k) = self.playing.iter().position(|p| p.handle == old) {
                                self.playing[k].life = Life::Done;
                            }
                        }
                    }
                    self.playing[i].life = Life::Playing;
                    return;
                }
                _ => self.music.primary = Some(handle),
            }
        }
        self.start_piece(i);
    }

    /// Put a sound's piece on the performance and its path's voice in the mixer.
    // D2-07871
    fn start_piece(&mut self, i: usize) {
        let Some(mut run) = self.playing[i].music else {
            return;
        };
        let Some(piece) = self
            .instances
            .get(&self.playing[i].id)
            .and_then(|x| x.piece.clone())
        else {
            return;
        };
        let settings = self.playing[i].settings;
        let boundary = boundary_of(settings.start);
        let repeats = if settings.track == Track::Primary {
            None
        } else if settings.looping {
            Some(u32::MAX)
        } else {
            Some(0)
        };
        let options = PlayOptions {
            boundary,
            repeats,
            lead: false,
        };
        let shared_voice = self.music.flat.and_then(|(_, v)| v);
        let (path, state, new_path) = {
            let Ok(mut perf) = self.music.perf.lock() else {
                return;
            };
            let (path, new_path) = if run.shared {
                match self.music.flat {
                    Some((p, _)) => (p, false),
                    None => {
                        let p = perf.add_music_path(false);
                        self.music.flat = Some((p, None));
                        (p, true)
                    }
                }
            } else if run.state.is_some() || run.path != PathId(0) {
                (run.path, false)
            } else {
                (perf.add_music_path(true), true)
            };
            let state = perf.play(piece, path, options);
            (path, state, new_path)
        };
        run.path = path;
        run.state = Some(state);
        self.playing[i].music = Some(run);
        self.stats.pieces += 1;
        let needs_voice = if run.shared {
            shared_voice.is_none() || new_path
        } else {
            self.playing[i].voice.is_none()
        };
        if needs_voice {
            let vs = self.voice_settings(&self.playing[i]);
            let stream = PathStream {
                perf: self.music.perf.clone(),
                path,
                mono: !run.shared,
                scratch: Vec::new(),
            };
            let voice = self.mixer.as_ref().and_then(|m| {
                m.lock()
                    .ok()
                    .and_then(|mut m| m.start_stream(Box::new(stream), vs))
            });
            if run.shared {
                self.music.flat = Some((path, voice));
            }
            if voice.is_some() {
                self.stats.voices += 1;
            } else {
                self.stats.mixer_full += 1;
            }
            self.playing[i].voice = voice;
        } else if run.shared {
            self.playing[i].voice = shared_voice;
            self.apply(i);
        }
        self.playing[i].life = Life::Playing;
    }

    /// The primary track's turn: when its piece has ended, the one waiting starts, or else it
    /// starts again unless it was stopped.
    // D2-07880, D2-07881
    fn primary_turn(&mut self) {
        let Some(cur) = self.music.primary else {
            return;
        };
        let Some(i) = self.playing.iter().position(|p| p.handle == cur) else {
            self.music.primary = self.music.next_primary.take();
            if let Some(n) = self.music.primary {
                if let Some(k) = self.playing.iter().position(|p| p.handle == n) {
                    self.start_piece(k);
                }
            }
            return;
        };
        let ended = self.playing[i]
            .music
            .and_then(|r| r.state)
            .is_none_or(|s| self.music.perf.lock().map_or(true, |p| p.is_done(s)));
        if !ended || self.playing[i].life == Life::KeepAlive {
            return;
        }
        if let Some(n) = self.music.next_primary.take() {
            self.playing[i].life = Life::Done;
            self.music.primary = Some(n);
            if let Some(k) = self.playing.iter().position(|p| p.handle == n) {
                self.start_piece(k);
            }
        } else if self.playing[i].life == Life::Playing {
            self.start_piece(i);
        } else {
            self.playing[i].life = Life::Done;
            self.music.primary = None;
        }
    }

    fn commit_placement_now(&mut self, i: usize) {
        let p = &mut self.playing[i];
        p.placement = if p.path.placed() {
            Placement::of(p.local, p.velocity, p.settings.min_distance)
        } else {
            Placement::AT_LISTENER
        };
    }

    fn release(&mut self, p: &Playing) {
        if p.music.is_some() {
            self.end_piece(p.music, true);
            if p.music.is_some_and(|r| r.shared) {
                return;
            }
        }
        if let (Some(v), Some(m)) = (p.voice, &self.mixer) {
            if let Ok(mut m) = m.lock() {
                m.stop(v);
            }
        }
    }

    /// Stop a sound's piece at once; a placed path goes with it when `drop_path`.
    fn end_piece(&mut self, run: Option<MusicRun>, drop_path: bool) {
        let Some(run) = run else { return };
        if let Ok(mut perf) = self.music.perf.lock() {
            if let Some(s) = run.state {
                perf.stop(s, Boundary::Immediate);
            }
            if drop_path && !run.shared && run.path != PathId(0) {
                perf.remove_path(run.path);
            }
        }
    }

    fn stop_voice(&mut self, i: usize) {
        if let Some(run) = self.playing[i].music {
            self.end_piece(Some(run), false);
            if let Some(r) = self.playing[i].music.as_mut() {
                r.state = None;
            }
            if run.shared {
                // The shared path's voice plays on for the other pieces.
                self.playing[i].voice = None;
                return;
            }
        }
        let v = self.playing[i].voice.take();
        if let (Some(v), Some(m)) = (v, &self.mixer) {
            if let Ok(mut m) = m.lock() {
                m.stop(v);
            }
        }
    }

    /// A sound's voice ended (or was stopped): a looping sound waiting for its end, or any other
    /// sound not kept, is done.
    // D2-07771
    fn on_segment_ended(&mut self, i: usize) {
        self.stop_voice(i);
        let p = &mut self.playing[i];
        let done = if p.looping() {
            p.life == Life::Ending
        } else {
            p.life != Life::KeepAlive
        };
        if done {
            p.life = Life::Done;
        }
    }

    /// A voice the mixer finished on its own, or a piece of music that has ended.
    fn ended(&mut self, i: usize) {
        if let Some(run) = self.playing[i].music {
            let Some(state) = run.state else { return };
            if self.music.primary == Some(self.playing[i].handle) {
                // The primary track's turn deals with its piece's end.
                return;
            }
            let done = self.music.perf.lock().map_or(true, |p| p.is_done(state));
            if done {
                self.on_segment_ended(i);
            }
            return;
        }
        let Some(v) = self.playing[i].voice else {
            return;
        };
        let alive = self
            .mixer
            .as_ref()
            .and_then(|m| m.lock().ok().map(|m| m.is_playing(v)))
            .unwrap_or(false);
        if !alive {
            self.on_segment_ended(i);
        }
    }

    /// Stop a sound: fading out over `fade` seconds when not `immediate`; else at once, `die`
    /// making it done, else kept to play again.
    // D2-07778
    fn stop_internal(&mut self, i: usize, fade: f32, die: bool, immediate: bool) {
        let from = self.millibels(&self.playing[i]);
        let now = self.now;
        let p = &mut self.playing[i];
        if !immediate && fade > 0.0 {
            p.fade = Some(Fade {
                start: now,
                seconds: fade,
                die,
                from,
            });
            p.life = Life::Fading;
            return;
        }
        if !matches!(p.life, Life::Playing | Life::Fading) {
            if immediate || die {
                p.life = Life::Done;
                self.stop_voice(i);
            }
            return;
        }
        p.life = if die {
            if immediate {
                p.settings.emitter = None;
                Life::Done
            } else {
                Life::Ending
            }
        } else {
            Life::KeepAlive
        };
        // A piece of music stops on the boundary its setting says and is done when it has; on
        // the primary track it is only no longer started again.
        // D2-07893
        if let (Some(run), false) = (p.music, immediate) {
            if die {
                if self.music.primary == Some(p.handle) {
                    return;
                }
                if let (Some(s), Ok(mut perf)) = (run.state, self.music.perf.lock()) {
                    perf.stop(s, boundary_of(p.settings.stop));
                    return;
                }
            }
        }
        // Its voice stops at once.
        self.on_segment_ended(i);
    }

    /// Pause a sound (kept to play again) or let it play.
    // D2-07784
    fn enable(&mut self, i: usize, on: bool) {
        let p = &mut self.playing[i];
        p.disabled = !on;
        if !p.has_played || p.muted {
            return;
        }
        if on {
            if p.life == Life::KeepAlive {
                self.play_internal(i);
            }
            return;
        }
        match p.life {
            Life::Playing => {
                p.life = Life::KeepAlive;
                self.on_segment_ended(i);
            }
            Life::Fading => {
                if p.fade.is_some_and(|f| f.die) {
                    p.life = Life::Done;
                    p.settings.emitter = None;
                } else {
                    p.life = Life::KeepAlive;
                }
                self.on_segment_ended(i);
            }
            _ => {}
        }
    }

    /// A sound's own turn: one whose maker is silent stops, kept to play again (a fade that was to
    /// end it is done); else one kept and played before plays again. Then one that does not loop
    /// is done once its length has run from its start; a fade that has run its time ends it.
    // D2-07781
    fn sound_use_time(&mut self, i: usize) {
        let now = self.now;
        let silent = self.playing[i]
            .settings
            .emitter
            .is_some_and(|e| self.muted.contains(&e));
        self.playing[i].muted = silent;
        if silent {
            let p = &mut self.playing[i];
            let stops = match p.life {
                Life::Playing => {
                    p.life = Life::KeepAlive;
                    true
                }
                Life::Fading => {
                    if p.fade.is_some_and(|f| f.die) {
                        p.life = Life::Done;
                        p.settings.emitter = None;
                    } else {
                        p.life = Life::KeepAlive;
                    }
                    true
                }
                _ => false,
            };
            if stops {
                self.on_segment_ended(i);
            }
        } else if self.playing[i].life == Life::KeepAlive && self.playing[i].has_played {
            self.play_internal(i);
        }
        let p = &mut self.playing[i];
        let start = p.start;
        if p.life != Life::Done
            && !p.looping()
            && p.length > 0.0
            && start + f64::from(p.length) < now
        {
            if matches!(p.life, Life::Playing | Life::Fading) {
                p.life = Life::Done;
                p.settings.emitter = None;
                self.on_segment_ended(i);
            } else {
                p.life = Life::Done;
                self.stop_voice(i);
            }
            return;
        }
        let p = &mut self.playing[i];
        if let Some(f) = p.fade {
            if f64::from(f.seconds) < now - f.start {
                if matches!(p.life, Life::Playing | Life::Fading) {
                    if f.die {
                        p.life = Life::Done;
                        p.settings.emitter = None;
                    } else {
                        p.life = Life::KeepAlive;
                    }
                    p.fade = None;
                    self.on_segment_ended(i);
                } else {
                    p.life = Life::Done;
                    self.stop_voice(i);
                }
            } else if p.life == Life::Fading {
                self.apply(i);
            }
        }
    }
}

/// Why a request was refused while its setting was read.
enum Refusal {
    Budget,
    Missing(String),
}

/// An effect's sound, as an effect plays it: the setting at the effect's strength, made by the
/// object (a music setting on the music channel when its setting says so, else the player
/// music's; a sound setting on the main channel).
// D2-06900, D2-06898
#[must_use]
pub fn effect_sound(
    files: &dyn SoundFiles,
    id: u32,
    volume: f32,
    emitter: Option<u64>,
) -> PlayRequest {
    let mut req = PlayRequest::new(id);
    req.volume = volume;
    req.emitter = emitter;
    req.channel = match records::setting_type(id) {
        Some(SettingType::Music) => {
            let music = records::music_info(files, id)
                .is_some_and(|m| m.test_bit_mask(settings::MUSIC_DEFAULT_CHANNEL_MUSIC));
            if music {
                Channel::Music
            } else {
                Channel::Player
            }
        }
        _ => Channel::Main,
    };
    req
}

/// The boundary a timing names.
fn boundary_of(t: Timing) -> Boundary {
    match t {
        Timing::OnEnd => Boundary::LeadEnd,
        Timing::OnGrid => Boundary::Grid,
        Timing::OnBeat => Boundary::Beat,
        Timing::OnMarker => Boundary::Marker,
        Timing::OnMeasure => Boundary::Measure,
        Timing::Immediate => Boundary::Immediate,
    }
}
