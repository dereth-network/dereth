//! `PhysicsWorld` — the manager, the time stepping and the per-object update.
//!
//! Transcribed against the client's own physics manager and physics-object code, from the
//! manager through the friction calculation.
//!
//! **The three quantum traps**, all of which live in the world object update:
//!
//! 1. The "drop the remainder if `<= 1/30`" test lives **inside** the `elapsed > 0.2` branch. An
//!    elapsed time of 0.03 s runs one 0.03 s sub-step; an elapsed time of 0.23 s runs one 0.2 s
//!    sub-step and then *throws away* the 0.03 s remainder. Hoisting the test out of the branch is
//!    the obvious "simplification" and it changes how far every falling object travels.
//! 2. `update_time` is set to the **simulated** clock, not the wall clock, so an object's clock
//!    may legitimately lag by up to 1/30 s and that lag persists.
//! 3. The simulation clock is the time of the object currently being stepped, not wall
//!    clock time. On a slow frame different objects therefore observe different values.

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_primitives::{CellId, Frame, LocalTime, ObjectId, Position, Quat, Vec3};

use crate::arena::{Arena, PhysHandle};
use crate::cell::{Cell, CellResolver, CellRuntime};
use crate::geom::plane::Plane;
use crate::geom::PlaneExt;
use crate::geom::Sphere;
use crate::geom::SphereExt;
use crate::globals;
use crate::landdefs;
use crate::longhash::LongHash;
use crate::math::{self, V3};
use crate::obj::{PhysicsObj, PhysicsTimeStamp, SetPositionError, TransientState};
use crate::pmanager::{InterpolateDecision, InterpolationManager};
use crate::source::{LandSource, SetupGeometry};
use crate::transition::insert::find_valid_position;
use crate::transition::objectinfo::ObjectInfoState;
use crate::transition::spherepath::InsertType;
use crate::transition::{Transition, TransitionCtx, TransitionPool};

/// Notices raised during a tick, drained by the gameplay layer. Physics never calls up.
#[derive(Debug, Clone, PartialEq)]
pub enum PhysicsNotice {
    /// Raised right after updating the player's own physics object.
    PlayerPhysicsUpdated,
    /// Object-collision reporting's ordinary arm — the game record's
    /// `DoCollision(const ObjCollisionProfile&)`.
    ///
    /// On the *client* that handler's whole body is `return 1;`. An ordinary object-against-object collision has
    /// no client-side effect at all. It is kept as a notice because the rebuild server's
    /// `serv-move` is a second consumer of this same crate and does act on it.
    ObjectCollision {
        object: ObjectId,
        other: ObjectId,
        relative_velocity: Vec3,
        was_contact: bool,
    },
    /// Object-collision reporting's **missile** arm — the game record's
    /// `DoCollision(const AtkCollisionProfile&)`.
    ///
    /// Retail picks between this and [`Self::ObjectCollision`] on the object's `MISSILE_PS` state
    /// bit, and the two arms call *different handlers*, so they are two notices here rather than
    /// one notice with a flag. On the client this handler does the same as the environment-collision
    /// handler, which plays the object's default script when `SCRIPTED_COLLISION_PS` is set.
    ///
    /// The `AtkCollisionProfile` retail fills in (`0xFFFFFFFF`, the other object's id, and the
    /// contact query's answer) is **not** reconstructed because the receiver never reads its
    /// argument: it goes straight to its own physics object. A
    /// quadrant nothing can observe would be a guess dressed as a transcription.
    MissileCollision {
        object: ObjectId,
        other: ObjectId,
        relative_velocity: Vec3,
        was_contact: bool,
    },
    /// The environment-collision report -- the game record's environment-collision handler.
    EnvironmentCollision {
        object: ObjectId,
        velocity: Vec3,
        me_in_contact: bool,
    },
    /// The end-of-collision report.
    CollisionEnd { object: ObjectId, other: ObjectId },
    /// **The house barrier sparked.** The cell entry-restriction check refused this
    /// body entry to a cell, and retail's refusal ends in
    /// the mover playing the barrier's `pwd._pscript` at the given intensity.
    ///
    /// Not a `PhysicsNotice` in retail — there is no notice, the script is played inline —
    /// but the play-script type belongs to the barrier's weenie and this crate holds no weenies.
    /// See [`crate::transition::CollisionInfo::restricted_by`].
    MoveRestricted {
        /// The body that was stopped.
        object: ObjectId,
        /// The cell's `restriction_obj` — the house whose `_pscript` is the effect to play.
        restriction_obj: ObjectId,
        /// `clamp(|get_velocity()| * 0.1, 0, 1)`, `PlayScript`'s second argument. **0.5** when
        /// the mover has no physics object to ask, which is the client's pre-loaded default.
        intensity: f32,
    },
}

/// Result of changing an object's ethereal state.
///
/// The client answers with an `int`, 1 or 0. Three
/// states rather than two, because "the bit is now what you asked for", "the bit is not what you
/// asked for and is queued to be retried" and "there is no such object to ask" are three
/// different facts and a `bool` collapses the last two into the failure the caller is least
/// likely to check for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtherealResult {
    /// `return 1` — `ETHEREAL_PS` now reads what was asked, and `CHECK_ETHEREAL_TS` is clear.
    Applied,
    /// `return 0` — clearing `ETHEREAL_PS` was refused because
    /// the ethereal collision check found something standing in the object.
    /// The bit was put back and `CHECK_ETHEREAL_TS` raised, so
    /// The object update retries this every sub-step until it takes.
    Deferred,
    /// The handle names no live object. The client cannot express this; a rebuild can, and an
    /// event arriving for an object physics has already destroyed is exactly how it happens.
    NoObject,
}

/// Notice emitted by the world's notice drain.
pub type DrainNotices<'a> = std::vec::Drain<'a, PhysicsNotice>;

/// Which arm of a remote position update took.
///
/// The client's own answer is a bare `int` — 0 for the two arms that move nothing and 1 for the
/// rest — which tells a caller neither *which* arm ran nor whether the body ended up in a cell.
/// Both matter here: `ObjectPhysics` runs the latch off the
/// second and its log line off the first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveOrTeleport {
    /// No cell, or a newer `TELEPORT_TS` event — the full `SetPosition` placement, which
    /// is also the arm a body this client failed to place retries on.
    Teleported {
        /// Whether the body ended up in a cell.
        placed: bool,
    },
    /// `if (!contact) return false;` — the body was **not** repositioned, and that is retail's
    /// answer, not a failure.
    NotInContact,
    /// `InterpolateTo(pos, IsMovingTo())` within the 96 m activity radius: no transition, and so
    /// no placement scatter.
    Interpolated {
        /// Whether the body ended up in a cell.
        placed: bool,
    },
    /// `SetPositionSimple(pos, 1)` beyond the activity radius, or for a body whose
    /// `player_distance` is still `FLT_MAX` because the physics tick has not reached it yet.
    Snapped {
        /// Whether the body ended up in a cell.
        placed: bool,
    },
}

impl MoveOrTeleport {
    /// Whether the body is in a cell at the position the server named. `NotInContact` answers
    /// **true**: nothing was asked of the cell graph and whatever the body already had still
    /// stands, so the `lost` latch must not fire for it.
    #[must_use]
    pub const fn placed(self) -> bool {
        match self {
            Self::Teleported { placed }
            | Self::Interpolated { placed }
            | Self::Snapped { placed } => placed,
            Self::NotInContact => true,
        }
    }

    /// Whether this arm actually moved the body, i.e. whether `ObjectPhysics` may memo the wire
    /// position as the one the body is standing at.
    #[must_use]
    pub const fn moved(self) -> bool {
        !matches!(self, Self::NotInContact)
    }
}

/// How a collision transition is seeded — the difference between the client's two ways of starting one,
/// and it is a bigger difference than it looks.
#[derive(Debug, Clone, Copy)]
enum TransitionSeed {
    /// The sweep: the object's info build makes the state word and seeds the contact planes,
    /// and receives a non-null `begin`, which makes it a **sweep** from `from` to `to`.
    Sweep { from: Position, admin: bool },
    /// The position set: the transition's object initialisation gets a literal `0`, then
    /// initializes a path from `cell` and `pos` with no object, making it a **placement**.
    ///
    /// **`begin_cell` is `CheckPositionInternal`'s first argument, and it is NOT the object's own
    /// cell.** The destination-cell resolver obtains it from the *destination* through an
    /// out-parameter stored in a stack local and passes that result to the internal position
    /// check. A sweep seeds
    /// `init_path` from the object's own cell; a placement never does, and an object that is not in
    /// the world yet has no cell to seed from.
    Placement { begin_cell: Option<CellId> },
}

/// The world owner and driver for client-side physics.
pub struct PhysicsWorld {
    land: Arc<dyn LandSource>,
    objects: Arena<PhysicsObj>,
    /// The object table swept during each physics update, in the fixed
    /// 128-bucket `LongHash` order.
    object_table: LongHash<PhysHandle>,
    cells: BTreeMap<u32, CellRuntime>,
    pool: TransitionPool,
    /// The global last-physics-update timestamp.
    last_physics_time: f64,
    /// The physics timer's current time. Initialised to `-1.0`.
    sim_time: f64,
    player: Option<PhysHandle>,
    notices: Vec<PhysicsNotice>,
    /// The interpolation sub-manager only: the node queue filled by interpolation requests and
    /// walked by the interpolation pass.
    ///
    /// **Why a side table and not a field on [`PhysicsObj`].** In retail the manager hangs off
    /// the object (the interpolation request creates it lazily),
    /// and here it is keyed by handle instead. Everything else is the retail shape: created
    /// lazily by [`Self::interpolate_to`], cleared by [`Self::stop_interpolating`] and dropped
    /// with the body in [`Self::destroy`]. Moving it onto `PhysicsObj` is a one-field change and no
    /// caller here would move.
    ///
    /// The other two sub-managers of the same position manager — sticky and constraint — live
    /// on the animation seam in this build and reach physics through
    /// [`crate::MotionSource::adjust_position_offset`]; see [`Self::update_position_internal`].
    interpolation: BTreeMap<PhysHandle, InterpolationManager>,
    /// Whether object maintenance is active. When false, the 96 m activity radius is ignored.
    pub obj_maint_is_active: bool,
    /// How many times [`Self::set_ethereal`] refused to clear `ETHEREAL_PS`.
    ethereal_deferrals: u64,
    /// The host's entry-restriction answer, handed to every transition. `None` (the
    /// client's) asks the gate keepers' pushed records.
    entry_host: Option<Arc<dyn crate::transition::EntryRestrictionHost>>,
}

impl std::fmt::Debug for PhysicsWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhysicsWorld")
            .field("objects", &self.objects.len())
            .field("sim_time", &self.sim_time)
            .field("last_physics_time", &self.last_physics_time)
            .field("transition_level", &self.pool.level())
            .finish_non_exhaustive()
    }
}

impl PhysicsWorld {
    #[must_use]
    pub fn new(land: Arc<dyn LandSource>) -> Self {
        Self {
            land,
            objects: Arena::new(),
            object_table: LongHash::object_table(),
            cells: BTreeMap::new(),
            pool: TransitionPool::new(),
            last_physics_time: 0.0,
            // The physics timer's current time is initialised to -1.0, not to zero.
            sim_time: -1.0,
            player: None,
            interpolation: BTreeMap::new(),
            notices: Vec::new(),
            obj_maint_is_active: true,
            ethereal_deferrals: 0,
            entry_host: None,
        }
    }

    /// The physics timer's current time — the simulated clock of the object currently being
    /// stepped.
    #[must_use]
    pub const fn sim_time(&self) -> f64 {
        self.sim_time
    }

    /// The manager's own "last physics update" timestamp, the global double.
    #[must_use]
    pub const fn last_physics_time(&self) -> f64 {
        self.last_physics_time
    }

    /// The deepest transition level ever reached, for diagnostics.
    #[must_use]
    pub const fn transition_high_water(&self) -> usize {
        self.pool.high_water
    }

    // ---------------------------------------------------------------------------------------
    // **Instrumentation.** Read-only gauges over the seven process-wide containers this
    // struct owns, so a long-session station can assert that a leave/return cycle gives back
    // everything it took. They are *gauges* and not counters: each answers "how many are live
    // now", which is the only shape an "is it back at baseline" assertion can use. Nothing in
    // production reads them.
    // ---------------------------------------------------------------------------------------

