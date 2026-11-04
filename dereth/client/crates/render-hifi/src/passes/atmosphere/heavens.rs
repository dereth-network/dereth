//! The Remastered sky: retail's clouds, sun, moons and stars drawn again over the physical sky,
//! each where retail puts it and at retail's hours, at modern quality.
//!
//! Retail's sky is a handful of objects: a dome, a night layer that darkens it after dusk with
//! its stars painted on, two moons on cards, a cloud deck, and the sun on a card of its own. While
//! an art pack is open and Retail Look is off, and the sky's knob has it on, the world replay
//! leaves out the night layer, the moons, the cloud deck and the sun, and they are drawn again
//! where the night layer would have been: the night layer, the stars and the sun, then the moons,
//! then the clouds over them all:
//!
//! - **The clouds** are a layer of air over the eye whose cover is retail's own cloud picture,
//!   laid over the world as retail lays it, as large overhead as retail shows it, and drifted as
//!   retail drifts it with the game's clock. Fine noise fixed to the world breaks up their edges
//!   and turns over slowly with the clock. The sun lights them through themselves, brightest at
//!   their rims toward it, and the sky about them lights their shaded sides in retail's ambient
//!   color, its lavender by day and its purple at dusk and night; they take the color of
//!   retail's picture, are as bright as retail's hours make its deck, and as opaque as its hours
//!   leave it. Far off they take the haze's light, as the land does. They are worked out at a
//!   share of the frame's pixels once a frame, before the world is drawn, and enlarged.
//! - **The night layer** blackens the sky as retail's hours of it do, and the stars are drawn
//!   over it as points, twinkling with the clock.
//! - **The moons** the art pack has pictures for are balls the sky makes itself, standing where
//!   retail's cards stand as the eye sees them and as large, the pack's pictures laid over them
//!   longitude by latitude. They are lit from the sun's side as in open space, each waxing and
//!   waning over a month of its own; the lights on their night sides flicker, and a moon's clouds
//!   are a shell over its ground that shades it. A moon the pack has no pictures for is retail's.
//! - **The sun** is a disk with its glow where its light comes from: where the sky model's glow
//!   is, and where the shadows point.
//!
//! The dome, the physical sky over it and everything else retail draws in the sky stay as they
//! are. Nothing here moves but with the game's clock, and every pixel is worked out from the
//! steady line through its middle, so the anti-aliasing's jitter moves nothing either.

use glam::{Vec3, Vec4};

use super::model;
use crate::build::Build;
use crate::shared::pack_image;
use crate::snapshot::{HifiArtPack, HifiFrame, HifiPackImage, HifiSkyKnobs, HifiSkyObject};

/// What the clouds and the heavens share.
pub(crate) const HEAVENS_WGSL: &str = include_str!("heavens.wgsl");
/// The clouds.
pub(crate) const CLOUDS_WGSL: &str = include_str!("clouds.wgsl");
/// The night layer, the stars, the moons, the sun, and the clouds laid over them.
pub(crate) const CELESTIAL_WGSL: &str = include_str!("celestial.wgsl");

/// The constant block's size, bytes: twenty-five vectors.
pub const CONSTANTS_SIZE: usize = 25 * 16;
/// The texels a side of the clouds' shadow map.
pub const SHADOW_SIZE: u32 = 1024;
/// How far from the eye each way the clouds' shadow map reaches, meters.
pub const SHADOW_REACH: f32 = 12_000.0;
/// The sun's height, degrees, over which the clouds' shadows come in: a lower sun's would lie
/// kilometers off along the ground, past the reach of the shadow map.
pub const SHADOW_SUN: (f32, f32) = (8.0, 20.0);
/// How many moons are drawn again; past these, retail's own are drawn.
pub const MOONS: usize = 2;
/// The fine detail's texels a side, over one period.
pub const DETAIL_SIZE: u32 = 64;
/// How many periods of the fine detail one period of retail's cloud picture holds, each way.
pub const DETAIL_PER_PICTURE: f32 = 8.0;
/// The period the stars' twinkle repeats over, seconds of the game's clock.
pub const TWINKLE_PERIOD: f64 = 600.0;
/// The period the clouds' fine detail turns over in, seconds of the game's clock.
pub const DETAIL_PERIOD: f64 = 1800.0;
/// How far off, meters, the clouds keep a share 1/e of their own light, the rest the haze's.
pub const CLOUD_FADE: f32 = 60_000.0;
/// The sun's light on the clouds, against its color's brightest channel.
pub const CLOUD_SUN: f32 = 0.9;
/// How brightly the moons' lit faces are drawn, against their pictures' colour.
pub const MOON_LIGHT: f32 = 1.2;
/// The segments round a moon's ball and from pole to pole.
pub const BALL_SEGMENTS: (u16, u16) = (64, 32);
/// How far the sun's light on the clouds warms toward retail's fog color in the lowest sun.
pub const DUSK_GLOW: f32 = 0.6;
/// How much of their color the stars take from the night's light.
pub const STAR_TINT: f32 = 0.5;
/// The light about the clouds in the high sun, against the light retail's deck is drawn with:
/// the sun's light on them makes up the rest. In the low sun and at night they have all of it.
pub const CLOUD_AMBIENT: f32 = 0.6;

/// What a sky object is to the Remastered sky.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The dome, drawn first: kept.
    Dome,
    /// A cloud deck, its picture laid flat from above.
    Deck,
    /// A card whose picture glows over what is behind it: retail's sun.
    Sun,
    /// A card whose picture covers what is behind it: a moon.
    Moon,
    /// The night layer: neither the dome, a deck nor a card, its see-through ramped by the hours.
    Night,
    /// Anything else (the weather's layer, the emitters of the birds): kept.
    Other,
}

