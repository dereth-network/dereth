//! Meshes (`.mesh`, `0x06000000`+) and cell meshes (`.cellmesh`, `0x01000000`+): vertices, the
//! triangles drawn at each level of detail, the polygons, and the collision trees.
//!
//! A mesh is, in order: its kind, its materials, its vertices, its bounding sphere, its levels of
//! detail, a list of polygons with (when the list is not empty) a tree over them, then a list of
//! physics polygons and a word saying whether a physics tree over those follows. A cell mesh is a
//! mesh followed by the cell's own tree over its physics polygons, its portals and an info id.

use auberean_dat::reader::Result;
use auberean_dat::{Reader, Writer};
use auberean_primitives::{BBox, DataId, Sphere, Vec3};

use crate::bsp::{Tree, TreeKind};
use crate::{Context, Decode};

/// The size in bytes of one vertex of a vertex format word.
///
/// In storage order: a position (12 bytes) when the `0xE` field is not 0; blend weights (4 bytes
/// each: the `0xE` field's values 6, 8, 10, 12, 14 give 1 to 5); a normal (`0x10`, 12); a point
/// size (`0x20`, 4); diffuse and specular colors (`0x40`, `0x80`, 4 each); three extra vectors
/// (`0x1000_0000`, `0x2000_0000`, `0x4000_0000`, 12 each); 1 to 8 pairs of texture coordinates (the
/// `0xF00` field, 8 bytes each); then, for each of the `0xFF_0000` field's matrices, a palette byte
/// and a 4-byte weight. With `0x0100_0000` set, the size is padded up to the next multiple of 32
/// above it (32 to 256), and a size of 256 or more stays as it is.
// D2-13121
#[must_use]
pub fn vertex_size(format: u32) -> usize {
    let weights_field = format & 0xE;
    let mut n = if weights_field != 0 { 12 } else { 0 };
    n += 4 * match weights_field {
        6 => 1,
        8 => 2,
        10 => 3,
        12 => 4,
        14 => 5,
        _ => 0,
    };
    for (bit, size) in [
        (0x10, 12),
        (0x20, 4),
        (0x40, 4),
        (0x80, 4),
        (0x1000_0000, 12),
        (0x2000_0000, 12),
        (0x4000_0000, 12),
    ] {
        if format & bit != 0 {
            n += size;
        }
    }
    let pairs = match format & 0xF00 {
        p @ 0x100..=0x800 if p % 0x100 == 0 => (p >> 8) as usize,
        _ => 0,
    };
    n += 8 * pairs;
    let matrices = ((format >> 16) & 0xFF) as usize;
    n += matrices * 5;
    if format & 0x0100_0000 != 0 && n < 0x100 {
        n = (n / 0x20 + 1) * 0x20;
    }
    n
}

/// A mesh's vertices, packed in its vertex format, and their bounds.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct VertexArray {
    pub format: u32,
    pub count: u32,
    pub bytes: Vec<u8>,
    pub bounds: BBox,
}

impl VertexArray {
    /// The format word, the vertex count, the vertices, alignment, then the bounds, as read.
    // D2-04200, D2-04201
    pub fn write(&self, w: &mut Writer) {
        w.u32(self.format);
        w.u32(self.count);
        w.bytes(&self.bytes);
        w.align();
        w.vec3(self.bounds.min);
        w.vec3(self.bounds.max);
    }

    /// The box about the first `count` vertices, each `stride` bytes and starting with its place.
    // D2-04199
    #[must_use]
    pub fn built_bounds(&self, stride: usize) -> BBox {
        let mut b = BBox {
            min: Vec3::new(f32::MAX, f32::MAX, f32::MAX),
            max: Vec3::new(f32::MIN, f32::MIN, f32::MIN),
        };
        if stride < 12 {
            return b;
        }
        for v in self.bytes.chunks_exact(stride).take(self.count as usize) {
            let at = |k: usize| f32::from_le_bytes([v[k], v[k + 1], v[k + 2], v[k + 3]]);
            let (x, y, z) = (at(0), at(4), at(8));
            b.min = Vec3::new(b.min.x.min(x), b.min.y.min(y), b.min.z.min(z));
            b.max = Vec3::new(b.max.x.max(x), b.max.y.max(y), b.max.z.max(z));
        }
        b
    }

