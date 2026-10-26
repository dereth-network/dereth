//! `dereth-headless --script <file>` -- parse a script, run it against a headless client, print what
//! each command did. With `--daemon`, the script is a connection's setup and the client stays in
//! the world until it is stopped, connecting again whenever the connection is lost.
//!
//! The argument parser is by hand and deliberately tiny: adding a dependency to read the switches
//! would be the larger change. `--script -` reads the script from stdin.
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
  dereth-headless --daemon --connect <host:port> --script <file>
                                       stay in the world until stopped (Ctrl+C): the
                                       script sets each connection up, and a lost
                                       connection is made again

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
  --account <name>     the name the replay endpoint is built with, or the account
                       --connect logs on as
  --connect <host:port>  log on to a server (no recording); `enter` takes a character in
  --password-file <file>  the account's password, with --connect: the file's first line
                       (else the DERETH_PASSWORD environment variable; never on the
                       command line, where other programs can read it)
  --daemon             run until stopped, connecting again after a lost connection (a
                       server restarted): the script runs at each connection, then the
                       client runs on as `run forever`
  --log-file <file>    append a log to the file, each line with the time: the daemon's
                       connections, the chat, and each plugin's console lines
  --world              build and drive the world with no device, so a body stands in it
                       and actions move it (world entry over a recording, or `world`)
  --plugins <dir>      load the plugin package in <dir>, or every package in it (repeatable)
  --plugin-store <dir> keep plugin stores in <dir> (default: in memory)
  --plugin-data <dir>  the plugins' data folders, <dir>/<plugin id>/ (default: none)
  --plugin-policy <file>  the world's plugin policy document
  --plugin-budget <ms>  how long a plugin's entry may run before it is stopped (default:
                       the host's own); a run on a busy machine may raise it
  --peers <dir>        the plugins' bus between clients reaches the other headless clients
                       naming the same folder (loopback only)
  --nav-published <file>  the server's published portal table, for the client's journeys
  --log <filter>       the client's log on stderr: a level (error, warn, info, debug,
                       trace, off) or `level,target=level,...` (default: info)
  -h, --help           this text

