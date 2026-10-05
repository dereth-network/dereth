//! The objects the server puts in the world, made **solid**.
//!
//! **This module wires; it does not implement.** Every decision below belongs elsewhere and is
//! called by name:
//!
//! | Decision | Whose |
//! |---|---|
//! | the arena, the cell object lists, the shadow lists | [`dereth_physics::PhysicsWorld`] |
//! | the shadow-list walk and what it skips | `dereth_physics::transition::collide` |
//! | which objects exist and where the server says they are | [`crate::objects::ObjectStream`] |
//! | a setup record's collision half | [`dereth_world_data::setup::setup_geometry`] |
//!
//! # Why this exists
//!
//! Cell object-collision discovery walks the shadow-object list, and
//! [`dereth_physics::PhysicsWorld`] maintains that list from `enter_cell` / `calc_cross_cells`.
//! [`crate::character`] calls them only for the player's own body, so without this module the
//! lists hold exactly one object and **nothing else in the world is solid**: a door is open
//! scenery and a creature a hologram, because physics is never told that anything else exists.
//!
//! Create handling tells physics about any unparented object with a nonzero cell through the
//! placement-and-slide path: cell resolution, object insertion, and cross-cell calculation.
//! Leaving removes its cell and shadow membership. This module connects those operations to forced
//! insertion and world-object destruction. Held objects remain non-solid because the collision
//! walk skips parented shadows.
//!
//! # What this deliberately does not do
//!
//! It does not *predict* anything. The object stream's rule stands: the server says where every
//! object it owns is, and a position that arrives replaces whatever the body drifted to. What
//! changes is that the body is now **in the cell lists**, so the player's own transition has to
//! get past it.
//!
//! An object with a parent, or with no position (`pd.position.objcell_id == 0`, i.e. carried or
//! wielded), gets no physics body, because step 4 calls `enter_world` only for
//! `pd.parent_id == 0` with a non-zero `objcell_id`. It would not be solid even if it had one:
//! the collision walk steps over a shadow whose physics object has a
//! parent, so a held sword collides through its holder or not at all.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use dereth_assets::{Decode, Setup};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::{PhysHandle, PhysicsState, PhysicsWorld, SetupGeometry};
use dereth_primitives::{DataId, ObjectId, Position};

use crate::objects::Presence;

/// The physics-word half of `set_hidden`, after `set_state` has assigned
/// `incoming`. Both the create-description path and a later `0xF74B` call those two functions.
fn with_hidden_collision_side_effects(
    previous: PhysicsState,
    mut incoming: PhysicsState,
) -> PhysicsState {
    if previous.is_hidden() != incoming.is_hidden() {
        let hidden = incoming.is_hidden();
        incoming.set_ignores_collisions(hidden);
        incoming.set_reports_collisions(!hidden);
    }
    incoming
}

use dereth_world_data::setup::{
    setup_geometry_with_parts_at, simple_setup_geometry, SetupPartStats,
};

/// The gameplay-object facts used by cell-entry restrictions, built from the
/// game-side object.
///
/// These are the answers the house-barrier, camera-creature, and player-pair collision ladders
/// read, and nothing else:
///
/// | physics field | retail |
/// |---|---|
/// | `is_creature` | the item-type bit `0x10` (`Creature`) |
/// | `is_player` | object-info initialization turns it into `IS_PLAYER` |
/// | `is_pk` | PWD bit `0x20` |
/// | `is_pk_lite` | PWD bit `0x0200_0000` |
/// | `is_impenetrable` | PWD bit `0x0020_0000` |
/// | `can_bypass` | move-restriction bypass: weenie bitfield `& (ADMIN \| CELL_BARRIER_IMMUNE)` |
/// | `house_owner` | the public description's house-owner identifier |
/// | `monarch` | the monarch identifier read off the **mover** |
/// | `restrictions` | the public description's restriction database, which `0x0248` delivers |
///
/// The restriction check reads only the bitmask's bit 0, the monarch iid and
/// the *presence* of a key in the guest table, so the guest table's values — the per-guest storage
/// permission — are dropped here rather than copied.
#[must_use]
pub fn weenie_restrictions(
    w: &dereth_client_model::Weenie,
) -> dereth_physics::obj::WeenieRestrictions {
    dereth_physics::obj::WeenieRestrictions {
        is_creature: w.is_creature(),
        is_player: w.is_player(),
        is_pk: w.is_pk(),
        is_pk_lite: w.is_pk_lite(),
        is_impenetrable: w.is_impenetrable(),
        can_bypass: w.can_bypass_move_restrictions(),
        house_owner: w.pwd.house_owner_iid.filter(|o| o.0 != 0),
        monarch: w.pwd.monarch.filter(|m| m.0 != 0),
        restrictions: w
            .pwd
            .restrictions
            .as_ref()
            .map(|db| dereth_physics::obj::Restrictions {
                open: db.bitmask & 1 != 0,
                monarch: Some(db.monarch_iid).filter(|m| m.0 != 0),
                guests: db.table.entries.iter().map(|(k, _)| k.0).collect(),
            }),
    }
}

