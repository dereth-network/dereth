// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Managers/ServerManager.cs
//! Port of `Source/ACE.Server/Managers/ServerManager.cs`.
//!
//! ServerManager handles unloading the server application properly.
//!
//! # The shutdown thread
//!
//! ACE's `ShutdownServer` runs on its own thread (or on the console thread for `DoShutdownNow`)
//! and waits, polling every 10 ms, for each step to finish: the countdown, the players logging
//! off, the sessions dropping, the landblocks unloading, the world loop stopping, the database
//! queue emptying. Those waits read world state that belongs to the world thread.
//!
//! DIVERGE: here the thread is a state machine, [`shutdown_server`], that the world thread
//! polls between loop iterations (the server's `WorldHost`, or the test harness) and once more
//! after the loop has stopped. Each call runs the sequence from where it stopped until the next
//! wait that is not over. Its `DateTime.UtcNow` reads are the world's tick clock, `w.now`.

use empyrean_common::dotnet::datetime::{DotNetDateTime, TimeSpan};
use empyrean_common::extensions::date_time_extensions::to_common_string;
use empyrean_common::master_configuration::MasterConfiguration;

use empyrean_entity::enums::{ChatMessageType, PropertyString};

use crate::entity::actions::i_action::Action;
use crate::managers::{player_manager, property_manager, world_manager};
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::sessions;
use crate::World;

/// Where the shutdown thread is: which of `ShutdownServer`'s waits it is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownStage {
    /// wait for shutdown interval to expire
    Countdown,
    /// Wait for all players to log out
    LoggingOffPlayers,
    /// Wait for all sessions to drop out
    DisconnectingSessions,
    /// Wait for all landblocks to unload
    UnloadingLandblocks,
    /// Wait for world to end
    StoppingWorld,
    /// Wait for the database queue to empty
    WaitingForDatabase,
    /// `Environment.Exit`: the process may end.
    Exited,
}

/// The shutdown thread's locals.
#[derive(Debug, Clone, Copy)]
struct ShutdownThread {
    stage: ShutdownStage,
    shutdown_time: DotNetDateTime,
    last_notice_time: DotNetDateTime,
    log_update_ts: DotNetDateTime,
    player_logoff_start: DotNetDateTime,
}

// ACE: ServerManager
/// The mutable static state of ACE's `ServerManager`, held as a field of `World`.
#[derive(Debug, Default)]
pub struct ServerManagerState {
    // ACE: ServerManager.ShutdownInitiated
    /// Indicates advanced warning if the applcation will unload.
    pub shutdown_initiated: bool,
    // ACE: ServerManager.ShutdownInProgress
    /// Indicates server shutting down.
    pub shutdown_in_progress: bool,
    // ACE: ServerManager.ShutdownInterval
    /// The amount of seconds that the server will wait before unloading the application.
    pub shutdown_interval: u32,
    // ACE: ServerManager.ShutdownTime
    pub shutdown_time: DotNetDateTime,
    /// Not ACE: the shutdown thread, while it runs (see the module docs).
    shutdown_thread: Option<ShutdownThread>,
}

impl ServerManagerState {
    /// Which wait the shutdown thread is in; `None` when no shutdown is running.
    pub fn shutdown_stage(&self) -> Option<ShutdownStage> {
        self.shutdown_thread.map(|t| t.stage)
    }
}

/// Not ACE: the transport keeps its own copy of the two values its login check reads
/// (`ShutdownInProgress`, and `ShutdownTime` while `ShutdownInitiated`); this refreshes it.
fn sync_net(w: &mut World) {
    let sm = &w.server_manager;
    w.net.shutdown_in_progress = sm.shutdown_in_progress;
    w.net.shutdown_time = sm.shutdown_initiated.then_some(sm.shutdown_time);
}

// ACE: ServerManager.SetShutdownInterval
/// Sets the Shutdown Interval in Seconds
pub fn set_shutdown_interval(w: &mut World, interval: u32) {
    log::info!("Server shutdown interval reset: {interval}");
    w.server_manager.shutdown_interval = interval;
}

