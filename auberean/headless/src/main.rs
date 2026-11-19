//! The client with no window: logs in to a server on this machine and plays a script of
//! commands, on the SDK alone.
//!
//! **Depends on** the SDK alone (`auberean-client-sdk`) and this crate's library. **Used by**
//! nothing: it is the `auberean-headless` program.
//!
//! With `--script FILE` it plays the script in the file ([`auberean_headless::script`] has the
//! commands). Without one it plays the script its flags make ([`auberean_headless::builtin`]):
//! it enters the world with a character (making it first with `--create` when the list has none of
//! that name), stays for `--seconds` and logs off; while in the world it can walk (`--walk`:
//! forward for two seconds, then turning right for one, over and over, the body kept to the ground
//! and stopped by walls as a window's is) and say a line now and then (`--say`). With `--agree` it
//! takes every place in a fellowship, every oath and every trade offered to it, and accepts a trade
//! once the other side has; with `--offer` it offers the first item of its pack when a trade opens;
//! with `--trade-with ID` it asks that player for a trade three seconds after it enters. With
//! `--craft RECIPE:SPINNER` (hex recipe, repeatable) it makes each recipe in turn, three seconds
//! after it enters and twelve after the one before, filling each slot with the first item the
//! scripts say fits, and tells what the slots hold after.
//!
//! Whatever the script, it tells what happens: the session's phases, what is said around it, the
//! other players it sees as they come and move, an effect put on it or taken off, a making's
//! answer. With `--plugins DIR` it runs the plugins there (one package, or a folder of them),
//! keeping their stores in `--plugin-store DIR` (in memory without it) and printing their lines;
//! with `--stdin` it reads lines from its input as they come, each a command that takes no time
//! (a plugin line among them) or a line starting `/` typed as the player would.
//!
//! ```text
//! auberean-headless --dats <install folder> [--host 127.0.0.1] [--port 9000] --account NAME --ticket TICKET
//!              [--script FILE]
//!              [--character ID | --create NAME] [--seconds 30] [--walk] [--say TEXT] [--agree] [--offer]
//!              [--trade-with ID] [--craft RECIPE:SPINNER]... [--world-overlays]
//!              [--plugins DIR]... [--plugin-store DIR] [--stdin]
//! ```
//!
//! `--world-overlays` says the world (a Yalain world) takes this client's own word that it keeps a
//! world's overlay over its data files.
//!
//! The install folder is the September 2013 client's (its data files answer the server's question
//! of which data the client has, its script library runs the server's calls, and its world is the
//! one the player's body walks through). The server must be on this machine: the client refuses
//! any other address. The SDK's driver runs the client's frames on a UDP port and the system's
//! steady clock.
//!
//! **Must never** read a device or reach past the SDK (`cargo xtask seams`).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use auberean_headless::builtin::{self, Flags};
use auberean_headless::plugins;
use auberean_headless::run::Runner;
use auberean_headless::script::{self, Command};

use auberean_client_sdk::host_clock::HostClock;
use auberean_client_sdk::plugins::{MemoryStorage, PluginHook, PluginSource};
use auberean_client_sdk::sim::Walk;
use auberean_client_sdk::udp::UdpPort;
use auberean_client_sdk::{install, ClientSettings, Driver};

struct Options {
    dats: PathBuf,
    host: Ipv4Addr,
    port: u16,
    account: String,
    ticket: String,
    script: Option<PathBuf>,
    flags: Flags,
    /// A flag that shapes the built-in script was given.
    played: Option<&'static str>,
    world_overlays: bool,
    plugins: Vec<PathBuf>,
    plugin_store: Option<PathBuf>,
    stdin: bool,
}

fn hex_id(flag: &str, v: &str) -> Result<u64, String> {
    u64::from_str_radix(v.trim_start_matches("0x"), 16).map_err(|e| format!("{flag}: {e}"))
}

