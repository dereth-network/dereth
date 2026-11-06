//! A book (or a note, a sign) read: its title, its pages and their headings, a foreground picture,
//! turned with the slider, its arrows or a press on the page.
//!
//! The scripts give the book's title, its text (pages ending in `<BR>`, each with an optional
//! heading between `<H>` and `</H>`), how it looks (the page's state), whether it shows its page
//! controls, and its picture. The window lays the pages out in the page text a page at a time
//! (each page padded to a whole number of the text's lines per page), each heading in the heading
//! column beside its page (cut to the column's width), and counts the pages (at most a thousand).
//! With controls it shows "page n of m" and the slider; a press on the picture turns to the next
//! page, closing the book after the last. Copy puts the book's title and text on the clipboard
//! (headings as they were marked, pages with blank lines between).

use std::any::Any;

use auberean_text::{StringInfo, Value as Var};
use auberean_ui::framework::FrameId;
use auberean_ui::system::Value;
use auberean_ui::token::{msg, state};
use auberean_ui::{Handle, Mail, Token, Ui};

use crate::gameplay::{Child, Notice, Record, Request, PLAY_LAYOUT_ENUM};
use crate::inventory;

/// The scripts' notice to the book: whether it shows its controls, how it looks, its title (the
/// record), its text (the record), its picture.
pub const USE_BOOK: u32 = 0x0092 << 16 | 0x0002;

mod name {
    use auberean_ui::Token;
    pub const BOOK: Token = Token::of("Book");
    pub const TITLE: Token = Token::of("Panel_Title_Book");
    pub const TEXT: Token = Token::of("BookClientAreaText");
    pub const HEADER: Token = Token::of("BookClientAreaTextHeader");
    pub const SLIDER: Token = Token::of("BookSliderBar");
    pub const PAGE_NUMBER: Token = Token::of("PageNumberText");
    pub const CLIENT_AREA: Token = Token::of("BookClientArea");
    pub const CONTROLS: Token = Token::of("BookBottom");
    pub const FOREGROUND: Token = Token::of("BookForeground");
    pub const CLOSE: Token = Token::of("BookCloseButton");
    pub const HELP: Token = Token::of("BookHelpButton");
    pub const COPY: Token = Token::of("BookCopyButton");
    pub const SLIDER_LEFT: Token = Token::of("BookSliderLeft");
    pub const SLIDER_RIGHT: Token = Token::of("BookSliderRight");
    pub const LINES_PER_PAGE: Token = Token::of("lines_per_page");
    pub const X_POSITION: Token = Token::of("x_position");
    pub const NEW_POSITION: Token = Token::of("new_position");
    pub const MOUSE_PRESS_TOP: Token = Token::of("mouse_press_top");
    /// The look a book starts with.
    pub const DIALOG: Token = Token::of("Dialog");
    pub const CURRENT: Token = Token::of("CURRENT");
    pub const LAST: Token = Token::of("LAST");
}

/// The most pages a book counts.
const MOST_PAGES: u32 = 1000;

/// A book read.
#[derive(Debug)]
pub struct Book {
    frame: FrameId,
    field: Option<Handle>,
    title: Option<Handle>,
    text: Option<Handle>,
    header: Option<Handle>,
    slider: Option<Handle>,
    page_number: Option<Handle>,
    current: u32,
    last: u32,
    page_number_text: StringInfo,
    show_controls: bool,
    lines_per_page: i32,
}

