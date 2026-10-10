//! What gamepad mode draws: the pad's menu of every window, the ring round the focus, the
//! on-screen keyboard, and the line of what the pad's buttons do now.

use dereth_client_contract::UiRequest;
use dereth_primitives::ObjectId;

use crate::art::Family;
use crate::draw::{with_alpha, Rect};
use crate::pad::{Osk, OskKey, OSK_ROWS};
use crate::ui::game::{Blip, BlipKind, GameState};
use crate::ui::input::PadHints;
use crate::ui::input::PadWorld;
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::panels::{WindowId, Windows};
use crate::ui::Outcome;

use super::Hud;

/// The pad's menu, in order: each entry's name and the window it opens, `None` for the entries
/// that are not windows.
pub const MENU: [(&str, Option<WindowId>); 10] = [
    ("Character", Some(WindowId::Character)),
    ("Inventory", Some(WindowId::Inventory)),
    ("Spellbook", Some(WindowId::Actions)),
    ("Social", Some(WindowId::Social)),
    ("Allegiance", Some(WindowId::Allegiance)),
    ("Journal", Some(WindowId::Journal)),
    ("Map", Some(WindowId::Map)),
    ("Settings", Some(WindowId::Options)),
    ("HUD Layout", Some(WindowId::Layout)),
    ("Log Out", None),
];

/// The width and one entry's height of the pad's menu, in layout units.
const MENU_W: f32 = 260.0;
const ENTRY_H: f32 = 34.0;

impl Hud {
    /// The pad's menu, while it is open: a window of buttons, one for each of the game's windows,
    /// the chat line and logging out. An entry chosen closes the menu.
    pub fn pad_menu(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, windows: &mut Windows) {
        self.pad_menu_rect = None;
        windows.pad_menu = None;
        if !ctx.input.pad.menu {
            return;
        }
        let k = p.scale;
        let (sw, sh) = p.screen;
        #[allow(clippy::cast_precision_loss)]
        // Room under the last entry, clear of the frame's foot.
        let h = 84.0 + ENTRY_H * MENU.len() as f32;
        let mut state = kit::WindowState {
            open: true,
            opened_at: ctx.time - 1.0,
            ..kit::WindowState::default()
        };
        let win = kit::window(
            p,
            ctx,
            &mut state,
            "Menu",
            (MENU_W, h),
            (sw - (MENU_W + 40.0) * k, sh / 2.0 - h * k / 2.0),
        );
        self.pad_menu_rect = Some(win.rect);
        windows.pad_menu = Some(win.rect);
        let mut chosen = None;
        for (i, (name, _)) in MENU.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                win.body.x + 10.0 * k,
                win.body.y + 6.0 * k + i as f32 * ENTRY_H * k,
                win.body.w - 20.0 * k,
                (ENTRY_H - 4.0) * k,
            );
            if kit::button(p, ctx, r, name, true) {
                chosen = Some(i);
            }
        }
        if win.closed {
            ctx.input.pad.menu = false;
        }
        let Some(i) = chosen else {
            return;
        };
        ctx.input.pad.menu = false;
        match MENU[i] {
            (_, Some(id)) => {
                if !windows.is_open(id) {
                    windows.toggle(id, ctx.time);
                }
            }
            ("Log Out", None) => self.ask_log_out(ctx),
            _ => {}
        }
    }
}

/// The ring round the pad's focus, the on-screen keyboard and the line of what the pad's
/// buttons do, over everything else.
pub fn overlays(p: &mut Painter<'_>, ctx: &mut Ctx<'_>) {
    let Some(hints) = ctx.input.pad.mode else {
        return;
    };
    let k = p.scale;
    if let Some(r) = ctx.input.pad.focus.filter(|_| hints == PadHints::Cursor) {
        draw_focus_pointer(p, r, ctx.time);
    }
    if let Some(osk) = ctx.input.pad.osk.filter(|_| hints == PadHints::Keyboard) {
        keyboard(p, osk);
    }
    // The map's cursor: a cross where the right stick has it.
    if let Some((x, y)) = ctx.input.pad.map_cursor {
        let c = 0xFFF0_C860;
        let (l, w) = (12.0 * k, 2.0 * k);
        p.fill(Rect::new(x - l, y - w / 2.0, 2.0 * l, w), c);
        p.fill(Rect::new(x - w / 2.0, y - l, w, 2.0 * l), c);
    }
}

