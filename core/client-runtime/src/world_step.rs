//! One frame of the world's simulation, with no device: the physics tick, the re-centre, the body
//! and the objects placed, their hooks run, and the calendar advanced.
//!
//! A presentation that draws interleaves its own work between these calls, in the frame's order:
//!
//! 1. [`step_physics`]: object targets, object physics, the body's physics tick (or the bodyless
//!    gate), collision scripts, the chase camera's orbit, finishing object physics;
//! 2. [`recenter`]: the streaming window follows the viewer, and hands back its actions for the
//!    presentation's block queue;
//! 3. [`step_objects`]: the body placed in the render space the re-centre chose, the viewer
//!    cell, every object advanced and posed, held objects hung on their holders, hooks run;
//! 4. *(drawing: world lights, particles)*;
//! 5. [`advance_clock`], *(drawing: the sky)*, [`tick_schedule`] *(drawing: lighting and fog
//!    when it says so)*;
//! 6. *(drawing: the detail levels)*.
//!
//! [`update`] is those calls with nothing between them, for a presentation that draws nothing.
//!
//! [`step_physics`]: crate::world_step::step_physics
//! [`recenter`]: crate::world_step::recenter
//! [`step_objects`]: crate::world_step::step_objects
//! [`advance_clock`]: crate::world_step::advance_clock
//! [`tick_schedule`]: crate::world_step::tick_schedule
//! [`update`]: crate::world_step::update

use std::collections::BTreeSet;

use dereth_primitives::{DataId, Frame, LocalTime, ObjectId, Position, Vec3};

use crate::anim_hooks::{AttackStats, EtherealStats, HookSounds};
use crate::audio::SoundTrigger;
use crate::camera::CameraInput;
use crate::character::{CharacterInput, RenderSpace};
use crate::object_step::ObjectStepStats;
use crate::scene::SceneConfig;
use crate::world_state::WorldState;

/// Where a frame's simulation reports what it did. A drawing presentation keeps these counters
/// beside its own; [`StepCounters`] keeps them alone.
pub trait StepStatsSink {
    /// One object-step pass's counters.
    fn fold_steps(&mut self, d: ObjectStepStats);
    /// One `update`; `without_a_sweep` when the camera made no sweep since the previous one.
    fn updated(&mut self, without_a_sweep: bool);
    /// A collision's default script was played, or could not be.
    fn collision_script(&mut self, played: bool);
    /// A held object whose holder has no holding location at its parent location.
    fn held_without_holding_location(&mut self);
    /// How many held objects are drawn on their holders this frame.
    fn server_objects_held(&mut self, n: usize);
    /// The hook counters, which hook processing accumulates into in place.
    fn hook_stats(&mut self) -> (&mut EtherealStats, &mut AttackStats);
}

/// The counters on their own.
#[derive(Debug, Default, Clone)]
pub struct StepCounters {
    pub steps: ObjectStepStats,
    pub updates: u64,
    pub updates_without_a_sweep: u64,
    pub collision_scripts_played: u64,
    pub collision_scripts_unplayed: u64,
    pub held_without_holding_location: u64,
    pub server_objects_held: usize,
    pub ethereal: EtherealStats,
    pub attack: AttackStats,
}

impl StepStatsSink for StepCounters {
    fn fold_steps(&mut self, d: ObjectStepStats) {
        self.steps.remote_move_tos_failed += d.remote_move_tos_failed;
        if d.remote_move_tos_failed != 0 {
            self.steps.remote_last_move_to_error = d.remote_last_move_to_error;
        }
        self.steps.remote_target_updates += d.remote_target_updates;
        self.steps.remote_sticks_pulled += d.remote_sticks_pulled;
    }
    fn updated(&mut self, without_a_sweep: bool) {
        if without_a_sweep {
            self.updates_without_a_sweep += 1;
        }
        self.updates += 1;
    }
    fn collision_script(&mut self, played: bool) {
        if played {
            self.collision_scripts_played += 1;
        } else {
            self.collision_scripts_unplayed += 1;
        }
    }
    fn held_without_holding_location(&mut self) {
        self.held_without_holding_location += 1;
    }
    fn server_objects_held(&mut self, n: usize) {
        self.server_objects_held = n;
    }
    fn hook_stats(&mut self) -> (&mut EtherealStats, &mut AttackStats) {
        (&mut self.ethereal, &mut self.attack)
    }
}

