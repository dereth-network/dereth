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
//! service (`yalain-friends`), and `ctrlc` for the system's stop signals. **Used by** nothing: it
//! is the `yalain` program.
//!
//! ```text
//! yalain --dats <install folder> [options]
//!
//!   --dats DIR            the September 2013 install the server reads its data from (a copy no
//!                         client is running from: a client opens its data files alone)
//!   --bind IP             the address to listen on (default 127.0.0.1)
//!   --login-port P        the login service's even port (default 9000; P + 1 is used too)
//!   --world-port P        the world service's even port (default 9002; P + 1 is used too)
//!   --advertise IP        the IPv4 address a referral sends clients to (default: --bind; needed
//!                         when --bind is 0.0.0.0 or an IPv6 address)
//!   --data FILE           the database of accounts and characters (default yalain.sqlite)
//!   --memory              keep nothing: a database in memory
//!   --world-name NAME     the world's name, shown at login (default "Local World")
//!   --listed-accounts     admit only accounts made with --add-account (default: any new name
//!                         makes its account, with its ticket)
//!   --banned-names FILE   refuse character names holding these words: one pattern a line, `*`
//!                         for any run of letters (default: none)
//!   --add-account N:T     make account N with ticket T, then carry on
//!   --add-character A:N:R:S  make character N on account A of race R and sex S (4096 or 8192),
//!                         with a middling physique, then carry on; a missing account is made on
//!                         an open world only
//!   --place A:N:CELL:X:Y:Z:H  put character N of account A at a place (cell in hex, a heading
//!                         in degrees) before it next enters, then carry on
//!   --money A:N:AMOUNT    set the money of character N of account A (one that has played) before
//!                         it next enters, then carry on
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
//! restart; `shutdown` tells every player in the world the world is shutting down, counts down two
//! minutes and stops.
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

use dereth_primitives::num::math;
use std::net::{IpAddr, Ipv4Addr, SocketAddrV4};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod console;
mod data;
mod friends;
mod help;
mod hub;
mod network;
mod referral;
mod stop;
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
    add_characters: Vec<(String, String, u32, u32)>,
    places: Vec<(String, String, StoredPosition)>,
    money: Vec<(String, String, i32)>,
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
}

