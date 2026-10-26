//! `lyceum-editor`, Lyceum: a world's packs over the player's data files, edited in place.
//!
//! ```text
//! lyceum-editor lyceum/sample/world.toml --dats <your client folder>
//! lyceum-editor <world.toml> --headless --script edits.toml --frames 60 --capture-at 60:after.png
//! lyceum-editor --write-text
//! ```
//!
//! The command line is [`lyceum_editor::args`]'s. A windowed run edits until its window closes and
//! saves when told to (the Save button, Ctrl+S); a `--headless` run draws off screen, runs its
//! script and captures, and saves only when its script says so. Started with no world, as a
//! desktop starts it, it opens its start screen ([`start`]).
//!
//! It is a windowed program: a desktop shows it no console. Started from a terminal, it borrows
//! that terminal and writes its log there; its log also goes to `lyceum-editor.log` in its settings
//! folder.
//!
//! In the window: the right button held turns the camera; W, A, S and D fly (or walk), E and Q
//! fly up and down (or step aside), Space rises (or, held and let go, jumps), Shift lowers (or
//! walks the body), and the wheel changes the speed, the body's too; G puts the camera on the
//! ground and F walks with a body or flies again. 1 to 4 choose Select, Place,
//! Terrain and Spawn; Delete removes the selected static or spawn, Escape lets it go; Ctrl+Z and
//! Ctrl+Y undo and redo, Ctrl+S saves.
#![windows_subsystem = "windows"]

use dereth_desktop::crashlog;
use lyceum_editor::Lyceum;

#[cfg_attr(not(gpu), allow(dead_code))]
mod start;

fn main() -> std::process::ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // First, before anything is written: the terminal the editor was started from, if any.
    let console = dereth_console::attach().is_open();
    // `--version` and `--list-recent` alone: nothing started, and no window.
    if argv == ["--version"] {
        print!("{}", start::version_text());
        return std::process::ExitCode::SUCCESS;
    }
    // `--write-text`: the built-in text written out as a file to change, and nothing started.
    if let Some(w) = lyceum_editor::args::write_text(&argv) {
        let written = w.and_then(|w| {
            let to = w
                .to
                .or_else(|| start::settings_dir().map(|d| d.join(lyceum_editor::text::FILE_NAME)))
                .ok_or("no settings folder: name the file")?;
            lyceum_editor::text::write_built_in(&to, w.overwrite)?;
            Ok(to)
        });
        return match written {
            Ok(to) => {
                println!("{}", to.display());
                std::process::ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("lyceum-editor: {e}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    if let Some(start::NoWorld::List { dats }) = start::no_world(&argv) {
        print!("{}", start::list(dats));
        return std::process::ExitCode::SUCCESS;
    }
    crashlog::install::<Lyceum>();
    match run(&argv) {
        Ok(()) => {
            crashlog::finished::<Lyceum>(0, "ran to the end of main");
            std::process::ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("lyceum-editor: {message}");
            let screen = matches!(start::no_world(&argv), Some(start::NoWorld::Screen { .. }));
            if start::error_box_wanted(console, screen) {
                start::error_box(&message);
            }
            crashlog::finished::<Lyceum>(1, &message);
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(gpu)]
fn run(argv: &[String]) -> Result<(), String> {
    // No world named: the start screen, which starts the editor again on the world chosen.
    if let Some(start::NoWorld::Screen { dats }) = start::no_world(argv) {
        return start::screen(dats);
    }
    let mut args = lyceum_editor::args::parse(argv)
        .map_err(|e| format!("{e}\n{}", lyceum_editor::args::USAGE))?;
    lyceum_editor::programs::fill_empyrean(&mut args.empyrean);
    // Where the world was left, when the command line names no landblock to start at.
    let start_at = args
        .landblock
        .or_else(|| lyceum_editor::place::read(&args.world).and_then(|l| l.landblock()))
        .unwrap_or(0xA9B4);
    let mut runtime = args.runtime_argv(start_at);
    if !args.headless {
        runtime.push("--log-file".to_owned());
    }
    let (cfg, _log) = dereth_desktop::start::<Lyceum>(&runtime)?;
    if !args.headless {
        start::remember(&args.world, &cfg.dat_dir);
    }
    let mut editor = lyceum_editor::app::EditorApp::open(&args, cfg)?;
    if args.headless {
        let pack = editor.session.packs()[editor.session.target()].name.clone();
        let steps = match &args.script {
            Some(p) => {
                let text =
                    std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
                lyceum_editor::script::steps(&text, &pack)
                    .map_err(|e| format!("{}: {e}", p.display()))?
            }
            None => Vec::new(),
        };
        let last = steps.last().map_or(0, |s| s.frame);
        let frames = args
            .frames
            .unwrap_or_else(|| last.max(args.captures.iter().map(|c| c.0).max().unwrap_or(1)));
        editor.run_script(&steps, frames, &args.captures)?;
        let s = editor.stats;
        println!(
            "{} frame(s), {} edit(s), {} reload(s), {} landblock(s) built again, {} fault(s)",
            s.frames,
            s.edits,
            s.reloads,
            s.blocks_reread,
            editor.faults.len()
        );
    } else {
        editor.run();
        if editor.session.unsaved() {
            eprintln!("lyceum-editor: the window closed with edits that were not saved");
        }
    }
    Ok(())
}

#[cfg(not(gpu))]
fn run(_argv: &[String]) -> Result<(), String> {
    Err(
        "lyceum-editor needs a graphics backend: build with --features vulkan, d3d12 or wgpu"
            .into(),
    )
}
