//! An AC2 entity as a pack's model: its meshes at rest, scaled to the height it stands for, its
//! trunk centered on the origin and fitted to the collision cylinder, written as glTF 2.0 with its
//! pictures; and the fits that put a rock wholly inside its collision.
//!
//! **Axes.** The model is measured in Asheron's Call's axes (meters, z up, its foot at 0) and written
//! in glTF's, whose height is the second axis: a point `(x, y, z)` is written `(x, z, -y)`. The fits
//! after it is written read the written numbers back, so a fit works on exactly what is drawn.

use std::collections::{BTreeMap, BTreeSet};

use dereth_primitives::num::math::hypot;
use serde_json::{json, Value};

use super::source::{Ac2, MeshPart, Rgba, Shape};
use super::tables::Fit;

/// The share of a trunk's height its slab starts at and ends at.
const SLAB: (f64, f64) = (0.03, 0.15);
/// The share of a slab's distances from its center taken as its girth.
const GIRTH_SHARE: f64 = 0.8;
/// Above the crown's base the bark's change fades out over this share of the height.
const CROWN_FADE: f64 = 0.15;
/// Within this many trunk radii the bark is squeezed; beyond, shifted.
const SQUEEZE_RADII: f64 = 1.5;

/// A double-precision number in glTF's single precision, rounded to the nearest.
#[allow(clippy::cast_possible_truncation)] // LINT-OK: glTF stores single precision.
const fn single(v: f64) -> f32 {
    v as f32
}

/// A JSON index or count, 0 when it is none.
fn index(v: &Value) -> usize {
    v.as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0)
}

/// Straight up, in glTF's axes.
const UP: [f32; 3] = [0.0, 1.0, 0.0];

/// A model written as glTF: its JSON, its buffer (`model.bin`), and its pictures by file name.
#[derive(Debug, Clone, PartialEq)]
pub struct Gltf {
    pub json: Value,
    pub bin: Vec<u8>,
    pub pictures: BTreeMap<String, Rgba>,
}

/// What a model's build measured.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Measured {
    /// The scale from the entity's own size.
    pub scale: f64,
    /// The trunk's girth before and after its fit, meters.
    pub girth: Option<(f64, f64)>,
}

/// How a static model is made.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Recipe {
    /// The height it stands, meters.
    pub height: f64,
    /// Its cut-out materials are growth: drawn cut out and two-sided.
    pub foliage: bool,
    /// The collision cylinder's radius its trunk is fitted to, 0 for none.
    pub cylinder: f64,
    pub fit: Option<Fit>,
    /// Every vertex's normal upright, as AC2 lights its frills: as the ground under them.
    pub upright: bool,
    /// The largest picture taken, texels a side.
    pub picture_limit: u32,
    /// A material is drawn cut out when this share of its texels...
    pub clear_share: f64,
    /// ...in its picture this many texels a side are clear.
    pub clear_size: u32,
}

fn percentile(xs: &mut [f64], q: f64) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.sort_by(f64::total_cmp);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    // LINT-OK: a share of a list's length, taken down to an index in it.
    let at = (q * xs.len() as f64) as usize;
    xs[at.min(xs.len() - 1)]
}

/// A trunk's center and girth: the points between the slab's shares of `top`, their mean as the
/// center and the share of their distances from it as the girth.
fn girth(points: &[[f64; 3]], top: f64) -> Option<(f64, f64, f64)> {
    let slab: Vec<&[f64; 3]> = points
        .iter()
        .filter(|p| SLAB.0 * top <= p[2] && p[2] <= SLAB.1 * top)
        .collect();
    if slab.is_empty() {
        return None;
    }
    let n = slab.len() as f64;
    let cx = slab.iter().map(|p| p[0]).sum::<f64>() / n;
    let cy = slab.iter().map(|p| p[1]).sum::<f64>() / n;
    let mut d: Vec<f64> = slab.iter().map(|p| hypot(p[0] - cx, p[1] - cy)).collect();
    Some((cx, cy, percentile(&mut d, GIRTH_SHARE)))
}

/// Appends `data` to the buffer as a view of its own, four-byte aligned; the view's index.
fn view(j: &mut Value, bin: &mut Vec<u8>, data: &[u8], target: u32) -> usize {
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let views = j["bufferViews"].as_array_mut().expect("views");
    views.push(
        json!({"buffer": 0, "byteOffset": bin.len(), "byteLength": data.len(), "target": target}),
    );
    bin.extend_from_slice(data);
    views.len() - 1
}

fn accessor(j: &mut Value, view: usize, component: u32, count: usize, kind: &str) -> usize {
    let a = j["accessors"].as_array_mut().expect("accessors");
    a.push(json!({"bufferView": view, "componentType": component, "count": count, "type": kind}));
    a.len() - 1
}

