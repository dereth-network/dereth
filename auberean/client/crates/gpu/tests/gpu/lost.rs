//! A graphics device that goes away. Fixture: a made-up flat wall, drawn again from one frame to the
//! next so its lists get buffers of their own, on a device destroyed between the two.

use crate::common::renderer;
use crate::fade::{frame_of, wall};

const SIDE: u32 = 16;

/// A device that goes away is noticed, and drawing on it asks for nothing it can't give: the frame
/// that would have kept a list in a buffer of its own goes by without one. Another device draws.
///
/// Behaviour: draw.mesh.a-lost-device-is-noticed-and-does-not-stop-the-game
#[test]
fn a_lost_device_is_noticed_and_does_not_stop_the_game() {
    let frame = frame_of(vec![wall(10.0, 0.25, false, 10.0)]);
    let mut load = |_: u32| None;
    let mut device = renderer();
    assert_eq!(device.device_lost(), None);
    let seen = device
        .render(&frame, SIDE, SIDE, &mut load)
        .expect("read back");
    assert_eq!(seen.rgba[0], 128, "drawn before");

    device.device().destroy();
    let _ = device.device().poll(wgpu::PollType::Poll);
    assert!(device.device_lost().is_some(), "noticed");
    let _ = device.render(&frame, SIDE, SIDE, &mut load);
    drop(device);

    let mut again = renderer();
    let seen = again
        .render(&frame, SIDE, SIDE, &mut load)
        .expect("read back");
    assert_eq!(seen.rgba[0], 128, "another device draws");
}
