//! The per-object physics / animation step — the physics update loop over
//! the object-maintenance table, and the object state it runs on.
//!
//! It is kept apart from `dereth_scene::world_scene`'s `impl WorldScene` because it is pure
//! simulation: it reads and writes an object's driver, its collision body, its position and its
//! move-to/stick plans, and it never touches a mesh, a texture slot or a device. The scene keeps
//! the render half of a scene object and calls these with the simulation half.
//!
//! **What "the simulation half" is.** `ObjectSim` holds the twelve fields the step reads;
//! `dereth_scene::world_scene::SceneObject` keeps the ten that draw it (the shared part meshes, the
//! per-frame degrade levels, the draw frames, the viewer distances) and reaches its own through
//! `AsObjectSim`. Splitting the struct rather than the map is what lets the step move with no
//! second table to keep in step, and it is why every function below is generic over the scene's
//! own object type instead of naming it.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use dereth_animation::{MotionCommand, MotionDriver};
use dereth_physics::obj::PhysicsState;
use dereth_primitives::{Frame, LocalTime, ObjectId, Position, ServerTime, Vec3};

use crate::character::Character;

/// The simulation half of a scene object: what object updating and the
/// movement layer read and write, with no drawing state in it; the drawing half stays on
/// `dereth_scene::world_scene::SceneObject`.
#[derive(Debug)]
pub struct ObjectSim {
    /// The part array plus the movement manager: the whole animation runtime.
    pub driver: Rc<RefCell<MotionDriver>>,
    /// The collision body shares this driver and owns its only simulation advancement.
    pub physics_handle: Option<dereth_physics::PhysHandle>,
    /// Motion can arrive before App creates the corresponding collision body that frame.
    pub pending_physics_effects: Vec<dereth_animation::motion::MotionEffect>,
    /// `MotionDriver::target_updates` as of the last `account_object_targets`.
    pub target_updates_seen: u32,
    /// Where the server last said it is. Kept as a `Position` rather than only as a render
    /// frame because the render space's origin moves with the viewer's block, so
    /// the frame is re-derived every step instead of being stored once.
    ///
    /// `None` for a **held** object: attachment calls
    /// `leave_world` on it, so it has no position of its own and its frame is derived from
    /// [`Self::parent`] once a frame instead.
    pub position: Option<Position>,
    /// The physics parent and the `ParentLocation` it is attached at.
    ///
    /// An object with one is drawn at the holder's part frame combined with the holding
    /// location's frame rather than at a server position; see [`crate::models::child_frame`]. Both
    /// object creation and `0xF749` set it, and a `0xF748` clears
    /// it.
    pub parent: Option<(ObjectId, u32)>,
    /// The physics state word. The renderer keeps its own copy because two of the word's
    /// bits decide whether an object is drawn and neither of them is decided by physics.
    ///
    /// Starts at [`PhysicsState::DEFAULT`], the physics-body constructor's value, and is
    /// then driven through `WorldScene::apply_object_state` exactly as description application
    /// and state-message handling drive the client's. It is **not** the
    /// wire word: `set_hidden` writes `NODRAW_PS` into a *child's* state, and
    /// `unset_parent` takes it back out, neither of which the server ever said.
    pub state: PhysicsState,
    /// the object's own simulated clock, not the wall clock.
    pub update_time: f64,
    /// Whether the object has a motion table, i.e. whether it can animate at all.
    pub animated: bool,
    /// The last server position actually stamped into [`Self::position`].
    ///
    /// The edge runs when a `0xF748` *arrives*, not on every frame's re-read of the object
    /// stream's **cached** copy in `WorldScene::sync_objects`: offset adjustment pulls a stuck
    /// object toward its target, and without the edge the pull would last exactly one frame
    /// and then be stamped back over by the server's last word.
    pub server_position: Option<Position>,
    /// Radius and height for this object, cached from
    /// the physics body at sync time because `advance_objects` does not have the
    /// [`crate::objects::ObjectStream`] the bodies live in.
    ///
    /// `0.0` for an object with no body, matching both stick-to-object and
    /// move-to-object handling when the part array is missing.
    pub radius: f32,
    pub height: f32,
}

/// A scene's own object type, reduced to the half this module steps.
///
/// `dereth_scene::world_scene::SceneObject` is the only implementor; the trait exists so that the step
/// can iterate the scene's real table without the render fields following it down here.
pub trait AsObjectSim {
    fn sim(&self) -> &ObjectSim;
    fn sim_mut(&mut self) -> &mut ObjectSim;
}

impl AsObjectSim for ObjectSim {
    fn sim(&self) -> &ObjectSim {
        self
    }
    fn sim_mut(&mut self) -> &mut ObjectSim {
        self
    }
}

