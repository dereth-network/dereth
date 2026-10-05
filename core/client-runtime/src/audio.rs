//! The sound system short of the device: the mixer shell around `dereth-audio`, the rate
//! conversion at the device boundary, the dat reads that feed it, and the world's sound step.
//!
//! The device is the host's: [`crate::platform::audio_out::AudioOutput`] is the endpoint, and the
//! executable installs its default one with `install_default_output`. With none installed the
//! mixer still runs and nothing is heard, which is also what a machine with no sound card gets.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use dereth_assets::audio::{SoundTable, Wave};
use dereth_assets::region::SoundDesc;
use dereth_audio::{AudioAssets, AudioSystem, Prefs, UiSoundRef, MIX_RATE};
use dereth_primitives::DataId;

/// `UIASSET` group 7, enum `0x10000003` — the client resolves this tuple as
/// `(0x10000003, 7, 0x22)` and never writes the result down.
pub const UI_SOUND_TABLE_GROUP: u32 = 7;
/// See [`UI_SOUND_TABLE_GROUP`].
pub const UI_SOUND_TABLE_ENUM: u32 = 0x1000_0003;

/// The UI button-press sound (`0x72`) — the `SoundType` every button in the game plays through
/// the UI `MediaPlayback`'s sound media descriptor.
///
/// Which UI event plays which sound is a property of the layout data and not of the executable, so
/// this is a *starting point* for the bring-up rather than a binding: the UI layer reads them
/// out of the layouts, as `dereth_audio::trigger`'s own documentation requires.
pub const SOUND_UI_BUTTON_PRESS: u32 = 0x72;

/// Anything that stops the sound coming up.
///
/// **None of it is fatal.** Retail start-up runs `srand((unsigned)time(NULL))`
/// *only if DirectSound initialised*, so a machine with no sound card is a state the client is
/// built for and keeps its CRT seed of 1. This type exists so the reason is reportable, not so the
/// application can die of it.
#[derive(Debug, thiserror::Error)]
pub enum AudioShellError {
    /// No output device, or none that would accept a stream.
    #[error("no audio output device: {0}")]
    NoDevice(String),
    /// The device exists and the stream would not build or start.
    #[error("the output stream would not start: {0}")]
    Stream(String),
}

/// What the sound path did, in numbers.
///
/// Two things here are tolerated and therefore counted. Retail's sound creation always returns
/// 0 and no caller checks it, so a wave that will not decode is a **silent** sound in the original
/// — [`AudioStats::wave_decode_failures`] is what stops that being silent here too. And the mixer
/// callback never blocks: if the game thread holds the lock the block is filled with silence, which
/// is an under-run and is counted rather than smoothed over.
#[derive(Debug, Default)]
pub struct AudioStats {
    /// Waves read out of the dat and handed to the mixer.
    pub waves_created: u64,
    /// Waves that resolved and would not decode. Retail returns 0 and the sound is
    /// silent; the number is the only way to see it.
    pub wave_decode_failures: u64,
    /// Sound tables read.
    pub tables_loaded: u64,
    /// Sound-play requests made.
    pub sounds_started: u64,
    /// Blocks the callback filled with silence because the game thread held the lock.
    pub underruns: Arc<std::sync::atomic::AtomicU64>,
    /// The largest absolute sample the device callback ever handed the endpoint, as `f32` bits.
    ///
    /// **This is the only number in this struct that answers "would a person hear it".** Every
    /// other counter says a sound was *asked* for: a wave that resolved, a voice that started and
    /// a block that was filled are all compatible with an endpoint receiving pure silence. The
    /// peak is taken from `out` *after* [`Resampler::fill`] has written it — the buffer cpal is
    /// about to submit — so nothing between the voice pool and the device can hide in it.
    pub peak_output: Arc<std::sync::atomic::AtomicU32>,
    /// Device callbacks served, so [`Self::peak_output`] and `underruns` have a denominator.
    pub blocks_filled: Arc<std::sync::atomic::AtomicU64>,
    /// `Sound.*` preferences that reached `AudioSystem::set_prefs` after start-up.
    pub preferences_applied: u64,
    /// A `Sound.*` name written with the wrong value type. Counted rather than dropped, so a
    /// mis-typed seam is a number instead of a silence.
    pub preference_type_errors: u64,
    /// Movie soundtracks handed to the mixer. Zero here means the Turbine logo plays silently.
    pub movie_tracks_started: u64,
}

/// Audio-system state, the device, and the dat reads that feed them.
pub struct Audio {
    /// The complete audio subsystem. Behind a `Mutex` because the mixer runs on the device's own
    /// thread while audio state is written from the frame; the lock is held for the length
    /// of one `VoicePool::mix`, which is a few hundred multiply-adds.
    system: Arc<Mutex<AudioSystem>>,
    assets: DatAudioAssets,
    /// The host's stream handle, kept alive because dropping it stops the stream. `None` headless,
    /// and `None` on a machine with no sound card. An opaque handle from
    /// [`crate::platform::audio_out::AudioOutput::start`], so nothing here names the
    /// audio library.
    stream: Option<Box<dyn std::any::Any>>,
    /// The endpoint's own rate, for the report line.
    device_rate: u32,
    /// World audio's own state. It lives here rather than on `App` so that the frame loop needs no
    /// field of its own; see [`world_use_time`].
    world: WorldAudio,
    /// The seed `srand` was given -- `time(NULL)` with a device, **1** without.
    ///
    /// Kept because it is not this subsystem's alone: the CRT generator is a process global and
    /// character generation draws from the same stream (its random-integer helper), so the wizard
    /// has to be seeded from the same decision.
    crt_seed: u32,
    pub stats: AudioStats,
}

impl std::fmt::Debug for Audio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Audio")
            .field("device", &self.stream.as_ref().map(|_| self.device_rate))
            .field("waves_created", &self.stats.waves_created)
            .field("sounds_started", &self.stats.sounds_started)
            .finish_non_exhaustive()
    }
}

impl Audio {
    /// Audio initialization minus the MIDI setup, which nothing in retail ever runs.
    ///
    /// The device is probed **before** the [`AudioSystem`] is built, because the seed depends on
    /// it: the CRT generator is seeded from the clock **only if DirectSound initialized**, so on a
    /// machine with no sound card it keeps its default seed of
    /// 1. That is an observable difference the original has — the CRT generator drives every
    ///    ambient probability roll — and reproducing it costs one `if`.
    ///
    /// `ran2_seed` belongs to the independent `ran2` generator and is set elsewhere.
    ///
    /// The device is the host's default output (`install_default_output`); a host that installed
    /// none runs silent, exactly as a machine with no sound card does.
    #[must_use]
    pub fn new(prefs: Prefs, ran2_seed: i32, want_device: bool) -> Self {
        match default_output().filter(|_| want_device) {
            Some(mut out) => Self::with_output(prefs, ran2_seed, Some(&mut *out)),
            None => Self::with_output(prefs, ran2_seed, None),
        }
    }

    /// [`Self::new`] over any host output. `None` is a run with no sound (the
    /// `--no-sound` switch, or a headless run), which probes nothing and keeps the seed of 1.
    #[must_use]
    pub fn with_output(
        prefs: Prefs,
        ran2_seed: i32,
        mut output: Option<&mut dyn crate::platform::audio_out::AudioOutput>,
    ) -> Self {
        let want_device = output.is_some();
        let endpoint = match output.as_deref_mut() {
            Some(out) => out.probe().map_err(AudioShellError::NoDevice),
            None => Err(AudioShellError::NoDevice(
                "--no-sound, or a headless run".into(),
            )),
        };

        let crt_seed = match &endpoint {
            // `time(NULL)`, truncated the way `(unsigned)` truncates it.
            Ok(_) => u32::try_from(
                crate::platform::clock::system_unix_time().map_or(1, |d| d.as_secs())
                    & u64::from(u32::MAX),
            )
            .unwrap_or(1),
            // On a machine with no sound card this keeps its default seed of 1.
            Err(_) => 1,
        };

        let mut me = Self {
            system: Arc::new(Mutex::new(AudioSystem::new(prefs, ran2_seed, crt_seed))),
            assets: DatAudioAssets::default(),
            stream: None,
            device_rate: 0,
            world: WorldAudio::new(),
            crt_seed,
            stats: AudioStats::default(),
        };
        match (endpoint, output) {
            (Ok((rate, channels)), Some(out)) => me.start_device(out, rate, channels),
            (Ok(_), None) => {}
            (Err(e), _) => {
                if want_device {
                    tracing::warn!("{e}; running silent");
                }
            }
        }
        me
    }

