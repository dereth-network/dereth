//! Behaviour: none (experimental)
//!
//! The remaster's weather: it falls on the game's own rainy days, as rain or over snowy land as
//! snow, and the game's own falling weather gives way to it; on a dry day it draws nothing; it
//! leaves the sky and the far land unveiled; a roof keeps all of it off what stands under it; far
//! puddles never turn into white sheets; snow falls but lies nowhere; and the puddles are a switch
//! of their own, lying in the ground's own material where its terrain takes them, the same near
//! and far, never fighting the ground.
//!
//! Each frame is compared with the same station loaded again without the weather and stepped the
//! same number of frames, so the sky and the clock stand alike in both.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::weather::Weather;
use dereth_dat::RetailDatStore;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_scene::world_scene::{
    RemasterCapture, RemasterLight, RemasterTerrain, RemasterWater, RemasterWeather,
    RemasterWeatherStats,
};

use crate::instruments::stations::{self, Clock, Shot, Station};

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
/// the frames: only the drops falling outside the door, in front of it, may cross it.
const BRIGHTER_IN_ROOM_PER_TEN_THOUSAND: u32 = 15;
/// How many pixels in a hundred of what the open door shows are drawn wet, at the least.
const OUTSIDE_WET_PERCENT: u32 = 20;
/// A pause between two frames, as a slow or busy machine leaves one.
const PAUSE: std::time::Duration = std::time::Duration::from_millis(750);
/// Where the far land starts, meters from the eye.
const FAR: f32 = 150.0;
/// The water's kind among the material's kinds.
const WATER: u8 = 7;

/// How a run is set up.
#[derive(Debug, Clone, Copy)]
struct Setup {
    /// The rain and the snow.
    weather: bool,
    /// The water pass's puddles.
    puddles: bool,
    /// The ground drawn over retail's, smoothed, as the player has it.
    drawn: bool,
    /// The game's own weather drawn.
    game_weather: bool,
    /// The weather asked for, as `@weather` asks.
    asked: Weather,
    /// The room's openings stamped into the depth when the frame steps indoors.
    stamped: bool,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            weather: false,
            puddles: false,
            drawn: false,
            game_weather: true,
            asked: Weather::Auto,
            stamped: true,
        }
    }
}

/// Without the weather.
fn dry() -> Setup {
    Setup::default()
}

/// With the rain and the snow.
fn weather() -> Setup {
    Setup {
        weather: true,
        ..Setup::default()
    }
}

/// One run of a station: every frame captured, and what the weather counted with the last.
struct Run {
    frames: Vec<RemasterCapture>,
    stats: RemasterWeatherStats,
}

impl Run {
    fn last(&self) -> &RemasterCapture {
        self.frames.last().expect("a frame")
    }
}

fn station(store: &RetailDatStore, name: &str) -> Station {
    stations::stations(store)
        .into_iter()
        .chain(stations::close_stations(store))
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no station {name}"))
}