/// What the step counts, handed back to the scene's own `SceneStats`.
///
/// The counters live on `dereth_scene::world_scene::SceneStats`, which the whole test tree reads by its
/// flat field names; the step accumulates into one of these and the scene folds it in, so the
/// public counters stay where the tests read them.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ObjectStepStats {
    /// `SceneStats::remote_move_tos_failed`.
    pub remote_move_tos_failed: u64,
    /// `SceneStats::remote_last_move_to_error`.
    pub remote_last_move_to_error: u32,
    /// `SceneStats::remote_target_updates`.
    pub remote_target_updates: u64,
    /// `SceneStats::remote_sticks_pulled`.
    pub remote_sticks_pulled: u64,
    /// `SceneStats::remote_sticks_applied` -- `crate::movement::apply_movement`'s.
    pub remote_sticks_applied: u64,
    /// `SceneStats::remote_sticks_unresolved` -- `crate::movement::apply_movement`'s.
    pub remote_sticks_unresolved: u64,
    /// `SceneStats::remote_move_tos_performed` -- `crate::movement::apply_movement`'s.
    pub remote_move_tos_performed: u64,
}

/// `Character::refresh_env`, for an object the **server** positions.
///
/// `MotionEnv` is what `MoveToManager` and `StickyManager` ask the physics object for. Left
/// unwritten it stands at `MotionEnv::default()`, the identity frame in cell **0** with
/// `object_id == 0` and a 0.5 m radius, and a move-to run against that measures every
/// distance from the origin of the world.
///
/// Attached objects use their live collision body's contact and velocity. The explicit
/// bodyless viewer fallback keeps a ground/zero-velocity environment; it does
/// not represent a physics simulation. Physics refreshes its own facts each substep too.
pub fn refresh_object_env<S: AsObjectSim>(
    objects: &mut BTreeMap<ObjectId, S>,
    character: &Option<Character>,
    id: ObjectId,
) {
    let Some(o) = objects.get_mut(&id).map(AsObjectSim::sim_mut) else {
        return;
    };
    let mut driver = o.driver.borrow_mut();
    if let Some(p) = o.position {
        driver.env.position = p;
    }
    driver.env.radius = o.radius;
    driver.env.height = o.height;
    driver.env.object_id = id;
    driver.env.in_cell = o.position.is_some();
    driver.env.is_creature = o.animated;
    driver.env.gravity_affected = o.state.has_gravity();
    let body = character
        .as_ref()
        .and_then(|c| c.world.by_object_id(id).and_then(|h| c.world.get(h)));
    if let Some(body) = body {
        driver.env.contact = body.transient_state.in_contact();
        driver.env.on_ground =
            body.transient_state.in_contact() && body.transient_state.on_walkable();
        driver.env.velocity = body.velocity_vector;
        // Reads the **cached** velocity (`cached_velocity`), and NOT the
        // integrated velocity.
        // The move-to-position handler's third
        // block is the one caller in the movement layer, and it is dead without this: a
        // creature walking on the ground carries an integrated velocity of zero for its whole
        // approach (`calc_acceleration` zeroes acceleration in walkable
        // contact) while `cached_velocity` carries the achieved metres per second that
        // the per-object physics update wrote.
        //
        // The local body's own arm is `Character::refresh_env`'s; this is the remote path only.
        driver.env.cached_velocity = body.cached_velocity;
        driver.env.radius = body.radius();
        driver.env.height = body.height();
    } else {
        // Explicit compatibility path for bodyless viewers. It does not simulate physics.
        driver.env.contact = true;
        driver.env.on_ground = true;
        driver.env.velocity = dereth_primitives::Vec3::ZERO;
    }
}

