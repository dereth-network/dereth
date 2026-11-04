//! The map window: the world's picture, zoomed with the wheel or its buttons and panned by dragging.
//!
//! The picture draws three of its pixels to a landcell, 6123 to a side. It is drawn from the
//! landblocks of the data the client plays with ([`dereth_scene::world_map`]): the world is read
//! and the picture of the whole at a third of that size ([`OVERVIEW`]) drawn off the frame when
//! the map first opens, and tiles of it at its own size ([`TILE`]) are drawn as a view first needs
//! them, one a frame, and let go when no view has needed them for a while. Closing the map lets
//! the world and the tiles go and keeps the picture of the whole.
//!
//! The picture names nothing: the places are [`PLACES_FILE`]'s, drawn over it. Players see the
//! major towns, each a [`town_icon`] with its name, and the houses as the map is zoomed in far
//! enough to tell them apart.

use std::collections::HashMap;
use std::sync::Arc;

use dereth_client_contract::panels::map::{MapButton, MapClick};
use dereth_primitives::num::math::hypotf;
use dereth_scene::world_map::{Built, MapBuild, Pixels, WorldMap};
use dereth_ui_screens::mapradar::map::graphic;

use super::Windows;
use crate::art::{AcArt as _, Art, Family, Image, Sprite};
use crate::draw::{Rect, WHITE};
use crate::ui::game::GameState;
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};

/// The picture's side, in its own pixels.
#[allow(clippy::cast_precision_loss)] // LINT-OK: 6123.
pub const MAP_SIZE: f32 = dereth_scene::world_map::SIDE as f32;
/// The side of the picture of the whole.
#[allow(clippy::cast_precision_loss)] // LINT-OK: 2048.
const OVERVIEW: f32 = dereth_scene::world_map::OVERVIEW as f32;
/// What the picture of the whole is kept under.
pub const OVERVIEW_KEY: &str = "map.overview";
/// A tile's side, in the picture's own pixels.
#[allow(clippy::cast_precision_loss)] // LINT-OK: 1024.
const TILE: f32 = dereth_scene::world_map::TILE as f32;
/// How many tiles make a side.
#[allow(clippy::cast_possible_truncation)] // LINT-OK: 6.
const TILES: u32 = dereth_scene::world_map::TILES as u32;
/// How far past its own size the picture of the whole is drawn before the tiles take over.
const OVERVIEW_STRETCH: f32 = 1.3;
/// How long a tile no view needs is kept, in seconds.
const TILE_KEPT: f64 = 30.0;

/// The picture's pixels to a unit of the game's coordinates: ten landcells of three pixels.
const PER_UNIT: f64 = 30.0;
/// Where 0E and 0N fall on the picture: landcell vertex `v` is drawn as the three pixels from `3v`,
/// and stands at `v / 10 - 101.95` on either axis.
const ORIGIN: (f64, f64) = (3060.0, 3063.0);
/// The west and south edge of the landscape, then its east and north edge.
const WORLD: (f64, f64) = (-101.95, 102.05);

/// How much a notch of the wheel zooms.
const WHEEL_STEP: f32 = 1.25;
/// How much a press of a zoom button zooms.
const BUTTON_STEP: f32 = 1.5;
/// How many screen pixels a picture pixel is drawn at, at the most.
const MOST_MAGNIFIED: f32 = 3.0;
/// The places the map marks.
const PLACES_FILE: &str = "map-places.json";
/// A town's icon, on the screen at this many layout units a side.
const TOWN_UNITS: f32 = 16.0;
/// The town icon's picture's side, in its pixels.
const TOWN_SIDE: u32 = 64;
/// The zoom from which portals' names are written beside their marks.
const PORTAL_NAMES_FROM: f32 = 4.0;
/// How near the pointer, in layout units, a mark answers it.
const MARK_REACH: f32 = 6.0;
/// A mark's side, in layout units.
const MARK_SIDE: f32 = 5.0;

/// The pixel of the picture that `ns`, `ew` falls on.
#[must_use]
pub fn map_pixel(ns: f64, ew: f64) -> (f32, f32) {
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: within the picture's few thousand.
    (
        (ORIGIN.0 + PER_UNIT * ew) as f32,
        (ORIGIN.1 - PER_UNIT * ns) as f32,
    )
}

