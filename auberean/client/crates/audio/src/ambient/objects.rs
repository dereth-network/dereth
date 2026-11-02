//! The sounds of one ambient description, and how each gathers its places.
//!
//! Each sound keeps what the latest recompute gathered of the places of its description around the
//! player: its weight and density, and the same for each of eight directions (45 degrees each,
//! clockwise from north) with the nearest and farthest distance a place of it stood at.
//!
//! - **A continuous sound** ([`Continuous`]) plays one or four objects of its setting for as long
//!   as its description is heard; a sound that plays only while its description dominates waits
//!   for that. Its objects stand at the listener (or about it, when the setting is placed), at the
//!   description's density as their volume.
//! - **An intermittent sound** ([`Intermittent`]) is played now and then, at a random time within
//!   its range, by chance, from a random place in a direction picked by the directions' densities.

use auberean_assets::region::{ContinuousSound, IntermittentSound};
use auberean_primitives::random::Random;
use dereth_primitives::num::{math, to_i32, to_i32_f64};

use super::{Place, Speaker};
use crate::request::{Channel, Looping, PlayRequest, PlayType, Track};

/// The bits of an ambient sound's flags.
pub mod bits {
    /// It plays a sound setting.
    pub const SOUND: u32 = 0x1;
    /// It plays a music setting (this bit wins over [`SOUND`]).
    pub const MUSIC: u32 = 0x2;
    /// It plays only while its description dominates the place.
    pub const DOMINANT_ONLY: u32 = 0x4;
    /// A continuous sound plays on when its description falls quiet or stops dominating.
    pub const PLAYS_ON: u32 = 0x8;
    /// A continuous sound plays four objects about the listener (else one).
    pub const FOUR: u32 = 0x10;
}

/// Changes and distances below this count as none.
pub const SMALL: f32 = 0.0002;
/// A place within this many meters weighs fully; past it, by the square of this over the square of
/// its distance.
pub const FULL_WEIGHT: f32 = 12.0;
/// A place farther than this many meters (squared) weighs nothing for a sound.
pub const REACH_SQUARED: f32 = 10_000.0;

/// A place `distance` away at `heading` degrees clockwise from north and `pitch` degrees up.
// D2-13123
fn from_spherical(distance: f32, heading: f32, pitch: f32) -> Place {
    let (h, p) = (
        f64::from(heading) * f64::from(0.017_453_292_f32),
        f64::from(pitch) * f64::from(0.017_453_292_f32),
    );
    let (d, cp) = (f64::from(distance), math::cos(p));
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: stored as floats, as the client does
    let v = [
        (math::sin(h) * cp * d) as f32,
        (math::cos(h) * cp * d) as f32,
        (math::sin(p) * d) as f32,
    ];
    v
}

/// One of the eight directions: the weight and density of the places in it, and the nearest and
/// farthest of them (-1 for none yet).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Direction {
    pub weight: f32,
    pub density: f32,
    pub min_distance: f32,
    pub max_distance: f32,
}

impl Default for Direction {
    // D2-03317
    fn default() -> Self {
        Self {
            weight: 0.0,
            density: 0.0,
            min_distance: -1.0,
            max_distance: -1.0,
        }
    }
}

impl Direction {
    // D2-03318
    fn reset(&mut self) {
        *self = Self::default();
    }

    // D2-03319
    fn add_weight(&mut self, w: f32) {
        self.weight += w;
    }

    /// A place `d` away: the nearest and farthest follow it.
    // D2-03320
    #[allow(clippy::float_cmp)] // -1 is the mark of none, set exactly
    fn add_distance(&mut self, d: f32) {
        if d < self.min_distance || self.min_distance == -1.0 {
            self.min_distance = d;
        }
        if self.max_distance < d || self.max_distance == -1.0 {
            self.max_distance = d;
        }
    }

    // D2-03321
    fn calc_weights(&mut self, inverse: f32) {
        self.density = inverse * self.weight;
    }

    /// A distance `fraction` of the way from the nearest place to the farthest.
    // D2-03322
    fn choose_distance(&self, fraction: f32) -> f32 {
        (self.max_distance - self.min_distance) * fraction + self.min_distance
    }
}

/// What a sound gathered of its description's places in the latest recompute.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gathered {
    pub weight: f32,
    pub density: f32,
    pub directions: [Direction; 8],
}

impl Gathered {
    /// A recompute starts: nothing gathered.
    // D2-03337, D2-03340
    pub fn start(&mut self) {
        self.weight = 0.0;
        self.density = 0.0;
        for d in &mut self.directions {
            d.reset();
        }
    }