fn floats(values: &[[f32; 3]]) -> Vec<u8> {
    values
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect()
}

/// The box round `values`, as glTF's least and most.
fn bounds(values: &[[f32; 3]]) -> (Value, Value) {
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for v in values {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    (json!(lo), json!(hi))
}

/// The point `p` (Asheron's Call's axes) in glTF's.
fn gl(p: [f64; 3]) -> [f32; 3] {
    [single(p[0]), single(p[2]), single(-p[1])]
}

/// Distances closer than this share of the farther are taken as one.
const SAME_DISTANCE: f64 = 0.02;

/// The levels a model of `parts` is drawn at, nearest first: for each, how far from the eye it is
/// drawn out to (`None` for the last) and which of each part's levels it draws (0 its most
/// detailed, `i` its `i`th coarser). Each level comes in where Asheron's Call 2 draws a coarser
/// level of any part, `scale` times farther as the model stands `scale` times taller; distances
/// all but alike are one.
#[must_use]
pub fn level_plan(parts: &[MeshPart], scale: f64) -> Vec<(Option<f64>, Vec<usize>)> {
    let mut at: Vec<f64> = parts
        .iter()
        .flat_map(|p| p.coarser.iter().map(|(d, _)| d * scale))
        .filter(|d| d.is_finite() && *d > 0.0)
        .collect();
    at.sort_by(f64::total_cmp);
    let mut steps: Vec<f64> = Vec::new();
    for d in at {
        if steps.last().is_none_or(|l| d - l > SAME_DISTANCE * d) {
            steps.push(d);
        }
    }
    let drawn = |from: f64| -> Vec<usize> {
        parts
            .iter()
            .map(|p| {
                p.coarser
                    .iter()
                    .filter(|(d, _)| d * scale <= from * (1.0 + SAME_DISTANCE))
                    .count()
            })
            .collect()
    };
    let mut out = vec![(steps.first().copied(), vec![0; parts.len()])];
    for (k, from) in steps.iter().enumerate() {
        out.push((steps.get(k + 1).copied(), drawn(*from)));
    }
    out
}

/// The entity `entity` as a static model made as `recipe` says, at its most detailed level.
///
/// # Errors
/// A record on the way will not read, or the entity has no mesh to draw.
pub fn static_model(ac2: &Ac2, entity: u32, recipe: &Recipe) -> Result<(Gltf, Measured), String> {
    let (levels, measured) = static_levels(ac2, entity, recipe, false)?;
    let (_, g) = levels
        .into_iter()
        .next()
        .ok_or_else(|| format!("entity {entity:08X} has no mesh to draw"))?;
    Ok((g, measured))
}

/// A model's levels, nearest first: each with how far from the eye it is drawn out to (`None` for
/// the last), and what it measured.
pub type Levels = (Vec<(Option<f64>, Gltf)>, Measured);

/// The entity `entity` as a static model made as `recipe` says, at each of its levels
/// ([`level_plan`]) when `levels`, else at its most detailed alone: each level with how far from
/// the eye it is drawn out to (`None` for the last). Every level is the same vertices, fitted as
/// the most detailed is.
///
/// # Errors
/// A record on the way will not read, or the entity has no mesh to draw.
#[allow(clippy::too_many_lines)]
pub fn static_levels(
    ac2: &Ac2,
    entity: u32,
    recipe: &Recipe,
    levels: bool,
) -> Result<Levels, String> {
    let parts = ac2.meshes(entity)?;
    if parts.is_empty() {
        return Err(format!("entity {entity:08X} has no mesh to draw"));
    }
    // It stands on its own origin, its roots reaching below: the scale is to the height above
    // the origin, which stays the foot.
    let hi = parts
        .iter()
        .flat_map(|p| p.vertices.iter().map(|v| f64::from(v.position.z)))
        .fold(f64::NEG_INFINITY, f64::max);
    let lo = 0.0;
    let k = recipe.height / (hi - lo).max(1e-3);
    // Each material's diffuse picture and whether it is cut out, in the order first drawn.
    let mut textures: BTreeMap<u32, (Option<u32>, f64)> = BTreeMap::new();
    for p in &parts {
        for (m, _) in &p.fragments {
            let mi = *p.materials.get(*m).ok_or("a fragment names no material")?;
            textures.entry(mi).or_insert_with(|| {
                let d = ac2.diffuse(mi);
                let clear = d
                    .and_then(|d| ac2.picture(d, recipe.clear_size))
                    .map_or(0.0, |p| p.clear_share());
                (d, clear)
            });
        }
    }
    let cut: BTreeSet<u32> = textures
        .iter()
        .filter(|(_, (_, c))| *c >= recipe.clear_share)
        .map(|(mi, _)| *mi)
        .collect();
    let is_cut = |p: &super::source::MeshPart, m: usize| cut.contains(&p.materials[m]);
    // Per part, the vertices an opaque material draws: the bark.
    let bark: Vec<BTreeSet<usize>> = parts
        .iter()
        .map(|p| {
            p.fragments
                .iter()
                .filter(|(m, _)| !is_cut(p, *m))
                .flat_map(|(_, ix)| ix.iter().map(|&i| usize::from(i)))
                .collect()
        })
        .collect();
    let mut scaled: Vec<Vec<[f64; 3]>> = parts
        .iter()
        .map(|p| {
            p.vertices
                .iter()
                .map(|v| {
                    let q = v.position;
                    [
                        f64::from(q.x) * k,
                        f64::from(q.y) * k,
                        (f64::from(q.z) - lo) * k,
                    ]
                })
                .collect()
        })
        .collect();
    let top = (hi - lo) * k;
    let mut measured = Measured {
        scale: k,
        girth: None,
    };
    let bark_points = |s: &[Vec<[f64; 3]>]| -> Vec<[f64; 3]> {
        bark.iter()
            .enumerate()
            .flat_map(|(i, b)| b.iter().filter_map(move |&j| s[i].get(j).copied()))
            .collect()
    };
    let all_points =
        |s: &[Vec<[f64; 3]>]| -> Vec<[f64; 3]> { s.iter().flatten().copied().collect() };
    let trunk = girth(&bark_points(&scaled), top).or_else(|| girth(&all_points(&scaled), top));
    if let Some((cx, cy, r)) = trunk {
        for ps in &mut scaled {
            for p in ps.iter_mut() {
                *p = [p[0] - cx, p[1] - cy, p[2]];
            }
        }
        if recipe.fit.is_some() && recipe.cylinder > 0.0 && r > 1e-3 {
            let f = recipe.cylinder / r;
            match recipe.fit {
                Some(Fit::Narrow) if f < 1.0 => {
                    for ps in &mut scaled {
                        for p in ps.iter_mut() {
                            *p = [p[0] * f, p[1] * f, p[2]];
                        }
                    }
                }
                Some(Fit::Girth) if !bark_points(&scaled).is_empty() && !cut.is_empty() => {
                    // Below the crown the bark is drawn in or out fully; above it the change fades
                    // out. Within a few trunk radii it is squeezed, beyond that shifted by the
                    // same amount, so branches keep their length.
                    let mut crown: Vec<f64> = parts
                        .iter()
                        .enumerate()
                        .flat_map(|(i, p)| {
                            let s = &scaled[i];
                            p.fragments.iter().filter(|(m, _)| is_cut(p, *m)).flat_map(
                                move |(_, ix)| ix.iter().map(move |&j| s[usize::from(j)][2]),
                            )
                        })
                        .collect();
                    let mut base = if crown.is_empty() {
                        0.35 * top
                    } else {
                        percentile(&mut crown, 0.05)
                    };
                    if base < 0.2 * top {
                        base = 0.35 * top;
                    }
                    let r0 = SQUEEZE_RADII * r;
                    for (i, b) in bark.iter().enumerate() {
                        for &j in b {
                            let [x, y, z] = scaled[i][j];
                            let w = if z <= base {
                                1.0
                            } else {
                                (1.0 - (z - base) / (CROWN_FADE * top)).max(0.0)
                            };
                            let d = hypot(x, y);
                            if w == 0.0 || d < 1e-6 {
                                continue;
                            }
                            let ds = if d <= r0 { d * f } else { d - r0 * (1.0 - f) };
                            let g = (d + w * (ds - d)) / d;
                            scaled[i][j] = [x * g, y * g, z];
                        }
                    }
                }
                _ => {}
            }
        }
        let after = girth(&bark_points(&scaled), top).or_else(|| girth(&all_points(&scaled), top));
        measured.girth = Some((r, after.map_or(r, |a| a.2)));
    }
    let plan = if levels {
        level_plan(&parts, k)
    } else {
        vec![(None, vec![0; parts.len()])]
    };
    let mut made = Vec::with_capacity(plan.len());
    for (distance, chosen) in plan {
        made.push((
            distance,
            gltf_of(ac2, &parts, &scaled, recipe, &textures, &chosen),
        ));
    }
    Ok((made, measured))
}

/// `parts` at the places `scaled` gives their vertices, written as glTF: per part, its places,
/// normals and coordinates, and a primitive per fragment of the level `chosen` names for it (0 its
/// most detailed, `i` its `i`th coarser).
fn gltf_of(
    ac2: &Ac2,
    parts: &[MeshPart],
    scaled: &[Vec<[f64; 3]>],
    recipe: &Recipe,
    textures: &BTreeMap<u32, (Option<u32>, f64)>,
    chosen: &[usize],
) -> Gltf {
    let level = |pi: usize| -> &[(usize, Vec<u16>)] {
        let p = &parts[pi];
        match chosen.get(pi).copied().unwrap_or(0) {
            0 => &p.fragments,
            i => p.coarser.get(i - 1).map_or(&p.fragments, |(_, f)| f),
        }
    };
    // The glTF: per part, its places, normals and coordinates, and a primitive per fragment.
    let mut j = json!({
        "asset": {"version": "2.0", "generator": "dereth-art"},
        "scene": 0, "scenes": [{"nodes": []}], "nodes": [], "meshes": [], "materials": [],
        "textures": [], "images": [],
        "samplers": [{"magFilter": 9729, "wrapS": 10497, "wrapT": 10497}],
        "accessors": [], "bufferViews": [], "buffers": []
    });
    let mut bin = Vec::new();
    let mut pictures = BTreeMap::new();
    let mut mats: BTreeMap<u32, usize> = BTreeMap::new();
    for (pi, p) in parts.iter().enumerate() {
        let pos: Vec<[f32; 3]> = scaled[pi].iter().map(|&q| gl(q)).collect();
        let nrm: Vec<[f32; 3]> = p
            .vertices
            .iter()
            .map(|v| match (recipe.upright, v.normal) {
                (false, Some(n)) => [n.x, n.z, -n.y],
                _ => UP,
            })
            .collect();
        let uv: Vec<u8> = p
            .vertices
            .iter()
            .flat_map(|v| v.uv.unwrap_or([0.0, 0.0]))
            .flat_map(f32::to_le_bytes)
            .collect();
        let vp = view(&mut j, &mut bin, &floats(&pos), 34962);
        let a_pos = accessor(&mut j, vp, 5126, pos.len(), "VEC3");
        let (min, max) = bounds(&pos);
        j["accessors"][a_pos]["min"] = min;
        j["accessors"][a_pos]["max"] = max;
        let vn = view(&mut j, &mut bin, &floats(&nrm), 34962);
        let a_nrm = accessor(&mut j, vn, 5126, nrm.len(), "VEC3");
        let vu = view(&mut j, &mut bin, &uv, 34962);
        let a_uv = accessor(&mut j, vu, 5126, p.vertices.len(), "VEC2");
        let mut prims = Vec::new();
        for (m, ix) in level(pi) {
            if ix.is_empty() {
                continue;
            }
            let mi = p.materials[*m];
            if let std::collections::btree_map::Entry::Vacant(slot) = mats.entry(mi) {
                let mut mat = json!({"name": format!("{mi:08X}"),
                    "pbrMetallicRoughness": {"metallicFactor": 0.0, "roughnessFactor": 0.8}});
                if let Some(d) = textures.get(&mi).and_then(|t| t.0) {
                    if let Some(rgba) = ac2.picture(d, recipe.picture_limit) {
                        let file = format!("tex_{d:08X}.png");
                        let clear = rgba.clear_share();
                        pictures.insert(file.clone(), rgba);
                        let images = j["images"].as_array_mut().expect("images");
                        images.push(json!({"uri": file}));
                        let image = images.len() - 1;
                        let tex = j["textures"].as_array_mut().expect("textures");
                        tex.push(json!({"sampler": 0, "source": image}));
                        mat["pbrMetallicRoughness"]["baseColorTexture"] =
                            json!({"index": tex.len() - 1});
                        if recipe.foliage && clear >= recipe.clear_share {
                            mat["alphaMode"] = json!("MASK");
                            mat["alphaCutoff"] = json!(0.5);
                            mat["doubleSided"] = json!(true);
                        }
                    }
                }
                let ms = j["materials"].as_array_mut().expect("materials");
                ms.push(mat);
                slot.insert(ms.len() - 1);
            }
            let data: Vec<u8> = ix.iter().flat_map(|i| i.to_le_bytes()).collect();
            let vi = view(&mut j, &mut bin, &data, 34963);
            let a_ix = accessor(&mut j, vi, 5123, ix.len(), "SCALAR");
            prims.push(
                json!({"attributes": {"POSITION": a_pos, "NORMAL": a_nrm, "TEXCOORD_0": a_uv},
                "indices": a_ix, "material": mats[&mi], "mode": 4}),
            );
        }
        if !prims.is_empty() {
            let meshes = j["meshes"].as_array_mut().expect("meshes");
            meshes.push(json!({"name": format!("{:08X}", p.id), "primitives": prims}));
            let mesh = meshes.len() - 1;
            let nodes = j["nodes"].as_array_mut().expect("nodes");
            nodes.push(json!({"mesh": mesh}));
            let node = nodes.len() - 1;
            j["scenes"][0]["nodes"]
                .as_array_mut()
                .expect("scene")
                .push(json!(node));
        }
    }
    j["buffers"] = json!([{"uri": "model.bin", "byteLength": bin.len()}]);
    for key in ["textures", "images", "materials"] {
        if j[key].as_array().is_some_and(Vec::is_empty) {
            j.as_object_mut().expect("an object").remove(key);
        }
    }
    Gltf {
        json: j,
        bin,
        pictures,
    }
}

/// The accessors a model's primitives use for `key`, each once, in the order first used.
fn used(g: &Gltf, key: &str) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    for mesh in g.json["meshes"].as_array().into_iter().flatten() {
        for p in mesh["primitives"].as_array().into_iter().flatten() {
            if let Some(i) = p["attributes"][key].as_u64() {
                let i = usize::try_from(i).unwrap_or(0);
                if !out.contains(&i) {
                    out.push(i);
                }
            }
        }
    }
    out
}

