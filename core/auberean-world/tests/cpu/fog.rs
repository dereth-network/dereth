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

/// The day's light, as a fog kind takes it for its fog: sun and ambient added, left alone up to
/// two ten-thousandths past full, and past that scaled so its brightest channel is full.
///
/// Behaviour: world.fog.the-days-light-is-scaled-only-past-its-range
#[test]
fn the_days_light_is_scaled_only_past_its_range() {
    let desc = FogDesc {
        default_fog: Some(light(0, 1, 0.0, 10_000.0, 0.0, &[])),
        ..FogDesc::default()
    };
    let kinds = BlockMap {
        height: 2,
        width: 2,
        cells: vec![0; 4],
    };
    let lit = |brightness: f32| {
        let d = DayLight {
            direct_color: Color::new(1.0, 0.5, 0.25, 1.0),
            ambient_color: Color::new(1.0, 0.5, 0.25, 1.0),
            direct_brightness: brightness,
            ambient_brightness: 0.0,
            ..day()
        };
        block_light(&d, &desc, &kinds, (0, 0), Vec3::new(1.0, 1.0, 0.0), 160.0).fog_color
    };
    let just_over = lit(1.0001);
    assert!(
        (just_over.r - 1.0001).abs() < 1e-5 && (just_over.g - 0.50005).abs() < 1e-5,
        "within the tolerance it stays as it is: {just_over:?}"
    );
    let bright = lit(2.0);
    assert!(
        (bright.r - 1.0).abs() < 1e-6
            && (bright.g - 0.5).abs() < 1e-6
            && (bright.b - 0.25).abs() < 1e-6,
        "{bright:?}"
    );
}

/// One depth of a water's table: its fog distances and sky fog, every channel the kind's own.
fn depth(at: f32, min: f32, max: f32, sky: f32, color: u32) -> auberean_assets::region::WaterLight {
    let mut t = light(0, 1, min, max, sky, &[0, 1, 2, 3, 4, 5, 6]);
    t.light.fog_color = Color32(color);
    auberean_assets::region::WaterLight {
        depth: at,
        light: t.light,
    }
}

/// Under water the fog is the water's, the light the same: with a table of depths 0 and 30, at the
/// surface the shallower's fog, at 30 m and below the deeper's, and between them the two blended
/// by the depth (the fog's alpha the deeper's). With three depths, the deeper of the two blended
/// is the last deeper than the viewer, as the client finds it. An empty table changes nothing.
///
/// Behaviour: world.fog.under-water-the-fog-is-the-waters-by-depth
#[test]
fn under_water_the_fog_is_the_waters_by_depth() {
    use auberean_world::fog::water_fog;
    let day = day();
    let table = [
        depth(0.0, 0.0, 40.0, 1.0, 0x8000_00FF),
        depth(30.0, 0.0, 10.0, 0.5, 0xFF00_0080),
    ];
    let at = |d: f32| water_fog(&day, &table, d);
    let top = at(0.0);
    assert_eq!((top.fog_min, top.fog_max, top.fog_sky), (0.0, 40.0, 1.0));
    assert_eq!(
        (top.direct_brightness, top.ambient_brightness),
        (day.direct_brightness, day.ambient_brightness),
        "the light stays"
    );
    let mid = at(15.0);
    assert_eq!((mid.fog_max, mid.fog_sky), (25.0, 0.75));
    assert!((mid.fog_color.b - (1.0 + 128.0 / 255.0) / 2.0).abs() < 1e-3);
    assert_eq!(mid.fog_color.a, 1.0, "the deeper's alpha");
    for d in [30.0, 45.0] {
        assert_eq!((at(d).fog_max, at(d).fog_sky), (10.0, 0.5), "{d} m");
    }
    let three = [
        table[0].clone(),
        table[1].clone(),
        depth(60.0, 0.0, 4.0, 0.0, 0xFF00_0000),
    ];
    // 15 m: between the 30 m and 60 m depths, their blend reaching past the 30 m one.
    assert_eq!(water_fog(&day, &three, 15.0).fog_max, 13.0);
    assert_eq!(water_fog(&day, &[], 15.0), day);
}