impl Book {
    /// Its framework, the scripts' book notice, its buttons, slider and picture heard; hidden.
    // D2-02269
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        if let Some(l) = ui.frame_listener(frame) {
            for b in [
                name::CLOSE,
                name::HELP,
                name::COPY,
                name::SLIDER_LEFT,
                name::SLIDER_RIGHT,
            ] {
                ui.listen_for_message(l, b, msg::BUTTON_RELEASE_ON);
            }
            ui.listen_for_message(l, name::SLIDER, name::NEW_POSITION);
            ui.listen_for_message(l, name::FOREGROUND, name::MOUSE_PRESS_TOP);
        }
        let mut me = Self {
            frame,
            field: None,
            title: None,
            text: None,
            header: None,
            slider: None,
            page_number: None,
            current: 1,
            last: 1,
            page_number_text: StringInfo::default(),
            show_controls: false,
            lines_per_page: 0,
        };
        me.show(ui, false, &mut Vec::new());
        me
    }

    /// The page shown and how many there are.
    #[must_use]
    pub const fn page(&self) -> (u32, u32) {
        (self.current, self.last)
    }

    /// Its window made when first shown, with its title, page text and headings, slider and page
    /// number.
    // D2-02260
    fn create_managed_elements(&mut self, ui: &mut Ui) -> bool {
        if self.field.is_some() {
            return true;
        }
        self.field = ui.frame_create_root(self.frame, PLAY_LAYOUT_ENUM, name::BOOK);
        let Some(f) = self.field else { return false };
        self.title = ui.child_recursive(f, name::TITLE);
        self.text = ui.child_recursive(f, name::TEXT);
        self.header = ui.child_recursive(f, name::HEADER);
        self.slider = ui.child_recursive(f, name::SLIDER);
        self.page_number = ui.child_recursive(f, name::PAGE_NUMBER);
        self.title.is_some()
            && self.text.is_some()
            && self.header.is_some()
            && self.slider.is_some()
            && self.page_number.is_some()
    }

    /// The slider at the page: the first at its start, the last at its end.
    // D2-02262
    fn synch_slider_bar(&self, ui: &mut Ui) {
        let Some(s) = self.slider else { return };
        #[allow(clippy::cast_precision_loss)]
        let (at, of) = (
            self.current.wrapping_sub(1) as f32,
            self.last.wrapping_sub(1) as f32,
        );
        let pos = if 0.0 < of && at <= of { at / of } else { 0.0 };
        ui.set_attr(s, name::X_POSITION, Value::Float(pos));
    }

    /// The page before (not before the first); whether it turned.
    // D2-02263
    fn previous_page(&mut self) -> bool {
        if self.current > 1 {
            self.current -= 1;
            return true;
        }
        false
    }

    /// The page after (not after the last); whether it turned.
    // D2-02264
    fn next_page(&mut self) -> bool {
        if self.current < self.last {
            self.current += 1;
            return true;
        }
        false
    }

    /// The page the slider stands at (rounded to the nearest, a half down).
    // D2-02265
    fn set_page_from_slider(&mut self, ui: &Ui) {
        let Some(s) = self.slider else { return };
        let pos = ui
            .node(s)
            .and_then(|n| n.float_attr(name::X_POSITION))
            .unwrap_or(0.0);
        #[allow(clippy::cast_precision_loss)]
        let x = self.last.wrapping_sub(1) as f32 * pos;
        let wide = f64::from(self.last.wrapping_sub(1) as f32) * f64::from(pos);
        let frac = f64::from(x) - wide.floor();
        let page = if frac > 0.5 { x + 2.0 } else { x + 1.0 };
        self.current = dereth_primitives::num::to_i32(page).cast_unsigned();
    }

    /// How the pages look (the page area, the text and the headings in that state); the controls
    /// shown when the book shows them.
    // D2-02266
    fn set_mode(&self, ui: &mut Ui, mode: Token) {
        let Some(f) = self.field else { return };
        if let Some(a) = ui.child_recursive(f, name::CLIENT_AREA) {
            ui.set_state(a, mode);
        }
        for e in [self.text, self.header].into_iter().flatten() {
            ui.set_state(e, mode);
        }
        if let Some(c) = ui.child_recursive(f, name::CONTROLS) {
            ui.set_visible(c, self.show_controls);
        }
    }

    /// Back to the first of one page: the title, text and headings cleared, the picture cleared,
    /// the first look.
    // D2-02267
    fn reset(&mut self, ui: &mut Ui) {
        self.current = 1;
        self.last = 1;
        for e in [self.title, self.text, self.header].into_iter().flatten() {
            ui.clear_all_text(e);
        }
        if let Some(fg) = self
            .field
            .and_then(|f| ui.child_recursive(f, name::FOREGROUND))
        {
            ui.clear_image(fg);
        }
        self.set_mode(ui, name::DIALOG);
    }

    /// The page shown: with controls, "page n of m" and the slider; the text and the headings
    /// scrolled to the page.
    // D2-02271
    fn update_book(&mut self, ui: &mut Ui) {
        if self.show_controls {
            if let Some(p) = self.page_number {
                self.page_number_text
                    .add_var(name::CURRENT.raw(), Var::Int(self.current.cast_signed()));
                let t = ui
                    .assets()
                    .write(&self.page_number_text)
                    .unwrap_or_default();
                ui.set_text(p, &t);
            }
            self.synch_slider_bar(ui);
        }
        let (Some(t), Some(h)) = (self.text, self.header) else {
            return;
        };
        for e in [t, h] {
            let height = ui.node(e).map_or(0, |n| n.rect.height());
            let page = self.current.cast_signed() - 1;
            ui.set_view_y(e, height * page);
        }
    }

    /// The text and headings laid out from the book's text: each page (up to `<BR>`) with its
    /// heading (between `<H>` and `</H>`, a lone line when it has none) a whole number of pages
    /// of the text's lines; the page count kept (at most a thousand). False when the window has
    /// no page text or counts no lines per page.
    // D2-02277, D2-02275, D2-02276
    fn parse_text(&mut self, ui: &mut Ui, given: &str) -> bool {
        let (Some(text), Some(header)) = (self.text, self.header) else {
            return false;
        };
        ui.clear_all_text(text);
        ui.clear_all_text(header);
        self.last = 1;
        self.lines_per_page = ui
            .node(text)
            .and_then(|n| n.int_attr(name::LINES_PER_PAGE))
            .unwrap_or(0);
        if self.lines_per_page <= 0 {
            return false;
        }
        let per = self.lines_per_page.cast_unsigned() as usize;
        let (headings, pages) = split_pages(given);
        let mut counted = 0u32;
        // The heading column's width less its scroll bar's.
        let width = ui.node(header).map_or(0, |n| n.rect.width()) - 0x13;
        for (page, heading) in pages.iter().zip(headings.iter()) {
            if heading != "\n" {
                ui.add_string(text, "\n");
            }
            ui.add_string(text, page);
            ui.recalculate_glyph_list(text);
            let total = ui.total_lines(text);
            let pad = if total.is_multiple_of(per) {
                0
            } else {
                per - total % per
            };
            if pad != 0 {
                ui.add_string(text, &"\n".repeat(pad));
            }
            let cut = truncate_text(ui, heading, header, width);
            ui.add_string(header, &cut);
            let these = u32::try_from((pad + total) / per).unwrap_or(u32::MAX) - counted;
            if these > 1 {
                ui.add_string(header, &"\n".repeat((these - 1) as usize));
            }
            counted += these;
        }
        ui.add_string(header, ".");
        ui.recalculate_glyph_list(text);
        self.last = counted.min(MOST_PAGES);
        true
    }

    /// A book opened: shown, back to its start, with its controls or not, its title, its look,
    /// its pages ("of m" kept for the page number), its picture; the first page.
    // D2-02278
    fn use_book(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        self.show(ui, true, out);
        self.reset(ui);
        self.show_controls = n.word_at(0) != 0;
        let texts: Vec<String> = n
            .records
            .iter()
            .flatten()
            .filter_map(|r| match r {
                Record::Text(t) => Some(ui.assets().write(t).unwrap_or_default()),
                _ => None,
            })
            .collect();
        let (title, body) = (
            texts.first().cloned().unwrap_or_default(),
            texts.get(1).cloned().unwrap_or_default(),
        );
        if let Some(t) = self.title {
            ui.set_text(t, &title);
        }
        self.set_mode(ui, Token(n.word_at(1)));
        if !body.is_empty() && self.parse_text(ui, &body) {
            let table = ui
                .assets()
                .data_id_by_enum(crate::flow::MESSAGES, crate::flow::TABLE_MAPPING);
            self.page_number_text =
                StringInfo::entry(table, Token::of("ID_Book_PageNumberText").raw());
            self.page_number_text
                .add_var(name::LAST.raw(), Var::Int(self.last.cast_signed()));
        }
        if let Some(fg) = self
            .field
            .and_then(|f| ui.child_recursive(f, name::FOREGROUND))
        {
            ui.clear_image(fg);
            let picture = n.word_at(4);
            if picture != 0 {
                ui.add_image_to_state(fg, picture, 2, state::STATELESS);
                ui.set_center_image(fg, true);
            }
        }
        self.update_book(ui);
    }

    /// The book's title and text on the clipboard: the title and a blank line, then the text from
    /// its first line on (after the title's marks), headings back in their marks, pages ending in
    /// blank lines.
    // D2-02280
    fn send_book_to_clipboard(&self, ui: &mut Ui) {
        let Some(text) = self.text else { return };
        let mut s = self.title.and_then(|t| ui.text(t)).unwrap_or_default();
        if !s.is_empty() {
            s.push_str("\n\n");
        }
        s.push_str(&ui.text(text).unwrap_or_default());
        if let Some(i) = s.find("<T>") {
            s = s[i + 3..].to_owned();
        }
        let s = s
            .replace("<H>", "")
            .replace("</H>", "\n")
            .replace("<BR>", "\n\n\n");
        ui.send_to_clipboard(s);
    }
}

