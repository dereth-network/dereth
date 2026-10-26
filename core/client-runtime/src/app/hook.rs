//! The frame's extension step: installing a frame hook, running it, and the plugin audit answer.

use super::*;

impl<S: Shell> App<S> {
    /// Install the frame's extension step (see [`crate::frame_hook`]), replacing any installed
    /// before; answers the one it replaced.
    pub fn install_frame_hook(
        &mut self,
        hook: Box<dyn crate::frame_hook::FrameHook>,
    ) -> Option<Box<dyn crate::frame_hook::FrameHook>> {
        self.hook_chat.clear();
        self.hook_outcomes.clear();
        self.frame_hook.replace(hook)
    }

    /// Take the installed frame hook out; the frame runs without one from then on.
    pub fn remove_frame_hook(&mut self) -> Option<Box<dyn crate::frame_hook::FrameHook>> {
        self.hook_chat.clear();
        self.hook_outcomes.clear();
        self.interaction.set_extension_verbs(Vec::new());
        self.hook_messages.clear();
        if let Some(link) = self.link.as_mut() {
            link.net.session.keep_sent(false);
        }
        self.frame_hook.take()
    }

    /// The installed frame hook.
    #[must_use]
    pub fn frame_hook(&self) -> Option<&dyn crate::frame_hook::FrameHook> {
        self.frame_hook.as_deref()
    }

    /// The installed frame hook, mutably.
    pub fn frame_hook_mut(&mut self) -> Option<&mut (dyn crate::frame_hook::FrameHook + 'static)> {
        self.frame_hook.as_deref_mut()
    }

    /// Keep lines for the frame hook, while there is one.
    pub(super) fn gather_hook_chat(
        &mut self,
        lines: Vec<dereth_client_contract::chat::interface::ChatMessage>,
    ) {
        if self.frame_hook.is_some() {
            self.hook_chat.extend(lines);
        }
    }

    /// Run the installed frame hook, and apply what it asked for.
    pub(super) fn run_frame_hook(&mut self) {
        let Some(mut hook) = self.frame_hook.take() else {
            return;
        };
        let chat = std::mem::take(&mut self.hook_chat);
        let outcomes = std::mem::take(&mut self.hook_outcomes);
        let mut messages = std::mem::take(&mut self.hook_messages);
        if let Some(link) = self.link.as_mut() {
            messages.extend(
                link.net
                    .session
                    .take_sent()
                    .into_iter()
                    .map(crate::frame_hook::WireMessage::sent),
            );
        }
        let typed = self.interaction.take_extension_commands();
        let account = self
            .link
            .as_ref()
            .map(|link| link.net.characters().account.clone())
            .filter(|a| !a.is_empty());
        let session = self.hook_session_facts();
        let output = {
            let view = self.hud.view(&self.objects);
            let player = self
                .world
                .as_ref()
                .and_then(|w| w.character.as_ref())
                .map(crate::character::Character::position);
            // Lines of sight are swept through the body's own world.
            let sight = self
                .world
                .as_mut()
                .and_then(|w| w.character.as_mut())
                .map(|c| crate::sight::Sight::new(&mut c.world, c.handle));
            let places = crate::frame_hook::StreamPlaces {
                player,
                objects: &self.objects,
                autorun: self.char_input.auto_run,
                sight: sight.as_ref(),
            };
            let mut frame = crate::frame_hook::HookFrame::new(
                &view,
                &self.objects.world,
                self.interaction.command_table(),
                self.timer.cur_time,
                self.timer.elapsed(),
                &chat,
                &typed,
            )
            .with_outcomes(&outcomes)
            .with_messages(&messages)
            .with_session(self.host_state.world_name.as_deref(), account.as_deref())
            .with_pregame(&session)
            .with_places(&places)
            .with_travel(
                &self.travel_status,
                (!self.travel.store().world.is_empty()).then(|| self.travel.store()),
            )
            .with_control(
                // The player's own movement since the hook last ran, carried out or still to be
                // offered, under any interface.
                self.player_moves != self.hook_player_moves || self.actions.player_movement(),
                // The server took control of the character's movement since the hook last ran.
                // (Being under its control is a latch the player's next movement lifts, set
                // with the character standing still after a teleport; the taking is the event.)
                self.movement.lists.control_losses != self.hook_control_losses,
            );
            hook.step(&mut frame);
            frame.into_output()
        };
        self.hook_control_losses = self.movement.lists.control_losses;
        self.hook_player_moves = self.player_moves;
        // The client keeps the frame's messages only while the hook reads them.
        if let Some(link) = self.link.as_mut() {
            link.net.session.keep_sent(hook.wants_messages());
        }
        self.apply_hook_nav(&output.nav);
        for r in output.travel {
            self.travel.request(r);
        }
        self.apply_hook_session(&output.session, session.phase);
        // A hidden line is taken out of the chat windows' queue: the last queued copy of it, since
        // this frame's lines are the queue's newest.
        for &i in &output.hidden {
            let line = &chat[i];
            if let Some(pos) = self.hud.pending_chat.iter().rposition(|(_, m)| m == line) {
                self.hud.pending_chat.remove(pos);
            }
        }
        self.hud
            .pending_chat
            .extend(output.printed.into_iter().map(|m| (None, m)));
        if !output.requests.is_empty() {
            self.interaction.queue(Vec::new(), output.requests);
        }
        if let Some(verbs) = output.verbs {
            self.interaction.set_extension_verbs(verbs);
        }
        self.frame_hook = Some(hook);
    }

