//! A static glTF scene read for a world builder: its parts, each with its frame and triangles, and
//! its materials, every resource kept inside the folder it came from.
//!
//! **Depends on** `dereth-primitives` (frames and vectors), `gltf`, `glam`, `base64` and `png`.
//! **Used by** the world builders' model import: Lyceum's (`lyceum-foundry`) makes its models,
//! surfaces and collision from a scene.
//!
//! **Must never** make a game's records: it reads a scene and checks what a static model can be
//! made of, and what the parts and materials become is the builder's. And it reads files only
//! through [`Resources`], never outside the folder a builder names.
//!
//! Source dimensions are meters. The scene is turned into the client's axes, `(x, y, z)` to
//! `(x, -z, y)`, as the client turns its own. A part is a node with a mesh: its frame is the
//! node's translation and rotation in those axes, and its transform keeps the node's scale, which
//! a builder bakes into the geometry. What is refused, each with a sentence: animations and skins,
//! required extensions, sparse accessors, alpha-cutoff materials, primitives that are not
//! triangles or have morph targets, empty or degenerate triangles, and non-finite numbers.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use dereth_primitives::num::math;
use dereth_primitives::{Frame, Quat, Vec3};
use glam::{Mat4, Vec3 as GVec};

/// Where a scene's files are read: the file itself and the buffers and pictures it names.
pub trait Resources {
    /// The file at `path`.
    ///
    /// # Errors
    /// The storage's own errors.
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;

    /// `path` made absolute, every link and `..` resolved: how a file is shown to be inside the
    /// folder the builder names.
    ///
    /// # Errors
    /// A path that names nothing, and the storage's own errors.
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
}

/// Files on disk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiskResources;

impl Resources for DiskResources {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        path.canonicalize()
    }
}

/// A scene: its parts, and its materials with one more at the end, plain white, for a primitive
/// that names none.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub parts: Vec<Part>,
    pub materials: Vec<Material>,
}

/// A material: its base color factor (linear, alpha 1 when opaque), its base color picture's
/// file bytes, and whether it is drawn on both sides and opaque.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub color: [f32; 4],
    pub image: Option<Vec<u8>>,
    pub double_sided: bool,
    pub opaque: bool,
}

/// A node with a mesh: its name (`node-<n>` when it has none), its frame in the client's axes,
/// its whole transform from the scene (scale included), and its triangles.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub name: String,
    pub frame: Frame,
    pub transform: Mat4,
    pub primitives: Vec<Primitive>,
}

/// Triangles of one material: positions, normals and texture coordinates per vertex, three
/// indices a triangle.
#[derive(Clone, Debug, PartialEq)]
pub struct Primitive {
    pub positions: Vec<GVec>,
    pub normals: Vec<GVec>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub material: usize,
    pub double_sided: bool,
}

/// A vector in the client's own type.
#[must_use]
pub fn vector(v: GVec) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

/// A frame as a transform.
#[must_use]
pub fn matrix(frame: Frame) -> Mat4 {
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
    /// Every vertex of the part where the scene puts it.
    pub fn world_vertices(&self) -> impl Iterator<Item = Vec3> + '_ {
        self.primitives
            .iter()
            .flat_map(|p| p.positions.iter())
            .map(|p| vector(self.transform.transform_point3(*p)))
    }
}

fn read_uri(files: &dyn Resources, root: &Path, file: &Path, uri: &str) -> Result<Vec<u8>, String> {
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
    let path = contained(files, root, &path)?;
    files
        .read(&path)
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn contained(files: &dyn Resources, root: &Path, path: &Path) -> Result<PathBuf, String> {
    let root = files.canonicalize(root).map_err(|e| e.to_string())?;
    let path = files
        .canonicalize(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if !path.starts_with(root) {
        return Err("a model's resources must stay inside its pack".into());
    }
    Ok(path)
}

/// The scene of the glTF file at `path` (a `.gltf` with its buffers and pictures beside it, or a
/// `.glb`), read from disk; every file it names must be inside `root`.
///
/// # Errors
/// A file outside `root` or that will not read, and anything the scene holds that a static
/// model cannot be made of, each as a sentence.
pub fn scene(root: &Path, path: &Path) -> Result<Scene, String> {
    scene_in(&DiskResources, root, path)
}

/// [`scene`], its files read from `files`.
///
/// # Errors
/// As [`scene`].
pub fn scene_in(files: &dyn Resources, root: &Path, path: &Path) -> Result<Scene, String> {
    let path = contained(files, root, path)?;
    let bytes = files.read(&path).map_err(|e| e.to_string())?;
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
            gltf::buffer::Source::Uri(uri) => read_uri(files, root, &path, uri)?,
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
                gltf::image::Source::Uri { uri, .. } => read_uri(files, root, &path, uri)?,
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

/// A color channel from 0 to 1 as a byte.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn channel(x: f32) -> u8 {
    (x.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A linear color channel in sRGB.
#[must_use]
pub fn to_srgb(x: f32) -> f32 {
    if x <= 0.003_130_8 {
        12.92 * x
    } else {
        1.055 * math::powf(x, 1.0 / 2.4) - 0.055
    }
}

/// An sRGB color channel made linear.
#[must_use]
pub fn from_srgb(x: f32) -> f32 {
    if x <= 0.04045 {
        x / 12.92
    } else {
        math::powf((x + 0.055) / 1.055, 2.4)
    }
}

/// A material's base color picture (a PNG, its sides powers of two up to 2,048) tinted by its
/// base color factor `color`: its width, height and pixels, four bytes each in the order blue,
/// green, red, alpha; alpha 255 throughout when the material is `opaque`.
///
/// # Errors
/// A picture that is no PNG or of the wrong size, as a sentence.
pub fn base_color_pixels(
    bytes: &[u8],
    color: [f32; 4],
    opaque: bool,
) -> Result<(u32, u32, Vec<u8>), String> {
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
