//! What the remaster draws with, as plain data: meshes, materials, textures and the ground's
//! surface, each decoded once and kept while something in the scene uses it.
//!
//! The scene's changes ([`crate::scene`]) name what is drawn by retail's ids: a GfxObj, an
//! Environment's piece, a surface with what an object's description does to it, a block's terrain
//! words. [`AssetTables`] turns those into entries, asking an [`AssetSource`] for each entry it
//! does not hold yet, and counts who holds each, so an entry that nothing uses any longer goes.
//! The source is the one thing here that reads the dats; the scene implements it with the same
//! decode path Retail Look draws through, so the remaster's entries are Retail Look's pictures
//! and triangles, not a second reading of the files.
//!
//! An [`AssetSink`] takes the entries to the device: [`AssetTables::upload`] hands it every entry
//! made since the last upload, textures first, then materials, then meshes and the ground, and
//! every entry let go. Nothing here touches a device.
//!
//! **Keys.**
//! - A mesh per GfxObj ([`MeshKey::GfxObj`]); each degrade level is a GfxObj of its own, so a
//!   placement's levels are each a mesh. An interior cell's geometry is a mesh per Environment
//!   piece ([`MeshKey::CellPiece`]), shared by every cell built from that piece; its subsets name
//!   the cell's surfaces by their place in the cell's list ([`SurfaceRef::Slot`]). A sky object
//!   is a mesh of its own ([`MeshKey::Sky`]), its parts folded in.
//! - A material per surface and appearance ([`MaterialKey`]): the appearance is only what an
//!   object's description changes about that surface's pixels, so a description that cannot change
//!   a surface leaves it sharing one material with every other user. Whether a polygon group is
//!   two-sided or tiled is the subset's ([`Subset`]), not the material's.
//! - A texture per picture as retail caches it ([`TextureKey`]).
//! - The ground's surface per block ([`TerrainSurface`]): retail's per-vertex normals and each
//!   drawn cell's picture.
//!
//! **Kinds.** Every material has a [`Kind`], from the picture's data category and the surface's
//! flags through [`kind::of`], a table whose exceptions are written out.

pub mod kind;
pub mod pack;
mod tables;
pub mod terrain;

pub use kind::{Kind, Use};
pub use tables::{AssetCounts, AssetTables, UploadCounts};
pub use terrain::{TerrainCell, TerrainSurface};

pub use dereth_primitives::{TextureData as Texture, TextureFormat};

use crate::scene::{BlockId, PartLook, Terrain};

/// An Environment's piece: the Environment's id and the piece's index in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellPieceKey {
    pub environment: u32,
    pub structure: u16,
}

/// Which mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MeshKey {
    /// A GfxObj's drawing polygons.
    GfxObj(u32),
    /// An Environment piece's polygons, which interior cells are built from.
    CellPiece(CellPieceKey),
    /// A sky object, a GfxObj or a Setup, its parts' frames folded into its vertices.
    Sky(u32),
}

/// One vertex: position, normal and texture coordinates, in the mesh's own space.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// The surface a subset is drawn with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SurfaceRef {
    /// A surface by its id: a GfxObj names its own.
    Surface(u32),
    /// The surface at this place in the drawing cell's own list: an Environment piece's polygons
    /// name their surfaces so, and each cell built from it lists its own.
    Slot(u16),
    /// The polygon names a surface its object does not have; retail draws it untextured.
    Missing,
}

/// One surface's triangles within a mesh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Subset {
    /// Its first index in [`Mesh::indices`], and how many indices it has (three a triangle).
    pub first_index: u32,
    pub index_count: u32,
    pub surface: SurfaceRef,
    /// Drawn from both sides: no face is culled.
    pub two_sided: bool,
    /// The texture repeats (wrap addressing) rather than clamping at its edges.
    pub tiled: bool,
}

/// One mesh: its vertices, the triangles over them, and one subset per surface, in the order
/// retail draws them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub subsets: Vec<Subset>,
    /// The drawing sphere, center and radius, when the GfxObj stores one.
    pub sphere: Option<([f32; 3], f32)>,
    /// The point a translucent part is sorted by.
    pub sort_center: [f32; 3],
    /// Each vertex's tangent (the way its texture's first coordinate runs, on the surface) and,
    /// in `w`, which way its second runs from it: 1 or -1. Empty when the mesh brings none; they
    /// are then worked out from the texture coordinates ([`tangents`]).
    pub tangents: Vec<[f32; 4]>,
    /// Each vertex's color, in display values, which a surface flagged for it
    /// ([`Material::vertex_colored`]) is drawn under. Empty when the mesh brings none: white.
    pub colors: Vec<[f32; 4]>,
}

