//! The light inside: what lights the interior cells of a block and what stands in them.
//!
//! Inside there is no sun overhead. A block's information names its interior ambient light and its
//! interior fog; its light record holds its lights, each a point light, a directional light or a
//! sky light (a point light that shines with the day's sun and ambient), with the cells each
//! reaches and, per cell, the vertices of the cell's mesh it lights; when the record carries
//! sunlight it lists, for each of nine directions, the vertices the sun reaches from there.
//!
//! A cell's vertices start black and each light listing them adds its share (its color times its
//! intensity, times how squarely it meets the surface, over `1 + 10 d² / R²` for a distance `d`
//! and range `R`); the sun adds its share to the vertices its two nearest directions list. Then
//! the lights kept about the cell (the strongest where the cell is, at most six, of the lights a
//! frame inside links: the sky lights first, then the others) light every vertex the same way, as
//! many as the cell's program takes, and the interior ambient is added where the cell is drawn.
//! The same lights give each vertex the direction its normal map is lit from.

use std::collections::HashMap;

use auberean_assets::common::Waveform;
use auberean_assets::landblock::LightInfo;
use auberean_assets::property::{PropertyCollection, PropertyValue};
use auberean_primitives::random::Random;
use auberean_primitives::Vec3;

use crate::color::Color;
use crate::math::{add, dot, length, normalize, scale, sub, Mat4};
use crate::records::WorldRecords;
use crate::sky::{sun_directions, DayLight};

/// The light kinds.
const POINT: u32 = 0x4000_0001;
const DIRECTIONAL: u32 = 0x4000_0002;
const SKY: u32 = 0x4000_0003;
/// The group a light's properties are read from, and the properties.
const LIGHT_GROUP: u32 = 0x4000_0006;
const KIND: u32 = 0x4000_0002;
const DIRECTION: u32 = 0x4000_0003;
const RANGE: u32 = 0x4000_0004;
const COLOR: u32 = 0x4000_0008;
const INTENSITY: u32 = 0x4000_0009;
const SPECULAR: u32 = 0x4000_000A;
/// The block's interior ambient light and interior fog.
const AMBIENT: u32 = 0x4000_005A;
const FOG_KIND: u32 = 0x4000_0056;
const FOG_COLOR: u32 = 0x4000_0057;
const FOG_NEAR: u32 = 0x4000_0058;
const FOG_FAR: u32 = 0x4000_0059;
const FOG_LINEAR: u32 = 0x4000_0002;
/// The fog kind a block has when it names none.
const FOG_NONE: u32 = 0x4000_0001;
/// How many point lights light one draw.
pub const DRAW_LIGHTS: usize = 4;
/// How many lights a draw keeps.
const KEPT: usize = 6;
/// The nearest a light is ever taken to be, and the least range it is drawn with.
const NEAR: f32 = 0.0002;

/// What a light is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightKind {
    /// Shines from its place, fading with distance.
    Point,
    /// Shines along its direction everywhere.
    Directional,
    /// A point light that shines with the day's light outside.
    Sky,
}

/// One light of a block.
#[derive(Clone, Debug, PartialEq)]
pub struct Light {
    /// The cell it stands in, and its place in the block's frame.
    pub cell: u32,
    pub origin: Vec3,
    pub kind: LightKind,
    /// The direction a directional light shines along.
    pub direction: Vec3,
    pub color: Color,
    /// Its specular color.
    pub specular: Color,
    /// Its intensity and range as they start (their waveforms' bases).
    pub intensity: f32,
    pub range: f32,
    /// Its intensity as it moves (flickering), when it does: the waveform as stored.
    pub flicker: Option<Waveform>,
    /// The most its intensity reaches, when that is known.
    pub most_intensity: Option<f32>,
    /// The most its range reaches (0 when that is not known).
    pub most_range: f32,
    /// Per cell (by full id), the indices of the cell mesh's vertices it lights.
    pub lit: HashMap<u32, Vec<u32>>,
}

impl Light {
    /// A light standing at `origin` in cell `cell` made from `properties`: a point light of range
    /// 5, white, at intensity 0.5, taking the properties of its light group (its kind, direction,
    /// range, color, intensity and specular color). With it, its range's and its intensity's
    /// waveforms.
    // D2-13329, D2-13330, D2-13327, D2-03486, D2-13322
    #[must_use]
    pub fn of_properties(
        cell: u32,
        origin: Vec3,
        properties: &auberean_assets::property::PropertyCollection,
    ) -> (Self, Waveform, Waveform) {
        let mut range = Waveform {
            base: 5.0,
            ..Waveform::defaults()
        };
        let mut intensity = Waveform {
            base: 0.5,
            ..Waveform::defaults()
        };
        let mut light = Self {
            cell,
            origin,
            kind: LightKind::Point,
            direction: Vec3::ZERO,
            color: Color::WHITE,
            specular: Color::WHITE,
            intensity: 0.0,
            range: 0.0,
            flicker: None,
            most_intensity: None,
            most_range: 0.0,
            lit: HashMap::new(),
        };
        let group = properties.groups.iter().filter(|g| g.group == LIGHT_GROUP);
        for (id, v) in group.flat_map(|g| g.properties.iter().map(|p| (p.id, &p.value))) {
            match (id, v) {
                (KIND, PropertyValue::Enum(k)) => {
                    light.kind = match *k {
                        DIRECTIONAL => LightKind::Directional,
                        SKY => LightKind::Sky,
                        POINT => LightKind::Point,
                        _ => light.kind,
                    };
                }
                (DIRECTION, PropertyValue::Vector(d)) => {
                    light.direction = Vec3::new(d.x, d.y, d.z);
                }
                (RANGE, v) => range = wave(v).unwrap_or(range),
                (COLOR, PropertyValue::Color(c)) => light.color = color32(c.0),
                (SPECULAR, PropertyValue::Color(c)) => light.specular = color32(c.0),
                (INTENSITY, v) => intensity = wave(v).unwrap_or(intensity),
                _ => {}
            }
        }
        light.intensity = intensity.base;
        light.flicker = (intensity.kind != 1 || intensity.base_rate != 0.0).then_some(intensity);
        light.most_intensity = intensity.upper_bound();
        light.range = range.base.max(NEAR);
        light.most_range = range.upper_bound().unwrap_or(0.0);
        (light, range, intensity)
    }