/// The world's sound queue as the hook processor writes it.
#[derive(Debug)]
pub struct SoundStash<'a>(pub &'a mut Vec<SoundTrigger>);

impl HookSounds for SoundStash<'_> {
    fn wave(&mut self, id: DataId, at: Vec3, volume: f32, priority: f32, probability: f32) {
        self.0.push(SoundTrigger::Wave {
            id,
            at,
            volume,
            priority,
            probability,
        });
    }
    fn table(&mut self, table: DataId, stype: u32, at: Vec3) {
        self.0.push(SoundTrigger::Table { table, stype, at });
    }
}

/// A server position in the viewer-block-relative space the world is drawn in: the block's
/// offset from the window's centre (not the configured landblock, because the window moves as the
/// viewer walks) added to the block-local origin.
#[must_use]
pub fn render_frame_of(ws: &WorldState, fallback_landblock: u16, pos: Position) -> Frame {
    let viewer = ws
        .streamer
        .window
        .viewer_block()
        .unwrap_or_else(|| crate::landblock::block_xy(fallback_landblock));
    let block = pos.cell.landblock();
    let length = dereth_terrain::consts::BLOCK_LENGTH;
    #[allow(clippy::cast_precision_loss)] // a block index difference, at most 255
    let (dx, dy) = (
        (i32::from(block.x()) - viewer.0) as f32 * length,
        (i32::from(block.y()) - viewer.1) as f32 * length,
    );
    Frame::new(
        Vec3::new(
            pos.frame.origin.x + dx,
            pos.frame.origin.y + dy,
            pos.frame.origin.z,
        ),
        pos.frame.rotation,
    )
}

/// The frame's physics, up to the re-centre. Returns whether the physics gate opened (the body's
/// tick, or the bodyless gate), which [`step_objects`] needs.
///
/// `now` reaches physics unchanged: the 30 Hz gate is inside the physics update and is not a
/// frame-rate cap, so nothing here pre-filters or quantises it.
pub fn step_physics(
    ws: &mut WorldState,
    input: CameraInput,
    character: CharacterInput,
    now: LocalTime,
    dt: f32,
    stats: &mut impl StepStatsSink,
) -> bool {
    // `update` is not a frame: this says so in a number.
    let sweeps = ws.character.as_ref().map(|c| c.camera.stats.sweeps);
    stats.updated(sweeps.is_some() && sweeps == ws.sweeps_seen);
    ws.sweeps_seen = sweeps;
    // The host's half of the target manager's voyeur model, before the sweep: every sub-step
    // below handles targeting against the target resolved here, once.
    ws.resolve_object_targets(now);
    let mut d = ObjectStepStats::default();
    crate::object_step::prepare_object_physics(&mut ws.objects, &mut ws.character, &mut d, now);
    stats.fold_steps(d);
    let physics_ticked;
    if let Some(c) = ws.character.as_mut() {
        c.input = character;
        physics_ticked = c.update(now);
        // The impact effect, drained on the frame it was raised and before `step_objects` runs
        // the scripts, so an emitter an impact raises is stepped by this frame. Restriction
        // effects are parked: they need the barrier's script and the player's option word.
        let effects = c.take_restriction_effects();
        ws.pending_restriction_effects.extend(effects);
        let impacts = c.take_collision_scripts();
        for id in impacts {
            let played = ws.play_default_script(id);
            stats.collision_script(played);
        }
        // The chase camera's yaw offset is what the look keys move when there is a body; the
        // body's own heading is turned by the turn keys, through the motion table.
        let axis = |neg: bool, pos: bool| f32::from(u8::from(pos)) - f32::from(u8::from(neg));
        ws.camera_orbit.0 +=
            crate::camera::LOOK_SPEED * dt * axis(input.look_right, input.look_left);
        ws.camera_orbit.1 = (ws.camera_orbit.1
            + crate::camera::LOOK_SPEED * dt * axis(input.look_down, input.look_up))
        .clamp(-crate::camera::PITCH_LIMIT, crate::camera::PITCH_LIMIT);
        ws.follow_character();
    } else {
        // A bodyless scene has no physics world to answer the outer gate: the same timestamp and
        // backward-clock rules, kept locally. This gates only the fallback animation ladder.
        let elapsed = now.0 - ws.last_bodyless_tick;
        physics_ticked = if elapsed < 0.0 {
            ws.last_bodyless_tick = now.0;
            false
        } else if elapsed < dereth_physics::globals::MIN_QUANTUM {
            false
        } else {
            ws.last_bodyless_tick = now.0;
            true
        };
        ws.camera.update(input, dt);
    }
    let mut d = ObjectStepStats::default();
    crate::object_step::finish_object_physics(&mut ws.objects, &mut ws.character, &mut d);
    stats.fold_steps(d);
    physics_ticked
}

