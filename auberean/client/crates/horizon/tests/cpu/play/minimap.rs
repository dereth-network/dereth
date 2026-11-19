//! The minimap: what it shows of the view's objects on the radar, where, and what a click and its
//! zoom ask; its rings; the place line under it.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::{self as v, radar_shape, View};
use auberean_client_sdk::Action;
use auberean_horizon::draw::Rect;
use auberean_horizon::state::{argb, Blip, Frame, GameState, Radar};
use auberean_horizon::ui::minimap::{
    blip_at, parts, place_line, reach, ring_meters, ring_radii, RING_DARK, SELECTED,
};

use crate::{in_world, Harness, PLAYER};

/// The player's cell, outdoors: block (0x29, 0x98).
const CELL: u32 = 0x2998_0021;
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

fn thing(id: u64, place: Option<(u32, [f32; 3])>, radar: Option<([f32; 4], u32)>) -> v::Thing {
    v::Thing {
        id,
        name: Some(format!("thing {id}")),
        place,
        radar,
        ..v::Thing::default()
    }
}

/// A view of the player at (80, 80) in `cell` facing east, and `things` around it.
fn view(cell: u32, things: Vec<v::Thing>, selected: u64) -> View {
    View {
        player_id: Some(PLAYER),
        player: Some(v::Player {
            id: PLAYER,
            body: v::Body {
                cell,
                origin: [80.0, 80.0, 0.0],
                heading: 90.0,
                moving: false,
            },
            ..v::Player::default()
        }),
        world: Some(v::Surroundings {
            player: PLAYER,
            things,
            selection: v::Selection {
                current: selected,
                ..v::Selection::default()
            },
            inside: cell & 0xFFFF >= 0x100,
            map_coordinates: Some((-12.34, 56.78)),
            show_coordinates: true,
            ..v::Surroundings::default()
        }),
        ..View::default()
    }
}

fn no_name(_: &TextRef) -> Option<String> {
    None
}

fn follow(view: &View) -> GameState {
    let mut s = GameState::default();
    s.follow(&Frame {
        view: Some(view),
        notices: &[],
        events: &[],
        told: &[],
        lines: &[],
        write: &no_name,
        unknown_command: "",
    });
    s
}

#[test]
fn the_minimap_holds_what_the_scripts_put_on_the_radar_within_its_reach_where_it_stands() {
    let shape = radar_shape::CROSS;
    let look = Some((RED, shape));
    // The next block east: 160 meters on, so 100 east of the player and 10 north.
    let east_block = CELL + (1 << 24);
    let mut held = thing(6, None, look);
    held.parent = Some(2);
    let things = vec![
        thing(PLAYER, Some((CELL, [80.0, 80.0, 0.0])), look),
        thing(2, Some((CELL, [90.0, 70.0, 0.0])), look),
        thing(3, Some((east_block, [20.0, 90.0, 0.0])), look),
        thing(4, Some((CELL, [81.0, 80.0, 0.0])), None),
        thing(5, None, look),
        held,
    ];
    let s = follow(&view(CELL, things, 2));
    let r = &s.radar;
    assert!((r.range - 75.0).abs() < 1e-6, "outdoors, the longer range");
    assert!((r.heading - 90.0).abs() < 1e-6);
    let at: Vec<(u64, f32, f32, bool)> = r
        .blips
        .iter()
        .map(|b| (b.id, b.east, b.north, b.selected))
        .collect();
    assert_eq!(
        at,
        vec![(2, 10.0, -10.0, true), (6, 10.0, -10.0, false)],
        "the player, one past the reach, one off the radar and one with no place are not on it; \
         a held thing is where its holder is"
    );
    assert_eq!(r.blips[0].colour, 0xFFFF_0000);
    assert_eq!(r.blips[0].shape, shape);
    assert_eq!(r.blips[0].name, "thing 2");

    // Inside, the shorter range: 50 meters.
    let inside = CELL & 0xFFFF_0000 | 0x0105;
    let far = thing(7, Some((inside, [80.0, 140.0, 0.0])), look);
    let s = follow(&view(inside, vec![far], 0));
    assert!(s.radar.inside);
    assert!((s.radar.range - 50.0).abs() < 1e-6);
    assert!(
        s.radar.blips.is_empty(),
        "60 meters on is past the reach inside"
    );
}