    /// A recompute ends: each density is its weight over the whole place's (`inverse` is one over
    /// that).
    // D2-03338
    pub fn end(&mut self, inverse: f32) {
        self.density = inverse * self.weight;
        for d in &mut self.directions {
            d.calc_weights(inverse);
        }
    }

    /// The direction a level offset points in: its compass heading's eighth (0 north, 2 east), 0
    /// for an offset too short to point.
    // D2-03342
    #[must_use]
    pub fn direction(offset: Place) -> usize {
        let (x, y) = (f64::from(offset[0]), f64::from(offset[1]));
        let len = (x * x + y * y).sqrt();
        if len < f64::from(SMALL) {
            return 0;
        }
        #[allow(clippy::cast_possible_truncation)] // LINT-OK: the client keeps it as a float
        let along = (x / len) as f32;
        #[allow(clippy::cast_possible_truncation)] // LINT-OK: radians, stored as a float
        let radians = (7.853_981_633_974_483 - math::atan2(y / len, f64::from(along))) as f32;
        let mut degrees = (radians * 57.295_78) % 360.0;
        if degrees < 0.0 {
            degrees += 360.0;
            if degrees >= 360.0 {
                degrees = 0.0;
            }
        }
        let eighth = to_i32_f64((f64::from(degrees) * f64::from(0.022_222_223_f32)).floor());
        // 0 to 8: a heading just short of 360 degrees rounds to the eighth eighth, kept in range.
        usize::try_from(eighth).unwrap_or(0).min(7)
    }

    /// One place of the description `offset` from the player (level, its height dropped):
    /// within 12 meters it weighs 1, past that 144 over its distance squared, halved when
    /// `muffled`; past 100 meters nothing. Its direction takes its weight and distance; a place
    /// at the player weighs 1 spread over all eight. Whether it counted, and its weight added to
    /// `sum`.
    // D2-03343
    pub fn add(&mut self, offset: Place, muffled: bool, sum: &mut f32) -> bool {
        let d2 = offset[0] * offset[0] + offset[1] * offset[1] + offset[2] * offset[2];
        if d2 > REACH_SQUARED {
            return false;
        }
        let near = FULL_WEIGHT * FULL_WEIGHT;
        let mut w = if d2 <= near {
            1.0
        } else if d2 <= SMALL {
            return false;
        } else {
            near / d2
        };
        if muffled {
            w *= 0.5;
        }
        if d2 < SMALL {
            w = 1.0;
            for d in &mut self.directions {
                d.add_weight(0.125);
                d.add_distance(0.0);
            }
        } else {
            let d = &mut self.directions[Self::direction(offset)];
            d.add_weight(w);
            d.add_distance(d2.sqrt());
        }
        self.weight += w;
        if w > 0.0 {
            *sum += w;
            return true;
        }
        false
    }

    /// Where an intermittent sound plays from: a direction picked at random by the directions'
    /// densities (the eighth when the first seven do not take the roll), a heading at random within
    /// it, and a distance at random between its nearest and farthest place; added to `place`.
    // D2-03339
    pub fn intermittent_place(&self, random: &Random, place: &mut Place) {
        let mut roll = random.roll_float(0.0, 1.0);
        let mut chosen = 7;
        for (i, d) in self.directions.iter().take(7).enumerate() {
            roll -= d.density;
            if roll < 0.0 {
                chosen = i;
                break;
            }
        }
        let turn = random.roll_float(0.0, 45.0);
        let fraction = random.roll_float(0.0, 1.0);
        let distance = self.directions[chosen].choose_distance(fraction);
        #[allow(clippy::cast_precision_loss)] // below eight
        let v = from_spherical(distance, chosen as f32 * 45.0 + turn, 0.0);
        for (p, o) in place.iter_mut().zip(v) {
            *p += o;
        }
    }
}

/// One object of a continuous sound: the setting it plays, where it stands (the listener plus its
/// offset), its volume's factor, and the sound it last played.
#[derive(Clone, Debug, PartialEq)]
pub struct ContinuousObject {
    /// The emitter the sound manager knows it by.
    pub emitter: u64,
    /// It plays a music setting (else a sound setting).
    pub music: bool,
    pub sound: u32,
    pub place: Place,
    pub offset: Place,
    pub factor: f32,
    pub handle: u32,
}

impl ContinuousObject {
    // D2-03345, D2-03352, D2-03357
    fn new(emitter: u64, music: bool, sound: u32) -> Self {
        Self {
            emitter,
            music,
            sound,
            place: [0.0; 3],
            offset: [0.0; 3],
            factor: 0.0,
            handle: 0,
        }
    }

