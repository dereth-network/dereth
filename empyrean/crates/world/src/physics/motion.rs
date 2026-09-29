// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Physics/PhysicsObj.cs, Source/ACE.Server/Physics/Animation/MovementParameters.cs, Source/ACE.Server/Physics/Animation/RawMotionState.cs, Source/ACE.Server/Physics/Managers/MovementManager.cs
//! Motion on server physics bodies: the shared `dereth-animation` motion stack
//! (`MotionDriver`: `MotionInterp`, `MoveToManager`, sticky and the target subscription) attached
//! to the bodies `phys_ext` makes, and ACE's `PhysicsObj` motion calls by ACE's names.
//!
//! **How a body carries its motion.** ACE's `PhysicsObj` owns a `PartArray` (with its
//! `MotionTableManager`) and a `MovementManager`. Here one `dereth-animation` `MotionDriver` holds both,
//! and the body's `MotionSource` seam is a thin adapter over a shared handle to it
//! ([`DriverSource`]), so the physics step advances the animation and the movement tick exactly as
//! the client's physics does (V1), while gameplay reaches the same driver through the
//! functions below. A body has a driver from `SetMotionTableID` (a non-zero table), or lazily from
//! the calls ACE makes create a `MovementManager` (`get_minterp`, `MoveToObject`, `TurnTo*`).
//!
//! **What the shared crate answers as data, the host applies** (the client does the same):
//!
//! * the movement layer's physics requests (`MotionEffect`: set heading, local velocity, on
//!   walkable) are applied to the body after each call and after each physics update;
//! * the target subscription (ACE's `TargetManager`) is resolved here: the watched object's
//!   position and cached velocity are read off its body and handed to the driver, whose
//!   per-sub-step tick sends the updates; a fresh subscription gets its first update at once;
//! * completions route to the world object: every `MotionDone` to `WeenieObject.OnMotionDone`
//!   (ACE's `MotionTableManager` does this) and every end of a MoveTo/TurnTo to
//!   `WeenieObject.OnMoveComplete` (ACE's `MoveToManager.CleanUpAndCallWeenie`, server custom).
//!
//! **Differences from ACE's physics port** (V1: the retail-faithful shared crate wins). Each is a
//! `retail` divergence:
//!
//! * the movement internals use the client's `MovementParameters` defaults (`0x1EE0F`, walk/run
//!   threshold 15) where ACE's internals use its own (`CanCharge`, 1.0); gameplay-built MoveTos
//!   take retail's parameters per kind of move (V257, `RetailMoveTo`), and the other
//!   gameplay-built parameters still start from ACE's constructor ([`ace_movement_parameters`]);
//! * ACE's server customs in `MoveToManager` are absent: the "in range while closing" arrival and
//!   the heading snap after an aux turn (`AlwaysTurn` is the shared manager's host option, V96);
//! * `HitGround`/`LeaveGround` re-apply the interpreted movement (ACE: raw for an autonomous
//!   creature), with no long-jump hint (ACE passes `allowJump = true`);
//! * notices are delivered after the call or the physics update that raised them, `MotionDone`s
//!   first, rather than synchronously inside the step (the V6 family);
//! * target updates are computed watcher-side from the target's body (retail's voyeur tick) and
//!   need no update of the target itself.

use std::cell::{RefCell, RefMut};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use dereth_animation::command::MotionCommand;
use dereth_animation::data::{
    AnimAssets, AnimationData, DegradeInfo, MotionTableData, ParticleEmitterInfo,
    PhysicsScriptData, PhysicsScriptTableData, SetupData, Sphere as AnimSphere,
};
use dereth_animation::hooks::AnimEvent;
use dereth_animation::motion::interp::{motion_allows_jump, MotionCtx};
use dereth_animation::motion::moveto::{MoveToRequest, TargetSnapshot};
use dereth_animation::motion::{
    flags, HoldKey, InterpretedMotionState, MotionEffect, MotionInterp, MovementManager,
    MovementParameters, RawMotionState, DEFAULT_FLAGS,
};
use dereth_animation::table::{MovementStruct, MovementType};
use dereth_animation::MotionDriver;
use dereth_physics::{MotionSource, PhysHandle};
use dereth_primitives::{
    DataId, Frame, LocalTime, ObjectId, Position as PPosition, ServerTime, Vec3,
};
use dereth_world_data::anim_convert as convert;
use empyrean_dat::file_types::{Animation, MotionTable, SetupModel};
use empyrean_dat::DatManager;
use empyrean_entity::enums::WeenieError;

use crate::physics::phys_ext::{self, physics_timer_current_time};
use crate::physics::server_object_manager;
use crate::World;

// ---------------------------------------------------------------------------------------------
// Assets: the portal dat through the shared converters
// ---------------------------------------------------------------------------------------------

/// The motion tables, animations and setups the drivers read, from the portal dat through
/// the shared converters (`dereth_world_data::anim_convert`), each converted once. The server draws nothing, so physics scripts,
/// particle emitters and degrade tables answer "absent" (ACE's server executes no animation hook
/// but `AnimationDone` either).
pub struct ServerAnimAssets {
    dats: Arc<DatManager>,
    motion_tables: Mutex<HashMap<u32, Option<Arc<MotionTableData>>>>,
    animations: Mutex<HashMap<u32, Option<Arc<AnimationData>>>>,
    setups: Mutex<HashMap<u32, Option<Arc<SetupData>>>>,
}

impl std::fmt::Debug for ServerAnimAssets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerAnimAssets").finish_non_exhaustive()
    }
}

impl ServerAnimAssets {
    #[must_use]
    pub fn new(dats: Arc<DatManager>) -> Self {
        Self {
            dats,
            motion_tables: Mutex::new(HashMap::new()),
            animations: Mutex::new(HashMap::new()),
            setups: Mutex::new(HashMap::new()),
        }
    }

    fn cached<T, D: empyrean_dat::database::DatFileType>(
        &self,
        cache: &Mutex<HashMap<u32, Option<Arc<T>>>>,
        id: DataId,
        convert: impl FnOnce(&D) -> T,
    ) -> Option<Arc<T>> {
        let mut cache = cache.lock().unwrap_or_else(PoisonError::into_inner);
        cache
            .entry(id.0)
            .or_insert_with(|| {
                self.dats
                    .portal_dat()
                    .read_from_dat::<D>(id.0)
                    .map(|d| Arc::new(convert(&d)))
            })
            .clone()
    }
}

