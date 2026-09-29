//! The sphere path — the swept path.
//!
//! Transcribed against the client's own sphere-path code.
//!
//! Three copies of the same one or two spheres live here, in different spaces: object space
//! (`local_sphere`), the check position's block space (`global_sphere`) and some other object's
//! local space (`localspace_sphere`). Keeping them straight is most of the work.

use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::geom::polygon::Polygon;
use crate::geom::sphere::Sphere;
use crate::globals::LANDING_Z;
use crate::landdefs;
use crate::math::{self, V3};

/// Sphere-path insertion type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InsertType {
    #[default]
    Transition = 0,
    Placement = 1,
    InitialPlacement = 2,
}

/// A sphere path.
#[derive(Debug, Clone)]
pub struct SpherePath {
    // ---- spheres -------------------------------------------------------------------------
    /// 1 or 2. The sphere initialisation clamps to 2.
    pub num_sphere: usize,
    /// The object's spheres scaled by `scale`, in object space.
    pub local_sphere: [Sphere; 2],
    /// `local_sphere[0].center - (0, 0, radius)`.
    pub local_low_point: Vec3,
    /// The same spheres transformed by `check_pos.frame`.
    pub global_sphere: [Sphere; 2],
    pub global_low_point: Vec3,
    /// The spheres transformed into some *other* object's local space.
    pub localspace_sphere: [Sphere; 2],
    pub localspace_low_point: Vec3,
    pub localspace_curr_center: [Vec3; 2],
    pub localspace_pos: Position,
    /// The third row of the target frame's `l2g`: the local direction of world "up".
    pub localspace_z: Vec3,
    /// The sphere centres at `curr_pos`.
    pub global_curr_center: [Vec3; 2],

    // ---- positions -----------------------------------------------------------------------
    pub begin_cell: Option<CellId>,
    /// `None` means "placement, no path".
    pub begin_pos: Option<Position>,
    pub end_pos: Position,
    pub curr_cell: Option<CellId>,
    pub curr_pos: Position,
    pub check_cell: Option<CellId>,
    pub check_pos: Position,
    pub backup_cell: Option<CellId>,
    pub backup_check_pos: Position,
    /// The offset applied for the current sub-step.
    pub global_offset: Vec3,
    /// Cleared whenever `check_pos` moves; forces the cell list to be rebuilt.
    pub cell_array_valid: bool,

    // ---- step up / step down ---------------------------------------------------------------
    pub step_up: bool,
    pub step_up_normal: Vec3,
    pub step_down: bool,
    pub step_down_amt: f32,
    /// Fraction of the step-down still available; starts at `1.0`. Each surface that eats part of
    /// the probe reduces it; going below `-0.1` or not decreasing aborts.
    pub walk_interp: f32,
    /// The polygon that was found walkable, plus what is needed to re-test it.
    pub walkable: Option<Polygon>,
    pub walkable_check_pos: Sphere,
    pub walkable_up: Vec3,
    pub walkable_pos: Position,
    pub walkable_scale: f32,
    /// Minimum `N.z` for a surface to count during this probe. `LANDING_Z` by default,
    /// `get_walkable_z()` when already on a walkable surface.
    pub walkable_allowance: f32,
    /// We are inside; collisions become hard failures.
    pub check_walkable: bool,
    /// Step-up was called: a landing happened and the step must be
    /// re-validated.
    pub collide: bool,

    // ---- flags -----------------------------------------------------------------------------
    pub insert_type: InsertType,
    /// Saved `insert_type` while a nested probe temporarily changes it.
    pub backup: InsertType,
    /// Set per candidate object: this pair passes through each other.
    pub obstruction_ethereal: bool,
    pub hits_interior_cell: bool,
    /// Inside building-collision search.
    pub bldg_check: bool,
    /// Default `1`; cleared when `SetPositionFlags::SLIDE` is absent.
    pub placement_allows_sliding: bool,
    pub neg_step_up: bool,
    pub neg_poly_hit: bool,
    pub neg_collision_normal: Vec3,
}

