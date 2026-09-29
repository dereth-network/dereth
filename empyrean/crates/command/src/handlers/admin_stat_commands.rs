// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/AdminStatCommands.cs
//! Port of `Source/ACE.Server/Command/Handlers/AdminStatCommands.cs`.
//!
//! The reports read the world's managers directly. DIVERGE (forced): the lines that report the
//! .NET host (`Environment`, `ThreadPool`, `Process`, `GC`) have no Rust runtime to read; they keep
//! ACE's text with the values [`HostInfo`] can give (the OS and processor count) and zeros for the
//! rest. Callees that are not ported yet are pointers at the bottom of the file.

use empyrean_common::dotnet::{format, format_aligned};
use empyrean_entity::enums::{ChatMessageType, PhysicsState};
use empyrean_entity::{LandblockId, ObjectGuid};
use empyrean_net::SessionId;
use empyrean_world::managers::{
    guid_manager, landblock_manager, player_manager, server_performance_monitor,
};
use empyrean_world::physics::phys_ext;
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::{CommandHandlerInfo, NamedHandler};
use crate::handler;
use crate::handlers::command_handler_helper;

/// This file's `[CommandHandler]` decorations, in declaration order (`HandleLandblockStats` has
/// two).
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let adv = empyrean_entity::enums::AccessLevel::Advocate;
    let none = CommandHandlerFlag::None;
    let lb = "Displays a summary of landblock performance statistics";
    let rows: [(CommandHandlerAttribute, NamedHandler); 7] = [
        (
            CommandHandlerAttribute::with_count(
                "allstats",
                adv,
                none,
                0,
                "Displays a summary of all server statistics and usage",
                "",
            ),
            handler!(handle_all_stats),
        ),
        (
            CommandHandlerAttribute::with_count(
                "serverstatus",
                adv,
                none,
                0,
                "Displays a summary of server statistics and usage",
                "",
            ),
            handler!(handle_server_status),
        ),
        (
            CommandHandlerAttribute::with_count(
                "serverperformance",
                adv,
                none,
                0,
                "Displays a summary of server performance statistics",
                "",
            ),
            handler!(handle_server_performance),
        ),
        (
            CommandHandlerAttribute::with_count("landblockperformance", adv, none, 0, lb, ""),
            handler!(handle_landblock_stats),
        ),
        (
            CommandHandlerAttribute::with_count("landblockstats", adv, none, 0, lb, ""),
            handler!(handle_landblock_stats),
        ),
        (
            CommandHandlerAttribute::with_count(
                "lbgroupstats",
                adv,
                none,
                0,
                "Displays a summary of landblock group stats",
                "",
            ),
            handler!(handle_lb_group_stats),
        ),
        (
            CommandHandlerAttribute::with_count(
                "gcstatus",
                adv,
                none,
                0,
                "Displays a summary of server GC Information",
                "",
            ),
            handler!(handle_gc_status),
        ),
    ];
    rows.into_iter()
        .map(|(attribute, (handler, handler_name))| CommandHandlerInfo {
            handler,
            handler_name,
            attribute,
        })
        .collect()
}

/// `CommandHandlerHelper.WriteOutputInfo(session, output)` (the default `ChatMessageType.Broadcast`).
fn write_output_info(w: &mut World, session: Option<SessionId>, output: &str) {
    command_handler_helper::write_output_info(w, session, output, ChatMessageType::Broadcast);
}

// ACE: AdminStatCommands.HandleAllStats
/// `allstats`: displays a summary of all server statistics and usage.
pub fn handle_all_stats(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    handle_server_status(w, session, parameters);

    handle_server_performance(w, session, parameters);

    handle_landblock_stats(w, session, parameters);

    handle_lb_group_stats(w, session, parameters);

    handle_gc_status(w, session, parameters);

    developer_database_commands_handle_database_queue_info(w, session, parameters);
}

/// What `HandleServerStatus` reads from the .NET host (see the module docs).
///
/// DIVERGE (forced): `Environment.OSVersion` is the Rust target OS name; the thread pool, process
/// times and private memory have no Rust counterpart and read 0.
#[derive(Debug, Clone, Default)]
pub struct HostInfo {
    pub os_version: String,
    pub processor_count: usize,
}

impl HostInfo {
    fn current() -> Self {
        Self {
            os_version: std::env::consts::OS.to_owned(),
            processor_count: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
        }
    }
}