// ACE: ServerManager.Initialize
/// Loads the configuration for ShutdownInterval from the settings file.
pub fn initialize(w: &mut World, config: &MasterConfiguration) {
    w.server_manager.shutdown_interval = config.server.shutdown_interval;
}

// ACE: ServerManager.BeginShutdown
/// Starts the shutdown wait thread.
pub fn begin_shutdown(w: &mut World) {
    w.server_manager.shutdown_initiated = true;

    start_shutdown_thread(w);
}

// ACE: ServerManager.CancelShutdown
/// Calling this function will always cancel an in-progress shutdown (application unload). This
/// will also stop the shutdown wait thread and alert users that the server will stay in operation.
pub fn cancel_shutdown(w: &mut World) {
    w.server_manager.shutdown_initiated = false;
    w.server_manager.shutdown_time = DotNetDateTime::MIN_VALUE;
    sync_net(w);
}

// ACE: ServerManager.DoShutdownNow
pub fn do_shutdown_now(w: &mut World) {
    set_shutdown_interval(w, 0);
    w.server_manager.shutdown_initiated = true;
    player_manager::broadcast_to_all(
        w,
        &game_message_system_chat(
            "Broadcast from System> ATTENTION - This Asheron's Call Server is shutting down NOW!!!!",
            ChatMessageType::WorldBroadcast,
        ),
    );
    start_shutdown_thread(w);
}

/// The head of `ShutdownServer`, run when its thread starts.
fn start_shutdown_thread(w: &mut World) {
    let utc = w.now.utc;
    let shutdown_time = utc.add_seconds(f64::from(w.server_manager.shutdown_interval));

    w.server_manager.shutdown_time = shutdown_time;

    let last_notice_time = utc;

    w.server_manager.shutdown_thread = Some(ShutdownThread {
        stage: ShutdownStage::Countdown,
        shutdown_time,
        last_notice_time,
        log_update_ts: DotNetDateTime::MIN_VALUE,
        player_logoff_start: DotNetDateTime::MIN_VALUE,
    });
    sync_net(w);
}

// ACE: ServerManager.ShutdownServer
/// Threaded task created when performing a server shutdown. Runs the sequence from where it
/// stopped until a wait that is not over (see the module docs); returns true once it reaches
/// ACE's `Environment.Exit`. Without a running shutdown it does nothing and returns false.
pub fn shutdown_server(w: &mut World) -> bool {
    let Some(mut t) = w.server_manager.shutdown_thread.take() else {
        return false;
    };
    let exited = shutdown_server_steps(w, &mut t);
    // A cancelled shutdown ends its thread.
    if w.server_manager.shutdown_initiated || t.stage != ShutdownStage::Countdown {
        w.server_manager.shutdown_thread = Some(t);
    }
    exited
}

