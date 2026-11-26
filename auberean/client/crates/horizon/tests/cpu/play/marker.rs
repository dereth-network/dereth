//! The selection's marks in the world: where the selection is on the screen, the arrow over its
//! name, the ring the world lays under it, the arrow at the screen's edge for one off it, and the
//! marks moved to where the world is drawn.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_horizon::draw::{DrawList, Rect};
use auberean_horizon::state::{GameState, Target};
use auberean_horizon::ui::marker::{
    edge_rect, follow, ground, locate, Recent, Where, MARK, NAME_GAP,
};
use auberean_primitives::Vec3;
use auberean_scene::seen::{Seen, SeenBody, SeenCamera};
use dereth_ui_kit::selection::{
    ARROW_BOB, ARROW_GAP, ARROW_HEIGHT, RING_INNER_TEXTURE, RING_SCALE, RING_TEXTURE, UNTINTED,
};

use crate::{in_world, Harness};

const DRUDGE: u64 = 0x8130_0000_0000_0042;
const SCREEN: (f32, f32) = (1920.0, 1080.0);

/// A body a meter square and two tall, its feet at `at`.
const BOUNDS: (Vec3, Vec3) = (Vec3::new(-0.5, -0.5, 0.0), Vec3::new(0.5, 0.5, 2.0));

/// A frame seen from a meter up at the block's origin, looking north, with the drudge at `at`.
fn seen_with(at: Vec3) -> Seen {
    Seen {
        camera: Some(SeenCamera {
            position: Vec3::new(0.0, 0.0, 1.0),
            heading: 0.0,
            pitch: 0.0,
            roll: 0.0,
            fov_y: 60.0,
            near: 0.1,
        }),
        block: (0, 0),
        bodies: vec![SeenBody {
            id: DRUDGE,
            origin: at,
            heading: 0.0,
            bounds: Some(BOUNDS),
            faded: false,
        }],
        hearing: None,
    }
}

fn selected(at: Where, attacking: bool) -> GameState {
    let mut s = in_world();
    s.target = Some(Target {
        id: DRUDGE,
        name: "Drudge".into(),
        health: None,
    });
    s.selection_at = Some(at);
    s.combat.attacking = attacking;
    s
}