/// Where accessor `a`'s values start in the buffer, its stride and its count.
fn layout(g: &Gltf, a: usize) -> (usize, usize, usize) {
    let acc = &g.json["accessors"][a];
    let v = &g.json["bufferViews"][index(&acc["bufferView"])];
    let off = index(&v["byteOffset"]) + index(&acc["byteOffset"]);
    let stride = if v["byteStride"].is_u64() {
        index(&v["byteStride"])
    } else {
        12
    };
    (off, stride, index(&acc["count"]))
}

fn read3(bin: &[u8], at: usize) -> [f32; 3] {
    let f = |k: usize| f32::from_le_bytes([bin[k], bin[k + 1], bin[k + 2], bin[k + 3]]);
    [f(at), f(at + 4), f(at + 8)]
}

fn write3(bin: &mut [u8], at: usize, v: [f32; 3]) {
    for (k, x) in v.iter().enumerate() {
        bin[at + 4 * k..at + 4 * k + 4].copy_from_slice(&x.to_le_bytes());
    }
}

/// The model's vertices in Asheron's Call's axes: every primitive's, so a part's vertices come
/// once for each of its primitives.
#[must_use]
pub fn points(g: &Gltf) -> Vec<[f64; 3]> {
    let mut out = Vec::new();
    for mesh in g.json["meshes"].as_array().into_iter().flatten() {
        for p in mesh["primitives"].as_array().into_iter().flatten() {
            if !p["attributes"]["POSITION"].is_u64() {
                continue;
            }
            let (off, stride, n) = layout(g, index(&p["attributes"]["POSITION"]));
            for k in 0..n {
                let q = read3(&g.bin, off + k * stride);
                out.push([f64::from(q[0]), -f64::from(q[2]), f64::from(q[1])]);
            }
        }
    }
    out
}