fn shutdown_server_steps(w: &mut World, t: &mut ShutdownThread) -> bool {
    loop {
        let utc = w.now.utc;
        match t.stage {
            ShutdownStage::Countdown => {
                // wait for shutdown interval to expire
                if t.shutdown_time != DotNetDateTime::MIN_VALUE && t.shutdown_time >= utc {
                    // this allows the server shutdown to be canceled
                    if !w.server_manager.shutdown_initiated {
                        // reset shutdown details
                        // DIVERGE: ACE shows DateTime.Now (local time) first; the world has only the UTC clock.
                        let shutdown_text = format!(
                            "The server shut down has been cancelled @ {} ({} UTC)",
                            to_common_string(utc),
                            to_common_string(utc)
                        );
                        log::info!("{shutdown_text}");

                        // special text
                        for player in player_manager::get_all_online(w) {
                            if let Some(session) = player_manager::player_session(w, player) {
                                sessions::world_broadcast(
                                    w,
                                    session,
                                    "Broadcast from System> ATTENTION - This Asheron's Call Server shut down has been cancelled.",
                                );
                            }
                        }

                        // break function
                        return false;
                    }

                    t.last_notice_time = notify_players_of_pending_shutdown(
                        w,
                        t.last_notice_time,
                        t.shutdown_time.add_seconds(1.0),
                    );

                    return false; // Thread.Sleep(10)
                }

                w.server_manager.shutdown_in_progress = true;
                sync_net(w);

                property_manager::resync_variables(w);
                property_manager::stop_updating(w);

                world_manager::enqueue_action(
                    w,
                    Action::delegate(|w: &mut World| {
                        log::debug!("Logging off all players...");

                        // logout each player
                        for player in player_manager::get_all_online(w) {
                            if let Some(session) = player_manager::player_session(w, player) {
                                sessions::log_off_player(w, session, true);
                            }
                        }
                    }),
                );

                // Wait for all players to log out
                t.log_update_ts = DotNetDateTime::MIN_VALUE;
                t.player_logoff_start = utc;
                t.stage = ShutdownStage::LoggingOffPlayers;
            }
            ShutdownStage::LoggingOffPlayers => {
                let player_count = player_manager::get_online_count(w);
                if player_count > 0 {
                    t.log_update_ts = log_status_update(
                        w,
                        t.log_update_ts,
                        &format!(
                            "Waiting for {player_count} player{} to log off...",
                            if player_count > 1 { "s" } else { "" }
                        ),
                    );
                    if utc - t.player_logoff_start > TimeSpan::from_minutes(5.0) {
                        t.player_logoff_start = utc;
                        log::warn!(
                            "5 minute log off failsafe reached and there are {player_count} player{} still online.",
                            if player_count > 1 { "s" } else { "" }
                        );
                        for player in player_manager::get_all_online(w) {
                            let name = w
                                .objects
                                .get(player)
                                .and_then(|o| o.get_property(PropertyString::Name))
                                .unwrap_or_default();
                            log::warn!(
                                "Player {name} (0x{:08X}) appears to be stuck in world and unable to log off normally. Requesting Forced Logoff...",
                                player.full()
                            );
                            if let Some(p) =
                                w.objects.get_mut(player).and_then(|o| o.player.as_mut())
                            {
                                p.player.forced_log_off_requested = true;
                            }
                            crate::world_objects::player::force_logoff(w, player);
                        }
                    }
                    return false; // Thread.Sleep(10)
                }

                world_manager::enqueue_action(
                    w,
                    Action::delegate(|w: &mut World| {
                        log::debug!("Disconnecting all sessions...");

                        // disconnect each session
                        let now = w.now;
                        w.net.disconnect_all_sessions_for_shutdown(now);
                    }),
                );

                // Wait for all sessions to drop out
                t.log_update_ts = DotNetDateTime::MIN_VALUE;
                t.stage = ShutdownStage::DisconnectingSessions;
            }
            ShutdownStage::DisconnectingSessions => {
                let session_count = w.net.get_authenticated_session_count();
                if session_count > 0 {
                    t.log_update_ts = log_status_update(
                        w,
                        t.log_update_ts,
                        &format!(
                            "Waiting for {session_count} authenticated session{} to disconnect...",
                            if session_count > 1 { "s" } else { "" }
                        ),
                    );
                    return false; // Thread.Sleep(10)
                }

                log::debug!("Adding all landblocks to destruction queue...");

                // Queue unloading of all the landblocks
                // The actual unloading will happen in WorldManager.UpdateGameWorld
                crate::managers::landblock_manager::add_all_active_landblocks_to_destruction_queue(
                    w,
                );

                // Wait for all landblocks to unload
                t.log_update_ts = DotNetDateTime::MIN_VALUE;
                t.stage = ShutdownStage::UnloadingLandblocks;
            }
            ShutdownStage::UnloadingLandblocks => {
                let landblock_count =
                    crate::managers::landblock_manager::get_loaded_landblocks(w).len();
                if landblock_count > 0 {
                    t.log_update_ts = log_status_update(
                        w,
                        t.log_update_ts,
                        &format!(
                            "Waiting for {landblock_count} loaded landblock{} to unload...",
                            if landblock_count > 1 { "s" } else { "" }
                        ),
                    );
                    return false; // Thread.Sleep(10)
                }

                log::debug!("Stopping world...");

                // Disabled thread update loop
                world_manager::stop_world(w);

                // Halt mods
                // ACE: ModManager.Shutdown
                // Not ported: Empyrean has no mod loader, so there are no mods to halt.

                // Wait for world to end
                t.log_update_ts = DotNetDateTime::MIN_VALUE;
                t.stage = ShutdownStage::StoppingWorld;
            }
            ShutdownStage::StoppingWorld => {
                if w.world_manager.world_active {
                    t.log_update_ts =
                        log_status_update(w, t.log_update_ts, "Waiting for world to stop...");
                    return false; // Thread.Sleep(10)
                }

                log::info!("Saving OfflinePlayers that have unsaved changes...");
                player_manager::save_offline_players_with_changes(w);

                // Wait for the database queue to empty
                t.log_update_ts = DotNetDateTime::MIN_VALUE;
                t.stage = ShutdownStage::WaitingForDatabase;
            }
            ShutdownStage::WaitingForDatabase => {
                let shard_queue_count = w.shard.queue_count();
                if shard_queue_count > 0 {
                    t.log_update_ts = log_status_update(
                        w,
                        t.log_update_ts,
                        &format!("Waiting for database queue ({shard_queue_count}) to empty..."),
                    );
                    return false; // Thread.Sleep(10)
                }

                // Write exit to console/log
                log::info!("Exiting at {}", to_common_string(utc));

                // System exit
                t.stage = ShutdownStage::Exited;
            }
            ShutdownStage::Exited => return true,
        }
    }
}

