//! Movement, camera, and action application.

use super::*;

impl<S: Shell> App<S> {
    /// What a window's focus loss does to the controls the runtime itself latches.
    ///
    /// The camera's held look controls are cleared, the keyboard's movement commands are dropped
    /// the way the client's keyboard-focus loss drops them, and the mouse-look hold is released.
    /// The per-control release -- every held control firing its end -- is the device input's, and
    /// a front end does it when it sees the same focus loss.
    ///
    /// **It is `pub`** because a focus loss is an event a test can construct, and the arm below is
    /// a step whose absence is only visible from outside the event loop.
    pub fn note_flycam_input(&mut self, event: &HostEvent) {
        if let HostEvent::Focused(false) = event {
            self.input = CameraInput::default();
            // The lists have to go with it. Retail's pressed-key release fires
            // a synthetic release for every held control on focus loss; clearing the slots
            // without clearing the lists would let the next command re-assert a key nobody is
            // holding, because `apply_current_movement` reads the list and not the bool.
            //
            // The per-control half of the pressed-key release is **not** here and must not be:
            // `Pump::map_window_event` turns this same `Focused(false)` into `WM_KILLFOCUS`,
            // and the device input's handler for it calls `release_pressed_keys`, which walks
            // the active-controls table and fires a real zero-extent release per held control
            // down the ordinary `fire_action_event` path. That is the focus-loss release loop,
            // and it is what makes an alt-tab with Jump held run the jump-release handler
            // rather than stranding the charge. The focus-release tests measure it end to end.
            //
            // This line calls `lose_keyboard_focus`, which the input manager invokes through the
            // command interpreter after its active-control hash is empty.
            //
            // It is not `self.char_input = CharacterInput::default()` followed by
            // `MovementCommands::clear_all_commands`: neither statement matches the client's
            // focus-loss behavior, which clears only the *keyboard* commands, never calls
            // `set_auto_run`, and refuses to re-apply movement while the server owns the body.
            // There is no wholesale wipe — `lose_keyboard_focus` reaches
            // the six motion slots the way every other caller does, through
            // `apply_current_movement`'s projection, and `char_input.jump` is cleared every
            // frame by `App::frame` regardless. See
            // [`crate::character::MovementCommands::lose_keyboard_focus`] for the bytes.
            //
            // The returned `bool` controls the movement event, which is not sent —
            // named as a gap on that function rather than half-wired here.
            let _movement_changed = self.movement.lose_keyboard_focus(&mut self.char_input);
            // **The third thing the pressed-key release fires.**
            // Its last act, when mouse-look mode is on and a device exists, is a
            // synthetic button input event (activation `0x80`) on the
            // virtual device's control 1 — the mouse-look hold, released so the mode cannot
            // survive the focus loss. The device input's `WM_KILLFOCUS` arm already does the
            // input-manager half (`leave_mouse_look`); this is the half that lives here,
            // because [`Self::mouse_look`] is a second, app-level latch that only
            // `WindowEvent::MouseInput` otherwise writes. Without this line an alt-tab with the
            // right button held comes back with the latch still set, and the next cursor
            // motion swings the camera with no button down until the user presses and
            // releases it again. Synthesised through the same seam a real button-up uses, so
            // there is one code path and not two.
            self.mouse_look_button(false);
        }
    }

    /// The app-level mouse-button projection: the right button is the mouse-look hold, and
    /// the mouse-look flag is what
    /// [`Self::cursor_moved`] reads.
    ///
    /// **`pub`** because a front end calls it for its mouse-look button.
    /// The window system's own mouse-button event carries a device id that cannot be constructed
    /// without `unsafe` — `crate::pump`'s own module documentation says so and splits its half out
    /// for the same reason — so code inline in that arm would be code no test in this workspace
    /// could reach. The pumped events are plain data, so that arm is reachable; this split stays
    /// because the two halves are the two things the arm does.
    pub fn mouse_look_button(&mut self, down: bool) {
        // The hold ending is mouse look ending: a turn the mouse gave the body stops.
        if self.mouse_look && !down {
            if let Some(mut world) = self.present.scene_mut(self.world.as_mut()) {
                if let Some(c) = world.world_mut().character.as_mut() {
                    c.camera.mouse_look_ended();
                }
            }
        }
        self.mouse_look = down;
        self.last_mouse_move = self.timer.cur_time;
        // A fresh hold starts from the cursor's next sample, never from where it was left.
        self.last_cursor = None;
    }