/// What the physics half of the object stream has done, for the log line and for the tests.
///
/// Every field but the first three records an object that is **not** solid. A tolerated failure
/// recorded into a number nothing compares goes unnoticed, and "the world looks right and nothing
/// collides" is exactly that shape of failure, so each case is a counter and the acceptance test
/// asserts on them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ObjectPhysicsStats {
    /// Physics-object construction plus `enter_world`: objects given a physics body.
    pub created: u64,
    /// Times a body's restriction half was written,
    /// i.e. how often the pushed collision/restriction answers actually changed. Once per
    /// object on the create, and again on every `0x0248 House_UpdateRestrictions` that reaches a
    /// house this client can see.
    pub weenies_pushed: u64,
    /// Bodies destroyed, by a `0xF747`, by losing their position to a pickup, or by a reset.
    pub destroyed: u64,
    /// Server positions applied to an existing body.
    pub moved: u64,
    /// The four move-or-teleport arms, counted so
    /// that "which arm is this build actually taking" is answerable without a debugger. Every one
    /// of them climbs several times a second for a walking creature, so
    /// [`ObjectPhysics::report_key`] takes their base-2 magnitude for the same reason it takes
    /// `moved`'s.
    ///
    /// No cell yet, or a newer `TELEPORT_TS` event — the full `set_position` placement.
    pub teleport_arm: u64,
    /// `if (!contact) return false;` — the body was deliberately not repositioned.
    pub no_contact_arm: u64,
    /// Interpolation inside the 96 m activity radius: no transition, and so no scatter.
    pub interpolate_arm: u64,
    /// The simple position snap beyond the activity radius.
    pub snap_arm: u64,
    /// Bodies whose cell would not resolve — an interior cell of a block that has not been
    /// prefetched, or a cell id outside the world. Resolution finds no cell
    /// and the object stays out of every list; this is retried on the next sync.
    pub unplaced: u64,
    /// Objects whose `PhysicsDesc` carried no `setup_id`. They get a body with no geometry, which
    /// is what the client's own part-array-less physics object collides as: nothing.
    pub without_setup: u64,
    /// A `setup_id` that is not a `0x02000000` setup id. The client accepts a
    /// `0x01000000` graphics-object id too through simple setup creation, and this build
    /// wraps it only when mesh collision is on; otherwise it is counted rather than guessed.
    pub setup_not_a_setup: u64,
    /// A setup record that is missing or would not decode. A layout bug, not bad input.
    pub setup_undecodable: u64,
    /// An object whose id the physics world already holds. The object-maintenance table is keyed
    /// by object id and a second insert **replaces** the first, so registering one would silently
    /// take the sitting object out of the physics update sweep.
    ///
    /// On the connected path this is 0 by construction: the local body takes the id `0xF746`
    /// sends ([`crate::character::Character::adopt_server_id`], called from `App::sync_objects`
    /// immediately before [`ObjectPhysics::sync`]), and the one object that can carry that id is
    /// the player himself, whom `exclude` skips. `tests/player_id.rs` asserts it over the corpus.
    /// A fixed placeholder id would not be safe here: `0x50000001` is the **first** GUID ACE hands
    /// out, so another character carrying it would evict our own body.
    ///
    /// It is **still reachable** for the state in between: a session in which no
    /// `0xF746` has arrived — so `exclude` is `None` and the body still carries the offline id —
    /// while object creates have. Nothing in ACE allocates `0x60000000`, but the guard is what
    /// stands between a server that did and a body that silently stops being simulated, and the
    /// counter is tripped on purpose in `tests/player_id.rs` so that it is not a number nobody can
    /// move.
    pub id_collision: u64,
    /// Bodies whose part arrays contain physics BSPs, so object-collision
    /// checking walks those parts' BSPs.
    pub bsp_arm: u64,
    /// Of those, the ones with **no collision sphere at all** — objects that would be wholly
    /// intangible without the part BSPs. Every shipped door is one.
    pub bsp_arm_without_spheres: u64,
    /// State writes to a body that already existed —
    /// the server changing an object's `PhysicsState` after its create, which is what a `0xF74B
    /// Item_SetState` is.
    ///
    /// It counts *changes*, not calls: the word a spawn installs is recorded without being
    /// counted, so a non-zero value here means a live body's state word actually moved.
    pub state_applied: u64,
}

/// The physics bodies of the server's objects, keyed by object id.
///
/// One of these lives inside `ObjectStream`, because the two tables have exactly the same life:
/// an object that leaves the object stream's tables must leave the cell lists in the same breath,
/// or the shadow list keeps a handle to a dead object — which is precisely what
/// `CollisionCounters::shadow_dangling` counts and what the acceptance test asserts is zero.
#[derive(Debug)]
pub struct ObjectPhysics {
    /// A setup record's collision half, memoised. A spawn of six mosswarts is six objects and one setup,
    /// and the original cache memoises for the same reason. `None` is cached too.
    ///
    /// Keyed on `(setup, placement)`: the placement moves the parts, so two objects
    /// of one setup posed differently do not share a collision hull. Colliding at a pose the object
    /// is not drawn at shows up as nothing at all until someone walks into thin air.
    geometry: BTreeMap<(u32, u32), Option<Arc<SetupGeometry>>>,
    /// The live bodies. `PhysHandle` is an arena handle, so a stale one is a bug in this table
    /// rather than a dangling pointer.
    handles: BTreeMap<ObjectId, PhysHandle>,
    /// Exact old handles retired at deletion, before a same-id presence can replace them.
    retired: Vec<PhysHandle>,
    /// The placement id each body's geometry was built at. A body whose object the
    /// server has since re-posed is destroyed and re-made rather than left at the old hull, which
    /// matches a placement-frame change's effect on the live part array.
    posed: BTreeMap<ObjectId, u32>,
    /// What was last handed to `set_position`, so an unchanged position costs no `calc_cross_cells`.
    placed: BTreeMap<ObjectId, Position>,
    /// The `PhysicsState` word last written onto each body.
    ///
    /// The diff is kept here rather than read back off the body because `PhysicsObj::new` sets
    /// `HAS_PHYSICS_BSP_PS` from the geometry before the descriptor is applied, so "the body's
    /// word" and "the word the server last sent" are legitimately different objects; comparing the
    /// body against the presence would re-run `set_state` every frame for any object whose
    /// geometry scan and wire word disagreed. They agree on all 837 recorded creates, which is
    /// exactly why that must not be *assumed* here.
    stated: BTreeMap<ObjectId, u32>,
    /// The gameplay object's movement-restriction half as it was last
    /// pushed onto each body, kept for the same reason `stated` is: the diff decides whether to
    /// write, and a house whose restriction data has not changed must not be re-written every
    /// frame. `None` is a body that has been told it has no weenie.
    restricted: BTreeMap<ObjectId, Option<dereth_physics::obj::WeenieRestrictions>>,
    /// [`Self::report_key`] as it stood when the two log lines were last printed, so the pair goes
    /// out on a change and not once per frame.
    reported: Option<(usize, ObjectPhysicsStats, SetupPartStats)>,
    /// At most one pair of those lines per [`crate::report_gate::REPORT_INTERVAL`].
    report_gate: crate::report_gate::ReportGate,
    /// The objects whose placement left them with **no cell**, i.e. the
    /// state checked at the end of create-object handling:
    /// a non-zero `position.objcell_id` and no cell on the physics object.
    ///
    /// Held here rather than acted on here because the answer belongs to object maintenance, which lives in
    /// `dereth_client_model` — see [`Self::take_placement_verdicts`]. Membership is a **latch**, so the
    /// transition into the state is reported once: retail runs that tail on the create, and
    /// re-scheduling the object's destruction every frame would push the 25 s deadline
    /// forward for ever, which is the same as never scheduling it at all.
    lost: BTreeSet<ObjectId>,
    /// The two edges of [`Self::lost`] this sync crossed: `(id, doomed)`, with `doomed == true`
    /// for scheduling the object's destruction and `false` for cancelling it.
    verdicts: Vec<(ObjectId, bool)>,
    pub stats: ObjectPhysicsStats,
    /// What loading those setups' parts did.
    pub part_stats: SetupPartStats,
    /// `SceneConfig::mesh_collision`; see [`dereth_world_data::env_cells::CellStaticObjects`].
    pub mesh_collision: bool,
}

