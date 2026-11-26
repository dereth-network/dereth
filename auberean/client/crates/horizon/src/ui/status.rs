//! The player's standing on the HUD: the experience band along the top (the level, the
//! experience through it and the experience to spend), the link line over the minimap (the world,
//! the machine's clock, the round trip and a lamp for how lately the server was heard), and the
//! vitae lamp beside the conflict lamp. The band and the line are drawn as the first game's
//! Horizon draws them ([`dereth_ui_kit::hud`]).
//!
//! **The bandwidth meter** shows under the link line while the player turns it on in the
//! character options, as the game's own network indicator draws its graph: a column for each
//! heartbeat (the last 32), the bytes sent a second hanging from the top and the bytes received
//! standing from the bottom, each a step for every doubling from 8 bytes a second; the link's
//! tooltip then gives both rates.
//!
//! **The vitae lamp** shows below full vitae only, as the first game's Horizon shows it: the heart,
//! opaque, with the vitae under it; over it, the game's own words for the vitae. A click on it
//! examines the vitae in the examine window, as the game's own lamp does.

use auberean_client_sdk::contract::notice::examination::kind as examine;
use auberean_client_sdk::contract::view::Progress;
use dereth_ui_kit::art::Family;
use dereth_ui_kit::hud::{self as shared, grouped, Experience};
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::Ctx;

use crate::draw::{Argb, Rect, WHITE};
use crate::state::{GameState, METER_COLUMNS};
use crate::ui::hotbar::Examine;
use crate::ui::{minimap, party, Tip};

/// What the experience band calls the experience the player has to spend.
pub const AVAILABLE: &str = "Available XP";

/// The plain lamp's ground where the art has no heart.
pub const VITAE_GROUND: Argb = 0xFF60_1818;

/// The vitae lamp's art.
pub const VITAE_PIECE: &str = "status.vitae";

/// Where the vitae lamp is on a screen drawn at `k` pixels a layout unit: just left of the
/// conflict lamp.
#[must_use]
pub fn vitae_rect(screen: (f32, f32), k: f32) -> Rect {
    let conflict = party::lamp_rect(screen, k);
    Rect::new(conflict.x - 38.0 * k, conflict.y, 32.0 * k, 32.0 * k)
}

/// Where the experience band's gauge is: centered along the top, clear of the vitae lamp and the
/// status row at its right.
#[must_use]
pub fn experience_rect(screen: (f32, f32), k: f32) -> Rect {
    let clear = (screen.0 - vitae_rect(screen, k).x) / k + 10.0;
    shared::experience_band_rect(screen, k, clear)
}

/// What the experience band shows: the level, the way through it as the scripts last said, and
/// the experience to spend.
#[must_use]
pub fn experience(state: &GameState) -> Experience {
    let xp = state.experience;
    let known = xp.max > 0;
    Experience {
        level: state.progress.level,
        into: known.then_some(xp.current),
        span: known.then_some(xp.max),
        total: state.progress.total_experience.unwrap_or(0),
        to_spend: state.progress.available_experience.map(|n| (AVAILABLE, n)),
    }
}

/// The experience band, once the scripts said how far through its level the player is; over it,
/// the experience through the level, in all and to spend.
pub fn experience_band(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    state: &GameState,
    tip: &mut Option<Tip>,
) {
    let xp = state.experience;
    if xp.max == 0 {
        return;
    }
    let bar = experience_rect(p.screen, p.scale);
    if shared::experience_band(p, ctx, bar, &experience(state)) {
        let mut lines = vec![format!("{} / {}", grouped(xp.current), grouped(xp.max))];
        if let Some(t) = state.progress.total_experience {
            lines.push(format!("Total {}", grouped(t)));
        }
        if let Some(a) = state.progress.available_experience {
            lines.push(format!("{AVAILABLE} {}", grouped(a)));
        }
        *tip = Some(Tip {
            title: "Experience".to_owned(),
            lines,
        });
    }
}

/// What the link line shows.
#[must_use]
pub fn link(state: &GameState) -> shared::Link {
    shared::Link {
        world: state.link.world.clone(),
        local_time: state.link.clock_text(),
        round_trip_ms: state.link.round_trip_ms,
        packet_loss: state.link.loss,
        quiet: state.link.since_heard,
        lamp: state.in_world,
    }
}

/// Where the link line's right edge and its top are: over the minimap, its lamp at the
/// minimap's right edge.
#[must_use]
pub fn link_anchor(screen: (f32, f32), k: f32) -> (f32, f32) {
    let map = minimap::parts(screen, k).map;
    (map.right() + 15.0 * k, map.y - 36.0 * k)
}

/// The bandwidth meter's colors: its ground, the bytes sent, the bytes received.
pub const METER_GROUND: Argb = 0x9010_0C08;
pub const METER_SENT: Argb = 0xFF58_B4E8;
pub const METER_RECEIVED: Argb = 0xFF7C_D46A;