    /// Live physics objects in the arena. Instrumentation.
    #[must_use]
    pub const fn body_count(&self) -> usize {
        self.objects.len()
    }

    /// Live rows of the object table swept in 128-bucket `LongHash` order. Must track
    /// [`Self::body_count`]; a divergence is a `destroy` that missed one of
    /// the two. Instrumentation.
    #[must_use]
    pub fn object_table_count(&self) -> usize {
        self.object_table.len()
    }

    /// Cells this world holds a [`crate::cell::CellRuntime`] for. Instrumentation.
    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Entries across every cell's `object_list`, the membership that cell entry adds and cell
    /// departure removes. Instrumentation.
    #[must_use]
    pub fn cell_object_count(&self) -> usize {
        self.cells.values().map(|c| c.object_list.len()).sum()
    }

    /// Entries across every cell's `shadow_object_list` — the collision lists
    /// `remove_shadows_from_cells` clears. A handle left here after its body died is exactly
    /// what `CollisionCounters::shadow_dangling` counts. Instrumentation.
    #[must_use]
    pub fn cell_shadow_count(&self) -> usize {
        self.cells
            .values()
            .map(|c| c.shadow_object_list.len())
            .sum()
    }

    /// Notices raised and not yet drained by [`Self::drain_notices`]. Instrumentation.
    #[must_use]
    pub fn pending_notice_count(&self) -> usize {
        self.notices.len()
    }

    pub fn create(&mut self, id: ObjectId, geom: Arc<SetupGeometry>, dynamic: bool) -> PhysHandle {
        let obj = PhysicsObj::new(id, geom, self.last_physics_time, dynamic);
        let h = self.objects.insert(obj);
        self.object_table.insert(id.0, h);
        h
    }

    pub fn destroy(&mut self, h: PhysHandle) {
        if let Some(o) = self.objects.get(h) {
            let id = o.id;
            self.object_table.remove(id.0);
        }
        self.remove_shadows_from_cells(h);
        for c in self.cells.values_mut() {
            c.object_list.retain(|x| *x != h);
        }
        // The manager dies with the object it hangs off.
        self.interpolation.remove(&h);
        if self.player == Some(h) {
            self.player = None;
        }
        self.objects.remove(h);
    }

    #[must_use]
    pub fn get(&self, h: PhysHandle) -> Option<&PhysicsObj> {
        self.objects.get(h)
    }

    pub fn get_mut(&mut self, h: PhysHandle) -> Option<&mut PhysicsObj> {
        self.objects.get_mut(h)
    }

    #[must_use]
    pub fn by_object_id(&self, id: ObjectId) -> Option<PhysHandle> {
        self.object_table.get(id.0).copied()
    }

    /// Re-key one object in the object table. Answers `false` and changes nothing when
    /// the handle is unknown or when **another** object already holds `id`.
    ///
    /// **The client never calls anything like this, and the reason is worth stating.**
    /// Object creation makes every physics object — the player's own included — only once the
    /// server has already named it, so in the original an id is fixed for
    /// the object's lifetime. A rebuild that stands a body on the ground before it logs in
    /// (`--no-connect`, and every offline slice) has one object whose id can still change: the
    /// local player's, when `0xF746 Login_CharacterSet` finally says what the server calls him.
    ///
    /// It is the two halves of a create in the other order — `object_table.remove(old)` then
    /// `insert(new)` — and it **refuses** rather than overwriting, because
    /// the object table inserts into a `LongHash` whose second insert for a
    /// key replaces the first, which would take the sitting object out of the physics update
    /// sweep with no error anywhere.
    pub fn set_object_id(&mut self, h: PhysHandle, id: ObjectId) -> bool {
        let Some(o) = self.objects.get(h) else {
            return false;
        };
        let old = o.id;
        if old == id {
            return true;
        }
        if self.object_table.get(id.0).is_some() {
            return false;
        }
        if self.object_table.get(old.0).copied() == Some(h) {
            self.object_table.remove(old.0);
        }
        self.object_table.insert(id.0, h);
        if let Some(o) = self.objects.get_mut(h) {
            o.id = id;
        }
        true
    }

    /// Mark the object as the player.
    pub fn set_player(&mut self, h: PhysHandle) {
        self.player = Some(h);
    }

    #[must_use]
    pub const fn player(&self) -> Option<PhysHandle> {
        self.player
    }