/// The role of sky object `o`.
#[must_use]
pub fn role(o: &HifiSkyObject) -> Role {
    if model::is_dome(o.index, o.properties) {
        return Role::Dome;
    }
    if o.properties & 4 != 0 {
        return Role::Other;
    }
    if o.deck.is_some() {
        return Role::Deck;
    }
    if let Some(card) = o.card {
        return if card.additive { Role::Sun } else { Role::Moon };
    }
    if o.properties == 0 && o.transparent >= 0.0 {
        return Role::Night;
    }
    Role::Other
}

/// What the Remastered sky draws again this frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// The cloud deck whose clouds are drawn again.
    pub deck: Option<HifiSkyObject>,
    /// The moons drawn again, in the order the sky draws them.
    pub moons: Vec<HifiSkyObject>,
    /// The night layer whose hours darken the sky.
    pub night: Option<HifiSkyObject>,
    /// The models whose own draws are left out, the presentation drawing them again.
    pub replaced: Vec<u32>,
}

impl Plan {
    /// Whether the draws of the model `gfx` are left out.
    #[must_use]
    pub fn replaces(&self, gfx: u32) -> bool {
        self.replaced.contains(&gfx)
    }
}

/// What `frame`'s sky draws again with `knobs`: the first deck with a picture, while the clouds
/// are on; the sun; the first two moons the art pack has pictures for; and the night layer.
#[must_use]
pub fn plan(frame: &HifiFrame, knobs: &HifiSkyKnobs) -> Plan {
    let mut p = Plan::default();
    let pictured = |gfx: u32| {
        frame
            .art_pack
            .as_ref()
            .is_some_and(|a| a.moons.iter().any(|m| m.object == gfx))
    };
    for o in frame.sky.objects.iter().filter(|o| o.gfx_id != 0) {
        match role(o) {
            Role::Deck if knobs.clouds && p.deck.is_none() && o.texture.is_some() => {
                p.deck = Some(*o);
                p.replaced.push(o.gfx_id);
            }
            Role::Sun => p.replaced.push(o.gfx_id),
            Role::Moon if p.moons.len() < MOONS && pictured(o.gfx_id) => {
                p.moons.push(*o);
                p.replaced.push(o.gfx_id);
            }
            Role::Night if p.night.is_none() => {
                p.night = Some(*o);
                p.replaced.push(o.gfx_id);
            }
            _ => {}
        }
    }
    p
}

/// How much of `o` its hours leave showing: 1 less its see-through, 1 when the hours leave it
/// alone.
fn showing(o: &HifiSkyObject) -> f32 {
    if o.transparent >= 0.0 {
        (1.0 - o.transparent * 0.01).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// An sRGB color's bytes as shares of one, still encoded.
fn srgb_byte_units(c: [u8; 3]) -> Vec3 {
    Vec3::new(f32::from(c[0]), f32::from(c[1]), f32::from(c[2])) / 255.0
}

/// Encoded sRGB shares of one in linear light.
fn srgb_to_linear(c: Vec3) -> Vec3 {
    let f = |x: f32| {
        if x <= 0.040_45 {
            x / 12.92
        } else {
            dereth_primitives::num::math::powf((x + 0.055) / 1.055, 2.4)
        }
    };
    Vec3::new(f(c.x), f(c.y), f(c.z))
}

/// `c` with its brightest channel 1.
fn normalized(c: Vec3) -> Vec3 {
    c / c.max_element().max(1e-4)
}

/// The laying of `deck`'s picture as the clouds lay it: retail's, its steps shrunk so the
/// picture is as large overhead at the clouds' height as retail's deck shows it at its own.
/// `[a, b, c, d]`, with `u = a + b x` and `v = c + d y` over the world's meters.
#[must_use]
pub fn laying(deck: &HifiSkyObject, knobs: &HifiSkyKnobs) -> Option<[f32; 4]> {
    let d = deck.deck?;
    let k = knobs.cloud_height / d.height.max(50.0) * knobs.cloud_scale.max(1e-3);
    let [a, b, c, e] = d.laying;
    Some([a, b / k, c, e / k])
}

/// Where the clouds' shadow map lies: over the world round the eye, its texels fixed to the
/// world, so the shadows hold still on the land as the eye moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowMap {
    /// Its center, render space, meters.
    pub center: glam::Vec2,
    /// How far it reaches each way from its center, meters.
    pub reach: f32,
    /// The height of the clouds' base, render space, meters: where the map reads them from, along
    /// the sun's line.
    pub base: f32,
    /// How much of the sun the clouds' shadows take.
    pub strength: f32,
}

/// The clouds' shadow map for `frame` with `knobs` and `plan`: none while the clouds are not
/// drawn again, their shadows' knob is at 0, or the sun stands too low to cast them.
#[must_use]
pub fn shadow_map(frame: &HifiFrame, knobs: &HifiSkyKnobs, plan: &Plan) -> Option<ShadowMap> {
    plan.deck.as_ref()?;
    let sun = frame.sky.sun_direction.normalize_or(Vec3::Z);
    let elevation = dereth_primitives::num::math::asinf(sun.z.clamp(-1.0, 1.0)).to_degrees();
    let rise = ((elevation - SHADOW_SUN.0) / (SHADOW_SUN.1 - SHADOW_SUN.0)).clamp(0.0, 1.0);
    let strength = knobs.cloud_shadows * rise * rise * (3.0 - 2.0 * rise);
    if strength <= 0.0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)] // a texture's size
    let texel = 2.0 * SHADOW_REACH / SHADOW_SIZE as f32;
    let cam = &frame.camera;
    // The eye in the world, render space less its shift, brought onto the world's texel grid.
    let shift = glam::Vec2::new(cam.block_shift.x, cam.block_shift.y);
    let world = glam::Vec2::new(cam.eye.x, cam.eye.y) - shift;
    let snapped = (world / texel).round() * texel;
    Some(ShadowMap {
        center: snapped + shift,
        reach: SHADOW_REACH,
        base: cam.eye.z + knobs.cloud_height,
        strength,
    })
}

