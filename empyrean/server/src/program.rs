// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Program.cs
//! Port of `Source/ACE.Server/Program.cs`.
//!
//! `Main` brings the server up in ACE's order and hands the world to its thread. What differs:
//!
//! - **Threads.** The world thread owns the `World` and its transport, runs
//!   `WorldManager.UpdateWorld`, and between iterations takes the commands the main thread sends
//!   it (routed through the world queue) and gives `ServerManager.ShutdownServer` its turn. The UDP
//!   driver's two receive threads only read datagrams. The main thread waits for Ctrl-C or
//!   `--run-for`, then asks for `ServerManager.DoShutdownNow` and waits for the world thread.
//! - **Databases** (see [`empyrean_server::database_manager`]): the world database is `PackContent`
//!   over `world.pack` (see [`empyrean_server::world_pack`]; a missing pack boots with empty content),
//!   the shard and authentication databases are SQLite files (`shard.db`, `auth.db`), opened on
//!   the main thread and handed to the world thread, where `DatabaseManager.Initialize`/`Start`
//!   run (their databases are fields of the world) and `DatabaseManager.Stop` runs after the loop.
//! - **Missing pieces.** Every unported step is a `not_ported!` site named after its ACE member.
//!   `GuidManager.Initialize` reads the shard through [`empyrean_server::guid_manager_boot`].
//! - **Logging** is a small stderr logger in place of log4net, at the level `server.log_level`
//!   names (`error`, `warn`, `info` (the default), `debug` or `trace`); until the configuration is
//!   read it logs at `info`. Its lines go through the console ([`empyrean_command::console`]), so on an
//!   interactive terminal they print above the prompt instead of into it.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::net::IpAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

use empyrean_common::clock::{Clock, ClockSnapshot, SystemClock};
use empyrean_common::config_manager::{ConfigError, ConfigManager};
use empyrean_common::config_paths::PathBase;
use empyrean_common::master_configuration::MasterConfiguration;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::WorldDatabase;
use empyrean_dat::{DatManager, RealDats};
use empyrean_net::driver::udp::UdpDriver;
use empyrean_net::{NetConfig, Outgoing, PortKind, ServerNet};
use empyrean_server::config_file::{self, ConfigSource};
use empyrean_server::websocket::{EndpointConfig, WebSocketEndpoint};
use empyrean_server::{dat_directory, database_manager, guid_manager_boot, world_pack};
use empyrean_store::SqliteShard;
use empyrean_world::entity::actions::i_action::Action;
use empyrean_world::entity::timers::TimersState;
use empyrean_world::managers::event_manager;
use empyrean_world::managers::guid_manager;
use empyrean_world::managers::server_manager;
use empyrean_world::managers::server_performance_monitor;
use empyrean_world::managers::world_manager::{self, NetDriver, WorldHost};
use empyrean_world::managers::{player_manager, property_manager};
use empyrean_world::network::managers::inbound_message_manager;
use empyrean_world::World;

/// A piece of work for the world thread (a console command, or the shutdown request).
pub type WorldCommand = Box<dyn FnOnce(&mut World) + Send>;

/// The command line.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Options {
    /// `--config <path>`: the `empyrean.toml` to read. Without it the server looks for one (see
    /// [`empyrean_server::config_file`]).
    pub config: Option<String>,
    /// Not ACE: `--write-config [<path>]` (or `--out <path>`): write the configuration in use, or
    /// the `Config.js` named by `--from`, as a commented `empyrean.toml` at this path (default
    /// `empyrean.toml`) and exit.
    pub write_config: Option<String>,
    /// Not ACE: `--from <Config.js>`, with `--write-config`: convert ACE's `Config.js` once.
    pub from: Option<String>,
    /// `--run-for <seconds>`: shut down (as on Ctrl-C) after this long; for smoke tests.
    pub run_for: Option<f64>,
    /// Not ACE: `--status <ip:port>`: serve `GET /status` and `GET /health` there (see
    /// [`empyrean_server::status_endpoint`]); overrides `server.status_address`.
    pub status: Option<String>,
    /// Not ACE: `--update-trial <file>`: the updater's health check of a newly installed release.
    /// The server starts as usual; once its databases are open, its world is running and its
    /// listeners are bound, it writes what it is (`--release-facts`' JSON) to `<file>` and shuts
    /// down cleanly. It never updates itself.
    pub update_trial: Option<PathBuf>,
    /// Not ACE, hidden: `--update-test-version <version>`, for the updater's own tests: the updater
    /// takes this process for that version (`crate::server_update::TEST_VERSION_FLAG`).
    pub update_test_version: Option<String>,
}

impl Options {
    /// Parses the arguments after the program name.
    ///
    /// # Errors
    /// An unknown argument, or a flag without its value.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.into_iter().peekable();
        let mut write_config = false;
        let mut out: Option<String> = None;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--config" => options.config = Some(args.next().ok_or("--config needs a path")?),
                "--write-config" => {
                    write_config = true;
                    if let Some(path) = args.next_if(|a| !a.starts_with("--")) {
                        out = Some(path);
                    }
                }
                "--from" => {
                    options.from = Some(args.next().ok_or("--from needs the path of a Config.js")?)
                }
                "--out" => out = Some(args.next().ok_or("--out needs a path")?),
                "--run-for" => {
                    let secs = args.next().ok_or("--run-for needs a number of seconds")?;
                    let secs: f64 = secs
                        .parse()
                        .map_err(|_| format!("--run-for: not a number: {secs}"))?;
                    options.run_for = Some(secs);
                }
                "--status" => {
                    options.status = Some(args.next().ok_or("--status needs an <ip>:<port>")?)
                }
                "--update-trial" => {
                    options.update_trial = Some(PathBuf::from(
                        args.next()
                            .ok_or("--update-trial needs the path of its report")?,
                    ))
                }
                crate::server_update::TEST_VERSION_FLAG => {
                    options.update_test_version =
                        Some(args.next().ok_or("--update-test-version needs a version")?)
                }
                "--help" | "-h" => return Err(USAGE.to_owned()),
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        if write_config {
            options.write_config =
                Some(out.unwrap_or_else(|| config_file::NATIVE_FILE_NAME.to_owned()));
        } else if options.from.is_some() || out.is_some() {
            return Err("--from and --out go with --write-config".to_owned());
        }
        if options.from.is_some() && options.config.is_some() {
            return Err("--write-config reads either --from <Config.js> or --config <empyrean.toml>, not both".to_owned());
        }
        Ok(options)
    }
}

