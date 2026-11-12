//! A setup as a skeleton an artist rigs a new body to: one joint a part, written as glTF 2.0 with
//! the setup's own parts as a reference body skinned to it, and its animations as clips.
//!
//! **The joints.** Joint `k` is part `k`, named `part_NN` (two digits or more), so a body rigged
//! to the skeleton names the parts it replaces by their place in the setup. A joint's rest
//! transform is its part's frame in the setup's resting placement, its default scale left out. An
//! object's parts move in the object's own space, each frame placing every part, so a skeleton
//! needs no hierarchy to be driven. An artist needs one to pose and weight a body, though, and a
//! setup's own parent list says nothing useful for a body: each part is hung from the part whose
//! triangles come nearest its origin, grown out from part 0 ([`hierarchy`]).
//!
//! **The reference body** is the setup's parts as they rest, each vertex bound wholly to its
//! part's joint, with the parts' pictures beside it. Its inverse bind matrices are the inverses of
//! the parts' rest transforms with their scale applied, so drawn through the joints at any pose it
//! is exactly the rigid parts at that pose.
//!
//! **Clips.** Each animation asked for becomes a clip at the rate asked, every part's frame
//! keyed at every frame, made parent-relative for the hierarchy above; the object's own motion is
//! left out, so a clip plays in place.
//!
//! **Axes.** glTF's height is its second axis: a point `(x, y, z)` of the setup is written
//! `(x, z, -y)`, the turn the art pack's loader undoes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dereth_assets::{Animation, Decode, Setup};
use dereth_client_runtime::models::resolve_parts;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, Frame};
use dereth_scene::textures::TextureStore;
use glam::{Mat3, Mat4, Quat, Vec3};
use serde_json::{json, Value};

use crate::model::{posed, Models, Vert};
use crate::tally::ModelId;
use crate::thumb::Material;

/// What to export.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub setup: DataId,
    /// Each animation to add as a clip, and its rate in frames a second.
    pub animations: Vec<(DataId, f32)>,
    /// The folder `skeleton.gltf`, `skeleton.bin` and the pictures are written to.
    pub out: PathBuf,
}

/// What was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub gltf: PathBuf,
    pub joints: usize,
    pub triangles: usize,
    pub pictures: usize,
    pub clips: usize,
    /// Each joint's parent, as [`hierarchy`] chose it.
    pub parents: Vec<Option<usize>>,
}

/// One part's triangles in the setup, at rest, by the surface they are drawn with.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PartMesh {
    pub groups: Vec<(Option<DataId>, bool, Vec<[Vert; 3]>)>,
}

/// One joint: its part's frame and default scale at rest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Joint {
    pub frame: Mat4,
    pub scale: Vec3,
}

/// A frame as a matrix: `M v = R v + t`, the rotation as the client turns a part.
#[must_use]
pub fn frame_matrix(f: &Frame) -> Mat4 {
    let m = dereth_primitives::frame::l2g(f.rotation).0;
    Mat4::from_cols(
        glam::Vec4::new(m[0], m[1], m[2], 0.0),
        glam::Vec4::new(m[3], m[4], m[5], 0.0),
        glam::Vec4::new(m[6], m[7], m[8], 0.0),
        glam::Vec4::new(f.origin.x, f.origin.y, f.origin.z, 1.0),
    )
}

/// The turn from the setup's axes to glTF's: `(x, y, z)` to `(x, z, -y)`.
#[must_use]
pub fn to_gltf() -> Mat4 {
    Mat4::from_mat3(Mat3::from_cols(Vec3::X, -Vec3::Z, Vec3::Y))
}

