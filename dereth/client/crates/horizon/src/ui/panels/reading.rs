//! Reading a book, scroll, letter or sign; the salvage window; and the spell components, doing
//! what the game's book, salvage and component panels do.
//!
//! A book opens when the game opens one, shows a page at a time with its author, asks the game
//! for a page it does not yet hold, and closes when the player walks away or closes it. The
//! salvage window opens when a salvage tool is used: items dropped on it are gathered (the game
//! decides which it takes), and Salvage breaks them down. The components tab lists every
//! component with how many are carried and how many the player wants kept, which the game's
//! component buying fills to.

use dereth_client_contract::panels::salvage::SalvageAction;
use dereth_client_contract::UiRequest;

use super::{WindowId, Windows};
use crate::art::Family;
use crate::draw::{Rect, WHITE};
use crate::ui::game::GameState;
use crate::ui::input::vk;
use crate::ui::kit::{self, Ctx, Drop};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

impl Windows {
    /// The book and salvage windows follow the game: open with it, closed with it, and closing
    /// either closes it in the game.
    pub(super) fn follow_reading(
        &mut self,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        match (&state.book, self.book_opening) {
            (Some(b), seen) if seen != Some(b.opening) => {
                self.open(WindowId::Book, ctx.time);
                self.book_opening = Some(b.opening);
                self.book_writing = None;
            }
            (None, Some(_)) => {
                self.close(WindowId::Book);
                self.book_opening = None;
            }
            (Some(b), Some(_)) if !self.is_open(WindowId::Book) => {
                // Closing the window closes the book in the game, saving the page written on.
                out.requests.push(UiRequest::Book(
                    dereth_client_contract::book::BookAction::Close { book: b.book_id },
                ));
                self.book_opening = None;
                self.book_writing = None;
                out.book_closed = true;
            }
            _ => {}
        }
        match (&state.salvage, self.salvage_shown) {
            (Some(_), false) => {
                self.open(WindowId::Salvage, ctx.time);
                self.salvage_shown = true;
            }
            (None, true) => {
                self.close(WindowId::Salvage);
                self.salvage_shown = false;
            }
            (Some(_), true) if !self.is_open(WindowId::Salvage) => {
                out.requests
                    .push(UiRequest::SalvageList(SalvageAction::Close));
                self.salvage_shown = false;
            }
            _ => {}
        }
    }

