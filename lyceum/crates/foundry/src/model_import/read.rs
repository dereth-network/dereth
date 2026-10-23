//! Read a static scene and its local buffers and PNG pictures without writing source files.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use dereth_assets::{GfxObj, Polygon, SwVertex, VertexArray};
use dereth_primitives::num::math;
use dereth_primitives::{DataId, Frame, Quat, Vec3};
use glam::{Mat3, Mat4, Vec3 as GVec};

use super::{add, named, Record};
use crate::error::{Error, Loc};
use crate::source::{Located, NewRecord, RecordRef, Target};

pub(super) struct Scene {
    pub parts: Vec<Part>,
    materials: Vec<Material>,
}

struct Material {
    color: [f32; 4],
    image: Option<Vec<u8>>,
    double_sided: bool,
    opaque: bool,
}

pub(super) struct Part {
    pub name: String,
    pub frame: Frame,
    transform: Mat4,
    primitives: Vec<Primitive>,
}

struct Primitive {
    positions: Vec<GVec>,
    normals: Vec<GVec>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
    material: usize,
    double_sided: bool,
}

fn vector(v: GVec) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

fn matrix(frame: Frame) -> Mat4 {
    Mat4::from_rotation_translation(
        glam::Quat::from_xyzw(
            frame.rotation.x,
            frame.rotation.y,
            frame.rotation.z,
            frame.rotation.w,
        ),
        GVec::new(frame.origin.x, frame.origin.y, frame.origin.z),
    )
}

