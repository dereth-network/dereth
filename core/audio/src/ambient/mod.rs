//! The ambient sound system: its controller, intermittent sounds and constant
//! sounds.
//!
//! The terrain of the 3x3 landblock neighbourhood contributes a distance weight to a
//! set of per-scene descriptors; each descriptor's entries schedule themselves on a min-heap keyed by
//! absolute time; each firing plays one row of that scene's sound table.
//!
//! Three things here are shipped behaviour that must be kept, and are easy to "improve" by
//! accident:
//!
//! * **Nothing loops.** A constant sound is a sample **re-triggered every `min_rate` seconds** at a
//!   volume proportional to the terrain share. If the authored sample happens to be `min_rate` long
//!   it sounds seamless; drift is inevitable and is part of the original's character.
//! * **The ambient sound list is never pruned.** Only the destructor clears it, and nothing
//!   calls it except ambient teardown, so the list grows monotonically over a session as the player visits
//!   new terrain/scene combinations, and stale entries survive with `sound_count == 0`.
//! * **The recompute happens on cell/position change, not per frame.** `total_sound_count` is a
//!   per-pass sum; recomputing it every frame gives slightly different volumes.

pub mod place;
pub mod scan;
pub mod sched;
pub mod weight;

use dereth_assets::region::Region;
use dereth_primitives::num::rng::Ran2;
use dereth_primitives::{DataId, LocalTime, Vec3};

use place::DirSet;
use scan::TerrainNeighbourhood;
use sched::PQueueArray;
use weight::{calc_dir, calc_weight, Direction, AMBIENT_SOUND_MIN_DIST, AMBIENT_SOUND_MIN_VOL};

/// Extra state corresponding to the original client's 0x80-byte intermittent-sound subclass.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct IntermitState {
    /// Recomputed when sound weights update; the per-pass reset zeroes it.
    pub play_chance: f32,
    pub dirs: DirSet,
}

/// Extra state corresponding to the original client's 0x1C-byte constant-sound subclass.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ConstantState {
    /// Recomputed when sound weights update, and **zeroed** when the terrain share drops to nothing — which
    /// is what makes a continuous sound fade out as you walk away.
    pub current_volume: f32,
}

/// The two concrete `AmbientSound` subclasses, chosen by `is_continuous`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// `base_chance != 0`: the intermittent kind.
    Intermit(IntermitState),
    /// `base_chance == 0`: the constant kind.
    Constant(ConstantState),
}

/// One `AmbientSound` (24 bytes plus the subclass tail).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AmbientSound {
    /// Index into `Region::sound_info` — the ambient sound-table descriptor pointer in the original.
    pub desc: usize,
    /// `ambient_sound_id`, the index into `desc->ambient_sounds`.
    pub ambient_sound_id: usize,
    pub on_queue: bool,
    /// The accumulated distance weight of the terrain that referenced this scene this pass.
    pub sound_count: f32,
    pub kind: Kind,
}

/// One firing of an ambient sound, ready for [`crate::trigger`] to turn into a play.
///
/// The split is the client's own: a sound whose position query succeeds uses the positioned
/// playback path (entry 5); returning 0 uses centered playback
/// (entry 6, with pan 0 and distance 0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AmbientPlay {
    /// DataID of the scene's sound table.
    pub stb_id: DataId,
    /// `AmbientSoundDesc::stype`, the row of that table.
    pub stype: u32,
    /// `None` for the from-centre form.
    pub pos: Option<Vec3>,
    /// The computed volume — **before** the two applications of `ambient_volume`.
    pub volume: f32,
}

/// The single ambient controller owned by the world-object system.
#[derive(Debug, Default)]
pub struct Ambient {
    /// Where the last ambient recomputation began.
    pub player_pos: Vec3,
    /// The per-pass sum of every contributing cell's weight.
    pub total_sound_count: f32,
    /// Never pruned between passes. See the module docs.
    pub sounds: Vec<AmbientSound>,
    /// The per-descriptor play count, one per `Region::sound_info` entry: how many `AmbientSound`s
    /// referenced that descriptor this pass. End-of-pass cleanup drops the table of every
    /// descriptor still at zero.
    play_count: Vec<u32>,
    queue: PQueueArray<usize>,
}