/// `worldObject is Player`, `is Creature`, `Missile ?? false`: the three counters of the landblock
/// loops, plus the rest.
#[derive(Debug, Clone, Copy, Default)]
struct ObjectCounts {
    players: i32,
    creatures: i32,
    missiles: i32,
    other: i32,
    total: i32,
}

fn count_objects(
    w: &World,
    objects: &[ObjectGuid],
    counts: &mut ObjectCounts,
    count_missiles: bool,
) {
    for &guid in objects {
        let Some(world_object) = w.objects.get(guid) else {
            continue;
        };
        if world_object.player.is_some() {
            counts.players += 1;
        } else if world_object.creature.is_some() {
            counts.creatures += 1;
        } else if count_missiles && phys_ext::get_physics_state(w, guid, PhysicsState::Missile) {
            counts.missiles += 1;
        } else {
            counts.other += 1;
        }

        counts.total += 1;
    }
}

// ACE: AdminStatCommands.HandleServerStatus
/// `serverstatus`: displays a summary of server statistics and usage.
pub fn handle_server_status(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    // This is formatted very similarly to GDL.

    let mut sb = String::new();

    let host = HostInfo::current();

    sb += "Server Status:\n";

    sb += &format!(
        "Host Info: {}, vCPU: {}\n",
        host.os_version, host.processor_count
    );

    // DIVERGE (forced): no .NET ThreadPool; min/max/available/current read 0.
    sb += "ThreadPool Min: 0 0, Max: 0 0, Avail: 0 0, Current: 0\n";

    // DIVERGE (forced): the process start time and CPU time are not read; they show 0.
    sb += "Server Runtime: 0h 0m 0s\n";

    sb += "Total CPU Time: 0h 0m 0s, Threads: 0\n";

    // todo, add actual system memory used/avail
    sb += &format!("{} MB used\n", format(0i64 >> 20, "N0")); // sb.Append($"{(proc.PrivateMemorySize64 >> 20)} MB used, xxxx / yyyy MB physical mem free.{'\n'}");

    sb += &format!(
        "{} connections, {} authenticated connections, {} unique connections, {} players online\n",
        format(w.net.get_session_count(), "N0"),
        format(w.net.get_authenticated_session_count(), "N0"),
        format(w.net.get_unique_session_endpoint_count(), "N0"),
        format(player_manager::get_online_count(w), "N0")
    );
    let account_count = w.auth.lock().get_account_count();
    let characters = i32::try_from(player_manager::get_offline_count(w)).unwrap_or(i32::MAX)
        + player_manager::get_online_count(w);
    sb += &format!(
        "Total Accounts Created: {}, Total Characters Created: {}\n",
        format(account_count, "N0"),
        format(characters, "N0")
    );

    // 330 active objects, 1931 total objects(16777216 buckets.)

    // todo, expand this
    let loaded_landblocks = landblock_manager::get_loaded_landblocks(w);
    let (mut dormant_landblocks, mut active_dungeon_landblocks, mut dormant_dungeon_landblocks) =
        (0i32, 0i32, 0i32);
    let mut counts = ObjectCounts::default();
    for &landblock in &loaded_landblocks {
        let Some(lb) = w.landblock_manager.landblocks.get_mut(landblock) else {
            continue;
        };
        let is_dormant = lb.is_dormant;
        if is_dormant {
            dormant_landblocks += 1;
        }

        if lb.is_dungeon() {
            if is_dormant {
                dormant_dungeon_landblocks += 1;
            } else {
                active_dungeon_landblocks += 1;
            }
        }

        let objects = lb.get_all_world_objects_for_diagnostics();
        count_objects(w, &objects, &mut counts, true);
    }
    let active = i32::try_from(loaded_landblocks.len()).unwrap_or(i32::MAX) - dormant_landblocks;
    sb += &format!(
        "Landblocks: {} active ({} dungeons), {} dormant ({} dungeons), Landblock Groups: {} - Players: {}, Creatures: {}, Missiles: {}, Other: {}, Total: {}.\n",
        format(active, "N0"),
        format(active_dungeon_landblocks, "N0"),
        format(dormant_landblocks, "N0"),
        format(dormant_dungeon_landblocks, "N0"),
        format(landblock_manager::landblock_groups_count(w), "N0"),
        format(counts.players, "N0"),
        format(counts.creatures, "N0"),
        format(counts.missiles, "N0"),
        format(counts.other, "N0"),
        format(counts.total, "N0")
    ); // 11 total blocks loaded. 11 active. 0 pending dormancy. 0 dormant. 314 unloaded.
       // 11 total blocks loaded. 11 active. 0 pending dormancy. 0 dormant. 314 unloaded.

    if w.performance.is_running {
        let (avg_5m, avg_1h) = server_performance_monitor_update_game_world_entire_averages(w);
        sb += &format!(
            "Server Performance Monitor - UpdateGameWorld ~5m {}, ~1h {} s\n",
            format(avg_5m, "N3"),
            format(avg_1h, "N3")
        );
    } else {
        sb += "Server Performance Monitor - Not running. To start use /serverperformance start\n";
    }

    // DIVERGE: ACE reports the thread counts its Server.Threading settings size. Empyrean has no
    // such settings: the world ticks on one thread and the database work runs on one more, which
    // is what the line reports.
    sb += &format!(
        "Threading - WorldThreadCount: {}, Multithread Physics: {}, Multithread Non-Physics: {}, DatabaseThreadCount: {}\n",
        1,
        bool_string(false),
        bool_string(false),
        1
    );

    let (bsp, gfx, polygon, vertex) = physics_cache_counts();
    sb += &format!(
        "Physics Cache Counts - BSPCache: {}, GfxObjCache: {}, PolygonCache: {}, VertexCache: {}\n",
        format(bsp, "N0"),
        format(gfx, "N0"),
        format(polygon, "N0"),
        format(vertex, "N0")
    );

    sb += &format!(
        "Total Server Objects: {}\n",
        format(w.server_object_manager.server_objects.len(), "N0")
    );

    let content = w.content.clone();
    sb += &format!(
        "World DB Cache Counts - Weenies: {}, LandblockInstances: {}, PointsOfInterest: {}, Cookbooks: {}, Spells: {}, Encounters: {}, Events: {}\n",
        format(content.get_weenie_cache_count(), "N0"),
        format(content.get_landblock_instances_cache_count(), "N0"),
        format(content.get_points_of_interest_cache_count(), "N0"),
        format(content.get_cookbook_cache_count(), "N0"),
        format(content.get_spell_cache_count(), "N0"),
        format(content.get_encounter_cache_count(), "N0"),
        format(content.get_events_cache_count(), "N0")
    );
    //sb.Append($"Shard DB Counts - Biotas: {DatabaseManager.Shard.BaseDatabase.GetBiotaCount():N0}{'\n'}");
    // (ACE passes its MySQL shard schema's name; the SQLite shard has none and counts its rows.)
    let estimated = w.shard.base_database().get_estimated_biota_count("");
    sb += &format!("Shard DB Counts - Biotas: ~{}\n", format(estimated, "N0"));
    // DIVERGE (forced): `BaseDatabase is ShardDatabaseWithCaching` is a type test the store's
    // trait object cannot answer, so the "Shard DB Cache Counts" line is not written.

    sb += &guid_manager::get_dynamic_guid_debug_info(w);
    sb.push('\n');

    let portal = w.dats.portal_dat();
    let cell = w.dats.cell_dat();
    sb += &format!(
        "Portal.dat has {} files cached of {} total\n",
        format(portal.file_cache_count(), "N0"),
        format(portal.all_files_count(), "N0")
    );
    sb += &format!(
        "Cell.dat has {} files cached of {} total\n",
        format(cell.file_cache_count(), "N0"),
        format(cell.all_files_count(), "N0")
    );

    write_output_info(w, session, &sb);
}

