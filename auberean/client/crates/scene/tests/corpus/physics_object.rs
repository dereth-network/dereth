//! An object's physics object as its scripts reach it, in a recorded 2017 session: what they read
//! of each body, and what they ask of it carried out on the body.
//! Fixture: the end-of-retail install and the capture index.

use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::protocol::ServerMessage;
use auberean_client_sdk::runtime::physics_object::BodyRequest;
use auberean_scene::{Intent, Scene};
use auberean_testkit::captures;

use crate::combat::{played_in_frames, FRAME};

/// The session in which the server scales the player's speed and acceleration (2.185 times) and
/// the creatures' about it.
const SCALED: &str = "9e56c155870fcf8d:1";

/// What the scripts read of a body, as its physics object answers: its radius and height are its
/// setup's by its look's height scale and its own (the creatures' own scales run 0.85 to 1.2);
/// its mode and the mode it is going to are both the mode its figure was last asked to go to.
fn expected_size(scene: &Scene<'_>, id: u64) -> Option<(f32, f32)> {
    let b = scene.body(id)?;
    let setup = &scene.figure(id)?.parts.first()?.setup;
    let k = b.look_scale.z * b.object_scale.z;
    Some((setup.radius * k, setup.height * k))
}

/// A minute in the world of a 2017 session in which the server scales the speed and acceleration
/// of the player and of the creatures about it: after each frame the scripts read, of every body,
/// the scales the server last sent for it (the player's 2.185, as the 2017 client's own scripts
/// read it), its jump scale, its radius and height (its setup's by its look's height scale and its
/// own, so a creature scaled by its qualities measures by that too), the mode its figure was last
/// asked to go to whether they ask for its mode or the mode it is going to, and the motion the
/// server last sent for it while it follows the server's positions.
///
/// Behaviour: desktop.bodies.the-scripts-read-each-bodys-physics-object
#[test]
fn the_scripts_read_each_bodys_physics_object() {
    const SECONDS: f64 = 60.0;
    let index = auberean_testkit::data::require_capture_index();
    // The last speed and acceleration scale sent for each object while the replay plays.
    let mut sent = std::collections::BTreeMap::<u64, [Option<f32>; 2]>::new();
    let mut entered = None;
    captures::capture_messages_in_time_order(&index, &[0x10060, 0x10092, 0x10093], SCALED, |m| {
        if entered.is_some_and(|e| m.time > e + SECONDS) {
            return;
        }
        match ServerMessage::decode(&m.raw) {
            Ok(ServerMessage::PlayerObject(_)) => {
                entered.get_or_insert(m.time);
            }
            Ok(ServerMessage::VelocityScale(e)) => {
                sent.entry(e.sender.id).or_default()[0] = Some(e.action.0);
            }
            Ok(ServerMessage::AccelerationScale(e)) => {
                sent.entry(e.sender.id).or_default()[1] = Some(e.action.0);
            }
            _ => {}
        }
    })
    .expect("the index reads");
    // The frames in which some body followed the server's positions, as the scripts read it.
    let mut followed = 0usize;
    let (c, scene, _) = played_in_frames(SCALED, SECONDS, FRAME, &[], &mut |c, _| {
        let readings = &c.scripts.as_ref().expect("the scripts run").host.readings;
        followed += usize::from(readings.values().any(|r| r.remote_motion.is_some()));
    });
    let materials = &c.scripts.as_ref().expect("the scripts run").host.materials;
    let player_material = materials
        .get(&c.model.player.expect("the player"))
        .expect("the player's body stands in a cell");
    assert_eq!(
        Some(&player_material.properties),
        scene
            .body(c.model.player.expect("the player"))
            .and_then(|b| b.current_properties()),
        "the scripts read what the cell made of the body"
    );
    let readings = &c.scripts.as_ref().expect("the scripts run").host.readings;
    let player = c.model.player.expect("the player");
    let (mut bodies, mut own_scaled, mut moded, mut following, mut scaled_read) = (0, 0, 0, 0, 0);
    for (&id, r) in readings {
        let Some(b) = scene.body(id) else {
            continue;
        };
        bodies += 1;
        assert_eq!(r.element, b.element(), "{id:#x}");
        assert_eq!(
            [r.velocity_scale, r.acceleration_scale, r.jump_scale],
            [b.velocity_scale(), b.acceleration_scale(), b.jump_scale()],
            "{id:#x}"
        );
        if let Some([v, a]) = sent.get(&id) {
            if let Some(v) = v {
                assert!((r.velocity_scale - v).abs() < 1e-6, "{id:#x}: {r:?}");
                scaled_read += 1;
            }
            if let Some(a) = a {
                assert!((r.acceleration_scale - a).abs() < 1e-6, "{id:#x}: {r:?}");
            }
        }
        if let Some((radius, height)) = expected_size(&scene, id) {
            assert!(
                (r.radius - radius).abs() < 1e-5 && (r.height - height).abs() < 1e-5,
                "{id:#x}: read {} {}, its setup's {radius} {height}",
                r.radius,
                r.height
            );
            own_scaled += usize::from((b.object_scale.z - 1.0).abs() > 1e-3);
        }
        match scene.figure(id) {
            Some(f) => {
                assert_eq!(r.mode, f.modes.desired_mode, "{id:#x}");
                moded += usize::from(r.mode != 0);
            }
            None => assert_eq!(r.mode, 0, "{id:#x}"),
        }
        assert_eq!(r.desired_mode, r.mode, "{id:#x}");
        match r.remote_motion {
            Some(m) => {
                let v = b.remote_do_motion_values();
                assert_eq!(m, [v.x, v.y, v.z], "{id:#x}");
                following += 1;
            }
            None => assert!(!b.has_remote_do_motion_values(), "{id:#x}"),
        }
    }
    eprintln!(
        "{bodies} bodies read: {scaled_read} with a speed scale sent, {own_scaled} scaled by \
         their own scale, {moded} in a mode, {following} following the server at the end, \
         some in {followed} frames"
    );
    let p = &readings[&player];
    assert!(
        (p.velocity_scale - 2.185).abs() < 1e-3,
        "the player's speed scale: {p:?}"
    );
    assert!(p.radius > 0.1 && p.height > 1.0, "the player's size: {p:?}");
    assert!(
        bodies >= 10 && scaled_read >= 5,
        "{bodies} bodies, {scaled_read} scaled"
    );
    assert!(own_scaled >= 3, "{own_scaled} bodies of their own scale");
    assert!(moded >= bodies / 2, "{moded} of {bodies} in a mode");
    assert!(
        followed >= 100,
        "{followed} frames with a body following the server"
    );
}

