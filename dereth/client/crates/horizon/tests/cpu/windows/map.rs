//! The map window: its picture zooms and pans for everyone, and what it says of a place follows.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use std::sync::{Arc, OnceLock};

use dereth_horizon::art::Art;
use dereth_horizon::draw::{DrawList, Rect};
use dereth_horizon::options::HorizonOptions;
use dereth_horizon::ui::game::GameState;
use dereth_horizon::ui::input::InputFrame;
use dereth_horizon::ui::panels::map::{map_pixel, tile_key, PlaceKind, OVERVIEW_KEY};
use dereth_horizon::ui::panels::WindowId;
use dereth_horizon::ui::HorizonUi;
use dereth_scene::world_map::{Built, MapGrid, Pixels, WorldMap};

use crate::Harness;

fn player() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        ..GameState::default()
    }
}

/// The map window open over the art behind it, and where its picture is.
fn opened(art: &Arc<Art>, state: &GameState) -> (Harness, Rect) {
    let mut h = Harness {
        ui: HorizonUi::new(Arc::clone(art), HorizonOptions::default()),
        list: DrawList::default(),
        input: InputFrame::default(),
        screen: (1920.0, 1080.0),
    };
    h.frame(state);
    h.ui.windows.open(WindowId::Map, 0.0);
    h.frame(state);
    let area = h.ui.windows.map_picture.expect("the map's picture");
    (h, area)
}

fn built_in() -> Arc<Art> {
    let pieces = dereth_horizon::pieces::Pieces::built_in().expect("the pieces built in");
    Arc::new(Art::new(Arc::new(pieces)))
}

/// A world of one island about Holtburg, drawn once for every test.
fn drawn() -> Built {
    static DRAWN: OnceLock<(Arc<WorldMap>, Pixels)> = OnceLock::new();
    let (map, overview) = DRAWN.get_or_init(|| {
        let mut heights = [0.0; 256];
        for (i, h) in heights.iter_mut().enumerate() {
            *h = f32::from(u8::try_from(i).unwrap_or(0)) * 2.0;
        }
        let names = ["WaterDeepSea", "Grassland", "Snow"].map(str::to_owned);
        let mut grid = MapGrid::new(names.to_vec(), heights);
        for x in 1300..1400 {
            for y in 1400..1500 {
                let height = u8::try_from((x + y) % 50).unwrap_or(0);
                grid.set((x, y), if x < 1350 { 1 } else { 2 }, height, y == 1450);
            }
        }
        let map = grid.prepare();
        let overview = map.overview();
        (Arc::new(map), overview)
    });
    Built {
        map: Arc::clone(map),
        overview: Some(overview.clone()),
    }
}

