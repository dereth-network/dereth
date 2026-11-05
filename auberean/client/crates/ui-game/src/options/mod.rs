//! The options window: the toolbar's window of the game's commands and options.
//!
//! [`OptionWindow`] is the window. It holds its pages, each behind its tab:
//!
//! - **Commands** (the first open): leave the world, quit the game (each asked first; while a
//!   question is up the commands are ghosted) and the assistance page's web site;
//! - **Character** ([`character`]): the player's own options, kept on the server;
//! - **Client** ([`client`]): the client's own preferences, kept on the computer;
//! - **Chat** ([`chat`]): the chat windows' options, kept with the player's options;
//! - **Assistance** ([`assistance`]): a call for help (its tab is off the window in this client).
//!
//! Shown, it comes to the front of the toolbar's windows; hidden with an option changed and not
//! applied, it asks whether to apply the changes (yes applies them, no puts back what was
//! applied); the commands' questions go with it. Its close button hides it, its help button opens
//! the help at its page, its toolbar button shows and hides it.
//!
//! The rows of the pages are [`helper`]'s; what each row keeps, [`preference`]; the client's
//! preferences behind the client page, [`bank`].

pub mod assistance;
pub mod bank;
pub mod character;
pub mod chat;
pub mod client;
pub mod helper;
pub mod preference;

use std::any::Any;
use std::sync::atomic::{AtomicU32, Ordering};

use auberean_ui::framework::FrameId;
use auberean_ui::token::{attr, msg};
use auberean_ui::{Handle, Mail, Token, Ui, Value};

use crate::examination::{window_to_front, BRING_TO_FRONT, WINDOW_FIELDS};
use crate::gameplay::{self, dialog, Child, Dialog, Notice, View};

/// The window's parts.
pub mod name {
    use auberean_ui::Token;
    pub const FIELD: Token = Token::of("OptionField");
    pub const CLOSE_BUTTON: Token = Token::of("OptionCloseButton");
    pub const HELP_BUTTON: Token = Token::of("OptionsHelpButton");
    pub const LEAVE_BUTTON: Token = Token::of("LeaveButton");
    pub const QUIT_BUTTON: Token = Token::of("QuitButton");
    pub const KEYBOARD_BUTTON: Token = Token::of("KeyboardButton");
    pub const ASSISTANCE_BUTTON: Token = Token::of("AssistanceButton");
    pub const OPTION_BUTTON: Token = Token::of("OptionButton");
    pub const COMMAND_TAB: Token = Token::of("CommandTab");
    pub const COMMAND_PAGE: Token = Token::of("CommandPage");
    /// A page's buttons.
    pub const APPLY_BUTTON: Token = Token::of("ApplyButton");
    pub const RECALL_BUTTON: Token = Token::of("RecallButton");
    pub const DEFAULT_BUTTON: Token = Token::of("DefaultButton");
    /// A menu's ways of opening.
    pub const OPEN_DOWN: Token = Token::of("open_down");
    pub const OPEN_UP: Token = Token::of("open_up");
}

/// A page's tab's messages.
pub mod page {
    pub use auberean_ui::types::panel::page::{CLOSED_MSG as CLOSED, OPENED};
}

/// The notices the window and its pages hear (the client's own, between its panels).
pub mod notice {
    /// A question answered: the dialog, yes (1) or no.
    pub const CLOSE_CONFIRMATION: u32 = 0x436;
    /// A message read: the dialog.
    pub const CLOSE_MESSAGE: u32 = 0x450;
    /// The shortcut bar resized by hand to its rows.
    pub const SET_SHORTCUT_HEIGHT: u32 = 0x49C;
    /// Leave the world (the play screen ends the session), quit the game.
    // D2-00311
    pub const LEAVE_WORLD: u32 = 0x444;
    // D2-00318
    pub const QUIT: u32 = 0x445;
    /// The options window changed the player's options.
    pub const OPTIONS_CHANGED: u32 = 0x46D;
}

/// The player's option bit for the bandwidth graph over the network indicator.
pub const LATENCY_GRAPH: u32 = 0x4000;

/// The player's options as the client last read them, for the character page.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Values {
    /// The yes-or-no options, by bit.
    pub bits: u32,
    /// The radar's filters, by bit.
    pub radar_mask: u32,
    /// The shortcut bar's rows.
    pub shortcut_height: u32,
    /// How far away others' damage shows.
    pub damage_range: f32,
    /// How often the client has read them (each reading a page shows again).
    pub read: u32,
}

