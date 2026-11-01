//! The opening pictures: the screen the game starts on. Its field shows the opening pictures one
//! after another and says when they are done; then, or at once on a key or a press let go, the
//! game goes on to connecting ([`crate::datapatch`]).

use std::any::Any;

use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::{Handle, Mail, Token, Ui};

use crate::flow::mode;
use crate::pregame::{self, Data, Request, Screen};

/// The layout the screen is built from, by enumeration value.
pub const LAYOUT_ENUM: u32 = 0x4100_0008;

mod name {
    use auberean_ui::Token;
    pub const INTRO_FIELD: Token = Token::of("IntroField");
    /// What the field says when its pictures are done.
    pub const INTRO_STOP: Token = Token::of("intro_stop");
    /// The music a screen plays, on its field.
    pub const MUSIC: Token = Token::of("music");
}

/// The opening pictures' screen.
#[derive(Debug)]
pub struct Intro {
    frame: FrameId,
    field: Option<Handle>,
}

/// The screens' music: the sound playing across them stopped and the field's own played.
pub(crate) fn play_music(ui: &Ui, field: Option<Handle>, music: Token, out: &mut Vec<Request>) {
    let Some(id) = field
        .and_then(|f| ui.node(f))
        .and_then(|n| n.id_attr(music))
    else {
        return;
    };
    out.push(Request::Silence);
    out.push(Request::Sound(id));
}

impl Intro {
    /// The screen made: its field, its music, and the field's end, any key and any press let go
    /// heard.
    // D2-01717, D2-01719, D2-01720
    pub fn new(ui: &mut Ui, out: &mut Vec<Request>) -> Self {
        let frame = ui.new_main_frame();
        let field = ui.frame_create_root(frame, LAYOUT_ENUM, name::INTRO_FIELD);
        play_music(ui, field, name::MUSIC, out);
        pregame::listen(ui, frame, name::INTRO_FIELD, name::INTRO_STOP);
        pregame::listen_global(ui, frame, global::KEY_DOWN);
        pregame::listen_global(ui, frame, global::MOUSE_UP);
        Self { frame, field }
    }

    /// On to connecting.
    // D2-01721
    fn close_intro(out: &mut Vec<Request>) {
        pregame::queue(mode::DATA_PATCH, out);
    }

    /// The screen's field, while it is made.
    #[must_use]
    pub const fn field(&self) -> Option<Handle> {
        self.field
    }
}

impl Screen for Intro {
    fn frame(&self) -> FrameId {
        self.frame
    }

    /// The field's pictures done; a key pressed, a press let go: on to connecting.
    // D2-01722, D2-01723
    fn tick(&mut self, ui: &mut Ui, _data: &mut Data, out: &mut Vec<Request>) {
        for m in pregame::take(ui, self.frame) {
            match m {
                Mail::Element { element, what, .. }
                    if element == name::INTRO_FIELD && what == name::INTRO_STOP =>
                {
                    Self::close_intro(out);
                }
                Mail::Global {
                    what: global::KEY_DOWN | global::MOUSE_UP,
                    ..
                } => Self::close_intro(out),
                _ => {}
            }
        }
    }

    /// The field goes with the screen.
    // D2-01718
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
