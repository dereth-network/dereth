//! Every pipeline the device makes, and frames that reach every kind of draw, pass the graphics
//! API's checks on the hardware GPU: a binding a shader reads from a stage its layout does not
//! name, a blend factor the API refuses, or a draw whose bindings do not fit its pipeline fails
//! here rather than in a player's client.
//!
//! Behaviour: none (a check of the device, which no behavior of the game names)

use std::collections::HashSet;

use auberean_draw::material::{draw_layers, instance_material, DEVICE_FLAGS};
use auberean_draw::scene::{SceneBuilder, Spot};
use auberean_primitives::Vec3;

use crate::common::{checked, ids_of, install, renderer};

/// Every pipeline the device makes builds: its own, and those every material layer in the data
/// asks for as a world object, a sky object and a water surface.
#[test]
fn every_pipeline_the_device_makes_builds() {
    let r = &install().records;
    let mut layers = Vec::new();
    let mut kinds = HashSet::new();
    for id in ids_of(0x31) {
        let Some(m) = instance_material(r, id) else {
            continue;
        };
        for l in draw_layers(&m, DEVICE_FLAGS) {
            kinds.insert((
                l.source_blend,
                l.dest_blend,
                l.depth_test,
                l.depth_write,
                l.cull,
            ));
            layers.push(l);
        }
    }
    assert!(layers.len() > 1_000, "{} layers", layers.len());
    let mut device = renderer();
    let before = device.pipeline_count();
    let built = checked(&mut device, |d| d.build_pipelines(&layers));
    println!(
        "{} layers in {} kinds of blend, depth and cull: {before} pipelines at start, {built} built",
        layers.len(),
        kinds.len()
    );
    // the terrain's five and the veil at start; the shadow map's two and the flares' two
    assert_eq!(before, 6);
    assert!(built > before + 4 + kinds.len(), "{built}");
}

/// A frame at the camp by the lake reaches every kind of draw the device has (the sky, the veil,
/// the near and far terrain with the sun's shadow map and the clouds' shadows, opaque and
/// translucent objects, frills, the water with its reflection, the lens flares and the markers
/// that look for the sun) and is drawn twice, the second time reading the first's look for the
/// sun, with no complaint from the device.
#[test]
fn a_frame_with_every_kind_of_draw_is_drawn() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    builder.uptime = 30.0;
    // the camp by the lake, looking east at the morning sun
    let spot = Spot {
        block: (0xC1, 0x56),
        position: Vec3::new(122.842, 89.758, 81.6),
        heading: 90.0,
        pitch: 5.0,
        time_of_day: 0.40,
        day: None,
    };
    let frame = builder.frame(&spot, 60.0);
    assert!(!frame.sky_before.is_empty(), "the sky");
    assert!(frame.terrain.iter().any(|b| b.far), "the far landscape");
    assert!(frame.terrain.iter().any(|b| b.layer > 0), "a blended layer");
    assert!(frame.shadow.is_some() && frame.clouds.is_some(), "shadows");
    assert!(frame.counts.frills > 0, "frills");
    assert!(
        !frame.water.is_empty() && frame.reflection.is_some(),
        "water"
    );
    assert!(!frame.flares.is_empty(), "a lens flare");
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let pictures = checked(&mut device, |d| {
        [0, 1].map(|_| d.render(&frame, 320, 240, &mut load).expect("read back"))
    });
    for p in &pictures {
        let colors: HashSet<&[u8]> = p.rgba.chunks(4).collect();
        assert!(colors.len() > 500, "{} colors", colors.len());
    }
}

/// Frames seen from inside, under two lights at once (the rooms under the interior's, what they
/// look out on under the light outdoors), are drawn with no complaint from the device: the start
/// hall's canyon open to the sky, and the cave whose first room has a doorway out to the land.
#[test]
fn frames_from_inside_under_two_lights_are_drawn() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    for (block, low, at) in [
        ((0x02, 0x98), 0x0109, Vec3::new(59.0, 240.0, -42.0)),
        ((0x69, 0x67), 0x0100, Vec3::new(109.75, 112.97, 100.8)),
    ] {
        let spot = Spot {
            block,
            position: at,
            heading: 0.0,
            pitch: 40.0,
            time_of_day: 0.5,
            day: None,
        };
        let (mut frame, _) = builder.inside_frame(&spot, 60.0, block, low);
        assert!(frame.outside.is_some(), "{block:02X?} looks out");
        // the rooms' own light made the interior's: dim, unfogged
        frame.light.ambient_brightness = 0.2;
        frame.light.fog_min = 0.0;
        frame.light.fog_max = 0.0;
        let p = checked(&mut device, |d| {
            d.render(&frame, 320, 240, &mut load).expect("read back")
        });
        let colors: HashSet<&[u8]> = p.rgba.chunks(4).collect();
        assert!(colors.len() > 100, "{block:02X?}: {} colors", colors.len());
    }
}
