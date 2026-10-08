//! The Journal's Notebook tab: the player's own notes, kept by the game's shared notebook and
//! saved beside the preferences for the character. Each page has a label, a title, notes, a timer
//! of days, hours and minutes, and a place; pages are added, deleted, turned, sorted and searched
//! as the game's journal panel does, every change through the shared notebook.

use dereth_client_contract::journal::{JournalAction, JournalField, JournalView};
use dereth_client_contract::UiRequest;
use dereth_presentation::journal::{location_text, parse_count};

use super::{WindowId, Windows};
use crate::art::Family;
use crate::draw::Rect;
use crate::ui::game::GameState;
use crate::ui::input::vk;
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

/// The text box ids the notebook's fields take the keyboard under.
mod field {
    pub const LABEL: u32 = 41;
    pub const TITLE: u32 = 42;
    pub const DAYS: u32 = 43;
    pub const HOURS: u32 = 44;
    pub const MINUTES: u32 = 45;
    pub const NOTES: u32 = 46;
    pub const SEARCH: u32 = 47;
}

/// The sort orders, as the journal panel numbers them: by page number, title, label and timer.
const SORTS: [(&str, u8); 4] = [("Number", 0), ("Title", 1), ("Label", 2), ("Timer", 3)];

/// One field's edit, for the page the notebook is open to.
fn set(j: &JournalView, field: JournalField) -> UiRequest {
    UiRequest::Journal(JournalAction::SetField {
        generation: j.generation,
        page: j.current_page,
        field,
    })
}

impl Windows {
    /// The game is told when the notebook is on screen, which loads it, and when it is not.
    pub(super) fn follow_notebook(&mut self, state: &GameState, out: &mut Outcome) {
        let shown = state.notebook && self.is_open(WindowId::Journal) && self.journal_tab == 1;
        if shown != self.notebook_shown {
            self.notebook_shown = shown;
            out.requests
                .push(UiRequest::Journal(JournalAction::Visibility(shown)));
        }
    }