/// `bool.ToString()`.
fn bool_string(b: bool) -> &'static str {
    if b {
        "True"
    } else {
        "False"
    }
}

// ACE: AdminStatCommands.HandleServerPerformance
/// `serverperformance [start | stop | reset] [cumulative]`: displays a summary of server
/// performance statistics.
pub fn handle_server_performance(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    if parameters.len() == 1 || parameters.len() == 2 {
        if !parameters.is_empty() && parameters[0].to_lowercase() == "start" {
            if parameters.len() >= 2 && parameters[1].to_lowercase() == "cumulative" {
                server_performance_monitor::start_cumulative(w);
                write_output_info(w, session, "Cumulative Server Performance Monitor started");
                return;
            }
            server_performance_monitor::start(w);
            write_output_info(w, session, "Server Performance Monitor started");
            return;
        }

        if !parameters.is_empty() && parameters[0].to_lowercase() == "stop" {
            if parameters.len() >= 2 && parameters[1].to_lowercase() == "cumulative" {
                server_performance_monitor::stop_cumulative(w);
                write_output_info(w, session, "Cumulative Server Performance Monitor stopped");
                return;
            }
            server_performance_monitor::stop(w);
            write_output_info(w, session, "Server Performance Monitor stopped");
            return;
        }

        if !parameters.is_empty() && parameters[0].to_lowercase() == "reset" {
            server_performance_monitor::reset(w);
            write_output_info(w, session, "Server Performance Monitor reset");
            return;
        }
    }

    if !w.performance.is_running {
        write_output_info(
            w,
            session,
            "Server Performance Monitor not running. To start use /serverperformance start",
        );
        return;
    }

    let text = server_performance_monitor_to_string(w);
    write_output_info(w, session, &text);
}