    /// The format word, the vertex count, the vertices, alignment, then the bounding box.
    // D2-04207, D2-05480
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let format = r.u32()?;
        let count = r.u32()?;
        let n = (count as usize).saturating_mul(vertex_size(format));
        let bytes = r.bytes(n)?.to_vec();
        r.align()?;
        let bounds = r.bbox()?;
        Ok(Self {
            format,
            count,
            bytes,
            bounds,
        })
    }
}

/// The triangles of one material at one level of detail.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Fragment {
    /// Which of the mesh's materials draws it.
    pub material: u32,
    /// Triangle-list vertex indices, three a triangle.
    pub indices: std::sync::Arc<Vec<u16>>,
}

/// One level of detail: when it is used, and its fragments.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DetailLevel {
    pub degrade: u32,
    pub detail: u32,
    pub fragments: Vec<Fragment>,
}

impl DetailLevel {
    /// The degrade mode, the detail levels word, then a counted list of fragments: a material
    /// index and a counted list of 16-bit indices, then alignment.
    // D2-07976, D2-05481
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let degrade = r.u32()?;
        let detail = r.u32()?;
        let fragments = r.list(8, |r| {
            let material = r.u32()?;
            let n = r.count(2)?;
            let mut indices = Vec::with_capacity(n);
            for _ in 0..n {
                indices.push(r.u16()?);
            }
            r.align()?;
            Ok(Fragment {
                material,
                indices: std::sync::Arc::new(indices),
            })
        })?;
        Ok(Self {
            degrade,
            detail,
            fragments,
        })
    }
}

/// A polygon of the mesh: its id and its vertices (indices into the vertex array).
// D2-00042, D2-04163, D2-04167, D2-04168, D2-04174
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MeshPolygon {
    pub id: u16,
    pub vertices: Vec<u16>,
}

impl MeshPolygon {
    /// Which vertex of the array its corner `k` is; none past its corners.
    // D2-04164
    #[must_use]
    pub fn corner(&self, k: usize) -> Option<u16> {
        self.vertices.get(k).copied()
    }

    /// The id, a 16-bit vertex count, the 16-bit vertex indices, alignment.
    // D2-04176
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let id = r.u16()?;
        let n = r.count16(2)?;
        let mut vertices = Vec::with_capacity(n);
        for _ in 0..n {
            vertices.push(r.u16()?);
        }
        r.align()?;
        Ok(Self { id, vertices })
    }

    // D2-05506
    fn read_list(r: &mut Reader<'_>) -> Result<Vec<Self>> {
        r.list(4, Self::read)
    }
}

/// How many bone matrices a vertex format weighs each vertex to: its third byte.
#[must_use]
pub const fn matrix_count(format: u32) -> u32 {
    (format >> 16) & 0xFF
}

/// A drawable mesh. Made whole when it is read (its geometry, its materials, its vertices and its
/// levels of detail), and let go of whole when dropped.
// D2-05502, D2-05503, D2-05509, D2-05488, D2-05526
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Mesh {
    pub kind: u32,
    /// Its materials (material instances, `0x31...`).
    pub materials: Vec<DataId>,
    pub vertices: VertexArray,
    pub sphere: Sphere,
    pub levels: Vec<DetailLevel>,
    /// Polygons over the drawn vertices, and the tree over them, present when there are any.
    pub polygons: Vec<MeshPolygon>,
    pub polygon_tree: Option<Tree>,
    /// The polygons collision uses, and the tree over them when the mesh has one.
    pub physics_polygons: Vec<MeshPolygon>,
    pub physics_tree: Option<Tree>,
}

impl Mesh {
    /// Whether its vertices are weighted to bones: their format names matrices.
    // D2-05477
    #[must_use]
    pub const fn has_matrix_weights(&self) -> bool {
        matrix_count(self.vertices.format) != 0
    }

    /// The box about it: its vertices' box, as stored (the default box is the same, kept from
    /// when the vertices were read).
    // D2-05478, D2-05479
    #[must_use]
    pub const fn bounding_box(&self) -> BBox {
        self.vertices.bounds
    }

