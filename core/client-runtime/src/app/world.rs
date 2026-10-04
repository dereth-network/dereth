//! World loading, object synchronization, and reset.

use super::*;

impl<S: Shell> App<S> {
    pub(super) fn step_incoming_world_objects(&mut self, shell: &mut S) {
        let now = dereth_primitives::LocalTime(self.timer.cur_time);
        while self
            .link
            .as_mut()
            .is_some_and(|link| link.net.session.process_next_world_object(now))
        {
            self.deliver_session_events(shell, now);
        }
        // The existing socket-free ObjectStream component API can have accepted a journal
        // directly. Consume it here, never before physics and never twice in the phased path.
        //
        // **Unconditional, and that is the point.** Smart-box use time reaches its input-queue
        // drain every frame after the two cell-manager arms rejoin. The guard tests the persistent
        // list object, not whether it contains an entry, so an empty list still enters the loop,
        // observes a null head, and breaks. Its tail is likewise unconditional. Retail has no
        // "is there anything queued" test in front of this
        // phase, and neither may this build: `player_teleport_use_time`'s counter is incremented
        // before each of its own early returns precisely so that "the step was skipped" and "the
        // step ran with nothing to do" stay distinguishable, and hoisting the function's own
        // `has_player_motion_dispatches` predicate into an outer `if` collapses those two states
        // back together. The teleport and stance-stop tests both pin one call per frame here,
        // before `position_use_time`.
        self.player_teleport_use_time(shell);
        // Newly created/retired scene projections settle before the world draw, not before physics.
        self.sync_objects();
    }

    /// Build the landscape around the player once the server has said where the player is.
    ///
    /// This is the application's stand-in for telling the landscape where to
    /// load. Landblock *streaming* — re-scrolling the window as the player walks — is not done
    /// here: the window is built once, around the block the player
    /// entered in, and a player who walks out of it walks off the drawn world.
    pub(super) fn load_pending_scene(&mut self) {
        let Some(cfg) = self.pending_scene else {
            return;
        };
        let Some(player) = self.objects.player() else {
            return;
        };
        let Some(pos) = self.objects.presence(player).and_then(|p| p.position) else {
            return;
        };
        let block = pos.cell.landblock();
        let landblock = (u16::from(block.x()) << 8) | u16::from(block.y());
        // The world and the body are built with the options as they are now, not as they were
        // at start-up: a second login keeps what the player changed on either interface's page.
        let cfg = crate::scene::SceneConfig {
            landblock,
            ..with_stored_options(cfg)
        };
        tracing::info!(
            "entering the world at landblock 0x{landblock:04X}, cell {:#010X}",
            pos.cell.0
        );
        self.pending_scene = None;
        let _load = tracing::debug_span!("load_world", landblock, cell = pos.cell.0).entered();
        if let Err(e) = self.present.load_world(&self.store, cfg, &mut self.world) {
            tracing::error!("the landscape would not load: {e}");
            return;
        }
        if let Some(mut world) = self.present.scene_mut(self.world.as_mut()) {
            world.set_environment_override_state(self.environment_override.clone());
        }
        // Stand the body where the server says the player is, so the chase camera looks at the
        // part of the world the server is talking about.
        if let Some(mut world) = self.present.scene_mut(self.world.as_mut()) {
            if let Some(c) = world.world_mut().character.as_mut() {
                c.teleport(pos);
            }
            world.world_mut().follow_character_now();
        }
        self.report_scene(landblock);
    }

