//! Geometry primitives shared by `GfxObj`, `Environment` and the BSP trees.
//!
//! Record layouts are described in `docs/formats/10-gfxobj.md` and
//! `docs/formats/16-environment.md`. The reference vertex, polygon and BSP readers decode every
//! shipped object with zero trailing bytes.

use dereth_dat::{packobj::read_n, Cursor, DatError};
use dereth_primitives::Vec3;

use crate::error::AssetError;

/// A centre and a radius, serialized as four floats. The shape is `dereth_primitives`', shared with the
/// animation and physics crates.
pub use dereth_primitives::shape::Sphere;

/// Decode one [`Sphere`]: the centre, then the radius.
pub fn decode_sphere(c: &mut Cursor<'_>) -> Result<Sphere, DatError> {
    Ok(Sphere {
        center: c.vec3()?,
        radius: c.f32()?,
    })
}

/// A plane: the normal and `d`, serialized as four floats. The shape is `dereth_primitives`', shared
/// with physics and world rendering.
pub use dereth_primitives::shape::Plane;

/// Decode one [`Plane`]: the normal, then `d`.
pub fn decode_plane(c: &mut Cursor<'_>) -> Result<Plane, DatError> {
    Ok(Plane {
        normal: c.vec3()?,
        d: c.f32()?,
    })
}

/// One software vertex. The uv count is per vertex, so the record is variable length.
#[derive(Debug, Clone, PartialEq)]
pub struct SwVertex {
    pub id: i16,
    pub position: Vec3,
    pub normal: Vec3,
    pub uvs: Vec<(f32, f32)>,
}

/// A decoded vertex array: the vertex type and the vertices themselves.
#[derive(Debug, Clone, PartialEq)]
pub struct VertexArray {
    /// Only `CSWVertexType` (1) is supported by the client.
    pub vertex_type: u32,
    pub vertices: Vec<SwVertex>,
}

impl VertexArray {
    pub fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let vertex_type = c.u32()?;
        if vertex_type != 1 {
            return Err(AssetError::Unsupported {
                what: "vertex-array type",
                value: vertex_type,
            });
        }
        let n = c.u32()? as usize;
        let vertices = read_n(c, n, |c| {
            let id = c.i16()?;
            let nuv = c.u16()? as usize;
            let position = c.vec3()?;
            let normal = c.vec3()?;
            let uvs = read_n(c, nuv, |c| Ok((c.f32()?, c.f32()?)))?;
            Ok(SwVertex {
                id,
                position,
                normal,
                uvs,
            })
        })?;
        Ok(Self {
            vertex_type,
            vertices,
        })
    }
}

/// A decoded polygon: its vertex indices, its two surfaces and its UV index sets.
///
/// Open question #90: `sides_type == 2` occurs on 11 polygons in the whole portal dat; both
/// surfaces and both UV index sets are decoded and handed on.
#[derive(Debug, Clone, PartialEq)]
pub struct Polygon {
    pub poly_id: i16,
    pub num_pts: u8,
    pub stippling: u8,
    pub sides_type: i32,
    pub pos_surface: u16,
    pub neg_surface: u16,
    pub vertex_ids: Vec<u16>,
    pub pos_uv_indices: Option<Vec<u8>>,
    pub neg_uv_indices: Option<Vec<u8>>,
}

impl Polygon {
    pub fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let poly_id = c.i16()?;
        let num_pts = c.u8()?;
        let stippling = c.u8()?;
        let sides_type = c.i32()?;
        let pos_surface = c.u16()?;
        let mut neg_surface = c.u16()?;
        let n = num_pts as usize;
        let vertex_ids = read_n(c, n, Cursor::u16)?;
        let mut pos_uv_indices = None;
        let mut neg_uv_indices = None;
        // STIPPLE_NO_POS_UVS = 4, STIPPLE_NO_NEG_UVS = 8.
        if stippling & 4 == 0 {
            pos_uv_indices = Some(read_n(c, n, Cursor::u8)?);
        }
        if sides_type == 2 && stippling & 8 == 0 {
            neg_uv_indices = Some(read_n(c, n, Cursor::u8)?);
        }
        if sides_type == 1 {
            neg_surface = pos_surface;
            neg_uv_indices.clone_from(&pos_uv_indices);
        }
        Ok(Self {
            poly_id,
            num_pts,
            stippling,
            sides_type,
            pos_surface,
            neg_surface,
            vertex_ids,
            pos_uv_indices,
            neg_uv_indices,
        })
    }
}