/// Attach each dynamic remote animation to its existing collision body before the one
/// PhysicsWorld sweep inside Character::update. Static/held/bodyless animation retains
/// the viewer ladder; no driver is cloned or advanced by two owners.
pub fn prepare_object_physics<S: AsObjectSim>(
    objects: &mut BTreeMap<ObjectId, S>,
    character: &mut Option<Character>,
    stats: &mut ObjectStepStats,
    now: LocalTime,
) {
    let mut queued = Vec::new();
    for (id, o) in objects.iter_mut().map(|(i, o)| (i, o.sim_mut())) {
        let handle = character.as_ref().and_then(|c| {
            c.world.by_object_id(*id).filter(|h| {
                // **`o.animated` alone is not the test.** The physics tick walks every
                // object in the object table and calls `update_object`
                // on each; there is no motion-table test anywhere in that loop.
                // `update_object`'s own test is `transient_state & ACTIVE_TS`, which
                // `set_velocity` and `set_active` set and which the physics
                // integration clears again the moment the object stops.
                //
                // A spell projectile has a setup and a velocity and **no motion table**, so
                // `animated` is false for it. Gated on `animated` alone it would get no
                // `physics_handle`, `finish_object_physics` would never publish its integrated
                // position, and the bolt would sit at the caster's feet however fast physics
                // moved its body.
                //
                // `ACTIVE_TS` is added rather than `animated` replaced: this build's
                // bodyless viewer ladder (see `advance_objects`) still owns every
                // object physics has *not* woken, and taking that from it would put every
                // barrel and lamp on the integrating path in one step. The two conditions
                // together are exactly the set `update_object` does work for.
                (o.animated
                    || c.world
                        .get(*h)
                        .is_some_and(|b| b.transient_state.is_active()))
                    && o.parent.is_none()
                    && c.world.get(*h).is_some_and(|b| !b.state.is_static())
            })
        });
        if o.physics_handle != handle {
            // A live `0xF74B` can make an attached object STATIC without clearing ACTIVE:
            // `set_state`; `set_active`. Release our old adapter before
            // the fallback owns this same driver, or both ladders advance its sequence.
            // Do not change the body's retail activation or physical velocity here.
            if let Some(body) = o
                .physics_handle
                .and_then(|h| character.as_mut().and_then(|c| c.world.get_mut(h)))
            {
                body.motion = None;
            }
        }
        if let Some(h) = handle {
            let c = character.as_mut().expect("body resolved above");
            let body = c.world.get_mut(h).expect("body resolved above");
            if o.physics_handle != Some(h) || body.motion.is_none() {
                body.set_motion(Box::new(crate::character::SharedMotion::new(Rc::clone(
                    &o.driver,
                ))));
                body.calc_acceleration();
            }
        }
        o.physics_handle = handle;
        let mut driver = o.driver.borrow_mut();
        driver.cur_time = ServerTime(now.0);
        driver.env.is_creature = o.animated;
        let mut effects = std::mem::take(&mut o.pending_physics_effects);
        effects.extend(driver.take_effects());
        if !effects.is_empty() {
            queued.push((*id, effects));
        }
    }
    for (id, effects) in queued {
        apply_object_effects(objects, character, stats, id, effects);
    }
}

/// Publish the achieved physics position, not the desired animation endpoint. Rendering,
/// targeting and children must all observe the same collision/contact result.
pub fn finish_object_physics<S: AsObjectSim>(
    objects: &mut BTreeMap<ObjectId, S>,
    character: &mut Option<Character>,
    stats: &mut ObjectStepStats,
) {
    let mut queued = Vec::new();
    for (id, o) in objects.iter_mut().map(|(i, o)| (i, o.sim_mut())) {
        let body = o
            .physics_handle
            .and_then(|h| character.as_ref().and_then(|c| c.world.get(h)));
        let Some(body) = body else { continue };
        o.position = body.cell.map(|_| body.position);
        o.update_time = body.update_time;
        o.radius = body.radius();
        o.height = body.height();
        let mut driver = o.driver.borrow_mut();
        driver.update_scripts();
        let effects = driver.take_effects();
        if !effects.is_empty() {
            queued.push((*id, effects));
        }
    }
    for (id, effects) in queued {
        apply_object_effects(objects, character, stats, id, effects);
    }
}

/// Run one call against a server object's `MovementManager`, the way
/// [`crate::character::Character::perform_move_to`] runs one against the player's.
///
/// The movement layer talks to the physics object through a
/// `dereth_animation::motion::MotionCtx` the caller assembles and a list of
/// [`dereth_animation::motion::MotionEffect`]s it hands back, so the three steps —
/// refresh the environment, make the call, apply what it asked for — have to happen
/// together or not at all. `MotionDriver::with_movement` publishes subscriptions inline
/// and keeps events on the driver's queue so `process_hooks` is the single drain.
pub fn drive_object_motion<S: AsObjectSim, R>(
    objects: &mut BTreeMap<ObjectId, S>,
    character: &mut Option<Character>,
    stats: &mut ObjectStepStats,
    id: ObjectId,
    f: impl FnOnce(
        &mut dereth_animation::motion::MovementManager,
        &mut dereth_animation::motion::interp::MotionCtx<'_>,
    ) -> R,
) -> Option<R> {
    refresh_object_env(objects, character, id);
    let (r, effects) = {
        let o = objects.get_mut(&id)?.sim_mut();
        let mut driver = o.driver.borrow_mut();
        let (r, effects) = driver.with_movement_scoped_effects(|movement, ctx| {
            let r = f(movement, ctx);
            movement.drain_completed_motions(ctx);
            r
        });
        // Every movement step ends with a completed-motion check -- **including its successful
        // motion tail**, which is the half that pops
        // the interpreter's pending-motion queue.
        //
        // Keeping the drained events is not routing them: each one must reach the motion
        // interpreter's completion handler. This site is live: every server object that
        // receives a `MoveTo`/`TurnTo` arm of `0xF74C` and every 0.5 s update-target tick
        // comes through here. If the manager's ledger drained on every call and the
        // interpreter's did not, the two would go one node further out of step each time and
        // `motions_pending()` -- which reads the interpreter's -- would latch true for the
        // object's whole life.
        //
        // **What that costs, measured, and it is not small.**
        // Beginning a turn-to-heading returns early to *"wait for the
        // current animation"* while `motions_pending()`, so **no server creature could
        // take a single step**: over a 4,328-frame window of `long-solo-play`, **10,740 of
        // 10,740** moving samples had a non-empty ledger and **0** ever issued a command.
        // With the drain routed the same window reads **18 of 10,740** pending (max depth 1)
        // and **10,740 of 10,740** issuing `WalkForward`, `TurnLeft` or `TurnRight`.
        (r, effects)
    };
    apply_object_effects(objects, character, stats, id, effects);
    Some(r)
}