    // D2-03348
    fn init(&mut self, offset: Place, factor: f32) {
        self.offset = offset;
        self.factor = factor;
    }

    // D2-03349
    fn should_play(&self) -> bool {
        self.factor >= SMALL
    }

    /// It stands at `at` plus its offset.
    // D2-03351
    fn update_pos(&mut self, at: Place) {
        for ((p, a), o) in self.place.iter_mut().zip(at).zip(self.offset) {
            *p = a + o;
        }
    }

    /// Play its setting at `volume` times its factor: a music setting on the music channel as the
    /// setting says, at its own pitch; a sound setting on the ambient channel at `pitch`, one at a
    /// time and counted against the budget.
    // D2-03359, D2-03353
    fn play(&mut self, speaker: &mut dyn Speaker, volume: f32, pitch: f32) {
        if !self.should_play() {
            return;
        }
        let mut req = PlayRequest::new(self.sound);
        req.volume = volume * self.factor;
        req.looping = Looping::Inherit;
        req.track = Track::Inherit;
        req.emitter = Some(self.emitter);
        if self.music {
            req.channel = Channel::Music;
        } else {
            req.frequency = pitch;
            req.kind = PlayType::SelfExclusiveBudgeted;
            req.channel = Channel::Ambient;
        }
        self.handle = speaker.play(&req);
    }

    /// Every sound it makes stops at once.
    // D2-03358, D2-03355
    fn stop(&self, speaker: &mut dyn Speaker) {
        speaker.stop_for_emitter(self.emitter);
    }

    /// Whether its setting plays placed in the world.
    // D2-03360, D2-03354
    fn placed(&self, speaker: &dyn Speaker) -> bool {
        speaker.placed(self.sound)
    }
}

/// A continuous sound of a description: its setting, its objects and what it gathered.
#[derive(Clone, Debug, PartialEq)]
pub struct Continuous {
    pub gathered: Gathered,
    pub objects: Vec<ContinuousObject>,
    pub info: ContinuousSound,
    /// The ambient volume it was last given.
    pub volume: f32,
}

impl Continuous {
    /// Its objects: four for [`bits::FOUR`], else one; each of the music setting for
    /// [`bits::MUSIC`], else of the sound setting for [`bits::SOUND`], else none. Each takes the
    /// next emitter from `next`.
    // D2-03334
    #[must_use]
    pub fn new(info: &ContinuousSound, next: &mut u64) -> Self {
        let count = if info.flags & bits::FOUR != 0 { 4 } else { 1 };
        let mut objects = Vec::new();
        for _ in 0..count {
            let music = if info.flags & bits::MUSIC != 0 {
                true
            } else if info.flags & bits::SOUND != 0 {
                false
            } else {
                continue;
            };
            objects.push(ContinuousObject::new(*next, music, info.sound.0));
            *next += 1;
        }
        Self {
            gathered: Gathered::default(),
            objects,
            info: *info,
            volume: 1.0,
        }
    }

    // D2-03323
    pub fn set_volume(&mut self, v: f32) {
        self.volume = v;
    }

    /// A random height between the setting's least and greatest.
    // D2-03324
    fn calc_height(&self, random: &Random) -> f32 {
        random.roll_float(self.info.min_height, self.info.max_height)
    }

    /// Its objects stand about `at`.
    // D2-03325
    pub fn update_pos(&mut self, at: Place) {
        for o in &mut self.objects {
            o.update_pos(at);
        }
    }

    /// Its objects' sounds stop, unless it plays on.
    // D2-03326
    pub fn stop_all(&self, speaker: &mut dyn Speaker) {
        if self.info.flags & bits::PLAYS_ON == 0 {
            for o in &self.objects {
                o.stop(speaker);
            }
        }
    }

    /// Each object plays at the setting's volume times the ambient volume, at a pitch rolled
    /// between the setting's least and greatest.
    // D2-03327
    pub fn play_sounds(&mut self, speaker: &mut dyn Speaker, random: &Random) {
        let volume = self.info.volume * self.volume;
        for o in &mut self.objects {
            let pitch = random.roll_float(self.info.min_pitch, self.info.max_pitch);
            o.play(speaker, volume, pitch);
        }
    }