impl Ambient {
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: PQueueArray::new(),
            ..Self::default()
        }
    }

    /// How many sounds are on the queue.
    #[must_use]
    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// Start a pass: remember the position, zero `total_sound_count`, and reset
    /// every sound's per-pass counts.
    ///
    /// The reset pass zeroes each descriptor's play count, `sound_count`, and — for an
    /// intermittent sound — `num_dir` and `play_chance`. Resetting a constant sound does **not**
    /// touch `current_volume`; only the subsequent sound-weight update does.
    fn init_sounds(&mut self, pos: Vec3, num_descs: usize) {
        self.player_pos = pos;
        self.total_sound_count = 0.0;
        self.play_count.clear();
        self.play_count.resize(num_descs, 0);
        for s in &mut self.sounds {
            s.sound_count = 0.0;
            if let Kind::Intermit(i) = &mut s.kind {
                i.dirs.reset();
                i.play_chance = 0.0;
            }
        }
    }

    /// Add one descriptor: find or create the `AmbientSound` for `(desc, index)` by a
    /// linear search over the existing list, appending on a miss.
    fn get_sound(&mut self, region: &Region, desc: usize, index: usize) -> usize {
        if let Some(i) = self
            .sounds
            .iter()
            .position(|s| s.desc == desc && s.ambient_sound_id == index)
        {
            return i;
        }
        // `is_continuous` is derived, not stored: the client sets it to
        // `(base_chance == 0.0f)`.
        let is_continuous = region
            .sound_info
            .as_ref()
            .and_then(|v| v.get(desc))
            .and_then(|d| d.ambient_sounds.get(index))
            .is_some_and(|a| a.base_chance == 0.0);
        self.sounds.push(AmbientSound {
            desc,
            ambient_sound_id: index,
            on_queue: false,
            sound_count: 0.0,
            kind: if is_continuous {
                Kind::Constant(ConstantState::default())
            } else {
                Kind::Intermit(IntermitState::default())
            },
        });
        self.sounds.len() - 1
    }

    /// One terrain cell's contribution.
    fn add_sound(&mut self, region: &Region, desc: usize, cell_pos: Vec3, enabled: bool) {
        if !enabled {
            return;
        }
        // The cell offset is `cell_pos - player_pos`.
        let off = Vec3::new(
            cell_pos.x - self.player_pos.x,
            cell_pos.y - self.player_pos.y,
            cell_pos.z - self.player_pos.z,
        );
        if off.x * off.x + off.y * off.y + off.z * off.z > weight::AMBIENT_SOUND_MAX_DIST_SQ {
            return;
        }
        // Both are computed before the zero test, and both are computed even when the weight is 0.
        let w = calc_weight(off);
        let dir = calc_dir(off);
        if w == 0.0 {
            return;
        }
        self.total_sound_count += w;
        let n = region
            .sound_info
            .as_ref()
            .and_then(|v| v.get(desc))
            .map_or(0, |d| d.ambient_sounds.len());
        for i in 0..n {
            let si = self.get_sound(region, desc, i);
            add_to(&mut self.sounds[si], w, off, dir);
        }
    }

    /// Close the pass: recompute every sound's chance or volume, then put
    /// anything not already on the queue onto it.
    fn update_play_queue(
        &mut self,
        region: &Region,
        now: LocalTime,
        rng: &mut Ran2,
        table_loaded: &dyn Fn(DataId) -> bool,
        out: &mut Vec<AmbientPlay>,
    ) {
        let total = self.total_sound_count;
        for i in 0..self.sounds.len() {
            self.update_sound(region, i, total);
            if !self.sounds[i].on_queue {
                self.play(region, i, now, rng, table_loaded, out);
            }
        }
    }

    /// The per-sound update, which differs for an intermittent and a constant sound.
    ///
    /// `play_chance = (base_chance / total) * sound_count` and
    /// `current_volume = (volume / total) * sound_count`: the scene's **share of the surrounding
    /// terrain**, weighted by distance. A share of ~1 means the scene fills the neighbourhood.
    ///
    /// The asymmetry is real: an intermittent sound leaves `play_chance` alone when `sound_count == 0`
    /// (only the reset pass zeroes it) while a constant sound explicitly zeroes `current_volume`.
    fn update_sound(&mut self, region: &Region, i: usize, total: f32) {
        let (desc, id) = (self.sounds[i].desc, self.sounds[i].ambient_sound_id);
        let Some(a) = region
            .sound_info
            .as_ref()
            .and_then(|v| v.get(desc))
            .and_then(|d| d.ambient_sounds.get(id))
            .copied()
        else {
            return;
        };
        let count = self.sounds[i].sound_count;
        match &mut self.sounds[i].kind {
            Kind::Intermit(s) => {
                if count > 0.0 {
                    if let Some(c) = self.play_count.get_mut(desc) {
                        *c += 1;
                    }
                    s.play_chance = (a.base_chance / total) * count;
                }
            }
            Kind::Constant(s) => {
                if count == 0.0 {
                    s.current_volume = 0.0;
                } else {
                    if let Some(c) = self.play_count.get_mut(desc) {
                        *c += 1;
                    }
                    s.current_volume = (a.volume / total) * count;
                }
            }
        }
    }

    /// Play whatever the queue says is due.
    ///
    /// The `ran2` draw order across one call, which is what a seeded replay pins:
    ///
    /// 1. the audibility check — no draw.
    /// 2. the play-now decision — one draw for an intermittent sound
    ///    (`roll_f32(0, 1) < play_chance`), none for a constant sound, which always returns 1.
    /// 3. sound-position selection — three draws for an intermittent sound (direction, angle, radius), none for
    ///    a constant sound, which returns 0 and plays from centre.
    /// 4. the sound-table row pick in the playback path — one draw, made by the caller.
    /// 5. play-interval selection — one draw for an intermittent sound
    ///    (`roll_f32(min_rate, max_rate)`), none for a constant sound, whose interval is exactly
    ///    `min_rate`.
    ///
    /// Step 5 runs **even when the play-now decision says no**, so an intermittent sound consumes a draw every
    /// time it comes off the queue whether or not it is heard.
    fn play(
        &mut self,
        region: &Region,
        i: usize,
        now: LocalTime,
        rng: &mut Ran2,
        table_loaded: &dyn Fn(DataId) -> bool,
        out: &mut Vec<AmbientPlay>,
    ) {
        let (desc, id) = (self.sounds[i].desc, self.sounds[i].ambient_sound_id);
        let Some(d) = region.sound_info.as_ref().and_then(|v| v.get(desc)) else {
            self.sounds[i].on_queue = false;
            return;
        };
        let Some(a) = d.ambient_sounds.get(id).copied() else {
            self.sounds[i].on_queue = false;
            return;
        };

        // An inaudible sound drops off the queue entirely and is
        // only put back the next time the play queue is recomputed.
        if !can_hear(&self.sounds[i], table_loaded(d.stb_id)) {
            self.sounds[i].on_queue = false;
            return;
        }

        if play_now(&self.sounds[i], rng) {
            let volume = get_volume(&self.sounds[i], a.volume);
            let pos = match &self.sounds[i].kind {
                Kind::Intermit(s) => place::get_sound_pos(&s.dirs, rng).map(|off| {
                    // z is the listener's: only x and y are offset.
                    Vec3::new(
                        self.player_pos.x + off.x,
                        self.player_pos.y + off.y,
                        self.player_pos.z,
                    )
                }),
                Kind::Constant(_) => None,
            };
            out.push(AmbientPlay {
                stb_id: d.stb_id,
                stype: a.stype,
                pos,
                volume,
            });
        }

        let interval = get_play_interval(&self.sounds[i], a.min_rate, a.max_rate, rng);
        self.queue.insert(now.0 + f64::from(interval), i);
        self.sounds[i].on_queue = true;
    }

    /// Pop everything strictly earlier than now and play it.
    ///
    /// Note this keeps running while the window is inactive: the scheduler tests only
    /// `ambient_enabled`. That is deliberate — the schedule does not pause, so regaining
    /// focus produces no pile-up. What suppresses the *audio* is the focus test inside
    /// the internal play routine. See [`crate::focus`].
    pub fn use_time(
        &mut self,
        region: &Region,
        now: LocalTime,
        enabled: bool,
        rng: &mut Ran2,
        table_loaded: &dyn Fn(DataId) -> bool,
    ) -> Vec<AmbientPlay> {
        let mut out = Vec::new();
        if !enabled {
            return out;
        }
        while self.queue.peek_key().is_some_and(|k| k < now.0) {
            let Some((_, i)) = self.queue.remove_min() else {
                break;
            };
            self.play(region, i, now, rng, table_loaded, &mut out);
        }
        out
    }

    /// The ambient half of a region change: reset the sounds, scan the terrain,
    /// update the play queue, then release unused sound tables.
    ///
    /// **Called only when the player's cell or position actually changed**, never once per frame.
    #[allow(clippy::too_many_arguments)]
    pub fn on_position_changed(
        &mut self,
        region: &Region,
        listener: Vec3,
        cells: &TerrainNeighbourhood,
        now: LocalTime,
        enabled: bool,
        rng: &mut Ran2,
        table_loaded: &dyn Fn(DataId) -> bool,
    ) -> Vec<AmbientPlay> {
        let num_descs = region.sound_info.as_ref().map_or(0, Vec::len);
        self.init_sounds(listener, num_descs);
        if cells.outdoors {
            for c in &cells.cells {
                if let Some(desc) = scan::stb_desc_index(region, c.terrain_type(), c.scene_index())
                {
                    self.add_sound(region, desc, c.pos, enabled);
                }
            }
        }
        let mut out = Vec::new();
        if enabled {
            self.update_play_queue(region, now, rng, table_loaded, &mut out);
        }
        out
    }

    /// Release unused tables: the descriptors whose `play_count` is still 0
    /// contributed nothing this pass, and their sound tables are released.
    #[must_use]
    pub fn unused_descs(&self) -> Vec<usize> {
        self.play_count
            .iter()
            .enumerate()
            .filter(|(_, &c)| c == 0)
            .map(|(i, _)| i)
            .collect()
    }
}

