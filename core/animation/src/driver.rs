//! `MotionDriver` — the facade physics holds, and the implementation of
//! `dereth_primitives::MotionSource`.
//!
//! This is where the pieces meet: the part array, the animation player, the
//! motion-table manager, the movement manager, the script manager and the particle manager, plus
//! the deferred hook queue that connects them.
//!
//! **What `advance` must not do**: scale the returned origin by the object's scale, or zero it when
//! airborne. The physics update does both, and both depend on
//! `ON_WALKABLE_TS`, which is physics state the physics crate owns:
//!
//! Translation is multiplied by the object scale while `ON_WALKABLE_TS` is set, and
//! by `0.0` otherwise. The physics caller applies this after advancing the animation;
//! the motion driver returns the unscaled offset.
//!
//! The **rotation** is never zeroed, which is why the seam carries a `Frame` and not a `Vec3`.

use std::sync::Arc;

use dereth_primitives::num::rng::Ran2;
use dereth_primitives::{DataId, Frame, LocalTime, MotionSource, Position, ServerTime};

use crate::command::MotionCommand;
use crate::data::{AnimAssets, NoAssets, SetupData};
use crate::frame::V3;
use crate::hooks::{AnimEvent, AnimHook, HookKind, HookQueue, SoundType};
use crate::motion::interp::MotionCtx;
use crate::motion::moveto::{self, TargetInfo, TargetSnapshot, TARGET_TICK};
use crate::motion::{MotionEffect, MotionEnv, MotionTarget, MovementManager, TargetedEffects};
use crate::particles::{EmitterContext, ParticleManager, NO_PART};
use crate::parts::PartArray;
use crate::script::ScriptManager;
use crate::seq::Sequence;
use crate::table::MotionTableManager;

/// Which of the frame-hook switch's eight arms an [`FpHook`] drives.
///
/// The discriminants are the switch values the client's own frame-hook dispatch uses. The eight
/// arms are, in order: the object
/// scale, the object translucency, one part's translucency, the object luminosity, one part's
/// luminosity, the object diffusion, one part's diffusion, and the deferred particle script.
///
/// **Arms 0 and 7 are here too.** Nothing downstream interpolates the scale, so the driver runs
/// that ramp itself; and 104 of the 458 shipped `CALL_PES` hooks — every one whose `pause` is at
/// or above the epsilon — need arm 7's timer, or they roll their delay and are dropped. The table
/// covers the whole switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpHookKind {
    /// 0 -- the object scale.
    Scaling,
    /// 1 — the whole part array.
    Translucency,
    /// 2 — one part.
    PartTranslucency,
    /// 3.
    Luminosity,
    /// 4.
    PartLuminosity,
    /// 5.
    Diffusion,
    /// 6.
    PartDiffusion,
    /// 7 — deferred particle script, whose script id rides in
    /// [`FpHook::part`] because that is where retail puts it.
    CallPes,
}

/// An interpolating hook owned by a physics object.
///
/// The hook stores its kind, double-precision start time and duration, `from` and `to`
/// values, and a part index for the per-part arms. This Rust record omits the original
/// linked-list pointers; the driver stores the ordered hooks in a vector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FpHook {
    pub kind: FpHookKind,
    /// The `void*` parameter. The per-part arms use it as a part index; [`FpHookKind::CallPes`]
    /// uses it as the script `DataID`, because the retail caller passes the id itself in that
    /// parameter and the hook processor's arm 7 casts it straight back to a script id; the rest
    /// ignore it.
    pub part: u32,
    /// The physics clock's current time when the hook was built.
    pub start_time: f64,
    pub duration: f64,
    pub from: f32,
    pub to: f32,
}

/// The animation half of one object.
pub struct MotionDriver {
    pub assets: Arc<dyn AnimAssets>,
    pub part_array: PartArray,
    pub sequence: Sequence,
    pub motion_table: MotionTableManager,
    pub movement: MovementManager,
    /// Authoritative existing host subscription, updated synchronously by motion callbacks.
    pub target: Option<MotionTarget>,
    /// The watched object's facts, resolved by the host once per frame off its
    /// object table and consumed by [`Self::handle_targetting`] once per physics sub-step.
    pub target_snapshot: Option<TargetSnapshot>,
    /// How many targeting ticks ran for this object — once
    /// per sub-step, so over one `update_object` it equals the number of sub-steps taken.
    pub targetting_ticks: u32,
    /// How many `TargetInfo` updates reached the movement driver.
    pub target_updates: u32,
    /// The last `TargetInfo` delivered, for the tests.
    pub last_target_info: Option<TargetInfo>,
    pub scripts: ScriptManager,
    pub particles: ParticleManager,
    /// Deferred animation-hook queue for the physics object.
    pub hooks: HookQueue,
    /// The interpolating half of the six ramp hooks. Index 0 is the list
    /// **head**, because each new hook is inserted there and updates walk from that head.
    ///
    /// Without it a ramp would be dropped, and the unhide play script's `Transparent 1.0 -> 0.0
    /// over 0.75 s` could never undo the `NoDraw` that the hidden play script's immediate
    /// `Transparent 1.0` sets.
    pub fp_hooks: Vec<FpHook>,
    /// The physics facts, refreshed by physics before each call.
    pub env: MotionEnv,
    /// Animation scale requested by `ScaleHook`; the *caller* applies it to the animation offset.
    pub scale: f32,
    /// Whether physics-state bit `0x100000` locks translucency.
    pub translucency_locked: bool,
    /// The object's own default play-script type and intensity, from `PhysicsDesc`.
    pub default_script: u32,
    pub default_script_intensity: f32,
    pub script_table: Option<DataId>,
    /// The `ran2` stream the particle emitters draw from.
    pub rng: Ran2,
    /// Current server time for the script timeline.
    pub cur_time: ServerTime,
    /// The frame [`Self::update_parts`] last placed the parts from; see
    /// [`Self::particle_parent_frame`].
    placed_frame: Option<Frame>,
    events: Vec<AnimEvent>,
    effects: Vec<MotionEffect>,
}

impl std::fmt::Debug for MotionDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MotionDriver")
            .field("parts", &self.part_array.parts.len())
            .field("nodes", &self.sequence.nodes().len())
            .field("pending_motions", &self.motion_table.pending_len())
            .field("emitters", &self.particles.len())
            .field("queued_hooks", &self.hooks.len())
            .finish()
    }
}

impl Default for MotionDriver {
    fn default() -> Self {
        Self::new(Arc::new(NoAssets))
    }
}

impl MotionDriver {
    #[must_use]
    pub fn new(assets: Arc<dyn AnimAssets>) -> Self {
        Self {
            assets,
            part_array: PartArray::new(),
            sequence: Sequence::new(),
            motion_table: MotionTableManager::new(None),
            movement: MovementManager::new(),
            target: None,
            target_snapshot: None,
            targetting_ticks: 0,
            target_updates: 0,
            last_target_info: None,
            scripts: ScriptManager::new(),
            particles: ParticleManager::new(),
            hooks: HookQueue::new(),
            fp_hooks: Vec::new(),
            env: MotionEnv::default(),
            scale: 1.0,
            translucency_locked: false,
            default_script: 0,
            default_script_intensity: 0.0,
            script_table: None,
            rng: Ran2::new(1),
            cur_time: ServerTime(0.0),
            placed_frame: None,
            events: Vec::new(),
            effects: Vec::new(),
        }
    }