/// `station` on a device of its own with the remaster on, its light per pixel and nothing else of
/// it but what `setup` asks, stepped [`STEPS`] frames with the clock running and captured `count`
/// times, [`APART`] frames apart after the first; with `pause` before the first captured frame,
/// as a slow machine draws it.
fn run_frames(
    store: &Arc<RetailDatStore>,
    station: &Station,
    setup: Setup,
    count: usize,
    pause: Option<std::time::Duration>,
) -> Run {
    let cfg = DeviceConfig {
        width: W,
        height: H,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(Backend::Vulkan, None, &cfg).expect("a Vulkan device");
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasterizer",
        gpu.adapter_name()
    );
    let mut shot = Shot::open(store, &mut gpu, station);
    if !setup.game_weather {
        dereth_client_runtime::present::SceneMut::set_weather_enabled(&mut shot.scene, false);
    }
    shot.scene.weather = setup.asked;
    shot.scene.draw.cfg.portal_depth_stamp = setup.stamped;
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_settings(dereth_scene::world_scene::RemasterSettings::off(), &mut gpu);
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    if setup.drawn {
        shot.scene.draw.set_remaster_terrain(
            RemasterTerrain {
                splat: true,
                grid: true,
                drawn: true,
                horizon: true,
            },
            &mut gpu,
        );
        shot.step_held(&mut gpu);
        shot.scene.wait_for_remaster_ground();
    }
    shot.scene
        .draw
        .set_remaster_weather(RemasterWeather { on: setup.weather }, &mut gpu);
    shot.scene.draw.set_remaster_water(
        RemasterWater {
            puddles: setup.puddles,
            ..RemasterWater::default()
        },
        &mut gpu,
    );
    for _ in 0..STEPS {
        shot.step(&mut gpu);
    }
    if let Some(pause) = pause {
        std::thread::sleep(pause);
    }
    let capture = |shot: &Shot, gpu: &mut Gpu| {
        shot.scene
            .draw
            .remaster_capture(gpu)
            .expect("the remaster drew")
    };
    shot.step(&mut gpu);
    assert_eq!(
        shot.scene.remaster_ground_active(),
        setup.drawn,
        "{}: the drawn ground drawn as asked",
        station.name
    );
    let mut frames = vec![capture(&shot, &mut gpu)];
    while frames.len() < count {
        for _ in 0..APART {
            shot.step(&mut gpu);
        }
        frames.push(capture(&shot, &mut gpu));
    }
    let stats = shot
        .scene
        .draw
        .remaster_weather_stats(&mut gpu)
        .expect("the remaster is on");
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(
        errors.is_empty(),
        "{}: the validation layer: {errors:#?}",
        station.name
    );
    Run { frames, stats }
}

fn run(store: &Arc<RetailDatStore>, station: &Station, setup: Setup) -> Run {
    run_frames(store, station, setup, 1, None)
}

fn luminance(rgba: &[u8], i: usize) -> f32 {
    0.2126 * f32::from(rgba[i]) + 0.7152 * f32::from(rgba[i + 1]) + 0.0722 * f32::from(rgba[i + 2])
}

/// How far the pixel `i` of `c` stands from the eye, meters; infinite where nothing was drawn
/// with depth.
#[allow(clippy::cast_precision_loss)]
fn distance(c: &RemasterCapture, inv: &glam::Mat4, i: usize) -> f32 {
    if c.depth[i] >= 1.0 {
        return f32::INFINITY;
    }
    (place_of(c, inv, i) - glam::Vec3::from(c.eye)).length()
}

/// Where the pixel `i` of `c` stands, from its depth.
#[allow(clippy::cast_precision_loss)]
fn place_of(c: &RemasterCapture, inv: &glam::Mat4, i: usize) -> glam::Vec3 {
    let (w, h) = (c.width as usize, c.height as usize);
    let (x, y) = ((i % w) as f32, (i / w) as f32);
    let ndc = glam::Vec4::new(
        (x + 0.5) / w as f32 * 2.0 - 1.0,
        1.0 - (y + 0.5) / h as f32 * 2.0,
        c.depth[i],
        1.0,
    );
    let p = *inv * ndc;
    (p / p.w).truncate()
}

fn inverse(c: &RemasterCapture) -> glam::Mat4 {
    glam::Mat4::from_cols_array(&c.view_projection).inverse()
}

/// The puddle share of the ground at pixel `i` of `c`, 0 to 255: the ground's material's fourth
/// byte where it is not water.
fn puddle(c: &RemasterCapture, i: usize) -> u8 {
    if c.class[i] == 1 && c.kind[i] != WATER {
        c.bed[i]
    } else {
        0
    }
}

