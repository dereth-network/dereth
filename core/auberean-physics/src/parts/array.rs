//! One setup's parts: the setup and its animation map, a part for each of its meshes, the
//! transforms its bones take, and what the physics asks of the setup.
//!
//! The setup is the loaded one the records hand every array made from it: changing its step-down
//! height or its spheres changes them for every object made from that setup.

use std::sync::Arc;

use auberean_assets::animation::AnimMap;
use auberean_assets::setup::{BoneTransform, Cylinder, Setup};
use auberean_primitives::{DataId, Quat as WorldQuat, Rgba, Sphere, Vec3 as WorldVec3};
use auberean_world::look::MaterialAtom;
use auberean_world::math::Mat4;
use auberean_world::place::LoadedSetup;
use dereth_physics::transition::{Transition, TransitionCtx, TransitionState};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use super::part::vec3;
use super::{parent_index, placement_frame, PartRecords, PhysicsPart, NO_OWNER};
use crate::geom::{point_to_local, rotation_from_axes, set_rotate, BBox, Ray};

/// The step heights of an array without a setup.
const NO_SETUP_STEP: f32 = 0.01;

/// A bone's transform before anything places it: where its parent is, unscaled, unturned.
const REST: BoneTransform = BoneTransform {
    offset: WorldVec3::ZERO,
    scale: WorldVec3::new(1.0, 1.0, 1.0),
    rotation: WorldQuat::IDENTITY,
};

const WHITE: Rgba = Rgba {
    r: 1.0,
    g: 1.0,
    b: 1.0,
    a: 1.0,
};

/// One bone of a skeleton as a point: where the bone is in the array's frame, and its parent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BonePoint {
    pub origin: Vec3,
    /// The parent bone, all ones for a root.
    pub parent: u32,
}

/// One setup's parts.
// D2-06676
#[derive(Clone, Debug)]
pub struct PartArray {
    /// The object the parts belong to.
    pub owner: u64,
    /// Whether a part has a physics tree, as last worked out ([`PartArray::cache_has_physics_tree`]):
    /// collisions and rays meet the parts only when it is set.
    pub has_physics_tree: bool,
    /// Whether a part's vertices are weighted to bones, as last worked out
    /// ([`PartArray::cache_has_bone_weights`]).
    pub has_bone_weights: bool,
    /// The setup as loaded, shared by every array made from it, and its id.
    pub setup: Option<Arc<LoadedSetup>>,
    pub setup_id: DataId,
    /// The animation map and its id.
    pub anim_map: Option<(DataId, Arc<AnimMap>)>,
    /// The setup's pose the bones were placed in (0 when the setup has none of the kind asked).
    pub placement: u32,
    /// One part for each of the setup's meshes; none for a mesh id of 0, or a part not made.
    pub parts: Vec<Option<PhysicsPart>>,
    /// Each bone's transform relative to its parent.
    pub bone_transforms: Vec<BoneTransform>,
    /// Each bone's transform into the array's frame, without the bones' own scales.
    pub bone_matrices: Vec<Mat4>,
    /// The bones as points, as last worked out ([`PartArray::work_out_bone_points`]).
    pub bone_points: Vec<BonePoint>,
    /// The color the parts are drawn in; its alpha is their opacity.
    pub color: Rgba,
    /// The box around what is drawn, and around what is drawn as it was read, as last worked out.
    pub render_bounds: BBox,
    pub default_render_bounds: BBox,
}

impl Default for PartArray {
    /// An array of no setup, no parts and no owner, drawn white.
    // D2-06675
    fn default() -> Self {
        Self {
            owner: NO_OWNER,
            has_physics_tree: false,
            has_bone_weights: false,
            setup: None,
            setup_id: DataId::NONE,
            anim_map: None,
            placement: 0,
            parts: Vec::new(),
            bone_transforms: Vec::new(),
            bone_matrices: Vec::new(),
            bone_points: Vec::new(),
            color: WHITE,
            render_bounds: BBox::default(),
            default_render_bounds: BBox::default(),
        }
    }
}

/// A frame from a transform matrix: its three axes each brought to unit length (a zero axis stays
/// zero), its translation as the origin.
fn frame_of(m: &Mat4) -> Frame {
    let r = &m.0;
    let unit = |v: [f32; 4]| {
        let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if n == 0.0 {
            [0.0; 3]
        } else {
            [v[0] / n, v[1] / n, v[2] / n]
        }
    };
    let (x, y, z) = (unit(r[0]), unit(r[1]), unit(r[2]));
    let mut f = Frame::new(Vec3::new(r[3][0], r[3][1], r[3][2]), Quat::IDENTITY);
    set_rotate(
        &mut f,
        rotation_from_axes(&[x[0], x[1], x[2], y[0], y[1], y[2], z[0], z[1], z[2]]),
    );
    f
}