Environment: DERETH_SETTINGS_DIR names the settings folder, as for the desktop client: the
preferences, and what the client's journeys learn for each world (portals used, portals that
refused it), are kept there between runs. Unset, the client writes neither.

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
  plugin load <dir or zip> | unload <id> | reload <id> | list | console <id>
  ui dump | ui layout | ui click <plugin> <window> <node> | ui set <plugin> <window> <node> <json>
  ui close <plugin> <window> | ui resize <plugin> <window> <width> <height> | ui bar <plugin>
                                  a plugin window's tree or its layout, or input on it (every
                                  window starts closed; ui bar opens a plugin's main window)
  chat                            the chat lines queued since the last `chat`, and the
                                  game's questions and pop-ups shown since
  answer yes | answer no          the player's answer to the oldest question on screen
  run <secs>                      frames at the wall clock's pace (for --connect)
  run forever                     the same until stopped (Ctrl+C) or the connection is lost
  enter <character> [mage]        enter the world as it (for --connect), created if missing
                                  (a soldier, or with mage a caster)
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
    let mut daemon = false;
    let mut log_file: Option<PathBuf> = None;
    let mut password_file: Option<PathBuf> = None;
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
            "--connect" => opts.connect = Some(value("--connect")?),
            "--password" => {
                return Err(
                    "--password is not taken: a password on the command line can be read \
                    by other programs. Use --password-file <file> or DERETH_PASSWORD"
                        .into(),
                )
            }
            "--password-file" => password_file = Some(PathBuf::from(value("--password-file")?)),
            "--daemon" => daemon = true,
            "--log-file" => log_file = Some(PathBuf::from(value("--log-file")?)),
            "--plugins" => opts.plugins.push(PathBuf::from(value("--plugins")?)),
            "--plugin-store" => opts.plugin_store = Some(PathBuf::from(value("--plugin-store")?)),
            "--plugin-data" => opts.plugin_data = Some(PathBuf::from(value("--plugin-data")?)),
            "--peers" => opts.peers = Some(PathBuf::from(value("--peers")?)),
            "--plugin-policy" => {
                let path = value("--plugin-policy")?;
                opts.plugin_policy = Some(
                    std::fs::read_to_string(&path)
                        .map_err(|e| format!("--plugin-policy {path}: {e}"))?,
                );
            }
            "--plugin-budget" => {
                let v = value("--plugin-budget")?;
                let ms: f64 = v
                    .parse()
                    .map_err(|_| format!("--plugin-budget {v}: want milliseconds"))?;
                opts.plugin_call_budget_ms = Some(ms);
            }
            "--log" => log = Some(value("--log")?),
            "--nav-published" => {
                opts.nav_published = Some(PathBuf::from(value("--nav-published")?));
            }
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
    install_log(log.as_deref(), log_file.as_deref())?;
    // A settings folder named, as for the desktop client: the preferences and what the journeys
    // learn are kept there between runs.
    if let Some(prefs) = dereth_headless::run::settings_preferences(
        std::env::var_os(dereth_headless::run::SETTINGS_DIR_ENV).as_deref(),
    ) {
        if let Some(dir) = prefs.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        opts.preferences_file = prefs;
    }
    if opts.connect.is_some() {
        opts.password = match &password_file {
            Some(path) => std::fs::read_to_string(path)
                .map_err(|e| format!("--password-file {}: {e}", path.display()))?
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned(),
            None => std::env::var("DERETH_PASSWORD").unwrap_or_default(),
        };
    }
    // Ctrl+C (and a termination request) stops the client at its next frame, which logs out.
    let stop = opts.stop.clone();
    if let Err(e) = ctrlc::set_handler(move || {
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }) {
        tracing::warn!("Ctrl+C will not stop the client cleanly: {e}");
    }
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
    if daemon {
        dereth_headless::daemon::daemon(&commands, &opts, &mut out)?;
    } else {
        dereth_headless::run(&commands, &opts, &mut out)?;
    }
    Ok(ExitCode::SUCCESS)
}

/// The subscriber for the client's log: compact lines on stderr, `LEVEL target: message`, with no
/// timestamp so that a replayed script logs the same bytes every run. `filter` is `--log`'s value;
/// with none the level is `info`. With `--log-file`, the file is appended to as well, each line
/// with its time: the daemon's connections (`daemon`), the chat (`chat`), every plugin's console
/// line (`dereth::plugin`) and the plugin host's own (`dereth::plugins`), and any warning.
///
/// # Errors
/// A filter this binary cannot read, so a mistyped `--log` fails the run rather than silently
/// logging at a level nobody asked for; or a log file that cannot be opened.
fn install_log(filter: Option<&str>, file: Option<&std::path::Path>) -> Result<(), String> {
    use tracing_subscriber::filter::{LevelFilter, Targets};
    use tracing_subscriber::layer::{Layer as _, SubscriberExt as _};
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
    let file_layer = match file {
        Some(path) => {
            let f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| format!("--log-file {}: {e}", path.display()))?;
            let kept = Targets::new()
                .with_default(LevelFilter::WARN)
                .with_target("daemon", LevelFilter::INFO)
                .with_target("chat", LevelFilter::INFO)
                .with_target("dereth::plugins", LevelFilter::INFO)
                .with_target("dereth::plugin", LevelFilter::DEBUG);
            Some(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(std::sync::Mutex::new(f))
                    .with_filter(kept),
            )
        }
        None => None,
    };
    // Also installs the bridge for `log` records. It fails only when a subscriber is already
    // installed, which in this binary nothing else does.
    let _ = tracing_subscriber::registry()
        .with(stderr.with_filter(targets))
        .with(file_layer)
        .try_init();
    Ok(())
}
