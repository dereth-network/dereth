//! Ambient sound by place: the sounds a kind of place makes, picked by what lies around the
//! player and heard from where the camera is.
//!
//! - **Places name kinds.** Every interior cell the player can see names its kind; the landscape's
//!   points (a landblock's vertices whose scenery carries a
//!   sound word) each name theirs. The region's ambient sound table describes each kind: its sound
//!   environment, and its sounds that play now and then and continuously ([`desc`],
//!   [`objects`]).
//! - **Recomputed as the player moves.** Once the player has moved 4 meters since the last
//!   recompute (and the cells about it are not being waited for), every place within 100 meters
//!   is weighed by its level distance (fully within 12 meters, by the square of 12 over the
//!   square of its distance beyond, half when it lies on the other side of the player's
//!   indoors or outdoors) into its kind's description, which is made the first time a kind is
//!   met. Each description's share of the whole decides whether it is heard (at least a
//!   hundredth) and the description with the greatest share dominates the place.
//! - **What plays.** A heard description's continuous sounds play at its share as their volume;
//!   those that wait for dominance play only while their description dominates, and stop when
//!   it no longer does (unless they play on). Its intermittent sounds play now and then by chance.
//!   In the end-of-retail data every description holds only continuous place music: the place's
//!   piece, played on the primary track while the description dominates.
//! - **Heard from the camera.** Each frame the sounds stand about where the camera was last drawn
//!   from; a description quiet for more than half a second's check goes, and its sounds stop.
//! - The dominant description's sound environment changes nothing: the client hands it to a
//!   function that does nothing.

pub mod desc;
pub mod objects;

use std::sync::Arc;

use auberean_assets::region::SoundDesc;
use auberean_primitives::random::Random;

use crate::manager::{stop, SoundPlayer};
use crate::records::{self, SettingType, SoundFiles};
use crate::request::PlayRequest;
use crate::settings::{MUSIC_PLACED, SOUND_PLACED};
use desc::Description;

/// A place in the world's axes: meters east, north and up from the south-west corner of
/// landblock (0, 0).
pub type Place = [f32; 3];

/// The emitters of the ambient sounds' objects: these top bits, then a running number. No
/// object of the world has them.
pub const EMITTERS: u64 = 0xA3B1_0000_0000_0000;
const EMITTER_MASK: u64 = 0xFFFF_0000_0000_0000;

/// Whether `id` is the emitter of one of the ambient sounds' objects: the id an ambient object
/// gives the sound manager, which names no object of the world.
// D2-03364
#[must_use]
pub fn is_emitter(id: u64) -> bool {
    id & EMITTER_MASK == EMITTERS
}

/// A point of the landscape that names a kind, in its landblock's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LandPoint {
    pub kind: u32,
    pub origin: Place,
}

/// The landscape's points of each landblock, which the world works out from its data.
pub trait LandPoints {
    /// The points of landblock `(x, y)` (none when it has no data).
    fn points(&mut self, block: (u8, u8)) -> Arc<[LandPoint]>;
}

/// What the world says about the player, as the cells about it stand.
#[derive(Clone, Copy, Debug)]
pub struct Surroundings<'a> {
    /// The player's cell (`0xXXYYnnnn`; an outdoor cell below `0x100` in its low word).
    pub cell: u32,
    pub player: Place,
    /// The cells about the player are being waited for: nothing is recomputed.
    pub blocking: bool,
    /// The interior cells in view: the kind each names and where its origin is.
    pub cells: &'a [(u32, Place)],
    /// The landscape's points are heard.
    pub outside: bool,
    /// The landblocks whose points are heard (the landscape's near window).
    pub blocks: &'a [(u8, u8)],
}

/// What the ambient sound asks of the sound manager and the data.
pub trait Speaker {
    /// Play a request; its handle, or 0 when it was refused.
    fn play(&mut self, req: &PlayRequest) -> u32;
    /// Stop at once every sound an emitter makes.
    fn stop_for_emitter(&mut self, emitter: u64);
    /// Stop the sounds a mask names ([`stop`]).
    fn stop_sounds(&mut self, mask: u32);
    /// Whether the sound of a handle is done (a handle of 0 is).
    fn has_finished(&self, handle: u32) -> bool;
    /// Change the frequency of the sound of a handle.
    fn set_frequency(&mut self, handle: u32, frequency: f32);
    /// Whether a sound or music setting plays placed in the world.
    fn placed(&self, id: u32) -> bool;
    /// The region's ambient sound table of record `id`.
    fn ambient_table(&self, id: u32) -> Option<SoundDesc>;
}

