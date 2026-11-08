//! The camera under the player's control, with no world: the held keys, the wheel, the views,
//! mouse look, where it stands about the player, the shoulder, the glide between views, the auto
//! distance, the slope, the effects, and the cameras the effects and animations ask for.

use auberean_scene::camera::{
    CameraControl, Effect, Facts, Held, Objects, Options, Press, Subject, View, Viewer,
    MAP_DISTANCE, PITCH_LIMIT, SHOULDER_DISTANCE, STEEP_PITCH,
};
use dereth_primitives::num::math;
use dereth_primitives::Vec3;

fn close(a: f32, b: f32, eps: f32) -> bool {
    (a - b).abs() <= eps
}

fn closev(a: Vec3, b: Vec3, eps: f32) -> bool {
    close(a.x, b.x, eps) && close(a.y, b.y, eps) && close(a.z, b.z, eps)
}

/// A player of the human races' height at the origin, facing north.
fn player() -> Subject {
    Subject {
        origin: Vec3::ZERO,
        heading: 0.0,
        height: 2.06,
        radius: 0.6,
        scale_z: 1.0,
        local_velocity: Vec3::ZERO,
    }
}

/// The player's id.
const PLAYER: u64 = 8;

/// A camera with `options` following [`player`].
fn camera_with(options: Options) -> CameraControl {
    let mut c = CameraControl::new(options);
    c.set_player(&player(), PLAYER, player().height);
    c
}

/// A camera following [`player`].
fn camera() -> CameraControl {
    camera_with(Options::default())
}

/// Holds `h` for `seconds` of 10 ms frames.
fn hold(c: &mut CameraControl, h: Held, seconds: f32) {
    c.hold(h);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (seconds / 0.01).round() as usize;
    for _ in 0..frames {
        c.update(0.01, &player());
    }
}

/// Held keys turn the camera a radian a second (five with Shift), tilt it half a radian a second
/// (one with Shift), and zoom it by 0.75 a second in and 1.5 out (0.5625 and 2.25 with Shift).
///
/// Behaviour: desktop.camera.held-keys-rotate-tilt-and-zoom-at-the-clients-rates
#[test]
fn held_keys_rotate_tilt_and_zoom_at_the_clients_rates() {
    let mut c = camera();
    assert!(close(c.distance, 4.5, 1e-6) && close(c.pitch(), 0.25, 1e-6) && c.yaw() == 0.0);
    let left = Held {
        rotate_left: true,
        ..Held::default()
    };
    hold(&mut c, left, 1.0);
    assert!(
        close(c.yaw(), 1.0, 1e-3),
        "a radian in a second: {}",
        c.yaw()
    );
    let right_fast = Held {
        rotate_right: true,
        fast: true,
        ..Held::default()
    };
    hold(&mut c, right_fast, 0.2);
    assert!(
        close(c.yaw(), 0.0, 1e-3),
        "five a second with Shift: {}",
        c.yaw()
    );
    let up = Held {
        tilt_up: true,
        ..Held::default()
    };
    hold(&mut c, up, 1.0);
    assert!(close(c.pitch(), 0.75, 1e-3), "half a radian: {}", c.pitch());
    let down_fast = Held {
        tilt_down: true,
        fast: true,
        ..Held::default()
    };
    hold(&mut c, down_fast, 0.5);
    assert!(
        close(c.pitch(), 0.25, 1e-3),
        "one with Shift: {}",
        c.pitch()
    );
    let zoom_in = Held {
        zoom_in: true,
        ..Held::default()
    };
    hold(&mut c, zoom_in, 1.0);
    assert!(close(c.distance, 4.5 * 0.75, 1e-3), "{}", c.distance);
    let zoom_out_fast = Held {
        zoom_out: true,
        fast: true,
        ..Held::default()
    };
    hold(&mut c, zoom_out_fast, 1.0);
    assert!(close(c.distance, 4.5 * 0.75 * 2.25, 1e-2), "{}", c.distance);
    hold(&mut c, Held::default(), 0.5);
    assert!(
        close(c.distance, 4.5 * 0.75 * 2.25, 1e-2),
        "let go, it stops"
    );
    hold(&mut c, up, 10.0);
    assert!(
        close(c.pitch(), PITCH_LIMIT, 1e-6),
        "the pitch stops at its limit"
    );
}

/// Each notch of the wheel moves the camera 1.5 m, never nearer than 0.11 m nor farther than
/// 160 m; the reset puts it behind at the default distance and tilt.
///
/// Behaviour: desktop.camera.the-wheel-steps-and-the-reset-returns-behind
#[test]
fn the_wheel_steps_and_the_reset_returns_behind() {
    let mut c = camera();
    c.press(Press::Wheel(1));
    assert!(close(c.distance, 3.0, 1e-5));
    c.press(Press::Wheel(-3));
    assert!(close(c.distance, 7.5, 1e-5));
    c.press(Press::Wheel(10));
    assert!(close(c.distance, 0.11, 1e-6), "the nearest: {}", c.distance);
    c.press(Press::Wheel(-1000));
    assert!(
        close(c.distance, 160.0, 1e-4),
        "the farthest: {}",
        c.distance
    );
    c.offset = Vec3::new(0.7, 0.7, -0.5);
    c.press(Press::Reset);
    assert_eq!((c.yaw(), c.view()), (0.0, View::Behind));
    assert!(close(c.distance, 4.5, 1e-6) && close(c.pitch(), 0.25, 1e-6));
}

