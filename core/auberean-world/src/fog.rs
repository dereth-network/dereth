//! Fog blocks: the region's per-landblock fog and light, blended over the day's by where the
//! viewer stands in its block.
//!
//! The region's fog description has a block map of fog kinds read at the four corners of the
//! viewer's block (the block and its north, east and north-east neighbors). When any corner's kind
//! has fog, each corner combines its kind's light and fog with the day's, channel by channel as
//! the kind says (multiply, add, the day's, the kind's, the greater, the lesser), and the four are
//! blended by the viewer's place in the block. A corner's fog distances and sky fog are always its
//! kind's own (the default kind's are 0 to 10,000 m, no sky fog).
//!
//! Under water the fog is the water's: its kind's table of depths, each combined with the light
//! above as a fog kind is, blended between the two depths about the viewer's ([`water_fog`]).

use auberean_assets::region::{BlockMap, Combine, FogAndLight, FogDesc, FogType, WaterLight};
use auberean_primitives::math::Rgba;
use auberean_primitives::Vec3;

use crate::color::Color;
use crate::sky::DayLight;
use crate::water::block_map_value;

/// One corner's light and fog after combining with the day's.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Corner {
    dir_bright: f32,
    dir_color: Color,
    amb_bright: f32,
    amb_color: Color,
    fog_min: f32,
    fog_max: f32,
    fog_sky: f32,
    fog_color: Color,
}

/// One channel combined: the day's value `a` with the kind's `b`.
// D2-10868
fn combine(c: &Combine, a: f32, b: f32) -> f32 {
    match c.mode {
        1 => b * c.second + a * c.first,
        2 => a,
        3 => b,
        4 => a.max(b),
        5 => a.min(b),
        _ => a * b,
    }
}

// D2-10869
fn combine_color(c: &Combine, a: Color, b: Color) -> Color {
    Color::new(
        combine(c, a.r, b.r),
        combine(c, a.g, b.g),
        combine(c, a.b, b.b),
        combine(c, a.a, b.a),
    )
}

/// The day's light as one color: the direct color times the sun's brightness plus the ambient
/// color times its brightness, scaled down by its greatest channel when any leaves 0 to 1.
// D2-10870
fn day_light_color(l: &DayLight) -> Color {
    let (d, a) = (l.direct_color, l.ambient_color);
    let mut c = Rgba {
        r: d.r * l.direct_brightness + a.r * l.ambient_brightness,
        g: d.g * l.direct_brightness + a.g * l.ambient_brightness,
        b: d.b * l.direct_brightness + a.b * l.ambient_brightness,
        a: d.a,
    };
    if !c.is_valid() {
        c.normalize();
    }
    Color::new(c.r, c.g, c.b, c.a)
}

/// A corner's kind combined with the day: the combines are ambient brightness, ambient color,
/// direct brightness, direct color, the day light, the day's fog and the distance fog.
// D2-10871
fn corner(l: &DayLight, k: &FogAndLight) -> Corner {
    let c = &k.combines;
    let day_fog = combine_color(&c[5], day_light_color(l), l.fog_color);
    Corner {
        dir_bright: combine(&c[2], l.direct_brightness, k.direct_brightness),
        dir_color: combine_color(&c[3], l.direct_color, Color::from_argb(k.direct_color.0)),
        amb_bright: combine(&c[0], l.ambient_brightness, k.ambient_brightness),
        amb_color: combine_color(&c[1], l.ambient_color, Color::from_argb(k.ambient_color.0)),
        fog_min: k.fog_min,
        fog_max: k.fog_max,
        fog_sky: k.fog_sky,
        fog_color: combine_color(&c[6], day_fog, Color::from_argb(k.fog_color.0)),
    }
}

/// The fog kind of index `kind`: the description's kind with that index, else its default.
// D2-07539
fn fog_type(desc: &FogDesc, kind: u32) -> Option<&FogType> {
    desc.fogs
        .iter()
        .find(|f| f.index == kind)
        .or(desc.default_fog.as_ref())
}