    /// Build and start the output stream against this `Audio`'s own [`AudioSystem`].
    fn start_device(
        &mut self,
        output: &mut dyn crate::platform::audio_out::AudioOutput,
        rate: u32,
        channels: u16,
    ) {
        use std::sync::atomic::Ordering;

        let system = Arc::clone(&self.system);
        let underruns = Arc::clone(&self.stats.underruns);
        let peak_output = Arc::clone(&self.stats.peak_output);
        let blocks_filled = Arc::clone(&self.stats.blocks_filled);
        let mut resampler = Resampler::new(MIX_RATE, rate, channels);
        // `f32` because `VoicePool::mix` produces `f32` and every modern endpoint accepts it; a
        // device that insists on `i16` or `u16` falls back to silence rather than to a second
        // conversion path nothing would exercise.
        let started = output
            .start(Box::new(move |out: &mut [f32]| {
                resampler.fill(out, |src| match system.try_lock() {
                    Ok(mut s) => s.mix(src),
                    Err(_) => {
                        // Never block the device thread. A missed block is silence and a number.
                        src.fill(0.0);
                        underruns.fetch_add(1, Ordering::Relaxed);
                    }
                });
                blocks_filled.fetch_add(1, Ordering::Relaxed);
                // The block the device is about to submit, measured after every gain, mute and
                // rate conversion this file applies. The callback is the only writer; the frame
                // loop only ever reads.
                let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
                peak_output.fetch_max(peak.to_bits(), Ordering::Relaxed);
            }))
            .map_err(AudioShellError::Stream);
        match started {
            Ok(s) => {
                self.stream = Some(s);
                self.device_rate = rate;
            }
            Err(e) => tracing::warn!("{e}; running silent"),
        }
    }

    /// The seed handed `srand` -- see [`Self::crt_seed`]'s field.
    ///
    /// The CRT generator is one process global, so anything else that draws from it has to start
    /// from the same value.
    #[must_use]
    pub const fn crt_seed(&self) -> u32 {
        self.crt_seed
    }

    /// Whether a real device is playing.
    #[must_use]
    pub const fn has_device(&self) -> bool {
        self.stream.is_some()
    }

    /// The loudest sample this client has actually submitted to the endpoint, 0.0 when silent.
    ///
    /// See [`AudioStats::peak_output`] for why this and not `sounds_started` is the number that
    /// answers "can a person hear it".
    #[must_use]
    pub fn peak_output(&self) -> f32 {
        f32::from_bits(
            self.stats
                .peak_output
                .load(std::sync::atomic::Ordering::Relaxed),
        )
    }

    /// The endpoint's sample rate, 0 when silent. For the start-up log line.
    #[must_use]
    pub const fn device_rate(&self) -> u32 {
        self.device_rate
    }

    /// Create a sound from `DataID`: read the `0x0A` record and decode it once.
    ///
    /// A decode failure is **not** an error to the caller, because the client's is not: the
    /// `SoundBufRef` is created either way and `Play` is silent. It is counted.
    pub fn create_sound(&mut self, store: &dereth_dat::RetailDatStore, id: DataId) {
        if self.assets.load_wave(store, id).is_err() {
            self.stats.wave_decode_failures += 1;
            return;
        }
        let ok = self
            .system
            .lock()
            .map(|mut s| s.create_sound(id, &self.assets).is_ok())
            .unwrap_or(false);
        if ok {
            self.stats.waves_created += 1;
        } else {
            self.stats.wave_decode_failures += 1;
        }
    }

    /// Read one `0x20` sound table, so entry 7 can pick a row out of it.
    pub fn load_sound_table(&mut self, store: &dereth_dat::RetailDatStore, id: DataId) -> bool {
        let ok = self.assets.load_table(store, id).is_ok();
        if ok {
            self.stats.tables_loaded += 1;
        }
        ok
    }

    /// **Entry 7 / 8** — the UI `MediaPlayback`'s path.
    pub fn play_ui_sound(&mut self, which: UiSoundRef) {
        if let Ok(mut s) = self.system.lock() {
            s.play_ui_sound(which, &self.assets);
            self.stats.sounds_started += 1;
        }
    }

    /// Play a layout media sound through either the direct-wave or sound-table arm.
    /// A missing descriptor or owning element refuses. The invalid sound type (0) selects a
    /// nonzero wave id at volume 1.0; every other sound type selects a loaded sound table.
    ///
    /// Two of the twelve places the retail client plays audio from are these two,
    /// and they are the **only** ones a button, a panel or a slider ever reaches. Which UI event
    /// plays which sound is a property of the `0x21xxxxxx` layout data and not of the executable,
    /// so nothing here names a sound.
    ///
    /// **Table loading is on the play path and is reproduced.** Reading a sound table loads every
    /// wave id in it, so the table arm makes its own waves resident while the wave arm does
    /// **not**: playing a wave id at a volume looks it up in the resident-sound hash and returns in
    /// silence if nothing put the wave there. That asymmetry is retail's and is kept. It costs
    /// nothing in practice: all three sound media records in the shipped layouts take the
    /// table arm.
    pub fn play_media_sound(
        &mut self,
        store: &dereth_dat::RetailDatStore,
        file: DataId,
        sound_type: u32,
    ) {
        // The invalid sound type has numeric value 0.
        if sound_type == 0 {
            // Only when the descriptor's file is non-zero.
            if file.0 != 0 {
                self.play_ui_sound(UiSoundRef::Wave(file));
            }
            return;
        }
        // Load the table by `QualifiedDataID(file, 0x22)`: one that will not load plays nothing.
        if self.assets.table(file).is_none() {
            if !self.load_sound_table(store, file) {
                return;
            }
            self.create_table_waves(store, file);
        }
        self.play_ui_sound(UiSoundRef::Table {
            table: file,
            stype: sound_type,
        });
    }