    /// Direction `i` (taken round the eight) counted `fraction` into the sums of headings and
    /// weights.
    // D2-03328
    fn calc_angle_sum(&self, i: u32, fraction: f32, angle: &mut f32, weight: &mut f32) {
        let d = (i % 8) as usize;
        let w = self.gathered.directions[d].density;
        *weight += fraction * w;
        #[allow(clippy::cast_precision_loss)] // a direction's number
        let heading = (i as f32 + 0.5) * 45.0;
        *angle += heading * w * fraction;
    }

    // D2-03329
    pub fn now_dominant(&mut self, speaker: &mut dyn Speaker, random: &Random) {
        if self.info.flags & bits::DOMINANT_ONLY != 0 {
            self.play_sounds(speaker, random);
        }
    }

    // D2-03330
    pub fn no_longer_dominant(&self, speaker: &mut dyn Speaker) {
        if self.info.flags & bits::DOMINANT_ONLY != 0 {
            self.stop_all(speaker);
        }
    }

    /// Whether its first object's setting plays placed in the world.
    // D2-03331
    fn placed(&self, speaker: &dyn Speaker) -> bool {
        self.objects.first().is_some_and(|o| o.placed(speaker))
    }

    /// It goes: its sounds stop (unless it plays on), then every object's.
    // D2-03332, D2-03356, D2-03361, D2-03347
    pub fn destroy(&self, speaker: &mut dyn Speaker) {
        self.stop_all(speaker);
        for o in &self.objects {
            o.stop(speaker);
        }
    }

    /// The recompute ends for it (`inverse` is one over the place's whole weight): its density,
    /// then where its objects stand and how loud. A setting heard everywhere puts its one object
    /// at the listener at the density. A placed one with one object puts it at the sum of eight
    /// spokes of the density's length, times ten, which all but cancel (so at the listener too). A
    /// placed one with four puts each at ten meters on the heading its directions' densities
    /// average to, a random height up, at their weight times the density (none when they weigh
    /// nothing); every object takes the same directions, the first two and a half eighths' worth
    /// (eight over four, plus a half, of them) added again for each, starting from the place's
    /// inverse weight. Then it plays, unless it plays only while its description dominates and that
    /// is not so (`dominant`, as of before this recompute).
    // D2-03333
    pub fn recalc(
        &mut self,
        inverse: f32,
        dominant: bool,
        speaker: &mut dyn Speaker,
        random: &Random,
    ) {
        self.gathered.end(inverse);
        if self.objects.is_empty() {
            return;
        }
        let density = self.gathered.density;
        if !self.placed(speaker) {
            self.objects[0].init([0.0; 3], density);
        } else if self.info.flags & bits::FOUR == 0 {
            let mut sum = [0.0f32; 3];
            for i in 0..8u8 {
                let v = from_spherical(density, (f32::from(i) + 0.05) * 45.0, 0.0);
                for (s, x) in sum.iter_mut().zip(v) {
                    *s += x;
                }
            }
            self.objects[0].init([sum[0] * 10.0, sum[1] * 10.0, sum[2] * 10.0], density);
        } else {
            #[allow(clippy::cast_precision_loss)] // four objects
            let span = 8.0 / self.objects.len() as f32 + 0.5;
            // The heading sum starts from nothing (a value the client never sets); the weight sum
            // from the place's inverse weight, whose slot the client reuses.
            let (mut angle, mut weight) = (0.0f32, inverse);
            let end = u32::try_from(to_i32(span.floor())).unwrap_or(0);
            for k in 0..self.objects.len() {
                self.calc_angle_sum(0, 0.5, &mut angle, &mut weight);
                for i in 1..end {
                    self.calc_angle_sum(i, 1.0, &mut angle, &mut weight);
                }
                #[allow(clippy::cast_precision_loss)] // a direction's number
                self.calc_angle_sum(end, span - end as f32, &mut angle, &mut weight);
                if weight <= SMALL {
                    self.objects[k].init([0.0; 3], 0.0);
                } else {
                    let mut v = from_spherical(10.0, angle / weight, 0.0);
                    v[2] = self.calc_height(random);
                    self.objects[k].init(v, weight * density);
                }
            }
        }
        if dominant || self.info.flags & bits::DOMINANT_ONLY == 0 {
            self.play_sounds(speaker, random);
        }
    }
}

/// An intermittent sound of a description: its setting, when it may play next, where it last
/// played from, and the sound it last played.
#[derive(Clone, Debug, PartialEq)]
pub struct Intermittent {
    pub gathered: Gathered,
    /// When it may play next.
    pub next: f64,
    pub place: Place,
    /// It has played.
    pub played: bool,
    pub info: IntermittentSound,
    /// It plays a music setting (else a sound setting).
    pub music: bool,
    pub handle: u32,
    pub emitter: u64,
}

