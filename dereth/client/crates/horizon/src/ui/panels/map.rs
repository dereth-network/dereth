//! The map window: the world's picture, zoomed with the wheel or its buttons and panned by dragging.
//!
//! The picture draws three of its pixels to a landcell, 6123 to a side. It is drawn from the
//! landblocks of the data the client plays with ([`dereth_scene::world_map`]): the world is read
//! and the picture of the whole at a third of that size ([`OVERVIEW`]) drawn off the frame when
//! the map first opens, and tiles of it at its own size ([`TILE`]) are drawn as a view first needs
//! them, one a frame, and let go when no view has needed them for a while. Closing the map lets
//! the world and the tiles go and keeps the picture of the whole.
//!
//! The picture names nothing: the places are drawn over it, read from the same data
//! ([`dereth_scene::world_map::places`]) and the places the player has learned of the world. The
//! whole world shows its towns alone, each a [`town_icon`] with its name. Closing in marks the
//! landmarks and dungeons, then the lifestones and portals, the houses and the vendors, and the
//! people last, each kind named a step after it is marked ([`marked_from`], [`named_from`]). The
//! steps go by how large the world is drawn on the screen, so a step is the same in any window,
//! and the closest view always reaches the people's names. A name that would overlap another is
//! left out, the towns' first.

use std::collections::HashMap;
use std::sync::Arc;

use dereth_client_contract::panels::map::{MapButton, MapClick};
use dereth_primitives::num::math::hypotf;
use dereth_scene::world_map::{Built, MapBuild, Pixels, Place, PlaceKind, WorldMap};
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
/// A town's icon, on the screen at this many layout units a side.
const TOWN_UNITS: f32 = 16.0;
/// The town icon's picture's side, in its pixels.
const TOWN_SIDE: u32 = 64;
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

/// The detail of the closest view: screen pixels to a unit of the game's coordinates, the most a
/// picture pixel is magnified times the picture's pixels to a unit, in any window.
#[allow(clippy::cast_possible_truncation)] // LINT-OK: 90.
pub const MOST_DETAIL: f32 = MOST_MAGNIFIED * PER_UNIT as f32;

/// The detail from which the map marks a place of `kind`, in screen pixels to a unit of the game's
/// coordinates ([`MapView::detail`]); towns at any, and when the whole world is shown, them alone
/// ([`MapView::marks`]). On a screen of 1080 lines at 100% the steps fall at the zooms 2, 3, 4, 6
/// and 10.
#[must_use]
pub const fn marked_from(kind: PlaceKind) -> f32 {
    match kind {
        PlaceKind::Town => 0.0,
        PlaceKind::Landmark | PlaceKind::Dungeon => 4.0,
        PlaceKind::Lifestone | PlaceKind::Portal => 7.0,
        PlaceKind::Mansion => 9.0,
        PlaceKind::Villa | PlaceKind::Cottage | PlaceKind::Vendor => 14.0,
        PlaceKind::Npc => 24.0,
    }
}

/// The detail from which the map names a place of `kind` beside its mark, a step after it is
/// marked; `None` for the houses, which are named only under the pointer. On a screen of 1080
/// lines at 100% the steps fall at the zooms 3, 5, 8 and 14.
#[must_use]
pub const fn named_from(kind: PlaceKind) -> Option<f32> {
    match kind {
        PlaceKind::Town => Some(0.0),
        PlaceKind::Landmark | PlaceKind::Dungeon => Some(7.0),
        PlaceKind::Lifestone | PlaceKind::Portal => Some(12.0),
        PlaceKind::Vendor => Some(19.0),
        PlaceKind::Npc => Some(34.0),
        PlaceKind::Mansion | PlaceKind::Villa | PlaceKind::Cottage => None,
    }
}

/// The names that fit, by rank (the lowest first, and in their order within a rank): each kept
/// clear of everything in `taken`, which takes it in turn; one that would overlap is left out.
fn fit_names<T>(taken: &mut Vec<Rect>, mut names: Vec<(u8, Rect, T)>) -> Vec<(Rect, T)> {
    names.sort_by_key(|(rank, _, _)| *rank);
    let mut kept = Vec::new();
    for (_, r, it) in names {
        if !taken.iter().any(|t| t.intersect(&r).is_some()) {
            taken.push(r);
            kept.push((r, it));
        }
    }
    kept
}

