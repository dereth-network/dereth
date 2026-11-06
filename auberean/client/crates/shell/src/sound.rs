//! The game's sound in the shell: the sound manager, the host's device it plays through, the
//! interface's requests, the sounds the world's effects play, the ambient sound by place, and the
//! listener at the camera.
//!
//! The interface keeps three sounds by name ([`SoundSlot`]): the sound playing across the screens
//! before the world, and a story's music and voice. A stop names the slot; a play into a slot
//! keeps its handle there. An effect's sound is made by its object and heard from where the object
//! was last drawn; the sounds an effect keeps stop when the effect stops, and every sound an
//! object makes stops when the object leaves the world.
//!
//! The ambient sound hears what the scene reports about the player each frame (its cell and place,
//! the interior cells in view, the near window's landblocks) and the camera it last drew from; the
//! landscape's points of each landblock are worked out from the world's data once while the block
//! is heard. Leaving the world clears it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use auberean_audio::ambient::{self, Ambient, LandPoint, LandPoints, Surroundings};
use auberean_audio::space::Listener;
use auberean_audio::{SoundFiles, SoundPlayer};
use auberean_client_sdk::dat::DatFile;
use auberean_draw::effects::FxSound;
use auberean_draw::portal::PassageSound;
use auberean_primitives::random::Random;
use auberean_scene::seen::Seen;
use auberean_scene::WorldData;
use auberean_world::sound::block_sound_points;

use crate::host::Host;
use crate::interface::{SoundOrder, SoundSlot};

/// The data files, as the sound manager reads them: each in the client's order.
struct Files(Vec<Arc<DatFile>>);

/// How the sounds an object makes are heard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heard {
    Heard,
    /// Silent for now, to play again.
    Silent,
    /// The object is gone: they stop for good.
    Gone,
}

/// How an object's sounds are heard: `in_world` whether the client's object is in the world (`None`
/// once the client no longer holds it), `muted` what its body answers (`None` with no body).
// D2-06178, D2-07696
#[must_use]
pub fn heard(in_world: Option<bool>, muted: Option<bool>) -> Heard {
    let Some(in_world) = in_world else {
        return Heard::Gone;
    };
    if muted.unwrap_or(!in_world) {
        Heard::Silent
    } else {
        Heard::Heard
    }
}

impl SoundFiles for Files {
    fn read(&self, id: u32) -> Option<Vec<u8>> {
        self.0.iter().find_map(|f| f.read(id).ok())
    }
}

/// The rain's steady sound, how loud it was last set, and when the next clap of thunder may come.
#[derive(Default)]
struct RainSound {
    steady: Option<u32>,
    volume: f32,
    next_thunder: f64,
}

/// The shell's sound.
pub struct Sound {
    pub manager: SoundPlayer,
    /// The host's stream, kept while it plays.
    stream: Option<Box<dyn std::any::Any>>,
    files: Files,
    slots: HashMap<SoundSlot, u32>,
    /// The sounds kept with an object's effect, by object and effect.
    kept: HashMap<(u64, u32), Vec<u32>>,
    block_length: f32,
    /// The ambient sound by place, whether it was given the region's table since the world was
    /// entered, and the landscape's points of the blocks heard.
    ambient: Ambient,
    ambient_on: bool,
    land_points: HashMap<(u8, u8), Arc<[LandPoint]>>,
    /// The rain's sounds.
    rain: RainSound,
    /// The loops of portal space, while they play.
    portal_loops: Vec<u32>,
    /// The loudest sample the device was handed since last read (as `f32` bits), and the blocks
    /// it was handed and of them those filled with silence because the game held the mixer.
    peak: Arc<AtomicU32>,
    blocks: Arc<AtomicU64>,
    underruns: Arc<AtomicU64>,
}

impl std::fmt::Debug for Sound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sound")
            .field("device", &self.stream.is_some())
            .field("playing", &self.manager.playing_count())
            .finish_non_exhaustive()
    }
}

