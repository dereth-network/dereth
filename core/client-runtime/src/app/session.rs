//! Login queues, character sessions, and host state.

use super::*;

impl<S: Shell> App<S> {
    /// Apply the environment-control message's local visual and sound effects.
    ///
    /// `true` means the event belonged to the client's visual `0..=6` / `9999` cases or the
    /// `101..=124` sound range, including the three sound values which deliberately fall through
    /// its switch. Malformed and unknown options remain available to the generic diagnostics.
    fn apply_admin_environs(
        &mut self,
        shell: &S,
        event: &dereth_client_net::client_session::SessionEvent,
    ) -> bool {
        let Some((opcode, body)) = event.ui_body() else {
            return false;
        };
        if opcode != dereth_protocol::Opcode::ADMIN_ENVIRONS {
            return false;
        }
        let Ok(message) =
            dereth_protocol::read_body_padded::<dereth_protocol::admin::AdminEnvirons>(body)
        else {
            return false;
        };
        if let Some(blank) = self
            .environment_override
            .apply_option(message.environ_option)
        {
            self.hud.set_admin_radar_blank(blank);
            if let Some(mut world) = self.present.scene_mut(self.world.as_mut()) {
                world.sync_environment_override_flags();
            }
            return true;
        }
        if !dereth_protocol::admin::AdminEnvirons::is_sound_cue(message.environ_option) {
            return false;
        }

        // The outer range encloses three absent switch cases: they are handled no-ops, not an
        // unknown Admin_Environs operation. All remaining exits preserve the client's order:
        // player physics object, UI system/table, and only then centered table playback.
        let Ok(option) = u32::try_from(message.environ_option) else {
            return true;
        };
        let Some(stype) = dereth_audio::trigger::environ_sound_type(option) else {
            return true;
        };
        let Some(player) = self.objects.player() else {
            return true;
        };
        if self.objects.world.physics(player).is_none() {
            return true;
        }
        // The live UI-system lookup is a separate client guard from the cached table. The
        // latter can outlive a screen transition in this host; it must not stand in for a UI
        // system that was never created.
        if !shell.has_ui() {
            return true;
        }
        if let (Some(table), Some(audio)) = (self.ui_sound_table, self.audio.as_mut()) {
            audio.play_ui_sound(dereth_audio::UiSoundRef::Table { table, stype });
        }
        true
    }

    /// Assemble [`HostState`] from the session, once per frame.
    ///
    /// Every value is read from somewhere that already exists: the character set is `0xF658`,
    /// `in_world` is `SessionState::Playable`, and the error is whatever ended the session.
    pub(super) fn build_host_state(&mut self) {
        let has_net = self.link.is_some();
        let (connected, in_world, error) = match self.link.as_ref() {
            Some(link) => (
                link.net.status() == crate::net::LinkStatus::Connected,
                link.net.session.state()
                    == dereth_client_net::client_session::SessionState::Playable,
                // The transport's own error if it has one, and otherwise **whatever was already
                // latched**. Overwriting the field with `None` every frame would erase a character
                // error raised by [`Self::recv_disconnect_notice`] *in the same frame it was
                // raised*, before the front end could see the edge, and a disconnected client
                // would never reach the disconnected screen with a server on the line.
                // Latching is the same statement the `None` arm below already makes, and for the
                // same reason: the notice is raised once and there is no reconnect path.
                link.net
                    .error()
                    .map(|c| c.id_string().to_string())
                    .or_else(|| self.host_state.error.clone()),
            ),
            // With no `--connect` there is no shared network to ask and no packet controller to wait
            // for. On this no-controller branch the set is not required, so readiness is immediate.
            // The connect meter is reported
            // satisfied because there is no connection to be waiting on -- a statement about *this
            // build having no network*, not a claim about the retail client, which always connects
            // before and therefore always has one.
            //
            // The error is **latched**, not cleared: the character-error event is a notice raised
            // once, and with no shared network there is nothing to ask
            // whether it is still true. Overwriting it with `None` every frame would erase a
            // locally-raised character error — the subscription-expired error is exactly that —
            // before the shell's own edge test could see it.
            None => (true, false, self.host_state.error.clone()),
        };
        self.host_state.connected = connected;
        self.host_state.has_packet_controller = has_net;
        self.host_state.in_world = in_world;
        if self.host_state.disconnect.is_none() {
            if let Some(code) = self.link.as_ref().and_then(|l| l.net.error()) {
                self.host_state.disconnect =
                    Some(dereth_client_contract::pregame::DisconnectNotice::Net(
                        code.id_string().to_string(),
                    ));
            }
        }
        self.host_state.error = error;
        // With a live connection the DDD stream is real and `patch_finished` is what the
        // patch-time-end message says rather than an assumption. Without one there is no shared
        // asset cache to interrogate anything, so the phase is over before it starts.
        if !has_net {
            self.host_state.patch_finished = true;
        }
        // Drained into the screen once per frame; `HostState` is cloned into `UiShell::frame`, so
        // the queue must be emptied here or every event would be delivered again next frame.
        self.host_state.ddd = std::mem::take(&mut self.pending_ddd);
        let phase = self.game_phase();
        if phase != self.host_state.phase {
            tracing::debug!("game phase {:?} -> {phase:?}", self.host_state.phase);
            self.host_state.phase = phase;
            self.host_state.phase_changes = self.host_state.phase_changes.wrapping_add(1);
        }
    }

    /// Where the game is this frame ([`dereth_client_contract::pregame::GamePhase`]), from the
    /// session and the requests the UI has made.
    ///
    /// The edges are the ones the retail flow takes: the character list is reachable once the
    /// connection is up, the data check is over and the list has arrived (or there is no server
    /// to wait for); the world is entered on the session becoming playable; and a log-off returns
    /// to the list only when the server answers it, which is when the session leaves the world.
    fn game_phase(&mut self) -> dereth_client_contract::pregame::GamePhase {
        use dereth_client_contract::pregame::{DisconnectNotice, GamePhase};
        use dereth_client_net::client_session::SessionState;
        if let Some(notice) = &self.host_state.disconnect {
            return GamePhase::Disconnected(notice.clone());
        }
        let Some(link) = self.link.as_ref() else {
            // No server: the character list is up from the start, empty.
            return if self.duties.creating_character {
                GamePhase::CharacterCreation
            } else {
                GamePhase::CharacterSelect
            };
        };
        let state = link.net.session.state();
        if state != SessionState::Playable {
            self.duties.leaving_world = false;
        }
        if state != SessionState::CharacterSelect {
            self.duties.creating_character = false;
        }
        match state {
            SessionState::Connected => GamePhase::Connecting,
            SessionState::Patching => GamePhase::Patching,
            SessionState::CharacterSelect if !self.host_state.connected => GamePhase::Connecting,
            SessionState::CharacterSelect if !self.host_state.patch_finished => GamePhase::Patching,
            SessionState::CharacterSelect if self.duties.creating_character => {
                GamePhase::CharacterCreation
            }
            SessionState::CharacterSelect => GamePhase::CharacterSelect,
            SessionState::EnteringWorld => GamePhase::EnteringWorld,
            SessionState::Playable if self.duties.leaving_world => GamePhase::LoggingOff,
            SessionState::Playable => GamePhase::InWorld,
            SessionState::Disconnected(_) => GamePhase::Disconnected(DisconnectNotice::ServerDied),
        }
    }

    /// Step 7's UI update and mode switch. Player-description arrival triggers
    /// loading the automatic screen layout.
    ///
    /// Kept as a narrow test seam for the path-and-file stations. The production arm is
    /// separately covered by a socket-free transport station, which delivers a wire-framed
    /// `0x0013` through the client network, `Session` and the App event drain.
    pub fn note_player_description(&mut self) {
        self.pending_auto_layout = true;
    }

    /// Whether [`Self::note_player_description`]'s load is still outstanding.
    #[must_use]
    pub fn auto_layout_pending(&self) -> bool {
        self.pending_auto_layout
    }

