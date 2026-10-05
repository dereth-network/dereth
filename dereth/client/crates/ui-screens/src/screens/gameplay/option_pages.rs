//! The option pages: client, character, chat and key-binding options.

use super::*;

impl GamePlayScreen {
    /// The client's three calls, on the page the
    /// click actually belongs to; see [`is_under_config_page`].
    ///
    /// Returns the option page's own answer — how many controls were written — so a test can say
    /// *"Cancel reverted three"* rather than *"Cancel did not crash"*.
    pub fn on_config_page_button(
        &mut self,
        ui: &mut UiSystem,
        source: dereth_ui::ElemHandle,
        source_id: ElementId,
    ) -> usize {
        use crate::options::config::button;
        if !matches!(source_id, button::APPLY | button::CANCEL | button::DEFAULTS) {
            return 0;
        }
        // The three ids are shared by all three option pages, so the click is attributed by
        // walking up to `0x10000213`; that is what makes this arm `ClientOptionsPanel`'s and not
        // `ChatOptionsPanel`'s.
        if !is_under_config_page(ui, source) {
            return 0;
        }
        match source_id {
            // The option page base's save of the current values — a snapshot, not a write.
            button::APPLY => {
                self.config_page.save_current_values();
                0
            }
            button::CANCEL => self.config_page.restore_saved_values(ui),
            _ => self.config_page.restore_default_values(ui),
        }
    }

    /// One press on the Options *Game / Support* page, performed.
    ///
    /// The client does all five of these itself; in
    /// this build the effects live on the screen and on the request queue, so the page returns the
    /// action ([`crate::options::gameplay::GameplayOptionsPage::on_element_message`]) and this
    /// performs it. Returns how many controls the press wrote, which is zero for everything but
    /// *Restore Defaults* — a number a station can assert on rather than "it did not panic".
    ///
    /// | action | here |
    /// |---|---|
    /// | `EndCharacterSession { ask }` | [`Self::on_end_character_session`], the asking form |
    /// | `OpenUrl` | [`crate::requests`] — this crate may not call `ShellExecuteA` |
    /// | `BroadcastGlobal { id: 1, param }` | [`Self::handle_key_press`], because the only listener in this build that answers global 1 with `0x10000027` is that arm, so it is called rather than the bus re-entered from inside a dispatch |
    /// | `BroadcastGlobal { id: 0x0C, .. }` | *Use Mouse Turning Settings*: the Client Options page's preset, then the wheel bindings |
    ///
    /// **Global `0x0C` applies the mouse-turning settings.** Its two listeners are the Client
    /// Options page, which sets its six mouse-turning rows
    /// ([`crate::options::page::PlayerOptionPage::set_mouse_turning_defaults`]) and prints a chat
    /// line for each it moves, and the key bindings page's two camera-zoom rows, which take the
    /// mouse wheel. The second needs the host's `InputManager`, so it runs in
    /// [`Self::drive_key_bindings`]. Nothing else changes: no other option and no character
    /// option. Each page restores its own defaults with its own *Defaults* button.
    pub fn on_gameplay_options_action(
        &mut self,
        ui: &mut UiSystem,
        a: &dereth_client_contract::options::sheet::Act,
    ) -> usize {
        use dereth_client_contract::options::sheet::Act as A;
        match a {
            A::ExitToCharacterSelection => {
                self.on_end_character_session(ui, 1);
                0
            }
            A::ExitGame => {
                self.handle_key_press(ui, 0x1000_0027);
                0
            }
            A::MouseTurningSettings => {
                let lines = self.config_page.set_mouse_turning_defaults(ui);
                let n = lines.len();
                for text in lines {
                    ui.requests.emit(UiRequest::DisplayChatText {
                        channel: crate::options::config::MOUSE_TURNING_CHANNEL,
                        text,
                        feedback: dereth_client_contract::feedback::Feedback::LOCAL,
                    });
                }
                self.mouse_turning_keys_pending = true;
                n
            }
            A::ConfigureKeyboard | A::UrgentAssistance | A::ReportAbuse => 0,
        }
    }