    /// Whether it is a sky light.
    #[must_use]
    pub fn sky(&self) -> bool {
        self.kind == LightKind::Sky
    }
}

/// The light of a block's interior cells.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockLights {
    pub ambient: Color,
    /// The interior fog: its color, and where it starts and is whole.
    pub fog: Option<(Color, f32, f32)>,
    /// The interior fog's far distance, past which cells are left out of a frame seen from
    /// inside, whenever the block names a fog other than none (whether or not the fog is drawn).
    pub fog_cut: Option<f32>,
    /// The interior fog's color as stored (white when the block names none), alpha included: the
    /// background painted where cells were left out for the fog.
    pub fog_paint: Color,
    pub lights: Vec<Light>,
    /// Per cell (by its low word), the lights that reach it.
    pub lookup: HashMap<u32, Vec<usize>>,
    /// Whether the block has a light record.
    pub record: bool,
    /// When the record carries sunlight: per sun direction, per cell (by its low word), the
    /// vertices the sun reaches from there.
    pub sunlight: Option<Vec<HashMap<u32, Vec<u32>>>>,
    /// The block, as a cell id's high word.
    pub block: u32,
}

/// A cell's vertices lit by the lights its record lists for them, as baked when the cell is made
/// ready: each vertex's color, and its lean toward those lights (in the cell's own frame, before
/// the surface's).
#[derive(Clone, Debug, PartialEq)]
pub struct CellBake {
    pub color: Vec<[f32; 3]>,
    pub lean: Vec<Vec3>,
}

/// A light listing vertices `lit` of `vertices` adds `shine` to `color` by how squarely its light
/// meets each (and by the fade `toward` gives); an index past the mesh ends the list.
fn add_light(
    vertices: &[CellVertex],
    color: &mut [[f32; 3]],
    lit: &[u32],
    shine: Color,
    toward: &dyn Fn(Vec3) -> Option<(Vec3, f32)>,
) {
    for &i in lit {
        let Some(v) = vertices.get(i as usize) else {
            break;
        };
        let Some((l, k)) = toward(v.position) else {
            continue;
        };
        let facing = dot(v.normal, l);
        if facing > NEAR {
            let k = facing.min(1.0) * k;
            let c = &mut color[i as usize];
            c[0] = (c[0] + shine.r * k).clamp(0.0, 1.0);
            c[1] = (c[1] + shine.g * k).clamp(0.0, 1.0);
            c[2] = (c[2] + shine.b * k).clamp(0.0, 1.0);
        }
    }
}

/// The day outside as the lights inside take it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outside {
    /// The sun's color and its brightness.
    pub sun: Color,
    pub brightness: f32,
    /// The direction the sun's light travels, unit length.
    pub direction: Vec3,
    /// The light outdoors besides the sun: the ambient color times its brightness.
    pub ambient: Color,
}

impl Outside {
    /// The day outside of the day's light `day`.
    #[must_use]
    pub fn of(day: &DayLight) -> Self {
        Self {
            sun: day.direct_color,
            brightness: day.direct_brightness,
            direction: day.light_direction,
            ambient: day.ambient(),
        }
    }

    /// What a sky light shines with before its own intensity: the sun's color times its
    /// brightness, plus the ambient light outdoors.
    #[must_use]
    pub fn sky(&self) -> Color {
        let (s, k, a) = (self.sun, self.brightness, self.ambient);
        Color::new(s.r * k + a.r, s.g * k + a.g, s.b * k + a.b, 1.0)
    }
}

/// A light as the device takes it for a draw: its place in the block's frame, its diffuse and
/// specular colors and its range (its light fading as `1 / (1 + 10 d² / R²)`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeviceLight {
    pub origin: Vec3,
    pub diffuse: Color,
    pub specular: Color,
    pub range: f32,
    /// A light whose intensity moves: its diffuse color is taken afresh each frame.
    pub flicker: Option<Flicker>,
}

/// A light whose intensity moves as the client runs: which light it is (its block and place in
/// the block's list), its color before its intensity, and the intensity's waveform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flicker {
    pub key: u32,
    pub color: Color,
    pub intensity: Waveform,
}

impl Flicker {
    /// The intensity `t` seconds into the client's run, rolling `rng` for any noise.
    #[must_use]
    pub fn intensity(&self, t: f64, rng: &auberean_primitives::random::Random) -> f32 {
        crate::wave::value(&self.intensity, t, rng)
    }

    /// Its color at intensity `i`.
    #[must_use]
    pub fn shine(&self, i: f32) -> Color {
        let c = self.color;
        Color::new(c.r * i, c.g * i, c.b * i, 1.0)
    }
}

/// How many of the lights about a particle system light its particles.
pub const PARTICLE_LIGHTS: usize = 5;

/// A light as a lit particle takes it: its place in the frame the particles are in, its color
/// times its intensity, and the range over which it fades as `1 / (1 + 10 d² / R²)` (none for a
/// light that does not fade with distance: a directional light, the sun).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleLight {
    pub origin: Vec3,
    pub color: Color,
    pub range: Option<f32>,
    /// A light whose intensity moves: its color is taken afresh for each particle.
    pub flicker: Option<Flicker>,
}

impl ParticleLight {
    /// The light it gives a particle at `at`, `t` seconds into the client's run (a flickering
    /// light's intensity taken then, rolling `rng` for any noise).
    // D2-13326
    #[must_use]
    pub fn at(&self, at: Vec3, t: f64, rng: &auberean_primitives::random::Random) -> Color {
        let fade = self.range.map_or(1.0, |r| {
            let d = length(sub(self.origin, at));
            1.0 / ((d / (r * r)) * d * 10.0 + 1.0)
        });
        let c = match &self.flicker {
            Some(f) => f.shine(f.intensity(t, rng)),
            None => self.color,
        };
        Color::new(c.r * fade, c.g * fade, c.b * fade, 1.0)
    }
}

