//! Spending experience and skill credits, in the character window: each attribute, vital and
//! trained skill raised by one or by ten for its experience, and each untrained skill trained for
//! its credits, as the game's attribute and skill panels do it.
//!
//! A raise waits for the game's answer before the same row can be raised again, as the game's
//! own panels hold their raise buttons down until the new value comes.

use dereth_client_contract::UiRequest;

use super::Windows;
use crate::art::Family;
use crate::draw::Rect;
use crate::ui::game::GameState;
use crate::ui::hud::grouped;
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Painter, TextStyle};
use crate::ui::Outcome;

/// How long a raise waits for the game's answer before its row may be raised again anyway.
const RAISE_WAIT: f64 = 3.0;

/// The colour of a value raised or lowered by magic.
fn value_colour(ctx: &Ctx<'_>, colour: u8) -> u32 {
    match colour {
        1 => 0xFF7C_E07C,
        2 => 0xFFF0_6A5A,
        _ => ctx.colours.text(),
    }
}

/// Where `style`'s capitals reach below the top of its line, in screen pixels: their top and the
/// baseline they stand on.
fn capitals(p: &Painter<'_>, style: &TextStyle) -> (f32, f32) {
    let lh = p.line_height(style);
    let Some(face) = p.face(style) else {
        return (lh * 0.2, lh * 0.75);
    };
    let font = &face.font;
    #[allow(clippy::cast_precision_loss)]
    let k = lh / font.line_height.max(1) as f32;
    match font.glyph('H') {
        Some(g) if g.h > 0 => (
            f32::from(g.y_offset) * k,
            (f32::from(g.y_offset) + f32::from(g.h)) * k,
        ),
        #[allow(clippy::cast_precision_loss)]
        _ => (lh * 0.2, font.ascent as f32 * k),
    }
}

/// The top to draw `style` at for its capitals to sit in the middle of `row`.
pub(super) fn centred_top(p: &Painter<'_>, style: &TextStyle, row: Rect) -> f32 {
    let (top, base) = capitals(p, style);
    (row.y + (row.h - top - base) / 2.0).round()
}

/// The top to draw `style` at for its baseline to fall on the baseline of `on` drawn from `top`,
/// so a label and its value in different sizes read as one line.
pub(super) fn beside_top(p: &Painter<'_>, on: &TextStyle, top: f32, style: &TextStyle) -> f32 {
    (top + capitals(p, on).1 - capitals(p, style).1).round()
}

/// A label at `x` and its value ending at `right`, in one row: the label's capitals centred in
/// `row`, the value on the label's baseline.
#[allow(clippy::too_many_arguments)]
pub(super) fn label_and_value(
    p: &mut Painter<'_>,
    row: Rect,
    label_style: &TextStyle,
    x: f32,
    label: &str,
    value_style: &TextStyle,
    right: f32,
    value: &str,
) {
    let top = centred_top(p, label_style, row);
    p.text(label_style, x, top, label);
    let w = p.measure(value_style, value);
    p.text(
        value_style,
        (right - w).round(),
        beside_top(p, label_style, top, value_style),
        value,
    );
}

/// How far a list inside a character panel keeps from the panel's sides: inside the frame's
/// bars, so its scrollbar stands within the frame.
/// Where a row of the attributes is a stop for the pad: from `left` to just short of its raise
/// buttons, which end at `right`.
fn row_stop(k: f32, left: f32, right: f32, y: f32) -> Rect {
    let buttons = 76.0 * k;
    Rect::new(
        left,
        y - 3.0 * k,
        (right - buttons - 4.0 * k - left).max(0.0),
        22.0 * k,
    )
}

fn list_margin(p: &Painter<'_>) -> f32 {
    10.0 * p.scale
}

/// The inside foot of the sheet's frame at the bottom of `area`.
fn frame_foot(p: &Painter<'_>, area: Rect) -> f32 {
    let k = p.scale;
    let (_, _, _, ib) = kit::panel_inset(p);
    area.bottom() - (ib - 8.0 * k).max(0.0)
}

/// The top of what the spending footer takes at the bottom of `area`: its rule, with the amounts
/// under it.
fn footer_top(p: &Painter<'_>, area: Rect) -> f32 {
    frame_foot(p, area) - 28.0 * p.scale
}

impl Windows {
    /// Whether the row keyed `key` is still waiting for the answer to a raise, given what it
    /// shows now.
    fn raising(&mut self, ctx: &Ctx<'_>, key: (bool, u32), shown: &str) -> bool {
        match &self.raise_wait {
            Some((k, was, at)) if *k == key => {
                if was != shown || ctx.time - at > RAISE_WAIT {
                    self.raise_wait = None;
                    false
                } else {
                    true
                }
            }
            _ => false,
        }
    }

