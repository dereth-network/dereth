//! One part of an object: a mesh of its setup, as the object's look swaps it, with its transform
//! into the object's frame, the material changes the look makes to it, and its mesh's physics tree.

use std::sync::Arc;

use auberean_assets::mesh::Mesh;
use auberean_primitives::{DataId, Vec3 as WorldVec3};
use auberean_world::look::{MaterialAtom, ANY_KIND};
use auberean_world::math::Mat4;
use dereth_physics::geom::BspTree;
use dereth_physics::source::PhysicsPart as PlacedPart;
use dereth_physics::transition::collide::object_part_find_obj_collisions_scaled;
use dereth_physics::transition::TreeScale;
use dereth_physics::transition::{Transition, TransitionCtx, TransitionState};
use dereth_primitives::{CellId, Position, Vec3};

use super::{PartRecords, NO_OWNER};
use crate::geom::{block_offset, point_to_local, BBox, BoxOps, Ray};

/// What a part needs of its mesh.
#[derive(Clone, Debug)]
pub struct PartMesh {
    pub id: DataId,
    /// Which body part the mesh is: what a look's mesh swap matches.
    pub kind: u32,
    /// Its physics tree, when it has one.
    pub tree: Option<Arc<BspTree>>,
    /// The box around its vertices, and the box they were read with (the same for a mesh read
    /// from the data).
    pub bounds: BBox,
    pub default_bounds: BBox,
    /// It has triangles to draw (levels of detail).
    pub drawn: bool,
    /// Its vertices are weighted to bones.
    pub weighted: bool,
}

fn bbox(b: auberean_primitives::BBox) -> BBox {
    BBox::new(
        Vec3::new(b.min.x, b.min.y, b.min.z),
        Vec3::new(b.max.x, b.max.y, b.max.z),
    )
}

impl PartMesh {
    /// Mesh `id` as a part shows it ([`PartRecords::part_mesh`]); none when it is not in the
    /// data.
    #[must_use]
    pub fn load(records: &dyn PartRecords, id: DataId) -> Option<Self> {
        records.part_mesh(id)
    }

    /// Mesh `id` as a part shows it, read from the records; none when it is not in the data.
    #[must_use]
    pub fn read<R: PartRecords + ?Sized>(records: &R, id: DataId) -> Option<Self> {
        let mesh = records.mesh(id)?;
        Some(Self::of(id, &mesh, records.physics_tree(id)))
    }

    /// Mesh `id` as a part shows it, given the mesh and its physics tree.
    #[must_use]
    pub fn of(id: DataId, mesh: &Mesh, tree: Option<Arc<BspTree>>) -> Self {
        let bounds = bbox(mesh.vertices.bounds);
        Self {
            id,
            kind: mesh.kind,
            tree,
            bounds,
            default_bounds: bounds,
            drawn: !mesh.levels.is_empty(),
            weighted: mesh.has_matrix_weights(),
        }
    }
}

/// One part of an object.
// D2-06687
#[derive(Clone, Debug)]
pub struct PhysicsPart {
    /// The object the part belongs to.
    pub owner: u64,
    /// The part's mesh space to the object's frame, as the last refresh left it.
    pub matrix: Mat4,
    /// Its mesh; none when the mesh it was last given is not in the data.
    pub mesh: Option<PartMesh>,
    /// The material changes the object's look makes to it, in the order they apply.
    pub materials: Vec<MaterialAtom>,
}

impl Default for PhysicsPart {
    /// A part with no mesh and no owner, placed at the object's frame.
    // D2-06678
    fn default() -> Self {
        Self {
            owner: NO_OWNER,
            matrix: Mat4::IDENTITY,
            mesh: None,
            materials: Vec::new(),
        }
    }
}

/// `p` through `m`, as the drawing does it: the fourth column divides.
fn project(m: &Mat4, p: Vec3) -> Vec3 {
    let r = &m.0;
    let w = 1.0 / (((p.x * r[0][3] + r[1][3] * p.y) + r[2][3] * p.z) + r[3][3]);
    Vec3::new(
        (((r[0][0] * p.x + r[1][0] * p.y) + p.z * r[2][0]) + r[3][0]) * w,
        (((p.z * r[2][1] + p.x * r[0][1]) + p.y * r[1][1]) + r[3][1]) * w,
        (((p.z * r[2][2] + p.x * r[0][2]) + p.y * r[1][2]) + r[3][2]) * w,
    )
}

fn corners(b: &BBox) -> [Vec3; 8] {
    let (lo, hi) = (b.min, b.max);
    [
        Vec3::new(lo.x, lo.y, lo.z),
        Vec3::new(hi.x, lo.y, lo.z),
        Vec3::new(lo.x, hi.y, lo.z),
        Vec3::new(hi.x, hi.y, lo.z),
        Vec3::new(lo.x, lo.y, hi.z),
        Vec3::new(hi.x, lo.y, hi.z),
        Vec3::new(lo.x, hi.y, hi.z),
        Vec3::new(hi.x, hi.y, hi.z),
    ]
}

impl PhysicsPart {
    /// A part showing mesh `mesh`; none when the mesh is not in the data.
    // D2-06688
    #[must_use]
    pub fn make(records: &dyn PartRecords, mesh: DataId) -> Option<Self> {
        let mut p = Self::default();
        p.set_mesh(records, mesh).then_some(p)
    }

    /// Shows mesh `mesh` instead of the one it has; false, leaving it with none, when the mesh is
    /// not in the data.
    // D2-06683
    pub fn set_mesh(&mut self, records: &dyn PartRecords, mesh: DataId) -> bool {
        self.mesh = PartMesh::load(records, mesh);
        self.mesh.is_some()
    }