/// The light a lit particle at `at` takes from `lights` (the first [`PARTICLE_LIGHTS`] of them)
/// `t` seconds into the client's run: the sum of what each gives it, with no light of its own
/// besides; its alpha is 1.
// D2-10250
#[must_use]
pub fn particle_light(
    lights: &[ParticleLight],
    at: Vec3,
    t: f64,
    rng: &auberean_primitives::random::Random,
) -> Color {
    let mut sum = Color::new(0.0, 0.0, 0.0, 1.0);
    for l in lights.iter().take(PARTICLE_LIGHTS) {
        let c = l.at(at, t, rng);
        sum = Color::new(sum.r + c.r, sum.g + c.g, sum.b + c.b, 1.0);
    }
    sum
}

/// One vertex of a cell's mesh as stored, in the cell's own frame: its place, its normal, and the
/// surface's own frame for its normal map (the directions in which its coordinates' u and v grow).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub tangent_s: Vec3,
    pub tangent_t: Vec3,
}

/// The light of one vertex of a cell: its color, and the unit direction its normal map is lit
/// from, in the surface's own frame (u, v, normal).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VertexLight {
    pub color: [f32; 3],
    pub toward: Vec3,
}

fn props(p: &PropertyCollection) -> impl Iterator<Item = (u32, &PropertyValue)> {
    p.groups
        .iter()
        .flat_map(|g| g.properties.iter().map(|p| (p.id, &p.value)))
}

fn color32(c: u32) -> Color {
    let ch = |s: u32| ((c >> s) & 0xFF) as f32 / 255.0;
    Color::new(ch(16), ch(8), ch(0), ch(24))
}

fn number(v: &PropertyValue) -> Option<f32> {
    match v {
        PropertyValue::Float(f) => Some(*f),
        PropertyValue::Waveform(w) => Some(w.base),
        #[allow(clippy::cast_precision_loss)]
        PropertyValue::Integer(i) => Some(*i as f32),
        _ => None,
    }
}

fn wave(v: &PropertyValue) -> Option<Waveform> {
    match v {
        PropertyValue::Waveform(w) => Some(*w),
        v => number(v).map(|base| Waveform {
            base,
            ..Waveform::defaults()
        }),
    }
}

/// `w` held to `0..=1` the original's way: anything not below 1 (a value that is not a number
/// too) is 1, then anything not above 0 is 0.
fn unit(w: f32) -> f32 {
    let t = if w < 1.0 { w } else { 1.0 };
    if t > 0.0 {
        t
    } else {
        0.0
    }
}

/// The vertex indices a packed list names, as 16-bit words in the run-length form.
// D2-03526
#[must_use]
pub fn unpack_indices(packed: &[u8]) -> Vec<u32> {
    let words: Vec<u16> = packed
        .as_chunks::<2>()
        .0
        .iter()
        .map(|w| u16::from_le_bytes(*w))
        .collect();
    auberean_assets::rle::decode(&words)
        .into_iter()
        .map(u32::from)
        .collect()
}

/// The length of a landblock's side, in meters.
const BLOCK_SIDE: f32 = 160.0;

/// Whether cell `id` (its full id) is an interior cell.
fn inside(id: u32) -> bool {
    id & 0xFFFF >= 0x100
}

/// Where the frame of the block of cell `from` (a full id) stands in the frame of the block of
/// cell `to`.
#[must_use]
pub fn block_offset(to: u32, from: u32) -> Vec3 {
    let block = |id: u32| (f64::from(id >> 24), f64::from((id >> 16) & 0xFF));
    let ((fx, fy), (tx, ty)) = (block(to), block(from));
    #[allow(clippy::cast_possible_truncation)] // a few blocks apart
    Vec3::new(
        ((tx - fx) * f64::from(BLOCK_SIDE)) as f32,
        ((ty - fy) * f64::from(BLOCK_SIDE)) as f32,
        0.0,
    )
}

/// Of the lights objects carry with them (`carried`: an effect's light goes with its object, its
/// place in its own cell's block), those that reach a draw in cell `cell` (its full id) at `at` in
/// its block's frame, reaching `radius` about it: a point light only on the same side (inside or
/// out) as the draw, a sky light only for a draw inside, a directional light always; each within
/// its most range and the radius, its place taken across the blocks between their cells. By their
/// place in `carried`, in its order.
// D2-10619
#[must_use]
pub fn carried_in_reach(carried: &[Light], cell: u32, at: Vec3, radius: f32) -> Vec<usize> {
    carried
        .iter()
        .enumerate()
        .filter(|(_, l)| match l.kind {
            LightKind::Point => inside(l.cell) == inside(cell),
            LightKind::Sky => inside(cell),
            LightKind::Directional => true,
        })
        .filter(|(_, l)| {
            length(sub(add(l.origin, block_offset(cell, l.cell)), at)) < l.most_range + radius
        })
        .map(|(k, _)| k)
        .collect()
}

/// Whether any of `carried` reaches a draw in cell `cell` at `at` reaching `radius` about it (see
/// [`carried_in_reach`]).
#[must_use]
pub fn any_carried_in_reach(carried: &[Light], cell: u32, at: Vec3, radius: f32) -> bool {
    carried.iter().any(|l| {
        let side = match l.kind {
            LightKind::Point => inside(l.cell) == inside(cell),
            LightKind::Sky => inside(cell),
            LightKind::Directional => true,
        };
        side && length(sub(add(l.origin, block_offset(cell, l.cell)), at)) < l.most_range + radius
    })
}

/// A light a draw keeps: one of its block's (by its place in the block's lights) or one an
/// object carries (by its place in the frame's carried lights).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kept {
    Block(usize),
    Carried(usize),
}

