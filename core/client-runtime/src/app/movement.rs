//! Position reports, control transfer, and jumping.

use super::*;

impl<S: Shell> App<S> {
    pub(super) const fn position_use_times(&self) -> u64 {
        self.events.total(FrameEventKind::PositionUseTime)
    }

    /// Tell the server where the player is.
    ///
    /// The client first asks `should_send_position_event`; a non-zero answer calls
    /// `send_position_event` and emits `0xF753`.
    /// The `0xF61C` half is `send_movement_event`, which retail calls from its six
    /// command handlers; [`dereth_client_net::client_session::PositionReporter`] carries the policy and this function
    /// is only the state hand-off.
    ///
    /// **Why it matters.** ACE writes `Player.Location` from
    /// `GameActionMoveToState`, `GameActionAutonomousPosition` and `GameActionJump` and from
    /// nothing else, so a client that sends none of the three leaves the server's copy of the
    /// player at the login position for the whole session — every range check, every approach and
    /// every reachability decision made against a stale point.
    ///
    /// **Where each field comes from, and the one gap.**
    ///
    /// * `position`, `contact`, `contact_plane` — the body's own physics object: `position`,
    ///   `transient_state & (CONTACT_TS | ON_WALKABLE_TS)` (which is `Character::on_ground`, and
    ///   is exactly the pair position reporting gates on), and `contact_plane`.
    /// * `position_valid` — an inbound-valid cell id (already transcribed in
    ///   `dereth_physics::landdefs`, not copied here) and seven finite frame components.
    /// * `raw_motion_state`, `longjump_mode` — the body's raw motion state and
    ///   standing-long-jump flag from the motion interpolation that its
    ///   motion driver owns.
    /// * `timestamps` — `update_times[8]`, `[5]`, `[4]`, `[6]`. `Presence` carries all four, so
    ///   the teleport and force-position timestamps are the values the client holds rather than a
    ///   literal 0. Measured over the 1,827 recorded position bodies, `teleport` is non-zero in
    ///   **1,481** of them (running 0..=4) and `force_position` in **none**, so a literal 0 would
    ///   be wrong four times in five for the first — and the second is right *because nothing
    ///   advances it*, which is a different fact from being hard-wired.
    pub(super) fn position_use_time(&mut self) {
        // Before the early return: this counts the *call*, not the send.
        self.events.push(FrameEvent::PositionUseTime);
        let now = self.timer.cur_time;
        // `enabled != 0 && player != NULL`. There is
        // no point reporting a position before the server has told us which object we are, and
        // none at all with no link to report it on.
        let player = self.objects.player();
        let has_link = self.link.is_some();
        let Some(motion) = self.player_motion() else {
            self.position.active = false;
            return;
        };
        self.position.active = has_link && player.is_some() && self.movement.is_enabled();
        let before = self.position.stats;
        if let Some(link) = self.link.as_mut() {
            self.position.use_time(now, &motion, &mut link.net.session);
        }
        let after = self.position.stats;
        if after.encode_failures != before.encode_failures {
            tracing::warn!("a position event would not encode");
        }
        // One line the first time each half goes out, so a driven run can say the producer fired
        // rather than leaving it to be inferred. Replay anchors prove encoding but cannot prove
        // that the producer emitted anything.
        if before.position_events == 0 && after.position_events == 1 {
            tracing::debug!(
                "0xF753 Movement_AutonomousPosition, cell {:#010X} \
                 (every {} s, plus a cell or contact-plane change)",
                motion.position.objcell_id,
                dereth_client_net::client_session::TIME_BETWEEN_POSITION_EVENTS
            );
        }
        if before.movement_events == 0 && after.movement_events == 1 {
            tracing::debug!(
                "0xF61C Movement_MoveToState, cell {:#010X}",
                motion.position.objcell_id
            );
        }
    }

    /// The two calls that give the control transfer its callers.
    ///
    /// Losing control, the per-frame retake and taking control from the server live on the
    /// command lists; this is where they are called. A player movement dispatch that is
    /// non-autonomous loses control. Each later use-time tick retakes it only when the interpreter
    /// is enabled, control is still held by the server, the player has no pending motion or move-to
    /// operation, and at least one command list or auto-run requests movement.
    ///
    /// The movement decoder guards its unpack with `(autonomous == 0 || not-the-player)` and sets
    /// its dispatch flag only for the player, so a true result means **non-autonomous and the
    /// player**, which is exactly the arm
    /// `world.rs::apply_player_movement` runs. That is why the stop edge is latched there and
    /// drained here rather than being decided in this file.
    ///
    /// **Where it sits in the frame, and the one deviation.** It is immediately after
    /// [`Self::sync_objects`], because that is where this build unpacks the player's movement
    /// buffer, preserving the retail order within one frame: dispatch, then control transfer. It
    /// is **not** beside
    /// [`Self::position_use_time`], which is the same retail function's *other* half at frame step
    /// 8.9: putting it there would consume the latch a frame after the dispatch that set it, and
    /// this build already runs `apply_player_movement` after step 8.9. The visible cost is that a
    /// `set_auto_run(0)` notice queued here is drained by the **next** frame's
    /// [`Self::apply_input_actions`], one frame (33 ms) later than the stop it describes.
    ///
    /// The three counters exist for the reason [`Self::player_teleport_use_time`] gives: a step
    /// nothing drives is invisible to every anchor in this project, and a mutation that deletes a
    /// call from `App::frame` survives everything until a counter can see it. They are separate —
    /// *ran*, *lost*, *retook* — so that "no dispatch arrived" and "the step never ran" cannot be
    /// confused: an instrument with no third state cannot tell them apart.
    ///
    /// The work itself is the free [`command_interpreter_control_transfer`], for the reason
    /// [`apply_player_teleport`] is free: a counter can prove the *call site* exists and can prove
    /// nothing at all about what the step does, so the step has to be reachable by a test holding
    /// a real body and no device.
    pub(super) fn command_interpreter_control_transfer(&mut self, shell: &mut S) {
        // Counted before the call: this is the **call**, not the transfer.
        self.events.push(FrameEvent::ControlTransfer);
        let (lost, retook) = command_interpreter_control_transfer_with_finish(
            self.present.scene_mut(self.world.as_mut()).as_deref_mut(),
            &mut self.movement,
            &mut self.char_input,
            |character| crate::jump::finish(&mut self.objects.world.combat, character),
        );
        if lost {
            self.events
                .push(FrameEvent::ServerControlLost(ControlLossSite::Transfer));
        }
        if retook {
            self.events.push(FrameEvent::ServerControlRetaken);
        }
        if lost {
            self.deliver_jump_power_bar_notices(shell);
        }
    }

