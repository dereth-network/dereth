// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Timers.cs
//! Port of `Source/ACE.Server/Entity/Timers.cs`.
//!
//! ACE's `Timers` is a static class read everywhere. Its start values live in [`TimersState`] (a
//! field of `World`); every read of the current time goes through `w.now`, the clocks captured once
//! per tick. `PortalYearTicks` is world state: the world
//! loop advances it by each iteration's measured time with
//! [`advance_portal_year_ticks`] and takes the next tick's `ClockSnapshot` from it.

use empyrean_common::clock::{Clock, ClockSnapshot};
use empyrean_common::dereth_date_time::DerethDateTime;
use empyrean_common::dotnet::datetime::{DotNetDateTime, TimeSpan};

use crate::World;

// ACE: Timers
/// The mutable static state of ACE's `Timers`, held as a field of `World`.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct TimersState {
    /// `WorldStartTime`: `DateTime.UtcNow` when the server started.
    pub world_start_time: DotNetDateTime,
    /// `WorldStartLoreTime.Ticks`: `DerethDateTime.UtcNowToEMUTime` when the server started. Held
    /// as ticks so that an arbitrary test clock never fails the calendar's range check here.
    pub world_start_lore_ticks: f64,
    /// `PortalYearTicks`: game time in seconds, advanced by the world loop. Readers inside a tick
    /// use [`portal_year_ticks`], which reads the snapshot taken from this value.
    pub portal_year_ticks: f64,
}

impl TimersState {
    /// ACE's static initialisers: `WorldStartTime = DateTime.UtcNow`,
    /// `WorldStartLoreTime = DerethDateTime.UtcNowToEMUTime`, and
    /// `PortalYearTicks = WorldStartLoreTime.Ticks`.
    ///
    /// # Panics
    /// When `clock` reads before 2017-01-31 17:00 UTC (see `DerethDateTime::utc_now_to_emu_time`).
    pub fn new(clock: &dyn Clock) -> Self {
        let world_start_time = clock.utc_now();
        let world_start_lore_ticks = DerethDateTime::utc_now_to_emu_time(clock).ticks();
        TimersState {
            world_start_time,
            world_start_lore_ticks,
            portal_year_ticks: world_start_lore_ticks,
        }
    }

    /// The start state for a world whose first tick is `now`: its UTC time is the start time and
    /// its game time is the start lore time. This is what `World::new` should use.
    pub fn from_snapshot(now: &ClockSnapshot) -> Self {
        TimersState {
            world_start_time: now.utc,
            world_start_lore_ticks: now.portal_year_ticks,
            portal_year_ticks: now.portal_year_ticks,
        }
    }
}

// ACE: Timers.WorldStartTime
pub fn world_start_time(w: &World) -> DotNetDateTime {
    w.timers.world_start_time
}

// ACE: Timers.CurrentTime
/// `DateTime.UtcNow`, as captured for this tick.
// DIVERGE: ACE reads the live clock; this reads the tick's snapshot.
pub fn current_time(w: &World) -> DotNetDateTime {
    w.now.utc
}

// ACE: Timers.RunningTime
/// `(CurrentTime - WorldStartTime).TotalSeconds`.
pub fn running_time(w: &World) -> f64 {
    (current_time(w) - world_start_time(w)).total_seconds()
}

// ACE: Timers.WorldStartLoreTime
///
/// # Panics
/// When the start ticks are outside the calendar (`ArgumentOutOfRangeException`).
pub fn world_start_lore_time(w: &World) -> DerethDateTime {
    DerethDateTime::from_ticks(w.timers.world_start_lore_ticks)
}

// ACE: Timers.CurrentLoreTime
/// `DerethDateTime.UtcNowToLoreTime`, from this tick's UTC time.
pub fn current_lore_time(w: &World) -> DerethDateTime {
    DerethDateTime::convert_real_world_to_lore_date_time(current_time(w))
}

// ACE: Timers.CurrentInGameTime
/// `new DerethDateTime(PortalYearTicks)`: the in-game time shown on the map panel.
///
/// # Panics
/// When `PortalYearTicks` is outside the calendar (`ArgumentOutOfRangeException`).
pub fn current_in_game_time(w: &World) -> DerethDateTime {
    DerethDateTime::from_ticks(portal_year_ticks(w))
}

// ACE: Timers.PortalYearTicks
/// The current Portal Year time in seconds, unchanged within one tick.
pub fn portal_year_ticks(w: &World) -> f64 {
    w.now.portal_year_ticks
}

/// `Timers.PortalYearTicks += worldTickTimer.Elapsed.TotalSeconds`, the last statement of each
/// `WorldManager.UpdateWorld` iteration. The world loop calls it with the iteration's
/// measured time and takes the next tick's snapshot from the result, which it returns.
pub fn advance_portal_year_ticks(w: &mut World, elapsed: TimeSpan) -> f64 {
    w.timers.portal_year_ticks += elapsed.total_seconds();
    w.timers.portal_year_ticks
}