/// Every place and normal of the model through `place` and `normal` (each in glTF's axes),
/// each accessor once; the places' box set again.
fn transform(
    g: &mut Gltf,
    place: impl Fn([f32; 3]) -> [f32; 3],
    normal: impl Fn([f32; 3]) -> [f32; 3],
) {
    for (key, f) in [
        ("POSITION", &place as &dyn Fn([f32; 3]) -> [f32; 3]),
        ("NORMAL", &normal),
    ] {
        for a in used(g, key) {
            let (off, stride, n) = layout(g, a);
            let mut vs = Vec::with_capacity(n);
            for k in 0..n {
                let v = f(read3(&g.bin, off + k * stride));
                write3(&mut g.bin, off + k * stride, v);
                vs.push(v);
            }
            if key == "POSITION" {
                let (min, max) = bounds(&vs);
                g.json["accessors"][a]["min"] = min;
                g.json["accessors"][a]["max"] = max;
            }
        }
    }
}

/// How far `p` stands outside every one of `shape`'s collision shapes (0 inside one).
#[must_use]
pub fn outside(shape: &Shape, p: [f64; 3]) -> f64 {
    let mut best = f64::INFINITY;
    for &[x, y, z, r, h] in &shape.cylinders {
        let d_xy = (hypot(p[0] - x, p[1] - y) - r).max(0.0);
        let d_z = (z - p[2]).max(p[2] - (z + h)).max(0.0);
        best = best.min(hypot(d_xy, d_z));
    }
    for &[x, y, z, r] in &shape.spheres {
        let d = ((p[0] - x).powi(2) + (p[1] - y).powi(2) + (p[2] - z).powi(2)).sqrt();
        best = best.min((d - r).max(0.0));
    }
    best
}