impl Default for ObjectPhysics {
    fn default() -> Self {
        Self {
            geometry: BTreeMap::new(),
            handles: BTreeMap::new(),
            retired: Vec::new(),
            posed: BTreeMap::new(),
            placed: BTreeMap::new(),
            stated: BTreeMap::new(),
            restricted: BTreeMap::new(),
            reported: None,
            report_gate: crate::report_gate::ReportGate::default(),
            lost: BTreeSet::new(),
            verdicts: Vec::new(),
            stats: ObjectPhysicsStats::default(),
            part_stats: SetupPartStats::default(),
            mesh_collision: true,
        }
    }
}

impl ObjectPhysics {
    /// Physical tail of preparing for visibility departure and leaving visibility. The model
    /// selected the exact nonstatic, unparented released-cell objects and owns their timers.
    /// Preserve the body, saved position and geometry; remove shadows before leaving its cell.
    pub(crate) fn leave_visibility(&mut self, id: ObjectId, world: &mut PhysicsWorld) {
        if let Some(handle) = self.handle(id).or_else(|| world.by_object_id(id)) {
            world.remove_shadows_from_cells(handle);
            world.leave_cell(handle);
            if let Some(body) = world.get_mut(handle) {
                body.transient_state.set_active_bit(false);
            }
        }
        // A later loaded cell must re-enter even when the saved wire position is unchanged.
        self.placed.remove(&id);
        // **Latch, do NOT raise an edge.** The caller has already run the model half
        // of visibility departure, whose fourth line
        // queues this object for destruction in object maintenance
        // and whose tail loop does the same for every child.
        // That deadline is the current time plus 25.0 — the `double` constant the deadline is
        // built from reads 25.0 — so it
        // is stamped at the **release's** clock.
        //
        // Clearing `placed` above makes the next [`Self::sync`] retry the placement, and for a
        // cell that has just been handed back that retry fails. Without this line the `else` arm
        // below would then see a fresh `lost` id, raise `(id, true)`, and
        // `ObjectStream::sync_physics_at` would call `schedule_destroy` a **second** time at that
        // later frame's clock — moving the 25 s deadline forward by the sync gap, silently, on
        // every release. Retail cannot do that: only one of the ten destruction-scheduling call
        // sites is a placement, and it runs once when the object is created.
        //
        // Latching without a verdict states what is true: the object is in the no-cell state and
        // is already on the object-maintenance queue. The removal edge
        // still fires from the `lost.remove` arm when a placement succeeds again.
        self.lost.insert(id);
    }

    /// Physical tail of preparing to enter the world. The caller checks the actual loaded cell and
    /// performs the model timer/lost-cell half before sync's existing `set_position` consumer.
    pub(crate) fn prepare_reentry(
        &mut self,
        id: ObjectId,
        world: &mut PhysicsWorld,
        now: f64,
    ) -> bool {
        let Some(handle) = self.handle(id).or_else(|| world.by_object_id(id)) else {
            return false;
        };
        let Some(body) = world.get_mut(handle) else {
            return false;
        };
        body.update_time = now;
        body.set_active(true, now); // Existing method retains the native STATIC_PS guard.
        self.placed.remove(&id);
        true
    }

    /// Complete visibility re-entry for a surviving body that the
    /// ordinary remote-object [`Self::sync`] deliberately excludes.
    ///
    /// [`Self::prepare_reentry`] is the preceding prepare-to-enter-world half. The
    /// saved physical pose, rather than a newer model snapshot, is what the retail position set
    /// (flags `0x11`) consumes. `set_position` may answer success after resolving to no cell, so
    /// both its answer and the committed cell are required before the private release latch is
    /// cleared.
    pub(crate) fn reenter_surviving_body(
        &mut self,
        id: ObjectId,
        world: &mut PhysicsWorld,
    ) -> bool {
        let Some(handle) = self.handle(id).or_else(|| world.by_object_id(id)) else {
            return false;
        };
        let Some(position) = world.get(handle).map(|body| body.position) else {
            return false;
        };
        let entered = world.set_position(handle, &position)
            && world.get(handle).is_some_and(|body| body.cell.is_some());
        if entered {
            self.lost.remove(&id);
        }
        entered
    }

    /// Capture identity now; an id-only deletion delayed until sync could destroy a replacement.
    pub(crate) fn retire(&mut self, id: ObjectId) {
        if let Some(handle) = self.handles.remove(&id) {
            self.retired.push(handle);
        }
        self.placed.remove(&id);
        self.posed.remove(&id);
        self.stated.remove(&id);
        self.restricted.remove(&id);
        self.lost.remove(&id);
    }

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The physics body of one object, for the tests and for anything that needs to name it.
    #[must_use]
    pub fn handle(&self, id: ObjectId) -> Option<PhysHandle> {
        self.handles.get(&id).copied()
    }

    /// How many objects currently have a body.
    #[must_use]
    pub fn len(&self) -> usize {
        self.handles.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }

    // ---------------------------------------------------------------------------------------
    // **Growth instrumentation.** [`Self::len`] already reports the body table; these are the
    // six *other* per-object maps beside it, which a leave/return cycle also writes and which a
    // cull has to give back. Gauges, read by the long-session growth station only.
    //
    // [`Self::geometry`] is deliberately included even though it is a memo keyed on
    // `(setup, placement)` rather than on the object: it is the one table here that is *meant*
    // to survive a cull, so a station that did not print it could not tell "the cache is doing
    // its job" apart from "the cache is the leak".
    // ---------------------------------------------------------------------------------------

