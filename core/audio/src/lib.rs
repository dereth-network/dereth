//! The client's sound engine short of the output device: the mixer, attenuation and pan, the
//! trigger paths, ambience and the audio and video codecs.
//!
//! **Depends on** `dereth-primitives` and the decoded sound tables and waves of `dereth-assets`.
//! **Used by** the client runtime (`dereth-client-runtime`), the SDK and the client, which supplies
//! the output device.
//!
//! **Must never** open a sound device: [`mixer::VoicePool::mix`] fills a caller-supplied
//! interleaved stereo block, so the whole crate and its tests run without a sound card. Reading
//! sound objects out of the data files is `dereth-assets`' job, the listener's position is the
//! camera's, and hook dispatch is animation's.
//!
//! The client's audio is small: decode the samples once, keep them resident, and start at most
//! sixteen **whole samples** at once with an integer-decibel gain and a ±15 pan. There is no mixer
//! graph, no streaming, no music and **no 3D audio**; all spatialisation is the arithmetic in
//! [`atten`].
//!
//! **The shipped bugs are the specification.** Each has a test that names it, and a rebuild that
//! fixes any one is wrong:
//!
//! | # | Bug | Where | Test |
//! |---|---|---|---|
//! | 1 | No 3D audio at all | [`atten`] | `pan` and `attenuation` are the only spatialisation |
//! | 2 | Attenuation is twice as steep as physical, quantised to whole dB, rounded **up**, with a floor | [`atten::attenuation`] | `the_gain_is_rounded_up_never_down` |
//! | 3 | The ambient volume is applied **twice**, so its slider is quadratic | [`AudioSystem::play_ambient_sound`] | `the_ambient_volume_is_applied_twice` |
//! | 4 | The voice priority system is inert, so the 17th sound is always dropped | [`mixer::VoicePool`] | `the_seventeenth_simultaneous_sound_is_dropped_not_stolen` |
//! | 5 | The last row of a multi-row table entry is unreachable | [`table::row_index`] | `the_last_row_of_a_multi_row_entry_is_unreachable` |
//! | 6 | Nothing loops; continuous ambience is a re-trigger on a timer | [`ambient`] | `continuous_ambience_re_triggers_at_min_rate_with_no_random_draw` |
//!
//! And two dead preferences: `Sound.InterfaceSoundVolume` is read by nothing, and
//! `Sound.SoundDisabled` is bound to an *enabled* boolean with no inversion.
//!
//! **Specified in** `docs/formats/21-sound-tables.md` (waves and the sound tables that name them).

#![doc(html_no_source)]

pub mod ambient;
pub mod atten;
pub mod cache;
pub mod error;
pub mod focus;
pub mod mixer;
pub mod prefs;
pub mod table;
pub mod testing;
pub mod trigger;
pub mod video;
pub mod wave;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use dereth_assets::audio::{SoundTable, Wave};
use dereth_assets::region::{Region, SoundDesc};
use dereth_primitives::num::math;
use dereth_primitives::num::rng::{CrtRand, Ran2};
use dereth_primitives::{DataId, LocalTime, Vec3};

pub use ambient::scan::{TerrainCell, TerrainNeighbourhood};
pub use ambient::{Ambient, AmbientPlay};
pub use atten::{attenuation, pan, VOL_MIN_DB, VOL_MIN_DIST, VOL_MIN_DIST_SQ};
pub use cache::{SoundBufRef, SoundCache, SoundData};
pub use error::AudioError;
pub use focus::Focus;
pub use mixer::{StartOutcome, Voice, VoicePool, NUM_VOICES};
pub use prefs::{Category, PrefScalar, PrefWrite, Prefs, SoundFeatures, VOLUME_SLIDER_RANGE};
pub use trigger::{SoundHook, UiSoundRef};
pub use wave::{Sample, MIX_RATE};

/// The ambient sound-table descriptor — `dereth-assets` decodes the region file's own descriptor
/// array as [`SoundDesc`]. Each entry is the same 20-byte record.
pub type AmbientSoundDescriptor = SoundDesc;

/// The three decoded asset types this crate needs, and nothing else.
///
/// This is the one hard edge: `dereth-audio` depends on
/// `dereth-assets` for `Wave`, `SoundTable` and the region's ambient descriptors, and reads no dat
/// itself.
///
/// **Why not `fn wave(&self, id) -> Option<&Wave>`.** The decoders' [`Wave`] is a header plus an
/// *offset into the record it was decoded from*
/// (`Wave::payload(record)`), so the record has to come with it. The trait therefore returns both,
/// and [`AudioAssets::sample`] is the method callers actually want.
pub trait AudioAssets {
    /// The decoded `0x0A` record, and the record bytes its payload offset indexes into.
    fn wave(&self, id: DataId) -> Option<(&Wave, &[u8])>;
    /// A `0x20` sound table.
    fn sound_table(&self, id: DataId) -> Option<&SoundTable>;
    /// The region's ambient descriptors, indexed as `Region::sound_info` is.
    fn ambient_desc(&self, index: usize) -> Option<&AmbientSoundDescriptor>;

