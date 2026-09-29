//! The transition system and its four sub-structures.
//!
//! Transcribed against the client's own transition, object-info, sphere-path, collision-info
//! and cell-array code.
//!
//! Moving an object is called a **transition**. A transition bundles an [`ObjectInfo`] (who is
//! moving and how), a [`SpherePath`] (the swept spheres and the step-up/step-down state), a
//! [`CollisionInfo`] (the accumulated contact plane, sliding normal and hit objects) and a
//! [`crate::cell::CellArray`] (the cells the check position touches).
//!
//! **The pool is exactly ten deep and it is shared state.** Transition creation
//! hands out one of ten statically constructed objects indexed by a global
//! `transition_level`; at depth ten it returns NULL and the caller aborts the move. A rebuild with
//! unbounded recursion behaves differently and blows the stack; a rebuild that silently allows
//! depth eleven moves objects the original refuses to move.

pub mod collide;
pub mod cylinder;
pub mod insert;
pub mod objectinfo;
pub mod place;
pub mod spherepath;
pub mod walk;

pub use objectinfo::{ObjectInfo, ObjectInfoState};
pub use spherepath::{InsertType, SpherePath};

use dereth_primitives::{CellId, ObjectId, Vec3};

use crate::arena::{Arena, PhysHandle};
use crate::cell::{CellArray, CellResolver, CellRuntime};
use crate::geom::plane::Plane;
use crate::globals::TRANSITION_POOL_SIZE;
use crate::math::V3;
use crate::obj::PhysicsObj;
use crate::source::LandSource;

/// `TransitionState`. Every value except `Ok` means "retry"; the caller's attempt budget is what
/// bounds the work.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum TransitionState {
    /// The initial value of `transitional_insert`'s accumulator; never returned to callers.
    #[default]
    Invalid = 0,
    /// No obstruction — the check position stands.
    Ok = 1,
    /// Hard block. `validate_transition` rolls back to `curr_pos` and reports the collision.
    Collided = 2,
    /// The check position was moved (pushed out of a surface, snapped onto ground, or clipped to
    /// an exact contact time). Retry.
    Adjusted = 3,
    /// The check position was moved *along* a surface. Retry, with the contact plane invalidated.
    Slid = 4,
}

/// Accumulated collision results for a transition.
#[derive(Debug, Clone, Default)]
pub struct CollisionInfo {
    pub last_known_contact_plane_valid: bool,
    pub last_known_contact_plane: Plane,
    pub last_known_contact_plane_is_water: bool,
    pub last_known_contact_plane_cell_id: CellId,

    pub contact_plane_valid: bool,
    pub contact_plane: Plane,
    pub contact_plane_cell_id: CellId,
    pub contact_plane_is_water: bool,

    pub sliding_normal_valid: bool,
    pub sliding_normal: Vec3,

    pub collision_normal_valid: bool,
    pub collision_normal: Vec3,

    /// Present in the layout; no writer of it is known.
    pub adjust_offset: Vec3,

    pub collide_object: Vec<(PhysHandle, TransitionState)>,
    pub last_collided_object: Option<PhysHandle>,
    pub collided_with_environment: bool,
    /// The anti-stuck counter: three consecutive sub-steps in which a gravity object failed to
    /// move and had no contact fabricate a synthetic plane.
    pub frames_stationary_fall: u32,
    /// **Extra bookkeeping outside the original client's collision-result layout:** the object
    /// id from the cell whose entry check refused this transition.
    ///
    /// Retail needs no such field because it plays the contact effect *inside* the refusal: the
    /// mover plays the barrier's `_pscript` at the given intensity right after refusing. This
    /// crate has no weenie and no play-script type, so the refusal is recorded here and
    /// the world object update turns it into
    /// [`crate::step::PhysicsNotice::MoveRestricted`]. The observable is the same script, raised
    /// on the same body, once per refused sub-step.
    pub restricted_by: Option<ObjectId>,
}

impl CollisionInfo {
    /// Initialise the collision info.
    pub fn init(&mut self) {
        *self = Self::default();
    }