    /// Drain the notices raised during the last tick, in the order they were raised.
    pub fn drain_notices(&mut self) -> DrainNotices<'_> {
        self.notices.drain(..)
    }

    /// The per-cell runtime state, created on demand.
    fn cell_mut(&mut self, id: CellId) -> &mut CellRuntime {
        self.cells.entry(id.0).or_default()
    }

    /// The iid of the house that fences this cell.
    ///
    /// Retail writes it from two places and both are data, not messages:
    ///
    /// * the landblock's own restriction scan, whose tail walks all `side_cell_count²` land
    ///   cells and asks the landblock info for each cell's restriction iid -- a
    ///   `PackableHashTable<cell_id, iid>` in the CELL dat's landblock info, present when the
    ///   `num_buildings` dword's high half has bit 0 (`dereth_assets::world::LandBlockInfo::
    ///   restrictions`);
    /// * where an interior cell carries its own `restriction_obj` behind
    ///   flag bit 3 (`dereth_assets::world::EnvCell::restriction_obj`).
    ///
    /// Measured over the retail cell data: **1,293** landblock infos carry a
    /// restriction table, **7,140** land-cell entries between them, and **103,766** env cells
    /// carry one of their own.
    pub fn set_cell_restriction(&mut self, cell: CellId, obj: Option<ObjectId>) {
        self.cell_mut(cell).restriction_obj = obj;
    }

    /// Installs (or, with `None`, removes) the host's answer to the entry-restriction question
    /// (see [`crate::transition::EntryRestrictionHost`]). Every transition run afterwards
    /// asks it in place of the gate keeper's pushed record. The client never calls this.
    pub fn set_entry_restriction_host(
        &mut self,
        host: Option<Arc<dyn crate::transition::EntryRestrictionHost>>,
    ) {
        self.entry_host = host;
    }

    /// Movement-restriction half of the object's game record.
    ///
    /// Pushed rather than queried: physics owns no game records, so the client
    /// re-pushes whenever a `0x0248 House_UpdateRestrictions` lands, whenever a
    /// `0xF745 CreateObject` brings a house in, and whenever the local player's own description
    /// changes.
    pub fn set_weenie_restrictions(
        &mut self,
        h: PhysHandle,
        w: Option<crate::obj::WeenieRestrictions>,
    ) {
        if let Some(o) = self.objects.get_mut(h) {
            o.weenie = w;
        }
    }

    /// Enter a cell without the voyeur notification (the detection pass sends that).
    pub fn enter_cell(&mut self, h: PhysHandle, cell: CellId) {
        let rt = self.cell_mut(cell);
        if !rt.object_list.contains(&h) {
            rt.object_list.push(h);
        }
        if let Some(o) = self.objects.get_mut(h) {
            o.cell = Some(cell);
            o.position.cell = cell;
        }
    }

    /// Leave the current cell.
    pub fn leave_cell(&mut self, h: PhysHandle) {
        if let Some(o) = self.objects.get_mut(h) {
            let cell = o.cell.take();
            if let Some(c) = cell {
                if let Some(rt) = self.cells.get_mut(&c.0) {
                    rt.object_list.retain(|x| *x != h);
                }
            }
        }
    }

    /// Remove the object's shadows from their cells.
    pub fn remove_shadows_from_cells(&mut self, h: PhysHandle) {
        let shadows: Vec<CellId> = self
            .objects
            .get(h)
            .map(|o| o.shadow_objects.iter().map(|s| s.cell_id).collect())
            .unwrap_or_default();
        for c in shadows {
            if let Some(rt) = self.cells.get_mut(&c.0) {
                rt.shadow_object_list.retain(|x| *x != h);
            }
        }
        if let Some(o) = self.objects.get_mut(h) {
            o.shadow_objects.clear();
        }
    }

    /// Releases the shadows that reach into one landblock's outdoor cells, for a landblock that
    /// is being unloaded: every body overlapping one of its land cells forgets that cell, and the
    /// cell's collision list is emptied. Bodies whose origin was in the landblock have left the
    /// world before this runs; what remains are bodies in a neighbouring landblock whose geometry
    /// reached across the boundary, which would otherwise keep a shadow in a cell that is gone
    /// (and stay listed in it when the landblock loads again). Interior cells are not touched.
    pub fn release_shadows_into_landblock(&mut self, landblock: dereth_primitives::LandblockId) {
        let first = landblock.cell(1).0;
        let last = landblock.cell(0x40).0; // the 8 x 8 land cells are indices 1..=0x40
        for (&cell_id, rt) in self.cells.range_mut(first..=last) {
            for h in rt.shadow_object_list.drain(..) {
                if let Some(o) = self.objects.get_mut(h) {
                    o.shadow_objects.retain(|s| s.cell_id.0 != cell_id);
                }
            }
        }
    }

    /// Recompute the crossed cells and re-add the shadows.
    ///
    /// A `PARTICLE_EMITTER_PS` object registers a single "particle shadow" in its own cell rather
    /// than a full shadow set.
    pub fn calc_cross_cells(&mut self, h: PhysHandle, do_not_load_cells: bool) {
        self.remove_shadows_from_cells(h);
        let Some(o) = self.objects.get(h) else { return };
        if o.state.is_particle_emitter() {
            let Some(cell) = o.cell else { return };
            if let Some(obj) = self.objects.get_mut(h) {
                obj.shadow_objects.push(crate::obj::ShadowObj {
                    cell_id: cell,
                    cell_present: true,
                });
            }
            self.cell_mut(cell).shadow_object_list.push(h);
            return;
        }
        let pos = o.position;
        let scale = o.scale;

        // **Arm 1: state bit `0x10000` -- `HAS_PHYSICS_BSP_PS`.** Any object whose parts carry a
        // physics mesh goes to the part-array registration path and
        // never looks at a sphere at all. Without this arm every mesh object -- 530 of the
        // 5,935 shipped setups, doors included -- would be registered from a sphere the client
        // never consults, and a door whose mesh spans two cells would occupy one.
        if o.state.has_physics_bsp() {
            // Registration reads each part's live position, so it sees
            // the same animated pose the collision walk does. A door registered from its
            // placement frame claims the cells a third-open door spans, not the ones it is in.
            let frames = o.part_frames.clone();
            let frames = frames.as_deref().map(Vec::as_slice);
            let parts: Vec<crate::source::PhysicsPart> = (0..o.geometry.parts.len())
                .filter_map(|i| o.geometry.placed_part_posed(i, &pos, scale, frames))
                .collect();
            let mut arr = crate::cell::CellArray::new();
            arr.do_not_load_cells = do_not_load_cells;
            {
                let resolver = CellResolver::new(&*self.land);
                resolver.find_bbox_cell_list(&pos, &parts, &mut arr);
            }
            self.store_shadow_set(h, &arr);
            return;
        }

        // **Arm 2.** A part array with any cylinder spheres hands **all** of them -- the low
        // point as the centre and the radius unchanged, with height discarded -- to the
        // count-taking cell-list search, which clamps the count to **10**.
        //
        // **Arm 3.** Everything else uses the setup's sorting sphere with no radius test of any
        // kind. A setup without a part array also pushes a count of `1`. **There is no path-sphere
        // arm**: an `if sorting.radius > 0 { .. } else { path_spheres() }` fallback would change
        // the shadow set of 163 shipped setups (a sphere list, zero sorting sphere, no
        // cylinder-spheres and no part BSP).
        let spheres: Vec<crate::geom::Sphere> = if o.geometry.cyl_spheres.is_empty() {
            let sorting = o.geometry.sorting_sphere;
            vec![crate::geom::Sphere::new(
                sorting.center.mul(scale),
                sorting.radius * scale,
            )]
        } else {
            o.geometry
                .cyl_spheres
                .iter()
                .take(crate::cell::MAX_CELL_LIST_SPHERES)
                .map(|c| crate::geom::Sphere::new(c.low_pt.mul(scale), c.radius * scale))
                .collect()
        };
        // The spheres must be in the position's block space, which is where find_cell_list works.
        let m = math::l2g(pos.frame.rotation);
        let spheres: Vec<crate::geom::Sphere> = spheres
            .into_iter()
            .map(|s| {
                crate::geom::Sphere::new(
                    math::localtoglobalvec(m, s.center).add(pos.frame.origin),
                    s.radius,
                )
            })
            .collect();

        let mut arr = crate::cell::CellArray::new();
        arr.do_not_load_cells = do_not_load_cells;
        let mut interior = false;
        {
            let resolver = CellResolver::new(&*self.land);
            resolver.find_cell_list(&pos, &spheres, &mut arr, false, &mut interior);
        }
        self.store_shadow_set(h, &arr);
    }

    /// The tail all three arms of
    /// [`Self::calc_cross_cells`] share.
    fn store_shadow_set(&mut self, h: PhysHandle, arr: &crate::cell::CellArray) {
        let entries: Vec<(CellId, bool)> = arr
            .cells
            .iter()
            .map(|c| (c.cell_id, c.cell.is_some()))
            .collect();
        if let Some(obj) = self.objects.get_mut(h) {
            obj.shadow_objects = entries
                .iter()
                .map(|(id, present)| crate::obj::ShadowObj {
                    cell_id: *id,
                    cell_present: *present,
                })
                .collect();
        }
        for (id, present) in entries {
            if present {
                let rt = self.cell_mut(id);
                if !rt.shadow_object_list.contains(&h) {
                    rt.shadow_object_list.push(h);
                }
            }
        }
    }

    /// Returns true if the 30 Hz gate opened and objects stepped.
    ///
    /// `blocking_for_cells` reproduces the prefetch freeze: while a landblock prefetch is in
    /// flight the **whole physics tick is skipped**, which is why the world freezes during a
    /// portal transition rather than continuing to simulate.
    pub fn use_time(&mut self, now: LocalTime, blocking_for_cells: bool) -> bool {
        if blocking_for_cells {
            return false;
        }
        let elapsed = now.0 - self.last_physics_time;
        if elapsed < 0.0 {
            // The clock went backwards.
            self.last_physics_time = now.0;
            return false;
        }
        if elapsed < globals::MIN_QUANTUM {
            // Below the gate nothing in physics moves at all, and the residual elapsed time is
            // carried forward because last_physics_time is NOT updated.
            return false;
        }

        // Update order is LongHash bucket order over the object table: not sorted, not
        // player-first. It decides which of two simultaneous collisions is reported first.
        for key in self.object_table.keys_in_order() {
            let Some(h) = self.object_table.get(key).copied() else {
                continue;
            };
            if !self.objects.contains(h) {
                continue;
            }
            self.update_object(h, now);
            if self.player == Some(h) {
                self.notices.push(PhysicsNotice::PlayerPhysicsUpdated);
            }
        }
        self.last_physics_time = now.0;
        // animate_static_object and texture-velocity updates are rendering-side; the list is kept
        // so the application can drive them.
        true
    }

    /// The per-object time stepping.
    pub fn update_object(&mut self, h: PhysHandle, now: LocalTime) {
        {
            let Some(o) = self.objects.get(h) else { return };
            if o.parent.is_some() || o.cell.is_none() || o.state.is_frozen() {
                if let Some(o) = self.objects.get_mut(h) {
                    o.transient_state.set_active_bit(false);
                }
                return;
            }
        }

        if let Some(p) = self.player {
            if let (Some(pl), Some(o)) = (self.objects.get(p), self.objects.get(h)) {
                let v = math::get_offset(&pl.position, &o.position);
                let d = v.mag2().sqrt();
                let within = d <= globals::ACTIVE_RADIUS || !self.obj_maint_is_active;
                if let Some(o) = self.objects.get_mut(h) {
                    o.player_vector = v;
                    o.player_distance = d;
                }
                if within {
                    if let Some(o) = self.objects.get_mut(h) {
                        o.set_active(true, now.0);
                    }
                } else if let Some(o) = self.objects.get_mut(h) {
                    o.transient_state.set_active_bit(false);
                }
            }
        }

        let update_time = self.objects.get(h).map_or(0.0, |o| o.update_time);
        let mut elapsed = now.0 - update_time;
        // The physics timer's current time starts at the object's own clock, not the wall clock.
        self.sim_time = update_time;

        if elapsed <= globals::MIN_STEP {
            if let Some(o) = self.objects.get_mut(h) {
                o.update_time = now.0;
            }
            return;
        }
        if elapsed > globals::HUGE_QUANTUM {
            if let Some(o) = self.objects.get_mut(h) {
                o.update_time = now.0;
            }
            return;
        }

        let mut done = false;
        if elapsed > globals::MAX_QUANTUM {
            loop {
                self.sim_time += globals::MAX_QUANTUM;
                self.update_object_internal(h, globals::MAX_QUANTUM, now);
                elapsed -= globals::MAX_QUANTUM;
                if elapsed <= globals::MAX_QUANTUM {
                    break;
                }
            }
            // This test is INSIDE the branch. Hoisting it out changes how far every
            // falling object travels.
            if elapsed <= globals::MIN_QUANTUM {
                done = true;
            }
        }
        if !done {
            self.sim_time += elapsed;
            self.update_object_internal(h, elapsed, now);
        }
        // update_time takes the SIMULATED clock, not `now`.
        if let Some(o) = self.objects.get_mut(h) {
            o.update_time = self.sim_time;
        }
    }

    /// One physics sub-step.
    fn update_object_internal(&mut self, h: PhysHandle, quantum: f64, now: LocalTime) {
        let active = self
            .objects
            .get(h)
            .is_some_and(|o| o.transient_state.is_active());
        if active {
            let Some(cell) = self.objects.get(h).and_then(|o| o.cell) else {
                return;
            };
            // When `transient_state & 0x100` is set, clear ethereal (`set_ethereal` off, no report).
            //
            // **The retry.** This is the real call, not an unconditional clear: clearing the bit
            // anyway is the opposite of what the client does, and an object that could not
            // become solid because someone was standing in it would be made solid on the very
            // next sub-step, on top of them. The pending bit survives until
            // [`Self::ethereal_check_for_collisions`] is actually clear, and this
            // line is what makes `CHECK_ETHEREAL_TS` a retry rather than a tombstone.
            if self
                .objects
                .get(h)
                .is_some_and(|o| o.transient_state.check_ethereal())
            {
                let _ = self.set_ethereal(h, false, false);
            }
            if let Some(o) = self.objects.get_mut(h) {
                o.jumped_this_frame = false;
            }

            let mut new_frame = Frame::new(Vec3::ZERO, Quat::IDENTITY);
            self.update_position_internal(h, quantum, &mut new_frame);
            let new_pos = Position::new(cell, new_frame);

            let has_geometry = self.objects.get(h).is_some_and(|o| {
                !o.geometry.path_spheres().is_empty()
                    && o.motion.as_ref().is_none_or(|m| m.has_collision_geometry())
            });
            let same_origin = self
                .objects
                .get(h)
                .is_some_and(|o| new_pos.frame.origin.eq_eps(o.position.frame.origin));

            if !has_geometry {
                if let Some(o) = self.objects.get_mut(h) {
                    if o.motion.is_none() && o.transient_state.on_walkable() {
                        o.transient_state.set_active_bit(false);
                    }
                    // The requested origin is **overwritten** with the object's own
                    // before `set_frame`, so an object with no collision geometry rotates and
                    // never translates. See [`Self::keep_origin`].
                    let kept = Self::keep_origin(o.position.frame, new_pos.frame);
                    o.set_frame(kept);
                    o.cached_velocity = Vec3::ZERO;
                }
            } else if same_origin {
                if let Some(o) = self.objects.get_mut(h) {
                    o.set_frame(new_pos.frame);
                    o.cached_velocity = Vec3::ZERO;
                }
            } else {
                let mut target = new_pos;
                if let Some(o) = self.objects.get(h) {
                    if o.state.is_alignpath() {
                        let mut dir = target.frame.origin.sub(o.position.frame.origin);
                        if !dir.normalize_check_small() {
                            let mut f = target.frame;
                            math::set_vector_heading(&mut f, dir);
                            target.frame = f;
                        }
                    } else if o.state.is_sledding() && !o.velocity_vector.is_zero() {
                        let mut f = target.frame;
                        let v = o.velocity_vector;
                        let heading = {
                            let mut probe = Frame::default();
                            math::set_vector_heading(&mut probe, v);
                            math::get_heading(&probe)
                        };
                        math::set_heading(&mut f, heading);
                        target.frame = f;
                    }
                }
                let from = self.objects.get(h).map(|o| o.position);
                if let Some(from) = from {
                    match self.transition(h, &from, &target, false) {
                        None => {
                            if let Some(o) = self.objects.get_mut(h) {
                                // The **origin is put back**, the rotation is kept,
                                // and the cached velocity is zeroed. See [`Self::keep_origin`] --
                                // this is the whole of the back-pressure a refused sweep applies,
                                // and without it a body whose transition fails advances with no
                                // collision test having run at all.
                                let kept = Self::keep_origin(o.position.frame, target.frame);
                                o.set_frame(kept);
                                o.cached_velocity = Vec3::ZERO;
                            }
                        }
                        Some(t) => {
                            let achieved = math::get_offset(&from, &t.sphere_path.curr_pos);
                            #[allow(clippy::cast_possible_truncation)]
                            let inv = (1.0 / quantum) as f32;
                            if let Some(o) = self.objects.get_mut(h) {
                                o.cached_velocity = achieved.mul(inv);
                            }
                            self.set_position_internal(h, &t);
                        }
                    }
                }
            }

            // Motion runs *after* the move, so the offset it computes is consumed by the next
            // sub-step.
            if let Some(o) = self.objects.get_mut(h) {
                o.sync_motion_physics_state();
                if let Some(m) = o.motion.as_mut() {
                    m.tick_movement(now);
                }
            }
            // The position manager's time step is the last call in the sub-step, after movement
            // time and part-array movement handling (which the motion seam's `tick_movement`
            // above represents).
            self.position_use_time(h);
        }
    }

    /// The requested frame with its **origin put back** — the internal object-update rule
    /// for every arm that does not commit a transition.
    ///
    /// Both non-transition arms restore all three components of the requested frame's
    /// origin from the object's current origin before installing the frame: a failed
    /// sweep and the no-collision-geometry case. The requested rotation is retained.
    /// ACE's public `PhysicsObj.UpdateObjectInternal` expresses the same assignment,
    /// `newPos.Frame.Origin = Position.Frame.Origin`, in both arms.
    ///
    /// So a refused move keeps the **rotation** it asked for and loses the **translation**.
    /// Accepting both lets a body walk a whole 39.866 m without one collision test ever running,
    /// straight through a chest and the walls behind it.
    const fn keep_origin(current: Frame, requested: Frame) -> Frame {
        Frame {
            origin: current.origin,
            ..requested
        }
    }

    /// One position sub-step.
    fn update_position_internal(&mut self, h: PhysHandle, quantum: f64, out: &mut Frame) {
        let Some(o) = self.objects.get_mut(h) else {
            return;
        };
        o.sync_motion_physics_state();
        let mut offset = Frame::new(Vec3::ZERO, Quat::IDENTITY);
        let hidden = o.state.is_hidden();
        if !hidden {
            let scale = if o.transient_state.on_walkable() {
                o.scale
            } else {
                0.0
            };
            if let Some(m) = o.motion.as_mut() {
                offset = m.advance(quantum);
            }
            // The animation origin is scaled by scale on the ground and by 0.0 in the air; the
            // rotation is never zeroed.
            offset.origin = offset.origin.mul(scale);
        }
        // Position-manager adjustments run after animation scaling: they are neither scaled
        // by the object nor zeroed in the air, and they participate in collision. The order
        // is interpolation, sticky adjustment, then constraint adjustment.
        //
        // Interpolation **overwrites** the offset frame instead of adding to it, so a server
        // correction replaces the animation offset. That lets a corrected remote body glide
        // instead of fighting its walk cycle. Sticky and constraint adjustments belong to
        // the animation seam in this build.
        if let Some(m) = self.interpolation.get_mut(&h) {
            // The interpolator reads the motion interpreter's `get_adjusted_max_speed` result
            // **inside** `adjust_offset`, so it is fresh on every sub-step:
            // a body that starts running part-way through a correction is corrected faster from
            // that sub-step on. `None` is retail's no-motion-interpreter arm, which is what
            // leaves the `7.5`. The arithmetic stays in the interpreter; this asks it. Without
            // the query every body would walk at the fallback.
            let use_adjusted = m.use_adjusted_speed;
            m.max_speed = o
                .motion
                .as_ref()
                .and_then(|s| s.motion_max_speed(use_adjusted));
            m.adjust_offset(
                &o.position,
                o.transient_state.in_contact(),
                quantum,
                &mut offset,
                // The omitted suppression query reads the *sticky* sub-manager of the same
                // position manager. That one lives on the animation
                // seam here and no query crosses back, so the failure detector is never
                // suppressed; a body that is both stuck to something and being corrected is the
                // only case that differs, and it gives up on the node a window sooner.
                false,
            );
        }
        if let Some(m) = o.motion.as_mut() {
            m.adjust_position_offset(&mut offset, quantum);
        }
        *out = math::combine(&o.position.frame, &offset);
        if !hidden {
            o.update_physics_internal(quantum, out);
        }
        if let Some(m) = o.motion.as_mut() {
            m.process_hooks();
        }
    }

    /// Build the initial `ObjectInfo` state and seed
    /// the transition's contact planes.
    fn get_object_info(&self, o: &PhysicsObj, t: &mut Transition, for_placement: bool) -> u32 {
        let mut s = if o.state.can_edge_slide() {
            ObjectInfoState::EDGE_SLIDE
        } else {
            0
        };
        if !for_placement {
            if o.transient_state.in_contact() {
                if o.check_contact(true) {
                    t.init_contact_plane(
                        o.contact_plane_cell_id,
                        o.contact_plane,
                        o.transient_state.in_water_contact(),
                    );
                    s |= ObjectInfoState::CONTACT;
                    if o.transient_state.on_walkable() {
                        s |= ObjectInfoState::ON_WALKABLE;
                    }
                } else {
                    t.init_last_known_contact_plane(
                        o.contact_plane_cell_id,
                        o.contact_plane,
                        o.transient_state.in_water_contact(),
                    );
                }
            }
            if o.transient_state.is_sliding() {
                t.init_sliding_normal(o.sliding_normal);
            }
        }
        if o.geometry.allow_free_heading {
            s |= ObjectInfoState::FREE_ROTATE;
        }
        if o.state.is_missile() {
            s |= ObjectInfoState::PATH_CLIPPED;
        }
        s
    }

    /// Run the collision transition and then find a valid position.
    ///
    /// Returns `None` when the ten-deep pool is exhausted; the caller **must** then abort the
    /// move.
    pub fn transition(
        &mut self,
        h: PhysHandle,
        from: &Position,
        to: &Position,
        admin: bool,
    ) -> Option<Transition> {
        self.run_transition(h, TransitionSeed::Sweep { from: *from, admin }, to)
    }

    /// The position set, past the internal setter's own guard -- **put the object at `pos` and
    /// commit the answer**.
    ///
    /// This is the whole of what a teleport does, and both of the client's arrivals funnel into
    /// it:
    ///
    /// * player teleport requests a position set with flags `0x1012` =
    ///   `TELEPORT_SPF | SLIDE_SPF` plus the send-position-event bit; the corresponding
    ///   non-teleport request uses `0x1002`. This is the portal/`@teleloc` arrival.
    /// * enter-world stores the position, then requests placement with flags `0x11` =
    ///   `PLACEMENT_SPF | SLIDE_SPF`. This is the login arrival.
    ///
    /// The force-into-cell path is a bypass for an object whose game record grants one
    /// of the three non-collision exemptions. It performs no transition.
    ///
    /// The placement seeds the transition with `init_object(this, 0)` —
    /// a **literal zero** state word, so no contact plane and no `ON_WALKABLE` are carried in —
    /// and initializes the path from `cell` and `pos` with no beginning object. The null `begin`
    /// makes it a **placement** rather than a sweep. Both facts
    /// are why the answer is a fresh reading of the destination rather than a continuation of
    /// wherever the object was standing.
    ///
    /// `SLIDE_SPF` is set on both arrival paths, so the contact-plane clear and epsilon
    /// recheck are both skipped. The achieved position from placement `step_down` is
    /// accepted. The observed predicate tests bit `0x10`; that bit **suppresses** both
    /// operations. `SLIDE_SPF` is this tree's descriptive name for the bit.
    ///
    /// Returns whether the position was committed. A failed position check
    /// ([`SetPositionError::NoValidPosition`] / [`SetPositionError::Collided`]) or exhaustion of
    /// the ten-entry transition pool returns false and leaves the prior position intact, as an
    /// object update does.
    pub fn set_position(&mut self, h: PhysHandle, pos: &Position) -> bool {
        self.resolve_cell_and_place(h, pos).is_ok()
    }

    /// The internal set-position path — the half of `SetPosition` that decides
    /// **which cell the placement starts from**.
    ///
    /// `SetPosition` -> scatter arms (not modelled
    /// here: no caller in this tree sets `0x100`/`0x200`) -> this. In order:
    ///
    /// 1. With no cell, `prepare_to_enter_world`. Not here — see
    ///    the note in the body: the preparation uses a wall clock and two object-maintenance
    ///    lists, and its tail lives at the client seam as
    ///    `ObjectPhysics::prepare_reentry`.
    /// 2. **`AdjustPosition(pos, <first local sphere's center>, &cell, flags>>5 & 1, 1)`.**
    ///    This is the resolver: for an interior id it descends through visible child cells, and
    ///    for an outdoor id it runs `adjust_to_outside`
    ///    and answers with the land cell. **It is the destination's cell, not the object's.**
    /// 3. A `NULL` answer is the lost-cell arm: `prepare_to_leave_visibility`,
    ///    `store_position`, the object-maintenance lost-cell operation, and clearing `ACTIVE_TS` —
    ///    and it still
    ///    returns `OK_SPE`.
    /// 4. Three game-record queries divert to the force path.
    ///    No game record exists in this tree, so the tests
    ///    are all `false` and the call is never reached. That bypass is [`Self::force_into_cell`].
    /// 5. `CheckPositionInternal(this, cell, pos, t, sps)` with the cell from (2),
    ///    then commits it.
    ///
    /// Returns the error the client would have returned, so `enter_world`'s success test is the
    /// client's `result == OK_SPE` rather than a re-reading of the object afterwards.
    fn resolve_cell_and_place(
        &mut self,
        h: PhysHandle,
        pos: &Position,
    ) -> Result<(), SetPositionError> {
        // The sphere path's local sphere — `SetPosition` has already run `init_sphere`, so
        // this is the object's first path sphere, or the dummy sphere's centre when it has none.
        let center = self.objects.get(h).map_or(Vec3::ZERO, |o| {
            o.geometry
                .path_spheres()
                .first()
                .map_or(Sphere::dummy().center, |s| s.center)
        });
        // The no-cell `prepare_to_enter_world` step is **not** written
        // here, and that is deliberate. Its behavior is an `update_time` assignment from current
        // time plus two object-maintenance list removals; this crate has neither a wall
        // clock nor an object-maintenance layer, and its tail already exists at the client seam as
        // `ObjectPhysics::prepare_reentry`, which writes `update_time` and `set_active` from the
        // caller's `now`. Writing `last_physics_time` here instead would **clobber** it: the
        // clock would read 0.0 where the seam had just written 1002.0.
        let Some((adjusted, cell)) = self.adjust_position(pos, center) else {
            // `AdjustPosition` answering 0 leaves the out-cell NULL, so this falls into the same
            // lost-cell arm rather than running a transition against a cell nobody resolved.
            self.goto_lost_cell(h, pos);
            return Ok(());
        };
        let Some(cell) = cell else {
            self.goto_lost_cell(h, &adjusted);
            return Ok(());
        };
        let Some(t) = self.run_transition(
            h,
            TransitionSeed::Placement {
                begin_cell: Some(cell),
            },
            &adjusted,
        ) else {
            return Err(SetPositionError::NoValidPosition);
        };
        if t.sphere_path.curr_cell.is_none() {
            // No current cell on the sphere path returns the no-cell error.
            return Err(SetPositionError::NoCell);
        }
        self.set_position_internal(h, &t);
        Ok(())
    }

    /// The no-cell arm of position setting:
    /// the destination resolved to no cell at all, so the object leaves the world and keeps the
    /// position it was asked for. The object-maintenance lost-cell operation has no counterpart here.
    fn goto_lost_cell(&mut self, h: PhysHandle, pos: &Position) {
        self.leave_cell(h);
        if let Some(o) = self.objects.get_mut(h) {
            o.store_position(pos);
            o.transient_state.set_active_bit(false);
        }
    }

    /// Enter-world with a position — **the create path's arrival** — and the
    /// only way an object that is not in the world yet is meant to get into a cell.
    ///
    /// The position-taking path stores the position before attempting entry. Entry then:
    ///
    /// * refuses an object with a parent, because its holder places it;
    /// * restarts the object's clock;
    /// * builds `SetPositionStruct` from the **stored, normalized position**, not the
    ///   caller's unnormalized `Position`, with flags `0x11` = `PLACEMENT_SPF | SLIDE_SPF`;
    /// * on `OK_SPE` only, sets `ACTIVE_TS` (`0x80`) for a non-`STATIC_PS` object, removes
    ///   link animations and notifies the movement manager of world entry.
    ///
    /// **This is not `force_into_cell`.** The bypass takes the wire's `objcell_id` verbatim, so an
    /// object created at an **outdoor** landcell while standing inside a building would keep the
    /// outdoor cell forever. The outdoor draw would then render that cell's objects
    /// inside — before the `Clear(4)` and before the
    /// interior is painted over the top.
    ///
    /// Returns `true` when placement succeeds with `OK_SPE`, and `false` otherwise.
    pub fn enter_world(&mut self, h: PhysHandle, pos: &Position) -> bool {
        // An arriving physics object has no position manager at all -- the one the body carried
        // went with the object that object maintenance deleted through the exit-world path. This
        // build re-uses the *same* body across a merge-create and across a leave/return, so the
        // clearing has to happen on the way in instead. Without it a player who walked out of
        // view with a correction still queued walks back **off** his return position.
        self.stop_interpolating(h);
        {
            let Some(o) = self.objects.get_mut(h) else {
                return false;
            };
            // Store the position before anything else looks at the cell.
            o.store_position(pos);
            if o.parent.is_some() {
                return false;
            }
            // Storing current time in `update_time` is the same
            // seam's, for the same reason: see the note in `resolve_cell_and_place`.
        }
        let target = self.objects.get(h).map(|o| o.position);
        let Some(target) = target else { return false };
        if self.resolve_cell_and_place(h, &target).is_err() {
            return false;
        }
        if let Some(o) = self.objects.get_mut(h) {
            if !o.state.is_static() {
                o.transient_state.set_active_bit(true);
            }
        }
        // **Two residuals, named rather than silently dropped.** Removing the link animations
        // and handing the movement manager an enter-world
        // both belong to seams this crate does not own: the part array is the animation crate's
        // and `MotionSource` has no `HandleEnterWorld`. Neither is reachable for an object created
        // by the create-object handler in this tree, which attaches no motion manager to a server
        // object — [`ObjectPhysics::spawn`] creates a body and a `SetupGeometry` and stops there.
        true
    }

    fn run_transition(
        &mut self,
        h: PhysHandle,
        seed: TransitionSeed,
        to: &Position,
    ) -> Option<Transition> {
        let idx = self.pool.make()?;
        let mut t = self.pool.take(idx);
        t.init();

        let (state, spheres, scale, mover_state, frames_stationary) = {
            let Some(o) = self.objects.get(h) else {
                self.pool.put(idx, t);
                self.pool.cleanup();
                return None;
            };
            // A sweep builds the state word from the object and carries its live contact plane
            // into the transition. A placement does **not**: it passes a literal `0` to object
            // initialization. That is why a
            // placement's answer about contact is a fresh reading of the destination rather than
            // a continuation of wherever the object was standing.
            let s = match seed {
                TransitionSeed::Sweep { admin, .. } => self.get_object_info(o, &mut t, admin),
                TransitionSeed::Placement { .. } => 0,
            };
            let spheres: Vec<crate::geom::Sphere> = if o.geometry.path_spheres().is_empty() {
                vec![crate::geom::Sphere::dummy()]
            } else {
                o.geometry.path_spheres().to_vec()
            };
            // The stationary-fall counter is re-seeded into a fresh transition from the transient
            // state each time.
            let ts = o.transient_state;
            let f = if ts.is_stationary_stuck() {
                3
            } else if ts.is_stationary_stop() {
                2
            } else if ts.is_stationary_fall() {
                1
            } else {
                0
            };
            (s, spheres, o.scale, o.state, f)
        };
        if let Some(o) = self.objects.get(h) {
            t.init_object(o, h, state);
        }
        t.collision_info.frames_stationary_fall = frames_stationary;
        t.sphere_path.init_sphere(&spheres, scale);
        // init_path takes the object's own cell, not the `from` position's.
        let own_cell = self.objects.get(h).and_then(|o| o.cell);
        match seed {
            TransitionSeed::Sweep { from, .. } => {
                t.sphere_path.init_path(own_cell, Some(from), to);
                t.sphere_path.set_check_pos(&from, own_cell);
            }
            // A placement initializes the transition path from the resolved destination cell and
            // position with no beginning object. A null beginning marks `PLACEMENT_INSERT` and is
            // the whole difference between a teleport and a step.
            //
            // The `set_check_pos` here is **provably overwritten before it is read**:
            // Position validation first assigns `check_pos = curr_pos; check_cell = curr_cell`.
            // The initial write is kept because leaving a
            // transition half-initialised on one arm and not the other is how a later reader
            // acquires a wrong belief about which fields a placement may depend on.
            //
            // **`begin_cell`, not `own_cell`.** The placement path receives the cell resolved from
            // the *destination* and passes that to `init_path`. Seeding from the object's own cell is a
            // different function, and it refuses every placement for an object that has no cell
            // yet — which is every object the create path is about.
            TransitionSeed::Placement { begin_cell } => {
                t.sphere_path.init_path(begin_cell, None, to);
                t.sphere_path.set_check_pos(to, begin_cell);
            }
        }

        let ok = {
            let ctx = TransitionCtx {
                land: &*self.land,
                objects: &self.objects,
                cells: &self.cells,
                mover: Some(h),
                object_table: Some(&self.object_table),
                entry_host: self.entry_host.as_deref(),
            };
            find_valid_position(&ctx, &mut t, mover_state)
        };
        self.pool.cleanup();
        if t.kill_velocity {
            if let Some(o) = self.objects.get_mut(h) {
                o.velocity_vector = Vec3::ZERO;
            }
        }
        // **The house barrier's contact effect.** Raised here, from the transition
        // itself, rather than from `handle_all_collisions`: retail plays it during the entry-
        // restriction check, which runs *during* the sweep and therefore
        // sparks whether or not `find_valid_position` ends up accepting a position, while
        // `handle_all_collisions` runs only out of `set_position_internal` and only on success.
        //
        // The barrier-effect intensity defaults to 0.5. When the mover's achieved velocity
        // is available, intensity is `clamp(|v| * 0.1, 0, 1)`. The script belongs to the
        // **barrier**, not the mover. The notice carries intensity and barrier id so the
        // client can resolve and play that script.
        //
        // **One notice per transition, versus one effect per refused check in the original
        // client.** Collision information accumulates across sub-steps, so four consecutive
        // refusals produce one notice here but four script plays in the original client. The
        // only observable is a particle emitter the script manager restarts either way, and the
        // alternative — a notice queue reachable from `check_entry_restrictions`, which holds only
        // a `&TransitionCtx` — would have cost the whole crate a mutable borrow. Declared here
        // rather than hidden.
        if let Some(restriction) = t.collision_info.restricted_by {
            if let Some(o) = self.objects.get(h) {
                // The client reads the **achieved** velocity, `cached_velocity`.
                let intensity = (o.cached_velocity.mag2().sqrt() * 0.1).clamp(0.0, 1.0);
                let object = o.id;
                self.notices.push(PhysicsNotice::MoveRestricted {
                    object,
                    restriction_obj: restriction,
                    intensity,
                });
            }
        }
        let out = std::mem::take(&mut t);
        self.pool.put(idx, t);
        // Transition construction returns NULL both when the pool is exhausted and
        // when find_valid_position fails, and `UpdateObjectInternal` treats the two the same.
        //
        // The object update does **not** accept the requested frame anyway: it puts the object's
        // own origin back before `set_frame`, so a refused sweep keeps its rotation and loses its
        // translation. See [`Self::keep_origin`].
        if ok {
            Some(out)
        } else {
            None
        }
    }

    /// Commit a finished transition to its object: the set-position-internal step that the
    /// object update and the placement run on their own transitions.
    ///
    /// Public for a host that runs a sweep with [`Self::transition`] and then decides to keep it
    /// (a server moving a body to a position its client reported). It commits exactly what the
    /// internal step commits, in the same order: the frame and cell (or the lost-cell arm when
    /// the sweep ended in no cell), the contact plane and its cell, the contact, water-contact,
    /// on-walkable and sliding state, every collision the sweep reported (queued as
    /// [`PhysicsNotice`]s), and the shadows. The client never calls it: its own update and
    /// placement reach the same step privately, so nothing on the client path changes.
    pub fn commit_transition(&mut self, h: PhysHandle, t: &Transition) {
        self.set_position_internal(h, t);
    }

    /// Commit a completed transition.
    fn set_position_internal(&mut self, h: PhysHandle, t: &Transition) {
        let Some(curr_cell) = t.sphere_path.curr_cell else {
            // The object left every known cell.
            if let Some(o) = self.objects.get_mut(h) {
                let pos = t.sphere_path.curr_pos;
                o.store_position(&pos);
                o.transient_state.set_active_bit(false);
            }
            return;
        };
        let old_transient = self
            .objects
            .get(h)
            .map_or(TransientState::default(), |o| o.transient_state);

        let same_cell = self.objects.get(h).and_then(|o| o.cell) == Some(curr_cell);
        if same_cell {
            if let Some(o) = self.objects.get_mut(h) {
                o.position.cell = t.sphere_path.curr_pos.cell;
            }
        } else {
            self.leave_cell(h);
            self.enter_cell(h, curr_cell);
        }

        let ci = &t.collision_info;
        if let Some(o) = self.objects.get_mut(h) {
            o.set_frame(t.sphere_path.curr_pos.frame);
            o.contact_plane = ci.contact_plane;
            o.contact_plane_cell_id = ci.contact_plane_cell_id;
            o.transient_state.set_contact(ci.contact_plane_valid);
            o.calc_acceleration();
            o.transient_state
                .set_water_contact(ci.contact_plane_is_water);
            if o.transient_state.in_contact() {
                // The client does `set_on_walkable(normal.z >= floor_z)` here, against the global
                // floor Z.
                //
                // **No test in this tree observes the test itself.** Replacing it with a literal
                // `true` survives the physics library tests and the client's interior walks
                // (eighty walks through real interior geometry), because no station leaves a
                // body *in contact with* a plane steeper than `floor_z`: `interiors`' bodies are
                // stopped by walls, which is `check_walkable`'s and `step_up`'s arm and never
                // reaches here. A station that rests a body on a steep slope is what this line
                // needs.
                let walkable = ci.contact_plane.normal.z >= globals::FLOOR_Z;
                o.set_on_walkable(walkable);
            } else {
                o.set_on_walkable(false);
                o.calc_acceleration();
            }
            o.sliding_normal = ci.sliding_normal;
            o.transient_state.set_sliding(ci.sliding_normal_valid);
        }

        self.handle_all_collisions(
            h,
            t,
            old_transient.in_contact(),
            old_transient.on_walkable(),
        );

        if self.objects.get(h).and_then(|o| o.cell).is_some() && !t.cell_array.is_empty() {
            self.calc_cross_cells(h, false);
        }
    }

    /// Handle every collision the transition reported.
    fn handle_all_collisions(
        &mut self,
        h: PhysHandle,
        t: &Transition,
        was_contact: bool,
        was_on_walkable: bool,
    ) {
        let ci = &t.collision_info;
        let (allow_bounce, id, velocity, elasticity, state) = {
            let Some(o) = self.objects.get(h) else { return };
            let allow =
                !was_on_walkable || !o.transient_state.on_walkable() || o.state.is_sledding();
            (allow, o.id, o.velocity_vector, o.elasticity, o.state)
        };

        // **The missile clear, at retail's two sites and behind retail's two gates.**
        //
        // Object-collision reporting samples the object's `MISSILE_PS` bit once, at the start of
        // the report, and closes by testing that sample and masking the state with `0xFFFFFCBF`.
        // Two things about where that store sits, both load-bearing:
        //
        // * The branch for the other object's `state & IGNORE_COLLISIONS_PS` goes
        //    *past* it. A missile that passes through something it ignores stays a missile.
        // * The branches for no `REPORT_COLLISIONS_PS` in its own state and for
        //    no weenie object land *on* it. So the clear is **not** inside the
        //    `reports_collisions()` gate the notice is inside. Losing that distinction would make
        //    a missile's flight end only when somebody was listening.
        //
        // **What is still not transcribed here, and why it costs nothing.** Collision reporting has a
        // *second*, symmetric half that reports the same contact to the **other**
        // object's weenie. This build raises notices only for the mover. On the client that half
        // is unobservable: the other object is not a missile (a missile candidate never reaches
        // here at all — the candidate filter returns 1 for it outright), so it
        // takes the `ObjCollisionProfile` arm, and that arm is a bare `return 1;`. It would
        // matter to a *server* built on this crate, which is the reason to record it rather
        // than to leave it unsaid.
        //
        // `is_missile` is carried across the loop rather than re-read from `state`, because the
        // first contact clears the bit and a second candidate in the same frame must then take the
        // *ordinary* arm, matching the next collision report's fresh state-word read.
        let mut is_missile = state.is_missile();
        for (other, _) in &ci.collide_object {
            let Some(oo) = self.objects.get(*other) else {
                continue;
            };
            if oo.state.is_static() || oo.state.reports_as_environment() {
                // The static-state arm and `report_object_collision`'s state-bit `0x200000` test both
                // tail into environment-collision reporting, which carries its own copy of the
                // state.
                self.notices.push(PhysicsNotice::EnvironmentCollision {
                    object: id,
                    velocity,
                    me_in_contact: was_contact,
                });
                if is_missile {
                    is_missile = false;
                    if let Some(o) = self.objects.get_mut(h) {
                        o.clear_missile_on_contact();
                    }
                }
                continue;
            }
            if oo.state.ignores_collisions() {
                continue;
            }
            if state.reports_collisions() {
                let rel = velocity.sub(oo.cached_velocity);
                let other_id = oo.id;
                self.notices.push(if is_missile {
                    PhysicsNotice::MissileCollision {
                        object: id,
                        other: other_id,
                        relative_velocity: rel,
                        was_contact,
                    }
                } else {
                    PhysicsNotice::ObjectCollision {
                        object: id,
                        other: other_id,
                        relative_velocity: rel,
                        was_contact,
                    }
                });
            }
            if is_missile {
                is_missile = false;
                if let Some(o) = self.objects.get_mut(h) {
                    o.clear_missile_on_contact();
                }
            }
        }

        let already = self
            .objects
            .get(h)
            .is_some_and(|o| o.colliding_with_environment);
        if already {
            if let Some(o) = self.objects.get_mut(h) {
                o.colliding_with_environment = ci.collided_with_environment;
            }
        } else {
            let now_walkable = self
                .objects
                .get(h)
                .is_some_and(|o| o.transient_state.on_walkable());
            if ci.collided_with_environment || (!was_on_walkable && now_walkable) {
                if state.reports_collisions() {
                    self.notices.push(PhysicsNotice::EnvironmentCollision {
                        object: id,
                        velocity,
                        me_in_contact: was_contact,
                    });
                }
                if let Some(o) = self.objects.get_mut(h) {
                    o.colliding_with_environment = true;
                    // Environment collision has its own copy of the state-mask store,
                    // inside the `colliding_with_environment` latch and outside the
                    // `REPORT_COLLISIONS_PS` test: if `state & 0x40`, clear `0x340` together.
                    //
                    // This is the arm a spell bolt reaching the ground takes.
                    o.clear_missile_on_contact();
                }
            }
        }

        let Some(o) = self.objects.get_mut(h) else {
            return;
        };
        if ci.frames_stationary_fall < 2 {
            if allow_bounce && ci.collision_normal_valid {
                if o.state.is_inelastic() {
                    o.velocity_vector = Vec3::ZERO;
                } else {
                    let d = ci.collision_normal.dot(o.velocity_vector);
                    if d < 0.0 {
                        // v' = v - (1 + e) * (v . n) * n; with the default e = 0.05 this removes
                        // 105% of the into-surface component, a very slight bounce.
                        o.velocity_vector = o
                            .velocity_vector
                            .add(ci.collision_normal.mul(-(elasticity + 1.0) * d));
                    }
                }
            }
        } else {
            o.velocity_vector = Vec3::ZERO;
        }

        match ci.frames_stationary_fall {
            1 => o.transient_state.set_stationary_fall(true),
            2 => o.transient_state.set_stationary_stop(true),
            3 => o.transient_state.set_stationary_stuck(true),
            _ => {
                o.transient_state.0 &= TransientState::CLEAR_STATIONARY_AND_ACTIVE_MASK;
            }
        }
    }

    /// Force an object into a cell using only what physics decides: place the object at `pos` and
    /// rebuild its shadows.
    pub fn force_into_cell(&mut self, h: PhysHandle, pos: &Position) {
        let Some(cell) = ({
            let resolver = CellResolver::new(&*self.land);
            resolver.get_visible(pos.cell).map(|c| c.id())
        }) else {
            return;
        };
        if self.objects.get(h).and_then(|o| o.cell) != Some(cell) {
            self.leave_cell(h);
            self.enter_cell(h, cell);
        }
        if let Some(o) = self.objects.get_mut(h) {
            o.set_frame(pos.frame);
            o.position.cell = pos.cell;
        }
        self.calc_cross_cells(h, false);
    }

    /// Apply a remote position update — **the whole function**, including the ordinary
    /// non-teleport arm.
    ///
    /// 1. A `POSITION_TS` stamp that is not newer (the `0x7FFF` wrap rule) returns false.
    /// 2. With no cell, or a newer `TELEPORT_TS` event: run the teleport hook, place with
    ///    `TELEPORT_SPF | DONT_CREATE_CELLS_SPF`, return true.
    /// 3. Without contact, return false — the body is not repositioned at all.
    /// 4. Within `player_distance < 96.0`, interpolate to the position (moving-to flag passed on);
    ///    otherwise stop interpolation and set the position directly.
    ///
    /// ACE transcribes the same function at `ACE.Server/Physics/PhysicsObj.cs:905`-`:933`, and
    /// this crate already carried the interpolate arm's predicate alone as
    /// [`Self::move_or_teleport_should_interpolate`] with no production caller.
    ///
    /// **The `POSITION_TS` gate is the caller's here**, not this function's. The object-stream
    /// receiver has already run it (and the `TELEPORT_TS` rollback) in `ObjectStream`'s
    /// `received_position`, which is where this build keeps the server's sequence numbers.
    /// `teleport` is that gate's own answer, handed down rather than re-derived from a physics-side
    /// timestamp table nothing writes for a remote body.
    ///
    /// **Interpolation walks; it does not move the body here.** A queued position target
    /// is approached over subsequent sub-steps at the motion interpreter's maximum speed.
    /// [`Self::interpolate_to`], [`Self::update_position_internal`] and
    /// [`Self::position_use_time`] do the queueing, walking and snapping respectively. Applying
    /// the endpoint immediately would avoid placement's 4 m scatter search, which can shift a
    /// remote body up to a metre from the server position, but would make an ordinary
    /// correction instantaneous.
    pub fn move_or_teleport(
        &mut self,
        h: PhysHandle,
        pos: &Position,
        teleport: bool,
        contact: bool,
    ) -> MoveOrTeleport {
        // Having no cell is the first half of the teleport test and it is what makes the
        // retry of an object this client could not place keep
        // running through a full `SetPosition` rather than through the interpolate arm.
        let no_cell = self.objects.get(h).is_none_or(|o| o.cell.is_none());
        if teleport || no_cell {
            // The teleport hook calls the position manager's stop-interpolating operation.
            // A teleport out-ranks whatever walk was queued; without this the stale node drags the
            // body back off the position the teleport put it on.
            self.stop_interpolating(h);
            return MoveOrTeleport::Teleported {
                placed: self.set_position_and_check(h, pos),
            };
        }
        if !contact {
            // `if (!contact) return false;` at the top of the else branch: the body keeps the
            // position it had, and retail's own copy keeps being integrated by
            // `UpdateObjectInternal`, exactly as this crate's is.
            return MoveOrTeleport::NotInContact;
        }
        // The 96 m activity radius — `player_distance` is `FLT_MAX` until
        // [`Self::update_object`] has run for this body once, in retail as here, so a body the
        // physics tick has not reached yet snaps.
        let within = self
            .objects
            .get(h)
            .is_some_and(|o| o.player_distance < globals::ACTIVE_RADIUS);
        if within {
            MoveOrTeleport::Interpolated {
                placed: self.interpolate_to(h, pos),
            }
        } else {
            // Stop interpolating before the snap, so a queued walk cannot drag the body back off it.
            self.stop_interpolating(h);
            MoveOrTeleport::Snapped {
                placed: self.set_position_and_check(h, pos),
            }
        }
    }

    /// How far a correction may be before the
    /// object blips to it instead of walking.
    ///
    /// It asks whether this is the player object and whether the low 16 bits of the position's
    /// cell id are below `0x100` (an outdoor land cell),
    /// giving 100 m outdoors either way, 25 m indoors for the player and 20 m indoors for
    /// everything else.
    #[must_use]
    pub fn autonomy_blip_distance(&self, h: PhysHandle) -> f32 {
        let Some(o) = self.objects.get(h) else {
            return globals::AUTONOMY_BLIP_OUTDOORS;
        };
        if u32::from(o.position.cell.index()) < 0x100 {
            return globals::AUTONOMY_BLIP_OUTDOORS;
        }
        if self.player == Some(h) {
            globals::AUTONOMY_BLIP_INDOORS_PLAYER
        } else {
            globals::AUTONOMY_BLIP_INDOORS
        }
    }

    /// Drop whatever walk is queued for `h`.
    /// A no-op for an object that has no manager, exactly as the client's null test is.
    pub fn stop_interpolating(&mut self, h: PhysHandle) {
        if let Some(m) = self.interpolation.get_mut(&h) {
            m.stop_interpolating();
        }
    }

    /// Whether a walk is queued for `h`.
    #[must_use]
    pub fn is_interpolating(&self, h: PhysHandle) -> bool {
        self.interpolation
            .get(&h)
            .is_some_and(InterpolationManager::is_interpolating)
    }

    /// The object's interpolation manager, for a station that needs to read the queue.
    #[must_use]
    pub fn interpolation(&self, h: PhysHandle) -> Option<&InterpolationManager> {
        self.interpolation.get(&h)
    }

    /// [`Self::set_position`] plus the answer its callers actually want: retail's
    /// `SetPositionError` is discarded and the question is whether the body ended up in a cell.
    fn set_position_and_check(&mut self, h: PhysHandle, pos: &Position) -> bool {
        self.set_position(h, pos) && self.objects.get(h).is_some_and(|o| o.cell.is_some())
    }

    /// **Queue** a walk toward `pos`; ordinary interpolation leaves the position unchanged
    /// here.
    ///
    /// Position interpolation chooses between snapping, already-at-target and queueing a
    /// node. Queueing reaches its target in later sub-steps through
    /// [`Self::update_position_internal`] and [`Self::position_use_time`].
    ///
    /// Applying the walk's endpoint immediately would avoid a placement scatter search, but
    /// would make corrected remote bodies teleport between ordinary updates instead of gliding.
    ///
    /// `keep_heading` is true only while the object's movement manager is running a move-to. That
    /// manager belongs to the animation seam, so physics queries
    /// [`dereth_primitives::MotionSource::is_moving_to`]. Always passing false would match the
    /// original only for a body without a movement manager. When set, the queued node keeps the
    /// body's heading instead of the server heading, correcting a creature's approach position
    /// without turning it.
    ///
    /// **The blip is run here, not one sub-step later.** A correction beyond the snap threshold
    /// arms `node_fail_counter = 4`, and the client's next movement step turns that into the simple
    /// position set, whose null-cell arm
    /// calls the exit-world path and dooms the body. **In this build nothing
    /// in physics raises that doom**: `ObjectPhysics::sync` does, from the answer this function
    /// returns, because the queue is `dereth_client_model`'s. Deferring the blip
    /// by a sub-step therefore does not delay it, it *loses* it — a player who teleports to an
    /// unloaded landblock would never be culled and his wielded items would outlive him as
    /// orphans. So the same `UseTime` call the client makes is made now, inline, through
    /// [`Self::position_use_time`]. The arm that is genuinely deferred is the other way into the
    /// blip: a node the **walk** fails four times over, which `position_use_time` still handles on
    /// the sub-step it happens.
    ///
    /// Returns whether the body is in a cell. The walk arms never query the cell graph and return
    /// a literal `1`; the blip returns the simple position set's own answer.
    fn interpolate_to(&mut self, h: PhysHandle, pos: &Position) -> bool {
        // **The seam rule, stated rather than hidden.** The original interpolation queue
        // does not resolve destination cells: it stores a `Position`, starts each sweep
        // in the body's current cell, and relies on the transition to cross cells. It can
        // therefore approach a destination cell that is not currently visible.
        //
        // This build cannot do that, because the lost-cell notice has exactly one producer here:
        // the placement
        // verdict `ObjectPhysics::sync` reads back — and the whole visibility and destruction
        // seam hangs off it. A correction naming a cell that does not resolve therefore still
        // takes the lost-cell arm, and only a destination the client can actually see queues a
        // walk. Named as a divergence: an env-cell index the resident landblock does not carry,
        // at coordinates a few metres away, culls the body here and would be walked towards in
        // retail. Every *real* version of that message — a player entering a dungeon, a
        // teleport across the map — is a different landblock and is the blip arm below.
        let center = self.objects.get(h).map_or(Vec3::ZERO, |o| {
            o.geometry
                .path_spheres()
                .first()
                .map_or(Sphere::dummy().center, |s| s.center)
        });
        let lost = match self.adjust_position(pos, center) {
            None => Some(*pos),
            Some((adjusted, None)) => Some(adjusted),
            Some((_, Some(_))) => None,
        };
        if let Some(lost) = lost {
            self.stop_interpolating(h);
            self.goto_lost_cell(h, &lost);
            return false;
        }

        let Some(current) = self.objects.get(h).map(|o| o.position) else {
            return false;
        };
        // The moving-to state is read **once**, here, and passed as the interpolation node's
        // keep-heading value. A hard-coded `false` would only be the answer for a body with no
        // movement manager.
        let keep_heading = self
            .objects
            .get(h)
            .is_some_and(|o| o.motion.as_ref().is_some_and(|m| m.is_moving_to()));
        let blip = self.autonomy_blip_distance(h);
        // Lazily, on the first correction the body takes.
        let m = self.interpolation.entry(h).or_default();
        match m.interpolate_to(&current, pos, keep_heading, blip) {
            InterpolateDecision::Blip => self.position_use_time(h),
            // The "already there" arm. Within 0.05 m there is nothing to walk, so
            // the only thing left to carry is the facing, and `keep_heading` is what decides
            // whether to take it:
            //
            // When `keep_heading` is set, leave the body's heading alone. Otherwise copy the wire
            // frame's heading to the physics object.
            //
            // `StopInterpolating` follows; it is run inside `interpolate_to`
            // above and touches no frame, so the order is retail's.
            InterpolateDecision::AlreadyThere if !keep_heading => {
                let heading = math::get_heading(&pos.frame);
                if let Some(o) = self.objects.get_mut(h) {
                    o.set_heading(heading);
                }
            }
            InterpolateDecision::AlreadyThere | InterpolateDecision::Queued => {}
        }
        self.objects.get(h).is_some_and(|o| o.cell.is_some())
    }

    /// The position manager's tick reaches the interpolation manager's, the last call the
    /// sub-step makes.
    ///
    /// A position node is walked by [`Self::update_position_internal`]; what happens here is the
    /// velocity node and the **blip**, the arm `InterpolateTo` arms by setting
    /// `node_fail_counter = 4` for a target beyond.
    fn position_use_time(&mut self, h: PhysHandle) {
        let Some(current) = self.objects.get(h).map(|o| o.position) else {
            return;
        };
        let Some(m) = self.interpolation.get_mut(&h) else {
            return;
        };
        let now = self.sim_time;
        match m.use_time(&current) {
            crate::pmanager::InterpolationStep::Idle => {}
            crate::pmanager::InterpolationStep::SetVelocity(v) => {
                if let Some(o) = self.objects.get_mut(h) {
                    o.set_velocity(v, now);
                }
            }
            crate::pmanager::InterpolationStep::Blip { pos, velocity } => {
                // `SetPositionSimple(pos, 1)`. A refusal keeps the queue and is
                // retried next sub-step:
                //
                // ```text
                //   if (SetPositionSimple(pos, 1) != 0) keep the queue
                //   else { set_velocity(...); StopInterpolating(); }
                // ```
                //
                // The retry is **not** how a blip into a landblock that has not been prefetched
                // yet waits for it: the simple position set's null-cell arm -- the one a
                // non-resident landblock takes, through the landscape's own "is this block
                // loaded" test -- returns **OK_SPE**, so it falls through to `StopInterpolating`
                // and the body stays doomed. The non-zero answers the retry exists for are `1`,
                // `3` and `2`/`4`, every one of them a transition or internal-position-check
                // refusal — a **collision** problem, which the next sub-step may well have moved
                // out of the way.
                //
                // `set_position` here returns `is_ok()`, and `resolve_cell_and_place`'s own
                // `goto_lost_cell` returns `Ok(())` for exactly the same reason, so this arm
                // already matches retail.
                if self.set_position(h, &pos) {
                    if let (Some(v), Some(o)) = (velocity, self.objects.get_mut(h)) {
                        o.set_velocity(v, now);
                    }
                    self.stop_interpolating(h);
                }
            }
        }
    }

    /// The position-correction decision: within the 96 m activity radius
    /// a contact update interpolates, beyond it the object snaps.
    #[must_use]
    pub fn move_or_teleport_should_interpolate(
        &self,
        h: PhysHandle,
        stamp: u16,
        contact: bool,
    ) -> bool {
        let Some(o) = self.objects.get(h) else {
            return false;
        };
        if o.newer_event(PhysicsTimeStamp::Teleport, stamp) || o.cell.is_none() {
            return false;
        }
        contact && o.player_distance < globals::ACTIVE_RADIUS
    }

    /// Resolve a `Position` to the cell that actually
    /// contains it, adjusting the position in place.
    ///
    /// Returns the adjusted position and the cell it resolved to, or `None` when the client
    /// returns 0. **The cell may be `None` on success**: an outdoor id that `adjust_to_outside`
    /// accepts but whose landblock is not resident answers 1 with a NULL out-cell, and the two
    /// camera call sites treat that exactly as the client does.
    ///
    /// `sphere_center` is the mover's local sphere centre; for the viewer sweep it is `(0, 0, 0)`
    /// and the transformed point is therefore the origin itself.
    #[must_use]
    pub fn adjust_position(
        &self,
        pos: &Position,
        sphere_center: Vec3,
    ) -> Option<(Position, Option<CellId>)> {
        // Valid ids are 1..=0x40 (land cells), 0x100..=0xFFFD and 0xFFFF (env cells). The client
        // writes the test inverted and returns 0 for everything else.
        let index = u32::from(pos.cell.index());
        let land_cell = (1..=0x40).contains(&index);
        let env_cell = (0x100..=0xFFFD).contains(&index) || index == 0xFFFF;
        if !land_cell && !env_cell {
            return None;
        }
        let resolver = CellResolver::new(&*self.land);
        let mut out = *pos;
        if landdefs::is_outdoors(out.cell) {
            let mut id = out.cell;
            let mut origin = out.frame.origin;
            landdefs::adjust_to_outside(&mut id, &mut origin);
            out.cell = id;
            out.frame.origin = origin;
            if id.0 == 0 {
                // `adjust_to_outside` zeroed the id, so the non-zero-id arm is skipped and
                // the function falls out with its initial answer of none.
                return None;
            }
            return Some((out, resolver.get_visible(id).map(|c| c.id())));
        }
        let start = resolver.get_visible(out.cell)?;
        // Convert the sphere center from local to global space — the block offset is zero because
        // both arguments are the same position.
        let point = math::localtoglobal(&out.frame, sphere_center);
        if let Some(child) = resolver.find_visible_child_cell(&start, point) {
            out.cell = child.id();
            return Some((out, Some(child.id())));
        }
        // An interior cell that can see the outdoors falls back to the
        // land cell the building stands in.
        let seen_outside = match &start {
            Cell::Env { geom, .. } => geom.seen_outside,
            Cell::Land { .. } => false,
        };
        if !seen_outside {
            return None;
        }
        let mut id = out.cell;
        let mut origin = out.frame.origin;
        landdefs::adjust_to_outside(&mut id, &mut origin);
        out.cell = id;
        out.frame.origin = origin;
        if id.0 == 0 {
            return None;
        }
        Some((out, resolver.get_visible(id).map(|c| c.id())))
    }

    /// Sweep an explicit sphere from `from` to `to` through intermediate cells, with an
    /// explicit object-info state. The camera path initializes that state from the player
    /// with `0x5C`, uses one viewer sphere at scale 1.0, initializes the path from the
    /// supplied start cell and endpoints, then searches for a valid position.
    ///
    /// This is deliberately *not* [`Self::transition`]: that one derives the state, the spheres
    /// and the start cell from the moving object, which is right for the object and wrong for the
    /// camera. Everything else — the pool, the step count, the collision walk, the deferred
    /// `kill_velocity` — is shared, and `ObjectInfoState::IS_VIEWER` is the state bit on which the
    /// transition insertion and walkability checks already branch.
    ///
    /// `mover` identifies the object this transition concerns (the player for a camera).
    /// Its scale, step heights and ethereality seed the object-info state. A requested
    /// `kill_velocity` zeroes **that object's** velocity, as in the original camera path.
    ///
    /// Returns `None` when the pool is exhausted or `find_valid_position` fails.
    pub fn sweep_sphere(
        &mut self,
        mover: PhysHandle,
        state: u32,
        sphere: Sphere,
        start_cell: CellId,
        from: &Position,
        to: &Position,
    ) -> Option<Transition> {
        let idx = self.pool.make()?;
        let mut t = self.pool.take(idx);
        t.init();

        let mover_state = {
            let Some(o) = self.objects.get(mover) else {
                self.pool.put(idx, t);
                self.pool.cleanup();
                return None;
            };
            t.init_object(o, mover, state);
            o.state
        };
        // Initialize one unscaled transition sphere for the viewer. The
        // object's own `scale` is deliberately not used: the camera sphere is a world constant.
        t.sphere_path.init_sphere(&[sphere], 1.0);
        t.sphere_path.init_path(Some(start_cell), Some(*from), to);
        t.sphere_path.set_check_pos(from, Some(start_cell));

        let ok = {
            let ctx = TransitionCtx {
                land: &*self.land,
                objects: &self.objects,
                cells: &self.cells,
                mover: Some(mover),
                object_table: Some(&self.object_table),
                entry_host: self.entry_host.as_deref(),
            };
            find_valid_position(&ctx, &mut t, mover_state)
        };
        self.pool.cleanup();
        if t.kill_velocity {
            if let Some(o) = self.objects.get_mut(mover) {
                o.velocity_vector = Vec3::ZERO;
            }
        }
        let out = std::mem::take(&mut t);
        self.pool.put(idx, t);
        if ok {
            Some(out)
        } else {
            None
        }
    }

    /// The only thing that moves `ETHEREAL_PS`.
    ///
    /// The ethereal hook's body is `set_ethereal` with the hook's `ethereal` value and no
    /// report, and it is the *whole* mechanism: a door becomes
    /// ethereal because its opening animation carries a hook that says so, and solid again
    /// because the closing one does. No message sets the bit and no script does.
    ///
    /// **The two directions are not symmetric.**
    ///
    /// * Turning it **on** is unconditional: `state |= ETHEREAL_PS`, and the queued-retry bit
    ///   [`crate::obj::TransientState::check_ethereal`] is cleared.
    /// * Turning it **off** may be **refused**. A door that closed on top of a body would make
    ///   the body solid-stuck inside a mesh, so the client clears the bit, runs
    ///   [`Self::ethereal_check_for_collisions`], and on a hit **puts the bit back** and raises
    ///   `CHECK_ETHEREAL_TS`. That bit is not a tombstone: [`Self::update_object`] retries this
    ///   call every sub-step until the overlap is gone, which is the half that makes the
    ///   mechanism work rather than merely fail safe. The client returns `0` for the refusal;
    ///   here it is [`EtherealResult::Deferred`], which is a *different* answer from
    ///   [`EtherealResult::NoObject`] on purpose.
    ///
    /// **The order is load-bearing and is not a tidy-up opportunity.** The bit is cleared
    /// *before* the overlap test, because that test runs with this object as the candidate and its
    /// function's second branch answers `OK_TS` for an ethereal candidate. Testing first and
    /// clearing afterwards makes the guard answer "clear" every time, which is exactly the
    /// instrument that cannot fail.
    ///
    /// The guard is skipped entirely for an object that has a parent or is in no cell —
    /// it runs only with no parent and a cell — so a wielded item's hooks apply
    /// immediately and unconditionally.
    ///
    /// `report` is the client's second argument. **It is dead in this build**: the retail ethereal
    /// setter never reads it, and neither do its sibling setters for translucency and lighting.
    /// It is taken so that the hook's own `0` can be written down at
    /// the call site rather than silently dropped, and no meaning is invented for it.
    pub fn set_ethereal(&mut self, h: PhysHandle, ethereal: bool, report: bool) -> EtherealResult {
        let _ = report;
        let guarded = {
            let Some(o) = self.objects.get_mut(h) else {
                return EtherealResult::NoObject;
            };
            if ethereal {
                o.state.set_ethereal_bit(true);
                false
            } else {
                // Cleared FIRST — see the note above.
                o.state.set_ethereal_bit(false);
                o.parent.is_none() && o.cell.is_some()
            }
        };
        if guarded && self.ethereal_check_for_collisions(h) {
            let Some(o) = self.objects.get_mut(h) else {
                return EtherealResult::NoObject;
            };
            o.state.set_ethereal_bit(true);
            o.transient_state.set_check_ethereal(true);
            self.ethereal_deferrals += 1;
            return EtherealResult::Deferred;
        }
        if let Some(o) = self.objects.get_mut(h) {
            o.transient_state.set_check_ethereal(false);
        }
        EtherealResult::Applied
    }

    /// How many times [`Self::set_ethereal`] has **refused** to clear `ETHEREAL_PS` because
    /// something was standing in the object, over the life of this world.
    ///
    /// A counter rather than a log line because a test needs a denominator: "the door came back
    /// solid" and "the door was never asked" look identical from outside, and this is the number
    /// that tells them apart.
    #[must_use]
    pub const fn ethereal_deferrals(&self) -> u64 {
        self.ethereal_deferrals
    }

    /// The attack entry plus its synchronous tail, the attack report.
    ///
    /// The single caller in the whole client is the animation attack-hook handler, whose entire
    /// body invokes the object's attack with its stored cone — so **the `AttackHook` is the only
    /// thing in the client that ever attacks anything**, and 716 of the 2,066 retail animations
    /// carry one (1,053 instances).
    ///
    /// The attack is skipped outside a loaded cell. Otherwise its sphere center Z is cone
    /// height times object scale, and its radius is attack radius plus cone radius times
    /// object scale. Cell-list search does not load cells; detection uses the cone's part
    /// index. Reporting runs when the attack's waiting-for-cells count is zero.
    ///
    /// Three observed invariants simplify this function:
    ///
    /// * **`attack_radius` is identically zero.** The attack manager's field is initialized to
    ///   `0.0`, as is the unrelated physics object's field, and **neither is assigned anywhere
    ///   else in the client**. So the search sphere's radius is exactly
    ///   `scale * cone.radius`, and the fifth argument to the attack check is always `0.0`.
    ///   The field is still read from the
    ///   object, so a rebuild server that sets it gets the retail arithmetic.
    /// * **The waiting-for-cells count is always zero**, for the same reason: attack-info
    ///   construction zeroes it and nothing writes it. `report_attacks` therefore always runs in
    ///   the same call, which is why this returns its profiles rather than queuing a notice.
    /// * **Adding an object dedupes by `object_id` and keeps the FIRST hit
    ///   location**, scanning the list linearly before appending. An object straddling two of the
    ///   cells the sphere touches is reported once, with the quadrant the first cell computed.
    ///
    /// Profiles are returned in attack-report order. The original client then resets
    /// collision bookkeeping (a no-op) and handles each collision by playing the
    /// **attacker's** default script when the attacker has `SCRIPTED_COLLISION_PS`.
    /// Physics does neither here: the script table is owned by the scene's animation
    /// driver, so `WorldScene::process_hooks` handles the local effect.
    pub fn attack(
        &mut self,
        h: PhysHandle,
        cone: &crate::detect::AttackCone,
    ) -> Vec<crate::detect::AtkCollisionProfile> {
        let Some(o) = self.objects.get(h) else {
            return Vec::new();
        };
        // An object outside a loaded cell does not attack; the original client does not
        // even allocate attack state on this path.
        if o.cell.is_none() {
            return Vec::new();
        }
        let (attacker_id, pos, scale, attack_radius) = (o.id, o.position, o.scale, o.attack_radius);

        // The search sphere. Its centre is the cone's height scaled, in the attacker's LOCAL frame
        // and stores literal zeroes for x and y. `find_cell_list` wants it
        // in the position's block space, the same conversion `calc_cross_cells` makes.
        let local = Vec3::new(0.0, 0.0, cone.height * scale);
        let centre =
            math::localtoglobalvec(math::l2g(pos.frame.rotation), local).add(pos.frame.origin);
        let sphere = Sphere::new(centre, scale * cone.radius + attack_radius);

        let mut arr = crate::cell::CellArray::new();
        arr.do_not_load_cells = true;
        let mut interior = false;
        {
            let resolver = CellResolver::new(&*self.land);
            resolver.find_cell_list(&pos, &[sphere], &mut arr, false, &mut interior);
        }

        // Begin the attack, then check each cell. The shared attack-state design supports
        // multi-frame completion on a server, but the client path reports and releases it
        // within this call. Attack state is therefore local here.
        let mut hits: Vec<crate::detect::AtkCollisionProfile> = Vec::new();
        for entry in &arr.cells {
            let candidates: Vec<PhysHandle> = match self.cells.get(&entry.cell_id.0) {
                Some(c) => c.shadow_object_list.clone(),
                None => continue,
            };
            for other in candidates {
                let Some(t) = self.objects.get(other) else {
                    continue;
                };
                // the object's id is not `attacker_id`, then `(state & STATIC_PS) == 0`, in
                // that order. A baked pillar is never attacked.
                if t.id == attacker_id || t.state.is_static() {
                    continue;
                }
                let Some(location) = crate::detect::check_attack(
                    &t.position,
                    t.parent.is_some(),
                    t.state,
                    t.geometry.radius,
                    t.geometry.height,
                    t.scale,
                    &pos,
                    scale,
                    cone,
                    attack_radius,
                ) else {
                    continue;
                };
                // dedupe by id, first hit location wins.
                if hits.iter().any(|p| p.id == t.id) {
                    continue;
                }
                hits.push(crate::detect::AtkCollisionProfile {
                    part: cone.part_index,
                    id: t.id,
                    location,
                });
            }
        }
        hits
    }

    /// "is anything standing in me?"
    ///
    /// Every cell this object has a **shadow** in is searched. Each *other* unparented object
    /// registered in that cell is asked — with itself as the mover — whether it collides with
    /// this one. The
    /// first hit wins; an object with no shadows answers no.
    ///
    /// Note the direction, because it is the opposite of the intuitive one. The transition is
    /// built around the **other** object's spheres at the **other** object's position, and this
    /// object supplies the candidate geometry. That is what makes a body standing in a doorway
    /// block the door's return to solid even though the door is the thing whose bit is moving.
    #[must_use]
    pub fn ethereal_check_for_collisions(&mut self, h: PhysHandle) -> bool {
        let shadows: Vec<CellId> = match self.objects.get(h) {
            Some(o) => o.shadow_objects.iter().map(|s| s.cell_id).collect(),
            None => return false,
        };
        for cell_id in shadows {
            let others: Vec<PhysHandle> = match self.cells.get(&cell_id.0) {
                Some(c) => c.shadow_object_list.clone(),
                None => continue,
            };
            for other in others {
                // `other != h` and `other.parent == NULL`, in that order.
                if other == h || self.objects.get(other).is_none_or(|o| o.parent.is_some()) {
                    continue;
                }
                if self.check_collision(other, h) {
                    return true;
                }
            }
        }
        false
    }

    /// Does `h`, standing exactly where it is,
    /// overlap `target`?
    ///
    /// A **placement** transition of zero length sets `insert_type = PLACEMENT_INSERT`, copies
    /// `curr_pos` into `check_pos`, and runs collision detection for the one candidate. Nothing moves and
    /// nothing is committed — the transition is handed straight back to the pool.
    ///
    /// A `STATIC_PS` mover answers **no** outright (state bit 1 set returns 0), so
    /// a baked pillar sharing a cell with a door never keeps the door ethereal.
    ///
    /// The sphere seeding is the client's, including its asymmetry: a mover with spheres uses
    /// them at its own `scale`, and a mover with none uses the shared unit `dummy_sphere` at
    /// scale **1.0** rather than at the object's scale.
    #[must_use]
    pub fn check_collision(&mut self, h: PhysHandle, target: PhysHandle) -> bool {
        if self.objects.get(h).is_none_or(|o| o.state.is_static()) {
            return false;
        }
        let Some(idx) = self.pool.make() else {
            return false;
        };
        let mut t = self.pool.take(idx);
        t.init();

        let (state, spheres, scale, pos, own_cell) = {
            let Some(o) = self.objects.get(h) else {
                self.pool.put(idx, t);
                self.pool.cleanup();
                return false;
            };
            // `get_object_info(this, t, 0)` — the contact-plane arm runs, as it does for any
            // non-placement query.
            let s = self.get_object_info(o, &mut t, false);
            (
                s,
                o.geometry.path_spheres().to_vec(),
                o.scale,
                o.position,
                o.cell,
            )
        };
        if let Some(o) = self.objects.get(h) {
            t.init_object(o, h, state);
        }
        if spheres.is_empty() {
            t.sphere_path.init_sphere(&[Sphere::dummy()], 1.0);
        } else {
            t.sphere_path.init_sphere(&spheres, scale);
        }
        t.sphere_path.init_path(own_cell, Some(pos), &pos);
        // Initialize the placement in its own order: the insert type first,
        // then `check_pos <- curr_pos`, which re-caches the global spheres.
        t.sphere_path.insert_type = InsertType::Placement;
        let curr = t.sphere_path.curr_pos;
        let curr_cell = t.sphere_path.curr_cell;
        t.sphere_path.set_check_pos(&curr, curr_cell);

        let hit = match self.objects.get(target) {
            Some(other) => {
                let geometry = Arc::clone(&other.geometry);
                let frames = other.part_frames.clone();
                let (o_state, o_scale, o_pos) = (other.state, other.scale, other.position);
                let ctx = TransitionCtx {
                    land: &*self.land,
                    objects: &self.objects,
                    cells: &self.cells,
                    mover: Some(h),
                    object_table: Some(&self.object_table),
                    entry_host: self.entry_host.as_deref(),
                };
                crate::transition::collide::find_obj_collisions_geom(
                    &ctx,
                    &mut t,
                    target,
                    o_state,
                    crate::transition::collide::ObjGeometry::Setup(
                        &geometry,
                        frames.as_deref().map(Vec::as_slice),
                    ),
                    o_scale,
                    &o_pos,
                ) != crate::TransitionState::Ok
            }
            None => false,
        };
        self.pool.put(idx, t);
        self.pool.cleanup();
        hit
    }

    /// The land source, for callers that need to resolve a cell themselves.
    #[must_use]
    pub fn land(&self) -> &dyn LandSource {
        &*self.land
    }

    /// Everything a transition reads from this world, so that a caller can run one of the
    /// `find_collisions` entry points itself against the live object arena and cell lists.
    ///
    /// Read-only, and it holds no transition: the pool is the world's. This exists because the
    /// counters on a [`Transition`] are only readable by whoever owns it, and `update_object`
    /// hands its own back to the pool before returning.
    #[must_use]
    pub fn transition_ctx(&self, mover: Option<PhysHandle>) -> TransitionCtx<'_> {
        TransitionCtx {
            land: &*self.land,
            objects: &self.objects,
            cells: &self.cells,
            mover,
            object_table: Some(&self.object_table),
            entry_host: self.entry_host.as_deref(),
        }
    }

    /// A height probe, for tests and for the placement helpers:
    /// the terrain Z under an outdoor position.
    #[must_use]
    pub fn terrain_height_at(&self, pos: &Position) -> Option<f32> {
        if !landdefs::is_outdoors(pos.cell) {
            return None;
        }
        let block = self.land.landblock(pos.cell.landblock())?;
        let poly = block.find_terrain_poly(pos.cell.index(), pos.frame.origin)?;
        let mut p = pos.frame.origin;
        let plane = Plane {
            normal: poly.plane.normal,
            d: poly.plane.d,
        };
        if plane.set_height(&mut p) {
            Some(p.z)
        } else {
            None
        }
    }
}

