//! The time of day and what follows from it: which day the sky shows, the light and fog of the
//! moment, the sky's objects and where they stand, and the terrain's lit vertex colors.

use auberean_assets::region::{DayDesc, GameTime, SkyDesc, SkyObject, SkyTime};
use auberean_primitives::{DataId, Quat, Vec3};
use dereth_primitives::num::math;

use crate::color::{lerp_angle_closest, lerp_color, Color};
use crate::math::{dot, quat_axis_angle, quat_mul};

/// The calendar position of a game time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DayClock {
    pub year: i64,
    /// The day of the year, from 0.
    pub day: i64,
    /// How far through the day, `[0, 1)`.
    pub fraction: f32,
}

/// Where game time `t` (seconds) falls in the calendar.
// D2-13060
#[must_use]
pub fn day_clock(cal: &GameTime, t: f64) -> DayClock {
    let day_length = f64::from(cal.day_length);
    let year_length = day_length * f64::from(cal.days_per_year);
    let d = t + cal.zero_time;
    #[allow(clippy::cast_possible_truncation)] // a calendar's years and days fit easily
    let year = (d / year_length).floor() as i64;
    let rem = d - year as f64 * year_length;
    #[allow(clippy::cast_possible_truncation)]
    let day = (rem / day_length).floor() as i64;
    #[allow(clippy::cast_possible_truncation)]
    let fraction = ((rem - day as f64 * day_length) / day_length) as f32;
    DayClock {
        year: year + i64::from(cal.zero_year),
        day,
        fraction,
    }
}

/// The day the sky shows on calendar day `n = year * days_per_year + day`: a hash of `n` scaled to
/// the days' frequency total, matched against their running total; the last day when none
/// exceeds it.
// D2-07457, D2-07458
#[must_use]
pub fn day_for(sky: &SkyDesc, n: i64) -> Option<DataId> {
    let last = sky.days.len().checked_sub(1)?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // wrapping 32-bit arithmetic
    let n = n as u32;
    let r = (n
        .wrapping_mul(0x3FBB_657F)
        .wrapping_sub(0x6B3C_5F11)
        .wrapping_mul(n)
        .wrapping_add(0x0719_BAF0))
        >> 16;
    let total: f32 = sky.days.iter().map(|d| d.frequency).sum();
    #[allow(clippy::cast_precision_loss)] // sixteen bits
    let threshold = r as f32 * total / 65_536.0;
    let mut acc = 0.0;
    for d in &sky.days[..last] {
        acc += d.frequency;
        if acc > threshold {
            return Some(d.day);
        }
    }
    Some(sky.days[last].day)
}

/// An interval `share` of the way from the longest to the shortest: the shortest below a share of
/// 0, the longest above a share of 1 (the client's own test lets a share of exactly 1 through to
/// the sum, which gives the shortest).
fn interval(shortest: f64, longest: f64, share: f32) -> f64 {
    if share < 0.0 {
        shortest
    } else if share <= 1.0 {
        longest - (longest - shortest) * f64::from(share)
    } else {
        longest
    }
}

/// The seconds of game time between two updates of the sky's objects, `share` of the way between
/// the sky's two intervals (the client asks for the middle, a share of one half); 0 in the
/// September 2013 data: every frame.
// D2-07452
#[must_use]
pub fn tick_size(sky: &SkyDesc, share: f32) -> f64 {
    interval(sky.min_tick, sky.max_tick, share)
}

/// The seconds of game time between two updates of the world's light, as [`tick_size`]: 30 in
/// the September 2013 data.
// D2-07453
#[must_use]
pub fn light_tick_size(sky: &SkyDesc, share: f32) -> f64 {
    interval(sky.min_light_tick, sky.max_light_tick, share)
}

/// The rain of day `day` at fraction `t`: its amount and the wind's two components, each carried
/// from the time of day before `t` to the one after by how far `t` is between them; none with no
/// times of day.
// D2-07583
#[must_use]
pub fn rain(day: &DayDesc, t: f32) -> (f32, f32, f32) {
    let Some((b, a, r)) = bracket(&day.times, t) else {
        return (0.0, 0.0, 0.0);
    };
    let (tb, ta) = (&day.times[b], &day.times[a]);
    let lerp = |x: f32, y: f32| (y - x) * r + x;
    (
        lerp(tb.rain_amount, ta.rain_amount),
        lerp(tb.rain_wind_x, ta.rain_wind_x),
        lerp(tb.rain_wind_y, ta.rain_wind_y),
    )
}