/// Re-centre the streaming window on the viewer. Returns the render space the frame's drawn
/// positions are written in and, only when the window actually scrolled, its actions for the
/// presentation's block queue (an empty list would release every resident block, which is why
/// "did not scroll" is `None`).
pub fn recenter(
    ws: &mut WorldState,
    cfg: &SceneConfig,
) -> (RenderSpace, Option<Vec<dereth_landscape::SlotAction>>) {
    let WorldState {
        streamer,
        character,
        camera,
        ..
    } = ws;
    let (_, space, actions) =
        streamer.recenter(character, camera, cfg.indoor_viewpoint_gate, cfg.landblock);
    (space, actions)
}

/// Everything after the re-centre that places a body or an object: the local body in `space`,
/// the viewer cell (an index into the viewer's own block, so after the re-centre), then every
/// object advanced and posed.
pub fn step_objects(
    ws: &mut WorldState,
    cfg: &SceneConfig,
    space: RenderSpace,
    now: LocalTime,
    physics_ticked: bool,
    stats: &mut impl StepStatsSink,
) {
    ws.place_local_body(space);
    ws.update_viewer_cell();
    advance_objects(ws, cfg, now, physics_ticked, stats);
}

/// Pose every remote object's parts and advance only the bodyless animation fallback: the motion
/// interpreter's time-stepping ladder with the position half removed, for objects with no
/// attached dynamic body (attached bodies advanced in the physics sweep). Then held objects are
/// hung on their holders and the hooks the step raised are run.
fn advance_objects(
    ws: &mut WorldState,
    cfg: &SceneConfig,
    now: LocalTime,
    physics_ticked: bool,
    stats: &mut impl StepStatsSink,
) {
    ws.last_object_time = now.0;
    let mut d = ObjectStepStats::default();
    crate::object_step::advance_object_ladder(
        &mut ws.objects,
        &mut ws.character,
        &mut d,
        now,
        physics_ticked,
    );
    stats.fold_steps(d);
    // Part placement reads the current animation frame; nothing interpolates. Done for every
    // object, moved or not, because a position event may have changed the frame without the clock
    // moving.
    let ids: Vec<ObjectId> = ws.objects.keys().copied().collect();
    for id in &ids {
        // Re-derived rather than stored: a window scroll moves the origin of this space.
        let Some(f) = ws
            .objects
            .get(id)
            .and_then(|o| o.sim.position)
            .map(|q| render_frame_of(ws, cfg.landblock, q))
        else {
            continue;
        };
        // The same frame reaches the body: drawing and object collision both read the part
        // array, and without this every solid object would collide at its setup's placement
        // frame for its whole life.
        let frames = if let Some(o) = ws.objects.get_mut(id) {
            o.frame = f;
            let mut driver = o.sim.driver.borrow_mut();
            driver.update_parts(&f);
            driver
                .sequence
                .get_curr_animframe()
                .map(|af| std::sync::Arc::new(af.frames.clone()))
        } else {
            continue;
        };
        if let Some(body) = ws
            .character
            .as_mut()
            .and_then(|c| c.world.by_object_id(*id).and_then(|h| c.world.get_mut(h)))
        {
            body.part_frames = frames;
        }
    }
    place_held_objects(ws, &ids, stats);
    // Hook processing runs at the end of the step that raised them.
    process_hooks(ws, stats);
}