    /// Memoised `(setup, placement)` collision hulls, hits and cached misses alike. Bounded by
    /// the distinct appearances seen, **not** by the live object count.
    #[must_use]
    pub fn geometry_cache_count(&self) -> usize {
        self.geometry.len()
    }

    /// Rows of the placement-id memo.
    #[must_use]
    pub fn posed_count(&self) -> usize {
        self.posed.len()
    }

    /// Rows of the last-handed-to-`set_position` memo.
    #[must_use]
    pub fn placed_count(&self) -> usize {
        self.placed.len()
    }

    /// Rows of the last-written `PhysicsState` memo.
    #[must_use]
    pub fn stated_count(&self) -> usize {
        self.stated.len()
    }

    /// Rows of the movement-restriction memo.
    #[must_use]
    pub fn restricted_count(&self) -> usize {
        self.restricted.len()
    }

    /// Objects currently latched as "placed with no cell" — the membership
    /// that schedules the object's destruction.
    #[must_use]
    pub fn lost_count(&self) -> usize {
        self.lost.len()
    }

    /// Handles retired and not yet freed by the next [`Self::sync`]. Non-zero only *between* a
    /// deletion and the following sync.
    #[must_use]
    pub fn retired_count(&self) -> usize {
        self.retired.len()
    }