#[test]
fn the_place_line_reads_the_map_coordinates_to_a_tenth_and_inside_reads_inside() {
    let mut s = in_world();
    s.radar.coordinates = Some((-12.34, 56.78));
    assert_eq!(place_line(&s).as_deref(), Some("56.8N 12.3W"));
    s.radar.coordinates = Some((3.0, -4.0));
    assert_eq!(place_line(&s).as_deref(), Some("4.0S 3.0E"));
    s.radar.inside = true;
    assert_eq!(place_line(&s).as_deref(), Some("Indoors"));
    s.radar.inside = false;
    s.radar.coordinates = None;
    assert_eq!(place_line(&s), None);
}

#[test]
fn a_color_from_the_scripts_is_one_word_alpha_red_green_blue() {
    assert_eq!(argb([1.0, 0.5, 0.0, 1.0]), 0xFFFF_7F00);
    assert_eq!(argb([0.0, 0.0, 1.0, 0.0]), 0x0000_00FF);
}

/// A player in the world with `blips` on a minimap reaching 75 meters.
fn radar_with(blips: Vec<Blip>) -> GameState {
    let mut s = in_world();
    s.radar = Radar {
        range: 75.0,
        show_coordinates: true,
        coordinates: Some((1.0, 2.0)),
        blips,
        ..Radar::default()
    };
    s
}

fn blip(id: u64, east: f32, north: f32, colour: u32, shape: u32) -> Blip {
    Blip {
        id,
        east,
        north,
        colour,
        shape,
        name: format!("blip {id}"),
        selected: false,
    }
}

/// Where the minimap draws a blip `east` and `north` of the player, at zoom `zoom`.
fn drawn_at(h: &Harness, east: f32, north: f32, zoom: f32) -> Option<(f32, f32)> {
    let map = parts(h.screen, h.k()).map;
    let centre = (map.x + map.w / 2.0, map.y + map.h / 2.0);
    blip_at(centre, reach(map), 75.0 * zoom, east, north)
}

fn fills(h: &Harness, colour: u32) -> Vec<Rect> {
    h.list
        .quads
        .iter()
        .filter(|q| q.tex.is_none() && q.colour == colour)
        .map(|q| q.dst)
        .collect()
}

#[test]
fn each_blip_is_drawn_north_up_in_its_color_and_shape_and_the_selection_is_marked() {
    let mut h = Harness::new();
    let green = 0xFF00_FF00;
    let blue = 0xFF00_00FF;
    let mut picked = blip(3, -30.0, 0.0, blue, radar_shape::POINT);
    picked.selected = true;
    h.frame(&radar_with(vec![
        blip(2, 0.0, 30.0, green, radar_shape::CROSS),
        picked,
    ]));
    let green_at = fills(&h, green);
    assert_eq!(green_at.len(), 5, "a cross is five points");
    let map = parts(h.screen, h.k()).map;
    let centre = (map.x + map.w / 2.0, map.y + map.h / 2.0);
    let (x, y) = drawn_at(&h, 0.0, 30.0, 1.0).expect("on the map");
    assert!(
        green_at.iter().any(|r| r.contains(x, y)),
        "a point at its place: {green_at:?}"
    );
    assert!((x - centre.0).abs() < 0.01 && y < centre.1, "north is up");
    let (bx, by) = drawn_at(&h, -30.0, 0.0, 1.0).expect("on the map");
    assert!(
        bx < centre.0 && (by - centre.1).abs() < 0.01,
        "west is left"
    );
    assert!(
        fills(&h, blue).iter().any(|r| r.contains(bx, by)),
        "the west one at its place"
    );
    assert_eq!(fills(&h, blue).len(), 1, "a point is one");
    assert!(
        h.list.quads.iter().any(|q| q.colour == SELECTED),
        "the selection is marked"
    );
}