/// One landblock's line in the landblock stats tables.
fn landblock_stats_line(w: &World, id: LandblockId) -> String {
    let Some(entry) = w.landblock_manager.landblocks.get(id) else {
        return String::new();
    };
    let mut counts = ObjectCounts::default();
    count_objects(
        w,
        &entry.get_all_world_objects_for_diagnostics(),
        &mut counts,
        false,
    );
    let h5 = &entry.monitor_5m.event_history;
    let h1 = &entry.monitor_1h.event_history;
    format!(
        "{} {} {} {} - {} {} {} {} - 0x{} {}  {}\n",
        pad_left(&h5.total_events.to_string(), 7),
        format(h5.average_event_duration(), "N4"),
        format(h5.longest_event, "N3"),
        format(h5.last_event, "N3"),
        pad_left(&h1.total_events.to_string(), 7),
        format(h1.average_event_duration(), "N4"),
        format(h1.longest_event, "N3"),
        format(h1.last_event, "N3"),
        format(entry.id.raw(), "X8"),
        pad_left(&counts.players.to_string(), 7),
        pad_left(&counts.creatures.to_string(), 9)
    )
}

/// `s.PadLeft(width)`.
fn pad_left(s: &str, width: usize) -> String {
    format!("{s:>width$}")
}

/// `list.OrderByDescending(key)`: a stable sort, greatest first (`double` comparison, NaN least).
fn order_by_descending(
    mut list: Vec<LandblockId>,
    key: impl Fn(LandblockId) -> f64,
) -> Vec<LandblockId> {
    list.sort_by(|a, b| total_cmp_dotnet(key(*b), key(*a)));
    list
}

/// `double.CompareTo`: NaN sorts below every number.
fn total_cmp_dotnet(a: f64, b: f64) -> std::cmp::Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (false, false) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
    }
}

/// `a.Concat(b).Distinct()`.
fn concat_distinct(a: Vec<LandblockId>, b: Vec<LandblockId>) -> Vec<LandblockId> {
    let mut out: Vec<LandblockId> = Vec::new();
    for id in a.into_iter().chain(b) {
        if !out.iter().any(|o| o.raw() == id.raw()) {
            out.push(id);
        }
    }
    out
}