    /// Bring the physics world into line with what the server has said.
    ///
    /// `exclude` is the player's own object id. The client has **one** physics object for the
    /// player — [`crate::character::Character`]'s — and creating a second from the server's copy of
    /// him would put two bodies in the same cell, each solid to the other, at the same place.
    ///
    /// The diff is taken against this table rather than against
    /// `ObjectStream::take_created` / `take_removed`, which the renderer already drains; the two
    /// consumers would otherwise have to share one queue, and a frame in which the renderer ran
    /// and physics did not would silently lose an object.
    ///
    /// **`game` is where the state word comes from, and it is a parameter for a structural
    /// reason.** Retail reads the state word off the physics object it is already
    /// holding; this build stores that word once, in `dereth_client_model`, because both of the retail
    /// readers of bit 0 are `dereth_client_model::World` methods and `dereth-client-model` cannot depend on
    /// `dereth-client`. `Presence` therefore has no `state` field to read and this function has to
    /// be handed the table that does — which also means a caller cannot drive the body with a
    /// word the rest of the client disagrees about.
    pub fn sync(
        &mut self,
        store: &RetailDatStore,
        world: &mut PhysicsWorld,
        presences: &BTreeMap<ObjectId, Presence>,
        game: &dereth_client_model::World,
        exclude: Option<ObjectId>,
    ) {
        for handle in self.retired.drain(..) {
            world.destroy(handle);
            self.stats.destroyed += 1;
        }
        // 1. Gone: removed by `0xF747`, picked up (its position is now `None`), or the player's own
        //    object, which `Character` owns.
        let dead: Vec<ObjectId> = self
            .handles
            .keys()
            .copied()
            .filter(|id| {
                // The *wire's* word, not the body's. `Presence::position` is
                // now republished off this very body, so asking it
                // whether the object still has a position of its own would be asking the body
                // about itself. The fact wanted here is the received-position handler's and the
                // pickup event's: the server took the position away.
                Some(*id) == exclude
                    || presences
                        .get(id)
                        .is_none_or(|p| p.server_position.is_none() || p.parent.is_some())
                    // The hull was built at one placement and the object is now drawn
                    // at another, so it is rebuilt. Nothing on screen shows a stale hull, which is
                    // exactly why it is checked rather than assumed constant.
                    || presences
                        .get(id)
                        .is_some_and(|p| self.posed.get(id) != Some(&p.placement))
            })
            .collect();
        for id in dead {
            if let Some(h) = self.handles.remove(&id) {
                // `leave_cell` and `remove_shadows_from_cells`.
                world.destroy(h);
                self.placed.remove(&id);
                self.posed.remove(&id);
                self.stated.remove(&id);
                self.restricted.remove(&id);
                self.lost.remove(&id);
                self.stats.destroyed += 1;
            }
        }

        // 2. Here: created, and moved.
        for (&id, p) in presences {
            if Some(id) == exclude {
                continue;
            }
            // Create-object handling, step 4: `enter_world` runs for `pd.parent_id == 0` and a
            // non-zero `objcell_id`, and for nothing else.
            if p.parent.is_some() {
                continue;
            }
            // **This is the one reader of the wire's position, and it reads it as a
            // target.** Position-event handling keeps the received position on its stack and
            // hands it to move-or-teleport, then reads back only the body's position. So the
            // destination below, the `placed` memo that suppresses a redundant second
            // interpolation, and nothing else in this client may come from here —
            // `Presence::position` is the body's achieved pose and is what every *reader* sees.
            let Some(pos) = p.server_position else {
                continue;
            };
            // The physics state word, from the one table that holds it. An object this
            // table has and the game table does not is the `StaleInstance` disagreement
            // `ObjectStats::state_events_without_physics` counts; it takes
            // `PhysicsPresence::default()`'s word rather than being skipped, so a body is still
            // created and is still non-static, exactly as a zero descriptor word would make it.
            let state_word = game.physics(id).map_or(0, |q| q.state);
            let h = match self.handles.get(&id).copied() {
                Some(h) => h,
                None => {
                    if world.by_object_id(id).is_some() {
                        self.stats.id_collision += 1;
                        continue;
                    }
                    let Some(h) = self.spawn(store, world, id, p, state_word) else {
                        continue;
                    };
                    self.handles.insert(id, h);
                    self.posed.insert(id, p.placement);
                    // `spawn` has just run `set_state` with this word; recorded, not counted.
                    self.stated.insert(id, state_word);
                    self.stats.created += 1;
                    h
                }
            };
            // **The second writer of the state word.**
            //
            // Smart-box state application sets the physics object's state on the
            // object it looked up, which is a **live** body: the set-state handler queues
            // the blob when the object does not exist yet, so by the time `set_state` runs the
            // body is already in the cell lists. [`Self::spawn`] runs only for an object with no
            // handle, so without this arm a `0xF74B` would change the state word and nothing would
            // carry it any further.
            //
            // It is applied **in place** rather than by destroying and re-spawning the body, which
            // is the other thing this function knows how to do (it rebuilds on a placement
            // change). `set_state` is a field assignment plus three side effects; it does not
            // touch the part array, the cell registration or the transient state, and a re-spawn
            // would drop all three and re-run `enter_world` for a door that merely became
            // ethereal. The client's own state application reaches straight into the existing object
            // for the same reason.
            if self.stated.get(&id) != Some(&state_word) {
                if let Some(o) = world.get_mut(h) {
                    let physical =
                        with_hidden_collision_side_effects(o.state(), PhysicsState(state_word));
                    // `set_state` writes the incoming word, then
                    // `set_hidden` derives the collision flags on a HIDDEN edge: the
                    // hide tail clears REPORT and sets IGNORE; unhide
                    // clears IGNORE and sets REPORT. Keep `stated` as the
                    // server's raw word so an unchanged `0xF74B` does not fabricate another edge,
                    // while the live body carries retail's final physical word.
                    // Part lights and NODRAW still belong to the renderer's copy. HIDDEN draw,
                    // child and script effects are handled in `world.rs`.
                    let _ = o.set_state(physical);
                    self.stated.insert(id, state_word);
                    self.stats.state_applied += 1;
                }
            }
            // **Gameplay-object restrictions, the half the barrier reads.**
            //
            // Retail does not push anything: the cell-entry restriction check reads the body's
            // gameplay-object pointer and calls two virtuals on it. This build keeps the weenie in
            // `dereth_client_model` and the body in `dereth_physics`, so the answers those virtuals return are
            // copied across here, on the same diff-and-write shape the state word above uses.
            // Every object gets one, not just houses: a candidate's creature type, the mover's
            // own player/PK status and the move-restriction bypass come through the same field.
            let w = game.weenie(id).map(weenie_restrictions);
            if self.restricted.get(&id) != Some(&w) {
                world.set_weenie_restrictions(h, w.clone());
                self.restricted.insert(id, w);
                self.stats.weenies_pushed += 1;
            }
            if self.placed.get(&id) == Some(&pos) {
                continue;
            }
            // **The create and an ordinary position update are different calls.**
            //
            // Create-object handling is the only caller of world entry; position-event
            // handling hands a non-player object to move-or-teleport instead. Running
            // [`Self::place`] — `enter_world`, i.e. the full `set_position` placement with the
            // transition's 4 m scatter search — for **both** would move a remote body the server
            // parked against anything solid up to a metre off the position the server named:
            // measured at 0.4547 m, 0.5145 m, 0.8958 m and 1.0291 m for a creature 0.9, 0.75,
            // 0.25 and 0.5 m from a player's body, with the drawn position — the wire's — staying
            // put.
            //
            // The create keeps `enter_world`, and so does the **retry** of a body this client has
            // never managed to place: `placed` has no row for either, and retail's first
            // move-or-teleport test (the body has no cell) sends the second one to the same
            // full placement anyway.
            //
            // [`crate::objects::Presence::position_from_create`] is the third case and it is not
            // the same as "this id has no row": create-object handling calls `enter_world` on its
            // **merge** path too, so a re-create for an id the client already holds — which is
            // exactly how ACE re-tracks a player who walked back into view — is
            // still a create and must still be placed by it.
            let outcome = if self.placed.contains_key(&id) && !p.position_from_create {
                let arm = world.move_or_teleport(h, &pos, p.teleported, p.contact);
                match arm {
                    dereth_physics::MoveOrTeleport::Teleported { .. } => {
                        self.stats.teleport_arm += 1
                    }
                    dereth_physics::MoveOrTeleport::NotInContact => self.stats.no_contact_arm += 1,
                    dereth_physics::MoveOrTeleport::Interpolated { .. } => {
                        self.stats.interpolate_arm += 1;
                    }
                    dereth_physics::MoveOrTeleport::Snapped { .. } => self.stats.snap_arm += 1,
                }
                // `!contact` moved nothing, so the wire position must **not** be memoised as the
                // one the body is standing at: the memo is what suppresses the next sync's call.
                //
                // The `Interpolated` arm does not move the body *here* either:
                // Interpolation queues a position target on the object, and subsequent physics
                // sub-steps walk the body onto it. The memo is still written, because it records the
                // last position the server named and handed to physics — which is exactly what
                // suppresses a redundant second interpolation to the same position — and not
                // where the body is standing this frame.
                if !arm.moved() {
                    continue;
                }
                arm.placed()
            } else {
                Self::place(world, h, pos)
            };
            if outcome {
                self.placed.insert(id, pos);
                self.stats.moved += 1;
                // Cancel the scheduled destruction — the object has a cell again, so it
                // comes back off the queue. Only for an id **this** latch doomed: retail's
                // create-object handling removes unconditionally because it runs once, on the
                // create, while `place` here runs on every server position, and an unconditional
                // remove would cancel the leave-visibility preparation's scheduling for
                // any object that merely walked.
                if self.lost.remove(&id) {
                    self.verdicts.push((id, false));
                }
            } else {
                // Left out of every cell list, and retried next frame: an interior cell whose
                // block has not been prefetched resolves to nothing, and visible-cell lookup
                // is deliberately the only interior-cell lookup physics does.
                //
                // **"Left out of every cell list" is what the lines below make
                // true.** Internal object positioning's null-cell arm
                // is four statements, not a refusal:
                //
                // Adjust the destination using the sphere's center. If no cell is found,
                // prepare to leave visibility, store the destination, register the object
                // in the lost-cell table under its destination cell id, and clear ACTIVE_TS.
                //
                // Internal position setting runs `goto_lost_cell` for a cell its resolver
                // refuses. If the body kept the cell it was standing in instead,
                // `WorldScene::finish_object_physics` would republish that stale cell's position
                // as the object's draw position every frame afterwards.
                //
                // On the surviving player's client a dying player's `0xF748` to his lifestone is
                // exactly such a destination (an outdoor landblock this client has not loaded),
                // and the `0xF74B HIDDEN_PS` that follows it one message later plays
                // the hidden play script's fourteen **infinite** emitters on the body — so a body
                // pinned to the death site would keep the teleport shimmer burning where he fell
                // until the unhide arrived some seconds later.
                //
                // Removing shadows and leaving the cell are the physical half of preparing to
                // leave visibility, the same pair [`Self::leave_visibility`] uses; the destruction
                // scheduling half is the `lost`/`verdicts` latch below, and `goto_lost_cell` is
                // `ObjectStream`'s. `store_position` keeps the
                // **destination**, as retail does, so the retry below starts from where the
                // server put the object rather than from where it used to stand.
                world.remove_shadows_from_cells(h);
                world.leave_cell(h);
                if let Some(body) = world.get_mut(h) {
                    body.store_position(&pos);
                    body.transient_state.set_active_bit(false);
                }
                self.placed.remove(&id);
                self.stats.unplaced += 1;
                // The retry is only half of what retail does here; without the other half an
                // object the client can never place would stay on the screen for the life of the
                // session.
                //
                // Smart-box create-object handling ends by scheduling destruction when the
                // physics object's `objcell_id` is non-zero and it has no cell,
                // whose deadline is the current time plus the nonvisible-object destruction
                // interval = **25.0 s**.
                // The `objcell_id != 0` half is already this loop's own gate, and
                // [`Self::place`]'s `o.cell.is_some()` is the other half verbatim.
                //
                // The object is **drawn** the whole time it is in this state, because
                // `SceneObject::position` is the wire's and never asked physics whether the
                // placement was accepted — and it is drawn in the pass its *wire* cell selects,
                // which for the observed Holtburg door is the outdoor half.
                if self.lost.insert(id) {
                    self.verdicts.push((id, true));
                }
            }
        }

        // **The mover's own weenie, which the whole restriction check turns on.**
        //
        // The loop above skips `exclude`, the local player, because `Character` owns that body
        // and creating a second would put two solid bodies in one cell. But the player is exactly
        // the object from which the client reads a weenie: its
        // player bit gates the whole barrier, and its move-restriction bypass is the admin
        // exemption. Without this the player's body has `weenie == None`, the check returns
        // `OK_TS`, and no
        // house in the world fences anyone.
        //
        // `by_object_id` rather than a stored handle: `Character::adopt_server_id` re-keys the
        // body to the server's id, and `App::sync_objects` calls it immediately before this.
        if let Some(id) = exclude {
            if let Some(h) = world.by_object_id(id) {
                let w = game.weenie(id).map(weenie_restrictions);
                if self.restricted.get(&id) != Some(&w) {
                    world.set_weenie_restrictions(h, w.clone());
                    self.restricted.insert(id, w);
                    self.stats.weenies_pushed += 1;
                }
            }
        }

        // The one state change a person walking around cannot otherwise see, on the same terms as
        // the viewer-cell line: printed when it changes, not once a frame. The key is everything
        // the two lines print rather than the object count alone — see [`Self::report_key`].
        let line = self.report_key();
        if self.reported != Some(line) && self.report_gate.ready() {
            self.reported = Some(line);
            let s = self.stats;
            tracing::debug!(
                "{} object(s) solid -- {} created, {} destroyed, {} placed, \
                 {} unplaced, {} without a setup, {} setup id(s) that are not setups, \
                 {} id collision(s)",
                line.0,
                s.created,
                s.destroyed,
                s.moved,
                s.unplaced,
                s.without_setup,
                s.setup_not_a_setup,
                s.id_collision
            );
            // Printed beside it: how many of them are solid by their own mesh, and how many of
            // those would have no collision shape at all without it.
            tracing::debug!(
                "{} of them collide against their parts' own BSP \
                 {} of which carry no spherical collision shape and \
                 would be intangible without the BSP; {} setup part(s) loaded, {} carrying a tree, \
                 {} undecodable; {} state word(s) applied to a live body (0xF74B Item_SetState)",
                s.bsp_arm,
                s.bsp_arm_without_spheres,
                self.part_stats.parts,
                self.part_stats.parts_with_bsp,
                self.part_stats.part_undecodable,
                s.state_applied,
            );
        }
    }