impl AnimAssets for ServerAnimAssets {
    fn motion_table(&self, id: DataId) -> Option<Arc<MotionTableData>> {
        self.cached::<_, MotionTable>(&self.motion_tables, id, convert::motion_table)
    }
    fn animation(&self, id: DataId) -> Option<Arc<AnimationData>> {
        self.cached::<_, Animation>(&self.animations, id, convert::animation)
    }
    fn setup(&self, id: DataId) -> Option<Arc<SetupData>> {
        self.cached::<_, SetupModel>(&self.setups, id, convert::setup)
    }
    fn script(&self, _: DataId) -> Option<Arc<PhysicsScriptData>> {
        None
    }
    fn script_table(&self, _: DataId) -> Option<Arc<PhysicsScriptTableData>> {
        None
    }
    fn emitter_info(&self, _: DataId) -> Option<Arc<ParticleEmitterInfo>> {
        None
    }
    fn degrade_info(&self, _: DataId) -> Option<Arc<DegradeInfo>> {
        None
    }
}

// ---------------------------------------------------------------------------------------------
// The per-body record and the MotionSource adapter
// ---------------------------------------------------------------------------------------------

/// One body's motion: the shared driver (ACE's `PartArray` motion half and `MovementManager`).
#[derive(Debug)]
pub struct BodyMotion {
    driver: Rc<RefCell<MotionDriver>>,
    /// `WeenieObject.InqRunRate`'s answer, set by a test while that inquiry is `not_ported!`
    /// (skills). `None`: ask the `WeenieObject`.
    pub run_rate_stand_in: Option<f32>,
    /// The status of the last `OnMoveComplete` routed for this body (diagnostics and tests).
    pub last_move_complete: Option<WeenieError>,
}

/// Borrows a driver. Re-entry is a porting bug: the driver is never held across a call into
/// physics or gameplay. (`MotionDriver` is not `Send`, so a shared `Rc` is all it can be.)
fn lock(d: &RefCell<MotionDriver>) -> RefMut<'_, MotionDriver> {
    d.try_borrow_mut()
        .expect("motion driver re-entered while borrowed")
}

/// The body's `MotionSource`: forwards to the shared driver, refreshing the physics facts the
/// movement layer reads live, and running sticky's offset adjustment (the position manager).
#[derive(Debug)]
struct DriverSource(Rc<RefCell<MotionDriver>>);

impl MotionSource for DriverSource {
    fn sync_physics_state(&mut self, state: dereth_primitives::MotionPhysicsState) {
        let mut d = lock(&self.0);
        d.env.object_id = state.object_id;
        d.env.position = state.position;
        d.env.velocity = state.velocity;
        d.env.cached_velocity = state.cached_velocity;
        d.env.radius = state.radius;
        d.env.height = state.height;
        d.env.in_cell = state.in_cell;
        d.env.contact = state.contact;
        d.env.on_ground = state.on_ground;
        d.env.gravity_affected = state.gravity_affected;
    }

    fn adjust_position_offset(&mut self, offset: &mut Frame, quantum: f64) {
        let d = lock(&self.0);
        #[allow(clippy::cast_possible_truncation)] // retail receives a float quantum
        d.movement.sticky.adjust_offset(
            offset,
            quantum as f32,
            &d.env.position,
            d.env.radius,
            Some(&d.movement.interp),
            &d.env,
        );
    }

    fn advance(&mut self, quantum: f64) -> Frame {
        lock(&self.0).advance(quantum)
    }

    fn tick_movement(&mut self, now: LocalTime) {
        let mut d = lock(&self.0);
        d.cur_time = ServerTime(now.0);
        d.tick_movement(now);
    }

    /// Physics sets contact before walkable, so both facts hold when the edge fires.
    fn hit_ground(&mut self) {
        let mut d = lock(&self.0);
        d.env.contact = true;
        d.env.on_ground = true;
        d.hit_ground();
    }

    fn leave_ground(&mut self) {
        let mut d = lock(&self.0);
        d.env.on_ground = false;
        d.leave_ground();
    }

    fn process_hooks(&mut self) {
        lock(&self.0).process_hooks();
    }

    fn has_collision_geometry(&self) -> bool {
        lock(&self.0).has_collision_geometry()
    }

    fn motion_max_speed(&self, use_adjusted: bool) -> Option<f32> {
        lock(&self.0).motion_max_speed(use_adjusted)
    }

    fn is_moving_to(&self) -> bool {
        lock(&self.0).is_moving_to()
    }
}

fn driver_of(w: &World, h: PhysHandle) -> Option<Rc<RefCell<MotionDriver>>> {
    phys_ext::ext(w, h)
        .and_then(|e| e.motion.as_ref())
        .map(|m| Rc::clone(&m.driver))
}

/// Whether the body has a driver (ACE: `MovementManager != null`).
#[must_use]
pub fn has_movement_manager(w: &World, h: PhysHandle) -> bool {
    driver_of(w, h).is_some()
}

/// A read of the body's driver, for queries and tests.
pub fn with_driver<R>(w: &World, h: PhysHandle, f: impl FnOnce(&MotionDriver) -> R) -> Option<R> {
    driver_of(w, h).map(|d| f(&lock(&d)))
}

/// The animation half of a setup the portal dat does not have (a test's synthetic setup): its
/// collision spheres and size, one placeholder part.
fn setup_from_geometry(g: &dereth_physics::SetupGeometry) -> SetupData {
    SetupData {
        parts: vec![DataId(0x0100_0000)],
        spheres: g
            .spheres
            .iter()
            .map(|s| AnimSphere {
                center: s.center,
                radius: s.radius,
            })
            .collect(),
        height: g.height,
        radius: g.radius,
        step_up_height: g.step_up_height,
        step_down_height: g.step_down_height,
        allow_free_heading: g.allow_free_heading,
        ..SetupData::default()
    }
}

/// Builds the body's driver with its setup and table, attaches it to the body and records it.
/// `false` when the body has no part array or its setup cannot make one.
fn attach(w: &mut World, h: PhysHandle, mtable_id: u32) -> bool {
    let Some(e) = phys_ext::ext(w, h) else {
        return false;
    };
    if !e.has_part_array {
        return false;
    }
    let setup_id = e.setup_id;
    let assets = Arc::clone(&w.phys_ext.motion_assets);
    // The part array comes from the same source as the collision geometry (`phys_ext`'s
    // `setup_geometry`): a test's synthetic setup wins over the dat's file of the same id. Reading
    // the dat first gave a body collision spheres from one setup and parts from another; a dat
    // setup with no parts then left a body that collides and falls with no movement manager, so
    // every MoveTo it was given did nothing.
    let from_body = || {
        w.physics
            .get(h)
            .map(|o| Arc::new(setup_from_geometry(&o.geometry)))
    };
    let setup = if phys_ext::has_synthetic_setup(w, setup_id) {
        from_body()
    } else {
        assets.setup(DataId(setup_id)).or_else(from_body)
    };
    let Some(setup) = setup else { return false };

    let mut d = MotionDriver::new(assets as Arc<dyn AnimAssets>);
    if !d.set_setup(setup) {
        log::warn!(
            "{:08X}: setup {setup_id:08X} makes no part array; no motion",
            phys_ext::id(w, h).unwrap_or(0)
        );
        return false;
    }
    let run_rate = inq_run_rate(w, h);
    refresh_env(w, h, &mut d, run_rate);
    // PartArray.SetMotionTableID, then MovementManager.EnterDefaultState. A table the dat does
    // not have leaves the manager tableless, as ACE's empty table does.
    if mtable_id == 0 || !d.set_motion_table(DataId(mtable_id)) {
        d.with_movement(|mm, ctx| mm.enter_default_state(ctx));
    }

    let driver = Rc::new(RefCell::new(d));
    if let Some(o) = w.physics.get_mut(h) {
        o.set_motion(Box::new(DriverSource(Rc::clone(&driver))));
    }
    if let Some(e) = phys_ext::ext_mut(w, h) {
        e.motion = Some(BodyMotion {
            driver,
            run_rate_stand_in: None,
            last_move_complete: None,
        });
    }
    true
}