/// Each part's parent: part 0 is the root, and the others are hung one at a time, each from the
/// part already hung whose triangles come nearest its origin, the nearest first. A part with no
/// triangles is never a parent. With no part 0, or none with triangles, every part is a root.
#[must_use]
pub fn hierarchy(origins: &[Vec3], points: &[Vec<Vec3>]) -> Vec<Option<usize>> {
    let n = origins.len();
    let mut parent = vec![None; n];
    if n == 0 {
        return parent;
    }
    let mut hung = vec![false; n];
    hung[0] = true;
    let nearest = |from: Vec3, to: &[Vec3]| {
        to.iter()
            .map(|p| p.distance_squared(from))
            .fold(f32::INFINITY, f32::min)
    };
    // The distance of each part not hung yet from the nearest hung part with triangles.
    let mut best: Vec<(f32, Option<usize>)> = (0..n)
        .map(|j| {
            let d = points
                .first()
                .map_or(f32::INFINITY, |p| nearest(origins[j], p));
            (d, d.is_finite().then_some(0))
        })
        .collect();
    for _ in 1..n {
        let next = (0..n)
            .filter(|&j| !hung[j])
            .min_by(|&a, &b| best[a].0.total_cmp(&best[b].0).then(a.cmp(&b)));
        let Some(j) = next else {
            break;
        };
        hung[j] = true;
        parent[j] = best[j].1;
        if points.get(j).is_some_and(|p| !p.is_empty()) {
            for k in (0..n).filter(|&k| !hung[k]) {
                let d = nearest(origins[k], &points[j]);
                if d < best[k].0 {
                    best[k] = (d, Some(j));
                }
            }
        }
    }
    parent
}

/// A setup's rest skeleton and reference body, read from `store`.
///
/// # Errors
/// A setup that will not read.
pub fn read(store: &RetailDatStore, setup: DataId) -> Result<(Vec<Joint>, Vec<PartMesh>), String> {
    let bytes = store
        .read_typed(DbType::Setup, setup)
        .map_err(|e| format!("setup 0x{:08X}: {e}", setup.0))?;
    Setup::decode_payload_in(store.era_of(setup), setup, &bytes)
        .map_err(|e| format!("setup 0x{:08X}: {e}", setup.0))?;
    let mut models = Models::new(store);
    let mut joints = Vec::new();
    let mut meshes = Vec::new();
    for part in resolve_parts(store, setup) {
        let mesh = models
            .mesh(ModelId::Object(part.gfxobj), &[])
            .unwrap_or_default();
        let groups = mesh
            .groups
            .into_iter()
            .map(|g| {
                let g = posed(g, &part.frame, part.scale);
                (g.surface, g.two_sided, g.triangles)
            })
            .collect();
        joints.push(Joint {
            frame: frame_matrix(&part.frame),
            scale: Vec3::new(part.scale.x, part.scale.y, part.scale.z),
        });
        meshes.push(PartMesh { groups });
    }
    Ok((joints, meshes))
}

/// [`hierarchy`] of `joints`, over the triangles of `meshes`.
#[must_use]
pub fn parents_of(joints: &[Joint], meshes: &[PartMesh]) -> Vec<Option<usize>> {
    let origins: Vec<Vec3> = joints.iter().map(|j| j.frame.w_axis.truncate()).collect();
    let points: Vec<Vec<Vec3>> = meshes
        .iter()
        .map(|m| {
            m.groups
                .iter()
                .flat_map(|(_, _, t)| t.iter().flatten().map(|v| Vec3::from(v.position)))
                .collect()
        })
        .collect();
    hierarchy(&origins, &points)
}

/// One clip: its name, and every frame's part frames, at a rate.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub name: String,
    pub fps: f32,
    pub frames: Vec<Vec<Mat4>>,
}

/// An animation record as a clip.
///
/// # Errors
/// A record that will not read.
pub fn clip(store: &RetailDatStore, id: DataId, fps: f32) -> Result<Clip, String> {
    let bytes = store
        .read_typed(DbType::Anim, id)
        .map_err(|e| format!("animation 0x{:08X}: {e}", id.0))?;
    let a = Animation::decode_payload_in(store.era_of(id), id, &bytes)
        .map_err(|e| format!("animation 0x{:08X}: {e}", id.0))?;
    Ok(Clip {
        name: format!("0x{:08X}", id.0),
        fps,
        frames: a
            .part_frames
            .iter()
            .map(|f| f.frames.iter().map(frame_matrix).collect())
            .collect(),
    })
}