    /// The "+1" and "+10" buttons at the end of a row: `Some(ten)` when one is pressed.
    #[allow(clippy::too_many_arguments)]
    fn raise_buttons(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        right: f32,
        y: f32,
        name: &str,
        cost: u32,
        cost_10: u32,
        have: u64,
        waiting: bool,
    ) -> Option<bool> {
        let k = p.scale;
        let b10 = Rect::new(right - 40.0 * k, y - 3.0 * k, 40.0 * k, 22.0 * k);
        let b1 = Rect::new(b10.x - 36.0 * k, b10.y, 32.0 * k, 22.0 * k);
        let can = |c: u32| !waiting && c > 0 && u64::from(c) <= have;
        let mut pressed = None;
        if kit::button(p, ctx, b1, "+1", can(cost)) {
            pressed = Some(false);
        }
        if kit::button(p, ctx, b10, "+10", can(cost_10)) {
            pressed = Some(true);
        }
        for (r, c, n) in [(b1, cost, 1), (b10, cost_10, 10)] {
            if ctx.input.hover(&r) && c > 0 {
                self.tip = Some((
                    format!("Raise {name} by {n}"),
                    vec![format!("{} experience", grouped(u64::from(c)))],
                ));
            }
        }
        pressed
    }

    /// The attributes and vitals, each with its raise buttons.
    pub(super) fn attributes_tab(
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
        let body = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let mut y = area.y;
        let value_right = area.right() - 100.0 * k;
        for (i, row) in state.stats.iter().enumerate() {
            if i == 0 || (row.vital && !state.stats[i - 1].vital) {
                if i > 0 {
                    y += 8.0 * k;
                }
                p.text(
                    &section,
                    area.x + 14.0 * k,
                    y,
                    if row.vital { "VITALS" } else { "ATTRIBUTES" },
                );
                y += 24.0 * k;
            }
            let style = body.colour(value_colour(ctx, row.colour));
            label_and_value(
                p,
                Rect::new(area.x, y - 3.0 * k, area.w, 22.0 * k),
                &label,
                area.x + 20.0 * k,
                &row.name,
                &style,
                value_right,
                &row.shown,
            );
            // The row itself is a stop for the pad, left of its buttons.
            ctx.stop(&row_stop(k, area.x + 14.0 * k, area.right() - 14.0 * k, y));
            let key = (row.vital, row.wire);
            let waiting = self.raising(ctx, key, &row.shown);
            if let Some(ten) = self.raise_buttons(
                p,
                ctx,
                area.right() - 14.0 * k,
                y,
                &row.name,
                row.cost,
                row.cost_10,
                state.unassigned_xp,
                waiting,
            ) {
                let xp = if ten { row.cost_10 } else { row.cost };
                out.requests.push(if row.vital {
                    UiRequest::TrainAttribute2nd {
                        vital: row.wire,
                        xp,
                    }
                } else {
                    UiRequest::TrainAttribute {
                        attribute: row.wire,
                        xp,
                    }
                });
                self.raise_wait = Some((key, row.shown.clone(), ctx.time));
            }
            y += 26.0 * k;
        }
        // For the pad the rows are one list, entered by stepping onto it.
        if y > area.y {
            let input = &mut *ctx.input;
            input.nav.list_of_rows(
                Rect::new(area.x, area.y, area.w, y - area.y),
                &input.occluders,
            );
        }
        self.spend_footer(p, ctx, state, area);
    }