    /// Press starts `CommenceJump`; release calls `DoJump(true)`.
    /// Commands preceding this event are completed without advancing the world clock.
    pub(super) fn apply_jump_action(
        &mut self,
        shell: &mut S,
        command: crate::actions::movement::MovementAction,
    ) -> bool {
        use crate::actions::movement::MovementAction as M;
        if !matches!(command, M::CommenceJump | M::DoJump) {
            return false;
        }
        // The command-interpreter action handler checks that it is active before the jump arms
        // too. It consumes the action while disabled, but neither starts nor finishes a jump.
        if !self.movement.is_enabled() {
            return true;
        }
        let retake = self.movement.take_control_retake_pending();
        let Some(character) = self.world.as_mut().and_then(|w| w.character.as_mut()) else {
            return true;
        };
        if retake {
            character.take_control_from_server();
        }
        crate::jump::refresh_qualities(
            character,
            self.hud.player_desc(&self.objects.world),
            self.hud.skill_table.as_ref(),
            self.hud.quality_filter.as_ref(),
            &self.objects.world.world_rules,
        );
        character.flush_command_input(self.char_input);
        let now = dereth_primitives::LocalTime(self.timer.cur_time);
        match command {
            M::CommenceJump => {
                let started =
                    crate::jump::commence(character, &mut self.objects.world, now, || {
                        if let Some(link) = self.link.as_mut() {
                            let _ = link
                                .net
                                .session
                                .send_action(&dereth_protocol::combat::CombatCancelAttack);
                        }
                    });
                if started {
                    // The commence-jump step's final call: the charge flag is already set, no
                    // physics tick has occurred, and this is independent of motion-change polling.
                    if let Some(motion) = self.player_motion() {
                        if let Some(link) = self.link.as_mut() {
                            self.position.send_movement_event(
                                now.0,
                                &motion,
                                &mut link.net.session,
                            );
                        }
                    }
                }
            }
            M::DoJump => {
                if let Some(result) = crate::jump::release(character, &mut self.objects.world, now)
                {
                    // One event for both halves of `jump_counts`: the refusals are the
                    // releases whose `status` is non-zero, which is the test the `else` was.
                    self.events.push(FrameEvent::JumpRequested {
                        status: result.status,
                    });
                    if result.status == 0 {
                        if let Some(motion) = self.player_motion() {
                            let pack =
                                dereth_client_net::client_session::PositionReporter::jump_pack(
                                    result.extent,
                                    result.local_velocity,
                                    &motion,
                                );
                            self.last_jump_request =
                                Some(dereth_protocol::movement::MovementJump(pack));
                            if let Some(link) = self.link.as_mut() {
                                self.position.send_jump_pack(pack, &mut link.net.session);
                            }
                        }
                    }
                }
            }
            _ => unreachable!(),
        }
        self.deliver_jump_power_bar_notices(shell);
        true
    }

    /// The jump's notice sends are synchronous with their input/control-loss owner. Finish the
    /// existing subscriber before the next action, not next frame. No UI tick or mode switch is
    /// performed here, and an absent/outgoing subscriber leaves no retained notice history.
    pub(super) fn deliver_jump_power_bar_notices(&mut self, shell: &mut S) {
        let notices = self.objects.world.combat.take_power_bar_notices();
        shell.deliver_power_bar_notices(&mut self.hud, notices);
    }

    /// Everything [`dereth_client_net::client_session::PositionReporter`] reads off the body, in one place.
    ///
    /// `None` when there is no body yet, which is every frame before `load_pending_scene` has
    /// stood one up.
    pub(super) fn player_motion(&self) -> Option<dereth_client_net::client_session::PlayerMotion> {
        let character = self.world.as_ref()?.character.as_ref()?;
        let presence = self
            .objects
            .player()
            .and_then(|id| self.objects.presence(id));
        // All four sequences are the client's own, and the mapping is a public
        // free function so that the test which owns the echo can call the same code this does.
        Some(body_motion_in(
            character,
            player_timestamps(presence),
            self.objects.command_numbering(),
        ))
    }
}