/// `v` brought into `0..period`.
fn wrap(v: f64, period: f64) -> f32 {
    if period <= 0.0 {
        return 0.0;
    }
    #[allow(clippy::cast_possible_truncation)] // within one period
    let w = v.rem_euclid(period) as f32;
    w
}

/// The constant block the clouds and the heavens read, for `frame` drawn with `knobs` and `plan`,
/// the frame's world drawn `height` pixels tall and the clouds worked out at `cloud_height`.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn constants(
    frame: &HifiFrame,
    knobs: &HifiSkyKnobs,
    plan: &Plan,
    height: u32,
    cloud_height: u32,
) -> Vec<u8> {
    let sky = &frame.sky;
    let cam = &frame.camera;
    let clock = frame.time;
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let pixel = cam.fov_y / height.max(1) as f32;
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let cloud_pixel = cam.fov_y / cloud_height.max(1) as f32;
    let day = model::day_weight(sky);
    let sun_color = normalized(model::srgb_bytes_to_linear(sky.sun_color));
    let fog = normalized(model::srgb_bytes_to_linear(
        sky.fog.as_ref().map_or([200, 200, 220], |f| f.color),
    ));
    // The low sun's warmth, as the sky model is tinted toward it.
    let warm = normalized(model::sun_illuminance(sky));
    let mut rows = [Vec4::ZERO; 25];
    // --- the clouds -----------------------------------------------------------------------
    if let (Some(deck), Some(lay)) = (
        plan.deck.as_ref(),
        plan.deck.as_ref().and_then(|d| laying(d, knobs)),
    ) {
        rows[0] = Vec4::new(
            1.0,
            knobs.cloud_cover,
            knobs.cloud_height,
            knobs.cloud_depth,
        );
        rows[1] = Vec4::from_array(lay);
        let period_x = 1.0 / f64::from(lay[1].abs().max(1e-9));
        let period_y = 1.0 / f64::from(lay[3].abs().max(1e-9));
        let speed = f64::from(knobs.cloud_speed);
        // Where render space's origin lies in the world: render space less its shift.
        let shift = cam.block_shift;
        rows[2] = Vec4::new(
            wrap(clock * f64::from(deck.tex_velocity[0]) * speed, 1.0),
            wrap(clock * f64::from(deck.tex_velocity[1]) * speed, 1.0),
            wrap(-f64::from(shift.x), period_x),
            wrap(-f64::from(shift.y), period_y),
        );
        // As bright as retail's hours make its deck.
        let bright = if deck.luminosity > 0.0 {
            deck.luminosity * 0.01
        } else {
            1.0
        };
        // The sun on the clouds, nearly white by day; in the low sun it warms toward the color
        // of retail's fog, as retail's dawn and dusk do.
        let glow = sun_color * fog;
        let sun_light = Vec3::ONE
            .lerp(sun_color, 0.35)
            .lerp(warm, 0.4 * model::twilight(sky))
            .lerp(normalized(glow), DUSK_GLOW * model::twilight(sky))
            * CLOUD_SUN
            * knobs.cloud_light
            * day
            * model::Weather::of(&sky.day_group).exposure();
        rows[3] = sun_light.extend(1.0);
        // The light retail's own deck is drawn with at this hour, its own glow and the ambient
        // light, which the clouds take from all about them: their lavender by day, their purple at
        // dusk and night.
        let lit = (Vec3::splat(bright) + srgb_byte_units(sky.ambient_color) * sky.ambient_level)
            .min(Vec3::ONE);
        // A wet day's clouds are heavier and grayer, as the sky model exposes its sky.
        let weather = model::Weather::of(&sky.day_group).exposure();
        // By night and in the low sun that light is all they have, as on retail's deck; in the
        // high sun the sun's own light on them makes up part of it.
        let high = day * (1.0 - model::twilight(sky));
        let share = 1.0 + (CLOUD_AMBIENT - 1.0) * high;
        rows[4] = (srgb_to_linear(lit) * share * weather).extend(knobs.cloud_shade);
        rows[5].w = knobs.cloud_detail;
        #[allow(clippy::cast_precision_loss)] // a step count
        let steps = knobs.cloud_steps.max(1) as f32;
        #[allow(clippy::cast_possible_truncation)] // a period's share
        let turned = (clock / DETAIL_PERIOD).rem_euclid(1.0) as f32;
        #[allow(clippy::cast_possible_truncation)] // meters
        let detail = (period_x / f64::from(DETAIL_PER_PICTURE)) as f32;
        rows[6] = Vec4::new(steps, cloud_pixel, turned, detail);
        rows[18].w = showing(deck);
    }
    // --- the sun --------------------------------------------------------------------------
    rows[7] = sky
        .sun_direction
        .normalize_or(Vec3::Z)
        .extend((knobs.sun_size * 0.5).to_radians());
    rows[8] = sun_color
        .lerp(warm, 0.6 * model::twilight(sky))
        .extend(knobs.sun_glow);
    // --- the moons --------------------------------------------------------------------------
    for (k, m) in plan.moons.iter().enumerate().take(MOONS) {
        let Some(card) = m.card else { continue };
        let size = knobs.moon_size.max(1e-3);
        rows[9 + k] = (card.center - cam.eye).extend(1.0);
        rows[11 + k] = (card.u * size).extend(card.uv[0]);
        rows[13 + k] = (card.v * size).extend(card.uv[1]);
        let pictures = frame
            .art_pack
            .as_ref()
            .and_then(|a| a.moons.iter().find(|p| p.object == m.gfx_id));
        let lamps = pictures.is_some_and(|p| p.lights.is_some());
        rows[15 + k] = Vec4::new(
            MOON_LIGHT,
            if lamps { 1.0 } else { 0.0 },
            knobs
                .moon_phases
                .iter()
                .find(|(gfx, _)| *gfx == m.gfx_id)
                .map_or(1.0, |(_, lit)| *lit),
            showing(m),
        );
        rows[20][k] = if pictures.is_some_and(|p| p.clouds.is_some()) {
            1.0
        } else {
            0.0
        };
    }
    let clip_from_ray = model::ray_from_ndc(cam).inverse();
    for (c, row) in rows[21..25].iter_mut().enumerate() {
        *row = clip_from_ray.col(c);
    }
    // --- the night layer and the stars -----------------------------------------------------
    let night = plan.night.as_ref().map_or(1.0 - day, showing);
    // The stars take half their color from the night's own light, as retail's painted ones do.
    let hue = srgb_byte_units(sky.ambient_color);
    let peak = hue.max_element();
    let hue = if peak > 1e-3 { hue / peak } else { Vec3::ONE };
    let tint = Vec3::ONE.lerp(hue, STAR_TINT);
    rows[5] = tint.extend(rows[5].w);
    #[allow(clippy::cast_possible_truncation)] // a period's share
    let twinkle = (clock / TWINKLE_PERIOD).rem_euclid(1.0) as f32;
    rows[17] = Vec4::new(
        knobs.stars * night * night,
        knobs.star_twinkle,
        twinkle,
        pixel,
    );
    // Retail's night layer: as much of the sky as its hours show it, black.
    rows[18].x = if plan.night.is_some() { night } else { 0.0 };
    rows[18].y = day;
    rows[18].z = CLOUD_FADE;
    if let Some(map) = shadow_map(frame, knobs, plan) {
        rows[19] = Vec4::new(map.center.x, map.center.y, map.reach, map.base);
    }
    let mut out = Vec::with_capacity(CONSTANTS_SIZE);
    for v in rows {
        for f in v.to_array() {
            out.extend_from_slice(&f.to_le_bytes());
        }
    }
    out
}

