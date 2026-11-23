//! AC2's sky as AC1's: AC2's seven sky days written into the converted region as AC1 day groups,
//! drawn by AC1's sky as any AC1 sky is, and colored by the hour by the world's sky tints.
//!
//! **What maps.** Each AC2 day is an AC1 day group, its frequency the group's chance, named
//! "Rainy" when rain falls in it and "Clear" otherwise. Each AC2 sky object is an AC1 sky object
//! in the same order (AC1 draws them in order, as AC2 does): one shown all day is shown always; one
//! that crosses the sky keeps its window, and its pitch arc is AC1's angle arc, carried the way AC2
//! carries it; its heading is the heading every time of day gives it. Each AC2 time of day is an
//! AC1 time of day at the same fraction of the day: the sun's brightness, heading, pitch and color,
//! the ambient brightness and color, and the fog's distances and color, fog on. Each object's alpha
//! at each time is its transparency there, interpolated between the times as AC2 interpolates it.
//! AC1's day length stays AC1's.
//!
//! **The tints.** AC2 colors most of its sky objects by a tint for the time of day, which AC1's
//! region has no place for. The world's sky tints ([`dereth_assets::sky_tints`]) carry it: for
//! each day group, time of day and object, AC2's tint (its color; its alpha is the transparency
//! above), and how much AC2's fog veils the sky then. Each object's picture is its material's
//! texture stages worked out over the tints it is drawn in: the picture that, times each tint,
//! comes nearest to AC2's stages under that tint. Where the stages only multiply by the tint, as
//! the clouds' do, that is AC2's own color at every hour. Where they do not, as the dome's add the
//! picture signed to its tinted double, no one picture times AC2's tint comes near: the dome's
//! picture and its tint at each hour are then fitted together, and the tints carry the fitted tint
//! for it rather than AC2's. A client that does not read the tints draws each picture as it is, at
//! full brightness at every hour.
//!
//! **How far out.** AC2 draws its sky at the viewer, and its sky models are small: its clouds a
//! dish a few tens of meters across. AC1 draws its sky about the corner of the viewer's landblock,
//! at the ground's zero height, and its models lie far enough out that where the viewer stands
//! makes no difference. Each model is scaled out from its center until its nearest point is as far
//! as AC1's own sky models lie, its farthest kept inside AC1's sky; from its center it looks as it
//! did.
//!
//! **Rain** is AC1's own: AC1's two rain layers on an AC2 day that rains, as clear as AC2's rain is
//! light (none: not drawn).
//!
//! **Left out:** the object some AC2 days send across the sky (it plays an animation), the sun's
//! lens flare, AC2's fog by place, rain's strength and wind, and the shadow color.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use auberean_assets::material::{CoordinateAnimation, Material, MaterialLayer};
use auberean_assets::mesh::Mesh;
use auberean_assets::region::{DayDesc, SkyObject as Ac2Object, SkyTime as Ac2Time};
use auberean_draw::material::{
    instance_material, stage_uv_transform, sub_objects, DEVICE_FLAGS, HARDWARE_CLASS,
};
use auberean_world::math::Mat4;
use auberean_world::place::parts_of;
use auberean_world::records::WorldRecords;
use dereth_assets::region::{Region, SkyDayPreset, SkyObject, SkyObjectReplace, SkyTime};
use dereth_assets::sky_tints::{SkyTints, TintedDay, TintedTime};
use dereth_assets::{
    GfxObj, Polygon, RenderSurface, Surface, SurfaceTexture, SwVertex, VertexArray,
};
use dereth_primitives::{DataId, EraId, Vec3};

use crate::ac2_look::source::Ac2;
use crate::ac2_world::art::{Ids, Records};

/// The greatest side of a sky picture taken, texels: the sky fills the screen.
const PICTURE_LIMIT: u32 = 1024;
/// The image category the converter's model pictures carry.
const MODEL_IMAGE_CATEGORY: u32 = 4;
/// AC1's 32-bit picture format: blue, green, red, alpha.
const PFID_A8R8G8B8: u32 = 21;
/// AC1's surface kinds: a picture, blended by its alpha, added to what is behind it.
const SURFACE_IMAGE: u32 = 2;
const SURFACE_ALPHA: u32 = 0x100;
const SURFACE_ADDITIVE: u32 = 0x1_0000;
/// AC2's blends: the source's alpha, one, and one less the source's alpha.
const BLEND_ONE: u32 = 2;
const BLEND_SRC_ALPHA: u32 = 5;
const BLEND_INV_SRC_ALPHA: u32 = 6;
/// AC2's cull mode that draws both faces.
const CULL_NONE: u32 = 1;
/// An AC1 sky object drawn after the land, rather than before it.
const AFTER_THE_LAND: u32 = 1;
/// An AC1 sky object that is weather.
const WEATHER: u32 = 4;
/// The brightness every AC2 sky object is drawn at, as a percentage: its own picture's color.
const FULL: f32 = 100.0;
/// A tint that leaves its object's color as it is.
const WHITE: u32 = 0xFFFF_FFFF;

/// How near a sky model's nearest point lies once scaled out, meters: as near as AC1's own sky
/// models lie (its domes and lights lie 1000 to 2700 meters out).
const SKY_NEAR: f32 = 1500.0;
/// How far a sky model's farthest point may lie once scaled out, meters: inside AC1's own sky (its
/// cloud deck reaches 14272 meters, its sky drawn to 16000 at the default view distance).
const SKY_FAR: f32 = 14000.0;

/// AC2's sky as AC1 day groups and their tints, and a line per object of what was made.
pub struct Sky {
    pub groups: Vec<SkyDayPreset>,
    /// A day of tints for each group.
    pub tints: SkyTints,
    /// A line per AC2 sky object of each day: what it became.
    pub tsv: String,
    /// One line for the stats.
    pub line: String,
}

/// One AC2 sky object's model: its id, how its texture scrolls (units a second), and how it is
/// drawn.
#[derive(Clone, Debug)]
struct Model {
    gfx: DataId,
    velocity: (f32, f32),
    note: String,
    /// The tint drawn for each of AC2's, where its picture and tints were fitted together.
    retint: BTreeMap<u32, u32>,
}

