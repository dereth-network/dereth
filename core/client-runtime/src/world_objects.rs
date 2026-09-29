//! The server's objects entering, changing and leaving the world, with no device: the
//! simulation half of object dispatch.
//!
//! A created object gets its `MotionDriver` (setup, motion table, script table, placement, scale,
//! description) and its `WorldObject`; a removed one loses it; positions, state words, placements,
//! movement buffers, vector updates and scripts reach the objects that exist. A presentation that
//! draws is told, through [`ObjectAppearance`], exactly where its drawing half has to follow: an
//! object removed, the removals done, an object's parts built, the body's parts changed. It
//! answers at the point it answers today, so its uploads keep their order and an upload that
//! fails stops the dispatch where it did.
//!
//! [`ObjectAppearance`]: crate::world_objects::ObjectAppearance

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use dereth_animation::MotionDriver;
use dereth_dat::RetailDatStore;
use dereth_physics::obj::{PhysicsState, StateSideEffects};
use dereth_primitives::{DataId, Frame, ObjectId, Vec3};

use crate::anim_assets::DatAnimAssets;
use crate::movement::to_anim_objdesc;
use crate::object_step::{ObjectSim, ObjectStepStats};
use crate::objects::ObjectStream;
use crate::scene::SceneConfig;
use crate::world_state::{WorldObject, WorldState};

/// What object dispatch counted. A presentation that keeps its own counters folds these in after
/// each call.
#[derive(Debug, Default, Clone, Copy)]
pub struct ObjectCounters {
    /// The object step's counters, from the movement buffers applied.
    pub steps: ObjectStepStats,
    pub objdesc_setup_mismatch: u64,
    pub objdesc_failures: u64,
    pub objdescs_applied: u64,
    pub object_nodraw_set: u64,
    pub object_nodraw_cleared: u64,
    pub object_hidden_changed: u64,
    pub scripts_played: u64,
    pub scripts_unplayed: u64,
    pub vector_updates_applied: u64,
    pub vector_updates_deferred: u64,
    pub vector_updates_without_body: u64,
    pub character_states_applied: u64,
    pub restriction_effects_disabled: u64,
    pub restriction_effects_unplayed: u64,
    pub restriction_effects_played: u64,
    /// How many server objects the world holds, and how many of them animate, after a sync.
    pub server_objects: Option<usize>,
    pub server_objects_animated: Option<usize>,
}

impl ObjectCounters {
    fn fold_steps(&mut self, d: ObjectStepStats) {
        self.steps.remote_move_tos_failed += d.remote_move_tos_failed;
        if d.remote_move_tos_failed != 0 {
            self.steps.remote_last_move_to_error = d.remote_last_move_to_error;
        }
        self.steps.remote_target_updates += d.remote_target_updates;
        self.steps.remote_sticks_pulled += d.remote_sticks_pulled;
        self.steps.remote_sticks_applied += d.remote_sticks_applied;
        self.steps.remote_sticks_unresolved += d.remote_sticks_unresolved;
        self.steps.remote_move_tos_performed += d.remote_move_tos_performed;
    }

    fn script(&mut self, played: bool) {
        if played {
            self.scripts_played += 1;
        } else {
            self.scripts_unplayed += 1;
        }
    }

    fn nodraw(&mut self, on: bool) {
        if on {
            self.object_nodraw_set += 1;
        } else {
            self.object_nodraw_cleared += 1;
        }
    }
}