/// The glTF's binary buffer, its views and accessors, built up as data is added.
#[derive(Debug, Default)]
struct Buffer {
    bytes: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Buffer {
    fn align(&mut self) {
        while !self.bytes.len().is_multiple_of(4) {
            self.bytes.push(0);
        }
    }

    /// Floats, `width` a element (`kind` is "SCALAR", "VEC2" ... "MAT4"), with their bounds when
    /// `bounds` (positions and times need them).
    fn floats(&mut self, data: &[f32], width: usize, kind: &str, bounds: bool) -> usize {
        self.align();
        let at = self.bytes.len();
        for f in data {
            self.bytes.extend(f.to_le_bytes());
        }
        self.views
            .push(json!({"buffer": 0, "byteOffset": at, "byteLength": data.len() * 4}));
        let count = data.len() / width;
        let mut acc = json!({"bufferView": self.views.len() - 1, "componentType": 5126,
            "count": count, "type": kind});
        if bounds && count > 0 {
            let mut lo = vec![f32::INFINITY; width];
            let mut hi = vec![f32::NEG_INFINITY; width];
            for e in data.chunks(width) {
                for (k, v) in e.iter().enumerate() {
                    lo[k] = lo[k].min(*v);
                    hi[k] = hi[k].max(*v);
                }
            }
            acc["min"] = json!(lo);
            acc["max"] = json!(hi);
        }
        self.accessors.push(acc);
        self.accessors.len() - 1
    }

    fn u16s(&mut self, data: &[u16], kind: &str) -> usize {
        self.align();
        let at = self.bytes.len();
        for v in data {
            self.bytes.extend(v.to_le_bytes());
        }
        self.views
            .push(json!({"buffer": 0, "byteOffset": at, "byteLength": data.len() * 2}));
        self.accessors.push(
            json!({"bufferView": self.views.len() - 1, "componentType": 5123,
            "count": data.len() / 4, "type": kind}),
        );
        self.accessors.len() - 1
    }

    fn indices(&mut self, data: &[u32]) -> usize {
        self.align();
        let at = self.bytes.len();
        for v in data {
            self.bytes.extend(v.to_le_bytes());
        }
        self.views.push(
            json!({"buffer": 0, "byteOffset": at, "byteLength": data.len() * 4,
            "target": 34963}),
        );
        self.accessors.push(
            json!({"bufferView": self.views.len() - 1, "componentType": 5125,
            "count": data.len(), "type": "SCALAR"}),
        );
        self.accessors.len() - 1
    }
}

/// How one surface is drawn in the glTF: a picture by its file name, or a color (linear, with
/// alpha), and whether it is cut out and two-sided.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    Picture { file: String, cut: bool },
    Color([f32; 4]),
}

/// A display value as a linear one.
fn linear(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        dereth_primitives::num::math::powf((c + 0.055) / 1.055, 2.4)
    }
}

/// One surface's triangles across the parts: its surface, whether it is two-sided, and each
/// triangle with its part.
type Drawn = (Option<DataId>, bool, Vec<(usize, [Vert; 3])>);