/// `Character::apply_effect_list`, for a server object.
///
/// Movement callbacks and subscriptions have already completed through the driver owner.
pub fn apply_object_effects<S: AsObjectSim>(
    objects: &mut BTreeMap<ObjectId, S>,
    character: &mut Option<Character>,
    stats: &mut ObjectStepStats,
    id: ObjectId,
    effects: Vec<dereth_animation::motion::MotionEffect>,
) {
    use dereth_animation::motion::MotionEffect;
    {
        let Some(o) = objects.get_mut(&id).map(AsObjectSim::sim_mut) else {
            return;
        };
        for e in effects {
            match e {
                // The exact heading snap at the end of a
                // turn. It is a **client-side** write to a server-positioned object's
                // frame, which survives until the next `0xF748` now that the position
                // write is the edge it is in the received-position handler.
                MotionEffect::SetHeading(h) => {
                    if let Some(p) = o.position.as_mut() {
                        dereth_animation::frame::set_heading(&mut p.frame, h);
                    }
                    if let Some(c) = character.as_mut() {
                        if let Some(body) =
                            c.world.by_object_id(id).and_then(|h| c.world.get_mut(h))
                        {
                            body.set_heading(h);
                        }
                    }
                }
                MotionEffect::MoveToFailed(err) => {
                    stats.remote_move_tos_failed += 1;
                    stats.remote_last_move_to_error = err;
                }
                // Physical requests go to the same body the PhysicsWorld sweep advances.
                // A request emitted while App is between its scene/body create passes is
                // retained until the body exists; true bodyless viewers discard it.
                MotionEffect::SetLocalVelocity(_) | MotionEffect::SetOnWalkable(_) => {
                    let body = character.as_mut().and_then(|c| {
                        let now = c.world.last_physics_time();
                        c.world
                            .by_object_id(id)
                            .and_then(|h| c.world.get_mut(h))
                            .map(|b| (b, now))
                    });
                    if let Some((body, now)) = body {
                        match e {
                            MotionEffect::SetLocalVelocity(v) => body.set_local_velocity(v, now),
                            MotionEffect::SetOnWalkable(on) => body.set_on_walkable(on),
                            _ => unreachable!(),
                        }
                    } else if character.is_some() && o.parent.is_none() && !o.state.is_static() {
                        o.pending_physics_effects.push(e);
                    }
                }
                MotionEffect::CancelMoveTo
                | MotionEffect::UnstickFromObject
                | MotionEffect::StickTo { .. }
                | MotionEffect::SetTarget { .. }
                | MotionEffect::ClearTarget
                | MotionEffect::SetTargetQuantum(_) => {
                    unreachable!("movement callback escaped its synchronous owner: {e:?}");
                }
            }
        }
    }
}

/// The host's half of `TargetManager`'s voyeur model, once per frame.
///
/// Retail keeps the watcher table on the **target** (`AddVoyeur`) and its
/// `HandleTargetting` reads the three facts off its own physics object
/// (`SendVoyeurUpdate`: `position`, `get_velocity()`, the status). This
/// build keeps the subscription on the watcher, so the facts are looked up here -- the
/// target's *own* position out of the object table, or the local player's, and its
/// **`cached_velocity`** (`get_velocity` reads the cached word) off whichever
/// collision body it has -- and handed to the watcher's driver as a `TargetSnapshot`.
/// The 0.5 s tick, the radius test and the delivery are `MotionDriver::handle_targetting`'s,
/// per physics sub-step, matching their position in the client's update loop.
///
/// Every `MoveToObject`, `TurnToObject` and `StickToObject` the server sends a remote
/// object in the corpus names the session's own character; the local body's own move-to
/// (a use, a pickup) names a server object.
///
/// A **fresh** subscription gets `AddVoyeur`'s unconditional first update right
/// here, the moment its target is resolved -- retail sends it synchronously inside
/// `SetTarget`, and a creature whose body is not stepped this frame
/// (`update_object`'s `HUGE_QUANTUM` exit on a replay clock jump) must still
/// have its plan built by the time anything samples it.
pub fn resolve_object_targets<S: AsObjectSim>(
    objects: &mut BTreeMap<ObjectId, S>,
    character: &mut Option<Character>,
    player_object: Option<ObjectId>,
    now: LocalTime,
) {
    use dereth_animation::motion::moveto::TargetSnapshot;
    let player = character.as_ref().map(|c| (c.position(), c.velocity()));
    let snapshot = |facts: Option<(Position, Vec3)>, id: ObjectId, here: Option<Position>| {
        match facts {
            Some((position, velocity)) => Some(TargetSnapshot {
                object_id: id,
                ok: true,
                position,
                velocity,
            }),
            // The object left the world -- somebody else picked it up.
            // `HandleUpdateTarget` cancels with `ObjectGone (0x37)` after the first
            // update and `NoObject (0x38)` before it.
            None => here.map(|position| TargetSnapshot {
                object_id: id,
                ok: false,
                position,
                velocity: Vec3::ZERO,
            }),
        }
    };
    let ids: Vec<ObjectId> = objects.keys().copied().collect();
    for id in ids {
        let Some(t) = objects
            .get(&id)
            .and_then(|o| o.sim().driver.borrow().target)
        else {
            continue;
        };
        let facts = target_facts(objects, character, player_object, t.id, player);
        // The first update runs `MoveToObject_Internal` right now, and it
        // measures the bearing from `MotionEnv::position`: refresh it, as the ladder does
        // before the per-frame update (the bearing is checked to 0.01 degrees).
        refresh_object_env(objects, character, id);
        let Some(o) = objects.get_mut(&id).map(AsObjectSim::sim_mut) else {
            continue;
        };
        let snap = snapshot(facts, t.id, o.position);
        let first = {
            let mut driver = o.driver.borrow_mut();
            driver.set_target_snapshot(snap);
            driver.deliver_first_target_update(now)
        };
        if let Some(fx) = first {
            o.pending_physics_effects.extend(fx);
        }
    }
    let local = character
        .as_ref()
        .and_then(|c| c.wanted_target())
        .map(|t| t.id);
    if let Some(tid) = local {
        let facts = target_facts(objects, character, player_object, tid, player);
        if let Some(c) = character.as_mut() {
            let here = c.position();
            c.set_target_snapshot(snapshot(facts, tid, Some(here)));
            c.deliver_first_target_update(now);
        }
    }
}