    /// Create the setup and then initialise its defaults.
    ///
    /// Default initialization queues the setup's `default_anim_id` as a single `AnimData`
    /// with `low = 0, high = -1, framerate = 30` — the client's own default — and hands the
    /// setup's motion table to the manager.
    pub fn set_setup(&mut self, setup: Arc<SetupData>) -> bool {
        let Some(pa) =
            PartArray::create_setup(Arc::clone(&setup), true, &mut self.sequence, &*self.assets)
        else {
            return false;
        };
        self.part_array = pa;
        if let Some(anim) = setup.default_animation {
            self.sequence.clear_animations();
            self.sequence.append_animation(
                crate::data::AnimData {
                    anim_id: anim,
                    low_frame: 0,
                    high_frame: -1,
                    framerate: 30.0,
                },
                &*self.assets,
            );
        }
        if let Some(mt) = setup.default_motion_table {
            self.set_motion_table(mt);
        }
        self.script_table = setup.default_phs_table;
        if let Some(script) = setup.default_script {
            self.scripts
                .add_script(script, &*self.assets, self.cur_time);
        }
        true
    }

    /// A new table destroys and recreates the movement
    /// manager, and re-enters the default state.
    pub fn set_motion_table(&mut self, id: DataId) -> bool {
        let Some(table) = self.assets.motion_table(id) else {
            return false;
        };
        self.motion_table.set_table(Some(table));
        self.movement = MovementManager::new();
        self.with_movement(|movement, ctx| movement.enter_default_state(ctx));
        true
    }

    /// Apply an object description's part swaps. The driver's own asset handle is the one the
    /// part swaps must be resolved against.
    ///
    /// `PartArray::do_obj_desc_changes` alone has no graphics-object lookup and accepts any
    /// non-zero id. This method forwards `self.assets`, so a swap naming an object absent
    /// from the dat fails and preserves the limb.
    pub fn do_obj_desc_changes(&mut self, od: &crate::parts::ObjDesc) -> bool {
        self.part_array.do_obj_desc_changes_with(od, &*self.assets)
    }

    /// The same, from the default object description.
    pub fn do_obj_desc_changes_from_default(&mut self, od: &crate::parts::ObjDesc) -> bool {
        self.part_array
            .do_obj_desc_changes_from_default_with(od, &*self.assets)
    }

    /// Drain the events raised since the last call, **in the order they were raised**. The order
    /// is observable, so this is a `Vec` drained FIFO and never a set.
    pub fn take_events(&mut self) -> Vec<AnimEvent> {
        std::mem::take(&mut self.events)
    }

    /// How many events are waiting — what the next [`Self::take_events`] would hand back.
    ///
    /// An undrained queue is a leak, and the only way to say so in a test is to
    /// be able to read its length from outside the crate. Retail has nothing to correspond to:
    /// the original client's hooks call straight into the object without an event queue.
    #[must_use]
    pub fn pending_events(&self) -> usize {
        self.events.len()
    }

    /// Drain the physics requests the movement layer made. Physics applies them.
    pub fn take_effects(&mut self) -> Vec<MotionEffect> {
        std::mem::take(&mut self.effects)
    }