/// On the game's rainy day, in the town's street and over snowy land, the weather falls in full
/// and the game's own falling weather is left out of the sky, so one precipitation falls; on a
/// dry day it draws nothing at all.
#[test]
fn on_a_rainy_day_it_falls_and_the_games_falling_rain_gives_way() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    for name in ["rain-street", "snow-field"] {
        let on = run(&store, &station(&store, name), weather());
        eprintln!("{name}: {:?}", on.stats);
        assert_eq!(on.stats.falling, 1000, "{name}: it does not fall");
        assert!(on.stats.particles > 0, "{name}: no particle drawn");
        assert!(
            on.stats.sky_left_out > 0,
            "{name}: the game's falling weather was drawn"
        );
    }
    let town = station(&store, "holtburg");
    let off = run(&store, &town, dry());
    let on = run(&store, &town, weather());
    eprintln!("holtburg, a dry day: {:?}", on.stats);
    assert_eq!(on.stats.frames, 0, "the weather was drawn on a dry day");
    assert_eq!(on.stats.sky_left_out, 0);
    assert_eq!(
        on.last().color,
        off.last().color,
        "the weather drew on a dry day"
    );
}

/// At the rain stations the sky and the land farther than [`FAR`] meters take only the falling
/// streaks: their mean brightness moves less than three levels in 255, so no mist or haze veils
/// the frame.
#[test]
fn the_rain_leaves_the_sky_and_the_far_land_unveiled() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    for name in ["rain", "rain-street", "rain-forest", "rain-coast"] {
        let at = station(&store, name);
        let off = run(&store, &at, dry());
        let on = run(&store, &at, weather());
        let (a, b) = (off.last(), on.last());
        let inv = inverse(a);
        let (mut shift, mut n) = (0.0f64, 0u32);
        for p in 0..(W * H) as usize {
            if distance(a, &inv, p) > FAR {
                shift += f64::from(luminance(&b.color, p * 4) - luminance(&a.color, p * 4));
                n += 1;
            }
        }
        let mean = if n == 0 { 0.0 } else { shift / f64::from(n) };
        eprintln!("{name}: {n} far pixels, their brightness moved {mean:.2}");
        assert!(n > W * H / 20, "{name}: too little far: {n}");
        assert!(mean.abs() < 3.0, "{name}: the far frame moved {mean:.2}");
    }
}

/// Of each pair of frames, how many pixels of the frame's part `xs` by `ys` the weather darkens
/// past six levels and brightens past twenty, and how many there are.
fn moved(
    dry: &RemasterCapture,
    on: &RemasterCapture,
    xs: std::ops::Range<u32>,
    ys: std::ops::Range<u32>,
) -> (u32, u32, u32) {
    let (mut n, mut darker, mut brighter) = (0u32, 0u32, 0u32);
    for y in ys {
        for x in xs.clone() {
            let i = ((y * W + x) * 4) as usize;
            let m = luminance(&on.color, i) - luminance(&dry.color, i);
            n += 1;
            darker += u32::from(m < -6.0);
            brighter += u32::from(m > 20.0);
        }
    }
    (n, darker, brighter)
}

/// Looking in at a house's open front door in the rain, the room under the roof is drawn as it
/// is without the weather: its floor and walls take no wet in any of eight frames, and no drop
/// falls in it: only the few falling outside in front of the door brighten it, under fifteen
/// pixels in ten thousand on average over the frames.
#[test]
fn a_roof_keeps_the_weather_off_what_stands_under_it() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let porch = station(&store, "rain-porch");
    let dry = run_frames(&store, &porch, dry(), ROOF_FRAMES, None);
    let on = run_frames(&store, &porch, weather(), ROOF_FRAMES, None);
    assert_eq!(
        on.stats.falling, 1000,
        "the weather is not falling at the porch"
    );
    let counts: Vec<(u32, u32, u32)> = dry
        .frames
        .iter()
        .zip(&on.frames)
        .map(|(a, b)| moved(a, b, W * 7 / 20..W * 13 / 20, H / 5..H * 4 / 5))
        .collect();
    let room = counts.first().map_or(0, |c| c.0);
    eprintln!(
        "rain-porch: of {room} pixels of the room, darker and brighter than on a dry day in each \
         frame: {:?}",
        counts.iter().map(|c| (c.1, c.2)).collect::<Vec<_>>()
    );
    let darker = counts.iter().map(|c| c.1).max().unwrap_or(0);
    assert!(
        darker * 100 < room,
        "{darker} of {room} pixels under the roof took the wet"
    );
    let brighter: u32 = counts.iter().map(|c| c.2).sum();
    let frames = u32::try_from(counts.len()).expect("a few frames");
    assert!(
        brighter * 10_000 < room * frames * BRIGHTER_IN_ROOM_PER_TEN_THOUSAND,
        "{brighter} of {room} pixels under the roof over {frames} frames were brightened: rain \
         falls in the room"
    );
}

