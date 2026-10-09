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

use dereth_client::Dereth;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::corestrings;
use dereth_desktop::crashlog;

mod input_replay;

fn main() -> std::process::ExitCode {
    // Before *everything*, including the crash log's own `START` record and the stderr line below:
    // `std` resolves the standard handles when it writes, so the console has to exist by the first
    // write or that write is lost. `--no-console` is read straight out of `argv` here rather than
    // taken off the parsed `Config`, because the parse comes later and reports its failures on the
    // stderr this call is what provides. See `dereth_client_runtime::config::no_console_in_argv`.
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if dereth_client_runtime::config::no_console_in_argv(&argv) {
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
    // parse the command line, make the settings folder and install the log.
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    let replay = input_replay::Replay::load(&mut argv)?;
    let cfg = dereth_desktop::start::<Dereth>(&argv)?;
    run_with(cfg, replay)
}

/// A `StartupError` on its way to `main`'s stderr sentence, with its cause logged first.
///
/// `StartupError::Device`'s text is corestrings 127, which only reports a fatal Windows API issue.
/// The variant also carries the underlying failure for this log line, so a start-up that dies on,
/// say, a missing Vulkan loader says so instead of asking the player to reboot.
#[cfg(gpu)]
fn fatal(e: dereth_client_runtime::app::StartupError) -> String {
    if let Some(cause) = e.cause() {
        tracing::error!("{cause}");
    }
    e.to_string()
}

#[cfg(gpu)]
fn run_with(cfg: Config, replay: Option<input_replay::Replay>) -> Result<(), String> {
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

    // initialize the client application and its presentation.
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
        if terrain_blending == dereth_client_runtime::render_prefs::TerrainBlending::Gpu {
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
        app.log_load_report(scene);
    } else {
        // The full-screen quad, kept reachable so its regression still runs.
        app.load_first_pixel_scene().map_err(fatal)?;
        tracing::info!(
            "surface 0x{:08X}",
            dereth_client_runtime::assets::FIRST_PIXEL_SURFACE
        );
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

    // run the client frame loop.
    let frames = if let Some(mut replay) = replay {
        app.state = dereth_client_runtime::app::AppState::Running;
        let mut frame = 1;
        loop {
            replay.drain_frame(frame, |event| app.queue_window_event(event));
            if let Some(pad) = replay.drain_pad(frame) {
                app.set_scripted_pad(pad);
            }
            if !app.frame() {
                break;
            }
            frame += 1;
        }
        app.state = dereth_client_runtime::app::AppState::ShuttingDown;
        app.frames_drawn()
    } else {
        app.run()
    };
    app.log_run_report(frames, cfg_connect, want_ui);

    if let Some(path) = capture {
        if !headless {
            tracing::warn!("--capture reads the offscreen target and needs --headless");
        }
        app.renderer_mut()
            .capture_png(&path)
            .map_err(|e| format!("capture: {e}"))?;
        tracing::info!("wrote {}", path.display());
    }

    // clean up the client, in the documented order; the log is printed rather
    // than dropped so a windowed run says what it tore down and in what order.
    let log = app.shutdown();
    for (step, outcome) in &log.0 {
        tracing::debug!("cleanup {step:?}: {outcome:?}");
    }
    Ok(())
}

#[cfg(not(gpu))]
fn run_with(_cfg: Config, _replay: Option<input_replay::Replay>) -> Result<(), String> {
    Err(
        "dereth-client needs a graphics backend: build with --features vulkan, d3d12 or wgpu"
            .into(),
    )
}
