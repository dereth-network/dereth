//! The book window: a book, a note or a sign the game opened, read a page at a time.
//!
//! The game gives a book's title, its text (each of its pages ending `<BR>`, with an optional
//! heading between `<H>` and `</H>`), its picture, and whether it shows its page controls. The
//! window lays each of the book's pages out to its width, a long one over as many of the window's
//! pages as it needs, its heading over the first. A press on the page turns to the next, closing
//! the book after the last. With controls it shows "Page n of m", the arrows to turn back and on,
//! and Copy, which puts the title and the text on the clipboard.

use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::draw::{Rect, WHITE};
use crate::journal::Book;
use crate::state::GameState;

/// The window's body, in layout units.
const BODY: (f32, f32) = (440.0, 520.0);

/// The picture's side, at the top of the page, in layout units.
const PICTURE: f32 = 96.0;

/// A book's text split into its pages, each its heading (when it has one) and its words: a page
/// ends at `<BR>` (what follows the last is not a page); its heading is what stands between `<H>`
/// and `</H>` when it closes after it opens. A text with no `<BR>` is one page.
#[must_use]
pub fn split_pages(text: &str) -> Vec<(Option<String>, String)> {
    let mut pages = Vec::new();
    let mut rest = text;
    while let Some(br) = rest.find("<BR>") {
        pages.push(page(&rest[..br]));
        rest = &rest[br + 4..];
    }
    if pages.is_empty() && !text.trim().is_empty() {
        pages.push(page(text));
    }
    pages
}

fn page(text: &str) -> (Option<String>, String) {
    match (text.find("<H>"), text.find("</H>")) {
        (Some(h), Some(end)) if h < end => {
            let heading = text[h + 3..end].trim().to_owned();
            let after = &text[end + 4..];
            let body = after.strip_prefix('\n').unwrap_or(after);
            (Some(heading), body.trim_end().to_owned())
        }
        _ => (None, text.trim_end().to_owned()),
    }
}

/// A book's title and text as Copy puts them on the clipboard: the title and a blank line, then
/// each page, its heading on a line of its own, pages apart by blank lines.
#[must_use]
pub fn copied(book: &Book) -> String {
    let mut s = book.title.clone();
    if !s.is_empty() {
        s.push_str("\n\n");
    }
    let pages: Vec<String> = split_pages(&book.text)
        .into_iter()
        .map(|(h, body)| match h {
            Some(h) => format!("{h}\n{body}"),
            None => body,
        })
        .collect();
    s.push_str(&pages.join("\n\n\n"));
    s
}

/// A page of the window: the heading over it (on the first of a book's page), and its lines.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Leaf {
    pub heading: Option<String>,
    pub lines: Vec<String>,
}

/// Where the window's parts are, in a window drawn with `body` as its body, at `k` pixels a
/// layout unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parts {
    /// The page, which a press turns.
    pub page: Rect,
    /// The arrows, the page number between them, Copy.
    pub back: Rect,
    pub on: Rect,
    pub number: Rect,
    pub copy: Rect,
}

/// The book window.
#[derive(Debug, Default)]
pub struct BookWindow {
    window: WindowState,
    drawn: Option<(Rect, Rect)>,
    /// The book shown, by how many books had opened when it did.
    pub opened: u64,
    /// The page shown, from 0, and the pages as laid out at the width they were laid out at.
    pub page: usize,
    pub leaves: Vec<Leaf>,
    laid_at: Option<(u64, u32)>,
}