/// The keypad's views: shoulder 1.3 m behind, first person at the eyes, looking at the ground and
/// the map view pitched 1.37 radians (the map view 160 m out), looking at oneself from in front
/// and level, swinging out to about 67 degrees to a side pitched 0.595 radians; the same key again
/// goes back behind, and the map view gives back the camera it replaced. In first person the wheel
/// and rotating do nothing; in mouse look the shoulder, the look at oneself and the swings do
/// nothing.
///
/// Behaviour: desktop.camera.the-keypad-views-toggle-as-the-client-does
#[test]
fn the_keypad_views_toggle_as_the_client_does() {
    let mut c = camera();
    c.press(Press::Shoulder);
    assert_eq!(c.view(), View::Shoulder);
    assert!(close(c.distance, SHOULDER_DISTANCE, 1e-6));
    c.press(Press::Shoulder);
    assert_eq!(c.view(), View::Behind);
    assert!(close(c.distance, 4.5, 1e-6), "back to the default distance");

    c.press(Press::FirstPerson);
    assert!(c.first_person());
    c.press(Press::Wheel(-2));
    let left = Held {
        rotate_left: true,
        ..Held::default()
    };
    hold(&mut c, left, 1.0);
    hold(&mut c, Held::default(), 0.0);
    assert!(
        close(c.distance, 0.01, 1e-6) && c.yaw() == 0.0,
        "no zoom or turn"
    );
    c.press(Press::FirstPerson);
    assert!(!c.first_person() && close(c.distance, 4.5, 1e-6));

    c.press(Press::TopDown);
    assert!(close(c.pitch(), STEEP_PITCH, 1e-6));
    c.press(Press::TopDown);
    assert!(close(c.pitch(), 0.25, 1e-6));

    c.press(Press::Reverse);
    assert!(close(c.yaw().abs(), std::f32::consts::PI, 1e-5) && c.pitch() == 0.0);
    c.press(Press::Reverse);
    assert!(c.yaw() == 0.0);

    c.press(Press::SwingLeft);
    assert!(
        close(c.yaw().to_degrees(), 67.04, 0.1),
        "{}",
        c.yaw().to_degrees()
    );
    assert!(close(c.pitch(), 0.595, 0.001), "{}", c.pitch());
    c.press(Press::SwingRight);
    assert!(
        close(c.yaw().to_degrees(), -67.04, 0.1),
        "the other side at once"
    );
    c.press(Press::SwingRight);
    assert_eq!(c.view(), View::Behind);

    c.offset = Vec3::new(0.479, -0.878, 0.25);
    let yaw = c.yaw();
    c.press(Press::MapView);
    assert!(close(c.distance, MAP_DISTANCE, 1e-6) && close(c.pitch(), STEEP_PITCH, 1e-6));
    c.press(Press::MapView);
    assert!(
        close(c.yaw(), yaw, 1e-6) && close(c.distance, 4.5, 1e-6),
        "given back"
    );

    c.press(Press::MouseLook);
    let before = c.offset;
    c.press(Press::Shoulder);
    c.press(Press::Reverse);
    c.press(Press::SwingLeft);
    assert!(
        c.offset == before && close(c.distance, 4.5, 1e-6),
        "fixed in mouse look"
    );
}

/// In first person the mouse always steers the character in mouse look: the option only speaks for
/// the third person view.
///
/// Behaviour: desktop.camera.first-person-mouse-look-steers-whatever-the-option-says
#[test]
fn first_person_mouse_look_steers_whatever_the_option_says() {
    let mut c = camera_with(Options {
        mouse_look_steers: false,
        ..Options::default()
    });
    c.press(Press::MouseLook);
    c.mouse(10.0, 0.0);
    assert_eq!(
        c.steering_heading(false),
        None,
        "third person, the option off"
    );
    c.set_first_person(true);
    assert!(
        c.steering_heading(false).is_some(),
        "first person, the option off"
    );
}

/// With steering, the heading mouse look gives the character is the way the camera looks, read
/// from the camera every frame: without a mouse count, and whatever way it looked before. Outside
/// mouse look it gives none.
///
/// Behaviour: desktop.camera.mouse-look-sets-the-heading-from-the-camera
#[test]
fn mouse_look_gives_the_character_the_way_the_camera_looks() {
    let mut c = camera();
    assert_eq!(
        c.steering_heading(false),
        None,
        "nothing outside mouse look"
    );
    c.press(Press::MouseLook);
    assert!(close(c.steering_heading(false).expect("steers"), 0.0, 1e-4));
    c.mouse(90.0, 0.0);
    let h = c.steering_heading(false).expect("steers");
    assert!(close(h, 90.0, 1e-3), "ninety counts right: {h}");
    assert!(
        close(c.steering_heading(false).expect("steers"), 90.0, 1e-3),
        "and the same the next frame, with no count"
    );
    // The camera looking off to one side when mouse look starts: the character takes that way.
    let mut c = camera();
    c.press(Press::SwingLeft);
    let swung = c.global_direction();
    c.press(Press::MouseLook);
    let h = c.steering_heading(false).expect("steers");
    let (sin, cos) = math::sin_cosf(h.to_radians());
    let len = (swung.x * swung.x + swung.y * swung.y).sqrt();
    assert!(
        close(sin * len, swung.x, 1e-3) && close(cos * len, swung.y, 1e-3),
        "{h} for {swung:?}"
    );
    assert!(h > 30.0 && h < 330.0, "off to one side: {h}");
    assert_eq!(
        c.steering_heading(true),
        None,
        "a move the server ordered keeps the heading"
    );
}

