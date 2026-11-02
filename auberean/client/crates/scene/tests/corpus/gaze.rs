//! Where the bodies' heads turn in recorded 2017 sessions: to the direction the server sends for
//! each other body, and toward what the server says a body looks at (the player's selections,
//! echoed back, among them).
//! Fixture: the end-of-retail install and the capture index.

use std::collections::BTreeMap;

use auberean_scene::gaze::{PITCH, TURN};
use dereth_primitives::num::math;

use crate::combat::{played_in_frames, FRAME};

/// A session in which the server sends the heads of the players about the player and says what
/// they look at.
const HEADS: &str = "a30caddd9c7285fe:1";
/// A session in which the player selects creatures and objects, the server echoing each look.
const SELECTS: &str = "d4f9461ff3dbee1a:1";

/// Within this distance of the player a body animates, so its sliders move.
const NEAR: f32 = 60.0;

fn near(a: &dereth_primitives::Position, b: &dereth_primitives::Position) -> bool {
    auberean_physics::geom::distance(a, b) < NEAR
}

/// Each other body's head turns to the pitch and turn the server last sent for it: two thirds of
/// a second after a direction arrived (with no newer one, and not looking at an object since),
/// the look sliders of every such body near the player stand at it (within the 0.0002 a slider
/// is let rest off a value it is asked for).
///
/// Behaviour: desktop.bodies.recorded-heads-turn-to-the-servers-directions
#[test]
fn other_bodies_heads_turn_to_the_direction_the_server_sends() {
    // Each body's last direction, how many frames it has stood, and whether the body has looked
    // at an object since it came (a released look leaves the head where the object was).
    let mut held: BTreeMap<u64, ((f32, f32), u32, bool)> = BTreeMap::new();
    let (mut checked, mut off) = (0usize, Vec::new());
    let (c, _, _) = played_in_frames(HEADS, 240.0, FRAME, &[], &mut |c, scene| {
        let Some(player) = c
            .model
            .player
            .and_then(|p| scene.body(p).map(|b| (p, b.position)))
        else {
            return;
        };
        for (&id, l) in scene.looks() {
            if id == player.0 || l.gaze.is_none() {
                continue;
            }
            let e = held.entry(id).or_insert((l.dir, 0, false));
            if e.0 == l.dir {
                e.1 += 1;
            } else {
                *e = (l.dir, 0, false);
            }
            e.2 |= l.target != 0;
            if e.2 {
                continue;
            }
            if e.1 != 20 {
                continue;
            }
            let (Some(b), Some(f)) = (scene.body(id), scene.figure(id)) else {
                continue;
            };
            if !near(&b.position, &player.1) {
                continue;
            }
            checked += 1;
            let now = (f.sliders.current(PITCH), f.sliders.current(TURN));
            if (now.0 - l.dir.0).abs() > 2e-4 || (now.1 - l.dir.1).abs() > 2e-4 {
                off.push((id, l.dir, now));
            }
        }
    });
    let sent = c
        .model
        .objects()
        .filter(|o| held.contains_key(&o.id))
        .count();
    eprintln!("{checked} directions held near the player, {sent} bodies sent one; off: {off:?}");
    assert!(checked >= 30, "{checked} checked");
    assert!(off.is_empty(), "{off:?}");
}

/// A body the server says looks at an object turns its head toward it: two thirds of a second
/// after it was told (while it still looks at it), its turn slider is within a tenth of a radian
/// of the object's bearing from straight ahead (one behind it taken as beside it), for the
/// player (whose selections the server echoes, some 50 m off) and for the others.
///
/// Behaviour: desktop.bodies.recorded-looks-turn-heads-toward-their-objects
#[test]
fn a_body_turns_its_head_toward_what_the_server_says_it_looks_at() {
    let (mut players, mut others, mut off) = (0usize, 0usize, Vec::new());
    for session in [HEADS, SELECTS] {
        let mut held: BTreeMap<u64, (u64, u32)> = BTreeMap::new();
        played_in_frames(session, 480.0, FRAME, &[], &mut |c, scene| {
            let player = c.model.player;
            for (&id, l) in scene.looks() {
                let e = held.entry(id).or_insert((l.target, 0));
                if e.0 == l.target {
                    e.1 += 1;
                } else {
                    *e = (l.target, 0);
                }
                if l.target == 0 || e.1 != 20 {
                    continue;
                }
                let (Some(b), Some(t), Some(f)) =
                    (scene.body(id), scene.body(l.target), scene.figure(id))
                else {
                    continue;
                };
                if !l.gaze.as_ref().is_some_and(|g| g.on) {
                    continue;
                }
                let v = auberean_physics::geom::offset(&b.position, &t.position);
                let local = auberean_physics::geom::global_to_local(&b.position.frame, v);
                if math::hypotf(local.x, local.y) < 0.5 {
                    continue;
                }
                // An object behind is taken as beside it.
                let bearing =
                    math::atan2f(local.y.max(0.0), local.x) - core::f32::consts::FRAC_PI_2;
                let turn = f.sliders.current(TURN);
                if Some(id) == player {
                    players += 1;
                } else {
                    others += 1;
                }
                if (turn - bearing).abs() > 0.1 {
                    off.push((id, l.target, bearing, turn));
                }
            }
        });
    }
    eprintln!("{players} of the player's looks, {others} of others'; off: {off:?}");
    assert!(players >= 3 && others >= 3, "{players} and {others}");
    assert!(off.is_empty(), "{off:?}");
}

/// The player's head follows the camera from the moment the player arrives, with nothing looked
/// at yet: five seconds in, the chase camera behind and above it has the head tipped down by the
/// camera's tilt (over the body's height scale) and turned straight ahead, and the client holds where the head points for the
/// server. With the option to look around off the head stays level and straight; the option to
/// look at the selection reaches the client.
///
/// Behaviour: desktop.bodies.the-players-head-follows-the-camera-from-its-arrival
#[test]
fn the_players_head_follows_the_camera_from_its_arrival_as_the_options_say() {
    for look_around in [true, false] {
        let mut arrived: Option<u32> = None;
        let mut seen = None;
        played_in_frames(HEADS, 20.0, FRAME, &[], &mut |c, scene| {
            scene.camera.options.look_around = look_around;
            scene.camera.options.look_at_selection = look_around;
            let Some(p) = c.model.player.filter(|p| scene.figure(*p).is_some()) else {
                return;
            };
            let n = arrived.get_or_insert(0);
            *n += 1;
            if *n != 150 {
                return;
            }
            let l = &scene.looks()[&p];
            assert_eq!(l.target, 0, "nothing looked at yet");
            let g = l
                .gaze
                .as_ref()
                .expect("the player's gaze, from its arrival");
            assert_eq!(g.look_around, look_around);
            let f = scene.figure(p).expect("the figure");
            let head = (f.sliders.current(PITCH), f.sliders.current(TURN));
            assert_eq!(c.head, Some(head));
            assert_eq!(c.look_at_selection, look_around);
            let scale = scene.body(p).expect("the body").total_scale(None).z;
            seen = Some((head, scale));
        });
        let ((pitch, turn), scale) = seen.expect("the player arrived");
        eprintln!("look around {look_around}: pitch {pitch}, turn {turn}");
        if look_around {
            let tilt = auberean_scene::camera::Options::default().tilt;
            assert!((pitch + tilt / scale).abs() < 0.02, "{pitch} {scale}");
            assert!(turn.abs() < 0.01, "{turn}");
        } else {
            assert!(pitch.abs() < 1e-4 && turn.abs() < 1e-4, "{pitch} {turn}");
        }
    }
}