    /// The `CursorMoved` arm, split out with [`Self::mouse_look_button`] and for the same reason.
    pub fn cursor_moved(&mut self, x: f64, y: f64) {
        let now = (x, y);
        if self.mouse_look {
            if let Some(prev) = self.last_cursor {
                #[allow(clippy::cast_possible_truncation)]
                // LINT-OK: a cursor delta in pixels for a debug flycam, narrowed for its
                // own f32 arithmetic. Not engine arithmetic.
                let (dx, dy) = ((now.0 - prev.0) as f32, (now.1 - prev.1) as f32);
                if dx != 0.0 || dy != 0.0 {
                    self.last_mouse_move = self.timer.cur_time;
                }
                if let Some(mut world) = self.present.scene_mut(self.world.as_mut()) {
                    // With a body this is camera-set mouse look through
                    // `crate::actions::camera`; without one it is the flycam look.
                    crate::camera::mouse_look(
                        &mut *world,
                        dx,
                        dy,
                        dereth_primitives::LocalTime(self.timer.cur_time),
                    );
                }
            }
        }
        self.last_cursor = Some(now);
    }

    /// The residual flycam's "up", held or released: the one control with no action behind it.
    ///
    /// The shipped keymap has no action that raises or lowers a viewpoint, because retail has no
    /// viewpoint to raise -- the camera is a swept sphere pivoting on a body. So the front end
    /// calls this directly for its flycam key, gated on the one condition only it can read: that
    /// nothing has claimed the keyboard. This side gates on the other: there is no body, which is
    /// the only configuration in which the free camera runs at all.
    ///
    /// **What it deliberately does not do is as important as what it does**: it does not touch
    /// [`Self::char_input`], so it can never walk a body.
    pub fn flycam_rise(&mut self, held: bool) {
        if self.has_body() {
            return;
        }
        self.input.up = held;
    }

    /// The residual flycam's "down", held or released. See [`Self::flycam_rise`].
    pub fn flycam_sink(&mut self, held: bool) {
        if self.has_body() {
            return;
        }
        self.input.down = held;
    }

    /// Whether the scene has a body. `WorldScene::character.is_some()`.
    fn has_body(&self) -> bool {
        self.world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .is_some()
    }

    /// The do-motion/stop-motion tail for the commands that are on no command list: the four
    /// stances and the 87 emotes. The emote hash is
    /// [`crate::actions::emote::INPUT_ACTION_COMMANDS`], and
    /// [`crate::character::MovementCommands::take_transient_motions`] is drained here into the
    /// body's motion driver. That is what the movement-command tail does with a command on no list.
    ///
    /// The queue is taken **unconditionally** so that the counter behind it moves even when no
    /// body exists to play the motion: *the key produced nothing* and *the key produced a motion
    /// nobody could play* are different findings, and a `take` inside the `if let` could not tell
    /// them apart.
    ///
    /// It is a method rather than a block at the foot of [`Self::apply_input_actions`], because
    /// that foot is below two early returns and a typed `*wave*` arrives on a frame with no input
    /// events at all.
    fn play_transient_motions(&mut self) {
        let transient = self.movement.take_transient_motions();
        if transient.is_empty() {
            return;
        }
        if let Some(c) = self.world.as_mut().and_then(|w| w.character.as_mut()) {
            for (cmd, start) in transient {
                c.command_motion(start, dereth_animation::MotionCommand(cmd));
            }
        }
    }