/// In mouse look a mouse count turns the camera a degree times the sensitivity, smoothed over the
/// newest ten counts weighted 1, 0.2, 0.04 and so on; moving the mouse toward the player tilts the
/// view down (the camera up) unless the axis is inverted; with steering the character turns with
/// the camera; the pitch stays within its limit.
///
/// Behaviour: desktop.camera.mouse-look-turns-a-degree-a-count-smoothed
#[test]
fn mouse_look_turns_a_degree_a_count_smoothed() {
    let mut c = camera();
    let before = c.yaw();
    c.mouse(10.0, 0.0);
    assert_eq!(c.yaw(), before, "nothing outside mouse look");
    c.press(Press::MouseLook);
    c.mouse(10.0, 0.0);
    let h = c.steering_heading(false).expect("steers");
    assert!(close(h, 10.0, 1e-3), "ten counts right: {h}");
    // The next sample is smoothed with the one before it: (0 * 1 + 10 * 0.2) / 1.2.
    c.mouse(0.0, 0.0);
    let h = c.steering_heading(false).expect("steers");
    assert!(close(h, 10.0 + 10.0 * 0.2 / 1.2, 1e-3), "{h}");
    let mut c = camera();
    c.press(Press::MouseLook);
    let p0 = c.pitch();
    c.mouse(0.0, 5.0);
    assert!(
        close(c.pitch() - p0, 5f32.to_radians(), 1e-5),
        "toward the player tilts up"
    );
    let mut c = camera_with(Options {
        invert_y: true,
        mouse_look_steers: false,
        mouse_sensitivity: 2.0,
        ..Options::default()
    });
    c.press(Press::MouseLook);
    let p0 = c.pitch();
    c.mouse(3.0, 5.0);
    assert_eq!(
        c.steering_heading(false),
        None,
        "the camera turns, not the character"
    );
    // Turning the view right swings the camera round to the player's left.
    assert!(close(c.yaw(), 6f32.to_radians(), 1e-5), "{}", c.yaw());
    assert!(
        close(c.pitch() - p0, -(10f32).to_radians(), 1e-5),
        "inverted"
    );
    for _ in 0..100 {
        c.mouse(0.0, -50.0);
    }
    assert!(
        close(c.pitch(), PITCH_LIMIT, 1e-6),
        "within the limit: {}",
        c.pitch()
    );
}

/// At rest the camera stands behind the player 4.5 m along its direction (pitched 0.25 radians)
/// times the player's height scale, raised by the player's height, and looks back along that
/// direction; the lines that keep it off the world start three quarters up the player, moved 0.7
/// of the player's radius to its right shoulder. First person puts the eyes nine tenths up the
/// player, 0.4 m ahead.
///
/// Behaviour: desktop.camera.it-stands-behind-and-its-lines-start-at-the-shoulder
#[test]
fn it_stands_behind_and_its_lines_start_at_the_shoulder() {
    let mut c = camera();
    let mut p = player();
    p.scale_z = 1.1;
    p.height = 2.06 * 1.1;
    c.update(0.016, &p);
    let v = c.viewer(&p);
    let back = 4.5 * 1.1;
    let want = Vec3::new(
        0.0,
        -back * math::cosf(0.25),
        back * math::sinf(0.25) + 2.06 * 1.1,
    );
    assert!(closev(v.origin, want, 1e-4), "{:?} {want:?}", v.origin);
    assert!(closev(
        v.look,
        Vec3::new(0.0, math::cosf(0.25), -math::sinf(0.25)),
        1e-5
    ));
    c.set_player(&p, PLAYER, p.height);
    let t = c.target_offset(&p);
    assert!(
        closev(t, Vec3::new(0.42, 0.0, 0.75 * 2.06 * 1.1), 1e-4),
        "{t:?}"
    );
    // Facing east, the right shoulder is to the south.
    p.heading = 90.0;
    let t = c.target_offset(&p);
    assert!(
        closev(t, Vec3::new(0.0, -0.42, 0.75 * 2.06 * 1.1), 1e-4),
        "{t:?}"
    );

    let mut c = camera();
    let p = player();
    c.press(Press::FirstPerson);
    c.update(0.016, &p);
    let v = c.viewer(&p);
    assert!(
        closev(v.origin, Vec3::new(0.0, 0.4, 0.9 * 2.06), 1e-4),
        "{:?}",
        v.origin
    );
}

