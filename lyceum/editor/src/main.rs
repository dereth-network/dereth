//! `lyceum-editor`, Lyceum: a world's packs over the player's data files, edited in place.
//!
//! ```text
//! lyceum-editor lyceum/sample/world.toml --dats <your client folder>
//! lyceum-editor <world.toml> --headless --script edits.toml --frames 60 --capture-at 60:after.png
//! ```
//!
//! The command line is [`lyceum_editor::args`]'s. A windowed run edits until its window closes and
//! saves when told to (the Save button, Ctrl+S); a `--headless` run draws off screen, runs its
//! script and captures, and saves only when its script says so. It is started from a terminal
//! with a world to open, and writes its log there.
//!
//! In the window: the right button held turns the camera; W, A, S and D fly (or walk), E and Q
//! fly up and down (or step aside), Space rises (or, held and let go, jumps), Shift lowers (or
//! walks the body), and the wheel changes the speed, the body's too; G puts the camera on the
//! ground and F walks with a body or flies again. 1 to 4 choose Select, Place,
//! Terrain and Spawn; Delete removes the selected static or spawn, Escape lets it go; Ctrl+Z and
//! Ctrl+Y undo and redo, Ctrl+S saves.

use dereth_desktop::crashlog;
use lyceum_editor::Lyceum;

fn main() -> std::process::ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    crashlog::install::<Lyceum>();
    match run(&argv) {
        Ok(()) => {
            crashlog::finished::<Lyceum>(0, "ran to the end of main");
            std::process::ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("lyceum-editor: {message}");
            crashlog::finished::<Lyceum>(1, &message);
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(gpu)]
fn run(argv: &[String]) -> Result<(), String> {
    let args = lyceum_editor::args::parse(argv)
        .map_err(|e| format!("{e}\n{}", lyceum_editor::args::USAGE))?;
    let runtime = args.runtime_argv(args.landblock.unwrap_or(0xA9B4));
    let cfg = dereth_desktop::start::<Lyceum>(&runtime)?;
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