// ACE: AdminStatCommands.HandleLandblockStats
/// `landblockperformance` / `landblockstats`: displays a summary of landblock performance
/// statistics.
pub fn handle_landblock_stats(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let mut sb = String::new();

    let loaded_landblocks = landblock_manager::get_loaded_landblocks(w);
    let lb = |id: LandblockId| w.landblock_manager.landblocks.get(id);
    let h5_avg = |id: LandblockId| {
        lb(id).map_or(0.0, |l| l.monitor_5m.event_history.average_event_duration())
    };
    let h1_avg = |id: LandblockId| {
        lb(id).map_or(0.0, |l| l.monitor_1h.event_history.average_event_duration())
    };
    let h5_long =
        |id: LandblockId| lb(id).map_or(0.0, |l| l.monitor_5m.event_history.longest_event);
    let h1_long =
        |id: LandblockId| lb(id).map_or(0.0, |l| l.monitor_1h.event_history.longest_event);

    // Filter out landblocks that haven't recorded a certain amount of events
    let sorted_by_5m_average: Vec<LandblockId> = order_by_descending(
        loaded_landblocks
            .iter()
            .copied()
            .filter(|&r| lb(r).is_some_and(|l| l.monitor_5m.event_history.total_events >= 10))
            .collect(),
        h5_avg,
    )
    .into_iter()
    .take(10)
    .collect();
    let sorted_by_1hr_average: Vec<LandblockId> = order_by_descending(
        loaded_landblocks
            .iter()
            .copied()
            .filter(|&r| lb(r).is_some_and(|l| l.monitor_1h.event_history.total_events >= 1000))
            .collect(),
        h1_avg,
    )
    .into_iter()
    .take(10)
    .collect();

    let combined_by_average: Vec<LandblockId> = order_by_descending(
        concat_distinct(sorted_by_5m_average, sorted_by_1hr_average),
        |r| dotnet_max(h5_avg(r), h1_avg(r)),
    )
    .into_iter()
    .take(10)
    .collect();

    sb += "Most Busy Landblock - By Average\n";
    sb +=
        "~5m Hits   Avg  Long  Last - ~1h Hits   Avg  Long  Last - Location   Players  Creatures\n";

    for entry in combined_by_average {
        sb += &landblock_stats_line(w, entry);
    }

    let sorted_by_5m_long: Vec<LandblockId> =
        order_by_descending(loaded_landblocks.clone(), h5_long)
            .into_iter()
            .take(10)
            .collect();
    let sorted_by_1hr_long: Vec<LandblockId> = order_by_descending(loaded_landblocks, h1_long)
        .into_iter()
        .take(10)
        .collect();

    let combined_by_long: Vec<LandblockId> = order_by_descending(
        concat_distinct(sorted_by_5m_long, sorted_by_1hr_long),
        |r| dotnet_max(h5_long(r), h1_long(r)),
    )
    .into_iter()
    .take(10)
    .collect();

    sb += "Most Busy Landblock - By Longest\n";
    sb +=
        "~5m Hits   Avg  Long  Last - ~1h Hits   Avg  Long  Last - Location   Players  Creatures\n";

    for entry in combined_by_long {
        sb += &landblock_stats_line(w, entry);
    }

    write_output_info(w, session, &sb);
}

/// `Math.Max(double, double)` (NaN wins).
fn dotnet_max(a: f64, b: f64) -> f64 {
    empyrean_common::dotnet::math::max(a, b)
}

/// One landblock group's line in the group stats tables.
fn landblock_group_line(g: &empyrean_world::entity::landblock_group::LandblockGroup) -> String {
    format!(
        "{},   {} - {},     {} - {}  ,                  {} {},                            {} {}\n",
        format_aligned(g.count(), 3, ""),
        format_aligned(g.x_min(), 2, "X2"),
        format_aligned(g.x_max(), 2, "X2"),
        format_aligned(g.y_min(), 2, "X2"),
        format_aligned(g.y_max(), 2, "X2"),
        format_aligned(g.tick_physics_tracker.average_amount(), 5, "N3"),
        format_aligned(g.tick_physics_tracker.largest_amount(), 5, "N3"),
        format_aligned(g.tick_multi_threaded_work_tracker.average_amount(), 5, "N3"),
        format_aligned(g.tick_multi_threaded_work_tracker.largest_amount(), 5, "N3")
    )
}

/// `groups.OrderByDescending(key).Take(5)`, as indices into the group list.
fn top_groups(
    groups: &[empyrean_world::entity::landblock_group::LandblockGroup],
    key: impl Fn(&empyrean_world::entity::landblock_group::LandblockGroup) -> f64,
) -> Vec<usize> {
    let mut order: Vec<usize> = (0..groups.len()).collect();
    order.sort_by(|&a, &b| total_cmp_dotnet(key(&groups[b]), key(&groups[a])));
    order.truncate(5);
    order
}

