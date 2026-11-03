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
//! random source (`getrandom`) for the directory's allegiance keys, and the players' friends
//! service (`yalain-friends`), and `ctrlc` for the system's stop signals, with `serde` and
//! `toml` for the operator's character recipes and the configuration file (and `serde_json` for character files). **Used
//! by** nothing: it is the `yalain` program.
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
//!   --secret-cost N       the bcrypt cost account secrets are kept at, 4 to 31 (default 8); a
//!                         secret kept at another cost is kept again at this one at its next login
//!   --save-interval S     keep each character in the world every S seconds, 1 or more (default
//!                         300); it is kept as it leaves, and at some events, besides
//!   --shutdown-interval S how many seconds a shutdown counts down, 0 to 99999 (default 120)
//!   --max-sessions N      how many logins the world holds at once (default 128); a login past
//!                         them is told the server is full
//!   --max-sessions-per-address N  how many logins one address holds at once (default
//!                         `unlimited`)
//!   --unlimited-sessions-from IP  an address no per-address limit holds back (may be given
//!                         many times)
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
//!                         fetched into the per-user cache with `yalain world fetch`; a world
//!                         that names a base database (`[base]`, as Athenaeum's do) is served
//!                         over that release in the cache
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
//!   --no-friends          serve no friends service: by default the world serves its players'
//!                         friends, presence and tells to Name@world on TCP at the login port's
//!                         number
//!   --seconds S           stop after S seconds (default: run until stopped)
//!   --verbose             log at the debug level (every connection event, and more)
//!   --stop-at-console-end stop, keeping everyone in the world, when the console's input ends:
//!                         for a program that runs the server and holds its console (by
//!                         default the end of the input ends only the console)
//!   --status IP:PORT      answer GET /health and GET /status (the world's state, as JSON) over
//!                         HTTP at this address (default: off); keep it on loopback or a private
//!                         network: it has no authentication
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
mod referral;
mod status;
mod stop;
mod verify;
mod world;

use yalain_content::{Catalog, ContentPack, LootTable, SpawnTable};
use yalain_net::udp::UdpPorts;
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
    /// The bcrypt cost of account secrets (`--secret-cost`).
    secret_cost: u32,
    /// Seconds between a character's saves (`--save-interval`).
    save_seconds: u32,
    /// Seconds a shutdown counts down (`--shutdown-interval`).
    shutdown_seconds: u32,
    /// Logins the world holds at once (`--max-sessions`).
    max_sessions: u32,
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
    world_base: Option<PathBuf>,
    referral: bool,
    echo: bool,
    game_time: Option<f64>,
    hold_clock: bool,
    /// Where and as what the world reports itself to a Dereth Network hub (`--hub`).
    hub: Option<hub::HubSettings>,
    /// Link to the hub's network (`--federation`).
    federation: bool,
    /// Serve the friends service (not `--no-friends`).
    friends: bool,
    seconds: Option<f64>,
    verbose: bool,
    /// Stop, keeping everyone, when the console's input ends (`--stop-at-console-end`).
    stop_at_console_end: bool,
}