/// The clouds' fine detail: noise of billows, the same in every direction, repeating over its
/// cube `DETAIL_SIZE` texels a side, one byte a texel, and every smaller level of it after it.
#[must_use]
pub fn detail_noise() -> Vec<Vec<u8>> {
    let n = DETAIL_SIZE as usize;
    let mut values = vec![0.0f32; n * n * n];
    #[allow(clippy::cast_precision_loss)] // a small cube
    let inv = 1.0 / n as f32;
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                #[allow(clippy::cast_precision_loss)] // a small cube
                let p = [x as f32 * inv, y as f32 * inv, z as f32 * inv];
                let w1 = 1.0 - worley(p, 6, 0x1a2b);
                let w2 = 1.0 - worley(p, 12, 0x3c4d);
                let g = gradient(p, 6, 0x5e6f) * 0.5 + 0.5;
                values[(z * n + y) * n + x] = 0.5 * w1 + 0.25 * w2 + 0.25 * g;
            }
        }
    }
    let lo = values.iter().copied().fold(f32::MAX, f32::min);
    let hi = values.iter().copied().fold(f32::MIN, f32::max);
    let span = (hi - lo).max(1e-6);
    // LINT-OK: a value brought into 0..1, as a byte.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let level0: Vec<u8> = values
        .iter()
        .map(|v| ((v - lo) / span * 255.0).round().clamp(0.0, 255.0) as u8)
        .collect();
    let mut levels = vec![level0];
    let mut side = n;
    while side > 1 {
        let prev = levels.last().map_or(&[][..], Vec::as_slice);
        let half = side / 2;
        let mut next = vec![0u8; half * half * half];
        for z in 0..half {
            for y in 0..half {
                for x in 0..half {
                    let mut sum = 0u32;
                    for (dz, dy, dx) in [
                        (0, 0, 0),
                        (0, 0, 1),
                        (0, 1, 0),
                        (0, 1, 1),
                        (1, 0, 0),
                        (1, 0, 1),
                        (1, 1, 0),
                        (1, 1, 1),
                    ] {
                        sum +=
                            u32::from(prev[((2 * z + dz) * side + 2 * y + dy) * side + 2 * x + dx]);
                    }
                    // LINT-OK: the mean of eight bytes.
                    #[allow(clippy::cast_possible_truncation)]
                    let mean = ((sum + 4) / 8) as u8;
                    next[(z * half + y) * half + x] = mean;
                }
            }
        }
        levels.push(next);
        side = half;
    }
    levels
}

/// The distance from `p` (each axis 0..1) to the nearest of a point in each cell of a `cells`
/// sided grid that wraps, as a share of a cell, at most 1.
fn worley(p: [f32; 3], cells: i32, seed: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // a small grid
    let g = cells as f32;
    let q = [p[0] * g, p[1] * g, p[2] * g];
    // LINT-OK: a cell index of a small grid.
    #[allow(clippy::cast_possible_truncation)]
    let c = [
        q[0].floor() as i32,
        q[1].floor() as i32,
        q[2].floor() as i32,
    ];
    let mut best = f32::MAX;
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let cell = [c[0] + dx, c[1] + dy, c[2] + dz];
                let w = [
                    cell[0].rem_euclid(cells),
                    cell[1].rem_euclid(cells),
                    cell[2].rem_euclid(cells),
                ];
                let h = cell_hash(w, seed);
                #[allow(clippy::cast_precision_loss)] // a small grid
                let at = [
                    cell[0] as f32 + unit(h),
                    cell[1] as f32 + unit(crate::shared::noise::pcg(h)),
                    cell[2] as f32 + unit(crate::shared::noise::pcg(h ^ 0x9e37)),
                ];
                let d = (at[0] - q[0]).powi(2) + (at[1] - q[1]).powi(2) + (at[2] - q[2]).powi(2);
                best = best.min(d);
            }
        }
    }
    best.sqrt().min(1.0)
}