    /// Everything after the id, shared with cell meshes: the mesh's geometry and its materials
    /// made, as many materials as stored, its vertices, its levels of detail.
    // D2-05539, D2-05534, D2-05531, D2-05530, D2-05533, D2-05489, D2-05532, D2-05511
    fn read_body(r: &mut Reader<'_>) -> Result<Self> {
        let kind = r.u32()?;
        let materials = r.list(4, Reader::data_id)?;
        let vertices = VertexArray::read(r)?;
        let sphere = r.sphere()?;
        let levels = r.list(12, DetailLevel::read)?;
        let polygons = MeshPolygon::read_list(r)?;
        let polygon_tree = if polygons.is_empty() {
            None
        } else {
            Some(Tree::read(r, TreeKind::Physics)?)
        };
        let physics_polygons = MeshPolygon::read_list(r)?;
        let physics_tree = if r.bool32()? {
            Some(Tree::read(r, TreeKind::Physics)?)
        } else {
            None
        };
        Ok(Self {
            kind,
            materials,
            vertices,
            sphere,
            levels,
            polygons,
            polygon_tree,
            physics_polygons,
            physics_tree,
        })
    }
}

impl Decode for Mesh {
    /// The id, then the mesh.
    // D2-05556
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        Self::read_body(r)
    }
}

/// A doorway of a cell's geometry: which side of it is the cell's, the polygon it is, and its
/// bounds.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Portal {
    pub side: u16,
    /// Index into the mesh's physics polygons.
    pub polygon: u16,
    pub sphere: Sphere,
}

impl Portal {
    /// A 16-bit side, a 16-bit polygon index, the bounding sphere.
    // D2-04235
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            side: r.u16()?,
            polygon: r.u16()?,
            sphere: r.sphere()?,
        })
    }
}

/// An interior cell's geometry: a mesh, the tree that says whether a point is inside the cell, the
/// cell's doorways, and an info record id.
#[derive(Clone, Debug, PartialEq)]
pub struct CellMesh {
    pub mesh: Mesh,
    pub cell_tree: Tree,
    pub portals: Vec<Portal>,
    pub info: DataId,
}

impl Decode for CellMesh {
    /// The id, the mesh, the cell's tree, a counted list of portals, then the info id.
    // D2-05577
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let mesh = Mesh::read_body(r)?;
        let cell_tree = Tree::read(r, TreeKind::Cell)?;
        let portals = r.list(20, Portal::read)?;
        let info = r.data_id()?;
        Ok(Self {
            mesh,
            cell_tree,
            portals,
            info,
        })
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the vertex size rule)
    use super::vertex_size;

    #[test]
    fn a_position_counts_whenever_the_weight_field_is_set() {
        assert_eq!(vertex_size(0x2), 12);
        assert_eq!(vertex_size(0x4), 12);
        assert_eq!(vertex_size(0x6), 16);
        assert_eq!(vertex_size(0xE), 32);
    }

    #[test]
    fn the_stored_formats_have_their_sizes() {
        // Position, normal, both colors, one or two texture pairs, the two extra vectors.
        assert_eq!(vertex_size(0xD2), 32);
        assert_eq!(vertex_size(0x1D2), 40);
        assert_eq!(vertex_size(0x3000_02D2), 72);
    }

    #[test]
    fn up_to_eight_texture_pairs_count() {
        assert_eq!(vertex_size(0x802), 12 + 64);
        assert_eq!(vertex_size(0x902), 12);
    }

    #[test]
    fn matrices_take_five_bytes_each() {
        assert_eq!(vertex_size(0x0003_0002), 12 + 15);
    }

    #[test]
    fn the_padding_flag_rounds_up_past_the_next_multiple_of_32() {
        assert_eq!(vertex_size(0x0100_0002), 32);
        assert_eq!(vertex_size(0x0100_00D2), 64);
        assert_eq!(vertex_size(0x0100_0802 | 0x7000_0000), 128);
    }
}

#[cfg(test)]
mod vertex_array_tests {
    //! Behaviour: none (a vertex array's forms, held to hand-worked values)
    use super::*;

    #[test]
    fn a_vertex_array_writes_itself_and_finds_its_box() {
        let mut bytes = Vec::new();
        for v in [1.0f32, 2.0, 3.0, -1.0, 5.0, 0.5] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        let a = VertexArray {
            format: 2,
            count: 2,
            bytes,
            bounds: BBox::default(),
        };
        let b = a.built_bounds(12);
        assert_eq!(b.min, Vec3::new(-1.0, 2.0, 0.5));
        assert_eq!(b.max, Vec3::new(1.0, 5.0, 3.0));
        let mut w = Writer::new();
        a.write(&mut w);
        assert_eq!(w.finish().len(), 8 + 24 + 24);
    }
}
