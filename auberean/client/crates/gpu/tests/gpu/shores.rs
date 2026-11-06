//! The frame's alpha as the shores write it, and what the water and its foam make of it: the
//! world's frame holds alpha 127 of 255 but where a shore's strip lies, the water shows less of
//! itself there, the foam lies over the water's reflection, and a scene laid over the interface
//! keeps how much of its picture it covers.

use std::sync::Arc;

use auberean_draw::material::{d3d, switch, DrawLayer};
use auberean_draw::mesh::MeshVertex;
use auberean_draw::scene::{Camera, Cull, Frame, ObjectBatch, Reflection, SceneBuilder, Spot};
use auberean_gpu::Renderer;
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::math::Mat4;
use dereth_primitives::num::math;

use crate::common::{checked, install, renderer};

const W: usize = 320;
const H: usize = 240;

/// The camera: 2 m over the water, looking north a little down.
const CAMERA: Camera = Camera {
    position: Vec3 {
        x: 0.0,
        y: 0.0,
        z: 2.0,
    },
    heading: 0.0,
    pitch: -12.0,
    fov_y: 60.0,
    near: 0.1,
    far: 500.0,
    roll: 0.0,
    orthographic: None,
};

/// The strip's near and far edges, meters north of the camera: it covers the rows between.
const STRIP: (f32, f32) = (4.0, 14.0);

/// How far north the ground at row `row` of the picture lies, seen by [`CAMERA`] (none above
/// the horizon).
fn distance_at(row: usize) -> Option<f32> {
    #[allow(clippy::cast_precision_loss)]
    let y = (H as f32 / 2.0 - row as f32 - 0.5) / (H as f32 / 2.0);
    let up = math::atanf(y * math::tanf(30.0_f32.to_radians()));
    let down = 12.0_f32.to_radians() - up;
    (down > 0.0).then(|| 2.0 / math::tanf(down))
}

/// The camp's water, shore and foam layers, as its frame draws them.
struct Layers {
    water: ObjectBatch,
    shore: ObjectBatch,
    foam: ObjectBatch,
}

fn camp_layers(builder: &mut SceneBuilder<'_>) -> Layers {
    let camp = Spot {
        block: (0xC1, 0x56),
        position: Vec3::new(122.842, 89.758, 81.6),
        heading: 90.0,
        pitch: 5.0,
        time_of_day: 0.45,
        day: None,
    };
    let frame = builder.frame(&camp, 60.0);
    Layers {
        water: frame.water.first().expect("the pond").clone(),
        shore: frame.shore.first().expect("the pond's shore").clone(),
        foam: frame.foam.first().expect("the pond's foam").clone(),
    }
}

fn vertex(p: [f32; 3], uv: [f32; 2], edge: f32) -> MeshVertex {
    MeshVertex {
        position: p,
        normal: [0.0, 0.0, 1.0],
        diffuse: 0xFFFF_FFFF,
        uv: [uv, [uv[0], edge], uv, uv],
        tangent_u: [1.0, 0.0, 0.0],
        tangent_v: [0.0, 1.0, 0.0],
        to_light: [0.0; 3],
    }
}

/// A flat square at height `z` from `y0` (its near edge, the second coordinate set's 0) to `y1`
/// (its far edge, 1), 160 m wide, its two triangles once.
fn flat(base: &ObjectBatch, z: f32, y0: f32, y1: f32, layer: DrawLayer) -> ObjectBatch {
    let mut b = base.clone();
    b.vertices = Arc::new(vec![
        vertex([-80.0, y0, z], [0.0, 0.0], 0.0),
        vertex([80.0, y0, z], [16.0, 0.0], 0.0),
        vertex([-80.0, y1, z], [0.0, 1.0], 1.0),
        vertex([80.0, y1, z], [16.0, 1.0], 1.0),
    ]);
    b.indices = Arc::new(vec![0, 1, 2, 1, 3, 2]);
    b.transform = Mat4::IDENTITY;
    b.layer = layer;
    b.layer.cull = d3d::CULL_NONE;
    b
}

