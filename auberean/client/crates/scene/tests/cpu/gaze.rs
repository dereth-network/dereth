//! Where a body's head turns: toward an object it looks at, along a direction the server sent,
//! and, for the player, the way the camera looks.

use auberean_scene::gaze::{Camera, Gaze, Owner, SliderAsk, Target, PITCH, TURN};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

const CELL: u32 = 0xA9B4_0021;

fn at(x: f32, y: f32, z: f32) -> Position {
    Position::new(CellId(CELL), Frame::new(Vec3::new(x, y, z), Quat::IDENTITY))
}

/// A body 1.8 m tall facing north at the cell's (10, 10, 0), its look sliders level.
fn owner(is_player: bool) -> Owner {
    Owner {
        is_player,
        position: at(10.0, 10.0, 0.0),
        height: 1.8,
        scale: Vec3::new(1.0, 1.0, 1.0),
        sliders: [0.0; 3],
        dir: (0.0, 0.0),
    }
}

fn ask(asks: &[SliderAsk], slider: u32) -> SliderAsk {
    *asks
        .iter()
        .find(|a| a.slider == slider)
        .unwrap_or_else(|| panic!("no ask of {slider:#x}: {asks:?}"))
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-5
}

/// A body asked to look at a creature ahead and to its left turns its head to it at once (each
/// step's asks take effect at once), blended in by a weight that grows over half a second and
/// then holds: the turn is the creature's angle from straight ahead (an eighth of a turn left), the
/// pitch the weighted share of the creature's rise over the level distance from the body's top to
/// three quarters up the creature (half way up something that does not move on its own).
///
/// Behaviour: desktop.bodies.a-body-turns-its-head-to-what-it-looks-at
#[test]
fn a_body_looking_at_an_object_turns_its_head_to_it_over_half_a_second_and_holds() {
    let o = owner(false);
    let creature = Target {
        position: at(7.0, 13.0, 0.0),
        height: 1.8,
        mobile: true,
    };
    let mut g = Gaze::new(1);
    g.look_at(2, 0.5, [0.0; 3]);
    let level = 18.0_f32.sqrt();
    let rise = 1.8 * 0.75 - 1.8;
    let mut weights = Vec::new();
    for step in 0..8 {
        let t = g.update(0.1, &o, Some(&creature), None);
        assert!(!t.stop_looking);
        weights.push(g.weight);
        let turn = ask(&t.asks, TURN);
        let pitch = ask(&t.asks, PITCH);
        assert!(
            close(turn.value, core::f32::consts::FRAC_PI_4 * g.weight),
            "step {step}: {turn:?} at {}",
            g.weight
        );
        assert!(close(pitch.value, rise / (level * g.weight)), "{pitch:?}");
        assert_eq!((turn.time, pitch.time), (0.0, 0.0));
    }
    let want = [0.2, 0.4, 0.6, 0.8, 1.0, 1.0, 1.0, 1.0];
    assert!(
        weights.iter().zip(want).all(|(w, x)| close(*w, x)),
        "{weights:?}"
    );
    assert!(close(g.aim_turn, core::f32::consts::FRAC_PI_4));
    assert!(close(g.aim_pitch, rise / level));
    // A still object is looked at half way up.
    let post = Target {
        mobile: false,
        ..creature
    };
    let t = g.update(0.1, &o, Some(&post), None);
    assert!(close(ask(&t.asks, PITCH).value, (0.9 - 1.8) / level));
}

/// The player stops looking at what it looks at once that has been behind it or more than 10 m
/// off (along the ground) for more than 2 seconds; another body keeps looking.
///
/// Behaviour: desktop.bodies.the-player-stops-looking-at-what-is-long-behind-it
#[test]
fn the_player_stops_looking_at_an_object_behind_it_for_more_than_two_seconds() {
    let behind = Target {
        position: at(10.0, 7.0, 0.0),
        height: 1.8,
        mobile: true,
    };
    let far = Target {
        position: at(10.0, 21.0, 0.0),
        ..behind
    };
    for target in [behind, far] {
        let mut g = Gaze::new(1);
        g.look_at(2, 0.5, [0.0; 3]);
        let stopped_at = (1..=12).find(|_| {
            g.update(0.25, &owner(true), Some(&target), None)
                .stop_looking
        });
        assert_eq!(stopped_at, Some(9), "after 2.25 s");
        let mut g = Gaze::new(1);
        g.look_at(2, 0.5, [0.0; 3]);
        assert!((0..40).all(|_| !g
            .update(0.25, &owner(false), Some(&target), None)
            .stop_looking));
    }
    // Behind, then ahead again in time: the count starts over.
    let mut g = Gaze::new(1);
    g.look_at(2, 0.5, [0.0; 3]);
    for _ in 0..7 {
        g.update(0.25, &owner(true), Some(&behind), None);
    }
    let ahead = Target {
        position: at(10.0, 13.0, 0.0),
        ..behind
    };
    g.update(0.25, &owner(true), Some(&ahead), None);
    assert!((0..8).all(|_| !g
        .update(0.25, &owner(true), Some(&behind), None)
        .stop_looking));
}