/// What `SendVoyeurUpdate` reads off the target: its position and its
/// `cached_velocity`. `None` when the object is not in the world.
pub fn target_facts<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    character: &Option<Character>,
    player_object: Option<ObjectId>,
    id: ObjectId,
    player: Option<(Position, Vec3)>,
) -> Option<(Position, Vec3)> {
    if player_object == Some(id) {
        return player;
    }
    let o = objects.get(&id)?.sim();
    let position = o.position?;
    let body = o
        .physics_handle
        .and_then(|h| character.as_ref().and_then(|c| c.world.get(h)));
    Some((position, body.map_or(Vec3::ZERO, |b| b.cached_velocity)))
}

/// `SceneStats::remote_target_updates`, accumulated off each driver's own count now that
/// the deliveries happen inside the sub-steps.
pub fn account_object_targets<S: AsObjectSim>(
    objects: &mut BTreeMap<ObjectId, S>,
    stats: &mut ObjectStepStats,
) {
    for o in objects.values_mut().map(AsObjectSim::sim_mut) {
        let n = o.driver.borrow().target_updates;
        let delivered = n.wrapping_sub(o.target_updates_seen);
        o.target_updates_seen = n;
        stats.remote_target_updates += u64::from(delivered);
    }
}

/// Apply the position manager's sticky offset.
///
/// The original position update builds one **local** offset frame —
/// the animation's, scaled — then hands it to the position manager, which *overwrites* its
/// origin with the sticky pull and sets its heading at the target, then
/// combines it with the object's position frame. That is what happens here, with
/// two omissions this function does not hide:
///
/// * the animation offset is **not** in it, because `advance_objects` discards it
///   for a server object (its own note says why), and the sticky arm overwrites the origin
///   anyway;
/// * the physics transition does not run afterwards, so the pull is not
///   collided against the world. A creature glued to a player who runs through a wall is
///   pulled through it and the server's next `0xF748` puts it right.
///
/// This is the bodyless viewer fallback's client-side position contribution. It is
/// separate from integration because a stick is a direct pull rather than an
/// integration: it needs no velocity, no contact plane and no transition.
pub fn pull_stuck_objects<S: AsObjectSim>(
    objects: &mut BTreeMap<ObjectId, S>,
    stats: &mut ObjectStepStats,
    quanta: &[(ObjectId, f64)],
) {
    let mut pulled = 0u64;
    for (id, quantum) in quanta {
        #[allow(clippy::cast_possible_truncation)] // a frame quantum, under HUGE_QUANTUM
        let q = *quantum as f32;
        let Some(o) = objects.get_mut(id).map(AsObjectSim::sim_mut) else {
            continue;
        };
        let driver = o.driver.borrow();
        if !driver.movement.sticky.is_sticky() || !driver.movement.sticky.initialized {
            continue;
        }
        let Some(here) = o.position else { continue };
        let mut offset = Frame::default();
        driver.movement.sticky.adjust_offset(
            &mut offset,
            q,
            &here,
            o.radius,
            Some(&driver.movement.interp),
            &driver.env,
        );
        let combined = dereth_animation::frame::combine(&here.frame, &offset);
        o.position = Some(dereth_primitives::Position::new(here.cell, combined));
        pulled += 1;
    }
    stats.remote_sticks_pulled += pulled;
}

