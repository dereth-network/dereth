//! Explore mode through the shell, with no server: the camera flown by the keys through the
//! September 2013 world, held over its ground, and where it is copied.
//! Fixture: the September 2013 install (the land about block 6FA9 and south of it).
//!
//! Behaviour: none (tooling: a developer's camera flown through the world with no server)

use std::cell::Cell;
use std::rc::Rc;

use auberean_scene::explore::{parse_start, FreeCamera, ABOVE_A_BODY, ABOVE_THE_GROUND};
use auberean_shell::host::TestHost;
use auberean_shell::input::{Button, Event, Key};
use auberean_shell::interface::{Game, Interface, Request};
use auberean_shell::Shell;
use dereth_primitives::num::math;

use crate::session::data;

/// An interface that counts what reaches it.
struct Counts {
    events: Rc<Cell<u32>>,
    updates: Rc<Cell<u32>>,
}

impl Interface for Counts {
    fn event(&mut self, _event: &Event, _game: &Game<'_>) -> bool {
        self.events.set(self.events.get() + 1);
        true
    }
    fn update(&mut self, _game: &Game<'_>, _requests: &mut Vec<Request>) {
        self.updates.set(self.updates.get() + 1);
    }
}

/// The shell exploring from `start`, and the counts of what reached its interface.
fn exploring(start: &str) -> (Shell<'static, TestHost>, Rc<Cell<u32>>, Rc<Cell<u32>>) {
    let (world, data) = data();
    let (events, updates) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let interface = Counts {
        events: Rc::clone(&events),
        updates: Rc::clone(&updates),
    };
    let mut s = Shell::new(
        TestHost::default(),
        world,
        data.clone(),
        Box::new(interface),
    );
    s.start_explore(parse_start(start).expect("the start reads"));
    (s, events, updates)
}

/// `frames` frames of a thirtieth of a second each.
fn run(s: &mut Shell<'_, TestHost>, frames: u32) {
    for _ in 0..frames {
        s.host.time += 1.0 / 30.0;
        let out = s.frame();
        assert!(out.world.is_some(), "every frame draws the world");
    }
}

fn press(s: &mut Shell<'_, TestHost>, key: Key, down: bool) {
    s.event(&Event::Key {
        key,
        down,
        repeat: false,
    });
}

fn camera(s: &Shell<'_, TestHost>) -> FreeCamera {
    s.explore.as_ref().expect("exploring").camera
}

/// Flown ahead at the top speed for three seconds, the camera crosses fifteen kilometers of the
/// world where it looks, the world's frame drawn about the block it has reached, and never under
/// the ground; no key reaches the game's interface, which is never asked for anything.
#[test]
fn a_camera_flown_at_the_top_speed_crosses_the_world_and_its_frame_follows() {
    let (mut s, events, updates) = exploring("0x6FA90015 [45.24 90.51 85.26] 178");
    for _ in 0..10 {
        press(&mut s, Key::Equal, true);
        press(&mut s, Key::Equal, false);
    }
    run(&mut s, 1);
    press(&mut s, Key::W, true);
    let start = camera(&s);
    for _ in 0..90 {
        run(&mut s, 1);
        let c = camera(&s);
        if let Some(ground) = s.scene.ground_under(&c) {
            assert!(
                c.position.z >= ground + ABOVE_THE_GROUND - 1e-3,
                "{c:?} under {ground}"
            );
        }
    }
    let end = camera(&s);
    #[allow(clippy::cast_precision_loss)]
    let global = |c: &FreeCamera| {
        (
            c.block.0 as f32 * 160.0 + c.position.x,
            c.block.1 as f32 * 160.0 + c.position.y,
        )
    };
    let (a, b) = (global(&start), global(&end));
    let heading = 178_f32.to_radians();
    let (dx, dy) = (
        15_000.0 * math::sinf(heading),
        15_000.0 * math::cosf(heading),
    );
    assert!(
        (b.0 - a.0 - dx).abs() < 1.0 && (b.1 - a.1 - dy).abs() < 1.0,
        "{start:?} to {end:?}"
    );
    assert_eq!(end.block, (0x72, 0x4B), "{end:?}");
    assert_eq!(
        s.scene.seen.block, end.block,
        "the frame is the reached block's"
    );
    let seen = s.scene.seen.camera.expect("a camera seen");
    assert_eq!(seen.position, end.position);
    assert_eq!((events.get(), updates.get()), (0, 0));
}

/// Flown straight down onto the lawn a body stood on, the camera stops a meter over the ground
/// there: the body's height plus one.
#[test]
fn a_camera_flown_down_stops_a_meter_over_the_ground() {
    let (mut s, _, _) = exploring("0x6FA90015 [45.24 90.51 135.26] 178");
    assert!((camera(&s).position.z - (135.26 + ABOVE_A_BODY)).abs() < 1e-3);
    press(&mut s, Key::Q, true);
    run(&mut s, 60);
    let c = camera(&s);
    assert!(
        (c.position.z - (85.26 + ABOVE_THE_GROUND)).abs() < 0.05,
        "the camera stopped at {}",
        c.position.z
    );
    assert!((c.position.x - 45.24).abs() < 1e-3 && (c.position.y - 90.51).abs() < 1e-3);
}

/// Map coordinates start the camera a little over the ground of their land cell once the ground
/// is drawn.
#[test]
fn map_coordinates_start_the_camera_a_little_over_the_ground() {
    let (mut s, _, _) = exploring("32.9N, 12.7W");
    run(&mut s, 1);
    let c = camera(&s);
    let ground = s.scene.ground_under(&c).expect("the ground is drawn");
    assert!(!c.find_ground);
    assert!(
        (c.position.z - (ground + ABOVE_A_BODY)).abs() < 1e-3,
        "{c:?} over {ground}"
    );
}

/// The copy key puts where the camera is on the clipboard as a location line that starts the camera
/// there again, and the corner says so; the right button held turns the camera with the mouse and
/// holds the pointer, let go of it with the button.
#[test]
fn the_copy_key_copies_a_location_that_reads_back_and_the_mouse_looks() {
    let (mut s, _, _) = exploring("0x6FA90015 [45.24 90.51 85.26] 178");
    run(&mut s, 1);
    press(&mut s, Key::C, true);
    press(&mut s, Key::C, false);
    run(&mut s, 1);
    let line = s.host.copied.last().expect("a line copied").clone();
    let back = parse_start(&line).expect("the copied line reads");
    let c = camera(&s);
    assert_eq!(back.block, c.block);
    assert!((back.position.x - c.position.x).abs() < 1e-3);
    assert!((back.position.z - (c.position.z + ABOVE_A_BODY)).abs() < 1e-3);
    assert!((back.heading - c.heading).abs() < 0.01);
    let lines = s.explore.as_ref().expect("exploring").lines(s.host.time);
    assert!(lines.contains(&"Location copied.".to_owned()), "{lines:?}");
    assert!(
        !s.frame().overlay.quads.is_empty(),
        "the corner line is drawn"
    );
    // Mouse look.
    s.event(&Event::Motion { dx: 100.0, dy: 0.0 });
    assert!(
        (camera(&s).heading - c.heading).abs() < 1e-6,
        "no look unheld"
    );
    s.event(&Event::Button {
        button: Button::Right,
        down: true,
    });
    assert!(s.host.pointer_held);
    s.event(&Event::Motion { dx: 100.0, dy: 0.0 });
    assert!((camera(&s).heading - c.heading - 15.0).abs() < 1e-3);
    s.event(&Event::Button {
        button: Button::Right,
        down: false,
    });
    assert!(!s.host.pointer_held);
}
