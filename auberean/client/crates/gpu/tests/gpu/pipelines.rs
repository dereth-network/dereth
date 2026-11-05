//! Every pipeline the device makes, and frames that reach every kind of draw, pass the graphics
//! API's checks on the hardware GPU: a binding a shader reads from a stage its layout does not
//! name, a blend factor the API refuses, or a draw whose bindings do not fit its pipeline fails
//! here rather than in a player's client.
//!
//! Behaviour: none (a check of the device, which no behavior of the game names)

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use auberean_draw::body::BodyDrawer;
use auberean_draw::material::{draw_layers, instance_material, DEVICE_FLAGS};
use auberean_draw::mesh::{MeshVertex, PosedVertex};
use auberean_draw::scene::{SceneBuilder, Spot};
use auberean_primitives::{DataId, Quat, Vec3};
use auberean_world::math::Mat4;

use crate::common::{checked, ids_of, install, renderer, renderer_antialiased};

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

/// The camp by the lake, every kind of draw in it but the lens flares, whose answers change the
/// picture from one frame to the next.
fn still_frame(builder: &mut SceneBuilder<'_>) -> auberean_draw::scene::Frame {
    builder.uptime = 30.0;
    let spot = Spot {
        block: (0xC1, 0x56),
        position: Vec3::new(122.842, 89.758, 81.6),
        heading: 90.0,
        pitch: 5.0,
        time_of_day: 0.40,
        day: None,
    };
    let mut frame = builder.frame(&spot, 60.0);
    frame.flares.clear();
    frame
}

/// Once a frame's lists have been seen twice nothing is made for a frame like it: no buffer, no
/// picture to draw into, and every list is found on the device. The picture is the same whether
/// a list lies among the frame's passing ones or in buffers of its own.
#[test]
fn a_frame_like_the_one_before_makes_nothing_new() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let frame = still_frame(&mut builder);
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let (first, second, third) = checked(&mut device, |d| {
        let picture = d.render(&frame, 320, 240, &mut load).expect("read back");
        d.take_work();
        d.render(&frame, 320, 240, &mut load).expect("read back");
        let second = d.take_work();
        let again = d.render(&frame, 320, 240, &mut load).expect("read back");
        assert_eq!(
            picture, again,
            "the picture does not depend on where a list lies"
        );
        (picture, second, d.take_work())
    });
    assert!(!first.rgba.is_empty());
    assert!(second.buffers > 0, "the lists seen twice get their own");
    assert!(third.mesh_hits > 100, "{third:?}");
    assert_eq!(third.mesh_misses, 0, "{third:?}");
    assert_eq!(third.buffers, 0, "{third:?}");
    assert_eq!(third.textures, 0, "{third:?}");
}

/// The sun's shadow map is drawn for the first frame and not again while its casters and its sun
/// stay as they were; a change in the sun's strength draws it again, the draws of the casters
/// coming back.
#[test]
fn the_shadow_map_is_drawn_again_only_when_it_would_differ() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let mut frame = still_frame(&mut builder);
    let casters = frame.shadow.as_ref().expect("a shadow map").casters.len();
    assert!(casters > 10, "{casters} casters");
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let (first, again, changed, picture_kept, picture_new) = checked(&mut device, |d| {
        d.render(&frame, 320, 240, &mut load).expect("read back");
        let first = d.take_work().draws;
        let picture_kept = d.render(&frame, 320, 240, &mut load).expect("read back");
        let again = d.take_work().draws;
        let map = frame.shadow.as_mut().expect("a shadow map");
        map.strength.r *= 0.5;
        let picture_new = d.render(&frame, 320, 240, &mut load).expect("read back");
        let changed = d.take_work().draws;
        (first, again, changed, picture_kept, picture_new)
    });
    assert_eq!(first as usize, again as usize + casters + 1);
    assert_eq!(changed, first);
    assert_ne!(
        picture_kept, picture_new,
        "the shadows take the new strength"
    );
}

/// A list that is new every frame, as a moving body's vertices are, makes no buffer of its own,
/// however many frames it goes on being new.
#[test]
fn a_list_new_each_frame_makes_no_buffer() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let mut frame = still_frame(&mut builder);
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let work = checked(&mut device, |d| {
        let mut work = Vec::new();
        for _ in 0..4 {
            let moving = frame.objects.first_mut().expect("an object");
            moving.vertices = std::sync::Arc::new(moving.vertices.to_vec());
            moving.indices = std::sync::Arc::new(moving.indices.to_vec());
            d.render(&frame, 320, 240, &mut load).expect("read back");
            work.push(d.take_work());
        }
        work
    });
    assert_eq!(work[3].buffers, 0, "{:?}", work[3]);
    assert_eq!(
        work[3].mesh_misses, 2,
        "the list and its indices: {:?}",
        work[3]
    );
}

