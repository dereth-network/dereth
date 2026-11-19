//! The orbit camera: where it stands and looks from its turn, tilt and distance; the mouse, the
//! camera's controls and the wheel moving it; and the body's steering it makes of the movement
//! the controls ask, camera-based and character-based. No clock: each step is handed its seconds.

use auberean_client_contract::control::CameraHeld;
use auberean_client_sim::orbit::{
    pivot_height, MovementMode, OrbitCamera, OrbitInput, OrbitSettings, DEFAULT_DISTANCE,
    MAX_DISTANCE, MIN_DISTANCE, PITCH_MAX, PIVOT_HEIGHT, RECENTRE,
};
use auberean_client_sim::player::Steer;
use dereth_primitives::Vec3;

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

/// Degrees apart, either way round.
fn apart(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

fn camera(movement: MovementMode) -> OrbitCamera {
    OrbitCamera::new(OrbitSettings {
        movement,
        ..OrbitSettings::default()
    })
}

/// The camera stands its distance back from the point it looks at, along the way it looks: level
/// and facing north it is due south of the point, turned to face east it is due west, and tilted
/// to look down it stands above. Holding the look-at-front control turns the view round to the
/// player's front. It looks at a point most of the way up the body.
///
/// Behaviour: desktop.orbit.the-orbit-camera-stands-back-along-the-way-it-looks
#[test]
fn the_orbit_camera_stands_its_distance_back_along_the_way_it_looks() {
    let pivot = Vec3::new(10.0, 20.0, 5.0);
    let mut o = camera(MovementMode::Camera);
    o.pitch = 0.0;
    o.place_behind(0.0);
    let (at, look) = o.eye(pivot);
    assert!(near(at.x, 10.0) && near(at.y, 20.0 - DEFAULT_DISTANCE) && near(at.z, 5.0));
    assert!(near(look.y, 1.0), "looking north: {look:?}");
    o.place_behind(90.0);
    let (at, look) = o.eye(pivot);
    assert!(
        near(at.x, 10.0 - DEFAULT_DISTANCE) && near(at.y, 20.0),
        "west of it: {at:?}"
    );
    assert!(near(look.x, 1.0), "looking east: {look:?}");
    o.pitch = -0.5;
    let (at, look) = o.eye(pivot);
    assert!(
        at.z > pivot.z && look.z < 0.0,
        "above, looking down: {at:?} {look:?}"
    );
    let back = DEFAULT_DISTANCE * dereth_primitives::num::math::cosf(0.5);
    assert!(
        near(pivot.x - at.x, back),
        "its distance back along the look"
    );
    o.step(
        CameraHeld::default(),
        &OrbitInput {
            front_view: true,
            ..OrbitInput::default()
        },
        &Steer::default(),
        90.0,
        0.0,
    );
    assert!(near(o.view_heading(), 270.0), "turned round to the front");
    let (at, _) = o.eye(pivot);
    assert!(at.x > pivot.x, "on the player's east, before it");
    assert!(near(o.heading, 90.0), "its own way kept for the movement");
    assert!(near(pivot_height(2.0), 1.7) && near(pivot_height(0.0), PIVOT_HEIGHT));
}

/// The mouse turns the camera only while a mouse button is held: right looks further right, away
/// from the player looks further up; reversed, each goes the other way. The camera's rotate and
/// tilt controls turn and tilt it at their rate, the tilt held to its range; the wheel brings it
/// closer and further, never past its nearest and farthest.
///
/// Behaviour: desktop.orbit.the-mouse-the-controls-and-the-wheel-move-the-orbit-camera
#[test]
fn the_mouse_turns_it_while_a_button_is_held_and_the_controls_and_wheel_move_it() {
    let mut o = camera(MovementMode::Camera);
    o.place_behind(0.0);
    let pitch = o.pitch;
    let drag = |buttons| OrbitInput {
        mouse: (100.0, -40.0),
        buttons,
        ..OrbitInput::default()
    };
    o.step(
        CameraHeld::default(),
        &drag((false, false)),
        &Steer::default(),
        0.0,
        0.0,
    );
    assert!(
        near(o.heading, 0.0) && near(o.pitch, pitch),
        "no button: unmoved"
    );
    o.step(
        CameraHeld::default(),
        &drag((true, false)),
        &Steer::default(),
        0.0,
        0.0,
    );
    let turned = (100.0 * OrbitSettings::default().mouse_turn).to_degrees();
    assert!(near(o.heading, turned), "right looks right: {}", o.heading);
    assert!(o.pitch > pitch, "away from the player looks up");
    let mut r = OrbitCamera::new(OrbitSettings {
        reverse_x: true,
        reverse_y: true,
        ..OrbitSettings::default()
    });
    r.place_behind(0.0);
    r.step(
        CameraHeld::default(),
        &drag((false, true)),
        &Steer::default(),
        0.0,
        0.0,
    );
    assert!(
        near(r.heading, 360.0 - turned),
        "reversed: left: {}",
        r.heading
    );
    assert!(r.pitch < pitch, "reversed: down");

    let mut k = camera(MovementMode::Camera);
    k.place_behind(0.0);
    let held = CameraHeld {
        rotate_left: true,
        tilt_up: true,
        ..CameraHeld::default()
    };
    k.step(held, &OrbitInput::default(), &Steer::default(), 0.0, 0.5);
    let by = (0.5 * OrbitSettings::default().key_turn).to_degrees();
    assert!(
        near(k.heading, 360.0 - by),
        "rotate left looks left: {}",
        k.heading
    );
    for _ in 0..10 {
        k.step(held, &OrbitInput::default(), &Steer::default(), 0.0, 0.5);
    }
    assert!(near(k.pitch, PITCH_MAX), "tilted only so far: {}", k.pitch);

    let wheel = |n| OrbitInput {
        zoom: n,
        ..OrbitInput::default()
    };
    k.step(
        CameraHeld::default(),
        &wheel(2),
        &Steer::default(),
        0.0,
        0.0,
    );
    assert!(near(k.distance, DEFAULT_DISTANCE - 2.0), "two steps closer");
    k.step(
        CameraHeld::default(),
        &wheel(50),
        &Steer::default(),
        0.0,
        0.0,
    );
    assert!(near(k.distance, MIN_DISTANCE));
    k.step(
        CameraHeld::default(),
        &wheel(-50),
        &Steer::default(),
        0.0,
        0.0,
    );
    assert!(near(k.distance, MAX_DISTANCE));
}

/// Camera-based, the body runs the way the movement controls point against where the camera
/// looks: forward its way, backward turned round toward the camera, a turn or a strafe across its
/// view, forward and a strafe between; always forward, at the pace asked (Shift's walk kept), and
/// no turn of its own. With nothing asked it stands as it faces; while the server moves it to a
/// place the camera does not turn it.
///
/// Behaviour: desktop.orbit.camera-based-movement-runs-the-way-the-controls-point
#[test]
fn camera_based_movement_runs_the_way_the_controls_point_against_the_camera() {
    let mut o = camera(MovementMode::Camera);
    o.place_behind(90.0);
    let none = OrbitInput::default();
    let ask = |forward: f32, sideways: f32, turn: f32| Steer {
        forward,
        sideways,
        turn,
        ..Steer::default()
    };
    let way = |s: Steer| {
        assert!(
            s.camera_steers && s.turn == 0.0 && s.sideways == 0.0,
            "{s:?}"
        );
        s.heading.expect("a heading")
    };
    assert!(near(way(o.steer(ask(1.0, 0.0, 0.0), &none, false)), 90.0));
    assert!(near(way(o.steer(ask(-1.0, 0.0, 0.0), &none, false)), 270.0));
    assert!(
        near(way(o.steer(ask(0.0, 0.0, 1.0), &none, false)), 0.0),
        "turn left"
    );
    assert!(
        near(way(o.steer(ask(0.0, 1.0, 0.0), &none, false)), 180.0),
        "strafe right"
    );
    assert!(near(way(o.steer(ask(1.0, -1.0, 0.0), &none, false)), 45.0));
    let walk = o.steer(ask(0.339, 0.0, 0.0), &none, false);
    assert!(near(walk.forward, 0.339), "the pace asked");
    let back = o.steer(ask(-1.0, 0.0, 0.0), &none, false);
    assert!(near(back.forward, 1.0), "runs forward, turned round");
    let still = o.steer(ask(0.0, 0.0, 0.0), &none, false);
    assert!(still.heading.is_none() && still.forward == 0.0 && !still.camera_steers);
    let led = o.steer(ask(1.0, 0.0, 0.0), &none, true);
    assert!(led.heading.is_none(), "the server's move keeps its heading");
}

/// Character-based, the movement is the game's own (forward and back, the turns turn, the strafes
/// step), unless the right button is held: then the body faces where the camera looks and the
/// turns step sideways. Moving or turning, with no button held, brings the camera round behind
/// the player in the recentre time, never past; standing still leaves it where it is.
///
/// Behaviour: desktop.orbit.character-based-movement-is-the-games-own
#[test]
fn character_based_movement_is_the_games_own_and_brings_the_camera_behind() {
    let mut o = camera(MovementMode::Character);
    o.place_behind(0.0);
    o.heading = 90.0;
    let none = OrbitInput::default();
    let asked = Steer {
        forward: 1.0,
        turn: 1.0,
        sideways: 0.0,
        ..Steer::default()
    };
    let s = o.steer(asked, &none, false);
    assert_eq!(s, asked, "as the game takes it");
    let right = OrbitInput {
        buttons: (false, true),
        ..OrbitInput::default()
    };
    let s = o.steer(asked, &right, false);
    assert_eq!(s.heading, Some(90.0), "faces the camera's way");
    assert!(
        s.camera_steers && s.turn == 0.0 && near(s.sideways, -1.0),
        "{s:?}"
    );

    let mut standing = o;
    standing.step(CameraHeld::default(), &none, &Steer::default(), 0.0, 1.0);
    assert!(
        near(standing.heading, 90.0),
        "standing still: left where it is"
    );
    let mut last = apart(o.heading, 0.0);
    let frames = 12;
    for _ in 0..frames {
        o.step(CameraHeld::default(), &none, &asked, 0.0, RECENTRE / 12.0);
        let now = apart(o.heading, 0.0);
        assert!(
            now <= last + 1e-4,
            "never swings back out: {last} then {now}"
        );
        assert!(o.heading < 180.0, "never past behind: {}", o.heading);
        last = now;
    }
    assert!(
        last < 1.0,
        "behind in the recentre time: {last} degrees off"
    );
    let mut held = camera(MovementMode::Character);
    held.place_behind(0.0);
    held.heading = 90.0;
    held.step(CameraHeld::default(), &right, &asked, 0.0, RECENTRE);
    assert!(
        near(held.heading, 90.0),
        "a button held keeps it where it is"
    );
}
