//! What falls: the game's own day decides whether anything does, the land or the player decides
//! the form, neither ever switches in one frame, and the weather's clock never coarsens.

use dereth_render_hifi::passes::weather::{self, Fall, Precipitation, EASE_SECONDS, TIME_PERIOD};
use dereth_render_hifi::snapshot::{AskedFall, HifiFrame, HifiTerrainBlock};
use glam::Vec3;

/// A terrain word of terrain type `t`, no road.
const fn word(t: u16) -> u16 {
    t << 2
}

/// Grassland, and snow.
const GRASS: u16 = word(1);
const SNOW: u16 = word(15);

/// A frame outdoors on a day of `group`, weather drawn or not, the viewer in the middle of a
/// three-by-three of blocks whose land within `snowy` metres east of the eye's block's west
/// edge, and everything west of it, is snow; the rest grass.
fn frame(group: &str, weather_enabled: bool, snow_east_of: f32) -> HifiFrame {
    let mut f = HifiFrame::default();
    f.sky.outdoor = true;
    f.sky.day_group = group.to_owned();
    f.sky.weather_enabled = weather_enabled;
    f.camera.eye = Vec3::new(288.0, 288.0, 50.0);
    for x in 0..3u8 {
        for y in 0..3u8 {
            let origin = Vec3::new(f32::from(x) * 192.0, f32::from(y) * 192.0, 0.0);
            let mut words = [GRASS; 81];
            for (i, w) in words.iter_mut().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let east = origin.x + (i / 9) as f32 * 24.0;
                if east < 192.0 + snow_east_of {
                    *w = SNOW;
                }
            }
            f.terrain.push(HifiTerrainBlock {
                block: u16::from(x) << 8 | u16::from(y),
                origin,
                words,
                ..HifiTerrainBlock::default()
            });
        }
    }
    f
}

/// The snow line `east` metres into the eye's block: the share of snow round the eye.
fn share_at(east: f32) -> f32 {
    weather::snow_share(&frame("Rainy", true, east)).expect("land in reach")
}

/// The weather as it stands after one first step on `f`, with `asked` the form the player asked
/// for.
fn first(f: &HifiFrame, asked: AskedFall) -> Fall {
    let mut fall = Fall::default();
    fall.step(weather::falls(f), weather::snow_share(f), asked, 0.0);
    fall
}

