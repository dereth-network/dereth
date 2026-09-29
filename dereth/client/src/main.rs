//! The `dereth-client` entry point. Everything it calls lives in the library half of this crate,
//! so the integration tests in `tests/` can drive the same `App` the binary drives.
//!
//! `WinMain`'s steps 9 to 12: parse, initialize the client, run it, and clean up.
//! The library owns the application behavior; this binary only performs those startup steps.
//!
//! # The subsystem, and `--no-console`
//!
//! The retail entry point is `WinMain`, not `main`: the client is a **windowed** application and
//! Windows makes no console for one. Cargo's default is the console subsystem, and a
//! console-subsystem child of a windowed process gets a console *window* -- a black rectangle on
//! the screen beside the game.
//!
//! The attribute below fixes the subsystem to the one the original had, and [`dereth_console`]
//! provides what the console subsystem would give for free: a terminal-started run borrows that
//! terminal, and a run started from Explorer or the launcher gets a console window.
//! `--no-console` skips both, and no console is ever created, rather than one being created,
//! shown and closed again.
//!
//! One behaviour does change for a run started **by hand from a shell**: cmd and PowerShell do not
//! wait for a windows-subsystem process, so the prompt returns immediately and the client's output
//! arrives behind it. `cargo run`, and any launcher that waits on the process itself, are
//! unaffected.
#![windows_subsystem = "windows"]

use dereth_client::config::Config;
use dereth_client::corestrings;

/// A crash report on disk, because an intermittent exit otherwise leaves no reason behind.
///
/// A launcher that starts the client with `Start-Process` and does not redirect its streams
/// sends everything `main` prints -- including the `Error: ...` line on the failure path --
/// to a console that dies with the process. An exit with code **1** would then leave no
/// record of why.
///
/// So every run appends to a file, and the file has **three** states rather than two:
///
/// | the log says | what happened |
/// |---|---|
/// | `START` then `EXIT code=N` | `main` returned; the client itself decided to stop |
/// | `START` then `PANIC` with a backtrace | a Rust panic unwound out of `main` (exit code **101**) |
/// | `START` and nothing else | the process was **killed**; it never reached the end of `main` |
///
/// The third state is the point. On a machine where several clients run at once, an external
/// process kill and a self-inflicted
/// failure are **indistinguishable from the exit code alone** -- both are 1 -- and only a record
/// written by the process itself can tell them apart.
mod crashlog {
    use std::io::Write as _;
    use std::path::PathBuf;

    /// `dereth-client-<pid>.log` in the settings directory's `crash-logs` folder
    /// (`dereth_client::config::crash_log_dir`): beside the player's other files, and per-pid so
    /// that concurrent clients cannot interleave into one file.
    #[must_use]
    pub fn path() -> PathBuf {
        dereth_client::config::crash_log_dir()
            .join(format!("dereth-client-{}.log", std::process::id()))
    }

    /// Seconds since the Unix epoch, so a record can be lined up against a harness transcript.
    fn stamp() -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64())
    }

    /// Append one record. This never fails the run and never panics: a diagnostic that can kill
    /// the process it is diagnosing is worse than no diagnostic at all.
    pub fn append(kind: &str, body: &str) {
        let p = path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&p)
        {
            let _ = writeln!(f, "[{kind}] unix={:.3} pid={}", stamp(), std::process::id());
            let _ = writeln!(f, "{body}");
            let _ = f.flush();
        }
    }

    /// Record the start of the run and install the panic hook. Call this first thing in `main`.
    pub fn install() {
        let argv: Vec<String> = std::env::args().collect();
        // The file names itself, so a reader who has the log does not need the launch transcript.
        // **This is where the path goes, and it is deliberately not on stderr.** The path embeds
        // the pid, and printing it unconditionally would break `headless_capture`'s three-run
        // stderr comparison: the pid is the one thing that cannot repeat, and stderr is the
        // channel a determinism test reads.
        append(
            "START",
            &format!("log = {}\nargv = {argv:?}", path().display()),
        );
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // `force_capture` ignores `RUST_BACKTRACE`. The moment an intermittent fault finally
            // fires is not the moment to discover the variable was unset.
            let backtrace = std::backtrace::Backtrace::force_capture();
            append(
                "PANIC",
                &format!(
                    "{info}
{backtrace}"
                ),
            );
            previous(info);
        }));
        // The crash log's calibration hook, and the reason it lives in production rather than in a
        // test: a zero from an instrument is worth nothing
        // until that instrument has produced a known-positive, and a panic hook is by definition
        // never exercised by a run that goes well. The hidden `--crash-test` switch fires one on
        // demand, so anyone can prove in one command that the hook still records a backtrace
        // before reporting that no crash occurred.
        if dereth_client::config::crash_test_in_argv(argv.get(1..).unwrap_or_default()) {
            panic!("--crash-test -- a deliberate panic, to prove the hook writes a backtrace");
        }
    }

    /// The last record of any run that reaches the end of `main`.
    pub fn finished(code: i32, detail: &str) {
        append("EXIT", &format!("code={code} {detail}"));
    }
}

