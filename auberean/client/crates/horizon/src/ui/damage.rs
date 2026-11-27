//! The damage text over a struck object: the words the scripts sent for a hit, in their color and
//! at their scale, standing centered over the box the object fills on the screen and rising three
//! of their own heights over their life as they fade out; over the selection's name and arrow when
//! the object is the selection. A text that would lie over one placed before it is moved off it
//! the shortest way it may go, as the game's own damage text is moved; one moved off the screen,
//! or over an object none of whose box is drawn on the screen, is not shown. Like the game's own,
//! a text stays on the screen across and out of the bands along the screen's top and foot that
//! the HUD's bars fill, standing at a band's edge where it would be in it ([`bands`]). A text goes
//! once its life is over or its object leaves the world.
//!
//! The texts are drawn with the rest of the interface, before the world moves on for the frame,
//! and moved once it has to where their objects are as the world is drawn
//! ([`crate::ui::marker::follow`]).

use auberean_client_sdk::contract::notice::HitText;
use auberean_ui_game::names::{self, Box2D};
use dereth_primitives::num::{to_i32, to_i32_f64};
use dereth_ui_kit::art::Family;
use dereth_ui_kit::draw::{Anchor, Spot};
use dereth_ui_kit::paint::{Painter, TextStyle};

use crate::draw::{Argb, Rect};
use crate::state::GameState;
use crate::ui::layout::{element, HudLayout};
use crate::ui::marker::{self, Where};

/// How large the text is written at a scale of 1, in points.
pub const POINTS: f32 = 16.0;

/// The HUD's bars the texts keep out of: the experience band and the target bar, the gauges and
/// the hotbar.
pub const BARS: [&str; 5] = [
    element::EXPERIENCE,
    element::TARGET,
    element::PARAMETER,
    element::FOCUS,
    element::HOTBAR,
];

/// How deep the bands along the top and foot of a screen `h` pixels tall that the texts keep out
/// of are, in pixels: down to the lowest foot of the [`BARS`] lying wholly in the screen's top
/// half, and up to the highest top of those wholly in its bottom half, each where the HUD's
/// layout showed it the last frame, as the texts are drawn under the bars, before them. A bar
/// counts only while it draws something: the target bar while there is a selection, the focus
/// gauge for a hero. One across the screen's middle keeps no band, so a bar moved there does not
/// fill the screen with one; one moved to the other edge keeps that edge's band.
#[must_use]
pub fn bands(layout: &HudLayout, state: &GameState, h: f32) -> (i32, i32) {
    let middle = h / 2.0;
    let (mut top, mut foot) = (0.0_f32, 0.0_f32);
    for name in BARS {
        if name == element::TARGET && state.target.is_none() {
            continue;
        }
        let Some(r) = layout.shown_last_frame(name) else {
            continue;
        };
        if r.bottom() <= middle {
            top = top.max(r.bottom());
        } else if r.y >= middle {
            foot = foot.max(h - r.y);
        }
    }
    (to_i32(top.ceil()), to_i32(foot.ceil()))
}

/// The edge round the text's letters.
const EDGE: Argb = 0xFF00_0000;

/// A damage text up: what it says, its color, when it began on the interface's clock, how long it
/// lasts and its scale, and the object it stands over.
#[derive(Clone, Debug, PartialEq)]
struct Up {
    object: u64,
    text: String,
    colour: Argb,
    start: f64,
    duration: f64,
    scale: f32,
}

impl Up {
    /// The text the scripts sent for a hit on `object`, begun at `now`. Its color is opaque: the
    /// alpha the scripts gave is not used, as the game's own damage text does not use it.
    fn new(object: u64, h: &HitText, now: f64) -> Self {
        let byte = |c: f32| u32::try_from(to_i32((c.clamp(0.0, 1.0) * 255.0).round())).unwrap_or(0);
        let [r, g, b, _] = h.color;
        Self {
            object,
            text: h.text.clone(),
            colour: 0xFF00_0000 | byte(r) << 16 | byte(g) << 8 | byte(b),
            start: now,
            duration: f64::from(h.seconds),
            scale: h.scale,
        }
    }
}

/// A damage text as the last frame drew it.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawn {
    pub object: u64,
    pub text: String,
    pub colour: Argb,
    /// Where it was written: its line's box, in pixels.
    pub rect: Rect,
    /// How much of it shows, 1 as it begins to 0 as its life ends.
    pub alpha: f32,
}

/// The damage texts up, oldest first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DamageTexts {
    up: Vec<Up>,
    drawn: Vec<Drawn>,
    /// The objects the last frame's texts over objects other than the selection stand on, a group
    /// of quads each from 1 on.
    anchored: Vec<u64>,
}

