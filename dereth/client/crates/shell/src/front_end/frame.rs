//! Ordered framework, HUD and media steps of the modern interface frame.

use super::{
    dispatch_ui_owner_requests, pointer_over_game_view_at, service_journal, Ui, UiNotices,
};
use crate::platform::host::Host;
impl<H: Host> Ui<'_, '_, H> {
    /// The UI's own step of frame step 7.
    pub(super) fn ui_frame(
        &mut self,
        now: dereth_primitives::LocalTime,
        notices: UiNotices,
    ) -> bool {
        let (screen_changed, requests) = self.advance_ui(now);
        self.restore_screen_layout(now, screen_changed);
        let shell = self.front.ui.as_mut().expect("checked above");
        // What the character-management screen asked the player system for. This is the whole of
        // the character screen's outbound half — log on, delete and restore a character — and it
        // is what makes clicking a character enter the world.
        let character_actions = shell.take_character_actions();
        let chargen_actions = shell.take_chargen_actions();
        // The epilogue screen logs off the character when the player system exists and
        // the network is still up.
        let log_off = shell.take_log_off();
        // Interaction takes what it owns out of this list in `interaction::use_time`; whatever it
        // hands back is still reported, because a request nobody owns must be a line and not a
        // silence.
        let mut ui_requests = requests;
        let mouse_events = shell.take_mouse_events();
        if shell.stats.device_done {
            // Mark the device done -- the epilogue screen's only job.
            self.cx.quit();
        }
        self.update_hud(screen_changed, notices);
        self.drive_key_bindings();
        self.save_requested_keymap();
        // PM restoration/this frame's HUD writes are synchronous local property changes in
        // retail. Do not defer them to next frame or make their cache change trigger a re-seed.
        self.cx.consume_placement_requests(
            &mut self
                .front
                .ui
                .as_mut()
                .map(|s| s.ui.requests.take_placement_updates())
                .unwrap_or_default(),
            dereth_primitives::ServerTime(now.0),
        );
        self.dispatch_hud_requests(now, &mut ui_requests);
        self.finish_ui_media(screen_changed, character_actions, chargen_actions, log_off);
        // The 3D character preview's update, then the preview pass's own device work -- both
        // **outside** the frame bracket, for the same reason `prepare_ui` is: adding preview objects uploads
        // textures and `Gpu::upload_texture` runs a command list of its own.
        self.cx.queue(mouse_events, ui_requests);
        screen_changed
    }

    /// Complete session actions and upload or play this frame's media in their existing order.
    fn finish_ui_media(
        &mut self,
        screen_changed: bool,
        character_actions: Vec<dereth_ui_screens::screens::charmgmt::CharacterAction>,
        chargen_actions: Vec<dereth_ui_screens::screens::chargen::CharGenAction>,
        log_off: bool,
    ) {
        let shell = self.front.ui.as_mut().expect("checked above");
        // The movie's current frame, uploaded outside the frame bracket with the rest
        // of the UI textures and released the moment the next one replaces it.
        let movie_frame = shell.take_movie_frame();
        // The same movie's soundtrack. It is taken beside the frame because the two come from one movie open and the audio
        // renderer starts with the video one.
        let movie_audio_cue = shell.take_movie_audio_cue();
        // The media-playback sound update — every sound media step this frame's element state
        // changes ran, which is every button click sound in the game. Taken here, beside the
        // movie's cue, because it is the same hand-off for the same reason and because the client
        // makes the call synchronously: a click's sound must start on the frame of the click.
        let ui_sound_requests = shell.take_sound_requests();
        // After the shell borrow ends: these reach the session, which the shell does not own.
        self.cx.run_character_actions(character_actions);
        self.cx.run_chargen_actions(chargen_actions);
        if log_off {
            self.cx.log_off_character();
        }
        // The mode transition destroyed the outgoing screen and every element in it, so every
        // image-texture link it held is gone: release the descriptor slots before the incoming screen
        // asks for its own. Here rather than inside the frame bracket, so a slot freed now is
        // reusable by the very next upload instead of waiting on this frame's fence.
        if screen_changed {
            let r = self.cx.present_mut().release_ui_textures();
            self.shared.ui_release.freed += r.freed;
            self.shared.ui_release.still_linked += r.still_linked;
            self.shared.ui_release.unknown += r.unknown;
            // Gameplay-screen construction and teardown, which are the only two
            // screen-lifetime calls to display-resolution forcing. It runs here for the
            // same reason as the texture release above: the flow has just destroyed
            // the outgoing screen and built the incoming one, so this *is* the ctor/dtor edge.
        }
        if let Some(frame) = movie_frame {
            self.cx
                .present_mut()
                .set_movie_frame(crate::ui::MOVIE_IMAGE_ID, &frame);
        }
        if let (Some(cue), Some(audio)) = (movie_audio_cue, self.cx.audio_mut()) {
            match cue {
                crate::ui::MovieAudioCue::Start(a) => audio.play_movie_audio(&a),
                crate::ui::MovieAudioCue::Stop => audio.stop_movie_audio(),
            }
        }
        // Anything that is not a `PlaySound` comes straight back out and joins the
        // frame's other unowned requests, so nothing disappears into the sound system.
        let leftover = self.cx.play_sounds(ui_sound_requests);
        for r in leftover {
            tracing::warn!(target: "dereth_client_shell::front_end", "UI sound drain returned {r:?}, which cannot happen");
        }
    }

