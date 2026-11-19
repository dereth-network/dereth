//! The map window: the world's map, zoomed with the wheel or its buttons and panned by dragging.
//!
//! - **The picture**: the world drawn as Dereth's Horizon draws its own ([`crate::world_map`]), a
//!   pixel to each ten-meter square of the land. The world is read and the picture of the whole
//!   shrunk ([`OVERVIEW_KEY`]) drawn off the frame when the map first opens; zoomed in past what
//!   that holds, tiles of the picture at its own size are drawn as a view first needs them, one a
//!   frame, the picture of the whole under any not drawn yet, and let go when no view has needed
//!   them for a while. Closing the map lets the world and the tiles go and keeps the picture of
//!   the whole. It opens on the land, framed by its own bounds with a margin about them; it
//!   zooms out no further.
//! - **The zoom**: the wheel zooms about the pointer, a drag on the picture pans; the buttons
//!   under it zoom in and out about the player while the player is in view (else about the
//!   view's middle), and back out to the land framed. Follow keeps the player in the middle of
//!   the view (as far as the land's edge allows) until a drag pans away. It is on as the map
//!   opens unless the player turned it off, which Horizon keeps in its own settings.
//! - **What it marks**: where the player stands and the way it faces (outdoors), while that is
//!   inside the picture; zoomed out, the continents' names alone, each on its land; zoomed in, the closer maps' labels, the markers the game put on the map, and the
//!   player's fellows (unless the player keeps them off the map). The one under the pointer is named with its map
//!   coordinates, and a click on a marker writes its words under the map.
//! - **Under the map**: where the player stands (its map coordinates, or that it is inside), then
//!   the game's time of day and, under it, its date.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_client_sdk::sim::WorldData;
use dereth_ui_kit::art::{Family, Image, Sprite};
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::atlas::{Atlas, Land};
use crate::draw::{Rect, WHITE};
use crate::state::GameState;
use crate::ui::Tip;
use crate::world_map::{Built, LandBounds, Landmass, MapBuild, MapLand, Pixels, WorldMap, TILE};

/// The window's body, in layout units: the picture's side and the room about it, as Dereth's
/// Horizon sizes its map ([`MapWindow::parts`]).
const BODY: (f32, f32) = (PICTURE + 16.0, PICTURE + UNDER + 8.0);

/// The picture's side in the window's own size, and the room under it for the buttons and the
/// lines, in layout units.
const PICTURE: f32 = 460.0;
const UNDER: f32 = 94.0;

/// How much a notch of the wheel zooms, and a press of a zoom button.
pub const WHEEL_STEP: f32 = 1.25;
pub const BUTTON_STEP: f32 = 1.5;

/// The room the view fully zoomed out leaves about the land, on each side, as a share of the
/// land's longer side.
pub const FRAME_MARGIN: f32 = 0.04;

/// How many screen pixels a land cell is drawn at, at the most.
const MOST_MAGNIFIED: f32 = 6.0;

/// Screen pixels to a land cell from which the closer maps' labels and the game's markers are
/// drawn; under it, the continents' names alone.
pub const MARKS_FROM: f32 = 0.9;

/// How far past its own size the picture of the whole is drawn before the tiles take over.
const OVERVIEW_STRETCH: f32 = 1.3;
/// How long a tile no view needs is kept, in seconds.
const TILE_KEPT: f64 = 30.0;
/// What the picture of the whole is kept under.
pub const OVERVIEW_KEY: &str = "map.overview";

/// The player's mark, a marker's and a label's, where they have no picture.
pub const PLAYER_MARK: u32 = 0xFF40_E040;
pub const MARKER: u32 = 0xFFE0_C040;
pub const LABEL: u32 = 0xFFD0_D0D0;

/// The continents' names' size, in layout units.
const CONTINENT_TEXT: f32 = 15.0;

/// How near the pointer, in layout units, a mark answers it; a mark's side.
const REACH: f32 = 8.0;
const MARK_SIDE: f32 = 12.0;

/// What tile `(col, row)` of the picture is kept under.
#[must_use]
pub fn tile_key(col: usize, row: usize) -> String {
    format!("map.tile-{col}-{row}")
}

