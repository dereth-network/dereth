//! The Asheron's Call 2 server executable: its settings, the data it reads at start, the sockets
//! and the loop.
//!
//! **Depends on** the game (`yalain-world`), its connections (`yalain-net`, with sockets), its
//! store (`yalain-store`, in SQLite), its content (`yalain-content`), the world database
//! (`yalain-world-db`: the world it serves, compiled at start, fetched by `yalain world`), and
//! Asheron's Call 2's core
//! for the data it reads at start (`auberean-dat`, `auberean-assets`, `auberean-script`,
//! `auberean-world`, `auberean-physics`), with `dereth-primitives`, and the Dereth Network's
//! protocol (`dereth-network-protocol`) for its heartbeat to a hub, its link
//! (`dereth-network-link`) and the world's half of it (`yalain-federation`), with the system's
//! random source (`getrandom`) for login cookies, checksum seeds, the world's dice and the
//! directory's allegiance keys, and the players' friends
//! service (`yalain-friends`), and `ctrlc` for the system's stop signals, with `serde` and
//! `toml` for the operator's character recipes and the configuration file (and `serde_json` for character files), and the
//! servers' updater (`dereth-server-update`) and WebSocket endpoint (`dereth-server-websocket`).
//! **Used by** nothing: it is the `yalain` program.
//!
//! ```text
//! yalain --dats <install folder> [options]
//!
//!   --dats DIR            the September 2013 install the server reads its data from (a copy no
//!                         client is running from: a client opens its data files alone)
//!   --bind IP             the address to listen on (default 127.0.0.1)
//!   --login-port P        the login service's even port (default 9000; P + 1 is used too)
//!   --world-port P        the world service's even port (default 9002; P + 1 is used too)
//!   --advertise IP        the IPv4 address players from elsewhere are sent to for the world;
//!                         players on this computer or its network get the nearby address
//!                         (default: --bind; needed when --bind is 0.0.0.0 or an IPv6 address)
//!   --data FILE           the database of accounts and characters (default yalain.sqlite)
//!   --memory              keep nothing: a database in memory
//!   --world-name NAME     the world's name, shown at login (default "Local World")
//!   --listed-accounts     admit only accounts made with --add-account (default: any new name
//!                         makes its account, with its ticket)
//!   --banned-names FILE   refuse character names holding these words: one pattern a line, `*`
//!                         for any run of letters (default: none)
//!   --add-account N:T     make account N with ticket T, then carry on
//!   --admin N             make account N an admin's when this start makes it (--add-account),
//!                         then carry on
//!   --admin-command W:G,G keep the account-level admin command W to the named groups G
//!   --serve-admin-library serve admin accounts' retail clients the script library that sends
//!                         their commands in the clear
//!   --secret-cost N       the bcrypt cost account secrets are kept at, 4 to 31 (default 8); a
//!                         secret kept at another cost is kept again at this one at its next login
//!   --save-interval S     keep each character in the world every S seconds, 1 or more (default
//!                         300); it is kept as it leaves, and at some events, besides
//!   --shutdown-interval S how many seconds a shutdown counts down, 0 to 99999 (default 120)
//!   --watchdog S          stop with a failure when the world has run no pass for S seconds, 0 for
//!                         never (default 300)
//!   --max-sessions N      how many logins the world holds at once (default 128); a login past
//!                         them is told the server is full
//!   --min-character-age S  seconds a character must have played before it sends to the Dereth
//!                         Network: tells, room lines, channels (default 0)
//!   --max-sessions-per-address N  how many logins one address holds at once (default
//!                         `unlimited`)
//!   --unlimited-sessions-from IP  an address no per-address limit holds back (may be given
//!                         many times)
//!   --download-rate N     the bytes a second a client is sent a large data download at, 4096 to
//!                         262144 (default 65536, the retail servers' rate)
//!   --motd TEXT           a message shown to each player as its character enters the world
//!                         (`\n` starts a new line)
//!   --add-character A:N:R:S  make character N on account A of race R and sex S (4096 or 8192),
//!                         with a middling physique, then carry on; a missing account is made on
//!                         an open world only
//!   --place A:N:CELL:X:Y:Z:H  put character N of account A at a place (cell in hex, a heading
//!                         in degrees) before it next enters, then carry on
//!   --money A:N:AMOUNT    set the money of character N of account A (one that has played) before
//!                         it next enters, then carry on
//!   --make-character FILE make the character a recipe file describes, with its build (a level or
//!                         experience, skills, money, items, quests, a hero, a place), before it
//!                         first plays, then carry on (the `characters` module)
//!   --world-db DIR        the world database to serve, compiled against --dats at start: a
//!                         folder, or `pinned` (the default) for the release this build pins,
//!                         fetched into the per-user cache as it starts if it is not there; a world
//!                         that names a base database (`[base]`, as Athenaeum's do) is served
//!                         over that release in the cache; its `overlay` folder, which
//!                         `athenaeum build` makes, is read over --dats, and one made against other
//!                         files stops the server
//!   --dat-overlay DIR     the world's overlay folder, in place of the world's own `overlay`
//!   --no-dat-overlay-patching  a client that keeps overlays and lacks the world's is refused, not
//!                         sent it
//!   --dat-overlay-records-per-minute N  how fast such a client is sent it (default 60000; 0 for
//!                         the original servers' rate)
//!   --enable-dat-patching a retail client lacking the world's overlay is sent it, at the original
//!                         servers' rate, not refused: it must run with its files writable
//!   --world-base DIR      the base database a --world-db world lies over, in place of the
//!                         release it names in the cache
//!   --no-referral         carry the world on the login connection instead of referring the
//!                         client to the world service
//!   --no-echo             do not send players their own positions back
//!   --game-time S         start the game clock at S seconds since the relaunch's start (1
//!                         December 2012; default: now); a day of the game is 7,347 seconds
//!   --hold-clock          hold the game clock where it starts: the sky and the time of day stand
//!                         still, for comparing clients at one moment; what the game counts in its
//!                         own time (a skill's recovery, the wait after a recall, a quest's wait)
//!                         does not run out while it is held
//!   --hub URL             report this world to the Dereth Network hub at URL (https://, or
//!                         http:// to this machine), with --hub-slug and --hub-key-file: the
//!                         hub's launcher lists it among Asheron's Call 2's worlds
//!   --hub-slug SLUG       the world's slug at the hub, as it was registered
//!   --hub-key-file FILE   the file holding the key the hub gave the world (drth_...)
//!   --federation          link the world to the hub's network (with --hub), once the world's
//!                         federation is on at the hub: characters whose players put them on the
//!                         network reach and are reached by other worlds' characters, of either
//!                         game
//!   --global-room ROOM    the room that carries the network's Global room until the hub keeps
//!                         one: general (the default), trade, lfg, roleplay, or empty for none
//!   --no-friends          serve no friends service: by default the world serves its players'
//!                         friends, presence and tells to Name@world on TCP at the login port's
//!                         number
//!   --seconds S           stop after S seconds (default: run until stopped)
//!   --verbose             log at the debug level (every connection event, and more)
//!   --stop-at-console-end stop, keeping everyone in the world, when the console's input ends:
//!                         for a program that runs the server and holds its console (by
//!                         default the end of the input ends only the console)
//!   --update POLICY       which new releases the server installs by itself: off (the default),
//!                         patch or minor; `yalain update --check` says what would be taken
//!   --update-check-hours H  hours between its checks for a new release (default 6)
//!   --update-warning-seconds S  the countdown players see before it restarts into a new release
//!                         (default 300)
//!   --update-source URL   where new releases are looked for (default: the GitHub repository this
//!                         build came from)
//!   --status IP:PORT      answer GET /health and GET /status (the world's state, as JSON) over
//!                         HTTP at this address (default: off); keep it on loopback or a private
//!                         network: it has no authentication
//!   --websocket           also take WebSocket connections, for clients in a browser, as
//!                         Empyrean's `[server.websocket]` does, with the options below; off by
//!                         default
//!   --websocket-listen IP:PORT  where (default 0.0.0.0:9443)
//!   --websocket-tls-certificate FILE, --websocket-tls-private-key FILE  the PEM files for wss://;
//!                         without them plain ws:// is served, on loopback or with
//!                         --websocket-behind-tls-proxy only
//!   --websocket-behind-tls-proxy  a reverse proxy in front terminates TLS
//!   --websocket-trusted-proxy IP  a proxy whose X-Forwarded-For names the client (may be given
//!                         many times)
//!   --websocket-allowed-origin URL  a web page allowed to connect, by origin (may be given many
//!                         times); none refuses every page
//!   --websocket-max-per-address N  connections open at once from one address (default 4; -1 is
//!                         unlimited)
//!   --websocket-idle-timeout S  seconds a connection may send nothing (default 60)
//!   --websocket-public-url URL  the URL clients use, reported to the status endpoint and the hub
//!                         (default: from --websocket-listen)
//!   --config FILE         read the settings from this configuration file, in the form of
//!                         Empyrean's empyrean.toml (default: yalain.toml in the working folder,
//!                         else beside the executable, if there is one); the command line wins
//!                         over it (the `config` module)
//!
//! yalain --version
//!
//!   the version, and the commit and target a release build was made from
//!
//! yalain world check|release|fetch|compile ...
//!
//!   the world database's commands (see the `world` module)
//!
//! yalain update --check|--apply [--config <yalain.toml>] [--policy <off|patch|minor>]
//!
//!   what a new release would involve, and installing it on a stopped server (the `update` module)
//! ```
//!
//! While it runs, lines typed into its standard input are the operator's commands (the `console`
//! module): `reload` compiles the world database again and takes what can change without a
//! restart; `shutdown [message]` tells every player in the world the world is shutting down,
//! counts down (two minutes, or `--shutdown-interval`) and stops; `cancel-shutdown`,
//! `set-shutdown-interval`, `stop-now` (or `stop`, `exit`) and `save-world` are as Empyrean's
//! console has them; `hero`, `teleport`, `place`, `make-character`, `apply-build`,
//! `deletecharacter`, `export-character` and `import-character` act on characters; and so are the moderation commands (`accountcreate`, `accountget`, `accountlist`,
//! `set-accountpassword`, `set-accountaccess`, `ban`, `unban`, `banlist`, `gag`, `ungag`, `boot`).
//! Staff (an account an advocate's or above) say the same commands in game, after `@`.
//! Each command's answer is one line in a fixed form a program reads (its words, then `[ok
//! <command>]` or `[refused <command>]`: `yalain_world::answers`). When the input ends the console ends
//! and the world goes on, unless `--stop-at-console-end` was given: then the server stops as at
//! `stop`.
//!
//! A line a player says that is `@dn` or begins with `@dn ` is the Dereth Network's command: heard
//! by nobody, and answered by the friends service in the player's chat window.
//!
//! A client reaches it with `--host <IP> --port <login port> --account <name>
//! --glsticketdirect <ticket>`. With the default open accounts any new name logs in, and its
//! account is made with that ticket, which it needs from then on.
//!
//! **Must never** hold the game's rules: it reads, binds and runs the loop; the game is
//! `yalain-world`'s.