    /// The notebook: the pages on the left, with the search and the sort; the open page on the
    /// right, its fields edited in place; and the buttons that turn, add and delete pages, start
    /// and stop the timer and mark the player's place.
    #[allow(clippy::too_many_lines)]
    pub(super) fn notebook(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let j = &state.journal;
        let name = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        if !state.notebook {
            p.text_in(&dim, body, Align::Centre, "This world has no journal.");
            return;
        }
        if !j.loaded {
            p.text_in(&dim, body, Align::Centre, "...");
            return;
        }
        // The pages, as the search and the sort leave them.
        let list = Rect::new(body.x, body.y + 34.0 * k, body.w * 0.36, body.h - 70.0 * k);
        p.fill(list, 0x6010_0C08);
        let search = Rect::new(body.x, body.y, body.w * 0.36 - 64.0 * k, 26.0 * k);
        if kit::text_box(
            p,
            ctx,
            search,
            &mut self.notebook_search,
            &mut self.typing_field,
            field::SEARCH,
        ) {
            out.requests.push(UiRequest::Journal(JournalAction::Search(
                self.notebook_search.clone(),
            )));
        }
        if kit::button(
            p,
            ctx,
            Rect::new(search.right() + 4.0 * k, body.y, 60.0 * k, 26.0 * k),
            if j.filtered { "Reset" } else { "Find" },
            true,
        ) {
            out.requests.push(UiRequest::Journal(if j.filtered {
                self.notebook_search.clear();
                JournalAction::ResetSearch
            } else {
                JournalAction::Search(self.notebook_search.clone())
            }));
        }
        let mut y = list.y + 4.0 * k;
        for page in &j.pages {
            let row = Rect::new(list.x, y, list.w, 22.0 * k);
            if row.bottom() > list.bottom() {
                break;
            }
            let on = page.page_number == j.current_page;
            if on || ctx.input.hover(&row) {
                crate::ui::pregame::list_highlight(p, row, on);
            }
            let caption = if page.title.is_empty() {
                page.label.clone()
            } else {
                page.title.clone()
            };
            p.text_in(
                &name,
                row.offset(8.0 * k, 0.0),
                Align::Left,
                &format!("{}  {caption}", page.page_number),
            );
            if ctx.over(&row) && ctx.input.clicked(&row) {
                out.requests
                    .push(UiRequest::Journal(JournalAction::Goto(page.page_number)));
            }
            y += 22.0 * k;
        }
        let sort_y = list.bottom() + 6.0 * k;
        #[allow(clippy::cast_precision_loss)]
        let sw = list.w / SORTS.len() as f32;
        for (n, (caption, order)) in SORTS.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(list.x + sw * n as f32, sort_y, sw - 2.0 * k, 24.0 * k);
            if kit::button(p, ctx, r, caption, true) {
                out.requests
                    .push(UiRequest::Journal(JournalAction::Sort(*order)));
            }
        }
        // The open page.
        let x = list.right() + 12.0 * k;
        let w = body.right() - x;
        let d = &j.draft;
        let page_shown = j.pages.iter().any(|p| p.page_number == j.current_page);
        p.text(
            &dim,
            x,
            body.y + 4.0 * k,
            &if page_shown {
                format!("Page {} of {}", j.current_page, j.pages.len())
            } else {
                "No page is open.".to_owned()
            },
        );
        let mut row_y = body.y + 30.0 * k;
        let mut text_field = |p: &mut Painter<'_>,
                              ctx: &mut Ctx<'_>,
                              this: &mut Windows,
                              caption: &str,
                              value: &str,
                              id: u32,
                              width: f32|
         -> Option<String> {
            p.text(&dim, x, row_y + 5.0 * k, caption);
            let r = Rect::new(x + 70.0 * k, row_y, width, 26.0 * k);
            let mut text = value.to_owned();
            kit::text_box(p, ctx, r, &mut text, &mut this.typing_field, id);
            row_y += 32.0 * k;
            (text != value).then_some(text)
        };
        if page_shown {
            if let Some(t) = text_field(p, ctx, self, "Label", &d.label, field::LABEL, w - 70.0 * k)
            {
                out.requests.push(set(j, JournalField::Label(t)));
            }
            if let Some(t) = text_field(p, ctx, self, "Title", &d.title, field::TITLE, w - 70.0 * k)
            {
                out.requests.push(set(j, JournalField::Title(t)));
            }
            // The timer: days, hours and minutes, and whether it runs.
            p.text(&dim, x, row_y + 5.0 * k, "Timer");
            for (n, (value, id, make)) in [
                (
                    d.days,
                    field::DAYS,
                    JournalField::Days as fn(u32) -> JournalField,
                ),
                (d.hours, field::HOURS, JournalField::Hours),
                (d.minutes, field::MINUTES, JournalField::Minutes),
            ]
            .into_iter()
            .enumerate()
            {
                #[allow(clippy::cast_precision_loss)]
                let r = Rect::new(
                    x + 70.0 * k + 64.0 * k * n as f32,
                    row_y,
                    56.0 * k,
                    26.0 * k,
                );
                let shown = value.to_string();
                let mut text = shown.clone();
                kit::text_box(p, ctx, r, &mut text, &mut self.typing_field, id);
                if text != shown {
                    out.requests.push(set(j, make(parse_count(&text))));
                }
            }
            let timer = Rect::new(x + 70.0 * k + 200.0 * k, row_y, 90.0 * k, 26.0 * k);
            if kit::button(
                p,
                ctx,
                timer,
                if d.timer_running { "Stop" } else { "Start" },
                true,
            ) {
                out.requests
                    .push(UiRequest::Journal(JournalAction::ToggleTimer));
            }
            row_y += 32.0 * k;
            // The place.
            p.text(&dim, x, row_y + 5.0 * k, "Place");
            let place = location_text(d.location_set, d.ns, d.ew);
            p.text(&name, x + 70.0 * k, row_y + 5.0 * k, &place);
            if kit::button(
                p,
                ctx,
                Rect::new(x + w - 120.0 * k, row_y, 120.0 * k, 26.0 * k),
                "Mark Here",
                true,
            ) {
                out.requests
                    .push(UiRequest::Journal(JournalAction::StampLocation));
            }
            row_y += 34.0 * k;
            // The notes, written in place: a click gives them the keyboard.
            let notes = Rect::new(x, row_y, w, body.bottom() - 40.0 * k - row_y);
            p.fill(notes, 0x9010_0C08);
            let pressed_in = ctx.over(&notes) && ctx.input.clicked(&notes);
            if pressed_in {
                self.typing_field = field::NOTES;
            }
            let focused = self.typing_field == field::NOTES;
            let mut text = d.notes.clone();
            let lh = p.line_height(&name);
            let (notes_x, notes_top, notes_w) =
                (notes.x + 8.0 * k, notes.y + 6.0 * k, notes.w - 16.0 * k);
            if focused {
                use crate::ui::edit;
                ctx.input.text_focus = true;
                edit::begin(ctx.input, edit::line_key(notes, field::NOTES), &text);
                let laid = edit::rows(p, &name, &text, notes_w);
                edit::lines_pointer(
                    p,
                    ctx.input,
                    &name,
                    &text,
                    &laid,
                    (notes_x, notes_top),
                    lh,
                    pressed_in,
                );
                edit::lines_keys(p, &name, ctx.input, &mut text, usize::MAX, notes_w);
                if ctx.input.take_key(vk::ESCAPE) {
                    self.typing_field = 0;
                }
                if text != d.notes {
                    out.requests.push(set(j, JournalField::Notes(text.clone())));
                }
            }
            let laid = crate::ui::edit::rows(p, &name, &text, notes_w);
            crate::ui::edit::draw_lines(
                p,
                focused.then_some(&ctx.input.edit),
                &name,
                &text,
                &laid,
                (notes_x, notes_top),
                lh,
                notes,
                focused && (ctx.time * 2.0).fract() < 0.5,
            );
        }
        // Turning, adding and deleting.
        let foot = body.bottom() - 30.0 * k;
        let bw = (w - 5.0 * 4.0 * k) / 6.0;
        let last = j.pages.iter().map(|p| p.page_number).max().unwrap_or(0);
        let buttons: [(&str, JournalAction, bool); 6] = [
            ("First", JournalAction::Goto(1), page_shown),
            ("Previous", JournalAction::Turn(-1), page_shown),
            ("Next", JournalAction::Turn(1), page_shown),
            ("Last", JournalAction::Goto(last), page_shown),
            ("New Page", JournalAction::NewPage, true),
            ("Delete", JournalAction::Delete(j.current_page), page_shown),
        ];
        for (n, (caption, action, enabled)) in buttons.into_iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(x + (bw + 4.0 * k) * n as f32, foot, bw, 28.0 * k);
            if kit::button(p, ctx, r, caption, enabled) {
                out.requests.push(UiRequest::Journal(action));
            }
        }
    }
}