/// The object a server object is stuck to, if any, exposed for a test.
#[must_use]
pub fn server_object_sticky<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    id: ObjectId,
) -> Option<ObjectId> {
    let o = objects.get(&id)?.sim();
    o.driver
        .borrow()
        .movement
        .sticky
        .is_sticky()
        .then(|| o.driver.borrow().movement.sticky.target_id)
}

/// Whether a server object's stick has had a `TargetInfo` yet — the sticky manager's
/// `initialized` flag, which is `adjust_offset`'s own gate.
#[must_use]
pub fn server_object_sticky_initialized<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    id: ObjectId,
) -> bool {
    objects
        .get(&id)
        .is_some_and(|o| o.sim().driver.borrow().movement.sticky.initialized)
}

/// Whether a move-to is active and which arm it is running, for a server
/// object.
#[must_use]
pub fn server_object_move_to<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    id: ObjectId,
) -> Option<(bool, dereth_animation::table::MovementType, ObjectId)> {
    let o = objects.get(&id)?.sim();
    let driver = o.driver.borrow();
    let m = &driver.movement.moveto;
    Some((m.is_moving_to(), m.movement_type, m.top_level_object_id))
}

/// What a server object's `MoveToManager` is **doing**: the command it has issued, its
/// auxiliary (turn) command, how many nodes are still queued, and whether a `TargetInfo`
/// has arrived.
///
/// Beginning forward movement and beginning a turn toward a heading
/// are the writers of the nonzero `current_command` and `aux_command` values.
/// Movement simulation refuses to take a step at all while
/// `!initialized` on an object-shaped arm — so these four are the difference between *"a
/// move-to was recorded"* and *"the creature was told to move"*.
#[must_use]
pub fn server_object_move_to_state<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    id: ObjectId,
) -> Option<(MotionCommand, MotionCommand, usize, bool)> {
    let driver = objects.get(&id)?.sim().driver.borrow();
    let m = &driver.movement.moveto;
    Some((
        m.current_command,
        m.aux_command,
        m.pending_actions.len(),
        m.initialized,
    ))
}

/// The **head** of the movement queue — the node simulation
/// switches on this frame — as `(type, heading)`.
///
/// `MovementNode` is a movement type plus a float heading, and only
/// `MoveToPosition (7)` and `TurnToHeading (9)` are ever used. The heading is what
/// `HandleTurnToHeading` measures `heading_greater` against and what
/// it snaps the body to with `set_heading` (the node's heading, sending the event), so a test cannot state
/// retail's turn rule at all without it. `server_object_move_to_state` reports the
/// queue's *length*, which cannot distinguish "turning to the right heading" from
/// "turning to the wrong one".
#[must_use]
pub fn server_object_move_to_node<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    id: ObjectId,
) -> Option<(dereth_animation::table::MovementType, f32)> {
    let driver = objects.get(&id)?.sim().driver.borrow();
    let node = driver.movement.moveto.pending_actions.front().copied()?;
    Some((node.kind, node.heading))
}

/// Everything `MoveToManager` aims at: `current_target_position`, `moving_away`, and the
/// target cylinder `sought_object_radius` / `sought_object_height`.
///
/// `current_target_position` is the goal used *right now* — the last position
/// `HandleUpdateTarget` reported, which is **not** the target object's live
/// position — and `moving_away` comes from movement-parameter command selection. The
/// corrective turn adds the heading from the current position to that reported target
/// to `get_desired_heading(current_command, moving_away)`, so a test that re-derives the
/// 20°/340° dead band from the live player position instead measures something retail
/// never computed.
///
/// The radius and height are the other half of the current-distance calculation's
/// sphere-aware arm — `(my_radius, my_height, position,
/// sought_object_radius, sought_object_height, current_target_position)`. They come off
/// the **target's** radius and height, so an arrival test written with
/// a bare point distance, or with the mover's radius alone, is measuring a different
/// number from the one retail compares against `distance_to_object`.
#[must_use]
pub fn server_object_move_to_aim<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    id: ObjectId,
) -> Option<(dereth_primitives::Position, bool, f32, f32)> {
    let driver = objects.get(&id)?.sim().driver.borrow();
    let m = &driver.movement.moveto;
    // Resetting the move-to's local state clears the position to cell 0 when the
    // move-to ends, and a cylinder distance measured across cell 0 is nonsense rather
    // than large. Reported only while there is a move-to to aim.
    m.is_moving_to().then_some({
        (
            m.current_target_position,
            m.moving_away,
            m.sought_object_radius,
            m.sought_object_height,
        )
    })
}

/// The extrapolation lead held for a server
/// object's target subscription. `HandleMoveToPosition`'s
/// third block is the only writer, and it writes **only** when the new ETA differs from
/// the stored one by more than one second.
#[must_use]
pub fn server_object_target_quantum<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    id: ObjectId,
) -> Option<f32> {
    objects
        .get(&id)?
        .sim()
        .driver
        .borrow()
        .target
        .map(|t| t.quantum)
}