fn main() -> std::process::ExitCode {
    // Before *everything*, including the crash log's own `START` record and the stderr line below:
    // `std` resolves the standard handles when it writes, so the console has to exist by the first
    // write or that write is lost. `--no-console` is read straight out of `argv` here rather than
    // taken off the parsed `Config`, because the parse comes later and reports its failures on the
    // stderr this call is what provides. See `dereth_client::config::no_console_in_argv`.
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if dereth_client::config::no_console_in_argv(&argv) {
        // Nothing to do, and that is the whole feature: no console is attached, none is created,
        // and the process's streams stay as the windows subsystem left them.
    } else {
        let _ = dereth_console::attach();
    }
    // Before anything else, so that a failure inside argument parsing is recorded too.
    crashlog::install();
    match run() {
        Ok(()) => {
            crashlog::finished(0, "ran to the end of main");
            std::process::ExitCode::SUCCESS
        }
        Err(message) => {
            // The final error display is a modal dialog, after which the
            // process ends. Without a UI framework the honest equivalent is the same text on stderr
            // and a non-zero exit. It is the dialog's stand-in rather than a log line, and a parse
            // failure reaches it before the log is installed, so it is written directly.
            eprintln!("{}: {message}", corestrings::CAPTION_ERROR);
            crashlog::finished(1, &format!("{}: {message}", corestrings::CAPTION_ERROR));
            std::process::ExitCode::FAILURE
        }
    }
}

