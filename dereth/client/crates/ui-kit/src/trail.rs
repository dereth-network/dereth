//! The way a journey goes, drawn in the world: a faint wisp that leaves the character at the middle
//! of its body and drifts along the route ahead, thinning and fading as it goes, with a few motes
//! drifting off it where it leaves the body; and where the journey ends, the selection's marker, a
//! ring on the ground turning slowly and an arrow over it bobbing. The wisp, the motes and the
//! arrow reach the world as bands ([`WorldBand`]) and the ring as a ground mark ([`GroundMark`]),
//! which the world renderer draws there.
//!
//! A plugin's paths are drawn the same way ([`path_band`]): a glow like the wisp, a ribbon, dashes
//! or a picture of the plugin's own.
//!
//! The wisp and the motes are drawn from the art's pictures where the host has them (a strip that
//! repeats along the wisp, and one mote, each in its own colours); where it has not, from a glow
//! and a soft dot drawn here.

use dereth_primitives::{TextureData, TextureFormat};
use dereth_ui_overlay::{
    BandPoint, GroundMark, OverlaySpace, OverlayTexture, PathLook, PathStyle, WorldBand,
};

use crate::selection;

/// The wisp's texture, among the interface's own: a soft glow across it.
pub const WISP_TEXTURE: OverlayTexture = OverlayTexture {
    space: OverlaySpace::Local,
    key: 0x5749_5350,
};

/// The journey's end's ring's texture, among the interface's own: the selection's ring.
pub const GOAL_TEXTURE: OverlayTexture = OverlayTexture {
    space: OverlaySpace::Local,
    key: 0x474F_414C,
};

/// A ribbon path's texture: flat across, soft at its edges.
pub const RIBBON_TEXTURE: OverlayTexture = OverlayTexture {
    space: OverlaySpace::Local,
    key: 0x5249_4242,
};

/// A dashed path's texture: one dash along it, then its gap.
pub const DASH_TEXTURE: OverlayTexture = OverlayTexture {
    space: OverlaySpace::Local,
    key: 0x4441_5348,
};

/// A mote's texture, among the interface's own.
pub const MOTE_TEXTURE: OverlayTexture = OverlayTexture {
    space: OverlaySpace::Local,
    key: 0x4D4F_5445,
};

/// The arrow over the journey's end's texture, among the interface's own.
pub const ARROW_TEXTURE: OverlayTexture = OverlayTexture {
    space: OverlaySpace::Local,
    key: 0x4741_5257,
};

/// Where on the body the wisp leaves it and how high it flows, as a share of the body's height.
pub const WISP_HEIGHT: f32 = 0.55;

/// A body's height, metres, where the body is not known.
pub const BODY_HEIGHT: f32 = 1.8;

/// How far along the wisp it has faded away, metres.
pub const WISP_FADE: f32 = 24.0;

/// How wide the wisp is at its widest, metres to each side of its line.
pub const WISP_HALF_WIDTH: f32 = 0.24;

/// The colour of the glow drawn here in the art's place, `0xRRGGBB`: a pale blue, nearly white. The
/// art has colours of its own, and the wisp is drawn in them.
pub const WISP_COLOUR: u32 = 0x9A_D8_FF;

/// How far before its end, where the way leaves the ground or the journey ends, the wisp fades
/// out, metres.
pub const WISP_END_FADE: f32 = 2.0;

/// How fast the wisp's picture moves along it, away from the body, metres a second.
pub const WISP_DRIFT: f32 = 0.8;

/// How many motes drift off the wisp at a time, how long each lasts, seconds, and how far along
/// the wisp from the body it drifts in that time, metres.
pub const MOTES: usize = 5;
pub const MOTE_LIFE: f64 = 3.2;
pub const MOTE_RUN: f32 = 1.8;

/// How far the journey's end's ring reaches from its centre, metres.
pub const GOAL_RADIUS: f32 = 0.75;

/// How far over the journey's end the arrow's point and its tail stand, metres, and how far it
/// bobs.
pub const ARROW_LOW: f32 = 1.6;
pub const ARROW_HIGH: f32 = 2.3;
pub const ARROW_BOB: f32 = 0.08;

/// How many pixels each picture is on a side.
const SIZE: u32 = 64;