impl Default for SpherePath {
    fn default() -> Self {
        let zero_pos = Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY));
        Self {
            num_sphere: 0,
            local_sphere: [Sphere::default(); 2],
            local_low_point: Vec3::ZERO,
            global_sphere: [Sphere::default(); 2],
            global_low_point: Vec3::ZERO,
            localspace_sphere: [Sphere::default(); 2],
            localspace_low_point: Vec3::ZERO,
            localspace_curr_center: [Vec3::ZERO; 2],
            localspace_pos: zero_pos,
            localspace_z: Vec3::new(0.0, 0.0, 1.0),
            global_curr_center: [Vec3::ZERO; 2],
            begin_cell: None,
            begin_pos: None,
            end_pos: zero_pos,
            curr_cell: None,
            curr_pos: zero_pos,
            check_cell: None,
            check_pos: zero_pos,
            backup_cell: None,
            backup_check_pos: zero_pos,
            global_offset: Vec3::ZERO,
            cell_array_valid: false,
            step_up: false,
            step_up_normal: Vec3::ZERO,
            step_down: false,
            step_down_amt: 0.0,
            walk_interp: 1.0,
            walkable: None,
            walkable_check_pos: Sphere::default(),
            walkable_up: Vec3::new(0.0, 0.0, 1.0),
            walkable_pos: zero_pos,
            walkable_scale: 1.0,
            walkable_allowance: LANDING_Z,
            check_walkable: false,
            collide: false,
            insert_type: InsertType::Transition,
            backup: InsertType::Transition,
            obstruction_ethereal: false,
            hits_interior_cell: false,
            bldg_check: false,
            placement_allows_sliding: true,
            neg_step_up: false,
            neg_poly_hit: false,
            neg_collision_normal: Vec3::ZERO,
        }
    }
}

impl SpherePath {
    /// Initialise the sphere path.
    pub fn init(&mut self) {
        *self = Self::default();
    }

    /// Store at most two spheres, scaling **centre and
    /// radius** by `scale`, then derive the low point from sphere 0.
    pub fn init_sphere(&mut self, spheres: &[Sphere], scale: f32) {
        self.num_sphere = spheres.len().min(2);
        for (dst, src) in self
            .local_sphere
            .iter_mut()
            .zip(spheres.iter())
            .take(self.num_sphere)
        {
            *dst = Sphere::new(src.center.mul(scale), src.radius * scale);
        }
        let s = self.local_sphere[0];
        self.local_low_point = Vec3::new(s.center.x, s.center.y, s.center.z - s.radius);
    }

    /// `begin_pos == None` means placement.
    pub fn init_path(&mut self, cell: Option<CellId>, begin: Option<Position>, end: &Position) {
        self.begin_cell = cell;
        self.begin_pos = begin;
        self.end_pos = *end;
        self.curr_pos = begin.unwrap_or(*end);
        self.curr_cell = cell;
        self.cache_global_curr_center();
        self.insert_type = if begin.is_some() {
            InsertType::Transition
        } else {
            InsertType::Placement
        };
    }

    /// Cache the global sphere.
    ///
    /// With an offset it **translates** the cached spheres rather than re-transforming them,
    /// which is both faster and, more importantly, not the same float arithmetic.
    pub fn cache_global_sphere(&mut self, offset: Option<Vec3>) {
        if let Some(v) = offset {
            for i in 0..self.num_sphere {
                self.global_sphere[i].center = self.global_sphere[i].center.add(v);
            }
            self.global_low_point = self.global_low_point.add(v);
            return;
        }
        let m = math::l2g(self.check_pos.frame.rotation);
        for i in 0..self.num_sphere {
            self.global_sphere[i].radius = self.local_sphere[i].radius;
            self.global_sphere[i].center = math::localtoglobalvec(m, self.local_sphere[i].center)
                .add(self.check_pos.frame.origin);
        }
        self.global_low_point =
            math::localtoglobalvec(m, self.local_low_point).add(self.check_pos.frame.origin);
    }

    /// Cache the global current centre.
    pub fn cache_global_curr_center(&mut self) {
        let m = math::l2g(self.curr_pos.frame.rotation);
        for i in 0..self.num_sphere {
            self.global_curr_center[i] = math::localtoglobalvec(m, self.local_sphere[i].center)
                .add(self.curr_pos.frame.origin);
        }
    }

