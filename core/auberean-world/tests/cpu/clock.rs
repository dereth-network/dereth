//! The calendar, the day the sky shows, the light of a moment, and a waveform's value at a
//! moment (each shape, the rates its terms move at, the noise shape's one roll of the client's
//! generator).

use auberean_assets::common::{Color32, Waveform};
use auberean_assets::region::{DayDesc, GameTime, SkyDesc, SkyTime, WeightedDay};
use auberean_primitives::random::Random;
use auberean_primitives::{DataId, Vec3};
use auberean_world::color::Color;
use auberean_world::sky::{
    bracket, day_clock, day_for, day_light, lit_vertex, self_shadow_bit, sun_reaches, time_ratio,
    toward, DayLight,
};
use auberean_world::wave::{value, value_for_phase};
use dereth_primitives::num::math;

/// The day fraction is the time since the calendar's zero, in days, past the whole days.
///
/// Behaviour: world.sky.the-day-fraction-counts-from-the-calendars-zero
#[test]
fn the_day_fraction_counts_from_the_calendars_zero() {
    let cal = GameTime {
        zero_time: 4893.0,
        zero_year: 498,
        day_length: 7347.0,
        days_per_year: 360,
        ..Default::default()
    };
    let c = day_clock(&cal, 0.0);
    assert_eq!((c.year, c.day), (498, 0));
    assert!((c.fraction - 4893.0 / 7347.0).abs() < 1e-6);
    let c = day_clock(&cal, 7347.0 * 361.5 - 4893.0);
    assert_eq!((c.year, c.day), (499, 1));
    assert!((c.fraction - 0.5).abs() < 1e-6);
}

/// The sky's day for a calendar day is the hash of the day's number scaled to the frequency total,
/// matched against the days' running total: with frequencies 1, 1, 2 the hash's fraction picks the
/// first day below a quarter, the second below a half, else the third.
///
/// Behaviour: world.sky.the-sky-picks-its-day-by-a-hash-of-the-date
#[test]
fn the_sky_picks_its_day_by_a_hash_of_the_date() {
    let sky = SkyDesc {
        days: vec![
            WeightedDay {
                day: DataId(1),
                frequency: 1.0,
            },
            WeightedDay {
                day: DataId(2),
                frequency: 1.0,
            },
            WeightedDay {
                day: DataId(3),
                frequency: 2.0,
            },
        ],
        ..Default::default()
    };
    // The hash's sixteen bits from the transcription: day 0 0x0719, day 1 0xDB98, day 180000
    // 0x4D03, day 180001 0x47CB. Scaled to the total of 4: 0.11, 3.43, 1.20, 1.12.
    assert_eq!(day_for(&sky, 0), Some(DataId(1)));
    assert_eq!(day_for(&sky, 1), Some(DataId(3)));
    assert_eq!(day_for(&sky, 180_000), Some(DataId(2)));
    assert_eq!(day_for(&sky, 180_001), Some(DataId(2)));
    assert_eq!(day_for(&SkyDesc::default(), 5), None);
}

fn time(begin: f32, heading: f32, pitch: f32, bright: f32) -> SkyTime {
    SkyTime {
        begin,
        direct_heading: heading,
        direct_pitch: pitch,
        direct_brightness: bright,
        direct_color: Color32(0xFFFF_FFFF),
        ambient_color: Color32(0xFFFF_FFFF),
        ambient_brightness: 0.25,
        fog_color: Color32(0xFF80_8080),
        fog_max: 1000.0,
        ..Default::default()
    }
}