/// The pointing hand at the left edge of what the focus is on, its fingertip just touching it and
/// bobbing gently towards it: the focus's one mark.
pub fn draw_focus_pointer(p: &mut Painter<'_>, r: Rect, time: f64) {
    let k = p.scale;
    let (w, h) = (32.0 * k, 17.0 * k);
    let bob = 3.0 * k * crate::ui::wave(time, 5.0);
    // Kept on the screen where what has the focus starts at its left edge.
    let at = Rect::new(
        (r.x - w - 2.0 * k - bob).max(0.0),
        r.y + r.h / 2.0 - h / 2.0,
        w,
        h,
    );
    if let Some(hand) = p.piece("focus.pointer") {
        p.sprite(&hand, at, crate::draw::WHITE);
        return;
    }
    // Without the art, a gold arrow pointing right, in steps.
    let steps = 6;
    for i in 0..steps {
        #[allow(clippy::cast_precision_loss)]
        let (fi, n) = (i as f32, steps as f32);
        let half = at.h / 2.0 * (1.0 - fi / n);
        let x = at.x + at.w / 2.0 + fi * at.w / 2.0 / n;
        p.fill(
            Rect::new(x, at.y + at.h / 2.0 - half, at.w / 2.0 / n, 2.0 * half),
            0xFFF0_C860,
        );
    }
    p.fill(
        Rect::new(at.x, at.y + at.h * 0.35, at.w / 2.0, at.h * 0.3),
        0xFFF0_C860,
    );
}

/// The buttons `keys` names ("A", "LT/RT", "Up/Down", "RB+", ...) drawn as their marks from
/// `x`, centred on `cy`, `h` high: a glyph for a face button, the d-pad, a stick or the menu
/// buttons, a badge with its name for a shoulder button. The width drawn; 0 when the art has
/// no marks, and nothing is drawn.
pub(crate) fn button_marks(p: &mut Painter<'_>, keys: &str, x: f32, cy: f32, h: f32) -> f32 {
    marks(p, keys, x, cy, h, true)
}

/// [`button_marks`], drawing only when `draw`.
fn marks(p: &mut Painter<'_>, keys: &str, x: f32, cy: f32, h: f32, draw: bool) -> f32 {
    if p.piece("glyph.a").is_none() {
        return 0.0;
    }
    let k = p.scale;
    let gap = 2.0 * k;
    let (keys, plus) = keys
        .strip_suffix('+')
        .map_or((keys, false), |rest| (rest, true));
    let mut at = x;
    for key in keys.split('/') {
        let glyph = match key {
            "A" => Some("a"),
            "B" => Some("b"),
            "X" => Some("x"),
            "Y" => Some("y"),
            "Up" | "D-pad" => Some("up"),
            "Down" => Some("down"),
            "Left" => Some("left"),
            "Right" => Some("right"),
            "L3" => Some("lstick"),
            "R3" | "RS" => Some("rstick"),
            "Start" => Some("menu"),
            "Back" => Some("view"),
            _ => None,
        };
        if let Some(g) = glyph.and_then(|g| p.piece(&format!("glyph.{g}"))) {
            if draw {
                p.sprite(&g, Rect::new(at, cy - h / 2.0, h, h), crate::draw::WHITE);
            }
            at += h + gap;
        } else {
            // A shoulder button: its name on a badge.
            let style =
                TextStyle::new(Family::Heading, h * 0.62 / k, 0xFFF4_E6C4).edge(0xFF00_0000);
            let w = p.measure(&style, key) + h * 0.8;
            if draw {
                let badge = Rect::new(at, cy - h / 2.0, w, h);
                if !p.halves("badge.normal", badge, crate::draw::WHITE) {
                    p.fill(badge, 0xC020_2830);
                }
                p.text_in(&style, badge, Align::Centre, key);
            }
            at += w + gap;
        }
    }
    if plus {
        let style = TextStyle::new(Family::Body, h * 0.7 / k, 0xFFF0_C860).edge(0xFF00_0000);
        at += if draw {
            p.text_in(
                &style,
                Rect::new(at, cy - h / 2.0, 40.0 * k, h),
                Align::Left,
                "+",
            )
        } else {
            p.measure(&style, "+")
        } + gap;
    }
    at - x - gap
}