/// Close in the camera slides toward the right shoulder by 0.7 of the player's radius times what
/// is left of 2.9 m, easing there over a second, and never stands nearer than half a meter across
/// the ground.
///
/// Behaviour: desktop.camera.close-in-it-slides-to-the-shoulder
#[test]
fn close_in_it_slides_to_the_shoulder() {
    let mut c = camera();
    let p = player();
    c.update(0.016, &p);
    assert!(
        close(c.viewer(&p).origin.x, 0.0, 1e-6),
        "no shoulder beyond 2.9 m"
    );
    c.press(Press::Shoulder);
    let side = (1.0 - 1.3 / 2.9) * 0.7 * 0.6;
    let mut xs = Vec::new();
    for _ in 0..120 {
        c.update(0.01, &p);
        xs.push(c.viewer(&p).origin.x);
    }
    assert!(
        xs.windows(2).all(|w| w[1] >= w[0] - 1e-6),
        "it eases one way"
    );
    assert!(xs[10] < side * 0.2, "not at once: {}", xs[10]);
    let end = xs[xs.len() - 1];
    assert!(close(end, side, 1e-4), "{end} {side}");
    c.press(Press::Wheel(10));
    for _ in 0..200 {
        c.update(0.01, &p);
    }
    let v = c.viewer(&p);
    let across = (v.origin.x * v.origin.x + v.origin.y * v.origin.y).sqrt();
    assert!(close(across, 0.5, 1e-4), "half a meter: {across}");
}

/// A change of view glides: each frame the camera moves from where it stood toward its new place
/// by the share of the transition time (a second) gone, and stands there once it is up.
///
/// Behaviour: desktop.camera.a-change-of-view-glides-over-the-transition-time
#[test]
fn a_change_of_view_glides_over_the_transition_time() {
    let mut c = camera();
    let p = player();
    for _ in 0..200 {
        c.update(0.01, &p);
        let v = c.viewer(&p);
        c.last = Some(v);
    }
    let behind = c.last.map_or(Vec3::ZERO, |v| v.origin);
    c.press(Press::TopDown);
    let mut ys = Vec::new();
    let mut end = behind;
    for _ in 0..150 {
        c.update(0.01, &p);
        let v = c.viewer(&p);
        c.last = Some(v);
        end = v.origin;
        ys.push(v.origin.y);
    }
    let pitch = STEEP_PITCH;
    let want = Vec3::new(
        0.0,
        -4.5 * math::cosf(pitch),
        4.5 * math::sinf(pitch) + 2.06,
    );
    assert!(
        close(ys[0], behind.y, 0.05),
        "starts where it stood: {} {}",
        ys[0],
        behind.y
    );
    assert!(
        ys[4] - behind.y > 0.1 && want.y - ys[4] > 0.1,
        "under way: {} of {} to {}",
        ys[4],
        behind.y,
        want.y
    );
    assert!(closev(end, want, 1e-3), "there after the second: {end:?}");
}

/// A camera riding a bone or part way through a glide leans with its frame: a frame with no lean
/// reads none, and one turned about the way it looks reads that turn, its right going up. The
/// view's axes carry it: leaned a quarter turn, its right is the level view's up.
///
/// Behaviour: desktop.camera.the-view-leans-as-its-frame-does
#[test]
fn the_view_leans_as_its_frame_does() {
    use auberean_draw::scene::view_axes;
    use auberean_scene::camera::{lean_of, stand};
    use dereth_primitives::{CellId, Quat};
    let level = stand(CellId(0x7F7F_0001), Vec3::ZERO, Vec3::new(0.3, 0.9, -0.2));
    assert_eq!(lean_of(&level), 0.0);
    let (s, c) = (math::sinf(0.15), math::cosf(0.15));
    // Turned about its own forward, y: the frame's x goes down for a positive turn, up for this.
    let about_forward = Quat {
        w: c,
        x: 0.0,
        y: -s,
        z: 0.0,
    };
    let mut leaned = stand(CellId(0x7F7F_0001), Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0));
    leaned.frame.rotation = about_forward;
    let want = 0.3_f32.to_degrees();
    assert!(close(lean_of(&leaned), want, 1e-3), "{}", lean_of(&leaned));
    let f = auberean_primitives::Vec3::new(0.0, 1.0, 0.0);
    let (flat_right, flat_up) = view_axes(f, 0.0);
    let (right, up) = view_axes(f, 90.0);
    let near = |a: auberean_primitives::Vec3, b: auberean_primitives::Vec3| {
        (a.x - b.x).abs() < 1e-5 && (a.y - b.y).abs() < 1e-5 && (a.z - b.z).abs() < 1e-5
    };
    assert!(near(right, flat_up), "{right:?}");
    assert!(
        near(
            up,
            auberean_primitives::Vec3::new(-flat_right.x, -flat_right.y, -flat_right.z)
        ),
        "{up:?}"
    );
}

/// Only the walls and the ground stop the camera in the map view, or while it looks at a bone: an
/// object between it and the bone doesn't push it in. Otherwise objects stop it too.
///
/// Behaviour: desktop.camera.objects-dont-stop-a-camera-on-a-bone
#[test]
fn objects_dont_stop_a_camera_looking_at_a_bone() {
    let mut c = camera();
    assert!(!c.check_env_only());
    c.targeting.target_bone = 3;
    assert!(c.check_env_only());
    c.targeting.target_bone = 0;
    assert!(!c.check_env_only());
}

