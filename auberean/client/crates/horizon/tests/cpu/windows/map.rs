//! The map window: where it draws a place, its zoom about the pointer and the player, the picture
//! of the world it draws (the whole of it zoomed out, its tiles zoomed in), the markers from the
//! view, and where the player stands.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use std::sync::{Arc, OnceLock};

use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::{MapMarker, MapMarkers};
use auberean_horizon::atlas::{
    body_place, cell_middle, fellow_markers, Atlas, Continent, Label, MapState, Marker,
    FELLOWS_OFF_MAP, FELLOW_MARKERS,
};
use auberean_horizon::draw::Rect;
use auberean_horizon::keys::{action, keyed, Keyed};
use auberean_horizon::state::GameState;
use auberean_horizon::ui::map::{
    on_picture, tile_key, tiles_under, MapView, MapWindow, MARKS_FROM, OVERVIEW_KEY,
};
use auberean_horizon::world_map::{Built, MapGrid, Pixels, WorldMap};

use crate::{centre, in_world, Harness};

const LAND: (i32, i32) = (2040, 2040);

fn area() -> Rect {
    Rect::new(100.0, 50.0, 600.0, 600.0)
}

#[test]
fn a_place_is_drawn_north_up_and_the_point_under_it_is_that_place() {
    let mut v = MapView::default();
    v.settle(area(), LAND);
    assert_eq!(
        v.centre,
        (1020.0, 1020.0),
        "the whole land, its middle in the middle"
    );
    let (x, y) = v.on_screen(area(), LAND, (0.0, 2040.0));
    assert!(
        (x - 100.0).abs() < 1e-3 && (y - 50.0).abs() < 1e-3,
        "the north-west corner at the top left"
    );
    let (x, y) = v.on_screen(area(), LAND, (2040.0, 0.0));
    assert!((x - 700.0).abs() < 1e-3 && (y - 650.0).abs() < 1e-3);
    v.zoom_about(area(), LAND, 4.0, (300.0, 1700.0));
    let at = v.on_land(area(), LAND, v.on_screen(area(), LAND, (812.5, 1203.0)));
    assert!((at.0 - 812.5).abs() < 1e-2 && (at.1 - 1203.0).abs() < 1e-2);
}

#[test]
fn a_zoom_keeps_its_anchor_where_it_is_and_the_view_stays_on_the_land() {
    let mut v = MapView::default();
    let anchor = (1500.0, 600.0);
    v.settle(area(), LAND);
    let was = v.on_screen(area(), LAND, anchor);
    v.zoom_about(area(), LAND, 3.0, anchor);
    let now = v.on_screen(area(), LAND, anchor);
    assert!(
        (was.0 - now.0).abs() < 0.1 && (was.1 - now.1).abs() < 0.1,
        "{was:?} {now:?}"
    );
    // At a corner the land's edge holds the view rather than the anchor.
    v.zoom_about(area(), LAND, 1.0, anchor);
    v.zoom_about(area(), LAND, 6.0, (0.0, 0.0));
    let s = v.shown(LAND);
    assert!(s.x >= 0.0 && s.y >= 0.0 && s.x + s.w <= 2040.0 && s.y + s.h <= 2040.0);
    v.zoom_about(area(), LAND, 1e6, (0.0, 0.0));
    assert!(
        (v.zoom - v.most_zoom(area(), LAND)).abs() < 1e-3,
        "held at the most"
    );
}

#[test]
fn a_body_stands_at_its_blocks_corner_and_its_place_in_it_and_nowhere_inside() {
    // Block (0x29, 0x98): cells 328 and 1216 at its corner; 20 meters a cell.
    let at = body_place(0x2998_0021, [80.0, 30.0, 0.0]).expect("outdoors");
    assert!(
        (at.0 - 332.0).abs() < 1e-4 && (at.1 - 1217.5).abs() < 1e-4,
        "{at:?}"
    );
    assert_eq!(body_place(0x2998_0105, [80.0, 30.0, 0.0]), None);
    assert_eq!(
        cell_middle(auberean_primitives::land::cell_at(10, 20, LAND), LAND),
        Some((10.5, 20.5))
    );
}

#[test]
fn the_markers_follow_the_views_each_at_its_cells_middle_with_its_words() {
    let mut m = MapState::default();
    let mut markers = MapMarkers::default();
    let cell = auberean_primitives::land::cell_at(300, 400, LAND);
    markers.add(
        MapMarker {
            kind: 1,
            object: 5,
            cell,
            picture: 0x4100_0009,
            label: Some(TextRef::literal("The mine")),
        },
        LAND,
    );
    let write = |t: &TextRef| t.literal.as_ref().map(|l| String::from_utf16_lossy(l));
    m.follow_markers(&markers, LAND, &write);
    assert_eq!(m.markers.len(), 1);
    assert_eq!(m.markers[0].at, (300.5, 400.5));
    assert_eq!(m.markers[0].words, "The mine");
    assert_eq!(
        m.markers[0].coordinates,
        auberean_horizon::atlas::coordinates(cell, LAND)
    );
}