/// Which of the three BSP flavours is being read. The tree type decides what trailing data each
/// node carries — the tags themselves do not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BspKind {
    Drawing,
    Physics,
    Cell,
}

/// One node of a BSP tree, in an arena.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BspNode {
    /// The four-character tag, as it sits in the file (little-endian dword).
    pub tag: u32,
    pub plane: Option<Plane>,
    pub pos_child: Option<u32>,
    pub neg_child: Option<u32>,
    pub sphere: Option<Sphere>,
    pub leaf_index: Option<u32>,
    /// Whether this leaf is solid; present only in physics trees.
    pub solid: Option<i32>,
    pub in_polys: Vec<i16>,
    /// `(polygon id, portal index)` pairs; drawing portals only.
    pub in_portals: Vec<(i16, i16)>,
}

impl BspNode {
    /// The tag as four ASCII characters, for diagnostics.
    #[must_use]
    pub fn tag_str(&self) -> String {
        let b = self.tag.to_le_bytes();
        [b[3], b[2], b[1], b[0]]
            .iter()
            .map(|&c| char::from(c))
            .collect()
    }
}

/// Unpack a BSP tree as an arena so that a deep tree cannot exhaust the stack. Node 0 is the root.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BspTree {
    pub nodes: Vec<BspNode>,
}

const TAG_PORT: u32 = 0x504F_5254; // 'PORT'
const TAG_LEAF: u32 = 0x4C45_4146; // 'LEAF'
const TAG_BPNN: u32 = 0x4250_6E6E; // 'BPnn' - positive child only
const TAG_BPIN_POS: u32 = 0x4250_496E; // 'BPIn' - positive child only
const TAG_BPIN_NEG: u32 = 0x4270_494E; // 'BpIN' - negative child only
const TAG_BPNN_NEG: u32 = 0x4270_6E4E; // 'BpnN' - negative child only
const TAG_BPIN_BOTH: u32 = 0x4250_494E; // 'BPIN' - both children
const TAG_BPNN_BOTH: u32 = 0x4250_6E4E; // 'BPnN' - both children