/// With the auto distance option a camera 1.5 m or nearer pulls back after 2.5 seconds of running,
/// growing each frame until 4 m out, and comes back after a second standing.
///
/// Behaviour: desktop.camera.auto-distance-pulls-a-close-camera-back-while-running
#[test]
fn auto_distance_pulls_a_close_camera_back_while_running() {
    let mut c = camera();
    c.press(Press::Shoulder);
    let mut p = player();
    p.local_velocity = Vec3::new(0.0, 6.5, 0.0);
    for _ in 0..240 {
        c.update(0.01, &p);
    }
    assert!(close(c.targeting.distance, 1.3, 1e-6), "not before 2.5 s");
    for _ in 0..200 {
        c.update(0.01, &p);
    }
    let out = c.targeting.distance;
    assert!(close(out, 4.0, 1e-4), "out to 4 m: {out}");
    assert!(close(c.distance, 1.3, 1e-6), "the chosen distance kept");
    p.local_velocity = Vec3::ZERO;
    for _ in 0..90 {
        c.update(0.01, &p);
    }
    assert!(close(c.targeting.distance, 4.0, 1e-4), "held for a second");
    for _ in 0..400 {
        c.update(0.01, &p);
    }
    let back = c.targeting.distance;
    assert!(close(back, 1.3, 1e-4), "back: {back}");
    let mut far = camera();
    p.local_velocity = Vec3::new(0.0, 6.5, 0.0);
    for _ in 0..600 {
        far.update(0.01, &p);
    }
    assert!(close(far.targeting.distance, 4.5, 1e-6), "only close in");
}

/// Align to slope (on by default) lowers the camera while the player climbs, easing back to level
/// once the ground is flat; the option off, the pitch stays.
///
/// Behaviour: desktop.camera.align-to-slope-follows-the-climb
#[test]
fn align_to_slope_follows_the_climb() {
    fn pitch_of(c: &mut CameraControl, p: &Subject) -> f32 {
        math::asinf(-c.viewer(p).look.z)
    }
    let mut c = camera();
    let mut p = player();
    for i in 0..100u8 {
        p.origin.z = f32::from(i) * 0.05;
        c.update(0.01, &p);
    }
    let climbing = pitch_of(&mut c, &p);
    assert!(climbing < 0.25 - 0.01, "lower while climbing: {climbing}");
    for _ in 0..2000 {
        c.update(0.01, &p);
    }
    let flat = pitch_of(&mut c, &p);
    assert!(close(flat, 0.25, 1e-3), "level again: {flat}");
    let mut off = camera_with(Options {
        align_to_slope: false,
        ..Options::default()
    });
    for i in 0..100u8 {
        p.origin.z = f32::from(i) * 0.05;
        off.update(0.01, &p);
    }
    assert!(close(pitch_of(&mut off, &p), 0.25, 1e-5));
}

/// The field of view is 65 degrees plus the effects' sum, which is never under 10: 75 at rest. An
/// effect ramps up, holds and ramps down over its period, then ends; a distance effect adds to the
/// camera's distance.
///
/// Behaviour: desktop.camera.effects-ramp-and-add-to-the-field-of-view
#[test]
fn effects_ramp_and_add_to_the_field_of_view() {
    let mut c = camera();
    let p = player();
    c.update(0.01, &p);
    assert!(close(c.fov, 75.0, 1e-6));
    c.effects
        .add(Effect::FieldOfView, 30.0, 1.0, 1.0, 0.25, 0.25, 0);
    c.update(0.125, &p);
    assert!(close(c.fov, 65.0 + 15.0, 1e-3), "halfway up: {}", c.fov);
    c.update(0.375, &p);
    assert!(close(c.fov, 95.0, 1e-3), "held: {}", c.fov);
    c.update(0.375, &p);
    assert!(close(c.fov, 65.0 + 15.0, 1e-3), "halfway down: {}", c.fov);
    c.update(0.2, &p);
    assert!(
        close(c.fov, 75.0, 1e-6) && c.effects.tracks.is_empty(),
        "ended"
    );
    c.effects
        .add(Effect::Distance, 2.0, 0.5, 1.0, 0.01, 0.01, 0);
    c.update(0.5, &p);
    assert!(
        close(c.camera_distance(), 5.5, 1e-4),
        "{}",
        c.camera_distance()
    );
}

/// A camera effect record starts the effects it names at its intensity (one when it names none)
/// times the scale asked, over its period; one for the player only does nothing from another
/// source.
///
/// Behaviour: desktop.camera.effects-ramp-and-add-to-the-field-of-view
#[test]
fn a_camera_effect_record_starts_its_effects() {
    use auberean_assets::fx::CameraFx;
    let mut c = camera();
    let p = player();
    let fx = CameraFx {
        field_of_view: Some(true),
        intensity: Some(20.0),
        period: Some(2.0),
        player_only: Some(true),
        ..CameraFx::default()
    };
    c.do_camera_fx(&p, &fx, 1.0, 7);
    assert!(c.effects.tracks.is_empty(), "the player's only");
    c.do_camera_fx(&p, &fx, 0.5, PLAYER);
    c.update(1.0, &p);
    assert!(close(c.fov, 65.0 + 10.0, 1e-4), "{}", c.fov);
    c.do_camera_fx(&p, &fx, 2.0, PLAYER);
    c.update(0.5, &p);
    assert!(
        close(c.fov, 65.0 + 10.0 + 40.0, 1e-3),
        "two at once: {}",
        c.fov
    );
}

