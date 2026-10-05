//! Ending the character session: the logout notices and the confirmation dialog.

use super::*;

impl GamePlayScreen {
    /// The game play screen's end character session notice.
    ///
    /// The screen keeps four flags — [`Self::ending_session`], [`Self::do_end_session`],
    /// [`Self::should_quit_on_logout`] and [`Self::logout_confirmed`] — and the notice sets them
    /// as follows:
    ///
    /// * `param != 0`: end the session, do not quit, and raise the confirmation dialog with
    ///   `ID_Client_EndCharacterSessionConfirm`;
    /// * `param == 0`: confirmed, end the session, and quit.
    ///
    /// The two branches are *ask, then go to character select* and *quit now, without asking*; the
    /// button that sends `1` is named for the former. The asking branch reads like "quit on
    /// logout"; retail does otherwise, and this follows retail.
    pub fn on_end_character_session(&mut self, ui: &mut UiSystem, param: i32) {
        if param != 0 {
            self.do_end_session = true;
            self.should_quit_on_logout = false;
            self.make_logout_confirmation_dialog(ui, logout::END_SESSION_CONFIRM);
        } else {
            self.logout_confirmed = true;
            self.do_end_session = true;
            self.should_quit_on_logout = true;
        }
    }

    /// The log-off notice: the same dialog with `ID_Client_LogoffConfirm`, and quit-on-logout set,
    /// so *Yes* quits.
    pub fn on_logoff(&mut self, ui: &mut UiSystem) {
        self.do_end_session = true;
        self.should_quit_on_logout = true;
        self.make_logout_confirmation_dialog(ui, logout::LOGOFF_CONFIRM);
    }

    /// The game play screen's logout confirmation dialog build.
    ///
    /// Only when no dialog context exists: clear the confirmation, show the framework, and make
    /// the dialog (properties `0x8E`, `0xC3`, `0xC5`) — it refuses to raise a second one while the
    /// first is up, un-confirms, and forces the framework visible, so the player can never hide a
    /// modal dialog.
    ///
    /// **What is faithful and what is not.** The element, its layout, its two buttons, the
    /// sentence, the modality (blocking clicks and moving the dialog to the front), and every
    /// flag transition are the
    /// client's. This path bypasses the dialog factory:
    /// the factory lives on [`dereth_ui::framework::UiFlow`] and a [`Screen`] has no handle to it,
    /// so the queues, the contexts, the open-dialog notice and the "N waiting" banner are not
    /// exercised here. This screen raises exactly one dialog and never two, so nothing it does
    /// depends on a queue — but the five server-driven confirmations and the wait dialog do.
    pub fn make_logout_confirmation_dialog(&mut self, ui: &mut UiSystem, string_id: &str) -> bool {
        if self.logout_dialog.is_some() {
            return false;
        }
        self.logout_confirmed = false;
        // `Show(1)`.
        self.shown = true;
        if let Some(root) = self.root() {
            ui.set_visible(root, true);
        }
        let Ok(h) = ui.require_env().and_then(|e| {
            e.create_and_add_root_element(ui, logout::DIALOG_LAYOUT, logout::CONFIRMATION_ROOT)
        }) else {
            return false;
        };
        // Property 0xAC. `DialogElement::on_set_attribute` turns it into `block_clicks` plus
        // bring-to-front, which is what puts the dialog over the HUD and stops a click reaching
        // the world behind it.
        ui.set_attribute_bool(h, dereth_ui::props::attr::DIALOG_MODAL, true);
        // By **element id**, not by bubbling: the confirmation dialog itself listens for message 1
        // from these two children and returns stop-processing
        // (`dialog::types::DialogElement::listen_to_element_message`), so a listener registered on
        // the dialog's root would never be reached. The by-id table is consulted first, which is
        // the same route `DisconnectedScreen` takes to its OK button.
        ui.register_for_element_message(logout::BUTTON_YES, MessageId(1), ME);
        ui.register_for_element_message(logout::BUTTON_NO, MessageId(1), ME);
        // Recorded as one of this screen's roots, so use_new_mode deletes it with the
        // screen — which is `~` calling `Reset()`,
        // "every UI mode switch clears all dialogs".
        self.roots.push(h);
        self.logout_dialog = Some(h);
        self.logout_prompt = Some(string_id.to_string());
        self.logout_prompt_applied = false;
        true
    }

    /// Whether the prompt still needs its text from the host.
    #[must_use]
    pub fn needs_logout_prompt_text(&self) -> bool {
        self.logout_prompt.is_some() && !self.logout_prompt_applied
    }

    /// The resolved sentence, written into the dialog's one `TextElement`.
    pub fn show_logout_prompt(&mut self, ui: &mut UiSystem, text: &str) {
        self.logout_prompt_applied = true;
        self.logout_prompt_text = Some(text.to_string());
        let Some(d) = self.logout_dialog else { return };
        if let Some(h) = ui.get_child_recursive(d, logout::PROMPT_TEXT) {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(text);
            }
        }
        // The dialog text write ends by resizing and repositioning the popup, and
        // the client calls it too: the panel is grown to fit the sentence and then
        // centred. Without it the shipped 400 x 95 panel shows the first line of a two-line prompt
        // and sits in the top-left corner instead of over the middle of the screen.
        dereth_ui::dialog::base::update_popup_size_and_position(ui, d);
    }

    /// The confirmation dialog, while it is up.
    #[must_use]
    pub fn logout_dialog(&self) -> Option<ElemHandle> {
        self.logout_dialog
    }

    /// The client's logout arm.
    ///
    /// On the context that matches the log-out dialog's: **[`Self::ending_session`] is cleared**,
    /// so the per-frame update will look again, and, when the answer under property 0x92 is
    /// true,
    /// [`Self::logout_confirmed`] and [`Self::do_end_session`] are set. *No* therefore leaves the
    /// player in the world with every flag back where it started.
    pub fn close_logout_dialog(&mut self, ui: &mut UiSystem, accepted: bool) {
        let Some(h) = self.logout_dialog.take() else {
            return;
        };
        self.roots.retain(|r| *r != h);
        ui.unregister_for_element_message(logout::BUTTON_YES, MessageId(1), ME);
        ui.unregister_for_element_message(logout::BUTTON_NO, MessageId(1), ME);
        ui.remove_and_delete_root(h);
        self.logout_prompt = None;
        self.logout_prompt_text = None;
        self.logout_prompt_applied = false;
        self.ending_session = false;
        if accepted {
            self.logout_confirmed = true;
            self.do_end_session = true;
        }
    }
}
