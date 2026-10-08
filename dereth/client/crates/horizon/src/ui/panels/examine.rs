//! The examine window: the game's appraisal of an item, a creature or a player, laid out as item
//! details, in the words and order of the game's own examine panel.

use dereth_client_contract::UiRequest;

use super::{WindowId, Windows};
use crate::art::Family;
use crate::draw::Rect;
use crate::ui::game::{ExaminePane, GameState};
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

/// The inscription box's keyboard focus id.
const INSCRIPTION_FIELD: u32 = 70;

/// How tall the inscription band at the foot of the window is, in layout units: its label, five
/// lines of writing and the signature under them.
const INSCRIPTION_BAND: f32 = 128.0;

/// How far the inscription band reaches below the window's body, in layout units.
const INSCRIPTION_SINK: f32 = 12.0;

/// The room the band keeps over its box for the label, and under it for the signature.
const INSCRIPTION_LABEL: f32 = 18.0;
const INSCRIPTION_SIGNATURE: f32 = 18.0;

/// The most characters an inscription holds, as the game's inscription box takes them. Its
/// lines are not counted: Enter starts a new one while there is room for the character.
const INSCRIPTION_MAX: usize = 300;

/// The colours of the description's blocks: plain, raised by magic (green), lowered (red), and
/// unknown (yellow), as the game's own pane colours them.
fn run_colour(ctx: &Ctx<'_>, colour: u8) -> u32 {
    match colour {
        1 => 0xFF7C_E07C,
        2 => 0xFFF0_6A5A,
        3 => 0xFFF0_D060,
        _ => ctx.colours.text(),
    }
}

impl Windows {
    /// The window follows the game: it opens on an examine, and while it is open it examines
    /// each new selection and closes when nothing is selected, or when the examine key asks.
    pub(super) fn follow_examine(
        &mut self,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        if state.examine_opened {
            self.open(WindowId::Examine, ctx.time);
            self.examine_scroll = 0.0;
        }
        if state.examine_closed {
            self.close(WindowId::Examine);
        }
        let target = state.target.as_ref().map(|t| t.id);
        if target != self.examine_last_target {
            self.examine_last_target = target;
            if self.is_open(WindowId::Examine) {
                match target {
                    Some(id) if state.examined.as_ref().is_none_or(|e| e.id != id) => {
                        out.requests.push(UiRequest::Examine(id));
                    }
                    Some(_) => {}
                    None => self.close(WindowId::Examine),
                }
            }
        }
    }

