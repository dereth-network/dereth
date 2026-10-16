//! The weather of the high-fidelity presentation: it falls on the game's own rainy days, as rain
//! or over snowy land as snow, and the game's falling-rain layer gives way to it; on a dry day it
//! draws nothing; it leaves the sky and the far land unveiled; a roof keeps all of it off what
//! stands under it; far puddles never turn into white sheets; and snow falls but lies nowhere.
//!
//! The weather is drawn inside the better lighting, so each frame is compared with the same
//! station loaded again with the better lighting alone and stepped the same number of frames,
//! because the clouds move with every frame drawn.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware `wgpu` device.

#![cfg(gpu)]

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_client_runtime::weather::Weather;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_render::wgpu::sidecar::SidecarReport;
use dereth_scene::world_scene::SceneWrites;

use crate::instruments::hifi_stations::{self, capture, moved, Clock, Shot, Station};

const W: u32 = 960;
const H: u32 = 540;
/// Frames a station is stepped before it is captured.
const STEPS: usize = 6;
/// Frames between two captures of one run.
const APART: usize = 4;
/// Frames of the porch the roof is judged on: the drops fall with the clock, and how many of those
/// falling outside the door cross the room on screen changes from frame to frame.
const ROOF_FRAMES: usize = 8;
/// How many pixels in ten thousand of a room may be brightened past twenty levels, on average over
/// the frames: at the porch the drops falling outside the door brighten none to twelve in ten
/// thousand in a frame and under two on average; with the drops in the room drawn too, over sixty
/// on average are, and from the doorway over twenty.
const BRIGHTER_IN_ROOM_PER_TEN_THOUSAND: u32 = 15;
/// How many pixels in a hundred of what the open door shows are drawn wet, at the least.
const OUTSIDE_WET_PERCENT: u32 = 20;
/// A pause between two frames, as a slow or busy machine leaves one.
const PAUSE: std::time::Duration = std::time::Duration::from_millis(750);

fn device() -> Gpu {
    let cfg = DeviceConfig {
        width: W,
        height: H,
        hifi: true,
        ..DeviceConfig::default()
    };
    let gpu = Gpu::new_on(Backend::Wgpu, None, &cfg)
        .unwrap_or_else(|e| panic!("these stations need a wgpu device and none opened: {e}"));
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasteriser",
        gpu.adapter_name()
    );
    gpu
}

fn fidelity(spec: &str) -> FidelityPreferences {
    FidelityPreferences::parse_switch(spec).expect("a fidelity spec")
}

/// The better lighting alone: what the weather is compared with.
fn lit() -> FidelityPreferences {
    fidelity("Lighting=1")
}

/// The weather's box ticked, with the better lighting it is drawn inside.
fn weather() -> FidelityPreferences {
    fidelity("Lighting=1,Weather=1")
}

fn station(name: &str) -> Station {
    hifi_stations::stations(&crate::common::dats())
        .into_iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no station {name}"))
}

/// One run of a station: its last frame, what the presentation did with it, and how many of the
/// game's sky objects were drawn.
struct Run {
    rgba: Vec<u8>,
    report: Option<SidecarReport>,
    sky_objects: usize,
    /// Every frame captured, the first [`STEPS`] frames in and the rest [`APART`] apart; the
    /// last is `rgba`.
    frames: Vec<Vec<u8>>,
}

/// `station` loaded with `prefs`, the game's own weather on or off, the weather `asked` for as
/// `@weather` asks, and stepped [`STEPS`] frames, on a device of its own.
fn run_asked(
    station: &Station,
    prefs: FidelityPreferences,
    game_weather: bool,
    asked: Weather,
) -> Run {
    run_frames(station, prefs, game_weather, asked, 1)
}