/// The middles of the flat quads drawn in `colour`.
fn marks(list: &DrawList, colour: u32) -> Vec<(f32, f32)> {
    list.quads
        .iter()
        .filter(|q| q.tex.is_none() && q.colour == colour)
        .map(|q| (q.dst.x + q.dst.w / 2.0, q.dst.y + q.dst.h / 2.0))
        .collect()
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

#[test]
fn a_selection_ahead_fills_a_box_round_the_screen_s_middle() {
    let seen = seen_with(Vec3::new(0.0, 10.0, 0.0));
    let Some(Where::Box(b)) = locate(&seen, DRUDGE, BOUNDS, SCREEN) else {
        panic!("on the screen");
    };
    assert!(near(b.x + b.w / 2.0, SCREEN.0 / 2.0), "{b:?}");
    assert!(b.y < SCREEN.1 / 2.0 && b.bottom() > SCREEN.1 / 2.0, "{b:?}");
    assert!(b.h > b.w, "taller than wide: {b:?}");
    // Nearer, it fills more of the screen.
    let Some(Where::Box(close)) =
        locate(&seen_with(Vec3::new(0.0, 5.0, 0.0)), DRUDGE, BOUNDS, SCREEN)
    else {
        panic!("on the screen");
    };
    assert!(close.h > b.h);
}

#[test]
fn a_selection_off_the_screen_or_behind_the_camera_is_pointed_to_and_one_not_drawn_is_not() {
    let right = locate(
        &seen_with(Vec3::new(100.0, 10.0, 0.0)),
        DRUDGE,
        BOUNDS,
        SCREEN,
    );
    let Some(Where::Off { toward }) = right else {
        panic!("off the screen: {right:?}");
    };
    assert!(toward.0 > 0.9, "to the right: {toward:?}");
    let behind = locate(
        &seen_with(Vec3::new(-5.0, -10.0, 0.0)),
        DRUDGE,
        BOUNDS,
        SCREEN,
    );
    let Some(Where::Off { toward }) = behind else {
        panic!("behind: {behind:?}");
    };
    assert!(
        toward.0 < 0.0,
        "to the left, the way the camera turns to it: {toward:?}"
    );
    let mut gone = seen_with(Vec3::new(0.0, 10.0, 0.0));
    gone.bodies.clear();
    assert_eq!(locate(&gone, DRUDGE, BOUNDS, SCREEN), None, "not drawn");
    gone = seen_with(Vec3::new(0.0, 10.0, 0.0));
    gone.bodies[0].faded = true;
    assert_eq!(locate(&gone, DRUDGE, BOUNDS, SCREEN), None, "faded out");
}

#[test]
fn the_box_is_averaged_over_its_last_four_frames_and_starts_again_for_a_new_selection() {
    let mut recent = Recent::default();
    let mut seen = seen_with(Vec3::new(0.0, 10.0, 0.0));
    assert_eq!(recent.average(&seen, DRUDGE), Some(BOUNDS));
    let tall = (Vec3::new(-0.5, -0.5, 0.0), Vec3::new(0.5, 0.5, 6.0));
    seen.bodies[0].bounds = Some(tall);
    // Two, then four frames: half, then three quarters of the way to the new box's top.
    assert!(near(recent.average(&seen, DRUDGE).unwrap().1.z, 4.0));
    recent.average(&seen, DRUDGE);
    assert!(near(recent.average(&seen, DRUDGE).unwrap().1.z, 5.0));
    assert!(
        near(recent.average(&seen, DRUDGE).unwrap().1.z, 6.0),
        "the first frame let go of"
    );
    seen.bodies[0].id = 9;
    seen.bodies[0].bounds = Some(BOUNDS);
    assert_eq!(
        recent.average(&seen, 9),
        Some(BOUNDS),
        "a new selection's own box"
    );
}

#[test]
fn one_arrow_points_down_over_the_name_centered_over_the_box_untinted_while_attacking() {
    let b = Rect::new(800.0, 300.0, 120.0, 260.0);
    for attacking in [false, true] {
        let mut h = Harness::new();
        let k = h.k();
        h.frame(&selected(Where::Box(b), attacking));
        let drawn = marks(&h.list, MARK);
        assert_eq!(drawn.len(), 1, "one arrow: {drawn:?}");
        let arrow = h
            .list
            .quads
            .iter()
            .find(|q| q.tex.is_none() && q.colour == MARK)
            .expect("the arrow")
            .dst;
        assert!(near(drawn[0].0, b.x + b.w / 2.0), "centered: {arrow:?}");
        // Over the name, which stands over the box's top (the arrow's square drawn a quarter in
        // where the art has none): its foot the gap over the name, bobbing up a little.
        assert!((arrow.h - ARROW_HEIGHT * k / 2.0).abs() < 0.01, "{arrow:?}");
        let foot = arrow.bottom() + arrow.h / 2.0;
        let over_name = b.y - NAME_GAP * k - ARROW_GAP * k;
        assert!(
            foot < over_name,
            "over the name: {arrow:?} (box top {})",
            b.y
        );
        assert!(
            foot >= over_name - 40.0 * k - ARROW_BOB * k,
            "close over it: {arrow:?} (box top {})",
            b.y
        );
    }
}

#[test]
fn the_arrow_is_drawn_in_the_art_s_own_colors_for_every_selection() {
    let b = Rect::new(800.0, 300.0, 120.0, 260.0);
    for attacking in [false, true] {
        let mut h = Harness::with_built_in_art();
        h.frame(&selected(Where::Box(b), attacking));
        let centre = b.x + b.w / 2.0;
        let arrow: Vec<_> = h
            .list
            .quads
            .iter()
            .filter(|q| q.tex.is_some() && q.dst.bottom() <= b.y && q.dst.h > 20.0)
            .filter(|q| near(q.dst.x + q.dst.w / 2.0, centre))
            .collect();
        assert!(!arrow.is_empty(), "the art's arrow over the box");
        assert!(
            arrow.iter().all(|q| q.colour == UNTINTED),
            "untinted: {:?}",
            arrow.iter().map(|q| q.colour).collect::<Vec<_>>()
        );
    }
}

#[test]
fn a_player_or_creature_selected_is_ringed_on_the_ground_untinted_and_a_thing_is_not() {
    let mut s = selected(Where::Box(Rect::new(800.0, 300.0, 120.0, 260.0)), false);
    s.ringed = true;
    let ring = ground(&s, 1.0, false);
    assert_eq!(ring.len(), 1);
    assert_eq!(ring[0].object, DRUDGE);
    assert_eq!(ring[0].texture, RING_TEXTURE);
    assert_eq!(ring[0].tint, UNTINTED);
    assert!((ring[0].scale - RING_SCALE).abs() < f32::EPSILON);
    // While attacking it stays as it is.
    s.combat.attacking = true;
    assert_eq!(ground(&s, 1.0, false), ring);
    let both = ground(&s, 1.0, true);
    assert_eq!(
        both.iter().map(|m| (m.texture, m.tint)).collect::<Vec<_>>(),
        [(RING_INNER_TEXTURE, UNTINTED), (RING_TEXTURE, UNTINTED)],
        "the inner layer under it, untinted too"
    );
    s.ringed = false;
    assert!(ground(&s, 1.0, true).is_empty(), "a thing has no ring");
    s.ringed = true;
    s.target = None;
    assert!(ground(&s, 1.0, true).is_empty(), "nothing selected");
}

#[test]
fn only_players_and_creatures_are_ringed() {
    use auberean_horizon::state::ringed;
    assert!(ringed(0x1F), "a player");
    assert!(ringed(0x4F), "a creature");
    assert!(!ringed(0x81), "an item");
    assert!(!ringed(0x103), "an item");
    assert!(!ringed(0x81 | 0x4F), "an item, whatever else its bits say");
    assert!(!ringed(0), "a thing");
}

#[test]
fn a_selection_off_the_screen_has_one_arrow_at_the_edge_the_way_to_it() {
    let mut h = Harness::new();
    let k = h.k();
    let (r, turn) = edge_rect(h.screen, k, (1.0, 0.0));
    assert!(
        r.right() <= h.screen.0 && r.right() > h.screen.0 - 80.0 * k,
        "at the right: {r:?}"
    );
    assert!(
        near(r.y + r.h / 2.0, h.screen.1 / 2.0),
        "level with the middle"
    );
    assert!(
        (turn + std::f32::consts::FRAC_PI_2).abs() < 1e-4,
        "turned a quarter back from pointing down, to point right: {turn}"
    );
    let (up, _) = edge_rect(h.screen, k, (0.0, -1.0));
    assert!(up.y >= 0.0 && up.y < 80.0 * k, "at the top: {up:?}");
    let (down, _) = edge_rect(h.screen, k, (0.0, 1.0));
    assert!(
        down.bottom() < auberean_horizon::ui::hotbar::slot_rect(h.screen, k, 0, 0).y,
        "clear of the hotbar: {down:?}"
    );
    h.frame(&selected(Where::Off { toward: (1.0, 0.0) }, false));
    let drawn = marks(&h.list, MARK);
    assert_eq!(drawn.len(), 1, "the one arrow: {drawn:?}");
    assert!(r.contains(drawn[0].0, drawn[0].1));
}

#[test]
fn the_marks_move_to_where_the_world_is_drawn_and_hide_when_it_goes_off_the_screen() {
    let b = Rect::new(800.0, 300.0, 120.0, 260.0);
    let mut h = Harness::new();
    h.frame(&selected(Where::Box(b), false));
    let before = marks(&h.list, MARK);
    assert_eq!(before.len(), 1);
    follow(&mut h.list, Some(Where::Box(b.offset(30.0, -20.0))), &[]);
    let after = marks(&h.list, MARK);
    for (a, m) in before.iter().zip(&after) {
        assert!(
            near(m.0, a.0 + 30.0) && near(m.1, a.1 - 20.0),
            "{a:?} to {m:?}"
        );
    }
    follow(&mut h.list, Some(Where::Off { toward: (1.0, 0.0) }), &[]);
    let hidden = h
        .list
        .quads
        .iter()
        .filter(|q| q.tex.is_none() && q.colour == MARK)
        .all(|q| q.dst.w == 0.0 && q.dst.h == 0.0);
    assert!(hidden, "hidden once off the screen");
}

#[test]
fn nothing_selected_draws_no_marks() {
    let mut h = Harness::new();
    let mut s = selected(Where::Box(Rect::new(800.0, 300.0, 120.0, 260.0)), false);
    s.target = None;
    h.frame(&s);
    assert!(marks(&h.list, MARK).is_empty());
}