    /// This frame's share of the object identity verdicts ([`Self::object_identity`]): a few
    /// milliseconds, more while the objects wait to take a look the player asked for or no world
    /// is drawn, and the verdicts handed to the presentation the frame they are ready. A store reopened after a
    /// patch starts them again for the new files.
    fn prepare_object_identity(&mut self) {
        let Some(prep) = self.object_identity.as_mut() else {
            return;
        };
        if !std::sync::Arc::ptr_eq(&prep.store, &self.store) {
            let (budget, cache) = (prep.budget, self.cfg.scene_config().object_identity_cache);
            self.object_identity = ObjectIdentityPrep::start(&self.store, budget, cache);
            return;
        }
        if prep.offered {
            return;
        }
        // More of each frame while the objects wait for it, and while no world is drawn (the
        // login and character screens, whose frames are light), so it is ready by world entry.
        let budget = if prep.awaited || self.world.is_none() {
            prep.budget.max(IDENTITY_WAITING_BUDGET)
        } else {
            prep.budget
        };
        let t = web_time::Instant::now();
        let ready = prep
            .build
            .step(crate::object_identity::Budget::Time(budget));
        prep.longest_step = prep.longest_step.max(t.elapsed());
        if let Some(id) = ready {
            let source = prep.build.source().unwrap_or_default();
            tracing::info!(
                "object identity: {} ids and {} rooms of the other era stand for the world's, \
                 {} ({} of the files hashed whole), ready in {:.2} s over {} frames ({} units \
                 of work, longest frame's share {:.1} ms)",
                id.len(),
                id.rooms_len(),
                if source.from_cache {
                    "read from the cache"
                } else {
                    "worked out"
                },
                source.files_hashed,
                prep.started.elapsed().as_secs_f64(),
                prep.build.steps(),
                prep.build.units(),
                prep.longest_step.as_secs_f64() * 1000.0
            );
            prep.offered = true;
            self.present.offer_object_identity(id);
        }
    }

    /// Whether the objects wait on the verdicts to take the look asked for, which gives the
    /// verdicts more of each frame; the player is told once.
    fn note_object_look_waiting(&mut self, w: &crate::frame_events::RenderPrefWork) {
        let Some(prep) = self.object_identity.as_mut() else {
            return;
        };
        prep.awaited = w.objects_waiting;
        if w.objects_waiting && !prep.noticed {
            prep.noticed = true;
            let text = "The objects' look is still being prepared; it is drawn as soon as it is \
                        ready.";
            tracing::info!("{text}");
            self.objects.world.scroll.add_feedback_to_scroll(
                text,
                dereth_client_model::scroll::LOCAL_ERROR_TYPE,
                true,
                0,
                dereth_client_contract::feedback::Feedback::LOCAL,
            );
        }
    }

