//! The Remastered sky: which of retail's sky objects it draws again, where it lays retail's cloud
//! picture and how it drifts it, and that its shaders are valid.
//!
//! Behaviour: none (experimental hi-fi art packs)

use dereth_render_hifi::passes::atmosphere::heavens::{self, Role};
use dereth_render_hifi::snapshot::{
    HifiFrame, HifiSky, HifiSkyCard, HifiSkyDeck, HifiSkyKnobs, HifiSkyObject,
};
use glam::Vec3;

#[test]
fn every_remastered_sky_shader_parses_validates_and_has_its_entry_points() {
    let wanted: [(&str, &[&str]); 2] = [
        ("clouds", &["vs_fullscreen", "fs_clouds"]),
        ("heavens", &["vs_fullscreen", "fs_heavens"]),
    ];
    for ((name, source), (want_name, entries)) in heavens::shader_sources().iter().zip(wanted) {
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

/// Retail's cloud deck as the scene hands it over: its picture laid flat from above, one unit
/// over 5000 meters, the deck 780 meters over the viewer at its middle.
fn deck(index: u32) -> HifiSkyObject {
    HifiSkyObject {
        index,
        gfx_id: 0x0100_4C36,
        properties: 2,
        transparent: 0.0,
        luminosity: 100.0,
        tex_velocity: [-0.013, 0.013],
        deck: Some(HifiSkyDeck {
            laying: [2.5, 1.0 / 5000.0, 2.5, -1.0 / 5000.0],
            height: 780.0,
        }),
        texture: Some(17),
        ..HifiSkyObject::default()
    }
}

fn card(gfx: u32, index: u32, additive: bool) -> HifiSkyObject {
    HifiSkyObject {
        index,
        gfx_id: gfx,
        transparent: -1.0,
        card: Some(HifiSkyCard {
            center: Vec3::new(1909.0, 1875.0, 0.0),
            u: Vec3::new(-130.0, 225.0, 0.0),
            v: Vec3::new(0.0, 0.0, -260.0),
            uv: [0.5, 0.5],
            additive,
        }),
        texture: Some(20 + index),
        ..HifiSkyObject::default()
    }
}

/// A sunny day group's objects as the sky lays them out: the dome, the night layer, two moons,
/// the cloud deck, the sun and the birds' emitter; and a rainy day's falling rain.
fn sky_objects() -> Vec<HifiSkyObject> {
    vec![
        HifiSkyObject {
            index: 0,
            gfx_id: 0x0100_15EE,
            luminosity: 100.0,
            transparent: -1.0,
            ..HifiSkyObject::default()
        },
        HifiSkyObject {
            index: 1,
            gfx_id: 0x0100_15EF,
            transparent: 100.0,
            ..HifiSkyObject::default()
        },
        card(0x0100_1F67, 2, false),
        card(0x0100_1F6A, 3, false),
        deck(4),
        card(0x0100_1348, 5, true),
        HifiSkyObject {
            index: 6,
            gfx_id: 0x0200_0714,
            transparent: -1.0,
            ..HifiSkyObject::default()
        },
        HifiSkyObject {
            index: 7,
            gfx_id: 0x0100_4C42,
            properties: 4,
            transparent: -1.0,
            ..HifiSkyObject::default()
        },
    ]
}

fn frame(clock: f64) -> HifiFrame {
    HifiFrame {
        time: clock,
        sky: HifiSky {
            sun_direction: Vec3::new(0.6, 0.0, 0.8),
            sun_color: [250, 215, 151],
            ambient_color: [230, 230, 255],
            objects: sky_objects(),
            outdoor: true,
            ..HifiSky::default()
        },
        ..HifiFrame::default()
    }
}

#[test]
fn the_remastered_sky_draws_again_the_night_layer_the_moons_the_deck_and_the_sun_and_keeps_the_rest(
) {
    let objects = sky_objects();
    let roles: Vec<Role> = objects.iter().map(heavens::role).collect();
    assert_eq!(
        roles,
        [
            Role::Dome,
            Role::Night,
            Role::Moon,
            Role::Moon,
            Role::Deck,
            Role::Sun,
            Role::Other,
            Role::Other,
        ]
    );
    let knobs = HifiSkyKnobs::default();
    let plan = heavens::plan(&frame(0.0), &knobs);
    for gfx in [
        0x0100_15EF,
        0x0100_1F67,
        0x0100_1F6A,
        0x0100_4C36,
        0x0100_1348,
    ] {
        assert!(plan.replaces(gfx), "{gfx:08X} is drawn again");
    }
    for gfx in [0x0100_15EE, 0x0200_0714, 0x0100_4C42] {
        assert!(!plan.replaces(gfx), "{gfx:08X} is kept");
    }
    assert_eq!(plan.moons.len(), 2);
    // With the clouds' knob off, retail's own deck is drawn.
    let off = HifiSkyKnobs {
        clouds: false,
        ..knobs
    };
    let plan = heavens::plan(&frame(0.0), &off);
    assert!(plan.deck.is_none());
    assert!(!plan.replaces(0x0100_4C36));
    assert!(plan.replaces(0x0100_1348), "the sun still is");
}

/// The constants' row `row` as four numbers.
fn row(bytes: &[u8], row: usize) -> [f32; 4] {
    let mut out = [0.0; 4];
    for (k, v) in out.iter_mut().enumerate() {
        let at = (row * 4 + k) * 4;
        *v = f32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"));
    }
    out
}

/// How far apart `a` and `b` lie round a circle of one.
fn apart(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(1.0);
    d.min(1.0 - d)
}

#[test]
fn the_clouds_lie_where_the_clock_and_retails_drift_put_them_and_stand_still_with_it() {
    let knobs = HifiSkyKnobs::default();
    let clock = 1250.0;
    let plan = heavens::plan(&frame(clock), &knobs);
    let at = |clock: f64, knobs: &HifiSkyKnobs| {
        let f = frame(clock);
        heavens::constants(&f, knobs, &plan, 1080, 540)
    };
    let held = at(clock, &knobs);
    assert_eq!(held, at(clock, &knobs), "the same clock, the same clouds");
    assert_eq!(held.len(), heavens::CONSTANTS_SIZE);
    // Retail's drift, a share of a picture a second, wrapped into one.
    let drift = row(&held, 2);
    #[allow(clippy::cast_possible_truncation)] // a share of one
    let want = |v: f64, clock: f64| (clock * v).rem_euclid(1.0) as f32;
    assert!(apart(drift[0], want(-0.013, clock)) < 1e-4, "{drift:?}");
    assert!(apart(drift[1], want(0.013, clock)) < 1e-4, "{drift:?}");
    assert!(apart(drift[0], 0.0) > 0.05, "{drift:?}");
    let later = row(&at(clock + 10.0, &knobs), 2);
    assert!(
        apart(later[0], want(-0.013, clock + 10.0)) < 1e-4,
        "{later:?}"
    );
    assert!(apart(later[0], drift[0]) > 0.1, "it moved");
    // Twice the speed: twice as far at the same clock.
    let fast = HifiSkyKnobs {
        cloud_speed: 2.0,
        ..knobs
    };
    let twice = row(&at(clock, &fast), 2);
    assert!(apart(twice[0], want(-0.026, clock)) < 1e-4, "{twice:?}");
    // Still: none at all.
    let still = HifiSkyKnobs {
        cloud_speed: 0.0,
        ..knobs
    };
    let none = row(&at(clock, &still), 2);
    assert!(
        apart(none[0], 0.0) < 1e-6 && apart(none[1], 0.0) < 1e-6,
        "{none:?}"
    );
}

#[test]
fn retails_cloud_picture_is_as_large_overhead_at_the_clouds_height_as_retail_draws_it() {
    let d = deck(4);
    // At retail's own height the laying is retail's.
    let at_retail = HifiSkyKnobs {
        cloud_height: 780.0,
        ..HifiSkyKnobs::default()
    };
    let l = heavens::laying(&d, &at_retail).expect("a laying");
    assert!((l[1] - 1.0 / 5000.0).abs() < 1e-9 && (l[3] + 1.0 / 5000.0).abs() < 1e-9);
    // Twice as high, twice as large: the same angle overhead.
    let high = HifiSkyKnobs {
        cloud_height: 1560.0,
        ..HifiSkyKnobs::default()
    };
    let l = heavens::laying(&d, &high).expect("a laying");
    assert!((l[1] - 1.0 / 10000.0).abs() < 1e-9, "{l:?}");
    let big = HifiSkyKnobs {
        cloud_height: 780.0,
        cloud_scale: 2.0,
        ..HifiSkyKnobs::default()
    };
    let l = heavens::laying(&d, &big).expect("a laying");
    assert!((l[1] - 1.0 / 10000.0).abs() < 1e-9, "{l:?}");
}

#[test]
fn the_clouds_fine_detail_repeats_over_its_cube_without_a_seam() {
    let levels = heavens::detail_noise();
    let n = heavens::DETAIL_SIZE as usize;
    assert_eq!(levels[0].len(), n * n * n);
    assert_eq!(levels.last().map(Vec::len), Some(1));
    // Neighbors across the wrap differ no more than neighbors inside the cube do.
    let at = |x: usize, y: usize, z: usize| f32::from(levels[0][(z * n + y) * n + x]);
    let (mut inside, mut across) = (0.0f32, 0.0f32);
    for z in 0..n {
        for y in 0..n {
            inside += (at(n / 2, y, z) - at(n / 2 - 1, y, z)).abs();
            across += (at(0, y, z) - at(n - 1, y, z)).abs();
        }
    }
    assert!(
        across < inside * 1.3,
        "across the wrap {across}, inside {inside}"
    );
    // And it varies: billows, not a flat field.
    let lo = levels[0].iter().min().copied().unwrap_or(0);
    let hi = levels[0].iter().max().copied().unwrap_or(0);
    assert!(hi - lo > 200, "{lo}..{hi}");
}
