//! One kind of place's ambient sound, as the player hears it: the region's description of that
//! kind (its sound environment, and the sounds that play now and then and continuously), what
//! its places around the player weigh, and whether it is heard and dominates.

use auberean_assets::region::AmbientSound;
use auberean_primitives::random::Random;

use super::objects::{Continuous, Intermittent};
use super::{Place, Speaker};

/// A description's share of the whole place below which it falls quiet.
pub const MIN_SHARE: f32 = 0.01;

/// A kind of place's ambient sound.
#[derive(Clone, Debug, PartialEq)]
pub struct Description {
    /// The kind, as places name it.
    pub kind: u32,
    /// Its sound environment (`.snv`).
    pub environment: u32,
    pub intermittent: Vec<Intermittent>,
    pub continuous: Vec<Continuous>,
    /// When it fell quiet.
    pub quiet_since: f64,
    /// It is heard (its share was at least [`MIN_SHARE`] in the latest recompute).
    pub active: bool,
    /// What its places weigh, and its share of the whole place.
    pub weight: f32,
    pub share: f32,
    /// It dominates the place.
    pub dominant: bool,
}

impl Description {
    /// The description of `info`'s kind: its intermittent sounds (those of a sound or music
    /// setting), then its continuous ones; each object takes the next emitter from `next`.
    // D2-03316
    #[must_use]
    pub fn new(info: &AmbientSound, next: &mut u64) -> Self {
        let mut intermittent = Vec::new();
        for s in &info.intermittent {
            // D2-03315
            if let Some(i) = Intermittent::new(s, *next) {
                intermittent.push(i);
                *next += 1;
            }
        }
        let continuous = info
            .continuous
            .iter()
            .map(|s| Continuous::new(s, next))
            .collect();
        Self {
            kind: info.kind,
            environment: info.environment.0,
            intermittent,
            continuous,
            quiet_since: 0.0,
            active: false,
            weight: 0.0,
            share: 0.0,
            dominant: false,
        }
    }

    /// Whether it is quiet and has been since before `now`, and may go.
    // D2-03306
    #[must_use]
    pub fn timed_out(&self, now: f64) -> bool {
        !self.active && self.quiet_since <= now
    }

    /// It goes: every sound of it stops (a continuous one that plays on, only as its objects go).
    // D2-03307, D2-03313
    pub fn destroy(&self, speaker: &mut dyn Speaker) {
        for i in &self.intermittent {
            i.destroy(speaker);
        }
        for c in &self.continuous {
            c.destroy(speaker);
        }
    }

    /// Its turn, while heard: each intermittent sound checks whether it plays (one at most a
    /// turn) and plays from about `viewer` at `volume`; each continuous sound takes `volume` and
    /// stands about `viewer`.
    // D2-03308
    pub fn use_time(
        &mut self,
        now: f64,
        viewer: Place,
        volume: f32,
        speaker: &mut dyn Speaker,
        random: &Random,
    ) {
        if !self.active {
            return;
        }
        let mut count = 0;
        for i in &mut self.intermittent {
            if i.play_check(now, &mut count, self.share, self.dominant, random) {
                i.play(viewer, volume, speaker, random);
            }
        }
        for c in &mut self.continuous {
            c.set_volume(volume);
            c.update_pos(viewer);
        }
    }

    /// A recompute starts: nothing weighed.
    // D2-03309
    pub fn reset(&mut self) {
        self.weight = 0.0;
        for i in &mut self.intermittent {
            i.gathered.start();
        }
        for c in &mut self.continuous {
            c.gathered.start();
        }
    }

    /// One of its places, `offset` from the player (its height dropped), muffled or not: each
    /// sound gathers it, and the place's weight (the mean of the sounds that counted it) is
    /// added to the description's. What the place adds to the whole: that weight, or, when no
    /// sound counted it, 1 within 12 meters and 144 over its distance squared beyond.
    // D2-03310
    #[allow(clippy::float_cmp)] // the client tests the sum against 0 exactly
    pub fn add_sound(&mut self, offset: Place, muffled: bool) -> f32 {
        let level = [offset[0], offset[1], 0.0];
        let mut sum = 0.0f32;
        let mut counted = 0u32;
        for i in &mut self.intermittent {
            if i.gathered.add(level, muffled, &mut sum) {
                counted += 1;
            }
        }
        for c in &mut self.continuous {
            if c.gathered.add(level, muffled, &mut sum) {
                counted += 1;
            }
        }
        if counted > 1 {
            #[allow(clippy::cast_precision_loss)] // a handful of sounds
            let n = counted as f32;
            sum /= n;
        }
        self.weight += sum;
        if sum == 0.0 {
            let d2 = level[0] * level[0] + level[1] * level[1];
            let near = super::objects::FULL_WEIGHT * super::objects::FULL_WEIGHT;
            return if d2 <= near { 1.0 } else { near / d2 };
        }
        sum
    }

    /// It is now the place's dominant description: each continuous sound that waits for that
    /// plays.
    // D2-03311
    pub fn set_dominant(&mut self, speaker: &mut dyn Speaker, random: &Random) {
        self.dominant = true;
        for c in &mut self.continuous {
            c.now_dominant(speaker, random);
        }
    }

    /// It no longer dominates: each continuous sound that waits for that stops, unless it plays
    /// on.
    // D2-03312
    pub fn no_longer_dominant(&mut self, speaker: &mut dyn Speaker) {
        self.dominant = false;
        for c in &self.continuous {
            c.no_longer_dominant(speaker);
        }
    }

    /// The recompute ends for it (`inverse` is one over the whole place's weight): its share is
    /// its weight times that. Below [`MIN_SHARE`] its continuous sounds stop (unless they play on)
    /// and, if it was heard, it falls quiet as of `now`; its share counts as 0. Otherwise it is
    /// heard, and each sound takes its densities (a continuous one placing its objects and
    /// playing). Its share.
    // D2-03314
    pub fn calc_weights(
        &mut self,
        now: f64,
        inverse: f32,
        speaker: &mut dyn Speaker,
        random: &Random,
    ) -> f32 {
        self.share = inverse * self.weight;
        if self.share < MIN_SHARE {
            for c in &self.continuous {
                c.stop_all(speaker);
            }
            if self.active {
                self.quiet_since = now;
                self.active = false;
            }
            return 0.0;
        }
        self.active = true;
        for i in &mut self.intermittent {
            i.gathered.end(inverse);
        }
        let dominant = self.dominant;
        for c in &mut self.continuous {
            c.recalc(inverse, dominant, speaker, random);
        }
        self.share
    }
}
