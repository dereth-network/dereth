//! `empyrean-server`: the server binary, ACE's `Program.Main` ([`program::main`]).
//!
//! `empyrean-server [--config <empyrean.toml>] [--run-for <seconds>] [--status <ip>:<port>]`, or
//! `empyrean-server --write-config [<path>] [--from <Config.js>] [--out <path>]`, or
//! `empyrean-server --version` (the version, commit, build time, target and source), or
//! `empyrean-server update --check|--apply [--config <empyrean.toml>] [--policy <off|patch|minor>]`
//! (the updater, [`server_update`]), or `empyrean-server --release-facts` (the database schema
//! and world-pack versions this build uses, as JSON, for the updater). The configuration is
//! `empyrean.toml`, else the defaults (`empyrean_server::config_file`); ACE's `Config.js` is read only
//! by `--write-config --from`, which converts it once. Ctrl-C, or `exit` on the console, shuts down
//! gracefully (`ServerManager.DoShutdownNow`); a second Ctrl-C exits at once. No environment
//! variable configures the server: the dat folder is `server.dat_files_directory` only. The
//! configuration guide is `SETUP.md` beside the server's crates.

use std::process::ExitCode;

fn main() -> ExitCode {
    // `--version` alone: the version, commit, build time, target and source, before anything
    // else starts (no logger, no console, no configuration).
    if std::env::args().nth(1).as_deref() == Some("--version") {
        print!(
            "{}",
            empyrean_common::brand::version_text("empyrean-server")
        );
        return ExitCode::SUCCESS;
    }
    if std::env::args().nth(1).as_deref() == Some("--release-facts") {
        println!(
            "{}",
            empyrean_server::update::release::Facts::this_build().to_json()
        );
        return ExitCode::SUCCESS;
    }
    program::init_logging();
    if std::env::args().nth(1).as_deref() == Some("update") {
        return server_update::command(std::env::args().skip(2));
    }
    // The interactive prompt's terminal is given back however `main` ends.
    let _console = ConsoleGuard;
    match program::Options::parse(std::env::args().skip(1)) {
        Ok(options) => program::main(&options),
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// Stops the console's line editor when dropped (see `empyrean_command::console::stop`).
struct ConsoleGuard;

impl Drop for ConsoleGuard {
    fn drop(&mut self) {
        empyrean_command::console::stop();
    }
}

// @scaffold-mods begin
pub mod program;
pub mod program_bin_updates;
pub mod program_db_updates;
pub mod program_setup;
pub mod program_web_client;
pub mod server_build_info_dynamic;
pub mod server_build_info_static;
// @scaffold-mods end
mod server_update;