/// Under the map's coordinates, as the game's own map window: the time of day on a line of its
/// own, then the date on the line under it, each in the middle.
#[test]
fn the_time_of_day_and_then_the_date_are_written_under_the_coordinates() {
    use dereth_ui_kit::art::Family;
    use dereth_ui_kit::paint::TextStyle;
    const TIME: &str = "Sunset";
    const DATE: &str = "Leafcull 21, 664 P.Y.";
    let mut s = with_atlas();
    s.radar.coordinates = Some((-7.4, 46.7));
    s.map.time = Some(TIME.to_owned());
    s.map.date = Some(DATE.to_owned());
    let mut h = Harness::with_built_in_art();
    h.ui.keyed(Keyed::Map, &s);
    h.frame(&s);
    let parts = MapWindow::parts(h.ui.map.body().expect("drawn"), h.k());
    assert!(
        parts.time.y >= parts.place.bottom(),
        "under the coordinates"
    );
    assert!(
        parts.date.y >= parts.time.bottom(),
        "the date under the time"
    );
    // The words drawn on a line: how far across they reach.
    let span = |r: Rect| {
        let (mut left, mut right) = (f32::MAX, f32::MIN);
        for q in h.list.quads.iter().filter(|q| q.tex.is_some()) {
            let (x, y) = centre(q.dst);
            if r.x <= x && x <= r.right() && r.y <= y && y <= r.bottom() && q.dst.h < r.h * 2.0 {
                left = left.min(q.dst.x);
                right = right.max(q.dst.right());
            }
        }
        (left < right).then_some((left, right))
    };
    let wide = |text: &str| {
        crate::measured(&h, |p| {
            let style = TextStyle::new(Family::Body, 13.0, 0xFFFF_FFFF);
            p.measure(&style, text)
        })
    };
    let mut across = Vec::new();
    for (line, text) in [(parts.time, TIME), (parts.date, DATE)] {
        let (left, right) = span(line).unwrap_or_else(|| panic!("{text} written"));
        assert!(
            ((left + right) / 2.0 - (line.x + line.w / 2.0)).abs() < 2.0 * h.k(),
            "{text} in the middle"
        );
        across.push(right - left);
    }
    // Each line as wide as its own words (their edges aside): the time's on the first.
    assert!(
        (across[1] - across[0] - (wide(DATE) - wide(TIME))).abs() < 2.0 * h.k(),
        "{across:?} across, {} and {} measured",
        wide(TIME),
        wide(DATE)
    );
}

/// A player in the world with the map's atlas: the land's size.
pub(crate) fn with_atlas() -> GameState {
    let mut s = in_world();
    s.map.atlas = Some(Arc::new(Atlas {
        world: 0x4110_0000,
        land: LAND,
        continents: Vec::new(),
        labels: Vec::new(),
    }));
    s.map.player = Some(((320.0, 320.0), 90.0));
    s
}

/// A picture of the world 2048 pixels a side, land with a stripe of another color across it, and
/// its overview: as the map window is handed one built.
fn picture() -> (Arc<WorldMap>, Pixels) {
    static BUILT: OnceLock<(Arc<WorldMap>, Pixels)> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            let mut g = MapGrid::new((128, 128), 16, 10.0, [0.0; 256]);
            for x in 0..=2048 {
                for y in 0..=2048 {
                    let c = if (1000..1100).contains(&y) {
                        [200, 60, 60]
                    } else {
                        [80, 120, 60]
                    };
                    g.set((x, y), 0, c, false, false);
                }
            }
            let map = Arc::new(g.prepare());
            let overview = map.overview();
            (map, overview)
        })
        .clone()
}

fn built() -> Built {
    let (map, overview) = picture();
    Built {
        map,
        overview: Some(overview),
    }
}

/// A picture of the world 2048 pixels a side whose land lies in its south middle (landblocks 32
/// to 95 across and 16 to 63 up), with a speck of land far out to its north-west.
fn framed() -> Built {
    static BUILT: OnceLock<(Arc<WorldMap>, Pixels)> = OnceLock::new();
    let (map, overview) = BUILT
        .get_or_init(|| {
            let mut g = MapGrid::new((128, 128), 16, 10.0, [0.0; 256]);
            let mut land = |xs: std::ops::RangeInclusive<usize>,
                            ys: std::ops::RangeInclusive<usize>| {
                for x in xs {
                    for y in ys.clone() {
                        g.set((x, y), 0, [80, 120, 60], false, false);
                    }
                }
            };
            land(513..=1535, 257..=1023);
            land(33..=40, 1921..=1930);
            let map = Arc::new(g.prepare());
            let overview = map.overview();
            (map, overview)
        })
        .clone();
    Built {
        map,
        overview: Some(overview),
    }
}