    /// The input seam for movement and for the camera's look controls.
    ///
    /// Input dispatch gives the winning map's callback first refusal and then walks the
    /// input-handler list; the movement interpreter and camera controller are two entries in
    /// that list. This is their dispatch point. Everything that reaches it has already been
    /// through step 6 — the device input's map walk, which is where the `MAP_BLOCK_KEYBOARD`
    /// barrier `break`s — so **the barrier is load-bearing for movement**.
    ///
    /// The two decoders are [`crate::actions::movement::on_action`] and
    /// [`crate::actions::camera::on_action`]; this is their caller.
    ///
    /// It runs immediately after UI input dispatch (step 7), where the UI has
    /// already taken what it consumed and handed the rest back, and **before** `WorldScene::update`
    /// reads `char_input`, so a key pressed this frame moves the body this frame.
    pub(super) fn apply_input_actions(&mut self, shell: &mut S) {
        self.events.push(FrameEvent::JumpUseTime);
        // **This must run before the two early returns below.**
        //
        // A typed pose plays through the command interpreter's motion entry point, the same one
        // the emote input-action hash's 91
        // keys reach through. So a typed `*wave*` and the `J` key put the
        // identical command into the identical list bookkeeping, and the `take_transient_motions`
        // drain at the foot of this function plays both.
        //
        // Placed above `let Some(shell) = ...` and above the `events.is_empty()` return because a
        // pose arrives from the **chat** dispatch, not from an input event: a frame where the
        // player typed and pressed nothing has no events at all, and that is the frame a pose
        // most often lands in -- and both of those returns are above the drain at the foot, so a
        // pose fed here and left for that drain would sit in the queue for ever. It is therefore
        // fed **and played** in one block, which is also the order retail has: the pose issues the
        // command and its motion tail runs inside that same call.
        let poses = self.interaction.take_pose_motions();
        if !poses.is_empty() {
            for cmd in poses {
                // Movement parameters are built by the movement command; the interpreter receives
                // the command and its start flag. `extent` is `None` because
                // keyboard-command handling reads it only for `AutoRun`, which no pose can be.
                let c = crate::actions::movement::CmdStruct {
                    command: cmd,
                    start: Some(true),
                    extent: None,
                };
                let m = crate::actions::movement::MovementAction::SetMotion(c);
                if self.movement.on_action(m, &mut self.char_input) {
                    self.events.push(FrameEvent::PoseMotionIssued);
                }
                // A pose reaches `add_command` through the identical bookkeeping, so it
                // can raise the identical edge. The 87 emotes are `0x13……` and carry neither
                // `0x40000000` nor a list, so in practice this drains nothing; the four stances
                // the emote input-action hash also carries are `0x41……` and do raise it, and
                // retail's stance change does abort an automatic attack. Drained per command so a
                // frame with no input events cannot leave one queued for the next one.
                self.abort_automatic_attack_on_new_forward_movement();
            }
            self.play_transient_motions();
        }
        let mut events = std::mem::take(&mut self.orbit_pending);
        let taken = self.actions.take();
        // Under the orbit camera the movement keys are turned into the game's movement as it reads
        // them; the mouse's asks are already the game's.
        match self.orbit {
            Some(settings) => {
                // While a spell is being cast the turning keys step sideways.
                let now = dereth_primitives::LocalTime(self.timer.cur_time);
                let casting = self.objects.world.magic.still_casting(now);
                events.extend(self.orbit_keys.set_casting(casting, settings));
                for e in taken {
                    events.extend(self.orbit_keys.convert(e, settings));
                }
            }
            None => events.extend(taken),
        }
        if events.is_empty() {
            return;
        }
        // **The second operand of `set_hold_run`'s XOR.**
        //
        // `set_hold_run` reads the toggle-run option on **every** call and XORs the physical key
        // against it. `ToggleRun` is default-on, so on a shipped character
        // `effective_run = !hold_run`: run is the default and holding the key walks you.
        //
        // [`crate::character::MovementCommands::ui_toggles_run`] carries that operand; left
        // unassigned it is `false` and the polarity is inverted in a running client. This is the
        // assignment. It sits here, once per frame ahead of the dispatch loop, because
        // every path that can reach `set_hold_run` — the `HoldRun` keyboard command and the
        // `SetHoldRun` action — goes through that loop, so a per-frame refresh and the client's
        // live option read cannot disagree within a frame.
        self.movement.ui_toggles_run = self.objects.world.player_system.options.toggle_run();
        let mut declined = Vec::with_capacity(events.len());
        for e in events {
            // The client UI system's complete first Escape leg returns before a following input
            // event. Leaving it in the declined end-of-batch queue would let an old Space-up
            // launch before cancellation. Non-jump Escape remains with its existing owner.
            if e.id == dereth_client_contract::actions::mapped::ESCAPE_KEY
                && self.interaction.try_finish_jump_from_escape(
                    &mut self.objects.world,
                    dereth_primitives::LocalTime(self.timer.cur_time),
                    self.world.as_ref().and_then(|w| w.character.as_ref()),
                )
            {
                self.events
                    .push(FrameEvent::ActionRouted(ActionRoute::EscapeFinishedJump));
                self.deliver_jump_power_bar_notices(shell);
                continue;
            }
            // The emote action-to-command table — see [`crate::actions::emote`] for why `|_| None`
            // here would close the four stance keys and all 87 emotes. The hash is the
            // interpreter's member and this is the interpreter's dispatch, so the table is passed
            // rather than duplicated.
            let m =
                crate::actions::movement::on_action(&e, crate::actions::emote::command_for_action);
            if self.apply_jump_action(shell, m) {
                self.events
                    .push(FrameEvent::ActionRouted(ActionRoute::Jump));
                continue;
            }
            if self.movement.on_action(m, &mut self.char_input) {
                self.events
                    .push(FrameEvent::ActionRouted(ActionRoute::Movement));
                // Accepting a new movement substate or listless motion invokes the movement
                // override, which performs two calls: `set_auto_run(0)`, already handled by
                // the movement handler, and the automatic-attack cancellation that needs the combat
                // system. This is that second call, and it is here -- inside the loop, on the
                // event that raised the edge -- because retail's is synchronous with the key: the
                // attack cancellation leaves before `move_player` has finished moving the body, in
                // the same `handle_keyboard_command`. Drained through `MovementCommands` for the
                // same one-borrow-at-a-time reason the retake and the notices below are.
                self.abort_automatic_attack_on_new_forward_movement();
                continue;
            }
            let cmd = crate::actions::camera::on_action(&e);
            if Self::apply_camera_action(&mut self.input, cmd) {
                self.events
                    .push(FrameEvent::ActionRouted(ActionRoute::Camera));
                continue;
            }
            // The other eight camera commands, which need the world.
            if self.apply_world_camera_action(shell, cmd) {
                self.events
                    .push(FrameEvent::ActionRouted(ActionRoute::WorldCamera));
                continue;
            }
            declined.push(e);
        }
        self.actions.put_back(declined);
        // `handle_keyboard_command` unconditionally requests `take_control_from_server` after
        // accepting a command. The interpreter half runs inside the dispatch loop above; this is
        // the body operation it cannot reach, and it is the
        // same call [`Self::command_interpreter_control_transfer`] makes when the per-frame step
        // retakes instead. Drained here, after the loop, for the reason the notices below are: one
        // borrow at a time, and a press cannot be answered twice.
        if self.movement.take_control_retake_pending() {
            if let Some(c) = self.world.as_mut().and_then(|w| w.character.as_mut()) {
                c.take_control_from_server();
            }
        }
        // The movement-command interpreter's do-motion/stop-motion tail for the
        // commands that are on no command list — the four stances and the 87 emotes. The queue is
        // taken unconditionally so that the counter behind it moves even when no body exists to
        // play the motion: *the key produced nothing* and *the key produced a motion nobody could
        // play* are different findings, and a `take` inside the `if let` could not tell them
        // apart.
        self.play_transient_motions();
        // Toggling auto-run selects `L"AutoRun ON"` or `L"AutoRun OFF"` and sends it as chat type
        // `0x1A`. That line is the only feedback a player gets that
        // `DIK_Q` did anything, and `MovementCommands::on_action` had nowhere to put it.
        //
        // This is the scroll-text surface and not a second road to the chat window: the channel is
        // the chat **type**, and `0x1A` is the one type the main window's default filter
        // `0xFBFFFFFF` drops (bit 26 clear). It draws in the spew panel's strip, which accepts
        // `0x1A` and nothing else. `Hud::drain_scroll` fans it out to both, each with its own test,
        // exactly as the final-string-info notice does.
        for text in self.movement.take_notices() {
            self.objects
                .world
                .scroll
                .on_display_string_info(dereth_client_model::scroll::LOCAL_ERROR_TYPE, text);
        }
    }