/// `a` as seen from `b`: its origin in `b`'s frame and the turn from `b`'s rotation to its own.
// D2-13171
pub(crate) fn relative(a: &Frame, b: &Frame) -> Frame {
    let mut out = Frame::new(point_to_local(b, a.origin), b.rotation);
    let (p, q) = (a.rotation, b.rotation);
    let w = ((q.y * p.y + q.x * p.x) + q.w * p.w) + q.z * p.z;
    let x = ((q.w * p.x - q.x * p.w) - p.z * q.y) + q.z * p.y;
    let y = ((p.z * q.x + q.w * p.y) - q.y * p.w) - q.z * p.x;
    let z = ((p.z * q.w - q.x * p.y) + q.y * p.x) - q.z * p.w;
    set_rotate(&mut out, Quat::new(w, x, y, z));
    out
}

impl PartArray {
    /// The setup as it is now, with what any array of it changed.
    #[must_use]
    pub fn current_setup(&self) -> Option<Arc<Setup>> {
        self.setup.as_ref().map(|s| s.get())
    }

    /// Gives the array and every part an owner.
    // D2-06601
    pub fn set_owner(&mut self, owner: u64) {
        self.owner = owner;
        for p in self.parts.iter_mut().flatten() {
            p.owner = owner;
        }
    }

    /// Takes animation map `id` (0 for none) in place of the one it has; false, with none, when the
    /// map is not in the data.
    // D2-06603
    pub fn set_anim_map(&mut self, records: &dyn PartRecords, id: DataId) -> bool {
        self.anim_map = None;
        if !id.is_none() {
            let Some(map) = records.anim_map(id) else {
                return false;
            };
            self.anim_map = Some((id, map));
        }
        true
    }

    /// The id of its animation map; 0 for none.
    #[must_use]
    pub fn anim_map_id(&self) -> DataId {
        self.anim_map.as_ref().map_or(DataId::NONE, |(id, _)| *id)
    }

    /// The setup's spheres become `spheres`, for every array of the setup; false without a setup.
    pub fn set_spheres(&mut self, spheres: &[Sphere]) -> bool {
        let Some(setup) = &self.setup else {
            return false;
        };
        setup.change(|s| s.spheres = spheres.to_vec());
        true
    }

    /// Places the bones in the setup's pose `id`, or its pose 0 when it has none of that kind: each
    /// bone takes the pose's transform. False without a setup, or when the setup has no pose 0
    /// either (the bones then keep their transforms).
    // D2-06606
    pub fn set_placement_frame(&mut self, id: u32) -> bool {
        let Some(setup) = self.current_setup() else {
            return false;
        };
        self.placement = if placement_frame(&setup, id).is_some() {
            id
        } else {
            0
        };
        let Some(pose) = placement_frame(&setup, self.placement) else {
            return false;
        };
        for (t, b) in self.bone_transforms.iter_mut().zip(&pose.bones) {
            *t = *b;
        }
        true
    }

    /// A part for each of the setup's meshes (none for a mesh id of 0); false without a setup, when
    /// it already has parts, or when a mesh is not in the data (the parts made before it are kept).
    // D2-06607
    pub fn init_parts(&mut self, records: &dyn PartRecords) -> bool {
        let Some(setup) = self.current_setup() else {
            return false;
        };
        if !self.parts.is_empty() {
            return false;
        }
        self.parts = vec![None; setup.meshes.len()];
        for (i, &mesh) in setup.meshes.iter().enumerate() {
            if mesh.is_none() {
                continue;
            }
            match PhysicsPart::make(records, mesh) {
                Some(p) => self.parts[i] = Some(p),
                None => return false,
            }
        }
        true
    }

    /// Lets go of its parts.
    // D2-06608
    pub fn destroy_parts(&mut self) {
        self.parts.clear();
    }

    /// A transform for each of the setup's bones, placed in its pose ([`PartArray::set_placement_frame`]),
    /// and their matrices.
    // D2-06609
    pub fn init_bones(&mut self) -> bool {
        let n = self.current_setup().map_or(0, |s| s.bones.len());
        self.bone_transforms = vec![REST; n];
        self.bone_matrices = vec![Mat4::IDENTITY; n];
        self.set_placement_frame(self.placement);
        self.update_bones();
        true
    }

