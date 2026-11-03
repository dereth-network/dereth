//! A part group: a part array with its key, the connection point of its parent's setup it hangs
//! from, and the groups hung from it, put together from an object's look and brought up to date
//! when the look changes.
//!
//! A group's frame is its connection point's transform, followed by the parent bone it is on
//! (when the parent has that bone), followed by its parent's frame; the root's is the object's.
//! Asked of the whole tree, a question goes to a group and then to the groups hung from it in the
//! order they were hung, each with the groups under it before the next.

use auberean_assets::setup::Location;
use auberean_primitives::DataId;
use auberean_world::look::{Atoms, Group, Look, LookRecords};
use auberean_world::math::Mat4;
use dereth_physics::transition::{Transition, TransitionCtx, TransitionState};
use dereth_primitives::{CellId, Position, Vec3};

use super::{connection_point, PartArray, PartRecords};
use crate::geom::{BBox, BoxOps, Ray};

/// The bone index of a group that hangs from no bone.
const NO_BONE: u32 = u32::MAX;

/// A part group.
// D2-06561, D2-06564
#[derive(Clone, Debug)]
pub struct PartGroup {
    pub array: PartArray,
    /// Its key in the look.
    pub key: u32,
    /// Whether it hangs from a parent group.
    pub parented: bool,
    /// The connection point it hangs from (or would, as the root), the parent's bone that point is
    /// on, and the point's transform.
    pub connection: u32,
    pub parent_bone: u32,
    pub parent_offset: Mat4,
    /// The groups hung from it, in the order they were hung.
    pub children: Vec<PartGroup>,
    /// Its parts' transforms are to be worked out again ([`PartGroup::refresh_if_needed`]).
    pub needs_refresh: bool,
}

impl Default for PartGroup {
    /// A group of nothing, hung from nothing, its transforms to be worked out.
    // D2-06559
    fn default() -> Self {
        Self {
            array: PartArray::default(),
            key: 0,
            parented: false,
            connection: 0,
            parent_bone: NO_BONE,
            parent_offset: Mat4::IDENTITY,
            children: Vec::new(),
            needs_refresh: true,
        }
    }
}

impl PartGroup {
    /// Takes setup `id` ([`PartArray::set_setup`]) and works out the box around what it draws.
    pub fn init_setup(&mut self, records: &dyn PartRecords, id: DataId) -> bool {
        self.array.set_setup(records, id) && self.init_bounds()
    }

    /// Hangs from `location` of `parent`'s setup: the location's kind, its bone, and its transform
    /// with its scale. False without a setup of its own or of the parent's, or when the location's
    /// bone is past the parent setup's bones.
    // D2-06556
    pub fn hang_from_location(&mut self, parent: &PartArray, location: &Location) -> bool {
        if self.array.setup.is_none() {
            return false;
        }
        let Some(ps) = parent.current_setup() else {
            return false;
        };
        if location.bone as usize >= ps.bones.len() {
            return false;
        }
        self.connection = location.kind;
        self.parent_bone = location.bone;
        let t = &location.transform;
        self.parent_offset = Mat4::from_parts(t.offset, t.rotation, t.scale);
        true
    }

    /// A ray through the group's own parts' physics trees, the object standing at `pos` scaled by
    /// `scale` ([`PartArray::ray_through_trees`]); the groups hung from it are not
    /// tried.
    // D2-06557
    pub fn ray_through_trees(
        &self,
        pos: &Position,
        scale: Vec3,
        ray: &Ray,
        ray_cell: CellId,
        fraction: &mut f32,
        normal: &mut Vec3,
    ) -> bool {
        let inv = Vec3::new(1.0 / scale.x, 1.0 / scale.y, 1.0 / scale.z);
        self.array
            .ray_through_trees(pos, inv, ray, ray_cell, fraction, normal)
    }

    /// Bone `i`'s transform into the group's frame.
    #[must_use]
    pub fn bone_matrix(&self, i: usize) -> Option<&Mat4> {
        self.array.bone_matrices.get(i)
    }