fn parse() -> Result<Options, String> {
    parse_from(std::env::args().skip(1))
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
        add_characters: Vec::new(),
        places: Vec::new(),
        money: Vec::new(),
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
                let half = -f(h)?.to_radians() / 2.0;
                let at = StoredPosition {
                    cell,
                    origin: [f(x)?, f(y)?, f(z)?],
                    rotation: [math::cosf(half), 0.0, 0.0, math::sinf(half)],
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
        if let Err(e) = add_account(store, n, t.as_bytes(), unix_now()) {
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
    brokers: std::collections::BTreeMap<u32, yalain_world::market::Broker>,
    walk_in: std::collections::HashSet<u32>,
    persons: std::collections::HashSet<u32>,
    shrines: std::collections::HashSet<u32>,
    doors: std::collections::HashMap<u32, yalain_world::usable::doors::Kind>,
    doors_through_locks: std::collections::HashSet<u32>,
    chests: std::collections::HashSet<u32>,
    destroyed_on_use: std::collections::HashSet<u32>,
    inscribable: std::collections::HashSet<u32>,
    held_answers: std::collections::HashMap<u32, (u32, u32)>,
    weapons: std::collections::HashMap<String, u32>,
    weapon_rules: std::collections::HashMap<String, yalain_content::WeaponRule>,
}

fn run<S: Store>(
    o: &Options,
    names: NameRules,
    mut store: S,
    rules: CharGenRules,
    start: StartRules,
    content: ContentPack,
    play: Play,
) -> Result<(), String> {
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
        ..ServerConfig::default()
    };
    let advertise = if o.referral {
        referral::advertised(o.bind, o.advertise)?
    } else {
        o.advertise.unwrap_or(Ipv4Addr::LOCALHOST)
    };
    let mut ports = UdpPorts::bind(o.bind, o.login_port, o.world_port)
        .map_err(|e| format!("binding the ports: {e}"))?;
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
    server.brokers = play.brokers;
    server.walk_in_packages = play.walk_in;
    server.person_packages = play.persons;
    server.shrine_packages = play.shrines;
    server.doors.packages = play.doors;
    server.doors.creatures_through_locks = play.doors_through_locks;
    server.chests.packages = play.chests;
    server.destroyed_on_use = play.destroyed_on_use;
    server.inscribable_packages = play.inscribable;
    server.held_answers = play.held_answers;
    server.creature_weapons = play.weapons;
    server.creature_weapon_rules = play.weapon_rules;
    tracing::info!(
        "listening on {}:{}/{} (login) and {}:{}/{} (world, referred as {advertise})",
        o.bind,
        o.login_port,
        o.login_port + 1,
        o.bind,
        o.world_port,
        o.world_port + 1
    );
    let mut reporter = o.hub.clone().map(hub::Reporter::start);
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
    let mut shutting = false;
    let interrupt = stop::Interrupt::install();
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
        while let Ok(line) = typed.try_recv() {
            match console::parse(&line) {
                None => {}
                Some(console::Command::Help) => tracing::info!("{}", console::HELP),
                Some(console::Command::Shutdown) => shutting = true,
                Some(console::Command::Reload) if reloading.is_some() => {
                    tracing::info!("a reload is already running");
                }
                Some(console::Command::Reload) => {
                    tracing::info!("reloading the world database");
                    let dats = o.dats.clone().unwrap_or_default();
                    let (db, base) = (o.world_db.clone(), o.world_base.clone());
                    let banned = o.banned_names.clone();
                    reloading = Some(console::start(move || {
                        reload_world(&dats, &db, base.as_deref(), banned.as_deref())
                    }));
                }
                Some(console::Command::Network(words)) => {
                    let words: Vec<&str> = words.iter().map(String::as_str).collect();
                    match &mut network {
                        Some(n) => {
                            for line in n.console(&mut server.store, &mut friends.service, &words) {
                                tracing::info!("{line}");
                            }
                        }
                        None => tracing::info!("This world is not on the Dereth Network."),
                    }
                }
                Some(console::Command::Hero(None)) => {
                    tracing::info!(
                        "hero <name> makes a character a hero, hero <name> off no longer"
                    );
                }
                Some(console::Command::Hero(Some((name, on)))) => {
                    match yalain_world::focus::set_hero(&mut server, &mut net, &name, on) {
                        Ok(done) => tracing::info!("{done}"),
                        Err(e) => tracing::warn!("hero: {e}"),
                    }
                }
                Some(console::Command::Unknown(c)) => {
                    tracing::warn!("{c}: not a command ({})", console::HELP);
                }
            }
        }
        if let Some(done) = reloading.as_ref().and_then(console::Job::poll) {
            reloading = None;
            match done {
                Ok((dir, tables)) => {
                    let r = server.reload(tables);
                    tracing::info!("world database {}: {}", dir.display(), r.summary());
                }
                Err(e) => tracing::warn!(
                    "the world database did not compile; the world is unchanged: {e}"
                ),
            }
        }
        let now = local();
        let got = ports.receive(|port, from, bytes| net.on_datagram(port, from, bytes, now));
        let clock = Clock {
            local: now,
            unix: unix_now(),
        };
        if std::mem::take(&mut shutting) {
            yalain_world::shutdown::begin(&mut server, &mut net, clock);
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
        if let Some(n) = &mut network {
            n.directory(&mut server, now.0);
            n.pump(&mut friends.service, now.0);
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
        let (doors, doors_through_locks) = data::door_packages(&portal, &content)?;
        let chests = data::chest_packages(&portal, &content)?;
        tracing::info!(
            doors = doors.len(),
            chests = chests.len(),
            "the doors' and chests' kinds read"
        );
        let destroyed_on_use = data::destroyed_on_use(&portal, &catalog)?;
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
            brokers,
            walk_in,
            persons,
            shrines,
            doors,
            doors_through_locks,
            chests,
            destroyed_on_use,
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
        effects: p.effects,
        crafting: p.craft,
        weapons: p.weapons,
        weapon_rules: p.weapon_rules,
        names,
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
    if args.first().map(String::as_str) == Some("world") {
        return match world::main(&args[1..]) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("yalain world: {e}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    let o = match parse() {
        Ok(o) => o,
        Err(e) if e.is_empty() => {
            print!("{}", help::TEXT);
            return std::process::ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("yalain: {e}");
            eprintln!("usage: yalain --dats <install folder> [--bind IP] [--login-port P] [--world-port P] [--advertise IP] [--data FILE | --memory] [--world-name NAME] [--listed-accounts] [--banned-names FILE] [--add-account NAME:TICKET] [--add-character ACCOUNT:NAME:RACE:SEX] [--place ACCOUNT:NAME:CELL:X:Y:Z:HEADING] [--money ACCOUNT:NAME:AMOUNT] [--world-db DIR|pinned] [--world-base DIR] [--no-referral] [--no-echo] [--game-time S] [--hold-clock] [--hub URL --hub-slug SLUG --hub-key-file FILE [--federation]] [--no-friends] [--seconds S] [--verbose]");
            eprintln!("       yalain world check|release|fetch|compile ...");
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
        match SqliteStore::open(&o.data) {
            Ok(store) => run(&o, names, store, rules, start, content, play),
            Err(e) => Err(format!("{}: {e}", o.data.display())),
        }
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("yalain: {e}");
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

#[cfg(test)]
mod version_tests {
    //! Behaviour: none (tooling: what `--version` prints, which the release workflow checks)
    use super::version_text;

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
