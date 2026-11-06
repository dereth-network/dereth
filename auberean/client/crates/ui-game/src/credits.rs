//! The credits: a screen of their own that scrolls the credits' text up from the foot of the
//! screen while pictures come and go and the credits' songs play one after another, and ends by
//! itself when the last line has scrolled off, or at once when an interface key is pressed
//! (Escape; a right press, which the player's keys bind to looking with the mouse). A left press
//! does nothing. Ended, please wait goes up and the character list comes back.

use std::any::Any;

use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::system::Handle;
use auberean_ui::text::measure;
use auberean_ui::token::attr;
use auberean_ui::{Listener, Mail, Token, Ui};

use crate::flow::mode;
use crate::pregame::{self, Data, Screen};

/// The layout the screen is built from, by enumeration value (record `0x22000022`).
pub const LAYOUT_ENUM: u32 = 0x4100_000E;

mod name {
    use auberean_ui::Token;
    pub const PICTURE_FIELD: Token = Token::of("PictureField");
    pub const ALT_PICTURE_FIELD: Token = Token::of("AltPictureField");
    pub const TEXT_FIELD: Token = Token::of("TextField");
    pub const ALT_TEXT_FIELD: Token = Token::of("AltTextField");
    pub const TEXTAREA: Token = Token::of("textarea");
    pub const STRINGTABLE: Token = Token::of("stringtable");
    pub const DURATION: Token = Token::of("duration");
    pub const FADING: Token = Token::of("fading");
}

/// What the screen asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// The credits are over: back to the character list.
    Close,
}

/// The credits screen.
#[derive(Debug)]
pub struct Credits {
    frame: FrameId,
    picture: Handle,
    text_field: Handle,
    text_area: Option<Handle>,
    pictures: Vec<u32>,
    xs: Vec<i32>,
    ys: Vec<i32>,
    last_picture: usize,
    last_x: usize,
    last_y: usize,
    /// The song before the credits has been stopped.
    silenced: bool,
    /// How long the text takes to scroll through, and when it is over.
    duration: f32,
    end: f64,
    paper_height: i32,
    closed: bool,
    /// The credits' songs played so far, and please wait.
    last_sound: u32,
    wait_context: u32,
}

/// The next of a list, going round (the first shown is the one after the first in the list).
fn next<T: Copy>(list: &[T], last: &mut usize) -> Option<T> {
    if list.is_empty() {
        return None;
    }
    *last = (*last + 1) % list.len();
    list.get(*last).copied()
}

impl Credits {
    /// Build the screen (the other set of fields with `alternate`, the client's Ctrl and Alt held
    /// without Shift) and start it at the interface's clock, the keys and the clock heard.
    // D2-01788, D2-01789, D2-01790
    pub fn open(ui: &mut Ui, alternate: bool) -> Option<Self> {
        let frame = ui.new_main_frame();
        let (p, t) = if alternate {
            (name::ALT_PICTURE_FIELD, name::ALT_TEXT_FIELD)
        } else {
            (name::PICTURE_FIELD, name::TEXT_FIELD)
        };
        let made = ui
            .frame_create_root(frame, LAYOUT_ENUM, p)
            .zip(ui.frame_create_root(frame, LAYOUT_ENUM, t));
        let Some((picture, text_field)) = made else {
            ui.drop_frame(frame);
            return None;
        };
        let text_area = ui
            .node(text_field)
            .and_then(|n| n.id_attr(name::TEXTAREA))
            .and_then(|id| ui.child_recursive(text_field, Token(id)));
        let now = ui.now();
        let mut me = Self {
            frame,
            picture,
            text_field,
            text_area,
            pictures: Vec::new(),
            xs: Vec::new(),
            ys: Vec::new(),
            last_picture: 0,
            last_x: 0,
            last_y: 0,
            silenced: false,
            duration: 0.0,
            end: now,
            paper_height: 0,
            closed: false,
            last_sound: 0,
            wait_context: 0,
        };
        me.initialize(ui, now);
        Some(me)
    }