// ACE: AdminStatCommands.HandleLBGroupStats
/// `lbgroupstats`: displays a summary of landblock group stats.
pub fn handle_lb_group_stats(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let mut sb = String::new();

    let (physics_efficiency, work_efficiency) = landblock_manager_efficiency_trackers(w);
    sb += &format!(
        "TickPhysicsEfficiencyTracker: {} %, TickMultiThreadedWorkEfficiencyTracker: {} %\n",
        format_aligned(physics_efficiency, 3, "N0"),
        format_aligned(work_efficiency, 3, "N0")
    );

    let loaded_lanblock_groups = w.landblock_manager.landblock_groups();

    #[allow(clippy::cast_precision_loss)]
    let sorted_by_largest = top_groups(loaded_lanblock_groups, |r| r.count() as f64);

    sb += "Largest Landblock Groups\n";
    sb += "Cnt, XMin - XMax, YMin - YMax, TickPhysicsTracker avg   max, TickMultiThreadedWorkTracker avg   max (s)\n";

    for i in sorted_by_largest {
        sb += &landblock_group_line(&loaded_lanblock_groups[i]);
    }

    let sorted_by_top_tick_physics_tracker = top_groups(loaded_lanblock_groups, |r| {
        r.tick_physics_tracker.average_amount()
    });

    sb += "Top TickPhysicsTracker Landblock Groups\n";

    for i in sorted_by_top_tick_physics_tracker {
        sb += &landblock_group_line(&loaded_lanblock_groups[i]);
    }

    let sorted_by_top_tick_multi_threaded_work_tracker = top_groups(loaded_lanblock_groups, |r| {
        r.tick_multi_threaded_work_tracker.average_amount()
    });

    sb += "Top TickMultiThreadedWorkTracker Landblock Groups\n";

    for i in sorted_by_top_tick_multi_threaded_work_tracker {
        sb += &landblock_group_line(&loaded_lanblock_groups[i]);
    }

    write_output_info(w, session, &sb);
}

// ACE: AdminStatCommands.HandleGCStatus
/// `gcstatus`: displays a summary of server GC Information.
///
/// DIVERGE (forced): there is no managed heap; every figure reads 0 and the generation and pause
/// lists are empty.
pub fn handle_gc_status(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let mut sb = String::new();

    sb += "GC.GetTotalMemory: 0 MB, GC.GetTotalAllocatedBytes: 0 MB\n";

    // https://docs.microsoft.com/en-us/dotnet/api/system.gcmemoryinfo?view=net-5.0
    sb += "GCMI Index: 0, Generation: 0, Compacted: False, Concurrent: False, PauseTimePercentage: 0\n";
    sb += "GCMI PinnedObjectsCount: 0, FinalizationPendingCount: 0\n";

    sb += "GCMI FragmentedBytes: 0 MB, PromotedBytes: 0 MB, HeapSizeBytes: 0 MB, TotalCommittedBytes: 0 MB\n";
    sb += "GCMI MemoryLoadBytes: 0 MB, HighMemoryLoadThresholdBytes: 0 MB, TotalAvailableMemoryBytes: 0 MB\n";

    write_output_info(w, session, &sb);
}

// ---------------------------------------------------------------------------------------------
// Pointers to members that are not ported yet (their owners swap these for the real calls).
// ---------------------------------------------------------------------------------------------

/// `DeveloperDatabaseCommands.HandleDatabaseQueueInfo(session, parameters)`.
fn developer_database_commands_handle_database_queue_info(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    crate::handlers::developer_database_commands::handle_database_queue_info(
        w, session, parameters,
    );
}

/// `ServerPerformanceMonitor.GetEventHistory5m/1h(MonitorType.UpdateGameWorld_Entire).AverageEventDuration`.
fn server_performance_monitor_update_game_world_entire_averages(w: &World) -> (f64, f64) {
    let t = server_performance_monitor::MonitorType::UpdateGameWorldEntire;
    (
        server_performance_monitor::get_event_history_5m(w, t).average_event_duration(),
        server_performance_monitor::get_event_history_1h(w, t).average_event_duration(),
    )
}

/// `ServerPerformanceMonitor.ToString()`.
fn server_performance_monitor_to_string(w: &World) -> String {
    server_performance_monitor::to_string(w)
}

/// `BSPCache.Count`, `GfxObjCache.Count`, `PolygonCache.Count`, `VertexCache.Count`: 0 each.
/// Not ported: those are ACE's interning caches of its own physics port; the shared physics
/// converts each setup's collision geometry once and shares it, and has no such caches to count.
// ACE: BSPCache.Count
fn physics_cache_counts() -> (i32, i32, i32, i32) {
    (0, 0, 0, 0)
}

/// `LandblockManager.TickPhysicsEfficiencyTracker.AverageAmount` and
/// `TickMultiThreadedWorkEfficiencyTracker.AverageAmount`.
fn landblock_manager_efficiency_trackers(w: &World) -> (f64, f64) {
    (
        w.landblock_manager
            .tick_physics_efficiency_tracker
            .0
            .average_amount(),
        w.landblock_manager
            .tick_multi_threaded_work_efficiency_tracker
            .0
            .average_amount(),
    )
}