/// AC2's sky, every day of it or only the `only`th (from 1), as AC1 day groups. Each sky object's
/// mesh is made an AC1 model with new ids from `ids`, its records added to `out`; AC1's rain is
/// taken from `ac1`'s own rainy days.
///
/// # Errors
/// When AC2's sky does not read, a day asked for is not there, or a record will not encode.
pub fn convert(
    ac2_dir: &Path,
    ac2: &Ac2,
    ac1: &Region,
    only: Option<usize>,
    ids: &mut Ids,
    out: &mut Records,
) -> Result<Sky, String> {
    let wr = crate::ac2_world::statics::world_records(ac2_dir)?;
    let rs = wr.region().map_err(|e| format!("AC2 region: {e:?}"))?;
    let desc = rs.sky.ok_or("AC2's region has no sky")?;
    let rain = ac1_rain(ac1)?;
    let mut days: Vec<(usize, DayDesc, f32)> = Vec::new();
    for (k, d) in desc.days.iter().enumerate() {
        if only.is_some_and(|n| n != k + 1) {
            continue;
        }
        let day = wr
            .day(d.day)
            .ok_or_else(|| format!("AC2 sky day {:08X} will not read", d.day.0))?;
        days.push((k + 1, day, d.frequency));
    }
    if days.is_empty() {
        return Err(format!("AC2's sky has no day {}", only.unwrap_or(0)));
    }
    // Every tint each mesh is drawn in, over every day, for its picture, with the most it shows in
    // it (its alpha there).
    let mut drawn_in: BTreeMap<u32, BTreeMap<u32, f32>> = BTreeMap::new();
    for (_, day, _) in &days {
        for o in &day.objects {
            let seen = drawn_in.entry(o.mesh.0).or_default();
            for t in &day.times {
                let c = color_of(t, o.name);
                let shows = f32::from(u8::try_from(c >> 24).unwrap_or(255)) / 255.0;
                let most = seen.entry(c | 0xFF00_0000).or_insert(0.0);
                *most = most.max(shows);
            }
        }
    }
    let mut models: BTreeMap<u32, Option<Model>> = BTreeMap::new();
    let mut tsv = String::from(
        "ac2_day\tac1_object\tac2_object\tac2_mesh\tac1_model\tshown\tturns\tscrolls\tdrawn\n",
    );
    let mut groups = Vec::new();
    let mut tints = SkyTints::default();
    let mut left = 0usize;
    for (n, day, frequency) in &days {
        let mut made = Vec::new();
        for o in &day.objects {
            if let std::collections::btree_map::Entry::Vacant(e) = models.entry(o.mesh.0) {
                let under: Vec<(u32, f32)> = drawn_in
                    .get(&o.mesh.0)
                    .map(|s| s.iter().map(|(c, a)| (*c, *a)).collect())
                    .unwrap_or_default();
                e.insert(model(&wr, ac2, o.mesh.0, &under, ids, out)?);
            }
            let Some(m) = models[&o.mesh.0].clone() else {
                left += 1;
                let _ = writeln!(
                    tsv,
                    "{n}\t-\t{:08X}\t{:08X}\t-\t-\t-\t-\tleft out: plays an animation, or draws nothing",
                    o.name, o.mesh.0
                );
                continue;
            };
            made.push((o.clone(), m));
        }
        let group = day_group(day, *frequency, &made, &rain);
        tints
            .days
            .push(tinted_day(day, &made, group.sky_objects.len()));
        for (k, (o, m)) in made.iter().enumerate() {
            let s = &group.sky_objects[k];
            let shown = if s.begin_time == s.end_time {
                "always".to_owned()
            } else {
                format!("{:.3}-{:.3}", s.begin_time, s.end_time)
            };
            let _ = writeln!(
                tsv,
                "{n}\t{k}\t{:08X}\t{:08X}\t{:08X}\t{shown}\t{} to {} at heading {}\t{:.4},{:.4}\t{}",
                o.name,
                o.mesh.0,
                m.gfx.0,
                s.begin_angle,
                s.end_angle,
                heading(o),
                m.velocity.0,
                m.velocity.1,
                m.note
            );
        }
        groups.push(group);
    }
    let line = format!(
        "sky: {} AC2 days as AC1 day groups ({}), {} sky models, {left} objects left out; AC1's rain on {} of them; tinted by the hour",
        groups.len(),
        groups
            .iter()
            .map(|g| g.day_name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        models.values().filter(|m| m.is_some()).count(),
        groups.iter().filter(|g| g.day_name == "Rainy").count(),
    );
    Ok(Sky {
        groups,
        tints,
        tsv,
        line,
    })
}

/// AC1's rain layers: the weather objects drawn as one model each (not a particle script) of
/// AC1's first rainy day, as AC1 shows them.
fn ac1_rain(ac1: &Region) -> Result<Vec<SkyObject>, String> {
    let sky = ac1.sky_info.as_ref().ok_or("AC1's region has no sky")?;
    let day = sky
        .day_groups
        .iter()
        .find(|g| dereth_assets::region::DayKind::Rainy.is(g))
        .ok_or("AC1's region has no rainy day")?;
    let rain: Vec<SkyObject> = day
        .sky_objects
        .iter()
        .filter(|o| {
            o.properties & WEATHER != 0
                && o.default_gfx_object.0 >> 24 == 0x01
                && o.default_pes_object.0 == 0
        })
        .map(|o| SkyObject {
            begin_time: 0.0,
            end_time: 0.0,
            ..o.clone()
        })
        .collect();
    if rain.is_empty() {
        return Err("AC1's rainy day has no rain layer".into());
    }
    Ok(rain)
}

/// The heading an AC2 object is drawn at: its own, which no shipped object changes over its window.
fn heading(o: &Ac2Object) -> f32 {
    o.begin_heading
}

/// An AC2 object's arc as AC1's angles: the pitch carried from its begin to its end the way AC2
/// carries it (westward, or eastward when it travels west to east), which AC1's straight
/// interpolation then follows.
fn arc(o: &Ac2Object) -> (f32, f32) {
    let (mut v0, mut v1) = (o.begin_pitch, o.end_pitch);
    if o.west_to_east == 0 && v1 < v0 {
        v0 -= 360.0;
    }
    if o.west_to_east != 0 && v0 < v1 {
        v1 -= 360.0;
    }
    (v0, v1)
}

/// Whether an AC2 object is shown all day: no window, or a window of the whole day.
fn always(o: &Ac2Object) -> bool {
    o.begin_time == o.end_time || (o.begin_time <= 0.0 && o.end_time >= 1.0)
}

/// An alpha as AC1's transparency, a percentage.
fn transparent(alpha: f32) -> f32 {
    (1.0 - alpha.clamp(0.0, 1.0)) * 100.0
}

/// AC2's tint of object `name` at time of day `t`, `0xAARRGGBB`: its replacement's color, white
/// where the time gives it none.
fn color_of(t: &Ac2Time, name: u32) -> u32 {
    auberean_world::sky::find_replacement(t, name).map_or(WHITE, |r| r.color.0)
}

/// A packed color's red, green and blue, 0 to 1.
fn rgb(c: u32) -> [f32; 3] {
    let t = dereth_assets::sky_tints::unpack(c);
    [t[0], t[1], t[2]]
}

/// An AC2 day as an AC1 day group: its objects (each with its model), then AC1's rain layers when
/// it rains; its times of day, each with a replacement for every object.
fn day_group(
    day: &DayDesc,
    frequency: f32,
    made: &[(Ac2Object, Model)],
    rain: &[SkyObject],
) -> SkyDayPreset {
    let rains = day.times.iter().any(|t| t.rain_amount > 0.0);
    let mut sky_objects: Vec<SkyObject> = made
        .iter()
        .map(|(o, m)| {
            let (begin_time, end_time) = if always(o) {
                (0.0, 0.0)
            } else {
                (o.begin_time, o.end_time)
            };
            let (begin_angle, end_angle) = if always(o) {
                (o.begin_pitch, o.begin_pitch)
            } else {
                arc(o)
            };
            SkyObject {
                begin_time,
                end_time,
                begin_angle,
                end_angle,
                tex_velocity: m.velocity,
                default_gfx_object: m.gfx,
                default_pes_object: DataId(0),
                properties: if o.after != 0 { AFTER_THE_LAND } else { 0 },
            }
        })
        .collect();
    if rains {
        sky_objects.extend(rain.iter().cloned());
    }
    let sky_time = day
        .times
        .iter()
        .map(|t| time_of_day(t, made, rains.then_some(rain.len())))
        .collect();
    SkyDayPreset {
        chance_of_occur: frequency,
        day_name: if rains { "Rainy" } else { "Clear" }.to_owned(),
        sky_objects,
        sky_time,
    }
}

/// An AC2 day's tints for its AC1 day group of `objects` objects: at each time of day, AC2's fog
/// over the sky as the veil, and each object's tint with its alpha whole (the alpha is the
/// object's transparency), the fitted one where its picture and tints were fitted together; AC1's
/// rain layers untinted.
fn tinted_day(day: &DayDesc, made: &[(Ac2Object, Model)], objects: usize) -> TintedDay {
    TintedDay {
        objects,
        times: day
            .times
            .iter()
            .map(|t| {
                let mut tints: Vec<u32> = made
                    .iter()
                    .map(|(o, m)| {
                        let c = color_of(t, o.name) | 0xFF00_0000;
                        m.retint.get(&c).copied().unwrap_or(c)
                    })
                    .collect();
                tints.resize(objects, WHITE);
                TintedTime {
                    veil: t.fog_sky.clamp(0.0, 1.0),
                    tints,
                }
            })
            .collect(),
    }
}

/// An AC2 time of day as AC1's: its light and fog, and for each object its heading, full
/// brightness and the transparency of its alpha there (white, opaque, where AC2 gives none); then,
/// on a day that rains, each of the `rain` layers as clear as the rain is light.
fn time_of_day(t: &Ac2Time, made: &[(Ac2Object, Model)], rain: Option<usize>) -> SkyTime {
    let mut sky_obj_replace: Vec<SkyObjectReplace> = made
        .iter()
        .enumerate()
        .map(|(k, (o, _))| {
            let alpha = f32::from(u8::try_from(color_of(t, o.name) >> 24).unwrap_or(255)) / 255.0;
            SkyObjectReplace {
                object_index: u32::try_from(k).unwrap_or(u32::MAX),
                gfx_obj_id: DataId(0),
                rotate: heading(o),
                transparent: transparent(alpha),
                luminosity: FULL,
                max_bright: -1.0,
            }
        })
        .collect();
    for r in 0..rain.unwrap_or(0) {
        sky_obj_replace.push(SkyObjectReplace {
            object_index: u32::try_from(made.len() + r).unwrap_or(u32::MAX),
            gfx_obj_id: DataId(0),
            rotate: 0.0,
            transparent: transparent(t.rain_amount),
            luminosity: 0.0,
            max_bright: 0.0,
        });
    }
    SkyTime {
        begin: t.begin,
        dir_bright: t.direct_brightness,
        dir_heading: t.direct_heading,
        dir_pitch: t.direct_pitch,
        dir_color: t.direct_color.0,
        amb_bright: t.ambient_brightness,
        amb_color: t.ambient_color.0,
        min_world_fog: t.fog_min,
        max_world_fog: t.fog_max,
        world_fog_color: t.fog_color.0,
        world_fog: 1,
        sky_obj_replace,
    }
}

/// The layer of `m` the device draws (the shader-capable class's, at the highest detail), as AC2
/// keeps it.
fn drawn_layer(m: &Material) -> Option<MaterialLayer> {
    let m = sub_objects(m, HARDWARE_CLASS);
    let keep = |l: &&MaterialLayer| {
        l.flags_set & DEVICE_FLAGS == l.flags_set && l.flags_clear & DEVICE_FLAGS == 0
    };
    m.layers
        .iter()
        .find(keep)
        .or_else(|| m.layers.first())
        .cloned()
}

/// How fast a waveform's value moves, units a second: a speed's frequency and rates, any other's
/// base rate.
fn rate(w: &auberean_assets::common::Waveform) -> f32 {
    if w.kind == 2 {
        w.frequency + w.phase_rate + w.base_rate
    } else {
        w.base_rate
    }
}

/// How fast a layer's first stage's picture slides, units a second in u and v.
fn velocity(l: &MaterialLayer) -> (f32, f32) {
    l.animations
        .iter()
        .filter(|a| a.stage == 0)
        .find_map(|a| match &a.animation {
            CoordinateAnimation::Translate { u, v } => Some((rate(u), rate(v))),
            _ => None,
        })
        .unwrap_or((0.0, 0.0))
}

/// One texel through a layer's texture stages as the device works them out with no pixel program:
/// each stage's color and alpha operation on its two arguments (the vertex color, the stage
/// before's result, the picture, the texture factor), the first stage whose color is turned off
/// ending them. `diffuse` and `factor` are red, green, blue, alpha from 0 to 1.
fn texel(
    stages: &[auberean_assets::material::TextureStage],
    tex: [f32; 4],
    diffuse: [f32; 4],
    factor: [f32; 4],
) -> [f32; 4] {
    let arg = |a: u32, current: [f32; 4]| {
        let mut v = match a & 0xF {
            0 => diffuse,
            2 => tex,
            3 => factor,
            _ => current,
        };
        if a & 0x20 != 0 {
            v = [v[3]; 4];
        }
        if a & 0x10 != 0 {
            v = v.map(|x| 1.0 - x);
        }
        v
    };
    let op = |o: u32, x: f32, y: f32| match o {
        2 => x,
        3 => y,
        4 => x * y,
        5 => 2.0 * x * y,
        6 => 4.0 * x * y,
        7 => x + y,
        8 => x + y - 0.5,
        9 => 2.0 * (x + y - 0.5),
        10 => x - y,
        11 => x + y - x * y,
        _ => x,
    };
    let mut current = diffuse;
    for s in stages.iter().take(3) {
        if s.color_op <= 1 {
            break;
        }
        let (c1, c2) = (arg(s.color_arg1, current), arg(s.color_arg2, current));
        let (a1, a2) = (arg(s.alpha_arg1, current), arg(s.alpha_arg2, current));
        let mut next = current;
        for k in 0..3 {
            next[k] = op(s.color_op, c1[k], c2[k]).clamp(0.0, 1.0);
        }
        if s.alpha_op > 1 {
            next[3] = op(s.alpha_op, a1[3], a2[3]).clamp(0.0, 1.0);
        }
        current = next;
    }
    current
}

/// The texel that, times each of `tints`, comes nearest to the stages' texel under that tint: for
/// each channel, the least-squares multiplier over them. The layer's diffuse color `d` times the
/// tint is the stages' vertex color, and it stored as a color their texture factor, as AC2 draws
/// a tinted layer. With no tints, or white alone, it is the texel under no tint.
fn fitted_texel(
    stages: &[auberean_assets::material::TextureStage],
    tex: [f32; 4],
    d: [f32; 4],
    tints: &[[f32; 3]],
) -> [f32; 4] {
    let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0).trunc() / 255.0;
    let white = [[1.0f32; 3]];
    let tints = if tints.is_empty() { &white[..] } else { tints };
    let (mut num, mut den) = ([0.0f32; 4], [0.0f32; 4]);
    for t in tints {
        let tinted = [d[0] * t[0], d[1] * t[1], d[2] * t[2], d[3]];
        let c = texel(stages, tex, tinted, tinted.map(q));
        for k in 0..3 {
            num[k] += t[k] * c[k];
            den[k] += t[k] * t[k];
        }
        num[3] += c[3];
        den[3] += 1.0;
    }
    let mut out = [0.0f32; 4];
    for k in 0..4 {
        out[k] = if den[k] > 1e-6 {
            (num[k] / den[k]).clamp(0.0, 1.0)
        } else {
            0.0
        };
    }
    out
}