/// The usage line (`--help`).
pub const USAGE: &str = "usage: empyrean-server [--config <empyrean.toml>] [--run-for <seconds>] [--status <ip>:<port>]\n       empyrean-server --write-config [<path>] [--from <Config.js>] [--out <path>]\n       empyrean-server --version";

// ACE: Program.Main
/// Brings the server up, runs it until Ctrl-C or `--run-for`, and shuts it down.
pub fn main(options: &Options) -> ExitCode {
    if let Some(path) = &options.write_config {
        return write_config(options, path);
    }

    std::panic::set_hook(Box::new(|info| {
        current_domain_unhandled_exception(&info.to_string())
    }));

    // DIVERGE: the start-up and abort lines name Empyrean where ACE's name ACEmulator (brand).
    log::info!("Starting {}...", empyrean_common::brand::PRODUCT);
    let started = std::time::Instant::now();

    log::info!("Initializing ConfigManager...");
    let (config, paths, config_path) = match initialize_config(options.config.as_deref()) {
        Ok(loaded) => loaded,
        Err(e) => {
            log::error!("Unable to load the configuration: {e}");
            return ExitCode::FAILURE;
        }
    };
    apply_log_level(&config.server.log_level);
    log_paths(&config, &paths);

    // ACE: ModManager.Initialize
    // Not ported: ACE loads compiled C# mods from a folder; Empyrean has no mod loader, so there
    // is nothing to initialize (and no "Initializing ModManager..." line).

    if config.server.world_name
        != empyrean_common::game_configuration::GameConfiguration::default().world_name
    {
        log::info!(
            "{} | {}",
            config.server.world_name,
            empyrean_common::brand::PRODUCT
        );
    }

    offline_tools(&config, &paths);
    // (ACE's update checks and customisation steps follow here; their settings are not part of
    // Empyrean's configuration, see empyrean_common::toml_config::REMOVED.)

    // pre-load starterGear.json, abort startup if file is not found as it is required to create new characters.
    // (The starter gear is compiled in here, so no file is read.)
    if empyrean_world::factories::starter_gear_factory::get_starter_gear_configuration().is_none() {
        log::error!("Unable to load the starter gear data. Empyrean will now abort startup.");
        // ServerManager.StartupAbort() sets ShutdownInitiated; the world that holds the flag is
        // not built yet, so it is passed on. Environment.Exit(0): OnProcessExit, exit code 0.
        on_process_exit(true);
        return ExitCode::SUCCESS;
    }

    log::info!("Initializing ServerManager...");
    // (ServerManager.Initialize runs below, once the world that holds its state exists.)

    log::info!("Initializing DatManager...");
    let dats = match open_dats(&config, &paths) {
        Ok(dats) => dats,
        Err(e) => {
            log::error!("DatManager initialization failed: {e}. Empyrean will now abort startup.");
            return ExitCode::FAILURE;
        }
    };

    // `DDDManager.Initialize()` fills state that lives in `World`, so it runs on the world thread as
    // soon as the world exists (below); the log line keeps ACE's position in the start-up sequence.
    // Not ACE: a world with a data overlay patches clients that keep overlays (V437).
    let ddd_patching = config.ddd.enable_dat_patching
        || (!config.dat_overlay.path.trim().is_empty() && config.dat_overlay.patching);
    if ddd_patching {
        log::info!("Initializing DDDManager...");
    } else {
        log::info!("DAT Patching Disabled...");
    }

    log::info!("Initializing DatabaseManager...");
    let clock = Arc::new(SystemClock::new());
    // DatabaseManager.World: PackContent over world.pack (server.world_pack_path).
    let world_pack_path = world_pack::configured_world_pack_path(&config, &paths);
    // Not ACE: the content overlay (Server.WorldOverlayPath) in front of the pack.
    let (content, world_pack_status, overlay_status) = world_pack::open_world_database_with_overlay(
        &world_pack_path,
        world_pack::configured_overlay(&config, &paths),
    );
    if let Some(Err(e)) = overlay_status {
        log::error!(
            "The content overlay could not be opened: {e}. Empyrean will now abort startup."
        );
        return ExitCode::FAILURE;
    }
    let world_content_loaded = matches!(
        world_pack_status,
        world_pack::WorldPackStatus::Loaded { .. }
    );
    // DIVERGE: ACE cannot start without its world database (the connection fails and
    // Initialize retries). The real server therefore refuses to start without a usable world.pack,
    // rather than booting an empty world whose first character creation fails. Test worlds are built
    // by TestServer, not here, and keep the empty-content path (start-up checks skipped).
    if let Some(message) = world_pack::unusable_pack_message(&world_pack_path, &world_pack_status) {
        log::error!("{message}");
        return ExitCode::FAILURE;
    }
    // DatabaseManager.Shard and .Authentication: shard.db and auth.db (database.shard_db_path,
    // database.auth_db_path).
    let opened = database_manager::open_databases(
        &database_manager::configured_shard_db_path(&config, &paths),
        &database_manager::configured_auth_db_path(&config, &paths),
        config.server.accounts.clone(),
        clock.clone(),
    );
    let (shard_db, auth_db) = match opened {
        Ok(databases) => databases,
        Err(e) => {
            log::error!("Unable to open the database: {e}. Empyrean will now abort startup.");
            return ExitCode::FAILURE;
        }
    };

    // The world holds the shared physics world, which is not `Send` (its `Box<dyn MotionSource>`
    // seam has no `Send` bound), so the world is built on its own thread and each start-up step
    // that needs it runs there, in ACE's order, while this thread waits.
    let (startup, startup_rx) = mpsc::channel::<Startup>();
    // The main thread waits on `stop` for Ctrl-C, `--run-for`, or the world thread ending (a
    // console `exit`, ACE's `Environment.Exit` at the end of `ShutdownServer`).
    let (stop, stop_rx) = mpsc::channel::<StopCause>();
    let world_loop_ended = Arc::new(AtomicBool::new(false));
    let world_thread = {
        let loop_ended = Arc::clone(&world_loop_ended);
        let clock = Arc::clone(&clock);
        let config = Arc::clone(&config);
        let world_ended = stop.clone();
        match std::thread::Builder::new()
            .name("World Manager".to_owned())
            .spawn(move || {
                let world = new_world(&*clock, dats, content, &config);
                let shutdown_initiated = world_thread_main(world, &*clock, &startup_rx);
                loop_ended.store(true, Ordering::SeqCst);
                let _ = world_ended.send(StopCause::WorldEnded);
                shutdown_initiated
            }) {
            Ok(handle) => handle,
            Err(e) => {
                log::error!("Unable to start the world thread: {e}");
                return ExitCode::FAILURE;
            }
        }
    };
    {
        let config = Arc::clone(&config);
        on_world(&startup, move |w| server_manager::initialize(w, &config));
    }
    if ddd_patching {
        on_world(&startup, |w| {
            let dats = Arc::clone(&w.dats);
            empyrean_world::managers::ddd_manager::initialize(&mut w.ddd_manager, &dats);
        });
    }
    {
        // ServerPerformanceMonitor times events on the real clock (tests inject a virtual one).
        let monitor_clock: Arc<dyn Clock> = clock.clone();
        on_world(&startup, move |w| {
            server_performance_monitor::use_clock(w, monitor_clock)
        });
    }

    let initialization_failure = {
        let options = database_manager::InitializeOptions {
            world_content_loaded,
            shard_player_biota_cache_time: config.server.shard_player_biota_cache_time,
            shard_non_player_biota_cache_time: config.server.shard_non_player_biota_cache_time,
            clock: clock.clone(),
            threaded: true,
        };
        on_world_value(&startup, move |w| {
            database_manager::initialize(w, Box::new(auth_db), shard_db, &options)
        })
        .unwrap_or(true)
    };

    if initialization_failure {
        log::error!("DatabaseManager initialization failed. Empyrean will now abort startup.");
        // Environment.Exit(0): runs OnProcessExit (DatabaseManager.Stop on the world's thread), and
        // exits with code 0 as ACE does.
        let shutdown_initiated = on_world_value(&startup, |w| {
            server_manager::startup_abort(w);
            database_manager::stop(w);
            w.server_manager.shutdown_initiated
        })
        .unwrap_or(false);
        drop(startup);
        let _ = world_thread.join();
        on_process_exit(shutdown_initiated);
        return ExitCode::SUCCESS;
    }

    log::info!("Starting DatabaseManager...");
    on_world(&startup, database_manager::start);

    log::info!("Starting PropertyManager...");
    // DatabaseManager.ShardConfig: its own connection to the shard database (ACE's
    // ShardConfigDatabase opens a context of its own for each call).
    let shard_config = match SqliteShard::open(database_manager::configured_shard_db_path(
        &config, &paths,
    )) {
        Ok(db) => property_manager::shard_config_handle(Box::new(db)),
        Err(e) => {
            log::error!("Unable to open the shard database for the server properties: {e}. Empyrean will now abort startup.");
            drop(startup);
            let _ = world_thread.join();
            return ExitCode::FAILURE;
        }
    };
    on_world(&startup, move |w| {
        property_manager::install_shard_config(w, shard_config);
        property_manager::initialize(w, true);
    });

    log::info!("Initializing GuidManager...");
    on_world(&startup, |w| {
        guid_manager_boot::initialize(w);
        // Not ACE: the allocators' start points, for the smoke log (ACE logs them at debug level).
        let player_current = w
            .guid_manager
            .player_alloc
            .as_ref()
            .map_or(0, |a| a.current());
        log::info!(
            "GuidManager: next player GUID {player_current:08X}; {}",
            guid_manager::get_dynamic_guid_debug_info(w)
        );
    });

    if config.server.server_performance_monitor_auto_start {
        log::info!("Server Performance Monitor auto starting...");
        on_world(&startup, server_performance_monitor::start);
    }

    // (ACE precaches the world database here when Server.WorldDatabasePrecaching is set; Empyrean
    // reads world.pack, memory-mapped, as content is needed, and has no such setting.)

    log::info!("Initializing PlayerManager...");
    on_world(&startup, player_manager::initialize);

    log::info!("Initializing HouseManager...");
    on_world(
        &startup,
        empyrean_world::managers::house_manager::initialize,
    );

    log::info!("Initializing InboundMessageManager...");
    inbound_message_manager::initialize();

    log::info!("Initializing SocketManager...");
    let driver = match bind_sockets(&config, &paths) {
        Ok(driver) => driver,
        Err(e) => {
            log::error!("Unable to bind the listeners: {e}");
            drop(startup);
            let _ = world_thread.join();
            return ExitCode::FAILURE;
        }
    };

    log::info!("Initializing WorldManager...");
    on_world(&startup, |w| world_manager::initialize(w));

    // DIVERGE: ACE runs EventManager.Initialize on the main thread just after the world thread
    // starts, racing its first landblock loads; here it runs on the world before the loop starts,
    // so no generator checks its event before the events are loaded.
    log::info!("Initializing EventManager...");
    on_world(&startup, event_manager::initialize);

    let (commands, command_rx) = mpsc::channel::<WorldCommand>();
    if startup.send(Startup::Go(driver, command_rx)).is_err() {
        log::error!("The world thread ended during start-up");
        return ExitCode::FAILURE;
    }

    // This should be last
    log::info!("Initializing CommandManager...");
    {
        // The console thread parses each line and sends its lookup and invoke to the world thread.
        let console = commands.clone();
        empyrean_command::command_manager::initialize(Some(Box::new(move |c| {
            console.send(c).is_ok()
        })));
    }

    // ACE: ModManager.RegisterCommands
    // Not ported: without a mod loader there are no mod commands to register.

    let status = start_status_endpoint(options, &config, &commands, started);

    // Not ACE: the updater (`server.update`); never in the updater's own health check.
    let auto_update = match &options.update_trial {
        Some(_) => None,
        None => crate::server_update::start(
            &config,
            &paths,
            config_path.as_deref(),
            &commands,
            options.update_test_version.as_deref(),
        ),
    };

    // The world runs on its own thread by now, so the property is read there.
    send(
        &commands,
        Box::new(|w: &mut World| {
            if !world_manager::property_manager_get_bool_world_closed(w) {
                world_manager::open(w, None);
            }
        }),
    );

    let run_for = match &options.update_trial {
        Some(report) => {
            report_ready(&commands, report);
            Some(0.0)
        }
        None => options.run_for,
    };
    let cause = wait_for_stop(run_for, stop, &stop_rx);

    // After a console `exit` the world thread has already run the whole shutdown.
    if !world_loop_ended.load(Ordering::SeqCst) {
        log::info!("Shutting down...");
        send(&commands, Box::new(server_manager::do_shutdown_now));
    }
    let shutdown_initiated = match world_thread.join() {
        Ok(initiated) => initiated,
        Err(_) => {
            log::error!("The world thread ended with a panic");
            return ExitCode::FAILURE;
        }
    };

    on_process_exit(shutdown_initiated);
    // The status address is free for whichever server runs next.
    if let Some(status) = status {
        status.shutdown();
    }
    // Not ACE: a release the updater staged is installed now that everything is saved, and the
    // new server (or, when it fails, the restored one) takes over this process.
    if let Some(code) = crate::server_update::after_shutdown(auto_update.as_deref(), cause) {
        return code;
    }
    ExitCode::SUCCESS
}

