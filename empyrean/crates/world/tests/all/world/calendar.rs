//! Each era's calendar: where its game time starts, read on an injected clock.
//! Divergence: V568

use std::time::Duration;

use empyrean_common::calendar::{self, Calendar};
use empyrean_common::clock::{Clock, VirtualClock};
use empyrean_common::dereth_date_time::{DerethDateTime, Months};
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::era::{EraExt as _, EraId};
use empyrean_world::entity::timers::TimersState;

/// The world's launch, in UTC.
fn launch() -> DotNetDateTime {
    DotNetDateTime::new_hms(2026, 12, 1, 18, 0, 0)
}

/// A New Dereth world started when `clock` reads.
fn new_dereth(clock: &dyn Clock) -> TimersState {
    TimersState::for_era(clock, EraId::NewDereth.rules().calendar, Some(launch()))
        .expect("the launch is given")
}

/// The date game time `t` is, as the server's calendar reads it: the month, day and year.
fn read(t: f64) -> (Months, i32, i32) {
    let d = DerethDateTime::from_game_ticks(t);
    (d.month_name(), d.day(), d.year())
}

/// Divergence: V568
#[test]
fn a_new_dereth_world_launches_on_leafcull_1_498_py_and_a_day_later_it_is_leafcull_2() {
    let clock = VirtualClock::new(launch());
    let at_launch = new_dereth(&clock).portal_year_ticks;
    assert_eq!(read(at_launch), (Months::Leafcull, 1, 498));
    // As the client reads it: the first moment of the year's day 210, Leafcull's first.
    assert_eq!(calendar::date_of(at_launch), (498, calendar::LEAFCULL, 0.0));

    // A world started a day of game time after the launch is a day on.
    clock.advance(Duration::from_secs_f64(calendar::DAY_SECONDS));
    let a_day_on = new_dereth(&clock).portal_year_ticks;
    assert_eq!(a_day_on - at_launch, calendar::DAY_SECONDS);
    assert_eq!(read(a_day_on), (Months::Leafcull, 2, 498));
    assert_eq!(
        calendar::date_of(a_day_on),
        (498, calendar::LEAFCULL + 1, 0.0)
    );
}

/// Divergence: V568
#[test]
fn a_new_dereth_world_is_not_started_without_its_launch() {
    let clock = VirtualClock::new(launch());
    let calendar = EraId::NewDereth.rules().calendar;
    assert!(calendar.needs_launch());
    assert!(TimersState::for_era(&clock, calendar, None).is_err());
}

/// Divergence: V568
#[test]
fn the_retail_eras_keep_aces_start_of_game_time() {
    let clock = VirtualClock::new(DotNetDateTime::new_hms(2026, 11, 23, 12, 34, 56));
    for era in [EraId::Eor, EraId::Launch, EraId::Infiltration] {
        assert_eq!(era.rules().calendar, Calendar::SinceRetailClosed, "{era:?}");
        let ours = TimersState::for_era(&clock, era.rules().calendar, Some(launch())).unwrap();
        assert_eq!(ours, TimersState::new(&clock), "{era:?}");
        assert_eq!(
            ours.portal_year_ticks,
            DerethDateTime::utc_now_to_emu_time(&clock).ticks(),
            "{era:?}"
        );
        // Within ACE's calendar, game time reads as ACE reads it.
        let t = ours.portal_year_ticks;
        assert_eq!(
            DerethDateTime::from_game_ticks(t).to_string(),
            DerethDateTime::from_ticks(t).to_string()
        );
    }
}