/// Gradient noise at `p` (each axis 0..1) over a lattice `cells` a side that wraps, about -1..1.
fn gradient(p: [f32; 3], cells: i32, seed: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // a small lattice
    let g = cells as f32;
    let q = [p[0] * g, p[1] * g, p[2] * g];
    // LINT-OK: a lattice index of a small lattice.
    #[allow(clippy::cast_possible_truncation)]
    let c = [
        q[0].floor() as i32,
        q[1].floor() as i32,
        q[2].floor() as i32,
    ];
    #[allow(clippy::cast_precision_loss)] // a small lattice
    let f = [q[0] - c[0] as f32, q[1] - c[1] as f32, q[2] - c[2] as f32];
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let u = [fade(f[0]), fade(f[1]), fade(f[2])];
    let corner = |dx: i32, dy: i32, dz: i32| {
        let w = [
            (c[0] + dx).rem_euclid(cells),
            (c[1] + dy).rem_euclid(cells),
            (c[2] + dz).rem_euclid(cells),
        ];
        let h = cell_hash(w, seed);
        // One of the twelve edges of a cube.
        const DIRS: [[f32; 3]; 12] = [
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
            [1.0, -1.0, 0.0],
            [-1.0, -1.0, 0.0],
            [1.0, 0.0, 1.0],
            [-1.0, 0.0, 1.0],
            [1.0, 0.0, -1.0],
            [-1.0, 0.0, -1.0],
            [0.0, 1.0, 1.0],
            [0.0, -1.0, 1.0],
            [0.0, 1.0, -1.0],
            [0.0, -1.0, -1.0],
        ];
        let dir = DIRS[(h % 12) as usize];
        #[allow(clippy::cast_precision_loss)] // corner offsets of one
        let off = [f[0] - dx as f32, f[1] - dy as f32, f[2] - dz as f32];
        dir[0] * off[0] + dir[1] * off[1] + dir[2] * off[2]
    };
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(corner(0, 0, 0), corner(1, 0, 0), u[0]);
    let x10 = lerp(corner(0, 1, 0), corner(1, 1, 0), u[0]);
    let x01 = lerp(corner(0, 0, 1), corner(1, 0, 1), u[0]);
    let x11 = lerp(corner(0, 1, 1), corner(1, 1, 1), u[0]);
    lerp(lerp(x00, x10, u[1]), lerp(x01, x11, u[1]), u[2])
}

/// A well-mixed value for lattice cell `c`.
fn cell_hash(c: [i32; 3], seed: u32) -> u32 {
    use crate::shared::noise::pcg;
    // LINT-OK: hashing the bits of small non-negative cell indices.
    #[allow(clippy::cast_sign_loss)]
    let h = pcg(c[0] as u32 ^ pcg(c[1] as u32 ^ pcg(c[2] as u32 ^ seed)));
    h
}

/// A hash's low bits as a share of one.
fn unit(h: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // 24 bits, exact
    let u = (h & 0x00ff_ffff) as f32 / 16_777_216.0;
    u
}

/// The shaders' sources, each complete: the full-target triangle, the color conversions, the
/// shared atmosphere and the light of its air, what the heavens share, and the shader's own
/// code. The clouds first, then the heavens over the sky.
#[must_use]
pub fn shader_sources() -> [(&'static str, String); 2] {
    use crate::shared::fullscreen::{COLOUR_WGSL, FULLSCREEN_WGSL};
    let with = |own: &str| {
        [
            FULLSCREEN_WGSL,
            COLOUR_WGSL,
            super::ATMOSPHERE_WGSL,
            super::AIR_WGSL,
            HEAVENS_WGSL,
            own,
        ]
        .join("\n")
    };
    [
        ("clouds", with(CLOUDS_WGSL)),
        ("heavens", with(CELESTIAL_WGSL)),
    ]
}

/// The format the clouds are worked out in: linear light over them, premultiplied, and how
/// much of what lies behind them they hide.
const CLOUD_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The format of the clouds' shadow map: the share of the sun let through.
pub const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

/// The size the clouds are worked out at for a world `width` by `height` pixels, `share` of it
/// each way.
#[must_use]
pub fn cloud_size(width: u32, height: u32, share: f32) -> (u32, u32) {
    let s = share.clamp(0.1, 1.0);
    // LINT-OK: pixel counts, rounded up.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let at = |n: u32| ((n as f32 * s).ceil() as u32).max(1);
    (at(width), at(height))
}

/// A moon's ball: the unit ball's points, x east, y north and z toward the world it circles,
/// [`BALL_SEGMENTS`] round and from pole to pole, and its triangles.
#[must_use]
pub fn ball() -> (Vec<[f32; 3]>, Vec<u16>) {
    let (round, poles) = BALL_SEGMENTS;
    let mut points = Vec::with_capacity(usize::from(round + 1) * usize::from(poles + 1));
    for j in 0..=poles {
        let lat =
            std::f32::consts::FRAC_PI_2 - std::f32::consts::PI * f32::from(j) / f32::from(poles);
        for i in 0..=round {
            let lon = std::f32::consts::TAU * f32::from(i) / f32::from(round);
            let (s, c) = dereth_primitives::num::math::sin_cosf(lat);
            let (sl, cl) = dereth_primitives::num::math::sin_cosf(lon);
            points.push([c * sl, s, c * cl]);
        }
    }
    let mut triangles = Vec::with_capacity(usize::from(round) * usize::from(poles) * 6);
    let at = |i: u16, j: u16| j * (round + 1) + i;
    for j in 0..poles {
        for i in 0..round {
            let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1));
            triangles.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    (points, triangles)
}

/// An art pack's picture of a moon on the device, all its levels.
fn moon_picture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &HifiPackImage,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sky moon"),
        size: wgpu::Extent3d {
            width: image.width.max(1),
            height: image.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: u32::try_from(image.levels.len().max(1)).unwrap_or(1),
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: pack_image::format(image, pack_image::samples_blocks(device)),
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    pack_image::write(queue, &texture, 0, image, image.levels.len());
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// A moon's pictures on the device: its ground, its lights and its clouds.
#[derive(Debug)]
struct MoonPictures {
    object: u32,
    ground: wgpu::TextureView,
    lamps: Option<wgpu::TextureView>,
    veil: Option<wgpu::TextureView>,
}

/// A device texture viewed as one flat picture with all its levels.
fn picture_view(t: &wgpu::Texture) -> wgpu::TextureView {
    t.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2),
        base_array_layer: 0,
        array_layer_count: Some(1),
        ..wgpu::TextureViewDescriptor::default()
    })
}

