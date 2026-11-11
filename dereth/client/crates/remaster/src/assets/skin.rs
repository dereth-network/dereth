//! Bodies skinned to an object's parts: an art pack's mesh drawn in place of a setup's rigid
//! parts, its vertices moved each frame by the parts' own transforms.
//!
//! **The parts are the joints.** An object's animation places every part each frame, in the
//! object's own space, and the frame hands each part's mesh transform to the remaster
//! ([`crate::scene::PartPose`]). A body's vertex follows up to four parts, each with a weight;
//! a part's inverse bind matrix takes the body's bind space to the part's own, so the part's
//! transform times it moves a vertex as the part moves. Nothing about the object but its pixels
//! changes: its animation, its collision, its selection and what the server sees are its parts'.
//!
//! **Drawn by one part.** The body is drawn through one part's instance, its *anchor* (the first
//! part it follows): each frame its vertices are written in that part's own space, so every pass
//! that draws the part (the G-buffer, the shadows, the lights) draws the body, lit and shadowed
//! as an object. The parts it follows draw nothing of their own; any other part draws as it is.
//!
//! **Names.** A body's mesh is one per object, named past every retail id ([`skin_mesh`]): the
//! body's place in the pack and a number of the object's own. Its surfaces are named by the body's
//! place and the primitive's ([`skin_surface`]).

use glam::{Mat3, Mat4, Vec3};

use super::Mesh;

/// The first id of the bodies' meshes: `PACK_SKIN_MESH | skin << 16 | object`.
pub const PACK_SKIN_MESH: u32 = 0xF400_0000;
/// The first id of the bodies' surfaces: `PACK_SKIN_SURFACE | skin << 8 | primitive`.
pub const PACK_SKIN_SURFACE: u32 = 0xF500_0000;

/// The id of the body `skin`'s mesh drawn for the object numbered `object`.
#[must_use]
pub fn skin_mesh(skin: u32, object: u16) -> u32 {
    PACK_SKIN_MESH | (skin & 0xFF) << 16 | u32::from(object)
}

/// The body and object a body's mesh id names; `None` for any other id.
#[must_use]
pub fn of_skin_mesh(id: u32) -> Option<(u32, u16)> {
    (id & 0xFF00_0000 == PACK_SKIN_MESH).then_some(((id >> 16) & 0xFF, (id & 0xFFFF) as u16))
}

/// The id of the body `skin`'s primitive `primitive`'s surface.
#[must_use]
pub fn skin_surface(skin: u32, primitive: u8) -> u32 {
    PACK_SKIN_SURFACE | (skin & 0xFFFF) << 8 | u32::from(primitive)
}

/// The body and primitive a body's surface id names; `None` for any other id.
#[must_use]
pub fn of_skin_surface(id: u32) -> Option<(u32, u8)> {
    (id & 0xFF00_0000 == PACK_SKIN_SURFACE).then_some(((id >> 8) & 0xFFFF, (id & 0xFF) as u8))
}

/// One body as the remaster draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct SkinModel {
    /// Its place in the pack.
    pub skin: u32,
    /// Its triangles in its bind space, each primitive a subset named by [`skin_surface`], with
    /// its vertices' tangents and colors.
    pub mesh: Mesh,
    /// Each vertex's parts, by their place in the setup, and its weight on each.
    pub parts: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    /// Each part's inverse bind matrix, by its place in the setup; `None` for a part the body
    /// does not follow.
    pub inverse_binds: Vec<Option<Mat4>>,
    /// The body's triangles at rest in its anchor's own space, which the tables hold as its mesh
    /// until the first frame poses it, with a sphere wide enough to hold it posed.
    pub rest: Mesh,
}

impl SkinModel {
    /// The body `skin` of `mesh`, whose vertices follow `parts` with `weights`, each part's
    /// inverse bind matrix in `inverse_binds`. `None` when its influences do not match its
    /// vertices, or it follows no part.
    #[must_use]
    pub fn new(
        skin: u32,
        mesh: Mesh,
        parts: Vec<[u16; 4]>,
        weights: Vec<[f32; 4]>,
        inverse_binds: &[(u16, Mat4)],
    ) -> Option<Self> {
        if parts.len() != mesh.vertices.len() || weights.len() != mesh.vertices.len() {
            return None;
        }
        let count = inverse_binds
            .iter()
            .map(|(p, _)| usize::from(*p) + 1)
            .max()?;
        let mut mesh = mesh;
        if mesh.tangents.len() != mesh.vertices.len() {
            mesh.tangents = super::tangents(&mesh);
        }
        let mut binds = vec![None; count];
        for (p, m) in inverse_binds {
            binds[usize::from(*p)] = Some(*m);
        }
        let mut model = Self {
            skin,
            mesh,
            parts,
            weights,
            inverse_binds: binds,
            rest: Mesh::default(),
        };
        model.rest = model.rest_mesh();
        Some(model)
    }