impl Sound {
    /// The sound over the host's device, when it has one that starts (a manager that plays nothing
    /// otherwise), reading the records from `files`; places in the world's frames are in blocks
    /// `block_length` long.
    pub fn start<H: Host>(host: &mut H, files: Vec<Arc<DatFile>>, block_length: f32) -> Self {
        let mut stream = None;
        let mut manager = SoundPlayer::new(None);
        let peak = Arc::new(AtomicU32::new(0));
        let blocks = Arc::new(AtomicU64::new(0));
        let underruns = Arc::new(AtomicU64::new(0));
        if let Some(out) = host.sound_output() {
            match out.probe() {
                Ok((rate, channels)) => {
                    let m = SoundPlayer::new(Some(rate));
                    if let Some(mixer) = m.mixer() {
                        let channels = usize::from(channels.max(1));
                        let mut scratch = Vec::new();
                        let (pk, bl, un) = (
                            Arc::clone(&peak),
                            Arc::clone(&blocks),
                            Arc::clone(&underruns),
                        );
                        let fill = Box::new(move |block: &mut [f32]| {
                            match mixer.try_lock() {
                                Ok(mut mx) => mx.fill(block, channels, &mut scratch),
                                // The game holds the mixer for a moment: a block of silence.
                                Err(_) => {
                                    block.fill(0.0);
                                    un.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                            bl.fetch_add(1, Ordering::Relaxed);
                            let loudest = block.iter().fold(0.0f32, |m, s| m.max(s.abs()));
                            pk.fetch_max(loudest.to_bits(), Ordering::Relaxed);
                        });
                        match out.start(fill) {
                            Ok(s) => {
                                tracing::info!(rate, channels, "sound started");
                                stream = Some(s);
                                manager = m;
                            }
                            Err(e) => {
                                tracing::warn!("the sound would not start: {e}; running silent")
                            }
                        }
                    }
                }
                Err(e) => tracing::warn!("no sound device: {e}; running silent"),
            }
        }
        Self {
            manager,
            stream,
            files: Files(files),
            slots: HashMap::new(),
            kept: HashMap::new(),
            block_length,
            ambient: Ambient::new(block_length),
            ambient_on: false,
            land_points: HashMap::new(),
            rain: RainSound::default(),
            portal_loops: Vec::new(),
            peak,
            blocks,
            underruns,
        }
    }

    /// The loudest sample the device was handed since the last call (0 for silence), and how many
    /// blocks it has been handed, and of them how many were silence because the game held the
    /// mixer.
    pub fn take_meter(&self) -> (f32, u64, u64) {
        let peak = f32::from_bits(self.peak.swap(0, Ordering::Relaxed));
        (
            peak,
            self.blocks.load(Ordering::Relaxed),
            self.underruns.load(Ordering::Relaxed),
        )
    }

    /// The ambient sound by place.
    #[must_use]
    pub fn ambient(&self) -> &Ambient {
        &self.ambient
    }

    /// Whether a device plays the sound.
    #[must_use]
    pub fn has_device(&self) -> bool {
        self.stream.is_some()
    }

    /// Whether the sound a slot keeps is done (or it keeps none).
    #[must_use]
    pub fn finished(&self, slot: SoundSlot) -> bool {
        self.manager
            .has_finished(self.slots.get(&slot).copied().unwrap_or(0))
    }

    /// Carry out one of the interface's requests.
    pub fn request(&mut self, r: SoundOrder) {
        match r {
            SoundOrder::Play { data, slot } => {
                let handle = self.manager.play(&self.files, &data);
                tracing::debug!(
                    id = format_args!("{:08X}", data.id),
                    ?slot,
                    ?handle,
                    "sound asked for"
                );
                if let Some(slot) = slot {
                    match handle {
                        Some(h) => {
                            self.slots.insert(slot, h);
                        }
                        None => {
                            self.slots.remove(&slot);
                        }
                    }
                }
            }
            SoundOrder::Stop { slot, fade } => {
                if let Some(h) = self.slots.remove(&slot) {
                    self.manager.stop(h, fade);
                }
            }
            SoundOrder::Groove(level) => self.manager.set_groove(level),
            SoundOrder::Preference {
                key,
                value,
                applied,
            } => {
                if applied {
                    self.manager.handle_apply(key, value);
                } else {
                    self.manager.handle_change(key, value);
                }
            }
        }
    }

    /// Carry out a sound an object's effect played or stopped.
    // D2-06900
    pub fn effect(&mut self, object: u64, s: FxSound) {
        match s {
            FxSound::Play {
                effect,
                sound,
                volume,
                kept,
            } => {
                let req = auberean_audio::effect_sound(&self.files, sound, volume, Some(object));
                let handle = self.manager.play(&self.files, &req);
                tracing::debug!(
                    object = format_args!("{object:X}"),
                    effect = format_args!("{effect:08X}"),
                    sound = format_args!("{sound:08X}"),
                    volume,
                    ?handle,
                    "an effect's sound"
                );
                if let (true, Some(h)) = (kept, handle) {
                    self.kept.entry((object, effect)).or_default().push(h);
                }
            }
            // D2-01716
            FxSound::Stop { effect } => {
                for h in self.kept.remove(&(object, effect)).unwrap_or_default() {
                    self.manager.stop(h, 0.0);
                }
            }
            // D2-06924, D2-06932
            FxSound::Volume { effect, volume } => {
                for &h in self.kept.get(&(object, effect)).into_iter().flatten() {
                    self.manager.set_volume(h, volume);
                }
            }
        }
    }

    /// The rain's sounds at `amount` of rain heard (0 under cover or out of the world): a steady
    /// sound as loud as the rain, and over 0.4 a clap of thunder at a random loudness from a
    /// quarter up every 15 to 60 seconds.
    // D2-11010
    pub fn rain(&mut self, amount: f32, now: f64, random: &Random, by_enum: &dyn Fn(u32) -> u32) {
        use auberean_draw::rain::{STEADY_SOUND, THUNDER};
        let amount = amount.clamp(0.0, 1.0);
        if amount > 0.4 && now > self.rain.next_thunder {
            let k = usize::try_from(random.roll_int(0, 3)).unwrap_or(0);
            let id = by_enum(THUNDER[k.min(3)]);
            if id != 0 {
                let mut req = auberean_audio::request::PlayRequest::new(id);
                req.volume = random.roll_float(0.25, 1.0);
                self.manager.play(&self.files, &req);
            }
            self.rain.next_thunder = f64::from(random.roll_float(15.0, 60.0)) + now;
        }
        if (amount - self.rain.volume).abs() < 0.0002 {
            return;
        }
        match self.rain.steady {
            None => {
                let mut req = auberean_audio::request::PlayRequest::new(by_enum(STEADY_SOUND));
                req.volume = amount;
                self.rain.steady = self.manager.play(&self.files, &req);
            }
            Some(h) if amount.abs() < 0.0002 => {
                self.manager.stop(h, 0.0);
                self.rain.steady = None;
            }
            Some(h) => self.manager.set_volume(h, amount),
        }
        self.rain.volume = amount;
    }

    /// Portal space started or stopped: started, its start sound and its two loops kept; stopped,
    /// its stop sound and its loops stopped at once. All heard where the listener is.
    // D2-11047, D2-11051
    pub fn portal(&mut self, order: PassageSound, by_enum: &dyn Fn(u32) -> u32) {
        use auberean_audio::request::PlayRequest;
        use auberean_draw::portal::{LOOP_SOUNDS, START_SOUND, STOP_SOUND};
        match order {
            PassageSound::Start => {
                self.manager
                    .play(&self.files, &PlayRequest::new(by_enum(START_SOUND)));
                for l in LOOP_SOUNDS {
                    let handle = self
                        .manager
                        .play(&self.files, &PlayRequest::new(by_enum(l)));
                    self.portal_loops.extend(handle);
                }
            }
            PassageSound::Stop => {
                self.manager
                    .play(&self.files, &PlayRequest::new(by_enum(STOP_SOUND)));
                for h in std::mem::take(&mut self.portal_loops) {
                    self.manager.stop(h, 0.0);
                }
            }
        }
    }

    /// Every sound object `object` makes stops: it is gone.
    pub fn object_gone(&mut self, object: u64) {
        self.kept.retain(|(o, _), _| *o != object);
        self.manager.stop_for_emitter(object);
    }

    /// The frame's turn at `now`: the ambient sound's (given the region's table when the world is
    /// entered, cleared when it is left), then the manager's, with the listener at the camera of
    /// the world last drawn (`seen`, when `in_world`), each object making a sound where it was
    /// drawn and each ambient object where the ambient sound stands it. The ambient sound draws
    /// from `random`.
    pub fn frame(
        &mut self,
        now: f64,
        seen: &Seen,
        in_world: bool,
        world: &WorldData,
        random: &Random,
    ) {
        let bl = self.block_length;
        #[allow(clippy::cast_precision_loss)] // block numbers below 256
        let origin = [seen.block.0 as f32 * bl, seen.block.1 as f32 * bl];
        let global = |p: auberean_primitives::Vec3| [p.x + origin[0], p.y + origin[1], p.z];
        self.ambient_frame(now, seen, in_world, world, random, &global);
        let listener = match seen.camera.filter(|_| in_world) {
            Some(c) => {
                let (f, r, u) = c.axes();
                Listener {
                    position: global(c.position),
                    right: [r.x, r.y, r.z],
                    ahead: [f.x, f.y, f.z],
                    up: [u.x, u.y, u.z],
                }
            }
            None => Listener::default(),
        };
        let ambient = &self.ambient;
        let emitters = |id: u64| {
            if !in_world {
                return None;
            }
            if ambient::is_emitter(id) {
                return ambient.emitter_place(id);
            }
            seen.body(id).map(|b| global(b.origin))
        };
        self.manager.use_time(now, listener, &emitters);
    }

    /// The ambient sound's turn: while the scene hears the player, the camera it is heard from,
    /// the update of what it hears and its own turn; once the player is gone, it is cleared.
    fn ambient_frame(
        &mut self,
        now: f64,
        seen: &Seen,
        in_world: bool,
        world: &WorldData,
        random: &Random,
        global: &dyn Fn(auberean_primitives::Vec3) -> [f32; 3],
    ) {
        let mut speaker = ambient::Manager {
            player: &mut self.manager,
            files: &self.files,
        };
        let Some(h) = seen.hearing.as_ref().filter(|_| in_world) else {
            if self.ambient_on {
                self.ambient.clear(&mut speaker);
                self.land_points.clear();
                self.ambient_on = false;
            }
            return;
        };
        if !self.ambient_on {
            // The region's table, as the region is set on entering the world.
            let table = world.region.region.sounds.first().map_or(0, |r| r.record.0);
            self.ambient.set_region(table);
            self.ambient_on = true;
        }
        if let Some(c) = seen.camera {
            self.ambient.set_viewer(global(c.position));
        }
        // The scene's places are in the player's landblock's coordinates.
        let bl = self.block_length;
        let corner = [
            f32::from(u8::try_from(h.cell >> 24).unwrap_or(0)) * bl,
            f32::from(u8::try_from((h.cell >> 16) & 0xFF).unwrap_or(0)) * bl,
        ];
        let at = |p: auberean_primitives::Vec3| [p.x + corner[0], p.y + corner[1], p.z];
        let cells: Vec<(u32, [f32; 3])> = h.cells.iter().map(|(k, c)| (*k, at(*c))).collect();
        let around = Surroundings {
            cell: h.cell,
            player: at(h.place),
            blocking: h.blocking,
            cells: &cells,
            outside: h.outside,
            blocks: &h.blocks,
        };
        self.land_points.retain(|b, _| h.blocks.contains(b));
        let mut land = WorldLand {
            world,
            kept: &mut self.land_points,
        };
        self.ambient
            .update(&around, &mut land, now, &mut speaker, random);
        self.ambient.use_time(now, &mut speaker, random);
    }
}

/// The landscape's ambient points of the blocks heard, worked out from the world's data the first
/// time a block is heard and kept while it is.
struct WorldLand<'a> {
    world: &'a WorldData,
    kept: &'a mut HashMap<(u8, u8), Arc<[LandPoint]>>,
}

impl LandPoints for WorldLand<'_> {
    fn points(&mut self, block: (u8, u8)) -> Arc<[LandPoint]> {
        let world = self.world;
        Arc::clone(self.kept.entry(block).or_insert_with(|| {
            let region = &world.region;
            let (Some(scenes), Some(b)) = (
                region.scenes.as_ref(),
                world
                    .records
                    .landblock(i32::from(block.0), i32::from(block.1)),
            ) else {
                return Arc::from(Vec::new());
            };
            block_sound_points(&region.land, &region.terrain, scenes, &b)
                .into_iter()
                .map(|p| LandPoint {
                    kind: p.kind,
                    origin: [p.origin.x, p.origin.y, p.origin.z],
                })
                .collect()
        }))
    }
}