/// The on-screen keyboard: its rows of keys centred low on the screen, the picked one lit.
fn keyboard(p: &mut Painter<'_>, osk: Osk) {
    let k = p.scale;
    let (sw, sh) = p.screen;
    let key = 40.0 * k;
    let gap = 4.0 * k;
    let rows = OSK_ROWS.len() + 1;
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (
        10.0 * (key + gap) + 20.0 * k,
        rows as f32 * (key + gap) + 20.0 * k,
    );
    let pad = Rect::new(sw / 2.0 - w / 2.0, sh * 0.62 - h / 2.0, w, h);
    p.fill(pad, 0xE010_0C08);
    p.outline(pad, 1.0 * k, 0xFFC8_B27A);
    let style = TextStyle::new(Family::Body, 15.0, 0xFFEE_E1C5).edge(0xFF00_0000);
    for row in 0..rows {
        let n = Osk::row_len(row);
        #[allow(clippy::cast_precision_loss)]
        let kw = (10.0 * (key + gap) - gap * n as f32) / n as f32;
        for col in 0..n {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                pad.x + 10.0 * k + col as f32 * (kw + gap),
                pad.y + 10.0 * k + row as f32 * (key + gap),
                kw,
                key,
            );
            let picked = osk.row == row && osk.col == col;
            p.fill(r, if picked { 0xFF6A_5430 } else { 0xFF2A_241C });
            if picked {
                p.outline(r, 2.0 * k, 0xFFF0_C860);
            }
            let label = match Osk::key_at(row, col, osk.shift) {
                OskKey::Char(c) => c.to_string(),
                OskKey::Shift if osk.shift => "SHIFT".to_owned(),
                OskKey::Shift => "Shift".to_owned(),
                OskKey::Space => "Space".to_owned(),
                OskKey::Back => "Back".to_owned(),
                OskKey::Enter => "Enter".to_owned(),
                OskKey::Done => "Done".to_owned(),
            };
            p.text_in(&style, r, Align::Centre, &label);
        }
    }
}

fn distance(b: &Blip) -> f32 {
    (b.dx * b.dx + b.dy * b.dy).sqrt()
}

/// Whether a thing on the ground can be seen from where the camera is: the things the last
/// world draw could see (a click could pick them). People and creatures need not be seen to be
/// chosen; with no draw yet, everything counts as seen.
fn seen(state: &GameState, b: &Blip) -> bool {
    !matches!(b.kind, BlipKind::Item | BlipKind::Portal)
        || state.in_sight.as_ref().is_none_or(|s| s.contains(&b.id))
}

/// The bearing of a radar blip from the camera's way, in degrees clockwise, from -180 to 180.
fn bearing(state: &GameState, b: &Blip) -> f32 {
    // The radar has things against the player's facing: right and ahead of them.
    let off_facing = dereth_primitives::num::math::atan2f(b.dx, b.dy).to_degrees();
    (off_facing + state.heading - state.camera_heading + 540.0).rem_euclid(360.0) - 180.0
}

/// How far `b` is from where the player looks, in degrees, 0 straight ahead: the camera's way
/// with camera-based movement, the character's facing with character-based movement.
fn off_look(state: &GameState, b: &Blip) -> f32 {
    if state.look_by_camera {
        bearing(state, b).abs()
    } else {
        // The radar has things against the player's facing: right and ahead of them.
        dereth_primitives::num::math::atan2f(b.dx, b.dy)
            .to_degrees()
            .abs()
    }
}

