//! The screen that says why the session ended (the account's trouble, the server gone, the
//! connection failed): the reason the flow was asked with, and a button that leaves the game.

use std::any::Any;

use auberean_ui::framework::FrameId;
use auberean_ui::token::msg;
use auberean_ui::{Handle, Mail, Ui};

use crate::pregame::{self, Data, Request, Screen};

/// The layout the screen is built from, by enumeration value.
pub const LAYOUT_ENUM: u32 = 0x4100_0005;

mod name {
    use auberean_ui::Token;
    pub const DISCONNECTED_FIELD: Token = Token::of("DisconnectedField");
    pub const ERROR_TEXT: Token = Token::of("ErrorText");
    pub const QUIT_BUTTON: Token = Token::of("QuitButton");
}

/// The disconnected screen.
#[derive(Debug)]
pub struct Disconnected {
    frame: FrameId,
    error_text: Option<Handle>,
}

impl Disconnected {
    /// The screen made: its field, its reason's line, its button heard.
    // D2-01724, D2-01725, D2-01728
    pub fn new(ui: &mut Ui) -> Self {
        let frame = ui.new_main_frame();
        let field = ui.frame_create_root(frame, LAYOUT_ENUM, name::DISCONNECTED_FIELD);
        let error_text = field.and_then(|f| ui.child_recursive(f, name::ERROR_TEXT));
        if error_text.is_some() {
            pregame::listen(ui, frame, name::QUIT_BUTTON, msg::BUTTON_RELEASE_ON);
        }
        Self { frame, error_text }
    }

    /// The reason shown.
    #[must_use]
    pub fn error_text(&self, ui: &Ui) -> Option<String> {
        self.error_text.and_then(|e| ui.text(e))
    }
}

impl Screen for Disconnected {
    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Quit leaves the game.
    // D2-01726
    fn tick(&mut self, ui: &mut Ui, _data: &mut Data, out: &mut Vec<Request>) {
        for m in pregame::take(ui, self.frame) {
            if let Mail::Element { element, .. } = m {
                if element == name::QUIT_BUTTON {
                    out.push(Request::Quit);
                }
            }
        }
    }

    /// Why the session ended.
    // D2-01729
    fn set_error_msg(&mut self, ui: &mut Ui, text: &str) {
        if let Some(e) = self.error_text {
            ui.set_text(e, text);
        }
    }

    /// The field goes with the screen.
    // D2-01727
    fn close(self: Box<Self>, ui: &mut Ui, _data: &mut Data, _out: &mut Vec<Request>) {
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