    /// Object-creation placement results, as edges for the caller to apply to the
    /// object-lifetime manager.
    ///
    /// `(id, true)` schedules destruction — the placement ended with a non-zero
    /// `objcell_id` and a NULL `cell`. `(id, false)` cancels it —
    /// the same object has a cell again. Drained, so an edge is applied exactly once.
    pub(crate) fn take_placement_verdicts(&mut self) -> Vec<(ObjectId, bool)> {
        std::mem::take(&mut self.verdicts)
    }

    /// Whether the last placement of `id` left it with no cell — the latch
    /// [`Self::take_placement_verdicts`] reports the edges of. Published for the tests.
    #[must_use]
    pub fn is_lost(&self, id: ObjectId) -> bool {
        self.lost.contains(&id)
    }

    /// What the two log lines at the end of [`Self::sync`] are gated on.
    ///
    /// # What this key is a superset OF
    ///
    /// Those lines read `handles.len()`, **ten** of [`ObjectPhysicsStats`]'s eleven counters
    /// (everything but `setup_undecodable`) and **three** of [`SetupPartStats`]'s eight. This key
    /// is **both whole structs** plus the count, so it is a superset
    /// by construction and stays one when somebody adds a counter to the line — which is the point:
    /// an enumerated key is a superset until the next edit, and a whole-struct one is a superset
    /// afterwards too. It is therefore a **strict** superset today, by one field:
    /// `setup_undecodable` is counted and not printed, so it now makes the pair go out carrying
    /// every other number. That is the right way round — a condition with no line of its own at
    /// least reaches the log as the frame it happened on — and it is deliberate rather than a
    /// side effect of the whole-struct compare.
    ///
    /// `handles.len()` **alone** would be only a *projection* of the line it gates:
    /// `unplaced` and `id_collision` climb on objects that never entered `handles`, `state_applied`
    /// climbs on objects already in it, and `created` + `destroyed` can both move in one sync while
    /// the map size holds still. The counters would advance and the line never print. **A
    /// diagnostic that can go quiet is worse than no diagnostic**, because its silence reads as
    /// absence of the condition — and `unplaced` is precisely the number somebody debugging *"the
    /// world looks right and nothing collides"* would go looking for.
    ///
    /// # The two deliberate exceptions, and why they are not the same defect again
    ///
    /// `moved` and `unplaced` are the only two counters that climb on a **retry** rather than on an
    /// event: a placement that fails is re-attempted on the next sync, for as long as the interior
    /// cell's block is unprefetched, and a server position lands several times a second for
    /// anything that walks. Keyed on their raw values these two lines would print **every frame**,
    /// which destroys the log exactly as thoroughly as silence does. They therefore enter the key
    /// by their **base-2 magnitude**, so the condition is announced on its first occurrence and
    /// again at 2, 4, 8, 16 … — bounded at 64 lines apiece for the life of a session, never silent,
    /// never a flood. Every print carries the *current* raw value, so the number a reader sees is
    /// never the rounded one.
    fn report_key(&self) -> (usize, ObjectPhysicsStats, SetupPartStats) {
        /// `64 - leading_zeros`: 0 -> 0, 1 -> 1, 2..3 -> 2, 4..7 -> 3, …
        const fn magnitude(n: u64) -> u64 {
            (u64::BITS - n.leading_zeros()) as u64
        }
        let mut s = self.stats;
        s.moved = magnitude(s.moved);
        s.unplaced = magnitude(s.unplaced);
        // The four `MoveOrTeleport` arms are per-message, exactly as
        // `moved` is, so they take the same base-2 key rather than reprinting the pair every frame
        // a creature walks.
        s.teleport_arm = magnitude(s.teleport_arm);
        s.no_contact_arm = magnitude(s.no_contact_arm);
        s.interpolate_arm = magnitude(s.interpolate_arm);
        s.snap_arm = magnitude(s.snap_arm);
        (self.handles.len(), s, self.part_stats)
    }