/// In a recorded session, what the scripts ask of a creature's body is carried out the next
/// frame, in order: its speed and acceleration scales; drawn cloaked (so not drawn at all);
/// reporting no collisions and ignoring them; a push in its own frame, which puts it in the air
/// moving that way; a push away from the player, along the line from the player to it; a layer
/// of its animation stopped. A push away from itself, or a request for an object with no body,
/// does nothing.
///
/// Behaviour: desktop.bodies.what-the-scripts-ask-of-a-body-is-carried-out
#[test]
fn what_the_scripts_ask_of_a_body_is_carried_out() {
    let (mut c, mut scene, _) =
        played_in_frames("1e177f6bfede2082:3", 40.0, FRAME, &[], &mut |_, _| {});
    let player = c.model.player.expect("the player");
    let creature = c
        .model
        .objects()
        .map(|o| o.id)
        .filter(|&id| id != player && scene.figure(id).is_some())
        .find(|&id| {
            scene
                .body(id)
                .is_some_and(|b| b.in_world && !b.is_no_draw())
        })
        .expect("a creature in the world");
    let mut t = c.local_time();
    scene.frame(t);
    assert!(
        scene.seen.bodies.iter().any(|s| s.id == creature),
        "drawn before"
    );
    let ask = |c: &mut auberean_client_sdk::Client, rs: &[BodyRequest]| {
        c.scripts
            .as_mut()
            .expect("the scripts run")
            .host
            .body_requests
            .extend_from_slice(rs);
    };
    let frame = |c: &mut auberean_client_sdk::Client, scene: &mut Scene<'_>, t: &mut f64| {
        *t += FRAME;
        c.set_time(LocalTime(*t));
        #[allow(clippy::cast_possible_truncation)]
        scene.update(c, &Intent::default(), *t, FRAME as f32);
    };
    ask(
        &mut c,
        &[
            BodyRequest::VelocityScale {
                object: creature,
                scale: 1.7,
            },
            BodyRequest::AccelerationScale {
                object: creature,
                scale: 0.6,
            },
            BodyRequest::Cloaked {
                object: creature,
                on: true,
            },
            BodyRequest::ReportCollisions {
                object: creature,
                on: false,
            },
            BodyRequest::IgnoreCollisions {
                object: creature,
                on: true,
            },
            BodyRequest::Impulse {
                object: creature,
                vector: [0.0, 2.0, 3.0],
                local: true,
                flags: 0,
            },
            BodyRequest::EndLayer {
                object: creature,
                layer: 1,
            },
            BodyRequest::VelocityScale {
                object: 0x7FFF_FFFF_0000_0001,
                scale: 9.0,
            },
        ],
    );
    frame(&mut c, &mut scene, &mut t);
    let b = scene.body(creature).expect("its body");
    assert!((b.velocity_scale() - 1.7).abs() < 1e-6);
    assert!((b.acceleration_scale() - 0.6).abs() < 1e-6);
    assert!(b.is_cloaked() && b.is_no_draw(), "cloaked, so not drawn");
    assert!(!b.reports_collisions() && b.ignores_collisions());
    // Its own frame's (0, 2, 3) in the world's: 2 m/s ahead of it, 3 up.
    let ahead = auberean_physics::geom::local_to_global(
        &b.position.frame,
        dereth_primitives::Vec3::new(0.0, 2.0, 3.0),
    );
    let v = b.velocity;
    assert!(
        (v.x - ahead.x).abs() < 1e-4 && (v.y - ahead.y).abs() < 1e-4 && (v.z - 3.0).abs() < 1e-4,
        "{v:?} for {ahead:?}"
    );
    assert!(!b.contact, "in the air");
    assert_eq!(b.desired_element(), auberean_animation::body::AIR);
    assert_eq!(
        b.element(),
        auberean_animation::body::EARTH,
        "its figure is not there yet"
    );
    let readings = &c.scripts.as_ref().expect("the scripts run").host.readings;
    assert!(
        (readings[&creature].velocity_scale - 1.7).abs() < 1e-6,
        "the scripts read it back"
    );
    scene.frame(t);
    assert!(
        !scene.seen.bodies.iter().any(|s| s.id == creature),
        "not drawn"
    );
    // Away from the player: 4 m/s along the line from the player to it, 1 up (the push is
    // carried out after the frame's ticks, from where both then stand).
    ask(
        &mut c,
        &[BodyRequest::TargetedImpulse {
            object: creature,
            target: player,
            speed: 4.0,
            flags: 0,
            extra: 1.0,
        }],
    );
    frame(&mut c, &mut scene, &mut t);
    let me = scene.body(player).expect("the player's body").position;
    let it = scene.body(creature).expect("its body");
    let way = auberean_physics::geom::offset(&me, &it.position);
    let len = (way.x * way.x + way.y * way.y + way.z * way.z).sqrt();
    let v = it.velocity;
    let want = [
        way.x / len * 4.0,
        way.y / len * 4.0,
        way.z / len * 4.0 + 1.0,
    ];
    assert!(
        (v.x - want[0]).abs() < 1e-3
            && (v.y - want[1]).abs() < 1e-3
            && (v.z - want[2]).abs() < 1e-3,
        "{v:?} for {want:?}"
    );
    // Away from itself: no push (it only falls, if it ticks).
    let before = v;
    ask(
        &mut c,
        &[BodyRequest::TargetedImpulse {
            object: creature,
            target: creature,
            speed: 4.0,
            flags: 0,
            extra: 1.0,
        }],
    );
    frame(&mut c, &mut scene, &mut t);
    let after = scene.body(creature).expect("its body").velocity;
    assert!(
        after.z <= before.z + 1e-6 && (after.x - before.x).abs() < 1e-3,
        "{after:?} from {before:?}"
    );
    assert!(c
        .scripts
        .as_ref()
        .expect("the scripts run")
        .host
        .body_requests
        .is_empty());
}