/// `WinMain` steps 9 to 12: parse, initialize the client, run it, and clean up.
fn run() -> Result<(), String> {
    // Step 9: parse the command line. On failure -> corestrings 205 -> exit.
    let mut cfg = Config::from_args_and_prefs().map_err(|e| e.to_string())?;
    // The retail dats, before anything else is started: `--dat-dir`, else the working directory,
    // else the executable's directory. A run that has none says where it looked and how to say
    // where they are. The install is then read-only for the run: a data-patch message that would
    // save into it is refused.
    if !dereth_dat::holds_retail_dats(&cfg.dat_dir) {
        let mut searched = dereth_client::config::dat_dir_candidates();
        if !searched.contains(&cfg.dat_dir) {
            searched = vec![cfg.dat_dir.clone()];
        }
        let e = dereth_dat::locate_retail_dats(&searched)
            .err()
            .map_or_else(String::new, |e| e.to_string());
        return Err(format!(
            "{} ({e}; pass --dat-dir <dir> naming the folder that holds them)",
            corestrings::display_string(corestrings::ID_CANT_OPEN_DATA_FILES, &[])
        ));
    }
    dereth_dat::protect_install(&cfg.dat_dir);
    // `Config::from_args_and_prefs` records either the parsed
    // `-prefs` path or the default it actually loaded, so Rust startup reaches this guard with one
    // coherent load/save destination. Keep the guard for a future alternate configuration source
    // that may deliberately leave the path empty; hand-built `Config`s used by embedded callers
    // still default to empty and therefore do not acquire a write target. So does a `--headless`
    // run that named no file (`Config::preferences_named`): it keeps out of the player's settings
    // folder, so it also skips the carry-over and the folder's creation below.
    if cfg.preferences_file.as_os_str().is_empty() && !cfg.headless {
        cfg.preferences_file = dereth_client::config::default_preferences_file();
    }
    // **The one-time carry-over, before the directory is created**, because "the new directory
    // does not exist yet" is exactly what makes this the first run. Copies, never moves -- the
    // source is the retail client's own directory. See `config::migrate_settings_dir`. Its
    // outcome is logged below, once the log (which may write into that directory) is installed.
    #[cfg(windows)]
    let migrated = {
        let to = dereth_client::config::default_settings_dir();
        // **One equality covers all three conditions**, because `default_preferences_file()` is
        // the whole default rule: it answers the *cwd* file when there is one, so a path equal to
        // the settings directory's means `-prefs` was not given, there is no `UserPreferences.ini`
        // beside the binary, and this run is about to load from the settings directory. Compare
        // against the cwd-probing function instead and a portable install would migrate too.
        let from_settings_dir =
            cfg.preferences_file == to.join(dereth_client::config::PREFERENCES_FILE_NAME);
        dereth_client::config::legacy_settings_dir()
            .filter(|_| from_settings_dir)
            .map(|legacy| {
                let outcome = dereth_client::config::migrate_settings_dir(&legacy, &to);
                (legacy, to, outcome)
            })
    };
    // The settings directory is the installer's job in retail and there is no installer here, so
    // the binary makes it. This is the *only* place it is made: `App` must not, and
    // `physical_window_resize_reaches_the_backbuffer_and_ui_without_changing_preferences` names a
    // directory that does not exist precisely to prove that nothing along that path creates one.
    //
    // It matters on every platform. `%USERPROFILE%\Documents` always exists and only the leaf was
    // ever missing, and the leaf is `Dereth`, which no install has yet; `~/.config/dereth` and
    // `~/Library/Application Support/dereth` do not exist at all until something makes them.
    // Without this, every preference, keymap and screen layout the player saved would fail to
    // write with `NotFound` and say nothing -- the same silent-save failure as the original.
    //
    // A failure is ignored for the same reason startup ignores preference initialization errors: a client
    // that cannot write settings must still run. The first save then fails as it did before.
    if let Some(dir) = cfg.preferences_file.parent() {
        if !dir.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(dir);
        }
    }
    logging::install(&cfg);
    // The crash log's path is not logged: it holds the pid, and stderr stays byte-identical
    // across runs. The pid is on every line of the file itself, which makes each run's record
    // independently identifiable.
    #[cfg(windows)]
    if let Some((legacy, to, outcome)) = migrated {
        match outcome {
            Ok(0) => {}
            Ok(n) => tracing::info!(
                "first run -- copied {n} file(s) from {} to {}",
                legacy.display(),
                to.display()
            ),
            // Ignored for the same reason the create above is: a client that could not carry
            // the old settings over must still start, with the defaults it would have had.
            Err(e) => tracing::warn!(
                "could not carry settings over from {}: {e}",
                legacy.display()
            ),
        }
    }
    run_with(cfg)
}

/// The client's log: the one subscriber for every crate's `tracing` events (and, bridged, the
/// `log` records of the dependencies that use that facade instead).
///
/// Configured by [`Config`] alone -- `--log <filter>` or `[Log] Level=`, `--log-file` or
/// `[Log] File=`, and `--log-spans` -- and never by the environment. The default is `info`. The
/// live diagnostic traces are the `dereth::trace::net`, `::camera`, `::raise` and `::notice`
/// targets at `debug`: `--log info,dereth::trace=debug` shows all four.
///
/// * **stderr**, always: one compact line per event, `LEVEL target: message`, with no timestamp,
///   so that two runs of the same deterministic scene write the same bytes (the headless capture's
///   three-run comparison reads this stream).
/// * **`dereth-client.log`** beside the preferences file, with `--log-file`: the same lines with a
///   UTC timestamp, appended, so a run started from the launcher with no console still leaves a
///   record. Several clients may share it, so each run starts with a line naming its process.
mod logging {
    use dereth_client::config::Config;
    use tracing_subscriber::filter::{LevelFilter, Targets};
    use tracing_subscriber::fmt::format::FmtSpan;
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::util::SubscriberInitExt as _;

