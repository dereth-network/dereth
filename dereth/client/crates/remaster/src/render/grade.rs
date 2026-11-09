//! The color grade: display values through a look-up table, one table for each kind of day and
//! hour, after the tone curve.
//!
//! Each table is a cube of 32 steps a side laid out as a strip, 32 slices of blue side by side,
//! each 32 red by 32 green; the strips of every table stand one above the other in one picture.
//! A frame blends up to three of them: night, the low sun of dawn or dusk, and the day's own (a
//! clear noon, an overcast day, a rainy one).
//!
//! The tables are made from a few numbers each ([`Grade`]): lift, gamma and gain per channel, the
//! saturation, extra saturation and a turn of hue for the yellows and greens, and a contrast.
//! The first table is the identity, so a frame graded by it alone is the frame as drawn.

use super::sky::{day_weight, twilight, Weather};
use crate::scene::FrameDelta;

/// The steps of a table's side.
pub const SIDE: u32 = 32;

/// A table's numbers, every one in display values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grade {
    /// Added to the shade and fading out toward white.
    pub lift: [f32; 3],
    /// Over 1 brightens the middle tones.
    pub gamma: [f32; 3],
    /// A factor on each channel.
    pub gain: [f32; 3],
    /// A factor on every color's saturation.
    pub saturation: f32,
    /// More saturation for the yellows and greens, as a share of it.
    pub greens: f32,
    /// How far the yellows and greens turn toward a fresh yellow-green, degrees.
    pub green_turn: f32,
    /// The contrast about middle gray, easing out toward black.
    pub contrast: f32,
}

impl Grade {
    /// The grade that changes nothing.
    pub const IDENTITY: Self = Self {
        lift: [0.0; 3],
        gamma: [1.0; 3],
        gain: [1.0; 3],
        saturation: 1.0,
        greens: 0.0,
        green_turn: 0.0,
        contrast: 1.0,
    };

    /// A clear or sunny day's middle hours: warm sun, shade lifted toward the sky's blue, fresh
    /// greens.
    pub const NOON: Self = Self {
        lift: [0.022, 0.028, 0.045],
        gamma: [1.05, 1.05, 1.0],
        gain: [1.04, 1.025, 0.96],
        saturation: 1.12,
        greens: 0.28,
        green_turn: 9.0,
        contrast: 1.05,
    };

    /// The low sun of a clear morning.
    pub const MORNING: Self = Self {
        lift: [0.026, 0.028, 0.05],
        gamma: [1.04, 1.03, 1.0],
        gain: [1.06, 1.02, 0.94],
        saturation: 1.08,
        greens: 0.2,
        green_turn: 6.0,
        contrast: 1.03,
    };

    /// The low sun of a clear evening: warmer than the morning.
    pub const EVENING: Self = Self {
        lift: [0.03, 0.024, 0.05],
        gamma: [1.03, 1.02, 1.0],
        gain: [1.09, 1.0, 0.9],
        saturation: 1.1,
        greens: 0.12,
        green_turn: 4.0,
        contrast: 1.04,
    };

    /// Night: a little cooler and grayer, the shade kept dark.
    pub const NIGHT: Self = Self {
        lift: [0.006, 0.01, 0.024],
        gamma: [1.0, 1.0, 1.02],
        gain: [0.97, 0.99, 1.05],
        saturation: 0.94,
        greens: 0.0,
        green_turn: 0.0,
        contrast: 1.02,
    };

    /// A rainy day: cool, the greens kept.
    pub const RAIN: Self = Self {
        lift: [0.018, 0.024, 0.034],
        gamma: [1.02, 1.02, 1.0],
        gain: [0.99, 1.0, 1.01],
        saturation: 1.04,
        greens: 0.14,
        green_turn: 4.0,
        contrast: 1.04,
    };

    /// An overcast day: soft and a little brighter in the middle.
    pub const OVERCAST: Self = Self {
        lift: [0.024, 0.028, 0.04],
        gamma: [1.04, 1.04, 1.0],
        gain: [1.01, 1.0, 0.98],
        saturation: 1.07,
        greens: 0.2,
        green_turn: 6.0,
        contrast: 1.04,
    };