/// What the options window asks of the client.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// Set the player's options (each by its label on the character page, with its value: 1 or
    /// 0, a float's bits, a number of rows) in order; with `tell`, the other panels told; the
    /// options sent to the server to keep.
    Set {
        changes: Vec<(u32, u32)>,
        tell: bool,
    },
    /// Open the web page where players asked for urgent help.
    AssistanceSite,
    /// Call for help: the kind of call, the one it is about, what happened.
    Assistance {
        kind: u32,
        abuser: String,
        description: String,
    },
    /// The client's own preferences changed, applied, kept in its preferences' file, or put back
    /// (see [`client`]).
    Preferences(client::Request),
}

/// The numbers the window's questions are known by (apart from the play screen's own).
static DIALOGS: AtomicU32 = AtomicU32::new(0x2000_0000);

fn new_dialog_id() -> u32 {
    DIALOGS.fetch_add(1, Ordering::Relaxed) + 1
}

/// The options window.
#[derive(Debug)]
pub struct OptionWindow {
    frame: FrameId,
    field: Option<Handle>,
    leave: Option<Handle>,
    quit: Option<Handle>,
    assistance_button: Option<Handle>,
    pub client: Option<client::ClientPage>,
    pub character: Option<character::CharacterPage>,
    pub chat: Option<chat::ChatOptionsPage>,
    pub assistance: Option<assistance::AssistancePage>,
    /// The questions up: apply the changes, quit, leave the world (0: none).
    confirm: u32,
    quit_context: u32,
    logout_context: u32,
    /// The client asked for the client page's preferences.
    attached: bool,
    /// The client's preferences' listing the client page last took in.
    listed: u32,
}