/// Which names have the room first when two would overlap: the lower, the sooner.
const fn name_rank(kind: PlaceKind) -> u8 {
    match kind {
        PlaceKind::Town => 0,
        PlaceKind::Landmark => 1,
        PlaceKind::Dungeon => 2,
        PlaceKind::Lifestone => 3,
        PlaceKind::Portal => 4,
        PlaceKind::Vendor => 5,
        PlaceKind::Npc => 6,
        PlaceKind::Mansion | PlaceKind::Villa | PlaceKind::Cottage => 7,
    }
}

/// The colour of a place of `kind`'s mark.
#[must_use]
pub const fn colour(kind: PlaceKind) -> u32 {
    match kind {
        PlaceKind::Town | PlaceKind::Landmark => 0xFFF0_C860,
        PlaceKind::Cottage => 0xFF40_B050,
        PlaceKind::Villa => 0xFF40_70E0,
        PlaceKind::Mansion => 0xFFE0_4040,
        PlaceKind::Lifestone => 0xFF60_C0F0,
        PlaceKind::Portal => 0xFFB0_60E0,
        PlaceKind::Dungeon => 0xFFE0_8030,
        PlaceKind::Vendor => 0xFFE0_E050,
        PlaceKind::Npc => 0xFFC8_C8C8,
    }
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
    Building(Box<MapBuild>),
    /// Built, or handed over ([`MapView::show`]).
    Ready(Arc<WorldMap>),
    /// The data held no world to draw: the plain ground until other data comes.
    Failed,
}

/// What the places drawn were made from: the world, the revision of its learned places, and the
/// data's places, by their address.
type LearnedFrom = (String, u64, usize);

/// Which part of the picture the map window shows.
#[derive(Debug)]
pub struct MapView {
    /// How many times the fit: 1 shows the whole picture.
    pub zoom: f32,
    /// The picture's pixel in the middle of the view.
    pub centre: (f32, f32),
    /// Where the out, in and fit buttons were drawn this frame.
    pub buttons: Option<[Rect; 3]>,
    /// The places named over the picture this frame, and where each name was drawn.
    pub named: Vec<(PlaceKind, String, Rect)>,
    /// Where the pointer last was in a pan under way.
    drag: Option<(f32, f32)>,
    /// The tiles drawn, and when a view last needed each.
    tiles: HashMap<(u32, u32), f64>,
    /// The places the data names, once read.
    places: Option<Arc<Vec<Place>>>,
    /// Those and the places the player learned of this world, and what they were made from: the
    /// world, its places' revision and the data's places.
    with_learned: Option<(LearnedFrom, Arc<Vec<Place>>)>,
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
            named: Vec::new(),
            drag: None,
            tiles: HashMap::new(),
            places: None,
            with_learned: None,
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

    /// How large the view over `area` draws the world: screen pixels to a unit of the game's
    /// coordinates. The whole picture fitted to the map's 514 layout units is about 2.5 a unit at
    /// a scale of 1; the closest view is [`MOST_DETAIL`] in any window.
    #[must_use]
    pub fn detail(&self, area: Rect) -> f32 {
        #[allow(clippy::cast_possible_truncation)] // LINT-OK: 30.
        let per_unit = PER_UNIT as f32;
        self.zoom * area.w / MAP_SIZE * per_unit
    }

    /// Whether the view over `area` marks a place of `kind`: the whole world its towns alone.
    #[must_use]
    pub fn marks(&self, area: Rect, kind: PlaceKind) -> bool {
        kind == PlaceKind::Town || (self.zoom > 1.0 && self.detail(area) >= marked_from(kind))
    }