/// The scripts stop the camera's effects as a played scene ends: what plays on the camera ends
/// with the next frame.
///
/// Behaviour: desktop.camera.the-scripts-stop-the-cameras-effects
#[test]
fn the_scripts_stop_the_cameras_effects() {
    let fx = auberean_assets::fx::CameraFx {
        shake: Some(true),
        period: Some(600.0),
        ..auberean_assets::fx::CameraFx::default()
    };
    let (mut frames, mut stood) = (0, false);
    let (_, scene, _) = played_in_frames(SCALED, 20.0, FRAME, &[], &mut |c, scene| {
        let Some(player) = c.model.player.filter(|p| scene.body(*p).is_some()) else {
            return;
        };
        frames += 1;
        if frames == 10 {
            let objects = auberean_scene::camera::NoObjects;
            scene.camera.do_camera_fx(&objects, &fx, 1.0, player);
        }
        if frames == 20 {
            stood = !scene.camera.effects.tracks.is_empty();
            let scripts = c.scripts.as_mut().expect("the scripts run");
            scripts.host.camera_fx_stops += 1;
        }
    });
    assert!(stood, "it plays until asked");
    assert!(scene.camera.effects.tracks.is_empty(), "the ask ended it");
}

/// The server's scripts freeze a player: its body's movement and animation are frozen as they say,
/// and while its animation is frozen its figure stands still, a slider under way waiting with it.
/// Thawed, both go on.
///
/// Behaviour: desktop.bodies.a-frozen-body-and-figure-stand-still
#[test]
fn a_frozen_body_and_figure_stand_still() {
    use auberean_physics::body::parent::FADE_SLIDER;
    // Well after it came, when the world no longer waits for its cells.
    let mut frames = 0;
    let (mut started, mut frozen, mut thawed) = (None, None, None);
    let (mut movement, mut animation, mut awake) = (false, false, false);
    played_in_frames(SCALED, 20.0, FRAME, &[], &mut |c, scene| {
        let Some(player) = c.model.player.filter(|p| scene.body(*p).is_some()) else {
            return;
        };
        let fade = |scene: &Scene<'_>| scene.figure(player).map(|f| f.sliders.current(FADE_SLIDER));
        frames += 1;
        if frames == 300 {
            let scripts = c.scripts.as_mut().expect("the scripts run");
            scripts.host.frozen.insert(player, (true, true));
        }
        if frames == 301 {
            let b = scene.body(player).expect("its body");
            movement = b.is_movement_frozen();
            animation = b.is_animation_frozen();
            if let Some(f) = scene.figure_mut(player) {
                f.sliders.do_slider(0, FADE_SLIDER, 0.0, 600.0);
            }
            started = fade(scene);
        }
        if frames == 331 {
            frozen = fade(scene);
            let scripts = c.scripts.as_mut().expect("the scripts run");
            scripts.host.frozen.insert(player, (false, false));
        }
        if frames == 361 {
            thawed = fade(scene);
            awake = scene.body(player).is_some_and(|b| b.is_movement_active());
        }
    });
    assert!(movement && animation, "the body takes the scripts' word");
    let (Some(started), Some(frozen), Some(thawed)) = (started, frozen, thawed) else {
        panic!("the player stayed in the world: {started:?} {frozen:?} {thawed:?}");
    };
    assert_eq!(started, frozen, "the slider waits while frozen");
    assert_ne!(frozen, thawed, "and goes on once thawed");
    assert!(awake, "thawed, the body's updates run again");
}