/// Whether a layer's stages give a color in proportion to its tint: half the tint, half the color,
/// on texels and tints dim enough that nothing clamps. The clouds' do (they multiply); the dome's
/// do not (they add the picture signed).
fn scales_with_the_tint(stages: &[auberean_assets::material::TextureStage], d: [f32; 4]) -> bool {
    let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0).trunc() / 255.0;
    let under = |tex: [f32; 4], t: f32| {
        let tinted = [d[0] * t, d[1] * t, d[2] * t, d[3]];
        texel(stages, tex, tinted, tinted.map(q))
    };
    [[0.2, 0.3, 0.4, 1.0], [0.4, 0.4, 0.4, 0.5]]
        .iter()
        .all(|&tex| {
            let (full, half) = (under(tex, 0.5), under(tex, 0.25));
            (0..3).all(|k| (half[k] * 2.0 - full[k]).abs() < 0.02)
        })
}

/// How many rounds the picture and the tints are fitted in turn; enough to settle.
const FIT_ROUNDS: usize = 48;

/// For stages whose color does not scale with the tint: each of `texels` (with how often it
/// appears) and each of `tints` (with how much the object `shows` in it, its alpha) fitted
/// together, so that the picture times each tint comes nearest to the stages under AC2's tint, a
/// tint counting as much as the object shows in it: for each channel, the nearest product of a
/// texel's value and a tint's, in turns. The picture's brightest texel is 1, unless a tint would
/// then pass 1. Its alpha is [`fitted_texel`]'s. Answers the pictures' texels and the tints drawn
/// in place of `tints`.
fn fitted_together(
    stages: &[auberean_assets::material::TextureStage],
    texels: &[([f32; 4], usize)],
    d: [f32; 4],
    tints: &[[f32; 3]],
    shows: &[f32],
) -> (Vec<[f32; 4]>, Vec<[f32; 3]>) {
    let shown: Vec<f64> = (0..tints.len())
        .map(|j| f64::from(shows.get(j).copied().unwrap_or(1.0).clamp(0.0, 1.0)))
        .collect();
    let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0).trunc() / 255.0;
    let drawn: Vec<Vec<[f32; 4]>> = texels
        .iter()
        .map(|(tex, _)| {
            tints
                .iter()
                .map(|t| {
                    let tinted = [d[0] * t[0], d[1] * t[1], d[2] * t[2], d[3]];
                    texel(stages, *tex, tinted, tinted.map(q))
                })
                .collect()
        })
        .collect();
    let mut pictures: Vec<[f32; 4]> = texels
        .iter()
        .map(|(tex, _)| [0.0, 0.0, 0.0, fitted_texel(stages, *tex, d, tints)[3]])
        .collect();
    let mut drawn_tints = vec![[0.0f32; 3]; tints.len()];
    for k in 0..3 {
        let w: Vec<f64> = texels.iter().map(|(_, n)| *n as f64).collect();
        let c = |i: usize, j: usize| f64::from(drawn[i][j][k]);
        let total: f64 = w.iter().sum();
        let mut u = vec![1.0f64; texels.len()];
        let mut v: Vec<f64> = (0..tints.len())
            .map(|j| (0..texels.len()).map(|i| w[i] * c(i, j)).sum::<f64>() / total.max(1e-12))
            .collect();
        for _ in 0..FIT_ROUNDS {
            let vv: f64 = (0..tints.len()).map(|j| shown[j] * v[j] * v[j]).sum();
            if vv <= 1e-12 {
                break;
            }
            for (i, ui) in u.iter_mut().enumerate() {
                *ui = (0..tints.len())
                    .map(|j| shown[j] * c(i, j) * v[j])
                    .sum::<f64>()
                    / vv;
            }
            let uu: f64 = (0..texels.len()).map(|i| w[i] * u[i] * u[i]).sum();
            if uu <= 1e-12 {
                break;
            }
            for (j, vj) in v.iter_mut().enumerate() {
                *vj = (0..texels.len())
                    .map(|i| w[i] * c(i, j) * u[i])
                    .sum::<f64>()
                    / uu;
            }
        }
        // The brightest texel at 1, the tints taking the rest; a tint past 1 takes 1 and the
        // picture the rest.
        let top = u.iter().copied().fold(0.0f64, f64::max);
        if top > 1e-12 {
            u.iter_mut().for_each(|x| *x /= top);
            v.iter_mut().for_each(|x| *x *= top);
        }
        let over = v.iter().copied().fold(0.0f64, f64::max);
        if over > 1.0 {
            u.iter_mut().for_each(|x| *x *= over);
            v.iter_mut().for_each(|x| *x /= over);
        }
        #[allow(clippy::cast_possible_truncation)]
        for (i, p) in pictures.iter_mut().enumerate() {
            p[k] = (u[i] as f32).clamp(0.0, 1.0);
        }
        #[allow(clippy::cast_possible_truncation)]
        for (j, t) in drawn_tints.iter_mut().enumerate() {
            t[k] = (v[j] as f32).clamp(0.0, 1.0);
        }
    }
    (pictures, drawn_tints)
}