/// As [`run_asked`], capturing `count` frames, [`APART`] frames apart after the first.
fn run_frames(
    station: &Station,
    prefs: FidelityPreferences,
    game_weather: bool,
    asked: Weather,
    count: usize,
) -> Run {
    run_shot(station, prefs, count, None, &|shot| {
        if !game_weather {
            dereth_client_runtime::present::SceneMut::set_weather_enabled(&mut shot.scene, false);
        }
        shot.scene.weather = asked;
    })
}

/// `station` loaded with `prefs`, changed by `set` before its first frame, stepped [`STEPS`]
/// frames and captured `count` times, [`APART`] frames apart after the first, on a device of its
/// own; with `pause` before the first captured frame, as a slow machine draws it.
fn run_shot(
    station: &Station,
    prefs: FidelityPreferences,
    count: usize,
    pause: Option<std::time::Duration>,
    set: &dyn Fn(&mut Shot),
) -> Run {
    let mut device = device();
    let gpu = &mut device;
    let store = crate::common::dats();
    let mut shot = Shot::open(&store, gpu, station);
    set(&mut shot);
    shot.scene.draw.cfg.render.fidelity = prefs;
    shot.scene
        .update_from_preferences(&store, gpu)
        .expect("the preferences poll");
    for _ in 0..STEPS {
        shot.step(gpu);
        assert!(gpu.hifi_settle(std::time::Duration::from_secs(30)));
    }
    if let Some(pause) = pause {
        std::thread::sleep(pause);
    }
    shot.step(gpu);
    let mut frames = vec![capture(gpu)];
    while frames.len() < count {
        for _ in 0..APART {
            shot.step(gpu);
        }
        frames.push(capture(gpu));
    }
    Run {
        rgba: frames.last().expect("a frame").clone(),
        report: gpu.hifi_report(),
        sky_objects: shot.scene.draw.stats.sky_objects,
        frames,
    }
}

fn run_with(station: &Station, prefs: FidelityPreferences, game_weather: bool) -> Run {
    run_asked(station, prefs, game_weather, Weather::Auto)
}

fn run(station: &Station, prefs: FidelityPreferences) -> Run {
    run_with(station, prefs, true)
}

/// The count the presentation noted as `name` with its last frame.
fn note(report: &SidecarReport, name: &str) -> u64 {
    report
        .notes
        .iter()
        .find(|(n, _)| *n == name)
        .map_or_else(|| panic!("no note {name}: {:?}", report.notes), |(_, v)| *v)
}

fn luminance(rgba: &[u8], i: usize) -> f32 {
    0.2126 * f32::from(rgba[i]) + 0.7152 * f32::from(rgba[i + 1]) + 0.0722 * f32::from(rgba[i + 2])
}

/// The frame's depth view at `station`: near white, far black on a logarithmic scale; the sky,
/// where nothing was drawn with depth, black.
fn depth_view(station: &Station) -> Vec<u8> {
    run(station, fidelity("Debug=2")).rgba
}

/// Behaviour: hifi.weather.on-a-rainy-day-it-falls-and-the-games-falling-rain-gives-way
/// On the game's rainy day, in the town and over snowy land, the weather is drawn and the
/// game's own falling-rain layer is left out of the frame, so one precipitation falls; on a dry
/// day the weather draws nothing at all.
#[test]
fn on_a_rainy_day_it_falls_and_the_games_falling_rain_gives_way() {
    let _gpu = crate::common::gpu_lock();
    for name in ["rain-street", "snow-field"] {
        let on = run(&station(name), weather());
        let report = on.report.expect("installed");
        eprintln!(
            "{name}: {:?} {:?} {:?}",
            report.passes, report.census, report.notes
        );
        assert!(report.passes.contains(&"weather"), "{name}");
        assert!(
            report.census.skipped_replaced_by_pass > 0,
            "{name}: the game's falling rain was drawn: {:?}",
            report.census
        );
    }
    let dry = station("holtburg");
    let off = run(&dry, lit());
    let on = run(&dry, weather());
    let report = on.report.expect("installed");
    assert!(
        !report.passes.contains(&"weather"),
        "the weather was drawn on a dry day: {:?}",
        report.passes
    );
    assert_eq!(report.census.skipped_replaced_by_pass, 0);
    let changed = moved(&off.rgba, &on.rgba);
    eprintln!("holtburg, a dry day: {changed} bytes differ");
    assert_eq!(changed, 0, "the weather drew on a dry day");
}