/// A picture drawn here, `alpha(x, y)` its strength at each point (`x` along the picture's
/// width and `y` down its height, 0 to 1), in white the tint colours.
fn drawn(alpha: impl Fn(f32, f32) -> f32) -> TextureData {
    drawn_in(0x00FF_FFFF, alpha)
}

/// [`drawn`] in `colour` (`0xRRGGBB`).
fn drawn_in(colour: u32, alpha: impl Fn(f32, f32) -> f32) -> TextureData {
    #[allow(clippy::cast_possible_truncation)] // a byte of the colour
    let [b, g, r] = [colour as u8, (colour >> 8) as u8, (colour >> 16) as u8];
    let n = SIZE;
    #[allow(clippy::cast_precision_loss)]
    let size = n as f32;
    let mut bgra = Vec::with_capacity((n * n * 4) as usize);
    for row in 0..n {
        for col in 0..n {
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = ((col as f32 + 0.5) / size, (row as f32 + 0.5) / size);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let a = (alpha(x, y).clamp(0.0, 1.0) * 255.0).round() as u8;
            bgra.extend_from_slice(&[b, g, r, a]);
        }
    }
    TextureData {
        width: n,
        height: n,
        format: TextureFormat::Bgra8,
        levels: vec![bgra],
    }
}

/// The wisp's glow where the host has no art: strongest along its middle (the picture's middle
/// row), soft to nothing at its edges, the same all along it, in [`WISP_COLOUR`].
#[must_use]
pub fn glow() -> TextureData {
    drawn_in(WISP_COLOUR, |_, y| {
        let d = (y - 0.5) / 0.5;
        let soft = dereth_primitives::num::math::expf(-d * d * 4.0);
        let core = dereth_primitives::num::math::expf(-d * d * 24.0);
        0.55 * soft + 0.45 * core
    })
}

/// A ribbon: full across its middle, soft to nothing at its edges.
#[must_use]
pub fn ribbon() -> TextureData {
    drawn(|_, y| ((0.5 - (y - 0.5).abs()) / 0.15).clamp(0.0, 1.0))
}

/// A dash: from the picture's left to past its middle along it, rounded at its ends and soft at
/// its edges, then nothing.
#[must_use]
pub fn dash() -> TextureData {
    drawn(|x, y| {
        let across = ((0.5 - (y - 0.5).abs()) / 0.15).clamp(0.0, 1.0);
        let along = ((x - 0.05) / 0.06).clamp(0.0, 1.0) * ((0.6 - x) / 0.06).clamp(0.0, 1.0);
        across * along
    })
}

/// A mote where the host has no art: a soft dot in [`WISP_COLOUR`].
#[must_use]
pub fn mote() -> TextureData {
    drawn_in(WISP_COLOUR, |x, y| {
        let r = ((x - 0.5) * (x - 0.5) + (y - 0.5) * (y - 0.5)).sqrt() / 0.5;
        dereth_primitives::num::math::expf(-r * r * 5.0) * (1.0 - r).clamp(0.0, 1.0)
    })
}

/// The selection's arrow drawn here, pointing to the picture's right (along the band from its
/// first point to its last): a head and a short shaft, soft at their edges.
#[must_use]
pub fn arrow() -> TextureData {
    drawn(|x, y| {
        let across = (y - 0.5).abs();
        // The head: from x 0.45 to its point at 0.95, as wide as the picture at its back.
        let head = if (0.45..=0.95).contains(&x) {
            let half = 0.45 * (0.95 - x) / 0.5;
            ((half - across) / 0.04).clamp(0.0, 1.0)
        } else {
            0.0
        };
        // The shaft: from 0.05 to the head's back, a fifth of the picture wide.
        let shaft = if (0.05..0.5).contains(&x) {
            ((0.1 - across) / 0.03).clamp(0.0, 1.0) * ((x - 0.05) / 0.08).clamp(0.0, 1.0)
        } else {
            0.0
        };
        head.max(shaft)
    })
}

/// `colour` (`0xRRGGBB`) at `strength` (0 to 1), packed `0xAARRGGBB`.
fn tinted(colour: u32, strength: f32) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let a = (strength.clamp(0.0, 1.0) * 255.0).round() as u32;
    a << 24 | (colour & 0x00FF_FFFF)
}