/// Released, the look blends back out over half a second from the time it held at: the weight
/// falls by a fifth each tenth of a second, and the head follows the camera (the player) or the
/// direction last sent (another body, once).
///
/// Behaviour: desktop.bodies.a-released-look-blends-back-out
#[test]
fn released_the_look_blends_back_out_over_half_a_second() {
    let creature = Target {
        position: at(7.0, 13.0, 0.0),
        height: 1.8,
        mobile: true,
    };
    let mut g = Gaze::new(1);
    g.look_at(2, 0.5, [0.0; 3]);
    for _ in 0..10 {
        g.update(0.1, &owner(true), Some(&creature), None);
    }
    g.release(0.5);
    let camera = Camera {
        offset: Vec3::new(0.0, -1.0, 0.0),
        mouse_look: false,
    };
    let mut weights = Vec::new();
    for _ in 0..6 {
        let t = g.update(0.1, &owner(true), None, Some(&camera));
        weights.push(g.weight);
        // The camera behind: straight ahead, blended out of the creature's turn.
        assert!(close(
            ask(&t.asks, TURN).value,
            core::f32::consts::FRAC_PI_4 * g.weight
        ));
    }
    let want = [0.8, 0.6, 0.4, 0.2, 0.0, 0.0];
    assert!(
        weights.iter().zip(want).all(|(w, x)| (w - x).abs() < 1e-4),
        "{weights:?}"
    );
}

/// The server's direction for another body turns its head there over half a second, once; a body
/// that is not looking at anything otherwise asks nothing of its sliders.
///
/// Behaviour: desktop.bodies.a-direction-from-the-server-turns-a-head
#[test]
fn a_direction_from_the_server_turns_another_bodys_head_over_half_a_second_once() {
    let mut g = Gaze::new(1);
    let mut o = owner(false);
    assert!(g.update(0.1, &o, None, None).asks.is_empty(), "not looking");
    o.dir = (0.25, -0.6);
    g.look_dir();
    let t = g.update(0.1, &o, None, None);
    assert_eq!(
        t.asks,
        vec![
            SliderAsk {
                slider: PITCH,
                value: 0.25,
                time: 0.5
            },
            SliderAsk {
                slider: TURN,
                value: -0.6,
                time: 0.5
            },
        ]
    );
    assert!(g.update(0.1, &o, None, None).asks.is_empty(), "once");
}

/// With nothing to look at, the player's head follows the camera: straight ahead with the camera
/// behind it (its pitch the camera's height along its offset, the other way), turned the way
/// the camera looks from beside it; in mouse look the camera's direction is the world's, turned
/// into the body's frame. A camera in front turns the head back toward it (the look at camera
/// option); with that option off the head looks no further than a quarter turn, and with the
/// look around option off it looks straight ahead. The slider moves over a tenth of a second, a
/// fifth when it is more than 0.2 off; a larger body turns its head less.
///
/// Behaviour: desktop.bodies.the-players-head-follows-the-camera
#[test]
fn the_players_head_follows_the_camera_with_its_options() {
    let o = owner(true);
    let turned = |g: &mut Gaze, o: &Owner, offset: Vec3, mouse_look: bool| {
        let t = g.update(0.1, o, None, Some(&Camera { offset, mouse_look }));
        (ask(&t.asks, PITCH), ask(&t.asks, TURN))
    };
    // Past the first look's blend (the weight back to nothing).
    let mut g = Gaze::new(1);
    turned(&mut g, &o, Vec3::new(0.0, -1.0, 0.0), false);
    g.update(2.0, &o, None, None);
    assert_eq!(g.weight, 0.0);
    // Behind and a little above: the head looks straight ahead and a little down.
    let (p, t) = turned(&mut g, &o, Vec3::new(0.0, -0.98, 0.2), false);
    assert!(close(p.value, -0.2) && close(t.value, 0.0), "{p:?} {t:?}");
    assert_eq!((p.time, t.time), (0.1, 0.1));
    // To the body's left looking right past it: looking at the camera, the head turns back
    // toward the camera on its left, a quarter turn.
    let (p, t) = turned(&mut g, &o, Vec3::new(-1.0, 0.0, 0.0), false);
    assert!(close(t.value, core::f32::consts::FRAC_PI_2), "{t:?}");
    assert!(close(p.value, 0.0));
    assert_eq!(t.time, 0.2, "more than 0.2 off");
    // Without the look at camera option it turns the way the camera looks, a quarter turn right,
    // and no further.
    g.look_at_camera = false;
    let (_, t) = turned(&mut g, &o, Vec3::new(-1.0, 0.0, 0.0), false);
    assert!(close(t.value, -core::f32::consts::FRAC_PI_2), "{t:?}");
    let (p, t) = turned(&mut g, &o, Vec3::new(0.0, 1.0, 0.3), false);
    assert!(close(t.value, 0.0) && close(p.value, -0.3), "{p:?} {t:?}");
    // In mouse look the offset is the world's: a body facing east sees a camera to the world's
    // south, a little west, looking north and a little east, as looking ahead and to its left.
    let mut east = o;
    auberean_physics::geom::set_heading(
        &mut east.position.frame,
        auberean_physics::geom::Heading::new(90.0),
    );
    let (_, t) = turned(&mut g, &east, Vec3::new(-0.2, -1.0, 0.0), true);
    assert!(
        close(
            t.value,
            core::f32::consts::FRAC_PI_2 - dereth_primitives::num::math::atanf(0.2)
        ),
        "{t:?}"
    );
    // A body twice the size turns its head half as far.
    let mut big = o;
    big.scale = Vec3::new(2.0, 2.0, 2.0);
    let (p, _) = turned(&mut g, &big, Vec3::new(0.0, -0.98, 0.2), false);
    assert!(close(p.value, -0.1), "{p:?}");
    // Looking around off: straight ahead whatever the camera.
    g.look_around = false;
    let (p, t) = turned(&mut g, &o, Vec3::new(-1.0, 0.0, 0.5), false);
    assert_eq!((p.value, t.value), (0.0, 0.0));
}