impl BspTree {
    /// Decodes a BSP tree: the interior nodes and their children, the leaves and the portal nodes.
    ///
    /// Open question #89: the tags `BPOL` (108,920 drawing nodes) and `BPFL` (3,168 cell nodes)
    /// fall through the child-unpacking switch and are read as **childless**. That fall-through is
    /// reproduced exactly; it is not an error, and the client reads these files successfully.
    ///
    /// Parsed with an explicit work list rather than recursion: deep DAT trees can exhaust
    /// a small stack.
    pub fn decode(c: &mut Cursor<'_>, kind: BspKind) -> Result<Self, AssetError> {
        /// Where a freshly parsed node records itself.
        enum Link {
            Root,
            Pos(usize),
            Neg(usize),
        }
        enum Task {
            Read(Link),
            /// Trailing fields that come *after* a node's children.
            Finish(usize),
        }

        let mut nodes: Vec<BspNode> = Vec::new();
        let mut stack = vec![Task::Read(Link::Root)];

        while let Some(task) = stack.pop() {
            match task {
                Task::Finish(i) => {
                    let is_port = nodes[i].tag == TAG_PORT;
                    match (kind, is_port) {
                        (BspKind::Drawing, true) => {
                            nodes[i].sphere = Some(decode_sphere(c)?);
                            let npoly = c.u32()? as usize;
                            let nport = c.u32()? as usize;
                            nodes[i].in_polys = read_n(c, npoly, Cursor::i16)?;
                            nodes[i].in_portals = read_n(c, nport, |c| Ok((c.i16()?, c.i16()?)))?;
                        }
                        (BspKind::Drawing, false) => {
                            nodes[i].sphere = Some(decode_sphere(c)?);
                            let npoly = c.u32()? as usize;
                            nodes[i].in_polys = read_n(c, npoly, Cursor::i16)?;
                        }
                        (BspKind::Physics, false) => {
                            nodes[i].sphere = Some(decode_sphere(c)?);
                        }
                        // A PORT in a physics or cell tree, and any node in a cell tree, carries
                        // nothing after its children.
                        _ => {}
                    }
                }
                Task::Read(link) => {
                    let idx = nodes.len();
                    nodes.push(BspNode::default());
                    match link {
                        Link::Root => {}
                        Link::Pos(p) => nodes[p].pos_child = Some(u32::try_from(idx).unwrap_or(0)),
                        Link::Neg(p) => nodes[p].neg_child = Some(u32::try_from(idx).unwrap_or(0)),
                    }
                    let tag = c.u32()?;
                    nodes[idx].tag = tag;

                    if tag == TAG_LEAF {
                        nodes[idx].leaf_index = Some(c.u32()?);
                        if kind == BspKind::Physics {
                            nodes[idx].solid = Some(c.i32()?);
                            nodes[idx].sphere = Some(decode_sphere(c)?);
                            let npoly = c.u32()? as usize;
                            nodes[idx].in_polys = read_n(c, npoly, Cursor::i16)?;
                        }
                        continue;
                    }

                    nodes[idx].plane = Some(decode_plane(c)?);

                    // Children, pushed so that they pop in file order (positive first).
                    stack.push(Task::Finish(idx));
                    match tag {
                        TAG_PORT | TAG_BPIN_BOTH | TAG_BPNN_BOTH => {
                            stack.push(Task::Read(Link::Neg(idx)));
                            stack.push(Task::Read(Link::Pos(idx)));
                        }
                        TAG_BPNN | TAG_BPIN_POS => stack.push(Task::Read(Link::Pos(idx))),
                        TAG_BPIN_NEG | TAG_BPNN_NEG => stack.push(Task::Read(Link::Neg(idx))),
                        // Open question #89: BPOL / BPFL and anything else fall through
                        // the child-unpacking switch and have no children at all.
                        _ => {}
                    }
                }
            }
        }
        Ok(Self { nodes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A vertex array of any type but the one the client draws is refused, and the record holding
    /// it with it, rather than read as vertices nobody filled in.
    #[test]
    fn a_vertex_array_of_type_zero_is_refused() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&0u32.to_le_bytes()); // vertex type
        buf.extend_from_slice(&3u32.to_le_bytes()); // vertex count
        assert!(matches!(
            VertexArray::decode(&mut Cursor::new(&buf)),
            Err(AssetError::Unsupported {
                what: "vertex-array type",
                value: 0
            })
        ));
    }

    /// Oracle: an independent reader of the shipped dats, which prints the dword byte-reversed.
    #[test]
    fn tags_render_as_four_characters() {
        let n = BspNode {
            tag: TAG_PORT,
            ..BspNode::default()
        };
        assert_eq!(n.tag_str(), "PORT");
        let n = BspNode {
            tag: TAG_LEAF,
            ..BspNode::default()
        };
        assert_eq!(n.tag_str(), "LEAF");
        let n = BspNode {
            tag: TAG_BPIN_BOTH,
            ..BspNode::default()
        };
        assert_eq!(n.tag_str(), "BPIN");
    }

    /// Open question #89: an unknown tag is a childless node, not a parse error.
    #[test]
    fn an_unrecognised_tag_reads_as_a_childless_node() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&0x4250_4F4Cu32.to_le_bytes()); // 'BPOL'
        buf.extend_from_slice(&[0u8; 16]); // plane
        let mut c = Cursor::new(&buf);
        let t = BspTree::decode(&mut c, BspKind::Cell).unwrap();
        assert_eq!(t.nodes.len(), 1);
        assert_eq!(t.nodes[0].tag_str(), "BPOL");
        assert!(t.nodes[0].pos_child.is_none() && t.nodes[0].neg_child.is_none());
        c.expect_end().unwrap();
    }
}