/// The built-in journey trail's style: a glow at the middle of the body, [`WISP_HALF_WIDTH`]
/// to each side at its widest, in its picture's own colours, moving away from the body at
/// [`WISP_DRIFT`] and fading over [`WISP_FADE`] metres.
#[must_use]
pub fn wisp_style() -> PathStyle {
    PathStyle {
        look: PathLook::Glow,
        width: 2.0 * WISP_HALF_WIDTH,
        height: None,
        color: 0x00FF_FFFF,
        glow: 0.9,
        fade: WISP_FADE,
        drift: WISP_DRIFT,
        reach: 30.0,
        from_player: true,
    }
}

/// The wisp through `line` (places `step` metres apart, the first where the character stands)
/// for a body `height` metres tall, at `time` seconds, its picture `aspect` times as long as it is
/// high and `between(a, b, f)` the place `f` of the way from `a` to `b`: the built-in journey
/// trail, a path in [`wisp_style`] at `strength` (0 to 1), which a trail coming in or going
/// out passes.
#[must_use]
pub fn wisp<P: Copy>(
    line: &[P],
    step: f32,
    height: f32,
    time: f64,
    aspect: f32,
    strength: f32,
    between: impl Fn(&P, &P, f32) -> P,
) -> WorldBand<P> {
    path_band(
        line,
        step,
        &wisp_style(),
        height,
        time,
        WISP_TEXTURE,
        aspect,
        strength,
        between,
    )
}

/// How long one dash and the gap after it are along a path, metres.
pub const DASH_LENGTH: f32 = 1.2;

/// The texture a path of `look` is drawn with; `None` for a picture of the plugin's own.
#[must_use]
pub fn look_texture(look: &PathLook) -> Option<OverlayTexture> {
    match look {
        PathLook::Glow => Some(WISP_TEXTURE),
        PathLook::Ribbon => Some(RIBBON_TEXTURE),
        PathLook::Dashes => Some(DASH_TEXTURE),
        PathLook::Image(_) => None,
    }
}