/// Behaviour: hifi.weather.the-sky-and-the-far-land-are-not-veiled
/// At the rain stations, the sky and the land past where the ground is soaked take only the
/// falling streaks: their mean brightness moves less than three levels in 255, so no mist or
/// haze veils the frame.
#[test]
fn the_rain_leaves_the_sky_and_the_far_land_unveiled() {
    let _gpu = crate::common::gpu_lock();
    for name in ["rain", "rain-street", "rain-forest", "rain-coast"] {
        let at = station(name);
        let depth = depth_view(&at);
        let off = run(&at, lit());
        let on = run(&at, weather());
        // The sky (no depth) and the land drawn far off (the depth view's darkest land).
        let (mut shift, mut n) = (0.0f64, 0u32);
        for i in (0..(W * H) as usize).map(|p| p * 4) {
            if depth[i] < 40 {
                shift += f64::from(luminance(&on.rgba, i) - luminance(&off.rgba, i));
                n += 1;
            }
        }
        let mean = if n == 0 { 0.0 } else { shift / f64::from(n) };
        eprintln!("{name}: {n} far pixels, their brightness moved {mean:.2}");
        assert!(n > W * H / 20, "{name}: too little far: {n}");
        assert!(mean.abs() < 3.0, "{name}: the far frame moved {mean:.2}");
    }
}

/// Behaviour: hifi.weather.a-roof-keeps-the-weather-off-what-stands-under-it
/// Looking in at a house's open front door in the rain, the room under the roof is drawn with the
/// weather on as it is on a day with the game's weather off: its floor and walls take no wet
/// darkening and no puddle in any of eight frames, and no drop falls in it: only the few falling
/// outside in front of the door brighten it, under fifteen pixels in ten thousand on average over
/// the frames.
#[test]
fn a_roof_keeps_the_weather_off_what_stands_under_it() {
    let _gpu = crate::common::gpu_lock();
    let porch = station("rain-porch");
    let dry = run_frames(&porch, lit(), false, Weather::Auto, ROOF_FRAMES);
    let on = run_frames(&porch, weather(), true, Weather::Auto, ROOF_FRAMES);
    assert!(
        on.report.expect("installed").passes.contains(&"weather"),
        "the weather is not falling at the porch"
    );
    // The room, seen through the door in the middle of the frame: of each pair of frames, how
    // many of its pixels the weather darkens and how many it brightens.
    let room = (H * 4 / 5 - H / 5) * (W * 13 / 20 - W * 7 / 20);
    let counts: Vec<(u32, u32)> = dry
        .frames
        .iter()
        .zip(&on.frames)
        .map(|(dry, on)| {
            let (mut darker, mut brighter) = (0u32, 0u32);
            for y in H / 5..H * 4 / 5 {
                for x in W * 7 / 20..W * 13 / 20 {
                    let i = ((y * W + x) * 4) as usize;
                    let moved = luminance(on, i) - luminance(dry, i);
                    darker += u32::from(moved < -6.0);
                    brighter += u32::from(moved > 20.0);
                }
            }
            (darker, brighter)
        })
        .collect();
    eprintln!(
        "rain-porch: of {room} pixels of the room, darker and brighter than on a dry day in each \
         frame: {counts:?}"
    );
    let darker = counts.iter().map(|c| c.0).max().unwrap_or(0);
    assert!(
        darker * 100 < room,
        "{darker} of {room} pixels under the roof took the wet"
    );
    let brighter: u32 = counts.iter().map(|c| c.1).sum();
    let frames = u32::try_from(counts.len()).expect("a few frames");
    assert!(
        brighter * 10_000 < room * frames * BRIGHTER_IN_ROOM_PER_TEN_THOUSAND,
        "{brighter} of {room} pixels under the roof over {frames} frames were brightened: rain \
         falls in the room"
    );
}