    /// Where a place in the world falls on the screen this frame, in back-buffer pixels: the
    /// inverse of the selection ray (the same viewer, field of view and game viewport), so a mark
    /// drawn there sits on what a click there would pick. `None` behind the camera, outside the
    /// game view, or with no world drawn.
    #[must_use]
    pub fn screen_point(&self, pos: &dereth_primitives::Position) -> Option<(f32, f32)> {
        self.screen_point_depth(pos, 0.0).map(|(x, y, _)| (x, y))
    }

    /// Where a place in the world falls in the world's own drawing space this frame (Z up), for a
    /// mark drawn in the world. `None` with no world drawn.
    #[must_use]
    pub fn render_point(&self, pos: &dereth_primitives::Position) -> Option<[f32; 3]> {
        let at = self
            .world
            .as_ref()?
            .character
            .as_ref()?
            .render_frame_of(*pos)
            .origin;
        Some([at.x, at.y, at.z])
    }

    /// [`Self::screen_point`] with the place's depth as the world's drawing stores it (0 at the
    /// near plane, 1 at the far one), so something drawn there can be tested against the world.
    /// The depth is taken `toward` metres nearer the camera, so that a label is not hidden by
    /// the very thing it names.
    #[must_use]
    pub fn screen_point_depth(
        &self,
        pos: &dereth_primitives::Position,
        toward: f32,
    ) -> Option<(f32, f32, f32)> {
        let world = self.world.as_ref()?;
        let at = world.character.as_ref()?.render_frame_of(*pos).origin;
        let scene = self.present.scene(Some(world))?;
        let pick = scene.as_pick_scene();
        let viewer = pick.viewer();
        let screen = self.present.size();
        let fov = pick.fov_y_rad(screen);
        let vp = self.interaction.device_viewport(screen);
        let (xa, ya, za) = crate::pick_geometry::viewer_axes(&viewer);
        let d = dereth_primitives::frame::V3::sub(at, viewer.origin);
        let (x, y, z) = (d.dot(xa), d.dot(ya), d.dot(za));
        if y < 0.5 {
            return None;
        }
        #[allow(clippy::cast_precision_loss)] // viewport extents are small integers
        let (half_w, half_h) = (
            (vp.width as f32 - 1.0) * 0.5,
            (vp.height as f32 - 1.0) * 0.5,
        );
        let vdst = half_h / dereth_primitives::num::math::tanf(fov * 0.5);
        let (px, py) = (x * vdst / y + half_w, -z * vdst / y + half_h);
        // The perspective projection's depth: `q (1 - near / y)`, `q = far / (far - near)`.
        let view = scene.view_params(screen.0, screen.1);
        let q = view.zfar / (view.zfar - view.znear);
        let depth = (q * (1.0 - view.znear / (y - toward).max(view.znear))).clamp(0.0, 1.0);
        #[allow(clippy::cast_precision_loss)]
        let inside = (0.0..vp.width as f32).contains(&px) && (0.0..vp.height as f32).contains(&py);
        #[allow(clippy::cast_precision_loss)]
        inside.then_some((vp.x as f32 + px, vp.y as f32 + py, depth))
    }

    /// The session's phase and the account's characters, for the frame hook.
    fn hook_session_facts(&self) -> crate::frame_hook::SessionFacts {
        let Some(link) = self.link.as_ref() else {
            return crate::frame_hook::SessionFacts::default();
        };
        crate::frame_hook::SessionFacts {
            phase: crate::frame_hook::SessionPhase::of(link.net.session_state()),
            characters: link
                .net
                .characters()
                .characters
                .iter()
                .map(|c| (c.gid, c.name.clone()))
                .collect(),
            portal_space: self.teleport.teleport_in_progress(),
            // The interface says when its character creation screens are up
            // (`UiRequest::CharacterCreation`).
            creating_character: self.duties.creating_character,
        }
    }

