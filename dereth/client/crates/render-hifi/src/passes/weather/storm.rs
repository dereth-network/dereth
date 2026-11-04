//! A storm: a rainy day this client alone draws wilder. The server's day is a rainy one; the
//! client rolls one rainy day in eight into a storm, or the player asks for one with `@weather
//! storm`.
//!
//! How far the storm has come in is eased by the scene, frame to frame, into
//! [`crate::snapshot::HifiSky::storm`]; everything else about it is worked out from that and the
//! scene's clock alone, so every pass that draws a part of it draws the same storm:
//!
//! - **The gale.** The breeze rises to a gale that gusts, heading much as the breeze does.
//! - **Lightning.** Now and then a stroke: a bolt from the cloud to the ground some way off, or a
//!   flash within the cloud with no bolt, each flickering two or three times as it fades.
//! - **Thunder.** Each stroke's peal, heard as long after it as sound takes to come that far,
//!   the louder the nearer ([`Thunder`]).

use dereth_primitives::num::math::{cosf, expf, sin, sinf};
use glam::Vec3;

use crate::snapshot::HifiFrame;

/// How long a storm takes to come in or die away, seconds.
pub const EASE_SECONDS: f32 = 8.0;
/// The gale's mean speed, and how far its gusts take it either way, metres a second.
pub const GALE: f32 = 8.0;
pub const GUST: f32 = 4.0;
/// How fast the rain falls in a storm, metres a second.
pub const STORM_RAIN_SPEED: f32 = 11.0;
/// The stretch of the clock each of which holds at most one stroke, seconds.
const SLOT: f64 = 5.0;
/// The chance a slot holds a stroke in a full storm.
const STROKE_CHANCE: f32 = 0.55;
/// The share of strokes that strike the ground with a bolt; the rest flash within the cloud.
const BOLT_SHARE: f32 = 0.55;
/// How long a stroke lights the sky, seconds.
const STROKE_SECONDS: f32 = 0.9;
/// How near and how far a stroke falls from the eye, metres.
const STROKE_REACH: (f32, f32) = (700.0, 3500.0);
/// How high over the eye a bolt leaves the cloud, metres.
pub const BOLT_TOP: f32 = 900.0;
/// The most points a bolt is drawn with, its branches' included.
pub const BOLT_POINTS: usize = 40;
/// How fast thunder comes, metres a second.
pub const SOUND_SPEED: f32 = 343.0;
/// How many peals of thunder there are to choose from.
pub const PEALS: u32 = 6;

/// How far a storm has come in, eased toward whether it storms.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Ease {
    strength: f32,
    last: Option<f64>,
}

impl Ease {
    /// Step to the scene's `now` toward a storm (`storms`) or none, and return how far it has
    /// come in, 0 to 1. The first step takes the weather as it stands; a clock that went back (a
    /// new scene) starts again.
    pub fn step(&mut self, storms: bool, now: f64) -> f32 {
        let want = if storms { 1.0 } else { 0.0 };
        match self.last {
            Some(last) if now >= last => {
                #[allow(clippy::cast_possible_truncation)] // a frame's step
                let by = ((now - last) as f32 / EASE_SECONDS).max(0.0);
                self.strength += (want - self.strength).clamp(-by, by);
            }
            _ => self.strength = want,
        }
        self.last = Some(now);
        self.strength
    }
}

/// How hard the gale gusts at the scene's `now`, -1 to 1: three slow swells of their own periods.
#[must_use]
pub fn gust(now: f64) -> f32 {
    let g = 0.5 * sin(now * 0.23) + 0.3 * sin(now * 0.57 + 1.3) + 0.2 * sin(now * 1.31 + 0.4);
    #[allow(clippy::cast_possible_truncation)] // -1..1
    let g = g as f32;
    g
}

/// The wind at `time_of_day` (0 to 1) and the scene's `now`, in a storm `strength` come in:
/// the day's breeze rising to a gusting gale on much its heading, metres a second east and north.
#[must_use]
pub fn wind(time_of_day: f32, strength: f32, now: f64) -> [f32; 2] {
    let breeze = super::wind(time_of_day);
    if strength <= 0.0 {
        return breeze;
    }
    let along = glam::Vec2::from(breeze).normalize_or(glam::Vec2::X);
    let gale = along * (GALE + GUST * gust(now));
    let w = glam::Vec2::from(breeze).lerp(gale, strength);
    [w.x, w.y]
}

/// The frame's wind, its storm included.
#[must_use]
pub fn frame_wind(frame: &HifiFrame) -> [f32; 2] {
    wind(frame.sky.time_of_day, frame.sky.storm, frame.time)
}