/// Standing in a house's doorway on the game's rainy day with the camera in the room, the ground
/// the open door shows is drawn wet, darker than without the weather, as it is from outdoors; the
/// room's floor under the roof takes no wet and no drop. So it is whether the frame stamps the
/// room's openings into the depth when it steps indoors, or leaves them cleared.
#[test]
fn from_a_doorway_the_ground_outside_stays_wet_and_the_room_dry() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let doorway = Station {
        clock: Clock::Rainy(0.5),
        weather: true,
        ..station(&store, "doorway")
    };
    for stamped in [true, false] {
        let off = run(&store, &doorway, Setup { stamped, ..dry() });
        let on = run(
            &store,
            &doorway,
            Setup {
                stamped,
                ..weather()
            },
        );
        let what = if stamped {
            "with its openings stamped"
        } else {
            "with its openings left cleared"
        };
        assert_eq!(
            on.stats.falling, 1000,
            "{what}: the weather stopped at the door"
        );
        let (outside, wet, _) = moved(off.last(), on.last(), 0..W / 4, H * 2 / 5..H);
        let (room, room_darker, room_brighter) = moved(
            off.last(),
            on.last(),
            W * 11 / 20..W * 17 / 20,
            H * 3 / 5..H,
        );
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

/// `@weather` drives both precipitations: rain and snow fall in the form asked for whatever the
/// land, the game's falling weather giving way; clear asked for on the game's rainy day draws
/// nothing of the weather's, and the frame is the one drawn without it.
#[test]
fn the_weather_asked_for_falls_in_the_form_asked_whatever_the_day_and_the_land() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let town = Station {
        weather: true,
        ..station(&store, "holtburg")
    };
    let field = station(&store, "snow-field");
    for (at, asked, snow) in [
        (&town, Weather::Rain, 0),
        (&town, Weather::Snow, 1000),
        (&field, Weather::Auto, 1000),
        (&field, Weather::Rain, 0),
    ] {
        let on = run(&store, at, Setup { asked, ..weather() });
        eprintln!("{} with {asked:?}: {:?}", at.name, on.stats);
        assert_eq!(on.stats.falling, 1000, "{} {asked:?}", at.name);
        assert_eq!(on.stats.snow, snow, "{} {asked:?}", at.name);
        assert!(
            on.stats.sky_left_out > 0,
            "{} {asked:?}: the game's falling weather was drawn",
            at.name
        );
    }
    let street = station(&store, "rain-street");
    let clear = Setup {
        asked: Weather::Clear,
        ..dry()
    };
    let off = run(&store, &street, clear);
    let on = run(
        &store,
        &street,
        Setup {
            weather: true,
            ..clear
        },
    );
    assert_eq!(on.stats.frames, 0, "the weather was drawn on a clear day");
    assert_eq!(
        on.last().color,
        off.last().color,
        "the weather drew on a clear day"
    );
}