/// A book's text split into pages (each up to `<BR>`, trimmed at its end, a new line after) and
/// their headings (between `<H>` and `</H>` when the heading closes after it opens, trimmed at its
/// end, a new line after; else a lone new line). What follows the last `<BR>` is not a page.
#[must_use]
pub fn split_pages(given: &str) -> (Vec<String>, Vec<String>) {
    let mut rest = given.to_owned();
    let (mut headings, mut pages) = (Vec::new(), Vec::new());
    while let Some(br) = rest.find("<BR>") {
        let mut page = rest[..br].to_owned();
        match (page.find("<H>"), page.find("</H>")) {
            (Some(h), Some(end)) if h < end => {
                let mut heading = page[h + 3..end].trim_end().to_owned();
                heading.push('\n');
                headings.push(heading);
                let after = &page[end + 4..];
                let skip = usize::from(after.starts_with('\n'));
                page = after[skip..].to_owned();
            }
            _ => headings.push("\n".to_owned()),
        }
        let mut body = page.trim_end().to_owned();
        body.push('\n');
        pages.push(body);
        rest = rest[br + 4..].to_owned();
    }
    (headings, pages)
}

/// A heading cut to a width: when it and a space are wider than `width` (and the width is
/// some), letters come off its end until what is left and a space fit, and it ends with a space.
fn truncate_text(ui: &mut Ui, heading: &str, element: Handle, width: i32) -> String {
    let mut wide = |s: &str| {
        ui.inq_size_with_margins(element, &format!("{s} "))
            .map_or(0, |(w, _)| w)
    };
    if !(width < wide(heading) && 0 < width) {
        return heading.to_owned();
    }
    let mut cut: Vec<char> = heading.chars().collect();
    while width < wide(&cut.iter().collect::<String>()) {
        if cut.pop().is_none() {
            break;
        }
    }
    let mut out: String = cut.into_iter().collect();
    out.push(' ');
    out
}

