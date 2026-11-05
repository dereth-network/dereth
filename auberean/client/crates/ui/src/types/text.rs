//! Text elements: a glyph list in the element's font and color, broken into lines inside its
//! margins and drawn from a view that scrolls; and, for an editable one, a caret that blinks,
//! typed characters and editing keys, a selection, cutting, copying and pasting.
//!
//! Text reaches an element as written strings or as string references, each in one of the
//! element's numbered fonts and colors (`font<n>`, `font_color<n>_red` ...), added at the caret.
//! Each is queued, in order, until the text it names is at hand: a written string at once, a
//! reference once the string tables it reads that the data lacked have come (or been refused:
//! it then shows the client's error message), when it is written and added. A string waits for
//! those queued before it.

use std::sync::Arc;

use crate::draw::Surface;
use crate::manager::global;
use crate::system::{Handle, Ui, Value};
use crate::text::{self, Color, GlyphRun, Justify, Line};
use crate::token::{attr, Token};
use crate::types::button_id;
use crate::Rect;

/// What a text element is: editable, one line, selectable, centered vertically, without the
/// input method's window.
pub mod bits {
    pub const EDITABLE: u32 = 0x1;
    pub const ONE_LINE: u32 = 0x2;
    pub const SELECTABLE: u32 = 0x4;
    pub const CENTER_VERTICALLY: u32 = 0x8;
    pub const NO_IME: u32 = 0x10;
}

/// The fonts a text element falls back to for characters its own font lacks, in order: their
/// values in the mapping of fonts (the whole of Unicode first).
const FALLBACK_FONTS: [u32; 2] = [0x4000_0004, 0x4000_0005];
/// The enumeration mapping that names the fallback fonts.
const FONT_MAPPING: u32 = 0x4000_000B;

mod name {
    use crate::Token;
    pub const FONT_RESET: Token = Token::of("font_reset");
    pub const NO_IME: Token = Token::of("no_IME");
    pub const TRIM_FROM_TOP: Token = Token::of("trim_from_top");
    pub const TEXT_LOCKED: Token = Token::of("text_locked");
    pub const RESIZE_HEIGHT_TO_TEXT: Token = Token::of("resizeheighttotext");
    pub const RESIZE_WIDTH_TO_TEXT: Token = Token::of("resizewidthtotext");
    pub const VERTICAL_SCROLLBAR: Token = Token::of("vertical_scrollbar");
    pub const HORIZONTAL_SCROLLBAR: Token = Token::of("horizontal_scrollbar");
    pub const UNIQUE_MSG: Token = Token::of("unique_msg");
    pub const DISABLED: Token = Token::of("disabled");
    pub const ALLOW_UPDATING: Token = Token::of("allow_updating");
    /// A menu showing its choice in this element.
    pub const DISPLAY_SELECTION: Token = Token::of("display_selection");
}

/// What a text element says.
pub mod text_msg {
    use crate::Token;
    pub const ACTIVATED: Token = Token::of("activated");
    pub const DEACTIVATED: Token = Token::of("deactivated");
    /// A character typed (the parameter), and another key pressed.
    pub const CHARACTER_INPUT: Token = Token::of("character_input");
    pub const KEY_INPUT: Token = Token::of("key_input");
    /// Queued strings were added.
    pub const DOWNLOAD_DONE: Token = Token::of("download_done");
}

/// What a scroll bar says to the element it scrolls.
mod bar_msg {
    use crate::Token;
    pub const NEW_POSITION: Token = Token::of("new_position");
    pub const VERTICAL_LINE_INCREMENT: Token = Token::of("vertical_line_increment");
    pub const VERTICAL_LINE_DECREMENT: Token = Token::of("vertical_line_decrement");
    pub const VERTICAL_PAGE_INCREMENT: Token = Token::of("vertical_page_increment");
    pub const VERTICAL_PAGE_DECREMENT: Token = Token::of("vertical_page_decrement");
    pub const HORIZONTAL_LINE_INCREMENT: Token = Token::of("horizontal_line_increment");
    pub const HORIZONTAL_LINE_DECREMENT: Token = Token::of("horizontal_line_decrement");
    pub const HORIZONTAL_PAGE_INCREMENT: Token = Token::of("horizontal_page_increment");
    pub const HORIZONTAL_PAGE_DECREMENT: Token = Token::of("horizontal_page_decrement");
}

/// Keys a text element takes as typed: the host's key codes for them.
pub mod key {
    pub const PAGE_UP: u16 = 0x21;
    pub const PAGE_DOWN: u16 = 0x22;
    pub const END: u16 = 0x23;
    pub const HOME: u16 = 0x24;
    pub const LEFT: u16 = 0x25;
    pub const UP: u16 = 0x26;
    pub const RIGHT: u16 = 0x27;
    pub const DOWN: u16 = 0x28;
    pub const DELETE: u16 = 0x2E;
}

/// Where a caret move goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    TextStart,
    TextEnd,
    LineStart,
    LineEnd,
    PageUp,
    PageDown,
    PrevWord,
    NextWord,
    Left,
    Right,
    Up,
    Down,
}

/// A string waiting to be added, with the numbers of the font and the color to add it in.
#[derive(Clone, Debug)]
pub(crate) struct Download {
    text: Queued,
    font: u32,
    color: u32,
    /// The string tables it asked for, while they have not come (the download's number).
    asked: Option<u32>,
}

/// What a download adds: a written string, or a reference written when it is added.
#[derive(Clone, Debug)]
enum Queued {
    Written(Vec<u16>),
    Reference(Box<auberean_text::StringInfo>),
}

impl Download {
    /// A string to add; written already, it never waits.
    const fn new(text: Vec<u16>, font: u32, color: u32) -> Self {
        Self {
            text: Queued::Written(text),
            font,
            color,
            asked: None,
        }
    }

    /// A reference to add: the string tables it reads that the data lacks are asked for.
    // D2-11742, D2-11740
    fn of_reference(
        assets: &dyn crate::source::Assets,
        info: &auberean_text::StringInfo,
        font: u32,
        color: u32,
    ) -> Self {
        Self {
            asked: assets.start_string_download(info),
            text: Queued::Reference(Box::new(info.clone())),
            font,
            color,
        }
    }

    /// Whether what it asked for has all come (or it asked for nothing).
    // D2-11738
    fn ready(&mut self, assets: &dyn crate::source::Assets) -> bool {
        if let Some(n) = self.asked {
            if assets.string_download_waiting(n) {
                return false;
            }
            self.asked = None;
        }
        true
    }

    /// Its text, written now.
    fn text(&self, assets: &dyn crate::source::Assets) -> Vec<u16> {
        match &self.text {
            Queued::Written(t) => t.clone(),
            Queued::Reference(info) => assets.element_text(info),
        }
    }

    /// Dropped: what it still waits for is let go.
    fn release(&self, assets: &dyn crate::source::Assets) {
        if let Some(n) = self.asked {
            assets.release_string_download(n);
        }
    }
}

/// A text element's own state.
#[derive(Clone, Debug)]
pub struct TextState {
    /// Which typed characters it takes (all, without one).
    pub filter: Option<fn(u16) -> bool>,
    pub(crate) downloads: Vec<Download>,
    /// Selecting by dragging the pointer (the client never starts it).
    pub(crate) mouse_selecting: bool,
    /// The font and color text is added in now.
    pub font_color: Color,
    pub font: u32,
    pub font_outline: bool,
    /// The laid-out text's size, and where the view of it starts.
    pub paper_w: i32,
    pub paper_h: i32,
    pub view_y: i32,
    pub view_x: i32,
    /// The margins in use (top, bottom, left, right), and as asked for.
    pub marg: [i32; 4],
    pub req_marg: [i32; 4],
    last_cursor_move: f64,
    last_flash_flip: f64,
    cursor_visible: bool,
    pub glyphs: GlyphRun,
    /// The glyphs changed since they were last broken into lines.
    pub dirty: bool,
    /// The caret: the position before which typing goes.
    pub cursor: i32,
    pub selecting: bool,
    pub sel_start: i32,
    pub sel_end: i32,
    pub bits: u32,
    pub justification: Justify,
    /// The element's size and margins the lines were broken for.
    laid_for: (i32, i32, [i32; 4]),
}

impl Default for TextState {
    // D2-11778
    fn default() -> Self {
        Self {
            filter: None,
            downloads: Vec::new(),
            mouse_selecting: false,
            font_color: Color::WHITE,
            font: 0,
            font_outline: false,
            paper_w: 0,
            paper_h: 0,
            view_y: 0,
            view_x: 0,
            marg: [0; 4],
            req_marg: [0; 4],
            last_cursor_move: -1.0,
            last_flash_flip: -1.0,
            cursor_visible: true,
            glyphs: GlyphRun::default(),
            dirty: true,
            cursor: 0,
            selecting: false,
            sel_start: 0,
            sel_end: 0,
            bits: 0,
            justification: Justify::Left,
            laid_for: (-1, -1, [0; 4]),
        }
    }
}

impl TextState {
    /// Whether one of the flags in [`bits`] is set.
    #[must_use]
    pub const fn has(&self, b: u32) -> bool {
        self.bits & b != 0
    }
}