    /// The movement handler's combat half.
    ///
    /// The base call is the run lock, which [`crate::actions::movement::CommandLists::add_command`]
    /// performs; this function supplies the automatic-attack abort. It is **state-gated** — *"is an attack
    /// pending, in progress, being requested, or repeating?"* — so an ordinary walk with no attack
    /// running sends nothing, and this function faithfully asks the same question by calling the
    /// gate rather than reimplementing it. The world's automatic-attack abort also has
    /// the `Escape` and selection-change callers; without this caller on this edge, a player
    /// could not break a sticky repeated melee by walking backwards.
    ///
    /// The request goes out through [`crate::requests::send_request`], the one place a
    /// [`dereth_client_model::Request`] becomes bytes, so the `0x01B7` carries an `OrderedActionHeader` stamp from the
    /// same global counter as every other game action and leaves in this input frame — ahead of
    /// `interaction_use_time`'s own flush, exactly as retail's leaves inside
    /// `handle_keyboard_command`. With no link there is nothing to send to and the state change
    /// still happens, which is what the client does while disconnected.
    fn abort_automatic_attack_on_new_forward_movement(&mut self) {
        if !self.movement.take_new_forward_movement() {
            return;
        }
        self.events.push(FrameEvent::NewForwardAttackAborted);
        let mut req = dereth_client_model::RecordingRequests::default();
        self.objects.world.abort_automatic_attack(&mut req);
        for r in req.0 {
            self.events.push(FrameEvent::NewForwardAttackCancelSent);
            if let Some(link) = self.link.as_mut() {
                let _ = crate::requests::send_request(&mut link.net.session, &r);
            }
        }
    }