    /// Run against this driver's authoritative workers, ledgers and subscription facts.
    /// The callback receives disjoint mutable fields, so nested movement calls never alias
    /// an already borrowed motion-interpreter, move-to or sticky worker or require take/replace.
    pub fn with_movement<R>(
        &mut self,
        f: impl FnOnce(&mut MovementManager, &mut MotionCtx<'_>) -> R,
    ) -> R {
        // The target manager's stored extrapolation lead, or 0.0 when it holds none, must be
        // visible in the environment: movement reads it before deciding whether to overwrite
        // it. Keeping it only on the effect sink would hide that value.
        self.env.target_quantum = self.target.map_or(0.0, |t| t.quantum);
        let mut effects = TargetedEffects {
            target: &mut self.target,
            pending: &mut self.effects,
        };
        let mut ctx = MotionCtx {
            mgr: &mut self.motion_table,
            seq: &mut self.sequence,
            assets: &*self.assets,
            env: &self.env,
            effects: &mut effects,
            events: &mut self.events,
        };
        f(&mut self.movement, &mut ctx)
    }

    /// A host-issued movement call returns only its own new non-target effects. Older queued
    /// physics effects keep their existing host drain phase; target facts still publish inline.
    pub fn with_movement_scoped_effects<R>(
        &mut self,
        f: impl FnOnce(&mut MovementManager, &mut MotionCtx<'_>) -> R,
    ) -> (R, Vec<MotionEffect>) {
        let first_new_effect = self.effects.len();
        let result = self.with_movement(f);
        (result, self.effects.split_off(first_new_effect))
    }

    /// The host's per-frame half of the voyeur model — see [`Self::handle_targetting`].
    pub fn set_target_snapshot(&mut self, snapshot: Option<TargetSnapshot>) {
        self.target_snapshot = snapshot;
    }

    /// The target manager's per-sub-step tick, transcribed onto the watcher.
    ///
    /// Retail's targetting is voyeur-side. Subscribing registers the
    /// watcher on the **target's** manager, whose tail sends one
    /// unconditional update with the target's raw `position` in both position slots. From then
    /// on the target's own targeting tick runs every sub-step from the movement tick. It gates
    /// on `physics clock - last_update_time >= 0.5` and runs
    /// for each watcher: the sent point is `position + get_velocity() * quantum`,
    /// and fires only when that point has drifted more than the
    /// watcher's `radius` from `last_sent_position`, carrying the raw position as
    /// `target_position`, the led point as `interpolated_position`, and `get_velocity()` — the
    /// target's **`cached_velocity`**. `last_update_time` advances whether or not anything was
    /// sent, so a stationary target says nothing after its first update.
    ///
    /// This build keeps the subscription on the watcher ([`Self::target`]) and lets the host
    /// resolve the target's facts once per frame into [`Self::target_snapshot`]; the tick above is
    /// run here, per sub-step, where retail runs it. Two things it does not do: it does not time a
    /// never-answered subscription out after 10 s (`TargetStatus::Undef` never occurs, because the
    /// first update is delivered on the next sub-step rather than awaited), and the 0.5 s clock is
    /// per subscription rather than per target.
    fn handle_targetting(&mut self, now: LocalTime) {
        self.targetting_ticks = self.targetting_ticks.wrapping_add(1);
        let Some(t) = self.target else { return };
        let Some(snap) = self.target_snapshot else {
            return;
        };
        if snap.object_id != t.id {
            return; // the host has not resolved the new subscription yet
        }
        if !snap.ok {
            // immediate, not gated, not radius-tested.
            let fx = self.receive_target_update(snap.position, snap.position, false, now.0);
            self.effects.extend(fx);
            return;
        }
        if let Some(fx) = self.deliver_first_target_update(now) {
            // A snapshot the host set without delivering (a station, or a host that resolves
            // between sub-steps): the subscription's first update goes out on the first sub-step
            // instead.
            self.effects.extend(fx);
            return;
        }
        if now.0 - t.tick_time < TARGET_TICK {
            return;
        }
        if let Some(t) = self.target.as_mut() {
            t.tick_time = now.0;
        }
        // `origin += (float)quantum * get_velocity()`.
        let mut led = snap.position;
        led.frame.origin = led.frame.origin.add(snap.velocity.mul(t.quantum));
        // Send only when the distance from `led` to `last_sent` exceeds `radius`.
        let last = t.last_sent.unwrap_or(snap.position);
        if moveto::distance(&led, &last) > t.radius {
            if let Some(t) = self.target.as_mut() {
                t.last_sent = Some(led);
            }
            let fx = self.receive_target_update(snap.position, led, true, now.0);
            self.effects.extend(fx);
        }
    }

    /// The subscription handler's tail sends the watcher an update with the object's own position
    /// and the "ok" target status, the one unconditional update a fresh subscription gets, with
    /// the raw position in both slots and no lead. Retail sends it **synchronously** inside the
    /// subscribe call; here the host calls this the moment it has resolved
    /// the target's facts into [`Self::target_snapshot`], which is the earliest the facts exist.
    /// `None` when the subscription is not fresh, not yet resolved, or already gone (`ok ==
    /// false` is the target-event notification's, and is [`Self::handle_targetting`]'s to deliver).
    #[must_use]
    pub fn deliver_first_target_update(&mut self, now: LocalTime) -> Option<Vec<MotionEffect>> {
        let t = self.target?;
        let snap = self.target_snapshot?;
        if snap.object_id != t.id || !snap.ok {
            return None;
        }
        if t.last_sent.is_some() {
            return None;
        }
        if let Some(t) = self.target.as_mut() {
            t.last_sent = Some(snap.position);
            t.tick_time = now.0;
        }
        Some(self.receive_target_update(snap.position, snap.position, true, now.0))
    }

    /// Receiving a target update hands the same `TargetInfo` to the
    /// movement manager **and** the position manager. Returns only the effects this
    /// delivery raised, so a synchronous host call can apply them in place; the per-sub-step
    /// caller pushes them back onto the ordered queue.
    pub fn receive_target_update(
        &mut self,
        target_position: Position,
        interpolated_position: Position,
        ok: bool,
        now: f64,
    ) -> Vec<MotionEffect> {
        let Some(t) = self.target.as_mut() else {
            return Vec::new();
        };
        t.last_update = now;
        let info = TargetInfo {
            object_id: t.id,
            ok,
            target_position,
            interpolated_position,
        };
        self.last_target_info = Some(info);
        self.target_updates = self.target_updates.wrapping_add(1);
        let ((), fx) = self.with_movement_scoped_effects(|movement, ctx| {
            movement.handle_update_target(&info, ctx);
            movement.handle_sticky_update_target(&info, ctx);
            // `PerformMovement`'s unconditional tail.
            movement.drain_completed_motions(ctx);
        });
        fx
    }

    /// The script hooks execute **inline**, right here,
    /// rather than joining the deferred animation-hook queue.
    pub fn update_scripts(&mut self) {
        let mut fired = Vec::new();
        self.scripts.update_scripts(self.cur_time, &mut fired);
        for h in fired {
            self.execute_hook(h);
        }
    }

    /// The particle-manager update.
    pub fn update_particles(&mut self, should_draw: bool) {
        let base = self.particle_parent_frame();
        let ctx = EmitterContext {
            parent_frame: base,
            part_frame: None,
            emitter_origin: base.origin,
            now: self.cur_time.0,
            should_draw,
        };
        let parts = &self.part_array.parts;
        self.particles.update_particles_with_part_frames(
            &ctx,
            |part| {
                parts
                    .get(usize::try_from(part).unwrap_or(usize::MAX))
                    .map(|p| p.pos)
            },
            &mut self.rng,
        );
    }

    /// Advance each interpolating hook once and remove completed hooks.
    ///
    /// The observed linked-list traversal saves the next entry before executing the current
    /// hook, then unlinks and deletes that hook if it has finished. This vector-backed
    /// implementation preserves head-to-tail execution order.
    ///
    /// The original client does this once per object per tick at the tail of position
    /// updating, after script and particle updates. Callers here preserve that ordering.
    pub fn update_fp_hooks(&mut self) {
        if self.fp_hooks.is_empty() {
            return;
        }
        let now = self.cur_time.0;
        // The walk saves `next` before `Execute` runs, so a hook cannot disturb the traversal;
        // taking the list first is that guarantee, and a hook this pass creates joins the head
        // afterwards exactly as a newly linked translucency ramp does.
        let walked = std::mem::take(&mut self.fp_hooks);
        let mut kept = Vec::with_capacity(walked.len());
        for h in walked {
            if !self.execute_fp_hook(h, now) {
                kept.push(h);
            }
        }
        // Anything queued while walking is newer, so it stays ahead of the survivors.
        self.fp_hooks.append(&mut kept);
    }

    /// Advances one step. Answers **true** when the hook is finished and
    /// `process_hooks` should unlink it.
    ///
    /// ```text
    ///   t = physics_clock - start_time
    ///   f = (t <= 0) ? 0 : (t >= duration) ? 1 : (float)(t / duration)
    ///   v = from + (to - from) * f
    ///   process_fp_hook(kind, v, user_data)
    ///   return f == 1.0                     ; finished
    /// ```
    ///
    /// The fraction is computed in `double` and **stored to a `float`** before the
    /// multiply, which is why `f` is an `f32` here.
    fn execute_fp_hook(&mut self, h: FpHook, now: f64) -> bool {
        let t = now - h.start_time;
        // LINT-OK: the client narrows the quotient to a float too.
        #[allow(clippy::cast_possible_truncation)]
        let f: f32 = if t <= 0.0 {
            0.0
        } else if t >= h.duration {
            1.0
        } else {
            (t / h.duration) as f32
        };
        self.process_fp_hook(h.kind, h.from + (h.to - h.from) * f, h.part);
        f >= 1.0
    }

    /// The frame-hook switch, minus the two arms this build has
    /// no consumer for. See [`FpHookKind`].
    ///
    /// **Named rather than hidden:** retail's translucency arm first clamps the requested value up
    /// to the object's original translucency, then stores the result as its current translucency.
    /// This build keeps neither field — translucency
    /// lives only on the parts — so the clamp and the store are absent. The original translucency
    /// comes from the physics description, which is `0.0` on every player create in the corpus, and
    /// `max(v, 0.0)` is `v`; the *store* is the one that matters.
    fn process_fp_hook(&mut self, kind: FpHookKind, v: f32, part: u32) {
        match kind {
            FpHookKind::Scaling => self.set_scale_internal(v),
            // The deferred-script arm -- short, and both of its guards matter:
            //
            // ```text
            //   if (v < 1.0)            do nothing
            //   if the object has no cell, do nothing
            //   play_script_internal(...)
            // ```
            //
            // So the ramp runs from `0.0` to `1.0` over the rolled delay and the script is played
            // on the **one** step that reaches 1.0 — which is also the step the hook's execute
            // answers `1` on, so it fires exactly once and then unlinks. The cell test is taken at
            // *fire* time, not at arm time: an object that left its cell while the timer ran plays
            // nothing, and that is how retail cancels a delayed effect on an object that died.
            FpHookKind::CallPes => {
                if v >= 1.0 && self.env.in_cell {
                    self.play_script_internal(DataId(part));
                }
            }
            FpHookKind::Translucency => {
                self.part_array
                    .set_translucency_internal(v, self.translucency_locked);
            }
            FpHookKind::PartTranslucency => {
                self.part_array
                    .set_part_translucency_internal(part, v, self.translucency_locked);
            }
            FpHookKind::Luminosity => self.part_array.set_luminosity_internal(v),
            FpHookKind::PartLuminosity => {
                self.part_array.set_part_luminosity_internal(part, v);
            }
            FpHookKind::Diffusion => self.part_array.set_diffusion_internal(v),
            FpHookKind::PartDiffusion => {
                self.part_array.set_part_diffusion_internal(part, v);
            }
        };
    }

    /// All six timed visual setters share a threshold of `0.0002`: a shorter duration
    /// applies `end` immediately; otherwise a new interpolating hook is inserted at the
    /// head of the list.
    ///
    /// The immediate arm applies **`end`**, not `start`: a `time == 0.0` ramp is its end value and
    /// nothing else, which is why the hidden play script's `1.0 -> 1.0 over 0.0` is a plain
    /// "become invisible".
    fn ramp(&mut self, kind: FpHookKind, part: u32, start: f32, end: f32, time: f32) {
        if time < dereth_primitives::num::consts::EPSILON {
            self.process_fp_hook(kind, end, part);
            return;
        }
        self.fp_hooks.insert(
            0,
            FpHook {
                kind,
                part,
                start_time: self.cur_time.0,
                duration: f64::from(time),
                from: start,
                to: end,
            },
        );
    }

    /// Set both the driver scale and the part-array scale `(s, s, s)`.
    ///
    /// Collision spheres and attack geometry use the physics object's scale, while this
    /// driver and its part array retain the animation-side values. The physics-side scale
    /// therefore also has to cross the seam as an event. Without it, physics scale would be
    /// written only from object descriptions, and a `ScaleHook` would change how big an object
    /// looked without changing its collision size.
    fn set_scale_internal(&mut self, v: f32) {
        self.scale = v;
        self.part_array
            .set_scale_internal(dereth_primitives::Vec3::new(v, v, v));
        self.events.push(AnimEvent::SetScale(v));
    }

    /// The scale ramp uses the same `0.0002` split as the six visual ramps: a shorter
    /// duration applies `end` immediately; otherwise it creates a scaling hook.
    ///
    /// The ramp starts at the object's **current** scale, ends at `end`, and carries
    /// `user_data = 0`.
    ///
    /// Measured over the retail dats: **122 shipped `SCALE` hooks,
    /// all in physics scripts, and every one of them carries `time == 0`** — so the ramp arm is
    /// written because the instruction is there, not because anything ships that takes it.
    fn set_scale(&mut self, end: f32, time: f32) {
        if time < dereth_primitives::num::consts::EPSILON {
            self.set_scale_internal(end);
            return;
        }
        self.fp_hooks.insert(
            0,
            FpHook {
                kind: FpHookKind::Scaling,
                part: 0,
                start_time: self.cur_time.0,
                duration: f64::from(time),
                from: self.scale,
                to: end,
            },
        );
    }

    /// Execute one hook.
    ///
    /// Everything whose target lives in another crate becomes an [`AnimEvent`]; everything this
    /// crate owns is applied here and now.
    #[allow(clippy::too_many_lines)]
    fn execute_hook(&mut self, h: AnimHook) {
        match h.kind {
            HookKind::NoOp => {}
            HookKind::AnimationDone => {
                // The part array's animation-done step reaches the motion-table manager,
                // whose completion tail records the motion and its successful status. Both
                // operations take this route so no caller can drain the completion without
                // publishing it.
                self.with_movement(|movement, ctx| movement.drain_animation_done(true, ctx));
            }
            HookKind::Sound { gid } => self.events.push(AnimEvent::PlaySound {
                gid,
                // `SoundHook` has no tweaks; `SoundTweakedHook`'s constructor defaults are the
                // values the plain form implies.
                volume: 1.0,
                priority: 0.9,
                probability: 1.0,
            }),
            HookKind::SoundTweaked(dereth_primitives::records::HookSoundTweaked {
                sound_id: gid,
                probability,
                priority,
                volume,
            }) => {
                self.events.push(AnimEvent::PlaySound {
                    gid,
                    volume,
                    priority,
                    probability,
                });
            }
            HookKind::SoundTable { sound_type } => {
                self.events.push(AnimEvent::PlaySoundType {
                    kind: SoundType(sound_type),
                });
            }
            HookKind::Attack(cone) => self.events.push(AnimEvent::Attack { cone }),
            HookKind::ReplaceObject {
                part_index,
                part_id,
            } => {
                // the retail replace-object hook's execute step
                // has no body (it is a pure-virtual stub), so whether the hook is dead in this
                // build is unknown. ACE's name implies a body-part swap, which would
                // swap a body part's gfxobj mid-animation. The hook is unpacked and stored, and
                // executing it raises an event and changes nothing: the documented approximation.
                self.events.push(AnimEvent::ReplaceObjectNoOp {
                    part_index,
                    part_id,
                });
            }
            HookKind::Ethereal { ethereal } => {
                self.events.push(AnimEvent::SetEthereal(ethereal != 0));
            }
            HookKind::NoDraw { nodraw } => {
                self.part_array.set_no_draw_internal(nodraw != 0);
                self.events.push(AnimEvent::SetNoDraw(nodraw != 0));
            }
            HookKind::SetLight { lights_on } => {
                self.events.push(AnimEvent::SetLights(lights_on != 0));
            }
            HookKind::SetOmega { axis } => self.events.push(AnimEvent::SetOmega(axis)),
            HookKind::Scale(dereth_primitives::records::HookScale { end, time }) => {
                self.set_scale(end, time)
            }
            // The six interpolating visual hooks. Each timed setter has two arms:
            // `time < 0.0002` applies the end value now; a longer duration creates an
            // interpolating hook that [`Self::update_fp_hooks`] steps.
            // Both arms matter: the hidden play script's `Transparent` is `time == 0.0` and the
            // unhide play script's is a 0.75 s ramp, so with only the immediate case a character
            // goes invisible at login and never comes back.
            HookKind::Transparent(dereth_primitives::records::HookRamp { start, end, time }) => {
                self.ramp(FpHookKind::Translucency, 0, start, end, time);
            }
            HookKind::TransparentPart(dereth_primitives::records::HookPartRamp {
                part,
                start,
                end,
                time,
            }) => {
                self.ramp(FpHookKind::PartTranslucency, part, start, end, time);
            }
            HookKind::Luminous(dereth_primitives::records::HookRamp { start, end, time }) => {
                self.ramp(FpHookKind::Luminosity, 0, start, end, time);
            }
            HookKind::LuminousPart(dereth_primitives::records::HookPartRamp {
                part,
                start,
                end,
                time,
            }) => {
                self.ramp(FpHookKind::PartLuminosity, part, start, end, time);
            }
            HookKind::Diffuse(dereth_primitives::records::HookRamp { start, end, time }) => {
                self.ramp(FpHookKind::Diffusion, 0, start, end, time);
            }
            HookKind::DiffusePart(dereth_primitives::records::HookPartRamp {
                part,
                start,
                end,
                time,
            }) => {
                self.ramp(FpHookKind::PartDiffusion, part, start, end, time);
            }
            HookKind::TextureVelocity(dereth_primitives::records::HookTextureVelocity {
                u_speed,
                v_speed,
            }) => {
                self.events.push(AnimEvent::SetTextureVelocity {
                    part: None,
                    u: u_speed,
                    v: v_speed,
                });
            }
            HookKind::TextureVelocityPart(
                dereth_primitives::records::HookTextureVelocityPart {
                    part_index,
                    u_speed,
                    v_speed,
                },
            ) => {
                self.events.push(AnimEvent::SetTextureVelocity {
                    part: Some(part_index),
                    u: u_speed,
                    v: v_speed,
                });
            }
            HookKind::CreateParticle(dereth_primitives::records::HookCreateParticle {
                emitter_info_id: info,
                part_index,
                offset,
                emitter_id,
            }) => {
                self.create_emitter(info, part_index, offset, emitter_id, false);
            }
            HookKind::CreateBlockingParticle(dereth_primitives::records::HookCreateParticle {
                emitter_info_id: info,
                part_index,
                offset,
                emitter_id,
            }) => {
                self.create_emitter(info, part_index, offset, emitter_id, true);
            }
            HookKind::DestroyParticle { emitter_id } => {
                self.particles.destroy_particle_emitter(emitter_id);
                self.events
                    .push(AnimEvent::DestroyParticleEmitter(emitter_id));
            }
            HookKind::StopParticle { emitter_id } => {
                self.particles.stop_particle_emitter(emitter_id);
                self.events.push(AnimEvent::StopParticleEmitter(emitter_id));
            }
            HookKind::DefaultScript => {
                self.play_default_script(None);
            }
            HookKind::DefaultScriptPart { part_index } => {
                // The *child object* attached at that part runs its own default script; the child
                // list belongs to physics, so this is reported rather than resolved.
                self.events.push(AnimEvent::PlayDefaultScript {
                    part: Some(part_index),
                });
            }
            HookKind::CallPes(dereth_primitives::records::HookCallPes { pes, pause }) => {
                // A `pause` at or above `0.0002` rolls uniformly in `[0, pause]` once and schedules
                // the script for that many seconds later, preventing identical objects from
                // flashing in lockstep. Below that threshold the script plays immediately if a
                // cell exists.
                //
                // The deferred arm inserts a timer at the head of the interpolating-hook list,
                // with start value 0.0, end value 1.0, the rolled duration and the script id as its
                // payload. It uses the same simulation clock as the other ramps and dies with
                // the object that owns the list. Nothing re-rolls: [`FpHookKind::CallPes`] plays
                // the script only on the step that reaches 1.0. Arming the hook again inserts a
                // second timer; replacing a script mid-flight does not cancel the first.
                //
                // The roll happens even for a delayed script, so the `ran2` stream stays in step.
                // 104 of the 458 shipped `CALL_PES` hooks carry a pause of 0.5 s to 900 s, so the
                // timer is not optional.
                if pause >= dereth_primitives::num::consts::EPSILON {
                    let delay = self.rng.roll_f32(0.0, pause);
                    self.fp_hooks.insert(
                        0,
                        FpHook {
                            kind: FpHookKind::CallPes,
                            part: pes.0,
                            start_time: self.cur_time.0,
                            duration: f64::from(delay),
                            from: 0.0,
                            to: 1.0,
                        },
                    );
                    self.events.push(AnimEvent::CallPes {
                        script: pes,
                        pause: delay,
                    });
                } else {
                    if self.env.in_cell {
                        self.scripts.add_script(pes, &*self.assets, self.cur_time);
                    }
                    self.events.push(AnimEvent::CallPes {
                        script: pes,
                        pause: 0.0,
                    });
                }
            }
        }
    }

    fn create_emitter(
        &mut self,
        info: DataId,
        part_index: u32,
        offset: Frame,
        emitter_id: u32,
        blocking: bool,
    ) {
        self.events.push(AnimEvent::CreateParticleEmitter {
            info,
            part: part_index,
            offset,
            id: emitter_id,
            blocking,
        });
        let Some(i) = self.assets.emitter_info(info) else {
            return;
        };
        let base = self.particle_parent_frame();
        let ctx = EmitterContext {
            parent_frame: base,
            part_frame: if part_index == NO_PART {
                None
            } else {
                self.part_array
                    .parts
                    .get(usize::try_from(part_index).unwrap_or(usize::MAX))
                    .map(|p| p.pos)
            },
            emitter_origin: base.origin,
            now: self.cur_time.0,
            should_draw: true,
        };
        if blocking {
            self.particles.create_blocking_particle_emitter(
                i,
                part_index,
                offset,
                emitter_id,
                &ctx,
                &mut self.rng,
            );
        } else {
            self.particles.create_particle_emitter(
                i,
                part_index,
                offset,
                emitter_id,
                &ctx,
                &mut self.rng,
            );
        }
    }

    /// Queue a `PhysicsScript` by its own
    /// `DataId`, with no cell test and no table lookup.
    ///
    /// The entry point reads the DataID argument, rejects zero immediately, and otherwise queues
    /// that script through the script manager.
    ///
    /// The original client allocates its script manager lazily; this driver owns one as a
    /// field. The zero-id rejection still runs before any queue operation.
    pub fn play_script_internal(&mut self, id: DataId) -> bool {
        if id == DataId(0) {
            return false;
        }
        self.scripts.add_script(id, &*self.assets, self.cur_time)
    }

    /// Play a script by DataID — the `0xF754 Effects_PlayScriptID` entry
    /// point.
    ///
    /// **An object that is not in a cell answers `true`.** That is the machine's answer, not a
    /// convenience: the client loads the cell from the object and on null runs
    /// the default script; the arm that does have
    /// a cell reaches the same code by a **tail** jump, so a call-graph search that
    /// only counts `call` sites under-reports `play_script_internal`'s callers by exactly this
    /// one.
    pub fn play_script_id(&mut self, id: DataId) -> bool {
        if !self.env.in_cell {
            return true;
        }
        self.play_script_internal(id)
    }

    /// Play a script by script type and modifier — the `0xF755
    /// Effects_PlayScriptType` entry point, and the one every spell impact, level-up, splatter
    /// and buff arrives through.
    ///
    /// The entry point returns success when the object has no cell, returns failure when it has no
    /// script table, otherwise resolves the script by type and modifier and queues it by DataID.
    ///
    /// Note the asymmetry retail shows and a summary loses: **no cell is success
    /// (1); no table is failure (0)**.
    pub fn play_script_type(&mut self, script_type: u32, intensity: f32) -> bool {
        if !self.env.in_cell {
            return true;
        }
        let Some(table_id) = self.script_table else {
            return false;
        };
        let Some(table) = self.assets.script_table(table_id) else {
            return false;
        };
        match crate::script::get_script(&table, script_type, intensity) {
            Some(id) => self.play_script_internal(id),
            None => false,
        }
    }

    /// Resolve the object's own default play-script type and
    /// intensity through its physics script table, in file order, with no randomisation.
    ///
    /// **It returns retail's value:** 1 for "no cell", 0 for "no `PhysicsScriptTable`", and
    /// otherwise whatever `play_script_internal` answers. Default-script playback is a caller, and
    /// a collision that queues nothing has to be distinguishable from one that queues a script.
    pub fn play_default_script(&mut self, part: Option<u32>) -> bool {
        self.events.push(AnimEvent::PlayDefaultScript { part });
        if part.is_some() {
            return true;
        }
        // The rest of that entry point *is* `play_script(default_script,
        // default_script_intensity)` — the cell test, the table test and `GetScript` in that
        // order — so it is that call now rather than a second copy of the same three steps.
        self.play_script_type(self.default_script, self.default_script_intensity)
    }

    /// Place every part from the current animation frame.
    ///
    /// `world` is also remembered as the frame the object-level particle emitters hang off (see
    /// [`Self::particle_parent_frame`]), so the object's particles and its parts are always in the
    /// one space the caller places parts in.
    pub fn update_parts(&mut self, world: &Frame) {
        self.placed_frame = Some(*world);
        self.part_array.update_parts(world, &self.sequence);
    }

    /// Place every part as a body drawn between its animation's keyframes is posed, `ahead`
    /// seconds on from where its sequence was last advanced
    /// ([`crate::parts::PartArray::update_parts_between`]). The sequence is not advanced.
    pub fn update_parts_between(&mut self, world: &Frame, ahead: f64) {
        self.placed_frame = Some(*world);
        self.part_array
            .update_parts_between(world, &self.sequence, ahead);
    }

    /// The frame an emitter that hangs off the object itself, rather than one of its parts,
    /// follows: the frame the parts were last placed with.
    ///
    /// A part emitter reads its part's placed frame, so an object emitter must read the frame the
    /// parts were placed *from*, or the two kinds of emitter on one object end up in different
    /// spaces. [`MotionEnv::position`] is not that frame when the caller places parts somewhere
    /// other than the object's own landblock (a renderer that draws relative to the viewer's
    /// landblock does): its origin is relative to the object's landblock, and particles born there
    /// would draw at the same landblock-local spot in whichever landblock the space is centred
    /// on. Before the first placement there is nothing else to follow, so it falls back to it.
    #[must_use]
    pub fn particle_parent_frame(&self) -> Frame {
        self.placed_frame.unwrap_or(self.env.position.frame)
    }

    /// The received-movement handler's `MovementType::Invalid` arm — the whole of
    /// what a server-sent movement buffer does to a **remote** object.
    ///
    /// This is `0xF74C Movement_SetObjectMovement`'s handling. The style change comes first and
    /// only when the style actually differs.
    ///
    /// **Why the comparison matters.** Reissuing the current style
    /// does not restart its animation: on the Aluvian table a repeated NonCombat command
    /// leaves the sequence node list, `curr`, `first_cyclic` and `frame_number` unchanged.
    /// A mutation deleting the comparison therefore survives tests of those four values.
    /// However, issuing an interpreted motion queues a pending entry even when the motion
    /// table does nothing, and an approach cannot take its first step while an entry is
    /// outstanding. The guard protects **move-to**, not animation; asserting
    /// `pending_motions.len()` detects the mutation.
    ///
    /// Resolve the style index, issue the style with default parameters only when it differs
    /// from `interpreted_state.current_style`, then apply the interpreted movement state.
    ///
    /// The packet host still cancels move-to and unsticks the object before this call.
    /// Cancellation caused by issuing the motion or applying its state is resolved
    /// synchronously through the shared `MovementManager`; no late cancellation is emitted.
    ///
    /// `is_the_player` gates one rule inside: the player never replays his own autonomous actions
    /// echoed back by the server.
    ///
    /// The application decodes the buffer
    /// (`dereth_protocol::movement::MovementBuffer`) and resolves its command **indices** through
    /// [`MotionCommand::from_index`], and then needs one entry point that does what the client
    /// does with the result.
    pub fn unpack_interpreted_movement(
        &mut self,
        style: MotionCommand,
        state: &crate::motion::InterpretedMotionState,
        is_the_player: bool,
    ) {
        self.apply_movement_style(style);
        self.move_to_interpreted_state(state, is_the_player);
    }

    /// The received-movement handler's **pre-switch half**, on its own.
    ///
    /// The buffer's second `u16` resolves through the command-id table and is compared
    /// with `interpreted_state.current_style`. When they differ, the handler issues that
    /// style with default `MovementParameters`. **Only then** does it dispatch on the
    /// buffer's low byte.
    ///
    /// So this runs for *every* buffer type, `MoveTo` and `TurnTo` included, and it is the only
    /// thing in a `MoveTo`/`TurnTo` buffer that can change a stance: those arms carry no
    /// `InterpretedMotionState` at all. Split out because the local player takes the
    /// `MoveTo` arms through a different path from [`Self::unpack_interpreted_movement`]'s.
    pub fn apply_movement_style(&mut self, style: MotionCommand) {
        if self.movement.interp.interpreted_state.current_style == style {
            return;
        }
        self.with_movement(|movement, ctx| {
            movement.do_motion(style, &crate::motion::MovementParameters::default(), ctx);
        });
    }

    /// Apply the movement-unpack `case 0` body by moving to an interpreted state.
    ///
    /// Split out for the same reason as [`Self::apply_movement_style`]: the
    /// local player reaches it from `world.rs::apply_player_movement`, which has already applied
    /// the style word for every arm.
    pub fn move_to_interpreted_state(
        &mut self,
        state: &crate::motion::InterpretedMotionState,
        is_the_player: bool,
    ) {
        self.with_movement(|movement, ctx| {
            movement.move_to_interpreted_state(state, is_the_player, ctx);
        });
    }

    /// The enter-world animation tail performs these two steps:
    ///
    /// ```text
    ///   if a part array exists, remove its link animations
    ///   if a movement manager exists, handle enter-world
    /// ```
    ///
    /// Object creation reaches it for a created object with `pd.parent_id == 0` and a position,
    /// after storing the position and applying the descriptor's own movement buffer. That order is
    /// the whole point: whatever
    /// motion the create carried has already been turned into a sequence, and this drops its
    /// **link** (non-cyclic) animations, so the object stands in the motion's cycle instead of
    /// playing its way into it.
    ///
    /// Removing link animations is the operation that changes the sequence; the movement
    /// manager forwards the enter-world notification to that operation.
    ///
    /// `WorldScene` gives server-created objects a `MotionDriver`, so this path matters: a corpse
    /// arrives with the dead creature's
    /// `CurrentMotionState` (`MotionCommand::Dead`) in its `PhysicsDesc`; without this tail,
    /// it plays the 64-frame fall-over again after the creature has already played it
    /// from its own `0xF74C` movement message.
    pub fn enter_world(&mut self) {
        self.remove_link_animations();
    }

    /// Removing the link animations reaches the part array. This is also the final part-array
    /// operation on both edges of hiding an object.
    pub fn remove_link_animations(&mut self) {
        self.with_movement(|movement, ctx| movement.remove_link_animations(ctx));
    }

    /// Convenience for physics and for the tests: issue one interpreted motion.
    pub fn do_interpreted_motion(
        &mut self,
        cmd: MotionCommand,
        params: &crate::motion::MovementParameters,
    ) -> u32 {
        self.with_movement(|movement, ctx| {
            let err = movement.do_interpreted_motion(cmd, params, ctx);
            movement.drain_completed_motions(ctx);
            err
        })
    }
}

impl MotionSource for MotionDriver {
    /// The part array's update, which drives the animation sequence's own update.
    fn advance(&mut self, quantum: f64) -> Frame {
        let mut offset = Frame::default();
        let mut queued = Vec::new();
        self.sequence
            .update(quantum, Some(&mut offset), &mut queued);
        self.hooks.extend(queued);
        offset
    }

    /// The movement tick, the part array's movement handling and then the position manager's
    /// tick, after the object's move for this quantum.
    fn tick_movement(&mut self, now: LocalTime) {
        self.env.cur_time = now;
        // The movement tick's tail, in retail order:
        //
        // ```text
        //   check detection
        //   handle targeting
        //   update movement time
        //   handle part-array movement
        //   update position time
        // ```
        //
        // Targetting runs *here*, every sub-step, immediately before `UseTime` — not once per
        // frame after the ladder, where a `TargetInfo` would reach the manager one frame late and
        // the walk would start a frame after the update that should have started it.
        self.handle_targetting(now);
        self.with_movement(|movement, ctx| {
            // movement -> part-array -> position manager.
            movement.use_movement_time(ctx);
            movement.drain_use_time(ctx);
            movement.use_position_time(ctx);
        });
    }

    fn hit_ground(&mut self) {
        self.with_movement(|movement, ctx| movement.hit_ground(ctx));
    }

    fn leave_ground(&mut self) {
        self.with_movement(|movement, ctx| movement.leave_ground(ctx));
    }

    /// Execute every queued animation hook in order, then clear the queue. A hook queued twice
    /// executes twice.
    fn process_hooks(&mut self) {
        for h in self.hooks.take() {
            self.execute_hook(h);
        }
    }

    /// True when the object **has** collision geometry. `UpdateObjectInternal` teleports straight
    /// to the new frame when it does not.
    fn has_collision_geometry(&self) -> bool {
        self.part_array.setup.is_some() && !self.part_array.spheres().is_empty()
    }

    /// The motion interpreter's adjusted and plain maximum speeds.
    ///
    /// The driver owns its movement manager, so the answer is always `Some`. The original
    /// null-manager arm represents a body with no motion source; such a body has no
    /// `MotionSource` in this build either. The arithmetic stays in [`crate::MotionInterp`]:
    /// physics asks for the result instead of reimplementing it.
    fn motion_max_speed(&self, use_adjusted: bool) -> Option<f32> {
        let interp = &self.movement.interp;
        Some(if use_adjusted {
            interp.get_adjusted_max_speed(&self.env)
        } else {
            interp.get_max_speed(&self.env)
        })
    }

    /// Whether the object has a movement manager and that manager says a `MoveTo` is in flight.
    fn is_moving_to(&self) -> bool {
        self.movement.is_moving_to()
    }
}

/// One animation-only sub-step of the world update.
///
/// `advance` updates the part array and sequence, `tick_movement` updates movement and then handles
/// part movement, and `process_hooks` executes deferred animation hooks. The order is the client's.
///
/// Every type it names -- [`MotionDriver`], [`MotionSource`] and [`LocalTime`] -- is this crate's
/// or `dereth-primitives`', so this crate owns it.
pub fn step_animation(driver: &mut MotionDriver, quantum: f64, now: LocalTime) {
    let _offset = MotionSource::advance(driver, quantum);
    driver.tick_movement(now);
    driver.process_hooks();
    driver.update_scripts();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{AnimFrame, AnimationData, MapAssets, Sphere};
    use dereth_primitives::{ObjectId, Vec3};

    #[test]
    fn scoped_movement_preserves_old_physics_effects_and_publishes_the_new_target_inline() {
        let mut driver = MotionDriver::new(Arc::new(crate::data::NoAssets));
        let prefix = vec![
            MotionEffect::SetHeading(37.0),
            MotionEffect::SetLocalVelocity(Vec3::new(1.0, 2.0, 3.0)),
        ];
        driver.effects = prefix.clone();
        let old_event = AnimEvent::MotionDone {
            motion: MotionCommand::READY,
            success: false,
        };
        let new_event = AnimEvent::MotionDone {
            motion: MotionCommand::READY,
            success: true,
        };
        driver.events.push(old_event);
        let (result, emitted) = driver.with_movement_scoped_effects(|movement, ctx| {
            movement.stick_to_object(ObjectId(0x7000_1041), 0.4, 8.0, ctx);
            // Explicit external-effect station; this tests the host queue boundary rather than
            // claiming that a particular real animation naturally emits this test heading.
            ctx.effects.push(MotionEffect::SetHeading(82.0));
            ctx.events.push(new_event);
            movement.sticky.target_id
        });
        assert_eq!(
            result,
            ObjectId(0x7000_1041),
            "actual new sticky command result"
        );
        assert_eq!(
            emitted,
            vec![MotionEffect::SetHeading(82.0)],
            "only the new call's output"
        );
        assert_eq!(
            driver.effects, prefix,
            "unrelated older effects keep their drain phase"
        );
        assert_eq!(driver.target.map(|t| t.id), Some(ObjectId(0x7000_1041)));
        assert_eq!(driver.movement.sticky.target_id, ObjectId(0x7000_1041));
        assert!(!driver.movement.motions_pending());
        assert!(!driver.motion_table.has_pending());
        assert_eq!(driver.take_events(), vec![old_event, new_event]);
        assert_eq!(driver.take_effects(), prefix);
    }

    fn driver_with_anim() -> MotionDriver {
        let mut assets = MapAssets::default();
        let id = DataId(0x0300_0001);
        assets.animations.insert(
            id.0,
            Arc::new(AnimationData {
                num_frames: 4,
                num_parts: 1,
                pos_frames: None,
                part_frames: (0..4)
                    .map(|i| AnimFrame {
                        frames: vec![Frame::default()],
                        hooks: if i == 1 {
                            vec![AnimHook::new(0, HookKind::SetEtherealForTest())]
                        } else {
                            Vec::new()
                        },
                    })
                    .collect(),
                has_hooks: true,
            }),
        );
        let mut d = MotionDriver::new(Arc::new(assets));
        d.sequence.append_animation(
            crate::data::AnimData {
                anim_id: id,
                low_frame: 0,
                high_frame: 3,
                framerate: 30.0,
            },
            &*d.assets,
        );
        d
    }

    // A tiny helper so the test above reads clearly.
    impl HookKind {
        #[allow(non_snake_case)]
        fn SetEtherealForTest() -> Self {
            Self::Ethereal { ethereal: 1 }
        }
    }

    /// `advance` returns the raw animation offset, **unscaled** and
    /// **not zeroed**, because both of those depend on physics state the caller owns.
    #[test]
    fn advance_returns_the_raw_offset_without_scaling_or_zeroing_it() {
        let mut d = driver_with_anim();
        d.scale = 10.0;
        d.sequence.set_velocity(Vec3::new(0.0, 3.0, 0.0));
        let f = d.advance(2.0 / 30.0);
        // Two frames crossed at 1/30 s each: 0.2 m, not 2.0 m.
        assert!((f.origin.y - 0.2).abs() < 1e-5, "{:?}", f.origin);
    }

    /// Hooks are deferred: `advance` queues them and only `process_hooks` executes them.
    #[test]
    fn hooks_are_queued_by_advance_and_executed_by_process_hooks() {
        let mut d = driver_with_anim();
        d.advance(3.0 / 30.0);
        assert_eq!(d.hooks.len(), 1, "queued, not executed");
        assert!(d.take_events().is_empty());
        d.process_hooks();
        assert_eq!(d.take_events(), vec![AnimEvent::SetEthereal(true)]);
        assert!(d.hooks.is_empty(), "the array is emptied wholesale");
    }

    /// `has_collision_geometry` is false without a setup and false with a setup that has no
    /// spheres — which is the branch that makes `UpdateObjectInternal` teleport.
    #[test]
    fn collision_geometry_needs_a_setup_and_at_least_one_sphere() {
        let mut d = MotionDriver::default();
        assert!(!d.has_collision_geometry());
        let mut seq = Sequence::new();
        d.part_array = PartArray::create_setup(
            Arc::new(SetupData {
                parts: vec![DataId(0x0100_0001)],
                ..SetupData::default()
            }),
            true,
            &mut seq,
            &NoAssets,
        )
        .expect("setup");
        assert!(!d.has_collision_geometry(), "no spheres");
        d.part_array = PartArray::create_setup(
            Arc::new(SetupData {
                parts: vec![DataId(0x0100_0001)],
                spheres: vec![Sphere {
                    center: Vec3::ZERO,
                    radius: 1.0,
                }],
                ..SetupData::default()
            }),
            true,
            &mut seq,
            &NoAssets,
        )
        .expect("setup");
        assert!(d.has_collision_geometry());
    }

    /// A **script** hook executes inline in `update_scripts`, while an
    /// **animation** hook from the same frame waits for `process_hooks`.
    #[test]
    fn script_hooks_are_inline_and_animation_hooks_are_deferred() {
        let mut d = driver_with_anim();
        let script = Arc::new(crate::data::PhysicsScriptData {
            steps: vec![crate::data::ScriptStep {
                start_time: 0.0,
                hook: AnimHook::new(0, HookKind::SetLight { lights_on: 1 }),
            }],
            length: 0.0,
        });
        d.scripts.add_script_internal(script, d.cur_time);

        d.advance(3.0 / 30.0); // queues the animation hook
        d.update_scripts(); // executes the script hook immediately
        let after_scripts = d.take_events();
        assert_eq!(after_scripts, vec![AnimEvent::SetLights(true)]);

        d.process_hooks();
        assert_eq!(d.take_events(), vec![AnimEvent::SetEthereal(true)]);
    }

    /// An emitter hung on the object itself is born at, and keeps emitting from, the frame the
    /// parts were placed from, not the object's landblock-local position. The two differ whenever
    /// the host places parts relative to another landblock, and then a landblock-local birth
    /// frame draws the particles at the same local spot in the host's landblock, away from the
    /// object.
    #[test]
    fn an_object_emitter_follows_the_frame_the_parts_were_placed_from() {
        use dereth_primitives::{CellId, Position, Quat};
        const EMITTER: u32 = 0x3200_0001;
        let mut info = crate::data::ParticleEmitterInfo {
            particle_type: crate::data::ParticleType::Still,
            hw_gfxobj_id: DataId(0x0100_0001),
            emitter_type: crate::data::BIRTHRATE_PER_SEC,
            birthrate: 0.1,
            max_particles: 8,
            initial_particles: 1,
            lifespan: 10.0,
            is_parent_local: 0,
            ..crate::data::ParticleEmitterInfo::default()
        };
        info.init_end();
        let mut assets = MapAssets::default();
        assets.emitters.insert(EMITTER, Arc::new(info));
        let mut d = MotionDriver::new(Arc::new(assets));
        // The object stands at (10, 20, 30) in its own landblock; the host draws relative to the
        // landblock one to the east, so it places the parts 192 m west of that.
        d.env.position = Position::new(
            CellId(0xA9B4_0001),
            Frame::new(Vec3::new(10.0, 20.0, 30.0), Quat::IDENTITY),
        );
        let placed = Frame::new(Vec3::new(10.0 - 192.0, 20.0, 30.0), Quat::IDENTITY);
        d.update_parts(&placed);

        d.create_emitter(DataId(EMITTER), NO_PART, Frame::default(), 0, false);
        d.cur_time = ServerTime(1.0);
        d.update_particles(true);

        let origins: Vec<Vec3> = d
            .particles
            .iter()
            .flat_map(|e| e.live().map(|p| p.frame.origin))
            .collect();
        assert!(
            origins.len() >= 2,
            "the initial particle and at least one emitted after it: {origins:?}"
        );
        for o in &origins {
            assert_eq!(
                *o, placed.origin,
                "every particle is at the placed object, not at its landblock-local position"
            );
        }
        let e = d.particles.iter().next().expect("the emitter");
        assert_eq!(e.object_origin, placed.origin);
    }

    /// The driver satisfies the shared motion source trait.
    #[test]
    fn the_driver_satisfies_the_shared_motion_source_trait() {
        fn takes(_: &mut dyn MotionSource) {}
        fn debuggable<T: std::fmt::Debug>(_: &T) {}
        // And `Send`, so a host can move a world of drivers to another thread.
        fn sendable<T: Send>(_: &T) {}
        let mut d = MotionDriver::default();
        takes(&mut d);
        debuggable(&d);
        sendable(&d);
        // The five other methods are callable and do not panic on an empty object.
        d.tick_movement(LocalTime(1.0));
        d.hit_ground();
        d.leave_ground();
        d.process_hooks();
        assert!(!d.has_collision_geometry());
    }
}