    /// Set the contact plane.
    ///
    /// **What `is_water` decides** — worth stating, because the name is misleading
    /// and every geometry call site is hard-coded rather than asking the cell:
    ///
    /// * The walkability test runs its anti-sink push — treat the contact
    ///   plane as an infinite plane and shove the sphere out until its centre is `radius` away —
    ///   **only** when `contact_plane_is_water == 0` and `contact_plane_cell_id != 0`.
    /// * Contact-state finalization (setting bit `0x8`) copies it into
    ///   `WATER_CONTACT_TS (0x8)` of `transient_state`, and **nothing in the client ever tests
    ///   that bit** except contact-plane initialization, which feeds it straight
    ///   back into `init_contact_plane`'s third argument on the next frame.
    ///
    /// So the two consumers do **not** pull in opposite directions: the water bit has no
    /// independent behaviour, and its only job is to carry the anti-sink suppression from one
    /// frame's transition into the next. The field is really *"this contact plane is a local
    /// tangent, do not extend it"*, which is why every site that builds a plane tangent to a
    /// curved surface passes `1` -- the sphere's step down, its walkable branch, and both
    /// cylinder sites in
    /// `super::cylinder` -- while every site that has a real polygon passes the real answer:
    /// the BSP walk passes `0`, and the walkability validation passes the cell's own water type.
    ///
    /// The visible consequence the client accepts is that a body standing on another object's
    /// sphere reports `WATER_CONTACT_TS`. That is reproduced here rather than resolved.
    pub fn set_contact_plane(&mut self, plane: Plane, is_water: bool) {
        self.contact_plane = plane;
        self.contact_plane_valid = true;
        self.contact_plane_is_water = is_water;
    }

    /// Set the collision normal.
    ///
    /// Sets `collision_normal_valid` **before** the normalise, so a degenerate normal still counts
    /// as "valid but zero".
    pub fn set_collision_normal(&mut self, n: Vec3) {
        self.collision_normal_valid = true;
        let mut n = n;
        if n.normalize_check_small() {
            n = Vec3::ZERO;
        }
        self.collision_normal = n;
    }

    /// The same, but **Z is forced to 0 first**.
    /// Sliding is always horizontal.
    pub fn set_sliding_normal(&mut self, n: Vec3) {
        self.sliding_normal_valid = true;
        let mut n = Vec3::new(n.x, n.y, 0.0);
        if n.normalize_check_small() {
            n = Vec3::ZERO;
        }
        self.sliding_normal = n;
    }

    /// De-duplicating append. A non-`Ok` state also
    /// records `last_collided_object`.
    pub fn add_object(&mut self, obj: PhysHandle, state: TransitionState) {
        if !self.collide_object.iter().any(|(h, _)| *h == obj) {
            self.collide_object.push((obj, state));
        }
        if state != TransitionState::Ok {
            self.last_collided_object = Some(obj);
        }
    }
}