    /// The log file's name, in the directory the preferences file is in.
    pub const FILE_NAME: &str = "dereth-client.log";

    /// Install the log for `cfg`. Called once, before anything that logs.
    pub fn install(cfg: &Config) {
        let (filter, refused) = match cfg.log_filter.as_deref().map(str::parse::<Targets>) {
            None => (Targets::new().with_default(LevelFilter::INFO), None),
            Some(Ok(t)) => (t, None),
            Some(Err(e)) => (Targets::new().with_default(LevelFilter::INFO), Some(e)),
        };
        let spans = if cfg.log_spans {
            FmtSpan::CLOSE
        } else {
            FmtSpan::NONE
        };
        let stderr = tracing_subscriber::fmt::layer()
            .compact()
            .without_time()
            .with_span_events(spans.clone())
            .with_writer(std::io::stderr);
        let path = cfg
            .preferences_file
            .parent()
            .map_or_else(|| FILE_NAME.into(), |d| d.join(FILE_NAME));
        let (file, file_error) = if cfg.log_file {
            match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                Ok(f) => (
                    Some(
                        tracing_subscriber::fmt::layer()
                            .compact()
                            .with_span_events(spans)
                            .with_writer(std::sync::Mutex::new(f)),
                    ),
                    None,
                ),
                Err(e) => (None, Some(e)),
            }
        } else {
            (None, None)
        };
        let has_file = file.is_some();
        // `try_init` also installs the bridge that turns `log` records into events. It fails only
        // if a subscriber is already installed, which in this binary nothing else does.
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(stderr)
            .with(file)
            .try_init();
        if let Some(e) = refused {
            tracing::warn!(
                "the log filter {:?} is not one this client reads ({e}); logging at info",
                cfg.log_filter.as_deref().unwrap_or_default()
            );
        }
        if let Some(e) = file_error {
            tracing::warn!("the log file {} would not open: {e}", path.display());
        }
        if has_file {
            tracing::info!(
                "log file {}, process {}",
                path.display(),
                std::process::id()
            );
        }
    }
}

/// A `StartupError` on its way to `main`'s stderr sentence, with its cause logged first.
///
/// `StartupError::Device`'s text is corestrings 127, which only reports a fatal Windows API issue.
/// The variant also carries the underlying failure for this log line, so a start-up that dies on,
/// say, a missing Vulkan loader says so instead of asking the player to reboot.
#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
fn fatal(e: dereth_client::app::StartupError) -> String {
    if let Some(cause) = e.cause() {
        tracing::error!("{cause}");
    }
    e.to_string()
}

