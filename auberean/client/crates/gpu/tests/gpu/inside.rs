//! Frames seen from inside, as the walk through the doorways leaves them.

use auberean_draw::scene::{Cull, PointLights, SceneBuilder, Spot};
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::inside::DeviceLight;
use auberean_world::math::Mat4;

use crate::common::{checked, install, renderer};

/// How much green the hall's normal-mapped objects show under two point lights, each near the
/// camera and shining on what it sees: the first red and the second green, in the frame's light
/// made black but for them, inside or outside as asked; and how much red.
fn two_lights_on_the_normal_mapped(inside: bool) -> (u64, u64) {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let block = (0x02, 0x98);
    let at = Vec3::new(1.0, 330.0, -42.0);
    let spot = Spot {
        block,
        position: at,
        heading: 0.0,
        pitch: 0.0,
        time_of_day: 0.5,
        day: None,
    };
    let (mut frame, _) = builder.inside_frame(&spot, 60.0, block, 0x0121);
    frame
        .objects
        .retain(|b| b.layer.pixel_program.is_some() && !matches!(b.cull, Cull::Cell(_)));
    assert!(frame.objects.len() > 5, "{} draws", frame.objects.len());
    let light = |color: Color| DeviceLight {
        origin: at,
        diffuse: color,
        specular: Color::BLACK,
        range: 1000.0,
        flicker: None,
    };
    let lights = [
        light(Color::new(1.0, 0.0, 0.0, 1.0)),
        light(Color::new(0.0, 1.0, 0.0, 1.0)),
    ];
    for b in &mut frame.objects {
        b.points = if inside {
            PointLights::inside(&lights, &Mat4::IDENTITY)
        } else {
            PointLights::outdoors(&lights, &Mat4::IDENTITY)
        };
    }
    frame.light.ambient_brightness = 0.0;
    frame.light.direct_brightness = 0.0;
    frame.light.fog_min = 0.0;
    frame.light.fog_max = 0.0;
    frame.rooms = None;
    frame.outside = None;
    frame.terrain.clear();
    frame.water.clear();
    frame.sky_before.clear();
    frame.sky_after.clear();
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let p = checked(&mut device, |d| {
        d.render(&frame, 320, 240, &mut load).expect("read back")
    });
    p.rgba.chunks(4).fold((0, 0), |(g, red), c| {
        (g + u64::from(c[1]), red + u64::from(c[0]))
    })
}

/// A normal-mapped layer under two point lights: the second's share of the light comes in the
/// first's color, for the program counts its diffuse colors only where they are used and the first
/// light uses none. The first red and the second green: what is drawn is red and holds no green,
/// inside and outdoors.
///
/// Behaviour: draw.inside.a-normal-mapped-layers-second-light-shines-in-the-firsts-color
#[test]
fn a_normal_mapped_layers_second_light_shines_in_the_firsts_color() {
    for inside in [true, false] {
        let (green, red) = two_lights_on_the_normal_mapped(inside);
        println!("inside {inside}: red {red}, green {green}");
        assert!(red > 100_000, "inside {inside}: red {red}");
        assert!(
            green * 100 < red,
            "inside {inside}: red {red}, green {green}"
        );
    }
}

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