/// Rebuild-only bookkeeping for the two object walks, with no counterpart in the original
/// transition layout.
///
/// Both walks are written to be tolerant: the object walk steps over a
/// shadow entry whose `physobj` has a parent or is the mover, and
/// building collision returns `OK_TS` for a building with no part
/// array. Tolerance with nothing counting it is how a walk that silently visits nothing passes for
/// a walk that found nothing, so every skip is counted and the tests assert on the counts rather
/// than on the absence of a crash.
///
/// Reset by [`Transition::init`], like every other field of the pooled object.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CollisionCounters {
    /// Shadow-list entries the walk looked at, skips included.
    pub shadows_visited: u32,
    /// Skipped because the object has a parent: a held or worn object collides through its
    /// holder, never on its own.
    pub skipped_parented: u32,
    /// Skipped because the shadow's `physobj` is `object_info.object` — the mover itself.
    pub skipped_self: u32,
    /// Handed to the object's collision search.
    pub objects_tested: u32,
    /// A shadow handle that no longer names a live object. **Unreachable in the client**, whose
    /// shadow list holds raw pointers that object removal clears; here the list
    /// holds arena handles, and a stale one would be a bug in
    /// [`crate::step::PhysicsWorld`]'s cell bookkeeping rather than a physics decision. Asserted
    /// to be zero.
    pub shadow_dangling: u32,
    /// Whole walks skipped by [`InsertType::InitialPlacement`].
    pub obj_walk_skipped_initial_placement: u32,

    /// Land cells whose building pointer was non-NULL.
    pub buildings_visited: u32,
    /// Buildings with no part array (or an empty one), which `find_building_collisions` answers
    /// with `OK_TS` without ever setting `bldg_check`.
    pub buildings_without_parts: u32,
    /// Building parts whose graphics object carries no physics BSP: part collision search returns
    /// `OK_TS` and never caches the localspace spheres.
    pub building_parts_without_bsp: u32,
    /// Buildings whose bounding-sphere prune rejected every one of the mover's spheres, so the
    /// tree was never walked.
    pub buildings_sphere_pruned: u32,
    /// The building's BSP was actually walked.
    pub buildings_bsp_walked: u32,

    /// Collision searches that reached one part of an ordinary
    /// object's part array.
    pub object_parts_visited: u32,
    /// As `building_parts_without_bsp`, for an ordinary object's part.
    pub object_parts_without_bsp: u32,
    /// As `buildings_sphere_pruned`, for an ordinary object's part.
    pub object_sphere_pruned: u32,
    /// As `buildings_bsp_walked`, for an ordinary object's part.
    pub object_bsp_walked: u32,
    /// Candidates skipped outright because the candidate
    /// is itself a missile, or is a non-target ethereal weenie or (with a projectile target) a
    /// non-target weenie creature, so no arm of the geometry test ran.
    pub objects_missile_ignored: u32,

    /// **Entries into the indoor collision path**, whatever it then did. Without
    /// this, "the interior branch ran and found nothing" and "the interior branch never ran" are
    /// indistinguishable from outside this crate.
    ///
    /// **Entries, not distinct cells.** `find_transitional_position` re-enters the check cell once
    /// per sub-step, and makes one sub-step per sphere radius, so a
    /// single-cell walk 5 m long with a 0.1 m sphere counts about 50. A probe reporting **1** over
    /// a 12 m path is the whole diagnosis: it means the walk stopped consulting cells after the
    /// first sub-step, which is what setting `check_cell = NULL` for a sphere that is inside no
    /// cell at all does, the indoor path then returning `OK_TS` at its opening guard for every step
    /// after. A low number here is "the walk left the cell system", not "the cell had one wall".
    pub env_cells_visited: u32,
    /// Interior cells whose physics BSP was NULL; collision search answers with `OK_TS` without
    /// walking anything.
    pub env_cells_without_bsp: u32,
    /// Interior cells whose physics BSP was actually walked -- either the collision search or
    /// the placement insert.
    pub env_bsp_walked: u32,

    /// Individual calls to the cylinder-sphere collision — the middle arm of the object's
    /// collision search. Zero over a whole walk means no candidate took the cylsphere arm.
    pub object_cylspheres_tested: u32,
}

/// One pooled transition.
#[derive(Debug, Default)]
pub struct Transition {
    pub object_info: ObjectInfo,
    pub sphere_path: SpherePath,
    pub collision_info: CollisionInfo,
    pub cell_array: CellArray,
    /// Rebuild-only. See [`CollisionCounters`].
    pub counters: CollisionCounters,
    /// Deferred. The original calls straight into the
    /// moving object; here the transition records it and the caller applies it after the
    /// transition ends. Nothing inside a transition reads the velocity again, so the two are
    /// observationally the same, and it is what lets the transition hold no mutable borrow of the
    /// arena.
    pub kill_velocity: bool,
}