/// Between two times of day the light interpolates by how far the moment is from the first to
/// the second, wrapping past midnight; the sun's heading runs clockwise from north and its pitch
/// up from the horizon, and the light travels away from it.
///
/// Behaviour: world.sky.the-light-interpolates-between-times-of-day
#[test]
fn the_light_interpolates_between_times_of_day() {
    let day = DayDesc {
        times: vec![time(0.25, 90.0, 0.0, 0.0), time(0.5, 180.0, 60.0, 1.0)],
        ..Default::default()
    };
    assert_eq!(bracket(&day.times, 0.375), Some((0, 1, 0.5)));
    assert!(
        (time_ratio(0.75, 0.25, 0.0) - 0.5).abs() < 1e-6,
        "0.75 to 0.25 wraps: midnight is halfway"
    );
    let l = day_light(&day, 0.375);
    assert!((l.direct_brightness - 0.5).abs() < 1e-6);
    let sun = toward(135.0, 30.0);
    assert!(
        (l.light_direction.x + sun.x).abs() < 1e-5 && (l.light_direction.y + sun.y).abs() < 1e-5
    );
    assert!(
        (l.light_direction.z + 0.5).abs() < 1e-5,
        "pitch 30 at the midpoint"
    );
    let east = toward(90.0, 0.0);
    assert!(
        (east.x - 1.0).abs() < 1e-6 && east.y.abs() < 1e-6,
        "heading 90 is east"
    );
}

/// A terrain vertex is lit by ambient plus the sun's share by its normal, times its own color:
/// a vertex facing the sun gets both, one facing away only the ambient, and a shadowed one
/// only the ambient.
///
/// Behaviour: world.sky.terrain-vertices-are-lit-by-ambient-and-the-sun
#[test]
fn terrain_vertices_are_lit_by_ambient_and_the_sun() {
    let mut l = DayLight::DEFAULT;
    l.ambient_brightness = 0.25;
    l.direct_brightness = 0.5;
    l.light_direction = Vec3::new(0.0, 0.0, -1.0);
    let own = Color::new(1.0, 0.5, 1.0, 1.0);
    let up = Vec3::new(0.0, 0.0, 1.0);
    let lit = lit_vertex(&l, up, own, 1.0);
    assert!((lit.r - 0.75).abs() < 1e-6 && (lit.g - 0.375).abs() < 1e-6);
    let down = Vec3::new(0.0, 0.0, -1.0);
    assert!((lit_vertex(&l, down, own, 1.0).r - 0.25).abs() < 1e-6);
    assert!((lit_vertex(&l, up, own, 0.0).r - 0.25).abs() < 1e-6);
}

/// A terrain vertex's self-shadow bits are read for the stored sun direction nearest the light's:
/// a morning sun low in the east reads the first bit, an evening one in the west the last, and a
/// light travelling upward, from below the horizon, is nearest the middle direction (none) and
/// lights every vertex; a vertex whose bit is clear gets no sunlight.
///
/// Behaviour: world.sky.a-vertex-in-its-hills-shadow-gets-no-sun
#[test]
fn a_vertex_in_its_hills_shadow_gets_no_sun() {
    let east_morning = toward(90.0, 20.0);
    let travel = |v: Vec3| Vec3::new(-v.x, -v.y, -v.z);
    assert_eq!(self_shadow_bit(travel(east_morning)), Some(0));
    assert_eq!(self_shadow_bit(travel(toward(270.0, 20.0))), Some(7));
    assert_eq!(
        self_shadow_bit(travel(toward(110.0, 75.0))),
        Some(3),
        "the last morning direction before noon"
    );
    assert_eq!(
        self_shadow_bit(Vec3::new(0.0, 0.0, 1.0)),
        None,
        "light from below the horizon: every vertex lit"
    );
    assert!((sun_reaches(0b0000_0001, Some(0)) - 1.0).abs() < f32::EPSILON);
    assert!(sun_reaches(0b0000_0010, Some(0)).abs() < f32::EPSILON);
    assert!((sun_reaches(0, None) - 1.0).abs() < f32::EPSILON);
}