    /// The keys and the clock heard; the text: every credits entry of the field's string table in
    /// order (each written as the client writes a string reference), laid out as wide as the
    /// field and as tall as its lines; the first picture; the time the text takes (from the
    /// field's duration, a screen and a line's worth of scrolling) and when it ends. The pictures
    /// and their places come from the picture field's numbered attributes.
    // D2-01787
    fn initialize(&mut self, ui: &mut Ui, now: f64) {
        pregame::listen_global(ui, self.frame, global::KEY_DOWN);
        pregame::listen_global(ui, self.frame, global::TIME);
        let Some(area) = self.text_area else { return };
        let table = ui
            .node(self.text_field)
            .and_then(|n| n.id_attr(name::STRINGTABLE))
            .unwrap_or(0);
        let mut text = String::new();
        let mut entries = 1u32;
        while let Some(s) = ui
            .assets()
            .text(table, Token::of(&format!("ID_Credits{entries}")).raw())
        {
            text.push_str(&s);
            entries += 1;
        }
        let field = ui.node(self.text_field).map(|n| n.rect).unwrap_or_default();
        let (fw, fh) = (field.width(), field.height());
        ui.set_text(area, &text);
        let font = ui
            .node(area)
            .and_then(|n| n.id_attr(attr::FONT))
            .and_then(|f| ui.assets().font(f));
        self.paper_height = font.map_or(0, |f| {
            measure(&f, &text, fw, false).iter().map(|l| l.height).sum()
        });
        ui.move_to(area, 0, fh);
        ui.resize_to(area, fw, self.paper_height.max(1));
        self.update_picture(ui);
        let duration = ui
            .node(self.text_field)
            .and_then(|n| n.float_attr(name::DURATION))
            .unwrap_or(1.0);
        #[allow(clippy::cast_precision_loss)]
        let (paper, screen, n) = (self.paper_height as f32, fh as f32, entries as f32);
        self.duration = (paper + screen) / ((paper / n + screen) / duration);
        self.end = now + f64::from(self.duration);
        let picture = self.picture;
        for (prefix, list) in [("x_", &mut self.xs), ("y_", &mut self.ys)] {
            let mut i = 0;
            while let Some(v) = ui
                .node(picture)
                .and_then(|n| n.int_attr(Token::of(&format!("{prefix}{i}"))))
            {
                list.push(v);
                i += 1;
            }
        }
        let mut i = 0;
        while let Some(v) = ui
            .node(picture)
            .and_then(|n| n.id_attr(Token::of(&format!("ID_Picture{i}"))))
        {
            self.pictures.push(v);
            i += 1;
        }
    }

    // D2-01777
    fn x(&mut self) -> i32 {
        next(&self.xs, &mut self.last_x).unwrap_or(0)
    }

    // D2-01778
    fn y(&mut self) -> i32 {
        next(&self.ys, &mut self.last_y).unwrap_or(0)
    }

    /// When the picture is not fading, the next picture comes up at the next place, at its own
    /// size, and starts fading.
    // D2-01779
    fn update_picture(&mut self, ui: &mut Ui) {
        if ui
            .node(self.picture)
            .is_some_and(|n| n.state == name::FADING)
        {
            return;
        }
        let Some(p) = next(&self.pictures, &mut self.last_picture) else {
            return;
        };
        if p == 0 {
            return;
        }
        let y = self.y();
        let x = self.x();
        ui.move_to(self.picture, x, y);
        ui.set_image(self.picture, p, 0);
        if let Some(img) = ui.assets().image(p) {
            let (w, h) = (
                i32::try_from(img.width).unwrap_or(0),
                i32::try_from(img.height).unwrap_or(0),
            );
            ui.resize_to(self.picture, w, h);
        }
        ui.set_state(self.picture, name::FADING);
    }

    /// The next song: the picture field's numbered song after the last played, going round to the
    /// first when the numbers run out.
    // D2-01782
    fn sound_id(&mut self, ui: &Ui) -> u32 {
        let id = ui
            .node(self.picture)
            .and_then(|n| n.id_attr(Token::of(&format!("ID_Sound{}", self.last_sound))))
            .unwrap_or(0);
        if id == 0 && self.last_sound != 0 {
            self.last_sound = 0;
            return self.sound_id(ui);
        }
        id
    }