/// Where the bandwidth meter is: under the link line, its right edge at the lamp's, a column of
/// two units a heartbeat.
#[must_use]
pub fn meter_rect(screen: (f32, f32), k: f32) -> Rect {
    let (right, top) = link_anchor(screen, k);
    #[allow(clippy::cast_precision_loss)]
    let w = METER_COLUMNS as f32 * 2.0 * k;
    Rect::new(right - 15.0 * k - w, top + 21.0 * k, w, 12.0 * k)
}

/// How far a bar of the meter reaches for `rate` bytes a second, as a share of half the meter's
/// height: the game's own graph's scale, a step for each doubling from 8 bytes a second, full at
/// fifteen steps.
#[must_use]
pub fn meter_share(rate: f32) -> f32 {
    if rate <= 8.0 {
        return 0.0;
    }
    let steps = dereth_primitives::num::math::log2(f64::from(rate / 8.0));
    #[allow(clippy::cast_possible_truncation)] // a share, 0 to 1
    let share = (steps / 15.0).clamp(0.0, 1.0) as f32;
    share
}

/// The bandwidth meter while the player turned it on: a column for each heartbeat, the newest at
/// the right, the bytes sent hanging from the top and the bytes received standing from the bottom.
fn meter(p: &mut Painter<'_>, link: &crate::state::Link) {
    let k = p.scale;
    let r = meter_rect(p.screen, k);
    p.fill(r, METER_GROUND);
    let half = r.h / 2.0;
    let column = 2.0 * k;
    #[allow(clippy::cast_precision_loss)]
    let first = r.right() - link.history.len() as f32 * column;
    for (i, (received, sent)) in link.history.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let x = first + i as f32 * column;
        let down = meter_share(*sent) * half;
        let up = meter_share(*received) * half;
        if down > 0.0 {
            p.fill(Rect::new(x, r.y, column, down), METER_SENT);
        }
        if up > 0.0 {
            p.fill(Rect::new(x, r.bottom() - up, column, up), METER_RECEIVED);
        }
    }
}

/// The meter's lines in the link's tooltip: the bytes received and sent a second.
#[must_use]
pub fn meter_lines(link: &crate::state::Link) -> Vec<String> {
    match link.rates {
        Some((received, sent)) if link.meter => vec![
            format!("Received: {received:.0} bytes/s"),
            format!("Sent: {sent:.0} bytes/s"),
        ],
        _ => Vec::new(),
    }
}

/// The link line over the minimap and, while the player turned it on, the bandwidth meter under
/// it; over the line, the connection's measures (with the meter's, while it shows).
pub fn link_line(p: &mut Painter<'_>, ctx: &mut Ctx<'_>, state: &GameState, tip: &mut Option<Tip>) {
    let (right, top) = link_anchor(p.screen, p.scale);
    if state.link.meter && state.in_world {
        meter(p, &state.link);
    }
    if let Some(mut lines) = shared::link_line(p, &*ctx.input, right, top, &link(state)) {
        lines.extend(meter_lines(&state.link));
        *tip = Some(Tip {
            title: "Connection".to_owned(),
            lines,
        });
    }
}

/// The vitae's words, as the game's own lamp gives them.
#[must_use]
pub fn vitae_words(progress: &Progress) -> String {
    let vitae = progress.vitae_or_full();
    if vitae >= 100.0 {
        "You are currently at full vitae.".to_owned()
    } else {
        format!(
            "Vitae: {}% XP to recover next 1 point: {}.",
            dereth_primitives::num::to_i32(vitae),
            progress.vitae_experience.unwrap_or(0)
        )
    }
}

/// The vitae lamp, below full vitae only: the heart, opaque, with the vitae under it; over it,
/// the vitae's words. Where it is while shown, and the examination a click asks for.
pub fn vitae_lamp(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    state: &GameState,
    tip: &mut Option<Tip>,
) -> (Option<Rect>, Option<Examine>) {
    let vitae = state.progress.vitae_or_full();
    if vitae >= 100.0 {
        return (None, None);
    }
    let k = p.scale;
    let r = vitae_rect(p.screen, k);
    match p.piece(VITAE_PIECE) {
        Some(heart) => p.sprite(&heart, r, WHITE),
        None => p.fill(r, VITAE_GROUND),
    }
    let label = TextStyle::new(Family::Numerals, 12.0, WHITE).edge(0xFF00_0000);
    p.text_in(
        &label,
        Rect::new(r.x - 8.0 * k, r.bottom(), r.w + 16.0 * k, 16.0 * k),
        Align::Centre,
        &format!("{}%", dereth_primitives::num::to_i32(vitae)),
    );
    let mut asked = None;
    if ctx.over_quiet(&r) {
        *tip = Some(Tip {
            title: "Vitae".to_owned(),
            lines: vec![vitae_words(&state.progress)],
        });
        if ctx.input.clicked(&r) {
            asked = Some(Examine {
                kind: examine::VITAE,
                data_enum: 0,
                item: 0,
                data_id: 0,
            });
        }
    }
    (Some(r), asked)
}