/// One stroke of lightning, as it stands at a moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    /// Seconds since it struck.
    pub age: f32,
    /// How brightly it lights the world now, 0 to about 1.
    pub flash: f32,
    /// Where it falls, metres east and north of the eye.
    pub offset: glam::Vec2,
    /// Whether it strikes the ground with a bolt, or flashes within the cloud.
    pub bolt: bool,
    /// Its own number, which shapes its bolt.
    pub seed: u32,
}

fn mix(v: u64) -> u32 {
    let mut x = v.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    #[allow(clippy::cast_possible_truncation)] // the high word
    let h = ((x ^ (x >> 31)) >> 32) as u32;
    h
}

fn unit(h: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // 24 bits
    let u = (h >> 8) as f32 / 16_777_216.0;
    u
}

/// How a stroke `age` seconds old lights the world: a first stroke and its return strokes, each
/// a sharp rise and a quick fall, the later ones the fainter; `seed` places them.
#[must_use]
pub fn flicker(age: f32, seed: u32) -> f32 {
    if !(0.0..STROKE_SECONDS).contains(&age) {
        return 0.0;
    }
    let second = 0.07 + 0.08 * unit(mix(u64::from(seed) ^ 0x51));
    let third = 0.22 + 0.2 * unit(mix(u64::from(seed) ^ 0x52));
    const RISE: f32 = 0.008;
    // Each pulse rises and falls, scaled so it tops out at its peak.
    let pulse = |at: f32, peak: f32, fall: f32| {
        let t = age - at;
        if t < 0.0 {
            return 0.0;
        }
        let top = RISE * dereth_primitives::num::math::logf(1.0 + fall / RISE);
        let shape = |t: f32| (1.0 - expf(-t / RISE)) * expf(-t / fall);
        peak * shape(t) / shape(top)
    };
    let f = pulse(0.0, 1.0, 0.06) + pulse(second, 0.7, 0.05) + pulse(third, 0.5, 0.12);
    // The last of it dies to nothing by the stroke's end.
    f.min(1.2) * (1.0 - age / STROKE_SECONDS).max(0.0).sqrt()
}

/// The stroke lighting the sky at the scene's `now` in a storm `strength` come in, if any: the
/// clock is cut into slots, each of which holds a stroke by a chance that grows with the storm,
/// at a moment, a place and a shape of its own.
#[must_use]
pub fn stroke(now: f64, strength: f32) -> Option<Stroke> {
    if strength <= 0.0 || now < 0.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a slot's number
    let slot = (now / SLOT).floor() as u64;
    let h = mix(slot);
    if unit(h) >= STROKE_CHANCE * strength {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)] // within the slot
    let lead = SLOT as f32 - STROKE_SECONDS;
    let at = unit(mix(slot ^ 0xa1)) * lead;
    #[allow(clippy::cast_possible_truncation)] // within the slot
    let age = (now - slot as f64 * SLOT) as f32 - at;
    let seed = mix(slot ^ 0xb2);
    let flash = flicker(age, seed);
    if flash <= 0.0 {
        return None;
    }
    let heading = unit(mix(slot ^ 0xc3)) * std::f32::consts::TAU;
    let reach = STROKE_REACH.0 + (STROKE_REACH.1 - STROKE_REACH.0) * unit(mix(slot ^ 0xd4));
    let bolt = unit(mix(slot ^ 0xe5)) < BOLT_SHARE;
    // A flash within the cloud lights it less than a bolt beneath it.
    let flash = flash * strength * if bolt { 1.0 } else { 0.6 };
    Some(Stroke {
        age,
        flash,
        offset: glam::Vec2::new(cosf(heading), sinf(heading)) * reach,
        bolt,
        seed,
    })
}

/// A peal of thunder due to be heard.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Peal {
    /// Which of the [`PEALS`] it is.
    pub which: u32,
    /// How loud, 0 to 1: a near bolt's whole, a far one's or a flash within the cloud's less.
    pub volume: f32,
}

/// The thunder of the strokes seen: each new stroke's peal, waiting to be heard.
#[derive(Debug, Clone, Default)]
pub struct Thunder {
    /// The last stroke whose peal was queued.
    heard: Option<u32>,
    /// The peals on their way, with the scene's clock they arrive at.
    coming: Vec<(f64, Peal)>,
}