/// Behaviour: hifi.weather.from-a-room-the-weather-is-drawn-over-what-its-openings-show
/// Standing in a house's doorway on the game's rainy day with the camera in the room, the ground
/// the open door shows is drawn wet: darker than the better lighting alone draws it, as it is from
/// outdoors. The room's floor under the roof takes no wet darkening and no drop. So it is whether
/// the frame stamps the depth of the room's openings after it steps indoors, as the client does,
/// or leaves them cleared, as it does with the stamps turned off.
#[test]
fn from_a_doorway_the_ground_outside_stays_wet_and_the_room_dry() {
    let _gpu = crate::common::gpu_lock();
    let doorway = Station {
        clock: Clock::Rainy(0.5),
        weather: true,
        ..station("doorway")
    };
    for stamped in [true, false] {
        let stamps = move |shot: &mut Shot| shot.scene.draw.cfg.portal_depth_stamp = stamped;
        let off = run_shot(&doorway, lit(), 1, None, &stamps);
        let on = run_shot(&doorway, weather(), 1, None, &stamps);
        let report = on.report.expect("installed");
        let what = if stamped {
            "with its openings stamped"
        } else {
            "with its openings left cleared"
        };
        assert!(
            report.passes.contains(&"weather"),
            "{what}: the weather stopped at the door: {:?}",
            report.passes
        );
        // How many pixels of a part of the frame the weather darkens past six levels and
        // brightens past twenty, and how many there are.
        let count = |xs: std::ops::Range<u32>, ys: std::ops::Range<u32>| {
            let (mut n, mut darker, mut brighter) = (0u32, 0u32, 0u32);
            for y in ys {
                for x in xs.clone() {
                    let i = ((y * W + x) * 4) as usize;
                    let moved = luminance(&on.rgba, i) - luminance(&off.rgba, i);
                    n += 1;
                    darker += u32::from(moved < -6.0);
                    brighter += u32::from(moved > 20.0);
                }
            }
            (n, darker, brighter)
        };
        // The ground outside, seen through the door at the frame's left, and the room's floor
        // at its right.
        let (outside, wet, _) = count(0..W / 4, H * 2 / 5..H);
        let (room, room_darker, room_brighter) = count(W * 11 / 20..W * 17 / 20, H * 3 / 5..H);
        eprintln!(
            "doorway in the rain {what}: of {outside} pixels of the ground the door shows, {wet} \
             wet; of {room} pixels of the room's floor, {room_darker} darker and {room_brighter} \
             brighter"
        );
        assert!(
            wet * 100 > outside * OUTSIDE_WET_PERCENT,
            "{what}: {wet} of {outside} pixels the door shows were drawn wet"
        );
        assert!(
            room_darker * 100 < room,
            "{what}: {room_darker} of {room} pixels under the roof took the wet"
        );
        assert!(
            room_brighter * 10_000 < room * BRIGHTER_IN_ROOM_PER_TEN_THOUSAND,
            "{what}: {room_brighter} of {room} pixels under the roof were brightened: rain falls \
             in the room"
        );
    }
}

