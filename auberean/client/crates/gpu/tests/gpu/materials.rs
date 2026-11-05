//! Materials as the device draws them.

use auberean_draw::scene::{Camera, Frame, SceneBuilder, Spot};
use auberean_primitives::{DataId, Vec3};
use auberean_world::math::Mat4;
use auberean_world::place::parts_of;

use crate::common::{checked, install, renderer};

/// The middle of the box about every vertex the frame's objects draw, and its largest side.
fn extent(frame: &Frame) -> (Vec3, f32) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for b in &frame.objects {
        for v in b.vertices.iter() {
            let p = b
                .transform
                .point(Vec3::new(v.position[0], v.position[1], v.position[2]));
            for (k, c) in [p.x, p.y, p.z].into_iter().enumerate() {
                lo[k] = lo[k].min(c);
                hi[k] = hi[k].max(c);
            }
        }
    }
    let mid = Vec3::new(
        0.5 * (lo[0] + hi[0]),
        0.5 * (lo[1] + hi[1]),
        0.5 * (lo[2] + hi[2]),
    );
    let side = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]);
    (mid, side)
}

/// A shiny surface shows its environment map: the object placed by the landscape whose mesh's
/// materials add one (the eye's reflection about the surface looking it up) is drawn brighter
/// where it shows than the same frame with the map left out, and nowhere darker.
///
/// Behaviour: draw.material.a-shiny-surface-shows-its-environment-map
#[test]
fn a_shiny_surface_shows_its_environment_map() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let parts = parts_of(
        r,
        DataId(0x4700_09CD),
        &Mat4::IDENTITY,
        Vec3::new(1.0, 1.0, 1.0),
    );
    assert!(!parts.is_empty(), "the object's parts");
    let spot = Spot {
        block: (0xB9, 0x45),
        position: Vec3::ZERO,
        heading: 0.0,
        pitch: 0.0,
        time_of_day: 0.45,
        day: None,
    };
    let day = builder.day(&spot);
    let light = builder.light(&spot, day.as_ref());
    let camera = |mid: Vec3, side: f32| Camera {
        position: Vec3::new(mid.x, mid.y - 1.5 * side, mid.z + 0.3 * side),
        heading: 0.0,
        pitch: -10.0,
        fov_y: 60.0,
        near: 0.05,
        far: 500.0,
        orthographic: None,
    };
    let mut shiny = builder.parts_frame(&parts, Vec::new(), camera(Vec3::ZERO, 1.0), light, None);
    let (mid, side) = extent(&shiny);
    shiny.camera = camera(mid, side);
    let mut plain = shiny.clone();
    let mut maps = 0;
    for b in &mut plain.objects {
        if let Some(p) = b.layer.pixel_program.as_mut() {
            if p.environment_add || p.environment_gloss {
                maps += 1;
            }
            p.environment_add = false;
            p.environment_gloss = false;
        }
    }
    assert!(maps > 0, "a draw adds an environment map");
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let [with, without] = checked(&mut device, |d| {
        [&shiny, &plain].map(|f| d.render(f, 320, 240, &mut load).expect("read back"))
    });
    let (mut brighter, mut darker) = (0, 0);
    for (a, b) in with.rgba.chunks(4).zip(without.rgba.chunks(4)) {
        let sum = |p: &[u8]| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]);
        let (a, b) = (sum(a), sum(b));
        if a > b + 6 {
            brighter += 1;
        }
        if b > a + 3 {
            darker += 1;
        }
    }
    println!("{maps} draws with a map; {brighter} pixels brighter, {darker} darker");
    assert!(brighter > 500, "{brighter} pixels brighter");
    assert_eq!(darker, 0);
}

/// Whether every part of a stored vector is a number.
fn finite(v: [f32; 3]) -> bool {
    v.iter().all(|c| c.is_finite())
}

/// A frame about `n` (unit length): two directions across it, each a unit and square to the other.
fn frame_about(n: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let other = if n[2].abs() < 0.9 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let u = cross(other, n);
    let len = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
    let u = [u[0] / len, u[1] / len, u[2] / len];
    (u, cross(n, u))
}

/// Where a mesh stores a surface frame that is not a number (the crystals' feet, where their
/// pictures' coordinates run flat), its normal-mapped layer is lit as if it faced the light: the
/// crystal drawn as stored is brighter about its feet than with every vertex's frame made whole
/// about its normal, and nowhere darker.
///
/// Behaviour: draw.material.a-surface-frame-that-is-not-a-number-is-lit-as-facing-the-light
#[test]
fn a_surface_frame_that_is_not_a_number_is_lit_as_facing_the_light() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let parts = parts_of(
        r,
        DataId(0x4700_0304),
        &Mat4::IDENTITY,
        Vec3::new(1.0, 1.0, 1.0),
    );
    assert!(!parts.is_empty(), "the crystal's parts");
    // the morning sun from the east, the crystal seen from the south
    let spot = Spot {
        block: (0xCB, 0x94),
        position: Vec3::ZERO,
        heading: 0.0,
        pitch: 0.0,
        time_of_day: 0.45,
        day: None,
    };
    let day = builder.day(&spot);
    let light = builder.light(&spot, day.as_ref());
    let camera = |mid: Vec3, side: f32| Camera {
        position: Vec3::new(mid.x, mid.y - 1.5 * side, mid.z + 0.3 * side),
        heading: 0.0,
        pitch: -10.0,
        fov_y: 60.0,
        near: 0.05,
        far: 500.0,
        orthographic: None,
    };
    let mut stored = builder.parts_frame(&parts, Vec::new(), camera(Vec3::ZERO, 1.0), light, None);
    let (mid, side) = extent(&stored);
    stored.camera = camera(mid, side);
    let mut whole = stored.clone();
    let mut undefined = 0;
    for b in &mut whole.objects {
        if b.layer.pixel_program.is_none() {
            continue;
        }
        let mut vertices = b.vertices.as_ref().clone();
        for v in &mut vertices {
            if !(finite(v.tangent_u) && finite(v.tangent_v)) {
                (v.tangent_u, v.tangent_v) = frame_about(v.normal);
                undefined += 1;
            }
        }
        b.vertices = std::sync::Arc::new(vertices);
    }
    assert!(
        undefined > 20,
        "{undefined} vertices with a frame that is not a number"
    );
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let [as_stored, made_whole] = checked(&mut device, |d| {
        [&stored, &whole].map(|f| d.render(f, 320, 240, &mut load).expect("read back"))
    });
    let (mut brighter, mut darker) = (0, 0);
    for (a, b) in as_stored.rgba.chunks(4).zip(made_whole.rgba.chunks(4)) {
        let sum = |p: &[u8]| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]);
        let (a, b) = (sum(a), sum(b));
        if a > b + 6 {
            brighter += 1;
        }
        if b > a + 6 {
            darker += 1;
        }
    }
    println!("{undefined} vertices; {brighter} pixels brighter as stored, {darker} darker");
    assert!(brighter > 500, "{brighter} pixels brighter");
    assert_eq!(darker, 0);
}