/// A layer of one color, unlit, as `blend` blends it.
fn plain(base: &DrawLayer, color: Color, blend: (u32, u32)) -> DrawLayer {
    let mut l = base.clone();
    l.stages.clear();
    l.switches = switch::MATERIAL_DIFFUSE;
    l.diffuse = color;
    l.source_blend = blend.0;
    l.dest_blend = blend.1;
    l.light_variants.clear();
    l.motion = None;
    l.pixel_program = None;
    l.alpha_test = false;
    l
}

/// An empty frame seen by [`CAMERA`] in the camp's light.
fn empty_frame(builder: &mut SceneBuilder<'_>) -> Frame {
    let camp = Spot {
        block: (0xC1, 0x56),
        position: Vec3::new(122.842, 89.758, 81.6),
        heading: 0.0,
        pitch: 0.0,
        time_of_day: 0.45,
        day: None,
    };
    let day = builder.day(&camp);
    let light = builder.light(&camp, day.as_ref());
    builder.parts_frame(&[], Vec::new(), CAMERA, light, None)
}

/// Read the picture `texture` holds back as it stands, alpha and all.
fn read_raw(device: &Renderer, texture: &wgpu::Texture) -> Vec<u8> {
    let (w, h) = (texture.width(), texture.height());
    let row = (w * 4).div_ceil(256) * 256;
    let buffer = device.device().create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    device.queue().submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.expect("mapped"));
    device
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("the device answers");
    let data = slice.get_mapped_range().expect("the mapped range");
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let s = (y * row) as usize;
        out.extend_from_slice(&data[s..s + (w * 4) as usize]);
    }
    out
}

/// The mean of channel `ch` of picture `p` over columns `x` of row `row`.
fn row_mean(p: &[u8], ch: usize, x: std::ops::Range<usize>, row: usize) -> f64 {
    let n = x.len();
    #[allow(clippy::cast_precision_loss)]
    let mean = x.map(|c| f64::from(p[(row * W + c) * 4 + ch])).sum::<f64>() / n as f64;
    mean
}

/// The world's frame holds alpha 127 of 255 under everything it draws (an opaque ground drawn
/// over it leaves it so): the device writes color alone. Only a shore's strip writes it, alone,
/// as its picture's alpha times half: across the strip the frame's alpha follows the picture from
/// the waterline out.
///
/// Behaviour: draw.water.the-frame-keeps-alpha-127-but-where-a-shore-lies
#[test]
fn the_frame_keeps_alpha_127_but_where_a_shore_lies() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let camp = camp_layers(&mut builder);
    let mut frame = empty_frame(&mut builder);
    let gray = plain(
        &camp.water.layer,
        Color::new(0.5, 0.5, 0.5, 1.0),
        (d3d::BLEND_ONE, d3d::BLEND_ZERO),
    );
    frame.objects = vec![flat(&camp.water, -1.0, -20.0, 140.0, gray)];
    frame.shore = vec![flat(
        &camp.shore,
        0.0,
        STRIP.0,
        STRIP.1,
        camp.shore.layer.clone(),
    )];
    let picture = auberean_draw::picture::load(r, camp.shore.layer.stages[0].texture)
        .expect("the shore's picture");
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let p = checked(&mut device, |d| {
        let t = d.draw(
            &frame,
            u32::try_from(W).expect("a width"),
            u32::try_from(H).expect("a height"),
            &mut load,
        );
        read_raw(d, &t)
    });
    // the picture's alpha across the strip (its middle column), from the waterline (v = 0) out
    let across = |v: f32| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let row = ((v * picture.height as f32) as usize).min(picture.height as usize - 1);
        f64::from(picture.bgra[(row * picture.width as usize + picture.width as usize / 2) * 4 + 3])
    };
    let (mut inside, mut outside) = (Vec::new(), Vec::new());
    for row in H / 2..H {
        let Some(d) = distance_at(row) else { continue };
        let a = row_mean(&p, 3, W / 4..3 * W / 4, row);
        let v = (d - STRIP.0) / (STRIP.1 - STRIP.0);
        if (0.1..0.9).contains(&v) {
            let want = across(v) * 127.0 / 255.0;
            println!("row {row}: {d:.2} m, v {v:.2}: alpha {a:.1}, the picture's half {want:.1}");
            inside.push((a, want));
        } else if !(-0.1..1.1).contains(&v) {
            outside.push(a);
        }
    }
    assert!(inside.len() > 20 && outside.len() > 20);
    for a in &outside {
        assert!((a - 127.0).abs() <= 1.0, "alpha {a} off the strip");
    }
    for (a, want) in &inside {
        assert!(
            (a - want).abs() <= 8.0,
            "alpha {a}, the picture's half {want}"
        );
    }
    let (lo, hi) = inside
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), (a, _)| {
            (lo.min(*a), hi.max(*a))
        });
    assert!(hi - lo > 40.0, "the alpha follows the picture: {lo}..{hi}");
}