    /// Build the landblocks the window scrolled onto this frame.
    ///
    /// A failure here is not fatal: the block stays undrawn and the next scroll will ask for it
    /// again. It is counted and logged rather than swallowed, because a device that cannot make a
    /// texture will not recover on its own and a silent hole in the world is the worst kind of
    /// gap.
    pub(super) fn stream_world(&mut self) {
        let _stream = tracing::trace_span!("stream_world").entered();
        // Before the preference poll, so a look waiting on the verdicts is drawn in the frame
        // they arrive.
        self.prepare_object_identity();
        let store = std::sync::Arc::clone(&self.store);
        // The client updates rendering preferences every frame. It sits **here**, at the top
        // of the frame's streaming step, because the work a changed preference asks for is the
        // same work a landblock scroll asks for -- release, queue, build -- and that work needs
        // the device and must stay outside the frame bracket.
        match self
            .present
            .update_render_preferences(&store, self.world.as_mut())
        {
            Ok(w) => {
                // The poll itself, whether or not it moved anything: this event's payload is
                // what `last_render_pref_work` reads.
                self.events.push(FrameEvent::RenderPreferencesPolled(w));
                self.report_landscape_refusals(&w);
                self.note_object_look_waiting(&w);
                if w.flushed
                    || w.mid_radius_changed
                    || w.detail_texturing_changed
                    || w.ground_changed
                    || w.sky_changed
                    || w.objects_changed
                {
                    self.events.push(FrameEvent::RenderPreferencesApplied);
                    tracing::info!(
                        "render preferences changed -- flush {}, mid_radius {}, \
                         detail textures {}, ground {}, sky {}, objects {}, \
                         {} block(s) queued, {} resident",
                        w.flushed,
                        w.mid_radius_changed,
                        w.detail_texturing_changed,
                        w.ground_changed,
                        w.sky_changed,
                        w.objects_changed,
                        w.blocks_queued,
                        w.blocks_rebuilt,
                    );
                }
            }
            Err(e) => {
                self.events
                    .push(FrameEvent::StreamFailed(StreamStage::RenderPreferences));
                tracing::warn!("applying a render preference failed: {e}");
            }
        }
        if let Err(e) = self.present.stream_world(&store, self.world.as_mut()) {
            self.events
                .push(FrameEvent::StreamFailed(StreamStage::Landblocks));
            tracing::warn!("landblock streaming failed: {e}");
        }
        let Some(world) = self.present.scene(self.world.as_ref()) else {
            return;
        };
        // Crossing a building's threshold changes the viewer's cell from a land cell to
        // an env cell, which is what switches the draw and the collision. Logged because it is the
        // one state change a person walking around cannot otherwise see.
        //
        // Keyed on everything the line prints, not on the cell alone. `inside`
        // streams and is asked of the *camera's* cell rather than the body's — see
        // [`ViewerCellReport`].
        let report = world
            .viewer_cell_id()
            .map(|c| ViewerCellReport::new(c, world.interior_batches()));
        if report != self.last_viewer_cell {
            self.last_viewer_cell = report;
            if let Some(r) = report {
                tracing::debug!(
                    "viewer cell {:#010X} ({}), {} interior batch(es)",
                    r.cell.0,
                    if r.outdoors { "outdoors" } else { "indoors" },
                    r.inside
                );
            }
        }
        let Some(block) = world.viewer_block() else {
            return;
        };
        // A window streaming in over several frames changes the census on every one of them;
        // report the block once it has settled rather than once per frame.
        if world.blocks_pending() > 0 {
            return;
        }
        // Keyed on everything the line prints: the six counters below stream in
        // asynchronously under a block that is not moving. See [`ViewerBlockReport`].
        let report = ViewerBlockReport::new(block, &world.census());
        if self.last_viewer_block != Some(report) && self.viewer_block_gate.ready() {
            self.last_viewer_block = Some(report);
            tracing::debug!(
                "landblock 0x{:02X}{:02X}, {} blocks resident, {} terrain surfaces, \
                 {} scenery + {} buildings + {} statics, {} triangles",
                report.block.0 & 0xFF,
                report.block.1 & 0xFF,
                report.blocks_meshed,
                report.terrain_surfaces,
                report.scenery_objects,
                report.buildings,
                report.static_objects,
                report.object_triangles
            );
        }
    }

    /// Give every new object geometry and every moved object its new frame.
    pub(super) fn sync_objects(&mut self) {
        let store = std::sync::Arc::clone(&self.store);
        if let Err(e) = self
            .present
            .sync_objects(&store, &mut self.objects, self.world.as_mut())
        {
            tracing::warn!("object sync failed: {e}");
        }
        // Step 4 both draws the object and
        // puts it in the world: the line above is the first half and this is the second. The
        // physics world is `Character`'s, so an object is solid exactly when there is a body for it
        // to be solid to. See [`crate::object_physics`].
        if let Some(c) = self.world.as_mut().and_then(|w| w.character.as_mut()) {
            // This must come first: the local body takes the id `0xF746` gave before
            // any of the server's objects are registered, so the physics world can
            // never be asked to hold two bodies under one id.
            c.adopt_server_id(self.objects.player());
            self.objects.sync_physics_at(
                &store,
                &mut c.world,
                dereth_primitives::LocalTime(self.timer.cur_time),
            );
        }
        // The gate is everything the line prints, with the two message-rate
        // counters entering by magnitude so the census cannot go quiet *or* flood; the raw values
        // are what is printed. See [`ObjectReport`].
        let n = self
            .present
            .scene(self.world.as_ref())
            .map_or(0, |w| w.server_object_count());
        let w = self
            .present
            .scene(self.world.as_ref())
            .map_or_else(Default::default, |w| w.census());
        let report = ObjectReport::new(n, &w, &self.objects.stats);
        let key = report.key();
        if self.last_object_report != Some(key) && self.object_report_gate.ready() {
            self.last_object_report = Some(key);
            tracing::debug!(
                "{} object(s) drawn ({} animated, {} held, {} setups, \
                 {} triangles) -- {} created, {} merged, {} recreated, {} removed, {} moved, \
                 {} motions, {} parent events, {} out of a container",
                report.drawn,
                report.animated,
                report.held,
                report.setups,
                report.triangles,
                report.creates,
                report.merges,
                report.recreates,
                report.removes,
                report.position_updates,
                report.movement_updates,
                report.parent_events,
                report.container_exits_offered
            );
        }
    }