/// How A with nothing selected chooses: whatever can be selected (a creature, a person, a
/// door, a corpse, a thing on the ground) best in line with where the player looks, a metre away
/// counting as much as two degrees off it. Only what is in front (within 90 degrees), within the
/// radar's reach, and, for a thing on the ground, seen.
#[must_use]
pub fn front_use(state: &GameState) -> Option<ObjectId> {
    state
        .targets
        .iter()
        .filter(|b| distance(b) <= state.radar_range && off_look(state, b) <= 90.0)
        .filter(|b| seen(state, b))
        .min_by(|a, b| use_score(state, a).total_cmp(&use_score(state, b)))
        .map(|b| b.id)
}

/// [`front_use`]'s score: lower is chosen.
fn use_score(state: &GameState, b: &Blip) -> f32 {
    off_look(state, b) + 2.0 * distance(b)
}

/// What the alternate selection cycles through, with where each stands across the screen:
/// everything that can be selected and is on screen, whatever its angle to the camera, in every
/// stance, ordered left to right by where it is drawn. A thing on the ground counts only where it
/// can be seen.
#[must_use]
pub fn alternates_across(state: &GameState) -> Vec<(ObjectId, f32)> {
    let mut v: Vec<(f32, ObjectId)> = state
        .targets
        .iter()
        .filter(|b| distance(b) <= state.radar_range && seen(state, b))
        .filter_map(|b| state.on_screen.get(&b.id).map(|(x, _)| (*x, b.id)))
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1 .0.cmp(&b.1 .0)));
    v.into_iter().map(|(x, id)| (id, x)).collect()
}

/// [`alternates_across`]'s ids alone.
#[must_use]
pub fn alternates(state: &GameState) -> Vec<ObjectId> {
    alternates_across(state)
        .into_iter()
        .map(|(id, _)| id)
        .collect()
}

impl Hud {
    /// What the pad asked of the world this frame: use, the fellowship's members, the alternate
    /// selection.
    pub fn pad_world(
        &mut self,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        windows: &mut Windows,
        out: &mut Outcome,
    ) {
        // The alternate selection lapses when what it rested on is gone.
        if let Some(alt) = self.alt {
            if !state.targets.iter().any(|b| b.id == alt) {
                self.alt = None;
            }
        }
        for ask in std::mem::take(&mut ctx.input.pad.world) {
            match ask {
                // An item armed to be used on something goes on the selection, or what is in
                // front with nothing selected.
                PadWorld::Use if state.targeting => {
                    if let Some(on) = state
                        .target
                        .as_ref()
                        .map(|t| t.id)
                        .or_else(|| front_use(state))
                    {
                        out.requests.push(UiRequest::ExecuteTargetItem(on));
                    }
                }
                PadWorld::Use => match (&state.target, front_use(state)) {
                    (Some(_), _) => ctx.input.actions.push(super::ACTION_USE),
                    (None, Some(id)) => out.requests.push(UiRequest::Select(id)),
                    (None, None) => {}
                },
                PadWorld::Fellow(by) => {
                    let members: Vec<ObjectId> = state
                        .fellowship_view
                        .as_ref()
                        .map(|f| f.members.iter().map(|m| m.id).collect())
                        .unwrap_or_default();
                    let pick = if members.is_empty() {
                        state.player_id
                    } else {
                        let n = i32::try_from(members.len()).unwrap_or(1);
                        let at = state
                            .target
                            .as_ref()
                            .and_then(|t| members.iter().position(|m| *m == t.id))
                            .and_then(|i| i32::try_from(i).ok());
                        let next = match at {
                            Some(i) => (i + by).rem_euclid(n),
                            None if by > 0 => 0,
                            None => n - 1,
                        };
                        usize::try_from(next)
                            .ok()
                            .and_then(|i| members.get(i).copied())
                    };
                    out.requests.extend(pick.map(UiRequest::Select));
                }
                PadWorld::Alternate(by) => {
                    let all = alternates_across(state);
                    if all.is_empty() {
                        continue;
                    }
                    let n = i32::try_from(all.len()).unwrap_or(1);
                    let from = self
                        .alt
                        .or(state.target.as_ref().map(|t| t.id))
                        .and_then(|id| all.iter().position(|(a, _)| *a == id))
                        .and_then(|i| i32::try_from(i).ok());
                    // Round at the ends; starting, the first that way from the middle of the
                    // screen.
                    let middle = self.screen_middle;
                    let next = match from {
                        Some(i) => (i + by).rem_euclid(n),
                        None if by > 0 => all
                            .iter()
                            .position(|(_, at)| *at >= middle)
                            .and_then(|i| i32::try_from(i).ok())
                            .unwrap_or(0),
                        None => all
                            .iter()
                            .rposition(|(_, at)| *at <= middle)
                            .and_then(|i| i32::try_from(i).ok())
                            .unwrap_or(n - 1),
                    };
                    self.alt = usize::try_from(next)
                        .ok()
                        .and_then(|i| all.get(i).map(|(id, _)| *id));
                }
                PadWorld::TakeAlternate => {
                    if let Some(id) = self.alt.take() {
                        out.requests.push(UiRequest::Select(id));
                    }
                }
                PadWorld::DropAlternate => self.alt = None,
                PadWorld::Options => {
                    if let Some(t) = &state.target {
                        windows.open_world_menu(t, state.on_screen.get(&t.id).copied());
                    }
                }
                // B with an item armed puts it down.
                PadWorld::Deselect if state.targeting => out.requests.push(
                    UiRequest::SetTargetMode(dereth_client_contract::view::TargetMode::None),
                ),
                PadWorld::Deselect => {
                    if state.target.is_some() {
                        out.requests.push(UiRequest::Select(ObjectId(0)));
                    }
                }
            }
        }
    }