    /// Transform the spheres into another
    /// frame's local space, scaling by `1 / scale` so the geometry can be tested unscaled.
    pub fn cache_localspace_sphere(&mut self, pos: &Position, scale: f32) {
        self.localspace_pos = *pos;
        self.walkable_scale = scale;
        let inv = 1.0 / scale;
        let m = math::l2g(pos.frame.rotation);
        for i in 0..self.num_sphere {
            let global = self.global_sphere[i].center;
            let off = landdefs::get_block_offset(pos.cell, self.check_pos.cell);
            let local = math::globaltolocalvec(m, global.add(off).sub(pos.frame.origin));
            self.localspace_sphere[i] =
                Sphere::new(local.mul(inv), self.global_sphere[i].radius * inv);
            let cc = self.global_curr_center[i];
            let off2 = landdefs::get_block_offset(pos.cell, self.curr_pos.cell);
            self.localspace_curr_center[i] =
                math::globaltolocalvec(m, cc.add(off2).sub(pos.frame.origin)).mul(inv);
        }
        let off = landdefs::get_block_offset(pos.cell, self.check_pos.cell);
        self.localspace_low_point =
            math::globaltolocalvec(m, self.global_low_point.add(off).sub(pos.frame.origin))
                .mul(inv);
        // The third row of the frame's l2g: the local direction of world "up".
        self.localspace_z = math::globaltolocalvec(m, Vec3::new(0.0, 0.0, 1.0));
    }

    /// Set the check position.
    pub fn set_check_pos(&mut self, pos: &Position, cell: Option<CellId>) {
        self.check_pos = *pos;
        self.check_cell = cell;
        self.cell_array_valid = false;
        self.cache_global_sphere(None);
    }

    /// Add an offset to the check position.
    pub fn add_offset_to_check_pos(&mut self, v: Vec3) {
        self.cell_array_valid = false;
        self.check_pos.frame.origin = self.check_pos.frame.origin.add(v);
        self.cache_global_sphere(Some(v));
    }

    /// How a sphere crosses a landblock boundary.
    ///
    /// Note the offset is applied **only** when the new id is outdoors; the id is always
    /// assigned.
    pub fn adjust_check_pos(&mut self, new_cell: CellId) {
        if landdefs::is_outdoors(new_cell) {
            let off = landdefs::get_block_offset(new_cell, self.check_pos.cell);
            self.cache_global_sphere(Some(off));
            self.check_pos.frame.origin = self.check_pos.frame.origin.add(off);
        }
        self.check_pos.cell = new_cell;
    }

    /// Save the check position.
    pub fn save_check_pos(&mut self) {
        self.backup_check_pos = self.check_pos;
        self.backup_cell = self.check_cell;
    }

    /// Restore the check position.
    pub fn restore_check_pos(&mut self) {
        let (pos, cell) = (self.backup_check_pos, self.backup_cell);
        self.set_check_pos(&pos, cell);
    }

    /// Check the walkables.
    ///
    /// This **halves `walkable_check_pos.radius` in place** on every call,
    /// permanently. That is the mechanism by which a character eventually falls off a narrow
    /// ledge: repeated probes shrink geometrically until the polygon no longer holds the sphere.
    /// A "clean" implementation that does not mutate never falls off.
    pub fn check_walkables(&mut self) -> bool {
        let Some(poly) = self.walkable.as_ref() else {
            return true;
        };
        self.walkable_check_pos.radius *= 0.5;
        poly.check_walkable(&self.walkable_check_pos, self.walkable_up)
    }

    /// Whether a walkable is allowable.
    #[inline]
    #[must_use]
    pub fn is_walkable_allowable(&self, z: f32) -> bool {
        self.walkable_allowance < z
    }

    /// The block offset between the current and check positions.
    #[must_use]
    pub fn curr_to_check_block_offset(&self) -> Vec3 {
        landdefs::get_block_offset(self.curr_pos.cell, self.check_pos.cell)
    }

    /// A landing happened: save the check position, arm
    /// the re-validation and reset the step-down budget.
    pub fn set_collide(&mut self, normal: Vec3) {
        self.save_check_pos();
        self.collide = true;
        self.walk_interp = 1.0;
        self.step_up_normal = normal;
    }

    /// Record the polygon that was found walkable and
    /// everything needed to re-test it.
    pub fn set_walkable(
        &mut self,
        sphere: Sphere,
        poly: Polygon,
        up: Vec3,
        pos: Position,
        scale: f32,
    ) {
        self.walkable = Some(poly);
        self.walkable_check_pos = sphere;
        self.walkable_up = up;
        self.walkable_pos = pos;
        self.walkable_scale = scale;
    }

    /// Set the walkable-check position on the sphere path.
    pub fn set_walkable_check_pos(&mut self, sphere: Sphere) {
        self.walkable_check_pos = sphere;
    }

    /// A back-facing polygon was hit; record the
    /// **negated** normal and whether it should be handled as a step-up.
    pub fn set_neg_poly_hit(&mut self, step_up: bool, normal: Vec3) {
        self.neg_step_up = step_up;
        self.neg_poly_hit = true;
        self.neg_collision_normal = normal.negate();
    }

