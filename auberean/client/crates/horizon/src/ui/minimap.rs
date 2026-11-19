//! The minimap at the top right: the objects around the player that the game puts on its radar,
//! in their own colors and shapes, north up and the player's arrow at the middle turned the way
//! it faces; the compass letters round its ring; and under it where the player stands (the map
//! coordinates outdoors, "Indoors" in a building or a dungeon) while the player chose to see them.
//!
//! It reaches as far as the game's radar does where the player stands (the shorter range inside),
//! and its plus and minus zoom in to a quarter of that and back out. The object under the pointer
//! is named, and a click on it selects it.
//!
//! **The rings.** Four dark rings stand on the disc at the game's own radar's places: 0.187,
//! 0.384, 0.581 and 0.770 of the way from the middle to the radar's reach, about a fifth of it
//! apart. The reach ends where the frame's inside edge begins, as the game's own radar's does
//! ([`RIM`]), so a blip at the very edge of the reach stands at the frame and no disc shows past
//! it. They stay where they are as the reach changes, so inside
//! (50 meters) they stand at 9.3, 19.2, 29.1 and 38.5 meters, and outdoors (75 meters) at 14.0,
//! 28.8, 43.6 and 57.7; zoomed in, they mark that much less. The reach is the shorter one wherever
//! the player stands in an interior cell, checked every frame as the place line's "Indoors" is.

use auberean_client_sdk::contract::view::radar_shape;
use auberean_client_sdk::Action;
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::Ctx;

use crate::draw::{Rect, WHITE};
use crate::state::{Blip, GameState};
use crate::ui::{Outcome, Tip};

/// The minimap's side, in layout units.
const SIDE: f32 = 196.0;

/// How far the radar's reach stands from the map's middle, as a share of half its side: where the
/// inside edge of the frame's picture begins (75.2 of its 88 texels).
pub const RIM: f32 = 75.2 / 88.0;

/// How far from a blip the pointer names it, in layout units.
const REACH: f32 = 8.0;

/// The zoom's steps: the radar's whole reach, a half, a quarter.
pub const ZOOMS: [f32; 3] = [1.0, 0.5, 0.25];

/// The selection's mark round its blip.
pub const SELECTED: u32 = 0xFFFF_E050;

/// Where the rings stand, each a share of the way from the middle to the radar's reach.
pub const RINGS: [f32; 4] = [0.187, 0.384, 0.581, 0.770];

/// A ring's dark line, and the faint light line just outside it.
pub const RING_DARK: u32 = 0x7000_0000;
pub const RING_LIGHT: u32 = 0x1CFF_FFFF;

/// How far from the middle each ring stands, in meters, on a radar reaching `range` meters.
#[must_use]
pub fn ring_meters(range: f32) -> [f32; 4] {
    RINGS.map(|r| r * range)
}

/// How far from the middle each ring stands, in pixels, on a map whose reach is `radius` pixels
/// from its middle.
#[must_use]
pub fn ring_radii(radius: f32) -> [f32; 4] {
    RINGS.map(|r| r * radius)
}

/// How far the radar's reach stands from the middle of `map`, in pixels: at the frame's inside
/// edge ([`RIM`]).
#[must_use]
pub fn reach(map: Rect) -> f32 {
    map.w / 2.0 * RIM
}

/// The minimap's zoom.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Minimap {
    /// Its step of [`ZOOMS`].
    step: usize,
}

/// Where the minimap's parts are on a screen drawn at `k` pixels a layout unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parts {
    /// The map.
    pub map: Rect,
    /// Its zoom buttons: in, out.
    pub zoom_in: Rect,
    pub zoom_out: Rect,
    /// The place line under it.
    pub place: Rect,
}

/// The minimap's parts on a screen of `screen` pixels, at `k` pixels a layout unit: below the
/// status row's effects and the link line.
#[must_use]
pub fn parts(screen: (f32, f32), k: f32) -> Parts {
    let d = SIDE * k;
    let x = screen.0 - d - 26.0 * k;
    let y = 200.0 * k;
    let button = |i: f32| {
        Rect::new(
            x + d - 30.0 * k,
            y + d - 58.0 * k + i * 26.0 * k,
            24.0 * k,
            24.0 * k,
        )
    };
    Parts {
        map: Rect::new(x, y, d, d),
        zoom_in: button(0.0),
        zoom_out: button(1.0),
        place: Rect::new(x, y + d + 6.0 * k, d, 16.0 * k),
    }
}

/// Where a blip `east` and `north` of the player is drawn on a map of `radius` pixels round
/// `centre` reaching `range` meters (north up); none past its rim.
#[must_use]
pub fn blip_at(
    centre: (f32, f32),
    radius: f32,
    range: f32,
    east: f32,
    north: f32,
) -> Option<(f32, f32)> {
    if range <= 0.0 {
        return None;
    }
    let (x, y) = (east / range * radius, -north / range * radius);
    (x * x + y * y <= radius * radius).then_some((centre.0 + x, centre.1 + y))
}

/// The place line: the map coordinates to a tenth, north or south then east or west; "Indoors"
/// inside; none where the player's place has no coordinates.
#[must_use]
pub fn place_line(state: &GameState) -> Option<String> {
    if state.radar.inside {
        return Some("Indoors".to_owned());
    }
    let (east, north) = state.radar.coordinates?;
    Some(format!(
        "{:.1}{} {:.1}{}",
        north.abs(),
        if north >= 0.0 { "N" } else { "S" },
        east.abs(),
        if east >= 0.0 { "E" } else { "W" }
    ))
}