/// Accumulate one cell into a sound, for both the intermittent and the constant kind.
///
/// A constant sound discards the direction and the distance entirely. An intermittent sound records a
/// `r +/- 10 m` band for the contributing direction, or — for a cell inside the viewer block —
/// `[4, 10]` in **all eight** compass directions.
fn add_to(s: &mut AmbientSound, w: f32, off: Vec3, dir: Direction) {
    s.sound_count += w;
    let Kind::Intermit(i) = &mut s.kind else {
        return;
    };
    let half = AMBIENT_SOUND_MIN_DIST * 0.5;
    let r = (off.x * off.x + off.y * off.y + off.z * off.z).sqrt();
    if dir == Direction::InViewerBlock {
        for d in weight::IN_BLOCK_DIRECTIONS {
            i.dirs.add_dir(d, 4.0, half);
        }
    } else {
        i.dirs.add_dir(dir, r - half, r + half);
    }
}

/// An intermittent sound can be heard when its `play_chance` is above 0; a constant one when
/// its computed volume is at least `0.03` **and** its sound table has loaded.
///
/// `table_loaded` is `desc->sound_table != NULL` — the lazily loaded sound table, which
/// the loader fetches once and never retries after a failure
/// (`stb_not_found` latches). It matters only for a constant sound: a scene whose table will not
/// load falls off the queue instead of re-triggering forever. Every shipped region descriptor's
/// `stb_id` resolves, so retail data never takes that branch.
#[must_use]
pub fn can_hear(s: &AmbientSound, table_loaded: bool) -> bool {
    match &s.kind {
        Kind::Intermit(i) => i.play_chance > 0.0,
        Kind::Constant(c) => c.current_volume >= AMBIENT_SOUND_MIN_VOL && table_loaded,
    }
}