    /// Build the landscape now rather than at startup. `--connect` uses this.
    pub fn defer_static_scene(&mut self, scene: crate::scene::SceneConfig) {
        // Remembered as well as armed, so [`App::reset_world_view`] can arm it
        // again on the next world entry.
        self.scene_config = Some(scene);
        self.pending_scene = Some(scene);
    }

    /// The phase-two smart-box reset — the world teardown.
    ///
    /// Raised by [`dereth_client_net::client_session::SessionEvent::WorldReset`], which
    /// login **phase 2** issues immediately before the enter-world request.
    /// So it runs on the *enter-world* edge, strictly before the new
    /// session's `0xF746`, its objects or its `0x0013` can arrive — which is what lets it be
    /// unconditional. Keyed on object lifetime instead, this reset would eat the session it is
    /// supposed to be preparing.
    ///
    /// The retail cascade and where each half of it lands here:
    ///
    /// | retail | here |
    /// |---|---|
    /// | release landscape blocks and flush cells | `Renderer::release_world` |
    /// | destroy objects, leaving the world for each | `ObjectStream::reset`, on the same event |
    /// | clear the physics player and smart-box player | the body is inside the scene, so it goes with it |
    /// | destroy queued network blobs | `Session`'s parked lists, already cleared |
    ///
    /// **Object destruction walks the player too** — its loop takes every
    /// entry of the object hash and calls `exit_world`, `leave_world`, `unset_parent`,
    /// `unparent_children` and the virtual destructor with no test for the player at all. This
    /// build splits the player's body (`WorldScene::character`) from the object tables
    /// (`ObjectStream`), and **retail has no such split**; releasing the scene is what retires the
    /// body here, and that is the only reason the two halves stay in step.
    pub(super) fn reset_world_view(&mut self) {
        let released = self.present.release_world(&mut self.world);
        self.events.push(FrameEvent::WorldReset {
            textures_released: released,
        });
        // The landscape is armed again rather than rebuilt now: the block to build it around is
        // the one the *server* puts the player in, and nothing knows that until the new session's
        // `0xF745` arrives. `load_pending_scene`'s own guards hold it until then, and it is that
        // function that stands the body where the server says, so it must run again.
        self.pending_scene = self.scene_config;
        self.last_viewer_block = None;
        self.last_viewer_cell = None;
        self.last_object_report = None;
        tracing::debug!(
            "(1) -- world torn down, {released} texture slot(s) \
             released, landscape re-armed: {}",
            self.pending_scene.is_some()
        );
    }

    /// Login phase 2's communication clears. They bracket the smart-box reset and
    /// enter-world request: clear the squelch database first, then set talk focus back to `All`
    /// after requesting entry. The following combat-mode reset is discussed below.
    ///
    /// All three run before anything of the new session can arrive — the enter-world send is the
    /// `0xF657` that *asks* for it — so they are taken on the one event raised at `phase_two`
    /// rather than at three separate positions in it. Nothing can interleave between them, which is
    /// what makes that collapse an ordering that cannot be observed rather than one that is
    /// guessed.
    ///
    /// # Why this is needed at all, given `ObjectStream::reset` already replaces the `World`
    ///
    /// It is needed for the **hole**, not for the common case, and the common case is already
    /// right: `ObjectStream::reset` ends `self.world = ()`, and the squelch DB, the talk
    /// focus and `CombatState::combat_mode` all live on that `World` — so every *ending* this
    /// client sees already clears them.
    ///
    /// The hole is that function's first two lines:
    ///
    /// ```text
    /// if self.presences.is_empty() && self.world.player.is_none() { return; }
    /// ```
    ///
    /// After an ending has run, both are true — so the `WorldReset` arm is a **no-op
    /// on the entry edge**, and anything written into the `World` between the ending and the next
    /// entry survives into the next session. `0x01F4 Communication_SetSquelchDB` is a *UI-queue*
    /// message and `Hud::ui_event`'s arm for it has no player gate at all, so it is exactly such a
    /// write. Retail has no hole to close because its squelch-database clear is unconditional.
    ///
    /// `set_combat_mode(NONCOMBAT_COMBAT_MODE, true)` is **deliberately not called
    /// here**, and it is not an omission:
    ///
    /// * the combat-mode setter's first statement is `if mode == combat_mode { return
    ///   Ok(()) }` — the client's early return — and `CombatState::combat_mode` is already
    ///   `CombatMode::NonCombat` from world construction, so the call would do nothing;
    /// * it cannot be reached through the hole above the way the squelch can. The only writer that
    ///   is not a `World` replacement is `Hud::apply_quality_update`, and that goes through
    ///   the player lookup, which returns `None` when there is no player — so
    ///   between an ending and the next entry there is nothing that can set it;
    /// * and with `send_to_server == true` it is the one of the three that would put a `0x0053` on
    ///   the wire. A reset that sends a datagram has to be earning it.
    pub(super) fn log_on_character_communication_clears(&mut self) {
        // See [`dereth_client_model::chat::ChatState::clear_squelch_db`].
        self.objects.world.chat.clear_squelch_db();
        // `TalkFocus::All` is the Say channel: a player who left the last session talking on
        // Allegiance must not still be
        // talking on Allegiance when a different character logs in.
        self.objects
            .world
            .chat
            .set_talk_focus(dereth_client_model::chat::TalkFocus::All);
    }