    /// When no song plays (and the client plays them), the next one.
    // D2-01783
    fn update_sound(&mut self, ui: &Ui, data: &Data, out: &mut Vec<pregame::Request>) {
        if data.sound_playing != Some(false) {
            return;
        }
        let id = self.sound_id(ui);
        self.last_sound += 1;
        out.push(pregame::Request::Sound(id));
    }

    /// The text's place by the time: from just under the screen's foot at the start to wholly
    /// above its top at the end; at the end the credits close.
    // D2-01784
    fn update_text(&mut self, ui: &mut Ui, now: f64) -> Option<Request> {
        let area = self.text_area?;
        #[allow(clippy::cast_possible_truncation)]
        let progress = if self.duration == 0.0 {
            1.0
        } else {
            (1.0 - (self.end - now) / f64::from(self.duration)) as f32
        }
        .clamp(0.0, 1.0);
        let field = ui.node(self.text_field).map(|n| n.rect).unwrap_or_default();
        #[allow(clippy::cast_precision_loss)]
        let travel = (field.height() + self.paper_height) as f32;
        let y = field.height() - dereth_primitives::num::to_i32(progress * travel);
        let x = ui.node(area).map_or(0, |n| n.rect.x0);
        ui.move_to(area, x, y);
        (progress >= 1.0).then(|| self.close())
    }

    /// One frame: the picture, the song, then the text.
    // D2-01786
    pub fn update(
        &mut self,
        ui: &mut Ui,
        data: &Data,
        now: f64,
        out: &mut Vec<pregame::Request>,
    ) -> Option<Request> {
        if self.closed {
            return None;
        }
        self.update_picture(ui);
        self.update_sound(ui, data, out);
        self.update_text(ui, now)
    }

    /// The credits end.
    fn close(&mut self) -> Request {
        self.closed = true;
        Request::Close
    }

    // D2-01775
    fn make_please_wait_dialog(&mut self, ui: &Ui, out: &mut Vec<pregame::Request>) {
        self.wait_context = pregame::please_wait(ui, self.frame, out);
    }

    /// The credits closed: the keys and the clock no longer heard, please wait, back to the list.
    // D2-01781
    fn leave(&mut self, ui: &mut Ui, out: &mut Vec<pregame::Request>) {
        if let Some(l) = ui.frame_listener(self.frame) {
            let me = Listener::Other(l);
            ui.unregister_for_global_message(me, global::KEY_DOWN);
            ui.unregister_for_global_message(me, global::TIME);
        }
        self.make_please_wait_dialog(ui, out);
        pregame::queue(mode::CHARACTER_SELECTION, out);
    }

    /// Where the text stands, from the field's top (for a check).
    #[must_use]
    pub fn text_top(&self, ui: &Ui) -> Option<i32> {
        self.text_area.and_then(|a| ui.node(a)).map(|n| n.rect.y0)
    }

    /// How long the credits run, in seconds.
    #[must_use]
    pub const fn duration(&self) -> f32 {
        self.duration
    }

    /// Whether the credits have ended.
    #[must_use]
    pub const fn closed(&self) -> bool {
        self.closed
    }
}

impl Screen for Credits {
    fn frame(&self) -> FrameId {
        self.frame
    }

    /// A key pressed ends the credits; the clock moves them on (and ends them at their end).
    // D2-01785
    fn tick(&mut self, ui: &mut Ui, data: &mut Data, out: &mut Vec<pregame::Request>) {
        // The song playing as the credits open stops for theirs.
        // D2-01787
        if !self.silenced {
            self.silenced = true;
            out.push(pregame::Request::Silence);
        }
        for m in pregame::take(ui, self.frame) {
            let ended = match m {
                Mail::Global {
                    what: global::KEY_DOWN,
                    ..
                } if !self.closed => Some(self.close()),
                Mail::Global {
                    what: global::TIME, ..
                } => {
                    let now = ui.now();
                    self.update(ui, data, now, out)
                }
                _ => None,
            };
            if ended == Some(Request::Close) {
                self.leave(ui, out);
            }
        }
    }

    /// The screen goes: its fields, its please wait.
    // D2-01776, D2-01780
    fn close(self: Box<Self>, ui: &mut Ui, _data: &mut Data, out: &mut Vec<pregame::Request>) {
        ui.drop_frame(self.frame);
        pregame::dialog_down(self.wait_context, out);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