/// The place `(ns, ew)` at pixel `(x, y)` of the picture; `None` off the landscape.
#[must_use]
pub fn map_place(x: f32, y: f32) -> Option<(f64, f64)> {
    let ew = (f64::from(x) - ORIGIN.0) / PER_UNIT;
    let ns = (ORIGIN.1 - f64::from(y)) / PER_UNIT;
    let on = |v: f64| (WORLD.0..WORLD.1).contains(&v);
    (on(ns) && on(ew)).then_some((ns, ew))
}

/// What a place on the map is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaceKind {
    /// A major town: the retail map draws its town's icon.
    Town,
    /// A smaller place the retail map marks with a cross, or a region it names.
    Landmark,
    Portal,
    Lifestone,
    Cottage,
    Villa,
    Mansion,
}

impl PlaceKind {
    /// The zoom from which the map marks it.
    #[must_use]
    pub const fn marked_from(self) -> f32 {
        match self {
            Self::Town | Self::Landmark => 1.0,
            Self::Portal | Self::Lifestone => 2.0,
            Self::Mansion => 4.0,
            Self::Villa | Self::Cottage => 6.0,
        }
    }

    /// Whether a player sees it, or only an administrator.
    #[must_use]
    pub const fn players_see(self) -> bool {
        matches!(
            self,
            Self::Town | Self::Cottage | Self::Villa | Self::Mansion
        )
    }

    /// Its mark's colour.
    #[must_use]
    pub const fn colour(self) -> u32 {
        match self {
            Self::Town | Self::Landmark => 0xFFF0_C860,
            Self::Portal => 0xFFB0_70E8,
            Self::Lifestone => 0xFF80_E8FF,
            Self::Cottage => 0xFF40_B050,
            Self::Villa => 0xFF40_70E0,
            Self::Mansion => 0xFFE0_4040,
        }
    }
}

/// A place the map marks: what it is, its name and where it stands.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct Place {
    pub kind: PlaceKind,
    pub name: String,
    pub ns: f64,
    pub ew: f64,
}

/// The places file's places; none from a file that does not read.
#[must_use]
pub fn read_places(bytes: &[u8]) -> Vec<Place> {
    #[derive(serde::Deserialize)]
    struct File {
        places: Vec<Place>,
    }
    serde_json::from_slice::<File>(bytes)
        .map(|f| f.places)
        .unwrap_or_default()
}

/// `ns`, `ew` as the game writes a place: `42.2N 33.8E`.
fn coords_text(ns: f32, ew: f32) -> String {
    format!(
        "{:.1}{} {:.1}{}",
        ns.abs(),
        if ns >= 0.0 { "N" } else { "S" },
        ew.abs(),
        if ew >= 0.0 { "E" } else { "W" }
    )
}

/// Where the map's picture comes from.
#[derive(Debug, Default)]
enum Picture {
    /// Not asked for yet, or let go.
    #[default]
    None,
    /// Being built from the data the art holds.
    Building(MapBuild),
    /// Built, or handed over ([`MapView::show`]).
    Ready(Arc<WorldMap>),
    /// The data held no world to draw: the plain ground until other data comes.
    Failed,
}

/// Which part of the picture the map window shows.
#[derive(Debug)]
pub struct MapView {
    /// How many times the fit: 1 shows the whole picture.
    pub zoom: f32,
    /// The picture's pixel in the middle of the view.
    pub centre: (f32, f32),
    /// Where the out, in and fit buttons were drawn this frame.
    pub buttons: Option<[Rect; 3]>,
    /// Where the pointer last was in a pan under way.
    drag: Option<(f32, f32)>,
    /// The tiles drawn, and when a view last needed each.
    tiles: HashMap<(u32, u32), f64>,
    /// The places, once read.
    places: Option<Arc<Vec<Place>>>,
    /// The world the picture is drawn from.
    picture: Picture,
    /// The data it is drawn from, by its address; `None` for a picture handed over.
    data: Option<usize>,
    /// A picture of the whole handed over, kept at the next frame.
    given_overview: Option<Pixels>,
}

impl Default for MapView {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            centre: (MAP_SIZE / 2.0, MAP_SIZE / 2.0),
            buttons: None,
            drag: None,
            tiles: HashMap::new(),
            places: None,
            picture: Picture::None,
            data: None,
            given_overview: None,
        }
    }
}