/// Why the main thread stopped waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopCause {
    /// The world thread ended by itself: a shutdown countdown ran out, or the console's `exit`.
    WorldEnded,
    /// Ctrl-C, or SIGINT, SIGTERM or SIGHUP on Unix.
    Interrupt,
    /// `--run-for` elapsed.
    RunFor,
}

/// Not ACE: the health check's report (`--update-trial`): once the world thread answers, this
/// build's facts are written to `report`. Nothing is written when it does not answer, which fails
/// the check.
fn report_ready(commands: &Sender<WorldCommand>, report: &std::path::Path) {
    let (tx, rx) = mpsc::channel();
    send(
        commands,
        Box::new(move |_: &mut World| {
            let _ = tx.send(());
        }),
    );
    if rx.recv_timeout(Duration::from_secs(120)).is_err() {
        log::error!("Health check: the world thread did not answer");
        return;
    }
    let facts = empyrean_server::update::release::Facts::this_build().to_json();
    match std::fs::write(report, facts) {
        Ok(()) => log::info!("Health check: ready; shutting down"),
        Err(e) => log::error!("Health check: {}: {e}", report.display()),
    }
}

// ACE: Program.CurrentDomain_UnhandledException
/// The panic hook: an unhandled panic is logged (a caught one, too, before its catch logs it).
fn current_domain_unhandled_exception(exception_object: &str) {
    log::error!("{exception_object}");
}