/// The first of a time of day's sky-object replacements that names object `name`.
// D2-12523
#[must_use]
pub fn find_replacement(
    time: &SkyTime,
    name: u32,
) -> Option<&auberean_assets::region::SkyReplacement> {
    time.replacements.iter().find(|x| x.object == name)
}

/// The two times of day around `t` and how far `t` is from the first to the second.
// D2-07579, D2-07580, D2-07571
#[must_use]
pub fn bracket(times: &[SkyTime], t: f32) -> Option<(usize, usize, f32)> {
    let n = times.len();
    let last = n.checked_sub(1)?;
    let before = (0..last)
        .find(|&i| times[i].begin <= t && t <= times[i + 1].begin)
        .unwrap_or(last);
    let after = (0..n).find(|&i| t < times[i].begin).unwrap_or(0);
    Some((
        before,
        after,
        time_ratio(times[before].begin, times[after].begin, t),
    ))
}

/// How far `t` is from `b` to `a`, going forward round the day.
#[must_use]
pub fn time_ratio(b: f32, a: f32, t: f32) -> f32 {
    if b == a {
        0.0
    } else if b < a {
        if b <= t && t <= a {
            (t - b) / (a - b)
        } else {
            0.5
        }
    } else {
        let mut d = t - b;
        if d < 0.0 {
            d += 1.0;
        }
        d / (a - b + 1.0)
    }
}

/// The light and fog of a moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DayLight {
    pub ambient_brightness: f32,
    pub ambient_color: Color,
    pub direct_brightness: f32,
    pub direct_color: Color,
    /// The direction light travels, unit length.
    pub light_direction: Vec3,
    pub fog_min: f32,
    pub fog_max: f32,
    /// How much the sky is fogged over, 0 to 1.
    pub fog_sky: f32,
    pub fog_color: Color,
    pub shadow_color: Color,
}

impl DayLight {
    /// The light of a day with no times of day: dim white ambient, a faint sun travelling east and
    /// down (its direction as the day keeps it, not made a unit), white shadows, fog out to 10,000.
    pub const DEFAULT: Self = Self {
        ambient_brightness: 0.7,
        ambient_color: Color::WHITE,
        direct_brightness: 0.1,
        direct_color: Color::WHITE,
        light_direction: Vec3 {
            x: 1.0,
            y: 0.0,
            z: -0.4,
        },
        fog_min: 0.0,
        fog_max: 10_000.0,
        fog_sky: 0.0,
        fog_color: Color::WHITE,
        shadow_color: Color::WHITE,
    };

    /// The light with no sky or no day at all: the same dim white ambient and faint white sun, but
    /// travelling up and east, so it reaches no ground.
    // D2-10905
    pub const NO_SKY: Self = Self {
        light_direction: Vec3 {
            x: 0.6,
            y: 0.0,
            z: 0.8,
        },
        ..Self::DEFAULT
    };

    /// The ambient light: its color times its brightness.
    // D2-10895
    #[must_use]
    pub fn ambient(&self) -> Color {
        let (c, k) = (self.ambient_color, self.ambient_brightness);
        Color::new(c.r * k, c.g * k, c.b * k, c.a)
    }

    /// The light vector: the direction light travels, as long as the sun is bright.
    #[must_use]
    pub fn light_vector(&self) -> Vec3 {
        let (d, k) = (self.light_direction, self.direct_brightness);
        Vec3::new(d.x * k, d.y * k, d.z * k)
    }
}

/// The direction toward the sun at heading `h` (degrees clockwise from north) and pitch `p`
/// (degrees above the horizon).
// D2-07574, D2-13123
#[must_use]
pub fn toward(h_deg: f32, p_deg: f32) -> Vec3 {
    let (h, p) = (h_deg.to_radians(), p_deg.to_radians());
    Vec3::new(
        math::sinf(h) * math::cosf(p),
        math::cosf(h) * math::cosf(p),
        math::sinf(p),
    )
}