/// Where a presentation's drawing half follows object dispatch. Every method is called at the
/// point the drawing has to change, with the world as it stands there.
pub trait ObjectAppearance {
    /// What stops a dispatch: a drawing resource that could not be created.
    type Error;
    /// `id` has just left the world.
    fn removed(&mut self, ws: &WorldState, id: ObjectId);
    /// Every removal of this dispatch is done.
    fn removals_done(&mut self, ws: &WorldState);
    /// The body's parts changed: its setup was rebuilt, or the server's description was applied
    /// to it.
    ///
    /// # Errors
    /// The body's drawing could not be rebuilt.
    fn body_parts_changed(&mut self, ws: &WorldState) -> Result<(), Self::Error>;
    /// Server object `id` has just been built from `setup` and its wire description, and is about
    /// to join the world; `driver` holds its parts. `is_player` when it is the local player's own
    /// object (only reached with no local body).
    ///
    /// # Errors
    /// The object's drawing could not be built; the dispatch stops there.
    fn object_built(
        &mut self,
        ws: &WorldState,
        id: ObjectId,
        setup: DataId,
        objdesc: &dereth_protocol::types::ObjDesc,
        driver: &MotionDriver,
        is_player: bool,
    ) -> Result<(), Self::Error>;
    /// Every create of this dispatch is done.
    fn creates_done(&mut self, ws: &WorldState);
}

/// The appearance of a presentation that draws nothing.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoAppearance;