/// The tangents of `mesh`'s vertices: each triangle's direction of its texture's first
/// coordinate, summed at its vertices, made square to each vertex's normal; `w` is 1 where the
/// second coordinate runs as the normal crossed with the tangent, -1 where it runs the other way.
/// A vertex no triangle gives a direction takes one square to its normal.
#[must_use]
pub fn tangents(mesh: &Mesh) -> Vec<[f32; 4]> {
    use glam::Vec3;
    let n = mesh.vertices.len();
    let mut t = vec![Vec3::ZERO; n];
    let mut b = vec![Vec3::ZERO; n];
    for tri in mesh.indices.as_chunks::<3>().0 {
        let [i0, i1, i2] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        let (Some(v0), Some(v1), Some(v2)) = (
            mesh.vertices.get(i0),
            mesh.vertices.get(i1),
            mesh.vertices.get(i2),
        ) else {
            continue;
        };
        let e1 = Vec3::from(v1.position) - Vec3::from(v0.position);
        let e2 = Vec3::from(v2.position) - Vec3::from(v0.position);
        let (du1, dv1) = (v1.uv[0] - v0.uv[0], v1.uv[1] - v0.uv[1]);
        let (du2, dv2) = (v2.uv[0] - v0.uv[0], v2.uv[1] - v0.uv[1]);
        let det = du1 * dv2 - du2 * dv1;
        if det.abs() < 1e-12 {
            continue;
        }
        let r = 1.0 / det;
        let ft = (e1 * dv2 - e2 * dv1) * r;
        let fb = (e2 * du1 - e1 * du2) * r;
        for i in [i0, i1, i2] {
            t[i] += ft;
            b[i] += fb;
        }
    }
    mesh.vertices
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let nrm = Vec3::from(v.normal).normalize_or(Vec3::Z);
            let along = t[i] - nrm * nrm.dot(t[i]);
            let tan = if along.length_squared() > 1e-20 {
                along.normalize()
            } else {
                nrm.any_orthonormal_vector()
            };
            let w = if nrm.cross(tan).dot(b[i]) < 0.0 {
                -1.0
            } else {
                1.0
            };
            [tan.x, tan.y, tan.z, w]
        })
        .collect()
}

/// The maps a material brings beyond its picture: a normal map (tangent space, its red and green
/// the normal's first two components), and an occlusion, roughness and metalness map (its green
/// the roughness). Retail's surfaces have none.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialMaps {
    pub normal: Option<TextureKey>,
    pub orm: Option<TextureKey>,
    /// How strongly the normal map bends the normal: 1 as it was made.
    pub normal_scale: f32,
    /// The roughness the map's green is multiplied by, or where there is no map the roughness
    /// itself (glTF's roughness factor): 1 the roughest.
    pub roughness: f32,
}

/// How a subset is described while a mesh is built: its surface and its two polygon flags.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SubsetInfo {
    pub surface: SurfaceRef,
    pub two_sided: bool,
    pub tiled: bool,
}

impl Mesh {
    /// A mesh from each subset's triangle list (three vertices a triangle), in order. A vertex
    /// that is the same in every bit as one already in the mesh is shared rather than repeated,
    /// so the triangles and their order are exactly the lists'.
    #[must_use]
    pub fn from_triangle_lists(lists: impl IntoIterator<Item = (SubsetInfo, Vec<Vertex>)>) -> Self {
        let white = lists
            .into_iter()
            .map(|(info, list)| (info, list.into_iter().map(|v| (v, [1.0; 4])).collect()));
        let mut mesh = Self::from_colored_triangle_lists(white);
        mesh.colors = Vec::new();
        mesh
    }