/// The light and fog of day `day` at fraction `t`.
// D2-07581, D2-07582
#[must_use]
pub fn day_light(day: &DayDesc, t: f32) -> DayLight {
    let Some((b, a, r)) = bracket(&day.times, t) else {
        return DayLight::DEFAULT;
    };
    let (tb, ta) = (&day.times[b], &day.times[a]);
    let lerp = |x: f32, y: f32| x + (y - x) * r;
    let col = |x: auberean_assets::common::Color32, y: auberean_assets::common::Color32| {
        lerp_color(Color::from_argb(x.0), Color::from_argb(y.0), r)
    };
    let heading = lerp_angle_closest(tb.direct_heading, ta.direct_heading, r, 360.0);
    let pitch = lerp_angle_closest(tb.direct_pitch, ta.direct_pitch, r, 360.0);
    let sun = toward(heading, pitch);
    DayLight {
        ambient_brightness: lerp(tb.ambient_brightness, ta.ambient_brightness),
        ambient_color: col(tb.ambient_color, ta.ambient_color),
        direct_brightness: lerp(tb.direct_brightness, ta.direct_brightness),
        direct_color: col(tb.direct_color, ta.direct_color),
        light_direction: Vec3::new(-sun.x, -sun.y, -sun.z),
        fog_min: lerp(tb.fog_min, ta.fog_min),
        fog_max: lerp(tb.fog_max, ta.fog_max),
        fog_sky: lerp(tb.fog_sky, ta.fog_sky),
        fog_color: col(tb.fog_color, ta.fog_color),
        shadow_color: col(tb.shadow_color, ta.shadow_color),
    }
}

/// The nine directions of travel of the sun's light that the landscape's self-shadow bits were
/// made for, east to west; the middle one is no direction at all.
const SUN_DIRECTIONS: [[f32; 3]; 9] = [
    [-0.45, 0.13, -0.32],
    [-0.36, 0.12, -0.44],
    [-0.28, 0.12, -0.57],
    [-0.14, 0.06, -0.58],
    [0.0, 0.0, 0.0],
    [0.11, -0.05, -0.59],
    [0.22, -0.10, -0.60],
    [0.33, -0.11, -0.49],
    [0.40, -0.12, -0.38],
];

/// Which of a terrain vertex's eight self-shadow bits says whether light travelling along
/// `direction` reaches it: the bit of the nearest of the nine stored directions (the middle one
/// skipped in the numbering), or `None` when the nearest is no direction and every vertex is lit.
// D2-10923, D2-10945
#[must_use]
pub fn self_shadow_bit(direction: Vec3) -> Option<u32> {
    let mut best = (-1.0f32, 4usize);
    for (k, d) in SUN_DIRECTIONS.iter().enumerate() {
        let v = Vec3::new(d[0], d[1], d[2]);
        let len = dot(v, v).sqrt();
        let n = if len > 0.0 { len } else { 1.0 };
        let c = dot(Vec3::new(v.x / n, v.y / n, v.z / n), direction);
        if c > best.0 {
            best = (c, k);
        }
    }
    match best.1 {
        4 => None,
        k if k > 4 => u32::try_from(k - 1).ok(),
        k => u32::try_from(k).ok(),
    }
}

/// The two of the nine stored directions nearest `direction` (the direction light travels), as
/// places in the stored list (the middle one counted), and the cosine between `direction` and the
/// nearest: each stored direction is made a unit and the greatest cosine wins, the first of equals;
/// the second is the greatest of the others. A place is `None` when no cosine passes -1.
// D2-10923
#[must_use]
pub fn sun_directions(direction: Vec3) -> (Option<usize>, Option<usize>, f32) {
    let cosines: Vec<f32> = SUN_DIRECTIONS
        .iter()
        .map(|d| {
            let v = Vec3::new(d[0], d[1], d[2]);
            let len = dot(v, v).sqrt();
            let n = if len > 0.0 { len } else { 1.0 };
            dot(Vec3::new(v.x / n, v.y / n, v.z / n), direction)
        })
        .collect();
    let mut best = (-1.0f32, None);
    for (k, &c) in cosines.iter().enumerate() {
        if c > best.0 {
            best = (c, Some(k));
        }
    }
    let mut second = (-1.0f32, None);
    for (k, &c) in cosines.iter().enumerate() {
        if Some(k) != best.1 && c > second.0 {
            second = (c, Some(k));
        }
    }
    (best.1, second.1, best.0)
}

/// Whether the sun reaches a terrain vertex with self-shadow bits `bits`: 1 when there is no bit
/// to read or its bit is set, else 0.
#[must_use]
pub fn sun_reaches(bits: u32, bit: Option<u32>) -> f32 {
    match bit {
        Some(b) if bits & (1 << b) == 0 => 0.0,
        _ => 1.0,
    }
}