/// A picture of the world 2048 pixels a side whose land reaches its east edge (landblocks 96 to
/// 127 across and 48 to 79 up).
fn framed_east() -> Built {
    static BUILT: OnceLock<(Arc<WorldMap>, Pixels)> = OnceLock::new();
    let (map, overview) = BUILT
        .get_or_init(|| {
            let mut g = MapGrid::new((128, 128), 16, 10.0, [0.0; 256]);
            for x in 1537..=2048 {
                for y in 769..=1279 {
                    g.set((x, y), 0, [80, 120, 60], false, false);
                }
            }
            let map = Arc::new(g.prepare());
            let overview = map.overview();
            (map, overview)
        })
        .clone();
    Built {
        map,
        overview: Some(overview),
    }
}

/// Land at the world's edge is framed with the same margin as anywhere: the view goes past the
/// edge, and the sea is drawn beyond it.
#[test]
fn land_at_the_worlds_edge_is_framed_past_it_with_the_sea_beyond() {
    let s = with_atlas();
    let mut h = Harness::new();
    h.ui.map.show(framed_east());
    h.ui.keyed(Keyed::Map, &s);
    h.ui.map.follow = false;
    h.frame(&s);
    h.frame(&s);
    // 510 cells each way and a twenty-fifth of that about them: 20.4 cells past the east edge.
    assert!(
        shows(&h.ui.map.view, (1509.6, 744.6, 550.8)),
        "{:?}",
        h.ui.map.view.shown(LAND)
    );
    let area = MapWindow::parts(h.ui.map.body().expect("drawn"), h.k()).map;
    let edge = h.ui.map.view.on_screen(area, LAND, (2040.0, 0.0)).0;
    assert!(edge < area.x + area.w - 5.0, "the edge inside the view");
    assert!(h
        .list
        .quads
        .iter()
        .any(|q| q.tex.is_none() && q.dst == area && q.colour == auberean_horizon::ui::map::sea()));
    let pictures: Vec<_> = h
        .list
        .quads
        .iter()
        .filter(|q| q.clip == Some(area) && q.tex.is_some())
        .collect();
    assert!(!pictures.is_empty());
    for q in pictures {
        assert!(q.dst.x + q.dst.w <= edge + 0.5, "{:?} past {edge}", q.dst);
    }
}

/// What the view of [`framed`]'s world shows fully zoomed out: its land, 1020 cells across and
/// 765 up, with a twenty-fifth of 1020 about it, the speck left out.
const FRAMED: (f32, f32, f32) = (469.2, 86.7, 1101.6);

fn shows(view: &MapView, (x, y, w): (f32, f32, f32)) -> bool {
    let s = view.shown(LAND);
    (s.x - x).abs() < 0.1 && (s.y - y).abs() < 0.1 && (s.w - w).abs() < 0.1 && (s.h - w).abs() < 0.1
}

/// The map opens framing its land, with a margin about it, not the whole world; Fit goes back
/// to it.
#[test]
fn the_map_opens_framing_its_land_with_a_margin_about_it() {
    let s = with_atlas();
    let mut h = Harness::new();
    h.ui.map.show(framed());
    h.ui.keyed(Keyed::Map, &s);
    h.ui.map.follow = false;
    h.frame(&s);
    h.frame(&s);
    assert!((h.ui.map.view.zoom - 1.0).abs() < 1e-6, "fully zoomed out");
    assert!(
        shows(&h.ui.map.view, FRAMED),
        "{:?}",
        h.ui.map.view.shown(LAND)
    );
    let parts = MapWindow::parts(h.ui.map.body().expect("drawn"), h.k());
    h.ui.map
        .view
        .zoom_about(parts.map, LAND, 3.0, (900.0, 600.0));
    assert!(!shows(&h.ui.map.view, FRAMED));
    h.click(&s, centre(parts.whole));
    assert!(
        shows(&h.ui.map.view, FRAMED),
        "{:?}",
        h.ui.map.view.shown(LAND)
    );
}