/// The lights a draw keeps once the lights objects carry are taken in: `kept`, the lights of the
/// block (`lights`) it keeps (see [`SceneLights::keep`]), then those of `carried` that reach it (see
/// [`carried_in_reach`]: the draw in cell `cell` at `at` in its block's frame, reaching `radius`);
/// when any does, all of them sorted by their weight at the draw, strongest first (the original's
/// sort, see [`SceneLights::keep`]). A draw no carried light reaches keeps what it kept.
// D2-10630, D2-10619, D2-10628
#[must_use]
pub fn keep_carried(
    lights: Option<&BlockLights>,
    kept: &[usize],
    carried: &[Light],
    cell: u32,
    at: Vec3,
    radius: f32,
    outside: &Outside,
) -> Vec<Kept> {
    let mut all: Vec<Kept> = kept.iter().map(|&k| Kept::Block(k)).collect();
    let reach = carried_in_reach(carried, cell, at, radius);
    if reach.is_empty() {
        return all;
    }
    all.extend(reach.into_iter().map(Kept::Carried));
    let weight = |k: &Kept| match *k {
        Kept::Block(i) => lights.map_or(0.0, |b| {
            let l = &b.lights[i];
            BlockLights::weight_of(l, length(sub(l.origin, at)), outside)
        }),
        Kept::Carried(i) => {
            let l = &carried[i];
            let d = length(sub(add(l.origin, block_offset(cell, l.cell)), at));
            BlockLights::weight_of(l, d, outside)
        }
    };
    let w: Vec<(Kept, f32)> = all.iter().map(|k| (*k, weight(k))).collect();
    let at_of = |k: &Kept| w.iter().find(|(x, _)| x == k).map_or(0.0, |(_, v)| *v);
    original_sort(&mut all, |a, b| at_of(b) < at_of(a));
    all
}

/// A light an object carries as the device takes it for a draw in cell `cell`: its place in the
/// draw's block's frame, its color times its intensity, its specular color, its range.
fn carried_device(l: &Light, cell: u32) -> DeviceLight {
    DeviceLight {
        origin: add(l.origin, block_offset(cell, l.cell)),
        diffuse: Color::new(
            l.color.r * l.intensity,
            l.color.g * l.intensity,
            l.color.b * l.intensity,
            1.0,
        ),
        specular: l.specular,
        range: l.range,
        flicker: None,
    }
}

/// The first `points` of the lights `kept` (see [`keep_carried`]) that are not directional, as the
/// device takes them for a draw in cell `cell` (see [`BlockLights::device_lights`]; a carried light
/// shines its color times its intensity as it stands).
// D2-13334, D2-10630
#[must_use]
pub fn device_lights_kept(
    lights: Option<&BlockLights>,
    kept: &[Kept],
    carried: &[Light],
    cell: u32,
    outside: &Outside,
    points: usize,
) -> Vec<DeviceLight> {
    kept.iter()
        .filter_map(|k| match *k {
            Kept::Block(i) => {
                let b = lights?;
                let l = &b.lights[i];
                (l.kind != LightKind::Directional).then(|| b.device_light(i, l, outside))
            }
            Kept::Carried(i) => {
                let l = &carried[i];
                (l.kind != LightKind::Directional).then(|| carried_device(l, cell))
            }
        })
        .take(points)
        .collect()
}

/// The lights `kept` about a particle system (see [`keep_carried`]) as its lit particles take
/// them, the first [`PARTICLE_LIGHTS`] (see [`BlockLights::particle_lights`]): a carried light its
/// color times its intensity as it stands, fading over its range.
// D2-13332, D2-10631
#[must_use]
pub fn particle_lights_kept(
    lights: Option<&BlockLights>,
    kept: &[Kept],
    carried: &[Light],
    cell: u32,
) -> Vec<ParticleLight> {
    kept.iter()
        .take(PARTICLE_LIGHTS)
        .filter_map(|k| match *k {
            Kept::Block(i) => {
                let b = lights?;
                let l = &b.lights[i];
                (!l.sky()).then(|| b.particle_light(i, l))
            }
            Kept::Carried(i) => {
                let d = carried_device(&carried[i], cell);
                Some(ParticleLight {
                    origin: d.origin,
                    color: d.diffuse,
                    range: (carried[i].kind == LightKind::Point).then_some(d.range),
                    flicker: None,
                })
            }
        })
        .collect()
}

/// The lights a frame inside draws with, in the order they are linked (the block's sky lights,
/// then its others, each in the order the block's cells list them) and as each draw's sorting
/// leaves them.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneLights {
    order: Vec<usize>,
}

impl SceneLights {
    /// The lights linked while the cells' own meshes are drawn: the sky lights alone. The rest are
    /// linked after them, for what stands in the cells.
    // D2-03520, D2-03519
    #[must_use]
    pub fn sky_alone(&self, lights: &BlockLights) -> Self {
        Self {
            order: self
                .order
                .iter()
                .copied()
                .filter(|&k| lights.lights[k].kind == LightKind::Sky)
                .collect(),
        }
    }

    /// The lights a draw at `at` reaching `radius` around it keeps, by their place in the block's
    /// lights, strongest first: the frame's lights sorted by their weight there (and left in that
    /// order for the next draw), then those whose most range reaches the draw, at most six.
    ///
    /// The sort is the original's, which does not keep the order of lights of equal weight (the
    /// lights out of reach all weigh nothing), so which of those a draw keeps follows from the
    /// order the draws before it left.
    // D2-10628, D2-10538, D2-10629
    pub fn keep(
        &mut self,
        lights: &BlockLights,
        at: Vec3,
        radius: f32,
        outside: &Outside,
    ) -> Vec<usize> {
        let mut reach = vec![false; lights.lights.len()];
        let mut weight = vec![0.0f32; lights.lights.len()];
        let mut placed = 0;
        for &k in &self.order {
            let l = &lights.lights[k];
            if l.kind == LightKind::Directional {
                reach[k] = true;
            } else {
                let d = length(sub(l.origin, at));
                reach[k] = d < l.most_range + radius;
                placed += 1;
            }
            weight[k] = lights.weight(l, length(sub(l.origin, at)), outside);
        }
        if placed > 0 {
            original_sort(&mut self.order, |a, b| weight[*b] < weight[*a]);
        }
        self.order
            .iter()
            .copied()
            .filter(|&k| reach[k])
            .take(KEPT)
            .collect()
    }
}