/// A bind group's entry for `binding`.
fn entry(binding: u32, resource: wgpu::BindingResource<'_>) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry { binding, resource }
}

/// The heavens' device objects.
#[derive(Debug)]
pub(super) struct HeavensGpu {
    format: wgpu::TextureFormat,
    pub(super) constants: wgpu::Buffer,
    cloud_layout: wgpu::BindGroupLayout,
    celestial_layout: wgpu::BindGroupLayout,
    clouds: wgpu::RenderPipeline,
    shadow: wgpu::RenderPipeline,
    celestial: wgpu::RenderPipeline,
    moons: wgpu::RenderPipeline,
    cloud_cover: wgpu::RenderPipeline,
    /// The moons' ball: its points, its triangles and how many corners they have.
    ball: (wgpu::Buffer, wgpu::Buffer, u32),
    /// The art pack's pictures of the moons on the device, and the pack they are of.
    moon_pictures: Option<(u64, Vec<MoonPictures>)>,
    detail: wgpu::TextureView,
    wrap: wgpu::Sampler,
    blank: wgpu::TextureView,
    /// The clouds' own picture, and its size.
    target: Option<((u32, u32), wgpu::TextureView)>,
    /// Retail's pictures in use: each slot's texture as it was viewed.
    pictures: Vec<(u32, wgpu::Texture, wgpu::TextureView)>,
    /// The clouds' bind group and the views it was made over.
    cloud_bind: Option<(Vec<wgpu::TextureView>, wgpu::BindGroup)>,
    /// The heavens' bind group and the views it was made over.
    celestial_bind: Option<(Vec<wgpu::TextureView>, wgpu::BindGroup)>,
    /// How many times the bind groups were made.
    pub(super) binds_made: u64,
}

/// One texture binding of a fragment shader.
fn texture_entry(binding: u32, dim: wgpu::TextureViewDimension) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: dim,
            multisampled: false,
        },
        count: None,
    }
}

/// One uniform binding of a vertex and fragment shader, `size` bytes.
fn uniform_entry(binding: u32, size: usize) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(size as u64),
        },
        count: None,
    }
}

/// One filtering sampler binding of a fragment shader.
fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

/// A one-texel picture of nothing, for a picture the frame does not have.
fn blank_view(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let size = wgpu::Extent3d {
        width: 1,
        height: 1,
        depth_or_array_layers: 1,
    };
    let blank = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sky no picture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &blank,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[0, 0, 0, 0],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        size,
    );
    blank.create_view(&wgpu::TextureViewDescriptor::default())
}

/// The clouds' fine detail on the device, every level of it.
fn detail_view(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let levels = detail_noise();
    let detail = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sky cloud detail"),
        size: wgpu::Extent3d {
            width: DETAIL_SIZE,
            height: DETAIL_SIZE,
            depth_or_array_layers: DETAIL_SIZE,
        },
        mip_level_count: u32::try_from(levels.len()).unwrap_or(1),
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, bytes) in levels.iter().enumerate() {
        let side = (DETAIL_SIZE >> level).max(1);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &detail,
                mip_level: u32::try_from(level).unwrap_or(0),
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(side),
                rows_per_image: Some(side),
            },
            wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: side,
            },
        );
    }
    detail.create_view(&wgpu::TextureViewDescriptor::default())
}