use std::net::{IpAddr, Ipv4Addr, SocketAddrV4};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod characters;
mod config;
mod console;
mod data;
mod friends;
mod help;
mod hub;
mod network;
mod random;
mod referral;
mod status;
mod stop;
mod update;
mod verify;
mod world;

use dereth_server_websocket::settings::WebSocketSettings;
use yalain_content::{Catalog, ContentPack, LootTable, SpawnTable};
use yalain_net::udp::UdpPorts;
use yalain_net::websocket;
use yalain_net::{LocalTime, NetConfig, ServerNet};
use yalain_store::{MemoryStore, SqliteStore, Store, StoredPosition};
use yalain_world::login::{account_for_command, add_account, seed_character};
use yalain_world::names::NameRules;
use yalain_world::reload::WorldTables;
use yalain_world::{AccountPolicy, CharGenRules, Clock, Server, ServerConfig, StartRules};
use yalain_world_db::compile::Tables;

/// The game clock counts from the relaunch's start (1 December 2012), as the 2017 servers' did.
const GAME_EPOCH: i64 = 1_354_320_000;

#[derive(Debug)]
struct Options {
    dats: Option<PathBuf>,
    bind: IpAddr,
    login_port: u16,
    world_port: u16,
    advertise: Option<Ipv4Addr>,
    data: PathBuf,
    memory: bool,
    world_name: String,
    listed: bool,
    banned_names: Option<PathBuf>,
    add_accounts: Vec<(String, String)>,
    admins: Vec<String>,
    admin_commands: Vec<(String, Vec<String>)>,
    serve_admin_library: bool,
    /// The bcrypt cost of account secrets (`--secret-cost`).
    secret_cost: u32,
    /// Seconds with no pass of the world before it is stopped (`--watchdog`), 0 for never.
    watchdog_seconds: u32,
    /// Seconds between a character's saves (`--save-interval`).
    save_seconds: u32,
    /// Seconds a shutdown counts down (`--shutdown-interval`).
    shutdown_seconds: u32,
    /// Logins the world holds at once (`--max-sessions`).
    max_sessions: u32,
    /// The bytes a second a client's large downloads go at (`--download-rate`).
    download_rate: u32,
    /// Seconds a character must have played before it sends to the Dereth Network
    /// (`--min-character-age`).
    min_character_age: u32,
    /// Logins one address holds at once (`--max-sessions-per-address`; none: no limit).
    max_sessions_per_address: Option<u32>,
    /// Addresses no per-address limit holds back (`--unlimited-sessions-from`).
    unlimited_addresses: Vec<IpAddr>,
    /// The message of the day (`--motd`).
    motd: String,
    /// Where the status endpoint listens (`--status`; none: off).
    status: Option<std::net::SocketAddr>,
    /// Answer the status ping on the login port (not `--no-status-ping`), within these limits
    /// (`--status-ping-hellos`, `--status-ping-asks`, `--status-ping-replies`).
    status_ping: bool,
    status_ping_limits: yalain_net::status_ping::Limits,
    add_characters: Vec<(String, String, u32, u32)>,
    places: Vec<(String, String, StoredPosition)>,
    money: Vec<(String, String, i32)>,
    /// Recipe files of characters to make with their builds (`--make-character`).
    recipes: Vec<PathBuf>,
    world_db: String,
    dat_overlay: Option<PathBuf>,
    dat_overlay_settings: yalain_world::world_patch::DatOverlaySettings,
    world_base: Option<PathBuf>,
    referral: bool,
    echo: bool,
    game_time: Option<f64>,
    hold_clock: bool,
    /// Where and as what the world reports itself to a Dereth Network hub (`--hub`).
    hub: Option<hub::HubSettings>,
    /// Link to the hub's network (`--federation`).
    federation: bool,
    /// The room that carries the Global room until the hub keeps one (`--global-room`), when the
    /// operator names one.
    global_room: Option<String>,
    /// Serve the friends service (not `--no-friends`).
    friends: bool,
    seconds: Option<f64>,
    verbose: bool,
    /// Stop, keeping everyone, when the console's input ends (`--stop-at-console-end`).
    stop_at_console_end: bool,
    /// The WebSocket endpoint (`--websocket` and the options after it).
    websocket: WebSocketSettings,
    /// The updater's settings (`--update`, `--update-check-hours`, `--update-warning-seconds`,
    /// `--update-source`).
    update: update::Settings,
    /// The updater's health check of a newly installed release (`--update-trial <report>`): start
    /// up, write this build's facts to the report, and stop. It never updates itself.
    update_trial: Option<PathBuf>,
    /// Hidden, for the updater's own tests: the updater takes this process for that version.
    update_test_version: Option<String>,
}

/// The options the configuration file and the command line give (the command line after the
/// file, so it wins), and what to log about the file once logging is up.
fn parse() -> Result<(Options, Vec<String>), String> {
    let (o, notes, _) = read_options(std::env::args().skip(1).collect())?;
    Ok((o, notes))
}

/// The options the configuration file and `args` give, what to log about the file once logging is
/// up, and the file read.
fn read_options(mut args: Vec<String>) -> Result<(Options, Vec<String>, Option<PathBuf>), String> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let exe = std::env::current_exe().ok();
    let exe_dir = exe.as_deref().and_then(Path::parent);
    let mut notes = Vec::new();
    let mut all = Vec::new();
    let found = config::locate(&mut args, &cwd, exe_dir)?;
    if let Some(file) = &found {
        let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        let folder = match file.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => cwd.clone(),
        };
        let (from_file, warnings) =
            config::arguments(&text, &folder).map_err(|e| format!("{}: {e}", file.display()))?;
        notes.push(format!("configuration read from {}", file.display()));
        notes.extend(
            warnings
                .into_iter()
                .map(|w| format!("{}: {w}", file.display())),
        );
        all = from_file;
    }
    all.extend(args);
    Ok((parse_from(all)?, notes, found))
}