    /// Gives every group of the tree, and every part, an owner.
    // D2-06562
    pub fn set_owner(&mut self, owner: u64) {
        self.array.set_owner(owner);
        for c in &mut self.children {
            c.set_owner(owner);
        }
    }

    /// Takes down the group hung from it with key `key`, which then hangs from nothing, from no
    /// bone, at no offset; none when no group of that key hangs from it.
    // D2-06565
    pub fn remove_child(&mut self, key: u32) -> Option<PartGroup> {
        let i = self.children.iter().position(|c| c.key == key)?;
        let mut c = self.children.remove(i);
        c.parented = false;
        c.parent_bone = NO_BONE;
        c.parent_offset = Mat4::IDENTITY;
        Some(c)
    }

    /// The group of key `key`: this one, or the first found among those hung from it.
    // D2-06566, D2-06452
    #[must_use]
    pub fn part_group(&self, key: u32) -> Option<&PartGroup> {
        if self.key == key {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.part_group(key))
    }

    /// As [`PartGroup::part_group`], to change it.
    pub fn part_group_mut(&mut self, key: u32) -> Option<&mut PartGroup> {
        if self.key == key {
            return Some(self);
        }
        self.children.iter_mut().find_map(|c| c.part_group_mut(key))
    }

    /// Counts one for this group and each group under it. Asked for the time a change of mode
    /// takes, the tree answers nothing else.
    // D2-06567
    pub fn count_groups(&self, count: &mut i32) {
        for c in &self.children {
            c.count_groups(count);
        }
        *count += 1;
    }

    /// Hangs from `parent`'s connection point of kind `connection` ([`PartGroup::hang_from_location`]);
    /// false when the parent's setup has none of that kind.
    // D2-06571
    pub fn hang_from_connection(&mut self, parent: &PartArray, connection: u32) -> bool {
        let Some(location) = parent
            .current_setup()
            .and_then(|s| connection_point(&s, connection).copied())
        else {
            return false;
        };
        self.hang_from_location(parent, &location)
    }

    /// Takes down and lets go of every group hung from it.
    // D2-06572
    pub fn destroy_children(&mut self) {
        self.children.clear();
    }

    /// The cell the object is in reaches no part: nothing changes, here or below.
    // D2-06573, D2-06470
    #[allow(clippy::only_used_in_recursion)] // every group passes it on and none keeps it
    pub fn set_cell(&mut self, cell: CellId) {
        for c in &mut self.children {
            c.set_cell(cell);
        }
    }

    /// Every group's bone matrices from their transforms ([`PartArray::update_bones`]); the parts'
    /// transforms are then to be worked out again.
    // D2-06574
    pub fn update_bones(&mut self) {
        self.array.update_bones();
        for c in &mut self.children {
            c.update_bones();
        }
        self.needs_refresh = true;
    }

    /// The nearest place a ray meets what is drawn, over this group and those under it: `through`
    /// says where the ray meets one array's drawn triangles (the fraction of its length and the
    /// kind of mesh met). The least fraction under 100,000 wins, the first of equals; none when
    /// nothing is met.
    // D2-06575
    pub fn trace_ray(
        &self,
        through: &mut dyn FnMut(&PartArray) -> Option<(f32, u32)>,
    ) -> Option<(f32, u32)> {
        let mut best: Option<(f32, u32)> = None;
        let mut take = |hit: Option<(f32, u32)>| {
            if let Some((t, kind)) = hit {
                if t < best.map_or(100_000.0, |b| b.0) {
                    best = Some((t, kind));
                }
            }
        };
        take(through(&self.array));
        for c in &self.children {
            take(c.trace_ray(through));
        }
        best
    }

    /// Every group's bones placed in its setup's pose `id` ([`PartArray::set_placement_frame`]);
    /// false when any group's setup could not be.
    // D2-06578
    pub fn set_placement_frame(&mut self, id: u32) -> bool {
        let mut ok = self.array.set_placement_frame(id);
        for c in &mut self.children {
            if !c.set_placement_frame(id) {
                ok = false;
            }
        }
        ok
    }