    /// Target tracking: the camera update routine, raised on the three edges retail raises it
    /// on.
    ///
    /// The gates themselves are [`crate::camera::update_target_tracking`]; this is the edge
    /// detection, and the triple is exactly `(tracking_target, combat_mode, selected_id)` — the
    /// three words retail's three call sites each change one of. See
    /// [`Self::last_target_tracking`].
    ///
    /// `ViewCombatTarget` is option ordinal 7, and `dereth_client_model::player`'s
    /// `OptionSideEffect::TrackTarget` is the producer `Interaction` counts as
    /// *"decided and not applied"* in `option_side_effects_unapplied`. That counter keeps counting:
    /// the option's bit is read here, from the model, rather than the side effect being consumed,
    /// because the bit is the state and the side effect is only the edge — and reading the state
    /// also gets the value right after a whole-module `0x01A1` load, which raises no side effect at
    /// all.
    pub(super) fn update_target_tracking(&mut self) {
        let tracking = self
            .objects
            .world
            .player_system
            .options
            .get(dereth_client_model::player::option::VIEW_COMBAT_TARGET);
        let now = (
            tracking,
            self.objects.world.combat.combat_mode,
            self.objects.world.selected,
        );
        if self.last_target_tracking == Some(now) {
            return;
        }
        let Some(mut world) = self
            .present
            .scene_mut(self.world.as_mut())
            .filter(|_| self.pending_scene.is_none())
        else {
            // No scene means no camera set, so retail returns
            // having touched nothing. **The latch is deliberately not advanced**, so the first
            // frame that does have a body applies the state the edge was about — otherwise an
            // option set or a mode entered while a scene was still loading would be lost for the
            // rest of the session.
            return;
        };
        crate::camera::update_target_tracking(&mut *world, &self.objects.world, tracking);
        self.last_target_tracking = Some(now);
    }