impl MapView {
    /// The part of the picture shown, in its pixels.
    #[must_use]
    pub fn shown(&self) -> Rect {
        let span = MAP_SIZE / self.zoom;
        Rect::new(
            self.centre.0 - span / 2.0,
            self.centre.1 - span / 2.0,
            span,
            span,
        )
    }

    /// The screen point of picture pixel `at`, the view drawn over `area`.
    #[must_use]
    pub fn on_screen(&self, area: Rect, at: (f32, f32)) -> (f32, f32) {
        let shown = self.shown();
        let k = area.w / shown.w;
        (area.x + (at.0 - shown.x) * k, area.y + (at.1 - shown.y) * k)
    }

    /// The picture pixel under screen point `at`, the view drawn over `area`.
    #[must_use]
    pub fn on_picture(&self, area: Rect, at: (f32, f32)) -> (f32, f32) {
        let shown = self.shown();
        let k = shown.w / area.w;
        (shown.x + (at.0 - area.x) * k, shown.y + (at.1 - area.y) * k)
    }

    /// The view held on the picture, at a zoom `area` can show.
    fn settle(&mut self, area: Rect) {
        self.zoom = self.zoom.clamp(1.0, most_zoom(area));
        let half = MAP_SIZE / self.zoom / 2.0;
        self.centre.0 = self.centre.0.clamp(half, MAP_SIZE - half);
        self.centre.1 = self.centre.1.clamp(half, MAP_SIZE - half);
    }

    /// Zoom to `zoom`, picture pixel `anchor` staying where it is on the screen.
    fn zoom_about(&mut self, area: Rect, zoom: f32, anchor: (f32, f32)) {
        let was = self.on_screen(area, anchor);
        self.zoom = zoom;
        self.settle(area);
        let now = self.on_screen(area, anchor);
        let k = self.shown().w / area.w;
        self.centre.0 += (now.0 - was.0) * k;
        self.centre.1 += (now.1 - was.1) * k;
        self.settle(area);
    }

    /// The tiles under the view, each with the part of the picture it draws.
    fn tiles_shown(&self) -> Vec<((u32, u32), Rect)> {
        let shown = self.shown();
        let mut tiles = Vec::new();
        for row in 0..TILES {
            for col in 0..TILES {
                #[allow(clippy::cast_precision_loss)]
                let (x, y) = (col as f32 * TILE, row as f32 * TILE);
                let tile = Rect::new(x, y, TILE.min(MAP_SIZE - x), TILE.min(MAP_SIZE - y));
                if let Some(part) = tile.intersect(&shown) {
                    tiles.push(((col, row), part));
                }
            }
        }
        tiles
    }

    /// Let go of the tiles no view has needed since `before`.
    fn let_tiles_go(&mut self, p: &Painter<'_>, before: f64) {
        self.tiles.retain(|&(col, row), used| {
            let keep = *used >= before;
            if !keep {
                p.art.let_go(&tile_key(col, row));
            }
            keep
        });
    }

    /// Draw from `built` rather than the data the art holds: a world drawn elsewhere.
    pub fn show(&mut self, built: Built) {
        self.picture = Picture::Ready(built.map);
        self.data = None;
        self.given_overview = built.overview;
    }

    /// Whether the world the tiles are drawn from is here.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        matches!(self.picture, Picture::Ready(_))
    }

    /// The picture kept up with the data `art` holds: built when the picture of the whole is
    /// missing or `close` asks for tiles, taken once built, and built again for other data.
    fn keep_picture(&mut self, art: &Art, close: bool) {
        if let Some(given) = self.given_overview.take() {
            art.let_go(OVERVIEW_KEY);
            art.cached(OVERVIEW_KEY, || Some(image(given)));
        }
        let store = art.ac_data();
        let data = store.as_ref().map(|s| Arc::as_ptr(s) as usize);
        let given = self.data.is_none() && self.is_ready();
        if !given && data != self.data {
            self.picture = Picture::None;
            self.data = data;
        }
        if let Picture::Building(build) = &mut self.picture {
            match build.poll() {
                Some(Ok(built)) => {
                    if let Some(overview) = built.overview {
                        art.let_go(OVERVIEW_KEY);
                        art.cached(OVERVIEW_KEY, || Some(image(overview)));
                    }
                    self.picture = Picture::Ready(built.map);
                }
                Some(Err(e)) => {
                    tracing::warn!("the world map was not drawn: {e}");
                    self.picture = Picture::Failed;
                }
                None => {}
            }
        }
        let whole = art.lookup(OVERVIEW_KEY).is_some();
        if given || (whole && !close) || !matches!(self.picture, Picture::None | Picture::Ready(_))
        {
            return;
        }
        if whole && self.is_ready() {
            return;
        }
        if let Some(store) = store {
            self.picture = Picture::Building(MapBuild::start(store, !whole));
        }
    }
}