// ACE: ServerManager.LogStatusUpdate
fn log_status_update(
    w: &World,
    log_update_ts: DotNetDateTime,
    log_message: &str,
) -> DotNetDateTime {
    let utc = w.now.utc;
    // logUpdateTS is always a UTC time, so ToUniversalTime() leaves it as it is.
    if log_update_ts == DotNetDateTime::MIN_VALUE || utc > log_update_ts {
        log::info!("{log_message}");
        return utc.add_seconds(10.0);
    }

    log_update_ts
}

// ACE: ServerManager.NotifyPlayersOfPendingShutdown
fn notify_players_of_pending_shutdown(
    w: &mut World,
    last_notice_time: DotNetDateTime,
    shutdown_time: DotNetDateTime,
) -> DotNetDateTime {
    let utc = w.now.utc;
    let sdt = shutdown_time - utc;
    let time = pending_shutdown_time_text(sdt);

    let notify = matches!(
        time.as_str(),
        "2 hours"
            | "1 hour"
            | "45 minutes"
            | "30 minutes"
            | "15 minutes"
            | "10 minutes"
            | "5 minutes"
            | "2 minutes"
            | "1 minute and 30 seconds"
            | "1 minute"
            | "30 seconds"
            | "15 seconds"
            | "10 seconds"
            | "5 seconds"
    );

    if notify && (utc - last_notice_time).total_seconds() > 2.0 {
        let broadcast = pending_shutdown_broadcast(sdt, &time);
        // Not ACE: the countdown the players see is in the server's log too.
        log::info!("{broadcast}");
        for player in player_manager::get_all_online(w) {
            if let Some(session) = player_manager::player_session(w, player) {
                sessions::world_broadcast(w, session, &broadcast);
            }
        }

        return utc;
    }

    last_notice_time
}

/// The `time` text `NotifyPlayersOfPendingShutdown` builds from the time left, for example
/// `"1 minute and 30 seconds"` or `"2 hours, 5 minutes and 1 second"`.
pub fn pending_shutdown_time_text(sdt: TimeSpan) -> String {
    let hours = sdt.hours();
    let minutes = sdt.minutes();
    let seconds = sdt.seconds();

    let time_hrs = format!(
        "{}{}",
        if hours >= 1 {
            sdt.format("%h")
        } else {
            String::new()
        },
        if hours >= 2 {
            " hours"
        } else if hours == 1 {
            " hour"
        } else {
            ""
        }
    );
    let time_mins = format!(
        "{}{}",
        if minutes != 0 {
            sdt.format("%m")
        } else {
            String::new()
        },
        if minutes >= 2 {
            " minutes"
        } else if minutes == 1 {
            " minute"
        } else {
            ""
        }
    );
    let time_secs = format!(
        "{}{}",
        if seconds != 0 {
            sdt.format("%s")
        } else {
            String::new()
        },
        if seconds >= 2 {
            " seconds"
        } else if seconds == 1 {
            " second"
        } else {
            ""
        }
    );

    let mut time = time_hrs.clone();
    if !time_mins.is_empty() {
        time += if time_hrs.is_empty() { "" } else { ", " };
        time += &time_mins;
    }
    if !time_secs.is_empty() {
        time += if !time_hrs.is_empty() || !time_mins.is_empty() {
            " and "
        } else {
            ""
        };
        time += &time_secs;
    }
    time
}