    /// Color `c` (display values, 0 to 1) through this grade.
    #[must_use]
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let mut o = [0.0f32; 3];
        // The gain fades out over the brightest colors, so the clouds and a pale sky keep their
        // own white and blue.
        let bright = smoothstep(0.5, 0.9, c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722);
        for k in 0..3 {
            let gain = self.gain[k] + (1.0 - self.gain[k]) * bright;
            let v = c[k] * gain + self.lift[k] * (1.0 - c[k]);
            o[k] = dereth_primitives::num::math::powf(v.max(0.0), 1.0 / self.gamma[k].max(1e-3));
        }
        let (h, s, v) = hsv(o);
        // How much of an olive, a yellow or a green the hue is: from orange to a blue-green.
        let green = bump(h, 25.0, 55.0, 150.0);
        let turned = h + self.green_turn * green;
        let s = (s * self.saturation * (1.0 + self.greens * green)).clamp(0.0, 1.0);
        o = rgb(turned, s, v);
        for x in &mut o {
            let ease = smoothstep(0.0, 0.25, *x);
            *x = *x + ((*x - 0.5) * self.contrast + 0.5 - *x) * ease;
            *x = x.clamp(0.0, 1.0);
        }
        o
    }
}

/// The tables a look changes, as `looks` lay them over the remaster's own: in the picture's order,
/// the identity first and never changed.
#[must_use]
pub fn tables_with(looks: &crate::look::LookStack) -> [Grade; 7] {
    let mut out = TABLES;
    for (k, name) in crate::look::GRADE_TABLES.iter().enumerate() {
        let p = looks.grade(name);
        let g = &mut out[k + 1];
        g.lift = p.lift.unwrap_or(g.lift);
        g.gamma = p.gamma.unwrap_or(g.gamma);
        g.gain = p.gain.unwrap_or(g.gain);
        g.saturation = p.saturation.unwrap_or(g.saturation);
        g.greens = p.greens.unwrap_or(g.greens);
        g.green_turn = p.green_turn.unwrap_or(g.green_turn);
        g.contrast = p.contrast.unwrap_or(g.contrast);
    }
    out
}

/// The tables the picture holds, in its order.
pub const TABLES: [Grade; 7] = [
    Grade::IDENTITY,
    Grade::NOON,
    Grade::MORNING,
    Grade::EVENING,
    Grade::NIGHT,
    Grade::RAIN,
    Grade::OVERCAST,
];

const NOON: u32 = 1;
const MORNING: u32 = 2;
const EVENING: u32 = 3;
const NIGHT: u32 = 4;
const RAIN: u32 = 5;
const OVERCAST: u32 = 6;

/// The picture of every table: `SIDE * SIDE` texels wide, `SIDE` tall for each, RGBA8.
#[must_use]
pub fn picture(tables: &[Grade]) -> Vec<u8> {
    let side = SIDE as usize;
    let width = side * side;
    let mut out = vec![0u8; width * side * tables.len() * 4];
    #[allow(clippy::cast_precision_loss)] // steps of a small cube
    let step = |i: usize| i as f32 / (side - 1) as f32;
    for (t, g) in tables.iter().enumerate() {
        for green in 0..side {
            for blue in 0..side {
                for red in 0..side {
                    let c = g.apply([step(red), step(green), step(blue)]);
                    let at = ((t * side + green) * width + blue * side + red) * 4;
                    for k in 0..3 {
                        // A value in 0..=255 after the clamp: the cast cannot wrap.
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let b = (c[k] * 255.0).round().clamp(0.0, 255.0) as u8;
                        out[at + k] = b;
                    }
                    out[at + 3] = 255;
                }
            }
        }
    }
    out
}

/// Which tables a frame blends, and how much of each: night, the low sun, and the day's own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blend {
    pub tables: [u32; 3],
    pub weights: [f32; 3],
}