/// Across a shore's strip the water shows its own color by the frame's alpha there, less of it
/// near the waterline: the ground under the water shows through more there, and where the strip
/// ends half and half again.
///
/// Behaviour: draw.water.the-water-shows-less-of-itself-along-the-shore
#[test]
fn the_water_shows_less_of_itself_along_the_shore() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let camp = camp_layers(&mut builder);
    let base = empty_frame(&mut builder);
    let orange = plain(
        &camp.water.layer,
        Color::new(0.8, 0.55, 0.3, 1.0),
        (d3d::BLEND_ONE, d3d::BLEND_ZERO),
    );
    let ground = flat(&camp.water, -1.0, -20.0, 140.0, orange);
    let water = flat(&camp.water, 0.0, -20.0, 140.0, camp.water.layer.clone());
    let mut alone = water.clone();
    alone.layer.source_blend = d3d::BLEND_ONE;
    alone.layer.dest_blend = d3d::BLEND_ZERO;
    let strip = flat(&camp.shore, 0.0, STRIP.0, STRIP.1, camp.shore.layer.clone());
    let with = |w: &ObjectBatch, shore: bool| {
        let mut f = base.clone();
        f.objects = vec![ground.clone()];
        f.water = vec![w.clone()];
        f.shore = if shore {
            vec![strip.clone()]
        } else {
            Vec::new()
        };
        f
    };
    let frames = [with(&water, true), with(&alone, false), {
        let mut f = base.clone();
        f.objects = vec![ground.clone()];
        f
    }];
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let p = checked(&mut device, |d| {
        frames.each_ref().map(|f| {
            d.render(
                f,
                u32::try_from(W).expect("a width"),
                u32::try_from(H).expect("a height"),
                &mut load,
            )
            .expect("read back")
            .rgba
        })
    });
    // the water's share per row, in its red channel (the water and the ground differ most there)
    let mut near_shore = f64::MAX;
    for row in H / 2..H {
        let Some(d) = distance_at(row) else { continue };
        let [a, w, g] = p.each_ref().map(|q| row_mean(q, 0, W / 4..3 * W / 4, row));
        let share = (a - g) / (w - g);
        let v = (d - STRIP.0) / (STRIP.1 - STRIP.0);
        println!("row {row}: {d:.2} m, v {v:.2}: the water's share {share:.3}");
        if !(-0.1..1.1).contains(&v) {
            assert!(
                (0.47..0.53).contains(&share),
                "off the strip: share {share:.3}"
            );
        } else if (0.0..0.3).contains(&v) {
            near_shore = near_shore.min(share);
        }
    }
    assert!(
        near_shore < 0.35,
        "near the waterline the water shows less of itself: {near_shore:.3}"
    );
}