    /// Advance the framework and finish its synchronous input listeners.
    fn advance_ui(
        &mut self,
        now: dereth_primitives::LocalTime,
    ) -> (bool, Vec<dereth_client_contract::UiRequest>) {
        let shell = self
            .front
            .ui
            .as_mut()
            .expect("the UI's own step runs only with a UI");
        let host = self.cx.pregame().clone();
        // A mode transition replaces the framework even for the same mode id.
        // Count actual reconstructions, not UiShell's mode-id changes: the old HUD handles
        // and notice subscribers die on either transition.
        let switches_before = shell.flow.switches;
        // Capture the outgoing notebook before its widgets are destroyed. Ordinary edits
        // already reach the shared store at the input-listener boundary; this visibility edge
        // requests its final tagged save, which the host services outside the model.
        if shell.flow.queued_mode().is_some()
            && shell.flow.current_mode() == Some(dereth_ui::framework::mode::GAME_PLAY)
        {
            self.cx
                .hud_mut()
                .panels
                .journal
                .on_visibility_changed(&mut shell.ui, false);
        }
        let targeted_dialogs = &mut self.front.targeted_dialogs;
        // The FPS meter reads the process render globals during the
        // UI tick. This scene owns their reconstructed values; sample before the world update
        // later in the frame, matching native's UI-before- ordering.
        let framerate_display_values = self.cx.scene().map(|world| {
            let (auto_update_deg_mul, deg_mul, user_bias) = world.degrade_meter();
            (
                world.frame_rate_fps(),
                dereth_ui_screens::hud::world_view::deg_var(
                    auto_update_deg_mul,
                    deg_mul,
                    user_bias,
                ),
            )
        });
        let serial = self.front.gameplay_serial;
        // Object search is gated on the input-device manager: without one, the client does not
        // search at all. A headless App with no `input_manager` runs `NullInputPump`, which
        // reproduces that condition.
        let cidm = self.shared.input.is_some();
        let mut early_unowned = Vec::new();
        // The motion facts and the pending selection notices were delivered by the runtime just
        // before this step (`App::ui_use_time`), ahead of the input below.
        // Tell-to-Selected reads the live selected id and its name when it starts. `Hud::drive`
        // normally projects that pair at the end of this UI step, which is one frame too late when an
        // object was cleared after the preceding projection. Refresh just that existing snapshot
        // before input dispatch; the ordinary Hud pass below still owns the rest of the screen.
        let cx = &mut *self.cx;
        if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
            crate::hud_drive::game_call(
                &mut shell.ui,
                screen,
                dereth_ui_screens::screens::gameplay_host::GameCall::AutoTargetWorld(
                    cx.hud().auto_target_world(cx.model()),
                ),
            );
        }
        if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
            crate::hud_drive::game_call(
                &mut shell.ui,
                screen,
                dereth_ui_screens::screens::gameplay_host::GameCall::ChatState(
                    cx.hud().chat_focus_view(cx.model()),
                ),
            );
        }
        // Retail external listeners call game globals synchronously. UiSystem has released
        // its element callback borrow here, so the complete listener sequence can finish before
        // the next action. No selective Use extraction may overtake a drag lock or split write.
        let mut dispatch = |shell: &mut crate::ui::UiShell, point: crate::ui::UiDispatch| {
            if let crate::ui::UiDispatch::Mouse(event) = point {
                cx.pointer(event);
                return;
            }
            // The smart-box global-loop listener, reached by
            // the global-message-3 broadcast. `UiShell::route_input` raises this hook once a
            // frame whether or not the pointer moved, and before the action drain, because that
            // is where the broadcast sits: after the mouse update and tooltip check and before
            // the input manager's per-frame update. The object finder takes the exact last-over
            // element as an item slot first; an empty or spell slot supplies id zero, while a
            // non-item falls through to the geometric pick.
            if let crate::ui::UiDispatch::Hover(position) = point {
                // The selection-blink flip counter in the global loop runs *before*
                // the input-device-manager gate, so the
                // blink ticks whether or not there is an input device to search with.
                if shell.flow.current_mode() == Some(dereth_ui::framework::mode::GAME_PLAY) {
                    cx.selection_blink(now);
                }
                if cidm && shell.flow.current_mode() == Some(dereth_ui::framework::mode::GAME_PLAY)
                {
                    let over = shell.ui.mouse_over();
                    let item = over
                        .and_then(|h| dereth_ui_screens::items::runtime::identity(&shell.ui, h))
                        .map(|i| i.0);
                    // `find_object`'s rectangle test belongs to *this* point.
                    // `note_game_viewport` also pushes this latch, once a frame and from the
                    // pointer position the previous frame left behind, so without this line the
                    // first hover after the pointer entered `<SBOX>` would be refused as being under
                    // a HUD window and armed no geometric pick. The last-entered element is what
                    // the object finder reads, and `UiSystem::mouse_over` is that field,
                    // already switched for this position by `UiShell::route_input`.
                    cx.hover(position, item, pointer_over_game_view_at(shell, over), now);
                }
                return;
            }
            early_unowned.extend(dispatch_ui_owner_requests(
                shell,
                cx,
                targeted_dialogs,
                serial,
                now,
            ));
        };
        let mut requests = match self.shared.input.as_mut() {
            Some(input) => shell.frame_with_dispatch(now, &host, input, &mut dispatch),
            None => {
                shell.frame_with_dispatch(now, &host, &mut dereth_ui::NullInputPump, &mut dispatch)
            }
        };
        // `@title` raises the real popup notice after command dispatch. Its receiver draws the
        // literal and emits the set-chat-window-title notice; take that one local-module write back
        // into this frame's request list so PlayerModule persistence cannot lag behind the visible
        // caption.
        let chat_window_title_notices = self.cx.take_chat_window_titles();
        if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
            for (window_id, title) in chat_window_title_notices {
                crate::hud_drive::game_call(
                    &mut shell.ui,
                    screen,
                    dereth_ui_screens::screens::gameplay_host::GameCall::ChatWindowTitle {
                        window_id,
                        title: dereth_ui_screens::view::ChatWindowTitle::Literal(title),
                    },
                );
            }
        }
        requests.extend(shell.ui.requests.take_placement_updates());

        // The frame-rate command has raised its ordered notice edge(s). Native notice handlers
        // have no replay history: drain them even when this is not a gameplay screen, and never
        // infer an enable for a replacement screen from the process flag. A bound receiver then
        // performs its ordinary guarded per-frame update from the existing renderer values.
        let framerate_display_notices = self.cx.take_framerate_display_switches();
        let had_framerate_display_notice = !framerate_display_notices.is_empty();
        if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
            use dereth_ui_screens::screens::gameplay_host::GameCall;
            for enabled in framerate_display_notices {
                crate::hud_drive::game_call(
                    &mut shell.ui,
                    screen,
                    GameCall::FramerateDisplay {
                        enabled,
                        values: framerate_display_values,
                    },
                );
            }
            if !had_framerate_display_notice {
                if let Some((framerate, degrade)) = framerate_display_values {
                    crate::hud_drive::game_call(
                        &mut shell.ui,
                        screen,
                        GameCall::FramerateUseTime { framerate, degrade },
                    );
                }
            }
        }
        // Resolve each pending object and apply or restore its lighting, for every
        // call requested by the wrapper's notice and
        // the global loop made this frame — in this frame, before the world draws, as the
        // client's synchronous calls are. The object may be gone (the lookup returns nothing);
        // that is `apply_object_lighting`'s `false` and is dropped exactly as retail drops it.
        self.cx.apply_selection_lighting();
        // These have no game owner. Preserve their existing diagnostic path, without replaying
        // any already completed request or silently dropping unsupported calls.
        requests.extend(early_unowned);
        let screen_changed = shell.flow.switches != switches_before;
        // The wizard is the one game phase a UI enters by asking: tell the runtime on each edge.
        let in_creation = shell.flow.current_mode() == Some(dereth_ui::framework::mode::CHAR_GEN);
        if in_creation != self.front.in_creation {
            self.front.in_creation = in_creation;
            requests.push(dereth_client_contract::UiRequest::CharacterCreation(
                in_creation,
            ));
        }
        if screen_changed {
            if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
                crate::hud_drive::game_call(
                    &mut shell.ui,
                    screen,
                    dereth_ui_screens::screens::gameplay_host::GameCall::ChatState(
                        self.cx.hud().chat_focus_view(self.cx.model()),
                    ),
                );
                for draft in self.cx.chat_entry_drafts() {
                    crate::hud_drive::game_call(
                        &mut shell.ui,
                        screen,
                        dereth_ui_screens::screens::gameplay_host::GameCall::ChatEntry(draft),
                    );
                }
            }
        }
        // Old-screen/input placement calls completed before the new screen's PM restoration.
        // Consume these local writes now, not in the later network/interaction step.
        self.cx
            .consume_placement_requests(&mut requests, dereth_primitives::ServerTime(now.0));
        let has_external_subscriber = shell.flow.current().is_some_and(|s| s.is_game());
        if screen_changed || !has_external_subscriber {
            self.cx.end_external_container_watches();
        }
        service_journal(self.cx);
        (screen_changed, requests)
    }

    /// Apply a pending layout after its construction frame and commit its placements.
    fn restore_screen_layout(&mut self, now: dereth_primitives::LocalTime, screen_changed: bool) {
        let shell = self.front.ui.as_mut().expect("checked above");
        // **The load end of the screen-layout file, and the only automatic one the client has.**
        // Player-description delivery loads the `"#auto"` screen layout and records whether the
        // layout came from a file; this is the caller of `dereth_ui_screens`' `load_screen_layout`
        // and the path builders.
        //
        // It is deferred to a frame rather than done in the `PlayerDescription` arm because the
        // mode switch is *queued*: `0x0013` arrives before the mode transition has built the gameplay
        // screen, and a notice that reaches no gameplay screen does nothing in the client either.
        // `None` is "not up yet, ask again"; a parse failure is reported and the flag cleared, so
        // a corrupt file cannot re-fail once per frame for the rest of the session.
        // The gameplay constructor's forced display resolution `(false, 800, 600)` is modeled at the
        // screen-lifetime edge below, after this shell borrow ends. On the construction frame the
        // new root therefore still measures the forced login presentation. `#auto` includes that
        // measurement in its filename, so looking now searches for `...-600-800.txt` and clears
        // the notice before the saved gameplay-size file can be found. Retail runs the constructor
        // (and lifts the force) before this notice receiver. Defer only this screen-change frame;
        // the next frame sees the restored presentation and preserves ordinary in-game 0x0013
        // refresh behavior.
        if self.cx.screen_layout_pending() && !screen_changed {
            let prefs = self.cx.config().preferences_file.clone();
            let character = self
                .cx
                .pregame()
                .entered_character
                .clone()
                .unwrap_or_default();
            let world = self.cx.pregame().world_name.clone().unwrap_or_default();
            if let Some(result) = shell.auto_load_screen_layout(&prefs, &character, &world) {
                self.cx.screen_layout_done();
                match result {
                    Ok(true) => {
                        tracing::info!(target: "dereth_client_shell::front_end", "screen layout loaded for {character:?} on {world:?}")
                    }
                    Ok(false) => tracing::info!(target: "dereth_client_shell::front_end",
                        "no saved screen layout for {character:?} on {world:?} \
                         (layout not from file)"
                    ),
                    Err(e) => {
                        tracing::warn!(target: "dereth_client_shell::front_end", "screen layout will not load: {e}")
                    }
                }
            }
        }
        // Local layout loading also invokes TBAR's real resize/move tails. Commit their
        // achieved values before PM readback; leave other queued UI actions in place.
        self.cx.consume_placement_requests(
            &mut shell.ui.requests.take_placement_updates(),
            dereth_primitives::ServerTime(now.0),
        );
    }

    /// Project the HUD and deliver notices only to subscribers that existed when emitted.
    fn update_hud(&mut self, screen_changed: bool, notices: UiNotices) {
        let UiNotices {
            power_bar: power_bar_notices,
            external_container: external_container_notices,
            slumlord_range_exits,
            book_range_exits,
            salvage: salvage_notices,
            trade_for_dummies,
        } = notices;
        // The HUD: the toolbar read-out, the radar's coordinates and compass, the player-module refresh and the chat deliveries — all of them inside step 7,
        // and all of them **before** the blit list is taken, or the frame would draw last frame's
        // HUD over this frame's world.
        if screen_changed {
            self.front.gameplay_serial += 1;
        }
        self.cx.sync_hud();
        // The map panel's first update block, whose source is the same
        // `GameTime` the sky runs on. Read beside the HUD sync above because this is the one
        // point in the frame where the scene and the HUD are both in hand; `Hud::sync` cannot do it
        // because it is not given the scene.
        let game_date_time = self.cx.scene().and_then(|s| s.game_date_time());
        self.cx.hud_mut().game_date_time = game_date_time;
        {
            let serial = self.front.gameplay_serial;
            let (hud, objects) = self.cx.hud_and_objects();
            let shell = self.front.ui.as_mut().expect("checked above");
            if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
                if !screen_changed {
                    hud.pending_external_container
                        .extend(external_container_notices);
                    hud.queue_salvage_notices(salvage_notices);
                    for slumlord in slumlord_range_exits {
                        hud.panels
                            .slumlord
                            .recv_object_range_exit(&mut shell.ui, slumlord);
                    }
                    use dereth_ui_screens::screens::gameplay_host::GameCall;
                    for book in book_range_exits {
                        crate::hud_drive::game_call(
                            &mut shell.ui,
                            screen,
                            GameCall::BookRangeExit(book),
                        );
                    }
                    // Into the same one-frame queue a drop on the table uses, so the optimistic row
                    // and the `0x01F8` come from `TradePanel::drop_item` once, from both of
                    // retail's two callers.
                    for item in trade_for_dummies {
                        crate::hud_drive::game_call(
                            &mut shell.ui,
                            screen,
                            GameCall::OfferTradeItem(item),
                        );
                    }
                }
                hud.drive(&mut shell.ui, screen, serial, objects);
                // A newly constructed subscriber did not exist when this batch was emitted.
                // Its `post_init` state remains authoritative until a subsequent notice arrives.
                if !screen_changed {
                    let writes = u64::from(crate::hud_drive::deliver_power_bar_notices(
                        &mut shell.ui,
                        &mut hud.panels,
                        power_bar_notices,
                    ));
                    hud.stats.power_bar_writes += writes;
                    hud.stats.panels_written += writes;
                }
            }
        }
    }

    /// Complete an options request through the host's keymap writer.
    fn save_requested_keymap(&mut self) {
        // Save to the keymap-file path (not forced), the *OK*
        // button's first line — the input manager saves the keymap to `dir + name`. Here rather
        // than in `UiShell` because the writer is `InputShell::save_keymap`, the same one shutdown
        // uses; the shell only latches the request.
        if self
            .front
            .ui
            .as_mut()
            .is_some_and(crate::ui::UiShell::take_save_keymap)
        {
            match self
                .shared
                .input
                .as_ref()
                .map(crate::input::InputShell::save_keymap)
            {
                Some(Ok(true)) => {}
                Some(Ok(false)) | None => {
                    tracing::warn!(target: "dereth_client_shell::front_end", "the key bindings were applied with no keymap path");
                }
                Some(Err(e)) => {
                    tracing::warn!(target: "dereth_client_shell::front_end", "the keymap was not saved: {e}")
                }
            }
        }
    }

    /// Finish HUD-produced requests, including the element messages that produce descendants.
    fn dispatch_hud_requests(
        &mut self,
        now: dereth_primitives::LocalTime,
        ui_requests: &mut Vec<dereth_client_contract::UiRequest>,
    ) {
        // Hud's external panel updates call game globals too: e.g.
        // selects the first row synchronously. Finish those calls in their producing frame,
        // not as retained requests which overwrite the next frame's fresh selection/input.
        // OpenVendor first emits an actual Menu message; its external handler then produces
        // Select. Draining only direct requests would leave that broadcast one frame late.
        let cx = &mut *self.cx;
        let targeted_dialogs = &mut self.front.targeted_dialogs;
        let serial = self.front.gameplay_serial;
        self.front
            .ui
            .as_mut()
            .expect("checked above")
            .deliver_with_dispatch(&mut |shell, _| {
                ui_requests.extend(dispatch_ui_owner_requests(
                    shell,
                    cx,
                    targeted_dialogs,
                    serial,
                    now,
                ));
            });
    }
}