impl Minimap {
    /// How much of the radar's reach it shows: 1 is all of it.
    #[must_use]
    pub fn zoom(&self) -> f32 {
        ZOOMS[self.step.min(ZOOMS.len() - 1)]
    }

    /// Draw it; where it answers the pointer.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        tip: &mut Option<Tip>,
        out: &mut Outcome,
    ) -> Rect {
        let k = p.scale;
        let at = parts(p.screen, k);
        let r = at.map;
        let centre = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let own = p.art.has_piece("minimap.ring");
        // The dark disc of the map, under the blips.
        match own.then(|| p.piece("minimap.mask")).flatten() {
            Some(mask) => p.sprite(&mask, r, 0x9020_2A34),
            None => p.fill(r.inset(20.0 * k), 0x7028_3848),
        }
        let radius = reach(r);
        // The rings, fixed on the disc whatever its reach.
        for ring in ring_radii(radius) {
            dereth_ui_kit::hud::ring(&mut *p, centre, ring + k, k, RING_LIGHT);
            dereth_ui_kit::hud::ring(&mut *p, centre, ring, (1.5 * k).max(1.0), RING_DARK);
        }
        let range = state.radar.range * self.zoom();
        let cell = 2.0 * k;
        let mut hovered: Option<(f32, &Blip)> = None;
        let (mx, my) = ctx.input.mouse;
        // The player's arrow at the middle, pointing the way the player faces, under the blips: what
        // stands at the player's feet (a creature fighting it) is drawn over it.
        match own.then(|| p.piece("minimap.arrow")).flatten() {
            Some(arrow) => {
                let h = 20.0 * k;
                let w = h * arrow.w / arrow.h.max(1.0);
                p.sprite_turned(
                    &arrow,
                    Rect::new(centre.0 - w / 2.0, centre.1 - h / 2.0, w, h),
                    WHITE,
                    state.radar.heading.to_radians(),
                );
            }
            None => p.fill(
                Rect::new(centre.0 - 2.0 * k, centre.1 - 2.0 * k, 4.0 * k, 4.0 * k),
                WHITE,
            ),
        }
        for b in &state.radar.blips {
            let Some((px, py)) = blip_at(centre, radius, range, b.east, b.north) else {
                continue;
            };
            for (dx, dy) in radar_shape::points(b.shape) {
                p.fill(
                    Rect::new(
                        px + f32::from(*dx) * cell - cell / 2.0,
                        py + f32::from(*dy) * cell - cell / 2.0,
                        cell,
                        cell,
                    ),
                    b.colour,
                );
            }
            if b.selected {
                let s = 11.0 * k;
                p.outline(Rect::new(px - s / 2.0, py - s / 2.0, s, s), k, SELECTED);
            }
            let d = (mx - px).powi(2) + (my - py).powi(2);
            if d <= (REACH * k).powi(2) && hovered.is_none_or(|(e, _)| d < e) {
                hovered = Some((d, b));
            }
        }
        // The ring, then the compass letters round it.
        if let Some(ring) = own.then(|| p.piece("minimap.ring")).flatten() {
            p.sprite_turned(&ring, r, WHITE, 45.0_f32.to_radians());
        }
        let style = TextStyle::new(Family::Heading, 18.0, 0xFFF5_EBD2).edge(0xFF14_120E);
        let rr = r.w / 2.0;
        for (letter, deg) in [("N", 0.0_f32), ("E", 90.0), ("S", 180.0), ("W", 270.0)] {
            let a = deg.to_radians();
            let (lx, ly) = (
                centre.0 + dereth_primitives::num::math::sinf(a) * rr,
                centre.1 - dereth_primitives::num::math::cosf(a) * rr,
            );
            p.text_in(
                &style,
                Rect::new(lx - 12.0 * k, ly - 12.0 * k, 24.0 * k, 24.0 * k),
                Align::Centre,
                letter,
            );
        }
        // Zoom in and back out.
        for (button, name, step) in [
            (at.zoom_in, "minimap.zoom-plus", 1_isize),
            (at.zoom_out, "minimap.zoom-minus", -1),
        ] {
            match own.then(|| p.piece(name)).flatten() {
                Some(s) => p.sprite(&s, button.inset(2.0 * k), WHITE),
                None => p.outline(button.inset(4.0 * k), k, 0xC0E8_C890),
            }
            if ctx.over(&button) && ctx.input.clicked(&button) {
                self.step = self.step.saturating_add_signed(step).min(ZOOMS.len() - 1);
            }
        }
        // Where the player stands, while the player chose to see it.
        if state.radar.show_coordinates {
            if let Some(line) = place_line(state) {
                let small = TextStyle::new(Family::Body, 12.0, WHITE).edge(0xFF00_0000);
                p.text_in(&small, at.place, Align::Centre, &line);
            }
        }
        // The object under the pointer named, and selected by a click.
        if ctx.over(&r) {
            if let Some((_, b)) = hovered {
                if !b.name.is_empty() {
                    *tip = Some(Tip {
                        title: b.name.clone(),
                        lines: Vec::new(),
                    });
                }
                if ctx.input.clicked(&r) {
                    out.acts.push(Action::SetSelection { target: b.id });
                }
            }
        }
        r
    }
}
