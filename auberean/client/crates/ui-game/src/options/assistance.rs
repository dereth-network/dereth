//! The options window's assistance page: a call for help to the game's staff. The player picks
//! the kind of call, names the one it is about and says what happened; Send checks the call (a
//! call about someone needs their name, an abuse call a description) and sends it, or says why
//! not. Its tab is off the window in this client, so players reach it only by its page's name.

use auberean_ui::framework::FrameId;
use auberean_ui::token::{attr, msg};
use auberean_ui::{Handle, Mail, Token, Ui, Value};

use super::helper::OPTION_STRINGS;
use super::{page, Request};
use crate::gameplay::{self, dialog, Dialog};

mod name {
    use auberean_ui::Token;
    pub const PAGE: Token = Token::of("AssistancePage");
    pub const TAB: Token = Token::of("AssistanceTab");
    pub const DESCRIPTION: Token = Token::of("AssistanceDescriptionEntryText");
    pub const ABUSER: Token = Token::of("AbuserNameEntryText");
    pub const MENU: Token = Token::of("CallTypeMenu");
    pub const SELECTION: Token = Token::of("CallTypeSelection");
    pub const SEND: Token = Token::of("AssistanceSendButton");
    /// A menu row's kind of call.
    pub const CALL_TYPE: Token = Token::of("GUNCallType");
}

/// The kinds of call, in the menu's order: their label and their kind. A kind with `0x4000`
/// needs the name of the one the call is about; one with `0x8000`, a description.
pub const CALL_TYPES: [(&str, u32); 9] = [
    ("ID_Assistance_CallType_Other", 0x8008),
    ("ID_Assistance_CallType_OtherCOC", 0xE007),
    ("ID_Assistance_CallType_Stuck", 0x8003),
    ("ID_Assistance_CallType_Spam", 0x6002),
    ("ID_Assistance_CallType_Macro", 0x4006),
    ("ID_Assistance_CallType_Lag", 4),
    ("ID_Assistance_CallType_Harassment", 0xE001),
    ("ID_Assistance_CallType_Bug", 0x8005),
    ("ID_Assistance_CallType_Abuse", 0xE000),
];

/// The assistance page.
#[derive(Debug)]
pub struct AssistancePage {
    frame: FrameId,
    page: Option<Handle>,
    send: Option<Handle>,
    menu: Option<Handle>,
    description: Option<Handle>,
    abuser: Option<Handle>,
    /// The message up (0: none).
    dialog: u32,
}

impl AssistancePage {
    /// The page under the window's framework: its parts found, its menu filled, its send button,
    /// tab and menu heard; emptied.
    // D2-02889
    pub fn new(ui: &mut Ui, window: FrameId) -> Self {
        let frame = ui.new_child_frame(window);
        let page = ui.element(name::PAGE);
        let find = |ui: &Ui, n| page.and_then(|p| ui.child_recursive(p, n));
        let description = find(ui, name::DESCRIPTION);
        let abuser = find(ui, name::ABUSER);
        let menu = find(ui, name::MENU);
        let send = find(ui, name::SEND);
        let me = Self {
            frame,
            page,
            send,
            menu,
            description,
            abuser,
            dialog: 0,
        };
        if page.is_some() {
            me.init_call_type_menu(ui);
            if let Some(l) = ui.frame_listener(frame) {
                ui.listen_for_message(l, name::SEND, msg::BUTTON_RELEASE_ON);
                ui.listen_for_message(l, name::TAB, page::CLOSED);
                ui.listen_for_message(l, name::MENU, msg::NEW_SELECTION);
            }
        }
        me.reset(ui);
        me
    }

    /// The menu's kinds of call, each row with its kind, each new row put first.
    // D2-02888
    fn init_call_type_menu(&self, ui: &mut Ui) {
        let Some(m) = self.menu else { return };
        for (label, kind) in CALL_TYPES {
            let words = crate::flow::message(ui, OPTION_STRINGS, label);
            if let Some(row) = ui.menu_insert_text(m, &words, 0) {
                ui.set_attr(row, name::CALL_TYPE, Value::Int(kind.cast_signed()));
            }
        }
    }