/// Drops the body's driver (ACE: `MovementManager = null`).
fn detach(w: &mut World, h: PhysHandle) {
    if let Some(e) = phys_ext::ext_mut(w, h) {
        e.motion = None;
    }
    if let Some(o) = w.physics.get_mut(h) {
        o.motion = None;
    }
}

/// The motion half of `SetMotionTableID`, after the part array test: `MovementManager = null`,
/// then `MakeMovementManager(true)` for a non-zero table.
pub(crate) fn set_motion_table_id(w: &mut World, h: PhysHandle, mtable_id: u32) {
    detach(w, h);
    if mtable_id != 0 {
        make_movement_manager(w, h, true);
    }
}

/// Creates the movement manager (entering the default state) and wakes a non-static body.
// ACE: PhysicsObj.MakeMovementManager
pub fn make_movement_manager(w: &mut World, h: PhysHandle, init_motion: bool) {
    if has_movement_manager(w, h) {
        return;
    }

    // `init_motion`: the shared driver always enters its default state when it is made.
    let _ = init_motion;
    let mtable_id = phys_ext::ext(w, h).map_or(0, |e| e.motion_table_id);
    if !attach(w, h, mtable_id) {
        return;
    }

    wake(w, h, true);
}

/// How the lazily-made movement manager's callers touch the body's activity.
#[derive(Clone, Copy)]
enum LazyWake {
    /// `MoveToObject`, `TurnToHeading`, `TurnToObject_Internal`: a non-static body is **cleared**
    /// active (ACE's copy of the refactor comment clears where `MakeMovementManager` sets; the
    /// movement call that follows sets it again).
    ClearActive,
    /// `get_minterp`: a **static** body is set active.
    Minterp,
}

/// ACE's `if (MovementManager == null) { Create; EnterDefaultState; <activity> }` prologue.
fn ensure_movement_manager(w: &mut World, h: PhysHandle, how: LazyWake) {
    if has_movement_manager(w, h) {
        return;
    }
    let mtable_id = phys_ext::ext(w, h).map_or(0, |e| e.motion_table_id);
    if !attach(w, h, mtable_id) {
        return;
    }
    let now = physics_timer_current_time(w);
    let Some(o) = w.physics.get_mut(h) else {
        return;
    };
    match how {
        LazyWake::ClearActive => {
            if !o.state.is_static() {
                if !o.transient_state.is_active() {
                    o.update_time = now;
                }
                o.transient_state.set_active_bit(false);
            }
        }
        LazyWake::Minterp => {
            if o.state.is_static() {
                if o.transient_state.is_active() {
                    o.update_time = now;
                }
                o.transient_state.set_active_bit(true);
            }
        }
    }
}

/// `if (!Static) { if (!Active) UpdateTime = now; Active = true; }` (`set` false: nothing).
fn wake(w: &mut World, h: PhysHandle, set: bool) {
    let now = physics_timer_current_time(w);
    let Some(o) = w.physics.get_mut(h) else {
        return;
    };
    if set && !o.state.is_static() {
        if !o.transient_state.is_active() {
            o.update_time = now;
        }
        o.transient_state.set_active_bit(true);
    }
}

// ---------------------------------------------------------------------------------------------
// The environment, the target subscription, effects and notices
// ---------------------------------------------------------------------------------------------

/// The run rate the movement layer uses: `WeenieObject.InqRunRate` (or a test's stand-in). The
/// inquiry reads the creature's Run skill (it may add the record, and fills the enchantment
/// caches), so it takes the world mutably and runs before the driver is borrowed.
fn inq_run_rate(w: &mut World, h: PhysHandle) -> Option<f32> {
    if let Some(r) = phys_ext::ext(w, h)
        .and_then(|e| e.motion.as_ref())
        .and_then(|m| m.run_rate_stand_in)
    {
        return Some(r);
    }
    let (answered, rate) = phys_ext::weenie_obj(w, h).inq_run_rate(w);
    answered.then_some(rate)
}

/// Refreshes the physics and weenie facts the movement layer reads (ACE reaches through its
/// `PhysicsObj` and `WeenieObj` for them live). `run_rate`: [`inq_run_rate`], asked just before.
fn refresh_env(w: &World, h: PhysHandle, d: &mut MotionDriver, run_rate: Option<f32>) {
    let now = physics_timer_current_time(w);
    d.env.cur_time = LocalTime(now);
    d.cur_time = ServerTime(now);
    if let Some(o) = w.physics.get(h) {
        d.env.object_id = o.id;
        d.env.position = o.position;
        d.env.velocity = o.velocity_vector;
        d.env.cached_velocity = o.cached_velocity;
        d.env.radius = o.radius();
        d.env.height = o.height();
        d.env.in_cell = o.cell.is_some();
        d.env.contact = o.transient_state.in_contact();
        d.env.on_ground = o.transient_state.in_contact() && o.transient_state.on_walkable();
        d.env.gravity_affected = o.state.has_gravity();
    }
    // every server body has a WeenieObject (a DummyObject at worst)
    d.env.has_weenie = true;
    d.env.is_creature = phys_ext::weenie_obj(w, h).is_creature;
    d.env.run_rate = run_rate;
}

/// The watched object's facts off its body (retail's `SendVoyeurUpdate` reads the same three):
/// its position and cached velocity, or "gone" with the watcher's own position.
fn target_snapshot(w: &World, h: PhysHandle, target: ObjectId) -> TargetSnapshot {
    let body = server_object_manager::get_object_a(w, target.0)
        .and_then(|t| w.physics.get(t))
        .filter(|o| o.cell.is_some());
    match body {
        Some(o) => TargetSnapshot {
            object_id: target,
            ok: true,
            position: o.position,
            velocity: o.cached_velocity,
        },
        None => TargetSnapshot {
            object_id: target,
            ok: false,
            position: w.physics.get(h).map_or_else(
                || PPosition::new(dereth_primitives::CellId(0), Frame::default()),
                |o| o.position,
            ),
            velocity: Vec3::ZERO,
        },
    }
}

