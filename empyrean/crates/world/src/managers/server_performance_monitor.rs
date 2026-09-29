// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Managers/ServerPerformanceMonitor.cs
//! Port of `Source/ACE.Server/Managers/ServerPerformanceMonitor.cs`: timings of the world loop's
//! stages over rolling ~5 minute, ~1 hour and ~24 hour windows.
//!
//! ACE's monitor only measures: every hook returns at once unless the monitor was started
//! (`IsRunning`, off unless `ServerPerformanceMonitorAutoStart` or the `serverperformance`
//! command turns it on).
//!
//! DIVERGE (arch): ACE's `Stopwatch`es and `DateTime.UtcNow` read the machine's clocks. Here they
//! read the clock installed with [`use_clock`] (the server host installs its system clock, a test
//! its virtual one); with none installed they read the tick's frozen snapshot, so every event lasts
//! zero seconds.

use std::sync::Arc;

use empyrean_common::clock::{Clock, SnapshotClock};
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::{align, format, CsCast, TimeSpan};
use empyrean_common::performance::rate_monitor::RateMonitor;
use empyrean_common::performance::timed_event_history::TimedEventHistory;

use crate::World;

// ACE: ServerPerformanceMonitor.MonitorType
/// What `RestartEvent` / `RegisterEventEnd` time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorType {
    // These are all found in WorldManager.UpdateWorld()
    PlayerManagerTick,
    NetworkManagerInboundClientMessageQueueRun,
    ActionQueueRunActions,
    DelayManagerRunActions,
    UpdateGameWorld,
    NetworkManagerDoSessionWork,

    // These are all found in WorldManager.UpdateGameWorld()
    UpdateGameWorldEntire,
    LandblockManagerTickPhysics,
    LandblockManagerTickMultiThreadedWork,
    LandblockManagerTickSingleThreadedWork,

    // These are all found in NetworkManager.DoSessionWork()
    DoSessionWorkTickOutbound,
    DoSessionWorkRemoveSessions,
}

impl MonitorType {
    /// `MonitorType.MaxItems`: keep this at the end to properly size our monitors array.
    pub const MAX_ITEMS: usize = 12;

    /// Every member, by value.
    pub const ALL: [Self; Self::MAX_ITEMS] = [
        Self::PlayerManagerTick,
        Self::NetworkManagerInboundClientMessageQueueRun,
        Self::ActionQueueRunActions,
        Self::DelayManagerRunActions,
        Self::UpdateGameWorld,
        Self::NetworkManagerDoSessionWork,
        Self::UpdateGameWorldEntire,
        Self::LandblockManagerTickPhysics,
        Self::LandblockManagerTickMultiThreadedWork,
        Self::LandblockManagerTickSingleThreadedWork,
        Self::DoSessionWorkTickOutbound,
        Self::DoSessionWorkRemoveSessions,
    ];

    /// `ToString()`: ACE's member name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PlayerManagerTick => "PlayerManager_Tick",
            Self::NetworkManagerInboundClientMessageQueueRun => {
                "NetworkManager_InboundClientMessageQueueRun"
            }
            Self::ActionQueueRunActions => "actionQueue_RunActions",
            Self::DelayManagerRunActions => "DelayManager_RunActions",
            Self::UpdateGameWorld => "UpdateGameWorld",
            Self::NetworkManagerDoSessionWork => "NetworkManager_DoSessionWork",
            Self::UpdateGameWorldEntire => "UpdateGameWorld_Entire",
            Self::LandblockManagerTickPhysics => "LandblockManager_TickPhysics",
            Self::LandblockManagerTickMultiThreadedWork => "LandblockManager_TickMultiThreadedWork",
            Self::LandblockManagerTickSingleThreadedWork => {
                "LandblockManager_TickSingleThreadedWork"
            }
            Self::DoSessionWorkTickOutbound => "DoSessionWork_TickOutbound",
            Self::DoSessionWorkRemoveSessions => "DoSessionWork_RemoveSessions",
        }
    }
}