#[test]
fn a_click_on_a_blip_selects_its_object_and_the_zoom_brings_the_reach_in() {
    let mut h = Harness::new();
    let s = radar_with(vec![blip(2, 0.0, 30.0, 0xFF00_FF00, radar_shape::FILL)]);
    h.frame(&s);
    let at = drawn_at(&h, 0.0, 30.0, 1.0).expect("on the map");
    let out = h.click(&s, at);
    assert_eq!(out.acts, vec![Action::SetSelection { target: 2 }]);
    let empty = parts(h.screen, h.k()).map;
    let corner = (
        empty.x + empty.w / 2.0 + 40.0 * h.k(),
        empty.y + empty.h / 2.0,
    );
    assert!(h.click(&s, corner).acts.is_empty(), "no blip: nothing");
    assert!(
        h.ui.covers(corner.0, corner.1),
        "the map answers the pointer"
    );

    // Zoomed in to half the reach, 30 meters north is at the edge's middle; to a quarter, off it.
    let zoom_in = parts(h.screen, h.k()).zoom_in;
    let middle = |r: Rect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
    h.click(&s, middle(zoom_in));
    assert!((h.ui.minimap.zoom() - 0.5).abs() < 1e-6);
    let near_edge = drawn_at(&h, 0.0, 30.0, 0.5).expect("still on the map");
    assert!(
        (near_edge.1 - at.1).abs() > 1.0,
        "drawn further out at half the reach"
    );
    h.click(&s, middle(zoom_in));
    h.click(&s, middle(zoom_in));
    assert!(
        (h.ui.minimap.zoom() - 0.25).abs() < 1e-6,
        "a quarter at most"
    );
    h.frame(&s);
    assert!(
        fills(&h, 0xFF00_FF00).is_empty(),
        "past the reach: not drawn"
    );
    let zoom_out = parts(h.screen, h.k()).zoom_out;
    for _ in 0..3 {
        h.click(&s, middle(zoom_out));
    }
    assert!(
        (h.ui.minimap.zoom() - 1.0).abs() < 1e-6,
        "the whole reach at most"
    );
}

#[test]
fn a_creature_at_the_player_s_feet_is_drawn_over_the_player_s_arrow() {
    for mut h in [Harness::new(), Harness::with_built_in_art()] {
        let red = 0xFFFF_2000;
        let mut s = radar_with(vec![blip(2, 1.0, 1.0, red, radar_shape::CROSS)]);
        s.radar.heading = 30.0;
        h.frame(&s);
        let map = parts(h.screen, h.k()).map;
        let centre = (map.x + map.w / 2.0, map.y + map.h / 2.0);
        // The arrow: the turned picture at the middle, or (with no art) the white square there.
        let arrow = h
            .list
            .quads
            .iter()
            .position(|q| {
                q.dst.contains(centre.0, centre.1)
                    && q.dst.w < map.w / 4.0
                    && (q.turn != 0.0 || (q.tex.is_none() && q.colour == 0xFFFF_FFFF))
            })
            .expect("the player's arrow");
        let blip = h
            .list
            .quads
            .iter()
            .rposition(|q| q.tex.is_none() && q.colour == red)
            .expect("the creature's blip");
        assert!(
            blip > arrow,
            "the blip drawn after the arrow: {blip} vs {arrow}"
        );
    }
}

/// How far from the minimap's middle each ring reaches, read off its dark line's runs across the
/// middle row, right of the middle, nearest first.
fn ring_radii_drawn(h: &Harness) -> Vec<f32> {
    let map = parts(h.screen, h.k()).map;
    let (cx, cy) = (map.x + map.w / 2.0, map.y + map.h / 2.0);
    let mut out: Vec<f32> = fills(h, RING_DARK)
        .iter()
        .filter(|r| r.y <= cy && r.bottom() > cy && r.x >= cx)
        .map(|r| r.right() - cx)
        .collect();
    out.sort_by(f32::total_cmp);
    out
}

/// Horizon's minimap draws four rings at the game's radar's places, the same inside an interior
/// cell and outdoors: only how far the blips stand changes with the reach, so the rings mark
/// 9.3, 19.2, 29.1 and 38.5 meters inside (a 50-meter reach) and 14.0, 28.8, 43.6 and 57.7
/// meters outdoors (75).
///
/// Behaviour: desktop.horizon.the-minimap-s-rings-stand-where-the-game-s-radar-s-do
#[test]
fn the_minimap_s_rings_stand_where_the_game_s_radar_s_do_inside_and_outdoors() {
    let near =
        |got: [f32; 4], want: [f32; 4]| got.iter().zip(want).all(|(g, w)| (g - w).abs() < 0.06);
    let inside = follow(&view(0x2998_0105, Vec::new(), 0));
    let outdoors = follow(&view(CELL, Vec::new(), 0));
    assert!(inside.radar.inside && !outdoors.radar.inside);
    assert!((inside.radar.range - 50.0).abs() < f32::EPSILON);
    assert!((outdoors.radar.range - 75.0).abs() < f32::EPSILON);
    let (inside_m, outdoors_m) = (
        ring_meters(inside.radar.range),
        ring_meters(outdoors.radar.range),
    );
    assert!(near(inside_m, [9.3, 19.2, 29.1, 38.5]), "{inside_m:?}");
    assert!(near(outdoors_m, [14.0, 28.8, 43.6, 57.7]), "{outdoors_m:?}");

    let mut h = Harness::new();
    let mut drawn = Vec::new();
    for s in [&inside, &outdoors] {
        let mut s = s.clone();
        s.in_world = true;
        h.frame(&s);
        drawn.push(ring_radii_drawn(&h));
    }
    let map = parts(h.screen, h.k()).map;
    let want = ring_radii(reach(map));
    assert_eq!(drawn[0].len(), 4, "{:?}", drawn[0]);
    for (got, want) in drawn[0].iter().zip(want) {
        assert!((got - want).abs() < 0.5, "{got} against {want}");
    }
    assert_eq!(drawn[0], drawn[1], "the rings stay where they are");
}