/// The foam is drawn after the water's reflection, over it: an opaque red foam over a white wall's
/// reflection shows red alone, not half covered by the reflection.
///
/// Behaviour: draw.water.the-foam-lies-over-the-waters-reflection
#[test]
fn the_foam_lies_over_the_waters_reflection() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let camp = camp_layers(&mut builder);
    let mut frame = empty_frame(&mut builder);
    let white = plain(
        &camp.water.layer,
        Color::WHITE,
        (d3d::BLEND_ONE, d3d::BLEND_ZERO),
    );
    // a white wall standing 20 m ahead from 1 m to 6 m over the water, and its reflection
    let mut bar = flat(&camp.water, 0.0, 0.0, 1.0, white);
    bar.vertices = Arc::new(vec![
        vertex([-80.0, 20.0, 1.0], [0.0, 0.0], 0.0),
        vertex([80.0, 20.0, 1.0], [1.0, 0.0], 0.0),
        vertex([-80.0, 20.0, 6.0], [0.0, 1.0], 1.0),
        vertex([80.0, 20.0, 6.0], [1.0, 1.0], 1.0),
    ]);
    bar.anchor = Some(Vec3::new(0.0, 0.0, 1.0));
    frame.objects = vec![bar];
    frame.water = vec![flat(
        &camp.water,
        0.0,
        -20.0,
        140.0,
        camp.water.layer.clone(),
    )];
    frame.reflection = Some(Reflection {
        height: 0.0,
        ripples: false,
    });
    let red = plain(
        &camp.foam.layer,
        Color::new(1.0, 0.0, 0.0, 1.0),
        (d3d::BLEND_ONE, d3d::BLEND_ZERO),
    );
    // the foam where the wall's reflection lies (its foot's 13.3 m north, its top's 5 m)
    let bare = frame.clone();
    let mut foam = flat(&camp.foam, 0.0, 6.0, 12.0, red);
    // the red itself, not the foam's black draw color
    foam.tint = Color::WHITE;
    frame.foam = vec![foam];
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let [p, without] = checked(&mut device, |d| {
        [&frame, &bare].map(|f| {
            d.render(
                f,
                u32::try_from(W).expect("a width"),
                u32::try_from(H).expect("a height"),
                &mut load,
            )
            .expect("read back")
            .rgba
        })
    });
    let mut rows = 0;
    for row in 0..H {
        let Some(d) = distance_at(row) else { continue };
        if !(6.5..11.5).contains(&d) {
            continue;
        }
        let rgb = [0, 1, 2].map(|ch| row_mean(&p, ch, W / 4..3 * W / 4, row));
        let bare = [0, 1, 2].map(|ch| row_mean(&without, ch, W / 4..3 * W / 4, row));
        println!("row {row}: {d:.2} m: {rgb:.1?}, without the foam {bare:.1?}");
        assert!(
            bare[1] > 120.0,
            "the wall's reflection lies there: {bare:?}"
        );
        assert!(
            rgb[0] > 250.0 && rgb[1] < 5.0 && rgb[2] < 5.0,
            "the foam over the reflection: {rgb:?}"
        );
        rows += 1;
    }
    assert!(rows >= 3, "{rows} rows");
}