/// The wheel zooms out no further than the land framed.
#[test]
fn the_wheel_out_stops_at_the_land_framed() {
    let s = with_atlas();
    let mut h = Harness::new();
    h.ui.map.show(framed());
    h.ui.keyed(Keyed::Map, &s);
    h.ui.map.follow = false;
    h.frame(&s);
    let parts = MapWindow::parts(h.ui.map.body().expect("drawn"), h.k());
    h.input.mouse = (
        parts.map.x + parts.map.w * 0.3,
        parts.map.y + parts.map.h * 0.6,
    );
    h.input.wheel = 3.0;
    h.frame(&s);
    assert!(h.ui.map.view.zoom > 1.5);
    h.input.wheel = -20.0;
    h.frame(&s);
    assert!((h.ui.map.view.zoom - 1.0).abs() < 1e-6);
    assert!(
        shows(&h.ui.map.view, FRAMED),
        "{:?}",
        h.ui.map.view.shown(LAND)
    );
}

/// Over the land framed, the wheel still zooms about the pointer and a drag pans, the view held
/// inside the frame.
#[test]
fn over_the_land_framed_the_wheel_zooms_about_the_pointer_and_a_drag_pans() {
    let s = with_atlas();
    let mut h = Harness::new();
    h.ui.map.show(framed());
    h.ui.keyed(Keyed::Map, &s);
    h.ui.map.follow = false;
    h.frame(&s);
    let parts = MapWindow::parts(h.ui.map.body().expect("drawn"), h.k());
    let pointer = (
        parts.map.x + parts.map.w * 0.25,
        parts.map.y + parts.map.h * 0.75,
    );
    let under = h.ui.map.view.on_land(parts.map, LAND, pointer);
    h.input.mouse = pointer;
    h.input.wheel = 2.0;
    h.frame(&s);
    assert!((h.ui.map.view.zoom - 1.5625).abs() < 1e-3, "two notches");
    let now = h.ui.map.view.on_land(parts.map, LAND, pointer);
    assert!(
        (now.0 - under.0).abs() < 0.5 && (now.1 - under.1).abs() < 0.5,
        "{under:?} {now:?}"
    );
    let before = h.ui.map.view.centre;
    h.drag(&s, pointer, (pointer.0 + 50.0, pointer.1 - 40.0));
    let after = h.ui.map.view.centre;
    assert!(after.0 < before.0, "dragged east: the view goes west");
    assert!(after.1 < before.1, "dragged up: the view goes south");
    // Dragged far, the view stops at the frame's edge.
    h.drag(&s, pointer, (pointer.0 + 300.0, pointer.1 - 300.0));
    let (x, y, _) = FRAMED;
    let shown = h.ui.map.view.shown(LAND);
    assert!(
        (shown.x - x).abs() < 0.1 && (shown.y - y).abs() < 0.1,
        "{shown:?}"
    );
}

/// The window open on the map key.
fn opened(s: &GameState) -> Harness {
    let mut h = Harness::new();
    assert_eq!(keyed(action::MAP), Some(Keyed::Map));
    h.ui.keyed(Keyed::Map, s);
    h.frame(s);
    assert!(h.ui.map.is_open());
    h
}