/// Behaviour: hifi.weather.the-weather-asked-for-falls-in-the-form-asked
/// `@weather` drives both precipitations. Without the effect, rain asked for on the town's sunny
/// day draws the game's rainy sky, weather layers and all. With it, rain and snow fall in the form
/// asked for whatever the land, the game's falling layer giving way; clear asked for on the
/// game's rainy day draws nothing of the effect's, and the frame is the better lighting's alone.
#[test]
fn the_weather_asked_for_falls_in_the_form_asked_whatever_the_day_and_the_land() {
    let _gpu = crate::common::gpu_lock();
    let town = Station {
        weather: true,
        ..station("holtburg")
    };
    let field = station("snow-field");
    let sunny = run_asked(&town, FidelityPreferences::default(), true, Weather::Auto);
    let rainy = run_asked(&town, FidelityPreferences::default(), true, Weather::Rain);
    eprintln!(
        "the town's sky: {} objects on its own day, {} with rain asked for",
        sunny.sky_objects, rainy.sky_objects
    );
    assert!(
        rainy.sky_objects > sunny.sky_objects,
        "the game's sky is not the rainy day's"
    );
    for (at, asked, snow) in [
        (&town, Weather::Rain, 0),
        (&town, Weather::Snow, 1000),
        (&field, Weather::Auto, 1000),
        (&field, Weather::Rain, 0),
    ] {
        let on = run_asked(at, weather(), true, asked);
        let report = on.report.expect("installed");
        eprintln!("{} with {asked:?}: {:?}", at.name, report.notes);
        assert!(report.passes.contains(&"weather"), "{} {asked:?}", at.name);
        assert!(
            report.census.skipped_replaced_by_pass > 0,
            "{} {asked:?}: the game's falling rain was drawn",
            at.name
        );
        assert_eq!(
            note(&report, "weather falling"),
            1000,
            "{} {asked:?}",
            at.name
        );
        assert_eq!(note(&report, "weather snow"), snow, "{} {asked:?}", at.name);
    }
    let street = station("rain-street");
    let own = run(&street, FidelityPreferences::default());
    let off = run_asked(&street, lit(), true, Weather::Clear);
    let on = run_asked(&street, weather(), true, Weather::Clear);
    let report = on.report.expect("installed");
    assert!(
        off.sky_objects < own.sky_objects,
        "clear asked for kept the rainy day's sky: {} against {}",
        off.sky_objects,
        own.sky_objects
    );
    assert!(
        !report.passes.contains(&"weather"),
        "the weather was drawn on a clear day: {:?}",
        report.passes
    );
    assert_eq!(report.census.skipped_replaced_by_pass, 0);
    let changed = moved(&off.rgba, &on.rgba);
    eprintln!("rain-street, clear asked for: {changed} bytes differ");
    assert_eq!(changed, 0, "the weather drew on a clear day");
}

/// Behaviour: hifi.weather.far-puddles-never-turn-white
/// In the town's street in the rain, the far ground (the road and the grass past the near
/// field) is hardly brightened by the weather: far puddles and wet road reflect a held-down
/// sky, not white sheets: under one and a half percent of it, the falling streaks included, is
/// lifted by more than forty levels.
#[test]
fn far_puddles_never_turn_white() {
    let _gpu = crate::common::gpu_lock();
    let street = station("rain-street");
    let depth = depth_view(&street);
    let off = run(&street, lit());
    let on = run(&street, weather());
    let (mut far, mut lifted) = (0u32, 0u32);
    for y in H * 2 / 5..H {
        for x in 0..W {
            let i = ((y * W + x) * 4) as usize;
            // Ground drawn, past the near field: the depth view's darker half.
            if depth[i] > 0 && depth[i] < 128 {
                far += 1;
                if luminance(&on.rgba, i) - luminance(&off.rgba, i) > 40.0 {
                    lifted += 1;
                }
            }
        }
    }
    eprintln!("rain-street: {lifted} of {far} far ground pixels lifted by more than forty levels");
    assert!(far > W * H / 50, "too little far ground: {far}");
    assert!(
        lifted * 1000 < far * 15,
        "{lifted} of {far} far ground pixels turned bright"
    );
}