/// The sea's color as the kit fills with it, where there is no picture yet.
#[must_use]
pub fn sea() -> u32 {
    let [r, g, b] = crate::world_map::sea_color();
    u32::from_be_bytes([0xFF, r, g, b])
}

/// Where the map's picture comes from.
#[derive(Debug, Default)]
enum Picture {
    /// Not asked for yet, or let go.
    #[default]
    None,
    /// Being built from the world's landblocks.
    Building(Box<MapBuild>),
    /// Built, or handed over ([`MapWindow::show`]).
    Ready(Arc<WorldMap>),
    /// The world held no land to draw: the plain sea until another world comes.
    Failed,
}

/// The part of the picture that shows `shown` of `land`, in the picture's pixels from its top
/// left: the picture is `size` pixels over the whole land, north at its top.
#[must_use]
pub fn on_picture(shown: Land, land: (i32, i32), size: (usize, usize)) -> Rect {
    #[allow(clippy::cast_precision_loss)]
    let (kx, ky) = (
        size.0 as f32 / land.0.max(1) as f32,
        size.1 as f32 / land.1.max(1) as f32,
    );
    #[allow(clippy::cast_precision_loss)]
    let top = land.1 as f32 - (shown.y + shown.h);
    Rect::new(shown.x * kx, top * ky, shown.w * kx, shown.h * ky)
}

/// The tiles of a picture `size` pixels under the part `part` of it, each with the part of the
/// picture it draws.
#[must_use]
pub fn tiles_under(part: Rect, size: (usize, usize)) -> Vec<((usize, usize), Rect)> {
    let (cols, rows) = (size.0.div_ceil(TILE), size.1.div_ceil(TILE));
    #[allow(clippy::cast_precision_loss)]
    let (w, h, t) = (size.0 as f32, size.1 as f32, TILE as f32);
    let mut tiles = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = (col as f32 * t, row as f32 * t);
            let tile = Rect::new(x, y, t.min(w - x), t.min(h - y));
            if let Some(p) = tile.intersect(&part) {
                tiles.push(((col, row), p));
            }
        }
    }
    tiles
}

/// Which part of the land the map window shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapView {
    /// How many times the view fully zoomed out ([`MapView::whole`]): 1 shows all of it.
    pub zoom: f32,
    /// The place in the middle of the view, in land cells; not a number until it is first
    /// settled.
    pub centre: (f32, f32),
    /// Where the pointer last was in a pan under way.
    drag: Option<(f32, f32)>,
    /// Whether the press under way has moved (a pan, not a click).
    dragged: bool,
    /// The land's own bounds, once the picture gives them: what the view fully zoomed out
    /// frames.
    pub bounds: Option<LandBounds>,
}

impl Default for MapView {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            centre: (f32::NAN, f32::NAN),
            drag: None,
            dragged: false,
            bounds: None,
        }
    }
}

/// The side of the land the whole world's view shows, in land cells: its longer side.
fn side(land: (i32, i32)) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let s = land.0.max(land.1).max(1) as f32;
    s
}

impl MapView {
    /// The part of the land shown fully zoomed out, a square: the land's bounds in its middle with
    /// [`FRAME_MARGIN`] about them, past the world's edge where the land comes near it (the sea
    /// beyond); the whole world until the picture gives the bounds.
    #[must_use]
    pub fn whole(&self, land: (i32, i32)) -> Land {
        let s = side(land);
        let Some(b) = self.bounds else {
            return Land {
                x: 0.0,
                y: 0.0,
                w: s,
                h: s,
            };
        };
        #[allow(clippy::cast_precision_loss)]
        let (lw, lh) = (land.0 as f32, land.1 as f32);
        let (x0, x1, y0, y1) = (b.west * lw, b.east * lw, b.south * lh, b.north * lh);
        let w = (x1 - x0).max(y1 - y0) * (1.0 + 2.0 * FRAME_MARGIN);
        let at = |lo: f32, hi: f32| (lo + hi - w) / 2.0;
        Land {
            x: at(x0, x1),
            y: at(y0, y1),
            w,
            h: w,
        }
    }