/// Where the texts over box `b` stand on, as they are moved after the world: the middle of its top.
#[must_use]
pub fn stands_on(b: Rect) -> Spot {
    Spot::at(b.x + b.w / 2.0, b.y)
}

impl DamageTexts {
    /// Follow one frame of `state` at `now` on the interface's clock: the texts over the objects
    /// that left the world go, and those the scripts sent begin.
    pub fn follow(&mut self, state: &GameState, now: f64) {
        let d = &state.damage;
        self.up.retain(|t| !d.gone.contains(&t.object));
        self.up
            .extend(d.texts.iter().map(|(o, h)| Up::new(*o, h, now)));
    }

    /// Whether any text is up.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.up.is_empty()
    }

    /// The objects the texts up stand over, oldest first (an object once for each of its texts).
    pub fn objects(&self) -> impl Iterator<Item = u64> + '_ {
        self.up.iter().map(|t| t.object)
    }

    /// The texts the last frame drew, in the order they were placed.
    #[must_use]
    pub fn drawn(&self) -> &[Drawn] {
        &self.drawn
    }

    /// The objects the last frame's texts over objects other than the selection stand on, the
    /// object of group 1 first ([`marker::follow`]).
    #[must_use]
    pub fn anchored(&self) -> &[u64] {
        &self.anchored
    }

    /// Draw the texts of `state` at `now`, oldest first: one whose life is over goes; the rest
    /// rise with their age over their object, keep out of the bands of the HUD's bars as `layout`
    /// shows them, are moved off the texts placed before them, and are written where they land on
    /// the screen, fading with their age.
    pub fn draw(&mut self, p: &mut Painter<'_>, state: &GameState, layout: &HudLayout, now: f64) {
        self.drawn.clear();
        self.anchored.clear();
        self.up
            .retain(|t| names::damage_age(t.start, t.duration, now) < 1.0);
        let (w, h) = p.screen;
        let screen = Box2D::sized(0, 0, to_i32(w), to_i32(h));
        let band = bands(layout, state, h);
        let selection = state.target.as_ref().map(|t| t.id);
        let mut placed: Vec<Box2D> = Vec::new();
        for t in &self.up {
            let age = names::damage_age(t.start, t.duration, now);
            let style = TextStyle::new(Family::Numerals, POINTS * t.scale, t.colour).edge(EDGE);
            let tw = to_i32(p.measure(&style, &t.text).ceil());
            let th = to_i32(p.line_height(&style).ceil());
            let selected = selection == Some(t.object);
            let (b, top) = if selected {
                let Some(Where::Box(b)) = state.selection_at else {
                    continue;
                };
                (b, marker::marks_top(p, state, b))
            } else {
                let Some(b) = state.damage.at.get(&t.object).copied() else {
                    continue;
                };
                (b, b.y)
            };
            let rise = names::damage_rise(names::DAMAGE_TRAVEL, th, age);
            let (middle, y) = names::keep_clear(
                to_i32((b.x + b.w / 2.0).round()),
                to_i32(top.round()),
                rise + th,
                (to_i32(w), to_i32(h)),
                band,
            );
            let x = to_i32_f64(f64::from(middle) - f64::from(tw) * 0.5);
            let Some(at) = names::place_damage(
                Box2D::sized(x, y, tw, th),
                &placed,
                &screen,
                names::PLACEMENT_ATTEMPTS,
            ) else {
                continue;
            };
            placed.push(at);
            #[allow(clippy::cast_precision_loss)] // pixels on the screen
            let r = Rect::new(at.x0 as f32, at.y0 as f32, tw as f32, th as f32);
            #[allow(clippy::cast_possible_truncation)] // a share of 1
            let alpha = (1.0 - age) as f32;
            let from = p.list.mark();
            let fade = p.fade;
            p.fade = fade * alpha;
            p.text(&style, r.x, r.y, &t.text);
            p.fade = fade;
            if selected {
                let middle = (b.x + b.w / 2.0, b.y + b.h / 2.0);
                p.list
                    .anchor(marker::GROUP, Anchor::Top, from, middle, middle);
            } else {
                let group = match self.anchored.iter().position(|o| *o == t.object) {
                    Some(i) => i + 1,
                    None => {
                        self.anchored.push(t.object);
                        self.anchored.len()
                    }
                };
                let s = stands_on(b);
                let id = dereth_primitives::ObjectId(u32::try_from(group).unwrap_or(u32::MAX));
                p.list.anchor(id, Anchor::Top, from, (s.x, s.y), (s.x, s.y));
            }
            self.drawn.push(Drawn {
                object: t.object,
                text: t.text.clone(),
                colour: t.colour,
                rect: r,
                alpha,
            });
        }
    }
}