/// Hands the driver its target's current facts (ACE's `TargetManager`, watcher side).
fn resolve_target(w: &World, h: PhysHandle, d: &mut MotionDriver) {
    let snapshot = d.target.map(|t| target_snapshot(w, h, t.id));
    d.set_target_snapshot(snapshot);
}

/// What a batch of motion work started from.
#[derive(Debug, Clone, Copy)]
pub struct Ticket {
    driver_moving: bool,
    starts_move_to: bool,
}

/// Applies the driver's physics requests to the body and routes its notices, until it raises no
/// more. `ticket`: whether a MoveTo was in flight before, and whether this batch began a new one
/// (its prefix cancels the old one first).
fn settle(w: &mut World, h: PhysHandle, driver: &Rc<RefCell<MotionDriver>>, ticket: Ticket) {
    let mut failures: Vec<u32> = Vec::new();
    let mut done: Vec<(MotionCommand, bool)> = Vec::new();

    for _ in 0..32 {
        // a fresh subscription's first update goes out at once (retail: inside SetTarget)
        let (mut effects, events) = {
            let mut d = lock(driver);
            let mut effects = d.take_effects();
            resolve_target(w, h, &mut d);
            let now = LocalTime(physics_timer_current_time(w));
            if let Some(fx) = d.deliver_first_target_update(now) {
                effects.extend(fx);
            }
            (effects, d.take_events())
        };
        {
            let mut d = lock(driver);
            effects.extend(d.take_effects());
        }
        for e in events {
            if let AnimEvent::MotionDone { motion, success } = e {
                done.push((motion, success));
            }
        }
        if effects.is_empty() {
            let d = lock(driver);
            if d.pending_events() == 0 {
                break;
            }
            continue;
        }
        let now = physics_timer_current_time(w);
        for e in effects {
            match e {
                MotionEffect::SetHeading(heading) => {
                    if let Some(o) = w.physics.get_mut(h) {
                        o.set_heading(heading);
                    }
                }
                MotionEffect::SetLocalVelocity(v) => {
                    if let Some(o) = w.physics.get_mut(h) {
                        o.set_local_velocity(v, now);
                    }
                }
                MotionEffect::SetOnWalkable(on) => {
                    if let Some(o) = w.physics.get_mut(h) {
                        o.set_on_walkable(on);
                    }
                }
                MotionEffect::MoveToFailed(err) => failures.push(err),
                // An owner of both managers resolves these itself; an isolated interpreter call
                // raises them, and they are the owner's calls.
                MotionEffect::CancelMoveTo => {
                    lock(driver).with_movement(|mm, ctx| {
                        mm.cancel_move_to(dereth_animation::motion::interp::ACTION_CANCELLED, ctx)
                    });
                }
                MotionEffect::UnstickFromObject => {
                    lock(driver).with_movement(|mm, ctx| mm.unstick_from_object(ctx));
                }
                MotionEffect::StickTo { id, radius, .. } => {
                    lock(driver).with_movement(|mm, ctx| mm.stick_to_object(id, radius, now, ctx));
                }
                MotionEffect::SetTarget { .. }
                | MotionEffect::ClearTarget
                | MotionEffect::SetTargetQuantum(_) => {}
            }
        }
    }

    // routing (WeenieObject): motions first, then the end of the MoveTo
    let wobj = phys_ext::weenie_obj(w, h);
    let routable = wobj.world_object(w).is_some();
    for (motion, success) in done {
        if routable {
            wobj.on_motion_done(w, motion.0, success);
        }
    }

    let moving_now = lock(driver).movement.is_moving_to();
    let old_cancels =
        usize::from(ticket.starts_move_to && ticket.driver_moving).min(failures.len());
    let own_failures = failures.len() - old_cancels;
    let mut statuses: Vec<WeenieError> = failures
        .iter()
        .map(|&e| WeenieError(i32::try_from(e).unwrap_or(i32::MAX)))
        .collect();
    let was_moving = ticket.starts_move_to || ticket.driver_moving;
    if was_moving && !moving_now && own_failures == 0 {
        statuses.push(WeenieError::None);
    }
    if let Some(&last) = statuses.last() {
        if let Some(m) = phys_ext::ext_mut(w, h).and_then(|e| e.motion.as_mut()) {
            m.last_move_complete = Some(last);
        }
    }
    for status in statuses {
        if routable {
            wobj.on_move_complete(w, status);
        }
    }
}

/// Before a physics update: refresh the driver's facts and its target's.
pub(crate) fn before_update(
    w: &mut World,
    h: PhysHandle,
) -> Option<(Rc<RefCell<MotionDriver>>, Ticket)> {
    let driver = driver_of(w, h)?;
    let run_rate = inq_run_rate(w, h);
    let ticket = {
        let mut d = lock(&driver);
        refresh_env(w, h, &mut d, run_rate);
        resolve_target(w, h, &mut d);
        Ticket {
            driver_moving: d.movement.is_moving_to(),
            starts_move_to: false,
        }
    };
    Some((driver, ticket))
}

/// After a physics update: apply what the step asked for and route its notices.
pub(crate) fn after_update(
    w: &mut World,
    h: PhysHandle,
    before: Option<(Rc<RefCell<MotionDriver>>, Ticket)>,
) {
    if let Some((driver, ticket)) = before {
        settle(w, h, &driver, ticket);
    }
}