fn options() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let mut o = Options {
        dats: PathBuf::new(),
        host: Ipv4Addr::LOCALHOST,
        port: 9000,
        account: String::new(),
        ticket: String::new(),
        script: None,
        flags: Flags::default(),
        played: None,
        world_overlays: false,
        plugins: Vec::new(),
        plugin_store: None,
        stdin: false,
    };
    while let Some(a) = args.next() {
        let mut value = || args.next().ok_or(format!("{a} needs a value"));
        let f = &mut o.flags;
        let played = match a.as_str() {
            "--dats" => {
                o.dats = PathBuf::from(value()?);
                None
            }
            "--host" => {
                o.host = value()?.parse().map_err(|e| format!("--host: {e}"))?;
                None
            }
            "--port" => {
                o.port = value()?.parse().map_err(|e| format!("--port: {e}"))?;
                None
            }
            "--account" => {
                o.account = value()?;
                None
            }
            "--ticket" => {
                o.ticket = value()?;
                None
            }
            "--script" => {
                o.script = Some(PathBuf::from(value()?));
                None
            }
            "--character" => {
                f.character = Some(hex_id("--character", &value()?)?);
                Some("--character")
            }
            "--create" => {
                f.create = Some(value()?);
                Some("--create")
            }
            "--seconds" => {
                f.seconds = value()?.parse().map_err(|e| format!("--seconds: {e}"))?;
                Some("--seconds")
            }
            "--walk" => {
                f.walk = true;
                Some("--walk")
            }
            "--agree" => {
                f.agree = true;
                Some("--agree")
            }
            "--offer" => {
                f.offer = true;
                Some("--offer")
            }
            "--trade-with" => {
                f.trade_with = Some(hex_id("--trade-with", &value()?)?);
                Some("--trade-with")
            }
            "--say" => {
                f.say = Some(value()?);
                Some("--say")
            }
            "--craft" => {
                let v = value()?;
                let (recipe, spinner) = v.split_once(':').ok_or("--craft takes RECIPE:SPINNER")?;
                f.crafts.push((
                    u32::from_str_radix(recipe.trim_start_matches("0x"), 16)
                        .map_err(|e| format!("--craft: {e}"))?,
                    spinner.parse().map_err(|e| format!("--craft: {e}"))?,
                ));
                Some("--craft")
            }
            "--world-overlays" => {
                o.world_overlays = true;
                None
            }
            "--plugins" => {
                o.plugins.push(PathBuf::from(value()?));
                None
            }
            "--plugin-store" => {
                o.plugin_store = Some(PathBuf::from(value()?));
                None
            }
            "--stdin" => {
                o.stdin = true;
                None
            }
            other => return Err(format!("unknown option {other}")),
        };
        o.played = o.played.or(played);
    }
    if o.dats.as_os_str().is_empty() || o.account.is_empty() {
        return Err("--dats and --account are needed".into());
    }
    if !o.host.is_loopback() {
        return Err("the server must be on this machine (a loopback address)".into());
    }
    if let (Some(_), Some(flag)) = (&o.script, o.played) {
        return Err(format!(
            "{flag} shapes the built-in script: not with --script"
        ));
    }
    Ok(o)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("auberean-headless: {e}");
        std::process::exit(1);
    }
}

/// What went wrong with a plugin's package or store, told on the error stream.
fn told(what: &str) {
    eprintln!("auberean-headless: {what}");
}

/// Install the plugin host, its plugins asked to load at the first frame.
fn install_plugins(driver: &mut Driver, o: &Options) {
    let storage: Box<dyn auberean_client_sdk::plugins::PluginStorage> = match &o.plugin_store {
        Some(root) => Box::new(plugins::DirStorage {
            root: root.clone(),
            report: told,
        }),
        None => Box::new(MemoryStorage::default()),
    };
    let mut source = plugins::DirSource::new(o.plugins.clone(), told);
    let found = source.list();
    let unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64());
    let mut host = plugins::host(Box::<plugins::MachineClock>::default(), storage, source)
        .with_wall_clock(unix - driver.local_time());
    host.set_session(
        Some(format!("{}:{}", o.host, o.port)),
        Some(o.account.clone()),
    );
    for p in found {
        host.request_load(p);
    }
    driver.install_hook(Box::new(PluginHook(host)));
}

/// Lines from the input as they come, read on a thread of their own.
fn read_input() -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

fn settings(o: &Options) -> ClientSettings {
    let mut key = [0u8; 64];
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    #[allow(clippy::cast_possible_truncation)]
    for (i, b) in key.iter_mut().enumerate() {
        let v = (seed >> ((i % 16) * 8)) as u8;
        *b = v ^ (i as u8).wrapping_mul(97);
    }
    ClientSettings {
        login_server: SocketAddr::new(IpAddr::V4(o.host), o.port),
        account: o.account.clone().into_bytes(),
        ticket: o.ticket.clone().into_bytes(),
        timestamp: u32::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
        )
        .unwrap_or(0),
        session_key: key,
        script_sends: true,
    }
}

fn run() -> Result<(), String> {
    let o = options()?;
    let commands = match &o.script {
        Some(path) => {
            let text =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            script::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?
        }
        None => builtin::script(&o.flags),
    };
    let mut port = UdpPort::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .map_err(|e| e.to_string())?;
    let data = install::client_data(&o.dats, |r| {
        eprintln!("auberean-headless: the script library: {r}");
    })?;
    let world = install::world_data(&o.dats)?;
    let mut driver = Driver::connect(settings(&o), data);
    driver.set_world_takes_overlays(o.world_overlays);
    let plugin_lines = commands.iter().any(|c| matches!(c, Command::Plugin(_)));
    if !o.plugins.is_empty() || o.stdin || plugin_lines {
        install_plugins(&mut driver, &o);
    }
    let input = o.stdin.then(read_input);
    let mut clock = HostClock::new();
    let mut runner = Runner::new(driver, Some(Walk::new(&world)), commands);
    let result = loop {
        let stepped = runner.step(&mut clock, &mut port);
        if let Some(input) = &input {
            while let Ok(line) = input.try_recv() {
                runner.line(&line);
            }
        }
        for l in runner.take_lines() {
            println!("{l}");
        }
        match stepped {
            Ok(true) => {}
            Ok(false) => break Ok(()),
            Err(e) => break Err(e),
        }
        std::thread::sleep(Duration::from_millis(if runner.moving() { 5 } else { 10 }));
    };
    runner.finish(&mut port);
    for l in runner.take_lines() {
        println!("{l}");
    }
    result
}