    /// The part the body is drawn through: the first it follows.
    #[must_use]
    pub fn anchor(&self) -> u16 {
        self.inverse_binds
            .iter()
            .position(Option::is_some)
            .and_then(|p| u16::try_from(p).ok())
            .unwrap_or(0)
    }

    /// Whether the body follows `part`, so that the part draws nothing of its own.
    #[must_use]
    pub fn follows(&self, part: usize) -> bool {
        self.inverse_binds.get(part).is_some_and(Option::is_some)
    }

    /// The body at rest in its anchor's own space.
    fn rest_mesh(&self) -> Mesh {
        let to_anchor = self
            .inverse_binds
            .get(usize::from(self.anchor()))
            .copied()
            .flatten()
            .unwrap_or(Mat4::IDENTITY);
        let normals = Mat3::from_mat4(to_anchor).inverse().transpose();
        let mut rest = self.mesh.clone();
        for v in &mut rest.vertices {
            v.position = to_anchor
                .transform_point3(Vec3::from(v.position))
                .to_array();
            v.normal = (normals * Vec3::from(v.normal))
                .normalize_or_zero()
                .to_array();
        }
        // A sphere that holds the body however its limbs swing: round its middle at rest, a
        // third again as wide and a quarter metre more.
        let (lo, hi) = rest.vertices.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(lo, hi), v| (lo.min(v.position.into()), hi.max(v.position.into())),
        );
        let centre = if lo.x <= hi.x {
            (lo + hi) * 0.5
        } else {
            Vec3::ZERO
        };
        let radius = rest
            .vertices
            .iter()
            .map(|v| Vec3::from(v.position).distance(centre))
            .fold(0.0f32, f32::max);
        rest.sphere = Some((centre.to_array(), radius * 1.35 + 0.25));
        rest.sort_center = centre.to_array();
        rest.tangents = Vec::new();
        rest
    }

    /// Each followed part's skinning matrix this frame, in the anchor's own space: the part's
    /// mesh transform times its inverse bind matrix, seen from the anchor's transform. `parts` is
    /// every part's mesh transform, in the setup's order. `None` when the frame lacks a part the
    /// body follows, or the anchor's transform cannot be inverted.
    #[must_use]
    pub fn joints(&self, parts: &[Mat4]) -> Option<Vec<Mat4>> {
        let anchor = *parts.get(usize::from(self.anchor()))?;
        if anchor.determinant().abs() < 1e-12 {
            return None;
        }
        let from_anchor = anchor.inverse();
        self.inverse_binds
            .iter()
            .enumerate()
            .map(|(k, b)| match b {
                Some(b) => parts.get(k).map(|m| from_anchor * *m * *b),
                None => Some(Mat4::IDENTITY),
            })
            .collect()
    }

    /// Every vertex posed by `joints` ([`Self::joints`]): its position, normal and tangent (with
    /// its handedness), in the anchor's own space.
    #[must_use]
    pub fn posed(&self, joints: &[Mat4]) -> Vec<([f32; 3], [f32; 3], [f32; 4])> {
        self.mesh
            .vertices
            .iter()
            .zip(self.parts.iter().zip(&self.weights))
            .zip(&self.mesh.tangents)
            .map(|((v, (ps, ws)), t)| {
                let mut m = Mat4::ZERO;
                for (p, w) in ps.iter().zip(ws) {
                    if *w > 0.0 {
                        m += *joints.get(usize::from(*p)).unwrap_or(&Mat4::IDENTITY) * *w;
                    }
                }
                let turn = Mat3::from_mat4(m);
                let normals = turn.inverse().transpose();
                let n = (normals * Vec3::from(v.normal)).normalize_or_zero();
                let tan = (turn * Vec3::new(t[0], t[1], t[2])).normalize_or_zero();
                (
                    m.transform_point3(Vec3::from(v.position)).to_array(),
                    n.to_array(),
                    [tan.x, tan.y, tan.z, t[3]],
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;
    use crate::assets::{Subset, SurfaceRef, Vertex};

    /// Two parts a metre apart along the third axis, one vertex on each, and one between them
    /// following both evenly; part 1 is the anchor (part 0 follows nothing).
    fn two_parts() -> SkinModel {
        let v = |z: f32| Vertex {
            position: [0.0, 0.0, z],
            normal: [1.0, 0.0, 0.0],
            uv: [0.0, 0.0],
        };
        let mesh = Mesh {
            vertices: vec![v(0.0), v(1.0), v(0.5)],
            indices: vec![0, 1, 2],
            subsets: vec![Subset {
                first_index: 0,
                index_count: 3,
                surface: SurfaceRef::Surface(skin_surface(3, 0)),
                two_sided: false,
                tiled: true,
            }],
            ..Mesh::default()
        };
        SkinModel::new(
            3,
            mesh,
            vec![[1, 0, 0, 0], [2, 0, 0, 0], [1, 2, 0, 0]],
            vec![
                [1.0, 0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0, 0.0],
                [0.5, 0.5, 0.0, 0.0],
            ],
            &[
                (1, Mat4::IDENTITY),
                (2, Mat4::from_translation(Vec3::new(0.0, 0.0, -1.0))),
            ],
        )
        .expect("a body")
    }

    #[test]
    fn a_bodys_ids_name_the_body_and_the_object_or_the_primitive() {
        assert_eq!(of_skin_mesh(skin_mesh(5, 900)), Some((5, 900)));
        assert_eq!(of_skin_surface(skin_surface(5, 2)), Some((5, 2)));
        assert_eq!(of_skin_mesh(0x0100_0001), None);
        assert_eq!(of_skin_surface(skin_mesh(5, 900)), None);
    }

    #[test]
    fn a_body_is_drawn_through_the_first_part_it_follows_and_hides_the_parts_it_follows() {
        let b = two_parts();
        assert_eq!(b.anchor(), 1);
        assert!(!b.follows(0) && b.follows(1) && b.follows(2) && !b.follows(3));
        let (centre, radius) = b.rest.sphere.expect("a sphere");
        assert!(Vec3::from(centre).distance(Vec3::new(0.0, 0.0, 0.5)) < 1e-6);
        assert!(radius > 0.5, "wide enough to hold the body as it moves");
    }

    #[test]
    fn a_body_whose_parts_move_together_moves_whole_and_a_shared_vertex_follows_both() {
        let b = two_parts();
        let anywhere =
            Mat4::from_rotation_z(0.7) * Mat4::from_translation(Vec3::new(3.0, 4.0, 5.0));
        // Both parts where they rest, moved together: in the anchor's own space, the rest.
        let parts = [
            Mat4::IDENTITY,
            anywhere,
            anywhere * Mat4::from_translation(Vec3::Z),
        ];
        let posed = b.posed(&b.joints(&parts).expect("joints"));
        for (p, v) in posed.iter().zip(&b.mesh.vertices) {
            assert!(Vec3::from(p.0).distance(Vec3::from(v.position)) < 1e-5);
            assert!(Vec3::from(p.1).distance(Vec3::X) < 1e-5);
        }
        // Part 2 lifted a metre more: its vertex follows it, the shared one goes half as far.
        let parts = [
            Mat4::IDENTITY,
            anywhere,
            anywhere * Mat4::from_translation(Vec3::Z * 2.0),
        ];
        let posed = b.posed(&b.joints(&parts).expect("joints"));
        assert!(Vec3::from(posed[0].0).distance(Vec3::ZERO) < 1e-5);
        assert!(Vec3::from(posed[1].0).distance(Vec3::new(0.0, 0.0, 2.0)) < 1e-5);
        assert!(Vec3::from(posed[2].0).distance(Vec3::new(0.0, 0.0, 1.0)) < 1e-5);
        // A frame that lacks a part the body follows poses nothing.
        assert!(b.joints(&parts[..2]).is_none());
    }

    #[test]
    fn influences_that_do_not_match_the_vertices_make_no_body() {
        let b = two_parts();
        assert!(SkinModel::new(0, b.mesh.clone(), vec![[0; 4]], b.weights.clone(), &[]).is_none());
        assert!(
            SkinModel::new(0, b.mesh.clone(), b.parts.clone(), b.weights.clone(), &[]).is_none(),
            "a body that follows no part"
        );
    }
}