    /// Apply player-module initialization and the three **environment** arms of its option-change
    /// switch.
    ///
    /// The option-change switch, indexed by `option - 2`, has five live arms; two of
    /// them are the fellowship mutual exclusion that `dereth_client_model::player` already owns, one is
    /// `ViewCombatTarget` which [`Self::update_target_tracking`] owns, and the remaining **three**
    /// are these:
    ///
    /// `PersistentAtDay` applies the option directly to the landscape; `DisableMostWeatherEffects`
    /// passes its inverse to weather enablement; `DisableDistanceFog` stores its inverse in the
    /// landscape fog-enabled field. The order is significant and retained.
    ///
    /// Player-module initialization runs the identical three (plus `ViewCombatTarget`) in the same
    /// order when the `PlayerModule` first arrives, which is the login edge.
    ///
    /// # Why this is a latch and not a side-effect consumer
    ///
    /// `dereth_client_model::player::OptionSideEffect` already names all four calls, and
    /// `interaction.rs`'s `UiRequest::SetPlayerOption` arm **counts** them into
    /// `option_side_effects_unapplied` without applying any, because the arm holds no scene; so
    /// the apply is here, in the frame, as an edge detector over the three bits.
    ///
    /// Reading the **state** rather than consuming the **edge** is deliberate and is the same
    /// choice [`Self::update_target_tracking`] documents: a whole-module load (`0x0013` at login,
    /// or a `0x01A1` round trip) raises no option-change call at all, and retail covers that case
    /// with its initialization pass. One latch over the three bits reproduces both producers
    /// exactly, and nothing else in the process writes the three scene fields.
    ///
    /// `option_side_effects_unapplied` keeps counting, for the reason target tracking gives: the
    /// counter is
    /// about the *edge* the interaction arm declined, and this is the *state*.
    pub(super) fn apply_player_option_effects(&mut self) {
        let o = &self.objects.world.player_system.options;
        let now = (
            // `PersistentAtDay` -- the second option word's bit 0.
            o.get(dereth_client_model::player::option::PERSISTENT_AT_DAY),
            // `DisableMostWeatherEffects` -- the first option word's bit 16.
            o.get(dereth_client_model::player::option::DISABLE_MOST_WEATHER_EFFECTS),
            // `DisableDistanceFog` -- the second option word's bit 21.
            o.get(dereth_client_model::player::option::DISABLE_DISTANCE_FOG),
        );
        if self.last_option_environment == Some(now) {
            return;
        }
        let Some(mut world) = self
            .present
            .scene_mut(self.world.as_mut())
            .filter(|_| self.pending_scene.is_none())
        else {
            // No scene, no landscape. The latch is deliberately **not** advanced, for exactly the
            // reason `update_target_tracking`'s is not: an option set while a scene was still
            // loading would otherwise be lost for the rest of the session.
            return;
        };
        let (day, no_weather, no_fog) = now;
        world.set_always_daylight(day);
        world.set_weather_enabled(!no_weather);
        world.set_world_fog(!no_fog);
        self.last_option_environment = Some(now);
    }

    /// Apply the camera's turn decision to the body.
    ///
    /// `CameraEffects`' turn-player and stop-turn fields are written by `Rotate`; this is their
    /// reader. Without it **turning in first person, and turning with `Input.UseMouseTurning` on,
    /// reaches no body at all**. Retail dispatches three different command-interpreter actions:
    /// move the player, stop drift, and turn to a heading. Each behavior is transcribed on
    /// [`crate::character::Character`]; this is the wire between them.
    ///
    /// Retail makes the call from inside `Rotate`, i.e. from inside
    /// the gameplay UI's camera update, which is *before* physics in retail.
    /// Here the camera's frame runs after `WorldScene::update`
    /// (its placement is measured by the sweep-lag tests), so a turn a held key repeats lands
    /// on the body in the same frame — `do_motion` applies to motion interpolation immediately — and
    /// is stepped by the *next* physics tick rather than this one. That is the same measured
    /// one-tick offset the camera placement already has, not a new one.
    ///
    /// Sending a movement event has the mouse-turning arm's own 0.5 s throttle, which is
    /// **not** [`dereth_client_net::client_session::PositionReporter`]'s change detector: retail runs both, and this
    /// does too.
    /// The input poll's idle tick: while mouse look holds and the mouse has not moved for 0.2 s,
    /// a still delta reaches the mouse-look handler every frame, which with mouse turning on
    /// stops the body's turn.
    pub(super) fn mouse_look_idle(&mut self, now: dereth_primitives::LocalTime) {
        if !self.mouse_look || self.timer.cur_time < self.last_mouse_move + MOUSE_LOOK_IDLE {
            return;
        }
        if let Some(mut world) = self.present.scene_mut(self.world.as_mut()) {
            if world.world_mut().character.is_some() {
                crate::camera::mouse_look(&mut *world, 0.0, 0.0, now);
            }
        }
    }