impl Thunder {
    /// Step to the scene's `now` in a storm `strength` come in: queue the peal of a stroke that
    /// has just struck, and return those that arrive now.
    pub fn step(&mut self, now: f64, strength: f32) -> Vec<Peal> {
        if let Some(s) = stroke(now, strength).filter(|s| self.heard != Some(s.seed)) {
            self.heard = Some(s.seed);
            let far = s.offset.length();
            let near = 1.0 - (far - STROKE_REACH.0) / (STROKE_REACH.1 - STROKE_REACH.0);
            let volume = (0.35 + 0.65 * near.clamp(0.0, 1.0)) * if s.bolt { 1.0 } else { 0.6 };
            let peal = Peal {
                which: s.seed % PEALS,
                volume: volume * strength,
            };
            let arrives = now - f64::from(s.age) + f64::from(far / SOUND_SPEED);
            self.coming.push((arrives, peal));
        }
        let (due, later): (Vec<_>, Vec<_>) = self.coming.drain(..).partition(|(at, _)| *at <= now);
        self.coming = later;
        // A clock that went back (a new scene) leaves nothing owed from the old one.
        if self.coming.iter().any(|(at, _)| *at > now + 60.0) {
            self.coming.clear();
        }
        due.into_iter().map(|(_, p)| p).collect()
    }
}

/// The frame's stroke, if any.
#[must_use]
pub fn frame_stroke(frame: &HifiFrame) -> Option<Stroke> {
    asked_stroke(frame.sky.storm).or_else(|| stroke(frame.time, frame.sky.storm))
}

/// A stroke a capture holds in place, in a build for captures: `DERETH_HIFI_STROKE` as `age,
/// heading in degrees, reach in metres, bolt (1) or cloud (0)[, seed]`.
#[cfg(feature = "capture")]
fn asked_stroke(strength: f32) -> Option<Stroke> {
    static ASKED: std::sync::OnceLock<Option<[f32; 5]>> = std::sync::OnceLock::new();
    let a = (*ASKED.get_or_init(|| {
        let v: Vec<f32> = std::env::var("DERETH_HIFI_STROKE")
            .ok()?
            .split(',')
            .filter_map(|w| w.trim().parse().ok())
            .collect();
        (v.len() >= 4).then(|| [v[0], v[1], v[2], v[3], v.get(4).copied().unwrap_or(7.0)])
    }))?;
    if strength <= 0.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a small seed
    let seed = a[4] as u32;
    let heading = a[1].to_radians();
    Some(Stroke {
        age: a[0],
        flash: flicker(a[0], seed) * strength,
        offset: glam::Vec2::new(cosf(heading), sinf(heading)) * a[2],
        bolt: a[3] > 0.5,
        seed,
    })
}

#[cfg(not(feature = "capture"))]
fn asked_stroke(_strength: f32) -> Option<Stroke> {
    None
}