/// The trigger test is `roll_f32(0, 1) < play_chance`, a **`ran2`** draw,
/// not the CRT one. Constant sounds always return true.
#[must_use]
pub fn play_now(s: &AmbientSound, rng: &mut Ran2) -> bool {
    match &s.kind {
        Kind::Intermit(i) => rng.roll_f32(0.0, 1.0) < i.play_chance,
        Kind::Constant(_) => true,
    }
}

/// An intermittent sound's volume is the **authored** one; a constant sound's is
/// `current_volume`, which is the authored one scaled by the terrain share.
#[must_use]
pub fn get_volume(s: &AmbientSound, authored: f32) -> f32 {
    match &s.kind {
        Kind::Intermit(_) => authored,
        Kind::Constant(c) => c.current_volume,
    }
}

/// An intermittent sound draws its interval (`roll_f32(min_rate, max_rate)`) and a constant one
/// takes exactly `min_rate` with no draw.
#[must_use]
pub fn get_play_interval(s: &AmbientSound, min_rate: f32, max_rate: f32, rng: &mut Ran2) -> f32 {
    match &s.kind {
        Kind::Intermit(_) => rng.roll_f32(min_rate, max_rate),
        Kind::Constant(_) => min_rate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::test_region;
    use scan::TerrainCell;

    /// Every shipped region descriptor's `stb_id` resolves, so `desc->sound_table != NULL` holds for
    /// all of them; `can_hear`'s table test is exercised separately.
    const LOADED: fn(DataId) -> bool = |_| true;

    /// A continuous ambient sound is a **re-trigger on a timer**, not a loop: it comes back onto the
    /// queue at exactly `min_rate` every time, with no draw. Oracle: a constant sound's play
    /// interval.
    #[test]
    fn continuous_ambience_re_triggers_at_min_rate_with_no_random_draw() {
        let region = test_region();
        let mut amb = Ambient::new();
        let mut rng = Ran2::new(1);
        let cells = TerrainNeighbourhood {
            outdoors: true,
            // Terrain type 1, scene 0 -> the continuous descriptor in `test_region`.
            cells: vec![TerrainCell {
                pos: Vec3::new(1.0, 0.0, 0.0),
                terrain_word: 1 << 2,
            }],
        };
        let first = amb.on_position_changed(
            &region,
            Vec3::ZERO,
            &cells,
            LocalTime(100.0),
            true,
            &mut rng,
            &LOADED,
        );
        assert_eq!(
            first.len(),
            1,
            "a constant ambient sound plays immediately when it joins the queue"
        );
        assert_eq!(first[0].pos, None, "and always from centre");
        // The interval must be exactly min_rate (2.0 in the fixture) and consume no randomness.
        let before = rng.clone().next_f64();
        let again = amb.use_time(&region, LocalTime(102.001), true, &mut rng, &LOADED);
        assert_eq!(again.len(), 1, "re-triggered, not looped");
        assert_eq!(
            rng.clone().next_f64(),
            before,
            "no draw for a continuous interval"
        );
        // Nothing is due before the next min_rate boundary.
        assert!(amb
            .use_time(&region, LocalTime(103.0), true, &mut rng, &LOADED)
            .is_empty());
    }

    /// A constant sound also requires its sound table to have loaded, so a scene whose
    /// sound table will not load falls off the queue instead of re-triggering forever.
    /// The loader latches its "not found" flag and never retries, which
    /// is why this is a terminal state and not a stall. No shipped descriptor takes the branch.
    #[test]
    fn a_continuous_sound_whose_table_will_not_load_leaves_the_queue() {
        let region = test_region();
        let mut amb = Ambient::new();
        let mut rng = Ran2::new(1);
        let cells = TerrainNeighbourhood {
            outdoors: true,
            cells: vec![TerrainCell {
                pos: Vec3::new(1.0, 0.0, 0.0),
                terrain_word: 1 << 2,
            }],
        };
        let missing: fn(DataId) -> bool = |_| false;
        let plays = amb.on_position_changed(
            &region,
            Vec3::ZERO,
            &cells,
            LocalTime(0.0),
            true,
            &mut rng,
            &missing,
        );
        assert!(plays.is_empty(), "nothing plays without a table");
        assert_eq!(amb.queued(), 0, "and it is not scheduled either");
        // With the table present the same scene does schedule.
        let mut amb = Ambient::new();
        let mut rng = Ran2::new(1);
        amb.on_position_changed(
            &region,
            Vec3::ZERO,
            &cells,
            LocalTime(0.0),
            true,
            &mut rng,
            &LOADED,
        );
        assert_eq!(amb.queued(), 1);
    }

    /// The terrain share model: `current_volume = (authored / total) * sound_count`. Standing in one
    /// uniform terrain gives a share of 1; splitting the neighbourhood halves it. Oracle: the
    /// constant-sound update, section 4.6.
    #[test]
    fn the_volume_is_this_scenes_share_of_the_surrounding_terrain() {
        let region = test_region();
        let mut amb = Ambient::new();
        let mut rng = Ran2::new(1);
        let one = TerrainCell {
            pos: Vec3::new(1.0, 0.0, 0.0),
            terrain_word: 1 << 2,
        };
        let other = TerrainCell {
            pos: Vec3::new(-1.0, 0.0, 0.0),
            terrain_word: 2 << 2,
        };

        let all_mine = TerrainNeighbourhood {
            outdoors: true,
            cells: vec![one],
        };
        let plays = amb.on_position_changed(
            &region,
            Vec3::ZERO,
            &all_mine,
            LocalTime(0.0),
            true,
            &mut rng,
            &LOADED,
        );
        assert!(
            (plays[0].volume - 0.8).abs() < 1e-6,
            "the authored volume at a share of 1"
        );

        // A fresh `Ambient`, because the queue update only plays sounds that are *not*
        // already on the queue -- a re-scan does not re-fire what is already scheduled.
        let mut amb = Ambient::new();
        let mut rng = Ran2::new(1);
        let split = TerrainNeighbourhood {
            outdoors: true,
            cells: vec![one, other],
        };
        let plays = amb.on_position_changed(
            &region,
            Vec3::ZERO,
            &split,
            LocalTime(0.0),
            true,
            &mut rng,
            &LOADED,
        );
        let mine = plays
            .iter()
            .find(|p| p.stype == 70)
            .expect("the continuous entry played");
        assert!(
            (mine.volume - 0.4).abs() < 1e-6,
            "half the neighbourhood, half the volume"
        );
    }

    /// Dungeons are ambient-silent: with `outdoors == false` nothing is scanned, so
    /// `total_sound_count` stays 0, every constant sound fades to zero and falls off the queue.
    /// Oracle: section 4.10, marked verified.
    #[test]
    fn indoors_the_ambience_falls_silent_and_leaves_the_queue() {
        let region = test_region();
        let mut amb = Ambient::new();
        let mut rng = Ran2::new(1);
        let cells = TerrainNeighbourhood {
            outdoors: true,
            cells: vec![TerrainCell {
                pos: Vec3::new(1.0, 0.0, 0.0),
                terrain_word: 1 << 2,
            }],
        };
        amb.on_position_changed(
            &region,
            Vec3::ZERO,
            &cells,
            LocalTime(0.0),
            true,
            &mut rng,
            &LOADED,
        );
        assert_eq!(amb.queued(), 1);

        let indoors = TerrainNeighbourhood {
            outdoors: false,
            cells: Vec::new(),
        };
        let plays = amb.on_position_changed(
            &region,
            Vec3::ZERO,
            &indoors,
            LocalTime(1.0),
            true,
            &mut rng,
            &LOADED,
        );
        assert!(plays.is_empty(), "nothing new joins the queue");
        assert_eq!(amb.total_sound_count, 0.0);
        // Its next firing finds it inaudible and it leaves the queue.
        let plays = amb.use_time(&region, LocalTime(10.0), true, &mut rng, &LOADED);
        assert!(plays.is_empty());
        assert_eq!(amb.queued(), 0, "an inaudible sound is not re-inserted");
    }

    /// The sound list is never pruned: revisiting terrain reuses the entry, and leaving it behind
    /// keeps it with `sound_count == 0`. Oracle: section 4.5's note that only destruction
    /// clears the list and nothing calls it.
    #[test]
    fn the_sound_list_grows_monotonically_and_is_never_trimmed() {
        let region = test_region();
        let mut amb = Ambient::new();
        let mut rng = Ran2::new(1);
        let a = TerrainCell {
            pos: Vec3::new(1.0, 0.0, 0.0),
            terrain_word: 1 << 2,
        };
        let b = TerrainCell {
            pos: Vec3::new(1.0, 0.0, 0.0),
            terrain_word: 2 << 2,
        };
        amb.on_position_changed(
            &region,
            Vec3::ZERO,
            &TerrainNeighbourhood {
                outdoors: true,
                cells: vec![a],
            },
            LocalTime(0.0),
            true,
            &mut rng,
            &LOADED,
        );
        let after_first = amb.sounds.len();
        assert!(after_first > 0);
        amb.on_position_changed(
            &region,
            Vec3::ZERO,
            &TerrainNeighbourhood {
                outdoors: true,
                cells: vec![b],
            },
            LocalTime(1.0),
            true,
            &mut rng,
            &LOADED,
        );
        assert!(
            amb.sounds.len() > after_first,
            "the new scene appended rather than replaced"
        );
        assert!(
            amb.sounds.iter().any(|s| s.sound_count == 0.0),
            "the scene we walked away from is still in the list with a zero count"
        );
    }

    /// The draw order across one play of an intermittent sound, which is the observable
    /// part. Oracle: the client's call sequence -- audibility, play-now decision,
    /// position, volume, then unconditional interval selection.
    #[test]
    fn an_intermittent_sound_draws_play_then_position_then_interval() {
        let region = test_region();
        let mut amb = Ambient::new();
        // Terrain type 3, scene 0 -> the intermittent descriptor, far enough away to be directional.
        let cells = TerrainNeighbourhood {
            outdoors: true,
            cells: vec![TerrainCell {
                pos: Vec3::new(0.0, 40.0, 0.0),
                terrain_word: 3 << 2,
            }],
        };
        let seed = 12_345;
        let mut rng = Ran2::new(seed);
        let plays = amb.on_position_changed(
            &region,
            Vec3::ZERO,
            &cells,
            LocalTime(0.0),
            true,
            &mut rng,
            &LOADED,
        );

        // Replay the same draws by hand, in the documented order.
        let mut hand = Ran2::new(seed);
        let played = hand.roll_f32(0.0, 1.0) < 1.0; // base_chance 1.0 at a share of 1
        assert!(
            played,
            "the fixture's intermittent entry has base_chance 1.0"
        );
        let mut dirs = DirSet::default();
        dirs.add_dir(Direction::North, 40.0 - 10.0, 40.0 + 10.0);
        let off = place::get_sound_pos(&dirs, &mut hand).expect("one direction");
        let interval = hand.roll_f32(1.0, 5.0);

        assert_eq!(plays.len(), 1);
        assert_eq!(plays[0].pos, Some(Vec3::new(off.x, off.y, 0.0)));
        // And the generator is left in exactly the same place, so no extra draw was made.
        assert_eq!(rng.next_f64(), hand.next_f64());
        assert!((1.0..=5.0).contains(&interval));
    }

    /// Interval selection runs even when the play-now decision says no, so an intermittent sound consumes a draw
    /// every time it comes off the queue. Oracle: interval selection
    /// is outside the conditional playback block.
    #[test]
    fn a_silent_intermittent_firing_still_consumes_an_interval_draw() {
        let mut s = AmbientSound {
            desc: 0,
            ambient_sound_id: 0,
            on_queue: false,
            sound_count: 1.0,
            kind: Kind::Intermit(IntermitState {
                play_chance: 0.0,
                dirs: DirSet::default(),
            }),
        };
        let mut rng = Ran2::new(5);
        // A zero play chance makes the play-now decision false...
        assert!(!play_now(&s, &mut rng));
        // ..but the interval draw happens anyway.
        let a = get_play_interval(&s, 1.0, 5.0, &mut rng);
        let b = get_play_interval(&s, 1.0, 5.0, &mut rng);
        assert_ne!(a, b, "each firing draws a fresh interval");
        s.kind = Kind::Constant(ConstantState {
            current_volume: 1.0,
        });
        let before = rng.clone().next_f64();
        assert_eq!(
            get_play_interval(&s, 2.0, 9.0, &mut rng),
            2.0,
            "constant is exactly min_rate"
        );
        assert_eq!(rng.next_f64(), before, "and draws nothing");
    }

    /// A cell inside the viewer block seeds all eight directions with `[4, 10]`; a distant one seeds
    /// its own direction with `r +/- 10`. Oracle: the client's per-cell accumulation.
    #[test]
    fn an_in_block_cell_seeds_all_eight_directions() {
        let mut s = AmbientSound {
            desc: 0,
            ambient_sound_id: 0,
            on_queue: false,
            sound_count: 0.0,
            kind: Kind::Intermit(IntermitState::default()),
        };
        add_to(
            &mut s,
            1.0,
            Vec3::new(3.0, 3.0, 0.0),
            Direction::InViewerBlock,
        );
        let Kind::Intermit(i) = s.kind else {
            panic!("intermittent")
        };
        assert_eq!(i.dirs.num_dir, 8);
        assert_eq!(i.dirs.min_dist[0], 4.0);
        assert_eq!(i.dirs.max_dist[0], 10.0);

        let mut s2 = AmbientSound {
            sound_count: 0.0,
            ..s
        };
        s2.kind = Kind::Intermit(IntermitState::default());
        add_to(&mut s2, 1.0, Vec3::new(0.0, 50.0, 0.0), Direction::North);
        let Kind::Intermit(i) = s2.kind else {
            panic!("intermittent")
        };
        assert_eq!(i.dirs.num_dir, 1);
        assert_eq!(i.dirs.min_dist[0], 40.0);
        assert_eq!(i.dirs.max_dist[0], 60.0);
    }
}