    /// Frame `bounds` fully zoomed out from now on: a view fully zoomed out moves to frame them.
    pub fn frame(&mut self, bounds: Option<LandBounds>) {
        if self.bounds != bounds {
            self.bounds = bounds;
            if self.zoom <= 1.0 {
                self.centre = (f32::NAN, f32::NAN);
            }
        }
    }

    /// The part of the land shown, a square.
    #[must_use]
    pub fn shown(&self, land: (i32, i32)) -> Land {
        let span = self.whole(land).w / self.zoom;
        Land {
            x: self.centre.0 - span / 2.0,
            y: self.centre.1 - span / 2.0,
            w: span,
            h: span,
        }
    }

    /// Screen pixels to a land cell, the view drawn over `area`.
    #[must_use]
    pub fn pixels_per_cell(&self, area: Rect, land: (i32, i32)) -> f32 {
        area.w / self.shown(land).w
    }

    /// The screen point of the place `at` (land cells), the view drawn over `area`: north up.
    #[must_use]
    pub fn on_screen(&self, area: Rect, land: (i32, i32), at: (f32, f32)) -> (f32, f32) {
        let s = self.shown(land);
        let k = area.w / s.w;
        (area.x + (at.0 - s.x) * k, area.y + (s.y + s.h - at.1) * k)
    }

    /// The place under the screen point `at`, the view drawn over `area`.
    #[must_use]
    pub fn on_land(&self, area: Rect, land: (i32, i32), at: (f32, f32)) -> (f32, f32) {
        let s = self.shown(land);
        let k = s.w / area.w;
        (s.x + (at.0 - area.x) * k, s.y + s.h - (at.1 - area.y) * k)
    }

    /// The most `area` zooms the land.
    #[must_use]
    pub fn most_zoom(&self, area: Rect, land: (i32, i32)) -> f32 {
        (MOST_MAGNIFIED * self.whole(land).w / area.w.max(1.0)).max(1.0)
    }

    /// The view held inside what it shows fully zoomed out, at a zoom `area` can show.
    pub fn settle(&mut self, area: Rect, land: (i32, i32)) {
        let whole = self.whole(land);
        let middle = (whole.x + whole.w / 2.0, whole.y + whole.h / 2.0);
        if self.centre.0.is_nan() || self.centre.1.is_nan() {
            self.centre = middle;
        }
        self.zoom = self.zoom.clamp(1.0, self.most_zoom(area, land));
        // How far the middle may go from the frame's own: none fully zoomed out.
        let reach = (whole.w - whole.w / self.zoom).max(0.0) / 2.0;
        self.centre.0 = self.centre.0.clamp(middle.0 - reach, middle.0 + reach);
        self.centre.1 = self.centre.1.clamp(middle.1 - reach, middle.1 + reach);
    }

    /// Zoom to `zoom`, the place `anchor` staying where it is on the screen.
    pub fn zoom_about(&mut self, area: Rect, land: (i32, i32), zoom: f32, anchor: (f32, f32)) {
        self.settle(area, land);
        let was = self.on_screen(area, land, anchor);
        self.zoom = zoom;
        self.settle(area, land);
        let now = self.on_screen(area, land, anchor);
        let k = self.shown(land).w / area.w;
        self.centre.0 += (now.0 - was.0) * k;
        self.centre.1 -= (now.1 - was.1) * k;
        self.settle(area, land);
    }
}

/// Where the window's parts are, in a window drawn with `body` as its body, at `k` pixels a
/// layout unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parts {
    /// The picture.
    pub map: Rect,
    /// The zoom buttons: out, in, back out to the land framed; and the box that keeps the player in the
    /// middle.
    pub zoom_out: Rect,
    pub zoom_in: Rect,
    pub whole: Rect,
    pub follow: Rect,
    /// Where the player stands; the time of day, and the date under it; a marker's words.
    pub place: Rect,
    pub time: Rect,
    pub date: Rect,
    pub note: Rect,
}