/// A layer's picture: its texture's id, its note, and the tint drawn for each of AC2's where it
/// was fitted with its tints.
type Picture = (DataId, String, BTreeMap<u32, u32>);

/// A drawn layer's picture as AC2 draws it under the tints `colors`: its first stage's picture
/// through every stage ([`fitted_texel`]), the texture factor the layer's diffuse color, as an AC1
/// image texture. Where the layer is its model's `alone` and its stages do not scale with the
/// tint, the picture and the tints are fitted together ([`fitted_together`]). Also its note.
fn picture(
    ac2: &Ac2,
    l: &MaterialLayer,
    colors: &[(u32, f32)],
    alone: bool,
    ids: &mut Ids,
    out: &mut Records,
) -> Result<Option<Picture>, String> {
    let tints: Vec<[f32; 3]> = colors.iter().map(|&(c, _)| rgb(c)).collect();
    let shows: Vec<f32> = colors.iter().map(|&(_, a)| a).collect();
    let Some(list) = l
        .stages
        .first()
        .map(|s| s.texture.0)
        .filter(|&t| t & 0x000F_FFFF != 0)
    else {
        return Ok(None);
    };
    let Some(img) = ac2.picture(list, PICTURE_LIMIT) else {
        return Ok(None);
    };
    let d = [l.diffuse.r, l.diffuse.g, l.diffuse.b, l.diffuse.a];
    let mut bgra = Vec::with_capacity(img.texels.len());
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let byte = |x: f32| (x * 255.0).round() as u8;
    // A picture's texels repeat: each distinct one is worked out once.
    let mut seen: BTreeMap<[u8; 4], [u8; 4]> = BTreeMap::new();
    let mut retint = BTreeMap::new();
    if alone && tints.len() > 1 && !scales_with_the_tint(&l.stages, d) {
        // The picture and its tints fitted together.
        let mut count: BTreeMap<[u8; 4], usize> = BTreeMap::new();
        for t in img.texels.as_chunks::<4>().0 {
            *count.entry(*t).or_default() += 1;
        }
        let texels: Vec<([f32; 4], usize)> = count
            .iter()
            .map(|(t, n)| (t.map(|b| f32::from(b) / 255.0), *n))
            .collect();
        let (pictures, drawn) = fitted_together(&l.stages, &texels, d, &tints, &shows);
        for ((t, _), c) in count.iter().zip(&pictures) {
            seen.insert(*t, [byte(c[2]), byte(c[1]), byte(c[0]), byte(c[3])]);
        }
        for (&(c, _), t) in colors.iter().zip(&drawn) {
            let packed = 0xFF00_0000
                | (u32::from(byte(t[0])) << 16)
                | (u32::from(byte(t[1])) << 8)
                | u32::from(byte(t[2]));
            retint.insert(c | 0xFF00_0000, packed);
        }
    }
    for t in img.texels.as_chunks::<4>().0 {
        let c = *seen.entry(*t).or_insert_with(|| {
            let tex = t.map(|b| f32::from(b) / 255.0);
            let c = fitted_texel(&l.stages, tex, d, &tints);
            [byte(c[2]), byte(c[1]), byte(c[0]), byte(c[3])]
        });
        bgra.extend_from_slice(&c);
    }
    let (rs_id, st_id) = (ids.next(0x06), ids.next(0x05));
    let rs = RenderSurface {
        id: rs_id,
        data_category: MODEL_IMAGE_CATEGORY,
        width: img.width,
        height: img.height,
        format: PFID_A8R8G8B8,
        image_size: u32::try_from(bgra.len()).unwrap_or(u32::MAX),
        data_offset: 0x18,
        default_palette_id: None,
    };
    out.add(rs_id, rs.encode(&bgra));
    let st = SurfaceTexture {
        id: st_id,
        data_category: 0,
        unknown_byte: 2,
        source_levels: vec![rs_id],
    };
    out.add(st_id, st.encode());
    let fitted = if retint.is_empty() {
        ""
    } else {
        ", it and its tints fitted together"
    };
    Ok(Some((
        st_id,
        format!("picture {list:08X} {}x{}{fitted}", img.width, img.height),
        retint,
    )))
}

/// AC1's surface kind for an AC2 layer's blend: added to what is behind it by its alpha, blended by
/// its alpha, or drawn whole.
fn surface_kind(l: &MaterialLayer) -> (u32, &'static str) {
    match (l.source_blend, l.dest_blend) {
        (BLEND_SRC_ALPHA, BLEND_ONE) => (
            SURFACE_IMAGE | SURFACE_ALPHA | SURFACE_ADDITIVE,
            "added by its alpha",
        ),
        (_, BLEND_ONE) => (SURFACE_IMAGE | SURFACE_ADDITIVE, "added"),
        (BLEND_SRC_ALPHA, BLEND_INV_SRC_ALPHA) => {
            (SURFACE_IMAGE | SURFACE_ALPHA, "blended by its alpha")
        }
        _ => (SURFACE_IMAGE, "whole"),
    }
}

/// One material of a sky mesh as an AC1 surface: its id, whether it is drawn from both sides, its
/// layer's start coordinate transform (`u' = m0 u + m1 v + m2`, `v' = m3 u + m4 v + m5`), its
/// layer's scroll, and a note. `None` when it draws nothing.
type MadeSurface = Option<(
    DataId,
    bool,
    [f32; 6],
    (f32, f32),
    String,
    BTreeMap<u32, u32>,
)>;