impl OptionWindow {
    /// The window under the toolbar's framework: its pages made (client, character, chat,
    /// assistance), its field and command buttons found, its buttons, tabs and the windows'
    /// fields heard; hidden.
    // D2-02809
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let client = client::ClientPage::new(ui, frame);
        let character = character::CharacterPage::new(ui, frame);
        let chat = chat::ChatOptionsPage::new(ui, frame);
        let assistance = assistance::AssistancePage::new(ui, frame);
        let field = ui.element(name::FIELD);
        let find = |ui: &Ui, n| field.and_then(|f| ui.child_recursive(f, n));
        let leave = find(ui, name::LEAVE_BUTTON);
        let quit = find(ui, name::QUIT_BUTTON);
        let assistance_button = find(ui, name::ASSISTANCE_BUTTON);
        let me = Self {
            frame,
            field,
            leave,
            quit,
            assistance_button,
            client: Some(client),
            character: Some(character),
            chat: Some(chat),
            assistance: Some(assistance),
            confirm: 0,
            quit_context: 0,
            logout_context: 0,
            attached: false,
            listed: 0,
        };
        if field.is_none() || leave.is_none() || quit.is_none() || assistance_button.is_none() {
            return me;
        }
        if let Some(l) = ui.frame_listener(frame) {
            for b in [
                name::CLOSE_BUTTON,
                name::HELP_BUTTON,
                name::LEAVE_BUTTON,
                name::QUIT_BUTTON,
                name::KEYBOARD_BUTTON,
                name::ASSISTANCE_BUTTON,
            ] {
                ui.listen_for_message(l, b, msg::BUTTON_RELEASE_ON);
            }
            ui.listen_for_message(l, name::COMMAND_TAB, page::OPENED);
            ui.listen_for_message(l, name::COMMAND_TAB, page::CLOSED);
            ui.listen_for_message(l, name::OPTION_BUTTON, msg::BUTTON_RELEASE_ON);
            for f in WINDOW_FIELDS {
                ui.listen_for_message(l, Token::of(f), BRING_TO_FRONT);
            }
            ui.listen_for_message(l, helper::name::DROP_DOWN_MENU, msg::MOUSE_OVER);
        }
        ui.frame_show(frame, false);
        if let Some(f) = field {
            ui.set_visible(f, false);
        }
        me.update_buttons(ui);
        me
    }

    /// The commands ghosted while the question to quit or to leave is up.
    // D2-02810
    pub fn update_buttons(&self, ui: &mut Ui) {
        let (Some(l), Some(q), Some(a)) = (self.leave, self.quit, self.assistance_button) else {
            return;
        };
        let up = self.quit_context != 0 || self.logout_context != 0;
        for b in [l, q, a] {
            ui.set_attr(b, attr::GHOSTED, Value::Bool(up));
        }
    }

    fn question(ui: &Ui, text: &str, out: &mut Vec<gameplay::Request>) -> u32 {
        let id = new_dialog_id();
        out.push(gameplay::Request::Dialog(Dialog {
            id,
            kind: dialog::CONFIRMATION,
            text: crate::flow::message(ui, helper::OPTION_STRINGS, text),
            modal: false,
            info: None,
        }));
        id
    }

    /// Ask whether to apply the changes.
    // D2-02811
    pub fn make_confirm_dialog(&mut self, ui: &Ui, out: &mut Vec<gameplay::Request>) {
        self.confirm = Self::question(ui, "ID_Option_ConfirmText", out);
    }

    /// The changes answered: yes applies every page's, no puts back what was applied.
    // D2-02812
    pub fn do_confirm(&mut self, ui: &mut Ui, yes: bool, out: &mut Vec<gameplay::Request>) {
        self.confirm = 0;
        if yes {
            if let Some(c) = &mut self.client {
                c.apply(ui, out);
            }
            if let Some(c) = &mut self.character {
                c.apply(ui, out);
            }
            if let Some(c) = &mut self.chat {
                c.apply(ui, out);
            }
        } else {
            if let Some(c) = &mut self.client {
                c.recall(ui);
            }
            if let Some(c) = &mut self.character {
                c.recall(ui);
            }
            if let Some(c) = &mut self.chat {
                c.recall(ui);
            }
        }
    }

    /// Ask whether to quit; the commands ghosted.
    // D2-02813
    pub fn make_quit_dialog(&mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        self.quit_context = Self::question(ui, "ID_Option_QuitText", out);
        self.update_buttons(ui);
    }

    /// Ask whether to leave the world; the commands ghosted.
    // D2-02814
    pub fn make_logout_dialog(&mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        self.logout_context = Self::question(ui, "ID_Option_LogoutText", out);
        self.update_buttons(ui);
    }

    /// A question taken down; the commands' ghosting.
    // D2-02815
    fn close_dialog(&self, ui: &mut Ui, id: u32, out: &mut Vec<gameplay::Request>) {
        if id != 0 {
            out.push(gameplay::Request::DialogDown(id));
        }
        self.update_buttons(ui);
    }

    fn close_quit_and_logout(&mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        let (q, l) = (self.quit_context, self.logout_context);
        self.quit_context = 0;
        self.close_dialog(ui, q, out);
        self.logout_context = 0;
        self.close_dialog(ui, l, out);
    }

    /// A question answered: the changes' (see [`Self::do_confirm`]); to quit (yes: the game
    /// ends); to leave the world (yes: the session ends).
    // D2-02818
    pub fn close_confirmation_dialog(
        &mut self,
        ui: &mut Ui,
        id: u32,
        yes: bool,
        out: &mut Vec<gameplay::Request>,
    ) {
        if id == 0 {
            return;
        }
        if id == self.confirm {
            self.do_confirm(ui, yes, out);
        } else if id == self.quit_context {
            self.quit_context = 0;
            self.update_buttons(ui);
            if yes {
                out.push(gameplay::Request::Notice(Notice {
                    native: notice::QUIT,
                    ..Notice::default()
                }));
            }
        } else if id == self.logout_context {
            self.logout_context = 0;
            self.update_buttons(ui);
            if yes {
                out.push(gameplay::Request::Notice(Notice {
                    native: notice::LEAVE_WORLD,
                    ..Notice::default()
                }));
            }
        }
    }

    /// Whether a page has an option changed and not applied.
    fn changed(&self, ui: &Ui) -> bool {
        self.character.as_ref().is_some_and(|c| c.changed(ui))
            || self.client.as_ref().is_some_and(|c| c.changed(ui))
            || self.chat.as_ref().is_some_and(|c| c.changed(ui))
    }

    /// What the window heard: a window come to the front; a menu of its pages opening, upward
    /// when it sits below the window's middle; the close, help and toolbar buttons; the commands;
    /// the commands' tab (its questions taken down; opened, the page laid out).
    // D2-02820
    fn listen_window(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<gameplay::Request>) {
        let Mail::Element {
            element,
            from,
            what,
            ..
        } = *mail
        else {
            return;
        };
        if what == BRING_TO_FRONT {
            window_to_front(ui, self.field, Some(from));
        }
        if element == helper::name::DROP_DOWN_MENU {
            let (Some(f), Some(m)) = (
                self.field.and_then(|f| ui.screen_rect(f)),
                ui.screen_rect(from),
            ) else {
                return;
            };
            let middle = |r: auberean_ui::Rect| (r.y0 + r.y0 + (r.y1 - r.y0)) / 2;
            let down = middle(m) < middle(f);
            ui.set_attr(from, name::OPEN_DOWN, Value::Bool(down));
            ui.set_attr(from, name::OPEN_UP, Value::Bool(!down));
            return;
        }
        if element == name::CLOSE_BUTTON {
            self.show(ui, false, out);
        } else if element == name::HELP_BUTTON {
            out.push(gameplay::Request::Help { open: true });
        } else if element == name::OPTION_BUTTON {
            let shown = ui.frame_shown(self.frame);
            self.show(ui, !shown, out);
        } else if element == name::LEAVE_BUTTON {
            self.make_logout_dialog(ui, out);
        } else if element == name::QUIT_BUTTON {
            self.make_quit_dialog(ui, out);
        } else if element == name::ASSISTANCE_BUTTON {
            out.push(gameplay::Request::Options(Request::AssistanceSite));
        } else {
            if element == name::COMMAND_TAB {
                self.close_quit_and_logout(ui, out);
            }
            if what == page::OPENED {
                if let Some(p) = ui.element(name::COMMAND_PAGE) {
                    ui.update(p);
                }
            }
        }
    }
}

