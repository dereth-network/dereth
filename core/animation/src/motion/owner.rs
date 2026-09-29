//! Callback-capable movement operations over authoritative, field-borrowed state.
//!
//! Method relocation from MotionInterp/MoveToManager preserves their public isolated adapters.
//! Owned calls can reenter the same interpreter, MoveTo and Sticky workers at retail call sites.
//! No worker is copied, replaced, or borrowed across a callback into another worker.
//!
//! Primary: the motion interpreter, the move-to manager, the motion-table manager and the
//! sticky manager.
//! Existing public adapters retain their per-method source notes.
//! The relocated `moveto_*` methods are the MoveToManager methods of the same suffix.
//! Completion is begin -> synchronous owner -> current-head continuation, including reentry.
use super::interp::*;
use super::moveto::*;
use super::{flags, MotionEffect, MotionInterp, MoveToManager, MovementParameters, StickyManager};
use crate::command::MotionCommand;
use crate::frame::{get_heading, set_heading, V3};
use crate::hooks::AnimEvent;
use crate::table::{MovementStruct, MovementType};
use dereth_primitives::num::consts::EPSILON;
use dereth_primitives::{ObjectId, Position};

pub(crate) struct MotionOwner<'a> {
    pub(crate) interp: &'a mut MotionInterp,
    pub(crate) moveto: &'a mut MoveToManager,
    pub(crate) sticky: &'a mut StickyManager,
    pub(crate) owns_moveto: bool,
    pub(crate) owns_sticky: bool,
}

