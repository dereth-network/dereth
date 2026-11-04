//! How the device draws a faded object: a layer blending by alpha shows what is behind it at its
//! tint's alpha and marks no depth once it is faded enough; a layer that neither tests nor blends
//! by alpha draws whole until the object is gone. Fixture: made-up flat walls before a black
//! backdrop, seen with no fog.

use std::sync::Arc;

use auberean_draw::material::{d3d, switch, DrawLayer};
use auberean_draw::mesh::MeshVertex;
use auberean_draw::scene::{Camera, Cull, Frame, ObjectBatch, PointLights, SceneBuilder};
use auberean_gpu::Renderer;
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::math::Mat4;
use auberean_world::sky::DayLight;

use crate::common::{checked, install, renderer};

const W: u32 = 64;
const H: u32 = 64;

/// A wall `y` meters north of the camera, filling the picture, lit by nothing but its layer's
/// diffuse color, twice gray `g` (a layer with no stages doubles its color).
pub(crate) fn wall(y: f32, g: f32, blended: bool, anchor: f32) -> ObjectBatch {
    let vertex = |x: f32, z: f32| MeshVertex {
        position: [x, y, z],
        normal: [0.0, 0.0, 1.0],
        diffuse: 0xFFFF_FFFF,
        uv: [[0.0; 2]; 4],
        tangent_u: [1.0, 0.0, 0.0],
        tangent_v: [0.0, 1.0, 0.0],
        to_light: [0.0; 3],
    };
    let layer = DrawLayer {
        stages: Vec::new(),
        source_blend: if blended {
            d3d::BLEND_SRC_ALPHA
        } else {
            d3d::BLEND_ONE
        },
        dest_blend: if blended {
            d3d::BLEND_INV_SRC_ALPHA
        } else {
            d3d::BLEND_ZERO
        },
        depth_test: 4,
        depth_write: true,
        cull: d3d::CULL_NONE,
        alpha_test: blended,
        alpha_ref: 0.0,
        tests_alpha: false,
        switches: switch::MATERIAL_DIFFUSE,
        ambient: Color::WHITE,
        diffuse: Color::new(g, g, g, 1.0),
        specular: Color::BLACK,
        specular_power: 0.0,
        pixel_program: None,
        light_variants: vec![(0, 0)],
        motion: None,
    };
    ObjectBatch {
        posed: None,
        vertices: Arc::new(vec![
            vertex(-100.0, -100.0),
            vertex(100.0, -100.0),
            vertex(-100.0, 100.0),
            vertex(100.0, 100.0),
        ]),
        indices: Arc::new(vec![0, 1, 2, 1, 3, 2]),
        transform: Mat4::IDENTITY,
        layer,
        tint: Color::WHITE,
        points: PointLights::NONE,
        mesh_type: 0,
        shade: None,
        outside: false,
        cull: Cull::Never,
        mesh: 0,
        casts_volume: false,
        caster: None,
        seen_through: false,
        kept_by: None,
        anchor: Some(Vec3::new(0.0, anchor, 0.0)),
        step: None,
        visible: Some(1000.0),
        block: None,
        far: false,
        ground: None,
        lod: None,
    }
}

fn frame_of(objects: Vec<ObjectBatch>) -> Frame {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let camera = Camera {
        position: Vec3::ZERO,
        heading: 0.0,
        pitch: 0.0,
        fov_y: 60.0,
        near: 0.1,
        far: 500.0,
        orthographic: None,
    };
    let mut frame = builder.parts_frame(&[], Vec::new(), camera, DayLight::DEFAULT, None);
    frame.objects = objects;
    frame
}

pub(crate) fn middle(device: &mut Renderer, frame: &Frame) -> u8 {
    let mut load = |_: u32| None;
    checked(device, |d| {
        let p = d.render(frame, W, H, &mut load).expect("read back");
        p.rgba[((H / 2 * W + W / 2) * 4) as usize]
    })
}

/// A wall blending by alpha shows the wall behind it at its tint's alpha; drawn before that wall
/// it marks depth (and hides it) while its alpha holds at 0.8 and up, and no longer does under it
/// or, far off, under one. A wall neither testing nor blending by alpha draws whole however faded,
/// until nothing is left of it.
///
/// Behaviour: draw.mesh.the-device-draws-a-faded-object-through
#[test]
fn the_device_draws_a_faded_object_through() {
    let mut device = renderer();
    // A white wall at 10 m, blended, half faded, over a gray wall at 20 m drawn first.
    let mut near = wall(10.0, 1.0, true, 10.0);
    near.tint.a = 0.5;
    let behind = wall(20.0, 0.2, false, 20.0);
    let over = middle(&mut device, &frame_of(vec![behind.clone(), near.clone()]));
    let want = 0.5_f32.mul_add(1.0, 0.5 * 0.4) * 255.0;
    assert!(
        (f32::from(over) - want).abs() < 6.0,
        "{over} against {want}"
    );

    // Drawn first (reckoned farther off), it leaves the wall behind it to show unless it marks
    // depth.
    let first = |alpha: f32, far: bool| {
        let mut n = wall(10.0, 1.0, true, 100.0);
        n.tint.a = alpha;
        n.far = far;
        n
    };
    let gray = (0.4_f32 * 255.0).round();
    let seen = |device: &mut Renderer, alpha: f32, far: bool| {
        f32::from(middle(
            device,
            &frame_of(vec![first(alpha, far), behind.clone()]),
        ))
    };
    let held = seen(&mut device, 0.85, false);
    assert!((held - 0.85 * 255.0).abs() < 6.0, "marks depth: {held}");
    let through = seen(&mut device, 0.7, false);
    assert!(
        (through - gray).abs() < 6.0,
        "under 0.8 the wall behind draws over it: {through}"
    );
    let far = seen(&mut device, 0.85, true);
    assert!(
        (far - gray).abs() < 6.0,
        "far off, under 1 it marks none: {far}"
    );

    // An opaque layer ignores the fade; gone, it is not drawn.
    let mut solid = wall(10.0, 1.0, false, 10.0);
    solid.tint.a = 0.1;
    let shown = middle(&mut device, &frame_of(vec![solid.clone()]));
    assert!(shown > 250, "{shown}");
    solid.tint.a = 0.0;
    let gone = middle(&mut device, &frame_of(vec![solid]));
    assert!(gone < shown, "{gone}");
}