/// A game time's date: the year (and its name), the season the day of the year has reached and
/// the day of that season, the part of the day the day's fraction has reached; a calendar with no
/// seasons or no parts of the day has none.
///
/// Behaviour: world.sky.a-game-times-date-names-its-season-day-and-part-of-the-day
#[test]
fn a_game_times_date_names_its_season_day_and_part_of_the_day() {
    use auberean_assets::region::{DayPart, Season};
    use auberean_world::calendar::date;
    let part = |begin, name| DayPart {
        begin,
        night: 0,
        name,
    };
    let mut cal = GameTime {
        zero_time: 0.0,
        zero_year: 100,
        day_length: 1000.0,
        days_per_year: 360,
        year_name: 7,
        day_parts: vec![part(0.0, 10), part(0.25, 11), part(0.75, 12)],
        seasons: vec![
            Season { name: 20, begin: 0 },
            Season {
                name: 21,
                begin: 90,
            },
            Season {
                name: 22,
                begin: 180,
            },
        ],
        ..Default::default()
    };
    // Day 100 of the year, half way through it.
    let d = date(&cal, 1000.0 * 100.5).expect("a date");
    assert_eq!((d.year, d.year_name), (100, 7));
    assert_eq!((d.season, d.season_name, d.day), (1, 21, 11));
    assert_eq!((d.part, d.part_name), (1, 11));
    // The next year's first day, late.
    let d = date(&cal, 1000.0 * 360.9).expect("a date");
    assert_eq!((d.year, d.season, d.day, d.part), (101, 0, 1, 2));
    cal.seasons.clear();
    assert!(date(&cal, 0.0).is_none());
}

fn shaped(kind: u32) -> Waveform {
    Waveform {
        kind,
        base: 1.0,
        amplitude: 2.0,
        phase: 0.5,
        frequency: 3.0,
        ..Waveform::defaults()
    }
}

/// Steady, speed, sine, square and bounce: the base plus the phase in half turns plus the
/// frequency times the moment, or the amplitude times the sine of that (its sign, its size);
/// every term moves on at its rate.
///
/// Behaviour: world.wave.a-waveform-takes-its-shape-at-the-moment
#[test]
fn a_waveform_takes_its_shape_at_the_moment() {
    let rng = Random::new();
    let t = 0.25;
    let pi = std::f64::consts::PI;
    let angle = 3.0 * t + 0.5 * pi;
    let close = |a: f32, b: f64| (f64::from(a) - b).abs() < 1e-5;
    assert_eq!(value(&shaped(1), t, &rng), 1.0);
    assert!(close(value(&shaped(2), t, &rng), angle + 1.0));
    assert!(close(
        value(&shaped(4), t, &rng),
        math::sin(angle) * 2.0 + 1.0
    ));
    let square = if math::sin(angle) < 0.0 { -2.0 } else { 2.0 };
    assert!(close(value(&shaped(5), t, &rng), square + 1.0));
    assert!(close(
        value(&shaped(6), t, &rng),
        math::sin(angle).abs() * 2.0 + 1.0
    ));
    // the rates: a steady base that grows, and a sine whose amplitude grows
    let growing = Waveform {
        base_rate: 10.0,
        base: 50.0,
        ..Waveform::defaults()
    };
    assert_eq!(value(&growing, 2.0, &rng), 70.0);
    let swelling = Waveform {
        amplitude_rate: 1.0,
        ..shaped(4)
    };
    assert!(close(
        value(&swelling, t, &rng),
        math::sin(angle) * (2.0 + t) + 1.0
    ));
    // a phase given adds to the waveform's own, in half turns
    assert!(close(
        value_for_phase(&shaped(2), 0.5, t, &rng),
        3.0 * t + pi + 1.0
    ));
    // a shape the client does not know is the base
    assert_eq!(value(&shaped(12), t, &rng), 1.0);
}