/// A scene laid over the interface (a panel's viewport) covers the interface only where it draws:
/// its picture's alpha is whole where an opaque thing stands, half where a thing half seen
/// through stands alone, and nothing elsewhere, so the panels behind show through.
///
/// Behaviour: draw.viewport.a-viewports-scene-covers-only-where-it-draws
#[test]
fn a_viewports_scene_covers_only_where_it_draws() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let camp = camp_layers(&mut builder);
    let mut frame = empty_frame(&mut builder);
    let opaque = plain(
        &camp.water.layer,
        Color::new(0.2, 0.6, 0.9, 1.0),
        (d3d::BLEND_ONE, d3d::BLEND_ZERO),
    );
    let half = plain(
        &camp.water.layer,
        Color::new(0.9, 0.2, 0.2, 0.5),
        (d3d::BLEND_SRC_ALPHA, d3d::BLEND_INV_SRC_ALPHA),
    );
    // an opaque square to the left of the middle, one half seen through to the right
    let square = |x0: f32, layer: DrawLayer| {
        let mut b = flat(&camp.water, 0.0, 0.0, 1.0, layer);
        b.vertices = Arc::new(vec![
            vertex([x0, 20.0, 0.0], [0.0, 0.0], 0.0),
            vertex([x0 + 4.0, 20.0, 0.0], [1.0, 0.0], 0.0),
            vertex([x0, 20.0, 4.0], [0.0, 1.0], 1.0),
            vertex([x0 + 4.0, 20.0, 4.0], [1.0, 1.0], 1.0),
        ]);
        b
    };
    frame.objects = vec![square(-5.0, opaque)];
    let mut seen_through = square(1.0, half);
    seen_through.seen_through = true;
    frame.objects.push(seen_through);
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let p = checked(&mut device, |d| {
        d.render_laid_over(
            &frame,
            u32::try_from(W).expect("a width"),
            u32::try_from(H).expect("a height"),
            &mut load,
        )
        .expect("read back")
        .rgba
    });
    let alpha = |x: usize, y: usize| p[(y * W + x) * 4 + 3];
    // the squares stand about rows 50 to 100; the left one about columns 105 to 145, the right
    // one 175 to 215
    let (a_opaque, a_half, a_none) = (alpha(125, 80), alpha(195, 80), alpha(160, 200));
    println!("coverage: opaque {a_opaque}, half {a_half}, none {a_none}");
    assert_eq!(a_opaque, 255);
    assert!((120..=136).contains(&a_half), "half: {a_half}");
    assert_eq!(a_none, 0);
    assert_eq!(alpha(5, 5), 0);
}

/// An object standing under the water's plane shows in the reflection only past the water: a wall
/// standing from under the water to above it reflects its part above the water; a wall wholly under
/// it reflects nothing.
///
/// Behaviour: draw.water.an-object-under-the-water-reflects-only-past-it
#[test]
fn an_object_under_the_water_reflects_only_past_it() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let camp = camp_layers(&mut builder);
    let white = plain(
        &camp.water.layer,
        Color::WHITE,
        (d3d::BLEND_ONE, d3d::BLEND_ZERO),
    );
    println!("the water's culling: {}", camp.water.layer.cull);
    let wall = |z0: f32, z1: f32| {
        let mut bar = flat(&camp.water, 0.0, 0.0, 1.0, white.clone());
        bar.vertices = Arc::new(vec![
            vertex([-80.0, 20.0, z0], [0.0, 0.0], 0.0),
            vertex([80.0, 20.0, z0], [1.0, 0.0], 0.0),
            vertex([-80.0, 20.0, z1], [0.0, 1.0], 1.0),
            vertex([80.0, 20.0, z1], [1.0, 1.0], 1.0),
        ]);
        // it stands under the plane
        bar.anchor = Some(Vec3::new(0.0, 20.0, -0.5));
        bar
    };
    let mut frame = empty_frame(&mut builder);
    frame.water = vec![flat(
        &camp.water,
        0.0,
        -20.0,
        140.0,
        camp.water.layer.clone(),
    )];
    frame.reflection = Some(Reflection {
        height: 0.0,
        ripples: false,
    });
    // A black floor just under the water, so the frame itself sees nothing below it.
    let black = plain(
        &camp.water.layer,
        Color::new(0.0, 0.0, 0.0, 1.0),
        (d3d::BLEND_ONE, d3d::BLEND_ZERO),
    );
    let mut floor = flat(&camp.water, -0.01, -20.0, 140.0, black);
    floor.anchor = Some(Vec3::new(0.0, 0.0, -0.01));
    let mut bare = frame.clone();
    bare.objects = vec![floor.clone()];
    let mut out = frame.clone();
    out.objects = vec![floor.clone(), wall(-4.0, 6.0)];
    let mut under = frame.clone();
    under.objects = vec![floor, wall(-6.0, -1.0)];
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let [none, past, hidden] = checked(&mut device, |d| {
        [&bare, &out, &under].map(|f| {
            d.render(
                f,
                u32::try_from(W).expect("a width"),
                u32::try_from(H).expect("a height"),
                &mut load,
            )
            .expect("read back")
            .rgba
        })
    });
    // The wall's reflection above the water lies on the water's near part (its foot's 20 m north;
    // its top seen at about 5 m); the wall itself is drawn farther up the picture.
    let mut rows = 0;
    for row in 0..H {
        let Some(d) = distance_at(row) else { continue };
        if !(6.5..13.0).contains(&d) {
            continue;
        }
        let at = |p: &[u8]| row_mean(p, 1, W / 4..3 * W / 4, row);
        println!(
            "row {row}: {d:.2} m: {:.1} bare, {:.1} past, {:.1} under",
            at(&none),
            at(&past),
            at(&hidden)
        );
        assert!(at(&past) > at(&none) + 20.0, "row {row}: the reflection");
        assert!((at(&hidden) - at(&none)).abs() < 2.0, "row {row}: nothing");
        rows += 1;
    }
    assert!(rows > 3, "{rows} rows");
}