impl ObjectAppearance for NoAppearance {
    type Error = std::convert::Infallible;
    fn removed(&mut self, _: &WorldState, _: ObjectId) {}
    fn removals_done(&mut self, _: &WorldState) {}
    fn body_parts_changed(&mut self, _: &WorldState) -> Result<(), Self::Error> {
        Ok(())
    }
    fn object_built(
        &mut self,
        _: &WorldState,
        _: ObjectId,
        _: DataId,
        _: &dereth_protocol::types::ObjDesc,
        _: &MotionDriver,
        _: bool,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn creates_done(&mut self, _: &WorldState) {}
}

/// Materialize pending removals and creates, and the local player's setup and appearance, before
/// any movement is dispatched: a create must install its movement manager before a later movement
/// event, and a target created earlier in the batch must exist when a move-to resolves it.
///
/// # Errors
/// Whatever the presentation's drawing half refuses.
pub fn prepare_object_dispatch<H: ObjectAppearance>(
    ws: &mut WorldState,
    store: &Arc<RetailDatStore>,
    cfg: &SceneConfig,
    stream: &mut ObjectStream,
    counters: &mut ObjectCounters,
    hooks: &mut H,
) -> Result<(), H::Error> {
    // The object stream's bodies take the same switch the cell statics do.
    stream.physics.mesh_collision = cfg.mesh_collision;
    // Object release for every cell the streaming window flushed, before the removal drain, for
    // the same reason cells are flushed before they are released: an object may not be left
    // pointing at a cell that has already gone. The clock is the object step's last, at most one
    // frame stale against a 25-second deferred-destruction timer.
    if !ws.released_interiors.is_empty() {
        let now = dereth_primitives::LocalTime(ws.last_object_time);
        for b in std::mem::take(&mut ws.released_interiors) {
            if let Some(c) = ws.character.as_mut() {
                stream.release_block_obj_cells_with_physics(b, now, &mut c.world);
            } else {
                stream.release_block_obj_cells(b, now);
            }
        }
    }
    for id in stream.take_removed() {
        ws.objects.remove(&id);
        hooks.removed(ws, id);
    }
    hooks.removals_done(ws);
    let assets = match &ws.anim_assets {
        Some(a) => Arc::clone(a),
        None => {
            let a = Arc::new(DatAnimAssets::new(Arc::clone(store)));
            ws.anim_assets = Some(Arc::clone(&a));
            a
        }
    };

    let local_body = ws.character.is_some();
    if !local_body {
        stream.discard_bodyless_player_dispatches();
    }
    // Latched for held-object placement, which runs without the stream.
    ws.player_object = stream.player();
    for id in stream.take_created() {
        // The player's own object is already in the world as the body, which local physics
        // drives: the client has one physics body for the player, not two. His description still
        // has to reach that one part array.
        if local_body && stream.player() == Some(id) {
            let (setup, mtable, phs, od) = match stream.presence(id) {
                Some(p) => (p.setup_id, p.mtable_id, p.phs_table, p.objdesc.clone()),
                None => continue,
            };
            apply_player_objdesc(ws, counters, hooks, setup, mtable, &od)?;
            // After the setup swap, because installing a setup installs its default script table
            // and would otherwise stamp over the server's.
            if let Some(t) = phs {
                if let Some(c) = ws.character.as_ref() {
                    c.driver_mut().script_table = Some(t);
                }
            }
            continue;
        }
        let Some(p) = stream.presence(id) else {
            continue;
        };
        let Some(setup_id) = p.setup_id else { continue };
        // An object with neither a position nor a holder is in a container and not in the world.
        // `position` is where it is (republished off its live body); `server_position` is the
        // server's last word, which position edges are measured against.
        let (position, parent) = (p.position, p.parent);
        let wire_position = p.server_position;
        if position.is_none() && parent.is_none() {
            continue;
        }
        // Read here, applied after the insert below through the one path.
        let wire_state = stream.physics_state(id).unwrap_or(0);
        let (scale, mtable) = (p.scale, p.mtable_id);
        let objdesc = to_anim_objdesc(&p.objdesc);
        let Some(setup) = dereth_animation::data::AnimAssets::setup(assets.as_ref(), setup_id)
        else {
            continue;
        };

        let mut driver =
            MotionDriver::new(Arc::clone(&assets) as Arc<dyn dereth_animation::data::AnimAssets>);
        if !driver.set_setup(Arc::clone(&setup)) {
            continue;
        }
        // Installing the motion table enters the default state, which queues the standing cycle.
        let animated = mtable.is_some_and(|mt| mt != DataId(0) && driver.set_motion_table(mt));
        // The description's script table overrides the setup's; a zero names none, which is not
        // the same as the field being absent.
        if let Some(t) = p.phs_table {
            driver.script_table = (t != DataId(0)).then_some(t);
        }
        // The description's placement, after the motion table, with the named id, then key 0,
        // then the identity as the fallback chain.
        let placement = p.placement;
        driver
            .part_array
            .set_placement_frame(placement, &mut driver.sequence);
        // Object scale, folded into every part on top of the setup's own default scale.
        let scale = if scale > 0.0 { scale } else { 1.0 };
        driver.scale = scale;
        if (scale - 1.0).abs() > f32::EPSILON {
            driver
                .part_array
                .set_scale_internal(Vec3::new(scale, scale, scale));
        }
        // The create path applies the description to the object it just made: part swaps,
        // texture-map swaps and the shift palette, through the driver so a swap is resolved
        // against the dat.
        if !objdesc.part_changes.is_empty()
            || !objdesc.texture_changes.is_empty()
            || !objdesc.subpalettes.is_empty()
        {
            if !driver.do_obj_desc_changes_from_default(&objdesc) {
                counters.objdesc_failures += 1;
            }
            counters.objdescs_applied += 1;
        }
        // With a local body the player never reaches here; without one the server's copy of him
        // is the only body there is.
        let is_player = stream.player() == Some(id);
        hooks.object_built(ws, id, setup_id, &p.objdesc, &driver, is_player)?;
        // A held object has no frame of its own until its holder is placed; the identity stands
        // in for one frame and is never drawn, because parts are updated before any submission.
        let frame = position.map_or_else(Frame::default, |q| {
            crate::world_step::render_frame_of(ws, cfg.landblock, q)
        });
        ws.objects.insert(
            id,
            WorldObject {
                sim: ObjectSim {
                    driver: Rc::new(RefCell::new(driver)),
                    physics_handle: None,
                    pending_physics_effects: Vec::new(),
                    target_updates_seen: 0,
                    position,
                    parent,
                    // The constructor's word; the description's is applied below through the
                    // same path a state-word message takes.
                    state: PhysicsState::DEFAULT,
                    update_time: 0.0,
                    animated,
                    // The create's own position is the first the received-position handler would
                    // have delivered, so the edge starts armed at it.
                    server_position: wire_position,
                    // Filled in once the body this object's part array produced exists.
                    radius: 0.0,
                    height: 0.0,
                },
                is_player,
                wire_state: PhysicsState::DEFAULT.0,
                // A held object has no place until its holder is found.
                drawn: parent.is_none(),
                frame,
                placement,
                sound_table: setup.default_sound_table,
            },
        );
        // The create path sets the description's state word on the object it just built, then
        // (second arm) sets its parent, whose tail forces no-draw on under a hidden holder.
        apply_object_state(ws, cfg, counters, id, PhysicsState(wire_state));
        if parent.is_some() {
            reparent_nodraw(ws, cfg, counters, id, None, parent);
        }
        // Entering the world is owed until the description's movement buffer has been applied.
        if parent.is_none() && position.is_some() {
            ws.entering_world.push(id);
        }
    }
    hooks.creates_done(ws);
    Ok(())
}

/// The server's setup and description for the local player, applied to the body: the setup
/// rebuilt first (the body has to be the server's body before an index into it means anything),
/// then the description through the driver.
fn apply_player_objdesc<H: ObjectAppearance>(
    ws: &mut WorldState,
    counters: &mut ObjectCounters,
    hooks: &mut H,
    setup_id: Option<DataId>,
    mtable_id: Option<DataId>,
    od: &dereth_protocol::types::ObjDesc,
) -> Result<(), H::Error> {
    if ws.character.is_none() {
        return Ok(());
    }
    let mut rebuilt = false;
    if let Some(want) = setup_id {
        // Unconditionally: the setter owns the "it is already that setup" decision.
        let Some(c) = ws.character.as_mut() else {
            return Ok(());
        };
        match c.set_setup_id(want, mtable_id) {
            Ok(changed) => rebuilt = changed,
            Err(e) => {
                // The body is untouched (both halves are resolved before anything changes), so
                // refusing the description leaves a consistent object.
                counters.objdesc_setup_mismatch += 1;
                tracing::warn!(
                    "the server's player setup record {:#010X} would not build \
                     ({e}); the description is refused",
                    want.0
                );
                return Ok(());
            }
        }
        if rebuilt {
            if let Some(c) = ws.character.as_ref() {
                ws.character_sound_table = crate::world_build::body_sound_table(c);
            }
        }
    }
    let anim = to_anim_objdesc(od);
    if anim == dereth_animation::parts::ObjDesc::default() {
        // A rebuild with nothing to apply still changed the parts.
        if rebuilt {
            hooks.body_parts_changed(ws)?;
        }
        return Ok(());
    }
    {
        let Some(c) = ws.character.as_ref() else {
            return Ok(());
        };
        // Through the driver, whose asset provider resolves the replacements.
        if !c.driver_mut().do_obj_desc_changes_from_default(&anim) {
            counters.objdesc_failures += 1;
        }
    }
    counters.objdescs_applied += 1;
    hooks.body_parts_changed(ws)
}

/// Give pending objects their simulation, then apply positions, state words, placements, movement,
/// world entry and vector updates to the objects that exist, and play the queued scripts.
///
/// # Errors
/// Whatever the presentation's drawing half refuses.
pub fn sync_objects<H: ObjectAppearance>(
    ws: &mut WorldState,
    store: &Arc<RetailDatStore>,
    cfg: &SceneConfig,
    stream: &mut ObjectStream,
    counters: &mut ObjectCounters,
    hooks: &mut H,
) -> Result<(), H::Error> {
    prepare_object_dispatch(ws, store, cfg, stream, counters, hooks)?;
    let local_body = ws.character.is_some();
    // Positions and movement buffers apply to objects that already exist, so after the creates.
    let ids: Vec<ObjectId> = ws.objects.keys().copied().collect();
    // With a local body the player's position belongs to the body; his movement buffer is not
    // thrown away, because a use on something out of reach is answered with a move-to addressed to
    // him, and executing it is the whole approach walk.
    if local_body {
        if let Some(id) = stream.player() {
            if let Some(buf) = stream.take_movement(id) {
                ws.dispatch_player_movement(&buf, stream);
            }
        }
    }
    let player = stream.player();
    for id in ids.iter().copied() {
        // The parent travels with the position: a position event clears the parent and a
        // parent event clears the position. `server_position` is the wire's edge; the achieved
        // position comes off the body.
        if let Some((pos, par)) = stream.presence(id).map(|p| (p.server_position, p.parent)) {
            let mut was: Option<Option<(ObjectId, u32)>> = None;
            if let Some(o) = ws.objects.get_mut(&id) {
                // The holder's hidden bit decides no-draw on attach and detach, so the change is
                // recorded here and acted on once the borrow is released.
                if o.sim.parent != par {
                    was = Some(o.sim.parent);
                }
                // An edge, not a level: re-applying the cached copy every frame would stamp the
                // server's last word over a stuck object's pull toward its target.
                if o.sim.server_position != pos {
                    o.sim.server_position = pos;
                    o.sim.position = pos;
                }
                o.sim.parent = par;
            }
            if let Some(before) = was {
                reparent_nodraw(ws, cfg, counters, id, before, par);
            }
        }
        // The state word reaching the parts, gated on it having changed: applying it also forces
        // no-draw on children, and re-running every frame would fight itself.
        if let Some(w) = stream.physics_state(id) {
            if ws.objects.get(&id).is_some_and(|o| o.wire_state != w) {
                apply_object_state(ws, cfg, counters, id, PhysicsState(w));
            }
        }
        // A new placement re-poses an object, except under a playing animation (the placement is
        // only what the sequence falls back to).
        if let Some(pid) = stream.take_pending_placement(id) {
            if let Some(o) = ws.objects.get_mut(&id) {
                if !o.sim.driver.borrow().sequence.has_anims() && o.placement != pid {
                    o.placement = pid;
                    let mut driver = o.sim.driver.borrow_mut();
                    let MotionDriver {
                        part_array,
                        sequence,
                        ..
                    } = &mut *driver;
                    part_array.set_placement_frame(pid, sequence);
                    stream.placement_installed(id, pid);
                }
            }
        }
        // Movement unpacking for a remote object resolves ids against the whole object table.
        if let Some(buf) = stream.take_movement(id) {
            if ws.objects.contains_key(&id) {
                apply_movement(ws, counters, id, &buf, stream, player == Some(id));
            }
        }
        // The create path's animation tail, now that the description's movement buffer has been
        // unpacked: movement unpacking, then entering the world.
        if let Some(at) = ws.entering_world.iter().position(|e| *e == id) {
            ws.entering_world.swap_remove(at);
            if let Some(o) = ws.objects.get(&id) {
                o.sim.driver.borrow_mut().enter_world();
            }
        }
        // Velocity, then angular velocity, unconditionally on each other.
        if let Some((velocity, omega)) = stream.take_vector_update(id) {
            let now = ws.last_object_time;
            let body = ws
                .character
                .as_mut()
                .and_then(|c| c.world.by_object_id(id).and_then(|h| c.world.get_mut(h)));
            if let Some(body) = body {
                body.set_velocity(velocity, now);
                body.set_omega(omega);
                counters.vector_updates_applied += 1;
            } else if stream.park_vector_update(id, (velocity, omega)) {
                // Re-parked: a create's own velocity reaches here the frame before its body is
                // made, and dropping it would drop every projectile's launch.
                counters.vector_updates_deferred += 1;
            } else {
                // The object left the table between the take and the put-back.
                counters.vector_updates_without_body += 1;
            }
        }
    }
    // Anything still owed a world entry names an object no longer in the table.
    ws.entering_world.clear();
    // The radius and height a move-to, a turn-to and a stick read off each body.
    ws.refresh_object_geometry(stream);
    // A new voyeur's first update for any subscription this batch created, with the world as it
    // stands when the arm is applied.
    ws.resolve_object_targets(dereth_primitives::LocalTime(ws.last_object_time));

    counters.server_objects = Some(ws.objects.len());
    counters.server_objects_animated = Some(ws.objects.values().filter(|o| o.sim.animated).count());
    // The player's own state word reaches his body, which is the one physics body the per-object
    // loop above does not cover. Before the script drain, so a hide and its script land in wire
    // order.
    if let (Some(pid), true) = (ws.player_object, ws.character.is_some()) {
        if let Some(w) = stream.physics_state(pid) {
            if ws.character_state.0 != w {
                apply_character_state(ws, cfg, counters, pid, PhysicsState(w));
                counters.character_states_applied += 1;
            }
        }
    }
    // The house barrier's sparks: the barrier's script played on the mover, unless the player's
    // option disables it. A disabled or missing effect still leaves the barrier solid.
    let disabled = stream
        .world
        .player_system
        .options
        .disable_house_restriction_effects();
    for (mover, barrier, intensity) in std::mem::take(&mut ws.pending_restriction_effects) {
        if disabled {
            counters.restriction_effects_disabled += 1;
            continue;
        }
        let Some(script) = stream
            .world
            .weenie(barrier)
            .and_then(|w| w.pwd.pscript)
            .filter(|s| *s != 0)
        else {
            counters.restriction_effects_unplayed += 1;
            continue;
        };
        if ws.play_script_type(mover, u32::from(script), intensity) {
            counters.restriction_effects_played += 1;
        } else {
            counters.restriction_effects_unplayed += 1;
        }
    }
    // Portal-storm warnings play on the player's body, at intensity zero (brewing) or one
    // (imminent); with no body yet they play nothing.
    let storms = stream.world.take_portal_storm_scripts();
    let storm_player = stream.world.player;
    for intensity in storms {
        let played = storm_player.is_some_and(|p| {
            ws.play_script_type(
                p,
                dereth_client_model::portal_storm::PS_PORTAL_STORM,
                intensity,
            )
        });
        counters.script(played);
    }
    for e in stream.take_script_events() {
        let played = match e {
            crate::objects::ScriptEvent::Type {
                id,
                script_type,
                intensity,
            } => {
                // An int on the wire and an unsigned table key: a negative one names no row.
                #[allow(clippy::cast_sign_loss)]
                let script_type = script_type as u32;
                ws.play_script_type(id, script_type, intensity)
            }
            crate::objects::ScriptEvent::Id { id, script } => ws.play_script_id(id, script),
        };
        counters.script(played);
    }
    Ok(())
}

/// Hand a server movement buffer to the animation runtime: the whole of movement unpacking for a
/// remote object.
fn apply_movement(
    ws: &mut WorldState,
    counters: &mut ObjectCounters,
    id: ObjectId,
    buf: &dereth_protocol::movement::MovementBuffer,
    stream: &ObjectStream,
    is_the_player: bool,
) {
    let mut d = ObjectStepStats::default();
    let now = ws.last_object_time;
    crate::movement::apply_movement(
        &mut ws.objects,
        &mut ws.character,
        &mut d,
        now,
        id,
        buf,
        stream,
        is_the_player,
    );
    counters.fold_steps(d);
}

/// Apply a state word to an object: its no-draw bit to its parts, and its hidden bit's script and
/// its children's no-draw, when the configuration draws object state. A change of the hidden bit
/// always drops the object's link animations.
pub fn apply_object_state(
    ws: &mut WorldState,
    cfg: &SceneConfig,
    counters: &mut ObjectCounters,
    id: ObjectId,
    s: PhysicsState,
) {
    let draw = cfg.object_state_draw;
    let eff = {
        let Some(o) = ws.objects.get_mut(&id) else {
            return;
        };
        let eff = StateSideEffects::between(o.sim.state, s);
        o.sim.state = s;
        o.wire_state = s.0;
        if draw {
            if let Some(on) = eff.nodraw {
                o.sim
                    .driver
                    .borrow_mut()
                    .part_array
                    .set_no_draw_internal(on);
            }
        }
        eff
    };
    if draw {
        if let Some(on) = eff.nodraw {
            counters.nodraw(on);
        }
        if let Some(on) = eff.hidden {
            counters.object_hidden_changed += 1;
            play_hidden_script(ws, counters, id, on);
            force_child_nodraw(ws, counters, id, on);
        }
    }
    if eff.hidden.is_some() {
        if let Some(o) = ws.objects.get(&id) {
            o.sim.driver.borrow_mut().remove_link_animations();
        }
    }
}

/// Hiding plays script type 118, unhiding 117, on the object.
fn play_hidden_script(
    ws: &mut WorldState,
    counters: &mut ObjectCounters,
    id: ObjectId,
    hidden: bool,
) -> bool {
    let script_type = if hidden { 118 } else { 117 };
    let played = ws.play_script_type(id, script_type, 1.0);
    counters.script(played);
    played
}

/// Apply a state word to the local body: to its physics state (a hidden body neither collides nor
/// reports collisions) and, when the configuration draws object state, to its parts, its hidden
/// script and its children.
pub fn apply_character_state(
    ws: &mut WorldState,
    cfg: &SceneConfig,
    counters: &mut ObjectCounters,
    id: ObjectId,
    s: PhysicsState,
) {
    let eff = StateSideEffects::between(ws.character_state, s);
    let mut physical = s;
    if let Some(hidden) = eff.hidden {
        physical.set_ignores_collisions(hidden);
        physical.set_reports_collisions(!hidden);
    }
    ws.character_state = s;
    if let Some(c) = ws.character.as_mut() {
        if let Some(body) = c.world.get_mut(c.handle) {
            let _ = body.set_state(physical);
        }
    }
    if cfg.object_state_draw {
        if let Some(on) = eff.nodraw {
            if let Some(c) = ws.character.as_ref() {
                c.driver_mut().part_array.set_no_draw_internal(on);
            }
            counters.nodraw(on);
        }
        if let Some(on) = eff.hidden {
            counters.object_hidden_changed += 1;
            play_hidden_script(ws, counters, id, on);
            force_child_nodraw(ws, counters, id, on);
        }
    }
    if eff.hidden.is_some() {
        if let Some(c) = ws.character.as_ref() {
            c.driver_mut().remove_link_animations();
        }
    }
}

fn force_child_nodraw(
    ws: &mut WorldState,
    counters: &mut ObjectCounters,
    holder: ObjectId,
    on: bool,
) {
    let children: Vec<ObjectId> = ws
        .objects
        .iter()
        .filter(|(_, c)| c.sim.parent.is_some_and(|(h, _)| h == holder))
        .map(|(id, _)| *id)
        .collect();
    for id in children {
        set_child_nodraw(ws, counters, id, on);
    }
}

fn set_child_nodraw(ws: &mut WorldState, counters: &mut ObjectCounters, id: ObjectId, on: bool) {
    {
        let Some(c) = ws.objects.get_mut(&id) else {
            return;
        };
        c.sim.state.set_nodraw_bit(on);
        c.sim
            .driver
            .borrow_mut()
            .part_array
            .set_no_draw_internal(on);
    }
    counters.nodraw(on);
}

/// An object changing holder takes the new holder's hidden bit: no-draw forced on joining a hidden
/// holder, and cleared on leaving one for a visible holder or none.
pub fn reparent_nodraw(
    ws: &mut WorldState,
    cfg: &SceneConfig,
    counters: &mut ObjectCounters,
    id: ObjectId,
    before: Option<(ObjectId, u32)>,
    after: Option<(ObjectId, u32)>,
) {
    if !cfg.object_state_draw {
        return;
    }
    let hidden = |s: &WorldState, h: Option<(ObjectId, u32)>| {
        h.is_some_and(|(h, _)| s.objects.get(&h).is_some_and(|o| o.sim.state.is_hidden()))
    };
    let (left, joined) = (hidden(ws, before), hidden(ws, after));
    if left != joined {
        set_child_nodraw(ws, counters, id, joined);
    }
}