    /// The alternate selection's ring: turning marks round where it stands on screen, and no
    /// name.
    pub fn alt_ring(&self, p: &mut Painter<'_>, ctx: &Ctx<'_>, state: &GameState) {
        let (Some(alt), Some((x, y))) = (self.alt, state.alt_at) else {
            return;
        };
        let from = p.list.mark();
        Self::ring_at(p, ctx, (x, y));
        // Drawn from where its object stood when the last frame was drawn: moved to where it
        // stands when this one is.
        p.list
            .anchor(alt, crate::draw::Anchor::Origin, from, (x, y), (x, y));
    }

    /// The turning marks of the alternate selection's ring round `(x, y)`.
    fn ring_at(p: &mut Painter<'_>, ctx: &Ctx<'_>, (x, y): (f32, f32)) {
        let k = p.scale;
        let radius = (34.0 + 4.0 * crate::ui::wave(ctx.time, 4.0)) * k;
        let turn = crate::ui::narrow(ctx.time * 2.0);
        // The ring: its outer layer turning one way (drawn twice, so its light stands out over
        // a bright ground), its inner the other, a little smaller.
        if let (Some(outer), Some(inner)) = (p.piece("ring.outer"), p.piece("ring.inner")) {
            let r = radius * 1.6;
            for _ in 0..2 {
                p.sprite_turned(
                    &outer,
                    Rect::new(x - r, y - r, 2.0 * r, 2.0 * r),
                    crate::draw::WHITE,
                    turn * 0.4,
                );
            }
            let ri = r * 0.82;
            p.sprite_turned(
                &inner,
                Rect::new(x - ri, y - ri, 2.0 * ri, 2.0 * ri),
                with_alpha(crate::draw::WHITE, 0.85),
                -turn * 0.25,
            );
            return;
        }
        let dot = 7.0 * k;
        for i in 0..12 {
            #[allow(clippy::cast_precision_loss)]
            let a = turn + i as f32 * std::f32::consts::TAU / 12.0;
            // Every third mark brighter: the ring visibly turns.
            let colour = if i % 3 == 0 { 0xFFF0_E080 } else { 0xC0E0_A030 };
            p.fill(
                Rect::new(
                    x + radius * dereth_primitives::num::math::cosf(a) - dot / 2.0,
                    y + radius * dereth_primitives::num::math::sinf(a) - dot / 2.0,
                    dot,
                    dot,
                ),
                colour,
            );
        }
    }
}