    pub(super) fn examine(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let Some(e) = &state.examined else {
            let dim =
                TextStyle::new(Family::Body, 13.0, ctx.colours.dim()).edge(ctx.colours.edge());
            p.text_in(&dim, body, Align::Centre, "Examining...");
            return;
        };
        // The heading: the icon and the name, as item details open.
        let head = Rect::new(
            body.x + 8.0 * k,
            body.y + 6.0 * k,
            body.w - 16.0 * k,
            52.0 * k,
        );
        // An item shows its picture; a creature or a person shows only its name.
        let item = matches!(e.pane, ExaminePane::Item);
        let icon_r = if item {
            Rect::new(head.x, head.y, 48.0 * k, 48.0 * k)
        } else {
            Rect::new(head.x - 10.0 * k, head.y, 0.0, 48.0 * k)
        };
        let icon = e
            .look
            .as_ref()
            .and_then(|i| p.art.ac_item(i))
            .or_else(|| e.icon.and_then(|d| p.art.ac_icon(d)))
            .filter(|_| item);
        if let Some(icon) = icon {
            p.sprite(&icon, icon_r, crate::draw::WHITE);
        }
        let title = TextStyle::new(Family::Body, 16.0, 0xFFFF_FFFF).edge(ctx.colours.edge());
        let sub = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let tx = icon_r.right() + 10.0 * k;
        let tw = head.right() - tx;
        let title_lines = p.wrap(&title, &e.title, tw);
        let mut ty = head.y + 2.0 * k;
        for l in title_lines.iter().take(2) {
            p.text(&title, tx, ty, l);
            ty += p.line_height(&title);
        }
        let kind = match e.pane {
            ExaminePane::Item => String::new(),
            ExaminePane::Creature | ExaminePane::Character => format!("Level {}", e.level),
        };
        if !kind.is_empty() {
            p.text(&sub, tx, ty, &kind);
        }
        let rule_y = head.bottom() + 6.0 * k;
        p.fill(
            Rect::new(body.x + 8.0 * k, rule_y, body.w - 16.0 * k, 1.0 * k),
            0x60C8_B27A,
        );
        // An item the player may write on keeps its inscription box at the foot of the window.
        // The band reaches down into the room the window keeps over its foot, between the
        // corner plates, so its signature sits close over the frame.
        let (band, sink) = if e.pane == ExaminePane::Item && e.inscription_editable {
            (INSCRIPTION_BAND * k, INSCRIPTION_SINK * k)
        } else {
            (0.0, 0.0)
        };
        if band > 0.0 {
            let r = Rect::new(
                body.x + 10.0 * k,
                body.bottom() + sink - band,
                body.w - 20.0 * k,
                band,
            );
            self.inscription_box(p, ctx, e, &state.name, r, out);
        } else {
            self.leave_inscription(out);
        }
        // The rest scrolls.
        let area = Rect::new(
            body.x + 10.0 * k,
            rule_y + 8.0 * k,
            body.w - 20.0 * k,
            body.bottom() - rule_y - 12.0 * k - (band - sink),
        );
        let text = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let lh = p.line_height(&text);
        let mut lines: Vec<(String, u32, Option<String>)> = Vec::new();
        match e.pane {
            ExaminePane::Item => {
                for (i, (run, same_line, colour)) in e.runs.iter().enumerate() {
                    if i > 0 && !same_line {
                        lines.push((String::new(), 0, None));
                    }
                    let c = run_colour(ctx, *colour);
                    for para in run.split('\n') {
                        // Wrapped clear of the scrollbar, which a long text brings.
                        let room = area.w - kit::scrollbar_width(p).map_or(12.0, |w| w + 6.0) * k;
                        for l in p.wrap(&text, para, room) {
                            lines.push((l, c, None));
                        }
                    }
                }
                // A written inscription is shown; the invitation to write one is the box's, and
                // only where the player may write.
                if let Some((inscription, signature)) = e
                    .inscription
                    .as_ref()
                    .filter(|(t, _)| !e.inscription_editable && !t.is_empty() && t != PLACEHOLDER)
                {
                    lines.push((String::new(), 0, None));
                    for l in p.wrap(&text, inscription, area.w - 12.0 * k) {
                        lines.push((l, 0xFFE8_D8B0, None));
                    }
                    if !signature.is_empty() {
                        lines.push((signature.clone(), ctx.colours.dim(), None));
                    }
                }
            }
            ExaminePane::Creature | ExaminePane::Character => {
                for (label, value) in [
                    ("Heritage", &e.heritage),
                    ("Profession", &e.profession),
                    ("Allegiance", &e.allegiance),
                    ("Status", &e.pk),
                ] {
                    if let Some(v) = value.as_ref().filter(|v| !v.is_empty()) {
                        lines.push((label.to_owned(), ctx.colours.dim(), Some(v.clone())));
                    }
                }
                if lines.iter().any(|l| l.2.is_some()) {
                    lines.push((String::new(), 0, None));
                }
                for (label, value, colour) in e.rows.iter().chain(e.misc.iter()) {
                    lines.push((label.clone(), run_colour(ctx, *colour), Some(value.clone())));
                }
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let content = lh * lines.len() as f32;
        let offset = kit::scroll(p, ctx, area, content, &mut self.examine_scroll);
        // While the pane scrolls, its values stop short of the bar.
        let bar = if content > area.h {
            kit::scrollbar_width(p).map_or(12.0 * k, |w| (w + 6.0) * k)
        } else {
            12.0 * k
        };
        p.list.push_clip(area);
        let mut y = area.y - offset;
        for (left, colour, right) in &lines {
            if y + lh >= area.y && y <= area.bottom() {
                let s = text.colour(*colour);
                match right {
                    None => {
                        p.text(&s, area.x, y, left);
                    }
                    Some(v) => {
                        p.text(&text.colour(ctx.colours.dim()), area.x, y, left);
                        p.text_in(&s, Rect::new(area.x, y, area.w - bar, lh), Align::Right, v);
                    }
                }
            }
            y += lh;
        }
        p.list.pop_clip();
    }

    /// The inscription box, as the game's: several lines, Enter starting a new one and the lines
    /// kept as typed (a drawing made of letters stays one), committed with the game's inscription
    /// request when the box lets the keyboard go (Escape, or a press anywhere else). On taking the
    /// keyboard an unsigned item shows the player's signature under it, and a committed one keeps
    /// the words and the player's signature without waiting for the server. Nothing is sent when
    /// the text is unchanged, or when both it and the scribe are empty.
    #[allow(clippy::too_many_arguments)]
    fn inscription_box(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        e: &crate::ui::game::Examined,
        player: &str,
        r: Rect,
        out: &mut Outcome,
    ) {
        use crate::ui::edit;
        use crate::ui::input::vk;
        let k = p.scale;
        if self.inscription_for != Some(e.id) {
            self.leave_inscription(out);
            self.inscription_for = Some(e.id);
            self.inscription_edit.clone_from(&e.inscription_value);
            self.inscription_scroll = 0.0;
        }
        // What the item says as far as the box knows: the game's, or what was just committed.
        let (value, scribe) = match &self.inscribed {
            Some((id, value, scribe)) if *id == e.id => (value.clone(), scribe.clone()),
            _ => (e.inscription_value.clone(), e.scribe.clone()),
        };
        let label = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        p.text(&label, r.x, r.y, "Inscription");
        let field = Rect::new(
            r.x,
            r.y + INSCRIPTION_LABEL * k,
            r.w,
            r.h - (INSCRIPTION_LABEL + INSCRIPTION_SIGNATURE) * k,
        );
        let mut pressed_in = false;
        if ctx.input.pressed[0] {
            if ctx.input.hover(&field) {
                self.typing_field = INSCRIPTION_FIELD;
                pressed_in = true;
                ctx.input.pressed[0] = false;
                ctx.input.captured = true;
            } else if self.typing_field == INSCRIPTION_FIELD {
                self.typing_field = 0;
            }
        }
        ctx.over(&field);
        // A size smaller than body text, and room above the first line.
        let text = TextStyle::new(Family::Body, 11.5, ctx.colours.text()).edge(ctx.colours.edge());
        let inner = Rect::new(
            field.x + 8.0 * k,
            field.y + 7.0 * k,
            field.w - 16.0 * k,
            field.h - 12.0 * k,
        );
        let line_h = p.line_height(&text);
        let bar = kit::scrollbar_width(p).map_or(6.0, |w| w + 4.0) * k;
        let room = inner.w - bar;
        // The lines as typed, a line too long for the box broken at its spaces (one that fits is
        // kept as typed, its spaces and all), scrolled as the game's box scrolls: by its bar or
        // the wheel, and to the caret as the player types or moves it.
        let mut moved = false;
        if self.typing_field == INSCRIPTION_FIELD {
            ctx.input.text_focus = true;
            let key = edit::line_key(field, INSCRIPTION_FIELD);
            edit::begin(ctx.input, key, &self.inscription_edit);
            let before = (ctx.input.edit.caret, ctx.input.edit.anchor);
            let laid = edit::rows(p, &text, &self.inscription_edit, room);
            edit::lines_pointer(
                p,
                ctx.input,
                &text,
                &self.inscription_edit,
                &laid,
                (inner.x, inner.y - self.inscription_scroll),
                line_h,
                pressed_in,
            );
            let changed = edit::lines_keys(
                p,
                &text,
                ctx.input,
                &mut self.inscription_edit,
                INSCRIPTION_MAX,
                room,
            );
            moved = changed || before != (ctx.input.edit.caret, ctx.input.edit.anchor);
            if ctx.input.take_key(vk::ESCAPE) {
                self.typing_field = 0;
            }
        }
        let focused = self.typing_field == INSCRIPTION_FIELD;
        p.fill(field, if focused { 0xE020_1C14 } else { 0x9010_0C08 });
        p.outline(
            field,
            1.0 * k,
            if focused { 0xFFC8_B27A } else { 0x60C8_B27A },
        );
        if !focused && self.inscription_edit.is_empty() {
            p.text_in(
                &text.colour(ctx.colours.dim()),
                inner,
                Align::Left,
                PLACEHOLDER,
            );
        } else {
            let laid = edit::rows(p, &text, &self.inscription_edit, room);
            #[allow(clippy::cast_precision_loss)]
            let content = line_h * laid.len() as f32;
            if focused && moved {
                self.inscription_scroll = edit::follow_caret(
                    &laid,
                    ctx.input.edit.caret,
                    line_h,
                    inner.h,
                    self.inscription_scroll,
                );
            }
            let offset = kit::scroll(
                p,
                ctx,
                Rect::new(inner.x, inner.y, inner.w + 4.0 * k, inner.h),
                content,
                &mut self.inscription_scroll,
            );
            edit::draw_lines(
                p,
                focused.then_some(&ctx.input.edit),
                &text,
                &self.inscription_edit,
                &laid,
                (inner.x, inner.y - offset),
                line_h,
                inner,
                focused && (ctx.time * 2.0).fract() < 0.5,
            );
        }
        // The signature: the scribe's under what they wrote, or the player's while an unsigned
        // item is being written on.
        let signature = if !scribe.is_empty() && !value.is_empty() {
            Some(scribe.as_str())
        } else if focused && scribe.is_empty() {
            Some(player)
        } else {
            None
        };
        if let Some(name) = signature.filter(|n| !n.is_empty()) {
            p.text_in(
                &label,
                Rect::new(r.x, field.bottom() + 4.0 * k, r.w, 14.0 * k),
                Align::Right,
                &format!("--{name}"),
            );
        }
        if self.inscription_was_focused && !focused {
            let committed = crate::ui::game::Examined {
                inscription_value: value,
                scribe,
                ..e.clone()
            };
            if commit_inscription(&committed, &self.inscription_edit, out) {
                let signed = if self.inscription_edit.is_empty() {
                    String::new()
                } else {
                    player.to_owned()
                };
                self.inscribed = Some((e.id, self.inscription_edit.clone(), signed));
            }
        }
        self.inscription_was_focused = focused;
    }

    /// The box is gone (another object, or one the player may not write on): a write in progress
    /// is committed as the box losing the keyboard commits it.
    fn leave_inscription(&mut self, _out: &mut Outcome) {
        if self.inscription_was_focused && self.typing_field == INSCRIPTION_FIELD {
            self.typing_field = 0;
        }
        self.inscription_was_focused = false;
        self.inscription_for = None;
    }
}

/// The invitation shown in an empty box the player may write in.
const PLACEHOLDER: &str = dereth_presentation::appraisal::PLACEHOLDER;

/// The inscription request for `text` written on `e`, as the game's box sends it on losing the
/// keyboard: nothing when unchanged, or when both the text and the scribe are empty. True when it
/// was sent.
fn commit_inscription(e: &crate::ui::game::Examined, text: &str, out: &mut Outcome) -> bool {
    let worth_saying = !(text.is_empty() && e.scribe.is_empty());
    let send = worth_saying && text != e.inscription_value;
    if send {
        out.requests.push(UiRequest::SetInscription {
            object: e.id,
            text: text.to_owned(),
        });
    }
    send
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;
    use crate::ui::game::Examined;

    fn sword(scribe: &str, value: &str) -> Examined {
        Examined {
            id: dereth_primitives::ObjectId(0x8000_0001),
            inscription_editable: true,
            scribe: scribe.into(),
            inscription_value: value.into(),
            ..Examined::default()
        }
    }

    /// `c` typed in the inscription box's editor at the caret, Enter for a new line.
    fn write_inscription(text: &mut String, c: char) {
        use crate::ui::input::{vk, InputFrame};
        let art = crate::art::Art::empty();
        let mut list = crate::draw::DrawList::default();
        let p = Painter {
            list: &mut list,
            art: &art,
            scale: 1.0,
            screen: (800.0, 600.0),
            fade: 1.0,
        };
        let style = TextStyle::new(Family::Body, 11.5, 0xFFFF_FFFF);
        let mut input = InputFrame::default();
        crate::ui::edit::begin(&mut input, 1, text);
        if c == '\n' {
            input.keys.push(vk::ENTER);
        } else {
            input.chars.push(c);
        }
        crate::ui::edit::lines_keys(&p, &style, &mut input, text, INSCRIPTION_MAX, 200.0);
    }

    #[test]
    fn an_inscription_takes_three_hundred_characters_however_many_lines_they_make() {
        let mut text = String::new();
        for _ in 0..200 {
            write_inscription(&mut text, 'a');
            write_inscription(&mut text, '\n');
        }
        assert_eq!(
            text.chars().count(),
            300,
            "a new line counts as a character"
        );
        assert_eq!(text.lines().count(), 150, "and the lines are not limited");
        write_inscription(&mut text, 'b');
        assert!(!text.contains('b'), "nothing past the most it takes");
    }

    #[test]
    fn an_inscription_is_sent_only_when_it_changed_and_says_something() {
        let mut out = Outcome::default();
        commit_inscription(&sword("", ""), "", &mut out);
        assert!(
            out.requests.is_empty(),
            "nothing written on an unsigned item"
        );
        commit_inscription(&sword("Aurelia", "Mine"), "Mine", &mut out);
        assert!(out.requests.is_empty(), "unchanged");
        commit_inscription(&sword("", ""), "Mine", &mut out);
        commit_inscription(&sword("Aurelia", "Mine"), "", &mut out);
        assert_eq!(
            out.requests,
            [
                UiRequest::SetInscription {
                    object: dereth_primitives::ObjectId(0x8000_0001),
                    text: "Mine".into()
                },
                UiRequest::SetInscription {
                    object: dereth_primitives::ObjectId(0x8000_0001),
                    text: String::new()
                }
            ],
            "a new inscription, and the player's own one wiped"
        );
    }
}