// ACE: ServerPerformanceMonitor.CumulativeEventHistoryType
/// The monitors resumed and paused many times within one `UpdateGameWorld`. Their purpose is to
/// give a performance value for a system and all the work it may process in a single loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CumulativeEventHistoryType {
    // These are found in Player_Tick.cs and WorldObject_Tick.cs
    PlayerTickUpdateObjectPhysics,
    WorldObjectTickUpdateObjectPhysics,

    // These are all found in Landblock.TickMultiThreadedWork()
    LandblockTickRunActions,
    LandblockTickMonsterTick,
    LandblockTickGeneratorUpdate,
    LandblockTickGeneratorRegeneration,
    LandblockTickHeartbeat,
    LandblockTickDatabaseSave,

    // These are all found in Landblock.TickSingleThreadedWork()
    LandblockTickPlayerTick,
    LandblockTickWorldObjectHeartbeat,

    // These are all found in various places and are cumulative per LandblockManager.Tick()
    MonsterAwarenessFindNextTarget,
    MonsterNavigationUpdatePositionPuo,
    LootGenerationFactoryCreateRandomLootObjects,
}

impl CumulativeEventHistoryType {
    /// `CumulativeEventHistoryType.MaxItems`.
    pub const MAX_ITEMS: usize = 13;

    /// Every member, by value.
    pub const ALL: [Self; Self::MAX_ITEMS] = [
        Self::PlayerTickUpdateObjectPhysics,
        Self::WorldObjectTickUpdateObjectPhysics,
        Self::LandblockTickRunActions,
        Self::LandblockTickMonsterTick,
        Self::LandblockTickGeneratorUpdate,
        Self::LandblockTickGeneratorRegeneration,
        Self::LandblockTickHeartbeat,
        Self::LandblockTickDatabaseSave,
        Self::LandblockTickPlayerTick,
        Self::LandblockTickWorldObjectHeartbeat,
        Self::MonsterAwarenessFindNextTarget,
        Self::MonsterNavigationUpdatePositionPuo,
        Self::LootGenerationFactoryCreateRandomLootObjects,
    ];

    /// `ToString()`: ACE's member name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PlayerTickUpdateObjectPhysics => "Player_Tick_UpdateObjectPhysics",
            Self::WorldObjectTickUpdateObjectPhysics => "WorldObject_Tick_UpdateObjectPhysics",
            Self::LandblockTickRunActions => "Landblock_Tick_RunActions",
            Self::LandblockTickMonsterTick => "Landblock_Tick_Monster_Tick",
            Self::LandblockTickGeneratorUpdate => "Landblock_Tick_GeneratorUpdate",
            Self::LandblockTickGeneratorRegeneration => "Landblock_Tick_GeneratorRegeneration",
            Self::LandblockTickHeartbeat => "Landblock_Tick_Heartbeat",
            Self::LandblockTickDatabaseSave => "Landblock_Tick_Database_Save",
            Self::LandblockTickPlayerTick => "Landblock_Tick_Player_Tick",
            Self::LandblockTickWorldObjectHeartbeat => "Landblock_Tick_WorldObject_Heartbeat",
            Self::MonsterAwarenessFindNextTarget => "Monster_Awareness_FindNextTarget",
            Self::MonsterNavigationUpdatePositionPuo => "Monster_Navigation_UpdatePosition_PUO",
            Self::LootGenerationFactoryCreateRandomLootObjects => {
                "LootGenerationFactory_CreateRandomLootObjects"
            }
        }
    }
}

/// `last5mClearInterval`.
fn last_5m_clear_interval() -> TimeSpan {
    TimeSpan::from_minutes(5.0)
}

/// `last1hClearInteval`.
fn last_1h_clear_inteval() -> TimeSpan {
    TimeSpan::from_hours(1.0)
}

/// `last24hClearInterval`.
fn last_24h_clear_interval() -> TimeSpan {
    TimeSpan::from_hours(24.0)
}

// ACE: ServerPerformanceMonitor
/// The mutable static state of ACE's `ServerPerformanceMonitor`, held as a field of `World`.
#[derive(Debug)]
pub struct ServerPerformanceMonitorState {
    // ACE: ServerPerformanceMonitor.IsRunning
    pub is_running: bool,
    // ACE: ServerPerformanceMonitor.IsRunningCumulative
    pub is_running_cumulative: bool,