    /// Lets go of its bones.
    // D2-06610
    pub fn destroy_bones(&mut self) {
        self.bone_transforms.clear();
        self.bone_matrices.clear();
    }

    /// Each bone's matrix from its transform: unscaled, followed by its parent's matrix (a bone
    /// whose parent is past the setup's follows nothing). The drawing's own matrices, with the
    /// bones' scales and the rest pose taken off, are the animation's.
    pub fn update_bones(&mut self) {
        let Some(setup) = self.current_setup() else {
            return;
        };
        for i in 0..self.bone_transforms.len().min(self.bone_matrices.len()) {
            let t = &self.bone_transforms[i];
            let own = Mat4::from_parts(t.offset, t.rotation, WorldVec3::new(1.0, 1.0, 1.0));
            let parent = parent_index(&setup, i);
            self.bone_matrices[i] = if parent == u32::MAX {
                own
            } else {
                let p = self
                    .bone_matrices
                    .get(parent as usize)
                    .copied()
                    .unwrap_or(Mat4::IDENTITY);
                own.then(&p)
            };
        }
    }

    /// How many bones it has.
    #[must_use]
    pub fn bone_count(&self) -> usize {
        self.bone_matrices.len()
    }

    /// The setup's selection sphere; none without a setup.
    // D2-06611
    #[must_use]
    pub fn selection_sphere(&self) -> Option<Sphere> {
        self.current_setup().map(|s| s.selection_sphere)
    }

    /// The setup's spheres (none without a setup).
    // D2-06612, D2-06613
    #[must_use]
    pub fn spheres(&self) -> Vec<Sphere> {
        self.current_setup()
            .map_or_else(Vec::new, |s| s.spheres.clone())
    }

    /// The setup's upright cylinders (none without a setup).
    // D2-06614, D2-06615, D2-06455, D2-06456
    #[must_use]
    pub fn cylinders(&self) -> Vec<Cylinder> {
        self.current_setup()
            .map_or_else(Vec::new, |s| s.cylinders.clone())
    }

    /// The setup's radius (0 without a setup).
    // D2-06616, D2-06457
    #[must_use]
    pub fn radius(&self) -> f32 {
        self.current_setup().map_or(0.0, |s| s.radius)
    }

    /// The setup's height (0 without a setup).
    // D2-06617, D2-06458
    #[must_use]
    pub fn height(&self) -> f32 {
        self.current_setup().map_or(0.0, |s| s.height)
    }

    /// Whether the setup lets the body turn freely.
    // D2-06618, D2-06459
    #[must_use]
    pub fn allows_free_heading(&self) -> bool {
        self.current_setup().is_some_and(|s| s.turns_freely())
    }

    /// The setup's step-up height, 0.01 without a setup.
    // D2-06619
    #[must_use]
    pub fn step_up_height(&self) -> f32 {
        self.current_setup()
            .map_or(NO_SETUP_STEP, |s| s.step_up_height)
    }

    /// The setup's step-down height, 0.01 without a setup.
    // D2-06620
    #[must_use]
    pub fn step_down_height(&self) -> f32 {
        self.current_setup()
            .map_or(NO_SETUP_STEP, |s| s.step_down_height)
    }

    /// The setup's step-down height becomes `height`, for every array of the setup; false without a
    /// setup.
    // D2-06621, D2-12873
    pub fn set_step_down_height(&mut self, height: f32) -> bool {
        let Some(setup) = &self.setup else {
            return false;
        };
        setup.change(|s| s.step_down_height = height);
        true
    }

    /// The setup's box around its physics (unscaled); an empty box at the origin without a setup.
    // D2-06622
    #[must_use]
    pub fn physics_bounding_box(&self) -> BBox {
        self.current_setup().map_or_else(BBox::default, |s| {
            let b = s.physics_bounds;
            BBox::new(vec3(b.min), vec3(b.max))
        })
    }

    /// The radius the setup gives around its physics (0 without a setup).
    // D2-06623
    #[must_use]
    pub fn physics_containment_radius(&self) -> f32 {
        self.current_setup().map_or(0.0, |s| s.physics_radius)
    }

    /// The setup's bounding sphere; a sphere of no size at the origin without a setup.
    // D2-06648, D2-06462
    #[must_use]
    pub fn bounding_sphere(&self) -> Sphere {
        self.current_setup()
            .map_or_else(Sphere::default, |s| s.bounding_sphere)
    }