    /// Emptied: no kind chosen (the menu says to choose one), no name, no description, Send
    /// ghosted.
    // D2-02886
    pub fn reset(&self, ui: &mut Ui) {
        let (Some(a), Some(d), Some(s), Some(m)) =
            (self.abuser, self.description, self.send, self.menu)
        else {
            return;
        };
        ui.menu_select_item(m, None);
        ui.clear_all_text(a);
        ui.clear_all_text(d);
        ui.set_attr(s, attr::GHOSTED, Value::Bool(true));
        if let Some(sel) = self
            .page
            .and_then(|p| ui.child_recursive(p, name::SELECTION))
        {
            let words = crate::flow::message(ui, OPTION_STRINGS, "ID_Assistance_CallType_Default");
            ui.set_text(sel, &words);
        }
    }

    /// A message read: forgotten.
    // D2-02887
    pub fn close_message_dialog(&mut self, id: u32) {
        if id == self.dialog {
            self.dialog = 0;
        }
    }

    fn message(&mut self, ui: &Ui, text: &str, out: &mut Vec<gameplay::Request>) {
        self.dialog = super::new_dialog_id();
        out.push(gameplay::Request::Dialog(Dialog {
            id: self.dialog,
            kind: dialog::MESSAGE,
            text: crate::flow::message(ui, OPTION_STRINGS, text),
            modal: false,
            info: None,
        }));
    }

    /// Send pressed: with a kind chosen, the message up taken down; a call that
    /// needs a name without one, or a description without one, says so; else the call is sent
    /// and the page emptied.
    // D2-02891
    pub fn handle_send_button(&mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        let Some(m) = self.menu else { return };
        let Some(row) = ui.menu_selected_item(m) else {
            return;
        };
        let Some(kind) = ui.node(row).and_then(|n| n.int_attr(name::CALL_TYPE)) else {
            return;
        };
        let kind = kind.cast_unsigned();
        if kind == 0 {
            return;
        }
        if self.dialog != 0 {
            out.push(gameplay::Request::DialogDown(self.dialog));
        }
        let text = |h: Option<Handle>| h.and_then(|h| ui.text(h)).unwrap_or_default();
        let (abuser, description) = (text(self.abuser), text(self.description));
        if kind & 0x4000 != 0 && abuser.is_empty() {
            self.message(ui, "ID_Assistance_Err_NoAbuserName", out);
        } else if kind & 0x8000 != 0 && description.is_empty() {
            self.message(ui, "ID_Assistance_Err_NoAbuseDesc", out);
        } else {
            out.push(gameplay::Request::Options(Request::Assistance {
                kind,
                abuser,
                description,
            }));
            self.reset(ui);
        }
    }

    /// What the page heard: Send; its tab closed (the message up taken down); a kind chosen
    /// (Send no longer ghosted).
    // D2-02892
    pub fn listen(&mut self, ui: &mut Ui, element: Token, out: &mut Vec<gameplay::Request>) {
        if element == name::SEND {
            self.handle_send_button(ui, out);
        }
        if element == name::TAB && self.dialog != 0 {
            out.push(gameplay::Request::DialogDown(self.dialog));
        }
        if element == name::MENU && self.menu.is_some() {
            if let Some(s) = self.send {
                ui.set_attr(s, attr::GHOSTED, Value::Bool(false));
            }
        }
    }

    /// The page's mail.
    pub fn hear(&mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        for m in crate::camera::take(ui, self.frame) {
            if let Mail::Element { element, .. } = m {
                self.listen(ui, element, out);
            }
        }
    }

    /// The page goes.
    // D2-02890
    pub fn close(self, ui: &mut Ui) {
        ui.drop_frame(self.frame);
    }
}