/// Runs one movement call on the body's driver with fresh facts, drains the completed motions
/// (every retail movement call ends so) and settles. `None` without a driver.
fn perform<R>(
    w: &mut World,
    h: PhysHandle,
    starts_move_to: bool,
    f: impl FnOnce(&mut MovementManager, &mut MotionCtx<'_>) -> R,
) -> Option<R> {
    let driver = driver_of(w, h)?;
    let run_rate = inq_run_rate(w, h);
    let (r, ticket) = {
        let mut d = lock(&driver);
        refresh_env(w, h, &mut d, run_rate);
        let ticket = Ticket {
            driver_moving: d.movement.is_moving_to(),
            starts_move_to,
        };
        let r = d.with_movement(|mm, ctx| {
            let r = f(mm, ctx);
            mm.drain_completed_motions(ctx);
            r
        });
        (r, ticket)
    };
    settle(w, h, &driver, ticket);
    Some(r)
}

/// The body enters the world: its link animations are dropped (ACE: `PartArray.HandleEnterWorld`
/// at the end of `enter_world`; `MovementManager.HandleEnterWorld` does nothing).
pub(crate) fn handle_enter_world(w: &mut World, h: PhysHandle) {
    perform(w, h, false, |mm, ctx| mm.remove_link_animations(ctx));
}

fn weenie_error(err: u32) -> WeenieError {
    WeenieError(i32::try_from(err).unwrap_or(i32::MAX))
}

// ---------------------------------------------------------------------------------------------
// ACE's MovementParameters constructor
// ---------------------------------------------------------------------------------------------

/// `new MovementParameters()` as ACE builds it for gameplay: the client's flags plus `CanCharge`,
/// distance 0.6, fail distance `float.MaxValue`, speed 1, walk/run threshold 1.0, hold key Invalid.
// ACE: MovementParameters.MovementParameters
// ACE-BUG: the client's constructor leaves CanCharge clear (0x1EE0F) and uses a walk/run threshold
// of 15.0; ACE sets CanCharge and 1.0, so every MoveTo built from these parameters runs however
// close the target is.
#[must_use]
pub fn ace_movement_parameters() -> MovementParameters {
    MovementParameters {
        flags: DEFAULT_FLAGS | flags::CAN_CHARGE,
        walk_run_threshold: 1.0,
        ..MovementParameters::default()
    }
}

// ---------------------------------------------------------------------------------------------
// ACE's PhysicsObj motion calls
// ---------------------------------------------------------------------------------------------

/// `PhysicsObj.IsAnimating`: motions are pending in the interpreter.
// ACE: PhysicsObj.IsAnimating
#[must_use]
pub fn is_animating(w: &World, h: PhysHandle) -> bool {
    with_driver(w, h, |d| d.movement.motions_pending()).unwrap_or(false)
}

/// `motions_pending()`, which ACE answers with `IsAnimating`.
// ACE: PhysicsObj.motions_pending
#[must_use]
pub fn motions_pending(w: &World, h: PhysHandle) -> bool {
    is_animating(w, h)
}

/// Animating, off the cycle, moving, holding a sidestep or turn, or walking a MoveTo. A body with
/// no movement manager answers from its velocities alone (DIVERGE: ACE dereferences the null
/// manager; no ACE caller asks for such a body).
// ACE: PhysicsObj.IsMovingOrAnimating
#[must_use]
pub fn is_moving_or_animating(w: &World, h: PhysHandle) -> bool {
    let Some(o) = w.physics.get(h) else {
        return false;
    };
    let moving = o.cached_velocity != Vec3::ZERO || o.velocity_vector != Vec3::ZERO;
    with_driver(w, h, |d| {
        let is_first_cyclic =
            d.sequence.curr().is_none() || d.sequence.curr() == d.sequence.first_cyclic();
        let s = &d.movement.interp.interpreted_state;
        // InterpretedMotionState.HasCommands
        let has_commands =
            s.sidestep_command != MotionCommand::NONE || s.turn_command != MotionCommand::NONE;
        d.movement.motions_pending()
            || !is_first_cyclic
            || moving
            || has_commands
            || d.movement.moveto.initialized
    })
    .unwrap_or(moving)
}

// ACE: PhysicsObj.IsMovingTo
#[must_use]
pub fn is_moving_to(w: &World, h: PhysHandle) -> bool {
    with_driver(w, h, |d| d.movement.is_moving_to()).unwrap_or(false)
}

/// The motion interpreter, made (with its manager) if the body has none: `f` reads or edits it,
/// as ACE's callers edit `RawState` before applying it.
// ACE: PhysicsObj.get_minterp
pub fn get_minterp<R>(
    w: &mut World,
    h: PhysHandle,
    f: impl FnOnce(&mut MotionInterp) -> R,
) -> Option<R> {
    ensure_movement_manager(w, h, LazyWake::Minterp);
    let driver = driver_of(w, h)?;
    let r = f(&mut lock(&driver).movement.interp);
    Some(r)
}

/// A copy of the interpreted state (`get_minterp().InterpretedState`).
#[must_use]
pub fn interpreted_state(w: &World, h: PhysHandle) -> Option<InterpretedMotionState> {
    with_driver(w, h, |d| d.movement.interp.interpreted_state.clone())
}

/// A copy of the raw state (`get_minterp().RawState`).
#[must_use]
pub fn raw_state(w: &World, h: PhysHandle) -> Option<RawMotionState> {
    with_driver(w, h, |d| d.movement.interp.raw_state.clone())
}

/// `MovementManager.PerformMovement`'s head: the body is set active.
fn perform_movement_prefix(w: &mut World, h: PhysHandle) {
    phys_ext::set_active(w, h, true);
}

/// A raw motion command (`MovementType.RawCommand`); `NoAnimationTable` without a manager.
// ACE: PhysicsObj.DoMotion
pub fn do_motion(
    w: &mut World,
    h: PhysHandle,
    motion: u32,
    movement_params: &MovementParameters,
) -> WeenieError {
    if let Some(o) = w.physics.get_mut(h) {
        o.last_move_was_autonomous = true;
    }
    if !has_movement_manager(w, h) {
        return WeenieError::NoAnimationTable;
    }
    perform_movement_prefix(w, h);
    let p = *movement_params;
    perform(w, h, false, |mm, ctx| {
        mm.do_motion(MotionCommand(motion), &p, ctx)
    })
    .map_or(WeenieError::NoAnimationTable, weenie_error)
}

/// Stops a raw motion command (`MovementType.StopRawCommand`).
// ACE: PhysicsObj.StopMotion
pub fn stop_motion(
    w: &mut World,
    h: PhysHandle,
    motion: u32,
    movement_params: &MovementParameters,
    send_event: bool,
) -> WeenieError {
    let _ = send_event;
    if let Some(o) = w.physics.get_mut(h) {
        o.last_move_was_autonomous = true;
    }
    if !has_movement_manager(w, h) {
        return WeenieError::NoAnimationTable;
    }
    perform_movement_prefix(w, h);
    let p = *movement_params;
    perform(w, h, false, |mm, ctx| {
        mm.stop_motion(MotionCommand(motion), &p, ctx)
    })
    .map_or(WeenieError::NoAnimationTable, weenie_error)
}

/// An interpreted command straight to the motion table (`PartArray.DoInterpretedMotion`): no
/// interpreter state, no pending motion. `GeneralMovementFailure` without a part array.
// ACE: PhysicsObj.DoInterpretedMotion
pub fn do_interpreted_motion(
    w: &mut World,
    h: PhysHandle,
    motion: u32,
    movement_params: &MovementParameters,
) -> WeenieError {
    table_movement(
        w,
        h,
        MovementType::InterpretedCommand,
        motion,
        movement_params,
    )
}

/// Stops an interpreted command at the motion table (`PartArray.StopInterpretedMotion`).
// ACE: PhysicsObj.StopInterpretedMotion
pub fn stop_interpreted_motion(
    w: &mut World,
    h: PhysHandle,
    motion: u32,
    movement_params: &MovementParameters,
) -> WeenieError {
    table_movement(
        w,
        h,
        MovementType::StopInterpretedCommand,
        motion,
        movement_params,
    )
}

/// `PartArray.{Do,Stop}InterpretedMotion` -> `MotionTableManager.PerformMovement`.
fn table_movement(
    w: &mut World,
    h: PhysHandle,
    kind: MovementType,
    motion: u32,
    p: &MovementParameters,
) -> WeenieError {
    if !phys_ext::ext(w, h).is_some_and(|e| e.has_part_array) {
        return WeenieError::GeneralMovementFailure;
    }
    let speed = p.speed;
    let r = perform(w, h, false, |_, ctx| {
        ctx.mgr.perform_movement(
            &MovementStruct {
                kind,
                motion: MotionCommand(motion),
                speed,
            },
            ctx.seq,
            ctx.assets,
        )
    });
    match r {
        Some(Ok(())) => WeenieError::None,
        Some(Err(e)) => weenie_error(e),
        // a part array without a motion table
        None => WeenieError::NoAnimationTable,
    }
}

/// Stops every motion and cancels any MoveTo (`MovementType.StopCompletely`).
// ACE: PhysicsObj.StopCompletely
pub fn stop_completely(w: &mut World, h: PhysHandle, send_event: bool) {
    let _ = send_event;
    if !has_movement_manager(w, h) {
        return;
    }
    perform_movement_prefix(w, h);
    perform(w, h, false, MovementManager::stop_completely);
}

/// The motion interpreter's own stop (`MovementManager.MotionInterpreter.StopCompletely()`): the
/// MoveTo cancelled, the raw and interpreted states back to Ready with no sidestep or turn, and
/// the sequence stopped, as [`stop_completely`] does, but without the movement manager's head
/// (the body is not set active by it). Nothing for a body without a movement manager.
// ACE: MotionInterp.StopCompletely
pub fn minterp_stop_completely(w: &mut World, h: PhysHandle) {
    if !has_movement_manager(w, h) {
        return;
    }
    perform(w, h, false, MovementManager::stop_completely);
}

/// `MovementManager.PerformMovement` for the four MoveTo types.
fn perform_move_to(
    w: &mut World,
    h: PhysHandle,
    req: MoveToRequest,
    movement_params: &MovementParameters,
) {
    perform_movement_prefix(w, h);
    let p = *movement_params;
    perform(w, h, true, |mm, ctx| mm.perform_movement(&req, &p, ctx));
}

/// `(obj.PartArray?.GetRadius(), GetHeight())`, 0 without a part array.
fn part_array_size(w: &World, obj: PhysHandle) -> (f32, f32) {
    let has_part_array = phys_ext::ext(w, obj).is_some_and(|e| e.has_part_array);
    match w.physics.get(obj) {
        Some(o) if has_part_array => (o.radius(), o.height()),
        _ => (0.0, 0.0),
    }
}

/// The top-level object: the parent if `obj` has one.
fn top_level(w: &World, obj: PhysHandle) -> PhysHandle {
    w.physics.get(obj).and_then(|o| o.parent).unwrap_or(obj)
}

/// Walks to an object (its top-level object, its radius and height).
// ACE: PhysicsObj.MoveToObject
pub fn move_to_object(
    w: &mut World,
    h: PhysHandle,
    obj: PhysHandle,
    movement_params: &MovementParameters,
) {
    ensure_movement_manager(w, h, LazyWake::ClearActive);

    let (radius, height) = part_array_size(w, obj);
    let parent = top_level(w, obj);
    let top_level_id = phys_ext::id(w, parent).unwrap_or(0);

    move_to_object_internal(w, h, obj, top_level_id, radius, height, movement_params);
}

// ACE: PhysicsObj.MoveToObject_Internal
pub fn move_to_object_internal(
    w: &mut World,
    h: PhysHandle,
    obj: PhysHandle,
    top_level_id: u32,
    obj_radius: f32,
    obj_height: f32,
    movement_params: &MovementParameters,
) {
    ensure_movement_manager(w, h, LazyWake::ClearActive);
    let object_id = ObjectId(phys_ext::id(w, obj).unwrap_or(0));
    let req = MoveToRequest::MoveToObject {
        object_id,
        top_level_id: ObjectId(top_level_id),
        radius: obj_radius,
        height: obj_height,
    };
    perform_move_to(w, h, req, movement_params);
}

/// Walks to a position.
// ACE: PhysicsObj.MoveToPosition
pub fn move_to_position(
    w: &mut World,
    h: PhysHandle,
    pos: &PPosition,
    movement_params: &MovementParameters,
) {
    if !has_movement_manager(w, h) {
        // DIVERGE: ACE dereferences the null MovementManager (a NullReferenceException); a body
        // without one cannot walk, and the call does nothing.
        log::error!(
            "MoveToPosition({:08X}): no movement manager",
            phys_ext::id(w, h).unwrap_or(0)
        );
        return;
    }
    perform_move_to(
        w,
        h,
        MoveToRequest::MoveToPosition { pos: *pos },
        movement_params,
    );
}

/// Turns to `movement_params.desired_heading`.
// ACE: PhysicsObj.TurnToHeading
pub fn turn_to_heading(w: &mut World, h: PhysHandle, movement_params: &MovementParameters) {
    ensure_movement_manager(w, h, LazyWake::ClearActive);
    perform_move_to(w, h, MoveToRequest::TurnToHeading, movement_params);
}

/// Turns to face an object (its top-level object); `false` for no object.
// ACE: PhysicsObj.TurnToObject
pub fn turn_to_object(
    w: &mut World,
    h: PhysHandle,
    obj: Option<PhysHandle>,
    movement_params: &MovementParameters,
) -> bool {
    let Some(obj) = obj else { return false };

    let parent = top_level(w, obj);

    let object_id = phys_ext::id(w, obj).unwrap_or(0);
    let top_level_id = phys_ext::id(w, parent).unwrap_or(0);
    turn_to_object_internal(w, h, object_id, top_level_id, movement_params);

    true
}

// ACE: PhysicsObj.TurnToObject_Internal
pub fn turn_to_object_internal(
    w: &mut World,
    h: PhysHandle,
    object_id: u32,
    top_level_id: u32,
    movement_params: &MovementParameters,
) {
    ensure_movement_manager(w, h, LazyWake::ClearActive);
    let req = MoveToRequest::TurnToObject {
        object_id: ObjectId(object_id),
        top_level_id: ObjectId(top_level_id),
    };
    perform_move_to(w, h, req, movement_params);
}

/// Cancels the MoveTo with `ActionCancelled`.
// ACE: PhysicsObj.cancel_moveto
pub fn cancel_moveto(w: &mut World, h: PhysHandle) {
    cancel_move_to(w, h, WeenieError::ActionCancelled);
}

/// `MovementManager.CancelMoveTo(error)` (ACE's gameplay also reaches it as
/// `MovementManager.MoveToManager.CancelMoveTo`).
// ACE: MovementManager.CancelMoveTo
pub fn cancel_move_to(w: &mut World, h: PhysHandle, error: WeenieError) {
    #[allow(clippy::cast_sign_loss)] // the same 32 bits
    let err = error.0 as u32;
    perform(w, h, false, |mm, ctx| mm.cancel_move_to(err, ctx));
}

/// Sticks to an object (its top-level object's radius) through the position manager.
// ACE: PhysicsObj.stick_to_object
pub fn stick_to_object(w: &mut World, h: PhysHandle, object_id: u32) {
    // MakePositionManager: sets a non-static body active; ObjMaint is always set on the server
    wake(w, h, true);
    if !has_movement_manager(w, h) {
        // the shared driver owns the position manager's sticky half
        let mtable_id = phys_ext::ext(w, h).map_or(0, |e| e.motion_table_id);
        if !attach(w, h, mtable_id) {
            return;
        }
    }

    let Some(mut object_a) = server_object_manager::get_object_a(w, object_id) else {
        return;
    };
    // ACE-BUG: ACE replaces a parented object with *this* object's Parent, not the object's
    // (`objectA = Parent`): a held target makes the body stick to its own parent, and a body with
    // no parent dereferences null (a NullReferenceException; nothing happens here).
    if w.physics.get(object_a).is_some_and(|o| o.parent.is_some()) {
        match w.physics.get(h).and_then(|o| o.parent) {
            Some(p) => object_a = p,
            None => return,
        }
    }

    let has_part_array = phys_ext::ext(w, object_a).is_some_and(|e| e.has_part_array);
    if has_part_array {
        let id = ObjectId(phys_ext::id(w, object_a).unwrap_or(0));
        let (radius, _height) = part_array_size(w, object_a);
        let now = physics_timer_current_time(w);
        perform(w, h, false, |mm, ctx| {
            mm.stick_to_object(id, radius, now, ctx)
        });
    } else {
        let before = before_update(w, h);
        if let Some(o) = w.physics.get_mut(h) {
            o.clear_transient_states();
        }
        after_update(w, h, before);
    }
}

// ACE: PhysicsObj.unstick_from_object
pub fn unstick_from_object(w: &mut World, h: PhysHandle) {
    perform(w, h, false, MovementManager::unstick_from_object);
}

/// Sets or clears on-walkable; the movement manager hears the edge (`HitGround`/`LeaveGround`).
// ACE: PhysicsObj.set_on_walkable
pub fn set_on_walkable(w: &mut World, h: PhysHandle, is_on_walkable: bool) {
    let before = before_update(w, h);
    if let Some(o) = w.physics.get_mut(h) {
        o.set_on_walkable(is_on_walkable);
    }
    after_update(w, h, before);
}

/// Zero acceleration and omega on the ground, gravity in the air.
// ACE: PhysicsObj.calc_acceleration
pub fn calc_acceleration(w: &mut World, h: PhysHandle) {
    if let Some(o) = w.physics.get_mut(h) {
        o.calc_acceleration();
    }
}

// ---------------------------------------------------------------------------------------------
// MoveToManager and MotionInterp fields gameplay reads
// ---------------------------------------------------------------------------------------------

/// `MovementManager.MoveToManager.FailProgressCount`.
#[must_use]
pub fn move_to_fail_progress_count(w: &World, h: PhysHandle) -> u32 {
    with_driver(w, h, |d| d.movement.moveto.fail_progress_count).unwrap_or(0)
}

/// `MovementManager.MoveToManager.FailProgressCount = count`.
pub fn set_move_to_fail_progress_count(w: &World, h: PhysHandle, count: u32) {
    if let Some(d) = driver_of(w, h) {
        lock(&d).movement.moveto.fail_progress_count = count;
    }
}

/// `MovementManager.MoveToManager.AlwaysTurn = always_turn` (the shared manager's host option,
/// A14: a turn-to-heading begun while the body animates starts at once).
pub fn set_move_to_always_turn(w: &World, h: PhysHandle, always_turn: bool) {
    if let Some(d) = driver_of(w, h) {
        lock(&d).movement.moveto.always_turn = always_turn;
    }
}

/// `MovementManager.MoveToManager.AlwaysTurn`.
#[must_use]
pub fn move_to_always_turn(w: &World, h: PhysHandle) -> bool {
    with_driver(w, h, |d| d.movement.moveto.always_turn).unwrap_or(false)
}

/// `MovementManager.MoveToManager.PendingActions.Count`.
#[must_use]
pub fn move_to_pending_actions(w: &World, h: PhysHandle) -> usize {
    with_driver(w, h, |d| d.movement.moveto.pending_actions.len()).unwrap_or(0)
}

/// `PartArray.Sequence.CurrAnim?.Value.Anim.ID`: the animation the body's sequence is playing,
/// `None` with no sequence node (or no driver).
#[must_use]
pub fn curr_anim_id(w: &World, h: PhysHandle) -> Option<u32> {
    with_driver(w, h, |d| {
        d.sequence.curr().map(|i| d.sequence.nodes()[i].anim_id.0)
    })
    .flatten()
}

/// `MovementManager.MoveToManager.Initialized`.
#[must_use]
pub fn move_to_initialized(w: &World, h: PhysHandle) -> bool {
    with_driver(w, h, |d| d.movement.moveto.initialized).unwrap_or(false)
}

/// ACE's teleport hack: `MotionInterpreter.PendingMotions.Clear(); IsAnimating = false;`. The
/// motion table's own queue is left as it is, as ACE leaves it.
pub fn clear_pending_motions(w: &World, h: PhysHandle) {
    if let Some(d) = driver_of(w, h) {
        lock(&d).movement.interp.pending_motions.clear();
    }
}

/// A test's (or a diagnostic's) run rate in place of `WeenieObject.InqRunRate`, until that
/// inquiry is ported; `None` asks the `WeenieObject` again.
pub fn set_run_rate_stand_in(w: &mut World, h: PhysHandle, rate: Option<f32>) {
    if let Some(m) = phys_ext::ext_mut(w, h).and_then(|e| e.motion.as_mut()) {
        m.run_rate_stand_in = rate;
    }
}

/// The status of the last `OnMoveComplete` routed for the body.
#[must_use]
pub fn last_move_complete(w: &World, h: PhysHandle) -> Option<WeenieError> {
    phys_ext::ext(w, h)
        .and_then(|e| e.motion.as_ref())
        .and_then(|m| m.last_move_complete)
}

// ---------------------------------------------------------------------------------------------
// Raw motion states (the physics halves of ACE's WorldObject and Player movement code)
// ---------------------------------------------------------------------------------------------

/// Copies a client's raw motion state into the interpreter's, with ACE's defaults for zeroes:
/// style NonCombat, forward Ready, each speed 1.0. The actions are not copied.
// ACE: RawMotionState.SetState
pub fn raw_motion_state_set_state(this: &mut RawMotionState, state: &RawMotionState) {
    this.current_holdkey = state.current_holdkey;
    this.current_style = state.current_style;
    if this.current_style == MotionCommand::NONE {
        this.current_style = MotionCommand::NON_COMBAT;
    }
    this.forward_command = state.forward_command;
    if this.forward_command == MotionCommand::NONE {
        this.forward_command = MotionCommand::READY;
    }
    this.forward_holdkey = state.forward_holdkey;
    this.forward_speed = state.forward_speed; // todo: verifications
    #[allow(clippy::float_cmp)] // C#'s == 0
    if this.forward_speed == 0.0 {
        this.forward_speed = 1.0;
    }
    this.sidestep_command = state.sidestep_command;
    this.sidestep_holdkey = state.sidestep_holdkey;
    this.sidestep_speed = state.sidestep_speed;
    #[allow(clippy::float_cmp)]
    if this.sidestep_speed == 0.0 {
        this.sidestep_speed = 1.0;
    }
    this.turn_command = state.turn_command;
    this.turn_holdkey = state.turn_holdkey;
    this.turn_speed = state.turn_speed;
    #[allow(clippy::float_cmp)]
    if this.turn_speed == 0.0 {
        this.turn_speed = 1.0;
    }
}

/// `if (!PhysicsObj.IsMovingOrAnimating) PhysicsObj.UpdateTime = PhysicsTimer.CurrentTime;`:
/// a body at rest restarts its clock before new motion, so the next update does not replay the
/// idle time.
pub fn restart_clock_if_idle(w: &mut World, h: PhysHandle) {
    if !is_moving_or_animating(w, h) {
        let now = physics_timer_current_time(w);
        if let Some(o) = w.physics.get_mut(h) {
            o.update_time = now;
        }
    }
}

/// `minterp.apply_raw_movement(cancelMoveTo, allowJump)` on the body's interpreter.
pub fn apply_raw_movement(w: &mut World, h: PhysHandle, cancel_move_to: bool, allow_jump: bool) {
    // ACE's `DisableJumpDuringLink = !allowJump` is the shared crate's long-jump hint
    perform(w, h, false, |mm, ctx| {
        mm.apply_raw_movement(cancel_move_to, !allow_jump, ctx)
    });
}

/// `motion_allows_jump(InterpretedState.ForwardCommand) == WeenieError.None`.
fn interpreted_forward_allows_jump(w: &World, h: PhysHandle) -> bool {
    with_driver(w, h, |d| {
        motion_allows_jump(d.movement.interp.interpreted_state.forward_command) == 0
    })
    .unwrap_or(true)
}

/// The physics of `Player.OnMoveToState_ServerMethod` (the player's port of the member calls
/// this): the client's raw state becomes the interpreter's (a standing long jump holds forward
/// and sidestep), then `apply_raw_movement(true, allowJump)`.
pub fn apply_raw_motion_state(
    w: &mut World,
    h: PhysHandle,
    raw: &RawMotionState,
    standing_long_jump: bool,
) {
    let set = get_minterp(w, h, |minterp| {
        raw_motion_state_set_state(&mut minterp.raw_state, raw);

        if standing_long_jump {
            minterp.raw_state.forward_command = MotionCommand::READY;
            minterp.raw_state.sidestep_command = MotionCommand::NONE;
        }
    });
    if set.is_none() {
        return;
    }

    let allow_jump = interpreted_forward_allows_jump(w, h);

    //PhysicsObj.cancel_moveto();

    apply_raw_movement(w, h, true, allow_jump);
}

/// The physics of `Player.OnMoveToState_ClientMethod`: each of the three axes pressed, changed
/// or released since `prev` becomes a `DoMotion`/`StopMotion` with the client's hold key.
pub fn apply_raw_motion_state_client_method(
    w: &mut World,
    h: PhysHandle,
    raw: &RawMotionState,
    prev: &RawMotionState,
) {
    let mvp = MovementParameters {
        hold_key_to_apply: raw.current_holdkey,
        ..ace_movement_parameters()
    };

    restart_clock_if_idle(w, h);

    let invalid = MotionCommand::NONE;

    // ForwardCommand
    if raw.forward_command != invalid {
        // press new key
        if prev.forward_command == invalid {
            do_motion(w, h, MotionCommand::READY.0, &mvp);
            do_motion(w, h, raw.forward_command.0, &mvp);
        }
        // press alternate key
        else if prev.forward_command != raw.forward_command {
            do_motion(w, h, raw.forward_command.0, &mvp);
        }
    } else if prev.forward_command != invalid {
        // release key
        stop_motion(w, h, prev.forward_command.0, &mvp, true);
    }

    // StrafeCommand
    if raw.sidestep_command != invalid {
        if prev.sidestep_command == invalid || prev.sidestep_command != raw.sidestep_command {
            do_motion(w, h, raw.sidestep_command.0, &mvp);
        }
    } else if prev.sidestep_command != invalid {
        stop_motion(w, h, prev.sidestep_command.0, &mvp, true);
    }

    // TurnCommand
    if raw.turn_command != invalid {
        if prev.turn_command == invalid || prev.turn_command != raw.turn_command {
            do_motion(w, h, raw.turn_command.0, &mvp);
        }
    } else if prev.turn_command != invalid {
        stop_motion(w, h, prev.turn_command.0, &mvp, true);
    }
}

/// The physics half of `WorldObject.ApplyPhysicsMotion` (for 2.2b's `EnqueueBroadcastMotion`):
/// the motion's stance, forward command and speed become the raw state, then
/// `apply_raw_movement(true, allowJump)` with `allowJump` read before the change.
pub fn apply_physics_motion(
    w: &mut World,
    h: PhysHandle,
    stance: u32,
    forward_command: u32,
    forward_speed: f32,
) {
    let allow_jump = {
        if get_minterp(w, h, |_| ()).is_none() {
            return;
        }
        interpreted_forward_allows_jump(w, h)
    };
    get_minterp(w, h, |minterp| {
        minterp.raw_state.current_style = MotionCommand(stance);
        minterp.raw_state.forward_command = MotionCommand(forward_command);
        minterp.raw_state.forward_speed = forward_speed;
    });

    restart_clock_if_idle(w, h);

    apply_raw_movement(w, h, true, allow_jump);
}

/// The physics half of `WorldObject.ExecuteMotion`: a fresh raw state whose style is the motion
/// (forward 0, hold key Run), applied with `apply_raw_movement(true, true)`. Nothing for Ready.
pub fn execute_motion_physics(w: &mut World, h: PhysHandle, motion_command: u32) {
    if motion_command == MotionCommand::READY.0 {
        return;
    }
    // var motionInterp = PhysicsObj.get_minterp();
    if get_minterp(w, h, |_| ()).is_none() {
        return;
    }

    let raw_state = RawMotionState {
        forward_command: MotionCommand::NONE, // always 0? must be this for monster sleep animations (skeletons, golems)
        // else the monster will immediately wake back up..
        current_holdkey: HoldKey::Run,
        current_style: MotionCommand(motion_command),
        ..RawMotionState::default()
    };

    restart_clock_if_idle(w, h);

    get_minterp(w, h, |minterp| minterp.raw_state = raw_state);
    apply_raw_movement(w, h, true, true);
}