/// The original's sort (an introspective sort finished by insertion), whose order of equal
/// elements the lights a draw keeps depend on: `less(a, b)` says `a` goes before `b`.
fn original_sort<T: Copy>(v: &mut [T], less: impl Fn(&T, &T) -> bool) {
    if v.len() < 2 {
        return;
    }
    let mut depth = 0;
    let mut n = v.len();
    while n != 1 {
        n >>= 1;
        depth += 1;
    }
    introsort_loop(v, 0, v.len(), depth * 2, &less);
    if v.len() > 16 {
        insertion_sort(v, 0, 16, &less);
        for i in 16..v.len() {
            unguarded_linear_insert(v, i, v[i], &less);
        }
    } else {
        insertion_sort(v, 0, v.len(), &less);
    }
}

fn introsort_loop<T: Copy>(
    v: &mut [T],
    first: usize,
    mut last: usize,
    mut depth: usize,
    less: &impl Fn(&T, &T) -> bool,
) {
    while last - first > 16 {
        if depth == 0 {
            heap_sort(&mut v[first..last], less);
            return;
        }
        depth -= 1;
        let mid = first + (last - first) / 2;
        let pivot = median(v[first], v[mid], v[last - 1], less);
        let cut = unguarded_partition(v, first, last, pivot, less);
        introsort_loop(v, cut, last, depth, less);
        last = cut;
    }
}

fn median<T: Copy>(a: T, b: T, c: T, less: &impl Fn(&T, &T) -> bool) -> T {
    if less(&a, &b) {
        if less(&b, &c) {
            b
        } else if less(&a, &c) {
            c
        } else {
            a
        }
    } else if less(&a, &c) {
        a
    } else if less(&b, &c) {
        c
    } else {
        b
    }
}

fn unguarded_partition<T: Copy>(
    v: &mut [T],
    mut first: usize,
    mut last: usize,
    pivot: T,
    less: &impl Fn(&T, &T) -> bool,
) -> usize {
    loop {
        while less(&v[first], &pivot) {
            first += 1;
        }
        last -= 1;
        while less(&pivot, &v[last]) {
            last -= 1;
        }
        if first >= last {
            return first;
        }
        v.swap(first, last);
        first += 1;
    }
}

fn insertion_sort<T: Copy>(v: &mut [T], first: usize, last: usize, less: &impl Fn(&T, &T) -> bool) {
    for i in first + 1..last {
        let x = v[i];
        if less(&x, &v[first]) {
            v.copy_within(first..i, first + 1);
            v[first] = x;
        } else {
            unguarded_linear_insert(v, i, x, less);
        }
    }
}

fn unguarded_linear_insert<T: Copy>(
    v: &mut [T],
    mut last: usize,
    x: T,
    less: &impl Fn(&T, &T) -> bool,
) {
    while less(&x, &v[last - 1]) {
        v[last] = v[last - 1];
        last -= 1;
    }
    v[last] = x;
}

/// The heap sort the original falls back on when its partitions go too deep.
fn heap_sort<T: Copy>(v: &mut [T], less: &impl Fn(&T, &T) -> bool) {
    let n = v.len();
    if n < 2 {
        return;
    }
    let mut parent = (n - 2) / 2;
    loop {
        let x = v[parent];
        adjust_heap(v, parent, n, x, less);
        if parent == 0 {
            break;
        }
        parent -= 1;
    }
    let mut end = n;
    while end > 1 {
        end -= 1;
        let x = v[end];
        v[end] = v[0];
        adjust_heap(v, 0, end, x, less);
    }
}

fn adjust_heap<T: Copy>(
    v: &mut [T],
    mut hole: usize,
    len: usize,
    x: T,
    less: &impl Fn(&T, &T) -> bool,
) {
    let top = hole;
    let mut child = 2 * hole + 2;
    while child < len {
        if less(&v[child], &v[child - 1]) {
            child -= 1;
        }
        v[hole] = v[child];
        hole = child;
        child = 2 * (child + 1);
    }
    if child == len {
        v[hole] = v[child - 1];
        hole = child - 1;
    }
    let mut parent = if hole > 0 { (hole - 1) / 2 } else { 0 };
    while hole > top && less(&v[parent], &x) {
        v[hole] = v[parent];
        hole = parent;
        parent = if hole > 0 { (hole - 1) / 2 } else { 0 };
    }
    v[hole] = x;
}

impl BlockLights {
    /// The lights of an interface scene: its light entities, all linked for each draw, with no
    /// ambient light of their own and nothing from outside.
    // D2-10618
    #[must_use]
    pub fn of_scene(lights: Vec<Light>) -> Self {
        let all = (0..lights.len()).collect();
        Self {
            ambient: Color::new(0.0, 0.0, 0.0, 1.0),
            fog_paint: Color::new(1.0, 1.0, 1.0, 1.0),
            lights,
            lookup: HashMap::from([(0, all)]),
            ..Self::default()
        }
    }