// ACE: Program.OnProcessExit
/// `DatabaseManager.Stop`, which needs the world, has already run on the world thread when the
/// loop ended (see [`run_world_thread`]).
fn on_process_exit(shutdown_initiated: bool) {
    if !shutdown_initiated {
        log::warn!("Unsafe server shutdown detected! Data loss is possible!");
    }

    // PropertyManager.StopUpdating() needs the world, so it runs on the world thread after the
    // loop (see [`run_world_thread`]).
}

/// `ConfigManager.Initialize()`, after ACE's check for `Config.js`.
///
/// DIVERGE: the file is `empyrean.toml`, found by [`config_file::discover`]; ACE reads
/// `Config.js`, which is read here only by the converter (`--write-config --from`).
///
/// Not ACE: also returns the base the configuration's paths resolve against (the file's folder,
/// or the working directory on the defaults; `empyrean_common::config_paths`).
fn initialize_config(
    explicit: Option<&str>,
) -> Result<(Arc<MasterConfiguration>, PathBase, Option<PathBuf>), ConfigError> {
    let (config, paths, file) = load_config(explicit)?;
    ConfigManager::initialize(config);
    Ok((ConfigManager::config(), paths, file))
}

/// Not ACE: finds and reads the configuration, logging which file is used and every key it does
/// not read, with the base its paths resolve against and the file read (`None` on the defaults).
pub fn load_config(
    explicit: Option<&str>,
) -> Result<(MasterConfiguration, PathBase, Option<PathBuf>), ConfigError> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf));
    let discovery = match config_file::discover(explicit, &cwd, exe_dir.as_deref(), &|p| p.exists())
    {
        Ok(discovery) => discovery,
        Err(config_file::DiscoverError::Missing(missing)) => {
            log::error!(
                "Configuration file {} does not exist.",
                absolute(&missing).display()
            );
            return Err(ConfigError::Missing(missing));
        }
        Err(config_file::DiscoverError::ConfigJs(path)) => {
            log::error!("{}", config_file::converter_message(&absolute(&path)));
            return Err(ConfigError::ConfigJs(path));
        }
    };
    match discovery.source {
        ConfigSource::Defaults => {
            // ACE runs its interactive first-time setup when Config.js is missing; Empyrean runs
            // on the defaults.
            log::warn!(
                "No {} in {} or beside the server executable: running on the default configuration",
                config_file::NATIVE_FILE_NAME,
                cwd.display()
            );
            Ok((
                MasterConfiguration::default(),
                PathBase::for_process(None),
                None,
            ))
        }
        ConfigSource::File(path) => {
            log::info!("Configuration: {}", absolute(&path).display());
            for ignored in &discovery.ignored {
                log::warn!(
                    "Configuration: {} is also present and is not read",
                    absolute(ignored).display()
                );
            }
            match config_file::load(&path) {
                Ok(loaded) => {
                    for warning in empyrean_common::toml_config::warnings(&loaded, &path) {
                        log::warn!("{warning}");
                    }
                    let paths = PathBase::for_process(Some(&path));
                    Ok((loaded.config, paths, Some(path)))
                }
                Err(exception) => {
                    log::error!("An exception occured while loading the configuration file!");
                    log::error!("Exception: {exception}");
                    Err(exception)
                }
            }
        }
    }
}