impl MotionOwner<'_> {
    pub(crate) fn cancel_move_to(&mut self, err: u32, ctx: &mut MotionCtx<'_>) {
        if self.owns_moveto {
            self.moveto_cancel_move_to(err, ctx);
        } else {
            ctx.effects.push(MotionEffect::CancelMoveTo);
        }
    }

    pub(crate) fn unstick_from_object(&mut self, ctx: &mut MotionCtx<'_>) {
        if !self.owns_sticky {
            ctx.effects.push(MotionEffect::UnstickFromObject);
        } else if self.sticky.clear_stick() {
            // UnStick: clear state/subscription before reentering cancellation.
            ctx.effects.push(MotionEffect::ClearTarget);
            self.cancel_move_to(ACTION_CANCELLED, ctx);
        }
    }

    pub(crate) fn stick_to_object(
        &mut self,
        id: ObjectId,
        radius: f32,
        height: f32,
        now: f64,
        ctx: &mut MotionCtx<'_>,
    ) {
        if !self.owns_sticky {
            ctx.effects
                .push(MotionEffect::StickTo { id, radius, height });
        } else {
            self.unstick_from_object(ctx);
            // The position manager ignores height in its sticky-manager arm.
            self.sticky.stick_to_after_unstick(id, radius, now, ctx);
        }
    }

    pub(crate) fn position_use_time(&mut self, ctx: &mut MotionCtx<'_>) {
        if self.sticky.is_sticky() && self.sticky.sticky_timeout_time < ctx.env.cur_time.0 {
            self.unstick_from_object(ctx);
        }
    }

    pub(crate) fn handle_sticky_update_target(
        &mut self,
        info: &TargetInfo,
        ctx: &mut MotionCtx<'_>,
    ) {
        if info.object_id != self.sticky.target_id {
            return;
        }
        if !info.ok {
            self.unstick_from_object(ctx);
        } else {
            self.sticky.handle_update_target(
                info.object_id,
                true,
                info.target_position,
                ctx.env.cur_time.0,
                ctx,
            );
        }
    }

    pub fn motion_done(&mut self, _success: bool, ctx: &mut MotionCtx<'_>) {
        let Some(head) = self.interp.pending_motions.front().copied() else {
            return;
        };
        if head.motion.is_action() {
            self.unstick_from_object(ctx);
            self.interp.interpreted_state.remove_action();
            self.interp.raw_state.remove_action();
        }
        self.interp.pending_motions.pop_front();
    }

    pub fn drain_completed_motions(&mut self, ctx: &mut MotionCtx<'_>) {
        while let Some(head) = ctx.mgr.begin_completion(false) {
            ctx.events.push(AnimEvent::MotionDone {
                motion: head.motion,
                success: true,
            });
            self.motion_done(true, ctx);
            ctx.mgr.finish_completion(None);
        }
    }

    pub fn drain_use_time(&mut self, ctx: &mut MotionCtx<'_>) {
        self.drain_completed_motions(ctx);
    }

    pub fn drain_animation_done(&mut self, success: bool, ctx: &mut MotionCtx<'_>) {
        if !ctx.mgr.begin_animation_done() {
            return;
        }
        while let Some(head) = ctx.mgr.begin_completion(true) {
            ctx.events.push(AnimEvent::MotionDone {
                motion: head.motion,
                success,
            });
            self.motion_done(success, ctx);
            ctx.mgr.finish_completion(Some(&head));
        }
        ctx.mgr.finish_animation_done();
    }

    pub fn remove_link_animations(&mut self, ctx: &mut MotionCtx<'_>) {
        ctx.seq.remove_all_link_animations();
        while ctx.mgr.has_pending() {
            self.drain_animation_done(false, ctx);
        }
    }

    pub fn handle_exit_world(&mut self, ctx: &mut MotionCtx<'_>) {
        while self.interp.motions_pending() {
            self.motion_done(false, ctx);
        }
    }

    pub fn do_motion(
        &mut self,
        cmd: MotionCommand,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) -> u32 {
        if params.has(flags::CANCEL_MOVETO) {
            self.cancel_move_to(ACTION_CANCELLED, ctx);
        }
        self.do_motion_after_cancel(cmd, params, ctx)
    }

    pub(crate) fn do_motion_after_cancel(
        &mut self,
        cmd: MotionCommand,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) -> u32 {
        let original = cmd;
        // A local copy, so `adjust_motion` cannot alter the caller's parameters.
        let mut p = *params;
        if params.has(flags::SET_HOLD_KEY) {
            self.interp.set_hold_key(params.hold_key_to_apply);
        }
        let mut cmd = cmd;
        self.interp
            .adjust_motion(&mut cmd, &mut p.speed, params.hold_key_to_apply, ctx.env);

        if self.interp.interpreted_state.current_style != MotionCommand::NON_COMBAT {
            if cmd == MotionCommand::CROUCH {
                return CANT_CROUCH_IN_COMBAT;
            }
            if cmd == MotionCommand::SITTING {
                return CANT_SIT_IN_COMBAT;
            }
            if cmd == MotionCommand::SLEEPING {
                return CANT_LIE_DOWN_IN_COMBAT;
            }
            if cmd.is_emote() {
                return CANT_CHAT_EMOTE_IN_COMBAT;
            }
        }
        if cmd.is_action() && self.interp.interpreted_state.num_actions() > 5 {
            return TOO_MANY_ACTIONS;
        }
        let err = self.do_interpreted_motion(cmd, &p, ctx);
        if err == 0 && params.has(flags::MODIFY_RAW_STATE) {
            self.interp.raw_state.apply_motion(original, params);
        }
        err
    }

    pub fn do_interpreted_motion(
        &mut self,
        cmd: MotionCommand,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) -> u32 {
        let state_only = |s: &mut Self| {
            if params.has(flags::MODIFY_INTERPRETED_STATE) {
                s.interp.interpreted_state.apply_motion(cmd, params);
            }
            0
        };

        let err = if !MotionInterp::contact_allows_move(cmd, ctx.env) {
            if cmd.is_action() {
                // Actions are refused outright when airborne.
                YOU_CANT_JUMP_WHILE_IN_THE_AIR
            } else {
                state_only(self)
            }
        } else if self.interp.standing_longjump
            && (cmd == MotionCommand::WALK_FORWARD
                || cmd == MotionCommand::RUN_FORWARD
                || cmd == MotionCommand::SIDE_STEP_RIGHT)
        {
            // A charged jump freezes lateral motion.
            state_only(self)
        } else {
            if cmd == MotionCommand::DEAD {
                self.remove_link_animations(ctx);
            }
            let r = ctx.mgr.perform_movement(
                &MovementStruct {
                    kind: MovementType::InterpretedCommand,
                    motion: cmd,
                    speed: params.speed,
                },
                ctx.seq,
                ctx.assets,
            );
            match r {
                Ok(()) => {
                    let mut jerr = if params.has(flags::DISABLE_JUMP_DURING_LINK) {
                        YOU_CANT_JUMP_FROM_THIS_POSITION
                    } else {
                        motion_allows_jump(cmd)
                    };
                    if jerr == 0 && !cmd.is_action() {
                        jerr = motion_allows_jump(self.interp.interpreted_state.forward_command);
                    }
                    self.interp.add_to_queue(params.context_id, cmd, jerr);
                    if params.has(flags::MODIFY_INTERPRETED_STATE) {
                        self.interp.interpreted_state.apply_motion(cmd, params);
                    }
                    0
                }
                Err(e) => e,
            }
        };
        if !ctx.env.in_cell {
            self.remove_link_animations(ctx);
        }
        err
    }

    pub fn stop_interpreted_motion(
        &mut self,
        cmd: MotionCommand,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) -> u32 {
        let lateral = cmd == MotionCommand::WALK_FORWARD
            || cmd == MotionCommand::RUN_FORWARD
            || cmd == MotionCommand::SIDE_STEP_RIGHT;
        let err = if !MotionInterp::contact_allows_move(cmd, ctx.env)
            || (self.interp.standing_longjump && lateral)
        {
            if params.has(flags::MODIFY_INTERPRETED_STATE) {
                self.interp.interpreted_state.remove_motion(cmd);
            }
            0
        } else {
            let r = ctx.mgr.perform_movement(
                &MovementStruct {
                    kind: MovementType::StopInterpretedCommand,
                    motion: cmd,
                    speed: params.speed,
                },
                ctx.seq,
                ctx.assets,
            );
            match r {
                Ok(()) => {
                    self.interp
                        .add_to_queue(params.context_id, MotionCommand::READY, 0);
                    if params.has(flags::MODIFY_INTERPRETED_STATE) {
                        self.interp.interpreted_state.remove_motion(cmd);
                    }
                    0
                }
                Err(e) => e,
            }
        };
        if !ctx.env.in_cell {
            self.remove_link_animations(ctx);
        }
        err
    }

    pub fn stop_motion(
        &mut self,
        cmd: MotionCommand,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) -> u32 {
        if params.has(flags::CANCEL_MOVETO) {
            self.cancel_move_to(ACTION_CANCELLED, ctx);
        }
        self.stop_motion_after_cancel(cmd, params, ctx)
    }

    pub(crate) fn stop_motion_after_cancel(
        &mut self,
        cmd: MotionCommand,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) -> u32 {
        let original = cmd;
        let mut p = *params;
        let mut cmd = cmd;
        self.interp
            .adjust_motion(&mut cmd, &mut p.speed, params.hold_key_to_apply, ctx.env);
        let err = self.stop_interpreted_motion(cmd, &p, ctx);
        if err == 0 && params.has(flags::MODIFY_RAW_STATE) {
            self.interp.raw_state.remove_motion(original);
        }
        err
    }

    pub fn stop_completely(&mut self, ctx: &mut MotionCtx<'_>) -> u32 {
        self.cancel_move_to(ACTION_CANCELLED, ctx);
        self.stop_completely_after_cancel(ctx)
    }

    /// The state/table half of StopCompletely, after its synchronous cancel_moveto call.
    /// MoveToManager already owns both managers while making that call; deferring cancellation
    /// until after it has initialized a replacement approach would cancel the new approach.
    pub(crate) fn stop_completely_after_cancel(&mut self, ctx: &mut MotionCtx<'_>) -> u32 {
        let jerr = motion_allows_jump(self.interp.interpreted_state.forward_command);
        self.interp.raw_state.forward_command = MotionCommand::READY;
        self.interp.interpreted_state.forward_command = MotionCommand::READY;
        self.interp.raw_state.forward_speed = 1.0;
        self.interp.interpreted_state.forward_speed = 1.0;
        self.interp.raw_state.sidestep_command = MotionCommand::NONE;
        self.interp.raw_state.turn_command = MotionCommand::NONE;
        self.interp.interpreted_state.sidestep_command = MotionCommand::NONE;
        self.interp.interpreted_state.turn_command = MotionCommand::NONE;
        let _ = ctx.mgr.perform_movement(
            &MovementStruct {
                kind: MovementType::StopCompletely,
                motion: MotionCommand::NONE,
                speed: 1.0,
            },
            ctx.seq,
            ctx.assets,
        );
        self.interp.add_to_queue(0, MotionCommand::READY, jerr);
        if !ctx.env.in_cell {
            self.remove_link_animations(ctx);
        }
        0
    }

    pub fn enter_default_state(&mut self, ctx: &mut MotionCtx<'_>) {
        self.interp.raw_state = RawMotionState::default();
        self.interp.interpreted_state = InterpretedMotionState::default();
        ctx.mgr.initialize_state(ctx.seq, ctx.assets);
        self.interp.pending_motions.push_back(MotionNode {
            context_id: 0,
            motion: MotionCommand::READY,
            jump_error_code: 0,
        });
        self.interp.initted = true;
        self.leave_ground(ctx);
    }

    pub fn hit_ground(&mut self, ctx: &mut MotionCtx<'_>) {
        if !(ctx.env.is_creature && ctx.env.gravity_affected) {
            return;
        }
        self.remove_link_animations(ctx);
        self.apply_current_movement(false, false, ctx);
    }

    pub fn leave_ground(&mut self, ctx: &mut MotionCtx<'_>) {
        if !(ctx.env.is_creature && ctx.env.gravity_affected) {
            return;
        }
        let v = self.interp.get_leave_ground_velocity(ctx.env);
        ctx.effects.push(MotionEffect::SetLocalVelocity(v));
        self.interp.standing_longjump = false;
        self.interp.jump_extent = 0.0;
        self.remove_link_animations(ctx);
        self.apply_current_movement(false, false, ctx);
    }

    pub fn jump(&mut self, extent: f32, ctx: &mut MotionCtx<'_>) -> u32 {
        self.cancel_move_to(ACTION_CANCELLED, ctx);
        self.jump_after_cancel(extent, ctx)
    }

    pub(crate) fn jump_after_cancel(&mut self, extent: f32, ctx: &mut MotionCtx<'_>) -> u32 {
        let err = self.interp.jump_is_allowed(ctx.env);
        if err == 0 {
            self.interp.jump_extent = extent;
            ctx.effects.push(MotionEffect::SetOnWalkable(false));
            return 0;
        }
        self.interp.standing_longjump = false;
        err
    }

    pub fn apply_interpreted_movement(
        &mut self,
        link: bool,
        longjump_hint: bool,
        ctx: &mut MotionCtx<'_>,
    ) {
        let mut p = MovementParameters {
            flags: (MovementParameters::default().flags & 0xFFFD_37FF)
                | (u32::from(link) << 15)
                | (u32::from(longjump_hint) << 17),
            ..MovementParameters::default()
        };
        if self.interp.interpreted_state.forward_command == MotionCommand::RUN_FORWARD {
            self.interp.my_run_rate = self.interp.interpreted_state.forward_speed;
        }
        let style = self.interp.interpreted_state.current_style;
        self.do_interpreted_motion(style, &p, ctx);

        let fwd = self.interp.interpreted_state.forward_command;
        if !MotionInterp::contact_allows_move(fwd, ctx.env) {
            p.speed = 1.0;
            self.do_interpreted_motion(MotionCommand::FALLING, &p, ctx);
        } else if self.interp.standing_longjump {
            p.speed = 1.0;
            self.do_interpreted_motion(MotionCommand::READY, &p, ctx);
            self.stop_interpreted_motion(MotionCommand::SIDE_STEP_RIGHT, &p, ctx);
        } else {
            p.speed = self.interp.interpreted_state.forward_speed;
            self.do_interpreted_motion(fwd, &p, ctx);
            if self.interp.interpreted_state.sidestep_command == MotionCommand::NONE {
                self.stop_interpreted_motion(MotionCommand::SIDE_STEP_RIGHT, &p, ctx);
            } else {
                p.speed = self.interp.interpreted_state.sidestep_speed;
                let side = self.interp.interpreted_state.sidestep_command;
                self.do_interpreted_motion(side, &p, ctx);
            }
        }
        if self.interp.interpreted_state.turn_command == MotionCommand::NONE {
            self.stop_interpreted_motion(MotionCommand::TURN_RIGHT, &p, ctx);
        } else {
            p.speed = self.interp.interpreted_state.turn_speed;
            let turn = self.interp.interpreted_state.turn_command;
            self.do_interpreted_motion(turn, &p, ctx);
        }
        // **The seventh call site, and this build did not have it.** The interpreted-movement
        // apply tests the object's cell pointer, and when the object is in **no** cell it calls
        // the link-animation removal. A pointer test, not an expression.
        if !ctx.env.in_cell {
            self.remove_link_animations(ctx);
        }
    }

    pub fn apply_raw_movement(&mut self, link: bool, longjump_hint: bool, ctx: &mut MotionCtx<'_>) {
        let r = self.interp.raw_state.clone();
        self.interp.interpreted_state.current_style = r.current_style;
        let mut fwd = r.forward_command;
        let mut fwd_speed = r.forward_speed;
        self.interp
            .adjust_motion(&mut fwd, &mut fwd_speed, r.forward_holdkey, ctx.env);
        let mut side = r.sidestep_command;
        let mut side_speed = r.sidestep_speed;
        if side != MotionCommand::NONE {
            self.interp
                .adjust_motion(&mut side, &mut side_speed, r.sidestep_holdkey, ctx.env);
        }
        let mut turn = r.turn_command;
        let mut turn_speed = r.turn_speed;
        if turn != MotionCommand::NONE {
            self.interp
                .adjust_motion(&mut turn, &mut turn_speed, r.turn_holdkey, ctx.env);
        }
        self.interp.interpreted_state.forward_command = fwd;
        self.interp.interpreted_state.forward_speed = fwd_speed;
        self.interp.interpreted_state.sidestep_command = side;
        self.interp.interpreted_state.sidestep_speed = side_speed;
        self.interp.interpreted_state.turn_command = turn;
        self.interp.interpreted_state.turn_speed = turn_speed;
        self.apply_interpreted_movement(link, longjump_hint, ctx);
    }

    pub fn apply_current_movement(
        &mut self,
        link: bool,
        longjump_hint: bool,
        ctx: &mut MotionCtx<'_>,
    ) {
        if !self.interp.initted {
            return;
        }
        // The client chooses between `apply_raw_movement` and `apply_interpreted_movement` on
        // whether the object has no game object or is the player, and its movement is autonomous.
        // Both of those are facts this crate does not hold — autonomous-movement state lives on
        // the physics body and player identity on the owning game object — so the caller chooses
        // directly for the autonomous player. Every internal
        // caller here is a remote or server-driven object, which is the interpreted arm.
        self.apply_interpreted_movement(link, longjump_hint, ctx);
    }

    pub fn move_to_interpreted_state(
        &mut self,
        state: &InterpretedMotionState,
        is_the_player: bool,
        ctx: &mut MotionCtx<'_>,
    ) {
        self.interp.raw_state.current_style = state.current_style;
        self.cancel_move_to(ACTION_CANCELLED, ctx);
        self.move_to_interpreted_state_after_cancel(state, is_the_player, ctx);
    }

    pub(crate) fn move_to_interpreted_state_after_cancel(
        &mut self,
        state: &InterpretedMotionState,
        is_the_player: bool,
        ctx: &mut MotionCtx<'_>,
    ) {
        let jerr = motion_allows_jump(self.interp.interpreted_state.forward_command);
        self.interp.interpreted_state.copy_movement_from(state);
        self.apply_current_movement(true, jerr != 0, ctx);

        let mut p = MovementParameters::default();
        for node in &state.actions {
            let a = node.stamp & 0x7FFF;
            let b = self.interp.server_action_stamp & 0x7FFF;
            let d = a.abs_diff(b);
            let newer = if d < 0x4000 { b < a } else { a < b };
            if !newer {
                continue;
            }
            if is_the_player && node.autonomous {
                continue; // we already played it
            }
            self.interp.server_action_stamp = node.stamp;
            p.speed = node.speed;
            p.flags = (p.flags & !flags::AUTONOMOUS) | (u32::from(node.autonomous) << 12);
            self.do_interpreted_motion(node.action, &p, ctx);
        }
    }

    pub fn perform_interpreter_movement(
        &mut self,
        kind: MovementType,
        cmd: MotionCommand,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) -> u32 {
        let err = match kind {
            MovementType::RawCommand => self.do_motion(cmd, params, ctx),
            MovementType::InterpretedCommand => self.do_interpreted_motion(cmd, params, ctx),
            MovementType::StopRawCommand => self.stop_motion(cmd, params, ctx),
            MovementType::StopInterpretedCommand => self.stop_interpreted_motion(cmd, params, ctx),
            MovementType::StopCompletely => self.stop_completely(ctx),
            _ => GENERAL_MOVEMENT_FAILURE,
        };
        self.drain_completed_motions(ctx);
        err
    }

    /// The move-to manager's own motion step -- adjust, then **`DoInterpretedMotion`**.
    pub(crate) fn moveto_do_motion(
        &mut self,
        cmd: MotionCommand,
        p: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) -> u32 {
        let mut cmd = cmd;
        let mut q = *p;
        self.interp
            .adjust_motion(&mut cmd, &mut q.speed, p.hold_key_to_apply, ctx.env);
        self.do_interpreted_motion(cmd, &q, ctx)
    }

    /// The move-to manager's own stop-motion step.
    pub(crate) fn moveto_stop_motion(
        &mut self,
        cmd: MotionCommand,
        p: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) -> u32 {
        let mut cmd = cmd;
        let mut q = *p;
        self.interp
            .adjust_motion(&mut cmd, &mut q.speed, p.hold_key_to_apply, ctx.env);
        self.stop_interpreted_motion(cmd, &q, ctx)
    }

    pub fn moveto_move_to_position(
        &mut self,
        pos: Position,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) {
        self.moveto_stop_completely(ctx);
        self.moveto.current_target_position = pos;
        self.moveto.sought_object_radius = 0.0;
        let d = if params.has(flags::USE_SPHERES) {
            cylinder_distance(
                ctx.env.radius,
                ctx.env.height,
                &ctx.env.position,
                0.0,
                0.0,
                &pos,
            )
        } else {
            distance(&ctx.env.position, &pos)
        };
        let (cmd, _hold, _away) = params.get_command(d);
        if cmd != MotionCommand::NONE {
            self.moveto.pending_actions.push_back(MovementNode {
                kind: MovementType::TurnToHeading,
                heading: position_heading(&ctx.env.position, &pos),
            });
            self.moveto.pending_actions.push_back(MovementNode {
                kind: MovementType::MoveToPosition,
                heading: 0.0,
            });
        }
        if params.has(flags::USE_FINAL_HEADING) {
            self.moveto.pending_actions.push_back(MovementNode {
                kind: MovementType::TurnToHeading,
                heading: params.desired_heading,
            });
        }
        self.moveto.sought_position = pos;
        self.moveto.starting_position = ctx.env.position;
        self.moveto.movement_type = MovementType::MoveToPosition;
        self.moveto.movement_params = *params;
        // You cannot stick to a bare position.
        self.moveto.movement_params.flags &= !flags::STICKY;
        self.moveto_begin_next_node(ctx);
    }

    pub fn moveto_perform_movement(
        &mut self,
        req: &MoveToRequest,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) {
        self.moveto_cancel_move_to(ACTION_CANCELLED, ctx);
        self.unstick_from_object(ctx);
        self.moveto_perform_movement_after_prefix(req, params, ctx);
    }

    /// The switch after the perform-movement step's cancel/unstick prefix. An owner of both managers resolves
    /// those callbacks before entering here; the isolated adapter above records them instead.
    pub(crate) fn moveto_perform_movement_after_prefix(
        &mut self,
        req: &MoveToRequest,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) {
        match *req {
            MoveToRequest::MoveToObject {
                object_id,
                top_level_id,
                radius,
                height,
            } => {
                self.moveto_move_to_object(object_id, top_level_id, radius, height, params, ctx);
            }
            MoveToRequest::MoveToPosition { pos } => {
                self.moveto_move_to_position(pos, params, ctx);
            }
            MoveToRequest::TurnToObject {
                object_id,
                top_level_id,
            } => {
                self.moveto_turn_to_object(object_id, top_level_id, params, ctx);
            }
            MoveToRequest::TurnToHeading => self.moveto_turn_to_heading(params, ctx),
        }
    }

    pub fn moveto_move_to_object(
        &mut self,
        object_id: ObjectId,
        top_level_id: ObjectId,
        radius: f32,
        height: f32,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) {
        self.moveto_stop_completely(ctx);
        self.moveto.starting_position = ctx.env.position;
        self.moveto.sought_object_id = object_id;
        self.moveto.sought_object_radius = radius;
        self.moveto.sought_object_height = height;
        self.moveto.movement_type = MovementType::MoveToObject;
        self.moveto.top_level_object_id = top_level_id;
        self.moveto.movement_params = *params;
        self.moveto.initialized = false;
        if top_level_id != ctx.env.object_id {
            ctx.effects.push(MotionEffect::SetTarget {
                id: top_level_id,
                radius: TARGET_RADIUS,
                quantum: 0.0,
            });
            return;
        }
        self.moveto_clean_up(ctx);
        self.moveto_stop_completely(ctx);
    }

    pub fn moveto_turn_to_object(
        &mut self,
        object_id: ObjectId,
        top_level_id: ObjectId,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) {
        if params.has(flags::STOP_COMPLETELY) {
            self.moveto_stop_completely(ctx);
        }
        self.moveto.movement_type = MovementType::TurnToObject;
        self.moveto.sought_object_id = object_id;
        set_heading(
            &mut self.moveto.current_target_position.frame,
            params.desired_heading,
        );
        self.moveto.top_level_object_id = top_level_id;
        self.moveto.movement_params = *params;
        if top_level_id != ctx.env.object_id {
            self.moveto.initialized = false;
            ctx.effects.push(MotionEffect::SetTarget {
                id: top_level_id,
                radius: TARGET_RADIUS,
                quantum: 0.0,
            });
            return;
        }
        self.moveto_clean_up(ctx);
        self.moveto_stop_completely(ctx);
    }

    /// The first `TargetInfo` turns the
    /// recorded target into nodes.
    ///
    /// **This is the range test.** `get_command` returns `NONE` when the current distance is
    /// already at or inside `distance_to_object` (
    /// takes the walk arm only on a strict `dist > distance_to_object`), and when it does
    /// **neither node is pushed** -- no turn, no walk. `BeginNextNode` then finds an empty queue
    /// and cleans up. That is how a use on something already in reach fires without a step.
    ///
    /// The final-heading node is composed, not copied: `bearing + desired_heading`, wrapped once
    /// at 360. [`Self::moveto_move_to_position`] pushes the raw `desired_heading` instead, and the
    /// difference is retail's.
    pub(crate) fn moveto_move_to_object_internal(
        &mut self,
        target_position: Position,
        interpolated_position: Position,
        ctx: &mut MotionCtx<'_>,
    ) {
        self.moveto.sought_position = interpolated_position;
        self.moveto.current_target_position = target_position;
        let bearing = position_heading(&ctx.env.position, &interpolated_position);
        let d = self.moveto.current_distance(ctx);
        let (cmd, _hold, _away) = self.moveto.movement_params.get_command(d);
        if cmd != MotionCommand::NONE {
            self.moveto.pending_actions.push_back(MovementNode {
                kind: MovementType::TurnToHeading,
                heading: bearing,
            });
            self.moveto.pending_actions.push_back(MovementNode {
                kind: MovementType::MoveToPosition,
                heading: 0.0,
            });
        }
        if self.moveto.movement_params.has(flags::USE_FINAL_HEADING) {
            let mut h = bearing + self.moveto.movement_params.desired_heading;
            if h >= 360.0 {
                h -= 360.0;
            }
            self.moveto.pending_actions.push_back(MovementNode {
                kind: MovementType::TurnToHeading,
                heading: h,
            });
        }
        self.moveto.initialized = true;
        self.moveto_begin_next_node(ctx);
    }

    /// The internal turn-to-object step.
    ///
    /// `heading = (bearing_to_target + sought_position's own heading) mod 360`. Nothing writes a
    /// heading onto `sought_position` before this runs -- [`Self::moveto_turn_to_object`] writes the
    /// desired heading onto `current_target_position`, which this line then overwrites -- so the
    /// second term is **0** on every turn the server sends, and the character faces the object
    /// exactly. Kept as retail does it rather than as it reads like it was meant; ACE's
    /// port agrees line for line.
    pub(crate) fn moveto_turn_to_object_internal(
        &mut self,
        target_position: Position,
        ctx: &mut MotionCtx<'_>,
    ) {
        self.moveto.current_target_position = target_position;
        let target_heading =
            position_heading(&ctx.env.position, &self.moveto.current_target_position);
        let sought_heading = get_heading(&self.moveto.sought_position.frame);
        let h = (target_heading + sought_heading) % 360.0;
        set_heading(&mut self.moveto.sought_position.frame, h);
        self.moveto.pending_actions.push_back(MovementNode {
            kind: MovementType::TurnToHeading,
            heading: h,
        });
        self.moveto.initialized = true;
        self.moveto_begin_next_node(ctx);
    }

    /// The clean-up-and-call-weenie step.
    ///
    /// **The name is the server's, and in the client it is a lie worth stating.** The whole body
    /// is `CleanUp(this); StopCompletely(physics_obj, 0);` -- the `ulong` error argument is *never
    /// read* and no weenie callback exists on this side of the codebase. A use that waited for the
    /// walk does not fire here; it fires on the server, whose `MoveToChain` polls the mover until
    /// the move-to is done. Nothing in the client queues an action against arrival.
    pub(crate) fn moveto_clean_up_and_call_weenie(&mut self, ctx: &mut MotionCtx<'_>) {
        self.moveto_clean_up(ctx);
        self.moveto_stop_completely(ctx);
    }

    pub fn moveto_turn_to_heading(&mut self, params: &MovementParameters, ctx: &mut MotionCtx<'_>) {
        self.moveto.movement_params = *params;
        self.moveto.movement_params.flags &= !flags::STICKY;
        self.moveto.sought_position = ctx.env.position;
        self.moveto.movement_type = MovementType::TurnToHeading;
        self.moveto.pending_actions.push_back(MovementNode {
            kind: MovementType::TurnToHeading,
            heading: params.desired_heading,
        });
        self.moveto_begin_next_node(ctx);
    }

    pub(crate) fn moveto_begin_next_node(&mut self, ctx: &mut MotionCtx<'_>) {
        match self.moveto.pending_actions.front().copied() {
            None => {
                if self.moveto.movement_params.has(flags::STICKY) {
                    let (id, radius, height) = (
                        self.moveto.top_level_object_id,
                        self.moveto.sought_object_radius,
                        self.moveto.sought_object_height,
                    );
                    self.moveto_clean_up(ctx);
                    self.moveto_stop_completely(ctx);
                    self.stick_to_object(id, radius, height, ctx.env.cur_time.0, ctx);
                } else {
                    self.moveto_clean_up(ctx);
                    self.moveto_stop_completely(ctx);
                }
            }
            Some(node) => match node.kind {
                MovementType::MoveToPosition => self.moveto_begin_move_forward(ctx),
                MovementType::TurnToHeading => self.moveto_begin_turn_to_heading(ctx),
                _ => {}
            },
        }
    }

    /// Begin moving forward.
    pub(crate) fn moveto_begin_move_forward(&mut self, ctx: &mut MotionCtx<'_>) {
        let d = self.moveto.current_distance(ctx);
        let (cmd, hold, away) = self.moveto.movement_params.get_command(d);
        if cmd == MotionCommand::NONE {
            self.moveto.pending_actions.pop_front();
            self.moveto_begin_next_node(ctx);
            return;
        }
        let mut p = self.moveto.default_params();
        p.hold_key_to_apply = hold;
        let err = self.moveto_do_motion(cmd, &p, ctx);
        if err != 0 {
            self.moveto_cancel_move_to(err, ctx);
            return;
        }
        self.moveto.current_command = cmd;
        self.moveto.moving_away = away;
        self.moveto.movement_params.hold_key_to_apply = hold;
        self.moveto.previous_distance = d;
        self.moveto.original_distance = d;
        self.moveto.previous_distance_time = ctx.env.cur_time.0;
        self.moveto.original_distance_time = ctx.env.cur_time.0;
    }

    /// Begin turning to a heading.
    pub(crate) fn moveto_begin_turn_to_heading(&mut self, ctx: &mut MotionCtx<'_>) {
        let Some(node) = self.moveto.pending_actions.front().copied() else {
            self.moveto_cancel_move_to(super::interp::NO_PHYSICS_OBJECT, ctx);
            return;
        };
        // `always_turn` is a host option the client never sets (see the field); with it clear
        // this is the client's wait for the current animation.
        if self.interp.motions_pending() && !self.moveto.always_turn {
            return; // wait for the current animation
        }
        let h = get_heading(&ctx.env.position.frame);
        let diff = heading_diff(node.heading, h, MotionCommand::TURN_RIGHT);
        let cmd = if diff > 180.0 {
            if diff + EPSILON >= 360.0 {
                // Already there.
                self.moveto.pending_actions.pop_front();
                self.moveto_begin_next_node(ctx);
                return;
            }
            MotionCommand::TURN_LEFT
        } else if diff > EPSILON {
            MotionCommand::TURN_RIGHT
        } else {
            self.moveto.pending_actions.pop_front();
            self.moveto_begin_next_node(ctx);
            return;
        };
        let p = self.moveto.default_params();
        let err = self.moveto_do_motion(cmd, &p, ctx);
        if err != 0 {
            self.moveto_cancel_move_to(err, ctx);
            return;
        }
        self.moveto.current_command = cmd;
        self.moveto.previous_heading = diff;
    }

    pub fn moveto_use_time(&mut self, ctx: &mut MotionCtx<'_>) {
        if !ctx.env.contact {
            return;
        }
        let Some(node) = self.moveto.pending_actions.front().copied() else {
            return;
        };
        if self.moveto.top_level_object_id != ObjectId(0)
            && self.moveto.movement_type != MovementType::Invalid
            && !self.moveto.initialized
        {
            return;
        }
        match node.kind {
            MovementType::MoveToPosition => self.moveto_handle_move_to_position(ctx),
            MovementType::TurnToHeading => self.moveto_handle_turn_to_heading(ctx),
            _ => {}
        }
    }

    /// Handle the move-to-position step.
    pub(crate) fn moveto_handle_move_to_position(&mut self, ctx: &mut MotionCtx<'_>) {
        let p = self.moveto.default_params();

        // (a) the corrective turn, with the 20°/340° dead-band.
        if self.interp.motions_pending() {
            if self.moveto.aux_command != MotionCommand::NONE {
                self.moveto_stop_motion(self.moveto.aux_command, &p, ctx);
                self.moveto.aux_command = MotionCommand::NONE;
            }
        } else {
            let mut want =
                position_heading(&ctx.env.position, &self.moveto.current_target_position)
                    + MovementParameters::get_desired_heading(
                        self.moveto.current_command,
                        self.moveto.moving_away,
                    );
            if want >= 360.0 {
                want -= 360.0;
            }
            let diff = heading_diff(
                want,
                get_heading(&ctx.env.position.frame),
                MotionCommand::TURN_RIGHT,
            );
            if diff <= 20.0 || diff >= 340.0 {
                if self.moveto.aux_command != MotionCommand::NONE {
                    self.moveto_stop_motion(self.moveto.aux_command, &p, ctx);
                    self.moveto.aux_command = MotionCommand::NONE;
                }
            } else {
                let cmd = if diff >= 180.0 {
                    MotionCommand::TURN_LEFT
                } else {
                    MotionCommand::TURN_RIGHT
                };
                if cmd != self.moveto.aux_command {
                    self.moveto_do_motion(cmd, &p, ctx);
                    self.moveto.aux_command = cmd;
                }
            }
        }

        // (b) progress and arrival.
        let d = self.moveto.current_distance(ctx);
        if self.moveto.check_progress_made(d, ctx.env.cur_time.0) {
            self.moveto.fail_progress_count = 0;
            let arrived = if self.moveto.moving_away {
                d >= self.moveto.movement_params.min_distance
            } else {
                d <= self.moveto.movement_params.distance_to_object
            };
            if arrived {
                self.moveto.pending_actions.pop_front();
                self.moveto_stop_motion(self.moveto.current_command, &p, ctx);
                self.moveto.current_command = MotionCommand::NONE;
                if self.moveto.aux_command != MotionCommand::NONE {
                    self.moveto_stop_motion(self.moveto.aux_command, &p, ctx);
                    self.moveto.aux_command = MotionCommand::NONE;
                }
                self.moveto_begin_next_node(ctx);
            } else if distance(&self.moveto.starting_position, &ctx.env.position)
                > self.moveto.movement_params.fail_distance
            {
                self.moveto_cancel_move_to(YOU_CHARGED_TOO_FAR, ctx);
            }
        } else if !ctx.env.is_interpolating && !self.interp.motions_pending() {
            self.moveto.fail_progress_count += 1;
        }

        // (c) keep the target-update rate proportional to the ETA.
        //
        // **Two things here are easy to get wrong; both are checked against retail.**
        //
        // ```text
        //   v = object velocity
        //   if (|projected speed - previous| <= 1.0) leave the quantum alone
        //   else set the target quantum
        // ```
        //
        // 1. **The velocity is `get_velocity()`, which is `cached_velocity`** and not the
        //    integrated velocity -- see `MotionEnv::cached_velocity`. The integrated velocity is
        //    identically zero for a creature walking on the ground, so reading it here makes
        //    `v > 0.1` never true and the whole block dead: over a recorded approach the quantum
        //    would be written **0** times during the walk.
        // 2. **The write is guarded by `|eta - get_target_quantum()| > 1.0`.** It is a hysteresis
        //    band, not a per-frame recompute: retail moves the extrapolation lead only when the
        //    estimated time of arrival has changed by more than a second.
        if self.moveto.top_level_object_id != ObjectId(0)
            && self.moveto.movement_type != MovementType::Invalid
        {
            let v = ctx.env.cached_velocity.mag2().sqrt();
            if v > 0.1 {
                let eta = d / v;
                if (f64::from(eta) - f64::from(ctx.env.target_quantum)).abs() > 1.0 {
                    ctx.effects.push(MotionEffect::SetTargetQuantum(eta));
                }
            }
        }
    }

    /// Handle the turn-to-heading step.
    pub(crate) fn moveto_handle_turn_to_heading(&mut self, ctx: &mut MotionCtx<'_>) {
        if self.moveto.current_command != MotionCommand::TURN_LEFT
            && self.moveto.current_command != MotionCommand::TURN_RIGHT
        {
            self.moveto_begin_turn_to_heading(ctx);
            return;
        }
        let Some(node) = self.moveto.pending_actions.front().copied() else {
            return;
        };
        let h = get_heading(&ctx.env.position.frame);
        if heading_greater(h, node.heading, self.moveto.current_command) {
            self.moveto.fail_progress_count = 0;
            ctx.effects.push(MotionEffect::SetHeading(node.heading));
            self.moveto.pending_actions.pop_front();
            let p = self.moveto.default_params();
            self.moveto_stop_motion(self.moveto.current_command, &p, ctx);
            self.moveto.current_command = MotionCommand::NONE;
            self.moveto_begin_next_node(ctx);
            return;
        }
        let delta = heading_diff(h, self.moveto.previous_heading, self.moveto.current_command);
        if delta < 180.0 && delta > EPSILON {
            self.moveto.fail_progress_count = 0;
            self.moveto.previous_heading = h;
            return;
        }
        self.moveto.previous_heading = h;
        if !ctx.env.is_interpolating && !self.interp.motions_pending() {
            self.moveto.fail_progress_count += 1;
        }
    }

    pub fn moveto_handle_update_target(&mut self, info: &TargetInfo, ctx: &mut MotionCtx<'_>) {
        if info.object_id != self.moveto.top_level_object_id {
            return;
        }
        if self.moveto.initialized {
            if !info.ok {
                self.moveto_cancel_move_to(OBJECT_GONE, ctx);
                return;
            }
            if self.moveto.movement_type == MovementType::MoveToObject {
                self.moveto.sought_position = info.interpolated_position;
                self.moveto.current_target_position = info.target_position;
                self.moveto.previous_distance = f32::MAX;
                self.moveto.original_distance = f32::MAX;
                self.moveto.previous_distance_time = ctx.env.cur_time.0;
                self.moveto.original_distance_time = ctx.env.cur_time.0;
            }
            return;
        }
        // The self-target arm comes **before** the status check, and that is the client's own
        // order. Unreachable in practice: `MoveToObject`/`TurnToObject` never set a target when
        // the top-level id is your own, so no `TargetInfo` about yourself ever arrives.
        if self.moveto.top_level_object_id == ctx.env.object_id {
            self.moveto.sought_position = ctx.env.position;
            self.moveto.current_target_position = ctx.env.position;
            self.moveto_clean_up_and_call_weenie(ctx);
            return;
        }
        if !info.ok {
            self.moveto_cancel_move_to(NO_OBJECT, ctx);
            return;
        }
        match self.moveto.movement_type {
            MovementType::MoveToObject => self.moveto_move_to_object_internal(
                info.target_position,
                info.interpolated_position,
                ctx,
            ),
            MovementType::TurnToObject => {
                self.moveto_turn_to_object_internal(info.target_position, ctx);
            }
            // `HandleUpdateTarget` has no other arm: it sets `initialized` only inside the two
            // `_Internal` calls, so an unexpected movement type leaves the manager waiting.
            _ => {}
        }
    }

    pub fn moveto_hit_ground(&mut self, ctx: &mut MotionCtx<'_>) {
        if self.moveto.is_moving_to() {
            self.moveto_begin_next_node(ctx);
        }
    }

    /// The move-to manager's clean-up.
    pub(crate) fn moveto_clean_up(&mut self, ctx: &mut MotionCtx<'_>) {
        let p = self.moveto.default_params();
        if self.moveto.current_command != MotionCommand::NONE {
            self.moveto_stop_motion(self.moveto.current_command, &p, ctx);
        }
        if self.moveto.aux_command != MotionCommand::NONE {
            self.moveto_stop_motion(self.moveto.aux_command, &p, ctx);
        }
        // The early-out guard shows that the whole stop/clear block needs a physics
        // object -- then **two** conditions for the clear, not one:
        //
        // ```text
        //   if (top_level_object_id != 0 && movement_type != 0)
        //       clear the target
        // ```
        if self.moveto.top_level_object_id != ObjectId(0)
            && self.moveto.movement_type != MovementType::Invalid
        {
            ctx.effects.push(MotionEffect::ClearTarget);
        }
        self.moveto.initialize_local_variables(ctx.env.cur_time.0);
    }

    pub fn moveto_cancel_move_to(&mut self, err: u32, ctx: &mut MotionCtx<'_>) {
        if !self.moveto.is_moving_to() {
            return;
        }
        ctx.effects.push(MotionEffect::MoveToFailed(err));
        self.moveto.pending_actions.clear();
        self.moveto_clean_up(ctx);
        self.moveto_stop_completely(ctx);
    }

    /// Stopping the object completely reaches the motion interpreter's stop.
    /// This is a synchronous call, not a velocity reset. CancelMoveTo cleans before recursively
    /// stopping; the nested cancel therefore sees Invalid and terminates, as in retail.
    ///
    /// **The drain is part of the call.** The move-to manager's stop does not call the interpreter
    /// directly: it builds a `MovementStruct { StopCompletely }` and goes through the movement
    /// dispatch, whose every arm ends in a check for completed motions:
    ///
    /// ```text
    ///   stop motion completely
    ///   check for completed motions
    /// ```
    ///
    /// and the interpreter's own tail is exactly that call. So when the move-to manager's
    /// perform-movement step
    /// cancels the previous move-to before starting the next, the READY nodes the cancel queued
    /// (`CleanUp`'s `_StopMotion` and `StopCompletely`'s own `add_to_queue`) are **already popped**
    /// by the time the next-node path asks `motions_pending()`,
    /// and the new turn's `DoMotion` is issued inside the same call. Without this drain the
    /// question is answered "yes", the new node waits, `UseTime` starts the turn one sub-step
    /// later, and the next frame's re-aim cancels it again before the body has moved: a held
    /// first-person turn key (one turn-to-heading step of `heading + 8` per frame, from the
    /// camera update) freezes the body after its first step -- 5.7 degrees over 120 frames.
    pub(crate) fn moveto_stop_completely(&mut self, ctx: &mut MotionCtx<'_>) {
        self.moveto_cancel_move_to(0x36, ctx);
        self.stop_completely_after_cancel(ctx);
        self.drain_completed_motions(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::NoAssets;
    use crate::motion::{ActionNode, MotionEnv, MovementManager};
    use crate::seq::Sequence;
    use crate::table::MotionTableManager;

    /// Constructed interpreter/table ledgers, through the same owner as Character.
    /// Unsticking synchronously cancels move-to, stops movement completely, then starts link-animation
    /// removal, which reenters BEFORE either outer completion pops its head.
    #[test]
    fn internal_removal_reentry_drains_both_live_ledgers_before_outer_head_reload() {
        let mut movement = MovementManager::new();
        let mut table = MotionTableManager::new(None);
        let mut seq = Sequence::new();
        for (motion, count) in [(MotionCommand::WAVE, 1), (MotionCommand::TURN_RIGHT, 0)] {
            table.add_to_queue(motion, count, &mut seq);
            movement.interp.pending_motions.push_back(MotionNode {
                context_id: 0,
                motion,
                jump_error_code: 0,
            });
        }
        let action = ActionNode {
            action: MotionCommand::WAVE,
            speed: 1.0,
            stamp: 1,
            autonomous: false,
        };
        movement.interp.interpreted_state.actions.push_back(action);
        movement.interp.raw_state.actions.push_back(action);
        table.state.add_action(MotionCommand::WAVE, 1.0);
        movement.moveto.movement_type = MovementType::TurnToObject;
        movement.moveto.top_level_object_id = ObjectId(0x7000_1031);
        movement.sticky.target_id = ObjectId(0x7000_1032);
        let env = MotionEnv {
            in_cell: false,
            ..MotionEnv::default()
        };
        let (mut effects, mut events) = (Vec::new(), Vec::new());
        let mut ctx = MotionCtx {
            mgr: &mut table,
            seq: &mut seq,
            assets: &NoAssets,
            env: &env,
            effects: &mut effects,
            events: &mut events,
        };
        movement.drain_animation_done(false, &mut ctx);
        assert!(!movement.is_moving_to());
        assert!(!movement.sticky.is_sticky());
        assert!(
            movement.interp.pending_motions.is_empty(),
            "outer reload must pop the Ready inserted by nested StopCompletely"
        );
        assert!(table.pending().is_empty());
        assert!(movement.interp.raw_state.actions.is_empty());
        assert!(movement.interp.interpreted_state.actions.is_empty());
        let done: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                AnimEvent::MotionDone { motion, success } => Some((*motion, *success)),
                _ => None,
            })
            .collect();
        assert_eq!(
            done,
            vec![
                (MotionCommand::WAVE, false),
                (MotionCommand::WAVE, false),
                (MotionCommand::TURN_RIGHT, false),
            ],
            "nested callback observes the still-current original before the outer pop"
        );
        assert_eq!(
            effects,
            vec![
                MotionEffect::ClearTarget,
                MotionEffect::MoveToFailed(ACTION_CANCELLED),
                MotionEffect::ClearTarget
            ]
        );
    }
}