impl BookWindow {
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.window.open
    }

    pub fn close(&mut self) {
        self.window.open = false;
        self.drawn = None;
    }

    /// Where it was last drawn, while it is open.
    #[must_use]
    pub fn rect(&self) -> Option<Rect> {
        self.drawn.map(|(r, _)| r)
    }

    /// Where its body was last drawn, while it is open.
    #[must_use]
    pub fn body(&self) -> Option<Rect> {
        self.drawn.map(|(_, b)| b)
    }

    /// Where the parts are in a window drawn with `body` as its body, at `k` pixels a layout
    /// unit.
    #[must_use]
    pub fn parts(body: Rect, k: f32) -> Parts {
        let foot = body.bottom() - 36.0 * k;
        Parts {
            page: Rect::new(
                body.x + 14.0 * k,
                body.y + 10.0 * k,
                body.w - 28.0 * k,
                foot - body.y - 16.0 * k,
            ),
            back: Rect::new(body.x + 14.0 * k, foot, 36.0 * k, 28.0 * k),
            number: Rect::new(body.x + 54.0 * k, foot, 150.0 * k, 28.0 * k),
            on: Rect::new(body.x + 208.0 * k, foot, 36.0 * k, 28.0 * k),
            copy: Rect::new(body.right() - 114.0 * k, foot, 100.0 * k, 28.0 * k),
        }
    }

    /// The pages laid out for a page of `area`, the book's picture taking room at the top.
    fn lay_out(p: &Painter<'_>, book: &Book, area: Rect, body: &TextStyle) -> Vec<Leaf> {
        let k = p.scale;
        let lh = p.line_height(body).max(1.0);
        let top = if book.picture != 0 {
            (PICTURE + 8.0) * k
        } else {
            0.0
        };
        let mut leaves = Vec::new();
        for (heading, text) in split_pages(&book.text) {
            let mut lines = Vec::new();
            for para in text.split('\n') {
                let wrapped = p.wrap(body, para, area.w);
                if wrapped.is_empty() {
                    lines.push(String::new());
                }
                lines.extend(wrapped);
            }
            let room = |first: bool| {
                let head = if first && heading.is_some() {
                    lh * 1.6
                } else {
                    0.0
                };
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let n = ((area.h - top - head) / lh).floor().max(1.0) as usize;
                n
            };
            let mut first = true;
            let mut rest = lines.as_slice();
            loop {
                let n = room(first).min(rest.len());
                leaves.push(Leaf {
                    heading: if first { heading.clone() } else { None },
                    lines: rest[..n].to_vec(),
                });
                rest = &rest[n..];
                first = false;
                if rest.is_empty() {
                    break;
                }
            }
        }
        if leaves.is_empty() {
            leaves.push(Leaf::default());
        }
        leaves
    }

    /// Draw it while it is open, the book the game opened last; where it is. A book opened since
    /// the last frame opens at its first page.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
    ) -> Option<Rect> {
        let book = state.journal.book.as_ref()?;
        if book.opened != self.opened {
            self.opened = book.opened;
            self.page = 0;
            self.laid_at = None;
            self.window.open = true;
        }
        if !self.window.open {
            return None;
        }
        let k = p.scale;
        let size = kit::window_size_for(p, BODY);
        let at = (
            (p.screen.0 * 0.5 - size.0 * k * 0.5).round(),
            ((p.screen.1 - size.1 * k) / 2.0).round(),
        );
        let title = if book.title.is_empty() {
            "Book"
        } else {
            book.title.as_str()
        };
        let at = crate::ui::place::clear(p, state, size, at);
        let w = kit::window(p, ctx, &mut self.window, title, size, at);
        if w.closed {
            self.close();
            return Some(w.rect);
        }
        self.drawn = Some((w.rect, w.body));
        let parts = Self::parts(w.body, k);
        let body = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let head =
            TextStyle::new(Family::Heading, 17.0, ctx.colours.heading()).edge(ctx.colours.edge());
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let width = parts.page.w as u32;
        if self.laid_at != Some((self.opened, width)) {
            self.laid_at = Some((self.opened, width));
            self.leaves = Self::lay_out(p, book, parts.page, &body);
            self.page = self.page.min(self.leaves.len() - 1);
        }
        let mut y = parts.page.y;
        if book.picture != 0 {
            let s = PICTURE * k;
            let r = Rect::new(parts.page.x + (parts.page.w - s) / 2.0, y, s, s);
            if let Some(pic) = p.art.icon(book.picture) {
                p.sprite(&pic, r, WHITE);
            }
            y += s + 8.0 * k;
        }
        let lh = p.line_height(&body);
        if let Some(leaf) = self.leaves.get(self.page) {
            if let Some(h) = &leaf.heading {
                p.text_in(
                    &head,
                    Rect::new(parts.page.x, y, parts.page.w, lh * 1.4),
                    Align::Centre,
                    h,
                );
                y += lh * 1.6;
            }
            for l in &leaf.lines {
                p.text(&body, parts.page.x, y, l);
                y += lh;
            }
        }
        let last = self.leaves.len().saturating_sub(1);
        if ctx.over(&parts.page) && ctx.input.clicked(&parts.page) {
            if self.page < last {
                self.page += 1;
            } else {
                self.close();
                return Some(w.rect);
            }
        }
        if book.controls {
            if kit::button(p, ctx, parts.back, "<", self.page > 0) {
                self.page -= 1;
            }
            if kit::button(p, ctx, parts.on, ">", self.page < last) {
                self.page += 1;
            }
            let dim =
                TextStyle::new(Family::Body, 13.0, ctx.colours.dim()).edge(ctx.colours.edge());
            let n = format!("Page {} of {}", self.page + 1, last + 1);
            p.text_in(&dim, parts.number, Align::Centre, &n);
            if kit::button(p, ctx, parts.copy, "Copy Text", true) {
                ctx.input.copied = Some(copied(book));
            }
        }
        Some(w.rect)
    }
}