    /// The light of block `(x, y)`'s interior cells, from its information and its light record.
    // D2-11007, D2-13327, D2-03578, D2-13330, D2-13329, D2-03899, D2-03734, D2-03740, D2-03746
    #[must_use]
    pub fn of(records: &WorldRecords, x: i32, y: i32) -> Option<Self> {
        let info = records.landblock_info(x, y)?;
        #[allow(clippy::cast_sign_loss)]
        let block = ((x as u32 & 0xFF) << 24) | ((y as u32 & 0xFF) << 16);
        let mut out = Self {
            ambient: Color::new(0.0, 0.0, 0.0, 1.0),
            block,
            ..Self::default()
        };
        let mut fog = (false, Color::new(0.0, 0.0, 0.0, 1.0), 0.0, 0.0);
        let mut kind = FOG_NONE;
        out.fog_paint = Color::new(1.0, 1.0, 1.0, 1.0);
        for (id, v) in props(&info.properties) {
            match (id, v) {
                (AMBIENT, PropertyValue::Color(c)) => out.ambient = color32(c.0),
                (FOG_KIND, PropertyValue::Enum(k)) => {
                    kind = *k;
                    fog.0 = *k == FOG_LINEAR;
                }
                (FOG_COLOR, PropertyValue::Color(c)) => {
                    fog.1 = color32(c.0);
                    out.fog_paint = fog.1;
                }
                (FOG_NEAR, v) => fog.2 = number(v).unwrap_or(0.0),
                (FOG_FAR, v) => fog.3 = number(v).unwrap_or(0.0),
                _ => {}
            }
        }
        if fog.0 && fog.3 > fog.2 {
            out.fog = Some((fog.1, fog.2, fog.3));
        }
        // D2-03601
        if kind != FOG_NONE {
            out.fog_cut = Some(fog.3);
        }
        if info.light_info.0 != 0 {
            if let Some(li) = records.light_info(x, y) {
                out.record = true;
                out.add_lights(&li);
            }
        }
        Some(out)
    }

    fn add_lights(&mut self, li: &LightInfo) {
        for l in &li.lights {
            let (mut light, _, _) = Light::of_properties(
                l.cell.0,
                Vec3::new(l.origin.x, l.origin.y, l.origin.z),
                &l.properties,
            );
            for c in &l.lit {
                if c.count > 0 {
                    light.lit.insert(c.cell, unpack_indices(&c.packed));
                }
            }
            self.lights.push(light);
        }
        for (cell, ls) in &li.lookup {
            self.lookup.insert(
                *cell,
                ls.iter()
                    .filter_map(|&k| usize::try_from(k).ok())
                    .filter(|&k| k < self.lights.len())
                    .collect(),
            );
        }
        self.sunlight = li.sunlight.as_ref().map(|dirs| {
            dirs.iter()
                .map(|cells| {
                    cells
                        .iter()
                        .filter(|c| c.count > 0)
                        .map(|c| (c.cell & 0xFFFF, unpack_indices(&c.packed)))
                        .collect()
                })
                .collect()
        });
    }

    /// The lights a frame inside links, in order: the lights each cell lists, by cell, each once,
    /// the sky lights first, then the rest.
    // D2-03578, D2-03520, D2-03519, D2-10618
    #[must_use]
    pub fn scene(&self) -> SceneLights {
        let mut cells: Vec<&u32> = self.lookup.keys().collect();
        cells.sort_unstable();
        let mut statics: Vec<usize> = Vec::new();
        for c in cells {
            for &k in &self.lookup[c] {
                if !statics.contains(&k) {
                    statics.push(k);
                }
            }
        }
        let (sky, rest): (Vec<usize>, Vec<usize>) =
            statics.into_iter().partition(|&k| self.lights[k].sky());
        SceneLights {
            order: sky.into_iter().chain(rest).collect(),
        }
    }

    /// Light `k`'s flicker, shining `color` before its intensity, when its intensity moves.
    fn flicker(&self, k: usize, l: &Light, color: Color) -> Option<Flicker> {
        let w = l.flicker?;
        Some(Flicker {
            key: self.block | (u32::try_from(k).unwrap_or(0xFFFF) & 0xFFFF),
            color,
            intensity: w,
        })
    }

    /// A light's color as it shines: a sky light's is the day's sun and ambient outside times its
    /// intensity; another light's its color times its intensity.
    // D2-13334
    fn shine(l: &Light, outside: &Outside) -> Color {
        let c = if l.sky() { outside.sky() } else { l.color };
        Color::new(c.r * l.intensity, c.g * l.intensity, c.b * l.intensity, 1.0)
    }

    /// The first `points` of the lights `kept` that are not directional, as the device takes them:
    /// a sky light shines the day's sun and ambient outside and is specular with the sun's color;
    /// another light shines its color and is specular with its specular color; each diffuse color
    /// times the light's intensity.
    // D2-13334, D2-10630
    #[must_use]
    pub fn device_lights(
        &self,
        kept: &[usize],
        outside: &Outside,
        points: usize,
    ) -> Vec<DeviceLight> {
        kept.iter()
            .map(|&k| (k, &self.lights[k]))
            .filter(|(_, l)| l.kind != LightKind::Directional)
            .take(points)
            .map(|(k, l)| self.device_light(k, l, outside))
            .collect()
    }

    /// Light `k` (`l`) as the device takes it (see [`Self::device_lights`]).
    fn device_light(&self, k: usize, l: &Light, outside: &Outside) -> DeviceLight {
        DeviceLight {
            origin: l.origin,
            diffuse: Self::shine(l, outside),
            specular: if l.sky() { outside.sun } else { l.specular },
            range: l.range,
            flicker: self.flicker(k, l, if l.sky() { outside.sky() } else { l.color }),
        }
    }

    /// The lights `kept` about a particle system inside (see [`SceneLights::keep`]) as its lit
    /// particles take them, the first [`PARTICLE_LIGHTS`]: a point light its color times its
    /// intensity, fading over its range; a directional light the same, not fading; a sky light
    /// takes its place among them but gives nothing (its color there is the sun's own ambient, which
    /// is black: the day outside lights particles inside only through the cells).
    // D2-13332, D2-10631
    #[must_use]
    pub fn particle_lights(&self, kept: &[usize]) -> Vec<ParticleLight> {
        kept.iter()
            .take(PARTICLE_LIGHTS)
            .map(|&k| (k, &self.lights[k]))
            .filter(|(_, l)| !l.sky())
            .map(|(k, l)| self.particle_light(k, l))
            .collect()
    }

