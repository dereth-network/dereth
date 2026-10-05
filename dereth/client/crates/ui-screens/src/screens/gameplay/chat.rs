//! The chat windows: keyboard, talk-focus menu, fading and the floaty write-backs.

use super::*;

impl GamePlayScreen {
    // -----------------------------------------------------------------------------------------
    // The chat surface's keyboard, its talk-focus menu and the floaty write-backs
    // -----------------------------------------------------------------------------------------

    /// The `ID_AssistedTell` row, or [`crate::chat::window::ASSISTED_TELL_FALLBACK`] when the host
    /// has no string service.
    fn reply_template(&self) -> &str {
        if self.reply_template.is_empty() {
            crate::chat::window::ASSISTED_TELL_FALLBACK
        } else {
            &self.reply_template
        }
    }

    /// The chat interface's child-action handler, fanned out to the five windows.
    ///
    /// Each window's arms are guarded on "the child that raised the action is *my*
    /// entry", so at most one of the five answers; the fan-out is what a container chain does in
    /// the client, where all five `ChatInterface`s sit on the same parent chain and each one's
    /// `on_child_action` runs the same test.
    ///
    /// Returns whether one of them consumed it. `true` is what stops the focused `TextElement`
    /// running its own `0x25`/`0x27`/arrow arms — see
    /// [`dereth_ui::UiSystem::dispatch_action`] for the ordering this stands in the middle of.
    pub fn chat_on_child_action(
        &mut self,
        ui: &mut UiSystem,
        child: ElemHandle,
        e: &dereth_ui::focus::InputEvent,
    ) -> bool {
        let stay = self.stay_in_chat_mode;
        for i in 0..self.chat_windows.len() {
            let w = self.chat_windows[i];
            let Some(iface) = self.chat.get_mut(i) else {
                continue;
            };
            let r = w.on_child_action(ui, iface, child, e, stay);
            if let Some(req) = r.request {
                ui.requests.emit(req);
            }
            if r.consumed {
                return true;
            }
        }
        false
    }

    /// The chat interface's action handler, delivered to the **main** chat window.
    ///
    /// The main chat panel's post-init is the one chat class that calls
    /// registers input maps `0x1000000D` and `0x1000000A` for itself, so the
    /// six reply/activate/toggle actions reach `MainChat`'s action handler and not the four floaty
    /// windows'. That is also what a player expects: the reply key fills the main box.
    ///
    /// `0x10000119`'s "start a tell to the selection" reads the existing host snapshot of the
    /// current selection and its name in form 2, then raises the same start-tell notice as the
    /// Friends button. The notice receiver owns focus and composition.
    pub fn chat_on_action(&mut self, ui: &mut UiSystem, action: u32) -> bool {
        let e = dereth_ui::focus::InputEvent {
            action,
            start: true,
            x: 0,
            y: 0,
        };
        let Some(w) = self.chat_windows.first().copied() else {
            return false;
        };
        let template = self.reply_template().to_owned();
        let targets = self.reply_targets.clone();
        let selected = Some((
            self.chat_auto_target_world.selected_id,
            self.chat_auto_target_world.selected_name.as_str(),
        ));
        let Some(iface) = self.chat.first_mut() else {
            return false;
        };
        let r = w.on_action(ui, iface, &e, &targets, &template, selected);
        if let Some(req) = r.request {
            ui.requests.emit(req);
        }
        r.consumed
    }

    /// The client's menu arm, reached from an element
    /// message on the talk-focus menu container `0x10000014`.
    ///
    /// On action 7 the menu's selection is cleared; the picked item is `p2` (none: stop). The
    /// squelch toggle item toggles squelch on the current speakable target; any other item whose
    /// enum attribute `0x1000000B` is set selects that talk focus.
    ///
    /// The client reads the focus off the **item's** attribute `0x1000000B`. The items are created
    /// at runtime by the menu's text-item insert, which this build's `Menu`
    /// does not implement, so the row is addressed by its **index in the menu** instead —
    /// the talk-focus menu build adds the squelch toggle at index 0 and then
    /// [`crate::chat::mainchat::TALK_FOCUS_MENU_ORDER`], which carries exactly the ids those
    /// enum-attribute writes set. Index 0 is the squelch toggle and is not a focus.
    pub fn chat_target_menu_selection(
        &mut self,
        ui: &mut UiSystem,
        index: usize,
    ) -> Option<crate::chat::mainchat::TalkFocusChange> {
        let focus = *crate::chat::mainchat::TALK_FOCUS_MENU_ORDER.get(index.checked_sub(1)?)?;
        let change = self.main_chat.on_menu_chosen(ui, focus)?;
        ui.requests.emit(UiRequest::SetTalkFocus {
            focus: change.focus,
        });
        Some(change)
    }