/// Behaviour: hifi.weather.the-weathers-motion-never-coarsens-in-a-long-session
/// The rain falls by the scene's clock: in the town's street on the game's rainy day, the frame
/// drawn after a pause, as a slow or busy machine draws it, is the frame drawn without one, to
/// the byte.
#[test]
fn a_pause_between_frames_does_not_move_the_rain() {
    let _gpu = crate::common::gpu_lock();
    let street = station("rain-street");
    let unchanged = |_: &mut Shot| {};
    let steady = run_shot(&street, weather(), 1, None, &unchanged);
    let paused = run_shot(&street, weather(), 1, Some(PAUSE), &unchanged);
    let report = paused.report.expect("installed");
    assert!(
        report.passes.contains(&"weather"),
        "the rain is not falling: {:?}",
        report.passes
    );
    let changed = moved(&steady.rgba, &paused.rgba);
    eprintln!("rain-street after a pause: {changed} bytes differ");
    assert_eq!(
        changed, 0,
        "a pause before the last frame changed {changed} bytes of it"
    );
}

/// The mean luminance of `rgba` over the pixels of the frame's lower part from row `from` that
/// `keep` names.
fn mean_luminance(rgba: &[u8], from: u32, keep: impl Fn(u32, u32) -> bool) -> f64 {
    let (mut s, mut n) = (0.0f64, 0u32);
    for y in from..H {
        for x in 0..W {
            if keep(x, y) {
                s += f64::from(luminance(rgba, ((y * W + x) * 4) as usize));
                n += 1;
            }
        }
    }
    s / f64::from(n.max(1))
}

/// Behaviour: hifi.weather.snow-falls-but-lies-nowhere
/// Snow falls and lies nowhere. In the town's street with snow asked for, the roofs are drawn as
/// the better lighting alone draws them but for the falling flakes: under two pixels in a hundred
/// of the frame's upper half are lifted by more than sixty levels (the flakes alone lift under
/// one, a cover on the roofs over four). In the snowbound village on the game's rainy day the land
/// the game paints as snow is drawn as the better lighting alone draws it: the near ground's mean
/// brightness moves by under three levels, the falling flakes included (a cover on it brightens
/// it by over ten).
#[test]
fn snow_falls_but_lies_nowhere() {
    let _gpu = crate::common::gpu_lock();
    let street = station("rain-street");
    let off = run_asked(&street, lit(), true, Weather::Snow);
    let on = run_asked(&street, weather(), true, Weather::Snow);
    let report = on.report.expect("installed");
    assert_eq!(
        note(&report, "weather snow"),
        1000,
        "the snow is not falling"
    );
    let mut lifted = 0u32;
    for y in 0..H / 2 {
        for x in 0..W {
            let i = ((y * W + x) * 4) as usize;
            if luminance(&on.rgba, i) - luminance(&off.rgba, i) > 60.0 {
                lifted += 1;
            }
        }
    }
    let village = station("snow-village");
    let depth = depth_view(&village);
    let off = run(&village, lit());
    let on = run(&village, weather());
    let report = on.report.expect("installed");
    assert!(
        report.passes.contains(&"weather"),
        "the weather is not falling in the village"
    );
    assert_eq!(
        note(&report, "weather snow"),
        1000,
        "the snow is not falling"
    );
    // The near ground, below the middle of the frame.
    let near = |x: u32, y: u32| depth[((y * W + x) * 4) as usize] > 60;
    let mean_off = mean_luminance(&off.rgba, H * 3 / 5, near);
    let mean_on = mean_luminance(&on.rgba, H * 3 / 5, near);
    eprintln!(
        "rain-street with snow: {lifted} pixels of the upper half lifted past sixty levels; \
         snow-village: the near ground {mean_off:.2} without, {mean_on:.2} with"
    );
    assert!(
        lifted * 100 < 2 * (W * H / 2),
        "the roofs took a cover: {lifted} pixels lifted"
    );
    assert!(
        (mean_on - mean_off).abs() < 3.0,
        "the snowy land took a cover: {mean_off:.2} to {mean_on:.2}"
    );
}