    /// Create a body with the setup's collision half and the descriptor's `PhysicsState`.
    fn spawn(
        &mut self,
        store: &RetailDatStore,
        world: &mut PhysicsWorld,
        id: ObjectId,
        p: &Presence,
        state_word: u32,
    ) -> Option<PhysHandle> {
        let geometry = match p.setup_id {
            Some(s) => self.setup_geometry(store, s.0, p.placement)?,
            None => {
                self.stats.without_setup += 1;
                // A physics object with no part array: `path_spheres()` is empty, so
                // object-collision checking runs no sphere and answers `OK_TS`.
                // It is still in the cell lists, exactly as the client's is.
                Arc::new(SetupGeometry::default())
            }
        };
        // State setting writes the whole word the descriptor carried;
        // `STATIC_PS` is read back out of it because `PhysicsObj::new` takes the same fact as its
        // `dynamic` argument, and because `set_active` refuses a static object.
        if geometry.caches_physics_bsp() {
            self.stats.bsp_arm += 1;
            if geometry.spheres.is_empty() {
                self.stats.bsp_arm_without_spheres += 1;
            }
        }
        let state = PhysicsState(state_word);
        let h = world.create(id, geometry, !state.is_static());
        let o = world.get_mut(h)?;
        // Physics-object construction seeds `0x00400C08` (no HIDDEN), object creation changes
        // only the STATIC bit, and applying the description
        // calls `set_state` with the create's word. An initially hidden descriptor therefore runs
        // the same collision-flag derivation as a later `0xF74B` edge. Part lights, NODRAW, scripts
        // and child rendering remain the renderer/effects seam.
        let physical = with_hidden_collision_side_effects(o.state(), state);
        let _ = o.set_state(physical);
        // A zero or absent scale is the client's 1.0.
        o.scale = if p.scale > 0.0 { p.scale } else { 1.0 };
        Some(h)
    }

    /// Entering the world with a position handles object creation's
    /// arrival, and every subsequent server position for the same object.
    ///
    /// This is the full placement, not forced cell insertion. Forced insertion is the bypass
    /// internal positioning takes for an object whose weenie reports one of three exemption
    /// states — **hook**, **storage** and **corpse** objects — and runs no transition at all, so a
    /// hooked item keeps the height the server assigned; it is not a general "non-colliding
    /// object" arm. It takes the wire's `objcell_id` verbatim after `adjust_to_outside`, which
    /// normalises an outdoor landcell index and never descends into a building. Used here, an
    /// object created at an **outdoor** landcell while standing inside a building's interior cell
    /// would keep the outdoor cell for ever — and an outdoor cell's objects are drawn inside the
    /// outdoor pass, before the clear and before the interior cells are painted over the top. A
    /// door standing two metres in front of the camera inside Holtburg's house painted **1 px of
    /// 1,080,000** that way; the same door in the cell that contains it painted 321,487.
    ///
    /// The placement transition in `run_transition` is seeded from the cell resolved from the
    /// **destination**, not from the object's own cell: an object the create path is about has
    /// no cell yet, and seeding from it would refuse every placement, *including* an object in
    /// the open on bare terrain. Storing the position carries the outdoor normalisation.
    ///
    /// The cell the object ends up in is therefore the one the placement transition resolved —
    /// the cell-list search descends into the building
    /// standing on the land cell — and not the one the wire named.
    ///
    /// **Residency.** Position adjustment resolves an outdoor id through visible-cell lookup,
    /// landscape cell lookup and finally landblock lookup. The last step returns NULL for a block
    /// absent from the resident array or whose load state is not `8` (loaded). A teleport
    /// changes the cell manager's position and releases every landscape block, including
    /// each slot's landblock contents, so
    /// an object the server names for a landblock the client has left cannot be placed there at
    /// all, and create-object handling's tail queues it for destruction instead.
    ///
    /// [`dereth_world_data::land_source::DatLandSource`] loads a block on demand and caches it for ever, so
    /// `landblock()` alone answers "yes" for every block in Dereth. Without the residency check
    /// that would not merely leak the late arrivals — it would undo a landblock release on the
    /// next frame. [`Self::leave_visibility`] clears the object's [`Self::placed`] memo on
    /// purpose, so a cell loaded later can re-enter it;
    /// the next `ObjectStream::sync_physics_at` therefore retries `place` from the **unchanged
    /// wire position**, and against a land source that still answers for the released block that
    /// retry would *succeed*: the object would take its old cell straight back,
    /// `publish_physics_cells` would republish it, and the release a frame earlier would be
    /// reverted, for a teleport **and** for a ring shift.
    ///
    /// The interior half: [`dereth_physics::source::LandSource::env_cell`]
    /// only looks up visible cells and never loads them, so an interior cell of a released
    /// block resolves to nothing.
    ///
    /// The residency check lives where retail keeps it, inside visible-cell lookup
    /// ([`dereth_physics::CellResolver::get_visible`]), not at this caller. Asked here it would
    /// cover exactly one caller — this placement — and leave every transition, cell-array build,
    /// collision sweep and `find_cell_list` resolving cells in blocks the landscape cache had
    /// already released; it would also short-circuit **in front of** the normal positioning path,
    /// so none of the null-cell arm's four statements would run.
    ///
    /// As it is, `enter_world` -> `set_position_internal` -> position adjustment reaches the
    /// null-cell arm on its own, and this is the client's create-object step 4 exactly:
    /// `enter_world`'s `int`, and the test for a physics object with no cell. The lost-cell arm
    /// returns `OK_SPE`, which is why the cell — not `enter_world`'s answer — is what decides.
    fn place(world: &mut PhysicsWorld, h: PhysHandle, pos: Position) -> bool {
        world.enter_world(h, &pos) && world.get(h).is_some_and(|o| o.cell.is_some())
    }