    /// A mesh as [`Self::from_triangle_lists`] makes it, from vertices that each bring a color: a
    /// vertex is shared only with one of the same color.
    #[must_use]
    pub fn from_colored_triangle_lists(
        lists: impl IntoIterator<Item = (SubsetInfo, Vec<(Vertex, [f32; 4])>)>,
    ) -> Self {
        let mut mesh = Self::default();
        let mut seen: std::collections::BTreeMap<[u32; 12], u32> =
            std::collections::BTreeMap::new();
        for (info, list) in lists {
            let first = u32::try_from(mesh.indices.len()).unwrap_or(u32::MAX);
            for (v, c) in list {
                let bits = [
                    v.position[0].to_bits(),
                    v.position[1].to_bits(),
                    v.position[2].to_bits(),
                    v.normal[0].to_bits(),
                    v.normal[1].to_bits(),
                    v.normal[2].to_bits(),
                    v.uv[0].to_bits(),
                    v.uv[1].to_bits(),
                    c[0].to_bits(),
                    c[1].to_bits(),
                    c[2].to_bits(),
                    c[3].to_bits(),
                ];
                let next = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
                let at = *seen.entry(bits).or_insert(next);
                if at == next {
                    mesh.vertices.push(v);
                    mesh.colors.push(c);
                }
                mesh.indices.push(at);
            }
            let count = u32::try_from(mesh.indices.len()).unwrap_or(u32::MAX) - first;
            mesh.subsets.push(Subset {
                first_index: first,
                index_count: count,
                surface: info.surface,
                two_sided: info.two_sided,
                tiled: info.tiled,
            });
        }
        mesh
    }

    /// Its triangles.
    #[must_use]
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }
}

/// A shift palette as the recipe that builds it: a copy of `base` with each range laid over it
/// from a palette set, `(palette set, first color, count)` in the description's units.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PaletteRecipe {
    pub base: u32,
    pub ranges: Vec<(u32, u32, u32)>,
}

/// What an object's description changes about one surface's pixels: a texture put in place of
/// the surface's own, and a shift palette. Both are left out when they cannot change a pixel of
/// that surface, so the default is the surface as the dat has it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Appearance {
    /// The texture replacing the surface's own.
    pub texture: Option<u32>,
    pub palette: Option<PaletteRecipe>,
}

/// Which material: a surface (0 for a polygon whose surface is missing) and its appearance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MaterialKey {
    pub surface: u32,
    pub appearance: Appearance,
}

impl MaterialKey {
    /// A surface as the dat has it.
    #[must_use]
    pub fn plain(surface: u32) -> Self {
        Self {
            surface,
            appearance: Appearance::default(),
        }
    }
}

/// A blend factor, as retail's surface setup chooses them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlendFactor {
    Zero,
    One,
    SrcAlpha,
    InvSrcAlpha,
}

/// How one surface is drawn and what it is.
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub kind: Kind,
    /// The surface record's own type word.
    pub surface_type: u32,
    /// The data category of the picture the surface draws, when it draws one from the dat.
    pub data_category: Option<u32>,
    /// The picture: the surface's texture, or for an untextured surface the one-texel picture of
    /// its colour ([`TextureKey::Solid`]). `None` when there is neither.
    pub texture: Option<TextureKey>,
    /// The texture's indices up to 7 expand to transparent black: retail's clip map, applied when
    /// a palettised picture is expanded.
    pub clip_map: bool,
    /// The alpha test, with its reference, when the surface is a clip map: a pixel is kept when
    /// its alpha is at least this.
    pub alpha_test: Option<u8>,
    /// The alpha written with every vertex: `(1 − translucency) × 255` on a translucent surface,
    /// 255 otherwise.
    pub vertex_alpha: u8,
    /// Drawn under its vertices' colors ([`Mesh::colors`]), as an art pack's model is; a retail
    /// surface is not.
    pub vertex_colored: bool,
    /// Blended translucently, with this alpha.
    pub translucent: Option<u8>,
    /// Added onto what is behind it, and never fogged.
    pub additive: bool,
    /// The blend retail sets: whether it blends, and its two factors.
    pub blended: bool,
    pub src_blend: BlendFactor,
    pub dst_blend: BlendFactor,
    /// Whether it writes depth.
    pub z_write: bool,
    /// Whether fog is applied to it (it is, unless it is additive).
    pub fogged: bool,
    /// Its color when it is untextured, as `0xAARRGGBB`.
    pub solid: Option<u32>,
    /// Light it gives off itself, added before the light is clamped: 1 is full bright.
    pub luminosity: f32,
    /// How much of the sun's light it takes.
    pub diffuse: f32,
    /// Its normal and roughness maps; `None` for retail's surfaces.
    pub maps: Option<MaterialMaps>,
}