/// A cell's mesh seen from the land is not in the water's reflection, however it stands: the
/// reflection gathers what stands on the land alone.
///
/// Behaviour: draw.water.a-rooms-draws-are-not-in-the-reflection
#[test]
fn a_rooms_draws_are_not_in_the_reflection() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let camp = camp_layers(&mut builder);
    let white = plain(
        &camp.water.layer,
        Color::WHITE,
        (d3d::BLEND_ONE, d3d::BLEND_ZERO),
    );
    let mut wall = flat(&camp.water, 0.0, 0.0, 1.0, white);
    wall.vertices = Arc::new(vec![
        vertex([-80.0, 20.0, 1.0], [0.0, 0.0], 0.0),
        vertex([80.0, 20.0, 1.0], [1.0, 0.0], 0.0),
        vertex([-80.0, 20.0, 6.0], [0.0, 1.0], 1.0),
        vertex([80.0, 20.0, 6.0], [1.0, 1.0], 1.0),
    ]);
    wall.anchor = Some(Vec3::new(0.0, 20.0, 1.0));
    let mut frame = empty_frame(&mut builder);
    frame.water = vec![flat(
        &camp.water,
        0.0,
        -20.0,
        140.0,
        camp.water.layer.clone(),
    )];
    frame.reflection = Some(Reflection {
        height: 0.0,
        ripples: false,
    });
    let bare = frame.clone();
    let mut land = frame.clone();
    land.objects = vec![wall.clone()];
    let mut room = frame.clone();
    wall.cull = Cull::InCell {
        cell: 0x100,
        lo: Vec3::new(-80.0, 20.0, 1.0),
        hi: Vec3::new(80.0, 20.0, 6.0),
    };
    room.objects = vec![wall];
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let [none, on_land, in_room] = checked(&mut device, |d| {
        [&bare, &land, &room].map(|f| {
            d.render(
                f,
                u32::try_from(W).expect("a width"),
                u32::try_from(H).expect("a height"),
                &mut load,
            )
            .expect("read back")
            .rgba
        })
    });
    let row = (0..H)
        .find(|&r| distance_at(r).is_some_and(|d| (7.0..12.0).contains(&d)))
        .expect("a row of water");
    let at = |p: &[u8]| row_mean(p, 1, W / 4..3 * W / 4, row);
    assert!(at(&on_land) > at(&none) + 20.0, "on the land it reflects");
    assert!(
        (at(&in_room) - at(&none)).abs() < 2.0,
        "in a room it does not"
    );
}