fn middle(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

fn wheel(h: &mut Harness, state: &GameState, at: (f32, f32), notches: f32) {
    h.move_to(at.0, at.1);
    h.input.wheel = notches;
    h.frame(state);
}

fn click(h: &mut Harness, state: &GameState, at: (f32, f32)) {
    h.move_to(at.0, at.1);
    h.frame(state);
    h.press();
    h.frame(state);
    h.release();
    h.frame(state);
}

#[test]
fn a_player_s_wheel_zooms_about_the_pointer_and_the_buttons_step_and_fit() {
    let (state, art) = (player(), Arc::new(Art::empty()));
    let (mut h, area) = opened(&art, &state);
    let at = (area.x + area.w * 0.7, area.y + area.h * 0.3);
    let before = h.ui.windows.map_view.on_picture(area, at);
    wheel(&mut h, &state, at, 4.0);
    let view = &h.ui.windows.map_view;
    assert!(view.zoom > 2.0, "{}", view.zoom);
    let after = view.on_picture(area, at);
    assert!(
        (before.0 - after.0).abs() < 1.0 && (before.1 - after.1).abs() < 1.0,
        "{before:?} {after:?}"
    );
    assert_eq!(
        h.input.wheel, 0.0,
        "the wheel is the map's, not the world's"
    );

    let zoom = h.ui.windows.map_view.zoom;
    let [out, closer, fit] = h.ui.windows.map_view.buttons.expect("the buttons");
    click(&mut h, &state, middle(closer));
    assert!(h.ui.windows.map_view.zoom > zoom);
    click(&mut h, &state, middle(out));
    assert!((h.ui.windows.map_view.zoom - zoom).abs() < 0.01);
    click(&mut h, &state, middle(fit));
    assert_eq!(h.ui.windows.map_view.zoom, 1.0);
}

#[test]
fn a_drag_pans_a_zoomed_map_and_the_whole_picture_stays_put() {
    let (state, art) = (player(), Arc::new(Art::empty()));
    let (mut h, area) = opened(&art, &state);
    let at = middle(area);
    let drag = |h: &mut Harness| {
        h.move_to(at.0, at.1);
        h.frame(&state);
        h.press();
        h.frame(&state);
        h.move_to(at.0 - 40.0, at.1 - 20.0);
        h.frame(&state);
        h.release();
        h.frame(&state);
    };
    let whole = h.ui.windows.map_view.centre;
    drag(&mut h);
    assert_eq!(h.ui.windows.map_view.centre, whole, "nowhere to pan to");

    wheel(&mut h, &state, at, 6.0);
    let was = h.ui.windows.map_view.centre;
    drag(&mut h);
    let view = &h.ui.windows.map_view;
    let k = view.shown().w / area.w;
    assert!((view.centre.0 - (was.0 + 40.0 * k)).abs() < 0.5);
    assert!((view.centre.1 - (was.1 + 20.0 * k)).abs() < 0.5);
}

/// The quads drawn from picture `key` this frame.
fn drawn_from(h: &Harness, art: &Art, key: &str) -> usize {
    let Some(Some(sprite)) = art.lookup(key) else {
        return 0;
    };
    h.list
        .quads
        .iter()
        .filter(|q| q.tex == Some(sprite.tex))
        .count()
}

#[test]
fn the_map_is_the_sea_until_its_picture_is_drawn_and_then_the_whole() {
    let (state, art) = (player(), Arc::new(Art::empty()));
    let (mut h, area) = opened(&art, &state);
    let sea = h
        .list
        .quads
        .iter()
        .find(|q| q.tex.is_none() && q.dst == area)
        .expect("the plain ground");
    assert_eq!(sea.colour, 0xFF70_859F, "the sea's colour");
    assert!(art.lookup(OVERVIEW_KEY).is_none(), "no data, no picture");

    h.ui.windows.map_view.show(drawn());
    h.frame(&state);
    assert_eq!(drawn_from(&h, &art, OVERVIEW_KEY), 1, "the whole");
    let picture = art
        .lookup(OVERVIEW_KEY)
        .flatten()
        .and_then(|s| art.image(s.tex))
        .expect("its pixels");
    assert_eq!((picture.width, picture.height), (2048, 2048));
}

#[test]
fn tiles_are_drawn_for_a_close_view_one_a_frame_and_let_go_when_the_map_closes() {
    let (state, art) = (player(), Arc::new(Art::empty()));
    let (mut h, _) = opened(&art, &state);
    h.ui.windows.map_view.show(drawn());
    h.frame(&state);
    assert!(art.lookup(OVERVIEW_KEY).is_some(), "the whole, at the fit");
    let kept = |art: &Art| {
        (0..6)
            .flat_map(|c| (0..6).map(move |r| tile_key(c, r)))
            .filter(|t| art.lookup(t).is_some())
            .count()
    };
    assert_eq!(kept(&art), 0, "no tile yet");

    let holtburg = map_pixel(42.2, 33.8);
    h.ui.windows.map_view.zoom = 12.0;
    h.ui.windows.map_view.centre = holtburg;
    h.frame(&state);
    assert_eq!(kept(&art), 1, "one a frame");
    for _ in 0..4 {
        h.frame(&state);
    }
    assert_eq!(
        drawn_from(&h, &art, &tile_key(3, 1)),
        1,
        "the tile under Holtburg"
    );

    h.ui.windows.close(WindowId::Map);
    h.frame(&state);
    assert_eq!(kept(&art), 0);
    assert!(!h.ui.windows.map_view.is_ready(), "the world let go");
    assert!(art.lookup(OVERVIEW_KEY).is_some(), "and the whole kept");
}

/// A player's zoomed map shows the major towns and the houses, and not the portals.
#[test]
fn a_player_sees_the_towns_and_houses() {
    let art = built_in();
    let state = player();
    let (mut h, _) = opened(&art, &state);
    h.ui.windows.map_view.zoom = 12.0;
    h.ui.windows.map_view.centre = map_pixel(42.2, 33.8);
    h.frame(&state);
    let town = art.lookup("map.town").flatten().expect("the town icon").tex;
    let icons = |h: &Harness| h.list.quads.iter().filter(|q| q.tex == Some(town)).count();
    let marks = |h: &Harness, kind: PlaceKind| {
        h.list
            .quads
            .iter()
            .filter(|q| q.tex.is_none() && q.colour & 0x00FF_FFFF == kind.colour() & 0x00FF_FFFF)
            .count()
    };
    assert_eq!(icons(&h), 1, "Holtburg's");
    assert!(
        marks(&h, PlaceKind::Cottage) > 0,
        "the cottages about Holtburg"
    );
    assert_eq!(marks(&h, PlaceKind::Portal), 0);
}
