//! The party list down the left of the screen while the player is in a fellowship: a row for each
//! fellow, the player first and then the others in the fellowship's order, each with its place,
//! its name (the leader marked), its health (what other players took of it shown past what is
//! left) and its vigor. A row pressed selects its fellow; pressed again at once, a tell to it is
//! started, as the game's own fellows' rows do. Over a row, its level and its numbers.
//!
//! Beside it, the conflict lamp at the status row's left end: the kind of conflict between players
//! the place allows, and over it the player's kingdom. It shows the art's picture for the kind
//! ([`lamp_piece`]) where the art has one, and otherwise a plain lamp of the kind's color with its
//! letters.

use auberean_client_sdk::contract::view::{conflict, kingdom};
use auberean_client_sdk::Action;
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx};

use crate::draw::{Argb, Rect, WHITE};
use crate::social::Fellow;
use crate::state::{GameState, Vital};
use crate::ui::hud::{self, HEALTH, VIGOR};
use crate::ui::Tip;

/// How long a second press on the same row may come after the first to start a tell, in seconds.
pub const DOUBLE_PRESS: f64 = 0.5;

/// The color of what other players took of a fellow's health or vigor.
pub const TAKEN: Argb = 0xFFC0_3030;

/// A row's size and the step between rows, in layout units.
const ROW: (f32, f32) = (300.0, 44.0);
const ROW_STEP: f32 = 50.0;

/// Where the party list's `n`th row is on a screen drawn at `k` pixels a layout unit: down the
/// left, under the screen's top.
#[must_use]
pub fn row_rect(k: f32, n: usize) -> Rect {
    #[allow(clippy::cast_precision_loss)]
    let n = n as f32;
    Rect::new(22.0 * k, (96.0 + n * ROW_STEP) * k, ROW.0 * k, ROW.1 * k)
}

/// The rows the party list shows, in order: the player first (its own vitals, which come sooner
/// than the fellowship's), then the other fellows in the fellowship's order; none out of a
/// fellowship.
#[must_use]
pub fn rows(state: &GameState) -> Vec<Fellow> {
    let Some(party) = &state.social.party else {
        return Vec::new();
    };
    let me = state
        .player
        .and_then(|id| party.fellow(id))
        .map(|f| Fellow {
            health: state.health,
            vigor: state.vigor,
            ..f.clone()
        });
    me.into_iter()
        .chain(
            party
                .fellows
                .iter()
                .filter(|f| Some(f.id) != state.player)
                .cloned(),
        )
        .collect()
}

/// The party list.
#[derive(Debug, Default)]
pub struct PartyList {
    /// The last row pressed, and when.
    last_press: Option<(u64, f64)>,
}

/// A gauge with what other players took drawn past what is left.
fn gauge(p: &mut Painter<'_>, r: Rect, v: Vital, taken: u32, colour: Argb) {
    let past = Vital {
        current: v.current + u64::from(taken),
        max: v.max,
    };
    let mut layers = Vec::with_capacity(2);
    if taken > 0 {
        layers.push((past.ratio(), TAKEN));
    }
    layers.push((v.ratio(), colour));
    if !kit::own_gauge_layers(p, r, &layers) {
        p.fill(r, p.art.palette().gauge_track);
        for (fill, c) in layers {
            p.fill(Rect::new(r.x, r.y, r.w * fill, r.h), c);
        }
    }
}

impl PartyList {
    /// Draw it in a fellowship; where its rows are, and what was asked.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        tip: &mut Option<Tip>,
        actions: &mut Vec<Action>,
    ) -> Vec<Rect> {
        let Some(party) = &state.social.party else {
            self.last_press = None;
            return Vec::new();
        };
        let k = p.scale;
        let name_style = TextStyle::new(Family::Body, 14.0, WHITE).edge(0xFF00_0000);
        let number = TextStyle::new(Family::Numerals, 12.0, 0xFFEE_E1C5).edge(0xFF00_0000);
        let badge = TextStyle::new(Family::Numerals, 10.0, 0xFF20_1000);
        let selected = state.target.as_ref().map(|t| t.id);
        let mut drawn = Vec::new();
        for (i, f) in rows(state).iter().enumerate() {
            let row = row_rect(k, i);
            drawn.push(row);
            let over = ctx.over(&row);
            kit::list_highlight(p, row, over || selected == Some(f.id));
            p.text(
                &number,
                row.x + 2.0 * k,
                row.y + 4.0 * k,
                &(i + 1).to_string(),
            );
            let w = p.text(&name_style, row.x + 20.0 * k, row.y + 1.0 * k, &f.name);
            if f.id == party.leader {
                let b = Rect::new(row.x + 26.0 * k + w, row.y + 3.0 * k, 16.0 * k, 16.0 * k);
                p.fill(b, 0xFFE8_C060);
                p.outline(b, k, 0xFF40_2808);
                p.text_in(&badge, b, Align::Centre, "L");
            }
            let health = Rect::new(row.x + 20.0 * k, row.y + 24.0 * k, 160.0 * k, 10.0 * k);
            gauge(p, health, f.health, f.health_taken, HEALTH);
            let vigor = Rect::new(row.x + 188.0 * k, row.y + 24.0 * k, 104.0 * k, 10.0 * k);
            gauge(p, vigor, f.vigor, f.vigor_taken, VIGOR);
            if over {
                *tip = Some(Tip {
                    title: f.name.clone(),
                    lines: vec![
                        format!("Level {}", f.level),
                        format!("Health {} / {}", f.health.current, f.health.max),
                        format!("Vigor {} / {}", f.vigor.current, f.vigor.max),
                    ],
                });
            }
            if over && ctx.input.clicked(&row) {
                actions.push(Action::SetSelection { target: f.id });
                let again = self
                    .last_press
                    .is_some_and(|(id, at)| id == f.id && ctx.time - at <= DOUBLE_PRESS);
                if again {
                    actions.push(Action::AssistedTell {
                        name: f.name.clone(),
                    });
                    self.last_press = None;
                } else {
                    self.last_press = Some((f.id, ctx.time));
                }
            }
        }
        drawn
    }
}

