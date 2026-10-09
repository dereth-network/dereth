//! The sky and the air: their shaders are valid, their numbers follow the authored fog and sky,
//! and positions come back from the frame's depth in the snapshot's own space.

use dereth_primitives::num::math;
use dereth_render_hifi::passes::atmosphere::{self, model};
use dereth_render_hifi::snapshot::{HifiCamera, HifiFog, HifiSky, HifiSkyObject};
use glam::{Mat4, Vec3, Vec4};

/// Behaviour: hifi.sky.the-sky-shaders-validate
#[test]
fn every_sky_and_air_shader_parses_validates_and_has_its_entry_points() {
    let wanted: [(&str, &[&str]); 3] = [
        (
            "tables",
            &[
                "vs_fullscreen",
                "fs_transmittance",
                "fs_multiple",
                "fs_sky_view",
            ],
        ),
        ("sky", &["vs_fullscreen", "fs_sky", "fs_veil"]),
        ("aerial", &["vs_fullscreen", "fs_aerial"]),
    ];
    for ((name, source), (want_name, entries)) in atmosphere::shader_sources().iter().zip(wanted) {
        assert_eq!(*name, want_name);
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let found: Vec<&str> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        for e in entries {
            assert!(found.contains(e), "{name} has no {e}: {found:?}");
        }
    }
}

/// Behaviour: hifi.sky.the-haze-is-calibrated-to-the-authored-fog
/// The haze leaves the land all its light short of where the authored fog begins, and never
/// nearer than its own least start; less the farther the land is; the weather's share at the
/// authored fog's end; more on high ground than on low; and a sunny day's distance clearer than
/// a rainy day's or the night's.
#[test]
fn the_haze_takes_more_light_the_farther_and_lower_the_land_and_meets_its_calibration() {
    let fog = HifiFog {
        min: 150.0,
        max: 2400.0,
        color: [195, 200, 220],
    };
    let day = model::Haze::for_fog(Some(&fog), model::Weather::Sunny, 1.0);
    assert!((day.transmittance(140.0, 0.0) - 1.0).abs() < 1e-6);
    let mut last = 1.0;
    for d in (200..4000).step_by(100) {
        #[allow(clippy::cast_precision_loss)] // small distances
        let t = day.transmittance(d as f32, 0.0);
        assert!(t < last, "not falling at {d} m: {t} after {last}");
        last = t;
    }
    let at_end = day.transmittance(2400.0, 0.0);
    let want = math::expf(-model::Weather::Sunny.haze_depth());
    assert!((at_end - want).abs() < 1e-4, "{at_end} against {want}");
    assert!(day.transmittance(1500.0, 300.0) > day.transmittance(1500.0, 0.0));
    assert!(day.transmittance(1500.0, -300.0) < day.transmittance(1500.0, 0.0));
    let rain = model::Haze::for_fog(Some(&fog), model::Weather::Rainy, 1.0);
    assert!(rain.transmittance(1500.0, 0.0) < day.transmittance(1500.0, 0.0));
    // The night's short fog, which starts at the eye, makes a thicker haze that still leaves
    // the near field alone.
    let night_fog = HifiFog {
        min: 0.0,
        max: 400.0,
        color: [23, 23, 40],
    };
    let night = model::Haze::for_fog(Some(&night_fog), model::Weather::Sunny, 0.0);
    assert!((night.transmittance(model::HAZE_NEAR, 0.0) - 1.0).abs() < 1e-6);
    assert!(night.transmittance(300.0, 0.0) < day.transmittance(300.0, 0.0));
    let at_night_end = night.transmittance(400.0, 0.0);
    assert!(
        (at_night_end - math::expf(-model::NIGHT_HAZE_DEPTH)).abs() < 1e-4,
        "{at_night_end}"
    );
    // The horizon's veil is narrower by night than by day.
    assert!(night.band < day.band, "{} against {}", night.band, day.band);
    // A rainy sky is exposed darker than a sunny one.
    assert!(model::Weather::Rainy.exposure() < model::Weather::Cloudy.exposure());
    assert!(model::Weather::Cloudy.exposure() < model::Weather::Sunny.exposure());
}