    /// The same arm addressed the way the client addresses it — by the **element** in `p2`.
    ///
    /// This is the whole of the client's message-7 body,
    /// including the branch [`Self::chat_target_menu_selection`] could not have: the squelch row
    /// is matched by pointer, not by attribute and not by index, because it is the one row
    /// `init_talk_focus_menu` deliberately leaves untagged.
    pub fn chat_target_menu_item(
        &mut self,
        ui: &mut UiSystem,
        item: dereth_ui::ElemHandle,
    ) -> crate::chat::mainchat::MenuRowChoice {
        use crate::chat::mainchat::MenuRowChoice;
        let choice = self.main_chat.on_menu_chosen_item(ui, item);
        match &choice {
            MenuRowChoice::Focus(change) => {
                ui.requests.emit(UiRequest::SetTalkFocus {
                    focus: change.focus,
                });
            }
            MenuRowChoice::Squelch => {
                let object = dereth_primitives::ObjectId(self.main_chat.last_speakable_target);
                if object.0 != 0 {
                    ui.requests.emit(UiRequest::ToggleCharacterSquelch(object));
                }
            }
            MenuRowChoice::None => {}
        }
        choice
    }

    /// Advances the opacity fade for each of the five chat windows by one step.
    ///
    /// Returns the windows that moved this frame, as `(window id, opacity)`, so a test can say
    /// *"the main window faded to 0.98"* rather than *"nothing crashed"*. A window whose fade has
    /// settled is absent — that is the client unregistering from global message 3, which a
    /// rebuild easily misses.
    pub fn chat_fade_tick(&mut self, ui: &mut UiSystem) -> Vec<(u32, f32)> {
        let mut out = Vec::new();
        for i in 0..self.chat.len() {
            let Some(w) = self.chat_windows.get(i).copied() else {
                continue;
            };
            let engaged = w.fade_engaged(ui);
            let Some(v) = self.chat[i].fade_tick(engaged) else {
                continue;
            };
            w.set_opacity(ui, &mut self.chat[i], v);
            out.push((self.chat[i].window_id, v));
        }
        out
    }

    /// The two **stored** chat options — gameplay-option properties `0x10000080` and
    /// `0x10000081`, which `ChatOptionsPanel`'s two general-section sliders write and the floaty
    /// chat panel's update from the player module re-reads.
    ///
    /// This is the property *read* half: the host decodes the `PlayerModule` (it owns
    /// the player system, a `Screen` does not) and hands the pair down. `None` leaves that half
    /// alone, which is a module that carries no value rather than one that carries zero — the
    /// distinction the client's fallback of `1.0f` turns on.
    ///
    /// Applied to every window, because the two sliders are in the **general** section and are
    /// not per-window. Re-arms the fade, since the target moved.
    /// Returns how many windows were written.
    pub fn chat_set_stored_opacity(
        &mut self,
        ui: &mut UiSystem,
        default_opacity: Option<f32>,
        active_opacity: Option<f32>,
    ) -> usize {
        if default_opacity.is_none() && active_opacity.is_none() {
            return 0;
        }
        for i in 0..self.chat.len() {
            let Some(w) = self.chat_windows.get(i).copied() else {
                continue;
            };
            // `set_default_opacity` first, exactly as `on_set_attribute` applies them, so its
            // "raise active to match" clause runs before the active value arrives.
            //
            // They go through [`crate::chat::window::ChatWindow`]'s own setters, which preserve
            // both active and idle tails: an idle window takes the new idle value **now**, an
            // engaged one takes the new active value now, and it is that push — not the fade — that
            // the shipped `ChatOptionsPanel` slider relies on. Moving only the fade's target would
            // change the window by 5 % of its travel per frame on a drag and not at all on a login
            // until something happened to tick the fade.
            if let Some(d) = default_opacity {
                w.set_default_opacity(ui, &mut self.chat[i], d);
            }
            if let Some(a) = active_opacity {
                w.set_active_opacity(ui, &mut self.chat[i], a);
            }
            // The window may still be the wrong side of its own target — an engaged window whose
            // *idle* value moved has nothing to apply now and everything to fade to when the
            // pointer leaves — so it re-subscribes either way, which is
            // where the client re-subscribes to global message 3.
            self.chat[i].fading = true;
        }
        self.chat.len()
    }

    /// The client's `case 0x12`, delivered to whichever of
    /// the five windows owns the entry the character was typed into.
    ///
    /// Returns what was expanded, which is `None` for every character that is not the space after
    /// one of the five aliases — i.e. almost all of them.
    pub fn chat_on_entry_character(
        &mut self,
        ui: &mut UiSystem,
        source: ElemHandle,
        ch: char,
    ) -> bool {
        let targets = self.reply_targets.clone();
        for i in 0..self.chat_windows.len() {
            let w = self.chat_windows[i];
            if w.entry != Some(source) {
                continue;
            }
            let Some(iface) = self.chat.get_mut(i) else {
                return false;
            };
            return w.on_entry_character(ui, iface, source, ch, &targets);
        }
        false
    }