    /// The orbit camera, when an interface asked for it: put in use on the body's camera with
    /// its settings, and under camera-based movement the player faced where the camera looks (or
    /// straight away from it, Back alone held) while a movement key is held. Without it the
    /// body's camera is the game's.
    pub(super) fn apply_orbit_camera(&mut self) {
        let settings = self.orbit;
        let keys = self.orbit_keys;
        let Some(c) = self.world.as_mut().and_then(|w| w.character.as_mut()) else {
            return;
        };
        c.camera.orbit_active = settings.is_some();
        // The orbit camera eases every frame, so the body it follows is drawn moving every frame.
        c.drawn_between_ticks = settings.is_some();
        let Some(settings) = settings else {
            c.camera.orbit_turns_with_player = false;
            return;
        };
        c.camera.orbit.apply_settings(settings);
        c.camera.orbit.stick = self.orbit_look;
        // While a spell is cast with the right button held, the mouse moved across turns the
        // player by the game's own turning keys, the camera following.
        let across = if keys.mouse_turns_player() {
            c.camera.orbit_mouse_turn.replace(0.0).unwrap_or(0.0)
        } else {
            c.camera.orbit_mouse_turn = None;
            0.0
        };
        #[allow(clippy::cast_possible_truncation)]
        let dt = (self.timer.cur_time - self.last_time).clamp(0.0, 0.25) as f32;
        let turns = self.orbit_keys.cast_mouse(across, dt);
        self.orbit_pending.extend(turns);
        let keys = self.orbit_keys;
        if !c.camera.orbit.placed {
            return;
        }
        let camera = c.camera.orbit.movement_heading(false);
        let face = keys.facing(settings, camera);
        c.camera.orbit_turns_with_player = keys.camera_turns_with_player(settings);
        // A mouse button held keeps the camera where it is: it no longer settles behind.
        if keys.buttons.0 || keys.buttons.1 {
            c.camera.orbit.settling_behind = false;
        }
        // Turned by the game itself (a move or turn toward something used, cast at or fought)
        // while the player neither steers nor holds a mouse button: the camera comes behind.
        c.camera.orbit.follow_behind = keys.camera_follows_game_turn(face, c.is_moving_to());
        if let Some(face) = face {
            c.face_heading(face);
        }
    }

    pub(super) fn apply_camera_turn(&mut self, now: dereth_primitives::LocalTime) {
        let Some(mut world) = self
            .present
            .scene_mut(self.world.as_mut())
            .filter(|_| self.pending_scene.is_none())
        else {
            return;
        };
        let Some(c) = world.world_mut().character.as_mut() else {
            return;
        };
        let (turn, send) = c.camera.take_turn();
        if turn.is_none() && !send {
            return;
        }
        match turn {
            Some(crate::camera::CameraTurn::TurnToHeading { heading }) => {
                c.turn_to_heading(heading);
            }
            Some(crate::camera::CameraTurn::MovePlayer { command, extent }) => {
                c.camera_turn_motion(dereth_animation::MotionCommand(command), extent);
            }
            Some(crate::camera::CameraTurn::StopDrift) => c.stop_drift(),
            None => {}
        }
        drop(world);
        if turn.is_some() {
            self.events.push(FrameEvent::WorldCameraTurnApplied);
        }
        if send {
            if let Some(motion) = self.player_motion() {
                if let Some(link) = self.link.as_mut() {
                    self.position
                        .send_movement_event(now.0, &motion, &mut link.net.session);
                }
            }
        }
    }