impl Transition {
    /// Reset `object_info`, every
    /// Collision-result validity flag and the cell-array counters.
    pub fn init(&mut self) {
        self.object_info = ObjectInfo::default();
        self.sphere_path.init();
        self.collision_info.init();
        self.cell_array.clear();
        self.cell_array.do_not_load_cells = false;
        self.kill_velocity = false;
        self.counters = CollisionCounters::default();
    }

    /// The transition's object initialisation and the object-info initialisation.
    ///
    /// Object-info initialization's tail is four queries on the weenie object:
    ///
    /// ```text
    ///   no weenie object                -> none of the four
    ///   impenetrable                    -> 0x0080
    ///   player                          -> 0x0100
    ///   player killer                   -> 0x0800
    ///   player killer lite              -> 0x1000
    /// ```
    ///
    /// `IS_PLAYER` serves the house barrier. The other three serve their reachable
    /// `FindObjCollisions` consumer: ordinary NPK player bodies pass through,
    /// while a matching PK status or either body's impenetrable status retains collision.
    pub fn init_object(&mut self, obj: &PhysicsObj, handle: PhysHandle, state: u32) {
        self.object_info.object = Some(handle);
        self.object_info.state = state;
        if let Some(w) = obj.weenie.as_ref() {
            if w.is_impenetrable {
                self.object_info.state |= ObjectInfoState::IS_IMPENETRABLE;
            }
            if w.is_player {
                self.object_info.state |= ObjectInfoState::IS_PLAYER;
            }
            if w.is_pk {
                self.object_info.state |= ObjectInfoState::IS_PK;
            }
            if w.is_pk_lite {
                self.object_info.state |= ObjectInfoState::IS_PK_LITE;
            }
        }
        self.object_info.scale = obj.scale;
        self.object_info.step_up_height = obj.step_up_height();
        self.object_info.step_down_height = obj.step_down_height();
        self.object_info.ethereal = obj.state.is_ethereal();
        // step_down = !(state >> 6 & 1): missiles never step down.
        self.object_info.step_down = !obj.state.is_missile();
        // The object info's target id has no client writer; the body carries the host's
        // projectile target, which is zero for every client body.
        self.object_info.target_id = obj.projectile_target_id;
    }

    /// Seeds **both** the contact plane and the
    /// last-known contact plane.
    pub fn init_contact_plane(&mut self, cell_id: CellId, plane: Plane, is_water: bool) {
        let ci = &mut self.collision_info;
        ci.contact_plane = plane;
        ci.contact_plane_valid = true;
        ci.contact_plane_is_water = is_water;
        ci.contact_plane_cell_id = cell_id;
        ci.last_known_contact_plane = plane;
        ci.last_known_contact_plane_valid = true;
        ci.last_known_contact_plane_is_water = is_water;
        ci.last_known_contact_plane_cell_id = cell_id;
    }

    /// Seeds only the last-known plane,
    /// but **still writes `contact_plane_cell_id`**, which looks like a client bug: the
    /// *last known* cell id field is left stale. Reproduced.
    pub fn init_last_known_contact_plane(&mut self, cell_id: CellId, plane: Plane, is_water: bool) {
        let ci = &mut self.collision_info;
        ci.last_known_contact_plane = plane;
        ci.last_known_contact_plane_valid = true;
        ci.last_known_contact_plane_is_water = is_water;
        ci.contact_plane_cell_id = cell_id;
    }

    /// Initialise the sliding normal.
    pub fn init_sliding_normal(&mut self, n: Vec3) {
        self.collision_info.set_sliding_normal(n);
    }
}

/// The ten statically constructed transition objects and the global
/// `transition_level` that indexes them.
#[derive(Debug, Default)]
pub struct TransitionPool {
    level: usize,
    slots: Vec<Transition>,
    /// The deepest level ever reached, for diagnostics and for the pool-depth test.
    pub high_water: usize,
}

impl TransitionPool {
    #[must_use]
    pub fn new() -> Self {
        Self {
            level: 0,
            slots: Vec::new(),
            high_water: 0,
        }
    }

