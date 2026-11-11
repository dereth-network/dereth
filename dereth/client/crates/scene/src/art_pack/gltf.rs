//! glTF 2.0 models, read into the triangles and materials an art pack draws.
//!
//! What is read: the default scene's nodes and their transforms, triangle primitives, their
//! positions, normals, tangents, first texture coordinates, first colours and indices, and their
//! metallic-roughness materials (base colour, normal, metallic-roughness and occlusion pictures,
//! their factors, the alpha mode and whether both faces are drawn). Buffers and pictures are files
//! beside the model. Anything else a model needs (embedded or binary buffers, sparse accessors,
//! other primitive modes, a second set of texture coordinates) refuses the model, so a pack never
//! draws half of one.
//!
//! glTF's height is its second axis and the world's its third: every position and direction is
//! turned onto the world's axes as it is read, so a model exported upright from a modelling tool
//! stands upright.

use dereth_primitives::DataId;
use glam::{Mat3, Mat4, Quat, Vec2, Vec3, Vec4};
use serde_json::Value;

/// The floats of one vertex: position (3), normal (3), tangent and its handedness (4), texture
/// coordinates (2) and colour (4).
pub const VERTEX_FLOATS: usize = 16;

/// One part of a model: triangles of one material, in the model's own space with height on the
/// third axis, counter-clockwise seen from the front.
#[derive(Debug, Clone, PartialEq)]
pub struct Primitive {
    /// [`VERTEX_FLOATS`] floats a vertex.
    pub vertices: Vec<f32>,
    /// Three a triangle.
    pub indices: Vec<u32>,
    /// How it is drawn.
    pub material: Material,
}

/// A metallic-roughness material, its pictures named by path relative to the pack.
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    /// The base colour factor, linear, with alpha.
    pub base_color: [f32; 4],
    /// The base colour picture.
    pub base_color_image: Option<String>,
    /// The tangent-space normal picture.
    pub normal_image: Option<String>,
    /// The normal picture's strength.
    pub normal_scale: f32,
    /// The metallic-roughness picture: roughness in green, metalness in blue.
    pub metal_rough_image: Option<String>,
    /// The metalness factor.
    pub metallic: f32,
    /// The roughness factor.
    pub roughness: f32,
    /// The occlusion picture: occlusion in red.
    pub occlusion_image: Option<String>,
    /// The occlusion's strength.
    pub occlusion_strength: f32,
    /// The alpha a pixel is cut below, for a cut-out material.
    pub alpha_cutoff: Option<f32>,
    /// Whether both faces are drawn.
    pub double_sided: bool,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            base_color: [1.0; 4],
            base_color_image: None,
            normal_image: None,
            normal_scale: 1.0,
            metal_rough_image: None,
            metallic: 1.0,
            roughness: 1.0,
            occlusion_image: None,
            occlusion_strength: 1.0,
            alpha_cutoff: None,
            double_sided: false,
        }
    }
}

/// Why a model could not be read, as a sentence.
pub type Refusal = String;

/// The path of `uri` beside the model at `at` (both relative to the pack, `/` between parts):
/// percent escapes decoded, `.` and `..` resolved, and refused when it climbs out of the pack.
///
/// # Errors
/// A `data:` URI, or a path that climbs out of the pack.
pub fn beside(at: &str, uri: &str) -> Result<String, Refusal> {
    if uri.starts_with("data:") {
        return Err(format!("{at}: embedded data ({:.24}...) is not read", uri));
    }
    let mut decoded = Vec::with_capacity(uri.len());
    let bytes = uri.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&uri[i + 1..i + 3], 16) {
                decoded.push(b);
                i += 3;
                continue;
            }
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    let uri = String::from_utf8(decoded).map_err(|_| format!("{at}: {uri:?} is not UTF-8"))?;
    let mut parts: Vec<&str> = at.split('/').collect();
    parts.pop();
    for part in uri.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(format!("{at}: {uri:?} climbs out of the pack"));
                }
            }
            p => parts.push(p),
        }
    }
    Ok(parts.join("/"))
}

/// The world's axes from glTF's: height from the second axis onto the third.
fn upright(v: Vec3) -> Vec3 {
    Vec3::new(v.x, -v.z, v.y)
}

fn num(v: &Value, key: &str, default: f32) -> f32 {
    #[allow(clippy::cast_possible_truncation)] // a model's own numbers, read as f32 on purpose
    v.get(key)
        .and_then(Value::as_f64)
        .map_or(default, |x| x as f32)
}

fn index(v: &Value, key: &str) -> Option<usize> {
    v.get(key)
        .and_then(Value::as_u64)
        .and_then(|i| usize::try_from(i).ok())
}

fn floats<const N: usize>(v: &Value, key: &str) -> Option<[f32; N]> {
    let a = v.get(key)?.as_array()?;
    if a.len() != N {
        return None;
    }
    let mut out = [0.0; N];
    for (o, x) in out.iter_mut().zip(a) {
        #[allow(clippy::cast_possible_truncation)] // a model's own numbers
        {
            *o = x.as_f64()? as f32;
        }
    }
    Some(out)
}

/// One accessor's elements, each widened to floats (integers normalised where the accessor
/// says so).
struct Accessor {
    count: usize,
    width: usize,
    values: Vec<f32>,
}

