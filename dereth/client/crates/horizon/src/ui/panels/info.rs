//! The information windows: a spell identified by a right-click (in the spellbook, on the spell
//! bar, or among the effects in force), and the vitae and burden windows their lamps open.
//!
//! A spell shows the details the spellbook shows, without its buttons; an effect also shows the
//! time it has left. The vitae window gives the penalty, what it does and the experience that
//! wins back the next point, and the burden window what is carried against what can be, and what
//! the excess costs, each in the game's own words under the figures.

use super::{WindowId, Windows};
use crate::art::Family;
use crate::draw::{Rect, WHITE};
use crate::ui::game::GameState;
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

/// The pictures the game's indicator strip shows: the vitae lamp lit, and the burden lamp at a
/// load of one to two times what can be carried and at two times or more.
pub const VITAE_ICON: u32 = 0x0600_74A0;
pub const BURDENED_ICON: u32 = 0x0600_74A3;
pub const OVERBURDENED_ICON: u32 = 0x0600_74A4;

/// The string table the vitae and burden words are read from.
pub const STRING_TABLE: u32 = 0x2300_0001;

/// A row of the string table with its named values filled in, or `None` when there is none.
pub type Fill<'a> = dyn FnMut(&str, &[(&str, &str)]) -> Option<String> + 'a;

/// What a click asked the information windows to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// A spell, identified.
    Spell(u32),
    /// A spell in force on the player, examined: its details and the time it has left.
    Effect(u32),
    Vitae,
    Burden,
}

/// The burden lamp's picture for a load, or none while the load is within what can be carried.
#[must_use]
pub fn burden_icon(load: Option<f32>) -> Option<u32> {
    use dereth_rules::burden::{load_band, LoadBand};
    match load.map(load_band)? {
        LoadBand::Normal => None,
        LoadBand::Encumbered => Some(BURDENED_ICON),
        LoadBand::OverBurdened => Some(OVERBURDENED_ICON),
    }
}

/// The vitae penalty in percent and the experience still needed to win back a point.
#[must_use]
pub fn vitae_figures(d: dereth_client_contract::VitaeDisplay) -> (i32, i64) {
    let c =
        dereth_presentation::stats::vitae_content(d, dereth_presentation::DisplayVariant::Modern);
    (c.penalty, c.experience)
}

/// The vitae window's words, as the game's vitae panel composes them: full strength, or the
/// loss, what it does, and the experience that wins back a point. `fill` reads a row of the
/// string table with its named values; `None` when the table has no such row.
pub fn vitae_words(d: dereth_client_contract::VitaeDisplay, fill: &mut Fill<'_>) -> Option<String> {
    let (pct, need) = vitae_figures(d);
    if pct < 1 {
        return fill("ID_Vitae_Text_Full", &[]);
    }
    let pct = dereth_presentation::character::num(pct);
    let need = dereth_presentation::character::num(need);
    let mut s = fill("ID_Vitae_Text_Vitae", &[("PERCENT", &pct)])?;
    s.push_str(&fill("ID_Vitae_Text_Skills", &[("PERCENT", &pct)])?);
    s.push_str(&fill("ID_Vitae_Text_Experience", &[("EXPERIENCE", &need)])?);
    Some(s)
}

/// The burden window's words, as the game's character sheet composes its load section: not
/// burdened, or by how much and what it costs; and the carrying augmentations, when there are
/// any.
pub fn burden_words(
    c: &dereth_client_contract::view::CharacterInfo,
    fill: &mut Fill<'_>,
) -> Option<String> {
    use dereth_presentation::character::{burden_penalty_percent, num, string, var};
    let mut s = if c.load < 1.0 {
        fill(string::LOAD_NONE, &[])?
    } else {
        let burden = num(c.encumbrance.saturating_sub(c.capacity));
        let penalty = num(burden_penalty_percent(c.load));
        fill(
            string::LOAD_BURDENED,
            &[(var::BURDEN, &burden), (var::PENALTY, &penalty)],
        )?
    };
    if c.augmentations > 0 {
        let n = num(c.augmentations);
        let extra = num(c.augmentations.saturating_mul(20));
        s.push_str(&fill(
            string::LOAD_AUGMENTATIONS,
            &[(var::NUM_AUGMENTATIONS, &n), (var::ADDITIONAL_LOAD, &extra)],
        )?);
    }
    Some(s)
}

