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
    render_frame_in(viewer_block(ws, fallback_landblock), pos)
}

/// The block the render space is relative to: the window's centre, or the configured landblock
/// before the window has one.
fn viewer_block(ws: &WorldState, fallback_landblock: u16) -> (i32, i32) {
    ws.streamer
        .window
        .viewer_block()
        .unwrap_or_else(|| dereth_world_data::landblock::block_xy(fallback_landblock))
}

/// [`render_frame_of`] in the render space relative to `viewer`.
fn render_frame_in(viewer: (i32, i32), pos: Position) -> Frame {
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
    if ws.smooth_movement {
        note_before_tick(ws);
    }
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
        } else if !dereth_physics::globals::tick_is_due(elapsed) {
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

/// Where every object stands as the frame's physics tick, if it has one, begins, for drawing it
/// between ticks: where its body is, for an object physics steps, and otherwise where it is.
fn note_before_tick(ws: &mut WorldState) {
    let WorldState {
        objects, character, ..
    } = ws;
    for o in objects.values_mut() {
        let body = o
            .sim
            .physics_handle
            .and_then(|h| character.as_ref().and_then(|c| c.world.get(h)));
        let here = match body {
            Some(b) => b.cell.map(|_| b.position),
            None => o.sim.position,
        };
        if let Some(here) = here {
            o.between.before_tick(here);
        }
    }
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
    let (shift, space, actions) =
        streamer.recenter(character, camera, cfg.indoor_viewpoint_gate, cfg.landblock);
    if shift != (0, 0) {
        rebase_render_space_particles(ws, shift);
    }
    (space, actions)
}

/// The window moved `(dx, dy)` landblocks, so every point fixed in the world now has render
/// coordinates `192 * (dx, dy)` smaller. The server objects' and the body's particles are
/// simulated in that space (their emitters follow the placed parts), and a particle keeps its
/// birth frame for life, so they are moved with it; otherwise every live particle, and every
/// particle a far emitter holds frozen, would be drawn a whole landblock shift away from where
/// it is in the world. The landblock statics' particles are simulated in absolute coordinates and
/// need nothing.
pub fn rebase_render_space_particles(ws: &mut WorldState, (dx, dy): (i32, i32)) {
    let length = dereth_terrain::consts::BLOCK_LENGTH;
    #[allow(clippy::cast_precision_loss)] // a block shift, at most 255
    let by = dereth_primitives::Vec3::new(-(dx as f32) * length, -(dy as f32) * length, 0.0);
    for o in ws.objects.values() {
        o.sim.driver.borrow_mut().particles.translate(by);
    }
    if let Some(c) = ws.character.as_ref() {
        c.driver_mut().particles.translate(by);
    }
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

/// The furthest on, in seconds, a body drawn between its animation's keyframes is drawn from where
/// its animation was last advanced: three physics ticks. Between ticks a body is drawn up to one
/// tick on; one whose animation is not being advanced (held still, or the world held while a
/// portal is crossed) is drawn this far on, and no further, until it is advanced again.
pub const MOST_AHEAD: f64 = 3.0 * dereth_physics::globals::MIN_QUANTUM;

/// How far on from where its animation was last advanced, `since` seconds ago, a body drawn
/// between keyframes is drawn: that long, up to [`MOST_AHEAD`], and never back.
#[must_use]
pub fn drawn_ahead(since: f64) -> f64 {
    if since.is_nan() {
        return 0.0;
    }
    since.clamp(0.0, MOST_AHEAD)
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
    // Part placement reads the current animation frame; nothing interpolates, unless the bodies
    // are drawn between keyframes or between ticks. Done for every object, moved or not, because
    // a position event may have changed the frame without the clock moving.
    let ids: Vec<ObjectId> = ws.objects.keys().copied().collect();
    let viewer = viewer_block(ws, cfg.landblock);
    for id in &ids {
        // Re-derived rather than stored: a window scroll moves the origin of this space.
        let Some(here) = ws.objects.get(id).and_then(|o| o.sim.position) else {
            continue;
        };
        let mut f = render_frame_in(viewer, here);
        // Drawn between physics ticks, it is drawn on its way between them instead.
        if ws.smooth_movement {
            let placements = placements_of(ws, *id);
            if let Some(o) = ws.objects.get_mut(id) {
                f = drawn_between_ticks(
                    &mut o.between,
                    f,
                    now,
                    physics_ticked,
                    here,
                    placements,
                    viewer,
                );
            }
        }
        // Drawn between keyframes, how long since the object's animation was last advanced: by
        // physics for an object with a body, by the fallback ladder for one without.
        let ahead = ws
            .smooth_animation
            .then(|| advanced_at(ws, *id).map_or(0.0, |at| drawn_ahead(now.0 - at)));
        // The same frame reaches the body: drawing and object collision both read the part
        // array, and without this every solid object would collide at its setup's placement
        // frame for its whole life. What it collides with is the keyframe's, drawn between
        // keyframes or not.
        let frames = if let Some(o) = ws.objects.get_mut(id) {
            o.frame = f;
            let mut driver = o.sim.driver.borrow_mut();
            match ahead {
                Some(ahead) => driver.update_parts_between(&f, ahead),
                None => driver.update_parts(&f),
            }
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

/// How many times physics has placed object `id`'s body rather than moved it; zero for an object
/// without one.
fn placements_of(ws: &WorldState, id: ObjectId) -> u32 {
    ws.objects
        .get(&id)
        .and_then(|o| o.sim.physics_handle)
        .and_then(|h| ws.character.as_ref()?.world.get(h))
        .map_or(0, |b| b.placements)
}

/// The frame an object is drawn at between its physics ticks: `f`, where physics has it (`here`)
/// in the render space relative to `viewer`, moved and turned by its way between ticks, after one
/// frame of its drawing at `now` (`crate::between_ticks::BetweenTicks::frame`).
fn drawn_between_ticks(
    between: &mut crate::between_ticks::BetweenTicks,
    mut f: Frame,
    now: LocalTime,
    ticked: bool,
    here: Position,
    placements: u32,
    viewer: (i32, i32),
) -> Frame {
    let space = |p: Position| render_frame_in(viewer, p).origin;
    between.frame(now.0, ticked, here, placements, true, space);
    let offset = between.offset(here, true, space);
    f.origin = Vec3::new(
        f.origin.x + offset.x,
        f.origin.y + offset.y,
        f.origin.z + offset.z,
    );
    let turn = between.turn(here, true);
    if turn != 0.0 {
        let heading = dereth_primitives::frame::get_heading(&f);
        dereth_primitives::frame::set_heading(&mut f, heading + turn);
    }
    f
}

/// When object `id`'s animation was last advanced: by physics, for an object with a body; by the
/// fallback ladder, for one without.
fn advanced_at(ws: &WorldState, id: ObjectId) -> Option<f64> {
    let o = ws.objects.get(&id)?;
    match o.sim.physics_handle {
        Some(h) => Some(ws.character.as_ref()?.world.get(h)?.update_time()),
        None => Some(o.sim.update_time),
    }
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

    #[test]
    fn a_body_drawn_between_keyframes_is_drawn_on_up_to_three_ticks_and_never_back() {
        assert_eq!(drawn_ahead(-0.5), 0.0, "a clock behind the animation's");
        assert_eq!(drawn_ahead(0.01), 0.01);
        assert_eq!(
            drawn_ahead(10.0),
            MOST_AHEAD,
            "an animation not advanced lately"
        );
        assert_eq!(drawn_ahead(f64::NAN), 0.0);
        assert!((MOST_AHEAD - 0.1).abs() < 1e-12);
    }

    /// A drudge, the creature the stations below set fighting, and the motions the server gives
    /// one.
    const DRUDGE: u32 = 0x0200_07DD;
    const DRUDGE_MOTIONS: u32 = 0x0900_0008;

    /// One frame of the stations below: the body's place, its animation (the animation playing
    /// and the frame it stands at), its parts as drawn, in the body's own frame, and whether each
    /// is drawn exactly where its animation's keyframe puts it; the same for the drudge, with the
    /// part frames its body collides with; and the attack hooks that have reached physics so far.
    #[derive(Debug, Clone, PartialEq)]
    struct Shot {
        body: Position,
        body_pose: (u32, f64),
        body_parts: Vec<Vec3>,
        body_at_keyframe: bool,
        drudge: Option<Position>,
        drudge_pose: (u32, f64),
        drudge_parts: Vec<Vec3>,
        drudge_at_keyframe: bool,
        drudge_collides: Option<Vec<Frame>>,
        attacks: u64,
    }

    /// Whether every part of `driver` is drawn exactly where the keyframe its animation stands at
    /// puts it, against `root`.
    fn at_keyframe(driver: &dereth_animation::MotionDriver, root: &Frame) -> bool {
        use dereth_primitives::frame::V3 as _;
        let Some(af) = driver.sequence.get_curr_animframe() else {
            return true;
        };
        let scale = driver.part_array.scale;
        driver
            .part_array
            .parts
            .iter()
            .zip(&af.frames)
            .all(|(p, f)| {
                let scaled = Frame::new(f.origin.mul_componentwise(scale), f.rotation);
                p.pos == dereth_primitives::frame::combine(root, &scaled)
            })
    }

    /// The parts of `driver`, placed against `root`, as offsets in `root`'s own frame.
    fn parts_in(driver: &dereth_animation::MotionDriver, root: &Frame) -> Vec<Vec3> {
        driver
            .part_array
            .parts
            .iter()
            .map(|p| {
                let d = Vec3::new(
                    p.pos.origin.x - root.origin.x,
                    p.pos.origin.y - root.origin.y,
                    p.pos.origin.z - root.origin.z,
                );
                dereth_primitives::frame::globaltolocalvec(
                    dereth_primitives::frame::l2g(root.rotation),
                    d,
                )
            })
            .collect()
    }

    fn pose(driver: &dereth_animation::MotionDriver) -> (u32, f64) {
        let s = &driver.sequence;
        s.curr()
            .map_or((0, 0.0), |i| (s.nodes()[i].anim_id.0, s.frame_number()))
    }

    /// Two seconds at `fps` of a body running north through Holtburg, a drudge beside its way
    /// fighting (into its combat stance, then a swing begun every second), each frame run as the
    /// drawn world runs it, with the bodies drawn between keyframes (`smooth`) or at them.
    fn running_past_a_fighting_drudge(fps: f64, smooth: bool) -> Vec<Shot> {
        use dereth_animation::motion::MovementParameters;
        use dereth_animation::MotionCommand;
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
        ws.smooth_animation = smooth;
        let mut stats = StepCounters::default();
        let step = 1.0 / fps;
        #[allow(clippy::cast_possible_truncation)]
        let dt = step as f32;
        let mut now = 1.0;
        let mut frame = |ws: &mut WorldState,
                         residency: &mut crate::world_build::BlockResidency,
                         stats: &mut StepCounters,
                         input: CharacterInput| {
            now += step;
            update(
                ws,
                residency,
                &cfg,
                CameraInput::default(),
                input,
                LocalTime(now),
                dt,
                stats,
            );
            now
        };
        let mut settled = 0.0;
        for _ in 0..30 {
            settled = frame(
                &mut ws,
                &mut residency,
                &mut stats,
                CharacterInput::default(),
            );
            residency.stream(&mut ws, &store, &cfg);
        }
        // The drudge, three metres east of the body, made as the server makes a creature.
        let at = ws.character.as_ref().expect("a body").position();
        let id = ObjectId(0x8300_0100);
        let mut stream = crate::objects::ObjectStream::new();
        let payload = dereth_protocol::objects::ObjectCreatePayload {
            id,
            physicsdesc: dereth_protocol::types::PhysicsDesc {
                bitfield: dereth_protocol::types::physicsdesc::flags::POSITION
                    | dereth_protocol::types::physicsdesc::flags::SETUP
                    | dereth_protocol::types::physicsdesc::flags::MTABLE,
                setup_id: Some(DRUDGE),
                mtable_id: Some(DRUDGE_MOTIONS),
                position: Some(dereth_protocol::types::PositionWire {
                    objcell_id: at.cell.0,
                    frame: dereth_protocol::types::Frame {
                        origin: Vec3::new(
                            at.frame.origin.x + 3.0,
                            at.frame.origin.y + 6.0,
                            at.frame.origin.z,
                        )
                        .into(),
                        orientation: dereth_primitives::Quat::IDENTITY.into(),
                    },
                }),
                timestamps: dereth_protocol::types::PhysicsTimestamps {
                    instance: 1,
                    ..dereth_protocol::types::PhysicsTimestamps::default()
                },
                ..dereth_protocol::types::PhysicsDesc::default()
            },
            ..Default::default()
        };
        let body =
            dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload))
                .expect("encode");
        stream.apply_event(
            &dereth_client_net::client_session::SessionEvent::WorldObject {
                opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
                body,
            },
            LocalTime(settled),
        );
        stream.world.update_visible_object_list();
        let mut counters = crate::world_objects::ObjectCounters::default();
        crate::world_objects::sync_objects(
            &mut ws,
            &store,
            &cfg,
            &mut stream,
            &mut counters,
            &mut crate::world_objects::NoAppearance,
        )
        .expect("the drudge is made");
        // Its body, as the client gives a creature one.
        stream.sync_physics(&store, &mut ws.character.as_mut().expect("a body").world);
        let params = MovementParameters::default();
        let drudge = |ws: &WorldState| ws.objects.get(&id).map(|o| o.sim.driver.clone());
        drudge(&ws)
            .expect("the drudge")
            .borrow_mut()
            .do_interpreted_motion(MotionCommand::HAND_COMBAT, &params);
        for _ in 0..30 {
            frame(
                &mut ws,
                &mut residency,
                &mut stats,
                CharacterInput::default(),
            );
        }
        let run = CharacterInput {
            forward: true,
            run: true,
            ..CharacterInput::default()
        };
        // Up to speed.
        for _ in 0..60 {
            frame(&mut ws, &mut residency, &mut stats, run);
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = (2.0 * fps) as usize;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let every = fps as usize;
        (0..n)
            .map(|i| {
                if i % every == 0 {
                    drudge(&ws)
                        .expect("the drudge")
                        .borrow_mut()
                        .do_interpreted_motion(MotionCommand::ATTACK_HIGH1, &params);
                }
                frame(&mut ws, &mut residency, &mut stats, run);
                residency.stream(&mut ws, &store, &cfg);
                let c = ws.character.as_ref().expect("a body");
                let root = c.drawn_frame();
                let o = ws.objects.get(&id).expect("the drudge");
                let d = o.sim.driver.borrow();
                let collides = o.sim.physics_handle.and_then(|h| {
                    c.world
                        .get(h)
                        .and_then(|b| b.part_frames.as_deref().cloned())
                });
                Shot {
                    body: c.position(),
                    body_pose: pose(&c.driver()),
                    body_parts: parts_in(&c.driver(), &root),
                    body_at_keyframe: at_keyframe(&c.driver(), &root),
                    drudge: o.sim.position,
                    drudge_pose: pose(&d),
                    drudge_parts: parts_in(&d, &o.frame),
                    drudge_at_keyframe: at_keyframe(&d, &o.frame),
                    drudge_collides: collides,
                    attacks: stats.attack.hooks,
                }
            })
            .collect()
    }

    /// Each frame, the furthest any part of a body moves within the body against the frame
    /// before.
    fn part_steps(shots: &[Shot], parts: impl Fn(&Shot) -> &Vec<Vec3>) -> Vec<f32> {
        shots
            .windows(2)
            .map(|w| {
                parts(&w[0])
                    .iter()
                    .zip(parts(&w[1]))
                    .map(|(a, b)| {
                        let d = Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z);
                        d.magnitude()
                    })
                    .fold(0.0, f32::max)
            })
            .collect()
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn drawn_between_keyframes_a_running_body_and_a_fighting_drudge_move_a_little_every_frame() {
        for fps in [240.0, 60.0] {
            let at = running_past_a_fighting_drudge(fps, false);
            let between = running_past_a_fighting_drudge(fps, true);
            for (who, parts) in [
                (
                    "the body",
                    (|s: &Shot| &s.body_parts) as fn(&Shot) -> &Vec<Vec3>,
                ),
                ("the drudge", |s: &Shot| &s.drudge_parts),
            ] {
                let (stepped, smooth) = (part_steps(&at, parts), part_steps(&between, parts));
                let still = |steps: &[f32]| steps.iter().filter(|s| **s < 1e-6).count();
                let most = |steps: &[f32]| steps.iter().copied().fold(0.0, f32::max);
                // At keyframes a part stands still between them and then jumps.
                assert!(
                    still(&stepped) * 3 > stepped.len(),
                    "{fps} fps, {who}: drawn at keyframes it stands still between them: \
                     {} of {}",
                    still(&stepped),
                    stepped.len()
                );
                // Between them it moves every frame, by no more than half a keyframe's step:
                // exactly half at 60 fps, where the world steps every second frame.
                assert_eq!(
                    still(&smooth),
                    0,
                    "{fps} fps, {who}: drawn between keyframes it moves every frame"
                );
                assert!(
                    most(&smooth) <= most(&stepped) * 0.5 * 1.001,
                    "{fps} fps, {who}: its largest step {} against {} at keyframes",
                    most(&smooth),
                    most(&stepped)
                );
            }
        }
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn drawn_between_keyframes_or_at_them_the_bodies_move_animate_fight_and_collide_alike() {
        let at = running_past_a_fighting_drudge(240.0, false);
        let between = running_past_a_fighting_drudge(240.0, true);
        assert!(
            at.last()
                .is_some_and(|s| s.attacks > 0 && s.drudge_collides.is_some()),
            "the drudge has a body and its swings reach physics"
        );
        for (n, (a, b)) in at.iter().zip(&between).enumerate() {
            assert_eq!(
                (
                    &a.body,
                    a.body_pose,
                    &a.drudge,
                    a.drudge_pose,
                    &a.drudge_collides,
                    a.attacks
                ),
                (
                    &b.body,
                    b.body_pose,
                    &b.drudge,
                    b.drudge_pose,
                    &b.drudge_collides,
                    b.attacks
                ),
                "frame {n}: only the drawing differs"
            );
        }
        // At keyframes, every part is exactly where its keyframe puts it, every frame; between
        // them, the drawing does differ.
        assert!(at
            .iter()
            .all(|s| s.body_at_keyframe && s.drudge_at_keyframe));
        assert!(between
            .iter()
            .any(|s| !s.body_at_keyframe && !s.drudge_at_keyframe));
    }

    /// A remote player, made as the server makes one, and the motions the server gives one.
    const HUMAN: u32 = 0x0200_0001;
    const HUMAN_MOTIONS: u32 = 0x0900_0001;
    /// An arrow's setup, flown as a spell bolt flies: no motion table, a velocity, the missile
    /// state word and no gravity.
    const ARROW: u32 = 0x0200_0124;
    const BOLT_STATE: u32 = 0x0002_8B48;
    /// A creature's state word: solid, and falling to the ground.
    const CREATURE_STATE: u32 = 0x0000_0408;

    /// The three movers of the stations below: a player running past the body, a drudge running
    /// after him, and an arrow flying the same way.
    const RUNNER: ObjectId = ObjectId(0x8300_0110);
    const CHASER: ObjectId = ObjectId(0x8300_0111);
    const BOLT: ObjectId = ObjectId(0x8300_0112);
    const MOVERS: [ObjectId; 3] = [RUNNER, CHASER, BOLT];

    /// One mover on one frame: where physics has it, as a position and in the space the world is
    /// drawn in; where it is drawn; where its parts are drawn, and whether they are exactly where
    /// its animation's keyframe puts them against where it is drawn; and the part frames its body
    /// collides with.
    #[derive(Debug, Clone, PartialEq)]
    struct Mover {
        physics: Position,
        at: Frame,
        drawn: Frame,
        parts: Vec<Frame>,
        at_keyframes: bool,
        collides: Option<Vec<Frame>>,
    }

    /// One frame of the stations below: each mover, in [`MOVERS`] order; where the body is; and
    /// how many physics ticks have run.
    #[derive(Debug, Clone, PartialEq)]
    struct Movers {
        movers: Vec<Option<Mover>>,
        body: Position,
        ticks: u64,
    }

    #[allow(clippy::too_many_arguments)] // one parameter per field of the create
    fn create(
        stream: &mut crate::objects::ObjectStream,
        id: ObjectId,
        setup: u32,
        mtable: Option<u32>,
        at: Position,
        velocity: Option<Vec3>,
        state: u32,
        now: f64,
    ) {
        use dereth_protocol::types::physicsdesc::flags;
        let mut bitfield = flags::POSITION | flags::SETUP;
        if mtable.is_some() {
            bitfield |= flags::MTABLE;
        }
        if velocity.is_some() {
            bitfield |= flags::VELOCITY;
        }
        let payload = dereth_protocol::objects::ObjectCreatePayload {
            id,
            physicsdesc: dereth_protocol::types::PhysicsDesc {
                bitfield,
                state,
                setup_id: Some(setup),
                mtable_id: mtable,
                velocity: velocity.map(Into::into),
                position: Some(dereth_protocol::types::PositionWire {
                    objcell_id: at.cell.0,
                    frame: dereth_protocol::types::Frame {
                        origin: at.frame.origin.into(),
                        orientation: at.frame.rotation.into(),
                    },
                }),
                timestamps: dereth_protocol::types::PhysicsTimestamps {
                    instance: 1,
                    ..dereth_protocol::types::PhysicsTimestamps::default()
                },
                ..dereth_protocol::types::PhysicsDesc::default()
            },
            ..Default::default()
        };
        let body =
            dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload))
                .expect("encode");
        stream.apply_event(
            &dereth_client_net::client_session::SessionEvent::WorldObject {
                opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
                body,
            },
            LocalTime(now),
        );
        stream.world.update_visible_object_list();
    }

    /// How the server moves the player in the stations below: teleports him three metres to his
    /// right, or corrects where he is by a short way to his right.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ServerMove {
        Teleport,
        Correct,
    }

    impl ServerMove {
        const fn metres(self) -> f32 {
            match self {
                Self::Teleport => 3.0,
                Self::Correct => 0.8,
            }
        }
    }

    /// The server moving `id` to `to`: a `Movement_PositionEvent` (`0xF748`) with a newer position
    /// stamp, and a newer teleport stamp when it is a teleport.
    fn server_moves(
        stream: &mut crate::objects::ObjectStream,
        id: ObjectId,
        to: Position,
        how: ServerMove,
        now: f64,
    ) {
        use dereth_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};
        let body = dereth_protocol::write_body(&MovementPositionEvent {
            id,
            position: PositionPack {
                flags: position_flags::IS_GROUNDED,
                origin: dereth_protocol::types::Origin {
                    objcell_id: to.cell.0,
                    origin: to.frame.origin.into(),
                },
                orientation: to.frame.rotation.into(),
                instance_timestamp: 1,
                position_timestamp: 1,
                teleport_timestamp: u16::from(how == ServerMove::Teleport),
                ..PositionPack::default()
            },
        })
        .expect("encode");
        stream.apply_event(
            &dereth_client_net::client_session::SessionEvent::WorldObject {
                opcode: dereth_protocol::Opcode::MOVEMENT_POSITION_EVENT,
                body,
            },
            LocalTime(now),
        );
    }

    /// Two seconds at `fps` of a player running past a body standing in Holtburg, a drudge running
    /// after him and turning as it runs, and an arrow flying the same way, each frame run as the
    /// client runs it (the server's objects synchronised, then the world stepped), with the
    /// movers drawn between their physics ticks (`smooth`) or where physics has them. With
    /// `server`, the server moves the player to his right on that frame.
    fn movers(fps: f64, smooth: bool, server: Option<(usize, ServerMove)>) -> Vec<Movers> {
        use dereth_animation::motion::interp::InterpretedMotionState;
        use dereth_animation::MotionCommand;
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
        ws.smooth_movement = smooth;
        let mut stats = StepCounters::default();
        let mut stream = crate::objects::ObjectStream::new();
        let mut counters = crate::world_objects::ObjectCounters::default();
        let step = 1.0 / fps;
        #[allow(clippy::cast_possible_truncation)]
        let dt = step as f32;
        let mut now = 1.0;
        // One frame, in the client's order: the server's objects, their bodies, the world.
        let mut frame = |ws: &mut WorldState,
                         residency: &mut crate::world_build::BlockResidency,
                         stream: &mut crate::objects::ObjectStream| {
            now += step;
            crate::world_objects::sync_objects(
                ws,
                &store,
                &cfg,
                stream,
                &mut counters,
                &mut crate::world_objects::NoAppearance,
            )
            .expect("the objects synchronise");
            if let Some(c) = ws.character.as_mut() {
                stream.sync_physics_at(&store, &mut c.world, LocalTime(now));
            }
            update(
                ws,
                residency,
                &cfg,
                CameraInput::default(),
                CharacterInput::default(),
                LocalTime(now),
                dt,
                &mut stats,
            );
            residency.stream(ws, &store, &cfg);
            now
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let half = (fps / 2.0) as usize;
        let mut settled = 0.0;
        for _ in 0..half {
            settled = frame(&mut ws, &mut residency, &mut stream);
        }
        // Beside the body and facing its way: the player three metres to its right, the drudge
        // six behind him, the arrow ten behind and above.
        let at = ws.character.as_ref().expect("a body").position();
        let axes = dereth_primitives::frame::l2g(at.frame.rotation);
        let along = |right: f32, ahead: f32, up: f32| {
            let v = dereth_primitives::frame::localtoglobalvec(axes, Vec3::new(right, ahead, up));
            let o = at.frame.origin;
            Position::new(
                at.cell,
                Frame::new(
                    Vec3::new(o.x + v.x, o.y + v.y, o.z + v.z),
                    at.frame.rotation,
                ),
            )
        };
        let forward = dereth_primitives::frame::localtoglobalvec(axes, Vec3::new(0.0, 15.0, 0.0));
        create(
            &mut stream,
            RUNNER,
            HUMAN,
            Some(HUMAN_MOTIONS),
            along(3.0, 0.0, 0.0),
            None,
            CREATURE_STATE,
            settled,
        );
        create(
            &mut stream,
            CHASER,
            DRUDGE,
            Some(DRUDGE_MOTIONS),
            along(3.0, -6.0, 0.0),
            None,
            CREATURE_STATE,
            settled,
        );
        create(
            &mut stream,
            BOLT,
            ARROW,
            None,
            along(5.0, -10.0, 1.5),
            Some(forward),
            BOLT_STATE,
            settled,
        );
        frame(&mut ws, &mut residency, &mut stream);
        // The player runs, and the drudge runs after him, turning a little as it goes.
        let run = |ws: &WorldState, id: ObjectId, state: &InterpretedMotionState| {
            ws.objects
                .get(&id)
                .expect("the mover is made")
                .sim
                .driver
                .borrow_mut()
                .move_to_interpreted_state(state, false);
        };
        run(
            &ws,
            RUNNER,
            &InterpretedMotionState {
                forward_command: MotionCommand::RUN_FORWARD,
                forward_speed: 2.5,
                ..InterpretedMotionState::default()
            },
        );
        run(
            &ws,
            CHASER,
            &InterpretedMotionState {
                forward_command: MotionCommand::RUN_FORWARD,
                forward_speed: 1.5,
                turn_command: MotionCommand::TURN_RIGHT,
                turn_speed: 0.2,
                ..InterpretedMotionState::default()
            },
        );
        // Up to speed.
        let mut clock = settled;
        for _ in 0..half / 2 {
            clock = frame(&mut ws, &mut residency, &mut stream);
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = (2.0 * fps) as usize;
        (0..n)
            .map(|i| {
                if let Some((_, how)) = server.filter(|(at, _)| *at == i) {
                    let p = ws.objects[&RUNNER].sim.position.expect("the player stands");
                    let axes = dereth_primitives::frame::l2g(p.frame.rotation);
                    let right = dereth_primitives::frame::localtoglobalvec(
                        axes,
                        Vec3::new(how.metres(), 0.0, 0.0),
                    );
                    let o = p.frame.origin;
                    let to = Position::new(
                        p.cell,
                        Frame::new(
                            Vec3::new(o.x + right.x, o.y + right.y, o.z + right.z),
                            p.frame.rotation,
                        ),
                    );
                    server_moves(&mut stream, RUNNER, to, how, clock);
                }
                clock = frame(&mut ws, &mut residency, &mut stream);
                let viewer = viewer_block(&ws, cfg.landblock);
                let c = ws.character.as_ref().expect("a body");
                let movers = MOVERS
                    .iter()
                    .map(|id| {
                        let o = ws.objects.get(id)?;
                        let physics = o.sim.position?;
                        let d = o.sim.driver.borrow();
                        let collides = o.sim.physics_handle.and_then(|h| {
                            c.world
                                .get(h)
                                .and_then(|b| b.part_frames.as_deref().cloned())
                        });
                        Some(Mover {
                            physics,
                            at: render_frame_in(viewer, physics),
                            drawn: o.frame,
                            parts: d.part_array.parts.iter().map(|p| p.pos).collect(),
                            at_keyframes: at_keyframe(&d, &o.frame),
                            collides,
                        })
                    })
                    .collect();
                Movers {
                    movers,
                    body: c.position(),
                    ticks: c.stats.physics_ticks,
                }
            })
            .collect()
    }

    /// Each frame, how far mover `n` is drawn from where it was drawn the frame before, where it
    /// was drawn on both.
    fn drawn_steps(frames: &[Movers], n: usize) -> Vec<f32> {
        frames
            .windows(2)
            .filter_map(|w| {
                let (a, b) = (w[0].movers[n].as_ref()?, w[1].movers[n].as_ref()?);
                let (a, b) = (a.drawn.origin, b.drawn.origin);
                Some(Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z).magnitude())
            })
            .collect()
    }

    fn sub(a: Vec3, b: Vec3) -> Vec3 {
        Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
    }

    fn dot(a: Vec3, b: Vec3) -> f32 {
        a.x * b.x + a.y * b.y + a.z * b.z
    }

    /// The largest step `steps` takes, and how many of them stand still.
    fn largest_and_still(steps: &[f32]) -> (f32, usize) {
        (
            steps.iter().copied().fold(0.0, f32::max),
            steps.iter().filter(|d| **d < 1e-5).count(),
        )
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn drawn_between_ticks_a_running_player_and_a_chasing_creature_move_a_little_every_frame() {
        for fps in [240.0, 60.0] {
            let stepped = movers(fps, false, None);
            let smooth = movers(fps, true, None);
            for (n, who) in [(0, "the player"), (1, "the drudge"), (2, "the arrow")] {
                let (a, b) = (drawn_steps(&stepped, n), drawn_steps(&smooth, n));
                let ((most_a, still_a), (most_b, still_b)) =
                    (largest_and_still(&a), largest_and_still(&b));
                eprintln!(
                    "{fps} fps, {who}: largest drawn step {most_a:.4} m stepping, {most_b:.4} m \
                     smooth; frames standing still {still_a} and {still_b} of {}",
                    b.len()
                );
                if n == 2 {
                    // The arrow is measured, not asserted: it may strike something on its way.
                    continue;
                }
                assert_eq!(a.len(), b.len(), "{fps} fps, {who}: drawn on every frame");
                assert!(
                    still_a * 3 > a.len(),
                    "{fps} fps, {who}: where physics has it, it stands still between ticks: \
                     {still_a} of {}",
                    a.len()
                );
                assert_eq!(
                    still_b, 0,
                    "{fps} fps, {who}: drawn between ticks it moves every frame"
                );
                assert!(
                    most_b < most_a * 0.75,
                    "{fps} fps, {who}: its largest step {most_b} against {most_a} stepping"
                );
            }
        }
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn drawn_between_ticks_a_running_body_is_never_drawn_ahead_of_where_physics_has_it() {
        for fps in [240.0, 60.0] {
            let frames = movers(fps, true, None);
            for (n, who) in [(0, "the player"), (1, "the drudge"), (2, "the arrow")] {
                // The way physics last carried it: from where it stood at the tick before.
                let mut way: Option<Vec3> = None;
                let mut stood: Option<Vec3> = None;
                let mut longest = 0.0f32;
                let mut behind = 0usize;
                for (f, m) in frames.iter().enumerate() {
                    let Some(m) = m.movers[n].as_ref() else {
                        continue;
                    };
                    if let Some(s) = stood.filter(|s| *s != m.at.origin) {
                        way = Some(sub(m.at.origin, s));
                    }
                    stood = Some(m.at.origin);
                    let off = sub(m.drawn.origin, m.at.origin);
                    let Some(way) = way else { continue };
                    let length = way.magnitude();
                    if length < 1e-4 {
                        continue;
                    }
                    longest = longest.max(length);
                    let ahead = dot(off, way) / length;
                    assert!(
                        ahead <= 1e-4,
                        "{fps} fps, {who}, frame {f}: drawn {ahead} m ahead of physics"
                    );
                    // A tick behind, and a spring's settling time more.
                    assert!(
                        off.magnitude() <= 4.0 * longest,
                        "{fps} fps, {who}, frame {f}: drawn {} m from physics, against a \
                         tick's {longest} m",
                        off.magnitude()
                    );
                    if ahead < -1e-3 {
                        behind += 1;
                    }
                }
                // The arrow may strike something and stop on its way.
                assert!(
                    n == 2 || behind * 2 > frames.len(),
                    "{fps} fps, {who}: drawn behind physics on {behind} of {} frames",
                    frames.len()
                );
            }
        }
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn drawn_between_ticks_a_player_the_server_teleports_is_drawn_where_he_lands_at_once() {
        let at = 120;
        let frames = movers(240.0, true, Some((at, ServerMove::Teleport)));
        let runner = |f: &Movers| f.movers[0].clone().expect("the player");
        let (before, landed) = (runner(&frames[at - 1]), runner(&frames[at]));
        let moved = sub(landed.at.origin, before.at.origin);
        assert!(
            moved.magnitude() > 2.5 && moved.magnitude() < crate::between_ticks::DRAWN_STEP_LIMIT,
            "the teleport, shorter than a step, has put him {} m away",
            moved.magnitude()
        );
        // Sideways: the teleport's way, not the run's.
        let side = sub(landed.at.origin, before.at.origin);
        let side = Vec3::new(side.x, side.y, 0.0);
        for (n, f) in frames.iter().enumerate().skip(at) {
            let m = runner(f);
            let off = sub(m.drawn.origin, m.at.origin);
            let back = -dot(off, side) / side.magnitude();
            assert!(
                back < 0.05,
                "frame {n}: drawn {back} m back toward where he was teleported from"
            );
        }
        assert_eq!(
            landed.drawn, landed.at,
            "drawn where he lands on the frame he lands"
        );
        assert!(
            frames[at + 24..]
                .iter()
                .any(|f| runner(f).drawn != runner(f).at),
            "and drawn between his ticks again once two have passed"
        );
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn drawn_between_ticks_a_player_the_server_corrects_a_short_way_is_drawn_gliding_there() {
        let at = 120;
        let stepped = movers(240.0, false, Some((at, ServerMove::Correct)));
        let smooth = movers(240.0, true, Some((at, ServerMove::Correct)));
        let runner = |f: &Movers| f.movers[0].clone().expect("the player");
        // To his right, the correction's way, against where he stood as it was sent.
        let start = runner(&smooth[at - 1]);
        let axes = dereth_primitives::frame::l2g(start.physics.frame.rotation);
        let right = dereth_primitives::frame::localtoglobalvec(axes, Vec3::new(1.0, 0.0, 0.0));
        let aside = |m: &Mover| dot(sub(m.at.origin, start.at.origin), right);
        let walked = aside(&runner(&smooth[at + 48]));
        assert!(
            walked > 0.5,
            "physics walks him most of the way to where the server says he is: {walked} m"
        );
        // Drawn, he glides there: every frame a little further, never by much more than his
        // running step, where drawn as physics has him he stands still and then steps.
        let (before, _) = largest_and_still(&drawn_steps(&smooth[at - 48..at], 0));
        let (gliding, still) = largest_and_still(&drawn_steps(&smooth[at - 1..at + 48], 0));
        let (stepping, _) = largest_and_still(&drawn_steps(&stepped[at - 1..at + 48], 0));
        eprintln!(
            "corrected {walked:.3} m aside: largest drawn step {gliding:.4} m smooth, \
             {stepping:.4} m stepping, {before:.4} m running before it"
        );
        assert_eq!(still, 0, "drawn moving every frame through the correction");
        assert!(
            gliding < before * 2.0,
            "no jump: its largest step {gliding} against {before} running"
        );
        assert!(
            gliding < stepping * 0.5,
            "its largest step {gliding} against {stepping} stepping"
        );
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn drawn_between_ticks_or_where_physics_has_them_the_movers_move_and_collide_alike() {
        let stepped = movers(240.0, false, Some((120, ServerMove::Teleport)));
        let smooth = movers(240.0, true, Some((120, ServerMove::Teleport)));
        assert_eq!(stepped.len(), smooth.len());
        for (n, (a, b)) in stepped.iter().zip(&smooth).enumerate() {
            assert_eq!(
                (&a.body, a.ticks),
                (&b.body, b.ticks),
                "frame {n}: the body"
            );
            for (who, (x, y)) in a.movers.iter().zip(&b.movers).enumerate() {
                let (x, y) = (x.as_ref(), y.as_ref());
                assert_eq!(
                    x.map(|m| (&m.physics, &m.at, &m.collides)),
                    y.map(|m| (&m.physics, &m.at, &m.collides)),
                    "frame {n}, mover {who}: only the drawing differs"
                );
            }
        }
        assert!(
            smooth.iter().any(|f| f.movers[0]
                .as_ref()
                .is_some_and(|m| m.drawn != m.at && m.at_keyframes)),
            "drawn between ticks, the player is drawn off where physics has him, posed there"
        );
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn with_smooth_movement_off_every_mover_is_drawn_exactly_where_physics_has_it() {
        for fps in [240.0, 60.0] {
            let frames = movers(fps, false, Some((60, ServerMove::Teleport)));
            let mut drawn = 0usize;
            for (n, f) in frames.iter().enumerate() {
                for (who, m) in f.movers.iter().enumerate() {
                    let Some(m) = m else { continue };
                    drawn += 1;
                    assert_eq!(
                        m.drawn, m.at,
                        "{fps} fps, frame {n}, mover {who}: its frame"
                    );
                    assert!(
                        m.at_keyframes,
                        "{fps} fps, frame {n}, mover {who}: its parts, posed where physics has it"
                    );
                }
            }
            assert!(drawn > frames.len() * 2, "the movers are drawn");
        }
    }

    /// A crowd in Holtburg: `n` players and drudges in a grid around a standing body, every one
    /// of them running and turning, stepped for five seconds at 240 frames a second, drawn between
    /// their ticks (`smooth`) or where physics has them. Answers the mean time one frame's world
    /// step took, in microseconds, and the mean time of the drawing between ticks alone.
    fn crowd(n: usize, smooth: bool) -> (f64, f64) {
        use dereth_animation::motion::interp::InterpretedMotionState;
        use dereth_animation::MotionCommand;
        let store = std::sync::Arc::new(dereth_dat::testing::open_store().expect("the dats"));
        let cfg = SceneConfig {
            landblock: 0xA9B4,
            character: true,
            ..SceneConfig::default()
        };
        let (mut ws, mut residency) =
            crate::world_build::load(&store, &cfg).expect("the world loads");
        ws.smooth_movement = smooth;
        let mut stats = StepCounters::default();
        let mut stream = crate::objects::ObjectStream::new();
        let mut counters = crate::world_objects::ObjectCounters::default();
        let fps = 240.0;
        #[allow(clippy::cast_possible_truncation)]
        let dt = (1.0 / fps) as f32;
        let mut now = 1.0;
        let mut sync =
            |ws: &mut WorldState, stream: &mut crate::objects::ObjectStream, now: f64| {
                crate::world_objects::sync_objects(
                    ws,
                    &store,
                    &cfg,
                    stream,
                    &mut counters,
                    &mut crate::world_objects::NoAppearance,
                )
                .expect("the objects synchronise");
                if let Some(c) = ws.character.as_mut() {
                    stream.sync_physics_at(&store, &mut c.world, LocalTime(now));
                }
            };
        let step = |ws: &mut WorldState,
                    residency: &mut crate::world_build::BlockResidency,
                    stats: &mut StepCounters,
                    now: f64| {
            update(
                ws,
                residency,
                &cfg,
                CameraInput::default(),
                CharacterInput::default(),
                LocalTime(now),
                dt,
                stats,
            );
        };
        for _ in 0..120 {
            now += 1.0 / fps;
            sync(&mut ws, &mut stream, now);
            step(&mut ws, &mut residency, &mut stats, now);
            residency.stream(&mut ws, &store, &cfg);
        }
        let at = ws.character.as_ref().expect("a body").position();
        #[allow(clippy::cast_possible_truncation)]
        let side = (n as f64).sqrt().ceil() as usize;
        let ids: Vec<ObjectId> = (0..n)
            .map(|k| ObjectId(0x8400_0000 + u32::try_from(k).expect("a crowd")))
            .collect();
        for (k, id) in ids.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = (
                (k % side) as f32 * 2.5 - 12.0,
                (k / side) as f32 * 2.5 - 12.0,
            );
            let o = at.frame.origin;
            let p = Position::new(
                at.cell,
                Frame::new(Vec3::new(o.x + x, o.y + y, o.z), at.frame.rotation),
            );
            let (setup, motions) = if k % 2 == 0 {
                (HUMAN, HUMAN_MOTIONS)
            } else {
                (DRUDGE, DRUDGE_MOTIONS)
            };
            create(
                &mut stream,
                *id,
                setup,
                Some(motions),
                p,
                None,
                CREATURE_STATE,
                now,
            );
        }
        now += 1.0 / fps;
        sync(&mut ws, &mut stream, now);
        step(&mut ws, &mut residency, &mut stats, now);
        for (k, id) in ids.iter().enumerate() {
            if let Some(o) = ws.objects.get(id) {
                o.sim.driver.borrow_mut().move_to_interpreted_state(
                    &InterpretedMotionState {
                        forward_command: MotionCommand::RUN_FORWARD,
                        forward_speed: 1.5,
                        turn_command: if k % 3 == 0 {
                            MotionCommand::TURN_RIGHT
                        } else {
                            MotionCommand::NONE
                        },
                        turn_speed: 0.3,
                        ..InterpretedMotionState::default()
                    },
                    false,
                );
            }
        }
        let frames = 1200;
        let mut stepping = std::time::Duration::ZERO;
        let mut drawing = std::time::Duration::ZERO;
        for _ in 0..frames {
            now += 1.0 / fps;
            sync(&mut ws, &mut stream, now);
            let t = std::time::Instant::now();
            step(&mut ws, &mut residency, &mut stats, now);
            stepping += t.elapsed();
            // The drawing between ticks alone, run again over every object on a copy of its
            // state: what the frame above spent on it.
            if smooth {
                let viewer = viewer_block(&ws, cfg.landblock);
                let mut copies: Vec<(crate::between_ticks::BetweenTicks, Position)> = ws
                    .objects
                    .values()
                    .filter_map(|o| Some((o.between.clone(), o.sim.position?)))
                    .collect();
                let t = std::time::Instant::now();
                for (b, here) in &mut copies {
                    let f = render_frame_in(viewer, *here);
                    std::hint::black_box(drawn_between_ticks(
                        b,
                        f,
                        LocalTime(now),
                        true,
                        *here,
                        0,
                        viewer,
                    ));
                }
                drawing += t.elapsed();
            }
            residency.stream(&mut ws, &store, &cfg);
        }
        let mean = |d: std::time::Duration| d.as_secs_f64() * 1e6 / f64::from(frames);
        (mean(stepping), mean(drawing))
    }

    /// What drawing a busy town's bodies between their ticks costs a frame.
    #[test]
    #[ignore = "instrument: prints timings; run with --features retail-dats and --ignored"]
    fn measure_what_drawing_a_crowd_between_ticks_costs() {
        for n in [100, 300] {
            let (off, _) = crowd(n, false);
            let (on, drawing) = crowd(n, true);
            eprintln!(
                "{n} running bodies: a frame's world step {off:.1} us where physics has them, \
                 {on:.1} us between ticks; the drawing between ticks alone {drawing:.1} us"
            );
        }
    }

    /// A scene with no body keeps the world's own beat: at 60 Hz its objects are stepped every
    /// second frame, as they are around a body.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_scene_with_no_body_steps_its_objects_every_second_frame_at_sixty_hertz() {
        let store = std::sync::Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {}",
                dereth_dat::testing::dat_dir().display()
            )
        }));
        let cfg = SceneConfig {
            landblock: 0xA9B4,
            character: false,
            ..SceneConfig::default()
        };
        let (mut ws, _residency) = crate::world_build::load(&store, &cfg).expect("the world loads");
        assert!(ws.character.is_none(), "premise: no body");
        let mut stats = StepCounters::default();
        let mut now = 1.0;
        let mut ticks = Vec::new();
        for i in 0..240 {
            now += 1.0 / 60.0;
            if step_physics(
                &mut ws,
                CameraInput::default(),
                CharacterInput::default(),
                LocalTime(now),
                1.0 / 60.0,
                &mut stats,
            ) {
                ticks.push(i);
            }
        }
        let gaps: Vec<i32> = ticks.windows(2).map(|p| p[1] - p[0]).collect();
        assert!(
            gaps.len() > 100 && gaps.iter().all(|g| *g == 2),
            "the objects must be stepped every second frame; gaps {gaps:?}"
        );
    }
}
