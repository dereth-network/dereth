//! ACE: Source/ACE.Server/Managers/ServerPerformanceMonitor.cs::ServerPerformanceMonitor
//! Serverstatus and serverperformance print the ported ServerPerformanceMonitor figures timed on
//! the virtual clock.
//! Fixture: synthetic command arguments, handler tables and isolated server state.

use std::sync::Arc;
use std::time::Duration;

use empyrean_command::command_handler::CommandHandler;
use empyrean_command::command_manager;
use empyrean_command::handlers::admin_stat_commands as ast;
use empyrean_common::clock::VirtualClock;
use empyrean_common::dotnet::format;
use empyrean_testkit::TestServer;
use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
use empyrean_world::managers::server_performance_monitor::{self as perf, MonitorType};

/// An empty shard for `GuidManager.Initialize` (which `serverstatus`'s guid report reads).
struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }
    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

/// The console lines one console command wrote.
fn console(ts: &mut TestServer, handler: CommandHandler, parameters: &[&str]) -> Vec<String> {
    let parameters: Vec<String> = parameters.iter().map(|p| (*p).to_owned()).collect();
    command_manager::start_console_capture();
    handler(&mut ts.world, None, &parameters);
    command_manager::take_console_output()
}

#[test]
fn serverstatus_and_serverperformance_show_the_monitor() {
    let mut ts = TestServer::new();
    guid_manager::initialize(&mut ts.world, &mut EmptyShard);
    perf::use_clock(&mut ts.world, Arc::<VirtualClock>::clone(&ts.clock));
    let status = console(&mut ts, ast::handle_server_status, &[]);
    assert!(status[0].contains(
        "Server Performance Monitor - Not running. To start use /serverperformance start\n"
    ));

    assert_eq!(
        console(&mut ts, ast::handle_server_performance, &["start"]),
        ["Server Performance Monitor started"]
    );
    for _ in 0..30 {
        ts.step();
    }
    // One UpdateGameWorld that took 20 ms, as the loop would time it.
    let t = MonitorType::UpdateGameWorldEntire;
    perf::restart_event(&mut ts.world, t);
    ts.clock.advance(Duration::from_millis(20));
    perf::register_event_end(&mut ts.world, t);

    let h5 = perf::get_event_history_5m(&ts.world, t).clone();
    let h1 = perf::get_event_history_1h(&ts.world, t).clone();
    assert!(
        h5.total_events > 1 && h5.longest_event > 0.019,
        "the loop's UpdateGameWorld runs were counted: {h5:?}"
    );
    let status = console(&mut ts, ast::handle_server_status, &[]);
    let line = format!(
        "Server Performance Monitor - UpdateGameWorld ~5m {}, ~1h {} s\n",
        format(h5.average_event_duration(), "N3"),
        format(h1.average_event_duration(), "N3")
    );
    assert!(status[0].contains(&line), "{line:?} in {status:?}");
    assert_ne!(format(h5.average_event_duration(), "N3"), "0.000");

    let report = console(&mut ts, ast::handle_server_performance, &[]);
    assert_eq!(report, [perf::to_string(&ts.world)]);
    let ticks = perf::get_event_history_5m(&ts.world, MonitorType::PlayerManagerTick).total_events;
    assert!(
        ticks >= 29,
        "every world iteration after the start is a PlayerManager.Tick hit: {ticks}"
    );
    assert!(
        report[0].contains(&format!("{ticks:>7} ")) && report[0].contains("PlayerManager_Tick\n"),
        "{report:?}"
    );
}