    /// Grows `out` to take in what this group and every group under it draws.
    // D2-06579
    pub fn grow_render_box(&self, out: &mut BBox) {
        self.array.grow_render_box(out);
        for c in &self.children {
            c.grow_render_box(out);
        }
    }

    /// As [`PartGroup::grow_render_box`], with the boxes the vertices were read with.
    // D2-06580
    pub fn grow_default_render_box(&self, out: &mut BBox) {
        self.array.grow_default_render_box(out);
        for c in &self.children {
            c.grow_default_render_box(out);
        }
    }

    /// Works out again the box around what this group and those under it draw, and keeps it.
    // D2-06626
    pub fn cache_render_box(&mut self) {
        let mut b = BBox::for_adjustment();
        self.grow_render_box(&mut b);
        self.array.render_bounds = b;
    }

    /// As [`PartGroup::cache_render_box`], with the boxes the vertices were read with.
    // D2-06628
    pub fn cache_default_render_box(&mut self) {
        let mut b = BBox::for_adjustment();
        self.grow_default_render_box(&mut b);
        self.array.default_render_bounds = b;
    }

    /// Works out the box around what it draws ([`PartGroup::cache_render_box`]).
    // D2-06642
    pub fn init_bounds(&mut self) -> bool {
        self.cache_render_box();
        true
    }

    /// Works out for every group whether a part has a physics tree; whether any has.
    // D2-06581
    pub fn cache_has_physics_tree(&mut self) -> bool {
        let mut any = self.array.cache_has_physics_tree();
        for c in &mut self.children {
            if c.cache_has_physics_tree() {
                any = true;
            }
        }
        any
    }

    /// Every group's parts' physics trees against the mover of `t` ([`PartArray::find_obj_collisions`]),
    /// each tested in the object's frame at `pos`: the first answer that is not OK decides.
    // D2-06582, D2-06469
    pub fn find_obj_collisions(
        &self,
        ctx: &TransitionCtx<'_>,
        t: &mut Transition,
        pos: &Position,
        scale: Vec3,
    ) -> TransitionState {
        let r = self.array.find_obj_collisions(ctx, t, pos, scale);
        if r != TransitionState::Ok {
            return r;
        }
        for c in &self.children {
            let r = c.find_obj_collisions(ctx, t, pos, scale);
            if r != TransitionState::Ok {
                return r;
            }
        }
        TransitionState::Ok
    }

    /// What the look does to the group's parts: each shows its setup's mesh again and drops its
    /// material changes; then the look's mesh swaps apply in order, its opacities (the last one
    /// stays), and each of its material changes whose modifier and template are in the data (or
    /// not named).
    // D2-06589
    pub fn apply_atoms(&mut self, records: &dyn PartRecords, atoms: &Atoms) -> bool {
        self.array.restore_meshes(records);
        self.array.clear_material_changes();
        for m in &atoms.meshes {
            self.array.swap_meshes(records, m.mesh_kind, m.mesh);
        }
        for o in &atoms.opacities {
            self.array.set_opacity(o.opacity);
        }
        for a in &atoms.materials {
            if !a.modifier.is_none() && !records.has_material(a.modifier) {
                continue;
            }
            if !a.template.is_none() && !records.has_material(a.template) {
                continue;
            }
            self.array.add_material_change(a);
        }
        true
    }

    /// The look's changes for this group's key ([`Look::atoms`]) applied ([`PartGroup::apply_atoms`]);
    /// false when a table they name is missing.
    // D2-06595
    pub fn apply_look(&mut self, records: &dyn PartRecords, look: &Look) -> bool {
        let Some(atoms) = look.atoms(records as &dyn LookRecords, self.key) else {
            return false;
        };
        self.apply_atoms(records, &atoms)
    }

