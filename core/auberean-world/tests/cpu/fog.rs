//! Fog blocks on a synthetic fog description.

use auberean_assets::common::Color32;
use auberean_assets::region::{BlockMap, Combine, FogAndLight, FogDesc, FogType};
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::fog::block_light;
use auberean_world::sky::DayLight;

fn day() -> DayLight {
    DayLight {
        ambient_brightness: 0.25,
        direct_brightness: 0.65,
        fog_min: -50.0,
        fog_max: 1500.0,
        fog_sky: 0.0,
        fog_color: Color::new(0.5, 0.5, 0.5, 1.0),
        ..DayLight::DEFAULT
    }
}

/// Every channel the day's (mode 2) unless `own` lists it, then the kind's (mode 3).
fn light(index: u32, fog_kind: u32, min: f32, max: f32, sky: f32, own: &[usize]) -> FogType {
    let mut combines = [Combine {
        mode: 2,
        first: 0.0,
        second: 0.0,
    }; 7];
    for &k in own {
        combines[k].mode = 3;
    }
    FogType {
        index,
        light: FogAndLight {
            fog_min: min,
            fog_max: max,
            fog_sky: sky,
            fog_kind,
            fog_color: Color32(0xFF00_00FF),
            ambient_brightness: 0.5,
            ambient_color: Color32(0xFFFF_FFFF),
            direct_color: Color32(0xFFFF_FFFF),
            combines,
            ..FogAndLight::default()
        },
    }
}

/// A block's fog and light are the day's when none of its four corners has a fog kind with fog;
/// otherwise each corner's (the default kind's distances 0 to 10,000) blended by where the viewer
/// stands, with the channels each kind takes for its own.
///
/// Behaviour: world.fog.a-blocks-corners-blend-their-fog-over-the-days
#[test]
fn a_blocks_corners_blend_their_fog_over_the_days() {
    let desc = FogDesc {
        default_fog: Some(light(0, 0, 0.0, 10_000.0, 0.0, &[])),
        fogs: vec![light(13, 1, -200.0, 1700.0, 0.2, &[0, 6])],
        ..FogDesc::default()
    };
    let kinds = BlockMap {
        height: 4,
        width: 4,
        cells: (0..16).map(|k| if k == 4 + 1 { 13 } else { 0 }).collect(),
    };
    let d = day();
    assert_eq!(
        block_light(
            &d,
            &desc,
            &kinds,
            (2, 2),
            Vec3::new(40.0, 120.0, 0.0),
            160.0
        ),
        d,
        "no corner of block (2, 2) has fog"
    );
    // Block (1, 1): its south-west corner is kind 13, weighted (1 - 0.25)(1 - 0.75).
    let l = block_light(
        &d,
        &desc,
        &kinds,
        (1, 1),
        Vec3::new(40.0, 120.0, 0.0),
        160.0,
    );
    let w = 0.75 * 0.25;
    assert!(
        (l.fog_max - (w * 1700.0 + (1.0 - w) * 10_000.0)).abs() < 1e-2,
        "{}",
        l.fog_max
    );
    assert!((l.fog_min - w * -200.0).abs() < 1e-3);
    assert!((l.fog_sky - w * 0.2).abs() < 1e-6);
    assert!((l.ambient_brightness - (w * 0.5 + (1.0 - w) * 0.25)).abs() < 1e-6);
    assert!(
        (l.direct_brightness - 0.65).abs() < 1e-6,
        "the day's in every corner"
    );
    assert!(
        l.fog_color.b > l.fog_color.r,
        "the kind's blue in its share"
    );
}