    /// Light `k` (`l`) as a lit particle takes it (see [`Self::particle_lights`]).
    fn particle_light(&self, k: usize, l: &Light) -> ParticleLight {
        ParticleLight {
            origin: l.origin,
            color: Color::new(
                l.color.r * l.intensity,
                l.color.g * l.intensity,
                l.color.b * l.intensity,
                1.0,
            ),
            range: (l.kind == LightKind::Point).then_some(l.range),
            flicker: self.flicker(k, l, l.color),
        }
    }

    /// How strongly light `l` lights a draw `d` from it, held to `0..=1`: how far within its range
    /// the draw stands, times the most its intensity reaches (1 when that is not known), times its
    /// color's mean; a sky light's color is the sun's own, times the sun's brightness; a
    /// directional light weighs 1.
    // D2-13333
    #[must_use]
    pub fn weight(&self, l: &Light, d: f32, outside: &Outside) -> f32 {
        Self::weight_of(l, d, outside)
    }

    /// How strongly light `l` lights a draw `d` from it (see [`Self::weight`]): the same for a
    /// block's light and one an object carries.
    // D2-13333
    #[must_use]
    pub fn weight_of(l: &Light, d: f32, outside: &Outside) -> f32 {
        let mean = |c: Color| (c.r + c.g + c.b) * 0.333_333_34;
        let (c, k) = match l.kind {
            LightKind::Directional => return 1.0,
            LightKind::Point => (mean(l.color), 1.0),
            LightKind::Sky => (mean(outside.sun), outside.brightness),
        };
        let i = l.most_intensity.unwrap_or(1.0) * k;
        unit((l.range - d) / l.range * i * c)
    }

    /// The lights that light a draw at `at` reaching `radius` around it, strongest first, as
    /// many as a program with four point lights takes, each its place, its color as it shines and
    /// its range.
    #[must_use]
    pub fn chosen(&self, at: Vec3, radius: f32, outside: &Outside) -> Vec<(Vec3, Color, f32)> {
        let mut v = self.cached(at, radius, outside);
        v.truncate(DRAW_LIGHTS);
        v
    }

    /// The lights a draw at `at` reaching `radius` around it keeps when it is the frame's first
    /// draw (see [`SceneLights::keep`]), strongest first; each its place, its color as it shines
    /// and its range.
    #[must_use]
    pub fn cached(&self, at: Vec3, radius: f32, outside: &Outside) -> Vec<(Vec3, Color, f32)> {
        self.scene()
            .keep(self, at, radius, outside)
            .into_iter()
            .map(|k| {
                let l = &self.lights[k];
                (l.origin, Self::shine(l, outside), l.range)
            })
            .collect()
    }

    /// The light of each vertex of interior cell `cell` (its full id), placed in the block by
    /// `frame` (a rotation and a translation), its vertices `vertices` in its own frame; `kept` the
    /// lights the cell keeps (see [`SceneLights::keep`]), `outside` the day outside, `points` how
    /// many point lights the cell's program takes.
    ///
    /// A cell's vertices start black when the block has a light record (the interior ambient is
    /// added where the cell is drawn), a mid gray when it has none. Each light reaching the cell
    /// adds its share to the vertices it lists: a point light by how squarely it meets the surface
    /// over `1 + 10 d² / R²`, a directional one by how squarely it meets it times its intensity,
    /// a sky light none. Each listed light also leans the direction the vertex's normal map is lit
    /// from toward itself, by `1 - d² / R²` with `R` its most range (going negative, and turning
    /// the lean away, past it), after a start of a hundredth of the normal, then seen in the
    /// surface's own frame. Then the sun adds its color, by how squarely its light meets the
    /// surface, times its brightness, to the vertices the record's nearest of its nine directions
    /// lists, times the cosine between the sun's way and that direction; and to the second
    /// nearest's, times one less that cosine. Then the first `points` of the kept lights that are
    /// not directional light every vertex (`max(n·l, 0)` over `1 + 10 d² / R²`) and lean its
    /// direction toward themselves, each by `max(1 - d² / R², 0)`; the direction is made a unit at
    /// the end.
    // D2-03609, D2-03517, D2-03526, D2-03559, D2-03567, D2-13326
    #[must_use]
    pub fn cell_vertices(
        &self,
        cell: u32,
        frame: &Mat4,
        vertices: &[CellVertex],
        kept: &[usize],
        outside: &Outside,
        points: usize,
    ) -> Vec<VertexLight> {
        let bake = self.cell_bake(cell, frame, vertices, None);
        self.cell_light(&bake, cell, frame, vertices, kept, outside, points)
    }

    /// The light cell `cell`'s record lists for its vertices, baked once as the cell is made ready
    /// to draw (see [`Self::cell_vertices`]): each listed point or directional light at its
    /// intensity at that moment, `at` (the client's running time, and its generator for a light
    /// whose intensity is noise, rolled afresh for each vertex), or at its intensity's start with
    /// none; and the lean toward each listed light.
    // D2-03609, D2-03526, D2-03494, D2-03495, D2-13326, D2-13338, D2-03898, D2-03915, D2-03916
    #[must_use]
    pub fn cell_bake(
        &self,
        cell: u32,
        frame: &Mat4,
        vertices: &[CellVertex],
        at: Option<(f64, &Random)>,
    ) -> CellBake {
        let start = if self.record { 0.0 } else { 0.4 };
        let mut color = vec![[start; 3]; vertices.len()];
        let local = |d: Vec3| sub(frame.rigid_preimage(d), frame.rigid_preimage(Vec3::ZERO));
        // A light's intensity as the bake takes it, for each vertex it reaches.
        let intensity = |l: &Light| match (at, &l.flicker) {
            (Some((t, rng)), Some(w)) => crate::wave::value(w, t, rng),
            _ => l.intensity,
        };
        let mut lean: Vec<Vec3> = vertices.iter().map(|v| scale(v.normal, 0.01)).collect();
        for &k in self
            .lookup
            .get(&(cell & 0xFFFF))
            .map_or(&[][..], |v| v.as_slice())
        {
            let l = &self.lights[k];
            let Some(lit) = l.lit.get(&cell) else {
                continue;
            };
            let from = frame.rigid_preimage(l.origin);
            match l.kind {
                LightKind::Point => add_light(vertices, &mut color, lit, l.color, &|p| {
                    let to = sub(from, p);
                    let d = length(to).max(NEAR);
                    let fade = intensity(l) / (1.0 + 10.0 * d * d / (l.range * l.range));
                    Some((scale(to, 1.0 / d), fade))
                }),
                LightKind::Directional => {
                    let way = local(scale(l.direction, -1.0));
                    add_light(vertices, &mut color, lit, l.color, &|_| {
                        Some((way, intensity(l)))
                    });
                }
                LightKind::Sky => {}
            }
            // A light whose most range is not known leans nothing.
            let most = l.most_range * l.most_range;
            if most <= 0.0 {
                continue;
            }
            for &i in lit {
                let Some(v) = vertices.get(i as usize) else {
                    continue;
                };
                let to = sub(from, v.position);
                let w = 1.0 - dot(to, to) / most;
                let m = &mut lean[i as usize];
                *m = add(*m, scale(normalize(to), w));
            }
        }
        CellBake { color, lean }
    }