/// The broadcast `NotifyPlayersOfPendingShutdown` sends each online player.
pub fn pending_shutdown_broadcast(sdt: TimeSpan, time: &str) -> String {
    if sdt.total_seconds() > 10.0 {
        format!(
            "Broadcast from System> {} - This Asheron's Call Server will be shutting down in {time}{}{}",
            if sdt.total_minutes() > 1.5 { "ATTENTION" } else { "WARNING" },
            if sdt.total_minutes() <= 1.0 { "!" } else { "." },
            if sdt.total_minutes() <= 3.0 {
                format!(" Please log out{}", if sdt.total_minutes() <= 1.0 { "!" } else { "." })
            } else {
                String::new()
            }
        )
    } else {
        "Broadcast from System> ATTENTION - This Asheron's Call Server is shutting down NOW!!!!"
            .to_owned()
    }
}

// ACE: ServerManager.StartupAbort
pub fn startup_abort(w: &mut World) {
    w.server_manager.shutdown_initiated = true;
    sync_net(w);
}

// ACE: ServerManager.ShutdownNoticeText
// Not ACE's (a fix, V344): a separator goes only between two parts that are
// both there, so exactly two minutes reads "2 minutes" and one hour and five seconds "1 hour and 5
// seconds". ACE added ", " and " and " whenever the text so far was non-empty, even before an
// empty part ("2 minutes and ", "1 hour,  and 5 seconds").
pub fn shutdown_notice_text(w: &World) -> String {
    let sdt = w.server_manager.shutdown_time - w.now.utc;

    let mut time_to_shutdown = if sdt.hours() > 0 {
        format!(
            "{} hour{}",
            sdt.hours(),
            if sdt.hours() > 1 { "s" } else { "" }
        )
    } else {
        String::new()
    };
    let minutes = if sdt.minutes() > 0 {
        format!(
            "{} minute{}",
            sdt.minutes(),
            if sdt.minutes() > 1 { "s" } else { "" }
        )
    } else {
        String::new()
    };
    if !minutes.is_empty() {
        time_to_shutdown = format!(
            "{time_to_shutdown}{}{minutes}",
            if time_to_shutdown.is_empty() {
                ""
            } else {
                ", "
            }
        );
    }
    let seconds = if sdt.seconds() > 0 {
        format!(
            "{} second{}",
            sdt.seconds(),
            if sdt.seconds() > 1 { "s" } else { "" }
        )
    } else {
        String::new()
    };
    if !seconds.is_empty() {
        time_to_shutdown = format!(
            "{time_to_shutdown}{}{seconds}",
            if time_to_shutdown.is_empty() {
                ""
            } else {
                " and "
            }
        );
    }

    if sdt.total_seconds() > 10.0 {
        format!(
            "Broadcast from System> {} - This Asheron's Call Server will be shutting down in {time_to_shutdown}{}{}",
            if sdt.total_minutes() > 1.5 { "ATTENTION" } else { "WARNING" },
            if sdt.total_minutes() <= 1.0 { "!" } else { "." },
            if sdt.total_minutes() <= 3.0 {
                format!(" Please log out{}", if sdt.total_minutes() <= 1.0 { "!" } else { "." })
            } else {
                String::new()
            }
        )
    } else {
        "Broadcast from System> ATTENTION - This Asheron's Call Server is shutting down NOW!!!!"
            .to_owned()
    }
}