/// The glTF of a skeleton and its reference body, with `clips`; `paint` says how each surface is
/// drawn (`None` leaves its triangles out). Returns the JSON and the binary buffer.
#[must_use]
pub fn gltf(
    name: &str,
    joints: &[Joint],
    parents: &[Option<usize>],
    meshes: &[PartMesh],
    paint: &dyn Fn(Option<DataId>) -> Option<Paint>,
    clips: &[Clip],
) -> (Value, Vec<u8>) {
    let c = to_gltf();
    let ci = c.inverse();
    let world: Vec<Mat4> = joints.iter().map(|j| c * j.frame * ci).collect();
    let local = |k: usize, w: &[Mat4]| match parents.get(k).copied().flatten() {
        Some(p) => w[p].inverse() * w[k],
        None => w[k],
    };
    let mut buf = Buffer::default();
    let mut nodes = Vec::new();
    for k in 0..joints.len() {
        let (_, r, t) = local(k, &world).to_scale_rotation_translation();
        let children: Vec<usize> = (0..joints.len())
            .filter(|&ch| parents.get(ch).copied().flatten() == Some(k))
            .collect();
        let mut node = json!({"name": format!("part_{k:02}"), "translation": t.to_array(),
            "rotation": r.normalize().to_array(), "extras": {"part": k}});
        if !children.is_empty() {
            node["children"] = json!(children);
        }
        nodes.push(node);
    }
    // Every part's triangles drawn with one surface make one primitive, each vertex bound to its
    // part's joint.
    let mut by_surface: BTreeMap<Option<u32>, Drawn> = BTreeMap::new();
    for (k, m) in meshes.iter().enumerate() {
        for (s, two, tris) in &m.groups {
            let e = by_surface
                .entry(s.map(|s| s.0))
                .or_insert((*s, false, Vec::new()));
            e.1 |= *two;
            e.2.extend(tris.iter().map(|t| (k, *t)));
        }
    }
    let mut materials = Vec::new();
    let mut images: Vec<String> = Vec::new();
    let mut primitives = Vec::new();
    let mut triangles = 0;
    for (surface, two_sided, tris) in by_surface.into_values() {
        let Some(p) = paint(surface) else {
            continue;
        };
        let mut pos = Vec::new();
        let mut nrm = Vec::new();
        let mut uv = Vec::new();
        let mut jnt = Vec::new();
        let mut wgt = Vec::new();
        let mut idx = Vec::new();
        for (k, t) in &tris {
            // The client's triangles turn counter-clockwise seen from the front, as glTF's do.
            for v in [t[0], t[1], t[2]] {
                let p = c.transform_point3(Vec3::from(v.position));
                let n = c.transform_vector3(Vec3::from(v.normal));
                pos.extend(p.to_array());
                nrm.extend(n.normalize_or(Vec3::Y).to_array());
                uv.extend(v.uv);
                jnt.extend([u16::try_from(*k).unwrap_or(0), 0, 0, 0]);
                wgt.extend([1.0f32, 0.0, 0.0, 0.0]);
                idx.push(u32::try_from(idx.len()).unwrap_or(0));
            }
        }
        triangles += tris.len();
        let mut material = json!({"name": surface.map_or("none".to_owned(), |s| format!("0x{:08X}", s.0)),
            "doubleSided": two_sided,
            "pbrMetallicRoughness": {"metallicFactor": 0.0, "roughnessFactor": 0.9}});
        match p {
            Paint::Picture { file, cut } => {
                material["pbrMetallicRoughness"]["baseColorTexture"] =
                    json!({"index": images.len()});
                images.push(file);
                if cut {
                    material["alphaMode"] = json!("MASK");
                    material["alphaCutoff"] = json!(0.5);
                }
            }
            Paint::Color(c) => material["pbrMetallicRoughness"]["baseColorFactor"] = json!(c),
        }
        materials.push(material);
        let attributes = json!({
            "POSITION": buf.floats(&pos, 3, "VEC3", true),
            "NORMAL": buf.floats(&nrm, 3, "VEC3", false),
            "TEXCOORD_0": buf.floats(&uv, 2, "VEC2", false),
            "JOINTS_0": buf.u16s(&jnt, "VEC4"),
            "WEIGHTS_0": buf.floats(&wgt, 4, "VEC4", false),
        });
        primitives.push(
            json!({"attributes": attributes, "indices": buf.indices(&idx),
            "material": materials.len() - 1}),
        );
    }
    // The inverse bind matrices: each part's rest transform with its scale, inverted.
    let ibm: Vec<f32> = joints
        .iter()
        .flat_map(|j| {
            (c * j.frame * Mat4::from_scale(j.scale) * ci)
                .inverse()
                .to_cols_array()
        })
        .collect();
    let ibm = buf.floats(&ibm, 16, "MAT4", false);
    let roots: Vec<usize> = (0..joints.len())
        .filter(|&k| parents.get(k).copied().flatten().is_none())
        .collect();
    let body = nodes.len();
    nodes.push(json!({"name": name, "mesh": 0, "skin": 0}));
    let mut animations = Vec::new();
    for clip in clips {
        if clip.frames.is_empty() || clip.fps <= 0.0 {
            continue;
        }
        let times: Vec<f32> = (0..clip.frames.len())
            .map(|f| {
                #[allow(clippy::cast_precision_loss)] // a clip's frame count
                let f = f as f32;
                f / clip.fps
            })
            .collect();
        let time = buf.floats(&times, 1, "SCALAR", true);
        let mut samplers = Vec::new();
        let mut channels = Vec::new();
        let posed: Vec<Vec<Mat4>> = clip
            .frames
            .iter()
            .map(|f| {
                (0..joints.len())
                    .map(|k| f.get(k).map_or(world[k], |m| c * *m * ci))
                    .collect()
            })
            .collect();
        for k in 0..joints.len() {
            let mut ts = Vec::new();
            let mut rs = Vec::new();
            let mut last = Quat::IDENTITY;
            for w in &posed {
                let (_, r, t) = local(k, w).to_scale_rotation_translation();
                // Keep each key on the side of the last, so the clip turns the short way.
                let r = if r.dot(last) < 0.0 { -r } else { r };
                last = r;
                ts.extend(t.to_array());
                rs.extend(r.normalize().to_array());
            }
            let t_out = buf.floats(&ts, 3, "VEC3", false);
            let r_out = buf.floats(&rs, 4, "VEC4", false);
            samplers.push(json!({"input": time, "output": t_out, "interpolation": "LINEAR"}));
            channels.push(json!({"sampler": samplers.len() - 1,
                "target": {"node": k, "path": "translation"}}));
            samplers.push(json!({"input": time, "output": r_out, "interpolation": "LINEAR"}));
            channels.push(json!({"sampler": samplers.len() - 1,
                "target": {"node": k, "path": "rotation"}}));
        }
        animations.push(json!({"name": clip.name, "samplers": samplers, "channels": channels}));
    }
    let mut scene_nodes = roots.clone();
    scene_nodes.push(body);
    let mut doc = json!({
        "asset": {"version": "2.0", "generator": "dereth-art skeleton"},
        "scene": 0,
        "scenes": [{"nodes": scene_nodes}],
        "nodes": nodes,
        "meshes": [{"name": name, "primitives": primitives}],
        "skins": [{"name": name, "joints": (0..joints.len()).collect::<Vec<_>>(),
            "inverseBindMatrices": ibm, "skeleton": roots.first().copied().unwrap_or(0)}],
        "materials": materials,
        "textures": (0..images.len()).map(|k| json!({"source": k})).collect::<Vec<_>>(),
        "images": images.iter().map(|f| json!({"uri": f})).collect::<Vec<_>>(),
        "buffers": [{"uri": "skeleton.bin", "byteLength": buf.bytes.len()}],
        "bufferViews": buf.views,
        "accessors": buf.accessors,
        "extras": {"triangles": triangles},
    });
    if !animations.is_empty() {
        doc["animations"] = json!(animations);
    }
    if images.is_empty() {
        if let Some(o) = doc.as_object_mut() {
            o.remove("textures");
            o.remove("images");
        }
    }
    (doc, buf.bytes)
}