/// Behaviour: hifi.weather.it-falls-on-the-games-rainy-days-as-rain-or-over-snowy-land-as-snow
#[test]
fn it_falls_on_a_rainy_day_as_rain_and_over_snowy_land_as_snow() {
    use Precipitation::{None as Dry, Rain, Snow};
    // The game's day decides whether anything falls.
    assert_eq!(
        first(&frame("Rainy", true, -1e4), AskedFall::Land).precipitation(),
        Rain
    );
    for group in ["Sunny", "Clear", "Cloudy", ""] {
        assert_eq!(
            first(&frame(group, true, -1e4), AskedFall::Land).precipitation(),
            Dry,
            "{group}"
        );
    }
    // Not with the game's weather turned off.
    assert_eq!(
        first(&frame("Rainy", false, -1e4), AskedFall::Land).precipitation(),
        Dry
    );
    // Indoors it still falls outside: the viewer stepping out finds it falling.
    let mut indoors = frame("Rainy", true, -1e4);
    indoors.sky.outdoor = false;
    assert!(weather::falls(&indoors));
    // The land decides the form: the share of snow round the eye, the nearer land counting
    // more, against four tenths when the weather starts.
    assert!(share_at(-1e4) == 0.0 && (share_at(1e4) - 1.0).abs() < 1e-6);
    // The share moves smoothly as the viewer walks, across a block's edge too: no whole block
    // comes into it or leaves it at once.
    let mut walk = frame("Rainy", true, 40.0);
    let mut last = None;
    for step in 0..480 {
        #[allow(clippy::cast_precision_loss)]
        let x = 280.0 + step as f32 * 0.25;
        walk.camera.eye.x = x;
        let share = weather::snow_share(&walk).expect("land in reach");
        if let Some(l) = last {
            let moved: f32 = share - l;
            assert!(moved.abs() < 0.005, "at {x}: the share jumped {moved}");
        }
        last = Some(share);
    }
    let line = |share: f32| {
        let (mut lo, mut hi) = (-300.0f32, 300.0f32);
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            if share_at(mid) < share {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        hi
    };
    let at_mark = line(0.4);
    assert_eq!(
        first(&frame("Rainy", true, at_mark - 2.0), AskedFall::Land).precipitation(),
        Rain
    );
    assert_eq!(
        first(&frame("Rainy", true, at_mark + 2.0), AskedFall::Land).precipitation(),
        Snow
    );
    assert_eq!(
        first(&frame("Rainy", true, 1e4), AskedFall::Land).precipitation(),
        Snow
    );
    // Snowy land on a dry day stays dry.
    assert_eq!(
        first(&frame("Sunny", true, 1e4), AskedFall::Land).precipitation(),
        Dry
    );
    // The form the player asks for takes the land's place: rain over snow, snow over grass. What
    // day it is stays the sky's to say, and the player's options can still turn it off.
    assert_eq!(
        first(&frame("Rainy", true, 1e4), AskedFall::Rain).precipitation(),
        Rain
    );
    assert_eq!(
        first(&frame("Rainy", true, -1e4), AskedFall::Snow).precipitation(),
        Snow
    );
    assert_eq!(
        first(&frame("Sunny", true, -1e4), AskedFall::Snow).precipitation(),
        Dry
    );
    assert_eq!(
        first(&frame("Rainy", false, -1e4), AskedFall::Snow).precipitation(),
        Dry
    );
}

/// Behaviour: hifi.weather.rain-snow-and-a-dry-day-ease-into-each-other
#[test]
fn rain_snow_and_a_dry_day_ease_into_each_other_and_the_form_holds_near_its_mark() {
    let dt = 1.0 / 60.0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few hundred frames
    let frames = |seconds: f32| (seconds / dt).round() as usize;
    let rainy = frame("Rainy", true, -1e4);
    let mut fall = first(&rainy, AskedFall::Land);
    assert_eq!((fall.amount(), fall.snow()), (1.0, 0.0));
    // Near the mark the form holds: a share between the two thresholds never turns rain to
    // snow, however long it stays.
    for share in [0.36, 0.4, 0.44] {
        for _ in 0..frames(10.0) {
            fall.step(true, Some(share), AskedFall::Land, dt);
        }
        assert_eq!(
            fall.snow(),
            0.0,
            "rain turned to snow at a share of {share}"
        );
    }
    // Well past it the rain turns to snow, a little each frame, over the easing time.
    let mut last = fall.snow();
    for i in 1..=frames(EASE_SECONDS) {
        fall.step(true, Some(0.46), AskedFall::Land, dt);
        let step = fall.snow() - last;
        assert!(
            step > 0.0 && step <= dt / EASE_SECONDS + 1e-6,
            "frame {i}: snow moved {step}"
        );
        last = fall.snow();
    }
    assert!((fall.snow() - 1.0).abs() < 1e-3, "{}", fall.snow());
    // And holds as snow until the share falls well under the mark.
    for share in [0.44, 0.4, 0.36] {
        for _ in 0..frames(10.0) {
            fall.step(true, Some(share), AskedFall::Land, dt);
        }
        assert!(
            (fall.snow() - 1.0).abs() < 1e-3,
            "snow turned to rain at {share}"
        );
    }
    // No land in reach keeps the form.
    for _ in 0..frames(10.0) {
        fall.step(true, None, AskedFall::Land, dt);
    }
    assert!((fall.snow() - 1.0).abs() < 1e-3);
    // A dry day: the weather eases out over the easing time, never at once.
    fall.step(false, Some(0.9), AskedFall::Land, dt);
    assert!(fall.amount() > 0.99, "the weather stopped at once");
    for _ in 0..frames(EASE_SECONDS) {
        fall.step(false, Some(0.9), AskedFall::Land, dt);
    }
    assert_eq!(fall.amount(), 0.0);
    assert_eq!(fall.precipitation(), Precipitation::None);
    // And back in over the same time.
    fall.step(true, Some(0.9), AskedFall::Land, dt);
    assert!(fall.amount() < 0.01, "the weather came back at once");
    // The form the player asks for eases in the same way: snow asked for over grass turns the
    // rain to snow a little each frame, never at once.
    let mut fall = first(&rainy, AskedFall::Land);
    fall.step(true, Some(0.0), AskedFall::Snow, dt);
    assert!(
        fall.snow() > 0.0 && fall.snow() <= dt / EASE_SECONDS + 1e-6,
        "asked for, the snow came at once: {}",
        fall.snow()
    );
    for _ in 0..frames(EASE_SECONDS) {
        fall.step(true, Some(0.0), AskedFall::Snow, dt);
    }
    assert!((fall.snow() - 1.0).abs() < 1e-3, "{}", fall.snow());
}

/// Behaviour: hifi.weather.the-weathers-motion-never-coarsens-in-a-long-session
#[test]
fn the_weathers_clock_steps_as_finely_after_hours_of_play_as_at_the_start() {
    let frame = 1.0 / 60.0;
    let (frame_f32, period) = (1.0f32 / 60.0, 1200.0f32);
    assert!((TIME_PERIOD - f64::from(period)).abs() < f64::EPSILON);
    for hours in [0.0, 1.0, 10.0, 100.0, 1000.0] {
        let start = hours * 3600.0;
        let mut last = None;
        let mut before: Option<f32> = None;
        for i in 0..240 {
            let now = start + f64::from(i) * frame;
            let (t, step) = weather::clock_reading(now, last);
            assert!(
                (0.0..period).contains(&t),
                "{hours} hours: {t} is outside the period"
            );
            if let Some(b) = before {
                // Each frame moves the clock by a frame, to a hundredth of one, but where the
                // period wraps it round.
                let moved: f32 = t - b;
                let wrapped = moved < 0.0;
                if !wrapped {
                    assert!(
                        (moved - frame_f32).abs() < frame_f32 * 1e-2,
                        "{hours} hours: the clock moved {moved} in a frame"
                    );
                }
                assert!(
                    (step - frame_f32).abs() < frame_f32 * 1e-2,
                    "{hours} hours: the step is {step}"
                );
            }
            last = Some(now);
            before = Some(t);
        }
    }
}
