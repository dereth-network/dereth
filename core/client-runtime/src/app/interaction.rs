//! Interaction frames and session requests.

use super::*;

impl<S: Shell> App<S> {
    pub(super) fn interaction_use_time(&mut self, shell: &mut S) {
        let now = dereth_primitives::LocalTime(self.timer.cur_time);
        let viewport = self.present.size();
        // The render device's viewport rectangle, before
        // `draw_use_time_with_chat_focus`'s step 3 reads it back out.
        self.note_game_viewport(shell);
        // The smart-box global-loop listener does not run here. Its broadcast belongs inside the
        // UI manager's per-frame step, i.e. step 7 -- so it runs in the UI step
        // ([`Shell::ui_frame`]), **before** the input drain (the order is the tooltip check, the
        // global-message-3 broadcast, the input manager's per-frame step). Run here, four frame
        // phases late, it could never precede a click in the same frame, and retail's always does.
        // The actions the UI declined this frame — the action dispatch gives the winning map's
        // callback first refusal and the rest reach the other input handlers.
        let mut actions = self.actions.take();
        // The performance panel's key belongs to no interface and no game system: it flips the
        // option, and the panel follows the option.
        actions.retain(|a| {
            if a.id != dereth_client_contract::actions::dereth::TOGGLE_PERFORMANCE_PANEL {
                return true;
            }
            if a.is_start() {
                let v = dereth_client_contract::options::performance::toggle();
                tracing::info!("performance panel {v:?}");
            }
            false
        });
        // The action handler's `0x1000002B` arm asks whether element
        // `0x100005F7` (`<EXAM>`) is visible before examining anything, and shuts it instead when
        // it is. The client
        // has a manager singleton in reach; `interaction.rs` has no UI at all, so the question is
        // asked here — this frame, after `ui_use_time` has already run — and pushed in. The
        // matching hide is performed below, the statement after `use_time` returns.
        let examine_panel_open = shell.examine_panel_open();
        self.interaction.note_examine_panel_open(examine_panel_open);
        // The spell-cast path's only command-interpreter test, the
        // command interpreter's `controlled_by_server` flag. The free-hands-and-cast path
        // reaches the interpreter through the world objects; `interaction.rs` has none, so
        // the field is pushed in here, in the same way and for the same reason as
        // `note_examine_panel_open` above it. `MovementCommands::lists` is where
        // `lose_control_to_server` sets it and the per-frame step clears it again.
        self.interaction
            .note_controlled_by_server(self.movement.lists.controlled_by_server);
        let (mut unowned, left) = crate::interaction::draw_use_time_with_chat_focus(
            &mut self.interaction,
            &self.store,
            self.present.scene(self.world.as_ref()).as_deref(),
            &mut self.objects,
            self.link.as_mut().map(|l| &mut l.net),
            actions,
            // The send re-reads the spell-component list out of
            // the interface system's player description immediately before it sends, which is the copy the
            // HUD keeps and the same one the footer's cost came from — not the object table's,
            // which `0x0013` reaches before the player's own `0xF745` has even created it.
            self.hud.player_desc_received,
            viewport,
            now,
            &mut |chat| {
                // An option's notice is synchronous relative to the next queued ChatLine.
                // Drain even with no subscriber; no hidden UI may replay this batch later.
                let notices = chat.take_talk_focus_notices();
                crate::ui_context::offer_talk_focus_notices(chat, notices, &mut |focus, notice| {
                    shell.talk_focus_notice(focus, notice)
                });
            },
        );
        // The magic notices this frame's input raised go to the UI's inbox now, in the order they
        // were raised, ahead of the panels that drain them later in this frame. With no UI they
        // stay where they are.
        if shell.has_ui() {
            shell.emit_magic_notices(self.interaction.magic_notices.take());
        }
        // The render-option command owns the parser and ordered command feedback above; the
        // existing `UiRequest::SetPreference` chain below owns live application and persistence.
        // Update the registry at the options-page boundary before the renderer consumes each
        // request, retaining the command order recorded by Interaction.
        for (name, value) in self.interaction.take_render_preferences() {
            let _ = dereth_client_contract::options::store::set_value(name, value.clone());
            unowned.push(dereth_client_contract::UiRequest::SetPreference(
                name, value,
            ));
        }
        // `--set-at`'s settings take the same road.
        for (name, value) in std::mem::take(&mut self.scripted_preferences) {
            unowned.push(dereth_client_contract::UiRequest::SetPreference(
                name, value,
            ));
        }
        // The component-fill operation raises its notice synchronously; the vendor-panel
        // receiver opens buying tab `0x100000BA`. The command/model half above
        // owns the basket but not this live tree. Take the edge every frame even without a shell,
        // and repeat the open guard so a stale edge cannot affect a subsequently opened vendor.
        if let Some(vendor) = self.interaction.take_vendor_buying_tab_request() {
            if self.objects.world.shop.vendor_id == Some(vendor) {
                shell.open_vendor_buying(&mut self.hud);
            }
        }
        // The four layout commands raise a UI notice; the live `UiShell` is
        // the gameplay-screen receiver. Resolve the file beside the configured preferences file
        // with the current character/world identity, then invoke the existing retail-format
        // persistence seam. Retail prints no success, missing-file or I/O-failure line here.
        let layout_commands = self.interaction.take_ui_layout_commands();
        if !layout_commands.is_empty() {
            let prefs = self.cfg.preferences_file.clone();
            let character = self
                .host_state
                .entered_character
                .clone()
                .unwrap_or_default();
            let world = self.host_state.world_name.clone().unwrap_or_default();
            shell.run_ui_layout_commands(&prefs, &character, &world, layout_commands);
        }
        // Complete a geometric target's dialog at this world-draw boundary, also draining
        // notices without UI. No extra frame tick or broadcast pass is introduced.
        shell.service_dialogs(&mut UiContext::new(self), now);
        // The arm calls the visibility setter with `false` on `<EXAM>` itself.
        // `ExaminationPanel::hide` is that
        // call and is the one place `closed` is counted, so this leg and the close button are the
        // same statement in the same function, as they are in the client.
        if self.interaction.take_examine_panel_close() {
            shell.close_examine_panel();
        }
        // The toolbar's case 2 raises this notice synchronously, but
        // `Interaction` cannot see the toolbar panel. Drain it in the same host-effect slot as the
        // neighbouring examine/visibility calls, against the live selection and object view.
        if let Some(selected) = self.interaction.take_split_stack_notice() {
            let view = self.hud.view(&self.objects);
            shell.split_stack(&view, selected);
        }
        // ---- The three effects whose action arms reach through a
        //      singleton that `interaction.rs` cannot see, drained in the same slot and for the
        //      same reason as `0x1000002B`'s hide above. ---------------------------------------
        //
        // `EscapeKey` with nothing selected requests input action `0x1000001B`.
        // `dereth_ui::UiSystem::dispatch_input_action` **is** retail's visibility-toggle dispatch,
        // element message `0x31` to every listener registered for the action; the `else` below
        // reproduces the successful no-UI return.
        if let Some(a) = self.interaction.take_visibility_toggle() {
            // The visibility-toggle dispatch's return means *"the action had a
            // bucket"*, not *"somebody consumed it"* — an empty bucket still answers TRUE.
            // Kept because it is the only thing the call itself hands back, and because a
            // counter beside the call cannot tell a call that happened from one that did not.
            // `None` is no UI to dispatch into.
            if let Some(answered) = shell.dispatch_input_action(a) {
                self.events
                    .push(FrameEvent::EscapeOptionsToggle { answered });
            }
        }
        // The command-interpreter stop call ends by stopping the player completely. The `else`
        // covers an absent player or physics object.
        if self.interaction.take_stop_completely() {
            if let Some(c) = self.world.as_mut().and_then(|w| w.character.as_mut()) {
                c.stop_completely_from_action();
                self.events.push(FrameEvent::EscapeStopPerformed);
            }
        }
        // The screenshot action initializes a path and, **only on success**, formats
        // `L"Screenshot saved to file '%hs'"` into channel `0x1A`.
        //
        // The capture path lacks device-level screenshot
        // operation: retail's device picks the filename, while this build uses the system preferences
        // directory; retail captures inside the action, while this build captures the buffer already
        // on the device when the request is drained — the frame before this one.
        if self.interaction.take_screenshot_request() {
            self.take_action_screenshot();
        }
        self.actions.put_back(left);
        // The device input follows the combat mode (exactly one of the three combat-mode control
        // sets is live at a time) and the target mode, after the step that may have changed them.
        shell.control_notice(crate::shell::ControlNotice::CombatMode(
            self.objects.world.combat.combat_mode.raw(),
        ));
        shell.control_notice(crate::shell::ControlNotice::TargetMode(
            self.interaction.target_mode() != crate::interaction::TargetMode::None,
        ));
        // `UiRequest::SetPreference`'s consumer: the eight `Sound.*` names reach
        // `AudioSystem::set_prefs`. Everything else falls through to the line below, which is the
        // point of that line.
        let unowned = crate::audio::apply_preference_requests(self.audio.as_mut(), unowned);
        // `Display.FullScreen`. It sits before the render names because it is a `Display.*` and not
        // a `Render.*`, and neither of the two below claims it.
        let unowned = self.apply_display_preference_requests(shell, unowned);
        // The client drains UI first, prepares the graphics device, and only then
        // starts the frame.
        // Apply a display request at that same boundary; the event-loop poll remains for
        // Alt+Enter, startup and live window events that do not originate in this drain.
        self.apply_changed_display_presentation(shell);
        // The landscape options are also the next world's: a choice made before the world is
        // drawn (at character select) or kept for the next login is recorded in the scene that
        // will be loaded. A drawn world takes the change live, below.
        for r in &unowned {
            if let dereth_client_contract::UiRequest::SetPreference(name, value) = r {
                if dereth_client_contract::options::landscape::Landscape::of(name).is_some() {
                    for scene in [self.pending_scene.as_mut(), self.scene_config.as_mut()]
                        .into_iter()
                        .flatten()
                    {
                        scene.render.set_named(name, value);
                    }
                }
            }
        }
        let unowned = self.present.apply_render_preference_requests(unowned);
        let unowned: Vec<dereth_client_contract::UiRequest> = unowned
            .into_iter()
            .filter(|r| {
                !matches!(r, dereth_client_contract::UiRequest::SetPreference(name, _)
                    if dereth_client_contract::options::landscape::Landscape::of(name).is_some())
            })
            .collect();
        // The three `Camera.*` names reach the body's camera controller, with
        // a live stiffness update for `Camera.Stiffness`.
        let unowned = crate::camera::apply_preference_requests(
            self.world.as_mut().and_then(|w| w.character.as_mut()),
            unowned,
        );
        // `UiRequest::OpenUrl`, from the Options *Game / Support* page's two support-ticket
        // buttons. Without this filter it would fall past every filter above and only be printed
        // by the line below. See
        // [`apply_open_url_requests`] for the complete consume-and-launch behavior.
        let (unowned, shell_calls) = apply_open_url_requests(unowned);
        for c in shell_calls {
            // Deviation 2 in [`apply_open_url_requests`]: retail's `MessageBoxA` is out
            // of reach of a `forbid(unsafe_code)` crate, so its **own text** goes to the scroll.
            if let ShellCall::ErrorBox { text, .. } = c {
                self.objects.world.scroll.add_feedback_to_scroll(
                    &text,
                    dereth_client_model::scroll::LOCAL_ERROR_TYPE,
                    true,
                    0,
                    dereth_client_contract::feedback::Feedback::LOCAL,
                );
            }
        }
        let unowned = self.apply_session_requests(shell, unowned);
        for r in unowned {
            if self.unowned_gate.ready() {
                let more = std::mem::take(&mut self.unowned_suppressed);
                if more > 0 {
                    tracing::debug!(
                        "UI request with no owner yet: {r:?} (and {more} more since the last line)"
                    );
                } else {
                    tracing::debug!("UI request with no owner yet: {r:?}");
                }
            } else {
                self.unowned_suppressed += 1;
            }
        }
    }