/// The light and fog at `at` (block-local) in block `(x, y)`: the day's, or, when any of the
/// block's four corners has a fog kind with fog, the four corners combined with the day's and
/// blended by `at`.
// D2-10874, D2-10875, D2-10867
#[must_use]
pub fn block_light(
    day: &DayLight,
    desc: &FogDesc,
    kinds: &BlockMap,
    block: (i32, i32),
    at: Vec3,
    block_length: f32,
) -> DayLight {
    let (Ok(x), Ok(y)) = (u32::try_from(block.0), u32::try_from(block.1)) else {
        return *day;
    };
    let kind = |dx: u32, dy: u32| u32::from(block_map_value(kinds, x + dx, y + dy));
    let [sw, nw, ne, se] =
        [kind(0, 0), kind(0, 1), kind(1, 1), kind(1, 0)].map(|k| fog_type(desc, k));
    // D2-07540, D2-12531
    let has_fog = |t: Option<&FogType>| t.is_some_and(|t| t.light.fog_kind != 0);
    if ![sw, nw, ne, se].into_iter().any(has_fog) {
        return *day;
    }
    // D2-07541, D2-12532
    let Some([sw, nw, ne, se]) = [sw, nw, ne, se]
        .into_iter()
        .map(|t| t.map(|t| corner(day, &t.light)))
        .collect::<Option<Vec<_>>>()
        .and_then(|v| <[Corner; 4]>::try_from(v).ok())
    else {
        return *day;
    };
    let u = (at.x / block_length).clamp(0.0, 1.0);
    let v = (at.y / block_length).clamp(0.0, 1.0);
    let w = [(1.0 - u) * (1.0 - v), (1.0 - u) * v, u * v, u * (1.0 - v)];
    let cs = [sw, nw, ne, se];
    let mix = |f: &dyn Fn(&Corner) -> f32| cs.iter().zip(w).map(|(c, w)| f(c) * w).sum::<f32>();
    let mix_color = |f: &dyn Fn(&Corner) -> Color, alpha: f32| {
        Color::new(mix(&|c| f(c).r), mix(&|c| f(c).g), mix(&|c| f(c).b), alpha)
    };
    DayLight {
        direct_brightness: mix(&|c| c.dir_bright),
        direct_color: mix_color(&|c| c.dir_color, sw.dir_color.a),
        ambient_brightness: mix(&|c| c.amb_bright),
        ambient_color: mix_color(&|c| c.amb_color, sw.amb_color.a),
        fog_min: mix(&|c| c.fog_min),
        fog_max: mix(&|c| c.fog_max),
        fog_sky: mix(&|c| c.fog_sky),
        fog_color: mix_color(&|c| c.fog_color, sw.fog_color.a),
        ..*day
    }
}

/// The light `light` (the day's, with the fog blocks') under water `depth` meters down, by the
/// water's table of depths (`table`, shallowest first): the fog alone changes, the light stays.
/// The table's entries about the depth are found as the client finds them: the last entry deeper
/// than the depth, and the one before it (the first when it is the first); with none deeper, the
/// last alone. Each is combined with `light` as a fog kind is, and the two are blended by where
/// the depth lies between theirs (the shallower's share growing as the depth nears its own); the
/// fog's alpha is the deeper's. An empty table leaves the light as it is.
// D2-10872, D2-07527, D2-12555, D2-12560
#[must_use]
pub fn water_fog(light: &DayLight, table: &[WaterLight], depth: f32) -> DayLight {
    let Some(last) = table.len().checked_sub(1) else {
        return *light;
    };
    let mut deeper = None;
    for (i, w) in table.iter().enumerate() {
        if depth < w.depth {
            deeper = Some(i);
        }
    }
    let (lo, hi) = deeper.map_or((last, last), |i| (i.saturating_sub(1), i));
    let a = corner(light, &table[lo].light);
    let mut out = *light;
    if lo == hi {
        out.fog_min = a.fog_min;
        out.fog_max = a.fog_max;
        out.fog_sky = a.fog_sky;
        out.fog_color = a.fog_color;
        return out;
    }
    let b = corner(light, &table[hi].light);
    let f = (depth - table[hi].depth) / (table[lo].depth - table[hi].depth);
    let g = 1.0 - f;
    out.fog_min = a.fog_min * f + b.fog_min * g;
    out.fog_max = a.fog_max * f + b.fog_max * g;
    out.fog_sky = a.fog_sky * f + b.fog_sky * g;
    out.fog_color = Color::new(
        a.fog_color.r * f + b.fog_color.r * g,
        a.fog_color.g * f + b.fog_color.g * g,
        a.fog_color.b * f + b.fog_color.b * g,
        b.fog_color.a,
    );
    out
}