/// The textures of the quads drawn on the map's picture.
fn textures_on(h: &Harness, area: Rect) -> Vec<u32> {
    let mut v: Vec<u32> = h
        .list
        .quads
        .iter()
        .filter(|q| q.clip == Some(area))
        .filter_map(|q| q.tex)
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// The map opens on the whole of a world that is all land, framed with the margin past its edges,
/// drawn from the picture of the whole: no other picture (none of the game's) lies on it, and the
/// picture fills the world's part of it.
#[test]
fn the_map_opens_on_the_whole_world_drawn_from_its_picture_of_the_whole() {
    let s = with_atlas();
    let mut h = Harness::new();
    h.ui.map.show(built());
    h.ui.keyed(Keyed::Map, &s);
    h.frame(&s);
    h.frame(&s);
    let area = MapWindow::parts(h.ui.map.body().expect("drawn"), h.k()).map;
    assert!((h.ui.map.view.zoom - 1.0).abs() < 1e-6, "fully zoomed out");
    let shown = h.ui.map.view.shown(LAND);
    // A twenty-fifth of 2040 past each edge.
    assert!(
        (shown.x + 81.6).abs() < 1e-2
            && (shown.y + 81.6).abs() < 1e-2
            && (shown.w - 2203.2).abs() < 1e-2,
        "{shown:?}"
    );
    let world = {
        let (x, y) = h.ui.map.view.on_screen(area, LAND, (0.0, 2040.0));
        let side = area.w * 2040.0 / 2203.2;
        Rect::new(x, y, side, side)
    };
    let whole =
        h.ui.art()
            .lookup(OVERVIEW_KEY)
            .flatten()
            .expect("the whole kept");
    assert_eq!(
        textures_on(&h, area),
        vec![whole.tex],
        "the picture of the whole alone"
    );
    let drawn = h
        .list
        .quads
        .iter()
        .find(|q| q.tex == Some(whole.tex))
        .expect("drawn");
    assert!(
        (drawn.dst.x - world.x).abs() < 0.5
            && (drawn.dst.y - world.y).abs() < 0.5
            && (drawn.dst.w - world.w).abs() < 0.5
            && (drawn.dst.h - world.h).abs() < 0.5,
        "{:?} over {world:?}",
        drawn.dst
    );
    assert!(h.ui.map.kept().is_empty(), "no tiles zoomed out");
}

/// Zoomed in past what the picture of the whole holds, the tiles under the view are drawn, one a
/// frame, over the picture of the whole until each is; closing the map lets them go.
#[test]
fn zoomed_in_the_tiles_under_the_view_are_drawn_one_a_frame_and_let_go_on_closing() {
    let s = with_atlas();
    let mut h = Harness::new();
    h.ui.map.show(built());
    h.ui.keyed(Keyed::Map, &s);
    h.ui.map.follow = false;
    h.frame(&s);
    let area = MapWindow::parts(h.ui.map.body().expect("drawn"), h.k()).map;
    // On the corner of the picture's four tiles.
    h.ui.map.view.zoom_about(area, LAND, 8.0, (1020.0, 1020.0));
    h.ui.map.view.centre = (1020.0, 1020.0);
    let part = on_picture(h.ui.map.view.shown(LAND), LAND, (2048, 2048));
    let under = tiles_under(part, (2048, 2048));
    assert_eq!(under.len(), 4);
    let mut frames = 0;
    while h.ui.map.kept().len() < 4 {
        h.frame(&s);
        frames += 1;
        assert!(frames <= 4, "a tile a frame");
    }
    assert_eq!(frames, 4);
    let tiles: Vec<u32> = under
        .iter()
        .map(|((c, r), _)| {
            h.ui.art()
                .lookup(&tile_key(*c, *r))
                .flatten()
                .expect("kept")
                .tex
        })
        .collect();
    h.frame(&s);
    let mut want = tiles.clone();
    want.sort_unstable();
    assert_eq!(
        textures_on(&h, area),
        want,
        "the tiles alone once each is drawn"
    );
    h.ui.keyed(Keyed::Map, &s);
    assert!(!h.ui.map.is_open());
    assert!(h.ui.map.kept().is_empty(), "let go of as it closes");
    h.frame(&s);
    assert!(
        h.ui.art().lookup(&tile_key(0, 0)).is_none(),
        "and gone from the art"
    );
    assert!(h.ui.art().lookup(OVERVIEW_KEY).is_some(), "the whole kept");
}

/// Before its picture is drawn the map shows the sea.
#[test]
fn before_its_picture_is_drawn_the_map_shows_the_sea() {
    let s = with_atlas();
    let h = opened(&s);
    let area = MapWindow::parts(h.ui.map.body().expect("drawn"), h.k()).map;
    assert!(textures_on(&h, area).is_empty());
    assert!(h
        .list
        .quads
        .iter()
        .any(|q| q.tex.is_none() && q.dst == area && q.colour == auberean_horizon::ui::map::sea()));
}

#[test]
fn the_wheel_zooms_about_the_pointer_and_a_drag_pans() {
    let s = with_atlas();
    let mut h = opened(&s);
    // The view moved by hand, not kept on the player.
    h.ui.map.follow = false;
    let parts = MapWindow::parts(h.ui.map.body().expect("drawn"), h.k());
    let pointer = (
        parts.map.x + parts.map.w * 0.25,
        parts.map.y + parts.map.h * 0.75,
    );
    let under = h.ui.map.view.on_land(parts.map, LAND, pointer);
    h.input.mouse = pointer;
    h.input.wheel = 2.0;
    h.frame(&s);
    assert!(h.ui.map.view.zoom > 1.5);
    let now = h.ui.map.view.on_land(parts.map, LAND, pointer);
    assert!(
        (now.0 - under.0).abs() < 0.5 && (now.1 - under.1).abs() < 0.5,
        "{under:?} {now:?}"
    );
    let before = h.ui.map.view.centre;
    h.drag(&s, pointer, (pointer.0 + 50.0, pointer.1 - 40.0));
    let after = h.ui.map.view.centre;
    assert!(after.0 < before.0, "dragged east: the view goes west");
    assert!(after.1 < before.1, "dragged up: the view goes south");
}

#[test]
fn zoomed_in_a_marker_is_drawn_and_a_click_on_it_writes_its_words() {
    let mut s = with_atlas();
    s.map.markers = vec![Marker {
        kind: 1,
        object: 5,
        at: (330.0, 330.0),
        picture: 0,
        words: "Dig here".into(),
        coordinates: "1.0N, 2.0E".into(),
    }];
    let mut h = opened(&s);
    // The view moved by hand, not kept on the player.
    h.ui.map.follow = false;
    let parts = MapWindow::parts(h.ui.map.body().expect("drawn"), h.k());
    let view = &mut h.ui.map.view;
    let zoom = MARKS_FROM * 2040.0 / parts.map.w * 1.5;
    view.zoom_about(parts.map, LAND, zoom, (330.0, 330.0));
    let at = h.ui.map.view.on_screen(parts.map, LAND, (330.0, 330.0));
    h.frame(&s);
    let drawn = h
        .list
        .quads
        .iter()
        .any(|q| q.tex.is_none() && q.colour == auberean_horizon::ui::map::MARKER);
    assert!(drawn, "the marker's plain mark");
    h.click(&s, at);
    assert_eq!(h.ui.map.note.as_deref(), Some("Dig here"));
}

#[test]
fn the_fellows_but_the_player_are_markers_at_their_cells_unless_kept_off_the_map() {
    use auberean_client_sdk::contract::notice::{Fellow, Fellowship};
    use auberean_client_sdk::contract::view::{self as v, View};
    let cell = |x, y| auberean_primitives::land::cell_at(x, y, LAND);
    let fellow = |name: &str, at: u32| Fellow {
        name: TextRef::literal(name),
        cell: at,
        ..Fellow::default()
    };
    let mut view = View {
        player_id: Some(crate::PLAYER),
        options: Some(v::GameOptions::default()),
        ..View::default()
    };
    view.social.fellowship = Some(Fellowship {
        fellows: vec![
            (crate::PLAYER, fellow("Me", cell(100, 100))),
            (7, fellow("Cara", cell(200, 300))),
            (8, fellow("Dov", cell(201, 300))),
            (9, fellow("Ell", cell(500, 500))),
        ],
        ..Fellowship::default()
    });
    let write = |t: &TextRef| t.literal.as_ref().map(|l| String::from_utf16_lossy(l));
    let shown = fellow_markers(&view, LAND, 0x4100_00AA, &write);
    let who: Vec<(u64, &str)> = shown.iter().map(|m| (m.object, m.words.as_str())).collect();
    assert_eq!(
        who,
        vec![(7, "Cara"), (9, "Ell")],
        "not the player; one standing by another of the fellows is not put, as the map's rules go"
    );
    assert!(shown
        .iter()
        .all(|m| m.kind == FELLOW_MARKERS && m.picture == 0x4100_00AA));
    assert_eq!(shown[0].at, (200.5, 300.5));
    if let Some(o) = &mut view.options {
        o.bits |= FELLOWS_OFF_MAP;
    }
    assert!(
        fellow_markers(&view, LAND, 0, &write).is_empty(),
        "kept off the map"
    );
}

/// The window is sized about its picture, as Dereth's Horizon sizes its map: the picture fills
/// the body's width but for a small margin, and the buttons and lines under it end at the body's
/// foot; the window is smaller than the screen it is drawn on with room to spare.
#[test]
fn the_map_fills_its_window_and_the_window_leaves_the_screen_room() {
    let s = with_atlas();
    let h = opened(&s);
    let k = h.k();
    let body = h.ui.map.body().expect("drawn");
    let parts = MapWindow::parts(body, k);
    assert!(
        body.w - parts.map.w <= 16.0 * k + 0.5,
        "{} of {} across",
        parts.map.w,
        body.w
    );
    let foot = parts.note.bottom().max(parts.follow.bottom());
    assert!(foot <= body.bottom(), "the lines inside");
    assert!(
        body.bottom() - foot <= 8.0 * k + 0.5,
        "no room left under them"
    );
    let window = h.ui.map.rect().expect("drawn");
    assert!(
        window.h <= h.screen.1 * 0.6,
        "{} of the screen's {}",
        window.h,
        h.screen.1
    );
}

/// Follow keeps the player in the middle of the view as it moves, until a drag pans away.
#[test]
fn follow_keeps_the_player_in_the_middle_until_a_drag_pans_away() {
    let mut s = with_atlas();
    let mut h = opened(&s);
    let k = h.k();
    let parts = MapWindow::parts(h.ui.map.body().expect("drawn"), k);
    h.ui.map
        .view
        .zoom_about(parts.map, LAND, 8.0, (1500.0, 1500.0));
    assert!(h.ui.map.follow, "on at first");
    let follow_box = (
        parts.follow.x + 10.0 * k,
        parts.follow.y + parts.follow.h / 2.0,
    );
    h.click(&s, follow_box);
    assert!(!h.ui.map.follow, "turned off");
    h.click(&s, follow_box);
    assert!(h.ui.map.follow);
    let near = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5;
    assert!(
        near(h.ui.map.view.centre, (320.0, 320.0)),
        "{:?}",
        h.ui.map.view.centre
    );
    s.map.player = Some(((700.0, 900.0), 0.0));
    h.frame(&s);
    assert!(near(h.ui.map.view.centre, (700.0, 900.0)), "followed");
    let middle = centre(parts.map);
    h.drag(&s, middle, (middle.0 + 60.0, middle.1));
    assert!(!h.ui.map.follow, "panned away");
    let panned = h.ui.map.view.centre;
    s.map.player = Some(((720.0, 900.0), 0.0));
    h.frame(&s);
    assert_eq!(h.ui.map.view.centre, panned, "no longer followed");
}

/// Follow turned off on the map is kept in Horizon's own settings, and the map opens following
/// the player again only once it is turned back on.
#[test]
fn follow_turned_off_is_kept_and_the_map_opens_without_it() {
    let s = with_atlas();
    let mut h = opened(&s);
    let k = h.k();
    let parts = MapWindow::parts(h.ui.map.body().expect("drawn"), k);
    let follow_box = (
        parts.follow.x + 10.0 * k,
        parts.follow.y + parts.follow.h / 2.0,
    );
    h.click(&s, follow_box);
    assert!(!h.ui.map.follow);
    assert!(!h.ui.settings.own.map_follow, "kept off");
    assert!(h.ui.settings.take_own_changed(), "written");
    assert!(h.ui.settings.own.write().contains("map-follow off\n"));
    h.ui.keyed(Keyed::Map, &s);
    assert!(!h.ui.map.is_open());
    h.ui.keyed(Keyed::Map, &s);
    assert!(!h.ui.map.follow, "opens without following");
    h.frame(&s);
    h.click(&s, follow_box);
    assert!(h.ui.settings.own.map_follow, "back on");
    h.ui.keyed(Keyed::Map, &s);
    h.ui.keyed(Keyed::Map, &s);
    assert!(h.ui.map.follow, "opens following");
    // Panning away stops following for the view, not in the settings.
    h.frame(&s);
    let middle = centre(parts.map);
    h.drag(&s, middle, (middle.0 + 60.0, middle.1));
    assert!(!h.ui.map.follow, "panned away");
    assert!(h.ui.settings.own.map_follow, "still kept on");
}

/// The player's arrow never reaches past the picture: out of the view it is not drawn, and at the
/// picture's edge, where the arrow turned would stick out, the plain mark is drawn cut at the edge.
#[test]
fn the_players_mark_stays_inside_the_picture() {
    // Facing a way of its own, so the map's arrow is told from the minimap's.
    const FACING: f32 = 37.0;
    let mut s = with_atlas();
    s.map.player = Some(((320.0, 320.0), FACING));
    let mut h = Harness::with_built_in_art();
    h.ui.keyed(Keyed::Map, &s);
    // The view moved by hand, not kept on the player.
    h.ui.map.follow = false;
    h.frame(&s);
    let k = h.k();
    let area = MapWindow::parts(h.ui.map.body().expect("drawn"), k).map;
    let turned = |h: &Harness| -> Vec<Rect> {
        h.list
            .quads
            .iter()
            .filter(|q| (q.turn - FACING.to_radians()).abs() < 1e-4)
            .map(|q| q.dst)
            .collect()
    };
    // In the middle: the arrow, turned the way the player faces.
    h.frame(&s);
    assert_eq!(turned(&h).len(), 1, "the arrow");
    // Zoomed in far from the player: nothing of it.
    h.ui.map.view.zoom_about(area, LAND, 8.0, (1500.0, 1500.0));
    h.frame(&s);
    assert!(turned(&h).is_empty(), "out of view");
    // At the picture's west edge: no turned arrow reaches past it, and the plain mark is cut.
    let shown = h.ui.map.view.shown(LAND);
    let per = area.w / shown.w;
    s.map.player = Some(((shown.x + 2.0 / per, shown.y + shown.h / 2.0), FACING));
    h.frame(&s);
    for r in turned(&h) {
        let reach = (r.w * r.w + r.h * r.h).sqrt() / 2.0;
        let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        assert!(
            cx - reach >= area.x && cy - reach >= area.y,
            "the arrow at {cx}, {cy} reaches past {area:?}"
        );
    }
    let mark = h
        .list
        .quads
        .iter()
        .find(|q| q.colour == auberean_horizon::ui::map::PLAYER_MARK)
        .expect("the plain mark");
    assert_eq!(mark.clip, Some(area), "cut at the picture's edge");
}

/// Fully zoomed out the map names the continents and nothing else: each name written about where
/// the game's map writes it, and no label or marker; from the zoom the labels and markers come in
/// at, they are drawn and the continents' names are not.
#[test]
fn zoomed_out_only_the_continents_are_named_and_from_the_threshold_the_labels_and_markers_come() {
    let mut s = in_world();
    let continents = vec![
        Continent {
            name: "Osteth".into(),
            at: (1200.0, 1800.0),
        },
        Continent {
            name: "Knorr".into(),
            at: (1700.0, 300.0),
        },
        Continent {
            name: "Linvak Massif".into(),
            at: (360.0, 320.0),
        },
    ];
    s.map.atlas = Some(Arc::new(Atlas {
        world: 0x4110_0000,
        land: LAND,
        continents: continents.clone(),
        labels: vec![Label {
            at: (330.5, 330.5),
            icon: 0,
            words: Some("Cragstone".into()),
            coordinates: "1.0N, 2.0E".into(),
            text: (0, 0),
        }],
    }));
    s.map.markers = vec![Marker {
        kind: 1,
        object: 5,
        at: (340.0, 340.0),
        picture: 0,
        words: "Dig here".into(),
        coordinates: "1.0N, 2.0E".into(),
    }];
    s.map.player = None;
    let mut h = Harness::with_built_in_art();
    h.ui.keyed(Keyed::Map, &s);
    h.ui.map.follow = false;
    h.frame(&s);
    let k = h.k();
    let area = MapWindow::parts(h.ui.map.body().expect("drawn"), k).map;
    let words = |h: &Harness| -> Vec<Rect> {
        h.list
            .quads
            .iter()
            .filter(|q| q.clip == Some(area) && q.tex.is_some())
            .map(|q| q.dst)
            .collect()
    };
    let marks = |h: &Harness, colour: u32| {
        h.list
            .quads
            .iter()
            .any(|q| q.tex.is_none() && q.colour == colour)
    };
    let drawn = words(&h);
    for c in &continents {
        let (x, y) = h.ui.map.view.on_screen(area, LAND, c.at);
        let near = drawn
            .iter()
            .filter(|r| {
                (centre(**r).0 - x).abs() < 60.0 * k && (centre(**r).1 - y).abs() < 12.0 * k
            })
            .count();
        assert!(
            near >= 3,
            "{} written about its place: {near} glyphs",
            c.name
        );
    }
    assert!(
        !marks(&h, auberean_horizon::ui::map::LABEL),
        "no label zoomed out"
    );
    assert!(
        !marks(&h, auberean_horizon::ui::map::MARKER),
        "no marker zoomed out"
    );
    let zoom = MARKS_FROM * 2040.0 / area.w * 1.5;
    h.ui.map.view.zoom_about(area, LAND, zoom, (335.0, 335.0));
    h.frame(&s);
    assert!(marks(&h, auberean_horizon::ui::map::LABEL), "the label");
    assert!(marks(&h, auberean_horizon::ui::map::MARKER), "the marker");
    assert!(
        words(&h).is_empty(),
        "no continent named zoomed in: {:?}",
        words(&h)
    );
}

/// The continents' names go onto the land: each to the middle of one of the largest stretches of
/// land, as many as there are names, matched so the names move least; with fewer stretches than
/// names, where the game's map writes them.
#[test]
fn each_continent_is_named_in_the_middle_of_its_own_stretch_of_land() {
    use auberean_horizon::atlas::on_the_land;
    use auberean_horizon::world_map::Landmass;
    let named = |name: &str, at: (f32, f32)| Continent {
        name: name.into(),
        at,
    };
    // Written beside the land: north of the one, south-east of the other.
    let continents = vec![
        named("North", (1000.0, 1900.0)),
        named("South", (1900.0, 100.0)),
    ];
    let mass = |blocks, centre| Landmass { blocks, centre };
    let masses = vec![
        mass(400, (0.5, 0.75)),
        mass(300, (0.75, 0.25)),
        mass(10, (0.52, 0.95)),
    ];
    let placed = on_the_land(&continents, &masses, LAND);
    assert_eq!(
        placed[0],
        named("North", (1020.0, 1530.0)),
        "the largest, not the islet nearer"
    );
    assert_eq!(placed[1], named("South", (1530.0, 510.0)));
    assert_eq!(
        on_the_land(&continents, &masses[..1], LAND),
        continents,
        "too few stretches: as the game writes them"
    );
}