fn sky(elevation_deg: f32, dome_luminosity: f32) -> HifiSky {
    let e = elevation_deg.to_radians();
    HifiSky {
        sun_direction: Vec3::new(math::cosf(e), 0.0, math::sinf(e)),
        sun_brightness: 0.8,
        sun_color: [250, 215, 151],
        objects: vec![HifiSkyObject {
            index: 0,
            gfx_id: 0x0100_15EE,
            luminosity: dome_luminosity,
            ..HifiSkyObject::default()
        }],
        outdoor: true,
        ..HifiSky::default()
    }
}

/// Behaviour: hifi.sky.the-physical-sky-follows-the-authored-day
/// The physical sky takes the whole sky by day, none of it at night, when the authored dome is
/// dim and the authored light keeps a low "sun", and part of it at dusk; only the day group's
/// first, plain object is its dome.
#[test]
fn the_physical_sky_takes_the_day_leaves_the_night_and_shares_the_dusk() {
    assert!((model::day_weight(&sky(90.0, 100.0)) - 1.0).abs() < 1e-6);
    assert!(model::day_weight(&sky(1.0, 11.0)) < 1e-6, "the night");
    assert!(model::day_weight(&sky(-10.0, 100.0)) < 1e-6, "the sun down");
    let dusk = model::day_weight(&sky(5.0, 35.0));
    assert!(dusk > 0.3 && dusk < 1.0, "{dusk}");
    // The authored dome's own cast shows through the physical sky a little, the more so the
    // wetter the day.
    let mut rainy = sky(90.0, 100.0);
    rainy.day_group = "Rainy".to_owned();
    let mut sunny = sky(90.0, 100.0);
    sunny.day_group = "Sunny".to_owned();
    assert!(model::sky_share(&rainy) < model::sky_share(&sunny));
    assert!(model::sky_share(&sunny) < 1.0);
    assert!(model::is_dome(0, 0));
    assert!(!model::is_dome(1, 0), "the star field");
    assert!(!model::is_dome(0, 2), "a cloud deck");
}

/// Behaviour: hifi.sky.positions-come-back-height-up
/// The frame draws with the world's height on its second axis; a point the frame's camera
/// projects comes back from its depth as the snapshot's height-up point, and a direction comes
/// back with no part of the eye's position in it.
#[test]
fn a_projected_point_comes_back_height_up_and_a_ray_points_at_it() {
    let eye = Vec3::new(96.0, 72.0, 81.8);
    let target = Vec3::new(140.0, 120.0, 60.0);
    let drawn = |p: Vec3| Vec3::new(p.x, p.z, p.y);
    let view = Mat4::look_at_lh(drawn(eye), drawn(target), Vec3::Y);
    let projection = Mat4::perspective_lh(1.0, 16.0 / 9.0, 0.1, 4000.0);
    let cam = HifiCamera {
        view,
        projection,
        eye,
        ..HifiCamera::default()
    };
    let p = Vec3::new(120.0, 101.0, 70.0);
    let clip = cam.view_projection() * drawn(p).extend(1.0);
    let ndc = clip / clip.w;
    let back = model::position_from_ndc(&cam) * Vec4::new(ndc.x, ndc.y, ndc.z, 1.0);
    let back = back.truncate() / back.w;
    assert!((back - p).length() < 1e-2, "{back} against {p}");
    let ray = model::ray_from_ndc(&cam) * Vec4::new(ndc.x, ndc.y, 0.5, 1.0);
    let ray = (ray.truncate() / ray.w).normalize();
    let want = (p - eye).normalize();
    assert!(ray.dot(want) > 0.9999, "{ray} against {want}");
    let bytes = model::constants(
        &dereth_render_hifi::HifiFrame {
            camera: cam,
            ..dereth_render_hifi::HifiFrame::default()
        },
        dereth_render_hifi::Level::High,
    );
    assert_eq!(bytes.len(), model::CONSTANTS_SIZE);
}