/// A terrain vertex's lit color: ambient plus the sun's share by its normal, times the vertex's
/// own color, clamped. `shadow` (0 or 1) gates the sun.
// D2-09881, D2-10924
#[must_use]
pub fn lit_vertex(light: &DayLight, normal: Vec3, own: Color, shadow: f32) -> Color {
    let d = (-dot(normal, light.light_vector()) * shadow).max(0.0);
    let (a, c) = (light.ambient(), light.direct_color);
    Color::new(
        (a.r + d * c.r) * own.r,
        (a.g + d * c.g) * own.g,
        (a.b + d * c.b) * own.b,
        1.0,
    )
    .clamped()
}

/// One sky object as the moment shows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyPiece {
    /// Its object's place in the day's list.
    pub index: usize,
    /// A setup or visual description, drawn around the viewer.
    pub object: DataId,
    pub heading: f32,
    pub pitch: f32,
    pub color: Color,
    /// Drawn after the world rather than before it.
    pub after: bool,
    /// Its rotation: `R_Y(-pitch) · R_Z(-heading)` in the world's axes.
    pub rotation: Quat,
    /// Whether it is drawn with the camera under water: 0 always, 1 only then, anything else never.
    pub water_effect: u32,
}

impl SkyPiece {
    /// Whether it is drawn with the camera under water or not (no object of the September 2013
    /// data shows only under water).
    // D2-10942
    #[must_use]
    pub const fn shows(&self, in_water: bool) -> bool {
        self.water_effect == 0 || self.water_effect == in_water as u32
    }
}

/// Whether object `o` shows at `t`.
fn showing(o: &SkyObject, t: f32) -> bool {
    let (b, e) = (o.begin_time, o.end_time);
    b == e || (b <= t && t < e) || (e < b && (t < e || t > b))
}

/// An angle carried from `v0` to `v1` at `r`, the way the object travels.
fn travel(v0: f32, v1: f32, r: f32, west_to_east: bool) -> f32 {
    let (mut v0, mut v1) = (v0, v1);
    if !west_to_east && v1 < v0 {
        v0 -= 360.0;
    }
    if west_to_east && v0 < v1 {
        v1 -= 360.0;
    }
    let v = v0 + (v1 - v0) * r;
    if v < 0.0 {
        v + 360.0
    } else {
        v
    }
}

/// The sky's objects at fraction `t` of day `day`.
// D2-07600, D2-10937, D2-10943, D2-06003, D2-06004
#[must_use]
pub fn sky_pieces(day: &DayDesc, t: f32) -> Vec<SkyPiece> {
    let around = bracket(&day.times, t);
    let mut out = Vec::new();
    for (index, o) in day.objects.iter().enumerate() {
        let mut object = if showing(o, t) { o.mesh } else { DataId::NONE };
        let (mut heading, mut pitch) = if o.begin_time == o.end_time {
            (o.begin_heading, o.begin_pitch)
        } else {
            let r = time_ratio(o.begin_time, o.end_time, t);
            let wte = o.west_to_east != 0;
            (
                travel(o.begin_heading, o.end_heading, r, wte),
                travel(o.begin_pitch, o.end_pitch, r, wte),
            )
        };
        // D2-07570, D2-07596
        let mut color = Color::WHITE;
        if let Some((b, a, r)) = around {
            let find = |s: &SkyTime| find_replacement(s, o.name).copied();
            if let Some(rb) = find(&day.times[b]) {
                if !rb.mesh.is_none() {
                    object = rb.mesh;
                }
                heading += rb.heading_offset;
                pitch += rb.pitch_offset;
                let cb = Color::from_argb(rb.color.0);
                color = match find(&day.times[a]) {
                    Some(ra) => {
                        let ca = Color::from_argb(ra.color.0);
                        let mut c = lerp_color(cb, ca, r);
                        c.a = cb.a + (ca.a - cb.a) * r;
                        c
                    }
                    None => cb,
                };
            }
        }
        if object.is_none() {
            continue;
        }
        let mut q = Quat::IDENTITY;
        if heading != 0.0 {
            q = quat_axis_angle(Vec3::new(0.0, 0.0, 1.0), -heading.to_radians());
        }
        if pitch != 0.0 {
            q = quat_mul(
                quat_axis_angle(Vec3::new(0.0, 1.0, 0.0), -pitch.to_radians()),
                q,
            );
        }
        out.push(SkyPiece {
            index,
            object,
            heading,
            pitch,
            color,
            after: o.after != 0,
            rotation: q,
            water_effect: o.water_effect,
        });
    }
    out
}
