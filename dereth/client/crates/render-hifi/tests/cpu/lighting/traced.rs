//! Behaviour: none (experimental Horizon rendering effects: which frames trace through the lamps'
//! scene, and how long the scene is kept once nothing does)

use dereth_render_hifi::passes::rt::{traces, Keeping, Kept, KEEP_FRAMES};
use dereth_render_hifi::snapshot::HifiLamp;
use dereth_render_hifi::HifiFrame;
use glam::Vec3;

/// A frame with the sun `height` up (the sine of its angle), out of doors or not, with one lamp
/// or none.
fn frame(height: f32, outdoor: bool, lamp: bool) -> HifiFrame {
    let mut f = HifiFrame::default();
    f.sky.sun_direction = Vec3::new(0.0, (1.0 - height * height).sqrt(), height);
    f.sky.sun_brightness = 1.0;
    f.sky.outdoor = outdoor;
    if lamp {
        f.lamps.push(HifiLamp {
            position: Vec3::new(1.0, 2.0, 3.0),
            color: [1.0, 0.8, 0.5],
            intensity: 100.0,
            falloff: 5.0,
            source: dereth_render_hifi::snapshot::LampSource::Authored,
            flicker: false,
            seed: 0,
        });
    }
    f
}

#[test]
fn by_day_out_of_doors_nothing_traces_and_after_dusk_the_lamps_do() {
    assert!(!traces(&frame(0.8, true, true)), "noon");
    assert!(
        !traces(&frame(0.25, true, true)),
        "the sun still high enough"
    );
    assert!(traces(&frame(0.05, true, true)), "dusk, with a lamp");
    assert!(!traces(&frame(0.05, true, false)), "dusk, with no lamp");
    assert!(traces(&frame(-0.5, true, true)), "night");
}

#[test]
fn in_a_room_the_rooms_own_lights_trace_at_any_hour() {
    assert!(traces(&frame(0.8, false, false)));
}

#[test]
fn the_scene_is_kept_while_frames_trace_and_for_a_while_after_then_let_go_once() {
    let mut k = Keeping::default();
    assert_eq!(k.frame(true), Kept::Take);
    for i in 0..KEEP_FRAMES {
        assert_eq!(
            k.frame(false),
            Kept::Take,
            "frame {i} after the last that traced"
        );
    }
    assert_eq!(k.frame(false), Kept::LetGo);
    for _ in 0..10 {
        assert_eq!(k.frame(false), Kept::Skip);
    }
    assert!(!k.take_request(), "nothing asked while nothing traces");
}

#[test]
fn the_first_frame_that_traces_after_the_scene_was_let_go_asks_for_every_block_once() {
    let mut k = Keeping::default();
    for _ in 0..=KEEP_FRAMES {
        k.frame(false);
    }
    assert!(!k.take_request());
    assert_eq!(k.frame(true), Kept::Take);
    assert!(k.take_request());
    assert!(!k.take_request(), "asked once");
    assert_eq!(k.frame(true), Kept::Take);
    assert!(!k.take_request(), "a scene kept asks for nothing");
}

#[test]
fn a_frame_that_traces_before_the_scene_is_let_go_keeps_it_without_asking() {
    let mut k = Keeping::default();
    for _ in 0..KEEP_FRAMES {
        k.frame(false);
    }
    assert_eq!(k.frame(true), Kept::Take);
    assert!(!k.take_request());
    for _ in 0..KEEP_FRAMES {
        assert_eq!(k.frame(false), Kept::Take);
    }
}
