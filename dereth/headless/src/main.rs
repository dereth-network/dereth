//! `dereth-headless --script <file>` -- parse a script, run it against a headless client, print what
//! each command did.
//!
//! The argument parser is by hand and deliberately tiny: this binary has six switches and adding
//! a dependency to read them would be the larger change. `--script -` reads the script from stdin.
//!
//! **stdout is the script's transcript and nothing else.** The client's own progress -- the
//! runtime's log events, and the `log` records of the crates below it -- goes to stderr through the
//! subscriber [`install_log`] sets up, at `info` unless `--log` says otherwise.

use std::io::Read as _;
use std::path::PathBuf;
use std::process::ExitCode;

use dereth_headless::run::Options;

const USAGE: &str = "\
dereth-headless -- the dere client with no window and no graphics device

  dereth-headless --script <file>      run a script; `-` reads it from stdin

  --dat-dir <dir>      the retail dats (default: the working directory, else the
                       directory holding this program, whichever has client_portal.dat);
                       the files from before Throne of Destiny (portal.dat, cell.dat) may
                       be beside them
  --classic-dat-dir <dir>  where portal.dat and cell.dat are when they are not beside the
                       retail dats
  --era <name>         the era the world plays (eor, infiltration); by default it chooses
                       which set draws the world
  --world-base <set>   the set that draws the world, over the era's: modern or classic
  --captures <dir>     the recorded sessions `login` resolves a name in
  --size <w>x<h>       the null presentation's extent (default: 800x600)
  --account <name>     the name the replay endpoint is built with
  --world              build and drive the world with no device, so a body stands in it
                       and actions move it (world entry over a recording, or `world`)
  --log <filter>       the client's log on stderr: a level (error, warn, info, debug,
                       trace, off) or `level,target=level,...` (default: info)
  -h, --help           this text

Script: one command per line, `#` starts a comment.
  login <session> [<character>]   replay a recorded login, optionally into the world
  tick <n>                        run n frames
  hold <action> <secs>            begin an action, run secs of simulated time, end it
  press <action>                  begin, one frame, end, one frame
  begin <action> | end <action>   one edge of an action
  walk <direction> <secs>         hold forward/back/left/right/strafe-left/strafe-right
  use <object-id>                 the Use request for an object
  say <text>                      a chat line, and the lines the chat took
  dump events | dump steps        the last frame's event log, or only its steps
  snapshot                        a summary of the game view
  world <landblock>               load a world offline with a body (needs --world)
  position                        where the body stands
  quit                            stop and shut the client down
An <action> is a retail action name (MovementForward, CameraRotateLeft) or an id.";

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("dereth-headless: {e}");
            ExitCode::FAILURE
        }
    }
}

fn real_main() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut script: Option<String> = None;
    let mut log: Option<String> = None;
    let mut opts = Options::default();
    let mut dat_dir_given = false;
    while let Some(a) = args.next() {
        let mut value = |what: &str| -> Result<String, String> {
            args.next().ok_or_else(|| format!("{what} needs a value"))
        };
        match a.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(ExitCode::SUCCESS);
            }
            "--script" => script = Some(value("--script")?),
            "--dat-dir" => {
                opts.dat_dir = PathBuf::from(value("--dat-dir")?);
                dat_dir_given = true;
            }
            "--classic-dat-dir" => {
                opts.classic_dat_dir = Some(PathBuf::from(value("--classic-dat-dir")?));
            }
            "--world-base" => {
                let v = value("--world-base")?;
                opts.world_base = Some(match v.to_ascii_lowercase().as_str() {
                    "modern" => dereth_client_sdk::primitives::ContainerEra::Modern,
                    "classic" => dereth_client_sdk::primitives::ContainerEra::Classic,
                    _ => {
                        return Err(format!("unknown --world-base {v:?} (modern or classic)").into())
                    }
                });
            }
            "--era" => {
                let v = value("--era")?;
                opts.era = Some(
                    dereth_client_sdk::primitives::EraId::parse(&v)
                        .ok_or_else(|| format!("unknown --era {v:?}"))?,
                );
            }
            "--captures" => opts.captures_dir = PathBuf::from(value("--captures")?),
            "--account" => opts.account = value("--account")?,
            "--world" => opts.world = true,
            "--log" => log = Some(value("--log")?),
            "--size" => {
                let s = value("--size")?;
                let (w, h) = s
                    .split_once('x')
                    .ok_or_else(|| format!("--size {s}: want <w>x<h>"))?;
                opts.width = w.parse()?;
                opts.height = h.parse()?;
            }
            other => return Err(format!("unknown argument {other:?}\n\n{USAGE}").into()),
        }
    }
    let Some(script) = script else {
        eprintln!("{USAGE}");
        return Ok(ExitCode::FAILURE);
    };
    install_log(log.as_deref())?;
    let searched = if dat_dir_given {
        vec![opts.dat_dir.clone()]
    } else {
        dereth_client_sdk::runtime::config::dat_dir_candidates()
    };
    if let Err(e) = dereth_client_sdk::dat::locate_modern_dats(&searched) {
        return Err(format!("{e} (pass --dat-dir <dir>)").into());
    }
    // The install is read, never written: a data-patch message that would save into it is refused.
    dereth_client_sdk::dat::protect_install(&opts.dat_dir);
    if let Some(classic) = &opts.classic_dat_dir {
        dereth_client_sdk::dat::protect_install(classic);
    }

    let text = if script == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        s
    } else {
        std::fs::read_to_string(&script).map_err(|e| format!("{script}: {e}"))?
    };

    let commands = dereth_headless::parse(&text)?;
    let mut out = std::io::stdout().lock();
    dereth_headless::run(&commands, &opts, &mut out)?;
    Ok(ExitCode::SUCCESS)
}

/// The subscriber for the client's log: compact lines on stderr, `LEVEL target: message`, with no
/// timestamp so that a replayed script logs the same bytes every run. `filter` is `--log`'s value;
/// with none the level is `info`.
///
/// # Errors
/// A filter this binary cannot read, so a mistyped `--log` fails the run rather than silently
/// logging at a level nobody asked for.
fn install_log(filter: Option<&str>) -> Result<(), String> {
    use tracing_subscriber::filter::{LevelFilter, Targets};
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::util::SubscriberInitExt as _;

    let targets = match filter {
        None => Targets::new().with_default(LevelFilter::INFO),
        Some(f) => f
            .parse::<Targets>()
            .map_err(|e| format!("--log {f:?}: {e}"))?,
    };
    let stderr = tracing_subscriber::fmt::layer()
        .compact()
        .without_time()
        .with_writer(std::io::stderr);
    // Also installs the bridge for `log` records. It fails only when a subscriber is already
    // installed, which in this binary nothing else does.
    let _ = tracing_subscriber::registry()
        .with(targets)
        .with(stderr)
        .try_init();
    Ok(())
}