/// Not ACE: logs where every configured path resolved (`empyrean_common::config_paths`), so an
/// operator can see which files the server uses.
fn log_paths(config: &MasterConfiguration, paths: &PathBase) {
    log::info!(
        "Paths: relative paths in the configuration resolve against {}",
        paths.describe()
    );
    log::info!(
        "Paths: dat files: {}",
        dat_directory::configured_dat_directory(config, paths).display()
    );
    log::info!(
        "Paths: world database: {}",
        world_pack::configured_world_pack_path(config, paths).display()
    );
    if let Some((overlay, base)) = world_pack::configured_overlay(config, paths) {
        log::info!("Paths: content overlay: {}", overlay.display());
        log::info!("Paths: overlay base dump: {}", base.sql.display());
        for patch in &base.patches {
            log::info!("Paths: overlay base patch: {}", patch.path.display());
        }
    }
    log::info!(
        "Paths: shard database: {}",
        database_manager::configured_shard_db_path(config, paths).display()
    );
    log::info!(
        "Paths: authentication database: {}",
        database_manager::configured_auth_db_path(config, paths).display()
    );
}

fn absolute(path: &std::path::Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Not ACE: `--write-config [<path>]`: writes the configuration in use (found as at startup, or
/// named by `--config`) as a commented `empyrean.toml`, and exits; with `--from <Config.js>`, the
/// one-time converter: ACE's `Config.js` is read and written as `empyrean.toml`, and every key it
/// drops is logged and listed in the file's header. It never overwrites a file.
fn write_config(options: &Options, path: &str) -> ExitCode {
    let target = PathBuf::from(path);
    if target.exists() {
        log::error!(
            "{} already exists: not overwritten. Name another path, or move it away first.",
            absolute(&target).display()
        );
        return ExitCode::FAILURE;
    }
    let text = if let Some(from) = &options.from {
        let source = PathBuf::from(from);
        let converted = std::fs::read_to_string(&source)
            .map_err(ConfigError::Io)
            .and_then(|text| empyrean_common::toml_config::from_config_js_str(&text));
        let converted = match converted {
            Ok(converted) => converted,
            Err(e) => {
                log::error!("Unable to read {}: {e}", absolute(&source).display());
                return ExitCode::FAILURE;
            }
        };
        for dropped in &converted.dropped {
            log::warn!(
                "{}: `{}` dropped: {}",
                source.display(),
                dropped.key,
                dropped.why
            );
        }
        let note = empyrean_common::toml_config::conversion_note(
            &source.display().to_string(),
            &converted.dropped,
        );
        empyrean_common::toml_config::to_toml_string_with_note(&converted.config, Some(&note))
    } else {
        match load_config(options.config.as_deref()) {
            Ok((config, _, _)) => empyrean_common::toml_config::to_toml_string(&config),
            Err(e) => {
                log::error!("Unable to load the configuration: {e}");
                return ExitCode::FAILURE;
            }
        }
    };
    match empyrean_common::toml_config::write_text_file(&text, &target, false) {
        Ok(()) => {
            log::info!("Wrote {}", absolute(&target).display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            log::error!("Unable to write {}: {e}", absolute(&target).display());
            ExitCode::FAILURE
        }
    }
}

/// The offline maintenance steps at the top of `Main` (ACE's update steps are not part of
/// Empyrean; see empyrean_common::toml_config::REMOVED).
fn offline_tools(config: &MasterConfiguration, paths: &PathBase) {
    use empyrean_store::shard_database_offline_tools as tools;
    let offline = &config.offline;
    // ACE's offline tools work on the shard directly, before DatabaseManager starts; here they open
    // the shard file for the duration of the steps that need it.
    let wanted = offline.purge_deleted_characters
        || offline.purge_orphaned_biotas
        || offline.prune_deleted_characters_from_friend_lists
        || offline.prune_deleted_objects_from_shortcut_bars
        || offline.prune_deleted_characters_from_squelch_lists;
    let mut shard = if wanted {
        match SqliteShard::open(database_manager::configured_shard_db_path(config, paths)) {
            Ok(shard) => Some(shard),
            Err(e) => {
                log::error!("Offline tools: unable to open the shard: {e}");
                None
            }
        }
    } else {
        None
    };
    if let Some(db) = shard.as_mut() {
        if offline.purge_deleted_characters {
            let now = SystemClock::new().utc_now();
            log::info!(
                "Purging deleted characters, and their possessions, older than {} days ({})...",
                offline.purge_deleted_characters_days,
                now.add_days(-f64::from(offline.purge_deleted_characters_days))
            );
            let (characters, player_biotas, possessions) =
                tools::purge_characters_in_parallel(db, offline.purge_deleted_characters_days, now);
            log::info!("Purged {characters} characters, {player_biotas} player biotas and {possessions} possessions.");
        }
        if offline.purge_orphaned_biotas {
            log::info!("Purging orphaned biotas...");
            let purged = tools::purge_orphaned_biotas_in_parallel(db);
            log::info!("Purged {purged} biotas.");
        }
        if offline.prune_deleted_characters_from_friend_lists {
            log::info!("Pruning invalid friends from all friend lists...");
            let pruned = tools::prune_deleted_characters_from_friend_lists(db);
            log::info!("Pruned {pruned} invalid friends found on friend lists.");
        }
        if offline.prune_deleted_objects_from_shortcut_bars {
            log::info!("Pruning invalid shortcuts from all shortcut bars...");
            let pruned = tools::prune_deleted_objects_from_shortcut_bars(db);
            log::info!("Pruned {pruned} deleted objects found on shortcut bars.");
        }
        if offline.prune_deleted_characters_from_squelch_lists {
            log::info!("Pruning invalid squelches from all squelch lists...");
            let pruned = tools::prune_deleted_characters_from_squelch_lists(db);
            log::info!("Pruned {pruned} invalid squelched characters found on squelch lists.");
        }
    }

    // `CheckForBiotaPropertiesPaletteOrderColumnInShard`: a MySQL schema repair; our schema has the
    // column (V173), so the port is a no-op.
    if let Some(db) = shard.as_mut() {
        tools::check_for_biota_properties_palette_order_column_in_shard(db);
    }
}

/// `DatManager.Initialize(ConfigManager.Config.Server.DatFilesDirectory, true)`.
///
/// Not ACE: the directory is found by [`dat_directory::configured_dat_directory`]
/// (`dat_files_directory` only; empty means the configuration file's folder, then beside the
/// executable).
fn open_dats(config: &MasterConfiguration, paths: &PathBase) -> Result<Arc<DatManager>, String> {
    let dir = dat_directory::configured_dat_directory(config, paths);
    // Not ACE: one folder may hold both dat sets; the world's overlay's base, else its era,
    // chooses which it reads.
    let mut source = RealDats::open_era(&dir, dat_directory::world_set(config, paths))
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    // Not ACE: the world's data overlay over the base files.
    let overlay = config.dat_overlay.path.trim();
    if !overlay.is_empty() {
        let overlay = paths.resolve(overlay);
        source = source
            .with_overlay(&overlay)
            .map_err(|e| format!("the data overlay {}: {e}", overlay.display()))?;
        log::info!(
            "The world's data overlay {} is read over the base files{}",
            overlay.display(),
            if config.dat_overlay.patching {
                ", and patched into clients that keep overlays"
            } else {
                ""
            }
        );
    }
    DatManager::initialize(Arc::new(source)).map_err(|e| e.to_string())
}

/// The world, its `Timers` started now, its world database `content`, and its transport
/// configured from `Config.Server.Network`.
fn new_world(
    clock: &dyn Clock,
    dats: Arc<DatManager>,
    content: Arc<dyn WorldDatabase>,
    config: &MasterConfiguration,
) -> World {
    let timers = TimersState::new(clock);
    let now = ClockSnapshot::take(clock, timers.portal_year_ticks);
    let mut world = World::new(now, dats);
    world.timers = timers;
    world.content = content;

    let n = &config.server.network;
    let net_config = NetConfig {
        port: u16::try_from(n.port).unwrap_or(9000),
        maximum_allowed_sessions: n.maximum_allowed_sessions,
        default_session_timeout: n.default_session_timeout,
        maximum_allowed_sessions_per_ip_address: n.maximum_allowed_sessions_per_ip_address,
        allow_unlimited_sessions_from_ip_addresses: n
            .allow_unlimited_sessions_from_ip_addresses
            .iter()
            .filter_map(|a| a.parse().ok())
            .collect(),
        rng_seed: RandomState::new().build_hasher().finish(),
    };
    world.net = ServerNet::new(
        net_config,
        empyrean_world::network::game_messages::game_message::transport_messages(),
    );
    world
}

/// `SocketManager.Initialize()`: the listeners on `Host`, ports `Port` and `Port + 1`, and (not ACE)
/// the WebSocket endpoint when `server.websocket` enables it. A WebSocket setting that cannot be
/// served stops the start-up, as a port that cannot be bound does.
fn bind_sockets(config: &MasterConfiguration, paths: &PathBase) -> Result<UdpWire, String> {
    let n = &config.server.network;
    let host: IpAddr = n
        .host
        .parse()
        .map_err(|_| format!("Host is not an IP address: {}", n.host))?;
    let port = u16::try_from(n.port).map_err(|_| format!("Port out of range: {}", n.port))?;
    let driver = UdpDriver::bind(host, port).map_err(|e| format!("{host}:{port}: {e}"))?;
    for kind in [PortKind::C2S, PortKind::S2C] {
        if let Ok(addr) = driver.local_addr(kind) {
            log::info!("Listening on {addr}");
        }
    }
    let settings = &config.server.web_socket;
    let websocket = if settings.enabled {
        let endpoint = EndpointConfig::from_settings(settings, |p| paths.resolve(p))
            .map_err(|e| format!("WebSocket: {e}"))?;
        let listen = endpoint.listen;
        let ws =
            WebSocketEndpoint::bind(endpoint).map_err(|e| format!("WebSocket {listen}: {e}"))?;
        log::info!(
            "WebSocket endpoint on {} ({}), reached at {}",
            ws.local_addr(),
            if ws.tls() {
                "wss://"
            } else if settings.behind_tls_proxy {
                "ws://, behind a TLS proxy"
            } else {
                "ws://, loopback"
            },
            settings.url(ws.local_addr(), ws.tls())
        );
        Some(ws)
    } else {
        None
    };
    Ok(UdpWire { driver, websocket })
}

/// A start-up step for the world thread, before its loop starts.
enum Startup {
    /// Run one step on the world, then signal.
    Run(WorldCommand, Sender<()>),
    /// Start the loop (`WorldManager.Initialize` starting the world thread).
    Go(UdpWire, Receiver<WorldCommand>),
}

/// Runs `f` on the world thread during start-up and waits for it.
fn on_world(startup: &Sender<Startup>, f: impl FnOnce(&mut World) + Send + 'static) {
    let (done, wait) = mpsc::channel();
    if startup.send(Startup::Run(Box::new(f), done)).is_ok() {
        let _ = wait.recv();
    }
}

/// [`on_world`] with a result; `None` when the world thread has gone.
fn on_world_value<R: Send + 'static>(
    startup: &Sender<Startup>,
    f: impl FnOnce(&mut World) -> R + Send + 'static,
) -> Option<R> {
    let (tx, rx) = mpsc::channel();
    on_world(startup, move |w| {
        let _ = tx.send(f(w));
    });
    rx.try_recv().ok()
}

/// The world thread's start-up: the steps the main thread hands it, then the loop. Answers
/// `ShutdownInitiated`; false when start-up was abandoned.
fn world_thread_main(mut world: World, clock: &dyn Clock, startup: &Receiver<Startup>) -> bool {
    loop {
        match startup.recv() {
            Ok(Startup::Run(step, done)) => {
                step(&mut world);
                let _ = done.send(());
            }
            Ok(Startup::Go(wire, commands)) => {
                return run_world_thread(world, wire, clock, commands)
            }
            Err(_) => return false,
        }
    }
}

/// The world thread: the "World Manager" thread `WorldManager.Initialize` starts, then the rest
/// of `ServerManager.ShutdownServer` once the loop has stopped. Returns `ShutdownInitiated`.
fn run_world_thread(
    mut world: World,
    mut wire: UdpWire,
    clock: &dyn Clock,
    commands: Receiver<WorldCommand>,
) -> bool {
    ThreadSafeRandom::seed_from_entropy();

    let mut host = ServerHost { commands };
    world_manager::world_thread(&mut world, &mut wire, clock, &mut host);

    // The shutdown thread's last waits (the world has stopped; the database queue).
    while world.server_manager.shutdown_stage().is_some()
        && !server_manager::shutdown_server(&mut world)
    {
        std::thread::sleep(Duration::from_millis(10));
    }

    wire.driver.shutdown();
    if let Some(ws) = wire.websocket {
        ws.shutdown();
    }
    // OnProcessExit's PropertyManager.StopUpdating, on the thread that holds the world.
    property_manager::stop_updating(&mut world);
    // OnProcessExit's DatabaseManager.Stop: the world (which holds the databases) cannot leave
    // this thread, so it stops them here, after the loop, before the process exits.
    database_manager::stop(&mut world);
    world.server_manager.shutdown_initiated
}

/// The real network: ACE's two UDP listeners, and (not ACE) the WebSocket endpoint beside them.
/// A datagram for an address the endpoint holds goes to it; every other goes out over UDP.
#[derive(Debug)]
struct UdpWire {
    driver: UdpDriver,
    websocket: Option<WebSocketEndpoint>,
}

impl UdpWire {
    fn transmit(&self, out: &Outgoing) -> std::io::Result<()> {
        if let Some(sent) = self.websocket.as_ref().and_then(|ws| ws.transmit(out)) {
            return sent;
        }
        self.driver.transmit(out)
    }
}

impl NetDriver for UdpWire {
    fn receive(&mut self, net: &mut ServerNet, now: ClockSnapshot) {
        while let Some(d) = self.driver.try_recv() {
            net.on_datagram(d.port_kind, d.from, &d.bytes, now);
        }
        if let Some(ws) = &self.websocket {
            while let Some(d) = ws.try_recv() {
                net.on_datagram(d.port_kind, d.from, &d.bytes, now);
            }
        }
    }

    fn do_session_work(&mut self, net: &mut ServerNet, now: ClockSnapshot) -> usize {
        // ACE's count includes the sessions this pass drops, so it is taken from the pass.
        let session_count = net.do_session_work(now);
        let outgoing: Vec<Outgoing> = net.drain_outgoing().collect();
        for out in &outgoing {
            if let Err(e) = self.transmit(out) {
                if let Some(session) = out.session {
                    net.on_send_error(session, &e.to_string(), now);
                }
            }
        }
        session_count
    }
}

/// What runs beside the world loop: `Thread.Sleep`, the main thread's commands (through the
/// world queue) and the shutdown thread's turn.
#[derive(Debug)]
struct ServerHost {
    commands: Receiver<WorldCommand>,
}

impl WorldHost for ServerHost {
    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }

    fn between_iterations(&mut self, w: &mut World) {
        while let Ok(command) = self.commands.try_recv() {
            world_manager::enqueue_action(w, Action::delegate(command));
        }
        server_manager::shutdown_server(w);
    }
}

fn send(commands: &Sender<WorldCommand>, command: WorldCommand) {
    if commands.send(command).is_err() {
        log::error!("The world thread is gone");
    }
}

/// Not ACE: starts the status endpoint when `--status` or `server.status_address` names an
/// address. Each request takes its snapshot on the world thread through `commands`.
fn start_status_endpoint(
    options: &Options,
    config: &MasterConfiguration,
    commands: &Sender<WorldCommand>,
    started: std::time::Instant,
) -> Option<empyrean_server::status_endpoint::StatusEndpoint> {
    use empyrean_server::status_endpoint::{self, StatusSnapshot};
    let addr = match status_endpoint::status_address(
        options.status.as_deref(),
        &config.server.status_address,
    ) {
        Ok(Some(addr)) => addr,
        Ok(None) => return None,
        Err(e) => {
            log::error!("Status endpoint not started: {e}");
            return None;
        }
    };
    let commands = commands.clone();
    let world_name = config.server.world_name.clone();
    let ws = &config.server.web_socket;
    let websocket_url = ws
        .enabled
        .then(|| ws.validate().ok().map(|(listen, tls)| ws.url(listen, tls)))
        .flatten();
    let allowed_origins = if ws.enabled {
        ws.allowed_origins.clone()
    } else {
        Vec::new()
    };
    let snapshot = move || {
        let uptime = started.elapsed().as_secs();
        let (tx, rx) = mpsc::channel();
        let name = world_name.clone();
        let asked = commands.send(Box::new(move |w: &mut World| {
            let _ = tx.send(StatusSnapshot::take(w, &name, uptime));
        }));
        asked.ok()?;
        let mut s = rx.recv_timeout(status_endpoint::WORLD_REPLY_TIMEOUT).ok()?;
        s.websocket_url.clone_from(&websocket_url);
        Some(s)
    };
    match status_endpoint::start(addr, allowed_origins, snapshot) {
        Ok(endpoint) => {
            log::info!(
                "Status endpoint: http://{}/status and /health",
                endpoint.local_addr()
            );
            Some(endpoint)
        }
        Err(e) => {
            log::error!("Status endpoint not started: {e}");
            None
        }
    }
}

/// Blocks until Ctrl-C, until `run_for` seconds have passed, or until the world thread ends
/// (`stop`/`rx` are the channel the world thread signals on), and says which. A second Ctrl-C
/// ends the process at once. Once the process has handed over to a newly started server
/// ([`crate::server_update`]), Ctrl-C is the new server's to handle and is ignored here.
fn wait_for_stop(
    run_for: Option<f64>,
    tx: Sender<StopCause>,
    rx: &Receiver<StopCause>,
) -> StopCause {
    let pressed = Arc::new(AtomicBool::new(false));
    let on_ctrl_c = move || {
        if crate::server_update::handed_over() {
            return;
        }
        if pressed.swap(true, Ordering::SeqCst) {
            log::warn!("Unsafe server shutdown detected! Data loss is possible!");
            empyrean_command::console::stop();
            std::process::exit(1);
        }
        log::info!("Interrupt (Ctrl-C, or SIGTERM/SIGHUP on Unix): shutting down (interrupt again to exit at once)");
        let _ = tx.send(StopCause::Interrupt);
    };
    // At the interactive prompt the terminal is in raw mode and Ctrl-C arrives as a key.
    empyrean_command::console::set_interrupt_handler(on_ctrl_c.clone());
    let installed = ctrlc::set_handler(on_ctrl_c);
    if let Err(e) = installed {
        log::warn!("Ctrl-C handler not installed: {e}");
    }

    match run_for {
        Some(secs) => match rx.recv_timeout(Duration::from_secs_f64(secs.max(0.0))) {
            Ok(cause) => cause,
            Err(RecvTimeoutError::Disconnected) => StopCause::WorldEnded,
            Err(RecvTimeoutError::Timeout) => {
                log::info!("--run-for {secs} s elapsed");
                StopCause::RunFor
            }
        },
        None => rx.recv().unwrap_or(StopCause::WorldEnded),
    }
}

// ---- logging (Not ACE: log4net's console appender) ---------------------------------------------

/// A stderr logger: `time LEVEL [thread] target: message`, at `log`'s maximum level. Each line is
/// written through the console ([`empyrean_command::console::write_log_line`]).
#[derive(Debug)]
struct StderrLogger {
    clock: SystemClock,
}

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        // The console's terminal library logs its own polling at trace; it would log itself.
        metadata.level() <= log::max_level() && !metadata.target().starts_with("mio")
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let time = self.clock.utc_now().format("yyyy-MM-dd HH:mm:ss.fff");
        let thread = std::thread::current();
        let thread = thread.name().unwrap_or("?");
        // ACE's layout: time, level, thread, message (the message names its own system). The
        // emitting module is added only at debug and trace, where it helps find the source.
        let line = if log::max_level() >= log::LevelFilter::Debug {
            format!(
                "{time} {:5} [{thread}] {}: {}",
                record.level(),
                record.target(),
                record.args()
            )
        } else {
            format!("{time} {:5} [{thread}] {}", record.level(), record.args())
        };
        empyrean_command::console::write_log_line(&line);
    }

    fn flush(&self) {}
}

/// Installs the stderr logger at `info`, the level until the configuration is read
/// ([`apply_log_level`]).
pub fn init_logging() {
    let logger: &'static StderrLogger = Box::leak(Box::new(StderrLogger {
        clock: SystemClock::new(),
    }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

/// Sets the log level `server.log_level` names; an unknown value is warned about and logs at
/// `info`.
fn apply_log_level(value: &str) {
    match config_file::log_level(value) {
        Some(level) => log::set_max_level(level),
        None => {
            log::set_max_level(log::LevelFilter::Info);
            log::warn!(
                "Configuration: server.log_level = \"{value}\" is not one of error, warn, info, debug, trace: logging at info"
            );
        }
    }
}