fn component_size(kind: u64) -> Option<usize> {
    match kind {
        5120 | 5121 => Some(1),
        5122 | 5123 => Some(2),
        5125 | 5126 => Some(4),
        _ => None,
    }
}

#[allow(clippy::cast_precision_loss)] // integer components widened to floats
fn component(bytes: &[u8], kind: u64, normalized: bool) -> f32 {
    match kind {
        5120 => {
            let v = f32::from(i8::from_le_bytes([bytes[0]]));
            if normalized {
                (v / 127.0).max(-1.0)
            } else {
                v
            }
        }
        5121 => {
            let v = f32::from(bytes[0]);
            if normalized {
                v / 255.0
            } else {
                v
            }
        }
        5122 => {
            let v = f32::from(i16::from_le_bytes([bytes[0], bytes[1]]));
            if normalized {
                (v / 32767.0).max(-1.0)
            } else {
                v
            }
        }
        5123 => {
            let v = f32::from(u16::from_le_bytes([bytes[0], bytes[1]]));
            if normalized {
                v / 65535.0
            } else {
                v
            }
        }
        5125 => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32,
        _ => f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
    }
}

/// The accessor `i`'s elements, and their indices when they are unsigned integers.
fn accessor(
    doc: &Value,
    buffers: &[Vec<u8>],
    i: usize,
) -> Result<(Accessor, Option<Vec<u32>>), Refusal> {
    let a = doc
        .get("accessors")
        .and_then(|l| l.get(i))
        .ok_or_else(|| format!("no accessor {i}"))?;
    if a.get("sparse").is_some() {
        return Err(format!("accessor {i} is sparse, which is not read"));
    }
    let count = index(a, "count").ok_or_else(|| format!("accessor {i} has no count"))?;
    let kind = a
        .get("componentType")
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("accessor {i} has no component type"))?;
    let size =
        component_size(kind).ok_or_else(|| format!("accessor {i}: component type {kind}"))?;
    let width = match a.get("type").and_then(Value::as_str) {
        Some("SCALAR") => 1,
        Some("VEC2") => 2,
        Some("VEC3") => 3,
        Some("VEC4") => 4,
        Some("MAT4") => 16,
        other => return Err(format!("accessor {i}: type {other:?} is not read")),
    };
    let normalized = a
        .get("normalized")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let view_index = index(a, "bufferView").ok_or_else(|| format!("accessor {i} has no data"))?;
    let view = doc
        .get("bufferViews")
        .and_then(|l| l.get(view_index))
        .ok_or_else(|| format!("no buffer view {view_index}"))?;
    let buffer = buffers
        .get(index(view, "buffer").unwrap_or(usize::MAX))
        .ok_or_else(|| format!("buffer view {view_index} names no buffer"))?;
    let element = size * width;
    let stride = index(view, "byteStride").unwrap_or(element).max(element);
    let start = index(view, "byteOffset").unwrap_or(0) + index(a, "byteOffset").unwrap_or(0);
    let view_end = index(view, "byteOffset").unwrap_or(0)
        + index(view, "byteLength")
            .ok_or_else(|| format!("buffer view {view_index} has no length"))?;
    let end = if count == 0 {
        start
    } else {
        start + stride * (count - 1) + element
    };
    if end > view_end || view_end > buffer.len() {
        return Err(format!("accessor {i} reaches past its buffer"));
    }
    let mut values = Vec::with_capacity(count * width);
    let integers = matches!(kind, 5121 | 5123 | 5125) && !normalized;
    let mut ints = integers.then(|| Vec::with_capacity(count * width));
    for e in 0..count {
        let at = start + e * stride;
        for c in 0..width {
            let b = &buffer[at + c * size..at + (c + 1) * size];
            values.push(component(b, kind, normalized));
            if let Some(ints) = ints.as_mut() {
                ints.push(match size {
                    1 => u32::from(b[0]),
                    2 => u32::from(u16::from_le_bytes([b[0], b[1]])),
                    _ => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                });
            }
        }
    }
    Ok((
        Accessor {
            count,
            width,
            values,
        },
        ints,
    ))
}

/// The local transform of `node`.
fn local(node: &Value) -> Mat4 {
    if let Some(m) = floats::<16>(node, "matrix") {
        return Mat4::from_cols_array(&m);
    }
    let t = floats::<3>(node, "translation").map_or(Vec3::ZERO, Vec3::from);
    let r =
        floats::<4>(node, "rotation").map_or(Quat::IDENTITY, |q| Quat::from_array(q).normalize());
    let s = floats::<3>(node, "scale").map_or(Vec3::ONE, Vec3::from);
    Mat4::from_scale_rotation_translation(s, r, t)
}