impl Intermittent {
    /// Of the music setting for [`bits::MUSIC`], else of the sound setting for [`bits::SOUND`],
    /// else none.
    // D2-03369, D2-03373, D2-03380
    #[must_use]
    pub fn new(info: &IntermittentSound, emitter: u64) -> Option<Self> {
        let music = if info.flags & bits::MUSIC != 0 {
            true
        } else if info.flags & bits::SOUND != 0 {
            false
        } else {
            return None;
        };
        Some(Self {
            gathered: Gathered::default(),
            next: 0.0,
            place: [0.0; 3],
            played: false,
            info: *info,
            music,
            handle: 0,
            emitter,
        })
    }

    /// A random height between the setting's least and greatest.
    // D2-03367
    fn calc_height(&self, random: &Random) -> f32 {
        random.roll_float(self.info.min_height, self.info.max_height)
    }

    /// Whether it plays now: not before its time; then its next time is a random wait within its
    /// range from now, and, when no sound of its description has played yet this turn (`count`)
    /// and it may play (its description dominates, or it does not wait for that), it plays by a
    /// roll within its probability times `density`. When it may not, its next time is pulled in to
    /// a random point between now and then.
    // D2-03371
    pub fn play_check(
        &mut self,
        now: f64,
        count: &mut u32,
        density: f32,
        dominant: bool,
        random: &Random,
    ) -> bool {
        if self.next >= now {
            return false;
        }
        self.next = f64::from(random.roll_float(self.info.min_time, self.info.max_time)) + now;
        if *count == 0 && (dominant || self.info.flags & bits::DOMINANT_ONLY == 0) {
            let roll = random.roll_float(0.0, 1.0);
            if roll <= self.info.probability * density {
                *count += 1;
                return true;
            }
            return false;
        }
        let wait = self.next - now;
        self.next = f64::from(random.roll_float(0.0, 1.0)) * wait + now;
        false
    }

    /// Whether its last sound plays on.
    // D2-03385, D2-03378
    fn is_playing(&self, speaker: &dyn Speaker) -> bool {
        if self.music && self.handle == 0 {
            return false;
        }
        !speaker.has_finished(self.handle)
    }

    /// Play it from about `at`, unless its last sound plays on: from a place in a direction
    /// picked by its densities, a random height up; at its volume times `volume`; at a pitch
    /// rolled between its least and greatest (a sound setting's only).
    // D2-03372
    pub fn play(&mut self, at: Place, volume: f32, speaker: &mut dyn Speaker, random: &Random) {
        if self.is_playing(speaker) {
            return;
        }
        self.place = at;
        self.gathered.intermittent_place(random, &mut self.place);
        self.place[2] += self.calc_height(random);
        self.played = true;
        let pitch = random.roll_float(self.info.min_pitch, self.info.max_pitch);
        self.play_internal(self.info.volume * volume, speaker);
        if (pitch - 1.0).abs() > SMALL {
            self.set_tempo(pitch, speaker);
        }
    }

    /// A music setting on the music channel as its setting says; a sound setting on the ambient
    /// channel, one at a time and counted against the budget.
    // D2-03382, D2-03375
    fn play_internal(&mut self, volume: f32, speaker: &mut dyn Speaker) {
        let mut req = PlayRequest::new(self.info.sound.0);
        req.volume = volume;
        req.emitter = Some(self.emitter);
        if self.music {
            req.channel = Channel::Music;
        } else {
            req.kind = PlayType::SelfExclusiveBudgeted;
            req.channel = Channel::Ambient;
        }
        self.handle = speaker.play(&req);
    }

    /// A sound setting's sound takes `pitch` as its frequency; music keeps its own.
    // D2-03379
    fn set_tempo(&self, pitch: f32, speaker: &mut dyn Speaker) {
        if !self.music {
            speaker.set_frequency(self.handle, pitch);
        }
    }

    /// It goes: its sounds stop (a music setting's only once it has played).
    // D2-03384, D2-03377, D2-03363, D2-03336, D2-03341
    pub fn destroy(&self, speaker: &mut dyn Speaker) {
        if !self.music || self.played {
            speaker.stop_for_emitter(self.emitter);
        }
    }

    /// Whether its setting plays placed in the world.
    // D2-03381, D2-03374
    #[must_use]
    pub fn placed(&self, speaker: &dyn Speaker) -> bool {
        speaker.placed(self.info.sound.0)
    }
}