/// `pixels` as the art keeps a picture.
fn image(pixels: Pixels) -> Image {
    Image {
        width: pixels.width,
        height: pixels.height,
        bgra: pixels.bgra,
    }
}

/// A town's icon: a castle on a gold disc in a dark rim, drawn at [`TOWN_SIDE`] a side.
#[must_use]
pub fn town_icon() -> Image {
    // Signed distance from `(x, y)` to a box, negative inside.
    let boxed = |x: f32, y: f32, (x0, y0, x1, y1): (f32, f32, f32, f32)| {
        let dx = (x0 - x).max(x - x1);
        let dy = (y0 - y).max(y - y1);
        hypotf(dx.max(0.0), dy.max(0.0)) + dx.max(dy).min(0.0)
    };
    let cover = |d: f32| (0.5 - d).clamp(0.0, 1.0);
    let mix = |a: [f32; 3], b: [f32; 3], t: f32| {
        [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
        ]
    };
    let (rim, gold, stone) = ([40.0, 28.0, 16.0], [236.0, 196.0, 96.0], [58.0, 40.0, 22.0]);
    // Two crenellated towers either side of a lower wall.
    let castle = [
        (16.0, 22.0, 26.0, 47.0),
        (38.0, 22.0, 48.0, 47.0),
        (26.0, 30.0, 38.0, 47.0),
        (16.0, 17.0, 19.5, 22.0),
        (22.5, 17.0, 26.0, 22.0),
        (38.0, 17.0, 41.5, 22.0),
        (44.5, 17.0, 48.0, 22.0),
        (29.0, 26.0, 35.0, 30.0),
    ];
    // The gate: an arch over a doorway.
    let gate = (29.0, 39.0, 35.0, 47.0);
    let arch = ((32.0, 39.0), 3.0);
    let side = TOWN_SIDE as usize;
    let mut bgra = vec![0; side * side * 4];
    for (i, px) in bgra.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        #[allow(clippy::cast_precision_loss)] // LINT-OK: within the picture's few pixels.
        let (x, y) = ((i % side) as f32 + 0.5, (i / side) as f32 + 0.5);
        let centre = hypotf(x - 32.0, y - 32.0);
        let mut colour = mix(rim, gold, cover(centre - 26.5));
        let walls = castle
            .iter()
            .map(|&b| boxed(x, y, b))
            .fold(f32::MAX, f32::min);
        colour = mix(colour, stone, cover(walls));
        let opening = boxed(x, y, gate).min(hypotf(x - arch.0 .0, y - arch.0 .1) - arch.1);
        colour = mix(colour, gold, cover(opening));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: colour channels and a coverage, each clamped to its range.
        px.copy_from_slice(&[
            colour[2] as u8,
            colour[1] as u8,
            colour[0] as u8,
            (cover(centre - 30.0) * 255.0) as u8,
        ]);
    }
    Image {
        width: TOWN_SIDE,
        height: TOWN_SIDE,
        bgra,
    }
}

/// The most `area` zooms the picture.
fn most_zoom(area: Rect) -> f32 {
    (MOST_MAGNIFIED * MAP_SIZE / area.w.max(1.0)).max(1.0)
}

/// What tile `(col, row)` is kept under.
#[must_use]
pub fn tile_key(col: u32, row: u32) -> String {
    format!("map.tile-{col}-{row}")
}

impl Windows {
    /// Let the map's world and tiles go while its window is closed.
    pub(super) fn map_closed(&mut self, p: &Painter<'_>) {
        if !self.map_view.tiles.is_empty() {
            self.map_view.let_tiles_go(p, f64::INFINITY);
        }
        self.map_view.picture = Picture::None;
    }