#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
fn run_with(cfg: Config) -> Result<(), String> {
    let capture = cfg.capture.clone();
    let headless = cfg.headless;
    let mut scene = cfg.scene_config();
    let want_world = cfg.world;
    let terrain_blending = cfg.terrain_blending;
    let cfg_connect = cfg.will_connect();
    let want_ui = cfg.ui;
    let cfg_ui_mode = cfg.ui_mode;
    let host = cfg.host.clone();
    let account = cfg.account.clone();

    // Step 10: initialize the client application and its presentation.
    let mut app = dereth_client::app::App::new(cfg).map_err(fatal)?;
    tracing::info!(
        "{} device, {}x{}",
        app.renderer_mut().adapter(),
        app.renderer_mut().size().0,
        app.renderer_mut().size().1
    );
    // On a software rasteriser the compute shader runs on the CPU too, and far more slowly than
    // the CPU compositor it would replace (a minute against six seconds for one Extreme-distance
    // load on WARP). The two are bit-identical, so the CPU one is taken instead.
    if app.renderer_mut().software() && scene.gpu_terrain_merge {
        scene.gpu_terrain_merge = false;
        if terrain_blending == dereth_client::render_prefs::TerrainBlending::Gpu {
            tracing::info!(
                "TerrainBlending=gpu on a software device -- composing on the CPU, which is faster there and identical"
            );
        }
    }

    // Initialize the keymap, sound and UI. The keymap and sound come up whatever the switches
    // say, because the pump has somewhere to send a `MSG` and the device is the thing being
    // proved; the screens come up with `--ui`.
    app.start_shell().map_err(fatal)?;

    if want_world && cfg_connect {
        // With a server the block to build the landscape around is the one the *server*
        // puts the player in, and nothing knows that until the player's `0xF745` arrives. So the
        // scene is deferred until world entry supplies a position and the landscape can load
        // landblocks. `--landblock` still chooses the scenery radius and the rest of the config.
        app.defer_static_scene(scene);
        tracing::info!("the landscape waits for the server to place the player");
    } else if want_world {
        // A missing region or landblock is fatal for the same reason a missing portal dat
        // is: there is nothing to draw and nothing else this build does.
        app.load_static_scene(scene).map_err(fatal)?;
        let descriptors = (
            app.renderer().descriptor_usage(),
            app.renderer().descriptor_stats(),
        );
        if let Some(world) = app.world_scene() {
            let s = world.draw.stats;
            tracing::info!(
                "landblock 0x{:04X}, {} blocks, {} terrain surfaces",
                scene.landblock,
                s.blocks_meshed,
                s.terrain_surfaces
            );
            tracing::debug!(
                "{} scenery + {} buildings + {} statics, {} batches ({} untextured), \
                 {} triangles, {} KiB of dynamic upload per frame",
                s.scenery_objects,
                s.buildings,
                s.static_objects,
                s.object_batches,
                s.object_batches_untextured,
                s.object_triangles,
                s.upload_bytes / 1024
            );
            // "The sky is not black" is a claim about a number, so it is printed.
            let (year, day, t) = world.game_time();
            let l = world.landscape_lighting();
            let sky = world.draw.stats.sky_stats;
            tracing::debug!(
                "year {year} day {day}, time of day {t:.3}; sky {}/{} gfx ids drawable, {} live ({} pass 0, {} pass 1), {} batches, {} triangles, {} missing",
                sky.gfx_ids_drawable,
                sky.gfx_ids,
                sky.live_objects,
                sky.pass0_objects,
                sky.pass1_objects,
                sky.batches,
                sky.triangles,
                sky.missing_geometry
            );
            // The descriptor allocator reuses freed slots, so what is worth printing is the
            // allocator's own view. `live` is what is held right now, `high_water` the largest
            // `live` ever reached, and `frontier` how much fresh heap space was ever taken --
            // `frontier` well below `high_water + free` is the reuse working.
            let (d, ds) = descriptors;
            tracing::debug!(
                "{} object/sky texture(s) + {} merged land surfaces; descriptors live {} / high water {} / frontier {} of {} (free {}, pending {}), {} reuses, {} exhaustions",
                s.textures_uploaded,
                s.terrain_surfaces,
                d.live,
                d.high_water,
                d.frontier,
                d.capacity,
                d.free,
                d.pending,
                ds.reuses,
                ds.exhaustions
            );
            tracing::debug!(
                "ambient {:.3} {:?}, sunlight {:?} (|v| = dir_bright = {:.3}) {:?}",
                l.ambient_level,
                l.ambient_color,
                l.sunlight,
                l.sunlight.magnitude(),
                l.sunlight_color
            );
            let (cells, inside) = world.env_cell_counts();
            tracing::debug!(
                "{cells} interior cell(s) baked for drawing, {inside} batch(es) in \
                 the viewer's own cell"
            );
            if let Some(c) = world.character.as_ref() {
                tracing::debug!(
                    "{} interior cell(s) resident for physics ({:?})",
                    c.land().resident_cells(),
                    c.land().cell_stats()
                );
                tracing::debug!(
                    "character at {:?} in cell {:#010X}, {} drawable parts, \
                     {} batches, {} triangles",
                    c.position().frame.origin,
                    c.position().cell.0,
                    s.character_parts,
                    s.character_batches,
                    s.character_triangles
                );
            }
        }
    } else {
        // The full-screen quad, kept reachable so its regression still runs.
        app.load_first_pixel_scene().map_err(fatal)?;
        tracing::info!("surface 0x{:08X}", dereth_client::gpu::FIRST_PIXEL_SURFACE);
    }

    if cfg_connect {
        // The connection step keeps the UI alive until the link reaches Connected; `App::run`
        // owns that frame loop. This branch therefore only reports which server was selected.
        tracing::info!("connecting to {host} as {account}");
    }

    if let Some(m) = cfg_ui_mode {
        app.queue_ui_mode(dereth_ui::UiMode(m));
        tracing::info!("--ui-mode queues {m:#010X}");
    }

    // The one sound that proves the device path end to end: the UI button-press sound out of the UI
    // sound table, played centred on the listener. It is played before the loop rather than from a
    // button because a run without the UI has no buttons to press.
    app.play_startup_sound();

    // Step 11: run the client frame loop.
    let frames = app.run();
    tracing::info!("{frames} frame(s) drawn");

    if cfg_connect {
        // What the server actually put in the world, and what was drawn.
        let s = app.objects().stats;
        tracing::info!(
            "objects -- {} created, {} merged, {} recreated, {} removed, \
             {} stale instances; {} position updates ({} stale), {} movement buffers \
             ({} undecodable)",
            s.creates,
            s.merges,
            s.recreates,
            s.removes,
            s.stale_instances,
            s.position_updates,
            s.stale_positions,
            s.movement_updates,
            s.movement_undecodable
        );
        if let Some(world) = app.world_scene() {
            let w = world.draw.stats;
            tracing::info!(
                "{} object(s) drawn from {} setup(s), {} animated, {} batches, \
                 {} triangles",
                w.server_objects,
                w.server_object_setups,
                w.server_objects_animated,
                w.server_object_batches,
                w.server_object_triangles
            );
        }
    }

    // The shell's report line: what it did, in the numbers the tests assert on.
    if let Some(shell) = app.ui() {
        let s = shell.stats;
        tracing::info!(
            "UI -- {} mode switch(es) {:?}, {} create failure(s), \
             {} unregistered request(s), {} request(s) handled, {} unowned",
            s.mode_switches,
            shell.transitions(),
            s.screen_create_failures,
            s.unregistered_mode_requests,
            s.requests_handled,
            s.requests_ignored
        );
    }
    // The HUD's report line: what it read out of the server and what it did with it.
    {
        let h = app.hud();
        let s = h.stats;
        tracing::info!(
            "HUD -- {} player description(s), {} placement row(s) decoded, \
             {} window visibility/ies applied, {} vital update(s) ({} unstorable), \
             {} chat line(s) shown ({} filtered out), {} undecodable; \
             {} vitals / {} toolbar / {} radar write(s), coords {:?}",
            s.player_desc_applied,
            s.placements_decoded,
            s.placements_applied,
            s.vital_updates,
            s.vital_updates_unstorable,
            s.chat_lines,
            s.chat_lines_dropped,
            s.undecodable,
            s.vitals_written,
            s.toolbar_written,
            s.radar_written,
            h.coords
        );
        for (id, row) in &h.placements.rows {
            tracing::debug!("HUD placement window {id}: {row:?}");
        }
        // The housing subsystem's whole round trip in one line. If the client never sends
        // `0x021E House_QueryHouse`, the shard never sends either answer, and every counter below
        // stays 0 exactly like a shard with nothing to say. A live run that ends with
        // `0 status / 0 data` means the *request* did not go out, which is a different bug from an
        // unreceived answer.
        tracing::info!(
            "house -- {} status / {} data answer(s) to the login QueryHouse, \
             {} rent-time ({} applied) / {} rent-payment ({} applied) update(s), \
             {} restriction update(s) ({} applied)",
            s.house_status_notices,
            s.house_data_notices,
            s.house_rent_time_updates,
            s.house_rent_time_applied,
            s.house_rent_payment_updates,
            s.house_rent_payment_applied,
            s.house_restriction_updates,
            s.house_restrictions_applied
        );
        // The slumlord window's own round trip, beside the login one, because the
        // two are about different houses and fail in different ways. A `0 profile(s)` after using
        // a slumlord means either the use never went out or the shard refused it; a non-zero
        // profile count with `0 opened` would mean the receiver ran and the panel did not.
        tracing::info!(
            "slumlord -- {} house profile(s) received, {} window open(s), \
             {} payment request(s), {} lord re-quer(ies)",
            s.house_profile_notices,
            app.hud().panels.slumlord.opens,
            app.interaction().stats.house_payments_sent,
            app.interaction().stats.house_lord_queries
        );
    }
    if want_ui {
        let t = app.renderer_mut().ui_stats;
        tracing::info!(
            "UI draw -- {} quad(s), {} image(s) uploaded, {} decode failure(s), \
             {} skipped, {} clipped away",
            t.quads_drawn,
            t.uploaded,
            t.decode_failures,
            t.skipped_draws,
            t.clipped_away
        );
        // Every UI image is one SRV slot, and a screen gives its slots back when `use_new_mode` destroys it: `freed` is what the screens released
        // and `unknown` is a double release, which must be zero. The gameplay screen alone is 114
        // images, so this is the number that says whether the heap is bounded or merely large.
        let d = app.renderer().descriptor_usage();
        let ds = app.renderer().descriptor_stats();
        let r = app.ui_release_report();
        tracing::info!(
            "UI textures hold {} descriptor slot(s); heap live {} / high water {} / frontier {} of {} (free {}), {} reuse(s), {} exhaustion(s); screens released {} (still linked {}, unknown {})",
            app.renderer().ui_texture_count(),
            d.live,
            d.high_water,
            d.frontier,
            d.capacity,
            d.free,
            ds.reuses,
            ds.exhaustions,
            r.freed,
            r.still_linked,
            r.unknown
        );
    }
    {
        let (offered, handled, no_scan, actions) =
            app.input_manager_mut().map_or((0, 0, 0, 0), |i| {
                (
                    i.stats.messages_offered,
                    i.stats.messages_handled,
                    i.stats.keyboard_without_scan_code,
                    i.stats.actions_fired,
                )
            });
        tracing::info!(
            "input -- {offered} message(s) offered, {handled} handled, \
             {no_scan} without a scan code, {actions} action(s)"
        );
    }
    if let Some(audio) = app.audio_mut() {
        tracing::info!(
            "audio -- device {}, {} wave(s) created, {} decode failure(s), \
             {} sound(s) started, {} voice(s) still playing, {} under-run(s), \
             {} block(s) submitted, {} movie track(s), peak {:.4}",
            if audio.has_device() { "up" } else { "silent" },
            audio.stats.waves_created,
            audio.stats.wave_decode_failures,
            audio.stats.sounds_started,
            audio.active_voices(),
            audio
                .stats
                .underruns
                .load(std::sync::atomic::Ordering::Relaxed),
            audio
                .stats
                .blocks_filled
                .load(std::sync::atomic::Ordering::Relaxed),
            audio.stats.movie_tracks_started,
            audio.peak_output()
        );
    }

    if let Some(path) = capture {
        if !headless {
            tracing::warn!("--capture reads the offscreen target and needs --headless");
        }
        app.renderer_mut()
            .capture_png(&path)
            .map_err(|e| format!("capture: {e}"))?;
        tracing::info!("wrote {}", path.display());
    }

    // Step 12: clean up the client, in the documented order; the log is printed rather
    // than dropped so a windowed run says what it tore down and in what order.
    let log = app.shutdown();
    for (step, outcome) in &log.0 {
        tracing::debug!("cleanup {step:?}: {outcome:?}");
    }
    Ok(())
}

#[cfg(not(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12"))))]
fn run_with(_cfg: Config) -> Result<(), String> {
    Err("dereth-client needs a graphics backend: build with --features vulkan or d3d12".into())
}