/// A bolt's path, render space, from the cloud `top` metres over the eye at `eye` down past the
/// ground: each point with whether a line runs to it from the point before (`false` starts a
/// branch). A jagged main channel and a branch or two off it, its own for `seed`.
#[must_use]
pub fn bolt_path(stroke: &Stroke, eye: Vec3, top: f32) -> Vec<(Vec3, bool)> {
    let seed = u64::from(stroke.seed);
    let mut k = 0u64;
    let mut rand = || {
        k += 1;
        unit(mix(seed.wrapping_mul(31).wrapping_add(k))) * 2.0 - 1.0
    };
    let foot = eye + stroke.offset.extend(-60.0);
    let head = foot + Vec3::new(rand() * 250.0, rand() * 250.0, top + 60.0);
    const MAIN: usize = 26;
    let mut points = Vec::with_capacity(BOLT_POINTS);
    let mut main = Vec::with_capacity(MAIN);
    let span = head.z - foot.z;
    #[allow(clippy::cast_precision_loss)] // a few points
    for i in 0..MAIN {
        let s = i as f32 / (MAIN - 1) as f32;
        let mut p = head.lerp(foot, s);
        // Jagged, but pinned at both ends.
        let jag = (s * (1.0 - s) * 4.0).sqrt() * span * 0.07;
        p += Vec3::new(rand() * jag, rand() * jag, rand() * span * 0.01);
        main.push(p);
        points.push((p, i > 0));
    }
    // A branch or two off the upper half of the channel, reaching down and out.
    let branches = if rand() > 0.0 { 2 } else { 1 };
    for _ in 0..branches {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a point of the main
        let from = (3.0 + (rand() * 0.5 + 0.5) * 9.0) as usize;
        let mut p = main[from.min(MAIN - 2)];
        let out = Vec3::new(rand(), rand(), 0.0).normalize_or(Vec3::X);
        points.push((p, false));
        let n = (BOLT_POINTS - MAIN) / branches - 1;
        for _ in 0..n {
            p += out * span * 0.035 + Vec3::new(rand(), rand(), -0.6 + rand() * 0.3) * span * 0.03;
            points.push((p, true));
        }
    }
    points.truncate(BOLT_POINTS);
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_storm_comes_in_and_dies_away_over_its_ease() {
        let mut e = Ease::default();
        assert_eq!(
            e.step(false, 10.0),
            0.0,
            "the first step takes the day as it stands"
        );
        let half = e.step(true, 10.0 + f64::from(EASE_SECONDS) * 0.5);
        assert!((half - 0.5).abs() < 1e-4, "{half}");
        assert_eq!(e.step(true, 100.0), 1.0);
        assert_eq!(e.step(true, 1.0), 1.0, "a new scene takes it at once");
    }

    #[test]
    fn the_gale_blows_far_harder_than_the_breeze_and_gusts() {
        let calm = wind(0.3, 0.0, 50.0);
        assert_eq!(calm, super::super::wind(0.3));
        let speeds: Vec<f32> = (0..400)
            .map(|k| glam::Vec2::from(wind(0.3, 1.0, f64::from(k) * 0.5)).length())
            .collect();
        let (lo, hi) = speeds
            .iter()
            .fold((f32::MAX, 0.0f32), |(lo, hi), s| (lo.min(*s), hi.max(*s)));
        assert!(
            lo >= GALE - GUST - 0.01 && hi <= GALE + GUST + 0.01,
            "{lo}..{hi}"
        );
        assert!(hi - lo > GUST, "it gusts: {lo}..{hi}");
    }

    #[test]
    fn lightning_strikes_now_and_then_in_a_full_storm_and_never_in_none() {
        let step = 1.0 / 60.0;
        let frames = (0..60 * 600).map(|k| f64::from(k) * step);
        let lit: Vec<Option<Stroke>> = frames.clone().map(|t| stroke(t, 1.0)).collect();
        // Strokes begin where a lit frame follows a dark one.
        let begun = lit
            .windows(2)
            .filter(|w| w[0].is_none() && w[1].is_some())
            .count();
        assert!((40..90).contains(&begun), "{begun} flashes in ten minutes");
        assert!(lit.iter().flatten().any(|s| s.bolt) && lit.iter().flatten().any(|s| !s.bolt));
        assert!(frames.map(|t| stroke(t, 0.0)).all(|s| s.is_none()));
    }

    #[test]
    fn each_stroke_peals_once_as_long_after_it_as_sound_takes_to_come() {
        let mut thunder = Thunder::default();
        let step = 1.0 / 60.0;
        let mut flashes = Vec::new();
        let mut peals = Vec::new();
        let mut lit = false;
        for k in 0..60 * 600 {
            let now = f64::from(k) * step;
            let s = stroke(now, 1.0);
            if let (Some(s), false) = (s, lit) {
                flashes.push((now, s.offset.length()));
            }
            lit = s.is_some();
            for p in thunder.step(now, 1.0) {
                assert!(
                    p.which < PEALS && p.volume > 0.2 && p.volume <= 1.0,
                    "{p:?}"
                );
                peals.push(now);
            }
        }
        // A near stroke's peal may overtake a far one's, so they are matched in the order heard.
        let mut owed: Vec<f64> = flashes
            .iter()
            .map(|(at, far)| at + f64::from(far / SOUND_SPEED))
            .filter(|at| *at < 600.0 - step)
            .collect();
        owed.sort_by(f64::total_cmp);
        assert_eq!(owed.len(), peals.len());
        for (want, heard) in owed.iter().zip(&peals) {
            assert!(
                (heard - want).abs() < 0.05,
                "heard at {heard}, owed at {want}"
            );
        }
        assert!(Thunder::default().step(10.0, 0.0).is_empty());
    }

    #[test]
    fn a_stroke_flickers_and_fades_within_its_time() {
        assert_eq!(flicker(-0.01, 7), 0.0);
        assert_eq!(flicker(STROKE_SECONDS, 7), 0.0);
        let peak = (0..90)
            .map(|k| flicker(k as f32 * 0.01, 7))
            .fold(0.0f32, f32::max);
        assert!(peak > 0.7 && peak <= 1.2, "{peak}");
    }

    #[test]
    fn a_bolt_runs_from_the_cloud_to_the_ground_within_its_points() {
        let s = Stroke {
            age: 0.0,
            flash: 1.0,
            offset: glam::Vec2::new(1500.0, -400.0),
            bolt: true,
            seed: 99,
        };
        let eye = Vec3::new(10.0, 20.0, 50.0);
        let path = bolt_path(&s, eye, BOLT_TOP);
        assert!(path.len() <= BOLT_POINTS && path.len() > 26);
        assert!(!path[0].1, "the channel starts a line");
        assert!(path[0].0.z > eye.z + BOLT_TOP * 0.9);
        assert!(path[25].0.z < eye.z, "it reaches the ground");
        assert_eq!(
            path,
            bolt_path(&s, eye, BOLT_TOP),
            "the same bolt every frame"
        );
    }
}