/// With antialiasing off every pixel is sampled once, a frame with every kind of draw still goes
/// through with no complaint from the device, and the picture differs from the smoothed one only
/// along edges: most of it is the same.
#[test]
fn a_frame_is_drawn_with_antialiasing_off() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    builder.uptime = 30.0;
    let spot = Spot {
        block: (0xC1, 0x56),
        position: Vec3::new(122.842, 89.758, 81.6),
        heading: 90.0,
        pitch: 5.0,
        time_of_day: 0.40,
        day: None,
    };
    let frame = builder.frame(&spot, 60.0);
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let mut draw = |on: bool| {
        let mut device = renderer_antialiased(on);
        assert_eq!(device.samples > 1, on);
        checked(&mut device, |d| {
            d.render(&frame, 320, 240, &mut load).expect("read back")
        })
    };
    let smooth = draw(true);
    let sharp = draw(false);
    let same = smooth
        .rgba
        .chunks(4)
        .zip(sharp.rgba.chunks(4))
        .filter(|(a, b)| a.iter().zip(*b).all(|(x, y)| x.abs_diff(*y) <= 16))
        .count();
    assert!(same > 320 * 240 / 2, "{same} of {}", 320 * 240);
    assert_ne!(smooth.rgba, sharp.rgba, "the edges differ");
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

/// A frame with the human male standing in it, and the same batches of him.
fn frame_with_a_body(
    builder: &mut SceneBuilder<'_>,
    r: &auberean_world::records::WorldRecords,
) -> (
    auberean_draw::scene::Frame,
    Vec<auberean_draw::scene::ObjectBatch>,
) {
    let at = Vec3::new(127.0, 148.0, 79.5);
    let spot = Spot {
        block: (0xCB, 0x94),
        position: Vec3::new(at.x + 1.0, at.y - 3.5, at.z + 2.5),
        heading: -10.0,
        pitch: -30.0,
        time_of_day: 0.4,
        day: None,
    };
    let frame = builder.frame(&spot, 60.0);
    let recs = auberean_animation::CachedRecords::new(r);
    let figure = auberean_animation::body::Body::assemble(&recs, DataId(0x1F00_0023))
        .expect("the human male");
    let poses = figure.pose(&recs);
    let place = Mat4::from_parts(at, Quat::IDENTITY, Vec3::new(1.0, 1.0, 1.0));
    let batches = BodyDrawer::default().batches(r, &figure, &poses, &place);
    (frame, batches)
}

/// A body's places and facings in a stream of their own draw the picture its vertices written whole
/// draw, and the device has no complaint of the two streams.
#[test]
fn a_body_in_two_streams_is_the_picture_of_its_vertices_written_whole() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let (bare, batches) = frame_with_a_body(&mut builder, r);
    assert!(batches.iter().all(|b| b.posed.is_some()));
    let mut streams = bare.clone();
    streams.objects.extend(batches.iter().cloned());
    let mut whole = bare.clone();
    whole.objects.extend(batches.iter().map(|b| {
        let mut b = b.clone();
        let posed = b.posed.take().expect("its places");
        let vertices = b
            .vertices
            .iter()
            .zip(posed.iter())
            .map(|(v, p)| MeshVertex {
                position: p.position,
                normal: p.normal,
                ..*v
            })
            .collect();
        b.vertices = Arc::new(vertices);
        b
    }));
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let (a, b, none) = checked(&mut device, |d| {
        (
            d.render(&streams, 640, 480, &mut load).expect("read back"),
            d.render(&whole, 640, 480, &mut load).expect("read back"),
            d.render(&bare, 640, 480, &mut load).expect("read back"),
        )
    });
    assert_eq!(a, b, "the same picture");
    assert_ne!(a, none, "the body shows");
}

/// A body's places and facings make no buffer of their own, whether its list is new each frame or
/// stands for as long as its pose: only the mesh's own list, seen again, gets one.
#[test]
fn a_bodys_places_make_no_buffer() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let (bare, batches) = frame_with_a_body(&mut builder, r);
    let mut frame = bare.clone();
    frame.objects.extend(batches.iter().cloned());
    let body = bare.objects.len()..frame.objects.len();
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let (standing, moving) = checked(&mut device, |d| {
        for _ in 0..3 {
            d.render(&frame, 320, 240, &mut load).expect("read back");
        }
        d.take_work();
        d.render(&frame, 320, 240, &mut load).expect("read back");
        let standing = d.take_work();
        // A new list for each list of places, shared by its draws as they were.
        let mut new: HashMap<usize, Arc<Vec<PosedVertex>>> = HashMap::new();
        for b in &mut frame.objects[body.clone()] {
            let list = b.posed.as_ref().expect("its places");
            let moved = new
                .entry(Arc::as_ptr(list) as usize)
                .or_insert_with(|| Arc::new(list.to_vec()));
            b.posed = Some(moved.clone());
        }
        d.render(&frame, 320, 240, &mut load).expect("read back");
        (standing, d.take_work())
    });
    assert_eq!(standing.buffers, 0, "{standing:?}");
    assert_eq!(moving.buffers, 0, "{moving:?}");
    let lists = batches
        .iter()
        .map(auberean_draw::scene::ObjectBatch::places)
        .collect::<HashSet<_>>();
    assert_eq!(
        moving.mesh_misses as usize,
        lists.len(),
        "each list of places, and nothing else: {moving:?}"
    );
}