    monitors_5m: [RateMonitor; MonitorType::MAX_ITEMS],
    monitors_1h: [RateMonitor; MonitorType::MAX_ITEMS],
    monitors_24h: [RateMonitor; MonitorType::MAX_ITEMS],

    cumulative_5m: [TimedEventHistory; CumulativeEventHistoryType::MAX_ITEMS],
    cumulative_1h: [TimedEventHistory; CumulativeEventHistoryType::MAX_ITEMS],
    cumulative_24h: [TimedEventHistory; CumulativeEventHistoryType::MAX_ITEMS],

    last_5m_clear: DotNetDateTime,
    last_1h_clear: DotNetDateTime,
    last_24h_clear: DotNetDateTime,

    cumulative_seconds: [f64; CumulativeEventHistoryType::MAX_ITEMS],

    /// Not ACE: the clock the stopwatches and `DateTime.UtcNow` read (see the module docs).
    clock: Option<Arc<dyn Clock>>,
}

impl Default for ServerPerformanceMonitorState {
    // ACE: ServerPerformanceMonitor.ServerPerformanceMonitor
    /// `static ServerPerformanceMonitor()`: a `RateMonitor` per monitor and window and a
    /// `TimedEventHistory` per cumulative type and window; the clear times are
    /// `DateTime.MinValue`.
    fn default() -> Self {
        Self {
            is_running: false,
            is_running_cumulative: false,
            monitors_5m: std::array::from_fn(|_| RateMonitor::new()),
            monitors_1h: std::array::from_fn(|_| RateMonitor::new()),
            monitors_24h: std::array::from_fn(|_| RateMonitor::new()),
            cumulative_5m: std::array::from_fn(|_| TimedEventHistory::new()),
            cumulative_1h: std::array::from_fn(|_| TimedEventHistory::new()),
            cumulative_24h: std::array::from_fn(|_| TimedEventHistory::new()),
            last_5m_clear: DotNetDateTime::MIN_VALUE,
            last_1h_clear: DotNetDateTime::MIN_VALUE,
            last_24h_clear: DotNetDateTime::MIN_VALUE,
            cumulative_seconds: [0.0; CumulativeEventHistoryType::MAX_ITEMS],
            clock: None,
        }
    }
}

/// Not ACE: installs the clock the monitor's stopwatches and `DateTime.UtcNow` read.
pub fn use_clock(w: &mut World, clock: Arc<dyn Clock>) {
    w.performance.clock = Some(clock);
}

fn clock(w: &World) -> Arc<dyn Clock> {
    w.performance
        .clock
        .clone()
        .unwrap_or_else(|| Arc::new(SnapshotClock(w.now)))
}

/// `DateTime.UtcNow`.
fn utc_now(w: &World) -> DotNetDateTime {
    clock(w).utc_now()
}

// ACE: ServerPerformanceMonitor.Monitors5mRunTime
fn monitors_5m_run_time(w: &World) -> TimeSpan {
    utc_now(w) - w.performance.last_5m_clear
}

// ACE: ServerPerformanceMonitor.Monitors1hRunTime
fn monitors_1h_run_time(w: &World) -> TimeSpan {
    utc_now(w) - w.performance.last_1h_clear
}

// ACE: ServerPerformanceMonitor.Monitors24hRunTime
fn monitors_24h_run_time(w: &World) -> TimeSpan {
    utc_now(w) - w.performance.last_24h_clear
}

// ACE: ServerPerformanceMonitor.Start
pub fn start(w: &mut World) {
    if w.performance.is_running {
        return;
    }

    // ACE calls Reset() here while IsRunning is still false, so it returns at once; the first
    // Tick then clears the histories (their clear times start at DateTime.MinValue).
    reset(w);

    w.performance.is_running = true;
    w.performance.is_running_cumulative = true;
}

// ACE: ServerPerformanceMonitor.Stop
pub fn stop(w: &mut World) {
    if !w.performance.is_running {
        return;
    }

    w.performance.is_running = false;
    w.performance.is_running_cumulative = false;
}