    /// Decode one wave to mixer-ready PCM. Overridable so that a production implementation can cache
    /// the decode; the default does it on demand.
    fn sample(&self, id: DataId) -> Result<Arc<Sample>, AudioError> {
        let (w, record) = self.wave(id).ok_or(AudioError::NoSuchWave(id))?;
        Ok(Arc::new(wave::decode(w, record)?))
    }
}

/// The listener position the sound engine keeps.
///
/// **This is the camera, not the player.** The client's setter has two callers, and the one
/// that matters passes
/// the camera manager's viewer position, updated once per rendered frame. The same value goes to
/// the sky and render camera, so the audio listener, sky, and render camera always share the same
/// frame. (The player's own physics position is used only as a fallback when the player has no cell.)
///
/// Only the origin and the yaw are read. Pitch and roll never affect audio, so looking up or down
/// does not change the mix.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Listener {
    pub pos: Vec3,
    /// Heading in degrees, normalized to `[0, 360)`.
    pub heading: f32,
}

/// The whole subsystem.
///
/// Retail sound management is a namespace of free functions over file-scope globals rather than an object;
/// this struct is those globals, gathered so that a test can drive them.
#[derive(Debug)]
pub struct AudioSystem {
    pool: VoicePool,
    cache: SoundCache,
    prefs: Prefs,
    /// The `ran2` generator -- the sound-table row pick and every ambient timing, direction,
    /// angle and radius.
    rng: Ran2,
    /// The **CRT** `rand()` -- and the inline ambient probability rolls. `srand((unsigned)time(NULL))`
    /// runs during sound start-up and
    /// **only if DirectSound initialised**, so on a machine with no sound card this keeps its default
    /// seed of 1. The two generators must never be merged.
    crt: CrtRand,
    ambient: Ambient,
    listener: Listener,
    focus: Focus,
    muted: Arc<AtomicBool>,
    /// The intro movie's soundtrack. **Not a voice**: see [`Self::play_movie_audio`].
    movie: Option<MovieVoice>,
}

/// The AVI soundtrack, playing outside the sixteen-slot pool.
#[derive(Debug)]
struct MovieVoice {
    /// Interleaved stereo at [`MIX_RATE`], already widened by [`video::Movie::audio`].
    frames: Arc<[f32]>,
    /// Sample index, advancing by 2 per frame exactly as [`mixer::Voice::pos`] does.
    pos: usize,
}

impl AudioSystem {
    /// Start the sound system, minus the device setup and MIDI setup, which has no caller.
    ///
    /// `ran2_seed` is the shared `ran2` seed, set elsewhere in the client; `crt_seed` is
    /// `time(NULL)`, or 1 when there is no audio device.
    #[must_use]
    pub fn new(prefs: Prefs, ran2_seed: i32, crt_seed: u32) -> Self {
        let muted = Arc::new(AtomicBool::new(false));
        Self {
            pool: VoicePool::new(Arc::clone(&muted)),
            cache: SoundCache::new(),
            prefs,
            rng: Ran2::new(ran2_seed),
            crt: CrtRand::new(crt_seed),
            ambient: Ambient::new(),
            listener: Listener::default(),
            focus: Focus::new(Arc::clone(&muted)),
            muted,
            movie: None,
        }
    }

    #[must_use]
    pub fn prefs(&self) -> &Prefs {
        &self.prefs
    }

    /// Replace all eight `Sound.*` variables at once.
    ///
    /// **When this takes effect.** The client registers the eight preferences with a
    /// pointer to the sound-management global and **no change callback**, so in the original there is
    /// no "apply" step at all: the preference variable *is* the global, and every entry point polls
    /// it on the way through. A volume change is therefore in force for the **next** sound started
    /// and never for one already playing -- the play path sets the buffer's volume and
    /// pan once and neither is touched again for the life of that voice. Nothing here re-gains a
    /// live [`Voice`], deliberately.
    ///
    /// Without a caller here the only `Prefs` that reaches the mixer is the `Prefs::default()`
    /// built at start-up and every volume control in the client is inert. See
    /// [`Self::set_preference`] for the single-name form the options seam uses.
    pub fn set_prefs(&mut self, prefs: Prefs) {
        self.prefs = prefs;
    }