    /// The frame hook's movements, applied to the player's character as a player's turning and
    /// run lock are: a turn goes through the turn-to-heading movement and is reported to the
    /// server as the camera's turn is; the run lock is toggled by its own action when it differs.
    fn apply_hook_nav(&mut self, requests: &[crate::frame_hook::NavRequest]) {
        use crate::frame_hook::NavRequest;
        for r in requests {
            match *r {
                NavRequest::Face(heading) => {
                    let turned = self
                        .present
                        .scene_mut(self.world.as_mut())
                        .filter(|_| self.pending_scene.is_none())
                        .and_then(|mut world| {
                            let c = world.world_mut().character.as_mut()?;
                            c.turn_to_heading(heading.rem_euclid(360.0));
                            Some(())
                        })
                        .is_some();
                    if turned {
                        if let Some(motion) = self.player_motion() {
                            if let Some(link) = self.link.as_mut() {
                                self.position.send_movement_event(
                                    self.timer.cur_time,
                                    &motion,
                                    &mut link.net.session,
                                );
                            }
                        }
                    }
                }
                NavRequest::Autorun(on) => {
                    // The client's own steering: the game's run lock, whatever the interface
                    // makes of the player's keys.
                    if self.char_input.auto_run != on {
                        self.actions.steer(crate::actions::Action::begin(
                            dereth_client_contract::actions::movement::AUTORUN,
                        ));
                    }
                }
                NavRequest::Hold(key, down) => {
                    // The client's own steering too: the control's press or release reaches the
                    // movement step as the game's own movement, the jump's charge and leap
                    // included.
                    let id = key.action();
                    self.actions.steer(if down {
                        crate::actions::Action::begin(id)
                    } else {
                        crate::actions::Action::end(id)
                    });
                }
                NavRequest::CancelJump => {
                    let now = dereth_primitives::LocalTime(self.timer.cur_time);
                    if self.objects.world.combat.jump_power_level(now) > 0.0
                        || self.objects.world.combat.jump_pending
                    {
                        crate::jump::finish(
                            &mut self.objects.world.combat,
                            self.world.as_ref().and_then(|w| w.character.as_ref()),
                        );
                    }
                }
            }
        }
    }

    /// The frame hook's pre-game operations: entering the world as a character, logging off, and
    /// quitting (after the log-off, when a character is in the world).
    fn apply_hook_session(
        &mut self,
        requests: &[crate::frame_hook::SessionRequest],
        phase: crate::frame_hook::SessionPhase,
    ) {
        use crate::frame_hook::{SessionPhase, SessionRequest};
        if self.hook_quit_pending && phase != SessionPhase::InWorld {
            self.hook_quit_pending = false;
            self.done();
        }
        for r in requests {
            match *r {
                SessionRequest::LogOn(gid) => {
                    if phase == SessionPhase::CharacterSelect {
                        self.run_character_actions(vec![
                            dereth_client_contract::pregame::CharacterAction::LogOn(gid),
                        ]);
                    }
                }
                SessionRequest::LogOff => {
                    if phase == SessionPhase::InWorld {
                        self.log_off_character();
                    }
                }
                SessionRequest::Quit => {
                    if phase == SessionPhase::InWorld {
                        self.log_off_character();
                        self.hook_quit_pending = true;
                    } else {
                        self.done();
                    }
                }
            }
        }
    }

    /// Answer `Admin_QueryPluginList` (`0x02AE`) and `Admin_QueryPlugin` (`0x02B1`): the installed
    /// frame hook's answer, or, without one, retail's answer when no third-party interface runs.
    pub(super) fn answer_plugin_audit(
        &mut self,
        event: &dereth_client_net::client_session::SessionEvent,
    ) {
        let Some(reply) = crate::frame_hook::audit_reply(event, self.frame_hook.as_deref()) else {
            return;
        };
        let sent = self.link.as_mut().map(|l| match &reply {
            crate::frame_hook::AuditReply::List(m) => l.net.session.send_action(m),
            crate::frame_hook::AuditReply::One(m) => l.net.session.send_action(m),
        });
        if let Some(Err(e)) = sent {
            tracing::warn!("the plugin audit answer could not be sent: {e}");
        }
    }
}