/// The shake's noise is the client's own: its fractional Brownian motion (half the weight an
/// octave, twice the frequency, three octaves) over the client's gradient noise gives, at a dozen
/// points from 0 to 1234.5, what the 0930 client's own code gives run in an emulator.
///
/// Behaviour: desktop.camera.the-shakes-noise-is-the-clients
#[test]
fn the_shakes_noise_is_the_clients() {
    let noise = auberean_draw::portal::Noise::new();
    // The point, and the 0930 client's answer.
    let retail: [(f64, f32); 12] = [
        (0.0, 0.0),
        (0.25, -0.055_887_286),
        (1.0, 0.0),
        (3.7, -0.056_939_72),
        (5.5, 0.059_570_313),
        (9.99, 0.007_867_187),
        (12.3, -0.188_122_18),
        (60.0, 0.0),
        (61.5, -0.083_007_81),
        (123.456, -0.187_881),
        (600.75, 0.170_060_47),
        (1234.5, 0.197_265_63),
    ];
    for (p, want) in retail {
        let got = noise.fbm1(p, 0.5, 2.0, 3.0);
        assert!(close(got, want, 1e-6), "at {p}: {got} against {want}");
    }
}

/// A shake moves the camera by three layers of noise sixty times its seconds along, its intensity
/// times its weight over its source's distance (at least a meter), up and down three times as
/// much; a source four meters off moves it a quarter as far, a source not here not at all, and
/// with no shake under way nothing moves.
///
/// Behaviour: desktop.camera.a-shake-moves-the-camera-by-the-clients-noise
#[test]
fn a_shake_moves_the_camera_by_its_noise_over_its_distance() {
    use auberean_assets::fx::CameraFx;
    let noise = auberean_draw::portal::Noise::new();
    let mut c = camera();
    let p = player();
    let me = Vec3::new(10.0, 20.0, 0.0);
    assert_eq!(
        c.effects.distortion(Some(me), |_| Some(me)),
        Vec3::ZERO,
        "no shake"
    );
    let fx = CameraFx {
        shake: Some(true),
        intensity: Some(2.0),
        period: Some(3.0),
        ramp_up: Some(1.0),
        ..CameraFx::default()
    };
    c.do_camera_fx(&p, &fx, 1.0, PLAYER);
    c.update(0.5, &p);
    let t = &c.effects.tracks[0];
    let (along, k) = (t.time * 60.0, t.intensity * t.weight);
    assert!(close(k, 1.0, 1e-6), "half way up: {k}");
    let [sx, sy, sz] = c.effects.shake_seeds;
    let fbm = |x: f32| noise.fbm1(f64::from(x), 0.5, 2.0, 3.0);
    let want = Vec3::new(
        fbm(sx + along) * k,
        fbm(along + sy) * k,
        fbm(along + sz) * 3.0 * k,
    );
    let at_me = c.effects.distortion(Some(me), |_| Some(me));
    assert!(closev(at_me, want, 1e-6), "{at_me:?} against {want:?}");
    assert!(want.x != 0.0 || want.y != 0.0 || want.z != 0.0, "it moves");
    let off = Vec3::new(10.0, 24.0, 0.0);
    let far = c.effects.distortion(Some(me), |_| Some(off));
    let quarter = Vec3::new(want.x / 4.0, want.y / 4.0, want.z / 4.0);
    assert!(closev(far, quarter, 1e-6), "{far:?} against {quarter:?}");
    assert_eq!(c.effects.distortion(Some(me), |_| None), Vec3::ZERO);
}

/// The player and one other object, 10 m east of it facing east, 1.5 m high, its height scale 1.2.
struct TwoObjects {
    player: Subject,
}

const OTHER: u64 = 0x8000_0000_0000_0042;

impl Objects for TwoObjects {
    fn place(&self, id: u64, bone: u32) -> Option<Viewer> {
        match id {
            PLAYER => self.player.place(id, bone),
            OTHER => Some(Viewer {
                origin: Vec3::new(10.0, 0.0, 0.0),
                look: Vec3::new(1.0, 0.0, 0.0),
                rotation: None,
            }),
            _ => None,
        }
    }

    fn facts(&self, id: u64) -> Option<Facts> {
        match id {
            PLAYER => self.player.facts(id),
            OTHER => Some(Facts {
                height: 1.5,
                scale_z: 1.2,
                ..Facts::default()
            }),
            _ => None,
        }
    }
}

/// Runs `seconds` of 10 ms frames, the camera placed each frame where it wants to be; where it
/// stands at the end.
fn run(c: &mut CameraControl, world: &TwoObjects, seconds: f32) -> Viewer {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (seconds / 0.01).round() as usize;
    let mut v = c.viewer_with(&world.player, world);
    for _ in 0..frames {
        c.update_with(0.01, &world.player, world);
        v = c.viewer_with(&world.player, world);
        c.last = Some(v);
    }
    v
}

/// The player and an object facing back at it, south of its look.
struct FacingBack {
    player: Subject,
}

impl Objects for FacingBack {
    fn place(&self, id: u64, bone: u32) -> Option<Viewer> {
        match id {
            PLAYER => self.player.place(id, bone),
            OTHER => Some(Viewer {
                origin: Vec3::new(0.0, 10.0, 0.0),
                look: Vec3::new(0.0, -1.0, 0.0),
                rotation: None,
            }),
            _ => None,
        }
    }