impl Book {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x479];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for Book {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown (its window made, shown as the framework shows) or hidden (its window let go).
    // D2-02261
    fn show(&mut self, ui: &mut Ui, on: bool, _out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        if !on {
            if let Some(f) = self.field.take() {
                ui.frame_remove_root(self.frame, f);
                ui.remove(f);
            }
            self.title = None;
            self.text = None;
            self.header = None;
            self.slider = None;
            self.page_number = None;
        } else if self.create_managed_elements(ui) {
            if let Some(f) = self.field {
                let shown = ui.frame_shown(self.frame);
                ui.set_visible(f, shown);
            }
        }
    }

    /// Close (hidden); the picture pressed (the next page, or after the last, closed); help; with
    /// controls, the slider's arrows and the slider, copy.
    // D2-02281
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        let Mail::Element { from, element, .. } = *mail else {
            return;
        };
        if element == name::CLOSE {
            self.show(ui, false, out);
            return;
        }
        if !self.shown(ui) {
            return;
        }
        if element == name::FOREGROUND {
            if self.current < self.last {
                self.current += 1;
                self.update_book(ui);
            } else {
                self.show(ui, false, out);
            }
            return;
        }
        if element == name::HELP {
            out.push(Request::Inventory(inventory::Request::Help {
                entry: Token::of("ID_Help_Book"),
                table: inventory::HELP_TABLE,
            }));
            return;
        }
        if !self.show_controls {
            return;
        }
        if element == name::SLIDER_LEFT {
            self.previous_page();
            self.update_book(ui);
        } else if element == name::SLIDER_RIGHT {
            self.next_page();
            self.update_book(ui);
        } else if Some(from) == self.slider {
            self.set_page_from_slider(ui);
            self.update_book(ui);
        } else if element == name::COPY {
            self.send_book_to_clipboard(ui);
        }
    }

    /// The scripts open a book.
    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        if n.native == USE_BOOK {
            self.use_book(ui, n, out);
        }
    }

    /// It goes: its window gone.
    // D2-02270
    fn close(mut self: Box<Self>, ui: &mut Ui) {
        self.show(ui, false, &mut Vec::new());
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