    /// Cell `cell`'s vertices lit on top of its bake `bake` (see [`Self::cell_vertices`]): the sun
    /// through its record's directions, and the first `points` of the kept lights.
    // D2-03559, D2-03567, D2-03517
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn cell_light(
        &self,
        bake: &CellBake,
        cell: u32,
        frame: &Mat4,
        vertices: &[CellVertex],
        kept: &[usize],
        outside: &Outside,
        points: usize,
    ) -> Vec<VertexLight> {
        let sunlit = self.cell_sunlit(bake, cell, frame, vertices, outside);
        let device: Vec<(Vec3, Color, f32)> = kept
            .iter()
            .map(|&k| &self.lights[k])
            .filter(|l| l.kind != LightKind::Directional)
            .take(points)
            .map(|l| {
                (
                    frame.rigid_preimage(l.origin),
                    Self::shine(l, outside),
                    l.range,
                )
            })
            .collect();
        vertices
            .iter()
            .zip(sunlit)
            .map(|(v, lit)| {
                let (mut c, mut m) = (lit.color, lit.toward);
                let (s, t, n) = (
                    normalize(v.tangent_s),
                    normalize(v.tangent_t),
                    normalize(v.normal),
                );
                for &(at, shine, range) in &device {
                    let to = sub(at, v.position);
                    let d2 = dot(to, to);
                    let k = dot(n, normalize(to)).max(0.0) / (1.0 + 10.0 * d2 / (range * range));
                    c[0] = (c[0] + shine.r * k).clamp(0.0, 1.0);
                    c[1] = (c[1] + shine.g * k).clamp(0.0, 1.0);
                    c[2] = (c[2] + shine.b * k).clamp(0.0, 1.0);
                    let seen = Vec3::new(dot(s, to), dot(t, to), dot(n, to));
                    let f = (1.0 - dot(seen, seen) / (range * range)).max(0.0);
                    m = add(m, scale(normalize(seen), f));
                }
                VertexLight {
                    color: c,
                    toward: if length(m) > 0.0 {
                        normalize(m)
                    } else {
                        Vec3::new(0.0, 0.0, 1.0)
                    },
                }
            })
            .collect()
    }

    /// Cell `cell`'s vertices as a device draws them before its kept lights (see
    /// [`Self::cell_vertices`]): the bake `bake` with the sun through its record's directions on
    /// top; each vertex's lean in the surface's own frame, not yet made a unit, which the kept lights
    /// add to as they shine at the moment.
    // D2-03559, D2-03567
    #[must_use]
    pub fn cell_sunlit(
        &self,
        bake: &CellBake,
        cell: u32,
        frame: &Mat4,
        vertices: &[CellVertex],
        outside: &Outside,
    ) -> Vec<VertexLight> {
        let mut color = bake.color.clone();
        let lean = &bake.lean;
        let local = |d: Vec3| sub(frame.rigid_preimage(d), frame.rigid_preimage(Vec3::ZERO));
        if let (Some(sun), (Some(first), second, near)) =
            (&self.sunlight, sun_directions(outside.direction))
        {
            let way = local(scale(outside.direction, -1.0));
            let mut bake = |dir: usize, share: f32| {
                if let Some(lit) = sun.get(dir).and_then(|d| d.get(&(cell & 0xFFFF))) {
                    let k = outside.brightness * share;
                    add_light(vertices, &mut color, lit, outside.sun, &|_| Some((way, k)));
                }
            };
            bake(first, near);
            if let Some(second) = second {
                bake(second, 1.0 - near);
            }
        }
        vertices
            .iter()
            .zip(lean)
            .zip(color)
            .map(|((v, &q), color)| VertexLight {
                color,
                toward: Vec3::new(dot(v.tangent_s, q), dot(v.tangent_t, q), dot(v.normal, q)),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the sort's own ordering, on synthetic lists)
    use super::{heap_sort, original_sort};

    /// The sort orders by its test, whatever the length.
    #[test]
    fn the_original_sort_orders_by_its_test() {
        for n in [0u32, 1, 2, 5, 16, 17, 40, 123, 300] {
            let mut v: Vec<u32> = (0..n).map(|i| (i * 7919 + 13) % 101).collect();
            original_sort(&mut v, |a, b| b < a);
            assert!(v.windows(2).all(|w| w[0] >= w[1]), "{n}: {v:?}");
            let mut h: Vec<u32> = (0..n).map(|i| (i * 7919 + 13) % 101).collect();
            heap_sort(&mut h, &|a: &u32, b: &u32| b < a);
            assert!(h.windows(2).all(|w| w[0] >= w[1]), "heap {n}: {h:?}");
        }
    }
}