    /// The mover's own spheres for the current check position, as a slice.
    #[must_use]
    pub fn global_spheres(&self) -> &[Sphere] {
        &self.global_sphere[..self.num_sphere]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the retail sphere path -- init_sphere, cache_global_sphere,
    // adjust_check_pos, check_walkables, init_path.

    fn path_with(radius: f32) -> SpherePath {
        let mut p = SpherePath::default();
        p.init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, 1.0), radius)], 1.0);
        p
    }

    #[test]
    fn init_sphere_clamps_to_two_and_scales_centre_and_radius() {
        let mut p = SpherePath::default();
        let s = [
            Sphere::new(Vec3::new(1.0, 0.0, 2.0), 0.5),
            Sphere::new(Vec3::new(0.0, 1.0, 3.0), 0.25),
            Sphere::new(Vec3::ZERO, 9.0),
        ];
        p.init_sphere(&s, 2.0);
        assert_eq!(p.num_sphere, 2, "the swept-sphere path stores at most two");
        assert_eq!(
            p.local_sphere[0],
            Sphere::new(Vec3::new(2.0, 0.0, 4.0), 1.0)
        );
        assert_eq!(
            p.local_sphere[1],
            Sphere::new(Vec3::new(0.0, 2.0, 6.0), 0.5)
        );
        // local_low_point is derived from the SCALED sphere 0.
        assert_eq!(p.local_low_point, Vec3::new(2.0, 0.0, 3.0));
    }

    #[test]
    fn cache_global_sphere_translates_when_given_an_offset() {
        let mut p = path_with(0.5);
        p.check_pos = Position::new(
            CellId(0xA9B4_0001),
            Frame::new(Vec3::new(10.0, 20.0, 30.0), Quat::IDENTITY),
        );
        p.cache_global_sphere(None);
        assert_eq!(p.global_sphere[0].center, Vec3::new(10.0, 20.0, 31.0));
        assert_eq!(p.global_sphere[0].radius, 0.5);
        assert_eq!(p.global_low_point, Vec3::new(10.0, 20.0, 30.5));
        // and a translate moves both without re-transforming
        p.cache_global_sphere(Some(Vec3::new(1.0, 2.0, 3.0)));
        assert_eq!(p.global_sphere[0].center, Vec3::new(11.0, 22.0, 34.0));
        assert_eq!(p.global_low_point, Vec3::new(11.0, 22.0, 33.5));
    }

    #[test]
    fn add_offset_to_check_pos_moves_the_origin_and_the_cached_spheres_together() {
        let mut p = path_with(0.5);
        p.set_check_pos(
            &Position::new(CellId(0xA9B4_0001), Frame::new(Vec3::ZERO, Quat::IDENTITY)),
            Some(CellId(0xA9B4_0001)),
        );
        p.cell_array_valid = true;
        p.add_offset_to_check_pos(Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(p.check_pos.frame.origin, Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(p.global_sphere[0].center, Vec3::new(1.0, 0.0, 1.0));
        assert!(
            !p.cell_array_valid,
            "moving the check position invalidates the cell list"
        );
    }

    #[test]
    fn adjust_check_pos_rebases_across_a_landblock_only_for_an_outdoor_id() {
        let mut p = path_with(0.5);
        p.set_check_pos(
            &Position::new(
                CellId(0xA9B4_0001),
                Frame::new(Vec3::new(191.0, 5.0, 0.0), Quat::IDENTITY),
            ),
            Some(CellId(0xA9B4_0001)),
        );
        // Move into the block to the east: the origin must drop by 192.
        p.adjust_check_pos(CellId(0xAAB4_0001));
        assert_eq!(p.check_pos.cell, CellId(0xAAB4_0001));
        assert!(
            (p.check_pos.frame.origin.x - (-1.0)).abs() < 1e-3,
            "{:?}",
            p.check_pos.frame.origin
        );
        // The cached sphere moved with it.
        assert!((p.global_sphere[0].center.x - (-1.0)).abs() < 1e-3);

        // An interior id assigns the cell but applies no offset.
        let mut q = path_with(0.5);
        q.set_check_pos(
            &Position::new(
                CellId(0xA9B4_0001),
                Frame::new(Vec3::new(5.0, 5.0, 0.0), Quat::IDENTITY),
            ),
            Some(CellId(0xA9B4_0001)),
        );
        q.adjust_check_pos(CellId(0xAAB4_0100));
        assert_eq!(q.check_pos.cell, CellId(0xAAB4_0100));
        assert_eq!(q.check_pos.frame.origin, Vec3::new(5.0, 5.0, 0.0));
    }

    /// Contract item 5.4: the halving is permanent, and it is what makes a character fall off a
    /// narrow ledge after a bounded number of probes.
    #[test]
    fn check_walkables_halves_the_probe_radius_in_place_until_it_fails() {
        let poly = Polygon::new(vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ]);
        let mut p = path_with(0.5);
        // A sphere whose projected centre sits 0.9 m outside the polygon's +X edge: a probe of
        // radius 1.0 reaches it, radius 0.5 does not.
        p.set_walkable(
            Sphere::new(Vec3::new(1.9, 0.5, 0.0), 1.0),
            poly,
            Vec3::new(0.0, 0.0, 1.0),
            Position::new(CellId(1), Frame::default()),
            1.0,
        );
        assert_eq!(p.walkable_check_pos.radius, 1.0);
        let mut probes = 0;
        let mut held = true;
        while held && probes < 10 {
            held = p.check_walkables();
            probes += 1;
        }
        assert!(!held, "the probe must eventually stop holding the sphere");
        assert_eq!(
            probes, 1,
            "radius 1.0 halves to 0.5 before the first test, which already fails"
        );
        assert_eq!(
            p.walkable_check_pos.radius, 0.5,
            "and the halving is permanent"
        );

        // With no walkable polygon at all the answer is an unconditional 1.
        let mut q = path_with(0.5);
        assert!(q.check_walkables());
    }

    #[test]
    fn check_walkables_shrinks_geometrically_over_repeated_probes() {
        let poly = Polygon::new(vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(10.0, 10.0, 0.0),
            Vec3::new(0.0, 10.0, 0.0),
        ]);
        let mut p = path_with(0.5);
        // Centre well inside: the probe always holds, and the radius still halves every time.
        p.set_walkable(
            Sphere::new(Vec3::new(5.0, 5.0, 0.0), 4.0),
            poly,
            Vec3::new(0.0, 0.0, 1.0),
            Position::new(CellId(1), Frame::default()),
            1.0,
        );
        for k in 1..=5 {
            assert!(p.check_walkables());
            let expect = 4.0_f32 / 2.0_f32.powi(k);
            assert!(
                (p.walkable_check_pos.radius - expect).abs() < 1e-6,
                "probe {k}"
            );
        }
    }

    #[test]
    fn init_path_chooses_the_insert_type_from_whether_there_is_a_begin_position() {
        let mut p = path_with(0.5);
        let end = Position::new(
            CellId(5),
            Frame::new(Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY),
        );
        p.init_path(Some(CellId(5)), None, &end);
        assert_eq!(p.insert_type, InsertType::Placement);
        assert_eq!(p.curr_pos, end);
        let begin = Position::new(CellId(5), Frame::new(Vec3::ZERO, Quat::IDENTITY));
        p.init_path(Some(CellId(5)), Some(begin), &end);
        assert_eq!(p.insert_type, InsertType::Transition);
        assert_eq!(p.curr_pos, begin);
    }

    #[test]
    fn save_and_restore_check_pos_round_trip() {
        let mut p = path_with(0.5);
        let a = Position::new(
            CellId(1),
            Frame::new(Vec3::new(1.0, 1.0, 1.0), Quat::IDENTITY),
        );
        p.set_check_pos(&a, Some(CellId(1)));
        p.save_check_pos();
        p.add_offset_to_check_pos(Vec3::new(5.0, 0.0, 0.0));
        assert_eq!(p.check_pos.frame.origin.x, 6.0);
        p.restore_check_pos();
        assert_eq!(p.check_pos, a);
        assert_eq!(p.check_cell, Some(CellId(1)));
        assert_eq!(p.global_sphere[0].center, Vec3::new(1.0, 1.0, 2.0));
    }

    #[test]
    fn set_neg_poly_hit_stores_the_negated_normal() {
        let mut p = path_with(0.5);
        p.set_neg_poly_hit(true, Vec3::new(0.0, 0.0, 1.0));
        assert!(p.neg_poly_hit);
        assert!(p.neg_step_up);
        assert_eq!(p.neg_collision_normal, Vec3::new(0.0, 0.0, -1.0));
    }

    #[test]
    fn is_walkable_allowable_is_a_strict_greater_than() {
        let mut p = path_with(0.5);
        p.walkable_allowance = LANDING_Z;
        assert!(!p.is_walkable_allowable(LANDING_Z));
        assert!(p.is_walkable_allowable(LANDING_Z + 0.001));
        assert!(!p.is_walkable_allowable(0.0));
    }
}