impl Part {
    pub fn world_vertices(&self) -> impl Iterator<Item = Vec3> + '_ {
        self.primitives
            .iter()
            .flat_map(|p| p.positions.iter())
            .map(|p| vector(self.transform.transform_point3(*p)))
    }

    pub fn model(&self, placement: Frame, surfaces: &[RecordRef]) -> Result<GfxObj, String> {
        let transform = matrix(placement).inverse() * self.transform;
        let normal_matrix = Mat3::from_mat4(transform).inverse().transpose();
        let mirrored = transform.determinant() < 0.0;
        let mut va = VertexArray {
            vertex_type: 1,
            vertices: Vec::new(),
        };
        let mut polygons = Vec::new();
        for p in &self.primitives {
            if va.vertices.len() + p.positions.len() > i16::MAX as usize
                || polygons.len() + p.indices.len() / 3 > i16::MAX as usize
            {
                return Err(format!(
                    "{} exceeds 32767 vertices or triangles; split the mesh",
                    self.name
                ));
            }
            let offset = u16::try_from(va.vertices.len()).map_err(|e| e.to_string())?;
            for (i, position) in p.positions.iter().enumerate() {
                let normal = normal_matrix.mul_vec3(p.normals[i]).normalize_or_zero();
                let position = transform.transform_point3(*position);
                if !position.is_finite() || !normal.is_finite() || normal.length_squared() < 1e-12 {
                    return Err(
                        "transformed vertices and normals must be finite and nonzero".into(),
                    );
                }
                va.vertices.push(SwVertex {
                    id: i16::try_from(va.vertices.len()).map_err(|e| e.to_string())?,
                    position: vector(position),
                    normal: vector(normal),
                    uvs: vec![(p.uvs[i][0], p.uvs[i][1])],
                });
            }
            for triangle in p.indices.as_chunks::<3>().0 {
                let mut ids = triangle
                    .iter()
                    .map(|v| {
                        u16::try_from(*v)
                            .map(|v| v + offset)
                            .map_err(|e| e.to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if mirrored {
                    ids.swap(1, 2);
                }
                let material = u16::try_from(p.material).map_err(|e| e.to_string())?;
                polygons.push(Polygon {
                    poly_id: i16::try_from(polygons.len()).map_err(|e| e.to_string())?,
                    num_pts: 3,
                    stippling: 1,
                    sides_type: i32::from(p.double_sided),
                    pos_surface: material,
                    neg_surface: material,
                    vertex_ids: ids,
                    pos_uv_indices: Some(vec![0; 3]),
                    neg_uv_indices: p.double_sided.then(|| vec![0; 3]),
                });
            }
        }
        let drawing = crate::bsp::drawing(&va, &polygons);
        let center = drawing
            .nodes
            .first()
            .and_then(|n| n.sphere)
            .map_or(Vec3::ZERO, |s| s.center);
        Ok(GfxObj {
            id: DataId(0),
            flags: 2,
            surfaces: vec![DataId(0); surfaces.len()],
            vertex_array: va,
            physics_polygons: Vec::new(),
            physics_bsp: None,
            sort_center: center,
            polygons,
            drawing_bsp: Some(drawing),
            did_degrade: None,
        })
    }
}

fn read_uri(root: &Path, file: &Path, uri: &str) -> Result<Vec<u8>, String> {
    if let Some(data) = uri.strip_prefix("data:") {
        let (kind, data) = data.split_once(',').ok_or("a data URI has no comma")?;
        if !kind.ends_with(";base64") {
            return Err("embedded buffers and pictures must use base64".into());
        }
        return base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| e.to_string());
    }
    let mut decoded = Vec::new();
    let bytes = uri.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let digits = bytes.get(i + 1..i + 3).ok_or("incomplete URI escape")?;
            let hex = std::str::from_utf8(digits).map_err(|e| e.to_string())?;
            decoded.push(u8::from_str_radix(hex, 16).map_err(|e| e.to_string())?);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    let path = String::from_utf8(decoded).map_err(|e| e.to_string())?;
    if path.contains(':') || Path::new(&path).is_absolute() {
        return Err("a model's resources must be local files inside its pack".into());
    }
    let path = file.parent().unwrap_or(root).join(path);
    let path = contained(root, &path)?;
    std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
}

fn contained(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let path = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if !path.starts_with(root) {
        return Err("a model's resources must stay inside its pack".into());
    }
    Ok(path)
}

pub(super) fn scene(root: &Path, path: &Path) -> Result<Scene, String> {
    let path = contained(root, path)?;
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let gltf = gltf::Gltf::from_slice(&bytes).map_err(|e| e.to_string())?;
    if gltf.extensions_required().next().is_some() {
        return Err("required glTF extensions are not supported".into());
    }
    if gltf.animations().next().is_some() || gltf.skins().next().is_some() {
        return Err("import a static scene without animations or skins".into());
    }
    let mut buffers = Vec::new();
    for buffer in gltf.buffers() {
        let bytes = match buffer.source() {
            gltf::buffer::Source::Bin => gltf.blob.clone().ok_or("the GLB has no binary buffer")?,
            gltf::buffer::Source::Uri(uri) => read_uri(root, &path, uri)?,
        };
        if bytes.len() < buffer.length() {
            return Err("a glTF buffer is shorter than its declared length".into());
        }
        buffers.push(bytes[..buffer.length()].to_vec());
    }
    for view in gltf.views() {
        let end = view
            .offset()
            .checked_add(view.length())
            .ok_or("buffer view overflow")?;
        if end > buffers[view.buffer().index()].len() {
            return Err("a buffer view reaches past its buffer".into());
        }
    }
    for accessor in gltf.accessors() {
        if accessor.sparse().is_some() {
            return Err("sparse accessors are not supported; export dense mesh data".into());
        }
        let view = accessor
            .view()
            .ok_or("a mesh accessor has no buffer view")?;
        let stride = view.stride().unwrap_or(accessor.size());
        let last = accessor
            .count()
            .checked_sub(1)
            .ok_or("an accessor is empty")?;
        let end = last
            .checked_mul(stride)
            .and_then(|n| n.checked_add(accessor.offset()))
            .and_then(|n| n.checked_add(accessor.size()))
            .ok_or("accessor size overflow")?;
        if stride < accessor.size() || end > view.length() {
            return Err("an accessor reaches past its buffer view".into());
        }
    }
    let mut materials = Vec::new();
    for m in gltf.materials() {
        let pbr = m.pbr_metallic_roughness();
        let image = if let Some(texture) = pbr.base_color_texture() {
            if texture.tex_coord() != 0 {
                return Err("base color must use the first texture coordinates".into());
            }
            Some(match texture.texture().source().source() {
                gltf::image::Source::Uri { uri, .. } => read_uri(root, &path, uri)?,
                gltf::image::Source::View { view, .. } => buffers[view.buffer().index()]
                    [view.offset()..view.offset() + view.length()]
                    .to_vec(),
            })
        } else {
            None
        };
        if m.alpha_mode() == gltf::material::AlphaMode::Mask {
            return Err(
                "alpha-cutoff materials are not supported; use opaque or blended surfaces".into(),
            );
        }
        materials.push(Material {
            color: {
                let mut color = pbr.base_color_factor();
                if m.alpha_mode() == gltf::material::AlphaMode::Opaque {
                    color[3] = 1.0;
                }
                color
            },
            image,
            double_sided: m.double_sided(),
            opaque: m.alpha_mode() == gltf::material::AlphaMode::Opaque,
        });
    }
    materials.push(Material {
        color: [1.0; 4],
        image: None,
        double_sided: false,
        opaque: true,
    });
    let selected = gltf
        .default_scene()
        .or_else(|| gltf.scenes().next())
        .ok_or("the model has no scene")?;
    let mut stack: Vec<_> = selected
        .nodes()
        .map(|n| (n, Mat4::IDENTITY, 0usize))
        .collect();
    let mut seen = BTreeSet::new();
    let mut parts = Vec::new();
    let upright = Mat4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]);
    while let Some((node, parent, depth)) = stack.pop() {
        if depth > 256 || !seen.insert(node.index()) {
            return Err("a scene node is repeated or the hierarchy is too deep".into());
        }
        let transform = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        if !transform.is_finite() || transform.determinant().abs() < 1e-12 {
            return Err("a node transform is non-finite or has zero scale".into());
        }
        stack.extend(node.children().map(|n| (n, transform, depth + 1)));
        let Some(mesh) = node.mesh() else { continue };
        let transform = upright * transform;
        let (_, rotation, translation) = transform.to_scale_rotation_translation();
        let rotation = rotation.normalize();
        if !rotation.is_finite() {
            return Err("a node rotation is invalid".into());
        }
        let frame = Frame::new(
            vector(translation),
            Quat::new(rotation.w, rotation.x, rotation.y, rotation.z),
        );
        let mut primitives = Vec::new();
        for primitive in mesh.primitives() {
            if primitive.mode() != gltf::mesh::Mode::Triangles
                || primitive.morph_targets().next().is_some()
            {
                return Err("import triangle primitives without morph targets".into());
            }
            let reader = primitive.reader(|b| Some(buffers[b.index()].as_slice()));
            let positions: Vec<GVec> = reader
                .read_positions()
                .ok_or("a mesh has no positions")?
                .map(GVec::from)
                .collect();
            if positions.is_empty() || positions.iter().any(|p| !p.is_finite()) {
                return Err("mesh positions must be finite and nonempty".into());
            }
            let indices: Vec<u32> = reader.read_indices().map_or_else(
                || (0..u32::try_from(positions.len()).unwrap_or(0)).collect(),
                |i| i.into_u32().collect(),
            );
            if indices.is_empty()
                || !indices.len().is_multiple_of(3)
                || indices
                    .iter()
                    .any(|i| usize::try_from(*i).map_or(true, |i| i >= positions.len()))
            {
                return Err("triangle indices are incomplete or outside the vertex array".into());
            }
            for t in indices.as_chunks::<3>().0 {
                let [a, b, c] = t.map(|v| v as usize);
                let area = (positions[b] - positions[a]).cross(positions[c] - positions[a]);
                if !area.is_finite() || area.length_squared() < 1e-16 {
                    return Err("a triangle has no finite area".into());
                }
            }
            let normals = if let Some(normals) = reader.read_normals() {
                normals.map(GVec::from).collect::<Vec<_>>()
            } else {
                let mut normals = vec![GVec::ZERO; positions.len()];
                for t in indices.as_chunks::<3>().0 {
                    let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
                    let n = (positions[b] - positions[a]).cross(positions[c] - positions[a]);
                    if n.length_squared() < 1e-16 {
                        return Err("a triangle has no area".into());
                    }
                    for v in [a, b, c] {
                        normals[v] += n;
                    }
                }
                normals.into_iter().map(GVec::normalize_or_zero).collect()
            };
            let material = primitive.material().index().unwrap_or(materials.len() - 1);
            let uvs = match reader.read_tex_coords(0) {
                Some(uv) => uv.into_f32().collect::<Vec<_>>(),
                None if materials[material].image.is_some() => {
                    return Err("a textured mesh has no texture coordinates".into())
                }
                None => vec![[0.0; 2]; positions.len()],
            };
            if normals.len() != positions.len()
                || uvs.len() != positions.len()
                || normals
                    .iter()
                    .any(|n| !n.is_finite() || n.length_squared() < 1e-12)
                || uvs.iter().flatten().any(|x| !x.is_finite())
            {
                return Err(
                    "mesh normals and texture coordinates must match its finite vertices".into(),
                );
            }
            primitives.push(Primitive {
                positions,
                normals,
                uvs,
                indices,
                material,
                double_sided: materials[material].double_sided,
            });
        }
        if primitives.is_empty() {
            return Err("a mesh contains no triangles".into());
        }
        parts.push(Part {
            name: node
                .name()
                .map_or_else(|| format!("node-{}", node.index()), str::to_owned),
            frame,
            transform,
            primitives,
        });
    }
    Ok(Scene { parts, materials })
}