    pub(super) fn map(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
    ) {
        let k = p.scale;
        p.fill(body, 0xFF2A_2418);
        // The picture, as large as the window holds it.
        let side = (body.w - 24.0 * k).min(body.h - 48.0 * k);
        let area = Rect::new(body.x + (body.w - side) / 2.0, body.y + 8.0 * k, side, side);
        self.map_view.settle(area);
        self.map_zoom_buttons(p, ctx, state, body, area);
        self.map_pointer(ctx, area);
        self.draw_map(p, ctx, area);
        // The pad's focus starts on the map itself, none of its places chosen.
        {
            let input = &mut *ctx.input;
            input
                .nav
                .note(area, crate::ui::nav::Kind::Plain, &input.occluders);
            input.nav.home(area);
        }
        self.map_picture = Some(area);
        self.tell_map_presses(ctx, area);
        self.map_places(p, ctx, area);
        // The plugins' points and lines on the map.
        if !self.plugins.frame.marks.map.is_empty() {
            let view = &self.map_view;
            let place = |ns: f64, ew: f64| {
                let (x, y) = view.on_screen(area, map_pixel(ns, ew));
                Some((
                    dereth_primitives::num::to_i32(x),
                    dereth_primitives::num::to_i32(y),
                ))
            };
            let glyphs = dereth_plugin_ui::marks::map_glyphs(&self.plugins.frame.marks.map, &place);
            p.list.push_clip(area);
            dereth_plugin_ui::layer::glyphs(p, &glyphs);
            p.list.pop_clip();
        }
        if let Some((n, e)) = state.coords {
            self.map_player(p, area, (n, e));
            let style =
                TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
            let label = coords_text(n, e);
            p.text_in(
                &style,
                Rect::new(body.x, area.bottom() + 6.0 * k, body.w, 24.0 * k),
                Align::Centre,
                &label,
            );
        }
    }

    /// The buttons under the picture: closer about the player while they are in view and otherwise
    /// about the view's middle, further out the same way, and back to the fit.
    fn map_zoom_buttons(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        area: Rect,
    ) {
        let k = p.scale;
        let view = &mut self.map_view;
        let shown = view.shown();
        let anchor = state
            .coords
            .map(|(n, e)| map_pixel(f64::from(n), f64::from(e)))
            .filter(|(x, y)| shown.contains(*x, *y))
            .unwrap_or(view.centre);
        let most = most_zoom(area);
        let y = area.bottom() + 4.0 * k;
        let mut x = body.x + 8.0 * k;
        let mut next = |w: f32| {
            let r = Rect::new(x, y, w * k, 26.0 * k);
            x += (w + 4.0) * k;
            r
        };
        let (out_at, in_at, fit_at) = (next(30.0), next(30.0), next(44.0));
        view.buttons = Some([out_at, in_at, fit_at]);
        if kit::button(p, ctx, out_at, "-", view.zoom > 1.0) {
            view.zoom_about(area, view.zoom / BUTTON_STEP, anchor);
        }
        if kit::button(p, ctx, in_at, "+", view.zoom < most) {
            view.zoom_about(area, view.zoom * BUTTON_STEP, anchor);
        }
        if kit::button(p, ctx, fit_at, "Fit", view.zoom > 1.0) {
            view.zoom_about(area, 1.0, anchor);
        }
    }

    /// The wheel over the picture zooms about the pointer, and a drag begun on it pans.
    fn map_pointer(&mut self, ctx: &mut Ctx<'_>, area: Rect) {
        let view = &mut self.map_view;
        let mouse = ctx.input.mouse;
        let over = ctx.input.hover(&area);
        if over && ctx.input.wheel != 0.0 {
            let zoom = view.zoom * dereth_primitives::num::math::powf(WHEEL_STEP, ctx.input.wheel);
            let anchor = view.on_picture(area, mouse);
            view.zoom_about(area, zoom, anchor);
            ctx.input.wheel = 0.0;
        }
        if let Some(last) = view.drag {
            if ctx.input.down[0] {
                let k = view.shown().w / area.w;
                view.centre.0 += (last.0 - mouse.0) * k;
                view.centre.1 += (last.1 - mouse.1) * k;
                view.settle(area);
                view.drag = Some(mouse);
            } else {
                view.drag = None;
            }
        } else if over && ctx.input.pressed[0] {
            view.drag = Some(mouse);
        }
    }