    /// The requests about the session and the process rather than the world: leaving, the
    /// character list's operations, the selected character, the key bindings and the start of a
    /// tell. A front end that handles one itself never passes it on; whatever it passes on is done
    /// here, the same way for every front end. Everything else is handed back.
    fn apply_session_requests(
        &mut self,
        shell: &mut S,
        requests: Vec<dereth_client_contract::UiRequest>,
    ) -> Vec<dereth_client_contract::UiRequest> {
        use dereth_client_contract::UiRequest;
        let mut rest = Vec::new();
        for r in requests {
            match r {
                UiRequest::CharacterAction(a) => self.run_character_actions(vec![a]),
                UiRequest::CharGenAction(a) => self.run_chargen_actions(vec![a]),
                UiRequest::SelectedAvatar(id) => self.host_state.selected_avatar = Some(id),
                UiRequest::EndCharacterSession { .. } => self.log_off_character(),
                UiRequest::DeviceDone => self.pump.done(),
                UiRequest::Quit => self.quit_game(),
                UiRequest::SaveKeyMap => {
                    let outcome = shell.save_bindings();
                    tracing::debug!("key bindings saved: {outcome:?}");
                }
                UiRequest::StartTell { name } => shell.start_tell(name),
                UiRequest::CharacterCreation(open) => self.duties.creating_character = open,
                other => rest.push(other),
            }
        }
        rest
    }
}