/// The last `TargetInfo` a server object's `HandleUpdateTarget` received --
/// the raw `target_position` and the `interpolated_position` `GetInterpolatedPosition`
/// led by the target's `cached_velocity` times the quantum.
#[must_use]
pub fn server_object_last_target_info<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    id: ObjectId,
) -> Option<dereth_animation::motion::moveto::TargetInfo> {
    objects.get(&id)?.sim().driver.borrow().last_target_info
}

/// How many times target tracking has run for a server object
/// (once per physics sub-step) and how many `TargetInfo`s it delivered.
#[must_use]
pub fn server_object_targetting<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    id: ObjectId,
) -> Option<(u32, u32)> {
    let d = objects.get(&id)?.sim().driver.borrow();
    Some((d.targetting_ticks, d.target_updates))
}

/// Pending motion-interpreter actions for a server object, as a count.
/// Beginning a turn-to-heading refuses to take a move-to's first step
/// while it is non-zero, so it is the first thing to look at when a move-to is recorded,
/// targeted, queued -- and frozen.
#[must_use]
pub fn server_object_motions_pending<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    id: ObjectId,
) -> Option<usize> {
    Some(
        objects
            .get(&id)?
            .sim()
            .driver
            .borrow()
            .movement
            .interp
            .pending_motions
            .len(),
    )
}

/// The parallel PartArray table ledger, without cloning animation or owner state.
#[must_use]
pub fn server_object_table_motions_pending<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    id: ObjectId,
) -> Option<usize> {
    Some(
        objects
            .get(&id)?
            .sim()
            .driver
            .borrow()
            .motion_table
            .pending_len(),
    )
}

/// A server object's interpreted turn command — the other half of
/// `server_object_motion`, which reports only the forward one.
/// 53 of the corpus's 83 remote arms are `TurnToObject`, so a suite that watched the
/// forward command alone would be blind to the arm the server sends most.
#[must_use]
pub fn server_object_turn<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    id: ObjectId,
) -> Option<MotionCommand> {
    let o = objects.get(&id)?.sim();
    Some(
        o.driver
            .borrow()
            .movement
            .interp
            .interpreted_state
            .turn_command,
    )
}

/// The target a server object is watching, as recorded by target selection.
///
#[must_use]
pub fn server_object_target<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    id: ObjectId,
) -> Option<ObjectId> {
    objects
        .get(&id)
        .and_then(|o| o.sim().driver.borrow().target)
        .map(|t| t.id)
}

/// A server object's standing-long-jump flag — the sibling flag,
/// which is inert across the whole corpus and is wired anyway.
#[must_use]
pub fn server_object_standing_longjump<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    id: ObjectId,
) -> bool {
    objects
        .get(&id)
        .is_some_and(|o| o.sim().driver.borrow().movement.interp.standing_longjump)
}

/// Where the scene currently has a server object, in its own `Position` —
/// the acceptance observable: a stuck creature's distance from what it is stuck to.
#[must_use]
pub fn server_object_position<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    id: ObjectId,
) -> Option<Position> {
    objects.get(&id).and_then(|o| o.sim().position)
}

/// The last position the **server** named for an object, before any client-side
/// contribution. The pull is exactly the difference between this and
/// `server_object_position`.
#[must_use]
pub fn server_object_reported_position<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    id: ObjectId,
) -> Option<Position> {
    objects.get(&id).and_then(|o| o.sim().server_position)
}

/// A point inside one resident interior cell that a body can stand at, in that cell's own
/// block-local space, for `--start-cell` and for the tests.
///
/// The cell's **own** geometry decides, not this function: `cell_bsp` through
/// its point-inside test says whether a point is in the room, and
/// `physics_bsp` says whether it is inside the masonry. Sampling is only how the two are
/// asked; the answer is the client's.
///
/// It lives here rather than in `dereth_scene::world_scene` because it is a physics query over the
/// body's own `LandSource` and names no render type.
#[must_use]
pub fn standable_point(character: &Character, cell: dereth_primitives::CellId) -> Option<Vec3> {
    let land = character.land();
    let g = dereth_physics::LandSource::env_cell(land.as_ref(), cell)?;
    let bsp = g.cell_bsp.as_ref()?;
    for &z in &[0.5f32, 1.0, 1.5, 2.0] {
        for i in -12i8..=12 {
            for j in -12i8..=12 {
                let local = Vec3::new(f32::from(i) * 0.5, f32::from(j) * 0.5, z);
                if !bsp.point_inside_cell_bsp(local) {
                    continue;
                }
                if g.physics_bsp
                    .as_ref()
                    .is_some_and(|b| b.point_intersects_solid(local))
                {
                    continue;
                }
                return Some(dereth_physics::math::localtoglobal(&g.frame, local));
            }
        }
    }
    None
}