    /// The part of the picture the view shows, the sea's colour until the picture of the whole is
    /// drawn. Zoomed in past what the picture of the whole holds it is drawn from the tiles, one
    /// newly drawn a frame, the picture of the whole under any not drawn yet.
    fn draw_map(&mut self, p: &mut Painter<'_>, ctx: &Ctx<'_>, area: Rect) {
        let view = &mut self.map_view;
        let shown = view.shown();
        let close = area.w * view.zoom > OVERVIEW * OVERVIEW_STRETCH;
        view.keep_picture(p.art, close);
        let overview = p.art.lookup(OVERVIEW_KEY).flatten();
        let draw_overview = |p: &mut Painter<'_>| match &overview {
            Some(whole) => {
                let k = whole.src.w / MAP_SIZE;
                let src = Rect::new(shown.x * k, shown.y * k, shown.w * k, shown.h * k);
                p.sprite(&Sprite { src, ..*whole }, area, WHITE);
            }
            None => {
                let [r, g, b] = dereth_scene::world_map::sea_colour();
                p.fill(area, u32::from_be_bytes([0xFF, r, g, b]));
            }
        };
        if !close {
            draw_overview(p);
            view.let_tiles_go(p, ctx.time - TILE_KEPT);
            return;
        }
        let map = match &view.picture {
            Picture::Ready(map) => Some(Arc::clone(map)),
            _ => None,
        };
        let mut drawn = false;
        let mut parts = Vec::new();
        let mut waiting = false;
        for ((col, row), part) in view.tiles_shown() {
            let key = tile_key(col, row);
            let tile = p.art.lookup(&key).unwrap_or_else(|| {
                let map = map.as_ref().filter(|_| !drawn)?;
                drawn = true;
                p.art
                    .cached(&key, || Some(image(map.tile((col as usize, row as usize)))))
            });
            match tile {
                Some(tile) => {
                    view.tiles.insert((col, row), ctx.time);
                    parts.push((tile, (col, row), part));
                }
                None => waiting = true,
            }
        }
        if waiting {
            draw_overview(p);
        }
        let k = area.w / shown.w;
        for (tile, (col, row), part) in parts {
            #[allow(clippy::cast_precision_loss)]
            let src = Rect::new(
                part.x - col as f32 * TILE,
                part.y - row as f32 * TILE,
                part.w,
                part.h,
            );
            let dst = Rect::new(
                area.x + (part.x - shown.x) * k,
                area.y + (part.y - shown.y) * k,
                part.w * k,
                part.h * k,
            );
            p.sprite(&Sprite { src, ..tile }, dst, WHITE);
        }
        view.let_tiles_go(p, ctx.time - TILE_KEPT);
    }

    /// The places over the picture: the major towns' icons and names, then the houses' marks as
    /// the zoom shows them, with the landmarks' names and the portals' where they fit; and the name
    /// and place of the one under the pointer.
    fn map_places(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, area: Rect) {
        let places = Arc::clone(self.map_view.places.get_or_insert_with(|| {
            Arc::new(p.art.file(PLACES_FILE).map(read_places).unwrap_or_default())
        }));
        let view = &self.map_view;
        let k = p.scale;
        let town =
            TextStyle::new(Family::Heading, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let small = TextStyle::new(Family::Body, 11.0, ctx.colours.text()).edge(ctx.colours.edge());
        let mut taken: Vec<Rect> = Vec::new();
        let mut pointed_at: Option<&Place> = None;
        p.list.push_clip(area);
        // The towns first, so their names have the room.
        let icon = p.art.cached("map.town", || Some(town_icon()));
        let side = TOWN_UNITS * k;
        for place in places.iter().filter(|pl| pl.kind == PlaceKind::Town) {
            let at = view.on_screen(area, map_pixel(place.ns, place.ew));
            if !area.contains(at.0, at.1) {
                continue;
            }
            let r = Rect::new(at.0 - side / 2.0, at.1 - side / 2.0, side, side);
            match &icon {
                Some(icon) => p.sprite(icon, r, WHITE),
                None => p.fill(r, PlaceKind::Town.colour()),
            }
            if ctx.over(&r) {
                pointed_at = Some(place);
            }
            taken.push(r);
            let (w, h) = (p.measure(&town, &place.name), p.line_height(&town));
            let name = Rect::new(at.0 - w / 2.0, r.bottom(), w, h);
            if !taken.iter().any(|t| t.intersect(&name).is_some()) {
                p.text(&town, name.x, name.y, &place.name);
                taken.push(name);
            }
        }
        {
            let mouse = ctx.input.mouse;
            let pointed = pointed_at.is_none() && ctx.input.hover(&area);
            let mark_side = MARK_SIDE * k;
            let mut nearest: Option<(f32, &Place)> = None;
            let shown = |kind: PlaceKind| {
                kind != PlaceKind::Town && kind.players_see() && view.zoom >= kind.marked_from()
            };
            for place in places.iter().filter(|pl| shown(pl.kind)) {
                let at = view.on_screen(area, map_pixel(place.ns, place.ew));
                if !area.contains(at.0, at.1) {
                    continue;
                }
                let mark = Rect::new(
                    at.0 - mark_side / 2.0,
                    at.1 - mark_side / 2.0,
                    mark_side,
                    mark_side,
                );
                p.fill(mark, 0xFF10_1010);
                p.fill(
                    Rect::new(
                        mark.x + k,
                        mark.y + k,
                        mark_side - 2.0 * k,
                        mark_side - 2.0 * k,
                    ),
                    place.kind.colour(),
                );
                if pointed {
                    let d = hypotf(mouse.0 - at.0, mouse.1 - at.1);
                    if d <= MARK_REACH * k && nearest.is_none_or(|(best, _)| d < best) {
                        nearest = Some((d, place));
                    }
                }
                let named = place.kind == PlaceKind::Landmark
                    || (place.kind == PlaceKind::Portal && view.zoom >= PORTAL_NAMES_FROM);
                if named {
                    let (w, h) = (p.measure(&small, &place.name), p.line_height(&small));
                    let r = Rect::new(at.0 + mark_side, at.1 - h / 2.0, w, h);
                    if !taken.iter().any(|t| t.intersect(&r).is_some()) {
                        p.text(&small, r.x, r.y, &place.name);
                        taken.push(r);
                    }
                }
            }
            pointed_at = pointed_at.or(nearest.map(|(_, place)| place));
        }
        p.list.pop_clip();
        if let Some(place) = pointed_at {
            #[allow(clippy::cast_possible_truncation)] // LINT-OK: a coordinate within the world.
            let at = coords_text(place.ns as f32, place.ew as f32);
            self.tip = Some((place.name.clone(), vec![at]));
        }
    }

    /// Where the player is, when that is in view: the game's mark.
    fn map_player(&self, p: &mut Painter<'_>, area: Rect, at: (f32, f32)) {
        let (x, y) = map_pixel(f64::from(at.0), f64::from(at.1));
        if !self.map_view.shown().contains(x, y) {
            return;
        }
        let (px, py) = self.map_view.on_screen(area, (x, y));
        let s = 12.0 * p.scale;
        match p.art.ac_icon(graphic::PLAYER_ICON.0) {
            Some(icon) => p.sprite(&icon, Rect::new(px - s, py - s, 2.0 * s, 2.0 * s), WHITE),
            None => p.fill(Rect::new(px - s / 2.0, py - s / 2.0, s, s), 0xFF40_E040),
        }
    }

    /// A press on the map's picture `area` told to the plugins: the place under the pointer, the
    /// button and the modifiers held (the pad's A at the map's cursor is a left press there). The
    /// press is left to the map, which does nothing else with it but pan; one on the picture's
    /// margin, beyond the world, is told to no one.
    fn tell_map_presses(&mut self, ctx: &mut Ctx<'_>, area: Rect) {
        let input = &*ctx.input;
        if !input.hover(&area) {
            return;
        }
        let (x, y) = self.map_view.on_picture(area, input.mouse);
        let Some((ns, ew)) = map_place(x, y) else {
            return;
        };
        for (pressed, button) in
            input
                .pressed
                .iter()
                .zip([MapButton::Left, MapButton::Right, MapButton::Middle])
        {
            if *pressed {
                self.plugins
                    .push_input(dereth_plugin_ui::Input::Map(MapClick {
                        ns,
                        ew,
                        button,
                        ctrl: input.ctrl,
                        shift: input.shift,
                        alt: input.alt,
                    }));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;

    /// A place and its pixel are each other's, and Holtburg's is where the picture draws it.
    #[test]
    fn a_place_and_its_pixel_are_each_other_s_and_holtburg_is_where_the_picture_draws_it() {
        let (x, y) = map_pixel(42.2, 33.8);
        assert!((x - 4074.0).abs() < 0.01 && (y - 1797.0).abs() < 0.01);
        let (ns, ew) = map_place(x, y).expect("on the landscape");
        assert!((ns - 42.2).abs() < 1e-3 && (ew - 33.8).abs() < 1e-3);
        assert_eq!(map_place(5.0, 5.0).map(|_| ()), Some(()));
        assert_eq!(
            map_place(0.0, 3000.0),
            None,
            "the picture's edge, off the world"
        );
    }

    #[test]
    fn a_zoom_keeps_its_anchor_where_it_is_and_the_view_stays_on_the_picture() {
        let area = Rect::new(100.0, 50.0, 500.0, 500.0);
        let mut view = MapView::default();
        let anchor = (4074.0, 1797.0);
        let was = view.on_screen(area, anchor);
        view.zoom_about(area, 4.0, anchor);
        let now = view.on_screen(area, anchor);
        assert!(
            (was.0 - now.0).abs() < 0.1 && (was.1 - now.1).abs() < 0.1,
            "{was:?} {now:?}"
        );
        // At a corner the picture's edge holds the view rather than the anchor.
        view.zoom_about(area, 1.0, anchor);
        view.zoom_about(area, 4.0, (0.0, 0.0));
        let shown = view.shown();
        assert!(shown.x >= 0.0 && shown.y >= 0.0);
        assert!(shown.right() <= MAP_SIZE && shown.bottom() <= MAP_SIZE);
        view.zoom_about(area, 1e6, (0.0, 0.0));
        assert!(
            (view.zoom - most_zoom(area)).abs() < 1e-3,
            "held at the most"
        );
    }

    /// The tiles under the view cover it exactly, and a view of the whole is every tile.
    #[test]
    fn the_tiles_under_a_view_cover_it() {
        let mut view = MapView::default();
        assert_eq!(view.tiles_shown().len(), 36);
        view.zoom = 8.0;
        view.centre = (1024.0, 1024.0);
        let parts = view.tiles_shown();
        assert_eq!(parts.len(), 4, "the view stands on a corner of four tiles");
        let area: f32 = parts.iter().map(|(_, r)| r.w * r.h).sum();
        let shown = view.shown();
        assert!((area - shown.w * shown.h).abs() < 1.0);
    }

    /// The places built in read, each of every kind, and every one stands on the landscape.
    #[test]
    fn the_built_in_places_read_and_each_stands_on_the_landscape() {
        let pieces = crate::pieces::Pieces::built_in().expect("built in");
        let places = read_places(pieces.get(PLACES_FILE).expect(PLACES_FILE));
        for kind in [
            PlaceKind::Town,
            PlaceKind::Landmark,
            PlaceKind::Portal,
            PlaceKind::Lifestone,
            PlaceKind::Cottage,
            PlaceKind::Villa,
            PlaceKind::Mansion,
        ] {
            assert!(places.iter().any(|p| p.kind == kind), "{kind:?}");
        }
        for place in &places {
            let (x, y) = map_pixel(place.ns, place.ew);
            assert!(map_place(x, y).is_some(), "{place:?}");
        }
        assert!(read_places(b"<html>").is_empty());
    }

    /// Opened over the game's data, the map reads the world and draws the picture of the whole
    /// off the frame, then for a close view the world again without it, and lets the world go
    /// when it closes.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn the_map_draws_its_picture_from_the_data_off_the_frame() {
        use dereth_dat::testing::{classic_shortfall, open_classic_store_or_fail, open_store};
        let store = open_store()
            .or_else(|| {
                classic_shortfall()
                    .is_none()
                    .then(open_classic_store_or_fail)
            })
            .expect("the retail dats, or the older ones");
        let art = Art::empty();
        art.set_ac_store(Arc::new(store));
        let mut view = MapView::default();
        let wait = |view: &mut MapView, close: bool| {
            let start = web_time::Instant::now();
            let mut frames = 0;
            while !view.is_ready() {
                view.keep_picture(&art, close);
                frames += 1;
                assert!(start.elapsed().as_secs() < 30, "built in time");
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            (frames, start.elapsed().as_secs_f64() * 1000.0)
        };
        let (frames, ms) = wait(&mut view, false);
        assert!(art.lookup(OVERVIEW_KEY).is_some(), "the whole");
        assert!(frames > 1, "built off the frame");
        eprintln!("first picture of the whole: {ms:.0} ms over {frames} frames");
        view.picture = Picture::None;
        let (_, ms) = wait(&mut view, true);
        eprintln!("the world again, for tiles: {ms:.0} ms");
    }
}
