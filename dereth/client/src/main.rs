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
//! gives back the one part of the console subsystem a developer wants: a run started from a
//! terminal borrows that terminal for its output. A run started from Explorer or the launcher has
//! no console to borrow and is shown none -- just the game's window. `--no-console` does not even
//! borrow one. A console is never created.
//!
//! One behaviour does change for a run started **by hand from a shell**: cmd and PowerShell do not
//! wait for a windows-subsystem process, so the prompt returns immediately and the client's output
//! arrives behind it. `cargo run`, and any launcher that waits on the process itself, are
//! unaffected.
#![windows_subsystem = "windows"]

use dereth_client::config::Config;
use dereth_client::corestrings;
use dereth_client::Dereth;
use dereth_desktop::crashlog;

fn main() -> std::process::ExitCode {
    // Before *everything*, including the crash log's own `START` record and the stderr line below:
    // `std` resolves the standard handles when it writes, so the console has to exist by the first
    // write or that write is lost. `--no-console` is read straight out of `argv` here rather than
    // taken off the parsed `Config`, because the parse comes later and reports its failures on the
    // stderr this call is what provides. See `dereth_client::config::no_console_in_argv`.
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if dereth_client::config::no_console_in_argv(&argv) {
        // Nothing to do, and that is the whole feature: no console is borrowed, and the process's
        // streams stay as the windows subsystem left them.
    } else {
        let _ = dereth_console::attach();
    }
    // `--version` alone: what this build is, and nothing started, written or read.
    if argv == ["--version"] {
        print!("{}", dereth_client::version_text());
        return std::process::ExitCode::SUCCESS;
    }
    // Before anything else, so that a failure inside argument parsing is recorded too.
    crashlog::install::<Dereth>();
    match run() {
        Ok(()) => {
            crashlog::finished::<Dereth>(0, "ran to the end of main");
            std::process::ExitCode::SUCCESS
        }
        Err(message) => {
            // The final error display is a modal dialog, after which the
            // process ends. Without a UI framework the honest equivalent is the same text on stderr
            // and a non-zero exit. It is the dialog's stand-in rather than a log line, and a parse
            // failure reaches it before the log is installed, so it is written directly.
            eprintln!("{}: {message}", corestrings::CAPTION_ERROR);
            crashlog::finished::<Dereth>(1, &format!("{}: {message}", corestrings::CAPTION_ERROR));
            std::process::ExitCode::FAILURE
        }
    }
}

/// `WinMain` steps 9 to 12: parse, initialize the client, run it, and clean up.
fn run() -> Result<(), String> {
    // Step 9: parse the command line, make the settings folder and install the log.
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let cfg = dereth_desktop::start::<Dereth>(&argv)?;
    run_with(cfg)
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
    {
        // The era the client plays and the systems it takes the world to lack (the server's
        // announcement over the era's table), and what the screens last took away for them.
        let h = app.hud();
        let lacks = |f: dereth_primitives::EraFeatures| {
            f.iter()
                .filter(|(_, on)| !on)
                .map(|(name, _)| name)
                .collect::<Vec<_>>()
                .join(",")
        };
        tracing::info!(
            "era -- {} ({}), {} system(s) announced; the world lacks [{}]; the screens hide [{}]",
            h.era.era,
            if h.era.era_announced {
                "announced"
            } else {
                "from the data files"
            },
            h.era.announced_features.iter().count(),
            lacks(h.era.features()),
            h.panels
                .era
                .applied()
                .map_or_else(|| "nothing yet".to_owned(), lacks)
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
        let w = audio.world_stats();
        tracing::info!(
            "world audio -- {} hook sound(s), {} unplayable; {} server sound(s), {} unplayable",
            w.triggers,
            w.trigger_misses,
            w.server_sounds,
            w.server_sound_misses
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
