//! A streak's ribbon as the device draws it.

use auberean_draw::material::{draw_layers, instance_material, DEVICE_FLAGS};
use auberean_draw::scene::{Camera, SceneBuilder, Spot};
use auberean_draw::streak::{StreakDesc, Trail};
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::math::Mat4;

use crate::common::{checked, install, renderer};

/// A streak swung through the air leaves a ribbon of light: a weapon's streak (its line 0.1 m to
/// 0.9 m along the blade, white, in the install's streak material), turned on and swung a meter
/// across the camera's view over a tenth of a second, adds its picture onto the dark frame along
/// the swing, brightening it and darkening nothing; with the streak off nothing is drawn.
///
/// Behaviour: draw.streak.a-swung-streak-leaves-a-ribbon-of-light
#[test]
fn a_swung_streak_leaves_a_ribbon_of_light() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let material = instance_material(r, 0x3100_0200).expect("the streak material");
    let layers = draw_layers(&material, DEVICE_FLAGS);
    assert!(!layers.is_empty());
    let desc = StreakDesc {
        start: Vec3::new(0.0, 0.0, 0.1),
        end: Vec3::new(0.0, 0.0, 0.9),
        lifespan: 0.0,
        trail: 0.2,
        min_time: 0.0,
        min_distance: 0.1,
        color: Color::WHITE,
        material: 0x3100_0200,
        texture_scale: 1.0,
    };
    let cell = 0x8080_0001;
    let swing = |on: bool| {
        let mut s = Trail::new(desc, 0.0);
        if on {
            s.activate();
        }
        for k in 0..=10_u8 {
            let x = f32::from(k) * 0.1 - 0.5;
            s.set_position(
                u64::from(k),
                cell,
                Mat4::translation(Vec3::new(x, 0.0, 0.0)),
                1.0,
            );
            s.think(f64::from(k) * 0.01);
        }
        s.batches(10, &mut 0, &layers, cell)
    };
    let lit = swing(true);
    assert!(!lit.is_empty(), "a ribbon");
    assert!(swing(false).is_empty(), "off: none");
    let spot = Spot {
        block: (0x80, 0x80),
        position: Vec3::ZERO,
        heading: 0.0,
        pitch: 0.0,
        time_of_day: 0.45,
        day: None,
    };
    let day = builder.day(&spot);
    let light = builder.light(&spot, day.as_ref());
    let camera = Camera {
        position: Vec3::new(0.0, -2.0, 0.5),
        heading: 0.0,
        pitch: 0.0,
        fov_y: 60.0,
        near: 0.05,
        far: 100.0,
        roll: 0.0,
        orthographic: None,
    };
    let with = builder.parts_frame(&[], lit, camera, light, None);
    let without = builder.parts_frame(&[], Vec::new(), camera, light, None);
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let [a, b] = checked(&mut device, |d| {
        [&with, &without].map(|f| d.render(f, 320, 240, &mut load).expect("read back"))
    });
    let (mut brighter, mut darker) = (0, 0);
    for (p, q) in a.rgba.chunks(4).zip(b.rgba.chunks(4)) {
        let sum = |p: &[u8]| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]);
        let (p, q) = (sum(p), sum(q));
        if p > q + 6 {
            brighter += 1;
        }
        if q > p + 3 {
            darker += 1;
        }
    }
    println!("{brighter} pixels brighter, {darker} darker");
    assert!(brighter > 1000, "{brighter} pixels brighter");
    assert_eq!(darker, 0);
}