    /// The ambient set the last pass left standing, as
    /// `(AmbientSTBDesc index, ambient_sound_id)` pairs.
    ///
    /// The ambient system's sound list is **never pruned between passes**
    /// (`dereth_audio::ambient`'s module note), so this is the accumulated set and not this pass's.
    /// It is here for the tests: "the ambience changed when the player entered a dungeon" is a
    /// claim about this list, and nothing outside `dereth-audio` could see it.
    #[must_use]
    pub fn ambient_set(&self) -> Vec<(usize, usize)> {
        self.system
            .lock()
            .map(|s| {
                s.ambient()
                    .sounds
                    .iter()
                    .map(|a| (a.desc, a.ambient_sound_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// **The intro movie's soundtrack.** The movie player's audio renderer, in this build's shape.
    ///
    /// See [`dereth_audio::AudioSystem::play_movie_audio`] for why this is not a voice and why it
    /// answers to none of the eight `Sound.*` preferences.
    pub fn play_movie_audio(&mut self, audio: &dereth_audio::video::MovieAudio) {
        if let Ok(mut s) = self.system.lock() {
            s.play_movie_audio(audio);
            self.stats.movie_tracks_started += 1;
        }
    }

    /// The movie player stopped — its playback graph is gone.
    pub fn stop_movie_audio(&mut self) {
        if let Ok(mut s) = self.system.lock() {
            s.stop_movie_audio();
        }
    }

    /// `WM_ACTIVATEAPP` — `Focus::set_active`, which is what the "play only when active"
    /// preference reads.
    pub fn set_focus(&self, active: bool) {
        if let Ok(mut s) = self.system.lock() {
            s.set_focus(active);
        }
    }

    /// How many voices are currently playing. `VoicePool::active`.
    #[must_use]
    pub fn active_voices(&self) -> usize {
        self.system.lock().map(|s| s.pool().active()).unwrap_or(0)
    }

    /// Fill one interleaved stereo block at [`MIX_RATE`] directly, with no device.
    ///
    /// This is what the headless tests measure: the device path and this share one
    /// `VoicePool::mix`, so proving that samples come out here proves the mixer, and
    /// [`Resampler`]'s own tests prove the only thing the device path adds.
    pub fn mix(&self, out: &mut [f32]) {
        if let Ok(mut s) = self.system.lock() {
            s.mix(out);
        }
    }

    /// The dat-backed asset view, for a caller that wants to look a table up.
    #[must_use]
    pub const fn assets(&self) -> &DatAudioAssets {
        &self.assets
    }

    /// One `Sound.*` preference changed; make it the mixer's.
    ///
    /// This is the far end of one sound-preference change: the selected option value is copied into
    /// the mixer's corresponding variable.
    ///
    /// **The original has no seam here at all.** Each of the eight preferences is registered
    /// against its mixer variable with a **null** change callback, so
    /// the option control's store lands on the effect-volume variable (and the other seven)
    /// directly; there is nothing to notify and nothing to schedule. This function is the price of
    /// the rebuild owning its `Prefs` by value instead of by address, and it is deliberately the
    /// only way that value changes after start-up.
    pub fn set_preference(
        &self,
        name: &str,
        value: dereth_audio::PrefScalar,
    ) -> dereth_audio::PrefWrite {
        match self.system.lock() {
            Ok(mut s) => s.set_preference(name, value),
            // A poisoned lock is a panicked mixer callback; the preference is dropped rather than
            // propagated, exactly as retail ignores sound creation's return.
            Err(_) => dereth_audio::PrefWrite::NotASoundPreference,
        }
    }
}

/// Load the `[Sound]` preferences used to construct the audio system.
///
/// This is the only caller of [`dereth_audio::Prefs::from_ini`] outside its own tests; without it
/// a `UserPreferences.ini` naming `Sound.SoundVolume=0.3` would be read by
/// `crate::config::Preferences`, stored, and never consulted by anything that makes a sound.
///
/// Loading runs *before* preference registration, which then overwrites each registered default with
/// whatever shadow value the load left behind — which is why reading the file here, at
/// construction, is the right order and not merely a convenient one.
#[must_use]
pub fn prefs_from_file(path: &std::path::Path) -> Prefs {
    match crate::platform::files::read_to_string(path) {
        Ok(text) => Prefs::from_ini(&text),
        // A missing file is not an error: retail ignores the preference loader's result.
        Err(_) => Prefs::default(),
    }
}

/// The far end of `UiRequest::SetPreference`.
///
/// Returns the requests this subsystem did **not** own, in order, so the caller's "UI request with
/// no owner yet" line still reports them. A `SetPreference` naming one of the other 35 registered
/// preferences comes straight back out: `Render.FieldOfView` is somebody else's and must not
/// disappear into the sound system.
pub fn apply_preference_requests(
    audio: Option<&mut Audio>,
    requests: Vec<dereth_client_contract::UiRequest>,
) -> Vec<dereth_client_contract::UiRequest> {
    use dereth_audio::{PrefScalar, PrefWrite};
    use dereth_client_contract::{PrefValue, UiRequest};
    let Some(audio) = audio else { return requests };
    let mut left = Vec::with_capacity(requests.len());
    for r in requests {
        let UiRequest::SetPreference(name, value) = &r else {
            left.push(r);
            continue;
        };
        let scalar = match value {
            PrefValue::Bool(b) => PrefScalar::Bool(*b),
            PrefValue::Int(i) => PrefScalar::Int(*i),
            PrefValue::Float(f) => PrefScalar::Float(*f),
            // No `Sound.*` preference is a `PString`; preference value type 5 is unused in this
            // build.
            PrefValue::Text(_) => {
                left.push(r);
                continue;
            }
        };
        match audio.set_preference(name, scalar) {
            PrefWrite::Applied => audio.stats.preferences_applied += 1,
            // A `Sound.*` name with the wrong value type is a bug at the seam, not a silence.
            PrefWrite::WrongType => {
                audio.stats.preference_type_errors += 1;
                tracing::warn!("{name} was written with the wrong type: {value:?}");
            }
            // The other 35 registered preferences land here and are somebody else's.
            PrefWrite::NotASoundPreference => left.push(r),
        }
    }
    left
}

/// The far end of [`dereth_client_contract::UiRequest::PlaySound`].
///
/// The producer is the shell's `tick_media`, which turns every media effect a real element state
/// change raised into one of these; this is the shell's call into the sound manager. Without
/// this consumer the effect the media machine raises would be dropped by the shell's
/// `media_effects` drain.
///
/// Returns the requests this subsystem did not own, in order, exactly as
/// [`apply_preference_requests`] does — so the host's "UI request with no owner yet" line still
/// reports everything else.
pub fn apply_sound_requests(
    audio: Option<&mut Audio>,
    store: &dereth_dat::RetailDatStore,
    requests: Vec<dereth_client_contract::UiRequest>,
) -> Vec<dereth_client_contract::UiRequest> {
    use dereth_client_contract::UiRequest;
    // With no sound subsystem the requests are still consumed rather than handed back: retail's
    // sound updates reach the audio manager whether or not DirectSound came up, and an unbounded
    // queue of clicks nobody will hear is the same mistake the server sound-event path avoids
    // on the other side of this seam.
    let Some(audio) = audio else {
        return requests
            .into_iter()
            .filter(|r| !matches!(r, UiRequest::PlaySound { .. }))
            .collect();
    };
    let mut left = Vec::with_capacity(requests.len());
    for r in requests {
        match r {
            UiRequest::PlaySound { file, sound_type } => {
                audio.play_media_sound(store, file, sound_type);
            }
            other => left.push(other),
        }
    }
    left
}

/// `dereth_audio::AudioAssets` over the retail dats.
///
/// Every record is read through `dereth_dat` and decoded by `dereth_assets`; this holds them so that
/// `AudioAssets::wave` can hand back the header **and** the record its payload offset indexes into.
#[derive(Debug, Default)]
pub struct DatAudioAssets {
    waves: BTreeMap<DataId, (Wave, Vec<u8>)>,
    tables: BTreeMap<DataId, SoundTable>,
    /// The region's sound-descriptor array. Ambience is `WorldAudio`'s; the slot is here because
    /// the trait has it and an empty one is honest where a panic would not be.
    ambient: Vec<SoundDesc>,
}

impl DatAudioAssets {
    /// Read and decode one `0x0A` record.
    ///
    /// # Errors
    /// Whatever the container or the record decoder reports.
    pub fn load_wave(
        &mut self,
        store: &dereth_dat::RetailDatStore,
        id: DataId,
    ) -> Result<(), String> {
        use dereth_assets::Decode;
        use dereth_primitives::AssetSource;
        if self.waves.contains_key(&id) {
            return Ok(());
        }
        let bytes = store.read(id).map_err(|e| format!("{id:?}: {e}"))?;
        let w = Wave::decode_payload(id, &bytes).map_err(|e| format!("{id:?}: {e}"))?;
        self.waves.insert(id, (w, bytes));
        Ok(())
    }

    /// Read and decode one `0x20` sound table.
    ///
    /// # Errors
    /// Whatever the container or the record decoder reports.
    pub fn load_table(
        &mut self,
        store: &dereth_dat::RetailDatStore,
        id: DataId,
    ) -> Result<(), String> {
        use dereth_assets::Decode;
        use dereth_primitives::AssetSource;
        if self.tables.contains_key(&id) {
            return Ok(());
        }
        let bytes = store.read(id).map_err(|e| format!("{id:?}: {e}"))?;
        let t = SoundTable::decode_payload(id, &bytes).map_err(|e| format!("{id:?}: {e}"))?;
        self.tables.insert(id, t);
        Ok(())
    }

    /// Whether a wave has already been read and decoded.
    #[must_use]
    pub fn wave_loaded(&self, id: DataId) -> bool {
        self.waves.contains_key(&id)
    }

    /// The loaded table, for a caller choosing a `SoundType`.
    #[must_use]
    pub fn table(&self, id: DataId) -> Option<&SoundTable> {
        self.tables.get(&id)
    }

    /// The region's ambient descriptors, for world audio.
    pub fn set_ambient(&mut self, descs: Vec<SoundDesc>) {
        self.ambient = descs;
    }
}

impl AudioAssets for DatAudioAssets {
    fn wave(&self, id: DataId) -> Option<(&Wave, &[u8])> {
        self.waves.get(&id).map(|(w, b)| (w, b.as_slice()))
    }
    fn sound_table(&self, id: DataId) -> Option<&SoundTable> {
        self.tables.get(&id)
    }
    fn ambient_desc(&self, index: usize) -> Option<&SoundDesc> {
        self.ambient.get(index)
    }
}

// ==============================================================================================
// World audio: the ambient beds, the triggered sounds and the attenuation.
// ==============================================================================================

/// The seed of the `ran2` stream used by every sound-table row pick and
/// every ambient timing draw.
///
/// Startup initializes the timer
/// and then seeds the random stream:
///
/// ```text
/// time_t t = time(NULL);
/// _seed = (long)t;
/// seed the generator with _seed
/// ```
///
/// so the Numerical Recipes generator is seeded from the wall clock, once, at start-up — and
/// **unconditionally**, unlike the CRT `srand((unsigned)time(NULL))` in sound initialization,
/// which runs only if DirectSound initialised. The two generators are therefore
/// seeded from the same second on a machine with sound and from different values on one without,
/// which is one more reason they must never be merged.
///
/// Seeding with 0 substitutes 1, so a `time(NULL)` of exactly 0 keeps `Ran2`'s own
/// default. That branch is reproduced by [`dereth_primitives::num::rng::Ran2`].
///
/// The clock is a parameter: `App` hands its
/// [`crate::platform::clock::Clock`] down, so a run's seeds come from the run's own clock and
/// not from a second, unnamed read of the host's.
#[must_use]
pub fn ran2_seed(clock: &dyn crate::platform::clock::Clock) -> i32 {
    // `(long)time(NULL)` — a signed 32-bit truncation of the 64-bit `time_t`, which is what the
    // 2013 build's `long` is.
    crate::audio::ran2_seed_at(clock.unix_time())
}

/// What the world audio path did, in numbers, for the log line and the tests.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WorldAudioStats {
    /// Sound tables the region's ambient descriptors named and that loaded.
    pub ambient_tables: u64,
    /// Descriptors whose `stb_id` would not load.
    /// The client latches this and never retries; a non-zero count is silent ambience.
    pub ambient_tables_missing: u64,
    /// Waves created for those tables' rows.
    pub ambient_waves: u64,
    /// Position-change scans run — **not** once per frame.
    pub position_scans: u64,
    /// Animation and physics-script sound hooks acted on.
    pub triggers: u64,
    /// Triggers whose wave or table was not in the dat, i.e. silent ones.
    pub trigger_misses: u64,
    /// `0xF750 Effects_SoundEvent`s taken off [`crate::objects::ObjectStream`].
    ///
    /// Counted apart from [`Self::triggers`] because it is a different entry point with a
    /// different volume rule — and because "the server can make a sound" is the claim this number
    /// is the evidence for.
    pub server_sounds: u64,
    /// Sound events that reached no voice, and there are only three ways, all of them the
    /// client's own: the object is not drawn, so it has no `position` to play at; it has no
    /// sound table at all, which makes the client's play-sound return 0 at once; or its table
    /// would not load out of the dat.
    pub server_sound_misses: u64,
}

/// Everything the ambient-position pass reads, remembered from the pass that ran.
///
/// # What this key is a superset OF
///
/// The gated work is `crate::world::WorldScene::terrain_neighbourhood` fed to
/// `AudioSystem::on_position_changed`, which takes **seven**
/// arguments. Two of them are what this key has to cover:
///
/// * the **listener position** — the scan stores it and every added sound
///   measures its cell's offset from it;
/// * the **3×3 neighbourhood's cells**, one sound add each.
///
/// The other five are named rather than skipped, because a key described only by what it contains
/// is the thing that goes stale. `region` and `now` are not gates: the region is cloned once in
/// [`WorldAudio::bring_up`] and never replaced, and `now` only stamps the play queue. `rng` is a
/// consumed `ran2` stream, not an input. `table_loaded` is `assets.sound_table(id).is_some()`, and
/// `bring_up` loads **every** table the region's descriptors name before the first scan — so the
/// set it answers over is fixed by then, and latches anyway.
/// `enabled` is `Prefs::category_enabled(Ambient)`, and it is **deliberately outside the key**:
/// position-change scanning does not test it, while use-time processing re-reads it every frame,
/// which is where a preference change is meant to take effect. That last one is the only input
/// this key is knowingly not a superset of, and it is faithful.
///
/// Retail's key is `(viewer cell, listener position truncated to whole metres)`. That is a
/// superset of the first of the two by construction — a truncation of the value the pass reads —
/// and it *was* a superset of the second **because retail's landblocks were already resident**:
/// the scan could not run while a block inside the 3×3 was
/// absent and about to appear.
///
/// # Asynchronous neighbourhood loading
///
/// In this rebuild landblocks arrive **asynchronously** — `WorldScene::stream` meshes them over
/// several frames after login — so the neighbourhood can change with retail's key held perfectly
/// still, and a player who does not move a whole metre after login keeps whatever 3×3 the first
/// frame happened to see. That is silence where there should be ambience, and nothing about it is
/// a mis-transcription: **the guard stopped being a superset because a property it silently
/// depended on now lives outside it**.
///
/// So [`ScanKey::cells`] is **added to the key, deliberately and unfaithfully**, and this is the
/// statement of it. It cannot make the scan run where retail would not have: with every block
/// resident the neighbourhood is a pure function of `viewer_block`, so it is constant until the
/// listener crosses a landblock boundary — and a boundary sits at a multiple of
/// `BLOCK_LENGTH = 192.0`, an integer, so crossing one necessarily moves the **truncated
/// whole-metre position** the retail key already carries. The extra scans this admits are exactly
/// the ones retail never needed, because it had nothing left to load.
///
/// The listener position stays **truncated to whole metres**, which is transcribed
/// exactly and is deliberately not tightened: `total_sound_count` is a per-pass sum and rescanning
/// on every sub-metre step gives slightly different volumes.
#[derive(Debug, Clone, PartialEq)]
struct ScanKey {
    /// The player's position cell id.
    cell: Option<dereth_primitives::CellId>,
    /// The listener position, truncated toward zero as retail truncates it.
    pos: [i32; 3],
    /// The ambient scan's outdoors gate, as `terrain_neighbourhood` computes it.
    ///
    /// **Not implied by [`Self::cell`]**, and the two really are read from different objects:
    /// `cell` is `WorldScene::viewer_cell_id`, the body's position-cell id. This flag instead
    /// comes from `WorldScene::viewer_cell().is_none()`, the camera cell determined by
    /// its swept sphere. A camera that steps through a
    /// doorway while the body stands still moves this and not that.
    outdoors: bool,
    /// The cells the scan ran over. **The deviation.** See this type's own note.
    cells: Vec<dereth_audio::TerrainCell>,
}

/// The state world audio keeps between frames, and nothing else.
///
/// The client keeps all of this in its sound-manager and ambient globals; both are inside
/// [`dereth_audio::AudioSystem`], so what is left over here is the *bring-up* — the region the scan
/// reads, which of its tables have been pulled out of the dat, and whether the listener moved.
#[derive(Debug, Default)]
pub struct WorldAudio {
    /// The region, cloned once at bring-up. `dereth_audio::ambient` needs the whole record for
    /// position-change scanning.
    region: Option<Box<dereth_assets::Region>>,
    /// The inputs the last scan actually ran over. See [`ScanKey`].
    last_scan: Option<ScanKey>,
    /// The sticky latch that stops a missing table being re-read
    /// every frame.
    missing_tables: std::collections::BTreeSet<u32>,
    /// Whether the "first world voice" line has been printed.
    announced: bool,
    pub stats: WorldAudioStats,
}

impl Audio {
    /// The world is gone: its sounds stop, the ambient set empties and the next world's
    /// surroundings are scanned afresh.
    pub fn end_world(&mut self) {
        if let Ok(mut s) = self.system.lock() {
            s.end_world();
        }
        self.world.last_scan = None;
    }

    /// Update the listener transform used by spatial sounds.
    pub fn set_listener(&self, l: dereth_audio::Listener) {
        if let Ok(mut s) = self.system.lock() {
            s.set_listener(l);
        }
    }

    /// The listener as it stands, for the tests.
    #[must_use]
    pub fn listener(&self) -> dereth_audio::Listener {
        self.system.lock().map(|s| s.listener()).unwrap_or_default()
    }

    /// Whether a wave has been decoded and is resident.
    #[must_use]
    pub fn has_wave(&self, id: DataId) -> bool {
        self.assets.wave_loaded(id)
    }

    /// The asset seam, mutably, so the region's ambient descriptors can be installed.
    pub fn assets_mut(&mut self) -> &mut DatAudioAssets {
        &mut self.assets
    }

    /// The world-audio counters.
    #[must_use]
    pub const fn world_stats(&self) -> WorldAudioStats {
        self.world.stats
    }

    /// Read one `0x20` table **and** create every wave its rows name.
    ///
    /// The client keeps every decoded sample resident for the session, roughly 50 MB of PCM decoded
    /// once, and row selection is a `ran2` draw that cannot be
    /// pre-resolved without consuming it — so a table's rows are created up front rather than on
    /// demand. **When** the decode happens is the only difference; what plays is unchanged.
    pub fn create_table_waves(&mut self, store: &dereth_dat::RetailDatStore, table: DataId) -> u64 {
        let Some(t) = self.assets.table(table) else {
            return 0;
        };
        let ids: Vec<DataId> = t
            .nodes
            .iter()
            .flat_map(|n| n.data.iter().map(|r| r.sound_id))
            .filter(|d| d.0 != 0)
            .collect();
        let mut n = 0;
        for id in ids {
            let before = self.stats.waves_created;
            self.create_sound(store, id);
            n += self.stats.waves_created - before;
        }
        n
    }

    /// **Entry 1 / 2** — an animation or physics-script sound hook.
    pub fn play_trigger(&mut self, t: SoundTrigger) {
        if let Ok(mut s) = self.system.lock() {
            match t {
                // A plain `SoundHook` reaches the animation event queue carrying the *tweaked* hook's
                // constructor defaults, and entry 1 makes **no probability roll at all**. Routing
                // it through entry 2 would draw one CRT value the client never draws, and the two
                // generators' draw order is observable — so the defaults select entry 1.
                SoundTrigger::Wave {
                    id,
                    at,
                    volume,
                    priority,
                    probability,
                } if (probability - 1.0).abs() < f32::EPSILON
                    && (volume - 1.0).abs() < f32::EPSILON
                    && (priority - 0.9).abs() < f32::EPSILON =>
                {
                    s.play_wave(id, at);
                }
                SoundTrigger::Wave {
                    id,
                    at,
                    volume,
                    priority,
                    probability,
                } => {
                    s.play_wave_tweaked(id, at, probability, priority, volume);
                }
                SoundTrigger::Table { table, stype, at } => {
                    s.play_from_table(table, stype, at, &self.assets);
                }
                // Entry 3, the server sound event's only call.
                SoundTrigger::TableAtVolume {
                    table,
                    stype,
                    volume,
                    at,
                } => {
                    s.play_from_table_at_volume(table, stype, volume, at, &self.assets);
                }
            }
            self.stats.sounds_started += 1;
        }
    }

    /// Advance ambient use time once per frame.
    fn ambient_use_time(
        &mut self,
        region: &dereth_assets::Region,
        now: dereth_primitives::LocalTime,
    ) {
        if let Ok(mut s) = self.system.lock() {
            s.use_time(region, now, &self.assets);
        }
    }

    /// The position-change handler's ambient half — the 3×3 terrain scan.
    fn ambient_position_changed(
        &mut self,
        region: &dereth_assets::Region,
        cells: &dereth_audio::TerrainNeighbourhood,
        now: dereth_primitives::LocalTime,
    ) {
        if let Ok(mut s) = self.system.lock() {
            s.on_position_changed(region, cells, now, &self.assets);
        }
    }
}

/// The world update's audio half, once per frame: one line in [`crate::app::App::frame`].
///
/// A free function rather than a method on `App` so that the frame loop needs exactly one call
/// and no audio state of its own.
///
/// The order is the client's:
///
/// 1. the listener is whatever the **last rendered frame's** camera was — listener position is
///    updated after the draw and audio use time runs before it, so the one-frame lag is original;
/// 2. **the server's `0xF750` sound events**. They are before the hooks because they
///    arrive during message dispatch, which in this build is [`crate::objects::ObjectStream::pump`], and that
///    has already run by the time this is called;
/// 3. the sound hooks this frame's animations and physics scripts raised;
/// 4. the 3×3 terrain rescan, **only when the viewer's cell or position actually changed**
///    — `total_sound_count` is a per-pass sum and recomputing it every frame gives
///    slightly different volumes;
/// 5. ambient use time, which fires whatever is due off the min-heap.
pub fn world_use_time(
    audio: Option<&mut Audio>,
    world: Option<&mut (dyn crate::present::SceneMut + '_)>,
    objects: &mut crate::objects::ObjectStream,
    anim: &dyn dereth_animation::data::AnimAssets,
    store: &dereth_dat::RetailDatStore,
    now: dereth_primitives::LocalTime,
) {
    let (Some(audio), Some(world)) = (audio, world) else {
        // **This is not a detail.** With no audio device the events still have to leave the
        // queue, or an hour of play grows an unbounded `Vec` of sounds nobody will ever hear. The
        // same holds on the other side of the seam, where an undrained hook queue would make a
        // door's collision state depend on having a sound card.
        objects.take_sound_events();
        return;
    };
    // `WorldAudio` lives inside `Audio` so that the frame loop needs no field of its own; taking
    // it out for the length of the call is what lets the two be borrowed at once.
    let mut w = std::mem::take(&mut audio.world);
    w.use_time(audio, world, objects, anim, store, now);
    audio.world = w;
}

impl WorldAudio {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The position-change scan gate includes newly resident cells.
    ///
    /// Answers whether the 3×3 ambient rescan has to run, and latches the key it ran for. Split
    /// out of [`Self::use_time`] because it is the whole of the decision and because a
    /// `WorldScene` needs a graphics device and the retail dats: the station that discriminates this
    /// defect — **a listener that does not move at all while a landblock finishes loading** —
    /// cannot be built through the frame loop without one, and it is three lines here.
    ///
    /// See [`ScanKey`] for what the key is a superset of, and for why `cells` is in it.
    fn scan_needed(
        &mut self,
        cell: Option<dereth_primitives::CellId>,
        pos: [i32; 3],
        cells: &dereth_audio::TerrainNeighbourhood,
    ) -> bool {
        let same = self.last_scan.as_ref().is_some_and(|k| {
            k.cell == cell && k.pos == pos && k.outdoors == cells.outdoors && k.cells == cells.cells
        });
        if same {
            return false;
        }
        self.last_scan = Some(ScanKey {
            cell,
            pos,
            outdoors: cells.outdoors,
            cells: cells.cells.clone(),
        });
        true
    }

    fn use_time(
        &mut self,
        audio: &mut Audio,
        world: &mut dyn crate::present::SceneMut,
        objects: &mut crate::objects::ObjectStream,
        anim: &dyn dereth_animation::data::AnimAssets,
        store: &dereth_dat::RetailDatStore,
        now: dereth_primitives::LocalTime,
    ) {
        if self.region.is_none() {
            self.bring_up(audio, store, world);
        }
        // Taken out for the length of the call, exactly as `world_use_time` takes `WorldAudio`
        // out of `Audio`: steps 2 and 3 want `&mut self` for their counters and steps 4 and 5 want
        // the region, and the two cannot be borrowed at once.
        let Some(region) = self.region.take() else {
            // Same argument as `world_use_time`'s early return: drain, then leave.
            objects.take_sound_events();
            return;
        };

        // 1. Update the listener transform.
        audio.set_listener(world.listener());

        // 2. The server's own sound path.
        self.server_sound_events(audio, world, objects, anim, store);

        // 3. The animation and physics-script hooks. They ran
        //    inside this frame's `update`, so the queue holds exactly this frame's.
        for t in world.take_sound_events() {
            self.stats.triggers += 1;
            // Retail's animation loading creates the hook's sound as it loads,
            // so by the time the hook fires the wave is resident. The asset seam does not
            // expose that moment, so the create happens here instead — the first firing of a given
            // hook is the one that pays for the decode, and nothing else changes.
            match t {
                SoundTrigger::Wave { id, .. } => {
                    if !audio.has_wave(id) {
                        audio.create_sound(store, id);
                        if !audio.has_wave(id) {
                            tracing::debug!("hook sound: wave {id:?} will not load");
                            self.stats.trigger_misses += 1;
                            continue;
                        }
                    }
                }
                // `TableAtVolume` cannot reach this loop — `WorldScene` raises hooks only, and
                // the server's events took `server_sound_events` above — but it wants the same
                // residency, so it shares the arm rather than being a `todo!()` that is one
                // refactor away from a panic.
                SoundTrigger::Table { table, .. } | SoundTrigger::TableAtVolume { table, .. } => {
                    if audio.assets().table(table).is_none() {
                        if audio.load_sound_table(store, table) {
                            audio.create_table_waves(store, table);
                        } else {
                            tracing::debug!("hook sound: sound table {table:?} will not load");
                            self.stats.trigger_misses += 1;
                            continue;
                        }
                    }
                }
            }
            tracing::trace!("hook sound {t:?}");
            audio.play_trigger(t);
        }

        // 4. The rescan, on a real change only.
        //    The neighbourhood is built every frame rather than only inside the gate, because it
        //    is now part of the question the gate asks. See [`ScanKey`] for what the key is a
        //    superset of, including newly resident cells.
        let l = audio.listener();
        let cells = world.terrain_neighbourhood();
        let pos = [
            dereth_primitives::num::to_i32(l.pos.x),
            dereth_primitives::num::to_i32(l.pos.y),
            dereth_primitives::num::to_i32(l.pos.z),
        ];
        if self.scan_needed(world.viewer_cell_id(), pos, &cells) {
            self.stats.position_scans += 1;
            audio.ambient_position_changed(&region, &cells, now);
        }

        // 5. Advance ambient sound use-time processing.
        audio.ambient_use_time(&region, now);
        self.region = Some(region);

        // The first time anything is actually playing, say so once. Without a line here the only
        // way to know the world made a sound is to hear it, and a headless run cannot.
        //
        // **This throttle is deliberate**, which is a different answer from the one
        // `ObjectPhysics::report_key` gets, and the reason is the subject rather than the fields.
        // `announced` is a **one-shot event notice**: what it reports is *"the world made a sound
        // for the first time"*, and that event happens once by definition.
        // The three counters beside it are the state of the world **at that moment**, context
        // for the event, not a level the line is tracking — so their climbing afterwards is not
        // a condition going unreported. And they are not only reachable here: all three are
        // live on [`WorldAudioStats`] through [`Audio::world_stats`], which is what every test
        // reads. A silently-climbing counter is acceptable exactly when the counter is not the
        // subject and has another reader; both hold here and neither holds for `reported`.
        if !self.announced && audio.active_voices() > 0 {
            self.announced = true;
            tracing::debug!(
                "first world voice -- {} active, {} hook(s) fired, {} scan(s)",
                audio.active_voices(),
                self.stats.triggers,
                self.stats.position_scans
            );
        }
    }

    /// **Handle server-directed object sounds.**
    ///
    /// The server's one "play this sound on that object" message, decoded by `dereth-protocol`
    /// and played through `AudioSystem::play_from_table_at_volume`; this is the caller that joins
    /// the two.
    ///
    /// The handler resolves the emitting object, queues the message when that object is not yet
    /// known, and otherwise plays `soundType` from the object's selected table at the wire volume.
    ///
    /// The queue-on-unknown-object arm is already done and is not repeated here: it is
    /// `dereth_client_net::client_session`'s dispatch, whose own test asserts that a `0xF750` for an object the
    /// instance table does not hold comes back `Queued` and is replayed when it does. That is why
    /// every one of the corpus's 249 sound events names an object the same recording created.
    ///
    /// Three things are resolved per message and each of them is a place a build can be silently
    /// wrong, so each is counted rather than assumed:
    ///
    /// * **the table** — the object's sound-table override when present, otherwise the region
    ///   default. All 38 creates of the 34 objects the corpus sounds carry the override, so the
    ///   order is not academic;
    /// * **the position** — the object's own position, the *emitting* object's, which is the renderer's
    ///   frame and not the listener's. An object the scene is not drawing has none, exactly as an
    ///   object the client has not put in a cell has none;
    /// * **the volume** — the message's, overriding the row's. That is the only difference between
    ///   entries 3 and 4, and 24 of the 249 exercise it.
    fn server_sound_events(
        &mut self,
        audio: &mut Audio,
        world: &dyn crate::present::Scene,
        objects: &mut crate::objects::ObjectStream,
        anim: &dyn dereth_animation::data::AnimAssets,
        store: &dereth_dat::RetailDatStore,
    ) {
        let player = objects.player();
        for m in objects.take_sound_events() {
            self.stats.server_sounds += 1;
            // The object's position. The body is its own physics entity in this build, so the player's
            // frame comes from `Character` rather than the server's copy.
            let at = if Some(m.id) == player {
                world.character().map(|c| c.render_frame().origin)
            } else {
                None
            }
            .or_else(|| world.server_object_frame(m.id).map(|f| f.origin));
            let Some(at) = at else {
                tracing::debug!(
                    "server sound {} on {:?}: the object has no position",
                    m.sound_type,
                    m.id
                );
                self.stats.server_sound_misses += 1;
                continue;
            };
            // Resolve the sound table in the order the object's description sets it.
            let table = objects
                .presence(m.id)
                .and_then(|p| p.sound_table)
                .or_else(|| {
                    if Some(m.id) == player {
                        world.character_sound_table()
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    objects
                        .presence(m.id)
                        .and_then(|p| p.setup_id)
                        .and_then(|sid| anim.setup(sid))
                        .and_then(|sd| sd.default_sound_table)
                });
            // The client's play-sound first check: no table, no sound, and no error either.
            let Some(table) = table else {
                tracing::debug!(
                    "server sound {} on {:?}: the object has no sound table",
                    m.sound_type,
                    m.id
                );
                self.stats.server_sound_misses += 1;
                continue;
            };
            if audio.assets().table(table).is_none() {
                if audio.load_sound_table(store, table) {
                    audio.create_table_waves(store, table);
                } else {
                    tracing::debug!(
                        "server sound {} on {:?}: sound table {table:?} will not load",
                        m.sound_type,
                        m.id
                    );
                    self.stats.server_sound_misses += 1;
                    continue;
                }
            }
            tracing::debug!("server sound {} on {:?} from {table:?}", m.sound_type, m.id);
            #[allow(clippy::cast_sign_loss)]
            // LINT-OK: `SoundType` is a `long` on the wire and an unsigned index into the
            // sound table's node keys. The invalid sound type is 0 and there are 205 sound types; a
            // negative would miss every row, matching lookup behavior for an unknown type.
            let stype = m.sound_type as u32;
            audio.play_trigger(SoundTrigger::TableAtVolume {
                table,
                stype,
                volume: m.volume,
                at,
            });
        }
    }

    /// Hand the region's sound-descriptor array to the asset seam and read the tables it names.
    ///
    /// Table loading is lazy and latches a table that was not found;
    /// [`Self::missing_tables`] is that latch and the reason a missing table is read once and
    /// never again.
    fn bring_up(
        &mut self,
        audio: &mut Audio,
        store: &dereth_dat::RetailDatStore,
        world: &dyn crate::present::Scene,
    ) {
        let region = world.region().clone();
        let descs = region.sound_info.clone().unwrap_or_default();
        for d in &descs {
            if d.stb_id.0 == 0 || self.missing_tables.contains(&d.stb_id.0) {
                continue;
            }
            if audio.assets().table(d.stb_id).is_some() {
                continue;
            }
            if audio.load_sound_table(store, d.stb_id) {
                self.stats.ambient_tables += 1;
                self.stats.ambient_waves += audio.create_table_waves(store, d.stb_id);
            } else {
                self.missing_tables.insert(d.stb_id.0);
                self.stats.ambient_tables_missing += 1;
            }
        }
        audio.assets_mut().set_ambient(descs);
        self.region = Some(Box::new(region));
        // Printed once, like every other bring-up in this client: "the ambience is not silent" is a
        // claim about numbers, so the numbers are on the console.
        tracing::info!(
            "world audio up -- {} ambient sound table(s), {} wave(s), {} missing",
            self.stats.ambient_tables,
            self.stats.ambient_waves,
            self.stats.ambient_tables_missing
        );
    }
}

/// The rate conversion at the device boundary, and the only arithmetic in this file.
///
/// The client's primary buffer is 11 025 Hz stereo and every secondary buffer was resampled into it
/// by the DirectSound software mixer. A modern shared-mode endpoint runs
/// at its own rate, so the same linear interpolation happens here instead — after the voice pool,
/// exactly where DirectSound did it, so nothing about the mix changes.
///
/// It is a separate type with its own tests because it is the one part of the device path that can
/// be wrong without a sound card to notice.
#[derive(Debug)]
pub struct Resampler {
    src_rate: u32,
    dst_rate: u32,
    channels: u16,
    /// Interleaved stereo at `src_rate`, holding whatever the last block did not consume.
    buf: Vec<f32>,
    /// The read cursor, in source frames from the head of `buf`.
    pos: f64,
}

impl Resampler {
    #[must_use]
    pub fn new(src_rate: u32, dst_rate: u32, channels: u16) -> Self {
        Self {
            src_rate,
            dst_rate: dst_rate.max(1),
            channels: channels.max(1),
            // One frame of history so the first interpolation has a left neighbour.
            buf: vec![0.0, 0.0],
            pos: 0.0,
        }
    }

    /// Fill one device block, calling `mix` for as many source frames as the block needs.
    ///
    /// `mix` is handed an interleaved **stereo** slice at `src_rate` — exactly the shape
    /// `VoicePool::mix` fills — and is called at most once per block.
    pub fn fill(&mut self, out: &mut [f32], mut mix: impl FnMut(&mut [f32])) {
        let ch = usize::from(self.channels);
        let out_frames = out.len() / ch;
        if out_frames == 0 {
            out.fill(0.0);
            return;
        }
        let step = f64::from(self.src_rate) / f64::from(self.dst_rate);

        // Drop the source frames the last block consumed, keeping the one the cursor sits on.
        let consumed =
            usize::try_from(dereth_primitives::num::to_i32_f64(self.pos.floor())).unwrap_or(0);
        if consumed > 0 {
            self.buf.drain(..consumed * 2);
            #[allow(clippy::cast_precision_loss)]
            // LINT-OK: a buffer index of at most a few thousand frames, exact in f64.
            {
                self.pos -= consumed as f64;
            }
        }

        // The last frame the block reads, plus its right neighbour for the interpolation.
        #[allow(clippy::cast_precision_loss)]
        // LINT-OK: a device block length, at most a few thousand, exact in f64.
        let last = self.pos + (out_frames - 1) as f64 * step;
        let want =
            usize::try_from(dereth_primitives::num::to_i32_f64(last.floor())).unwrap_or(0) + 2;
        let have = self.buf.len() / 2;
        if want > have {
            let start = self.buf.len();
            self.buf.resize(want * 2, 0.0);
            mix(&mut self.buf[start..]);
        }

        for f in 0..out_frames {
            #[allow(clippy::cast_precision_loss)]
            // LINT-OK: as above.
            let p = self.pos + f as f64 * step;
            let i = usize::try_from(dereth_primitives::num::to_i32_f64(p.floor())).unwrap_or(0);
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: an interpolation weight in [0, 1). This is device-boundary arithmetic, not
            // engine arithmetic: bit-exactness applies to the client's own computations, and
            // DirectSound's mixer was not one of ours to reproduce bit for bit.
            let t = (p - p.floor()) as f32;
            let l = lerp(self.buf.get(i * 2), self.buf.get((i + 1) * 2), t);
            let r = lerp(self.buf.get(i * 2 + 1), self.buf.get((i + 1) * 2 + 1), t);
            for (c, slot) in out[f * ch..f * ch + ch].iter_mut().enumerate() {
                *slot = match c {
                    0 => l,
                    1 => r,
                    // A device with more than two channels gets silence in the extras: the client's
                    // primary buffer is stereo and there is nothing to put there.
                    _ => 0.0,
                };
            }
        }
        #[allow(clippy::cast_precision_loss)]
        // LINT-OK: as above.
        {
            self.pos += out_frames as f64 * step;
        }
    }
}

fn lerp(a: Option<&f32>, b: Option<&f32>, t: f32) -> f32 {
    let a = a.copied().unwrap_or(0.0);
    let b = b.copied().unwrap_or(a);
    a + (b - a) * t
}

/// A constructor of the host's default audio endpoint.
pub type OutputFactory = fn() -> Box<dyn crate::platform::audio_out::AudioOutput>;

static DEFAULT_OUTPUT: std::sync::OnceLock<OutputFactory> = std::sync::OnceLock::new();

/// Install the host's default audio endpoint. The first installation stands.
pub fn install_default_output(f: OutputFactory) {
    let _ = DEFAULT_OUTPUT.set(f);
}

/// The host's default endpoint, if one was installed.
fn default_output() -> Option<Box<dyn crate::platform::audio_out::AudioOutput>> {
    DEFAULT_OUTPUT.get().map(|f| f())
}

/// One sound an animation or physics-script hook asked for, with the position it plays at.
///
/// The client plays a hook's sound at the object's own position, so the position is the
/// **emitting object's**, not the listener's; the attenuation and the pan are then the only
/// spatialisation there is (`dereth-audio`'s shipped bug 1 — there is no 3D audio).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SoundTrigger {
    /// Entries 1 and 2 — a direct wave id. `SoundHook` carries `SoundTweakedHook`'s constructor
    /// defaults (`prio = 0.9, prob = 1.0, vol = 1.0`), so the two arrive here in one shape.
    Wave {
        id: DataId,
        at: dereth_primitives::Vec3,
        volume: f32,
        priority: f32,
        probability: f32,
    },
    /// Entry 4 — the object's own sound table, by `SoundType`.
    Table {
        table: DataId,
        stype: u32,
        at: dereth_primitives::Vec3,
    },
    /// **Entry 3 — the server's `0xF750 Effects_SoundEvent`.**
    ///
    /// The same lookup as [`Self::Table`] — the object's own sound table, rolled against the
    /// row's probability — but at the **message's** volume rather than the row's, which is the
    /// only thing separating the client's play-by-type-at-volume entry from plain
    /// play-by-type. 24 of the corpus's 249 sound events carry a volume
    /// of 0.5, so a build that took the row's volume here would be wrong 24 times in seven
    /// recorded sessions and have no way to notice.
    TableAtVolume {
        table: DataId,
        stype: u32,
        volume: f32,
        at: dereth_primitives::Vec3,
    },
}

/// The client generator's seed from a wall-clock reading: `(long)time(NULL)`, a signed 32-bit
/// truncation of the 64-bit time, with an unreadable clock answering 1 (the generator's own
/// default). The host's clock-taking form is `dereth_client_runtime::audio::ran2_seed`.
#[must_use]
pub fn ran2_seed_at(unix_time: Option<std::time::Duration>) -> i32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    // LINT-OK: the deliberate 32-bit wrap `(long)time(NULL)` has. Not a float conversion.
    let t = unix_time.map_or(1, |d| d.as_secs() as i64) as i32;
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the primary buffer is 11025 Hz stereo and the mixer resamples into it. A 1:1 device
    // rate must pass the mix through unchanged, because in that case there is no resampling to do and any difference
    // would be this module inventing one.
    #[test]
    fn a_device_at_the_mix_rate_passes_the_samples_through_unchanged() {
        let mut r = Resampler::new(MIX_RATE, MIX_RATE, 2);
        let mut out = vec![0.0f32; 8 * 2];
        let mut n = 0.0f32;
        r.fill(&mut out, |src| {
            for s in src.iter_mut() {
                n += 1.0;
                *s = n;
            }
        });
        // The first frame is the history frame the constructor seeded; from there the block is the
        // mixed samples in order.
        assert_eq!(
            &out[2..],
            &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0]
        );
    }

    // Oracle: the rate ratio itself. 44100 / 11025 is exactly 4, so a block of 8 device frames must
    // consume exactly 2 source frames, and the cursor must carry across blocks rather than
    // restarting -- which is the failure that makes a resampler click once per callback.
    #[test]
    fn the_cursor_carries_across_blocks_and_consumes_the_right_number_of_source_frames() {
        let mut r = Resampler::new(MIX_RATE, MIX_RATE * 4, 2);
        let mut produced = 0usize;
        let mut out = vec![0.0f32; 64 * 2];
        for _ in 0..4 {
            r.fill(&mut out, |src| produced += src.len() / 2);
        }
        // 4 blocks x 64 device frames / 4 = 64 source frames, plus the one-frame lookahead.
        assert!(
            (64..=66).contains(&produced),
            "{produced} source frames for 256 device frames at 4x"
        );
        // The buffer does not grow without bound: one block's worth plus the lookahead, whatever
        // the block count. A resampler that forgets to drop consumed frames grows for ever, and
        // that is a leak a listener never hears.
        assert!(
            r.buf.len() <= 64 / 4 * 2 + 8,
            "the scratch buffer grew to {}",
            r.buf.len()
        );
        let after_four = r.buf.len();
        for _ in 0..8 {
            r.fill(&mut out, |src| src.fill(0.0));
        }
        assert_eq!(
            r.buf.len(),
            after_four,
            "the scratch buffer grew over twelve blocks"
        );
    }

    // Oracle: the interpolation itself, evaluated rather than restated (BUILDER_BRIEF section 5).
    // At 2x the device rate every other output frame is the midpoint of two source frames.
    #[test]
    fn a_doubled_rate_interpolates_the_midpoints() {
        let mut r = Resampler::new(MIX_RATE, MIX_RATE * 2, 2);
        let mut out = vec![0.0f32; 6 * 2];
        r.fill(&mut out, |src| {
            // Source frames 1, 2, 3 ... after the seeded zero frame.
            for (i, f) in src.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let v = (i + 1) as f32;
                f[0] = v;
                f[1] = v;
            }
        });
        // Frame 0 is the seeded 0.0; frame 1 is halfway to source frame 1; frame 2 is source
        // frame 1; and so on.
        let left: Vec<f32> = out.as_chunks::<2>().0.iter().map(|f| f[0]).collect();
        assert_eq!(left, vec![0.0, 0.5, 1.0, 1.5, 2.0, 2.5]);
    }

    fn cell_at(x: f32, y: f32, word: u16) -> dereth_audio::TerrainCell {
        dereth_audio::TerrainCell {
            pos: dereth_primitives::Vec3::new(x, y, 0.0),
            terrain_word: word,
        }
    }

    fn hood(cells: Vec<dereth_audio::TerrainCell>) -> dereth_audio::TerrainNeighbourhood {
        dereth_audio::TerrainNeighbourhood {
            outdoors: true,
            cells,
        }
    }

    /// A landblock arriving under a motionless listener rescans.
    #[test]
    fn a_landblock_arriving_under_a_motionless_listener_rescans() {
        let mut w = WorldAudio::new();
        let cell = Some(dereth_primitives::CellId(0xA9B4_0025));
        let pos = [24, 12, 0];

        // Frame 1: one block has meshed. First pass ever, so it scans.
        let one = hood(vec![cell_at(1.0, 1.0, 1 << 2)]);
        assert!(
            w.scan_needed(cell, pos, &one),
            "the first pass must always scan"
        );
        // Frame 2: nothing has changed at all. This is the retail gate and it must still hold --
        // the deviation adds scans, it does not remove the guard.
        assert!(
            !w.scan_needed(cell, pos, &one),
            "a frame with nothing changed must not rescan"
        );

        // Frame 3: the second landblock's mesh landed. The listener has not moved a millimetre.
        let two = hood(vec![cell_at(1.0, 1.0, 1 << 2), cell_at(193.0, 1.0, 2 << 2)]);
        assert!(
            w.scan_needed(cell, pos, &two),
            "a landblock arriving under a motionless listener must re-run the 3x3 scan"
        );
        assert!(!w.scan_needed(cell, pos, &two), "and then settle again");
    }

    // The other half of the same key, and the reverse control for the test above: a listener that
    // moves a whole metre over an unchanged neighbourhood must still rescan, because that is
    // the reference client's own listener-position key. A "fix" keyed on cells alone would pass
    // the test above and fail
    // this one.
    #[test]
    fn a_whole_metre_of_movement_still_rescans_over_an_unchanged_neighbourhood() {
        let mut w = WorldAudio::new();
        let cell = Some(dereth_primitives::CellId(0xA9B4_0025));
        let one = hood(vec![cell_at(1.0, 1.0, 1 << 2)]);
        assert!(w.scan_needed(cell, [24, 12, 0], &one));
        assert!(!w.scan_needed(cell, [24, 12, 0], &one));
        assert!(
            w.scan_needed(cell, [25, 12, 0], &one),
            "a whole metre of movement rescans"
        );
        // And the viewer's cell on its own, which is the other term compares.
        assert!(
            w.scan_needed(
                Some(dereth_primitives::CellId(0xA9B4_0100)),
                [25, 12, 0],
                &one
            ),
            "a cell change rescans"
        );
    }

    // The ambient scan's outdoors gate is carried inside the neighbourhood rather
    // than beside it, so it is asserted separately -- and it is a genuinely **independent** input,
    // not a restatement of the cell the key already carries. The two are read from different
    // objects: the key's cell is `WorldScene::viewer_cell_id`, the **body's**
    // position-cell id; `outdoors` is
    // `WorldScene::viewer_cell().is_none()`, the **camera's** cell off `update_viewer`'s
    // swept sphere. A camera that steps through a doorway while the body stands still flips one and
    // not the other, and the 3x3 cells do not move either -- they are a function of the viewer
    // *block*. So all three arms below hold the cells and the position identical and change only
    // the gate, which is what makes this an assertion about the gate.
    //
    // (Its first version passed `cells: Vec::new()` on the indoor side, so the neighbourhood
    // differed as well and a mutation deleting the gate from the key SURVIVED -- a weak test, and
    // the survivor is what said so.)
    #[test]
    fn the_outdoors_gate_is_part_of_the_key() {
        let mut w = WorldAudio::new();
        let cell = Some(dereth_primitives::CellId(0xA9B4_0025));
        let cells = vec![cell_at(1.0, 1.0, 1 << 2)];
        let outside = dereth_audio::TerrainNeighbourhood {
            outdoors: true,
            cells: cells.clone(),
        };
        let inside = dereth_audio::TerrainNeighbourhood {
            outdoors: false,
            cells,
        };
        assert_eq!(
            outside.cells, inside.cells,
            "the two arms differ in more than the gate"
        );
        assert!(w.scan_needed(cell, [24, 12, 0], &outside));
        assert!(
            w.scan_needed(cell, [24, 12, 0], &inside),
            "the outdoors gate flipped"
        );
        assert!(!w.scan_needed(cell, [24, 12, 0], &inside));
        assert!(
            w.scan_needed(cell, [24, 12, 0], &outside),
            "and back out through the doorway"
        );
    }

    // A device with a channel count other than 2 must not read past the block or leave garbage in
    // the extra channels.
    #[test]
    fn a_multi_channel_device_gets_stereo_in_the_first_two_channels() {
        let mut r = Resampler::new(MIX_RATE, MIX_RATE, 6);
        let mut out = vec![9.0f32; 4 * 6];
        r.fill(&mut out, |src| src.fill(1.0));
        for f in out.as_chunks::<6>().0 {
            assert_eq!(&f[2..], &[0.0, 0.0, 0.0, 0.0]);
        }
    }

    /// With no output the audio is silent and keeps the no device seed.
    #[test]
    fn with_no_output_the_audio_is_silent_and_keeps_the_no_device_seed() {
        let audio = Audio::with_output(Prefs::default(), 0, None);
        assert!(!audio.has_device());
        assert_eq!(audio.device_rate(), 0);
        assert_eq!(audio.crt_seed(), 1);
    }

    /// An output the host was asked for but whose endpoint is missing is the same silence and
    /// the same seed; nothing is started.
    #[test]
    fn an_output_with_no_endpoint_is_silent_and_is_never_started() {
        struct NoEndpoint {
            started: bool,
        }
        impl crate::platform::audio_out::AudioOutput for NoEndpoint {
            fn probe(&mut self) -> Result<(u32, u16), String> {
                Err("no default output endpoint".into())
            }
            fn start(
                &mut self,
                _fill: Box<dyn FnMut(&mut [f32]) + Send>,
            ) -> Result<Box<dyn std::any::Any>, String> {
                self.started = true;
                Err("not reached".into())
            }
        }
        let mut out = NoEndpoint { started: false };
        let audio = Audio::with_output(Prefs::default(), 0, Some(&mut out));
        assert!(!audio.has_device());
        assert_eq!(audio.crt_seed(), 1);
        assert!(!out.started, "an output with no endpoint is not started");
    }

    /// An output that probes and starts: the stream is kept, the rate is the endpoint's, and the
    /// fill the host is handed runs the mixer into the device's block.
    #[test]
    fn an_output_that_starts_keeps_its_stream_and_its_fill_mixes_into_the_block() {
        struct Endpoint {
            #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
            fill: Option<Box<dyn FnMut(&mut [f32]) + Send>>,
        }
        impl crate::platform::audio_out::AudioOutput for Endpoint {
            fn probe(&mut self) -> Result<(u32, u16), String> {
                Ok((MIX_RATE * 4, 2))
            }
            fn start(
                &mut self,
                fill: Box<dyn FnMut(&mut [f32]) + Send>,
            ) -> Result<Box<dyn std::any::Any>, String> {
                self.fill = Some(fill);
                Ok(Box::new(()))
            }
        }
        let mut out = Endpoint { fill: None };
        let audio = Audio::with_output(Prefs::default(), 0, Some(&mut out));
        assert!(audio.has_device());
        assert_eq!(audio.device_rate(), MIX_RATE * 4);
        let mut fill = out.fill.take().expect("the output was started with a fill");
        let mut block = vec![1.0f32; 64 * 2];
        fill(&mut block);
        assert_eq!(
            audio
                .stats
                .blocks_filled
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert!(
            block.iter().all(|s| *s == 0.0),
            "nothing is playing, so the block is silence"
        );
    }
}