/// Put every held object on its holder. Setting a frame in the client updates the children, so
/// a moving parent places them on the way out; here the holders were just placed, so the
/// children are swept afterwards, repeatedly, because a child may itself hold something and the
/// map is in id order. The pass that places nothing new ends it.
///
/// The player is a holder with no object of his own here, so his part frames are read off the
/// body: that is the wielded weapon a person actually looks at.
fn place_held_objects(ws: &mut WorldState, ids: &[ObjectId], stats: &mut impl StepStatsSink) {
    // `settled`: this object's frame is final for this call; `drawable`: the subset a child may
    // hang off. They differ for an object whose holder refused the attachment.
    let mut settled: BTreeSet<ObjectId> = BTreeSet::new();
    let mut drawable: BTreeSet<ObjectId> = BTreeSet::new();
    for (id, o) in &mut ws.objects {
        if o.sim.parent.is_none() {
            settled.insert(*id);
            drawable.insert(*id);
            // An object with neither a holder nor a position is in a container: out of every cell
            // list, not drawn and not pickable. An assignment, so an object given a position back
            // is drawn again.
            o.drawn = o.sim.position.is_some();
        } else {
            // Re-derived every frame: the holder may have gone away since the last one.
            o.drawn = false;
        }
    }
    let player = ws.player_object_id();
    loop {
        let mut progress = false;
        for id in ids {
            if settled.contains(id) {
                continue;
            }
            let Some((holder, location)) = ws.objects.get(id).and_then(|o| o.sim.parent) else {
                continue;
            };
            // The holder's part frames must be current before the child can hang off them. With
            // no local body the player has no part array, and nothing can hang off him: the
            // object stays unsettled and undrawn.
            let from_player = player == Some(holder) && ws.character.is_some();
            if !from_player && !drawable.contains(&holder) {
                if settled.contains(&holder) {
                    settled.insert(*id);
                    progress = true;
                }
                continue;
            }
            match ws.held_object_frame(holder, location, from_player) {
                Some(f) => {
                    if let Some(o) = ws.objects.get_mut(id) {
                        o.frame = f;
                        o.drawn = true;
                        o.sim.driver.borrow_mut().update_parts(&f);
                    }
                    drawable.insert(*id);
                }
                // No holding location at that parent location on the holder's setup: the
                // attachment is refused outright, so the client does not draw it on the holder.
                None => stats.held_without_holding_location(),
            }
            settled.insert(*id);
            progress = true;
        }
        if !progress {
            break;
        }
    }
    stats.server_objects_held(
        ws.objects
            .values()
            .filter(|o| o.sim.parent.is_some() && o.drawn)
            .count(),
    );
}

/// Run the animation hooks the drivers queued: sounds into the world's queue, the rest applied.
pub fn process_hooks(ws: &mut WorldState, stats: &mut impl StepStatsSink) {
    let WorldState {
        objects,
        character,
        pending_sound,
        character_sound_table,
        player_object,
        ..
    } = ws;
    let (ethereal, attack) = stats.hook_stats();
    crate::anim_hooks::process_hooks(
        objects,
        character,
        &mut SoundStash(pending_sound),
        *character_sound_table,
        *player_object,
        ethereal,
        attack,
    );
}

/// Advance the calendar to `now`.
pub fn advance_clock(ws: &mut WorldState, now: f64) {
    ws.clock.use_time(now);
}

/// The environment tick schedule. `None` before the next tick is due; otherwise the next tick
/// (and, when it is due too, the next light tick) is scheduled and `Some(light)` says whether the
/// light tick came due. The region overrides the periods; a region with no sky info uses 3 s and
/// 20 s.
pub fn tick_schedule(
    ws: &mut WorldState,
    region: &dereth_assets::Region,
    now: f64,
) -> Option<bool> {
    if now < ws.next_tick {
        return None;
    }
    let (tick, light_tick) = region
        .sky_info
        .as_ref()
        .map_or((3.0, 20.0), |s| (s.tick_size, s.light_tick_size));
    ws.next_tick = now + tick;
    if now > ws.next_light_tick {
        ws.next_light_tick = now + light_tick;
        Some(true)
    } else {
        Some(false)
    }
}

