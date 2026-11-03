//! What the body is drawn and collides as: the part group its look puts together, the scale its
//! look gives it and the scale its look gives what it holds, its own scale, and its whole scale.
//!
//! With a part group the body's step heights are its setup's, shared with every object made from
//! that setup (a change to one is a change to all), and it collides as its parts do: through their
//! physics trees when any part has one, else its setup's upright cylinders, else its spheres.

use auberean_primitives::Sphere;
use dereth_primitives::{CellId, Position, Vec3};

use super::{flags, Animate, Body, Notice};
use crate::parts::PartGroup;
use auberean_assets::setup::Cylinder;

impl Body {
    /// Takes the part group its look puts together, working out whether a part of it has a
    /// physics tree.
    pub fn set_parts(&mut self, mut parts: PartGroup) {
        let tree = parts.cache_has_physics_tree();
        self.put(flags::HAS_PHYSICS_TREE, tree);
        self.parts = Some(Box::new(parts));
    }

    /// What making it ends with, once it has its part group: whether a part has a physics tree,
    /// and the slowest impacts it reports (along the ground: its velocity scale times its standard
    /// forward speed, plus 0.1; up and down: its standard jump impulse times its jump scale, plus
    /// 0.1). False, and nothing done, without a part group. (Its look's startup effects are the
    /// host's to begin, and the world it is put in sizes it.)
    // D2-06116
    pub fn init_object_end(&mut self) -> bool {
        let Some(parts) = self.parts.as_deref_mut() else {
            return false;
        };
        let tree = parts.cache_has_physics_tree();
        self.put(flags::HAS_PHYSICS_TREE, tree);
        let standard = self.standard_motion_values();
        self.min_impact = (
            self.minterp.velocity_scale * standard.max_forward + 0.1,
            standard.jump_impulse * self.jump_scale + 0.1,
        );
        true
    }

    /// Brings its part group up to date with a changed look, the root hanging from nothing; false
    /// without a part group or when the look's groups cannot be put together.
    pub fn update_parts_to_look(
        &mut self,
        records: &dyn crate::parts::PartRecords,
        look: &auberean_world::look::Look,
    ) -> bool {
        let Some(parts) = self.parts.as_deref_mut() else {
            return false;
        };
        parts.update_to_look(records, None, look)
    }

    /// Its part group, once it has one.
    // D2-05840
    #[must_use]
    pub fn part_group(&self) -> Option<&PartGroup> {
        self.parts.as_deref()
    }

    /// Places every part group's bones in their setups' pose `id` (pose 0 for a setup with none of
    /// that kind), their matrices then to be worked out again, and asks its figure to stand in
    /// that pose; false without a part group or when a setup has no pose to take.
    // D2-06143, D2-06473
    pub fn set_placement_frame(&mut self, id: u32) -> bool {
        let Some(parts) = self.parts.as_deref_mut() else {
            return false;
        };
        if !parts.set_placement_frame(id) {
            return false;
        }
        parts.update_bones();
        self.notices
            .push(Notice::Animate(Animate::PlacementFrame(id)));
        true
    }

    /// Its whole scale: its look's, times its own, times, while something holds it and it is not
    /// flying as a missile, `holder`: the holder's whole scale and the scale the holder's look
    /// gives what it holds.
    // D2-06149, D2-06188
    #[must_use]
    pub fn total_scale(&self, holder: Option<(Vec3, f32)>) -> Vec3 {
        let (l, o) = (self.look_scale, self.object_scale);
        let mut s = Vec3::new(l.x * o.x, l.y * o.y, l.z * o.z);
        if let Some((h, child)) = holder.filter(|_| self.has_parent && self.missile.is_none()) {
            s = Vec3::new(s.x * h.x * child, s.y * h.y * child, child * s.z * h.z);
        }
        s
    }

    /// Its setup's upright cylinders (none without a part group).
    // D2-05866, D2-05867
    #[must_use]
    pub fn cylinders(&self) -> Vec<Cylinder> {
        self.parts
            .as_deref()
            .map_or_else(Vec::new, |g| g.array.cylinders())
    }

    /// Whether its setup lets it turn freely; it may without a part group.
    // D2-05873
    #[must_use]
    pub fn allows_free_heading(&self) -> bool {
        self.parts
            .as_deref()
            .is_none_or(|g| g.array.allows_free_heading())
    }

    /// The sphere it is sorted by: its setup's bounding sphere; a sphere of no size at its origin
    /// without a part group.
    // D2-05874
    #[must_use]
    pub fn sorting_sphere(&self) -> Sphere {
        self.parts
            .as_deref()
            .map_or_else(Sphere::default, |g| g.array.bounding_sphere())
    }

    /// Its setup's selection sphere scaled by its look's scale and then by its own, each along its
    /// axis (the radius by their heights); none without a part group or a setup.
    // D2-05876, D2-06463
    #[must_use]
    pub fn selection_sphere(&self) -> Option<Sphere> {
        let s = self.parts.as_deref()?.array.selection_sphere()?;
        let by = |s: Sphere, k: Vec3| Sphere {
            center: auberean_primitives::Vec3::new(
                s.center.x * k.x,
                s.center.y * k.y,
                s.center.z * k.z,
            ),
            radius: s.radius * k.z,
        };
        Some(by(by(s, self.look_scale), self.object_scale))
    }

    /// The object at the top of what holds it: what holds it, else itself.
    #[must_use]
    pub const fn top_level_object(&self) -> u64 {
        if self.has_parent {
            self.parent
        } else {
            self.id
        }
    }

    /// Where it stands for its cell: its first collision sphere's center carried into its
    /// landblock (its own place when it has no sphere), turned as it is.
    #[must_use]
    pub fn cell_position(&self) -> Position {
        let mut p = self.position;
        if let Some(s) = self.spheres().first() {
            p.frame.origin = dereth_physics::math::localtoglobal(&self.position.frame, s.center);
        }
        p
    }

    /// Its cell becomes `cell` (handed to its part group too, which keeps nothing of it).
    pub fn set_cell_id(&mut self, cell: CellId) {
        self.position.cell = cell;
        if let Some(g) = self.parts.as_deref_mut() {
            g.set_cell(cell);
        }
    }
}