    fn facts(&self, id: u64) -> Option<Facts> {
        match id {
            PLAYER => self.player.facts(id),
            OTHER => Some(Facts {
                height: 1.5,
                scale_z: 1.2,
                ..Facts::default()
            }),
            _ => None,
        }
    }
}

/// A glide to a view facing back the other way turns the whole frame, swinging round to the side
/// and leaning on the way, where a turn of the look alone would dip straight at the ground.
///
/// Behaviour: desktop.camera.a-glide-turns-the-whole-view
#[test]
fn a_glide_turns_the_whole_view_rather_than_its_look() {
    use auberean_physics::geom::{rotation_axes, set_vector_heading, slerp_rotation};
    use dereth_primitives::{Frame, Quat};
    let world = FacingBack { player: player() };
    let mut c = camera();
    let mut v = c.viewer_with(&world.player, &world);
    for _ in 0..10 {
        c.update_with(0.01, &world.player, &world);
        v = c.viewer_with(&world.player, &world);
        c.last = Some(v);
    }
    let before = v.look;
    c.set_free_camera(&world, OTHER, 30.0, 1.0);
    for _ in 0..50 {
        c.update_with(0.01, &world.player, &world);
        v = c.viewer_with(&world.player, &world);
        c.last = Some(v);
    }
    let w = c.targeting.weight;
    assert!(w > 0.2 && w < 0.8, "under way: {w}");
    let frame_of = |look| {
        let mut f = Frame::new(Vec3::ZERO, Quat::IDENTITY);
        set_vector_heading(&mut f, look);
        f
    };
    let mut want = Frame::new(Vec3::ZERO, Quat::IDENTITY);
    slerp_rotation(
        &mut want,
        &frame_of(before),
        &frame_of(Vec3::new(0.0, -1.0, 0.0)),
        w,
    );
    let m = rotation_axes(want.rotation);
    assert!(
        closev(v.look, Vec3::new(m[3], m[4], m[5]), 1e-4),
        "the frames' turn: {:?}",
        v.look
    );
    println!("look under way {:?} at {w}", v.look);
    assert!(v.look.z > -0.5, "never at the ground: {:?}", v.look);
    let right = rotation_axes(v.rotation.expect("the whole turn"));
    assert!(right[2].abs() > 0.01, "it leans on the way: {}", right[2]);
}

/// The cameras the effects and the animations ask for, at their places once their glides are
/// over (the default view: 4.5 m out, pitched 0.25 radians, behind the player facing north):
///
/// - **Free** (hung from an object, looking at nothing): at the object, out a third of the
///   distance along the view, raised by its height, facing its way; once its period is over it
///   glides back to the player over two seconds, half way after one.
/// - **Looking at an object:** the distance out from it along the view times its height scale,
///   raised by its height, looking back along the view.
/// - **Hung from the player, looking at an object:** back from the player a third of the distance
///   along the line from the object, raised by the player's height, looking from the player's
///   head to the object.
/// - **Hung from another object, looking at the player:** at the object raised by its height,
///   looking from there to the player.
///
/// Behaviour: desktop.camera.the-scripts-cameras-hang-from-look-at-and-come-back
#[test]
fn the_scripts_cameras_hang_from_look_at_and_come_back() {
    let world = TwoObjects { player: player() };
    let (cp, sp) = (math::cosf(0.25), math::sinf(0.25));
    let behind = Vec3::new(0.0, -4.5 * cp, 4.5 * sp + 2.06);
    let mut c = camera();
    let v = run(&mut c, &world, 0.1);
    assert!(closev(v.origin, behind, 1e-4), "behind: {:?}", v.origin);

    // Free, from the other object, for three seconds, gliding half a second.
    c.set_free_camera(&world, OTHER, 3.0, 0.5);
    let free = Vec3::new(10.0, -4.5 * cp / 3.0, 4.5 * sp / 3.0 + 1.5);
    let v = run(&mut c, &world, 0.25);
    assert!(
        !closev(v.origin, free, 0.5) && !closev(v.origin, behind, 0.5),
        "under way: {:?}",
        v.origin
    );
    let v = run(&mut c, &world, 0.5);
    assert!(
        closev(v.origin, free, 1e-4),
        "at the object: {:?}",
        v.origin
    );
    assert!(closev(v.look, Vec3::new(1.0, 0.0, 0.0), 1e-5), "its way");
    // Its period over (at 2.5 s, its ramp before the end), back to the player over two seconds.
    let v = run(&mut c, &world, 1.75 + 1.0);
    let half = Vec3::new(
        (free.x + behind.x) / 2.0,
        (free.y + behind.y) / 2.0,
        (free.z + behind.z) / 2.0,
    );
    assert!(
        closev(v.origin, half, 0.15),
        "half way back: {:?}",
        v.origin
    );
    let v = run(&mut c, &world, 1.1);
    assert!(closev(v.origin, behind, 1e-4), "back: {:?}", v.origin);
    let v = run(&mut c, &world, 5.0);
    assert!(closev(v.origin, behind, 1e-4), "and held: {:?}", v.origin);

    // Looking at the other object, from a camera effect record it plays.
    let fx = auberean_assets::fx::CameraFx {
        target_camera: Some(true),
        period: Some(30.0),
        ramp_down: Some(0.5),
        ..auberean_assets::fx::CameraFx::default()
    };
    c.do_camera_fx(&world, &fx, 1.0, OTHER);
    let v = run(&mut c, &world, 1.0);
    let at = Vec3::new(10.0, -4.5 * 1.2 * cp, 4.5 * 1.2 * sp + 1.5);
    assert!(closev(v.origin, at, 1e-4), "out from it: {:?}", v.origin);
    assert!(
        closev(v.look, Vec3::new(0.0, cp, -sp), 1e-5),
        "along the view"
    );

    // Hung from the player, looking at the other object.
    c.set_parent_object(&world, PLAYER, 30.0, 0.5);
    let v = run(&mut c, &world, 1.0);
    let line = Vec3::new(10.0, 0.0, -2.06);
    let len = (line.x * line.x + line.z * line.z).sqrt();
    let want = Vec3::new(-line.x / len * 1.5, 0.0, -line.z / len * 1.5 + 2.06);
    assert!(
        closev(v.origin, want, 1e-4),
        "behind the player's head: {:?}",
        v.origin
    );
    assert!(
        closev(v.look, Vec3::new(line.x / len, 0.0, line.z / len), 1e-5),
        "toward the object: {:?}",
        v.look
    );

    // Hung from the other object, looking at the player: at it, raised by its height, looking
    // from there to the player's feet. (Each place here is where the 0930 client's own code puts
    // the camera for these objects.)
    c.set_parent_object(&world, OTHER, 30.0, 0.5);
    c.set_target_object(&world, PLAYER, 30.0, 0.5);
    let v = run(&mut c, &world, 1.0);
    assert!(
        closev(v.origin, Vec3::new(10.0, 0.0, 1.5), 1e-4),
        "at the object: {:?}",
        v.origin
    );
    assert!(
        closev(v.look, Vec3::new(-0.988_936_4, 0.0, -0.148_340_45), 1e-5),
        "toward the player: {:?}",
        v.look
    );
}