/// The sound manager and the data files, as the ambient sound speaks through them.
pub struct Manager<'a> {
    pub player: &'a mut SoundPlayer,
    pub files: &'a dyn SoundFiles,
}

impl std::fmt::Debug for Manager<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Manager")
            .field("player", &self.player)
            .finish_non_exhaustive()
    }
}

impl Speaker for Manager<'_> {
    fn play(&mut self, req: &PlayRequest) -> u32 {
        self.player.play(self.files, req).unwrap_or(0)
    }

    fn stop_for_emitter(&mut self, emitter: u64) {
        self.player.stop_for_emitter(emitter);
    }

    fn stop_sounds(&mut self, mask: u32) {
        self.player.stop_sounds(mask);
    }

    fn has_finished(&self, handle: u32) -> bool {
        self.player.has_finished(handle)
    }

    fn set_frequency(&mut self, handle: u32, frequency: f32) {
        self.player.set_frequency(handle, frequency);
    }

    fn placed(&self, id: u32) -> bool {
        match records::setting_type(id) {
            Some(SettingType::Sound) => {
                records::sound_info(self.files, id).is_some_and(|s| s.flags & SOUND_PLACED != 0)
            }
            Some(SettingType::Music) => {
                records::music_info(self.files, id).is_some_and(|m| m.flags & MUSIC_PLACED != 0)
            }
            None => false,
        }
    }

    fn ambient_table(&self, id: u32) -> Option<SoundDesc> {
        records::ambient_table(self.files, id)
    }
}

/// The ambient sound by place.
#[derive(Debug)]
pub struct Ambient {
    /// Where the player stood at the last recompute (none before the first).
    player_old: Option<Place>,
    player: Place,
    /// Where the camera was last drawn from.
    viewer: Place,
    enabled: bool,
    /// The ambient volume every sound plays at (always 1).
    volume: f32,
    /// The whole place's weight in the latest recompute.
    total: f32,
    /// How often quiet descriptions are let go of, and when next.
    check_every: f64,
    check_at: f64,
    /// Places farther than this from the player are not heard.
    max_distance: f32,
    /// How far the player moves before the places are weighed again.
    update_distance: f32,
    /// A place nearer the player than this is heard at the player.
    nearby: f32,
    /// The dominant description's number (`u32::MAX`: none).
    dominant: u32,
    descriptions: Vec<Description>,
    /// The region's ambient sound table (`.sod`; 0: none), and the table as last read.
    table_id: u32,
    table: Option<(u32, Arc<SoundDesc>)>,
    /// A landblock's side in meters.
    block_length: f32,
    next_emitter: u64,
}

impl Ambient {
    /// The ambient sound of a world whose landblocks are `block_length` meters a side: on, no
    /// table yet, nothing heard.
    // D2-03296
    #[must_use]
    pub fn new(block_length: f32) -> Self {
        Self {
            player_old: None,
            player: [0.0; 3],
            viewer: [0.0; 3],
            enabled: true,
            volume: 1.0,
            total: 0.0,
            check_every: 0.5,
            check_at: 0.0,
            max_distance: 100.0,
            update_distance: 4.0,
            nearby: 0.0002,
            dominant: u32::MAX,
            descriptions: Vec::new(),
            table_id: 0,
            table: None,
            block_length,
            next_emitter: EMITTERS,
        }
    }

    /// The descriptions being heard or kept, in the order they were met.
    #[must_use]
    pub fn descriptions(&self) -> &[Description] {
        &self.descriptions
    }

    /// The dominant description's kind, if one dominates.
    #[must_use]
    pub fn dominant_kind(&self) -> Option<u32> {
        let i = usize::try_from(self.dominant).ok()?;
        self.descriptions.get(i).map(|d| d.kind)
    }