/// The map window.
#[derive(Debug, Default)]
pub struct MapWindow {
    window: WindowState,
    drawn: Option<(Rect, Rect)>,
    pub view: MapView,
    /// The view keeps the player in its middle.
    pub follow: bool,
    /// Follow turned on or off with its box since this was last taken.
    follow_set: Option<bool>,
    /// The words of the marker last clicked.
    pub note: Option<String>,
    /// The world the picture is drawn from.
    picture: Picture,
    /// The world it is drawn from, by its address; `None` for a picture handed over.
    world: Option<usize>,
    /// A picture of the whole built or handed over, kept by the art at the next frame.
    given_overview: Option<Pixels>,
    /// Whether the art keeps a picture of the whole, as the last frame found; and whether the
    /// last frame's view was close enough to want tiles.
    has_overview: bool,
    wants_tiles: bool,
    /// The picture of the whole to let go of at the next frame (another world's).
    forget_overview: bool,
    /// The tiles drawn, and when a view last needed each (the interface's clock).
    tiles: HashMap<(usize, usize), f64>,
    /// The tiles to let go of at the next frame (the window closed).
    let_go: Vec<(usize, usize)>,
    /// The stretches of land of the world drawn, largest first, kept while the world is: where
    /// the continents are named.
    masses: Vec<Landmass>,
}