/// In the town's street in the rain, the far ground (past twenty-five meters) is hardly
/// brightened, with the puddles gathering or without: far puddles and wet road hold a held-down
/// sky, not white sheets: under one and a half percent of it, the falling streaks included, is
/// lifted by more than forty levels.
#[test]
fn far_puddles_never_turn_white() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let street = station(&store, "rain-street");
    let off = run(&store, &street, dry());
    let a = off.last();
    let inv = inverse(a);
    for puddles in [false, true] {
        let on = run(
            &store,
            &street,
            Setup {
                puddles,
                ..weather()
            },
        );
        let b = on.last();
        let (mut far, mut lifted) = (0u32, 0u32);
        for p in 0..(W * H) as usize {
            if a.class[p] == 1 && distance(a, &inv, p) > 25.0 {
                far += 1;
                if luminance(&b.color, p * 4) - luminance(&a.color, p * 4) > 40.0 {
                    lifted += 1;
                }
            }
        }
        eprintln!(
            "rain-street, puddles {puddles}: {lifted} of {far} far ground pixels lifted by more \
             than forty levels"
        );
        assert!(far > W * H / 50, "too little far ground: {far}");
        assert!(
            lifted * 1000 < far * 15,
            "puddles {puddles}: {lifted} of {far} far ground pixels turned bright"
        );
    }
}

/// The rain falls by the scene's clock: in the town's street on the game's rainy day, the frame
/// drawn after a pause, as a slow or busy machine draws it, is the frame drawn without one, to
/// the byte.
#[test]
fn a_pause_between_frames_does_not_move_the_rain() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let street = station(&store, "rain-street");
    let steady = run_frames(&store, &street, weather(), 1, None);
    let paused = run_frames(&store, &street, weather(), 1, Some(PAUSE));
    assert_eq!(paused.stats.falling, 1000, "the rain is not falling");
    let changed = steady
        .last()
        .color
        .iter()
        .zip(&paused.last().color)
        .filter(|(a, b)| a != b)
        .count();
    eprintln!("rain-street after a pause: {changed} bytes differ");
    assert_eq!(
        changed, 0,
        "a pause before the last frame changed {changed} bytes of it"
    );
}

/// Snow falls and lies nowhere. In the town's street with snow asked for, the roofs are drawn as
/// they are without the weather but for the falling flakes: under two pixels in a hundred of the
/// frame's upper half are lifted by more than sixty levels. In the snowbound village on the
/// game's rainy day the near ground is drawn as it is without the weather: its mean brightness
/// moves by under three levels, the falling flakes included.
#[test]
fn snow_falls_but_lies_nowhere() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let street = station(&store, "rain-street");
    let snow = Setup {
        asked: Weather::Snow,
        ..dry()
    };
    let off = run(&store, &street, snow);
    let on = run(
        &store,
        &street,
        Setup {
            weather: true,
            ..snow
        },
    );
    assert_eq!(on.stats.snow, 1000, "the snow is not falling");
    let mut lifted = 0u32;
    for p in 0..(W * H / 2) as usize {
        if luminance(&on.last().color, p * 4) - luminance(&off.last().color, p * 4) > 60.0 {
            lifted += 1;
        }
    }
    let village = station(&store, "snow-village");
    let off = run(&store, &village, dry());
    let on = run(&store, &village, weather());
    assert_eq!(
        on.stats.snow, 1000,
        "the snow is not falling in the village"
    );
    let (a, b) = (off.last(), on.last());
    let inv = inverse(a);
    let (mut sum_off, mut sum_on, mut n) = (0.0f64, 0.0f64, 0u32);
    for p in (W * H * 3 / 5) as usize..(W * H) as usize {
        if a.class[p] == 1 && distance(a, &inv, p) < 30.0 {
            sum_off += f64::from(luminance(&a.color, p * 4));
            sum_on += f64::from(luminance(&b.color, p * 4));
            n += 1;
        }
    }
    let n = f64::from(n.max(1));
    let (mean_off, mean_on) = (sum_off / n, sum_on / n);
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