impl OptionWindow {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x436, 0x49C, 0x417, 0x48B, 0x468, 0x450];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for OptionWindow {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown or hidden: to the front of the toolbar's windows, its field with it; hidden with
    /// changes not applied, the question whether to apply them; the commands' questions down.
    // D2-02817
    fn show(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<gameplay::Request>) {
        let was = ui.frame_shown(self.frame);
        ui.frame_show(self.frame, on);
        let Some(f) = self.field else { return };
        ui.broadcast(f, BRING_TO_FRONT, 0);
        let shown = ui.frame_shown(self.frame);
        ui.set_visible(f, shown);
        if !shown && was {
            if self.changed(ui) {
                self.make_confirm_dialog(ui, out);
            }
            self.close_quit_and_logout(ui, out);
        }
    }

    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<gameplay::Request>) {
        self.listen_window(ui, mail, out);
    }

    /// The questions answered (the window's and its pages'); the shortcut bar's rows; a message
    /// read.
    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<gameplay::Request>) {
        match n.native {
            notice::CLOSE_CONFIRMATION => {
                let (id, yes) = (n.word_at(0), n.word_at(1) != 0);
                self.close_confirmation_dialog(ui, id, yes, out);
                if let Some(c) = &mut self.character {
                    c.close_confirmation_dialog(ui, id, yes, out);
                }
                if let Some(c) = &mut self.client {
                    c.close_confirmation_dialog(ui, id, yes, out);
                }
                if let Some(c) = &mut self.chat {
                    c.close_confirmation_dialog(ui, id, yes, out);
                }
            }
            notice::CLOSE_MESSAGE => {
                if let Some(a) = &mut self.assistance {
                    a.close_message_dialog(n.word_at(0));
                }
            }
            notice::SET_SHORTCUT_HEIGHT => {
                if let Some(c) = &mut self.character {
                    #[allow(clippy::cast_possible_wrap)]
                    c.set_shortcut_height(ui, n.word_at(0) as i32, out);
                }
            }
            _ => {}
        }
    }

    /// The pages' mail, and the player's options for the character and chat pages.
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<gameplay::Request>) {
        if let Some(c) = &mut self.client {
            if !self.attached {
                out.push(c.attach());
                self.attached = true;
            }
            c.hear(ui, out);
            if let Some(l) = view.preferences.filter(|l| l.serial != self.listed) {
                for (section, prefs) in &l.sections {
                    c.monitor_reload(ui, *section, prefs);
                }
                self.listed = l.serial;
            }
            for change in view.preferences_changed {
                c.monitor_change(ui, change);
            }
        }
        if let Some(c) = &mut self.character {
            c.hear(ui, out);
            c.update(ui, view.options.as_ref());
        }
        if let Some(c) = &mut self.chat {
            c.hear(ui, out);
            c.update(ui, view.chat, view.chat_defaults);
        }
        if let Some(a) = &mut self.assistance {
            a.hear(ui, out);
        }
    }

    /// The window goes: its pages, its questions.
    // D2-02816, D2-02819
    fn close(mut self: Box<Self>, ui: &mut Ui) {
        let mut out = Vec::new();
        if let Some(c) = self.client.take() {
            c.close(ui, &mut out);
        }
        if let Some(c) = self.character.take() {
            c.close(ui, &mut out);
        }
        if let Some(c) = self.chat.take() {
            c.close(ui, &mut out);
        }
        if let Some(a) = self.assistance.take() {
            a.close(ui);
        }
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
