//! How the device takes textures as the graphics preferences say: a texture drawn from the level
//! its detail starts at (the textures loaded again when the detail changes), its levels blended by
//! trilinear filtering, and a slanted texture drawn from larger levels with anisotropic filtering.
//! Fixture: a made-up picture list of eight levels, each one gray, lighter the larger the level.

use std::sync::Arc;

use auberean_draw::material::{d3d, switch, DrawLayer, Stage, UV_IDENTITY};
use auberean_draw::mesh::MeshVertex;
use auberean_draw::picture::Image;
use auberean_draw::scene::{Camera, Frame, SceneBuilder, Spot, Textures};
use auberean_gpu::Renderer;
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::math::Mat4;
use auberean_world::sky::DayLight;

use crate::common::{checked, install, renderer};

const W: u32 = 320;
const H: u32 = 240;

/// The made-up picture list's id.
const LIST: u32 = 0x4000_7777;

/// The gray of each level, largest first.
const GRAYS: [u8; 8] = [255, 220, 185, 150, 115, 80, 45, 10];

/// The picture list: 128 texels a side, then each level half the one before, down to one.
fn list() -> Image {
    let level = |k: usize| {
        let side = 128u32 >> k;
        let g = GRAYS[k];
        Image {
            width: side,
            height: side,
            bgra: [g, g, g, 255].repeat((side * side) as usize),
            smaller: Vec::new(),
        }
    };
    Image {
        smaller: (1..8).map(level).collect(),
        ..level(0)
    }
}

fn vertex(p: [f32; 3], uv: [f32; 2]) -> MeshVertex {
    MeshVertex {
        position: p,
        normal: [0.0, 0.0, 1.0],
        diffuse: 0xFFFF_FFFF,
        uv: [uv; 4],
        tangent_u: [1.0, 0.0, 0.0],
        tangent_v: [0.0, 1.0, 0.0],
        to_light: [0.0; 3],
    }
}

/// An unlit layer showing the picture list as it stands.
fn shown(base: &DrawLayer) -> DrawLayer {
    let mut l = base.clone();
    l.stages = vec![Stage {
        texture: LIST,
        color_op: d3d::OP_SELECT_ARG1,
        color_arg1: d3d::ARG_TEXTURE,
        color_arg2: d3d::ARG_DIFFUSE,
        alpha_op: d3d::OP_SELECT_ARG1,
        alpha_arg1: d3d::ARG_TEXTURE,
        alpha_arg2: d3d::ARG_DIFFUSE,
        clamp_u: false,
        clamp_v: false,
        tex_coord: 0,
        uv_transform: UV_IDENTITY,
    }];
    l.switches = switch::MATERIAL_DIFFUSE;
    l.diffuse = Color::WHITE;
    l.source_blend = d3d::BLEND_ONE;
    l.dest_blend = d3d::BLEND_ZERO;
    l.cull = d3d::CULL_NONE;
    l.light_variants.clear();
    l.motion = None;
    l.pixel_program = None;
    l.alpha_test = false;
    l
}

/// A frame of one quad (its corners, each with its coordinates) showing the picture list, seen
/// by `camera` with no fog.
fn frame_of(corners: [([f32; 3], [f32; 2]); 4], camera: Camera) -> Frame {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let camp = Spot {
        block: (0xC1, 0x56),
        position: Vec3::new(122.842, 89.758, 81.6),
        heading: 90.0,
        pitch: 5.0,
        time_of_day: 0.45,
        day: None,
    };
    let base = builder
        .frame(&camp, 60.0)
        .water
        .first()
        .expect("the camp's pond")
        .clone();
    let mut quad = base.clone();
    quad.vertices = Arc::new(corners.map(|(p, uv)| vertex(p, uv)).to_vec());
    quad.indices = Arc::new(vec![0, 1, 2, 1, 3, 2]);
    quad.transform = Mat4::IDENTITY;
    quad.layer = shown(&base.layer);
    let mut frame = builder.parts_frame(&[], Vec::new(), camera, DayLight::DEFAULT, None);
    frame.objects = vec![quad];
    frame
}

/// Draw `frame` and read back its grays (the red channel), row by row.
fn grays(device: &mut Renderer, frame: &Frame) -> Vec<u8> {
    let mut load = |id: u32| (id == LIST).then(list);
    checked(device, |d| {
        d.render(frame, W, H, &mut load)
            .expect("read back")
            .rgba
            .chunks(4)
            .map(|p| p[0])
            .collect()
    })
}