/// The keyboard's state the text elements read, and the clipboard.
#[derive(Clone, Debug, Default)]
pub struct Keyboard {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// What was last cut or copied, for the host to put on the system's clipboard; and what the
    /// host last read from it, for pasting.
    pub copied: Option<String>,
    pub clipboard: Option<String>,
    /// Where the host's input method writes, while an editable text element that allows it has
    /// the keyboard.
    pub ime: Option<Ime>,
}

/// The input method turned on for an editable text element: where on the screen it shows (the
/// part of the element that shows), and the font and color the element writes in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ime {
    pub area: Rect,
    pub font: u32,
    pub color: Color,
}

/// Only digits: the filter of an entry that takes numbers.
// D2-11743
#[must_use]
pub fn number_input_filter(c: u16) -> bool {
    (0x30..=0x39).contains(&c)
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// The layout a text element is drawn with: its lines, the paper's size and the view.
struct Laid {
    lines: Vec<Line>,
    paper_w: i32,
    paper_h: i32,
    view_y: i32,
    view_x: i32,
}

/// A text laid out for an element `w` by `h`: lines no wider than the element within its
/// margins (one line when asked); the paper at least the element's size, or the text's own when
/// the element sizes itself to it; the view kept on the paper.
fn lay_out_state(t: &mut TextState, w: i32, h: i32, fit_w: bool, fit_h: bool) {
    let [mu, md, ml, mr] = t.marg;
    let (pw, ph) = t.glyphs.recalculate(w - mr - ml, t.has(bits::ONE_LINE));
    t.paper_h = if fit_h {
        ph + md + mu
    } else {
        (ph + md + mu).max(h)
    };
    t.paper_w = if fit_w {
        pw + mr + ml
    } else {
        (pw + mr + ml).max(w)
    };
    t.laid_for = (w, h, t.marg);
    keep_in_bounds(t, w, h);
}

impl Ui {
    /// A text element's state.
    #[must_use]
    pub fn text_state(&self, h: Handle) -> Option<&TextState> {
        self.node(h)?.text_state.as_deref()
    }

    pub(crate) fn ts_mut(&mut self, h: Handle) -> Option<&mut TextState> {
        self.node_mut(h)?.text_state.as_deref_mut()
    }

    /// Which typed characters a text element takes (all, with none): a panel's entry that takes
    /// only numbers.
    pub fn set_input_filter(&mut self, h: Handle, filter: Option<fn(u16) -> bool>) {
        if let Some(t) = self.ts_mut(h) {
            t.filter = filter;
        }
    }

    fn size_of(&self, h: Handle) -> (i32, i32) {
        self.node(h)
            .map_or((0, 0), |n| (n.rect.width(), n.rect.height()))
    }

    fn text_locked(&self, h: Handle) -> bool {
        self.node(h).is_some_and(|n| n.bool_attr(name::TEXT_LOCKED))
    }

    /// The keyboard's modifiers, as the host sees them.
    pub fn set_modifiers(&mut self, shift: bool, ctrl: bool) {
        self.keyboard.shift = shift;
        self.keyboard.ctrl = ctrl;
    }

    /// Whether the host says Shift is held (a panel's drag check reads it).
    #[must_use]
    pub const fn shift_held(&self) -> bool {
        self.keyboard.shift
    }

    /// Whether a control key is held, as the host last said.
    #[must_use]
    pub fn ctrl_held(&self) -> bool {
        self.keyboard.ctrl
    }

    /// Alt as the host sees it.
    pub fn set_alt(&mut self, alt: bool) {
        self.keyboard.alt = alt;
    }

    /// Whether an Alt key is held, as the host last said.
    // D2-03448
    #[must_use]
    pub fn alt_held(&self) -> bool {
        self.keyboard.alt
    }

    /// What the host read from the system's clipboard, for pasting.
    pub fn set_clipboard(&mut self, text: Option<String>) {
        self.keyboard.clipboard = text;
    }

    /// What was cut or copied since last asked, for the host to put on the system's clipboard.
    pub fn take_copied(&mut self) -> Option<String> {
        self.keyboard.copied.take()
    }

    /// Text for the system's clipboard, as a panel sends it (the host puts it there as it does
    /// what was cut or copied).
    pub fn send_to_clipboard(&mut self, text: String) {
        self.keyboard.copied = Some(text);
    }

    // -----------------------------------------------------------------------------------------
    // Making and its attributes
    // -----------------------------------------------------------------------------------------

    /// A new text element: its margins worked out, its scroll bars set, the pointer's moves and
    /// the clock heard (queued strings are added, the caret blinks).
    // D2-11781, D2-11786, D2-11787
    pub(crate) fn text_created(&mut self, h: Handle) {
        self.determine_margin_values(h);
        self.register_for_global_message(h, global::MOUSE_MOVE);
        self.register_for_global_message(h, global::TIME);
        self.update_scrollbars(h);
    }

    /// What a text element does when one of its attributes is set (`changed` when the value is
    /// new): its flags, margins, greatest length, scroll bars, text and font. A font reset takes
    /// effect each time it is set.
    // D2-11780, D2-11785, D2-11835, D2-11836, D2-11837
    pub(crate) fn text_attr(&mut self, h: Handle, a: Token, v: &Value, changed: bool) {
        if self.text_state(h).is_none() {
            return;
        }
        if a == name::FONT_RESET {
            self.set_font_num(h, 0);
            self.reset_text(h);
        }
        if !changed {
            return;
        }
        match v {
            Value::Bool(b) => self.text_bool(h, a, *b),
            Value::Int(i) => self.text_int(h, a, *i),
            Value::Id(id) if a == name::VERTICAL_SCROLLBAR || a == name::HORIZONTAL_SCROLLBAR => {
                let bar = Token(*id);
                let msgs = if a == name::HORIZONTAL_SCROLLBAR {
                    [
                        bar_msg::NEW_POSITION,
                        bar_msg::HORIZONTAL_LINE_INCREMENT,
                        bar_msg::HORIZONTAL_LINE_DECREMENT,
                        bar_msg::HORIZONTAL_PAGE_INCREMENT,
                        bar_msg::HORIZONTAL_PAGE_DECREMENT,
                    ]
                } else {
                    [
                        bar_msg::NEW_POSITION,
                        bar_msg::VERTICAL_LINE_INCREMENT,
                        bar_msg::VERTICAL_LINE_DECREMENT,
                        bar_msg::VERTICAL_PAGE_INCREMENT,
                        bar_msg::VERTICAL_PAGE_DECREMENT,
                    ]
                };
                for m in msgs {
                    self.register_for_message(h, bar, m);
                }
                self.update_scrollbars(h);
            }
            Value::StringToken(tok) if a == attr::ENTRY => self.set_string(h, *tok),
            Value::Id(id) | Value::DataId(id) if a == attr::FONT && *id != 0 => {
                self.set_font_num(h, 0);
                self.reset_text(h);
            }
            _ => {}
        }
    }

    /// The text written again in the font and color of now.
    fn reset_text(&mut self, h: Handle) {
        let t = self.text(h).unwrap_or_default();
        self.set_text(h, &t);
    }

    fn text_bool(&mut self, h: Handle, a: Token, v: bool) {
        if a == attr::EDITABLE {
            self.set_atrb_editable(h, v);
        }
        if a == attr::ONE_LINE {
            self.set_atrb_one_line(h, v);
        }
        if a == attr::SELECTABLE {
            if let Some(t) = self.ts_mut(h) {
                set_bit(&mut t.bits, bits::SELECTABLE, v);
                t.mouse_selecting = false;
                t.selecting = false;
            }
        }
        if a == attr::CENTER_VERTICALLY {
            self.set_atrb_center_vertically(h, v);
        }
        if a == attr::CENTER {
            self.set_justification(h, Justify::Center);
        }
        if a == attr::RIGHT_JUSTIFY {
            self.set_justification(h, Justify::Right);
        }
        if a == name::NO_IME {
            if let Some(t) = self.ts_mut(h) {
                set_bit(&mut t.bits, bits::NO_IME, v);
            }
        }
        if a == name::TRIM_FROM_TOP {
            if let Some(t) = self.ts_mut(h) {
                t.glyphs.trim_from_top = v;
            }
        }
        if a == name::TEXT_LOCKED {
            if v {
                if let Some(t) = self.ts_mut(h) {
                    t.dirty = false;
                }
            } else {
                self.set_text_dirty(h, true);
            }
        }
    }

    fn text_int(&mut self, h: Handle, a: Token, v: i32) {
        let side = if a == attr::TOP_MARGIN {
            0
        } else if a == attr::BOTTOM_MARGIN {
            1
        } else if a == attr::LEFT_MARGIN {
            2
        } else if a == attr::RIGHT_MARGIN {
            3
        } else {
            if a == attr::MAX_CHARS {
                if let Some(t) = self.ts_mut(h) {
                    t.glyphs.max_chars = v;
                }
            }
            return;
        };
        let mut m = [-1; 4];
        m[side] = v;
        self.set_margins(h, m);
    }

    /// Ask for margins (top, bottom, left, right; -1 keeps one).
    // D2-11775
    pub fn set_margins(&mut self, h: Handle, m: [i32; 4]) {
        if let Some(t) = self.ts_mut(h) {
            for (i, v) in m.iter().enumerate() {
                if *v != -1 {
                    t.req_marg[i] = *v;
                }
            }
        }
        self.determine_margin_values(h);
        self.set_text_dirty(h, true);
    }

    /// The margins in use: as asked, but never more than the element holds (top before bottom,
    /// left before right).
    // D2-11754
    fn determine_margin_values(&mut self, h: Handle) {
        let (w, hh) = self.size_of(h);
        if let Some(t) = self.ts_mut(h) {
            let [u, d, l, r] = t.req_marg;
            let mu = u.min(hh);
            let md = d.min(hh - mu);
            let ml = l.min(w);
            let mr = r.min(w - ml);
            t.marg = [mu, md, ml, mr];
        }
    }

    /// Mark the text to be laid out again and drawn (unless locked).
    // D2-11747
    fn set_text_dirty(&mut self, h: Handle, dirty: bool) {
        if let Some(t) = self.ts_mut(h) {
            t.dirty = dirty;
        }
        if dirty && !self.text_locked(h) {
            self.touch();
        }
    }

    // D2-11753
    fn set_atrb_editable(&mut self, h: Handle, v: bool) {
        if let Some(t) = self.ts_mut(h) {
            set_bit(&mut t.bits, bits::EDITABLE, v);
        }
        if self.node(h).is_some_and(|n| n.active) {
            if v {
                self.activate_element(h);
            } else {
                self.deactivate_element(h);
            }
        }
    }

    // D2-11755
    fn set_atrb_one_line(&mut self, h: Handle, v: bool) {
        if let Some(t) = self.ts_mut(h) {
            set_bit(&mut t.bits, bits::ONE_LINE, v);
        }
        self.set_text_dirty(h, true);
    }

    // D2-11791
    fn set_atrb_center_vertically(&mut self, h: Handle, v: bool) {
        if let Some(t) = self.ts_mut(h) {
            set_bit(&mut t.bits, bits::CENTER_VERTICALLY, v);
        }
        self.keep_in_bounds(h);
        self.update_scrollbars(h);
    }

    // D2-11765
    fn set_justification(&mut self, h: Handle, j: Justify) {
        if let Some(t) = self.ts_mut(h) {
            t.justification = j;
        }
        self.set_text_dirty(h, true);
    }

    /// The element resized: its margins worked out again and its text laid out again.
    // D2-11774
    pub(crate) fn text_resized(&mut self, h: Handle) {
        self.determine_margin_values(h);
        self.set_text_dirty(h, true);
    }

    // -----------------------------------------------------------------------------------------
    // Fonts
    // -----------------------------------------------------------------------------------------

    /// Text is added from now in font `n`: the element's `font<n>` (`font` for 0), in the color
    /// of its `font<n>_color_red`, `_green` and `_blue` (white where one is missing) and with its
    /// `font<n>_outline`. Without that font nothing changes.
    // D2-11792
    pub fn set_font_num(&mut self, h: Handle, n: u32) {
        let font = numbered("font", n);
        let color = format!("{font}_color");
        self.apply_font(h, &font, &color);
    }

    /// Text is added from now in font `font` and color `color`: the element's `font<f>` and
    /// `font_color<c>_red`, `_green`, `_blue` (`font` and `font_color` for 0), with
    /// `font<f>_outline`.
    // D2-11793
    pub fn set_font_num_and_color(&mut self, h: Handle, font: u32, color: u32) {
        let f = numbered("font", font);
        let c = numbered("font_color", color);
        self.apply_font(h, &f, &c);
    }

    fn apply_font(&mut self, h: Handle, font: &str, color: &str) {
        let Some(n) = self.node(h) else { return };
        let did = n.id_attr(Token::of(font)).unwrap_or(0);
        let ch = |s: &str| n.float_attr(Token::of(&format!("{color}{s}")));
        let c = Color {
            r: ch("_red").unwrap_or(1.0),
            g: ch("_green").unwrap_or(1.0),
            b: ch("_blue").unwrap_or(1.0),
            a: 1.0,
        };
        let outline = n.bool_attr(Token::of(&format!("{font}_outline")));
        if did == 0 || self.assets().font(did).is_none() {
            return;
        }
        if let Some(t) = self.ts_mut(h) {
            t.font = did;
            t.font_color = c;
            t.font_outline = outline;
        }
    }

    /// Before a string is added or measured: when the element's font lacks one of its characters,
    /// the first of the fallback fonts that holds every one of them becomes the element's font
    /// (an element without a font keeps none).
    // D2-11776, D2-10050
    fn determine_font(&mut self, h: Handle, units: &[u16]) {
        let Some((_, font)) = self.current_font(h) else {
            return;
        };
        if text::font_contains(&font, units) {
            return;
        }
        for value in FALLBACK_FONTS {
            let id = self.assets().data_id_by_enum(value, FONT_MAPPING);
            let Some(fallback) = (id != 0).then(|| self.assets().font(id)).flatten() else {
                continue;
            };
            if text::font_contains(&fallback, units) {
                if let Some(t) = self.ts_mut(h) {
                    t.font = id;
                }
                return;
            }
        }
    }

    fn current_font(&self, h: Handle) -> Option<(u32, Arc<auberean_assets::ui::Font>)> {
        let id = self.text_state(h)?.font;
        if id == 0 {
            return None;
        }
        Some((id, self.assets().font(id)?))
    }

    // -----------------------------------------------------------------------------------------
    // Adding and setting text
    // -----------------------------------------------------------------------------------------

    /// Add a written string in font `font` and color `color` (numbers of the element's fonts and
    /// colors) at the caret; returns whether it was added at once.
    // D2-11823, D2-11824
    pub fn add_string_with_font(&mut self, h: Handle, s: &str, font: u32, color: u32) -> bool {
        let d = Download::new(utf16(s), font, color);
        self.add_download(h, d)
    }

    /// Queue a download and add what is ready; whether it was ready at once.
    fn add_download(&mut self, h: Handle, mut d: Download) -> bool {
        let assets = self.assets().clone();
        let ready = d.ready(&*assets);
        let Some(t) = self.ts_mut(h) else {
            d.release(&*assets);
            return false;
        };
        t.downloads.push(d);
        self.check_string_downloads(h);
        ready
    }

    /// Add a string reference in font `font` and color `color` at the caret, once the string
    /// tables it reads that the data lacks have come; returns whether it was added at once.
    // D2-11823, D2-11824
    pub fn add_string_info_with_font(
        &mut self,
        h: Handle,
        info: &auberean_text::StringInfo,
        font: u32,
        color: u32,
    ) -> bool {
        if self.text_state(h).is_none() {
            return false;
        }
        let assets = self.assets().clone();
        let d = Download::of_reference(&*assets, info, font, color);
        self.add_download(h, d)
    }

    /// Add a string reference in the element's own font and color.
    // D2-11826
    pub fn add_string_info(&mut self, h: Handle, info: &auberean_text::StringInfo) -> bool {
        self.add_string_info_with_font(h, info, 0, 0)
    }

    /// Replace the text with a string reference in font `font` and color `color`.
    // D2-11828
    pub fn set_string_info_with_font(
        &mut self,
        h: Handle,
        info: &auberean_text::StringInfo,
        font: u32,
        color: u32,
    ) -> bool {
        self.clear_all_text(h);
        self.add_string_info_with_font(h, info, font, color)
    }

    /// Replace the text with a string reference in the element's own font and color.
    // D2-11827
    pub fn set_string_info(&mut self, h: Handle, info: &auberean_text::StringInfo) -> bool {
        self.clear_all_text(h);
        self.add_string_info(h, info)
    }

    /// Add a written string in the element's own font and color.
    // D2-11826, D2-11829
    pub fn add_string(&mut self, h: Handle, s: &str) -> bool {
        self.add_string_with_font(h, s, 0, 0)
    }

    /// Replace the text with a written string in font `font` and color `color`.
    // D2-11828, D2-11832
    pub fn set_string_with_font(&mut self, h: Handle, s: &str, font: u32, color: u32) -> bool {
        self.clear_all_text(h);
        self.add_string_with_font(h, s, font, color)
    }

    /// Replace the text with a written string in the element's own font and color.
    // D2-11827, D2-11831
    pub fn set_text(&mut self, h: Handle, s: &str) {
        self.clear_all_text(h);
        self.add_string_with_font(h, s, 0, 0);
    }

    /// Replace the text with an entry of the element's string table.
    // D2-11833
    pub fn set_string(&mut self, h: Handle, token: u32) {
        let table = self.string_table(h);
        let s = self.assets().string(table, token).unwrap_or_default();
        self.set_text(h, &s);
    }

    /// Add the strings that are ready, in order, each in its font and color; say so when any
    /// was.
    // D2-11819
    fn check_string_downloads(&mut self, h: Handle) {
        let assets = self.assets().clone();
        let mut done = false;
        loop {
            let Some(t) = self.ts_mut(h) else { return };
            if t.downloads.first_mut().is_none_or(|d| !d.ready(&*assets)) {
                break;
            }
            let d = t.downloads.remove(0);
            self.set_font_num_and_color(h, d.font, d.color);
            let text = d.text(&*assets);
            self.add_text_internal(h, &text);
            done = true;
        }
        if done {
            self.broadcast(h, text_msg::DOWNLOAD_DONE, 0);
        }
    }

    /// Drop the strings waiting (and, as the element goes, what they still wait for).
    // D2-11752, D2-11767
    pub(crate) fn cleanup_string_downloads(&mut self, h: Handle) {
        let assets = self.assets().clone();
        if let Some(t) = self.ts_mut(h) {
            for d in t.downloads.drain(..) {
                d.release(&*assets);
            }
        }
    }

    /// Add `text` at the caret in the font and color of now, the caret after it: an editable
    /// element first drops what is selected, another stops selecting. (A one-line element keeps
    /// the caret in view.) Without a font the glyphs take no room and are not drawn, but are
    /// kept for when one comes.
    // D2-11816
    fn add_text_internal(&mut self, h: Handle, units: &[u16]) {
        if units.is_empty() {
            return;
        }
        let editable = self.text_state(h).is_some_and(|t| t.has(bits::EDITABLE));
        if editable {
            self.delete_selection(h);
        } else {
            self.set_selecting(h, false);
        }
        self.determine_font(h, units);
        let font = self.current_font(h);
        let Some(t) = self.ts_mut(h) else { return };
        let moved = t
            .glyphs
            .add_text(t.cursor, units, t.font_color, font, t.font_outline);
        t.cursor += moved;
        clamp_position(t);
        if t.has(bits::ONE_LINE) {
            self.scroll_horizontal_view(h);
        }
        self.set_text_dirty(h, true);
    }

    /// Drop every glyph, the selection and the strings waiting; the caret to the start.
    // D2-11768
    pub fn clear_all_text(&mut self, h: Handle) {
        self.cleanup_string_downloads(h);
        if let Some(t) = self.ts_mut(h) {
            t.glyphs.flush();
            t.selecting = false;
            t.cursor = 0;
        }
        self.set_text_dirty(h, true);
    }

    /// Whether the text, laid out, is wider than the element.
    pub fn text_too_wide(&mut self, h: Handle) -> bool {
        self.recalculate_glyph_list(h);
        let (w, _) = self.size_of(h);
        self.text_state(h).is_some_and(|t| t.paper_w > w)
    }

    /// Shorten the text from its end until it and `suffix` after it fit across the element.
    // D2-11830
    pub fn truncate_text(&mut self, h: Handle, suffix: &str) {
        self.recalculate_glyph_list(h);
        let (w, _) = self.size_of(h);
        let Some(paper) = self.text_state(h).map(|t| t.paper_w) else {
            return;
        };
        if paper <= w {
            return;
        }
        let mut units = self
            .text_state(h)
            .map(|t| t.glyphs.inq_text())
            .unwrap_or_default();
        let mut shown = String::new();
        let mut width = paper;
        while w < width {
            units.pop();
            shown = String::from_utf16_lossy(&units) + suffix;
            width = self.inq_size_with_margins(h, &shown).map_or(0, |s| s.0);
            if units.is_empty() {
                break;
            }
        }
        self.clear_all_text(h);
        self.add_string(h, &shown);
    }

    /// The text.
    // D2-11771
    #[must_use]
    pub fn text(&self, h: Handle) -> Option<String> {
        self.text_state(h)
            .map(|t| String::from_utf16_lossy(&t.glyphs.inq_text()))
    }

    /// The text selected.
    // D2-11772
    #[must_use]
    pub fn selected_text(&self, h: Handle) -> String {
        self.get_selection(h)
            .and_then(|(a, b)| {
                self.text_state(h)
                    .map(|t| String::from_utf16_lossy(&t.glyphs.inq_text_range(a, b)))
            })
            .unwrap_or_default()
    }

    /// How many lines the text broke into.
    // D2-11766
    #[must_use]
    pub fn total_lines(&self, h: Handle) -> usize {
        self.text_state(h).map_or(0, |t| t.glyphs.lines.len())
    }

    /// The size `s` would take in this element, margins included: laid out in the font of now
    /// no wider than the element's greatest width (else the screen's).
    // D2-11782
    #[must_use]
    pub fn inq_size_with_margins(&mut self, h: Handle, s: &str) -> Option<(i32, i32)> {
        let units = utf16(s);
        if units.is_empty() {
            return Some((0, 0));
        }
        self.determine_font(h, &units);
        let t = self.text_state(h)?;
        let font = self.current_font(h)?;
        let mut temp = GlyphRun::default();
        temp.add_text(0, &units, t.font_color, Some(font), t.font_outline);
        let max_w = self
            .node(h)
            .and_then(|n| n.int_attr(attr::MAX_WIDTH))
            .unwrap_or(self.display().0);
        let [mu, md, ml, mr] = t.marg;
        let (w, hh) = temp.recalculate(max_w - ml - mr, t.has(bits::ONE_LINE));
        Some((w + mr + ml, hh + md + mu))
    }

    // -----------------------------------------------------------------------------------------
    // Laying out and the view
    // -----------------------------------------------------------------------------------------

    /// Before the elements are drawn: each text element shown whose text or size changed is broken
    /// into lines, and its scroll bars follow.
    // D2-11796
    pub fn lay_out_text_shown(&mut self) {
        fn walk(ui: &Ui, h: Handle, out: &mut Vec<Handle>) {
            let Some(n) = ui.node(h) else { return };
            if !n.visible {
                return;
            }
            if let Some(t) = &n.text_state {
                if t.dirty || t.laid_for != (n.rect.width(), n.rect.height(), t.marg) {
                    out.push(h);
                }
            }
            for c in &n.children {
                walk(ui, *c, out);
            }
        }
        let mut changed = Vec::new();
        for r in self.roots() {
            walk(self, *r, &mut changed);
        }
        for h in changed {
            self.recalculate_glyph_list(h);
        }
    }

    /// Break the text into lines when it changed (see [`lay_out_state`]); the scroll bars
    /// follow.
    // D2-11794, D2-11796
    pub fn recalculate_glyph_list(&mut self, h: Handle) {
        let (w, hh) = self.size_of(h);
        let Some(n) = self.node(h) else { return };
        let fit_h = n.bool_attr(name::RESIZE_HEIGHT_TO_TEXT);
        let fit_w = n.bool_attr(name::RESIZE_WIDTH_TO_TEXT);
        let Some(t) = self.ts_mut(h) else { return };
        if !t.dirty && t.laid_for == (w, hh, t.marg) {
            return;
        }
        lay_out_state(t, w, hh, fit_w, fit_h);
        self.update_scrollbars(h);
        if let Some(t) = self.ts_mut(h) {
            t.dirty = false;
        }
    }

    /// The layout the element is drawn with now, without changing it.
    fn laid(&self, h: Handle) -> Option<Laid> {
        let t = self.text_state(h)?;
        let n = self.node(h)?;
        let (w, hh) = (n.rect.width(), n.rect.height());
        if !t.dirty && t.laid_for == (w, hh, t.marg) {
            return Some(Laid {
                lines: t.glyphs.lines.clone(),
                paper_w: t.paper_w,
                paper_h: t.paper_h,
                view_y: t.view_y,
                view_x: t.view_x,
            });
        }
        let mut s = t.clone();
        lay_out_state(
            &mut s,
            w,
            hh,
            n.bool_attr(name::RESIZE_WIDTH_TO_TEXT),
            n.bool_attr(name::RESIZE_HEIGHT_TO_TEXT),
        );
        Some(Laid {
            lines: s.glyphs.lines,
            paper_w: s.paper_w,
            paper_h: s.paper_h,
            view_y: s.view_y,
            view_x: s.view_x,
        })
    }

    // D2-11789
    fn keep_in_bounds(&mut self, h: Handle) {
        let (w, hh) = self.size_of(h);
        if let Some(t) = self.ts_mut(h) {
            keep_in_bounds(t, w, hh);
        }
    }

    /// Scroll the view down to `y`.
    // D2-11795
    pub fn set_view_y(&mut self, h: Handle, y: i32) {
        if let Some(t) = self.ts_mut(h) {
            t.view_y = y;
        }
        self.keep_in_bounds(h);
        self.touch();
    }

    /// Scroll the view to the line holding position `pos`.
    // D2-11804
    pub fn scroll_to_position(&mut self, h: Handle, pos: i32) {
        self.recalculate_glyph_list(h);
        let Some((_, y)) = self
            .text_state(h)
            .and_then(|t| t.glyphs.find_xy_from_position(pos))
        else {
            return;
        };
        if let Some(t) = self.ts_mut(h) {
            t.view_y = y;
        }
        self.keep_in_bounds(h);
        self.touch();
        self.update_scrollbars(h);
    }

    // D2-11809
    pub fn scroll_to_home(&mut self, h: Handle) {
        self.scroll_to_position(h, 0);
    }

    /// Scroll the view to the end of the text (the newest lines at the foot).
    // D2-11810
    pub fn scroll_to_end(&mut self, h: Handle) {
        let end = self.text_state(h).map_or(0, |t| t.glyphs.count());
        self.scroll_to_position(h, end);
    }

    /// A one-line element keeps the caret in view across it.
    // D2-11805
    fn scroll_horizontal_view(&mut self, h: Handle) {
        if !self.text_state(h).is_some_and(|t| t.has(bits::ONE_LINE)) {
            return;
        }
        self.recalculate_glyph_list(h);
        let (w, _) = self.size_of(h);
        let Some(t) = self.ts_mut(h) else { return };
        let Some((x, _)) = t.glyphs.find_xy_from_position(t.cursor) else {
            return;
        };
        if x < t.view_x {
            t.view_x = x.max(0);
        } else if x > w + 1 + t.view_x {
            t.view_x = (w - 1 + x).max(0);
        }
    }

    /// Scroll a line down or forward (`forward`) or back (`vertical`; else a column's width
    /// across): by the height or width of the line at the view's corner.
    // D2-11802
    fn determine_and_set_new_line(&mut self, h: Handle, vertical: bool, forward: bool) {
        let pos = self.determine_position_from_xy(h, 0, 0);
        if let Some(t) = self.ts_mut(h) {
            if let Some((line, _)) = t.glyphs.find_current_line(pos) {
                let step = if vertical {
                    t.glyphs.line_height(line)
                } else {
                    t.glyphs.line_width(line)
                };
                if let Some(step) = step {
                    let step = if forward { step } else { -step };
                    if vertical {
                        t.view_y += step;
                    } else {
                        t.view_x += step;
                    }
                }
            }
        }
        self.keep_in_bounds(h);
        self.touch();
        self.update_scrollbars(h);
    }

    /// Scroll a page (the element's height, or its width across).
    // D2-11803
    fn determine_and_set_new_page(&mut self, h: Handle, vertical: bool, forward: bool) {
        let (w, hh) = self.size_of(h);
        let page = if vertical { hh } else { w };
        let page = if forward { page } else { -page };
        if let Some(t) = self.ts_mut(h) {
            if vertical {
                t.view_y += page;
            } else {
                t.view_x += page;
            }
        }
        self.keep_in_bounds(h);
        self.touch();
        self.update_scrollbars(h);
    }

    // D2-11807
    pub fn vertical_page_decrement(&mut self, h: Handle) {
        self.determine_and_set_new_page(h, true, false);
    }

    // D2-11808
    pub fn vertical_page_increment(&mut self, h: Handle) {
        self.determine_and_set_new_page(h, true, true);
    }

    /// How far down the view is, 0 to 1 (0 when everything shows).
    // D2-11756
    fn percentage_v(&self, h: Handle) -> f32 {
        let (_, hh) = self.size_of(h);
        self.text_state(h)
            .map_or(0.0, |t| percentage(t.view_y, t.paper_h, hh))
    }

    // D2-11757
    fn percentage_h(&self, h: Handle) -> f32 {
        let (w, _) = self.size_of(h);
        self.text_state(h)
            .map_or(0.0, |t| percentage(t.view_x, t.paper_w, w))
    }

    // -----------------------------------------------------------------------------------------
    // Scroll bars
    // -----------------------------------------------------------------------------------------

    /// The scroll bar an attribute names: the one in the element's own window when its messages
    /// are its window's own, else the first anywhere.
    // D2-11744
    fn get_scrollbar(&self, h: Handle, which: Token, unique: bool) -> Option<Handle> {
        let id = self.node(h)?.id_attr(which)?;
        if id == 0 {
            return None;
        }
        if unique {
            let root = self.root_of(h);
            if self.node(root)?.id == Token(id) {
                return Some(root);
            }
            return self.child_recursive(root, Token(id));
        }
        self.find(Token(id))
    }

    /// A scroll bar set from the text: its widget as long as the share in view, at the view's
    /// place, laid out; disabled when everything shows.
    // D2-11773
    fn update_scrollbar(&mut self, h: Handle, which: Token, unique: bool) {
        let Some(bar) = self.get_scrollbar(h, which, unique) else {
            return;
        };
        let horizontal = which != name::VERTICAL_SCROLLBAR;
        let (w, hh) = self.size_of(h);
        let Some(t) = self.text_state(h) else { return };
        let (paper, view) = if horizontal {
            (t.paper_w, w)
        } else {
            (t.paper_h, hh)
        };
        self.set_attr(bar, name::ALLOW_UPDATING, Value::Bool(false));
        self.set_attr(bar, name::DISABLED, Value::Bool(false));
        #[allow(clippy::cast_precision_loss)]
        let share = if paper > 0 {
            (view as f32 / paper as f32).min(1.0)
        } else {
            1.0
        };
        let (size, pos, p) = if horizontal {
            (attr::X_WIDGET_SIZE, attr::X_POSITION, self.percentage_h(h))
        } else {
            (attr::Y_WIDGET_SIZE, attr::Y_POSITION, self.percentage_v(h))
        };
        self.set_attr(bar, size, Value::Float(share));
        self.set_attr(bar, pos, Value::Float(p));
        self.set_attr(bar, name::ALLOW_UPDATING, Value::Bool(true));
        self.lay_out_scrollbar(bar);
        if paper <= view {
            self.set_attr(bar, name::DISABLED, Value::Bool(true));
        }
    }

    // D2-11783
    fn update_scrollbars(&mut self, h: Handle) {
        let unique = self.node(h).is_some_and(|n| n.bool_attr(name::UNIQUE_MSG));
        self.update_scrollbar(h, name::VERTICAL_SCROLLBAR, unique);
        self.update_scrollbar(h, name::HORIZONTAL_SCROLLBAR, unique);
    }

    /// What one of the element's scroll bars said: the view follows its new place, or scrolls a
    /// page or a line.
    // D2-11811
    fn handle_scrollbar_message(&mut self, h: Handle, from: Handle, which: Token, what: Token) {
        let Some(id) = self.node(h).and_then(|n| n.id_attr(which)) else {
            return;
        };
        if self.node(from).map(|n| n.id) != Some(Token(id)) {
            return;
        }
        let vertical = which == name::VERTICAL_SCROLLBAR;
        if what == bar_msg::NEW_POSITION {
            let p = self
                .node(from)
                .and_then(|n| {
                    n.float_attr(if vertical {
                        attr::Y_POSITION
                    } else {
                        attr::X_POSITION
                    })
                })
                .unwrap_or(0.0);
            let (w, hh) = self.size_of(h);
            if let Some(t) = self.ts_mut(h) {
                let (paper, view) = if vertical {
                    (t.paper_h, hh)
                } else {
                    (t.paper_w, w)
                };
                #[allow(clippy::cast_precision_loss)]
                let at = dereth_primitives::num::to_i32(p * (paper - view) as f32);
                if vertical {
                    t.view_y = at;
                } else {
                    t.view_x = at;
                }
            }
            self.keep_in_bounds(h);
            self.touch();
            return;
        }
        let table = [
            (bar_msg::VERTICAL_PAGE_INCREMENT, true, true, true),
            (bar_msg::VERTICAL_PAGE_DECREMENT, true, true, false),
            (bar_msg::VERTICAL_LINE_INCREMENT, false, true, true),
            (bar_msg::VERTICAL_LINE_DECREMENT, false, true, false),
            (bar_msg::HORIZONTAL_PAGE_INCREMENT, true, false, true),
            (bar_msg::HORIZONTAL_PAGE_DECREMENT, true, false, false),
            (bar_msg::HORIZONTAL_LINE_INCREMENT, false, false, true),
            (bar_msg::HORIZONTAL_LINE_DECREMENT, false, false, false),
        ];
        if let Some(&(_, page, vert, forward)) = table.iter().find(|r| r.0 == what) {
            if page {
                self.determine_and_set_new_page(h, vert, forward);
            } else {
                self.determine_and_set_new_line(h, vert, forward);
            }
        }
    }

    /// What another element said to this one: its scroll bars (in its own window when its
    /// messages are its window's own); a menu showing its choice here: the chosen row's text
    /// (a row of text only), or nothing when none is chosen.
    // D2-11838
    pub(crate) fn text_hear(&mut self, h: Handle, from: Handle, what: Token, param: u32) {
        let unique = self.node(h).is_some_and(|n| n.bool_attr(name::UNIQUE_MSG));
        if !unique || param == self.root_of(h).index() {
            self.handle_scrollbar_message(h, from, name::VERTICAL_SCROLLBAR, what);
            self.handle_scrollbar_message(h, from, name::HORIZONTAL_SCROLLBAR, what);
        }
        if what != name::DISPLAY_SELECTION
            || self
                .node(from)
                .is_none_or(|n| n.kind != crate::token::kind::MENU)
        {
            return;
        }
        match self.menu_selected_item(from) {
            Some(row) => {
                if self
                    .node(row)
                    .is_some_and(|n| n.kind == crate::token::kind::TEXT)
                {
                    let words = self.text(row).unwrap_or_default();
                    self.set_text(h, &words);
                }
            }
            None => {
                if self.text(h).is_some_and(|t| !t.is_empty()) {
                    self.set_text(h, "");
                }
            }
        }
    }

    /// The wheel over the element turns its vertical scroll bar (when it has one, enabled).
    // D2-11746
    fn text_mouse_wheel(&mut self, h: Handle, x: i32, y: i32, b: u32) {
        let unique = self.node(h).is_some_and(|n| n.bool_attr(name::UNIQUE_MSG));
        let Some(bar) = self.get_scrollbar(h, name::VERTICAL_SCROLLBAR, unique) else {
            return;
        };
        if self.node(bar).is_some_and(|n| n.bool_attr(name::DISABLED)) {
            return;
        }
        let mut handled = false;
        self.mouse_down(bar, x, y, b, &mut handled);
    }

    // -----------------------------------------------------------------------------------------
    // The caret and the selection
    // -----------------------------------------------------------------------------------------

    /// Selecting on or off; turning it on starts an empty selection.
    // D2-11761
    pub fn set_selecting(&mut self, h: Handle, on: bool) {
        if let Some(t) = self.ts_mut(h) {
            if t.selecting != on {
                if on {
                    t.sel_start = 0;
                    t.sel_end = 0;
                }
                t.selecting = on;
            }
        }
    }

    /// Where the selection starts (and ends, for now); only while selecting.
    // D2-11762
    fn set_selection_start(&mut self, h: Handle, pos: i32) -> bool {
        let Some(t) = self.ts_mut(h) else {
            return false;
        };
        if !t.selecting {
            return false;
        }
        let p = pos.min(t.glyphs.count()).max(0);
        t.sel_start = p;
        t.sel_end = p;
        true
    }

    // D2-11763
    fn set_selection_end(&mut self, h: Handle, pos: i32) -> bool {
        let Some(t) = self.ts_mut(h) else {
            return false;
        };
        if !t.selecting {
            return false;
        }
        t.sel_end = pos.min(t.glyphs.count()).max(0);
        true
    }

    /// The selection, its ends in order, while selecting.
    // D2-11764
    #[must_use]
    pub fn get_selection(&self, h: Handle) -> Option<(i32, i32)> {
        let t = self.text_state(h)?;
        t.selecting
            .then(|| (t.sel_start.min(t.sel_end), t.sel_start.max(t.sel_end)))
    }

    /// Select every glyph (a selectable element only).
    // D2-11769
    pub fn select_all(&mut self, h: Handle) {
        let Some(t) = self.ts_mut(h) else { return };
        if !t.has(bits::SELECTABLE) {
            return;
        }
        if !t.selecting {
            t.sel_start = 0;
            t.sel_end = 0;
            t.selecting = true;
        }
        let end = t.glyphs.count();
        self.set_selection_start(h, 0);
        self.set_selection_end(h, end);
        self.touch();
    }

    // D2-11770
    pub fn deselect(&mut self, h: Handle) {
        if let Some(t) = self.ts_mut(h) {
            t.selecting = false;
        }
        self.touch();
    }

    /// Put the caret at `pos` (within the text). Selecting by the pointer or with Shift held
    /// grows the selection from where the caret was; otherwise any selection goes. The caret
    /// shows steadily for a while after it moves.
    // D2-11777
    pub fn set_cursor_position(&mut self, h: Handle, pos: i32) -> bool {
        let shift = self.keyboard.shift;
        let now = self.now();
        let Some(t) = self.text_state(h) else {
            return false;
        };
        let (dragging, selecting, selectable, cursor) = (
            t.mouse_selecting,
            t.selecting,
            t.has(bits::SELECTABLE),
            t.cursor,
        );
        if !dragging && !shift {
            if selecting {
                self.set_selecting(h, false);
            }
        } else if !selecting && selectable {
            self.set_selecting(h, true);
            self.set_selection_start(h, cursor);
        }
        let Some(t) = self.ts_mut(h) else {
            return false;
        };
        t.cursor = pos;
        clamp_position(t);
        t.last_cursor_move = now;
        let (selecting, at, editable) = (t.selecting, t.cursor, t.has(bits::EDITABLE));
        if selecting {
            self.set_selection_end(h, at);
        }
        if editable && self.node(h).is_some_and(|n| n.active) {
            self.touch();
        }
        true
    }

    /// The caret put at the position nearest `(x, y)` in the element.
    // D2-11798
    fn set_cursor_position_xy(&mut self, h: Handle, x: i32, y: i32) -> bool {
        let pos = self.determine_position_from_xy(h, x, y);
        self.set_cursor_position(h, pos)
    }

    /// The position nearest `(x, y)` in the element: the line under it, then across that line as
    /// it is justified.
    // D2-11797
    fn determine_position_from_xy(&mut self, h: Handle, x: i32, y: i32) -> i32 {
        self.recalculate_glyph_list(h);
        let Some(t) = self.text_state(h) else {
            return 0;
        };
        let [mu, _, ml, mr] = t.marg;
        let px = t.view_x - ml + x;
        let py = t.view_y - mu + y;
        let line = t.glyphs.find_line_from_y(py);
        let lw = t.glyphs.line_width(line).unwrap_or(0);
        let avail = t.paper_w - mr - ml;
        let px = px - text::justify(avail, lw, t.justification);
        t.glyphs
            .find_pos_from_line_and_pixels(line, px)
            .map_or(0, |p| p.min(t.glyphs.count()).max(0))
    }

    /// Move the caret: to the text's start or end, the line's start or end, a page up or down,
    /// the word before or after, a character left or right, a line up or down. A one-line element
    /// keeps the caret in view.
    // D2-11812
    pub fn move_cursor(&mut self, h: Handle, m: Move) -> bool {
        let Some(t) = self.text_state(h) else {
            return false;
        };
        let cursor = t.cursor;
        let to = match m {
            Move::TextStart => Some(0),
            Move::TextEnd => Some(t.glyphs.count()),
            Move::LineStart => self.move_begin_end_line(h, true, cursor),
            Move::LineEnd => self.move_begin_end_line(h, false, cursor),
            Move::PageUp | Move::PageDown => {
                self.determine_and_set_new_page(h, true, m == Move::PageDown);
                Some(self.determine_position_from_xy(h, 0, 0))
            }
            Move::PrevWord => t.glyphs.find_prev_word(cursor),
            Move::NextWord => t.glyphs.find_next_word(cursor),
            Move::Left => Some(cursor - 1),
            Move::Right => Some(cursor + 1),
            Move::Up => self.move_up_down(h, true, cursor),
            Move::Down => self.move_up_down(h, false, cursor),
        };
        let Some(to) = to else { return false };
        let ok = self.set_cursor_position(h, to);
        if ok {
            self.scroll_horizontal_view(h);
        }
        ok
    }

    /// The caret to the end of the text.
    // D2-11813
    pub fn move_cursor_to_end(&mut self, h: Handle) {
        let end = self.text_state(h).map_or(0, |t| t.glyphs.count());
        if self.set_cursor_position(h, end) {
            self.scroll_horizontal_view(h);
        }
    }

    /// The position a line up (`up`) or down from `pos`, as far across as `pos` is (on the first
    /// line up, or the last down, it stays put).
    // D2-11759
    fn move_up_down(&self, h: Handle, up: bool, pos: i32) -> Option<i32> {
        let t = self.text_state(h)?;
        let g = &t.glyphs;
        let current = g.find_current_line(pos);
        if let Some((line, _)) = current {
            let edge = if up { 0 } else { int(g.lines.len()) - 1 };
            if line == edge {
                return Some(pos);
            }
        }
        let mut px = g.find_pixels_from_pos(pos)?;
        let (to, _) = if up {
            g.find_prev_line(pos)?
        } else {
            g.find_next_line(pos)?
        };
        let line = current.map_or(0, |c| c.0);
        let [_, _, ml, mr] = t.marg;
        let avail = t.paper_w - mr - ml;
        px += text::justify(avail, g.line_width(line).unwrap_or(0), t.justification);
        px -= text::justify(avail, g.line_width(to).unwrap_or(0), t.justification);
        g.find_pos_from_line_and_pixels(to, px)
    }

    /// The start (`start`) or end of the line holding `pos`: the end is the place before the next
    /// line, or the text's end when the next line is the last.
    // D2-11760
    fn move_begin_end_line(&self, h: Handle, start: bool, pos: i32) -> Option<i32> {
        let g = &self.text_state(h)?.glyphs;
        let (_, first) = g.find_current_line(pos)?;
        if start {
            return Some(first);
        }
        let (next, next_start) = g.find_next_line(pos)?;
        if next == int(g.lines.len()) - 1 {
            return Some(g.count());
        }
        Some(next_start - 1)
    }

    /// Drop the glyphs from `from` up to `to`; the caret moves back with them.
    // D2-11806
    fn delete_section(&mut self, h: Handle, from: i32, to: i32) {
        let Some(t) = self.ts_mut(h) else { return };
        let count = t.glyphs.count();
        if from < 0 || from >= count || to <= 0 || to > count || from > to {
            return;
        }
        let gone = t.glyphs.delete(from, to);
        if from < t.cursor {
            t.cursor -= gone;
            clamp_position(t);
        }
        if t.has(bits::ONE_LINE) {
            self.scroll_horizontal_view(h);
        }
        self.set_text_dirty(h, true);
    }

    /// Drop the glyph after the caret.
    // D2-11814
    pub fn delete_char(&mut self, h: Handle) {
        let at = self.text_state(h).map_or(0, |t| t.cursor);
        self.delete_section(h, at, at + 1);
    }

    /// Drop the selection, and stop selecting.
    // D2-11815
    pub fn delete_selection(&mut self, h: Handle) {
        let Some((a, b)) = self.get_selection(h) else {
            return;
        };
        self.delete_section(h, a, b);
        if let Some(t) = self.ts_mut(h) {
            t.selecting = false;
        }
    }

    /// Copy the selection.
    // D2-11788
    pub fn copy(&mut self, h: Handle) {
        self.keyboard.copied = Some(self.selected_text(h));
    }

    /// Copy the selection and drop it.
    // D2-11817
    pub fn cut(&mut self, h: Handle) {
        if self.get_selection(h).is_none() {
            return;
        }
        self.copy(h);
        self.delete_selection(h);
    }

    /// Put what the clipboard holds at the caret, in place of the selection.
    // D2-11820
    pub fn paste(&mut self, h: Handle) {
        if self.get_selection(h).is_some() {
            self.delete_selection(h);
        }
        if let Some(s) = self.keyboard.clipboard.clone() {
            self.check_string_downloads(h);
            self.add_text_internal(h, &utf16(&s));
        }
    }

    // -----------------------------------------------------------------------------------------
    // The keyboard
    // -----------------------------------------------------------------------------------------

    /// Take the keyboard: an editable element takes typed characters from now and says so.
    /// Returns whether it newly took the keyboard as an editable element.
    // D2-11750
    pub(crate) fn text_activate(&mut self, h: Handle) -> bool {
        let was = self.node(h).is_some_and(|n| n.active);
        let ok = self.element_activate(h);
        let editable = self.text_state(h).is_some_and(|t| t.has(bits::EDITABLE));
        if ok && !was && editable {
            self.ime_turn_on(h);
            self.broadcast(h, text_msg::ACTIVATED, 0);
            self.touch();
            return true;
        }
        false
    }

    /// Turn the host's input method on for the element, over the part of it that shows, in its
    /// font and color; an element marked `no_IME` does not. Whether it was turned on.
    // D2-11749
    fn ime_turn_on(&mut self, h: Handle) -> bool {
        let Some(t) = self.text_state(h) else {
            return false;
        };
        if t.has(bits::NO_IME) {
            return false;
        }
        let (font, color) = (t.font, t.font_color);
        let Some(area) = self.screen_clip_rect(h) else {
            return false;
        };
        self.keyboard.ime = Some(Ime { area, font, color });
        true
    }

    /// A text element matched to another: the other takes its text when theirs differ.
    // D2-11834
    pub(crate) fn text_match_attributes(&mut self, h: Handle, other: Handle) {
        let mine = self.text(h).unwrap_or_default();
        if self.text(other).unwrap_or_default() != mine {
            self.set_text(other, &mine);
        }
    }

    /// Where the host's input method writes, while it is on.
    #[must_use]
    pub const fn ime(&self) -> Option<&Ime> {
        self.keyboard.ime.as_ref()
    }

    // D2-11751
    pub(crate) fn text_deactivate(&mut self, h: Handle) -> bool {
        let was = self.node(h).is_some_and(|n| n.active);
        let ok = self.element_deactivate(h);
        let editable = self.text_state(h).is_some_and(|t| t.has(bits::EDITABLE));
        if ok && was && editable {
            self.keyboard.ime = None;
            self.broadcast(h, text_msg::DEACTIVATED, 0);
            self.touch();
            return true;
        }
        false
    }

    /// The interface's movement keys while the element has the keyboard: a character (a word
    /// with Ctrl) left or right, a line up or down, a page up or down, the line's start or end
    /// (the text's with Ctrl). Returns whether it moved the caret for the key.
    // D2-11818
    pub(crate) fn text_action(&mut self, h: Handle, action: u32, down: bool) -> bool {
        use crate::types::key_id;
        if !down {
            return false;
        }
        let ctrl = self.keyboard.ctrl;
        let m = match action {
            key_id::LEFT if ctrl => Move::PrevWord,
            key_id::LEFT => Move::Left,
            key_id::RIGHT if ctrl => Move::NextWord,
            key_id::RIGHT => Move::Right,
            key_id::UP => Move::Up,
            key_id::DOWN => Move::Down,
            key_id::PAGE_UP => Move::PageUp,
            key_id::PAGE_DOWN => Move::PageDown,
            key_id::HOME if ctrl => Move::TextStart,
            key_id::HOME => Move::LineStart,
            key_id::END if ctrl => Move::TextEnd,
            key_id::END => Move::LineEnd,
            _ => return false,
        };
        self.move_cursor(h, m);
        true
    }

    /// A character typed (`ch`, with `key` 0) or another key pressed (`key`, the host's code) on
    /// the element with the keyboard. The movement keys move the caret. An element that is not
    /// editable only says what came. An editable one: Delete drops the selection or the glyph
    /// after the caret; Ctrl+C copies, Ctrl+X cuts, Ctrl+V pastes; Backspace drops the
    /// selection or the glyph before the caret; Tab and Escape do nothing; Enter adds a new line
    /// (not on one line); any other printable character its filter takes is added at the caret
    /// (the view following a caret at the end). It says a key came or a character was typed.
    // D2-11821
    pub(crate) fn character_handler(&mut self, h: Handle, ch: u16, key: u16) {
        let ctrl = self.keyboard.ctrl;
        let m = match key {
            key::PAGE_UP => Some(Move::PageUp),
            key::PAGE_DOWN => Some(Move::PageDown),
            key::END if ctrl => Some(Move::TextEnd),
            key::END => Some(Move::LineEnd),
            key::HOME if ctrl => Some(Move::TextStart),
            key::HOME => Some(Move::LineStart),
            key::LEFT if ctrl => Some(Move::PrevWord),
            key::LEFT => Some(Move::Left),
            key::UP => Some(Move::Up),
            key::RIGHT if ctrl => Some(Move::NextWord),
            key::RIGHT => Some(Move::Right),
            key::DOWN => Some(Move::Down),
            _ => None,
        };
        if let Some(m) = m {
            self.move_cursor(h, m);
        }
        let Some(t) = self.text_state(h) else { return };
        if !t.has(bits::EDITABLE) {
            if key == 0 {
                self.broadcast(h, text_msg::CHARACTER_INPUT, u32::from(ch));
            } else {
                self.broadcast(h, text_msg::KEY_INPUT, u32::from(key));
            }
            return;
        }
        if key == key::DELETE {
            if t.selecting {
                self.delete_selection(h);
            } else {
                self.delete_char(h);
            }
            self.broadcast(h, text_msg::KEY_INPUT, u32::from(key));
            return;
        }
        if key != 0 {
            self.broadcast(h, text_msg::KEY_INPUT, u32::from(key));
            return;
        }
        let one_line = t.has(bits::ONE_LINE);
        let filter = t.filter;
        let selecting = t.selecting;
        let cursor = t.cursor;
        match ch {
            3 => self.copy(h),
            8 => {
                if selecting {
                    self.delete_selection(h);
                } else if cursor != 0 {
                    if self.set_cursor_position(h, cursor - 1) {
                        self.scroll_horizontal_view(h);
                    }
                    self.delete_char(h);
                }
            }
            9 | 0x1B => {}
            10 | 13 => {
                if !one_line {
                    self.type_glyph(h, text::NEW_LINE);
                }
            }
            0x16 => self.paste(h),
            0x18 => self.cut(h),
            c => {
                if c >= 0x20 && c != 0x7F && filter.is_none_or(|f| f(c)) {
                    self.type_glyph(h, c);
                }
            }
        }
        self.broadcast(h, text_msg::CHARACTER_INPUT, u32::from(ch));
    }

    /// One typed character added at the caret; a caret then at the end keeps the end in view.
    fn type_glyph(&mut self, h: Handle, c: u16) {
        self.add_text_internal(h, &[c]);
        if self
            .text_state(h)
            .is_some_and(|t| t.cursor == t.glyphs.count())
        {
            self.scroll_to_end(h);
        }
    }

    /// A character typed (`key` 0) or a key pressed (the host's code), to the editable text
    /// element with the keyboard; returns whether one took it.
    pub fn character_input(&mut self, ch: u16, key: u16) -> bool {
        let Some(h) = self.focus() else {
            return false;
        };
        self.character_handler(h, ch, key);
        true
    }

    /// Type into the entry that has the keyboard, a character at a time; returns whether an entry
    /// took the text.
    // D2-04864
    pub fn type_text(&mut self, typed: &str) -> bool {
        let Some(h) = self.focus() else {
            return false;
        };
        for c in typed.encode_utf16() {
            self.character_handler(h, c, 0);
        }
        true
    }

    /// Backspace in the entry that has the keyboard.
    pub fn backspace(&mut self) -> bool {
        self.character_input(8, 0)
    }

    // -----------------------------------------------------------------------------------------
    // The pointer and the clock
    // -----------------------------------------------------------------------------------------

    /// A press on a text element: the plain element's; then (neither moving nor resizing, the
    /// topmost) the left button on an editable or selectable one puts the caret under the
    /// pointer, and an editable one takes the keyboard; the wheel turns its scroll bar.
    // D2-11799
    pub(crate) fn text_mouse_down(
        &mut self,
        h: Handle,
        x: i32,
        y: i32,
        b: u32,
        handled: &mut bool,
    ) {
        self.element_mouse_down(h, x, y, b, handled);
        let Some(n) = self.node(h) else { return };
        let wheel = b == button_id::WHEEL_UP || b == button_id::WHEEL_DOWN;
        if n.moving || n.resizing || !n.over_top || !(b == button_id::LEFT || wheel) {
            return;
        }
        if b == button_id::LEFT {
            let t_bits = self.text_state(h).map_or(0, |t| t.bits);
            if t_bits & (bits::EDITABLE | bits::SELECTABLE) != 0 {
                let (sx, sy) = self.screen_origin(h).unwrap_or((0, 0));
                self.set_cursor_position_xy(h, x - sx, y - sy);
                if t_bits & bits::EDITABLE != 0 {
                    self.activate_element(h);
                }
            }
        } else {
            self.text_mouse_wheel(h, x, y, b);
        }
    }

    /// A release ends a selection made by dragging, the caret at the pointer.
    // D2-11800
    pub(crate) fn text_mouse_up(&mut self, h: Handle, x: i32, y: i32, b: u32, handled: &mut bool) {
        self.element_mouse_up(h, x, y, b, handled);
        if !self.text_state(h).is_some_and(|t| t.mouse_selecting) {
            return;
        }
        if let Some(t) = self.ts_mut(h) {
            t.mouse_selecting = false;
        }
        let (px, py) = self.pointer();
        let (sx, sy) = self.screen_origin(h).unwrap_or((0, 0));
        let pos = self.determine_position_from_xy(h, px - sx, py - sy);
        self.set_cursor_position(h, pos);
    }

    /// While dragging a selection, the caret follows the pointer.
    // D2-11801
    fn text_global_mouse_move(&mut self, h: Handle) {
        if !self.text_state(h).is_some_and(|t| t.mouse_selecting) {
            return;
        }
        let (px, py) = self.pointer();
        let (sx, sy) = self.screen_origin(h).unwrap_or((0, 0));
        let pos = self.determine_position_from_xy(h, px - sx, py - sy);
        self.set_cursor_position(h, pos);
    }

    /// The clock: queued strings added; an editable element's caret shows steadily for 1.5
    /// seconds after it moves, then blinks, a second on and a second off.
    // D2-11822
    fn text_global_loop(&mut self, h: Handle) {
        self.check_string_downloads(h);
        let now = self.now();
        let Some(t) = self.ts_mut(h) else { return };
        if !t.has(bits::EDITABLE) {
            return;
        }
        let before = t.cursor_visible;
        if now - t.last_cursor_move < 1.5 {
            t.cursor_visible = true;
        } else {
            if now - t.last_flash_flip < 1.0 {
                return;
            }
            t.last_flash_flip = now;
            t.cursor_visible = !t.cursor_visible;
        }
        if t.cursor_visible != before {
            self.touch();
        }
    }

    /// The global messages to a text element: the pointer's moves and the clock.
    // D2-11825
    pub(crate) fn text_listen(&mut self, h: Handle, what: u32, param: u32) {
        self.element_listen(h, what, param);
        if what == global::MOUSE_MOVE {
            self.text_global_mouse_move(h);
        } else if what == global::TIME {
            self.text_global_loop(h);
        }
    }

    /// Whether the element shows its caret now: it has the keyboard, is editable, and the caret
    /// is in the on half of its blink.
    #[must_use]
    pub fn caret_shown(&self, h: Handle) -> bool {
        self.node(h).is_some_and(|n| n.active)
            && self
                .text_state(h)
                .is_some_and(|t| t.has(bits::EDITABLE) && t.cursor_visible)
    }

    // -----------------------------------------------------------------------------------------
    // Lines in their own colors (a log)
    // -----------------------------------------------------------------------------------------

    /// Add a line at the text's end in `color` (in the element's own font), on a new line when
    /// there is text before it, keeping the newest `keep` lines; the view at the end.
    pub fn add_line(&mut self, h: Handle, line: &str, color: Color, keep: usize) {
        if self.text_state(h).is_none() {
            return;
        }
        self.move_cursor_to_end(h);
        self.set_font_num_and_color(h, 0, 0);
        let had = self.text_state(h).is_some_and(|t| t.glyphs.count() > 0);
        let mut units = Vec::new();
        if had {
            units.push(text::NEW_LINE);
        }
        units.extend(line.encode_utf16());
        if let Some(t) = self.ts_mut(h) {
            t.font_color = color;
        }
        self.add_text_internal(h, &units);
        while let Some(t) = self.text_state(h) {
            let lines = t.glyphs.glyphs.iter().filter(|g| g.is_new_line()).count() + 1;
            if lines <= keep {
                break;
            }
            let cut = t
                .glyphs
                .glyphs
                .iter()
                .position(text::Glyph::is_new_line)
                .map_or(0, |i| int(i) + 1);
            self.delete_section(h, 0, cut);
        }
        self.scroll_to_end(h);
    }

    /// Drop a text element's text.
    pub fn clear_lines(&mut self, h: Handle) {
        self.clear_all_text(h);
    }

    /// A text element's lines (split at its new lines).
    #[must_use]
    pub fn lines(&self, h: Handle) -> Vec<String> {
        self.text(h)
            .filter(|t| !t.is_empty())
            .map(|t| t.split('\n').map(str::to_owned).collect())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------------------------

fn int(v: usize) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

fn set_bit(b: &mut u32, flag: u32, on: bool) {
    if on {
        *b |= flag;
    } else {
        *b &= !flag;
    }
}

fn numbered(base: &str, n: u32) -> String {
    if n == 0 {
        base.to_owned()
    } else {
        format!("{base}{n}")
    }
}

/// The caret kept within the text.
// D2-11758
fn clamp_position(t: &mut TextState) {
    t.cursor = t.cursor.min(t.glyphs.count()).max(0);
}

/// The view kept on the paper; a vertically centered text's view puts its lines in the middle.
fn keep_in_bounds(t: &mut TextState, w: i32, h: i32) {
    if t.view_x < 0 {
        t.view_x = 0;
    }
    if t.paper_w < t.view_x + w {
        t.view_x = (t.paper_w - w).max(0);
    }
    if t.has(bits::CENTER_VERTICALLY) {
        let total: i32 = t.glyphs.lines.iter().map(|l| l.height).sum();
        t.view_y = total / 2 - h / 2;
    } else {
        if t.view_y < 0 {
            t.view_y = 0;
        }
        if t.paper_h < t.view_y + h {
            t.view_y = (t.paper_h - h).max(0);
        }
    }
}

fn percentage(view: i32, paper: i32, size: i32) -> f32 {
    if paper <= size {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let p = view as f32 / (paper - size) as f32;
    if p < 1.0 {
        p.max(0.0)
    } else {
        1.0
    }
}

/// Draw a text element's glyphs at `area` (on the screen), clipped to `clip`: each line from the
/// view's corner, justified within the margins (a text that is not editable and has no
/// horizontal scroll bar justified across its element), each glyph in its own font and color
/// (outlined ones over a black outline), selected glyphs turned over; and the caret, while it
/// shows, a line the font's height in the font's color before the glyph it stands at.
// D2-11790
pub(crate) fn post_blit(ui: &Ui, s: &mut Surface, h: Handle, area: Rect, clip: Rect) {
    let Some(n) = ui.node(h) else { return };
    let Some(t) = n.text_state.as_deref() else {
        return;
    };
    let Some(laid) = ui.laid(h) else { return };
    let (w, hh) = (n.rect.width(), n.rect.height());
    let [mu, md, ml, mr] = t.marg;
    let mut view_x = laid.view_x;
    if !t.has(bits::EDITABLE) {
        let unique = n.bool_attr(name::UNIQUE_MSG);
        if ui
            .get_scrollbar(h, name::HORIZONTAL_SCROLLBAR, unique)
            .is_none()
        {
            view_x = -text::justify(w, laid.paper_w - mr - ml, t.justification);
        }
    }
    let left = ml - view_x;
    let avail = laid.paper_w - mr - ml;
    if w <= 0 || hh <= 0 || avail <= 0 || laid.paper_h - md - mu <= 0 {
        return;
    }
    let lines = &laid.lines;
    if lines.is_empty() {
        return;
    }
    let selection = ui.get_selection(h);
    let mut x = text::justify(avail, lines[0].width, t.justification) + left;
    let mut y = mu - laid.view_y;
    let mut height = lines[0].height;
    let mut next = 1;
    let mut caret: Option<(i32, i32)> = None;
    for (i, g) in t.glyphs.glyphs.iter().enumerate() {
        let Some(face) = &g.face else { continue };
        if next < lines.len() && lines[next].start <= i {
            y += height;
            x = text::justify(avail, lines[next].width, t.justification) + left;
            height = lines[next].height;
            next += 1;
        }
        if int(i) == t.cursor {
            caret = Some((x, y));
        }
        if g.is_new_line() {
            continue;
        }
        let (gx, gy) = (area.x0 + x, area.y0 + y);
        if g.outline {
            for dy in -1..2 {
                for dx in -1..2 {
                    if dx != 0 || dy != 0 {
                        text::draw_glyph(
                            s,
                            face,
                            g.ch,
                            gx + dx,
                            gy + dy,
                            clip,
                            Color::BLACK,
                            false,
                        );
                    }
                }
            }
        }
        let adv = text::draw_glyph(s, face, g.ch, gx, gy, clip, g.color, true);
        if selection.is_some_and(|(a, b)| a <= int(i) && int(i) < b) {
            text::not_color_bits(s, (gx, gy), (gx + adv, gy + height), clip);
        }
        x += adv;
    }
    if !ui.caret_shown(h) {
        return;
    }
    let (cx, cy) = caret.unwrap_or((x, y));
    let font_h = (t.font != 0)
        .then(|| ui.assets().font(t.font))
        .flatten()
        .map_or(0, |f| i32::try_from(f.max_height).unwrap_or(0));
    let r = Rect::new(
        area.x0 + cx,
        area.y0 + cy,
        area.x0 + cx,
        area.y0 + cy - 1 + font_h,
    );
    if let Some(c) = r.intersect(clip) {
        let color = t.font_color.bgra();
        for py in c.y0..=c.y1 {
            for px in c.x0..=c.x1 {
                s.put(px, py, color);
            }
        }
    }
}