// ACE: ServerPerformanceMonitor.StartCumulative
pub fn start_cumulative(w: &mut World) {
    if w.performance.is_running_cumulative {
        return;
    }

    w.performance.is_running_cumulative = true;
}

// ACE: ServerPerformanceMonitor.StopCumulative
pub fn stop_cumulative(w: &mut World) {
    if !w.performance.is_running_cumulative {
        return;
    }

    w.performance.is_running_cumulative = false;
}

// ACE: ServerPerformanceMonitor.Tick
/// Clears the 5 min / 1 h / 24 h histories when their interval has passed.
pub fn tick(w: &mut World) {
    if !w.performance.is_running {
        return;
    }

    // check to see if we should clear history
    if utc_now(w) - w.performance.last_5m_clear >= last_5m_clear_interval() {
        for monitor in &mut w.performance.monitors_5m {
            monitor.clear_event_history();
        }

        for event_history in &mut w.performance.cumulative_5m {
            event_history.clear_history();
        }

        w.performance.last_5m_clear = utc_now(w);
    }

    if utc_now(w) - w.performance.last_1h_clear >= last_1h_clear_inteval() {
        for monitor in &mut w.performance.monitors_1h {
            monitor.clear_event_history();
        }

        for event_history in &mut w.performance.cumulative_1h {
            event_history.clear_history();
        }

        w.performance.last_1h_clear = utc_now(w);
    }

    if utc_now(w) - w.performance.last_24h_clear >= last_24h_clear_interval() {
        for monitor in &mut w.performance.monitors_24h {
            monitor.clear_event_history();
        }

        for event_history in &mut w.performance.cumulative_24h {
            event_history.clear_history();
        }

        w.performance.last_24h_clear = utc_now(w);
    }
}

// ACE: ServerPerformanceMonitor.Reset
pub fn reset(w: &mut World) {
    if !w.performance.is_running {
        return;
    }

    let p = &mut w.performance;
    for i in 0..MonitorType::MAX_ITEMS {
        p.monitors_5m[i].clear_event_history();
        p.monitors_1h[i].clear_event_history();
        p.monitors_24h[i].clear_event_history();
    }

    for i in 0..CumulativeEventHistoryType::MAX_ITEMS {
        p.cumulative_5m[i].clear_history();
        p.cumulative_1h[i].clear_history();
        p.cumulative_24h[i].clear_history();
    }

    w.performance.last_5m_clear = utc_now(w);
    w.performance.last_1h_clear = utc_now(w);
    w.performance.last_24h_clear = utc_now(w);
}

// ACE: ServerPerformanceMonitor.RestartEvent
pub fn restart_event(w: &mut World, monitor_type: MonitorType) {
    if !w.performance.is_running {
        return;
    }

    let clock = clock(w);
    let i = monitor_type as usize;
    w.performance.monitors_24h[i].restart(&*clock);
    w.performance.monitors_1h[i].restart(&*clock);
    w.performance.monitors_5m[i].restart(&*clock);
}

// ACE: ServerPerformanceMonitor.RegisterEventEnd
pub fn register_event_end(w: &mut World, monitor_type: MonitorType) {
    if !w.performance.is_running {
        return;
    }

    let clock = clock(w);
    let i = monitor_type as usize;
    w.performance.monitors_5m[i].register_event_end(&*clock);
    w.performance.monitors_1h[i].register_event_end(&*clock);
    w.performance.monitors_24h[i].register_event_end(&*clock);
}

// ACE: ServerPerformanceMonitor.RestartCumulativeEvents
pub fn restart_cumulative_events(w: &mut World) {
    if !w.performance.is_running || !w.performance.is_running_cumulative {
        return;
    }

    w.performance.cumulative_seconds.fill(0.0);
}

// ACE: ServerPerformanceMonitor.AddToCumulativeEvent
pub fn add_to_cumulative_event(
    w: &mut World,
    event_history_type: CumulativeEventHistoryType,
    seconds: f64,
) {
    if !w.performance.is_running || !w.performance.is_running_cumulative {
        return;
    }

    // lock (cumulative5m[(int)eventHistoryType])
    w.performance.cumulative_seconds[event_history_type as usize] += seconds;
}