/// Which picture, keyed as retail caches pictures, so two surfaces that draw the same picture
/// share one texture.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TextureKey {
    /// A picture of the dat under its own palette (0 for none): the picture's id and the
    /// palette's.
    Dat { picture: u32, palette: u32 },
    /// A palettised picture under a shift palette built from a recipe.
    Shifted {
        picture: u32,
        palette: PaletteRecipe,
        clip_map: bool,
    },
    /// The one-texel picture of an untextured surface's color, `0xAARRGGBB`.
    Solid(u32),
    /// A ground cell's picture, composited from its corners' terrain: the cell's merge key.
    Terrain(u32),
    /// One of the pictures the ground is composited from (a terrain's tile or an alpha map), as
    /// the composite carries it: `texels` a side at most, picked as the composite picks them.
    TerrainSource { picture: u32, texels: u32 },
}

/// What the tables ask for what they do not hold. The scene implements it over the dats.
pub trait AssetSource {
    /// A GfxObj's drawing mesh; `None` when the dat has no drawing polygons for it.
    fn gfxobj(&mut self, id: u32) -> Option<Mesh>;
    /// An Environment piece's mesh, its subsets naming surfaces by slot; `None` when the dat
    /// lacks it.
    fn cell_piece(&mut self, key: CellPieceKey) -> Option<Mesh>;
    /// What `look` changes about `surface`'s pixels; the default when it changes nothing.
    fn appearance(&mut self, surface: u32, look: Option<&PartLook>) -> Appearance;
    /// The material a key names, first used as `used`. Every key has one: a surface the dat
    /// lacks is drawn untextured.
    fn material(&mut self, key: &MaterialKey, used: kind::Use) -> Material;
    /// A texture's pixels, every level; `clip_map` is the first material's that names it.
    /// `None` when the picture will not decode.
    fn texture(&mut self, key: &TextureKey, clip_map: bool) -> Option<Texture>;
    /// The pictures of a block's drawn ground cells, in the terrain's cell order.
    fn terrain_cells(&mut self, block: BlockId, terrain: &Terrain) -> Vec<TerrainCell>;
    /// The layers of the ground cells' picture `key` ([`terrain::splat_key`]); `None` when the
    /// region has no ground pictures.
    fn splat(&mut self, key: u32) -> Option<terrain::Splat> {
        let _ = key;
        None
    }
    /// Each terrain type's color as the ground is seen far off: its picture's mean, in display
    /// values; `None` when the region has no ground pictures.
    fn ground_colors(&mut self) -> Option<[[f32; 4]; 32]> {
        None
    }
    /// Every sky object the region's days can show, GfxObjs and Setups, in id order.
    fn sky_objects(&mut self) -> Vec<u32>;
    /// A sky object's mesh, every part in it at its place; `None` when it draws nothing.
    fn sky(&mut self, id: u32) -> Option<Mesh>;
    /// The art pack's model for the object `id` (a Setup or a GfxObj), when the source has a
    /// pack, the pack has one, and its kind is switched on ([`pack`]). None by default.
    fn pack_model(&mut self, id: u32) -> Option<pack::PackModel> {
        let _ = id;
        None
    }
}

/// One entry, as [`AssetSink::release`] names it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AssetKey {
    Mesh(MeshKey),
    Material(MaterialKey),
    Texture(TextureKey),
    Terrain(BlockId),
    /// A ground cells' splat, by its key.
    Splat(u32),
}

/// What takes the entries to a device.
pub trait AssetSink {
    fn texture(&mut self, key: &TextureKey, texture: &Texture);
    fn material(&mut self, key: &MaterialKey, material: &Material);
    fn mesh(&mut self, key: MeshKey, mesh: &Mesh);
    fn terrain(&mut self, block: BlockId, surface: &TerrainSurface);
    /// A ground cells' splat, after the textures it names.
    fn splat(&mut self, key: u32, splat: &terrain::Splat);
    /// An entry nothing uses any more.
    fn release(&mut self, key: &AssetKey);
}

#[cfg(test)]
mod tests;