/// A wall 2 m ahead of the camera, head on, its picture list repeated every 2 m: its texels
/// larger than the pixels at every detail.
fn wall() -> Frame {
    let k = 0.5;
    let at = |x: f32, z: f32| ([x, 2.0, z], [x * k, z * k]);
    frame_of(
        [at(-3.0, -1.0), at(3.0, -1.0), at(-3.0, 5.0), at(3.0, 5.0)],
        Camera {
            position: Vec3::new(0.0, 0.0, 2.0),
            heading: 0.0,
            pitch: 0.0,
            fov_y: 60.0,
            near: 0.1,
            far: 500.0,
            orthographic: None,
        },
    )
}

/// The device draws a texture from the level its detail starts at: a wall seen close shows the
/// largest level at very high detail, the second at high and the fifth at very low, the device's
/// textures loaded again each time the detail changes.
///
/// Behaviour: draw.picture.the-device-draws-a-texture-from-the-level-its-detail-starts-at
#[test]
fn the_device_draws_a_texture_from_the_level_its_detail_starts_at() {
    let mut device = renderer();
    let mut frame = wall();
    let middle = (H / 2 * W + W / 2) as usize;
    let mut at = |detail: u32| {
        frame.textures = Textures {
            detail,
            ..Textures::default()
        };
        grays(&mut device, &frame)[middle]
    };
    assert_eq!(at(4), GRAYS[0]);
    assert_eq!(at(3), GRAYS[1]);
    assert_eq!(at(0), GRAYS[4]);
    assert_eq!(at(4), GRAYS[0], "back to the largest");
}

/// A floor reaching 400 m north of a camera a meter over it, looking a little down, its picture
/// list repeated every meter: farther off its texture is drawn from smaller and smaller levels.
fn floor() -> Frame {
    let at = |x: f32, y: f32| ([x, y, 0.0], [x, y]);
    frame_of(
        [
            at(-400.0, 1.0),
            at(400.0, 1.0),
            at(-400.0, 400.0),
            at(400.0, 400.0),
        ],
        Camera {
            position: Vec3::new(0.0, 0.0, 1.0),
            heading: 0.0,
            pitch: -10.0,
            fov_y: 60.0,
            near: 0.1,
            far: 1000.0,
            orthographic: None,
        },
    )
}

/// The floor's rows well below the horizon (the horizon is about row 83).
const FLOOR_ROWS: std::ops::Range<u32> = 100..230;

/// With trilinear filtering a texture blends between its two nearest levels; without it each
/// texel comes from the nearest level alone: the floor shows only the levels' own grays. A device
/// keeps the filtering its first frame asked for: turned on later, the floor stays unblended.
///
/// Behaviour: draw.picture.trilinear-filtering-blends-between-a-textures-levels
#[test]
fn trilinear_filtering_blends_between_a_textures_levels() {
    let mut frame = floor();
    let mut between = |device: &mut Renderer, trilinear: bool| {
        frame.textures = Textures {
            detail: 4,
            trilinear,
            anisotropic: false,
        };
        let g = grays(device, &frame);
        let rows = FLOOR_ROWS.flat_map(|y| (0..W).map(move |x| (y * W + x) as usize));
        let (mut n, mut mixed) = (0u32, 0u32);
        for k in rows {
            n += 1;
            if GRAYS.iter().all(|&v| g[k].abs_diff(v) > 3) {
                mixed += 1;
            }
        }
        f64::from(mixed) / f64::from(n)
    };
    let mut started_off = renderer();
    let off = between(&mut started_off, false);
    let later = between(&mut started_off, true);
    drop(started_off);
    let on = between(&mut renderer(), true);
    assert!(off < 0.01, "nearest level only: {off}");
    assert!(later < 0.01, "kept from the first frame: {later}");
    assert!(on > 0.2, "blended: {on}");
}

/// With anisotropic filtering a texture seen at a slant is drawn from larger levels than without
/// it: the far floor is lighter (its larger levels' grays).
///
/// Behaviour: draw.picture.anisotropic-filtering-draws-a-slanted-texture-from-larger-levels
#[test]
fn anisotropic_filtering_draws_a_slanted_texture_from_larger_levels() {
    let mut device = renderer();
    let mut frame = floor();
    let mut mean = |anisotropic: bool, rows: std::ops::Range<u32>| {
        frame.textures = Textures {
            detail: 4,
            trilinear: true,
            anisotropic,
        };
        let g = grays(&mut device, &frame);
        let n = (rows.end - rows.start) * W;
        let sum: u32 = rows
            .flat_map(|y| (0..W).map(move |x| (y * W + x) as usize))
            .map(|k| u32::from(g[k]))
            .sum();
        f64::from(sum) / f64::from(n)
    };
    let far = 100..140;
    let (off, on) = (mean(false, far.clone()), mean(true, far));
    assert!(on > off + 30.0, "far: {on} with, {off} without");
}