    /// Hand out the transition at the current level and
    /// increment. Returns `None` at depth ten, and the caller **must** abort the move.
    pub fn make(&mut self) -> Option<usize> {
        if self.level >= TRANSITION_POOL_SIZE {
            return None;
        }
        let idx = self.level;
        while self.slots.len() <= idx {
            self.slots.push(Transition::default());
        }
        self.slots[idx].init();
        self.level += 1;
        self.high_water = self.high_water.max(self.level);
        Some(idx)
    }

    /// Decrement the level. It does nothing else.
    pub fn cleanup(&mut self) {
        debug_assert!(self.level > 0, "transition cleanup below zero");
        self.level = self.level.saturating_sub(1);
    }

    #[must_use]
    pub const fn level(&self) -> usize {
        self.level
    }

    /// Take the transition out of its slot so it can be borrowed mutably alongside the world.
    /// Put it back with [`Self::put`].
    pub fn take(&mut self, idx: usize) -> Transition {
        std::mem::take(&mut self.slots[idx])
    }

    pub fn put(&mut self, idx: usize, t: Transition) {
        self.slots[idx] = t;
    }
}

/// Everything a transition reads from the world while it runs.
///
/// Deliberately shared-reference-only over the object arena: object-versus-object collision reads
/// the other object's state, geometry and position and writes only into the transition's own
/// collision result. That is what makes the ten-deep re-entrancy expressible without holding a
/// `&mut PhysicsObj` across a call.
pub struct TransitionCtx<'a> {
    pub land: &'a dyn LandSource,
    pub objects: &'a Arena<PhysicsObj>,
    pub cells: &'a std::collections::BTreeMap<u32, CellRuntime>,
    /// The object being moved, excluded from its own collision tests.
    pub mover: Option<PhysHandle>,
    /// Object-maintenance table used for searches.
    ///
    /// Exactly one physics path resolves an *object id* rather than a handle:
    /// Move-restriction handling looks up its `restriction_obj` in the object table
    /// because a cell names its gate keeper by iid and not by pointer.
    ///
    /// `None` is a context that cannot look one up, and it is **not** the same as an empty table:
    /// retail treats a lookup that answers NULL as a closed gate (the null arm falls into the
    /// `COLLIDED_TS` tail), so a `None` here would fence every restricted cell in the world. It
    /// therefore means *"this context does not run the restriction check at all"*, which is what
    /// every unit test that is measuring something else wants, and the one production
    /// world transition-context constructor always supplies the table.
    pub object_table: Option<&'a crate::longhash::LongHash<PhysHandle>>,
    /// A host's answer to the entry-restriction question. `None`, the client's case and the
    /// default of every world, asks the gate keeper's pushed record as the client does; see
    /// [`EntryRestrictionHost`].
    pub entry_host: Option<&'a dyn EntryRestrictionHost>,
}

/// A host that answers a cell's entry-restriction question itself.
///
/// When a transition crosses into a cell whose `restriction_obj` names a gate keeper, the client
/// looks the gate keeper up by id and asks its record whether the mover may enter. A host that
/// simulates bodies of its own (a server, whose gate keepers are its own objects and whose
/// answer depends on game state physics does not hold) installs this with
/// [`crate::PhysicsWorld::set_entry_restriction_host`]; the check then asks it instead of the
/// gate keeper's record. Everything before that question is unchanged: an object with no weenie,
/// a mover that is not a player, a cell with no restriction, a mover that can bypass, and a
/// context without an object table are all answered as before, and the refusal's collision
/// normal and contact effect are the same.
///
/// **The client installs none**, so its transitions ask the pushed records exactly as before.
pub trait EntryRestrictionHost: Send + Sync {
    /// Whether the mover `mover` may enter a cell whose gate keeper is `restriction`: the whole
    /// of the gate keeper's lookup and answer (`false` is a closed gate, as a gate keeper that
    /// cannot be found is).
    fn can_move_into(&self, restriction: ObjectId, mover: ObjectId) -> bool;
}

impl std::fmt::Debug for TransitionCtx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransitionCtx")
            .field("mover", &self.mover)
            .finish_non_exhaustive()
    }
}