/// A new default distance or tilt takes the camera there at once; another option, or the same
/// ones again, leaves it where it was put.
///
/// Behaviour: desktop.camera.a-new-default-distance-or-tilt-applies-at-once
#[test]
fn a_new_default_distance_or_tilt_applies_at_once() {
    let mut c = camera();
    c.distance = 9.0;
    let same = c.options;
    c.set_options(same);
    assert!(
        close(c.distance, 9.0, 1e-6),
        "the same options move nothing"
    );
    let quicker = Options {
        zoom_speed: same.zoom_speed * 2.0,
        ..same
    };
    c.set_options(quicker);
    assert!(close(c.distance, 9.0, 1e-6), "another option moves nothing");
    let further = Options {
        distance: 6.0,
        ..quicker
    };
    c.set_options(further);
    assert!(close(c.distance, 6.0, 1e-6), "{}", c.distance);
    let higher = Options {
        tilt: 0.5,
        ..further
    };
    c.distance = 9.0;
    c.set_options(higher);
    assert!(close(c.offset.z, 0.5, 1e-6), "{}", c.offset.z);
    assert!(close(c.distance, 6.0, 1e-6), "{}", c.distance);
}

/// A place whose frame isn't one is not taken: the camera stays where it stood, or stays unplaced;
/// every way the camera can look is a frame.
///
/// Behaviour: desktop.camera.a-frame-that-isnt-one-is-not-taken
#[test]
#[allow(clippy::cast_possible_truncation)]
fn a_frame_that_isnt_one_is_not_taken() {
    use auberean_scene::camera::{take_place, Stands};
    use dereth_primitives::{CellId, Frame, Position, Quat};
    let cell = CellId(0x4040_0001);
    let good = Position::new(cell, Frame::new(Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY));
    let not_a_number = Position::new(
        cell,
        Frame::new(Vec3::new(f32::NAN, 2.0, 3.0), Quat::IDENTITY),
    );
    let long = Position::new(
        cell,
        Frame::new(Vec3::new(1.0, 2.0, 3.0), Quat::new(2.0, 0.0, 0.0, 0.0)),
    );
    for bad in [not_a_number, long] {
        let mut stands = Some(Stands::at(good));
        assert_eq!(take_place(&mut stands, Some(bad), good, false), None);
        assert_eq!(stands.unwrap().current, good, "it stays where it stood");
        let mut unplaced = None;
        take_place(&mut unplaced, None, bad, false);
        assert!(unplaced.is_none(), "it stays unplaced");
    }
    let mut stands = None;
    assert_eq!(take_place(&mut stands, Some(good), good, false), Some(good));
    assert_eq!(stands.unwrap().current, good);
    for yaw in 0..72 {
        for pitch in -8..=8 {
            let (yaw, pitch) = (
                f64::from(yaw) * 0.087_266_46,
                f64::from(pitch) * 0.174_532_9,
            );
            let viewer = Viewer {
                origin: Vec3::new(1.0, 2.0, 3.0),
                look: Vec3::new(
                    (math::cos(pitch) * math::sin(yaw)) as f32,
                    (math::cos(pitch) * math::cos(yaw)) as f32,
                    math::sin(pitch) as f32,
                ),
                rotation: None,
            };
            let at = viewer.stand_at(cell, viewer.origin);
            assert!(
                auberean_physics::geom::frame_is_valid(&at.frame),
                "yaw {yaw} pitch {pitch}: {at:?}"
            );
        }
    }
}