    /// Whether the ambient sound is on.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The region was set: its ambient sound table is `table` (0: none), and the ambient sound is
    /// on.
    // D2-03292, D2-11006
    pub fn set_region(&mut self, table: u32) {
        self.table_id = table;
        self.enabled = true;
    }

    /// The camera was drawn from `at`: the sounds stand about it from the next turn.
    pub fn set_viewer(&mut self, at: Place) {
        self.viewer = at;
    }

    /// Where an ambient sound's object stands, by its emitter.
    // D2-14064, D2-03346, D2-03365, D2-03366
    #[must_use]
    pub fn emitter_place(&self, emitter: u64) -> Option<Place> {
        if !is_emitter(emitter) {
            return None;
        }
        self.descriptions.iter().find_map(|d| {
            d.continuous
                .iter()
                .flat_map(|c| c.objects.iter())
                .find(|o| o.emitter == emitter)
                .map(|o| o.place)
                .or_else(|| {
                    d.intermittent
                        .iter()
                        .find(|i| i.emitter == emitter)
                        .map(|i| i.place)
                })
        })
    }

    /// The world was left: every description goes (their sounds stop), nothing dominates, the
    /// places are forgotten, and the music and the sounds heard everywhere stop.
    // D2-03298, D2-03297
    pub fn clear(&mut self, speaker: &mut dyn Speaker) {
        for d in self.descriptions.drain(..) {
            d.destroy(speaker);
        }
        self.dominant = u32::MAX;
        self.player_old = None;
        self.player = [0.0; 3];
        self.viewer = [0.0; 3];
        speaker.stop_sounds(stop::FLAT_MUSIC | stop::PLACED_MUSIC | stop::FLAT_SOUNDS);
    }

    /// The frame's turn at `now`, while on: every half second the descriptions that fell quiet
    /// before now go (the last taking the place of each), then each description has its turn.
    // D2-03299
    pub fn use_time(&mut self, now: f64, speaker: &mut dyn Speaker, random: &Random) {
        if !self.enabled {
            return;
        }
        if self.check_at < now {
            self.check_at = now + self.check_every;
            let mut i = 0;
            while i < self.descriptions.len() {
                if self.descriptions[i].timed_out(now) {
                    let d = self.descriptions.swap_remove(i);
                    d.destroy(speaker);
                } else {
                    i += 1;
                }
            }
        }
        let (viewer, volume) = (self.viewer, self.volume);
        for d in &mut self.descriptions {
            d.use_time(now, viewer, volume, speaker, random);
        }
    }

    /// The player's place after the cells about it were updated: unless they are being waited
    /// for, and once the player has moved [`Self::update_distance`] since the last recompute,
    /// every place in view is weighed again and the dominant description picked.
    // D2-11005
    pub fn update(
        &mut self,
        around: &Surroundings<'_>,
        land: &mut dyn LandPoints,
        now: f64,
        speaker: &mut dyn Speaker,
        random: &Random,
    ) {
        if around.blocking || !self.do_sound_update(around.player) {
            return;
        }
        self.change_cell_start();
        self.add_cell_sounds(around, land, speaker);
        self.change_cell_end(now, speaker, random);
    }

    /// Whether the player at `at` has moved far enough to weigh the places again (then it is
    /// where the next move counts from); never while off.
    // D2-03303
    fn do_sound_update(&mut self, at: Place) -> bool {
        if !self.enabled {
            return false;
        }
        self.player = at;
        if let Some(old) = self.player_old {
            let d2: f32 = old.iter().zip(at).map(|(a, b)| (b - a) * (b - a)).sum();
            if d2 < self.update_distance * self.update_distance {
                return false;
            }
        }
        self.player_old = Some(at);
        true
    }

    /// A recompute starts: nothing weighed.
    // D2-03300
    fn change_cell_start(&mut self) {
        self.total = 0.0;
        for d in &mut self.descriptions {
            d.reset();
        }
    }