/// How far from the middle of the frame's picture its inside edge begins, as a share of half the
/// picture's side: where its texels' mean opacity, all the way round, first reaches half.
fn frame_inside_edge() -> f32 {
    use dereth_primitives::num::math;
    use dereth_ui_kit::pieces::{atlas_path, decode_png, Manifest};
    let pieces = auberean_horizon::art::Pieces::built_in().expect("the pieces built in");
    let manifest =
        Manifest::parse(pieces.get("manifest.json").expect("the manifest")).expect("a manifest");
    let (scale, atlas, [x, y, w, h], _) = manifest.place("minimap.ring", 1.0).expect("placed");
    let image =
        decode_png(pieces.get(&atlas_path(scale, atlas)).expect("the atlas")).expect("a picture");
    #[allow(clippy::cast_precision_loss)]
    let (cx, cy, half) = (w as f32 / 2.0, h as f32 / 2.0, w as f32 / 2.0);
    let alpha = |px: f32, py: f32| -> f32 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (tx, ty) = (px.floor().max(0.0) as u32, py.floor().max(0.0) as u32);
        let (tx, ty) = (x + tx.min(w - 1), y + ty.min(h - 1));
        let i = usize::try_from((ty * image.width + tx) * 4 + 3).expect("a texel");
        f32::from(image.bgra[i])
    };
    let mut r = half * 0.5;
    while r < half {
        let mut sum = 0.0;
        for step in 0..720 {
            #[allow(clippy::cast_precision_loss)]
            let a = step as f32 * std::f32::consts::TAU / 720.0;
            sum += alpha(cx + r * math::cosf(a), cy + r * math::sinf(a));
        }
        if sum / 720.0 >= 127.5 {
            return r / half;
        }
        r += 0.05;
    }
    panic!("the frame has no inside edge");
}

/// The minimap's reach ends where its frame's inside edge begins, as the game's own radar's reach
/// ends at its bezel: a blip at the very edge of the reach stands at the frame, inside or out, and
/// the rings stand at their shares of that reach.
///
/// Behaviour: desktop.horizon.the-minimap-s-reach-ends-at-its-frame-as-the-game-s-radar-s-does
#[test]
fn the_minimap_s_reach_ends_at_its_frame_as_the_game_s_radar_s_does() {
    let mut h = Harness::with_built_in_art();
    let edge = frame_inside_edge();
    let map = parts(h.screen, h.k()).map;
    let share = reach(map) / (map.w / 2.0);
    assert!(
        (share - edge).abs() < 0.004,
        "{share} against the frame's {edge}"
    );
    // A blip at the reach, north, inside and out: at the frame's edge, and drawn.
    for range in [50.0_f32, 75.0] {
        let mut s = radar_with(vec![blip(2, 0.0, range, 0xFF00_FF00, radar_shape::POINT)]);
        s.radar.range = range;
        h.frame(&s);
        let centre = (map.x + map.w / 2.0, map.y + map.h / 2.0);
        let (x, y) = blip_at(centre, reach(map), range, 0.0, range).expect("on the map");
        assert!((x - centre.0).abs() < 0.01);
        assert!(((centre.1 - y) - reach(map)).abs() < 0.01, "{y}");
        assert_eq!(fills(&h, 0xFF00_FF00).len(), 1, "drawn");
    }
    // The last ring is 0.770 of the way to the frame, as the game's is 46.2 of its 60.
    let last = ring_radii(reach(map))[3] / reach(map);
    assert!((last - 46.2 / 60.0).abs() < 0.001, "{last}");
}