/// Logging from here on, at the debug level when `verbose`.
fn start_logging(verbose: bool) {
    tracing_subscriber::fmt()
        .with_max_level(if verbose {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .with_ansi(false)
        .init();
    // A panic's place and its calls go where the operator reads, whichever thread it is on.
    std::panic::set_hook(Box::new(|info| {
        let calls = std::backtrace::Backtrace::force_capture();
        tracing::error!("panic: {info}\n{calls}");
    }));
}

/// What [`read_options`] had to say about the file.
fn log_notes(notes: &[String]) {
    for n in notes {
        if n.starts_with("configuration read") {
            tracing::info!("{n}");
        } else {
            tracing::warn!("{n}");
        }
    }
}

/// The options these arguments give; `Err("")` for `--help`.
fn parse_from(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut o = Options {
        dats: None,
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        login_port: 9000,
        world_port: 9002,
        advertise: None,
        data: PathBuf::from("yalain.sqlite"),
        memory: false,
        world_name: ServerConfig::default().world_name,
        listed: false,
        banned_names: None,
        add_accounts: Vec::new(),
        admins: Vec::new(),
        admin_commands: Vec::new(),
        serve_admin_library: false,
        secret_cost: yalain_world::login::SECRET_COST,
        watchdog_seconds: yalain_world::guard::NO_PASS_SECONDS,
        save_seconds: yalain_world::saves::SAVE_SECONDS,
        shutdown_seconds: yalain_world::shutdown::LEAD,
        max_sessions: yalain_world::login::MAX_SESSIONS,
        min_character_age: 0,
        download_rate: yalain_net::pace::DEFAULT_BYTES_PER_SECOND,
        max_sessions_per_address: None,
        unlimited_addresses: Vec::new(),
        motd: String::new(),
        status: None,
        status_ping: true,
        status_ping_limits: yalain_net::status_ping::Limits::default(),
        add_characters: Vec::new(),
        places: Vec::new(),
        money: Vec::new(),
        recipes: Vec::new(),
        world_db: "pinned".into(),
        dat_overlay: None,
        dat_overlay_settings: yalain_world::world_patch::DatOverlaySettings::default(),
        world_base: None,
        referral: true,
        echo: true,
        game_time: None,
        hold_clock: false,
        hub: None,
        federation: false,
        global_room: None,
        friends: true,
        seconds: None,
        verbose: false,
        stop_at_console_end: false,
        websocket: WebSocketSettings::default(),
        update: update::Settings::default(),
        update_trial: None,
        update_test_version: None,
    };
    let (mut hub_url, mut hub_slug, mut hub_key_file) = (None, None, None);
    let mut args = args.into_iter();
    while let Some(a) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{a} needs a value"));
        match a.as_str() {
            "--dats" => o.dats = Some(PathBuf::from(value()?)),
            "--bind" => o.bind = value()?.parse().map_err(|e| format!("--bind: {e}"))?,
            "--login-port" => {
                o.login_port = value()?.parse().map_err(|e| format!("--login-port: {e}"))?
            }
            "--world-port" => {
                o.world_port = value()?.parse().map_err(|e| format!("--world-port: {e}"))?
            }
            "--advertise" => {
                o.advertise = Some(value()?.parse().map_err(|e| format!("--advertise: {e}"))?)
            }
            "--data" => o.data = PathBuf::from(value()?),
            "--memory" => o.memory = true,
            "--world-name" => o.world_name = value()?,
            "--listed-accounts" => o.listed = true,
            "--banned-names" => o.banned_names = Some(PathBuf::from(value()?)),
            "--add-account" => {
                let v = value()?;
                let (n, t) = v.split_once(':').ok_or("--add-account takes NAME:TICKET")?;
                o.add_accounts.push((n.to_string(), t.to_string()));
            }
            "--admin" => o.admins.push(value()?),
            "--serve-admin-library" => o.serve_admin_library = true,
            "--admin-command" => {
                let v = value()?;
                let (word, groups) = v
                    .split_once(':')
                    .ok_or("--admin-command takes COMMAND:GROUP,GROUP")?;
                if !yalain_world::admin_groups::is_command(word) {
                    return Err(format!(
                        "--admin-command: {word} is none of {}",
                        yalain_world::admin_groups::WORDS.join(", ")
                    ));
                }
                o.admin_commands.push((
                    word.to_ascii_lowercase(),
                    yalain_world::admin_groups::read_list(groups),
                ));
            }
            "--secret-cost" => {
                let n: u32 = value()?
                    .parse()
                    .map_err(|e| format!("--secret-cost: {e}"))?;
                if !yalain_world::login::SECRET_COSTS.contains(&n) {
                    return Err("--secret-cost takes 4 to 31".into());
                }
                o.secret_cost = n;
            }
            "--save-interval" => {
                let n: u32 = value()?
                    .parse()
                    .map_err(|e| format!("--save-interval: {e}"))?;
                if n == 0 {
                    return Err("--save-interval takes seconds, 1 or more".into());
                }
                o.save_seconds = n;
            }
            "--watchdog" => {
                o.watchdog_seconds = value()?.parse().map_err(|e| format!("--watchdog: {e}"))?;
            }
            "--shutdown-interval" => {
                let n: u32 = value()?
                    .parse()
                    .map_err(|e| format!("--shutdown-interval: {e}"))?;
                if n > yalain_world::shutdown::MAX_SECONDS {
                    return Err("--shutdown-interval takes 0 to 99999 seconds".into());
                }
                o.shutdown_seconds = n;
            }
            "--min-character-age" => {
                o.min_character_age = value()?
                    .parse()
                    .map_err(|e| format!("--min-character-age: {e}"))?;
            }
            "--max-sessions" => {
                o.max_sessions = value()?
                    .parse()
                    .map_err(|e| format!("--max-sessions: {e}"))?;
            }
            "--download-rate" => {
                let rate = value()?
                    .parse()
                    .map_err(|e| format!("--download-rate: {e}"))?;
                yalain_net::pace::check_rate(rate).map_err(|e| format!("--download-rate: {e}"))?;
                o.download_rate = rate;
            }
            "--max-sessions-per-address" => {
                let v = value()?;
                o.max_sessions_per_address = if v == "unlimited" || v == "-1" {
                    None
                } else {
                    Some(
                        v.parse()
                            .map_err(|e| format!("--max-sessions-per-address: {e}"))?,
                    )
                };
            }
            "--unlimited-sessions-from" => o.unlimited_addresses.push(
                value()?
                    .parse()
                    .map_err(|e| format!("--unlimited-sessions-from: {e}"))?,
            ),
            "--motd" => o.motd = value()?.replace("\\n", "\n"),
            "--status" => {
                o.status = Some(value()?.parse().map_err(|e| format!("--status: {e}"))?);
            }
            "--no-status-ping" => o.status_ping = false,
            "--status-ping-hellos" | "--status-ping-asks" | "--status-ping-replies" => {
                let n: u32 = value()?.parse().map_err(|e| format!("{a}: {e}"))?;
                let l = &mut o.status_ping_limits;
                match a.as_str() {
                    "--status-ping-hellos" => l.hellos_per_minute = n,
                    "--status-ping-asks" => l.asks_per_minute = n,
                    _ => l.replies_per_second = n,
                }
            }
            // Taken out before the options are read (the `config` module).
            "--config" => {
                value()?;
            }
            "--add-character" => {
                let v = value()?;
                let p: Vec<&str> = v.split(':').collect();
                let [a, n, r, x] = p[..] else {
                    return Err("--add-character takes ACCOUNT:NAME:RACE:SEX".into());
                };
                let num = |s: &str| {
                    s.parse::<u32>()
                        .map_err(|e| format!("--add-character: {e}"))
                };
                o.add_characters
                    .push((a.to_string(), n.to_string(), num(r)?, num(x)?));
            }
            "--place" => {
                let v = value()?;
                let p: Vec<&str> = v.split(':').collect();
                let [a, n, cell, x, y, z, h] = p[..] else {
                    return Err("--place takes ACCOUNT:NAME:CELL:X:Y:Z:HEADING".into());
                };
                let f = |s: &str| s.parse::<f32>().map_err(|e| format!("--place: {e}"));
                let cell = u32::from_str_radix(cell, 16).map_err(|e| format!("--place: {e}"))?;
                let at = StoredPosition {
                    cell,
                    origin: [f(x)?, f(y)?, f(z)?],
                    rotation: console::rotation_of_degrees(f(h)?),
                };
                o.places.push((a.to_string(), n.to_string(), at));
            }
            "--money" => {
                let v = value()?;
                let p: Vec<&str> = v.split(':').collect();
                let [a, n, m] = p[..] else {
                    return Err("--money takes ACCOUNT:NAME:AMOUNT".into());
                };
                let m = m.parse::<i32>().map_err(|e| format!("--money: {e}"))?;
                o.money.push((a.to_string(), n.to_string(), m));
            }
            "--make-character" => o.recipes.push(PathBuf::from(value()?)),
            "--content" | "--spawns" | "--loot" | "--stores" | "--quests" => {
                return Err(format!(
                    "{a}: the world's content is the world database now: give --world-db <folder>, or nothing for the pinned release"
                ));
            }
            "--world-db" => o.world_db = value()?,
            "--dat-overlay" => o.dat_overlay = Some(PathBuf::from(value()?)),
            "--no-dat-overlay-patching" => o.dat_overlay_settings.patching = false,
            "--dat-overlay-records-per-minute" => {
                let v = value()?;
                o.dat_overlay_settings.records_per_minute =
                    v.parse().map_err(|e| format!("{a}: {v}: {e}"))?;
            }
            "--enable-dat-patching" => o.dat_overlay_settings.enable_dat_patching = true,
            "--world-base" => o.world_base = Some(PathBuf::from(value()?)),
            "--no-referral" => o.referral = false,
            "--no-echo" => o.echo = false,
            "--game-time" => {
                let t: f64 = value()?.parse().map_err(|e| format!("--game-time: {e}"))?;
                if !t.is_finite() || t < 0.0 {
                    return Err("--game-time takes seconds, 0 or more".into());
                }
                o.game_time = Some(t);
            }
            "--hold-clock" => o.hold_clock = true,
            "--hub" => hub_url = Some(value()?),
            "--hub-slug" => hub_slug = Some(value()?),
            "--hub-key-file" => hub_key_file = Some(PathBuf::from(value()?)),
            "--federation" => o.federation = true,
            "--global-room" => o.global_room = Some(value()?),
            "--no-friends" => o.friends = false,
            "--seconds" => {
                o.seconds = Some(value()?.parse().map_err(|e| format!("--seconds: {e}"))?)
            }
            "--verbose" => o.verbose = true,
            "--stop-at-console-end" => o.stop_at_console_end = true,
            "--websocket" => o.websocket.enabled = true,
            "--websocket-listen" => o.websocket.listen = value()?,
            "--websocket-tls-certificate" => o.websocket.tls_certificate = value()?,
            "--websocket-tls-private-key" => o.websocket.tls_private_key = value()?,
            "--websocket-behind-tls-proxy" => o.websocket.behind_tls_proxy = true,
            "--websocket-trusted-proxy" => o.websocket.trusted_proxies.push(value()?),
            "--websocket-allowed-origin" => o.websocket.allowed_origins.push(value()?),
            "--websocket-max-per-address" => {
                o.websocket.maximum_connections_per_ip_address = value()?
                    .parse()
                    .map_err(|e| format!("--websocket-max-per-address: {e}"))?;
            }
            "--websocket-idle-timeout" => {
                o.websocket.idle_timeout = value()?
                    .parse()
                    .map_err(|e| format!("--websocket-idle-timeout: {e}"))?;
            }
            "--websocket-public-url" => o.websocket.public_url = value()?,
            "--update" => o.update.update = value()?,
            "--update-check-hours" => {
                o.update.check_hours = value()?
                    .parse()
                    .map_err(|e| format!("--update-check-hours: {e}"))?;
            }
            "--update-warning-seconds" => {
                o.update.warning_seconds = value()?
                    .parse()
                    .map_err(|e| format!("--update-warning-seconds: {e}"))?;
            }
            "--update-source" => o.update.source = value()?,
            "--update-trial" => o.update_trial = Some(PathBuf::from(value()?)),
            dereth_server_update::running::TEST_VERSION_FLAG => {
                o.update_test_version = Some(value()?);
            }
            "--help" | "-h" => return Err(String::new()),
            other => return Err(format!("unknown option {other}")),
        }
    }
    if !o.login_port.is_multiple_of(2) || !o.world_port.is_multiple_of(2) {
        return Err("the ports are even: each service uses its port and the one above".into());
    }
    o.update.own_world = !world::uses_the_pin(&o.world_db, o.world_base.as_deref());
    o.hub = match (hub_url, hub_slug, hub_key_file) {
        (None, None, None) => None,
        (Some(url), Some(slug), Some(key)) => Some(hub::HubSettings::new(&url, &slug, &key)?),
        (Some(_), ..) => return Err("--hub needs --hub-slug and --hub-key-file".into()),
        (None, ..) => return Err("--hub-slug and --hub-key-file go with --hub".into()),
    };
    if o.referral {
        referral::advertised(o.bind, o.advertise)?;
    }
    if o.websocket.enabled {
        o.websocket
            .validate()
            .map_err(|e| format!("WebSocket: {e}"))?;
    }
    if o.federation {
        match &o.hub {
            None => return Err("--federation needs --hub, --hub-slug and --hub-key-file".into()),
            Some(h) => {
                network::settings(h, None, o.global_room.as_deref())?;
                network::link_url(h)?;
            }
        }
    }
    Ok(o)
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

/// The accounts the configuration lists, made with their tickets, and those it names as admins'
/// raised. A name is raised only on the start that made its account from a ticket given here: only
/// then is its secret surely the operator's. An account already there may be a stranger's, who
/// logged in with that name on an open world, whatever ticket the list holds for it; and one the
/// console has made a player again stays a player. Either is the console's to raise.
fn seed_accounts<S: Store>(
    store: &mut S,
    accounts: &[(String, String)],
    admins: &[String],
    cost: u32,
    now: i64,
) {
    let mut made = Vec::new();
    for (n, t) in accounts {
        match add_account(store, n, t.as_bytes(), cost, now) {
            Ok(a) => made.push(a.id),
            Err(e) => tracing::warn!("account {n}: {e}"),
        }
    }
    for name in admins {
        match store.account_by_name(name) {
            Ok(Some(a)) if made.contains(&a.id) => match store.set_account_admin(a.id, true) {
                Ok(()) => tracing::info!(account = %a.name, "an admin's account"),
                Err(e) => tracing::warn!(account = %a.name, "not made an admin: {e}"),
            },
            Ok(Some(a)) if a.admin => {}
            Ok(_) => tracing::warn!(
                account = %name,
                "not made an admin: the list raises only an account this start makes from its \
                 ticket (--add-account); at the console, set-accountadmin {name}"
            ),
            Err(e) => tracing::warn!(account = %name, "not made an admin: {e}"),
        }
    }
}

/// Make the accounts and characters the command line asks for.
fn seed<S: Store>(o: &Options, store: &mut S, rules: &CharGenRules) {
    let policy = if o.listed {
        AccountPolicy::Listed
    } else {
        AccountPolicy::Open
    };
    seed_accounts(store, &o.add_accounts, &o.admins, o.secret_cost, unix_now());
    for (a, n, race, sex) in &o.add_characters {
        let account = match account_for_command(policy, store, a, unix_now()) {
            Ok(acc) => acc,
            Err(e) => {
                tracing::warn!("--add-character {n}: {e}");
                continue;
            }
        };
        match seed_character(store, rules, &account, n, *race, *sex, unix_now()) {
            Ok(c) => tracing::info!("character {n} made on {a}: {:#x}", c.id),
            Err(e) => tracing::warn!("character {n}: {e}"),
        }
    }
    for (a, n, amount) in &o.money {
        let found = store
            .account_by_name(a)
            .ok()
            .flatten()
            .and_then(|acc| store.characters(acc.id).ok())
            .and_then(|cs| cs.into_iter().find(|c| c.name.eq_ignore_ascii_case(n)));
        let Some(c) = found else {
            tracing::warn!("--money: no character {n} on {a}");
            continue;
        };
        let mut st = store.world_state(c.id).ok().flatten().unwrap_or_default();
        if st.qualities.is_empty() {
            tracing::warn!("--money: {n} has not played yet");
            continue;
        }
        yalain_world::quals::set_int(&mut st.qualities, yalain_world::store::MONEY, *amount);
        match store.set_world_state(c.id, &st) {
            Ok(()) => tracing::info!("{n} has {amount} money"),
            Err(e) => tracing::warn!("--money {n}: {e}"),
        }
    }
    for (a, n, at) in &o.places {
        let found = store
            .account_by_name(a)
            .ok()
            .flatten()
            .and_then(|acc| store.characters(acc.id).ok())
            .and_then(|cs| cs.into_iter().find(|c| c.name.eq_ignore_ascii_case(n)));
        let Some(c) = found else {
            tracing::warn!("--place: no character {n} on {a}");
            continue;
        };
        let mut st = store.world_state(c.id).ok().flatten().unwrap_or_default();
        st.position = Some(*at);
        match store.set_world_state(c.id, &st) {
            Ok(()) => tracing::info!("{n} placed at {:08X} {:?}", at.cell, at.origin),
            Err(e) => tracing::warn!("--place {n}: {e}"),
        }
    }
}

/// What the world plays: its templates, camps and loot.
struct Play {
    catalog: Catalog,
    spawns: SpawnTable,
    loot: LootTable,
    ground: data::LandGround,
    walls: yalain_world::walls::Walls,
    /// The files a client's requests for records are served from.
    files: yalain_world::download::InstallFiles,
    levels: yalain_world::levels::LevelTable,
    skills: yalain_world::skills::SkillTable,
    effects: yalain_world::effect_defs::EffectTable,
    item_effects: std::collections::HashMap<u32, Vec<yalain_world::profile::ItemEffect>>,
    mounts: yalain_world::mounts::Mounts,
    interactions: yalain_world::usable::interactions::Interactions,
    plant_looks: yalain_world::plants::Looks,
    item_uses: yalain_world::item_use::ItemUses,
    worn_effects: yalain_world::item_use::ItemUses,
    quest_ids: std::collections::HashMap<String, u32>,
    craft: yalain_world::craft::Crafting,
    vitae_costs: Vec<u64>,
    stores: yalain_content::StoreTable,
    quests: yalain_content::QuestTable,
    talk: yalain_content::TalkTable,
    story: yalain_content::StoryTable,
    locks: yalain_content::LockTable,
    places_named: std::collections::BTreeMap<u32, yalain_content::Place>,
    places_by_name: std::collections::BTreeMap<String, yalain_content::Place>,
    brokers: std::collections::BTreeMap<u32, yalain_world::market::Broker>,
    walk_in: std::collections::HashSet<u32>,
    persons: std::collections::HashSet<u32>,
    shrines: std::collections::HashSet<u32>,
    shrine_kingdoms: std::collections::HashMap<u32, u32>,
    cannot_use: std::collections::HashSet<u32>,
    doors: std::collections::HashMap<u32, yalain_world::usable::doors::Kind>,
    doors_through_locks: std::collections::HashSet<u32>,
    chests: std::collections::HashSet<u32>,
    destroyed_on_use: std::collections::HashSet<u32>,
    treasure: yalain_world::treasure::Kinds,
    tesserae: std::collections::HashSet<u32>,
    inscribable: std::collections::HashSet<u32>,
    held_answers: std::collections::HashMap<u32, (u32, u32)>,
    weapons: std::collections::HashMap<String, u32>,
    weapon_rules: std::collections::HashMap<String, yalain_content::WeaponRule>,
    death_spawns: std::collections::HashMap<String, yalain_content::DeathSpawn>,
    death_effects: yalain_world::item_use::ItemUses,
}

/// The exit code of a server whose ports another program holds, beside the line that says so
/// (`yalain_world::answers::ports_taken`): a program starting the server tries other ports.
const PORTS_TAKEN: u8 = 3;

/// Why the server stopped short: the words, and whether a port it listens on was taken.
#[derive(Debug)]
struct Failure {
    why: String,
    ports_taken: bool,
}

impl From<String> for Failure {
    fn from(why: String) -> Self {
        Self {
            why,
            ports_taken: false,
        }
    }
}

impl Failure {
    /// A port could not be bound: `what` and the system's error, which says whether another
    /// program holds it.
    fn binding(what: String, e: &std::io::Error) -> Self {
        Self {
            why: format!("{what}: {e}"),
            ports_taken: e.kind() == std::io::ErrorKind::AddrInUse,
        }
    }
}

/// How the world's loop ended, for what follows it.
struct Ended {
    /// The updater, when it ran.
    auto: Option<std::sync::Arc<update::AutoUpdate>>,
    /// The world ended by itself: its countdown ran out, or `stop-now`.
    world_ended: bool,
    /// The console's lines.
    typed: std::sync::mpsc::Receiver<String>,
}

fn run<S: Store>(
    o: &Options,
    names: NameRules,
    mut store: S,
    rules: CharGenRules,
    start: StartRules,
    content: ContentPack,
    play: Play,
) -> Result<Ended, Failure> {
    seed(o, &mut store, &rules);
    // The world's dice. A world whose system has no random source does not start: its cookies come
    // from the same place, and nothing stands in for it.
    let seed = random::os_u64()?;
    let cfg = ServerConfig {
        world_name: o.world_name.clone(),
        accounts: if o.listed {
            AccountPolicy::Listed
        } else {
            AccountPolicy::Open
        },
        referral: o.referral,
        echo_positions: o.echo,
        seed,
        secret_cost: o.secret_cost,
        dat_overlay: o.dat_overlay_settings,
        admin_commands: o.admin_commands.clone(),
        save_seconds: o.save_seconds,
        shutdown_seconds: o.shutdown_seconds,
        max_sessions: o.max_sessions,
        min_character_age_seconds: o.min_character_age,
        max_sessions_per_address: o.max_sessions_per_address,
        unlimited_addresses: o.unlimited_addresses.clone(),
        motd: o.motd.clone(),
        ..ServerConfig::default()
    };
    let advertise = if o.referral {
        referral::advertised(o.bind, o.advertise)?
    } else {
        o.advertise.unwrap_or(Ipv4Addr::LOCALHOST)
    };
    let ports = UdpPorts::bind(o.bind, o.login_port, o.world_port).map_err(|e| {
        Failure::binding(
            format!(
                "binding the ports: {}:{}-{} and {}-{}",
                o.bind,
                o.login_port,
                o.login_port + 1,
                o.world_port,
                o.world_port + 1
            ),
            &e,
        )
    })?;
    // A port of the endpoint's that is taken is no reason to try other game ports: it stops the
    // start like any setting that cannot be served.
    let mut wire = websocket::Wire {
        ports,
        websocket: websocket::start(&o.websocket)?,
        friends: Vec::new(),
    };
    let websocket_url = wire
        .websocket
        .as_ref()
        .map(|ws| o.websocket.url(ws.local_addr(), ws.tls()));
    let started = Instant::now();
    let local = || LocalTime(started.elapsed().as_secs_f64());
    #[allow(clippy::cast_precision_loss)]
    let game_time = o
        .game_time
        .unwrap_or_else(|| (unix_now() - GAME_EPOCH) as f64);
    let mut net = ServerNet::new(
        NetConfig {
            large_message_bytes_per_second: o.download_rate,
            ..NetConfig::retail_ids(SocketAddrV4::new(advertise, o.world_port))
        },
        random::os_u64,
        game_time,
        local(),
    );
    if o.referral {
        net.refer_nearby(o.bind, yalain_net::udp::address_toward);
    }
    if let Some(numbering) = wire.websocket.as_ref().map(|ws| ws.numbering()) {
        net.refer_carried(move |client| websocket::world_port(&numbering, client));
    }
    if o.hold_clock {
        net.hold_clock(local());
        tracing::info!("the game clock is held at {game_time} s");
    }
    let mut server = Server::new(cfg, store, rules)
        .map_err(|e| format!("the database will not set ids aside for the world's things: {e}"))?
        .with_world(start, content)
        .with_play(play.catalog, play.spawns, play.loot);
    server.ground = Some(Box::new(play.ground));
    server.walls = Some(play.walls);
    server.stand_placed();
    let files = play.files;
    if o.serve_admin_library {
        use yalain_world::admin_library::{AdminLibrary, RECORD};
        use yalain_world::download::DataFiles as _;
        let record = files
            .read(auberean_dat::DatKind::Portal, RECORD)
            .ok_or_else(|| {
                "--serve-admin-library: no script library in the data files".to_owned()
            })?;
        let library = AdminLibrary::of(&record)?;
        let bytes = library.bytes();
        tracing::info!(
            "serving admin accounts' retail clients the script library: {bytes} bytes, about {} s \
             of a client's first login",
            bytes / o.download_rate
        );
        server.admin_library = Some(library);
    }
    server.world_patch = yalain_world::world_patch::WorldPatch::of(&files).map(std::sync::Arc::new);
    server.data = Some(Box::new(files));
    server.levels = play.levels;
    server.skill_table = play.skills;
    server.effect_table = play.effects;
    server.item_effects = play.item_effects;
    yalain_world::download::prepare(&mut server);
    server.mounts = play.mounts;
    server.interactions = play.interactions;
    server.plants.looks = play.plant_looks;
    server.item_uses = play.item_uses;
    server.worn_effects = play.worn_effects;
    server.quest_ids = play.quest_ids;
    server.crafting = play.craft;
    server.vitae_costs = play.vitae_costs;
    server.stores = play.stores;
    server.quests = play.quests;
    server.talk = play.talk;
    server.names = names;
    server.story = play.story;
    server.locks = play.locks;
    server.places_named = play.places_named;
    server.places_by_name = play.places_by_name;
    server.brokers = play.brokers;
    server.walk_in_packages = play.walk_in;
    server.person_packages = play.persons;
    server.shrine_packages = play.shrines;
    server.shrine_kingdoms = play.shrine_kingdoms;
    server.cannot_use_packages = play.cannot_use;
    server.doors.packages = play.doors;
    server.doors.creatures_through_locks = play.doors_through_locks;
    server.chests.packages = play.chests;
    server.destroyed_on_use = play.destroyed_on_use;
    server.treasure = play.treasure;
    server.tesserae = play.tesserae;
    server.inscribable_packages = play.inscribable;
    server.held_answers = play.held_answers;
    server.creature_weapons = play.weapons;
    server.creature_weapon_rules = play.weapon_rules;
    server.death_spawns = play.death_spawns;
    server.death_effects = play.death_effects;
    for path in &o.recipes {
        make_character(&mut server, path, unix_now());
    }
    tracing::info!(
        target: yalain_world::answers::TARGET,
        "{}",
        yalain_world::answers::listening(&format!(
            "listening on {}:{}/{} (login) and {}:{}/{} (world, referred as {advertise})",
            o.bind,
            o.login_port,
            o.login_port + 1,
            o.bind,
            o.world_port,
            o.world_port + 1
        ))
    );
    let mut ping = status_ping(o);
    let mut reporter = o.hub.clone().map(hub::Reporter::start);
    let status = o
        .status
        .map(|at| {
            let first = snapshot(&server, &net, started, websocket_url.as_ref());
            let pages = if wire.websocket.is_some() {
                o.websocket.allowed_origins.clone()
            } else {
                Vec::new()
            };
            status::Endpoint::start(at, first, pages)
        })
        .transpose()?;
    if let Some(s) = &status {
        tracing::info!("status endpoint listening on http://{}/status", s.addr());
    }
    let mut network = match (&o.hub, o.federation) {
        (Some(h), true) => Some(network::Network::start(
            h,
            websocket_url.as_deref(),
            o.global_room.as_deref(),
            unix_now(),
            &mut server.store,
        )?),
        _ => None,
    };
    let slug = match (&o.hub, o.federation) {
        (Some(h), true) => h.slug.clone(),
        _ => String::new(),
    };
    let service = yalain_friends::Service::load(
        yalain_friends::Config::new(&o.world_name, &slug),
        &mut server.store,
    )
    .map_err(|e| format!("the friends lists: {e}"))?;
    let mut friends = friends::Friends::start(
        service,
        o.friends.then(|| friends::listen_at(o.bind, o.login_port)),
    )?;
    let typed = console::lines();
    let mut reloading: Option<console::Job<(PathBuf, WorldTables)>> = None;
    // The console's commands that act on the world, run at the loop's next clock.
    let mut pending: Vec<console::Command> = Vec::new();
    let interrupt = stop::Interrupt::install();
    // Whether the console's input is still open, and whether its end stops the server.
    let mut console_open = true;
    let mut console_ended = false;
    // What the updater thread asks the world, and the updater: never in its own health check.
    let (ask, asks) = std::sync::mpsc::channel();
    let auto = match &o.update_trial {
        Some(_) => None,
        None => update::start(
            &o.update,
            (!o.memory).then_some(o.data.as_path()),
            o.update_test_version.as_deref(),
            ask,
        ),
    };
    let passes = yalain_world::guard::Passes::default();
    watch_the_world(&passes, o.watchdog_seconds);
    // A panic anywhere in the loop ends it the same way: see `yalain_world::guard`.
    let run_loop = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        loop {
            passes.pass();
            if o.seconds
                .is_some_and(|s| started.elapsed().as_secs_f64() >= s)
            {
                break;
            }
            if interrupt.raised() {
                tracing::info!("stopping at the system's word");
                break;
            }
            if console_ended {
                break;
            }
            loop {
                let line = match typed.try_recv() {
                    Ok(line) => line,
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        if console_open {
                            console_open = false;
                            if o.stop_at_console_end {
                                tracing::info!("stopping: the console's input ended");
                                console_ended = true;
                            } else {
                                tracing::info!(
                                "the console's input ended: no more commands are read, and the \
                                 world goes on"
                            );
                            }
                        }
                        break;
                    }
                };
                let Some(c) = console::parse(&line) else {
                    continue;
                };
                let name = c.name();
                match c {
                    console::Command::Help => console::answer(&name, &Ok(console::HELP.to_owned())),
                    c @ (console::Command::Shutdown(_)
                    | console::Command::CancelShutdown
                    | console::Command::StopNow(_)
                    | console::Command::SaveWorld
                    | console::Command::Teleport(Ok(_))
                    | console::Command::Place(Ok(_))
                    | console::Command::MakeCharacter(Ok(_))
                    | console::Command::ApplyBuild(Ok(_))
                    | console::Command::DeleteCharacter(Ok(_))
                    | console::Command::ForceLogoff(Ok(_))
                    | console::Command::ExportCharacter(Ok(_))
                    | console::Command::ImportCharacter(Ok(_))
                    | console::Command::Admin(Ok(_))) => pending.push(c),
                    // The moderation commands answer in Empyrean's words, under their own word.
                    console::Command::Admin(Err(usage)) => console::answer(
                        &line
                            .split_whitespace()
                            .next()
                            .unwrap_or_default()
                            .to_ascii_lowercase(),
                        &Err(usage),
                    ),
                    console::Command::Teleport(Err(usage))
                    | console::Command::Place(Err(usage))
                    | console::Command::MakeCharacter(Err(usage))
                    | console::Command::ApplyBuild(Err(usage))
                    | console::Command::DeleteCharacter(Err(usage))
                    | console::Command::ForceLogoff(Err(usage))
                    | console::Command::ExportCharacter(Err(usage))
                    | console::Command::ImportCharacter(Err(usage)) => {
                        console::answer(&name, &Err(usage));
                    }
                    console::Command::SetShutdownInterval(None) => console::answer(
                        &name,
                        &Err("set-shutdown-interval takes seconds, 0 to 99999".into()),
                    ),
                    console::Command::SetShutdownInterval(Some(n)) => {
                        console::answer(
                            &name,
                            &Ok(format!(
                                "the shutdown interval is {n} seconds (it was {})",
                                server.cfg.shutdown_seconds
                            )),
                        );
                        server.cfg.shutdown_seconds = n;
                    }
                    console::Command::Reload if reloading.is_some() => {
                        console::answer(&name, &Err("a reload is already running".into()));
                    }
                    console::Command::Reload => {
                        tracing::info!("reloading the world database");
                        let dats = o.dats.clone().unwrap_or_default();
                        let (db, base) = (o.world_db.clone(), o.world_base.clone());
                        let banned = o.banned_names.clone();
                        let overlay = o.dat_overlay.clone();
                        reloading = Some(console::start(move || {
                            reload_world(
                                &dats,
                                &db,
                                overlay.as_deref(),
                                base.as_deref(),
                                banned.as_deref(),
                            )
                        }));
                    }
                    // The network's lines, as its own console has them.
                    console::Command::Network(words) => {
                        let words: Vec<&str> = words.iter().map(String::as_str).collect();
                        match &mut network {
                            Some(n) => {
                                let t = local().0;
                                for line in
                                    n.console(&mut server.store, &mut friends.service, t, &words)
                                {
                                    tracing::info!("{line}");
                                }
                            }
                            None => tracing::info!("This world is not on the Dereth Network."),
                        }
                    }
                    console::Command::Hero(None) => console::answer(
                        &name,
                        &Err(
                            "hero <name> makes a character a hero, hero <name> off no longer"
                                .into(),
                        ),
                    ),
                    console::Command::Hero(Some((who, on))) => console::answer(
                        &name,
                        &yalain_world::focus::set_hero(&mut server, &mut net, &who, on)
                            .map_err(|e| format!("hero: {e}")),
                    ),
                    console::Command::Unknown(c) => {
                        console::answer(
                            &name,
                            &Err(format!("{c}: not a command ({})", console::HELP)),
                        );
                    }
                }
            }
            if let Some(done) = reloading.as_ref().and_then(console::Job::poll) {
                reloading = None;
                match done {
                    Ok((dir, tables)) => {
                        let clock = Clock {
                            local: local(),
                            unix: unix_now(),
                        };
                        let r = server.reload(&mut net, clock, tables);
                        console::answer(
                            "reload",
                            &Ok(format!("world database {}: {}", dir.display(), r.summary())),
                        );
                        for line in &r.details {
                            tracing::info!("{line}");
                        }
                    }
                    Err(e) => console::answer(
                        "reload",
                        &Err(format!(
                            "the world database did not compile; the world is unchanged: {e}"
                        )),
                    ),
                }
            }
            let now = local();
            passes.at(yalain_world::guard::Part::Network);
            // The status ping is answered on the login port before the transport reads anything.
            let mut pinged = Vec::new();
            let seconds = started.elapsed().as_secs();
            let got = wire.receive(|port, from, bytes| {
                if port == yalain_net::Port::LOGIN_EVEN
                    && yalain_net::status_ping::is_status_ping(bytes)
                {
                    if let Some(reply) = ping.answer(from, bytes, seconds) {
                        pinged.push(yalain_net::Datagram {
                            from: port,
                            to: from,
                            bytes: reply,
                        });
                    }
                    return;
                }
                net.on_datagram(port, from, bytes, now);
            });
            wire.send(&pinged);
            let clock = Clock {
                local: now,
                unix: unix_now(),
            };
            for c in pending.drain(..) {
                server.saves.piece_begins();
                world_command(&mut server, &mut net, clock, c);
            }
            for a in asks.try_iter() {
                update::answer(a, &mut server, &mut net, clock);
            }
            passes.at(yalain_world::guard::Part::World);
            server.pump(&mut net, clock);
            passes.at(yalain_world::guard::Part::Sending);
            if let Some(r) = &mut reporter {
                let state = if server.shutdown == yalain_world::shutdown::Shutdown::No {
                    hub::State::Online
                } else {
                    hub::State::Maintenance
                };
                r.now(
                    state,
                    u32::try_from(server.players.len()).unwrap_or(u32::MAX),
                );
            }
            if let Some(s) = &status {
                s.set(snapshot(&server, &net, started, websocket_url.as_ref()));
            }
            ping.facts.state = if server.shutdown == yalain_world::shutdown::Shutdown::No {
                yalain_net::status_ping::WorldState::Open
            } else {
                yalain_net::status_ping::WorldState::ShuttingDown
            };
            ping.facts.players = u16::try_from(server.players.len()).unwrap_or(u16::MAX);
            if let Some(n) = &mut network {
                n.directory(&mut server, now.0);
                n.pump(&mut friends.service, now.0);
                for (who, line) in n.verification(&mut net, now.0) {
                    friends::show_operator(&server, &mut net, who, &line);
                }
            }
            let carried = wire.take_friends();
            // With no friends service the stream goes unanswered, and the client gives it up.
            if o.friends {
                let at = Duration::from_secs_f64(now.0);
                for (from, bytes) in carried {
                    friends.on_carried(
                        at,
                        from,
                        &bytes,
                        &mut server.store,
                        network.as_mut().map(|n| &mut n.federation),
                    );
                }
                friends.drop_carried(
                    at,
                    |a| wire.is_open(a),
                    &mut server.store,
                    network.as_mut().map(|n| &mut n.federation),
                );
            }
            let talked = friends.pump(
                Duration::from_secs_f64(now.0),
                &mut server,
                &mut net,
                network.as_mut().map(|n| &mut n.federation),
            );
            for (to, bytes) in friends.take_carried_out() {
                wire.send_friends(to, &bytes);
            }
            net.tick(now);
            let out = net.take_datagrams();
            wire.send(&out);
            if server.shutdown == yalain_world::shutdown::Shutdown::Done {
                break;
            }
            // Everything has started and the world has run a pass: the health check has its answer.
            if let Some(report) = &o.update_trial {
                update::report_ready(report);
                break;
            }
            if got == 0 && out.is_empty() && !talked {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }));
    passes.over();
    let panicked = run_loop
        .err()
        .map(|p| yalain_world::guard::words(p.as_ref()));
    if let Some(what) = &panicked {
        let clock = Clock {
            local: local(),
            unix: unix_now(),
        };
        yalain_world::guard::stopped(&mut server, &mut net, clock, what);
    }
    let world_ended = server.shutdown == yalain_world::shutdown::Shutdown::Done;
    let left = yalain_world::shutdown::stopping(&mut server, &mut net, local());
    let lost = save_at_stop(&mut server);
    if lost.is_none() && panicked.is_none() {
        tracing::info!("stopping: {left} characters in the world kept");
    }
    wire.send(&net.take_datagrams());
    wire.stop();
    friends.stop();
    if let Some(n) = &mut network {
        n.stop();
    }
    if let Some(r) = reporter {
        r.stop();
    }
    // The status address is free for whichever server runs next.
    if let Some(s) = status {
        s.stop();
    }
    for (what, n) in server.unhandled.counts() {
        tracing::info!("not handled yet: {what} x{n}");
    }
    if let Some(lost) = lost {
        return Err(lost.into());
    }
    if let Some(what) = panicked {
        return Err(format!("the world panicked: {what}").into());
    }
    Ok(Ended {
        auto,
        world_ended,
        typed,
    })
}