    /// Every interior cell in view, then (when the landscape is heard) every point of the
    /// landblocks heard, each a place of its kind; a place on the other side of the player's
    /// indoors or outdoors is muffled.
    // D2-10811, D2-10919
    fn add_cell_sounds(
        &mut self,
        around: &Surroundings<'_>,
        land: &mut dyn LandPoints,
        speaker: &mut dyn Speaker,
    ) {
        let outdoors = around.cell & 0xFFFF < 0x100;
        for (kind, c) in around.cells {
            self.add_cell_sound(*kind, *c, outdoors, speaker);
        }
        if !around.outside {
            return;
        }
        for &(bx, by) in around.blocks {
            let corner = [
                f32::from(bx) * self.block_length,
                f32::from(by) * self.block_length,
            ];
            for p in land.points((bx, by)).iter() {
                let at = [
                    corner[0] + p.origin[0],
                    corner[1] + p.origin[1],
                    p.origin[2],
                ];
                self.add_cell_sound(p.kind, at, !outdoors, speaker);
            }
        }
    }

    /// The description of `kind` (none when it was never met).
    // D2-03301
    fn find(&self, kind: u32) -> Option<usize> {
        self.descriptions.iter().position(|d| d.kind == kind)
    }

    /// One place of `kind` at `at`: past the reach, nothing; within the nearby distance it is
    /// heard at the player. Its kind's description weighs it, made from the region's table the
    /// first time the kind is met (a kind the table lacks is not heard; no table, or a table that
    /// cannot be read, turns the ambient sound off).
    // D2-03305, D2-07628, D2-07480
    fn add_cell_sound(
        &mut self,
        kind: u32,
        at: Place,
        muffled: bool,
        speaker: &mut dyn Speaker,
    ) -> bool {
        let mut offset = [
            at[0] - self.player[0],
            at[1] - self.player[1],
            at[2] - self.player[2],
        ];
        let d2: f32 = offset.iter().map(|v| v * v).sum();
        if self.max_distance * self.max_distance < d2 {
            return false;
        }
        if d2 < self.nearby * self.nearby {
            offset = [0.0; 3];
        }
        let i = match self.find(kind) {
            Some(i) => i,
            None => {
                let Some(table) = self.table(speaker) else {
                    self.enabled = false;
                    return false;
                };
                let Some(info) = table.sounds.iter().find(|s| s.kind == kind) else {
                    return false;
                };
                let d = Description::new(info, &mut self.next_emitter);
                self.descriptions.push(d);
                self.descriptions.len() - 1
            }
        };
        self.total += self.descriptions[i].add_sound(offset, muffled);
        true
    }

    /// The region's ambient table, read once.
    fn table(&mut self, speaker: &dyn Speaker) -> Option<Arc<SoundDesc>> {
        if self.table_id == 0 {
            return None;
        }
        if let Some((id, t)) = &self.table {
            if *id == self.table_id {
                return Some(Arc::clone(t));
            }
        }
        let t = Arc::new(speaker.ambient_table(self.table_id)?);
        self.table = Some((self.table_id, Arc::clone(&t)));
        Some(t)
    }

    /// A recompute ends: each description takes its share of the whole (heard or not, its sounds
    /// placed and played); the one with the greatest share (the first of equals) dominates. When
    /// that changes, the new one dominates (its environment handed on, which changes nothing)
    /// before the old one, by its number, no longer does.
    // D2-03302
    fn change_cell_end(&mut self, now: f64, speaker: &mut dyn Speaker, random: &Random) {
        let inverse = if self.total.abs() >= 0.0002 {
            1.0 / self.total
        } else {
            0.0
        };
        let mut best = u32::MAX;
        let mut best_share = 0.0f32;
        for (i, d) in self.descriptions.iter_mut().enumerate() {
            let share = d.calc_weights(now, inverse, speaker, random);
            if best_share < share {
                best = u32::try_from(i).unwrap_or(u32::MAX);
                best_share = share;
            }
        }
        if best == self.dominant {
            return;
        }
        if let Some(d) = usize::try_from(best)
            .ok()
            .and_then(|i| self.descriptions.get_mut(i))
        {
            d.set_dominant(speaker, random);
        }
        if let Some(d) = usize::try_from(self.dominant)
            .ok()
            .and_then(|i| self.descriptions.get_mut(i))
        {
            d.no_longer_dominant(speaker);
        }
        self.dominant = best;
    }
}