/// The collision's box, the ground clipped off: its least and most corner.
fn collision_box(shape: &Shape) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for &[x, y, z, r, h] in &shape.cylinders {
        for (k, (a, b)) in [(x - r, x + r), (y - r, y + r), (z, z + h)]
            .into_iter()
            .enumerate()
        {
            lo[k] = lo[k].min(a);
            hi[k] = hi[k].max(b);
        }
    }
    for &[x, y, z, r] in &shape.spheres {
        for (k, c) in [x, y, z].into_iter().enumerate() {
            lo[k] = lo[k].min(c - r);
            hi[k] = hi[k].max(c + r);
        }
    }
    lo[2] = lo[2].max(0.0);
    (lo, hi)
}

/// An inside fit: per-axis scales, and the ground point the model's middle is moved from and to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InsideFit {
    pub scale: [f64; 3],
    pub from: [f64; 2],
    pub to: [f64; 2],
}

/// The per-axis scales and the ground offset that put a model wholly inside its collision: first
/// its box brought to the collision's box (its middle over the collision's middle, its sides to the
/// collision's sides, its top to the collision's top), then each axis shrunk, searched on a grid
/// and refined, to the largest box of shrinks that leaves no vertex more than `tolerance` out.
#[must_use]
pub fn inside_fit(pts: &[[f64; 3]], shape: &Shape, tolerance: f64) -> InsideFit {
    let above: Vec<[f64; 3]> = {
        let a: Vec<[f64; 3]> = pts.iter().copied().filter(|p| p[2] > 0.02).collect();
        if a.is_empty() {
            pts.to_vec()
        } else {
            a
        }
    };
    let (lo, hi) = collision_box(shape);
    let mut mlo = [f64::INFINITY; 3];
    let mut mhi = [f64::NEG_INFINITY; 3];
    for p in &above {
        for k in 0..3 {
            mlo[k] = mlo[k].min(p[k]);
            mhi[k] = mhi[k].max(p[k]);
        }
    }
    let mid = [(mlo[0] + mhi[0]) / 2.0, (mlo[1] + mhi[1]) / 2.0];
    let cmid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    let s = [
        (hi[0] - lo[0]) / (mhi[0] - mlo[0]).max(1e-3),
        (hi[1] - lo[1]) / (mhi[1] - mlo[1]).max(1e-3),
        hi[2] / mhi[2].max(1e-3),
    ];
    let fits = |t: [f64; 3]| {
        above.iter().all(|p| {
            outside(
                shape,
                [
                    cmid[0] + (p[0] - mid[0]) * s[0] * t[0],
                    cmid[1] + (p[1] - mid[1]) * s[1] * t[1],
                    p[2] * s[2] * t[2],
                ],
            ) <= tolerance
        })
    };
    if fits([1.0, 1.0, 1.0]) {
        return InsideFit {
            scale: s,
            from: mid,
            to: cmid,
        };
    }
    let steps: Vec<f64> = (0..17).map(|k| 0.2 + 0.05 * f64::from(k)).collect();
    let (mut best, mut most) = ([0.0; 3], 0.0);
    for &tx in &steps {
        for &ty in &steps {
            if tx * ty * 1.0 <= most {
                continue;
            }
            if !fits([tx, ty, 0.05]) {
                continue;
            }
            let (mut lo_z, mut hi_z) = (0.0, 1.0);
            for _ in 0..12 {
                let m = (lo_z + hi_z) / 2.0;
                if fits([tx, ty, m]) {
                    lo_z = m;
                } else {
                    hi_z = m;
                }
            }
            if tx * ty * lo_z > most {
                best = [tx, ty, lo_z];
                most = tx * ty * lo_z;
            }
        }
    }
    InsideFit {
        scale: [s[0] * best[0], s[1] * best[1], s[2] * best[2]],
        from: mid,
        to: cmid,
    }
}