    /// Write one preference by name — the shape the Options page's controls have.
    ///
    /// Goes through [`Self::set_prefs`] rather than round the back of it, so there is exactly one
    /// place where the mixer's view of the preferences changes.
    pub fn set_preference(&mut self, name: &str, value: prefs::PrefScalar) -> prefs::PrefWrite {
        let mut next = self.prefs;
        let outcome = next.set_named(name, value);
        if outcome == prefs::PrefWrite::Applied {
            self.set_prefs(next);
        }
        outcome
    }

    #[must_use]
    pub fn pool(&self) -> &VoicePool {
        &self.pool
    }

    #[must_use]
    pub fn cache(&mut self) -> &mut SoundCache {
        &mut self.cache
    }

    #[must_use]
    pub fn ambient(&self) -> &Ambient {
        &self.ambient
    }

    /// Set the listener, called once per rendered frame.
    pub fn set_listener(&mut self, l: Listener) {
        self.listener = l;
    }

    #[must_use]
    pub fn listener(&self) -> Listener {
        self.listener
    }

    /// `WM_ACTIVATEAPP`. See [`focus`].
    pub fn set_focus(&mut self, active: bool) {
        self.focus.set_active(active);
    }

    /// The flag the audio callback reads.
    #[must_use]
    pub fn mute_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.muted)
    }

    /// Fill one interleaved stereo block at [`MIX_RATE`].
    pub fn mix(&mut self, out: &mut [f32]) {
        self.pool.mix(out);
        self.mix_movie(out);
    }

    /// **Start the intro movie's soundtrack.** The audio half of opening the movie.
    ///
    /// The retail client plays the AVI through DirectShow, whose audio renderer is a filter of its
    /// own: the track never enters sound management, so it takes no voice slot, is not rolled against
    /// a probability, does not pass through positional attenuation, and — as this module's header records
    /// **bypasses every `Sound.*` preference**. Reproducing that means mixing it beside the pool
    /// rather than through it, which is what this does.
    ///
    /// Replaces whatever was playing: there is only ever one movie.
    pub fn play_movie_audio(&mut self, audio: &video::MovieAudio) {
        self.movie = Some(MovieVoice {
            frames: Arc::clone(&audio.frames),
            pos: 0,
        });
    }

    /// Close the movie audio, stopping the soundtrack wherever it had got to.
    /// The world is gone: every voice stops and the ambient set is emptied, so nothing of the last
    /// world goes on being heard and the next one starts from its own surroundings.
    pub fn end_world(&mut self) {
        self.pool.stop_all();
        self.ambient = Ambient::new();
    }

    pub fn stop_movie_audio(&mut self) {
        self.movie = None;
    }

    /// Whether the movie's soundtrack still has samples left.
    #[must_use]
    pub fn movie_audio_playing(&self) -> bool {
        self.movie.as_ref().is_some_and(|m| m.pos < m.frames.len())
    }

    /// Add the movie's block, and advance it whether or not anyone can hear it.
    ///
    /// The advance is unconditional for the same reason [`mixer::VoicePool::mix`]'s is: losing the
    /// foreground silenced DirectSound's buffers but did not pause them, so a movie whose audio was
    /// muted for two seconds comes back two seconds further in rather than two seconds behind.
    fn mix_movie(&mut self, out: &mut [f32]) {
        let audible = !self.muted.load(std::sync::atomic::Ordering::Relaxed);
        let Some(m) = self.movie.as_mut() else { return };
        let n = out.len() & !1;
        let take = (m.frames.len() - m.pos.min(m.frames.len())).min(n);
        if audible {
            for (o, s) in out.iter_mut().zip(&m.frames[m.pos..m.pos + take]) {
                *o += *s;
            }
        }
        m.pos += take;
    }

    /// Create a sound. Errors are the caller's to ignore; the client's are.
    pub fn create_sound(&mut self, id: DataId, assets: &dyn AudioAssets) -> Result<(), AudioError> {
        self.cache.create_sound(id, assets)
    }

    /// Destroy a sound, dropping one link on it.
    pub fn destroy_sound(&mut self, id: DataId) {
        self.cache.destroy_sound(id);
    }

    /// The compass heading **from the sound to the
    /// listener**, degrees, 0 = north, clockwise.
    ///
    /// `off = listener - sound`, z zeroed; a degenerate offset returns `0.0`.
    /// The landblock arithmetic belongs to the physics track; positions reaching this crate are
    /// already in one world frame.
    #[must_use]
    pub fn compass_heading(sound: Vec3, listener: Vec3) -> f32 {
        let dx = listener.x - sound.x;
        let dy = listener.y - sound.y;
        if dx * dx + dy * dy <= f32::EPSILON {
            return 0.0;
        }
        let deg = math::atan2(f64::from(dy), f64::from(dx)).to_degrees();
        // `atan2` then C `fmod` against 360.0.
        let h = (450.0 - deg) % 360.0;
        // LINT-OK: the documented calculation narrows its extended-precision result to `float`
        // here. The original computes at extended precision and returns a `float`.
        #[allow(clippy::cast_possible_truncation)]
        {
            h as f32
        }
    }

    /// 3-D Euclidean distance between two positions.
    #[must_use]
    fn distance(a: Vec3, b: Vec3) -> f32 {
        let d = Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z);
        d.magnitude()
    }

    /// The internal play path -- the positioned one, which every world sound
    /// takes.
    fn play_positioned(&mut self, id: DataId, at: Vec3, volume: f32, cat: Category) {
        if !self.focus.may_start(self.prefs.only_when_active) {
            return;
        }
        let Some(sample) = self.cache.find(id).and_then(|r| r.sample.clone()) else {
            return;
        };
        let dist = Self::distance(at, self.listener.pos);
        let bearing = Self::compass_heading(at, self.listener.pos);
        let p = pan(bearing, self.listener.heading, dist, self.prefs.features);
        let Some(db) = attenuation(dist, volume, cat, &self.prefs) else {
            return;
        };
        // The stored buffer priority is 0.0 for every sound. See [`mixer`].
        let priority = self.cache.find(id).map_or(0.0, |r| r.data.priority);
        self.pool.start(Arc::clone(&sample.frames), db, p, priority);
    }

    /// The "from centre" form: attenuation at distance 0.0 with the supplied volume, and pan 0.
    /// Entries 6, 7 and 8.
    fn play_from_center(&mut self, id: DataId, volume: f32, cat: Category) {
        let Some(sample) = self.cache.find(id).and_then(|r| r.sample.clone()) else {
            return;
        };
        let Some(db) = attenuation(0.0, volume, cat, &self.prefs) else {
            return;
        };
        let priority = self.cache.find(id).map_or(0.0, |r| r.data.priority);
        self.pool.start(Arc::clone(&sample.frames), db, 0, priority);
    }

    /// **Entry 1** -- play a sound on an object. The animation `Sound` hook.
    ///
    /// There is **no probability roll**: an animation `Sound` hook always fires.
    ///
    /// **The effect volume is applied twice on this entry point, and only on this one.** The client
    /// passes `effect_volume` as the *volume argument* on its one call into the internal play path
    /// and the attenuation calculation then multiplies by `effect_volume` again — the same shape as
    /// the documented ambient bug, on a different slider. Entries 2, 3 and 4 pass the hook's, the
    /// message's or the row's own volume and are therefore linear in the slider.
    ///
    /// So the Sound Volume slider is **quadratic for animation `Sound` hooks and linear for
    /// `SoundTable` and `SoundTweaked` hooks**, which is audible: at 50% the two differ by 6 dB. This
    /// follows from the entry-point behavior even though it was not stated explicitly.
    pub fn play_wave(&mut self, id: DataId, at: Vec3) {
        if !self.prefs.category_enabled(Category::Effect) {
            return;
        }
        self.play_positioned(id, at, self.prefs.effect_volume, Category::Effect);
    }

    /// **Entry 2** -- play a sound on an object with an explicit priority, probability and
    /// volume. The `SoundTweaked` hook.
    ///
    /// `prio` is read out of the dat, passed in, and **never touched**: the client reads the
    /// probability and volume arguments off the stack and never the priority one. It is accepted here
    /// so the signature matches, and dropped, because that is what the client does.
    pub fn play_wave_tweaked(
        &mut self,
        id: DataId,
        at: Vec3,
        probability: f32,
        _priority: f32,
        volume: f32,
    ) {
        if !self.prefs.category_enabled(Category::Effect) {
            return;
        }
        if !table::play_probability(self.crt.next_u16(), probability) {
            return;
        }
        self.play_positioned(id, at, volume, Category::Effect);
    }

    /// **Entry 3** -- play a sound type on an object at a given volume. The server's
    /// `Sound` message (`0xF750`), whose volume overrides the row's.
    ///
    /// `dereth_desktop::audio`'s `SoundTrigger::TableAtVolume` is the caller that routes the
    /// message to it.
    pub fn play_from_table_at_volume(
        &mut self,
        table_id: DataId,
        stype: u32,
        volume: f32,
        at: Vec3,
        assets: &dyn AudioAssets,
    ) {
        if !self.prefs.category_enabled(Category::Effect) {
            return;
        }
        let Some(row) = self.pick_row(table_id, stype, assets) else {
            return;
        };
        if !table::play_probability(self.crt.next_u16(), row.probability) {
            return;
        }
        self.play_positioned(row.sound_id, at, volume, Category::Effect);
    }

    /// **Entry 4** -- play a sound type on an object. The animation `SoundTable`
    /// hook, at the row's own volume.
    pub fn play_from_table(
        &mut self,
        table_id: DataId,
        stype: u32,
        at: Vec3,
        assets: &dyn AudioAssets,
    ) {
        if !self.prefs.category_enabled(Category::Effect) {
            return;
        }
        let Some(row) = self.pick_row(table_id, stype, assets) else {
            return;
        };
        if !table::play_probability(self.crt.next_u16(), row.probability) {
            return;
        }
        self.play_positioned(row.sound_id, at, row.volume, Category::Effect);
    }

    /// **Entry 5** -- play an ambient sound at a position.
    ///
    /// **The ambient volume is applied here and again inside the attenuation calculation**, so the slider is
    /// quadratic. Preserve it.
    ///
    /// The draw order within this function is `table::get_sound` (a `ran2` draw for the row pick) and *then*
    /// the CRT `rand()` for the probability — the two generators, in that order.
    pub fn play_ambient_sound(
        &mut self,
        table_id: DataId,
        stype: u32,
        pos: Vec3,
        volume: f32,
        assets: &dyn AudioAssets,
    ) {
        if !self.prefs.category_enabled(Category::Ambient) {
            return;
        }
        // `v = ambient_sound_volume * volume` -- application one.
        let v = self.prefs.ambient_volume * volume;
        let Some(row) = self.pick_row(table_id, stype, assets) else {
            return;
        };
        if !table::play_probability(self.crt.next_u16(), row.probability) {
            return;
        }
        // Application two is inside `attenuation` for Category::Ambient.
        self.play_positioned(row.sound_id, pos, v, Category::Ambient);
    }

    /// **Entry 6** -- play an ambient sound from the listener's centre. Same
    /// double application, pan 0 and distance 0.
    pub fn play_ambient_sound_from_center(
        &mut self,
        table_id: DataId,
        stype: u32,
        volume: f32,
        assets: &dyn AudioAssets,
    ) {
        if !self.prefs.category_enabled(Category::Ambient) {
            return;
        }
        let v = self.prefs.ambient_volume * volume;
        let Some(row) = self.pick_row(table_id, stype, assets) else {
            return;
        };
        if !table::play_probability(self.crt.next_u16(), row.probability) {
            return;
        }
        self.play_from_center(row.sound_id, v, Category::Ambient);
    }

    /// **Entries 7 and 8** — the UI `MediaPlayback`, the two portal transitions and `/environ`.
    ///
    /// Both are the **Interface** category, and neither applies `Sound.InterfaceSoundVolume`: entry 8
    /// passes a literal 1.0 and entry 7 uses the row's, and both then go through
    /// non-ambient attenuation, which multiplies by `effect_volume`.
    ///
    pub fn play_ui_sound(&mut self, which: UiSoundRef, assets: &dyn AudioAssets) {
        if !self.prefs.category_enabled(Category::Interface)
            || !self.focus.may_start(self.prefs.only_when_active)
        {
            return;
        }
        match which {
            // Entry 8: no `get_sound`, no roll, volume 1.0.
            UiSoundRef::Wave(id) => self.play_from_center(id, 1.0, Category::Interface),
            // Entry 7: the row's volume, rolled against the row's probability.
            UiSoundRef::Table {
                table: table_id,
                stype,
            } => {
                let Some(row) = self.pick_row(table_id, stype, assets) else {
                    return;
                };
                if !table::play_probability(self.crt.next_u16(), row.probability) {
                    return;
                }
                self.play_from_center(row.sound_id, row.volume, Category::Interface);
            }
        }
    }

    /// Pick a row -- one `ran2` draw, whether or not anything comes back.
    fn pick_row(
        &mut self,
        table_id: DataId,
        stype: u32,
        assets: &dyn AudioAssets,
    ) -> Option<dereth_assets::audio::SoundEntry> {
        let t = assets.sound_table(table_id)?;
        table::get_sound(t, stype, &mut self.rng)
    }

    /// The ambient half of a region change -- the 3x3 terrain scan and the queue
    /// refresh.
    ///
    /// **Called only when the player's cell or position actually changed**, never once per frame.
    pub fn on_position_changed(
        &mut self,
        region: &Region,
        cells: &TerrainNeighbourhood,
        now: LocalTime,
        assets: &dyn AudioAssets,
    ) {
        let loaded = |id: DataId| assets.sound_table(id).is_some();
        let plays = self.ambient.on_position_changed(
            region,
            self.listener.pos,
            cells,
            now,
            self.prefs.category_enabled(Category::Ambient),
            &mut self.rng,
            &loaded,
        );
        self.emit_ambient(&plays, assets);
    }

    /// The per-frame tick, run once per rendered frame.
    pub fn use_time(&mut self, region: &Region, now: LocalTime, assets: &dyn AudioAssets) {
        let loaded = |id: DataId| assets.sound_table(id).is_some();
        let plays = self.ambient.use_time(
            region,
            now,
            self.prefs.category_enabled(Category::Ambient),
            &mut self.rng,
            &loaded,
        );
        self.emit_ambient(&plays, assets);
    }

    fn emit_ambient(&mut self, plays: &[AmbientPlay], assets: &dyn AudioAssets) {
        for p in plays {
            match p.pos {
                Some(pos) => self.play_ambient_sound(p.stb_id, p.stype, pos, p.volume, assets),
                None => self.play_ambient_sound_from_center(p.stb_id, p.stype, p.volume, assets),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::StubAssets;

    const WAVE: DataId = DataId(0x0A00_0002);
    const TABLE: DataId = DataId(0x2000_0001);

    fn system(prefs: Prefs) -> (AudioSystem, StubAssets) {
        let assets = StubAssets::with_table(TABLE, 66, WAVE);
        let mut s = AudioSystem::new(prefs, 12_345, 1);
        s.create_sound(WAVE, &assets).expect("decodes");
        (s, assets)
    }

    /// The ambient volume is applied by the caller **and** again inside
    /// attenuation, so the effective factor is the square. Oracle: the ambient play path
    /// (`ambient_sound_volume * volume`) and the attenuation, which multiplies by the
    /// ambient volume again.
    #[test]
    fn the_ambient_volume_is_applied_twice() {
        // At 0.5 the ambient factor is 0.25, which is -12 dB, not the -6 one application would give.
        let p = Prefs {
            ambient_volume: 0.5,
            ..Prefs::default()
        };
        let (mut s, assets) = system(p);
        s.play_ambient_sound_from_center(TABLE, 66, 1.0, &assets);
        let v = s.pool().voice(0).expect("a voice started");
        let want = mixer::gain_from_db(-12);
        assert!(
            (v.gl - want).abs() < 1e-6,
            "expected -12 dB, got gain {}",
            v.gl
        );

        // A table-driven effect applies its volume once, for contrast: row volume 1.0 times the
        // slider is -6 dB, not -12.
        let p = Prefs {
            effect_volume: 0.5,
            ..Prefs::default()
        };
        let (mut s, assets) = system(p);
        s.play_from_table(TABLE, 66, Vec3::ZERO, &assets);
        let v = s.pool().voice(0).expect("a voice started");
        assert!((v.gl - mixer::gain_from_db(-6)).abs() < 1e-6);
    }

    /// The same shape as the ambient bug, on a different slider, and undocumented until now: entry 1
    /// passes `effect_sound_volume` as the *volume argument* and attenuation multiplies by it
    /// again. Oracle: the effect play path, whose only call passes `effect_sound_volume` as the
    /// volume argument, against the two hook paths, which pass the hook's volume and
    /// the table row's volume respectively.
    #[test]
    fn the_effect_volume_is_applied_twice_for_a_plain_animation_sound_hook() {
        let p = Prefs {
            effect_volume: 0.5,
            ..Prefs::default()
        };
        let (mut s, _a) = system(p);
        // 0.5 * 0.5 = 0.25 -> -12.04 dB -> ceil -12. One application would give -6.
        s.play_wave(WAVE, Vec3::ZERO);
        let v = s.pool().voice(0).expect("a voice started");
        assert!(
            (v.gl - mixer::gain_from_db(-12)).abs() < 1e-6,
            "gain {} is not -12 dB",
            v.gl
        );

        // At full volume the two are indistinguishable, which is why it went unnoticed.
        let (mut s, _a) = system(Prefs::default());
        s.play_wave(WAVE, Vec3::ZERO);
        let v = s.pool().voice(0).expect("a voice started");
        assert!((v.gl - 1.0).abs() < 1e-6);
    }

    /// `Sound.InterfaceSoundVolume` is inert and UI sounds follow the *effect* slider.
    #[test]
    fn ui_sounds_follow_the_effect_slider_and_ignore_the_interface_one() {
        let p = Prefs {
            effect_volume: 0.5,
            interface_volume: 0.0,
            ..Prefs::default()
        };
        let (mut s, assets) = system(p);
        s.play_ui_sound(UiSoundRef::Wave(WAVE), &assets);
        let v = s
            .pool()
            .voice(0)
            .expect("a voice started despite interface_volume = 0");
        assert!(
            (v.gl - mixer::gain_from_db(-6)).abs() < 1e-6,
            "scaled by effect_volume"
        );
    }

    /// The listener is the camera and only its yaw matters: pitching the camera up and down must not
    /// change the mix. Oracle: the recovered rule that roll and pitch never affect audio.
    #[test]
    fn the_listener_is_a_position_and_a_yaw_and_nothing_else() {
        let (mut s, _a) = system(Prefs::default());
        // A sound 10 m due east of the listener, who faces north.
        s.set_listener(Listener {
            pos: Vec3::ZERO,
            heading: 0.0,
        });
        let east = Vec3::new(10.0, 0.0, 0.0);
        let bearing = AudioSystem::compass_heading(east, Vec3::ZERO);
        assert!(
            (bearing - 270.0).abs() < 1e-3,
            "the sound is west of the listener: {bearing}"
        );
        s.play_wave(WAVE, east);
        let v = s.pool().voice(0).expect("a voice started");
        assert!(v.gl < v.gr, "a sound to the east is panned right");
    }

    /// Focus: the preference suppresses new sounds, and the master mute silences the output.
    #[test]
    fn losing_focus_suppresses_new_sounds_and_mutes_the_output() {
        let (mut s, _a) = system(Prefs::default());
        s.set_focus(false);
        s.play_wave(WAVE, Vec3::ZERO);
        assert_eq!(
            s.pool().active(),
            0,
            "PlaySoundOnlyWhenActive suppressed the start"
        );
        let mut out = vec![0.0f32; 8];
        s.mix(&mut out);
        assert_eq!(out, vec![0.0f32; 8]);

        // With the preference off the sound starts, but the output is still muted.
        s.set_prefs(Prefs {
            only_when_active: false,
            ..Prefs::default()
        });
        s.play_wave(WAVE, Vec3::ZERO);
        assert_eq!(s.pool().active(), 1);
        let mut out = vec![0.0f32; 8];
        s.mix(&mut out);
        assert_eq!(out, vec![0.0f32; 8], "no buffer has DSBCAPS_GLOBALFOCUS");
    }

    /// Each category enable gates its own entry points, with no inversion anywhere.
    #[test]
    fn each_category_enable_gates_its_own_entry_points() {
        let p = Prefs {
            effects_enabled: false,
            ..Prefs::default()
        };
        let (mut s, assets) = system(p);
        s.play_wave(WAVE, Vec3::ZERO);
        assert_eq!(s.pool().active(), 0);
        s.play_ui_sound(UiSoundRef::Wave(WAVE), &assets);
        assert_eq!(
            s.pool().active(),
            1,
            "interface sounds are not gated by effects_enabled"
        );

        let p = Prefs {
            interface_enabled: false,
            ..Prefs::default()
        };
        let (mut s, assets) = system(p);
        s.play_ui_sound(UiSoundRef::Wave(WAVE), &assets);
        assert_eq!(s.pool().active(), 0);
    }

    /// A wave that was never created plays nothing at all — entries 1, 2 and 8 look the id up
    /// in the sound cache and do nothing on a miss.
    #[test]
    fn an_uncached_wave_plays_nothing() {
        let (mut s, _a) = system(Prefs::default());
        s.play_wave(DataId(0x0A00_9999), Vec3::ZERO);
        assert_eq!(s.pool().active(), 0);
    }

    /// The two generators are separate streams. Oracle: the random
    /// call sites — `ran2` for the row pick and ambient timing, the CRT LCG for the probability
    /// gates, drawn from within the same call.
    #[test]
    fn the_row_pick_and_the_probability_gate_draw_from_different_generators() {
        let assets = StubAssets::with_table(TABLE, 66, WAVE);
        let mut s = AudioSystem::new(Prefs::default(), 999, 1);
        s.create_sound(WAVE, &assets).expect("decodes");
        let ran2_before = s.rng.clone().next_f64();
        let crt_before = s.crt.clone().next_u16();
        s.play_from_table(TABLE, 66, Vec3::ZERO, &assets);
        // Both advanced, and by their own rules.
        assert_ne!(s.rng.clone().next_f64(), ran2_before);
        assert_ne!(s.crt.clone().next_u16(), crt_before);
    }

    /// Entry three takes the messages volume over the rows.
    #[test]
    fn entry_three_takes_the_messages_volume_over_the_rows() {
        let mut assets = StubAssets::with_tone(WAVE, 4);
        // A row that asks for a quarter volume and always plays.
        assets.add_table_rows(TABLE, &[(66, WAVE, 0.25, 1.0)]);

        // Entry 4 — the animation `SoundTable` hook — obeys the row: 0.25 is -12 dB.
        let mut s = AudioSystem::new(Prefs::default(), 12_345, 1);
        s.create_sound(WAVE, &assets).expect("decodes");
        s.play_from_table(TABLE, 66, Vec3::ZERO, &assets);
        let v = s.pool().voice(0).expect("a voice started");
        assert!(
            (v.gl - mixer::gain_from_db(-12)).abs() < 1e-6,
            "entry 4 gain {}",
            v.gl
        );

        // Entry 3 — the server's `0xF750` — overrides it. The message's 1.0 is 0 dB.
        let mut s = AudioSystem::new(Prefs::default(), 12_345, 1);
        s.create_sound(WAVE, &assets).expect("decodes");
        s.play_from_table_at_volume(TABLE, 66, 1.0, Vec3::ZERO, &assets);
        let v = s.pool().voice(0).expect("a voice started");
        assert!(
            (v.gl - 1.0).abs() < 1e-6,
            "entry 3 gain {} is not the message's 1.0",
            v.gl
        );

        // And the corpus's other volume, 0.5, against the same 0.25 row: -6 dB, not -12.
        let mut s = AudioSystem::new(Prefs::default(), 12_345, 1);
        s.create_sound(WAVE, &assets).expect("decodes");
        s.play_from_table_at_volume(TABLE, 66, 0.5, Vec3::ZERO, &assets);
        let v = s.pool().voice(0).expect("a voice started");
        assert!(
            (v.gl - mixer::gain_from_db(-6)).abs() < 1e-6,
            "entry 3 gain {}",
            v.gl
        );
    }

    /// Entry 3 still rolls the **row's** `probability_`, unlike entry 8, which rolls nothing.
    ///
    /// Oracle: section 1's entry table — entry 3's roll column is `play_probability(row.probability_)`
    /// -- and the two draws, whose product is strictly below 1.0 so a row at 1.0
    /// always plays and a row at 0.0 never does.
    #[test]
    fn entry_three_rolls_the_rows_probability_not_the_messages_volume() {
        let mut assets = StubAssets::with_tone(WAVE, 4);
        assets.add_table_rows(TABLE, &[(66, WAVE, 1.0, 0.0)]);
        let mut s = AudioSystem::new(Prefs::default(), 12_345, 1);
        s.create_sound(WAVE, &assets).expect("decodes");
        // A full-volume message cannot buy its way past a row that never plays.
        s.play_from_table_at_volume(TABLE, 66, 1.0, Vec3::ZERO, &assets);
        assert_eq!(s.pool().active(), 0, "probability 0.0 plays nothing");

        assets.add_table_rows(TABLE, &[(66, WAVE, 1.0, 1.0)]);
        let mut s = AudioSystem::new(Prefs::default(), 12_345, 1);
        s.create_sound(WAVE, &assets).expect("decodes");
        s.play_from_table_at_volume(TABLE, 66, 1.0, Vec3::ZERO, &assets);
        assert_eq!(s.pool().active(), 1, "probability 1.0 always plays");
    }

    /// Entry 3 is an **Effect**: `Sound.EffectsOn` silences it and the effect slider scales it.
    ///
    /// Oracle: section 1's `enabled` column, `effect_sounds_enabled` for entries 1-4.
    #[test]
    fn entry_three_is_gated_by_the_effect_category() {
        let mut assets = StubAssets::with_tone(WAVE, 4);
        assets.add_table_rows(TABLE, &[(66, WAVE, 1.0, 1.0)]);
        let p = Prefs {
            effects_enabled: false,
            ..Prefs::default()
        };
        let mut s = AudioSystem::new(p, 12_345, 1);
        s.create_sound(WAVE, &assets).expect("decodes");
        s.play_from_table_at_volume(TABLE, 66, 1.0, Vec3::ZERO, &assets);
        assert_eq!(
            s.pool().active(),
            0,
            "Sound.EffectsOn off silences the server's sounds too"
        );

        // Unlike entry 1, the slider is applied **once**: 0.5 is -6 dB, not -12.
        let p = Prefs {
            effect_volume: 0.5,
            ..Prefs::default()
        };
        let mut s = AudioSystem::new(p, 12_345, 1);
        s.create_sound(WAVE, &assets).expect("decodes");
        s.play_from_table_at_volume(TABLE, 66, 1.0, Vec3::ZERO, &assets);
        let v = s.pool().voice(0).expect("a voice started");
        assert!(
            (v.gl - mixer::gain_from_db(-6)).abs() < 1e-6,
            "gain {}",
            v.gl
        );
    }

    /// Beyond about 94.2 m nothing plays at all: the sound is dropped, not played at the floor.
    #[test]
    fn a_sound_past_the_drop_threshold_never_reaches_a_voice() {
        let (mut s, _a) = system(Prefs::default());
        s.play_wave(WAVE, Vec3::new(94.0, 0.0, 0.0));
        assert_eq!(s.pool().active(), 1);
        let (mut s, _a) = system(Prefs::default());
        s.play_wave(WAVE, Vec3::new(95.0, 0.0, 0.0));
        assert_eq!(s.pool().active(), 0, "inaudible means dropped");
    }
}