    /// The book open in the game, as the game's book session has it: the page it is turned to,
    /// that page's text, whether the player may write on it, and whether the game is still to
    /// send it. Writing edits the page; turning past the last page adds one; a page of the
    /// player's own left blank is deleted when the book turns away from it.
    #[allow(clippy::too_many_lines)]
    pub(super) fn book(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        use dereth_client_contract::book::{author_label, BookAction};
        let Some(b) = &state.book else {
            return;
        };
        let session = &state.book_session;
        let book = b.book_id;
        let k = p.scale;
        let text = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        // The title is the window's; the page fills the window down to the page count and the
        // buttons.
        let foot = body.bottom() - 32.0 * k;
        let area = Rect::new(
            body.x,
            body.y + 2.0 * k,
            body.w,
            foot - 28.0 * k - body.y - 2.0 * k,
        );
        let page = usize::try_from(session.current_page).ok();
        let writable = session.editable && !session.pending;
        let pressed_in = writable && ctx.input.clicked(&area);
        if pressed_in {
            self.book_writing = Some(session.current_page);
        }
        let focused = writable && self.book_writing == Some(session.current_page);
        // Where the page's lines are written.
        let (page_x, page_top, page_w) = (area.x + 10.0 * k, area.y + 10.0 * k, area.w - 20.0 * k);
        let lh = p.line_height(&text);
        let lines_fit = |p: &Painter<'_>, words: &str| {
            let lh = p.line_height(&text);
            let lines: usize = words
                .split('\n')
                .map(|para| p.wrap(&text, para, area.w - 20.0 * k).len().max(1))
                .sum();
            #[allow(clippy::cast_precision_loss)]
            let need = lh * lines as f32;
            need <= area.h - 44.0 * k
        };
        if focused {
            use crate::ui::edit;
            ctx.input.text_focus = true;
            let mut draft = session.draft.clone();
            #[allow(clippy::cast_sign_loss)]
            let key = edit::line_key(area, 0xB00C_0000 ^ session.current_page as u32);
            edit::begin(ctx.input, key, &draft);
            let kept = ctx.input.edit.clone();
            let laid = edit::rows(p, &text, &draft, page_w);
            edit::lines_pointer(
                p,
                ctx.input,
                &text,
                &draft,
                &laid,
                (page_x, page_top),
                lh,
                pressed_in,
            );
            edit::lines_keys(p, &text, ctx.input, &mut draft, usize::MAX, page_w);
            if ctx.input.take_key(vk::ESCAPE) {
                self.book_writing = None;
            }
            // A page takes what fits on it, as the game's page does.
            if draft != session.draft {
                if lines_fit(p, &draft) {
                    out.requests.push(UiRequest::Book(BookAction::Edit {
                        book,
                        page: session.current_page,
                        text: draft,
                    }));
                } else {
                    ctx.input.edit = kept;
                }
            }
        }
        if session.pending {
            p.text_in(&dim, area, Align::Centre, "...");
        } else if session.draft.is_empty() && !focused {
            p.text_in(
                &dim,
                area,
                Align::Centre,
                if writable {
                    "This page is blank. Click to write on it."
                } else {
                    "This page is blank."
                },
            );
        } else {
            // The page's own lines, the rows past its foot not drawn.
            let laid = crate::ui::edit::rows(p, &text, &session.draft, page_w);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let fit = ((area.bottom() - 24.0 * k - page_top) / lh).max(0.0) as usize;
            crate::ui::edit::draw_lines(
                p,
                focused.then_some(&ctx.input.edit),
                &text,
                &session.draft,
                &laid[..laid.len().min(fit)],
                (page_x, page_top),
                lh,
                area,
                focused && (ctx.time * 2.0).fract() < 0.5,
            );
        }
        if let Some(pg) = page.and_then(|i| b.pages.get(i)) {
            let author = author_label(&pg.author_name, &pg.author_account, b.viewer_is_psr);
            if !author.is_empty() {
                p.text_in(
                    &dim,
                    Rect::new(
                        area.x,
                        area.bottom() - 22.0 * k,
                        area.w - 10.0 * k,
                        18.0 * k,
                    ),
                    Align::Right,
                    &author,
                );
            }
        }
        let turn = |page: i32| UiRequest::Book(BookAction::Turn { book, page });
        let bw = 92.0 * k;
        if kit::button(
            p,
            ctx,
            Rect::new(body.x + 8.0 * k, foot, bw, 28.0 * k),
            "Previous",
            session.current_page > 0 && !session.pending,
        ) {
            out.requests.push(turn(session.current_page - 1));
            self.book_writing = None;
        }
        let pages = i32::try_from(b.pages.len()).unwrap_or(i32::MAX);
        let max = i32::try_from(b.max_num_pages).unwrap_or(i32::MAX);
        p.text_in(
            &dim,
            Rect::new(body.x, foot - 26.0 * k, body.w, 22.0 * k),
            Align::Centre,
            &format!("Page {} of {}", session.current_page + 1, pages.max(1)),
        );
        // A new page: the book turned to one past its last, which the game adds.
        let room = pages < max && !session.pending;
        if kit::button(
            p,
            ctx,
            Rect::new(body.x + 16.0 * k + bw, foot, bw, 28.0 * k),
            "New Page",
            room,
        ) {
            out.requests.push(turn(pages));
            self.book_writing = Some(pages);
        }
        // Deleting the page: blanked, and the book turned back to itself, which saves it; a
        // blank page of the player's own is deleted on saving.
        let own = page
            .and_then(|i| b.pages.get(i))
            .is_some_and(|pg| pg.author_id == b.player_id);
        if kit::button(
            p,
            ctx,
            Rect::new(body.right() - 24.0 * k - 2.0 * bw, foot, bw, 28.0 * k),
            "Delete Page",
            writable && own,
        ) {
            out.requests.push(UiRequest::Book(BookAction::Edit {
                book,
                page: session.current_page,
                text: String::new(),
            }));
            out.requests
                .push(UiRequest::Book(BookAction::Flush { book }));
            self.book_writing = None;
        }
        if kit::button(
            p,
            ctx,
            Rect::new(body.right() - 8.0 * k - bw, foot, bw, 28.0 * k),
            "Next",
            session.current_page + 1 < max && !session.pending,
        ) {
            out.requests.push(turn(session.current_page + 1));
            self.book_writing = None;
        }
    }

    pub(super) fn salvage(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let Some((_, items)) = &state.salvage else {
            return;
        };
        let k = p.scale;
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        // The instruction, broken to the window's width.
        let lh = p.line_height(&dim);
        let mut y = body.y;
        for line in p.wrap(
            &dim,
            "Drag the items to salvage here. A click takes one back.",
            body.w - 16.0 * k,
        ) {
            p.text(&dim, body.x + 8.0 * k, y, &line);
            y += lh;
        }
        let area = Rect::new(
            body.x,
            y + 6.0 * k,
            body.w,
            body.bottom() - 40.0 * k - y - 6.0 * k,
        );
        ctx.drops.push((area, Some(Drop::Salvage)));
        // The game takes as many items as are dropped: the slots grow by rows, always with one
        // free, and scroll when there are more than the window shows.
        let slot = 44.0 * k;
        let gap = 6.0 * k;
        let cols = 6;
        let slots = salvage_slots(items.len(), cols);
        #[allow(clippy::cast_precision_loss)]
        let content = (slots / cols) as f32 * (slot + gap) + 6.0 * k;
        let offset = kit::scroll(p, ctx, area, content, &mut self.salvage_scroll);
        // The slots keep clear of the scrollbar, there or not, so they do not move when it comes.
        let bar = kit::scrollbar_width(p).unwrap_or(4.0) * k + 6.0 * k;
        #[allow(clippy::cast_precision_loss)]
        let x0 = area.x + (area.w - bar - cols as f32 * (slot + gap) + gap) / 2.0;
        p.list.push_clip(area);
        for i in 0..slots {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                x0 + (i % cols) as f32 * (slot + gap),
                area.y + 6.0 * k + (i / cols) as f32 * (slot + gap) - offset,
                slot,
                slot,
            );
            if r.bottom() < area.y || r.y > area.bottom() {
                continue;
            }
            let item = items.get(i);
            // A slot half out of view answers only where it shows.
            let shown = ctx.input.hover(&area);
            if self.item_slot(p, ctx, r, item) && shown {
                if let Some(it) = item {
                    out.requests
                        .push(UiRequest::SalvageList(SalvageAction::Remove(it.id)));
                }
            }
        }
        p.list.pop_clip();
        if kit::button(
            p,
            ctx,
            Rect::new(
                body.right() - 128.0 * k,
                body.bottom() - 32.0 * k,
                120.0 * k,
                28.0 * k,
            ),
            "Salvage",
            !items.is_empty(),
        ) {
            // The game's salvage session sends the list, newest first, and empties it.
            out.requests
                .push(UiRequest::SalvageList(SalvageAction::Submit));
        }
    }

    /// The spell components: each category's components with how many are carried and how many
    /// are wanted, which the player types into the component's box (set when the box lets the
    /// keyboard go, Escape dropping it) or steps with the arrows beside it (by ten with Shift).
    pub(super) fn components(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let name = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let area = Rect::new(body.x, body.y + 34.0 * k, body.w, body.h - 38.0 * k);
        let rows: Vec<&dereth_client_contract::view::ComponentRow> = state
            .components
            .iter()
            .flat_map(|c| c.rows.iter())
            .collect();
        let row_h = 40.0 * k;
        #[allow(clippy::cast_precision_loss)]
        let content = row_h * rows.len() as f32;
        let offset = kit::scroll(p, ctx, area, content, &mut self.components_scroll);
        // The rows keep clear of the scrollbar, there or not, so they do not move when it comes.
        let bar = kit::scrollbar_width(p).unwrap_or(4.0) * k + 6.0 * k;
        let right = area.right() - bar;
        p.list.push_clip(area);
        if rows.is_empty() {
            p.text(
                &dim,
                area.x + 8.0 * k,
                area.y + 4.0 * k,
                "No components known.",
            );
        }
        for (i, c) in rows.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let y = area.y + i as f32 * row_h - offset;
            if y + row_h < area.y || y > area.bottom() {
                continue;
            }
            if let Some(icon) = c.icon.and_then(|d| p.art.ac_component(d.0)) {
                p.sprite(
                    &icon,
                    Rect::new(area.x + 6.0 * k, y + 4.0 * k, 32.0 * k, 32.0 * k),
                    WHITE,
                );
            }
            let side = 24.0 * k;
            let down = Rect::new(right - side, y + (row_h - side) / 2.0, side, side);
            let up = Rect::new(down.x - side - 2.0 * k, down.y, side, side);
            let shown = Rect::new(up.x - 68.0 * k, y + 6.0 * k, 64.0 * k, 28.0 * k);
            let carried = Rect::new(area.x, y, shown.x - 18.0 * k - area.x, row_h);
            p.text_in(
                &name,
                Rect::new(area.x + 48.0 * k, y, carried.w - 48.0 * k, row_h),
                Align::Left,
                &c.name,
            );
            p.text_in(&dim, carried, Align::Right, &format!("{} carried", c.owned));
            self.wanted_box(p, ctx, shown, c, out);
            // Wanted: one more or one less (ten with Shift), within the game's bounds.
            let step = if ctx.input.shift { 10 } else { 1 };
            if kit::step_button(p, ctx, up, true, "", c.desired < MOST_WANTED) {
                out.requests.push(UiRequest::SetDesiredComponentLevel {
                    wcid: c.wcid,
                    level: (c.desired + step).min(MOST_WANTED),
                });
            }
            if kit::step_button(p, ctx, down, false, "", c.desired > 0) {
                out.requests.push(UiRequest::SetDesiredComponentLevel {
                    wcid: c.wcid,
                    level: (c.desired - step).max(0),
                });
            }
        }
        p.list.pop_clip();
    }

    /// A component's wanted count, as a box that shows it and takes a typed one: a click empties
    /// it for typing, and what is typed is set when Enter is pressed or the box lets the keyboard
    /// go, unless Escape let it go.
    fn wanted_box(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        r: Rect,
        c: &dereth_client_contract::view::ComponentRow,
        out: &mut Outcome,
    ) {
        let field = component_field(c.wcid);
        let mine = self.typing_field == field;
        let dropped = mine && ctx.input.keys.contains(&vk::ESCAPE);
        let mut text = if mine {
            std::mem::take(&mut self.component_typed)
        } else {
            c.desired.to_string()
        };
        let entered = kit::text_box_aligned(
            p,
            ctx,
            r,
            &mut text,
            &mut self.typing_field,
            field,
            crate::ui::paint::Align::Right,
        );
        text.retain(|ch| ch.is_ascii_digit());
        text.truncate(4);
        let held = self.typing_field == field;
        if mine && (entered || !held) {
            if !dropped {
                if let Some(level) = wanted_typed(&text).filter(|l| *l != c.desired) {
                    out.requests.push(UiRequest::SetDesiredComponentLevel {
                        wcid: c.wcid,
                        level,
                    });
                }
            }
            if held {
                self.typing_field = 0;
            }
            self.component_typing = None;
        } else if held {
            // Just taken, the box starts empty, so what is typed replaces the count.
            self.component_typed = if mine { text } else { String::new() };
            self.component_typing = Some(c.wcid);
        }
    }

    /// The components tab put away: a count being typed is set, as when its box lets go.
    pub(super) fn leave_components(&mut self, state: &GameState, out: &mut Outcome) {
        let Some(wcid) = self.component_typing.take() else {
            return;
        };
        if self.typing_field == component_field(wcid) {
            self.typing_field = 0;
        }
        let held = state
            .components
            .iter()
            .flat_map(|c| c.rows.iter())
            .find(|r| r.wcid == wcid)
            .map(|r| r.desired);
        if let (Some(level), Some(held)) = (wanted_typed(&self.component_typed), held) {
            if level != held {
                out.requests
                    .push(UiRequest::SetDesiredComponentLevel { wcid, level });
            }
        }
        self.component_typed.clear();
    }
}