/// The land-block restriction lookup and environment-cell unpacker use this restriction object as
/// the physics world's cell fence. Returns how many cells were fenced.
///
/// The bake table the pairs are collected into is the scene's
/// (`dereth_scene::world_scene::init_cell_restrictions` fills `BlockDraw::cell_restrictions`); this
/// is the half that talks to `dereth_physics`.
pub fn register_cell_restrictions(
    character: &mut Character,
    cells: &[(dereth_primitives::CellId, ObjectId)],
) -> usize {
    for &(cell, obj) in cells {
        character.world.set_cell_restriction(cell, Some(obj));
    }
    cells.len()
}

/// The object's time-stepping sequence — the `MAX_QUANTUM` loop,
/// the `MIN_STEP` and `HUGE_QUANTUM` early-outs, and the rule that `update_time` takes the
/// *simulated* clock and not `now` (traps 1 and 2 below) -- with the position half removed,
/// for objects without an attached dynamic physics body.
///
/// Attached bodies already advanced in `Character`'s `PhysicsWorld` sweep and must skip this
/// ladder; their achieved positions were published by [`finish_object_physics`]. The animation
/// offset `advance` returns is therefore discarded rather than applied. That is a declared
/// bodyless-viewer limitation, not the production remote-body path.
///
/// The part of `dereth_scene::world_scene::advance_objects` that is *not* here is the loop that
/// writes each object's **drawn** frame through `WorldScene::render_frame_of` and places its
/// held children: that writes render state, so it stays with the scene and runs immediately
/// after this returns.
pub fn advance_object_ladder<S: AsObjectSim>(
    objects: &mut BTreeMap<ObjectId, S>,
    character: &mut Option<Character>,
    stats: &mut ObjectStepStats,
    now: LocalTime,
    physics_ticked: bool,
) {
    use dereth_animation::step_animation;
    use dereth_physics::globals::{HUGE_QUANTUM, MAX_QUANTUM, MIN_QUANTUM, MIN_STEP};
    // Refresh `MotionEnv` before the ladder, because movement simulation
    // runs inside it and every distance a `MoveToManager` measures comes out
    // of the environment.
    let step_ids: Vec<ObjectId> = objects.keys().copied().collect();
    for id in &step_ids {
        refresh_object_env(objects, character, *id);
    }
    // What the movement layer asked the physics object for during
    // the ladder, collected here and applied below. Move-to simulation raises
    // `SetHeading` from inside `tick_movement` (subscription/quantum facts publish inline);
    // without this drain a server object's heading snaps would never be applied.
    let mut queued: Vec<(ObjectId, Vec<dereth_animation::motion::MotionEffect>)> = Vec::new();
    // Each object's own simulated elapsed for this frame, summed over the sub-steps the ladder took.
    let mut quanta: Vec<(ObjectId, f64)> = Vec::new();
    for (oid, o) in objects.iter_mut().map(|(i, o)| (i, o.sim_mut())) {
        if o.physics_handle.is_some() {
            continue;
        }
        o.driver.borrow_mut().cur_time = ServerTime(now.0);
        // Object updating and static-object animation
        // run only when the outer physics tick takes its update arm. The inner
        // `MIN_STEP` test below is part of each object's ladder and cannot stand in for
        // that shared outer gate: at 120 Hz it admitted every display frame. Keep the
        // frame/part placement loop below unconditional, because `0xF748` is dispatched
        // outside the per-frame update and must reach the very next draw even on a quiet frame.
        if !physics_ticked {
            continue;
        }
        if o.update_time <= 0.0 {
            o.update_time = now.0;
            continue;
        }
        let mut elapsed = now.0 - o.update_time;
        if elapsed <= MIN_STEP || elapsed > HUGE_QUANTUM {
            o.update_time = now.0;
            continue;
        }
        let mut sim = o.update_time;
        let mut done = false;
        if elapsed > MAX_QUANTUM {
            loop {
                sim += MAX_QUANTUM;
                step_animation(&mut o.driver.borrow_mut(), MAX_QUANTUM, now);
                elapsed -= MAX_QUANTUM;
                if elapsed <= MAX_QUANTUM {
                    break;
                }
            }
            // The test is inside the branch, exactly as in physics' `update_object`.
            if elapsed <= MIN_QUANTUM {
                done = true;
            }
        }
        if !done {
            sim += elapsed;
            step_animation(&mut o.driver.borrow_mut(), elapsed, now);
        }
        // The simulated clock, not `now`.
        quanta.push((*oid, sim - o.update_time));
        o.update_time = sim;
        let fx = o.driver.borrow_mut().take_effects();
        if !fx.is_empty() {
            queued.push((*oid, fx));
        }
    }
    for (id, fx) in queued {
        apply_object_effects(objects, character, stats, id, fx);
    }
    // Target updates: a move-to and a stick both wait on
    // one, and neither takes a step until the first arrives. Delivery
    // runs per sub-step, inside `step_animation` / the physics sweep; only the
    // count is taken here.
    account_object_targets(objects, stats);
    // The position manager adjusts the offset during its internal position update.
    pull_stuck_objects(objects, stats, &quanta);
}
