//! The map window: its picture zooms and pans for everyone, and what it says of a place follows.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use std::sync::{Arc, OnceLock};

use dereth_horizon::art::Art;
use dereth_horizon::draw::{DrawList, Rect};
use dereth_horizon::options::HorizonOptions;
use dereth_horizon::ui::game::GameState;
use dereth_horizon::ui::input::InputFrame;
use dereth_horizon::ui::panels::map::{colour, map_pixel, tile_key, OVERVIEW_KEY};
use dereth_horizon::ui::panels::WindowId;
use dereth_horizon::ui::HorizonUi;
use dereth_scene::world_map::{Built, MapGrid, Pixels, Place, PlaceKind, WorldMap};

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

/// A world of one island about Holtburg, and with `corners` a rock near each of two opposite
/// corners of the world, so the map frames all of it.
fn world(corners: bool) -> WorldMap {
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
    let rocks: &[usize] = if corners { &[20, 2017] } else { &[] };
    for &at in rocks {
        for x in at..at + 3 {
            for y in at..at + 3 {
                grid.set((x, y), 2, u8::try_from(x + y).unwrap_or(0), false);
            }
        }
    }
    grid.prepare()
}

/// The world of [`world`] with its rocks, drawn once for every test, with Holtburg, two cottages
/// and a villa on its island.
fn drawn() -> Built {
    static DRAWN: OnceLock<(Arc<WorldMap>, Pixels)> = OnceLock::new();
    let (map, overview) = DRAWN.get_or_init(|| {
        let map = world(true);
        let overview = map.overview();
        (Arc::new(map), overview)
    });
    let place = |kind, name: &str, ns, ew| Place {
        kind,
        name: name.to_owned(),
        ns,
        ew,
    };
    Built {
        map: Arc::clone(map),
        overview: Some(overview.clone()),
        places: Arc::new(vec![
            place(PlaceKind::Town, "Holtburg", 42.2, 33.8),
            place(PlaceKind::Cottage, "Cottage", 42.0, 33.6),
            place(PlaceKind::Cottage, "Cottage", 42.4, 34.0),
            place(PlaceKind::Villa, "Villa", 41.9, 34.1),
        ]),
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

/// A world whose land is one island is framed to it: fully zoomed out the map shows the island
/// with a margin about it, zooms out no further and pans no further; Holtburg's town is marked on
/// it. A world whose land reaches across it is framed by the whole picture.
#[test]
fn a_world_s_map_is_framed_to_its_land() {
    let (state, art) = (player(), Arc::new(Art::empty()));
    let (mut h, area) = opened(&art, &state);
    let whole = Rect::new(0.0, 0.0, 6123.0, 6123.0);
    assert_eq!(
        h.ui.windows.map_view.shown(),
        whole,
        "the whole until it is drawn"
    );
    let map = world(false);
    let frame = map.frame();
    assert!(frame.side < 400.0, "the island's: {frame:?}");
    h.ui.windows.map_view.show(Built {
        overview: Some(map.overview()),
        map: Arc::new(map),
        places: drawn().places,
    });
    h.frame(&state);
    #[allow(clippy::cast_possible_truncation)]
    let want = Rect::new(
        frame.x as f32,
        frame.y as f32,
        frame.side as f32,
        frame.side as f32,
    );
    assert_eq!(h.ui.windows.map_view.shown(), want);
    let holtburg = map_pixel(42.2, 33.8);
    assert!(
        want.contains(holtburg.0, holtburg.1),
        "Holtburg's on the island"
    );
    // No further out, and nowhere to pan to.
    wheel(
        &mut h,
        &state,
        (area.x + area.w / 2.0, area.y + area.h / 2.0),
        -4.0,
    );
    assert_eq!(h.ui.windows.map_view.shown(), want);
    let icons = art.lookup("map.town").flatten().map_or(0, |t| {
        h.list.quads.iter().filter(|q| q.tex == Some(t.tex)).count()
    });
    assert_eq!(icons, 1, "Holtburg's town");

    h.ui.windows.map_view.show(drawn());
    h.frame(&state);
    assert_eq!(
        h.ui.windows.map_view.shown(),
        whole,
        "land across the world: the whole"
    );
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

/// The map names nothing until the data's places are read; then it shows the towns, and the
/// houses once it is zoomed in far enough to tell them apart.
#[test]
fn a_player_sees_the_towns_and_then_the_houses_as_the_map_zooms_in() {
    let (state, art) = (player(), Arc::new(Art::empty()));
    let (mut h, _) = opened(&art, &state);
    let icons = |h: &Harness| {
        art.lookup("map.town").flatten().map_or(0, |t| {
            h.list.quads.iter().filter(|q| q.tex == Some(t.tex)).count()
        })
    };
    let marks = |h: &Harness, kind: PlaceKind| {
        h.list
            .quads
            .iter()
            .filter(|q| q.tex.is_none() && q.colour & 0x00FF_FFFF == colour(kind) & 0x00FF_FFFF)
            .count()
    };
    assert_eq!(icons(&h), 0, "no data, no places");

    h.ui.windows.map_view.show(drawn());
    h.frame(&state);
    assert_eq!(icons(&h), 1, "Holtburg's");
    assert_eq!(marks(&h, PlaceKind::Cottage), 0, "too far out for houses");

    h.ui.windows.map_view.zoom = 12.0;
    h.ui.windows.map_view.centre = map_pixel(42.2, 33.8);
    h.frame(&state);
    assert_eq!(icons(&h), 1, "Holtburg's");
    assert_eq!(
        marks(&h, PlaceKind::Cottage),
        2,
        "the cottages about Holtburg"
    );
    assert_eq!(marks(&h, PlaceKind::Villa), 1);
}

/// The whole map is towns alone; closing in marks the landmarks and dungeons, then the portals
/// and lifestones, then the people.
#[test]
fn the_whole_map_shows_only_towns_and_more_kinds_of_place_as_it_zooms_in() {
    let (state, art) = (player(), Arc::new(Art::empty()));
    let (mut h, _) = opened(&art, &state);
    let marks = |h: &Harness, kind: PlaceKind| {
        h.list
            .quads
            .iter()
            .filter(|q| q.tex.is_none() && q.colour & 0x00FF_FFFF == colour(kind) & 0x00FF_FFFF)
            .count()
    };
    let mut built = drawn();
    let place = |kind, name: &str, ns, ew| Place {
        kind,
        name: name.to_owned(),
        ns,
        ew,
    };
    built.places = Arc::new(vec![
        place(PlaceKind::Town, "Holtburg", 42.2, 33.8),
        place(PlaceKind::Dungeon, "Drudge Hideout", 42.5, 33.5),
        place(PlaceKind::Portal, "Portal to Somewhere", 42.0, 34.1),
        place(PlaceKind::Lifestone, "Life Stone", 42.1, 33.9),
        place(PlaceKind::Npc, "Guard", 42.25, 33.7),
    ]);
    h.ui.windows.map_view.show(built);
    let at = |h: &mut Harness, zoom: f32| {
        h.ui.windows.map_view.zoom = zoom;
        h.ui.windows.map_view.centre = map_pixel(42.2, 33.8);
        h.frame(&state);
        [
            PlaceKind::Dungeon,
            PlaceKind::Portal,
            PlaceKind::Lifestone,
            PlaceKind::Npc,
        ]
        .map(|k| marks(h, k))
    };
    assert_eq!(at(&mut h, 1.0), [0, 0, 0, 0], "towns alone");
    assert_eq!(at(&mut h, 2.0), [1, 0, 0, 0], "and the dungeons");
    assert_eq!(
        at(&mut h, 3.0),
        [1, 1, 1, 0],
        "and the portals and lifestones"
    );
    assert_eq!(at(&mut h, 10.0), [1, 1, 1, 1], "and the people");
}

/// The map window open on a screen of `screen` pixels at the interface scale `scale` (`None`, the
/// one the screen offers), with the fonts the client carries so its names have their widths,
/// showing `places` about `(ns, ew)` at `zoom`.
fn showing(
    (screen, scale): ((f32, f32), Option<f32>),
    places: Vec<Place>,
    zoom: f32,
    about: (f64, f64),
) -> (Harness, Rect) {
    let state = player();
    let pieces = dereth_horizon::pieces::Pieces::built_in().expect("the pieces built in");
    let mut h = Harness {
        ui: HorizonUi::new(
            Arc::new(Art::new(Arc::new(pieces))),
            HorizonOptions {
                scale,
                ..HorizonOptions::default()
            },
        ),
        list: DrawList::default(),
        input: InputFrame::default(),
        screen,
    };
    h.frame(&state);
    h.ui.windows.open(WindowId::Map, 0.0);
    h.frame(&state);
    let mut built = drawn();
    built.places = Arc::new(places);
    h.ui.windows.map_view.show(built);
    h.ui.windows.map_view.zoom = zoom;
    h.ui.windows.map_view.centre = map_pixel(about.0, about.1);
    h.frame(&state);
    let area = h.ui.windows.map_picture.expect("the map's picture");
    (h, area)
}

fn named(h: &Harness) -> Vec<&str> {
    h.ui.windows
        .map_view
        .named
        .iter()
        .map(|(_, name, _)| name.as_str())
        .collect()
}

/// No two names drawn over the picture overlap.
fn assert_apart(h: &Harness) {
    let names = &h.ui.windows.map_view.named;
    for (i, (_, a, ra)) in names.iter().enumerate() {
        for (_, b, rb) in &names[i + 1..] {
            assert!(
                ra.intersect(rb).is_none(),
                "{a:?} {ra:?} overlaps {b:?} {rb:?}"
            );
        }
    }
}

/// A screen of 1080 lines at 100%, and the largest picture the map draws: 7680x4320 at 300%.
const FULL_HD: ((f32, f32), Option<f32>) = ((1920.0, 1080.0), None);
const LARGEST: ((f32, f32), Option<f32>) = ((7680.0, 4320.0), Some(3.0));

fn spot(kind: PlaceKind, name: &str, ns: f64, ew: f64) -> Place {
    Place {
        kind,
        name: name.to_owned(),
        ns,
        ew,
    }
}

/// A town and, five units south and on, one place of each kind three units apart, the person
/// beside the vendor: far enough apart that no two names meet at any step.
fn a_column() -> Vec<Place> {
    vec![
        spot(PlaceKind::Town, "Holtburg", 48.0, 33.8),
        spot(PlaceKind::Dungeon, "Drudge Hideout", 43.0, 33.8),
        spot(PlaceKind::Portal, "Portal to Somewhere", 40.0, 33.8),
        spot(PlaceKind::Lifestone, "Life Stone", 37.0, 33.8),
        spot(PlaceKind::Vendor, "Shopkeeper", 34.0, 33.8),
        spot(PlaceKind::Npc, "Guard", 34.0, 36.8),
    ]
}

/// The whole map names its towns alone; closing in names the dungeons a step after they are
/// marked, then the portals and lifestones, the vendors, and the people last.
#[test]
fn each_kind_of_place_is_named_a_step_after_it_is_marked() {
    let at = |zoom| {
        let (h, _) = showing(FULL_HD, a_column(), zoom, (40.0, 34.5));
        assert_apart(&h);
        named(&h).into_iter().map(str::to_owned).collect::<Vec<_>>()
    };
    assert_eq!(at(1.0), ["Holtburg"]);
    assert_eq!(at(2.0), ["Holtburg"], "the dungeon marked, not named");
    assert_eq!(at(3.0), ["Holtburg", "Drudge Hideout"]);
    assert_eq!(
        at(5.0),
        [
            "Holtburg",
            "Drudge Hideout",
            "Life Stone",
            "Portal to Somewhere"
        ],
        "the towns first, then by rank"
    );
    assert_eq!(at(6.0).len(), 4, "the vendor marked, not named");
    assert_eq!(at(8.0).len(), 5);
    assert!(
        !at(10.0).contains(&"Guard".to_owned()),
        "the person marked, not named"
    );
    assert!(at(14.0).contains(&"Guard".to_owned()));
}

/// The whole world is its towns alone in a window of any size, and the closest view names the
/// people in the largest map as in one at 1080 lines, though it cannot zoom as far.
#[test]
fn the_closest_view_names_the_people_in_a_small_window_and_a_large_one() {
    for screen in [FULL_HD, LARGEST] {
        let (whole, area) = showing(screen, a_column(), 1.0, (40.0, 34.5));
        assert_eq!(named(&whole), ["Holtburg"], "{screen:?}: {area:?}");
        let (closest, area) = showing(screen, a_column(), 1000.0, (34.0, 36.8));
        let zoom = closest.ui.windows.map_view.zoom;
        assert!(
            named(&closest).contains(&"Guard"),
            "{screen:?}, at {zoom}: {:?}",
            named(&closest)
        );
        if screen == LARGEST {
            assert!(area.w > 1500.0, "{area:?}");
            assert!(
                zoom < 14.0,
                "its closest is under the zoom 1080 lines names people at: {zoom}"
            );
        }
    }
}

/// Two towns side by side: both are drawn, and the name that would overlap the other's is left
/// out. A dungeon just under a town: the town keeps its name and the dungeon's is left out.
#[test]
fn names_drawn_over_the_picture_never_overlap_and_towns_keep_theirs() {
    let (h, _) = showing(
        FULL_HD,
        vec![
            spot(PlaceKind::Town, "Holtburg", 42.2, 33.8),
            spot(PlaceKind::Town, "Holtburg Outskirts", 42.1, 33.9),
        ],
        1.0,
        (42.2, 33.8),
    );
    assert_apart(&h);
    assert_eq!(named(&h), ["Holtburg"]);

    let (h, _) = showing(
        FULL_HD,
        vec![
            spot(PlaceKind::Dungeon, "Drudge Hideout", 41.0, 33.6),
            spot(PlaceKind::Town, "Holtburg", 42.2, 33.8),
        ],
        3.0,
        (42.2, 33.8),
    );
    assert_apart(&h);
    assert_eq!(
        named(&h),
        ["Holtburg"],
        "the town's name over the dungeon's"
    );
    let (h, _) = showing(
        FULL_HD,
        vec![
            spot(PlaceKind::Dungeon, "Drudge Hideout", 36.0, 33.6),
            spot(PlaceKind::Town, "Holtburg", 42.2, 33.8),
        ],
        3.0,
        (40.0, 33.8),
    );
    assert_eq!(
        named(&h),
        ["Holtburg", "Drudge Hideout"],
        "apart, both named"
    );
}

/// A store of one world's learned places about Holtburg: a lifestone outdoors, a vendor in a
/// shop of the town, and a person in a dungeon.
fn learned(world: &str) -> (Arc<dereth_navigation::PlaceStore>, u64) {
    use dereth_navigation::{EdgeSource, KnownPlace, PlaceKind as L, PlaceStore};
    use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};
    let at = |cell: u32, x: f32, y: f32| {
        Position::new(
            CellId(cell),
            Frame::new(Vec3::new(x, y, 60.0), Quat::IDENTITY),
        )
    };
    let place = |kind, name: &str, at| KnownPlace {
        kind,
        source: EdgeSource::Learned,
        wcid: None,
        name: name.to_owned(),
        at,
        first_seen: 0,
        last_seen: 0,
        owner: None,
        to: None,
    };
    let mut s = PlaceStore::new(world);
    s.see(place(
        L::Lifestone,
        "Life Stone",
        at(0xA9B4_002A, 132.0, 36.0),
    ));
    s.see(place(L::Vendor, "Grocer", at(0xA9B4_0150, 140.0, 40.0)));
    s.see(place(L::Npc, "Guard", at(0x01D9_0108, 10.0, 20.0)));
    let revision = s.revision();
    (Arc::new(s), revision)
}

/// What the player learned of the world they are in is marked over the data's places, and only
/// that world's: a place in a dungeon is not on the map.
#[test]
fn a_player_sees_the_places_they_learned_of_this_world_over_the_data_s() {
    let art = Arc::new(Art::empty());
    let marks = |h: &Harness, kind: PlaceKind| {
        h.list
            .quads
            .iter()
            .filter(|q| q.tex.is_none() && q.colour & 0x00FF_FFFF == colour(kind) & 0x00FF_FFFF)
            .count()
    };
    let mut state = player();
    let (mut h, _) = opened(&art, &state);
    h.ui.windows.map_view.show(drawn());
    h.ui.windows.map_view.zoom = 12.0;
    h.ui.windows.map_view.centre = map_pixel(42.2, 33.8);
    h.frame(&state);
    assert_eq!(marks(&h, PlaceKind::Lifestone), 0, "nothing learned yet");

    state.world_places = Some(learned("Here@127.0.0.1:9000"));
    h.frame(&state);
    assert_eq!(marks(&h, PlaceKind::Lifestone), 1, "the lifestone outdoors");
    assert_eq!(
        marks(&h, PlaceKind::Vendor),
        1,
        "the vendor in a shop of the town"
    );
    assert_eq!(marks(&h, PlaceKind::Npc), 0, "the guard in a dungeon");
    assert_eq!(marks(&h, PlaceKind::Cottage), 2, "the data's places still");

    // Another world: what was learned in this one is gone from the map.
    state.world_places = Some((
        Arc::new(dereth_navigation::PlaceStore::new(
            "Elsewhere@10.0.0.1:9000",
        )),
        0,
    ));
    h.frame(&state);
    assert_eq!(marks(&h, PlaceKind::Lifestone), 0);
    assert_eq!(marks(&h, PlaceKind::Vendor), 0);
}
