//! The key-binding page's ordered initialization, capture and file controls.

use super::{Ui, KEYMAP_DEFAULT};
use crate::platform::host::Host;
use dereth_ui_screens::screens::gameplay_host::{GameCall, KeyBindingsCall as K};

impl<H: Host> Ui<'_, '_, H> {
    /// **The key-binding page's three host calls.**
    ///
    /// The Character Options drains that need only a `GameView` run in `Hud::drive`. These three
    /// need the host's `dereth_input::InputManager`, which `Hud::drive` is not given; without this
    /// function the page builds no rows in a running client and a captured key reaches nothing.
    ///
    /// The client's own three call sites, in this order:
    ///
    /// | this | client |
    /// |---|---|
    /// | `Self::key_bindings_built` gate | initialize options once per page |
    /// | the key-hit drain | call the registered key-hit handler inside input firing |
    /// | `drive_key_bindings` | handle the option-page element message synchronously |
    /// | `set_key_hit_handler` | register the row as input handler `0x20` when binding starts |
    ///
    /// **The order inside matters and is the client's.** Option initialization comes first, because nothing can
    /// capture before there are rows. The key hits **after** `drive_key_bindings` would lose a
    /// frame; before it would dispatch a key into a row whose capture this frame's click has not
    /// started yet. Queued hits therefore go before the drain, which consumes them. Key-hit
    /// handling runs inside the message pump, while element-message handling runs
    /// in the frame — the same two moments the client has.
    ///
    /// **The registration is mirrored at the end, on the edge**, exactly as
    /// `UiShell::sync_text_mode` mirrors: this crate cannot hand
    /// `dereth_input` a pointer to an action-key-map option, so the row's own *"is my map-warn
    /// dialog up"* state is copied into the manager's exclusive
    /// key-hit-handler registration instead.
    pub(super) fn drive_key_bindings(&mut self) {
        let serial = self.front.gameplay_serial;
        let preferences_file = self.cx.config().preferences_file.clone();
        let built = &mut self.front.key_bindings_built;
        let stats = &mut self.front.key_binding_stats;
        let (Some(shell), Some(input)) = (self.front.ui.as_mut(), self.shared.input.as_mut())
        else {
            return;
        };
        let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) else {
            return;
        };
        let mut pass = KeyBindingPass {
            ui: &mut shell.ui,
            screen,
            input,
            stats,
            built,
            preferences_file: &preferences_file,
        };
        if !pass.initialize(serial) || !pass.capture_controls() || !pass.page_buttons() {
            return;
        }
        pass.synchronize_capture();
    }
}

/// One borrow of the live page and its input manager for the ordered drains.
struct KeyBindingPass<'a> {
    ui: &'a mut dereth_ui::UiSystem,
    screen: &'a mut dyn dereth_ui::Screen,
    input: &'a mut crate::input::InputShell,
    stats: &'a mut super::KeyBindingStats,
    built: &'a mut Option<u64>,
    preferences_file: &'a std::path::Path,
}