    /// The same three buttons, on the **Character Options** page.
    ///
    /// The character settings panel's element-message handler is one function and both pages
    /// use it — the config panel's construction and the character settings panel's construction
    /// both build a `PlayerOptionPage`, and the handler is the one they share — so the
    /// three element ids `0x100001FC`/`FD`/`FE` mean the same three calls here as they do there.
    /// What tells the two apart is only **which page the click sits under**, which is exactly what
    /// [`is_under_config_page`] settles for the other one.
    ///
    /// Without this arm Apply, Cancel and *Restore Defaults* on the Character page do nothing at
    /// all: the buttons are in the shipped tree and raise message 1, but nothing else is keyed on
    /// this page.
    ///
    /// The page is matched by **handle** rather than by id, because `0x10000211` appears twice in
    /// the shipped tree — see [`crate::options::character::find_page`], which is what bound this
    /// one.
    ///
    /// Returns the page's own answer — how many rows were written — so a test can say *"Cancel
    /// reverted three"* rather than *"Cancel did not crash"*. Defaults answers **0** on a host
    /// with no `GameView::player_option_default`; see
    /// [`crate::options::character::CharacterSettingsPage::restore_default_values`].
    pub fn on_character_page_button(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        source_id: ElementId,
    ) -> usize {
        use crate::options::config::button;
        match source_id {
            // Re-read every row from the module and
            // re-snapshot the saved values, which is what gives Cancel something to revert to. It
            // writes no option, exactly as on the other page.
            button::APPLY => self.character_options.save_current_values(ui, view),
            button::CANCEL => self.character_options.restore_saved_values(ui),
            button::DEFAULTS => self.character_options.restore_default_values(ui),
            _ => 0,
        }
    }

    ///  on the Client Options page.
    ///
    /// **This is the part a rebuild is most likely to skip**, and it is the one the row named:
    /// closing the page without pressing Apply calls `restore_saved_values` and rolls every
    /// uncommitted change back. Returns how many controls were reverted.
    pub fn config_page_visibility_changed(&mut self, ui: &mut UiSystem, visible: bool) -> usize {
        self.config_page.on_visibility_changed(ui, visible)
    }