    /// Clear movement commands on the next session edge.
    ///
    /// Without this the three command lists are never emptied on a session edge, so the run lock
    /// survives a logout: auto-run, log out, log in, and the new character is **already
    /// running**, because `apply_current_movement` opens with
    /// `if (auto_run) move_player(0x45000005 /* WalkForward */)` and stays there for as long as the
    /// flag is set. The body walks off the placement the shard just gave it and
    /// [`App::position_use_time`] reports that it did, so item drops land in the wrong place on a
    /// second login.
    ///
    /// The client's disable step clears all command lists, releases hold-run and hold-sidestep,
    /// conditionally applies and sends the resulting movement while the player remains autonomous,
    /// and finally clears the interpreter's enabled flag. This entry-edge helper performs the
    /// command and hold cleanup that prevents the next character inheriting the old run state.
    ///
    /// The broader next-entry cleanup protects an unclean ending too, but it deliberately does
    /// not change `enabled`: the client has no reset-to-disable edge. Disable happens on the
    /// retail-proven `request_log_off` edge and enable on the accepted `0xF746` edge. The client's
    /// disable itself does **not** clear `auto_run`, so this entry-edge cleanup is not
    /// attributed to the scalar store above either.
    pub(super) fn command_interpreter_disable(&mut self) {
        // `clear_all_commands`, `set_auto_run(false)` and the projection, all three of which
        // `MovementCommands::clear_all_commands` already is.
        self.movement.clear_all_commands(&mut self.char_input);
        // Release the physical hold-run input and recompute its effective value against the UI
        // toggle, just like every other `set_hold_run` caller.
        self.char_input.run = self
            .movement
            .lists
            .set_hold_run(false, self.movement.ui_toggles_run);
        self.movement.lists.hold_sidestep = false;
        // The entry-edge cleanup intentionally also clears the run lock through
        // `clear_all_commands`; unlike `request_log_off`, this edge does not disable the interpreter.
    }

    /// The startup line for a scene, shared by the immediate and the deferred path.
    fn report_scene(&mut self, landblock: u16) {
        let Some(world) = self.present.scene(self.world.as_ref()) else {
            return;
        };
        let s = world.census();
        tracing::info!(
            "landblock 0x{landblock:04X}, {} blocks, {} terrain surfaces, \
             {} scenery + {} buildings + {} statics",
            s.blocks_meshed,
            s.terrain_surfaces,
            s.scenery_objects,
            s.buildings,
            s.static_objects
        );
    }

    pub(super) fn deliver_object_notices(&mut self) {
        self.interaction.last_use_time = dereth_primitives::LocalTime(self.timer.cur_time);
        let notices = self.objects.take_notices();
        self.interaction
            .apply_object_notices(&mut self.objects.world, notices);
    }
}
