//! The Journal (the contracts the player holds, each with its notes, contact and places, and
//! Abandon; and the notebook, in its own module), and the Profile tab's titles and life facts, doing what the game's contracts,
//! titles and character information panels do.

use dereth_client_contract::UiRequest;

use super::Windows;
use crate::art::Family;
use crate::draw::Rect;
use crate::ui::game::GameState;
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

/// Seconds as the game counts an age: days, hours, minutes and seconds, the largest first and
/// the zero ones left out.
#[must_use]
pub fn duration_words(seconds: i64) -> String {
    let s = seconds.max(0);
    let parts = [
        (s / 86_400, "day"),
        (s / 3_600 % 24, "hour"),
        (s / 60 % 60, "minute"),
        (s % 60, "second"),
    ];
    let words: Vec<String> = parts
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, w)| format!("{n} {w}{}", if *n == 1 { "" } else { "s" }))
        .collect();
    if words.is_empty() {
        "0 seconds".to_owned()
    } else {
        words.join(", ")
    }
}

impl Windows {
    /// The contracts: the list on the left, the chosen one's notes, contact and places on the
    /// right, and Abandon.
    pub(super) fn journal(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        kit::tabs(
            p,
            ctx,
            Rect::new(body.x, body.y, body.w, 26.0 * k),
            &["Contracts", "Notebook"],
            &mut self.journal_tab,
        );
        let body = Rect::new(body.x, body.y + 34.0 * k, body.w, body.h - 34.0 * k);
        if self.journal_tab == 1 {
            self.notebook(p, ctx, state, body, out);
            return;
        }
        let name = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let list = Rect::new(body.x, body.y, body.w * 0.42, body.h - 36.0 * k);
        p.fill(list, 0x6010_0C08);
        if state.contracts.is_empty() {
            p.text_in(&dim, list, Align::Centre, "No contracts held.");
        }
        let mut y = list.y + 4.0 * k;
        for c in &state.contracts {
            let row = Rect::new(list.x, y, list.w, 40.0 * k);
            let on = self.contract_selected == Some(c.contract_id);
            if on || ctx.input.hover(&row) {
                crate::ui::pregame::list_highlight(p, row, on);
            }
            p.text(&name, row.x + 8.0 * k, y + 3.0 * k, &c.name);
            p.text(&dim, row.x + 8.0 * k, y + 21.0 * k, &c.status);
            if ctx.over(&row) && ctx.input.clicked(&row) {
                self.contract_selected = Some(c.contract_id);
            }
            y += 42.0 * k;
        }
        let pane = Rect::new(
            list.right() + 12.0 * k,
            body.y,
            body.right() - list.right() - 12.0 * k,
            list.h,
        );
        kit::panel(p, pane, 0.5);
        let Some(c) = self
            .contract_selected
            .and_then(|id| state.contracts.iter().find(|c| c.contract_id == id))
        else {
            p.text_in(&dim, pane, Align::Centre, "Choose a contract.");
            return;
        };
        let head = TextStyle::new(Family::Body, 15.0, 0xFFFF_FFFF).edge(ctx.colours.edge());
        let x = pane.x + 12.0 * k;
        let w = pane.w - 24.0 * k;
        let mut y = pane.y + 10.0 * k;
        for l in p.wrap(&head, &c.name, w) {
            p.text(&head, x, y, &l);
            y += 20.0 * k;
        }
        let mut facts = vec![c.status.clone()];
        if !c.timed.is_empty() {
            facts.push(c.timed.clone());
        }
        if !c.contact.is_empty() {
            facts.push(format!("Contact: {}", c.contact));
            facts.push(format!(
                "Contact's place: {}",
                c.contact_location
                    .clone()
                    .unwrap_or_else(|| "Indoors".to_owned())
            ));
        }
        if let Some(area) = &c.area_location {
            facts.push(format!("Where: {area}"));
        }
        for f in facts.iter().filter(|f| !f.is_empty()) {
            for l in p.wrap(&dim, f, w) {
                p.text(&dim, x, y, &l);
                y += 16.0 * k;
            }
        }
        y += 8.0 * k;
        let lh = p.line_height(&name);
        for l in p.wrap(&name, &c.description, w) {
            if y + lh > pane.bottom() - 6.0 * k {
                break;
            }
            p.text(&name, x, y, &l);
            y += lh;
        }
        if kit::button(
            p,
            ctx,
            Rect::new(
                body.right() - 120.0 * k,
                body.bottom() - 30.0 * k,
                120.0 * k,
                28.0 * k,
            ),
            "Abandon",
            true,
        ) {
            self.abandon_asked = Some((c.contract_id, c.name.clone()));
        }
        let _ = out;
    }