impl KeyBindingPass<'_> {
    fn call(&mut self, k: K) -> K {
        let mut call = GameCall::KeyBindings(k);
        let took = self.screen.on_game(
            &mut dereth_ui::framework::ScreenCx::new(self.ui),
            &mut dereth_ui::framework::GameCx {
                call: &mut call,
                lent: None,
                input: Some(&mut self.input.manager),
            },
        );
        crate::hud_drive::expect_taken(took, &call);
        match call {
            GameCall::KeyBindings(k) => k,
            _ => unreachable!("the page answers in the call it was given"),
        }
    }

    fn initialize(&mut self, serial: u64) -> bool {
        // 1., once per screen construction.
        if *self.built != Some(serial) {
            *self.built = Some(serial);
            let K::InitOptions {
                out: (rows, headers, failures),
            } = self.call(K::InitOptions { out: (0, 0, 0) })
            else {
                return false;
            };
            let bindable = self
                .input
                .manager
                .action_map
                .entries()
                .filter(|(map, action, _)| {
                    dereth_input::presentation::find(*map, *action)
                        .is_some_and(|r| r.shown(dereth_input::presentation::Interface::Retail))
                })
                .count();
            self.stats.init_calls += 1;
            self.stats.rows_built = rows;
            self.stats.bindable_actions = bindable;
            self.stats.headers = headers;
            self.stats.failures = failures;
            let name = self.input.keymap_display_name();
            self.call(K::RefreshFileName(name));
            // A denominator, not a bare number: `0` and `0 of 306` read the same in a log and only
            // one of them is a bug.
            tracing::debug!(target: "dereth_client_shell::front_end",
                "key bindings -- {rows} of {bindable} bindable action(s) got a row, \
                 {} section header(s), {} failure(s)",
                headers,
                failures
            );
        }

        true
    }

    fn capture_controls(&mut self) -> bool {
        // 2. -- the controls the manager diverted
        //    while a capture was in flight, offered to the page that asked for them.
        for control in self.input.take_key_hits() {
            self.stats.key_hits_offered += 1;
            if let K::KeyHit { taken: true, .. } = self.call(K::KeyHit {
                control,
                taken: false,
            }) {
                self.stats.key_hits_taken += 1;
            }
        }

        // 3. The per-frame drain: the queued element messages, then the queued key hits.
        let K::Drive { events, verdicts } = self.call(K::Drive {
            events: Vec::new(),
            verdicts: Vec::new(),
        }) else {
            return false;
        };
        self.stats.row_events += events.len() as u64;
        self.stats.captures += verdicts.len() as u64;
        for v in &verdicts {
            if matches!(v, dereth_input::binding::Capture::Ready { .. }) {
                self.stats.bindings_made += 1;
            }
        }

        true
    }

    fn page_buttons(&mut self) -> bool {
        // 3b. The key-binding page's four button arms, drained from the same pass as the rows' —
        //     Apply, Cancel, *Restore Defaults* and *Revert to Saved*.
        let K::TakePageEvents(page_events) = self.call(K::TakePageEvents(Vec::new())) else {
            return false;
        };
        for e in page_events {
            use dereth_ui_screens::options::keybinding::PageEvent;
            self.stats.page_button_events += 1;
            match e {
                PageEvent::Applied { rows, saved } => {
                    self.stats.rows_applied += rows as u64;
                    if saved {
                        self.stats.keymaps_written += 1;
                    }
                }
                PageEvent::RestoredSaved(n) => self.stats.rows_reverted += n as u64,
                PageEvent::RestoredDefaults(n) => self.stats.rows_defaulted += n as u64,
                PageEvent::LoadKeymapDialog => {
                    match self.input.scheme_names(crate::input::MODERN_SLUG) {
                        Ok(mut files) => {
                            // "Default", the shipped maps, first; then this interface's own saved
                            // key maps, by the names they were saved under.
                            files.insert(0, KEYMAP_DEFAULT.to_owned());
                            let current = self.input.modern_scheme_in_use();
                            self.call(K::OpenLoad { files, current });
                        }
                        Err(error) => {
                            tracing::warn!(target: "dereth_client_shell::front_end", "enumerate keymaps failed: {error}")
                        }
                    }
                }
                PageEvent::SaveKeymapDialog => {
                    self.call(K::OpenSave);
                }
                PageEvent::LoadKeymap(name) if name == KEYMAP_DEFAULT => {
                    // The shipped maps, in the key map file in use.
                    if self.input.restore_shipped_keys() {
                        self.call(K::Reinit);
                    }
                }
                PageEvent::LoadKeymap(name) => match self
                    .input
                    .load_keymap_file(&crate::input::scheme_file(&name, crate::input::MODERN_SLUG))
                {
                    Ok(true) => {
                        if let Some(file) = self.input.keymap_file_name() {
                            if let Err(error) =
                                crate::input::save_keymap_preference(self.preferences_file, &file)
                            {
                                tracing::warn!(target: "dereth_client_shell::front_end", "save keymap preference failed: {error}");
                            }
                        }
                        self.call(K::Reinit);
                        self.call(K::RefreshFileName(Some(name)));
                    }
                    Ok(false) => {}
                    Err(error) => {
                        tracing::warn!(target: "dereth_client_shell::front_end", "load keymap failed: {error}")
                    }
                },
                PageEvent::SaveKeymap(name) => match self.input.save_keymap_as(&name, false) {
                    Ok(Some(crate::input::SaveKeymapAs::Saved)) => {
                        self.stats.keymaps_written += 1;
                        if let Some(name) = self.input.keymap_file_name() {
                            if let Err(error) =
                                crate::input::save_keymap_preference(self.preferences_file, &name)
                            {
                                tracing::warn!(target: "dereth_client_shell::front_end", "save keymap preference failed: {error}");
                            }
                            let label = self.input.keymap_display_name();
                            self.call(K::RefreshFileName(label));
                        }
                    }
                    Ok(Some(crate::input::SaveKeymapAs::NeedsOverwrite)) => {
                        self.call(K::OpenOverwrite(name));
                    }
                    Ok(Some(crate::input::SaveKeymapAs::ReadOnly)) => {
                        self.call(K::OpenReadOnly(name));
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(target: "dereth_client_shell::front_end", "save keymap failed: {error}")
                    }
                },
                PageEvent::OverwriteKeymap(name) => match self.input.save_keymap_as(&name, true) {
                    Ok(Some(crate::input::SaveKeymapAs::Saved)) => {
                        self.stats.keymaps_written += 1;
                        if let Some(name) = self.input.keymap_file_name() {
                            if let Err(error) =
                                crate::input::save_keymap_preference(self.preferences_file, &name)
                            {
                                tracing::warn!(target: "dereth_client_shell::front_end", "save keymap preference failed: {error}");
                            }
                            let label = self.input.keymap_display_name();
                            self.call(K::RefreshFileName(label));
                        }
                    }
                    Ok(Some(crate::input::SaveKeymapAs::ReadOnly)) => {
                        self.call(K::OpenReadOnly(name));
                    }
                    Ok(Some(crate::input::SaveKeymapAs::NeedsOverwrite)) | Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(target: "dereth_client_shell::front_end", "overwrite keymap failed: {error}")
                    }
                },
            }
        }

        true
    }

    fn synchronize_capture(&mut self) {
        // 4. Register the row as input handler `0x20` / unregister it, on the edge.
        let K::Capturing(capturing) = self.call(K::Capturing(false)) else {
            return;
        };
        if capturing != self.input.manager.key_hit_handler_registered() {
            self.input.set_key_hit_handler(capturing);
        }
    }
}