/// Report each detected attack hit as a possible local effect.
///
/// The original client resets collision bookkeeping, handles every hit, then frees
/// the completed attack state. Resetting is a no-op. Collision handling ignores the
/// hit profile: it checks the **attacker's** physics body and plays that body's default
/// script only when state bit `0x8000` (`SCRIPTED_COLLISION_PS`) is set. This is the
/// same local effect used for missile and environment collisions.
///
/// So: **nothing reaches the wire**, the target is not touched, and a player whose body has
/// no `SCRIPTED_COLLISION_PS` sees no effect from his own swing at all — the client's attack
/// hook is a *detection* that the rebuild's server will need and a *local effect* that only
/// fires for objects carrying the bit (a spell bolt's `0x28B48` does). Transcribed as what it
/// is rather than padded out.
///
/// The `SCRIPTED_COLLISION_PS` test uses the **attacker's** state word, as in
/// `Character::update` collision handling. Each appended tuple is
/// `(attacker == body, attacker id, whether the attacker has scripted collision)`.
///
/// It lives in physics because every type it names is this crate's, and the client's
/// `AttackStats::objects_hit` is the `u64` it returns rather than a counter it reaches into.
pub fn report_attacks(
    world: &PhysicsWorld,
    attacker: PhysHandle,
    body: PhysHandle,
    hits: &[crate::detect::AtkCollisionProfile],
    out: &mut Vec<(bool, ObjectId, bool)>,
) -> u64 {
    let Some(o) = world.get(attacker) else {
        return 0;
    };
    let (id, plays) = (o.id, o.state.has_scripted_collision());
    let is_body = attacker == body;
    #[allow(clippy::cast_possible_truncation)]
    let n = hits.len() as u64;
    // Append one report per hit, not one per attack. An empty hit list appends none.
    for _ in hits {
        out.push((is_body, id, plays));
    }
    n
}

#[cfg(test)]
mod tests;