/// A path through `line` (places `step` metres apart, the first where it starts) drawn in `style`
/// with `texture` at `strength` (0 to 1, as it comes in or goes out), for a body `body` metres
/// tall, at `time` seconds; `aspect` is a picture's width over its height, and `between(a, b, f)`
/// the place `f` of the way from `a` to `b`.
///
/// - A glow leaves its start thin and widens over the first 0.8 m, thins and fades along it,
///   rises and falls a little and shimmers a little; its picture repeats along it, as long a
///   repeat as the picture is across its widest, moving along it at the style's drift.
/// - A ribbon is as wide as its style says all along, after the first half metre.
/// - Dashes, and a picture, repeat along it ([`DASH_LENGTH`], or the picture's width over its
///   height times the path's width), moving along it at the style's drift.
///
/// Each fades to nothing over the style's fade (with a fade of 0 it does not), and out over its
/// last [`WISP_END_FADE`] metres. It is held the style's height over the ground, or at [`WISP_HEIGHT`] of the body.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn path_band<P: Copy>(
    line: &[P],
    step: f32,
    style: &PathStyle,
    body: f32,
    time: f64,
    texture: OverlayTexture,
    aspect: f32,
    strength: f32,
    between: impl Fn(&P, &P, f32) -> P,
) -> WorldBand<P> {
    use dereth_primitives::num::math::{powf, sinf};
    #[allow(clippy::cast_possible_truncation)]
    let t = (time % 3600.0) as f32;
    #[allow(clippy::cast_precision_loss)]
    let end = step * line.len().saturating_sub(1) as f32;
    let height = style.height.unwrap_or(body * WISP_HEIGHT);
    let half = style.width * 0.5;
    let glow = style.look == PathLook::Glow;
    // The length one repeat of a dash or a picture takes along the path.
    let tile = match &style.look {
        PathLook::Dashes => Some(DASH_LENGTH),
        PathLook::Image(_) | PathLook::Glow => Some((style.width * aspect).max(0.05)),
        PathLook::Ribbon => None,
    };
    let point = |at: P, s: f32, u: f32| {
        let left = if style.fade > 0.0 {
            (1.0 - s / style.fade).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let ending = ((end - s) / WISP_END_FADE).clamp(0.0, 1.0);
        if glow {
            let start = (s / 0.8).clamp(0.0, 1.0);
            let shimmer = 0.85 + 0.15 * sinf(s * 2.3 - t * 3.1) * sinf(s * 0.9 + t * 1.1 + 1.0);
            BandPoint {
                at,
                lift: height + 0.05 * sinf(s * 1.1 - t * 1.7) * start,
                half_width: 0.015 + (half - 0.015).max(0.0) * start * powf(left, 0.6),
                tint: tinted(
                    style.color,
                    style.glow * strength * start.max(0.25) * powf(left, 1.2) * ending * shimmer,
                ),
                u,
            }
        } else {
            let start = (s / 0.5).clamp(0.0, 1.0);
            BandPoint {
                at,
                lift: height,
                half_width: half,
                tint: tinted(
                    style.color,
                    style.glow * strength * start.max(0.3) * left * ending,
                ),
                u,
            }
        }
    };
    let Some(tile) = tile else {
        let points = line
            .iter()
            .enumerate()
            .map(|(i, &at)| {
                #[allow(clippy::cast_precision_loss)]
                let s = step * i as f32;
                point(at, s, 0.5)
            })
            .collect();
        return WorldBand { points, texture };
    };
    // Repeated along it: each repeat's picture runs `u` 0 to 1, so where one ends and the next
    // begins the path has two points in one place, the first ending the repeat and the second
    // beginning the next.
    let offset = style.drift * t;
    let along = |s: f32| (s - offset) / tile;
    let mut points = Vec::with_capacity(line.len() * 2);
    for (i, &at) in line.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let s = step * i as f32;
        let u = along(s);
        points.push(point(at, s, u - u.floor()));
        let Some(&next) = line.get(i + 1) else {
            break;
        };
        // The next point's `u` as that point works it out, so a repeat ending on it is not lost
        // to rounding.
        #[allow(clippy::cast_precision_loss)]
        let s_next = step * (i + 1) as f32;
        let (k0, k1) = (u.floor(), along(s_next).floor());
        let mut k = k0 + 1.0;
        while k <= k1 {
            let sb = k * tile + offset;
            let f = ((sb - s) / (s_next - s)).clamp(0.0, 1.0);
            let at_b = between(&at, &next, f);
            points.push(point(at_b, sb, 1.0));
            points.push(point(at_b, sb, 0.0));
            k += 1.0;
        }
    }
    WorldBand { points, texture }
}

/// The motes drifting off the wisp through `line` (as for [`wisp`]) at `time` seconds: [`MOTES`]
/// of them, each leaving the body at the wisp's height, drifting [`MOTE_RUN`] metres along it and
/// rising a little over [`MOTE_LIFE`] seconds, faint (at `strength`, as the wisp), fading in and
/// out; each a small square band
/// facing the camera, drawn with [`MOTE_TEXTURE`].
#[must_use]
pub fn motes<P: Copy>(
    line: &[P],
    step: f32,
    height: f32,
    time: f64,
    strength: f32,
    between: impl Fn(&P, &P, f32) -> P,
) -> Vec<WorldBand<P>> {
    use dereth_primitives::num::math::sinf;
    if line.len() < 2 {
        return Vec::new();
    }
    (0..MOTES)
        .map(|k| {
            #[allow(clippy::cast_precision_loss)]
            let k = k as f32;
            #[allow(clippy::cast_possible_truncation)]
            let phase = ((time / MOTE_LIFE + f64::from(k) / 5.0).fract()) as f32;
            let s = phase * MOTE_RUN;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let i = ((s / step) as usize).min(line.len() - 2);
            #[allow(clippy::cast_precision_loss)]
            let f = (s / step - i as f32).clamp(0.0, 1.0);
            let at = between(&line[i], &line[i + 1], f);
            let size = 0.07 + 0.03 * sinf(k * 2.3);
            let lift = height * WISP_HEIGHT + 0.06 * sinf(k * 1.7) + phase * 0.25;
            let strength = 0.6 * strength * sinf(std::f32::consts::PI * phase);
            let point = |lift: f32, u: f32| BandPoint {
                at,
                lift,
                half_width: size * 0.5,
                tint: tinted(0x00FF_FFFF, strength),
                u,
            };
            WorldBand {
                points: vec![point(lift - size * 0.5, 0.0), point(lift + size * 0.5, 1.0)],
                texture: MOTE_TEXTURE,
            }
        })
        .collect()
}

