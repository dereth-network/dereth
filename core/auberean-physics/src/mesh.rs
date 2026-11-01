//! A mesh's solid shape for collision: its physics polygons and the tree over them, in the shared
//! core's form; and a placed part's transform taken apart into a frame and a scale.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_assets::bsp::{Node, Tree};
use auberean_assets::mesh::{Mesh, MeshPolygon, VertexArray};
use auberean_world::math::Mat4;
use dereth_physics::geom::{BspNode, BspNodeKind, BspTree, Plane, Polygon, Sphere};
use dereth_primitives::{Quat, Vec3};

fn v3(v: auberean_primitives::Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

/// The positions of a vertex array (its first twelve bytes per vertex), or `None` when it stores
/// none or its bytes do not fit its format.
#[must_use]
pub fn positions(v: &VertexArray) -> Option<Vec<Vec3>> {
    if v.format & 0xE == 0 {
        return None;
    }
    let stride = auberean_assets::mesh::vertex_size(v.format);
    let count = usize::try_from(v.count).ok()?;
    if stride < 12 || v.bytes.len() != stride * count {
        return None;
    }
    let f = |b: &[u8], k: usize| f32::from_le_bytes([b[k], b[k + 1], b[k + 2], b[k + 3]]);
    Some(
        v.bytes
            .chunks_exact(stride)
            .map(|b| Vec3::new(f(b, 0), f(b, 4), f(b, 8)))
            .collect(),
    )
}

fn polygon(p: &MeshPolygon, at: &[Vec3]) -> Option<Polygon> {
    let vertices: Option<Vec<Vec3>> = p
        .vertices
        .iter()
        .map(|&k| at.get(usize::from(k)).copied())
        .collect();
    let vertices = vertices?;
    (vertices.len() >= 3).then(|| Polygon::new(vertices))
}

/// A mesh's physics tree in the shared core's form, or `None` when the mesh has none. A leaf names
/// its polygons by id; an id the mesh does not have is taken as an index.
// D2-04121, D2-04134, D2-04146, D2-04160
#[must_use]
pub fn physics_tree(mesh: &Mesh) -> Option<BspTree> {
    let tree = mesh.physics_tree.as_ref()?;
    let at = positions(&mesh.vertices)?;
    let polygons: Vec<Polygon> = mesh
        .physics_polygons
        .iter()
        .map(|p| polygon(p, &at).unwrap_or_else(|| Polygon::new(vec![Vec3::ZERO; 3])))
        .collect();
    let by_id: HashMap<u16, u32> = mesh
        .physics_polygons
        .iter()
        .enumerate()
        .filter_map(|(i, p)| Some((p.id, u32::try_from(i).ok()?)))
        .collect();
    Some(convert(tree, polygons, &by_id))
}

/// A tree over `polygons`, its leaves' polygon ids resolved through `by_id`.
#[must_use]
pub fn convert(tree: &Tree, polygons: Vec<Polygon>, by_id: &HashMap<u16, u32>) -> BspTree {
    let mut nodes = Vec::new();
    let mut leaves = 0i32;
    push(&tree.root, &mut nodes, &mut leaves, by_id, polygons.len());
    BspTree { nodes, polygons }
}

fn push(
    n: &Node,
    nodes: &mut Vec<BspNode>,
    leaves: &mut i32,
    by_id: &HashMap<u16, u32>,
    count: usize,
) -> u32 {
    let at = u32::try_from(nodes.len()).unwrap_or(u32::MAX);
    match n {
        Node::Leaf {
            solid,
            sphere,
            polygons,
        } => {
            let in_polys = polygons
                .iter()
                .filter_map(|&k| {
                    by_id
                        .get(&k)
                        .copied()
                        .or_else(|| (usize::from(k) < count).then(|| u32::from(k)))
                })
                .collect();
            nodes.push(BspNode {
                sphere: Sphere::new(v3(sphere.center), sphere.radius),
                splitting_plane: Plane::default(),
                pos_child: None,
                neg_child: None,
                kind: BspNodeKind::Leaf {
                    leaf_index: *leaves,
                    solid: *solid != 0,
                },
                in_polys,
            });
            *leaves += 1;
        }
        Node::Split {
            plane,
            front,
            back,
            sphere,
            ..
        } => {
            nodes.push(BspNode {
                sphere: sphere
                    .map_or_else(Sphere::default, |s| Sphere::new(v3(s.center), s.radius)),
                splitting_plane: Plane {
                    normal: v3(plane.normal),
                    d: plane.distance,
                },
                pos_child: None,
                neg_child: None,
                kind: BspNodeKind::Node,
                in_polys: Vec::new(),
            });
            let k = at as usize;
            if let Some(f) = front {
                let c = push(f, nodes, leaves, by_id, count);
                nodes[k].pos_child = Some(c);
            }
            if let Some(b) = back {
                let c = push(b, nodes, leaves, by_id, count);
                nodes[k].neg_child = Some(c);
            }
        }
    }
    at
}

/// A placed part's transform taken apart: its origin, its rotation and its scale along each of its
/// own axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub origin: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

/// `m` (scale, then rotate, then move) taken apart.
#[must_use]
pub fn decompose(m: &Mat4) -> Placement {
    let r = &m.0;
    let len = |i: usize| (r[i][0] * r[i][0] + r[i][1] * r[i][1] + r[i][2] * r[i][2]).sqrt();
    let s = [len(0), len(1), len(2)];
    let row = |i: usize| {
        let k = if s[i] > 0.0 { 1.0 / s[i] } else { 0.0 };
        [r[i][0] * k, r[i][1] * k, r[i][2] * k]
    };
    let (a, b, c) = (row(0), row(1), row(2));
    // Rows are the images of the axes, so the rotation matrix (columns as images) is their
    // transpose: m[j][i] = row_i[j].
    let (m00, m01, m02) = (a[0], b[0], c[0]);
    let (m10, m11, m12) = (a[1], b[1], c[1]);
    let (m20, m21, m22) = (a[2], b[2], c[2]);
    let trace = m00 + m11 + m22;
    let q = if trace > 0.0 {
        let t = (trace + 1.0).sqrt() * 2.0;
        Quat::new(0.25 * t, (m21 - m12) / t, (m02 - m20) / t, (m10 - m01) / t)
    } else if m00 > m11 && m00 > m22 {
        let t = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        Quat::new((m21 - m12) / t, 0.25 * t, (m01 + m10) / t, (m02 + m20) / t)
    } else if m11 > m22 {
        let t = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        Quat::new((m02 - m20) / t, (m01 + m10) / t, 0.25 * t, (m12 + m21) / t)
    } else {
        let t = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        Quat::new((m10 - m01) / t, (m02 + m20) / t, (m12 + m21) / t, 0.25 * t)
    };
    Placement {
        origin: Vec3::new(r[3][0], r[3][1], r[3][2]),
        rotation: q,
        scale: Vec3::new(s[0], s[1], s[2]),
    }
}

/// A cache of meshes' physics trees, and of what a part needs of each mesh, by mesh id.
#[derive(Debug, Default)]
pub struct Trees {
    by_mesh: HashMap<u32, Option<Arc<BspTree>>>,
    part_meshes: HashMap<u32, Option<crate::parts::PartMesh>>,
}

impl Trees {
    /// The physics tree of mesh `id`, decoded with `load` the first time it is asked for.
    pub fn get(&mut self, id: u32, load: impl FnOnce(u32) -> Option<Mesh>) -> Option<Arc<BspTree>> {
        self.by_mesh
            .entry(id)
            .or_insert_with(|| load(id).as_ref().and_then(physics_tree).map(Arc::new))
            .clone()
    }

    /// What a part needs of mesh `id`, when it was kept: `Some(None)` for a mesh not in the data.
    #[must_use]
    pub fn part_mesh(&self, id: u32) -> Option<Option<crate::parts::PartMesh>> {
        self.part_meshes.get(&id).cloned()
    }

    /// Keeps what a part needs of mesh `id`.
    pub fn keep_part_mesh(&mut self, id: u32, mesh: Option<crate::parts::PartMesh>) {
        self.part_meshes.insert(id, mesh);
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (transform arithmetic against its own construction)
    use super::*;
    use auberean_primitives::Quat as Q;

    #[test]
    fn a_transform_comes_apart_into_what_built_it() {
        let q = Q {
            w: 0.8,
            x: 0.1,
            y: -0.4,
            z: 0.3,
        };
        let n = (q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z).sqrt();
        let q = Q {
            w: q.w / n,
            x: q.x / n,
            y: q.y / n,
            z: q.z / n,
        };
        let m = Mat4::from_parts(
            auberean_primitives::Vec3::new(10.0, -3.0, 2.5),
            q,
            auberean_primitives::Vec3::new(2.0, 2.0, 2.0),
        );
        let p = decompose(&m);
        assert!((p.origin.x - 10.0).abs() < 1e-5 && (p.origin.z - 2.5).abs() < 1e-5);
        assert!((p.scale.x - 2.0).abs() < 1e-5 && (p.scale.z - 2.0).abs() < 1e-5);
        let same = (p.rotation.w - q.w).abs() < 1e-4
            && (p.rotation.x - q.x).abs() < 1e-4
            && (p.rotation.y - q.y).abs() < 1e-4
            && (p.rotation.z - q.z).abs() < 1e-4;
        let opposite = (p.rotation.w + q.w).abs() < 1e-4 && (p.rotation.x + q.x).abs() < 1e-4;
        assert!(same || opposite, "{:?} vs {q:?}", p.rotation);
    }
}