/// One whole frame of the world with no device, in the order a drawing presentation runs it
/// with its drawing left out. The window's actions go to `residency`, which builds the blocks
/// they ask for on the next [`crate::world_build::BlockResidency::stream`].
#[allow(clippy::too_many_arguments)]
// LINT-OK: the frame's inputs are the ones a drawing presentation's own update takes, plus the
// residency it streams into.
pub fn update(
    ws: &mut WorldState,
    residency: &mut crate::world_build::BlockResidency,
    cfg: &SceneConfig,
    input: CameraInput,
    character: CharacterInput,
    now: LocalTime,
    dt: f32,
    stats: &mut impl StepStatsSink,
) {
    let physics_ticked = step_physics(ws, input, character, now, dt, stats);
    let (space, actions) = recenter(ws, cfg);
    if let Some(actions) = actions {
        residency.queue(ws, cfg, &actions);
    }
    step_objects(ws, cfg, space, now, physics_ticked, stats);
    advance_clock(ws, now.0);
    let _ = tick_schedule(ws, residency.region(), now.0);
}

/// The tail of the camera update: the body fades as the chase camera closes on it, and the
/// translucency reaches the whole of the player's hierarchy (a wielded weapon fades with the hand
/// holding it).
///
/// The latch is load-bearing: the client issues the zero only on the transition, and applying
/// translucency clears the no-draw bit on every call that is not exactly 1.0, so issuing zero
/// every frame would un-hide whatever an animation's no-draw hook had hidden.
pub fn apply_camera_translucency(ws: &mut WorldState, cfg: &SceneConfig) {
    if !cfg.material_translucency {
        return;
    }
    let Some(c) = ws.character.as_ref() else {
        return;
    };
    let t = c.camera.player_translucency;
    if t == 0.0 && ws.camera_translucency == 0.0 {
        return;
    }
    ws.camera_translucency = t;
    // Each object's value is clamped up to its own original translucency first, and that floor is
    // 0 for every object here (nothing writes it), so the value is applied as it is. The
    // translucency-locked state bit is never set.
    c.driver_mut()
        .part_array
        .set_translucency_internal(t, false);
    let player = ws.player_object;
    for o in ws.objects.values_mut() {
        if o.sim
            .parent
            .is_none_or(|(holder, _)| Some(holder) != player)
        {
            continue;
        }
        o.sim
            .driver
            .borrow_mut()
            .part_array
            .set_translucency_internal(t, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With no device, a body in Holtburg walks: two seconds of frames with forward held move it
    /// north of where it stood, and it stays on the ground of its landblock window.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_body_walks_forward_with_no_device() {
        let store = std::sync::Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {}",
                dereth_dat::testing::dat_dir().display()
            )
        }));
        let cfg = SceneConfig {
            landblock: 0xA9B4,
            character: true,
            ..SceneConfig::default()
        };
        let (mut ws, mut residency) =
            crate::world_build::load(&store, &cfg).expect("the world loads");
        let mut stats = StepCounters::default();
        let start = ws.character.as_ref().expect("a body").position();
        let step: f64 = 1.0 / 30.0;
        let dt: f32 = 1.0 / 30.0;
        let forward = CharacterInput {
            forward: true,
            ..CharacterInput::default()
        };
        let mut now = 1.0;
        for _ in 0..60 {
            now += step;
            update(
                &mut ws,
                &mut residency,
                &cfg,
                CameraInput::default(),
                forward,
                LocalTime(now),
                dt,
                &mut stats,
            );
            residency.stream(&mut ws, &store, &cfg);
        }
        let end = ws.character.as_ref().expect("a body").position();
        let (dx, dy) = (
            end.frame.origin.x - start.frame.origin.x,
            end.frame.origin.y - start.frame.origin.y,
        );
        let moved = dereth_primitives::num::math::hypotf(dx, dy);
        assert!(
            moved > 2.0,
            "the body moved {moved} m from {start:?} to {end:?}"
        );
        assert_eq!(stats.updates, 60);
    }
}