impl MapWindow {
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.window.open
    }

    /// Open it, following the player while `follow`, or close it.
    pub fn toggle(&mut self, follow: bool) {
        if self.window.open {
            self.close();
        } else {
            self.window.open = true;
            self.follow = follow;
        }
    }

    /// Whether Follow was turned on or off with its box since this was last asked.
    pub fn take_follow_set(&mut self) -> Option<bool> {
        self.follow_set.take()
    }

    /// Close it: the world it was drawn from and the tiles are let go of (the tiles at the next
    /// frame); the picture of the whole is kept.
    pub fn close(&mut self) {
        self.window.open = false;
        self.drawn = None;
        self.note = None;
        self.let_go.extend(self.tiles.drain().map(|(t, _)| t));
        if matches!(self.picture, Picture::Ready(_)) && self.world.is_some() {
            self.picture = Picture::None;
        }
    }

    /// Where it was last drawn, while it is open.
    #[must_use]
    pub fn rect(&self) -> Option<Rect> {
        self.drawn.map(|(r, _)| r)
    }

    /// Where its body was last drawn, while it is open.
    #[must_use]
    pub fn body(&self) -> Option<Rect> {
        self.drawn.map(|(_, b)| b)
    }

    /// The tiles drawn and still kept.
    #[must_use]
    pub fn kept(&self) -> Vec<(usize, usize)> {
        let mut v: Vec<(usize, usize)> = self.tiles.keys().copied().collect();
        v.sort_unstable();
        v
    }

    /// Whether the world the tiles are drawn from is here.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        matches!(self.picture, Picture::Ready(_))
    }

    /// Whether the world is being read and drawn.
    #[must_use]
    pub fn is_building(&self) -> bool {
        matches!(self.picture, Picture::Building(_))
    }

    /// Whether the world held no land to draw: the map shows the sea until another world comes.
    #[must_use]
    pub fn is_failed(&self) -> bool {
        matches!(self.picture, Picture::Failed)
    }

    /// Draw from `built` rather than the world the game plays in: a world drawn elsewhere.
    pub fn show(&mut self, built: Built) {
        self.masses = built.map.landmasses().to_vec();
        self.view.frame(built.map.land_bounds());
        self.picture = Picture::Ready(built.map);
        self.world = None;
        if built.overview.is_some() {
            self.given_overview = built.overview;
        }
    }

    /// The picture kept up with `world`: built, while the window is open, when the picture of
    /// the whole is missing or the last view wanted tiles; taken once built; and built again for
    /// another world. Where there are no threads, polling it here is where a share of the
    /// frame's time builds it.
    pub fn keep_picture(&mut self, world: &WorldData) {
        let at = std::ptr::from_ref(world) as usize;
        let given = self.world.is_none() && self.is_ready();
        if !given && self.world != Some(at) {
            // Another world: neither the old picture nor its tiles will do.
            self.picture = Picture::None;
            self.world = Some(at);
            self.given_overview = None;
            self.has_overview = false;
            self.forget_overview = true;
            self.masses.clear();
            self.view.frame(None);
            self.let_go.extend(self.tiles.drain().map(|(t, _)| t));
        }
        if let Picture::Building(build) = &mut self.picture {
            match build.poll() {
                Some(Ok(built)) => {
                    if built.overview.is_some() {
                        self.given_overview = built.overview;
                        self.has_overview = true;
                    }
                    self.masses = built.map.landmasses().to_vec();
                    self.view.frame(built.map.land_bounds());
                    self.picture = Picture::Ready(built.map);
                }
                Some(Err(e)) => {
                    tracing::warn!("the world map was not drawn: {e}");
                    self.picture = Picture::Failed;
                }
                None => {}
            }
        }
        let open = self.window.open;
        if given || !open || !matches!(self.picture, Picture::None) {
            return;
        }
        if self.has_overview && !self.wants_tiles {
            return;
        }
        let land = Arc::new(MapLand::of(&world.region));
        let build = MapBuild::start(world.records.landblock_reader(), land, !self.has_overview);
        self.picture = Picture::Building(Box::new(build));
    }

    /// Where the parts are in a window drawn with `body` as its body, at `k` pixels a layout
    /// unit.
    #[must_use]
    pub fn parts(body: Rect, k: f32) -> Parts {
        let side = (body.w - 16.0 * k).min(body.h - (UNDER + 8.0) * k);
        let map = Rect::new(body.x + (body.w - side) / 2.0, body.y + 8.0 * k, side, side);
        let y = map.bottom() + 6.0 * k;
        let button = |i: f32, w: f32| Rect::new(map.x + i * k, y, w * k, 26.0 * k);
        let line = |i: f32| Rect::new(map.x, y + (30.0 + i * 20.0) * k, map.w, 18.0 * k);
        Parts {
            map,
            zoom_out: button(0.0, 30.0),
            zoom_in: button(34.0, 30.0),
            whole: button(68.0, 60.0),
            follow: button(140.0, 90.0),
            place: Rect::new(map.x + 236.0 * k, y + 4.0 * k, map.w - 236.0 * k, 18.0 * k),
            time: line(0.0),
            date: line(1.0),
            note: line(2.0),
        }
    }

    /// Draw it while it is open; where it is.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        tip: &mut Option<Tip>,
    ) -> Option<Rect> {
        for (col, row) in self.let_go.drain(..) {
            p.art.let_go(&tile_key(col, row));
        }
        if std::mem::take(&mut self.forget_overview) {
            p.art.let_go(OVERVIEW_KEY);
        }
        if let Some(given) = self.given_overview.take() {
            p.art.let_go(OVERVIEW_KEY);
            p.art.cached(OVERVIEW_KEY, || Some(image(given)));
        }
        if !self.window.open {
            return None;
        }
        let k = p.scale;
        let size = kit::window_size_for(p, BODY);
        let at = (
            (p.screen.0 * 0.5 - size.0 * k * 0.5).round(),
            ((p.screen.1 - size.1 * k) / 2.0).round(),
        );
        let at = crate::ui::place::clear(p, state, size, at);
        let w = kit::window(p, ctx, &mut self.window, "Map", size, at);
        if w.closed {
            self.close();
            return Some(w.rect);
        }
        self.drawn = Some((w.rect, w.body));
        let parts = Self::parts(w.body, k);
        let atlas = state.map.atlas.clone().unwrap_or_default();
        let land = atlas.land;
        let area = parts.map;
        self.view.settle(area, land);
        self.buttons(p, ctx, state, &parts, land);
        let clicked = self.pointer(ctx, area, land);
        if let Some((at, _)) = state.map.player.filter(|_| self.follow) {
            self.view.centre = at;
            self.view.settle(area, land);
        }
        p.list.push_clip(area);
        self.draw_picture(p, ctx.time, area, land);
        let pointed = self.marks(p, ctx, state, area, &atlas, clicked);
        self.player(p, state, area, land);
        p.list.pop_clip();
        if let Some(t) = pointed {
            *tip = Some(t);
        }
        let style = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let place = if state.map.inside {
            Some("Indoors".to_owned())
        } else {
            state.radar.coordinates.map(|(east, north)| {
                format!(
                    "{:.1}{} {:.1}{}",
                    north.abs(),
                    if north >= 0.0 { "N" } else { "S" },
                    east.abs(),
                    if east >= 0.0 { "E" } else { "W" }
                )
            })
        };
        if let Some(place) = place {
            p.text_in(&style, parts.place, Align::Right, &place);
        }
        for (line, words) in [(parts.time, &state.map.time), (parts.date, &state.map.date)] {
            if let Some(w) = words {
                p.text_in(&style, line, Align::Centre, w);
            }
        }
        if let Some(n) = &self.note {
            p.text_in(&style, parts.note, Align::Centre, n);
        }
        Some(w.rect)
    }

    /// The zoom buttons: in and out about the player while in view (else the view's middle),
    /// and back out to the land framed.
    fn buttons(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        parts: &Parts,
        land: (i32, i32),
    ) {
        let area = parts.map;
        let view = &mut self.view;
        let shown = view.shown(land);
        let anchor = state
            .map
            .player
            .map(|(at, _)| at)
            .filter(|at| shown.holds(*at))
            .unwrap_or(view.centre);
        let most = view.most_zoom(area, land);
        if kit::button(p, ctx, parts.zoom_out, "-", view.zoom > 1.0) {
            view.zoom_about(area, land, view.zoom / BUTTON_STEP, anchor);
        }
        if kit::button(p, ctx, parts.zoom_in, "+", view.zoom < most) {
            view.zoom_about(area, land, view.zoom * BUTTON_STEP, anchor);
        }
        if kit::button(p, ctx, parts.whole, "Fit", view.zoom > 1.0) {
            view.zoom_about(area, land, 1.0, anchor);
        }
        let style = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        if let Some(on) = kit::check_row(p, ctx, parts.follow, self.follow, &style, "Follow") {
            self.follow = on;
            self.follow_set = Some(on);
        }
    }

    /// The wheel over the picture zooms about the pointer, and a drag begun on it pans; whether
    /// a press let go of without moving (a click) ended this frame, and where.
    fn pointer(&mut self, ctx: &mut Ctx<'_>, area: Rect, land: (i32, i32)) -> Option<(f32, f32)> {
        let view = &mut self.view;
        let mouse = ctx.input.mouse;
        let over = ctx.over(&area);
        let mut panned = false;
        if over && ctx.input.wheel != 0.0 {
            let zoom = view.zoom * dereth_primitives::num::math::powf(WHEEL_STEP, ctx.input.wheel);
            let anchor = view.on_land(area, land, mouse);
            view.zoom_about(area, land, zoom, anchor);
            ctx.input.wheel = 0.0;
        }
        if let Some(last) = view.drag {
            if ctx.input.down[0] {
                if (last.0 - mouse.0).abs() > 0.0 || (last.1 - mouse.1).abs() > 0.0 {
                    view.dragged = true;
                    panned = true;
                }
                let k = view.shown(land).w / area.w;
                view.centre.0 += (last.0 - mouse.0) * k;
                view.centre.1 -= (last.1 - mouse.1) * k;
                view.settle(area, land);
                view.drag = Some(mouse);
                ctx.input.captured = true;
            } else {
                view.drag = None;
                if !std::mem::take(&mut view.dragged) {
                    return Some(mouse);
                }
            }
        } else if over && ctx.input.clicked(&area) {
            view.drag = Some(mouse);
            view.dragged = false;
        }
        // Panned away: the view no longer follows the player.
        if panned {
            self.follow = false;
        }
        None
    }

    /// The part of the picture the view shows, the sea until the picture of the whole is drawn.
    /// Zoomed in past what the picture of the whole holds it is drawn from the tiles, one newly
    /// drawn a frame, the picture of the whole under any not drawn yet.
    fn draw_picture(&mut self, p: &mut Painter<'_>, now: f64, area: Rect, land: (i32, i32)) {
        let shown = self.view.shown(land);
        let overview = p.art.lookup(OVERVIEW_KEY).flatten();
        self.has_overview = overview.is_some() || self.given_overview.is_some();
        p.fill(area, sea());
        let map = match &self.picture {
            Picture::Ready(map) => Some(Arc::clone(map)),
            _ => None,
        };
        // The picture's size: the world's, else the whole's, which keeps its proportions.
        let size = map.as_ref().map(|m| m.size()).or_else(|| {
            overview.map(|o| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let s = (o.src.w as usize, o.src.h as usize);
                s
            })
        });
        let Some(size) = size else {
            self.wants_tiles = false;
            return;
        };
        let part = on_picture(shown, land, size);
        let draw = |p: &mut Painter<'_>, sprite: &Sprite, src: Rect, of: Rect| {
            let k = area.w / part.w;
            let dst = Rect::new(
                area.x + (of.x - part.x) * k,
                area.y + (of.y - part.y) * k,
                of.w * k,
                of.h * k,
            );
            p.sprite(&Sprite { src, ..*sprite }, dst, WHITE);
        };
        // What the view shows of the picture itself; past its edge is the sea already filled.
        #[allow(clippy::cast_precision_loss)]
        let inside = part.intersect(&Rect::new(0.0, 0.0, size.0 as f32, size.1 as f32));
        let draw_overview = |p: &mut Painter<'_>| {
            if let (Some(whole), Some(part)) = (&overview, inside) {
                #[allow(clippy::cast_precision_loss)]
                let (kx, ky) = (whole.src.w / size.0 as f32, whole.src.h / size.1 as f32);
                let src = Rect::new(
                    whole.src.x + part.x * kx,
                    whole.src.y + part.y * ky,
                    part.w * kx,
                    part.h * ky,
                );
                draw(p, whole, src, part);
            }
        };
        // Screen pixels to a pixel of the picture of the whole.
        #[allow(clippy::cast_precision_loss)]
        let close = overview
            .is_none_or(|o| area.w / part.w * size.0 as f32 / o.src.w.max(1.0) > OVERVIEW_STRETCH);
        self.wants_tiles = close;
        if !close {
            draw_overview(p);
            self.let_tiles_go(p, now - TILE_KEPT);
            return;
        }
        let mut drawn = false;
        let mut parts = Vec::new();
        let mut waiting = false;
        for ((col, row), of) in tiles_under(part, size) {
            let key = tile_key(col, row);
            let tile = p.art.lookup(&key).unwrap_or_else(|| {
                let map = map.as_ref().filter(|_| !drawn)?;
                drawn = true;
                p.art.cached(&key, || Some(image(map.tile((col, row)))))
            });
            match tile {
                Some(tile) => {
                    self.tiles.insert((col, row), now);
                    parts.push((tile, (col, row), of));
                }
                None => waiting = true,
            }
        }
        if waiting {
            draw_overview(p);
        }
        for (tile, (col, row), of) in parts {
            #[allow(clippy::cast_precision_loss)]
            let src = Rect::new(
                tile.src.x + of.x - (col * TILE) as f32,
                tile.src.y + of.y - (row * TILE) as f32,
                of.w,
                of.h,
            );
            draw(p, &tile, src, of);
        }
        self.let_tiles_go(p, now - TILE_KEPT);
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

    /// Zoomed out, the continents' names alone, each in the middle of its land
    /// ([`crate::atlas::on_the_land`]), kept inside the picture.
    fn continents(&self, p: &mut Painter<'_>, ctx: &Ctx<'_>, area: Rect, atlas: &Atlas) {
        let style = TextStyle::new(Family::Heading, CONTINENT_TEXT, ctx.colours.text())
            .edge(ctx.colours.edge());
        for c in &crate::atlas::on_the_land(&atlas.continents, &self.masses, atlas.land) {
            let (x, y) = self.view.on_screen(area, atlas.land, c.at);
            let (w, h) = (p.measure(&style, &c.name), p.line_height(&style));
            let left = (x - w / 2.0).clamp(area.x, (area.right() - w).max(area.x));
            let top = (y - h / 2.0).clamp(area.y, (area.bottom() - h).max(area.y));
            p.text(&style, left, top, &c.name);
        }
    }

    /// Zoomed in, the closer maps' labels and the game's markers: each its picture (a plain mark
    /// without one); the one nearest the pointer named; a click on a marker writes its words.
    fn marks(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        area: Rect,
        atlas: &Atlas,
        clicked: Option<(f32, f32)>,
    ) -> Option<Tip> {
        let land = atlas.land;
        if self.view.pixels_per_cell(area, land) < MARKS_FROM {
            self.continents(p, ctx, area, atlas);
            return None;
        }
        let k = p.scale;
        let side = MARK_SIDE * k;
        let mouse = ctx.input.mouse;
        let pointing = ctx.input.hover(&area);
        let mut nearest: Option<(f32, Tip, Option<String>)> = None;
        let mark = |p: &mut Painter<'_>, at: (f32, f32), icon: u32, colour: u32| {
            let (x, y) = self.view.on_screen(area, land, at);
            if !area.contains(x, y) {
                return None;
            }
            let r = Rect::new(x - side / 2.0, y - side / 2.0, side, side);
            match p.art.icon(icon) {
                Some(s) => p.sprite(&s, r, WHITE),
                None => {
                    p.fill(r, 0xFF10_1010);
                    p.fill(r.inset(k.max(1.0)), colour);
                }
            }
            let d = dereth_primitives::num::math::hypotf(mouse.0 - x, mouse.1 - y);
            (pointing && d <= REACH * k).then_some(d)
        };
        for l in &atlas.labels {
            if let Some(d) = mark(p, l.at, l.icon, LABEL) {
                if nearest.as_ref().is_none_or(|(e, ..)| d < *e) {
                    let tip = Tip {
                        title: l.words.clone().unwrap_or_default(),
                        lines: vec![l.coordinates.clone()],
                    };
                    nearest = Some((d, tip, None));
                }
            }
        }
        for m in state.map.markers.iter().chain(&state.map.fellows) {
            if let Some(d) = mark(p, m.at, m.picture, MARKER) {
                if nearest.as_ref().is_none_or(|(e, ..)| d <= *e) {
                    let tip = Tip {
                        title: m.words.clone(),
                        lines: vec![m.coordinates.clone()],
                    };
                    nearest = Some((d, tip, Some(m.words.clone())));
                }
            }
        }
        let (_, tip, words) = nearest?;
        if clicked.is_some() {
            if let Some(w) = words {
                self.note = Some(w);
            }
        }
        Some(tip)
    }

    /// Where the player stands, while it is outdoors and inside the picture: an arrow the way it
    /// faces, where the arrow turned any way stays inside the picture, else a plain mark cut at
    /// the picture's edge (a turned picture is not cut).
    fn player(&self, p: &mut Painter<'_>, state: &GameState, area: Rect, land: (i32, i32)) {
        let Some((at, heading)) = state.map.player else {
            return;
        };
        let (x, y) = self.view.on_screen(area, land, at);
        if !area.contains(x, y) {
            return;
        }
        let s = 10.0 * p.scale;
        let arrow = p.piece("minimap.arrow").and_then(|arrow| {
            let h = 2.0 * s;
            let w = h * arrow.w / arrow.h.max(1.0);
            let reach = dereth_primitives::num::math::hypotf(w, h) / 2.0;
            let inside = x - reach >= area.x
                && y - reach >= area.y
                && x + reach <= area.right()
                && y + reach <= area.bottom();
            inside.then_some((arrow, Rect::new(x - w / 2.0, y - h / 2.0, w, h)))
        });
        match arrow {
            Some((arrow, r)) => p.sprite_turned(&arrow, r, WHITE, heading.to_radians()),
            None => p.fill(Rect::new(x - s / 2.0, y - s / 2.0, s, s), PLAYER_MARK),
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