/// The model moved and scaled to stand inside `shape` ([`inside_fit`]); the fit.
pub fn refit(g: &mut Gltf, shape: &Shape, tolerance: f64) -> InsideFit {
    let f = inside_fit(&points(g), shape, tolerance);
    let [sx, sy, sz] = f.scale;
    let ([fx, fy], [tx, ty]) = (f.from, f.to);
    transform(
        g,
        |[gx, gy, gz]| {
            let (x, y, z) = (f64::from(gx), -f64::from(gz), f64::from(gy));
            let (x, y, z) = (tx + (x - fx) * sx, ty + (y - fy) * sy, z * sz);
            [single(x), single(z), single(-y)]
        },
        |[gx, gy, gz]| {
            let (x, y, z) = (f64::from(gx) / sx, f64::from(gy) / sz, f64::from(gz) / sy);
            let n = (x * x + y * y + z * z).sqrt();
            let n = if n == 0.0 { 1.0 } else { n };
            [single(x / n), single(y / n), single(z / n)]
        },
    );
    f
}

/// The model turned half a turn about its upright axis.
pub fn turn_half(g: &mut Gltf) {
    let turn = |[x, y, z]: [f32; 3]| [-x, y, -z];
    transform(g, turn, turn);
}

/// The models as one: their meshes side by side, each keeping its own materials and pictures.
#[must_use]
pub fn merge(models: &[Gltf]) -> Gltf {
    let mut j = json!({"asset": {"version": "2.0", "generator": "dereth-art"}, "scene": 0,
        "scenes": [{"nodes": []}], "nodes": [], "meshes": [], "materials": [], "textures": [],
        "images": [], "samplers": [], "accessors": [], "bufferViews": [], "buffers": []});
    let mut bin: Vec<u8> = Vec::new();
    let mut pictures = BTreeMap::new();
    let mut images: BTreeMap<String, usize> = BTreeMap::new();
    let len = |j: &Value, k: &str| j[k].as_array().map_or(0, Vec::len);
    let push = |j: &mut Value, k: &str, v: Value| j[k].as_array_mut().expect("a list").push(v);
    let list = |g: &Gltf, k: &str| g.json[k].as_array().cloned().unwrap_or_default();
    for g in models {
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let base = bin.len() as u64;
        bin.extend_from_slice(&g.bin);
        let (nv, na, nm, nt, ns) = (
            len(&j, "bufferViews"),
            len(&j, "accessors"),
            len(&j, "materials"),
            len(&j, "textures"),
            len(&j, "samplers"),
        );
        for mut v in list(g, "bufferViews") {
            v["byteOffset"] = json!(v["byteOffset"].as_u64().unwrap_or(0) + base);
            push(&mut j, "bufferViews", v);
        }
        for mut a in list(g, "accessors") {
            a["bufferView"] = json!(index(&a["bufferView"]) + nv);
            push(&mut j, "accessors", a);
        }
        for s in list(g, "samplers") {
            push(&mut j, "samplers", s);
        }
        let own_images = list(g, "images");
        for im in &own_images {
            let uri = im["uri"].as_str().unwrap_or_default().to_owned();
            if let std::collections::btree_map::Entry::Vacant(slot) = images.entry(uri.clone()) {
                if let Some(p) = g.pictures.get(&uri) {
                    pictures.insert(uri.clone(), p.clone());
                }
                slot.insert(len(&j, "images"));
                push(&mut j, "images", json!({"uri": uri}));
            }
        }
        for mut t in list(g, "textures") {
            let src = index(&t["source"]);
            let uri = own_images[src]["uri"].as_str().unwrap_or_default();
            t["source"] = json!(images[uri]);
            if t["sampler"].is_u64() {
                t["sampler"] = json!(index(&t["sampler"]) + ns);
            }
            push(&mut j, "textures", t);
        }
        for mut m in list(g, "materials") {
            let tex = &mut m["pbrMetallicRoughness"]["baseColorTexture"]["index"];
            if tex.is_u64() {
                *tex = json!(index(tex) + nt);
            }
            push(&mut j, "materials", m);
        }
        for mut mesh in list(g, "meshes") {
            for p in mesh["primitives"].as_array_mut().into_iter().flatten() {
                let attrs = p["attributes"].as_object_mut().expect("attributes");
                for v in attrs.values_mut() {
                    *v = json!(index(v) + na);
                }
                if p["indices"].is_u64() {
                    p["indices"] = json!(index(&p["indices"]) + na);
                }
                if p["material"].is_u64() {
                    p["material"] = json!(index(&p["material"]) + nm);
                }
            }
            push(&mut j, "meshes", mesh);
            let m = len(&j, "meshes") - 1;
            push(&mut j, "nodes", json!({"mesh": m}));
            let n = len(&j, "nodes") - 1;
            j["scenes"][0]["nodes"]
                .as_array_mut()
                .expect("scene")
                .push(json!(n));
        }
    }
    j["buffers"] = json!([{"uri": "model.bin", "byteLength": bin.len()}]);
    for key in ["textures", "images", "materials", "samplers"] {
        if j[key].as_array().is_some_and(Vec::is_empty) {
            j.as_object_mut().expect("an object").remove(key);
        }
    }
    Gltf {
        json: j,
        bin,
        pictures,
    }
}