    /// Every skill: specialised, trained and untrained. A trained skill is raised with
    /// experience; an untrained one is trained with credits.
    pub(super) fn skills_tab(
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
        let label = TextStyle::new(Family::Body, 12.0, ctx.colours.text()).edge(ctx.colours.edge());
        let body = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        // The list ends above the footer's rule, and nothing in it draws past that.
        let m = list_margin(p);
        let list = Rect::new(
            area.x + m,
            area.y,
            area.w - 2.0 * m,
            (footer_top(p, area) - 4.0 * k - area.y).max(0.0),
        );
        let row_h = 26.0 * k;
        let mut lines: Vec<(Option<&str>, Option<&crate::ui::game::Skill>)> = Vec::new();
        for (training, heading) in [(3, "SPECIALISED"), (2, "TRAINED"), (1, "UNTRAINED")] {
            let mut skills: Vec<&crate::ui::game::Skill> = state
                .skills
                .iter()
                .filter(|s| s.training == training)
                .collect();
            if skills.is_empty() {
                continue;
            }
            skills.sort_by(|a, b| a.name.cmp(&b.name));
            lines.push((Some(heading), None));
            lines.extend(skills.into_iter().map(|s| (None, Some(s))));
        }
        #[allow(clippy::cast_precision_loss)]
        let content = row_h * lines.len() as f32;
        // The bar stands a little in from the list's right edge.
        let scrolled = Rect::new(list.x, list.y, list.w - 2.0 * k, list.h);
        let offset = kit::scroll(p, ctx, scrolled, content, &mut self.skills_scroll);
        // For the pad the rows are one list, entered by stepping onto it.
        {
            let input = &mut *ctx.input;
            input.nav.list_of_rows(scrolled, &input.occluders);
        }
        // While the list scrolls, its bar stands at the right and the rows give way to it.
        let bar = if content > list.h {
            kit::scrollbar_width(p).unwrap_or(4.0) * k + 4.0 * k
        } else {
            0.0
        };
        let rows = Rect::new(list.x, list.y, list.w - bar, list.h);
        p.list.push_clip(list);
        let buttons_right = rows.right() - 6.0 * k;
        let value_right = buttons_right - 90.0 * k;
        for (i, (heading, skill)) in lines.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let y = list.y + i as f32 * row_h - offset + 4.0 * k;
            if y + row_h < list.y || y > list.bottom() {
                continue;
            }
            if let Some(h) = heading {
                p.text(&section, rows.x + 4.0 * k, y, h);
                continue;
            }
            let Some(s) = skill else { continue };
            if let Some(icon) = s.icon.and_then(|d| p.art.ac_skill(d)) {
                p.sprite(
                    &icon,
                    Rect::new(rows.x + 4.0 * k, y - 3.0 * k, 22.0 * k, 22.0 * k),
                    crate::draw::WHITE,
                );
            }
            let shown = s.value.to_string();
            label_and_value(
                p,
                Rect::new(rows.x, y - 3.0 * k, rows.w, 22.0 * k),
                &label,
                rows.x + 32.0 * k,
                &s.name,
                &body,
                value_right,
                &shown,
            );
            // The row itself is a stop for the pad, left of its buttons.
            let buttons = if s.training >= 2 { 76.0 } else { 80.0 };
            ctx.stop(&Rect::new(
                rows.x + 4.0 * k,
                y - 3.0 * k,
                (buttons_right - buttons * k - 4.0 * k - rows.x).max(0.0),
                22.0 * k,
            ));
            let key = (true, 0x1000 + s.id);
            let waiting = self.raising(ctx, key, &shown);
            if s.training >= 2 {
                if let Some(ten) = self.raise_buttons(
                    p,
                    ctx,
                    buttons_right,
                    y,
                    &s.name,
                    s.cost,
                    s.cost_10,
                    state.unassigned_xp,
                    waiting,
                ) {
                    out.requests.push(UiRequest::TrainSkill {
                        skill: s.id,
                        xp: if ten { s.cost_10 } else { s.cost },
                    });
                    self.raise_wait = Some((key, shown, ctx.time));
                }
            } else {
                // Training an untrained skill costs credits; the game asks before it spends
                // them.
                let b = Rect::new(buttons_right - 76.0 * k, y - 3.0 * k, 76.0 * k, 22.0 * k);
                let credits = i64::from(s.cost);
                let can = !waiting && s.cost > 0 && credits <= state.skill_credits;
                if kit::button(p, ctx, b, "Train", can) {
                    out.requests.push(UiRequest::TrainSkillAdvancementClass {
                        skill: s.id,
                        credits: s.cost,
                    });
                    self.raise_wait = Some((key, shown, ctx.time));
                }
                if ctx.input.hover(&b) && s.cost > 0 {
                    self.tip = Some((
                        format!("Train {}", s.name),
                        vec![format!(
                            "{} skill credit{}",
                            s.cost,
                            if s.cost == 1 { "" } else { "s" }
                        )],
                    ));
                }
            }
        }
        p.list.pop_clip();
        self.spend_footer(p, ctx, state, area);
    }

    /// What the player has to spend: unassigned experience and skill credits.
    fn spend_footer(&self, p: &mut Painter<'_>, ctx: &Ctx<'_>, state: &GameState, area: Rect) {
        let k = p.scale;
        let label = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let value = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        // The amounts in the middle of the room between the rule and the frame's foot.
        let rule = footer_top(p, area);
        let y = rule + (frame_foot(p, area) - rule - 20.0 * k) / 2.0;
        // Clear of the panel's corners.
        let (il, _, ir, _) = kit::panel_inset(p);
        let area = Rect::new(
            area.x + (il - 20.0 * k).max(0.0),
            area.y,
            area.w - (il - 20.0 * k).max(0.0) - (ir - 20.0 * k).max(0.0),
            area.h,
        );
        p.fill(
            Rect::new(area.x + 10.0 * k, rule, area.w - 20.0 * k, 1.0 * k),
            0x60C8_B27A,
        );
        // The experience's ten digits take the wider share; the credits are a few.
        let split = area.x + area.w * 0.62;
        let xp = grouped(state.unassigned_xp);
        // The short label when the long one would run into the amount.
        let room = split - area.x - 30.0 * k - p.measure(&value, &xp) - 8.0 * k;
        let name = if p.measure(&label, "Unassigned XP") <= room {
            "Unassigned XP"
        } else {
            "XP"
        };
        let row = Rect::new(area.x, y, area.w, 20.0 * k);
        label_and_value(
            p,
            row,
            &label,
            area.x + 20.0 * k,
            name,
            &value,
            split - 10.0 * k,
            &xp,
        );
        let x2 = split + 10.0 * k;
        label_and_value(
            p,
            row,
            &label,
            x2,
            "Skill Credits",
            &value,
            area.right() - 20.0 * k,
            &state.skill_credits.to_string(),
        );
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own character window layout)
    use super::*;
    use crate::art::Art;
    use crate::draw::DrawList;

    /// Run `f` with a painter over no art, and give back what it drew.
    fn drawn(f: impl FnOnce(&mut Painter<'_>)) -> DrawList {
        let art = Art::empty();
        let mut list = DrawList::default();
        let mut p = Painter {
            list: &mut list,
            art: &art,
            scale: 1.0,
            screen: (800.0, 600.0),
            fade: 1.0,
        };
        f(&mut p);
        list
    }

    #[test]
    fn a_label_and_its_larger_value_stand_on_one_baseline() {
        let label = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF);
        let value = TextStyle::new(Family::Body, 14.0, 0xFFFF_FFFF);
        drawn(|p| {
            let top = 40.0;
            let value_top = beside_top(p, &label, top, &value);
            assert!(value_top < top, "the larger value starts higher");
            let label_base = top + capitals(p, &label).1;
            let value_base = value_top + capitals(p, &value).1;
            assert!(
                (label_base - value_base).abs() <= 0.5,
                "{label_base} against {value_base}"
            );
        });
    }

    #[test]
    fn a_row_s_text_has_its_capitals_in_the_middle_of_the_row() {
        let style = TextStyle::new(Family::Body, 13.0, 0xFFFF_FFFF);
        drawn(|p| {
            let row = Rect::new(0.0, 100.0, 200.0, 22.0);
            let top = centred_top(p, &style, row);
            let (cap_top, base) = capitals(p, &style);
            let middle = top + (cap_top + base) / 2.0;
            assert!((middle - (row.y + row.h / 2.0)).abs() <= 0.5, "{middle}");
        });
    }

    #[test]
    fn the_figure_stands_on_an_opaque_grey_that_darkens_downward_inside_its_rect() {
        let r = Rect::new(20.0, 30.0, 180.0, 300.0);
        let list = drawn(|p| kit::figure_ground(p, r));
        assert!(!list.quads.is_empty());
        for q in &list.quads {
            assert!(
                q.dst.x >= r.x - 0.01
                    && q.dst.y >= r.y - 0.01
                    && q.dst.right() <= r.right() + 0.01
                    && q.dst.bottom() <= r.bottom() + 0.6,
                "{:?} outside {r:?}",
                q.dst
            );
        }
        // The ground's bands: full width, opaque so nothing behind the window shows through.
        let bands: Vec<_> = list
            .quads
            .iter()
            .filter(|q| q.tex.is_none() && (q.dst.w - r.w).abs() < 0.01 && q.dst.h > 1.0)
            .collect();
        assert!(bands.len() > 10);
        assert!(bands.iter().all(|q| q.colour >> 24 == 0xFF));
        let light = |c: u32| (c >> 16 & 0xFF) + (c >> 8 & 0xFF) + (c & 0xFF);
        let (first, last) = (bands[0], bands[bands.len() - 1]);
        assert!(first.dst.y < last.dst.y);
        assert!(light(first.colour) > light(last.colour) + 60);
    }
}