// ACE: ServerPerformanceMonitor.RegisterCumulativeEvents
pub fn register_cumulative_events(w: &mut World) {
    if !w.performance.is_running || !w.performance.is_running_cumulative {
        return;
    }

    let p = &mut w.performance;
    for i in 0..CumulativeEventHistoryType::MAX_ITEMS {
        p.cumulative_5m[i].register_event(p.cumulative_seconds[i]);
        p.cumulative_1h[i].register_event(p.cumulative_seconds[i]);
        p.cumulative_24h[i].register_event(p.cumulative_seconds[i]);
    }
}

// ACE: ServerPerformanceMonitor.GetEventHistory5m
#[must_use]
pub fn get_event_history_5m(w: &World, monitor_type: MonitorType) -> &TimedEventHistory {
    &w.performance.monitors_5m[monitor_type as usize].event_history
}

// ACE: ServerPerformanceMonitor.GetEventHistory1h
#[must_use]
pub fn get_event_history_1h(w: &World, monitor_type: MonitorType) -> &TimedEventHistory {
    &w.performance.monitors_1h[monitor_type as usize].event_history
}

// ACE: ServerPerformanceMonitor.GetEventHistory24h
#[must_use]
pub fn get_event_history_24h(w: &World, monitor_type: MonitorType) -> &TimedEventHistory {
    &w.performance.monitors_24h[monitor_type as usize].event_history
}

/// Not ACE: the monitor's clock (the installed clock, else the tick's frozen one), which the
/// landblocks' `RateMonitor`s read too.
#[must_use]
pub fn monitor_clock(w: &World) -> Arc<dyn Clock> {
    clock(w)
}

/// Not ACE: `new Stopwatch()` / `stopwatch.Restart()` on the monitor's clock (the installed clock,
/// else the tick's frozen one, on which every elapsed time is zero).
#[must_use]
pub fn stopwatch_start(w: &World) -> std::time::Duration {
    clock(w).monotonic()
}

/// Not ACE: `stopwatch.Elapsed.TotalSeconds` for a stopwatch started with [`stopwatch_start`].
#[must_use]
pub fn stopwatch_elapsed_seconds(w: &World, start: std::time::Duration) -> f64 {
    clock(w).monotonic().saturating_sub(start).as_secs_f64()
}

/// Not ACE: the 24-hour history of a cumulative event (`cumulative24h[(int)type]`, private in
/// ACE; `ToString` reads it), for the soak suite's hot-spot table.
#[must_use]
pub fn get_cumulative_event_history_24h(
    w: &World,
    event_history_type: CumulativeEventHistoryType,
) -> &TimedEventHistory {
    &w.performance.cumulative_24h[event_history_type as usize]
}