fn surface(
    wr: &WorldRecords,
    ac2: &Ac2,
    mi: u32,
    colors: &[(u32, f32)],
    alone: bool,
    ids: &mut Ids,
    out: &mut Records,
) -> Result<MadeSurface, String> {
    let Some(m) = instance_material(wr, mi) else {
        return Ok(None);
    };
    let Some(l) = drawn_layer(&m) else {
        return Ok(None);
    };
    let Some((tex, pic, retint)) = picture(ac2, &l, colors, alone, ids, out)? else {
        return Ok(None);
    };
    let (kind, how) = surface_kind(&l);
    let s = Surface {
        surface_type: kind,
        orig_texture_id: Some(tex),
        orig_palette_id: Some(DataId(0)),
        color_value: None,
        translucency: 0.0,
        luminosity: 0.0,
        diffuse: 1.0,
    };
    let id = ids.next(0x08);
    out.add(id, s.encode());
    Ok(Some((
        id,
        l.cull == CULL_NONE,
        stage_uv_transform(&l, 0),
        velocity(&l),
        format!("{how}, {pic}"),
        retint,
    )))
}

/// An AC2 sky object's setup or visual description as one AC1 model, its pictures made for the
/// `tints` it is drawn in: every part's most detailed level where its setup places it, each
/// material its own surface and its own texture coordinates. `None` when it plays an animation or
/// draws nothing.
fn model(
    wr: &WorldRecords,
    ac2: &Ac2,
    id: u32,
    colors: &[(u32, f32)],
    ids: &mut Ids,
    out: &mut Records,
) -> Result<Option<Model>, String> {
    let parts = parts_of(
        wr,
        auberean_primitives::DataId(id),
        &Mat4::IDENTITY,
        auberean_primitives::Vec3::new(1.0, 1.0, 1.0),
    );
    if parts.is_empty() || parts.iter().any(|p| p.played.is_some()) {
        return Ok(None);
    }
    // A model of one material may have its picture and tints fitted together: its tint is its
    // material's alone.
    let materials: BTreeSet<u32> = parts
        .iter()
        .filter_map(|p| wr.get::<Mesh>(p.mesh.0))
        .flat_map(|m| m.materials.iter().map(|mi| mi.0).collect::<Vec<_>>())
        .collect();
    let alone = materials.len() == 1;
    let mut retint = BTreeMap::new();
    let mut vertices: Vec<SwVertex> = Vec::new();
    let mut polygons: Vec<Polygon> = Vec::new();
    let mut surfaces: Vec<DataId> = Vec::new();
    let mut made: BTreeMap<u32, MadeSurface> = BTreeMap::new();
    let mut velocity = None;
    let mut notes: Vec<String> = Vec::new();
    for p in &parts {
        let Some(m) = wr.get::<Mesh>(p.mesh.0) else {
            continue;
        };
        if m.has_matrix_weights() {
            return Ok(None);
        }
        let Some(vs) = m.vertices.attributes() else {
            continue;
        };
        let total = |k: usize| {
            m.levels[k]
                .fragments
                .iter()
                .map(|f| f.indices.len())
                .sum::<usize>()
        };
        let Some(best) = (0..m.levels.len()).max_by_key(|&k| (total(k), usize::MAX - k)) else {
            continue;
        };
        // Each of the mesh's materials gets its own coordinates on every vertex, moved as its layer
        // moves them at the start.
        let mut own: Vec<MadeSurface> = Vec::with_capacity(m.materials.len());
        for mi in &m.materials {
            if let std::collections::btree_map::Entry::Vacant(e) = made.entry(mi.0) {
                e.insert(surface(wr, ac2, mi.0, colors, alone, ids, out)?);
            }
            own.push(made[&mi.0].clone());
        }
        let base = vertices.len();
        for (k, v) in vs.iter().enumerate() {
            let q = p.transform.point(v.position);
            let uv = v.uv.unwrap_or([0.0, 0.0]);
            let uvs = own
                .iter()
                .map(|s| {
                    let t = s.as_ref().map_or([1.0, 0.0, 0.0, 0.0, 1.0, 0.0], |s| s.2);
                    (
                        t[0] * uv[0] + t[1] * uv[1] + t[2],
                        t[3] * uv[0] + t[4] * uv[1] + t[5],
                    )
                })
                .collect::<Vec<_>>();
            vertices.push(SwVertex {
                id: i16::try_from(base + k).unwrap_or(0),
                position: Vec3::new(q.x, q.y, q.z),
                normal: Vec3::new(0.0, 0.0, 1.0),
                uvs: if uvs.is_empty() {
                    vec![(uv[0], uv[1])]
                } else {
                    uvs
                },
            });
        }
        for f in &m.levels[best].fragments {
            let Some(Some((sid, both, _, vel, note, fitted))) = own.get(f.material as usize) else {
                continue;
            };
            retint.extend(fitted.iter().map(|(a, b)| (*a, *b)));
            velocity.get_or_insert(*vel);
            if !notes.contains(note) {
                notes.push(note.clone());
            }
            let s = surfaces.iter().position(|x| x == sid).unwrap_or_else(|| {
                surfaces.push(*sid);
                surfaces.len() - 1
            });
            let uv = u8::try_from(f.material).unwrap_or(0);
            for tri in f.indices.as_chunks::<3>().0 {
                let Some(v) = tri
                    .iter()
                    .map(|&i| {
                        u16::try_from(base + usize::from(i))
                            .ok()
                            .filter(|&i| usize::from(i) < vertices.len())
                    })
                    .collect::<Option<Vec<u16>>>()
                else {
                    continue;
                };
                polygons.push(Polygon {
                    poly_id: i16::try_from(polygons.len()).unwrap_or(i16::MAX),
                    num_pts: 3,
                    stippling: 1,
                    sides_type: i32::from(*both),
                    pos_surface: u16::try_from(s).unwrap_or(0),
                    neg_surface: u16::try_from(s).unwrap_or(0),
                    vertex_ids: v,
                    pos_uv_indices: Some(vec![uv; 3]),
                    neg_uv_indices: both.then(|| vec![uv; 3]),
                });
            }
        }
    }
    if polygons.is_empty() {
        return Ok(None);
    }
    let k = distance_scale(&vertices);
    for v in &mut vertices {
        v.position = Vec3::new(v.position.x * k, v.position.y * k, v.position.z * k);
    }
    let va = VertexArray {
        vertex_type: 1,
        vertices,
    };
    let drawing = dereth_builder::bsp::drawing(&va, &polygons);
    let sort_center = drawing
        .nodes
        .first()
        .and_then(|n| n.sphere)
        .map_or(Vec3::ZERO, |s| s.center);
    let gfx = ids.next(0x01);
    let g = GfxObj {
        id: gfx,
        flags: 2,
        surfaces,
        vertex_array: va,
        physics_polygons: Vec::new(),
        physics_bsp: None,
        sort_center,
        polygons,
        drawing_bsp: Some(drawing),
        did_degrade: None,
    };
    out.add(
        gfx,
        g.encode_for(EraId::Eor)
            .map_err(|e| format!("{gfx:?}: {e}"))?,
    );
    Ok(Some(Model {
        gfx,
        velocity: velocity.unwrap_or((0.0, 0.0)),
        note: notes.join("; "),
        retint,
    }))
}

/// How much a sky model of `vertices` is scaled out from its center: until its nearest point lies
/// [`SKY_NEAR`] out, never past [`SKY_FAR`] for its farthest, never smaller.
fn distance_scale(vertices: &[SwVertex]) -> f32 {
    let (mut near, mut far) = (f32::MAX, 0.0f32);
    for v in vertices {
        let p = v.position;
        let r = (p.x * p.x + p.y * p.y + p.z * p.z).sqrt();
        near = near.min(r);
        far = far.max(r);
    }
    if vertices.is_empty() || far <= 0.0 {
        return 1.0;
    }
    (SKY_NEAR / near.max(1e-3)).min(SKY_FAR / far).max(1.0)
}