/// The noise shape rolls the client's one generator once each time, between minus and plus its
/// amplitude about its base; no other shape draws on it.
///
/// Behaviour: world.wave.the-noise-shape-rolls-the-clients-generator-once
#[test]
fn the_noise_shape_rolls_the_clients_generator_once() {
    let (a, b) = (Random::new(), Random::new());
    let mut seen = Vec::new();
    for k in 0..20 {
        let v = value(&shaped(3), f64::from(k), &a);
        assert!((-1.0..=3.0).contains(&v), "{v}");
        seen.push(v);
        // the same generator drawn once by hand stays in step
        let r = b.draw();
        assert!((f64::from(v) - ((2.0 * r - 1.0) * 2.0 + 1.0)).abs() < 1e-5);
    }
    assert!(seen.windows(2).any(|w| w[0] != w[1]));
    // the other shapes leave the generator where it was
    for kind in [1, 2, 4, 5, 6, 7, 8, 9] {
        let _ = value(&shaped(kind), 1.5, &a);
    }
    assert_eq!(a.draw(), b.draw());
}

/// The frame loop shows its amplitude's frames at its frequency's rate, each frame stepping the
/// base on by its phase in half turns; a loop of no frames is its base.
///
/// Behaviour: world.wave.a-frame-loop-steps-through-its-frames
#[test]
fn a_frame_loop_steps_through_its_frames() {
    let rng = Random::new();
    // four frames at 24 a second, a quarter each (the 4 by 4 picture's columns)
    let w = Waveform {
        kind: 9,
        base: 0.0,
        amplitude: 4.0,
        frequency: 24.0,
        phase: 0.25 / std::f32::consts::PI,
        ..Waveform::defaults()
    };
    let at = |t: f64| value(&w, t, &rng);
    let frame = 1.0 / 24.0;
    assert!(at(0.2 * frame).abs() < 1e-6);
    assert!((at(1.2 * frame) - 0.25).abs() < 1e-6);
    assert!((at(3.5 * frame) - 0.75).abs() < 1e-6);
    // the fifth frame starts the next round
    assert!(at(4.5 * frame).abs() < 1e-6);
    let none = Waveform {
        amplitude: 0.0,
        ..w
    };
    assert_eq!(value(&none, 0.3, &rng), 0.0);
}

/// A calendar of 1,000 second days and 360 day years with three parts of the day and three
/// seasons, its zero at year 100.
fn running_calendar() -> GameTime {
    use auberean_assets::region::{DayPart, Season};
    let part = |begin, night, name| DayPart { begin, night, name };
    GameTime {
        zero_time: 0.0,
        zero_year: 100,
        day_length: 1000.0,
        days_per_year: 360,
        year_name: 7,
        day_parts: vec![part(0.0, 1, 10), part(0.25, 0, 11), part(0.75, 1, 12)],
        seasons: vec![
            Season { name: 20, begin: 0 },
            Season {
                name: 21,
                begin: 90,
            },
            Season {
                name: 22,
                begin: 180,
            },
        ],
        ..Default::default()
    }
}

/// The running clock works out the year, the day, the season and the part of the day at a change
/// of the part only; between changes only the time of day moves.
///
/// Behaviour: world.calendar.the-running-clock-updates-at-a-change-of-the-part-of-the-day
#[test]
fn the_running_clock_updates_at_a_change_of_the_part_of_the_day() {
    use auberean_world::calendar::GameClock;
    let mut c = GameClock::new(running_calendar());
    c.use_time(100_500.0);
    assert_eq!((c.year(), c.day(), c.time_of_day()), (100, 100, 0.5));
    assert_eq!(c.night(), 0, "the middle part is day");
    let d = c.current_date_time().expect("a date");
    assert_eq!(
        (d.year, d.day, d.season_name, d.part_name),
        (100, 11, 21, 11)
    );
    // Past the next change, in the same day: the night part.
    c.use_time(100_800.0);
    assert_eq!(c.night(), 1);
    assert_eq!(c.current_date_time().unwrap().part_name, 12);
    // Earlier than the next change, even a part earlier: only the time of day moves.
    c.use_time(100_500.0);
    assert_eq!(c.time_of_day(), 0.5);
    assert_eq!(c.current_date_time().unwrap().part_name, 12);
    // Past the day's end.
    c.use_time(101_100.0);
    assert_eq!(
        (c.day(), c.current_date_time().unwrap().part_name),
        (101, 10)
    );
    // A year and a bit on.
    c.use_time(360_000.0 + 100.0);
    assert_eq!((c.year(), c.day()), (101, 0));
}