    /// Works out whether a part's vertices are weighted to bones, and keeps it.
    // D2-06624
    pub fn cache_has_bone_weights(&mut self) -> bool {
        self.has_bone_weights = self
            .parts
            .iter()
            .flatten()
            .any(PhysicsPart::has_bone_weights);
        self.has_bone_weights
    }

    /// Works out whether a part has a physics tree, and keeps it.
    // D2-06629
    pub fn cache_has_physics_tree(&mut self) -> bool {
        self.has_physics_tree = self
            .parts
            .iter()
            .flatten()
            .any(PhysicsPart::has_physics_tree);
        self.has_physics_tree
    }

    /// Grows `out` to take in every part's box around what it draws: carried through the part's
    /// transform when the setup has no bones, as it is otherwise.
    // D2-06625
    pub fn grow_render_box(&self, out: &mut BBox) {
        let transformed = self.bone_count() == 0;
        for p in self.parts.iter().flatten() {
            p.grow_render_box(out, transformed);
        }
    }

    /// As [`PartArray::grow_render_box`], with the boxes the parts' vertices were read with.
    // D2-06627
    pub fn grow_default_render_box(&self, out: &mut BBox) {
        let transformed = self.bone_count() == 0;
        for p in self.parts.iter().flatten() {
            p.grow_default_render_box(out, transformed);
        }
    }

    /// The parts' physics trees, the object standing at `pos` scaled by `scale`, against the mover
    /// of `t`: the first part that does not answer OK decides. OK when no part has a tree (as last
    /// worked out).
    // D2-06630
    pub fn find_obj_collisions(
        &self,
        ctx: &TransitionCtx<'_>,
        t: &mut Transition,
        pos: &Position,
        scale: Vec3,
    ) -> TransitionState {
        if !self.has_physics_tree {
            return TransitionState::Ok;
        }
        for p in self.parts.iter().flatten() {
            let r = p.find_obj_collisions(ctx, t, pos, scale);
            if r != TransitionState::Ok {
                return r;
            }
        }
        TransitionState::Ok
    }

    /// A ray through every part's physics tree ([`PhysicsPart::ray_through_tree`]), the object
    /// standing at `pos`, `inv_scale` one over its scale. Every part the ray reaches multiplies
    /// `fraction` in turn, each against the whole ray (so two parts met at half and at a tenth of
    /// the way leave a twentieth); whether any was reached. Nothing is reached when no part has a
    /// tree (as last worked out).
    // D2-06631
    pub fn ray_through_trees(
        &self,
        pos: &Position,
        inv_scale: Vec3,
        ray: &Ray,
        ray_cell: CellId,
        fraction: &mut f32,
        normal: &mut Vec3,
    ) -> bool {
        if !self.has_physics_tree {
            return false;
        }
        let mut hit = false;
        for p in self.parts.iter().flatten() {
            if p.ray_through_tree(pos, inv_scale, ray, ray_cell, fraction, normal) {
                hit = true;
            }
        }
        hit
    }

    /// Each part's transform from the array's: a part not weighted to bones follows the bone of its
    /// own index (a part past the bones follows none), then `m`; a weighted one takes `m` as it is.
    // D2-06632
    pub fn refresh_matrices(&mut self, m: &Mat4) {
        let weighted = self.has_bone_weights;
        let bones = &self.bone_matrices;
        for (i, p) in self.parts.iter_mut().enumerate() {
            let Some(p) = p else { continue };
            let own = if weighted {
                *m
            } else {
                bones.get(i).copied().unwrap_or(Mat4::IDENTITY).then(m)
            };
            p.refresh_matrix(&own);
        }
    }

    /// A look's mesh swap on every part ([`PhysicsPart::swap_mesh`]).
    // D2-06633
    pub fn swap_meshes(&mut self, records: &dyn PartRecords, kind: u32, mesh: DataId) {
        for p in self.parts.iter_mut().flatten() {
            p.swap_mesh(records, kind, mesh);
        }
    }

    /// Every part shows its setup's mesh again.
    // D2-06634
    pub fn restore_meshes(&mut self, records: &dyn PartRecords) {
        let Some(setup) = self.current_setup() else {
            return;
        };
        for (i, p) in self.parts.iter_mut().enumerate() {
            if let Some(p) = p {
                let mesh = setup.meshes.get(i).copied().unwrap_or(DataId::NONE);
                p.set_mesh(records, mesh);
            }
        }
    }