// ACE: ServerPerformanceMonitor.ToString
/// The `serverperformance` report.
#[must_use]
pub fn to_string(w: &World) -> String {
    let p = &w.performance;
    let mut sb = String::new();

    sb += &format!(
        "Monitoring Durations: ~5m {} min, ~1h {} min, ~24h {} min{}",
        format(monitors_5m_run_time(w).total_minutes(), "N2"),
        format(monitors_1h_run_time(w).total_minutes(), "N2"),
        format(monitors_24h_run_time(w).total_minutes(), "N2"),
        '\n'
    );
    sb += "~5m Hits   Avg  Long  Last Tot - ~1h Hits   Avg  Long  Last  Tot - ~24h Hits  Avg  Long  Last   Tot (s) - Name\n";

    let monitor = |sb: &mut String, t: MonitorType| {
        let i = t as usize;
        add_monitor_output_to_string_builder(
            &p.monitors_5m[i].event_history,
            &p.monitors_1h[i].event_history,
            &p.monitors_24h[i].event_history,
            t.name(),
            sb,
        );
    };
    let cumulative = |sb: &mut String, t: CumulativeEventHistoryType| {
        let i = t as usize;
        add_monitor_output_to_string_builder(
            &p.cumulative_5m[i],
            &p.cumulative_1h[i],
            &p.cumulative_24h[i],
            t.name(),
            sb,
        );
    };
    let monitors = |sb: &mut String, from: MonitorType, to: MonitorType| {
        for &t in &MonitorType::ALL[from as usize..=to as usize] {
            monitor(sb, t);
        }
    };
    let cumulatives =
        |sb: &mut String, from: CumulativeEventHistoryType, to: CumulativeEventHistoryType| {
            for &t in &CumulativeEventHistoryType::ALL[from as usize..=to as usize] {
                cumulative(sb, t);
            }
        };

    sb += "Calls from WorldManager.UpdateWorld()\n";
    monitors(
        &mut sb,
        MonitorType::PlayerManagerTick,
        MonitorType::NetworkManagerDoSessionWork,
    );

    sb += "WorldManager.UpdateGameWorld() time not including throttled returns\n";
    monitor(&mut sb, MonitorType::UpdateGameWorldEntire);

    sb += "Calls from WorldManager.UpdateGameWorld()\n";
    monitors(
        &mut sb,
        MonitorType::LandblockManagerTickPhysics,
        MonitorType::LandblockManagerTickSingleThreadedWork,
    );

    if p.is_running_cumulative {
        sb +=
            "Calls from Landblock.TickPhysics() - Cumulative over a single UpdateGameWorld Tick\n";
        cumulatives(
            &mut sb,
            CumulativeEventHistoryType::PlayerTickUpdateObjectPhysics,
            CumulativeEventHistoryType::WorldObjectTickUpdateObjectPhysics,
        );

        sb += "Calls from Landblock.TickMultiThreadedWork() - Cumulative over a single UpdateGameWorld Tick\n";
        cumulatives(
            &mut sb,
            CumulativeEventHistoryType::LandblockTickRunActions,
            CumulativeEventHistoryType::LandblockTickDatabaseSave,
        );

        sb += "Calls from Landblock.TickMultiThreadedWork() - Misc - Cumulative over a single UpdateGameWorld Tick\n";
        cumulatives(
            &mut sb,
            CumulativeEventHistoryType::MonsterAwarenessFindNextTarget,
            CumulativeEventHistoryType::LootGenerationFactoryCreateRandomLootObjects,
        );

        sb += "Calls from Landblock.TickSingleThreadedWork() - Cumulative over a single UpdateGameWorld Tick\n";
        cumulatives(
            &mut sb,
            CumulativeEventHistoryType::LandblockTickPlayerTick,
            CumulativeEventHistoryType::LandblockTickWorldObjectHeartbeat,
        );
    }

    sb += "Calls from NetworkManager.DoSessionWork()\n";
    monitors(
        &mut sb,
        MonitorType::DoSessionWorkTickOutbound,
        MonitorType::DoSessionWorkRemoveSessions,
    );

    //sb.Append($"Calls from NetworkManager.ProcessPacket(){'\n'}");
    //for (int i = (int)MonitorType.ProcessPacket_0; i <= (int)MonitorType.ProcessPacket_1; i++)
    //    AddMonitorOutputToStringBuilder(monitors5m[i].EventHistory, monitors1h[i].EventHistory, monitors24h[i].EventHistory, ((MonitorType)i).ToString(), sb);

    sb
}

/// `((int)history.TotalSeconds).ToString().PadLeft(width)`.
fn total_seconds_padded(history: &TimedEventHistory, width: i32) -> String {
    let seconds: i32 = history.total_seconds.cs_cast();
    align(&seconds.to_string(), width)
}

// ACE: ServerPerformanceMonitor.AddMonitorOutputToStringBuilder
fn add_monitor_output_to_string_builder(
    event_history_5m: &TimedEventHistory,
    event_history_1h: &TimedEventHistory,
    event_history_24h: &TimedEventHistory,
    name: &str,
    sb: &mut String,
) {
    let window = |h: &TimedEventHistory, tot_width: i32| {
        format!(
            "{} {} {} {} {}",
            align(&h.total_events.to_string(), 7),
            format(h.average_event_duration(), "N4"),
            format(h.longest_event, "N3"),
            format(h.last_event, "N3"),
            total_seconds_padded(h, tot_width)
        )
    };
    *sb += &format!(
        "{} - {} - {} - {name}\n",
        window(event_history_5m, 3),
        window(event_history_1h, 4),
        window(event_history_24h, 5)
    );
}