    /// Becomes the group `desc` describes, without the groups hung from it: its key, setup and
    /// animation map, its box, and no children.
    // D2-06590
    pub fn remake_from(&mut self, records: &dyn PartRecords, desc: &Group) -> bool {
        self.key = desc.key;
        if !self
            .array
            .set_setup_and_map(records, desc.setup, desc.anim_map)
        {
            return false;
        }
        if !self.init_bounds() {
            return false;
        }
        self.destroy_children();
        true
    }

    /// Hangs from connection point `connection` again: of `parent`'s setup when it hangs from a
    /// parent ([`PartGroup::hang_from_connection`]); as the root, from no bone at no offset,
    /// keeping the point's kind.
    // D2-06591
    pub fn rehang(&mut self, parent: Option<&PartArray>, connection: u32) -> bool {
        match parent {
            None => {
                self.connection = connection;
                self.parent_bone = NO_BONE;
                self.parent_offset = Mat4::IDENTITY;
                true
            }
            Some(p) => self.hang_from_connection(p, connection),
        }
    }

    /// Works out the parts' transforms again when they are to be, the object's frame being the
    /// root's.
    // D2-06592
    pub fn refresh_if_needed(&mut self, parent: Option<&PartArray>) {
        if self.needs_refresh {
            self.refresh_matrices(parent, &Mat4::IDENTITY);
            self.needs_refresh = false;
        }
    }

    /// The parts' transforms of this group and those under it, `m` being the frame of the group it
    /// hangs from (`parent`): the group's frame is its connection point's transform, followed by
    /// the parent's bone when the parent has that bone, followed by `m`.
    // D2-06577
    pub fn refresh_matrices(&mut self, parent: Option<&PartArray>, m: &Mat4) {
        let local = match parent {
            Some(p) if (self.parent_bone as usize) < p.bone_count() => self
                .parent_offset
                .then(&p.bone_matrices[self.parent_bone as usize]),
            _ => self.parent_offset,
        };
        let here = local.then(m);
        self.array.refresh_matrices(&here);
        let (array, children) = (&self.array, &mut self.children);
        for c in children {
            c.refresh_matrices(Some(array), &here);
        }
    }

    /// Hangs `child` from connection point `connection` of this group's setup, after the groups
    /// already hung; gives it back when it already hangs from a group or the setup has no such
    /// point.
    // D2-06593
    pub fn add_child(
        &mut self,
        mut child: Box<PartGroup>,
        connection: u32,
    ) -> Result<(), Box<PartGroup>> {
        if child.parented || !child.hang_from_connection(&self.array, connection) {
            return Err(child);
        }
        child.parented = true;
        self.children.push(*child);
        Ok(())
    }

    /// Takes down and lets go of each group hung from it whose key the look no longer has; false,
    /// stopping there, at a group of key 0, or at one of its own key (which it cannot let go of).
    // D2-06594
    pub fn drop_children_gone_from(&mut self, look: &Look) -> bool {
        let gone: Vec<u32> = self
            .children
            .iter()
            .map(|c| c.key)
            .filter(|k| look.groups.get(*k).is_none())
            .collect();
        for key in gone {
            if key == 0 || key == self.key {
                return false;
            }
            if self.remove_child(key).is_none() {
                return false;
            }
        }
        true
    }

    /// Becomes the group `desc` describes, hung from `parent` when it has one, without the groups
    /// hung from it: its setup, animation map and box, its key, its connection to the parent, and
    /// what the look does to its parts. The caller hangs it from the parent.
    // D2-06596
    fn init_alone(
        &mut self,
        records: &dyn PartRecords,
        parent: Option<&PartArray>,
        desc: &Group,
        look: &Look,
    ) -> bool {
        if !self
            .array
            .set_setup_and_map(records, desc.setup, desc.anim_map)
        {
            return false;
        }
        if !self.init_bounds() {
            return false;
        }
        self.destroy_children();
        self.key = desc.key;
        if let Some(p) = parent {
            if self.parented || !self.hang_from_connection(p, desc.connection) {
                return false;
            }
            self.parented = true;
        }
        self.apply_look(records, look)
    }