/// The tables `frame` is graded by.
#[must_use]
pub fn blend(frame: &FrameDelta) -> Blend {
    let weather = Weather::of(&frame.day_group);
    let day = day_weight(frame);
    let (low, high) = match weather {
        Weather::Clear | Weather::Sunny => {
            let low = if frame.light.time_of_day < 0.5 {
                MORNING
            } else {
                EVENING
            };
            (low, NOON)
        }
        Weather::Cloudy => (OVERCAST, OVERCAST),
        Weather::Rainy => (RAIN, RAIN),
    };
    let tw = twilight(frame);
    Blend {
        tables: [NIGHT, low, high],
        weights: [1.0 - day, day * tw, day * (1.0 - tw)],
    }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 1 at `peak` degrees of hue, falling to 0 at `from` and `to`, smoothly.
fn bump(h: f32, from: f32, peak: f32, to: f32) -> f32 {
    if h <= from || h >= to {
        0.0
    } else if h < peak {
        smoothstep(from, peak, h)
    } else {
        1.0 - smoothstep(peak, to, h)
    }
}

/// Hue in degrees, saturation and value.
fn hsv(c: [f32; 3]) -> (f32, f32, f32) {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let d = max - min;
    if d <= 1e-6 || max <= 1e-6 {
        return (0.0, 0.0, max);
    }
    let h = if (max - c[0]).abs() < f32::EPSILON {
        60.0 * ((c[1] - c[2]) / d).rem_euclid(6.0)
    } else if (max - c[1]).abs() < f32::EPSILON {
        60.0 * ((c[2] - c[0]) / d + 2.0)
    } else {
        60.0 * ((c[0] - c[1]) / d + 4.0)
    };
    (h, d / max, max)
}

fn rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let m = v - c;
    // A sector of the hue circle, 0 to 5.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r + m, g + m, b + m]
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// The identity table holds every step of the cube as it is, to the byte.
    #[test]
    fn the_identity_table_holds_every_color_as_it_is() {
        let p = picture(&[Grade::IDENTITY]);
        let side = SIDE as usize;
        for green in 0..side {
            for blue in 0..side {
                for red in 0..side {
                    let at = (green * side * side + blue * side + red) * 4;
                    #[allow(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        clippy::cast_precision_loss
                    )]
                    let want = |i: usize| ((i * 255) as f32 / 31.0).round() as u8;
                    assert_eq!(
                        &p[at..at + 3],
                        &[want(red), want(green), want(blue)],
                        "{red} {green} {blue}"
                    );
                }
            }
        }
    }

    /// A day's grade freshens an olive ground toward yellow-green and lifts the black of the
    /// shade toward blue, and keeps white white.
    #[test]
    fn a_days_grade_freshens_olive_and_lifts_the_shade_toward_blue() {
        let olive = [0.36, 0.33, 0.16];
        let (h0, s0, _) = hsv(olive);
        let (h1, s1, _) = hsv(Grade::NOON.apply(olive));
        assert!(h1 > h0 + 3.0, "{h0} -> {h1}");
        assert!(s1 > s0, "{s0} -> {s1}");
        let black = Grade::NOON.apply([0.0; 3]);
        assert!(black[2] > black[0] && black[0] > 0.0, "{black:?}");
        let white = Grade::NOON.apply([1.0; 3]);
        assert!(white.iter().all(|v| *v > 0.99), "{white:?}");
    }

    /// A look's grade table changes that table alone, the player's over the pack's, number by
    /// number; the identity is never changed.
    #[test]
    fn a_looks_grade_table_changes_that_table_the_players_over_the_packs() {
        use crate::look::{Layer, LookStack, LookToml};
        let mut looks = LookStack::new();
        looks.set(
            Layer::ArtPack,
            Some(
                LookToml::parse("[grade.noon]\nsaturation = 1.3\ncontrast = 1.2\n")
                    .expect("a look"),
            ),
        );
        looks.set(
            Layer::Player,
            Some(LookToml::parse("[grade.noon]\nsaturation = 0.9\n").expect("a look")),
        );
        let t = tables_with(&looks);
        assert_eq!(t[0], Grade::IDENTITY);
        assert_eq!(t[1].saturation, 0.9);
        assert_eq!(t[1].contrast, 1.2);
        assert_eq!(t[1].gain, Grade::NOON.gain);
        assert_eq!(t[2], Grade::MORNING);
        assert_eq!(tables_with(&LookStack::new()), TABLES);
    }

    /// The weights of a frame's tables add to one.
    #[test]
    fn a_frames_tables_weigh_one_in_all() {
        let mut frame = FrameDelta::default();
        for t in [0.0, 0.2, 0.3, 0.5, 0.8, 0.95] {
            frame.light.time_of_day = t;
            for z in [-1.0, -0.05, 0.05, 0.2, 0.7, 1.0] {
                frame.light.sun = glam::Vec3::new(0.3, 0.2, z);
                let b = blend(&frame);
                let sum: f32 = b.weights.iter().sum();
                assert!((sum - 1.0).abs() < 1e-5, "{b:?}");
            }
        }
    }
}