/// The rain and the puddles are switches of their own. In the town's street on the game's rainy
/// day, on the ground the player draws: the rain alone falls and gathers no puddle; the puddles
/// alone gather on the flat ground, its own material saying where, while nothing falls and the
/// game's own falling weather stays. On retail's own grid the puddles gather the same way. With
/// the view holding its place, the overhead map is drawn once.
#[test]
fn the_rain_and_the_puddles_are_switches_of_their_own() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let street = station(&store, "rain-street");
    let rain = run(
        &store,
        &street,
        Setup {
            drawn: true,
            ..weather()
        },
    );
    let puddles = run(
        &store,
        &street,
        Setup {
            drawn: true,
            puddles: true,
            ..dry()
        },
    );
    let count = |c: &RemasterCapture| (0..c.class.len()).filter(|i| puddle(c, *i) > 0).count();
    let (rain_puddled, puddled) = (count(rain.last()), count(puddles.last()));
    eprintln!(
        "rain-street: the rain alone {:?}, {rain_puddled} puddle pixels; the puddles alone {:?}, \
         {puddled} puddle pixels",
        rain.stats, puddles.stats
    );
    assert!(rain.stats.particles > 0, "the rain alone drew no drop");
    assert_eq!(
        rain.stats.cover_drawn, 1,
        "the overhead map was drawn again"
    );
    assert_eq!(rain_puddled, 0, "the rain alone gathered puddles");
    assert_eq!(puddles.stats.particles, 0, "the puddles alone drew drops");
    assert_eq!(puddles.stats.falling, 0, "the puddles alone made it fall");
    assert_eq!(
        puddles.stats.sky_left_out, 0,
        "the puddles alone left the game's falling weather out"
    );
    assert!(
        puddled > 2_000,
        "the puddles alone gathered {puddled} pixels"
    );
    let grid = run(
        &store,
        &street,
        Setup {
            puddles: true,
            ..dry()
        },
    );
    let on_grid = count(grid.last());
    eprintln!("rain-street, retail's grid: {on_grid} puddle pixels");
    assert!(
        on_grid > 2_000,
        "the puddles gathered {on_grid} pixels on retail's grid"
    );
}

/// The puddles lie in the ground's own material where its terrain takes them, fixed to the world:
/// on the ground the player draws, in the town's street, every pixel with a share is flat ground,
/// and seen again from three meters farther back the world's spots a quarter meter across that
/// both frames show are puddle or dry alike, near and far: under one in a hundred of the puddled
/// ones is a puddle (three quarters or more) in one frame and dry (a quarter or less) in the
/// other.
#[test]
fn the_puddles_lie_where_the_ground_takes_them_the_same_near_and_far() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let street = station(&store, "rain-street");
    let setup = Setup {
        drawn: true,
        puddles: true,
        ..dry()
    };
    let near = run(&store, &street, setup);
    let back = match street.place {
        stations::Place::Free {
            block,
            x,
            y,
            eye,
            yaw,
            pitch,
        } => {
            let (s, c) = (
                dereth_primitives::num::math::sinf(yaw),
                dereth_primitives::num::math::cosf(yaw),
            );
            Station {
                place: stations::Place::Free {
                    block,
                    x: x - 3.0 * s,
                    y: y - 3.0 * c,
                    eye,
                    yaw,
                    pitch,
                },
                ..street.clone()
            }
        }
        _ => panic!("the street is a free camera"),
    };
    let far = run(&store, &back, setup);
    // Every puddle pixel is flat ground.
    let a = near.last();
    let mut steep = 0usize;
    let mut shares = std::collections::HashMap::new();
    let inv = inverse(a);
    for i in 0..a.class.len() {
        let s = puddle(a, i);
        if s > 0 && a.normal[i][2] < 0.9 {
            steep += 1;
        }
        if a.class[i] == 1 && a.kind[i] != WATER {
            let p = place_of(a, &inv, i);
            #[allow(clippy::cast_possible_truncation)] // a spot a quarter meter across
            let key = ((p.x * 4.0).floor() as i64, (p.y * 4.0).floor() as i64);
            shares.entry(key).or_insert(s);
        }
    }
    let b = far.last();
    let inv = inverse(b);
    let (mut both, mut differ, mut wet, mut moved) = (0usize, 0usize, 0usize, 0usize);
    for i in 0..b.class.len() {
        if b.class[i] != 1 || b.kind[i] == WATER {
            continue;
        }
        let p = place_of(b, &inv, i);
        #[allow(clippy::cast_possible_truncation)] // a spot a quarter meter across
        let key = ((p.x * 4.0).floor() as i64, (p.y * 4.0).floor() as i64);
        if let Some(s) = shares.get(&key) {
            let t = puddle(b, i);
            both += 1;
            wet += usize::from(*s > 0);
            moved += usize::from(s.abs_diff(t) > 12);
            differ += usize::from((*s >= 192 && t <= 64) || (*s <= 64 && t >= 192));
        }
    }
    eprintln!(
        "rain-street: {steep} puddle pixels off flat ground; of {both} spots seen from both places \
         ({wet} puddled), {moved} differ by more than a twentieth, {differ} puddle in one and dry \
         in the other"
    );
    assert_eq!(steep, 0, "puddles lie on sloping ground");
    assert!(wet > 1_000, "too few puddled spots seen from both: {wet}");
    assert!(differ * 100 < wet, "{differ} of {wet} puddled spots differ");
}