    /// Request character logoff through `dereth_client_net::client_session::Session::log_off`.
    ///
    /// **The client's function takes a `bool` and this one does not, which is a declared
    /// simplification and not a transcription.** The direct character-logoff arm used during
    /// epilogue-screen construction saves character state and executes logoff **directly**: no `0xF653`, no three
    /// second deadline, no portal space, because the process is about to exit.
    /// The ordinary character-logoff arm — also used by the 1,200-second
    /// idle kick — saves character state, then (unless an item request is outstanding, in which case
    /// logoff is deferred to the next opportunity) requests departure,
    /// which is the departure. This build runs the `false` arm for both, and the difference is
    /// invisible on the quit path only because the epilogue screen requests device shutdown in the same
    /// frame. The `ask` flag on `UiRequest::EndCharacterSession` already carries the distinction;
    /// nothing reads it yet.
    pub fn log_off_character(&mut self) {
        if let Some(effect) = self.resolution.cancel() {
            self.apply_resolution_effect(effect);
        }

        // The logoff request is sent and the log-off-requested flag is armed; that flag fades
        // the world out three seconds later,
        // plus twenty when the local player's `is_player_killer` predicate answers true — so
        // the fade is armed even when there is no link to tell.
        let is_player_killer = self
            .objects
            .player()
            .and_then(|id| self.objects.world.weenie(id))
            .is_some_and(dereth_client_model::Weenie::is_player_killer);
        // **Saving the player module, which is the *first* thing both
        // character-logoff arms do.**
        //
        // ```text
        // direct epilogue arm  : save character state ; execute logoff
        // ordinary logoff arm : save character state ; request logoff
        //
        // save(force): if the module is dirty or forced, send the whole player module
        //              then clear the dirty flag
        // ```
        //
        // Without it the `0x01A1` that carries the **whole** player module never goes out on the
        // way out. That is not a cosmetic gap: individual changes send a single-option
        // `0x0005` only for the twenty-one auto-save options, while
        // changes to every other gameplay option, including the chat
        // windows' text filters (`0x1000007F`) and all of their placement — sends **nothing** and
        // only stamps the module dirty. So the other 31 character options, every chat filter and
        // every window position would reach the shard only by two accidents: the 480-second
        // player-module flush, and reopening an options page (whose show edge
        // raises `UiRequest::SavePlayerOptions`). Change one and log out inside eight minutes and
        // the next login would come back to the old value.
        //
        // `force` is **false**, which is the arm both callers use: a session that deferred nothing
        // still sends nothing, because the save gate is `dirty || force`. The order
        // is the client's too — the save precedes departure request `0xF653`, so the
        // module is on the wire before the departure is.
        if self.link.is_some() {
            let mut req = dereth_client_model::RecordingRequests::default();
            if self
                .objects
                .world
                .player_system
                .save_to_server(&mut req, false)
            {
                self.events.push(FrameEvent::PlayerModuleSavedAtLogout);
            }
            for r in req.0 {
                if let Some(link) = self.link.as_mut() {
                    let _ = crate::requests::send_request(&mut link.net.session, &r);
                }
            }
        }

        self.teleport
            .request_log_off(self.timer.cur_time, is_player_killer);
        self.duties.leaving_world = true;
        if let Some(link) = self.link.as_mut() {
            tracing::info!("the epilogue UI requested character logoff");
            // The departure request sends before its local teardown tail.
            link.net.session.log_off();
        }

        // Local teardown disables movement. The returned guard is
        // `autonomy_level && player && !controlled_by_server`, split at the body boundary here.
        let apply_and_send = self.movement.disable(&mut self.char_input);
        if apply_and_send {
            if let Some(character) = self.world.as_mut().and_then(|w| w.character.as_mut()) {
                character.flush_command_input(self.char_input);
            }
            if let Some(motion) = self.player_motion() {
                if let Some(link) = self.link.as_mut() {
                    let _ = self.position.send_movement_event(
                        self.timer.cur_time,
                        &motion,
                        &mut link.net.session,
                    );
                }
            }
        }
    }

    /// Leave the game. With a character in the world it logs off first, as the game's own quit
    /// does: the player module goes to the server ahead of the departure, and the loop ends at
    /// the next frame's UI step, once that frame's packet step has carried both out. A dead
    /// server costs that one frame and no more. Anywhere else the device is done at once.
    pub fn quit_game(&mut self) {
        use dereth_client_net::client_session::SessionState;
        let in_world = !self.duties.leaving_world
            && self
                .link
                .as_ref()
                .is_some_and(|link| link.net.session.state() == SessionState::Playable);
        if in_world {
            self.log_off_character();
            self.duties.quit_owed = true;
        } else {
            self.pump.done();
        }
    }

    /// The player system's three character calls, made on the session instead of on a singleton.
    ///
    /// Log-on is the two-step `0xF7C8` / `0xF657` exchange (`Session::enter_world` is that state
    /// machine), delete is `0xF655` addressed by **slot**, and restore is `0xF7D9` on net queue 2.
    pub fn run_character_actions(
        &mut self,
        actions: Vec<dereth_client_contract::pregame::CharacterAction>,
    ) {
        use dereth_client_contract::pregame::CharacterAction;
        if actions.is_empty() {
            return;
        }
        let Some(link) = self.link.as_mut() else {
            for a in actions {
                tracing::warn!("{a:?} with no server to ask");
            }
            return;
        };
        let account = link.net.characters().account.clone();
        for a in actions {
            match a {
                CharacterAction::LogOn(gid) => {
                    let name = link
                        .net
                        .characters()
                        .characters
                        .iter()
                        .find(|c| c.gid == gid)
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    tracing::info!("entering the world as {name:?} ({:#010X})", gid.0);
                    // Building the screen-layout path asks
                    // the player object's singular name for half of the automatic layout file's
                    // name. This build has
                    // no `WorldObjects` singleton, so the name is recorded on the edge that knows it.
                    self.host_state.entered_character = Some(name.clone());
                    link.net.enter_world(gid, &account);
                    self.script = EnterWorldScript::Entering;
                }
                CharacterAction::Delete(gid) => {
                    tracing::info!("deleting {:#010X}", gid.0);
                    link.net.session.delete_character(gid);
                }
                CharacterAction::Restore(gid) => {
                    // Restore-character requests use net queue **2**, with two
                    // empty packed strings after the id.
                    tracing::info!("restoring {:#010X}", gid.0);
                    link.net.session.restore_character(gid);
                }
            }
        }
    }

    /// Send the character-generation result and the log-on that follows a successful creation.
    ///
    /// The conversion from the UI's `CharGenResultData` to the protocol's
    /// `dereth_protocol::login::CharGenResult` is field for field and belongs to neither side, so
    /// it lives here — the same seam, in the same direction, as `ui::character_set_from_login`. The
    /// **checksum** is computed rather than carried: sums
    /// fields 2-10, 12, 14, 16 and 24-31 as it writes them, and `dereth_protocol` already knows how.
    pub fn run_chargen_actions(
        &mut self,
        actions: Vec<dereth_client_contract::pregame::CharGenAction>,
    ) {
        use dereth_client_contract::pregame::CharGenAction;
        if actions.is_empty() {
            return;
        }
        let Some(link) = self.link.as_mut() else {
            for a in actions {
                tracing::warn!("{a:?} with no server to ask");
            }
            return;
        };
        for a in actions {
            match a {
                CharGenAction::SendCharGenResult(r) => {
                    let mut msg = chargen_result_to_wire(&r);
                    msg.checksum_value = msg.checksum();
                    tracing::info!(
                        "0xF656 creating {:?} -- heritage {}, gender {}, town {}, \
                         {} skill entries, slot {}",
                        msg.name,
                        msg.heritage_group,
                        msg.gender,
                        msg.start_area,
                        msg.skill_advancement_classes.len(),
                        msg.slot
                    );
                    link.net.session.create_character(msg);
                }
                CharGenAction::LogOn(gid) => {
                    let account = link.net.characters().account.clone();
                    tracing::info!("entering the world as the new {:#010X}", gid.0);
                    // See the other two sites. The name is not in the character
                    // set yet on this path (the server sends the new one back with `0xF658`), so
                    // it is looked up and left `None` when it is not there rather than guessed:
                    // an automatic layout file for a character with no name would be `UI--<world>`
                    // and would collide with every other unnamed one.
                    self.host_state.entered_character = link
                        .net
                        .characters()
                        .characters
                        .iter()
                        .find(|c| c.gid == gid)
                        .map(|c| c.name.clone());
                    link.net.enter_world(gid, &account);
                    self.script = EnterWorldScript::Entering;
                }
            }
        }
    }