    /// A look's mesh swap: when `kind` is set and the part's mesh is of it (or `kind` matches every
    /// mesh), the part shows `mesh` instead, or nothing when `mesh` is not in the data. Whether it
    /// was swapped.
    // D2-05601
    pub fn swap_mesh(&mut self, records: &dyn PartRecords, kind: u32, mesh: DataId) -> bool {
        if kind == 0 {
            return false;
        }
        let Some(own) = &self.mesh else {
            return false;
        };
        if kind != ANY_KIND && kind != own.kind {
            return false;
        }
        self.mesh = PartMesh::load(records, mesh);
        true
    }

    /// Whether its mesh's vertices are weighted to bones.
    // D2-06679
    #[must_use]
    pub fn has_bone_weights(&self) -> bool {
        self.mesh.as_ref().is_some_and(|m| m.weighted)
    }

    /// Whether its mesh has a physics tree.
    // D2-06682
    #[must_use]
    pub fn has_physics_tree(&self) -> bool {
        self.mesh.as_ref().is_some_and(|m| m.tree.is_some())
    }

    /// Grows `out` to take in the box around its mesh's vertices: as they are (`transformed`
    /// false), or carried through the part's transform, the box over the eight carried corners,
    /// for a mesh that has triangles to draw. The box around the mesh's vertices is the one its
    /// record keeps.
    // D2-06680, D2-05535
    pub fn grow_render_box(&self, out: &mut BBox, transformed: bool) {
        self.build_box(out, transformed, |m| m.bounds);
    }

    /// As [`PhysicsPart::grow_render_box`], with the box the vertices were read with.
    // D2-06681, D2-05536
    pub fn grow_default_render_box(&self, out: &mut BBox, transformed: bool) {
        self.build_box(out, transformed, |m| m.default_bounds);
    }

    /// The box `of` gives for the part's mesh (the one its look left it, shared by every part
    /// showing it), as it is or carried through the part's transform.
    // D2-05595
    fn build_box(&self, out: &mut BBox, transformed: bool, of: impl Fn(&PartMesh) -> BBox) {
        let Some(mesh) = &self.mesh else {
            return;
        };
        if !transformed {
            of(mesh).build_bounding_box(out);
            return;
        }
        if !mesh.drawn {
            return;
        }
        let mut local = BBox::for_adjustment();
        of(mesh).build_bounding_box(&mut local);
        let mut moved = BBox::for_adjustment();
        for c in corners(&local) {
            moved.adjust_bbox(project(&self.matrix, c));
        }
        moved.build_bounding_box(out);
    }

    /// Takes `m` as its transform.
    // D2-06684
    pub fn refresh_matrix(&mut self, m: &Mat4) {
        self.matrix = *m;
    }

    /// Its physics tree against the mover of `t`, the tree standing at `pos` scaled by `scale`
    /// along each axis. OK when its mesh has no tree.
    // D2-06686
    pub fn find_obj_collisions(
        &self,
        ctx: &TransitionCtx<'_>,
        t: &mut Transition,
        pos: &Position,
        scale: Vec3,
    ) -> TransitionState {
        let Some(tree) = self.mesh.as_ref().and_then(|m| m.tree.clone()) else {
            return TransitionState::Ok;
        };
        object_part_find_obj_collisions_scaled(
            ctx,
            t,
            &PlacedPart {
                pos: *pos,
                gfxobj_scale: scale.z,
                physics_bsp: Some(tree),
                ..PlacedPart::default()
            },
            TreeScale::axes(scale),
        )
    }

    /// A ray, given in the landblock frame of `ray_cell`, through its physics tree standing at
    /// `pos`: the ray is brought into the tree's frame and multiplied by `inv_scale` (one over the
    /// object's scale); where it reaches the tree's solid side multiplies `fraction`, and `normal`
    /// is the tree's (in its own frame). False when its mesh has no tree or the ray reaches
    /// nothing solid.
    // D2-06689
    pub fn ray_through_tree(
        &self,
        pos: &Position,
        inv_scale: Vec3,
        ray: &Ray,
        ray_cell: CellId,
        fraction: &mut f32,
        normal: &mut Vec3,
    ) -> bool {
        let Some(tree) = self.mesh.as_ref().and_then(|m| m.tree.as_ref()) else {
            return false;
        };
        let off = block_offset(pos.cell, ray_cell);
        let p = ray.point;
        let start = point_to_local(&pos.frame, Vec3::new(off.x + p.x, off.y + p.y, off.z + p.z));
        let start = Vec3::new(
            start.x * inv_scale.x,
            start.y * inv_scale.y,
            start.z * inv_scale.z,
        );
        let l = ray.length;
        let end = point_to_local(
            &pos.frame,
            Vec3::new(
                (l * ray.dir.x + p.x) + off.x,
                (l * ray.dir.y + p.y) + off.y,
                (l * ray.dir.z + p.z) + off.z,
            ),
        );
        let local = Ray::from_offset(
            start,
            Vec3::new(
                end.x * inv_scale.x - start.x,
                end.y * inv_scale.y - start.y,
                end.z * inv_scale.z - start.z,
            ),
        );
        let mut t = 0.0;
        if !super::trace_ray(tree, &local, &mut t, normal) {
            return false;
        }
        *fraction *= t;
        true
    }
}

/// A world-records vector as the physics' own.
pub(super) fn vec3(v: WorldVec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}
