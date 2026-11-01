//! Frames seen from inside, as the walk through the doorways leaves them.

use auberean_draw::scene::{SceneBuilder, Spot};
use auberean_primitives::Vec3;

use crate::common::{checked, install, renderer};

/// Seen from inside, rooms left out for the interior fog leave the background painted the fog's
/// color where they would have been: the cave's first room facing in, with every other room past
/// the fog, shows that color through its openings, and none of it with them drawn.
///
/// Behaviour: draw.inside.the-fog-paints-the-background-where-rooms-are-left-out
#[test]
fn the_fog_paints_the_background_where_rooms_are_left_out() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let block = (0x69, 0x67);
    let spot = Spot {
        block,
        position: Vec3::new(109.75, 112.97, 100.8),
        heading: 0.0,
        pitch: 0.0,
        time_of_day: 0.5,
        day: None,
    };
    let (mut frame, _) = builder.inside_frame(&spot, 60.0, block, 0x0100);
    frame.light.ambient_brightness = 0.2;
    let mut rooms = (*frame.rooms.clone().expect("the rooms")).clone();
    rooms.fog_color = [1.0, 0.0, 1.0, 1.0];
    let plain = {
        let mut f = frame.clone();
        f.rooms = Some(std::sync::Arc::new(rooms.clone()));
        f
    };
    for (low, room) in &mut rooms.cells {
        if *low != rooms.root {
            room.fog_cut = Some(0.0);
        }
    }
    frame.rooms = Some(std::sync::Arc::new(rooms));
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let [fogged, clear] = checked(&mut device, |d| {
        [&frame, &plain].map(|f| d.render(f, 320, 240, &mut load).expect("read back"))
    });
    let fog_colored = |p: &auberean_gpu::Picture| {
        p.rgba
            .chunks(4)
            .filter(|c| c[0] > 250 && c[1] < 5 && c[2] > 250)
            .count()
    };
    println!(
        "fog-colored pixels: {} with the rooms left out, {} with them drawn",
        fog_colored(&fogged),
        fog_colored(&clear)
    );
    assert!(fog_colored(&fogged) > 500);
    assert_eq!(fog_colored(&clear), 0);
}