    /// A material change of the look, for every part.
    // D2-06635
    pub fn add_material_change(&mut self, change: &MaterialAtom) -> bool {
        for p in self.parts.iter_mut().flatten() {
            p.materials.push(*change);
        }
        true
    }

    /// Every part drops the look's material changes.
    // D2-06636
    pub fn clear_material_changes(&mut self) {
        for p in self.parts.iter_mut().flatten() {
            p.materials.clear();
        }
    }

    /// The color's red, green and blue become `color`'s; its alpha (the opacity) stays.
    // D2-06560
    pub fn set_color(&mut self, color: Rgba) {
        let a = self.color.a;
        self.color = Rgba { a, ..color };
    }

    // D2-00091
    pub fn set_opacity(&mut self, opacity: f32) {
        self.color.a = opacity;
    }

    // D2-15478
    #[must_use]
    pub fn opacity(&self) -> f32 {
        self.color.a
    }

    /// Takes setup `id` in place of the one it has (nothing to do when it has that one already):
    /// its bones and parts go, a part is made for each of the new setup's meshes, whether one is
    /// weighted to bones is worked out, and the bones are made and placed in the setup's pose 0.
    /// False, changing nothing, when the setup is not in the data; false, keeping the new setup
    /// and the parts made, when a mesh is not.
    // D2-06647
    pub fn set_setup(&mut self, records: &dyn PartRecords, id: DataId) -> bool {
        if self.setup.is_some() && self.setup_id == id {
            return true;
        }
        let Some(setup) = records.loaded_setup(id) else {
            return false;
        };
        self.destroy_bones();
        self.destroy_parts();
        self.placement = 0;
        self.setup = Some(setup);
        self.setup_id = id;
        if !self.init_parts(records) {
            return false;
        }
        self.cache_has_bone_weights();
        self.init_bones()
    }

    /// Takes setup `setup` and animation map `anim_map` ([`PartArray::set_setup`],
    /// [`PartArray::set_anim_map`]).
    // D2-06655
    pub fn set_setup_and_map(
        &mut self,
        records: &dyn PartRecords,
        setup: DataId,
        anim_map: DataId,
    ) -> bool {
        self.set_setup(records, setup) && self.set_anim_map(records, anim_map)
    }

    /// The bones as points ([`PartArray::bone_points`]): each bone's origin through its matrix,
    /// and its parent. Only for a setup whose parts are weighted to bones; false otherwise, with
    /// the points kept from before.
    // D2-06657
    pub fn work_out_bone_points(&mut self) -> bool {
        let Some(setup) = self.current_setup() else {
            return false;
        };
        if !self.has_bone_weights {
            return false;
        }
        self.bone_points = self
            .bone_matrices
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let r = &m.0;
                let w = 1.0 / r[3][3];
                BonePoint {
                    origin: Vec3::new(r[3][0] * w, r[3][1] * w, r[3][2] * w),
                    parent: parent_index(&setup, i),
                }
            })
            .collect();
        true
    }

    /// The bone nearest `frame`'s origin (by the bones' origins, the first of equals) in `bone`,
    /// and in `offset` the turn from that bone's rotation to `frame`'s, at the bone's origin. False
    /// for a setup without bones; with bones, `offset` is at the origin whatever is found.
    // D2-06641
    pub fn nearest_bone(&self, frame: &Frame, bone: &mut u32, offset: &mut Frame) -> bool {
        if self.bone_count() == 0 {
            return false;
        }
        let mut best = f32::MAX;
        for (i, m) in self.bone_matrices.iter().enumerate() {
            let at = frame_of(m);
            let (dx, dy, dz) = (
                at.origin.x - frame.origin.x,
                at.origin.y - frame.origin.y,
                at.origin.z - frame.origin.z,
            );
            let d = (dx * dx + dz * dz) + dy * dy;
            if d < best {
                *bone = u32::try_from(i).unwrap_or(u32::MAX);
                *offset = relative(frame, &at);
                best = d;
            }
        }
        offset.origin = Vec3::ZERO;
        true
    }

    /// Back to an array of nothing: the parts drop the look's material changes and go, with the
    /// animation map, the bones and the setup; it has no owner and is drawn white again.
    // D2-06671
    pub fn destroy(&mut self) {
        self.clear_material_changes();
        self.anim_map = None;
        self.destroy_bones();
        self.destroy_parts();
        self.placement = 0;
        self.setup = None;
        self.setup_id = DataId::NONE;
        self.has_physics_tree = false;
        self.has_bone_weights = false;
        self.set_owner(NO_OWNER);
        self.color = WHITE;
    }
}
