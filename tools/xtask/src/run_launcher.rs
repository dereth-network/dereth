//! `cargo xtask run`: build the games' products and the launcher, then start the launcher, which
//! plays and hosts with the builds just made.
//!
//! ```text
//! cargo xtask run [--debug] [--game dereth|auberean|all] [-- LAUNCHER-ARGS]
//! ```
//!
//! 1. **The products**, as `cargo xtask build --game <game>` builds them, in release unless
//!    **`--debug`**: the clients, the servers, the builders and the tools, both games' by default.
//! 2. **The launcher**, a workspace of its own (`launcher/`), in the same profile.
//! 3. **The launcher started** from `launcher/target/<profile>/`. A launcher built in the tree plays
//!    the tree's own builds: the Dereth client from `target/release` (then `debug`), and the
//!    servers and builders by their newest build. With `--debug` the debug client is named to it
//!    (`DERETH_CLIENT`), so an older release build is not played instead. What follows `--` goes to
//!    the launcher (`-- --downloads` has it download the programs as a release does).

use std::path::{Path, PathBuf};

use crate::game::Game;
use crate::util::workspace_root;

/// The command's usage.
pub const USAGE: &str =
    "usage: cargo xtask run [--debug] [--game dereth|auberean|all] [-- LAUNCHER-ARGS]";

/// What `run ...` asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Options {
    debug: bool,
    game: Game,
    /// The arguments after `--`, for the launcher.
    launcher_args: Vec<String>,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let (ours, launcher_args) = match args.iter().position(|a| a == "--") {
        Some(i) => (&args[..i], args[i + 1..].to_vec()),
        None => (args, Vec::new()),
    };
    let game = Game::from_args(ours)?;
    let mut debug = false;
    for a in Game::without_flag(ours) {
        match a.as_str() {
            "--debug" => debug = true,
            other => return Err(format!("{other:?} is not an option of run\n{USAGE}")),
        }
    }
    Ok(Options {
        debug,
        game,
        launcher_args,
    })
}

/// The cargo profile folder `debug` builds into.
fn profile(debug: bool) -> &'static str {
    if debug {
        "debug"
    } else {
        "release"
    }
}

/// The launcher program built in `launcher` for `profile`.
fn launcher_program(launcher: &Path, profile: &str) -> PathBuf {
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| launcher.join("target"), PathBuf::from);
    target
        .join(profile)
        .join(format!("dereth{}", std::env::consts::EXE_SUFFIX))
}

/// `cargo xtask run ...`.
pub fn run(args: &[String]) -> i32 {
    let o = match parse(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let ws = workspace_root();
    let mut build = vec![crate::game::FLAG.to_owned(), o.game.name().to_owned()];
    if !o.debug {
        build.push("--release".to_owned());
    }
    println!("run: building the products ({})", build.join(" "));
    let code = crate::build(&build);
    if code != 0 {
        return code;
    }
    let launcher = ws.join("launcher");
    let mut argv = vec!["build"];
    if !o.debug {
        argv.push("--release");
    }
    println!("run: building the launcher");
    if !crate::util::run(&launcher, "cargo", &argv) {
        return 1;
    }
    let program = launcher_program(&launcher, profile(o.debug));
    let client = ws
        .join("target")
        .join(profile(o.debug))
        .join(format!("dereth-client{}", std::env::consts::EXE_SUFFIX));
    let mut cmd = std::process::Command::new(&program);
    cmd.args(&o.launcher_args).current_dir(&launcher);
    if o.debug && client.is_file() {
        cmd.env("DERETH_CLIENT", &client);
    }
    println!(
        "run: starting {} (it plays the tree's builds{})",
        program.display(),
        if o.debug && client.is_file() {
            format!(", the client {}", client.display())
        } else {
            String::new()
        }
    );
    match cmd.status() {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!("{}: {e}", program.display());
            1
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the developer's build-and-start command).
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    /// Release and both games by default; `--debug` and `--game` narrow it; what follows `--` is
    /// the launcher's own, and anything else is refused rather than ignored.
    #[test]
    fn run_builds_release_for_both_games_unless_told_and_passes_the_rest_to_the_launcher() {
        assert_eq!(
            parse(&[]).expect("reads"),
            Options {
                debug: false,
                game: Game::All,
                launcher_args: Vec::new()
            }
        );
        let o = parse(&args("--debug --game auberean -- --downloads")).expect("reads");
        assert!(o.debug);
        assert_eq!(o.game, Game::Auberean);
        assert_eq!(o.launcher_args, args("--downloads"));
        assert!(parse(&args("--release")).is_err());
        assert!(parse(&args("--game nope")).is_err());
        assert_eq!(profile(true), "debug");
        assert_eq!(profile(false), "release");
    }
}