/// Time left to the second under an hour, and to the minute above: `40s`, `12m 05s`, `1h 29m`.
#[must_use]
pub fn time_left_exact(seconds: f64) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: seconds left of an effect, small and not negative once clamped.
    let s = seconds.max(0.0).ceil() as u64;
    match s {
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m {:02}s", s / 60, s % 60),
        _ => format!("{}h {:02}m", s / 3600, s % 3600 / 60),
    }
}

impl Windows {
    /// Open the window a click asked for.
    pub fn ask(&mut self, ask: Ask, now: f64) {
        let id = match ask {
            Ask::Spell(spell) => {
                self.info_spell = Some((spell, false));
                WindowId::SpellInfo
            }
            Ask::Effect(spell) => {
                self.info_spell = Some((spell, true));
                WindowId::SpellInfo
            }
            Ask::Vitae => WindowId::Vitae,
            Ask::Burden => WindowId::Burden,
        };
        self.open(id, now);
    }

    /// The spell information window.
    pub(super) fn spell_info(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let Some((id, effect)) = self.info_spell else {
            self.close(WindowId::SpellInfo);
            return;
        };
        out.identify_spell = Some(id);
        let Some((_, d)) = state.spell_identify.as_ref().filter(|(sid, _)| *sid == id) else {
            let dim =
                TextStyle::new(Family::Body, 13.0, ctx.colours.dim()).edge(ctx.colours.edge());
            p.text_in(&dim, body, Align::Centre, "Identifying...");
            return;
        };
        // An effect also says how long it has left, or that it has run out.
        let extra: Vec<String> = if effect {
            vec![state.effects.iter().find(|e| e.spell == id).map_or_else(
                || "No longer in force".to_owned(),
                |e| {
                    e.remaining.map_or_else(
                        || "Lasting".to_owned(),
                        |left| format!("Time left: {}", time_left_exact(left)),
                    )
                },
            )]
        } else {
            Vec::new()
        };
        let pad = 10.0 * k;
        let r = Rect::new(
            body.x + pad,
            body.y + pad,
            body.w - 2.0 * pad,
            body.h - 2.0 * pad,
        );
        let d = d.clone();
        self.spell_details(p, ctx, state, (id, &d), r, &extra);
    }

    /// The vitae window: the penalty and its figures, then the game's words.
    pub(super) fn vitae(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
    ) {
        let k = p.scale;
        let d = state
            .vitae_display
            .unwrap_or(dereth_client_contract::VitaeDisplay {
                multiplier: state.vitae.unwrap_or(1.0),
                cp_pool: 0,
                threshold: 0,
            });
        let (pct, need) = vitae_figures(d);
        let mut y = self.info_head(
            p,
            ctx,
            body,
            VITAE_ICON,
            "Vitae",
            &if pct > 0 {
                format!("{pct}% vitae penalty")
            } else {
                "At full strength".to_owned()
            },
        );
        if pct > 0 {
            let rows = [
                ("Life force", format!("{}%", 100 - pct)),
                ("Vitals and skills", format!("-{pct}%")),
                (
                    "Experience to regain 1%",
                    dereth_presentation::character::num(need.max(0)),
                ),
            ];
            y = info_rows(p, ctx, body, y, &rows);
            // The experience earned towards the next point.
            if d.threshold > 0 {
                #[allow(clippy::cast_precision_loss)]
                let done = (d.cp_pool as f32 / d.threshold as f32).clamp(0.0, 1.0);
                let bar = Rect::new(body.x + 14.0 * k, y + 2.0 * k, body.w - 28.0 * k, 10.0 * k);
                kit::gauge(p, bar, done, 0xFFC8_A040);
                y += 22.0 * k;
            }
        }
        info_words(p, ctx, body, y, state.vitae_words.as_deref());
    }