    /// Whether the view over `area` names a place of `kind` beside its mark.
    #[must_use]
    pub fn names(&self, area: Rect, kind: PlaceKind) -> bool {
        self.marks(area, kind) && named_from(kind).is_some_and(|d| self.detail(area) >= d)
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
        self.places = Some(built.places);
    }

    /// The places to draw: the data's (`shared`, the client's own set, which its plugins read too;
    /// else those read with the picture), and over them those the player learned of `world` (its
    /// places and their revision), made again only when either changes.
    pub fn places_with(
        &mut self,
        shared: Option<&Arc<Vec<Place>>>,
        world: Option<&(Arc<dereth_navigation::PlaceStore>, u64)>,
    ) -> Option<Arc<Vec<Place>>> {
        let data = shared.cloned().or_else(|| self.places.clone())?;
        let Some((store, revision)) = world else {
            return Some(data);
        };
        let key = (store.world.clone(), *revision, Arc::as_ptr(&data) as usize);
        if let Some((k, all)) = &self.with_learned {
            if *k == key {
                return Some(Arc::clone(all));
            }
        }
        let mut all = (*data).clone();
        all.extend(dereth_scene::world_map::places::learned(
            store.places(),
            &data,
        ));
        let all = Arc::new(all);
        self.with_learned = Some((key, Arc::clone(&all)));
        Some(all)
    }