/// The region record with its sky replaced by `groups` (AC1's tick sizes kept), encoded whole.
///
/// # Errors
/// When the record does not decode, or does not read back as written.
pub fn splice(record: &[u8], groups: &[SkyDayPreset]) -> Result<Vec<u8>, String> {
    let id = DataId(0x1300_0000);
    let mut r = <Region as dereth_assets::Decode>::decode_payload(id, record)
        .map_err(|e| format!("region: {e:?}"))?;
    let sky = r.sky_info.as_mut().ok_or("the region has no sky")?;
    sky.day_groups = groups.to_vec();
    let bytes = r.encode().map_err(|e| format!("region: {e:?}"))?;
    let back = <Region as dereth_assets::Decode>::decode_payload(id, &bytes)
        .map_err(|e| format!("spliced region: {e:?}"))?;
    if back != r {
        return Err("the region with AC2's sky reads back otherwise".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's sky)
    use super::*;
    use auberean_assets::common::{Color32, Waveform};
    use auberean_assets::material::TextureStage;
    use auberean_assets::region::SkyReplacement;

    fn object(
        name: u32,
        t: (f32, f32),
        pitch: (f32, f32),
        heading: f32,
        west_to_east: u32,
    ) -> Ac2Object {
        Ac2Object {
            name,
            mesh: auberean_primitives::DataId(0x0400_0023),
            begin_time: t.0,
            end_time: t.1,
            begin_pitch: pitch.0,
            end_pitch: pitch.1,
            begin_heading: heading,
            end_heading: heading,
            west_to_east,
            ..Ac2Object::default()
        }
    }

    fn model(gfx: u32) -> Model {
        Model {
            gfx: DataId(gfx),
            velocity: (0.01, 0.0),
            note: String::new(),
            retint: BTreeMap::new(),
        }
    }

    fn time(begin: f32, colors: &[(u32, u32)], rain: f32, fog_sky: f32) -> Ac2Time {
        Ac2Time {
            begin,
            fog_min: -50.0,
            fog_max: 1500.0,
            fog_sky,
            fog_color: Color32(0xFF8E_5D4D),
            direct_brightness: 0.65,
            direct_heading: 90.0,
            direct_pitch: 30.0,
            direct_color: Color32(0xFFFB_BCA7),
            ambient_brightness: 0.25,
            ambient_color: Color32(0xFF64_6495),
            replacements: colors
                .iter()
                .map(|&(object, c)| SkyReplacement {
                    object,
                    color: Color32(c),
                    ..SkyReplacement::default()
                })
                .collect(),
            rain_amount: rain,
            ..Ac2Time::default()
        }
    }

    fn rain() -> Vec<SkyObject> {
        vec![SkyObject {
            begin_time: 0.0,
            end_time: 0.0,
            begin_angle: 0.0,
            end_angle: 0.0,
            tex_velocity: (0.02, -2.0),
            default_gfx_object: DataId(0x0100_4C42),
            default_pes_object: DataId(0),
            properties: WEATHER,
        }]
    }

    /// The dome (always), the sun (a window and an arc) and the moon (an arc west to east); the
    /// dome blue and clear at midnight, opaque and pale at noon, the moon gone by noon.
    fn day(rain_at_noon: f32) -> (DayDesc, Vec<(Ac2Object, Model)>) {
        let dome = object(1, (0.0, 1.0), (0.0, 0.0), 0.0, 0);
        let sun = object(5, (0.25, 0.96), (-39.0, 200.0), 270.0, 0);
        let moon = object(7, (0.001, 0.31), (205.0, -25.0), 300.0, 1);
        let made = vec![
            (dome.clone(), model(0x0100_C000)),
            (sun.clone(), model(0x0100_C001)),
            (moon.clone(), model(0x0100_C002)),
        ];
        let day = DayDesc {
            version: 1,
            times: vec![
                time(
                    0.0,
                    &[(1, 0x0010_2040), (5, 0x0012_3456), (7, 0xFF12_3456)],
                    0.0,
                    0.5,
                ),
                time(
                    0.5,
                    &[(1, 0xFF80_A0FF), (7, 0x0012_3456)],
                    rain_at_noon,
                    0.0,
                ),
            ],
            objects: vec![dome, sun, moon],
        };
        (day, made)
    }

    #[test]
    fn an_object_shown_all_day_is_shown_always_and_a_crossing_one_keeps_its_window_and_arc() {
        let (d, made) = day(0.0);
        let g = day_group(&d, 25.0, &made, &rain());
        assert_eq!(g.chance_of_occur, 25.0);
        assert_eq!(g.day_name, "Clear");
        assert_eq!(
            g.sky_objects.len(),
            3,
            "no rain layers on a day without rain"
        );
        let o = &g.sky_objects;
        assert_eq!((o[0].begin_time, o[0].end_time), (0.0, 0.0));
        assert_eq!(
            (
                o[1].begin_time,
                o[1].end_time,
                o[1].begin_angle,
                o[1].end_angle
            ),
            (0.25, 0.96, -39.0, 200.0)
        );
        assert_eq!((o[2].begin_angle, o[2].end_angle), (205.0, -25.0));
        assert_eq!(o[1].default_gfx_object, DataId(0x0100_C001));
        assert_eq!(o[1].tex_velocity, (0.01, 0.0));
    }

    #[test]
    fn every_time_of_day_carries_every_object_its_heading_full_brightness_and_its_alpha() {
        let (d, made) = day(0.0);
        let g = day_group(&d, 6.0, &made, &rain());
        assert_eq!(g.sky_time.len(), 2);
        let t0 = &g.sky_time[0];
        assert_eq!(t0.sky_obj_replace.len(), 3);
        let r: Vec<(u32, f32, f32, f32)> = t0
            .sky_obj_replace
            .iter()
            .map(|r| (r.object_index, r.rotate, r.transparent, r.luminosity))
            .collect();
        assert_eq!(
            r,
            vec![
                (0, 0.0, 100.0, 100.0),
                (1, 270.0, 100.0, 100.0),
                (2, 300.0, 0.0, 100.0)
            ]
        );
        // No replacement for the sun at noon: AC2 draws it white and opaque.
        let t1 = &g.sky_time[1];
        assert_eq!(t1.sky_obj_replace[1].transparent, 0.0);
        assert_eq!(t1.sky_obj_replace[0].transparent, 0.0);
        assert_eq!(t1.sky_obj_replace[2].transparent, 100.0);
        assert!(
            t1.sky_obj_replace.iter().all(|r| r.gfx_obj_id == DataId(0)),
            "no object is brought back outside its window"
        );
        assert_eq!(
            (t0.dir_bright, t0.dir_heading, t0.dir_pitch, t0.dir_color),
            (0.65, 90.0, 30.0, 0xFFFB_BCA7)
        );
        assert_eq!(
            (
                t0.min_world_fog,
                t0.max_world_fog,
                t0.world_fog_color,
                t0.world_fog
            ),
            (-50.0, 1500.0, 0xFF8E_5D4D, 1)
        );
    }

    /// The tints are AC2's own colors, read off its replacements: each time of day's color for
    /// each object (white where AC2 gives none), whole (the alpha is the transparency), and the
    /// fog over the sky as the veil; AC1's rain layers untinted.
    #[test]
    fn the_tints_are_ac2s_colors_and_its_fog_over_the_sky() {
        let (d, made) = day(0.75);
        let g = day_group(&d, 25.0, &made, &rain());
        let t = tinted_day(&d, &made, g.sky_objects.len());
        assert!(t.fits(g.sky_objects.len(), g.sky_time.len()));
        assert_eq!(
            t.times[0].tints,
            vec![0xFF10_2040, 0xFF12_3456, 0xFF12_3456, WHITE]
        );
        assert_eq!(
            t.times[1].tints,
            vec![0xFF80_A0FF, WHITE, 0xFF12_3456, WHITE]
        );
        assert_eq!(
            t.times.iter().map(|t| t.veil).collect::<Vec<_>>(),
            vec![0.5, 0.0]
        );
        // The tints read back from the record the converter writes.
        let all = SkyTints { days: vec![t] };
        assert_eq!(SkyTints::decode(&all.encode()).unwrap(), all);
    }

    /// A model whose picture and tints were fitted together is drawn in its fitted tint for each
    /// of AC2's; the others keep AC2's.
    #[test]
    fn a_dome_fitted_with_its_tints_is_tinted_with_the_fitted_ones() {
        let (d, mut made) = day(0.0);
        made[0].1.retint = [(0xFF10_2040, 0xFF20_3050), (0xFF80_A0FF, 0xFFE0_E8F0)].into();
        let g = day_group(&d, 25.0, &made, &rain());
        let t = tinted_day(&d, &made, g.sky_objects.len());
        assert_eq!(
            t.times[0].tints[..3],
            [0xFF20_3050, 0xFF12_3456, 0xFF12_3456]
        );
        assert_eq!(t.times[1].tints[..3], [0xFFE0_E8F0, WHITE, 0xFF12_3456]);
        // The alpha stays the transparency's, as AC2's color gives it.
        assert_eq!(g.sky_time[0].sky_obj_replace[0].transparent, 100.0);
    }

    /// On AC2's own first day: the dome's tint at noon is AC2's pale blue, whole, and its alpha
    /// (none at midnight, all at noon) is the dome's transparency; the fog veils half the sky at
    /// midnight and none of it at noon.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn the_tints_of_ac2s_first_day_are_its_own() {
        let wr = crate::ac2_world::statics::world_records(
            &auberean_dat::testing::require_target_install(),
        )
        .expect("the install");
        let sky = wr.region().expect("the region").sky.expect("a sky");
        let day = wr.day(sky.days[0].day).expect("the first day");
        let made: Vec<(Ac2Object, Model)> = day
            .objects
            .iter()
            .enumerate()
            .map(|(k, o)| (o.clone(), model(0x0100_C000 + u32::try_from(k).unwrap())))
            .collect();
        let g = day_group(&day, sky.days[0].frequency, &made, &rain());
        let t = tinted_day(&day, &made, g.sky_objects.len());
        assert!(t.fits(g.sky_objects.len(), g.sky_time.len()));
        let at = |begin: f32| {
            day.times
                .iter()
                .position(|x| (x.begin - begin).abs() < 1e-4)
                .expect("a time of day there")
        };
        let (midnight, noon) = (at(0.0), at(0.5));
        // The dome is the day's first object.
        assert_eq!(day.objects[0].mesh.0, 0x0400_0022);
        assert_eq!(t.times[noon].tints[0], 0xFF8E_8BCC);
        assert_eq!(t.times[midnight].tints[0], 0xFF0E_1426);
        assert_eq!(g.sky_time[noon].sky_obj_replace[0].transparent, 0.0);
        assert_eq!(g.sky_time[midnight].sky_obj_replace[0].transparent, 100.0);
        assert_eq!(t.times[midnight].veil, 0.5);
        assert_eq!(t.times[noon].veil, 0.0);
    }

    #[test]
    fn a_day_that_rains_adds_ac1s_rain_as_clear_as_the_rain_is_light() {
        let (d, made) = day(0.75);
        let g = day_group(&d, 25.0, &made, &rain());
        assert_eq!(g.day_name, "Rainy");
        assert_eq!(g.sky_objects.len(), 4);
        assert_eq!(g.sky_objects[3].default_gfx_object, DataId(0x0100_4C42));
        let tr: Vec<f32> = g
            .sky_time
            .iter()
            .map(|t| t.sky_obj_replace[3].transparent)
            .collect();
        assert_eq!(tr, vec![100.0, 25.0]);
        assert!(dereth_assets::region::DayKind::Rainy.is(&g));
    }

    #[test]
    fn an_arc_is_carried_the_way_ac2_carries_it() {
        // Westward, ending below where it starts: AC2 goes up through 360 (here, 350 to 370).
        let o = object(5, (0.2, 0.4), (350.0, 10.0), 0.0, 0);
        assert_eq!(arc(&o), (-10.0, 10.0));
        let o = object(5, (0.2, 0.4), (10.0, 350.0), 0.0, 0);
        assert_eq!(arc(&o), (10.0, 350.0));
        let o = object(5, (0.2, 0.4), (200.0, 20.0), 0.0, 0);
        assert_eq!(arc(&o), (-160.0, 20.0));
        // West to east, rising: AC2 goes down through 0.
        let o = object(5, (0.2, 0.4), (20.0, 200.0), 0.0, 1);
        assert_eq!(arc(&o), (20.0, -160.0));
    }

    /// AC2's and AC1's sky turn a point of the sun's mesh to the same place: AC2's turn of a
    /// heading and a pitch, against AC1's frame for the same heading and the arc angle the
    /// converter gives it, all along the sun's and the moon's arcs.
    #[test]
    fn the_sun_and_the_moon_stand_where_ac2_stands_them_in_ac1s_sky() {
        let (d, made) = day(0.0);
        let g = day_group(&d, 25.0, &made, &rain());
        // A corner of the sun's quad and of the moon's, where AC2's setups place them.
        let points = [
            auberean_primitives::Vec3::new(500.0, -876.884, 600.0),
            auberean_primitives::Vec3::new(-334.9, -931.2, -295.5),
        ];
        for (k, t) in [
            (1usize, 0.26f32),
            (1, 0.5),
            (1, 0.7),
            (1, 0.95),
            (2, 0.01),
            (2, 0.15),
            (2, 0.3),
        ] {
            let ac2 = auberean_world::sky::sky_pieces(&d, t);
            let piece = ac2
                .iter()
                .find(|p| p.index == k)
                .expect("shown at that time");
            let ac2_world = Mat4::from_parts(
                auberean_primitives::Vec3::new(0.0, 0.0, 0.0),
                piece.rotation,
                auberean_primitives::Vec3::new(1.0, 1.0, 1.0),
            );
            let ac1 = dereth_world_render::sky::get_sky(&g, t);
            let mut frame = dereth_primitives::Frame::default();
            dereth_world_render::sky::calc_frame(&mut frame, ac1[k].heading, ac1[k].rotation);
            let m = dereth_primitives::frame::l2g(frame.rotation);
            for p in points {
                let a = ac2_world.point(p);
                let b = dereth_primitives::frame::localtoglobalvec(m, Vec3::new(p.x, p.y, p.z));
                let d = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt();
                assert!(d < 0.5, "object {k} at {t}: AC2 {a:?}, AC1 {b:?}");
            }
        }
    }

    #[test]
    fn an_alpha_is_a_transparency() {
        assert_eq!(transparent(0.0), 100.0);
        assert_eq!(transparent(1.0), 0.0);
        assert!((transparent(0.25) - 75.0).abs() < 1e-4);
        assert_eq!(transparent(2.0), 0.0);
    }

    fn stage(color_op: u32, c: (u32, u32), alpha_op: u32, a: (u32, u32)) -> TextureStage {
        TextureStage {
            color_op,
            color_arg1: c.0,
            color_arg2: c.1,
            alpha_op,
            alpha_arg1: a.0,
            alpha_arg2: a.1,
            ..TextureStage::default()
        }
    }

    #[test]
    fn the_domes_stages_with_no_tint_double_its_picture_about_the_middle() {
        // Twice the picture times half white, its alpha the factor's; then the picture added signed.
        let stages = [stage(5, (2, 3), 3, (2, 3)), stage(8, (2, 1), 3, (2, 1))];
        let half = [0.5, 0.5, 0.5, 1.0];
        let c = texel(&stages, [0.6, 0.3, 0.2, 0.1], half, half);
        let want = [0.7, 0.1, 0.0, 1.0];
        for k in 0..4 {
            assert!((c[k] - want[k]).abs() < 1e-5, "{c:?}");
        }
    }

    #[test]
    fn the_clouds_stages_with_no_tint_keep_their_picture_and_double_its_alpha_squared() {
        let stages = [stage(4, (2, 3), 4, (2, 3)), stage(3, (2, 1), 5, (2, 1))];
        let white = [1.0; 4];
        let c = texel(&stages, [0.8, 0.7, 0.6, 0.5], white, white);
        let want = [0.8, 0.7, 0.6, 0.5];
        for k in 0..4 {
            assert!((c[k] - want[k]).abs() < 1e-5, "{c:?}");
        }
        let c = texel(&stages, [0.8, 0.7, 0.6, 0.3], white, white);
        assert!((c[3] - 0.18).abs() < 1e-5);
    }

    /// Stages that only multiply by the tint, as the clouds' do, give a picture that times each
    /// tint is AC2's own texel under it; with white alone, the picture is the untinted texel.
    #[test]
    fn a_picture_made_for_its_tints_times_each_tint_is_ac2s_texel_under_it() {
        let stages = [stage(4, (2, 3), 4, (2, 3)), stage(3, (2, 1), 5, (2, 1))];
        let tex = [0.8, 0.7, 0.6, 0.5];
        let tints = [[0.2, 0.4, 1.0], [1.0, 0.5, 0.25], [0.9, 0.9, 0.9]];
        let p = fitted_texel(&stages, tex, [1.0; 4], &tints);
        let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0).trunc() / 255.0;
        for t in tints {
            let tinted = [t[0], t[1], t[2], 1.0];
            let want = texel(&stages, tex, tinted, tinted.map(q));
            for k in 0..3 {
                assert!(
                    (p[k] * t[k] - want[k]).abs() < 0.01,
                    "{t:?}: {p:?} against {want:?}"
                );
            }
        }
        let alone = fitted_texel(&stages, tex, [1.0; 4], &[]);
        let white = texel(&stages, tex, [1.0; 4], [1.0; 4]);
        for k in 0..4 {
            assert!((alone[k] - white[k]).abs() < 1e-6);
        }
    }

    /// The clouds' stages multiply by the tint; the dome's add the picture signed, which no
    /// picture times the tint follows.
    #[test]
    fn the_clouds_stages_scale_with_the_tint_and_the_domes_do_not() {
        let clouds = [stage(4, (2, 3), 4, (2, 3)), stage(3, (2, 1), 5, (2, 1))];
        let dome = [stage(5, (2, 3), 3, (2, 3)), stage(8, (2, 1), 3, (2, 1))];
        assert!(scales_with_the_tint(&clouds, [1.0; 4]));
        assert!(scales_with_the_tint(
            &[stage(5, (2, 3), 3, (2, 3))],
            [0.5, 0.5, 0.5, 1.0]
        ));
        assert!(!scales_with_the_tint(&dome, [0.5, 0.5, 0.5, 1.0]));
    }

    /// The dome under AC2's own tints through a day (dark blue night, gray dawn, pale violet day,
    /// warm dusk): its picture and tints fitted together come near AC2's texels at every hour, its
    /// bright texels near white by day where one picture fitted alone draws them as the bare tint.
    #[test]
    fn the_domes_picture_and_tints_fitted_together_come_near_ac2s_dome_at_every_hour() {
        let dome = [stage(5, (2, 3), 3, (2, 3)), stage(8, (2, 1), 3, (2, 1))];
        let d = [0.5, 0.5, 0.5, 1.0];
        let tints = [
            [0.05, 0.08, 0.15],
            [0.32, 0.32, 0.44],
            [0.56, 0.55, 0.80],
            [0.60, 0.59, 0.82],
            [0.83, 0.78, 0.89],
            [0.54, 0.55, 0.72],
            [0.21, 0.25, 0.40],
        ];
        // A pale, mostly bright picture, as the dome's is.
        let texels: Vec<([f32; 4], usize)> = (0..=10)
            .map(|k| {
                let v = 0.5 + 0.05 * k as f32;
                ([v, v, v * 0.97, 1.0], if v >= 0.85 { 8 } else { 1 })
            })
            .collect();
        // Hidden at night, as AC2's dome is.
        let shows = [0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0];
        let (pictures, drawn) = fitted_together(&dome, &texels, d, &tints, &shows);
        assert!(pictures
            .iter()
            .all(|p| p.iter().all(|x| (0.0..=1.0).contains(x))));
        assert!(drawn
            .iter()
            .all(|t| t.iter().all(|x| (0.0..=1.0).contains(x))));
        let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0).trunc() / 255.0;
        let ac2 = |tex: [f32; 4], t: [f32; 3]| {
            let tinted = [d[0] * t[0], d[1] * t[1], d[2] * t[2], d[3]];
            texel(&dome, tex, tinted, tinted.map(q))
        };
        let (mut together, mut alone, mut n) = (0.0f32, 0.0f32, 0.0f32);
        for ((tex, w), p) in texels.iter().zip(&pictures) {
            let single = fitted_texel(&dome, *tex, d, &tints);
            for ((t, g), s) in tints.iter().zip(&drawn).zip(shows) {
                if s == 0.0 {
                    continue;
                }
                let want = ac2(*tex, *t);
                for k in 0..3 {
                    let w = *w as f32;
                    together += w * (p[k] * g[k] - want[k]).abs();
                    alone += w * (single[k] * t[k] - want[k]).abs();
                    n += w;
                }
            }
        }
        let (together, alone) = (together / n, alone / n);
        assert!(
            together < 0.06,
            "fitted together, off by {together} on average"
        );
        assert!(together * 3.0 < alone, "{together} against {alone} alone");
        // By day the brightest texel draws near white, as AC2 draws it.
        let (top, day) = (pictures.last().unwrap(), drawn[3]);
        let white = ac2(texels.last().unwrap().0, tints[3]);
        for k in 0..3 {
            assert!(
                (top[k] * day[k] - white[k]).abs() < 0.1,
                "{top:?} x {day:?} against {white:?}"
            );
        }
    }

    fn at(x: f32, y: f32, z: f32) -> SwVertex {
        SwVertex {
            id: 0,
            position: Vec3::new(x, y, z),
            normal: Vec3::new(0.0, 0.0, 1.0),
            uvs: Vec::new(),
        }
    }

    /// AC2's clouds, a dish 8 to 83 meters out, are scaled out as far as AC1's sky lies, kept
    /// inside it; a dome already that far is left; a small light is scaled to AC1's distance.
    #[test]
    fn a_sky_model_is_scaled_out_as_far_as_ac1s_sky_lies() {
        let clouds = [at(0.0, 0.0, 8.0), at(83.0, 0.0, 0.0), at(0.0, -40.0, -6.0)];
        let k = distance_scale(&clouds);
        assert!((k * 83.0 - SKY_FAR).abs() < 1e-2, "{k}");
        assert!(k * 8.0 > 1000.0, "its nearest {} out", k * 8.0);
        let dome = [at(1600.0, 0.0, 0.0), at(0.0, 0.0, 1800.0)];
        assert_eq!(distance_scale(&dome), 1.0);
        let light = [at(1.0, 0.0, 0.0), at(0.0, 2.0, 0.0)];
        assert!((distance_scale(&light) * 1.0 - SKY_NEAR).abs() < 1e-2);
        assert_eq!(distance_scale(&[]), 1.0);
    }

    #[test]
    fn a_stage_whose_color_is_turned_off_ends_the_stages() {
        let stages = [stage(1, (2, 3), 2, (2, 3)), stage(2, (2, 1), 2, (2, 1))];
        let d = [0.1, 0.2, 0.3, 0.4];
        assert_eq!(texel(&stages, [1.0; 4], d, [1.0; 4]), d);
    }

    #[test]
    fn a_speed_slides_by_its_frequency_and_rates_and_a_steady_wave_by_its_base_rate() {
        let speed = Waveform {
            kind: 2,
            frequency: 0.01,
            base_rate: 0.002,
            ..Waveform::default()
        };
        let steady = Waveform {
            kind: 1,
            base: 5.0,
            base_rate: -0.003,
            ..Waveform::default()
        };
        assert!((rate(&speed) - 0.012).abs() < 1e-7);
        assert_eq!(rate(&steady), -0.003);
        let l = MaterialLayer {
            animations: vec![auberean_assets::material::StageAnimation {
                stage: 0,
                animation: CoordinateAnimation::Translate {
                    u: speed,
                    v: steady,
                },
            }],
            ..MaterialLayer::default()
        };
        let (u, v) = velocity(&l);
        assert!((u - 0.012).abs() < 1e-7 && v == -0.003);
    }

    #[test]
    fn an_additive_layer_is_added_by_its_alpha_and_a_blended_one_blended() {
        let l = |s, d| MaterialLayer {
            source_blend: s,
            dest_blend: d,
            ..MaterialLayer::default()
        };
        assert_eq!(
            surface_kind(&l(5, 2)).0,
            SURFACE_IMAGE | SURFACE_ALPHA | SURFACE_ADDITIVE
        );
        assert_eq!(surface_kind(&l(5, 6)).0, SURFACE_IMAGE | SURFACE_ALPHA);
        assert_eq!(surface_kind(&l(2, 1)).0, SURFACE_IMAGE);
    }
}