/// A thread outside the world's that stops the server when the world runs no pass for `seconds`: the
/// failure's exit, so a service manager starts it again and no staged update goes in. It can reach
/// nothing the world holds, so each character goes back to its last save.
fn watch_the_world(passes: &yalain_world::guard::Passes, seconds: u32) {
    if seconds == 0 {
        return;
    }
    let passes = passes.clone();
    std::thread::spawn(move || {
        yalain_world::guard::watch(
            &passes,
            seconds,
            || std::thread::sleep(Duration::from_secs(1)),
            |part| {
                tracing::error!(
                    "THE WORLD HAS RUN NO PASS FOR {seconds} SECONDS, last in {}. It is stopping: \
                     nothing since the last saves can be written, and characters go back to them. \
                     This is a bug.",
                    part.words()
                );
                std::process::exit(1);
            },
        );
    });
}

/// How many times, a second apart, a server stopping tries to save what saves have not taken.
const STOP_SAVE_TRIES: u32 = 30;

/// A server stopping saves what it holds, trying for a while. If it cannot, what it says went wrong
/// and whom it has lost: this process is the last to hold them.
fn save_at_stop<S: Store>(server: &mut Server<S>) -> Option<String> {
    let mut last = None;
    for attempt in 1..=STOP_SAVE_TRIES {
        match yalain_world::saves::write_out(server) {
            Ok(()) => return None,
            Err(e) => {
                tracing::error!(
                    "stopping: the save failed (try {attempt} of {STOP_SAVE_TRIES}): {e}. Free the \
                     database's disk or let go of its file, and it is taken."
                );
                last = Some(e);
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    let names: Vec<String> = server
        .saves
        .unsaved()
        .into_iter()
        .map(|id| {
            server
                .store
                .character(id)
                .ok()
                .flatten()
                .map_or_else(|| format!("{id:#x}"), |c| c.name)
        })
        .collect();
    Some(format!(
        "stopped without saving: {}. Lost since their last save: {}",
        last.map_or_else(String::new, |e| e.to_string()),
        names.join(", ")
    ))
}

/// The status ping on the login port, as Empyrean's on its game port: on unless turned off, with
/// the world's name and this server's version.
fn status_ping(o: &Options) -> yalain_net::status_ping::StatusPing {
    use yalain_net::status_ping::{Facts, StatusPing, WorldState};
    let limits = o.status_ping_limits;
    if o.status_ping {
        tracing::info!(
            "status ping on the login port: {} hellos and {} asks a minute from one address, {} \
             replies a second",
            limits.hellos_per_minute,
            limits.asks_per_minute,
            limits.replies_per_second
        );
    } else {
        tracing::info!("status ping off ([status] udp_ping = false)");
    }
    StatusPing::new(
        o.status_ping,
        limits,
        Facts {
            state: WorldState::Starting,
            players: 0,
            software: "Yalain".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            world_name: o.world_name.clone(),
        },
        Box::new(|| {
            let mut secret = [0u8; 32];
            getrandom::fill(&mut secret).expect("the system's random source");
            secret
        }),
    )
}

/// What the status endpoint says of the world now.
fn snapshot<S: Store>(
    server: &Server<S>,
    net: &ServerNet,
    started: Instant,
    websocket_url: Option<&String>,
) -> status::Snapshot {
    status::Snapshot {
        websocket_url: websocket_url.cloned(),
        world_name: server.cfg.world_name.clone(),
        uptime_seconds: started.elapsed().as_secs(),
        shutting_down: server.shutdown != yalain_world::shutdown::Shutdown::No,
        players: server.players.len(),
        logins: server.sessions.len(),
        max_logins: server.cfg.max_sessions,
        connections: net.connection_count(),
        saves_failing: server.saves.failing(),
    }
}

/// Run a console command that acts on the world, and answer it.
fn world_command<S: Store>(
    server: &mut Server<S>,
    net: &mut ServerNet,
    clock: Clock,
    c: console::Command,
) {
    use yalain_world::characters;
    use yalain_world::shutdown;
    let name = c.name();
    let done = match c {
        console::Command::Shutdown(message) => {
            if shutdown::begin(server, net, clock, message.as_deref()) {
                Ok(format!(
                    "the world shuts down in {} seconds",
                    server.cfg.shutdown_seconds.min(shutdown::MAX_SECONDS)
                ))
            } else {
                Err("a shutdown is already in progress".to_owned())
            }
        }
        console::Command::CancelShutdown => {
            if shutdown::cancel(server, net) {
                Ok("the shutdown is cancelled".to_owned())
            } else {
                Err("no shutdown is counting down".to_owned())
            }
        }
        console::Command::StopNow(message) => {
            shutdown::stop_now(server, net, clock, message.as_deref());
            Ok("stopping now".to_owned())
        }
        console::Command::SaveWorld => yalain_world::saves::save_world(server, clock)
            .map(|n| format!("saved: {n} characters in the world kept"))
            .map_err(|e| format!("not saved: {e}; every character stays held and is tried again")),
        console::Command::Teleport(Ok((who, to))) => {
            characters::teleport(server, net, clock, &who, &to)
        }
        console::Command::Place(Ok((who, to))) => characters::place(server, &who, &to),
        console::Command::MakeCharacter(Ok(path)) => {
            return make_character(server, &path, clock.unix)
        }
        console::Command::ApplyBuild(Ok(path)) => crate::characters::read_recipe(&path)
            .and_then(|r| characters::apply(server, &r, clock.unix)),
        console::Command::DeleteCharacter(Ok((who, false))) => {
            characters::delete(server, net, clock, &who)
        }
        console::Command::DeleteCharacter(Ok((who, true))) => {
            characters::delete_now(server, net, clock, &who)
        }
        console::Command::ForceLogoff(Ok(who)) => {
            characters::force_logoff(server, net, clock, &who)
        }
        console::Command::ExportCharacter(Ok((who, path))) => characters::export(server, &who)
            .and_then(|f| crate::characters::write_file(&path, &f).map(|()| f.character.name))
            .map(|who| format!("{who} written to {}", path.display())),
        console::Command::ImportCharacter(Ok((path, account, who))) => {
            crate::characters::read_file(&path)
                .and_then(|f| characters::import(server, &f, &account, who.as_deref(), clock.unix))
        }
        // Each line of its answer in Empyrean's words, under the command's own word.
        console::Command::Admin(Ok(c)) => {
            for line in yalain_world::admin::answered(
                server,
                net,
                clock,
                &yalain_world::admin::Issuer::Console,
                &c,
            ) {
                console::answer(c.word(), &line);
            }
            return;
        }
        _ => return,
    };
    // A character command's refusal names the command first, as it always has.
    let shutting = ["shutdown", "cancel-shutdown"].contains(&name.as_str());
    let done = done.map_err(|e| if shutting { e } else { format!("{name}: {e}") });
    console::answer(&name, &done);
}

/// Make the character a recipe file describes, with its build; the answer logged.
fn make_character<S: Store>(server: &mut Server<S>, path: &Path, now: i64) {
    let done = crate::characters::read_recipe(path)
        .and_then(|r| yalain_world::characters::make(server, &r, now))
        .map_err(|e| format!("make-character {}: {e}", path.display()));
    console::answer("make-character", &done);
}

/// Compile the world database (a folder, or `pinned`) against the install: its folder and tables.
fn compile_world(
    portal: &data::Portal,
    world_db: &str,
    base: Option<&Path>,
) -> Result<(PathBuf, Tables), String> {
    world::database_dir(world_db).and_then(|d| {
        let t = world::compile_dir(portal, &d, base)?;
        Ok((d, t))
    })
}

/// The world's objects and what it plays, from its database compiled and the install.
fn load_play(portal: &data::Portal, compiled: Tables) -> Result<(ContentPack, Play), String> {
    let Tables {
        content,
        spawns,
        loot,
        stores,
        quests,
        talk,
        crafting,
        story,
        locks,
        places_named,
        places_by_name,
        weapons: listed_weapons,
        weapon_rules,
        death_spawns,
        ..
    } = compiled;
    let play = (|| {
        let skills = data::skill_table(portal)?;
        tracing::info!(
            skills = skills.skills.len(),
            tables = skills.tables.len(),
            "the skills read"
        );
        // The effects skills and quests put on players, the recipes they grant, and the effects
        // the recipes put on their makers.
        let mut states: Vec<u32> = skills
            .skills
            .values()
            .filter_map(|d| d.usage.as_ref())
            .flat_map(|u| {
                u.target_effects
                    .iter()
                    .chain(&u.user_effects)
                    .chain(&u.required_effects)
                    .copied()
            })
            .chain(quests.steps.iter().flat_map(|s| s.effects.iter().copied()))
            .chain(
                quests
                    .quests
                    .iter()
                    .flat_map(|q| q.effects.iter().map(|(_, e)| *e)),
            )
            .collect();
        let effects = data::effect_table(portal, &states)?;
        let granted: Vec<u32> = effects
            .effects
            .values()
            .filter_map(|d| match d.kind {
                yalain_world::effect_defs::Kind::GrantRecipe(r)
                | yalain_world::effect_defs::Kind::RemoveRecipe(r) => Some(r),
                _ => None,
            })
            .collect();
        let mut craft = data::crafting(portal, &crafting, &granted)?;
        craft.butchery = crafting
            .butchery
            .iter()
            .map(|b| {
                // A share of nothing is a yield of no material.
                let nothing =
                    (b.empty > 0).then_some((&yalain_world::butchery::NOTHING_FOUND, &b.empty));
                let yields = b
                    .items
                    .iter()
                    .zip(&b.weights)
                    .chain(nothing)
                    .map(|(t, w)| yalain_world::butchery::Yield {
                        template: *t,
                        weight: *w,
                    })
                    .collect();
                (b.profile, yields)
            })
            .collect();
        let actions: Vec<yalain_world::craft::Action> = craft
            .recipes
            .values()
            .flat_map(|r| r.checks.iter())
            .flat_map(|c| c.outcomes.iter())
            .flat_map(|o| o.actions.iter().cloned())
            .collect();
        states.extend(actions.iter().filter_map(|a| match a {
            yalain_world::craft::Action::Effect { state, .. }
            | yalain_world::craft::Action::ItemEffect { state, .. } => Some(*state),
            _ => None,
        }));
        let effects = data::effect_table(portal, &states)?;
        tracing::info!(
            effects = effects.effects.len(),
            recipes = craft.recipes.len(),
            craft_skills = craft.skills.len(),
            "the effects and crafting read"
        );
        let more: Vec<u32> = data::MORE_TEMPLATES
            .iter()
            .copied()
            .chain(yalain_world::mounts::ITEMS)
            .chain([yalain_world::usable::portals::TEMPLATE])
            .chain(stores.sales.iter().map(|s| s.product))
            .chain(quests.steps.iter().flat_map(|s| s.gives.iter().copied()))
            .chain(
                crafting
                    .butchery
                    .iter()
                    .flat_map(|b| b.items.iter().copied()),
            )
            .chain(quests.steps.iter().filter_map(|s| match s.trigger {
                yalain_content::quests::Trigger::Take(t) => Some(t),
                _ => None,
            }))
            .chain(craft.templates())
            // The placed objects' own (a chest's lock and contents, a door's).
            .chain(content.objects.iter().map(|o| o.template))
            .collect();
        let (mut catalog, mut skipped) = data::catalog(portal, &spawns, &loot, &more)?;
        // The weapons the creatures wield, described with the rest.
        let weapons = data::creature_weapons(portal, &catalog, &listed_weapons)?;
        let wanted: Vec<u32> = weapons.values().copied().collect();
        let (held, also) = data::catalog(
            portal,
            &SpawnTable::default(),
            &LootTable::default(),
            &wanted,
        )?;
        skipped.extend(also);
        for (id, t) in held.templates {
            catalog.templates.entry(id).or_insert(t);
        }
        for (id, icon) in held.icons {
            catalog.icons.entry(id).or_insert(icon);
        }
        for (id, tables) in held.icon_tables {
            catalog.icon_tables.entry(id).or_insert(tables);
        }
        tracing::info!(weapons = weapons.len(), "the creatures' weapons read");
        catalog.bodies = data::bodies(portal, &catalog);
        catalog.implements = data::implements(portal, &catalog)?;
        data::resolve_ingredient_classes(portal, &mut craft, &catalog)?;
        // The effects workshops put on players nearby and the recipes need, read with the rest.
        states.extend(craft.recipes.values().filter_map(|r| r.required_effect));
        states.extend(
            catalog
                .templates
                .values()
                .filter_map(|t| t.props.word(yalain_world::workshops::EFFECT_PROPERTY)),
        );
        for s in &skipped {
            tracing::warn!("not described: {s}");
        }
        tracing::info!(
            camps = spawns.camps.len(),
            profiles = loot.profiles.len(),
            templates = catalog.templates.len(),
            "spawns, loot and templates read"
        );
        let brokers = data::brokers(portal, &content);
        tracing::info!(brokers = brokers.len(), "the market's brokers read");
        let (ground, walls) = data::land_ground(portal, &spawns, &catalog)?;
        tracing::info!("the ground under the camps, and the interior cells: {ground:?}; {walls:?}");
        let levels = data::level_table(portal)?;
        tracing::info!(levels = levels.levels.len(), "the level table read");
        let vitae_costs = data::vitae_costs(portal)?;
        let item_effects = data::item_effects(portal, &catalog)?;
        // The mounts, their effects read with the rest.
        let bodies: Vec<u32> = data::chargen_rules(portal)?
            .bodies
            .values()
            .copied()
            .collect();
        let mut mounts = data::mounts(portal, &bodies)?;
        states.extend(mounts.effects.keys().copied());
        tracing::info!(
            effects = mounts.effects.len(),
            looks = mounts.looks.len(),
            "the mounts read"
        );
        // The effects the items' uses put on, read with the rest.
        let (item_uses, worn_effects, death_effects) = data::item_uses(portal, &catalog)?;
        states.extend(item_uses.values().flatten().map(|e| e.state));
        states.extend(yalain_world::item_use::FOOD_EFFECTS);
        states.extend(worn_effects.values().flatten().map(|e| e.state));
        states.extend(death_effects.values().flatten().map(|e| e.state));
        // The treasure potions' effects, put on when they are drunk.
        states.extend(yalain_world::treasure::POTION_EFFECTS.iter().flatten());
        // What the practice dummies hold.
        states.push(yalain_world::spawn::PRACTICE_DUMMY_EFFECT);
        // What treasure weapons, armor and shields carry and put on what they hit.
        states.extend(yalain_content::treasure_effects::states());
        mounts.note_items(&item_uses);
        let effects = data::effect_table(portal, &states)?;
        tracing::info!(
            templates = item_uses.len(),
            effects = effects.effects.len(),
            "the effects items' uses put on read"
        );
        tracing::info!(
            templates = worn_effects.len(),
            "the effects items put on their wearers read"
        );
        let walk_in = data::walk_in_packages(portal, &content)?;
        let persons =
            data::packages_deriving_from(portal, &content, &yalain_world::places::PERSONS)?;
        let shrines = data::packages_deriving_from(
            portal,
            &content,
            &yalain_world::usable::shrines::FACTION_SHRINES,
        )?;
        // The kingdom each shrine's use joins, by its kind.
        let mut shrine_kingdoms = std::collections::HashMap::new();
        for (kind, kingdom) in yalain_world::kingdoms::SHRINES {
            for package in data::packages_deriving_from(portal, &content, &[kind])? {
                shrine_kingdoms.insert(package, kingdom);
            }
        }
        let cannot_use =
            data::packages_deriving_from(portal, &content, &yalain_world::places::CANNOT_BE_USED)?;
        let (doors, doors_through_locks) = data::door_packages(portal, &content)?;
        let chests = data::chest_packages(portal, &content)?;
        tracing::info!(
            doors = doors.len(),
            chests = chests.len(),
            "the doors' and chests' kinds read"
        );
        let destroyed_on_use = data::destroyed_on_use(portal, &catalog)?;
        let treasure = data::treasure(portal, &catalog)?;
        let tesserae =
            data::catalog_packages_from(portal, &catalog, &[yalain_world::treasure::TESSERAE])?;
        tracing::info!(
            equipment = treasure.equipment.len(),
            potions = treasure.potions.len(),
            looks = treasure.potion_looks.len(),
            tesserae = tesserae.len(),
            "the treasure items' kinds read"
        );
        let interactions = data::interactions(portal, &catalog)?;
        tracing::info!(
            tables = interactions.tables.len(),
            "the items' interaction tables read"
        );
        let plant_looks = data::plant_looks(portal, &catalog);
        tracing::info!(looks = plant_looks.len(), "the dye plants' ripe looks read");
        let inscribable =
            data::catalog_packages_from(portal, &catalog, &yalain_world::personal::INSCRIBABLE)?;
        let mut held_answers = std::collections::HashMap::new();
        for (base, status, flags) in yalain_world::places::HELD_ANSWERED {
            for pkg in data::catalog_packages_from(portal, &catalog, &[base])? {
                held_answers.insert(pkg, (status, flags));
            }
        }
        tracing::info!(
            packages = walk_in.len(),
            "the portals used by walking into them"
        );
        tracing::info!(
            templates = item_effects.len(),
            "the effects item profiles list read"
        );
        Ok(Play {
            files: data::install_files(portal),
            catalog,
            spawns,
            loot,
            ground,
            walls,
            levels,
            skills,
            effects,
            item_effects,
            mounts,
            interactions,
            plant_looks,
            item_uses,
            worn_effects,
            quest_ids: data::quest_ids(portal)?,
            craft,
            vitae_costs,
            stores,
            quests,
            talk,
            story,
            locks,
            places_named,
            places_by_name,
            brokers,
            walk_in,
            persons,
            shrines,
            shrine_kingdoms,
            cannot_use,
            doors,
            doors_through_locks,
            chests,
            destroyed_on_use,
            treasure,
            tesserae,
            inscribable,
            held_answers,
            weapons,
            weapon_rules: weapon_rules.into_iter().collect(),
            death_spawns: death_spawns.into_iter().collect(),
            death_effects,
        })
    })();
    play.map(|p| (content, p))
}
/// The world database compiled again for a running server: its folder and its tables, with what
/// the install adds to them.
fn reload_world(
    dats: &Path,
    world_db: &str,
    dat_overlay: Option<&Path>,
    base: Option<&Path>,
    banned_names: Option<&Path>,
) -> Result<(PathBuf, WorldTables), String> {
    let names = banned_names.map(read_banned_names).transpose()?;
    // A portal file of its own: this reload is the only reader of it, on its own thread.
    let portal = data::Portal::open(dats, world::overlay_of(world_db, dat_overlay)?)?;
    let (dir, compiled) = compile_world(&portal, world_db, base)?;
    for w in &compiled.warnings {
        tracing::warn!("{w}");
    }
    let (content, p) = load_play(&portal, compiled)?;
    let tables = WorldTables {
        content,
        catalog: p.catalog,
        spawns: p.spawns,
        loot: p.loot,
        stores: p.stores,
        quests: p.quests,
        talk: p.talk,
        story: p.story,
        locks: p.locks,
        places_named: p.places_named,
        places_by_name: p.places_by_name,
        effects: p.effects,
        crafting: p.craft,
        weapons: p.weapons,
        weapon_rules: p.weapon_rules,
        death_spawns: p.death_spawns,
        names,
        kinds: Some(yalain_world::reload::PlacedKinds {
            brokers: p.brokers,
            walk_in: p.walk_in,
            persons: p.persons,
            shrines: p.shrines,
            cannot_use: p.cannot_use,
            doors: p.doors,
            doors_through_locks: p.doors_through_locks,
            chests: p.chests,
        }),
        ground: Some(Box::new(p.ground)),
        patch: yalain_world::world_patch::WorldPatch::of(&p.files),
        files: Some(Box::new(p.files)),
    };
    Ok((dir, tables))
}

/// The operator's banned names, from their file.
fn read_banned_names(path: &Path) -> Result<NameRules, String> {
    std::fs::read_to_string(path)
        .map(|t| NameRules::from_text(&t))
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--version"] {
        print!("{}", version_text());
        return std::process::ExitCode::SUCCESS;
    }
    if args == ["--release-facts"] {
        println!("{}", update::Facts::this_build().to_json());
        return std::process::ExitCode::SUCCESS;
    }
    if args.first().map(String::as_str) == Some("update") {
        return update::command(args.into_iter().skip(1));
    }
    if args.first().map(String::as_str) == Some("world") {
        return match world::main(&args[1..]) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("yalain world: {e}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    let (o, notes) = match parse() {
        Ok(o) => o,
        Err(e) if e.is_empty() => {
            print!("{}", help::TEXT);
            return std::process::ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("yalain: {e}");
            eprintln!("usage: yalain --dats <install folder> [--bind IP] [--login-port P] [--world-port P] [--advertise IP] [--data FILE | --memory] [--world-name NAME] [--listed-accounts] [--banned-names FILE] [--add-account NAME:TICKET] [--secret-cost N] [--save-interval S] [--shutdown-interval S] [--watchdog S] [--max-sessions N] [--max-sessions-per-address N] [--unlimited-sessions-from IP] [--download-rate N] [--motd TEXT] [--status IP:PORT] [--no-status-ping] [--status-ping-hellos N] [--status-ping-asks N] [--status-ping-replies N] [--config FILE] [--add-character ACCOUNT:NAME:RACE:SEX] [--place ACCOUNT:NAME:CELL:X:Y:Z:HEADING] [--money ACCOUNT:NAME:AMOUNT] [--make-character FILE] [--world-db DIR|pinned] [--world-base DIR] [--dat-overlay DIR] [--no-dat-overlay-patching] [--dat-overlay-records-per-minute N] [--enable-dat-patching] [--no-referral] [--no-echo] [--game-time S] [--hold-clock] [--hub URL --hub-slug SLUG --hub-key-file FILE [--federation] [--global-room ROOM]] [--no-friends] [--seconds S] [--verbose] [--stop-at-console-end] [--update off|patch|minor] [--update-check-hours H] [--update-warning-seconds S] [--update-source URL]");
            eprintln!("       yalain world check|release|fetch|compile ...");
            eprintln!(
                "       yalain update --check|--apply [--config FILE] [--policy off|patch|minor]"
            );
            eprintln!("       yalain --version | --release-facts");
            return std::process::ExitCode::FAILURE;
        }
    };
    start_logging(o.verbose);
    log_notes(&notes);
    let Some(dats) = o.dats.clone() else {
        eprintln!("yalain: --dats names the install to read the data from");
        return std::process::ExitCode::FAILURE;
    };

    let loaded = world::overlay_of(&o.world_db, o.dat_overlay.as_deref())
        .and_then(|overlay| {
            if let Some(w) = &overlay {
                tracing::info!(
                    "reading the data files through world {}'s overlay",
                    w.world()
                );
            }
            data::Portal::open(&dats, overlay)
        })
        .and_then(|p| Ok((data::chargen_rules(&p)?, data::start_rules(&p)?, p)));
    let (rules, start, portal) = match loaded {
        Ok(r) => r,
        Err(e) => {
            eprintln!("yalain: reading the character data: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    tracing::info!(
        areas = start.areas.len(),
        races = start.skills.len(),
        "start areas and starting skills read"
    );
    let names = match o.banned_names.as_deref().map(read_banned_names).transpose() {
        Ok(n) => {
            let n = n.unwrap_or_default();
            tracing::info!(patterns = n.banned.len(), "banned names read");
            n
        }
        Err(e) => {
            eprintln!("yalain: --banned-names: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let compiled = match world::fetch_for_start(&o.world_db, o.world_base.as_deref())
        .and_then(|()| compile_world(&portal, &o.world_db, o.world_base.as_deref()))
    {
        Ok((d, t)) => {
            for w in &t.warnings {
                tracing::warn!("{w}");
            }
            tracing::info!(
                "world database {} compiled: {}",
                d.display(),
                t.counts()
                    .iter()
                    .map(|(k, n)| format!("{k} {n}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            t
        }
        Err(e) => {
            eprintln!("yalain: the world database: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let play = load_play(&portal, compiled);
    drop(portal);
    let (content, play) = match play {
        Ok(p) => p,
        Err(e) => {
            eprintln!("yalain: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let result = if o.memory {
        run(
            &o,
            names,
            MemoryStore::default(),
            rules,
            start,
            content,
            play,
        )
    } else {
        let before = yalain_store::backups(&o.data);
        match SqliteStore::open(&o.data) {
            Ok(store) => {
                for b in yalain_store::backups(&o.data) {
                    if !before.contains(&b) {
                        tracing::info!(
                            "{} was backed up to {} before its schema was upgraded",
                            o.data.display(),
                            b.display()
                        );
                    }
                }
                run(&o, names, store, rules, start, content, play)
            }
            Err(e) => Err(format!("{}: {e}", o.data.display()).into()),
        }
    };
    match result {
        // A release the updater staged is installed now that everything is saved and closed, and
        // the new server (or, when it fails, the restored one) takes over this process.
        Ok(ended) => update::after_shutdown(ended.auto.as_deref(), ended.world_ended, ended.typed)
            .unwrap_or(std::process::ExitCode::SUCCESS),
        Err(f) if f.ports_taken => {
            tracing::error!(
                target: yalain_world::answers::TARGET,
                "{}",
                yalain_world::answers::ports_taken(&f.why)
            );
            std::process::ExitCode::from(PORTS_TAKEN)
        }
        Err(f) => {
            eprintln!("yalain: {}", f.why);
            std::process::ExitCode::FAILURE
        }
    }
}

/// What `yalain --version` prints: the version, and the commit and target a release build
/// was made from (`unknown` in a build that did not say).
fn version_text() -> String {
    format!(
        "yalain {}\ncommit: {}\ntarget: {}\nsource: {}\n",
        env!("CARGO_PKG_VERSION"),
        option_env!("YALAIN_BUILD_COMMIT").unwrap_or("unknown"),
        option_env!("YALAIN_BUILD_TARGET").unwrap_or("unknown"),
        update::SOURCE_URL,
    )
}

#[cfg(test)]
mod watchdog_tests {
    //! Behaviour: none (the option's reading; the watch itself is held in the world's tests)
    use super::parse_from;

    fn with(more: &[&str]) -> Result<u32, String> {
        let args = ["--dats", "here"].iter().chain(more);
        parse_from(args.map(|a| (*a).to_owned())).map(|o| o.watchdog_seconds)
    }

    #[test]
    fn the_watchdog_defaults_to_five_minutes_and_takes_a_number_of_seconds() {
        assert_eq!(with(&[]), Ok(300));
        assert_eq!(with(&["--watchdog", "0"]), Ok(0));
        assert_eq!(with(&["--watchdog", "45"]), Ok(45));
        assert!(with(&["--watchdog", "soon"]).is_err());
    }
}

#[cfg(test)]
mod admin_tests {
    use super::seed_accounts;
    use yalain_store::{MemoryStore, Store as _};
    use yalain_world::login::admits;
    use yalain_world::AccountPolicy;

    const COST: u32 = 4;

    fn list(accounts: &[&str]) -> Vec<(String, String)> {
        accounts
            .iter()
            .map(|a| ((*a).to_owned(), "operators-ticket".to_owned()))
            .collect()
    }

    fn names(admins: &[&str]) -> Vec<String> {
        admins.iter().map(|a| (*a).to_owned()).collect()
    }

    fn is_admin(store: &mut MemoryStore, n: &str) -> Option<bool> {
        store.account_by_name(n).expect("reads").map(|a| a.admin)
    }

    /// A stranger who logged in with a name on an open world first.
    fn stranger(store: &mut MemoryStore, n: &str) {
        admits(AccountPolicy::Open, COST, store, n, b"strangers-ticket", 1).expect("admitted");
    }

    /// The list raises an account the same start makes from its ticket, its name in any case.
    /// Behaviour: server.admin.config-raises-only-what-this-start-makes
    #[test]
    fn the_configured_admins_raise_an_account_made_from_its_ticket_at_that_start() {
        let mut store = MemoryStore::default();
        seed_accounts(&mut store, &list(&["kara"]), &names(&["KARA"]), COST, 1);
        assert_eq!(is_admin(&mut store, "kara"), Some(true));
    }

    /// An account a stranger made first stays a player, whether the list holds a ticket for its name
    /// (which cannot make it) or not, its name in any case; so does one the list names ahead of it,
    /// at this start and the next; nothing is made for a name with no account.
    /// Behaviour: server.admin.config-raises-only-what-this-start-makes
    #[test]
    fn the_configured_admins_do_not_raise_a_strangers_account() {
        let mut store = MemoryStore::default();
        stranger(&mut store, "kara");
        seed_accounts(&mut store, &list(&["kara"]), &names(&["kara"]), COST, 1);
        assert_eq!(is_admin(&mut store, "kara"), Some(false), "ticket listed");
        seed_accounts(&mut store, &list(&["KARA"]), &names(&["KARA"]), COST, 1);
        assert_eq!(is_admin(&mut store, "kara"), Some(false), "in other case");

        seed_accounts(&mut store, &[], &names(&["fen"]), COST, 1);
        assert_eq!(is_admin(&mut store, "fen"), None, "nothing is made");
        stranger(&mut store, "fen");
        seed_accounts(&mut store, &[], &names(&["fen"]), COST, 1);
        assert_eq!(is_admin(&mut store, "fen"), Some(false), "the next start");
    }

    /// An account the list raised and the console then made a player again stays a player at the
    /// next start, with the list as it was, even with the ticket the list holds; an admin stays one.
    /// Behaviour: server.admin.a-restart-keeps-the-consoles-demotion
    #[test]
    fn a_restart_does_not_undo_the_consoles_demotion() {
        let mut store = MemoryStore::default();
        let (accounts, admins) = (list(&["kara", "ivo"]), names(&["kara", "ivo"]));
        seed_accounts(&mut store, &accounts, &admins, COST, 1);
        let kara = store.account_by_name("kara").expect("reads").expect("made");
        store.set_account_admin(kara.id, false).expect("set");
        seed_accounts(&mut store, &accounts, &admins, COST, 2);
        assert_eq!(is_admin(&mut store, "kara"), Some(false), "stays a player");
        assert_eq!(is_admin(&mut store, "ivo"), Some(true), "stays an admin");
    }
}

#[cfg(test)]
mod version_tests {
    //! Behaviour: none (tooling: what `--release-facts` prints for releases)
    use crate::update::Facts;

    #[test]
    fn release_facts_name_the_version_the_database_schema_and_the_world_database() {
        let v: serde_json::Value =
            serde_json::from_str(&Facts::this_build().to_json()).expect("one line of JSON");
        assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(v["databases"]["store"], yalain_store::SCHEMA_VERSION);
        assert_eq!(v["world_database"], yalain_world_db::release::PINNED_TAG);
        assert!(v["target"].as_str().is_some_and(|t| !t.is_empty()));
    }
}

#[cfg(test)]
mod version_text_tests {
    use super::version_text;
    use crate::update::SOURCE_URL;

    /// `--version` says what Empyrean's says, its build time aside: the program and its version,
    /// its commit, its target, and where its releases come from.
    /// Behaviour: server.operator.version-names-its-source
    #[test]
    fn version_names_the_program_its_version_its_commit_its_target_and_its_source() {
        let text = version_text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4, "{text}");
        assert_eq!(lines[0], concat!("yalain ", env!("CARGO_PKG_VERSION")));
        assert!(lines[1].starts_with("commit: "), "{text}");
        assert!(lines[2].starts_with("target: "), "{text}");
        assert_eq!(lines[3], format!("source: {SOURCE_URL}"));
    }
}