/// Exports `req`'s setup from `store`.
///
/// # Errors
/// A setup or animation that will not read, or a file that will not write.
pub fn export(store: &RetailDatStore, req: &Request) -> Result<Written, String> {
    let (joints, mut meshes) = read(store, req.setup)?;
    let clips = req
        .animations
        .iter()
        .map(|&(id, fps)| clip(store, id, fps))
        .collect::<Result<Vec<_>, _>>()?;
    std::fs::create_dir_all(&req.out).map_err(|e| format!("{}: {e}", req.out.display()))?;
    let mut models = Models::new(store);
    let pictures = TextureStore::new(store);
    let mut paints: BTreeMap<Option<u32>, Option<Paint>> = BTreeMap::new();
    let mut written = 0;
    for m in &meshes {
        for (s, _, _) in &m.groups {
            if paints.contains_key(&s.map(|s| s.0)) {
                continue;
            }
            let paint = match s.and_then(|s| crate::survey::material_of(&mut models, &pictures, s))
            {
                None => Some(Paint::Color([0.6, 0.6, 0.6, 1.0])),
                Some(Material::Hidden) => None,
                Some(Material::Colour([b, g, r, a])) => Some(Paint::Color([
                    linear(r),
                    linear(g),
                    linear(b),
                    f32::from(a) / 255.0,
                ])),
                Some(Material::Picture(img)) => {
                    let file = format!("tex_{:08X}.png", s.map_or(0, |s| s.0));
                    let rgba: Vec<u8> = img
                        .bgra
                        .iter()
                        .flat_map(|p| [p[2], p[1], p[0], p[3]])
                        .collect();
                    let cut = img.bgra.iter().any(|p| p[3] < 128);
                    write_png(&req.out.join(&file), img.width, img.height, &rgba)?;
                    written += 1;
                    Some(Paint::Picture { file, cut })
                }
            };
            paints.insert(s.map(|s| s.0), paint);
        }
    }
    // What is not drawn is no part of the body, nor of what the joints are hung from.
    for m in &mut meshes {
        m.groups
            .retain(|(s, _, _)| paints.get(&s.map(|s| s.0)).is_some_and(Option::is_some));
    }
    let parents = parents_of(&joints, &meshes);
    let name = format!("0x{:08X}", req.setup.0);
    let (doc, bin) = gltf(
        &name,
        &joints,
        &parents,
        &meshes,
        &|s| paints.get(&s.map(|s| s.0)).cloned().flatten(),
        &clips,
    );
    let gltf = req.out.join("skeleton.gltf");
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    std::fs::write(&gltf, text).map_err(|e| format!("{}: {e}", gltf.display()))?;
    let bin_path = req.out.join("skeleton.bin");
    std::fs::write(&bin_path, bin).map_err(|e| format!("{}: {e}", bin_path.display()))?;
    let triangles = doc["extras"]["triangles"]
        .as_u64()
        .and_then(|t| usize::try_from(t).ok())
        .unwrap_or(0);
    Ok(Written {
        gltf,
        joints: joints.len(),
        triangles,
        pictures: written,
        clips: clips.len(),
        parents,
    })
}

fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    fn vert(p: [f32; 3]) -> Vert {
        Vert {
            position: p,
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        }
    }

    /// A chain of three parts up the third axis, a metre apart, the middle one turned and scaled,
    /// each with a triangle round its origin; and a part with no triangles at the top.
    fn chain() -> (Vec<Joint>, Vec<PartMesh>) {
        let at = |z: f32| Mat4::from_translation(Vec3::new(0.0, 0.0, z));
        let joints = vec![
            Joint {
                frame: at(0.0),
                scale: Vec3::ONE,
            },
            Joint {
                frame: at(1.0) * Mat4::from_rotation_z(0.5),
                scale: Vec3::splat(2.0),
            },
            Joint {
                frame: at(2.0),
                scale: Vec3::ONE,
            },
            Joint {
                frame: at(3.0),
                scale: Vec3::ONE,
            },
        ];
        let tri = |z: f32| PartMesh {
            groups: vec![(
                Some(DataId(0x0800_0001)),
                false,
                vec![[
                    vert([0.0, 0.0, z]),
                    vert([0.1, 0.0, z]),
                    vert([0.0, 0.1, z]),
                ]],
            )],
        };
        let meshes = vec![tri(0.0), tri(1.0), tri(2.0), PartMesh::default()];
        (joints, meshes)
    }

    #[test]
    fn each_part_hangs_from_the_part_whose_triangles_come_nearest_its_origin() {
        let (joints, meshes) = chain();
        assert_eq!(
            parents_of(&joints, &meshes),
            [None, Some(0), Some(1), Some(2)]
        );
        // With nothing drawn, every part is a root.
        assert_eq!(hierarchy(&[Vec3::ZERO; 2], &[vec![], vec![]]), [None, None]);
    }

    #[test]
    fn the_skeleton_names_a_joint_a_part_and_binds_each_vertex_wholly_to_its_part() {
        let (joints, meshes) = chain();
        let parents = parents_of(&joints, &meshes);
        let paint = |_: Option<DataId>| Some(Paint::Color([1.0; 4]));
        let (doc, bin) = gltf("test", &joints, &parents, &meshes, &paint, &[]);
        let names: Vec<&str> = doc["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .filter_map(|n| n["name"].as_str())
            .collect();
        assert_eq!(names[..4], ["part_00", "part_01", "part_02", "part_03"]);
        // Read back as the art pack reads a body: each part's matrix takes its rest triangle to
        // the part's own space, its scale undone.
        let text = serde_json::to_vec(&doc).expect("json");
        let skin = dereth_scene::art_pack::gltf::decode_skin(&text, "b/skeleton.gltf", &mut |p| {
            assert_eq!(p, "b/skeleton.bin");
            Ok(bin.clone())
        })
        .expect("the body reads");
        assert_eq!(skin.primitives.len(), 1);
        let p = &skin.primitives[0];
        assert_eq!(
            p.parts.iter().map(|j| j[0]).collect::<Vec<_>>(),
            [0, 0, 0, 1, 1, 1, 2, 2, 2]
        );
        assert!(p.weights.iter().all(|w| *w == [1.0, 0.0, 0.0, 0.0]));
        for (part, bind) in &skin.inverse_binds {
            let j = joints[usize::from(*part)];
            let rest = j.frame * Mat4::from_scale(j.scale);
            assert!(
                (*bind * rest).abs_diff_eq(Mat4::IDENTITY, 1e-5),
                "part {part}"
            );
        }
    }
}