impl<'a> TransitionCtx<'a> {
    #[must_use]
    pub fn resolver(&self) -> CellResolver<'a> {
        CellResolver::new(self.land)
    }

    /// The shadow-object list of one cell — the object set collision uses.
    #[must_use]
    pub fn shadow_objects(&self, id: CellId) -> &[PhysHandle] {
        self.cells
            .get(&id.0)
            .map_or(&[][..], |c| &c.shadow_object_list)
    }

    /// Cell restriction object.
    #[must_use]
    pub fn restriction_obj(&self, id: CellId) -> Option<ObjectId> {
        self.cells.get(&id.0).and_then(|c| c.restriction_obj)
    }

    /// The object with this iid, or `None` for retail's
    /// NULL.
    #[must_use]
    pub fn get_object_a(&self, id: ObjectId) -> Option<&'a PhysicsObj> {
        let h = self.object_table?.get(id.0).copied()?;
        self.objects.get(h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the recovered collision and transition behavior section
    // The transition pool and collision result both reproduce the recovered behavior.

    #[test]
    fn init_clears_the_collision_counters_with_everything_else() {
        // The counters are rebuild-only, but they live on the pooled object, and a pool slot that
        // carried counts from a previous transition would make every assertion on them meaningless
        // at depth two or more.
        let mut t = Transition::default();
        t.counters.shadows_visited = 7;
        t.counters.buildings_bsp_walked = 3;
        t.init();
        assert_eq!(t.counters, CollisionCounters::default());
    }

    #[test]
    fn init_object_only_ors_weenie_status_and_retains_supplied_bits() {
        let mut objects: Arena<PhysicsObj> = Arena::new();
        let mut body = PhysicsObj::new(
            ObjectId(1),
            std::sync::Arc::new(crate::source::SetupGeometry::dummy()),
            0.0,
            true,
        );
        body.weenie = Some(crate::obj::WeenieRestrictions::default());
        let handle = objects.insert(body);
        let supplied = ObjectInfoState::IS_IMPENETRABLE
            | ObjectInfoState::IS_PLAYER
            | ObjectInfoState::IS_PK
            | ObjectInfoState::IS_PK_LITE;
        let mut t = Transition::default();
        t.init_object(objects.get(handle).expect("body"), handle, supplied);
        assert_eq!(
            t.object_info.state & supplied,
            supplied,
            " conditionally ORs true virtual results; a false result \
             never clears a bit supplied by the transition initializer's caller"
        );

        let mut marked = PhysicsObj::new(
            ObjectId(2),
            std::sync::Arc::new(crate::source::SetupGeometry::dummy()),
            0.0,
            true,
        );
        marked.weenie = Some(crate::obj::WeenieRestrictions {
            is_player: true,
            is_pk: true,
            is_pk_lite: true,
            is_impenetrable: true,
            ..crate::obj::WeenieRestrictions::default()
        });
        let marked = objects.insert(marked);
        t.init_object(objects.get(marked).expect("marked body"), marked, 0);
        assert_eq!(
            t.object_info.state & supplied,
            supplied,
            "the four true PWD answers must OR their exact object-state bits"
        );
    }

    #[test]
    fn the_pool_is_exactly_ten_deep_and_then_refuses() {
        let mut p = TransitionPool::new();
        let mut handles = Vec::new();
        for i in 0..TRANSITION_POOL_SIZE {
            let h = p.make().unwrap_or_else(|| panic!("depth {i} must succeed"));
            assert_eq!(h, i, "the pool is indexed by transition_level");
            handles.push(h);
        }
        assert_eq!(p.level(), 10);
        assert_eq!(
            p.make(),
            None,
            "depth ten returns NULL and the caller aborts the move"
        );
        assert_eq!(
            p.level(),
            10,
            "a refused transition request does not advance the level"
        );
        for _ in 0..TRANSITION_POOL_SIZE {
            p.cleanup();
        }
        assert_eq!(p.level(), 0);
        assert_eq!(p.high_water, 10);
        // and the pool is reusable afterwards
        assert_eq!(p.make(), Some(0));
    }

    #[test]
    fn set_collision_normal_is_valid_even_when_degenerate() {
        let mut ci = CollisionInfo::default();
        ci.set_collision_normal(Vec3::new(0.0, 0.0, 0.000_01));
        assert!(
            ci.collision_normal_valid,
            "valid is set BEFORE the normalise"
        );
        assert_eq!(ci.collision_normal, Vec3::ZERO);
        ci.set_collision_normal(Vec3::new(0.0, 0.0, 3.0));
        assert_eq!(ci.collision_normal, Vec3::new(0.0, 0.0, 1.0));
    }

    #[test]
    fn set_sliding_normal_forces_z_to_zero_before_normalising() {
        let mut ci = CollisionInfo::default();
        ci.set_sliding_normal(Vec3::new(3.0, 0.0, 4.0));
        assert!(ci.sliding_normal_valid);
        assert_eq!(
            ci.sliding_normal,
            Vec3::new(1.0, 0.0, 0.0),
            "sliding is always horizontal"
        );
        // A purely vertical normal collapses to zero rather than staying vertical.
        ci.set_sliding_normal(Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(ci.sliding_normal, Vec3::ZERO);
    }

    #[test]
    fn add_object_deduplicates_and_tracks_the_last_collision() {
        let mut a: Arena<u32> = Arena::new();
        let h1 = a.insert(1);
        let h2 = a.insert(2);
        let mut ci = CollisionInfo::default();
        ci.add_object(h1, TransitionState::Ok);
        ci.add_object(h1, TransitionState::Collided);
        assert_eq!(ci.collide_object.len(), 1, "de-duplicated by handle");
        assert_eq!(
            ci.last_collided_object,
            Some(h1),
            "but the state still registers"
        );
        ci.add_object(h2, TransitionState::Ok);
        assert_eq!(ci.collide_object.len(), 2);
        assert_eq!(
            ci.last_collided_object,
            Some(h1),
            "an Ok add does not overwrite it"
        );
    }

    #[test]
    fn init_last_known_contact_plane_leaves_the_last_known_cell_id_stale() {
        // A client bug, faithfully reproduced: it writes contact_plane_cell_id, not
        // last_known_contact_plane_cell_id.
        let mut t = Transition::default();
        let plane = Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        t.init_last_known_contact_plane(CellId(0xA9B4_0001), plane, false);
        assert_eq!(t.collision_info.contact_plane_cell_id, CellId(0xA9B4_0001));
        assert_eq!(
            t.collision_info.last_known_contact_plane_cell_id,
            CellId(0),
            "the last-known cell id is left stale, exactly as the client leaves it"
        );
        assert!(
            !t.collision_info.contact_plane_valid,
            "and the contact plane is NOT made valid"
        );
        assert!(t.collision_info.last_known_contact_plane_valid);
    }

    #[test]
    fn init_contact_plane_seeds_both_slots() {
        let mut t = Transition::default();
        let plane = Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: -5.0,
        };
        t.init_contact_plane(CellId(7), plane, true);
        let ci = &t.collision_info;
        assert!(ci.contact_plane_valid && ci.last_known_contact_plane_valid);
        assert_eq!(ci.contact_plane_cell_id, CellId(7));
        assert_eq!(ci.last_known_contact_plane_cell_id, CellId(7));
        assert!(ci.contact_plane_is_water && ci.last_known_contact_plane_is_water);
    }

    #[test]
    fn init_resets_everything_a_reused_pool_slot_could_be_carrying() {
        let mut t = Transition::default();
        t.collision_info.frames_stationary_fall = 3;
        t.collision_info.collided_with_environment = true;
        t.cell_array.add_cell(CellId(1), None);
        t.kill_velocity = true;
        t.init();
        assert_eq!(t.collision_info.frames_stationary_fall, 0);
        assert!(!t.collision_info.collided_with_environment);
        assert!(t.cell_array.is_empty());
        assert!(!t.kill_velocity);
    }
}