    /// The main chat panel's element-message handler's **message-1** arm:
    /// element `0x1000046F` runs the maximize-button handling.
    pub fn chat_on_maximize_button(&mut self, ui: &mut UiSystem) -> Option<(i32, i32)> {
        let root = self.root()?;
        let main = ui.get_child_recursive(root, window::MAIN_CHAT)?;
        // The main window's own children, so `handle_maximize_button`'s resize lands in
        // resize_to the way the client's virtual call does.
        let w = self.chat_windows.first().copied().unwrap_or_default();
        self.main_chat.handle_maximize_button(ui, main, &w)
    }

    /// Light the lamp button that stands
    /// for one of the four floaty chat windows.
    ///
    /// This runs **in addition to** [`Self::recv_set_panel_visibility`]'s page swap, because the
    /// notice is broadcast and both `PanelStack` and `MainChat` receive it.
    pub fn chat_recv_set_panel_visibility(&mut self, ui: &mut UiSystem, panel: u32, visible: bool) {
        let Some(root) = self.root() else { return };
        crate::chat::mainchat::MainChatPanel::on_set_panel_visibility(
            ui,
            root,
            ElementId(panel),
            visible,
        );
    }

    /// The start-tell notice's `(name)` one
    /// receiver, which is on the **main** window.
    ///
    /// The notice is broadcast to every notice handler, but only the main chat window (and its
    /// floating main-chat variant) handles the start-tell notice, so the four `FloatingChat`
    /// windows ignore it and only `chat_windows[0]` fills. Reached from `UiShell::handle_request`'s
    /// [`UiRequest::StartTell`] arm.
    ///
    /// Returns whether a main window was there to take it.
    /// Apply shared entry text without submitting or recording another command.
    pub fn chat_entry_drafts(&mut self, ui: &mut UiSystem) -> Vec<(u32, String)> {
        self.chat
            .iter_mut()
            .zip(&self.chat_windows)
            .filter_map(|(iface, window)| {
                let text = ui.text_element_mut(window.entry?)?.glyphs.inq_text(true);
                iface.entry.clone_from(&text);
                Some((iface.window_id, text))
            })
            .collect()
    }

    pub fn chat_entry_update(
        &mut self,
        ui: &mut UiSystem,
        update: &dereth_client_contract::chat::entry::EntryUpdate,
    ) {
        let Some(index) = self.chat.iter().position(|c| c.window_id == update.window) else {
            return;
        };
        let Some(window) = self.chat_windows.get(index) else {
            return;
        };
        let iface = &mut self.chat[index];
        iface.entry.clone_from(&update.text);
        if update.focus {
            window.activate_chat_entry(ui, iface);
        }
        if let Some(text) = window.entry.and_then(|entry| ui.text_element_mut(entry)) {
            text.set_text(&update.text);
            text.cursor = update.cursor.min(text.glyphs.len());
            text.deselect();
        }
    }

    pub fn chat_recv_notice_start_tell(&mut self, ui: &mut UiSystem, name: &str) -> bool {
        let Some(w) = self.chat_windows.first().copied() else {
            return false;
        };
        let Some(iface) = self.chat.first_mut() else {
            return false;
        };
        w.on_start_tell(ui, iface, name);
        true
    }

    /// The main chat panel's text tag iid string click notice.
    ///
    /// The receiver accepts only `TextTagType::Tell` (`0x10000001`), ignores the IID, and starts
    /// the main tell editor only while its entry is not focused. The string payload is the
    /// appropriate-form name composed into the clickable run.
    pub fn chat_recv_notice_iid_string_click(
        &mut self,
        ui: &mut UiSystem,
        payload: &dereth_ui::NoticePayload,
    ) {
        if payload.a != 0x1000_0001 {
            return;
        }
        if self
            .chat_windows
            .first()
            .is_some_and(|w| w.is_text_entry_focused(ui))
        {
            return;
        }
        self.chat_recv_notice_start_tell(ui, payload.text.as_deref().unwrap_or(""));
    }

    ///  for the four floaty windows: message 1
    /// on `0x1000052A` hides the window, which writes `Option_Placement_Visibility` back.
    ///
    /// Returns the window id that closed.
    pub fn chat_on_close_button(&mut self, ui: &mut UiSystem, source: ElemHandle) -> Option<u32> {
        for w in &self.floaty_chat {
            if w.listen_to_element_message(ui, source, dereth_ui::msg::element::id::BUTTON_CLICKED)
            {
                return Some(w.window_id);
            }
        }
        None
    }

    /// The set-chat-window-title notice `(windowId, si)`, broadcast to all five windows and
    /// taken by the one whose window id matches — the floaty chat panel's set chat window title
    /// notice.
    pub fn on_set_chat_window_title(
        &mut self,
        ui: &mut UiSystem,
        window_id: u32,
        title: crate::view::ChatWindowTitle,
    ) -> bool {
        for i in 0..self.floaty_chat.len() {
            let mut w = std::mem::take(&mut self.floaty_chat[i]);
            let took = w.on_set_chat_window_title(ui, window_id, title.clone());
            self.floaty_chat[i] = w;
            if took {
                return true;
            }
        }
        false
    }
}