/// The options the configuration file and the command line give (the command line after the
/// file, so it wins), and what to log about the file once logging is up.
fn parse() -> Result<(Options, Vec<String>), String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().unwrap_or_default();
    let exe = std::env::current_exe().ok();
    let exe_dir = exe.as_deref().and_then(Path::parent);
    let mut notes = Vec::new();
    let mut all = Vec::new();
    if let Some(file) = config::locate(&mut args, &cwd, exe_dir)? {
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
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
    Ok((parse_from(all)?, notes))
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
        secret_cost: yalain_world::login::SECRET_COST,
        save_seconds: yalain_world::saves::SAVE_SECONDS,
        shutdown_seconds: yalain_world::shutdown::LEAD,
        max_sessions: yalain_world::login::MAX_SESSIONS,
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
        world_base: None,
        referral: true,
        echo: true,
        game_time: None,
        hold_clock: false,
        hub: None,
        federation: false,
        friends: true,
        seconds: None,
        verbose: false,
        stop_at_console_end: false,
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
            "--shutdown-interval" => {
                let n: u32 = value()?
                    .parse()
                    .map_err(|e| format!("--shutdown-interval: {e}"))?;
                if n > yalain_world::shutdown::MAX_SECONDS {
                    return Err("--shutdown-interval takes 0 to 99999 seconds".into());
                }
                o.shutdown_seconds = n;
            }
            "--max-sessions" => {
                o.max_sessions = value()?
                    .parse()
                    .map_err(|e| format!("--max-sessions: {e}"))?;
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
            "--no-friends" => o.friends = false,
            "--seconds" => {
                o.seconds = Some(value()?.parse().map_err(|e| format!("--seconds: {e}"))?)
            }
            "--verbose" => o.verbose = true,
            "--stop-at-console-end" => o.stop_at_console_end = true,
            "--help" | "-h" => return Err(String::new()),
            other => return Err(format!("unknown option {other}")),
        }
    }
    if !o.login_port.is_multiple_of(2) || !o.world_port.is_multiple_of(2) {
        return Err("the ports are even: each service uses its port and the one above".into());
    }
    o.hub = match (hub_url, hub_slug, hub_key_file) {
        (None, None, None) => None,
        (Some(url), Some(slug), Some(key)) => Some(hub::HubSettings::new(&url, &slug, &key)?),
        (Some(_), ..) => return Err("--hub needs --hub-slug and --hub-key-file".into()),
        (None, ..) => return Err("--hub-slug and --hub-key-file go with --hub".into()),
    };
    if o.referral {
        referral::advertised(o.bind, o.advertise)?;
    }
    if o.federation {
        match &o.hub {
            None => return Err("--federation needs --hub, --hub-slug and --hub-key-file".into()),
            Some(h) => {
                network::settings(h)?;
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

/// Make the accounts and characters the command line asks for.
fn seed<S: Store>(o: &Options, store: &mut S, rules: &CharGenRules) {
    let policy = if o.listed {
        AccountPolicy::Listed
    } else {
        AccountPolicy::Open
    };
    for (n, t) in &o.add_accounts {
        if let Err(e) = add_account(store, n, t.as_bytes(), o.secret_cost, unix_now()) {
            tracing::warn!("account {n}: {e}");
        }
    }
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
    levels: yalain_world::levels::LevelTable,
    skills: yalain_world::skills::SkillTable,
    effects: yalain_world::effect_defs::EffectTable,
    item_effects: std::collections::HashMap<u32, Vec<yalain_world::profile::ItemEffect>>,
    mounts: yalain_world::mounts::Mounts,
    interactions: yalain_world::usable::interactions::Interactions,
    plant_looks: yalain_world::plants::Looks,
    item_uses: yalain_world::item_use::ItemUses,
    worn_effects: yalain_world::item_use::ItemUses,
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

fn run<S: Store>(
    o: &Options,
    names: NameRules,
    mut store: S,
    rules: CharGenRules,
    start: StartRules,
    content: ContentPack,
    play: Play,
) -> Result<(), Failure> {
    seed(o, &mut store, &rules);
    // Cookies, checksum seeds and the world's dice: the clock's nanoseconds and the process id.
    #[allow(clippy::cast_possible_truncation)]
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
        ^ u64::from(std::process::id());
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
        save_seconds: o.save_seconds,
        shutdown_seconds: o.shutdown_seconds,
        max_sessions: o.max_sessions,
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
    let mut ports = UdpPorts::bind(o.bind, o.login_port, o.world_port).map_err(|e| {
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
    let started = Instant::now();
    let local = || LocalTime(started.elapsed().as_secs_f64());
    #[allow(clippy::cast_precision_loss)]
    let game_time = o
        .game_time
        .unwrap_or_else(|| (unix_now() - GAME_EPOCH) as f64);
    let mut net = ServerNet::new(
        NetConfig::retail_ids(SocketAddrV4::new(advertise, o.world_port)),
        seed,
        game_time,
        local(),
    );
    if o.referral {
        net.refer_nearby(o.bind, yalain_net::udp::address_toward);
    }
    if o.hold_clock {
        net.hold_clock(local());
        tracing::info!("the game clock is held at {game_time} s");
    }
    let mut server = Server::new(cfg, store, rules)
        .with_world(start, content)
        .with_play(play.catalog, play.spawns, play.loot);
    server.ground = Some(Box::new(play.ground));
    server.walls = Some(play.walls);
    server.data = Some(Box::new(data::install_files(
        &o.dats.clone().unwrap_or_default(),
    )?));
    server.levels = play.levels;
    server.skill_table = play.skills;
    server.effect_table = play.effects;
    server.item_effects = play.item_effects;
    server.mounts = play.mounts;
    server.interactions = play.interactions;
    server.plants.looks = play.plant_looks;
    server.item_uses = play.item_uses;
    server.worn_effects = play.worn_effects;
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
        .map(|at| status::Endpoint::start(at, snapshot(&server, &net, started)))
        .transpose()?;
    if let Some(s) = &status {
        tracing::info!("status endpoint listening on http://{}/status", s.addr());
    }
    let mut network = match (&o.hub, o.federation) {
        (Some(h), true) => Some(network::Network::start(h, unix_now(), &mut server.store)?),
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
    loop {
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
                    reloading = Some(console::start(move || {
                        reload_world(&dats, &db, base.as_deref(), banned.as_deref())
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
                    &Err("hero <name> makes a character a hero, hero <name> off no longer".into()),
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
        // The status ping is answered on the login port before the transport reads anything.
        let mut pinged = Vec::new();
        let seconds = started.elapsed().as_secs();
        let got = ports.receive(|port, from, bytes| {
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
        ports.send(&pinged);
        let clock = Clock {
            local: now,
            unix: unix_now(),
        };
        for c in pending.drain(..) {
            world_command(&mut server, &mut net, clock, c);
        }
        server.pump(&mut net, clock);
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
            s.set(snapshot(&server, &net, started));
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
        let talked = friends.pump(
            Duration::from_secs_f64(now.0),
            &mut server,
            &mut net,
            network.as_mut().map(|n| &mut n.federation),
        );
        net.tick(now);
        let out = net.take_datagrams();
        ports.send(&out);
        if server.shutdown == yalain_world::shutdown::Shutdown::Done {
            break;
        }
        if got == 0 && out.is_empty() && !talked {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let kept = yalain_world::shutdown::save_everyone(&mut server, &mut net);
    tracing::info!("stopping: {kept} characters in the world kept");
    ports.send(&net.take_datagrams());
    friends.stop();
    if let Some(n) = &mut network {
        n.stop();
    }
    if let Some(r) = reporter {
        r.stop();
    }
    for (what, n) in server.unhandled.counts() {
        tracing::info!("not handled yet: {what} x{n}");
    }
    Ok(())
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
fn snapshot<S: Store>(server: &Server<S>, net: &ServerNet, started: Instant) -> status::Snapshot {
    status::Snapshot {
        world_name: server.cfg.world_name.clone(),
        uptime_seconds: started.elapsed().as_secs(),
        shutting_down: server.shutdown != yalain_world::shutdown::Shutdown::No,
        players: server.players.len(),
        logins: server.sessions.len(),
        max_logins: server.cfg.max_sessions,
        connections: net.connection_count(),
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
        console::Command::SaveWorld => {
            let n = yalain_world::saves::save_world(server, clock);
            Ok(format!("saved: {n} characters in the world kept"))
        }
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
    dats: &Path,
    world_db: &str,
    base: Option<&Path>,
) -> Result<(PathBuf, Tables), String> {
    world::database_dir(world_db).and_then(|d| {
        let t = world::compile_dir(dats, &d, base)?;
        Ok((d, t))
    })
}

/// The world's objects and what it plays, from its database compiled and the install.
fn load_play(dats: &Path, compiled: Tables) -> Result<(ContentPack, Play), String> {
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
        ..
    } = compiled;
    let play = data::Portal::open(dats).and_then(|portal| {
        let skills = data::skill_table(&portal)?;
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
        let effects = data::effect_table(&portal, &states)?;
        let granted: Vec<u32> = effects
            .effects
            .values()
            .filter_map(|d| match d.kind {
                yalain_world::effect_defs::Kind::GrantRecipe(r)
                | yalain_world::effect_defs::Kind::RemoveRecipe(r) => Some(r),
                _ => None,
            })
            .collect();
        let mut craft = data::crafting(&portal, &crafting, &granted)?;
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
        let effects = data::effect_table(&portal, &states)?;
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
            .chain(actions.iter().filter_map(|a| match a {
                yalain_world::craft::Action::Produce { template, .. } => Some(*template),
                _ => None,
            }))
            // The placed objects' own (a chest's lock and contents, a door's).
            .chain(content.objects.iter().map(|o| o.template))
            .collect();
        let (mut catalog, mut skipped) = data::catalog(&portal, &spawns, &loot, &more)?;
        // The weapons the creatures wield, described with the rest.
        let weapons = data::creature_weapons(&portal, &catalog, &listed_weapons)?;
        let wanted: Vec<u32> = weapons.values().copied().collect();
        let (held, also) = data::catalog(
            &portal,
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
        tracing::info!(weapons = weapons.len(), "the creatures' weapons read");
        catalog.bodies = data::bodies(&portal, &catalog);
        for s in &skipped {
            tracing::warn!("not described: {s}");
        }
        tracing::info!(
            camps = spawns.camps.len(),
            profiles = loot.profiles.len(),
            templates = catalog.templates.len(),
            "spawns, loot and templates read"
        );
        let brokers = data::brokers(&portal, &content);
        tracing::info!(brokers = brokers.len(), "the market's brokers read");
        let (ground, walls) = data::land_ground(dats, &spawns, &catalog)?;
        tracing::info!("the ground under the camps, and the interior cells: {ground:?}; {walls:?}");
        let levels = data::level_table(&portal)?;
        tracing::info!(levels = levels.levels.len(), "the level table read");
        let vitae_costs = data::vitae_costs(&portal)?;
        let item_effects = data::item_effects(&portal, &catalog)?;
        // The mounts, their effects read with the rest.
        let bodies: Vec<u32> = data::chargen_rules(&portal)?
            .bodies
            .values()
            .copied()
            .collect();
        let mut mounts = data::mounts(&portal, &bodies)?;
        states.extend(mounts.effects.keys().copied());
        tracing::info!(
            effects = mounts.effects.len(),
            looks = mounts.looks.len(),
            "the mounts read"
        );
        // The effects the items' uses put on, read with the rest.
        let (item_uses, worn_effects) = data::item_uses(&portal, &catalog)?;
        states.extend(item_uses.values().flatten().map(|e| e.state));
        states.extend(yalain_world::item_use::FOOD_EFFECTS);
        states.extend(worn_effects.values().flatten().map(|e| e.state));
        // The treasure potions' effects, put on when they are drunk.
        states.extend(yalain_world::treasure::POTION_EFFECTS.iter().flatten());
        mounts.note_items(&item_uses);
        let effects = data::effect_table(&portal, &states)?;
        tracing::info!(
            templates = item_uses.len(),
            effects = effects.effects.len(),
            "the effects items' uses put on read"
        );
        tracing::info!(
            templates = worn_effects.len(),
            "the effects items put on their wearers read"
        );
        let walk_in = data::walk_in_packages(&portal, &content)?;
        let persons =
            data::packages_deriving_from(&portal, &content, &yalain_world::places::PERSONS)?;
        let shrines = data::packages_deriving_from(
            &portal,
            &content,
            &yalain_world::usable::shrines::FACTION_SHRINES,
        )?;
        // The kingdom each shrine's use joins, by its kind.
        let mut shrine_kingdoms = std::collections::HashMap::new();
        for (kind, kingdom) in yalain_world::kingdoms::SHRINES {
            for package in data::packages_deriving_from(&portal, &content, &[kind])? {
                shrine_kingdoms.insert(package, kingdom);
            }
        }
        let cannot_use =
            data::packages_deriving_from(&portal, &content, &yalain_world::places::CANNOT_BE_USED)?;
        let (doors, doors_through_locks) = data::door_packages(&portal, &content)?;
        let chests = data::chest_packages(&portal, &content)?;
        tracing::info!(
            doors = doors.len(),
            chests = chests.len(),
            "the doors' and chests' kinds read"
        );
        let destroyed_on_use = data::destroyed_on_use(&portal, &catalog)?;
        let treasure = data::treasure(&portal, &catalog)?;
        let tesserae =
            data::catalog_packages_from(&portal, &catalog, &[yalain_world::treasure::TESSERAE])?;
        tracing::info!(
            equipment = treasure.equipment.len(),
            potions = treasure.potions.len(),
            looks = treasure.potion_looks.len(),
            tesserae = tesserae.len(),
            "the treasure items' kinds read"
        );
        let interactions = data::interactions(&portal, &catalog)?;
        tracing::info!(
            tables = interactions.tables.len(),
            "the items' interaction tables read"
        );
        let plant_looks = data::plant_looks(&portal, &catalog);
        tracing::info!(looks = plant_looks.len(), "the dye plants' ripe looks read");
        let inscribable =
            data::catalog_packages_from(&portal, &catalog, &yalain_world::personal::INSCRIBABLE)?;
        let mut held_answers = std::collections::HashMap::new();
        for (base, status, flags) in yalain_world::places::HELD_ANSWERED {
            for pkg in data::catalog_packages_from(&portal, &catalog, &[base])? {
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
        })
    });
    play.map(|p| (content, p))
}
/// The world database compiled again for a running server: its folder and its tables, with what
/// the install adds to them.
fn reload_world(
    dats: &Path,
    world_db: &str,
    base: Option<&Path>,
    banned_names: Option<&Path>,
) -> Result<(PathBuf, WorldTables), String> {
    let names = banned_names.map(read_banned_names).transpose()?;
    let (dir, compiled) = compile_world(dats, world_db, base)?;
    for w in &compiled.warnings {
        tracing::warn!("{w}");
    }
    let (content, p) = load_play(dats, compiled)?;
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
        println!("{}", release_facts());
        return std::process::ExitCode::SUCCESS;
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
            eprintln!("usage: yalain --dats <install folder> [--bind IP] [--login-port P] [--world-port P] [--advertise IP] [--data FILE | --memory] [--world-name NAME] [--listed-accounts] [--banned-names FILE] [--add-account NAME:TICKET] [--secret-cost N] [--save-interval S] [--shutdown-interval S] [--max-sessions N] [--max-sessions-per-address N] [--unlimited-sessions-from IP] [--motd TEXT] [--status IP:PORT] [--no-status-ping] [--status-ping-hellos N] [--status-ping-asks N] [--status-ping-replies N] [--config FILE] [--add-character ACCOUNT:NAME:RACE:SEX] [--place ACCOUNT:NAME:CELL:X:Y:Z:HEADING] [--money ACCOUNT:NAME:AMOUNT] [--make-character FILE] [--world-db DIR|pinned] [--world-base DIR] [--no-referral] [--no-echo] [--game-time S] [--hold-clock] [--hub URL --hub-slug SLUG --hub-key-file FILE [--federation]] [--no-friends] [--seconds S] [--verbose] [--stop-at-console-end]");
            eprintln!("       yalain world check|release|fetch|compile ...");
            eprintln!("       yalain --version | --release-facts");
            return std::process::ExitCode::FAILURE;
        }
    };
    tracing_subscriber::fmt()
        .with_max_level(if o.verbose {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .with_ansi(false)
        .init();
    for n in &notes {
        if n.starts_with("configuration read") {
            tracing::info!("{n}");
        } else {
            tracing::warn!("{n}");
        }
    }
    let Some(dats) = o.dats.clone() else {
        eprintln!("yalain: --dats names the install to read the data from");
        return std::process::ExitCode::FAILURE;
    };

    let loaded = data::Portal::open(&dats)
        .and_then(|p| Ok((data::chargen_rules(&p)?, data::start_rules(&p)?)));
    let (rules, start) = match loaded {
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
    let compiled = match compile_world(&dats, &o.world_db, o.world_base.as_deref()) {
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
    let play = load_play(&dats, compiled);
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
        Ok(()) => std::process::ExitCode::SUCCESS,
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
        "yalain {}\ncommit: {}\ntarget: {}\n",
        env!("CARGO_PKG_VERSION"),
        option_env!("YALAIN_BUILD_COMMIT").unwrap_or("unknown"),
        option_env!("YALAIN_BUILD_TARGET").unwrap_or("unknown"),
    )
}

/// What `yalain --release-facts` prints, one line of JSON, as Empyrean's: this build's version and
/// target, its database's schema version and the world database release it serves by default,
/// which a launcher compares with a release's upgrade declaration.
fn release_facts() -> String {
    let target = option_env!("YALAIN_BUILD_TARGET").map_or_else(
        || match (std::env::consts::ARCH, std::env::consts::OS) {
            ("x86_64", "windows") => "x86_64-pc-windows-msvc".to_owned(),
            ("x86_64", "linux") => "x86_64-unknown-linux-gnu".to_owned(),
            ("aarch64", "linux") => "aarch64-unknown-linux-gnu".to_owned(),
            ("aarch64", "macos") => "aarch64-apple-darwin".to_owned(),
            (arch, os) => format!("{arch}-{os}"),
        },
        str::to_owned,
    );
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "target": target,
        "databases": { "store": yalain_store::SCHEMA_VERSION },
        "world_database": yalain_world_db::release::PINNED_TAG,
    })
    .to_string()
}

#[cfg(test)]
mod version_tests {
    //! Behaviour: none (tooling: what `--version` and `--release-facts` print for releases)
    use super::{release_facts, version_text};

    #[test]
    fn release_facts_name_the_version_the_database_schema_and_the_world_database() {
        let v: serde_json::Value =
            serde_json::from_str(&release_facts()).expect("one line of JSON");
        assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(v["databases"]["store"], yalain_store::SCHEMA_VERSION);
        assert_eq!(v["world_database"], yalain_world_db::release::PINNED_TAG);
        assert!(v["target"].as_str().is_some_and(|t| !t.is_empty()));
    }

    #[test]
    fn version_names_the_program_its_version_its_commit_and_its_target() {
        let text = version_text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text}");
        assert_eq!(lines[0], concat!("yalain ", env!("CARGO_PKG_VERSION")));
        assert!(lines[1].starts_with("commit: "), "{text}");
        assert!(lines[2].starts_with("target: "), "{text}");
    }
}