/// The selection's ring laid at the journey's end `at` at `time` seconds, turning as the
/// selection's does.
#[must_use]
pub fn goal_ring<P>(at: P, time: f64) -> GroundMark<P> {
    let turns = (time / selection::RING_TURN_SECONDS).fract();
    #[allow(clippy::cast_possible_truncation)]
    let turn = (turns * std::f64::consts::TAU) as f32;
    GroundMark {
        at,
        radius: GOAL_RADIUS,
        texture: GOAL_TEXTURE,
        uv: [0.0, 0.0, 1.0, 1.0],
        tint: selection::UNTINTED,
        turn,
    }
}

/// The selection's arrow over the journey's end `at` at `time` seconds, pointing down at it and
/// bobbing as the selection's does.
#[must_use]
pub fn goal_arrow<P: Copy>(at: P, time: f64) -> WorldBand<P> {
    #[allow(clippy::cast_possible_truncation)]
    let phase = ((time * selection::ARROW_BOB_RATE) % std::f64::consts::TAU) as f32;
    let bob = ARROW_BOB * dereth_primitives::num::math::sinf(phase);
    let half_width = (ARROW_HIGH - ARROW_LOW) * 0.45;
    let point = |lift: f32, u: f32| BandPoint {
        at,
        lift: lift + bob,
        half_width,
        tint: selection::UNTINTED,
        u,
    };
    WorldBand {
        points: vec![point(ARROW_HIGH, 0.0), point(ARROW_LOW, 1.0)],
        texture: ARROW_TEXTURE,
    }
}