/// How many slots the salvage window shows for `items` items in rows of `cols`: two rows at
/// least, and always a free slot for the next item.
fn salvage_slots(items: usize, cols: usize) -> usize {
    (items + 1).max(2 * cols).div_ceil(cols) * cols
}

/// The most of a component the game lets the player want kept.
const MOST_WANTED: i32 = 5000;

/// The text-field id of component `wcid`'s wanted box.
const fn component_field(wcid: u32) -> u32 {
    0x4300_0000 | (wcid & 0x00FF_FFFF)
}

/// The wanted count typed, held to what the game allows; `None` while nothing is typed.
fn wanted_typed(text: &str) -> Option<i32> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    // A number too long for the box is more than the game allows.
    Some(
        digits
            .parse::<i32>()
            .unwrap_or(MOST_WANTED)
            .clamp(0, MOST_WANTED),
    )
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;

    #[test]
    fn the_salvage_window_grows_by_rows_and_always_keeps_a_slot_free() {
        assert_eq!(salvage_slots(0, 6), 12, "two rows when empty");
        assert_eq!(salvage_slots(11, 6), 12);
        assert_eq!(salvage_slots(12, 6), 18, "a full grid gets another row");
        assert_eq!(salvage_slots(40, 6), 42);
        assert_eq!(salvage_slots(200, 6), 204, "the game sets no limit");
    }

    #[test]
    fn a_typed_wanted_count_is_held_to_what_the_game_allows() {
        assert_eq!(wanted_typed(""), None, "nothing typed sets nothing");
        assert_eq!(wanted_typed("0"), Some(0));
        assert_eq!(wanted_typed("025"), Some(25));
        assert_eq!(wanted_typed("9999"), Some(MOST_WANTED));
        assert_eq!(wanted_typed("99999999999"), Some(MOST_WANTED));
    }
}