/// The words for a kind of conflict between players a place allows, and its lamp's color.
#[must_use]
pub const fn conflict_words(kind: u32) -> Option<(&'static str, Argb)> {
    Some(match kind {
        conflict::PEACE => ("Peace Area", 0xFF4E_A85A),
        conflict::CONTROLLED => ("Controlled Conflict Area", 0xFFD0_A040),
        conflict::KINGDOM_AGAINST_KINGDOM => ("Kingdom vs. Kingdom Area", 0xFFD0_4A3A),
        conflict::FREE_FOR_ALL => ("Free-for-All Area", 0xFF9A_1E1E),
        conflict::KINGDOM_AGAINST_KINGDOM_ANYWHERE => {
            ("Kingdom vs. Kingdom Everywhere", 0xFFA0_3CB4)
        }
        _ => return None,
    })
}

/// The art's picture for a kind of conflict's lamp: Peace, Controlled, Kingdom against kingdom
/// (here or everywhere) and Free for all.
#[must_use]
pub const fn lamp_piece(kind: u32) -> Option<&'static str> {
    Some(match kind {
        conflict::PEACE => "lamp.peace",
        conflict::CONTROLLED => "lamp.controlled",
        conflict::KINGDOM_AGAINST_KINGDOM | conflict::KINGDOM_AGAINST_KINGDOM_ANYWHERE => {
            "lamp.kingdom"
        }
        conflict::FREE_FOR_ALL => "lamp.free-for-all",
        _ => return None,
    })
}

/// The words for a kingdom; for none, that the player is in none.
#[must_use]
pub const fn kingdom_words(k: Option<u32>) -> &'static str {
    match k {
        Some(kingdom::SHADOW) => "The Shadow",
        Some(kingdom::ORDER) => "The Order",
        Some(kingdom::DOMINION) => "The Dominion",
        Some(kingdom::NEUTRAL) => "Neutral",
        _ => "No Kingdom",
    }
}

/// Where the conflict lamp is on a screen drawn at `k` pixels a layout unit: at the status row's
/// left end, past its last effect.
#[must_use]
pub fn lamp_rect(screen: (f32, f32), k: f32) -> Rect {
    let last = hud::effect_rect(screen, k, hud::EFFECTS_PER_ROW - 1);
    Rect::new(last.x - 44.0 * k, last.y, 32.0 * k, 32.0 * k)
}

/// The conflict lamp, once the place's conflict is told; over it, the conflict's words and the
/// player's kingdom. Where it is.
pub fn conflict_lamp(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    state: &GameState,
    tip: &mut Option<Tip>,
) -> Option<Rect> {
    let kind = state.social.conflict?;
    let (words, colour) = conflict_words(kind)?;
    let k = p.scale;
    let r = lamp_rect(p.screen, k);
    match lamp_piece(kind).and_then(|n| p.piece(n)) {
        Some(picture) => p.sprite(&picture, r, WHITE),
        None => {
            p.fill(r, 0xC010_0C08);
            let inner = Rect::new(r.x + 4.0 * k, r.y + 4.0 * k, r.w - 8.0 * k, r.h - 8.0 * k);
            p.fill(inner, colour);
            p.outline(r, k, 0xFFA0_7C48);
            let label = TextStyle::new(Family::Numerals, 11.0, WHITE).edge(0xFF00_0000);
            let short = match kind {
                conflict::PEACE => "P",
                conflict::CONTROLLED => "C",
                conflict::FREE_FOR_ALL => "FFA",
                _ => "KvK",
            };
            p.text_in(&label, inner, Align::Centre, short);
        }
    }
    if ctx.over_quiet(&r) {
        *tip = Some(Tip {
            title: words.to_owned(),
            lines: vec![kingdom_words(state.social.kingdom).to_owned()],
        });
    }
    Some(r)
}