impl Scene {
    pub fn materials(
        &self,
        at: &Loc,
        name: &str,
        out: &mut Vec<Located<NewRecord>>,
    ) -> Result<Vec<RecordRef>, Error> {
        let mut surfaces = Vec::new();
        for (i, material) in self.materials.iter().enumerate() {
            let sym = format!("{name}-material-{i}");
            if material
                .color
                .iter()
                .any(|c| !c.is_finite() || !(0.0..=1.0).contains(c))
            {
                return Err(Error::source(
                    at,
                    "base color components must be between zero and one",
                ));
            }
            let textured = material.image.is_some();
            if let Some(image) = &material.image {
                let (width, height, pixels) = png_pixels(image, material.color, material.opaque)
                    .map_err(|s| Error::source(at, s))?;
                add(
                    out,
                    at,
                    sym.clone(),
                    Record::Image {
                        width,
                        height,
                        pixels,
                    },
                );
                out.push(Located {
                    at: at.clone(),
                    item: NewRecord::SurfaceTexture {
                        target: Target::New(sym.clone()),
                        levels: vec![named(&sym)],
                        category: 0,
                    },
                });
            }
            let [r, g, b] = [0, 1, 2].map(|i| channel(to_srgb(material.color[i])));
            let a = channel(material.color[3]);
            out.push(Located {
                at: at.clone(),
                item: NewRecord::Surface {
                    target: Target::New(sym.clone()),
                    surface_type: if textured { 2 } else { 1 },
                    texture: textured.then(|| named(&sym)),
                    palette: None,
                    color: (!textured).then_some(u32::from_be_bytes([a, r, g, b])),
                    translucency: if textured {
                        0.0
                    } else {
                        1.0 - material.color[3]
                    },
                    luminosity: 0.0,
                    diffuse: 1.0,
                },
            });
            surfaces.push(named(&sym));
        }
        Ok(surfaces)
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn channel(x: f32) -> u8 {
    (x.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn to_srgb(x: f32) -> f32 {
    if x <= 0.003_130_8 {
        12.92 * x
    } else {
        1.055 * math::powf(x, 1.0 / 2.4) - 0.055
    }
}

fn from_srgb(x: f32) -> f32 {
    if x <= 0.04045 {
        x / 12.92
    } else {
        math::powf((x + 0.055) / 1.055, 2.4)
    }
}

fn png_pixels(bytes: &[u8], color: [f32; 4], opaque: bool) -> Result<(u32, u32, Vec<u8>), String> {
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("base color picture must be PNG: {e}"))?;
    let info = reader.info();
    if !info.width.is_power_of_two()
        || !info.height.is_power_of_two()
        || info.width > 2048
        || info.height > 2048
    {
        return Err("picture dimensions must be powers of two, up to 2048".into());
    }
    let mut bytes = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut bytes).map_err(|e| e.to_string())?;
    let count = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err("an indexed PNG was not expanded".into()),
    };
    let mut pixels = Vec::new();
    for px in bytes[..frame.buffer_size()].chunks_exact(count) {
        let rgba = match count {
            1 => [px[0], px[0], px[0], 255],
            2 => [px[0], px[0], px[0], px[1]],
            3 => [px[0], px[1], px[2], 255],
            _ => [px[0], px[1], px[2], px[3]],
        };
        let tint = |i: usize| {
            let x = f32::from(rgba[i]) / 255.0;
            channel(if i == 3 {
                x * color[i]
            } else {
                to_srgb(from_srgb(x) * color[i])
            })
        };
        pixels.extend_from_slice(&[
            tint(2),
            tint(1),
            tint(0),
            if opaque { 255 } else { tint(3) },
        ]);
    }
    Ok((frame.width, frame.height, pixels))
}