    /// Becomes the group `desc` describes with every group the look hangs under it, each hung from
    /// its connection point, then works out the box around what they all draw. False when any of
    /// them cannot be put together (the groups put together before it stay hung).
    // D2-06597
    pub fn init(
        &mut self,
        records: &dyn PartRecords,
        parent: Option<&PartArray>,
        desc: &Group,
        look: &Look,
    ) -> bool {
        if !self.init_alone(records, parent, desc, look) {
            return false;
        }
        for key in &desc.children {
            let Some(child_desc) = look.groups.get(*key) else {
                return false;
            };
            let mut child = PartGroup::default();
            if !child.init(records, Some(&self.array), child_desc, look) {
                return false;
            }
            self.children.push(child);
        }
        self.cache_render_box();
        true
    }

    /// The tree a look describes, from its root ([`PartGroup::init`]); false for a look without one.
    // D2-06598
    pub fn init_look(&mut self, records: &dyn PartRecords, look: &Look) -> bool {
        match look.root_group() {
            Some(root) => self.init(records, None, root, look),
            None => false,
        }
    }

    /// Brings the tree up to date with a changed look. The group of this one's key in the look
    /// (when it has none, the whole look is first put together over this group from its root, and
    /// the key it then has is looked up): a different setup or animation map makes the group anew
    /// without its children; it hangs from its connection point again and takes the look's changes;
    /// each group the look hangs under it is brought up to date (made first when the tree has no
    /// group of its key); and the groups hung from it that the look no longer has go.
    // D2-06599
    pub fn update_to_look(
        &mut self,
        records: &dyn PartRecords,
        parent: Option<&PartArray>,
        look: &Look,
    ) -> bool {
        let mut desc = look.groups.get(self.key);
        if desc.is_none() {
            if let Some(root) = look.root_group() {
                self.init(records, None, root, look);
            }
            desc = look.groups.get(self.key);
        }
        let Some(desc) = desc else {
            return false;
        };
        if desc.key != 0 {
            let same = self.key == desc.key
                && self.array.setup.is_some()
                && self.array.setup_id == desc.setup
                && self.array.anim_map_id() == desc.anim_map;
            if !same {
                self.remake_from(records, desc);
            }
        }
        self.rehang(parent, desc.connection);
        self.apply_look(records, look);
        if !self.update_children_to_look(records, parent, look, desc) {
            return false;
        }
        self.drop_children_gone_from(look)
    }

    /// Each group the look hangs under `desc`: brought up to date where the tree has a group of its
    /// key (this one, or the first found under it), else made, hung from this group and brought up
    /// to date. False at a group the look names but does not have, or one that cannot be made.
    // D2-06600
    fn update_children_to_look(
        &mut self,
        records: &dyn PartRecords,
        parent: Option<&PartArray>,
        look: &Look,
        desc: &Group,
    ) -> bool {
        for key in &desc.children {
            let Some(child_desc) = look.groups.get(*key) else {
                return false;
            };
            if let Some(r) = self.update_in_tree(records, parent, look, child_desc.key) {
                if !r {
                    return false;
                }
                continue;
            }
            let mut child = PartGroup::default();
            if !child.init_alone(records, Some(&self.array), child_desc, look) {
                return false;
            }
            self.children.push(child);
            let (array, children) = (&self.array, &mut self.children);
            let Some(child) = children.last_mut() else {
                return false;
            };
            if !child.update_to_look(records, Some(array), look) {
                return false;
            }
        }
        true
    }

    /// Brings up to date the group of key `key` in this tree (this one first, then those hung from
    /// it in order), with the group it hangs from; none when the tree has no such group.
    fn update_in_tree(
        &mut self,
        records: &dyn PartRecords,
        parent: Option<&PartArray>,
        look: &Look,
        key: u32,
    ) -> Option<bool> {
        if self.key == key {
            return Some(self.update_to_look(records, parent, look));
        }
        let (array, children) = (&self.array, &mut self.children);
        for c in children {
            if let Some(r) = c.update_in_tree(records, Some(array), look, key) {
                return Some(r);
            }
        }
        None
    }
}