/// The share of `pts` out of `shape` by more than `tolerance`, and the farthest out.
#[must_use]
pub fn measure(pts: &[[f64; 3]], shape: &Shape, tolerance: f64) -> (f64, f64) {
    let d: Vec<f64> = pts
        .iter()
        .filter(|p| p[2] > 0.02)
        .map(|p| outside(shape, *p))
        .collect();
    if d.is_empty() {
        return (0.0, 0.0);
    }
    let out = d.iter().filter(|&&x| x > tolerance).count();
    (
        out as f64 / d.len() as f64,
        d.iter().copied().fold(0.0, f64::max),
    )
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 look's fits)
    use super::*;

    fn part(coarser: &[f64]) -> MeshPart {
        MeshPart {
            id: 0x0600_0001,
            materials: Vec::new(),
            vertices: Vec::new(),
            fragments: Vec::new(),
            coarser: coarser.iter().map(|d| (*d, Vec::new())).collect(),
        }
    }

    /// A model's levels come in wherever any part's coarser level does, as far again as the model
    /// stands taller; each part draws the coarsest of its levels come in by then, and distances
    /// all but alike are one.
    #[test]
    fn a_models_levels_come_in_where_any_parts_coarser_level_does() {
        let parts = [part(&[40.0, 80.0]), part(&[60.0, 80.5])];
        let plan = level_plan(&parts, 2.0);
        assert_eq!(
            plan,
            vec![
                (Some(80.0), vec![0, 0]),
                (Some(120.0), vec![1, 0]),
                (Some(160.0), vec![1, 1]),
                (None, vec![2, 2]),
            ]
        );
        // A part with no coarser level draws its most detailed at every level.
        assert_eq!(
            level_plan(&[part(&[]), part(&[50.0])], 1.0),
            vec![(Some(50.0), vec![0, 0]), (None, vec![0, 1])]
        );
        assert_eq!(level_plan(&[part(&[])], 1.0), vec![(None, vec![0])]);
    }

    #[test]
    fn a_trunks_girth_is_its_slabs_share_of_distances_from_its_middle() {
        // A ring of radius 2 round (1, 1) in the slab, a far point above it.
        let mut pts: Vec<[f64; 3]> = (0..10)
            .map(|k| {
                let a = f64::from(k) * std::f64::consts::TAU / 10.0;
                [
                    1.0 + 2.0 * dereth_primitives::num::math::cos(a),
                    1.0 + 2.0 * dereth_primitives::num::math::sin(a),
                    1.0,
                ]
            })
            .collect();
        pts.push([50.0, 50.0, 9.0]);
        let (cx, cy, r) = girth(&pts, 10.0).expect("a slab");
        assert!((cx - 1.0).abs() < 1e-9 && (cy - 1.0).abs() < 1e-9);
        assert!((r - 2.0).abs() < 1e-9);
        assert!(girth(&pts, 1.0).is_none());
    }

    #[test]
    fn a_rock_too_big_for_its_sphere_is_shrunk_until_it_fits() {
        let shape = Shape {
            cylinders: Vec::new(),
            spheres: vec![[0.0, 0.0, 1.0, 1.0]],
        };
        // A box 4 m on a side, standing on the ground.
        let pts: Vec<[f64; 3]> = [-2.0, 2.0]
            .iter()
            .flat_map(|&x| [-2.0, 2.0].map(move |y| (x, y)))
            .flat_map(|(x, y)| [0.03, 4.0].map(move |z| [x + 3.0, y, z]))
            .collect();
        let f = inside_fit(&pts, &shape, 0.05);
        assert_eq!(f.to, [0.0, 0.0]);
        assert_eq!(f.from, [3.0, 0.0]);
        let placed: Vec<[f64; 3]> = pts
            .iter()
            .map(|p| {
                [
                    f.to[0] + (p[0] - f.from[0]) * f.scale[0],
                    f.to[1] + (p[1] - f.from[1]) * f.scale[1],
                    p[2] * f.scale[2],
                ]
            })
            .collect();
        let (share, far) = measure(&placed, &shape, 0.05);
        assert_eq!(share, 0.0);
        assert!(far <= 0.05, "{far}");
        assert!(
            f.scale.iter().all(|&s| s > 0.05 && s < 1.0),
            "{:?}",
            f.scale
        );
    }

    #[test]
    fn a_point_inside_a_cylinder_is_not_out_and_one_beside_it_is() {
        let shape = Shape {
            cylinders: vec![[0.0, 0.0, 0.0, 1.0, 2.0]],
            spheres: Vec::new(),
        };
        assert_eq!(outside(&shape, [0.5, 0.0, 1.0]), 0.0);
        assert!((outside(&shape, [3.0, 0.0, 1.0]) - 2.0).abs() < 1e-12);
        assert!((outside(&shape, [0.0, 0.0, 5.0]) - 3.0).abs() < 1e-12);
    }
}