    /// The eight `CameraCommand`s [`Self::apply_camera_action`] declines.
    ///
    /// `crate::camera::CameraControl::on_action` implements every one of them — the two zooms,
    /// the three view modes, `SetDefaultOffsets` and the two mouse-look toggles. Its other caller,
    /// [`crate::camera::CameraControl::apply_input`], can construct exactly six of its twelve
    /// commands: `Rotate`, `StopRotating`, `Raise`, `StopRaising`, `Lower` and `StopLowering`.
    /// Reaching the other eight needs the world, which `apply_input_actions` does not take; this
    /// is that plumbing.
    ///
    /// The four *rotation* commands are deliberately **not** here: they stay on
    /// [`Self::apply_camera_action`]'s held-key flags, which the camera update repeats every frame
    /// and the body-less flycam also reads. Routing them
    /// through both would apply each press twice.
    ///
    /// Action `0x3E` is the one with a second half: it registers input map **6** at the
    /// unfocused-UI input priority while it is held and unregisters it on the release, *then* falls
    /// through to `0x3D`'s `ToggleMouseLook`. That registration is why it is here rather than in the
    /// world-free arm — and it is done even when there is no body, because the map stack is the
    /// input manager's and exists in every build.
    fn apply_world_camera_action(
        &mut self,
        shell: &mut S,
        cmd: crate::actions::camera::CameraCommand,
    ) -> bool {
        use crate::actions::camera::CameraCommand as C;
        match cmd {
            C::Closer { .. }
            | C::StopCloser
            | C::Farther { .. }
            | C::StopFarther
            | C::SetDefaultOffsets
            | C::SetInHead
            | C::ToggleLookDown
            | C::ToggleMapMode
            | C::ToggleMouseLook(_)
            | C::FrontView(_) => {}
            C::AlternateMode { on } => {
                shell.control_notice(crate::shell::ControlNotice::AlternateCamera(on));
            }
            // The four rotations, and `NotHandled`.
            _ => return false,
        }
        let now = self.timer.cur_time;
        self.with_camera(|camera, player| camera.on_action(cmd, player, now))
    }

    /// Run `f` against the body's [`crate::camera::CameraControl`] and its object id, answering
    /// whether there was a body to run it against.
    ///
    /// Without a body there is no `CameraState` at all — the free camera is a debug flycam with no
    /// zoom, no first person and no map mode — so the eight commands are consumed and do nothing,
    /// exactly as they would with a null body and the camera handler's own active guard failing.
    fn with_camera(
        &mut self,
        f: impl FnOnce(&mut crate::camera::CameraControl, dereth_primitives::ObjectId),
    ) -> bool {
        let Some(c) = self.world.as_mut().and_then(|w| w.character.as_mut()) else {
            return true;
        };
        let id = c.object_id();
        f(&mut c.camera, id);
        true
    }

    /// The four look controls of the camera action set, applied to the held-key
    /// flags both cameras read.
    ///
    /// `CameraState` repeats a held rotation every frame from `update_camera` rather than acting once
    /// per event, which is what these four bools are: with a body they are
    /// [`crate::camera::CameraControl::apply_input`]'s input and become `Rotate`/`Raise`/`Lower`
    /// again on the far side; without one they are the flycam's yaw and pitch.
    ///
    /// The other **eight** commands the decoder produces — the two zooms, the three view modes,
    /// `SetDefaultOffsets` and the two mouse-look toggles — need the world, and are routed
    /// through [`Self::apply_world_camera_action`] instead.
    fn apply_camera_action(
        input: &mut CameraInput,
        cmd: crate::actions::camera::CameraCommand,
    ) -> bool {
        use crate::actions::camera::CameraCommand as C;
        match cmd {
            C::Rotate { left: true, .. } => input.look_left = true,
            C::StopRotating { left: true } => input.look_left = false,
            C::Rotate { left: false, .. } => input.look_right = true,
            C::StopRotating { left: false } => input.look_right = false,
            C::Raise { .. } => input.look_up = true,
            C::StopRaising => input.look_up = false,
            C::Lower { .. } => input.look_down = true,
            C::StopLowering => input.look_down = false,
            _ => return false,
        }
        true
    }
}