    /// The question before a contract is abandoned.
    pub(super) fn abandon_question(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        out: &mut Outcome,
    ) {
        let Some((id, name)) = self.abandon_asked.clone() else {
            return;
        };
        let text = format!("Abandon {name}?");
        if let Some(b) = crate::ui::pregame::dialog_box(
            p,
            ctx,
            "Abandon Contract",
            &text,
            &["Yes", "No"],
            true,
            0,
        ) {
            self.abandon_asked = None;
            if b == 0 {
                out.requests
                    .push(UiRequest::AbandonContract { contract_id: id });
                self.contract_selected = None;
            }
        }
    }

    /// The Profile tab's lower half: the titles to choose from, and the character's life.
    pub(super) fn profile_extra(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        area: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let section =
            TextStyle::new(Family::Heading, 18.4, ctx.colours.heading()).edge(ctx.colours.edge());
        let label = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let value = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let mut y = area.y;
        let row = |p: &mut Painter<'_>, y: f32, l: &str, v: &str| {
            super::stats::label_and_value(
                p,
                Rect::new(area.x, y - 2.0 * k, area.w, 18.0 * k),
                &label,
                area.x + 20.0 * k,
                l,
                &value,
                area.right() - 20.0 * k,
                v,
            );
        };
        if let Some(info) = &state.character_info {
            p.text(&section, area.x + 14.0 * k, y, "LIFE");
            y += 24.0 * k;
            if let Some(age) = info.age {
                row(p, y, "Age", &duration_words(i64::from(age)));
                y += 18.0 * k;
            }
            row(p, y, "Deaths", &info.num_deaths.to_string());
            y += 18.0 * k;
            row(
                p,
                y,
                "Burden",
                &format!("{} / {}", info.encumbrance, info.capacity),
            );
            y += 18.0 * k;
            row(p, y, "Augmentations", &info.augmentations.to_string());
            y += 18.0 * k;
            let innate = info
                .innate
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" / ");
            // The short label when the long one would run into the six numbers.
            let room = area.w - 40.0 * k - p.measure(&value, &innate) - 12.0 * k;
            let name = if p.measure(&label, "Innate attributes") <= room {
                "Innate attributes"
            } else {
                "Innate"
            };
            row(p, y, name, &innate);
            y += 26.0 * k;
        }
        // The titles: the one shown, and a list to choose another from.
        let titles = &state.titles;
        if titles.titles.is_empty() {
            return;
        }
        p.text(&section, area.x + 14.0 * k, y, "TITLES");
        // A little room between the heading and the first row, so a highlight on it does not crowd
        // the heading.
        y += 28.0 * k;
        let list = Rect::new(
            area.x + 10.0 * k,
            y,
            area.w - 20.0 * k,
            area.bottom() - y - 4.0 * k,
        );
        let row_h = 22.0 * k;
        #[allow(clippy::cast_precision_loss)]
        let content = row_h * titles.titles.len() as f32;
        let offset = kit::scroll(p, ctx, list, content, &mut self.titles_scroll);
        // For the pad the titles are the page's one list of rows.
        {
            let input = &mut *ctx.input;
            input.nav.list_of_rows(list, &input.occluders);
        }
        p.list.push_clip(list);
        for (i, (id, name)) in titles.titles.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                list.x,
                list.y + i as f32 * row_h - offset,
                list.w - 8.0 * k,
                row_h,
            );
            if r.bottom() < list.y || r.y > list.bottom() {
                continue;
            }
            let on = *id == titles.display;
            if on || ctx.input.hover(&r) {
                crate::ui::pregame::list_highlight(p, r, on);
            }
            p.text_in(&value, r.offset(8.0 * k, 0.0), Align::Left, name);
            // The title shown is a stop too, though choosing it again changes nothing.
            if ctx.over(&r) && !on && ctx.input.clicked(&r) {
                out.requests
                    .push(UiRequest::SetDisplayCharacterTitle { title_id: *id });
            }
        }
        p.list.pop_clip();
    }
}

#[cfg(test)]
mod tests {
    /// Behaviour: none (this client's own wording of an age)
    #[test]
    fn an_age_is_worded_largest_first_without_its_zero_parts() {
        assert_eq!(
            super::duration_words(90_061),
            "1 day, 1 hour, 1 minute, 1 second"
        );
        assert_eq!(super::duration_words(7_200), "2 hours");
        assert_eq!(super::duration_words(0), "0 seconds");
    }
}