/// Every picture the journey is drawn with, drawn here, as the textures they go under: the wisp's
/// glow and the mote stand in for the art's until the host has it.
#[must_use]
pub fn pictures() -> [(OverlayTexture, TextureData); 6] {
    [
        (WISP_TEXTURE, glow()),
        (MOTE_TEXTURE, mote()),
        (RIBBON_TEXTURE, ribbon()),
        (DASH_TEXTURE, dash()),
        (GOAL_TEXTURE, selection::drawn()),
        (ARROW_TEXTURE, arrow()),
    ]
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (pictures and a wisp a front end draws for a journey; not a behaviour of
    //! the retail client)
    use super::*;

    fn alpha(t: &TextureData, x: u32, y: u32) -> u8 {
        t.levels[0][((y * t.width + x) * 4 + 3) as usize]
    }

    #[test]
    fn the_glow_is_strongest_along_its_middle_and_the_arrow_points_along_the_band() {
        let g = glow();
        assert!(alpha(&g, 10, 32) > 200 && alpha(&g, 10, 2) < 20);
        assert_eq!(alpha(&g, 10, 32), alpha(&g, 50, 32), "the same all along");
        let a = arrow();
        assert!(alpha(&a, 55, 32) > 200, "the head, near the point");
        assert!(alpha(&a, 55, 4) < 20, "beside the point");
        assert!(alpha(&a, 32, 12) > 150, "the head's back is wide");
        assert!(
            alpha(&a, 15, 32) > 150 && alpha(&a, 15, 20) < 20,
            "the shaft is narrow"
        );
    }

    /// 30 m in 0.4 m steps, each place its distance along.
    fn line() -> Vec<f32> {
        (0..=75).map(|i| i as f32 * 0.4).collect()
    }

    fn lerp(a: &f32, b: &f32, f: f32) -> f32 {
        a + (b - a) * f
    }

    #[test]
    fn the_wisp_leaves_the_body_at_its_middle_and_fades_away_along_the_way() {
        let w = wisp(&line(), 0.4, 1.8, 0.0, 18.4, 1.0, lerp);
        let first = w.points[0];
        assert!(
            (first.lift - 0.99).abs() < 0.06,
            "the body's middle: {}",
            first.lift
        );
        let strength = |p: &BandPoint<f32>| p.tint >> 24;
        let at = |s: f32| {
            w.points
                .iter()
                .find(|p| (p.at - s).abs() < 1e-3)
                .expect("a place there")
        };
        let widest = w.points.iter().map(|p| p.half_width).fold(0.0, f32::max);
        assert!(
            first.half_width < widest / 2.0,
            "thin where it leaves the body"
        );
        let (near, far) = (strength(at(2.0)), strength(at(20.0)));
        assert!(
            near > far && far > 0,
            "fading along the way: {near} then {far}"
        );
        assert!(
            w.points
                .iter()
                .filter(|p| p.at >= 24.0)
                .all(|p| strength(p) == 0),
            "gone by 24 m"
        );
        assert!(
            w.points.iter().all(|p| strength(p) < 240),
            "never quite solid"
        );
        assert!(
            w.points.iter().all(|p| p.tint & 0x00FF_FFFF == 0x00FF_FFFF),
            "in the picture's own colours"
        );
    }

    /// The picture repeats along the wisp, a repeat as long as the picture is across the wisp's
    /// widest, each repeat's `u` 0 to 1 with its ends doubled in one place; it moves away from
    /// the body at the wisp's drift.
    #[test]
    fn the_wisps_picture_repeats_along_it_and_moves_away_from_the_body() {
        let ends = |time: f64| {
            let w = wisp(&line(), 0.4, 1.8, time, 10.0, 1.0, lerp);
            assert!(w.points.iter().all(|p| (0.0..=1.0).contains(&p.u)));
            w.points
                .windows(2)
                .filter(|p| (p[0].at - p[1].at).abs() < 1e-5 && p[0].u == 1.0 && p[1].u == 0.0)
                .map(|p| p[0].at)
                .collect::<Vec<f32>>()
        };
        let tile = 2.0 * WISP_HALF_WIDTH * 10.0;
        let now = ends(0.0);
        assert!(now.len() >= 5, "{now:?}");
        assert!((now[1] - now[0] - tile).abs() < 1e-3, "{now:?}");
        let later = ends(1.0);
        assert!(
            later
                .iter()
                .any(|a| (a - (now[0] + WISP_DRIFT)).abs() < 1e-3),
            "a second on, each end has moved {WISP_DRIFT} m away from the body: {later:?}"
        );
    }

    /// A wisp that ends short of its fade (where the way leaves the ground) fades out over its last
    /// two metres, not along its whole length; and it is drawn at the strength it is given, as a
    /// trail coming in or going out is.
    #[test]
    fn a_short_wisp_fades_out_at_its_end_and_is_drawn_at_its_strength() {
        let short: Vec<f32> = (0..=25).map(|i| i as f32 * 0.4).collect(); // 10 m to a portal
        let w = wisp(&short, 0.4, 1.8, 0.0, 18.4, 1.0, lerp);
        let at = |w: &WorldBand<f32>, s: f32| {
            w.points
                .iter()
                .find(|p| (p.at - s).abs() < 1e-3)
                .expect("a place there")
                .tint
                >> 24
        };
        let long = wisp(&line(), 0.4, 1.8, 0.0, 18.4, 1.0, lerp);
        assert_eq!(
            at(&w, 7.6),
            at(&long, 7.6),
            "short of its end, as strong as a long wisp there"
        );
        assert!(at(&w, 9.2) < at(&w, 7.6) / 2, "fading at its end");
        assert_eq!(at(&w, 10.0), 0, "gone at its end");
        let half = wisp(&short, 0.4, 1.8, 0.0, 18.4, 0.5, lerp);
        let (full, halved) = (at(&w, 4.0), at(&half, 4.0));
        assert!(
            halved.abs_diff(full / 2) <= 1,
            "{full} at full strength, {halved} at half"
        );
    }

    /// The picture's phase belongs to the character, not to the line: two lines of different
    /// lengths at one moment (the way before and after a corner is passed, or a new plan) start at
    /// one place in the picture and repeat at the same distances from the character, so the pattern
    /// does not jump when the line under it changes.
    #[test]
    fn the_wisps_phase_carries_on_when_the_line_under_it_changes() {
        let short: Vec<f32> = (0..=40).map(|i| i as f32 * 0.4).collect();
        let (a, b) = (
            wisp(&line(), 0.4, 1.8, 2.3, 10.0, 1.0, lerp),
            wisp(&short, 0.4, 1.8, 2.3, 10.0, 1.0, lerp),
        );
        assert!((a.points[0].u - b.points[0].u).abs() < 1e-6);
        let ends = |w: &WorldBand<f32>| {
            w.points
                .iter()
                .filter(|p| p.u == 1.0)
                .map(|p| p.at)
                .take(3)
                .collect::<Vec<f32>>()
        };
        assert_eq!(ends(&a), ends(&b));
    }

    #[test]
    fn a_few_faint_motes_drift_off_the_wisp_where_it_leaves_the_body() {
        let m = motes(&line(), 0.4, 1.8, 0.7, 1.0, lerp);
        assert_eq!(m.len(), MOTES);
        for band in &m {
            assert_eq!(band.texture, MOTE_TEXTURE);
            let (low, high) = (band.points[0], band.points[1]);
            assert!(low.at <= MOTE_RUN + 1e-4, "near the body: {}", low.at);
            assert!(
                high.lift > low.lift && (high.lift - low.lift) < 0.15,
                "small"
            );
            assert!(
                low.lift > 0.8 && low.lift < 1.5,
                "about the wisp's height: {}",
                low.lift
            );
            assert!(low.tint >> 24 < 160, "faint");
        }
        let later = motes(&line(), 0.4, 1.8, 1.0, 1.0, lerp);
        assert_ne!(m[0].points[0].at, later[0].points[0].at, "drifting");
    }

    /// Dashes repeat along the path, each repeat's `u` running 0 to 1 and its ends doubled in one
    /// place; a whole repeat's drift later they stand where they stood.
    #[test]
    fn dashes_repeat_along_a_path_and_drift_along_it() {
        let line: Vec<f32> = (0..=25).map(|i| i as f32 * 0.4).collect(); // 10 m
        let style = PathStyle {
            look: PathLook::Dashes,
            fade: 0.0,
            ..wisp_style()
        };
        let lerp = |a: &f32, b: &f32, f: f32| a + (b - a) * f;
        let band = path_band(&line, 0.4, &style, 1.8, 0.0, DASH_TEXTURE, 1.0, 1.0, lerp);
        let ends: Vec<f32> = band
            .points
            .windows(2)
            .filter(|w| (w[0].at - w[1].at).abs() < 1e-5 && w[0].u == 1.0 && w[1].u == 0.0)
            .map(|w| w[0].at)
            .collect();
        assert_eq!(ends.len(), 8, "a repeat every 1.2 m along 10 m: {ends:?}");
        assert!((ends[1] - ends[0] - DASH_LENGTH).abs() < 1e-4);
        assert!(band.points.iter().all(|p| (0.0..=1.0).contains(&p.u)));
        let moving = PathStyle {
            drift: 1.2,
            ..style.clone()
        };
        let later = path_band(&line, 0.4, &moving, 1.8, 0.5, DASH_TEXTURE, 1.0, 1.0, lerp);
        let moved: Vec<f32> = later
            .points
            .iter()
            .filter(|p| p.u == 1.0)
            .map(|p| p.at)
            .collect();
        assert!(
            moved.iter().any(|a| (a - (ends[0] + 0.6)).abs() < 1e-3),
            "half a second at 1.2 m/s moves each end 0.6 m on: {moved:?}"
        );
        let round = path_band(&line, 0.4, &moving, 1.8, 1.0, DASH_TEXTURE, 1.0, 1.0, lerp);
        let b = round
            .points
            .iter()
            .find(|p| p.u == 1.0)
            .expect("a repeat ends")
            .at;
        assert!(
            (b - ends[0]).abs() < 1e-3,
            "a whole repeat later, where it was: {b}"
        );
    }

    #[test]
    fn the_journeys_end_has_the_selections_ring_and_an_arrow_over_it() {
        let ring = goal_ring(7_u32, 0.0);
        assert_eq!((ring.at, ring.texture), (7, GOAL_TEXTURE));
        let arrow = goal_arrow(7_u32, 0.0);
        assert_eq!(arrow.points.len(), 2);
        assert!(arrow.points[0].lift > arrow.points[1].lift, "pointing down");
        let ring = pictures()
            .into_iter()
            .find(|(t, _)| *t == GOAL_TEXTURE)
            .expect("the ring's picture");
        assert_eq!(ring.1.levels, selection::drawn().levels);
    }
}