/// The puddles never fight the ground (the smoothed ground the player draws): with the clock held,
/// four frames in a row, the rain falling and the puddles gathering, keep every pixel's puddle
/// share, and draw the world without the puddles the same to the byte (the puddles mirror the
/// sky's clouds, which drift with every frame drawn).
#[test]
fn the_puddles_never_fight_the_ground() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let street = station(&store, "rain-street");
    let cfg = DeviceConfig {
        width: W,
        height: H,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(Backend::Vulkan, None, &cfg).expect("a Vulkan device");
    let mut shot = Shot::open(&store, &mut gpu, &street);
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster(true, &store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_settings(dereth_scene::world_scene::RemasterSettings::off(), &mut gpu);
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    shot.scene.draw.set_remaster_terrain(
        RemasterTerrain {
            splat: true,
            grid: true,
            drawn: true,
            horizon: true,
        },
        &mut gpu,
    );
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    shot.scene
        .draw
        .set_remaster_weather(RemasterWeather { on: true }, &mut gpu);
    shot.scene.draw.set_remaster_water(
        RemasterWater {
            puddles: true,
            ..RemasterWater::default()
        },
        &mut gpu,
    );
    for _ in 0..STEPS {
        shot.step(&mut gpu);
    }
    assert!(
        shot.scene.remaster_ground_active(),
        "the drawn ground is not drawn"
    );
    let mut frames = Vec::new();
    for _ in 0..4 {
        shot.step_held(&mut gpu);
        frames.push(
            shot.scene
                .draw
                .remaster_capture(&mut gpu)
                .expect("the remaster drew"),
        );
    }
    shot.scene
        .draw
        .set_remaster(false, &store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    let first = &frames[0];
    let puddled = (0..first.class.len())
        .filter(|i| puddle(first, *i) > 0)
        .count();
    let mut changed = 0usize;
    let mut shares = 0usize;
    for f in &frames[1..] {
        for i in 0..f.class.len() {
            // The sky's clouds drift with every frame drawn; the world holds still.
            let still = first.class[i] != 0 && puddle(first, i) == 0;
            if still && f.color[i * 4..i * 4 + 4] != first.color[i * 4..i * 4 + 4] {
                changed += 1;
            }
            shares += usize::from(puddle(f, i) != puddle(first, i));
        }
    }
    eprintln!(
        "rain-street, held: {puddled} puddle pixels; over three frames {changed} world pixels and \
         {shares} puddle shares changed"
    );
    assert!(puddled > 2_000, "too few puddle pixels: {puddled}");
    assert_eq!(changed, 0, "the world flickers");
    assert_eq!(shares, 0, "the puddles flicker");
}