    /// One setup record's collision half at one placement, memoised. `None` for anything that will not
    /// produce one.
    fn setup_geometry(
        &mut self,
        store: &RetailDatStore,
        id: u32,
        placement: u32,
    ) -> Option<Arc<SetupGeometry>> {
        if let Some(hit) = self.geometry.get(&(id, placement)) {
            return hit.clone();
        }
        let loaded = if id >> 24 == 0x02 {
            let did = DataId(id);
            match store
                .read_typed(DbType::Setup, did)
                .ok()
                .and_then(|b| Setup::decode_payload_in(store.era_of(did), did, &b).ok())
            {
                Some(s) => Some(Arc::new(if self.mesh_collision {
                    setup_geometry_with_parts_at(store, &s, placement, &mut self.part_stats)
                } else {
                    dereth_world_data::setup::setup_geometry(&s)
                })),
                None => {
                    self.stats.setup_undecodable += 1;
                    None
                }
            }
        } else if id >> 24 == 0x01 && self.mesh_collision {
            // Physics-object construction accepts a `0x01000000` `GfxObj` id and wraps it with
            // a one-part setup with **no** spheres and the
            // gfxobj's physics BSP. Without mesh collision it is counted and dropped instead of
            // approximated, because a sphere the client would not have used is worse than
            // nothing.
            let loaded = simple_setup_geometry(store, DataId(id), &mut self.part_stats);
            if loaded.is_none() {
                self.stats.setup_undecodable += 1;
            }
            loaded.map(Arc::new)
        } else {
            self.stats.setup_not_a_setup += 1;
            None
        };
        self.geometry.insert((id, placement), loaded.clone());
        loaded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every counter the two lines print, with the value one would have to change to move it.
    /// Enumerated here **as literals** and not derived from the key, because a test that reads a
    /// key through the same function it writes it through cannot detect a missing field.
    #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
    fn every_printed_counter() -> Vec<(&'static str, fn(&mut ObjectPhysics))> {
        vec![
            ("created", |o| o.stats.created += 1),
            ("destroyed", |o| o.stats.destroyed += 1),
            ("without_setup", |o| o.stats.without_setup += 1),
            ("setup_not_a_setup", |o| o.stats.setup_not_a_setup += 1),
            ("id_collision", |o| o.stats.id_collision += 1),
            ("bsp_arm", |o| o.stats.bsp_arm += 1),
            ("bsp_arm_without_spheres", |o| {
                o.stats.bsp_arm_without_spheres += 1
            }),
            ("state_applied", |o| o.stats.state_applied += 1),
            ("parts", |o| o.part_stats.parts += 1),
            ("parts_with_bsp", |o| o.part_stats.parts_with_bsp += 1),
            ("part_undecodable", |o| o.part_stats.part_undecodable += 1),
        ]
    }

    // The defect: the throttle was `handles.len()`, so every one of these could climb with the map
    // size held still and the line that prints them never went out. `id_collision` is the sharpest
    // — an object that collides never enters `handles` at all — and `created`/`destroyed` moving
    // together in one sync is the case the count cannot see even in principle.
    #[test]
    fn no_counter_the_line_prints_can_climb_without_moving_the_throttle() {
        for (name, bump) in every_printed_counter() {
            let mut o = ObjectPhysics::default();
            let before = o.report_key();
            bump(&mut o);
            assert_ne!(
                before,
                o.report_key(),
                "{name} climbs and the log line stays silent"
            );
            // And the object count is genuinely unchanged, so the old throttle really could not
            // have seen it: without this the test would pass against `handles.len()` alone.
            assert_eq!(
                before.0,
                o.report_key().0,
                "{name} moved the object count too"
            );
        }
    }

    // The two exceptions, stated as behaviour rather than left in a comment. Both climb on a
    // **retry** — an unplaceable object is re-attempted every sync — so keyed raw they would print
    // a line per frame, which loses the log as completely as silence does.
    #[test]
    fn the_two_retry_counters_announce_themselves_and_then_report_on_a_log_scale() {
        let mut o = ObjectPhysics::default();
        let quiet = o.report_key();

        // The condition appearing is always news.
        o.stats.unplaced = 1;
        let first = o.report_key();
        assert_ne!(quiet, first, "the first unplaced object was never reported");

        // Doubling is news too, so the number cannot run away unseen: 1, 2, 4, 8 ...
        o.stats.unplaced = 2;
        let second = o.report_key();
        assert_ne!(first, second, "unplaced doubled in silence");

        // A further retry inside the same octave is not, or a stuck object floods the log.
        o.stats.unplaced = 3;
        assert_eq!(
            second,
            o.report_key(),
            "a retried placement prints a line per frame"
        );

        // `moved` is the same shape, and its own reverse control.
        let mut m = ObjectPhysics::default();
        let base = m.report_key();
        m.stats.moved = 1;
        assert_ne!(base, m.report_key());
        let one = m.report_key();
        m.stats.moved = 1000;
        assert_ne!(
            one,
            m.report_key(),
            "a thousand moves and the census never printed again"
        );
    }

    // The reverse calibration for the two tests above: a `report_key` that ignored its inputs
    // entirely would satisfy neither, but one that simply returned the whole stats struct raw would
    // satisfy the first and fail the second. This pins the property that separates them.
    #[test]
    fn a_frame_in_which_nothing_happened_does_not_reprint() {
        let mut o = ObjectPhysics::default();
        o.stats.created = 12;
        o.stats.unplaced = 7;
        o.part_stats.parts = 300;
        let a = o.report_key();
        assert_eq!(
            a,
            o.report_key(),
            "the key is not stable across two reads of the same state"
        );
    }
}