impl HeavensGpu {
    #[allow(clippy::too_many_lines)]
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let [(_, clouds_src), (_, celestial_src)] = shader_sources();
        let module = |label: &str, src: &str| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(src.into()),
            })
        };
        let clouds_module = module("sky clouds", &clouds_src);
        let celestial_module = module("sky heavens", &celestial_src);
        let d2 = wgpu::TextureViewDimension::D2;
        let shared = [
            uniform_entry(0, super::model::CONSTANTS_SIZE),
            texture_entry(1, d2),
            sampler_entry(2),
            uniform_entry(3, CONSTANTS_SIZE),
        ];
        let layout = |label: &str, own: &[wgpu::BindGroupLayoutEntry]| {
            let entries: Vec<_> = shared.iter().chain(own).cloned().collect();
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &entries,
            })
        };
        let cloud_layout = layout(
            "sky clouds",
            &[
                texture_entry(4, wgpu::TextureViewDimension::D3),
                sampler_entry(5),
                texture_entry(6, d2),
            ],
        );
        let celestial_layout = layout(
            "sky heavens",
            &[
                texture_entry(4, d2),
                sampler_entry(5),
                texture_entry(6, d2),
                texture_entry(7, d2),
                texture_entry(8, d2),
                texture_entry(9, d2),
                texture_entry(10, d2),
                texture_entry(11, d2),
            ],
        );
        let pipeline = |label: &str,
                        module: &wgpu::ShaderModule,
                        group: &wgpu::BindGroupLayout,
                        entry: &str,
                        target: wgpu::ColorTargetState,
                        depth: Option<wgpu::DepthStencilState>| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(group)],
                immediate_size: 0,
            });
            device.build_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some("vs_fullscreen"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: depth,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(target)],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let clouds = pipeline(
            "sky clouds",
            &clouds_module,
            &cloud_layout,
            "fs_clouds",
            wgpu::ColorTargetState {
                format: CLOUD_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            },
            None,
        );
        let shadow = pipeline(
            "sky cloud shadows",
            &clouds_module,
            &cloud_layout,
            "fs_cloud_shadow",
            wgpu::ColorTargetState {
                format: SHADOW_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            },
            None,
        );
        // Drawn inside the world replay, in the sky pass, as the sky's layers are: premultiplied
        // over the sky, leaving the target's alpha alone and touching no depth.
        let over = wgpu::ColorTargetState {
            format,
            blend: Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Zero,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
            write_mask: wgpu::ColorWrites::ALL,
        };
        let no_depth = wgpu::DepthStencilState {
            format: crate::SidecarContext::depth_format(),
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };
        let celestial = pipeline(
            "sky heavens",
            &celestial_module,
            &celestial_layout,
            "fs_heavens",
            over.clone(),
            Some(no_depth.clone()),
        );
        let cloud_cover = pipeline(
            "sky cloud cover",
            &celestial_module,
            &celestial_layout,
            "fs_cloud_cover",
            over.clone(),
            Some(no_depth.clone()),
        );
        let moons = {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("sky moons"),
                bind_group_layouts: &[Some(&celestial_layout)],
                immediate_size: 0,
            });
            device.build_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("sky moons"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &celestial_module,
                    entry_point: Some("vs_moon"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: 12,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                    })],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(no_depth),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &celestial_module,
                    entry_point: Some("fs_moon"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(over)],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let ball = {
            use wgpu::util::DeviceExt;
            let (points, triangles) = ball();
            let points: Vec<u8> = points
                .iter()
                .flatten()
                .flat_map(|f| f.to_le_bytes())
                .collect();
            let corners: Vec<u8> = triangles.iter().flat_map(|i| i.to_le_bytes()).collect();
            (
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("sky moon ball"),
                    contents: &points,
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("sky moon ball"),
                    contents: &corners,
                    usage: wgpu::BufferUsages::INDEX,
                }),
                u32::try_from(triangles.len()).unwrap_or(0),
            )
        };
        let constants = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky heavens"),
            size: CONSTANTS_SIZE as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let wrap = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sky clouds"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });
        Self {
            format,
            constants,
            cloud_layout,
            celestial_layout,
            clouds,
            shadow,
            celestial,
            moons,
            cloud_cover,
            ball,
            moon_pictures: None,
            detail: detail_view(device, queue),
            wrap,
            blank: blank_view(device, queue),
            target: None,
            pictures: Vec::new(),
            cloud_bind: None,
            celestial_bind: None,
            binds_made: 0,
        }
    }

    /// The format the heavens are drawn into.
    pub(super) fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// The view of retail's picture in `slot` this frame, made again when the slot holds
    /// another texture.
    fn picture(
        &mut self,
        seam: &crate::SidecarContext<'_>,
        slot: Option<u32>,
    ) -> Option<wgpu::TextureView> {
        let slot = slot?;
        let t = seam.texture(dereth_render::device::TextureSlot(slot))?;
        if let Some((_, _, v)) = self
            .pictures
            .iter()
            .find(|(s, held, _)| *s == slot && held == t)
        {
            return Some(v.clone());
        }
        let view = picture_view(t);
        self.pictures.retain(|(s, _, _)| *s != slot);
        self.pictures.push((slot, t.clone(), view.clone()));
        // Only the few pictures the sky uses are kept.
        if self.pictures.len() > 8 {
            self.pictures.remove(0);
        }
        Some(view)
    }

    /// Ready the frame's targets and bind groups: the clouds' picture `size`, retail's pictures
    /// `plan` names, the art `pack`'s pictures of the moons it names, and the sky's constants
    /// `atmosphere`, its view table `sky_view` and its `sampler`.
    pub(super) fn prepare(
        &mut self,
        seam: &crate::SidecarContext<'_>,
        plan: &Plan,
        pack: Option<&HifiArtPack>,
        size: (u32, u32),
        (atmosphere, sky_view, sampler): (&wgpu::Buffer, &wgpu::TextureView, &wgpu::Sampler),
    ) {
        let device = seam.device().clone();
        if self.target.as_ref().is_none_or(|(s, _)| *s != size) {
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("sky clouds"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: CLOUD_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.target = Some((size, t.create_view(&wgpu::TextureViewDescriptor::default())));
        }
        let deck = self
            .picture(seam, plan.deck.and_then(|d| d.texture))
            .unwrap_or_else(|| self.blank.clone());
        if let Some(pack) = pack {
            if self
                .moon_pictures
                .as_ref()
                .is_none_or(|(s, _)| *s != pack.serial)
            {
                let queue = seam.queue();
                let made = pack
                    .moons
                    .iter()
                    .map(|m| MoonPictures {
                        object: m.object,
                        ground: moon_picture(&device, queue, &m.surface),
                        lamps: m.lights.as_ref().map(|i| moon_picture(&device, queue, i)),
                        veil: m.clouds.as_ref().map(|i| moon_picture(&device, queue, i)),
                    })
                    .collect();
                self.moon_pictures = Some((pack.serial, made));
            }
        }
        // Each moon's ground, lights and clouds, in the order the shader binds them.
        let mut moons: [wgpu::TextureView; 6] = std::array::from_fn(|_| self.blank.clone());
        for (k, m) in plan.moons.iter().enumerate().take(MOONS) {
            let Some(p) = self
                .moon_pictures
                .as_ref()
                .and_then(|(_, all)| all.iter().find(|p| p.object == m.gfx_id))
            else {
                continue;
            };
            moons[k] = p.ground.clone();
            if let Some(v) = &p.lamps {
                moons[2 + k] = v.clone();
            }
            if let Some(v) = &p.veil {
                moons[4 + k] = v.clone();
            }
        }
        let Some(clouds) = self.target.as_ref().map(|(_, v)| v.clone()) else {
            return;
        };
        let shared = [
            entry(0, atmosphere.as_entire_binding()),
            entry(1, wgpu::BindingResource::TextureView(sky_view)),
            entry(2, wgpu::BindingResource::Sampler(sampler)),
            entry(3, self.constants.as_entire_binding()),
        ];
        let cloud_key = vec![sky_view.clone(), deck.clone()];
        if self
            .cloud_bind
            .as_ref()
            .is_none_or(|(k, _)| *k != cloud_key)
        {
            let mut entries = shared.to_vec();
            entries.push(entry(4, wgpu::BindingResource::TextureView(&self.detail)));
            entries.push(entry(5, wgpu::BindingResource::Sampler(&self.wrap)));
            entries.push(entry(6, wgpu::BindingResource::TextureView(&deck)));
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sky clouds"),
                layout: &self.cloud_layout,
                entries: &entries,
            });
            self.cloud_bind = Some((cloud_key, bind));
            self.binds_made += 1;
        }
        let mut celestial_key = vec![sky_view.clone(), clouds.clone()];
        celestial_key.extend(moons.iter().cloned());
        if self
            .celestial_bind
            .as_ref()
            .is_none_or(|(k, _)| *k != celestial_key)
        {
            let mut entries = shared.to_vec();
            entries.push(entry(4, wgpu::BindingResource::TextureView(&clouds)));
            entries.push(entry(5, wgpu::BindingResource::Sampler(&self.wrap)));
            for (b, v) in (6..).zip(&moons) {
                entries.push(entry(b, wgpu::BindingResource::TextureView(v)));
            }
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sky heavens"),
                layout: &self.celestial_layout,
                entries: &entries,
            });
            self.celestial_bind = Some((celestial_key, bind));
            self.binds_made += 1;
        }
    }

    /// Work the clouds out into their own picture when `drawn`, or leave it clear; whether the
    /// picture was ready to draw into.
    pub(super) fn encode_clouds(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
        drawn: bool,
    ) -> bool {
        let (Some((_, target)), Some((_, bind))) = (self.target.as_ref(), self.cloud_bind.as_ref())
        else {
            return false;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sky clouds"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: timestamps,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if drawn {
            pass.set_pipeline(&self.clouds);
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..3, 0..1);
        }
        true
    }

    /// Work the clouds' shadows out into `target`; whether they were.
    pub(super) fn encode_shadow(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) -> bool {
        let Some((_, bind)) = self.cloud_bind.as_ref() else {
            return false;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sky cloud shadows"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: timestamps,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.shadow);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
        true
    }

    /// Draw the heavens into the open sky pass: the night layer, the stars and the sun, then the
    /// moons' balls (each one's ground, then its clouds), then the clouds over them all; whether
    /// they were drawn.
    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) -> bool {
        let Some((_, bind)) = self.celestial_bind.as_ref() else {
            return false;
        };
        pass.set_pipeline(&self.celestial);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
        let (points, triangles, corners) = &self.ball;
        pass.set_pipeline(&self.moons);
        pass.set_vertex_buffer(0, points.slice(..));
        pass.set_index_buffer(triangles.slice(..), wgpu::IndexFormat::Uint16);
        #[allow(clippy::cast_possible_truncation)] // two moons
        pass.draw_indexed(0..*corners, 0, 0..2 * MOONS as u32);
        pass.set_pipeline(&self.cloud_cover);
        pass.draw(0..3, 0..1);
        true
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the sky, an art-pack presentation's own)
    use super::*;

    #[test]
    fn a_moons_ball_is_the_whole_unit_ball_its_triangles_closing_over_it() {
        let (points, triangles) = ball();
        let (round, poles) = BALL_SEGMENTS;
        assert_eq!(
            points.len(),
            usize::from(round + 1) * usize::from(poles + 1)
        );
        assert_eq!(triangles.len(), usize::from(round) * usize::from(poles) * 6);
        for p in &points {
            let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!((r - 1.0).abs() < 1e-5, "{p:?} lies on the unit ball");
        }
        assert!(triangles.iter().all(|&i| usize::from(i) < points.len()));
        // The triangles cover the ball's whole surface: their areas sum to its own.
        let area: f32 = triangles
            .chunks(3)
            .map(|t| {
                let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(points[usize::from(t[k])]));
                (b - a).cross(c - a).length() * 0.5
            })
            .sum();
        let whole = 4.0 * std::f32::consts::PI;
        assert!((area / whole - 1.0).abs() < 0.01, "{area} of {whole}");
        // The ball's face toward the world, longitude 0, is its +z.
        assert!(points.iter().any(|p| p[2] > 0.999 && p[1].abs() < 1e-3));
    }
}