    /// Show the first connection's failure, if the login has just ended before the link was up.
    ///
    /// Returns whether it did, which ends the frame loop. The box is shown once, through the
    /// platform's [`crate::platform::dialog::ErrorDialogHost`], and this returns only after the
    /// player has closed it; the same text goes to the log first so a run with no desktop still
    /// says why it stopped.
    pub(super) fn connect_failure_use_time(&mut self) -> bool {
        if self.connect_failure.is_some() {
            return true;
        }
        let Some(code) = self.link.as_ref().and_then(|link| link.net.login_refusal()) else {
            return false;
        };
        let failure = crate::connect_failure::connect_failure(code, &*self.store);
        tracing::warn!("net error {}", code.id_string());
        tracing::warn!("{}: {}", failure.popup.caption, failure.popup.text);
        self.last_net_error = Some(code);
        self.dialog.show_modal(&failure.popup);
        self.connect_failure = Some(failure);
        true
    }

    /// The first connection's failure and the box it was shown in, once the login has ended
    /// before the link was up. `None` while the login is running, after it has succeeded, and for
    /// a link that was up and then went down.
    #[must_use]
    pub fn connect_failure(&self) -> Option<&crate::connect_failure::ConnectFailure> {
        self.connect_failure.as_ref()
    }

    /// Act on what the session decoded this frame.
    ///
    /// The retail function drives the player system and the login UI; with no UI, this is
    /// the same decisions taken by `--enter-world`'s script, plus a log line per event so a
    /// headless run says what it reached. Everything it reads is a [`dereth_client_net::client_session::SessionEvent`];
    /// it decodes nothing.
    pub fn process_logon_event_queue(
        &mut self,
        shell: &mut S,
        events: Vec<dereth_client_net::client_session::SessionEvent>,
    ) {
        use dereth_client_net::client_session::{SessionEvent, SessionState};

        // **The two "any -> Disconnected" edges, taken first.**
        //
        // Character-error and server-died receivers
        // are *notices*: the player system raises them, they queue a UI mode, and they read
        // nothing from the shared network state. So they are taken here, **before** the link guard below,
        // which makes them arrive with or without a packet controller -- and it is the one call
        // site, so there is nothing for a second path to drift away from.
        for e in &events {
            self.recv_disconnect_notice(e);
            // **The phase-two smart-box reset, taken here for the same reason the two notices
            // above it are: before the link guard.**
            //
            // Character log-on phase 2 does not read shared network state to
            // decide to wipe the world — it wipes it and *then* asks the UI protocol to send. A
            // teardown that only ran while the link was healthy would skip exactly the endings
            // that need it (a transport drop, the 110 s timeout). See [`App::reset_world_view`].
            if matches!(e, SessionEvent::WorldReset) {
                self.reset_world_view();
                // **The subsystem sweep, on the same edge and for the same reason.**
                //
                // End-character-session handling is retail's *ending*, and in
                // retail it is reached by the same broadcast that reaches the
                // client object manager. It is taken here on the **entry** edge instead, which
                // is the same shape as `ObjectStream::reset` and `Teleport::reset`:
                // an ending this client never sees leaves the state set, and the entry edge is
                // reached by every ending there is. Nothing of the new session — its `0xF746`, its
                // objects, its `0x0013` — has arrived yet, so it needs no object-lifetime guard.
                //
                // See [`crate::interaction::Interaction::on_end_character_session`] for the field
                // list, which is read off rather than chosen here.
                self.interaction.on_end_character_session();
                // The same sweep empties the busy count: nothing the last character asked for is
                // still owed an answer.
                self.objects.world.magic.busy_count = 0;
                self.command_interpreter_disable();
                self.log_on_character_communication_clears();
            }
            if matches!(e, SessionEvent::LoggedOff) {
                // This client's own rule (client divergence CD-032): the world is torn down when
                // the server answers the log-off, not when the next character enters, so nothing
                // of it -- its scene, its objects, its sounds -- goes on behind the character
                // screen. The entry edge tears it down again, which finds nothing left to do.
                self.reset_world_view();
                if let Some(audio) = self.audio.as_mut() {
                    audio.end_world();
                }
            }
            if matches!(e, SessionEvent::PlayerCreated(_)) {
                // The player-description handler enables the command interpreter after accepting
                // the player id. Like Reset above,
                // this is local work owed by the already-decoded event, not by a surviving
                // packet controller.
                self.movement.enable(&mut self.char_input);
            }
            if let SessionEvent::CharacterSet(set) = e {
                // **The player system's account-name field.**
                //
                // Character-set delivery copies the unpacked account value into the field
                // **before** it looks
                // at the Turbine-chat flag and without consulting the link. That field is the one
                // `get_appropriate_spell_formula` hashes for a spell's tapers, so it is
                // taken here for the same reason the startup call below is: ahead of the link
                // guard, which a logon-queue delivery with no packet controller never passes.
                // `Hud`'s own `CharacterSet` arm sets the same field in stream order for the
                // batch path; both are plain assignments of the same string and are idempotent.
                self.objects
                    .world
                    .player_system
                    .account
                    .clone_from(&set.account);
                if set.use_turbine_chat != 0 {
                    // Startup performs local communication-provider registration before any player
                    // or gameplay screen exists. `Hud` already observes the stream-ordered model
                    // half; this idempotent call also serves direct logon-queue delivery.
                    self.objects.world.chat.startup_turbine_chat();
                    self.interaction.startup_turbine_chat_commands();
                }
            }
        }
        let Some(link) = self.link.as_mut() else {
            return;
        };
        let status = link.net.status();
        if self.last_link_status != Some(status) {
            self.last_link_status = Some(status);
            tracing::info!("link {status:?}");
        }
        let rejected = link.net.rejected();
        if rejected != self.last_rejected {
            self.last_rejected = rejected;
            tracing::warn!("{rejected} datagram(s) rejected");
        }
        if let Some(code) = link.net.error() {
            // Keyed on the code, not on a bool: the line prints
            // `code.id_string()`, so a latch prints the first error and swallows every distinct
            // one after it. See [`App::last_net_error`].
            if should_report_net_error(self.last_net_error, code) {
                self.last_net_error = Some(code);
                tracing::warn!("net error {}", code.id_string());
                // With a UI up the error is a *screen*, not an exit:
                // `build_host_state` already puts `id_string()` into `HostState::error` and
                // the front end queues the disconnected screen from it, and the screen's
                // OK button is the documented way out (the quit path goes gameplay ->
                // epilogue, never straight to `exit`). A run with no UI at all -- `--no-ui`, the
                // scripted slices -- still ends here, because in that build there is nothing that
                // could show the reason or take the player's answer, and so does an
                // `--enter-world` run, whose script has nobody to press the button.
                if !shell.has_ui() || self.cfg.enter_world {
                    self.script = EnterWorldScript::Done;
                }
            }
            return;
        }
        let scripted = self.cfg.enter_world;
        let no_ui = !shell.has_ui();
        let runtime_enters = !shell.drives_scripted_entry();
        let wanted = self.cfg.start_char.clone();

        // The `--linger` deadline, checked before the batch so a run with no traffic still ends.
        if self.script == EnterWorldScript::Entering && self.cfg.linger > 0.0 {
            if let Some(at) = self.playable_at {
                if self.timer.local_time - at >= self.cfg.linger {
                    tracing::info!("logging off");
                    link.net.session.log_off();
                    self.script = EnterWorldScript::LoggingOff;
                }
            }
        }

        for e in events {
            match e {
                SessionEvent::CharacterSet(set) => {
                    // The persistent-data object's character-set notice is the hinge between
                    // the network and the flow. The session decodes `0xF658` and the pre-game
                    // view owns the destination; the copy between them is
                    // `character_set_from_login`. Setting it here rather than in `build_host_state`
                    // is deliberate: the client's is a **notice**, and a notice arrives once.
                    self.host_state.character_set = Some(character_set_from_login(&set));
                    self.host_state.received_set = true;
                    // **A notice is a count, not a value.** ACE re-sends the *same* list six
                    // seconds after a log-off (all five recorded sessions: `0xF653` and `0xF658`
                    // in the same instant). An identical set is no value edge at all, so a shell
                    // that applied this on `host.character_set != last_host.character_set` would
                    // drop the second notice — and it is the second one that carries the player
                    // out of the world in retail. See
                    // `UiShell::apply_host_notices`.
                    self.host_state.character_set_notices =
                        self.host_state.character_set_notices.wrapping_add(1);
                    // The account's Throne of Destiny flag gates one heritage and one start area
                    // in the character-generation wizard.
                    self.host_state.account_has_tod = set.has_throne_of_destiny != 0;
                    self.hud.era.account_has_throne_of_destiny = self.host_state.account_has_tod;
                    tracing::info!(
                        "account {:?}, {} character(s): {}",
                        set.account,
                        set.characters.len(),
                        set.characters
                            .iter()
                            .map(|c| c.name.clone())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    // A front end whose pre-game drive ([`Shell::drive_pregame_screens`])
                    // presses its screens' own buttons does the rest itself. For every other one
                    // -- `--no-ui` included -- the runtime logs the character on here.
                    if scripted
                        && runtime_enters
                        && self.script == EnterWorldScript::AwaitingCharacterSet
                    {
                        // `-u`/`-user` names the character in the retail switch set and this build
                        // is the first thing to read it; without one, the first slot.
                        let pick = set
                            .characters
                            .iter()
                            .find(|c| !wanted.is_empty() && c.name.eq_ignore_ascii_case(&wanted))
                            .or_else(|| set.characters.first());
                        if let Some(c) = pick {
                            tracing::info!("entering the world as {:?}", c.name);
                            let account = set.account.clone();
                            // See `CharacterAction::LogOn`.
                            self.host_state.entered_character = Some(c.name.clone());
                            link.net.enter_world(c.gid, &account);
                            self.script = EnterWorldScript::Entering;
                        } else {
                            tracing::info!("the account has no characters");
                            self.script = EnterWorldScript::Done;
                        }
                    }
                }
                SessionEvent::WorldInfo {
                    name,
                    connections,
                    max_connections,
                } => {
                    tracing::info!("world {name:?}, {connections}/{max_connections} connections");
                    // The restore response flows through the character-generation response notice
                    // and updates the character row that the UI then reads back.
                    self.host_state.world_name = Some(name);
                }
                SessionEvent::CharacterScreenMessage(text) => {
                    tracing::info!(
                        "0xF65A character screen message, {} character(s)",
                        text.len()
                    );
                    self.host_state.character_screen_message = Some(text);
                }
                SessionEvent::EnterWorldReady => tracing::info!("0xF7DF server ready"),
                SessionEvent::CharGenResponse(r) => {
                    // The character-create response handler ends every arm by sending the
                    // character-generation verification notice.
                    //
                    // The successful case is **not** followed by a fresh `0xF658`. ACE sends
                    // `GameMessageCharacterList` on authenticate, on delete and on log-off only
                    // (`AuthenticationHandler.cs:258`, `CharacterHandler.cs:322`,
                    // `Session.cs:273`); a creation is answered by
                    // `GameMessageCharacterCreateResponse` and a restore by
                    // `GameMessageCharacterRestore`, both `0xF643` and both alone. The client
                    // rebuilds the row out of *this* message — see the `0xF643` arm in
                    // `dereth_client_net::client_session` — and the notice below is what closes the please-wait modal.
                    tracing::info!(
                        "0xF643 char-gen response {} for {:?} ({:#010X})",
                        r.response_type,
                        r.identity.name,
                        r.identity.gid.0
                    );
                    self.host_state.chargen_response = Some(r.response_type);
                    // A notice is a count: two refusals in a row carry the same code.
                    self.host_state.chargen_response_notices =
                        self.host_state.chargen_response_notices.wrapping_add(1);
                }
                SessionEvent::CharacterDeleted => {
                    tracing::info!("0xF655 the slot was deleted");
                }
                SessionEvent::Ddd(dereth_client_net::client_session::DddEvent::Interrogation(
                    interrogation,
                )) => {
                    // The interrogation handler builds a list from every open data-file
                    // controller and answers with `0xF7E6` on queue 5. **Without
                    // this the phase never ends**: ACE's `DDDHandler` answers the *response*, not
                    // the interrogation, so a client that stays quiet is left on the data-patch
                    // screen for ever and `DDD_EndDDD` never arrives.
                    // Interrogation handling first tests product-id mask `0x4` and loads the
                    // high-resolution dat only when it is set. Opening it whenever it exists would
                    // make every two-level surface texture draw the high-res art retail never
                    // shows on ACE (which sends `0x1` unless `allow_highres_dat`).
                    if interrogation.product_id & PRODUCT_HIGHRES != 0 {
                        match self.store.grant_highres() {
                            Ok(opened) => tracing::debug!(
                                "0xF7E5 product id {:#x} grants client_highres.dat: {}",
                                interrogation.product_id,
                                if opened { "opened" } else { "no such file" }
                            ),
                            Err(e) => tracing::warn!("client_highres.dat did not open: {e}"),
                        }
                        self.ddd.set_bases(&self.store);
                    }
                    let response = ddd_interrogation_response(
                        &self.store,
                        interrogation.product_id,
                        self.ddd.keeps_overlay(),
                    );
                    tracing::debug!(
                        "0xF7E6 answering with {} iteration list(s): {:?}",
                        response.iters_with_keys.len(),
                        response
                            .iters_with_keys
                            .iter()
                            .map(|l| (l.dat_file_type, l.dat_file_id, l.iterations.iterations))
                            .collect::<Vec<_>>()
                    );
                    link.net.session.answer_ddd_interrogation(&response);
                    // The DDD state becomes interrogation-received: records
                    // that arrive from here until `0xF7E7` are early saves, which the begin handler
                    // subtracts from the byte count it shows the player.
                    self.ddd.on_interrogation();
                    self.pending_ddd
                        .push(dereth_client_contract::pregame::DddEvent::PatchtimeInterrogation);
                    tracing::debug!("DDD Interrogation({interrogation:?})");
                }
                SessionEvent::Ddd(d) => {
                    // The DDD notifier fans the event out to its plugins, including
                    // the data-patch screen while it is up. The translation is the whole of
                    // it: the protocol's `DddEvent` is the wire message, the pre-game view's is
                    // what the screen shows, and the client's `DDDEvent` enum is the same seven
                    // values.
                    use dereth_client_contract::pregame::DddEvent as UiDdd;
                    // **The patch is applied here, before the screen is told.**
                    // The `0xF7E2` arm saves the downloaded record asynchronously and only *then*
                    // notifies the UI of its compressed size, so the byte count
                    // the player watches is the count of bytes that reached the dat.
                    let ui_event = match &d {
                        // The interrogation is handled above, because answering it is what makes
                        // the phase finish.
                        dereth_client_net::client_session::DddEvent::Interrogation(_) => None,
                        // "The begin handler with a non-zero remaining byte count". The subtraction
                        // is expected bytes minus early-save bytes, which the patcher owns because
                        // it is the thing that counted the early saves.
                        dereth_client_net::client_session::DddEvent::Begin(b) => {
                            let (expected, action) = self.ddd.on_begin(b);
                            if action.send_end {
                                // Begin-request completion: nothing to download, so
                                // the DDD state becomes end-sent and `0xF7EA` goes out now.
                                link.net.session.send_ddd_end();
                            }
                            (expected != 0).then_some(UiDdd::PatchtimeBegin { expected })
                        }
                        dereth_client_net::client_session::DddEvent::Data(m) => {
                            let (outcome, action) = self.ddd.on_data(m);
                            if action.send_end {
                                // Removing the last pending download sends the end request.
                                link.net.session.send_ddd_end();
                            }
                            tracing::debug!("DDD applied {outcome:?}");
                            // The reported compressed size is the payload size minus 4, the wire
                            // payload length whether or not the body was compressed.
                            Some(UiDdd::DataDownloaded {
                                bytes: m.data.len() as u64,
                            })
                        }
                        dereth_client_net::client_session::DddEvent::End => {
                            // End-of-DDD closes the books; the application's cache flush runs
                            // below, after the `link`
                            // borrow ends.
                            self.ddd_invalidation = Some(self.ddd.on_end());
                            Some(UiDdd::PatchtimeEnd)
                        }
                        dereth_client_net::client_session::DddEvent::PatchtimePending => {
                            Some(UiDdd::PatchtimePending { total: 0 })
                        }
                        // The overlay extension's manifest, ahead of the patch: the patcher
                        // checks it names this client's bases and a world it takes.
                        dereth_client_net::client_session::DddEvent::OverlayManifest(m) => {
                            self.ddd.on_manifest(m);
                            None
                        }
                        dereth_client_net::client_session::DddEvent::Error(e) => {
                            // The per-frame step's `0xF7E4` arm only marks the asynchronous get
                            // failed: the pending download stays pending, and no byte is written.
                            self.ddd.on_error(e);
                            None
                        }
                    };
                    if let Some(e) = ui_event {
                        // `DDD_PatchtimeEnd` also triggers the application's cache reload; what the
                        // *flow* needs from it is that the patch phase is over.
                        if e == UiDdd::PatchtimeEnd {
                            self.host_state.patch_finished = true;
                        }
                        self.pending_ddd.push(e);
                    }
                    tracing::debug!("DDD {d:?}");
                }
                SessionEvent::Dropped {
                    queue,
                    opcode,
                    reason,
                } => {
                    tracing::debug!("dropped {opcode:?} on {queue:?}: {reason:?}");
                }
                SessionEvent::PlayerCreated(id) => {
                    tracing::debug!("0xF746 player is {:#010X}", id.0);
                }
                SessionEvent::PlayerDescription(_) => {
                    tracing::info!("0x0013 player description -- in world");
                    // Player-description delivery raises the notice answered with
                    // loading the `"#auto"` screen layout. The field is used rather than
                    // [`Self::note_player_description`] because `link` is borrowed here; the
                    // method exists so the same decision is reachable from a test.
                    self.pending_auto_layout = true;
                }
                SessionEvent::StateChanged(SessionState::Playable) if scripted => {
                    // `--linger` holds the session in the world so the objects have time to move.
                    // Without it a scripted run logs off in the same frame `0x0013` arrives, which
                    // is the default.
                    self.playable_at = Some(self.timer.local_time);
                    if self.cfg.linger <= 0.0 {
                        tracing::info!("logging off");
                        link.net.session.log_off();
                        self.script = EnterWorldScript::LoggingOff;
                    } else {
                        tracing::info!("in world; staying {} s", self.cfg.linger);
                    }
                }
                SessionEvent::StateChanged(s) => tracing::info!("session {s:?}"),
                SessionEvent::LoggedOff => {
                    tracing::info!("logged off");
                    // **The loop does not end on every `0xF653`.**
                    //
                    // `EnterWorldScript::Done` is `App::frame`'s exit condition. Set with no guard,
                    // the *server's* `0xF653` — which in every one of the five recorded sessions
                    // arrives **six seconds** after the client asks — would end the main loop of a
                    // client that had just returned to character select, and the player could not
                    // enter the world again because there would be no process to do it with.
                    //
                    // A log-off to character select never ends the retail client:
                    // character-session teardown clears the player and leaves the
                    // loop running, and the only thing that stops is
                    // the device-done edge from the epilogue screen. So the loop ends here **only** when
                    // there is no screen to go back to (`--no-ui`) or when this run's own script
                    // asked for the log-off (`--enter-world --linger`), which are the two builds
                    // that have nothing to return to. This is exactly the shape of the
                    // `CharacterError` arm below, and for the same reason: ending the loop instead
                    // of showing a screen is indistinguishable from a crash from the player's seat.
                    if no_ui || self.script == EnterWorldScript::LoggingOff {
                        self.script = EnterWorldScript::Done;
                    }
                }
                SessionEvent::CharacterError(code) => {
                    tracing::warn!("character error {code}");
                    // Character-error delivery reaches the UI flow, which maps the code
                    // to a `StringInfo` in table `0x10000002` and
                    // queues disconnected mode `0x10000002` with that text. A code with no token is the
                    // switch's `default:` arm and **never queues a mode**.
                    //
                    // The notice itself is raised in [`Self::recv_disconnect_notice`], above and
                    // outside this loop. What is left here is the *script*, and it does not end the
                    // process when there is a UI: the client's own answer to a character error is
                    // a screen with the reason on it and a button, and ending the loop instead is
                    // indistinguishable from a crash from the player's seat. `--no-ui` ends the
                    // loop, because in that build there is no screen to show it on and nothing to
                    // take the answer. So does an `--enter-world` run, screen or not: the screen's
                    // one button only leads to the quit screen, and a script that will never press
                    // it would otherwise wait on it for ever (a second login a second after a
                    // log-off is refused as an account already logged on, code 1).
                    if no_ui || scripted {
                        self.script = EnterWorldScript::Done;
                    }
                }
                _ => {}
            }
        }
        // The `DDD_PatchtimeEnd` cache-invalidation arm is taken here
        // rather than in the loop because it needs `&mut self` and the loop holds `link`.
        self.invalidate_after_ddd();
    }

    /// Handle `DDD_PatchtimeEnd` by making caches forget what
    /// the patch changed.
    ///
    /// The client shuts down the language interface, releases the master property list, flushes
    /// each object cache, restarts the language interface, and refreshes the active region, in that
    /// order.
    ///
    /// Retail's free-object flush drops only objects nothing still holds, so **the client does not
    /// invalidate a referenced object either** — it drops the two singletons it knows are
    /// referenced (the language interface and the master property list) by hand and re-creates
    /// them.
    ///
    /// This build's equivalent is to reopen the store, with the world's overlay laid over it as the
    /// patch left it. Every `DatFile` caches its whole B-tree at open, so a reader that was up
    /// across the patch would read the files as they were. The reopened store is handed to
    /// everything that holds one through [`Self::adopt_store`], and the front ends read their
    /// files again, letting go of the pictures they made from the old records, when they see
    /// [`Self::store_generation`] move.
    ///
    /// The `0xF7EA` arm runs this once the event loop's borrow of the link has ended; a driver
    /// that patches through the patcher (`App::ddd`) itself ends the patch by setting
    /// [`Self::ddd_invalidation`] and calling it.
    pub fn invalidate_after_ddd(&mut self) {
        let Some(summary) = self.ddd_invalidation.take() else {
            return;
        };
        tracing::info!(
            "DDD finished -- {} applied, {} refused, {} stale, {} still pending, \
             {} file(s) changed",
            summary.applied,
            summary.refused,
            summary.stale,
            summary.still_pending,
            summary.changed.len()
        );
        if !summary.changed_anything() {
            // The overwhelmingly common case: the server said "you are up to date". Reopening
            // 1.4 GB of container for nothing is exactly the kind of cost a patch path should not
            // impose on a session that had no patch.
            return;
        }
        match self.reopen_store() {
            Ok(fresh) => {
                self.adopt_store(std::sync::Arc::new(fresh));
                tracing::info!(
                    "DDD reopened the dat files; {} changed",
                    summary
                        .changed
                        .iter()
                        .map(|t| t.file_name())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            // Reopening failed, so the old store stands. Stale is better than absent: the client
            // has a complete, self-consistent view of the files as they were.
            Err(e) => tracing::warn!(
                "DDD could not reopen the dat files ({}); the old view stands and a \
                 restart is needed to see the patch",
                e.cause
            ),
        }
    }

    /// The data files opened again after a patch, with the world's overlay over them: from
    /// [`crate::config::Config::dat_dir`], or, when the platform opened the files itself, over the
    /// files it opened ([`Self::bring_up_with_store`]), which a patch never changes.
    ///
    /// # Errors
    /// The files will not open.
    pub fn reopen_store(
        &self,
    ) -> Result<dereth_dat::RetailDatStore, crate::assets::DataFilesError> {
        match &self.base_store {
            Some(base) => Ok(crate::world_overlay::lay_over((**base).clone(), &self.cfg)),
            None => crate::assets::open_store(&self.cfg),
        }
    }

    /// Make `fresh`, the data files reopened after a patch, the store every reader of this
    /// application reads: the application's own handle, the animation assets the preview spaces
    /// and the portal space build from, the object stream's, and the bases the patcher writes
    /// against. Then count the reopen, so a front end that keeps what it read from the files (its
    /// pictures, its fonts, its own handle on them) reads them again ([`Self::store_generation`]).
    ///
    /// The world, once entered, keeps the handles it took at world entry; the one patch that
    /// reaches it in play, a landblock answering a run-time get, is handed to the land source by
    /// its caller.
    pub fn adopt_store(&mut self, fresh: std::sync::Arc<dereth_dat::RetailDatStore>) {
        self.anim_assets = std::sync::Arc::new(dereth_world_data::anim_assets::DatAnimAssets::new(
            std::sync::Arc::clone(&fresh),
        ));
        self.objects.set_store(std::sync::Arc::clone(&fresh));
        self.ddd.set_bases(&fresh);
        self.store = fresh;
        self.store_generation += 1;
    }

    /// How many times the data files have been reopened since start-up. See
    /// [`Self::adopt_store`].
    #[must_use]
    pub fn store_generation(&self) -> u64 {
        self.store_generation
    }

    /// The asynchronous cache-miss path's production caller.
    ///
    /// [`dereth_world_data::land_source::DatLandSource::build`] is the run-time cache miss: a landblock record
    /// the store does not carry, discovered while the streaming ring was building a block. This
    /// turns each one into the `0xF7E3` the client sends, under the client's own two conditions --
    /// a connection has to exist and one `QualifiedDataID` gets one
    /// outstanding request per `QualifiedDataID`.
    ///
    /// The return leg is the other half: a `0xF7E2` answering one of those gets goes through the
    /// ordinary patcher (the client's arm has no state gate either), and the record then has to
    /// reach the *land source's* handle on the store, which is an `Arc` it took at world entry.
    /// That is what [`dereth_world_data::land_source::DatLandSource::resupply`] is for.
    pub(super) fn request_missing_cell_records(&mut self) {
        let land = self
            .world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .map(|c| std::sync::Arc::clone(c.land()));
        let Some(land) = land else { return };
        let resupplied = self.ddd.take_resupplied();
        if !resupplied.is_empty() {
            // The patch is on disk; the land source is still reading through the handle it took
            // at world entry. `invalidate_after_ddd` replaced `App::store` for the *patch* phase;
            // a run-time answer arrives with no `0xF7EA` behind it, so the reopen happens here.
            match self.reopen_store() {
                Ok(fresh) => {
                    let fresh = std::sync::Arc::new(fresh);
                    self.adopt_store(std::sync::Arc::clone(&fresh));
                    land.resupply(fresh, &resupplied);
                    tracing::info!(
                        "0xF7E2 answered {} run-time get(s); the land source was reseeded",
                        resupplied.len()
                    );
                }
                Err(e) => tracing::warn!(
                    "a run-time DDD answer landed but the dat files would not reopen ({}); the old view stands",
                    e.cause
                ),
            }
        }
        // `0xF7E4` failed a get. The asynchronous-get failure path has already taken
        // it out of the pending-gets list; forgetting the cached `None` is what lets the streaming
        // ring ask for the block again rather than answering from the miss it remembers.
        let failed = self.ddd.take_failed_gets();
        if !failed.is_empty() {
            land.forget_blocks(&failed);
        }
        // A network cache lookup returns false without
        // a packet controller, so with no link the get simply fails and nothing is
        // queued for later. Draining the misses anyway would lose them; leaving them is what lets
        // the next connected frame ask.
        let Some(link) = self.link.as_mut() else {
            return;
        };
        let sent = crate::ddd::drain_cache_misses(&land, &mut self.ddd, &mut link.net.session);
        if sent > 0 {
            tracing::info!("0xF7E3 requested {sent} missing cell record(s) from the server");
        }
    }

    /// The `DddPatcher` this client patches through, for a test that wants to see what a session
    /// did with a patch.
    #[must_use]
    pub fn ddd(&self) -> &crate::ddd::DddPatcher {
        &self.ddd
    }

    /// Character-error and server-died notices, the two edges that reach the disconnected screen
    /// from anywhere; and account-booted and account-banned responses, the two that reach it
    /// without a notice.
    ///
    /// Both are notices rather than reads: the player system raises them and the flow queues
    /// disconnected mode `0x10000002` with the string info. What this function produces is its
    /// **symbolic id**; the front end's UI does the queueing (its host-state step carries
    /// the server-died notice's "unless the current mode is already `0x10000002`" guard) and
    /// the screen resolves the id against table enum `0x10000002`.
    ///
    /// `character_error_string_id` returning `None` is the switch's `default:` arm, which "leaves
    /// the `StringInfo` untouched and **never queues a mode**" — so a code with no token shows
    /// nothing at all, rather than an empty screen.
    ///
    /// The latch is first-notice-wins because there is no reconnect path: once the client is on
    /// the disconnected screen the only way off it is the OK button.
    ///
    /// **`0xF7DC` and `0xF7C1` are the two edges that carry their own sentence.**
    /// Account-booted and account-banned handling raises no character error and looks no token up:
    /// each formats a message from a built-in literal, wraps it with
    /// a literal string value, and queues disconnected mode `0x10000002`
    /// itself. So what those two arms put in `HostState::error` is the **text**, not a token, and
    /// the shell's resolve-or-pass-through is what shows it — which is why the shell's string
    /// resolver must not find a row for it.
    ///
    /// The literals and the tables are [`dereth_client_contract::disconnect`]'s, so every front end
    /// shows the same reason; the notice itself is `HostState::disconnect`, and its phase
    /// `GamePhase::Disconnected`.
    fn recv_disconnect_notice(&mut self, e: &dereth_client_net::client_session::SessionEvent) {
        use dereth_client_contract::disconnect as text;
        use dereth_client_contract::pregame::DisconnectNotice;
        use dereth_client_net::client_session::{DisconnectReason, SessionEvent, SessionState};
        let (notice, id) = match e {
            // Send the character-error notice for this code. A code with no token is the
            // switch's `default:` arm and never shows the screen.
            SessionEvent::CharacterError(code) => match text::character_error_string_id(*code) {
                Some(id) => (DisconnectNotice::CharacterError(*code), id.to_string()),
                None => return,
            },
            // The session independently decides both the 110-second world-entry timeout and the
            // 40-second heartbeat gap.
            SessionEvent::StateChanged(SessionState::Disconnected(
                DisconnectReason::ServerDied,
            )) => (
                DisconnectNotice::ServerDied,
                text::SERVER_DIED_STRING_ID.to_string(),
            ),
            // `None` is a `0xF7DC` with no body at all, which the handler cannot tell from an
            // empty reason; both take the default.
            SessionEvent::AccountBooted(reason) => (
                DisconnectNotice::Booted(reason.clone()),
                text::account_booted_message(reason.as_deref()),
            ),
            // The clock is an input because the handler reads real time at the moment the
            // message lands, and the zone is an input because it does `asctime(localtime(&t))`.
            // The zone is read for the **expiry** instant, not for now: a ban that ends after a
            // daylight change is announced in the zone it will end in.
            SessionEvent::AccountBanned { expiry, reason } => {
                let now = self.clock.unix_secs();
                let at = text::ban_expiry_epoch(*expiry, now);
                (
                    DisconnectNotice::Banned {
                        expiry: *expiry,
                        reason: reason.clone(),
                    },
                    text::account_banned_message(
                        *expiry,
                        reason,
                        now,
                        self.clock.utc_offset_secs(at),
                    ),
                )
            }
            _ => return,
        };
        if self.host_state.error.is_none() {
            tracing::info!("the disconnected screen, showing {id}");
            self.host_state.error = Some(id);
            self.host_state.disconnect = Some(notice);
        }
    }

    /// Complete one admitted message and its synchronous object-arrival callbacks before the
    /// owner asks Session for another raw entry. No second instance or UI timestamp admission.
    pub(super) fn deliver_session_events(
        &mut self,
        shell: &mut S,
        now: dereth_primitives::LocalTime,
    ) {
        let events = self
            .link
            .as_mut()
            .map(|link| link.net.drain_events())
            .unwrap_or_default();
        // The use-position-from-server test, which answers `autonomy_level != 2`.
        // The vector-update path asks it before applying a `0xF74E` **about the player**, and `ObjectStream`
        // has no command interpreter — in the client `WorldObjects` holds a pointer to one.
        // Pushed in here, before the drain that consumes it, so the answer a message is
        // measured against is the one the interpreter held when it arrived.
        self.objects
            .note_use_position_from_server(self.movement.lists.autonomy_level != 2);
        for event in events {
            let body = self.world.as_ref().and_then(|w| w.character.as_ref());
            self.interaction.prepare_ui_dispatch(
                body,
                &mut self.objects.world,
                self.present.size(),
            );
            let (geometry, radius) = self.current_selection_geometry();
            self.interaction.last_use_time = now;
            let inter = &mut self.interaction;
            let hud = &mut self.hud;
            let arrived =
                self.objects
                    .apply_event_with_dispatch(&event, now, &mut |world, notice| {
                        inter.dispatch_object_notice(world, notice.clone(), &geometry, radius, now);
                        object_panel_notice(shell, hud, inter, world, &notice);
                    });
            // A public description update forces a synchronous object-description Control enqueue. Preserve that
            // boundary: it reaches Session before queued notice delivery, released arrivals, or
            // the next admitted object message. Packet serialization retains the packet controller's
            // frame slot and may therefore happen on the next frame.
            self.objects
                .drain_pending_requests(self.link.as_mut().map(|link| &mut link.net.session));
            // Old lifetime queues are retired here, before publishing the new instance and
            // before any callback can admit a subsequent object-scoped packet.
            self.objects
                .retire_session_instances(self.link.as_mut().map(|l| &mut l.net.session));
            self.deliver_object_notices();
            // `Admin_Environs` is a player-system local handler, not a Hud or interaction
            // notice. Consume only its accepted visual and sound cases here; unknown values remain
            // visible to the generic unreceived-opcode diagnostics.
            let admin_environs = self.apply_admin_environs(shell, &event);
            let events = if admin_environs {
                &[][..]
            } else {
                std::slice::from_ref(&event)
            };
            // `Hud::now` is stamped here, not only once a frame by `Hud::drive` — which runs on
            // the **gameplay screen**, so a message arriving during the login transition would be
            // measured against `0.0`.
            // `0x0013 Login_PlayerDescription` is exactly such a message and it carries the
            // enchantment registry, whose `_start_time` is relative to receipt. Stamped here, from
            // the same clock `ObjectStream::apply_event` and `Interaction::last_use_time` get, so
            // that every handler in the batch reads the instant the packet was processed — which
            // is what reads in retail.
            self.hud.now = now;
            let _ = self.apply_hud_events(shell, events);
            self.interaction.last_use_time = now;
            let (geometry, radius) = self.current_selection_geometry();
            let hud = &mut self.hud;
            crate::interaction::apply_events_at_boundary(
                &mut self.interaction,
                events,
                &mut self.objects.world,
                Some((&geometry, radius)),
                &mut |inter, world, notice| object_panel_notice(shell, hud, inter, world, notice),
            );
            // Defender handlers stamp/AutoTarget first; their synchronously raised selection
            // notices then reenter Combat before another admitted UI packet can change state.
            let body = self.world.as_ref().and_then(|w| w.character.as_ref());
            self.interaction
                .dispatch_ui_selection_notices(body, &mut self.objects, now);
            self.teleport.apply_events(events);
            // Accepted local movement/control-transfer tails precede the next accepted message.
            self.player_teleport_use_time(shell);
            if matches!(
                event,
                dereth_client_net::client_session::SessionEvent::WorldObject { .. }
                    | dereth_client_net::client_session::SessionEvent::PlayerCreated(_)
            ) {
                // `sync_objects` performs no physics or animation clock advance. Drain remote
                // movement and physical/state projections now, so two accepted commands cannot
                // overwrite Presence.pending_movement before the first callback runs. The local
                // journal above has already removed its matching snapshot to avoid double apply.
                self.sync_objects();
            }
            self.process_logon_event_queue(shell, vec![event]);
            for (id, instance) in arrived {
                let Some(link) = self.link.as_mut() else {
                    continue;
                };
                if self.objects.world.weenie(id).is_some() {
                    link.net.session.begin_weenie_arrival(id);
                }
                // Weenie creation and the physical null-object setup have independent success. A
                // surviving Weenie remains known to subsequent UI input, without publishing a body.
                if self.objects.world.physics(id).is_none() {
                    continue;
                }
                link.net.session.begin_object_arrival(id, instance);
                // Object-net-blob processing updates the object first. Callback
                // deletion can remove this ordering window, so ask it for only one entry.
                while self
                    .link
                    .as_mut()
                    .is_some_and(|link| link.net.session.process_next_object_ui(id, now))
                {
                    self.deliver_session_events(shell, now);
                }
                // Snapshot only after UI callbacks finish; each physical blob is admitted
                // against the then-current object table, and re-parks if still not ready.
                let blobs = self
                    .link
                    .as_mut()
                    .map(|link| link.net.session.take_object_message(id))
                    .unwrap_or_default();
                for blob in blobs {
                    if let Some(link) = self.link.as_mut() {
                        link.net.session.process_object_message(&blob, now);
                    }
                    self.deliver_session_events(shell, now);
                }
            }
        }
    }
}

/// `CharGenResultData` → `dereth_protocol::login::CharGenResult`, field for field.
///
/// The UI crate may not depend on `dereth-protocol` and the protocol may not depend on the UI; the
/// copy between them is the application's, exactly as [`character_set_from_login`] is in the other
/// direction. `version` is the client's hard-coded 1 and the checksum is filled in by the caller.
#[must_use]
pub fn chargen_result_to_wire(
    r: &dereth_client_contract::pregame::CharGenResultData,
) -> dereth_protocol::login::CharGenResult {
    dereth_protocol::login::CharGenResult {
        // "Hard-coded 1 by the client"; ACE's `CharacterCreateInfo::Unpack` skips it as
        // "Unknown constant (1)".
        version: 1,
        heritage_group: r.heritage_group,
        gender: r.gender,
        eyes_strip: r.eyes_strip,
        nose_strip: r.nose_strip,
        mouth_strip: r.mouth_strip,
        hair_color: r.hair_color,
        eye_color: r.eye_color,
        hair_style: r.hair_style,
        headgear_style: r.headgear_style,
        headgear_color: r.headgear_color,
        shirt_style: r.shirt_style,
        shirt_color: r.shirt_color,
        trousers_style: r.trousers_style,
        trousers_color: r.trousers_color,
        footwear_style: r.footwear_style,
        footwear_color: r.footwear_color,
        skin_shade: r.skin_shade,
        hair_shade: r.hair_shade,
        headgear_shade: r.headgear_shade,
        shirt_shade: r.shirt_shade,
        trousers_shade: r.trousers_shade,
        footwear_shade: r.footwear_shade,
        template_num: r.template_num,
        strength: r.strength,
        endurance: r.endurance,
        coordination: r.coordination,
        quickness: r.quickness,
        focus: r.focus,
        self_: r.self_,
        slot: r.slot,
        class_id: r.class_id,
        skill_advancement_classes: r.skill_advancement_classes.clone(),
        name: r.name.clone(),
        start_area: r.start_area,
        is_admin: r.is_admin,
        is_envoy: r.is_envoy,
        checksum_value: 0,
    }
}

/// `0xF658 Login_LoginCharacterSet` → the persistent-data object's character set.
///
/// The session decodes the message and the pre-game view owns the destination; the conversion
/// between them is the application's. It is a field-for-field copy and nothing more.
#[must_use]
pub fn character_set_from_login(
    set: &dereth_protocol::login::LoginCharacterSet,
) -> dereth_client_contract::persist::persistent_data::CharacterSet {
    let one = |c: &dereth_protocol::login::CharacterIdentity| {
        dereth_client_contract::persist::persistent_data::CharacterIdentity {
            id: c.gid,
            name: c.name.clone(),
            seconds_grace_period: c.seconds_greyed_out,
        }
    };
    dereth_client_contract::persist::persistent_data::CharacterSet {
        set: set.characters.iter().map(one).collect(),
        del_set: set.deleted.iter().map(one).collect(),
        status: set.status,
        // The client's own default is 5 and the server overwrites it; a negative count is not a
        // state the message can legally carry, and clamping is what the client's unsigned
        // allowed-characters field does with one.
        num_allowed_characters: u32::try_from(set.num_allowed_characters).unwrap_or(0),
        account: set.account.clone(),
        // `0xF658` carries `has_throne_of_destiny` and nothing about Dark Majesty or a pre-order;
        // the other two flags are set from elsewhere in the client and are left at their defaults
        // rather than guessed at.
        is_dark_majesty: false,
        is_throne_of_destiny: set.has_throne_of_destiny != 0,
        pre_ordered_throne_of_destiny: false,
    }
}

/// Answer server interrogation with one iteration list per open DAT file.
///
/// ```text
/// iteration lists = []
/// ok = true
/// for each open data file:
///     if it is initialized:
///         append its type, then its file id
///         ok = load its mostly-consecutive integer set and ok
/// if (ok) adopt and deliver the interrogation response;                // 0xF7E6 on queue 5
/// ```
///
/// The **type dword comes first**, then the id: type 0 portal, 1 cell and language, `"HiFi"` as an
/// int for the high-res dat; id 1 portal and highres, 2 cell, 3 language. The id ordering was
/// cross-checked against the server's `switch (entry.DatFileId)`.
///
/// **The set is re-emitted from the dat's own bytes rather than re-encoded.** In the original client,
/// every initialized file contributes its load result to the final success condition and the response
/// copies that loaded payload. This Rust function instead skips an unreadable set while still
/// avoiding re-encoding ambiguity. The dat reader in `dereth_dat::iteration` expands a negative `-k`
/// by reading a **following `first` dword**; `dereth_protocol`'s (and ACE's) reader treats it as a run of `k - 1` with no operand.
/// Both consume the retail portal list (`count 2072`, then `-2072, 1`) to exactly the same end, and
/// which is right in general is a question this function does not need to answer to send the
/// right bytes.
///
/// **Known limitation:** the two serializers for the
/// mostly-consecutive integer set disagree for any list that is not one run, and no shipped dat exercises the
/// difference.
/// A product-id bit test loads the high-resolution dat when mask `0x4` is set.
/// `dereth_protocol::admin::DddInterrogation::PRODUCT_HIGHRES` is the same bit on the wire side.
pub const PRODUCT_HIGHRES: u32 = 4;

///
/// `keeps_overlay` is whether the client keeps the world's records in an overlay this run
/// ([`crate::ddd::DddPatcher::keeps_overlay`]): only then does the answer carry the overlay
/// extension.
#[must_use]
pub fn ddd_interrogation_response(
    store: &dereth_dat::RetailDatStore,
    product_id: u32,
    keeps_overlay: bool,
) -> dereth_protocol::admin::DddInterrogationResponse {
    use dereth_protocol::admin::{
        DddInterrogationResponse, MostlyConsecutiveIntSet, TaggedIterationList,
    };

    /// `"HiFi"` as a little-endian int, which is `DDDManager.HiFi_String_As_Int`.
    const HIFI: u32 = u32::from_le_bytes(*b"HiFi");

    /// The file's `0xFFFF0001` payload as `(count, the run-length dwords that follow)`.
    ///
    /// A file from before Throne of Destiny has no such record; its iteration is in its header,
    /// and it answers as one run of that many (`count n`, then `-n, 1`), the shape a file patched
    /// from its first iteration to its last has.
    fn set_of(f: &dereth_dat::DatFile) -> Option<MostlyConsecutiveIntSet> {
        if let Some(n) = f.header_iteration() {
            let n = i32::try_from(n).ok()?;
            return Some(MostlyConsecutiveIntSet {
                iterations: n,
                ints: vec![-n, 1],
            });
        }
        let raw = f.read(dereth_dat::divine::ITERATION_LIST).ok()?;
        let (head, rest) = raw.split_at_checked(4)?;
        let iterations = i32::from_le_bytes([head[0], head[1], head[2], head[3]]);
        let ints = rest
            .as_chunks::<4>()
            .0
            .iter()
            .copied()
            .map(i32::from_le_bytes)
            .collect();
        Some(MostlyConsecutiveIntSet { iterations, ints })
    }

    let mut iters_with_keys = Vec::new();
    let mut push = |ty: u32, id: u32, f: &dereth_dat::DatFile| {
        if let Some(iterations) = set_of(f) {
            iters_with_keys.push(TaggedIterationList {
                dat_file_type: ty,
                dat_file_id: id,
                iterations,
            });
        }
    };
    push(0, 1, store.portal());
    push(1, 2, store.cell());
    push(1, 3, store.local());
    if product_id & PRODUCT_HIGHRES != 0 {
        if let Some(hi) = store.highres() {
            push(HIFI, 1, hi);
        }
    }
    // Not retail: the overlay extension. The flag says the client keeps the world's records in an
    // overlay over its locked files, and the bases it holds follow; a server that does not know the
    // extension reads none of it (see `dereth_protocol::admin::DddInterrogationResponse`). A run
    // that keeps no overlay answers as retail does, with neither.
    let mut overlay_bases = Vec::new();
    let mut base = |ty: u32, id: u32, f: &dereth_dat::DatFile| {
        overlay_bases.push(dereth_protocol::admin::OverlayBase {
            dat_file_type: ty,
            dat_file_id: id,
            fingerprint: dereth_dat::overlay::fingerprint(f),
        });
    };
    if keeps_overlay {
        base(0, 1, store.portal());
        base(1, 2, store.cell());
        base(1, 3, store.local());
        if product_id & PRODUCT_HIGHRES != 0 {
            if let Some(hi) = store.highres() {
                base(HIFI, 1, hi);
            }
        }
    }
    DddInterrogationResponse {
        // Read the local language. 1 is English, which is the only `client_local_*.dat` this
        // install has; `-language` does not reach this yet.
        client_language: 1,
        iters_with_keys,
        // "a second `CAllIterationList`, always empty in practice" -- and ACE does not read it.
        iters_without_keys: Vec::new(),
        flags: if keeps_overlay {
            DddInterrogationResponse::FLAG_OVERLAY
        } else {
            0
        },
        overlay_bases,
    }
}