    /// The burden window: what is carried against what can be, then the game's words.
    pub(super) fn burden(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
    ) {
        let k = p.scale;
        let load = state
            .character_info
            .as_ref()
            .map(|c| c.load)
            .or(state.burden)
            .unwrap_or(0.0);
        let icon = burden_icon(Some(load)).unwrap_or(BURDENED_ICON);
        let heading = format!("{} of what you can carry", super::burden_percent(load));
        let mut y = self.info_head(p, ctx, body, icon, "Burden", &heading);
        if let Some(c) = &state.character_info {
            use dereth_presentation::character::{burden_penalty_percent, num};
            let mut rows = vec![
                ("Carried", num(c.encumbrance)),
                ("Can carry", num(c.capacity)),
            ];
            if c.load >= 1.0 {
                rows.push(("Over by", num(c.encumbrance.saturating_sub(c.capacity))));
                rows.push((
                    "Run, Jump, Melee and Missile Defense",
                    format!("-{}%", burden_penalty_percent(c.load)),
                ));
            }
            y = info_rows(p, ctx, body, y, &rows);
            // The load against twice the capacity, the most that can be carried at all.
            let bar = Rect::new(body.x + 14.0 * k, y + 2.0 * k, body.w - 28.0 * k, 10.0 * k);
            let colour = if c.load >= 1.0 {
                0xFFE0_6040
            } else {
                0xFF60_A0E0
            };
            kit::gauge(p, bar, (c.load / 2.0).clamp(0.0, 1.0), colour);
            y += 22.0 * k;
        }
        info_words(p, ctx, body, y, state.burden_words.as_deref());
    }

    /// An information window's heading: the lamp's picture, the title and a line under it, over
    /// a rule. Where the rows begin.
    fn info_head(
        &self,
        p: &mut Painter<'_>,
        ctx: &Ctx<'_>,
        body: Rect,
        icon: u32,
        title: &str,
        line: &str,
    ) -> f32 {
        let k = p.scale;
        let head = Rect::new(
            body.x + 12.0 * k,
            body.y + 8.0 * k,
            body.w - 24.0 * k,
            44.0 * k,
        );
        let mut tx = head.x;
        // The interface's own icon for the lamp where it has one, square; else the game's.
        let own = match icon {
            VITAE_ICON => Some("status.vitae"),
            BURDENED_ICON => Some("status.burden"),
            OVERBURDENED_ICON => Some("status.overburden"),
            _ => None,
        };
        if let Some(s) = own.and_then(|n| p.piece(n)) {
            p.sprite(&s, Rect::new(head.x, head.y, head.h, head.h), WHITE);
            tx += head.h + 10.0 * k;
        } else if let Some(s) = p.art.ac_icon(icon) {
            // The lamp's own proportions, as tall as the heading.
            let w = if s.h > 0.0 {
                head.h * s.w / s.h
            } else {
                head.h
            };
            p.sprite(&s, Rect::new(head.x, head.y, w, head.h), WHITE);
            tx += w + 10.0 * k;
        }
        let big = TextStyle::new(Family::Body, 16.0, 0xFFFF_FFFF).edge(ctx.colours.edge());
        let sub = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        p.text(&big, tx, head.y + 2.0 * k, title);
        p.text(&sub, tx, head.y + 24.0 * k, line);
        let rule = head.bottom() + 8.0 * k;
        p.fill(
            Rect::new(body.x + 8.0 * k, rule, body.w - 16.0 * k, 1.0 * k),
            0x60C8_B27A,
        );
        rule + 10.0 * k
    }
}

/// Label and value rows from `y`; where the next thing begins.
fn info_rows(
    p: &mut Painter<'_>,
    ctx: &Ctx<'_>,
    body: Rect,
    y: f32,
    rows: &[(&str, String)],
) -> f32 {
    let k = p.scale;
    let label = TextStyle::new(Family::Body, 13.0, ctx.colours.dim()).edge(ctx.colours.edge());
    let value = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
    let mut y = y;
    for (l, v) in rows {
        let row = Rect::new(body.x + 14.0 * k, y, body.w - 28.0 * k, 20.0 * k);
        super::stats::label_and_value(p, row, &label, row.x, l, &value, row.right(), v);
        y += 20.0 * k;
    }
    y + 4.0 * k
}