/// Dilating runs the world's time faster from where it is: the time of day holds, then moves at
/// the new rate. At or below 0.0002 it is refused.
///
/// Behaviour: world.calendar.dilating-time-keeps-the-time-of-day
#[test]
fn dilating_time_keeps_the_time_of_day_and_runs_it_at_the_rate() {
    use auberean_world::calendar::GameClock;
    let mut c = GameClock::new(running_calendar());
    let t = 100_500.0;
    c.use_time(t);
    assert!(!c.dilate(0.0002, t), "too slow");
    assert_eq!(c.dilation(), 1.0);
    assert!(c.dilate(2.0, t));
    assert_eq!(c.dilation(), 2.0);
    c.use_time(t);
    assert!((c.time_of_day() - 0.5).abs() < 1e-6, "{}", c.time_of_day());
    assert_eq!(c.day(), 100);
    c.use_time(t + 100.0);
    assert!((c.time_of_day() - 0.7).abs() < 1e-6, "{}", c.time_of_day());
}

/// Moving the time of day goes straight there and asks for the land to be lit; the world's offset
/// stays above minus one day.
///
/// Behaviour: world.calendar.moving-the-time-of-day-goes-straight-there
#[test]
fn moving_the_time_of_day_goes_straight_there_and_asks_for_light() {
    use auberean_world::calendar::GameClock;
    let mut c = GameClock::new(running_calendar());
    c.use_time(100_500.0);
    assert!(!c.needs_light());
    c.alter_time_of_day(0.9, 100_500.0);
    assert!((c.time_of_day() - 0.9).abs() < 1e-6);
    assert_eq!(c.current_date_time().unwrap().part_name, 12);
    assert!(c.needs_light());
    c.lit();
    assert!(!c.needs_light());
    assert!((c.time_til_tomorrow() - 100.0).abs() < 1e-3);
    // Back: the offset would go below zero, so it wraps a day forward.
    c.alter_time_of_day(0.1, 100_500.0);
    assert!(
        (c.world_time(0.0) - 600.0).abs() < 1e-3,
        "{}",
        c.world_time(0.0)
    );
}

/// A stamp's date takes the present year, day and season; only the part of the day comes from the
/// stamp.
///
/// Behaviour: world.calendar.a-stamps-date-takes-the-present-year-and-day
#[test]
fn a_stamps_date_takes_the_present_year_day_and_season() {
    use auberean_world::calendar::GameClock;
    let mut c = GameClock::new(running_calendar());
    c.use_time(100_500.0);
    // A stamp in another year and season, its time of day in the morning of its own day counted
    // against the present day: its seconds into the year are 200,100 less 100,000.
    let d = c.date_time_at(360_000.0 * 3.0 + 100_100.0).expect("a date");
    assert_eq!((d.year, d.day, d.season_name), (100, 11, 21));
    assert_eq!(d.part_name, 10);
    let d = c.date_time_at(100_500.0).expect("a date");
    assert_eq!(d.part_name, 11);
    // A stamp 100 days on: still the present season, and its seconds are past the present day's
    // end, so the last part.
    let d = c.date_time_at(200_100.0).expect("a date");
    assert_eq!((d.season_name, d.part_name), (21, 12));
}

/// A clock with no calendar is one long day in a one-day year; a calendar's date needs its lists.
///
/// Behaviour: world.calendar.a-clock-with-no-calendar-is-one-long-day
#[test]
fn a_clock_with_no_calendar_is_one_long_day() {
    use auberean_world::calendar::GameClock;
    let mut c = GameClock::default();
    c.use_time(50_000.0);
    assert_eq!(c.time_of_day(), 0.5);
    assert_eq!((c.day_length_seconds(), c.days_per_year()), (100_000.0, 1));
    assert!(c.current_date_time().is_none());
    assert_eq!(c.night(), 0);
}