/// The picture a texture reference names, as a path relative to the pack.
fn picture(doc: &Value, at: &str, reference: Option<&Value>) -> Result<Option<String>, Refusal> {
    let Some(r) = reference else {
        return Ok(None);
    };
    if index(r, "texCoord").unwrap_or(0) != 0 {
        return Err(format!(
            "{at}: a picture on the second texture coordinates is not read"
        ));
    }
    let t = index(r, "index").ok_or_else(|| format!("{at}: a texture reference names nothing"))?;
    let source = doc
        .get("textures")
        .and_then(|l| l.get(t))
        .and_then(|t| index(t, "source"))
        .ok_or_else(|| format!("{at}: texture {t} names no picture"))?;
    let uri = doc
        .get("images")
        .and_then(|l| l.get(source))
        .and_then(|i| i.get("uri"))
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{at}: picture {source} is not a file beside the model"))?;
    beside(at, uri).map(Some)
}

fn material(doc: &Value, at: &str, i: Option<usize>) -> Result<Material, Refusal> {
    let Some(i) = i else {
        return Ok(Material::default());
    };
    let m = doc
        .get("materials")
        .and_then(|l| l.get(i))
        .ok_or_else(|| format!("{at}: no material {i}"))?;
    let pbr = m.get("pbrMetallicRoughness");
    let empty = Value::Null;
    let pbr_v = pbr.unwrap_or(&empty);
    let normal = m.get("normalTexture");
    let occlusion = m.get("occlusionTexture");
    Ok(Material {
        base_color: floats::<4>(pbr_v, "baseColorFactor").unwrap_or([1.0; 4]),
        base_color_image: picture(doc, at, pbr_v.get("baseColorTexture"))?,
        normal_image: picture(doc, at, normal)?,
        normal_scale: normal.map_or(1.0, |n| num(n, "scale", 1.0)),
        metal_rough_image: picture(doc, at, pbr_v.get("metallicRoughnessTexture"))?,
        metallic: num(pbr_v, "metallicFactor", 1.0),
        roughness: num(pbr_v, "roughnessFactor", 1.0),
        occlusion_image: picture(doc, at, occlusion)?,
        occlusion_strength: occlusion.map_or(1.0, |o| num(o, "strength", 1.0)),
        alpha_cutoff: match m.get("alphaMode").and_then(Value::as_str) {
            Some("MASK") => Some(num(m, "alphaCutoff", 0.5)),
            _ => None,
        },
        double_sided: m
            .get("doubleSided")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Tangents for triangles whose model carries none: each vertex's direction of growing first
/// texture coordinate across its triangles, square to its normal, with the handedness of the
/// second.
fn tangents(positions: &[Vec3], normals: &[Vec3], uvs: &[Vec2], indices: &[u32]) -> Vec<Vec4> {
    let n = positions.len();
    let mut tan = vec![Vec3::ZERO; n];
    let mut bit = vec![Vec3::ZERO; n];
    for t in indices.as_chunks::<3>().0.iter() {
        let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
        if a >= n || b >= n || c >= n {
            continue;
        }
        let (e1, e2) = (positions[b] - positions[a], positions[c] - positions[a]);
        let (d1, d2) = (uvs[b] - uvs[a], uvs[c] - uvs[a]);
        let det = d1.x * d2.y - d2.x * d1.y;
        if det.abs() < 1e-12 {
            continue;
        }
        let r = 1.0 / det;
        let s = (e1 * d2.y - e2 * d1.y) * r;
        let u = (e2 * d1.x - e1 * d2.x) * r;
        for v in [a, b, c] {
            tan[v] += s;
            bit[v] += u;
        }
    }
    (0..n)
        .map(|v| {
            let nrm = normals[v];
            let t = (tan[v] - nrm * nrm.dot(tan[v])).normalize_or_zero();
            let t = if t == Vec3::ZERO {
                nrm.any_orthonormal_vector()
            } else {
                t
            };
            let w = if nrm.cross(t).dot(bit[v]) < 0.0 {
                -1.0
            } else {
                1.0
            };
            t.extend(w)
        })
        .collect()
}

/// A glTF document whose text is `json`, at `at` in the pack, and its buffers, read through
/// `read` by path relative to the pack.
fn open_doc(
    json: &[u8],
    at: &str,
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>, Refusal>,
) -> Result<(Value, Vec<Vec<u8>>), Refusal> {
    if json.starts_with(b"glTF") {
        return Err(format!(
            "{at}: a binary glTF is not read; export it as .gltf"
        ));
    }
    let doc: Value = serde_json::from_slice(json).map_err(|e| format!("{at}: not glTF: {e}"))?;
    let version = doc
        .get("asset")
        .and_then(|a| a.get("version"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if !version.starts_with("2.") {
        return Err(format!("{at}: glTF version {version:?}, and 2.0 is read"));
    }
    if let Some(required) = doc.get("extensionsRequired").and_then(Value::as_array) {
        if !required.is_empty() {
            return Err(format!("{at}: needs the extensions {required:?}"));
        }
    }
    let mut buffers = Vec::new();
    for (i, b) in doc
        .get("buffers")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .enumerate()
    {
        let uri = b
            .get("uri")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{at}: buffer {i} is not a file beside the model"))?;
        let bytes = read(&beside(at, uri)?)?;
        if bytes.len() < index(b, "byteLength").unwrap_or(0) {
            return Err(format!("{at}: buffer {i} is shorter than it says"));
        }
        buffers.push(bytes);
    }
    Ok((doc, buffers))
}

/// One triangle primitive `p` of a mesh, its vertices taken through `world` and turned upright.
#[allow(clippy::too_many_lines)] // one pass over a primitive's attributes
fn primitive(
    doc: &Value,
    buffers: &[Vec<u8>],
    at: &str,
    p: &Value,
    world: Mat4,
) -> Result<Primitive, Refusal> {
    let turn = Mat3::from_mat4(world);
    let normal_turn = turn.inverse().transpose();
    let mirrored = turn.determinant() < 0.0;
    if index(p, "mode").unwrap_or(4) != 4 {
        return Err(format!(
            "{at}: a primitive that is not triangles is not read"
        ));
    }
    let attrs = p
        .get("attributes")
        .ok_or_else(|| format!("{at}: a primitive with no attributes"))?;
    let read_attr = |name: &str| -> Result<Option<Accessor>, Refusal> {
        index(attrs, name)
            .map(|i| accessor(doc, buffers, i).map(|(a, _)| a))
            .transpose()
            .map_err(|e| format!("{at}: {e}"))
    };
    let pos =
        read_attr("POSITION")?.ok_or_else(|| format!("{at}: a primitive with no positions"))?;
    if pos.width != 3 {
        return Err(format!("{at}: positions are not three numbers"));
    }
    let count = pos.count;
    let positions: Vec<Vec3> = pos
        .values
        .as_chunks::<3>()
        .0
        .iter()
        .map(|v| upright(world.transform_point3(Vec3::new(v[0], v[1], v[2]))))
        .collect();
    let indices: Vec<u32> = match index(p, "indices") {
        Some(i) => {
            let (_, ints) = accessor(doc, buffers, i).map_err(|e| format!("{at}: {e}"))?;
            ints.ok_or_else(|| format!("{at}: indices that are not unsigned integers"))?
        }
        None => (0..u32::try_from(count).unwrap_or(u32::MAX)).collect(),
    };
    if !indices.len().is_multiple_of(3) || indices.iter().any(|i| *i as usize >= count) {
        return Err(format!("{at}: indices that do not make whole triangles"));
    }
    let indices: Vec<u32> = if mirrored {
        indices
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|t| [t[0], t[2], t[1]])
            .collect()
    } else {
        indices
    };
    let normals: Vec<Vec3> = match read_attr("NORMAL")? {
        Some(a) if a.width == 3 && a.count == count => a
            .values
            .as_chunks::<3>()
            .0
            .iter()
            .map(|v| upright((normal_turn * Vec3::new(v[0], v[1], v[2])).normalize_or_zero()))
            .collect(),
        Some(_) => return Err(format!("{at}: normals that do not match the positions")),
        None => {
            // Flat normals, each vertex taking its last triangle's.
            let mut n = vec![Vec3::Z; count];
            for t in indices.as_chunks::<3>().0.iter() {
                let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
                let f = (positions[b] - positions[a])
                    .cross(positions[c] - positions[a])
                    .normalize_or_zero();
                for v in [a, b, c] {
                    n[v] = f;
                }
            }
            n
        }
    };
    let uvs: Vec<Vec2> = match read_attr("TEXCOORD_0")? {
        Some(a) if a.width == 2 && a.count == count => a
            .values
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| Vec2::new(v[0], v[1]))
            .collect(),
        Some(_) => return Err(format!("{at}: texture coordinates that do not match")),
        None => vec![Vec2::ZERO; count],
    };
    let colors: Vec<Vec4> = match read_attr("COLOR_0")? {
        Some(a) if a.width == 4 && a.count == count => a
            .values
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| Vec4::new(v[0], v[1], v[2], v[3]))
            .collect(),
        Some(a) if a.width == 3 && a.count == count => a
            .values
            .as_chunks::<3>()
            .0
            .iter()
            .map(|v| Vec4::new(v[0], v[1], v[2], 1.0))
            .collect(),
        Some(_) => return Err(format!("{at}: colours that do not match")),
        None => vec![Vec4::ONE; count],
    };
    let tangents: Vec<Vec4> = match read_attr("TANGENT")? {
        Some(a) if a.width == 4 && a.count == count => a
            .values
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| {
                let t = upright((turn * Vec3::new(v[0], v[1], v[2])).normalize_or_zero());
                t.extend(if mirrored { -v[3] } else { v[3] })
            })
            .collect(),
        _ => tangents(&positions, &normals, &uvs, &indices),
    };
    let mut vertices = Vec::with_capacity(count * VERTEX_FLOATS);
    for v in 0..count {
        vertices.extend(positions[v].to_array());
        vertices.extend(normals[v].to_array());
        vertices.extend(tangents[v].to_array());
        vertices.extend(uvs[v].to_array());
        vertices.extend(colors[v].to_array());
    }
    Ok(Primitive {
        vertices,
        indices,
        material: material(doc, at, index(p, "material"))?,
    })
}

/// The primitives of the model whose glTF text is `json`, at `at` in the pack, its buffers read
/// through `read` by path relative to the pack.
///
/// # Errors
/// What the model needs that is not read, or a buffer that will not read or is too short.
pub fn decode(
    json: &[u8],
    at: &str,
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>, Refusal>,
) -> Result<Vec<Primitive>, Refusal> {
    let (doc, buffers) = open_doc(json, at, read)?;
    let scene = index(&doc, "scene").unwrap_or(0);
    let roots: Vec<usize> = doc
        .get("scenes")
        .and_then(|s| s.get(scene))
        .and_then(|s| s.get("nodes"))
        .and_then(Value::as_array)
        .map(|n| {
            n.iter()
                .filter_map(|i| i.as_u64().and_then(|i| usize::try_from(i).ok()))
                .collect()
        })
        .unwrap_or_default();
    let nodes = doc
        .get("nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut stack: Vec<(usize, Mat4, u32)> =
        roots.into_iter().map(|n| (n, Mat4::IDENTITY, 0)).collect();
    let mut out = Vec::new();
    while let Some((n, parent, depth)) = stack.pop() {
        if depth > 64 {
            return Err(format!("{at}: the node tree is deeper than 64"));
        }
        let node = nodes.get(n).ok_or_else(|| format!("{at}: no node {n}"))?;
        let world = parent * local(node);
        if let Some(children) = node.get("children").and_then(Value::as_array) {
            for c in children {
                if let Some(c) = c.as_u64().and_then(|c| usize::try_from(c).ok()) {
                    stack.push((c, world, depth + 1));
                }
            }
        }
        let Some(mesh) = index(node, "mesh") else {
            continue;
        };
        let primitives = doc
            .get("meshes")
            .and_then(|m| m.get(mesh))
            .and_then(|m| m.get("primitives"))
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{at}: no mesh {mesh}"))?;
        for p in primitives {
            out.push(primitive(&doc, &buffers, at, p, world)?);
        }
    }
    Ok(out)
}

/// One primitive of a skinned body: its triangles in the body's bind space, upright, and each
/// vertex's parts (by their place in the setup) and its weight on each, summing to one.
#[derive(Debug, Clone, PartialEq)]
pub struct SkinPrimitive {
    pub primitive: Primitive,
    pub parts: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
}

/// A skinned body: its primitives, and for each part a vertex follows, the matrix that takes the
/// body's bind space to the part's own (upright both, the part's scale applied).
#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    pub primitives: Vec<SkinPrimitive>,
    pub inverse_binds: Vec<(u16, Mat4)>,
}

/// The part a node names: `part_NN`, or `extras.part`.
fn part_named(node: &Value) -> Option<u16> {
    if let Some(p) = node
        .get("extras")
        .and_then(|e| e.get("part"))
        .and_then(Value::as_u64)
    {
        return u16::try_from(p).ok();
    }
    let name = node.get("name").and_then(Value::as_str)?;
    let digits = name.strip_prefix("part_")?;
    // A tool may add a suffix of its own to a name it has seen twice (`part_03.001`).
    let digits = digits.split(['.', ' ']).next()?;
    digits.parse().ok()
}

/// The skinned body of the glTF whose text is `json`, at `at` in the pack, its buffers read
/// through `read`: the first node that has both a mesh and a skin. Each joint follows the part it
/// names (`part_NN`, or `extras.part`), or the nearest node above it in the node tree that names
/// one. The positions are the mesh's own, in its bind space: the node's transform is not applied
/// to a skinned mesh.
///
/// # Errors
/// As [`decode`]; no skinned mesh; a joint that follows no part; or joints and weights that do
/// not match the positions.
#[allow(clippy::too_many_lines)] // one pass over the skin, then over its primitives
pub fn decode_skin(
    json: &[u8],
    at: &str,
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>, Refusal>,
) -> Result<Skin, Refusal> {
    let (doc, buffers) = open_doc(json, at, read)?;
    let nodes = doc
        .get("nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut parent = vec![None; nodes.len()];
    for (i, n) in nodes.iter().enumerate() {
        for c in n
            .get("children")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice)
        {
            if let Some(slot) = c
                .as_u64()
                .and_then(|c| usize::try_from(c).ok())
                .and_then(|c| parent.get_mut(c))
            {
                *slot = Some(i);
            }
        }
    }
    // Each node's transform in the scene, its parents' applied.
    let world = |mut n: usize| -> Mat4 {
        let mut m = Mat4::IDENTITY;
        for _ in 0..=64 {
            let Some(node) = nodes.get(n) else {
                break;
            };
            m = local(node) * m;
            match parent[n] {
                Some(p) => n = p,
                None => break,
            }
        }
        m
    };
    let follows = |mut n: usize| -> Option<(u16, usize)> {
        for _ in 0..=64 {
            if let Some(p) = part_named(nodes.get(n)?) {
                return Some((p, n));
            }
            n = parent.get(n).copied().flatten()?;
        }
        None
    };
    let (body, skin) = nodes
        .iter()
        .enumerate()
        .find_map(|(i, n)| {
            n.get("mesh")?;
            Some((i, index(n, "skin")?))
        })
        .ok_or_else(|| format!("{at}: no skinned mesh"))?;
    let skin_v = doc
        .get("skins")
        .and_then(|l| l.get(skin))
        .ok_or_else(|| format!("{at}: no skin {skin}"))?;
    let joints: Vec<usize> = skin_v
        .get("joints")
        .and_then(Value::as_array)
        .map(|j| {
            j.iter()
                .filter_map(|i| i.as_u64().and_then(|i| usize::try_from(i).ok()))
                .collect()
        })
        .unwrap_or_default();
    let binds: Option<Vec<Mat4>> = match index(skin_v, "inverseBindMatrices") {
        Some(i) => {
            let (a, _) = accessor(&doc, &buffers, i).map_err(|e| format!("{at}: {e}"))?;
            if a.width != 16 || a.count != joints.len() {
                return Err(format!(
                    "{at}: inverse bind matrices that do not match the joints"
                ));
            }
            Some(
                a.values
                    .as_chunks::<16>()
                    .0
                    .iter()
                    .map(Mat4::from_cols_array)
                    .collect(),
            )
        }
        None => None,
    };
    // Each joint's part, and each part's inverse bind matrix: its own joint's, or where its node
    // is no joint of the skin, the inverse of the node's place in the scene.
    let mut part_of_joint = Vec::with_capacity(joints.len());
    let mut by_part: std::collections::BTreeMap<u16, Mat4> = std::collections::BTreeMap::new();
    for (k, &j) in joints.iter().enumerate() {
        let (part, node) =
            follows(j).ok_or_else(|| format!("{at}: joint {k} (node {j}) follows no part"))?;
        part_of_joint.push(part);
        let own = binds.as_ref().filter(|_| node == j).map(|b| b[k]);
        if own.is_some() || !by_part.contains_key(&part) {
            by_part.insert(part, own.unwrap_or_else(|| world(node).inverse()));
        }
    }
    // glTF's axes on both sides: the body's bind space in, the part's own space out.
    let up = Mat4::from_cols(
        Vec4::X,
        Vec4::new(0.0, 0.0, 1.0, 0.0),
        Vec4::new(0.0, -1.0, 0.0, 0.0),
        Vec4::W,
    );
    let inverse_binds = by_part
        .into_iter()
        .map(|(p, m)| (p, up * m * up.inverse()))
        .collect();
    let mesh = index(&nodes[body], "mesh").unwrap_or(0);
    let listed = doc
        .get("meshes")
        .and_then(|m| m.get(mesh))
        .and_then(|m| m.get("primitives"))
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{at}: no mesh {mesh}"))?;
    let mut primitives = Vec::with_capacity(listed.len());
    for p in listed {
        let primitive = primitive(&doc, &buffers, at, p, Mat4::IDENTITY)?;
        let count = primitive.vertices.len() / VERTEX_FLOATS;
        let attrs = p.get("attributes").unwrap_or(&Value::Null);
        let (Some(ji), Some(wi)) = (index(attrs, "JOINTS_0"), index(attrs, "WEIGHTS_0")) else {
            return Err(format!(
                "{at}: a skinned primitive with no joints or weights"
            ));
        };
        let (ja, jints) = accessor(&doc, &buffers, ji).map_err(|e| format!("{at}: {e}"))?;
        let (wa, _) = accessor(&doc, &buffers, wi).map_err(|e| format!("{at}: {e}"))?;
        let jints = jints.ok_or_else(|| format!("{at}: joints that are not unsigned integers"))?;
        if ja.width != 4 || wa.width != 4 || ja.count != count || wa.count != count {
            return Err(format!(
                "{at}: joints and weights that do not match the positions"
            ));
        }
        let mut parts = Vec::with_capacity(count);
        let mut weights = Vec::with_capacity(count);
        for v in 0..count {
            // The weights on one part summed, the largest first, made to sum to one.
            let mut on: Vec<(u16, f32)> = Vec::with_capacity(4);
            for c in 0..4 {
                let w = wa.values[v * 4 + c];
                if w.is_nan() || w <= 0.0 {
                    continue;
                }
                let j = jints[v * 4 + c] as usize;
                let part = *part_of_joint.get(j).ok_or_else(|| {
                    format!("{at}: a vertex names joint {j}, which the skin has not")
                })?;
                match on.iter_mut().find(|(p, _)| *p == part) {
                    Some(e) => e.1 += w,
                    None => on.push((part, w)),
                }
            }
            on.sort_by(|a, b| b.1.total_cmp(&a.1));
            let total: f32 = on.iter().map(|e| e.1).sum();
            let mut ps = [0u16; 4];
            let mut ws = [0.0f32; 4];
            if total > 0.0 {
                for (k, (p, w)) in on.iter().enumerate() {
                    ps[k] = *p;
                    ws[k] = *w / total;
                }
            } else {
                ps[0] = part_of_joint.first().copied().unwrap_or(0);
                ws[0] = 1.0;
            }
            parts.push(ps);
            weights.push(ws);
        }
        primitives.push(SkinPrimitive {
            primitive,
            parts,
            weights,
        });
    }
    Ok(Skin {
        primitives,
        inverse_binds,
    })
}

/// The id a pack names a model by, for messages.
#[must_use]
pub fn label(id: DataId) -> String {
    format!("{:#010X}", id.0)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi art packs)
    use super::*;

    /// One triangle, its height on glTF's second axis, with a cut-out material and a picture.
    fn one_triangle() -> (String, Vec<u8>) {
        let mut bin = Vec::new();
        for p in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 2.0, 0.0]] {
            for c in p {
                bin.extend(c.to_le_bytes());
            }
        }
        for uv in [[0.0f32, 1.0], [1.0, 1.0], [0.0, 0.0]] {
            for c in uv {
                bin.extend(c.to_le_bytes());
            }
        }
        for i in [0u16, 1, 2] {
            bin.extend(i.to_le_bytes());
        }
        bin.extend([0, 0]);
        let json = r#"{
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"mesh": 0, "translation": [0, 0, 5]}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "TEXCOORD_0": 1},
                "indices": 2, "material": 0}]}],
            "materials": [{"alphaMode": "MASK", "doubleSided": true,
                "pbrMetallicRoughness": {"baseColorTexture": {"index": 0},
                    "metallicFactor": 0, "roughnessFactor": 0.75}}],
            "textures": [{"source": 0}],
            "images": [{"uri": "../shared/needle%20card.png"}],
            "buffers": [{"uri": "tri.bin", "byteLength": 62}],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 36},
                {"buffer": 0, "byteOffset": 36, "byteLength": 24},
                {"buffer": 0, "byteOffset": 60, "byteLength": 6}
            ],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC2"},
                {"bufferView": 2, "componentType": 5123, "count": 3, "type": "SCALAR"}
            ]
        }"#;
        (json.to_owned(), bin)
    }

    #[test]
    fn a_model_is_read_upright_with_its_material_and_its_pictures_beside_it() {
        let (json, bin) = one_triangle();
        let mut asked = Vec::new();
        let prims = decode(json.as_bytes(), "tree/lod0.gltf", &mut |path| {
            asked.push(path.to_owned());
            Ok(bin.clone())
        })
        .expect("the triangle reads");
        assert_eq!(asked, ["tree/tri.bin"]);
        assert_eq!(prims.len(), 1);
        let p = &prims[0];
        assert_eq!(p.indices, [0, 1, 2]);
        let at = |v: usize| {
            let f = &p.vertices[v * VERTEX_FLOATS..(v + 1) * VERTEX_FLOATS];
            Vec3::new(f[0], f[1], f[2])
        };
        // glTF's second axis is the height; the node's push along its third is a step south.
        assert!(at(0).distance(Vec3::new(0.0, -5.0, 0.0)) < 1e-6);
        assert!(at(1).distance(Vec3::new(1.0, -5.0, 0.0)) < 1e-6);
        assert!(at(2).distance(Vec3::new(0.0, -5.0, 2.0)) < 1e-6);
        // No normals given: the triangle's own, facing south here, toward glTF's front.
        let n = Vec3::new(p.vertices[3], p.vertices[4], p.vertices[5]);
        assert!(n.distance(Vec3::new(0.0, -1.0, 0.0)) < 1e-6, "{n}");
        // The tangent runs the way the first texture coordinate grows: east.
        let t = Vec3::new(p.vertices[6], p.vertices[7], p.vertices[8]);
        assert!(t.distance(Vec3::X) < 1e-6, "{t}");
        assert_eq!(
            p.vertices[12..16],
            [1.0; 4],
            "white where no colour is given"
        );
        assert_eq!(p.material.alpha_cutoff, Some(0.5));
        assert!(p.material.double_sided);
        assert_eq!(p.material.metallic, 0.0);
        assert_eq!(p.material.roughness, 0.75);
        assert_eq!(
            p.material.base_color_image.as_deref(),
            Some("shared/needle card.png")
        );
    }

    #[test]
    fn a_model_the_reader_cannot_draw_whole_is_refused() {
        let (json, bin) = one_triangle();
        let refused = |from: &str, to: &str| {
            assert!(json.contains(from), "{from}");
            decode(json.replacen(from, to, 1).as_bytes(), "m.gltf", &mut |_| {
                Ok(bin.clone())
            })
            .is_err()
        };
        assert!(refused("\"2.0\"", "\"1.0\""));
        assert!(refused("\"indices\": 2,", "\"indices\": 2, \"mode\": 1,"));
        assert!(refused("\"byteLength\": 62", "\"byteLength\": 99"));
        assert!(refused(
            "\"count\": 3, \"type\": \"VEC3\"",
            "\"count\": 4, \"type\": \"VEC3\""
        ));
        assert!(refused("{\"index\": 0}", "{\"index\": 0, \"texCoord\": 1}"));
        assert!(refused(
            "\"tri.bin\"",
            "\"data:application/octet-stream;base64,AAAA\""
        ));
        assert!(decode(b"glTF\x02\0\0\0", "m.glb", &mut |_| Ok(Vec::new())).is_err());
        assert!(decode(json.as_bytes(), "m.gltf", &mut |_| Err("missing".into())).is_err());
    }

    #[test]
    fn a_path_beside_a_model_stays_inside_the_pack() {
        assert_eq!(beside("a/b/m.gltf", "t.png").as_deref(), Ok("a/b/t.png"));
        assert_eq!(beside("a/b/m.gltf", "../t.png").as_deref(), Ok("a/t.png"));
        assert_eq!(
            beside("m.gltf", "./x/t%2Dy.png").as_deref(),
            Ok("x/t-y.png")
        );
        assert!(beside("m.gltf", "../t.png").is_err());
        assert!(beside("a/m.gltf", "data:image/png;base64,AA").is_err());
    }

    /// A triangle skinned to two joints: `part_00` a metre up, and an unnamed joint under
    /// `part_02`, which stands a metre east of it. Its mesh node is moved, which a skinned mesh
    /// ignores.
    fn skinned_triangle() -> (String, Vec<u8>) {
        let mut bin = Vec::new();
        for p in [[0.0f32, 1.0, 0.0], [1.0, 1.0, 0.0], [0.5, 1.5, 0.0]] {
            for c in p {
                bin.extend(c.to_le_bytes());
            }
        }
        bin.extend([0u8, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0]);
        for w in [
            [1.0f32, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.25, 0.25, 0.0, 0.0],
        ] {
            for c in w {
                bin.extend(c.to_le_bytes());
            }
        }
        let ibm = [
            Mat4::from_translation(Vec3::new(0.0, -1.0, 0.0)),
            Mat4::from_translation(Vec3::new(9.0, 9.0, 9.0)),
        ];
        for m in ibm {
            for c in m.to_cols_array() {
                bin.extend(c.to_le_bytes());
            }
        }
        for i in [0u16, 1, 2, 0] {
            bin.extend(i.to_le_bytes());
        }
        let json = r#"{
            "asset": {"version": "2.0"},
            "scenes": [{"nodes": [0, 3]}],
            "nodes": [
                {"name": "part_00", "translation": [0, 1, 0], "children": [1]},
                {"name": "part_02", "translation": [1, 0, 0], "children": [2]},
                {"name": "twist"},
                {"mesh": 0, "skin": 0, "translation": [5, 5, 5]}
            ],
            "skins": [{"joints": [0, 2], "inverseBindMatrices": 3}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "JOINTS_0": 1,
                "WEIGHTS_0": 2}, "indices": 4}]}],
            "buffers": [{"uri": "skin.bin", "byteLength": 232}],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 36},
                {"buffer": 0, "byteOffset": 36, "byteLength": 12},
                {"buffer": 0, "byteOffset": 48, "byteLength": 48},
                {"buffer": 0, "byteOffset": 96, "byteLength": 128},
                {"buffer": 0, "byteOffset": 224, "byteLength": 6}
            ],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 1, "componentType": 5121, "count": 3, "type": "VEC4"},
                {"bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC4"},
                {"bufferView": 3, "componentType": 5126, "count": 2, "type": "MAT4"},
                {"bufferView": 4, "componentType": 5123, "count": 3, "type": "SCALAR"}
            ]
        }"#;
        (json.to_owned(), bin)
    }

    #[test]
    fn a_skinned_body_names_each_vertexs_parts_and_takes_its_bind_space_to_each_parts_own() {
        let (json, bin) = skinned_triangle();
        let skin = decode_skin(json.as_bytes(), "body/skin.gltf", &mut |path| {
            assert_eq!(path, "body/skin.bin");
            Ok(bin.clone())
        })
        .expect("the skin reads");
        assert_eq!(skin.primitives.len(), 1);
        let p = &skin.primitives[0];
        // The unnamed joint follows the part above it; a vertex's weights are made whole.
        assert_eq!(p.parts, [[0, 0, 0, 0], [2, 0, 0, 0], [0, 2, 0, 0]]);
        assert_eq!(p.weights[2], [0.5, 0.5, 0.0, 0.0]);
        // The positions are the mesh's own, upright, its node's move left out.
        let first = Vec3::from_slice(&p.primitive.vertices[0..3]);
        assert!(first.distance(Vec3::new(0.0, 0.0, 1.0)) < 1e-6, "{first}");
        // Part 0's own matrix; part 2's from its node's place, its joint being another node.
        let bind = |part: u16| {
            skin.inverse_binds
                .iter()
                .find(|(p, _)| *p == part)
                .map(|(_, m)| *m)
                .expect("a part's bind")
        };
        assert!(bind(0)
            .transform_point3(Vec3::new(0.0, 0.0, 1.0))
            .abs_diff_eq(Vec3::ZERO, 1e-6));
        assert!(bind(2)
            .transform_point3(Vec3::new(1.0, 0.0, 1.0))
            .abs_diff_eq(Vec3::ZERO, 1e-6));
        assert_eq!(skin.inverse_binds.len(), 2);
    }

    #[test]
    fn a_skin_that_is_not_whole_is_refused() {
        let (json, bin) = skinned_triangle();
        let refused = |from: &str, to: &str| {
            assert!(json.contains(from), "{from}");
            decode_skin(json.replacen(from, to, 1).as_bytes(), "m.gltf", &mut |_| {
                Ok(bin.clone())
            })
            .is_err()
        };
        assert!(
            refused("\"name\": \"part_00\", ", ""),
            "a joint that follows no part"
        );
        assert!(refused("\"skin\": 0, ", ""), "no skinned mesh");
        assert!(
            refused("\"WEIGHTS_0\": 2", "\"TEXCOORD_1\": 2"),
            "no weights"
        );
        assert!(refused(
            "\"count\": 2, \"type\": \"MAT4\"",
            "\"count\": 1, \"type\": \"MAT4\""
        ));
        assert!(decode_skin(json.as_bytes(), "m.gltf", &mut |_| Ok(bin.clone())).is_ok());
    }
}