/// The game's words, wrapped from `y` to the foot of the window.
fn info_words(p: &mut Painter<'_>, ctx: &Ctx<'_>, body: Rect, y: f32, words: Option<&str>) {
    let Some(words) = words else { return };
    let k = p.scale;
    let style = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
    let lh = p.line_height(&style);
    let x = body.x + 14.0 * k;
    let w = body.w - 28.0 * k;
    let mut y = y + 6.0 * k;
    // The game's words open with blank lines, which the figures above already stand in for.
    for l in p.wrap(&style, words.trim_start(), w) {
        if y + lh > body.bottom() - 6.0 * k {
            break;
        }
        p.text(&style, x, y, &l);
        y += lh;
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;
    use dereth_client_contract::VitaeDisplay;

    fn table(token: &str, values: &[(&str, &str)]) -> Option<String> {
        let vals: Vec<String> = values.iter().map(|(n, v)| format!("{n}={v}")).collect();
        Some(format!("[{token} {}]", vals.join(" ")))
    }

    #[test]
    fn an_effect_s_time_left_is_exact_to_the_second_under_an_hour() {
        assert_eq!(time_left_exact(40.2), "41s");
        assert_eq!(time_left_exact(725.0), "12m 05s");
        assert_eq!(time_left_exact(5_340.0), "1h 29m");
        assert_eq!(time_left_exact(-3.0), "0s");
    }

    #[test]
    fn the_burden_lamp_shows_only_over_what_can_be_carried_and_darkens_at_twice_it() {
        assert_eq!(burden_icon(None), None);
        assert_eq!(burden_icon(Some(0.99)), None);
        assert_eq!(burden_icon(Some(1.0)), Some(BURDENED_ICON));
        assert_eq!(burden_icon(Some(1.99)), Some(BURDENED_ICON));
        assert_eq!(burden_icon(Some(2.0)), Some(OVERBURDENED_ICON));
    }

    #[test]
    fn vitae_words_give_the_loss_twice_and_the_experience_to_the_next_point() {
        let d = VitaeDisplay {
            multiplier: 0.88,
            cp_pool: 1_000,
            threshold: 13_345,
        };
        let w = vitae_words(d, &mut table).expect("composed");
        assert_eq!(
            w,
            "[ID_Vitae_Text_Vitae PERCENT=12][ID_Vitae_Text_Skills PERCENT=12]\
             [ID_Vitae_Text_Experience EXPERIENCE=12,345]"
        );
        let full = VitaeDisplay {
            multiplier: 1.0,
            ..d
        };
        assert_eq!(
            vitae_words(full, &mut table).as_deref(),
            Some("[ID_Vitae_Text_Full ]")
        );
    }

    #[test]
    fn burden_words_say_the_excess_and_its_cost_and_add_the_augmentations() {
        let mut c = dereth_client_contract::view::CharacterInfo {
            load: 1.2,
            encumbrance: 3_600,
            capacity: 3_000,
            augmentations: 2,
            ..Default::default()
        };
        let w = burden_words(&c, &mut table).expect("composed");
        assert!(
            w.starts_with("[ID_CharacterInfo_Load_Burdened BURDEN=600 PENALTY=30]"),
            "{w}"
        );
        assert!(
            w.ends_with("NUM_AUGMENTATIONS=2 ADDITIONAL_LOAD=40]"),
            "{w}"
        );
        c.load = 0.5;
        c.augmentations = 0;
        assert_eq!(
            burden_words(&c, &mut table).as_deref(),
            Some("[ID_CharacterInfo_Load_None ]")
        );
    }

    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
    fn the_vitae_and_burden_words_come_whole_out_of_the_game_s_string_table() {
        let store = dereth_dat::testing::open_store().expect("the retail dats");
        let mut strings = crate::strings::Strings::default();
        let mut fill = |token: &str, values: &[(&str, &str)]| {
            strings.fill(&store, STRING_TABLE, token, values)
        };
        let d = VitaeDisplay {
            multiplier: 0.88,
            cp_pool: 1_000,
            threshold: 13_345,
        };
        let w = vitae_words(d, &mut fill).expect("the vitae rows");
        assert!(
            w.contains("you have temporarily lost 12% of your Vitae"),
            "{w}"
        );
        assert!(w.contains("reduced by 12%."), "{w}");
        assert!(w.contains("once you earn 12,345 more experience"), "{w}");
        let c = dereth_client_contract::view::CharacterInfo {
            load: 1.2,
            encumbrance: 3_600,
            capacity: 3_000,
            augmentations: 1,
            ..Default::default()
        };
        let w = burden_words(&c, &mut fill).expect("the load rows");
        assert!(w.contains("overburdened by 600 burden units"), "{w}");
        assert!(w.contains("skills by 30%"), "{w}");
        assert!(w.contains("Seventh Mule"), "{w}");
    }
}