    /// Whether the world the tiles are drawn from is here.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        matches!(self.picture, Picture::Ready(_))
    }

    /// The picture kept up with the data `art` holds: built when the picture of the whole or the
    /// places are missing or `close` asks for tiles, taken once built, and built again for other
    /// data.
    fn keep_picture(&mut self, art: &Art, close: bool) {
        if let Some(given) = self.given_overview.take() {
            art.let_go(OVERVIEW_KEY);
            art.cached(OVERVIEW_KEY, || Some(image(given)));
        }
        let store = art.ac_data();
        let data = store.as_ref().map(|s| Arc::as_ptr(s) as usize);
        let given = self.data.is_none() && self.is_ready();
        if !given && data != self.data {
            // Other data draws another world: neither the old picture nor its places will do.
            self.picture = Picture::None;
            self.data = data;
            self.places = None;
            art.let_go(OVERVIEW_KEY);
        }
        if let Picture::Building(build) = &mut self.picture {
            match build.poll() {
                Some(Ok(built)) => {
                    if let Some(overview) = built.overview {
                        art.let_go(OVERVIEW_KEY);
                        art.cached(OVERVIEW_KEY, || Some(image(overview)));
                    }
                    self.picture = Picture::Ready(built.map);
                    self.places = Some(built.places);
                }
                Some(Err(e)) => {
                    tracing::warn!("the world map was not drawn: {e}");
                    self.picture = Picture::Failed;
                }
                None => {}
            }
        }
        let overview = art.lookup(OVERVIEW_KEY).is_some();
        let whole = overview && self.places.is_some();
        if given || (whole && !close) || !matches!(self.picture, Picture::None | Picture::Ready(_))
        {
            return;
        }
        if whole && self.is_ready() {
            return;
        }
        if let Some(store) = store {
            self.picture = Picture::Building(Box::new(MapBuild::start(store, !overview)));
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
        self.map_places(p, ctx, state, area);
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

    /// The places over the picture: the towns' icons and names, then the other places' marks as
    /// the zoom shows them ([`marked_from`]), with their names a step later ([`named_from`]) where
    /// they fit; and the name and place of the one under the pointer. None until the data has
    /// been read.
    fn map_places(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        area: Rect,
    ) {
        self.map_view.named.clear();
        let Some(places) = self
            .map_view
            .places_with(state.map_places.as_ref(), state.world_places.as_ref())
        else {
            return;
        };
        let view = &self.map_view;
        let mut named: Vec<(PlaceKind, String, Rect)> = Vec::new();
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
                None => p.fill(r, colour(PlaceKind::Town)),
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
                named.push((place.kind, place.name.clone(), name));
            }
        }
        {
            let mouse = ctx.input.mouse;
            let pointed = pointed_at.is_none() && ctx.input.hover(&area);
            let mark_side = MARK_SIDE * k;
            let mut nearest: Option<(f32, &Place)> = None;
            let shown = |kind: PlaceKind| kind != PlaceKind::Town && view.marks(area, kind);
            // The names to place once every mark is down, by their rank: one that would overlap a
            // name before it is left out at this zoom.
            let mut names: Vec<(u8, (f32, f32), &Place)> = Vec::new();
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
                    colour(place.kind),
                );
                if pointed {
                    let d = hypotf(mouse.0 - at.0, mouse.1 - at.1);
                    if d <= MARK_REACH * k && nearest.is_none_or(|(best, _)| d < best) {
                        nearest = Some((d, place));
                    }
                }
                if view.names(area, place.kind) {
                    names.push((name_rank(place.kind), at, place));
                }
            }
            let boxes: Vec<(u8, Rect, &Place)> = names
                .into_iter()
                .map(|(rank, at, place)| {
                    let (w, h) = (p.measure(&small, &place.name), p.line_height(&small));
                    (
                        rank,
                        Rect::new(at.0 + mark_side, at.1 - h / 2.0, w, h),
                        place,
                    )
                })
                .collect();
            for (r, place) in fit_names(&mut taken, boxes) {
                p.text(&small, r.x, r.y, &place.name);
                named.push((place.kind, place.name.clone(), r));
            }
            pointed_at = pointed_at.or(nearest.map(|(_, place)| place));
        }
        p.list.pop_clip();
        let pointed_at = pointed_at.map(|place| (place.name.clone(), place.ns, place.ew));
        self.map_view.named = named;
        if let Some((name, ns, ew)) = pointed_at {
            #[allow(clippy::cast_possible_truncation)] // LINT-OK: a coordinate within the world.
            let at = coords_text(ns as f32, ew as f32);
            self.tip = Some((name, vec![at]));
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

    const KINDS: [PlaceKind; 10] = [
        PlaceKind::Town,
        PlaceKind::Landmark,
        PlaceKind::Dungeon,
        PlaceKind::Lifestone,
        PlaceKind::Portal,
        PlaceKind::Mansion,
        PlaceKind::Villa,
        PlaceKind::Cottage,
        PlaceKind::Vendor,
        PlaceKind::Npc,
    ];

    /// The kinds a view at `zoom` over a picture `side` pixels across marks, and those it names.
    fn shown_at(side: f32, zoom: f32) -> (Vec<PlaceKind>, Vec<PlaceKind>) {
        let area = Rect::new(0.0, 0.0, side, side);
        let view = MapView {
            zoom,
            ..MapView::default()
        };
        let pick = |f: &dyn Fn(PlaceKind) -> bool| KINDS.into_iter().filter(|k| f(*k)).collect();
        (
            pick(&|k| view.marks(area, k)),
            pick(&|k| view.names(area, k)),
        )
    }

    /// The map's picture at 1920x1080 and 100%, and one four times as wide.
    const SMALL: f32 = 514.0;
    const LARGE: f32 = 2056.0;

    /// The whole map marks and names its towns alone; closing in marks the landmarks and dungeons,
    /// then the portals and lifestones, the houses and vendors, and the people last; and each kind
    /// is named only from a step after the one that marks it.
    #[test]
    fn the_whole_map_shows_towns_alone_and_each_kind_is_named_after_it_is_marked() {
        use PlaceKind::{
            Cottage, Dungeon, Landmark, Lifestone, Mansion, Npc, Portal, Town, Vendor, Villa,
        };
        let at = |zoom| shown_at(SMALL, zoom);
        assert_eq!(at(1.0), (vec![Town], vec![Town]));
        assert_eq!(at(2.0), (vec![Town, Landmark, Dungeon], vec![Town]));
        let (marked, named) = at(3.0);
        assert_eq!(marked, [Town, Landmark, Dungeon, Lifestone, Portal]);
        for k in [Vendor, Mansion, Villa, Cottage, Npc] {
            assert!(!marked.contains(&k), "{k:?} marked at 3");
        }
        assert_eq!(named, [Town, Landmark, Dungeon]);
        assert!(at(4.0).0.contains(&Mansion) && !at(4.0).0.contains(&Vendor));
        assert!(at(6.0).0.contains(&Vendor) && at(6.0).0.contains(&Villa));
        assert!(
            !at(9.0).0.contains(&Npc),
            "people only at the closest zooms"
        );
        assert_eq!(at(10.0).0.len(), KINDS.len());
        assert!(!at(10.0).1.contains(&Npc) && at(14.0).1.contains(&Npc));
        for k in KINDS {
            if let Some(d) = named_from(k) {
                assert!(d >= marked_from(k), "{k:?} named before it is marked");
                if k != Town {
                    assert!(d > marked_from(k), "{k:?} named as soon as it is marked");
                }
            }
        }
    }

    /// A step shows the same in any window: a picture four times as wide shows at a quarter of
    /// the zoom what the smaller shows; the whole world is still its towns alone, and the closest
    /// view names the people in both.
    #[test]
    fn a_zoom_step_shows_the_same_in_a_window_four_times_as_wide() {
        for zoom in [6.0, 8.0, 10.0, 14.0, 20.0] {
            assert_eq!(
                shown_at(LARGE, zoom / 4.0),
                shown_at(SMALL, zoom),
                "zoom {zoom} against {}",
                zoom / 4.0
            );
        }
        assert_eq!(
            shown_at(LARGE, 1.0),
            (vec![PlaceKind::Town], vec![PlaceKind::Town])
        );
        for side in [SMALL, LARGE] {
            let most = most_zoom(Rect::new(0.0, 0.0, side, side));
            assert!(
                shown_at(side, most).1.contains(&PlaceKind::Npc),
                "{side} pixels across: the people are not named at the closest, {most}"
            );
            assert!(
                (MapView {
                    zoom: most,
                    ..MapView::default()
                }
                .detail(Rect::new(0.0, 0.0, side, side))
                    - MOST_DETAIL)
                    .abs()
                    < 0.01
            );
        }
        assert!(
            most_zoom(Rect::new(0.0, 0.0, LARGE, LARGE)) < 10.0,
            "the larger picture's closest is under the zoom that marked the people before"
        );
    }

    /// Names never overlap: one that would is left out, the towns' first and then by rank,
    /// whatever order the places come in.
    #[test]
    fn a_name_that_would_overlap_another_is_left_out_by_rank() {
        let r = |x: f32, y: f32| Rect::new(x, y, 40.0, 10.0);
        let mut taken = vec![r(0.0, 0.0)];
        let names = vec![
            (name_rank(PlaceKind::Npc), r(30.0, 100.0), "Guard"),
            (name_rank(PlaceKind::Portal), r(20.0, 5.0), "under the town"),
            (name_rank(PlaceKind::Landmark), r(10.0, 102.0), "Landmark"),
            (name_rank(PlaceKind::Vendor), r(200.0, 0.0), "clear"),
        ];
        let kept: Vec<&str> = fit_names(&mut taken, names)
            .into_iter()
            .map(|(_, n)| n)
            .collect();
        assert_eq!(kept, ["Landmark", "clear"]);
        for (i, a) in taken.iter().enumerate() {
            for b in &taken[i + 1..] {
                assert!(a.intersect(b).is_none(), "{a:?} {b:?}");
            }
        }
    }

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

    /// Opened over the game's data, the map reads the world and its places and draws the picture
    /// of the whole off the frame, every place on the landscape, then for a close view the world
    /// again without the picture of the whole.
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
        let places = view.places.clone().expect("the places");
        assert!(places.iter().any(|p| p.name == "Holtburg"), "Holtburg");
        for place in places.iter() {
            let (x, y) = map_pixel(place.ns, place.ew);
            assert!(map_place(x, y).is_some(), "{place:?}");
        }
        eprintln!("first picture of the whole: {ms:.0} ms over {frames} frames");
        view.picture = Picture::None;
        let (_, ms) = wait(&mut view, true);
        eprintln!("the world again, for tiles: {ms:.0} ms");
    }
}