    /// Read all three option pages again from what they show: the Client Options page from
    /// the preference store, the Character and Chat Options pages from the character. For when
    /// the other interface changed them while this one was put away. Returns how many rows
    /// moved.
    pub fn reread_option_pages(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> usize {
        let fonts = self.chat_font_rows.clone();
        self.config_page.reread(ui)
            + self.config_page.reread_options(ui, &fonts)
            + self.character_options.save_current_values(ui, view)
            + self.chat_options.save_current_values(ui, view)
    }

    // ---- the Character Options page ---------------------------------------------------------

    /// The option page's visibility handling on the Character Options page —
    /// `save_current_values` on show, `restore_saved_values` on hide.
    ///
    /// Returns how many rows moved, which is the observable half: a show that re-read 50 identical
    /// values and one that read nothing at all are otherwise the same.
    ///
    /// **Known defect, pinned by a test.** The *first* show of a `Panel` page never delivers its
    /// `0x18`, so this show arm does not run on a first visit. The fix belongs in
    /// `dereth_ui::widgets`.
    pub fn character_options_visibility_changed(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        visible: bool,
    ) -> usize {
        self.character_options
            .on_visibility_changed(ui, view, visible)
    }

    /// Re-read every Character Options check box from the `PlayerModule`.
    ///
    /// This is `save_current_values` without the visibility edge, for the host to call when the
    /// module it is a view of changes — `0x0013`'s `PlayerModule` arriving, or a `0x01A1` going
    /// out. Returns how many rows moved.
    pub fn update_character_options(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> usize {
        self.character_options.save_current_values(ui, view)
    }

    /// Take the queued visibility edges without applying them — for a test that wants to observe
    /// **which** edges reached the screen rather than what they did.
    pub fn take_character_option_visibility(&mut self) -> Vec<bool> {
        std::mem::take(&mut self.character_option_visibility)
    }

    /// Drain the Character Options page's queued visibility edges against the host's view.
    ///
    /// Called once per frame, beside [`Self::drive_key_bindings`] and `crate::requests::take`.
    /// Returns how many rows each edge moved, in the order the edges arrived.
    pub fn drive_character_options(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
    ) -> Vec<usize> {
        // Apply / Cancel / Restore Defaults, drained ahead of the visibility edges
        // because a click always precedes the close that would follow it.
        let buttons: Vec<usize> = std::mem::take(&mut self.character_option_buttons)
            .into_iter()
            .map(|id| self.on_character_page_button(ui, view, id))
            .collect();
        let mut out = buttons;
        out.extend(
            std::mem::take(&mut self.character_option_visibility)
                .into_iter()
                .map(|visible| {
                    // The character-option page's save-current-values method is an
                    // *override*: it calls `save_to_server(module, false)` and only then the base
                    // save-current-values method. The page's visibility handler reaches it on
                    // the show edge, so this is where a page that comes up with un-flushed option
                    // changes behind it sends its `0x01A1`. The host owns the module and the wire;
                    // the page raises the request, in the client's order — save first, re-read second.
                    if visible {
                        ui.requests.emit(UiRequest::SavePlayerOptions);
                    }
                    self.character_options
                        .on_visibility_changed(ui, view, visible)
                })
                .collect::<Vec<usize>>(),
        );
        out
    }

    // ---- the Chat Options page --------------------------------------------------------------

    /// The gameplay-option-changed notice `(prop, windowId)`, delivered to the chat windows.
    ///
    /// The player module's property-changed hook raises it as its **first line** on every
    /// per-window chat option write and every option write, so the sender is the module and the
    /// receivers are the five windows. Sender and receiver are both inside this process and the
    /// module is the host's, so the page hands its effect back and this delivers it — the same
    /// shape [`Self::on_end_character_session`] uses for a notice whose sender and
    /// receiver are both this screen.
    ///
    /// The two arms are **not** symmetrical, and that is measured:
    ///
    /// * The chat interface's gameplay option changed notice tests that the property is
    ///   `0x1000007F` **and** that the window id is its own, so a filter reaches exactly one
    ///   window;
    /// * The floaty main chat panel's gameplay option changed notice intercepts
    ///   `0x10000080` / `0x10000081` **before** chaining and applies them with **no** window-id
    ///   test, so one pair of sliders moves all five.
    ///
    /// Returns how many windows took it.
    pub fn chat_recv_notice_gameplay_option_changed(
        &mut self,
        ui: &mut UiSystem,
        effect: crate::options::chat::ChatOptionEffect,
    ) -> usize {
        use crate::chat::interface::opacity_attr;
        use crate::options::chat::ChatOptionEffect as E;
        match effect {
            E::Opacity { property, value } => {
                let (d, a) = match property {
                    opacity_attr::DEFAULT => (Some(value), None),
                    opacity_attr::ACTIVE => (None, Some(value)),
                    _ => return 0,
                };
                self.chat_set_stored_opacity(ui, d, a)
            }
            E::Filter { window_id, mask } => self.chat_set_stored_filter(ui, window_id, mask),
        }
    }

    /// The chat interface's font-settings-changed notice is offered to every chat
    /// window.
    ///
    /// The producer is the font-preference change callback,
    /// registered during preference initialization on **both** `UI.ChatFontFace` and
    /// `UI.ChatFontSize`; it reads the two statics it was registered against and broadcasts them.
    /// Here the statics are [`crate::options::store`]'s registry, which is where the Client
    /// Options page's Apply writes, and the callback is
    /// [`crate::options::store::font_preference_epoch`] — see [`Self::drive_chat_font`].
    ///
    /// The face and size are `UInt` preferences, so the value is the **row index** into
    /// the chat font face and chat font size choice lists; `Chat_<Face>_<Size>` is then a name
    /// in the font enum-id map and that name's enum maps to the font's `DataID`. The shipped
    /// `client_local_English.dat` carries all 25 of them, e.g. `Chat_PalatinoLinotype_Small` ->
    /// `0x40000000` (which is what the layout already gives the log, so the *defaults* agree and
    /// only a change is observable) and `Chat_Tahoma_XL` -> `0x4000000C`.
    ///
    /// Returns how many windows re-resolved their font. `0` is a miss at any hop and is the
    /// client's behaviour too: every failure arm of the client leaves the log's font alone.
    pub fn chat_recv_notice_font_settings_changed(&mut self, ui: &mut UiSystem) -> usize {
        use crate::view::PrefValue;
        let index = |name: &str| match crate::options::store::inq_value(name) {
            Some(PrefValue::Int(v)) => usize::try_from(v).ok(),
            _ => None,
        };
        let face = index(dereth_ui::persist::preferences::keys::CHAT_FONT_FACE);
        let size = index(dereth_ui::persist::preferences::keys::CHAT_FONT_SIZE);
        let (Some(face), Some(size)) = (face, size) else {
            return 0;
        };
        let Some(did) = crate::chat::interface::resolve_chat_font(ui, face, size) else {
            return 0;
        };
        // The font fetch. A font this build cannot measure is a miss, not a silent
        // swap to nothing.
        let Some(metrics) = ui.font_metrics(did) else {
            return 0;
        };
        let mut n = 0;
        for w in self.chat_windows.clone() {
            n += usize::from(w.set_chat_font(ui, did, std::sync::Arc::clone(&metrics)));
        }
        n
    }

    /// The client's callback edge, drained once a frame.
    ///
    /// Retail's callback is synchronous, inside the store's own write. This crate's store cannot
    /// call back into a screen, so [`crate::options::store::font_preference_epoch`] counts the
    /// writes and this compares it with the one already answered — the same edge, and it catches
    /// the live menu choice, *Cancel* and *Restore Defaults* alike because all three go through
    /// `set_value`. Returns how many windows moved.
    pub fn drive_chat_font(&mut self, ui: &mut UiSystem) -> usize {
        let now = crate::options::store::font_preference_epoch();
        if now == self.font_preference_epoch {
            return 0;
        }
        self.font_preference_epoch = now;
        self.chat_recv_notice_font_settings_changed(ui)
    }

    /// The chat interface's gameplay option changed notice's `0x1000007F` arm, offered to
    /// all five windows so the window-id compare is the thing that selects one.
    ///
    /// Returns how many windows took it — **one** for a real window id, and `0` for an id no
    /// window carries, which is the loud form of "the blob names a window this tree does not
    /// have".
    pub fn chat_set_stored_filter(
        &mut self,
        ui: &mut UiSystem,
        window_id: u32,
        mask: u64,
    ) -> usize {
        let _ = ui;
        let mut n = 0;
        for w in &mut self.chat {
            n += usize::from(w.on_gameplay_option_changed(
                crate::options::chat::CHAT_FILTER_PROPERTY,
                window_id,
                mask,
            ));
        }
        n
    }

    /// The chat interface's per-window filter read over all five windows — the **filter** third of
    /// the client's "filter, position, opacity".
    ///
    /// Returns how many windows moved.
    pub fn chat_set_stored_filters(&mut self, ui: &mut UiSystem, filters: &[(u32, u64)]) -> usize {
        let _ = ui;
        let mut n = 0;
        for w in &mut self.chat {
            let mask = filters
                .iter()
                .find(|(id, _)| *id == w.window_id)
                .map(|(_, m)| *m);
            n += usize::from(w.update_filter_from_player_module(mask));
        }
        n
    }

    /// The Chat Options page's Apply / Cancel / Restore Defaults, once the host's [`GameView`] is
    /// available.
    ///
    /// The character settings panel's element-message handler is the one handler all three
    /// `PlayerOptionPage` subclasses share, so `0x100001FC`/`FD`/`FE` mean the same three calls
    /// here as on the other two pages; what tells them apart is only which page the press sits
    /// under. Returns each press's answer — how many controls were written.
    pub fn on_chat_page_button(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        source_id: ElementId,
    ) -> usize {
        use crate::options::config::button;
        let fonts = self.chat_font_rows.clone();
        let (n, effects) = match source_id {
            // The option page base's save of the current values — a re-read, not a write.
            button::APPLY => (
                self.chat_options.save_current_values(ui, view)
                    + self.config_page.reread_options(ui, &fonts),
                Vec::new(),
            ),
            button::CANCEL => {
                let (n, e) = self.chat_options.restore_saved_values(ui);
                (n + self.config_page.restore_saved_options(ui, &fonts), e)
            }
            button::DEFAULTS => {
                let (n, e) = self.chat_options.restore_default_values(ui);
                (n + self.config_page.restore_default_options(ui, &fonts), e)
            }
            _ => return 0,
        };
        for e in effects {
            self.chat_recv_notice_gameplay_option_changed(ui, e);
        }
        n
    }

    /// Re-read every Chat Options control from the `PlayerModule` — `save_current_values` without
    /// the visibility edge, for the host to call when the module it is a view of changes.
    pub fn update_chat_options(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> usize {
        self.chat_options.save_current_values(ui, view)
    }

    /// Drain the Chat Options page's queued button presses and visibility edges against the
    /// host's view, delivering every notice the page's writes raise.
    ///
    /// Called once per frame beside [`Self::drive_character_options`]. Returns what each drained
    /// event answered, buttons first — a click always precedes the close that would follow it.
    pub fn drive_chat_options(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> Vec<usize> {
        let mut out: Vec<usize> = std::mem::take(&mut self.chat_option_buttons)
            .into_iter()
            .map(|id| self.on_chat_page_button(ui, view, id))
            .collect();
        for visible in std::mem::take(&mut self.chat_option_visibility) {
            // The player-option page's save-current-values is an override: it calls
            // `save_to_server(module, false)` and only then
            // `save_current_values`, and the visibility-changed handler reaches it on
            // the **show** edge. Same order as the Character Options page — save first, re-read
            // second.
            if visible {
                ui.requests.emit(UiRequest::SavePlayerOptions);
            }
            // The chat font's rows on the page read again on show and go back on hide, as the
            // page's own do.
            let fonts = self.chat_font_rows.clone();
            if visible {
                self.config_page.reread_options(ui, &fonts);
            } else {
                self.config_page.restore_saved_options(ui, &fonts);
            }
            let (n, effects) = self.chat_options.on_visibility_changed(ui, view, visible);
            for e in effects {
                self.chat_recv_notice_gameplay_option_changed(ui, e);
            }
            out.push(n);
        }
        out
    }

    /// Take the queued visibility edges without applying them — for a test that wants to observe
    /// **which** edges reached the screen rather than what they did.
    pub fn take_chat_option_visibility(&mut self) -> Vec<bool> {
        std::mem::take(&mut self.chat_option_visibility)
    }

    // ---- the key-binding page ---------------------------------------------------------------

    /// Build one row per user-bindable action.
    ///
    /// Separate from [`Self::post_init`] because every row reads the merged master input map,
    /// which lives in the host's `InputManager`. Returns how many rows were built; `0` with a
    /// bound page means the action map was empty, which is a loud answer rather than a silent one.
    pub fn key_bindings_init_options(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_input::InputManager,
    ) -> usize {
        self.key_bindings.init_options(ui, m)
    }

    /// A key press captured while a key-binding row is in capture —
    /// the input manager's key-hit handler call, whose one registered handler is the row that
    /// raised the map-warn dialog.
    ///
    /// Queued rather than dispatched, for the reason [`Self::key_binding_inbox`] gives. Returns
    /// whether any row is actually capturing, which is the client's own *"is a key-hit handler
    /// registered"* and is what stops every keystroke in the game being queued here.
    pub fn key_bindings_key_hit(&mut self, control: dereth_input::ControlChord) -> bool {
        if !self
            .key_bindings
            .rows
            .iter()
            .any(super::super::super::options::keybinding::ActionKeyMapRow::capturing)
        {
            return false;
        }
        self.key_binding_key_hits.push(control);
        true
    }

    /// Drain the queued key-binding work against the host's `InputManager`.
    ///
    /// Called once per frame, after `UiFlow::frame`, exactly where `crate::requests::take` is
    /// called and for the same reason. Returns `(row, event)` per element message taken and the
    /// verdict of each captured key, in dispatch order.
    pub fn drive_key_bindings(
        &mut self,
        ui: &mut UiSystem,
        m: &mut dereth_input::InputManager,
    ) -> (
        Vec<(usize, crate::options::keybinding::RowEvent)>,
        Vec<dereth_input::binding::Capture>,
    ) {
        let mut events = Vec::new();
        // The mouse-turning settings' second half: the two camera-zoom rows take the wheel.
        if std::mem::take(&mut self.mouse_turning_keys_pending) {
            for line in self.key_bindings.set_mouse_turning_defaults(ui, m) {
                ui.requests.emit(UiRequest::DisplayChatText {
                    feedback: dereth_client_contract::feedback::Feedback::LOCAL,
                    channel: crate::options::config::MOUSE_TURNING_CHANNEL,
                    text: line.to_owned(),
                });
            }
        }
        for msg in std::mem::take(&mut self.key_binding_inbox) {
            // The key-binding page handler is the page's own handler, and each
            // `ActionKeyMapOption` has the row handler;
            // they are two listeners on one broadcast, so both are offered every message and the
            // page is offered it first, which is the initialization registration order. A message
            // can only ever match one of them — the page's buttons are not
            // row children.
            if let Some(e) = self.key_bindings.on_page_element_message(ui, m, &msg) {
                self.key_binding_page_events.push(e);
                continue;
            }
            if let Some(e) = self.key_bindings.on_element_message(ui, m, &msg) {
                events.push(e);
            }
        }
        // The client's element half. The row above owns the
        // context; this screen owns the live root alongside its other gameplay elements.
        self.roots
            .extend(self.key_bindings.service_dialog_elements(ui));
        let mut verdicts = Vec::new();
        for control in std::mem::take(&mut self.key_binding_key_hits) {
            // Through the page rather than into the row, so that
            //  runs on the far side of
            // Which is the only thing that lights *Revert to Saved*.
            if let Some(v) = self.key_bindings.key_hit(ui, m, control) {
                verdicts.push(v);
            }
        }
        // A captured key closes and deletes its wait dialog before returning here, then may open a
        // conflict dialog. Give that new context its live element in the same frame, and deliver
        // physical Yes/No/Notice answers from dialogs that were already on screen.
        self.roots
            .extend(self.key_bindings.service_dialog_elements(ui));
        let _ = self.key_bindings.service_dialog_answers(ui, m);
        if let Some(event) = self.key_bindings.service_file_dialog_answer(ui) {
            self.key_binding_page_events.push(event);
        }
        self.roots.retain(|h| ui.node(*h).is_some());
        (events, verdicts)
    }

    /// The refresh-action-key-mapping notice: every row re-reads the map.
    pub fn key_bindings_refresh(&mut self, ui: &mut UiSystem, m: &dereth_input::InputManager) {
        self.key_bindings.on_refresh_action_key_mapping(ui, m);
    }

    /// Whether an element message is addressed to a key-binding row — a key button, a clear
    /// button, or the row element itself — **or** to one of the page's own six buttons.
    ///
    /// **The page half matters.** Queueing only row elements would mean a click on Apply, Cancel,
    /// *Restore Defaults* or *Revert to Saved* never reaches [`Self::drive_key_bindings`] at all:
    /// the four buttons are in the shipped tree and raise their messages.
    #[must_use]
    pub(super) fn is_key_binding_element(&self, source: ElemHandle) -> bool {
        let p = &self.key_bindings;
        if [
            p.load_button,
            p.save_button,
            p.reset_defaults_button,
            p.revert_to_saved_button,
            p.ok_button,
            p.cancel_button,
        ]
        .contains(&Some(source))
        {
            return true;
        }
        p.rows.iter().any(|r| {
            r.element == Some(source)
                || r.clear_button == Some(source)
                || r.key_buttons.contains(&source)
        })
    }

    /// Take what the Key Bindings page's own buttons did this frame.
    pub fn take_key_binding_page_events(&mut self) -> Vec<crate::options::keybinding::PageEvent> {
        std::mem::take(&mut self.key_binding_page_events)
    }
}
